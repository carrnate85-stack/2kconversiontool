import argparse
import importlib
import inspect
import json
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
    result = bpy.ops.import_vcnbacharacter.load(
        filepath=path,
        convert_bin_to_gz=False,
        split_groups=False,
        keep_extracted=False,
        apply_scene_placement=False,
        legacy_flip_uv_v=True,
        allow_missing_matrix_weights=label.lower() == "source",
    )
    if "FINISHED" not in result:
        raise RuntimeError(f"Blender could not import the {label.lower()} IFF.")
    imported = [obj for obj in bpy.data.objects if obj not in before]
    mesh = choose_character_mesh(imported, label)
    mesh["nba2k_source_path"] = original_path or path
    return mesh


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


def run_full_transfer(addon_name, source_obj, target_obj, shrinkwrap_body=True):
    emit("STAGE", "Running Blender Full Transfer")
    scene = bpy.context.scene
    scene.nbachar_full_source_mesh = source_obj
    scene.nbachar_full_target_mesh = target_obj
    eye_tool = importlib.import_module(f"{addon_name}.nba2k_character_eyeball_tool")
    transfer = eye_tool.experiment_full_transfer
    parameters = inspect.signature(transfer).parameters
    if "include_body_shrinkwrap" in parameters:
        ok, message = transfer(bpy.context, include_body_shrinkwrap=shrinkwrap_body)
    elif shrinkwrap_body:
        ok, message = transfer(bpy.context)
    else:
        raise RuntimeError(
            "The selected Blender Swap Tool is too old to disable body shrinkwrap. Choose the bundled current tool."
        )
    if not ok:
        raise RuntimeError(message)
    emit("STAGE", message)


def run_body_transfer(addon_name, source_obj, target_obj):
    emit("STAGE", "Running Body-Only Transfer")
    scene = bpy.context.scene
    scene.nbachar_body_source_mesh = source_obj
    scene.nbachar_body_target_mesh = target_obj
    eye_tool = importlib.import_module(f"{addon_name}.nba2k_character_eyeball_tool")
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
        target_obj = import_character(staged_target, "Target", target_path)
        if args.transfer_mode == "body":
            run_body_transfer(addon_name, source_obj, target_obj)
        elif args.transfer_mode == "head":
            run_full_transfer(addon_name, source_obj, target_obj, False)
        else:
            run_full_transfer(addon_name, source_obj, target_obj, args.shrinkwrap_body == "yes")
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
