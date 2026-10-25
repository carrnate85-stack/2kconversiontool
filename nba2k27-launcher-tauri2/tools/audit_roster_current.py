"""Read-only inventory of roster runtime fingerprints against the installed PE.

Reports every mismatch instead of stopping at the first profile gate. Does not
change profiles, read game memory, or authorize writes by updating a hash.
"""
from pathlib import Path
import argparse, hashlib, json, re, sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT.parent / 'research/python_modules'))
import pefile

IMAGE = 'B673EB04593D6D5A4DFB9E270178E04C017BD9F277514D256EA98FFB792ED27B'
NUM = r'(?:0x[\da-fA-F_]+|\d[\d_]*)'

def number(s):
    return int(str(s).replace('_', ''), 16 if str(s).lower().startswith('0x') else 10)

def inventory():
    checks = []
    def add(source, label, rva, size, expected, kind):
        checks.append(dict(source=source, label=label, rva=rva, size=size, expected=expected.lower(), kind=kind))
    sources = sorted(set(ROOT.glob('src/roster_v2*.rs')) | {ROOT/'src/season_structure_v2_native.rs'})
    for path in sources:
        if 'test' in path.stem:
            continue
        text = re.sub(r'//[^\n]*', '', path.read_text(encoding='utf-8-sig'))
        for m in re.finditer(r'\(\s*('+NUM+r')\s*,\s*('+NUM+r')\s*,\s*"([\da-fA-F]{64})"\s*,?\s*\)', text):
            add(path.name, 'code hash', number(m[1]), number(m[2]), m[3], 'sha256')
        for m in re.finditer(r'\(\s*('+NUM+r')\s*,\s*"((?:[\da-fA-F]{2})+)"\s*,?\s*\)', text):
            add(path.name, 'byte anchor', number(m[1]), len(m[2])//2, m[2], 'hex')
    def walk(source, value, label=''):
        if isinstance(value, dict):
            rva = value.get('rva', value.get('begin'))
            if rva is not None:
                rva = number(rva)
                size = value.get('bytes')
                if size is None and 'end' in value:
                    size = number(value['end']) - rva
                if isinstance(size, int) and 'sha256' in value:
                    add(source, label, rva, size, value['sha256'], 'sha256')
                for key in ('hex', 'prefixHex'):
                    if isinstance(value.get(key), str):
                        add(source, label, rva, len(value[key])//2, value[key], 'hex')
                if value.get('metadataRva') and value.get('metadataHex'):
                    add(source, label+'/metadata', number(value['metadataRva']), len(value['metadataHex'])//2, value['metadataHex'], 'hex')
            for key, item in value.items():
                walk(source, item, label+'/'+key)
        elif isinstance(value, list):
            for i,item in enumerate(value):
                walk(source, item, label+'/'+str(i))
    for path in sorted(ROOT.glob('assets/*v2*proof.json')):
        if not path.name.startswith(('roster-', 'appearance-')):
            continue
        walk(path.name, json.loads(path.read_text(encoding='utf-8-sig')))
    return checks

def main():
    p=argparse.ArgumentParser(); p.add_argument('image', type=Path); p.add_argument('--output', type=Path, required=True)
    args=p.parse_args()
    with args.image.open('rb') as f:
        digest=hashlib.file_digest(f,'sha256').hexdigest().upper()
    assert digest==IMAGE, 'Unrecognized game image; no profile assumptions permitted'
    pe=pefile.PE(str(args.image),fast_load=True)
    checks=inventory()
    for c in checks:
        data=pe.get_data(c['rva'],c['size'])
        c['actual']=hashlib.sha256(data).hexdigest() if c['kind']=='sha256' else data.hex()
        c['matches']=c['expected']==c['actual']
        c['exactRelocations']=[]
        if not c['matches']:
            for delta in (0xb0,0x280,-0xb0,-0x280):
                data=pe.get_data(c['rva']+delta,c['size'])
                value=hashlib.sha256(data).hexdigest() if c['kind']=='sha256' else data.hex()
                if value==c['expected']: c['exactRelocations'].append(c['rva']+delta)
    report=dict(imageSha256=digest,gameWrites=False,checks=checks)
    args.output.write_text(json.dumps(report,indent=2)+'\n',encoding='utf8')
    groups={}
    for c in checks:
        g=groups.setdefault(c['source'],dict(total=0,passed=0,stale=0,exactRelocated=0))
        g['total']+=1;g['passed']+=int(c['matches']);g['stale']+=int(not c['matches']);g['exactRelocated']+=int(bool(c['exactRelocations']))
    print(json.dumps(groups,indent=2))
    return int(any(not c['matches'] for c in checks))

if __name__=='__main__': raise SystemExit(main())
