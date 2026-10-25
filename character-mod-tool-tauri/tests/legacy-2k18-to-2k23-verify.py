import hashlib
import json
import sys
import zipfile


def digest(data):
    return hashlib.sha256(data).hexdigest()


def members(path):
    with zipfile.ZipFile(path, "r") as archive:
        return {name: archive.read(name) for name in archive.namelist() if not name.endswith("/")}


target_player, output_player, target_face, output_face = sys.argv[1:]
target = members(target_player)
output = members(output_player)
missing_player_members = sorted(set(target) - set(output))
if missing_player_members:
    raise SystemExit("Converted player dropped NBA 2K23 target members: " + ", ".join(missing_player_members))
added_player_members = sorted(set(output) - set(target))
unexpected_player_members = [
    name for name in added_player_members
    if not name.lower().startswith(("chest_color_o", "legs_color_o", "torso_color_o"))
]
if unexpected_player_members:
    raise SystemExit("Converted player added unexpected members: " + ", ".join(unexpected_player_members))

models = sorted(name for name in target if name.lower().endswith(".model"))
changed_models = []
for name in models:
    if len(target[name]) != len(output[name]):
        raise SystemExit(f"Packed model size changed: {name}")
    if target[name][:32] != output[name][:32]:
        raise SystemExit(f"Packed model header changed: {name}")
    if target[name] != output[name]:
        changed_models.append(name)
if len(changed_models) != 6:
    raise SystemExit(f"Expected all six NBA 2K23 body/head LOD models to change; got {len(changed_models)}")

original_face = members(target_face)
converted_face = members(output_face)
if set(original_face) != set(converted_face):
    raise SystemExit("Converted face archive member layout differs from the NBA 2K23 target")
changed_face = sorted(name for name in original_face if original_face[name] != converted_face[name])
if not any(name.lower().startswith("face_color_o") for name in changed_face):
    raise SystemExit("Face color was not converted")
if not any(name.lower().startswith("face_normal_o") for name in changed_face):
    raise SystemExit("Face normal was not converted")
unexpected = [name for name in changed_face if not name.lower().startswith(("face_color_o", "face_normal_o"))]
if unexpected:
    raise SystemExit("Target-only face payloads changed: " + ", ".join(unexpected))

print(json.dumps({
    "playerMembers": len(output),
    "packedModels": len(models),
    "changedPackedModels": changed_models,
    "addedBodyTextureMembers": added_player_members,
    "faceMembers": len(converted_face),
    "changedFaceMembers": changed_face,
    "playerSha256": digest(open(output_player, "rb").read()),
    "faceSha256": digest(open(output_face, "rb").read()),
}))
