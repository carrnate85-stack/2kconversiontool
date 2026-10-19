"""Agent-only offline staging. Never activates Dev/App or installs a build."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
from datetime import datetime, timezone

ROOT = Path(__file__).resolve().parents[1]
PAYLOAD = "81952F0CF8D13226AF8F664A226E617B21F2EC278E6F7BF94844246E8D80CB04"


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
    assert sha(payload) == PAYLOAD and payload.stat().st_size == 1137152
    full_log = (ROOT / "artifacts/qa068-final-full-rust-tests.log").read_text(encoding="utf-8-sig", errors="replace")
    result = re.findall(r"test result: ok\. (\d+) passed; 0 failed; (\d+) ignored", full_log)
    assert result and int(result[-1][0]) > 1200, "Final whole Rust suite has not passed"
    stage = ROOT / "Dev/Pending/QA068/launcher.exe"
    assert not stage.exists(), "An existing stage is never overwritten"
    for leaf in ["Start QA068 Dev.ps1", "Start QA068 Dev.vbs"]:
        assert not (ROOT / leaf).exists(), "An existing starter is never overwritten"
    sources = []
    for folder in ["src", "frontend", "native", "browser-overrides"]:
        for path in sorted((ROOT / folder).rglob("*")):
            if path.is_file() and path.suffix in {".rs", ".tsx", ".ts", ".js", ".mjs", ".cpp", ".hpp", ".h", ".json"}:
                content = path.read_bytes()
                assert content and b"\0" not in content, f"Empty or NUL source: {path}"
                sources.append({"path":path.relative_to(ROOT).as_posix(),"sha256":hashlib.sha256(content).hexdigest().upper(),"bytes":len(content)})
    digest, size = sha(source), source.stat().st_size
    durable_new(stage, source.read_bytes())
    assert sha(stage) == digest and stage.stat().st_size == size
    ps = (ROOT / "Start QA065 Dev.ps1").read_text(encoding="utf-8-sig")
    ps = ps.replace("QA065", "QA068").replace("qa065", "qa068")
    ps = ps.replace("AA264F1AD5A630BBCDF059888E1FCE3F1FB2934E0722122B9EFD5107E8792740", digest).replace("52292608", str(size))
    old = "    Copy-Item -LiteralPath $taskSource -Destination $taskDestination"
    assert old in ps
    ps = ps.replace(old, old + "\n    $taskFlush = [IO.File]::Open($taskDestination,[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::Read)\n    try { $taskFlush.Flush($true) } finally { $taskFlush.Dispose() }")
    durable_new(ROOT / "Start QA068 Dev.ps1", ps.encode("utf-8-sig"))
    vbs = (ROOT / "Start QA065 Dev.vbs").read_bytes().replace(b"QA065", b"QA068")
    durable_new(ROOT / "Start QA068 Dev.vbs", vbs)
    record = {"schema":"qa068-dev-stage-v1","createdUtc":datetime.now(timezone.utc).isoformat(),
              "devOnly":True,"activated":False,"installed":False,"promoted":False,
              "executable":{"path":str(stage),"sha256":digest,"bytes":size},
              "payload":{"sha256":PAYLOAD,"bytes":payload.stat().st_size},
              "embeddedAssets":[{"path":name,"sha256":sha(ROOT/name),"bytes":(ROOT/name).stat().st_size}
                  for name in ["assets/wpf-browser.json.gz","assets/wpf-browser-manifest.json","assets/choices.json.gz","assets/appearance-v2-proof.json","assets/startup-readiness-v2-fixture.bin","assets/scoreboard-v2-context-lifecycle.bin"]],
              "rustTests":{"passed":int(result[-1][0]),"ignored":int(result[-1][1])},
              "sourceFiles":sources}
    durable_new(ROOT / "artifacts/qa068-final-stage-20261018.json", (json.dumps(record,indent=2)+"\n").encode())
    print(json.dumps({k:v for k,v in record.items() if k != "sourceFiles"},indent=2))


if __name__ == "__main__":
    main()
