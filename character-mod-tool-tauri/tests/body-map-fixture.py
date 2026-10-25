import hashlib
import json
import sys
import zipfile
from pathlib import Path


source, destination = map(Path, sys.argv[1:3])
seed = int(sys.argv[3])
with zipfile.ZipFile(source) as archive:
    members = [(info, archive.read(info)) for info in archive.infolist()]

textures = []
with zipfile.ZipFile(destination, "w", compression=zipfile.ZIP_DEFLATED) as archive:
    for info, data in members:
        if info.filename.lower().endswith(".dds"):
            assert data.startswith(b"DDS ") and len(data) > 160
            changed = bytearray(data)
            changed[-1] ^= seed
            data = bytes(changed)
            textures.append({
                "name": info.filename,
                "sha256": hashlib.sha256(data).hexdigest(),
            })
        archive.writestr(info, data)
assert textures, f"No DDS payloads in {source.name}"
print(json.dumps({"textures": textures}))
