"""Isolated file decoder research; never accesses or modifies a game process.

The protected wrapper requires an explicitly supplied key. No guessing, runtime
key capture, or launcher integration. Accept output only after authored size/CRC
checks. The current crypto provider supports 8..56 byte, 8-byte-aligned keys;
the game's longer-key variant remains unsupported until independently tested.
"""
import argparse
import ctypes
import hashlib
import json
import pathlib
import struct
import zlib

MAX_BYTES = 64 * 1024 * 1024
O0DLE_SHA256 = '9a66b713306e504863646969a59fa435c3eac01457925f25024a4883803892f5'
ARTIFACTS = pathlib.Path(__file__).resolve().parent.parent / 'artifacts' / 'roster-key-offline'


def inspect(data):
    if not 24 <= len(data) <= MAX_BYTES:
        raise ValueError('Wrapper length outside offline bounds')
    if data[:3] != b'\x1f\x8b\x21' or data[3] not in (4, 0x84):
        raise ValueError('Unsupported wrapper method/flags')
    if data[10:16] != b'\x04\x00VCZ\x00':
        raise ValueError('Unsupported VCZ extra header')
    crc, size = struct.unpack_from('<II', data, len(data) - 8)
    if not 1 <= size <= MAX_BYTES:
        raise ValueError('Authored output size outside offline bounds')
    encrypted = bool(data[3] & 0x80)
    if encrypted and (len(data) - 24) % 8:
        raise ValueError('Encrypted payload must be 8-byte aligned')
    return {'encrypted': encrypted, 'payloadOffset': 16,
            'payloadBytes': len(data) - 24, 'decodedBytes': size,
            'crc32': f'{crc:08X}', 'sha256': hashlib.sha256(data).hexdigest()}


def decode_wrapper(data, decompress, key=None):
    metadata = inspect(data)
    payload = data[16:-8]
    if metadata['encrypted']:
        if key is None:
            raise ValueError('Encrypted wrapper requires an explicitly supplied decryption key')
        if not 8 <= len(key) <= 56 or len(key) % 8:
            raise ValueError('Offline provider supports only 8..56 byte aligned keys; longer game keys are unverified')
        from cryptography.hazmat.decrepit.ciphers.algorithms import Blowfish
        from cryptography.hazmat.primitives.ciphers import Cipher, modes
        decryptor = Cipher(Blowfish(key), modes.ECB()).decryptor()
        payload = decryptor.update(payload) + decryptor.finalize()
    elif key is not None:
        raise ValueError('A key was supplied for an unencrypted wrapper')
    decoded = decompress(payload, metadata['decodedBytes'])
    if len(decoded) != metadata['decodedBytes']:
        raise ValueError('Decoded length differs from authored trailer')
    if f'{zlib.crc32(decoded):08X}' != metadata['crc32']:
        raise ValueError('Decoded CRC differs from authored trailer')
    return decoded, metadata


def oodle_decoder(path):
    path = path.resolve(strict=True)
    if hashlib.sha256(path.read_bytes()).hexdigest() != O0DLE_SHA256:
        raise ValueError('Oodle runtime is not the independently recorded Version 2 DLL')
    library = ctypes.WinDLL(str(path), winmode=0x100 | 0x800)
    function = library.OodleLZ_Decompress
    function.restype = ctypes.c_int64
    function.argtypes = [ctypes.c_void_p, ctypes.c_int64, ctypes.c_void_p,
                         ctypes.c_int64, ctypes.c_int, ctypes.c_int, ctypes.c_int,
                         ctypes.c_void_p, ctypes.c_int64, ctypes.c_void_p,
                         ctypes.c_void_p, ctypes.c_void_p, ctypes.c_int64, ctypes.c_int]
    def decompress(payload, size):
        source = ctypes.create_string_buffer(payload)
        destination = ctypes.create_string_buffer(size)
        result = function(source, len(payload), destination, size, 1, 0, 0,
                          None, 0, None, None, None, 0, 3)
        if result != size:
            raise ValueError(f'Oodle rejected payload or returned wrong length ({result})')
        return destination.raw
    return decompress


def main():
    p = argparse.ArgumentParser()
    p.add_argument('input', type=pathlib.Path)
    p.add_argument('--oodle', type=pathlib.Path)
    p.add_argument('--key-file', type=pathlib.Path)
    p.add_argument('--output-name')
    args = p.parse_args()
    data = args.input.read_bytes()
    metadata = inspect(data)
    if args.output_name is None:
        print(json.dumps(metadata, indent=2))
        return
    # Research outputs are create-only and confined to this project, never game/Data/Library.
    name = args.output_name
    if pathlib.Path(name).name != name or name in ('.', '..') or any(c in name for c in '/\\:'):
        raise ValueError('Output must be a simple artifact filename')
    if metadata['encrypted'] and args.key_file is None:
        raise ValueError('Encrypted wrapper requires an explicitly supplied decryption key; no decoder was loaded')
    key = args.key_file.read_bytes() if args.key_file else None
    if args.oodle is None:
        raise ValueError('Explicit verified Oodle DLL path required')
    decoded, metadata = decode_wrapper(data, oodle_decoder(args.oodle), key)
    ARTIFACTS.mkdir(parents=True, exist_ok=True)
    output = ARTIFACTS / name
    with output.open('xb') as file:
        file.write(decoded)
    print(json.dumps({'output': str(output), 'lengthAndCrcVerified': True,
                      'outputSha256': hashlib.sha256(decoded).hexdigest(), **metadata}, indent=2))


if __name__ == '__main__':
    main()
