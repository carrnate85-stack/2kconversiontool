"""Read-only pinned-image function trace. No process access or native calls."""
import sys,mmap,struct,hashlib,json,os
from pathlib import Path
sys.path.insert(0,str(Path(__file__).parent/'python-packages'))
import capstone
from appearance_v2_disk_audit import Image,SHA
exe=Path(r"C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27\NBA2K27.exe")
out={"imageSha256":SHA,"gameProcessAccess":False,"functions":[]}
with exe.open('rb') as f,mmap.mmap(f.fileno(),0,access=mmap.ACCESS_READ) as data:
 image=Image(data)
 assert hashlib.sha256(data).hexdigest().upper()==SHA
 pdata=image.read(image.pdata,image.pdata_size)
 rows=[struct.unpack_from('<III',pdata,i) for i in range(0,len(pdata),12)]
 md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_64)
 out['blacktopRootSlots']={hex(o):hex(struct.unpack('<Q',image.read(0x791E158+o,8))[0]-0x140000000) for o in [8,0x100,0x110]}
 for at in [0x222AAC0,0x236EB30,0x32FF5A0,0x236E440,0x236FB30,0x236FB50,0x2F1E0B0,0x2F1E360,0x308C5B0,0x220B400,0x213D0C0,0x216D7F0,0x21633C0,0x308B740]:
  row=next((r for r in rows if r[0]==at),None)
  size=row[1]-at if row else 26
  code=image.read(at,size)
  out['functions'].append({'rva':hex(at),'bytes':size,'sha256':hashlib.sha256(code).hexdigest().upper(),'instructions':[f'{i.address:X}: {i.mnemonic} {i.op_str}' for i in md.disasm(code,at)]})
path=Path('artifacts/qa069-blacktop-install-disassembly-20261018.json')
with path.open('w') as f:json.dump(out,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
print('PASS exact image, pinned known methods traced offline; no process access')
