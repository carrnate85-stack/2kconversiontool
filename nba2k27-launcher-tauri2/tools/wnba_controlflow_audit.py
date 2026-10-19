"""Offline-only Version 2 WNBA roster selector control-flow audit.

This helper opens NBA2K27.exe as an ordinary file.  It never opens a process,
writes game files, or executes game code.  It exists to make the reverse-
engineering evidence behind the WNBA Freestyle fail-closed policy reproducible.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import struct
import sys

VENDORED = pathlib.Path(__file__).with_name("python-packages")
sys.path.insert(0, str(VENDORED))

import capstone  # type: ignore  # vendored by the research task
import pefile  # type: ignore  # vendored by the research task


SELECTORS = {
    "generic-selector-a": 0x234D640,
    "generic-selector-b": 0x236D330,
    "mode-branch": 0x363EDA4,
    "tku-normal": 0x236FB30,
    "tku-wnba": 0x236FF70,
    "tku-common": 0x2F1E0B0,
}
TEAMSELECT_MODE = 0xB1FBE68


def u32(data: bytes, at: int) -> int:
    return struct.unpack_from("<I", data, at)[0]


class Image:
    def __init__(self, path: pathlib.Path):
        self.path = path
        self.data = path.read_bytes()
        self.pe = pefile.PE(data=self.data, fast_load=True)
        self.pe.parse_data_directories(
            directories=[pefile.DIRECTORY_ENTRY["IMAGE_DIRECTORY_ENTRY_EXCEPTION"]]
        )
        self.base = int(self.pe.OPTIONAL_HEADER.ImageBase)
        self.sections = []
        for section in self.pe.sections:
            name = section.Name.rstrip(b"\0").decode("ascii", errors="replace")
            self.sections.append(
                {
                    "name": name,
                    "rva": int(section.VirtualAddress),
                    "virtual_size": int(section.Misc_VirtualSize),
                    "raw": int(section.PointerToRawData),
                    "raw_size": int(section.SizeOfRawData),
                    "characteristics": int(section.Characteristics),
                }
            )

    def section_for_rva(self, rva: int):
        for section in self.sections:
            size = max(section["virtual_size"], section["raw_size"])
            if section["rva"] <= rva < section["rva"] + size:
                return section
        raise ValueError(f"RVA 0x{rva:X} is not in a section")

    def offset(self, rva: int) -> int:
        section = self.section_for_rva(rva)
        delta = rva - section["rva"]
        if delta >= section["raw_size"]:
            raise ValueError(f"RVA 0x{rva:X} is not backed by file bytes")
        return section["raw"] + delta

    def bytes(self, rva: int, size: int) -> bytes:
        at = self.offset(rva)
        return self.data[at : at + size]

    def rva_for_offset(self, at: int) -> int | None:
        for section in self.sections:
            if section["raw"] <= at < section["raw"] + section["raw_size"]:
                return section["rva"] + at - section["raw"]
        return None

    def qword(self, rva: int) -> int:
        return struct.unpack("<Q", self.bytes(rva, 8))[0]

    def dword(self, rva: int) -> int:
        return struct.unpack("<I", self.bytes(rva, 4))[0]

    def executable_sections(self):
        return [s for s in self.sections if s["characteristics"] & 0x20000000]

    def function_containing(self, rva: int) -> tuple[int, int] | None:
        for entry in getattr(self.pe, "DIRECTORY_ENTRY_EXCEPTION", []):
            begin = int(entry.struct.BeginAddress)
            end = int(entry.struct.EndAddress)
            if begin <= rva < end:
                return (begin, end)
        return None


def decoder() -> capstone.Cs:
    md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_64)
    md.detail = True
    return md


def instruction_row(insn) -> dict:
    row = {
        "rva": insn.address,
        "bytes": insn.bytes.hex().upper(),
        "mnemonic": insn.mnemonic,
        "operands": insn.op_str,
    }
    targets = []
    for operand in insn.operands:
        if operand.type == capstone.x86.X86_OP_IMM:
            targets.append(int(operand.imm))
        elif (
            operand.type == capstone.x86.X86_OP_MEM
            and operand.mem.base == capstone.x86.X86_REG_RIP
        ):
            targets.append(int(insn.address + insn.size + operand.mem.disp))
    if targets:
        row["targets"] = targets
    return row


def disassemble(image: Image, rva: int, size: int) -> list[dict]:
    return [instruction_row(i) for i in decoder().disasm(image.bytes(rva, size), rva)]


def direct_calls(image: Image, targets: set[int]) -> dict[int, list[int]]:
    """Find exact x64 near-call rel32 references in one executable pass."""
    result = {target: [] for target in targets}
    for section in image.executable_sections():
        raw = image.data[section["raw"] : section["raw"] + section["raw_size"]]
        i = raw.find(b"\xE8")
        while 0 <= i <= len(raw) - 5:
            source = section["rva"] + i
            relative = struct.unpack_from("<i", raw, i + 1)[0]
            target = source + 5 + relative
            if target in result:
                result[target].append(source)
            i = raw.find(b"\xE8", i + 1)
    return result


def data_occurrences(image: Image, needle: bytes) -> list[int]:
    found = []
    start = 0
    while True:
        at = image.data.find(needle, start)
        if at < 0:
            break
        for section in image.sections:
            if section["raw"] <= at < section["raw"] + section["raw_size"]:
                found.append(section["rva"] + at - section["raw"])
                break
        start = at + 1
    return found


def absolute_pointer_occurrences(image: Image, target_rva: int) -> list[int]:
    return data_occurrences(image, struct.pack("<Q", image.base + target_rva))


def c_string(image: Image, rva: int, limit: int = 512) -> str | None:
    try:
        raw = image.bytes(rva, limit)
    except ValueError:
        return None
    end = raw.find(b"\0")
    if end < 0:
        return None
    try:
        return raw[:end].decode("ascii")
    except UnicodeDecodeError:
        return None


def msvc_vtable_owner(image: Image, entry_rva: int) -> dict | None:
    """Best-effort MSVC x64 RTTI owner for a vtable method entry."""
    # Walk backwards over contiguous image pointers and test each possible
    # vtable beginning.  A Version 1 CompleteObjectLocator stores image-relative
    # TypeDescriptor/ClassDescriptor/Self fields after the three leading words.
    for back in range(0, 0x1200, 8):
        start = entry_rva - back
        try:
            col_va = image.qword(start - 8)
        except (ValueError, struct.error):
            continue
        col = col_va - image.base
        try:
            signature = image.dword(col)
            type_rva = image.dword(col + 12)
            self_rva = image.dword(col + 20)
        except (ValueError, struct.error):
            continue
        if signature != 1 or self_rva != col:
            continue
        decorated = c_string(image, type_rva + 16)
        if not decorated or not decorated.startswith(".?"):
            continue
        return {
            "vtableRva": start,
            "slotOffset": back,
            "completeObjectLocatorRva": col,
            "typeDescriptorRva": type_rva,
            "decoratedType": decorated,
        }
    return None


def referenced_targets(rows: list[dict], targets: set[int]) -> list[dict]:
    return [row for row in rows if any(t in targets for t in row.get("targets", []))]


def roster_asset_inventory(image: Image) -> list[str]:
    found = set()
    for match in re.finditer(rb"roster/[A-Za-z0-9_./@-]{1,120}\.iff", image.data):
        found.add(match.group().decode("ascii"))
    return sorted(found)


def common_rip_references(image: Image, target: int) -> list[dict]:
    """Find exact refs for the common RIP-relative instruction forms in scope."""
    prefixes = [
        b"\x8B\x05",
        b"\x89\x05",
        b"\xC7\x05",
        b"\x44\x8B\x05",
        b"\x44\x89\x05",
        b"\x0F\xB6\x05",
        b"\x0F\xBE\x05",
        *[b"\x48\x8D" + bytes([modrm]) for modrm in (0x05, 0x0D, 0x15, 0x1D, 0x25, 0x2D, 0x35, 0x3D)],
        *[b"\x4C\x8D" + bytes([modrm]) for modrm in (0x05, 0x0D, 0x15, 0x1D, 0x25, 0x2D, 0x35, 0x3D)],
        *[b"\x48\x8B" + bytes([modrm]) for modrm in (0x05, 0x0D, 0x15, 0x1D, 0x25, 0x2D, 0x35, 0x3D)],
        *[b"\x48\x89" + bytes([modrm]) for modrm in (0x05, 0x0D, 0x15, 0x1D, 0x25, 0x2D, 0x35, 0x3D)],
        b"\x83\x3D",
        b"\x80\x3D",
        b"\xFF\x05",
        b"\xF0\xFF\x05",
    ]
    rows = []
    md = decoder()
    for section in image.executable_sections():
        raw = image.data[section["raw"] : section["raw"] + section["raw_size"]]
        for prefix in prefixes:
            at = raw.find(prefix)
            while 0 <= at <= len(raw) - len(prefix) - 4:
                source = section["rva"] + at
                disp = struct.unpack_from("<i", raw, at + len(prefix))[0]
                if source + len(prefix) + 4 + disp == target:
                    decoded = list(md.disasm(raw[at : at + 16], source, count=1))
                    if decoded:
                        row = instruction_row(decoded[0])
                        if target in row.get("targets", []):
                            span = image.function_containing(source)
                            row["functionRange"] = list(span) if span else None
                            start = span[0] if span else source
                            pointers = absolute_pointer_occurrences(image, start)
                            row["vtableOwners"] = [
                                owner
                                for entry in pointers
                                if (owner := msvc_vtable_owner(image, entry)) is not None
                            ]
                            rows.append(row)
                at = raw.find(prefix, at + 1)
    return sorted({row["rva"]: row for row in rows}.values(), key=lambda row: row["rva"])


def caller_graph(image: Image, seed: int, depth: int = 4) -> list[dict]:
    pending = {seed}
    seen = set()
    levels = []
    for level in range(depth):
        targets = pending - seen
        if not targets:
            break
        seen.update(targets)
        calls = direct_calls(image, targets)
        rows = []
        next_pending = set()
        for target in sorted(targets):
            sites = calls[target]
            callers = []
            for site in sites:
                span = image.function_containing(site)
                start = span[0] if span else site
                next_pending.add(start)
                pointers = absolute_pointer_occurrences(image, start)
                callers.append(
                    {
                        "callSite": site,
                        "functionRange": list(span) if span else None,
                        "absolutePointerOccurrences": pointers,
                        "vtableOwners": [
                            owner
                            for entry in pointers
                            if (owner := msvc_vtable_owner(image, entry)) is not None
                        ],
                    }
                )
            rows.append({"target": target, "callers": callers})
        levels.append({"level": level, "targets": rows})
        pending = next_pending
    return levels


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("exe", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path)
    parser.add_argument("--window", type=lambda v: int(v, 0), default=0x180)
    args = parser.parse_args()

    image = Image(args.exe)
    strings = {}
    for text in [
        "roster/roster.iff",
        "roster/wnba_roster.iff",
        "roster/learn2k_roster.iff",
        "roster/learn2k_wnba_roster.iff",
    ]:
        matches = sorted(
            set(data_occurrences(image, text.encode("ascii") + b"\0"))
            | set(data_occurrences(image, text.encode("utf-16-le") + b"\0\0"))
        )
        strings[text] = matches

    string_targets = {value for values in strings.values() for value in values}
    functions = {
        rva: image.function_containing(rva) for rva in SELECTORS.values()
    }
    call_targets = set(SELECTORS.values()) | {
        span[0] for span in functions.values() if span is not None
    }
    calls = direct_calls(image, call_targets)
    asset_references = {
        text: [
            row
            for target in targets
            for row in common_rip_references(image, target)
        ]
        for text, targets in strings.items()
    }
    normal_ranges = {
        tuple(row["functionRange"])
        for row in asset_references["roster/roster.iff"]
        if row.get("functionRange")
    }
    wnba_ranges = {
        tuple(row["functionRange"])
        for row in asset_references["roster/wnba_roster.iff"]
        if row.get("functionRange")
    }
    report = {
        "executable": str(args.exe.resolve()),
        "imageBase": image.base,
        "timestamp": int(image.pe.FILE_HEADER.TimeDateStamp),
        "sizeOfImage": int(image.pe.OPTIONAL_HEADER.SizeOfImage),
        "selectors": {},
        "assetStrings": strings,
        "assetStringRipReferences": asset_references,
        "dualNormalWnbaAssetFunctions": [
            {
                "functionRange": list(span),
                "instructions": disassemble(image, span[0], span[1] - span[0]),
            }
            for span in sorted(normal_ranges & wnba_ranges)
        ],
        "rosterAssetInventory": roster_asset_inventory(image),
        "assetStringReferencesWithinAuditedWindows": [],
        "modeBranchCallerGraph": caller_graph(
            image,
            (functions[SELECTORS["mode-branch"]] or (SELECTORS["mode-branch"], 0))[0],
        ),
        "teamSelectModeGlobal": {
            "rva": TEAMSELECT_MODE,
            "commonRipReferences": common_rip_references(image, TEAMSELECT_MODE),
        },
    }
    for name, rva in SELECTORS.items():
        function = functions[rva]
        function_start = function[0] if function else rva
        callers = sorted(set(calls[rva]) | set(calls[function_start]))
        instructions = disassemble(
            image,
            function[0] if function else rva,
            (function[1] - function[0]) if function else args.window,
        )
        caller_windows = {
            f"0x{caller:X}": disassemble(image, max(0, caller - 0x40), 0xA0)
            for caller in callers
        }
        report["selectors"][name] = {
            "rva": rva,
            "functionRange": list(function) if function else None,
            "directCallers": callers,
            "absolutePointerOccurrences": absolute_pointer_occurrences(image, rva),
            "instructions": instructions,
            "callerWindows": caller_windows,
        }
        report["selectors"][name]["vtableOwners"] = [
            owner
            for entry in report["selectors"][name]["absolutePointerOccurrences"]
            if (owner := msvc_vtable_owner(image, entry)) is not None
        ]
        report["assetStringReferencesWithinAuditedWindows"].extend(
            referenced_targets(instructions, string_targets)
        )
        for rows in caller_windows.values():
            report["assetStringReferencesWithinAuditedWindows"].extend(
                referenced_targets(rows, string_targets)
            )

    rendered = json.dumps(report, indent=2)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered + "\n", encoding="utf-8")
    else:
        print(rendered)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
