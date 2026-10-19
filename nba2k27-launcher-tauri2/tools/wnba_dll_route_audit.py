"""Bounded offline Learn2K resource-manager vs provider-device call audit.

Reads only the installed executable and writes metadata/disassembly evidence in
project artifacts. No process, DLL loading, game execution or game-file writes.
"""
import argparse
import hashlib
import json
import pathlib
import sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from wnba_controlflow_audit import Image, disassemble, c_string, msvc_vtable_owner
from wnba_delivery_audit import ROOT, EXE_SHA

ROOTS = {
    'roster-state-machine': 0x226A7D0,
    'roster-common-loader': 0x2F1E0B0,
    'resource-manager-submit': 0x3C76D20,
    'resource-async-request': 0x5168BE0,
    'resource-request-initialize': 0x5166700,
    'resource-request-virtual19': 0x516DB40,
    'resource-open-or-source':0x50C6C30,
    'file-manager-open-wrapper':0x4BA1A00,
}
PROVIDER = {'manager-getter': 0x4B9C0A0, 'manager-open': 0x4BA0C40,
            'manager-find': 0x4B9CE40, 'manager-select': 0x4B9D120}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--exe', type=pathlib.Path, required=True)
    p.add_argument('--out', type=pathlib.Path, required=True)
    a = p.parse_args()
    output = a.out.resolve()
    if output.exists() or not output.is_relative_to((ROOT / 'artifacts').resolve()):
        p.error('--out must be a new project artifact')
    image = Image(a.exe)
    if hashlib.sha256(image.data).hexdigest() != EXE_SHA:
        raise ValueError('Executable differs from audited Version2')
    known = {rva: name for name, rva in {**ROOTS, **PROVIDER}.items()}
    found = {}
    queue = [(rva, 0) for rva in ROOTS.values()]
    # Bounded direct graph only: indirect calls are explicitly unresolved, not
    # treated as evidence that a route cannot exist.
    while queue and len(found) < 100:
        rva, depth = queue.pop(0)
        if rva in found:
            continue
        span = image.function_containing(rva)
        if not span or span[0] != rva or span[1] - span[0] > 65536:
            continue
        rows = disassemble(image, rva, span[1] - rva)
        direct, indirect, strings, fields = [], [], [], []
        for row_index, row in enumerate(rows):
            if any(offset in row['operands'] for offset in ('0x69]', '0x239]', '0x230]')):
                fields.append({'at':row['rva'], 'context':rows[max(0,row_index-4):row_index+6]})
            if row['mnemonic'] == 'call':
                targets = row.get('targets', [])
                if len(targets) == 1 and row['operands'].startswith('0x'):
                    target = targets[0]
                    direct.append({'at': row['rva'], 'target': target,
                                   'knownName': known.get(target)})
                else:
                    indirect.append({'at': row['rva'], 'operands': row['operands'],
                        'context': rows[max(0, row_index - 6):row_index + 3]})
            if row['mnemonic'] == 'lea' and 'rip' in row['operands']:
                for target in row.get('targets', []):
                    text = c_string(image, target)
                    if text and any(w in text.lower() for w in ('archive', 'roster', 'vcresource', 'filesystem')):
                        strings.append({'at': row['rva'], 'target': target, 'text': text[:256]})
        found[rva] = {'rva': rva, 'end': span[1], 'name': known.get(rva), 'depth': depth,
                      'directCalls': direct, 'indirectCalls': indirect, 'resourceStrings': strings,
                      'pathFlagReferences':fields}
        if depth < 2:
            queue.extend((c['target'], depth + 1) for c in direct[:25]
                         if c['target'] not in found and c['target'] not in PROVIDER.values())
    result = {'schema': 'nba2k27-wnba-dll-route-audit-v1', 'executableSha256': EXE_SHA,
              'processAccess': False, 'gameWrites': 0, 'maximumFunctions': 100, 'maximumDepth': 2,
              'providerDeviceSymbols': PROVIDER, 'roots': ROOTS,
              'requestVtables':[{'rva':v, 'owner':msvc_vtable_owner(image,v),
                   'virtual19':image.qword(v+19*8)-image.base} for v in (0x810E118,0x810E1C8)],
              'sourceGateInstructions':disassemble(image,0x50C6F13,0x75),
              'fileFallbackInstructions':disassemble(image,0x50C70B4,0x45),
              'requestSourceSetupInstructions':disassemble(image,0x516DB71,0x2A),
              'functions': list(found.values()),
              'providerCallsFound': [dict(source=rva, **call) for rva, f in found.items()
                                    for call in f['directCalls'] if call['target'] in PROVIDER.values()],
              'limit': 'Direct calls only. Indirect dispatch, async completion and runtime selected device require separate proof; absence is not proof of impossibility.'}
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open('x', encoding='utf-8') as f:
        json.dump(result, f, indent=2)
    print(json.dumps({'report': str(output), 'functions': len(found),
                      'providerCallsFound': result['providerCallsFound'], 'gameWrites': 0}))


if __name__ == '__main__':
    main()
