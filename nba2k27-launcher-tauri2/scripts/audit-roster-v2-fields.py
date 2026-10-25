"""Developer-only bounded PE/Capstone analysis; no game process or writes.

Track record-bit provenance through the named PLAYER serializer. Extraction
does not consult the expected field offsets; those are compared afterwards.
Unsupported instructions destroy provenance rather than guessing.
"""
from __future__ import annotations
import argparse
import bisect
from dataclasses import dataclass, replace
import hashlib
import json
from pathlib import Path
import re
import struct
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT.parent / "research/python_modules"))
import pefile
from capstone import Cs, CS_ARCH_X86, CS_MODE_64
from capstone.x86 import X86_OP_REG, X86_OP_MEM, X86_OP_IMM, X86_REG_RDI, X86_REG_RIP

IMAGE_SHA = "29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1"
START, END = 0x6AE330, 0x6BA7CB
GET_PROPERTY = 0x4BEC9A0
VALUE_WRITERS = {0x4BED070: "signed", 0x4BED170: "unsigned", 0x4BED210: "boolean", 0x4BED300: "enum"}
FORMATTER = 0x4B8E7E0

@dataclass(frozen=True)
class Bits:
    offset: int
    shift: int
    mask: int

    def trim(self, width):
        return replace(self, mask=self.mask & ((1 << width) - 1))

    def shr(self, amount):
        return replace(self, shift=self.shift + amount, mask=self.mask >> amount)

    def shl(self, amount, width):
        return replace(self, shift=self.shift - amount, mask=(self.mask << amount) & ((1 << width) - 1))

@dataclass(frozen=True)
class EnumValue:
    bits: Bits
    table: int

@dataclass(frozen=True)
class StackRef:
    offset: int

@dataclass(frozen=True)
class RecordRef:
    offset: int

def register(name):
    known = {
        "al": ("rax", 8, 0), "ah": ("rax", 8, 8), "ax": ("rax", 16, 0), "eax": ("rax", 32, 0),
        "bl": ("rbx", 8, 0), "bh": ("rbx", 8, 8), "bx": ("rbx", 16, 0), "ebx": ("rbx", 32, 0),
        "cl": ("rcx", 8, 0), "ch": ("rcx", 8, 8), "cx": ("rcx", 16, 0), "ecx": ("rcx", 32, 0),
        "dl": ("rdx", 8, 0), "dh": ("rdx", 8, 8), "dx": ("rdx", 16, 0), "edx": ("rdx", 32, 0),
        "bpl": ("rbp", 8, 0), "bp": ("rbp", 16, 0), "ebp": ("rbp", 32, 0),
        "sil": ("rsi", 8, 0), "si": ("rsi", 16, 0), "esi": ("rsi", 32, 0),
        "dil": ("rdi", 8, 0), "di": ("rdi", 16, 0), "edi": ("rdi", 32, 0),
        "spl": ("rsp", 8, 0), "sp": ("rsp", 16, 0), "esp": ("rsp", 32, 0),
    }
    if name in known:
        return known[name]
    if match := re.fullmatch(r"(r\d+)([dwb])", name):
        return match[1], {"d": 32, "w": 16, "b": 8}[match[2]], 0
    return name, 64, 0

def extract(pe, md, instructions):
    state = {"rdi": RecordRef(0)}
    stack = {}
    stack_widths = {}
    formatted = {}
    active = None
    results = []
    named = []
    def value(reg_id):
        parent, width, low = register(md.reg_name(reg_id))
        current = state.get(parent)
        if isinstance(current, Bits):
            return current.shr(low).trim(width)
        if isinstance(current, int):
            return (current >> low) & ((1 << width) - 1)
        return current if low == 0 and width == 64 else None

    def assign(reg_id, current):
        parent, width, low = register(md.reg_name(reg_id))
        if low or width < 32:
            previous = state.get(parent)
            # A zero-extended byte load followed by byte-only operations can
            # preserve the full register only when its upper bits are proven0.
            if low == 0 and isinstance(previous, Bits) and previous.mask < 1 << width and isinstance(current, Bits):
                state[parent] = current.trim(width)
            else:
                state.pop(parent, None)
        elif current is None:
            state.pop(parent, None)
        elif isinstance(current, Bits):
            state[parent] = current.trim(width)
        elif isinstance(current, int):
            state[parent] = current & ((1 << width) - 1)
        else:
            state[parent] = current

    for index, ins in enumerate(instructions):
        ops = ins.operands
        if ins.mnemonic == "call":
            target = ops[0].imm if len(ops) == 1 and ops[0].type == X86_OP_IMM else None
            if target == FORMATTER:
                output, count, pattern = state.get("rcx"), state.get("r9"), state.get("r8")
                if isinstance(output, StackRef):
                    formatted.pop(output.offset, None)
                args_ref = stack.get(0x20)
                arg_ref = stack.get(args_ref.offset) if isinstance(args_ref, StackRef) else None
                arg = stack.get(arg_ref.offset) if isinstance(arg_ref, StackRef) else None
                pattern_bytes = pe.get_data(pattern, 8) if isinstance(pattern, int) else b""
                if isinstance(output, StackRef) and count == 1 and isinstance(arg, Bits) and pattern_bytes == b"{\x000\x00}\x00\x00\x00":
                    formatted[output.offset] = arg
            if target == 0x6AA040 and active and isinstance(state.get("r8"), RecordRef):
                results.append({"key": active[0], "nameRva": active[1], "writeRva": ins.address, "writer": "utf16", "bits": state["r8"], "enumTable": None,
                                "context": [f"{r.address:08X} {r.mnemonic} {r.op_str}" for r in instructions[max(0, active[2]-10):index+1]]})
                active = None
            if target in VALUE_WRITERS and active:
                result = value(md.reg_name2id("rdx")) if hasattr(md, "reg_name2id") else state.get("rdx")
                if isinstance(result, StackRef):
                    result = formatted.get(result.offset)
                if isinstance(result, EnumValue):
                    bits, table = result.bits, result.table
                else:
                    bits, table = result, None
                results.append({"key": active[0], "nameRva": active[1], "writeRva": ins.address,
                                "writer": VALUE_WRITERS[target], "bits": bits, "enumTable": table,
                                "context": [f"{r.address:08X} {r.mnemonic} {r.op_str}" for r in instructions[max(0, active[2]-10):index+1]]})
                active = None
            for name in ("rax", "rcx", "rdx", "r8", "r9", "r10", "r11"):
                state.pop(name, None)
            continue
        if ops and ops[0].type == X86_OP_MEM and ops[0].mem.base and md.reg_name(ops[0].mem.base) == "rsp" and not ops[0].mem.index:
            address, size = ops[0].mem.disp, ops[0].size
            for slot in list(stack):
                if slot < address + size and address < slot + stack_widths[slot]:
                    stack.pop(slot, None)
                    stack_widths.pop(slot, None)
            for slot in list(formatted):
                if slot < address + size and address < slot + 8:
                    formatted.pop(slot, None)
            if ins.mnemonic == "mov" and len(ops) == 2:
                if ops[1].type == X86_OP_REG:
                    stack[address] = value(ops[1].reg)
                elif ops[1].type == X86_OP_IMM:
                    stack[address] = ops[1].imm
                if address in stack:
                    stack_widths[address] = size
        if not ops or ops[0].type != X86_OP_REG:
            continue
        dest = ops[0].reg
        parent, width, _ = register(md.reg_name(dest))
        if ins.mnemonic == "lea" and len(ops) == 2 and ops[1].type == X86_OP_MEM:
            mem = ops[1].mem
            address = ins.address + ins.size + mem.disp if mem.base == X86_REG_RIP else StackRef(mem.disp) if mem.base and md.reg_name(mem.base) == "rsp" and not mem.index else RecordRef(mem.disp) if mem.base == X86_REG_RDI and not mem.index else None
            if isinstance(address, int) and parent == "rdx":
                try:
                    name = pe.get_data(address, 128).split(b"\0", 1)[0].decode("ascii")
                except (UnicodeError, pefile.PEFormatError):
                    name = ""
                if re.fullmatch(r"[A-Za-z][A-Za-z0-9_]*", name):
                    active = (name, ins.address, index)
                    named.append((name, ins.address))
            assign(dest, address)
            continue
        if ins.mnemonic in ("mov", "movzx", "movsxd", "movsx") and len(ops) == 2:
            source = ops[1]
            result = None
            if source.type == X86_OP_IMM:
                result = source.imm
            elif source.type == X86_OP_REG:
                result = value(source.reg)
                if ins.address == START + 0x3c and ins.mnemonic == "mov" and md.reg_name(dest) == "rdi" and md.reg_name(source.reg) == "rdx":
                    result = RecordRef(0)
            elif source.type == X86_OP_MEM:
                mem = source.mem
                if mem.base == X86_REG_RDI and not mem.index:
                    result = Bits(mem.disp, 0, (1 << (source.size * 8)) - 1)
                elif mem.base == X86_REG_RIP:
                    result = None
                else:
                    a = value(mem.base) if mem.base else 0
                    b = value(mem.index) if mem.index else 0
                    if isinstance(a, int) and isinstance(b, Bits) and mem.scale in (1, 2, 4, 8):
                        byte_shift = 3 - (mem.scale.bit_length() - 1)
                        if b.mask & ((1 << byte_shift) - 1) == 0:
                            result = EnumValue(b.shr(byte_shift), a + mem.disp)
                    elif isinstance(a, Bits) and isinstance(b, int) and mem.scale == 1 and a.mask & 7 == 0:
                        result = EnumValue(a.shr(3), b + mem.disp)
            assign(dest, result)
            continue
        if ins.mnemonic in ("and", "shr", "sar", "shl") and len(ops) == 2:
            current = value(dest)
            source = ops[1]
            result = None
            if source.type == X86_OP_IMM:
                amount = source.imm
                if isinstance(current, Bits):
                    if ins.mnemonic == "and":
                        result = replace(current, mask=current.mask & (amount & ((1 << width) - 1)))
                    elif ins.mnemonic == "shl":
                        result = current.shl(amount, width)
                    else:
                        result = current.shr(amount)
                elif isinstance(current, int):
                    if ins.mnemonic == "and": result = current & amount
                    elif ins.mnemonic == "shl": result = (current << amount) & ((1 << width) - 1)
                    else: result = current >> amount
            elif ins.mnemonic == "and" and isinstance(current, int) and source.type == X86_OP_MEM and source.mem.base == X86_REG_RDI and not source.mem.index:
                result = Bits(source.mem.disp, 0, current & ((1 << (source.size * 8)) - 1))
            assign(dest, result)
            continue
        if ins.mnemonic == "add" and len(ops) == 2 and ops[1].type == X86_OP_REG and ops[1].reg == dest:
            current = value(dest)
            assign(dest, current.shl(1, width) if isinstance(current, Bits) else current * 2 if isinstance(current, int) else None)
            continue
        if ins.mnemonic == "xor" and len(ops) == 2 and ops[1].type == X86_OP_REG and ops[1].reg == dest:
            assign(dest, 0)
            continue
        if ins.mnemonic not in ("test", "cmp", "push", "nop"):
            # Never let unmodelled register writes retain a stale provenance.
            assign(dest, None)
    return results, named

def direct_callers(pe, md, target):
    section = next(s for s in pe.sections if s.Name.rstrip(b"\0") == b".text" and s.Characteristics == 0x60000020)
    code, code_rva = section.get_data(), section.VirtualAddress
    pdata = next(s for s in pe.sections if s.Name.rstrip(b"\0") == b".pdata").get_data()
    functions = [(start, end) for start, end, _ in struct.iter_unpack("<III", pdata[:len(pdata)//12*12]) if start and end > start]
    starts = [f[0] for f in functions]
    output, index = [], 0
    while True:
        index = code.find(b"\xe8", index)
        if index < 0 or index + 5 > len(code): break
        rva = code_rva + index
        if rva + 5 + struct.unpack_from("<i", code, index+1)[0] == target:
            candidate = bisect.bisect_right(starts, rva) - 1
            if candidate >= 0 and functions[candidate][0] <= rva < functions[candidate][1]:
                start, end = functions[candidate]
                # Function bounds avoid inventing mid-instruction context.
                insns = list(md.disasm(pe.get_data(start, min(end-start, 0x10000)), start))
                position = next((i for i, row in enumerate(insns) if row.address == rva and row.mnemonic == "call"), None)
                if position is not None:
                    output.append({"functionRva": start, "callRva": rva, "context": [f"{row.address:08X} {row.mnemonic} {row.op_str}" for row in insns[max(0,position-45):position+16]]})
        index += 1
    return output

def named_references(pe, names):
    data_section = next(s for s in pe.sections if s.Name.rstrip(b"\0") == b".rdata")
    code_section = next(s for s in pe.sections if s.Name.rstrip(b"\0") == b".text" and s.Characteristics == 0x60000020)
    data, code = data_section.get_data(), code_section.get_data()
    strings = {}
    for name in names:
        pattern, index = name.encode("ascii")+b"\0", 0
        while True:
            index = data.find(pattern,index)
            if index < 0: break
            if index == 0 or data[index-1] == 0: strings[data_section.VirtualAddress+index] = name
            index += 1
    output, index = [], 0
    while True:
        index = code.find(b"\x48\x8d\x15", index)
        if index < 0 or index+7 > len(code): break
        rva = code_section.VirtualAddress+index
        target = rva+7+struct.unpack_from("<i",code,index+3)[0]
        if target in strings: output.append({"key":strings[target],"instructionRva":rva,"stringRva":target})
        index += 1
    return output

def shape(bits):
    if not isinstance(bits, Bits) or bits.shift < 0 or bits.mask < 1 or bits.mask & (bits.mask + 1):
        return None
    return {"offset": bits.offset, "shift": bits.shift, "bits": bits.mask.bit_length()}

def self_test():
    assert shape(Bits(0x440, 0, 0xffffffff).shr(24)) == {"offset": 0x440, "shift": 24, "bits": 8}
    assert shape(Bits(0x1a8, 0, 0xffffffff).shr(1).trim(32).shl(0, 32)) is not None
    assert shape(Bits(0x1a8, 1, 0x78).shr(3)) == {"offset": 0x1a8, "shift": 4, "bits": 4}
    assert register("bh") == ("rbx", 8, 8)
    assert register("r14d") == ("r14", 32, 0)
    assert shape(Bits(0x100, 0, 0x55)) is None
    md = Cs(CS_ARCH_X86, CS_MODE_64); md.detail = True
    class FixturePE:
        def get_data(self, address, length):
            if address == 0x9000: return b"FixtureField\0"
            if address == 0xa000: return b"{\x000\x00}\x00\x00\x00"
            if address == 0xa100: return b"{\x001\x00}\x00\x00\x00"
            return b""
    def fixture(load, transform=b"", unknown=b"", to_argument=b"\x89\xea", writer=0x4BED170):
        code = bytearray(load)
        def relative(prefix, target):
            rva = 0x1000 + len(code)
            code.extend(prefix + struct.pack("<i", target-rva-len(prefix)-4))
        relative(b"\x48\x8d\x15", 0x9000)
        relative(b"\xe8", GET_PROPERTY)
        code.extend(transform + unknown + to_argument)
        relative(b"\xe8", writer)
        output, _ = extract(FixturePE(), md, list(md.disasm(code, 0x1000)))
        assert len(output) == 1
        return shape(output[0]["bits"])
    assert fixture(b"\x0f\xb6\xaf\x3c\x04\x00\x00") == {"offset":0x43c,"shift":0,"bits":8}
    assert fixture(b"\x0f\xb6\xaf\x3d\x04\x00\x00") == {"offset":0x43d,"shift":0,"bits":8}
    assert fixture(b"\x0f\xb6\xaf\xd6\x02\x00\x00", b"\x40\x80\xe5\x20\x40\xc0\xed\x05") == {"offset":0x2d6,"shift":5,"bits":1}
    assert fixture(b"\x8b\xaf\xd6\x02\x00\x00", b"\x40\x80\xe5\x20\x40\xc0\xed\x05") is None
    assert fixture(b"\x0f\xb6\xaf\x3c\x04\x00\x00", unknown=b"\x6b\xed\x02") is None
    assert fixture(b"\x8b\xaf\x40\x04\x00\x00", b"\xc1\xed\x18") == {"offset":0x440,"shift":24,"bits":8}
    assert fixture(b"\x8b\xaf\x40\x04\x00\x00", b"\xc1\xed\x17") == {"offset":0x440,"shift":23,"bits":9}
    enum = b"\x8b\x87\x40\x04\x00\x00\xc1\xe8\x18\x83\xe0\x03\xbb" + struct.pack("<I",0x6800000)
    assert fixture(enum+b"\x48\x8b\x2c\xc3", to_argument=b"\x48\x89\xea", writer=0x4BED300) == {"offset":0x440,"shift":24,"bits":2}
    assert fixture(enum+b"\x48\x8b\x2c\x83", to_argument=b"\x48\x89\xea", writer=0x4BED300) is None
    assert fixture(enum+b"\x31\xc0\x48\x8b\x2c\xc3", to_argument=b"\x48\x89\xea", writer=0x4BED300) is None
    def format_program(offset=0x188, count=1, pattern=0xa000, corrupt=b""):
        code=bytearray(b"\x8b\x87"+struct.pack("<i",offset)+b"\x89\x44\x24\x28\x48\x8d\x44\x24\x28\x48\x89\x44\x24\x30\x48\x8d\x44\x24\x30\x48\x89\x44\x24\x20")
        code.extend(corrupt+b"\x41\xb8"+struct.pack("<I",pattern)+b"\x48\x8d\x4c\x24\x50\x41\xb9"+struct.pack("<I",count))
        at=0x1000+len(code);code.extend(b"\xe8"+struct.pack("<i",0x4B8E7E0-at-5))
        return bytes(code)
    assert fixture(format_program(),to_argument=b"\x48\x8d\x54\x24\x50",writer=0x4BED300) == {"offset":0x188,"shift":0,"bits":32}
    assert fixture(format_program(offset=0x18c),to_argument=b"\x48\x8d\x54\x24\x50",writer=0x4BED300) == {"offset":0x18c,"shift":0,"bits":32}
    assert fixture(format_program(count=2),to_argument=b"\x48\x8d\x54\x24\x50",writer=0x4BED300) is None
    assert fixture(format_program(pattern=0xa100),to_argument=b"\x48\x8d\x54\x24\x50",writer=0x4BED300) is None
    # This overwrites the upper byte of the saved pointer. Partial stack-slot
    # overlap must destroy provenance, even when its starting offset differs.
    assert fixture(format_program(corrupt=b"\xc6\x44\x24\x34\x00"),to_argument=b"\x48\x8d\x54\x24\x50",writer=0x4BED300) is None
    print(json.dumps({"pass": True, "checks": 21, "gameProcessAccess": False, "gameFilesChanged": False}))

def main():
    global IMAGE_SHA, END, GET_PROPERTY, VALUE_WRITERS, FORMATTER
    p = argparse.ArgumentParser()
    p.add_argument("image", type=Path, nargs="?")
    p.add_argument("--output", type=Path)
    p.add_argument("--asset", type=Path, help="Optional compact read-only runtime proof; no game files are modified")
    p.add_argument("--self-test", action="store_true")
    p.add_argument("--current-build", action="store_true", help="Audit the B673EB04 installed image using its named player-field span")
    args = p.parse_args()
    if args.self_test:
        self_test()
        return 0
    assert args.image is not None and args.output is not None
    if args.current_build:
        IMAGE_SHA = "B673EB04593D6D5A4DFB9E270178E04C017BD9F277514D256EA98FFB792ED27B"
        # Named player fields end before RDI is reused for the trailing map.
        END = 0x6BB820
        GET_PROPERTY = 0x4BECC20
        VALUE_WRITERS = {0x4BED2F0: "signed", 0x4BED3F0: "unsigned", 0x4BED490: "boolean", 0x4BED580: "enum"}
        FORMATTER = 0x4B8EA60
    # Read the file as a PE analysis input; never load its code or DLLs.
    with args.image.open("rb") as file:
        digest = hashlib.file_digest(file, "sha256").hexdigest().upper()
    assert digest == IMAGE_SHA, "Exact Version 2 disk image required"
    pe = pefile.PE(str(args.image), fast_load=True)
    stamp, size = (0x6AD2E48C, 0x36532000) if args.current_build else (0x6AC53765, 0x35BDE000)
    assert pe.FILE_HEADER.TimeDateStamp == stamp and pe.OPTIONAL_HEADER.SizeOfImage == size
    md = Cs(CS_ARCH_X86, CS_MODE_64)
    md.detail = True
    data = pe.get_data(START, END - START)
    instructions = list(md.disasm(data, START))
    assert instructions and instructions[-1].address + instructions[-1].size == END, "Serializer disassembly is incomplete"
    record_writes = [ins for ins in instructions if X86_REG_RDI in ins.regs_access()[1]]
    assert len(record_writes) == 1 and record_writes[0].address == START+0x3c and record_writes[0].mnemonic == "mov" and record_writes[0].op_str == "rdi, rdx", "Serializer record provenance changed"
    results, named = extract(pe, md, instructions)
    expected = json.loads((ROOT / "assets/roster-layout.json").read_text(encoding="utf8"))["fields"]
    rows, missing, mismatches = [], [], []
    for wanted in expected:
        found = [r for r in results if r["key"] == wanted["key"]]
        if len(found) != 1:
            missing.append({"key": wanted["key"], "matches": len(found), "namedReferences": [hex(rva) for key, rva in named if key == wanted["key"]]})
            continue
        row = found[0]
        extracted = shape(row["bits"])
        matches = extracted is not None and (extracted["offset"] * 8 + extracted["shift"], extracted["bits"]) == (wanted["offset"] * 8 + wanted["shift"], wanted["bits"])
        output = {key: value for key, value in row.items() if key not in ("bits", "context")}
        output.update({"extracted": extracted, "expected": {key: wanted[key] for key in ("offset", "shift", "bits")}, "matches": matches})
        if not matches:
            output["context"] = row["context"]
            mismatches.append(output)
        rows.append(output)
    metadata = [{"key": r["key"], "nameRva":r["nameRva"], "writeRva":r["writeRva"], "writer":r["writer"], "shape":shape(r["bits"]), "recordOffset":r["bits"].offset if isinstance(r["bits"],RecordRef) else None, "enumTable":r["enumTable"]} for r in results if r["key"] not in {f["key"] for f in expected} and (shape(r["bits"]) is not None or isinstance(r["bits"],RecordRef))]
    report = {"schema": "nba2k27-v2-player-bitfield-proof-v1", "imageSha256": IMAGE_SHA, "gameProcessAccess": False, "gameFilesChanged": False,
              "serializerRva": START, "serializerBytes": END - START, "serializerSha256": hashlib.sha256(data).hexdigest(),
              "expectedFields": len(expected), "matchedFields": sum(r["matches"] for r in rows), "missing": missing, "mismatches": mismatches, "rows": rows,
              "metadata":metadata, "uniformGetterCallers":direct_callers(pe,md,0x2EDCBD0 if args.current_build else 0x2EDCB20),
              "teamNamedReferences":named_references(pe,["TeamData","UnlocalizedName","TeamName","Name","CityName","CityNameShort","StateNameShort","UniqueId","RosterType","Arena"]),
              "candidateOnly": True, "writesSupported": False}
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf8")
    if args.asset:
        assert not missing and not mismatches and len(rows) == 455, "Incomplete proof cannot become an asset"
        compact = {key: report[key] for key in ("schema", "imageSha256", "serializerRva", "serializerBytes", "serializerSha256", "expectedFields", "matchedFields", "missing", "mismatches", "metadata", "writesSupported")}
        compact["generatorSha256"] = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
        compact["rows"] = [{key: row[key] for key in ("key", "nameRva", "writeRva", "writer", "enumTable", "extracted", "matches")} for row in rows]
        compact["readOnly"] = True
        args.asset.write_text(json.dumps(compact, separators=(",", ":")) + "\n", encoding="utf8")
    print(json.dumps({"matched": report["matchedFields"], "expected": len(expected), "missing": len(missing), "mismatches": len(mismatches), "output": str(args.output), "gameProcessAccess": False}))
    return 0 if not missing and not mismatches else 1

if __name__ == "__main__":
    sys.exit(main())
