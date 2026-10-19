"""Disk-only scan of the Version 2 roster owner's table directories.

The proven V2 read path (src/roster_v2_profile.rs) reads the roster owner's
16-byte directories {count u64, table ptr u64}. The exe has one generated
bounded accessor per directory, of the form

    cmp  edx, r8d / [rcx + D]       ; index < count at owner+D
    jae  ...
    mov  eax, edx
    <index * STRIDE>                ; imul / shl / lea combinations
    add  rax, [rcx + D + 8]         ; table pointer at owner+D+8

Known V2 values: players D=0x10 stride 0x4F8, arenas D=0x60 stride 0x1278,
teams D=0x80 stride 0x1550, uniforms D=0x290 stride 0x170. Every other owner
directory (staff, season stats, awards, playbooks, ...) is indexed the same
way, so this lists all (D, STRIDE) pairs with their code sites. They are
candidates only: a table is named by matching stride and slot order against the
old build (see artifacts/editor-services-v2-research-20261017.md).

Reads the exe file only. Never opens the game process. Standard library only.

Usage: python -I tools/roster_v2_owner_directory_scan.py <NBA2K27.exe> [--json out.json] [--max-disp 0x800]
"""
from __future__ import annotations

import argparse
import collections
import json
import pathlib
import re
import struct
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from outfit_v2_disk_audit import EXPECTED_IMAGE_SIZE, EXPECTED_STAMP, Image  # noqa: E402

KNOWN = {0x10: (0x4F8, "players"), 0x60: (0x1278, "arenas"), 0x80: (0x1550, "teams"), 0x290: (0x170, "uniforms")}
# Byte-exact V2 code anchors for the editor services (staff, rotations, seasons).
# Found in the roster validator / team copy-assignment; each matches the old
# WPF helper's offsets (NBA2K27RosterResearch TeamStaff/TeamRotations/SeasonStats).
SERVICE_ANCHORS = [
    ("staff: 32 team slots", 0x2F18C26, "4180fc20"),                        # cmp r12b,0x20
    ("staff: team+0xF70+slot*8 pointer", 0x2F18C35, "4a8b94e0700f0000"),     # mov rdx,[rax+r12*8+0xF70]
    ("staff: record size 0x1C8", 0x2F18C4B, "41b8c8010000"),                 # mov r8d,0x1C8
    ("staff: slot = staff+0x182 & 0x1F", 0x2F18C7D, "0fb6868201000083e01f"),
    ("staff: team slot points back to staff", 0x2F115ED, "4c39a4d0700f0000"),
    ("staff: staff+0x18 -> team, team id +0xF20", 0x2F1164E, "498b4424180fb780200f0000"),
    ("rotations: team+0x1360 block", 0x2F27B00, "488d8f60130000488d8660130000"),
    ("rotations: 0x1E0 bytes (48 x 5 u16)", 0x2F27B84, "483de0010000"),
    ("seasons: pool kind +0x1C0 / pool +0x1C8", 0x2F0B6C7, "83bec001000000740c488bb6c8010000"),
    ("seasons: owner+0x1D0 count, 64-byte records", 0x2F06306, "48638ed00100004885c97e4d48c1e106"),
]


def check_anchors(img: Image):
    out = []
    for name, rva, want in SERVICE_ANCHORS:
        try:
            got = img.read(rva, len(want) // 2).hex()
        except ValueError:
            got = None
        out.append({"name": name, "rva": f"0x{rva:X}", "ok": got == want})
    return out


ADD = re.compile(rb"\x48\x03(\x41|\x81)")
REGS = {0: "ax", 2: "dx"}


def stride_of(arith: bytes):
    """Evaluate the index arithmetic between `mov eax, edx` and the table add.

    Supports the forms MSVC emits for this accessor family: imul imm32/imm8,
    shl, lea [r+r*k], add rax,rax and mov rdx,rax. Returns the stride or None.
    """
    r = {"ax": 1, "dx": 1}
    i = 0
    while i < len(arith):
        s = arith[i:]
        if s[:3] == b"\x48\x69\xc0" and len(s) >= 7:
            r["ax"] *= struct.unpack_from("<I", s, 3)[0]
            i += 7
        elif s[:3] == b"\x48\x6b\xc0" and len(s) >= 4:
            r["ax"] *= s[3]
            i += 4
        elif s[:3] == b"\x48\xc1\xe0" and len(s) >= 4:
            r["ax"] <<= s[3]
            i += 4
        elif s[:3] == b"\x48\xc1\xe2" and len(s) >= 4:
            r["dx"] <<= s[3]
            i += 4
        elif s[:3] == b"\x48\x89\xc2":
            r["dx"] = r["ax"]
            i += 3
        elif s[:3] == b"\x48\x01\xc0":
            r["ax"] *= 2
            i += 3
        elif s[:3] == b"\x48\x8d\x04" and len(s) >= 4:
            scale, index, base = 1 << (s[3] >> 6), (s[3] >> 3) & 7, s[3] & 7
            if index not in REGS or base not in REGS:
                return None
            r["ax"] = r[REGS[base]] + r[REGS[index]] * scale
            i += 4
        else:
            return None
    return r["ax"] if r["ax"] > 1 else None


def scan_code(code: bytes, rva0: int, max_disp: int = 0x800):
    """Yield (owner offset, stride, rva of the add) for `mov eax,edx; <arith>; add rax,[rcx+D+8]`."""
    for m in ADD.finditer(code):
        i = m.start()
        if m.group(1) == b"\x41":
            if i + 4 > len(code):
                continue
            disp = struct.unpack_from("<b", code, i + 3)[0]
        else:
            if i + 7 > len(code):
                continue
            disp = struct.unpack_from("<i", code, i + 3)[0]
        disp -= 8
        if not 0 <= disp <= max_disp or disp % 8:
            continue
        j = code.rfind(b"\x89\xd0", max(0, i - 20), i)
        if j < 0:
            continue
        stride = stride_of(code[j + 2:i])
        if stride:
            yield disp, stride, rva0 + i


def scan(img: Image, max_disp: int = 0x800):
    pairs = collections.defaultdict(list)
    for _name, rva, _size, raw, raw_size, flags in img.sections:
        if not flags & 0x20000000:
            continue
        for disp, stride, site in scan_code(img.data[raw:raw + raw_size], rva, max_disp):
            pairs[(disp, stride)].append(site)
    return pairs


def family(pairs, window: int = 0x40000):
    """{owner offset: [strides]} from sites near the players/teams accessors only.

    Unrelated code that uses the same idiom on other objects is excluded by
    keeping sites within `window` of the densest players/teams anchor.
    """
    anchors = pairs.get((0x10, 0x4F8), []) + pairs.get((0x80, 0x1550), [])
    if not anchors:
        return {}
    best = max(anchors, key=lambda a: sum(1 for b in anchors if abs(a - b) < window))
    out = {}
    for (disp, stride), sites in pairs.items():
        if any(abs(s - best) < window for s in sites):
            out.setdefault(disp, set()).add(stride)
    return {d: sorted(v) for d, v in sorted(out.items())}


def report(img: Image, pairs, max_sites: int = 6):
    rows = []
    for (disp, stride), sites in sorted(pairs.items()):
        known = KNOWN.get(disp)
        rows.append({
            "ownerOffset": f"0x{disp:X}",
            "stride": f"0x{stride:X}",
            "sites": len(sites),
            "examples": [f"0x{s:X}" for s in sorted(sites)[:max_sites]],
            "known": known[1] if known and known[0] == stride else None,
        })
    missing = [f"{v[1]} 0x{k:X}/0x{v[0]:X}" for k, v in KNOWN.items() if (k, v[0]) not in pairs]
    fam = family(pairs)
    return {
        "stamp": f"0x{img.stamp:X}",
        "sizeOfImage": f"0x{img.size_of_image:X}",
        "expectedBuild": img.stamp == EXPECTED_STAMP and img.size_of_image == EXPECTED_IMAGE_SIZE,
        "knownDirectoriesFound": not missing,
        "missingKnown": missing,
        "family": {f"0x{d:X}": [f"0x{x:X}" for x in v] for d, v in fam.items()},
        "familyConflicts": [f"0x{d:X}" for d, v in fam.items() if len(v) > 1],
        "serviceAnchors": check_anchors(img),
        "directories": rows,
    }


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("exe")
    ap.add_argument("--json")
    ap.add_argument("--max-disp", type=lambda s: int(s, 0), default=0x800)
    ap.add_argument("--min-sites", type=int, default=1)
    a = ap.parse_args(argv)
    img = Image(pathlib.Path(a.exe).read_bytes())
    out = report(img, scan(img, a.max_disp))
    out["directories"] = [r for r in out["directories"] if r["sites"] >= a.min_sites]
    text = json.dumps(out, indent=2)
    if a.json:
        pathlib.Path(a.json).write_text(text + "\n", encoding="utf-8")
    print(text)
    anchors_ok = all(a["ok"] for a in out["serviceAnchors"])
    return 0 if out["expectedBuild"] and out["knownDirectoriesFound"] and anchors_ok else 1


if __name__ == "__main__":
    sys.exit(main())
