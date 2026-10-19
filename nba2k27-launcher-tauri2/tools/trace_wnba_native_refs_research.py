"""Disk-only bounded native reference investigation; no keys or game access."""
import argparse,bisect,hashlib,json,mmap,os,struct,sys
from pathlib import Path

def main():
 p=argparse.ArgumentParser();p.add_argument('--target',type=lambda x:int(x,16),action='append',required=True);p.add_argument('--out',type=Path,required=True);a=p.parse_args()
 assert not a.out.exists() and a.out.parent.name=='artifacts'
 exe=Path(r'C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27\NBA2K27.exe')
 assert hashlib.file_digest(exe.open('rb'),'sha256').hexdigest()=='29a96471c55909fc2bcea6c699c59238656a501c19b50c74ed64b0d842e576a1'
 sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'research/python_modules'))
 from capstone import Cs,CS_ARCH_X86,CS_MODE_64
 with exe.open('rb')as f,mmap.mmap(f.fileno(),0,access=mmap.ACCESS_READ)as m:
  pe=struct.unpack_from('<I',m,60)[0];n=struct.unpack_from('<H',m,pe+6)[0];sz=struct.unpack_from('<H',m,pe+20)[0];sections=[]
  for i in range(n):
   at=pe+24+sz+i*40;vs,va,rs,raw=struct.unpack_from('<IIII',m,at+8);sections.append((m[at:at+8].rstrip(b'\0').decode(),va,raw,rs))
  def read(rva,n):
   for _,va,raw,rs in sections:
    if va<=rva and rva+n<=va+rs:return m[raw+rva-va:raw+rva-va+n]
   raise ValueError(hex(rva))
  _,_,raw,size=next(x for x in sections if x[0]=='.pdata');pdata=sorted(v[:2]for v in(struct.unpack_from('<III',m,raw+i)for i in range(0,size-size%12,12))if v[0]and v[0]<v[1]);starts=[x[0]for x in pdata];md=Cs(CS_ARCH_X86,CS_MODE_64);results=[];functions={}
  def record(rva,target,kind):
   j=bisect.bisect_right(starts,rva)-1;b,e=pdata[j]
   if not b<=rva<e or e-b>100000:return
   code=read(b,e-b);ins=list(md.disasm(code,b));hit=next((i for i,z in enumerate(ins)if z.address==rva),None)
   if hit is None:return
   results.append({'rva':hex(rva),'target':hex(target),'kind':kind,'function':hex(b),'context':[(hex(z.address),z.mnemonic,z.op_str)for z in ins[max(0,hit-24):hit+16]]})
   functions[hex(b)]={'end':hex(e),'sha256':hashlib.sha256(code).hexdigest(),'instructions':[(hex(z.address),z.mnemonic,z.op_str)for z in ins]}
  for name,va,raw,rs in sections:
   if name!='.text':continue
   for opcode in(b'\xe8',b'\xe9'):
    pos=raw
    while True:
     pos=m.find(opcode,pos,raw+rs-4)
     if pos<0:break
     rva=va+pos-raw;target=rva+5+struct.unpack_from('<i',m,pos+1)[0];pos+=1
     if target in a.target:record(rva,target,'call'if opcode==b'\xe8'else'jump')
   pos=raw
   while True:
    pos=m.find(b'\x8d',pos,raw+rs-6)
    if pos<0:break
    x=pos;pos+=1
    if m[x-1]not in(0x48,0x4c)or m[x+1]&0xc7!=5:continue
    rva=va+x-1-raw;target=rva+7+struct.unpack_from('<i',m,x+2)[0]
    if target in a.target:record(rva,target,'rip-lea')
  for target in a.target:
   for name,va,raw,rs in sections:
    if name not in('.rdata','.data'):continue
    needle=struct.pack('<Q',0x140000000+target);pos=raw
    while True:
     pos=m.find(needle,pos,raw+rs)
     if pos<0:break
     results.append({'target':hex(target),'pointerRef':hex(va+pos-raw),'section':name});pos+=1
 report={'researchOnly':True,'gameProcessAccess':False,'saltReads':0,'keyReads':0,'targets':[hex(x)for x in a.target],'refs':results,'functions':functions}
 with a.out.open('x',encoding='utf-8')as f:json.dump(report,f,indent=2);f.flush();os.fsync(f.fileno())
 print(json.dumps({'out':str(a.out),'refs':[{k:v for k,v in x.items()if k!='context'}for x in results]}))
if __name__=='__main__':main()
