"""Verify the deliberate highest-source-LOD policy independently of the bridge."""
import json
import struct
import sys
import zipfile
from pathlib import Path

engine, source, actual, template, expected, report_path = map(Path, sys.argv[1:])
sys.path.insert(0, str(engine))
from hair_tools.convert_2k23_hair_to_2k26_lod0 import convert as convert_legacy_hair, find_segments, read_scne
from hair_tools.hair_picker import force_highest_hair_archive, read_hair_document, decode_vcz_buffer

with zipfile.ZipFile(source) as archive:
    scene, _ = read_scne(archive, next(name for name in archive.namelist() if name.lower().endswith(".scne")))
    candidates = []
    for name, model in next(iter(scene.values()))["Model"].items():
        if not isinstance(model, dict) or not model.get("Binary") or not model.get("Prim"):
            continue
        segments = find_segments(archive.read(model["Binary"]))
        if len(segments) < 8:
            continue
        count = int(next(iter(model["Prim"].values()))["Count"])
        indices = segments[0][2][:count * 2]
        vertices = max(struct.unpack("<" + "H" * count, indices)) + 1
        candidates.append((vertices, name, model, segments, indices))
vertices, name, source_model, segments, indices = max(candidates, key=lambda row: row[0])

with zipfile.ZipFile(actual) as archive:
    members = {name: archive.read(name) for name in archive.namelist()}
scene = read_hair_document(next(data for name, data in members.items() if name.lower().endswith(".scne")))
root_name, root = next(iter(scene.items()))
model = root["Model"]["hihead"]

def buffer(descriptor):
    binary = descriptor["Binary"]
    value = next(members[name] for name in (binary, binary.replace(".gz", ".bin")) if name in members)
    return decode_vcz_buffer(value, int(descriptor["Size"]))

assert all(int(lod["LodVerts"]) == vertices for lod in model["Lods"])
assert buffer(model["IndexBuffer"]) == indices
assert all(int(lod["Start"]) == 0 and int(lod["Count"]) == len(indices) // 2
           for primitive in model["Prim"] for lod in primitive["LodList"])
streams = model["VertexStream"]
assert int(streams[0]["Size"]) // int(streams[0]["Stride"]) == vertices
assert model["VertexFormat"]["POSITION0"]["Format"] == "R32G32B32_FLOAT"
position_bytes = buffer(streams[0])
position_format = source_model["VertexFormat"]["POSITION0"]
maximum_error = 0.0
for vertex in range(vertices):
    packed = struct.unpack_from("<4h", segments[1][2], vertex * 8)
    decoded = struct.unpack_from("<3f", position_bytes, vertex * 12)
    for axis in range(3):
        wanted = position_format["Offset"][axis] + max(-1.0, packed[axis] / 32767.0) * position_format["Scale"][axis]
        maximum_error = max(maximum_error, abs(decoded[axis] - wanted))
assert maximum_error < 0.00001, maximum_error
attributes = buffer(streams[1])
for channel, segment in (("TEXCOORD0", 4), ("TEXCOORD1", 5)):
    descriptor = model["VertexFormat"][channel]
    encoded = b"".join(attributes[vertex * int(streams[1]["Stride"]) + int(descriptor["ByteOffset"]):
                                 vertex * int(streams[1]["Stride"]) + int(descriptor["ByteOffset"]) + 4]
                       for vertex in range(vertices))
    assert encoded == segments[segment][2][:vertices * 4]
    assert all(descriptor.get(field) == source_model["VertexFormat"][channel].get(field) for field in ("Offset", "Scale"))

convert_legacy_hair(source, expected, force_bone=48, target_name=root_name,
                    target_scne_name=root_name + ".SCNE", duplicate_lods=False,
                    template_path=template, preserve_template_vertex_metadata=True,
                    fit_template_vertex_count=True, use_source_uv_metadata=True, prefer_highest_lod=True)
force_highest_hair_archive(expected)
report = {"sourceLods": [{"name": row[1], "vertices": row[0]} for row in candidates],
          "selected": name, "vertices": vertices, "indexCount": len(indices) // 2,
          "indicesMatchSource": True, "uvBytesAndMetadataMatchSource": True,
          "maximumPositionError": maximum_error, "allDistancesUseHighestLod": True}
report_path.write_text(json.dumps(report, indent=2), encoding="utf-8")
print(json.dumps(report))
