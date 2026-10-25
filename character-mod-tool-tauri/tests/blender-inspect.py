import json
import os
from pathlib import Path

import bpy

scene = bpy.context.scene
images = [image.filepath for image in bpy.data.images if image.source == "FILE"]
report = {
    "references": scene.get("character_mod_tool_reference_count", 0),
    "textures": scene.get("character_mod_tool_texture_count", 0),
    "jersey": scene.get("character_mod_tool_jersey_loaded", False),
    "images": images,
    "materials": len(bpy.data.materials),
    "meshes": len([obj for obj in scene.objects if obj.type == "MESH"]),
    "signal": json.loads(Path(scene["character_mod_tool_signal"]).read_text()),
}
assert report["references"] >= 2, report
assert report["textures"] > 0 and images, report
assert report["jersey"], report
assert report["signal"]["status"] == "success", report
Path(os.environ["CMT_BLENDER_TEST_REPORT"]).write_text(json.dumps(report, indent=2))
print("TAURI_BLENDER_ROUNDTRIP_OK")
