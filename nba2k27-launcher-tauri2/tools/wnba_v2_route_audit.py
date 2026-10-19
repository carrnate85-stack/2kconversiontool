"""Offline-only Version 2 WNBA Blacktop/Freestyle route audit.

Opens NBA2K27.exe as an ordinary read-only file. It never opens a process,
writes game files or executes game code. Every check is pinned to the exact
Version 2 image (PE 6AC53765 / SizeOfImage 35BDE000) and verifies fixed
instruction bytes, so it runs in seconds without a whole-image scan.

What it proves (see artifacts/wnba-v2-research-20261017.md):

1. WPF's old-build Blacktop write (BLACKTOP_DELEGATE vtable +0xD10: stock
   `ret` -> general Learn2K thunk) maps to V2 vtable +0xD30. V2 +0xD10 is an
   unrelated `xor eax,eax; ret` getter; V2 +0xD30 is the stock `ret`.
2. GAMEMODE_MANAGER::InstallDelegate (0x222AAC0) calls old+0xD38 then
   new+0xD30, the same call shape for every play-now delegate.
3. PlayNow_InitGameMode (0x236EB30) maps play-now type -> delegate object;
   Freestyle launches pass type 7 + isWnba (TKU or TKU_WNBA).
4. TKU and TKU_WNBA vtables differ only in identity slots and +0xD30.
5. The V2 Blacktop category switch keeps WPF's exact layout (table, case 3,
   continuation at fn+0x5B).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import re
import struct
import sys

EXPECTED_SHA256 = "29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1"
EXPECTED_STAMP = 0x6AC53765
EXPECTED_IMAGE_SIZE = 0x35BDE000
DEFAULT_EXE = r"C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27\NBA2K27.exe"

DELEGATES = [
    "BLACKTOP_DELEGATE",
    "NBATODAY_DELEGATE",
    "NCAA_QUICKGAME_DELEGATE",
    "PNONLINE_DELEGATE",
    "QUICKGAME_DELEGATE",
    "QUICKGAMEONLINE_DELEGATE",
    "TKU_DELEGATE",
    "TKU_WNBA_DELEGATE",
]
# Pinned V2 facts this audit re-derives and checks.
V2 = {
    "blacktopVtable": 0x74CD158,
    "tkuVtable": 0x74DB678,
    "tkuWnbaVtable": 0x74DDE38,
    "stockRet": 0x236E510,
    "generalLoader": 0x236FB30,
    "wnbaLoader": 0x236FF70,
    "restoreLoader": 0x236FB50,
    "blacktopD38": 0x2350BF0,
    "oldSlotGetter": 0x23531A0,
    "installDelegate": 0x222AAC0,
    "installOldD38Call": 0x222AC30,
    "installNewD30Call": 0x222AC40,
    "playNowInit": 0x236EB30,
    "playNowJumpTable": 0x7426E90,
    "currentPlayNowDelegate": 0xAB3CE38,
    "isWnbaAccessor": 0x249910,
    "setWnbaLeague": 0x249900,
    "resetNbaLeague": 0x24A320,
    "leagueByte": 0xA313DD9,
    "freestyleLeague": 0xB1EA778,
    "freestyleLaunch": 0x32F3220,
    "freestyleLaunchCall": 0x32F32EC,
    "blacktopLaunch": 0x32FF5A0,
    "blacktopLaunchCall": 0x32FF613,
    "categorySwitch": 0x230BDD0,
    "categoryTable": 0x7425F4C,
    "categoryContinue": 0x230BE2B,
    "categoryUnusedCase": 0x230BEA4,
    "labelRecord": 0xACFC4D0,
}
# WPF NBA2K27FreestyleTeams.exe constants for the last WPF-supported build
# (PE 6AA2013D / 36558000), from unlocks-helper/BlacktopTeams.cs + BlacktopCategory.cs.
WPF_OLD_BUILD = {
    "blacktopDelegateVtable": 0x7461028,
    "callbackSlot": 0x7461D38,
    "callbackSlotOffset": 0xD10,
    "stockRet": 0x2342E00,
    "loader": 0x2344400,
    "categoryTable": 0x73BA190,
    "categorySwitch": 0x22E09F0,
    "categoryContinue": 0x22E0A4B,
    "categoryTargets": [0x22E0A38, 0x22E0A46, 0x22E0A3F, 0x22E0AC4, 0x22E0A9A, 0x22E0AA3],
    "label": 0xAC6C6B0,
}


class Image:
    """Minimal PE reader over raw bytes (no third-party dependency)."""

    def __init__(self, data: bytes):
        self.data = data
        pe = struct.unpack_from("<I", data, 0x3C)[0]
        if data[:2] != b"MZ" or data[pe : pe + 4] != b"PE\0\0":
            raise ValueError("not a PE image")
        self.stamp = struct.unpack_from("<I", data, pe + 8)[0]
        count = struct.unpack_from("<H", data, pe + 6)[0]
        opt = struct.unpack_from("<H", data, pe + 20)[0]
        self.image_base = struct.unpack_from("<Q", data, pe + 24 + 24)[0]
        self.size_of_image = struct.unpack_from("<I", data, pe + 24 + 56)[0]
        self.sections = []
        start = pe + 24 + opt
        for i in range(count):
            at = start + i * 40
            name = data[at : at + 8].rstrip(b"\0").decode("ascii", "replace")
            vsize, va, rsize, raw = struct.unpack_from("<IIII", data, at + 8)
            chars = struct.unpack_from("<I", data, at + 36)[0]
            self.sections.append((name, va, vsize, raw, rsize, chars))

    def offset(self, rva: int) -> int:
        for _, va, vsize, raw, rsize, _ in self.sections:
            if va <= rva < va + max(vsize, rsize) and rva - va < rsize:
                return raw + rva - va
        raise ValueError(f"RVA 0x{rva:X} is not file-backed")

    def rva_for_offset(self, off: int) -> int | None:
        for _, va, _, raw, rsize, _ in self.sections:
            if raw <= off < raw + rsize:
                return va + off - raw
        return None

    def read(self, rva: int, n: int) -> bytes:
        at = self.offset(rva)
        return self.data[at : at + n]

    def u32(self, rva: int) -> int:
        return struct.unpack("<I", self.read(rva, 4))[0]

    def i32(self, rva: int) -> int:
        return struct.unpack("<i", self.read(rva, 4))[0]

    def u64(self, rva: int) -> int:
        return struct.unpack("<Q", self.read(rva, 8))[0]

    def ptr_rva(self, rva: int) -> int:
        return self.u64(rva) - self.image_base

    def cstr(self, rva: int, limit: int = 160) -> str:
        raw = self.read(rva, limit)
        return raw.split(b"\0", 1)[0].decode("ascii", "replace")


def rel32_target(img: Image, rva: int, opcode_len: int, total_len: int) -> int:
    """Target of a RIP-relative/rel32 operand whose disp starts after opcode_len."""
    return rva + total_len + img.i32(rva + opcode_len)


def find_vtable(img: Image, class_name: str) -> int:
    """Resolve the single MSVC x64 vtable for `.?AV<class>@@` via its COL."""
    pattern = b".?AV" + class_name.encode() + b"@@\0"
    hits = []
    for m in re.finditer(re.escape(pattern), img.data):
        td = img.rva_for_offset(m.start() - 16)
        if td is None:
            continue
        for c in re.finditer(re.escape(struct.pack("<I", td)), img.data):
            col_off = c.start() - 12
            if col_off < 0 or struct.unpack_from("<II", img.data, col_off) != (1, 0):
                continue
            col = img.rva_for_offset(col_off)
            ref = struct.pack("<Q", img.image_base + col)
            for v in re.finditer(re.escape(ref), img.data):
                vt = img.rva_for_offset(v.start() + 8)
                if vt is not None:
                    hits.append(vt)
    hits = sorted(set(hits))
    if len(hits) != 1:
        raise ValueError(f"{class_name}: expected one vtable, found {[hex(h) for h in hits]}")
    return hits[0]


def class_of_object(img: Image, obj_rva: int) -> str | None:
    """Static delegate objects carry their vtable in initialised .data."""
    vt = img.ptr_rva(obj_rva)
    col = img.ptr_rva(vt - 8)
    if img.u32(col) != 1:
        return None
    return img.cstr(img.u32(col + 12) + 16)


def slot(img: Image, vtable: int, offset: int) -> int:
    return img.ptr_rva(vtable + offset)


def vtable_diff(img: Image, a: int, b: int, slots: int = 1000) -> list[int]:
    return [i * 8 for i in range(slots) if img.u64(a + i * 8) != img.u64(b + i * 8)]


def decode_jump_table(img: Image, table: int, count: int) -> list[int]:
    return [table + img.i32(table + 4 * i) for i in range(count)]


def lea_rax_rip(img: Image, rva: int) -> int:
    """Decode `48 8D 05 disp32` (lea rax,[rip+disp])."""
    if img.read(rva, 3) != b"\x48\x8D\x05":
        raise ValueError(f"0x{rva:X} is not lea rax,[rip+disp]")
    return rel32_target(img, rva, 3, 7)


def check(cond: bool, message: str, failures: list[str]) -> bool:
    if not cond:
        failures.append(message)
    return cond


def audit(img: Image) -> dict:
    failures: list[str] = []
    out: dict = {}
    check(img.stamp == EXPECTED_STAMP and img.size_of_image == EXPECTED_IMAGE_SIZE,
          "executable is not the pinned Version 2 build", failures)

    vt = {name: find_vtable(img, name) for name in DELEGATES}
    out["delegateVtables"] = {k: f"0x{v:X}" for k, v in vt.items()}
    check(vt["BLACKTOP_DELEGATE"] == V2["blacktopVtable"], "Blacktop vtable moved", failures)
    check(vt["TKU_DELEGATE"] == V2["tkuVtable"], "TKU vtable moved", failures)
    check(vt["TKU_WNBA_DELEGATE"] == V2["tkuWnbaVtable"], "TKU_WNBA vtable moved", failures)

    # 1. Roster-hook slots.
    rows = {}
    for name in ("BLACKTOP_DELEGATE", "QUICKGAME_DELEGATE", "TKU_DELEGATE", "TKU_WNBA_DELEGATE"):
        rows[name] = {f"+0x{o:X}": f"0x{slot(img, vt[name], o):X}" for o in (0xD10, 0xD30, 0xD38)}
    out["rosterHookSlots"] = rows
    bt = vt["BLACKTOP_DELEGATE"]
    check(slot(img, bt, 0xD30) == V2["stockRet"] and img.read(V2["stockRet"], 1) == b"\xC3",
          "Blacktop +0xD30 is not the stock ret stub", failures)
    check(slot(img, bt, 0xD10) == V2["oldSlotGetter"]
          and img.read(V2["oldSlotGetter"], 3) == b"\x31\xC0\xC3",
          "Blacktop +0xD10 is not the xor-eax getter", failures)
    check(slot(img, bt, 0xD38) == V2["blacktopD38"] and img.read(V2["blacktopD38"], 1) == b"\xC3",
          "Blacktop +0xD38 is not a stock ret stub", failures)
    check(slot(img, vt["TKU_DELEGATE"], 0xD30) == V2["generalLoader"], "TKU +0xD30 moved", failures)
    check(slot(img, vt["TKU_WNBA_DELEGATE"], 0xD30) == V2["wnbaLoader"], "TKU_WNBA +0xD30 moved", failures)
    for name in ("TKU_DELEGATE", "TKU_WNBA_DELEGATE"):
        check(slot(img, vt[name], 0xD38) == V2["restoreLoader"], f"{name} +0xD38 moved", failures)

    def thunk(rva: int) -> dict:
        raw = img.read(rva, 26)
        ok = raw[0:3] == b"\x48\x8D\x0D" and raw[7:9] == b"\xB2\x01" and raw[9:11] == b"\x41\xB8" \
            and raw[15:17] == b"\x41\xB9" and raw[21] == 0xE9
        return {
            "rva": f"0x{rva:X}",
            "shapeOk": ok,
            "asset": img.cstr(rel32_target(img, rva, 3, 7)) if ok else None,
            "hash": f"0x{struct.unpack_from('<I', raw, 11)[0]:08X}" if ok else None,
            "id": struct.unpack_from("<I", raw, 17)[0] if ok else None,
            "jumpsTo": f"0x{rel32_target(img, rva + 21, 1, 5):X}" if ok else None,
            "bytes": raw.hex(),
        }

    out["loaderThunks"] = {
        "general": thunk(V2["generalLoader"]),
        "wnba": thunk(V2["wnbaLoader"]),
    }
    restore = img.read(V2["restoreLoader"], 26)
    out["restoreThunk"] = {"asset": img.cstr(rel32_target(img, V2["restoreLoader"], 3, 7)),
                           "bytes": restore[:21].hex()}
    check(out["loaderThunks"]["general"]["asset"] == "roster/learn2k_roster.iff", "general thunk asset", failures)
    check(out["loaderThunks"]["wnba"]["asset"] == "roster/learn2k_wnba_roster.iff", "WNBA thunk asset", failures)
    check(out["restoreThunk"]["asset"] == "roster/roster.iff", "restore thunk asset", failures)
    check(out["loaderThunks"]["general"]["jumpsTo"] == out["loaderThunks"]["wnba"]["jumpsTo"] == "0x2F1E0B0",
          "thunks do not share RosterData_Reset", failures)

    # 2. InstallDelegate call shape.
    check(img.read(V2["installOldD38Call"], 6) == b"\xFF\x90\x38\x0D\x00\x00", "old+0xD38 call moved", failures)
    check(img.read(V2["installNewD30Call"], 6) == b"\xFF\x90\x30\x0D\x00\x00", "new+0xD30 call moved", failures)
    out["installDelegate"] = {
        "function": f"0x{V2['installDelegate']:X}",
        "oldDelegateD38Call": f"0x{V2['installOldD38Call']:X}",
        "newDelegateD30Call": f"0x{V2['installNewD30Call']:X}",
        "note": "Both +0xD30 calls (TKU loader, Blacktop ret) use rcx=delegate only.",
    }

    # 3. PlayNow_InitGameMode type -> delegate object -> class.
    targets = decode_jump_table(img, V2["playNowJumpTable"], 12)
    types = {}
    for index, case in enumerate(targets):
        try:
            obj = lea_rax_rip(img, case)
            types[index] = {"case": f"0x{case:X}", "object": f"0x{obj:X}", "class": class_of_object(img, obj)}
        except ValueError:
            types[index] = {"case": f"0x{case:X}", "object": None, "class": "BASE_PLAYNOW wrapper/flag case"}
    out["playNowTypes"] = types
    check(types[0]["class"] == ".?AVBLACKTOP_DELEGATE@@", "type 0 is not Blacktop", failures)
    check(types[7]["class"] == ".?AVTKU_DELEGATE@@", "type 7 is not TKU", failures)
    check(types[8]["class"] == ".?AVTKU_WNBA_DELEGATE@@", "type 8 is not TKU_WNBA", failures)

    # League byte accessor/setters.
    check(img.read(V2["isWnbaAccessor"], 3) == b"\x0F\xB6\x05"
          and rel32_target(img, V2["isWnbaAccessor"], 3, 7) == V2["leagueByte"]
          and img.read(V2["isWnbaAccessor"] + 7, 1) == b"\xC3", "isWnba accessor moved", failures)
    check(img.read(V2["setWnbaLeague"] + 7, 2) == b"\xC6\x05"
          and rel32_target(img, V2["setWnbaLeague"] + 7, 2, 7) == V2["leagueByte"]
          and img.read(V2["setWnbaLeague"] + 13, 1) == b"\x01", "WNBA league setter moved", failures)

    # Freestyle launch: isWnba -> type 7+isWnba.
    fs = img.read(V2["freestyleLaunchCall"], 0x19)
    fs_ok = (fs[0] == 0xE8 and rel32_target(img, V2["freestyleLaunchCall"], 1, 5) == V2["isWnbaAccessor"]
             and fs[5:7] == b"\x89\xC3" and fs[7] == 0xE8
             and fs[12:18] == b"\x0F\xB6\xC8\x83\xC1\x07" and fs[18:20] == b"\x89\xDA" and fs[20] == 0xE8
             and rel32_target(img, V2["freestyleLaunchCall"] + 20, 1, 5) == V2["playNowInit"])
    check(fs_ok, "Freestyle launch no longer selects 7+isWnba", failures)
    check(img.read(0x36378B0, 5) == b"\xB8\x12\x5D\x4D\xF5", "Freestyle mode hash getter moved", failures)
    out["freestyleLaunch"] = {
        "function": f"0x{V2['freestyleLaunch']:X}",
        "playNowType": "7 + isWnba (TKU_DELEGATE or TKU_WNBA_DELEGATE)",
        "leagueByte": f"0x{V2['leagueByte']:X}",
        "freestyleLeagueGlobal": f"0x{V2['freestyleLeague']:X} = isWnba + 1",
        "freestyleModeHash": "0xF54D5D12",
        "verified": fs_ok,
    }

    # Blacktop launch: hard-coded type 0, league 0.
    btl = img.read(V2["blacktopLaunchCall"], 9)
    bt_ok = btl[0:4] == b"\x31\xC9\x31\xD2" and btl[4] == 0xE8 \
        and rel32_target(img, V2["blacktopLaunchCall"] + 4, 1, 5) == V2["playNowInit"]
    check(bt_ok, "Blacktop launch no longer calls PlayNow_InitGameMode(0,0)", failures)
    check(img.read(0x32FF654, 5) == b"\xB9\xDE\x06\xDF\x92", "Blacktop selection mode hash moved", failures)
    out["blacktopLaunch"] = {"function": f"0x{V2['blacktopLaunch']:X}", "playNowType": 0,
                             "league": 0, "consultsIsWnba": False, "verified": bt_ok}

    # 4. TKU vs TKU_WNBA vtable differences.
    diff = vtable_diff(img, vt["TKU_DELEGATE"], vt["TKU_WNBA_DELEGATE"])
    out["tkuVsTkuWnbaDifferingSlots"] = [f"+0x{o:X}" for o in diff]
    check(diff == [0x8, 0x18, 0xD30], "TKU/TKU_WNBA differ beyond identity + roster loader", failures)

    # 5. Blacktop category switch (WPF BcTable equivalent).
    cat = decode_jump_table(img, V2["categoryTable"], 6)
    fn = V2["categorySwitch"]
    old = WPF_OLD_BUILD
    old_rel = [t - old["categorySwitch"] for t in old["categoryTargets"]]
    new_rel = [t - fn for t in cat]
    out["blacktopCategory"] = {
        "switch": f"0x{fn:X}",
        "table": f"0x{V2['categoryTable']:X}",
        "targets": [f"0x{t:X}" for t in cat],
        "relativeTargets": [f"+0x{r:X}" for r in new_rel],
        "wpfOldRelativeTargets": [f"+0x{r:X}" for r in old_rel],
        "continuation": f"0x{V2['categoryContinue']:X}",
        "case3OriginalRel32": f"0x{(V2['categoryUnusedCase'] - V2['categoryTable']) & 0xFFFFFFFF:08X}",
        "caseMasks": {
            "0": f"0x{img.u32(cat[0] + 1):X}",
            "1": f"0x{img.u32(cat[1] + 1):X}",
            "2": f"0x{img.u32(cat[2] + 1):X}",
        },
        "bridge": "mov esi,0x20000000 (kind 29 WNBA); jmp continuation  -- same as WPF BcPrepareBridge",
    }
    check(new_rel == old_rel, "category switch case layout differs from WPF", failures)
    check(cat[3] == V2["categoryUnusedCase"], "case 3 target moved", failures)
    check(img.read(V2["categoryContinue"], 3) == b"\x48\x89\xF1"
          and V2["categoryContinue"] - fn == old["categoryContinue"] - old["categorySwitch"],
          "category continuation moved", failures)
    check(img.read(cat[0], 1) == b"\xBE" and img.u32(cat[0] + 1) == 1
          and img.u32(cat[1] + 1) == 2 and img.u32(cat[2] + 1) == 0x8000000,
          "category mask cases changed", failures)
    # Label initializer (prior audit): record id + Euro key stored at 0x3119780/0x311978A.
    check(img.read(0x3119780, 2) == b"\xC7\x05" and rel32_target(img, 0x3119780, 2, 10) == V2["labelRecord"]
          and img.u32(0x3119786) == 0xBA3C982D, "label record initializer moved", failures)

    out["wpfOldBuild"] = {k: (f"0x{v:X}" if isinstance(v, int) else [f"0x{x:X}" for x in v])
                          for k, v in old.items()}
    out["wpfToV2"] = {
        "callbackSlot": f"0x{old['callbackSlot']:X} (+0xD10) -> 0x{bt + 0xD30:X} (+0xD30)",
        "stockRet": f"0x{old['stockRet']:X} -> 0x{V2['stockRet']:X}",
        "loader": f"0x{old['loader']:X} -> 0x{V2['generalLoader']:X} (general) / 0x{V2['wnbaLoader']:X} (WNBA)",
        "categoryTable": f"0x{old['categoryTable']:X} -> 0x{V2['categoryTable']:X}",
        "categoryContinue": f"0x{old['categoryContinue']:X} -> 0x{V2['categoryContinue']:X}",
        "label": f"0x{old['label']:X} -> 0x{V2['labelRecord']:X}",
    }
    out["failures"] = failures
    out["ok"] = not failures
    return out


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--exe", default=DEFAULT_EXE)
    parser.add_argument("--out", help="write JSON report here")
    parser.add_argument("--skip-hash", action="store_true", help="skip SHA-256 (faster reruns)")
    args = parser.parse_args(argv)
    path = pathlib.Path(args.exe)
    data = path.read_bytes()
    report = {
        "schema": "nba2k27-wnba-v2-route-audit-v1",
        "executable": str(path),
        "sha256": None if args.skip_hash else hashlib.sha256(data).hexdigest().upper(),
        "gameProcessAccess": False,
        "gameFilesChanged": False,
        "writesPerformed": 0,
    }
    if report["sha256"] and report["sha256"] != EXPECTED_SHA256:
        report.update(ok=False, failures=["SHA-256 differs from the pinned Version 2 executable"])
    else:
        report.update(audit(Image(data)))
    text = json.dumps(report, indent=2)
    if args.out:
        pathlib.Path(args.out).write_text(text + "\n", encoding="utf-8")
    print(text)
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
