import json
import struct
import zipfile
from pathlib import Path

import bpy


def game_to_blender(position):
    x, y, z = position
    return x, -z, y


def blender_to_game(position):
    return position.x, position.z, -position.y


def scene_model(archive):
    scne_name = next((name for name in archive.namelist() if name.lower().endswith(".scne")), None)
    if not scne_name:
        raise RuntimeError("IFF contains no SCNE file.")
    text = archive.read(scne_name).decode("utf-8-sig")
    scene = json.loads(text if text.lstrip().startswith("{") else "{" + text + "}")
    root_name, root = next(iter(scene.items()))
    models = root.get("Model", {})
    if not models:
        raise RuntimeError(f"{scne_name} contains no model.")
    model_name, model = next(iter(models.items()))
    return scne_name, root_name, model_name, model


def archive_buffer_name(archive, binary_name):
    candidates = {binary_name.lower()}
    if binary_name.lower().endswith(".gz"):
        candidates.add((binary_name[:-3] + ".bin").lower())
    member = next((name for name in archive.namelist() if name.lower() in candidates), None)
    if not member:
        raise RuntimeError(f"IFF buffer is missing: {binary_name}")
    return member


def read_positions(archive, model):
    position = model.get("VertexFormat", {}).get("POSITION0", {})
    if position.get("Format") != "R32G32B32_FLOAT":
        raise RuntimeError(f"Unsupported position format: {position.get('Format')}")
    stream_index = int(position.get("Stream", 0))
    streams = model.get("VertexStream", [])
    if stream_index >= len(streams) or not streams[stream_index]:
        raise RuntimeError("Position vertex stream is missing.")
    stream = streams[stream_index]
    stride = int(stream.get("Stride", 0))
    offset = int(position.get("ByteOffset", 0))
    size = int(stream.get("Size", 0))
    if stride < offset + 12 or size <= 0 or size % stride:
        raise RuntimeError("Position vertex stream layout is invalid.")
    member = archive_buffer_name(archive, str(stream.get("Binary", "")))
    data = archive.read(member)
    count = size // stride
    if len(data) < count * stride:
        raise RuntimeError("Position vertex buffer is truncated.")
    return [
        game_to_blender(struct.unpack_from("<fff", data, index * stride + offset))
        for index in range(count)
    ]


def read_indices(archive, model):
    descriptor = model.get("IndexBuffer", {})
    format_name = descriptor.get("Format")
    formats = {"R16_UINT": ("H", 2), "R32_UINT": ("I", 4)}
    if format_name not in formats:
        raise RuntimeError(f"Unsupported index format: {format_name}")
    code, width = formats[format_name]
    size = int(descriptor.get("Size", 0))
    member = archive_buffer_name(archive, str(descriptor.get("Binary", "")))
    data = archive.read(member)
    if size <= 0 or size % width or len(data) < size:
        raise RuntimeError("Index buffer layout is invalid.")
    return struct.unpack_from(f"<{size // width}{code}", data)


def lod0_faces(model, indices, vertex_count):
    faces = []
    for primitive in model.get("Prim", []):
        if str(primitive.get("Type", "TRIANGLE_LIST")).upper() != "TRIANGLE_LIST":
            continue
        lods = primitive.get("LodList") or []
        source = lods[0] if lods else primitive
        start = int(source.get("Start", primitive.get("Start", 0)))
        count = int(source.get("Count", primitive.get("Count", 0)))
        end = min(start + count, len(indices))
        end -= (end - start) % 3
        for index in range(start, end, 3):
            face = (indices[index], indices[index + 1], indices[index + 2])
            if max(face) < vertex_count and len(set(face)) == 3:
                faces.append(face)
    if not faces:
        raise RuntimeError("SCNE contains no usable LOD0 triangles.")
    return faces


def import_iff_mesh(path, object_name=None):
    path = Path(path)
    with zipfile.ZipFile(path, "r") as archive:
        scne_name, root_name, model_name, model = scene_model(archive)
        vertices = read_positions(archive, model)
        indices = read_indices(archive, model)
        faces = lod0_faces(model, indices, len(vertices))

    name = object_name or root_name or path.stem
    mesh = bpy.data.meshes.new(f"{name}_mesh")
    mesh.from_pydata(vertices, [], faces)
    mesh.update(calc_edges=False)
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    obj["nba2k_safe_hair"] = False
    obj["nba2k_source_iff"] = str(path)
    obj["nba2k_source_scne"] = scne_name
    obj["nba2k_source_model"] = model_name
    print(
        f"[Hair Picker IFF] Imported {path.name}: "
        f"{len(vertices):,} vertices, {len(faces):,} LOD0 triangles"
    )
    return [obj]
