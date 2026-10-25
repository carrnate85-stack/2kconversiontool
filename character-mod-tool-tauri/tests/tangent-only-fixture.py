import json
import shutil
import sys
import zipfile
from pathlib import Path

source, destination = map(Path, sys.argv[1:])
assert destination.name.startswith("tangent-only-") and destination.parent.name == "artifacts"
assert destination.is_dir() and not destination.is_symlink()
for path in source.parent.glob("*.iff"):
    if path.name.lower().endswith("_geo_hair_curly_afro.iff"):
        continue
    shutil.copy2(path, destination / path.name)
target = destination / source.name
with zipfile.ZipFile(target) as archive:
    members = [(info, archive.read(info)) for info in archive.infolist()]
with zipfile.ZipFile(target, "w") as archive:
    for info, data in members:
        if info.filename.lower() == "appearance_info.json":
            appearance = json.loads(data)
            for item in appearance["accessory_items"]["items"]:
                if item["name"].lower() == "hair_curly_afro":
                    item["tangentspace"] = "yes"
            data = json.dumps(appearance).encode()
        archive.writestr(info, data)
print(target)
