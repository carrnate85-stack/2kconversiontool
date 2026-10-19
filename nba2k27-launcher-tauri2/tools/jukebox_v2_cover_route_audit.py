"""Offline proof of the NBA 2K27 Version 2 Jukebox cover-art route (read-only).

Opens NBA2K27.exe, `manifest`, archives `0B`/`0I` and the game's own Oodle DLL
(`data/oodle/oo2core_9_win64.dll`, SHA-256 pinned, loaded with a restricted
search path) with plain read-only file opens. It never opens a game process and
never writes inside the game folder.

What it proves (see artifacts/jukebox-covers-v2-route-20261017.md):

1. `music.iff` (music.json) names `"Default Cover Art Context": "jukebox"`; the
   exe formats `alb_cvr_cont_{0:x8}.iff` (lea at RVA 0x2624C0) and the stock
   container is `alb_cvr_cont_<CRC-32("jukebox")>.iff` = `alb_cvr_cont_fcb780e2.iff`.
2. The container is a STORED zip of `<cover>.TXTR` descriptors. Each TXTR holds
   CRC-32(name) at +0x18, Width/Height 256, Format BC1_UNORM, PixelDataSize
   32768, Twiddled 1, CompressionMethod 0x21 and a `Binary` leaf
   `<cover>.<16 hex>.tld`. Every music.json Cover Name has a TXTR.
3. The exe builds `shared/` + `%02x/` + leaf; the 16 hex digits are
   CRC-64/XZ of the whole stored TLD file and the folder is its first byte.
4. Stock cover TLDs: header `TLD ` .. `05 21 47 01`, stored size = file-32,
   payload is Oodle (Leviathan) that decodes to 32768 bytes of BC1 in a planar
   split (4096 endpoint dwords, then 4096 index dwords).
5. The only stock TLDs with stored == raw use compression byte 0x00, never 0x21;
   and OodleLZ_Decompress refuses comp == raw. The current Rust/WPF cover TLD
   (0x21, stored == raw, interleaved BC1) therefore has no stock precedent.
6. The game's Oodle DLL compresses (Leviathan) and round-trips, so a stock-form
   TLD can be produced by the launcher.

Run:  python -I tools\\jukebox_v2_cover_route_audit.py [--game DIR] [--json out]
      python -I tools\\jukebox_v2_cover_route_audit.py --synth NAME [--cover 001_helicopter]
`--synth` writes a stock-form demo TLD, the retargeted container and a plan into
a NEW folder artifacts\\jukebox-cover-route-synth\\NAME (create-only).
Tests: python -m unittest discover -s tools -p test_jukebox_v2_cover_route_audit.py
       (NBA2K27_ACTUAL_DISK_AUDIT=1 adds the installed-game checks).
"""
import argparse
import ctypes
import hashlib
import io
import json
import mmap
import os
import pathlib
import re
import struct
import sys
import zipfile
import zlib

DEFAULT_GAME = r"C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27"
EXPECTED_STAMP = 0x6AC53765
EXPECTED_IMAGE_SIZE = 0x35BDE000
OODLE_SHA256 = "9a66b713306e504863646969a59fa435c3eac01457925f25024a4883803892f5"
OODLE_LEVIATHAN = 13
OODLE_LEVEL_OPTIMAL1 = 5
CONTEXT = "jukebox"
KNOWN_ODD_COVER = "071_ruff_ryders_anthem"  # stock 255x256 cover; not a slot to reuse
FORMAT_LEA_RVA = 0x2624C0
FORMAT_TEXT = "alb_cvr_cont_{0:x8}.iff"
TXTR_MAGIC = bytes.fromhex("91f81fe0")
SIDE = 256
RAW_BYTES = 32768
BLOCKS = 4096
# 24 header bytes shared by every stock 256x256 cover and the Rust codec.
COVER_PREFIX = bytes.fromhex("544c442000000000052147011f0000000001000101000100")
FIELDS = ("Width", "Height", "Format", "Min", "Max", "PixelDataSize", "Twiddled",
          "CompressionMethod", "Binary")
LEAF = re.compile(r"^(?P<name>[^/\\\0]+)\.(?P<hash>[0-9a-f]{16})\.tld$")
ARTIFACTS = pathlib.Path(__file__).resolve().parent.parent / "artifacts"


# ---------------------------------------------------------------- hashes
def crc32(data: bytes) -> int:
    return zlib.crc32(data) & 0xFFFFFFFF


_CRC64_TABLE = []
for _i in range(256):
    _c = _i
    for _ in range(8):
        _c = (_c >> 1) ^ 0xC96C5795D7870F42 if _c & 1 else _c >> 1
    _CRC64_TABLE.append(_c)


def crc64_xz(data: bytes) -> int:
    c = 0xFFFFFFFFFFFFFFFF
    for b in data:
        c = _CRC64_TABLE[(c ^ b) & 0xFF] ^ (c >> 8)
    return c ^ 0xFFFFFFFFFFFFFFFF


def container_name(context: str) -> str:
    return f"alb_cvr_cont_{crc32(context.encode()):08x}.iff"


def tld_leaf(name: str, tld: bytes) -> str:
    return f"{name}.{crc64_xz(tld):016x}.tld"


def shared_path(leaf: str) -> str:
    m = LEAF.match(leaf)
    if not m:
        raise ValueError(f"not a shared texture leaf: {leaf!r}")
    return f"shared/{m['hash'][:2]}/{leaf}"


# ---------------------------------------------------------------- VCZ / Oodle
class Oodle:
    """The game's own Oodle runtime, SHA-256 pinned (same DLL the launcher pins)."""

    def __init__(self, game: pathlib.Path):
        path = (game / "data" / "oodle" / "oo2core_9_win64.dll").resolve(strict=True)
        if hashlib.sha256(path.read_bytes()).hexdigest() != OODLE_SHA256:
            raise ValueError("Oodle runtime is not the recorded Version 2 DLL")
        lib = ctypes.WinDLL(str(path), winmode=0x100 | 0x800)
        self._d = lib.OodleLZ_Decompress
        self._d.restype = ctypes.c_int64
        self._d.argtypes = [ctypes.c_void_p, ctypes.c_int64, ctypes.c_void_p, ctypes.c_int64,
                            ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_void_p,
                            ctypes.c_int64, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p,
                            ctypes.c_int64, ctypes.c_int]
        self._c = lib.OodleLZ_Compress
        self._c.restype = ctypes.c_int64
        self._c.argtypes = [ctypes.c_int, ctypes.c_void_p, ctypes.c_int64, ctypes.c_void_p,
                            ctypes.c_int, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p,
                            ctypes.c_void_p, ctypes.c_int64]

    def decompress(self, payload: bytes, size: int) -> tuple:
        dst = ctypes.create_string_buffer(size)
        n = self._d(payload, len(payload), dst, size, 1, 0, 0, None, 0, None, None, None, 0, 3)
        return n, (dst.raw if n == size else b"")

    def compress(self, raw: bytes, level: int = OODLE_LEVEL_OPTIMAL1) -> bytes:
        dst = ctypes.create_string_buffer(len(raw) + 65536)
        n = self._c(OODLE_LEVIATHAN, raw, len(raw), dst, level, None, None, None, None, 0)
        if n <= 0:
            raise ValueError("Oodle compression failed")
        return dst.raw[:n]


def vcz_decode(packed: bytes, oodle: Oodle) -> bytes:
    if packed[:4] != b"\x1f\x8b\x21\x04" or packed[10:16] != b"\x04\x00VCZ\x00":
        raise ValueError("not an unencrypted VCZ entry")
    crc, size = struct.unpack_from("<II", packed, len(packed) - 8)
    n, out = oodle.decompress(packed[16:-8], size)
    if n != size or crc32(out) != crc:
        raise ValueError("VCZ length/CRC mismatch")
    return out


# ---------------------------------------------------------------- TXTR
def txtr_parse(data: bytes) -> dict:
    """Name, Binary leaf and integer fields of one TXTR descriptor."""
    if data[:4] != TXTR_MAGIC:
        raise ValueError("bad TXTR magic")
    table = struct.unpack_from("<I", data, 8)[0]
    if not 0 < table < len(data) or data[-1] != 0:
        raise ValueError("bad TXTR string table")
    strings = data[len(data) - table:-1].split(b"\0")
    name = strings[0].decode("latin1")
    leaf = strings[-1].decode("latin1")
    if struct.unpack_from("<I", data, 0x18)[0] != crc32(strings[0]):
        raise ValueError("TXTR object hash is not CRC-32(name)")
    fields = {}
    for field in FIELDS:
        key = struct.pack("<I", crc32(field.encode()))
        at = data.find(key)
        if at < 0:
            raise ValueError(f"TXTR field {field} missing")
        if data[at + 4:at + 8] == b"\x06\x00\x00\x40":
            fields[field] = struct.unpack_from("<I", data, at + 16)[0]
    m = LEAF.match(leaf)
    if not m or m["name"] != name or b"Binary" not in strings:
        raise ValueError("TXTR Binary leaf does not match its name")
    return {"name": name, "leaf": leaf, "hash": m["hash"], "fields": fields,
            "leafOffset": len(data) - 1 - len(leaf)}


def txtr_retarget(data: bytes, new_hash: str) -> bytes:
    """Same TXTR with only the 16 hex digits of its Binary leaf replaced."""
    if not re.fullmatch(r"[0-9a-f]{16}", new_hash):
        raise ValueError("hash must be 16 lowercase hex digits")
    info = txtr_parse(data)
    at = info["leafOffset"] + len(info["name"]) + 1
    out = data[:at] + new_hash.encode() + data[at + 16:]
    assert len(out) == len(data) and txtr_parse(out)["hash"] == new_hash
    return out


# ---------------------------------------------------------------- stored zip
def zip_entries(data: bytes) -> list:
    z = zipfile.ZipFile(io.BytesIO(data))
    out = []
    for info in z.infolist():
        if info.compress_type != zipfile.ZIP_STORED or info.file_size != info.compress_size:
            raise ValueError("container entries must be STORED")
        fn, ex = struct.unpack_from("<HH", data, info.header_offset + 26)
        start = info.header_offset + 30 + fn + ex
        out.append({"name": info.filename, "start": start, "size": info.file_size,
                    "local": info.header_offset, "crc": info.CRC})
    return out


def zip_replace_stored(data: bytes, name: str, payload: bytes) -> bytes:
    """Replace one STORED member of equal size in place (local + central CRC)."""
    entry = next((e for e in zip_entries(data) if e["name"] == name), None)
    if entry is None or len(payload) != entry["size"]:
        raise ValueError("same-size STORED member required")
    out = bytearray(data)
    out[entry["start"]:entry["start"] + entry["size"]] = payload
    new_crc = struct.pack("<I", crc32(payload))
    out[entry["local"] + 14:entry["local"] + 18] = new_crc
    eocd = data.rfind(b"PK\x05\x06")
    count, size, offset = struct.unpack_from("<HII", data, eocd + 10)
    at = offset
    for _ in range(count):
        if data[at:at + 4] != b"PK\x01\x02":
            raise ValueError("bad central directory")
        fn, ex, cm = struct.unpack_from("<HHH", data, at + 28)
        local = struct.unpack_from("<I", data, at + 42)[0]
        if local == entry["local"]:
            out[at + 16:at + 20] = new_crc
        at += 46 + fn + ex + cm
    out = bytes(out)
    check = zipfile.ZipFile(io.BytesIO(out))
    if check.testzip() is not None or check.read(name) != payload:
        raise ValueError("rebuilt container failed its CRC check")
    return out


# ---------------------------------------------------------------- BC1 / TLD
def rgb565(r, g, b):
    return ((r * 31 + 127) // 255) << 11 | ((g * 63 + 127) // 255) << 5 | ((b * 31 + 127) // 255)


def unpack565(v):
    return ((v >> 11) & 31) * 255 // 31, ((v >> 5) & 63) * 255 // 63, (v & 31) * 255 // 31


def bc1_block(pixels) -> bytes:
    """Opaque 4-colour BC1 block (c0 > c1) from 16 RGB tuples."""
    lum = [r * 3 + g * 6 + b for r, g, b in pixels]
    hi, lo = pixels[lum.index(max(lum))], pixels[lum.index(min(lum))]
    c0, c1 = rgb565(*hi), rgb565(*lo)
    if c0 == c1:
        c0, c1 = (c0, c0 - 1) if c0 else (1, 0)
    elif c0 < c1:
        c0, c1 = c1, c0
    p0, p1 = unpack565(c0), unpack565(c1)
    pal = [p0, p1, tuple((2 * a + b) // 3 for a, b in zip(p0, p1)),
           tuple((a + 2 * b) // 3 for a, b in zip(p0, p1))]
    bits = 0
    for i, px in enumerate(pixels):
        best = min(range(4), key=lambda k: sum((x - y) ** 2 for x, y in zip(px, pal[k])))
        bits |= best << (2 * i)
    return struct.pack("<HHI", c0, c1, bits)


def bc1_encode(rgb: bytes) -> bytes:
    """Linear (row-major) interleaved BC1 for a 256x256 RGB image."""
    if len(rgb) != SIDE * SIDE * 3:
        raise ValueError("need 256x256 RGB")
    out = bytearray()
    for by in range(64):
        for bx in range(64):
            px = []
            for y in range(4):
                row = ((by * 4 + y) * SIDE + bx * 4) * 3
                px += [tuple(rgb[row + x * 3:row + x * 3 + 3]) for x in range(4)]
            out += bc1_block(px)
    return bytes(out)


def planar_split(interleaved: bytes) -> bytes:
    """BC1 blocks -> 4096 endpoint dwords then 4096 index dwords (V2 stock layout)."""
    if len(interleaved) != RAW_BYTES:
        raise ValueError("need 32768 BC1 bytes")
    return (b"".join(interleaved[i * 8:i * 8 + 4] for i in range(BLOCKS))
            + b"".join(interleaved[i * 8 + 4:i * 8 + 8] for i in range(BLOCKS)))


def planar_join(planar: bytes) -> bytes:
    if len(planar) != RAW_BYTES:
        raise ValueError("need 32768 BC1 bytes")
    half = RAW_BYTES // 2
    return b"".join(planar[i * 4:i * 4 + 4] + planar[half + i * 4:half + i * 4 + 4]
                    for i in range(BLOCKS))


def bc1_decode_block(block: bytes) -> list:
    a, b, bits = struct.unpack("<HHI", block)
    p0, p1 = unpack565(a), unpack565(b)
    if a > b:
        pal = [p0, p1, tuple((2 * x + y) // 3 for x, y in zip(p0, p1)),
               tuple((x + 2 * y) // 3 for x, y in zip(p0, p1))]
    else:
        pal = [p0, p1, tuple((x + y) // 2 for x, y in zip(p0, p1)), (0, 0, 0)]
    return [pal[(bits >> (2 * i)) & 3] for i in range(16)]


def edge_step(interleaved: bytes) -> float:
    """Mean |RGB step| across every 4x4 block edge; small when blocks line up."""
    px = [bc1_decode_block(interleaved[i * 8:i * 8 + 8]) for i in range(BLOCKS)]
    total = count = 0
    for i in range(BLOCKS):
        if (i + 1) % 64:
            for y in range(4):
                total += sum(abs(s - t) for s, t in zip(px[i][y * 4 + 3], px[i + 1][y * 4]))
                count += 1
        if i + 64 < BLOCKS:
            for x in range(4):
                total += sum(abs(s - t) for s, t in zip(px[i][12 + x], px[i + 64][x]))
                count += 1
    return total / count


def layout_scores(raw: bytes) -> dict:
    """Block-edge continuity of the 32768 bytes read as planar vs interleaved."""
    p, q = edge_step(planar_join(raw)), edge_step(raw)
    return {"planarEdgeStep": round(p, 2), "interleavedEdgeStep": round(q, 2), "planar": p < q}


def tld_build(planar_raw: bytes, oodle: Oodle) -> bytes:
    """Stock-form cover TLD: header, stored size, Oodle-Leviathan payload."""
    payload = oodle.compress(planar_raw)
    n, back = oodle.decompress(payload, RAW_BYTES)
    if n != RAW_BYTES or back != planar_raw:
        raise ValueError("Oodle round-trip failed")
    return COVER_PREFIX + struct.pack("<II", RAW_BYTES, len(payload)) + payload


def tld_validate_stock_form(tld: bytes) -> None:
    if tld[:24] != COVER_PREFIX or struct.unpack_from("<I", tld, 24)[0] != RAW_BYTES:
        raise ValueError("not a 256x256 BC1 cover TLD header")
    stored = struct.unpack_from("<I", tld, 28)[0]
    if stored != len(tld) - 32 or stored >= RAW_BYTES or tld[32] != 0x8C:
        raise ValueError("cover TLD payload is not stock-form Oodle")


# ---------------------------------------------------------------- game reads
def read_manifest(game: pathlib.Path) -> dict:
    rows = {}
    with open(game / "manifest", "rb") as f:
        for raw in f:
            p = raw.decode("latin1").strip().split(",")
            if len(p) == 4:
                rows[p[0]] = (p[1], int(p[2]), int(p[3]))
    return rows


def read_entry(game: pathlib.Path, row) -> bytes:
    archive, offset, size = row
    if not re.fullmatch(r"[0-9A-Z]{2}", archive):
        raise ValueError("unexpected archive name")
    with open(game / archive, "rb") as f:
        f.seek(offset)
        data = f.read(size)
    if len(data) != size:
        raise ValueError("short archive read")
    return data


def check_exe(game: pathlib.Path) -> dict:
    with open(game / "NBA2K27.exe", "rb") as f, mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ) as m:
        pe = struct.unpack_from("<I", m, 0x3C)[0]
        stamp = struct.unpack_from("<I", m, pe + 8)[0]
        nsec = struct.unpack_from("<H", m, pe + 6)[0]
        opt = struct.unpack_from("<H", m, pe + 20)[0]
        size = struct.unpack_from("<I", m, pe + 24 + 56)[0]
        secs = []
        for i in range(nsec):
            o = pe + 24 + opt + 40 * i
            vs, va, rs, rp = struct.unpack_from("<IIII", m, o + 8)
            secs.append((va, max(vs, rs), rp))

        def rva_off(rva):
            for va, vs, rp in secs:
                if va <= rva < va + vs:
                    return rva - va + rp
            return None
        fmt = m.find(FORMAT_TEXT.encode("utf-16-le"))
        fmt_rva = next((fmt - rp + va for va, vs, rp in secs if rp <= fmt < rp + vs), None) if fmt >= 0 else None
        lea_off = rva_off(FORMAT_LEA_RVA)
        lea_ok = False
        if lea_off is not None and fmt_rva is not None:
            ins = bytes(m[lea_off:lea_off + 7])
            lea_ok = ins[:3] == b"\x4c\x8d\x05" and FORMAT_LEA_RVA + 7 + struct.unpack_from("<i", ins, 3)[0] == fmt_rva
        shared = m.find(b"shared/\0%02x/\0")
        missing_msg = m.find("shared texture file {0} not found".encode("utf-16-le"))
        ctx = m.find(b"MusicData_CreateCoverArtContext\0")
    return {"stamp": f"{stamp:08X}", "imageSize": f"{size:08X}",
            "pinnedV2": stamp == EXPECTED_STAMP and size == EXPECTED_IMAGE_SIZE,
            "containerFormatUtf16": fmt >= 0, "containerFormatLea": lea_ok,
            "sharedPrefixAndFolderFormat": shared >= 0, "missingTextureMessage": missing_msg >= 0,
            "createCoverArtContext": ctx >= 0}


def audit(game: pathlib.Path) -> dict:
    report = {"schema": "nba2k27-v2-jukebox-cover-route-audit-v1",
              "gameProcessAccess": False, "gameFilesChanged": False}
    checks = []

    def add(name, ok, detail=""):
        checks.append({"name": name, "ok": bool(ok), "detail": detail})

    exe = check_exe(game)
    report["exe"] = exe
    add("pinned Version 2 image", exe["pinnedV2"], f"{exe['stamp']}/{exe['imageSize']}")
    add("exe formats alb_cvr_cont_{0:x8}.iff at lea 0x2624C0", exe["containerFormatUtf16"] and exe["containerFormatLea"])
    add("exe builds shared/ + %02x/ texture paths", exe["sharedPrefixAndFolderFormat"] and exe["missingTextureMessage"])

    rows = read_manifest(game)
    oodle = Oodle(game)
    music_zip = vcz_decode(read_entry(game, rows["music.iff"]), oodle)
    music = json.loads(zipfile.ZipFile(io.BytesIO(music_zip)).read("music.json"))
    context = music.get("Default Cover Art Context")
    name = container_name(context or "")
    add("music.json default cover context is 'jukebox'", context == CONTEXT, str(context))
    add("container = alb_cvr_cont_<CRC-32(context)>.iff exists", name in rows, name)
    cont = vcz_decode(read_entry(game, rows[name]), oodle)
    entries = zip_entries(cont)
    report["container"] = {"name": name, "row": rows[name], "decodedBytes": len(cont),
                           "entries": len(entries), "stored": True}

    z = zipfile.ZipFile(io.BytesIO(cont))
    txtrs, bad = {}, []
    for e in entries:
        try:
            info = txtr_parse(z.read(e["name"]))
            if e["name"] != info["name"] + ".TXTR":
                raise ValueError("entry name differs from TXTR name")
            txtrs[info["name"]] = info
        except ValueError as err:
            bad.append(f"{e['name']}: {err}")
    add("every container entry is a valid TXTR (hash = CRC-32(name), Binary = name.<16hex>.tld)", not bad, "; ".join(bad[:5]))
    covers = [t["Cover Name"] for t in music["Tracks"] if "Cover Name" in t]
    missing = sorted(set(covers) - set(txtrs))
    add("every music.json Cover Name has a TXTR", not missing and covers, f"{len(covers)} covers, missing {missing[:5]}")
    add("Cover Names are unique per track", len(set(covers)) == len(covers))

    tlds = {"count": 0, "crc64Matches": 0, "folderMatches": 0, "stockHeader": 0, "storedIsFileMinus32": 0,
            "oodleTo32768": 0, "planar": 0, "uniform256": 0, "compression0x21": 0, "leviathan": 0}
    failures, deviants = [], []
    for cname in covers:
        t = txtrs.get(cname)
        if not t:
            continue
        tlds["count"] += 1
        path = shared_path(t["leaf"])
        row = rows.get(path)
        if not row:
            failures.append(f"{cname}: {path} not in manifest")
            continue
        tlds["folderMatches"] += 1
        data = read_entry(game, row)
        tlds["crc64Matches"] += f"{crc64_xz(data):016x}" == t["hash"]
        header_ok = data[:24] == COVER_PREFIX and struct.unpack_from("<I", data, 24)[0] == RAW_BYTES
        tlds["stockHeader"] += header_ok
        stored = struct.unpack_from("<I", data, 28)[0]
        tlds["storedIsFileMinus32"] += stored == len(data) - 32
        tlds["leviathan"] += data[32:34] == b"\x8c\x0c"
        f = t["fields"]
        fields_ok = (f.get("Width"), f.get("Height"), f.get("PixelDataSize"), f.get("Twiddled")) == (256, 256, RAW_BYTES, 1)
        tlds["uniform256"] += fields_ok
        if not (header_ok and fields_ok):
            # The TLD width (u16 at +16) must agree with the TXTR Width even for an odd cover.
            deviants.append({"cover": cname, "tldWidth": struct.unpack_from("<H", data, 16)[0],
                             "txtrWidth": f.get("Width"), "txtrHeight": f.get("Height")})
        tlds["compression0x21"] += f.get("CompressionMethod") == 0x21 and data[9] == 0x21
        n, raw = oodle.decompress(data[32:], RAW_BYTES)
        if n == RAW_BYTES:
            tlds["oodleTo32768"] += 1
            tlds["planar"] += layout_scores(raw)["planar"]
    report["stockCoverTlds"] = tlds
    total = tlds["count"]
    add("each cover TLD sits at shared/<hash[0:2]>/<leaf>", tlds["folderMatches"] == total and total, "; ".join(failures[:3]))
    add("leaf hash = CRC-64/XZ(stored TLD file)", tlds["crc64Matches"] == total)
    report["stockCoverExceptions"] = deviants
    known = [d["cover"] for d in deviants] == [KNOWN_ODD_COVER] and deviants[0]["tldWidth"] == deviants[0]["txtrWidth"] == 255
    add("stock header 05 21 47 01, raw 32768, stored = file-32 (only 071 is 255 wide)",
        tlds["storedIsFileMinus32"] == total and (not deviants or known), json.dumps(deviants))
    add("TXTR fields 256x256, PixelDataSize 32768, Twiddled 1, CompressionMethod 0x21 (same exception)",
        tlds["compression0x21"] == total and (not deviants or known))
    add("payload is Oodle Leviathan decoding to 32768 bytes", tlds["oodleTo32768"] == total and tlds["leviathan"] == total)
    add("decoded BC1 is planar split (endpoints then indices)", tlds["planar"] == total, f"{tlds['planar']}/{total}")

    # Stock precedent for uncompressed TLDs (archive 0B holds the only ones seen).
    unc = []
    with open(game / "0B", "rb") as f:
        for path, row in rows.items():
            if row[0] == "0B" and path.startswith("shared/") and path.endswith(".tld"):
                f.seek(row[1])
                head = f.read(32)
                if head[:4] == b"TLD " and head[24:28] == head[28:32]:
                    unc.append({"path": path, "compressionByte": head[9]})
    report["stockUncompressedTlds0B"] = {"count": len(unc), "compressionBytes": sorted({u["compressionByte"] for u in unc})}
    add("stock uncompressed TLDs use compression byte 0x00 (never 0x21)", unc and all(u["compressionByte"] == 0 for u in unc), f"{len(unc)} rows")

    probe = bytes((i * 131 + 7) & 0xFF for i in range(RAW_BYTES))
    n, _ = oodle.decompress(probe, RAW_BYTES)
    add("OodleLZ_Decompress refuses comp == raw (no pass-through)", n != RAW_BYTES, f"returned {n}")
    sample = txtrs[covers[0]]
    stock_raw = oodle.decompress(read_entry(game, rows[shared_path(sample["leaf"])])[32:], RAW_BYTES)[1]
    rebuilt = tld_build(stock_raw, oodle)
    tld_validate_stock_form(rebuilt)
    add("game Oodle DLL compresses Leviathan and round-trips a cover", True, f"{len(rebuilt) - 32} stored bytes")

    report["checks"] = checks
    report["result"] = "PASS" if all(c["ok"] for c in checks) else "FAIL"
    return report


# ---------------------------------------------------------------- synth
def demo_image() -> bytes:
    """Deterministic 256x256 test card: colour bars over a diagonal gradient."""
    bars = [(255, 255, 255), (255, 255, 0), (0, 255, 255), (0, 255, 0),
            (255, 0, 255), (255, 0, 0), (0, 0, 255), (16, 16, 16)]
    out = bytearray()
    for y in range(SIDE):
        for x in range(SIDE):
            if y < 160:
                out += bytes(bars[x // 32])
            else:
                v = (x + y) & 0xFF
                out += bytes((v, 255 - v, (x * 2) & 0xFF))
    return bytes(out)


def synth(game: pathlib.Path, label: str, cover: str) -> dict:
    if pathlib.Path(label).name != label or not re.fullmatch(r"[A-Za-z0-9_.-]{1,64}", label):
        raise ValueError("label must be a simple folder name")
    out_dir = ARTIFACTS / "jukebox-cover-route-synth" / label
    out_dir.mkdir(parents=True, exist_ok=False)  # create-only, never inside the game folder
    rows = read_manifest(game)
    oodle = Oodle(game)
    name = container_name(CONTEXT)
    cont = vcz_decode(read_entry(game, rows[name]), oodle)
    original = zipfile.ZipFile(io.BytesIO(cont)).read(cover + ".TXTR")
    planar = planar_split(bc1_encode(demo_image()))
    tld = tld_build(planar, oodle)
    leaf = tld_leaf(cover, tld)
    path = shared_path(leaf)
    if path in rows:
        raise ValueError("new texture path collides with a stock row")
    new_txtr = txtr_retarget(original, leaf.split(".")[1])
    new_cont = zip_replace_stored(cont, cover + ".TXTR", new_txtr)
    changed = [i for i, (a, b) in enumerate(zip(cont, new_cont)) if a != b]
    files = {name: new_cont, path: tld}
    for logical, data in files.items():
        target = out_dir / "mods" / pathlib.PurePosixPath(logical)
        target.parent.mkdir(parents=True, exist_ok=True)
        with open(target, "xb") as f:
            f.write(data)
    plan = {"schema": "nba2k27-v2-jukebox-cover-route-synth-v1", "cover": cover,
            "containerLogical": name, "containerOverride": f"OVERRIDE_n={name}",
            "textureLogical": path, "textureServedBy": "stock-miss provider fallback (no override)",
            "musicCoverName": cover, "containerBytesChanged": len(changed),
            "containerSha256": hashlib.sha256(new_cont).hexdigest(),
            "textureSha256": hashlib.sha256(tld).hexdigest(), "textureBytes": len(tld),
            "gameFilesChanged": False}
    (out_dir / "plan.json").write_text(json.dumps(plan, indent=1) + "\n", encoding="utf-8")
    return plan


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--game", default=DEFAULT_GAME)
    ap.add_argument("--json", help="also write the report here")
    ap.add_argument("--synth", metavar="LABEL", help="build demo route files into artifacts (create-only)")
    ap.add_argument("--cover", default="001_helicopter")
    a = ap.parse_args(argv)
    game = pathlib.Path(a.game)
    report = synth(game, a.synth, a.cover) if a.synth else audit(game)
    text = json.dumps(report, indent=1)
    print(text)
    if a.json:
        pathlib.Path(a.json).write_text(text + "\n", encoding="utf-8")
    return 0 if a.synth or report["result"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
