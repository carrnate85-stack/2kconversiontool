"""Read-only exact installed-PE verification of the appearance CAS proof asset."""
import hashlib,json,mmap,os,struct,sys
from pathlib import Path
from appearance_v2_disk_audit import Image
root=Path(__file__).resolve().parent.parent
proof=json.loads((root/'assets/appearance-v2-production-proof.json').read_text())
path=Path(r'C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27\NBA2K27.exe')
with path.open('rb') as f,mmap.mmap(f.fileno(),0,access=mmap.ACCESS_READ) as data:
 assert hashlib.sha256(data).hexdigest().upper()==proof['imageSha256']
 image=Image(data)
 rows=[]
 for m in proof['methods']:
  actual=hashlib.sha256(image.read(m['rva'],m['bytes'])).hexdigest()
  assert actual==m['sha256'],hex(m['rva'])
  rows.append({'rva':m['rva'],'bytes':m['bytes'],'sha256':actual})
 for rva in [0x211A0FB,0x211A120]:
  b=image.read(rva,7);assert b[:3]==bytes.fromhex('488b05') and rva+7+struct.unpack_from('<i',b,3)[0]==0x9ACE338
 assert image.read(0x5D0E560,4)==struct.pack('<I',0x3C23D70A)
 assert struct.unpack('<QQ',image.read(0x74EBE98,16))==(0x140000000+0x24E65F0,0x140000000+0x24E6660)
 result={'schema':'qa084-appearance-production-disk-audit-v1','image':str(path),'imageSha256':proof['imageSha256'],'methods':rows,'rootReferences':True,'appearanceUnits':True,'rootClass':True,'liveAccess':False,'writes':0,'acceptance':'Disk proof only. Rust/native compilation and actual dev Save acceptance not established.'}
out=root/'artifacts/qa084-major-parity-20261019/appearance-production-disk-audit.json'
with out.open('w',encoding='utf-8')as f:json.dump(result,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
print('PASS',len(rows),'exact method hashes; root/class/units; disk-only')
