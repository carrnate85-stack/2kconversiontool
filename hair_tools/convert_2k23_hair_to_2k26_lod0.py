import argparse
import copy
import hashlib
import json
import math
import re
import struct
import zipfile
from collections import OrderedDict
from pathlib import Path


SEGMENT_MAGIC = 0x0011799A


def snorm16(value):
    return max(-1.0, min(1.0, value / 32767.0))


def normalize3(vector):
    length = math.sqrt(sum(value * value for value in vector))
    if length < 1e-12:
        return (0.0, 0.0, 1.0)
    return tuple(value / length for value in vector)


def cross3(left, right):
    return (
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    )


def oct_encode_normal(normal):
    x, y, z = normalize3(normal)
    divisor = abs(x) + abs(y) + abs(z)
    x /= divisor
    y /= divisor
    z /= divisor
    if z < 0.0:
        old_x = x
        x = (1.0 - abs(y)) * (1.0 if old_x >= 0.0 else -1.0)
        y = (1.0 - abs(old_x)) * (1.0 if y >= 0.0 else -1.0)
    packed_x = max(0, min(1023, round((x + 1.0) * 511.5)))
    packed_y = max(0, min(1023, round((y + 1.0) * 511.5)))
    return packed_x | (packed_y << 10)


def pack_tangent_frame(binormal_bytes, tangent_bytes):
    binormal = tuple(snorm16(value) for value in struct.unpack("<3h", binormal_bytes[:6]))
    tangent = tuple(snorm16(value) for value in struct.unpack("<3h", tangent_bytes[:6]))
    normal = normalize3(cross3(tangent, binormal))
    # The known-good 2K26 exporter stores the octahedral normal in the low
    # 20 bits and leaves the optional tangent-angle/handedness bits at zero.
    return oct_encode_normal(normal)


def read_scne(zip_file, entry_name):
    text = zip_file.read(entry_name).decode("utf-8-sig")
    fixed = re.sub(r":\s*(?=(?:,|\r?\n\s*[},]))", ": null", "{" + text + "}")
    return json.loads(fixed, object_pairs_hook=OrderedDict), text


def find_segments(data):
    segments = []
    offset = 0
    while offset + 8 <= len(data):
        magic = struct.unpack_from("<I", data, offset)[0]
        if magic != SEGMENT_MAGIC:
            offset += 1
            continue
        size = struct.unpack_from("<I", data, offset + 4)[0]
        if size < 24 or offset + size > len(data):
            offset += 1
            continue
        payload = data[offset + 24 : offset + size]
        segments.append((offset, size, payload))
        offset += size
    return segments


def crc32_unsigned(data):
    import zlib

    return zlib.crc32(data) & 0xFFFFFFFF


def hash_name(prefix, data, suffix):
    return f"{prefix}.{hashlib.sha1(data).hexdigest()[:16]}{suffix}"


def renamed_zip_info(info, filename, timestamp=None):
    cloned = copy.copy(info)
    cloned.filename = filename
    cloned.orig_filename = filename
    cloned.compress_type = zipfile.ZIP_DEFLATED
    cloned.create_system = 3
    cloned.create_version = 63
    cloned.extract_version = 20
    cloned.external_attr = 0x81B60000
    if timestamp is not None:
        cloned.date_time = timestamp
    return cloned


def native_zip_info(filename):
    info = zipfile.ZipInfo(filename)
    info.compress_type = zipfile.ZIP_DEFLATED
    info.create_system = 3
    info.create_version = 63
    info.extract_version = 20
    info.external_attr = 0x81B60000
    return info


def render_scne(scene):
    rendered = json.dumps(scene, indent="\t", ensure_ascii=True)
    lines = rendered.splitlines()[1:-1]
    return "\n".join(line[1:] if line.startswith("\t") else line for line in lines) + "\n"


def convert(
    input_path,
    output_path,
    force_bone=None,
    target_name=None,
    target_scne_name=None,
    duplicate_lods=False,
    template_path=None,
    keep_template_duv=False,
    preserve_template_vertex_metadata=False,
    fit_template_vertex_count=False,
    use_source_uv_metadata=False,
):
    template_vertex_count = None
    if fit_template_vertex_count:
        if not template_path:
            raise ValueError("Template vertex fitting requires a 2K26 template IFF.")
        with zipfile.ZipFile(template_path, "r") as ztemplate:
            template_scne_name = next(name for name in ztemplate.namelist() if name.lower().endswith(".scne"))
            template_text = ztemplate.read(template_scne_name).decode("utf-8-sig")
            template_scene = json.loads("{" + template_text + "}", object_pairs_hook=OrderedDict)
            template_model = next(iter(template_scene.values()))["Model"]["hihead"]
            template_lods = template_model.get("Lods") or []
            if template_lods:
                template_vertex_count = int(template_lods[0]["LodVerts"])
            else:
                template_streams = template_model.get("VertexStream") or []
                stream_counts = [
                    int(stream["Size"]) // int(stream["Stride"])
                    for stream in template_streams
                    if int(stream.get("Stride", 0)) > 0
                ]
                if not stream_counts or len(set(stream_counts)) != 1:
                    raise ValueError("The static 2K26 template has inconsistent vertex streams.")
                template_vertex_count = stream_counts[0]

    with zipfile.ZipFile(input_path, "r") as zin:
        scne_name = next(name for name in zin.namelist() if name.lower().endswith(".scne"))
        scne, _ = read_scne(zin, scne_name)
        source_root_name = next(iter(scne))
        root_name = target_name or source_root_name
        root = scne[source_root_name]
        model = root["Model"]
        selected = None
        fallback_selected = None
        for model_name, candidate in model.items():
            if not isinstance(candidate, dict) or "Binary" not in candidate or not candidate.get("Prim"):
                continue
            candidate_data = zin.read(candidate["Binary"])
            candidate_segments = find_segments(candidate_data)
            if len(candidate_segments) < 8:
                continue
            candidate_primitive = next(iter(candidate["Prim"].values()))
            candidate_index_count = int(candidate_primitive["Count"])
            candidate_index_data = candidate_segments[0][2][: candidate_index_count * 2]
            candidate_indices = struct.unpack("<" + "H" * candidate_index_count, candidate_index_data)
            candidate_vertex_count = max(candidate_indices) + 1
            candidate_selection = (
                model_name,
                candidate,
                candidate_segments,
                candidate_index_count,
                candidate_index_data,
                candidate_indices,
                candidate_vertex_count,
            )
            if fallback_selected is None or candidate_vertex_count < fallback_selected[-1]:
                fallback_selected = candidate_selection
            if template_vertex_count is None or candidate_vertex_count <= template_vertex_count:
                selected = candidate_selection
                break
        expanded_template = False
        if selected is None and fallback_selected is not None and template_vertex_count is not None:
            selected = fallback_selected
            expanded_template = selected[-1] > template_vertex_count
        if selected is None:
            target_text = f" under {template_vertex_count} vertices" if template_vertex_count else ""
            raise RuntimeError(f"No supported legacy hair LOD was found{target_text}.")

    (
        lod0_name,
        lod0,
        segments,
        index_count,
        index_buffer,
        indices,
        vertex_count,
    ) = selected
    if expanded_template:
        preserve_template_vertex_metadata = False

    index_data = segments[0][2]
    pos_data = segments[1][2]
    binormal_data = segments[2][2]
    tangent_data = segments[3][2]
    tex0_data = segments[4][2]
    tex1_data = segments[5][2]
    blend_index_data = segments[6][2]
    blend_weight_data = segments[7][2]

    old_position_format = lod0["VertexFormat"]["POSITION0"]
    pos_offset = old_position_format["Offset"]
    pos_scale = old_position_format["Scale"]

    vertex_stream0 = bytearray()
    mins = [float("inf"), float("inf"), float("inf")]
    maxs = [float("-inf"), float("-inf"), float("-inf")]
    for i in range(vertex_count):
        x, y, z, _w = struct.unpack_from("<4h", pos_data, i * 8)
        converted = [
            pos_offset[0] + snorm16(x) * pos_scale[0],
            pos_offset[1] + snorm16(y) * pos_scale[1],
            pos_offset[2] + snorm16(z) * pos_scale[2],
        ]
        for axis in range(3):
            mins[axis] = min(mins[axis], converted[axis])
            maxs[axis] = max(maxs[axis], converted[axis])
        vertex_stream0.extend(struct.pack("<3f", *converted))

    matrix_weights = bytearray()
    vertex_stream1 = bytearray()
    used_bones = []
    for i in range(vertex_count):
        tangent_frame = pack_tangent_frame(
            binormal_data[i * 8 : i * 8 + 8],
            tangent_data[i * 8 : i * 8 + 8],
        )
        tex0 = tex0_data[i * 4 : i * 4 + 4]
        tex1 = tex1_data[i * 4 : i * 4 + 4]

        if force_bone is not None:
            weight_data = (force_bone << 8) | 0
            used_bones.append(force_bone)
        else:
            raw_indices = list(blend_index_data[i * 4 : i * 4 + 4])
            raw_weights = list(blend_weight_data[i * 4 : i * 4 + 4])
            influences = [(bone, weight) for bone, weight in zip(raw_indices, raw_weights) if weight > 0]
            if not influences:
                influences = [(0, 255)]
            total = sum(weight for _bone, weight in influences)
            normalized = []
            for bone, weight in influences:
                weight16 = int(round(weight / total * 65535.0)) if total else 65535
                normalized.append((bone, max(0, min(65535, weight16))))
                used_bones.append(bone)
            if len(normalized) == 1:
                weight_data = (normalized[0][0] << 8) | 0
            else:
                start = len(matrix_weights) // 4
                for bone, weight16 in normalized:
                    matrix_weights.extend(struct.pack("<I", weight16 | (bone << 16)))
                weight_data = (start << 8) | (len(normalized) - 1)

        vertex_stream1.extend(struct.pack("<I", tangent_frame))
        vertex_stream1.extend(tex0)
        vertex_stream1.extend(tex1)
        vertex_stream1.extend(struct.pack("<I", weight_data))

    source_vertex_count = vertex_count
    if template_vertex_count is not None and vertex_count < template_vertex_count:
        vertex_stream0.extend(vertex_stream0[-12:] * (template_vertex_count - vertex_count))
        vertex_stream1.extend(vertex_stream1[-16:] * (template_vertex_count - vertex_count))
        vertex_count = template_vertex_count

    center = [(mins[i] + maxs[i]) / 2.0 for i in range(3)]
    radius = max(
        ((x - center[0]) ** 2 + (y - center[1]) ** 2 + (z - center[2]) ** 2) ** 0.5
        for x, y, z in struct.iter_unpack("<3f", vertex_stream0)
    )

    shader_name = f"{root_name}_shader" if target_name else next(iter(root["Material"]))
    mesh_name = f"{root_name}Shape" if target_name else next(iter(lod0["Prim"].values()))["Mesh"]
    blend_min = min(used_bones) if used_bones else 0
    blend_max = max(used_bones) if used_bones else 0
    model_blend_min = 0 if target_name or force_bone == 48 else blend_min
    model_blend_max = max(48, blend_max) if target_name else blend_max
    prim_blend_min = 48 if force_bone == 48 else blend_min
    prim_blend_max = 48 if force_bone == 48 else blend_max

    if duplicate_lods:
        lod_pixels = [None, 750.0, 300.0, 200.0, 100.0, 50.0]
        lods = []
        lod_list = []
        combined_index_buffer = bytearray()
        for lod_pixel in reversed(lod_pixels):
            start = len(combined_index_buffer) // 2
            combined_index_buffer.extend(index_buffer)
            lod_list.insert(0, OrderedDict([("Start", start), ("Count", index_count)]))
        for lod_pixel in lod_pixels:
            item = OrderedDict([("LodSize", lod0.get("LodSize", 200.0))])
            if lod_pixel is not None:
                item["LodPixelSize"] = lod_pixel
            item["LodVerts"] = vertex_count
            lods.append(item)
        index_buffer = bytes(combined_index_buffer)
    else:
        lods = [OrderedDict([("LodSize", lod0.get("LodSize", 200.0)), ("LodVerts", vertex_count)])]
        lod_list = [OrderedDict([("Start", 0), ("Count", index_count)])]

    vb0_name = hash_name("VertexBuffer", vertex_stream0, ".bin")
    vb1_name = hash_name("VertexBuffer", vertex_stream1, ".bin")
    ib_name = hash_name("IndexBuffer", index_buffer, ".bin")
    mw_name = hash_name("MatrixWeightsBuffer", matrix_weights or b"\0\0\0\0", ".bin")
    out_scne_name = target_scne_name or (f"{root_name}.SCNE" if target_name else scne_name)

    tex0_fmt = lod0["VertexFormat"]["TEXCOORD0"]
    tex1_fmt = lod0["VertexFormat"]["TEXCOORD1"]
    duv0 = [
        abs(tex0_fmt.get("Scale", [0.0, 0.0])[0] - tex0_fmt.get("Offset", [0.0, 0.0])[0]),
        abs(tex0_fmt.get("Scale", [0.0, 0.0])[1] - tex0_fmt.get("Offset", [0.0, 0.0])[1]),
    ]
    duv1 = [
        abs(tex1_fmt.get("Scale", [0.0, 0.0])[0] - tex1_fmt.get("Offset", [0.0, 0.0])[0]),
        abs(tex1_fmt.get("Scale", [0.0, 0.0])[1] - tex1_fmt.get("Offset", [0.0, 0.0])[1]),
    ]

    template_replaced_names = set()
    template_member_roles = {}
    template_timestamp = None
    if template_path:
        with zipfile.ZipFile(template_path, "r") as ztemplate:
            template_scne_name = next(name for name in ztemplate.namelist() if name.lower().endswith(".scne"))
            template_text = ztemplate.read(template_scne_name).decode("utf-8-sig")
            out_doc = json.loads("{" + template_text + "}", object_pairs_hook=OrderedDict)
            template_names = set(ztemplate.namelist())
            template_timestamp = ztemplate.getinfo(template_scne_name).date_time
        template_root_name = next(iter(out_doc))
        if target_name and target_name != template_root_name:
            out_doc = OrderedDict([(target_name, out_doc[template_root_name])])
        out_root = out_doc[next(iter(out_doc))]
        node = out_root["Model"]["hihead"]

        def actual_template_member(binary_name):
            candidates = (binary_name, binary_name.replace(".gz", ".bin"))
            return next((name for name in candidates if name in template_names), binary_name)

        template_member_roles = {
            "scne": template_scne_name,
            "index": actual_template_member(node["IndexBuffer"]["Binary"]),
            "matrix": actual_template_member(node["MatrixWeightsBuffer"]["Binary"]),
            "vertex0": actual_template_member(node["VertexStream"][0]["Binary"]),
            "vertex1": actual_template_member(node["VertexStream"][1]["Binary"]),
        }
        template_replaced_names.update(
            {
                template_scne_name,
                node["IndexBuffer"]["Binary"],
                node["IndexBuffer"]["Binary"].replace(".gz", ".bin"),
                node["MatrixWeightsBuffer"]["Binary"],
                node["MatrixWeightsBuffer"]["Binary"].replace(".gz", ".bin"),
            }
        )
        for stream in node["VertexStream"]:
            template_replaced_names.add(stream["Binary"])
            template_replaced_names.add(stream["Binary"].replace(".gz", ".bin"))
        prim = node["Prim"][0]
        template_materials = out_root.get("Material", {})
        if template_materials:
            shader_name = next(iter(template_materials))
        else:
            out_root["Material"] = OrderedDict([(shader_name, None)])
        mesh_name = prim.get("Mesh", mesh_name)
        if keep_template_duv and not preserve_template_vertex_metadata:
            duv0 = node.get("Duv0", duv0)
            duv1 = node.get("Duv1", duv1)
        node["LodSize"] = lod0.get("LodSize", 200.0)
        if not preserve_template_vertex_metadata:
            node["Radius"] = radius
            node["Center"] = center
            node["Min"] = mins
            node["Max"] = maxs
        node["Lods"] = lods
        if not preserve_template_vertex_metadata:
            node["Duv0"] = duv0
            node["Duv1"] = duv1
        node["WeightBits"] = 16
        node["BlendIndexRange"] = [model_blend_min, model_blend_max]
        prim["Material"] = shader_name
        prim["Mesh"] = mesh_name
        prim["Type"] = "TRIANGLE_LIST"
        prim["BlendIndexRange"] = [prim_blend_min, prim_blend_max]
        prim["Start"] = lod_list[0]["Start"]
        prim["Count"] = lod_list[0]["Count"]
        if not preserve_template_vertex_metadata:
            prim["Radius"] = radius
            prim["Center"] = center
            prim["Min"] = mins
            prim["Max"] = maxs
        if not preserve_template_vertex_metadata:
            prim["Duv0"] = duv0
            prim["Duv1"] = duv1
        prim["LodList"] = lod_list
        if not preserve_template_vertex_metadata:
            node["VertexFormat"] = OrderedDict(
                [
                    ("POSITION0", OrderedDict([("Format", "R32G32B32_FLOAT"), ("CpuAccess", "READWRITE")])),
                    ("TANGENTFRAME0", OrderedDict([("Format", "R10G10B10A2_UINT"), ("Stream", 1)])),
                    (
                        "TEXCOORD0",
                        OrderedDict(
                            [
                                ("Format", "R16G16_SNORM"),
                                ("ByteOffset", 4),
                                ("Stream", 1),
                                ("Offset", tex0_fmt.get("Offset", [0.0, 0.0, 0.0, 0.0])),
                                ("Scale", tex0_fmt.get("Scale", [1.0, 1.0, 1.0, 1.0])),
                            ]
                        ),
                    ),
                    (
                        "TEXCOORD1",
                        OrderedDict(
                            [
                                ("Format", "R16G16_SNORM"),
                                ("ByteOffset", 8),
                                ("Stream", 1),
                                ("Offset", tex1_fmt.get("Offset", [0.0, 0.0, 0.0, 0.0])),
                                ("Scale", tex1_fmt.get("Scale", [1.0, 1.0, 1.0, 1.0])),
                            ]
                        ),
                    ),
                    ("WEIGHTDATA0", OrderedDict([("Format", "R32_UINT"), ("ByteOffset", 12), ("Stream", 1)])),
                ]
            )
        if use_source_uv_metadata:
            for channel, source_format in (("TEXCOORD0", tex0_fmt), ("TEXCOORD1", tex1_fmt)):
                target_format = node["VertexFormat"].get(channel, {})
                for field in ("Offset", "Scale"):
                    if field in source_format:
                        target_format[field] = source_format[field]
        node["IndexBuffer"] = OrderedDict(
            [
                ("Format", "R16_UINT"),
                ("CpuAccess", "READONLY"),
                ("CompressionMethod", 33),
                ("Size", len(index_buffer)),
                ("Binary", ib_name.replace(".bin", ".gz")),
            ]
        )
        node["IndexBufferCrc32"] = crc32_unsigned(index_buffer)
        node["MatrixWeightsBuffer"] = OrderedDict(
            [
                ("Format", "R32_UINT"),
                ("Dimension", "BYTEADDRESSBUFFER"),
                ("Size", len(matrix_weights or b"\0\0\0\0")),
                ("Binary", mw_name),
            ]
        )
        node["VertexStream"] = [
            OrderedDict(
                [
                    ("CpuAccess", "READWRITE"),
                    ("Stride", 12),
                    ("CompressionMethod", 33),
                    ("Size", len(vertex_stream0)),
                    ("Binary", vb0_name.replace(".bin", ".gz")),
                ]
            ),
            OrderedDict(
                [
                    ("Stride", 16),
                    ("CompressionMethod", 33),
                    ("Size", len(vertex_stream1)),
                    ("Binary", vb1_name.replace(".bin", ".gz")),
                ]
            ),
        ]
        root_name = next(iter(out_doc))
    else:
        morph = lod0.get("Morph", OrderedDict())
        out_root = OrderedDict(
            [
                ("EndTime", root.get("EndTime", 2.0)),
                ("Material", OrderedDict([(shader_name, None)])),
                (
                    "Model",
                    OrderedDict(
                        [
                            (
                                "hihead",
                                OrderedDict(
                                    [
                                        ("LodSize", lod0.get("LodSize", 200.0)),
                                        ("Radius", radius),
                                        ("Center", center),
                                        ("Min", mins),
                                        ("Max", maxs),
                                        ("Lods", lods),
                                        ("Duv0", duv0),
                                        ("Duv1", duv1),
                                        ("WeightBits", 16),
                                        ("BlendIndexRange", [model_blend_min, model_blend_max]),
                                        (
                                            "Prim",
                                            [
                                                OrderedDict(
                                                    [
                                                        ("Material", shader_name),
                                                        ("Mesh", mesh_name),
                                                        ("Type", "TRIANGLE_LIST"),
                                                        ("BlendIndexRange", [prim_blend_min, prim_blend_max]),
                                                        ("Start", lod_list[0]["Start"]),
                                                        ("Count", lod_list[0]["Count"]),
                                                        ("Radius", radius),
                                                        ("Center", center),
                                                        ("Min", mins),
                                                        ("Max", maxs),
                                                        ("Duv0", duv0),
                                                        ("Duv1", duv1),
                                                        ("LodList", lod_list),
                                                    ]
                                                )
                                            ],
                                        ),
                                        (
                                            "VertexFormat",
                                            OrderedDict(
                                                [
                                                    ("POSITION0", OrderedDict([("Format", "R32G32B32_FLOAT"), ("CpuAccess", "READWRITE")])),
                                                    ("TANGENTFRAME0", OrderedDict([("Format", "R10G10B10A2_UINT"), ("Stream", 1)])),
                                                    (
                                                        "TEXCOORD0",
                                                        OrderedDict(
                                                            [
                                                                ("Format", "R16G16_SNORM"),
                                                                ("ByteOffset", 4),
                                                                ("Stream", 1),
                                                                ("Offset", tex0_fmt.get("Offset", [0.0, 0.0, 0.0, 0.0])),
                                                                ("Scale", tex0_fmt.get("Scale", [1.0, 1.0, 1.0, 1.0])),
                                                            ]
                                                        ),
                                                    ),
                                                    (
                                                        "TEXCOORD1",
                                                        OrderedDict(
                                                            [
                                                                ("Format", "R16G16_SNORM"),
                                                                ("ByteOffset", 8),
                                                                ("Stream", 1),
                                                                ("Offset", tex1_fmt.get("Offset", [0.0, 0.0, 0.0, 0.0])),
                                                                ("Scale", tex1_fmt.get("Scale", [1.0, 1.0, 1.0, 1.0])),
                                                            ]
                                                        ),
                                                    ),
                                                    ("WEIGHTDATA0", OrderedDict([("Format", "R32_UINT"), ("ByteOffset", 12), ("Stream", 1)])),
                                                ]
                                            ),
                                        ),
                                        (
                                            "IndexBuffer",
                                            OrderedDict(
                                                [
                                                    ("Format", "R16_UINT"),
                                                    ("CpuAccess", "READONLY"),
                                                    ("CompressionMethod", 33),
                                                    ("Size", len(index_buffer)),
                                                    ("Binary", ib_name.replace(".bin", ".gz")),
                                                ]
                                            ),
                                        ),
                                        ("IndexBufferCrc32", crc32_unsigned(index_buffer)),
                                        (
                                            "MatrixWeightsBuffer",
                                            OrderedDict(
                                                [
                                                    ("Format", "R32_UINT"),
                                                    ("Dimension", "BYTEADDRESSBUFFER"),
                                                    ("Size", len(matrix_weights or b"\0\0\0\0")),
                                                    ("Binary", mw_name),
                                                ]
                                            ),
                                        ),
                                        (
                                            "VertexStream",
                                            [
                                                OrderedDict(
                                                    [
                                                        ("CpuAccess", "READWRITE"),
                                                        ("Stride", 12),
                                                        ("CompressionMethod", 33),
                                                        ("Size", len(vertex_stream0)),
                                                        ("Binary", vb0_name.replace(".bin", ".gz")),
                                                    ]
                                                ),
                                                OrderedDict(
                                                    [
                                                        ("Stride", 16),
                                                        ("CompressionMethod", 33),
                                                        ("Size", len(vertex_stream1)),
                                                        ("Binary", vb1_name.replace(".bin", ".gz")),
                                                    ]
                                                ),
                                            ],
                                        ),
                                        ("Morph", morph),
                                    ]
                                ),
                            )
                        ]
                    ),
                ),
                ("Object", OrderedDict([("hihead", OrderedDict([("Type", "OBJECT"), ("Target", "hihead"), ("Transform", "def_hips")]))])),
            ]
        )

    scne_text = render_scne(OrderedDict([(root_name, out_root)]))

    if template_path:
        replacement_members = {
            template_member_roles["scne"]: (out_scne_name, scne_text.encode("utf-8")),
            template_member_roles["index"]: (ib_name, index_buffer),
            template_member_roles["matrix"]: (mw_name, matrix_weights or b"\0\0\0\0"),
            template_member_roles["vertex0"]: (vb0_name, vertex_stream0),
            template_member_roles["vertex1"]: (vb1_name, vertex_stream1),
        }
        with zipfile.ZipFile(template_path, "r") as ztemplate, zipfile.ZipFile(
            output_path, "w", compression=zipfile.ZIP_DEFLATED
        ) as zout:
            template_infos = {info.filename: info for info in ztemplate.infolist()}
            for info in ztemplate.infolist():
                if info.filename not in template_replaced_names:
                    zout.writestr(
                        renamed_zip_info(info, info.filename, template_timestamp),
                        ztemplate.read(info.filename),
                    )
            for role in ("index", "scne", "vertex1", "vertex0", "matrix"):
                old_name = template_member_roles[role]
                name, data = replacement_members[old_name]
                zout.writestr(
                    renamed_zip_info(template_infos[old_name], name, template_timestamp),
                    data,
                )
    else:
        with zipfile.ZipFile(output_path, "w", compression=zipfile.ZIP_DEFLATED) as zout:
            for name, data in (
                (vb0_name, vertex_stream0),
                (vb1_name, vertex_stream1),
                (mw_name, matrix_weights or b"\0\0\0\0"),
                (ib_name, index_buffer),
                (out_scne_name, scne_text.encode("utf-8")),
            ):
                zout.writestr(native_zip_info(name), data)

    return {
        "output": str(output_path),
        "vertices": source_vertex_count,
        "stream_vertices": vertex_count,
        "source_lod": lod0_name,
        "indices": index_count,
        "expanded_template": expanded_template,
        "template_vertices": template_vertex_count,
        "blend_range": [blend_min, blend_max],
        "matrix_weight_bytes": len(matrix_weights or b"\0\0\0\0"),
    }


def main():
    parser = argparse.ArgumentParser(description="Build a first-pass 2K26-style LOD0 hair IFF from a 2K23 hair IFF.")
    parser.add_argument("input")
    parser.add_argument("output")
    parser.add_argument("--force-bone", type=int, default=None)
    parser.add_argument("--target-name", default=None)
    parser.add_argument("--target-scne-name", default=None)
    parser.add_argument("--duplicate-lods", action="store_true")
    parser.add_argument("--template", default=None)
    parser.add_argument("--keep-template-duv", action="store_true")
    parser.add_argument("--fit-template-vertex-count", action="store_true")
    parser.add_argument("--use-source-uv-metadata", action="store_true")
    args = parser.parse_args()
    result = convert(
        Path(args.input),
        Path(args.output),
        args.force_bone,
        args.target_name,
        args.target_scne_name,
        args.duplicate_lods,
        Path(args.template) if args.template else None,
        args.keep_template_duv,
        False,
        args.fit_template_vertex_count,
        args.use_source_uv_metadata,
    )
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
