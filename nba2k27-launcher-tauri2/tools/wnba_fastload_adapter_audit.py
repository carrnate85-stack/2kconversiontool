"""Disk-only audit of native VCFAST_LOAD file-device adapters on pinned V2."""
import argparse
import hashlib
import json
import pathlib
import struct
from wnba_controlflow_audit import (Image, disassemble, c_string,
    data_occurrences, absolute_pointer_occurrences, common_rip_references)
from wnba_delivery_audit import ROOT, EXE_SHA

NAMES = ('VCFAST_LOAD', 'VCFAST_LOAD_WIN32', 'VCFAST_LOAD_FILE_DEVICE',
         'VCFAST_LOAD_FILE_DEVICE_BLOCKING_READ',
         'VCFAST_LOAD_GET_FILE_INFO_FILE_DEVICE',
         'VCFAST_LOAD_DEVELOPMENT_FILES_MANIFEST', 'VCFAST_LOAD_WADFILES_MANIFEST',
         'VCFAST_LOAD_SET_MANIFEST', 'VCFAST_LOAD_MANIFEST')


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--exe', required=True, type=pathlib.Path)
    p.add_argument('--out', required=True, type=pathlib.Path)
    p.add_argument('--skip-references', action='store_true', help='omit expensive constructor cross-reference scan')
    a = p.parse_args()
    out = a.out.resolve()
    if out.exists() or not out.is_relative_to((ROOT / 'artifacts').resolve()):
        p.error('output must be a new project artifact')
    image = Image(a.exe)
    if hashlib.sha256(image.data).hexdigest() != EXE_SHA:
        raise ValueError('Executable differs from audited Version2')
    classes = []
    functions = {}
    for name in NAMES:
        decorated = ('.?AV' + name + '@@').encode() + b'\0'
        types = [r - 16 for r in data_occurrences(image, decorated)]
        tables = []
        for type_rva in types:
            for hit in data_occurrences(image, struct.pack('<I', type_rva)):
                col = hit - 12
                try:
                    if image.dword(col) != 1 or image.dword(col+20) != col:
                        continue
                    for reference in absolute_pointer_occurrences(image, col):
                        table = reference + 8
                        slots = []
                        for slot in range(34):
                            target = image.qword(table+slot*8) - image.base
                            try:
                                executable = bool(image.section_for_rva(target)['characteristics'] & 0x20000000)
                            except ValueError:
                                executable = False
                            if not executable:
                                break
                            slots.append({'slot': slot, 'target': target})
                            if target not in functions:
                                span = image.function_containing(target)
                                size = span[1]-target if span and span[0] == target else 0x40
                                if size <= 16384:
                                    rows = disassemble(image, target, size)
                                    strings = []
                                    for row in rows:
                                        if row['mnemonic'] == 'lea' and 'rip' in row['operands']:
                                            for r in row.get('targets', []):
                                                value = c_string(image, r)
                                                if value:
                                                    strings.append({'at': row['rva'], 'text': value})
                                    functions[target] = {'begin': target, 'instructions': rows, 'strings': strings,
                                                         'unwindBacked': bool(span and span[0] == target)}
                        if slots:
                            tables.append({'vtable': table, 'col': col, 'objectOffset': image.dword(col+4),
                                           'slots': slots, 'references': [] if a.skip_references else common_rip_references(image, table)})
                except (ValueError, struct.error):
                    continue
        classes.append({'name': name, 'typeRvas': types, 'vtables': tables})
    result = {'schema': 'wnba-fastload-adapter-disk-v1', 'executableSha256': EXE_SHA,
              'processAccess': False, 'gameWrites': 0, 'classes': classes,
              'constructorReferencesScanned': not a.skip_references,
              'functions': list(functions.values()),
              'limit': 'RTTI and static methods do not establish registration lifetime or a live roster route.'}
    with out.open('x', encoding='utf-8') as f:
        json.dump(result, f, indent=2)
    print(json.dumps({'report': str(out), 'classes': len(classes), 'functions': len(functions)}))


if __name__ == '__main__':
    main()
