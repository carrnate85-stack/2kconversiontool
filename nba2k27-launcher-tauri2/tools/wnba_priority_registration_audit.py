"""Bounded disk-only search for native fast-load priority registration.

Does not access processes, keys, archives, or executable code execution.
"""
import argparse
import hashlib
import json
import re
from wnba_controlflow_audit import Image, disassemble, c_string
from wnba_delivery_audit import ROOT, EXE_SHA


def audit(image):
    candidates = []
    for entry in image.pe.DIRECTORY_ENTRY_EXCEPTION:
        begin, end = int(entry.struct.BeginAddress), int(entry.struct.EndAddress)
        if not (0x51E0000 <= begin < 0x5210000) or end - begin > 65536:
            continue
        rows = disassemble(image, begin, end - begin)
        writes = [r for r in rows if r['mnemonic'] in ('mov', 'xchg', 'cmpxchg') and
                  re.match(r'qword ptr \[(?!rsp|rbp)[a-z0-9]+ \+ 0x48\],', r['operands'])]
        if not writes:
            continue
        strings = []
        for row in rows:
            if row['mnemonic'] == 'lea' and 'rip' in row['operands']:
                for target in row.get('targets', []):
                    value = c_string(image, target)
                    if value and len(value) >= 6 and all(c.isprintable() for c in value):
                        strings.append({'at': row['rva'], 'text': value})
        candidates.append({'begin': begin, 'end': end, 'writes': writes,
                           'strings': strings, 'instructions': rows})
    return candidates


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--exe', required=True)
    p.add_argument('--out', required=True, type=__import__('pathlib').Path)
    a = p.parse_args()
    out = a.out.resolve()
    if out.exists() or not out.is_relative_to((ROOT / 'artifacts').resolve()):
        p.error('output must be a new project artifact')
    image = Image(__import__('pathlib').Path(a.exe))
    if hashlib.sha256(image.data).hexdigest() != EXE_SHA:
        raise ValueError('Executable differs from audited Version2')
    candidates = audit(image)
    result = {'schema': 'wnba-priority-registration-disk-v1',
              'executableSha256': EXE_SHA, 'gameWrites': 0, 'processAccess': False,
              'range': [0x51E0000, 0x5210000], 'candidates': candidates,
              'limit': 'Only unwind-described functions in the bounded fast-load module range. A +0x48 write is a candidate, not type or ownership proof.'}
    with out.open('x', encoding='utf-8') as f:
        json.dump(result, f, indent=2)
    print(json.dumps({'report': str(out), 'candidates': len(candidates)}))


if __name__ == '__main__':
    main()
