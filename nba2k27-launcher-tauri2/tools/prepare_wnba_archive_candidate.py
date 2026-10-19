"""Offline local-only level9 candidate; never installs or modifies game files.

Reads an explicit local validation key, never prints/copies it. Only writes a
new encrypted candidate below project artifacts/qa054-wnba-archive-candidate.
"""
import argparse
import hashlib
import json
import pathlib
import sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from roster_vcz_codec import encode, decode, oodle_compressor
from roster_wrapper_offline import oodle_decoder
from wnba_archive_patch_plan import DECODED_SHA, ORIGINAL_SHA, ROOT


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--scratch', type=pathlib.Path, required=True)
    p.add_argument('--key-file', type=pathlib.Path, required=True)
    p.add_argument('--oodle', type=pathlib.Path, required=True)
    a = p.parse_args()
    destination = ROOT / 'artifacts/qa054-wnba-archive-candidate/learn2k_roster.level9.iff'
    if destination.exists():
        raise ValueError('Candidate already exists; refusing overwrite')
    original = (a.scratch / 'learn2k_roster.iff').read_bytes()
    raw = (a.scratch / 'merged-wpf-era/learn2k_roster.merged.decoded.iff').read_bytes()
    if hashlib.sha256(original).hexdigest() != ORIGINAL_SHA:
        raise ValueError('Original stock wrapper differs')
    if hashlib.sha256(raw).hexdigest() != DECODED_SHA['wpf-era']:
        raise ValueError('Audited WPF-structure decoded roster differs')
    key = a.key_file.read_bytes()
    if len(key) != 32:
        raise ValueError('Unexpected local key length')
    wrapped = encode(raw, original[:16], key, oodle_compressor(a.oodle, 9))
    if len(wrapped) > len(original):
        raise ValueError('New compression output exceeds original archive slot')
    back, _, _ = decode(wrapped, key, oodle_decoder(a.oodle))
    if back != raw:
        raise ValueError('Encrypted wrapper round-trip differs')
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open('xb') as f:
        f.write(wrapped)
    print(json.dumps({'candidate': str(destination), 'bytes': len(wrapped),
        'sha256': hashlib.sha256(wrapped).hexdigest(), 'decodedRoundTripVerified': True,
        'gameWrites': 0, 'keyIncluded': False}))


if __name__ == '__main__':
    main()
