"""Create-only offline Blacktop source trial fixture from the pinned QA058 pack.

Copies existing encrypted bytes verbatim; no keys, process/game access, native
calls, or installation. Outputs are restricted to this project's artifacts.
"""
import hashlib
import json
import os
import pathlib
import stat

from wnba_priority_wad import parse_manifest, build_two_alias_manifest, parse_two_alias_manifest

ROOT = pathlib.Path(__file__).resolve().parent.parent
SOURCE = ROOT / 'artifacts/qa058-wnba-private-wad-wpf-era-20261018'
OUTPUT = ROOT / 'artifacts/qa064-wnba-blacktop-source-20261018'
INDEX_SHA = '6630bedcbf708145fdfb55a48de467265f3de29cd76081e9c24a1c8efd7b84bd'
PAYLOAD_SHA = 'd42b4e4ddfa03a83df799a49a9baef1526a74393f4cfc44a5ce01aa2bf7ce7ee'
TRIAL_SHA = '0c25d0839b11b4af085f587c1baf4e74f821cd34d2b902fc950dbc91e5123cc9'


def ordinary(path):
    for current in (path, *path.parents):
        info = current.lstat()
        if getattr(info, 'st_file_attributes', 0) & 0x400:
            raise ValueError('Reparse inputs/output ancestors are unsupported')


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    if OUTPUT.exists():
        raise ValueError('Fixture output already exists; original preserved')
    ordinary(OUTPUT.parent)
    for name in ('0A', '0B'):
        ordinary(SOURCE / name)
        info = (SOURCE / name).stat()
        if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
            raise ValueError('Fixture source must be an ordinary single-link file')
    index = (SOURCE / '0A').read_bytes()
    payload = (SOURCE / '0B').read_bytes()
    if sha(index) != INDEX_SHA or sha(payload) != PAYLOAD_SHA:
        raise ValueError('Previously verified source fixture changed')
    fields = parse_manifest(index)
    if fields['key'] != 0x673F6D519B5CC655 or fields['decoded_size'] != 18873560:
        raise ValueError('Source index does not describe the pinned mixed403 roster')
    trial = build_two_alias_manifest(alias_key=0xFE5CBA7819830553, **fields)
    parsed = parse_two_alias_manifest(trial)
    if sha(trial) != TRIAL_SHA:
        raise ValueError('Two-alias fixture differs from reviewed metadata')
    report = dict(schema='nba2k27-blacktop-source-trial-fixture-v1', processAccess=False,
                  gameWrites=0, keyReads=0, nativeCalls=0, installed=False,
                  sourceOnly=True, categoryWrites=0, runtimeVerified=False,
                  manifest=dict(file='0A', bytes=len(trial), sha256=sha(trial),
                                lookupHashes=['673F6D519B5CC655', 'FE5CBA7819830553'],
                                fileCount=1, entryCount=2, parsed=parsed),
                  payload=dict(file='0B', bytes=len(payload), sha256=sha(payload), copiedVerbatim=True),
                  sourceFixture=str(SOURCE), portableFolder='Data/wnba-priority-v2/qa064-blacktop-source',
                  approvalMagic='C27WNBT1', approvalBytes=72,
                  limitations=['Default-roster alias affects all default-roster requests during the game lifetime.',
                               'Direct Home-to-Blacktop entry does not itself reload the resident roster.',
                               'Natural Freestyle exit must request the default alias before the Blacktop probe.',
                               '53 appended players remain absent from the native player hash index.',
                               'This fixture does not enable Blacktop or Freestyle category writes.'])
    OUTPUT.mkdir()
    for name, content in [('0A', trial), ('0B', payload),
                          ('fixture-report.json', (json.dumps(report, indent=2) + '\n').encode())]:
        path = OUTPUT / name
        with path.open('xb') as output:
            output.write(content)
            output.flush()
            os.fsync(output.fileno())
        if path.read_bytes() != content:
            raise ValueError(f'Fixture readback differs: {name}')
        path.chmod(stat.S_IREAD)
    print(json.dumps(dict(output=str(OUTPUT), indexSha256=sha(trial), payloadSha256=sha(payload),
                          installed=False, nativeCalls=0, categoryWrites=0)))


if __name__ == '__main__':
    main()
