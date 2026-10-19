"""Stage an exact tested dev candidate. Never activate, install, or launch it."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]


def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest().upper()


def durable_new(path, content):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("xb") as stream:
        stream.write(content)
        stream.flush()
        os.fsync(stream.fileno())


def source_files():
    entries = []
    for folder in ["src", "frontend", "native", "browser-overrides", "test"]:
        for path in sorted((ROOT / folder).rglob("*")):
            if folder == "test" and path.suffix not in {".cpp", ".hpp", ".h", ".rs", ".ts", ".js"}:
                continue
            if path.is_file() and path.suffix in {".rs", ".tsx", ".ts", ".js", ".mjs", ".cpp", ".hpp", ".h", ".json"}:
                data = path.read_bytes()
                assert data and b"\0" not in data, f"Empty or NUL source: {path}"
                entries.append({"path": path.relative_to(ROOT).as_posix(), "sha256": sha(path), "bytes": len(data)})
    for name in ["Cargo.toml", "Cargo.lock", "build.rs", "tauri.conf.json"]:
        path = ROOT / name
        entries.append({"path": name, "sha256": sha(path), "bytes": path.stat().st_size})
    return entries


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--payload-sha", required=True)
    parser.add_argument("--payload-bytes", required=True, type=int)
    parser.add_argument("--build-inputs", required=True)
    args = parser.parse_args()
    assert re.fullmatch(r"[A-F0-9]{64}", args.payload_sha)
    source = ROOT / "target-qa054/debug/nba2k27-launcher-tauri2.exe"
    payload = ROOT / "assets/native/version-outfits-v2.dll"
    assert sha(payload) == args.payload_sha and payload.stat().st_size == args.payload_bytes
    for name in ["src/outfit_worker_v2_loader.rs", "src/provider_outfit_profile.rs"]:
        assert args.payload_sha in (ROOT / name).read_text()
    log = ROOT / "artifacts/qa083-final-full-rust-tests.log"
    capture = ROOT / "artifacts/qa074-natural-qa083-roster-final-2.txt"
    capture_rest = ROOT / "artifacts/qa074-natural-qa083-rest-final-2.txt"
    capture_catalog = ROOT / "artifacts/qa074-natural-qa083-catalog-final-2.txt"
    results = re.findall(r"test result: ok\. (\d+) passed; 0 failed; (\d+) ignored", log.read_text(encoding="utf-8-sig", errors="replace"))
    assert results and int(results[-1][0]) >= 1316 and results[-1][1] == "54"
    capture_text = capture.read_text(encoding="utf-8-sig", errors="replace")
    capture_rest_text = capture_rest.read_text(encoding="utf-8-sig", errors="replace")
    capture_catalog_text = capture_catalog.read_text(encoding="utf-8-sig", errors="replace")
    assert re.search(r"EXIT_CODE=0(?:\s|$)", capture_catalog_text)
    assert re.search(r"CLAIMED=0(?:\s|$)", capture_catalog_text)
    assert re.search(r"EXIT_CODE=0(?:\s|$)", capture_rest_text)
    assert re.search(r"CLAIMED=0(?:\s|$)", capture_rest_text)
    assert re.search(r"EXIT_CODE=0(?:\s|$)", capture_text)
    assert re.search(r"CLAIMED=0(?:\s|$)", capture_text)
    test_exe = ROOT / "target-qa054/debug/deps/nba2k27_launcher_tauri2-281ce2d259e8f3fd.exe"
    test_pdb = test_exe.with_suffix(".pdb")
    assert "sha256=" + sha(test_exe).lower() in capture_text
    assert "sha256=" + sha(test_pdb).lower() in capture_text
    assert "sha256=" + sha(test_exe).lower() in capture_rest_text
    assert "sha256=" + sha(test_pdb).lower() in capture_rest_text
    assert "sha256=" + sha(test_exe).lower() in capture_catalog_text
    assert "sha256=" + sha(test_pdb).lower() in capture_catalog_text
    coverage = json.loads((ROOT / "artifacts/qa083-final-test-coverage.json").read_text())
    assert coverage["completeExactPartition"] is True
    assert coverage["testListSha256"] == sha(ROOT / "artifacts/qa083-final-test-list.log")
    for part in coverage["parts"]:
        assert part["logSha256"] == sha(ROOT / part["log"])
        assert part["captureSha256"] == sha(ROOT / part["capture"])
    assert coverage["testExeSha256"] == sha(test_exe) and coverage["testPdbSha256"] == sha(test_pdb)
    assert coverage["passed"] == int(results[-1][0]) and coverage["ignored"] == int(results[-1][1])
    inputs = json.loads((ROOT / args.build_inputs).read_text())
    for item in inputs["sourceFiles"]:
        assert sha(ROOT / item["path"]) == item["sha256"], f"Source changed after build input freeze: {item['path']}"
    for item in inputs["buildScriptsAndRegressionSources"] + inputs["embeddedAssets"]:
        assert sha(ROOT / item["path"]) == item["sha256"] and (ROOT / item["path"]).stat().st_size == item["bytes"], f"Build script or embedded asset changed after freeze: {item['path']}"
    entries = source_files()
    assert {e["path"] for e in entries} == {e["path"] for e in inputs["sourceFiles"]}
    stage = ROOT / "Dev/Pending/QA083/launcher.exe"
    outputs = [stage, ROOT / "Start QA083 Dev.ps1", ROOT / "Start QA083 Dev.vbs", ROOT / "artifacts/qa083-final-stage-20261019.json"]
    assert not any(p.exists() for p in outputs), "Prior stages are never overwritten"
    digest, size = sha(source), source.stat().st_size
    ps = (ROOT / "Start QA080 Dev.ps1").read_text(encoding="utf-8-sig")
    ps = ps.replace("QA080", "QA083").replace("qa080", "qa083")
    ps = ps.replace("783B64372BFB73621AE3D6083EB2460D0AA42A140DEBFF27365315EDC330E7EC", digest).replace("53165568", str(size))
    assert ps.count(digest) == 1 and "if ($CheckOnly)" in ps
    durable_new(stage, source.read_bytes())
    assert sha(stage) == digest and stage.stat().st_size == size
    durable_new(ROOT / "Start QA083 Dev.ps1", ps.encode("utf-8-sig"))
    durable_new(ROOT / "Start QA083 Dev.vbs", (ROOT / "Start QA080 Dev.vbs").read_bytes().replace(b"QA080", b"QA083"))
    record = {
        "schema": "qa083-dev-stage-v1", "createdUtc": datetime.now(timezone.utc).isoformat(),
        "devOnly": True, "activated": False, "installed": False, "promoted": False,
        "candidateOnly": True, "releaseEligible": False, "wnbaLiveValidated": False,
        "executable": {"path": str(stage), "sha256": digest, "bytes": size},
        "payload": {"sha256": args.payload_sha, "bytes": args.payload_bytes},
        "buildInputs": args.build_inputs,
        "wnbaBlacktopTypedPreparationWait": True,
        "wnbaBlacktopBoundedBulkReads": True,
        "wnbaFreestyleCompactPublicationGuards": True,
        "wnbaFreestylePreEffectWaiting": True,
        "contextNormalization": False, "wnbaResearchAssetIntegrated": False,
        "randomAllStreetwearOnly": True, "randomAllRoutinePopups": False,
        "initialScalerBeforeCamera": True, "scalerAdmissionChanged": False,
        "scalerFirstDueImmediatelyAfterArmed": True, "scalerTimingTelemetry": True,
        "wnbaChangedSinceQA079": False,
        "npcPlayerClassAdoption": False, "courtsideCelebrityOptInAvailable": False,
        "fullNameAudioScalar": True, "customCatalogImport": True, "savedRatingsTextImport": True, "onlineRatingsCapture": False, "explicitCareerYearPicker": True, "nativeTeamTypeYearSave": True, "existingArenaSelection": True, "existingOriginSelection": True, "firstLastAudioPreviewMapping": True, "structuralSeasonPureModelOnly": True, "editorCorrections": True, "teamDnaFileTransfer": True, "appearancePolicyOwnershipFixedButUnwired": True,
        "scalerNpcEffectDiagnostics": True, "visibleScalerTimingLiveValidated": False,
        "readOnlyShadowOptIn": True, "outfitAdmissionChanged": False,
        "embeddedAssets": [dict(item) for item in inputs["embeddedAssets"]],
        "rustTests": {"passed": int(results[-1][0]), "ignored": int(results[-1][1]), "underDebugger": False,
                      "boundedNaturalRun": True, "log": str(log.relative_to(ROOT)), "capture": [str(capture.relative_to(ROOT)), str(capture_catalog.relative_to(ROOT)), str(capture_rest.relative_to(ROOT))], "coverage": "artifacts/qa083-final-test-coverage.json",
                      "testExeSha256": sha(test_exe), "testPdbSha256": sha(test_pdb),
                      "intermittentAccessViolationUnresolved": True,
                      "historicalNaturalFailure": "artifacts/qa073-natural-test-av-identity.json",
                      "qualification": "A passing changed image does not explain or resolve the earlier natural access violation."},
        "sourceFiles": entries,
    }
    durable_new(outputs[-1], (json.dumps(record, indent=2) + "\n").encode())
    print(json.dumps({k: v for k, v in record.items() if k != "sourceFiles"}, indent=2))


if __name__ == "__main__":
    main()
