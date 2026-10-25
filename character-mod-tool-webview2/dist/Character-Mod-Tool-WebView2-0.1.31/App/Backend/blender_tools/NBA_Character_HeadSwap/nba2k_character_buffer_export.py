import gzip
import math
import os
import re
import struct

try:
    import bpy
except Exception:
    bpy = None


def _read_text(path):
    with open(path, "r", encoding="utf-8", errors="ignore") as handle:
        return handle.read()


def _find_balanced(text, start, open_char, close_char):
    depth = 0
    in_string = False
    escaped = False
    for index in range(start, len(text)):
        ch = text[index]
        if in_string:
            if escaped:
                escaped = False
            elif ch == "\\":
                escaped = True
            elif ch == '"':
                in_string = False
            continue
        if ch == '"':
            in_string = True
        elif ch == open_char:
            depth += 1
        elif ch == close_char:
            depth -= 1
            if depth == 0:
                return text[start:index + 1]
    return ""


def _extract_vertexstream_array(scne_text):
    key = scne_text.find('"VertexStream"')
    if key < 0:
        return ""
    bracket = scne_text.find("[", key)
    if bracket < 0:
        return ""
    return _find_balanced(scne_text, bracket, "[", "]")


def _split_vertexstream_entries(scne_text):
    array_text = _extract_vertexstream_array(scne_text)
    if not array_text:
        return []
    entries = []
    index = 1
    while index < len(array_text) - 1:
        ch = array_text[index]
        if ch.isspace() or ch == ",":
            index += 1
            continue
        if array_text.startswith("null", index):
            entries.append(None)
            index += 4
            continue
        if ch == "{":
            block = _find_balanced(array_text, index, "{", "}")
            if not block:
                break
            entries.append(block)
            index += len(block)
            continue
        index += 1
    return entries


def _parse_vertexstream_entries(scne_text):
    parsed = []
    for entry in _split_vertexstream_entries(scne_text):
        if entry is None:
            parsed.append(None)
            continue
        stride = re.search(r'"Stride"\s*:\s*(\d+)', entry)
        binary = re.search(r'"Binary"\s*:\s*"([^"]+)"', entry)
        size = re.search(r'"Size"\s*:\s*(\d+)', entry)
        parsed.append({
            "stride": int(stride.group(1)) if stride else 0,
            "binary": binary.group(1) if binary else "",
            "size": int(size.group(1)) if size else 0,
        })
    return parsed


def _parse_vertex_format(scne_text, name):
    match = re.search(r'"%s"\s*:\s*\{([^}]*)\}' % re.escape(name), scne_text, flags=re.S)
    if not match:
        return {}
    block = match.group(1)
    info = {}
    fmt = re.search(r'"Format"\s*:\s*"([^"]+)"', block)
    byte_offset = re.search(r'"ByteOffset"\s*:\s*(\d+)', block)
    stream = re.search(r'"Stream"\s*:\s*(\d+)', block)
    offset = re.search(r'"Offset"\s*:\s*\[([^\]]+)\]', block)
    scale = re.search(r'"Scale"\s*:\s*\[([^\]]+)\]', block)
    if fmt:
        info["format"] = fmt.group(1)
    if byte_offset:
        info["byte_offset"] = int(byte_offset.group(1))
    if stream:
        info["stream"] = int(stream.group(1))
    if offset:
        info["offset"] = _parse_float_list(offset.group(1), 4, 0.0)
    if scale:
        info["scale"] = _parse_float_list(scale.group(1), 4, 1.0)
    return info


def _parse_float_list(raw, count, fill):
    values = []
    for part in raw.replace(",", " ").split():
        try:
            values.append(float(part))
        except Exception:
            pass
    while len(values) < count:
        values.append(fill)
    return values[:count]


def _candidate_buffer_paths(scne_path, binary_name):
    directory = os.path.dirname(scne_path)
    base = os.path.basename(binary_name)
    stem = os.path.splitext(base)[0]
    names = [base, stem + ".bin", stem + ".gz"]
    seen = set()
    for name in names:
        if name in seen:
            continue
        seen.add(name)
        yield os.path.join(directory, name)


def _resolve_buffer_path(scne_path, binary_name):
    for path in _candidate_buffer_paths(scne_path, binary_name):
        if os.path.exists(path):
            return path
    return None


def _is_gzip_file(path):
    try:
        with open(path, "rb") as handle:
            return handle.read(2) == b"\x1f\x8b"
    except Exception:
        return False


def _read_buffer(path):
    if _is_gzip_file(path):
        with gzip.open(path, "rb") as handle:
            return bytearray(handle.read())
    with open(path, "rb") as handle:
        return bytearray(handle.read())


def _write_buffer(path, data):
    if _is_gzip_file(path):
        with gzip.open(path, "wb") as handle:
            handle.write(data)
        return
    with open(path, "wb") as handle:
        handle.write(data)


def _encode_snorm_i16(value):
    value = max(-1.0, min(1.0, float(value)))
    if value <= -1.0:
        return -32768
    if value >= 1.0:
        return 32767
    return int(round(value * 32767.0))


def _decode_snorm_i16(value):
    if value <= -32768:
        return -1.0
    if value >= 32767:
        return 1.0
    return float(value) / 32767.0


def _encode_unorm_u16(value):
    value = max(0.0, min(1.0, float(value)))
    return int(round(value * 65535.0))


def _collect_mesh_uvs(objects, vertex_count):
    uv_per_vertex = [None] * vertex_count
    for obj in objects:
        if not obj.data.uv_layers:
            continue
        uv_layer = obj.data.uv_layers.active or obj.data.uv_layers[0]
        base = _object_vertex_offset(obj)
        for polygon in obj.data.polygons:
            for loop_index in polygon.loop_indices:
                local_index = obj.data.loops[loop_index].vertex_index
                vertex_index = base + local_index
                if vertex_index >= vertex_count:
                    continue
                uv = uv_layer.data[loop_index].uv
                if uv_per_vertex[vertex_index] is None:
                    uv_per_vertex[vertex_index] = (float(uv.x), float(uv.y))
    if all(uv is None for uv in uv_per_vertex):
        return None
    return [(0.0, 0.0) if uv is None else uv for uv in uv_per_vertex]


def _score_uv_mode(data, stride, byte_offset, is_snorm, scale, offset, mesh_uvs, mode, swap_uv):
    sample_count = min(800, len(mesh_uvs))
    step = max(1, len(mesh_uvs) // max(1, sample_count))
    error = 0.0
    count = 0
    for vertex_index in range(0, len(mesh_uvs), step):
        pos = vertex_index * stride + byte_offset
        if pos + 4 > len(data):
            break
        if is_snorm:
            packed_u, packed_v = struct.unpack_from("<hh", data, pos)
            su = _decode_snorm_i16(packed_u)
            sv = _decode_snorm_i16(packed_v)
        else:
            packed_u, packed_v = struct.unpack_from("<HH", data, pos)
            su = float(packed_u) / 65535.0
            sv = float(packed_v) / 65535.0
        if mode.startswith("A"):
            decoded = (su * scale[0] + offset[0], sv * scale[1] + offset[1])
        elif mode.startswith("B"):
            decoded = ((su * 0.5 + 0.5) * scale[0] + offset[0], (sv * 0.5 + 0.5) * scale[1] + offset[1])
        else:
            decoded = ((su * 0.5 + 0.5) if is_snorm else su, (sv * 0.5 + 0.5) if is_snorm else sv)
        if mode.endswith("_VFLIP"):
            decoded = (decoded[0], 1.0 - decoded[1])
        mesh_u, mesh_v = mesh_uvs[vertex_index]
        if swap_uv:
            mesh_u, mesh_v = mesh_v, mesh_u
        error += abs(decoded[0] - mesh_u) + abs(decoded[1] - mesh_v)
        count += 1
        if count >= sample_count:
            break
    return error / max(1, count)


def _patch_uv_buffer(scne_path, scne_text, objects, vertex_count):
    uv_info = _parse_vertex_format(scne_text, "TEXCOORD0")
    stream_index = int(uv_info.get("stream", 0))
    streams = _parse_vertexstream_entries(scne_text)
    if not (0 <= stream_index < len(streams)) or not streams[stream_index]:
        return None
    binary_name = streams[stream_index].get("binary", "")
    buffer_path = _resolve_buffer_path(scne_path, binary_name)
    if not buffer_path:
        return None

    data = _read_buffer(buffer_path)
    stride = int(streams[stream_index].get("stride", 0)) or 16
    byte_offset = int(uv_info.get("byte_offset", 0))
    if len(data) // stride != vertex_count:
        print("[NBA Character Export] UV buffer count did not match; model positions were still exported.")
        return None

    mesh_uvs = _collect_mesh_uvs(objects, vertex_count)
    if mesh_uvs is None:
        return None

    fmt = str(uv_info.get("format", "R16G16_SNORM")).upper()
    is_snorm = "SNORM" in fmt
    scale = uv_info.get("scale", [1.0, 1.0, 1.0, 1.0])
    offset = uv_info.get("offset", [0.0, 0.0, 0.0, 0.0])
    modes = ["A", "A_VFLIP", "B", "B_VFLIP", "C", "C_VFLIP"]
    best_mode = "A"
    best_swap = False
    best_score = 1.0e30
    for mode in modes:
        for swap_uv in (False, True):
            score = _score_uv_mode(data, stride, byte_offset, is_snorm, scale, offset, mesh_uvs, mode, swap_uv)
            if score < best_score:
                best_score = score
                best_mode = mode
                best_swap = swap_uv

    for vertex_index, (u, v) in enumerate(mesh_uvs):
        if best_swap:
            u, v = v, u
        if best_mode.endswith("_VFLIP"):
            v = 1.0 - v
        if best_mode.startswith("A"):
            su = (u - offset[0]) / scale[0]
            sv = (v - offset[1]) / scale[1]
        elif best_mode.startswith("B"):
            su = ((u - offset[0]) / scale[0] - 0.5) * 2.0
            sv = ((v - offset[1]) / scale[1] - 0.5) * 2.0
        else:
            su = u * 2.0 - 1.0 if is_snorm else u
            sv = v * 2.0 - 1.0 if is_snorm else v
        pos = vertex_index * stride + byte_offset
        if is_snorm:
            struct.pack_into("<hh", data, pos, _encode_snorm_i16(su), _encode_snorm_i16(sv))
        else:
            struct.pack_into("<HH", data, pos, _encode_unorm_u16(su), _encode_unorm_u16(sv))

    _write_buffer(buffer_path, data)
    return os.path.basename(buffer_path)


def _object_vertex_offset(obj):
    try:
        return int(obj.get("nba2k_global_vertex_offset", 0) or 0)
    except Exception:
        return 0


def _ordered_mesh_objects(objects):
    meshes = [obj for obj in objects if obj is not None and getattr(obj, "type", None) == "MESH"]
    return sorted(meshes, key=lambda obj: (_object_vertex_offset(obj), obj.name))


def _baseline_position(obj, vertex):
    try:
        attr = obj.data.attributes.get("nba2k_import_position")
        if attr and vertex.index < len(attr.data):
            return attr.data[vertex.index].vector
    except Exception:
        pass
    return vertex.co


def _read_position(data, index, stride):
    pos = index * stride
    return struct.unpack_from("<fff", data, pos)


def _encode_r21g21b22(position, offset, scale, vertex_index):
    bit_widths = (21, 21, 22)
    encoded = []
    for axis, bits in enumerate(bit_widths):
        axis_scale = float(scale[axis])
        if not math.isfinite(axis_scale) or axis_scale == 0.0:
            raise RuntimeError(f"Invalid NBA 2K27 POSITION0 scale on axis {axis}.")
        value = (float(position[axis]) - float(offset[axis])) / axis_scale
        quantized = int(math.floor(value + 0.5))
        maximum = (1 << bits) - 1
        if quantized < 0 or quantized > maximum:
            axis_name = "XYZ"[axis]
            raise RuntimeError(
                f"Vertex {vertex_index} {axis_name} position is outside the NBA 2K27 packed range "
                f"({position[axis]:.6f}; encoded value {quantized}, allowed 0..{maximum})."
            )
        encoded.append(quantized)
    return encoded[0] | (encoded[1] << 21) | (encoded[2] << 42)


def _write_position(data, byte_position, position, position_info, vertex_index):
    fmt = str(position_info.get("format", "R32G32B32_FLOAT")).upper()
    if fmt == "R32G32B32_FLOAT":
        struct.pack_into("<fff", data, byte_position, position[0], position[1], position[2])
        return
    if fmt == "R21G21B22_UINT":
        offset = position_info.get("offset", [0.0, 0.0, 0.0, 0.0])
        scale = position_info.get("scale", [1.0, 1.0, 1.0, 1.0])
        packed = _encode_r21g21b22(position, offset, scale, vertex_index)
        struct.pack_into("<Q", data, byte_position, packed)
        return
    raise RuntimeError(f"Unsupported main POSITION0 format: {fmt}.")


def _evaluated_vertex_positions(obj):
    if bpy is None:
        return None
    try:
        depsgraph = bpy.context.evaluated_depsgraph_get()
        eval_obj = obj.evaluated_get(depsgraph)
        eval_mesh = eval_obj.to_mesh()
    except Exception:
        return None
    try:
        if len(eval_mesh.vertices) != len(obj.data.vertices):
            print(
                "[NBA Character Export] Evaluated mesh vertex count changed for "
                f"{obj.name}; exporting base mesh positions instead."
            )
            return None
        return [vertex.co.copy() for vertex in eval_mesh.vertices]
    finally:
        try:
            eval_obj.to_mesh_clear()
        except Exception:
            pass


def _export_vertex_position(obj, vertex, evaluated_positions):
    if evaluated_positions is not None and vertex.index < len(evaluated_positions):
        return evaluated_positions[vertex.index]
    return vertex.co


def _blender_to_game_position(position):
    return (
        float(position.x),
        float(position.z),
        -float(position.y),
    )


def patch_hihead_model_buffers(scne_path, objects, update_uv=False):
    if not isinstance(objects, (list, tuple)):
        objects = [objects]
    objects = _ordered_mesh_objects(objects)
    if not objects:
        raise RuntimeError("Select the imported hihead/body mesh before exporting.")
    if any(bool(obj.get("nba2k_legacy_packed_import")) for obj in objects):
        from .nba2k_legacy_packed_import import patch_legacy_packed_scene
        return patch_legacy_packed_scene(scne_path, objects)

    scne_text = _read_text(scne_path)
    streams = _parse_vertexstream_entries(scne_text)
    if not streams or not streams[0]:
        raise RuntimeError("Could not find the main position VertexStream in hihead.SCNE.")

    pos_stream = streams[0]
    position_info = _parse_vertex_format(scne_text, "POSITION0")
    position_format = str(position_info.get("format", "R32G32B32_FLOAT")).upper()
    pos_path = _resolve_buffer_path(scne_path, pos_stream.get("binary", ""))
    if not pos_path:
        raise RuntimeError("Could not find the main model VertexBuffer beside hihead.SCNE.")

    stride = int(pos_stream.get("stride", 0)) or 12
    expected_stride = {
        "R32G32B32_FLOAT": 12,
        "R21G21B22_UINT": 8,
    }.get(position_format)
    if expected_stride is None:
        raise RuntimeError(f"Unsupported main POSITION0 format: {position_format}.")
    if stride != expected_stride:
        raise RuntimeError(
            f"The {position_format} position stream has stride {stride}; expected {expected_stride}."
        )

    old_data = _read_buffer(pos_path)
    buffer_vertex_count = len(old_data) // stride
    if len(old_data) % stride != 0:
        raise RuntimeError("The main model VertexBuffer size is not aligned to its stride.")

    data = bytearray(old_data)
    written = 0
    highest_index = -1
    for obj in objects:
        base = _object_vertex_offset(obj)
        evaluated_positions = _evaluated_vertex_positions(obj)
        for vertex in obj.data.vertices:
            buffer_index = base + vertex.index
            highest_index = max(highest_index, buffer_index)
            if buffer_index < 0 or buffer_index >= buffer_vertex_count:
                raise RuntimeError(
                    "Blender mesh topology no longer matches the hihead model buffer "
                    f"(buffer vertices: {buffer_vertex_count}, attempted index: {buffer_index})."
                )
            export_position = _export_vertex_position(obj, vertex, evaluated_positions)
            position = _blender_to_game_position(export_position)
            _write_position(data, buffer_index * stride, position, position_info, buffer_index)
            written += 1
    if written == 0:
        raise RuntimeError("No character vertices were available to export.")
    _write_buffer(pos_path, data)

    result = {
        "scne": os.path.basename(scne_path),
        "position_buffer": os.path.basename(pos_path),
        "uv_buffer": None,
        "vertex_count": written,
        "buffer_vertex_count": buffer_vertex_count,
        "position_format": position_format,
    }
    if update_uv:
        uv_name = _patch_uv_buffer(scne_path, scne_text, objects, buffer_vertex_count)
        if uv_name:
            result["uv_buffer"] = uv_name
    return result
