"""Offline parser/writer for decoded NBA 2K27 V2 roster IFF (zip -> roster.RDAT).

Reverse-engineered 2026-10-17 from the four stock V2 rosters (roster, wnba_roster,
learn2k_roster, learn2k_wnba_roster) and the V2 loader (NBA2K27.exe 0x210EAE0).
Never touches a game process or game files.

  IFF   zip, one stored member 'roster.RDAT'.
  RDAT  u32le (len - 0x10), 12 bytes, 'BNH(' u32le (len - 0x18), 16 bytes, then
        'BNDL' chunks. Chunk = 'BNDL', u32le payloadLen, 0x34-byte header, payload;
        every chunk ('BNDL'..end) is padded with zeros to a multiple of 8 bytes.
  Header (13 x u32le): [0] table id, [1] dataLen, [2] auxLen, [3] w3,
        [4] auxCrc, [5] crcOffset, [6] crcLen, [7] dataCrc, [8..10] 0, [11] id2, [12] 0.
        dataCrc = ~crc32 state over payload[crcOffset:+crcLen] + SALT, i.e.
        zlib.crc32(section + SALT); auxCrc likewise over the aux section.
  Payload: dataLen bytes of MSB-first bit-packed rows, then auxLen bytes of
        schema (big-endian u32s): 19-word header (total row bits as u64, type hash,
        table id, row count at word 15) and 19-word field records
        (relOffset, width, type, typeHash, nameHash, parentIndex, index, isArray,
        ..., count at word 15). Type 21 = reference: 64 bits = u32 row index +
        u32 tag (0x10E7BC3A), all-ones = null. Type 0 = struct (members follow
        with parentIndex set, offsets relative to the parent).
  The first chunk (8c76b363) is the directory: u32be (table id, row count) pairs.
SALT is read from the installed executable (V2 RVA 0x73B5180, 22 bytes) and
checked against the stock CRCs; it is not stored in this repository.
"""
import io
import struct
import zipfile
import zlib

DIRECTORY = '8c76b363'
HEADER = 0x34
FIELD_RECORD = 76
REF_TYPE = 21
REF_TAG = 0x10E7BC3A
NULL_REF = (1 << 64) - 1
SALT_RVA = 0x73B5180
SALT_LEN = 0x16


def padded(n):
    """Chunk payload length for n content bytes (chunk total 8-aligned)."""
    return ((8 + HEADER + n + 7) & ~7) - 8 - HEADER


def get(row, bits, off, width):
    return (row >> (bits - off - width)) & ((1 << width) - 1)


def put(row, bits, off, width, value):
    shift = bits - off - width
    mask = ((1 << width) - 1) << shift
    return (row & ~mask) | ((value << shift) & mask)


def wide(row, bits, off, chars):
    s = ''.join(chr(get(row, bits, off + 16 * k, 16)) for k in range(chars))
    return s.split('\0')[0]


def pack_rows(rows, bits):
    if not rows:
        return b''
    nbits = len(rows) * bits
    out = bytearray((nbits + 7) // 8)
    acc, acc_bits, pos = 0, 0, 0
    for r in rows:
        acc = (acc << bits) | r
        acc_bits += bits
        whole = acc_bits // 8
        if whole:
            acc_bits -= whole * 8
            out[pos:pos + whole] = (acc >> acc_bits).to_bytes(whole, 'big')
            pos += whole
            acc &= (1 << acc_bits) - 1
    if acc_bits:
        out[pos] = (acc << (8 - acc_bits)) & 0xFF
    return bytes(out)


def unpack_rows(data, rows, bits):
    if not rows:
        return []
    total = int.from_bytes(data, 'big') >> (len(data) * 8 - rows * bits)
    mask = (1 << bits) - 1
    return [(total >> ((rows - 1 - i) * bits)) & mask for i in range(rows)]


def read_salt(exe_path):
    """22-byte CRC salt from the installed V2 executable (read-only)."""
    import mmap
    with open(exe_path, 'rb') as f, mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ) as m:
        pe = struct.unpack_from('<I', m, 0x3C)[0]
        nsec = struct.unpack_from('<H', m, pe + 6)[0]
        opt = struct.unpack_from('<H', m, pe + 20)[0]
        sec = pe + 24 + opt
        for k in range(nsec):
            vsize, va, size, raw = struct.unpack_from('<IIII', m, sec + 40 * k + 8)
            if va <= SALT_RVA < va + max(vsize, size):
                off = raw + SALT_RVA - va
                return bytes(m[off:off + SALT_LEN])
    raise ValueError('salt RVA not mapped')


class Table:
    def __init__(self, chunk, rows):
        self.header = bytearray(chunk[:HEADER])
        self.tid = chunk[:4][::-1].hex()
        data_len, aux_len = struct.unpack_from('<II', chunk, 4)
        if len(chunk) != padded(data_len + aux_len) + HEADER:
            raise ValueError(f'table {self.tid}: bad chunk length')
        if any(chunk[HEADER + data_len + aux_len:]):
            raise ValueError(f'table {self.tid}: non-zero padding')
        self.data = bytes(chunk[HEADER:HEADER + data_len])
        self.aux = bytes(chunk[HEADER + data_len:HEADER + data_len + aux_len])
        self.rows = rows
        self.bits = 0
        if len(self.aux) >= FIELD_RECORD:
            hi, lo = struct.unpack_from('>II', self.aux, 0)
            total = (hi << 32) | lo
            if struct.unpack_from('>I', self.aux, 60)[0] != rows:
                raise ValueError(f'table {self.tid}: schema row count differs from directory')
            if rows:
                if total % rows or (total + 7) // 8 != data_len:
                    raise ValueError(f'table {self.tid}: schema bit total inconsistent')
                self.bits = total // rows
        elif rows:
            raise ValueError(f'table {self.tid}: rows without schema')
        self._rows = None

    def h(self, i):
        return struct.unpack_from('<I', self.header, 4 * i)[0]

    def all_rows(self):
        if self._rows is None:
            self._rows = unpack_rows(self.data, self.rows, self.bits)
        return list(self._rows)

    def row(self, i):
        return self.all_rows()[i]

    def set_rows(self, rows):
        if any(r >> self.bits for r in rows):
            raise ValueError(f'table {self.tid}: row wider than schema')
        self._rows = list(rows)
        self.rows = len(rows)
        self.data = pack_rows(rows, self.bits)
        total = self.rows * self.bits
        aux = bytearray(self.aux)
        struct.pack_into('>II', aux, 0, total >> 32, total & 0xFFFFFFFF)
        struct.pack_into('>I', aux, 60, self.rows)
        self.aux = bytes(aux)

    def check_crcs(self, salt):
        ok = True
        if self.h(6):
            ok &= zlib.crc32(self.content()[self.h(5):self.h(5) + self.h(6)] + salt) == self.h(7)
        if self.h(2):
            ok &= zlib.crc32(self.aux + salt) == self.h(4)
        return ok

    def content(self):
        return self.data + self.aux

    def chunk(self, salt):
        hdr = bytearray(self.header)
        struct.pack_into('<II', hdr, 4, len(self.data), len(self.aux))
        if self.h(6) or self.data:
            struct.pack_into('<II', hdr, 20, 0, len(self.data))
            struct.pack_into('<I', hdr, 28, zlib.crc32(self.data + salt))
        if self.aux:
            struct.pack_into('<I', hdr, 16, zlib.crc32(self.aux + salt))
        body = self.content()
        return bytes(hdr) + body + bytes(padded(len(body)) - len(body))


class Directory:
    def __init__(self, chunk):
        self.header = bytearray(chunk[:HEADER])
        h = struct.unpack_from('<13I', chunk, 0)
        if f'{h[0]:08x}' != DIRECTORY or h[1] or h[2] or h[5]:
            raise ValueError('unexpected directory chunk header')
        self.length = h[6]
        body = chunk[HEADER:HEADER + self.length]
        if any(chunk[HEADER + self.length:]) or len(chunk) != HEADER + padded(self.length):
            raise ValueError('bad directory padding')
        self.body = body

    def entries_start(self, first_table):
        return self.body.find(bytes.fromhex(first_table))

    def parse(self, first_table):
        q = self.entries_start(first_table)
        if q < 0 or (len(self.body) - q) % 8 not in (0, 2):
            raise ValueError('directory entries not found')
        self.prefix = self.body[:q]
        n = (len(self.body) - q) // 8
        self.entries = [(self.body[q + 8 * k:q + 8 * k + 4].hex(),
                         struct.unpack_from('>I', self.body, q + 8 * k + 4)[0]) for k in range(n)]
        self.suffix = self.body[q + 8 * n:]

    def chunk(self, counts, salt):
        body = bytearray(self.prefix)
        for tid, _ in self.entries:
            body += bytes.fromhex(tid) + struct.pack('>I', counts[tid])
        body += self.suffix
        hdr = bytearray(self.header)
        struct.pack_into('<I', hdr, 24, len(body))
        struct.pack_into('<I', hdr, 28, zlib.crc32(bytes(body) + salt))
        return bytes(hdr) + bytes(body) + bytes(padded(len(body)) - len(body))


class Roster:
    def __init__(self, iff_bytes):
        self.iff = bytes(iff_bytes)
        z = zipfile.ZipFile(io.BytesIO(self.iff))
        if z.namelist() != ['roster.RDAT'] or z.getinfo('roster.RDAT').compress_type != 0:
            raise ValueError('expected one stored roster.RDAT member')
        self.rdat = z.read('roster.RDAT')
        self._parse()

    def _parse(self):
        d = self.rdat
        if struct.unpack_from('<I', d, 0)[0] != len(d) - 0x10 or d[0x10:0x14] != b'BNH(' \
                or struct.unpack_from('<I', d, 0x14)[0] != len(d) - 0x18:
            raise ValueError('not an RDAT stream')
        self.prefix = d[:0x28]
        chunks, p = [], 0x28
        while p < len(d):
            if d[p:p + 4] != b'BNDL':
                raise ValueError(f'expected BNDL at {p:#x}')
            size = struct.unpack_from('<I', d, p + 4)[0]
            chunks.append(d[p + 8:p + 8 + HEADER + size])
            p += 8 + HEADER + size
        if p != len(d):
            raise ValueError('trailing bytes')
        self.directory = Directory(chunks[0])
        self.directory.parse(chunks[1][:4][::-1].hex())
        counts = dict(self.directory.entries)
        self.order = [c[:4][::-1].hex() for c in chunks[1:]]
        if self.order != [t for t, _ in self.directory.entries]:
            raise ValueError('directory order differs from chunk order')
        self.tables = {}
        for c in chunks[1:]:
            t = Table(c, counts[c[:4][::-1].hex()])
            self.tables[t.tid] = t
        self.counts = {t: self.tables[t].rows for t in self.order}

    def check_crcs(self, salt):
        bad = [t for t in self.order if not self.tables[t].check_crcs(salt)]
        h = struct.unpack_from('<13I', self.directory.header, 0)
        if zlib.crc32(self.directory.body + salt) != h[7]:
            bad.append(DIRECTORY)
        return bad

    def type_map(self):
        """schema type hash -> [table ids]"""
        out = {}
        for tid in self.order:
            t = self.tables[tid]
            if len(t.aux) >= FIELD_RECORD:
                out.setdefault(struct.unpack_from('>I', t.aux, 12)[0], []).append(tid)
        return out

    def rdat_bytes(self, salt):
        counts = {t: self.tables[t].rows for t in self.order}
        body = bytearray()
        for chunk in [self.directory.chunk(counts, salt)] + [self.tables[t].chunk(salt) for t in self.order]:
            body += b'BNDL' + struct.pack('<I', len(chunk) - HEADER) + chunk
        total = len(self.prefix) + len(body)
        prefix = bytearray(self.prefix)
        struct.pack_into('<I', prefix, 0, total - 0x10)
        struct.pack_into('<I', prefix, 0x14, total - 0x18)
        return bytes(prefix) + bytes(body)

    def iff_bytes(self, salt):
        """Rebuild the zip with the original local/central header fields."""
        rdat = self.rdat_bytes(salt)
        src = self.iff
        name_len, extra_len = struct.unpack_from('<HH', src, 26)
        local = bytearray(src[:30 + name_len + extra_len])
        old_end = len(local) + struct.unpack_from('<I', src, 18)[0]
        central = bytearray(src[old_end:])
        crc = zlib.crc32(rdat)
        struct.pack_into('<III', local, 14, crc, len(rdat), len(rdat))
        if central[:4] != b'PK\x01\x02':
            raise ValueError('central directory not where expected')
        struct.pack_into('<III', central, 16, crc, len(rdat), len(rdat))
        eocd = central.rfind(b'PK\x05\x06')
        struct.pack_into('<I', central, eocd + 16, len(local) + len(rdat))
        return bytes(local) + rdat + bytes(central)


class Field:
    """One schema record. offset is absolute within the row (parents resolved)."""
    def __init__(self, w, parent):
        self.rel, self.width, self.type, self.type_hash, self.name_hash = w[0], w[1], w[2], w[3], w[4]
        self.parent_index, self.index = w[5], w[6]
        self.is_array = w[7] == 1
        self.count = w[15] if self.is_array else 1
        self.offset = self.rel + (parent.offset if parent else 0)


def schema(table):
    """All schema records of a table, in file order, with absolute offsets."""
    a = table.aux
    n = len(a) // 4
    w = struct.unpack(f'>{n}I', a[:n * 4])
    recs, i = [], 19
    while i + 19 <= n and w[i + 6] == len(recs) + 1:
        recs.append(w[i:i + 19])
        i += 19
    fields, pending = {}, list(recs)
    while pending:
        ready = [r for r in pending if r[5] == 0 or r[5] in fields]
        if not ready:
            raise ValueError(f'{table.tid}: unresolved parent fields')
        for r in ready:
            fields[r[6]] = Field(r, fields.get(r[5]))
            pending.remove(r)
    result = [fields[r[6]] for r in recs]
    top = [f for f in result if f.parent_index == 0]
    if table.bits and top and max(f.offset + f.width for f in top) != table.bits:
        raise ValueError(f'{table.tid}: schema does not cover row bits')
    return result


def ref_slots(table, fields=None):
    """(field, bit offset) of every 64-bit reference element in a row."""
    fields = fields if fields is not None else schema(table)
    slots = []
    for f in fields:
        if f.type != REF_TYPE:
            continue
        if f.parent_index:
            raise ValueError(f'{table.tid}: nested references are not supported')
        if f.width != 64 * f.count:
            raise ValueError(f'{table.tid}: unexpected reference width')
        slots.extend((f, f.offset + 64 * k) for k in range(f.count))
    return slots
