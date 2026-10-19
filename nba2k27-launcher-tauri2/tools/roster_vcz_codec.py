"""Offline VCZ roster wrapper codec (decode + re-wrap) for research builds.

Never touches a game process or game files. Decoding: VCZ header check,
VCBLOWFISH ECB decrypt (tools/vcblowfish.py), Oodle decompress, authored
length + CRC check. Encoding mirrors it: Oodle Leviathan compress, zero-pad to
8 bytes, Blowfish encrypt, original 16-byte header, crc32/size trailer.
Keys come from a local file and are never printed.
"""
import argparse
import ctypes
import hashlib
import json
import pathlib
import struct
import sys
import zlib

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from roster_wrapper_offline import O0DLE_SHA256, inspect, oodle_decoder  # noqa: E402
from vcblowfish import Blowfish  # noqa: E402

LEVIATHAN = 13          # OodleLZ_Compressor_Leviathan (stream header byte 0x0C)
LEVEL_NORMAL = 4


def decode(data, key, decompress):
    meta = inspect(data)
    payload = data[16:-8]
    if meta['encrypted']:
        payload = Blowfish(key).decrypt(payload)
    out = decompress(payload, meta['decodedBytes'])
    if len(out) != meta['decodedBytes'] or f'{zlib.crc32(out):08X}' != meta['crc32']:
        raise ValueError('Decoded length/CRC mismatch')
    return out, meta, payload


def oodle_compressor(path, level=LEVEL_NORMAL):
    path = pathlib.Path(path).resolve(strict=True)
    if hashlib.sha256(path.read_bytes()).hexdigest() != O0DLE_SHA256:
        raise ValueError('Oodle runtime is not the independently recorded Version 2 DLL')
    library = ctypes.WinDLL(str(path), winmode=0x100 | 0x800)
    function = library.OodleLZ_Compress
    function.restype = ctypes.c_int64
    function.argtypes = [ctypes.c_int, ctypes.c_void_p, ctypes.c_int64, ctypes.c_void_p,
                         ctypes.c_int, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p,
                         ctypes.c_void_p, ctypes.c_int64]

    def compress(raw):
        source = ctypes.create_string_buffer(raw, len(raw))
        destination = ctypes.create_string_buffer(len(raw) + 65536 + len(raw) // 8)
        size = function(LEVIATHAN, source, len(raw), destination, level, None, None, None, None, 0)
        if size <= 0:
            raise ValueError('Oodle compression failed')
        return destination.raw[:size]
    return compress


def encode(raw, header16, key, compress):
    """Wrap decoded IFF bytes like the stock 0x84 roster wrappers."""
    if len(header16) != 16 or header16[:4] != b'\x1f\x8b\x21\x84':
        raise ValueError('Expected the stock encrypted VCZ header')
    packed = compress(raw)
    packed += bytes(-len(packed) % 8)
    return bytes(header16) + Blowfish(key).encrypt(packed) + struct.pack('<II', zlib.crc32(raw), len(raw))


def main():
    p = argparse.ArgumentParser()
    p.add_argument('input', type=pathlib.Path)
    p.add_argument('--key-file', type=pathlib.Path, required=True)
    p.add_argument('--oodle', type=pathlib.Path, required=True)
    p.add_argument('--out', type=pathlib.Path, required=True)
    a = p.parse_args()
    data = a.input.read_bytes()
    out, meta, _ = decode(data, a.key_file.read_bytes(), oodle_decoder(a.oodle))
    with a.out.open('xb') as f:
        f.write(out)
    print(json.dumps({'output': str(a.out), 'lengthAndCrcVerified': True,
                      'outputSha256': hashlib.sha256(out).hexdigest(), **meta}, indent=2))


if __name__ == '__main__':
    main()
