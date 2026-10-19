from pathlib import Path
import importlib.util,json,os,datetime
root=Path.cwd()
spec=importlib.util.spec_from_file_location('qa083_stage',root/'tools/qa083_stage_dev_checkpoint.py')
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
scripts=['scripts/prepare-browser-workspace.mjs','scripts/test-editor-capability-staging.mjs','scripts/test-season-import-staging.mjs','scripts/test-team-dna-v2.mjs','scripts/test-custom-catalog-source-generation.mjs','scripts/test-qa083-import-workflows.mjs','scripts/test-qa083-saved-ratings-ui.mjs','scripts/test-qa083-team-metadata-ui.mjs','scripts/test-qa083-player-origin-ui.mjs','tools/qa083_stage_dev_checkpoint.py','tools/qa083_archive_evidence.py','tools/qa083_collect_test_coverage.py','tools/qa083_freeze_build_inputs.py','tools/qa083_verify_preservation.py']
def rows(names):
 return [{'path':n,'sha256':m.sha(root/n),'bytes':(root/n).stat().st_size} for n in names]
assets=[p.relative_to(root).as_posix() for p in sorted((root/'assets').rglob('*')) if p.is_file()]
obj={'schema':'qa083-build-input-freeze-v1','utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'devOnly':True,'sourceFiles':m.source_files(),'buildScriptsAndRegressionSources':rows(scripts),'embeddedAssets':rows(assets)}
p=root/'artifacts/qa083-final-build-inputs-20261019.json'
with p.open('x',encoding='utf-8') as f:json.dump(obj,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
print(len(obj['sourceFiles']),len(scripts),len(assets))
