"""Archive the exact passing QA082 dev/test evidence; never run or activate it."""
from pathlib import Path
import hashlib,json,zipfile,os,datetime
ROOT=Path(__file__).resolve().parents[1]
def sha(path):
 with path.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest().upper()
def main():
 stage=json.loads((ROOT/'artifacts/qa082-final-stage-20261019.json').read_text())
 inputs=json.loads((ROOT/stage['buildInputs']).read_text())
 for row in inputs['sourceFiles']+inputs['embeddedAssets']+inputs['buildScriptsAndRegressionSources']:
  assert sha(ROOT/row['path'])==row['sha256'],row['path']
 names={row['path'] for row in inputs['sourceFiles']+inputs['embeddedAssets']+inputs['buildScriptsAndRegressionSources']}
 names.update(['Dev/Pending/QA082/launcher.exe','Start QA082 Dev.ps1','Start QA082 Dev.vbs',
  'target-qa054/debug/deps/nba2k27_launcher_tauri2-281ce2d259e8f3fd.exe',
  'target-qa054/debug/deps/nba2k27_launcher_tauri2-281ce2d259e8f3fd.pdb',
  'target-qa054/debug/nba2k27_launcher_tauri2.pdb','assets/native/version-outfits-v2.dll',
  'NEXT-CHAT-HANDOFF-QA082.txt','tools/qa082_archive_evidence.py'])
 names.update(f.relative_to(ROOT).as_posix() for f in (ROOT/'artifacts').glob('qa082*.json'))
 names.update(f.relative_to(ROOT).as_posix() for f in (ROOT/'artifacts').glob('qa082*.log'))
 names.update(['artifacts/qa074-natural-qa082-whole-1.txt','artifacts/qa074-natural-qa082-whole-3.txt','artifacts/qa074-natural-qa082-whole-4.txt','artifacts/qa074-natural-qa082-storage-check-2.txt'])
 names.update(f.relative_to(ROOT).as_posix() for f in (ROOT/'artifacts/qa082-editor-corrections-20261019').glob('*.json'))
 archive=ROOT/'Backups/qa082-passing-test-and-dev-20261019/exact-images-and-sources.zip'
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
 with (ROOT/'artifacts/qa082-exact-evidence-archive-20261019.json').open('x',encoding='utf-8') as f:
  json.dump(report,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
 print(json.dumps({k:v for k,v in report.items() if k!='files'}))
if __name__=='__main__':main()
