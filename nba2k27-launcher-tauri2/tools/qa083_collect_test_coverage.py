"""Verify three exact disjoint bounded offline test partitions before staging."""
from pathlib import Path
import hashlib,json,re,os,datetime
R=Path(__file__).resolve().parents[1]
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest().upper()
def put(p,data):
 with p.open('x',encoding='utf-8') as f:f.write(data);f.flush();os.fsync(f.fileno())
def main():
 expected=set(re.findall(r'^(.+): test$',(R/'artifacts/qa083-final-test-list.log').read_text(encoding='utf-8-sig'),re.M))
 assert len(expected)>=1464
 seen=set();passed=ignored=0;parts=[];texts=[]
 exe=R/'target-qa054/debug/deps/nba2k27_launcher_tauri2-281ce2d259e8f3fd.exe';pdb=exe.with_suffix('.pdb')
 for part in ['roster','catalog','rest']:
  log=R/f'artifacts/qa083-final-{part}-rust-tests.log'; capture=R/f'artifacts/qa074-natural-qa083-{part}-final-2.txt'
  text=log.read_text(encoding='utf-8-sig');c=capture.read_text(encoding='utf-8-sig')
  assert re.search(r'EXIT_CODE=0(?:\s|$)',c) and re.search(r'CLAIMED=0(?:\s|$)',c) and re.search(r'DUMP=0(?:\s|$)',c)
  assert 'sha256='+sha(exe).lower() in c and 'sha256='+sha(pdb).lower() in c
  entries=re.findall(r'^test (.+) \.\.\. (ok|ignored)(?:,.*)?$',text,re.M)
  names=[n for n,v in entries];assert len(names)==len(set(names))
  want={n for n in expected if (n.startswith('roster_v2_') if part=='roster' else n.startswith('player_catalog') if part=='catalog' else not n.startswith(('roster_v2_','player_catalog')))}
  assert set(names)==want,(part,len(names),len(want),want-set(names))
  assert not seen.intersection(names);seen.update(names)
  summaries=re.findall(r'test result: ok\. (\d+) passed; 0 failed; (\d+) ignored; 0 measured; (\d+) filtered out;',text)
  assert len(summaries)==1
  p,i,f=map(int,summaries[0]);assert p==sum(v=='ok' for n,v in entries) and i==sum(v=='ignored' for n,v in entries) and p+i+f==len(expected)
  passed+=p;ignored+=i;parts.append({'part':part,'log':str(log.relative_to(R)),'logSha256':sha(log),'capture':str(capture.relative_to(R)),'captureSha256':sha(capture),'passed':p,'ignored':i,'filtered':f});texts.append(text)
 assert seen==expected and passed+ignored==len(expected)
 report={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'completeExactPartition':True,'testExeSha256':sha(exe),'testPdbSha256':sha(pdb),'testListSha256':sha(R/'artifacts/qa083-final-test-list.log'),'enumerated':len(expected),'passed':passed,'ignored':ignored,'parts':parts,'underDebugger':False,'qa073NaturalAvResolved':False}
 put(R/'artifacts/qa083-final-test-coverage.json',json.dumps(report,indent=2)+'\n')
 # Original failed whole run remains in its exact evidence archive and explicit log.
 path=R/'artifacts/qa083-final-full-rust-tests.log'
 with path.open('w',encoding='utf-8') as f:f.write('\n'.join(texts)+f'\nExact disjoint partition aggregate:\ntest result: ok. {passed} passed; 0 failed; {ignored} ignored; 0 measured; 0 filtered out;\n');f.flush();os.fsync(f.fileno())
 print(json.dumps(report,indent=2))
if __name__=='__main__':main()
