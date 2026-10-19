"""Disk-only audit of the Version 2 Blacktop outfit mapping (selection + carry-over).

Reads the installed NBA2K27.exe file and launcher source/assets only. Never opens
the game process and never writes any file. Standard library only.

Checks:
  * PE identity (timestamp/image size; optional full SHA-256 with --full-hash)
  * every FIELD_ANCHORS byte run in src/outfit_selection_v2.rs
  * every VIRTUAL_ANCHORS slot in src/outfit_selection_v2.rs
  * both lineup-count stores resolve to LINEUP+4 / LINEUP+8
  * MSVC RTTI class name of every vtable used by the selection and worker proofs
  * worker globals (assets/outfit-match-v2-profile.json) lie in a writable data section
  * optional (--xrefs): RIP-relative code references to ROOT+0x1080, LINEUP+{4,8,0xB8,0xC0}
    and the worker globals, as independent evidence that they are live globals

Usage: python -I tools/outfit_v2_disk_audit.py <NBA2K27.exe> [--root <launcher dir>] [--xrefs] [--full-hash]
"""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import re
import struct
import sys

EXPECTED_STAMP = 0x6AC53765
EXPECTED_IMAGE_SIZE = 0x35BDE000
EXPECTED_SHA = "29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1"
BASE = 0x140000000
# Old September build (0x6AA2013D / 0x36558000) values from src/blacktop_outfits.rs, for the report only.
LEGACY = {"ROOT": 0xAEB69D0, "LINEUP": 0x9BBF0A0, "MODEL": 0x6778FC8, "DESCRIPTION": 0x682D858}
LINEUP_STORES = [(0x32F799A, 4), (0x32F79A0, 8)]
CLONE_TABLE = 0x6884898


class Image:
    def __init__(self, data: bytes):
        self.data = data
        if data[:2] != b"MZ":
            raise ValueError("not an MZ image")
        pe = struct.unpack_from("<I", data, 0x3C)[0]
        if data[pe:pe + 4] != b"PE\0\0":
            raise ValueError("missing PE signature")
        machine, count, self.stamp = struct.unpack_from("<HHI", data, pe + 4)
        if machine != 0x8664:
            raise ValueError("not x64")
        optional = pe + 24
        optional_size = struct.unpack_from("<H", data, pe + 20)[0]
        self.base = struct.unpack_from("<Q", data, optional + 24)[0]
        self.size_of_image = struct.unpack_from("<I", data, optional + 56)[0]
        self.sections = []
        at = optional + optional_size
        for _ in range(count):
            name = data[at:at + 8].rstrip(b"\0").decode("ascii", "replace")
            vsize, rva, raw_size, raw = struct.unpack_from("<IIII", data, at + 8)
            flags = struct.unpack_from("<I", data, at + 36)[0]
            self.sections.append((name, rva, max(vsize, raw_size), raw, raw_size, flags))
            at += 40

    def section(self, rva: int):
        for s in self.sections:
            if s[1] <= rva < s[1] + s[2]:
                return s
        return None

    def read(self, rva: int, size: int) -> bytes:
        s = self.section(rva)
        if s is None or rva + size > s[1] + s[4]:
            raise ValueError(f"RVA {rva:X}+{size} not file-backed")
        off = s[3] + rva - s[1]
        return self.data[off:off + size]

    def q(self, rva: int) -> int:
        return struct.unpack("<Q", self.read(rva, 8))[0]

    def d(self, rva: int) -> int:
        return struct.unpack("<I", self.read(rva, 4))[0]

    def executable(self, rva: int) -> bool:
        s = self.section(rva)
        return bool(s and s[5] & 0x20000000)

    def writable_data(self, rva: int) -> bool:
        s = self.section(rva)
        return bool(s and s[5] & 0x80000000 and not s[5] & 0x20000000)

    def rtti_name(self, vtable: int) -> str | None:
        """MSVC x64: vtable[-1] -> CompleteObjectLocator (sig 1) -> TypeDescriptor name."""
        try:
            col = self.q(vtable - 8) - self.base
            if self.d(col) != 1 or self.d(col + 20) != col:
                return None
            td = self.d(col + 12)
            raw = self.read(td + 16, 256)
            name = raw[:raw.index(0)].decode("ascii")
            return name if name.startswith(".?A") else None
        except (ValueError, struct.error):
            return None


def parse_selection_source(text: str):
    consts = {k: int(v, 16) for k, v in re.findall(r"pub\(crate\) const (ROOT|LINEUP|MODEL|DESCRIPTION): u64 = 0x([0-9A-Fa-f_]+);", text)}
    field_block = text[text.index("FIELD_ANCHORS"):text.index("VIRTUAL_ANCHORS")]
    fields = []
    for rva, body in re.findall(r"\(\s*(0x[0-9A-Fa-f]+),\s*&\[([^\]]*)\]", field_block, re.S):
        fields.append((int(rva, 16), bytes(int(b, 0) for b in re.findall(r"0x[0-9A-Fa-f]+|\d+", body))))
    virt_block = text[text.index("VIRTUAL_ANCHORS"):text.index("pub(crate) fn verify_profile")]
    virtuals = [(consts[t], int(o, 0), int(m, 16))
                for t, o, m in re.findall(r"\((MODEL|DESCRIPTION),\s*(0x[0-9A-Fa-f]+|\d+),\s*0x([0-9A-Fa-f]+)\)", virt_block)]
    return consts, fields, virtuals


def rip_xrefs(img: Image, targets: dict[str, int], limit: int = 8) -> dict[str, list[int]]:
    """Find disp32 RIP-relative operands (ModRM mod=00 rm=101) whose instruction ends at disp+4.

    Approximate: instructions with a trailing immediate are missed, so counts are lower bounds.
    """
    want = {}
    for name, t in targets.items():
        want.setdefault(t, []).append(name)
    found = {name: [] for name in targets}
    modrm = re.compile(rb"[\x05\x0D\x15\x1D\x25\x2D\x35\x3D]")
    for name, rva, _vs, raw, raw_size, flags in img.sections:
        if not flags & 0x20000000:
            continue
        blob = img.data[raw:raw + raw_size]
        for m in modrm.finditer(blob, 0, len(blob) - 4):
            j = m.start() + 1
            disp = struct.unpack_from("<i", blob, j)[0]
            target = rva + j + 4 + disp
            if target in want:
                for n in want[target]:
                    if len(found[n]) < limit:
                        found[n].append(rva + j)  # RVA of the disp32 operand
    return found


ALLOCATE = 0x4B75FA0  # kAllocate in native/outfit_match_v2_profile.hpp
REQUEST_HEAP_TAG = 0xAD7FD911
PRODUCER_METHOD = 0x5FF090  # SHA-pinned stock create-request producer (profile methods)


def producer_allocation_lines(img: Image, allocate: int = ALLOCATE, tag: int = REQUEST_HEAP_TAG) -> list[tuple[int, int]]:
    """Line immediates the stock 0x40-byte create-request producer passes to Allocate.

    Live V2 order (0x5FF0D8): mov dword [rsp+20h], line (C7 44 24 20 imm32);
    mov ecx, 40h (B9 40 00 00 00); ...; mov r9d, tag (41 B9 imm32); call Allocate.
    WPF audit D9: the worker's packet allocation must use the same immediate.
    """
    lines = []
    needle = bytes.fromhex("41B9") + struct.pack("<I", tag)
    for _name, rva, _vs, raw, raw_size, flags in img.sections:
        if not flags & 0x20000000:
            continue
        blob = img.data[raw:raw + raw_size]
        at = blob.find(needle)
        while at >= 0:
            lo = max(0, at - 24)
            before = blob[lo:at]
            size = before.rfind(bytes.fromhex("B940000000"))
            line = before.rfind(bytes.fromhex("C7442420"))
            call = at + len(needle)
            if (size >= 0 and 0 <= line < size and call + 5 <= len(blob) and blob[call] == 0xE8
                    and rva + call + 5 + struct.unpack_from("<i", blob, call + 1)[0] == allocate):
                lines.append((rva + lo + line, struct.unpack_from("<I", before, line + 4)[0]))
            at = blob.find(needle, at + 1)
    return lines


def worker_allocation_line(root: pathlib.Path) -> int | None:
    text = (root / "native" / "outfit_match_v2_native.hpp").read_text(encoding="utf-8")
    m = re.search(r"0xAD7FD911,(0x[0-9A-Fa-f]+),", text)
    return int(m.group(1), 16) if m else None


def audit(img: Image, root: pathlib.Path, xrefs: bool = False) -> dict:
    checks, failures = [], []

    def check(name: str, ok: bool, detail=None):
        checks.append({"check": name, "ok": bool(ok), **({"detail": detail} if detail is not None else {})})
        if not ok:
            failures.append(name)

    check("pe-identity", img.stamp == EXPECTED_STAMP and img.size_of_image == EXPECTED_IMAGE_SIZE and img.base == BASE,
          f"stamp={img.stamp:08X} size={img.size_of_image:08X}")
    consts, fields, virtuals = parse_selection_source((root / "src" / "outfit_selection_v2.rs").read_text(encoding="utf-8"))
    check("selection-constants-parsed", set(consts) == {"ROOT", "LINEUP", "MODEL", "DESCRIPTION"} and len(fields) >= 10 and len(virtuals) >= 10,
          {k: f"{v:X}" for k, v in consts.items()})
    for rva, expected in fields:
        try:
            ok = img.read(rva, len(expected)) == expected and img.executable(rva)
        except ValueError:
            ok = False
        check(f"field-anchor-{rva:X}", ok)
    for table, slot, method in virtuals:
        try:
            ok = img.q(table + slot) == img.base + method
        except ValueError:
            ok = False
        check(f"virtual-{table:X}+{slot:X}", ok, f"-> {method:X}")
    lineup = consts.get("LINEUP", 0)
    for at, field in LINEUP_STORES:
        b = img.read(at, 6)
        ok = b[:2] == b"\x89\x05" and at + 6 + struct.unpack("<i", b[2:6])[0] == lineup + field
        check(f"lineup-store-{at:X}->LINEUP+{field}", ok)
    for name in ("ROOT", "LINEUP"):
        check(f"{name.lower()}-in-writable-data", img.writable_data(consts.get(name, 0)))

    profile = json.loads((root / "assets" / "outfit-match-v2-profile.json").read_text(encoding="utf-8"))
    tables = {consts["MODEL"]: "selection MODEL", consts["DESCRIPTION"]: "selection DESCRIPTION", CLONE_TABLE: "worker clone"}
    for t in profile["virtualTables"]:
        tables.setdefault(t["rva"], "worker profile")
    rtti = {}
    for t, role in sorted(tables.items()):
        name = img.rtti_name(t)
        rtti[f"{t:X}"] = {"role": role, "class": name}
        check(f"rtti-{t:X}", name is not None, name)
    for g, v in profile["globals"].items():
        check(f"global-{g}-in-writable-data", img.writable_data(v), f"{v:X}")
    # The worker emulates the producer inside the hashed method 0x5FF090.
    producer = next((m for m in profile["methods"] if m["rva"] == PRODUCER_METHOD), None)
    sites = producer_allocation_lines(img)
    inside = [v for at, v in sites if producer and producer["rva"] <= at < producer["rva"] + producer["bytes"]]
    worker_line = worker_allocation_line(root)
    check("producer-allocation-line-matches-worker",
          worker_line is not None and inside == [worker_line],
          {"producerMethod": f"{PRODUCER_METHOD:X}", "inside": [f"{v:X}" for v in inside],
           "allSites": {f"{at:X}": f"{v:X}" for at, v in sites},
           "worker": f"{worker_line:X}" if worker_line is not None else None})

    result = {
        "schema": "outfit-v2-disk-audit-v1",
        "stamp": f"{img.stamp:08X}",
        "imageSize": f"{img.size_of_image:08X}",
        "selection": {k: f"{v:X}" for k, v in consts.items()},
        "legacyDelta": {k: f"+{consts[k] - LEGACY[k]:X}" for k in LEGACY if k in consts},
        "rtti": rtti,
        "globals": {k: f"{v:X}" for k, v in profile["globals"].items()},
        "checks": checks,
        "failures": failures,
        "gameProcessAccess": False,
        "filesWritten": False,
    }
    if xrefs:
        targets = {"ROOT": consts["ROOT"], "ROOT+1080": consts["ROOT"] + 0x1080}
        for off in (4, 8, 0xB8, 0xC0):
            targets[f"LINEUP+{off:X}"] = lineup + off
        targets.update({f"global:{k}": v for k, v in profile["globals"].items()})
        refs = rip_xrefs(img, targets)
        result["xrefs"] = {k: [f"{r:X}" for r in v] for k, v in refs.items()}
        for k, v in refs.items():
            if k.startswith("ROOT"):
                continue
            check(f"xref-{k}", len(v) > 0, len(v))
        # ROOT is a struct base (lea/mov ROOT, then [reg+0x1080]); either form is evidence.
        check("xref-ROOT-or-ROOT+1080", len(refs["ROOT"]) + len(refs["ROOT+1080"]) > 0,
              len(refs["ROOT"]) + len(refs["ROOT+1080"]))
        result["failures"] = failures
    result["result"] = "passed" if not failures else "failed"
    return result


def main(argv=None) -> int:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("exe")
    p.add_argument("--root", default=str(pathlib.Path(__file__).resolve().parent.parent))
    p.add_argument("--xrefs", action="store_true")
    p.add_argument("--full-hash", action="store_true")
    a = p.parse_args(argv)
    data = pathlib.Path(a.exe).read_bytes()
    img = Image(data)
    result = audit(img, pathlib.Path(a.root), a.xrefs)
    if a.full_hash:
        sha = hashlib.sha256(data).hexdigest().upper()
        result["sha256"] = sha
        if sha != EXPECTED_SHA:
            result["failures"].append("full-sha256")
            result["result"] = "failed"
    print(json.dumps(result, indent=1))
    return 0 if result["result"] == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())
