"""Bounded disk-only source/lifetime trace for the pinned Version2 REQUEST.

No process handles, executable loading, game writes, roster keys or archive IO.
"""
import argparse
import hashlib
import json
import pathlib
import re
from wnba_controlflow_audit import Image, disassemble, c_string, msvc_vtable_owner
from wnba_delivery_audit import ROOT, EXE_SHA
import pefile

ROOTS = (0x5166700, 0x516D120, 0x516D920, 0x516DB40,
         0x51F7E90, 0x51F1C30, 0x51F8C30, 0x51F8D30,
         0x4CB35A0, 0x4CB1990, 0x4CB21A0, 0x4BD2780, 0x5166F10,
         0x51F79A0, 0x51F8760, 0x51F91F0, 0x51EDAC0, 0x5201CC0,
         0x5201D00, 0x52034B0, 0x52034F0, 0x5204790, 0x51F1AE0,
         0x52057D0, 0x5205A50, 0x51EDA00, 0x51EDB20,
         0x5201E90, 0x5201FE0, 0x5201D70, 0x52022A0, 0x4BA20F0,
         0x5205BD0, 0x5205C60, 0x5205C80, 0x5205C90, 0x4B7FE30,
         0x3C76D20, 0x5168BE0)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--exe', required=True, type=pathlib.Path)
    p.add_argument('--out', required=True, type=pathlib.Path)
    args = p.parse_args()
    out = args.out.resolve()
    if out.exists() or not out.is_relative_to((ROOT / 'artifacts').resolve()):
        p.error('output must be a new project artifact')
    image = Image(args.exe)
    if hashlib.sha256(image.data).hexdigest() != EXE_SHA:
        raise ValueError('Executable differs from audited Version2')
    image.pe.parse_data_directories(directories=[pefile.DIRECTORY_ENTRY['IMAGE_DIRECTORY_ENTRY_IMPORT']])
    imports = {int(i.address) - image.base: (d.dll.decode() + '!' +
               (i.name.decode() if i.name else '#' + str(i.ordinal)))
               for d in image.pe.DIRECTORY_ENTRY_IMPORT for i in d.imports}
    vtables = []
    queue = list(ROOTS)
    manager_fields = []
    for entry in image.pe.DIRECTORY_ENTRY_EXCEPTION:
        begin, end = int(entry.struct.BeginAddress), int(entry.struct.EndAddress)
        if 0x51EDA00 <= begin < 0x51F1C30 and end-begin <= 16384:
            rows = disassemble(image, begin, end-begin)
            writes = [r for r in rows if r['mnemonic'] == 'mov' and re.match(
                r'qword ptr \[[a-z0-9]+ \+ 0x(?:48|50|58|60|68)\],', r['operands'])]
            if writes:
                manager_fields.append({'begin': begin, 'writes': writes})
                queue.append(begin)
    for table in (0x810E118, 0x810E1C8, 0x810F098, 0x812C4F8, 0x812C5C8, 0x5D39038):
        slots = []
        for n in range(22):
            target = image.qword(table + n * 8) - image.base
            try:
                executable = bool(image.section_for_rva(target)['characteristics'] & 0x20000000)
            except ValueError:
                executable = False
            if not executable:
                break
            slots.append({'slot': n, 'target': target})
        queue.extend(s['target'] for s in slots if s['slot'] in (0, 1, 19, 20))
        vtables.append({'table': table, 'owner': msvc_vtable_owner(image, table), 'slots': slots})
    functions = []
    seen = set()
    while queue and len(functions) < 75:
        begin = queue.pop(0)
        if begin in seen:
            continue
        seen.add(begin)
        span = image.function_containing(begin)
        size = span[1] - begin if span and span[0] == begin else 0x80
        if size > 65536:
            continue
        try:
            rows = disassemble(image, begin, size)
        except ValueError:
            continue
        strings, calls = [], []
        for row in rows:
            for target in row.get('targets', []):
                if target in imports:
                    row['import'] = imports[target]
                if row['mnemonic'] == 'lea' and 'rip' in row['operands']:
                    value = c_string(image, target)
                    if value:
                        strings.append({'at': row['rva'], 'text': value})
            if row['mnemonic'] in ('call', 'jmp'):
                calls.append(row)
                # One bounded callee layer for source machinery, not CRT helpers.
                if begin in ROOTS and len(row.get('targets', [])) == 1 and row['operands'].startswith('0x'):
                    target = row['targets'][0]
                    if 0x51E0000 <= target < 0x5200000:
                        queue.append(target)
        functions.append({'begin': begin, 'unwindBacked': bool(span and span[0] == begin),
                          'strings': strings, 'calls': calls, 'instructions': rows})
    result = {'schema': 'wnba-source-lifetime-disk-v1', 'executableSha256': EXE_SHA,
              'gameWrites': 0, 'processAccess': False, 'vtables': vtables, 'functions': functions,
              'managerFieldFunctions': manager_fields,
              'limit': 'Bounded linear disk disassembly; indirect calls and live ownership remain unproven. Leaf windows may include following functions.'}
    with out.open('x', encoding='utf-8') as f:
        json.dump(result, f, indent=2)
    print(json.dumps({'report': str(out), 'functions': len(functions), 'gameWrites': 0}))


if __name__ == '__main__':
    main()
