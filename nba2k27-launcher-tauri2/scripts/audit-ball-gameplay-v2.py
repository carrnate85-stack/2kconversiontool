"""Exact V2 PE-only gameplay-ball model provenance; never opens a process."""
from pathlib import Path
import argparse,bisect,hashlib,json,mmap,re,runpy,struct
ROOT=Path(__file__).resolve().parents[1];A=runpy.run_path(str(ROOT/'scripts/audit-roster-v2-fields.py'))
IMAGE_SHA='B673EB04593D6D5A4DFB9E270178E04C017BD9F277514D256EA98FFB792ED27B'
def main():
 p=argparse.ArgumentParser();p.add_argument('image',type=Path);p.add_argument('--output',type=Path,required=True);args=p.parse_args()
 with args.image.open('rb') as f:assert hashlib.file_digest(f,'sha256').hexdigest().upper()==IMAGE_SHA
 pe=A['pefile'].PE(str(args.image),fast_load=True);md=A['Cs'](A['CS_ARCH_X86'],A['CS_MODE_64']);md.skipdata=True
 pdata=next(s for s in pe.sections if s.Name.rstrip(b'\0')==b'.pdata');functions=sorted((s,e)for s,e,_ in struct.iter_unpack('<III',pdata.get_data()[:pdata.SizeOfRawData//12*12])if 0x1000<=s<e<0x6500000);starts=[s for s,e in functions]
 rows=[]
 with args.image.open('rb') as f,mmap.mmap(f.fileno(),0,access=mmap.ACCESS_READ)as image:
  text=next(s for s in pe.sections if s.Name.rstrip(b'\0')==b'.text');data=image[text.PointerToRawData:text.PointerToRawData+text.SizeOfRawData]
  for label,needle in [('gameplay-default-model-42A7A5EF',bytes.fromhex('efa5a742')),('legacy-default-alias-DC1BFEC5',bytes.fromhex('c5fe1bdc')),('AI-ball-model-offset34C',bytes.fromhex('4c030000')),('GameData-ball-offsetDA4',bytes.fromhex('a40d0000'))]:
   hits=[];at=0;seen=set()
   while True:
    at=data.find(needle,at)
    if at<0:break
    rva=text.VirtualAddress+at;at+=1
    if label=='AI-ball-model-offset34C' and not 0x4000000<=rva<0x4200000:continue
    index=bisect.bisect_right(starts,rva)-1
    if index<0 or rva>=functions[index][1]:continue
    start,end=functions[index]
    if start in seen:continue
    seen.add(start);lo=max(start,rva-160);hi=min(end,rva+260)
    # Full native function decoding preserves instruction boundaries. Output
    # only a bounded local window; do not infer meaning from raw DWORD hits.
    ins=list(md.disasm(pe.get_data(start,min(end-start,65536)),start));window=[i for i in ins if lo<=i.address<hi]
    hits.append({'referenceRva':f'{rva:08X}','functionStart':f'{start:08X}','functionEnd':f'{end:08X}','instructions':[f'{i.address:08X} {i.bytes.hex()} {i.mnemonic} {i.op_str}'for i in window]})
    assert len(hits)<=512,f'Bounded provenance hit budget: {label}'
   rows.append({'name':label,'hits':hits})
  calls=[]
  for hit in re.finditer(rb'\xe8....',data,re.DOTALL):
   rva=text.VirtualAddress+hit.start();target=rva+5+struct.unpack('<i',hit.group()[1:])[0]
   if target not in (0x25A7AF0,0x25A7B00):continue
   i=bisect.bisect_right(starts,rva)-1;start,end=functions[i];lo=max(start,rva-100);hi=min(end,rva+100);ins=md.disasm(pe.get_data(start,min(end-start,65536)),start)
   calls.append({'rva':f'{rva:08X}','target':f'{target:08X}','functionStart':f'{start:08X}','instructions':[f'{i.address:08X} {i.bytes.hex()} {i.mnemonic} {i.op_str}'for i in ins if lo<=i.address<hi]})
  rows.append({'name':'native-gamedata-ball-getter-callers','hits':calls})
 report={'schema':'nba2k27-v2-gameplay-ball-offline-provenance-v1','sha256':IMAGE_SHA,'gameProcessAccess':False,'gameFilesChanged':False,'candidateOnly':True,'rows':rows};args.output.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8');print(json.dumps({'output':str(args.output),'counts':{r['name']:len(r['hits'])for r in rows},'gameProcessAccess':False}))
if __name__=='__main__':main()
