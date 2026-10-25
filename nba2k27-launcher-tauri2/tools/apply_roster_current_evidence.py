"""One-time, evidence-gated profile migration. No process or game writes.

Requires the independent field/relocation reports. Keeps a source backup before
publishing the verified current-build RVAs and byte hashes. No runtime checks
are removed, and historical journal tags are not promoted to live authority.
"""
import hashlib,json,re,shutil,struct
from audit_roster_current import ROOT,IMAGE,NUM,number,pefile
from compare_roster_relocations import compare,Cs,CS_ARCH_X86,CS_MODE_64
OLD_IMAGE='29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1'

def main():
    path=r'C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27\NBA2K27.exe'
    with open(path,'rb') as f:assert hashlib.file_digest(f,'sha256').hexdigest().upper()==IMAGE
    pe=pefile.PE(path,fast_load=True);md=Cs(CS_ARCH_X86,CS_MODE_64);md.detail=True
    checks=json.loads((ROOT/'artifacts/roster-full-audit-before-20261025.json').read_text())['checks']
    reloc=json.loads((ROOT/'artifacts/roster-relocation-evidence.json').read_text())
    services=json.loads((ROOT/'artifacts/roster-current-services-proof.json').read_text())
    assert services['imageSha256']==IMAGE and len(services['staffVsWpf'])==308
    assert all(x['setterOk'] and x['getterOk'] for x in services['seasonVsWpf'])
    addresses={0x6ac53765:0x6ad2e48c,0x35bde000:0x36532000,0x24e65f0:0x24e66a0,0x24e6660:0x24e6710}
    hashes={};hexes={}
    for c in reloc:
        if c['rva']==0x6bba00:
            assert c['size']==17580
            c['candidates']=[dict(rva=c['rva'],sha256=hashlib.sha256(pe.get_data(c['rva'],c['size'])).hexdigest())]
        assert len(c['candidates'])==1, f"Unproven span {c['rva']:x}"
        new=c['candidates'][0];addresses[c['rva']]=new['rva'];hashes[c['expected']]=new['sha256']
        assert hashlib.sha256(pe.get_data(new['rva'],c['size'])).hexdigest()==new['sha256']
    for c in checks:
        if c['kind']!='hex':continue
        old=bytes.fromhex(c['expected']);candidates=[]
        for delta in (0,0xb0,0xc0,0x280):
            at=c['rva']+delta;new=pe.get_data(at,c['size'])
            if old==new or compare(md,old,new,c['rva'],at) is not None:candidates.append((at,new))
        assert len(candidates)==1,(c['source'],hex(c['rva']),len(candidates))
        at,new=candidates[0];addresses[c['rva']]=at;hexes[c['expected']]=new.hex()
    # Every full-byte fixture must be exactly one of the old hashed spans.
    old_blobs=json.loads((ROOT/'artifacts/roster-old-byte-evidence.json').read_text())
    for c in reloc:
        if c['expected'] in old_blobs:
            old=old_blobs[c['expected']]['hex'];assert hashlib.sha256(bytes.fromhex(old)).hexdigest()==c['expected']
            hexes[old]=pe.get_data(addresses[c['rva']],c['size']).hex()
    for site,target,name in [(0x2e24b3a,0x68ed802,'APPEARANCEDATA::SetHeightInTenthsOfMM'),(0x2e24c2e,0x68ed8e0,'APPEARANCEDATA::SetWingspanInTenthsOfMM')]:
        at=site+0xb0;b=pe.get_data(at,7)
        assert b[:3]==bytes.fromhex('4c8d0d') and at+7+struct.unpack_from('<i',b,3)[0]==target
        assert pe.get_data(target,len(name)+1)==name.encode()+b'\0'
        addresses[site]=at
    for site,key in [(0x6b2eba,'FirstLastNameSID'),(0x6ae7df,'Weight')]:
        b=pe.get_data(site,7);assert b[:3]==bytes.fromhex('488d15')
        target=site+7+struct.unpack_from('<i',b,3)[0]
        assert pe.get_data(target,len(key)+1)==key.encode()+b'\0'
    assert struct.unpack('<Q',pe.get_data(0x89be960,8))[0]==pe.OPTIONAL_HEADER.ImageBase+0xa1e7cd0
    assert pe.get_data(0x5d0e560,4)==struct.pack('<f',0.01)
    for begin,end in [(0x2e24a80,0x2e24b53),(0x2e24b70,0x2e24c47)]:addresses[end]=addresses[begin]+end-begin
    # Research serializer now uses the separately proven named-field span.
    hashes['b5f0449fac3b3e2cd0d6009212a6b39062bdfd6568aada1f7704425923c739bf']='bea589403432ea7a3de52024502554fda1e3ca842c75e6e01abe8f667d56feb0'
    def scalar(v):
        if isinstance(v,str):
            if v==OLD_IMAGE:return IMAGE
            if v in hashes:return hashes[v]
            if v in hexes:return hexes[v]
            if re.fullmatch(r'0x[\da-fA-F]+',v) and number(v) in addresses:return hex(addresses[number(v)])
        elif type(v)==int and v in addresses:return addresses[v]
        return v
    def walk(v):
        if isinstance(v,dict):return {k:walk(x) for k,x in v.items()}
        if isinstance(v,list):return [walk(x) for x in v]
        return scalar(v)
    updates={}
    for file in ROOT.glob('assets/*v2*proof.json'):
        if not file.name.startswith(('roster-','appearance-')):continue
        data=json.loads(file.read_text(encoding='utf-8-sig'))
        if file.name=='roster-v2-field-proof.json':continue
        if file.name=='roster-v2-write-proof.json':data=json.loads((ROOT/'artifacts/roster-current-write-proof.json').read_text())
        else:data=walk(data)
        # Unwind metadata is recorded for evidence, not used as write authority.
        if 'setters' in data:
            entries=pe.OPTIONAL_HEADER.DATA_DIRECTORY[3];raw=pe.get_data(entries.VirtualAddress,entries.Size)
            unwind={a:(b,c) for a,b,c in struct.iter_unpack('<III',raw)}
            for item in data['setters']:
                end,uw=unwind[number(item['begin'])];assert end==number(item['end']);item['unwind']=hex(uw)
        updates[file]=json.dumps(data,indent=2)+'\n'
    arena_hash=hashlib.sha256(updates[ROOT/'assets/roster-v2-arena-proof.json'].encode()).hexdigest()
    files=list(ROOT.glob('src/roster_v2*.rs'))+list(ROOT.glob('src/season_structure*.rs'))+[ROOT/'src/owned_transaction.rs']
    for file in files:
        s=file.read_text(encoding='utf-8-sig');original=s
        # One pass prevents a new RVA being mistaken for an old adjacent RVA.
        def token(m):
            t=m[0]
            if t.startswith('"'):
                v=t[1:-1];return '"'+(arena_hash if v=='f5fae5c88d96cfcb091298a5cabfbc50edaf2785392c27d1899108853643dc8d' else str(scalar(v)))+'"'
            n=number(t);return hex(addresses[n]) if n in addresses else t
        s=re.sub(r'"[\da-fA-F]{32,}"|(?<![\w])(?:0x[\da-fA-F_]+|\d[\d_]*)(?![\w])',token,s)
        # Suffixed Rust literals are excluded by the identifier boundary above.
        s=re.sub(r'\b(0x[\da-fA-F_]+)(?=u(?:32|64)\b)',lambda m:hex(addresses.get(number(m[1]),number(m[1]))),s)
        if file.name=='roster_v2_profile.rs':s=s.replace('SERIALIZER_BYTES: usize = 50325','SERIALIZER_BYTES: usize = 54512')
        if s!=original:updates[file]=s
    backup=ROOT/'Backups/before-roster-batch-profile-20261025';assert not backup.exists(),'Migration already applied; use verifier'
    for file in updates:
        target=backup/file.relative_to(ROOT);target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(file,target)
    for file,s in updates.items():file.write_text(s,encoding='utf8',newline='\n')
    (ROOT/'artifacts/roster-current-migration.json').write_text(json.dumps(dict(imageSha256=IMAGE,files=[str(p.relative_to(ROOT)) for p in updates],addresses={hex(k):hex(v) for k,v in addresses.items()},gameProcessAccess=False,gameFilesChanged=False),indent=2)+'\n')
    print('Updated',len(updates),'source/proof files; preserved backups; no game access')
if __name__=='__main__':main()
