"""Developer-only Version 2 roster WRITE-mapping proof. Disk image only.

Never opens a game process and never changes game files. It proves, from the
exact NBA 2K27 Version 2 executable, the record layouts the browser Save &
verify writers use, then emits a compact asset that the launcher re-checks
against live image bytes before any write:

* uniform record: the 52 legacy uniform controls plus identity fields are
  traced through the named UNIFORM serializer (same provenance extractor as
  scripts/audit-roster-v2-fields.py; rdi stays the record until 0x688E9B);
* team text: Franchise_HistoricChanges_HandleRebranding copies each TEAMDATA
  wide-string buffer with an explicit capacity, and every capacity ends
  exactly where the next copied buffer starts (proves offset, width, UTF-16);
* uniform slots: TEAMDATA stores owner+0x298 + index*0x170 into
  team+0xDE0+slot*8 (slots 30/31 are 0xED0/0xED8 in the slot getter).

Player bitfields are proven separately by assets/roster-v2-field-proof.json.
"""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools/python-packages"))
import pefile  # noqa: E402
from capstone import CS_ARCH_X86, CS_MODE_64, Cs  # noqa: E402
from capstone.x86 import X86_REG_RDI  # noqa: E402

IMAGE_SHA = "29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1"
UNIFORM_START, UNIFORM_RECORD_END, UNIFORM_END = 0x687590, 0x688E9B, 0x68914F
REBRAND_TEXT = [(0x1DAE4BD, 0x1DAE531), (0x1DAE658, 0x1DAE692)]
WIDE_COPY, NARROW_COPY = 0x23F4DD0, 0x226B1F0
SLOT_STORE = (0x2F3C9F5, 0x2F3CA13)
SLOT_GETTER = (0x2F89452, 0x2F8947B)
TEAM_GETTERS = [(0x2F87C20, 8), (0x2F86B90, 8), (0x2F86BA0, 8)]
EXPECTED_TEXT = {0x2FA: 0x19, 0x32C: 0x12, 0x350: 7, 0x35E: 0xA, 0x170: 0x1F, 0x1AE: 0x26}
TEAM_KEYS = {0x2FA: "name", 0x32C: "cityName", 0x350: "cityShort", 0x35E: "stateShort", 0x1AE: "mainLogo"}
# Legacy uniform layout spells a few serializer properties differently.
UNIFORM_ALIASES = {"Id": "ID", "TeamId": "TeamID", "HomePriority": "HomeSelectionPriorityRank",
                   "AwayPriority": "AwaySelectionPriorityRank", "OverrideNbaPatch": "OverrideNBAPatch",
                   "LocalizedNameToken": "LocalizedName", "LocalizedTeamNameToken": "LocalizedTeamName",
                   "Name": "UnlocalizedName", "TeamName": "UnlocalizedTeamName"}


def extractor():
    spec = importlib.util.spec_from_file_location("audit_roster_v2_fields", ROOT / "scripts/audit-roster-v2-fields.py")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def code(pe, start, end):
    return pe.get_data(start, end - start)


def disasm(md, pe, start, end):
    rows = list(md.disasm(code(pe, start, end), start))
    assert rows and rows[-1].address + rows[-1].size == end, f"Incomplete disassembly {start:#x}"
    return rows


def uniform_proof(pe, md, aud):
    rows = disasm(md, pe, UNIFORM_START, UNIFORM_END)
    writes = [(r.address, r.mnemonic, r.op_str) for r in rows if X86_REG_RDI in r.regs_access()[1]]
    assert writes[0] == (UNIFORM_START + 0x2F, "mov", "rdi, rdx"), "Uniform record provenance changed"
    assert writes[1] == (UNIFORM_RECORD_END, "mov", "rdi, qword ptr [rdi]"), "Uniform record range changed"
    results, _ = aud.extract(pe, md, [r for r in rows if r.address < UNIFORM_RECORD_END])
    found = {}
    for row in results:
        assert row["key"] not in found, f"Duplicate uniform property {row['key']}"
        found[row["key"]] = row
    legacy = json.loads((ROOT / "assets/roster-layout.json").read_text(encoding="utf8"))["uniforms"]
    by_key = {u["key"]: u for u in legacy if u["key"]}
    write = json.loads((ROOT / "assets/uniform-write-layout.json").read_text(encoding="utf8"))["fields"]
    controls, mismatches = [], []
    for field in write:
        prop = by_key[field["readKey"]]["property"]
        row = found.get(UNIFORM_ALIASES.get(prop, prop))
        shape = aud.shape(row["bits"]) if row else None
        width = field["mask"].bit_length()
        ok = (shape is not None and field["mask"] == (1 << width) - 1
              and shape["offset"] * 8 + shape["shift"] == field["offset"] * 8 + field["shift"]
              and shape["bits"] == width)
        entry = {"request": field["request"], "property": prop, "serializerKey": UNIFORM_ALIASES.get(prop, prop), "offset": field["offset"], "shift": field["shift"],
                 "mask": field["mask"], "nameRva": row["nameRva"] if row else None, "matches": ok}
        (controls if ok else mismatches).append(entry)
    identity = []
    for key, prop, offset, bits in [("uniformId", "ID", 0x50, 16), ("teamId", "TeamID", 0x52, 16)]:
        row = found[prop]
        assert aud.shape(row["bits"]) == {"offset": offset, "shift": 0, "bits": bits}, prop
        identity.append({"key": key, "property": prop, "offset": offset, "bytes": bits // 8, "nameRva": row["nameRva"]})
    base = found["BaseFilename"]
    assert isinstance(base["bits"], aud.RecordRef) and base["bits"].offset == 8, "BaseFilename moved"
    identity.append({"key": "baseFilename", "property": "BaseFilename", "offset": 8, "bytes": 64, "nameRva": base["nameRva"]})
    # Read-only display fields of the browser uniform card: every legacy
    # property must resolve to exactly the same record bits. Flags words are
    # whole-word aggregates of the controls above, not serializer properties.
    display = []
    for entry in legacy:
        if entry["property"].startswith("Flags"):
            assert entry["kind"] == "uint" and entry["offset"] in (0x160, 0x164, 0x168, 0x16C) and entry["bytes"] == 4
            continue
        row = found[UNIFORM_ALIASES.get(entry["property"], entry["property"])]
        if entry["kind"] == "ascii":
            ok = isinstance(row["bits"], aud.RecordRef) and row["bits"].offset == entry["offset"]
        else:
            shape = aud.shape(row["bits"])
            start = entry["offset"] * 8 + entry.get("shift", 0)
            width = entry["mask"].bit_length() if entry["kind"] in ("masked", "bool") else entry["bytes"] * 8
            ok = shape is not None and shape["offset"] * 8 + shape["shift"] == start and shape["bits"] == width
        assert ok, f"Uniform display field {entry['property']} differs"
        display.append(entry["property"])
    body = code(pe, UNIFORM_START, UNIFORM_RECORD_END)
    return {"displayFields": display, "serializerRva": UNIFORM_START, "recordBytes": UNIFORM_RECORD_END - UNIFORM_START,
            "recordSha256": hashlib.sha256(body).hexdigest(), "controls": controls, "identity": identity,
            "mismatches": mismatches}


def team_text_proof(pe, md):
    copies = []
    for start, end in REBRAND_TEXT:
        rows = disasm(md, pe, start, end)
        assert len(rows) % 5 == 0, "Unexpected rebranding copy shape"
        for i in range(0, len(rows), 5):
            src, dst, cap, flag, call = rows[i:i + 5]
            assert src.mnemonic == dst.mnemonic == "lea" and src.op_str.startswith("rdx, [r12 + ") and dst.op_str.startswith("rcx, [r13 + ")
            offset = int(src.op_str.split("+ ")[1].rstrip("]"), 16)
            assert dst.op_str == f"rcx, [r13 + {offset:#x}]" and cap.mnemonic == "mov" and cap.op_str.startswith("r8d, ")
            assert flag.op_str == "r9d, r9d" and call.mnemonic == "call" and int(call.op_str, 16) == WIDE_COPY
            copies.append((offset, int(cap.op_str.split(", ")[1], 16)))
    assert dict(copies) == EXPECTED_TEXT, f"Team text capacities changed: {copies}"
    # The next copied buffer begins exactly at offset + 2*capacity, so each
    # capacity counts UTF-16 units and no unlisted field hides in between.
    after = disasm(md, pe, 0x1DAE5C7, 0x1DAE5E4)
    assert after[0].op_str == "rdx, [r12 + 0x372]" and int(after[-1].op_str, 16) == NARROW_COPY
    logo_next = disasm(md, pe, 0x1DAE601, 0x1DAE61E)
    assert logo_next[0].op_str == "rdx, [r12 + 0x1fa]" and int(logo_next[-1].op_str, 16) == NARROW_COPY
    ends = {0x2FA: 0x32C, 0x32C: 0x350, 0x350: 0x35E, 0x35E: 0x372, 0x170: 0x1AE, 0x1AE: 0x1FA}
    for offset, capacity in copies:
        assert offset + capacity * 2 == ends[offset], f"Team buffer {offset:#x} is not contiguous"
    getters = [(rva, code(pe, rva, rva + n)) for rva, n in TEAM_GETTERS]
    assert getters[0][1] == bytes.fromhex("488d81fa020000c3") and getters[2][1] == bytes.fromhex("488d8150030000c3")
    assert getters[1][1] == bytes.fromhex("0fb781200f0000c3")
    return [{"key": TEAM_KEYS[o], "offset": o, "capacityUnits": c} for o, c in copies if o in TEAM_KEYS]


def slot_proof(pe, md):
    rows = disasm(md, pe, *SLOT_STORE)
    text = [f"{r.mnemonic} {r.op_str}" for r in rows]
    assert text[:3] == ["imul ecx, ecx, 0x170", "add rcx, qword ptr [rax + 0x298]", "mov qword ptr [r13 + rbx*8 + 0xde0], rcx"], text
    getter = [f"{r.mnemonic} {r.op_str}" for r in disasm(md, pe, *SLOT_GETTER)]
    assert getter[0] == "mov rax, qword ptr [rcx + rax*8 + 0xde0]" and "mov rax, qword ptr [rcx + 0xed0]" in getter, getter
    return {"offset": 0xDE0, "count": 32, "stride": 8, "uniformTable": 0x298, "uniformStride": 0x170}


def anchors(pe):
    spans = REBRAND_TEXT + [SLOT_STORE, SLOT_GETTER] + [(r, r + n) for r, n in TEAM_GETTERS]
    return [{"rva": a, "hex": code(pe, a, b).hex()} for a, b in spans]


def main():
    p = argparse.ArgumentParser()
    p.add_argument("image", type=Path)
    p.add_argument("--asset", type=Path, help="Write the compact runtime proof (no game files change)")
    args = p.parse_args()
    with args.image.open("rb") as file:
        assert hashlib.file_digest(file, "sha256").hexdigest().upper() == IMAGE_SHA, "Exact Version 2 image required"
    pe = pefile.PE(str(args.image), fast_load=True)
    assert pe.FILE_HEADER.TimeDateStamp == 0x6AC53765 and pe.OPTIONAL_HEADER.SizeOfImage == 0x35BDE000
    md = Cs(CS_ARCH_X86, CS_MODE_64)
    md.detail = True
    uniform = uniform_proof(pe, md, extractor())
    report = {"schema": "nba2k27-v2-roster-write-proof-v1", "imageSha256": IMAGE_SHA, "gameProcessAccess": False,
              "gameFilesChanged": False, "uniform": uniform, "teamText": team_text_proof(pe, md),
              "uniformSlots": slot_proof(pe, md), "anchors": anchors(pe),
              "refused": {"team": ["rosterTypeId (V2 consumes six bits, legacy wrote five)", "historicYearId", "arena/floor"],
                          "player": ["weight", "height/wingspan (appearance pointer)", "audio", "From/college"],
                          "uniform": ["linking an empty slot"]},
              "generatorSha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
    ok = not uniform["mismatches"] and len(uniform["controls"]) == 52
    if args.asset:
        assert ok, "Incomplete proof cannot become an asset"
        args.asset.write_text(json.dumps(report, separators=(",", ":")) + "\n", encoding="utf8")
    print(json.dumps({"uniformControls": len(uniform["controls"]), "mismatches": len(uniform["mismatches"]),
                      "teamText": len(report["teamText"]), "anchors": len(report["anchors"]), "gameProcessAccess": False}))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
