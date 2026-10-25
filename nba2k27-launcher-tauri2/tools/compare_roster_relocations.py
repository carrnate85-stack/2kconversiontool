"""Compare pinned old code with current code, allowing only PC-relative relocation.

Record offsets, masks, constants, instructions, lengths and internal branches
must be identical. Emits evidence only; never edits runtime proofs.
"""
import hashlib,json,struct
from audit_roster_current import ROOT,inventory,pefile
from capstone import Cs,CS_ARCH_X86,CS_MODE_64,CS_GRP_JUMP,CS_GRP_CALL
from capstone.x86 import X86_OP_MEM,X86_REG_RIP,X86_OP_IMM

def compare(md,old,new,start,new_start):
    a=list(md.disasm(old,start));b=list(md.disasm(new,new_start))
    if not a or sum(i.size for i in a)!=len(old) or len(a)!=len(b):return None
    refs=[]
    for x,y in zip(a,b):
        if x.id!=y.id or x.size!=y.size:return None
        p=bytearray(x.bytes);q=bytearray(y.bytes)
        reloc=None
        if any(o.type==X86_OP_MEM and o.mem.base==X86_REG_RIP for o in x.operands):
            reloc=(x.disp_offset,x.disp_size,x.address+x.size+x.disp,y.address+y.size+y.disp,'rip')
        elif (x.group(CS_GRP_JUMP) or x.group(CS_GRP_CALL)) and x.operands[0].type==X86_OP_IMM:
            reloc=(x.imm_offset,x.imm_size,x.operands[0].imm,y.operands[0].imm,'branch')
        if reloc:
            off,n,src,dst,kind=reloc
            if start<=src<start+len(old):
                if dst-new_start!=src-start:return None
            elif dst-src not in (0,0xb0,0x280) and (src,dst) not in ((0x546b8d0,0x546bcd0),(0x546b8d6,0x546bcd6),(0x546aa90,0x546ae90)):return None
            p[off:off+n]=q[off:off+n]=bytes(n)
            refs.append(dict(instruction=hex(x.address),kind=kind,oldTarget=hex(src),newTarget=hex(dst)))
        if p!=q:return None
    return refs

def main():
    pe=pefile.PE(r'C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27\NBA2K27.exe',fast_load=True)
    evidence=json.loads((ROOT/'artifacts/roster-old-byte-evidence.json').read_text())
    md=Cs(CS_ARCH_X86,CS_MODE_64);md.detail=True
    result=[];seen=set()
    before=ROOT/'artifacts/roster-full-audit-before-20261025.json'
    checks=json.loads(before.read_text())['checks'] if before.exists() else inventory()
    for c in checks:
        if c['kind']!='sha256' or c['expected'] in seen:continue
        seen.add(c['expected']);old=evidence.get(c['expected'])
        item=dict(c,candidates=[])
        for delta in (0,0xb0,0xc0,0x280):
            new=pe.get_data(c['rva']+delta,c['size']);sha=hashlib.sha256(new).hexdigest()
            refs=compare(md,bytes.fromhex(old['hex']),new,c['rva'],c['rva']+delta) if old else None
            if sha==c['expected'] or refs is not None:
                item['candidates'].append(dict(rva=c['rva']+delta,sha256=sha,exact=sha==c['expected'],references=refs or []))
        result.append(item)
        print(c['source'],hex(c['rva']),c['size'],'MATCH' if len(item['candidates'])==1 else 'NEEDS PROOF')
    (ROOT/'artifacts/roster-relocation-evidence.json').write_text(json.dumps(result,indent=2)+'\n')
if __name__=='__main__':main()
