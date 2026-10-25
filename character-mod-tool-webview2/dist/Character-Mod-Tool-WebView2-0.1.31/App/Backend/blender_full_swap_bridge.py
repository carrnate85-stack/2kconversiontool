import argparse
import importlib
import inspect
import json
import math
import os
import shutil
import sys
import tempfile
import traceback
import zipfile

import bpy


def emit(kind, message):
    print(f"CHARMOD_{kind}|{message}", flush=True)


def parse_args():
    argv = sys.argv
    argv = argv[argv.index("--") + 1:] if "--" in argv else []
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--addon", required=True)
    parser.add_argument("--mesh-data-transfer", required=True)
    parser.add_argument("--shrinkwrap-body", choices=("yes", "no"), default="yes")
    parser.add_argument("--transfer-mode", choices=("full", "head", "body"), default="full")
    parser.add_argument("--source-game", default="")
    parser.add_argument("--target-game", default="")
    return parser.parse_args(argv)


def load_package(package_dir):
    package_dir = os.path.normpath(package_dir)
    init_path = os.path.join(package_dir, "__init__.py")
    if not os.path.isfile(init_path):
        raise RuntimeError(f"Missing add-on package: {package_dir}")
    package_name = os.path.basename(package_dir)
    if not package_name.isidentifier():
        raise RuntimeError(f"Add-on folder cannot be imported: {package_name}")
    parent = os.path.dirname(package_dir)
    if parent not in sys.path:
        sys.path.insert(0, parent)
    return package_name, importlib.import_module(package_name)


def register_tools(args):
    emit("STAGE", "Loading Mesh Data Transfer")
    if not hasattr(bpy.types.Object, "mesh_data_transfer_object"):
        _mdt_name, mesh_data_transfer = load_package(args.mesh_data_transfer)
        mesh_data_transfer.register()
    if not hasattr(bpy.types.Object, "mesh_data_transfer_object"):
        raise RuntimeError("Mesh Data Transfer did not register in Blender.")

    emit("STAGE", "Loading NBA Character Head Swap")
    addon_name, addon = load_package(args.addon)
    addon.register()
    if "load" not in dir(bpy.ops.import_vcnbacharacter):
        raise RuntimeError("NBA Character Head Swap import operator did not register.")
    return addon_name, addon


def clear_scene():
    for obj in list(bpy.data.objects):
        bpy.data.objects.remove(obj, do_unlink=True)


def choose_character_mesh(objects, label):
    meshes = [obj for obj in objects if getattr(obj, "type", None) == "MESH"]
    metadata_meshes = [
        obj for obj in meshes
        if obj.get("nba2k_vertex_components") or obj.get("nba2k_legacy_packed_import")
    ]
    if metadata_meshes:
        meshes = metadata_meshes
    if not meshes:
        raise RuntimeError(f"{label} IFF imported without a usable character mesh.")

    def score(obj):
        group_names = {group.name.lower() for group in obj.vertex_groups}
        return (
            1 if "blend_headshape" in group_names else 0,
            len(obj.data.vertices),
        )

    return max(meshes, key=score)


def import_character(path, label, original_path=None):
    emit("STAGE", f"Importing {label} character")
    before = set(bpy.data.objects)
    import_options = {
        "filepath": path,
        "convert_bin_to_gz": False,
        "split_groups": False,
        "keep_extracted": False,
        "apply_scene_placement": False,
        "legacy_flip_uv_v": True,
    }
    try:
        supported = set(bpy.ops.import_vcnbacharacter.load.get_rna_type().properties.keys())
    except Exception:
        supported = set()
    if "allow_missing_matrix_weights" in supported:
        import_options["allow_missing_matrix_weights"] = label.lower() == "source"
    result = bpy.ops.import_vcnbacharacter.load(**import_options)
    if "FINISHED" not in result:
        raise RuntimeError(f"Blender could not import the {label.lower()} IFF.")
    imported = [obj for obj in bpy.data.objects if obj not in before]
    mesh = choose_character_mesh(imported, label)
    mesh["nba2k_source_path"] = original_path or path
    return mesh


def normalize_game_label(value):
    compact = "".join(character for character in str(value or "").upper() if character.isalnum())
    for year in range(17, 28):
        if compact in {f"NBA2K{year}", f"2K{year}"}:
            return f"NBA 2K{year}"
    return str(value or "").strip()


def source_generation(source_obj):
    return normalize_game_label(
        source_obj.get("nba2k_source_game", "")
        or source_obj.get("nba2k_legacy_game", "")
    )


def is_nba_2k18_source(source_obj):
    return source_generation(source_obj).upper() == "NBA 2K18"


def uses_protected_legacy_body_wrap(source_obj):
    return source_generation(source_obj).upper() in {"NBA 2K18", "NBA 2K19"}


def conversion_route(source_game, target_game, transfer_mode):
    source_game = normalize_game_label(source_game)
    target_game = normalize_game_label(target_game)
    if transfer_mode == "full" and source_game == "NBA 2K26" and target_game == "NBA 2K27":
        return "NBA 2K26 -> NBA 2K27 Full Swap"
    if transfer_mode == "full" and source_game == "NBA 2K18" and target_game == "NBA 2K23":
        return "NBA 2K18 -> NBA 2K23 Packed Full Swap"
    return ""


def staged_import_path(staging_dir, role, original_path):
    folder = os.path.join(staging_dir, role.lower())
    os.makedirs(folder, exist_ok=True)
    staged_path = os.path.join(folder, os.path.basename(original_path))
    shutil.copy2(original_path, staged_path)
    return staged_path


def select_only(obj):
    if bpy.context.object and bpy.context.object.mode != "OBJECT":
        bpy.ops.object.mode_set(mode="OBJECT")
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    bpy.context.view_layer.update()


def match_modern_source_eye_sizes(eye_tool, source_obj, target_obj):
    if (source_generation(source_obj) not in {"NBA 2K25", "NBA 2K26"}
            or normalize_game_label(target_obj.get("nba2k_target_game", "")) != "NBA 2K27"):
        return

    # Match only eyeballs, not the target eyelids/accessories or their blink morphs.
    fits = []
    occupied = set()
    for side in ("L", "R"):
        group_name = side + "_eyeShape"
        source_points = eye_tool._collect_vg_points_local(source_obj, group_name, .001)
        target_points = eye_tool._collect_vg_points_local(target_obj, group_name, .001)
        if not source_points or not target_points:
            raise RuntimeError("Source-matched eye sizing needs both eyeball groups: " + group_name)
        _, source_radius = eye_tool._center_and_radius(source_points)
        center, target_radius = eye_tool._center_and_radius(target_points)
        if not all(math.isfinite(radius) and radius > 1e-6
                   for radius in (source_radius, target_radius)):
            raise RuntimeError("Source-matched eye sizing found an invalid radius: " + group_name)
        group = target_obj.vertex_groups.get(group_name)
        indices = set(eye_tool._vertex_indices_for_group_obj(target_obj, group, .001))
        if not indices or occupied.intersection(indices):
            raise RuntimeError("Source-matched eye sizing found invalid eyeball membership: " + group_name)
        occupied.update(indices)
        fits.append((side, center, target_radius, source_radius / target_radius, indices))

    if target_obj.data.users > 1:
        target_obj.data = target_obj.data.copy()
    for side, center, radius, scale, indices in fits:
        # Retain the pre-transfer baseline so joint export includes position AND size.
        if "nba2k_eye_transfer_base_" + side not in target_obj:
            target_obj["nba2k_eye_transfer_base_" + side] = tuple(center)
            target_obj["nba2k_eye_transfer_radius_" + side] = radius
        for index in indices:
            vertex = target_obj.data.vertices[index]
            vertex.co = center + (vertex.co - center) * scale
        target_obj["nba2k_source_eye_size_scale_" + side] = scale
    target_obj.data.update()
    target_obj["nba2k_source_eye_size_matched"] = True
    emit("STAGE", "Source-matched eyeball sizes; eyelids/accessories unchanged (%s)" %
         ", ".join("%s %.6f" % (side, scale) for side, _, _, scale, _ in fits))


def run_full_transfer(
    addon_name,
    source_obj,
    target_obj,
    shrinkwrap_body=True,
    route="",
):
    emit("STAGE", f"Running {route or 'Blender Full Transfer'}")
    scene = bpy.context.scene
    scene.nbachar_full_source_mesh = source_obj
    scene.nbachar_full_target_mesh = target_obj
    eye_tool = importlib.import_module(f"{addon_name}.nba2k_character_eyeball_tool")
    if route == "NBA 2K26 -> NBA 2K27 Full Swap":
        transfer = getattr(eye_tool, "experiment_full_transfer_2k26_to_2k27", None)
        if transfer is None:
            raise RuntimeError(
                "The selected Blender Swap Tool does not include the NBA 2K26 to NBA 2K27 full-swap route."
            )
    else:
        transfer = eye_tool.experiment_full_transfer
    parameters = inspect.signature(transfer).parameters
    transfer_body = shrinkwrap_body and not uses_protected_legacy_body_wrap(source_obj)
    if "include_body_shrinkwrap" in parameters:
        ok, message = transfer(bpy.context, include_body_shrinkwrap=transfer_body)
    elif transfer_body:
        ok, message = transfer(bpy.context)
    else:
        raise RuntimeError(
            "The selected Blender Swap Tool is too old to disable body shrinkwrap. Choose the bundled current tool."
        )
    if not ok:
        raise RuntimeError(message)
    emit("STAGE", message)
    if shrinkwrap_body and uses_protected_legacy_body_wrap(source_obj):
        run_2k18_body_transfer(eye_tool, source_obj, target_obj)
    match_modern_source_eye_sizes(eye_tool, source_obj, target_obj)


def run_2k18_body_transfer(eye_tool, source_obj, target_obj):
    emit("STAGE", f"Running protected {source_generation(source_obj)} Body Wrap")
    scene = bpy.context.scene
    scene.nbachar_body_source_mesh = source_obj
    scene.nbachar_body_target_mesh = target_obj
    transfer = getattr(eye_tool, "wrap_2k18_body", None)
    if transfer is None:
        raise RuntimeError(
            "The selected Blender Swap Tool does not include NBA 2K18 body-wrap support."
        )
    ok, message = transfer(bpy.context)
    if not ok:
        raise RuntimeError(message)
    emit("STAGE", message)


def run_body_transfer(addon_name, source_obj, target_obj):
    emit("STAGE", "Running Body-Only Transfer")
    scene = bpy.context.scene
    scene.nbachar_body_source_mesh = source_obj
    scene.nbachar_body_target_mesh = target_obj
    eye_tool = importlib.import_module(f"{addon_name}.nba2k_character_eyeball_tool")
    if uses_protected_legacy_body_wrap(source_obj):
        run_2k18_body_transfer(eye_tool, source_obj, target_obj)
        return
    weight_min = getattr(scene, "nbachar_eye_vg_weight_min", 0.001)
    matching_pairs, matching_details = eye_tool._matching_body_group_pairs(
        source_obj,
        target_obj,
        weight_min,
    )
    if matching_pairs:
        ok, message = eye_tool.transfer_matching_body_groups(bpy.context)
        mode = f"matched active-UV groups; anchors skipped ({matching_details})"
    else:
        ok, message = eye_tool.test_shrinkwrap_body(bpy.context)
        if ok:
            ok, anchor_message = eye_tool.test_anchor_shrinkwrap_body(bpy.context)
            message = f"{message} | {anchor_message}"
        mode = f"legacy shrinkwrap fallback ({matching_details})"
    if not ok:
        raise RuntimeError(message)
    emit("STAGE", f"Body-Only Transfer complete: {mode} | {message}")


def export_target(target_path, output_path, target_obj, include_eye_tool_data=True):
    emit("STAGE", "Exporting and rebuilding target IFF")
    select_only(target_obj)
    bpy.context.scene.nbachar_export_original_iff = target_path
    result = bpy.ops.export_vcnbacharacter.save(
        filepath=output_path,
        source_iff_path=target_path,
        export_object_name=target_obj.name,
        auto_source_iff=False,
        keep_backup=False,
        include_eye_tool_data=include_eye_tool_data,
    )
    if "FINISHED" not in result:
        raise RuntimeError("Blender could not export the swapped target IFF.")
    if not os.path.isfile(output_path) or not zipfile.is_zipfile(output_path):
        raise RuntimeError("The Blender export finished without creating a valid output IFF.")


def main():
    args = parse_args()
    source_path = os.path.abspath(args.source)
    target_path = os.path.abspath(args.target)
    output_path = os.path.abspath(args.output)
    for label, path in (("Source", source_path), ("Target", target_path)):
        if not os.path.isfile(path) or not zipfile.is_zipfile(path):
            raise RuntimeError(f"{label} is not a readable ZIP-style IFF: {path}")
    if output_path in (source_path, target_path):
        raise RuntimeError("Output IFF must be different from the source and target files.")

    addon_name, addon = register_tools(args)
    import_staging = tempfile.mkdtemp(prefix="character_mod_blender_import_")
    try:
        staged_source = staged_import_path(import_staging, "source", source_path)
        staged_target = staged_import_path(import_staging, "target", target_path)
        clear_scene()
        source_obj = import_character(staged_source, "Source", source_path)
        source_obj.name = "FULL_SWAP_SOURCE"
        source_game = normalize_game_label(args.source_game)
        target_game = normalize_game_label(args.target_game)
        if source_game:
            source_obj["nba2k_source_game"] = source_game
            source_obj["nba2k_legacy_game"] = source_game
        generation = source_generation(source_obj)
        if generation:
            emit("STAGE", f"Detected {generation} source geometry")
        target_obj = import_character(staged_target, "Target", target_path)
        if target_game:
            target_obj["nba2k_target_game"] = target_game
        route = conversion_route(generation, target_game, args.transfer_mode)
        if route:
            emit("STAGE", f"Selected {route} pathway")
        if args.transfer_mode == "body":
            run_body_transfer(addon_name, source_obj, target_obj)
        elif args.transfer_mode == "head":
            run_full_transfer(addon_name, source_obj, target_obj, False)
        else:
            run_full_transfer(
                addon_name,
                source_obj,
                target_obj,
                args.shrinkwrap_body == "yes",
                route,
            )
        export_target(
            target_path,
            output_path,
            target_obj,
            include_eye_tool_data=args.transfer_mode != "body",
        )
        version = ".".join(str(value) for value in addon.bl_info.get("version", (0, 0, 0)))
        emit(
            "SUCCESS",
            json.dumps(
                {
                    "output": output_path,
                    "source_object": source_obj.name,
                    "target_object": target_obj.name,
                    "addon_version": version,
                    "transfer_mode": args.transfer_mode,
                    "source_generation": generation or "Modern/unclassified",
                    "target_generation": target_game or "Modern/unclassified",
                    "conversion_route": route or "Automatic Full Swap",
                }
            ),
        )
    finally:
        shutil.rmtree(import_staging, ignore_errors=True)


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        emit("ERROR", str(exc))
        traceback.print_exc()
        raise SystemExit(1)
