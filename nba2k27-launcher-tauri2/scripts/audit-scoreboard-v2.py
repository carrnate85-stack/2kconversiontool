"""Installed Version 2 PE-only scoreboard provenance. No process APIs/writes.

The production WPF scalar consumers locate candidate functions. This script
prints native function-boundary disassembly, not a guessed relocation delta.
"""
from pathlib import Path
import argparse,bisect,hashlib,json,mmap,re,runpy,struct

ROOT=Path(__file__).resolve().parents[1]
A=runpy.run_path(str(ROOT/"scripts/audit-roster-v2-fields.py"))
PATTERNS={
 "package-table-stride24-limit128":"4883c6184881fe000c0000",
 "descriptor-property-sentinel":"4c8d6e18448b4e18",
 "cached-scorebug-scalar-consumer":"8138f01035c675ef4885c074068b4008894120",
 "instance-resource-copy":"8b57208993e8020000",
 "property-search":"4d8b49504939c9740e45390175f24d85c97404418b5108",
 "registry-class-hash":"3b27e5dc",
 "registry-bucket-key":"c4628e98",
 "registry-type-key":"18f7e57d",
 "blacktop-controller-class":"ccdf222a",
 "eras-controller-class":"3ae6af75",
 "package-property-parser":"56574883ec484c89c64889cf4c894c2440488d4c2440",
}
def main():
 p=argparse.ArgumentParser();p.add_argument("image",type=Path);p.add_argument("--output",type=Path,required=True);p.add_argument("--anchors",type=Path);args=p.parse_args()
 with args.image.open("rb") as f: assert hashlib.file_digest(f,"sha256").hexdigest().upper()==A["IMAGE_SHA"],"Exact Version 2 image required"
 pe=A["pefile"].PE(str(args.image),fast_load=True)
 assert pe.FILE_HEADER.TimeDateStamp==0x6AC53765 and pe.OPTIONAL_HEADER.SizeOfImage==0x35BDE000
 pdata=next(s for s in pe.sections if s.Name.rstrip(b"\0")==b".pdata")
 functions=[]
 for start,end,_ in struct.iter_unpack("<III",pdata.get_data()[:pdata.SizeOfRawData//12*12]):
  if 0x1000<=start<end<0x6500000:functions.append((start,end))
 functions.sort();starts=[r[0] for r in functions]
 md=A["Cs"](A["CS_ARCH_X86"],A["CS_MODE_64"])
 md.skipdata=True
 rows=[]
 with args.image.open("rb") as f,mmap.mmap(f.fileno(),0,access=mmap.ACCESS_READ) as image:
  text=next(s for s in pe.sections if s.Name.rstrip(b"\0")==b".text")
  lo,hi=text.PointerToRawData,text.PointerToRawData+text.SizeOfRawData
  for name,value in PATTERNS.items():
   needle=bytes.fromhex(value);at=lo;hits=[]
   while True:
    found=image.find(needle,at,hi)
    if found<0:break
    rva=pe.get_rva_from_offset(found);index=bisect.bisect_right(starts,rva)-1
    bounds=functions[index] if index>=0 and rva<functions[index][1] else (rva,rva+len(needle))
    start,end=bounds;end=min(end,start+8192)
    data=pe.get_data(start,end-start)
    hits.append({"rva":f"{rva:08X}","functionStart":f"{start:08X}","functionEnd":f"{end:08X}","instructions":[f"{i.address:08X} {i.bytes.hex()} {i.mnemonic} {i.op_str}" for i in md.disasm(data,start)]})
    at=found+1
    assert len(hits)<=256,"Ambiguous signature budget"
   rows.append({"name":name,"pattern":value,"hits":hits})
  for name,rva,length in [("native-resource-root-getter",0x507FE10,0x180),("blacktop-controller-constructor",0x18E7C70,0x80),("blacktop-base-constructor",0x18E5610,0x200),("scorebug-base-constructor",0x11338B0,0x4000),("resource-table-lookup",0x50D1980,0x230),("native-resource-bucket-lookup",0x50D0C70,0x500),("secondary-active-user",0x1055490,0x200),("native-resource-data-getter",0x50C5D70,0x80),("native-package-node-constructor",0x11944F0,0x510)]:
   data=pe.get_data(rva,length)
   rows.append({"name":name,"pattern":"","hits":[{"rva":f"{rva:08X}","functionStart":f"{rva:08X}","functionEnd":f"{rva+length:08X}","instructions":[f"{i.address:08X} {i.bytes.hex()} {i.mnemonic} {i.op_str}" for i in md.disasm(data,rva)]}]})
  table_refs=[]
  for match in re.finditer(rb'[\x48\x4c][\x8d\x8b\x89][\x05\x0d\x15\x1d\x25\x2d\x35\x3d]....',image[lo:hi],re.DOTALL):
   rva=pe.get_rva_from_offset(lo+match.start());target=rva+7+struct.unpack('<i',match.group()[3:7])[0]
   if not 0xA98CC30<=target<0xA98D850:continue
   index=bisect.bisect_right(starts,rva)-1;start,end=functions[index];end=min(end,start+8192)
   table_refs.append({"rva":f"{rva:08X}","targetRva":f"{target:08X}","functionStart":f"{start:08X}","functionEnd":f"{end:08X}","instructions":[f"{i.address:08X} {i.bytes.hex()} {i.mnemonic} {i.op_str}" for i in md.disasm(pe.get_data(start,end-start),start)]})
  rows.append({"name":"package-table-native-references","pattern":"RIP references within the independently identified 128-slot package table","hits":table_refs})
  rdata=next(s for s in pe.sections if s.Name.rstrip(b"\0")==b".rdata")
  needle=struct.pack("<Q",pe.OPTIONAL_HEADER.ImageBase+0x50C5D70);at=rdata.PointerToRawData;end=at+rdata.SizeOfRawData;vtables=[]
  while True:
   found=image.find(needle,at,end)
   if found<0:break
   slot=pe.get_rva_from_offset(found);vt=slot-0xC8
   locator=struct.unpack("<Q",pe.get_data(vt-8,8))[0]-pe.OPTIONAL_HEADER.ImageBase
   if 0x6500000<=locator<0xAA00000:
    signature,offset,ctor,type_rva,hierarchy,self_rva=struct.unpack("<6I",pe.get_data(locator,24))
    if signature==1 and self_rva==locator:
     typename=pe.get_data(type_rva+16,512).split(b"\0",1)[0].decode("ascii",errors="replace")
     vtables.append({"vtableRva":f"{vt:08X}","slotRva":f"{slot:08X}","methodRva":"050C5D70","locatorRva":f"{locator:08X}","objectOffset":offset,"type":typename})
   at=found+8
 report={"schema":"nba2k27-v2-scoreboard-native-discovery-v1","imageSha256":A["IMAGE_SHA"],"gameProcessAccess":False,"gameFilesChanged":False,"candidateOnly":True,"rows":rows,"resourceVtables":vtables}
 args.output.write_text(json.dumps(report,indent=2)+"\n",encoding="utf-8")
 if args.anchors:
  expected={"package-table-stride24-limit128":2,"descriptor-property-sentinel":1,"cached-scorebug-scalar-consumer":1,"instance-resource-copy":1,"property-search":1}
  assert all(len(next(r for r in rows if r['name']==key)['hits'])==count for key,count in expected.items()),'Independent native consumers changed'
  generic=[v for v in vtables if v['type']=='.?AVVCRESOURCECONTEXT@@' and v['objectOffset']==0]
  assert len(generic)==1 and generic[0]['vtableRva']=='08101C38' and generic[0]['locatorRva']=='08101E00'
  proof=[(0x119504D,7,'package-table-lea'),(0x1195060,36,'package-stride-count-entry'),(0x119515C,19,'package-table-native-zero-init'),(0x119450A,31,'package-tree-root-and-children'),(0x11948D7,38,'package-storage-used-capacity-stride136-sentinel'),(0x1194911,47,'package-property-list-and-tree-initialization'),(0x10350F0,29,'property-next-key-value'),(0x118B639,19,'scorebug-cached-resource-consumer'),(0x1051F0B,9,'scorebug-instance-resource-copy'),(0x11344EF,7,'main-active-scorebug-store'),(0x1055484,7,'secondary-active-scorebug-load'),(0x507FE23,7,'native-resource-root-load'),(0x1053841,28,'scoreboard-registry-four-type-keys'),(0x1053867,39,'registry-count-rows-stride260'),(0x50D1023,27,'registry-bucket-count-and-pointer'),(0x50D1070,63,'registry-row-owner-type-checks'),(0x50D19B2,25,'registry-vtable-C8-dispatch'),(0x50C5D70,52,'typed-resource-data-getter')]
  rip=[(0x119504D,0xA98CC38),(0x119515C,0xA98CC40),(0x11344EF,0xA99B850),(0x1055484,0xA976900),(0x507FE23,0xBE95238)]
  assert all(rva+7+struct.unpack('<i',pe.get_data(rva+3,4))[0]==target for rva,target in rip),'Native globals do not resolve independently'
  anchors={"schema":"nba2k27-v2-scoreboard-anchors-v1","sha256":A['IMAGE_SHA'],"peTimestamp":0x6AC53765,"imageSize":0x35BDE000,"gameProcessAccess":False,"packageTableRva":0xA98CC40,"activeRva":0xA99B850,"secondaryActiveRva":0xA976900,"resourcesRva":0xBE95238,"resourceVtableRva":0x8101C38,"resourceMethodRva":0x50C5D70,"resourceLocatorRva":0x8101E00,"anchors":[{"rva":rva,"hex":pe.get_data(rva,size).hex(),"label":label} for rva,size,label in proof]}
  args.anchors.write_text(json.dumps(anchors,indent=2)+'\n',encoding='utf-8')
 print(json.dumps({"output":str(args.output),"counts":{r["name"]:len(r["hits"]) for r in rows},"gameProcessAccess":False,"gameFilesChanged":False}))
if __name__=="__main__":main()
