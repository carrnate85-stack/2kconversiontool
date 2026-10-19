"""Read only existing decoded scratch rosters and the pinned executable; no keys or game access."""
from pathlib import Path
import argparse,collections,hashlib,json,mmap,os,struct,sys
sys.path.insert(0,str(Path(__file__).resolve().parent))
from rdat_roster import Roster,get,ref_slots,wide,NULL_REF
from roster_merge_wnba import V2,field_by_name

def native_mix(value):
 value=(value-(value<<6))&0xffffffff
 value^=value>>17
 value=((value-(value<<9))^(value*0xffffe010))&0xffffffff
 value=((value-(value<<3))^(value*0xffffe400))&0xffffffff
 return value^(value>>15)

def inspect(path):
 raw=path.read_bytes();r=Roster(raw);p=r.tables[V2['player']];f=field_by_name(p,V2['player_id'])
 ids=[get(row,p.bits,f.offset,f.width)for row in p.all_rows()]
 table=r.tables['222e3502'];assert table.bits==16
 slots=table.all_rows();count=len(slots)-2*len(ids);assert count>0
 def lookup(key):
  at=native_mix(key)%count
  for probe in range(count):
   row=slots[(at+probe)%count]
   if row==65535:return None
   if row<len(ids)and ids[row]==key:return row
  return None
 passes=[i for i,key in enumerate(ids)if lookup(key)==i]
 regions=[slots[:count],slots[count:count+len(ids)],slots[count+len(ids):]]
 return r,ids,{'sha256':hashlib.sha256(raw).hexdigest(),'players':p.rows,'slots':len(slots),'hashBuckets':count,
  'indexSlotsSha256':hashlib.sha256(table.data).hexdigest(),'nativeIdLookupPass':len(passes),'nativeIdLookupFail':len(ids)-len(passes),
  'missingPlayerRows':sorted(set(range(p.rows))-set(slots)),
  'regions':[{'slots':len(a),'uniquePlayerRows':len(set(a)-{65535}),'emptySlots':a.count(65535)}for a in regions]}

def main():
 a=argparse.ArgumentParser();a.add_argument('--scratch',type=Path,required=True);a.add_argument('--exe',type=Path,required=True);a.add_argument('--out',type=Path,required=True);args=a.parse_args()
 assert not args.out.exists() and args.out.parent.name=='artifacts'
 b,bids,base=inspect(args.scratch/'decoded/learn2k_roster.decoded.iff')
 s,sids,source=inspect(args.scratch/'decoded/learn2k_wnba_roster.decoded.iff')
 r,ids,mixed=inspect(args.scratch/'merged-wpf-era/learn2k_roster.merged.decoded.iff')
 assert base['nativeIdLookupFail']==source['nativeIdLookupFail']==0
 assert mixed['missingPlayerRows']==list(range(len(bids),len(ids)))
 assert b.tables['222e3502'].data==r.tables['222e3502'].data
 tt=r.tables[V2['team']];tm=r.type_map();p=r.tables[V2['player']];idf=field_by_name(tt,V2['team_id']);nf=field_by_name(tt,V2['team_name']);kf=field_by_name(tt,V2['team_type_names'][0])
 refs=[(f,off)for f,off in ref_slots(tt)if tm.get(f.type_hash)==[V2['player']]]
 clubs=[];using=[];clubplayers=set();appended=set()
 for ti,row in enumerate(tt.all_rows()):
  teamid=get(row,tt.bits,idf.offset,idf.width);kind=get(row,tt.bits,kf.offset,kf.width)
  players=[get(row,tt.bits,off,64)>>32 for _,off in refs if get(row,tt.bits,off,64)!=NULL_REF]
  bad=[i for i in players if i>=p.rows];extra=sorted(set(i for i in players if i>=len(bids)));assert not bad
  item={'teamId':teamid,'kind':kind,'name':wide(row,tt.bits,nf.offset,nf.count),'directPlayerReferences':len(players),'uniquePlayers':len(set(players)),'appendedRows':extra}
  if kind==29:clubs.append(item);clubplayers.update(players)
  if extra:using.append(item);appended.update(extra)
 assert len(clubs)==15 and len(clubplayers)==209 and not any(i['appendedRows']for i in clubs)
 assert appended==set(range(len(bids),len(ids)))
 assert all(ids[i]==bids[i] for i in clubplayers)
 exe_hash=hashlib.sha256(args.exe.read_bytes()).hexdigest();assert exe_hash=='29a96471c55909fc2bcea6c699c59238656a501c19b50c74ed64b0d842e576a1'
 sys.path.insert(0,str(Path(__file__).resolve().parents[2]/'research/python_modules'))
 from capstone import Cs,CS_ARCH_X86,CS_MODE_64
 functions={}
 with args.exe.open('rb')as file,mmap.mmap(file.fileno(),0,access=mmap.ACCESS_READ)as m:
  pe=struct.unpack_from('<I',m,60)[0];n=struct.unpack_from('<H',m,pe+6)[0];sz=struct.unpack_from('<H',m,pe+20)[0];sections=[]
  for i in range(n):
   at=pe+24+sz+i*40;name=m[at:at+8].rstrip(b'\0').decode();vs,rva,rs,raw=struct.unpack_from('<IIII',m,at+8);sections.append((name,rva,raw,rs))
  def read(rva,n):
   for _,va,raw,rs in sections:
    if va<=rva<va+rs:return m[raw+rva-va:raw+rva-va+n]
   raise ValueError('unmapped')
  _,_,raw,size=next(x for x in sections if x[0]=='.pdata');pdata=dict((v[0],v[1])for v in(struct.unpack_from('<III',m,raw+i)for i in range(0,size-size%12,12))if v[0])
  md=Cs(CS_ARCH_X86,CS_MODE_64)
  for start,size_hint in [(0x21182b0,0),(0x2f08e80,0),(0x2ed9e00,0),(0x2f49fe0,0),(0x3e4aa10,0xe8),(0x3e4fd00,0),(0x2133f00,0),(0x212f950,0),(0x2133fe0,0),(0x21365e0,0),(0x2fd0cc0,0x25),(0x2fd0cf0,0x24),(0x2fd0d20,0x24)]:
   end=pdata.get(start,start+size_hint);assert end>start,hex(start)
   code=read(start,end-start)
   functions[f'{start:08X}']={'end':f'{end:08X}','sha256':hashlib.sha256(code).hexdigest(),'instructions':[{'rva':f'{x.address:08X}','mnemonic':x.mnemonic,'operands':x.op_str}for x in md.disasm(code,start)]}
 report={'schema':'wnba-appended-player-index-offline-v1','gameProcessAccess':False,'gameCalls':0,'gameWrites':0,'cryptoKeyReads':0,'saltReads':0,'packWrites':0,
  'exeSha256':exe_hash,'base':base,'source':source,'mixed':mixed,'clubs':clubs,'uniqueClubPlayers':len(clubplayers),'clubPlayerIdsUnchanged':True,'teamsUsingAppended':using,
  'allAppendedReferenced':True,'missingNewIdRange':[min(ids[len(bids):]),max(ids[len(bids):])],
  'schemaFingerprintsUnchanged':all(b.tables[t].h(11)==r.tables[t].h(11)for t in b.order),
  'functions':functions,'limit':'Lookup simulation models native preserved-serialized-index path only. A native dirty/rebuild branch can rebuild these regions. No loaded-runtime index read or gameplay failure claimed; normal loader flag is passed from root+5C and still requires complete branch proof.'}
 with args.out.open('x',encoding='utf-8')as out:json.dump(report,out,indent=2);out.flush();os.fsync(out.fileno())
 print(json.dumps({'report':str(args.out),'clubs':len(clubs),'uniqueClubPlayers':len(clubplayers),'appended':len(appended),'baseLookupPass':base['nativeIdLookupPass'],'sourceLookupPass':source['nativeIdLookupPass'],'mixedPreservedIndexLookupPass':mixed['nativeIdLookupPass'],'functionsPinned':len(functions),'cryptoKeyReads':0}))
if __name__=='__main__':main()
