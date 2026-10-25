import importlib.util
import json
import math
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import traceback
import zipfile
from datetime import datetime, timezone
from pathlib import Path


PROJECT_ROOT = Path(
    os.environ.get("CHARACTER_MOD_TOOL_ROOT") or Path(__file__).resolve().parent.parent
).resolve()
if str(PROJECT_ROOT) not in sys.path:
    sys.path.insert(0, str(PROJECT_ROOT))

import app_settings
from character_manifest import (
    build_character_manifest_catalog,
    extract_character_manifest_package,
)


DISABLED_HEADBAND_STYLES = frozenset({"Stock NBA 2K26 Headband"})


def read_payload():
    raw = sys.stdin.read()
    return json.loads(raw) if raw.strip() else {}


def emit_event(event_type, message="", **extra):
    print(
        "CMT_EVENT|"
        + json.dumps({"type": event_type, "message": str(message), **extra}),
        flush=True,
    )


def emit_result(payload):
    print("CMT_RESULT|" + json.dumps(payload), flush=True)


def require_file(path, label):
    resolved = os.path.abspath(str(path or ""))
    if not os.path.isfile(resolved):
        raise FileNotFoundError(f"{label} was not found: {resolved}")
    return resolved


def fast_output_dir(preferred=""):
    runtime = os.environ.get("CMT_PORTABLE_RUNTIME", "")
    fallback = app_settings.user_data_dir() / "outputs" if runtime else PROJECT_ROOT / "outputs"
    candidates = [preferred, str(fallback)]
    for candidate in candidates:
        if not candidate:
            continue
        if runtime:
            try:
                Path(candidate).resolve().relative_to(Path(runtime).resolve().parent)
                continue
            except ValueError:
                pass
        try:
            os.makedirs(candidate, exist_ok=True)
            return os.path.normpath(os.path.abspath(candidate))
        except OSError:
            continue
    return ""


def fast_settings():
    values = dict(app_settings.DEFAULTS)
    try:
        loaded = json.loads(app_settings.SETTINGS_PATH.read_text(encoding="utf-8"))
        if isinstance(loaded, dict):
            values.update({key: str(loaded.get(key, "") or "") for key in values})
    except (OSError, ValueError, TypeError):
        pass
    values["game_root"] = app_settings.discover_game_root(values["game_root"])
    values["game_root_2k27"] = app_settings.discover_nba2k27_root(
        values["game_root_2k27"]
    )
    values["blender_exe"] = app_settings.discover_blender(values["blender_exe"])
    values["photoshop_exe"] = app_settings.discover_photoshop(values["photoshop_exe"])
    values["head_swap_tool"] = app_settings.discover_head_swap_tool(
        values["head_swap_tool"]
    )
    values["mesh_data_transfer"] = app_settings.discover_mesh_data_transfer(
        values["mesh_data_transfer"]
    )
    values["output_dir"] = fast_output_dir(values["output_dir"])
    return values


def game_root_for(settings, game_label):
    if game_label == "NBA 2K27":
        root = app_settings.discover_nba2k27_root(settings.get("game_root_2k27", ""))
    elif game_label == "NBA 2K26":
        root = app_settings.discover_game_root(settings.get("game_root", ""))
    elif game_label == "NBA 2K25":
        from nba2k25_support import discover_root
        root = discover_root()
    elif game_label == "NBA 2K23":
        root = app_settings.discover_nba2k23_root()
    elif game_label == "NBA 2K20":
        root = app_settings.discover_nba2k20_root()
    elif game_label == "NBA 2K19":
        root = app_settings.discover_nba2k19_root()
    elif game_label == "NBA 2K18":
        root = app_settings.discover_nba2k18_root()
    else:
        raise ValueError(f"Character Mod Tool manifest selection does not support {game_label}.")
    if not root or not os.path.isfile(os.path.join(root, "manifest")):
        raise FileNotFoundError(f"The {game_label} manifest could not be found.")
    return root


def game_is_available(settings, game_label):
    try:
        root = game_root_for(settings, game_label)
    except (OSError, ValueError):
        return False
    required_archive = "mod.exe" if game_label in {"NBA 2K25", "NBA 2K26", "NBA 2K27"} else "0A"
    return os.path.isfile(os.path.join(root, required_archive))


def player_names():
    path = app_settings.resource_path("hair_tools", "player_names.json")
    try:
        loaded = json.loads(Path(path).read_text(encoding="utf-8"))
        return {str(key): str(value) for key, value in loaded.items() if value}
    except (OSError, ValueError, TypeError):
        return {}


def catalog_rows(game_label):
    settings = fast_settings()
    root = game_root_for(settings, game_label)
    if game_label in {"NBA 2K18", "NBA 2K19", "NBA 2K20", "NBA 2K23"}:
        from character_mod_tool import build_nba2k23_source_catalog

        names_file = app_settings.resource_path(
            "assets", f"{game_label.lower().replace(' ', '')}_player_names.txt"
        )
        return root, build_nba2k23_source_catalog(
            os.path.join(root, "manifest"),
            names_file,
        )
    return root, build_character_manifest_catalog(
        os.path.join(root, "manifest"),
        player_names(),
        game_label,
    )


def command_bootstrap(_payload):
    settings = fast_settings()
    from character_mod_tool import APP_VERSION

    return {
        "engineVersion": APP_VERSION,
        "settings": settings,
        "games": {
            label: game_is_available(settings, label)
            for label in (
                "NBA 2K18",
                "NBA 2K19",
                "NBA 2K20",
                "NBA 2K23",
                "NBA 2K25",
                "NBA 2K26",
                "NBA 2K27",
            )
        },
    }


def health_check(check_id, label, status, message, path="", repair=None):
    result = {
        "id": check_id,
        "label": label,
        "status": status,
        "message": message,
    }
    if path:
        result["path"] = os.path.normpath(str(path))
    if repair:
        result["repair"] = repair
    return result


def directory_size(root):
    path = Path(root)
    if not path.exists():
        return 0
    total = 0
    try:
        for candidate in path.rglob("*"):
            if candidate.is_file():
                total += candidate.stat().st_size
    except OSError:
        return total
    return total


def writable_directory_check(check_id, label, directory, *, required=True):
    path = Path(str(directory or "")).expanduser()
    if not str(directory or "").strip():
        return health_check(
            check_id,
            label,
            "error" if required else "warning",
            "No folder is configured.",
            repair={"action": "create-output", "label": "Create Output Folder"},
        )
    probe = None
    try:
        path.mkdir(parents=True, exist_ok=True)
        probe = path / f".character-mod-health-{os.getpid()}.tmp"
        probe.write_bytes(b"Character Mod Tool health check\n")
        probe.unlink(missing_ok=True)
        return health_check(
            check_id,
            label,
            "pass",
            "Folder is available and writable.",
            path,
            {"action": "open-path", "label": "Open Folder", "path": str(path)},
        )
    except OSError as exc:
        if probe:
            probe.unlink(missing_ok=True)
        return health_check(
            check_id,
            label,
            "error" if required else "warning",
            f"Folder is not writable: {exc}",
            path,
            {"action": "settings", "label": "Open Settings"},
        )


def file_component_check(check_id, label, file_path):
    path = Path(file_path)
    if path.is_file():
        return health_check(check_id, label, "pass", f"Found {path.name}.", path)
    return health_check(
        check_id,
        label,
        "error",
        f"Required component is missing: {path.name}",
        path,
    )


def game_health_check(label, root, *, required=False, modern=False):
    path = Path(str(root or "")).expanduser()
    missing_status = "error" if required else ("warning" if label == "NBA 2K26" else "info")
    repair = {"action": "settings", "label": "Open Settings"} if label in {"NBA 2K26", "NBA 2K27"} else None
    if not str(root or "").strip() or not path.is_dir():
        return health_check(
            f"game-{label.lower().replace(' ', '-')}",
            label,
            missing_status,
            "Installation was not detected." if not required else "Required game installation was not detected.",
            path if str(root or "").strip() else "",
            repair,
        )
    manifest = path / "manifest"
    archive = path / ("mod.exe" if modern else "0A")
    if not manifest.is_file():
        return health_check(
            f"game-{label.lower().replace(' ', '-')}",
            label,
            "error" if required else "warning",
            "The game folder exists, but its manifest is missing.",
            path,
            repair,
        )
    try:
        manifest_size = manifest.stat().st_size
        with manifest.open("rb") as handle:
            handle.read(64)
    except OSError as exc:
        return health_check(
            f"game-{label.lower().replace(' ', '-')}",
            label,
            "error" if required else "warning",
            f"The manifest cannot be read: {exc}",
            manifest,
            repair,
        )
    if manifest_size <= 0:
        return health_check(
            f"game-{label.lower().replace(' ', '-')}",
            label,
            "error" if required else "warning",
            "The manifest is empty.",
            manifest,
            repair,
        )
    if not archive.is_file():
        return health_check(
            f"game-{label.lower().replace(' ', '-')}",
            label,
            "error" if required else "warning",
            f"The manifest is readable, but {archive.name} is missing.",
            path,
            repair,
        )
    return health_check(
        f"game-{label.lower().replace(' ', '-')}",
        label,
        "pass",
        f"Manifest is readable ({manifest_size:,} bytes).",
        path,
        {"action": "open-path", "label": "Open Game Folder", "path": str(path)},
    )


def summarize_health(groups):
    counts = {"pass": 0, "warning": 0, "error": 0, "info": 0}
    for group in groups:
        for check in group.get("checks", []):
            status = check.get("status", "info")
            counts[status] = counts.get(status, 0) + 1
    overall = "error" if counts["error"] else "warning" if counts["warning"] else "pass"
    return overall, counts


def command_health(_payload):
    settings = fast_settings()
    settings_check = health_check(
        "settings-file",
        "Local settings",
        "pass",
        "Settings are readable; auto-detected paths are available.",
        app_settings.SETTINGS_PATH,
        {"action": "auto-detect-settings", "label": "Re-detect Paths"},
    )
    if app_settings.SETTINGS_PATH.exists():
        try:
            loaded = json.loads(app_settings.SETTINGS_PATH.read_text(encoding="utf-8"))
            if not isinstance(loaded, dict):
                raise ValueError("settings root is not an object")
        except (OSError, ValueError, TypeError) as exc:
            settings_check = health_check(
                "settings-file",
                "Local settings",
                "warning",
                f"Settings could not be parsed: {exc}",
                app_settings.SETTINGS_PATH,
                {"action": "auto-detect-settings", "label": "Repair Settings"},
            )

    blender_value = settings.get("blender_exe", "")
    blender = Path(blender_value) if blender_value else None
    blender_check = (
        health_check(
            "blender-executable",
            "Blender",
            "pass",
            "Blender executable is available.",
            blender,
            {"action": "settings", "label": "Open Settings"},
        )
        if blender and blender.is_file()
        else health_check(
            "blender-executable",
            "Blender 5.1",
            "error",
            "Blender 5.1 could not be found.",
            blender or "",
            {"action": "settings", "label": "Open Settings"},
        )
    )

    packaged_tools = PROJECT_ROOT / "blender_tools"
    backend_checks = [
        health_check(
            "python-runtime",
            "Python backend",
            "pass",
            f"Python {sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro} executed the diagnostics bridge.",
            sys.executable,
        ),
        settings_check,
        file_component_check("backend-core", "Conversion backend", PROJECT_ROOT / "character_mod_tool.py"),
        file_component_check("manifest-reader", "Manifest reader", PROJECT_ROOT / "character_manifest.py"),
        file_component_check("vcz-decoder", "VCZ geometry decoder", PROJECT_ROOT / "tools" / "nba2k27_exporter" / "Nba2kVczDecoder.exe"),
        file_component_check("vcz-texture-decoder", "VCZ texture decoder", PROJECT_ROOT / "tools" / "nba2k27_exporter" / "Nba2kVczTextureDecoder.exe"),
        file_component_check("texconv", "Texture converter", PROJECT_ROOT / "tools" / "texconv.exe"),
    ]
    blender_checks = [
        blender_check,
        file_component_check("head-swap-addon", "Head Swap extension", packaged_tools / "NBA_Character_HeadSwap" / "__init__.py"),
        file_component_check("mesh-transfer-addon", "Mesh Data Transfer extension", packaged_tools / "mesh_data_transfer" / "__init__.py"),
        file_component_check("headband-addon", "Headband Swap extension", packaged_tools / "NBA_Character_HeadbandSwap" / "__init__.py"),
    ]

    game_roots = {
        "NBA 2K18": app_settings.discover_nba2k18_root(),
        "NBA 2K19": app_settings.discover_nba2k19_root(),
        "NBA 2K20": app_settings.discover_nba2k20_root(),
        "NBA 2K23": app_settings.discover_nba2k23_root(),
        "NBA 2K25": __import__('nba2k25_support').discover_root(),
        "NBA 2K26": settings.get("game_root", ""),
        "NBA 2K27": settings.get("game_root_2k27", ""),
    }
    game_checks = [
        game_health_check(label, root, required=label == "NBA 2K27", modern=label in {"NBA 2K25", "NBA 2K26", "NBA 2K27"})
        for label, root in game_roots.items()
    ]

    backup_root = app_settings.user_data_dir() / "hair_picker_backups"
    backup_bytes = directory_size(backup_root)
    backup_status = "warning" if backup_bytes > 512 * 1024 * 1024 else "pass"
    backup_repair = (
        {"action": "prune-backups", "label": "Prune Old Backups"}
        if backup_status == "warning"
        else {"action": "open-path", "label": "Open Backups", "path": str(backup_root)}
    )
    groups = [
        {"id": "backend", "label": "Backend", "checks": backend_checks},
        {"id": "blender", "label": "Blender", "checks": blender_checks},
        {"id": "games", "label": "Games and manifests", "checks": game_checks},
        {"id": "storage", "label": "Output and retained data", "checks": [
            writable_directory_check("output-directory", "Output folder", settings.get("output_dir", "")),
            health_check(
                "hair-backups",
                "Hair install backups",
                backup_status,
                f"{backup_bytes / (1024 * 1024):.1f} MB retained; the limit is 512 MB.",
                backup_root,
                backup_repair,
            ),
        ]},
    ]
    overall, counts = summarize_health(groups)
    return {
        "generatedAt": datetime.now(timezone.utc).isoformat(),
        "overall": overall,
        "counts": counts,
        "groups": groups,
    }


def prune_directory(root, target_bytes):
    path = Path(root)
    if not path.exists():
        return {"removed": 0, "bytes": 0}
    files = []
    for candidate in path.rglob("*"):
        try:
            if candidate.is_file():
                stat = candidate.stat()
                files.append((stat.st_mtime_ns, stat.st_size, candidate))
        except OSError:
            continue
    total = sum(size for _modified, size, _candidate in files)
    removed = 0
    for _modified, size, candidate in sorted(files):
        if total <= target_bytes:
            break
        try:
            candidate.unlink(missing_ok=True)
            total -= size
            removed += 1
        except OSError:
            continue
    for directory in sorted((item for item in path.rglob("*") if item.is_dir()), key=lambda item: len(item.parts), reverse=True):
        try:
            directory.rmdir()
        except OSError:
            pass
    return {"removed": removed, "bytes": max(0, total)}


def command_health_repair(payload):
    action = str(payload.get("action") or "").strip()
    if action == "auto-detect-settings":
        app_settings.save_settings(fast_settings())
    elif action == "create-output":
        settings = fast_settings()
        output_dir = app_settings.ensure_output_dir(settings.get("output_dir", ""))
        if not output_dir:
            raise OSError("No writable output folder could be created.")
        settings["output_dir"] = output_dir
        app_settings.save_settings(settings)
    elif action == "clean-caches":
        shutil.rmtree(app_settings.user_data_dir() / "manifest-cache", ignore_errors=True)
    elif action == "prune-backups":
        prune_directory(app_settings.user_data_dir() / "hair_picker_backups", 384 * 1024 * 1024)
    else:
        raise ValueError(f"Unsupported health repair: {action}")
    return command_health({})


def command_save_settings(payload):
    current = fast_settings()
    for key in app_settings.DEFAULTS:
        if key in payload:
            current[key] = str(payload[key] or "")
    app_settings.save_settings(current)
    return {"settings": fast_settings()}


def command_inspect(payload):
    from character_package import inspect_character_package

    file_path = require_file(payload.get("filePath"), "Character package")
    return inspect_character_package(file_path, payload.get("stageRoot"))


def detect_modern_target_game(file_path):
    try:
        with zipfile.ZipFile(file_path, "r") as archive:
            for info in archive.infolist():
                if info.is_dir() or not info.filename.lower().endswith(".txtr"):
                    continue
                data = archive.read(info.filename)
                if re.search(rb"\.[0-9a-f]{16}\.tld", data, re.IGNORECASE):
                    return "NBA 2K27"
    except (OSError, ValueError, zipfile.BadZipFile):
        pass
    return "NBA 2K26"


def load_hair_backend():
    bundled_hair_tools = PROJECT_ROOT / "hair_tools"
    hair_tools_dir = (
        bundled_hair_tools
        if bundled_hair_tools.is_dir()
        else Path(app_settings.resource_path("hair_tools")).resolve()
    )
    backend_path = hair_tools_dir / "hair_picker.py"
    if not backend_path.is_file():
        raise FileNotFoundError(f"The bundled Hair backend is missing: {backend_path}")
    if str(hair_tools_dir) not in sys.path:
        sys.path.insert(0, str(hair_tools_dir))
    module_name = "character_mod_electron_hair_backend"
    module = sys.modules.get(module_name)
    if module is None:
        spec = importlib.util.spec_from_file_location(module_name, backend_path)
        if spec is None or spec.loader is None:
            raise RuntimeError("Could not load the bundled Hair backend.")
        module = importlib.util.module_from_spec(spec)
        sys.modules[module_name] = module
        resolved_settings = fast_settings()
        original_load_settings = app_settings.load_settings
        app_settings.load_settings = lambda: dict(resolved_settings)
        try:
            spec.loader.exec_module(module)
        finally:
            app_settings.load_settings = original_load_settings
    configure = getattr(module, "configure_environment", None)
    if configure:
        settings = fast_settings()
        configure(
            game_root=settings.get("game_root", ""),
            blender_exe=settings.get("blender_exe", ""),
            log_dir=str(app_settings.LOG_DIR),
        )
    return module


def identify_source_hair(path, backend, generation_hint=""):
    path = backend.Path(path)
    normalized_hint = re.sub(r"[^A-Z0-9]", "", str(generation_hint).upper())
    filename_match = re.fullmatch(
        r"(?i)png(\d+)_geo_(hair(?:_[A-Za-z0-9_]+)?)\.iff",
        path.name,
    )
    if normalized_hint in {"2K26", "NBA2K26"} and filename_match:
        source_png, source_key = filename_match.groups()
        if not source_key.lower().endswith("_tangentspace") and zipfile.is_zipfile(path):
            with zipfile.ZipFile(path, "r") as archive:
                scne_files = [
                    name for name in archive.namelist()
                    if name.lower().endswith(".scne")
                ]
            if len(scne_files) == 1:
                return "2K26", source_png, source_key
    generation, source_png, source_key = backend.external_hair_identity(
        path,
        generation_hint,
    )
    normalized_hint = re.sub(r"[^A-Z0-9]", "", str(generation_hint).upper())
    if normalized_hint in {"2K18", "NBA2K18"} and generation == "2K23":
        generation = "2K18"
    return generation, source_png, source_key


def appearance_hair_inventory(character_path):
    character_path = Path(character_path)
    png_match = re.search(r"(?i)png(\d+)", character_path.name)
    png_id = png_match.group(1) if png_match else ""
    with zipfile.ZipFile(character_path, "r") as archive:
        appearance_name = next(
            (
                name for name in archive.namelist()
                if name.lower().endswith("appearance_info.json")
            ),
            "",
        )
        if not appearance_name:
            return png_id, [], []
        document = json.loads(archive.read(appearance_name).decode("utf-8-sig"))

    accessory = document.get("accessory_items", {})
    item_defs = {
        item.get("name"): item
        for item in accessory.get("items", [])
        if isinstance(item, dict) and item.get("name")
    }
    default_config = str(accessory.get("default_config") or "")
    preferred_keys = []
    slots = {}
    configurations = sorted(
        accessory.get("configurations", []),
        key=lambda config: str(config.get("name") or "") != default_config,
    )
    for config in configurations:
        config_name = str(config.get("name") or "")
        for item_name in config.get("items", []):
            item = item_defs.get(item_name, {})
            if str(item.get("type") or "").lower() != "hair":
                continue
            key = str(item_name or "")
            if key and key not in preferred_keys:
                preferred_keys.append(key)
            slot = slots.setdefault(
                key,
                {
                    "key": key,
                    "name": str(item.get("name") or key),
                    "configs": [],
                    "tangentspace": str(item.get("tangentspace") or "").lower()
                    in {"yes", "both"},
                },
            )
            slot["configs"].append(config_name)

    public_slots = []
    for key, slot in sorted(slots.items()):
        configs = ", ".join(slot["configs"])
        default = " default" if default_config in slot["configs"] else ""
        public_slots.append(
            {
                "label": f"{key} ({configs}{default})",
                "key": key,
                "tangentspace": slot["tangentspace"],
                "default": default_config in slot["configs"],
            }
        )
    return png_id, preferred_keys, public_slots


def fast_2k26_hair_candidates(character_path):
    character = Path(character_path)
    png_id, preferred_keys, _slots = appearance_hair_inventory(character)
    filename_pattern = re.compile(
        rf"(?i)^png{re.escape(png_id)}_geo_(hair(?:_[A-Za-z0-9_]+)?)\.iff$"
    )
    candidates = []
    seen = set()

    def add(path):
        path = Path(path)
        match = filename_pattern.fullmatch(path.name)
        if not match or match.group(1).lower().endswith("_tangentspace"):
            return
        resolved = str(path.resolve()).casefold()
        if resolved in seen or not zipfile.is_zipfile(path):
            return
        with zipfile.ZipFile(path, "r") as archive:
            if sum(name.lower().endswith(".scne") for name in archive.namelist()) != 1:
                return
        seen.add(resolved)
        candidates.append(
            {
                "path": str(path),
                "name": path.name,
                "generation": "2K26",
                "sourcePng": png_id,
                "sourceKey": match.group(1),
            }
        )

    for key in preferred_keys:
        add(character.parent / f"png{png_id}_geo_{key}.iff")
    for path in sorted(character.parent.glob(f"png{png_id}_geo_hair*.iff")):
        add(path)
    return candidates


def source_hair_candidates(character_path, backend, generation_hint=""):
    character = backend.Path(character_path)
    png_match = re.search(r"(?i)png(\d+)", character.name)
    png_id = png_match.group(1) if png_match else ""
    preferred_keys = []
    try:
        summary = backend.parse_appearance_iff(character)
        png_id = summary.png_id or png_id
        ordered_configs = sorted(
            summary.configs,
            key=lambda config: config.name != summary.default_config,
        )
        for config in ordered_configs:
            for item in config.hair:
                key = item.get("asset_key") or item.get("name")
                if key and key not in preferred_keys:
                    preferred_keys.append(key)
    except Exception:
        pass

    candidates = []
    seen = set()

    def add_candidate(path):
        path = backend.Path(path)
        resolved = str(path.resolve()).casefold()
        if resolved in seen or not path.is_file() or path.stem.lower().endswith("_tangentspace"):
            return
        try:
            generation, source_png, source_key = identify_source_hair(
                path,
                backend,
                generation_hint,
            )
        except Exception:
            return
        seen.add(resolved)
        candidates.append(
            {
                "path": str(path),
                "name": path.name,
                "generation": generation,
                "sourcePng": source_png,
                "sourceKey": source_key,
            }
        )

    if png_id:
        for hair_key in preferred_keys:
            add_candidate(character.parent / f"png{png_id}_geo_{hair_key}.iff")
        for pattern in (f"png{png_id}_geo_hair*.iff", f"png{png_id}_hair*.iff"):
            for path in sorted(character.parent.glob(pattern)):
                add_candidate(path)
    return candidates


def command_hair_options(payload):
    source = require_file(payload.get("source"), "Source character")
    target = require_file(payload.get("target"), "Target character")
    explicit_source = str(payload.get("hairSource") or "").strip()
    source_game = str(payload.get("sourceGame") or "").strip()
    if source_game == "NBA 2K26":
        target_png, _preferred, slots = appearance_hair_inventory(target)
        candidates = fast_2k26_hair_candidates(source)
        if explicit_source:
            explicit_path = require_file(explicit_source, "Source hair")
            explicit = next(
                (
                    candidate for candidate in fast_2k26_hair_candidates(source)
                    if os.path.normcase(candidate["path"]) == os.path.normcase(explicit_path)
                ),
                None,
            )
            if explicit is None:
                raise ValueError("The selected file is not a regular NBA 2K26 hair geometry IFF.")
            candidates = [
                explicit,
                *[
                    candidate for candidate in candidates
                    if os.path.normcase(candidate["path"]) != os.path.normcase(explicit_path)
                ],
            ]
    else:
        backend = load_hair_backend()
        summary = backend.parse_appearance_iff(backend.Path(target))
        target_png = summary.png_id
        slots = [
            {
                "label": label,
                "key": key,
                "tangentspace": bool(tangent),
                "default": " default)" in label,
            }
            for label, key, tangent in backend.appearance_asset_slots(summary, "hair")
        ]
        candidates = source_hair_candidates(source, backend, source_game)
        if explicit_source:
            explicit_path = require_file(explicit_source, "Source hair")
            generation, source_png, source_key = identify_source_hair(
                backend.Path(explicit_path),
                backend,
                source_game,
            )
            explicit = {
                "path": explicit_path,
                "name": os.path.basename(explicit_path),
                "generation": generation,
                "sourcePng": source_png,
                "sourceKey": source_key,
            }
            candidates = [
                explicit,
                *[
                    candidate for candidate in candidates
                    if os.path.normcase(candidate["path"]) != os.path.normcase(explicit_path)
                ],
            ]

    target_game = str(payload.get("targetGame") or "").strip()
    if target_game not in {"NBA 2K26", "NBA 2K27"}:
        target_game = detect_modern_target_game(target)
    route = ""
    if candidates and slots:
        route = f"{candidates[0]['generation']} -> {target_game} native hair template"
    return {
        "candidates": candidates,
        "slots": slots,
        "targetPng": target_png,
        "targetGame": target_game,
        "sourceGame": source_game,
        "route": route,
    }


def command_facial_hair_options(payload):
    from hair_tools import legacy_facial_hair as facial

    source = require_file(payload.get("source"), "Source character")
    target = require_file(payload.get("target"), "Target character")
    source_game = str(payload.get("sourceGame") or "").strip()
    target_game = str(payload.get("targetGame") or "").strip() or detect_modern_target_game(target)
    result = dict(candidates=[], slots=[], sourceGame=source_game, targetGame=target_game, route="", message="")
    if source_game not in {"NBA 2K23", "NBA 2K25"} or target_game != "NBA 2K27":
        result["message"] = "Facial hair conversion supports NBA 2K23/2K25 to NBA 2K27."
        return result
    provider = facial
    if source_game == 'NBA 2K25':
        import nba2k25_facial_hair as provider
    generation = source_game.replace('NBA ', '')
    candidates = provider.source_candidates(source)
    explicit = str(payload.get("facialHairSource") or "").strip()
    if explicit:
        bundle = provider.source_bundle(source, require_file(explicit, "Source facial hair"))
        if source_game == 'NBA 2K25':
            provider.read_pair(bundle)
        else:
            facial.read_legacy(bundle["regular"])
        candidates = [dict(path=bundle["regular"], name=Path(bundle["regular"]).name,
                           generation=generation, sourcePng=bundle["sourcePng"], sourceKey=bundle["sourceKey"]),
                      *[item for item in candidates if os.path.normcase(item["path"]) != os.path.normcase(bundle["regular"])]]
    backend = load_hair_backend()
    summary = backend.parse_appearance_iff(backend.Path(target))
    slots = []
    rejected = []
    for label, key, _tangent in backend.appearance_asset_slots(summary, "facialhair"):
        try:
            templates = facial.target_bundle(target, key)
            facial.validate_templates(templates)
        except (ValueError, KeyError, OSError, StopIteration, zipfile.BadZipFile) as exc:
            rejected.append(dict(key=key, label=label, reason=str(exc) or type(exc).__name__))
            continue
        mode = facial.target_mode(templates)
        mode_label = {"both": "Regular + tangent", "yes": "Tangent only", "no": "Regular only"}[mode]
        slots.append(dict(label=f"{label} | {mode_label}", key=key, tangentspace=mode != "no",
                          geometryMode=mode, default=" default)" in label))
    result.update(candidates=candidates, slots=slots, rejectedSlots=rejected)
    if not candidates:
        result["message"] = ("No supported 2K25 paired or tangent-only facial hair with decoded textures found."
                             if source_game == 'NBA 2K25' else
                             f"No complete {generation} facial-hair geometry, tangent companion, and texture set found.")
    elif not slots:
        result["message"] = ("Target facial hair unavailable: " + "; ".join(f"{item['key']}: {item['reason']}" for item in rejected[:2])
                             if rejected else "The target appearance has no facial-hair slot.")
    else:
        result["route"] = f"{generation} -> NBA 2K27 facial hair, fitted to the swapped head"
    return result


def native_hair_backend():
    settings = fast_settings()
    root = game_root_for(settings, "NBA 2K27")
    backend = load_hair_backend()
    backend.configure_environment(
        game_root=root,
        blender_exe=settings.get("blender_exe", ""),
        log_dir=str(app_settings.LOG_DIR),
    )
    return backend


def command_native_hair_catalog(payload):
    asset_type = str(payload.get("assetType") or "hair").strip().lower()
    if asset_type not in {"hair", "facialhair"}:
        raise ValueError("Asset type must be hair or facialhair.")
    emit_event("progress", f"Reading NBA 2K27 {asset_type} manifest assets...")
    backend = native_hair_backend()
    options = (
        backend.discover_hairs()
        if asset_type == "hair"
        else backend.discover_facial_hairs()
    )
    _root, rows = catalog_rows("NBA 2K27")
    names_by_png = {row["png"]: row["name"] for row in rows}
    manifest_entries = backend.parse_manifest_entries()

    def preview_entry(virtual_path):
        if not virtual_path:
            return None
        record = manifest_entries.get(str(virtual_path).lower())
        if not record:
            return None
        name, archive, offset, size = record
        return {
            "name": name,
            "archive": archive,
            "offset": offset,
            "size": size,
        }

    assets = []
    descriptors = []
    for option in options:
        if not option.archive_geo:
            continue
        assets.append({
            "id": option.archive_geo,
            "sourcePng": option.source_png,
            "playerName": names_by_png.get(option.source_png, option.player_name or "Unknown"),
            "key": option.hair_key,
            "hasItem": bool(option.archive_item or option.source_item),
            "hasConfig": bool(option.archive_config or option.source_config),
            "tangentspace": bool(option.tangentspace),
            "origin": option.origin,
        })
        descriptors.append({
            "assetType": asset_type,
            "assetId": option.archive_geo,
            "sourcePng": option.source_png,
            "key": option.hair_key,
            "tangentspace": bool(option.tangentspace),
            "archiveGeo": option.archive_geo,
            "archiveItem": option.archive_item or "",
            "sourceGeo": str(option.source_geo or ""),
            "sourceItem": str(option.source_item or ""),
            "geoEntry": preview_entry(option.archive_geo),
            "itemEntry": preview_entry(option.archive_item),
            "playerEntry": preview_entry(f"char/sig/png{option.source_png}.iff"),
        })
    return {
        "assetType": asset_type,
        "gameRoot": str(backend.GAME_ROOT),
        "assets": assets,
        "previewDescriptors": descriptors,
    }


def selected_native_hair_option(backend, asset_type, asset_id):
    options = (
        backend.discover_hairs()
        if asset_type == "hair"
        else backend.discover_facial_hairs()
    )
    option = next(
        (
            candidate
            for candidate in options
            if os.path.normcase(str(candidate.archive_geo or ""))
            == os.path.normcase(asset_id)
        ),
        None,
    )
    if option is None:
        raise ValueError("The selected NBA 2K27 manifest asset is no longer available.")
    return option


def command_native_hair_preview(payload):
    asset_type = str(payload.get("assetType") or "hair").strip().lower()
    asset_id = str(payload.get("assetId") or "").strip()
    if asset_type not in {"hair", "facialhair"}:
        raise ValueError("Asset type must be hair or facialhair.")
    if not asset_id:
        raise ValueError("Choose a source asset from the NBA 2K27 manifest.")
    backend = native_hair_backend()
    descriptor = payload.get("previewDescriptor")
    if not isinstance(descriptor, dict) or str(descriptor.get("assetId") or "") != asset_id \
            or str(descriptor.get("assetType") or "").lower() != asset_type:
        descriptor = None
    if descriptor:
        source_png = str(descriptor.get("sourcePng") or "")
        if not source_png.isdigit():
            raise ValueError("The cached preview descriptor has an invalid source player.")
        source_geo = str(descriptor.get("sourceGeo") or "")
        source_item = str(descriptor.get("sourceItem") or "")
        geometry = Path(source_geo) if source_geo and Path(source_geo).is_file() else backend.ensure_extracted(
            str(descriptor.get("archiveGeo") or ""), required=True
        )
        item = Path(source_item) if source_item and Path(source_item).is_file() else backend.ensure_extracted(
            str(descriptor.get("archiveItem") or ""), required=False
        )
        hair_key = str(descriptor.get("key") or "")
        tangentspace = bool(descriptor.get("tangentspace"))
    else:
        option = selected_native_hair_option(backend, asset_type, asset_id)
        source_png = option.source_png
        geometry = option.source_geo or backend.ensure_extracted(option.archive_geo, required=True)
        item = option.source_item or backend.ensure_extracted(option.archive_item, required=False)
        hair_key = option.hair_key
        tangentspace = bool(option.tangentspace)
    source_player = backend.ensure_extracted(
        f"char/sig/png{source_png}.iff",
        required=True,
    )
    return {
        "assetType": asset_type,
        "sourcePng": source_png,
        "key": hair_key,
        "tangentspace": tangentspace,
        "geometryPath": str(geometry),
        "itemPath": str(item) if item else "",
        "sourcePlayerPath": str(source_player),
    }


def command_native_hair_target(payload):
    if payload.get('configScoped') and not payload.get('config'):
        raise ValueError('Select a configuration in the Configs tab first.')
    if payload.get('config'):
        import configs_backend
        _, appearance = configs_backend.read(payload['target'])
        config = configs_backend.config_named(appearance, payload['config'])
        return {'target': payload['target'], 'targetPng': Path(payload['target']).stem[3:],
                'slots': [{'key': '@config', 'label': config['name'], 'tangentspace': False}]}
    target = require_file(payload.get("target"), "Working player")
    asset_type = str(payload.get("assetType") or "hair").strip().lower()
    if asset_type not in {"hair", "facialhair"}:
        raise ValueError("Asset type must be hair or facialhair.")
    backend = native_hair_backend()
    summary = backend.parse_appearance_iff(backend.Path(target))
    return {
        "target": target,
        "targetPng": summary.png_id,
        "assetType": asset_type,
        "slots": [
            {
                "label": label,
                "key": key,
                "tangentspace": bool(tangentspace),
            }
            for label, key, tangentspace in backend.appearance_asset_slots(summary, asset_type)
        ],
    }


def command_native_hair_install(payload):
    if payload.get('configScoped') and not payload.get('config'):
        raise ValueError('Select a configuration in the Configs tab first.')
    if payload.get('config'):
        import configs_backend
        return configs_backend.install(payload, sys.modules[__name__])
    target = require_file(payload.get("target"), "Working player")
    asset_type = str(payload.get("assetType") or "hair").strip().lower()
    asset_id = str(payload.get("assetId") or "").strip()
    target_key = str(payload.get("targetKey") or "").strip()
    if asset_type not in {"hair", "facialhair"}:
        raise ValueError("Asset type must be hair or facialhair.")
    if not asset_id:
        raise ValueError("Choose a source asset from the NBA 2K27 manifest.")

    backend = native_hair_backend()
    summary = backend.parse_appearance_iff(backend.Path(target))
    slots = {
        key: bool(tangentspace)
        for _label, key, tangentspace in backend.appearance_asset_slots(summary, asset_type)
    }
    if target_key not in slots:
        raise ValueError(
            f"The working player's appearance does not contain {asset_type} slot "
            f"{target_key or '(none)'}."
        )

    option = selected_native_hair_option(backend, asset_type, asset_id)
    copy_item = asset_type == "facialhair" or bool(payload.get("copyItem", True))
    if asset_type == "facialhair" and not (option.source_item or option.archive_item):
        raise ValueError(
            f"The selected facial hair {option.hair_key} has no matching item texture package."
        )

    backend.TARGET_SIG_DIR = backend.Path(target).resolve().parent
    tangent_staging = None
    install_option = option
    try:
        if asset_type == "hair" and option.tangentspace:
            if not slots[target_key]:
                raise ValueError(
                    "This tangent-space-only hair requires a tangent-space target hair slot."
                )
            emit_event(
                "progress",
                f"Installing native tangent-space {option.hair_key} on staged png{summary.png_id}...",
            )
        elif asset_type == "hair":
            emit_event(
                "progress",
                f"Fitting {option.hair_key} to staged png{summary.png_id}...",
            )
            install_option, tangent_staging = backend.prepare_tangent_fitted_option(
                option,
                summary.png_id,
                status_callback=lambda message: emit_event("progress", message),
                target_head_path=target,
            )
        else:
            emit_event(
                "progress",
                f"Installing {option.hair_key} on staged png{summary.png_id}...",
            )
        target_geo, target_item, target_config, backups = backend.install_hair(
            install_option,
            summary.png_id,
            target_key,
            copy_item,
            False,
            slots[target_key],
        )
    finally:
        if tangent_staging:
            shutil.rmtree(tangent_staging, ignore_errors=True)

    outputs = [str(target_geo)]
    warnings = backend.audit_hair_mesh_references(target, [(target_geo, target_geo)])
    if target_item:
        outputs.append(str(target_item))
    if target_config:
        outputs.append(str(target_config))
    return {
        "target": target,
        "targetPng": summary.png_id,
        "assetType": asset_type,
        "sourcePng": option.source_png,
        "sourceKey": option.hair_key,
        "targetKey": target_key,
        "texturesCopied": bool(target_item),
        "warnings": warnings,
        "outputs": outputs,
        "backups": [str(path) for path in backups],
    }


def read_body_fit(path):
    from character_mod_tool import CharacterModTool, is_older_body_fit_key

    entry_name, appearance = CharacterModTool.read_appearance_from_archive(path)
    flat_body_fit = {
        key: value
        for key, value in appearance.items()
        if isinstance(key, str) and is_older_body_fit_key(key)
    }
    if isinstance(appearance.get("body_fit"), dict):
        body_fit = dict(appearance["body_fit"])
        for key, value in flat_body_fit.items():
            body_fit.setdefault(key, value)
    else:
        body_fit = flat_body_fit
    return entry_name, appearance, body_fit


def set_body_fit_value(appearance, key, value):
    from character_mod_tool import is_older_body_fit_key

    nested = appearance.get("body_fit")
    if isinstance(nested, dict) and key in nested:
        nested[key] = value
        return
    if key in appearance and isinstance(key, str) and is_older_body_fit_key(key):
        appearance[key] = value
        return
    raise ValueError(f"Body-fit field is not writable on this target: {key}")


def write_body_fit_archive(target, entry_name, appearance, backup_suffix, validation_label):
    from character_mod_tool import CharacterModTool, serialize_structured_entry

    with zipfile.ZipFile(target, "r") as source_archive:
        infos = source_archive.infolist()
        original_data = source_archive.read(entry_name)
        replacement_data = serialize_structured_entry(
            entry_name,
            original_data,
            appearance,
            False,
        )
        archive_data = {
            info.filename: b"" if info.is_dir() else source_archive.read(info.filename)
            for info in infos
        }

    target_path = Path(target)
    backup = target_path.with_name(f"{target_path.stem}_{backup_suffix}{target_path.suffix}")
    if not backup.exists():
        shutil.copy2(target_path, backup)
    temporary = target_path.with_name(f".{target_path.name}.{backup_suffix}.tmp")
    try:
        with zipfile.ZipFile(temporary, "w") as output:
            for info in infos:
                data = replacement_data if info.filename == entry_name else archive_data[info.filename]
                output.writestr(CharacterModTool.copied_zip_info(info), data)
        with zipfile.ZipFile(temporary, "r") as verification:
            bad_member = verification.testzip()
            if bad_member:
                raise RuntimeError(f"{validation_label} failed ZIP validation at {bad_member}.")
        os.replace(temporary, target_path)
    finally:
        if temporary.exists():
            temporary.unlink()
    return backup


def command_body_fit(payload):
    from character_mod_tool import CharacterModTool

    target = require_file(payload.get("target"), "Staged target player")
    source_value = str(payload.get("source") or "").strip()
    source = require_file(source_value, "Full Swap source player") if source_value else ""
    fallback_value = str(payload.get("fallbackSource") or "").strip()
    fallback_source = require_file(fallback_value, "Original target player") if fallback_value else ""
    target_entry, _target_appearance, target_body_fit = read_body_fit(target)
    source_entry = ""
    source_body_fit = {}
    source_role = "No comparison source"
    if source:
        source_entry, _source_appearance, source_body_fit = read_body_fit(source)
        source_body_fit = CharacterModTool.normalize_legacy_body_fit_fields(source_body_fit)
        source_role = "Source"
    if not source_body_fit and fallback_source:
        source = fallback_source
        source_entry, _source_appearance, source_body_fit = read_body_fit(fallback_source)
        source_body_fit = CharacterModTool.normalize_legacy_body_fit_fields(source_body_fit)
        source_role = "Original target baseline"
    return {
        "source": source,
        "target": target,
        "sourceEntry": source_entry,
        "targetEntry": target_entry,
        "sourceRole": source_role,
        "sourceBodyFit": source_body_fit,
        "targetBodyFit": target_body_fit,
    }


def command_body_fit_save(payload):
    target = require_file(payload.get("target"), "Staged target player")
    changes = payload.get("changes")
    if not isinstance(changes, dict) or not changes:
        raise ValueError("No body-fit edits were supplied.")
    if len(changes) > 512:
        raise ValueError("Too many body-fit edits were supplied.")

    entry_name, appearance, body_fit = read_body_fit(target)
    applied = {}
    for key, value in changes.items():
        if not isinstance(key, str) or key not in body_fit:
            raise ValueError(f"Body-fit field is not editable on this target: {key}")
        if isinstance(value, bool) or not isinstance(value, (int, float)):
            raise ValueError(f"Body-fit value must be numeric: {key}")
        numeric = float(value)
        if not math.isfinite(numeric):
            raise ValueError(f"Body-fit value must be finite: {key}")
        set_body_fit_value(appearance, key, numeric)
        body_fit[key] = numeric
        applied[key] = numeric

    backup = write_body_fit_archive(
        target,
        entry_name,
        appearance,
        "before_body_fit_edit",
        "Body-fit edit output",
    )
    return {
        "target": target,
        "backup": str(backup),
        "entry": entry_name,
        "changed": sorted(applied),
        "targetBodyFit": body_fit,
    }


def command_body_chest_fix(payload):
    from character_mod_tool import CharacterModTool

    target = require_file(payload.get("target"), "Staged target player")
    entry_name, appearance, body_fit = read_body_fit(target)
    if "chestTHIN" not in body_fit or "chestTHICK" not in body_fit:
        raise ValueError("The staged player has no chestTHIN/chestTHICK body-fit values.")
    old_thin = body_fit["chestTHIN"]
    old_thick = body_fit["chestTHICK"]
    new_thin, new_thick, thick_increase = CharacterModTool.calculate_chest_fix(
        old_thin,
        old_thick,
    )
    body_fit["chestTHIN"] = new_thin
    body_fit["chestTHICK"] = new_thick
    set_body_fit_value(appearance, "chestTHIN", new_thin)
    set_body_fit_value(appearance, "chestTHICK", new_thick)

    backup = write_body_fit_archive(
        target,
        entry_name,
        appearance,
        "before_chest_fix",
        "Chest-fix output",
    )

    return {
        "target": target,
        "backup": str(backup),
        "entry": entry_name,
        "oldThin": old_thin,
        "oldThick": old_thick,
        "newThin": new_thin,
        "newThick": new_thick,
        "thickIncrease": thick_increase,
    }


def headband_source_candidates(source):
    from character_mod_tool import CharacterModTool

    if not source:
        return []
    folder = os.path.dirname(source)
    candidates = []
    for path in sorted(Path(folder).glob("*.iff")):
        ready, _status, details = CharacterModTool.inspect_headband_iff(str(path), True)
        if ready:
            candidates.append({
                "path": str(path),
                "name": path.name,
                "details": details,
                "styleHint": "Fury Headband" if "fury" in path.stem.lower() else "Stock NBA 2K27 Headband",
            })
    return candidates


def headband_target_configs(target):
    from character_mod_tool import CharacterModTool

    _entry, appearance = CharacterModTool.read_appearance_from_archive(target)
    accessory = appearance.get("accessory_items")
    configurations = accessory.get("configurations") if isinstance(accessory, dict) else None
    if not isinstance(configurations, list):
        return [], ""
    names = [
        str(config.get("name"))
        for config in configurations
        if isinstance(config, dict) and config.get("name")
    ]
    default_name = str(accessory.get("default_config") or "")
    return names, default_name


def supported_headband_styles():
    from character_mod_tool import AVAILABLE_BUILT_IN_HEADBANDS

    return {
        name: style for name, style in AVAILABLE_BUILT_IN_HEADBANDS.items()
        if name not in DISABLED_HEADBAND_STYLES
    }


def command_headband_options(payload):

    target = require_file(payload.get("target"), "Staged player")
    source_value = str(payload.get("source") or "").strip()
    source = require_file(source_value, "Full Swap source player") if source_value else ""
    configs, default_config = headband_target_configs(target)
    candidates = headband_source_candidates(source)
    return {
        "target": target,
        "source": source,
        "sourceHeadbands": candidates,
        "configs": configs,
        "defaultConfig": default_config,
        "styles": list(supported_headband_styles()),
    }


def run_headband_add(app, target, style_name, config_name, color_hex, add_logo, upside_down, logo_position):
    import tkinter.messagebox as messagebox
    style = supported_headband_styles().get(style_name)
    if not style:
        raise ValueError("Choose a supported built-in headband style.")
    output_dir = os.path.dirname(target)
    app.settings["output_dir"] = output_dir
    app.headband_add_target_var.set(target)
    app.headband_add_style_var.set(style_name)
    app.headband_add_config_var.set(config_name)
    app.headband_color_hex_var.set(color_hex)
    app.headband_logo_enabled_var.set(add_logo)
    app.headband_logo_upside_down_var.set(upside_down)
    app.headband_logo_position_var.set(logo_position)

    messagebox.askyesno = lambda *_args, **_kwargs: True
    messagebox.showinfo = lambda *_args, **_kwargs: None

    def raise_headband_error(_title, message, **_kwargs):
        raise RuntimeError(str(message))

    messagebox.showerror = raise_headband_error
    emit_event("progress", f"Adding and fitting {style_name}...")
    app.add_headband_to_character()

    target_number = app.character_number_from_path(target)
    output_main = os.path.join(output_dir, f"png{target_number}.iff")
    output_headband = os.path.join(output_dir, f"png{target_number}{style['output_suffix']}")
    if not os.path.isfile(output_main) or not zipfile.is_zipfile(output_main):
        raise RuntimeError("Headband fitting did not produce a valid staged player IFF.")
    if not os.path.isfile(output_headband) or not zipfile.is_zipfile(output_headband):
        raise RuntimeError("Headband fitting did not produce a valid headband companion IFF.")
    return output_main, output_headband, style


def transfer_headband_shape(app, source_headband, target_headband):
    from character_mod_tool import HEADBAND_SWAP_BRIDGE

    source_headband = require_file(source_headband, "Source headband")
    ready, _status, details = app.inspect_headband_iff(source_headband, True)
    if not ready:
        raise ValueError(f"The source headband is not transferable: {details}")
    blender = app.full_swap_blender_var.get().strip() or app.find_blender_executable()
    local_addon = PROJECT_ROOT / "blender_tools" / "NBA_Character_HeadbandSwap"
    addon = (
        str(local_addon)
        if (local_addon / "__init__.py").is_file()
        else app.headband_tool_var.get().strip() or app.find_headband_swap_tool()
    )
    mesh_transfer = app.find_mesh_data_transfer_tool()
    if not os.path.isfile(blender):
        raise FileNotFoundError("Blender 5.1 was not found for Headband Swap.")
    if not os.path.isfile(os.path.join(addon, "__init__.py")):
        raise FileNotFoundError("The headband-capable Blender tool was not found.")
    if not os.path.isfile(os.path.join(mesh_transfer, "__init__.py")):
        raise FileNotFoundError("Mesh Data Transfer was not found.")
    transfer_bridge = Path(__file__).with_name("blender_headband_transfer.py")
    if not transfer_bridge.is_file():
        raise FileNotFoundError("The 2K27-compatible headband transfer bridge was not found.")
    temporary = os.path.join(
        os.path.dirname(target_headband),
        f".{Path(target_headband).stem}.source_transfer.tmp.iff",
    )
    emit_event("progress", f"Transferring {os.path.basename(source_headband)} in Blender...")
    try:
        result = subprocess.run(
            [
                blender,
                "--background",
                "--factory-startup",
                "--python",
                str(transfer_bridge),
                "--",
                "--source",
                source_headband,
                "--target",
                target_headband,
                "--output",
                temporary,
                "--addon",
                addon,
                "--mesh-data-transfer",
                mesh_transfer,
            ],
            cwd=os.path.dirname(HEADBAND_SWAP_BRIDGE),
            env={**os.environ, "CHARACTER_MOD_TOOL_ROOT": str(PROJECT_ROOT)},
            capture_output=True,
            text=True,
            errors="replace",
            timeout=300,
            creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
        )
        output = "\n".join(part for part in (result.stdout, result.stderr) if part)
        for line in output.splitlines():
            if line.startswith("CHARMOD_HEADBAND_STAGE|"):
                emit_event("progress", line.split("|", 1)[1])
        if result.returncode != 0 or not os.path.isfile(temporary) or not zipfile.is_zipfile(temporary):
            error = next(
                (line.split("|", 1)[1] for line in output.splitlines() if line.startswith("CHARMOD_HEADBAND_ERROR|")),
                "Blender did not create a valid transferred headband IFF.",
            )
            raise RuntimeError(error)
        os.replace(temporary, target_headband)
    finally:
        if os.path.exists(temporary):
            os.remove(temporary)


def command_headband_apply(payload):
    if str(payload.get("style") or "").strip() in DISABLED_HEADBAND_STYLES:
        raise ValueError("The stock NBA 2K26 headband is currently disabled.")
    if payload.get('configScoped') and not payload.get('_configIsolated'):
        import configs_backend
        return configs_backend.headband(payload, sys.modules[__name__])
    import character_mod_tool as tool_module
    from character_mod_tool import CharacterModTool

    target = require_file(payload.get("target"), "Staged player")
    mode = str(payload.get("mode") or "add").strip().lower()
    style_name = str(payload.get("style") or "Stock NBA 2K27 Headband").strip()
    config_name = str(payload.get("config") or "").strip()
    color_hex = str(payload.get("color") or "#FFFFFF").strip().upper()
    add_logo = bool(payload.get("addLogo", True))
    upside_down = bool(payload.get("upsideDown", False))
    logo_position = str(payload.get("logoPosition") or "Front Center").strip()
    if mode not in {"add", "transfer"}:
        raise ValueError("Unsupported headband operation.")
    if not re.fullmatch(r"#[0-9A-F]{6}", color_hex):
        raise ValueError("Headband color must use #RRGGBB format.")
    configs, default_config = headband_target_configs(target)
    if not config_name:
        config_name = default_config or (configs[0] if configs else "")
    if config_name not in configs:
        raise ValueError("Choose an existing target appearance configuration.")

    local_fit_script = PROJECT_ROOT / "hair_tools" / "blender_hair_autofit.py"
    if local_fit_script.is_file():
        tool_module.HEADBAND_SURFACE_FIT_SCRIPT = str(local_fit_script)

    app = CharacterModTool()
    app.withdraw()
    try:
        output_main, output_headband, style = run_headband_add(
            app,
            target,
            style_name,
            config_name,
            color_hex,
            add_logo,
            upside_down,
            logo_position,
        )
        source_headband = ""
        if mode == "transfer":
            source_headband = require_file(payload.get("sourceHeadband"), "Source headband")
            transfer_headband_shape(app, source_headband, output_headband)
        return {
            "mode": mode,
            "target": output_main,
            "headband": output_headband,
            "sourceHeadband": source_headband,
            "style": style_name,
            "assetName": style["asset_name"],
            "config": config_name,
            "color": color_hex,
        }
    finally:
        try:
            app.destroy()
        except Exception:
            pass


def command_catalog(payload):
    game_label = str(payload.get("gameLabel") or "")
    emit_event("progress", f"Reading {game_label} manifest...")
    root, rows = catalog_rows(game_label)
    def asset_count(row, key):
        count = len(row[key])
        if count:
            return count
        return "In PNG" if game_label == "NBA 2K18" and key == "hair" else 0

    public_rows = [
        {
            "png": row["png"],
            "name": row["name"],
            "support": len(row.get("face_files", row.get("configs", ()))),
            "packageFiles": len(row.get("files", row.get("companion_entries", ()))) + 1,
            "hair": asset_count(row, "hair"),
            "facialhair": asset_count(row, "facialhair"),
            "headband": len(row["headband"]),
            "companions": len(row.get("companion_entries", ())),
        }
        for row in rows
    ]
    return {
        "gameLabel": game_label,
        "gameRoot": root,
        "supportLabel": "Face" if game_label in {"NBA 2K18", "NBA 2K19", "NBA 2K20", "NBA 2K23"} else "Configs",
        "rows": public_rows,
    }


def extract_nba2k18_embedded_hair(main_path, png, work_dir):
    """Expose active embedded 2K18 head hair as an ordinary legacy source IFF."""
    generated = []
    with zipfile.ZipFile(main_path, "r") as source:
        source_names = source.namelist()
        names_by_lower = {name.lower(): name for name in source_names}
        texture_names = [
            name
            for name in source_names
            if name.lower().endswith((".txtr", ".dds"))
            and os.path.basename(name).lower().startswith("hair_")
        ]
        for scne_name in source_names:
            stem = Path(scne_name).stem
            normalized = stem.lower()
            if not scne_name.lower().endswith(".scne"):
                continue
            if normalized.endswith("_shell") or normalized == "hair_custom":
                continue
            if not normalized.startswith("hair_"):
                continue

            scne_data = source.read(scne_name)
            binary_names = re.findall(
                rb'"Binary"\s*:\s*"([^"\r\n]+)"',
                scne_data,
                flags=re.IGNORECASE,
            )
            if b'"Model"' not in scne_data or b'"Prim"' not in scne_data or not binary_names:
                continue

            referenced = []
            for encoded_name in binary_names:
                try:
                    binary_name = encoded_name.decode("utf-8")
                except UnicodeDecodeError:
                    continue
                actual_name = names_by_lower.get(binary_name.lower())
                if actual_name and actual_name not in referenced:
                    referenced.append(actual_name)
            if not referenced:
                continue

            geometry_path = os.path.join(work_dir, f"png{png}_geo_{stem}.iff")
            with zipfile.ZipFile(geometry_path, "w", compression=zipfile.ZIP_DEFLATED) as output:
                for member_name in (scne_name, *referenced):
                    output.writestr(source.getinfo(member_name), source.read(member_name))
            generated.append(geometry_path)

            readable_textures = []
            for member_name in texture_names:
                try:
                    readable_textures.append((member_name, source.read(member_name)))
                except (KeyError, RuntimeError, zipfile.BadZipFile):
                    continue
            if readable_textures:
                item_path = os.path.join(work_dir, f"png{png}_item_{stem}.iff")
                with zipfile.ZipFile(item_path, "w", compression=zipfile.ZIP_DEFLATED) as output:
                    for member_name, member_data in readable_textures:
                        output.writestr(source.getinfo(member_name), member_data)
                generated.append(item_path)

            emit_event(
                "progress",
                f"Found embedded NBA 2K18 hair: {stem}",
            )
    return generated


def extract_legacy_source_package(game_label, root, row, work_dir):
    from character_mod_tool import (
        build_nba2k23_archive_layout,
        load_nba2k23_clean_archive_index,
        read_nba2k20_clean_manifest_record,
        read_nba2k23_clean_manifest_record,
    )

    archive_layout = build_nba2k23_archive_layout(root)
    clean_sizes = None
    if game_label == "NBA 2K23":
        clean_sizes = load_nba2k23_clean_archive_index(os.path.join(root, "0A"))

    package_names = sorted(
        set(row.get("png_files", ())) | set(row.get("face_files", ())),
        key=str.casefold,
    )
    exported = []
    for index, name in enumerate(package_names, start=1):
        emit_event(
            "progress",
            f"Extracting {game_label} file {index} of {len(package_names)}: {name}",
        )
        record = row["manifest_records"][name.casefold()]
        if clean_sizes is None:
            data = read_nba2k20_clean_manifest_record(record, archive_layout)
        else:
            data = read_nba2k23_clean_manifest_record(
                record,
                archive_layout,
                clean_sizes,
            )
        destination = os.path.join(work_dir, os.path.basename(name))
        temporary = f"{destination}.tmp"
        try:
            with open(temporary, "wb") as handle:
                handle.write(data)
            os.replace(temporary, destination)
        finally:
            if os.path.exists(temporary):
                os.remove(temporary)
        if not zipfile.is_zipfile(destination):
            raise ValueError(f"{game_label} exported an invalid IFF archive for {name}.")
        exported.append(destination)

    main_name = os.path.basename(row["archive_entry"])
    main_path = os.path.join(work_dir, main_name)
    if not os.path.isfile(main_path):
        raise FileNotFoundError(f"The extracted source did not contain {main_name}.")
    if game_label == "NBA 2K18":
        exported.extend(
            extract_nba2k18_embedded_hair(main_path, row["png"], work_dir)
        )
    companions = [path for path in exported if os.path.normcase(path) != os.path.normcase(main_path)]
    return main_path, companions, []


def command_extract(payload):
    game_label = str(payload.get("gameLabel") or "")
    png = str(payload.get("png") or "")
    stage_root = os.path.abspath(str(payload.get("stageRoot") or tempfile.gettempdir()))
    root, rows = catalog_rows(game_label)
    row = next((candidate for candidate in rows if candidate["png"] == png), None)
    if row is None:
        raise ValueError(f"png{png} was not found in the {game_label} manifest.")
    work_dir = tempfile.mkdtemp(prefix=f"png{png}_", dir=stage_root)
    decoder = ""
    if game_label == "NBA 2K27":
        for filename in ("Nba2kVczTextureDecoder.exe", "Nba2kVczDecoder.exe"):
            candidate = app_settings.resource_path(
                "tools", "nba2k27_exporter", filename
            )
            if os.path.isfile(candidate):
                decoder = candidate
                break
    try:
        emit_event("progress", f"Extracting {game_label} png{png}...")
        if game_label in {"NBA 2K18", "NBA 2K19", "NBA 2K20", "NBA 2K23"}:
            main_path, companions, failures = extract_legacy_source_package(
                game_label,
                root,
                row,
                work_dir,
            )
        else:
            main_path, companions, failures = extract_character_manifest_package(
                root,
                row,
                work_dir,
                include_configs=True,
                native_decoder=decoder,
            )
        if game_label == "NBA 2K25":
            from nba2k25_support import prepare_package
            prepared = prepare_package(main_path, work_dir, root,
                                       lambda message: emit_event("progress", message))
            emit_event("progress", f"Prepared {prepared['decodedBuffers']} NBA 2K25 shared buffers.")
    except Exception:
        shutil.rmtree(work_dir, ignore_errors=True)
        raise
    return {
        "gameLabel": game_label,
        "png": png,
        "name": row["name"],
        "mainPath": main_path,
        "workDir": work_dir,
        "companions": len(companions),
        "warnings": [f"{entry}: {error}" for entry, error in failures],
    }


def command_run_swap(payload):
    if str(payload.get("sourceGame") or "").strip() != "NBA 2K25":
        return _command_run_swap(payload)
    from nba2k25_support import prepare_iff, prepare_package, prepare_target_hair, SharedBuffers
    source = require_file(payload.get("source"), "Source character")
    root = game_root_for(fast_settings(), "NBA 2K25")
    target_game = str(payload.get('targetGame') or '').strip() or detect_modern_target_game(
        require_file(payload.get('target'), 'Target character'))
    if target_game != 'NBA 2K27':
        raise ValueError('NBA 2K25 source conversion currently targets NBA 2K27 only.')
    with tempfile.TemporaryDirectory(prefix="character-mod-2k25-source-") as folder:
        prepared = prepare_package(source, folder, root,
                                   lambda message: emit_event("progress", message))
        payload = dict(payload, source=prepared['source'])
        if payload.get("hairEnabled"):
            hair = Path(require_file(payload.get("hairSource"), "Source hair"))
            destination = Path(folder) / hair.name
            if not destination.is_file():
                prepare_iff(hair, destination, SharedBuffers(root))
            payload['hairSource'] = str(destination)
            target = Path(require_file(payload.get('target'), 'Target character'))
            target_folder = Path(folder) / 'target'
            target_folder.mkdir()
            for companion in target.parent.glob(target.stem + '_*.iff'):
                shutil.copy2(companion, target_folder / companion.name)
            prepared_target = target_folder / target.name
            prepare_target_hair(target, prepared_target, str(payload.get('hairTargetKey') or ''))
            payload['target'] = str(prepared_target)
            emit_event('progress', 'Selected hair uses its highest-detail mesh at every distance.')
        if payload.get('facialHairEnabled'):
            from nba2k25_facial_hair import installed_converter
            beard = Path(require_file(payload.get('facialHairSource'), 'Source facial hair'))
            prepared_beard = Path(folder) / beard.name
            if not prepared_beard.is_file():
                prepare_iff(beard, prepared_beard, SharedBuffers(root))
            payload['facialHairSource'] = str(prepared_beard)
            target = Path(require_file(payload.get('target'), 'Target character'))
            target_folder = Path(folder) / 'target'
            if not target_folder.is_dir():
                target_folder.mkdir()
                for companion in target.parent.glob(target.stem + '_*.iff'):
                    shutil.copy2(companion, target_folder / companion.name)
            prepared_target = target_folder / target.name
            prepare_target_hair(target, prepared_target, str(payload.get('facialHairTargetKey') or ''), 'facialhair')
            payload['target'] = str(prepared_target)
            emit_event('progress', 'Full Swap route: NBA 2K25 -> NBA 2K27, including fitted facial hair.')
            with installed_converter():
                return _command_run_swap(payload)
        emit_event("progress", "Full Swap route: NBA 2K25 -> NBA 2K27 modern head/body transfer.")
        return _command_run_swap(payload)


def _command_run_swap(payload):
    source = require_file(payload.get("source"), "Source character")
    target = require_file(payload.get("target"), "Target character")
    output_dir = os.path.abspath(str(payload.get("outputDir") or ""))
    if not output_dir:
        raise ValueError("Choose an output folder.")
    os.makedirs(output_dir, exist_ok=True)

    import tkinter.messagebox as messagebox
    from character_mod_tool import CharacterModTool

    app_settings.ensure_output_dir = fast_output_dir
    app = CharacterModTool()
    app.withdraw()
    app.settings["output_dir"] = output_dir
    app.everything_swap_source_var.set(source)
    app.everything_swap_target_var.set(target)
    source_game = str(payload.get("sourceGame") or "").strip()
    app.everything_swap_source_game_label = source_game
    requested_target_game = str(payload.get("targetGame") or "").strip()
    target_game = requested_target_game
    supported_target_games = {"NBA 2K23", "NBA 2K26", "NBA 2K27"}
    if target_game not in supported_target_games:
        target_game = detect_modern_target_game(target)
    if target_game == "NBA 2K23" and source_game != "NBA 2K18":
        raise ValueError("NBA 2K23 targets currently require an NBA 2K18 source player.")
    if target_game == "NBA 2K23" and (
        bool(payload.get("hairEnabled", False))
        or bool(payload.get("facialHairEnabled", False))
    ):
        raise ValueError(
            "Hair and facial-hair conversion are not yet supported for NBA 2K18 to NBA 2K23. "
            "Turn those options off to preserve the target player's accessories."
        )
    if target_game in supported_target_games:
        app.everything_swap_target_game_var.set(target_game)
        electron_stage = os.path.join(
            tempfile.gettempdir(),
            "CharacterModToolElectron",
        )
        target_is_manifest_selection = bool(requested_target_game)
        try:
            target_is_manifest_selection = target_is_manifest_selection or (
                os.path.commonpath((electron_stage, target)) == electron_stage
            )
        except ValueError:
            pass
        if target_is_manifest_selection:
            app.everything_swap_manifest_target_path = target
    app.everything_swap_shrinkwrap_var.set(bool(payload.get("shrinkwrapBody", True)))
    app.everything_swap_facial_hair_plan = None
    if bool(payload.get("facialHairEnabled", False)):
        from hair_tools import legacy_facial_hair as facial
        provider = facial
        if source_game == 'NBA 2K25':
            import nba2k25_facial_hair as provider

        options = command_facial_hair_options(dict(payload, targetGame=target_game))
        key = str(payload.get("facialHairTargetKey") or "").strip()
        if not any(slot["key"] == key for slot in options["slots"]):
            raise ValueError(options["message"] or "Choose a supported existing target facial-hair slot.")
        beard = require_file(payload.get("facialHairSource"), "Source facial hair")
        app.everything_swap_facial_hair_plan = dict(
            source=provider.source_bundle(source, beard),
            templates=facial.target_bundle(target, key), target_key=key,
        )
        emit_event("progress", f"Facial hair route: {source_game} -> {target_game} {key}; fitting after Blender.")
    hair_enabled = bool(payload.get("hairEnabled", False))
    app.everything_swap_hair_enabled_var.set(hair_enabled)
    if hair_enabled:
        hair_source = require_file(payload.get("hairSource"), "Source hair")
        hair_target_key = str(payload.get("hairTargetKey") or "").strip()
        backend = load_hair_backend()
        generation, _source_png, source_key = identify_source_hair(
            backend.Path(hair_source),
            backend,
            source_game,
        )
        target_summary = backend.parse_appearance_iff(backend.Path(target))
        target_keys = {
            key for _label, key, _tangent in backend.appearance_asset_slots(target_summary, "hair")
        }
        if hair_target_key not in target_keys:
            raise ValueError(
                f"The target appearance does not contain hair slot {hair_target_key or '(none)'}."
            )
        app.everything_swap_hair_source_var.set(hair_source)
        app.everything_swap_hair_slot_map = {hair_target_key: hair_target_key}
        app.everything_swap_hair_slot_var.set(hair_target_key)
        emit_event(
            "progress",
            f"Hair route: {generation} {source_key} -> {target_game} {hair_target_key}.",
        )
    from hair_tools.accessory_output_names import make_plan as accessory_name_plan

    name_requests = []
    if hair_enabled:
        name_requests.append((hair_target_key, payload.get("hairOutputName"), "hair"))
    if app.everything_swap_facial_hair_plan:
        name_requests.append((app.everything_swap_facial_hair_plan["target_key"],
                              payload.get("facialHairOutputName"), "facialhair"))
    app.everything_swap_accessory_name_plan = accessory_name_plan(target, name_requests)
    rename_id = str(payload.get("renameId") or "").strip()
    app.everything_swap_rename_enabled_var.set(bool(rename_id))
    app.everything_swap_rename_name_var.set(rename_id)
    app.full_swap_bridge_result = "Connected (Electron)"
    app.confirm_full_swap_output_plan = lambda *_args, **_kwargs: True

    state = {"done": False, "result": None, "error": ""}

    def finish_success(message):
        if state["done"]:
            return
        state["done"] = True
        state["result"] = {
            "output": app.last_full_swap_output,
            "message": str(message),
            "validation": app.last_validation_report,
            "facialHair": getattr(app, "everything_swap_facial_hair_result", None),
        }
        emit_event("progress", "Full Swap completed.", progress=1)
        app.after_idle(app.destroy)

    def finish_error(message):
        if state["done"]:
            return
        state["done"] = True
        state["error"] = str(message)
        emit_event("error", state["error"])
        app.after_idle(app.destroy)

    def show_info(_title, message, **_kwargs):
        text = str(message)
        if source_game == 'NBA 2K25':
            text = text.replace('Facial hair: 2K23 ->', 'Facial hair: 2K25 ->')
        emit_event("log", text)
        if text.startswith("Full Swap completed successfully"):
            finish_success(text)

    def show_error(_title, message, **_kwargs):
        finish_error(message)

    messagebox.showinfo = show_info
    messagebox.showwarning = lambda _title, message, **_kwargs: emit_event("warning", message)
    messagebox.showerror = show_error
    messagebox.askyesno = lambda *_args, **_kwargs: False

    emit_event("progress", "Preparing Full Swap...", progress=0.02)
    app.run_everything_swap()
    if app.full_swap_progress_window is not None:
        app.full_swap_progress_window.withdraw()
    if getattr(app, "full_swap_progress_var", None) is not None:
        last_stage = {"value": ""}

        def progress_changed(*_args):
            value = app.full_swap_progress_var.get()
            if source_game == 'NBA 2K25':
                value = value.replace('Fitting 2K23 facial hair', 'Fitting 2K25 facial hair')
            if value and value != last_stage["value"]:
                last_stage["value"] = value
                emit_event("progress", value)

        app.full_swap_progress_var.trace_add("write", progress_changed)

    if not app.everything_swap_active and app.full_swap_process is None and not state["done"]:
        finish_error("The Full Swap job did not start. Check the source textures and target configs.")

    def cancel_job(_signum, _frame):
        emit_event("warning", "Cancelling conversion...")
        try:
            app.after(0, app.cancel_full_swap)
        except Exception:
            pass

    signal.signal(signal.SIGTERM, cancel_job)
    app.mainloop()
    if state["error"]:
        raise RuntimeError(state["error"])
    if not state["result"]:
        raise RuntimeError("The Full Swap job ended without a result.")
    return state["result"]


COMMANDS = {
    "configs-list": lambda payload: __import__('configs_backend').inspect(payload['target']),
    "configs-change": lambda payload: __import__('configs_backend').change(payload),
    "bootstrap": command_bootstrap,
    "health": command_health,
    "health-repair": command_health_repair,
    "save-settings": command_save_settings,
    "inspect": command_inspect,
    "catalog": command_catalog,
    "extract": command_extract,
    "hair-options": command_hair_options,
    "facial-hair-options": command_facial_hair_options,
    "native-hair-catalog": command_native_hair_catalog,
    "native-hair-preview": command_native_hair_preview,
    "native-hair-target": command_native_hair_target,
    "native-hair-install": command_native_hair_install,
    "body-fit": command_body_fit,
    "body-fit-save": command_body_fit_save,
    "body-chest-fix": command_body_chest_fix,
    "headband-options": command_headband_options,
    "headband-apply": command_headband_apply,
    "run-swap": command_run_swap,
}


def main():
    command = sys.argv[1] if len(sys.argv) > 1 else ""
    handler = COMMANDS.get(command)
    if handler is None:
        raise ValueError(f"Unknown bridge command: {command}")
    result = handler(read_payload())
    emit_result(result)


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        emit_result({"error": str(exc), "traceback": traceback.format_exc()})
        raise SystemExit(1)
