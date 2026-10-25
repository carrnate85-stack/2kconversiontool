import dataclasses
import hashlib
import json
import sys
import zipfile
from pathlib import Path

engine, target, destination = map(Path, sys.argv[1:])
sys.path.insert(0, str(engine))
from character_mod_tool import validate_archive_snapshot, try_parse_structured_text
from hair_tools.hair_lod_policy import validate_highest_detail
from hair_tools.hair_picker import validate_hair_scene_buffers

results = []
for path in sorted(target.parent.glob(target.stem + "*.iff")):
    before = hashlib.sha256(path.read_bytes()).hexdigest()
    with zipfile.ZipFile(path) as archive:
        corrupt = archive.testzip()
        if corrupt:
            raise ValueError(f"CRC failure: {path.name}:{corrupt}")
        names = archive.namelist()
        payloads = {name: archive.read(name) for name in names}
    validation = validate_archive_snapshot(names, payloads, allow_shared_textures=True)
    lod = []
    for name, content in payloads.items():
        if name.lower().endswith(".scne"):
            scene, _, error = try_parse_structured_text(name, content)
            if error or not scene:
                continue
            if "hair" in path.name.lower():
                try:
                    checked = validate_hair_scene_buffers(scene, payloads)
                    lod.append({"name": name, "models": checked, "valid": True})
                except (KeyError, ValueError) as error:
                    lod.append({"name": name, "valid": False, "error": str(error)})
    if hashlib.sha256(path.read_bytes()).hexdigest() != before:
        raise ValueError("Read-only audit changed an export")
    results.append({"archive": path.name, "sha256": before, "crcValid": True,
                    "checks": [dataclasses.asdict(row) for row in validation], "lod": lod})
destination.write_text(json.dumps(results, indent=2), encoding="utf-8")
print(json.dumps({"archives": len(results), "errors": sum(row["severity"] == "ERROR" for result in results for row in result["checks"]),
                  "warnings": [row for result in results for row in result["checks"] if row["severity"] == "WARNING"],
                  "lod": [row for result in results for row in result["lod"]]}, indent=2))
