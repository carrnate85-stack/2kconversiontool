import os
import re
import struct
import zlib

from .Blender.blender_mesh import loadSkinMesh
from .Blender.blender_scene import getNewSceneCollection, splitAllMaterialGroups


class _LegacyMaterialGroup:
    def __init__(self, material, face_begin, face_end):
        self.material = material
        self.face_begin = face_begin
        self.face_end = face_end


class _LegacySkinMesh:
    def __init__(self, name, vertices, triangles, texcoords, prim_groups, material_groups):
        self.name = name
        self.mesh_name = name
        self.vertex_components = 3
        self.vertices = vertices
        self.vertex_normals = None
        self.index_list = triangles
        self.texcoords = texcoords
        self.material_groups = material_groups


def _read_text(path):
    with open(path, "r", encoding="utf-8", errors="ignore") as handle:
        return handle.read()


def _match_brace(text, open_index):
    depth = 0
    in_string = False
    escaped = False
    for index in range(open_index, len(text)):
        char = text[index]
        if in_string:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
            continue
        if char == '"':
            in_string = True
        elif char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return index
    return -1


def _find_key_block(text, key, start=0):
    pattern = re.compile(r'"%s"\s*:\s*\{' % re.escape(key))
    match = pattern.search(text, start)
    if not match:
        return None
    open_index = text.find("{", match.start())
    close_index = _match_brace(text, open_index)
    if close_index < 0:
        return None
    return text[open_index + 1:close_index], open_index, close_index


def _iter_named_blocks(text):
    index = 0
    pattern = re.compile(r'"([^"]+)"\s*:\s*\{')
    while index < len(text):
        match = pattern.search(text, index)
        if not match:
            break
        open_index = text.find("{", match.start())
        close_index = _match_brace(text, open_index)
        if close_index < 0:
            break
        yield match.group(1), text[open_index + 1:close_index], match.start(), close_index
        index = close_index + 1


def _iter_named_entries(text):
    index = 0
    pattern = re.compile(r'"([^"]+)"\s*:\s*')
    while index < len(text):
        match = pattern.search(text, index)
        if not match:
            break
        value_index = match.end()
        while value_index < len(text) and text[value_index].isspace():
            value_index += 1
        if value_index < len(text) and text[value_index] == "{":
            close_index = _match_brace(text, value_index)
            if close_index < 0:
                break
            yield match.group(1), text[value_index + 1:close_index], match.start(), close_index
            index = close_index + 1
            continue

        line_end = text.find("\n", value_index)
        if line_end < 0:
            line_end = len(text)
        yield match.group(1), None, match.start(), line_end
        index = max(line_end + 1, match.end())


def _parse_number_list(value):
    result = []
    for item in re.split(r"[\s,]+", value.strip()):
        if not item:
            continue
        try:
            result.append(float(item))
        except Exception:
            pass
    return result


def _find_array(block, key, default=None):
    match = re.search(r'"%s"\s*:\s*\[([^\]]*)\]' % re.escape(key), block)
    if not match:
        return list(default or [])
    return _parse_number_list(match.group(1))


def _find_string(block, key, default=""):
    match = re.search(r'"%s"\s*:\s*"([^"]*)"' % re.escape(key), block)
    if not match:
        return default
    return match.group(1)


def _find_int(block, key, default=0):
    match = re.search(r'"%s"\s*:\s*(-?\d+)' % re.escape(key), block)
    if not match:
        return default
    try:
        return int(match.group(1))
    except Exception:
        return default


def _build_prim_ranges(prim_entries):
    prim_groups = []
    material_groups = []
    index_cursor = 0
    face_cursor = 0
    for prim_index, entry in enumerate(prim_entries):
        count = int(entry.get("count", 0) or 0)
        if count <= 0:
            continue
        material = entry.get("material", "")
        mesh_name = entry.get("mesh", "") or material or "Prim_%03d" % prim_index
        face_count = count // 3
        group = {
            "name": mesh_name,
            "mesh": mesh_name,
            "material": material,
            "range_kind": "face",
            "face_begin": face_cursor,
            "face_end": face_cursor + face_count,
        }
        prim_groups.append(group)
        material_groups.append(
            _LegacyMaterialGroup(material or mesh_name, face_cursor, face_cursor + face_count)
        )
        index_cursor += count
        face_cursor += face_count
    return prim_groups, material_groups, index_cursor


def _parse_primary_model(scne_text):
    model_block = _find_key_block(scne_text, "Model")
    if not model_block:
        return None
    models = list(_iter_named_blocks(model_block[0]))
    if not models:
        return None

    selected = None
    for name, block, _start, _end in models:
        binary = _find_string(block, "Binary")
        if binary.lower().endswith(".model") and name.lower() == "hihead":
            selected = name, block
            break
    if selected is None:
        for name, block, _start, _end in models:
            binary = _find_string(block, "Binary")
            if binary.lower().endswith(".model"):
                selected = name, block
                break
    if selected is None:
        return None

    name, block = selected
    prim_block = _find_key_block(block, "Prim")
    prim_entries = []
    if prim_block:
        for prim_index, (material, prim, _start, _end) in enumerate(_iter_named_entries(prim_block[0])):
            count = _find_int(prim, "Count", 0) if prim is not None else 0
            mesh_name = (
                _find_string(prim, "Mesh", "") if prim is not None else ""
            ) or material or "Prim_%03d" % prim_index
            prim_entries.append({
                "material": material,
                "mesh": mesh_name,
                "count": count,
                "missing_count": prim is None or count <= 0,
            })

    prim_groups, material_groups, index_cursor = _build_prim_ranges(prim_entries)

    vertex_format = _find_key_block(block, "VertexFormat")
    position_offset = [0.0, 0.0, 0.0, 0.0]
    position_scale = [1.0, 1.0, 1.0, 1.0]
    uv_offset = [0.0, 0.0, 0.0, 0.0]
    uv_scale = [1.0, 1.0, 1.0, 1.0]
    texcoord1_stream = None
    texcoord1_byte_offset = None
    if vertex_format:
        position = _find_key_block(vertex_format[0], "POSITION0")
        if position:
            position_offset = _find_array(position[0], "Offset", position_offset)
            position_scale = _find_array(position[0], "Scale", position_scale)
        texcoord = _find_key_block(vertex_format[0], "TEXCOORD0")
        if texcoord:
            uv_offset = _find_array(texcoord[0], "Offset", uv_offset)
            uv_scale = _find_array(texcoord[0], "Scale", uv_scale)
        texcoord1 = _find_key_block(vertex_format[0], "TEXCOORD1")
        if texcoord1:
            texcoord1_stream = _find_int(texcoord1[0], "Stream", None)
            texcoord1_byte_offset = _find_int(texcoord1[0], "ByteOffset", None)

    return {
        "name": name,
        "binary": _find_string(block, "Binary"),
        "index_count": index_cursor,
        "index_crc32": _find_int(block, "IndexBufferCrc32", None),
        "position_offset": position_offset,
        "position_scale": position_scale,
        "uv_offset": uv_offset,
        "uv_scale": uv_scale,
        "texcoord1_stream": texcoord1_stream,
        "texcoord1_byte_offset": texcoord1_byte_offset,
        "vertex_weights": _find_int(block, "VertexWeights", None),
        "prim_entries": prim_entries,
        "prim_groups": prim_groups,
        "material_groups": material_groups,
    }


def _detect_packed_generation(info):
    if _is_nba2k17_packed_model(info):
        return "NBA 2K17"
    if (
        info.get("texcoord1_stream") == 2
        and info.get("texcoord1_byte_offset") is None
        and info.get("vertex_weights") is None
    ):
        return "NBA 2K18"
    if (
        info.get("texcoord1_stream") == 1
        and info.get("texcoord1_byte_offset") == 20
        and info.get("vertex_weights") == 4
    ):
        return "NBA 2K24"
    return "Legacy (unclassified)"


def _decode_snorm16(value, offset, scale):
    return (float(value) / 32767.0) * float(scale) + float(offset)


def _legacy_game_to_blender_position(position):
    x, y, z = position
    return (x, -z, y)


def _walk_sections(data, start, vertex_count):
    sections = []
    cursor = start
    while cursor + 24 <= len(data):
        marker, size, crc_a, crc_b = struct.unpack_from("<IIII", data, cursor)
        if size < 24 or cursor + size > len(data):
            break
        payload = size - 24
        if payload in (vertex_count * 8, vertex_count * 4):
            sections.append({
                "start": cursor,
                "data_start": cursor + 24,
                "size": size,
                "payload": payload,
                "bytes_per_vertex": payload // vertex_count,
                "marker": marker,
                "crc_a": crc_a,
                "crc_b": crc_b,
            })
            cursor += size
            continue
        sections.append({
            "start": cursor,
            "data_start": cursor + 24,
            "size": size,
            "payload": payload,
            "bytes_per_vertex": None,
            "marker": marker,
            "crc_a": crc_a,
            "crc_b": crc_b,
        })
        cursor += size
        if len(sections) >= 8:
            break
    return sections


def _is_nba2k17_packed_model(info):
    model_name = str(info.get("name", "") or "").strip().lower()
    binary_name = os.path.basename(str(info.get("binary", "") or "")).lower()
    return model_name == "player" and binary_name.startswith("player.") and binary_name.endswith(".model")


def _packed_index_section(data):
    if len(data) < 56:
        return None
    marker, size = struct.unpack_from("<II", data, 8)
    if size < 24 or (size - 24) % 2:
        return None
    next_section = 8 + size
    if next_section + 24 > len(data):
        return None
    next_marker, next_size = struct.unpack_from("<II", data, next_section)
    if next_marker != marker or next_size < 24 or next_section + next_size > len(data):
        return None
    return {
        "marker": marker,
        "size": size,
        "index_offset": 32,
        "index_count": (size - 24) // 2,
        "next_section": next_section,
    }


def _resolve_nba2k17_prim_counts(info, packed_index_count):
    scne_index_count = int(info.get("index_count", 0) or 0)
    missing_count = int(packed_index_count) - scne_index_count
    info["scne_index_count"] = scne_index_count
    info["packed_index_count"] = int(packed_index_count)
    info["inferred_prim_index_count"] = 0
    info["inferred_prim_name"] = ""
    if missing_count == 0:
        return
    if missing_count < 0 or missing_count % 3:
        raise ValueError(
            "NBA 2K17 packed index count does not match SCNE Prim ranges "
            "(SCNE %d, packed %d)." % (scne_index_count, packed_index_count)
        )

    missing_entries = [
        entry for entry in info.get("prim_entries", [])
        if int(entry.get("count", 0) or 0) <= 0
    ]
    if len(missing_entries) == 1:
        inferred_entry = missing_entries[0]
    else:
        inferred_entry = {
            "material": "legacy_unassigned",
            "mesh": "legacy_unassigned",
            "count": 0,
            "missing_count": True,
        }
        info.setdefault("prim_entries", []).append(inferred_entry)

    inferred_entry["count"] = missing_count
    inferred_entry["inferred_count"] = True
    prim_groups, material_groups, rebuilt_index_count = _build_prim_ranges(
        info.get("prim_entries", [])
    )
    if rebuilt_index_count != packed_index_count:
        raise ValueError(
            "Could not rebuild NBA 2K17 Prim ranges "
            "(rebuilt %d, packed %d)." % (rebuilt_index_count, packed_index_count)
        )
    info["index_count"] = rebuilt_index_count
    info["prim_groups"] = prim_groups
    info["material_groups"] = material_groups
    info["inferred_prim_index_count"] = missing_count
    info["inferred_prim_name"] = inferred_entry.get("mesh", "") or inferred_entry.get("material", "")


def _read_packed_model(model_path, info, flip_uv_v=True):
    with open(model_path, "rb") as handle:
        data = handle.read()
    if len(data) < 64:
        raise ValueError("Packed .model is too small.")

    file_size = struct.unpack_from("<I", data, 4)[0]
    if file_size != len(data):
        raise ValueError("Packed .model size header does not match the file.")

    legacy_game = _detect_packed_generation(info)
    is_nba2k17 = legacy_game == "NBA 2K17"
    index_section = _packed_index_section(data)
    if is_nba2k17:
        if index_section is None:
            raise ValueError("Could not find the NBA 2K17 packed index section.")
        index_count = int(index_section["index_count"])
        _resolve_nba2k17_prim_counts(info, index_count)
        info["legacy_game"] = "NBA 2K17"
        print(
            "[NBA 2K17 Legacy Import] Packed player model detected: "
            "%d indices (%d inferred for %s)."
            % (
                index_count,
                int(info.get("inferred_prim_index_count", 0) or 0),
                info.get("inferred_prim_name", "") or "no missing Prim",
            )
        )
    else:
        index_count = int(info["index_count"] or 0)
        info["legacy_game"] = legacy_game
        info["scne_index_count"] = index_count
        info["packed_index_count"] = index_count
        info["inferred_prim_index_count"] = 0
        info["inferred_prim_name"] = ""
    if index_count <= 0:
        raise ValueError("SCNE Prim data did not provide an index count.")

    index_offset = index_section["index_offset"] if is_nba2k17 else 32
    index_bytes = index_count * 2
    index_end = index_section["next_section"] if is_nba2k17 else index_offset + index_bytes
    if index_end > len(data):
        raise ValueError("Packed .model index block is outside the file.")

    index_blob = data[index_offset:index_offset + index_bytes]
    expected_crc = info.get("index_crc32")
    if expected_crc is not None and (zlib.crc32(index_blob) & 0xFFFFFFFF) != expected_crc:
        print("[NBA 2K Legacy Import] Index CRC did not match; continuing with decoded indices.")

    indices = struct.unpack_from("<%dH" % index_count, data, index_offset)
    vertex_count = max(indices) + 1 if indices else 0
    if vertex_count <= 0:
        raise ValueError("Packed .model did not contain usable indices.")

    sections = _walk_sections(data, index_end, vertex_count)
    if not sections or sections[0].get("bytes_per_vertex") != 8:
        raise ValueError("Could not find packed POSITION0 stream.")

    pos_start = sections[0]["data_start"]
    po = info["position_offset"] + [0.0, 0.0, 0.0, 0.0]
    ps = info["position_scale"] + [1.0, 1.0, 1.0, 1.0]
    vertices = []
    for index in range(vertex_count):
        x, y, z, _w = struct.unpack_from("<hhhh", data, pos_start + index * 8)
        raw_position = (
            _decode_snorm16(x, po[0], ps[0]),
            _decode_snorm16(y, po[1], ps[1]),
            _decode_snorm16(z, po[2], ps[2]),
        )
        vertices.append(_legacy_game_to_blender_position(raw_position))

    triangles = [tuple(indices[index:index + 3]) for index in range(0, len(indices), 3)]

    texcoords = []
    uv_section = None
    four_byte_sections = [section for section in sections if section.get("bytes_per_vertex") == 4]
    if four_byte_sections:
        uv_section = four_byte_sections[0]
    if uv_section:
        uo = info["uv_offset"] + [0.0, 0.0, 0.0, 0.0]
        us = info["uv_scale"] + [1.0, 1.0, 1.0, 1.0]
        uv_values = []
        for index in range(vertex_count):
            u, v = struct.unpack_from("<hh", data, uv_section["data_start"] + index * 4)
            decoded_u = _decode_snorm16(u, uo[0], us[0])
            decoded_v = _decode_snorm16(v, uo[1], us[1])
            if flip_uv_v:
                decoded_v = 1.0 - decoded_v
            uv_values.append((
                decoded_u,
                decoded_v,
            ))
        texcoords.append(uv_values)

    return vertices, triangles, texcoords


def _blender_to_legacy_game_position(position):
    try:
        x, y, z = position.x, position.y, position.z
    except AttributeError:
        x, y, z = position
    return (float(x), float(z), -float(y))


def _encode_snorm16(value, offset, scale, vertex_index, axis_name):
    scale = float(scale)
    if not scale:
        raise RuntimeError("Packed POSITION0 has a zero %s scale." % axis_name)
    encoded = int(round(((float(value) - float(offset)) / scale) * 32767.0))
    if encoded < -32767 or encoded > 32767:
        raise RuntimeError(
            "Vertex %d %s is outside the target NBA 2K23 packed position range "
            "(value %.6f, encoded %d)." % (vertex_index, axis_name, value, encoded)
        )
    return encoded


def _packed_model_infos(scne_text):
    model_block = _find_key_block(scne_text, "Model")
    if not model_block:
        return []
    infos = []
    for name, block, _start, _end in _iter_named_blocks(model_block[0]):
        if not _find_string(block, "Binary").lower().endswith(".model"):
            continue
        isolated = '"Model":{"%s":{%s}}}' % (name, block)
        info = _parse_primary_model(isolated)
        if info:
            infos.append(info)
    return infos


def _patch_packed_model_positions(model_path, info, positions):
    with open(model_path, "rb") as handle:
        original = handle.read()
    if len(original) < 64 or struct.unpack_from("<I", original, 4)[0] != len(original):
        raise RuntimeError("The NBA 2K23 packed model header is invalid.")

    index_count = int(info.get("index_count", 0) or 0)
    if index_count <= 0:
        raise RuntimeError("The NBA 2K23 scene has no usable primary index range.")
    index_end = 32 + index_count * 2
    if index_end > len(original):
        raise RuntimeError("The NBA 2K23 packed index block is outside the model file.")
    indices = struct.unpack_from("<%dH" % index_count, original, 32)
    vertex_count = max(indices) + 1 if indices else 0
    sections = _walk_sections(original, index_end, vertex_count)
    if not sections or sections[0].get("bytes_per_vertex") != 8:
        raise RuntimeError("Could not find the NBA 2K23 packed POSITION0 section.")

    if len(positions) != vertex_count:
        raise RuntimeError(
            "NBA 2K23 packed export must preserve vertex count (%d imported, %d target)."
            % (len(positions), vertex_count)
        )

    po = list(info.get("position_offset", ())) + [0.0, 0.0, 0.0, 0.0]
    ps = list(info.get("position_scale", ())) + [1.0, 1.0, 1.0, 1.0]
    data = bytearray(original)
    position_start = sections[0]["data_start"]
    for vertex_index, position in enumerate(positions):
        game_position = _blender_to_legacy_game_position(position)
        encoded = [
            _encode_snorm16(game_position[axis], po[axis], ps[axis], vertex_index, "XYZ"[axis])
            for axis in range(3)
        ]
        _old_x, _old_y, _old_z, old_w = struct.unpack_from(
            "<hhhh", data, position_start + vertex_index * 8
        )
        struct.pack_into(
            "<hhhh",
            data,
            position_start + vertex_index * 8,
            encoded[0], encoded[1], encoded[2], old_w,
        )

    temporary = model_path + ".packed-position.tmp"
    try:
        with open(temporary, "wb") as handle:
            handle.write(data)
        os.replace(temporary, model_path)
    finally:
        if os.path.exists(temporary):
            os.remove(temporary)
    return vertex_count


def patch_legacy_packed_scene(scne_path, objects):
    """Patch packed POSITION0 streams while preserving topology and every other payload."""
    if not isinstance(objects, (list, tuple)):
        objects = [objects]
    objects = [obj for obj in objects if obj is not None]
    if not objects:
        raise RuntimeError("No imported NBA 2K23 mesh was selected for packed export.")

    scne_text = _read_text(scne_path)
    infos = _packed_model_infos(scne_text)
    info = next((item for item in infos if item.get("name", "").lower() == "hihead"), None)
    if not info:
        raise RuntimeError("Could not find the primary packed model in hihead.SCNE.")
    model_path = os.path.join(os.path.dirname(scne_path), info["binary"])
    original_positions, _triangles, _texcoords = _read_packed_model(
        model_path, info, bool(objects[0].get("nba2k_legacy_flip_uv_v", True))
    )
    positions = [vertex.co.copy() for obj in objects for vertex in obj.data.vertices]
    vertex_count = _patch_packed_model_positions(model_path, info, positions)

    lod_buffers = []
    if len(original_positions) == len(positions):
        from mathutils import Vector
        from mathutils.kdtree import KDTree

        tree = KDTree(len(original_positions))
        for index, position in enumerate(original_positions):
            tree.insert(Vector(position), index)
        tree.balance()
        deltas = [positions[index] - Vector(original_positions[index]) for index in range(len(positions))]
        for lod_info in infos:
            if not lod_info.get("name", "").lower().startswith("hihead_lodshape"):
                continue
            lod_path = os.path.join(os.path.dirname(scne_path), lod_info["binary"])
            lod_positions, _lod_triangles, _lod_texcoords = _read_packed_model(
                lod_path, lod_info, bool(objects[0].get("nba2k_legacy_flip_uv_v", True))
            )
            adjusted = []
            for position in lod_positions:
                vector = Vector(position)
                _nearest, source_index, _distance = tree.find(vector)
                adjusted.append(vector + deltas[source_index])
            _patch_packed_model_positions(lod_path, lod_info, adjusted)
            lod_buffers.append(lod_path)

    return {
        "scne": scne_path,
        "position_buffer": model_path,
        "vertex_count": vertex_count,
        "buffer_vertex_count": vertex_count,
        "position_format": "R16G16B16A16_SNORM_PACKED_MODEL",
        "uv_buffer": "",
        "lod_buffers": lod_buffers,
    }


def _legacy_group_vertex_indices(obj, group_name):
    group = obj.vertex_groups.get(group_name)
    if group is None:
        return []
    indices = []
    for vertex in obj.data.vertices:
        for membership in vertex.groups:
            if membership.group == group.index:
                indices.append(vertex.index)
                break
    return indices


def _replace_vertex_group(obj, group_name, indices):
    old_group = obj.vertex_groups.get(group_name)
    if old_group is not None:
        obj.vertex_groups.remove(old_group)
    if not indices:
        return None
    group = obj.vertex_groups.new(name=group_name)
    group.add(sorted(set(indices)), 1.0, 'ADD')
    return group


def _legacy_group_vertex_components(obj, indices):
    index_set = set(indices)
    adjacency = {index: set() for index in index_set}
    for polygon in obj.data.polygons:
        polygon_indices = [index for index in polygon.vertices if index in index_set]
        if len(polygon_indices) < 2:
            continue
        for first in polygon_indices:
            for second in polygon_indices:
                if first != second:
                    adjacency[first].add(second)

    components = []
    seen = set()
    for start in sorted(index_set):
        if start in seen:
            continue
        stack = [start]
        seen.add(start)
        component = []
        while stack:
            current = stack.pop()
            component.append(current)
            for neighbor in adjacency.get(current, ()):
                if neighbor not in seen:
                    seen.add(neighbor)
                    stack.append(neighbor)
        components.append(component)
    return components


def _split_legacy_lashes_by_mesh_island(obj, indices):
    components = _legacy_group_vertex_components(obj, indices)
    useful_components = [component for component in components if len(component) > 1]
    if len(useful_components) >= 2:
        useful_components.sort(
            key=lambda component: (
                -len(component),
                -sum(float(obj.data.vertices[index].co.z) for index in component) / len(component),
            )
        )
        selected = useful_components[:2]
        selected.sort(
            key=lambda component: sum(float(obj.data.vertices[index].co.z) for index in component) / len(component),
            reverse=True,
        )
        return selected[0], selected[1], "mesh_island"

    z_values = [float(obj.data.vertices[index].co.z) for index in indices]
    threshold = (min(z_values) + max(z_values)) * 0.5
    upper_indices = []
    lower_indices = []
    for index in indices:
        if float(obj.data.vertices[index].co.z) >= threshold:
            upper_indices.append(index)
        else:
            lower_indices.append(index)
    return upper_indices, lower_indices, "z_threshold"


def _split_legacy_combined_eyelash_groups(obj):
    created = []
    split_specs = (
        ("L_eyelashesShape", "L_upper_eyelashesShape", "L_lower_eyelashesShape"),
        ("R_eyelashesShape", "R_upper_eyelashesShape", "R_lower_eyelashesShape"),
    )
    for combined_name, upper_name, lower_name in split_specs:
        indices = _legacy_group_vertex_indices(obj, combined_name)
        if not indices:
            continue
        upper_indices, lower_indices, split_method = _split_legacy_lashes_by_mesh_island(obj, indices)
        if not upper_indices or not lower_indices:
            continue
        _replace_vertex_group(obj, upper_name, upper_indices)
        _replace_vertex_group(obj, lower_name, lower_indices)
        created.append({
            "combined": combined_name,
            "upper": upper_name,
            "lower": lower_name,
            "method": split_method,
            "upper_count": len(set(upper_indices)),
            "lower_count": len(set(lower_indices)),
        })
    if created:
        obj["nba2k_legacy_eyelashes_split"] = True
        obj["nba2k_legacy_eyelash_split_groups"] = repr(created)
        for group in created:
            print(
                "[NBA 2K Legacy Import] Split %s into %s (%d verts) and %s (%d verts)."
                % (
                    group["combined"],
                    group["upper"],
                    group["upper_count"],
                    group["lower"],
                    group["lower_count"],
                )
            )
    return created


def can_load_legacy_packed_scene(scne_path):
    try:
        if not scne_path or not os.path.isfile(scne_path):
            return False
        text = _read_text(scne_path)
        info = _parse_primary_model(text)
        if not info:
            return False
        model_path = os.path.join(os.path.dirname(scne_path), info["binary"])
        if not os.path.isfile(model_path):
            return False
        with open(model_path, "rb") as handle:
            header = handle.read(8)
        return len(header) == 8 and struct.unpack_from("<I", header, 4)[0] == os.path.getsize(model_path)
    except Exception:
        return False


def load_legacy_packed_scene(scne_path, args):
    text = _read_text(scne_path)
    info = _parse_primary_model(text)
    if not info:
        raise ValueError("No packed legacy .model entry found in SCNE.")

    model_path = os.path.join(os.path.dirname(scne_path), info["binary"])
    flip_uv_v = True
    if isinstance(args, dict):
        flip_uv_v = bool(args.get("legacy_flip_uv_v", True))
    vertices, triangles, texcoords = _read_packed_model(model_path, info, flip_uv_v)
    mesh = _LegacySkinMesh(
        info["name"],
        vertices,
        triangles,
        texcoords,
        info["prim_groups"],
        info["material_groups"],
    )

    collection = getNewSceneCollection(os.path.basename(scne_path).split(".")[0] + "_legacy")
    obj = loadSkinMesh(mesh, args or {}, scne_path, info["prim_groups"], 0, 0)
    if obj is None:
        raise ValueError("Could not create Blender mesh from packed legacy model.")
    obj["nba2k_legacy_packed_import"] = True
    obj["nba2k_legacy_basis"] = "game_xyz_to_blender_x_negz_y"
    obj["nba2k_legacy_flip_uv_v"] = bool(flip_uv_v)
    obj["nba2k_legacy_model_binary"] = info["binary"]
    obj["nba2k_legacy_game"] = info.get("legacy_game", "NBA 2K24")
    obj["nba2k_legacy_uv_profile"] = "%s_%s" % (
        obj["nba2k_legacy_game"].replace("NBA ", "").replace(" ", "_"),
        "flipped_v" if flip_uv_v else "raw_v",
    )
    obj["nba2k_legacy_vertex_count"] = len(vertices)
    obj["nba2k_legacy_triangle_count"] = len(triangles)
    obj["nba2k_legacy_scne_index_count"] = int(info.get("scne_index_count", 0) or 0)
    obj["nba2k_legacy_packed_index_count"] = int(info.get("packed_index_count", 0) or 0)
    obj["nba2k_legacy_inferred_prim_index_count"] = int(
        info.get("inferred_prim_index_count", 0) or 0
    )
    obj["nba2k_legacy_inferred_prim_name"] = info.get("inferred_prim_name", "")
    obj["nba2k_source_path"] = scne_path
    _split_legacy_combined_eyelash_groups(obj)
    collection.objects.link(obj)

    if isinstance(args, dict) and args.get("split_groups"):
        splitAllMaterialGroups(obj)

    try:
        import bpy
        bpy.ops.view3d.view_all()
    except Exception:
        pass

    print(
        "[NBA 2K Legacy Import] Imported %s packed model %s: %d verts, %d faces, %d groups."
        % (
            info.get("legacy_game", "legacy"),
            info["binary"],
            len(vertices),
            len(triangles),
            len(info["prim_groups"]),
        )
    )
    return True
