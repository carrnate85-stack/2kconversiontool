"""Read-only disk audit of a possible Blacktop default-roster priority alias.

No keys, process access, payload output, manifest output, or game writes.
Writes only a create-only metadata report below this project's artifacts.
"""
import argparse
import hashlib
import json
import pathlib
import struct

ROOT = pathlib.Path(__file__).resolve().parent.parent
DEFAULT = '5305831978ba5cfe90f18801d06f64001008217b12dc2000'
LEARN = '55c65c9b516d3f67e01f1901c0c935001008210ba0ce2000'


class Image:
    def __init__(self, path):
        self.file = path.open('rb')
        self.file.seek(0x3c)
        pe = struct.unpack('<I', self.file.read(4))[0]
        self.file.seek(pe)
        head = self.file.read(24)
        if head[:4] != b'PE\0\0' or struct.unpack_from('<I', head, 8)[0] != 0x6AC53765:
            raise ValueError('Wrong executable PE timestamp')
        optional = self.file.read(struct.unpack_from('<H', head, 20)[0])
        self.base = struct.unpack_from('<Q', optional, 24)[0]
        if struct.unpack_from('<I', optional, 56)[0] != 0x35BDE000:
            raise ValueError('Wrong executable image size')
        self.sections = []
        for _ in range(struct.unpack_from('<H', head, 6)[0]):
            entry = self.file.read(40)
            self.sections.append(struct.unpack_from('<IIII', entry, 8))

    def read(self, rva, count):
        for _, va, size, raw in self.sections:
            if va <= rva and rva + count <= va + size:
                self.file.seek(raw + rva - va)
                data = self.file.read(count)
                if len(data) != count:
                    raise ValueError('Short image read')
                return data
        raise ValueError('RVA not file backed')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--game', type=pathlib.Path, default=pathlib.Path(r'C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27'))
    parser.add_argument('--out', type=pathlib.Path, required=True)
    args = parser.parse_args()
    out = args.out.resolve()
    if out.exists() or not out.is_relative_to((ROOT / 'artifacts').resolve()):
        parser.error('Output must be create-only below project artifacts')
    records = []
    with (args.game / '0A').open('rb') as source:
        for path, offset, expected in [('roster/learn2k_roster.iff', 23065320, LEARN),
                                       ('roster/roster.iff', 56773632, DEFAULT)]:
            source.seek(offset)
            raw = source.read(24)
            if raw.hex() != expected:
                raise ValueError(f'Current installed archive record differs: {path}')
            key, decoded, packed, location = struct.unpack('<QIIQ', raw)
            records.append(dict(path=path, indexOffset=offset, rawEntry=raw.hex(),
                                lookupHash=f'{key:016X}', decodedBytes=decoded,
                                packedBytes=packed >> 1, flags=f'{location & 0x7fffff:X}'))
    image = Image(args.game / 'NBA2K27.exe')
    restore = image.read(0x236FB50, 26)
    if restore[:3] != b'\x48\x8d\x0d' or restore[21] != 0xe9:
        raise ValueError('Restore thunk changed')
    asset_rva = 0x236FB50 + 7 + struct.unpack_from('<i', restore, 3)[0]
    if image.read(asset_rva, 18) != b'roster/roster.iff\0':
        # Exact string is 18 bytes including the terminator.
        raise ValueError('Restore thunk no longer requests default roster')
    table = image.read(0x7425F4C, 24)
    targets = [0x7425F4C + struct.unpack_from('<i', table, i * 4)[0] for i in range(6)]
    if targets != [0x230BE18, 0x230BE26, 0x230BE1F, 0x230BEA4, 0x230BE7A, 0x230BE83]:
        raise ValueError('Blacktop category dispatch changed')
    switch = image.read(0x230BDD0, 0xfb)
    if not all(code in switch for code in [b'\xbe\x01\0\0\0', b'\xbe\x02\0\0\0', b'\xbe\0\0\0\x08']):
        raise ValueError('Blacktop immediate category masks changed')
    private = (ROOT / 'artifacts/qa058-wnba-private-wad-wpf-era-20261018/0A').read_bytes()
    if hashlib.sha256(private).hexdigest() != '6630bedcbf708145fdfb55a48de467265f3de29cd76081e9c24a1c8efd7b84bd':
        raise ValueError('Existing private metadata changed')
    proposed = bytearray(private)
    struct.pack_into('<I', proposed, 32, 2)
    alias = bytearray(private[96:120])
    struct.pack_into('<Q', alias, 0, int('FE5CBA7819830553', 16))
    proposed.extend(alias)
    report = dict(schema='nba2k27-blacktop-priority-alias-disk-audit-v1', processAccess=False,
                  gameWrites=0, keyReads=0, payloadWrites=0, manifestWrites=0,
                  stockRecords=records, restoreThunk=dict(rva='236FB50', asset='roster/roster.iff', bytes=restore.hex()),
                  blacktopDelegateEntry=dict(slot='74CDE88', target='236E510', bytes=image.read(0x236E510, 1).hex()),
                  category=dict(switchRva='230BDD0', sha256=hashlib.sha256(switch).hexdigest(), bytes=switch.hex(),
                                targets=[f'{v:X}' for v in targets], dataOnlyWnbaMask=False),
                  hypotheticalMetadata=dict(bytes=len(proposed), entries=2, files=1,
                                            sha256=hashlib.sha256(proposed).hexdigest(), emitted=False),
                  assessment=['Default roster alias can point both stock lookup hashes at the same already verified mixed payload.',
                              'This alias is global to all default-roster consumers, not scoped to Blacktop.',
                              'Blacktop entry does not reload a roster; current Home registration occurs after the initial stock roster is resident.',
                              'A natural Freestyle exit or Play Now roster reload must be proven before claiming first Blacktop entry has mixed rows.',
                              'No data-only WNBA category dispatch established: stock cases use instruction-immediate NBA/classic/all-time masks.',
                              'Existing source ownership profile hardcodes one entry and its metadata hash; two-entry metadata requires independent new pins and guards.'])
    with out.open('x', encoding='utf-8') as output:
        output.write(json.dumps(report, indent=2) + '\n')
    print(json.dumps(dict(report=str(out), recordsVerified=len(records), manifestEmitted=False)))


if __name__ == '__main__':
    main()
