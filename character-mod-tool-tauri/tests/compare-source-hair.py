import hashlib
import json
import sys
import zipfile
from pathlib import Path

engine, source, output, destination = map(Path, sys.argv[1:])
sys.path.insert(0, str(engine))
from hair_tools.convert_2k23_hair_to_2k26_lod0 import read_scne, find_segments
from hair_tools.hair_picker import read_hair_document, decode_vcz_buffer, dds_layout

def packed(path):
    with zipfile.ZipFile(path) as archive:
        return {name: archive.read(name) for name in archive.namelist()}

with zipfile.ZipFile(source / "png6720_hair_spiral_dreads.iff") as archive:
    scene, _ = read_scne(archive, next(n for n in archive.namelist() if n.lower().endswith(".scne")))
    root = next(iter(scene.values()))
    candidates = [(m, find_segments(archive.read(m["Binary"]))) for m in root["Model"].values() if isinstance(m, dict) and "Binary" in m and m.get("Prim")]
    model, segments = max(candidates, key=lambda row: int(next(iter(row[0]["Prim"].values()))["Count"]))
    primitive = next(iter(model["Prim"].values()))
    count = int(primitive["Count"])
    indices = segments[0][2][:count * 2]

native = packed(output / "png6720_geo_hair_dreads.iff")
scene = read_hair_document(next(v for n, v in native.items() if n.lower().endswith(".scne")))
target_model = next(iter(next(iter(scene.values()))["Model"].values()))
def buffer(desc):
    name = desc["Binary"]
    raw = next(native[n] for n in [name, name.replace(".gz", ".bin"), name.replace(".bin", ".gz")] if n in native)
    return decode_vcz_buffer(raw, int(desc["Size"]))

streams = target_model["VertexStream"]
vertices = max(int.from_bytes(indices[i:i+2], "little") for i in range(0, len(indices), 2)) + 1
stream = buffer(streams[1])
uv = {}
for number, segment in [(0, 4), (1, 5)]:
    key = "TEXCOORD" + str(number)
    desc = target_model["VertexFormat"][key]
    encoded = b"".join(stream[i * streams[1]["Stride"] + desc["ByteOffset"]:i * streams[1]["Stride"] + desc["ByteOffset"] + 4] for i in range(vertices))
    uv[key] = {
        "bytesMatch": encoded == segments[segment][2][:vertices * 4],
        "offsetMatch": desc.get("Offset") == model["VertexFormat"][key].get("Offset"),
        "scaleMatch": desc.get("Scale") == model["VertexFormat"][key].get("Scale")
    }
start = int(target_model["Prim"][0]["Start"])
native_indices = buffer(target_model["IndexBuffer"])[start * 2:(start + count) * 2]
source_item = packed(source / "face6720_item_hair_spiral_dreads.iff")
target_item = packed(output / "png6720_item_hair_dreads.iff")
textures = []
for logical in ["hair_color_o", "hair_tangent_o", "hair_lightmap_o"]:
    pick = lambda data, suffix: next((n, v) for n, v in data.items() if n.lower().startswith(logical) and n.lower().endswith(suffix))
    sn, sd = pick(source_item, ".dds")
    tn, td = pick(target_item, ".dds")
    sl, tl = dds_layout(sd), dds_layout(td)
    sm = read_hair_document(pick(source_item, ".txtr")[1])[logical]
    tm = read_hair_document(pick(target_item, ".txtr")[1])[logical]
    textures.append({"logical": logical, "source": sn, "output": tn,
        "pixelsMatch": sd[sl["header_size"]:sl["header_size"] + tl["payload_size"]] == td[tl["header_size"]:tl["header_size"] + tl["payload_size"]],
        "layoutMatch": all(sl[k] == tl[k] for k in ["width", "height", "format", "mips"]),
        "rangeMatch": all(sm.get(k) == tm.get(k) for k in ["Min", "Max"])})
report = {"vertices": vertices, "indicesMatch": indices == native_indices, "uv": uv, "textures": textures}
destination.write_text(json.dumps(report, indent=2), encoding="utf-8")
print(json.dumps(report, indent=2))
if not report["indicesMatch"] or not all(all(row.values()) for row in uv.values()) or not all(all(row[k] for k in ["pixelsMatch", "layoutMatch", "rangeMatch"]) for row in textures):
    raise SystemExit("Export/source hair mismatch")
