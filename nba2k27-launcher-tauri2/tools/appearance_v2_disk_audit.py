"""Exact-image, disk-only V2 player appearance measurement proof.
No process access, native calls, roster/key reads, or game writes. Research only.
"""
from __future__ import annotations
import argparse, hashlib, json, mmap, os, struct
from pathlib import Path
SHA = "29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1"
PROOFS = [
 ("height getter: player+78 -> word0 &7fff", 0x2E24A70, "488b41780fb70025ff7f0000c3"),
 ("wingspan getter: player+78 -> word2 &7fff", 0x2E24B60, "488b41780fb7400225ff7f0000c3"),
 ("height setter preserves bit15", 0x2E24A87, "488b41780fb7ca81f9ff7f0000b9ff7f00000f42ca41b80080ffff4423004109c866448900"),
 ("wingspan setter preserves bit15", 0x2E24B77, "488b41780fb7ca81f9ff7f0000b9ff7f00000f42ca440fb740024181e00080ffff4109c86644894002"),
 ("same roster player has uid128/face110 and height getter", 0x31A45E0, "89e9e8798cbdfe440fb7b0280100004101fe440fb7b8100100004889c1e86e04c8ff"),
 ("same player height-set weight108 and wingspan-set", 0x1B0DE74, "0fb757024889f1e8006c3101807f040175d7c5fa104708c5fa118608010000807f0c0175ca0fb7570e4889f1e8cb6c3101"),
 ("wingspan float getter masked then cm scaling", 0x2E14490, "488b41780fb7400225ff7f0000c5d22ac0c5fa5905b7a0ef02c3"),
]
NAMES = [
 (0x2E24B3A, 0x68ED802, "APPEARANCEDATA::SetHeightInTenthsOfMM"),
 (0x2E24C2E, 0x68ED8E0, "APPEARANCEDATA::SetWingspanInTenthsOfMM"),
]
class Image:
 def __init__(self, data):
  self.data=data
  if data[:2] != b"MZ": raise ValueError("Not an executable")
  pe=struct.unpack_from("<I",data,60)[0]
  if data[pe:pe+4]!=b"PE\0\0": raise ValueError("Missing PE")
  if struct.unpack_from("<H",data,pe+4)[0]!=0x8664 or struct.unpack_from("<I",data,pe+8)[0]!=0x6AC53765 or struct.unpack_from("<I",data,pe+24+56)[0]!=0x35BDE000: raise ValueError("Not exact V2 PE")
  count=struct.unpack_from("<H",data,pe+6)[0]; at=pe+24+struct.unpack_from("<H",data,pe+20)[0]
  self.sections=[struct.unpack_from("<IIII",data,at+i*40+8) for i in range(count)]
  self.pdata,self.pdata_size=struct.unpack_from("<II",data,pe+24+112+24)
 def read(self,rva,size):
  for _,base,length,offset in self.sections:
   if base<=rva and rva+size<=base+length:return self.data[offset+rva-base:offset+rva-base+size]
  raise ValueError(f"Unbacked RVA {rva:x}+{size}")
def verify_code(image):
 result=[]
 for label,rva,expected in PROOFS:
  expected=bytes.fromhex(expected); actual=image.read(rva,len(expected))
  if actual!=expected:raise ValueError(f"Changed native proof: {label}")
  result.append({"label":label,"rva":hex(rva),"bytes":len(actual),"sha256":hashlib.sha256(actual).hexdigest()})
 for instruction,target,name in NAMES:
  code=image.read(instruction,7)
  if code[:3]!=bytes.fromhex("4c8d0d") or instruction+7+struct.unpack_from("<i",code,3)[0]!=target or image.read(target,len(name)+1)!=name.encode()+b"\0":raise ValueError("Changed named appearance setter assertion")
 return result
def audit(path):
 with Path(path).open("rb") as file,mmap.mmap(file.fileno(),0,access=mmap.ACCESS_READ) as data:
  image=Image(data)
  if hashlib.sha256(data).hexdigest().upper()!=SHA:raise ValueError("Exact image SHA mismatch")
  proofs=verify_code(image)
  setters=[];pdata=image.read(image.pdata,image.pdata_size)
  for expected_begin,expected_end in [(0x2E24A80,0x2E24B53),(0x2E24B70,0x2E24C47)]:
   row=next((struct.unpack_from("<III",pdata,i) for i in range(0,len(pdata),12) if struct.unpack_from("<I",pdata,i)[0]==expected_begin),None)
   if not row or row[1]!=expected_end:raise ValueError("Changed setter pdata boundaries")
   setters.append({"begin":hex(row[0]),"end":hex(row[1]),"unwind":hex(row[2]),"sha256":hashlib.sha256(image.read(row[0],row[1]-row[0])).hexdigest()})
  scale_rva=0x2E144A9+struct.unpack("<i",image.read(0x2E144A5,4))[0]
  scale=struct.unpack("<f",image.read(scale_rva,4))[0]
  if abs(scale-0.01)>1e-8:raise ValueError("Changed cm scale")
  return {"schema":"qa066-v2-appearance-disk-proof-v1","imageSha256":SHA,"playerAppearancePointerOffset":"0x78","height":{"offset":0,"bits":15,"mask":"0x7fff","unit":"tenths of millimeter","cmDivisor":100},"wingspan":{"offset":2,"bits":15,"mask":"0x7fff","unit":"tenths of millimeter","cmDivisor":100},"cmScale":{"rva":hex(scale_rva),"f32":scale},"preserve":"bit15 of each u16; flag semantics unproven","nativeCodeProofs":proofs,"setters":setters,"liveAccess":False,"productionEnabled":False,"remaining":["Retained appearance-owner capability, roster directory membership and unique ownership/alias proof","Double stable player+78 pointer, 4byte privateRW plain-region preimage, bounded masked journal transaction and exact readback","Explicit user-run save verification and surrounding-field stability"]}
def main():
 ap=argparse.ArgumentParser();ap.add_argument("image");ap.add_argument("--out",type=Path,required=True);args=ap.parse_args()
 result=audit(args.image)
 with args.out.open("w",encoding="utf-8",newline="\n") as out:json.dump(result,out,indent=2);out.write("\n");out.flush();os.fsync(out.fileno())
 print("PASS exact V2 appearance accessors, named mask-preserving setters, player callers and units; production remains disabled")
if __name__=="__main__":main()
