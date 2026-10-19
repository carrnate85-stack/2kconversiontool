"""Create only fresh raw index research artifacts; never modify IFF, packs or archives."""
import argparse,hashlib,json,os,sys,zlib
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parent))
from rdat_roster import Roster,get,pack_rows
from roster_merge_wnba import V2,field_by_name
from wnba_player_index_research import rebuild,verify,encode
EXPECTED={'source':'2e5f46372c186e5ef55a575032165bf5e13fc46f1e4e8aa1ada9e14ca7bfe640','base':'cd5463c17ab1819879530e09ad3e5a8c6b0aadce9c612b72801f4b598a64392f','mixed':'8827184c6bf3ec8d7a833db8f71880ee9e9af6c35609d26ae2d0c2559ed89fe5'}
def read(path,label):
 raw=path.read_bytes();digest=hashlib.sha256(raw).hexdigest()
 if label in EXPECTED:assert digest==EXPECTED[label]
 r=Roster(raw);table=r.tables[V2['player']];rows=table.all_rows();fields=[field_by_name(table,zlib.crc32(name.encode()))for name in ['UniqueId','AudioSignatureId','SignatureId']]
 keys=[[get(row,table.bits,f.offset,f.width)for row in rows]for f in fields];index=r.tables['222e3502'];slots=index.all_rows()
 built=rebuild(*keys,len(slots)-2*table.rows);proof=verify(built,*keys)
 return keys,built,index,{'input':str(path),'bytes':len(raw),'sha256':digest,'players':table.rows,'slots':index.rows,'bucketCount':len(slots)-2*table.rows,'storedIndexBytes':len(index.data),'storedIndexSha256':hashlib.sha256(index.data).hexdigest(),'rebuiltMatchesStoredAll3Regions':pack_rows(built,index.bits)==index.data,'proof':proof}
def main():
 p=argparse.ArgumentParser();p.add_argument('--scratch',type=Path,required=True);p.add_argument('--out',type=Path,required=True);a=p.parse_args();root=Path(__file__).resolve().parents[1]/'artifacts'
 assert not a.out.exists() and a.out.resolve().is_relative_to(root.resolve())
 b,built_b,index_b,base=read(a.scratch/'decoded/learn2k_roster.decoded.iff','base')
 s,built_s,index_s,source=read(a.scratch/'decoded/learn2k_wnba_roster.decoded.iff','source')
 m,built_m,index_m,mixed=read(a.scratch/'merged-wpf-era/learn2k_roster.merged.decoded.iff','mixed')
 assert base['rebuiltMatchesStoredAll3Regions']and source['rebuiltMatchesStoredAll3Regions']
 assert all(m[i][:len(b[i])]==b[i]for i in range(3));assert index_m.rows==18734 and len(m[0])==5563
 native=encode(built_m);packed=pack_rows(built_m,16);assert len(native)==len(packed)==len(index_m.data)==37468
 a.out.mkdir()
 for name,body in [('index.bin',native),('index-rdat-bits.bin',packed)]:
  with(a.out/name).open('xb')as f:f.write(body);f.flush();os.fsync(f.fileno())
 report={'schema':'wnba-player-index-research-candidate-v1','researchOnly':True,'gameProcessAccess':False,'gameCalls':0,'gameWrites':0,'keyReads':0,'saltReads':0,'packWrites':0,'iffWrites':0,
 'base':base,'source':source,'mixed':mixed,'fields':{'hash':'UniqueId/u16/native+128/schema31BD8AF6','sortedSecond':'AudioSignatureId/u32/native+124/schema4664F677','sortedThird':'SignatureId/u32/native+120/schema32442C7F'},
 'oldBaseKeysAndRowIndicesUnchanged':True,'nativeMemoryRepresentation':'index.bin = little-endian u16 array; index-rdat-bits.bin = original RDAT MSB-first row encoding, no chunk/header/checksum/container bytes',
 'candidate':{'slots':len(built_m),'hashBuckets':7608,'sortedRowsEach':5563,'bytes':len(native),'nativeSha256':hashlib.sha256(native).hexdigest(),'rdatPayloadSha256':hashlib.sha256(packed).hexdigest()},
 'limit':'No full IFF output or source pack mutation. Old row keys/lookup semantics preserved; hash bucket positions cannot remain byte-identical when partition modulus changes. No runtime/gameplay claim until normal loader flag and loaded index proof. Existing missing-index limitation may be repaired by native dirty rebuild; candidate is research only.'}
 with(a.out/'manifest.json').open('x',encoding='utf-8')as f:json.dump(report,f,indent=2);f.flush();os.fsync(f.fileno())
 print(json.dumps({'output':str(a.out),'stockBase3RegionsExact':True,'stockWnba3RegionsExact':True,'mixedLookupPass':5563,'sortedAliasesPass':11126,'rawBytes':len(native),'packWrites':0,'keyReads':0,'saltReads':0}))
if __name__=='__main__':main()
