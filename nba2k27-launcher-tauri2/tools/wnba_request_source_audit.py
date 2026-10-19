"""Pinned, disk-only REQUEST field audit; never opens a process or game output."""
import argparse
import hashlib
import json
import pathlib
from wnba_controlflow_audit import Image, disassemble, c_string
from wnba_delivery_audit import ROOT, EXE_SHA


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exe', required=True, type=pathlib.Path)
    parser.add_argument('--out', required=True, type=pathlib.Path)
    args = parser.parse_args()
    output = args.out.resolve()
    if output.exists() or not output.is_relative_to((ROOT / 'artifacts').resolve()):
        parser.error('output must be a new project artifact')
    image = Image(args.exe)
    if hashlib.sha256(image.data).hexdigest() != EXE_SHA:
        raise ValueError('Executable differs from audited Version2')
    functions = []
    extra = {0x4CB35A0, 0x51F7DF0, 0x51F8550, 0x51F8C30, 0x51F8D30,
             0x4CB1990, 0x4CB21A0, 0x4BD2780, 0x51F7E90}
    # Bounded class neighbourhood, decoded from unwind-described function starts.
    for entry in image.pe.DIRECTORY_ENTRY_EXCEPTION:
        begin, end = int(entry.struct.BeginAddress), int(entry.struct.EndAddress)
        if not (0x5160000 <= begin < 0x5170000 or begin in extra):
            continue
        rows = disassemble(image, begin, end - begin)
        hits = [i for i, row in enumerate(rows) if any(
            field in row['operands'] for field in
            ('0x498]', '0x4a0]', '0x239]', '0x230]'))]
        if hits or begin == 0x5167070 or begin in extra:
            strings = []
            for row in rows:
                if row['mnemonic'] == 'lea' and 'rip' in row['operands']:
                    for target in row.get('targets', []):
                        value = c_string(image, target)
                        if value:
                            strings.append({'at': row['rva'], 'text': value})
            functions.append({'begin': begin, 'end': end, 'strings': strings,
                              'hits': [{'at': rows[i]['rva'],
                                        'context': rows[max(0, i-8):i+9]} for i in hits],
                              'instructions': rows})
    leaves = []
    recorded = {f['begin'] for f in functions}
    for begin in sorted(extra - recorded):
        rows = []
        for row in disassemble(image, begin, 0x80):
            rows.append(row)
            if row['mnemonic'] in ('ret', 'jmp'):
                break
        leaves.append({'begin': begin, 'instructions': rows,
                       'limit': 'No unwind entry. Bounded linear decode; stops at first ret/jmp.'})
    result = {'schema': 'nba2k27-request-source-audit-v1',
              'executableSha256': EXE_SHA, 'processAccess': False, 'gameWrites': 0,
              'range': [0x5160000, 0x5170000], 'extraFunctions': sorted(extra), 'functions': functions,
              'leafCandidates': leaves,
              'limit': 'Class neighbourhood only; register aliases and callees require manual tracing. No runtime lifetime proof.'}
    with output.open('x', encoding='utf-8') as stream:
        json.dump(result, stream, indent=2)
    print(json.dumps({'report': str(output), 'functions': len(functions), 'gameWrites': 0}))


if __name__ == '__main__':
    main()
