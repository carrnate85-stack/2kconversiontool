import argparse
import importlib
import inspect
import json
import os
import sys
import traceback
import zipfile

import bpy


HEADBAND_GROUP_NAMES = {
    "headbandshape",
    "headbandshader",
    "headband",
    "hair01shape",
    "hairshader",
    "hair01",
}


def emit(kind, message):
    print(f"CHARMOD_HEADBAND_{kind}|{message}", flush=True)


def parse_args():
    argv = sys.argv
    argv = argv[argv.index("--") + 1:] if "--" in argv else []
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--addon", required=True)
    parser.add_argument("--mesh-data-transfer", required=True)
    return parser.parse_args(argv)


def load_package(package_dir):
    package_dir = os.path.normpath(package_dir)
    if not os.path.isfile(os.path.join(package_dir, "__init__.py")):
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
    _mdt_name, mesh_data_transfer = load_package(args.mesh_data_transfer)
    mesh_data_transfer.register()
    if not hasattr(bpy.types.Object, "mesh_data_transfer_object"):
        raise RuntimeError("Mesh Data Transfer did not register in Blender.")

    emit("STAGE", "Loading NBA Headband Swap")
    addon_name, addon = load_package(args.addon)
    addon.register()
    eye_tool = importlib.import_module(f"{addon_name}.nba2k_character_eyeball_tool")
    if not hasattr(eye_tool, "transfer_headband_shape"):
        raise RuntimeError("The selected Blender tool does not include Headband Swap.")
    return addon_name, addon, eye_tool


def clear_scene():
    for obj in list(bpy.data.objects):
        bpy.data.objects.remove(obj, do_unlink=True)


def normalized_name(value):
    return "".join(character for character in value.lower() if character.isalnum())


def choose_headband_mesh(objects, label, expected_legacy):
    meshes = [obj for obj in objects if getattr(obj, "type", None) == "MESH"]
    headbands = []
    for obj in meshes:
        group_names = {normalized_name(group.name) for group in obj.vertex_groups}
        if str(obj.get("nba2k_asset_type", "") or "").lower() == "headband" or group_names.intersection(HEADBAND_GROUP_NAMES):
            headbands.append(obj)
    if headbands:
        meshes = headbands
    if expected_legacy is not None:
        matching = [
            obj for obj in meshes
            if bool(obj.get("nba2k_legacy_packed_import")) == expected_legacy
        ]
        if matching:
            meshes = matching
    if not meshes:
        expected = "source" if expected_legacy is None else ("legacy" if expected_legacy else "NBA 2K26")
        raise RuntimeError(f"{label} IFF did not import a usable {expected} headband mesh.")
    return max(meshes, key=lambda obj: len(obj.data.vertices))


def import_headband(path, label, expected_legacy):
    emit("STAGE", f"Importing {label} headband")
    before = set(bpy.data.objects)
    result = bpy.ops.import_vcnbacharacter.load(
        filepath=path,
        convert_bin_to_gz=False,
        split_groups=False,
        keep_extracted=False,
        apply_scene_placement=False,
        legacy_flip_uv_v=True,
    )
    if "FINISHED" not in result:
        raise RuntimeError(f"Blender could not import the {label.lower()} headband IFF.")
    imported = [obj for obj in bpy.data.objects if obj not in before]
    mesh = choose_headband_mesh(imported, label, expected_legacy)
    mesh["nba2k_source_path"] = path
    return mesh


def select_only(obj):
    if bpy.context.object and bpy.context.object.mode != "OBJECT":
        bpy.ops.object.mode_set(mode="OBJECT")
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    bpy.context.view_layer.update()


def swap_headband(eye_tool, source_obj, target_obj):
    emit("STAGE", "Running Blender Headband Mesh Data Transfer")
    parameters = inspect.signature(eye_tool.transfer_headband_shape).parameters
    if len(parameters) >= 3:
        ok, message = eye_tool.transfer_headband_shape(
            bpy.context,
            source_obj,
            target_obj,
            1.0,
        )
    else:
        scene = bpy.context.scene
        scene.nbachar_headband_source_mesh = source_obj
        scene.nbachar_headband_target_mesh = target_obj
        scene.nbachar_headband_transfer_strength = 1.0
        ok, message = eye_tool.transfer_headband_shape(bpy.context)
    if not ok:
        raise RuntimeError(message)
    emit("STAGE", message)


def export_target(target_path, output_path, target_obj):
    emit("STAGE", "Exporting and rebuilding target headband IFF")
    select_only(target_obj)
    bpy.context.scene.nbachar_export_original_iff = target_path
    result = bpy.ops.export_vcnbacharacter.save(
        filepath=output_path,
        source_iff_path=target_path,
        export_object_name=target_obj.name,
        auto_source_iff=False,
        keep_backup=False,
        include_eye_tool_data=False,
    )
    if "FINISHED" not in result:
        raise RuntimeError("Blender could not export the swapped headband IFF.")
    if not os.path.isfile(output_path) or not zipfile.is_zipfile(output_path):
        raise RuntimeError("The Blender export finished without creating a valid headband IFF.")


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

    _addon_name, addon, eye_tool = register_tools(args)
    clear_scene()
    source_obj = import_headband(source_path, "Source", None)
    source_obj.name = "HEADBAND_SOURCE"
    target_obj = import_headband(target_path, "NBA 2K26 target", False)
    swap_headband(eye_tool, source_obj, target_obj)
    export_target(target_path, output_path, target_obj)
    version = ".".join(str(value) for value in addon.bl_info.get("version", (0, 0, 0)))
    emit(
        "SUCCESS",
        json.dumps(
            {
                "output": output_path,
                "source_object": source_obj.name,
                "target_object": target_obj.name,
                "addon_version": version,
            }
        ),
    )


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        emit("ERROR", str(exc))
        traceback.print_exc()
        raise SystemExit(1)
