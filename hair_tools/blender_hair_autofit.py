import io
import json
import math
import re
import shutil
import struct
import sys
import zipfile
from datetime import datetime
from pathlib import Path

import bpy
from bpy.props import StringProperty
from bpy.types import Operator, Panel
from bpy_extras.io_utils import ExportHelper
from mathutils import Vector
from mathutils.geometry import barycentric_transform, closest_point_on_tri
from mathutils.kdtree import KDTree

SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

from blender_iff_mesh import blender_to_game, import_iff_mesh, vertex_streams


SAFE_TEMPLATE_BYTES = None
SAFE_EXPORT_DEFAULT = ""
SAFE_TARGET_KEY = "hair"
ACTIVE_FIT_MODE = "baseline"


def command_line_values():
    if "--" not in sys.argv:
        raise RuntimeError("No Hair Fixer paths were passed to Blender.")
    arguments = sys.argv[sys.argv.index("--") + 1 :]
    if len(arguments) < 3:
        raise RuntimeError("Hair Fixer requires hair, donor head, and target head paths.")
    hair_path = Path(arguments[0]).resolve()
    donor_head_path = Path(arguments[1]).resolve()
    target_head_path = Path(arguments[2]).resolve()
    export_path = Path(arguments[3]).resolve() if len(arguments) > 3 and arguments[3] else None
    target_key = arguments[4] if len(arguments) > 4 and arguments[4] else "hair"
    fit_mode = arguments[5].lower() if len(arguments) > 5 else "baseline"
    if fit_mode not in {"baseline", "tangent", "headband"}:
        raise RuntimeError(f"Unknown Hair Fixer mode: {fit_mode}")
    return hair_path, donor_head_path, target_head_path, export_path, target_key, fit_mode


def scne_model(scne_data):
    text = scne_data.decode("utf-8-sig")
    scene = json.loads(text if text.lstrip().startswith("{") else "{" + text + "}")
    root = next(iter(scene.values()))
    models = root.get("Model", {})
    if not models:
        raise RuntimeError("The source SCNE has no model.")
    return next(iter(models.values())), text


def position_buffer_details(archive):
    scne_name = next((name for name in archive.namelist() if name.lower().endswith(".scne")), None)
    if not scne_name:
        raise RuntimeError("The source IFF has no SCNE file.")
    model, scne_text = scne_model(archive.read(scne_name))
    position = model.get("VertexFormat", {}).get("POSITION0", {})
    if position.get("Format") != "R32G32B32_FLOAT":
        raise RuntimeError(f"Safe export does not support position format {position.get('Format')}.")
    stream_index = int(position.get("Stream", 0))
    streams = vertex_streams(model, scne_text)
    if stream_index >= len(streams):
        raise RuntimeError("The source position stream is missing.")
    stream = streams[stream_index]
    stride = int(stream.get("Stride", 0))
    byte_offset = int(position.get("ByteOffset", 0))
    size = int(stream.get("Size", 0))
    if stride < byte_offset + 12 or size <= 0 or size % stride:
        raise RuntimeError("The source position stream layout is invalid.")
    binary = str(stream.get("Binary", ""))
    candidates = {binary.lower()}
    if binary.lower().endswith(".gz"):
        candidates.add((binary[:-3] + ".bin").lower())
    member = next((name for name in archive.namelist() if name.lower() in candidates), None)
    if not member:
        raise RuntimeError(f"The source position buffer {binary} is missing.")
    return scne_name, member, stride, byte_offset, size // stride


def headband_weight_buffer_details(archive):
    scne_name = next((name for name in archive.namelist() if name.lower().endswith(".scne")), None)
    if not scne_name:
        raise RuntimeError("The headband IFF has no SCNE file.")
    model, scne_text = scne_model(archive.read(scne_name))
    weight = model.get("VertexFormat", {}).get("WEIGHTDATA0", {})
    if weight.get("Format") != "R32_UINT":
        raise RuntimeError(f"Headband binding does not support weight format {weight.get('Format')}.")
    stream_index = int(weight.get("Stream", 0))
    streams = vertex_streams(model, scne_text)
    if stream_index >= len(streams):
        raise RuntimeError("The headband weight stream is missing.")
    stream = streams[stream_index]
    stride = int(stream.get("Stride", 0))
    byte_offset = int(weight.get("ByteOffset", 0))
    size = int(stream.get("Size", 0))
    if stride < byte_offset + 4 or size <= 0 or size % stride:
        raise RuntimeError("The headband weight stream layout is invalid.")
    binary = str(stream.get("Binary", ""))
    candidates = {binary.lower()}
    if binary.lower().endswith(".gz"):
        candidates.add((binary[:-3] + ".bin").lower())
    member = next((name for name in archive.namelist() if name.lower() in candidates), None)
    if not member:
        raise RuntimeError(f"The headband weight buffer {binary} is missing.")
    return scne_name, member, stride, byte_offset, size // stride


def bind_headband_scne_to_head(scne_data):
    text = scne_data.decode("utf-8-sig")
    model_pattern = re.compile(
        r'("BlendIndexRange"\s*:\s*\[\s*)0(\s*,\s*)(?:47|48)(\s*\])',
        re.S,
    )
    primitive_pattern = re.compile(
        r'("BlendIndexRange"\s*:\s*\[\s*)(?:47|48)(\s*,\s*)(?:47|48)(\s*\])',
        re.S,
    )
    text, model_count = model_pattern.subn(r"\g<1>0\g<2>48\g<3>", text, count=1)
    text, primitive_count = primitive_pattern.subn(r"\g<1>48\g<2>48\g<3>", text, count=1)
    if model_count != 1 or primitive_count != 1:
        raise RuntimeError("The headband SCNE bone ranges could not be rebound safely.")
    return text.encode("utf-8")


def selected_fitted_mesh():
    selected = [
        obj
        for obj in bpy.context.selected_objects
        if obj.type == "MESH" and (obj.get("nba2k_safe_hair") or obj.name.startswith("FITTED_"))
    ]
    if len(selected) != 1:
        raise RuntimeError("Select exactly one FITTED hair mesh before safe export.")
    return selected[0]


def write_safe_fitted_iff(filepath):
    if SAFE_TEMPLATE_BYTES is None:
        raise RuntimeError("The clean source IFF is no longer available. Reopen the hair through Hair Picker.")
    mesh = selected_fitted_mesh()
    with zipfile.ZipFile(io.BytesIO(SAFE_TEMPLATE_BYTES), "r") as source:
        scne_name, position_member, stride, byte_offset, expected_vertices = position_buffer_details(source)
        if len(mesh.data.vertices) != expected_vertices:
            raise RuntimeError(
                f"Vertex count changed: Blender has {len(mesh.data.vertices):,}, source requires "
                f"{expected_vertices:,}. Safe export was cancelled."
            )
        position_data = bytearray(source.read(position_member))
        required_size = expected_vertices * stride
        if len(position_data) < required_size:
            raise RuntimeError("The source position buffer is truncated.")
        matrix = mesh.matrix_world
        for index, vertex in enumerate(mesh.data.vertices):
            point = matrix @ vertex.co
            struct.pack_into(
                "<fff",
                position_data,
                index * stride + byte_offset,
                *blender_to_game(point),
            )

        weight_member = ""
        weight_data = None
        scne_data = source.read(scne_name)
        if ACTIVE_FIT_MODE == "headband":
            (
                weight_scne,
                weight_member,
                weight_stride,
                weight_offset,
                weight_vertices,
            ) = headband_weight_buffer_details(source)
            if weight_scne != scne_name or weight_vertices != expected_vertices:
                raise RuntimeError("The headband position and weight streams do not match.")
            weight_data = bytearray(source.read(weight_member))
            if len(weight_data) < weight_vertices * weight_stride:
                raise RuntimeError("The headband weight buffer is truncated.")
            for index in range(weight_vertices):
                struct.pack_into("<I", weight_data, index * weight_stride + weight_offset, 48 << 8)
            scne_data = bind_headband_scne_to_head(scne_data)

        output_path = Path(filepath)
        output_path.parent.mkdir(parents=True, exist_ok=True)
        if output_path.exists():
            stamp = datetime.now().strftime("%Y%m%d_%H%M%S")
            backup_dir = SCRIPT_DIR / "hair_picker_backups" / "blender_safe" / stamp
            backup_dir.mkdir(parents=True, exist_ok=True)
            shutil.copy2(output_path, backup_dir / output_path.name)

        temp_path = output_path.with_name(f"{output_path.name}.safe_tmp")
        try:
            with zipfile.ZipFile(temp_path, "w", compression=zipfile.ZIP_DEFLATED) as output:
                for info in source.infolist():
                    if info.filename == position_member:
                        data = bytes(position_data)
                    elif weight_data is not None and info.filename == weight_member:
                        data = bytes(weight_data)
                    elif info.filename == scne_name:
                        data = scne_data
                    else:
                        data = source.read(info.filename)
                    name = f"{SAFE_TARGET_KEY}.SCNE" if info.filename == scne_name else info.filename
                    replacement = zipfile.ZipInfo(name, info.date_time)
                    replacement.compress_type = zipfile.ZIP_DEFLATED
                    replacement.comment = info.comment
                    replacement.extra = info.extra
                    replacement.internal_attr = info.internal_attr
                    replacement.external_attr = info.external_attr
                    replacement.create_system = info.create_system
                    output.writestr(replacement, data)
            with zipfile.ZipFile(temp_path, "r") as check:
                check_scne, _buffer, _stride, _offset, check_vertices = position_buffer_details(check)
                if Path(check_scne).stem.lower() != SAFE_TARGET_KEY.lower() or check_vertices != expected_vertices:
                    raise RuntimeError("Safe export verification failed.")
                if ACTIVE_FIT_MODE == "headband":
                    (
                        _weight_scne,
                        check_weight_member,
                        check_weight_stride,
                        check_weight_offset,
                        check_weight_vertices,
                    ) = headband_weight_buffer_details(check)
                    check_weight_data = check.read(check_weight_member)
                    check_weights = {
                        struct.unpack_from(
                            "<I",
                            check_weight_data,
                            index * check_weight_stride + check_weight_offset,
                        )[0]
                        for index in range(check_weight_vertices)
                    }
                    check_scne_text = check.read(check_scne).decode("utf-8-sig")
                    if (
                        check_weight_vertices != expected_vertices
                        or check_weights != {48 << 8}
                        or not re.search(
                            r'"BlendIndexRange"\s*:\s*\[\s*48\s*,\s*48\s*\]',
                            check_scne_text,
                        )
                    ):
                        raise RuntimeError("Headband head-bone binding verification failed.")
                if check.testzip() is not None:
                    raise RuntimeError("Safe export ZIP verification failed.")
            temp_path.replace(output_path)
        except Exception:
            temp_path.unlink(missing_ok=True)
            raise
    return output_path


class NBA2K_OT_safe_hair_export(Operator, ExportHelper):
    bl_idname = "nba2k.safe_hair_export"
    bl_label = "NBA 2K Safe Fitted Hair"
    bl_description = "Preserve the original IFF and replace only its existing position buffer"
    filename_ext = ".iff"
    filter_glob: StringProperty(default="*.iff", options={"HIDDEN"})

    def invoke(self, context, event):
        if SAFE_EXPORT_DEFAULT:
            self.filepath = SAFE_EXPORT_DEFAULT
        return ExportHelper.invoke(self, context, event)

    def execute(self, context):
        try:
            output = write_safe_fitted_iff(self.filepath)
        except Exception as exc:
            self.report({"ERROR"}, str(exc))
            return {"CANCELLED"}
        self.report({"INFO"}, f"Safe fitted hair exported: {output.name}")
        return {"FINISHED"}


class NBA2K_PT_hair_fixer(Panel):
    bl_label = "NBA 2K Hair Fixer"
    bl_idname = "NBA2K_PT_hair_fixer"
    bl_space_type = "VIEW_3D"
    bl_region_type = "UI"
    bl_category = "Hair Fixer"

    def draw(self, context):
        layout = self.layout
        layout.label(text="Tangent Fit Test" if ACTIVE_FIT_MODE == "tangent" else "Auto-Fit")
        layout.operator(NBA2K_OT_safe_hair_export.bl_idname, text="Export Fitted Hair IFF", icon="EXPORT")
        layout.label(text="Select one FITTED hair mesh.")
        layout.label(text="Topology must remain unchanged.")


def safe_export_menu(self, context):
    self.layout.operator(
        NBA2K_OT_safe_hair_export.bl_idname,
        text="NBA 2K Safe Fitted Hair (.IFF)",
        icon="EXPORT",
    )


def register_safe_exporter(template_bytes, export_path, target_key, fit_mode):
    global SAFE_TEMPLATE_BYTES, SAFE_EXPORT_DEFAULT, SAFE_TARGET_KEY, ACTIVE_FIT_MODE
    SAFE_TEMPLATE_BYTES = template_bytes
    SAFE_EXPORT_DEFAULT = str(export_path) if export_path else ""
    SAFE_TARGET_KEY = target_key
    ACTIVE_FIT_MODE = fit_mode
    for cls in (NBA2K_OT_safe_hair_export, NBA2K_PT_hair_fixer):
        try:
            bpy.utils.register_class(cls)
        except RuntimeError:
            pass
    try:
        bpy.types.TOPBAR_MT_file_export.append(safe_export_menu)
    except Exception:
        pass


def import_iff(path):
    return import_iff_mesh(path)


def mesh_objects(objects):
    return [obj for obj in objects if obj.type == "MESH" and len(obj.data.vertices)]


def head_mesh(objects, label):
    meshes = mesh_objects(objects)
    if not meshes:
        raise RuntimeError(f"The {label} IFF did not produce a mesh.")
    named = [obj for obj in meshes if "hihead" in obj.name.lower() or "hihead" in obj.data.name.lower()]
    return max(named or meshes, key=lambda obj: len(obj.data.vertices))


def percentile(values, fraction):
    ordered = sorted(values)
    if not ordered:
        raise RuntimeError("Cannot measure an empty vertex set.")
    position = (len(ordered) - 1) * fraction
    lower = math.floor(position)
    upper = math.ceil(position)
    if lower == upper:
        return ordered[lower]
    blend = position - lower
    return ordered[lower] * (1.0 - blend) + ordered[upper] * blend


def world_vertices(obj):
    matrix = obj.matrix_world
    step = max(1, len(obj.data.vertices) // 80000)
    return [matrix @ obj.data.vertices[index].co for index in range(0, len(obj.data.vertices), step)]


def scalp_measurement(obj):
    vertices = world_vertices(obj)
    heights = [point.z for point in vertices]
    low_height = percentile(heights, 0.02)
    high_height = percentile(heights, 0.98)
    scalp_floor = low_height + (high_height - low_height) * 0.68
    scalp = [point for point in vertices if point.z >= scalp_floor]
    if len(scalp) < 50:
        scalp = vertices

    xs = [point.x for point in scalp]
    depths = [point.y for point in scalp]
    scalp_heights = [point.z for point in scalp]
    x_low, x_high = percentile(xs, 0.05), percentile(xs, 0.95)
    depth_low, depth_high = percentile(depths, 0.05), percentile(depths, 0.95)
    return {
        "anchor": Vector(
            (
                percentile(xs, 0.50),
                percentile(depths, 0.50),
                percentile(scalp_heights, 0.95),
            )
        ),
        "width": max(x_high - x_low, 0.001),
        "depth": max(depth_high - depth_low, 0.001),
    }


def conservative_ratio(target, donor):
    raw = max(0.88, min(1.12, target / donor))
    return 1.0 + (raw - 1.0) * 0.75


def fit_hair(hair_objects, donor_head, target_head):
    donor = scalp_measurement(donor_head)
    target = scalp_measurement(target_head)
    width_scale = conservative_ratio(target["width"], donor["width"])
    depth_scale = conservative_ratio(target["depth"], donor["depth"])
    uniform_scale = math.sqrt(width_scale * depth_scale)
    scale_x = scale_y = scale_z = uniform_scale
    scale = Vector((uniform_scale, uniform_scale, uniform_scale))
    location = target["anchor"] - Vector(
        (
            donor["anchor"].x * scale.x,
            donor["anchor"].y * scale.y,
            donor["anchor"].z * scale.z,
        )
    )

    control = bpy.data.objects.new("HAIR_FIXER_CONTROL", None)
    bpy.context.scene.collection.objects.link(control)
    control.empty_display_type = "ARROWS"
    control.empty_display_size = 12.0
    control["nba2k_fit_width"] = scale_x
    control["nba2k_fit_height"] = scale_y
    control["nba2k_fit_depth"] = scale_z
    control["nba2k_fit_note"] = "Adjust this Empty in Object Mode; child mesh world transforms export."

    for obj in hair_objects:
        world_matrix = obj.matrix_world.copy()
        obj.parent = control
        obj.matrix_world = world_matrix

    # Parent at identity first, then apply one transform to every LOD/mesh together.
    control.scale = scale
    control.location = location
    return control, donor, target


def triangle_frame(first, second, third):
    tangent = second - first
    if tangent.length_squared < 1.0e-12:
        tangent = third - first
    tangent.normalize()
    normal = (second - first).cross(third - first)
    if normal.length_squared < 1.0e-12:
        raise RuntimeError("Head mesh contains a degenerate triangle.")
    normal.normalize()
    bitangent = normal.cross(tangent)
    bitangent.normalize()
    return tangent, bitangent, normal


def clamp_ratio(value):
    return max(0.80, min(1.20, value))


def tangent_frame_fit(hair_objects, donor_head, target_head, surface_locked=False):
    donor_vertices = [donor_head.matrix_world @ vertex.co for vertex in donor_head.data.vertices]
    target_vertices = [target_head.matrix_world @ vertex.co for vertex in target_head.data.vertices]
    donor_faces = [tuple(polygon.vertices) for polygon in donor_head.data.polygons]
    target_faces = [tuple(polygon.vertices) for polygon in target_head.data.polygons]
    if not donor_faces or not target_faces or any(len(face) != 3 for face in donor_faces + target_faces):
        raise RuntimeError("Tangent Fit Test requires triangulated donor and target heads.")

    donor_kd = KDTree(len(donor_vertices))
    for index, point in enumerate(donor_vertices):
        donor_kd.insert(point, index)
    donor_kd.balance()
    donor_vertex_faces = [[] for _point in donor_vertices]
    for face_index, face in enumerate(donor_faces):
        for vertex_index in face:
            donor_vertex_faces[vertex_index].append(face_index)
    target_kd = KDTree(len(target_vertices))
    for index, point in enumerate(target_vertices):
        target_kd.insert(point, index)
    target_kd.balance()
    target_vertex_faces = [[] for _point in target_vertices]
    for face_index, face in enumerate(target_faces):
        for vertex_index in face:
            target_vertex_faces[vertex_index].append(face_index)
    matching_topology = len(donor_vertices) == len(target_vertices) and donor_faces == target_faces
    if surface_locked and not matching_topology:
        raise RuntimeError(
            "Headband surface fitting requires matching donor/target head topology."
        )
    donor_measure = scalp_measurement(donor_head)
    target_measure = scalp_measurement(target_head)
    width_scale = conservative_ratio(target_measure["width"], donor_measure["width"])
    depth_scale = conservative_ratio(target_measure["depth"], donor_measure["depth"])
    baseline_scale = math.sqrt(width_scale * depth_scale)

    control = bpy.data.objects.new("TANGENT_FRAME_FIT_TEST", None)
    bpy.context.scene.collection.objects.link(control)
    control.empty_display_type = "ARROWS"
    control.empty_display_size = 12.0
    control["nba2k_fit_mode"] = "tangent_frame_test"
    control["nba2k_fit_note"] = "Scalp-local triangle frame transfer with distance falloff."

    moved = 0
    distances = []
    weights = []
    for obj in hair_objects:
        inverse_world = obj.matrix_world.inverted()
        for vertex in obj.data.vertices:
            source_point = obj.matrix_world @ vertex.co
            _nearest_vertex, donor_vertex_index, _vertex_distance = donor_kd.find(source_point)
            adjacent_faces = donor_vertex_faces[donor_vertex_index]
            if not adjacent_faces:
                continue
            nearest = None
            face_index = None
            best_distance_squared = None
            for candidate_face_index in adjacent_faces:
                candidate_ids = donor_faces[candidate_face_index]
                candidate = closest_point_on_tri(
                    source_point,
                    *(donor_vertices[index] for index in candidate_ids),
                )
                candidate_distance_squared = (source_point - candidate).length_squared
                if best_distance_squared is None or candidate_distance_squared < best_distance_squared:
                    nearest = candidate
                    face_index = candidate_face_index
                    best_distance_squared = candidate_distance_squared
            if nearest is None or face_index is None:
                continue
            distance = math.sqrt(best_distance_squared)
            donor_ids = donor_faces[face_index]
            da, db, dc = (donor_vertices[index] for index in donor_ids)
            if matching_topology:
                target_ids = target_faces[face_index]
                ta, tb, tc = (target_vertices[index] for index in target_ids)
                target_surface = barycentric_transform(nearest, da, db, dc, ta, tb, tc)
            else:
                target_query = target_measure["anchor"] + (
                    nearest - donor_measure["anchor"]
                ) * baseline_scale
                target_surface, target_vertex_index, _target_distance = target_kd.find(target_query)
                adjacent_faces = target_vertex_faces[target_vertex_index]
                if target_surface is None or not adjacent_faces:
                    continue
                target_face_index = adjacent_faces[0]
                target_ids = target_faces[target_face_index]
                ta, tb, tc = (target_vertices[index] for index in target_ids)
            donor_tangent, donor_bitangent, donor_normal = triangle_frame(da, db, dc)
            target_tangent, target_bitangent, target_normal = triangle_frame(ta, tb, tc)
            offset = source_point - nearest
            tangent_offset = offset.dot(donor_tangent)
            bitangent_offset = offset.dot(donor_bitangent)
            normal_offset = offset.dot(donor_normal)
            tangent_scale = clamp_ratio((tb - ta).length / max((db - da).length, 1.0e-6))
            donor_height = abs((dc - da).dot(donor_bitangent))
            target_height = abs((tc - ta).dot(target_bitangent))
            bitangent_scale = clamp_ratio(target_height / max(donor_height, 1.0e-6))
            local_fit = (
                target_surface
                + target_tangent * tangent_offset * tangent_scale
                + target_bitangent * bitangent_offset * bitangent_scale
                + target_normal * normal_offset
            )
            if surface_locked:
                local_weight = 1.0
                fitted_point = local_fit
            else:
                baseline_fit = target_measure["anchor"] + (
                    source_point - donor_measure["anchor"]
                ) * baseline_scale
                falloff = max(0.0, min(1.0, (distance - 0.75) / 11.25))
                falloff = falloff * falloff * (3.0 - 2.0 * falloff)
                local_weight = max(0.20, 1.0 - falloff)
                fitted_point = baseline_fit.lerp(local_fit, local_weight)
            vertex.co = inverse_world @ fitted_point
            moved += 1
            distances.append(distance)
            weights.append(local_weight)
        obj.data.update()

    if not moved:
        raise RuntimeError("Tangent Fit Test could not map any hair vertices to the donor head.")
    control["nba2k_fit_vertices"] = moved
    control["nba2k_fit_correspondence"] = "topology" if matching_topology else "aligned_nearest_surface"
    control["nba2k_fit_average_distance"] = sum(distances) / len(distances)
    control["nba2k_fit_average_local_weight"] = sum(weights) / len(weights)
    return control, donor_measure, target_measure


def style_reference_head(objects, hidden=False):
    for obj in objects:
        obj.hide_render = True
        obj.hide_set(hidden)
        if obj.type == "MESH":
            obj.display_type = "WIRE"
            obj.color = (0.15, 0.65, 1.0, 0.35)


def frame_selection(objects):
    bpy.ops.object.select_all(action="DESELECT")
    for obj in objects:
        obj.hide_set(False)
        obj.select_set(True)
    if objects:
        bpy.context.view_layer.objects.active = objects[0]
    for screen in bpy.data.screens:
        for area in screen.areas:
            if area.type != "VIEW_3D":
                continue
            region = next((item for item in area.regions if item.type == "WINDOW"), None)
            if region:
                with bpy.context.temp_override(screen=screen, area=area, region=region):
                    bpy.ops.view3d.view_selected(use_all_regions=False)


def select_hair_for_export(objects):
    bpy.ops.object.select_all(action="DESELECT")
    for obj in objects:
        obj.hide_set(False)
        obj.select_set(True)
    if objects:
        bpy.context.view_layer.objects.active = objects[0]


def cleanup_staged_files(paths):
    parents = set()
    for path in paths:
        if not path:
            continue
        parents.add(path.parent)
        path.unlink(missing_ok=True)
    for parent in parents:
        try:
            parent.rmdir()
        except OSError:
            pass


def main():
    hair_path, donor_head_path, target_head_path, export_path, target_key, fit_mode = command_line_values()
    for path in (hair_path, donor_head_path, target_head_path):
        if not path.exists():
            raise FileNotFoundError(path)

    try:
        template_bytes = hair_path.read_bytes()
        register_safe_exporter(template_bytes, export_path, target_key, fit_mode)
        donor_objects = import_iff(donor_head_path)
        target_objects = import_iff(target_head_path)
        hair_objects = mesh_objects(import_iff(hair_path))
        if not hair_objects:
            raise RuntimeError("The selected hair IFF did not produce a mesh.")

        donor_head = head_mesh(donor_objects, "donor head")
        target_head = head_mesh(target_objects, "target head")
        if fit_mode in {"tangent", "headband"}:
            control, donor, target = tangent_frame_fit(
                hair_objects,
                donor_head,
                target_head,
                surface_locked=fit_mode == "headband",
            )
        else:
            control, donor, target = fit_hair(hair_objects, donor_head, target_head)

        style_reference_head(donor_objects, hidden=True)
        style_reference_head(target_objects, hidden=False)
        for obj in hair_objects:
            obj.name = f"FITTED_{obj.name}"
            obj["nba2k_safe_hair"] = True
        if not bpy.app.background:
            frame_selection(hair_objects + [target_head])
        select_hair_for_export(hair_objects)
        print(
            f"[Hair Fixer] {fit_mode} fit complete: "
            f"scale=({control.scale.x:.5f}, {control.scale.y:.5f}, {control.scale.z:.5f}) "
            f"location=({control.location.x:.5f}, {control.location.y:.5f}, {control.location.z:.5f}) "
            f"donor=({donor['width']:.3f}w, {donor['depth']:.3f}d) "
            f"target=({target['width']:.3f}w, {target['depth']:.3f}d)"
        )
        if fit_mode in {"tangent", "headband"}:
            print(
                f"[Hair Fixer] {'Headband surface' if fit_mode == 'headband' else 'Tangent'} mapping: "
                f"vertices={control.get('nba2k_fit_vertices', 0):,} "
                f"correspondence={control.get('nba2k_fit_correspondence', '?')} "
                f"average_distance={control.get('nba2k_fit_average_distance', 0.0):.5f} "
                f"average_local_weight={control.get('nba2k_fit_average_local_weight', 0.0):.5f}"
            )
        if bpy.app.background and export_path:
            output = write_safe_fitted_iff(export_path)
            print(f"[Hair Fixer] Headless safe export: {output}")
    finally:
        cleanup_staged_files([hair_path, donor_head_path, target_head_path])


if __name__ == "__main__":
    main()
