from pathlib import Path
import importlib.util,json,os,datetime
root=Path.cwd()
spec=importlib.util.spec_from_file_location('qa084_stage',root/'tools/qa084_stage_dev_checkpoint.py')
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
scripts=sorted({p.relative_to(root).as_posix() for p in (root/'scripts').iterdir() if p.is_file() and (p.name.startswith('test-') or p.name in {'prepare-browser-workspace.mjs','build-qa084-outfit-proxy.ps1'})} | {p.relative_to(root).as_posix() for p in (root/'tools').glob('qa084_*.py')})

def rows(names):
 return [{'path':n,'sha256':m.sha(root/n),'bytes':(root/n).stat().st_size} for n in names]
assets=[p.relative_to(root).as_posix() for p in sorted((root/'assets').rglob('*')) if p.is_file()]
obj={'schema':'qa084-build-input-freeze-v1','utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'devOnly':True,'sourceFiles':m.source_files(),'buildScriptsAndRegressionSources':rows(scripts),'embeddedAssets':rows(assets)}
p=root/'artifacts/qa084-final-build-inputs-v4-20261019.json'
with p.open('x',encoding='utf-8') as f:json.dump(obj,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
print(len(obj['sourceFiles']),len(scripts),len(assets))
