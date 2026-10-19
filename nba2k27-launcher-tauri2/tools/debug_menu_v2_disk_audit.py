"""Pinned V2 debug-feature structure audit. Ordinary disk reads only."""
import argparse
import bisect
import hashlib
import json
import mmap
import os
from pathlib import Path
import struct
from wnba_v2_route_audit import Image, find_vtable

ROOT = Path(__file__).resolve().parents[1]
EXE_SHA = '29a96471c55909fc2bcea6c699c59238656a501c19b50c74ed64b0d842e576a1'
FEATURES = (
    ('ENABLE_DEBUG_MENUS', 0x8EB3D32B, 0x39E3537, 0xB5246A8, 0x7C04398),
    ('MENU_ENABLE_DEBUG_MENU_OPTION_IN_CLEAN', 0x976369F2, 0x39E3603, 0xB524738, 0x7C049F8),
    ('NAVIGATION_MENU_SHOW_ALL_ITEMS', 0x5B4DD268, 0x39E3625, 0xB524750, 0x7C04B08),
)


def audit(path):
    with path.open('rb') as stream, mmap.mmap(stream.fileno(), 0, access=mmap.ACCESS_READ) as data:
        if hashlib.sha256(data).hexdigest() != EXE_SHA:
            raise ValueError('Exact V2 disk SHA256 required')
        image = Image(data)
        pe = struct.unpack_from('<I', data, 60)[0]
        rva, size = struct.unpack_from('<II', data, pe+24+112+24)
        entries = [struct.unpack('<III', image.read(rva+n, 12)) for n in range(0, size, 12)]
        starts = [entry[0] for entry in entries]
        methods = []
        for name, target, leaf_size in (
            ('CanonicalManagerGetter', 0x4AF80, None), ('ManagerConstructor', 0x4B700, None),
            ('BooleanFeatureConstructor', 0x4BC60, None), ('ManagerGetBool', 0x4E110, None),
            ('SortedMapGetBool', 0x4BB0BB0, None), ('SortedMapFind', 0x4BAE860, 71),
            ('BooleanElementRead', 0x4CD2EC0, 43), ('MapCreateBoolean', 0x4BAB260, None),
            ('MapCreateBooleanWrapper', 0x4BACFC0, None), ('FeatureRegistrationInitializer', 0x39DDFD0, None),
        ):
            entry = entries[bisect.bisect_right(starts, target)-1]
            if leaf_size is None and entry[0] != target:
                raise ValueError('Full function start changed')
            count = leaf_size or entry[1]-entry[0]
            if not 0 < count <= 65536:
                raise ValueError('Unexpected complete function size')
            methods.append(dict(name=name, rva=target, bytes=count,
                                sha256=hashlib.sha256(image.read(target, count)).hexdigest(),
                                hasUnwind=leaf_size is None))
        # Exact decoded native getter proves the global, override and static fallback.
        for at, target, opcode in ((0x4AF85, 0xA1F1248, b'\x48\x8b\x05'),
                                  (0x4AFF2, 0xA1F1250, b'\x48\x8b\x0d'),
                                  (0x4AFB7, 0xA1F1260, b'\x48\x8d\x35')):
            code = image.read(at, 7)
            if code[:3] != opcode or at+7+struct.unpack_from('<i', code, 3)[0] != target:
                raise ValueError('Manager getter field proof differs')
        fields = ((0x4E136, '488d8bf8000000'), (0x4BB0BC2, '448b4918'),
                  (0x4BB0BCB, '4c8b11'), (0x4BB0BEE, '498b04f2'),
                  (0x4BB0BF2, '3910'), (0x4CD2EE2, '0fb6440108'))
        for at, expected in fields:
            if image.read(at, len(bytes.fromhex(expected))) != bytes.fromhex(expected):
                raise ValueError('Native map/entry field proof differs')
        features = []
        for name, key, at, obj, vtable in FEATURES:
            code = image.read(at, 34)
            if (code[:3] != bytes.fromhex('488d0d')
                or at+7+struct.unpack_from('<i', code, 3)[0] != obj
                or code[7] != 0xBA or struct.unpack_from('<I', code, 8)[0] != key
                or code[12:15] != bytes.fromhex('4531c0') or code[15] != 0xE8
                or at+20+struct.unpack_from('<i', code, 16)[0] != 0x4BC60
                or code[20:23] != bytes.fromhex('488d05')
                or at+27+struct.unpack_from('<i', code, 23)[0] != vtable
                or find_vtable(image, 'VCFEATURECODE_'+name) != vtable):
                raise ValueError('Feature constructor/RTTI/default proof differs')
            features.append(dict(name=name, key=key, objectRva=obj, vtableRva=vtable,
                                 default=False, registrationRva=at, registrationBytes=code.hex()))
        return dict(schema='debug-menu-v2-native-profile-v1', imageSha256=EXE_SHA,
                    processAccess=False, gameWrites=0, gameCalls=0, methods=methods, features=features,
                    manager=dict(defaultGlobalRva=0xA1F1248, overrideGlobalRva=0xA1F1250,
                                 staticFallbackRva=0xA1F1260, mapOffset=0xF8, pointerTableOffset=0,
                                 countOffset=0x110, pointerStride=8,
                                 canonicalSelection='nonzero override, otherwise default; never construct missing singleton',
                                 type='Critical-section-backed plain manager; no manager RTTI/vtable assumed'),
                    entry=dict(keyOffset=0, packedOffset=4, valueOffset=8, metadataBytes=9,
                               typeMask=31, booleanType=11, countShift=5, countMask=0x3FFFFFF,
                               requiredCount=1, allowedValues=[0, 1]),
                    nativeLookup='Unsigned binary search over sorted pointer table; inspect duplicate neighbors only for the three allowed keys',
                    remainingLiveChecks=['Exact owned offline lease and loaded image method hashes',
                                         'Canonical manager identity stable; static fallback or readable private allocation',
                                         'Table/count/three binary-search paths and target neighbors stable twice',
                                         'Exactly three allowlisted entry IDs/type/count/Boolean values; no unrelated value reads'],
                    productionEnabled=False)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exe', type=Path, default=Path(r'C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27\NBA2K27.exe'))
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    out = args.out.resolve()
    if out.exists() or not out.is_relative_to((ROOT/'artifacts').resolve()):
        raise ValueError('new project artifact required')
    report = audit(args.exe)
    with out.open('x', encoding='utf-8') as stream:
        json.dump(report, stream, indent=2)
        stream.flush()
        os.fsync(stream.fileno())
    print(json.dumps({'report': str(out), 'methods': len(report['methods']), 'features': len(report['features']), 'processAccess': False}))


if __name__ == '__main__':
    main()
