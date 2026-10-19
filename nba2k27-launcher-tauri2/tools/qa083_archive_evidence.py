"""Archive the exact passing QA083 dev/test evidence; never run or activate it."""
from pathlib import Path
import hashlib,json,zipfile,os,datetime
ROOT=Path(__file__).resolve().parents[1]
def sha(path):
 with path.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest().upper()
def main():
 stage=json.loads((ROOT/'artifacts/qa083-final-stage-20261019.json').read_text())
 inputs=json.loads((ROOT/stage['buildInputs']).read_text())
 for row in inputs['sourceFiles']+inputs['embeddedAssets']+inputs['buildScriptsAndRegressionSources']:
  assert sha(ROOT/row['path'])==row['sha256'],row['path']
 names={row['path'] for row in inputs['sourceFiles']+inputs['embeddedAssets']+inputs['buildScriptsAndRegressionSources']}
 names.update(['Dev/Pending/QA083/launcher.exe','Start QA083 Dev.ps1','Start QA083 Dev.vbs',
  'target-qa054/debug/deps/nba2k27_launcher_tauri2-281ce2d259e8f3fd.exe',
  'target-qa054/debug/deps/nba2k27_launcher_tauri2-281ce2d259e8f3fd.pdb',
  'target-qa054/debug/nba2k27_launcher_tauri2.pdb','assets/native/version-outfits-v2.dll',
  'NEXT-CHAT-HANDOFF-QA083.txt','tools/qa083_archive_evidence.py'])
 names.update(f.relative_to(ROOT).as_posix() for f in (ROOT/'artifacts').glob('qa083*.json'))
 names.update(f.relative_to(ROOT).as_posix() for f in (ROOT/'artifacts').glob('qa083*.log'))
 names.update(f.relative_to(ROOT).as_posix() for f in (ROOT/'artifacts').glob('qa074-natural-qa083-*.txt'))
 names.update(f.relative_to(ROOT).as_posix() for d in (ROOT/'artifacts').glob('qa083-*') if d.is_dir() for f in d.rglob('*') if f.is_file() and f.suffix in {'.json','.log','.md','.txt'})
 archive=ROOT/'Backups/qa083-passing-test-and-dev-20261019/exact-images-and-sources.zip'
 archive.parent.mkdir(parents=True,exist_ok=True)
 rows=[]
 for name in sorted(names):
  f=ROOT/name;assert f.is_file(),name
  rows.append({'path':name,'bytes':f.stat().st_size,'sha256':sha(f)})
 with archive.open('xb') as raw:
  with zipfile.ZipFile(raw,'w',zipfile.ZIP_DEFLATED,compresslevel=6) as z:
   for row in rows:z.write(ROOT/row['path'],row['path'])
  raw.flush();os.fsync(raw.fileno())
 with zipfile.ZipFile(archive) as z:
  assert len(z.infolist())==len(rows)
  for row in rows:
   with z.open(row['path']) as f:assert hashlib.file_digest(f,'sha256').hexdigest().upper()==row['sha256'],row['path']
 report={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'archive':str(archive),
  'bytes':archive.stat().st_size,'sha256':sha(archive),'roundtripVerified':True,'files':rows,
  'devOnly':True,'activated':False,'installed':False,'promoted':False,'qa073NaturalAvResolved':False}
 with (ROOT/'artifacts/qa083-exact-evidence-archive-20261019.json').open('x',encoding='utf-8') as f:
  json.dump(report,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
 print(json.dumps({k:v for k,v in report.items() if k!='files'}))
if __name__=='__main__':main()
