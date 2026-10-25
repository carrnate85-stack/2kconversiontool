import hashlib
import json
import re
import sys
import zipfile
from pathlib import Path

sys.path.insert(0, sys.argv[1])
from hair_tools.hair_picker import item_asset_key, read_hair_document
from hair_tools.hair_lod_policy import hair_models, validate_highest_detail


def inspect(target):
    paths = [target, *sorted(target.parent.glob(target.stem + "_*.iff"))]
    before = {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in paths}
    geometry = {}
    for path in paths[1:]:
        match = re.fullmatch(r"png\d+_geo_(.+?)(?:_tangentspace)?\.iff", path.name, re.I)
        if not match:
            continue
        rows = geometry.setdefault(match.group(1).lower(), [])
        with zipfile.ZipFile(path) as archive:
            for name in archive.namelist():
                if not name.lower().endswith(".scne"):
                    continue
                scene = read_hair_document(archive.read(name))
                models = list(hair_models(scene))
                try:
                    validate_highest_detail(scene)
                    full_detail = True
                except (ValueError, KeyError):
                    full_detail = False
                rows.append({
                    "archive": path.name, "scene": name,
                    "meshes": sorted({prim["Mesh"] for model in models for prim in model["Prim"] if prim.get("Mesh")}),
                    "lodVertices": [[lod.get("LodVerts") for lod in model.get("Lods", [])] for model in models],
                    "fullDetail": full_detail,
                })
    with zipfile.ZipFile(target) as archive:
        appearance_name = next(name for name in archive.namelist() if name.lower().endswith("appearance_info.json"))
        appearance = read_hair_document(archive.read(appearance_name))
        rdat = {}
        for name in archive.namelist():
            if name.lower().endswith("appearance.rdat"):
                try:
                    rdat[name] = read_hair_document(archive.read(name))
                except (ValueError, UnicodeError) as error:
                    rdat[name] = {"parseError": str(error)}
    items = []
    for item in appearance.get("accessory_items", {}).get("items", []):
        key = item_asset_key(str(item.get("name", ""))).lower()
        if key not in geometry:
            continue
        available = {mesh for row in geometry[key] for mesh in row["meshes"]}
        items.append({"item": key, "type": item.get("type"), "mesh": item.get("mesh"), "meshlo": item.get("meshlo"),
                      "primaryPresent": item.get("mesh") in available, "lowPresent": item.get("meshlo") in available,
                      "geometry": geometry[key]})
    assert before == {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in paths}
    return {"player": str(target), "items": items, "rdat": rdat, "allSourceHashesUnchanged": True}


print(json.dumps([inspect(Path(path)) for path in sys.argv[2:]], indent=2))
