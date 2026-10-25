"""Isolate prototype settings without altering the conversion implementation."""
import json
import os
import sys
from pathlib import Path

root = Path(os.environ["CHARACTER_MOD_TOOL_ROOT"])
sys.path.insert(0, str(root))
import app_settings
import prototype_services
import prototype_blender_snapshot
import prototype_blender_paths
import nba2k25_support

nba2k25_support.build_dds = prototype_services.build_rust_dds

data = Path(os.environ["CMT_RUST_DATA"])
data.mkdir(parents=True, exist_ok=True)
settings_path = data / "settings.json"
if not settings_path.exists():
    try:
        values = json.loads(app_settings.SETTINGS_PATH.read_text(encoding="utf-8"))
        if not isinstance(values, dict):
            values = {}
    except (OSError, ValueError):
        values = {}
    for key, directory in (("head_swap_tool", "NBA_Character_HeadSwap"),
                           ("mesh_data_transfer", "mesh_data_transfer")):
        bundled = root / "blender_tools" / directory
        if bundled.is_dir():
            values[key] = str(bundled)
    values["output_dir"] = str(data / "outputs")
    if prototype_blender_paths.bundled_blender(os.environ.get("CMT_RUST_EXE")):
        values["blender_exe"] = ""
    prototype_services.atomic_json(settings_path, values)

app_settings._USER_DATA_DIR = data
app_settings.SETTINGS_PATH = settings_path
app_settings.LOG_DIR = data / "logs"
app_settings.DIAGNOSTICS_DIR = data / "diagnostics"
import python_bridge
original_fast_settings = python_bridge.fast_settings

def private_settings():
    values = original_fast_settings()
    try:
        stored = json.loads(settings_path.read_text(encoding="utf-8"))
        if not isinstance(stored, dict):
            stored = {}
    except (OSError, ValueError):
        stored = {}
    values["blender_exe"] = prototype_blender_paths.select_blender(
        values.get("blender_exe", ""), str(stored.get("blender_exe") or ""),
        os.environ.get("CMT_RUST_EXE"))
    for key, directory in (("head_swap_tool", "NBA_Character_HeadSwap"),
                           ("mesh_data_transfer", "mesh_data_transfer")):
        configured = str(stored.get(key) or "")
        normalized = configured.replace("\\", "/").lower()
        bundled_runtime = "/charactermodtooltauri/runtime/" in normalized
        if configured:
            try:
                relative = Path(configured).resolve().relative_to((data / "Runtime").resolve())
                parts = tuple(part.casefold() for part in relative.parts)
                bundled_runtime |= (len(parts) == 4 and parts[0].startswith("engine-")
                                    and parts[1:] == ("backend", "blender_tools", directory.casefold()))
            except (OSError, ValueError):
                pass
        if not configured or bundled_runtime:
            values[key] = str(root / "blender_tools" / directory)
    return values

python_bridge.fast_settings = private_settings
app_settings.load_settings = private_settings
original_hair_backend = python_bridge.load_hair_backend
def highest_detail_backend(backend):
    if not getattr(backend.convert_legacy_hair, "_cmt_highest_detail", False):
        original_convert = backend.convert_legacy_hair
        def highest_detail_convert(*args, **kwargs):
            kwargs["prefer_highest_lod"] = True
            return original_convert(*args, **kwargs)
        highest_detail_convert._cmt_highest_detail = True
        backend.convert_legacy_hair = highest_detail_convert
    return backend

def private_hair_backend():
    backend = highest_detail_backend(original_hair_backend())
    backend.LOCAL_CACHE_ROOT = data
    backend.MANIFEST_EXTRACT_DIR = Path(os.environ["CMT_SESSION"]) / "hair-cache"
    backend.MANIFEST_EXTRACT_DIR.mkdir(parents=True, exist_ok=True)
    backend.native_vcz_decoder_path = lambda: root / "electron/helpers/Nba2kVczDecoder.exe"
    return backend
python_bridge.load_hair_backend = private_hair_backend
if sys.argv[1:2] == ["run-swap"]:
    from character_mod_tool import CharacterModTool
    original_swap_hair_backend = CharacterModTool.load_hair_backend
    def highest_detail_swap_backend(app):
        return highest_detail_backend(original_swap_hair_backend(app))
    CharacterModTool.load_hair_backend = highest_detail_swap_backend
    original_swap_validation = CharacterModTool.validate_everything_swap_outputs
    def validate_swap_distances(app, player_path, config_archives, companion_stages=()):
        return prototype_services.validate_swap_distances(
            app, original_swap_validation, player_path, config_archives, companion_stages)
    CharacterModTool.validate_everything_swap_outputs = validate_swap_distances
python_bridge.COMMANDS.update(prototype_services.commands(python_bridge, root, data))
python_bridge.COMMANDS.update(prototype_blender_snapshot.commands(python_bridge, root, data))
import prototype_accessories
python_bridge.COMMANDS.update(prototype_accessories.recovery_hooks(python_bridge.COMMANDS, python_bridge.emit_event))

original_save_settings_command = python_bridge.COMMANDS["save-settings"]

def save_portable_settings(payload):
    payload = dict(payload)
    if "blender_exe" in payload:
        payload["blender_exe"] = prototype_blender_paths.portable_setting(
            str(payload["blender_exe"] or ""), os.environ.get("CMT_RUST_EXE"))
    return original_save_settings_command(payload)

python_bridge.COMMANDS["save-settings"] = save_portable_settings

try:
    python_bridge.main()
except Exception as error:
    python_bridge.emit_result({"error": str(error)})
    raise SystemExit(1)
