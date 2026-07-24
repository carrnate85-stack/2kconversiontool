import argparse
import copy
import json
import math
import re
import struct
import zipfile
from collections import OrderedDict
from pathlib import Path

from convert_2k23_hair_to_2k26_lod0 import (
    crc32_unsigned,
    hash_name,
    renamed_zip_info,
    render_scne,
)


def preserve_duplicate_keys(pairs):
    result = OrderedDict()
    for key, value in pairs:
        if key not in result:
            result[key] = value
        elif isinstance(result[key], list):
            result[key].append(value)
        else:
            result[key] = [result[key], value]
    return result


def read_scne(archive, member, preserve_duplicates=False):
    text = archive.read(member).decode("utf-8-sig")
    fixed = re.sub(r":\s*(?=(?:,|\r?\n\s*[},]))", ": null", "{" + text + "}")
    hook = preserve_duplicate_keys if preserve_duplicates else OrderedDict
    return json.loads(fixed, object_pairs_hook=hook)


def actual_member(archive, binary_name):
    candidates = (
        binary_name,
        binary_name.replace(".gz", ".bin"),
        binary_name.replace(".bin", ".gz"),
    )
    return next((name for name in candidates if name in archive.namelist()), None)


def stream_descriptors(model):
    streams = model.get("VertexStream")
    if isinstance(streams, list):
        return streams
    if isinstance(streams, dict):
        streams = streams.get("VertexBuffer", streams)
    return streams if isinstance(streams, list) else [streams]


def calculate_bounds(position_data):
    positions = list(struct.iter_unpack("<3f", position_data))
    if not positions:
        raise ValueError("The 2K25 position stream is empty.")
    mins = [min(point[axis] for point in positions) for axis in range(3)]
    maxs = [max(point[axis] for point in positions) for axis in range(3)]
    center = [(mins[axis] + maxs[axis]) / 2.0 for axis in range(3)]
    radius = max(
        math.sqrt(sum((point[axis] - center[axis]) ** 2 for axis in range(3)))
        for point in positions
    )
    return mins, maxs, center, radius


def convert(source_path, template_path, output_path, force_bone=48):
    source_path = Path(source_path)
    template_path = Path(template_path)
    output_path = Path(output_path)
    output_path.parent.mkdir(parents=True, exist_ok=True)

    with zipfile.ZipFile(source_path, "r") as source:
        source_scne_member = next(
            name for name in source.namelist() if name.lower().endswith(".scne")
        )
        source_scene = read_scne(source, source_scne_member, preserve_duplicates=True)
        source_root = next(iter(source_scene.values()))
        source_model = source_root["Model"]["hihead"]
        source_prim = source_model["Prim"][0]

        source_streams = stream_descriptors(source_model)
        streams_by_stride = {
            int(stream["Stride"]): stream
            for stream in source_streams
            if isinstance(stream, dict) and "Stride" in stream
        }
        if 12 not in streams_by_stride or 16 not in streams_by_stride:
            raise ValueError("Expected 2K25 position (12-byte) and attribute (16-byte) streams.")

        position_member = actual_member(source, streams_by_stride[12]["Binary"])
        attribute_member = actual_member(source, streams_by_stride[16]["Binary"])
        index_member = actual_member(source, source_model["IndexBuffer"]["Binary"])
        if not position_member or not attribute_member or not index_member:
            raise ValueError("One or more 2K25 geometry buffers are missing.")

        position_data = source.read(position_member)
        attribute_data = bytearray(source.read(attribute_member))
        full_index_data = source.read(index_member)
        if len(position_data) % 12 or len(attribute_data) % 16:
            raise ValueError("The 2K25 vertex stream sizes are invalid.")
        vertex_count = len(position_data) // 12
        if len(attribute_data) // 16 != vertex_count:
            raise ValueError("The 2K25 vertex streams have different vertex counts.")

        lod_list = source_prim.get("LodList")
        if lod_list:
            lod = lod_list[0]
            index_start = int(lod["Start"])
            index_count = int(lod["Count"])
        else:
            index_start = int(source_prim.get("Start", 0))
            index_count = int(source_prim["Count"])
        index_data = full_index_data[index_start * 2 : (index_start + index_count) * 2]
        if len(index_data) != index_count * 2:
            raise ValueError("The selected 2K25 LOD index data is truncated.")
        indices = struct.unpack(f"<{index_count}H", index_data)
        if max(indices, default=-1) >= vertex_count:
            raise ValueError("The selected 2K25 LOD references a missing vertex.")

        static_weight = force_bone << 8
        for vertex_index in range(vertex_count):
            struct.pack_into("<I", attribute_data, vertex_index * 16 + 12, static_weight)
        attribute_data = bytes(attribute_data)
        matrix_data = b"\0\0\0\0"
        mins, maxs, center, radius = calculate_bounds(position_data)

    with zipfile.ZipFile(template_path, "r") as template:
        template_scne_member = next(
            name for name in template.namelist() if name.lower().endswith(".scne")
        )
        output_scene = read_scne(template, template_scne_member)
        output_root = next(iter(output_scene.values()))
        output_model = output_root["Model"]["hihead"]
        output_prim = output_model["Prim"][0]
        template_streams = stream_descriptors(output_model)
        if len(template_streams) != 2:
            raise ValueError("Expected two vertex streams in the 2K26 template.")

        template_roles = {
            "scne": template_scne_member,
            "index": actual_member(template, output_model["IndexBuffer"]["Binary"]),
            "matrix": actual_member(template, output_model["MatrixWeightsBuffer"]["Binary"]),
            "position": actual_member(template, template_streams[0]["Binary"]),
            "attribute": actual_member(template, template_streams[1]["Binary"]),
        }
        if any(member is None for member in template_roles.values()):
            raise ValueError("One or more 2K26 template members are missing.")

        position_name = hash_name("VertexBuffer", position_data, ".bin")
        attribute_name = hash_name("VertexBuffer", attribute_data, ".bin")
        index_name = hash_name("IndexBuffer", index_data, ".bin")
        matrix_name = hash_name("MatrixWeightsBuffer", matrix_data, ".bin")

        for node in (output_model, output_prim):
            node["Radius"] = radius
            node["Center"] = center
            node["Min"] = mins
            node["Max"] = maxs
        for field in ("Duv0", "Duv1"):
            if field in source_model:
                output_model[field] = copy.deepcopy(source_model[field])
            if field in source_prim:
                output_prim[field] = copy.deepcopy(source_prim[field])
        output_model["WeightBits"] = 16
        output_model["BlendIndexRange"] = [0, force_bone]
        output_prim["BlendIndexRange"] = [force_bone, force_bone]
        output_prim["Count"] = index_count
        output_model["VertexFormat"] = copy.deepcopy(source_model["VertexFormat"])

        if "Lods" in output_model:
            output_model["Lods"] = [
                OrderedDict(
                    [
                        ("LodSize", float(source_model.get("LodSize", 200.0))),
                        ("LodVerts", vertex_count),
                    ]
                )
            ]
            output_prim["Start"] = 0
            output_prim["LodList"] = [OrderedDict([("Start", 0), ("Count", index_count)])]
        else:
            output_prim.pop("Start", None)
            output_prim.pop("LodList", None)

        output_model["IndexBuffer"] = OrderedDict(
            [
                ("Format", "R16_UINT"),
                ("CpuAccess", "READONLY"),
                ("Size", len(index_data)),
                ("Binary", index_name),
            ]
        )
        output_model["IndexBufferCrc32"] = crc32_unsigned(index_data)
        output_model["MatrixWeightsBuffer"] = OrderedDict(
            [
                ("Format", "R32_UINT"),
                ("Dimension", "BYTEADDRESSBUFFER"),
                ("Size", len(matrix_data)),
                ("Binary", matrix_name),
            ]
        )
        output_model["VertexStream"] = [
            OrderedDict(
                [
                    ("CpuAccess", "READWRITE"),
                    ("Stride", 12),
                    ("Size", len(position_data)),
                    ("Binary", position_name),
                ]
            ),
            OrderedDict(
                [
                    ("Stride", 16),
                    ("Size", len(attribute_data)),
                    ("Binary", attribute_name),
                ]
            ),
        ]

        replacements = {
            template_roles["scne"]: (
                template_scne_member,
                render_scne(output_scene).encode("utf-8"),
            ),
            template_roles["index"]: (index_name, index_data),
            template_roles["matrix"]: (matrix_name, matrix_data),
            template_roles["position"]: (position_name, position_data),
            template_roles["attribute"]: (attribute_name, attribute_data),
        }
        timestamp = template.getinfo(template_scne_member).date_time
        temp_path = output_path.with_name(output_path.name + ".tmp")
        try:
            with zipfile.ZipFile(temp_path, "w", compression=zipfile.ZIP_DEFLATED) as output:
                for info in template.infolist():
                    name, data = replacements.get(
                        info.filename,
                        (info.filename, template.read(info.filename)),
                    )
                    output.writestr(renamed_zip_info(info, name, timestamp), data)
            with zipfile.ZipFile(temp_path, "r") as check:
                bad_member = check.testzip()
                if bad_member:
                    raise RuntimeError(f"Converted ZIP failed validation at {bad_member}.")
            temp_path.replace(output_path)
        except Exception:
            temp_path.unlink(missing_ok=True)
            raise

    return {
        "output": str(output_path),
        "vertices": vertex_count,
        "indices": index_count,
        "bone": force_bone,
        "source_lod": 0,
    }


def main():
    parser = argparse.ArgumentParser(
        description="Convert a modern 2K25 hair mesh to a static, single-LOD 2K26 hair."
    )
    parser.add_argument("source")
    parser.add_argument("template")
    parser.add_argument("output")
    parser.add_argument("--force-bone", type=int, default=48)
    args = parser.parse_args()
    result = convert(args.source, args.template, args.output, args.force_bone)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
