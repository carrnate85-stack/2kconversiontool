"""Disk-only audit of all worker method hashes, typed tables and ASLR safety.

No process APIs or output-file writes. Uses standard library only.
"""
import hashlib
import json
import pathlib
import re
import struct
import sys

path = pathlib.Path(sys.argv[1])
source = pathlib.Path(__file__).resolve().parent.parent / "native" / "outfit_match_v2_profile.hpp"
proof = source.read_text(encoding="utf-8")
methods = [(int(r, 16), int(n), h) for r, n, h in re.findall(
    r'\{0x([0-9a-fA-F]+)(?:ull)?,\s*(\d+)u?,\s*"([0-9A-F]{64})"\}', proof)]
tables = [(int(t, 16), int(s, 16), int(m, 16)) for t, s, m in re.findall(
    r'\{0x([0-9a-fA-F]+)ull,\s*0x([0-9a-fA-F]+)ull,\s*0x([0-9a-fA-F]+)ull\}', proof)]
assert len(methods) == 29 and len(tables) == 32
with path.open("rb") as stream:
    sha = hashlib.file_digest(stream, "sha256").hexdigest().upper()
    assert sha == "29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1"
    assert stream.seek(0, 2) == 870548760
    stream.seek(0)
    header = stream.read(4096)
    assert header[:2] == b"MZ"
    pe = struct.unpack_from("<I", header, 60)[0]
    assert header[pe:pe+4] == b"PE\0\0"
    machine, count, timestamp = struct.unpack_from("<HHI", header, pe+4)
    optional = pe+24
    assert machine == 0x8664 and timestamp == 0x6AC53765
    assert struct.unpack_from("<H", header, optional)[0] == 0x20B
    base = struct.unpack_from("<Q", header, optional+24)[0]
    assert base == 0x140000000
    assert struct.unpack_from("<I", header, optional+56)[0] == 0x35BDE000
    optional_bytes = struct.unpack_from("<H", header, pe+20)[0]
    stream.seek(optional+optional_bytes)
    sections = []
    for _ in range(count):
        section = stream.read(40)
        virtual_size, rva, raw_size, offset = struct.unpack_from("<IIII", section, 8)
        sections.append((rva, max(virtual_size, raw_size), offset, raw_size))

    def read_rva(rva, size):
        for start, length, raw, raw_size in sections:
            if start <= rva and rva + size <= start + raw_size:
                stream.seek(raw + rva-start)
                data = stream.read(size)
                assert len(data) == size
                return data
        raise AssertionError(f"Unbacked bounded RVA {rva:X}")

    for rva, size, expected in methods:
        assert hashlib.sha256(read_rva(rva, size)).hexdigest().upper() == expected
    for table, slot, method in tables:
        assert struct.unpack("<Q", read_rva(table+slot, 8))[0] == base+method
    assert struct.unpack("<Q", read_rva(0x6884898+0xA0, 8))[0] == base+0x6FD8C0
    reloc_rva, reloc_size = struct.unpack_from("<II", header, optional+112+5*8)
    relocations = read_rva(reloc_rva, reloc_size)
    at = 0
    while at < len(relocations):
        page, block_size = struct.unpack_from("<II", relocations, at)
        assert block_size >= 8 and not block_size % 2 and at+block_size <= len(relocations)
        for offset in range(at+8, at+block_size, 2):
            entry = struct.unpack_from("<H", relocations, offset)[0]
            if entry >> 12:
                target = page+(entry & 0xFFF)
                assert all(not (rva <= target < rva+size) for rva, size, _ in methods)
        at += block_size
print(json.dumps({"schema": "outfit-match-v2-native-disk-audit-v1", "sha256": sha,
                  "methodBodies": len(methods), "virtualSlots": len(tables)+1,
                  "relocationsInsideMethods": 0, "gameProcessAccess": False,
                  "gameWritten": False, "result": "passed"}))
