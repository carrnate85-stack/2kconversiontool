"""Create fresh encrypted index-repair research packs only; no activation."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import stat
import struct
import time
import zlib

from roster_vcz_codec import decode, encode, oodle_compressor
from roster_wrapper_offline import inspect, oodle_decoder, O0DLE_SHA256
from wnba_priority_wad import build_manifest, build_two_alias_manifest, parse_manifest, parse_two_alias_manifest

ROOT = Path(__file__).resolve().parents[1]
RAW_SHA = '9a25d4ef3ecd0aee51d91e569585fcd62edca42b9ae5b8a75fd652725fdcfff1'
ORIGINAL_SHA = '69445e272c0c02f6905a99bc0926e2d76e331ab7925f3621b799ac33fd6c28c2'
STOCK_DECODED_SHA = 'cd5463c17ab1819879530e09ad3e5a8c6b0aadce9c612b72801f4b598a64392f'
PRIMARY = 0x673F6D519B5CC655
ALIAS = 0xFE5CBA7819830553
FLAGS = 0x210810


def digest(data):
    return hashlib.sha256(data).hexdigest()


def ordinary(path, expected_size=None):
    path = path.absolute()
    for current in (path, *path.parents):
        info = current.lstat()
        if getattr(info, 'st_file_attributes', 0) & 0x400:
            raise ValueError('Reparse inputs or ancestors are unsupported')
    info = path.stat()
    if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
        raise ValueError('Input must be ordinary and single-link')
    if expected_size is not None and info.st_size != expected_size:
        raise ValueError('Input length differs from required identity')
    return path


def output_dir(path):
    path = path.absolute()
    if path.exists() or not path.resolve().is_relative_to((ROOT / 'artifacts').resolve()):
        raise ValueError('Outputs must be a fresh project artifact folder')
    for current in (path.parent, *path.parent.parents):
        if getattr(current.lstat(), 'st_file_attributes', 0) & 0x400:
            raise ValueError('Reparse output ancestors are unsupported')
    return path


def create(path, data):
    with path.open('xb') as f:
        f.write(data); f.flush(); os.fsync(f.fileno())
    if path.read_bytes() != data:
        raise ValueError('Durable artifact readback differs')


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--raw', type=Path, required=True)
    p.add_argument('--original', type=Path, required=True)
    p.add_argument('--key-file', type=Path, required=True)
    p.add_argument('--oodle', type=Path, required=True)
    p.add_argument('--out', type=Path, required=True)
    a = p.parse_args()
    out = output_dir(a.out)
    raw = ordinary(a.raw, 18873560).read_bytes()
    original = ordinary(a.original, 1762528).read_bytes()
    dll = ordinary(a.oodle)
    if digest(raw) != RAW_SHA or digest(original) != ORIGINAL_SHA or digest(dll.read_bytes()) != O0DLE_SHA256:
        raise ValueError('Pinned research input or Oodle identity differs')
    # The explicit local key is contained to the encoder/decoder roundtrip.
    # It is never copied, printed, hashed, included in metadata or sent remotely.
    key = ordinary(a.key_file, 32).read_bytes()
    started = time.perf_counter()
    # A same-key self roundtrip alone cannot establish the game's real key.
    # Prove it against the exact stock ciphertext and independently pinned plaintext first.
    decompress = oodle_decoder(dll)
    stock_decoded, _, _ = decode(original, key, decompress)
    if digest(stock_decoded) != STOCK_DECODED_SHA or len(stock_decoded) != 18423776:
        raise ValueError('Local key does not decode the exact pinned stock input')
    wrapped = encode(raw, original[:16], key, oodle_compressor(dll, 9))
    if not 24 < len(wrapped) <= 4 * 1024 * 1024:
        raise ValueError('Research wrapper exceeds its separate priority-file bound')
    decoded, meta, _ = decode(wrapped, key, decompress)
    if decoded != raw or wrapped[:16] != original[:16] or not meta['encrypted']:
        raise ValueError('Encrypted roundtrip or original public header differs')
    if meta['decodedBytes'] != len(raw) or meta['crc32'] != f'{zlib.crc32(raw):08X}':
        raise ValueError('Public wrapper trailer differs')
    fields = dict(key=PRIMARY, decoded_size=len(raw), packed_size=len(wrapped),
                  location_flags=FLAGS, payload_name='0B', packed_flag=0)
    one = build_manifest(**fields)
    two = build_two_alias_manifest(alias_key=ALIAS, **fields)
    parsed_one, parsed_two = parse_manifest(one), parse_two_alias_manifest(two)
    if len(one) != 120 or len(two) != 144 or parsed_one != fields or parsed_two != dict(fields, alias_key=ALIAS):
        raise ValueError('Strict priority metadata roundtrip differs')
    records = []
    for i in range(2):
        lookup, decoded_size, packed, location = struct.unpack_from('<QIIQ', two, 96 + 24 * i)
        if decoded_size != len(raw) or packed != len(wrapped) << 1 or location != FLAGS:
            raise ValueError('Alias metadata does not identify the same exact payload')
        records.append(f'{lookup:016X}')
    if records != [f'{PRIMARY:016X}', f'{ALIAS:016X}']:
        raise ValueError('Lookup aliases differ')
    files = {'freestyle/0A': one, 'freestyle/0B': wrapped, 'blacktop/0A': two, 'blacktop/0B': wrapped}
    report = {'researchOnly': True, 'activated': False, 'currentAssetWrites': 0, 'installedWrites': 0,
        'gameProcessAccess': False, 'saltReads': 0, 'localKeyReadForContainedRoundtripOnly': True,
        'keyIncluded': False, 'keyInOutputMemoryCheck': True, 'remoteIO': False,
        'decodedSha256': RAW_SHA, 'decodedBytes': len(raw), 'decodedCrc32': f'{zlib.crc32(raw):08X}',
        'originalWrapperSha256': ORIGINAL_SHA, 'originalPublicHeader16Sha256': digest(original[:16]),
        'stockDecodedSha256': STOCK_DECODED_SHA, 'keyVerifiedAgainstOriginalStock': True,
        'oodleDllSha256': O0DLE_SHA256, 'compressionLevel': 9, 'wrapper': inspect(wrapped),
        'roundtripExact': True, 'maximumResearchWrapperBytes': 4194304, 'archiveSlotLimitUsed': False,
        'lookupAliases': records, 'oneEntryParsed': parsed_one, 'twoAliasParsed': parsed_two,
        'files': {name: {'bytes': len(data), 'sha256': digest(data)} for name, data in files.items()},
        'elapsedMs': round((time.perf_counter() - started) * 1000),
        'protectedCrcProof': 'Derived from exact historical valid-old-file QA054 anchors and independently reviewed equal-length affine delta with unchanged22B suffix; no actual salt reread. See qa074-wnba-index-crc-derivation-pack-proposal.json.',
        'limits': 'No runtime/category/reentry/gameplay claim. Not current assets; exact Rust/native profile guards intentionally reject these research packs until separately reviewed integration.'}
    body = (json.dumps(report, indent=2) + '\n').encode('utf-8')
    if any(key in data for data in [*files.values(), body]):
        raise ValueError('Refusing output with secret-key bytes')
    # Do not create any artifact until all cryptographic, structural and leak checks pass.
    out.mkdir()
    (out / 'freestyle').mkdir(); (out / 'blacktop').mkdir()
    for name, data in files.items():
        create(out / name, data)
    create(out / 'manifest.json', body)
    print(json.dumps({'output': str(out), 'wrapperBytes': len(wrapped), 'wrapperSha256': digest(wrapped),
                      'roundtrip': True, 'oneEntryBytes': len(one), 'twoAliasBytes': len(two), 'keyIncluded': False}))


if __name__ == '__main__':
    main()
