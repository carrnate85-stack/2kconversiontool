"""Pure offline one-entry/two-alias WAD metadata; no game/process or key access.

The returned metadata describes a separate payload file; callers must provide
the already established archive key, decoded size and native location flags.
This module neither reads nor copies the payload and does not register a WAD.
Native evidence (pinned V2 executable): Init 0x5200BA0 reads a 48-byte header,
48-byte descriptors and 24-byte sorted index records; +8 is the file size unit;
0x52012A2 checks descriptor units * header unit against actual file size.
0x5205440 extracts position as location >> 23. Unknown flags are preserved,
never inferred from an arbitrary extension. Runtime compatibility is untested.
"""
from __future__ import annotations

import struct

MAGIC = 0xAA00B3BF
HEADER_BYTES = 48
DESCRIPTOR_BYTES = 48
INDEX_BYTES = 24
MANIFEST_BYTES = HEADER_BYTES + DESCRIPTOR_BYTES + INDEX_BYTES
LOCATION_FLAGS_MASK = (1 << 23) - 1


def _integer(name: str, value: int, low: int, high: int) -> int:
    if type(value) is not int or not low <= value <= high:
        raise ValueError(f'{name} must be an integer from {low} through {high}')
    return value


def _filename(name: str) -> bytes:
    # The last dword of the native descriptor becomes a handle tag. Keep the
    # NUL terminator before it; do not rely on unproven long-name behavior.
    if not isinstance(name, str) or not name or name in ('.', '..'):
        raise ValueError('payload name must be a plain ASCII filename')
    try:
        result = name.encode('ascii')
    except UnicodeEncodeError as error:
        raise ValueError('payload name must be ASCII') from error
    if len(result) > 27 or any(c < 32 or c in b'\\/:*?"<>|' for c in result):
        raise ValueError('payload name must be a plain ASCII filename of at most 27 bytes')
    if name[-1] in ' .':
        raise ValueError('payload name must not end in a space or period')
    return result


def build_manifest(*, key: int, decoded_size: int, packed_size: int,
                   location_flags: int, payload_name: str, packed_flag: int = 0) -> bytes:
    """Describe one already encoded payload located at offset zero in its file.

    location_flags must come from a validated compatible stock index; this
    function does not claim that arbitrary wrapper/compression flags are valid.
    The 33-bit decoded field is native getter 0x52017E0, and the packed field
    begins at bit 33 (getter 0x52017C0). Its bit 32 is packed_flag.
    """
    key = _integer('key', key, 0, (1 << 64) - 1)
    decoded_size = _integer('decoded size', decoded_size, 1, (1 << 32) - 1)
    packed_size = _integer('packed size', packed_size, 1, (1 << 31) - 1)
    location_flags = _integer('location flags', location_flags, 0, LOCATION_FLAGS_MASK)
    packed_flag = _integer('packed flag', packed_flag, 0, 1)
    name = _filename(payload_name)
    out = bytearray(MANIFEST_BYTES)
    struct.pack_into('<I', out, 0, MAGIC)
    struct.pack_into('<Q', out, 8, 1)  # Byte-sized units, as installed 0A.
    struct.pack_into('<I', out, 16, 1)
    struct.pack_into('<I', out, 32, 1)
    struct.pack_into('<Q', out, 48, packed_size)
    out[64:64 + len(name)] = name
    struct.pack_into('<QIIQ', out, 96, key, decoded_size,
                     (packed_size << 1) | packed_flag, location_flags)
    return bytes(out)


def parse_manifest(data: bytes) -> dict:
    """Strictly parse this minimal fixture form, rejecting extra metadata."""
    if len(data) != MANIFEST_BYTES:
        raise ValueError('private one-entry manifest must be exactly 120 bytes')
    if struct.unpack_from('<I', data)[0] != MAGIC:
        raise ValueError('unexpected WAD magic')
    if struct.unpack_from('<Q', data, 8)[0] != 1:
        raise ValueError('private manifest must use byte units')
    if struct.unpack_from('<I', data, 16)[0] != 1 or struct.unpack_from('<I', data, 32)[0] != 1:
        raise ValueError('private manifest must have one file and one entry')
    name_field = data[64:92]
    try:
        nul = name_field.index(0)
        name = name_field[:nul].decode('ascii')
    except (ValueError, UnicodeDecodeError) as error:
        raise ValueError('invalid private payload filename') from error
    key, decoded, packed, location = struct.unpack_from('<QIIQ', data, 96)
    physical_size = struct.unpack_from('<Q', data, 48)[0]
    if physical_size != packed >> 1:
        raise ValueError('payload size differs from index size')
    if location >> 23:
        raise ValueError('private payload must start at zero')
    result = dict(key=key, decoded_size=decoded, packed_size=packed >> 1,
                  location_flags=location & LOCATION_FLAGS_MASK,
                  payload_name=name, packed_flag=packed & 1)
    if build_manifest(**result) != data:
        raise ValueError('reserved fields or filename padding are nonzero')
    return result


def build_two_alias_manifest(*, alias_key: int, **fields) -> bytes:
    """Two sorted lookup keys referencing one identical offset-zero payload.

    This creates metadata only. It grants no native registration permission and
    deliberately leaves the existing one-entry builder/parser unchanged.
    """
    alias_key = _integer('alias key', alias_key, 0, (1 << 64) - 1)
    first = build_manifest(**fields)
    primary = struct.unpack_from('<Q', first, 96)[0]
    if primary == alias_key:
        raise ValueError('alias key must differ from primary key')
    record = first[96:120]
    alias = struct.pack('<Q', alias_key) + record[8:]
    result = bytearray(first[:96])
    struct.pack_into('<I', result, 32, 2)
    result.extend(b''.join(sorted((record, alias), key=lambda item: struct.unpack_from('<Q', item)[0])))
    return bytes(result)


def parse_two_alias_manifest(data: bytes) -> dict:
    """Strictly parse exactly two aliases sharing the same retained payload."""
    if len(data) != 144 or struct.unpack_from('<I', data, 32)[0] != 2:
        raise ValueError('two-alias manifest must be exactly 144 bytes with two entries')
    if data[104:120] != data[128:144]:
        raise ValueError('alias payload metadata must match exactly')
    first = bytearray(data[:120])
    struct.pack_into('<I', first, 32, 1)
    fields = parse_manifest(bytes(first))
    alias_key = struct.unpack_from('<Q', data, 120)[0]
    result = {**fields, 'alias_key': alias_key}
    if build_two_alias_manifest(**result) != data:
        raise ValueError('aliases must be unique and sorted by lookup key')
    return result
