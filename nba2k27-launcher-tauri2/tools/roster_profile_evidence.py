"""Recover previously pinned method bytes from local audit evidence, read-only."""
from pathlib import Path
import hashlib,json,re,sys
from audit_roster_current import ROOT,number,inventory

def main():
    before=ROOT/'artifacts/roster-full-audit-before-20261025.json'
    checks=json.loads(before.read_text())['checks'] if before.exists() else inventory()
    wanted={c['expected'] for c in checks if c['kind']=='sha256'}
    cached=ROOT/'artifacts/roster-old-byte-evidence.json'
    blobs=json.loads(cached.read_text()) if cached.exists() else {}
    for h,v in blobs.items():assert hashlib.sha256(bytes.fromhex(v['hex'])).hexdigest()==h
    memory={}
    def blob(b,source):
        h=hashlib.sha256(b).hexdigest()
        if h in wanted: blobs[h]={'hex':b.hex(),'source':str(source.relative_to(ROOT))}
    def walk(v,source):
        if isinstance(v,dict):
            for key in ('hex','bodyHex','bytesHex'):
                if isinstance(v.get(key),str) and re.fullmatch('[0-9a-fA-F]+',v[key]):
                    blob(bytes.fromhex(v[key]),source)
            for x in v.values():walk(x,source)
        elif isinstance(v,list):
            for x in v:walk(x,source)
        elif isinstance(v,str): lines(v,source)
    def lines(s,source):
        for m in re.finditer(r'(?:^|\n)\s*(?:0x)?([0-9a-fA-F]{7,9}):\s*((?:[0-9a-fA-F]{2} )+[0-9a-fA-F]{2})\s+\t',s):
            at=int(m[1],16);at=at-0x140000000 if at>=0x140000000 else at
            for i,b in enumerate(bytes.fromhex(m[2])):memory[at+i]=b
        for m in re.finditer(r'(?:^|\n)\s*(?:0x)?([0-9a-fA-F]{7,9})[: ]+\s*([0-9a-fA-F]{2,32})\s+[a-z]',s):
            at=int(m[1],16);at=at-0x140000000 if at>=0x140000000 else at
            if len(m[2])%2:continue
            for i,b in enumerate(bytes.fromhex(m[2])):memory[at+i]=b
    files=list(ROOT.glob('src/*tests.rs'))+list(ROOT.glob('tools/*.py'))+list(ROOT.glob('artifacts/*.json'))+list(ROOT.glob('artifacts/*.txt'))
    for directory in ('qa081-two-hour-parity-research-20261018','qa084-major-parity-20261019'):
        files+=list((ROOT/'artifacts'/directory).rglob('*.json'))+list((ROOT/'artifacts'/directory).rglob('*.txt'))
    for path in files:
        if path.stat().st_size>8_000_000:continue
        s=path.read_text(encoding='utf8',errors='replace')
        if path.suffix=='.json':
            try:walk(json.loads(s),path)
            except (ValueError,TypeError):pass
        else:
            lines(s,path)
            for m in re.finditer(r'"([0-9a-fA-F]{64,})"',s):
                if len(m[1])%2==0:blob(bytes.fromhex(m[1]),path)
    for c in checks:
        if c['kind']=='sha256' and all(c['rva']+i in memory for i in range(c['size'])):
            b=bytes(memory[c['rva']+i] for i in range(c['size']))
            blob(b,ROOT/'artifacts/reconstructed-pinned-disassembly')
    out=ROOT/'artifacts/roster-old-byte-evidence.json';out.write_text(json.dumps(blobs,indent=2)+'\n')
    print('Recovered',len(blobs),'of',len(wanted),'unique pinned spans')
    seen=set()
    for c in checks:
        if c['kind']=='sha256' and c['expected'] not in blobs and c['expected'] not in seen:
            seen.add(c['expected']);print(c['source'],hex(c['rva']),c['size'])
if __name__=='__main__':main()
