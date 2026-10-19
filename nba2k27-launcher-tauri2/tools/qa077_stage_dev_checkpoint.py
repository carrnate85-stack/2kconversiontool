"""Create a new dev-only candidate; never activate, install or launch it."""
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
PAYLOAD = "1E3A0E67030D0D2421C75797659AC7E6F390115921228F14A18CC84D0B5E0EB1"


def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest().upper()


def durable_new(path, content):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("xb") as stream:
        stream.write(content)
        stream.flush()
        os.fsync(stream.fileno())


def main():
    source = ROOT / "target-qa054/debug/nba2k27-launcher-tauri2.exe"
    payload = ROOT / "assets/native/version-outfits-v2.dll"
    assert sha(payload) == PAYLOAD and payload.stat().st_size == 1194496
    log = ROOT / "artifacts/qa077-final-full-rust-tests.log"
    capture = ROOT / "artifacts/qa074-natural-qa077-whole-1.txt"
    text = log.read_text(encoding="utf-8-sig", errors="replace")
    result = re.findall(r"test result: ok\. (\d+) passed; 0 failed; (\d+) ignored", text)
    assert result and result[-1] == ("1302", "54"), "Changed whole suite did not pass"
    capture_text = capture.read_text(encoding="utf-8-sig", errors="replace")
    assert re.search(r"EXIT_CODE=0(?:\s|$)", capture_text), "Child exit was not zero"
    assert re.search(r"CLAIMED=0(?:\s|$)", capture_text), "Fatal exception was claimed"
    audit = json.loads((ROOT / "artifacts/qa075-outfit-readonly-shadow-source-audit-20261018.json").read_text())
    for name, pin in audit["files"].items():
        assert sha(ROOT / "src" / name) == pin["sha256"].upper(), f"Source changed: {name}"
    stage = ROOT / "Dev/Pending/QA077/launcher.exe"
    outputs = [stage, ROOT / "Start QA077 Dev.ps1", ROOT / "Start QA077 Dev.vbs",
               ROOT / "artifacts/qa077-final-stage-20261018.json"]
    assert not any(path.exists() for path in outputs), "Existing stages are never overwritten"
    sources = []
    for folder in ["src", "frontend", "native", "browser-overrides"]:
        for path in sorted((ROOT / folder).rglob("*")):
            if path.is_file() and path.suffix in {".rs", ".tsx", ".ts", ".js", ".mjs", ".cpp", ".hpp", ".h", ".json"}:
                content = path.read_bytes()
                assert content and b"\0" not in content, f"Empty or NUL source: {path}"
                sources.append({"path": path.relative_to(ROOT).as_posix(), "sha256": hashlib.sha256(content).hexdigest().upper(), "bytes": len(content)})
    digest, size = sha(source), source.stat().st_size
    ps = (ROOT / "Start QA073 Dev.ps1").read_text(encoding="utf-8-sig")
    ps = ps.replace("QA073", "QA077").replace("qa073", "qa077")
    ps = ps.replace("B322735EC913237B1F0A8EEDEB52A0D33C6E5150B90ADCCA43C49DD87BA55048", digest).replace("52860928", str(size))
    start = "    Start-Process -FilePath $taskDestination -WorkingDirectory (Split-Path $taskDestination -Parent) | Out-Null"
    assert ps.count(start) == 1
    ps = ps.replace(start, "    # Dev-only read-only timing observer; the normal provider child inherits this.\n"
                    "    $taskPreviousShadow = [Environment]::GetEnvironmentVariable('C27_OUTFIT_READY_SHADOW','Process')\n"
                    "    try {\n"
                    "        $env:C27_OUTFIT_READY_SHADOW = '1'\n" + start + "\n"
                    "    } finally { [Environment]::SetEnvironmentVariable('C27_OUTFIT_READY_SHADOW',$taskPreviousShadow,'Process') }")
    assert ps.index("if ($CheckOnly)") < ps.index("$env:C27_OUTFIT_READY_SHADOW = '1'")
    durable_new(stage, source.read_bytes())
    assert sha(stage) == digest and stage.stat().st_size == size
    durable_new(ROOT / "Start QA077 Dev.ps1", ps.encode("utf-8-sig"))
    durable_new(ROOT / "Start QA077 Dev.vbs", (ROOT / "Start QA073 Dev.vbs").read_bytes().replace(b"QA073", b"QA077"))
    record = {
        "schema": "qa077-dev-stage-v1", "createdUtc": datetime.now(timezone.utc).isoformat(),
        "devOnly": True, "activated": False, "installed": False, "promoted": False,
        "candidateOnly": True, "releaseEligible": False,
        "executable": {"path": str(stage), "sha256": digest, "bytes": size},
        "payload": {"sha256": PAYLOAD, "bytes": payload.stat().st_size},
        "readOnlyShadowOptIn": True, "shadowLiveValidated": False,
        "outfitAdmissionChanged": False, "wnbaResearchAssetIntegrated": False,
        "pairedTriggerSubmission": True, "wnbaDeepCaptureDiagnostics": True,
        "bridgeFailureReasonPreserved": True,
        "randomAllStreetwearOnly": True, "randomAllRoutinePopups": False,
        "embeddedAssets": [{"path": name, "sha256": sha(ROOT / name), "bytes": (ROOT / name).stat().st_size}
                           for name in ["assets/wpf-browser.json.gz", "assets/wpf-browser-manifest.json", "assets/choices.json.gz", "assets/appearance-v2-proof.json", "assets/startup-readiness-v2-fixture.bin", "assets/scoreboard-v2-context-lifecycle.bin"]],
        "rustTests": {"passed": 1302, "ignored": 54, "underDebugger": False,
                      "boundedNaturalRun": True, "log": str(log.relative_to(ROOT)), "capture": str(capture.relative_to(ROOT)),
                      "intermittentAccessViolationUnresolved": True,
                      "historicalNaturalFailure": "artifacts/qa073-natural-test-av-identity.json",
                      "qualification": "Passing changed test image does not explain or resolve the earlier natural access violation."},
        "sourceFiles": sources,
    }
    durable_new(ROOT / "artifacts/qa077-final-stage-20261018.json", (json.dumps(record, indent=2) + "\n").encode())
    print(json.dumps({k: v for k, v in record.items() if k != "sourceFiles"}, indent=2))


if __name__ == "__main__":
    main()
