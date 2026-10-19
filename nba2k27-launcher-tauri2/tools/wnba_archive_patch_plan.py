"""Build a metadata-only WNBA archive patch PLAN, never apply it.

The installed game/index/archive and candidate are opened read-only. A local key
file is used only to validate the candidate wrapper; key bytes are never logged,
included in the plan, copied or emitted. The output is only a JSON audit/plan.
No writer, patch executor or runtime feature enablement exists in this module.
"""
import argparse
import hashlib
import json
import pathlib
import struct
import sys
import zlib

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from wnba_delivery_audit import decode_index, EXE_SHA, EXPECTED_CLUBS, ROOT, sha_file
from roster_vcz_codec import decode
from roster_wrapper_offline import oodle_decoder
from rdat_roster import Roster, get, read_salt
from roster_merge_wnba import V2, field_by_name

ORIGINAL_RECORD = bytes.fromhex('55c65c9b516d3f67e01f1901c0c935001008210ba0ce2000')
ORIGINAL_SHA = '69445e272c0c02f6905a99bc0926e2d76e331ab7925f3621b799ac33fd6c28c2'
INDEX_OFFSET = 23065320
PAYLOAD_OFFSET = 1006445174
DECODED_SHA = {
    'clubs': '1530bbec733b8d2437616aad5e6ca34c81fdfbcdabac6e9c2523c9ab02f55b86',
    'national': 'e3d944f4e2876f99f2cedd93a4d3f586f1e29df0a544e74d16e381efe176f95a',
    'wpf-era': '8827184c6bf3ec8d7a833db8f71880ee9e9af6c35609d26ae2d0c2559ed89fe5'}


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def plan_ranges(record, original, wrapped, decoded, expected_record=ORIGINAL_RECORD,
                expected_original_sha=ORIGINAL_SHA, expected_decoded_sha=None):
    """Pure range/byte verifier; caller must independently decrypt/parse decoded."""
    if record != expected_record:
        raise ValueError('Stock index record changed')
    before = decode_index(record)
    if len(original) != before['packedBytes'] or digest(original) != expected_original_sha:
        raise ValueError('Stock payload identity changed')
    if expected_decoded_sha is not None and digest(decoded) != expected_decoded_sha:
        raise ValueError('Decoded candidate is not the audited merged roster')
    if (len(wrapped) < 32 or wrapped[:4] != b'\x1f\x8b\x21\x84'
            or wrapped[:16] != original[:16] or (len(wrapped) - 24) % 8):
        raise ValueError('Candidate encrypted wrapper shape differs')
    if len(wrapped) > before['packedBytes']:
        raise ValueError('Candidate exceeds original archive slot; do not shift archive positions')
    crc, size = struct.unpack('<II', wrapped[-8:])
    if size != len(decoded) or crc != zlib.crc32(decoded):
        raise ValueError('Candidate authored decoded length/CRC mismatch')
    after = bytearray(record)
    struct.pack_into('<II', after, 8, len(decoded), (len(wrapped) << 1) | before['packedFlag'])
    # No other index key, address or flag can change.
    if after[:8] != record[:8] or after[16:] != record[16:]:
        raise ValueError('Patch plan would change resource identity/position')
    slot = wrapped + bytes(before['packedBytes'] - len(wrapped))
    return {'schema': 'nba2k27-wnba-data-patch-plan-v1', 'applyImplemented': False,
            'requiresExplicitGameFileApproval': True, 'gameplayCompatibilityVerified': False,
            'processAccess': False, 'gameWrites': 0, 'keyIncluded': False,
            'sameFileLengthsRequired': True, 'resourcePositionsUnchanged': True,
            'index': {'archive': '0A', 'offset': INDEX_OFFSET, 'length': 24,
                      'expectedHex': record.hex(), 'replacementHex': bytes(after).hex()},
            'payload': {'archive': '0B', 'offset': PAYLOAD_OFFSET,
                        'slotBytes': len(original), 'candidateBytes': len(wrapped),
                        'zeroPadBytes': len(slot) - len(wrapped),
                        'expectedSha256': digest(original), 'replacementSlotSha256': digest(slot),
                        'candidateSha256': digest(wrapped), 'decodedSha256': digest(decoded),
                        'decodedBytes': len(decoded), 'decodedCrc32': f'{crc:08X}'},
            'requiredTransaction': [
                'Game and every launcher closed, exact executable/index/archive identities pinned.',
                'Durably back up original 24-byte entry and entire original payload slot before any write.',
                'Journal both ranges and verify expected bytes immediately before each mutation.',
                'Read back exact slot hash/index fields; preserve total archive lengths.',
                'On exit or failure restore only exact owned replacements; refuse foreign changes.',
                'Interrupted two-file publication must recover before Steam may start.']}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--game', type=pathlib.Path, required=True)
    p.add_argument('--candidate', type=pathlib.Path, required=True)
    p.add_argument('--variant', choices=DECODED_SHA, required=True)
    p.add_argument('--key-file', type=pathlib.Path, required=True)
    p.add_argument('--out', type=pathlib.Path, required=True)
    a = p.parse_args()
    output = a.out.resolve()
    if not output.is_relative_to((ROOT / 'artifacts').resolve()) or output.exists():
        p.error('--out must be a new report inside project artifacts')
    game = a.game.resolve(strict=True)
    if sha_file(game / 'NBA2K27.exe') != EXE_SHA:
        raise ValueError('Unrecognized game build')
    with (game / '0A').open('rb') as f:
        f.seek(INDEX_OFFSET)
        record = f.read(24)
    if record != ORIGINAL_RECORD:
        raise ValueError('Stock index changed')
    with (game / '0B').open('rb') as f:
        f.seek(PAYLOAD_OFFSET)
        original = f.read(decode_index(record)['packedBytes'])
    if a.candidate.stat().st_size > len(original):
        raise ValueError('Candidate exceeds original archive slot')
    wrapped = a.candidate.read_bytes()
    key = a.key_file.read_bytes()
    if len(key) != 32:
        raise ValueError('Unexpected local validation key length')
    decoded, _, _ = decode(wrapped, key, oodle_decoder(game / 'data/oodle/oo2core_9_win64.dll'))
    roster = Roster(decoded)
    if roster.check_crcs(read_salt(game / 'NBA2K27.exe')):
        raise ValueError('Merged RDAT CRC validation failed')
    t = roster.tables[V2['team']]
    idf, kf = (field_by_name(t, V2['team_id']), field_by_name(t, V2['team_type_names'][0]))
    teams = [(get(r, t.bits, idf.offset, idf.width), get(r, t.bits, kf.offset, kf.width)) for r in t.all_rows()]
    if sorted(i for i, k in teams if k == 29) != EXPECTED_CLUBS or sum(k == 0 for _, k in teams) != 30:
        raise ValueError('Mixed team inventory differs')
    result = plan_ranges(record, original, wrapped, decoded, expected_decoded_sha=DECODED_SHA[a.variant])
    result['variant'] = a.variant
    result['teams'] = t.rows
    result['risk'] = {'clubs': 'No appended player IDs; strict current verifier also needs USA361 and must not be weakened speculatively.',
        'national': '11 appended national copy players remain absent from player hash index.',
        'wpf-era': '53 appended copy players remain absent from player hash index.'}[a.variant]
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open('x', encoding='utf-8') as f:
        json.dump(result, f, indent=2)
    print(json.dumps({'report': str(output), 'gameWrites': 0, 'applyImplemented': False}))


if __name__ == '__main__':
    main()
