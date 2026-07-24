import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import tkinter as tk
import zipfile
import ctypes
import io
import struct
import zlib
from dataclasses import dataclass, replace
from datetime import datetime
from pathlib import Path
from tkinter import filedialog
from tkinter import messagebox, ttk

try:
    import app_settings
except ImportError:
    sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
    import app_settings

from convert_2k23_hair_to_2k26_lod0 import convert as convert_legacy_hair
from convert_2k23_hair_to_2k26_lod0 import read_scne as read_legacy_scne
from convert_2k25_hair_to_2k26_static import convert as convert_2k25_hair
from convert_2k25_hair_to_2k26_static import read_scne as read_2k25_scne


_SETTINGS = app_settings.load_settings()
GAME_ROOT = Path(_SETTINGS["game_root"]) if _SETTINGS.get("game_root") else Path("__NBA2K26_NOT_CONFIGURED__")
SOURCE_SIG_DIRS = [
]
TARGET_SIG_DIR = GAME_ROOT / "mods" / "char" / "sig"
WORKSPACE_BACKUP_DIR = app_settings.user_data_dir() / "hair_picker_backups"
MANIFEST_PATH = GAME_ROOT / "manifest"
MOD_EXE = GAME_ROOT / "mod.exe"
OODLE_DLL = GAME_ROOT / "data" / "oodle" / "oo2core_9_win64.dll"
_MANIFEST_CACHE_KEY = None
_MANIFEST_CACHE = None
PLAYER_NAMES_PATH = Path(__file__).resolve().parent / "player_names.json"
BLENDER_EXE = Path(_SETTINGS["blender_exe"]) if _SETTINGS.get("blender_exe") else Path("__BLENDER_NOT_CONFIGURED__")
BLENDER_IMPORT_SCRIPT = Path(__file__).resolve().parent / "blender_hair_import.py"
BLENDER_AUTOFIT_SCRIPT = Path(__file__).resolve().parent / "blender_hair_autofit.py"
app_settings.LOG_DIR.mkdir(parents=True, exist_ok=True)
BLENDER_AUTOFIT_LOG = app_settings.LOG_DIR / "hair_fixer_blender.log"
HAIR_PICKER_VERSION = "1.0.7 External Conversion"


def configure_environment(game_root="", blender_exe="", log_dir=""):
    global GAME_ROOT, TARGET_SIG_DIR, MANIFEST_PATH, MOD_EXE, OODLE_DLL, BLENDER_EXE, BLENDER_AUTOFIT_LOG
    global _MANIFEST_CACHE_KEY, _MANIFEST_CACHE
    GAME_ROOT = Path(game_root) if game_root else Path("__NBA2K26_NOT_CONFIGURED__")
    TARGET_SIG_DIR = GAME_ROOT / "mods" / "char" / "sig"
    MANIFEST_PATH = GAME_ROOT / "manifest"
    MOD_EXE = GAME_ROOT / "mod.exe"
    OODLE_DLL = GAME_ROOT / "data" / "oodle" / "oo2core_9_win64.dll"
    _MANIFEST_CACHE_KEY = None
    _MANIFEST_CACHE = None
    BLENDER_EXE = Path(blender_exe) if blender_exe else Path("__BLENDER_NOT_CONFIGURED__")
    if log_dir:
        BLENDER_AUTOFIT_LOG = Path(log_dir) / "hair_fixer_blender.log"
    BLENDER_AUTOFIT_LOG.parent.mkdir(parents=True, exist_ok=True)


GEO_RE = re.compile(r"^png(?P<png>\d+)_geo_(?P<hair>hair_.+)\.iff$", re.IGNORECASE)
FACIAL_GEO_RE = re.compile(r"^png(?P<png>\d+)_geo_(?P<facial>facialhair_.+)\.iff$", re.IGNORECASE)


@dataclass
class HairOption:
    source_geo: Path | None
    source_item: Path | None
    source_config: Path | None
    archive_geo: str | None
    archive_item: str | None
    archive_config: str | None
    source_png: str
    player_name: str
    hair_key: str
    root_name: str
    lod_verts: int | None
    radius: float | None
    center_y: float | None
    origin: str
    asset_type: str
    tangentspace: bool

    @property
    def label(self) -> str:
        item = " + item" if self.source_item else ""
        if self.archive_item and not self.source_item:
            item = " + item"
        config = " + config" if self.source_config or self.archive_config else ""
        verts = f"{self.lod_verts:,} verts" if self.lod_verts else "unknown verts"
        y = f"Y {self.center_y:.1f}" if self.center_y is not None else "Y ?"
        tangent = " + tangentspace" if self.tangentspace else ""
        player = f" - {self.player_name}" if self.player_name else ""
        return f"[{self.origin}] png{self.source_png}{player} - {self.hair_key}{tangent}{item}{config} | {verts} | {y}"


@dataclass
class AppearanceConfig:
    name: str
    hair: list[dict]
    facialhair: list[dict]
    other: list[dict]


@dataclass
class AppearanceSummary:
    source_path: Path
    png_id: str
    default_config: str
    configs: list[AppearanceConfig]


def read_scne_summary(path: Path):
    try:
        with zipfile.ZipFile(path) as zf:
            scne_names = [name for name in zf.namelist() if name.lower().endswith(".scne")]
            if not scne_names:
                return "", None, None, None
            text = zf.read(scne_names[0]).decode("utf-8-sig")
            obj = json.loads("{" + text + "}")
            root = next(iter(obj))
            node = obj[root].get("Model", {}).get("hihead", {})
            lods = node.get("Lods", [])
            lod_verts = lods[0].get("LodVerts") if lods else None
            radius = node.get("Radius")
            center = node.get("Center") or []
            center_y = center[1] if len(center) > 1 else None
            return root, lod_verts, radius, center_y
    except Exception:
        return "", None, None, None


def item_asset_key(item_name: str):
    if item_name.startswith("facialHair"):
        return "facialhair" + item_name[len("facialHair") :]
    return item_name


def parse_appearance_iff(path: Path):
    path = Path(path)
    if not path.exists():
        raise FileNotFoundError(path)
    png_match = re.search(r"png(\d+)", path.name, re.IGNORECASE)
    png_id = png_match.group(1) if png_match else ""
    with zipfile.ZipFile(path) as zf:
        names = zf.namelist()
        match_names = [name for name in names if name.lower().endswith("appearance_info.json")]
        if not match_names:
            raise ValueError(f"No appearance_info.json found inside {path.name}")
        data = json.loads(zf.read(match_names[0]).decode("utf-8-sig"))

    accessory = data.get("accessory_items", {})
    item_defs = {item.get("name"): item for item in accessory.get("items", []) if item.get("name")}
    configs: list[AppearanceConfig] = []
    for config in accessory.get("configurations", []):
        hair_items = []
        facial_items = []
        other_items = []
        for item_name in config.get("items", []):
            item = item_defs.get(item_name, {"name": item_name, "type": "unknown"})
            enriched = dict(item)
            enriched["asset_key"] = item_asset_key(item_name)
            item_type = str(item.get("type", "")).lower()
            if item_type == "hair":
                hair_items.append(enriched)
            elif item_type == "facialhair":
                facial_items.append(enriched)
            else:
                other_items.append(enriched)
        configs.append(
            AppearanceConfig(
                name=config.get("name", ""),
                hair=hair_items,
                facialhair=facial_items,
                other=other_items,
            )
        )

    return AppearanceSummary(
        source_path=path,
        png_id=png_id,
        default_config=accessory.get("default_config", ""),
        configs=configs,
    )


def format_appearance_summary(summary: AppearanceSummary):
    lines = [
        f"Imported: {summary.source_path}",
        f"PNG ID: {summary.png_id or '?'}",
        f"Default config: {summary.default_config or '?'}",
        "",
        "Configs:",
    ]
    for config in summary.configs:
        default_mark = " (default)" if config.name == summary.default_config else ""
        lines.append(f"- {config.name}{default_mark}")
        if config.hair:
            for item in config.hair:
                lines.append(f"  Hair: {item.get('name')} -> geo_{item.get('asset_key')}")
        else:
            lines.append("  Hair: none")
        if config.facialhair:
            for item in config.facialhair:
                suffix = "_tangentspace" if item.get("tangentspace") == "yes" else ""
                lines.append(f"  Facial hair: {item.get('name')} -> geo_{item.get('asset_key')}{suffix}")
        else:
            lines.append("  Facial hair: none")
        if config.other:
            other = ", ".join(f"{item.get('type')}:{item.get('name')}" for item in config.other)
            lines.append(f"  Other: {other}")
    return "\n".join(lines)


def appearance_asset_slots(summary: AppearanceSummary, asset_type: str):
    slots: dict[str, dict] = {}
    for config in summary.configs:
        items = config.hair if asset_type == "hair" else config.facialhair
        for item in items:
            key = item.get("asset_key") or item.get("name")
            if not key:
                continue
            slot = slots.setdefault(
                key,
                {
                    "key": key,
                    "name": item.get("name", key),
                    "configs": [],
                    "tangentspace": item.get("tangentspace") == "yes",
                },
            )
            slot["configs"].append(config.name)
    result = []
    for key, slot in sorted(slots.items()):
        configs = ", ".join(slot["configs"])
        default = " default" if summary.default_config in slot["configs"] else ""
        label = f"{key} ({configs}{default})"
        result.append((label, key, slot["tangentspace"]))
    return result


def discover_hairs():
    return discover_archive_assets("hair")


def discover_facial_hairs():
    return discover_archive_assets("facialhair")


def config_suffix_from_hair_key(hair_key: str):
    return hair_key.removeprefix("hair_")


def find_loose_config(sig_dir: Path, png: str, hair_key: str):
    suffix = config_suffix_from_hair_key(hair_key)
    exact = sig_dir / f"png{png}_config_{suffix}.iff"
    if exact.exists():
        return exact
    configs = sorted(sig_dir.glob(f"png{png}_config_*.iff"))
    if len(configs) == 1:
        return configs[0]
    for config in configs:
        if suffix.lower() in config.stem.lower():
            return config
    return None


def parse_manifest_entries():
    global _MANIFEST_CACHE_KEY, _MANIFEST_CACHE
    entries = {}
    if not MANIFEST_PATH.exists():
        return entries
    stat = MANIFEST_PATH.stat()
    cache_key = (str(MANIFEST_PATH.resolve()), stat.st_mtime_ns, stat.st_size)
    if _MANIFEST_CACHE_KEY == cache_key and _MANIFEST_CACHE is not None:
        return _MANIFEST_CACHE
    for line in MANIFEST_PATH.read_text(errors="replace").splitlines():
        parts = line.split(",")
        if len(parts) != 4:
            continue
        name, archive_id, offset, size = parts
        entries[name.lower()] = (name, archive_id, int(offset), int(size))
    _MANIFEST_CACHE_KEY = cache_key
    _MANIFEST_CACHE = entries
    return entries


def load_player_names():
    try:
        data = json.loads(PLAYER_NAMES_PATH.read_text(encoding="utf-8"))
        return {str(key): str(value) for key, value in data.items() if value}
    except (OSError, ValueError, TypeError):
        return {}


def discover_archive_assets(asset_type: str):
    manifest = parse_manifest_entries()
    player_names = load_player_names()
    item_names = {
        entry[0].lower(): entry[0]
        for entry in manifest.values()
        if "_item_hair" in entry[0] or "_item_facialhair" in entry[0]
    }
    configs_by_png: dict[str, list[str]] = {}
    for name, _archive_id, _offset, _size in manifest.values():
        match = re.match(r"^char/sig/png(\d+)_config_.+\.iff$", name)
        if match:
            configs_by_png.setdefault(match.group(1), []).append(name)

    options: list[HairOption] = []
    for key, (name, _archive_id, _offset, _size) in manifest.items():
        if not name.startswith("char/sig/"):
            continue
        filename = Path(name).name
        tangentspace = False
        if asset_type == "hair":
            if "_geo_hair" not in name or name.endswith("_tangentspace.iff"):
                continue
            match = GEO_RE.match(filename)
            if not match:
                continue
            png = match.group("png")
            hair_key = match.group("hair")
        else:
            if "_geo_facialhair" not in name:
                continue
            match = FACIAL_GEO_RE.match(filename)
            if not match:
                continue
            png = match.group("png")
            hair_key = match.group("facial")
            if hair_key.lower().endswith("_tangentspace"):
                hair_key = hair_key[: -len("_tangentspace")]
                tangentspace = True
            if not tangentspace:
                continue
        item_name = f"char/sig/png{png}_item_{hair_key}.iff"
        config = find_archive_config_from_index(configs_by_png, png, hair_key) if asset_type == "hair" else None
        options.append(
            HairOption(
                source_geo=None,
                source_item=None,
                source_config=None,
                archive_geo=name,
                archive_item=item_names.get(item_name.lower()),
                archive_config=config,
                source_png=png,
                player_name=player_names.get(str(int(png)), ""),
                hair_key=hair_key,
                root_name="archive",
                lod_verts=None,
                radius=None,
                center_y=None,
                origin="archive",
                asset_type=asset_type,
                tangentspace=tangentspace,
            )
        )
    options.sort(
        key=lambda option: (
            int(option.source_png),
            option.hair_key.lower(),
            (option.archive_geo or "").lower(),
        )
    )
    return options


def find_archive_config_from_index(configs_by_png: dict[str, list[str]], png: str, hair_key: str):
    suffix = config_suffix_from_hair_key(hair_key)
    exact = f"char/sig/png{png}_config_{suffix}.iff"
    configs = configs_by_png.get(png, [])
    for config in configs:
        if config.lower() == exact.lower():
            return config
    if len(configs) == 1:
        return configs[0]
    for config in configs:
        if suffix.lower() in config.lower():
            return config
    return None


def ensure_extracted(archive_entry: str | None, required: bool = True):
    if not archive_entry:
        return None
    target = GAME_ROOT / "mods" / archive_entry
    if not MOD_EXE.exists():
        raise FileNotFoundError(f"Missing mod extractor: {MOD_EXE}")
    result = subprocess.run(
        [str(MOD_EXE), archive_entry],
        cwd=str(GAME_ROOT),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    invalid_iff = target.suffix.lower() == ".iff" and (
        not target.exists() or not zipfile.is_zipfile(target)
    )
    if invalid_iff:
        target.unlink(missing_ok=True)
        try:
            extract_archive_iff_fallback(archive_entry, target)
        except Exception as fallback_error:
            if not required:
                return None
            raise RuntimeError(
                f"mod.exe failed to extract {archive_entry}\n\n{result.stdout}\n"
                f"Direct archive fallback also failed: {fallback_error}"
            ) from fallback_error
    if not target.exists():
        if not required:
            return None
        raise RuntimeError(f"mod.exe failed to extract {archive_entry}\n\n{result.stdout}")
    if target.suffix.lower() == ".iff" and zipfile.is_zipfile(target):
        try:
            rewrite_iff_zip_deflated(target)
        except Exception:
            target.unlink(missing_ok=True)
            target.with_name(f"{target.name}.tmp").unlink(missing_ok=True)
            raise
    return target


def archive_zip_is_valid(path: Path):
    try:
        if not path.exists() or not zipfile.is_zipfile(path):
            return False
        with zipfile.ZipFile(path) as archive:
            archive.namelist()
            return archive.testzip() is None
    except (OSError, zipfile.BadZipFile, ValueError):
        return False


def extract_clean_config_with_mod(archive_entry: str, destination: Path):
    """Expand a game config while preserving any installed loose override."""
    live_path = GAME_ROOT / "mods" / archive_entry
    held_path = destination.with_suffix(destination.suffix + ".held_loose")
    live_path.parent.mkdir(parents=True, exist_ok=True)
    held_path.unlink(missing_ok=True)
    had_loose = live_path.exists()
    if had_loose:
        try:
            shutil.move(live_path, held_path)
        except PermissionError as exc:
            raise RuntimeError(
                f"{live_path.name} is locked. Close NBA 2K26 and any tool using the config, then try again."
            ) from exc
    try:
        result = None
        for _attempt in range(2):
            live_path.unlink(missing_ok=True)
            result = subprocess.run(
                [str(MOD_EXE), archive_entry],
                cwd=str(GAME_ROOT),
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                check=False,
            )
            if result.returncode == 0 and archive_zip_is_valid(live_path):
                break
        if result is None or result.returncode or not archive_zip_is_valid(live_path):
            output = result.stdout if result is not None else "mod.exe did not run."
            raise RuntimeError(f"mod.exe could not expand {archive_entry}.\n\n{output}")
        with zipfile.ZipFile(live_path) as expanded:
            names = expanded.namelist()
            if not any(name.lower().endswith(".dds") for name in names):
                raise RuntimeError(f"mod.exe did not expand the shared textures for {archive_entry}.")
            for name in names:
                if not name.lower().endswith(".txtr"):
                    continue
                try:
                    expanded.read(name).decode("utf-8-sig")
                except UnicodeDecodeError as exc:
                    raise RuntimeError(f"mod.exe left {name} in compiled binary form.") from exc
        shutil.copy2(live_path, destination)
    finally:
        live_path.unlink(missing_ok=True)
        if had_loose and held_path.exists():
            shutil.move(held_path, live_path)


def extract_clean_archive_with_mod(archive_entry: str, destination: Path):
    """Expand any manifest IFF while preserving an installed loose override."""
    destination = Path(destination)
    live_path = GAME_ROOT / "mods" / archive_entry
    held_path = destination.with_name(f"{destination.name}.held_loose")
    if not MOD_EXE.exists():
        raise FileNotFoundError(f"Missing mod extractor: {MOD_EXE}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    live_path.parent.mkdir(parents=True, exist_ok=True)
    held_path.unlink(missing_ok=True)
    had_loose = live_path.exists()
    if had_loose:
        try:
            shutil.move(live_path, held_path)
        except PermissionError as exc:
            raise RuntimeError(
                f"{live_path.name} is locked. Close NBA 2K26 and any tool using it, then try again."
            ) from exc
    try:
        result = None
        for _attempt in range(2):
            live_path.unlink(missing_ok=True)
            result = subprocess.run(
                [str(MOD_EXE), archive_entry],
                cwd=str(GAME_ROOT),
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                check=False,
            )
            if result.returncode == 0 and archive_zip_is_valid(live_path):
                break
        if result is None or result.returncode or not archive_zip_is_valid(live_path):
            output = result.stdout if result is not None else "mod.exe did not run."
            raise RuntimeError(f"mod.exe could not expand {archive_entry}.\n\n{output}")
        rewrite_iff_zip_deflated(live_path)
        shutil.copy2(live_path, destination)
    finally:
        live_path.unlink(missing_ok=True)
        if had_loose and held_path.exists():
            shutil.move(held_path, live_path)
    return destination


def extract_archive_iff_alias_aware(archive_entry: str, destination: Path):
    """Use direct manifest data when valid, then fall back to the game extractor."""
    destination = Path(destination)
    try:
        return extract_archive_iff_fallback(archive_entry, destination)
    except Exception:
        destination.unlink(missing_ok=True)
        destination.with_name(f"{destination.name}.tmp").unlink(missing_ok=True)
        return extract_clean_archive_with_mod(archive_entry, destination)


def backup_existing(path: Path, stamp: str):
    if not path.exists():
        return None
    WORKSPACE_BACKUP_DIR.mkdir(parents=True, exist_ok=True)
    backup = WORKSPACE_BACKUP_DIR / stamp / path.relative_to(GAME_ROOT)
    backup.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(path, backup)
    return backup


def external_hair_identity(path: Path):
    path = Path(path)
    filename_match = re.match(r"(?i)^png(\d+)_", path.name)
    if not filename_match:
        raise ValueError("The source filename must begin with png followed by its player id.")
    if not zipfile.is_zipfile(path):
        raise ValueError("The selected source is not a ZIP-style NBA 2K IFF.")

    with zipfile.ZipFile(path, "r") as archive:
        scne_name = next((name for name in archive.namelist() if name.lower().endswith(".scne")), None)
        if not scne_name:
            raise ValueError("The selected source contains no SCNE file.")
        scene = read_2k25_scne(archive, scne_name, preserve_duplicates=True)
        root_name, root = next(iter(scene.items()))
        models = root.get("Model", {})
        model = models.get("hihead") if isinstance(models, dict) else None
        if not isinstance(model, dict):
            model = next((value for value in models.values() if isinstance(value, dict)), None)
        if not model:
            raise ValueError("The selected SCNE contains no supported hair model.")
        vertex_format = model.get("VertexFormat", {})
        position_format = vertex_format.get("POSITION0", {}).get("Format")
        if (
            position_format == "R16G16B16A16_SNORM"
            and "BINORMAL0" in vertex_format
            and "TANGENT0" in vertex_format
            and str(model.get("Binary", "")).lower().endswith(".model")
        ):
            generation = "2K23"
        elif (
            position_format == "R32G32B32_FLOAT"
            and "TANGENTFRAME0" in vertex_format
            and model.get("VertexStream")
            and model.get("IndexBuffer")
        ):
            generation = "2K25"
        else:
            raise ValueError("The selected hair does not use a supported 2K23 or 2K25 mesh layout.")

    if not re.fullmatch(r"hair_[A-Za-z0-9_]+", root_name, re.IGNORECASE):
        raise ValueError(f"Unsupported source hair SCNE key: {root_name}")
    return generation, filename_match.group(1), root_name


def external_hair_conversion_plan(source_path: Path, target_png: str, target_hair_key: str):
    source_path = Path(source_path)
    if source_path.stem.lower().endswith("_tangentspace"):
        regular_path = source_path.with_name(
            source_path.name[: -len("_tangentspace.iff")] + ".iff"
        )
        if not regular_path.exists():
            raise FileNotFoundError(
                "The selected tangent-space companion has no ordinary geometry partner:\n"
                f"{regular_path}"
            )
        source_path = regular_path

    generation, source_png, source_hair_key = external_hair_identity(source_path)
    target_png = re.sub(r"\D", "", str(target_png))
    if not target_png:
        raise ValueError("Open a target appearance IFF before converting hair.")
    target_hair_key = validate_target_asset_key(target_hair_key)
    archive_template = f"char/sig/png{target_png}_geo_{target_hair_key}.iff"
    if archive_template.lower() not in parse_manifest_entries():
        raise FileNotFoundError(
            f"The selected target slot has no native 2K26 geometry shell:\n{archive_template}"
        )
    return {
        "generation": generation,
        "source_path": source_path,
        "source_png": source_png,
        "source_hair_key": source_hair_key,
        "target_png": target_png,
        "target_hair_key": target_hair_key,
        "archive_template": archive_template,
        "target_path": TARGET_SIG_DIR / f"png{target_png}_geo_{target_hair_key}.iff",
    }


def validate_converted_external_hair(path: Path, target_hair_key: str, expected_vertex_count: int):
    with zipfile.ZipFile(path, "r") as archive:
        bad_member = archive.testzip()
        if bad_member:
            raise RuntimeError(f"Converted IFF failed ZIP validation at {bad_member}.")
        scne_names = [name for name in archive.namelist() if name.lower().endswith(".scne")]
        if len(scne_names) != 1:
            raise RuntimeError(f"Converted IFF must contain one SCNE file, found {len(scne_names)}.")
        scne_name = scne_names[0]
        scne_data = archive.read(scne_name)
        if scne_data.startswith((b" ", b"\t", b"\r", b"\n")):
            raise RuntimeError("Converted SCNE retains invalid top-level indentation.")
        scene, _text = read_legacy_scne(archive, scne_name)
        root_name = next(iter(scene))
        if root_name.lower() != target_hair_key.lower():
            raise RuntimeError(f"Converted SCNE root is {root_name}, expected {target_hair_key}.")
        model = scene[root_name]["Model"]["hihead"]

        def member_data(binary_name):
            for candidate in (
                binary_name,
                binary_name.replace(".gz", ".bin"),
                binary_name.replace(".bin", ".gz"),
            ):
                if candidate in archive.namelist():
                    return archive.read(candidate)
            raise RuntimeError(f"Converted IFF is missing referenced buffer {binary_name}.")

        streams = model.get("VertexStream", [])
        if len(streams) != 2:
            raise RuntimeError(f"Converted hair must contain two vertex streams, found {len(streams)}.")
        for stream in streams:
            data = member_data(stream["Binary"])
            stride = int(stream["Stride"])
            if len(data) != int(stream["Size"]) or len(data) != expected_vertex_count * stride:
                raise RuntimeError(f"Converted vertex stream {stream['Binary']} has an invalid size.")

        lods = model.get("Lods") or []
        if lods and (
            len(lods) != 1
            or int(lods[0].get("LodVerts", -1)) != expected_vertex_count
        ):
            raise RuntimeError("Converted SCNE does not contain one valid full-detail LOD.")
        index_data = member_data(model["IndexBuffer"]["Binary"])
        if len(index_data) != int(model["IndexBuffer"]["Size"]):
            raise RuntimeError("Converted index buffer size does not match its SCNE descriptor.")
        if (zlib.crc32(index_data) & 0xFFFFFFFF) != int(model["IndexBufferCrc32"]):
            raise RuntimeError("Converted index buffer CRC does not match its SCNE descriptor.")
        primitive = model["Prim"][0]
        if int(primitive["Count"]) * 2 != len(index_data):
            raise RuntimeError("Converted primitive count does not match its index buffer.")
        lod_list = primitive.get("LodList") or []
        if lod_list and len(lod_list) != 1:
            raise RuntimeError("Converted primitive contains more than one LOD.")
        if index_data:
            indices = struct.unpack(f"<{len(index_data) // 2}H", index_data)
            if max(indices) >= expected_vertex_count:
                raise RuntimeError("Converted index buffer references a missing vertex.")

        weight_stream = member_data(streams[1]["Binary"])
        weights = {
            struct.unpack_from("<I", weight_stream, offset + 12)[0]
            for offset in range(0, len(weight_stream), 16)
        }
        if weights != {48 << 8}:
            raise RuntimeError("Converted hair is not fully static on head bone 48.")
        if len(member_data(model["MatrixWeightsBuffer"]["Binary"])) != 4:
            raise RuntimeError("Converted static matrix-weight buffer must contain four bytes.")
        if any(
            info.create_system != 3
            or info.create_version != 63
            or info.extract_version != 20
            or info.external_attr != 0x81B60000
            for info in archive.infolist()
        ):
            raise RuntimeError("Converted IFF did not preserve native archive member headers.")


def convert_external_hair_to_output(
    source_path: Path,
    target_png: str,
    target_hair_key: str,
    output_path: Path,
    template_path: Path | None = None,
):
    source_path = Path(source_path)
    output_path = Path(output_path)
    target_png = re.sub(r"\D", "", str(target_png))
    if not target_png:
        raise ValueError("Target PNG id must contain digits.")
    target_hair_key = validate_target_asset_key(target_hair_key)

    if template_path:
        template_path = Path(template_path)
        if not archive_zip_is_valid(template_path):
            raise ValueError(f"The target hair template is not a valid IFF:\n{template_path}")
        generation, source_png, source_hair_key = external_hair_identity(source_path)
        plan = {
            "generation": generation,
            "source_path": source_path,
            "source_png": source_png,
            "source_hair_key": source_hair_key,
            "target_png": target_png,
            "target_hair_key": target_hair_key,
            "archive_template": "",
            "target_path": output_path,
        }
    else:
        plan = external_hair_conversion_plan(source_path, target_png, target_hair_key)

    staging = Path(tempfile.mkdtemp(prefix="character_mod_hair_stage_"))
    staged_template = staging / f"template_png{target_png}_geo_{target_hair_key}.iff"
    converted_path = staging / f"png{target_png}_geo_{target_hair_key}.iff"
    try:
        if template_path:
            shutil.copy2(template_path, staged_template)
        else:
            extract_archive_iff_fallback(plan["archive_template"], staged_template)

        if plan["generation"] == "2K23":
            result = convert_legacy_hair(
                plan["source_path"],
                converted_path,
                force_bone=48,
                target_name=plan["target_hair_key"],
                target_scne_name=f"{plan['target_hair_key']}.SCNE",
                duplicate_lods=False,
                template_path=staged_template,
                preserve_template_vertex_metadata=True,
                fit_template_vertex_count=True,
                use_source_uv_metadata=True,
            )
            expected_vertex_count = int(result["stream_vertices"])
        else:
            result = convert_2k25_hair(
                plan["source_path"],
                staged_template,
                converted_path,
                force_bone=48,
            )
            expected_vertex_count = int(result["vertices"])

        validate_converted_external_hair(
            converted_path,
            plan["target_hair_key"],
            expected_vertex_count,
        )
        output_path.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(converted_path, output_path)
        validate_converted_external_hair(
            output_path,
            plan["target_hair_key"],
            expected_vertex_count,
        )
        return output_path, result, plan
    finally:
        shutil.rmtree(staging, ignore_errors=True)


def convert_external_hair_to_target(source_path: Path, target_png: str, target_hair_key: str):
    plan = external_hair_conversion_plan(source_path, target_png, target_hair_key)
    staging = Path(tempfile.mkdtemp(prefix="character_mod_hair_convert_"))
    template_path = staging / f"template_{Path(plan['archive_template']).name}"
    converted_path = staging / plan["target_path"].name
    target_path = plan["target_path"]
    stamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    backup = None
    try:
        extract_archive_iff_fallback(plan["archive_template"], template_path)
        if plan["generation"] == "2K23":
            result = convert_legacy_hair(
                plan["source_path"],
                converted_path,
                force_bone=48,
                target_name=plan["target_hair_key"],
                target_scne_name=f"{plan['target_hair_key']}.SCNE",
                duplicate_lods=False,
                template_path=template_path,
                preserve_template_vertex_metadata=True,
                fit_template_vertex_count=True,
                use_source_uv_metadata=True,
            )
            expected_vertex_count = int(result["stream_vertices"])
        else:
            result = convert_2k25_hair(
                plan["source_path"],
                template_path,
                converted_path,
                force_bone=48,
            )
            expected_vertex_count = int(result["vertices"])

        validate_converted_external_hair(
            converted_path,
            plan["target_hair_key"],
            expected_vertex_count,
        )
        TARGET_SIG_DIR.mkdir(parents=True, exist_ok=True)
        backup = backup_existing(target_path, stamp)
        converted_path.replace(target_path)
        return target_path, backup, result, plan
    finally:
        shutil.rmtree(staging, ignore_errors=True)


def rename_scne_payload(data: bytes, source_hair_key: str, target_hair_key: str):
    if len(data) >= 4 and struct.unpack_from("<I", data)[0] == 0xE01FF891:
        scene = decode_binary_scne(data)
    else:
        text = data.decode("utf-8-sig")
        scene = json.loads("{" + text + "}")

    def force_full_detail(value):
        if isinstance(value, dict):
            lods = value.get("Lods")
            if isinstance(lods, list) and lods and isinstance(lods[0], dict):
                full_verts = lods[0].get("LodVerts")
                if full_verts is not None:
                    for lod in lods:
                        if isinstance(lod, dict):
                            lod["LodVerts"] = full_verts
            prims = value.get("Prim")
            if isinstance(prims, list):
                for prim in prims:
                    if not isinstance(prim, dict):
                        continue
                    lod_list = prim.get("LodList")
                    if not isinstance(lod_list, list) or not lod_list:
                        continue
                    full_start = prim.get("Start", lod_list[0].get("Start"))
                    full_count = prim.get("Count", lod_list[0].get("Count"))
                    for lod in lod_list:
                        if isinstance(lod, dict):
                            lod["Start"] = full_start
                            lod["Count"] = full_count
            for child in value.values():
                force_full_detail(child)
        elif isinstance(value, list):
            for child in value:
                force_full_detail(child)

    force_full_detail(scene)
    return render_scne_text(scene)


def target_geo_member_name(member_name: str, _source_hair_key: str, target_hair_key: str):
    path = Path(member_name)
    name = path.name
    lower_name = name.lower()
    if lower_name.endswith(".scne"):
        name = f"{target_hair_key}{path.suffix}"
    if path.parent == Path("."):
        return name
    return str(path.parent / name).replace("\\", "/")


def fixed_zip_info(source_info: zipfile.ZipInfo, filename: str):
    info = zipfile.ZipInfo(filename, source_info.date_time)
    info.compress_type = zipfile.ZIP_DEFLATED
    info.comment = source_info.comment
    info.extra = source_info.extra
    info.internal_attr = source_info.internal_attr
    info.external_attr = 0x81B60000
    info.create_system = 3
    return info


def scne_buffer_sizes(zip_file: zipfile.ZipFile):
    sizes = {}
    for name in zip_file.namelist():
        if not name.lower().endswith(".scne"):
            continue
        sizes.update(scne_buffer_sizes_from_text(zip_file.read(name)))
    return sizes


def scne_buffer_sizes_from_text(data: bytes):
    sizes = {}
    text = data.decode("utf-8-sig", errors="replace")
    for match in re.finditer(
        r'"Size"\s*:\s*(\d+).*?"Binary"\s*:\s*"([^"]+)"',
        text,
        flags=re.DOTALL,
    ):
        size = int(match.group(1))
        binary = match.group(2)
        sizes[binary] = size
        if binary.endswith(".gz"):
            sizes[binary[:-3] + ".bin"] = size
    return sizes


_OODLE_DECOMPRESS = None


def oodle_decompress_result(comp_data: bytes, raw_size: int):
    global _OODLE_DECOMPRESS
    if _OODLE_DECOMPRESS is None:
        if not OODLE_DLL.exists():
            raise FileNotFoundError(f"Missing Oodle DLL: {OODLE_DLL}")
        dll = ctypes.CDLL(str(OODLE_DLL))
        _OODLE_DECOMPRESS = dll.OodleLZ_Decompress
        _OODLE_DECOMPRESS.restype = ctypes.c_longlong

    comp_buf = ctypes.create_string_buffer(comp_data)
    raw_buf = ctypes.create_string_buffer(raw_size)
    result = _OODLE_DECOMPRESS(
        comp_buf,
        len(comp_data),
        raw_buf,
        raw_size,
        1,
        1,
        0,
        None,
        0,
        None,
        None,
        None,
        0,
        3,
    )
    return result, raw_buf.raw


def oodle_decompress(comp_data: bytes, raw_size: int):
    result, raw = oodle_decompress_result(comp_data, raw_size)
    if result != raw_size:
        raise RuntimeError(f"Oodle decompression returned {result}, expected {raw_size}.")
    return raw


def trim_complete_zip(data: bytes):
    marker = b"PK\x05\x06"
    end_record = data.rfind(marker)
    if end_record < 0 or end_record + 22 > len(data):
        return None
    comment_size = struct.unpack_from("<H", data, end_record + 20)[0]
    zip_end = end_record + 22 + comment_size
    if zip_end > len(data):
        return None
    candidate = data[:zip_end]
    try:
        with zipfile.ZipFile(io.BytesIO(candidate)) as archive:
            if archive.testzip() is not None:
                return None
    except (OSError, zipfile.BadZipFile):
        return None
    return candidate


def oodle_decompress_zip(comp_data: bytes):
    start_size = max(16384, ((len(comp_data) + 4095) // 4096) * 4096)
    max_size = min(64 * 1024 * 1024, max(4 * 1024 * 1024, len(comp_data) * 128))
    for raw_size in range(start_size, max_size + 1, 4096):
        result, raw = oodle_decompress_result(comp_data, raw_size)
        if result <= 0:
            continue
        candidate = trim_complete_zip(raw[:result])
        if candidate is not None:
            return candidate
    raise RuntimeError(
        f"Could not determine the expanded archive ZIP size "
        f"(compressed {len(comp_data):,} bytes; tested through {max_size:,} bytes)."
    )


BINARY_SCNE_SIZE_HASH = 0x57F28B54
BINARY_SCNE_BINARY_HASH = 0xC14CEC33
BINARY_SCNE_UINT_TYPE = 0x40000006
BINARY_SCNE_STRING_TYPE = 0x40000003


def binary_scne_buffer_sizes(data: bytes):
    sizes = {}
    binary_signature = struct.pack("<II", BINARY_SCNE_BINARY_HASH, BINARY_SCNE_STRING_TYPE)
    size_signature = struct.pack("<II", BINARY_SCNE_SIZE_HASH, BINARY_SCNE_UINT_TYPE)
    for match in re.finditer(re.escape(binary_signature), data):
        binary_hash_pos = match.start()
        size_hash_pos = binary_hash_pos - 40
        if size_hash_pos < 0 or data[size_hash_pos : size_hash_pos + 8] != size_signature:
            continue
        size = struct.unpack_from("<I", data, size_hash_pos + 16)[0]
        pointer_pos = binary_hash_pos + 16
        pointer = struct.unpack_from("<I", data, pointer_pos)[0]
        string_pos = pointer_pos + pointer - 1
        if string_pos < 0 or string_pos >= len(data):
            continue
        string_end = data.find(b"\x00", string_pos)
        if string_end < 0:
            continue
        try:
            binary = data[string_pos:string_end].decode("ascii")
        except UnicodeDecodeError:
            continue
        if binary.lower().endswith(".gz"):
            sizes[binary] = size
            sizes[binary[:-3] + ".bin"] = size
    return sizes


def decode_binary_scne(data: bytes):
    if len(data) < 16 or struct.unpack_from("<I", data)[0] != 0xE01FF891:
        raise ValueError("SCNE does not use the supported compiled format.")
    record_count = struct.unpack_from("<I", data, 12)[0]
    records_start = 16
    records_end = records_start + record_count * 40
    if records_end > len(data):
        raise ValueError("Compiled SCNE record table is truncated.")

    def read_string(position: int):
        if position < 0 or position >= len(data):
            raise ValueError("Compiled SCNE string pointer is out of range.")
        end = data.find(b"\x00", position)
        if end < 0:
            raise ValueError("Compiled SCNE string is unterminated.")
        return data[position:end].decode("utf-8")

    active = set()

    def read_record(start: int):
        if start < records_start or start + 40 > records_end or (start - records_start) % 40:
            raise ValueError("Compiled SCNE record pointer is invalid.")
        if start in active:
            raise ValueError("Compiled SCNE contains a record cycle.")
        active.add(start)
        try:
            key_pointer = struct.unpack_from("<I", data, start)[0]
            key = read_string(start + key_pointer - 1)
            value_type = struct.unpack_from("<I", data, start + 12)[0]
            value_position = start + 24

            if value_type == 0x40000000:
                value = None
            elif value_type in (0x40000001, 0x40000002):
                count = struct.unpack_from("<I", data, start + 20)[0]
                list_pointer = struct.unpack_from("<I", data, value_position)[0]
                list_start = value_position + list_pointer - 1
                children = []
                for index in range(count):
                    entry_position = list_start + index * 8
                    if entry_position < records_end or entry_position + 8 > len(data):
                        raise ValueError("Compiled SCNE child list is out of range.")
                    relative = struct.unpack_from("<q", data, entry_position)[0]
                    child_start = entry_position + relative - 1
                    children.append(read_record(child_start))
                value = dict(children) if value_type == 0x40000001 else [child_value for _key, child_value in children]
            elif value_type == 0x40000003:
                string_pointer = struct.unpack_from("<I", data, value_position)[0]
                value = read_string(value_position + string_pointer - 1)
            elif value_type == 0x40000004:
                value = bool(struct.unpack_from("<I", data, value_position)[0])
            elif value_type == 0x40000005:
                value = struct.unpack_from("<d", data, value_position)[0]
            elif value_type == 0x40000006:
                value = struct.unpack_from("<I", data, value_position)[0]
            else:
                raise ValueError(f"Unsupported compiled SCNE value type: 0x{value_type:08x}")
            return key, value
        finally:
            active.remove(start)

    root_key, root_value = read_record(records_start)
    return {root_key: root_value}


def render_scne_text(scene: dict):
    rendered = json.dumps(scene, indent="\t", ensure_ascii=True)
    lines = rendered.splitlines()[1:-1]
    return ("\n".join(line[1:] if line.startswith("\t") else line for line in lines) + "\n").encode("utf-8")


def read_manifest_archive_data(entry: tuple[str, str, int, int]):
    name, archive_id, offset, size = entry
    archive_path = GAME_ROOT / archive_id
    if not archive_path.exists():
        raise FileNotFoundError(f"Missing game archive {archive_id} for {name}")
    with archive_path.open("rb") as archive:
        archive.seek(offset)
        data = archive.read(size)
    if len(data) != size:
        raise RuntimeError(f"Could not read the full archive entry {name}")
    return data


def decode_manifest_archive_data(entry: tuple[str, str, int, int], raw_size: int):
    data = read_manifest_archive_data(entry)
    if len(data) >= 16 and data[12:16] == b"VCZ\x00":
        return oodle_decompress(data[16:], raw_size)
    if len(data) != raw_size:
        raise RuntimeError(f"Unexpected raw size for {entry[0]}: {len(data)}, expected {raw_size}")
    return data


def shared_manifest_name(binary_name: str):
    match = re.fullmatch(r"(?i)(?:IndexBuffer|MatrixWeightsBuffer|Morph|VertexBuffer)\.([0-9a-f]+)\.gz", binary_name)
    if not match:
        return None
    digest = match.group(1).lower()
    return f"shared/{digest[:2]}/{binary_name.lower()}"


def extract_archive_iff_fallback(archive_entry: str, target: Path):
    manifest = parse_manifest_entries()
    source_entry = manifest.get(archive_entry.lower())
    if not source_entry:
        raise FileNotFoundError(f"Archive entry is missing from the manifest: {archive_entry}")
    wrapped = read_manifest_archive_data(source_entry)
    if zipfile.is_zipfile(io.BytesIO(wrapped)):
        zip_data = wrapped
    elif len(wrapped) >= 16 and wrapped[12:16] == b"VCZ\x00":
        zip_data = oodle_decompress_zip(wrapped[16:])
    else:
        raise RuntimeError("Archive entry is neither a ZIP nor a supported VCZ-wrapped ZIP.")

    with zipfile.ZipFile(io.BytesIO(zip_data), "r") as source_zip:
        source_infos = source_zip.infolist()
        member_data = {info.filename: source_zip.read(info.filename) for info in source_infos}

    buffer_sizes = {}
    for name, data in member_data.items():
        if name.lower().endswith(".scne"):
            compiled_sizes = binary_scne_buffer_sizes(data)
            is_compiled = len(data) >= 4 and struct.unpack_from("<I", data)[0] == 0xE01FF891
            if is_compiled:
                buffer_sizes.update(compiled_sizes)
                member_data[name] = render_scne_text(decode_binary_scne(data))
            else:
                buffer_sizes.update(scne_buffer_sizes_from_text(data))

    existing_lower = {name.lower() for name in member_data}
    shared_members = []
    for binary_name, raw_size in buffer_sizes.items():
        if not binary_name.lower().endswith(".gz"):
            continue
        output_name = binary_name[:-3] + ".bin"
        if output_name.lower() in existing_lower:
            continue
        shared_name = shared_manifest_name(binary_name)
        shared_entry = manifest.get(shared_name.lower()) if shared_name else None
        if not shared_entry:
            raise FileNotFoundError(f"Missing shared archive resource for {binary_name}")
        shared_members.append((output_name, decode_manifest_archive_data(shared_entry, raw_size)))
        existing_lower.add(output_name.lower())

    target.parent.mkdir(parents=True, exist_ok=True)
    temp = target.with_name(f"{target.name}.tmp")
    try:
        with zipfile.ZipFile(temp, "w", compression=zipfile.ZIP_DEFLATED) as output_zip:
            for name, data in shared_members:
                info = zipfile.ZipInfo(name)
                info.compress_type = zipfile.ZIP_DEFLATED
                info.create_system = 3
                info.external_attr = 0x81B60000
                output_zip.writestr(info, data)
            for info in source_infos:
                output_zip.writestr(fixed_zip_info(info, info.filename), member_data[info.filename])
        verify_no_vcz_buffers(temp)
        temp.replace(target)
    except Exception:
        temp.unlink(missing_ok=True)
        raise
    return target


def decode_vcz_buffer(data: bytes, expected_size: int):
    if len(data) >= 16 and data[12:16] == b"VCZ\x00":
        return oodle_decompress(data[16:], expected_size)
    return data


def verify_no_vcz_buffers(path: Path):
    with zipfile.ZipFile(path, "r") as archive:
        compressed = []
        for name in archive.namelist():
            if not name.lower().endswith((".bin", ".gz")):
                continue
            data = archive.read(name)
            if len(data) >= 16 and data[12:16] == b"VCZ\x00":
                compressed.append(name)
    if compressed:
        sample = ", ".join(compressed[:5])
        more = f" and {len(compressed) - 5} more" if len(compressed) > 5 else ""
        raise RuntimeError(f"Exporter left compressed VCZ buffers: {sample}{more}")


def rewrite_iff_zip_deflated(
    source_path: Path,
    target_path: Path | None = None,
    rename_scne_to: str | None = None,
    decode_vcz: bool = True,
):
    source_path = Path(source_path)
    target_path = Path(target_path) if target_path else source_path
    temp = target_path.with_name(f"{target_path.name}.tmp")
    with zipfile.ZipFile(source_path, "r") as zin, zipfile.ZipFile(
        temp, "w", compression=zipfile.ZIP_DEFLATED
    ) as zout:
        buffer_sizes = scne_buffer_sizes(zin) if decode_vcz else {}
        for source_info in zin.infolist():
            name = source_info.filename
            if rename_scne_to and name.lower().endswith(".scne"):
                path = Path(name)
                name = str((path.parent / f"{rename_scne_to}{path.suffix}") if path.parent != Path(".") else Path(f"{rename_scne_to}{path.suffix}")).replace("\\", "/")
            data = zin.read(source_info.filename)
            if decode_vcz and source_info.filename in buffer_sizes:
                data = decode_vcz_buffer(data, buffer_sizes[source_info.filename])
            if rename_scne_to and source_info.filename.lower().endswith(".scne"):
                data = rename_scne_payload(data, "", rename_scne_to)
            zout.writestr(fixed_zip_info(source_info, name), data)
    try:
        verify_no_vcz_buffers(temp)
    except Exception:
        temp.unlink(missing_ok=True)
        raise
    temp.replace(target_path)
    return target_path


def copy_geo_with_target_names(source_geo: Path, target_geo: Path, source_hair_key: str, target_hair_key: str):
    target_geo.parent.mkdir(parents=True, exist_ok=True)
    temp = target_geo.with_name(f"{target_geo.name}.tmp")
    rewrite_iff_zip_deflated(source_geo, temp, rename_scne_to=target_hair_key)
    verify_geo_rename_only(source_geo, temp, source_hair_key, target_hair_key)
    temp.replace(target_geo)


def verify_geo_rename_only(source_geo: Path, target_geo: Path, source_hair_key: str, target_hair_key: str):
    with zipfile.ZipFile(source_geo, "r") as source_zip, zipfile.ZipFile(target_geo, "r") as target_zip:
        buffer_sizes = scne_buffer_sizes(source_zip)
        expected_names = [
            target_geo_member_name(info.filename, source_hair_key, target_hair_key)
            for info in source_zip.infolist()
        ]
        actual_names = target_zip.namelist()
        if actual_names != expected_names:
            raise RuntimeError("Geo rename verification failed: archive member list changed unexpectedly.")

        for info, expected_name in zip(source_zip.infolist(), expected_names):
            source_data = source_zip.read(info.filename)
            if info.filename in buffer_sizes:
                source_data = decode_vcz_buffer(source_data, buffer_sizes[info.filename])
            if info.filename.lower().endswith(".scne"):
                source_data = rename_scne_payload(source_data, source_hair_key, target_hair_key)
            target_data = target_zip.read(expected_name)
            if target_data != source_data:
                raise RuntimeError(f"Geo rename verification failed: entry data changed in {info.filename}.")


def cleanup_archive_exports(paths: list[Path], keep_paths: list[Path | None]):
    keep = {path.resolve() for path in keep_paths if path}
    for path in paths:
        if not path:
            continue
        resolved = path.resolve()
        if resolved in keep:
            continue
        resolved.unlink(missing_ok=True)
        resolved.with_name(f"{resolved.name}.tmp").unlink(missing_ok=True)


def scne_asset_name(asset_key: str, tangentspace: bool):
    if asset_key.startswith("facialhair"):
        asset_key = "facialHair" + asset_key[len("facialhair") :]
    return f"{asset_key}_tangentspace" if tangentspace else asset_key


def validate_target_asset_key(asset_key: str):
    asset_key = asset_key.strip()
    if not asset_key:
        raise ValueError("Target asset key is empty.")
    if not re.fullmatch(r"[A-Za-z0-9_]+", asset_key):
        raise ValueError(
            "Target asset key may contain only letters, numbers, and underscores. "
            "Choose a slot from the imported appearance file when possible."
        )
    return asset_key


def install_hair(
    option: HairOption,
    target_png: str,
    target_hair_key: str,
    copy_item: bool,
    copy_config: bool = False,
    target_tangentspace: bool = False,
):
    target_png = re.sub(r"\D", "", target_png)
    if not target_png:
        raise ValueError("Target PNG id must contain digits.")
    target_hair_key = validate_target_asset_key(target_hair_key)

    TARGET_SIG_DIR.mkdir(parents=True, exist_ok=True)
    stamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    tangent_suffix = "_tangentspace" if target_tangentspace else ""
    target_geo = TARGET_SIG_DIR / f"png{target_png}_geo_{target_hair_key}{tangent_suffix}.iff"
    target_item = TARGET_SIG_DIR / f"png{target_png}_item_{target_hair_key}.iff"
    target_config = TARGET_SIG_DIR / f"png{target_png}_config_{config_suffix_from_hair_key(target_hair_key)}.iff"

    backups = []
    backup = backup_existing(target_geo, stamp)
    if backup:
        backups.append(backup)

    if copy_item and (option.source_item or option.archive_item):
        backup = backup_existing(target_item, stamp)
        if backup:
            backups.append(backup)

    if option.asset_type == "hair" and copy_config and (option.source_config or option.archive_config):
        backup = backup_existing(target_config, stamp)
        if backup:
            backups.append(backup)

    archive_exports = []
    copied_item = False
    copied_config = False
    try:
        source_geo = option.source_geo or ensure_extracted(option.archive_geo, required=True)
        if not option.source_geo and source_geo:
            archive_exports.append(source_geo)

        source_item = (option.source_item or ensure_extracted(option.archive_item, required=False)) if copy_item else None
        if not option.source_item and source_item:
            archive_exports.append(source_item)

        source_config = (
            option.source_config or ensure_extracted(option.archive_config, required=False)
        ) if option.asset_type == "hair" and copy_config else None
        if option.asset_type == "hair" and copy_config and not option.source_config and source_config:
            archive_exports.append(source_config)

        if not source_geo:
            raise FileNotFoundError("No source geo file available.")
        copy_geo_with_target_names(
            source_geo,
            target_geo,
            scne_asset_name(option.hair_key, option.tangentspace),
            scne_asset_name(
                target_hair_key,
                option.tangentspace if option.asset_type == "facialhair" else target_tangentspace,
            ),
        )

        if copy_item and source_item:
            shutil.copy2(source_item, target_item)
            copied_item = True

        if source_config:
            shutil.copy2(source_config, target_config)
            copied_config = True

        return target_geo, target_item if copied_item else None, target_config if copied_config else None, backups
    finally:
        cleanup_archive_exports(
            archive_exports,
            [target_geo, target_item if copied_item else None, target_config if copied_config else None],
        )


def prepare_tangent_fitted_option(option, target_png, status_callback=None):
    target_png = re.sub(r"\D", "", str(target_png))
    if not target_png:
        raise ValueError("Target PNG id must contain digits.")
    if not BLENDER_EXE.exists():
        raise FileNotFoundError(f"Blender 5.1 was not found at:\n{BLENDER_EXE}")
    if not BLENDER_AUTOFIT_SCRIPT.exists():
        raise FileNotFoundError(f"The Blender Hair Fixer is missing:\n{BLENDER_AUTOFIT_SCRIPT}")
    staging_dir = Path(tempfile.mkdtemp(prefix="nba2k_tangent_install_"))
    source_name = option.source_geo.name if option.source_geo else Path(option.archive_geo or "selected_hair.iff").name
    staged_hair = staging_dir / source_name
    donor_head = staging_dir / f"donor_png{option.source_png}.iff"
    target_head = staging_dir / f"target_png{target_png}.iff"
    fitted_hair = staging_dir / f"fitted_{source_name}"
    try:
        if option.source_geo:
            shutil.copy2(option.source_geo, staged_hair)
        elif option.archive_geo:
            extract_archive_iff_fallback(option.archive_geo, staged_hair)
        else:
            raise FileNotFoundError("The selected hair has no source IFF.")
        extract_archive_iff_fallback(f"char/sig/png{option.source_png}.iff", donor_head)
        extract_archive_iff_fallback(f"char/sig/png{target_png}.iff", target_head)
        if status_callback:
            status_callback(f"Tangent-fitting {source_name} from png{option.source_png} to png{target_png}...")
        creation_flags = subprocess.CREATE_NO_WINDOW if hasattr(subprocess, "CREATE_NO_WINDOW") else 0
        with BLENDER_AUTOFIT_LOG.open("w", encoding="utf-8") as blender_log:
            result = subprocess.run(
                [
                    str(BLENDER_EXE),
                    "--background",
                    "--factory-startup",
                    "--python",
                    str(BLENDER_AUTOFIT_SCRIPT),
                    "--",
                    str(staged_hair),
                    str(donor_head),
                    str(target_head),
                    str(fitted_hair),
                    option.hair_key,
                    "tangent",
                ],
                cwd=str(BLENDER_EXE.parent),
                stdout=blender_log,
                stderr=subprocess.STDOUT,
                creationflags=creation_flags,
                check=False,
            )
        if result.returncode:
            details = BLENDER_AUTOFIT_LOG.read_text(encoding="utf-8", errors="replace")
            raise RuntimeError(f"Background Tangent Fit failed.\n\n{details[-3000:]}")
        if not fitted_hair.exists() or not zipfile.is_zipfile(fitted_hair):
            raise RuntimeError("Background Tangent Fit did not create a valid fitted hair IFF.")
        return replace(option, source_geo=fitted_hair, archive_geo=None), staging_dir
    except Exception:
        shutil.rmtree(staging_dir, ignore_errors=True)
        raise


class HairPicker(tk.Tk):
    def __init__(self):
        super().__init__()
        self.title(f"NBA 2K26 Hair Picker v{HAIR_PICKER_VERSION}")
        self.geometry("980x620")
        self.minsize(860, 520)

        self.asset_type_var = tk.StringVar(value="hair")
        self.options = discover_hairs()
        self.filtered: list[HairOption] = []
        self.appearance_summary: AppearanceSummary | None = None

        self.search_var = tk.StringVar()
        self.target_png_var = tk.StringVar()
        self.target_hair_var = tk.StringVar()
        self.target_slot_var = tk.StringVar()
        self.target_slot_map: dict[str, str] = {}
        self.target_tangent_map: dict[str, bool] = {}
        self.target_key_label_var = tk.StringVar(value="Target Hair Key")
        self.copy_item_var = tk.BooleanVar(value=True)
        self.copy_config_var = tk.BooleanVar(value=False)
        self.status_var = tk.StringVar(value=f"Found {len(self.options)} native 2K26 hair geo files.")

        self._build_ui()
        self._refresh_list()

    def _build_ui(self):
        root = ttk.Frame(self, padding=12)
        root.pack(fill=tk.BOTH, expand=True)

        mode_row = ttk.Frame(root)
        mode_row.pack(fill=tk.X, pady=(0, 10))
        ttk.Label(mode_row, text="Asset Type").pack(side=tk.LEFT)
        ttk.Radiobutton(
            mode_row,
            text="Hair",
            value="hair",
            variable=self.asset_type_var,
            command=self._asset_type_changed,
        ).pack(side=tk.LEFT, padx=(10, 4))
        ttk.Radiobutton(
            mode_row,
            text="Facial Hair",
            value="facialhair",
            variable=self.asset_type_var,
            command=self._asset_type_changed,
        ).pack(side=tk.LEFT, padx=(4, 0))

        top = ttk.Frame(root)
        top.pack(fill=tk.X)
        ttk.Label(top, text="Search").pack(side=tk.LEFT)
        search = ttk.Entry(top, textvariable=self.search_var)
        search.pack(side=tk.LEFT, fill=tk.X, expand=True, padx=(8, 16))
        search.bind("<KeyRelease>", lambda _event: self._refresh_list())

        ttk.Label(top, text="Target PNG").pack(side=tk.LEFT)
        ttk.Entry(top, textvariable=self.target_png_var, width=8).pack(side=tk.LEFT, padx=(8, 16))
        ttk.Button(top, text="Import Appearance IFF", command=self._import_appearance_iff).pack(side=tk.LEFT, padx=(12, 0))

        target_row = ttk.Frame(root)
        target_row.pack(fill=tk.X, pady=(10, 0))
        ttk.Label(target_row, text="Overwrite Slot").pack(side=tk.LEFT)
        self.target_slot_combo = ttk.Combobox(
            target_row,
            textvariable=self.target_slot_var,
            state="readonly",
            width=42,
            values=list(self.target_slot_map.keys()),
        )
        self.target_slot_combo.pack(side=tk.LEFT, padx=(8, 16))
        self.target_slot_combo.bind("<<ComboboxSelected>>", lambda _event: self._target_slot_selected())
        ttk.Label(target_row, textvariable=self.target_key_label_var).pack(side=tk.LEFT)
        ttk.Entry(target_row, textvariable=self.target_hair_var, width=18).pack(side=tk.LEFT, padx=(8, 0))

        middle = ttk.Frame(root)
        middle.pack(fill=tk.BOTH, expand=True, pady=(12, 8))

        self.listbox = tk.Listbox(middle, activestyle="dotbox")
        self.listbox.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)
        self.listbox.bind("<<ListboxSelect>>", lambda _event: self._show_details())
        scrollbar = ttk.Scrollbar(middle, orient=tk.VERTICAL, command=self.listbox.yview)
        scrollbar.pack(side=tk.LEFT, fill=tk.Y)
        self.listbox.configure(yscrollcommand=scrollbar.set)

        details = ttk.Frame(middle, padding=(12, 0, 0, 0))
        details.pack(side=tk.RIGHT, fill=tk.BOTH)
        self.details_text = tk.Text(details, width=42, height=22, wrap=tk.WORD)
        self.details_text.pack(fill=tk.BOTH, expand=True)
        self.details_text.configure(state=tk.DISABLED)

        bottom = ttk.Frame(root)
        bottom.pack(fill=tk.X)
        ttk.Checkbutton(bottom, text="Copy matching item_hair file when available", variable=self.copy_item_var).pack(
            side=tk.LEFT
        )
        self.copy_config_check = ttk.Checkbutton(bottom, text="Copy source config file", variable=self.copy_config_var)
        self.copy_config_check.pack(
            side=tk.LEFT, padx=(16, 0)
        )
        ttk.Button(bottom, text="Refresh", command=self._refresh_all).pack(side=tk.RIGHT)
        ttk.Button(bottom, text="Install Selected", command=self._install_selected).pack(side=tk.RIGHT, padx=(0, 8))
        ttk.Button(bottom, text="Open Selected in Blender", command=self._open_selected_in_blender).pack(
            side=tk.RIGHT, padx=(0, 8)
        )
        fit_row = ttk.Frame(root)
        fit_row.pack(fill=tk.X, pady=(8, 0))
        ttk.Button(fit_row, text="Open + Auto-Fit", command=self._open_selected_with_autofit).pack(side=tk.RIGHT)
        ttk.Button(
            fit_row,
            text="Open + Tangent Fit Test",
            command=self._open_selected_with_tangent_fit,
        ).pack(side=tk.RIGHT, padx=(0, 8))

        ttk.Label(root, textvariable=self.status_var).pack(fill=tk.X, pady=(8, 0))

    def _refresh_all(self):
        self.options = discover_hairs() if self.asset_type_var.get() == "hair" else discover_facial_hairs()
        label = "hair" if self.asset_type_var.get() == "hair" else "facial-hair"
        self.status_var.set(f"Found {len(self.options)} native 2K26 {label} geo files.")
        self._refresh_list()

    def _asset_type_changed(self):
        is_hair = self.asset_type_var.get() == "hair"
        self.target_key_label_var.set("Target Hair Key" if is_hair else "Target Facial-Hair Key")
        if is_hair:
            self.copy_config_check.state(["!disabled"])
        else:
            self.copy_config_var.set(False)
            self.copy_config_check.state(["disabled"])
        self._update_target_slots_from_appearance()
        self._refresh_all()

    def _refresh_list(self):
        query = self.search_var.get().lower().strip()
        self.filtered = [
            option
            for option in self.options
            if not query
            or query in (option.source_geo.name.lower() if option.source_geo else "")
            or query in (option.archive_geo or "").lower()
            or query in (option.archive_item or "").lower()
            or query in (option.archive_config or "").lower()
            or query in option.hair_key.lower()
            or query in option.player_name.lower()
            or query in option.source_png
            or query in option.root_name.lower()
        ]
        self.listbox.delete(0, tk.END)
        for option in self.filtered:
            self.listbox.insert(tk.END, option.label)
        if self.filtered:
            self.listbox.selection_set(0)
            self._show_details()

    def _selected_option(self):
        selection = self.listbox.curselection()
        if not selection:
            return None
        return self.filtered[selection[0]]

    def _target_slot_selected(self):
        key = self.target_slot_map.get(self.target_slot_var.get())
        if key:
            self.target_hair_var.set(key)
            self._show_details()

    def _update_target_slots_from_appearance(self):
        if not self.appearance_summary:
            self.target_slot_map = {}
            self.target_tangent_map = {}
            self.target_slot_var.set("")
            self.target_hair_var.set("")
            self.target_slot_combo.configure(values=[])
            return
        slots = appearance_asset_slots(self.appearance_summary, self.asset_type_var.get())
        if not slots:
            self.target_slot_map = {}
            self.target_tangent_map = {}
            self.target_slot_var.set("")
            self.target_hair_var.set("")
            self.target_slot_combo.configure(values=[])
            return
        self.target_slot_map = {label: key for label, key, _tangentspace in slots}
        self.target_tangent_map = {label: tangentspace for label, _key, tangentspace in slots}
        labels = list(self.target_slot_map.keys())
        default_label = next((label for label in labels if " default)" in label), labels[0])
        self.target_slot_var.set(default_label)
        self.target_hair_var.set(self.target_slot_map[default_label])
        self.target_slot_combo.configure(values=labels)

    def _show_details(self):
        option = self._selected_option()
        self.details_text.configure(state=tk.NORMAL)
        self.details_text.delete("1.0", tk.END)
        blocks = []
        if self.appearance_summary:
            blocks.append(format_appearance_summary(self.appearance_summary))
        if option:
            target_tangent = self.target_tangent_map.get(self.target_slot_var.get(), False)
            internal_tangent = option.tangentspace if option.asset_type == "facialhair" else target_tangent
            tangent_suffix = "_tangentspace" if target_tangent else ""
            asset_label = "hair" if option.asset_type == "hair" else "facial hair"
            lines = [
                f"Selected archive {asset_label}:",
                f"Origin: {option.origin}",
                f"Player: {option.player_name or 'unknown'}",
                f"Geo: {option.source_geo.name if option.source_geo else option.archive_geo}",
                f"Item: {option.source_item.name if option.source_item else option.archive_item or 'none found'}",
                f"Config: {option.source_config.name if option.source_config else option.archive_config or 'none found'}",
                f"Source folder: {option.source_geo.parent if option.source_geo else 'game archive'}",
                "",
                f"Internal root: {option.root_name or '?'}",
                f"LOD0 verts: {option.lod_verts or '?'}",
                f"Radius: {option.radius if option.radius is not None else '?'}",
                f"Center Y: {option.center_y if option.center_y is not None else '?'}",
                "",
                "Install target:",
                f"Overwrite slot: {self.target_slot_var.get()}",
                f"png{self.target_png_var.get()}_geo_{self.target_hair_var.get()}{tangent_suffix}.iff",
                f"png{self.target_png_var.get()}_item_{self.target_hair_var.get()}.iff",
                (
                    f"Internal SCNE: {'tangent' if internal_tangent else 'non-tangent'}"
                    if option.asset_type == "facialhair"
                    else ""
                ),
                f"Source config copy: {'on' if self.copy_config_var.get() else 'off'}",
            ]
            blocks.append("\n".join(lines))
        if blocks:
            self.details_text.insert("1.0", "\n\n" + ("-" * 40) + "\n\n".join(blocks) if len(blocks) > 1 else blocks[0])
        self.details_text.configure(state=tk.DISABLED)

    def _import_appearance_iff(self):
        initial_dir = TARGET_SIG_DIR if TARGET_SIG_DIR.exists() else GAME_ROOT
        filename = filedialog.askopenfilename(
            title="Import player appearance IFF",
            initialdir=str(initial_dir),
            filetypes=[("IFF files", "*.iff"), ("All files", "*.*")],
        )
        if not filename:
            return
        try:
            summary = parse_appearance_iff(Path(filename))
        except Exception as exc:
            messagebox.showerror("Import Failed", str(exc))
            return
        self.appearance_summary = summary
        if summary.png_id:
            self.target_png_var.set(summary.png_id)
        self._update_target_slots_from_appearance()
        self.status_var.set(
            f"Imported {Path(filename).name}: {len(summary.configs)} configs, default {summary.default_config or '?'}"
        )
        self._show_details()

    def _open_selected_in_blender(self):
        option = self._selected_option()
        if not option:
            messagebox.showwarning("No Selection", "Pick a hair first.")
            return
        if not BLENDER_EXE.exists():
            messagebox.showerror("Blender Not Found", f"Blender 5.1 was not found at:\n{BLENDER_EXE}")
            return
        if not BLENDER_IMPORT_SCRIPT.exists():
            messagebox.showerror("Import Script Missing", f"Missing Blender handoff script:\n{BLENDER_IMPORT_SCRIPT}")
            return

        staging_dir = Path(tempfile.mkdtemp(prefix="nba2k_hair_blender_"))
        source_name = option.source_geo.name if option.source_geo else Path(option.archive_geo or "selected_hair.iff").name
        staged_iff = staging_dir / source_name
        try:
            if option.source_geo:
                shutil.copy2(option.source_geo, staged_iff)
            elif option.archive_geo:
                extract_archive_iff_fallback(option.archive_geo, staged_iff)
            else:
                raise FileNotFoundError("The selected hair has no source IFF.")

            subprocess.Popen(
                [
                    str(BLENDER_EXE),
                    "--factory-startup",
                    "--python",
                    str(BLENDER_IMPORT_SCRIPT),
                    "--",
                    str(staged_iff),
                ],
                cwd=str(BLENDER_EXE.parent),
            )
        except Exception as exc:
            shutil.rmtree(staging_dir, ignore_errors=True)
            messagebox.showerror("Blender Export Failed", str(exc))
            return

        self.status_var.set(f"Opening {source_name} in Blender 5.1...")

    def _open_selected_with_autofit(self):
        self._launch_selected_fit("baseline")

    def _open_selected_with_tangent_fit(self):
        self._launch_selected_fit("tangent")

    def _launch_selected_fit(self, fit_mode):
        option = self._selected_option()
        if not option:
            messagebox.showwarning("No Selection", "Pick a hair first.")
            return
        if option.asset_type != "hair":
            messagebox.showwarning("Hair Only", "Auto-Fit currently supports head hair, not facial hair.")
            return
        target_png = re.sub(r"\D", "", self.target_png_var.get())
        if not target_png:
            messagebox.showwarning("No Target Player", "Import an appearance IFF or enter a target PNG id first.")
            return
        if not BLENDER_EXE.exists():
            messagebox.showerror("Blender Not Found", f"Blender 5.1 was not found at:\n{BLENDER_EXE}")
            return
        if not BLENDER_AUTOFIT_SCRIPT.exists():
            messagebox.showerror("Hair Fixer Missing", f"Missing Blender Hair Fixer script:\n{BLENDER_AUTOFIT_SCRIPT}")
            return

        staging_dir = Path(tempfile.mkdtemp(prefix="nba2k_hair_fixer_"))
        source_name = option.source_geo.name if option.source_geo else Path(option.archive_geo or "selected_hair.iff").name
        staged_hair = staging_dir / source_name
        donor_head = staging_dir / f"donor_png{option.source_png}.iff"
        target_head = staging_dir / f"target_png{target_png}.iff"
        target_key = self.target_hair_var.get().strip() or option.hair_key
        target_output = TARGET_SIG_DIR / f"png{target_png}_geo_{target_key}.iff"
        self.status_var.set(
            f"Preparing png{option.source_png} hair and png{target_png} head for Blender..."
        )
        self.update_idletasks()
        try:
            if option.source_geo:
                shutil.copy2(option.source_geo, staged_hair)
            elif option.archive_geo:
                extract_archive_iff_fallback(option.archive_geo, staged_hair)
            else:
                raise FileNotFoundError("The selected hair has no source IFF.")

            extract_archive_iff_fallback(f"char/sig/png{option.source_png}.iff", donor_head)
            extract_archive_iff_fallback(f"char/sig/png{target_png}.iff", target_head)
            with BLENDER_AUTOFIT_LOG.open("w", encoding="utf-8") as blender_log:
                subprocess.Popen(
                    [
                        str(BLENDER_EXE),
                        "--factory-startup",
                        "--python",
                        str(BLENDER_AUTOFIT_SCRIPT),
                        "--",
                        str(staged_hair),
                        str(donor_head),
                        str(target_head),
                        str(target_output),
                        target_key,
                        fit_mode,
                    ],
                    cwd=str(BLENDER_EXE.parent),
                    stdout=blender_log,
                    stderr=subprocess.STDOUT,
                )
        except Exception as exc:
            shutil.rmtree(staging_dir, ignore_errors=True)
            messagebox.showerror("Hair Fixer Failed", str(exc))
            return

        mode_label = "Tangent Fit Test" if fit_mode == "tangent" else "Auto-Fit"
        self.status_var.set(
            f"Blender launched with {mode_label}. Export only with NBA 2K Safe Fitted Hair (.IFF). "
            f"Log: {BLENDER_AUTOFIT_LOG.name}"
        )

    def _install_selected(self):
        option = self._selected_option()
        if not option:
            messagebox.showwarning("No Selection", "Pick a hair first.")
            return
        if not re.sub(r"\D", "", self.target_png_var.get()):
            messagebox.showwarning("No Target Player", "Import an appearance IFF or enter a target PNG id first.")
            return
        target_key = self.target_hair_var.get().strip()
        if not target_key:
            messagebox.showwarning("No Target Asset", "Choose an overwrite slot or enter a target asset key first.")
            return
        target_tangentspace = self.target_tangent_map.get(self.target_slot_var.get(), False)
        tangent_suffix = "_tangentspace" if target_tangentspace else ""
        tangent_mode_changed = option.asset_type == "facialhair" and target_tangentspace != option.tangentspace
        if option.hair_key != target_key or tangent_mode_changed:
            mode_text = (
                f"\nThe outer IFF will keep the slot's tangent-space name, while the internal SCNE remains "
                f"{'tangent' if option.tangentspace else 'non-tangent'}.\n"
                if tangent_mode_changed
                else ""
            )
            proceed = messagebox.askyesno(
                "Confirm Facial-Hair Transfer" if tangent_mode_changed else "Source/Target Names Differ",
                f"The selected source is {option.hair_key}; the imported appearance slot is {target_key}.\n\n"
                f"I will create the target files using the appearance reference name:\n"
                f"png{self.target_png_var.get()}_geo_{target_key}{tangent_suffix}.iff\n"
                f"png{self.target_png_var.get()}_item_{target_key}.iff\n"
                f"{mode_text}\n"
                "Continue?",
            )
            if not proceed:
                return
        tangent_staging = None
        try:
            install_option = option
            if option.asset_type == "hair":
                install_option, tangent_staging = self._prepare_tangent_fitted_install(
                    option,
                    re.sub(r"\D", "", self.target_png_var.get()),
                )
            target_geo, target_item, target_config, backups = install_hair(
                install_option,
                self.target_png_var.get(),
                target_key,
                self.copy_item_var.get(),
                self.copy_config_var.get(),
                target_tangentspace,
            )
        except Exception as exc:
            messagebox.showerror("Install Failed", str(exc))
            return
        finally:
            if tangent_staging:
                shutil.rmtree(tangent_staging, ignore_errors=True)

        backup_text = "\n".join(str(path) for path in backups) if backups else "No existing files needed backup."
        item_text = f"\nInstalled item: {target_item}" if target_item else "\nNo item file copied."
        config_text = f"\nInstalled config: {target_config}" if target_config else "\nNo config file copied."
        source_name = option.source_geo.name if option.source_geo else Path(option.archive_geo or "").name
        self.status_var.set(f"Installed {source_name} -> {target_geo.name}")
        self._show_details()
        messagebox.showinfo(
            "Installed",
            f"Installed geo:\n{target_geo}{item_text}{config_text}\n\nBackups:\n{backup_text}",
        )

    def _prepare_tangent_fitted_install(self, option, target_png):
        return prepare_tangent_fitted_option(
            option,
            target_png,
            status_callback=lambda text: (self.status_var.set(text), self.update_idletasks()),
        )


if __name__ == "__main__":
    app = HairPicker()
    app.mainloop()
