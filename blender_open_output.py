import argparse
import importlib
import os
import sys
import traceback

import bpy


def parse_args():
    arguments = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    parser = argparse.ArgumentParser()
    parser.add_argument("--iff", required=True)
    parser.add_argument("--addon", required=True)
    return parser.parse_args(arguments)


def main():
    args = parse_args()
    iff_path = os.path.abspath(args.iff)
    addon_path = os.path.abspath(args.addon)
    addon_parent = os.path.dirname(addon_path)
    module_name = os.path.basename(addon_path)
    if addon_parent not in sys.path:
        sys.path.insert(0, addon_parent)
    addon = importlib.import_module(module_name)
    addon.register()
    result = bpy.ops.import_vcnbacharacter.load(
        filepath=iff_path,
        convert_bin_to_gz=False,
        split_groups=False,
        keep_extracted=False,
        apply_scene_placement=False,
    )
    if "FINISHED" not in result:
        raise RuntimeError(f"Character import returned {result}")
    bpy.context.scene["character_mod_tool_output"] = iff_path
    bpy.context.scene["nba_character_source_path"] = iff_path
    print(f"CHARMOD_OPENED|{iff_path}")


try:
    main()
except Exception:
    traceback.print_exc()
    raise
