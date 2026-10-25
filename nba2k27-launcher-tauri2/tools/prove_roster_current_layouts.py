"""Current installed-build dataflow proof; reads PE only, writes audit artifacts."""
import hashlib,json,struct,sys,importlib.util
from audit_roster_current import ROOT,IMAGE,pefile
from compare_roster_relocations import compare
from capstone import Cs,CS_ARCH_X86,CS_MODE_64
import roster_v2_write_audit as uniform
import editor_fields_v2_audit as services

PATH=r'C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27\NBA2K27.exe'
def main():
    with open(PATH,'rb') as f:assert hashlib.file_digest(f,'sha256').hexdigest().upper()==IMAGE
    pe=pefile.PE(PATH,fast_load=True);md=Cs(CS_ARCH_X86,CS_MODE_64);md.detail=True
    aud=uniform.extractor()
    aud.GET_PROPERTY=0x4BECC20
    aud.VALUE_WRITERS={0x4BED2F0:'signed',0x4BED3F0:'unsigned',0x4BED490:'boolean',0x4BED580:'enum'}
    aud.FORMATTER=0x4B8EA60
    # Exact previous anchor bytes must survive relocation before re-extracting.
    original=ROOT/'Backups/before-roster-batch-profile-20261025/assets/roster-v2-write-proof.json'
    old=json.loads((original if original.exists() else ROOT/'assets/roster-v2-write-proof.json').read_text())
    for a in old['anchors']:assert pe.get_data(a['rva']+0xb0,len(a['hex'])//2).hex()==a['hex']
    uniform.REBRAND_TEXT=[(a+0xb0,b+0xb0) for a,b in uniform.REBRAND_TEXT]
    uniform.WIDE_COPY+=0xb0;uniform.NARROW_COPY+=0xb0
    uniform.SLOT_STORE=tuple(x+0xb0 for x in uniform.SLOT_STORE)
    uniform.SLOT_GETTER=tuple(x+0xb0 for x in uniform.SLOT_GETTER)
    uniform.TEAM_GETTERS=[(r+0xb0,n) for r,n in uniform.TEAM_GETTERS]
    # Older helper has two extra caller spans embedded in its function body.
    original_disasm=uniform.disasm
    def mapped_disasm(md,pe,a,b):
        if a in (0x1DAE5C7,0x1DAE601):a+=0xb0;b+=0xb0
        return original_disasm(md,pe,a,b)
    uniform.disasm=mapped_disasm
    u=uniform.uniform_proof(pe,md,aud)
    assert len(u['controls'])==52 and not u['mismatches'] and len(u['displayFields'])==81
    write=dict(old,imageSha256=IMAGE,uniform=u,teamText=uniform.team_text_proof(pe,md),uniformSlots=uniform.slot_proof(pe,md),anchors=uniform.anchors(pe),generatorSha256=hashlib.sha256(__file__.encode()+open(__file__,'rb').read()).hexdigest())
    (ROOT/'artifacts/roster-current-write-proof.json').write_text(json.dumps(write,indent=2)+'\n')
    print('Uniform: 52 write controls, 81 display fields, 5 team strings and 32 slots proved')
    img=services.Image(open(PATH,'rb').read())
    services.EXPECTED_STAMP=0x6AD2E48C;services.EXPECTED_IMAGE_SIZE=0x36532000
    services.FIND=aud.GET_PROPERTY
    services.WRITERS={hex(x) for x in (0x4BED2F0,0x4BED3F0,0x4BED490,0x4BED580,0x4BED440)}
    services.STAT_SET=tuple(x+0xb0 if i<2 else x for i,x in enumerate(services.STAT_SET))
    services.STAT_GET=tuple(x+0xb0 if i<2 else x for i,x in enumerate(services.STAT_GET))
    services.STAT_TAILS={x+0xb0 for x in services.STAT_TAILS}
    staff=services.serializer_fields(img,*services.STAFF_SERIALIZER)
    rows,editable=services.compare_staff(staff,services.DEFAULT_WPF_REF,services.DEFAULT_HELPER)
    assert rows and all(r['match']!='unmapped' and 'differs' not in r['match'] for r in rows)
    assert editable and not editable['outsideNamedFields']
    setters,getters=services.stat_setters(img),services.stat_getters(img)
    seasons,summary=services.compare_seasons(setters,getters)
    assert len(seasons)==32 and all(r['setterOk'] and r['getterOk'] for r in seasons)
    enums=services.enum_tables(img,services.DEFAULT_WPF_REF)
    assert all(e['referenced'] and e['allNamed'] and e.get('matchesWpfRoles',True) for e in enums)
    player=services.serializer_fields(img,*services.PLAYER_SERIALIZER)
    directory=next(f for f in player if f['name']=='SeasonStats')
    assert directory['offsets']==[0x130+2*k for k in range(33)] and directory['elementBits']==16
    result=dict(imageSha256=IMAGE,gameProcessAccess=False,gameFilesChanged=False,staffFields=staff,staffVsWpf=rows,staffEditableMask=editable,seasonVsWpf=seasons,seasonSummary=summary,enums=enums,playerSeasonDirectory=directory)
    (ROOT/'artifacts/roster-current-services-proof.json').write_text(json.dumps(result,indent=2)+'\n')
    print('Staff:',len(rows),'controls; season:',len(seasons),'getter/setter pairs; editable masks and player directory proved')
if __name__=='__main__':main()
