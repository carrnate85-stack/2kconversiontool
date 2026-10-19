"""Archive the exact QA080 test/dev images and verify every retained member. No activation."""
from pathlib import Path
from datetime import datetime, timezone
import hashlib, json, os, runpy, zipfile
ROOT=Path(__file__).resolve().parents[1]
def sha(p):
    with p.open('rb') as f: return hashlib.file_digest(f,'sha256').hexdigest().upper()
def save(name,data):
    with (ROOT/name).open('xb') as f:
        f.write((json.dumps(data,indent=2)+'\n').encode());f.flush();os.fsync(f.fileno())
def main():
    stage=json.loads((ROOT/'artifacts/qa080-final-stage-20261018.json').read_text())
    inputs=json.loads((ROOT/'artifacts/qa080-final-build-inputs-20261018.json').read_text())
    prior=json.loads((ROOT/'artifacts/qa079-final-stage-20261018.json').read_text())
    actual=runpy.run_path(str(ROOT/'tools/qa080_stage_dev_checkpoint.py'))['source_files']()
    assert actual==stage['sourceFiles']==inputs['sourceFiles'] and len(actual)==549
    old={i['path']:i['sha256'] for i in prior['sourceFiles']}
    changes=[i['path'] for i in actual if old[i['path']]!=i['sha256']]
    assert set(changes)=={'src/feature_runtime.rs','src/scaler_npc_v2.rs','src/scaler_npc_v2_tests.rs'}
    assert sha(ROOT/'Dev/Pending/QA080/launcher.exe')==stage['executable']['sha256']==sha(ROOT/'target-qa054/debug/nba2k27-launcher-tauri2.exe')
    assert sha(ROOT/'assets/native/version-outfits-v2.dll')==stage['payload']['sha256']==prior['payload']['sha256']
    active=ROOT/'Dev/Portable/App/2k Launcher Rust Prototype.exe'
    assert sha(active)==prior['executable']['sha256'],'Active dev changed; inspect before finalizing'
    assert 'EXIT_CODE=0 CLAIMED=0 DUMP=0' in (ROOT/'artifacts/qa074-natural-qa080-whole-1.txt').read_text(encoding='utf-8-sig')
    files=[
        'target-qa054/debug/deps/nba2k27_launcher_tauri2-281ce2d259e8f3fd.exe',
        'target-qa054/debug/deps/nba2k27_launcher_tauri2-281ce2d259e8f3fd.pdb',
        'target-qa054/debug/nba2k27_launcher_tauri2.pdb',
        'Dev/Pending/QA080/launcher.exe','assets/native/version-outfits-v2.dll',
        'Start QA080 Dev.ps1','Start QA080 Dev.vbs',
        'artifacts/qa080-final-full-rust-tests.log','artifacts/qa074-natural-qa080-whole-1.txt',
        'artifacts/qa080-final-rust-test-build.log','artifacts/qa080-final-dev-build.log',
        'artifacts/qa080-final-stage-20261018.json','artifacts/qa080-final-build-inputs-20261018.json',
        'artifacts/qa080-scaler-independent-review.json',
        'artifacts/qa080-scaler-player-npc-boundary-review.json',
        'artifacts/qa080-scaler-user-report-live-evidence-20261018.json',
        'artifacts/qa080-scaler-wpf-player-initialization-research.json',
        'artifacts/qa080-scaler-player-native-lifecycle-disk-20261018.json',
        'artifacts/qa080-scaler-player-native-lifecycle-disk-20261018.txt',
        'artifacts/qa080-starter-check.log','artifacts/qa080-vbs-check.log',
        'Backups/qa080-scaler-user-resize-20261018/provider-state.json',
        'src/feature_runtime.rs','src/scaler_npc_v2.rs','src/scaler_npc_v2_tests.rs',
        'docs/wpf-reference/scaler.md','tools/qa080_stage_dev_checkpoint.py',
        'tools/qa080_archive_evidence.py']
    for name in files:
        with (ROOT/name).open('rb+') as f:f.flush();os.fsync(f.fileno())
    hashes={name:sha(ROOT/name) for name in files}
    assert hashes[files[0]]==stage['rustTests']['testExeSha256']
    assert hashes[files[1]]==stage['rustTests']['testPdbSha256']
    archive=ROOT/'Backups/qa080-passing-test-image-20261018/exact-test-and-dev-images.zip'
    archive.parent.mkdir(parents=True,exist_ok=True)
    with zipfile.ZipFile(archive,'x',zipfile.ZIP_DEFLATED,compresslevel=1,allowZip64=True) as z:
        for name in files:z.write(ROOT/name,name)
    with archive.open('rb+') as f:f.flush();os.fsync(f.fileno())
    with zipfile.ZipFile(archive) as z:
        assert set(z.namelist())==set(files)
        for name,digest in hashes.items():
            with z.open(name) as f:assert hashlib.file_digest(f,'sha256').hexdigest().upper()==digest,name
    record={'createdUtc':datetime.now(timezone.utc).isoformat(),
        'status':'PASS exact natural passing changed test and staged dev images; original natural AV unresolved',
        'archive':str(archive),'sha256':sha(archive),'bytes':archive.stat().st_size,'roundTripHashesVerified':True,
        'files':hashes,'rustTests':{'passed':1318,'failed':0,'ignored':54,'seconds':42.06,'underDebugger':False,'repeatUntilGreen':False,'exitCode':0,'crashClaimed':False,'dumpProduced':False},
        'historicalNaturalAccessViolationUnresolved':True,
        'compilerFailureEvidence':'Backups/qa078-clang-coordinator-crash-20261018',
        'candidateLiveTested':False,'activated':False,'installed':False,'promoted':False}
    save('artifacts/qa080-test-and-dev-evidence-20261018.json',record)
    starter_text=(ROOT/'artifacts/qa080-starter-check.log').read_text(encoding='utf-8-sig',errors='replace')
    freeze={'createdUtc':datetime.now(timezone.utc).isoformat(),'status':'PASS',
        'sourceFilesVerified':len(actual),'changedSinceQA079':changes,'sourceFreezeMatched':True,
        'stagedExe':stage['executable'],'nativePayload':stage['payload'],'nativeUnchangedSinceQA079':True,
        'wnbaSourceUnchangedSinceQA079':True,'activeDevImageSha256':sha(active),'activeDevImageStillQA079':True,
        'starterChecks':{'powerShellAst':'PASS','hashAndSizeCheck':'PASS','checkOnlyOutput':starter_text,'vbsCheck':'PASS echo-only /check'},
        'starterHashes':{name:hashes[name] for name in ['Start QA080 Dev.ps1','Start QA080 Dev.vbs']},
        'archive':{'path':str(archive.relative_to(ROOT)),'sha256':record['sha256'],'bytes':record['bytes'],'roundTripVerified':True},
        'playerRefereeVisualTimingLiveValidated':False,'npcPlayerClassAdoption':False,'additionalCourtsidePlayerFeatureWithheld':True,
        'diagnosticQualification':'Only fresh quiescent/normalclosed totalzero excludesNPCwritercalls; stale/crashzero not durableproof; successfulreturn not transactionApplied',
        'activated':False,'installed':False,'promoted':False}
    save('artifacts/qa080-final-freeze-verification-20261018.json',freeze)
    print(json.dumps({'archiveBytes':record['bytes'],'archiveSha256':record['sha256'],'roundTripHashesVerified':True,'sourceFilesVerified':len(actual),'exe':stage['executable'],'activeDevStillQA079':True},indent=2))
if __name__=='__main__': main()
