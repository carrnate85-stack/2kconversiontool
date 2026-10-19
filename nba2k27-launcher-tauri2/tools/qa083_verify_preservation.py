from pathlib import Path
import hashlib,json,os,datetime
r=Path.cwd()
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest().upper()
d=json.loads((r/'artifacts/qa083-final-build-inputs-20261019.json').read_text(encoding='utf-8'))
rows=d['sourceFiles']+d['buildScriptsAndRegressionSources']+d['embeddedAssets']
for x in rows:assert sha(r/x['path'])==x['sha256'],x['path']
checks=[('Dev/Pending/QA082/launcher.exe',53273600,'71404BF01756E4A63ED267BCF4860CD023BA8E0FCA1FBF84813E4646F63D882D'),('Dev/Pending/QA080/launcher.exe',53165568,'783B64372BFB73621AE3D6083EB2460D0AA42A140DEBFF27365315EDC330E7EC'),('assets/native/version-outfits-v2.dll',1215488,'83AF5D0613D1E90D51933F382C22D215F766A61A198CAFEB41E80606A1722518')]
for n,b,h in checks:assert (r/n).stat().st_size==b and sha(r/n)==h,n
out={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'verifiedFrozenInputs':len(rows),'priorStagesAndNativeDllUnchanged':[{'path':n,'bytes':b,'sha256':h} for n,b,h in checks],'devOnly':True,'gameLaunched':False,'installed':False,'promoted':False}
p=r/'artifacts/qa083-final-source-preservation-20261019.json'
with p.open('x',encoding='utf-8') as f:json.dump(out,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
print(json.dumps(out))
