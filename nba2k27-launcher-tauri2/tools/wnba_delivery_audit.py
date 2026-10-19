"""Read-only V2 roster delivery feasibility audit; no game writes or key reads.

Writes a metadata-only report to --out in this project's artifacts folder.
No roster payload, decryption key, save file or archive patch is produced.
"""
import argparse
import hashlib
import json
import pathlib
import struct
import sys
import time

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from rdat_roster import Roster, get, read_salt  # noqa: E402
from roster_merge_wnba import V2, field_by_name  # noqa: E402
from roster_vcz_codec import oodle_compressor  # noqa: E402
from roster_wrapper_offline import oodle_decoder  # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parent.parent
EXE_SHA = '29a96471c55909fc2bcea6c699c59238656a501c19b50c74ed64b0d842e576a1'
EXPECTED_CLUBS = list(range(300, 312)) + [315, 316, 317]


def decode_index(record):
    if len(record) != 24:
        raise ValueError('Archive index record must be exactly 24 bytes')
    key, decoded, packed, location = struct.unpack('<QIIQ', record)
    return {'key': f'{key:016X}', 'decodedBytes': decoded,
            'packedBytes': packed >> 1, 'packedFlag': packed & 1,
            'position': location >> 23, 'locationFlags': location & ((1 << 23) - 1)}


def sha_file(path):
    h = hashlib.sha256()
    with path.open('rb') as f:
        for block in iter(lambda: f.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--game', type=pathlib.Path, required=True)
    p.add_argument('--scratch', type=pathlib.Path, required=True)
    p.add_argument('--out', type=pathlib.Path, required=True)
    p.add_argument('--levels', default='4,6,8', help='Oodle compression levels, no rewrapping/key access')
    p.add_argument('--variants', default='clubs,national,wpf-era', help='Comma-separated merged variants')
    a = p.parse_args()
    out = a.out.resolve()
    if not out.is_relative_to((ROOT / 'artifacts').resolve()):
        p.error('--out must be inside this project artifacts folder')
    levels = [int(x) for x in a.levels.split(',') if x]
    variants = a.variants.split(',')
    if any(v not in ('clubs', 'national', 'wpf-era') for v in variants):
        p.error('unknown merged variant')
    if any(n < 1 or n > 9 for n in levels):
        p.error('compression levels must be 1 through 9')
    if out.exists():
        p.error('report already exists; choose a new filename')
    game = a.game.resolve(strict=True)
    exe = game / 'NBA2K27.exe'
    if sha_file(exe) != EXE_SHA:
        raise ValueError('Installed executable does not match the audited V2 build')
    provenance = json.loads((a.scratch / 'asset-provenance.json').read_text(encoding='utf-8-sig'))
    src = next(x for x in provenance['results'] if x['path'] == 'roster/learn2k_roster.iff')
    with (game / '0A').open('rb') as f:
        f.seek(src['indexOffset'])
        record = f.read(24)
    if record.hex() != src['indexRecordHex']:
        raise ValueError('Installed roster archive index changed')
    index = decode_index(record)
    for actual, expected in [('key', src['key']), ('position', src['position']),
                             ('decodedBytes', src['decodedSize']), ('packedBytes', src['size'])]:
        if index[actual] != expected:
            raise ValueError(f'Archive provenance differs: {actual}')
    with (game / src['archive']).open('rb') as f:
        f.seek(src['offset'])
        original = f.read(src['size'])
    if hashlib.sha256(original).hexdigest() != src['sha256']:
        raise ValueError('Installed Learn2K stock bytes changed')
    if original != (a.scratch / 'learn2k_roster.iff').read_bytes():
        raise ValueError('Scratch Learn2K input differs from installed game')
    salt = read_salt(exe)
    report = {'schema': 'nba2k27-wnba-delivery-audit-v1', 'gameExecutableSha256': EXE_SHA,
              'gameWrites': 0, 'processAccess': False, 'keyReads': 0,
              'stockIndex': {**index, 'indexOffset': src['indexOffset'],
                             'archive': src['archive'], 'archiveOffset': src['offset'],
                             'stockSha256': src['sha256']}, 'variants': []}
    decompress = oodle_decoder(game / 'data/oodle/oo2core_9_win64.dll')
    for variant in variants:
        raw = (a.scratch / f'merged-{variant}' / 'learn2k_roster.merged.decoded.iff').read_bytes()
        roster = Roster(raw)
        bad = roster.check_crcs(salt)
        if bad:
            raise ValueError(f'{variant}: CRC failures {bad}')
        t = roster.tables[V2['team']]
        idf, kf = (field_by_name(t, V2['team_id']), field_by_name(t, V2['team_type_names'][0]))
        pairs = [(get(r, t.bits, idf.offset, idf.width), get(r, t.bits, kf.offset, kf.width))
                 for r in t.all_rows()]
        clubs = sorted(i for i, k in pairs if k == 29)
        nba = sum(k == 0 for _, k in pairs)
        if clubs != EXPECTED_CLUBS or nba != 30:
            raise ValueError(f'{variant}: mixed team inventory differs')
        item = {'variant': variant, 'teams': t.rows, 'nbaTeams': nba,
                'wnbaClubIds': clubs, 'decodedBytes': len(raw),
                'decodedSha256': hashlib.sha256(raw).hexdigest(), 'crcsVerified': True,
                'compression': []}
        for level in levels:
            started = time.perf_counter()
            compressed = oodle_compressor(game / 'data/oodle/oo2core_9_win64.dll', level)(raw)
            if decompress(compressed, len(raw)) != raw:
                raise ValueError(f'{variant}: Oodle round-trip differs at level {level}')
            wrapped_size = 24 + ((len(compressed) + 7) & ~7)
            item['compression'].append({'level': level, 'compressedBytes': len(compressed),
                'encryptedWrapperBytes': wrapped_size,
                'fitsOriginalArchiveSlot': wrapped_size <= src['size'],
                'compressionRoundTripVerified': True,
                'milliseconds': round((time.perf_counter() - started) * 1000)})
        report['variants'].append(item)
    report['deliveryAssessment'] = {
        'existingProviderOverride': 'No requests reached its file-device hook in QA 0.52; not a working route.',
        'customSave': 'Not established. WPF exports 2KRSTR2 save bundles read-only; raw RDAT IFF is not that format.',
        'archiveDataOnly': 'Possible research route only. Requires changing stock archive data and index metadata, explicit approval, exact backup/restoration, and live validation.',
        'archiveAppend': 'Do not append to an early archive: positions are concatenated across subsequent archives, shifting unrelated resources.',
        'sameSlot': 'Even if compressed bytes fit, decoded length and possibly packed size in index must change. No patch generated.',
        'runtimeToggle': 'Remain disabled; no offline result proves successful game loading or lifecycle safety.'}
    out.parent.mkdir(parents=True, exist_ok=True)
    with out.open('x', encoding='utf-8') as f:
        json.dump(report, f, indent=2)
    print(json.dumps({'report': str(out), 'variants': report['variants'], 'gameWrites': 0,
                      'processAccess': False, 'keyReads': 0}, indent=2))


if __name__ == '__main__':
    main()
