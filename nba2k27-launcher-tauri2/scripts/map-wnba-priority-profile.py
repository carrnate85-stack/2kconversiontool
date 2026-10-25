"""Disk-only remapper for the WNBA native priority profile.

Reconstructs the prior verified function bodies from QA058 disassembly JSON,
turns relative control-flow and RIP displacements into wildcards, then searches
the current executable's .text image. It also resolves the two required native
vtables from MSVC RTTI by class name. It never opens the game process.
"""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import struct
import sys

HERE = Path(__file__).resolve().parent
REPO = HERE.parent
ROOT = REPO.parent
sys.path.insert(0, str(ROOT / "research" / "python_modules"))

import pefile
from capstone import Cs, CS_ARCH_X86, CS_MODE_64
from capstone.x86_const import X86_OP_MEM, X86_REG_RIP

IMAGE = Path(
    os.environ.get(
        "C27_GAME_EXE",
        r"C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27\NBA2K27.exe",
    )
)
SOURCES = [
    REPO / "artifacts" / "qa057-wnba-fastload-live-types-20261018.json",
    REPO / "artifacts" / "qa058-wnba-priority-registration-20261018.json",
    REPO / "artifacts" / "qa058-wnba-native-priority-lifecycle-20261018.json",
    REPO / "artifacts" / "qa058-wnba-priority-callers-20261018.json",
    REPO / "artifacts" / "qa058-wnba-priority-ownership-20261018.json",
]
METHODS = {
    "register": (0x51F2230, 349),
    "lock": (0x4B99460, 39),
    "unlock": (0x4B99420, 62),
    "wad_ctor": (0x5200960, 49),
    "wad_init": (0x5200BA0, 2222),
    "wad_get": (0x5201650, 360),
    "wad_contains": (0x5201560, 143),
    "wad_cleanup": (0x5200A10, 292),
    "wad_lock": (0x51F8000, 141),
}


def walk(value):
    if isinstance(value, dict):
        yield value
        for child in value.values():
            yield from walk(child)
    elif isinstance(value, list):
        for child in value:
            yield from walk(child)


def reconstruct(target: int, size: int, documents: list[object]) -> bytes:
    candidates = []
    for doc in documents:
        for node in walk(doc):
            ins = node.get("instructions")
            if not isinstance(ins, list) or not ins:
                continue
            rows = [x for x in ins if isinstance(x, dict) and isinstance(x.get("rva"), int) and isinstance(x.get("bytes"), str)]
            if not rows:
                continue
            by_rva = {x["rva"]: bytes.fromhex(x["bytes"]) for x in rows}
            cursor = target
            out = bytearray()
            while cursor in by_rva and len(out) < size:
                raw = by_rva[cursor]
                out.extend(raw)
                cursor += len(raw)
            if len(out) >= size:
                candidates.append(bytes(out[:size]))
    unique = list(dict.fromkeys(candidates))
    if len(unique) != 1:
        raise RuntimeError(f"0x{target:X}: expected one reconstructed body, got {len(unique)}")
    return unique[0]


def relocation_mask(code: bytes, base: int) -> bytes:
    mask = bytearray(b"x" * len(code))
    md = Cs(CS_ARCH_X86, CS_MODE_64)
    md.detail = True
    for ins in md.disasm(code, base):
        off = ins.address - base
        if ins.group(1) or ins.group(2):  # CS_GRP_JUMP / CS_GRP_CALL
            if ins.imm_size:
                mask[off + ins.imm_offset : off + ins.imm_offset + ins.imm_size] = b"?" * ins.imm_size
        if any(op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP for op in ins.operands):
            if ins.disp_size:
                mask[off + ins.disp_offset : off + ins.disp_offset + ins.disp_size] = b"?" * ins.disp_size
    return bytes(mask)


def longest_fixed(mask: bytes) -> tuple[int, int]:
    best = (0, 0)
    start = None
    for i, c in enumerate(mask + b"?"):
        if c == ord("x") and start is None:
            start = i
        elif c != ord("x") and start is not None:
            if i - start > best[1]:
                best = (start, i - start)
            start = None
    return best


def masked_hits(haystack: bytes, base: int, needle: bytes, mask: bytes) -> list[int]:
    anchor_at, anchor_len = longest_fixed(mask)
    if anchor_len < 8:
        raise RuntimeError(f"weak fixed anchor: {anchor_len}")
    anchor = needle[anchor_at : anchor_at + anchor_len]
    hits = []
    cursor = 0
    while True:
        at = haystack.find(anchor, cursor)
        if at < 0:
            return hits
        start = at - anchor_at
        if start >= 0 and start + len(needle) <= len(haystack):
            sample = haystack[start : start + len(needle)]
            if all(m == ord("?") or a == b for a, b, m in zip(sample, needle, mask)):
                hits.append(base + start)
        cursor = at + 1


def section(pe, name: bytes):
    return next(x for x in pe.sections if x.Name.rstrip(b"\0") == name)


def rtti_vtable(pe, class_name: str) -> dict:
    image = pe.__data__
    base = pe.OPTIONAL_HEADER.ImageBase
    type_name = (".?AV" + class_name + "@@").encode() + b"\0"
    name_offsets = []
    cursor = 0
    while True:
        at = image.find(type_name, cursor)
        if at < 0:
            break
        name_offsets.append(at)
        cursor = at + 1
    rows = []
    for name_off in name_offsets:
        type_off = name_off - 16
        type_rva = pe.get_rva_from_offset(type_off)
        # x64 COL stores the type descriptor as an RVA at +0x0c.
        needle = struct.pack("<I", type_rva)
        for sec in pe.sections:
            raw = sec.get_data()
            p = 0
            while True:
                p = raw.find(needle, p)
                if p < 0:
                    break
                col_rva = sec.VirtualAddress + p - 12
                try:
                    sig, obj, cd, typ, hier, self_rva = struct.unpack("<IIIIII", pe.get_data(col_rva, 24))
                except struct.error:
                    p += 1
                    continue
                if sig == 1 and typ == type_rva and self_rva == col_rva:
                    col_va = base + col_rva
                    qneedle = struct.pack("<Q", col_va)
                    for sec2 in pe.sections:
                        raw2 = sec2.get_data()
                        q = 0
                        while True:
                            q = raw2.find(qneedle, q)
                            if q < 0:
                                break
                            rows.append({"typeRva": type_rva, "colRva": col_rva,
                                         "objectOffset": obj, "vtableRva": sec2.VirtualAddress + q + 8})
                            q += 1
                p += 1
    unique = []
    seen = set()
    for row in rows:
        key = tuple(row.values())
        if key not in seen:
            seen.add(key)
            unique.append(row)
    return {"class": class_name, "matches": unique}


def main():
    digest = hashlib.sha256(IMAGE.read_bytes()).hexdigest().upper()
    documents = [json.loads(p.read_text(encoding="utf-8")) for p in SOURCES]
    pe = pefile.PE(str(IMAGE), fast_load=True)
    text = section(pe, b".text")
    raw = text.get_data()
    pdata = section(pe, b".pdata").get_data()
    runtime_functions = {begin: (end, unwind) for begin, end, unwind in
                         struct.iter_unpack("<III", pdata[: len(pdata) // 12 * 12]) if begin and end > begin}
    # Prior helper 0x3C78080 loads the resource-wrapper context pointer
    # (wrapper+0x38) before calling the stable request initializer. Preserve
    # structure and constants; wildcard only RIP/call displacements.
    wrapper_probe = bytes.fromhex(
        "56574883EC3889CE488B3D00000000E800000000C5F857C0C5F811442420"
        "48C7442430000000004889F94889C24189F04531C9E800000000"
    )
    wrapper_mask = bytearray(b"x" * len(wrapper_probe))
    wrapper_mask[11:15] = b"????"
    wrapper_mask[16:20] = b"????"
    wrapper_mask[-4:] = b"????"
    wrapper_hits = masked_hits(raw, text.VirtualAddress, wrapper_probe, bytes(wrapper_mask))
    wrappers = []
    for hit in wrapper_hits:
        insn = pe.get_data(hit + 8, 7)
        context_slot = hit + 15 + struct.unpack_from("<i", insn, 3)[0]
        wrappers.append({"probeRva": hit, "contextSlotRva": context_slot,
                         "wrapperRva": context_slot - 0x38})
    mapped = {}
    for name, (old_rva, size) in METHODS.items():
        prior = reconstruct(old_rva, size, documents)
        mask = relocation_mask(prior, old_rva)
        hits = masked_hits(raw, text.VirtualAddress, prior, mask)
        mapped[name] = {
            "oldRva": old_rva,
            "bytes": size,
            "oldSha256": hashlib.sha256(prior).hexdigest().upper(),
            "fixedBytes": mask.count(b"x"),
            "longestFixed": longest_fixed(mask)[1],
            "hits": hits,
            "hitSha256": [hashlib.sha256(pe.get_data(x, size)).hexdigest().upper() for x in hits],
            "runtimeFunctions": [
                {"begin": x, "end": runtime_functions[x][0], "unwind": runtime_functions[x][1],
                 "bytes": runtime_functions[x][0] - x}
                for x in hits if x in runtime_functions
            ],
        }
    report = {
        "schema": "nba2k27-wnba-priority-static-remap-v1",
        "image": str(IMAGE),
        "imageSha256": digest,
        "timestamp": pe.FILE_HEADER.TimeDateStamp,
        "sizeOfImage": pe.OPTIONAL_HEADER.SizeOfImage,
        "processAccess": False,
        "gameWrites": 0,
        "methods": mapped,
        "wrapper": {"oldRva": 0xBB9A8E8, "matches": wrappers},
        "rtti": [rtti_vtable(pe, "VCFAST_LOAD_WIN32"), rtti_vtable(pe, "VCFAST_LOAD_WADFILES_MANIFEST")],
    }
    output_dir = REPO / "artifacts" / "wnba-priority-remap"
    output_dir.mkdir(parents=True, exist_ok=True)
    out = output_dir / f"priority-static-map-{digest[:12]}.json"
    out.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    bad_methods = [
        name
        for name, row in mapped.items()
        if len(row["hits"]) != 1
        or len(row["runtimeFunctions"]) != 1
        or row["runtimeFunctions"][0]["bytes"] != row["bytes"]
    ]
    bad_types = [row["class"] for row in report["rtti"] if len(row["matches"]) != 1]
    if bad_methods or bad_types or len(wrappers) != 1:
        raise RuntimeError(
            f"ambiguous remap; methods={bad_methods}, types={bad_types}, "
            f"wrappers={len(wrappers)}; inspect {out}"
        )
    print(json.dumps({"out": str(out), "methods": {k: [hex(x) for x in v["hits"]] for k, v in mapped.items()},
                      "rtti": report["rtti"]}, indent=2))


if __name__ == "__main__":
    main()
