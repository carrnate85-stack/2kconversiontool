"""Recheck retained conversion fixtures with the duplicate-preserving comparator."""
import json
import subprocess
import sys
from pathlib import Path

root = Path(__file__).resolve().parents[1]
artifacts = root / "artifacts"
fixtures = []
for name in ("wpf-conversion-NxikuL", "accessory-conversion-khO4uS", "legacy-hair-parity-VwMbZM"):
    for folder in sorted((artifacts / name).iterdir()):
        if (folder / "wpf").is_dir() and (folder / "tauri").is_dir():
            fixtures.append(folder)
assert len(fixtures) == 10, "Expected ten retained full-conversion comparisons"
fixtures.append(artifacts / "wpf-config-TTvLtm")
report = {"completed": False, "duplicatePropertiesPreserved": True, "cases": []}
try:
    for folder in fixtures:
        command = [sys.executable, "-B", str(root / "tests/wpf-output-compare.py"),
                   str(folder / "wpf"), str(folder / "tauri")]
        if (folder / "asset-ids.json").is_file():
            command.append(str(folder / "asset-ids.json"))
        result = subprocess.run(command, capture_output=True, text=True, check=True, timeout=30)
        comparison = json.loads(result.stdout)
        report["cases"].append({"fixture": str(folder.relative_to(artifacts)), **comparison})
        assert not any(comparison[key] for key in ("missing", "extra", "different")), comparison
        print(folder.name, comparison["archives"], comparison["entries"], "match", flush=True)
    report["completed"] = True
finally:
    (artifacts / "duplicate-preserving-parity-report.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
