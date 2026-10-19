"""Fresh research-only equal-length index repair. Never read keys or CRC salt."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import struct
import zipfile
import zlib

from rdat_roster import Roster
from prepare_wnba_player_index_research import EXPECTED, read
from crc_equal_length_research import replace_equal_prefix_crc


def sha(data):
    return hashlib.sha256(data).hexdigest()


def layout(raw):
    """Accept only the exact one-member ordinary stored ZIP layout."""
    assert raw[:4] == b'PK\x03\x04'
    version, flags, method = struct.unpack_from('<HHH', raw, 4)
    assert version <= 20 and flags == 0 and method == 0
    crc, compressed, expanded = struct.unpack_from('<III', raw, 14)
    name_len, extra_len = struct.unpack_from('<HH', raw, 26)
    assert extra_len == 0 and raw[30:30 + name_len] == b'roster.RDAT'
    start = 30 + name_len
    assert compressed == expanded and start + compressed <= len(raw)
    central = start + compressed
    assert raw[central:central + 4] == b'PK\x01\x02'
    assert struct.unpack_from('<HH', raw, central + 8) == (0, 0)
    assert struct.unpack_from('<III', raw, central + 16) == (crc, compressed, expanded)
    cn, ce, cc, disk, internal, external, local = struct.unpack_from('<HHHHHII', raw, central + 28)
    assert (cn, ce, cc, disk, internal, local) == (name_len, 0, 0, 0, 0, 0)
    assert raw[central + 46:central + 46 + cn] == b'roster.RDAT'
    end = central + 46 + cn
    assert raw[end:end + 4] == b'PK\x05\x06' and len(raw) == end + 22
    assert struct.unpack_from('<HHHHIIH', raw, end + 4) == (0, 0, 1, 1, end - central, central, 0)
    data = raw[start:central]
    assert zlib.crc32(data) == crc
    return start, central, data


def index_span(rdat):
    """Walk the parsed BNDL stream; never locate a chunk with byte-find."""
    p, found = 0x28, []
    while p < len(rdat):
        assert rdat[p:p + 4] == b'BNDL' and p + 60 <= len(rdat)
        size = struct.unpack_from('<I', rdat, p + 4)[0]
        h = struct.unpack_from('<13I', rdat, p + 8)
        end = p + 60 + size
        assert end <= len(rdat) and end % 8 == 0
        if h[0] == 0x222E3502:
            assert h[1] == 37468 and h[5] == 0 and h[6] == h[1]
            assert h[1] + h[2] <= size
            found.append((p, p + 60, p + 8 + 28, h))
        p = end
    assert p == len(rdat) and len(found) == 1
    return found[0]


def build(base_raw, mixed_raw, payload):
    assert sha(base_raw) == EXPECTED['base'] and sha(mixed_raw) == EXPECTED['mixed']
    bs, bc, bd = layout(base_raw)
    ms, mc, md = layout(mixed_raw)
    bp, bi, bcrc, bh = index_span(bd)
    mp, mi, mcrc, mh = index_span(md)
    assert bh == mh and bd[bi:bi + 37468] == md[mi:mi + 37468]
    assert len(payload) == 37468
    changed = bytearray(mixed_raw)
    changed[ms + mi:ms + mi + len(payload)] = payload
    next_crc = replace_equal_prefix_crc(mh[7], md[mi:mi + 37468], payload, 22)
    struct.pack_into('<I', changed, ms + mcrc, next_crc)
    zip_crc = zlib.crc32(changed[ms:mc])
    struct.pack_into('<I', changed, 14, zip_crc)
    struct.pack_into('<I', changed, mc + 16, zip_crc)
    allowed = [(14, 18), (mc + 16, mc + 20), (ms + mcrc, ms + mcrc + 4), (ms + mi, ms + mi + 37468)]
    differences = [i for i, (a, b) in enumerate(zip(mixed_raw, changed)) if a != b]
    assert len(changed) == len(mixed_raw) and differences
    assert all(any(start <= i < end for start, end in allowed) for i in differences)
    layout(changed)
    reparsed = Roster(changed)
    original = Roster(mixed_raw)
    assert reparsed.tables['222e3502'].data == payload
    assert reparsed.order == original.order
    for tid in original.order:
        a, b = original.tables[tid], reparsed.tables[tid]
        assert a.aux == b.aux and a.rows == b.rows and a.bits == b.bits
        if tid != '222e3502':
            assert a.header == b.header and a.data == b.data
    assert original.directory.header == reparsed.directory.header and original.directory.body == reparsed.directory.body
    return bytes(changed), {'baseIndexHeaderMatchesMixed': True, 'baseIndexPayloadMatchesMixed': True,
        'headerWords5And6': [mh[5], mh[6]], 'oldDataCrc': f'{mh[7]:08X}', 'newDataCrc': f'{next_crc:08X}',
        'allowedAbsoluteSpans': allowed, 'changedBytes': len(differences), 'otherChunksByteExact': True,
        'allOtherFileBytesExact': True, 'zipCrcVerified': True, 'reparsed': True}


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--scratch', type=Path, required=True)
    p.add_argument('--out', type=Path, required=True)
    a = p.parse_args()
    root = Path(__file__).resolve().parents[1] / 'artifacts'
    assert not a.out.exists() and a.out.resolve().is_relative_to(root.resolve())
    base = a.scratch / 'decoded/learn2k_roster.decoded.iff'
    mixed = a.scratch / 'merged-wpf-era/learn2k_roster.merged.decoded.iff'
    payload = (root / 'qa074-wnba-index-research-candidate/index-rdat-bits.bin').read_bytes()
    manifest = json.loads((root / 'qa074-wnba-index-research-candidate/manifest.json').read_text(encoding='utf-8'))
    assert sha(payload) == manifest['candidate']['rdatPayloadSha256']
    result, proof = build(base.read_bytes(), mixed.read_bytes(), payload)
    a.out.mkdir()
    target = a.out / 'learn2k_roster.index-research.decoded.iff'
    with target.open('xb') as f:
        f.write(result); f.flush(); os.fsync(f.fileno())
    keys, rebuilt, table, verification = read(target, 'candidate')
    assert verification['rebuiltMatchesStoredAll3Regions'] and verification['players'] == 5563
    report = {'researchOnly': True, 'existingPackWrites': 0, 'devAssetWrites': 0, 'saltReads': 0, 'keyReads': 0,
        'gameProcessAccess': False, 'candidateSha256': sha(result), 'candidateBytes': len(result), 'proof': proof,
        'indexVerification': verification, 'rawCandidateManifest': str(root / 'qa074-wnba-index-research-candidate/manifest.json'),
        'crcValidity': 'CONDITIONAL: equal-length affine delta preserves the original salted CRC iff original file CRC was valid and checksum suffix is the known 22-byte suffix. No salt read; no independent actual salted CRC validation in this research run.',
        'limits': 'No runtime/gameplay/category/reentry claim; normal native loader may rebuild index. Not integrated or promoted. Existing source files remain unchanged.'}
    with (a.out / 'manifest.json').open('x', encoding='utf-8') as f:
        json.dump(report, f, indent=2); f.flush(); os.fsync(f.fileno())
    print(json.dumps({'output': str(a.out), 'bytes': len(result), 'sha256': sha(result), 'lookupPass': 5563, 'crcConditional': True}))


if __name__ == '__main__':
    main()
