"""Offline exact-mode hash refs and RTTI; no process access."""
import sys,mmap,struct,hashlib,json,os,re
from pathlib import Path
sys.path.insert(0,str(Path(__file__).parent/'python-packages'))
import capstone
from appearance_v2_disk_audit import Image,SHA
exe=Path(r"C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27\NBA2K27.exe")
out={'modeHash':'4FDE5DCD','gameProcessAccess':False,'imageSha256':SHA,'references':[],'getterVtables':[]}
with exe.open('rb') as f,mmap.mmap(f.fileno(),0,access=mmap.ACCESS_READ) as data:
 image=Image(data);assert hashlib.sha256(data).hexdigest().upper()==SHA
 def rva(off):
  for _,base,size,start in image.sections:
   if start<=off<start+size:return base+off-start
 def string(at):return image.read(at,200).split(b'\0')[0].decode('ascii',errors='replace')
 pdata=image.read(image.pdata,image.pdata_size);rows=[struct.unpack_from('<III',pdata,i) for i in range(0,len(pdata),12)]
 md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_64)
 at=0;getters=[]
 while True:
  at=data.find(bytes.fromhex('cd5dde4f'),at)
  if at<0:break
  loc=rva(at);at+=4
  if loc is None:continue
  row=next((r for r in rows if r[0]<=loc<r[1]),None)
  if row and row[1]-row[0]<=20000:
   out['references'].append({'hitRva':hex(loc),'functionRva':hex(row[0]),'bytes':row[1]-row[0],'instructions':[f'{i.address:X}: {i.mnemonic} {i.op_str}' for i in md.disasm(image.read(row[0],row[1]-row[0]),row[0])]})
  if image.read(loc-1,6)==bytes.fromhex('b8cd5dde4fc3'):getters.append(loc-1)
 for getter in sorted(set(getters+[0x32F6000,0x32F3140,0x3247140,0x36331F0])):
  pos=0
  while True:
   pos=data.find(struct.pack('<Q',0x140000000+getter),pos)
   if pos<0:break
   slot=rva(pos);pos+=8
   if slot is None:continue
   # Identity getter is typically vtable+18; inspect nearby complete-object locator candidates.
   for delta in range(0,0x1000,8):
    try:
     col=struct.unpack('<Q',image.read(slot-delta-8,8))[0]-0x140000000
     sig,offset,_,td,_,self=struct.unpack('<IIIIII',image.read(col,24))
     if sig==1 and self==col and offset<4096:
      out['getterVtables'].append({'getterRva':hex(getter),'vtableRva':hex(slot-delta),'slot':hex(delta),'baseOffset':offset,'class':string(td+16)})
      break
    except Exception:pass
 for ref in out['references']:
  begin=int(ref['functionRva'],16); strings=[]
  md.detail=True
  for ins in md.disasm(image.read(begin,ref['bytes']),begin):
   for op in ins.operands:
    if op.type==capstone.x86.X86_OP_MEM and op.mem.base==capstone.x86.X86_REG_RIP:
     target=ins.address+ins.size+op.mem.disp
     try:
      raw=image.read(target,160).split(b'\0')[0]
      if len(raw)>=4 and all(32<=x<127 for x in raw):strings.append({'instruction':hex(ins.address),'target':hex(target),'text':raw.decode()})
     except Exception:pass
  ref['strings']=strings
path=Path('artifacts/qa069-mode-4fde5dcd-native-proof-20261018.json')
with path.open('w') as f:json.dump(out,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
print('PASS mode refs',len(out['references']),'classes',out['getterVtables'])
