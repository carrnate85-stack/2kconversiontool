"""Create-only offline private WAD fixture from an already verified WPF-era IFF.

No decryption, compression, keys, process access, game writes, or registration.
The encrypted candidate is copied verbatim into a new project artifacts folder.
Matching public trailer/decoded metadata does not independently verify ciphertext;
the pinned candidate SHA ties it to the prior verified candidate artifact.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import stat
import struct
import zlib

from wnba_priority_wad import build_manifest, parse_manifest
from roster_wrapper_offline import inspect

ROOT = pathlib.Path(__file__).resolve().parent.parent
DECODED_SHA = '8827184c6bf3ec8d7a833db8f71880ee9e9af6c35609d26ae2d0c2559ed89fe5'
CANDIDATE_SHA = 'd42b4e4ddfa03a83df799a49a9baef1526a74393f4cfc44a5ce01aa2bf7ce7ee'
STOCK_RECORD = '55c65c9b516d3f67e01f1901c0c935001008210ba0ce2000'
LOOKUP_HASH = 0x673F6D519B5CC655
FLAGS = 0x210810


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def verified_pair(candidate: bytes, decoded: bytes) -> dict:
    if sha(decoded) != DECODED_SHA or len(decoded) != 18873560:
        raise ValueError('decoded file differs from pinned WPF-era merge')
    if sha(candidate) != CANDIDATE_SHA or len(candidate) != 1762144:
        raise ValueError('encrypted file differs from prior verified level9 candidate')
    meta = inspect(candidate)
    if not meta['encrypted']:
        raise ValueError('expected existing encrypted wrapper')
    if meta['decodedBytes'] != len(decoded) or meta['crc32'] != f'{zlib.crc32(decoded):08X}':
        raise ValueError('wrapper public trailer differs from decoded candidate')
    return meta


def ordinary_input(path: pathlib.Path) -> pathlib.Path:
    # Reject alternate extensions before reading; this tool has no key input.
    if path.suffix.lower() not in ('.iff', '.json'):
        raise ValueError('only explicit IFF or JSON inputs are accepted')
    result = path.resolve(strict=True)
    info = result.stat()
    if not stat.S_ISREG(info.st_mode) or info.st_size > 64 * 1024 * 1024:
        raise ValueError('input must be an ordinary file within offline size bounds')
    if getattr(info, 'st_file_attributes', 0) & 0x400:
        raise ValueError('reparse input files are unsupported')
    return result


def output_directory(path: pathlib.Path) -> pathlib.Path:
    out = path.resolve()
    artifacts = (ROOT / 'artifacts').resolve(strict=True)
    if not out.is_relative_to(artifacts) or out == artifacts:
        raise ValueError('output must be a new folder inside project artifacts')
    if out.exists():
        raise ValueError('output already exists; fixtures are create-only')
    if not out.parent.is_dir():
        raise ValueError('output parent must already exist')
    return out


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--candidate', type=pathlib.Path, required=True)
    p.add_argument('--decoded', type=pathlib.Path, required=True)
    p.add_argument('--provenance', type=pathlib.Path, required=True)
    p.add_argument('--verified-plan', type=pathlib.Path, required=True)
    p.add_argument('--out', type=pathlib.Path, required=True)
    a = p.parse_args()
    out = output_directory(a.out)
    provenance = json.loads(ordinary_input(a.provenance).read_text(encoding='utf-8-sig'))
    rows = [x for x in provenance['results'] if x['path'] == 'roster/learn2k_roster.iff']
    if len(rows) != 1 or rows[0]['indexRecordHex'] != STOCK_RECORD:
        raise ValueError('stock roster provenance differs from pinned native record')
    key, _, packed, location = struct.unpack('<QIIQ', bytes.fromhex(STOCK_RECORD))
    if key != LOOKUP_HASH or location & 0x7fffff != FLAGS or packed & 1:
        raise ValueError('stock native metadata differs')
    plan = json.loads(ordinary_input(a.verified_plan).read_text(encoding='utf-8-sig'))
    if (plan['variant'] != 'wpf-era' or plan['payload']['candidateSha256'] != CANDIDATE_SHA
            or plan['payload']['decodedSha256'] != DECODED_SHA):
        raise ValueError('existing verification artifact differs from pinned candidate')
    decoded_path = ordinary_input(a.decoded)
    decoded = decoded_path.read_bytes()
    # Validate decoded identity before opening/copying encrypted candidate.
    if sha(decoded) != DECODED_SHA:
        raise ValueError('decoded WPF-era identity differs')
    candidate_path = ordinary_input(a.candidate)
    candidate = candidate_path.read_bytes()
    meta = verified_pair(candidate, decoded)
    manifest = build_manifest(key=key, decoded_size=len(decoded), packed_size=len(candidate),
                              location_flags=FLAGS, payload_name='0B', packed_flag=0)
    parsed = parse_manifest(manifest)
    report = {
        'schema': 'nba2k27-private-wnba-wad-fixture-v1', 'variant': 'wpf-era',
        'offlineFixtureOnly': True, 'runtimeIntegrated': False,
        'gameplayCompatibilityVerified': False, 'processAccess': False,
        'gameWrites': 0, 'keyReads': 0, 'recompressed': False, 'decrypted': False,
        'candidateSource': str(candidate_path), 'decodedSource': str(decoded_path),
        'decodedBytes': len(decoded), 'decodedSha256': sha(decoded),
        'decodedCrc32': meta['crc32'], 'wrapperTrailerMatchesDecoded': True,
        'candidatePinnedToPriorVerifiedArtifact': True,
        'manifest': {'file': '0A', 'bytes': len(manifest), 'sha256': sha(manifest),
                     'lookupHash': f'{key:016X}', 'locationFlags': f'{FLAGS:06X}',
                     'fileCount': 1, 'entryCount': 1, 'sizeUnit': 1,
                     'payloadPosition': 0, 'parsed': parsed},
        'payload': {'file': '0B', 'bytes': len(candidate), 'sha256': sha(candidate),
                    'copiedVerbatim': True},
        'limitations': ['Native registration and path resolution have not been exercised.',
                        '53 appended players remain absent from the native player hash index.'],
    }
    out.mkdir()
    for name, content in [('0A', manifest), ('0B', candidate),
                          ('fixture-report.json', (json.dumps(report, indent=2) + '\n').encode())]:
        path = out / name
        with path.open('xb') as file:
            file.write(content)
        if path.read_bytes() != content:
            raise ValueError(f'fixture readback differs for {name}')
        path.chmod(stat.S_IREAD)
    print(json.dumps({'output': str(out), 'manifestSha256': sha(manifest),
                      'payloadSha256': sha(candidate), 'processAccess': False,
                      'gameWrites': 0, 'keyReads': 0, 'runtimeIntegrated': False}, indent=2))


if __name__ == '__main__':
    main()
