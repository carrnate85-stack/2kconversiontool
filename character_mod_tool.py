import binascii
import copy
import ctypes
import glob
import hashlib
import importlib.util
import io
import json
import logging
import os
import platform
import queue
import re
import shutil
import stat
import struct
import subprocess
import sys
import tempfile
import threading
import time
import traceback
import tkinter as tk
import zipfile
from dataclasses import dataclass
from datetime import datetime
from logging.handlers import RotatingFileHandler
from pathlib import Path
from tkinter import colorchooser, filedialog, messagebox, simpledialog, ttk

import app_settings


TEXT_EXTENSIONS = {
    ".json",
    ".scne",
    ".txt",
    ".xml",
    ".material",
    ".fx",
    ".shader",
    ".rdat",
}

DYNAMIC_BODY_DIR = app_settings.resource_path("dynamic_body_package")
DYNAMIC_BODY_SCNE = os.path.join(DYNAMIC_BODY_DIR, "hihead_DYN_BODY.SCNE")
DYNAMIC_BODY_MORPHS = os.path.join(DYNAMIC_BODY_DIR, "Morph.zip")
ACCESSORY_TEMPLATE_DIR = app_settings.resource_path("accessory_templates")
HAIR_TOOLS_DIR = app_settings.resource_path("hair_tools")
HAIR_BACKEND_PATH = os.path.join(HAIR_TOOLS_DIR, "hair_picker.py")
BUILT_IN_GLASSES_DIR = app_settings.resource_path("built_in_glasses")
BUILT_IN_SANDERS_DIR = os.path.join(BUILT_IN_GLASSES_DIR, "sanders")
BUILT_IN_SANDERS_GEO = os.path.join(BUILT_IN_SANDERS_DIR, "png7736_geo_goggles_sanders.iff")
BUILT_IN_SANDERS_ITEM = os.path.join(BUILT_IN_SANDERS_DIR, "png7736_item_goggles_sanders.iff")
BUILT_IN_MOSES_DIR = os.path.join(BUILT_IN_GLASSES_DIR, "moses")
BUILT_IN_MOSES_GEO = os.path.join(BUILT_IN_MOSES_DIR, "png0763_geo_goggles_moses.iff")
BUILT_IN_MOSES_ITEM = os.path.join(BUILT_IN_MOSES_DIR, "png0763_item_goggles_moses.iff")
BUILT_IN_KAREEM_DIR = os.path.join(BUILT_IN_GLASSES_DIR, "kareem")
BUILT_IN_KAREEM_GEO = os.path.join(BUILT_IN_KAREEM_DIR, "png0776_geo_goggles_kareem.iff")
BUILT_IN_KAREEM_ITEM = os.path.join(BUILT_IN_KAREEM_DIR, "png0776_item_goggles_kareem.iff")
BUILT_IN_RAMBIS_DIR = os.path.join(BUILT_IN_GLASSES_DIR, "rambis")
BUILT_IN_RAMBIS_GEO = os.path.join(BUILT_IN_RAMBIS_DIR, "png1977_geo_goggles_rambis.iff")
BUILT_IN_RAMBIS_ITEM = os.path.join(BUILT_IN_RAMBIS_DIR, "png1977_item_goggles_rambis.iff")
BUILT_IN_CURRENT_DIR = os.path.join(BUILT_IN_GLASSES_DIR, "current")
BUILT_IN_CURRENT_GEO = os.path.join(BUILT_IN_CURRENT_DIR, "png4419_geo_goggles_current_geometry.iff")
BUILT_IN_CURRENT_ITEM = os.path.join(BUILT_IN_CURRENT_DIR, "png4419_item_goggles_current_geometry.iff")
BUILT_IN_CUSTOM_DIR = os.path.join(BUILT_IN_GLASSES_DIR, "custom")
BUILT_IN_CUSTOM_GEO = os.path.join(BUILT_IN_CUSTOM_DIR, "png4454_geo_goggles_custom_geo.iff")
BUILT_IN_CUSTOM_ITEM = os.path.join(BUILT_IN_CUSTOM_DIR, "png4454_item_goggles_custom_geo.iff")
BUILT_IN_GLASSES = {
    "Sanders Glasses": (BUILT_IN_SANDERS_GEO, BUILT_IN_SANDERS_ITEM),
    "Moses Goggles": (BUILT_IN_MOSES_GEO, BUILT_IN_MOSES_ITEM),
    "Kareem Goggles": (BUILT_IN_KAREEM_GEO, BUILT_IN_KAREEM_ITEM),
    "Rambis Glasses": (BUILT_IN_RAMBIS_GEO, BUILT_IN_RAMBIS_ITEM),
    "Current Glasses": (BUILT_IN_CURRENT_GEO, BUILT_IN_CURRENT_ITEM),
    "Custom Goggles": (BUILT_IN_CUSTOM_GEO, BUILT_IN_CUSTOM_ITEM),
}
AVAILABLE_BUILT_IN_GLASSES = {
    label: paths for label, paths in BUILT_IN_GLASSES.items() if all(os.path.isfile(path) for path in paths)
}
FULL_SWAP_BRIDGE = app_settings.resource_path("blender_full_swap_bridge.py")
HEADBAND_SWAP_BRIDGE = app_settings.resource_path("blender_headband_swap_bridge.py")
BUNDLED_HEADBAND_SWAP_TOOL = app_settings.resource_path(
    os.path.join("blender_tools", "NBA_Character_HeadbandSwap")
)
OPEN_OUTPUT_BRIDGE = app_settings.resource_path("blender_open_output.py")
ROSTER_CLI = app_settings.resource_path("tools", "live_roster", "roster.exe")
ROSTER_CLI_SHA256 = "E1A55F8BCD032E2C2719535BCE5CB454CD74EE52AEAFDABFFB4027A9D8E32659"
ROSTER_WRITABLE_FIELDS = ("FirstName", "LastName", "CyberfaceID", "HeadshotID", "PortraitID")
ROSTER_DISPLAY_FIELDS = (
    ("FirstName", "First name"),
    ("LastName", "Last name"),
    ("PlayerKey", "Player key"),
    ("Nickname", "Nickname"),
    ("JerseyNickname", "Jersey nickname"),
    ("CyberfaceID", "Cyberface ID"),
    ("HeadshotID", "Headshot ID"),
    ("PortraitID", "Portrait ID"),
)
DYNAMIC_BODY_START_MARKER = '"forearmUpperTHICK"'
DYNAMIC_BODY_END_MARKER = '"bustTHICK"'
TATTOO_TEXTURE_PREFIXES = ("chest_color", "legs_color", "torso_color")
APPEARANCE_ENTRY_NAMES = ("appearance_info.json", "appearance_info.RDAT", "appearance_info.rdat")
OLDER_BODY_FIT_SUFFIXES = (
    ".tx", ".ty", ".tz",
    ".rx", ".ry", ".rz",
    ".sx", ".sy", ".sz",
    ".r1", ".r2",
)
APP_VERSION = "1.0.130-beta"


LOGGER = logging.getLogger("character_mod_tool")
LOG_PATH = app_settings.LOG_DIR / "character_mod_tool.log"
if not LOGGER.handlers:
    LOGGER.setLevel(logging.INFO)
    try:
        app_settings.LOG_DIR.mkdir(parents=True, exist_ok=True)
        _log_handler = RotatingFileHandler(LOG_PATH, maxBytes=2_000_000, backupCount=3, encoding="utf-8")
    except OSError:
        _fallback_log_dir = Path(tempfile.gettempdir()) / "CharacterModTool" / "logs"
        _fallback_log_dir.mkdir(parents=True, exist_ok=True)
        LOG_PATH = _fallback_log_dir / "character_mod_tool.log"
        _log_handler = RotatingFileHandler(LOG_PATH, maxBytes=2_000_000, backupCount=3, encoding="utf-8")
    _log_handler.setFormatter(logging.Formatter("%(asctime)s | %(levelname)s | %(message)s"))
    LOGGER.addHandler(_log_handler)


@dataclass
class ArchiveEntry:
    name: str
    data: bytes
    info: zipfile.ZipInfo

    @property
    def size(self):
        return len(self.data)

    @property
    def ext(self):
        return os.path.splitext(self.name)[1].lower() or "<none>"


@dataclass
class ValidationResult:
    severity: str
    check: str
    details: str


DDS_DXGI_FORMATS = {
    71: "BC1_UNORM",
    72: "BC1_UNORM_SRGB",
    77: "BC3_UNORM",
    78: "BC3_UNORM_SRGB",
    80: "BC4_UNORM",
    83: "BC5_UNORM",
    95: "BC6H_UF16",
    96: "BC6H_SF16",
    98: "BC7_UNORM",
    99: "BC7_UNORM_SRGB",
}

DDS_LEGACY_FORMATS = {
    b"DXT1": "BC1_UNORM",
    b"DXT3": "BC2_UNORM",
    b"DXT5": "BC3_UNORM",
    b"ATI1": "BC4_UNORM",
    b"BC4U": "BC4_UNORM",
    b"ATI2": "BC5_UNORM",
    b"BC5U": "BC5_UNORM",
}


def normalized_texture_format(value):
    result = str(value or "").upper()
    if result.endswith("_SRGB") and "_UNORM_SRGB" not in result:
        result = result[:-5] + "_UNORM_SRGB"
    return result


def parse_dds_header(data):
    if len(data) < 128 or data[:4] != b"DDS ":
        raise ValueError("DDS header is missing or truncated")
    if struct.unpack_from("<I", data, 4)[0] != 124:
        raise ValueError("DDS header size is not 124 bytes")
    height = struct.unpack_from("<I", data, 12)[0]
    width = struct.unpack_from("<I", data, 16)[0]
    mips = struct.unpack_from("<I", data, 28)[0] or 1
    fourcc = data[84:88]
    header_size = 128
    if fourcc == b"DX10":
        if len(data) < 148:
            raise ValueError("DDS DX10 header is truncated")
        header_size = 148
        texture_format = DDS_DXGI_FORMATS.get(struct.unpack_from("<I", data, 128)[0], "DXGI_UNKNOWN")
    else:
        texture_format = DDS_LEGACY_FORMATS.get(fourcc, fourcc.decode("ascii", errors="replace").strip("\x00 "))
    return {
        "width": width,
        "height": height,
        "mips": mips,
        "format": texture_format,
        "header_size": header_size,
        "pixel_data_size": len(data) - header_size,
    }


def parse_txtr_metadata(data):
    text = data.decode("utf-8-sig")
    try:
        parsed = json.loads(text)
    except json.JSONDecodeError:
        parsed = json.loads("{" + text + "}")
    if not isinstance(parsed, dict) or not parsed:
        raise ValueError("TXTR root is empty")
    value = next(iter(parsed.values()))
    if not isinstance(value, dict):
        raise ValueError("TXTR metadata is not an object")
    return value


def collect_scne_nodes(value, resources, lod_nodes):
    if isinstance(value, dict):
        binary = value.get("Binary")
        if isinstance(binary, str):
            resources.append((binary, value.get("Size")))
        if isinstance(value.get("Lods"), list) and isinstance(value.get("Prim"), list):
            lod_nodes.append(value)
        for child in value.values():
            collect_scne_nodes(child, resources, lod_nodes)
    elif isinstance(value, list):
        for child in value:
            collect_scne_nodes(child, resources, lod_nodes)


def validate_archive_snapshot(
    entry_names,
    data_by_name,
    allow_shared_textures=False,
    shared_texture_names=(),
):
    results = []
    shared_texture_names = {str(name).lower() for name in shared_texture_names}
    active_names = list(entry_names)
    lower_names = {name.lower(): name for name in active_names}
    base_names = {os.path.basename(name).lower(): name for name in active_names}

    if not active_names:
        return [ValidationResult("ERROR", "Archive", "The archive has no active entries.")]
    duplicates = sorted({name for name in active_names if active_names.count(name) > 1})
    if duplicates:
        results.append(ValidationResult("ERROR", "Archive entries", "Duplicate names: " + ", ".join(duplicates[:5])))
    else:
        results.append(ValidationResult("PASS", "Archive entries", f"{len(active_names)} uniquely named active entries."))
    empty = [name for name in active_names if len(data_by_name.get(name, b"")) == 0]
    if empty:
        results.append(ValidationResult("WARNING", "Empty entries", ", ".join(empty[:8])))

    vcz_entries = [
        name for name in active_names
        if len(data_by_name.get(name, b"")) >= 16 and data_by_name[name][12:16] == b"VCZ\x00"
    ]
    if vcz_entries:
        results.append(ValidationResult("ERROR", "Compressed VCZ resources", ", ".join(vcz_entries[:8])))
    else:
        results.append(ValidationResult("PASS", "Compressed resources", "No embedded VCZ payloads remain."))

    appearance_names = [name for name in active_names if os.path.basename(name).lower() in {item.lower() for item in APPEARANCE_ENTRY_NAMES}]
    if appearance_names:
        appearance_name = appearance_names[0]
        appearance, _wrapped, error = try_parse_structured_text(appearance_name, data_by_name[appearance_name])
        if appearance is None:
            results.append(ValidationResult("ERROR", "Appearance data", error or "appearance_info is invalid."))
        else:
            section = appearance.get("accessory_items") if isinstance(appearance, dict) else None
            if not isinstance(section, dict):
                results.append(ValidationResult("ERROR", "Appearance data", "accessory_items is missing."))
            else:
                items = section.get("items") or []
                configurations = section.get("configurations") or []
                item_names = [str(item.get("name")) for item in items if isinstance(item, dict) and item.get("name")]
                duplicate_items = sorted({name for name in item_names if item_names.count(name) > 1})
                unknown = []
                config_names = []
                for config in configurations:
                    if not isinstance(config, dict):
                        continue
                    if config.get("name"):
                        config_names.append(str(config["name"]))
                    unknown.extend(str(name) for name in config.get("items", []) if str(name) not in item_names)
                default_config = str(section.get("default_config") or "")
                if duplicate_items:
                    results.append(ValidationResult("ERROR", "Appearance items", "Duplicate item definitions: " + ", ".join(duplicate_items)))
                if unknown:
                    results.append(ValidationResult("ERROR", "Appearance references", "Undefined items: " + ", ".join(sorted(set(unknown))[:8])))
                if default_config and default_config not in config_names:
                    results.append(ValidationResult("ERROR", "Default configuration", f"{default_config} is not defined."))
                if not duplicate_items and not unknown and (not default_config or default_config in config_names):
                    results.append(ValidationResult("PASS", "Appearance data", f"{len(items)} items and {len(configurations)} configurations are internally consistent."))
    else:
        results.append(ValidationResult("INFO", "Appearance data", "No appearance_info entry; valid for geo, item, face, and config IFFs."))

    txtr_names = [name for name in active_names if name.lower().endswith(".txtr")]
    dds_names = [name for name in active_names if name.lower().endswith(".dds")]
    paired_dds = set()
    texture_errors = 0
    texture_warnings = 0
    for txtr_name in txtr_names:
        logical = os.path.splitext(os.path.basename(txtr_name))[0].lower()
        matches = [name for name in dds_names if os.path.basename(name).lower().startswith(logical + ".") or os.path.basename(name).lower() == logical + ".dds"]
        if not matches:
            if allow_shared_textures or logical in shared_texture_names:
                results.append(
                    ValidationResult(
                        "INFO",
                        "Shared texture reference",
                        f"{os.path.basename(txtr_name)} uses its native manifest/shared DDS resource.",
                    )
                )
                continue
            texture_errors += 1
            results.append(ValidationResult("ERROR", "Texture pair", f"{os.path.basename(txtr_name)} has no matching DDS."))
            continue
        paired_dds.update(matches)
        if len(matches) > 1:
            texture_warnings += 1
            results.append(ValidationResult("WARNING", "Texture pair", f"{os.path.basename(txtr_name)} has {len(matches)} matching DDS files."))
        dds_name = matches[0]
        try:
            metadata = parse_txtr_metadata(data_by_name[txtr_name])
            dds = parse_dds_header(data_by_name[dds_name])
        except Exception as exc:
            texture_errors += 1
            results.append(ValidationResult("ERROR", "Texture metadata", f"{logical}: {exc}"))
            continue
        mismatches = []
        expected_format = normalized_texture_format(metadata.get("Format"))
        actual_format = normalized_texture_format(dds["format"])
        if expected_format and actual_format and expected_format != actual_format:
            mismatches.append(f"format TXTR={expected_format}, DDS={actual_format}")
        for key, label in (("Width", "width"), ("Height", "height"), ("Mips", "mips")):
            expected = metadata.get(key)
            if expected is not None and int(expected) != int(dds[label]):
                mismatches.append(f"{key} TXTR={expected}, DDS={dds[label]}")
        expected_pixels = metadata.get("PixelDataSize")
        if expected_pixels is not None and int(expected_pixels) != dds["pixel_data_size"]:
            mismatches.append(f"PixelDataSize TXTR={expected_pixels}, DDS={dds['pixel_data_size']}")
        binary = str(metadata.get("Binary") or "")
        if binary and os.path.splitext(os.path.basename(binary))[0].lower() != os.path.splitext(os.path.basename(dds_name))[0].lower():
            mismatches.append(f"Binary={binary} does not match {os.path.basename(dds_name)}")
        if binary.lower().endswith(".dds"):
            packed_fields = [
                key for key in ("Segments", "CompressionMethod", "Twiddled")
                if key in metadata
            ]
            if packed_fields:
                mismatches.append(
                    "editable DDS TXTR retains packed-resource fields: "
                    + ", ".join(packed_fields)
                )
        if mismatches:
            texture_errors += 1
            results.append(ValidationResult("ERROR", "Texture metadata", f"{logical}: " + "; ".join(mismatches)))
    unpaired = [name for name in dds_names if name not in paired_dds]
    if unpaired:
        texture_warnings += 1
        results.append(ValidationResult("WARNING", "Unpaired DDS", ", ".join(os.path.basename(name) for name in unpaired[:8])))
    if txtr_names and not texture_errors and not texture_warnings:
        results.append(ValidationResult("PASS", "Texture resources", f"{len(txtr_names)} TXTR/DDS pair(s) agree on format, dimensions, mips, and payload size."))
    elif not txtr_names and not dds_names:
        results.append(ValidationResult("INFO", "Texture resources", "No embedded texture resources."))

    torso_txtr = any(os.path.basename(name).lower() == "torso_color_o.txtr" for name in txtr_names)
    torso_dds = any(os.path.basename(name).lower().startswith("torso_color_o.") for name in dds_names)
    if torso_txtr != torso_dds:
        if (allow_shared_textures or "torso_color_o" in shared_texture_names) and torso_txtr:
            results.append(
                ValidationResult(
                    "INFO",
                    "Protected torso texture",
                    "torso_color_o uses its native manifest/shared DDS resource.",
                )
            )
        else:
            results.append(ValidationResult("ERROR", "Protected torso texture", "torso_color_o TXTR/DDS pair is incomplete."))
    elif torso_txtr:
        results.append(ValidationResult("PASS", "Protected torso texture", "The accessory/glasses torso texture pair is present."))

    scne_names = [name for name in active_names if name.lower().endswith(".scne")]
    scne_errors = 0
    scne_warnings = 0
    full_detail_nodes = 0
    for scne_name in scne_names:
        parsed, _wrapped, error = try_parse_structured_text(scne_name, data_by_name[scne_name])
        if parsed is None:
            scne_errors += 1
            results.append(ValidationResult("ERROR", "SCNE data", f"{os.path.basename(scne_name)}: {error or 'invalid structured data'}"))
            continue
        resources = []
        lod_nodes = []
        collect_scne_nodes(parsed, resources, lod_nodes)
        missing_resources = []
        for binary, _size in resources:
            candidates = {binary.lower(), os.path.basename(binary).lower()}
            if binary.lower().endswith(".gz"):
                candidates.update({binary[:-3].lower() + ".bin", os.path.basename(binary[:-3]).lower() + ".bin"})
            if not any(candidate in lower_names or candidate in base_names for candidate in candidates):
                missing_resources.append(binary)
        if missing_resources:
            scne_warnings += 1
            results.append(ValidationResult("WARNING", "SCNE resources", f"{os.path.basename(scne_name)} references non-embedded/shared resources: " + ", ".join(missing_resources[:6])))
        for node in lod_nodes:
            lods = node.get("Lods") or []
            lod_verts = [lod.get("LodVerts") for lod in lods if isinstance(lod, dict)]
            if not lods or any(not isinstance(value, int) or value <= 0 for value in lod_verts):
                scne_errors += 1
                results.append(ValidationResult("ERROR", "SCNE LODs", f"{os.path.basename(scne_name)} contains invalid LodVerts."))
                continue
            if len(set(lod_verts)) == 1:
                prim_lists = [prim.get("LodList") or [] for prim in node.get("Prim", []) if isinstance(prim, dict)]
                if prim_lists and all(len({(lod.get('Start'), lod.get('Count')) for lod in items if isinstance(lod, dict)}) <= 1 for items in prim_lists):
                    full_detail_nodes += 1
            for prim in node.get("Prim", []):
                if not isinstance(prim, dict):
                    continue
                lod_list = prim.get("LodList") or []
                if lod_list and len(lod_list) > len(lods):
                    scne_warnings += 1
                    results.append(ValidationResult("WARNING", "SCNE LODs", f"{os.path.basename(scne_name)} has {len(lods)} model LODs but a primitive contains {len(lod_list)} ranges."))
                if any(int(lod.get("Start", -1)) < 0 or int(lod.get("Count", -1)) <= 0 for lod in lod_list if isinstance(lod, dict)):
                    scne_errors += 1
                    results.append(ValidationResult("ERROR", "SCNE indices", f"{os.path.basename(scne_name)} has an invalid primitive index range."))
    if scne_names and not scne_errors and not scne_warnings:
        detail = f"{len(scne_names)} SCNE file(s) parsed successfully."
        if full_detail_nodes:
            detail += f" {full_detail_nodes} model node(s) force full-detail LOD0 at every distance."
        results.append(ValidationResult("PASS", "SCNE structure", detail))
    elif not scne_names:
        results.append(ValidationResult("INFO", "SCNE structure", "No SCNE entry; valid for texture/config/item archives."))

    return results


def is_probably_text(name, data):
    if os.path.basename(name).lower() == "appearance_info.rdat":
        return len(data) >= 16
    ext = os.path.splitext(name)[1].lower()
    if ext not in TEXT_EXTENSIONS:
        return False
    sample = data[:4096]
    if b"\x00" in sample:
        return False
    try:
        sample.decode("utf-8")
        return True
    except UnicodeDecodeError:
        return False


def decode_text(data):
    return data.decode("utf-8-sig")


def is_appearance_rdat(name):
    return os.path.basename(name).lower() == "appearance_info.rdat"


def decode_entry_text(name, data):
    if is_appearance_rdat(name):
        return data[16:].decode("utf-8-sig", errors="replace")
    return decode_text(data)


def serialize_appearance_rdat(original_data, text):
    header = bytearray(original_data[:16] if len(original_data) >= 16 else b"\x00" * 16)
    encoded = text.encode("utf-8")
    header[:4] = len(encoded).to_bytes(4, "little")
    return bytes(header) + encoded


def try_parse_structured_text(name, data):
    if not is_probably_text(name, data):
        return None, False, ""
    text = decode_entry_text(name, data)
    try:
        return json.loads(text), False, ""
    except json.JSONDecodeError as first_error:
        if os.path.splitext(name)[1].lower() == ".scne":
            wrapped = "{\n" + text.strip().rstrip(",") + "\n}"
            try:
                return json.loads(wrapped), True, ""
            except json.JSONDecodeError:
                return None, False, str(first_error)
        return None, False, str(first_error)


def serialize_structured(data, wrapped_scne):
    text = json.dumps(data, indent="\t", ensure_ascii=False)
    if wrapped_scne:
        lines = text.splitlines()
        if len(lines) >= 2:
            text = "\n".join(lines[1:-1])
            if text.startswith("\t"):
                text = text.replace("\t", "", 1)
    return (text.rstrip() + "\n").encode("utf-8")


def serialize_structured_entry(name, original_data, data, wrapped_scne=False):
    text = json.dumps(data, indent="\t", ensure_ascii=False)
    if wrapped_scne:
        lines = text.splitlines()
        if len(lines) >= 2:
            text = "\n".join(lines[1:-1])
            if text.startswith("\t"):
                text = text.replace("\t", "", 1)
    text = text.rstrip() + "\n"
    if is_appearance_rdat(name):
        return serialize_appearance_rdat(original_data, text)
    return text.encode("utf-8")


def is_older_body_fit_key(key):
    lower = key.lower()
    return any(lower.endswith(suffix) for suffix in OLDER_BODY_FIT_SUFFIXES)


def json_value_preview(value):
    if isinstance(value, dict):
        return f"object ({len(value)} keys)"
    if isinstance(value, list):
        return f"array ({len(value)} items)"
    if isinstance(value, str):
        return value
    return json.dumps(value)


def set_json_path(root, path, value):
    node = root
    for key in path[:-1]:
        node = node[key]
    node[path[-1]] = value


def detect_eol(text):
    return "\r\n" if text.count("\r\n") >= text.count("\n") - text.count("\r\n") else "\n"


def normalize_eol(text, eol):
    return text.replace("\r\n", "\n").replace("\n", eol)


def build_dynamic_body_scne(original_text, replacement_text):
    eol = detect_eol(original_text)
    original_start = original_text.find(DYNAMIC_BODY_START_MARKER)
    original_end = original_text.find(DYNAMIC_BODY_END_MARKER)
    if original_start == -1:
        raise ValueError(f"Start marker not found in hihead.SCNE: {DYNAMIC_BODY_START_MARKER}")
    if original_end == -1 or original_end < original_start:
        raise ValueError(f"End marker not found after start marker in hihead.SCNE: {DYNAMIC_BODY_END_MARKER}")

    replacement_start = replacement_text.find(DYNAMIC_BODY_START_MARKER)
    replacement_end = replacement_text.find(DYNAMIC_BODY_END_MARKER)
    if replacement_start == -1:
        raise ValueError(f"Start marker not found in bundled dynamic-body SCNE: {DYNAMIC_BODY_START_MARKER}")
    if replacement_end == -1:
        replacement_segment = replacement_text[replacement_start:]
    else:
        replacement_segment = replacement_text[replacement_start:replacement_end]

    replacement_segment = normalize_eol(replacement_segment, eol)
    return original_text[:original_start] + replacement_segment + original_text[original_end:]


def read_dynamic_body_package():
    if not os.path.exists(DYNAMIC_BODY_SCNE):
        raise FileNotFoundError(f"Missing dynamic-body SCNE template: {DYNAMIC_BODY_SCNE}")
    if not os.path.exists(DYNAMIC_BODY_MORPHS):
        raise FileNotFoundError(f"Missing dynamic-body morph package: {DYNAMIC_BODY_MORPHS}")

    with open(DYNAMIC_BODY_SCNE, "rb") as handle:
        replacement_text = handle.read().decode("utf-8-sig", "replace")

    morphs = {}
    with zipfile.ZipFile(DYNAMIC_BODY_MORPHS, "r") as archive:
        for info in archive.infolist():
            if info.is_dir():
                continue
            base_name = os.path.basename(info.filename)
            if base_name:
                morphs[base_name] = archive.read(info)
    return replacement_text, morphs


def apply_windows_window_icons(window, icon_path):
    if sys.platform != "win32" or not os.path.isfile(icon_path):
        return
    try:
        user32 = ctypes.windll.user32
        load_image = user32.LoadImageW
        load_image.argtypes = (
            ctypes.c_void_p,
            ctypes.c_wchar_p,
            ctypes.c_uint,
            ctypes.c_int,
            ctypes.c_int,
            ctypes.c_uint,
        )
        load_image.restype = ctypes.c_void_p
        send_message = user32.SendMessageW
        send_message.argtypes = (
            ctypes.c_void_p,
            ctypes.c_uint,
            ctypes.c_size_t,
            ctypes.c_void_p,
        )
        send_message.restype = ctypes.c_ssize_t

        hwnd = ctypes.c_void_p(window.winfo_id())
        handles = []
        for icon_kind, size in ((0, 24), (1, 32)):
            handle = load_image(None, icon_path, 1, size, size, 0x0010)
            if handle:
                send_message(hwnd, 0x0080, icon_kind, handle)
                handles.append(handle)
        window._windows_icon_handles = handles
    except (AttributeError, OSError):
        LOGGER.warning("Could not apply native Windows window icons.", exc_info=True)


class CharacterModTool(tk.Tk):
    def __init__(self):
        if sys.platform == "win32":
            try:
                ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID(
                    f"CharacterModTool.{APP_VERSION}"
                )
            except (AttributeError, OSError):
                LOGGER.warning("Could not set the Windows application identity.", exc_info=True)
        super().__init__()
        self._app_icon_image = None
        icon_path = app_settings.resource_path("assets", "character_mod_tool_icon.ico")
        if os.path.isfile(icon_path):
            try:
                self.iconbitmap(default=icon_path)
            except tk.TclError:
                LOGGER.warning("Could not apply the Windows app icon.", exc_info=True)
            self.after(100, lambda: apply_windows_window_icons(self, icon_path))
        icon_png_path = app_settings.resource_path("assets", "character_mod_tool_icon.png")
        if sys.platform != "win32" and os.path.isfile(icon_png_path):
            try:
                self._app_icon_image = tk.PhotoImage(file=icon_png_path)
                self.iconphoto(True, self._app_icon_image)
            except tk.TclError:
                LOGGER.warning("Could not apply the Tk app icon.", exc_info=True)
        self.settings = app_settings.load_settings()
        LOGGER.info("Starting Character Mod Tool %s", APP_VERSION)
        self.title(f"Character Mod Tool v{APP_VERSION}")
        self.geometry("1220x760")
        self.minsize(920, 560)

        self.file_path = ""
        self.entries = {}
        self.entry_order = []
        self.modified = {}
        self.removed = set()
        self.current_name = ""
        self.current_json = None
        self.current_json_wrapped = False
        self.tree_paths = {}
        self.appearance_json = None
        self.appearance_paths = {}
        self.texture_export_dir = os.path.join(
            app_settings.ensure_output_dir(self.settings.get("output_dir", "")),
            "texture_exports",
        )
        self.face_iff_path = ""
        self.config_file_paths = []
        self.face_archive_rows = {}
        self.face_texture_rows = {}
        self.related_iff_mods = {}
        self.full_swap_bridge_result = "Not tested"
        self.full_swap_process = None
        self.full_swap_queue = None
        self.full_swap_progress_window = None
        self.full_swap_progress_var = None
        self.full_swap_output_path = ""
        self.full_swap_error = ""
        self.full_swap_success = None
        self.full_swap_cancel_requested = False
        self.body_swap_active = False
        self.headband_bridge_result = "Not tested"
        self.headband_process = None
        self.headband_queue = None
        self.headband_progress_window = None
        self.headband_progress_var = None
        self.headband_output_path = ""
        self.headband_error = ""
        self.headband_success = None
        self.headband_cancel_requested = False
        self.accessory_source_data = None
        self.accessory_target_data = None
        self.accessory_source_entry = ""
        self.accessory_target_entry = ""
        self.accessory_target_original_data = b""
        self.accessory_target_wrapped = False
        self.accessory_rows = {}
        self.accessory_config_rows = {}
        self.hair_backend = None
        self.hair_options = []
        self.filtered_hair_options = []
        self.hair_appearance_summary = None
        self.hair_slot_map = {}
        self.hair_tangent_map = {}

        self.search_var = tk.StringVar()
        self.status_var = tk.StringVar(value="Open a character .iff file to begin.")
        self.everything_swap_source_var = tk.StringVar()
        self.everything_swap_target_var = tk.StringVar()
        self.recent_output_var = tk.StringVar()
        self.recent_output_path_map = {}
        self.recent_output_combos = []
        self.everything_swap_shrinkwrap_var = tk.BooleanVar(value=True)
        self.everything_swap_active = False
        self.everything_swap_source_path = ""
        self.everything_swap_source_package_path = ""
        self.everything_swap_source_package_dir = ""
        self.everything_swap_source_package_main = ""
        self.everything_swap_target_path = ""
        self.everything_swap_final_output = ""
        self.everything_swap_rename_name = ""
        self.everything_swap_stage_dir = ""
        self.everything_swap_include_appearance = False
        self.everything_swap_companion_plan = []
        self.everything_swap_manifest_target_path = ""
        self.everything_swap_manifest_work_dir = ""
        self.everything_swap_manifest_companions = []
        self.everything_swap_target_info_var = tk.StringVar(
            value="Browse for a custom target or select a clean NBA 2K26 player from the manifest."
        )
        self.everything_swap_hair_enabled_var = tk.BooleanVar(value=False)
        self.everything_swap_rename_enabled_var = tk.BooleanVar(value=False)
        self.everything_swap_rename_name_var = tk.StringVar()
        self.everything_swap_hair_source_var = tk.StringVar()
        self.everything_swap_hair_slot_var = tk.StringVar()
        self.everything_swap_hair_status_var = tk.StringVar(
            value="Choose source and target characters to detect a compatible hair swap."
        )
        self.everything_swap_hair_slot_map = {}
        self.everything_swap_hair_source_path = ""
        self.everything_swap_hair_target_key = ""
        self.everything_swap_hair_result = None
        self.full_swap_log_tail = []
        self.last_validation_report = {}
        self.last_full_swap_output = ""
        self.full_swap_source_var = tk.StringVar()
        self.full_swap_target_var = tk.StringVar()
        self.full_swap_blender_var = tk.StringVar(value=self.find_blender_executable())
        self.full_swap_tool_var = tk.StringVar(value=self.find_head_swap_tool())
        self.full_swap_shrinkwrap_var = tk.BooleanVar(value=False)
        self.full_swap_status_var = tk.StringVar(value="Choose source and target character IFFs.")
        self.body_swap_source_var = tk.StringVar()
        self.body_swap_target_var = tk.StringVar()
        self.body_swap_status_var = tk.StringVar(value="Choose source and target character IFFs.")
        self.headband_source_var = tk.StringVar()
        self.headband_target_var = tk.StringVar()
        self.headband_tool_var = tk.StringVar(value=self.find_headband_swap_tool())
        self.headband_status_var = tk.StringVar(
            value="Choose a legacy or NBA 2K25 source and an NBA 2K26 target headband IFF."
        )
        self.accessory_source_var = tk.StringVar()
        self.accessory_target_var = tk.StringVar()
        self.builtin_glasses_var = tk.StringVar(value=next(iter(AVAILABLE_BUILT_IN_GLASSES), ""))
        self.accessory_frame_color_var = tk.StringVar(value="#FFFFFF")
        self.accessory_status_var = tk.StringVar(value="Open a player IFF, then choose a player-fitted png####_geo_goggles IFF.")
        self.hair_asset_type_var = tk.StringVar(value="hair")
        self.hair_search_var = tk.StringVar()
        self.hair_slot_var = tk.StringVar()
        self.hair_copy_item_var = tk.BooleanVar(value=True)
        self.hair_copy_config_var = tk.BooleanVar(value=False)
        self.hair_status_var = tk.StringVar(value="Open a player IFF, then load the hair catalog.")
        self.hair_selection_var = tk.StringVar(value="No hair selected.")
        self.live_roster_status_var = tk.StringVar(value="Ready to read the highlighted NBA 2K26 player.")
        self.live_roster_field_var = tk.StringVar(value="CyberfaceID")
        self.live_roster_value_var = tk.StringVar()
        self.live_roster_busy = False
        self.validator_status_var = tk.StringVar(value="Open an IFF to run game-ready checks.")
        self.validator_results = []
        self.validator_job = None
        self.rename_package_source_var = tk.StringVar()
        self.rename_package_new_name_var = tk.StringVar()
        self.rename_package_delete_original_var = tk.BooleanVar(value=True)
        self.rename_package_status_var = tk.StringVar(
            value="Choose the finished character IFF after all hair and accessory work is complete."
        )
        self.tattoo_source_var = tk.StringVar()
        self.tattoo_target_var = tk.StringVar()
        self.face_swap_source_var = tk.StringVar()
        self.face_swap_target_var = tk.StringVar()
        self.face_swap_target_config_var = tk.StringVar()
        self.face_swap_target_configs = {}
        self.appearance_swap_source_var = tk.StringVar()
        self.appearance_swap_target_var = tk.StringVar()
        self.advanced_dynamic_body_iff_var = tk.StringVar()
        self.dynamic_body_status_var = tk.StringVar(value="Open an IFF to apply the dynamic-body package.")
        self.dynamic_body_last_report = ""

        self._build_ui()
        self.after(200, self.refresh_live_roster_tool_status)
        self.refresh_full_swap_status()
        self.refresh_body_swap_status()
        self.refresh_headband_status()
        if self.settings.get("game_root") and os.path.isfile(self.settings.get("blender_exe", "")):
            self.after(100, self.refresh_hair_catalog)
        else:
            self.hair_status_var.set("Configure the NBA 2K26 and Blender paths in Settings to load the hair catalog.")
            self.after(250, self.offer_first_run_setup)

    def offer_first_run_setup(self):
        missing = []
        if not os.path.isdir(self.settings.get("game_root", "")):
            missing.append("NBA 2K26 folder")
        if not os.path.isfile(self.settings.get("blender_exe", "")):
            missing.append("Blender")
        if missing and messagebox.askyesno(
            "Character Mod Tool Setup",
            "Complete first-run setup before using conversion workflows.\n\nMissing: "
            + ", ".join(missing)
            + "\n\nOpen Settings now?",
        ):
            self.open_settings_dialog()

    def report_callback_exception(self, exc_type, exc_value, exc_traceback):
        details = "".join(traceback.format_exception(exc_type, exc_value, exc_traceback))
        LOGGER.error("Unhandled GUI exception\n%s", details)
        messagebox.showerror(
            "Character Mod Tool",
            f"An unexpected error occurred. A diagnostic log was saved to:\n{LOG_PATH}\n\n{exc_value}",
        )

    def _build_ui(self):
        style = ttk.Style()
        try:
            style.theme_use("clam")
        except tk.TclError:
            pass

        toolbar = ttk.Frame(self, padding=(8, 8, 8, 4))
        toolbar.pack(fill=tk.X)

        ttk.Button(toolbar, text="Open IFF", command=self.open_iff).pack(side=tk.LEFT)
        ttk.Button(toolbar, text="Save As", command=self.save_as).pack(side=tk.LEFT, padx=(6, 0))
        ttk.Button(toolbar, text="Settings", command=self.open_settings_dialog).pack(side=tk.LEFT, padx=(6, 0))
        ttk.Button(toolbar, text="Open Output Folder", command=self.open_outputs_folder).pack(side=tk.LEFT, padx=(6, 0))
        ttk.Button(toolbar, text="Diagnostics", command=self.open_diagnostics_dialog).pack(side=tk.LEFT, padx=(6, 0))
        ttk.Label(toolbar, textvariable=self.status_var).pack(side=tk.LEFT, padx=(16, 0))

        main = ttk.Frame(self)
        main.pack(fill=tk.BOTH, expand=True, padx=8, pady=(4, 8))

        # Kept as a hidden compatibility surface for the low-level entry editing methods.
        self.entry_tree = ttk.Treeview(
            main,
            columns=("type", "size", "status"),
            show="tree headings",
            selectmode="browse",
        )

        right = ttk.Frame(main, padding=6)
        right.pack(fill=tk.BOTH, expand=True)

        self.notebook = ttk.Notebook(right)
        self.notebook.pack(fill=tk.BOTH, expand=True)

        everything_swap = ttk.Frame(self.notebook, padding=8)
        self.notebook.add(everything_swap, text="Full Swap")
        everything_swap_paths = ttk.Frame(everything_swap)
        everything_swap_paths.pack(fill=tk.X)
        everything_swap_paths.columnconfigure(1, weight=1)
        ttk.Label(everything_swap_paths, text="Source Character").grid(
            row=0, column=0, sticky=tk.W, padx=(0, 8), pady=3
        )
        ttk.Entry(
            everything_swap_paths,
            textvariable=self.everything_swap_source_var,
            state="readonly",
        ).grid(row=0, column=1, sticky=tk.EW, pady=3)
        ttk.Button(
            everything_swap_paths,
            text="Browse",
            command=self.browse_everything_swap_source,
        ).grid(row=0, column=2, padx=(6, 0), pady=3)
        ttk.Label(everything_swap_paths, text="Target Character").grid(
            row=1, column=0, sticky=tk.W, padx=(0, 8), pady=3
        )
        ttk.Entry(
            everything_swap_paths,
            textvariable=self.everything_swap_target_var,
            state="readonly",
        ).grid(row=1, column=1, sticky=tk.EW, pady=3)
        ttk.Button(
            everything_swap_paths,
            text="Browse",
            command=self.browse_everything_swap_target,
        ).grid(row=1, column=2, padx=(6, 0), pady=3)
        ttk.Button(
            everything_swap_paths,
            text="Select from Manifest",
            command=self.open_manifest_target_picker,
        ).grid(row=1, column=3, padx=(6, 0), pady=3)
        rename_swap_row = ttk.Frame(everything_swap_paths)
        rename_swap_row.grid(row=2, column=0, columnspan=4, sticky=tk.W, pady=(5, 0))
        ttk.Checkbutton(
            rename_swap_row,
            text="Rename Character Package",
            variable=self.everything_swap_rename_enabled_var,
            command=self.update_everything_swap_rename_state,
        ).pack(side=tk.LEFT)
        ttk.Label(rename_swap_row, text="New PNG ID").pack(side=tk.LEFT, padx=(14, 6))
        self.everything_swap_rename_entry = ttk.Entry(
            rename_swap_row,
            textvariable=self.everything_swap_rename_name_var,
            width=18,
            state=tk.DISABLED,
        )
        self.everything_swap_rename_entry.pack(side=tk.LEFT)
        everything_swap_actions = ttk.Frame(everything_swap_paths)
        everything_swap_actions.grid(row=3, column=0, columnspan=4, sticky=tk.W, pady=(8, 0))
        self.everything_swap_run_button = ttk.Button(
            everything_swap_actions,
            text="Run Full Swap",
            command=self.run_everything_swap,
        )
        self.everything_swap_run_button.pack(side=tk.LEFT)
        self.everything_swap_open_blender_button = ttk.Button(
            everything_swap_actions,
            text="Open Output in Blender",
            command=self.open_full_swap_output_in_blender,
            state=tk.DISABLED,
        )
        self.everything_swap_open_blender_button.pack(side=tk.LEFT, padx=(6, 0))
        ttk.Checkbutton(
            everything_swap_actions,
            text="Shrinkwrap Body",
            variable=self.everything_swap_shrinkwrap_var,
        ).pack(side=tk.LEFT, padx=(14, 0))
        ttk.Label(
            everything_swap_paths,
            textvariable=self.everything_swap_target_info_var,
            wraplength=1040,
        ).grid(row=4, column=0, columnspan=4, sticky=tk.W, pady=(8, 0))
        ttk.Separator(everything_swap_paths, orient=tk.HORIZONTAL).grid(
            row=5, column=0, columnspan=4, sticky=tk.EW, pady=(12, 8)
        )
        ttk.Checkbutton(
            everything_swap_paths,
            text="Include Hair Swap",
            variable=self.everything_swap_hair_enabled_var,
            command=self.update_everything_swap_hair_status,
        ).grid(row=6, column=0, columnspan=4, sticky=tk.W, pady=(0, 4))
        ttk.Label(everything_swap_paths, text="Source Hair").grid(
            row=7, column=0, sticky=tk.W, padx=(0, 8), pady=3
        )
        ttk.Entry(
            everything_swap_paths,
            textvariable=self.everything_swap_hair_source_var,
            state="readonly",
        ).grid(row=7, column=1, sticky=tk.EW, pady=3)
        ttk.Button(
            everything_swap_paths,
            text="Browse",
            command=self.browse_everything_swap_hair_source,
        ).grid(row=7, column=2, padx=(6, 0), pady=3)
        ttk.Button(
            everything_swap_paths,
            text="Detect",
            command=self.detect_everything_swap_hair,
        ).grid(row=7, column=3, padx=(6, 0), pady=3)
        ttk.Label(everything_swap_paths, text="Target Hair Slot").grid(
            row=8, column=0, sticky=tk.W, padx=(0, 8), pady=3
        )
        self.everything_swap_hair_slot_combo = ttk.Combobox(
            everything_swap_paths,
            textvariable=self.everything_swap_hair_slot_var,
            state="readonly",
        )
        self.everything_swap_hair_slot_combo.grid(
            row=8, column=1, columnspan=3, sticky=tk.EW, pady=3
        )
        self.everything_swap_hair_slot_combo.bind(
            "<<ComboboxSelected>>",
            lambda _event: self.update_everything_swap_hair_status(),
        )
        ttk.Label(
            everything_swap_paths,
            textvariable=self.everything_swap_hair_status_var,
            wraplength=1040,
        ).grid(row=9, column=0, columnspan=4, sticky=tk.W, pady=(5, 0))

        appearance = ttk.Frame(self.notebook, padding=6)
        self.notebook.add(appearance, text="Appearance")
        appearance_swap_paths = ttk.Frame(appearance)
        appearance_swap_paths.pack(fill=tk.X, pady=(0, 8))
        appearance_swap_paths.columnconfigure(1, weight=1)
        ttk.Label(appearance_swap_paths, text="Source Character").grid(
            row=0, column=0, sticky=tk.W, padx=(0, 8), pady=3
        )
        ttk.Entry(
            appearance_swap_paths,
            textvariable=self.appearance_swap_source_var,
            state="readonly",
        ).grid(row=0, column=1, sticky="ew", pady=3)
        ttk.Button(
            appearance_swap_paths,
            text="Browse",
            command=self.browse_appearance_swap_source,
        ).grid(row=0, column=2, padx=(8, 0), pady=3)
        ttk.Label(appearance_swap_paths, text="Target Character").grid(
            row=1, column=0, sticky=tk.W, padx=(0, 8), pady=3
        )
        ttk.Entry(
            appearance_swap_paths,
            textvariable=self.appearance_swap_target_var,
            state="readonly",
        ).grid(row=1, column=1, sticky="ew", pady=3)
        ttk.Button(
            appearance_swap_paths,
            text="Browse",
            command=self.browse_appearance_swap_target,
        ).grid(row=1, column=2, padx=(8, 0), pady=3)
        ttk.Label(appearance_swap_paths, text="Recent Output").grid(
            row=1, column=3, sticky=tk.W, padx=(12, 6), pady=3
        )
        self.create_recent_output_combo(appearance_swap_paths).grid(
            row=1, column=4, sticky=tk.EW, pady=3
        )
        appearance_swap_actions = ttk.Frame(appearance_swap_paths)
        appearance_swap_actions.grid(row=2, column=0, columnspan=5, sticky=tk.W, pady=(3, 5))
        ttk.Button(
            appearance_swap_actions,
            text="Swap Appearance + Body Fit",
            command=self.swap_appearance_and_body_fit,
        ).pack(side=tk.LEFT)
        ttk.Button(
            appearance_swap_actions,
            text="Apply and Save",
            command=self.apply_and_save_appearance_only,
        ).pack(side=tk.LEFT, padx=(6, 0))
        appearance_buttons = ttk.Frame(appearance)
        appearance_buttons.pack(fill=tk.X, pady=(0, 6))
        ttk.Button(appearance_buttons, text="Edit Selected Value", command=self.edit_selected_appearance_value).pack(side=tk.LEFT)
        ttk.Button(appearance_buttons, text="Import Body Fit", command=self.import_body_fit).pack(side=tk.LEFT, padx=(6, 0))
        ttk.Button(appearance_buttons, text="Export Body Fit", command=self.export_body_fit).pack(side=tk.LEFT, padx=(6, 0))
        self.appearance_status_var = tk.StringVar(value="Open an .iff with appearance_info.")
        ttk.Label(appearance_buttons, textvariable=self.appearance_status_var).pack(side=tk.LEFT, padx=(12, 0))
        self.appearance_tree = ttk.Treeview(
            appearance,
            columns=("section", "value"),
            show="tree headings",
            selectmode="browse",
        )
        self.appearance_tree.heading("#0", text="Field")
        self.appearance_tree.heading("section", text="Section")
        self.appearance_tree.heading("value", text="Value")
        self.appearance_tree.column("#0", width=300, stretch=True)
        self.appearance_tree.column("section", width=180, stretch=False)
        self.appearance_tree.column("value", width=520, stretch=True)
        appearance_scrollbar = ttk.Scrollbar(appearance, orient=tk.VERTICAL, command=self.appearance_tree.yview)
        self.appearance_tree.configure(yscrollcommand=appearance_scrollbar.set)
        self.appearance_tree.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)
        appearance_scrollbar.pack(side=tk.RIGHT, fill=tk.Y)
        self.appearance_tree.bind("<Double-1>", self.on_appearance_double_click)

        advanced = ttk.Frame(self.notebook, padding=8)
        self.notebook.add(advanced, text="Advanced")
        advanced_paths = ttk.Frame(advanced)
        advanced_paths.pack(fill=tk.X)
        advanced_paths.columnconfigure(1, weight=1)
        ttk.Label(advanced_paths, text="Character IFF").grid(row=0, column=0, sticky=tk.W, padx=(0, 8), pady=3)
        ttk.Entry(
            advanced_paths,
            textvariable=self.advanced_dynamic_body_iff_var,
            state="readonly",
        ).grid(row=0, column=1, sticky=tk.EW, pady=3)
        ttk.Button(
            advanced_paths,
            text="Open IFF",
            command=self.browse_advanced_dynamic_body_iff,
        ).grid(row=0, column=2, padx=(6, 0), pady=3)
        ttk.Label(advanced_paths, text="Recent Output").grid(
            row=0, column=3, sticky=tk.W, padx=(12, 6), pady=3
        )
        self.create_recent_output_combo(advanced_paths).grid(
            row=0, column=4, sticky=tk.EW, pady=3
        )
        advanced_actions = ttk.Frame(advanced_paths)
        advanced_actions.grid(row=1, column=0, columnspan=5, sticky=tk.W, pady=(8, 0))
        self.advanced_dynamic_body_apply_button = ttk.Button(
            advanced_actions,
            text="Apply Dynamic Body",
            command=self.apply_dynamic_body,
            state=tk.DISABLED,
        )
        self.advanced_dynamic_body_apply_button.pack(side=tk.LEFT)
        self.advanced_dynamic_body_save_button = ttk.Button(
            advanced_actions,
            text="Save Dynamic Body IFF",
            command=self.save_as,
            state=tk.DISABLED,
        )
        self.advanced_dynamic_body_save_button.pack(side=tk.LEFT, padx=(6, 0))
        ttk.Label(advanced, textvariable=self.dynamic_body_status_var).pack(fill=tk.X, anchor=tk.W, pady=(12, 6))
        advanced_report_frame = ttk.Frame(advanced)
        advanced_report_frame.pack(fill=tk.BOTH, expand=True)
        self.advanced_dynamic_body_text = tk.Text(advanced_report_frame, wrap=tk.WORD, state=tk.DISABLED)
        advanced_report_scrollbar = ttk.Scrollbar(
            advanced_report_frame,
            orient=tk.VERTICAL,
            command=self.advanced_dynamic_body_text.yview,
        )
        self.advanced_dynamic_body_text.configure(yscrollcommand=advanced_report_scrollbar.set)
        self.advanced_dynamic_body_text.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)
        advanced_report_scrollbar.pack(side=tk.RIGHT, fill=tk.Y)

        validator = ttk.Frame(self.notebook, padding=8)
        self.notebook.add(validator, text="Validator")
        validator_buttons = ttk.Frame(validator)
        validator_buttons.pack(fill=tk.X, pady=(0, 6))
        ttk.Button(validator_buttons, text="Run Validation", command=self.run_validator).pack(side=tk.LEFT)
        ttk.Button(validator_buttons, text="Save Report", command=self.save_validation_report).pack(side=tk.LEFT, padx=(6, 0))
        ttk.Label(validator_buttons, text="Recent Output").pack(side=tk.LEFT, padx=(14, 6))
        self.create_recent_output_combo(validator_buttons, width=30).pack(side=tk.LEFT)
        ttk.Label(validator_buttons, textvariable=self.validator_status_var).pack(side=tk.LEFT, padx=(12, 0))
        validator_frame = ttk.Frame(validator)
        validator_frame.pack(fill=tk.BOTH, expand=True)
        self.validator_tree = ttk.Treeview(
            validator_frame,
            columns=("severity", "details"),
            show="tree headings",
            selectmode="browse",
        )
        self.validator_tree.heading("#0", text="Check")
        self.validator_tree.heading("severity", text="Result")
        self.validator_tree.heading("details", text="Details")
        self.validator_tree.column("#0", width=210, stretch=False)
        self.validator_tree.column("severity", width=95, stretch=False, anchor=tk.CENTER)
        self.validator_tree.column("details", width=760, stretch=True)
        self.validator_tree.tag_configure("ERROR", foreground="#b42318")
        self.validator_tree.tag_configure("WARNING", foreground="#9a6700")
        self.validator_tree.tag_configure("PASS", foreground="#16794b")
        self.validator_tree.tag_configure("INFO", foreground="#4b5563")
        validator_scrollbar = ttk.Scrollbar(validator_frame, orient=tk.VERTICAL, command=self.validator_tree.yview)
        self.validator_tree.configure(yscrollcommand=validator_scrollbar.set)
        self.validator_tree.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)
        validator_scrollbar.pack(side=tk.RIGHT, fill=tk.Y)

        rename_package = ttk.Frame(self.notebook, padding=8)
        self.notebook.add(rename_package, text="Rename Character Package")
        rename_paths = ttk.Frame(rename_package)
        rename_paths.pack(fill=tk.X)
        rename_paths.columnconfigure(1, weight=1)
        ttk.Label(rename_paths, text="Finished Character").grid(row=0, column=0, sticky=tk.W, padx=(0, 8), pady=3)
        ttk.Entry(rename_paths, textvariable=self.rename_package_source_var, state="readonly").grid(
            row=0, column=1, sticky=tk.EW, pady=3
        )
        ttk.Button(rename_paths, text="Browse", command=self.browse_rename_package_source).grid(
            row=0, column=2, padx=(6, 0), pady=3
        )
        ttk.Label(rename_paths, text="Recent Output").grid(
            row=0, column=3, sticky=tk.W, padx=(12, 6), pady=3
        )
        self.create_recent_output_combo(rename_paths).grid(
            row=0, column=4, sticky=tk.EW, pady=3
        )
        ttk.Label(rename_paths, text="New PNG Number").grid(row=1, column=0, sticky=tk.W, padx=(0, 8), pady=3)
        ttk.Entry(rename_paths, textvariable=self.rename_package_new_name_var).grid(
            row=1, column=1, sticky=tk.EW, pady=3
        )
        rename_actions = ttk.Frame(rename_paths)
        rename_actions.grid(row=2, column=0, columnspan=3, sticky=tk.W, pady=(8, 0))
        ttk.Button(rename_actions, text="Create Renamed Package", command=self.rename_character_package).pack(side=tk.LEFT)
        ttk.Checkbutton(
            rename_actions,
            text="Delete Original Package After Rename",
            variable=self.rename_package_delete_original_var,
        ).pack(side=tk.LEFT, padx=(12, 0))
        ttk.Label(rename_package, textvariable=self.rename_package_status_var, wraplength=1050).pack(
            fill=tk.X, anchor=tk.W, pady=(12, 0)
        )

        tattoos = ttk.Frame(self.notebook, padding=6)
        self.notebook.add(tattoos, text="Tattoos")
        tattoo_paths = ttk.Frame(tattoos)
        tattoo_paths.pack(fill=tk.X, pady=(0, 8))
        tattoo_paths.columnconfigure(1, weight=1)
        ttk.Label(tattoo_paths, text="Source Character").grid(row=0, column=0, sticky=tk.W, padx=(0, 8), pady=3)
        ttk.Entry(tattoo_paths, textvariable=self.tattoo_source_var, state="readonly").grid(
            row=0, column=1, sticky="ew", pady=3
        )
        ttk.Button(tattoo_paths, text="Browse", command=self.browse_tattoo_source).grid(
            row=0, column=2, padx=(8, 0), pady=3
        )
        ttk.Label(tattoo_paths, text="Target Character").grid(row=1, column=0, sticky=tk.W, padx=(0, 8), pady=3)
        ttk.Entry(tattoo_paths, textvariable=self.tattoo_target_var, state="readonly").grid(
            row=1, column=1, sticky="ew", pady=3
        )
        ttk.Button(tattoo_paths, text="Browse", command=self.browse_tattoo_target).grid(
            row=1, column=2, padx=(8, 0), pady=3
        )
        ttk.Label(tattoo_paths, text="Recent Output").grid(
            row=1, column=3, sticky=tk.W, padx=(12, 6), pady=3
        )
        self.create_recent_output_combo(tattoo_paths).grid(
            row=1, column=4, sticky=tk.EW, pady=3
        )
        tattoo_swap_actions = ttk.Frame(tattoo_paths)
        tattoo_swap_actions.grid(row=2, column=0, columnspan=5, sticky=tk.W, pady=(3, 5))
        ttk.Button(
            tattoo_swap_actions,
            text="Swap Tattoos",
            command=self.swap_tattoos_from_legacy_iff,
        ).pack(side=tk.LEFT)
        ttk.Button(
            tattoo_swap_actions,
            text="Save Tattoos",
            command=self.save_tattoos_only,
        ).pack(side=tk.LEFT, padx=(6, 0))
        tattoo_buttons = ttk.Frame(tattoos)
        tattoo_buttons.pack(fill=tk.X, pady=(0, 6))
        ttk.Button(tattoo_buttons, text="Open Folder", command=self.open_tattoo_folder).pack(side=tk.LEFT)
        ttk.Button(tattoo_buttons, text="Edit TXTR Text", command=self.edit_selected_tattoo_txtr).pack(side=tk.LEFT, padx=(6, 0))
        ttk.Button(tattoo_buttons, text="Open Texture in Photoshop", command=self.open_selected_tattoo_texture).pack(side=tk.LEFT, padx=(6, 0))
        ttk.Button(tattoo_buttons, text="Replace Texture", command=self.replace_selected_tattoo_texture).pack(side=tk.LEFT, padx=(6, 0))
        ttk.Button(tattoo_buttons, text="Remove Tattoo", command=self.remove_selected_tattoo_pair).pack(side=tk.LEFT, padx=(6, 0))
        self.tattoo_status_var = tk.StringVar(value="Open an .iff to scan for tattoo/body textures.")
        ttk.Label(tattoo_buttons, textvariable=self.tattoo_status_var).pack(side=tk.LEFT, padx=(12, 0))
        self.tattoo_tree = ttk.Treeview(
            tattoos,
            columns=("size", "status"),
            show="tree headings",
            selectmode="browse",
        )
        self.tattoo_tree.heading("#0", text="Texture Name")
        self.tattoo_tree.heading("size", text="Size")
        self.tattoo_tree.heading("status", text="Status")
        self.tattoo_tree.column("#0", width=520, stretch=True)
        self.tattoo_tree.column("size", width=120, stretch=False, anchor=tk.E)
        self.tattoo_tree.column("status", width=120, stretch=False)
        tattoo_scrollbar = ttk.Scrollbar(tattoos, orient=tk.VERTICAL, command=self.tattoo_tree.yview)
        self.tattoo_tree.configure(yscrollcommand=tattoo_scrollbar.set)
        self.tattoo_tree.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)
        tattoo_scrollbar.pack(side=tk.RIGHT, fill=tk.Y)
        self.tattoo_tree.bind("<Double-1>", self.on_tattoo_double_click)

        face = ttk.Frame(self.notebook, padding=6)
        self.notebook.add(face, text="Face")
        face_swap_paths = ttk.Frame(face)
        face_swap_paths.pack(fill=tk.X, pady=(0, 8))
        face_swap_paths.columnconfigure(1, weight=1)
        ttk.Label(face_swap_paths, text="Source Character").grid(row=0, column=0, sticky=tk.W, padx=(0, 8), pady=3)
        ttk.Entry(face_swap_paths, textvariable=self.face_swap_source_var, state="readonly").grid(
            row=0, column=1, sticky="ew", pady=3
        )
        ttk.Button(face_swap_paths, text="Browse", command=self.browse_face_swap_source).grid(
            row=0, column=2, padx=(8, 0), pady=3
        )
        ttk.Label(face_swap_paths, text="Target Character").grid(row=1, column=0, sticky=tk.W, padx=(0, 8), pady=3)
        ttk.Entry(face_swap_paths, textvariable=self.face_swap_target_var, state="readonly").grid(
            row=1, column=1, sticky="ew", pady=3
        )
        ttk.Button(face_swap_paths, text="Browse", command=self.browse_face_swap_target).grid(
            row=1, column=2, padx=(8, 0), pady=3
        )
        ttk.Label(face_swap_paths, text="Recent Output").grid(
            row=1, column=3, sticky=tk.W, padx=(12, 6), pady=3
        )
        self.create_recent_output_combo(face_swap_paths).grid(
            row=1, column=4, sticky=tk.EW, pady=3
        )
        ttk.Label(face_swap_paths, text="Target Config").grid(row=2, column=0, sticky=tk.W, padx=(0, 8), pady=3)
        self.face_swap_target_config_combo = ttk.Combobox(
            face_swap_paths, textvariable=self.face_swap_target_config_var, state="readonly"
        )
        self.face_swap_target_config_combo.grid(row=2, column=1, sticky="ew", pady=3)
        face_config_actions = ttk.Frame(face_swap_paths)
        face_config_actions.grid(row=3, column=0, columnspan=5, sticky=tk.W, pady=(3, 5))
        ttk.Button(
            face_config_actions,
            text="Swap Face Textures",
            command=self.swap_selected_face_config_textures,
        ).pack(side=tk.LEFT)
        ttk.Button(
            face_config_actions,
            text="Save Face Config",
            command=self.save_target_face_config_as,
        ).pack(side=tk.LEFT, padx=(6, 0))
        face_buttons = ttk.Frame(face)
        face_buttons.pack(fill=tk.X, pady=(0, 6))
        ttk.Button(face_buttons, text="Open Folder", command=self.open_face_folder).pack(side=tk.LEFT)
        ttk.Button(face_buttons, text="Edit TXTR Text", command=self.edit_selected_face_txtr).pack(side=tk.LEFT, padx=(6, 0))
        ttk.Button(face_buttons, text="Open Texture in Photoshop", command=self.open_selected_face_texture).pack(side=tk.LEFT, padx=(6, 0))
        ttk.Button(face_buttons, text="Replace Texture", command=self.replace_selected_face_texture).pack(side=tk.LEFT, padx=(6, 0))
        self.face_status_var = tk.StringVar(value="Open a player .iff to find matching face/config files.")
        ttk.Label(face_buttons, textvariable=self.face_status_var).pack(side=tk.LEFT, padx=(12, 0))
        self.face_tree = ttk.Treeview(
            face,
            columns=("type", "size"),
            show="tree headings",
            selectmode="browse",
        )
        self.face_tree.heading("#0", text="Related File / Texture")
        self.face_tree.heading("type", text="Type")
        self.face_tree.heading("size", text="Size")
        self.face_tree.column("#0", width=620, stretch=True)
        self.face_tree.column("type", width=140, stretch=False)
        self.face_tree.column("size", width=120, stretch=False, anchor=tk.E)
        face_scrollbar = ttk.Scrollbar(face, orient=tk.VERTICAL, command=self.face_tree.yview)
        self.face_tree.configure(yscrollcommand=face_scrollbar.set)
        self.face_tree.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)
        face_scrollbar.pack(side=tk.RIGHT, fill=tk.Y)
        self.face_tree.bind("<Double-1>", self.on_face_double_click)

        hair = ttk.Frame(self.notebook, padding=8)
        self.notebook.add(hair, text="Hair")

        hair_converter = ttk.LabelFrame(hair, text="Hair Converter", padding=8)
        hair_converter.pack(fill=tk.X, pady=(0, 8))
        hair_target = ttk.Frame(hair_converter)
        hair_target.pack(fill=tk.X)
        ttk.Label(hair_target, text="Recent Output").pack(side=tk.LEFT)
        self.create_recent_output_combo(hair_target, width=30).pack(
            side=tk.LEFT, padx=(8, 18)
        )
        ttk.Label(hair_target, text="Target Appearance Slot").pack(side=tk.LEFT)
        self.hair_slot_combo = ttk.Combobox(
            hair_target,
            textvariable=self.hair_slot_var,
            state="readonly",
            width=44,
        )
        self.hair_slot_combo.pack(side=tk.LEFT, fill=tk.X, expand=True, padx=(8, 8))
        ttk.Button(
            hair_target,
            text="Convert 2K23 / 2K25 IFF",
            command=self.convert_external_hair,
        ).pack(side=tk.RIGHT)

        hair_picker = ttk.LabelFrame(hair, text="Hair Picker", padding=8)
        hair_picker.pack(fill=tk.BOTH, expand=True)
        hair_controls = ttk.Frame(hair_picker)
        hair_controls.pack(fill=tk.X, pady=(0, 6))
        ttk.Radiobutton(
            hair_controls,
            text="Hair",
            value="hair",
            variable=self.hair_asset_type_var,
            command=self.refresh_hair_catalog,
        ).pack(side=tk.LEFT)
        ttk.Radiobutton(
            hair_controls,
            text="Facial Hair",
            value="facialhair",
            variable=self.hair_asset_type_var,
            command=self.refresh_hair_catalog,
        ).pack(side=tk.LEFT, padx=(8, 18))
        ttk.Label(hair_controls, text="Search").pack(side=tk.LEFT)
        hair_search = ttk.Entry(hair_controls, textvariable=self.hair_search_var)
        hair_search.pack(side=tk.LEFT, fill=tk.X, expand=True, padx=(6, 8))
        hair_search.bind("<KeyRelease>", lambda _event: self.filter_hair_catalog())
        ttk.Button(hair_controls, text="Refresh", command=self.refresh_hair_catalog).pack(side=tk.LEFT)

        hair_actions = ttk.Frame(hair_picker)
        hair_actions.pack(fill=tk.X, pady=(0, 8))
        ttk.Checkbutton(
            hair_actions,
            text="Copy matching item file",
            variable=self.hair_copy_item_var,
        ).pack(side=tk.LEFT)
        self.hair_copy_config_check = ttk.Checkbutton(
            hair_actions,
            text="Copy source config",
            variable=self.hair_copy_config_var,
        )
        self.hair_copy_config_check.pack(side=tk.LEFT, padx=(14, 0))
        ttk.Button(hair_actions, text="Install Selected", command=self.install_selected_hair).pack(
            side=tk.RIGHT
        )

        hair_list_frame = ttk.Frame(hair_picker)
        hair_list_frame.pack(fill=tk.BOTH, expand=True)
        self.hair_tree = ttk.Treeview(
            hair_list_frame,
            columns=("player", "asset", "origin", "vertices", "item"),
            show="tree headings",
            selectmode="browse",
        )
        self.hair_tree.heading("#0", text="PNG")
        self.hair_tree.heading("player", text="Player")
        self.hair_tree.heading("asset", text="Asset")
        self.hair_tree.heading("origin", text="Source")
        self.hair_tree.heading("vertices", text="LOD0 Vertices")
        self.hair_tree.heading("item", text="Item")
        self.hair_tree.column("#0", width=85, stretch=False)
        self.hair_tree.column("player", width=210, stretch=True)
        self.hair_tree.column("asset", width=310, stretch=True)
        self.hair_tree.column("origin", width=100, stretch=False)
        self.hair_tree.column("vertices", width=110, stretch=False, anchor=tk.E)
        self.hair_tree.column("item", width=70, stretch=False, anchor=tk.CENTER)
        hair_list_scrollbar = ttk.Scrollbar(hair_list_frame, orient=tk.VERTICAL, command=self.hair_tree.yview)
        self.hair_tree.configure(yscrollcommand=hair_list_scrollbar.set)
        self.hair_tree.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)
        hair_list_scrollbar.pack(side=tk.RIGHT, fill=tk.Y)
        self.hair_tree.bind("<<TreeviewSelect>>", lambda _event: self.show_selected_hair_details())
        ttk.Label(hair, textvariable=self.hair_selection_var).pack(fill=tk.X, pady=(6, 0))
        ttk.Label(hair, textvariable=self.hair_status_var).pack(fill=tk.X, pady=(8, 0))

        full_swap = ttk.Frame(self.notebook, padding=8)
        self.notebook.add(full_swap, text="Head Swap")

        swap_paths = ttk.Frame(full_swap)
        swap_paths.pack(fill=tk.X)
        swap_paths.columnconfigure(1, weight=1)

        ttk.Label(swap_paths, text="Source Character").grid(row=0, column=0, sticky=tk.W, padx=(0, 8), pady=3)
        ttk.Entry(swap_paths, textvariable=self.full_swap_source_var, state="readonly").grid(
            row=0, column=1, sticky=tk.EW, pady=3
        )
        ttk.Button(swap_paths, text="Browse", command=self.browse_full_swap_source).grid(
            row=0, column=2, padx=(6, 0), pady=3
        )

        ttk.Label(swap_paths, text="Target Character").grid(row=1, column=0, sticky=tk.W, padx=(0, 8), pady=3)
        ttk.Entry(swap_paths, textvariable=self.full_swap_target_var, state="readonly").grid(
            row=1, column=1, sticky=tk.EW, pady=3
        )
        ttk.Button(swap_paths, text="Browse", command=self.browse_full_swap_target).grid(
            row=1, column=2, padx=(6, 0), pady=3
        )
        ttk.Button(swap_paths, text="Use Open IFF", command=self.use_open_iff_as_swap_target).grid(
            row=1, column=3, padx=(6, 0), pady=3
        )
        ttk.Label(swap_paths, text="Recent Output").grid(
            row=1, column=4, sticky=tk.W, padx=(12, 6), pady=3
        )
        self.create_recent_output_combo(swap_paths).grid(
            row=1, column=5, sticky=tk.EW, pady=3
        )

        ttk.Label(swap_paths, text="Blender").grid(row=2, column=0, sticky=tk.W, padx=(0, 8), pady=3)
        ttk.Entry(swap_paths, textvariable=self.full_swap_blender_var, state="readonly").grid(
            row=2, column=1, sticky=tk.EW, pady=3
        )
        ttk.Button(swap_paths, text="Browse", command=self.browse_blender_executable).grid(
            row=2, column=2, padx=(6, 0), pady=3
        )
        ttk.Button(swap_paths, text="Detect", command=self.detect_full_swap_paths).grid(
            row=2, column=3, padx=(6, 0), pady=3
        )

        ttk.Label(swap_paths, text="Blender Swap Tool").grid(row=3, column=0, sticky=tk.W, padx=(0, 8), pady=3)
        ttk.Entry(swap_paths, textvariable=self.full_swap_tool_var, state="readonly").grid(
            row=3, column=1, sticky=tk.EW, pady=3
        )
        ttk.Button(swap_paths, text="Browse", command=self.browse_head_swap_tool).grid(
            row=3, column=2, padx=(6, 0), pady=3
        )
        ttk.Button(swap_paths, text="Detect", command=self.detect_full_swap_paths).grid(
            row=3, column=3, padx=(6, 0), pady=3
        )

        swap_buttons = ttk.Frame(full_swap)
        swap_buttons.pack(fill=tk.X, pady=(8, 6))
        self.full_swap_run_button = ttk.Button(
            swap_buttons,
            text="Run Head Swap",
            command=self.run_full_swap,
            state=tk.DISABLED,
        )
        self.full_swap_run_button.pack(side=tk.LEFT)

        swap_secondary_buttons = ttk.Frame(full_swap)
        swap_secondary_buttons.pack(fill=tk.X, pady=(0, 6))
        ttk.Button(
            swap_secondary_buttons,
            text="Check Swap Setup",
            command=self.check_full_swap_setup,
        ).pack(side=tk.LEFT)
        ttk.Button(
            swap_secondary_buttons,
            text="Test Background Link",
            command=self.test_full_swap_bridge,
        ).pack(side=tk.LEFT, padx=(6, 0))
        ttk.Button(
            swap_secondary_buttons,
            text="Open Swap Tool Folder",
            command=self.open_head_swap_tool_folder,
        ).pack(side=tk.LEFT, padx=(6, 0))

        ttk.Label(full_swap, textvariable=self.full_swap_status_var).pack(fill=tk.X, pady=(0, 6))

        swap_status_frame = ttk.Frame(full_swap)
        swap_status_frame.pack(fill=tk.BOTH, expand=True)
        self.full_swap_tree = ttk.Treeview(
            swap_status_frame,
            columns=("status", "details"),
            show="tree headings",
            selectmode="none",
        )
        self.full_swap_tree.heading("#0", text="Stage")
        self.full_swap_tree.heading("status", text="Status")
        self.full_swap_tree.heading("details", text="Details")
        self.full_swap_tree.column("#0", width=180, stretch=False)
        self.full_swap_tree.column("status", width=120, stretch=False)
        self.full_swap_tree.column("details", width=560, stretch=True)
        full_swap_scrollbar = ttk.Scrollbar(
            swap_status_frame,
            orient=tk.VERTICAL,
            command=self.full_swap_tree.yview,
        )
        self.full_swap_tree.configure(yscrollcommand=full_swap_scrollbar.set)
        self.full_swap_tree.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)
        full_swap_scrollbar.pack(side=tk.RIGHT, fill=tk.Y)

        body_swap = ttk.Frame(self.notebook, padding=8)
        self.notebook.add(body_swap, text="Body Swap")

        body_swap_paths = ttk.Frame(body_swap)
        body_swap_paths.pack(fill=tk.X)
        body_swap_paths.columnconfigure(1, weight=1)

        ttk.Label(body_swap_paths, text="Source Character").grid(
            row=0, column=0, sticky=tk.W, padx=(0, 8), pady=3
        )
        ttk.Entry(
            body_swap_paths,
            textvariable=self.body_swap_source_var,
            state="readonly",
        ).grid(row=0, column=1, sticky=tk.EW, pady=3)
        ttk.Button(
            body_swap_paths,
            text="Browse",
            command=self.browse_body_swap_source,
        ).grid(row=0, column=2, padx=(6, 0), pady=3)

        ttk.Label(body_swap_paths, text="Target Character").grid(
            row=1, column=0, sticky=tk.W, padx=(0, 8), pady=3
        )
        ttk.Entry(
            body_swap_paths,
            textvariable=self.body_swap_target_var,
            state="readonly",
        ).grid(row=1, column=1, sticky=tk.EW, pady=3)
        ttk.Button(
            body_swap_paths,
            text="Browse",
            command=self.browse_body_swap_target,
        ).grid(row=1, column=2, padx=(6, 0), pady=3)
        ttk.Button(
            body_swap_paths,
            text="Use Open IFF",
            command=self.use_open_iff_as_body_swap_target,
        ).grid(row=1, column=3, padx=(6, 0), pady=3)
        ttk.Label(body_swap_paths, text="Recent Output").grid(
            row=1, column=4, sticky=tk.W, padx=(12, 6), pady=3
        )
        self.create_recent_output_combo(body_swap_paths).grid(
            row=1, column=5, sticky=tk.EW, pady=3
        )

        body_swap_buttons = ttk.Frame(body_swap)
        body_swap_buttons.pack(fill=tk.X, pady=(8, 6))
        self.body_swap_run_button = ttk.Button(
            body_swap_buttons,
            text="Run Body Swap",
            command=self.run_body_swap,
            state=tk.DISABLED,
        )
        self.body_swap_run_button.pack(side=tk.LEFT)

        body_swap_secondary = ttk.Frame(body_swap)
        body_swap_secondary.pack(fill=tk.X, pady=(0, 6))
        ttk.Button(
            body_swap_secondary,
            text="Check Swap Setup",
            command=self.check_body_swap_setup,
        ).pack(side=tk.LEFT)
        ttk.Button(
            body_swap_secondary,
            text="Test Background Link",
            command=self.test_body_swap_bridge,
        ).pack(side=tk.LEFT, padx=(6, 0))
        ttk.Button(
            body_swap_secondary,
            text="Open Swap Tool Folder",
            command=self.open_head_swap_tool_folder,
        ).pack(side=tk.LEFT, padx=(6, 0))

        ttk.Label(body_swap, textvariable=self.body_swap_status_var).pack(fill=tk.X, pady=(0, 6))

        body_swap_status_frame = ttk.Frame(body_swap)
        body_swap_status_frame.pack(fill=tk.BOTH, expand=True)
        self.body_swap_tree = ttk.Treeview(
            body_swap_status_frame,
            columns=("status", "details"),
            show="tree headings",
            selectmode="none",
        )
        self.body_swap_tree.heading("#0", text="Stage")
        self.body_swap_tree.heading("status", text="Status")
        self.body_swap_tree.heading("details", text="Details")
        self.body_swap_tree.column("#0", width=180, stretch=False)
        self.body_swap_tree.column("status", width=120, stretch=False)
        self.body_swap_tree.column("details", width=560, stretch=True)
        body_swap_scrollbar = ttk.Scrollbar(
            body_swap_status_frame,
            orient=tk.VERTICAL,
            command=self.body_swap_tree.yview,
        )
        self.body_swap_tree.configure(yscrollcommand=body_swap_scrollbar.set)
        self.body_swap_tree.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)
        body_swap_scrollbar.pack(side=tk.RIGHT, fill=tk.Y)

        headband_swap = ttk.Frame(self.notebook, padding=8)
        self.notebook.add(headband_swap, text="Headband Swap")

        headband_paths = ttk.Frame(headband_swap)
        headband_paths.pack(fill=tk.X)
        headband_paths.columnconfigure(1, weight=1)

        ttk.Label(headband_paths, text="Source Headband (Legacy / 2K25)").grid(
            row=0, column=0, sticky=tk.W, padx=(0, 8), pady=3
        )
        ttk.Entry(headband_paths, textvariable=self.headband_source_var, state="readonly").grid(
            row=0, column=1, sticky=tk.EW, pady=3
        )
        ttk.Button(headband_paths, text="Browse", command=self.browse_headband_source).grid(
            row=0, column=2, padx=(6, 0), pady=3
        )

        ttk.Label(headband_paths, text="NBA 2K26 Target Headband").grid(
            row=1, column=0, sticky=tk.W, padx=(0, 8), pady=3
        )
        ttk.Entry(headband_paths, textvariable=self.headband_target_var, state="readonly").grid(
            row=1, column=1, sticky=tk.EW, pady=3
        )
        ttk.Button(headband_paths, text="Browse", command=self.browse_headband_target).grid(
            row=1, column=2, padx=(6, 0), pady=3
        )
        ttk.Label(headband_paths, text="Recent Output").grid(
            row=1, column=3, sticky=tk.W, padx=(12, 6), pady=3
        )
        self.create_recent_output_combo(headband_paths).grid(
            row=1, column=4, sticky=tk.EW, pady=3
        )

        ttk.Label(headband_paths, text="Blender").grid(row=2, column=0, sticky=tk.W, padx=(0, 8), pady=3)
        ttk.Entry(headband_paths, textvariable=self.full_swap_blender_var, state="readonly").grid(
            row=2, column=1, sticky=tk.EW, pady=3
        )
        ttk.Button(headband_paths, text="Detect", command=self.detect_headband_paths).grid(
            row=2, column=2, padx=(6, 0), pady=3
        )

        ttk.Label(headband_paths, text="Blender Headband Tool").grid(
            row=3, column=0, sticky=tk.W, padx=(0, 8), pady=3
        )
        ttk.Entry(headband_paths, textvariable=self.headband_tool_var, state="readonly").grid(
            row=3, column=1, sticky=tk.EW, pady=3
        )
        ttk.Button(headband_paths, text="Detect", command=self.detect_headband_paths).grid(
            row=3, column=2, padx=(6, 0), pady=3
        )
        ttk.Button(headband_paths, text="Browse", command=self.browse_headband_tool).grid(
            row=3, column=3, padx=(6, 0), pady=3
        )

        headband_buttons = ttk.Frame(headband_swap)
        headband_buttons.pack(fill=tk.X, pady=(8, 6))
        self.headband_run_button = ttk.Button(
            headband_buttons,
            text="Run Headband Swap",
            command=self.run_headband_swap,
            state=tk.DISABLED,
        )
        self.headband_run_button.pack(side=tk.LEFT)

        headband_secondary_buttons = ttk.Frame(headband_swap)
        headband_secondary_buttons.pack(fill=tk.X, pady=(0, 6))
        ttk.Button(
            headband_secondary_buttons,
            text="Check Headband Setup",
            command=self.check_headband_setup,
        ).pack(side=tk.LEFT)
        ttk.Button(
            headband_secondary_buttons,
            text="Test Background Link",
            command=self.test_headband_bridge,
        ).pack(side=tk.LEFT, padx=(6, 0))

        ttk.Label(headband_swap, textvariable=self.headband_status_var).pack(fill=tk.X, pady=(0, 6))
        headband_status_frame = ttk.Frame(headband_swap)
        headband_status_frame.pack(fill=tk.BOTH, expand=True)
        self.headband_tree = ttk.Treeview(
            headband_status_frame,
            columns=("status", "details"),
            show="tree headings",
            selectmode="none",
        )
        self.headband_tree.heading("#0", text="Stage")
        self.headband_tree.heading("status", text="Status")
        self.headband_tree.heading("details", text="Details")
        self.headband_tree.column("#0", width=190, stretch=False)
        self.headband_tree.column("status", width=120, stretch=False)
        self.headband_tree.column("details", width=550, stretch=True)
        headband_scrollbar = ttk.Scrollbar(
            headband_status_frame,
            orient=tk.VERTICAL,
            command=self.headband_tree.yview,
        )
        self.headband_tree.configure(yscrollcommand=headband_scrollbar.set)
        self.headband_tree.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)
        headband_scrollbar.pack(side=tk.RIGHT, fill=tk.Y)

        accessories = ttk.Frame(self.notebook, padding=8)
        self.notebook.add(accessories, text="Glasses (Experimental)")
        accessory_paths = ttk.Frame(accessories)
        accessory_paths.pack(fill=tk.X)
        accessory_paths.columnconfigure(1, weight=1)

        ttk.Label(accessory_paths, text="Glasses Source IFF").grid(
            row=0, column=0, sticky=tk.W, padx=(0, 8), pady=3
        )
        ttk.Entry(accessory_paths, textvariable=self.accessory_source_var, state="readonly").grid(
            row=0, column=1, sticky=tk.EW, pady=3
        )
        ttk.Button(accessory_paths, text="Choose IFF", command=self.browse_accessory_source).grid(
            row=0, column=2, padx=(6, 0), pady=3
        )

        ttk.Label(accessory_paths, text="Open Player IFF").grid(
            row=1, column=0, sticky=tk.W, padx=(0, 8), pady=3
        )
        ttk.Entry(accessory_paths, textvariable=self.accessory_target_var, state="readonly").grid(
            row=1, column=1, sticky=tk.EW, pady=3
        )
        ttk.Button(accessory_paths, text="Use Open IFF", command=self.use_open_iff_as_accessory_target).grid(
            row=1, column=2, padx=(6, 0), pady=3
        )
        ttk.Button(accessory_paths, text="Browse", command=self.browse_accessory_target).grid(
            row=1, column=3, padx=(6, 0), pady=3
        )
        ttk.Label(accessory_paths, text="Recent Output").grid(
            row=1, column=4, sticky=tk.W, padx=(12, 6), pady=3
        )
        self.create_recent_output_combo(accessory_paths).grid(
            row=1, column=5, sticky=tk.EW, pady=3
        )

        accessory_buttons = ttk.Frame(accessories)
        accessory_buttons.pack(fill=tk.X, pady=(8, 6))
        self.builtin_glasses_combo = ttk.Combobox(
            accessory_buttons,
            textvariable=self.builtin_glasses_var,
            values=tuple(AVAILABLE_BUILT_IN_GLASSES),
            state="readonly",
            width=24,
        )
        self.builtin_glasses_combo.pack(side=tk.LEFT)
        self.builtin_glasses_load_button = ttk.Button(
            accessory_buttons,
            text="Load Selected",
            command=self.load_selected_builtin_glasses,
            state=tk.NORMAL if AVAILABLE_BUILT_IN_GLASSES else tk.DISABLED,
        )
        self.builtin_glasses_load_button.pack(side=tk.LEFT, padx=(6, 0))
        ttk.Button(accessory_buttons, text="Load Glasses IFF", command=self.scan_custom_accessories).pack(
            side=tk.LEFT, padx=(6, 0)
        )
        ttk.Button(
            accessory_buttons,
            text="Select All Configurations",
            command=self.select_all_accessory_configurations,
        ).pack(side=tk.LEFT, padx=(6, 0))
        ttk.Button(accessory_buttons, text="Add Selected Accessory", command=self.add_selected_accessory).pack(
            side=tk.LEFT, padx=(6, 0)
        )
        ttk.Button(
            accessory_buttons,
            text="Bake Into Hihead (Experimental)",
            command=self.bake_selected_accessory_into_hihead,
        ).pack(side=tk.LEFT, padx=(6, 0))

        accessory_style = ttk.Frame(accessories)
        accessory_style.pack(fill=tk.X, pady=(0, 6))
        ttk.Label(accessory_style, text="Frame Color").pack(side=tk.LEFT)
        self.accessory_frame_color_swatch = tk.Canvas(
            accessory_style,
            width=32,
            height=20,
            highlightthickness=1,
            highlightbackground="#777777",
            background=self.accessory_frame_color_var.get(),
        )
        self.accessory_frame_color_swatch.pack(side=tk.LEFT, padx=(6, 4))
        self.accessory_frame_color_swatch.bind("<Button-1>", lambda _event: self.choose_accessory_frame_color())
        ttk.Button(accessory_style, text="Choose Color", command=self.choose_accessory_frame_color).pack(side=tk.LEFT)
        ttk.Button(accessory_style, text="Reset", command=self.reset_accessory_frame_color).pack(
            side=tk.LEFT, padx=(6, 0)
        )
        ttk.Label(accessories, textvariable=self.accessory_status_var).pack(fill=tk.X, pady=(0, 6))

        accessory_split = ttk.Panedwindow(accessories, orient=tk.HORIZONTAL)
        accessory_split.pack(fill=tk.BOTH, expand=True)

        accessory_list_frame = ttk.Frame(accessory_split, padding=(0, 0, 6, 0))
        accessory_split.add(accessory_list_frame, weight=1)
        ttk.Label(accessory_list_frame, text="Source Glasses").pack(anchor=tk.W, pady=(0, 4))
        accessory_tree_frame = ttk.Frame(accessory_list_frame)
        accessory_tree_frame.pack(fill=tk.BOTH, expand=True)
        self.accessory_tree = ttk.Treeview(
            accessory_tree_frame,
            columns=("type", "geometry", "textures"),
            show="tree headings",
            selectmode="browse",
        )
        self.accessory_tree.heading("#0", text="Accessory")
        self.accessory_tree.heading("type", text="Type")
        self.accessory_tree.heading("geometry", text="Geometry")
        self.accessory_tree.heading("textures", text="Textures")
        self.accessory_tree.column("#0", width=230, stretch=True)
        self.accessory_tree.column("type", width=100, stretch=False)
        self.accessory_tree.column("geometry", width=80, stretch=False)
        self.accessory_tree.column("textures", width=80, stretch=False)
        accessory_list_scrollbar = ttk.Scrollbar(
            accessory_tree_frame,
            orient=tk.VERTICAL,
            command=self.accessory_tree.yview,
        )
        self.accessory_tree.configure(yscrollcommand=accessory_list_scrollbar.set)
        self.accessory_tree.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)
        accessory_list_scrollbar.pack(side=tk.RIGHT, fill=tk.Y)

        config_list_frame = ttk.Frame(accessory_split, padding=(6, 0, 0, 0))
        accessory_split.add(config_list_frame, weight=1)
        ttk.Label(config_list_frame, text="Target Configurations").pack(anchor=tk.W, pady=(0, 4))
        config_tree_frame = ttk.Frame(config_list_frame)
        config_tree_frame.pack(fill=tk.BOTH, expand=True)
        self.accessory_config_tree = ttk.Treeview(
            config_tree_frame,
            columns=("items", "default"),
            show="tree headings",
            selectmode="extended",
        )
        self.accessory_config_tree.heading("#0", text="Configuration")
        self.accessory_config_tree.heading("items", text="Items")
        self.accessory_config_tree.heading("default", text="Default")
        self.accessory_config_tree.column("#0", width=280, stretch=True)
        self.accessory_config_tree.column("items", width=70, stretch=False, anchor=tk.E)
        self.accessory_config_tree.column("default", width=90, stretch=False)
        config_scrollbar = ttk.Scrollbar(
            config_tree_frame,
            orient=tk.VERTICAL,
            command=self.accessory_config_tree.yview,
        )
        self.accessory_config_tree.configure(yscrollcommand=config_scrollbar.set)
        self.accessory_config_tree.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)
        config_scrollbar.pack(side=tk.RIGHT, fill=tk.Y)

        live_roster = ttk.Frame(self.notebook, padding=8)
        self.notebook.add(live_roster, text="Live Roster")
        live_roster_actions = ttk.Frame(live_roster)
        live_roster_actions.pack(fill=tk.X, pady=(0, 6))
        self.live_roster_read_button = ttk.Button(
            live_roster_actions,
            text="Read Highlighted Player",
            command=self.read_live_roster_player,
        )
        self.live_roster_read_button.pack(side=tk.LEFT)
        self.live_roster_admin_button = ttk.Button(
            live_roster_actions,
            text="Restart as Administrator",
            command=self.restart_as_administrator,
        )
        self.live_roster_admin_button.pack(side=tk.LEFT, padx=(6, 0))
        ttk.Label(live_roster_actions, textvariable=self.live_roster_status_var).pack(
            side=tk.LEFT, padx=(12, 0)
        )

        live_roster_editor = ttk.Frame(live_roster)
        live_roster_editor.pack(fill=tk.X, pady=(0, 8))
        ttk.Label(live_roster_editor, text="Field").pack(side=tk.LEFT)
        self.live_roster_field_combo = ttk.Combobox(
            live_roster_editor,
            textvariable=self.live_roster_field_var,
            values=ROSTER_WRITABLE_FIELDS,
            state="readonly",
            width=18,
        )
        self.live_roster_field_combo.pack(side=tk.LEFT, padx=(6, 12))
        ttk.Label(live_roster_editor, text="New Value").pack(side=tk.LEFT)
        self.live_roster_value_entry = ttk.Entry(
            live_roster_editor,
            textvariable=self.live_roster_value_var,
            width=30,
        )
        self.live_roster_value_entry.pack(side=tk.LEFT, padx=(6, 6))
        self.live_roster_apply_button = ttk.Button(
            live_roster_editor,
            text="Apply to Highlighted Player",
            command=self.apply_live_roster_value,
        )
        self.live_roster_apply_button.pack(side=tk.LEFT)

        live_roster_panes = ttk.Panedwindow(live_roster, orient=tk.HORIZONTAL)
        live_roster_panes.pack(fill=tk.BOTH, expand=True)
        live_roster_table_frame = ttk.Frame(live_roster_panes)
        live_roster_output_frame = ttk.Frame(live_roster_panes)
        live_roster_panes.add(live_roster_table_frame, weight=3)
        live_roster_panes.add(live_roster_output_frame, weight=2)
        self.live_roster_tree = ttk.Treeview(
            live_roster_table_frame,
            columns=("value",),
            show="tree headings",
            selectmode="browse",
        )
        self.live_roster_tree.heading("#0", text="Field")
        self.live_roster_tree.heading("value", text="Current Value")
        self.live_roster_tree.column("#0", width=210, stretch=False)
        self.live_roster_tree.column("value", width=300, stretch=True)
        live_roster_scrollbar = ttk.Scrollbar(
            live_roster_table_frame,
            orient=tk.VERTICAL,
            command=self.live_roster_tree.yview,
        )
        self.live_roster_tree.configure(yscrollcommand=live_roster_scrollbar.set)
        self.live_roster_tree.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)
        live_roster_scrollbar.pack(side=tk.RIGHT, fill=tk.Y)
        self.live_roster_tree.bind("<<TreeviewSelect>>", self.on_live_roster_field_selected)
        self.live_roster_output_text = tk.Text(live_roster_output_frame, wrap=tk.WORD, state=tk.DISABLED)
        live_roster_output_scrollbar = ttk.Scrollbar(
            live_roster_output_frame,
            orient=tk.VERTICAL,
            command=self.live_roster_output_text.yview,
        )
        self.live_roster_output_text.configure(yscrollcommand=live_roster_output_scrollbar.set)
        self.live_roster_output_text.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)
        live_roster_output_scrollbar.pack(side=tk.RIGHT, fill=tk.Y)

        workflow_tab_order = (
            everything_swap,
            full_swap,
            body_swap,
            face,
            tattoos,
            appearance,
            hair,
            headband_swap,
            validator,
            rename_package,
            live_roster,
            advanced,
            accessories,
        )
        for index, tab in enumerate(workflow_tab_order):
            self.notebook.insert(index, tab)

        structured = ttk.Frame(self.notebook, padding=6)
        button_row = ttk.Frame(structured)
        button_row.pack(fill=tk.X, pady=(0, 6))
        ttk.Button(button_row, text="Edit Selected Value", command=self.edit_selected_json_value).pack(side=tk.LEFT)
        ttk.Button(button_row, text="Apply GUI Changes", command=self.apply_json_change).pack(side=tk.LEFT, padx=(6, 0))
        self.json_status_var = tk.StringVar(value="Select a JSON or SCNE entry.")
        ttk.Label(button_row, textvariable=self.json_status_var).pack(side=tk.LEFT, padx=(12, 0))
        self.json_tree = ttk.Treeview(structured, columns=("value",), show="tree headings")
        self.json_tree.heading("#0", text="Field")
        self.json_tree.heading("value", text="Value")
        self.json_tree.column("#0", width=360, stretch=True)
        self.json_tree.column("value", width=520, stretch=True)
        self.json_tree.pack(fill=tk.BOTH, expand=True)

        raw = ttk.Frame(self.notebook, padding=6)
        raw_buttons = ttk.Frame(raw)
        raw_buttons.pack(fill=tk.X, pady=(0, 6))
        ttk.Button(raw_buttons, text="Apply Text Change", command=self.apply_text_change).pack(side=tk.LEFT)
        self.raw_status_var = tk.StringVar(value="Select a text entry.")
        ttk.Label(raw_buttons, textvariable=self.raw_status_var).pack(side=tk.LEFT, padx=(12, 0))
        self.raw_text = tk.Text(raw, wrap=tk.NONE, undo=True)
        self.raw_text.pack(fill=tk.BOTH, expand=True)

        self.hex_text = tk.Text(self, wrap=tk.WORD)
        self.hex_text.configure(state=tk.DISABLED)

        bottom = ttk.Frame(self, padding=(8, 0, 8, 8))
        bottom.pack(fill=tk.X)
        ttk.Label(bottom, text="Tip: save to a new .iff first, then test that copy in the game.").pack(anchor=tk.W)
        self.after(500, self.cleanup_stale_output_staging)
        self.after(150, self.refresh_recent_output_choices)

    def _add_text_tab(self, title):
        frame = ttk.Frame(self.notebook, padding=6)
        self.notebook.add(frame, text=title)
        text = tk.Text(frame, wrap=tk.WORD)
        text.pack(fill=tk.BOTH, expand=True)
        text.configure(state=tk.DISABLED)
        return text

    def configure_hair_backend(self):
        if self.hair_backend is None:
            return
        configure = getattr(self.hair_backend, "configure_environment", None)
        if configure:
            configure(
                game_root=self.settings.get("game_root", ""),
                blender_exe=self.settings.get("blender_exe", ""),
                log_dir=str(app_settings.LOG_DIR),
            )

    def apply_runtime_settings(self, values, persist=True):
        self.settings = app_settings.save_settings(values) if persist else dict(values)
        self.settings["game_root"] = app_settings.discover_game_root(self.settings.get("game_root", ""))
        self.settings["blender_exe"] = app_settings.discover_blender(self.settings.get("blender_exe", ""))
        self.settings["photoshop_exe"] = app_settings.discover_photoshop(self.settings.get("photoshop_exe", ""))
        self.settings["head_swap_tool"] = app_settings.discover_head_swap_tool(self.settings.get("head_swap_tool", ""))
        self.settings["mesh_data_transfer"] = app_settings.discover_mesh_data_transfer(
            self.settings.get("mesh_data_transfer", "")
        )
        self.settings["output_dir"] = app_settings.ensure_output_dir(self.settings.get("output_dir", ""))
        self.texture_export_dir = os.path.join(self.settings["output_dir"], "texture_exports")
        if persist:
            app_settings.save_settings(self.settings)
        self.full_swap_blender_var.set(self.settings.get("blender_exe", ""))
        self.full_swap_tool_var.set(self.settings.get("head_swap_tool", ""))
        self.headband_tool_var.set(self.find_headband_swap_tool())
        self.full_swap_bridge_result = "Not tested"
        self.headband_bridge_result = "Not tested"
        self.configure_hair_backend()
        self.refresh_full_swap_status()
        self.refresh_headband_status()
        self.refresh_recent_output_choices()
        if self.settings.get("game_root") and os.path.isfile(self.settings.get("blender_exe", "")):
            self.after(50, self.refresh_hair_catalog)

    def open_settings_dialog(self):
        window = tk.Toplevel(self)
        window.title("Character Mod Tool Settings")
        window.transient(self)
        window.grab_set()
        window.resizable(True, False)
        body = ttk.Frame(window, padding=12)
        body.pack(fill=tk.BOTH, expand=True)
        body.columnconfigure(1, weight=1)
        fields = (
            ("NBA 2K26 Folder", "game_root", "directory"),
            ("Blender", "blender_exe", "file"),
            ("Photoshop (Optional)", "photoshop_exe", "file"),
            ("Head Swap Tool", "head_swap_tool", "directory"),
            ("Mesh Data Transfer", "mesh_data_transfer", "directory"),
            ("Default Output Folder", "output_dir", "directory"),
        )
        variables = {key: tk.StringVar(value=self.settings.get(key, "")) for _label, key, _kind in fields}

        def browse(key, kind):
            current = variables[key].get().strip()
            if kind == "directory":
                path = filedialog.askdirectory(parent=window, initialdir=current if os.path.isdir(current) else "")
            else:
                path = filedialog.askopenfilename(parent=window, initialdir=os.path.dirname(current) if current else "")
            if path:
                variables[key].set(path)

        for row, (label, key, kind) in enumerate(fields):
            ttk.Label(body, text=label).grid(row=row, column=0, sticky=tk.W, padx=(0, 8), pady=4)
            ttk.Entry(body, textvariable=variables[key], width=72).grid(row=row, column=1, sticky=tk.EW, pady=4)
            ttk.Button(body, text="Browse", command=lambda k=key, t=kind: browse(k, t)).grid(
                row=row, column=2, padx=(6, 0), pady=4
            )

        status = tk.StringVar(value="Paths are stored per Windows user, outside the app folder.")
        ttk.Label(body, textvariable=status).grid(row=len(fields), column=0, columnspan=3, sticky=tk.W, pady=(8, 4))

        def auto_detect():
            variables["game_root"].set(app_settings.discover_game_root(variables["game_root"].get()))
            variables["blender_exe"].set(app_settings.discover_blender(variables["blender_exe"].get()))
            variables["photoshop_exe"].set(app_settings.discover_photoshop(variables["photoshop_exe"].get()))
            variables["head_swap_tool"].set(app_settings.discover_head_swap_tool(variables["head_swap_tool"].get()))
            variables["mesh_data_transfer"].set(
                app_settings.discover_mesh_data_transfer(variables["mesh_data_transfer"].get())
            )
            status.set("Auto-detection complete. Review the paths, then save.")

        def save():
            values = {key: variable.get().strip() for key, variable in variables.items()}
            self.apply_runtime_settings(values)
            LOGGER.info("Settings updated")
            window.destroy()

        actions = ttk.Frame(body)
        actions.grid(row=len(fields) + 1, column=0, columnspan=3, sticky=tk.E, pady=(8, 0))
        ttk.Button(actions, text="Auto Detect", command=auto_detect).pack(side=tk.LEFT)
        ttk.Button(actions, text="Cancel", command=window.destroy).pack(side=tk.LEFT, padx=(6, 0))
        ttk.Button(actions, text="Save", command=save).pack(side=tk.LEFT, padx=(6, 0))

    def create_diagnostics_bundle(self):
        app_settings.DIAGNOSTICS_DIR.mkdir(parents=True, exist_ok=True)
        stamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        output = app_settings.DIAGNOSTICS_DIR / f"CharacterModTool_Diagnostics_{stamp}.zip"
        system_info = {
            "app_version": APP_VERSION,
            "python": sys.version,
            "platform": platform.platform(),
            "frozen_exe": bool(getattr(sys, "frozen", False)),
            "settings_path": str(app_settings.SETTINGS_PATH),
            "resource_root": app_settings.resource_path(),
            "path_checks": {
                "game_root": os.path.isdir(self.settings.get("game_root", "")),
                "blender": os.path.isfile(self.settings.get("blender_exe", "")),
                "photoshop": os.path.isfile(self.settings.get("photoshop_exe", "")),
                "head_swap_tool": os.path.isfile(os.path.join(self.find_head_swap_tool(), "__init__.py")),
                "mesh_data_transfer": os.path.isfile(os.path.join(self.find_mesh_data_transfer_tool(), "__init__.py")),
            },
        }
        with zipfile.ZipFile(output, "w", zipfile.ZIP_DEFLATED) as archive:
            if LOG_PATH.is_file():
                archive.write(LOG_PATH, "logs/character_mod_tool.log")
            archive.writestr("system_info.json", json.dumps(system_info, indent=2))
            archive.writestr("settings.json", json.dumps(self.settings, indent=2))
            if self.last_validation_report:
                archive.writestr("last_validation.json", json.dumps(self.last_validation_report, indent=2))
        return str(output)

    def open_diagnostics_dialog(self):
        try:
            path = self.create_diagnostics_bundle()
        except Exception as exc:
            LOGGER.exception("Could not create diagnostics bundle")
            messagebox.showerror("Character Mod Tool", f"Could not create diagnostics:\n{exc}")
            return
        if messagebox.askyesno(
            "Character Mod Tool Diagnostics",
            f"Created a support bundle containing logs, path settings, and validation results.\n\n{path}\n\nOpen its folder?",
        ):
            os.startfile(os.path.dirname(path))

    def open_outputs_folder(self):
        output_dir = app_settings.ensure_output_dir(self.settings.get("output_dir", ""))
        if not output_dir:
            messagebox.showerror("Character Mod Tool", "The output folder could not be created.")
            return
        self.settings["output_dir"] = output_dir
        app_settings.save_settings(self.settings)
        try:
            os.startfile(output_dir)
        except OSError as exc:
            messagebox.showerror("Character Mod Tool", f"Could not open the output folder:\n{exc}")

    def find_blender_executable(self):
        return app_settings.discover_blender(self.settings.get("blender_exe", ""))

    def find_head_swap_tool(self):
        return app_settings.discover_head_swap_tool(self.settings.get("head_swap_tool", ""))

    def find_headband_swap_tool(self):
        candidates = [
            BUNDLED_HEADBAND_SWAP_TOOL,
            self.settings.get("head_swap_tool", ""),
            self.find_head_swap_tool(),
        ]
        valid = []
        for path in candidates:
            normalized = os.path.normpath(path)
            tool_file = os.path.join(normalized, "nba2k_character_eyeball_tool.py")
            if normalized in valid or not os.path.isfile(os.path.join(normalized, "__init__.py")):
                continue
            try:
                with open(tool_file, "r", encoding="utf-8-sig") as handle:
                    supports_headband = "def transfer_headband_shape" in handle.read()
            except OSError:
                supports_headband = False
            if supports_headband:
                valid.append(normalized)
        if not valid:
            return ""

        def version_key(path):
            version = self.read_head_swap_tool_version(path)
            try:
                return tuple(int(part) for part in version.split("."))
            except ValueError:
                return (0, 0, 0)

        return max(valid, key=version_key)

    def find_mesh_data_transfer_tool(self):
        return app_settings.discover_mesh_data_transfer(self.settings.get("mesh_data_transfer", ""))

    @staticmethod
    def read_head_swap_tool_version(path):
        init_path = os.path.join(path, "__init__.py")
        if not os.path.isfile(init_path):
            return ""
        try:
            with open(init_path, "r", encoding="utf-8-sig") as handle:
                text = handle.read()
        except OSError:
            return ""
        match = re.search(r'["\']version["\']\s*:\s*\((\d+)\s*,\s*(\d+)\s*,\s*(\d+)\)', text)
        if not match:
            return ""
        return ".".join(match.groups())

    @staticmethod
    def inspect_full_swap_iff(path):
        if not path:
            return False, "Choose file", "No character IFF selected"
        if not os.path.isfile(path):
            return False, "Missing", "File was not found"
        if not zipfile.is_zipfile(path):
            return False, "Unsupported", "Not a ZIP-style character IFF"
        try:
            with zipfile.ZipFile(path, "r") as archive:
                names = [info.filename for info in archive.infolist() if not info.is_dir()]
        except (OSError, zipfile.BadZipFile) as exc:
            return False, "Could not read", str(exc)

        base_names = [os.path.basename(name).lower() for name in names]
        vertex_buffers = sum(name.startswith("vertexbuffer") for name in base_names)
        index_buffers = sum(name.startswith("indexbuffer") for name in base_names)
        model_files = sum(name.endswith(".model") for name in base_names)
        head_scenes = sum(name == "hihead.scne" for name in base_names)
        geometry_count = vertex_buffers + model_files
        if not geometry_count or not head_scenes:
            return False, "Check file", f"{len(names)} entries; recognizable head geometry is incomplete"
        geometry_label = f"{vertex_buffers} vertex buffers" if vertex_buffers else f"{model_files} model files"
        return True, "Ready", f"{len(names)} entries; {geometry_label}; {index_buffers} index buffers"

    def set_full_swap_row(self, iid, label, status, details):
        values = (status, details)
        if self.full_swap_tree.exists(iid):
            self.full_swap_tree.item(iid, text=label, values=values)
        else:
            self.full_swap_tree.insert("", tk.END, iid=iid, text=label, values=values)

    def refresh_full_swap_status(self):
        source_path = self.full_swap_source_var.get().strip()
        target_path = self.full_swap_target_var.get().strip()
        source_ready, source_status, source_details = self.inspect_full_swap_iff(source_path)
        target_ready, target_status, target_details = self.inspect_full_swap_iff(target_path)
        if source_path and target_path and os.path.abspath(source_path) == os.path.abspath(target_path):
            target_ready = False
            target_status = "Change file"
            target_details = "Source and target must be different character IFFs"

        blender_path = self.full_swap_blender_var.get().strip()
        blender_ready = os.path.isfile(blender_path) and os.path.basename(blender_path).lower() == "blender.exe"
        blender_status = "Ready" if blender_ready else "Missing"
        blender_details = blender_path if blender_path else "Blender was not detected"

        tool_path = self.full_swap_tool_var.get().strip()
        tool_ready = os.path.isfile(os.path.join(tool_path, "__init__.py"))
        tool_version = self.read_head_swap_tool_version(tool_path) if tool_ready else ""
        tool_status = "Ready" if tool_ready else "Missing"
        tool_details = tool_path if tool_path else "NBA Character Head Swap tool was not detected"
        if tool_ready and tool_version:
            tool_details = f"Version {tool_version} | {tool_path}"

        mesh_transfer_path = self.find_mesh_data_transfer_tool()
        mesh_transfer_ready = os.path.isfile(os.path.join(mesh_transfer_path, "__init__.py"))
        mesh_transfer_status = "Ready" if mesh_transfer_ready else "Missing"
        mesh_transfer_details = mesh_transfer_path if mesh_transfer_path else "Mesh Data Transfer was not detected"

        bridge_ready = self.full_swap_bridge_result.startswith("Connected")
        bridge_status = "Connected" if bridge_ready else self.full_swap_bridge_result
        bridge_details = "Blender can load the current swap tool in background mode" if bridge_ready else "Run Test Background Link"

        self.set_full_swap_row("source", "Source IFF", source_status, source_details)
        self.set_full_swap_row("target", "Target IFF", target_status, target_details)
        self.set_full_swap_row("blender", "Blender Background", blender_status, blender_details)
        self.set_full_swap_row("tool", "Blender Swap Tool", tool_status, tool_details)
        self.set_full_swap_row("mesh_transfer", "Mesh Data Transfer", mesh_transfer_status, mesh_transfer_details)
        self.set_full_swap_row("bridge", "Background Link", bridge_status, bridge_details)
        setup_ready = source_ready and target_ready and blender_ready and tool_ready and mesh_transfer_ready
        pipeline_ready = setup_ready and bridge_ready and os.path.isfile(FULL_SWAP_BRIDGE)
        self.set_full_swap_row(
            "pipeline",
            "Automated Head Swap",
            "Ready" if pipeline_ready else "Waiting",
            "Head shape, eyes, mouth, and eyelashes only; body geometry is preserved",
        )

        self.full_swap_run_button.configure(
            state=tk.NORMAL if pipeline_ready and self.full_swap_process is None else tk.DISABLED
        )
        if pipeline_ready:
            self.full_swap_status_var.set("Head Swap is ready to run in background Blender.")
        elif setup_ready:
            self.full_swap_status_var.set("Swap files and tools are ready. Test the background link.")

    def set_body_swap_row(self, iid, label, status, details):
        values = (status, details)
        if self.body_swap_tree.exists(iid):
            self.body_swap_tree.item(iid, text=label, values=values)
        else:
            self.body_swap_tree.insert("", tk.END, iid=iid, text=label, values=values)

    def refresh_body_swap_status(self):
        source_path = self.body_swap_source_var.get().strip()
        target_path = self.body_swap_target_var.get().strip()
        source_ready, source_status, source_details = self.inspect_full_swap_iff(source_path)
        target_ready, target_status, target_details = self.inspect_full_swap_iff(target_path)
        if source_path and target_path and os.path.abspath(source_path) == os.path.abspath(target_path):
            target_ready = False
            target_status = "Change file"
            target_details = "Source and target must be different character IFFs"

        blender_path = self.full_swap_blender_var.get().strip()
        blender_ready = os.path.isfile(blender_path) and os.path.basename(blender_path).lower() == "blender.exe"
        tool_path = self.full_swap_tool_var.get().strip()
        tool_ready = os.path.isfile(os.path.join(tool_path, "__init__.py"))
        tool_version = self.read_head_swap_tool_version(tool_path) if tool_ready else ""
        mesh_transfer_path = self.find_mesh_data_transfer_tool()
        mesh_transfer_ready = os.path.isfile(os.path.join(mesh_transfer_path, "__init__.py"))
        bridge_ready = self.full_swap_bridge_result.startswith("Connected")

        self.set_body_swap_row("source", "Source IFF", source_status, source_details)
        self.set_body_swap_row("target", "Target IFF", target_status, target_details)
        self.set_body_swap_row(
            "blender",
            "Blender Background",
            "Ready" if blender_ready else "Missing",
            blender_path or "Blender was not detected",
        )
        self.set_body_swap_row(
            "tool",
            "Blender Swap Tool",
            "Ready" if tool_ready else "Missing",
            (f"Version {tool_version} | {tool_path}" if tool_version else tool_path)
            or "NBA Character Head Swap tool was not detected",
        )
        self.set_body_swap_row(
            "mesh_transfer",
            "Mesh Data Transfer",
            "Ready" if mesh_transfer_ready else "Missing",
            mesh_transfer_path or "Mesh Data Transfer was not detected",
        )
        self.set_body_swap_row(
            "bridge",
            "Background Link",
            "Connected" if bridge_ready else self.full_swap_bridge_result,
            "Blender can load the current swap tool in background mode"
            if bridge_ready else "Run Test Background Link",
        )
        setup_ready = source_ready and target_ready and blender_ready and tool_ready and mesh_transfer_ready
        pipeline_ready = setup_ready and bridge_ready and os.path.isfile(FULL_SWAP_BRIDGE)
        self.set_body_swap_row(
            "pipeline",
            "Automated Body Swap",
            "Ready" if pipeline_ready else "Waiting",
            "Matched body groups when topology agrees; legacy shrinkwrap fallback otherwise",
        )
        self.body_swap_run_button.configure(
            state=tk.NORMAL if pipeline_ready and self.full_swap_process is None else tk.DISABLED
        )
        if pipeline_ready:
            self.body_swap_status_var.set("Body Swap is ready to run in background Blender.")
        elif setup_ready:
            self.body_swap_status_var.set("Body files and tools are ready. Test the background link.")

    def browse_everything_swap_source(self):
        current = self.everything_swap_source_var.get().strip()
        path = filedialog.askopenfilename(
            title="Choose Full Swap source character IFF or ZIP",
            filetypes=[
                ("NBA 2K character package", "*.iff *.zip"),
                ("NBA 2K character IFF", "*.iff"),
                ("ZIP package", "*.zip"),
                ("All files", "*.*"),
            ],
            initialdir=os.path.dirname(current or self.file_path),
        )
        if path:
            try:
                if path.lower().endswith(".zip"):
                    self.prepare_everything_swap_source_package(path)
                else:
                    self.clear_everything_swap_source_package()
            except (OSError, ValueError, zipfile.BadZipFile) as exc:
                messagebox.showerror("Character Mod Tool", f"Could not open source package.\n\n{exc}")
                return
            self.everything_swap_source_var.set(path)
            self.everything_swap_hair_source_var.set("")
            self.refresh_everything_swap_hair_options(auto_detect_source=True)

    def create_recent_output_combo(self, parent, width=32):
        combo = ttk.Combobox(
            parent,
            textvariable=self.recent_output_var,
            state="readonly",
            width=width,
        )
        combo.bind("<<ComboboxSelected>>", self.load_selected_recent_output)
        combo.bind(
            "<Button-1>",
            lambda _event: self.refresh_recent_output_choices(),
            add="+",
        )
        self.recent_output_combos.append(combo)
        return combo

    def recent_output_candidates(self):
        output_dir = os.path.abspath(
            app_settings.ensure_output_dir(self.settings.get("output_dir", ""))
        )
        if not os.path.isdir(output_dir):
            return []
        candidates = []
        for root, directories, filenames in os.walk(output_dir):
            directories[:] = [
                name
                for name in directories
                if not name.startswith((".character_mod_", "texture_exports"))
            ]
            for filename in filenames:
                if not re.fullmatch(r"(?i)png\d+\.iff", filename):
                    continue
                path = os.path.join(root, filename)
                try:
                    modified = os.path.getmtime(path)
                except OSError:
                    continue
                candidates.append((modified, os.path.abspath(path)))
        candidates.sort(key=lambda item: (-item[0], item[1].lower()))
        return candidates[:40]

    def refresh_recent_output_choices(self, select_path=""):
        previous_path = self.recent_output_path_map.get(
            self.recent_output_var.get(),
            "",
        )
        output_dir = os.path.abspath(
            app_settings.ensure_output_dir(self.settings.get("output_dir", ""))
        )
        path_map = {}
        for modified, path in self.recent_output_candidates():
            try:
                relative_parent = os.path.relpath(os.path.dirname(path), output_dir)
            except ValueError:
                relative_parent = ""
            timestamp = datetime.fromtimestamp(modified).strftime("%m/%d %I:%M %p")
            location = "" if relative_parent in ("", ".") else f" | {relative_parent}"
            label = f"{os.path.basename(path)} | {timestamp}{location}"
            if label in path_map:
                label = f"{label} | {len(path_map) + 1}"
            path_map[label] = path

        self.recent_output_path_map = path_map
        values = tuple(path_map)
        for combo in self.recent_output_combos:
            combo.configure(values=values)

        wanted = os.path.abspath(select_path) if select_path else previous_path
        selected_label = next(
            (
                label
                for label, path in path_map.items()
                if wanted and os.path.normcase(path) == os.path.normcase(wanted)
            ),
            "",
        )
        if not selected_label and values:
            selected_label = values[0]
        self.recent_output_var.set(selected_label)

    def load_selected_recent_output(self, _event=None):
        path = self.recent_output_path_map.get(self.recent_output_var.get(), "")
        if not path or not os.path.isfile(path):
            self.refresh_recent_output_choices()
            path = self.recent_output_path_map.get(self.recent_output_var.get(), "")
        if not path:
            messagebox.showinfo(
                "Character Mod Tool",
                "No completed png####.iff files were found in the output folder.",
            )
            return

        self.load_iff(path)
        if os.path.normcase(os.path.abspath(self.file_path or "")) != os.path.normcase(path):
            return
        self.rename_package_source_var.set(path)
        self.rename_package_status_var.set(
            f"Recent output loaded: {os.path.basename(path)}"
        )
        self.everything_swap_target_info_var.set(
            f"Recent output loaded for follow-up work: {os.path.basename(path)}."
        )

        prefix = self.player_iff_prefix(path)
        headband_path = ""
        if prefix:
            exact = os.path.join(os.path.dirname(path), f"{prefix}_geo_headband.iff")
            matches = sorted(
                glob.glob(os.path.join(os.path.dirname(path), f"{prefix}_geo_headband*.iff"))
            )
            headband_path = exact if os.path.isfile(exact) else (matches[0] if matches else "")
        self.headband_target_var.set(headband_path)
        self.refresh_headband_status()
        self.refresh_recent_output_choices(select_path=path)
        self.status_var.set(
            f"Loaded recent output {os.path.basename(path)} for follow-up adjustments."
        )

    @staticmethod
    def source_package_member_basename(member_name):
        return member_name.replace("\\", "/").rstrip("/").rsplit("/", 1)[-1]

    @classmethod
    def select_source_package_main(cls, package_path, members):
        main_candidates = []
        companion_counts = {}
        basenames = {
            info.filename: cls.source_package_member_basename(info.filename)
            for info in members
            if not info.is_dir()
        }
        for member_name, basename in basenames.items():
            match = re.fullmatch(r"(?i)png(\d+)\.iff", basename)
            if not match:
                continue
            png_id = match.group(1)
            main_candidates.append((member_name, basename, png_id))
            companion_pattern = re.compile(
                rf"(?i)^(?:face{re.escape(png_id)}(?:_.+)?|png{re.escape(png_id)}_.+)\.iff$"
            )
            companion_counts[member_name] = sum(
                bool(companion_pattern.fullmatch(other_basename))
                for other_basename in basenames.values()
            )

        if not main_candidates:
            raise ValueError("The ZIP does not contain a main character file named png####.iff.")
        if len(main_candidates) == 1:
            return main_candidates[0]

        package_match = re.search(r"(?i)png(\d+)", os.path.basename(package_path))
        if package_match:
            package_id = package_match.group(1)
            matching = [candidate for candidate in main_candidates if candidate[2] == package_id]
            if len(matching) == 1:
                return matching[0]

        best_count = max(companion_counts[candidate[0]] for candidate in main_candidates)
        best = [
            candidate
            for candidate in main_candidates
            if companion_counts[candidate[0]] == best_count
        ]
        if len(best) == 1:
            return best[0]
        choices = ", ".join(sorted(candidate[1] for candidate in best))
        raise ValueError(
            "The ZIP contains multiple possible main character IFFs and no unique package could "
            f"be identified: {choices}. Put one character package in each ZIP."
        )

    def prepare_everything_swap_source_package(self, package_path):
        package_path = os.path.abspath(package_path)
        if (
            self.everything_swap_source_package_path
            and os.path.normcase(self.everything_swap_source_package_path) == os.path.normcase(package_path)
            and os.path.isfile(self.everything_swap_source_package_main)
        ):
            return self.everything_swap_source_package_main
        if not os.path.isfile(package_path) or not zipfile.is_zipfile(package_path):
            raise ValueError(f"{os.path.basename(package_path)} is not a readable ZIP package.")

        with zipfile.ZipFile(package_path, "r") as package:
            members = package.infolist()
            if len(members) > 5000:
                raise ValueError("The ZIP contains too many entries to be treated as one character package.")
            main_member, main_basename, png_id = self.select_source_package_main(
                package_path,
                members,
            )
            package_pattern = re.compile(
                rf"(?i)^(?:png{re.escape(png_id)}|"
                rf"face{re.escape(png_id)}(?:_.+)?|"
                rf"png{re.escape(png_id)}_.+)\.iff$"
            )
            selected = [
                info
                for info in members
                if not info.is_dir()
                and package_pattern.fullmatch(self.source_package_member_basename(info.filename))
            ]
            if len(selected) > 500:
                raise ValueError("The selected character package contains too many companion IFFs.")
            total_size = sum(info.file_size for info in selected)
            if total_size > 2 * 1024 * 1024 * 1024:
                raise ValueError("The selected character package expands beyond the 2 GB safety limit.")

            output_names = {}
            for info in selected:
                basename = self.source_package_member_basename(info.filename)
                key = basename.lower()
                if key in output_names:
                    raise ValueError(
                        f"The ZIP contains duplicate companion filenames: {basename}."
                    )
                output_names[key] = basename

            work_dir = tempfile.mkdtemp(prefix="character_mod_source_package_")
            try:
                for info in selected:
                    basename = self.source_package_member_basename(info.filename)
                    output_path = os.path.join(work_dir, basename)
                    with package.open(info, "r") as source, open(output_path, "wb") as output:
                        shutil.copyfileobj(source, output, length=1024 * 1024)
                    if not zipfile.is_zipfile(output_path):
                        raise ValueError(f"{basename} is not a readable ZIP-style IFF.")
                main_path = os.path.join(work_dir, main_basename)
                if not os.path.isfile(main_path):
                    raise ValueError(f"The selected main character {main_basename} could not be extracted.")
            except Exception:
                self.remove_directory_with_retries(work_dir)
                raise

        self.clear_everything_swap_source_package()
        self.everything_swap_source_package_path = package_path
        self.everything_swap_source_package_dir = work_dir
        self.everything_swap_source_package_main = main_path
        LOGGER.info(
            "Prepared Full Swap source package %s as %s with %s companion file(s)",
            package_path,
            main_path,
            max(0, len(selected) - 1),
        )
        return main_path

    def resolve_everything_swap_source(self):
        selected = self.everything_swap_source_var.get().strip()
        if selected.lower().endswith(".zip"):
            return self.prepare_everything_swap_source_package(selected)
        return selected

    def clear_everything_swap_source_package(self):
        work_dir = self.everything_swap_source_package_dir
        self.everything_swap_source_package_path = ""
        self.everything_swap_source_package_dir = ""
        self.everything_swap_source_package_main = ""
        self.schedule_directory_cleanup(work_dir)

    def browse_everything_swap_target(self):
        current = self.everything_swap_target_var.get().strip()
        path = filedialog.askopenfilename(
            title="Choose Full Swap target character IFF",
            filetypes=[("NBA 2K character IFF", "*.iff"), ("All files", "*.*")],
            initialdir=os.path.dirname(current or self.file_path),
        )
        if path:
            self.clear_manifest_target_cache()
            self.everything_swap_target_var.set(path)
            self.everything_swap_target_info_var.set(
                f"Custom target: {os.path.basename(path)}. Companion files will be read from the same folder."
            )
            self.refresh_everything_swap_hair_options(auto_detect_source=False)

    def source_hair_candidates(self, character_path):
        if not character_path or not os.path.isfile(character_path):
            return []
        backend = self.load_hair_backend()
        character = backend.Path(character_path)
        folder = character.parent
        png_id = self.character_number_from_path(character_path)
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
            key = str(path.resolve()).lower()
            if key in seen or not path.is_file() or path.stem.lower().endswith("_tangentspace"):
                return
            try:
                backend.external_hair_identity(path)
            except Exception:
                return
            seen.add(key)
            candidates.append(path)

        if png_id:
            for hair_key in preferred_keys:
                add_candidate(folder / f"png{png_id}_geo_{hair_key}.iff")
            for pattern in (
                f"png{png_id}_geo_hair*.iff",
                f"png{png_id}_hair*.iff",
            ):
                for path in sorted(folder.glob(pattern)):
                    add_candidate(path)
        return candidates

    def refresh_everything_swap_hair_options(self, auto_detect_source=False):
        backend = self.load_hair_backend()
        target_path = self.everything_swap_target_var.get().strip()
        current_key = self.everything_swap_hair_slot_map.get(
            self.everything_swap_hair_slot_var.get(),
            "",
        )
        self.everything_swap_hair_slot_map = {}
        self.everything_swap_hair_slot_var.set("")
        self.everything_swap_hair_slot_combo.configure(values=())
        if target_path and os.path.isfile(target_path):
            try:
                summary = backend.parse_appearance_iff(backend.Path(target_path))
                slots = backend.appearance_asset_slots(summary, "hair")
            except Exception as exc:
                self.everything_swap_hair_status_var.set(f"Could not read target hair slots: {exc}")
                return
            self.everything_swap_hair_slot_map = {
                label: key for label, key, _tangent in slots
            }
            labels = tuple(self.everything_swap_hair_slot_map)
            self.everything_swap_hair_slot_combo.configure(values=labels)
            selected = next(
                (
                    label for label, key in self.everything_swap_hair_slot_map.items()
                    if key == current_key
                ),
                "",
            )
            if not selected and labels:
                selected = next((label for label in labels if " default)" in label), labels[0])
            self.everything_swap_hair_slot_var.set(selected)

        if auto_detect_source or not self.everything_swap_hair_source_var.get().strip():
            try:
                source_character = self.resolve_everything_swap_source()
            except (OSError, ValueError, zipfile.BadZipFile) as exc:
                self.everything_swap_hair_status_var.set(f"Could not read source package: {exc}")
                return
            candidates = self.source_hair_candidates(source_character)
            if candidates:
                self.everything_swap_hair_source_var.set(str(candidates[0]))
                self.everything_swap_hair_enabled_var.set(True)
        self.update_everything_swap_hair_status()

    def detect_everything_swap_hair(self):
        self.everything_swap_hair_source_var.set("")
        self.refresh_everything_swap_hair_options(auto_detect_source=True)
        if not self.everything_swap_hair_source_var.get().strip():
            messagebox.showinfo(
                "Character Mod Tool",
                "No supported NBA 2K23 or NBA 2K25 hair geometry was found beside the source character.",
            )

    def browse_everything_swap_hair_source(self):
        current = self.everything_swap_hair_source_var.get().strip()
        source_character = self.everything_swap_source_var.get().strip()
        path = filedialog.askopenfilename(
            title="Choose NBA 2K23 or NBA 2K25 source hair geometry IFF",
            filetypes=[("NBA 2K hair IFF", "*.iff"), ("All files", "*.*")],
            initialdir=os.path.dirname(current or source_character),
        )
        if not path:
            return
        try:
            backend = self.load_hair_backend()
            generation, _source_png, source_key = backend.external_hair_identity(
                backend.Path(path)
            )
        except Exception as exc:
            messagebox.showerror("Unsupported Hair", str(exc))
            return
        self.everything_swap_hair_source_var.set(path)
        self.everything_swap_hair_enabled_var.set(True)
        self.everything_swap_hair_status_var.set(
            f"Ready: {generation} {source_key}. Choose the target slot and run Full Swap."
        )

    def update_everything_swap_hair_status(self):
        source = self.everything_swap_hair_source_var.get().strip()
        target_label = self.everything_swap_hair_slot_var.get()
        target_key = self.everything_swap_hair_slot_map.get(target_label, "")
        if not self.everything_swap_hair_enabled_var.get():
            self.everything_swap_hair_status_var.set(
                "Hair Swap is off; target hair companion files will be preserved."
            )
        elif not source:
            self.everything_swap_hair_status_var.set(
                "Choose or detect an NBA 2K23/2K25 source hair geometry IFF."
            )
        elif not target_key:
            self.everything_swap_hair_status_var.set(
                "The target character has no selectable hair appearance slot."
            )
        else:
            self.everything_swap_hair_status_var.set(
                f"Full Swap will convert {os.path.basename(source)} into target slot {target_key}."
            )

    def update_everything_swap_rename_state(self):
        enabled = self.everything_swap_rename_enabled_var.get()
        self.everything_swap_rename_entry.configure(state=tk.NORMAL if enabled else tk.DISABLED)
        if enabled:
            self.everything_swap_rename_entry.focus_set()

    @staticmethod
    def manifest_asset_summary(values):
        if not values:
            return "None"
        values = sorted(values, key=str.lower)
        return ", ".join(values[:4]) + (f" (+{len(values) - 4} more)" if len(values) > 4 else "")

    def manifest_target_catalog(self):
        backend = self.load_hair_backend()
        manifest = backend.parse_manifest_entries()
        if not manifest:
            raise ValueError(
                "The NBA 2K26 manifest could not be read. Open Settings and verify the game folder first."
            )
        player_names = backend.load_player_names()
        players = {}
        for name, _archive_id, _offset, _size in manifest.values():
            normalized = name.replace("\\", "/")
            main_match = re.fullmatch(r"(?i)char/sig/png(\d+)\.iff", normalized)
            if main_match:
                png = main_match.group(1)
                players.setdefault(
                    png,
                    {
                        "png": png,
                        "name": player_names.get(str(int(png)), "Unknown Player"),
                        "archive_entry": name,
                        "hair": set(),
                        "facialhair": set(),
                        "headband": set(),
                    },
                )["archive_entry"] = name
                continue
            asset_match = re.fullmatch(
                r"(?i)char/sig/png(\d+)_geo_(hair|facialhair|headband)(.*?)\.iff",
                normalized,
            )
            if not asset_match:
                continue
            png, asset_type, suffix = asset_match.groups()
            row = players.setdefault(
                png,
                {
                    "png": png,
                    "name": player_names.get(str(int(png)), "Unknown Player"),
                    "archive_entry": "",
                    "hair": set(),
                    "facialhair": set(),
                    "headband": set(),
                },
            )
            asset_name = (asset_type + suffix).removesuffix("_tangentspace")
            row[asset_type.lower()].add(asset_name)
        rows = [row for row in players.values() if row["archive_entry"]]
        rows.sort(key=lambda row: (row["name"] == "Unknown Player", row["name"].lower(), int(row["png"])))
        return rows

    def open_manifest_target_picker(self):
        try:
            rows = self.manifest_target_catalog()
        except Exception as exc:
            LOGGER.exception("Could not build manifest target catalog")
            messagebox.showerror("Character Mod Tool", f"Could not load NBA 2K26 players:\n\n{exc}")
            return

        window = tk.Toplevel(self)
        window.title("Select NBA 2K26 Target from Manifest")
        window.geometry("980x650")
        window.minsize(760, 470)
        window.transient(self)
        window.grab_set()

        body = ttk.Frame(window, padding=10)
        body.pack(fill=tk.BOTH, expand=True)
        search_row = ttk.Frame(body)
        search_row.pack(fill=tk.X, pady=(0, 8))
        ttk.Label(search_row, text="Search").pack(side=tk.LEFT)
        search_var = tk.StringVar()
        search_entry = ttk.Entry(search_row, textvariable=search_var, width=34)
        search_entry.pack(side=tk.LEFT, padx=(6, 14))
        hair_only = tk.BooleanVar(value=False)
        facial_only = tk.BooleanVar(value=False)
        headband_only = tk.BooleanVar(value=False)
        ttk.Checkbutton(search_row, text="Has Hair", variable=hair_only).pack(side=tk.LEFT)
        ttk.Checkbutton(search_row, text="Has Facial Hair", variable=facial_only).pack(side=tk.LEFT, padx=(8, 0))
        ttk.Checkbutton(search_row, text="Has Headband", variable=headband_only).pack(side=tk.LEFT, padx=(8, 0))

        tree_frame = ttk.Frame(body)
        tree_frame.pack(fill=tk.BOTH, expand=True)
        columns = ("player", "png", "hair", "facialhair", "headband")
        tree = ttk.Treeview(tree_frame, columns=columns, show="headings", selectmode="browse")
        headings = {
            "player": "Player",
            "png": "PNG",
            "hair": "Hair",
            "facialhair": "Facial Hair",
            "headband": "Headband",
        }
        widths = {"player": 330, "png": 95, "hair": 110, "facialhair": 120, "headband": 110}
        for column in columns:
            tree.heading(column, text=headings[column])
            tree.column(column, width=widths[column], anchor=tk.W if column == "player" else tk.CENTER)
        y_scroll = ttk.Scrollbar(tree_frame, orient=tk.VERTICAL, command=tree.yview)
        tree.configure(yscrollcommand=y_scroll.set)
        tree.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)
        y_scroll.pack(side=tk.RIGHT, fill=tk.Y)

        detail_var = tk.StringVar(value="Select a player to see the exact manifest assets.")
        count_var = tk.StringVar()
        ttk.Label(body, textvariable=detail_var, wraplength=930).pack(fill=tk.X, pady=(8, 3))
        ttk.Label(body, textvariable=count_var).pack(fill=tk.X)
        row_by_iid = {}

        def refresh_rows(*_args):
            query = search_var.get().strip().lower()
            tree.delete(*tree.get_children())
            row_by_iid.clear()
            for row in rows:
                if query and query not in row["name"].lower() and query not in row["png"].lower():
                    continue
                if hair_only.get() and not row["hair"]:
                    continue
                if facial_only.get() and not row["facialhair"]:
                    continue
                if headband_only.get() and not row["headband"]:
                    continue
                iid = f"png_{row['png']}"
                row_by_iid[iid] = row
                tree.insert(
                    "",
                    tk.END,
                    iid=iid,
                    values=(
                        row["name"],
                        f"png{row['png']}",
                        "Yes" if row["hair"] else "No",
                        "Yes" if row["facialhair"] else "No",
                        "Yes" if row["headband"] else "No",
                    ),
                )
            count_var.set(f"{len(row_by_iid):,} of {len(rows):,} manifest players shown")

        def update_details(_event=None):
            selection = tree.selection()
            if not selection:
                return
            row = row_by_iid.get(selection[0])
            if not row:
                return
            detail_var.set(
                f"Hair: {self.manifest_asset_summary(row['hair'])}    |    "
                f"Facial Hair: {self.manifest_asset_summary(row['facialhair'])}    |    "
                f"Headband: {self.manifest_asset_summary(row['headband'])}"
            )

        def use_selected(_event=None):
            selection = tree.selection()
            if not selection:
                messagebox.showinfo("Character Mod Tool", "Select a target player first.", parent=window)
                return
            row = row_by_iid.get(selection[0])
            if not row:
                return
            detail_var.set(f"Extracting clean png{row['png']} and its companion files...")
            window.update_idletasks()
            try:
                target_path, companion_count = self.extract_manifest_target(row)
            except Exception as exc:
                LOGGER.exception("Could not extract manifest target png%s", row["png"])
                messagebox.showerror(
                    "Character Mod Tool",
                    f"Could not prepare png{row['png']} from the manifest:\n\n{exc}",
                    parent=window,
                )
                update_details()
                return
            self.everything_swap_target_var.set(target_path)
            self.everything_swap_target_info_var.set(
                f"Manifest target: {row['name']} (png{row['png']}) | "
                f"Hair: {'Yes' if row['hair'] else 'No'} | "
                f"Facial Hair: {'Yes' if row['facialhair'] else 'No'} | "
                f"Headband: {'Yes' if row['headband'] else 'No'} | "
                f"{companion_count} companion IFF(s) staged"
            )
            self.refresh_everything_swap_hair_options(auto_detect_source=False)
            window.grab_release()
            window.destroy()

        search_var.trace_add("write", refresh_rows)
        for variable in (hair_only, facial_only, headband_only):
            variable.trace_add("write", refresh_rows)
        tree.bind("<<TreeviewSelect>>", update_details)
        tree.bind("<Double-1>", use_selected)
        button_row = ttk.Frame(body)
        button_row.pack(fill=tk.X, pady=(10, 0))
        ttk.Button(button_row, text="Use Selected Target", command=use_selected).pack(side=tk.RIGHT)
        ttk.Button(button_row, text="Cancel", command=window.destroy).pack(side=tk.RIGHT, padx=(0, 6))
        refresh_rows()
        search_entry.focus_set()

    def extract_manifest_target(self, row):
        backend = self.load_hair_backend()
        manifest = backend.parse_manifest_entries()
        png = row["png"]
        work_dir = tempfile.mkdtemp(prefix=f"character_mod_manifest_png{png}_")
        try:
            target_path = os.path.join(work_dir, f"png{png}.iff")
            backend.extract_archive_iff_fallback(row["archive_entry"], backend.Path(target_path))
            ready, _status, details = self.inspect_full_swap_iff(target_path)
            if not ready:
                raise ValueError(f"The extracted target is not swap-ready: {details}")

            player_pattern = re.compile(rf"(?i)^char/sig/png{re.escape(png)}_(?!config).+\.iff$")
            face_pattern = re.compile(rf"(?i)^char/sig/face{re.escape(png)}.*\.iff$")
            archive_entries = sorted(
                {
                    entry[0]
                    for entry in manifest.values()
                    if player_pattern.fullmatch(entry[0].replace("\\", "/"))
                    or face_pattern.fullmatch(entry[0].replace("\\", "/"))
                },
                key=str.lower,
            )
            companion_dir = os.path.join(work_dir, "companions")
            companions = []
            for archive_entry in archive_entries:
                source = os.path.join(companion_dir, os.path.basename(archive_entry))
                backend.extract_archive_iff_alias_aware(archive_entry, backend.Path(source))
                companions.append((source, archive_entry))
        except Exception:
            shutil.rmtree(work_dir, ignore_errors=True)
            raise

        self.clear_manifest_target_cache()
        self.everything_swap_manifest_target_path = target_path
        self.everything_swap_manifest_work_dir = work_dir
        self.everything_swap_manifest_companions = companions
        return target_path, len(companions)

    def clear_manifest_target_cache(self):
        work_dir = self.everything_swap_manifest_work_dir
        self.everything_swap_manifest_target_path = ""
        self.everything_swap_manifest_work_dir = ""
        self.everything_swap_manifest_companions = []
        if work_dir and os.path.isdir(work_dir):
            shutil.rmtree(work_dir, ignore_errors=True)

    @staticmethod
    def normalized_png_export_name(value):
        text = os.path.splitext(os.path.basename(str(value or "").strip()))[0]
        match = re.fullmatch(r"(?i)(?:png)?(\d+)", text)
        return f"png{match.group(1)}" if match else ""

    @classmethod
    def export_companion_plan(cls, target_path, export_name, output_dir):
        target_number = cls.character_number_from_path(target_path)
        export_number = cls.character_number_from_path(export_name)
        target_dir = os.path.dirname(target_path)
        if not target_number or not export_number or not os.path.isdir(target_dir):
            return []
        player_pattern = re.compile(rf"(?i)^png{re.escape(target_number)}_(.+)\.iff$")
        face_pattern = re.compile(rf"(?i)^face{re.escape(target_number)}(.*)\.iff$")
        plan = []
        for filename in sorted(os.listdir(target_dir), key=str.lower):
            source = os.path.join(target_dir, filename)
            if not os.path.isfile(source):
                continue
            player_match = player_pattern.fullmatch(filename)
            if player_match:
                suffix = player_match.group(1)
                if suffix.lower().startswith("config"):
                    continue
                output_name = f"png{export_number}_{suffix}.iff"
            else:
                face_match = face_pattern.fullmatch(filename)
                if not face_match:
                    continue
                output_name = f"face{export_number}{face_match.group(1)}.iff"
            plan.append((source, os.path.join(output_dir, output_name)))
        return plan

    def full_swap_companion_plan(self, target_path, export_name, output_dir):
        plan = self.export_companion_plan(target_path, export_name, output_dir)
        if (
            not self.everything_swap_manifest_target_path
            or os.path.abspath(target_path) != os.path.abspath(self.everything_swap_manifest_target_path)
        ):
            return plan
        target_number = self.character_number_from_path(target_path)
        export_number = self.character_number_from_path(export_name)
        if not target_number or not export_number:
            return plan
        player_pattern = re.compile(rf"(?i)^png{re.escape(target_number)}_(.+)\.iff$")
        face_pattern = re.compile(rf"(?i)^face{re.escape(target_number)}(.*)\.iff$")
        seen = {os.path.abspath(destination).lower() for _source, destination in plan}
        for source, archive_entry in self.everything_swap_manifest_companions:
            filename = os.path.basename(archive_entry)
            player_match = player_pattern.fullmatch(filename)
            if player_match:
                output_name = f"png{export_number}_{player_match.group(1)}.iff"
            else:
                face_match = face_pattern.fullmatch(filename)
                if not face_match:
                    continue
                output_name = f"face{export_number}{face_match.group(1)}.iff"
            destination = os.path.join(output_dir, output_name)
            key = os.path.abspath(destination).lower()
            if key not in seen:
                seen.add(key)
                plan.append((source, destination))
        return plan

    def planned_config_outputs(self, target_path, export_name, output_dir):
        outputs = []
        seen = set()
        for path in self.face_config_options_for_character(target_path).values():
            filename = re.sub(r"(?i)^png\d+", export_name, os.path.basename(path), count=1)
            destination = os.path.join(output_dir, filename)
            key = os.path.abspath(destination).lower()
            if key not in seen:
                seen.add(key)
                outputs.append(destination)
        return outputs

    def confirm_full_swap_output_plan(
        self,
        output_path,
        config_outputs,
        companion_plan,
        rename_name="",
    ):
        all_outputs = [output_path, *config_outputs, *(destination for _source, destination in companion_plan)]
        if self.everything_swap_hair_enabled_var.get():
            target_key = self.everything_swap_hair_slot_map.get(
                self.everything_swap_hair_slot_var.get(),
                "",
            )
            target_png = self.character_number_from_path(output_path)
            if target_key and target_png:
                all_outputs.append(
                    os.path.join(
                        os.path.dirname(output_path),
                        f"png{target_png}_geo_{target_key}.iff",
                    )
                )
        if rename_name:
            old_number = self.character_number_from_path(output_path)
            new_number = self.character_number_from_path(rename_name)
            renamed_outputs = []
            for path in all_outputs:
                filename = os.path.basename(path)
                filename = re.sub(
                    rf"(?i)^png{re.escape(old_number)}",
                    f"png{new_number}",
                    filename,
                    count=1,
                )
                filename = re.sub(
                    rf"(?i)^face{re.escape(old_number)}",
                    f"face{new_number}",
                    filename,
                    count=1,
                )
                renamed_outputs.append(os.path.join(os.path.dirname(path), filename))
            all_outputs.extend(renamed_outputs)
        unique_outputs = []
        seen = set()
        for path in all_outputs:
            key = os.path.abspath(path).lower()
            if key not in seen:
                seen.add(key)
                unique_outputs.append(path)
        collisions = [path for path in unique_outputs if os.path.exists(path)]
        detail = (
            f"Player IFF: 1\n"
            f"Face configs: {len(config_outputs)}\n"
            f"Target companion IFFs: {len(companion_plan)}\n"
            f"Hair swap: {'Yes' if self.everything_swap_hair_enabled_var.get() else 'No'}\n"
            f"Automatic package rename: {rename_name or 'No'}\n"
            f"Existing files to replace: {len(collisions)}"
        )
        if collisions:
            preview = "\n".join(os.path.basename(path) for path in collisions[:8])
            if len(collisions) > 8:
                preview += f"\n...and {len(collisions) - 8} more"
            detail += "\n\nFiles that will be replaced:\n" + preview
        return messagebox.askyesno(
            "Run Full Swap?",
            "The complete output package will be staged and validated before any final files are replaced.\n\n"
            + detail
            + "\n\nContinue?",
        )

    def browse_rename_package_source(self):
        path = filedialog.askopenfilename(
            title="Choose the finished character IFF",
            filetypes=[("NBA 2K character IFF", "*.iff"), ("All files", "*.*")],
            initialdir=app_settings.ensure_output_dir(self.settings.get("output_dir", "")),
        )
        if path:
            self.rename_package_source_var.set(path)
            source_name = self.normalized_png_export_name(os.path.basename(path))
            self.rename_package_status_var.set(
                f"Loaded {source_name or os.path.basename(path)}. Enter the final PNG number."
            )

    @classmethod
    def rename_package_plan(cls, source_main, new_name, output_dir):
        old_number = cls.character_number_from_path(source_main)
        new_number = cls.character_number_from_path(new_name)
        source_dir = os.path.dirname(source_main)
        if not old_number or not new_number or not os.path.isdir(source_dir):
            return []
        player_pattern = re.compile(rf"(?i)^png{re.escape(old_number)}(.*)\.iff$")
        face_pattern = re.compile(rf"(?i)^face{re.escape(old_number)}(.*)\.iff$")
        plan = []
        for filename in sorted(os.listdir(source_dir), key=str.lower):
            source = os.path.join(source_dir, filename)
            if not os.path.isfile(source):
                continue
            match = player_pattern.fullmatch(filename)
            if match:
                destination_name = f"png{new_number}{match.group(1)}.iff"
            else:
                match = face_pattern.fullmatch(filename)
                if not match:
                    continue
                destination_name = f"face{new_number}{match.group(1)}.iff"
            plan.append((source, os.path.join(output_dir, destination_name)))
        return plan

    @staticmethod
    def rename_text_references(name, data, old_number, new_number):
        extension = os.path.splitext(name)[1].lower()
        if extension not in TEXT_EXTENSIONS and extension != ".txtr":
            return data
        replacements = (
            (re.compile(rb"png" + re.escape(old_number.encode("ascii")), re.IGNORECASE), f"png{new_number}".encode("ascii")),
            (re.compile(rb"face" + re.escape(old_number.encode("ascii")), re.IGNORECASE), f"face{new_number}".encode("ascii")),
        )
        for pattern, replacement in replacements:
            data = pattern.sub(replacement, data)
        return data

    def stage_renamed_archive(self, source, staged, old_number, new_number):
        old_tokens = (f"png{old_number}".encode("ascii"), f"face{old_number}".encode("ascii"))
        needs_rewrite = False
        with zipfile.ZipFile(source, "r") as source_archive:
            for info in source_archive.infolist():
                lowered_name = info.filename.lower()
                if any(token.decode("ascii") in lowered_name for token in old_tokens):
                    needs_rewrite = True
                    break
                extension = os.path.splitext(info.filename)[1].lower()
                base_name = os.path.basename(info.filename).lower()
                reference_text = base_name == "appearance_info.json" or extension in {".txtr", ".txt", ".xml"}
                if (
                    info.is_dir()
                    or info.file_size > 1_000_000
                    or not reference_text
                ):
                    continue
                lowered_data = source_archive.read(info.filename).lower()
                if any(token in lowered_data for token in old_tokens):
                    needs_rewrite = True
                    break
        if not needs_rewrite:
            shutil.copy2(source, staged)
            return False
        with zipfile.ZipFile(source, "r") as source_archive, zipfile.ZipFile(staged, "w") as output_archive:
            for info in source_archive.infolist():
                data = b"" if info.is_dir() else source_archive.read(info.filename)
                data = self.rename_text_references(info.filename, data, old_number, new_number)
                output_name = re.sub(rf"(?i)png{re.escape(old_number)}", f"png{new_number}", info.filename)
                output_name = re.sub(rf"(?i)face{re.escape(old_number)}", f"face{new_number}", output_name)
                output_archive.writestr(self.copied_zip_info(info, output_name), data)
        return True

    @staticmethod
    def archive_metadata_signature(path):
        with zipfile.ZipFile(path, "r") as archive:
            return [
                (info.filename, info.CRC, info.file_size, info.compress_size)
                for info in archive.infolist()
            ]

    @staticmethod
    def stage_original_package_deletion(plan, stage_dir):
        sources = []
        seen = set()
        destinations = {os.path.abspath(destination).lower() for _source, destination in plan}
        for source, _destination in plan:
            key = os.path.abspath(source).lower()
            if key in seen:
                continue
            if key in destinations:
                raise ValueError(f"Refusing to delete a file that is also a rename destination: {source}")
            if not os.path.isfile(source):
                raise FileNotFoundError(f"Original package file is missing: {source}")
            seen.add(key)
            sources.append(source)

        deletion_dir = os.path.join(stage_dir, "original_package_to_delete")
        os.makedirs(deletion_dir, exist_ok=True)
        moved = []
        try:
            for index, source in enumerate(sources):
                held = os.path.join(deletion_dir, f"{index:04d}_{os.path.basename(source)}")
                shutil.move(source, held)
                moved.append((source, held))
        except Exception:
            for original, held in reversed(moved):
                try:
                    if os.path.exists(held) and not os.path.exists(original):
                        os.makedirs(os.path.dirname(original), exist_ok=True)
                        shutil.move(held, original)
                except OSError:
                    LOGGER.exception("Could not restore original package file %s", original)
            raise
        return [source for source, _held in moved]

    @classmethod
    def rename_package_plan_for_sources(cls, source_main, source_paths, new_name, output_dir):
        old_number = cls.character_number_from_path(source_main)
        new_number = cls.character_number_from_path(new_name)
        if not old_number or not new_number:
            return []
        player_pattern = re.compile(rf"(?i)^png{re.escape(old_number)}(.*)\.iff$")
        face_pattern = re.compile(rf"(?i)^face{re.escape(old_number)}(.*)\.iff$")
        plan = []
        seen = set()
        for source in source_paths:
            source = os.path.abspath(source)
            source_key = source.lower()
            if source_key in seen or not os.path.isfile(source):
                continue
            seen.add(source_key)
            filename = os.path.basename(source)
            match = player_pattern.fullmatch(filename)
            if match:
                destination_name = f"png{new_number}{match.group(1)}.iff"
            else:
                match = face_pattern.fullmatch(filename)
                if not match:
                    continue
                destination_name = f"face{new_number}{match.group(1)}.iff"
            plan.append((source, os.path.join(output_dir, destination_name)))
        return plan

    def execute_character_package_rename(
        self,
        source_main,
        new_name,
        output_dir,
        delete_original=True,
        source_paths=None,
    ):
        old_number = self.character_number_from_path(source_main)
        new_number = self.character_number_from_path(new_name)
        if not source_main or not os.path.isfile(source_main) or not zipfile.is_zipfile(source_main):
            raise ValueError("Choose a readable finished character IFF first.")
        if not old_number or not new_number:
            raise ValueError("Enter a PNG number such as 1335 or png1335.")
        if old_number == new_number:
            raise ValueError("The new PNG number must be different from the current number.")
        if source_paths is None:
            plan = self.rename_package_plan(source_main, new_name, output_dir)
        else:
            plan = self.rename_package_plan_for_sources(
                source_main,
                source_paths,
                new_name,
                output_dir,
            )
        if not plan:
            raise ValueError("No matching character package files were found.")

        stage_dir = tempfile.mkdtemp(prefix=".character_mod_rename_", dir=output_dir)
        staged_outputs = []
        total_warnings = 0
        deleted_originals = []
        deletion_error = ""
        try:
            for index, (source, destination) in enumerate(plan):
                staged = os.path.join(stage_dir, f"{index:04d}_{os.path.basename(destination)}")
                rewritten = self.stage_renamed_archive(source, staged, old_number, new_number)
                if rewritten:
                    results = self.validate_archive_file(staged)
                    errors = [result for result in results if result.severity == "ERROR"]
                    if errors:
                        raise ValueError(
                            f"{os.path.basename(source)} failed validation: "
                            + "; ".join(f"{result.check}: {result.details}" for result in errors[:4])
                        )
                    total_warnings += sum(result.severity == "WARNING" for result in results)
                elif self.archive_metadata_signature(source) != self.archive_metadata_signature(staged):
                    raise ValueError(f"{os.path.basename(source)} did not pass archive-copy integrity checks.")
                staged_outputs.append((staged, destination))
            self.commit_staged_outputs(staged_outputs, work_dir=stage_dir)
            if delete_original:
                try:
                    deleted_originals = self.stage_original_package_deletion(plan, stage_dir)
                except Exception as exc:
                    deletion_error = str(exc)
                    LOGGER.exception("Renamed package succeeded, but original package deletion was rolled back")
        finally:
            self.schedule_directory_cleanup(stage_dir)

        new_main = next(
            (
                destination
                for _source, destination in plan
                if os.path.basename(destination).lower() == f"{new_name}.iff".lower()
            ),
            "",
        )
        if not new_main:
            raise ValueError("The renamed package did not produce its main player IFF.")
        return {
            "old_number": old_number,
            "new_number": new_number,
            "new_main": new_main,
            "outputs": [destination for _source, destination in plan],
            "path_map": {
                os.path.abspath(source).lower(): destination
                for source, destination in plan
            },
            "warnings": total_warnings,
            "deleted_originals": deleted_originals,
            "deletion_error": deletion_error,
        }

    def rename_character_package(self):
        source_main = self.rename_package_source_var.get().strip()
        new_name = self.normalized_png_export_name(self.rename_package_new_name_var.get())
        if not source_main or not os.path.isfile(source_main) or not zipfile.is_zipfile(source_main):
            messagebox.showerror("Character Mod Tool", "Choose a readable finished character IFF first.")
            return
        if not new_name:
            messagebox.showerror("Character Mod Tool", "Enter a PNG number such as 1335 or png1335.")
            return
        old_number = self.character_number_from_path(source_main)
        new_number = self.character_number_from_path(new_name)
        if old_number == new_number:
            messagebox.showinfo("Character Mod Tool", "The new PNG number must be different from the current number.")
            return
        output_dir = app_settings.ensure_output_dir(self.settings.get("output_dir", ""))
        if not output_dir:
            messagebox.showerror("Character Mod Tool", "The output folder could not be created.")
            return
        plan = self.rename_package_plan(source_main, new_name, output_dir)
        if not plan:
            messagebox.showerror("Character Mod Tool", "No matching character package files were found.")
            return
        collisions = [destination for _source, destination in plan if os.path.exists(destination)]
        delete_original = self.rename_package_delete_original_var.get()
        detail = f"Package files: {len(plan)}\nExisting files to replace: {len(collisions)}"
        if delete_original:
            detail += f"\nOriginal package files to delete after success: {len(plan)}"
        if collisions:
            detail += "\n\n" + "\n".join(os.path.basename(path) for path in collisions[:10])
        if not messagebox.askyesno(
            "Create Renamed Character Package?",
            f"png{old_number} will be copied to png{new_number}.\n\n"
            + (
                "After the new package is validated, the original package will be deleted."
                if delete_original
                else "The original package will remain unchanged."
            )
            + f"\n\n{detail}\n\nContinue?",
        ):
            return
        try:
            report = self.execute_character_package_rename(
                source_main,
                new_name,
                output_dir,
                delete_original=delete_original,
            )
        except Exception as exc:
            LOGGER.exception("Rename Character Package failed")
            messagebox.showerror("Character Mod Tool", f"Rename Character Package did not complete.\n\n{exc}")
            self.rename_package_status_var.set("Rename Character Package did not complete.")
            return

        self.rename_package_new_name_var.set(new_name)
        new_main = report["new_main"]
        deleted_originals = report["deleted_originals"]
        deletion_error = report["deletion_error"]
        total_warnings = report["warnings"]
        if deleted_originals and new_main:
            self.rename_package_source_var.set(new_main)
        original_status = (
            f"Deleted {len(deleted_originals)} original package file(s)."
            if deleted_originals
            else ("Original package deletion failed; all original files were kept." if deletion_error else "Original package kept.")
        )
        self.rename_package_status_var.set(
            f"Created png{new_number} package with {len(plan)} files and {total_warnings} validation warning(s). "
            + original_status
        )
        LOGGER.info("Renamed character package png%s to png%s: %s files", old_number, new_number, len(plan))
        message = (
            f"Renamed package created successfully.\n\nFiles: {len(plan)}\n"
            f"Validation warnings: {total_warnings}\n{original_status}\nFolder: {output_dir}"
        )
        if deletion_error:
            message += f"\n\nDeletion error: {deletion_error}"
            messagebox.showwarning("Character Mod Tool", message)
        else:
            messagebox.showinfo("Character Mod Tool", message)

    @classmethod
    def matching_legacy_face_path(cls, character_path):
        number = cls.character_number_from_path(character_path)
        folder = os.path.dirname(character_path or "")
        if not number or not folder:
            return ""
        exact = os.path.join(folder, f"face{number}.iff")
        if os.path.isfile(exact):
            return exact
        matches = sorted(
            path for path in glob.glob(os.path.join(folder, "face*.iff"))
            if number in os.path.splitext(os.path.basename(path))[0]
        )
        return matches[0] if matches else ""

    def matching_source_face_texture_path(self, character_path):
        face_path = self.matching_legacy_face_path(character_path)
        if face_path:
            return face_path

        number = self.character_number_from_path(character_path)
        folder = os.path.dirname(character_path or "")
        if not number or not folder:
            return ""
        candidates = self.config_files_for_character(character_path)
        if not candidates:
            return ""

        default_config = ""
        try:
            _name, appearance = self.read_appearance_from_archive(character_path)
            accessory_items = appearance.get("accessory_items") if isinstance(appearance, dict) else None
            if isinstance(accessory_items, dict):
                default_config = str(accessory_items.get("default_config") or "").strip()
        except (OSError, ValueError, zipfile.BadZipFile):
            pass

        if default_config:
            suffix = re.sub(r"[^A-Za-z0-9_-]+", "_", default_config).strip("_").lower()
            preferred = os.path.join(folder, f"png{number}_config_{suffix}.iff")
            candidates = [preferred, *(path for path in candidates if os.path.abspath(path) != os.path.abspath(preferred))]

        for path in candidates:
            if not os.path.isfile(path) or not zipfile.is_zipfile(path):
                continue
            try:
                with zipfile.ZipFile(path, "r") as archive:
                    names = archive.namelist()
                    if all(
                        all(self.archive_texture_pair(names, logical))
                        for logical in ("face_color_o", "face_normal_o")
                    ):
                        return path
            except (OSError, zipfile.BadZipFile):
                continue
        return ""

    def run_everything_swap(self):
        source_selection = self.everything_swap_source_var.get().strip()
        target_path = self.everything_swap_target_var.get().strip()
        if not source_selection or not target_path:
            messagebox.showinfo("Character Mod Tool", "Choose Full Swap source and target characters first.")
            return
        try:
            source_path = self.resolve_everything_swap_source()
        except (OSError, ValueError, zipfile.BadZipFile) as exc:
            messagebox.showerror("Character Mod Tool", f"Could not open source package.\n\n{exc}")
            return
        source_ready = self.inspect_full_swap_iff(source_path)[0]
        target_ready = self.inspect_full_swap_iff(target_path)[0]
        if not source_ready or not target_ready:
            messagebox.showinfo("Character Mod Tool", "Choose readable source and target character IFFs first.")
            return
        source_face_textures = self.matching_source_face_texture_path(source_path)
        if not source_face_textures:
            messagebox.showerror(
                "Character Mod Tool",
                f"No face texture IFF or usable source config was found beside {os.path.basename(source_path)}.",
            )
            return
        target_configs = self.face_config_options_for_character(target_path)
        if not target_configs:
            messagebox.showerror("Character Mod Tool", "The target character has no appearance configurations.")
            return
        if not self.full_swap_bridge_result.startswith("Connected"):
            self.test_full_swap_bridge()
            if not self.full_swap_bridge_result.startswith("Connected"):
                return
        self.run_full_swap(combined=True)

    def browse_full_swap_source(self):
        current = self.full_swap_source_var.get().strip()
        path = filedialog.askopenfilename(
            title="Choose source character IFF",
            filetypes=[("NBA 2K character IFF", "*.iff"), ("All files", "*.*")],
            initialdir=os.path.dirname(current or self.file_path),
        )
        if path:
            self.full_swap_source_var.set(path)
            self.refresh_full_swap_status()

    def browse_full_swap_target(self):
        current = self.full_swap_target_var.get().strip()
        path = filedialog.askopenfilename(
            title="Choose target character IFF",
            filetypes=[("NBA 2K character IFF", "*.iff"), ("All files", "*.*")],
            initialdir=os.path.dirname(current or self.file_path),
        )
        if path:
            self.full_swap_target_var.set(path)
            self.refresh_full_swap_status()

    def use_open_iff_as_swap_target(self):
        if not self.file_path:
            messagebox.showinfo("Character Mod Tool", "Open the target character IFF first.")
            return
        self.full_swap_target_var.set(self.file_path)
        self.refresh_full_swap_status()

    def browse_body_swap_source(self):
        current = self.body_swap_source_var.get().strip()
        path = filedialog.askopenfilename(
            title="Choose Body Swap source character IFF",
            filetypes=[("NBA 2K character IFF", "*.iff"), ("All files", "*.*")],
            initialdir=os.path.dirname(current or self.file_path),
        )
        if path:
            self.body_swap_source_var.set(path)
            self.refresh_body_swap_status()

    def browse_body_swap_target(self):
        current = self.body_swap_target_var.get().strip()
        path = filedialog.askopenfilename(
            title="Choose Body Swap target character IFF",
            filetypes=[("NBA 2K character IFF", "*.iff"), ("All files", "*.*")],
            initialdir=os.path.dirname(current or self.file_path),
        )
        if path:
            self.body_swap_target_var.set(path)
            self.refresh_body_swap_status()

    def use_open_iff_as_body_swap_target(self):
        if not self.file_path:
            messagebox.showinfo("Character Mod Tool", "Open the target character IFF first.")
            return
        self.body_swap_target_var.set(self.file_path)
        self.refresh_body_swap_status()

    def browse_blender_executable(self):
        current = self.full_swap_blender_var.get().strip()
        path = filedialog.askopenfilename(
            title="Choose Blender",
            filetypes=[("Blender", "blender.exe"), ("Applications", "*.exe"), ("All files", "*.*")],
            initialdir=os.path.dirname(current) if current else "",
        )
        if path:
            self.full_swap_blender_var.set(path)
            self.settings["blender_exe"] = path
            app_settings.save_settings(self.settings)
            self.configure_hair_backend()
            self.full_swap_bridge_result = "Not tested"
            self.headband_bridge_result = "Not tested"
            self.refresh_full_swap_status()
            self.refresh_body_swap_status()
            self.refresh_headband_status()

    def browse_head_swap_tool(self):
        current = self.full_swap_tool_var.get().strip()
        path = filedialog.askdirectory(
            title="Choose NBA Character Head Swap tool folder",
            initialdir=current if os.path.isdir(current) else "",
        )
        if not path:
            return
        nested = os.path.join(path, "NBA_Character_HeadSwap")
        if os.path.isfile(os.path.join(nested, "__init__.py")):
            path = nested
        self.full_swap_tool_var.set(path)
        self.settings["head_swap_tool"] = path
        app_settings.save_settings(self.settings)
        self.full_swap_bridge_result = "Not tested"
        self.refresh_full_swap_status()
        self.refresh_body_swap_status()
        self.refresh_headband_status()

    def detect_full_swap_paths(self):
        blender_path = self.find_blender_executable()
        tool_path = self.find_head_swap_tool()
        if blender_path:
            self.full_swap_blender_var.set(blender_path)
        if tool_path:
            self.full_swap_tool_var.set(tool_path)
        self.full_swap_bridge_result = "Not tested"
        self.refresh_full_swap_status()
        self.refresh_body_swap_status()
        self.refresh_headband_status()

    def check_full_swap_setup(self):
        self.refresh_full_swap_status()
        source_ready = self.inspect_full_swap_iff(self.full_swap_source_var.get().strip())[0]
        target_ready = self.inspect_full_swap_iff(self.full_swap_target_var.get().strip())[0]
        blender_ready = os.path.isfile(self.full_swap_blender_var.get().strip())
        tool_ready = os.path.isfile(os.path.join(self.full_swap_tool_var.get().strip(), "__init__.py"))
        mesh_transfer_ready = os.path.isfile(os.path.join(self.find_mesh_data_transfer_tool(), "__init__.py"))
        if source_ready and target_ready and blender_ready and tool_ready and mesh_transfer_ready:
            self.full_swap_status_var.set("Setup is ready. Test the background link to Blender.")
        else:
            self.full_swap_status_var.set("Complete the items marked Missing, Check file, or Change file.")

    def check_body_swap_setup(self):
        self.refresh_body_swap_status()
        source_ready = self.inspect_full_swap_iff(self.body_swap_source_var.get().strip())[0]
        target_ready = self.inspect_full_swap_iff(self.body_swap_target_var.get().strip())[0]
        blender_ready = os.path.isfile(self.full_swap_blender_var.get().strip())
        tool_ready = os.path.isfile(os.path.join(self.full_swap_tool_var.get().strip(), "__init__.py"))
        mesh_transfer_ready = os.path.isfile(os.path.join(self.find_mesh_data_transfer_tool(), "__init__.py"))
        if source_ready and target_ready and blender_ready and tool_ready and mesh_transfer_ready:
            self.body_swap_status_var.set("Setup is ready. Test the background link to Blender.")
        else:
            self.body_swap_status_var.set("Complete the items marked Missing, Check file, or Change file.")

    def test_full_swap_bridge(self):
        blender_path = self.full_swap_blender_var.get().strip()
        tool_path = self.full_swap_tool_var.get().strip()
        if not os.path.isfile(blender_path):
            messagebox.showinfo("Character Mod Tool", "Choose or detect Blender first.")
            return
        if not os.path.isfile(os.path.join(tool_path, "__init__.py")):
            messagebox.showinfo("Character Mod Tool", "Choose or detect the Blender Swap Tool first.")
            return
        mesh_transfer_path = self.find_mesh_data_transfer_tool()
        if not os.path.isfile(os.path.join(mesh_transfer_path, "__init__.py")):
            messagebox.showinfo("Character Mod Tool", "Install or enable Mesh Data Transfer in Blender first.")
            return

        module_name = os.path.basename(os.path.normpath(tool_path))
        if not module_name.isidentifier():
            messagebox.showerror("Character Mod Tool", "The Blender Swap Tool folder name cannot be loaded as an add-on.")
            return
        tool_parent = os.path.dirname(os.path.normpath(tool_path))
        mesh_transfer_parent = os.path.dirname(os.path.normpath(mesh_transfer_path))
        expression = (
            "import sys, bpy; "
            f"sys.path.insert(0, {mesh_transfer_parent!r}); "
            "import mesh_data_transfer as mdt; mdt.register(); "
            f"sys.path.insert(0, {tool_parent!r}); "
            f"import {module_name} as addon; "
            "addon.register(); "
            "print('CHARMOD_BRIDGE_OK|' + '.'.join(map(str, addon.bl_info.get('version', (0, 0, 0)))) "
            "+ '|' + str(hasattr(bpy.types.Object, 'mesh_data_transfer_object')))"
        )
        self.full_swap_status_var.set("Starting Blender in the background...")
        self.update_idletasks()
        try:
            result = subprocess.run(
                [blender_path, "--background", "--factory-startup", "--python-expr", expression],
                capture_output=True,
                text=True,
                timeout=60,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            self.full_swap_bridge_result = "Failed"
            self.refresh_full_swap_status()
            self.refresh_body_swap_status()
            messagebox.showerror("Character Mod Tool", f"Could not start the Blender background link:\n{exc}")
            return

        output = "\n".join(part for part in (result.stdout, result.stderr) if part)
        match = re.search(r"CHARMOD_BRIDGE_OK\|([0-9.]+)\|True", output)
        if result.returncode == 0 and match:
            self.full_swap_bridge_result = f"Connected (v{match.group(1)})"
            self.refresh_full_swap_status()
            self.refresh_body_swap_status()
            self.full_swap_status_var.set(
                f"Background Blender connected to NBA Character Head Swap v{match.group(1)}."
            )
            return

        self.full_swap_bridge_result = "Failed"
        self.refresh_full_swap_status()
        self.refresh_body_swap_status()
        useful_lines = [line.strip() for line in output.splitlines() if line.strip()]
        detail = useful_lines[-1] if useful_lines else f"Blender exited with code {result.returncode}."
        messagebox.showerror("Character Mod Tool", f"Blender started but could not load the swap tool.\n\n{detail}")

    def test_body_swap_bridge(self):
        self.test_full_swap_bridge()
        self.refresh_body_swap_status()
        if self.full_swap_bridge_result.startswith("Connected"):
            self.body_swap_status_var.set(
                f"Background Blender {self.full_swap_bridge_result.lower()} for Body Swap."
            )

    def run_body_swap(self):
        self.run_full_swap(body_only=True)

    def run_full_swap(self, combined=False, body_only=False):
        if self.full_swap_process is not None:
            return
        if combined:
            try:
                source_path = self.resolve_everything_swap_source()
            except (OSError, ValueError, zipfile.BadZipFile) as exc:
                messagebox.showerror("Character Mod Tool", f"Could not open source package.\n\n{exc}")
                return
            target_path = self.everything_swap_target_var.get().strip()
            shrinkwrap_body = self.everything_swap_shrinkwrap_var.get()
            self.full_swap_source_var.set(source_path)
            self.full_swap_target_var.set(target_path)
        elif body_only:
            source_path = self.body_swap_source_var.get().strip()
            target_path = self.body_swap_target_var.get().strip()
            shrinkwrap_body = True
        else:
            source_path = self.full_swap_source_var.get().strip()
            target_path = self.full_swap_target_var.get().strip()
            shrinkwrap_body = False
        blender_path = self.full_swap_blender_var.get().strip()
        tool_path = self.full_swap_tool_var.get().strip()
        mesh_transfer_path = self.find_mesh_data_transfer_tool()
        source_ready = self.inspect_full_swap_iff(source_path)[0]
        target_ready = self.inspect_full_swap_iff(target_path)[0]
        if not source_ready or not target_ready:
            messagebox.showinfo("Character Mod Tool", "Choose valid source and target character IFFs first.")
            return
        if os.path.abspath(source_path) == os.path.abspath(target_path):
            messagebox.showinfo("Character Mod Tool", "Source and target must be different character IFFs.")
            return
        if not self.full_swap_bridge_result.startswith("Connected"):
            messagebox.showinfo("Character Mod Tool", "Run Test Background Link first.")
            return
        if not all(
            (
                os.path.isfile(blender_path),
                os.path.isfile(os.path.join(tool_path, "__init__.py")),
                os.path.isfile(os.path.join(mesh_transfer_path, "__init__.py")),
                os.path.isfile(FULL_SWAP_BRIDGE),
            )
        ):
            messagebox.showinfo("Character Mod Tool", "The Blender swap tools are incomplete.")
            return

        hair_enabled = combined and self.everything_swap_hair_enabled_var.get()
        hair_source = ""
        hair_target_key = ""
        if hair_enabled:
            hair_source = self.everything_swap_hair_source_var.get().strip()
            hair_target_key = self.everything_swap_hair_slot_map.get(
                self.everything_swap_hair_slot_var.get(),
                "",
            )
            if not hair_source or not os.path.isfile(hair_source):
                messagebox.showinfo(
                    "Character Mod Tool",
                    "Choose a valid source hair IFF or turn off Include Hair Swap.",
                )
                return
            if not hair_target_key:
                messagebox.showinfo(
                    "Character Mod Tool",
                    "Choose a target hair appearance slot or turn off Include Hair Swap.",
                )
                return
            try:
                backend = self.load_hair_backend()
                backend.external_hair_identity(backend.Path(hair_source))
            except Exception as exc:
                messagebox.showerror("Unsupported Hair", str(exc))
                return

        target_base = os.path.splitext(os.path.basename(target_path))[0]
        rename_name = ""
        if combined:
            export_name = self.normalized_png_export_name(target_base)
            if not export_name:
                messagebox.showerror(
                    "Character Mod Tool",
                    "The target character filename must be a PNG number such as png1335.iff.",
                )
                return
            if self.everything_swap_rename_enabled_var.get():
                rename_name = self.normalized_png_export_name(
                    self.everything_swap_rename_name_var.get()
                )
                if not rename_name:
                    messagebox.showerror(
                        "Character Mod Tool",
                        "Enter a new PNG ID such as 1335 or png1335, or turn off Rename Character Package.",
                    )
                    return
                if self.character_number_from_path(rename_name) == self.character_number_from_path(export_name):
                    messagebox.showerror(
                        "Character Mod Tool",
                        "The renamed PNG ID must be different from the target character number.",
                    )
                    return
            output_dir = app_settings.ensure_output_dir(self.settings.get("output_dir", ""))
            output_path = os.path.join(output_dir, export_name + ".iff") if output_dir else ""
        elif body_only:
            output_path = filedialog.asksaveasfilename(
                title="Save completed Body Swap IFF",
                defaultextension=".iff",
                initialfile=f"{target_base}_body_swap.iff",
                initialdir=app_settings.ensure_output_dir(self.settings.get("output_dir", "")),
                filetypes=[("NBA 2K character IFF", "*.iff"), ("All files", "*.*")],
            )
        else:
            output_path = filedialog.asksaveasfilename(
                title="Save completed Head Swap IFF",
                defaultextension=".iff",
                initialfile=f"{target_base}_head_swap.iff",
                initialdir=app_settings.ensure_output_dir(self.settings.get("output_dir", "")),
                filetypes=[("NBA 2K character IFF", "*.iff"), ("All files", "*.*")],
            )
        if not output_path:
            return
        output_abs = os.path.abspath(output_path)
        if output_abs in (os.path.abspath(source_path), os.path.abspath(target_path)):
            messagebox.showerror(
                "Character Mod Tool",
                "The swap must save to a new IFF. Choose a different output filename.",
            )
            return
        if combined:
            config_outputs = self.planned_config_outputs(target_path, export_name, os.path.dirname(output_abs))
            companion_plan = self.full_swap_companion_plan(target_path, export_name, os.path.dirname(output_abs))
            if not self.confirm_full_swap_output_plan(
                output_abs,
                config_outputs,
                companion_plan,
                rename_name=rename_name,
            ):
                return

        if combined:
            self.last_full_swap_output = ""
            self.everything_swap_open_blender_button.configure(state=tk.DISABLED)
            stage_dir = tempfile.mkdtemp(
                prefix=".character_mod_full_swap_",
                dir=os.path.dirname(output_abs),
            )
            stage_output = os.path.join(stage_dir, os.path.basename(output_abs))
            self.everything_swap_active = True
            self.everything_swap_source_path = source_path
            self.everything_swap_target_path = target_path
            self.everything_swap_final_output = output_abs
            self.everything_swap_rename_name = rename_name
            self.everything_swap_stage_dir = stage_dir
            self.everything_swap_include_appearance = shrinkwrap_body
            self.everything_swap_companion_plan = companion_plan
            self.everything_swap_hair_source_path = hair_source
            self.everything_swap_hair_target_key = hair_target_key
            self.everything_swap_hair_result = None
            self.full_swap_output_path = stage_output
        else:
            self.everything_swap_active = False
            self.everything_swap_stage_dir = ""
            self.everything_swap_companion_plan = []
            self.full_swap_output_path = output_abs
        self.body_swap_active = body_only
        self.full_swap_error = ""
        self.full_swap_success = None
        self.full_swap_cancel_requested = False
        self.full_swap_log_tail = []
        self.full_swap_queue = queue.Queue()
        self.full_swap_progress_var = tk.StringVar(value="Starting background Blender...")
        progress_window = tk.Toplevel(self)
        self.full_swap_progress_window = progress_window
        progress_window.title(
            "Full Swap" if combined else ("Body Swap" if body_only else "Head Swap")
        )
        progress_window.resizable(False, False)
        progress_window.transient(self)
        progress_window.grab_set()
        ttk.Label(progress_window, textvariable=self.full_swap_progress_var, width=64).pack(
            fill=tk.X, padx=14, pady=(14, 8)
        )
        progress = ttk.Progressbar(progress_window, mode="indeterminate", length=500)
        progress.pack(fill=tk.X, padx=14)
        progress.start(12)
        ttk.Button(progress_window, text="Cancel", command=self.cancel_full_swap).pack(pady=14)
        progress_window.protocol("WM_DELETE_WINDOW", self.cancel_full_swap)

        command = [
            blender_path,
            "--background",
            "--factory-startup",
            "--python",
            FULL_SWAP_BRIDGE,
            "--",
            "--source",
            source_path,
            "--target",
            target_path,
            "--output",
            self.full_swap_output_path,
            "--addon",
            tool_path,
            "--mesh-data-transfer",
            mesh_transfer_path,
            "--shrinkwrap-body",
            "yes" if shrinkwrap_body else "no",
            "--transfer-mode",
            "full" if combined else ("body" if body_only else "head"),
        ]
        self.full_swap_status_var.set(
            "Full Swap is running Blender head/body work..." if combined
            else ("Body Swap is running in background Blender..."
                  if body_only else "Head Swap is running in background Blender...")
        )
        if body_only:
            self.body_swap_status_var.set("Body Swap is running in background Blender...")
        self.full_swap_run_button.configure(state=tk.DISABLED)
        self.body_swap_run_button.configure(state=tk.DISABLED)
        if combined:
            self.everything_swap_run_button.configure(state=tk.DISABLED)
        worker = threading.Thread(target=self._full_swap_worker, args=(command,), daemon=True)
        worker.start()
        self.after(100, self._poll_full_swap_queue)

    def _full_swap_worker(self, command):
        try:
            LOGGER.info("Starting Blender swap command: %s", subprocess.list2cmdline(command))
            process = subprocess.Popen(
                command,
                cwd=os.path.dirname(FULL_SWAP_BRIDGE),
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                errors="replace",
                bufsize=1,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
            )
            self.full_swap_process = process
            if self.full_swap_cancel_requested and process.poll() is None:
                process.terminate()
            if process.stdout is not None:
                for line in process.stdout:
                    clean_line = line.rstrip()
                    LOGGER.info("BLENDER | %s", clean_line)
                    self.full_swap_queue.put(("line", clean_line))
            return_code = process.wait()
            LOGGER.info("Blender swap exited with code %s", return_code)
            self.full_swap_queue.put(("done", return_code))
        except Exception as exc:
            LOGGER.exception("Blender swap worker failed")
            self.full_swap_queue.put(("worker_error", str(exc)))

    def _poll_full_swap_queue(self):
        if self.full_swap_queue is None:
            return
        finished = False
        return_code = 1
        try:
            while True:
                event = self.full_swap_queue.get_nowait()
                if event[0] == "line":
                    line = event[1]
                    if line.startswith("CHARMOD_STAGE|"):
                        stage = line.split("|", 1)[1]
                        self.full_swap_progress_var.set(stage)
                        if self.body_swap_active:
                            self.body_swap_status_var.set(stage)
                        else:
                            self.full_swap_status_var.set(stage)
                    elif line.startswith("CHARMOD_ERROR|"):
                        self.full_swap_error = line.split("|", 1)[1]
                    elif line.startswith("CHARMOD_SUCCESS|"):
                        try:
                            self.full_swap_success = json.loads(line.split("|", 1)[1])
                        except json.JSONDecodeError:
                            self.full_swap_success = None
                    elif line.strip():
                        self.full_swap_log_tail.append(line.strip())
                        self.full_swap_log_tail = self.full_swap_log_tail[-12:]
                elif event[0] == "done":
                    finished = True
                    return_code = event[1]
                elif event[0] == "worker_error":
                    finished = True
                    self.full_swap_error = event[1]
        except queue.Empty:
            pass

        if finished:
            self._finish_full_swap(return_code)
        else:
            self.after(100, self._poll_full_swap_queue)

    def build_combined_face_config_archives(self, source_character, target_character):
        source_face = self.matching_source_face_texture_path(source_character)
        if not source_face:
            raise ValueError(
                f"No face texture IFF or usable source config was found for {os.path.basename(source_character)}."
            )
        options = self.face_config_options_for_character(target_character)
        target_paths = []
        seen = set()
        for path in options.values():
            key = os.path.abspath(path).lower()
            if key not in seen:
                seen.add(key)
                target_paths.append(path)
        if not target_paths:
            raise ValueError("The target character has no face configurations to rebuild.")

        config_signatures = self.face_config_texture_signatures(target_character)
        backend = self.load_hair_backend()
        manifest = backend.parse_manifest_entries()

        def manifest_has_config(path):
            return f"char/sig/{os.path.basename(path)}".lower() in manifest

        def config_suffix(path):
            match = re.search(r"(?i)_config_(.+)\.iff$", os.path.basename(path))
            return match.group(1).lower() if match else ""

        expanded_cache = {}

        def expanded_entries(path):
            key = os.path.abspath(path).lower()
            if key not in expanded_cache:
                expanded_cache[key] = self.archived_face_config_entries(path)
            return dict(expanded_cache[key])

        def compatible_template(path):
            wanted = config_signatures.get(config_suffix(path))
            candidates = [candidate for candidate in target_paths if manifest_has_config(candidate)]
            if wanted is not None:
                exact = [
                    candidate for candidate in candidates
                    if config_signatures.get(config_suffix(candidate)) == wanted
                ]
                if exact:
                    return exact[0]
            return ""

        with zipfile.ZipFile(source_face, "r") as source_archive:
            source_names = source_archive.namelist()
            source_pairs = {}
            for logical_name in ("face_color_o", "face_normal_o"):
                txtr_name, dds_name = self.archive_texture_pair(source_names, logical_name)
                if not txtr_name or not dds_name:
                    raise ValueError(f"{os.path.basename(source_face)} is missing {logical_name} TXTR/DDS.")
                source_pairs[logical_name] = (
                    source_archive.read(txtr_name),
                    source_archive.read(dds_name),
                )

        outputs = {}
        for target_config in target_paths:
            if manifest_has_config(target_config):
                base_entries = expanded_entries(target_config)
            elif os.path.isfile(target_config) and zipfile.is_zipfile(target_config):
                with zipfile.ZipFile(target_config, "r") as loose_archive:
                    loose_names = loose_archive.namelist()
                    if not any(name.lower().endswith(".dds") for name in loose_names):
                        raise ValueError(
                            f"{os.path.basename(target_config)} is not in the manifest and its loose copy has no DDS textures."
                        )
                    base_entries = {
                        info.filename: loose_archive.read(info.filename)
                        for info in loose_archive.infolist()
                        if not info.is_dir()
                    }
            else:
                template = compatible_template(target_config)
                if not template:
                    raise ValueError(
                        f"{os.path.basename(target_config)} is missing from the manifest and no compatible "
                        "manifest-backed face configuration was found."
                    )
                base_entries = expanded_entries(template)
            base_names = list(base_entries)
            for logical_name, (source_txtr_data, source_dds_data) in source_pairs.items():
                target_txtr, target_dds = self.archive_texture_pair(base_names, logical_name)
                if not target_txtr or not target_dds:
                    raise ValueError(
                        f"Original {os.path.basename(target_config)} is missing an expanded {logical_name} pair."
                    )
                final_txtr, final_dds, _resized = self.prepare_texture_pair_for_target(
                    source_txtr_data,
                    source_dds_data,
                    base_entries[target_txtr],
                    base_entries[target_dds],
                    target_dds,
                    preserve_source_profile=True,
                )
                base_entries[target_txtr] = final_txtr
                base_entries[target_dds] = final_dds

            base_entries = self.materialize_embedded_texture_txtrs(base_entries)
            buffer = io.BytesIO()
            with zipfile.ZipFile(buffer, "w", compression=zipfile.ZIP_DEFLATED) as output:
                for name, data in base_entries.items():
                    info = zipfile.ZipInfo(name)
                    info.compress_type = zipfile.ZIP_DEFLATED
                    info.create_system = 3
                    info.external_attr = 0x81B60000
                    output.writestr(info, data)
            outputs[os.path.basename(target_config)] = buffer.getvalue()
        return source_face, outputs

    @staticmethod
    def face_config_texture_signatures(character_path):
        try:
            with zipfile.ZipFile(character_path, "r") as archive:
                appearance_name = next((
                    name for name in archive.namelist()
                    if os.path.basename(name).lower() in {item.lower() for item in APPEARANCE_ENTRY_NAMES}
                ), "")
                if not appearance_name:
                    return {}
                appearance, _wrapped, _error = try_parse_structured_text(
                    appearance_name,
                    archive.read(appearance_name),
                )
        except (OSError, zipfile.BadZipFile):
            return {}
        accessory = appearance.get("accessory_items") if isinstance(appearance, dict) else None
        configurations = accessory.get("configurations", []) if isinstance(accessory, dict) else []
        signatures = {}
        for config in configurations:
            if not isinstance(config, dict) or not config.get("name"):
                continue
            suffix = re.sub(r"[^A-Za-z0-9_-]+", "_", str(config["name"])).strip("_").lower()
            facetexture = config.get("facetexture")
            if isinstance(facetexture, dict):
                signatures[suffix] = tuple(sorted((str(key), str(value)) for key, value in facetexture.items()))
        return signatures

    def apply_combined_tattoos(self, source_character, target_iff):
        with zipfile.ZipFile(source_character, "r") as source_archive:
            source_names = source_archive.namelist()
            source_data = {name: source_archive.read(name) for name in source_names}
        with zipfile.ZipFile(target_iff, "r") as target_archive:
            target_infos = target_archive.infolist()
            target_names = [info.filename for info in target_infos if not info.is_dir()]
            target_data = {
                info.filename: target_archive.read(info.filename)
                for info in target_infos
                if not info.is_dir()
            }

        replacements = {}
        removals = set()
        actions = []
        for logical_name in ("chest_color_o", "legs_color_o"):
            source_txtr, source_dds = self.archive_texture_pair(source_names, logical_name)
            target_txtr, target_dds = self.archive_texture_pair(target_names, logical_name)
            if not source_txtr and not source_dds:
                matches = {
                    name for name in target_names
                    if self.is_saved_tattoo_texture(name) and self.texture_logical_name(name) == logical_name
                }
                removals.update(matches)
                if matches:
                    actions.append(f"removed {logical_name}")
                continue
            if not source_txtr or not source_dds:
                raise ValueError(f"Source {logical_name} tattoo pair is incomplete.")
            if not target_txtr or not target_dds:
                actions.append(f"target has no {logical_name} slot")
                continue
            final_txtr, final_dds, resized = self.prepare_texture_pair_for_target(
                source_data[source_txtr],
                source_data[source_dds],
                target_data[target_txtr],
                target_data[target_dds],
                target_dds,
                preserve_source_profile=True,
            )
            replacements[target_txtr] = final_txtr
            replacements[target_dds] = final_dds
            actions.append(
                f"replaced {logical_name}" + (" (resized to target)" if resized else "")
            )

        temp_handle = tempfile.NamedTemporaryFile(
            prefix=os.path.basename(target_iff) + ".tattoos.",
            suffix=".tmp",
            dir=os.path.dirname(target_iff),
            delete=False,
        )
        temp_path = temp_handle.name
        temp_handle.close()
        try:
            with zipfile.ZipFile(target_iff, "r") as source, zipfile.ZipFile(temp_path, "w") as output:
                for info in source.infolist():
                    if info.is_dir():
                        output.writestr(self.copied_zip_info(info), b"")
                        continue
                    if info.filename in removals:
                        continue
                    data = replacements.get(info.filename, source.read(info.filename))
                    output.writestr(self.copied_zip_info(info), data)
            os.replace(temp_path, target_iff)
        except Exception:
            try:
                os.remove(temp_path)
            except OSError:
                pass
            raise
        return actions

    def apply_combined_appearance(self, source_character, target_iff):
        _source_name, source_appearance = self.read_appearance_from_archive(source_character)
        with zipfile.ZipFile(target_iff, "r") as target_archive:
            target_infos = target_archive.infolist()
            appearance_name = next((
                info.filename for info in target_infos
                if not info.is_dir()
                and os.path.basename(info.filename).lower() in {item.lower() for item in APPEARANCE_ENTRY_NAMES}
            ), "")
            if not appearance_name:
                raise ValueError(f"{os.path.basename(target_iff)} does not contain appearance_info.")
            original_data = target_archive.read(appearance_name)

        target_appearance, _wrapped, error = try_parse_structured_text(appearance_name, original_data)
        if not isinstance(target_appearance, dict):
            raise ValueError(error or f"{appearance_name} could not be parsed.")
        merged, appearance_matches, body_fit_matches = self.merge_same_named_appearance_fields(
            source_appearance,
            target_appearance,
        )
        if not appearance_matches and not body_fit_matches:
            raise ValueError("No same-name appearance or body-fit fields were found for Full Swap.")
        replacement_data = serialize_structured_entry(
            appearance_name,
            original_data,
            merged,
            False,
        )

        temp_handle = tempfile.NamedTemporaryFile(
            prefix=os.path.basename(target_iff) + ".appearance.",
            suffix=".tmp",
            dir=os.path.dirname(target_iff),
            delete=False,
        )
        temp_path = temp_handle.name
        temp_handle.close()
        try:
            with zipfile.ZipFile(target_iff, "r") as source, zipfile.ZipFile(temp_path, "w") as output:
                for info in source.infolist():
                    data = b"" if info.is_dir() else source.read(info.filename)
                    if info.filename == appearance_name:
                        data = replacement_data
                    output.writestr(self.copied_zip_info(info), data)
            os.replace(temp_path, target_iff)
        except Exception:
            try:
                os.remove(temp_path)
            except OSError:
                pass
            raise
        return {
            "entry": appearance_name,
            "appearance_fields": len(appearance_matches),
            "body_fit_fields": len(body_fit_matches),
        }

    @staticmethod
    def atomic_write_bytes(path, data):
        folder = os.path.dirname(path)
        if folder:
            os.makedirs(folder, exist_ok=True)
        handle = tempfile.NamedTemporaryFile(
            prefix=os.path.basename(path) + ".",
            suffix=".tmp",
            dir=folder or None,
            delete=False,
        )
        temp_path = handle.name
        try:
            handle.write(data)
            handle.close()
            os.replace(temp_path, path)
        except Exception:
            try:
                handle.close()
            except OSError:
                pass
            try:
                os.remove(temp_path)
            except OSError:
                pass
            raise

    def commit_staged_outputs(self, staged_outputs, work_dir=None):
        backup_dir = os.path.join(work_dir or self.everything_swap_stage_dir, "rollback")
        os.makedirs(backup_dir, exist_ok=True)
        backups = {}
        committed = []
        try:
            for index, (staged, destination) in enumerate(staged_outputs):
                os.makedirs(os.path.dirname(destination), exist_ok=True)
                if os.path.exists(destination):
                    backup = os.path.join(backup_dir, f"{index:04d}_{os.path.basename(destination)}")
                    shutil.copy2(destination, backup)
                    backups[destination] = backup
                os.replace(staged, destination)
                committed.append(destination)
        except Exception:
            LOGGER.exception("Output package commit failed; restoring previous files")
            for destination in reversed(committed):
                backup = backups.get(destination)
                try:
                    if backup and os.path.exists(backup):
                        os.replace(backup, destination)
                    elif os.path.exists(destination):
                        os.remove(destination)
                except OSError:
                    LOGGER.exception("Could not roll back %s", destination)
            raise

    def stage_export_companions(self):
        staged = []
        package_dir = os.path.join(self.everything_swap_stage_dir, "companions")
        os.makedirs(package_dir, exist_ok=True)
        for index, (source, destination) in enumerate(self.everything_swap_companion_plan):
            staged_path = os.path.join(package_dir, f"{index:04d}_{os.path.basename(destination)}")
            shutil.copy2(source, staged_path)
            if not zipfile.is_zipfile(staged_path):
                raise ValueError(f"Companion archive is not a readable IFF: {os.path.basename(source)}")
            staged.append((staged_path, destination))
        return staged

    def stage_everything_swap_hair(self, staged_companions):
        source_path = self.everything_swap_hair_source_path
        target_key = self.everything_swap_hair_target_key
        if not source_path or not target_key:
            return None

        target_png = self.character_number_from_path(self.everything_swap_final_output)
        if not target_png:
            raise ValueError("Could not determine the Full Swap target PNG for Hair Swap.")
        output_name = f"png{target_png}_geo_{target_key}.iff"
        destination = os.path.join(
            os.path.dirname(self.everything_swap_final_output),
            output_name,
        )
        template_path = None
        replace_index = None
        for index, (staged, planned_destination) in enumerate(staged_companions):
            if os.path.abspath(planned_destination).lower() == os.path.abspath(destination).lower():
                template_path = staged
                replace_index = index
                break

        hair_dir = os.path.join(self.everything_swap_stage_dir, "hair")
        os.makedirs(hair_dir, exist_ok=True)
        converted_path = os.path.join(hair_dir, output_name)
        backend = self.load_hair_backend()
        output, result, plan = backend.convert_external_hair_to_output(
            backend.Path(source_path),
            target_png,
            target_key,
            backend.Path(converted_path),
            backend.Path(template_path) if template_path else None,
        )
        staged_hair = (str(output), destination)
        if replace_index is None:
            staged_companions.append(staged_hair)
        else:
            staged_companions[replace_index] = staged_hair

        details = {
            "generation": plan["generation"],
            "source": str(plan["source_path"]),
            "source_key": plan["source_hair_key"],
            "target_key": target_key,
            "output": destination,
            "vertices": int(result.get("vertices", result.get("stream_vertices", 0))),
        }
        self.everything_swap_hair_result = details
        return details

    def complete_everything_swap(self):
        appearance = None
        if self.everything_swap_include_appearance:
            appearance = self.apply_combined_appearance(
                self.everything_swap_source_path,
                self.full_swap_output_path,
            )
        source_face, config_archives = self.build_combined_face_config_archives(
            self.everything_swap_source_path,
            self.everything_swap_target_path,
        )
        tattoo_actions = self.apply_combined_tattoos(
            self.everything_swap_source_path,
            self.full_swap_output_path,
        )
        output_dir = os.path.dirname(self.everything_swap_final_output)
        export_prefix = os.path.splitext(os.path.basename(self.everything_swap_final_output))[0]
        config_paths = []
        staged_outputs = []
        package_dir = os.path.join(self.everything_swap_stage_dir, "configs")
        os.makedirs(package_dir, exist_ok=True)
        for filename, data in config_archives.items():
            output_filename = re.sub(r"(?i)^png\d+", export_prefix, filename, count=1)
            path = os.path.join(output_dir, output_filename)
            staged_path = os.path.join(package_dir, output_filename)
            self.atomic_write_bytes(staged_path, data)
            staged_outputs.append((staged_path, path))
            config_paths.append(path)
        staged_companions = self.stage_export_companions()
        hair_result = self.stage_everything_swap_hair(staged_companions)
        validation = self.validate_everything_swap_outputs(
            self.full_swap_output_path,
            config_archives,
            staged_companions,
        )
        staged_outputs.extend(staged_companions)
        staged_outputs.append((self.full_swap_output_path, self.everything_swap_final_output))
        self.commit_staged_outputs(staged_outputs)
        companion_paths = [destination for _source, destination in self.everything_swap_companion_plan]
        if hair_result and hair_result["output"] not in companion_paths:
            companion_paths.append(hair_result["output"])
        return (
            source_face,
            config_paths,
            companion_paths,
            tattoo_actions,
            appearance,
            hair_result,
            validation,
        )

    @staticmethod
    def validate_archive_file(path, allow_shared_textures=False, shared_texture_names=()):
        with zipfile.ZipFile(path, "r") as archive:
            names = [info.filename for info in archive.infolist() if not info.is_dir()]
            data_by_name = {name: archive.read(name) for name in names}
        return validate_archive_snapshot(
            names,
            data_by_name,
            allow_shared_textures=allow_shared_textures,
            shared_texture_names=shared_texture_names,
        )

    @staticmethod
    def archive_shared_texture_names(path):
        with zipfile.ZipFile(path, "r") as archive:
            names = [info.filename for info in archive.infolist() if not info.is_dir()]
        txtr_names = [name for name in names if name.lower().endswith(".txtr")]
        dds_names = [name for name in names if name.lower().endswith(".dds")]
        shared = set()
        for txtr_name in txtr_names:
            logical = os.path.splitext(os.path.basename(txtr_name))[0].lower()
            if not any(
                os.path.basename(name).lower().startswith(logical + ".")
                or os.path.basename(name).lower() == logical + ".dds"
                for name in dds_names
            ):
                shared.add(logical)
        return shared

    @staticmethod
    def validate_archive_bytes(data):
        with zipfile.ZipFile(io.BytesIO(data), "r") as archive:
            names = [info.filename for info in archive.infolist() if not info.is_dir()]
            data_by_name = {name: archive.read(name) for name in names}
        return validate_archive_snapshot(names, data_by_name)

    def validate_everything_swap_outputs(self, player_path, config_archives, companion_stages=()):
        main_shared_textures = set()
        if (
            self.everything_swap_manifest_target_path
            and os.path.abspath(self.everything_swap_target_path)
            == os.path.abspath(self.everything_swap_manifest_target_path)
        ):
            main_shared_textures = self.archive_shared_texture_names(self.everything_swap_manifest_target_path)
        grouped = [
            (
                os.path.basename(self.everything_swap_final_output),
                self.validate_archive_file(player_path, shared_texture_names=main_shared_textures),
            )
        ]
        grouped.extend(
            (filename, self.validate_archive_bytes(data))
            for filename, data in config_archives.items()
        )
        manifest_companion_names = {
            os.path.basename(archive_entry).lower()
            for _source, archive_entry in self.everything_swap_manifest_companions
        }
        grouped.extend(
            (
                os.path.basename(destination),
                self.validate_archive_file(
                    staged,
                    allow_shared_textures=os.path.basename(destination).lower() in manifest_companion_names,
                ),
            )
            for staged, destination in companion_stages
        )
        errors = [
            f"{filename}: {result.check} - {result.details}"
            for filename, results in grouped
            for result in results
            if result.severity == "ERROR"
        ]
        warnings = sum(
            result.severity == "WARNING"
            for _filename, results in grouped
            for result in results
        )
        passes = sum(
            result.severity == "PASS"
            for _filename, results in grouped
            for result in results
        )
        if errors:
            preview = "\n".join(errors[:8])
            if len(errors) > 8:
                preview += f"\n...and {len(errors) - 8} more error(s)."
            raise ValueError(f"Game-ready validation found {len(errors)} error(s):\n{preview}")
        report = {
            "archives": len(grouped),
            "errors": 0,
            "warnings": warnings,
            "passes": passes,
            "results": [
                {
                    "archive": filename,
                    "severity": result.severity,
                    "check": result.check,
                    "details": result.details,
                }
                for filename, results in grouped
                for result in results
            ],
        }
        self.last_validation_report = report
        LOGGER.info(
            "Full Swap validation: %s archives, %s warnings, %s passed checks",
            report["archives"],
            report["warnings"],
            report["passes"],
        )
        return report

    def reset_everything_swap_run(self, remove_stage=False):
        stage_dir = self.everything_swap_stage_dir
        stage_path = self.full_swap_output_path if self.everything_swap_active else ""
        if remove_stage and stage_path and os.path.isfile(stage_path):
            try:
                os.remove(stage_path)
            except OSError:
                pass
        self.everything_swap_active = False
        self.everything_swap_source_path = ""
        self.everything_swap_target_path = ""
        self.everything_swap_final_output = ""
        self.everything_swap_rename_name = ""
        self.everything_swap_stage_dir = ""
        self.everything_swap_include_appearance = False
        self.everything_swap_hair_source_path = ""
        self.everything_swap_hair_target_key = ""
        self.everything_swap_hair_result = None
        self.everything_swap_run_button.configure(state=tk.NORMAL)
        self.schedule_directory_cleanup(stage_dir)

    @staticmethod
    def remove_directory_with_retries(path, attempts=20):
        if not path:
            return True

        def clear_read_only(function, item_path, _error_info):
            try:
                os.chmod(item_path, stat.S_IWRITE)
                function(item_path)
            except OSError:
                pass

        for attempt in range(attempts):
            if not os.path.exists(path):
                return True
            try:
                shutil.rmtree(path, onerror=clear_read_only)
            except OSError:
                pass
            if not os.path.exists(path):
                return True
            time.sleep(min(0.15 * (attempt + 1), 1.0))
        LOGGER.warning("Could not remove temporary staging folder after retries: %s", path)
        return False

    def schedule_directory_cleanup(self, path):
        if not path or not os.path.exists(path):
            return
        if self.remove_directory_with_retries(path, attempts=1):
            return
        threading.Thread(
            target=self.remove_directory_with_retries,
            args=(path,),
            daemon=True,
            name="CharacterModToolCleanup",
        ).start()

    def cleanup_stale_output_staging(self):
        output_dir = os.path.abspath(app_settings.ensure_output_dir(self.settings.get("output_dir", "")))
        if not os.path.isdir(output_dir):
            return
        cutoff = time.time() - 3600
        try:
            entries = list(os.scandir(output_dir))
        except OSError:
            return
        for entry in entries:
            if not entry.name.startswith((".character_mod_full_swap_", ".character_mod_rename_")) or not entry.is_dir():
                continue
            try:
                if entry.stat().st_mtime > cutoff:
                    continue
                candidate = os.path.abspath(entry.path)
                if os.path.commonpath((output_dir, candidate)) != output_dir:
                    continue
            except (OSError, ValueError):
                continue
            self.schedule_directory_cleanup(candidate)

    def _finish_full_swap(self, return_code):
        combined = self.everything_swap_active
        body_only = self.body_swap_active
        progress_window = self.full_swap_progress_window
        if progress_window is not None and progress_window.winfo_exists():
            try:
                progress_window.grab_release()
            except tk.TclError:
                pass
            progress_window.destroy()
        self.full_swap_progress_window = None
        self.full_swap_process = None
        self.full_swap_queue = None
        output_valid = os.path.isfile(self.full_swap_output_path) and zipfile.is_zipfile(self.full_swap_output_path)
        if return_code == 0 and self.full_swap_success and output_valid:
            version = self.full_swap_success.get("addon_version", "")
            if combined:
                try:
                    hair_suffix = " and hair" if self.everything_swap_hair_source_path else ""
                    if self.everything_swap_include_appearance:
                        post_blender_status = (
                            f"Swapping appearance, face textures, tattoos{hair_suffix}..."
                        )
                    else:
                        post_blender_status = (
                            f"Creating face textures and applying tattoos{hair_suffix}..."
                        )
                    self.full_swap_progress_var.set(post_blender_status)
                    self.full_swap_status_var.set("Full Swap is " + post_blender_status.lower())
                    self.update_idletasks()
                    (
                        source_face,
                        config_paths,
                        companion_paths,
                        tattoo_actions,
                        appearance,
                        hair_result,
                        validation,
                    ) = self.complete_everything_swap()
                except Exception as exc:
                    LOGGER.exception("Full Swap post-processing failed")
                    self.reset_everything_swap_run(remove_stage=True)
                    self.refresh_full_swap_status()
                    self.refresh_body_swap_status()
                    self.full_swap_status_var.set("Full Swap did not complete.")
                    messagebox.showerror(
                        "Character Mod Tool",
                        "Full Swap failed after Blender. No final player IFF was created.\n\n" + str(exc),
                    )
                    return
                final_output = self.everything_swap_final_output
                rename_report = None
                rename_name = self.everything_swap_rename_name
                if rename_name:
                    self.full_swap_progress_var.set(f"Renaming character package to {rename_name}...")
                    self.full_swap_status_var.set(
                        f"Full Swap is renaming the completed package to {rename_name}."
                    )
                    self.update_idletasks()
                    original_outputs = [
                        final_output,
                        *config_paths,
                        *companion_paths,
                    ]
                    try:
                        rename_report = self.execute_character_package_rename(
                            final_output,
                            rename_name,
                            os.path.dirname(final_output),
                            delete_original=True,
                            source_paths=original_outputs,
                        )
                    except Exception as exc:
                        LOGGER.exception("Full Swap automatic package rename failed")
                        self.reset_everything_swap_run()
                        self.refresh_full_swap_status()
                        self.refresh_body_swap_status()
                        self.last_full_swap_output = final_output
                        self.everything_swap_open_blender_button.configure(state=tk.NORMAL)
                        self.refresh_recent_output_choices(select_path=final_output)
                        self.full_swap_status_var.set(
                            "Full Swap completed, but the automatic package rename did not."
                        )
                        messagebox.showerror(
                            "Character Mod Tool",
                            "Full Swap completed under the target PNG number, but the automatic "
                            f"rename to {rename_name} did not complete.\n\n"
                            f"Original output:\n{final_output}\n\n{exc}",
                        )
                        return
                    path_map = rename_report["path_map"]
                    final_output = rename_report["new_main"]
                    config_paths = [
                        path_map.get(os.path.abspath(path).lower(), path)
                        for path in config_paths
                    ]
                    companion_paths = [
                        path_map.get(os.path.abspath(path).lower(), path)
                        for path in companion_paths
                    ]
                config_text = "\n".join(config_paths)
                companion_text = "\n".join(companion_paths) if companion_paths else "No target companions were present."
                tattoo_text = ", ".join(tattoo_actions) if tattoo_actions else "No chest/legs tattoo changes needed."
                if appearance:
                    appearance_text = (
                        f"swapped {appearance['appearance_fields']} appearance fields and "
                        f"{appearance['body_fit_fields']} body-fit fields"
                    )
                else:
                    appearance_text = "preserved target appearance_info"
                if hair_result:
                    hair_text = (
                        f"{hair_result['generation']} {hair_result['source_key']} -> "
                        f"{hair_result['target_key']} ({hair_result['vertices']:,} vertices)"
                    )
                else:
                    hair_text = "preserved target hair"
                verdict = "GAME READY" if not validation["warnings"] else "CHECK WARNINGS"
                validation_text = (
                    f"{verdict}: {validation['archives']} archive(s), "
                    f"{validation['errors']} errors, {validation['warnings']} warnings, "
                    f"{validation['passes']} passed checks."
                )
                rename_text = ""
                if rename_report:
                    rename_text = (
                        f"\n\nPackage rename: png{rename_report['old_number']} -> "
                        f"png{rename_report['new_number']} "
                        f"({len(rename_report['outputs'])} files)."
                    )
                    if rename_report["deletion_error"]:
                        rename_text += "\nThe original package could not be deleted and was kept."
                self.reset_everything_swap_run()
                self.refresh_full_swap_status()
                self.refresh_body_swap_status()
                self.last_full_swap_output = final_output
                self.everything_swap_open_blender_button.configure(state=tk.NORMAL)
                self.refresh_recent_output_choices(select_path=final_output)
                self.full_swap_status_var.set(f"Full Swap completed. Validation: {verdict}.")
                messagebox.showinfo(
                    "Character Mod Tool",
                    "Full Swap completed successfully.\n\n"
                    f"Player IFF:\n{final_output}\n\n"
                    f"Face source:\n{source_face}\n\n"
                    f"Face configs:\n{config_text}\n\n"
                    f"Companion IFFs:\n{companion_text}\n\n"
                    f"Appearance: {appearance_text}\n\n"
                    f"Tattoos: {tattoo_text}\n\n"
                    f"Hair: {hair_text}\n\n{validation_text}{rename_text}",
                )
                return
            if body_only:
                self.body_swap_active = False
                self.refresh_full_swap_status()
                self.refresh_body_swap_status()
                self.set_body_swap_row(
                    "pipeline",
                    "Automated Body Swap",
                    "Complete",
                    self.full_swap_output_path,
                )
                self.body_swap_status_var.set(f"Body Swap completed with Blender Swap Tool v{version}.")
                messagebox.showinfo(
                    "Character Mod Tool",
                    f"Body Swap completed successfully.\n\nSaved:\n{self.full_swap_output_path}",
                )
            else:
                self.refresh_full_swap_status()
                self.refresh_body_swap_status()
                self.set_full_swap_row(
                    "pipeline",
                    "Automated Head Swap",
                    "Complete",
                    self.full_swap_output_path,
                )
                self.full_swap_status_var.set(f"Head Swap completed with Blender Swap Tool v{version}.")
                messagebox.showinfo(
                    "Character Mod Tool",
                    f"Head Swap completed successfully.\n\nSaved:\n{self.full_swap_output_path}",
                )
            return

        if combined:
            self.reset_everything_swap_run(remove_stage=True)
        self.refresh_full_swap_status()
        self.refresh_body_swap_status()
        detail = self.full_swap_error
        if not detail and self.full_swap_log_tail:
            detail = "Blender's final output was:\n" + "\n".join(self.full_swap_log_tail[-8:])
        if not detail:
            detail = "Background Blender stopped before creating the output IFF."
        label = "Full Swap" if combined else ("Body Swap" if body_only else "Head Swap")
        if body_only:
            self.body_swap_status_var.set(f"{label} did not complete.")
        else:
            self.full_swap_status_var.set(f"{label} did not complete.")
        self.body_swap_active = False
        LOGGER.error("%s did not complete: %s", label, detail)
        messagebox.showerror("Character Mod Tool", f"{label} did not complete.\n\n{detail}")

    def cancel_full_swap(self):
        self.full_swap_cancel_requested = True
        process = self.full_swap_process
        label = (
            "Full Swap"
            if self.everything_swap_active
            else ("Body Swap" if self.body_swap_active else "Head Swap")
        )
        self.full_swap_error = f"{label} was cancelled."
        if self.full_swap_progress_var is not None:
            self.full_swap_progress_var.set(f"Cancelling {label}...")
        if process is not None and process.poll() is None:
            try:
                process.terminate()
            except OSError:
                pass

    @staticmethod
    def inspect_headband_iff(path, source):
        if not path:
            return False, "Choose file", "No headband IFF selected"
        if not os.path.isfile(path):
            return False, "Missing", "File was not found"
        if not zipfile.is_zipfile(path):
            return False, "Unsupported", "Not a ZIP-style headband IFF"
        try:
            with zipfile.ZipFile(path, "r") as archive:
                names = [info.filename for info in archive.infolist() if not info.is_dir()]
        except (OSError, zipfile.BadZipFile) as exc:
            return False, "Could not read", str(exc)
        base_names = [os.path.basename(name).lower() for name in names]
        if "headband.scne" not in base_names:
            return False, "Wrong file", "headband.SCNE was not found"
        model_count = sum(name.endswith(".model") for name in base_names)
        vertex_buffers = sum(name.startswith("vertexbuffer") for name in base_names)
        index_buffers = sum(name.startswith("indexbuffer") for name in base_names)
        if source:
            if model_count:
                return True, "Ready", f"Legacy headband; {model_count} model file; {len(names)} entries"
            if vertex_buffers and index_buffers:
                return True, "Ready", (
                    f"NBA 2K25/modern headband; {vertex_buffers} vertex buffers; {len(names)} entries"
                )
            return False, "Wrong format", (
                "Source must contain either a legacy .model or modern vertex/index buffers"
            )
        if not vertex_buffers or not index_buffers:
            return False, "Wrong format", "Target must be a modern geo_headband IFF with model buffers"
        return True, "Ready", f"NBA 2K26 headband; {vertex_buffers} vertex buffers; {len(names)} entries"

    def set_headband_row(self, iid, label, status, details):
        values = (status, details)
        if self.headband_tree.exists(iid):
            self.headband_tree.item(iid, text=label, values=values)
        else:
            self.headband_tree.insert("", tk.END, iid=iid, text=label, values=values)

    def refresh_headband_status(self):
        source_path = self.headband_source_var.get().strip()
        target_path = self.headband_target_var.get().strip()
        source_ready, source_status, source_details = self.inspect_headband_iff(source_path, True)
        target_ready, target_status, target_details = self.inspect_headband_iff(target_path, False)
        if source_path and target_path and os.path.abspath(source_path) == os.path.abspath(target_path):
            target_ready = False
            target_status = "Change file"
            target_details = "Source and target headbands must be different IFFs"

        blender_path = self.full_swap_blender_var.get().strip()
        blender_ready = os.path.isfile(blender_path) and os.path.basename(blender_path).lower() == "blender.exe"
        tool_path = self.headband_tool_var.get().strip()
        tool_ready = os.path.isfile(os.path.join(tool_path, "__init__.py"))
        tool_version = self.read_head_swap_tool_version(tool_path) if tool_ready else ""
        mesh_transfer_path = self.find_mesh_data_transfer_tool()
        mesh_transfer_ready = os.path.isfile(os.path.join(mesh_transfer_path, "__init__.py"))
        bridge_ready = self.headband_bridge_result.startswith("Connected")
        setup_ready = source_ready and target_ready and blender_ready and tool_ready and mesh_transfer_ready
        pipeline_ready = setup_ready and bridge_ready and os.path.isfile(HEADBAND_SWAP_BRIDGE)

        self.set_headband_row("source", "Source IFF (Legacy / 2K25)", source_status, source_details)
        self.set_headband_row("target", "NBA 2K26 Target IFF", target_status, target_details)
        self.set_headband_row(
            "blender",
            "Blender Background",
            "Ready" if blender_ready else "Missing",
            blender_path if blender_path else "Blender was not detected",
        )
        tool_details = tool_path if tool_path else "Headband-capable Blender tool was not detected"
        if tool_ready and tool_version:
            tool_details = f"Version {tool_version} | {tool_path}"
        self.set_headband_row("tool", "Blender Headband Tool", "Ready" if tool_ready else "Missing", tool_details)
        self.set_headband_row(
            "mesh_transfer",
            "Mesh Data Transfer",
            "Ready" if mesh_transfer_ready else "Missing",
            mesh_transfer_path if mesh_transfer_path else "Mesh Data Transfer was not detected",
        )
        self.set_headband_row(
            "bridge",
            "Background Link",
            "Connected" if bridge_ready else self.headband_bridge_result,
            "Blender can load Headband Swap in background mode" if bridge_ready else "Run Test Background Link",
        )
        self.set_headband_row(
            "pipeline",
            "Automated Headband Swap",
            "Ready" if pipeline_ready else "Waiting",
            "Legacy/2K25 import -> Active UV transfer -> 2K26 headband export",
        )
        self.headband_run_button.configure(state=tk.NORMAL if pipeline_ready else tk.DISABLED)
        if pipeline_ready:
            self.headband_status_var.set("Headband Swap is ready to run in background Blender.")
        elif setup_ready:
            self.headband_status_var.set("Headband files and tools are ready. Test the background link.")

    def browse_headband_source(self):
        current = self.headband_source_var.get().strip()
        path = filedialog.askopenfilename(
            title="Choose legacy or NBA 2K25 source headband IFF",
            filetypes=[("NBA 2K headband IFF", "*.iff"), ("All files", "*.*")],
            initialdir=os.path.dirname(current) if current else "",
        )
        if path:
            self.headband_source_var.set(path)
            self.refresh_headband_status()

    def browse_headband_target(self):
        current = self.headband_target_var.get().strip()
        path = filedialog.askopenfilename(
            title="Choose NBA 2K26 target headband IFF",
            filetypes=[("NBA 2K headband IFF", "*.iff"), ("All files", "*.*")],
            initialdir=os.path.dirname(current) if current else "",
        )
        if path:
            self.headband_target_var.set(path)
            self.refresh_headband_status()

    def browse_headband_tool(self):
        current = self.headband_tool_var.get().strip()
        path = filedialog.askdirectory(
            title="Choose headband-capable Blender tool folder",
            initialdir=current if os.path.isdir(current) else "",
        )
        if not path:
            return
        nested = os.path.join(path, "NBA_Character_HeadSwap")
        if os.path.isfile(os.path.join(nested, "__init__.py")):
            path = nested
        self.headband_tool_var.set(path)
        self.headband_bridge_result = "Not tested"
        self.refresh_headband_status()

    def detect_headband_paths(self):
        blender_path = self.find_blender_executable()
        tool_path = self.find_headband_swap_tool()
        if blender_path:
            self.full_swap_blender_var.set(blender_path)
        if tool_path:
            self.headband_tool_var.set(tool_path)
        self.headband_bridge_result = "Not tested"
        self.refresh_headband_status()
        self.refresh_full_swap_status()

    def check_headband_setup(self):
        self.refresh_headband_status()
        source_ready = self.inspect_headband_iff(self.headband_source_var.get().strip(), True)[0]
        target_ready = self.inspect_headband_iff(self.headband_target_var.get().strip(), False)[0]
        blender_ready = os.path.isfile(self.full_swap_blender_var.get().strip())
        tool_ready = os.path.isfile(os.path.join(self.headband_tool_var.get().strip(), "__init__.py"))
        mesh_transfer_ready = os.path.isfile(os.path.join(self.find_mesh_data_transfer_tool(), "__init__.py"))
        if source_ready and target_ready and blender_ready and tool_ready and mesh_transfer_ready:
            self.headband_status_var.set("Setup is ready. Test the background link to Blender.")
        else:
            self.headband_status_var.set("Complete the headband setup items marked Missing or Wrong format.")

    def test_headband_bridge(self):
        blender_path = self.full_swap_blender_var.get().strip()
        tool_path = self.headband_tool_var.get().strip()
        mesh_transfer_path = self.find_mesh_data_transfer_tool()
        if not os.path.isfile(blender_path):
            messagebox.showinfo("Character Mod Tool", "Choose or detect Blender first.")
            return
        if not os.path.isfile(os.path.join(tool_path, "__init__.py")):
            messagebox.showinfo("Character Mod Tool", "Choose or detect the Blender Headband Tool first.")
            return
        if not os.path.isfile(os.path.join(mesh_transfer_path, "__init__.py")):
            messagebox.showinfo("Character Mod Tool", "Install or enable Mesh Data Transfer in Blender first.")
            return
        module_name = os.path.basename(os.path.normpath(tool_path))
        tool_parent = os.path.dirname(os.path.normpath(tool_path))
        mesh_transfer_parent = os.path.dirname(os.path.normpath(mesh_transfer_path))
        expression = (
            "import sys, bpy; "
            f"sys.path.insert(0, {mesh_transfer_parent!r}); "
            "import mesh_data_transfer as mdt; mdt.register(); "
            f"sys.path.insert(0, {tool_parent!r}); "
            f"import {module_name} as addon; addon.register(); "
            f"from {module_name} import nba2k_character_eyeball_tool as tool; "
            "print('CHARMOD_HEADBAND_OK|' + '.'.join(map(str, addon.bl_info.get('version', (0, 0, 0)))) "
            "+ '|' + str(hasattr(tool, 'transfer_headband_shape')) "
            "+ '|' + str(hasattr(bpy.types.Object, 'mesh_data_transfer_object')))"
        )
        self.headband_status_var.set("Starting Blender in the background...")
        self.update_idletasks()
        try:
            result = subprocess.run(
                [blender_path, "--background", "--factory-startup", "--python-expr", expression],
                capture_output=True,
                text=True,
                timeout=60,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            self.headband_bridge_result = "Failed"
            self.refresh_headband_status()
            messagebox.showerror("Character Mod Tool", f"Could not start the headband background link:\n{exc}")
            return
        output = "\n".join(part for part in (result.stdout, result.stderr) if part)
        match = re.search(r"CHARMOD_HEADBAND_OK\|([0-9.]+)\|True\|True", output)
        if result.returncode == 0 and match:
            self.headband_bridge_result = f"Connected (v{match.group(1)})"
            self.refresh_headband_status()
            self.headband_status_var.set(f"Background Blender connected to Headband Swap v{match.group(1)}.")
            return
        self.headband_bridge_result = "Failed"
        self.refresh_headband_status()
        useful_lines = [line.strip() for line in output.splitlines() if line.strip()]
        detail = useful_lines[-1] if useful_lines else f"Blender exited with code {result.returncode}."
        messagebox.showerror("Character Mod Tool", f"Blender could not load Headband Swap.\n\n{detail}")

    def run_headband_swap(self):
        if self.headband_process is not None:
            return
        source_path = self.headband_source_var.get().strip()
        target_path = self.headband_target_var.get().strip()
        blender_path = self.full_swap_blender_var.get().strip()
        tool_path = self.headband_tool_var.get().strip()
        mesh_transfer_path = self.find_mesh_data_transfer_tool()
        if not self.inspect_headband_iff(source_path, True)[0] or not self.inspect_headband_iff(target_path, False)[0]:
            messagebox.showinfo(
                "Character Mod Tool",
                "Choose a valid legacy or NBA 2K25 source and an NBA 2K26 target headband IFF.",
            )
            return
        if not self.headband_bridge_result.startswith("Connected"):
            messagebox.showinfo("Character Mod Tool", "Run the Headband Test Background Link first.")
            return
        target_base = os.path.splitext(os.path.basename(target_path))[0]
        output_path = filedialog.asksaveasfilename(
            title="Save swapped headband IFF",
            defaultextension=".iff",
            initialfile=f"{target_base}_swapped.iff",
            initialdir=app_settings.ensure_output_dir(self.settings.get("output_dir", "")),
            filetypes=[("NBA 2K headband IFF", "*.iff"), ("All files", "*.*")],
        )
        if not output_path:
            return
        output_abs = os.path.abspath(output_path)
        if output_abs in (os.path.abspath(source_path), os.path.abspath(target_path)):
            messagebox.showerror("Character Mod Tool", "Headband Swap must save to a new IFF filename.")
            return

        self.headband_output_path = output_abs
        self.headband_error = ""
        self.headband_success = None
        self.headband_cancel_requested = False
        self.headband_queue = queue.Queue()
        self.headband_progress_var = tk.StringVar(value="Starting background Blender...")
        window = tk.Toplevel(self)
        self.headband_progress_window = window
        window.title("Headband Swap")
        window.resizable(False, False)
        window.transient(self)
        window.grab_set()
        ttk.Label(window, textvariable=self.headband_progress_var, width=64).pack(fill=tk.X, padx=14, pady=(14, 8))
        progress = ttk.Progressbar(window, mode="indeterminate", length=500)
        progress.pack(fill=tk.X, padx=14)
        progress.start(12)
        ttk.Button(window, text="Cancel", command=self.cancel_headband_swap).pack(pady=14)
        window.protocol("WM_DELETE_WINDOW", self.cancel_headband_swap)

        command = [
            blender_path,
            "--background",
            "--factory-startup",
            "--python",
            HEADBAND_SWAP_BRIDGE,
            "--",
            "--source",
            source_path,
            "--target",
            target_path,
            "--output",
            output_abs,
            "--addon",
            tool_path,
            "--mesh-data-transfer",
            mesh_transfer_path,
        ]
        self.headband_status_var.set("Headband Swap is running in background Blender...")
        self.headband_run_button.configure(state=tk.DISABLED)
        threading.Thread(target=self._headband_worker, args=(command,), daemon=True).start()
        self.after(100, self._poll_headband_queue)

    def _headband_worker(self, command):
        try:
            LOGGER.info("Starting Blender headband command: %s", subprocess.list2cmdline(command))
            process = subprocess.Popen(
                command,
                cwd=os.path.dirname(HEADBAND_SWAP_BRIDGE),
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                errors="replace",
                bufsize=1,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
            )
            self.headband_process = process
            if self.headband_cancel_requested and process.poll() is None:
                process.terminate()
            if process.stdout is not None:
                for line in process.stdout:
                    clean_line = line.rstrip()
                    LOGGER.info("BLENDER HEADBAND | %s", clean_line)
                    self.headband_queue.put(("line", clean_line))
            return_code = process.wait()
            LOGGER.info("Blender headband exited with code %s", return_code)
            self.headband_queue.put(("done", return_code))
        except Exception as exc:
            LOGGER.exception("Blender headband worker failed")
            self.headband_queue.put(("worker_error", str(exc)))

    def _poll_headband_queue(self):
        if self.headband_queue is None:
            return
        finished = False
        return_code = 1
        try:
            while True:
                event = self.headband_queue.get_nowait()
                if event[0] == "line":
                    line = event[1]
                    if line.startswith("CHARMOD_HEADBAND_STAGE|"):
                        stage = line.split("|", 1)[1]
                        self.headband_progress_var.set(stage)
                        self.headband_status_var.set(stage)
                    elif line.startswith("CHARMOD_HEADBAND_ERROR|"):
                        self.headband_error = line.split("|", 1)[1]
                    elif line.startswith("CHARMOD_HEADBAND_SUCCESS|"):
                        try:
                            self.headband_success = json.loads(line.split("|", 1)[1])
                        except json.JSONDecodeError:
                            self.headband_success = None
                elif event[0] == "done":
                    finished = True
                    return_code = event[1]
                elif event[0] == "worker_error":
                    finished = True
                    self.headband_error = event[1]
        except queue.Empty:
            pass
        if finished:
            self._finish_headband_swap(return_code)
        else:
            self.after(100, self._poll_headband_queue)

    def _finish_headband_swap(self, return_code):
        window = self.headband_progress_window
        if window is not None and window.winfo_exists():
            try:
                window.grab_release()
            except tk.TclError:
                pass
            window.destroy()
        self.headband_progress_window = None
        self.headband_process = None
        self.headband_queue = None
        output_valid = os.path.isfile(self.headband_output_path) and zipfile.is_zipfile(self.headband_output_path)
        if return_code == 0 and self.headband_success and output_valid:
            version = self.headband_success.get("addon_version", "")
            self.refresh_headband_status()
            self.set_headband_row("pipeline", "Automated Headband Swap", "Complete", self.headband_output_path)
            self.headband_status_var.set(f"Headband Swap completed with Blender tool v{version}.")
            messagebox.showinfo(
                "Character Mod Tool",
                f"Headband Swap completed successfully.\n\nSaved:\n{self.headband_output_path}",
            )
            return
        self.refresh_headband_status()
        detail = self.headband_error or "Background Blender stopped before creating the output headband IFF."
        self.headband_status_var.set("Headband Swap did not complete.")
        messagebox.showerror("Character Mod Tool", f"Headband Swap did not complete.\n\n{detail}")

    def cancel_headband_swap(self):
        self.headband_cancel_requested = True
        self.headband_error = "Headband Swap was cancelled."
        if self.headband_progress_var is not None:
            self.headband_progress_var.set("Cancelling Headband Swap...")
        process = self.headband_process
        if process is not None and process.poll() is None:
            try:
                process.terminate()
            except OSError:
                pass

    @staticmethod
    def player_iff_prefix(path):
        match = re.match(r"(png\d+)", os.path.basename(path), re.IGNORECASE)
        return match.group(1) if match else ""

    @staticmethod
    def read_accessory_appearance(path):
        if not path or not os.path.isfile(path) or not zipfile.is_zipfile(path):
            raise ValueError("Choose a readable ZIP-style player IFF.")
        wanted = {name.lower() for name in APPEARANCE_ENTRY_NAMES}
        with zipfile.ZipFile(path, "r") as archive:
            entry_name = next(
                (name for name in archive.namelist() if os.path.basename(name).lower() in wanted),
                "",
            )
            if not entry_name:
                raise ValueError("appearance_info was not found in the player IFF.")
            original_data = archive.read(entry_name)
        parsed, wrapped, error = try_parse_structured_text(entry_name, original_data)
        if not isinstance(parsed, dict):
            raise ValueError(error or "appearance_info could not be read as structured data.")
        accessory_items = parsed.get("accessory_items")
        if not isinstance(accessory_items, dict):
            raise ValueError("appearance_info does not contain accessory_items.")
        return entry_name, original_data, parsed, wrapped

    @staticmethod
    def find_accessory_companions(source_player_path, accessory_name):
        folder = os.path.dirname(source_player_path)
        prefix = CharacterModTool.player_iff_prefix(source_player_path)
        if not prefix or not os.path.isdir(folder):
            return "", ""
        try:
            files = {name.lower(): name for name in os.listdir(folder)}
        except OSError:
            return "", ""
        geo_name = f"{prefix}_geo_{accessory_name}.iff"
        item_name = f"{prefix}_item_{accessory_name}.iff"
        geo_actual = files.get(geo_name.lower(), "")
        item_actual = files.get(item_name.lower(), "")
        return (
            os.path.join(folder, geo_actual) if geo_actual else "",
            os.path.join(folder, item_actual) if item_actual else "",
        )

    @staticmethod
    def write_iff_entry_replacement(source_path, output_path, entry_name, replacement_data):
        output_dir = os.path.dirname(output_path) or os.getcwd()
        os.makedirs(output_dir, exist_ok=True)
        temp_handle = tempfile.NamedTemporaryFile(
            prefix=os.path.basename(output_path) + ".",
            suffix=".tmp",
            dir=output_dir,
            delete=False,
        )
        temp_path = temp_handle.name
        temp_handle.close()
        try:
            with zipfile.ZipFile(source_path, "r") as source_archive:
                with zipfile.ZipFile(temp_path, "w") as output_archive:
                    for info in source_archive.infolist():
                        data = replacement_data if info.filename == entry_name else source_archive.read(info.filename)
                        copied_info = zipfile.ZipInfo(info.filename, date_time=info.date_time)
                        copied_info.comment = info.comment
                        copied_info.extra = info.extra
                        copied_info.internal_attr = info.internal_attr
                        copied_info.external_attr = info.external_attr
                        copied_info.create_system = info.create_system
                        copied_info.flag_bits = info.flag_bits
                        copied_info.compress_type = info.compress_type or zipfile.ZIP_DEFLATED
                        output_archive.writestr(copied_info, data)
            os.replace(temp_path, output_path)
        except Exception:
            try:
                os.remove(temp_path)
            except OSError:
                pass
            raise

    @staticmethod
    def copy_accessory_companion(source_path, destination_path):
        output_dir = os.path.dirname(destination_path) or os.getcwd()
        os.makedirs(output_dir, exist_ok=True)
        temp_handle = tempfile.NamedTemporaryFile(
            prefix=os.path.basename(destination_path) + ".",
            suffix=".tmp",
            dir=output_dir,
            delete=False,
        )
        temp_path = temp_handle.name
        temp_handle.close()
        try:
            shutil.copy2(source_path, temp_path)
            os.replace(temp_path, destination_path)
        except Exception:
            try:
                os.remove(temp_path)
            except OSError:
                pass
            raise

    @staticmethod
    def copied_zip_info(info, filename=None):
        copied = zipfile.ZipInfo(filename or info.filename, date_time=info.date_time)
        copied.comment = info.comment
        copied.extra = info.extra
        copied.internal_attr = info.internal_attr
        copied.external_attr = info.external_attr
        copied.create_system = info.create_system
        copied.flag_bits = info.flag_bits
        copied.compress_type = info.compress_type or zipfile.ZIP_DEFLATED
        return copied

    @staticmethod
    def embedded_binary_names(value):
        names = set()
        if isinstance(value, dict):
            for key, child in value.items():
                if key == "Binary" and isinstance(child, str) and child:
                    names.add(child.lower())
                names.update(CharacterModTool.embedded_binary_names(child))
        elif isinstance(value, list):
            for child in value:
                names.update(CharacterModTool.embedded_binary_names(child))
        return names

    @staticmethod
    def parse_fragment_object(data):
        text = data.decode("utf-8-sig")
        return json.loads("{\n" + text.strip().rstrip(",") + "\n}")

    @staticmethod
    def serialize_fragment_object(data):
        text = json.dumps(data, indent="\t", ensure_ascii=False)
        lines = text.splitlines()
        if len(lines) >= 2:
            text = "\n".join(line[1:] if line.startswith("\t") else line for line in lines[1:-1])
        return (text.rstrip() + "\n").encode("utf-8")

    @staticmethod
    def find_texconv_executable():
        candidates = [
            os.path.join(os.path.dirname(os.path.abspath(__file__)), "tools", "texconv.exe"),
            shutil.which("texconv") or "",
            os.path.join(
                os.path.expanduser("~"),
                "OneDrive", "Documents", "NBA 2k Jersey Modder", "tools", "texconv.exe",
            ),
        ]
        return next((path for path in candidates if path and os.path.isfile(path)), "")

    @staticmethod
    def load_torso_texture_template():
        txtr_path = os.path.join(ACCESSORY_TEMPLATE_DIR, "torso_color_o.TXTR")
        dds_matches = glob.glob(os.path.join(ACCESSORY_TEMPLATE_DIR, "torso_color_o.*.dds"))
        if not os.path.isfile(txtr_path) or not dds_matches:
            raise ValueError("The bundled accessory torso texture template is missing.")
        result = {}
        for path in (txtr_path, dds_matches[0]):
            with open(path, "rb") as handle:
                data = handle.read()
            info = zipfile.ZipInfo(os.path.basename(path))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.create_system = 3
            info.external_attr = 0x81B60000
            result[info.filename] = (data, info)
        return result

    @staticmethod
    def load_solid_glasses_texture_template():
        txtr_path = os.path.join(ACCESSORY_TEMPLATE_DIR, "opaque_color_o.TXTR")
        dds_path = os.path.join(ACCESSORY_TEMPLATE_DIR, "opaque_color_o.dds")
        if not os.path.isfile(txtr_path) or not os.path.isfile(dds_path):
            raise ValueError("The bundled stock-glasses color template is missing.")
        with open(txtr_path, "rb") as handle:
            txtr_data = handle.read()
        with open(dds_path, "rb") as handle:
            dds_data = handle.read()
        return {"stock_glasses_color_o.TXTR": txtr_data, "stock_glasses_color_o.dds": dds_data}

    @staticmethod
    def read_tga_rgba(path):
        with open(path, "rb") as handle:
            data = handle.read()
        if len(data) < 18:
            raise ValueError("Texture converter produced an invalid TGA file.")
        id_length, color_map_type, image_type = struct.unpack_from("<BBB", data, 0)
        width, height, bits_per_pixel, descriptor = struct.unpack_from("<HHBB", data, 12)
        if color_map_type != 0 or image_type != 2 or bits_per_pixel not in (24, 32):
            raise ValueError("Texture converter produced an unsupported TGA layout.")
        bytes_per_pixel = bits_per_pixel // 8
        start = 18 + id_length
        expected = width * height * bytes_per_pixel
        if start + expected > len(data):
            raise ValueError("Texture converter produced a truncated TGA file.")
        source = data[start:start + expected]
        top_origin = bool(descriptor & 0x20)
        pixels = bytearray(width * height * 4)
        for y in range(height):
            source_y = y if top_origin else height - 1 - y
            for x in range(width):
                source_pos = (source_y * width + x) * bytes_per_pixel
                target_pos = (y * width + x) * 4
                blue, green, red = source[source_pos:source_pos + 3]
                alpha = source[source_pos + 3] if bytes_per_pixel == 4 else 255
                pixels[target_pos:target_pos + 4] = bytes((red, green, blue, alpha))
        return width, height, pixels

    @staticmethod
    def write_tga_rgba(path, width, height, pixels):
        if len(pixels) != width * height * 4:
            raise ValueError("RGBA atlas pixel count is invalid.")
        header = bytearray(18)
        header[2] = 2
        struct.pack_into("<HH", header, 12, width, height)
        header[16] = 32
        header[17] = 0x28
        bgra = bytearray(len(pixels))
        for pos in range(0, len(pixels), 4):
            red, green, blue, alpha = pixels[pos:pos + 4]
            bgra[pos:pos + 4] = bytes((blue, green, red, alpha))
        with open(path, "wb") as handle:
            handle.write(header)
            handle.write(bgra)

    @classmethod
    def build_torso_goggles_atlas(
        cls,
        torso_dds,
        torso_txtr,
        goggles_dds,
        goggles_txtr,
        frame_color="#FFFFFF",
    ):
        texconv = cls.find_texconv_executable()
        if not texconv:
            raise ValueError("The bundled tools\\texconv.exe texture converter was not found.")
        torso_meta = cls.parse_fragment_object(torso_txtr)
        goggles_meta = cls.parse_fragment_object(goggles_txtr)
        torso_value = next(iter(torso_meta.values()))
        goggles_value = next(iter(goggles_meta.values()))
        torso_min = list(torso_value.get("Min", [0.0, 0.0, 0.0, 0.0]))
        torso_max = list(torso_value.get("Max", [1.0, 1.0, 1.0, 1.0]))
        goggles_min = list(goggles_value.get("Min", [0.0, 0.0, 0.0, 0.0]))
        goggles_max = list(goggles_value.get("Max", [1.0, 1.0, 1.0, 1.0]))
        if not re.fullmatch(r"#[0-9A-Fa-f]{6}", frame_color or ""):
            raise ValueError("Frame color must use #RRGGBB format.")
        frame_rgb = [int(frame_color[index:index + 2], 16) / 255.0 for index in (1, 3, 5)]
        preserve_source_color = frame_color.upper() == "#FFFFFF"
        frame_multiplier = frame_rgb + [1.0]
        if preserve_source_color:
            tinted_goggles_min = list(goggles_min)
            tinted_goggles_max = list(goggles_max)
        else:
            tinted_goggles_min = [frame_rgb[i] * 0.70 for i in range(3)] + [goggles_min[3]]
            tinted_goggles_max = list(frame_rgb) + [goggles_max[3]]
        combined_min = [min(torso_min[i], tinted_goggles_min[i]) for i in range(4)]
        combined_max = [max(torso_max[i], tinted_goggles_max[i]) for i in range(4)]

        with tempfile.TemporaryDirectory(prefix="character_mod_atlas_") as temp_dir:
            torso_path = os.path.join(temp_dir, "torso.dds")
            goggles_path = os.path.join(temp_dir, "goggles.dds")
            with open(torso_path, "wb") as handle:
                handle.write(torso_dds)
            with open(goggles_path, "wb") as handle:
                handle.write(goggles_dds)
            creation_flags = subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0
            for source_path in (torso_path, goggles_path):
                result = subprocess.run(
                    [
                        texconv, "-ft", "TGA", "-f", "R8G8B8A8_UNORM",
                        "-w", "512", "-h", "1024", "-m", "1", "-y",
                        "-o", temp_dir, source_path,
                    ],
                    capture_output=True,
                    text=True,
                    creationflags=creation_flags,
                )
                if result.returncode:
                    raise ValueError("Texture conversion failed:\n" + (result.stderr or result.stdout).strip())
            torso_width, torso_height, torso_pixels = cls.read_tga_rgba(os.path.join(temp_dir, "torso.tga"))
            goggles_width, goggles_height, goggles_pixels = cls.read_tga_rgba(os.path.join(temp_dir, "goggles.tga"))
            if (torso_width, torso_height) != (512, 1024) or (goggles_width, goggles_height) != (512, 1024):
                raise ValueError("Texture converter returned an unexpected atlas source size.")

            atlas_pixels = bytearray(1024 * 1024 * 4)
            for y in range(1024):
                for half, source_pixels, source_min, source_max, multiplier in (
                    (0, torso_pixels, torso_min, torso_max, [1.0, 1.0, 1.0, 1.0]),
                    (1, goggles_pixels, goggles_min, goggles_max, frame_multiplier),
                ):
                    for x in range(512):
                        source_pos = (y * 512 + x) * 4
                        target_pos = (y * 1024 + half * 512 + x) * 4
                        source_luma = (
                            source_pixels[source_pos] * 0.2126
                            + source_pixels[source_pos + 1] * 0.7152
                            + source_pixels[source_pos + 2] * 0.0722
                        ) / 255.0
                        for channel in range(4):
                            encoded = source_pixels[source_pos + channel] / 255.0
                            if half == 1 and channel < 3 and not preserve_source_color:
                                shade = 0.70 + source_luma * 0.30
                                physical = frame_rgb[channel] * shade
                            else:
                                physical = (
                                    source_min[channel] + encoded * (source_max[channel] - source_min[channel])
                                ) * multiplier[channel]
                            span = combined_max[channel] - combined_min[channel]
                            normalized = (physical - combined_min[channel]) / span if span else 0.0
                            atlas_pixels[target_pos + channel] = max(0, min(255, int(round(normalized * 255.0))))
            atlas_tga = os.path.join(temp_dir, "torso_goggles_atlas.tga")
            cls.write_tga_rgba(atlas_tga, 1024, 1024, atlas_pixels)
            result = subprocess.run(
                [texconv, "-f", "BC7_UNORM", "-m", "0", "-y", "-o", temp_dir, atlas_tga],
                capture_output=True,
                text=True,
                creationflags=creation_flags,
            )
            if result.returncode:
                raise ValueError("Atlas compression failed:\n" + (result.stderr or result.stdout).strip())
            atlas_dds_path = os.path.join(temp_dir, "torso_goggles_atlas.dds")
            with open(atlas_dds_path, "rb") as handle:
                atlas_dds = handle.read()
            goggles_alpha = bytes(goggles_pixels[3::4])

        torso_value["Min"] = combined_min
        torso_value["Max"] = combined_max
        torso_value["Format"] = "BC7_UNORM"
        dds_header_size = 148 if len(atlas_dds) >= 148 and atlas_dds[84:88] == b"DX10" else 128
        torso_value["PixelDataSize"] = len(atlas_dds) - dds_header_size
        return atlas_dds, cls.serialize_fragment_object(torso_meta), (goggles_width, goggles_height, goggles_alpha)

    @classmethod
    def build_torso_multi_accessory_atlas(cls, torso_dds, torso_txtr, accessory_textures, frame_color):
        if not 2 <= len(accessory_textures) <= 4:
            raise ValueError("Multi-accessory atlas requires 2 to 4 accessories.")
        texconv = cls.find_texconv_executable()
        if not texconv:
            raise ValueError("The bundled tools\\texconv.exe texture converter was not found.")
        if not re.fullmatch(r"#[0-9A-Fa-f]{6}", frame_color or ""):
            raise ValueError("Frame color must use #RRGGBB format.")

        torso_meta = cls.parse_fragment_object(torso_txtr)
        torso_value = next(iter(torso_meta.values()))
        torso_min = list(torso_value.get("Min", [0.0, 0.0, 0.0, 0.0]))
        torso_max = list(torso_value.get("Max", [1.0, 1.0, 1.0, 1.0]))
        frame_rgb = [int(frame_color[index:index + 2], 16) / 255.0 for index in (1, 3, 5)]
        preserve_source_color = frame_color.upper() == "#FFFFFF"

        if len(accessory_textures) == 2:
            cells = [(512, 0, 512, 512), (512, 512, 512, 512)]
        else:
            cells = [
                (512, 0, 256, 512),
                (768, 0, 256, 512),
                (512, 512, 256, 512),
                (768, 512, 256, 512),
            ][:len(accessory_textures)]

        parsed_accessories = []
        combined_min = list(torso_min)
        combined_max = list(torso_max)
        for texture in accessory_textures:
            metadata = cls.parse_fragment_object(texture["txtr"])
            value = next(iter(metadata.values()))
            source_min = list(value.get("Min", [0.0, 0.0, 0.0, 0.0]))
            source_max = list(value.get("Max", [1.0, 1.0, 1.0, 1.0]))
            if preserve_source_color:
                tinted_min = source_min
                tinted_max = source_max
            else:
                tinted_min = [frame_rgb[i] * 0.70 for i in range(3)] + [source_min[3]]
                tinted_max = list(frame_rgb) + [source_max[3]]
            combined_min = [min(combined_min[i], tinted_min[i]) for i in range(4)]
            combined_max = [max(combined_max[i], tinted_max[i]) for i in range(4)]
            parsed_accessories.append((source_min, source_max))

        with tempfile.TemporaryDirectory(prefix="character_mod_multi_atlas_") as temp_dir:
            creation_flags = subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0

            def convert_to_tga(label, dds_data, width, height):
                dds_path = os.path.join(temp_dir, label + ".dds")
                with open(dds_path, "wb") as handle:
                    handle.write(dds_data)
                result = subprocess.run(
                    [
                        texconv, "-ft", "TGA", "-f", "R8G8B8A8_UNORM",
                        "-w", str(width), "-h", str(height), "-m", "1", "-y",
                        "-o", temp_dir, dds_path,
                    ],
                    capture_output=True,
                    text=True,
                    creationflags=creation_flags,
                )
                if result.returncode:
                    raise ValueError("Texture conversion failed:\n" + (result.stderr or result.stdout).strip())
                return cls.read_tga_rgba(os.path.join(temp_dir, label + ".tga"))

            torso_width, torso_height, torso_pixels = convert_to_tga("torso", torso_dds, 512, 1024)
            if (torso_width, torso_height) != (512, 1024):
                raise ValueError("Texture converter returned an unexpected torso atlas size.")

            atlas_pixels = bytearray(1024 * 1024 * 4)
            alpha_maps = []

            def write_region(source_pixels, source_width, source_height, target_x, target_y, source_min, source_max, tint):
                for y in range(source_height):
                    for x in range(source_width):
                        source_pos = (y * source_width + x) * 4
                        target_pos = ((target_y + y) * 1024 + target_x + x) * 4
                        source_luma = (
                            source_pixels[source_pos] * 0.2126
                            + source_pixels[source_pos + 1] * 0.7152
                            + source_pixels[source_pos + 2] * 0.0722
                        ) / 255.0
                        for channel in range(4):
                            encoded = source_pixels[source_pos + channel] / 255.0
                            if tint and channel < 3 and not preserve_source_color:
                                physical = frame_rgb[channel] * (0.70 + source_luma * 0.30)
                            else:
                                physical = source_min[channel] + encoded * (source_max[channel] - source_min[channel])
                            span = combined_max[channel] - combined_min[channel]
                            normalized = (physical - combined_min[channel]) / span if span else 0.0
                            atlas_pixels[target_pos + channel] = max(0, min(255, int(round(normalized * 255.0))))

            write_region(torso_pixels, 512, 1024, 0, 0, torso_min, torso_max, False)
            for index, (texture, cell, ranges) in enumerate(zip(accessory_textures, cells, parsed_accessories)):
                cell_x, cell_y, cell_width, cell_height = cell
                width, height, pixels = convert_to_tga(
                    f"accessory_{index}", texture["dds"], cell_width, cell_height
                )
                write_region(pixels, width, height, cell_x, cell_y, ranges[0], ranges[1], True)
                alpha_maps.append({
                    "width": width,
                    "height": height,
                    "pixels": bytes(pixels[3::4]),
                    "u": cell_x / 1024.0,
                    "v": cell_y / 1024.0,
                    "w": cell_width / 1024.0,
                    "h": cell_height / 1024.0,
                })

            atlas_tga = os.path.join(temp_dir, "torso_multi_accessory_atlas.tga")
            cls.write_tga_rgba(atlas_tga, 1024, 1024, atlas_pixels)
            result = subprocess.run(
                [texconv, "-f", "BC7_UNORM", "-m", "0", "-y", "-o", temp_dir, atlas_tga],
                capture_output=True,
                text=True,
                creationflags=creation_flags,
            )
            if result.returncode:
                raise ValueError("Atlas compression failed:\n" + (result.stderr or result.stdout).strip())
            atlas_path = os.path.join(temp_dir, "torso_multi_accessory_atlas.dds")
            with open(atlas_path, "rb") as handle:
                atlas_dds = handle.read()

        torso_value["Min"] = combined_min
        torso_value["Max"] = combined_max
        torso_value["Format"] = "BC7_UNORM"
        dds_header_size = 148 if atlas_dds[84:88] == b"DX10" else 128
        torso_value["PixelDataSize"] = len(atlas_dds) - dds_header_size
        return atlas_dds, cls.serialize_fragment_object(torso_meta), alpha_maps

    @classmethod
    def write_hihead_accessory_bake(
        cls,
        target_path,
        output_path,
        geo_path,
        item_path,
        accessory_name,
        appearance_entry,
        appearance_data,
        frame_color="#FFFFFF",
        geometry_lens_split=False,
    ):
        with zipfile.ZipFile(target_path, "r") as target_archive:
            target_infos = target_archive.infolist()
            target_entries = {info.filename: target_archive.read(info.filename) for info in target_infos}

        hihead_name = next(
            (
                name for name in target_entries
                if os.path.basename(name).lower() == "hihead.scne"
            ),
            "",
        )
        if not hihead_name:
            raise ValueError("The target player does not contain hihead.SCNE.")

        target_scne, target_wrapped, target_error = try_parse_structured_text(
            hihead_name,
            target_entries[hihead_name],
        )
        if not isinstance(target_scne, dict) or not target_scne:
            raise ValueError(target_error or "The target hihead.SCNE could not be parsed.")

        with zipfile.ZipFile(geo_path, "r") as geo_archive:
            geo_infos = [info for info in geo_archive.infolist() if not info.is_dir()]
            geo_entries = {info.filename: geo_archive.read(info.filename) for info in geo_infos}
            geo_scne_info = next(
                (info for info in geo_infos if info.filename.lower().endswith(".scne")),
                None,
            )
            if geo_scne_info is None:
                raise ValueError("The accessory geometry IFF does not contain a SCNE entry.")
            geo_scne, _geo_wrapped, geo_error = try_parse_structured_text(
                geo_scne_info.filename,
                geo_entries[geo_scne_info.filename],
            )
            if not isinstance(geo_scne, dict) or not geo_scne:
                raise ValueError(geo_error or "The accessory SCNE could not be parsed.")
            additions = {}

        item_entries = {}
        if item_path:
            with zipfile.ZipFile(item_path, "r") as item_archive:
                for info in item_archive.infolist():
                    if not info.is_dir():
                        item_entries[info.filename] = item_archive.read(info.filename)
        if geometry_lens_split and not item_entries:
            item_entries = cls.load_solid_glasses_texture_template()
        target_root = next(iter(target_scne))
        source_root = next(iter(geo_scne))
        target_scene = target_scne[target_root]
        source_scene = geo_scne[source_root]
        if not isinstance(target_scene, dict) or not isinstance(source_scene, dict):
            raise ValueError("The target or accessory SCNE root is not an object.")

        source_models = source_scene.get("Model", {})
        if not isinstance(source_models, dict) or not source_models:
            raise ValueError("The accessory SCNE does not contain a model.")
        source_model = copy.deepcopy(next(iter(source_models.values())))
        source_prims = source_model.get("Prim", [])
        if not source_prims:
            raise ValueError("The accessory model does not contain a Prim.")
        source_mesh_name = str(source_prims[0].get("Mesh", "") or "")

        target_models = target_scene.get("Model", {})
        target_model = target_models.get("hihead") if isinstance(target_models, dict) else None
        if not isinstance(target_model, dict):
            raise ValueError("The target SCNE does not contain Model hihead.")
        target_prims = target_model.setdefault("Prim", [])
        target_model["Prim"] = [
            prim for prim in target_prims
            if not (
                isinstance(prim, dict)
                and any(token in str(prim.get("Mesh", "") or "").lower() for token in ("goggle", "glass"))
            )
        ]
        previous_accessory_model = target_models.pop(accessory_name, None)
        stale_names = cls.embedded_binary_names(previous_accessory_model) if previous_accessory_model else set()
        target_objects = target_scene.get("Object", {})
        if isinstance(target_objects, dict):
            target_objects.pop(accessory_name, None)

        def resolve_binary(entries, reference):
            reference_stem = os.path.splitext(os.path.basename(reference or ""))[0].lower()
            match = next(
                (
                    name for name in entries
                    if os.path.splitext(os.path.basename(name))[0].lower() == reference_stem
                ),
                "",
            )
            if not match:
                raise ValueError(f"Referenced binary was not found: {reference}")
            return match

        target_streams = target_model.get("VertexStream", [])
        source_streams = source_model.get("VertexStream", [])
        if len(target_streams) < 2 or len(source_streams) < 2:
            raise ValueError("The target and accessory require position and surface vertex streams.")
        if not target_streams[0] or not target_streams[1] or not source_streams[0] or not source_streams[1]:
            raise ValueError("A required target or accessory vertex stream is empty.")
        if int(target_streams[0].get("Stride", 0)) != 12 or int(source_streams[0].get("Stride", 0)) != 12:
            raise ValueError("The position streams are not the expected 12-byte format.")
        if int(target_streams[1].get("Stride", 0)) != 16 or int(source_streams[1].get("Stride", 0)) != 12:
            raise ValueError("This experiment requires a 16-byte target and 12-byte accessory surface stream.")

        target_position_name = resolve_binary(target_entries, target_streams[0].get("Binary"))
        target_surface_name = resolve_binary(target_entries, target_streams[1].get("Binary"))
        source_position_name = resolve_binary(geo_entries, source_streams[0].get("Binary"))
        source_surface_name = resolve_binary(geo_entries, source_streams[1].get("Binary"))
        target_positions = target_entries[target_position_name]
        target_surfaces = target_entries[target_surface_name]
        source_positions = geo_entries[source_position_name]
        source_surfaces = geo_entries[source_surface_name]
        target_vertex_count = len(target_positions) // 12
        source_vertex_count = len(source_positions) // 12
        if len(target_positions) % 12 or len(target_surfaces) != target_vertex_count * 16:
            raise ValueError("The target hihead vertex streams have inconsistent sizes.")
        if len(source_positions) % 12 or len(source_surfaces) != source_vertex_count * 12:
            raise ValueError("The accessory vertex streams have inconsistent sizes.")
        if target_vertex_count + source_vertex_count > 65535:
            raise ValueError("The merged hihead exceeds the 16-bit vertex-index limit.")

        source_format = source_model.get("VertexFormat", {}).get("TEXCOORD0", {})
        target_format = target_model.get("VertexFormat", {}).get("TEXCOORD0", {})
        source_offset = list(source_format.get("Offset", [0.0, 0.0]))
        source_scale = list(source_format.get("Scale", [1.0, 1.0]))
        target_offset = list(target_format.get("Offset", [0.0, 0.0]))
        target_scale = list(target_format.get("Scale", [1.0, 1.0]))
        if len(source_offset) < 2 or len(source_scale) < 2 or len(target_offset) < 2 or len(target_scale) < 2:
            raise ValueError("The source or target TEXCOORD0 mapping is incomplete.")
        if not target_scale[0] or not target_scale[1]:
            raise ValueError("The target TEXCOORD0 scale cannot be zero.")

        def decode_snorm(value):
            if value <= -32768:
                return -1.0
            if value >= 32767:
                return 1.0
            return float(value) / 32767.0

        def encode_snorm(value):
            value = max(-1.0, min(1.0, float(value)))
            if value <= -1.0:
                return -32768
            if value >= 1.0:
                return 32767
            return int(round(value * 32767.0))

        converted_surfaces = bytearray()
        source_uvs = []
        for vertex_index in range(source_vertex_count):
            source_pos = vertex_index * 12
            tangent = source_surfaces[source_pos:source_pos + 4]
            packed_u, packed_v = struct.unpack_from("<hh", source_surfaces, source_pos + 4)
            source_u = decode_snorm(packed_u) * float(source_scale[0]) + float(source_offset[0])
            source_v = decode_snorm(packed_v) * float(source_scale[1]) + float(source_offset[1])
            source_uvs.append((source_u, source_v))
            target_u = encode_snorm((source_u - float(target_offset[0])) / float(target_scale[0]))
            target_v = encode_snorm((source_v - float(target_offset[1])) / float(target_scale[1]))
            weight_data = (
                struct.pack("<I", 49 << 8)
                if geometry_lens_split
                else source_surfaces[source_pos + 8:source_pos + 12]
            )
            converted_surfaces.extend(tangent)
            converted_surfaces.extend(struct.pack("<hh", target_u, target_v))
            converted_surfaces.extend(b"\x00\x00\x00\x00")
            converted_surfaces.extend(weight_data)

        target_index_info = target_model.get("IndexBuffer", {})
        source_index_info = source_model.get("IndexBuffer", {})
        if target_index_info.get("Format") != "R16_UINT" or source_index_info.get("Format") != "R16_UINT":
            raise ValueError("The target and accessory must both use 16-bit index buffers.")
        target_index_name = resolve_binary(target_entries, target_index_info.get("Binary"))
        source_index_name = resolve_binary(geo_entries, source_index_info.get("Binary"))
        target_indices = target_entries[target_index_name]
        source_indices = geo_entries[source_index_name]
        source_prim = copy.deepcopy(source_prims[0])
        source_lod_specs = source_prim.get("LodList") or [
            {"Start": source_prim.get("Start", 0), "Count": source_prim.get("Count", 0)}
        ]
        source_lod_indices = []
        for lod_spec in source_lod_specs:
            source_start = int(lod_spec.get("Start", 0))
            source_count = int(lod_spec.get("Count", 0))
            source_index_slice = source_indices[source_start * 2:(source_start + source_count) * 2]
            if source_count <= 0 or len(source_index_slice) != source_count * 2 or source_count % 3:
                raise ValueError("An accessory Prim LOD index range is invalid.")
            unpacked_lod = struct.unpack(f"<{source_count}H", source_index_slice)
            if unpacked_lod and max(unpacked_lod) >= source_vertex_count:
                raise ValueError("An accessory Prim LOD references a vertex outside its position stream.")
            source_lod_indices.append(unpacked_lod)
        unpacked_indices = source_lod_indices[0]
        merged_positions = target_positions + source_positions
        merged_surfaces = bytearray(target_surfaces + bytes(converted_surfaces))

        goggles_vertex_ids = {target_vertex_count + value for value in unpacked_indices}
        torso_vertex_ids = set()

        def read_target_uv(vertex_index):
            packed_u, packed_v = struct.unpack_from("<hh", merged_surfaces, vertex_index * 16 + 4)
            return (
                decode_snorm(packed_u) * float(target_scale[0]) + float(target_offset[0]),
                decode_snorm(packed_v) * float(target_scale[1]) + float(target_offset[1]),
            )

        def write_target_uv(vertex_index, u_value, v_value):
            packed_u = encode_snorm((u_value - float(target_offset[0])) / float(target_scale[0]))
            packed_v = encode_snorm((v_value - float(target_offset[1])) / float(target_scale[1]))
            struct.pack_into("<hh", merged_surfaces, vertex_index * 16 + 4, packed_u, packed_v)

        atlas_dds = None
        atlas_txtr = None
        torso_txtr_name = ""
        torso_dds_name = ""
        target_index_values = struct.unpack(f"<{len(target_indices) // 2}H", target_indices)
        torso_prim = next(
            (prim for prim in target_model.get("Prim", []) if prim.get("Material") == "torso_shader"),
            None,
        )
        if torso_prim is None:
            raise ValueError("The target hihead does not contain a torso_shader Prim for the texture atlas.")
        torso_start = int(torso_prim.get("Start", 0))
        torso_count = int(torso_prim.get("Count", 0))
        torso_vertex_ids = set(target_index_values[torso_start:torso_start + torso_count])
        if torso_vertex_ids.intersection(goggles_vertex_ids):
            raise ValueError("The target torso and glasses unexpectedly share vertices.")
        for vertex_index in torso_vertex_ids:
            u_value, v_value = read_target_uv(vertex_index)
            write_target_uv(vertex_index, 2.0 + (u_value - 2.0) * 0.5, v_value)
        for vertex_index in goggles_vertex_ids:
            u_value, v_value = read_target_uv(vertex_index)
            write_target_uv(vertex_index, 2.5 + u_value * 0.5, v_value)

        torso_txtr_name = next(
            (name for name in target_entries if os.path.basename(name).lower() == "torso_color_o.txtr"),
            "",
        )
        torso_dds_name = next(
            (
                name for name in target_entries
                if os.path.basename(name).lower().startswith("torso_color_o.") and name.lower().endswith(".dds")
            ),
            "",
        )
        if not torso_txtr_name or not torso_dds_name:
            template_entries = cls.load_torso_texture_template()
            for name, value in template_entries.items():
                additions[name] = value
                target_entries[name] = value[0]
            torso_txtr_name = next(
                name for name in template_entries
                if os.path.basename(name).lower() == "torso_color_o.txtr"
            )
            torso_dds_name = next(
                name for name in template_entries
                if os.path.basename(name).lower().startswith("torso_color_o.") and name.lower().endswith(".dds")
            )
        goggles_txtr_name = next(
            (
                name for name in item_entries
                if "color_o" in os.path.basename(name).lower() and name.lower().endswith(".txtr")
            ),
            "",
        )
        goggles_dds_name = next(
            (
                name for name in item_entries
                if "color_o." in os.path.basename(name).lower() and name.lower().endswith(".dds")
            ),
            "",
        )
        if not all((torso_txtr_name, torso_dds_name, goggles_txtr_name, goggles_dds_name)):
            raise ValueError("The target torso color or glasses color TXTR/DDS pair was not found.")
        atlas_dds, atlas_txtr, alpha_map = cls.build_torso_goggles_atlas(
            target_entries[torso_dds_name],
            target_entries[torso_txtr_name],
            item_entries[goggles_dds_name],
            item_entries[goggles_txtr_name],
            frame_color,
        )
        if torso_txtr_name in additions:
            additions[torso_txtr_name] = (atlas_txtr, additions[torso_txtr_name][1])
        if torso_dds_name in additions:
            additions[torso_dds_name] = (atlas_dds, additions[torso_dds_name][1])

        frame_indices = []
        lens_indices = []
        if geometry_lens_split:
            triangles = [tuple(unpacked_indices[pos:pos + 3]) for pos in range(0, len(unpacked_indices), 3)]
            parent = {vertex: vertex for vertex in unpacked_indices}

            def find_component(vertex):
                while parent[vertex] != vertex:
                    parent[vertex] = parent[parent[vertex]]
                    vertex = parent[vertex]
                return vertex

            def join_components(left, right):
                left_root = find_component(left)
                right_root = find_component(right)
                if left_root != right_root:
                    parent[right_root] = left_root

            for triangle in triangles:
                join_components(triangle[0], triangle[1])
                join_components(triangle[1], triangle[2])
            components = {}
            for triangle in triangles:
                components.setdefault(find_component(triangle[0]), []).append(triangle)
            by_triangle_count = {}
            for component in components.values():
                by_triangle_count.setdefault(len(component), []).append(component)
            paired_counts = [
                count for count, groups in by_triangle_count.items()
                if count >= 20 and len(groups) == 2
            ]
            if not paired_counts:
                raise ValueError("Could not identify the paired lens components in these stock glasses.")
            lens_count = max(paired_counts)
            lens_components = {id(group) for group in by_triangle_count[lens_count]}
            for component in components.values():
                destination = lens_indices if id(component) in lens_components else frame_indices
                for triangle in component:
                    destination.extend(target_vertex_count + value for value in triangle)
        else:
            alpha_width, alpha_height, alpha_pixels = alpha_map
            for index_pos in range(0, len(unpacked_indices), 3):
                triangle = unpacked_indices[index_pos:index_pos + 3]
                triangle_uvs = [source_uvs[vertex_index] for vertex_index in triangle]
                center_u = sum(value[0] for value in triangle_uvs) / 3.0
                center_v = sum(value[1] for value in triangle_uvs) / 3.0
                pixel_x = max(0, min(alpha_width - 1, int(round(center_u * (alpha_width - 1)))))
                pixel_y = max(0, min(alpha_height - 1, int(round(center_v * (alpha_height - 1)))))
                alpha = alpha_pixels[pixel_y * alpha_width + pixel_x]
                destination = lens_indices if alpha < 245 else frame_indices
                destination.extend(target_vertex_count + value for value in triangle)
        if not frame_indices or not lens_indices:
            raise ValueError("The glasses color alpha did not separate into frame and lens triangles.")

        frame_start = len(target_indices) // 2
        lens_start = frame_start + len(frame_indices)
        lod_reference_count = max(1, len(source_lod_specs))
        frame_lod_list = [
            {"Start": frame_start, "Count": len(frame_indices)}
            for _index in range(lod_reference_count)
        ]
        lens_lod_list = [
            {"Start": lens_start, "Count": len(lens_indices)}
            for _index in range(lod_reference_count)
        ]
        appended_index_values = frame_indices + lens_indices
        appended_indices = struct.pack(f"<{len(appended_index_values)}H", *appended_index_values)
        merged_indices = target_indices + appended_indices

        target_entries[target_position_name] = merged_positions
        target_entries[target_surface_name] = bytes(merged_surfaces)
        target_entries[target_index_name] = merged_indices
        target_streams[0]["Size"] = len(merged_positions)
        target_streams[1]["Size"] = len(merged_surfaces)
        target_index_info["Size"] = len(merged_indices)
        target_model["IndexBufferCrc32"] = binascii.crc32(merged_indices) & 0xFFFFFFFF

        source_prim["Start"] = frame_lod_list[0]["Start"]
        source_prim["Count"] = len(frame_indices)
        source_prim["LodList"] = frame_lod_list
        if geometry_lens_split:
            source_prim["BlendIndexRange"] = [49, 49]
        source_prim["Material"] = "torso_shader"
        lens_prim = copy.deepcopy(source_prim)
        lens_prim["Mesh"] = source_mesh_name.rsplit("Shape", 1)[0] + "_lensShape"
        lens_prim["Material"] = "teartube_shader"
        lens_prim["Start"] = lens_lod_list[0]["Start"]
        lens_prim["Count"] = len(lens_indices)
        lens_prim["LodList"] = lens_lod_list
        target_model.setdefault("Prim", []).extend((source_prim, lens_prim))
        target_lods = target_model.get("Lods", [])
        if target_lods and isinstance(target_lods[0], dict):
            target_lods[0]["LodVerts"] = target_vertex_count + source_vertex_count

        target_scene.setdefault("Material", {}).pop("goggles_custom_shader", None)

        replacements = {
            hihead_name: serialize_structured_entry(
                hihead_name,
                target_entries[hihead_name],
                target_scne,
                target_wrapped,
            ),
            appearance_entry: appearance_data,
            target_position_name: merged_positions,
            target_surface_name: bytes(merged_surfaces),
            target_index_name: merged_indices,
        }
        if torso_txtr_name and atlas_txtr is not None:
            replacements[torso_txtr_name] = atlas_txtr
        if torso_dds_name and atlas_dds is not None:
            replacements[torso_dds_name] = atlas_dds
        addition_names = {name.lower() for name in additions}
        item_names = {name.lower() for name in item_entries}
        accessory_prefix = accessory_name.lower() + "_"
        output_dir = os.path.dirname(output_path) or os.getcwd()
        os.makedirs(output_dir, exist_ok=True)
        temp_handle = tempfile.NamedTemporaryFile(
            prefix=os.path.basename(output_path) + ".",
            suffix=".tmp",
            dir=output_dir,
            delete=False,
        )
        temp_path = temp_handle.name
        temp_handle.close()
        try:
            written_names = set()
            with zipfile.ZipFile(temp_path, "w") as output_archive:
                for info in target_infos:
                    lower_name = info.filename.lower()
                    if lower_name in stale_names and lower_name not in addition_names:
                        continue
                    if lower_name in item_names or os.path.basename(lower_name).startswith(accessory_prefix):
                        continue
                    if lower_name in addition_names:
                        data, source_info = next(
                            value for name, value in additions.items() if name.lower() == lower_name
                        )
                        output_archive.writestr(cls.copied_zip_info(source_info), data)
                    else:
                        data = replacements.get(info.filename, target_entries[info.filename])
                        output_archive.writestr(cls.copied_zip_info(info), data)
                    written_names.add(lower_name)
                for name, (data, info) in additions.items():
                    if name.lower() not in written_names:
                        output_archive.writestr(cls.copied_zip_info(info), data)
                        written_names.add(name.lower())
            os.replace(temp_path, output_path)
        except Exception:
            try:
                os.remove(temp_path)
            except OSError:
                pass
            raise
        return len(torso_vertex_ids), len(goggles_vertex_ids), len(frame_indices) // 3, len(lens_indices) // 3

    def browse_accessory_source(self):
        current = self.accessory_source_var.get().strip()
        path = filedialog.askopenfilename(
            title="Choose player-fitted glasses geometry IFF or source player IFF",
            filetypes=[("NBA 2K glasses/player IFF", "*.iff"), ("All files", "*.*")],
            initialdir=os.path.dirname(current) if current else "",
        )
        if path:
            self.accessory_source_var.set(path)
            if self.file_path:
                self.accessory_target_var.set(self.file_path)
            self.accessory_status_var.set("Glasses selected. Click Load Glasses.")

    def load_selected_builtin_glasses(self):
        if not self.file_path:
            messagebox.showinfo("Character Mod Tool", "Open the target player IFF first.")
            return
        glasses_name = self.builtin_glasses_var.get().strip()
        glasses_files = AVAILABLE_BUILT_IN_GLASSES.get(glasses_name)
        if not glasses_files:
            messagebox.showinfo("Character Mod Tool", "Choose glasses from the built-in list.")
            return
        geo_path, item_path = glasses_files
        if not os.path.isfile(geo_path) or not os.path.isfile(item_path):
            messagebox.showerror(
                "Character Mod Tool",
                f"The built-in files for {glasses_name} are missing from the app folder.",
            )
            return
        self.accessory_source_var.set(geo_path)
        self.accessory_target_var.set(self.file_path)
        self.scan_custom_accessories()
        if self.accessory_rows:
            self.accessory_status_var.set(f"{glasses_name} loaded. Choose a color and bake into hihead.")

    def choose_accessory_frame_color(self):
        _rgb, selected = colorchooser.askcolor(
            color=self.accessory_frame_color_var.get(),
            title="Choose accessory frame color",
            parent=self,
        )
        if selected:
            selected = selected.upper()
            self.accessory_frame_color_var.set(selected)
            self.accessory_frame_color_swatch.configure(background=selected)

    def reset_accessory_frame_color(self):
        self.accessory_frame_color_var.set("#FFFFFF")
        self.accessory_frame_color_swatch.configure(background="#FFFFFF")

    def browse_accessory_target(self):
        current = self.accessory_target_var.get().strip()
        path = filedialog.askopenfilename(
            title="Choose target player IFF",
            filetypes=[("NBA 2K player IFF", "*.iff"), ("All files", "*.*")],
            initialdir=os.path.dirname(current or self.file_path),
        )
        if path:
            self.accessory_target_var.set(path)
            self.accessory_status_var.set("Target selected. Scan accessories to continue.")

    def use_open_iff_as_accessory_target(self):
        if not self.file_path:
            messagebox.showinfo("Character Mod Tool", "Open the target player IFF first.")
            return
        self.accessory_target_var.set(self.file_path)
        self.accessory_status_var.set("Open IFF selected as the accessory target.")

    def scan_custom_accessories(self):
        source_path = self.accessory_source_var.get().strip()
        source_name = os.path.basename(source_path).lower()
        player_geo_match = re.fullmatch(r"png\d+_geo_(.+)\.iff", source_name, re.IGNORECASE)
        direct_player_geometry = bool(
            player_geo_match
            and any(token in player_geo_match.group(1) for token in ("goggle", "glasses", "eyewear"))
        )
        if source_name.startswith("clothing_resource_") and "goggles" in source_name:
            messagebox.showinfo(
                "Character Mod Tool",
                "Shared clothing_resource glasses use an average-head fit and can be positioned incorrectly. "
                "Choose glasses already fitted to a player, named png####_geo_goggles_*.iff.",
            )
            return
        target_path = (
            self.file_path
            if direct_player_geometry and self.file_path
            else self.accessory_target_var.get().strip()
        )
        if direct_player_geometry and target_path:
            self.accessory_target_var.set(target_path)
        if source_path and target_path and os.path.abspath(source_path) == os.path.abspath(target_path):
            messagebox.showinfo("Character Mod Tool", "Source and target players must be different IFFs.")
            return
        try:
            if direct_player_geometry:
                if not zipfile.is_zipfile(source_path):
                    raise ValueError("The selected glasses geometry is not a ZIP-style IFF.")
                glasses_name = player_geo_match.group(1)
                definition = {"name": glasses_name, "type": "goggles", "mesh": glasses_name}
                source_entry = ""
                source_data = {"accessory_items": {"items": [definition]}}
            else:
                source_entry, _source_original, source_data, _source_wrapped = self.read_accessory_appearance(source_path)
            target_entry, target_original, target_data, target_wrapped = self.read_accessory_appearance(target_path)
        except (OSError, ValueError, zipfile.BadZipFile) as exc:
            messagebox.showerror("Character Mod Tool", f"Could not scan accessories:\n{exc}")
            return

        self.accessory_source_entry = source_entry
        self.accessory_target_entry = target_entry
        self.accessory_source_data = source_data
        self.accessory_target_data = target_data
        self.accessory_target_original_data = target_original
        self.accessory_target_wrapped = target_wrapped
        self.accessory_rows = {}
        self.accessory_config_rows = {}
        self.accessory_tree.delete(*self.accessory_tree.get_children())
        self.accessory_config_tree.delete(*self.accessory_config_tree.get_children())

        source_section = source_data.get("accessory_items", {})
        source_items = source_section.get("items", []) if isinstance(source_section, dict) else []
        for item_index, definition in enumerate(source_items if isinstance(source_items, list) else []):
            if not isinstance(definition, dict):
                continue
            name = str(definition.get("name", "") or "").strip()
            accessory_type = str(definition.get("type", "") or "").strip()
            normalized_type = re.sub(r"[^a-z0-9]", "", accessory_type.lower())
            is_glasses = normalized_type.startswith(("goggle", "glasses", "eyewear"))
            is_glasses = is_glasses or any(token in name.lower() for token in ("goggle", "glasses", "eyewear"))
            if not name or not is_glasses:
                continue
            if direct_player_geometry:
                geo_path = source_path
                item_filename = re.sub(r"(?i)_geo_", "_item_", os.path.basename(source_path), count=1)
                item_candidate = os.path.join(os.path.dirname(source_path), item_filename)
                item_path = item_candidate if os.path.isfile(item_candidate) else ""
            else:
                geo_path, item_path = self.find_accessory_companions(source_path, name)
            if not geo_path and not item_path:
                continue
            iid = f"accessory:{item_index}"
            self.accessory_rows[iid] = {
                "definition": copy.deepcopy(definition),
                "geo_path": geo_path,
                "item_path": item_path,
                "geometry_lens_split": False,
                "direct_player_geometry": direct_player_geometry,
            }
            self.accessory_tree.insert(
                "",
                tk.END,
                iid=iid,
                text=name,
                values=(accessory_type or "custom", "Found" if geo_path else "Missing", "Found" if item_path else "Missing"),
            )

        target_section = target_data.get("accessory_items", {})
        configurations = target_section.get("configurations", []) if isinstance(target_section, dict) else []
        default_names = {
            str(target_section.get(key, "") or "")
            for key in ("default_config", "default_out_of_uniform_config", "default_out_of_uniform")
        }
        selected_defaults = []
        for config_index, configuration in enumerate(configurations if isinstance(configurations, list) else []):
            if not isinstance(configuration, dict):
                continue
            name = str(configuration.get("name", "") or f"Configuration {config_index + 1}")
            items = configuration.get("items", [])
            iid = f"config:{config_index}"
            self.accessory_config_rows[iid] = config_index
            is_default = name in default_names
            self.accessory_config_tree.insert(
                "",
                tk.END,
                iid=iid,
                text=name,
                values=(len(items) if isinstance(items, list) else 0, "Yes" if is_default else ""),
            )
            if is_default:
                selected_defaults.append(iid)

        accessory_children = self.accessory_tree.get_children()
        if accessory_children:
            self.accessory_tree.selection_set(accessory_children[0])
            self.accessory_tree.focus(accessory_children[0])
        config_children = self.accessory_config_tree.get_children()
        if selected_defaults:
            self.accessory_config_tree.selection_set(selected_defaults)
        elif config_children:
            self.accessory_config_tree.selection_set(config_children[0])

        self.accessory_status_var.set(
            f"Found {len(self.accessory_rows)} glasses package(s) and {len(self.accessory_config_rows)} target configurations."
        )
        if not self.accessory_rows:
            messagebox.showinfo(
                "Character Mod Tool",
                "No glasses definitions with matching geo/item IFFs were found beside the source player.",
            )

    def select_all_accessory_configurations(self):
        children = self.accessory_config_tree.get_children()
        if children:
            self.accessory_config_tree.selection_set(children)

    def add_selected_accessory(self):
        accessory_selection = self.accessory_tree.selection()
        config_selection = self.accessory_config_tree.selection()
        if not accessory_selection or accessory_selection[0] not in self.accessory_rows:
            messagebox.showinfo("Character Mod Tool", "Scan and select a custom accessory first.")
            return
        if not config_selection:
            messagebox.showinfo("Character Mod Tool", "Select at least one target configuration.")
            return
        source_path = self.accessory_source_var.get().strip()
        target_path = self.accessory_target_var.get().strip()
        source_prefix = self.player_iff_prefix(source_path)
        target_prefix = self.player_iff_prefix(target_path)
        if not source_prefix or not target_prefix:
            messagebox.showerror("Character Mod Tool", "Source and target filenames must begin with a png player number.")
            return

        row = self.accessory_rows[accessory_selection[0]]
        if row.get("direct_player_geometry"):
            messagebox.showinfo("Character Mod Tool", "A directly selected glasses geometry uses Bake Into Hihead.")
            return
        definition = copy.deepcopy(row["definition"])
        accessory_name = str(definition.get("name", "") or "").strip()
        geo_path = row.get("geo_path", "")
        item_path = row.get("item_path", "")
        if not geo_path:
            messagebox.showerror("Character Mod Tool", "The selected accessory is missing its geometry IFF.")
            return
        if definition.get("texture") and not item_path:
            messagebox.showerror("Character Mod Tool", "The selected textured accessory is missing its item/texture IFF.")
            return

        target_data = copy.deepcopy(self.accessory_target_data)
        target_section = target_data.setdefault("accessory_items", {})
        target_items = target_section.setdefault("items", [])
        existing_index = next(
            (
                index for index, item in enumerate(target_items)
                if isinstance(item, dict) and str(item.get("name", "")).lower() == accessory_name.lower()
            ),
            None,
        )
        if existing_index is None:
            target_items.append(definition)
        else:
            target_items[existing_index] = definition

        configurations = target_section.get("configurations", [])
        changed_configs = []
        for iid in config_selection:
            config_index = self.accessory_config_rows.get(iid)
            if config_index is None or config_index >= len(configurations):
                continue
            configuration = configurations[config_index]
            items = configuration.setdefault("items", [])
            if accessory_name not in items:
                items.append(accessory_name)
            changed_configs.append(str(configuration.get("name", iid)))
        if not changed_configs:
            messagebox.showinfo("Character Mod Tool", "No valid target configurations were selected.")
            return

        output_path = filedialog.asksaveasfilename(
            title="Save target player with custom accessory",
            defaultextension=".iff",
            initialfile=f"{target_prefix}_with_{accessory_name}.iff",
            initialdir=app_settings.ensure_output_dir(self.settings.get("output_dir", "")),
            filetypes=[("NBA 2K player IFF", "*.iff"), ("All files", "*.*")],
        )
        if not output_path:
            return
        if os.path.abspath(output_path) == os.path.abspath(target_path):
            messagebox.showerror("Character Mod Tool", "Save the accessory player as a new IFF first.")
            return

        output_dir = os.path.dirname(output_path) or os.getcwd()
        companion_sources = [path for path in (geo_path, item_path) if path]
        companion_outputs = []
        for companion_source in companion_sources:
            source_name = os.path.basename(companion_source)
            suffix = source_name[len(source_prefix):] if source_name.lower().startswith(source_prefix.lower()) else f"_{source_name}"
            companion_outputs.append(os.path.join(output_dir, target_prefix + suffix))
        existing_outputs = [path for path in companion_outputs if os.path.exists(path)]
        if existing_outputs and not messagebox.askyesno(
            "Replace accessory files?",
            "These target accessory files already exist and will be replaced:\n\n"
            + "\n".join(os.path.basename(path) for path in existing_outputs),
        ):
            return

        replacement_data = serialize_structured_entry(
            self.accessory_target_entry,
            self.accessory_target_original_data,
            target_data,
            self.accessory_target_wrapped,
        )
        try:
            self.write_iff_entry_replacement(
                target_path,
                output_path,
                self.accessory_target_entry,
                replacement_data,
            )
            for companion_source, companion_output in zip(companion_sources, companion_outputs):
                self.copy_accessory_companion(companion_source, companion_output)
        except Exception as exc:
            messagebox.showerror("Character Mod Tool", f"Could not build the accessory package:\n{exc}")
            return

        self.accessory_status_var.set(f"Added {accessory_name} to {len(changed_configs)} target configuration(s).")
        created = [output_path] + companion_outputs
        messagebox.showinfo(
            "Character Mod Tool",
            f"Added {accessory_name} successfully.\n\nCreated:\n" + "\n".join(created),
        )

    def bake_selected_accessory_into_hihead(self):
        accessory_selection = self.accessory_tree.selection()
        if not accessory_selection or accessory_selection[0] not in self.accessory_rows:
            messagebox.showinfo("Character Mod Tool", "Scan and select a custom accessory first.")
            return

        target_path = self.accessory_target_var.get().strip()
        target_prefix = self.player_iff_prefix(target_path)
        if not target_prefix:
            messagebox.showerror("Character Mod Tool", "The target filename must begin with a png player number.")
            return

        row = self.accessory_rows[accessory_selection[0]]
        definition = copy.deepcopy(row["definition"])
        accessory_name = str(definition.get("name", "") or "").strip()
        geo_path = row.get("geo_path", "")
        item_path = row.get("item_path", "")
        geometry_lens_split = bool(row.get("geometry_lens_split"))
        if not accessory_name or not geo_path:
            messagebox.showerror("Character Mod Tool", "The selected accessory is missing its name or geometry IFF.")
            return
        if definition.get("texture") and not item_path:
            messagebox.showerror("Character Mod Tool", "The selected textured accessory is missing its item IFF.")
            return

        target_data = copy.deepcopy(self.accessory_target_data)
        target_section = target_data.setdefault("accessory_items", {})
        target_items = target_section.setdefault("items", [])
        target_section["items"] = [
            item for item in target_items
            if not (
                isinstance(item, dict)
                and str(item.get("name", "")).lower() == accessory_name.lower()
            )
        ]
        for configuration in target_section.get("configurations", []):
            if isinstance(configuration, dict):
                configuration["items"] = [
                    item for item in configuration.get("items", [])
                    if str(item).lower() != accessory_name.lower()
                ]
                configuration.setdefault("torsotexture", {}).setdefault("color", "torso_color")

        output_path = filedialog.asksaveasfilename(
            title="Save experimental hihead accessory bake",
            defaultextension=".iff",
            initialfile=f"{target_prefix}_baked_{accessory_name}.iff",
            initialdir=app_settings.ensure_output_dir(self.settings.get("output_dir", "")),
            filetypes=[("NBA 2K player IFF", "*.iff"), ("All files", "*.*")],
        )
        if not output_path:
            return
        if os.path.abspath(output_path) == os.path.abspath(target_path):
            messagebox.showerror("Character Mod Tool", "Save the experiment as a new IFF.")
            return
        if not messagebox.askyesno(
            "Experimental hihead bake",
            "This will replace any glasses already baked into the hihead, merge the selected glasses, "
            "and build a BC7 torso/glasses texture atlas inside a new player IFF. Continue?",
        ):
            return

        appearance_data = serialize_structured_entry(
            self.accessory_target_entry,
            self.accessory_target_original_data,
            target_data,
            self.accessory_target_wrapped,
        )
        try:
            torso_vertex_count, accessory_vertex_count, frame_triangle_count, lens_triangle_count = (
                self.write_hihead_accessory_bake(
                target_path,
                output_path,
                geo_path,
                item_path,
                accessory_name,
                self.accessory_target_entry,
                appearance_data,
                self.accessory_frame_color_var.get(),
                geometry_lens_split,
            )
            )
        except Exception as exc:
            messagebox.showerror("Character Mod Tool", f"Could not build the hihead experiment:\n{exc}")
            return

        self.accessory_status_var.set(
            f"Baked {accessory_name} into one hihead with a BC7 torso texture atlas."
        )
        detail = (
            f"Torso vertices remapped: {torso_vertex_count}\n"
            f"Glasses vertices remapped: {accessory_vertex_count}\n"
            f"Frame triangles: {frame_triangle_count}\nClear lens triangles: {lens_triangle_count}\n"
            f"Texture atlas: BC7 torso_color_o"
        )
        messagebox.showinfo(
            "Character Mod Tool",
            f"Experimental hihead bake created successfully.\n\n{output_path}\n\n{detail}",
        )

    def open_head_swap_tool_folder(self):
        path = self.full_swap_tool_var.get().strip()
        if not os.path.isdir(path):
            messagebox.showinfo("Character Mod Tool", "Choose or detect the Blender Swap Tool first.")
            return
        try:
            os.startfile(path)
        except OSError as exc:
            messagebox.showerror("Character Mod Tool", f"Could not open the folder:\n{exc}")

    def open_full_swap_output_in_blender(self):
        output_path = self.last_full_swap_output
        blender_path = self.full_swap_blender_var.get().strip()
        tool_path = self.full_swap_tool_var.get().strip()
        if not output_path or not os.path.isfile(output_path):
            self.everything_swap_open_blender_button.configure(state=tk.DISABLED)
            messagebox.showinfo("Character Mod Tool", "Run a successful Full Swap before opening its output.")
            return
        if not os.path.isfile(blender_path) or not os.path.isfile(os.path.join(tool_path, "__init__.py")):
            messagebox.showerror("Character Mod Tool", "Blender or the bundled Head Swap Tool is not available.")
            return
        command = [
            blender_path,
            "--factory-startup",
            "--no-splash",
            "--python",
            OPEN_OUTPUT_BRIDGE,
            "--",
            "--iff",
            output_path,
            "--addon",
            tool_path,
        ]
        try:
            subprocess.Popen(command, cwd=os.path.dirname(OPEN_OUTPUT_BRIDGE))
        except OSError as exc:
            LOGGER.exception("Could not open Full Swap output in Blender")
            messagebox.showerror("Character Mod Tool", f"Could not start Blender:\n{exc}")
            return
        LOGGER.info("Opening Full Swap output in Blender: %s", output_path)
        self.full_swap_status_var.set("Opening the completed Full Swap output in Blender...")

    @staticmethod
    def is_running_as_administrator():
        try:
            return bool(ctypes.windll.shell32.IsUserAnAdmin())
        except (AttributeError, OSError):
            return False

    def validate_live_roster_cli(self):
        if not os.path.isfile(ROSTER_CLI):
            raise FileNotFoundError(f"The bundled Live Roster tool is missing:\n{ROSTER_CLI}")
        digest = hashlib.sha256(Path(ROSTER_CLI).read_bytes()).hexdigest().upper()
        if digest != ROSTER_CLI_SHA256:
            raise RuntimeError("The bundled Live Roster tool does not match the supported build.")

    def refresh_live_roster_tool_status(self):
        elevated = self.is_running_as_administrator()
        self.live_roster_admin_button.configure(
            text="Running as Administrator" if elevated else "Restart as Administrator",
            state=tk.DISABLED if elevated else tk.NORMAL,
        )
        try:
            self.validate_live_roster_cli()
            result = subprocess.run(
                [ROSTER_CLI, "--version"],
                capture_output=True,
                text=True,
                encoding="utf-8",
                errors="replace",
                timeout=5,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
            )
            version = (result.stdout or result.stderr).strip() or "version unavailable"
            if result.returncode != 0:
                raise RuntimeError(version)
        except (OSError, subprocess.TimeoutExpired, ValueError, RuntimeError) as exc:
            self.live_roster_status_var.set(f"Live Roster tool unavailable: {exc}")
            self.live_roster_read_button.configure(state=tk.DISABLED)
            self.live_roster_apply_button.configure(state=tk.DISABLED)
            return
        self.live_roster_read_button.configure(state=tk.NORMAL)
        self.live_roster_apply_button.configure(state=tk.NORMAL)
        admin_text = "Administrator" if elevated else "Standard user"
        self.live_roster_status_var.set(f"{version} ready | {admin_text}")

    def restart_as_administrator(self):
        if self.is_running_as_administrator():
            return
        if not messagebox.askyesno(
            "Character Mod Tool",
            "Restart Character Mod Tool as administrator?\n\nUnsaved in-app IFF changes will be lost.",
        ):
            return
        if getattr(sys, "frozen", False):
            executable = sys.executable
            arguments = sys.argv[1:]
        else:
            executable = sys.executable
            arguments = [str(Path(__file__).resolve()), *sys.argv[1:]]
        result = ctypes.windll.shell32.ShellExecuteW(
            None,
            "runas",
            executable,
            subprocess.list2cmdline(arguments),
            os.getcwd(),
            1,
        )
        if result <= 32:
            messagebox.showerror("Character Mod Tool", "Windows did not restart the app as administrator.")
            return
        self.destroy()

    def set_live_roster_output(self, text):
        self.live_roster_output_text.configure(state=tk.NORMAL)
        self.live_roster_output_text.delete("1.0", tk.END)
        self.live_roster_output_text.insert("1.0", text)
        self.live_roster_output_text.configure(state=tk.DISABLED)

    def set_live_roster_busy(self, busy):
        self.live_roster_busy = busy
        state = tk.DISABLED if busy else tk.NORMAL
        self.live_roster_read_button.configure(state=state)
        self.live_roster_apply_button.configure(state=state)

    def run_live_roster_command(self, arguments, on_success):
        if self.live_roster_busy:
            return
        try:
            self.validate_live_roster_cli()
        except (OSError, ValueError, RuntimeError) as exc:
            messagebox.showerror("Character Mod Tool", str(exc))
            return

        self.set_live_roster_busy(True)
        self.live_roster_status_var.set("Contacting NBA 2K26...")
        results = queue.Queue()

        def worker():
            try:
                completed = subprocess.run(
                    [ROSTER_CLI, "NBA2K26", "sp", *arguments],
                    capture_output=True,
                    text=True,
                    encoding="utf-8",
                    errors="replace",
                    timeout=15,
                    creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
                )
                output = "\n".join(
                    part.strip() for part in (completed.stdout, completed.stderr) if part and part.strip()
                )
                results.put((completed.returncode, output, None))
            except (OSError, subprocess.TimeoutExpired) as exc:
                results.put((1, "", exc))

        def poll():
            try:
                return_code, output, error = results.get_nowait()
            except queue.Empty:
                self.after(60, poll)
                return
            self.set_live_roster_busy(False)
            if error is not None or return_code != 0:
                detail = str(error) if error is not None else output
                detail = detail or f"Live Roster exited with code {return_code}."
                self.set_live_roster_output(detail)
                self.live_roster_status_var.set(detail.splitlines()[0])
                messagebox.showerror("Live Roster", detail)
                return
            on_success(output)

        threading.Thread(target=worker, daemon=True).start()
        self.after(60, poll)

    @staticmethod
    def parse_live_roster_output(output):
        clean_output = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", output)
        labels = dict(ROSTER_DISPLAY_FIELDS)
        aliases = {}
        for key, label in ROSTER_DISPLAY_FIELDS:
            aliases[re.sub(r"[^a-z0-9]", "", key.lower())] = key
            aliases[re.sub(r"[^a-z0-9]", "", label.lower())] = key

        parsed = {}
        for raw_line in clean_output.splitlines():
            line = raw_line.replace("│", "|").strip()
            cells = [cell.strip() for cell in line.split("|") if cell.strip()]
            for index, cell in enumerate(cells[:-1]):
                key = aliases.get(re.sub(r"[^a-z0-9]", "", cell.lower()))
                if key and key not in parsed:
                    value = cells[index + 1].strip()
                    if value.lower() != "value":
                        parsed[key] = value
            if ":" in line:
                name, value = line.split(":", 1)
                key = aliases.get(re.sub(r"[^a-z0-9]", "", name.lower()))
                if key and value.strip() and key not in parsed:
                    parsed[key] = value.strip()
        return [(key, labels[key], parsed[key]) for key, _label in ROSTER_DISPLAY_FIELDS if key in parsed]

    def read_live_roster_player(self):
        def complete(output):
            self.set_live_roster_output(output)
            rows = self.parse_live_roster_output(output)
            self.live_roster_tree.delete(*self.live_roster_tree.get_children())
            for key, label, value in rows:
                self.live_roster_tree.insert("", tk.END, iid=key, text=label, values=(value,))
            self.live_roster_status_var.set(f"Read {len(rows)} field(s) from the highlighted player.")

        self.run_live_roster_command(["get"], complete)

    def on_live_roster_field_selected(self, _event=None):
        selection = self.live_roster_tree.selection()
        if not selection:
            return
        field = selection[0]
        if field not in ROSTER_WRITABLE_FIELDS:
            return
        values = self.live_roster_tree.item(field, "values")
        self.live_roster_field_var.set(field)
        if values:
            self.live_roster_value_var.set(values[0])

    def apply_live_roster_value(self):
        field = self.live_roster_field_var.get().strip()
        value = self.live_roster_value_var.get().strip()
        if field not in ROSTER_WRITABLE_FIELDS:
            messagebox.showinfo("Live Roster", "Choose a supported player field.")
            return
        if not value:
            messagebox.showinfo("Live Roster", "Enter a new value.")
            return
        if field.endswith("ID") and not re.fullmatch(r"\d+", value):
            messagebox.showinfo("Live Roster", f"{field} must be a non-negative number.")
            return
        if not messagebox.askyesno(
            "Live Roster",
            f"Set {field} to {value} on the player currently highlighted in NBA 2K26?",
        ):
            return

        def complete(output):
            self.set_live_roster_output(output)
            self.live_roster_status_var.set(f"Updated {field} to {value}.")
            self.after(250, self.read_live_roster_player)

        self.run_live_roster_command(["set", field, value], complete)

    def load_hair_backend(self):
        if self.hair_backend is not None:
            return self.hair_backend
        if not os.path.isfile(HAIR_BACKEND_PATH):
            raise FileNotFoundError(f"The bundled Hair backend is missing:\n{HAIR_BACKEND_PATH}")
        if HAIR_TOOLS_DIR not in sys.path:
            sys.path.insert(0, HAIR_TOOLS_DIR)
        spec = importlib.util.spec_from_file_location("character_mod_hair_backend", HAIR_BACKEND_PATH)
        if spec is None or spec.loader is None:
            raise RuntimeError("Could not load the bundled Hair backend.")
        module = importlib.util.module_from_spec(spec)
        sys.modules[spec.name] = module
        spec.loader.exec_module(module)
        self.hair_backend = module
        self.configure_hair_backend()
        return module

    def refresh_hair_target_slots(self):
        self.hair_appearance_summary = None
        self.hair_slot_map = {}
        self.hair_tangent_map = {}
        self.hair_slot_var.set("")
        self.hair_slot_combo.configure(values=())
        if not self.file_path:
            self.hair_status_var.set("Open a player IFF to choose its hair slot.")
            return
        try:
            backend = self.load_hair_backend()
            summary = backend.parse_appearance_iff(backend.Path(self.file_path))
            slots = backend.appearance_asset_slots(summary, self.hair_asset_type_var.get())
        except Exception as exc:
            self.hair_status_var.set(f"Could not read hair slots: {exc}")
            return
        self.hair_appearance_summary = summary
        self.hair_slot_map = {label: key for label, key, _tangent in slots}
        self.hair_tangent_map = {label: tangent for label, _key, tangent in slots}
        labels = tuple(self.hair_slot_map)
        self.hair_slot_combo.configure(values=labels)
        if labels:
            default_label = next((label for label in labels if " default)" in label), labels[0])
            self.hair_slot_var.set(default_label)
            self.hair_status_var.set(f"Target png{summary.png_id}: choose an asset and install it.")
        else:
            asset_label = "hair" if self.hair_asset_type_var.get() == "hair" else "facial-hair"
            self.hair_status_var.set(f"The open player has no {asset_label} appearance slot.")

    def refresh_hair_catalog(self):
        try:
            backend = self.load_hair_backend()
            self.hair_status_var.set("Loading native hair catalog...")
            self.update_idletasks()
            self.hair_options = (
                backend.discover_hairs()
                if self.hair_asset_type_var.get() == "hair"
                else backend.discover_facial_hairs()
            )
        except Exception as exc:
            messagebox.showerror("Character Mod Tool", f"Could not load the hair catalog:\n{exc}")
            return
        self.hair_copy_config_var.set(False)
        self.hair_copy_config_check.configure(
            state=tk.NORMAL if self.hair_asset_type_var.get() == "hair" else tk.DISABLED
        )
        self.refresh_hair_target_slots()
        self.filter_hair_catalog()

    def filter_hair_catalog(self):
        query = self.hair_search_var.get().strip().lower()
        self.filtered_hair_options = [
            option for option in self.hair_options
            if not query or query in str(option).lower()
        ]
        self.hair_tree.delete(*self.hair_tree.get_children())
        for index, option in enumerate(self.filtered_hair_options):
            self.hair_tree.insert(
                "",
                tk.END,
                iid=f"hair:{index}",
                text=f"png{option.source_png}",
                values=(
                    option.player_name or "Unknown",
                    option.hair_key,
                    option.origin,
                    f"{option.lod_verts:,}" if option.lod_verts else "",
                    "Yes" if option.source_item or option.archive_item else "No",
                ),
            )
        if self.filtered_hair_options:
            self.hair_tree.selection_set("hair:0")
            self.hair_tree.focus("hair:0")
            self.show_selected_hair_details()
        else:
            self.hair_selection_var.set("No matching hair assets.")
        mode = "hair" if self.hair_asset_type_var.get() == "hair" else "facial-hair"
        self.hair_status_var.set(f"Showing {len(self.filtered_hair_options):,} native {mode} asset(s).")

    def selected_hair_option(self):
        selection = self.hair_tree.selection()
        if not selection:
            return None
        try:
            index = int(selection[0].split(":", 1)[1])
        except (IndexError, ValueError):
            return None
        return self.filtered_hair_options[index] if index < len(self.filtered_hair_options) else None

    def show_selected_hair_details(self):
        option = self.selected_hair_option()
        if option:
            config_text = "Available" if option.source_config or option.archive_config else "None"
            self.hair_selection_var.set(
                f"Selected: png{option.source_png} | {option.player_name or 'Unknown'} | "
                f"{option.hair_key} | Source config: {config_text}"
            )
        else:
            self.hair_selection_var.set("No hair selected.")

    def set_hair_install_status(self, text):
        self.hair_status_var.set(text)
        self.update_idletasks()

    def install_selected_hair(self):
        option = self.selected_hair_option()
        if option is None:
            messagebox.showinfo("Character Mod Tool", "Choose a hair asset first.")
            return
        if self.hair_appearance_summary is None:
            messagebox.showinfo("Character Mod Tool", "Open a player IFF with an appearance hair slot first.")
            return
        slot_label = self.hair_slot_var.get()
        target_key = self.hair_slot_map.get(slot_label, "")
        if not target_key:
            messagebox.showinfo("Character Mod Tool", "Choose the target appearance slot.")
            return
        backend = self.load_hair_backend()
        target_png = self.hair_appearance_summary.png_id
        target_tangent = self.hair_tangent_map.get(slot_label, False)
        tangent_staging = None
        install_option = option
        try:
            if option.asset_type == "hair":
                install_option, tangent_staging = backend.prepare_tangent_fitted_option(
                    option,
                    target_png,
                    status_callback=self.set_hair_install_status,
                )
            else:
                self.set_hair_install_status(f"Installing native tangent-space facial hair to png{target_png}...")
            target_geo, target_item, target_config, backups = backend.install_hair(
                install_option,
                target_png,
                target_key,
                self.hair_copy_item_var.get(),
                self.hair_copy_config_var.get(),
                target_tangent,
            )
        except Exception as exc:
            messagebox.showerror("Character Mod Tool", f"Hair installation failed:\n{exc}")
            self.hair_status_var.set("Hair installation did not complete.")
            return
        finally:
            if tangent_staging:
                shutil.rmtree(tangent_staging, ignore_errors=True)
        created = [str(target_geo)]
        if target_item:
            created.append(str(target_item))
        if target_config:
            created.append(str(target_config))
        self.hair_status_var.set(f"Installed {option.hair_key} to png{target_png} as {target_key}.")
        backup_text = "\n".join(str(path) for path in backups) if backups else "No existing files needed backup."
        messagebox.showinfo(
            "Character Mod Tool",
            "Hair installation completed.\n\nCreated:\n"
            + "\n".join(created)
            + "\n\nBackups:\n"
            + backup_text,
        )

    def convert_external_hair(self):
        if self.hair_asset_type_var.get() != "hair":
            messagebox.showinfo(
                "Character Mod Tool",
                "External conversion currently supports head hair, not facial hair.",
            )
            return
        if self.hair_appearance_summary is None:
            messagebox.showinfo(
                "Character Mod Tool",
                "Open a player IFF with an appearance hair slot first.",
            )
            return
        slot_label = self.hair_slot_var.get()
        target_key = self.hair_slot_map.get(slot_label, "")
        if not target_key:
            messagebox.showinfo("Character Mod Tool", "Choose the target appearance slot.")
            return

        source = filedialog.askopenfilename(
            title="Choose NBA 2K23 or NBA 2K25 hair geometry IFF",
            filetypes=[("NBA 2K hair IFF", "*.iff"), ("All files", "*.*")],
            initialdir=os.path.dirname(self.file_path) if self.file_path else "",
        )
        if not source:
            return

        backend = self.load_hair_backend()
        target_png = self.hair_appearance_summary.png_id
        try:
            plan = backend.external_hair_conversion_plan(
                backend.Path(source),
                target_png,
                target_key,
            )
        except Exception as exc:
            messagebox.showerror("Unsupported Hair", str(exc))
            return

        if not messagebox.askyesno(
            "Convert External Hair",
            f"{plan['generation']} geometry donor:\n{plan['source_path']}\n\n"
            f"2K26 target:\npng{target_png}_geo_{target_key}.iff\n\n"
            "The converter will use full-detail geometry, make it static on head bone 48, "
            "and retain the target slot's existing 2K26 item textures.\n\n"
            "Continue?",
        ):
            return

        self.set_hair_install_status(
            f"Converting {plan['generation']} {plan['source_hair_key']} to "
            f"png{target_png} {target_key}..."
        )
        try:
            output, backup, result, plan = backend.convert_external_hair_to_target(
                backend.Path(source),
                target_png,
                target_key,
            )
        except Exception as exc:
            messagebox.showerror("Hair Conversion Failed", str(exc))
            self.hair_status_var.set("External hair conversion did not complete.")
            return

        source_lod = result.get("source_lod", 0)
        vertices = result.get("vertices", result.get("stream_vertices", 0))
        self.hair_status_var.set(
            f"Converted {plan['generation']} hair to {output.name} "
            f"({vertices:,} source vertices)."
        )
        backup_text = str(backup) if backup else "No existing file needed backup."
        messagebox.showinfo(
            "Hair Converted",
            f"Created:\n{output}\n\n"
            f"Source: {plan['generation']} {plan['source_hair_key']}\n"
            f"Source LOD: {source_lod}\n"
            f"Static bone: 48\n\n"
            f"Backup:\n{backup_text}",
        )

    def open_iff(self):
        path = filedialog.askopenfilename(
            title="Open character IFF",
            filetypes=[("NBA 2K character IFF", "*.iff"), ("All files", "*.*")],
            initialdir=os.path.dirname(self.file_path) if self.file_path else "",
        )
        if not path:
            return
        self.load_iff(path)

    def browse_advanced_dynamic_body_iff(self):
        current = self.advanced_dynamic_body_iff_var.get().strip()
        path = filedialog.askopenfilename(
            title="Open Dynamic Body character IFF",
            filetypes=[("NBA 2K character IFF", "*.iff"), ("All files", "*.*")],
            initialdir=os.path.dirname(current or self.file_path) if (current or self.file_path) else "",
        )
        if not path:
            return
        self.load_iff(path)
        if self.file_path == path:
            self.inspect_dynamic_body(show_popup=False)

    def schedule_validation(self):
        if not self.file_path or not hasattr(self, "validator_tree"):
            return
        if self.validator_job is not None:
            try:
                self.after_cancel(self.validator_job)
            except tk.TclError:
                pass
        self.validator_job = self.after(150, self.run_validator)

    def run_validator(self):
        self.validator_job = None
        self.validator_tree.delete(*self.validator_tree.get_children())
        if not self.file_path:
            self.validator_results = []
            self.validator_status_var.set("Open an IFF to run game-ready checks.")
            return
        names = self.active_entry_names()
        data_by_name = {name: self.get_entry_data(name) for name in names}
        results = validate_archive_snapshot(names, data_by_name)
        if self.modified or self.removed:
            details = f"{len(self.modified)} edited and {len(self.removed)} removed entr{'y' if len(self.removed) == 1 else 'ies'} are not saved yet."
            results.append(ValidationResult("INFO", "Pending changes", details))
        self.validator_results = sorted(
            results,
            key=lambda item: ({"ERROR": 0, "WARNING": 1, "PASS": 2, "INFO": 3}.get(item.severity, 4), item.check.lower()),
        )
        for index, result in enumerate(self.validator_results):
            self.validator_tree.insert(
                "",
                tk.END,
                iid=f"validator_{index}",
                text=result.check,
                values=(result.severity, result.details),
                tags=(result.severity,),
            )
        errors = sum(result.severity == "ERROR" for result in self.validator_results)
        warnings = sum(result.severity == "WARNING" for result in self.validator_results)
        passes = sum(result.severity == "PASS" for result in self.validator_results)
        if errors:
            verdict = "NOT READY"
        elif warnings:
            verdict = "CHECK WARNINGS"
        else:
            verdict = "GAME READY"
        self.validator_status_var.set(f"{verdict}: {errors} error(s), {warnings} warning(s), {passes} passed check(s).")
        self.last_validation_report = {
            "file": self.file_path,
            "verdict": verdict,
            "errors": errors,
            "warnings": warnings,
            "passes": passes,
            "results": [
                {"severity": result.severity, "check": result.check, "details": result.details}
                for result in self.validator_results
            ],
        }
        LOGGER.info("Validation %s: %s", verdict, self.file_path)

    def validation_report_text(self):
        if not self.validator_results:
            return "No validation results.\n"
        lines = [
            f"Character Mod Tool v{APP_VERSION} - Game-Ready Validation",
            f"File: {self.file_path}",
            self.validator_status_var.get(),
            "",
        ]
        lines.extend(f"[{result.severity}] {result.check}: {result.details}" for result in self.validator_results)
        return "\n".join(lines) + "\n"

    def save_validation_report(self):
        if not self.file_path:
            messagebox.showinfo("Character Mod Tool", "Open an IFF and run validation first.")
            return
        self.run_validator()
        default = os.path.splitext(os.path.basename(self.file_path))[0] + "_validation.txt"
        path = filedialog.asksaveasfilename(
            title="Save validation report",
            defaultextension=".txt",
            initialfile=default,
            initialdir=app_settings.ensure_output_dir(self.settings.get("output_dir", "")),
            filetypes=[("Text report", "*.txt"), ("All files", "*.*")],
        )
        if not path:
            return
        try:
            with open(path, "w", encoding="utf-8", newline="\n") as handle:
                handle.write(self.validation_report_text())
        except Exception as exc:
            messagebox.showerror("Character Mod Tool", f"Could not save validation report:\n{exc}")
            return
        self.validator_status_var.set(f"Saved validation report: {os.path.basename(path)}")

    def load_iff(self, path):
        if not zipfile.is_zipfile(path):
            messagebox.showerror("Character Mod Tool", "This .iff is not a ZIP-style archive.")
            return

        entries = {}
        order = []
        try:
            with zipfile.ZipFile(path, "r") as archive:
                for info in archive.infolist():
                    if info.is_dir():
                        continue
                    data = archive.read(info.filename)
                    entries[info.filename] = ArchiveEntry(info.filename, data, info)
                    order.append(info.filename)
        except Exception as exc:
            messagebox.showerror("Character Mod Tool", f"Could not open file:\n{exc}")
            return

        self.file_path = path
        self.entries = entries
        self.entry_order = order
        self.modified = {}
        self.removed = set()
        self.current_name = ""
        self.face_iff_path = ""
        self.config_file_paths = []
        self.face_archive_rows = {}
        self.face_texture_rows = {}
        self.related_iff_mods = {}
        self.everything_swap_target_var.set(path)
        self.full_swap_target_var.set(path)
        self.accessory_target_var.set(path)
        self.tattoo_target_var.set(path)
        self.appearance_swap_target_var.set(path)
        self.face_swap_target_var.set(path)
        self.advanced_dynamic_body_iff_var.set(path)
        package_ready = os.path.isfile(DYNAMIC_BODY_SCNE) and os.path.isfile(DYNAMIC_BODY_MORPHS)
        self.advanced_dynamic_body_apply_button.configure(state=tk.NORMAL if package_ready else tk.DISABLED)
        self.advanced_dynamic_body_save_button.configure(state=tk.DISABLED)
        self.refresh_face_swap_config_choices()
        self.title(f"Character Mod Tool v{APP_VERSION} - {os.path.basename(path)}")
        self.status_var.set(f"Loaded {len(order)} entries from {os.path.basename(path)}")
        self.refresh_entries()
        self.show_appearance()
        self.show_tattoos()
        self.show_face()
        self.refresh_hair_target_slots()
        self.refresh_everything_swap_hair_options(auto_detect_source=False)
        self.refresh_full_swap_status()
        self.run_validator()
        self.inspect_dynamic_body(show_popup=False)

    def refresh_entries(self):
        query = self.search_var.get().strip().lower()
        selected = self.current_name
        self.entry_tree.delete(*self.entry_tree.get_children())
        for name in sorted(self.entry_order, key=self.archive_entry_display_key):
            if query and query not in name.lower():
                continue
            entry = self.entries[name]
            kind = self.classify_entry(entry)
            if name in self.removed:
                status = "removed"
            elif name in self.modified:
                status = "edited"
            else:
                status = ""
            iid = name
            self.entry_tree.insert("", tk.END, iid=iid, text=name, values=(kind, f"{entry.size:,}", status))
        if selected and self.entry_tree.exists(selected):
            self.entry_tree.selection_set(selected)
        self.schedule_validation()

    def archive_entry_display_key(self, name):
        base = os.path.basename(name).lower()
        original_index = self.entry_order.index(name) if name in self.entry_order else 999999
        if base.startswith("indexbuffer"):
            group = 10
        elif base.startswith("matrixweightsbuffer"):
            group = 11
        elif base.startswith("vertexbuffer"):
            group = 12
        elif base.startswith("morph."):
            group = 13
        else:
            group = 0
        return (group, base, original_index)

    def classify_entry(self, entry):
        if try_parse_structured_text(entry.name, self.get_entry_data(entry.name))[0] is not None:
            return "structured"
        if is_probably_text(entry.name, self.get_entry_data(entry.name)):
            return "text"
        return entry.ext

    def get_entry_data(self, name):
        return self.modified.get(name, self.entries[name].data)

    def active_entry_names(self):
        return [name for name in self.entry_order if name not in self.removed]

    def find_entry_name(self, target_name):
        target = target_name.lower()
        for name in self.entry_order:
            if name.lower() == target:
                return name
        return ""

    def appearance_entry_name(self):
        for candidate in APPEARANCE_ENTRY_NAMES:
            name = self.find_entry_name(candidate)
            if name:
                return name
        return ""

    def make_added_entry(self, name, data):
        info = zipfile.ZipInfo(name)
        info.compress_type = zipfile.ZIP_DEFLATED
        info.external_attr = 0o600 << 16
        return ArchiveEntry(name, data, info)

    def tattoo_texture_names(self):
        textures = []
        for name in self.active_entry_names():
            base = os.path.basename(name).lower()
            if base.endswith((".dds", ".txtr")) and any(base.startswith(prefix) for prefix in TATTOO_TEXTURE_PREFIXES):
                textures.append(name)
        return sorted(textures, key=self.texture_sort_key)

    @staticmethod
    def protected_tattoo_texture(name):
        base = os.path.basename(name).lower()
        return base.startswith("torso_color") and base.endswith((".dds", ".txtr"))

    @staticmethod
    def archive_texture_pair(entries, logical_name):
        logical = logical_name.lower()
        txtr_name = next((
            name for name in entries
            if os.path.basename(name).lower() == logical + ".txtr"
        ), "")
        dds_name = next((
            name for name in entries
            if os.path.basename(name).lower().endswith(".dds")
            and (
                os.path.basename(name).lower() == logical + ".dds"
                or os.path.basename(name).lower().startswith(logical + ".")
            )
        ), "")
        return txtr_name, dds_name

    @classmethod
    def renamed_texture_txtr(cls, txtr_data, target_dds_name):
        metadata = cls.parse_fragment_object(txtr_data)
        if len(metadata) != 1:
            raise ValueError("The source TXTR does not contain exactly one texture definition.")
        value = next(iter(metadata.values()))
        if not isinstance(value, dict):
            raise ValueError("The source TXTR texture definition is invalid.")
        value["Binary"] = os.path.basename(target_dds_name)
        return cls.serialize_fragment_object(metadata)

    @classmethod
    def synchronized_texture_txtr(cls, txtr_data, dds_data, target_dds_name):
        metadata = cls.parse_fragment_object(txtr_data)
        if len(metadata) != 1:
            raise ValueError("The source TXTR does not contain exactly one texture definition.")
        value = next(iter(metadata.values()))
        if not isinstance(value, dict):
            raise ValueError("The source TXTR texture definition is invalid.")
        dds = parse_dds_header(dds_data)
        for key in ("Segments", "CompressionMethod", "Twiddled"):
            value.pop(key, None)
        value["Binary"] = os.path.basename(target_dds_name)
        value["Width"] = int(dds["width"])
        value["Height"] = int(dds["height"])
        value["Mips"] = int(dds["mips"])
        value["Format"] = normalized_texture_format(dds["format"])
        value["HeaderSize"] = int(dds["header_size"])
        value["PixelDataSize"] = int(dds["pixel_data_size"])
        return cls.serialize_fragment_object(metadata)

    @classmethod
    def materialize_embedded_texture_txtrs(cls, entries):
        materialized = dict(entries)
        names = list(materialized)
        for txtr_name in (name for name in names if name.lower().endswith(".txtr")):
            logical_name = os.path.splitext(os.path.basename(txtr_name))[0]
            _paired_txtr, dds_name = cls.archive_texture_pair(names, logical_name)
            if not dds_name:
                continue
            materialized[txtr_name] = cls.synchronized_texture_txtr(
                materialized[txtr_name],
                materialized[dds_name],
                dds_name,
            )
        return materialized

    @classmethod
    def resize_dds_to_profile(cls, dds_data, width, height, texture_format, mips):
        texconv = cls.find_texconv_executable()
        if not texconv:
            raise ValueError(
                "Texture dimensions differ and the bundled tools\\texconv.exe converter was not found."
            )
        texture_format = normalized_texture_format(texture_format)
        with tempfile.TemporaryDirectory(prefix="character_mod_texture_resize_") as temp_dir:
            input_dir = os.path.join(temp_dir, "input")
            output_dir = os.path.join(temp_dir, "output")
            os.makedirs(input_dir, exist_ok=True)
            os.makedirs(output_dir, exist_ok=True)
            input_path = os.path.join(input_dir, "source.dds")
            with open(input_path, "wb") as handle:
                handle.write(dds_data)
            command = [
                texconv,
                "-nologo",
                "-y",
                "-f",
                texture_format,
                "-w",
                str(int(width)),
                "-h",
                str(int(height)),
                "-m",
                str(max(1, int(mips))),
                "-o",
                output_dir,
                input_path,
            ]
            result = subprocess.run(
                command,
                capture_output=True,
                text=True,
                errors="replace",
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
            )
            if result.returncode != 0:
                raise ValueError(
                    "Texture resize failed:\n" + (result.stderr or result.stdout).strip()
                )
            output_path = next((
                os.path.join(output_dir, name)
                for name in os.listdir(output_dir)
                if name.lower().endswith(".dds")
            ), "")
            if not output_path:
                raise ValueError("Texture resize completed without creating a DDS file.")
            with open(output_path, "rb") as handle:
                resized = handle.read()
        resized_header = parse_dds_header(resized)
        if (
            int(resized_header["width"]) != int(width)
            or int(resized_header["height"]) != int(height)
        ):
            raise ValueError(
                "Texture resize produced unexpected dimensions "
                f"{resized_header['width']}x{resized_header['height']}."
            )
        return resized

    @classmethod
    def prepare_texture_pair_for_target(
        cls,
        source_txtr_data,
        source_dds_data,
        target_txtr_data,
        target_dds_data,
        target_dds_name,
        preserve_source_profile=False,
    ):
        source_header = parse_dds_header(source_dds_data)
        target_header = parse_dds_header(target_dds_data)
        final_dds = source_dds_data
        profile_differs = (
            int(source_header["width"]) != int(target_header["width"])
            or int(source_header["height"]) != int(target_header["height"])
        )
        resized = profile_differs and not preserve_source_profile
        if resized:
            final_dds = cls.resize_dds_to_profile(
                source_dds_data,
                target_header["width"],
                target_header["height"],
                target_header["format"],
                target_header["mips"],
            )
        txtr_template = source_txtr_data
        if resized:
            source_metadata = cls.parse_fragment_object(source_txtr_data)
            target_metadata = cls.parse_fragment_object(target_txtr_data)
            if len(source_metadata) != 1 or len(target_metadata) != 1:
                raise ValueError("Texture TXTR metadata must contain exactly one definition.")
            source_value = next(iter(source_metadata.values()))
            target_value = next(iter(target_metadata.values()))
            if not isinstance(source_value, dict) or not isinstance(target_value, dict):
                raise ValueError("Texture TXTR definitions are invalid.")
            for key in ("Min", "Max", "TexelUsage"):
                if key in source_value:
                    target_value[key] = copy.deepcopy(source_value[key])
            txtr_template = cls.serialize_fragment_object(target_metadata)
        final_txtr = cls.synchronized_texture_txtr(
            txtr_template,
            final_dds,
            target_dds_name,
        )
        final_header = parse_dds_header(final_dds)
        LOGGER.info(
            "Prepared texture %s: %sx%s %s -> %sx%s %s%s",
            os.path.basename(target_dds_name),
            source_header["width"],
            source_header["height"],
            source_header["format"],
            final_header["width"],
            final_header["height"],
            final_header["format"],
            " (resized to target profile)" if resized else "",
        )
        return final_txtr, final_dds, resized

    def browse_tattoo_source(self):
        current = self.tattoo_source_var.get().strip()
        path = filedialog.askopenfilename(
            title="Choose legacy source character IFF",
            filetypes=[("NBA 2K character IFF", "*.iff"), ("All files", "*.*")],
            initialdir=os.path.dirname(current or self.file_path),
        )
        if path:
            self.tattoo_source_var.set(path)
            self.tattoo_status_var.set(f"Tattoo source selected: {os.path.basename(path)}")

    def browse_tattoo_target(self):
        current = self.tattoo_target_var.get().strip()
        path = filedialog.askopenfilename(
            title="Choose target character IFF",
            filetypes=[("NBA 2K character IFF", "*.iff"), ("All files", "*.*")],
            initialdir=os.path.dirname(current or self.file_path),
        )
        if path:
            self.load_iff(path)
            if os.path.abspath(self.file_path or "") == os.path.abspath(path):
                self.tattoo_status_var.set(f"Tattoo target loaded: {os.path.basename(path)}")

    def open_tattoo_folder(self):
        target_path = self.tattoo_target_var.get().strip() or self.file_path
        folder = os.path.dirname(target_path or "")
        if not folder:
            messagebox.showinfo("Character Mod Tool", "Choose a target character first.")
            return
        try:
            os.startfile(folder)
        except OSError as exc:
            messagebox.showerror("Character Mod Tool", f"Could not open the folder:\n{exc}")

    def swap_tattoos_from_legacy_iff(self):
        source_path = self.tattoo_source_var.get().strip()
        target_path = self.tattoo_target_var.get().strip()
        if not source_path:
            messagebox.showinfo("Character Mod Tool", "Choose the legacy source character at the top of the Tattoos tab.")
            return
        if not target_path:
            messagebox.showinfo("Character Mod Tool", "Choose the target character at the top of the Tattoos tab.")
            return
        if os.path.abspath(source_path) == os.path.abspath(target_path):
            messagebox.showinfo("Character Mod Tool", "Choose a different IFF as the legacy tattoo source.")
            return
        if os.path.abspath(self.file_path or "") != os.path.abspath(target_path):
            self.load_iff(target_path)
            if os.path.abspath(self.file_path or "") != os.path.abspath(target_path):
                return

        try:
            with zipfile.ZipFile(source_path, "r") as source_archive:
                source_names = source_archive.namelist()
                swaps = []
                removals = []
                skipped = []
                for logical_name in ("chest_color_o", "legs_color_o"):
                    source_txtr, source_dds = self.archive_texture_pair(source_names, logical_name)
                    target_txtr, target_dds = self.archive_texture_pair(self.active_entry_names(), logical_name)
                    if not source_txtr and not source_dds:
                        target_matches = [
                            name for name in self.entry_order
                            if self.is_saved_tattoo_texture(name)
                            and self.texture_logical_name(name) == logical_name
                        ]
                        if target_matches:
                            removals.append((logical_name, target_matches))
                        continue
                    if not source_txtr or not source_dds:
                        skipped.append(f"{logical_name}: source pair is incomplete")
                        continue
                    if not target_txtr or not target_dds:
                        skipped.append(f"{logical_name}: missing from target")
                        continue
                    txtr_data, dds_data, resized = self.prepare_texture_pair_for_target(
                        source_archive.read(source_txtr),
                        source_archive.read(source_dds),
                        self.get_entry_data(target_txtr),
                        self.get_entry_data(target_dds),
                        target_dds,
                        preserve_source_profile=True,
                    )
                    swaps.append((
                        logical_name,
                        target_txtr,
                        txtr_data,
                        target_dds,
                        dds_data,
                        resized,
                    ))
        except (OSError, zipfile.BadZipFile, KeyError, ValueError, UnicodeError, json.JSONDecodeError) as exc:
            messagebox.showerror("Character Mod Tool", f"Could not read legacy tattoo textures:\n{exc}")
            return

        if not swaps and not removals:
            details = "\n".join(skipped) or "No chest or legs texture pairs were found."
            messagebox.showinfo("Character Mod Tool", f"Nothing was swapped.\n\n{details}")
            return

        preview_lines = [
            f"Replace {logical}: TXTR + DDS" + (" (resize to target)" if resized else "")
            for logical, _target_txtr, _txtr_data, _target_dds, _dds_data, resized in swaps
        ]
        preview_lines.extend(
            f"Remove {logical}: {len(names)} target entries"
            for logical, names in removals
        )
        preview = "\n".join(preview_lines)
        if skipped:
            preview += "\n\nSkipped:\n" + "\n".join(skipped)
        if not messagebox.askyesno(
            "Swap Tattoos?",
            f"Apply these tattoo changes to the open target IFF?\n\n{preview}\n\n"
            "The target entry names will be preserved. torso_color will not be changed.",
        ):
            return

        for _logical, target_txtr, txtr_data, target_dds, dds_data, _resized in swaps:
            self.modified[target_txtr] = txtr_data
            self.modified[target_dds] = dds_data
            self.removed.discard(target_txtr)
            self.removed.discard(target_dds)
        for _logical, target_names in removals:
            for name in target_names:
                self.removed.add(name)
                self.modified.pop(name, None)

        self.refresh_entries()
        self.show_tattoos()
        replaced_names = [logical.replace("_color_o", "") for logical, *_rest in swaps]
        removed_names = [logical.replace("_color_o", "") for logical, _names in removals]
        result_parts = []
        if replaced_names:
            result_parts.append("replaced " + ", ".join(replaced_names))
        if removed_names:
            result_parts.append("removed " + ", ".join(removed_names))
        self.tattoo_status_var.set(
            f"Tattoo swap {'; '.join(result_parts)} from {os.path.basename(source_path)}. Use Save Tattoos."
        )
        self.status_var.set("Legacy tattoo swap applied in memory. Use Save Tattoos to write the target IFF.")
        messagebox.showinfo(
            "Tattoo Swap Complete",
            "Swap complete. Missing source chest/legs pairs were removed from the target. "
            "Click Save Tattoos to write only the chest and legs tattoo entries.",
        )

    @staticmethod
    def is_saved_tattoo_texture(name):
        base = os.path.basename(name).lower()
        return base.endswith((".dds", ".txtr")) and base.startswith(("chest_color", "legs_color"))

    def save_tattoos_only(self):
        if not self.file_path:
            messagebox.showinfo("Character Mod Tool", "Choose a target character first.")
            return
        changed_names = [
            name for name in self.entry_order
            if self.is_saved_tattoo_texture(name) and (name in self.modified or name in self.removed)
        ]
        if not changed_names:
            messagebox.showinfo("Character Mod Tool", "There are no pending chest or legs tattoo changes to save.")
            return

        base, ext = os.path.splitext(os.path.basename(self.file_path))
        output_path = filedialog.asksaveasfilename(
            title="Save tattoos",
            defaultextension=".iff",
            initialfile=f"{base}_tattoos{ext or '.iff'}",
            initialdir=app_settings.ensure_output_dir(self.settings.get("output_dir", "")),
            filetypes=[("NBA 2K character IFF", "*.iff"), ("All files", "*.*")],
        )
        if not output_path:
            return
        if os.path.abspath(output_path) == os.path.abspath(self.file_path):
            if not messagebox.askyesno(
                "Overwrite target IFF?",
                "Only chest and legs tattoo entries will change. Torso and every unrelated entry will remain unchanged. Continue?",
            ):
                return
        try:
            self.write_tattoos_only_iff(output_path)
        except Exception as exc:
            messagebox.showerror("Character Mod Tool", f"Could not save tattoos:\n{exc}")
            return

        if os.path.abspath(output_path) == os.path.abspath(self.file_path):
            for name in changed_names:
                if name in self.removed:
                    self.entries.pop(name, None)
                    if name in self.entry_order:
                        self.entry_order.remove(name)
                    self.removed.discard(name)
                    self.modified.pop(name, None)
                elif name in self.modified and name in self.entries:
                    self.entries[name].data = self.modified.pop(name)
            self.refresh_entries()
            self.show_tattoos()

        self.tattoo_status_var.set(
            f"Saved {os.path.basename(output_path)} with {len(changed_names)} tattoo entries changed."
        )
        self.status_var.set("Tattoo-only save complete. All unrelated IFF entries were copied unchanged.")
        messagebox.showinfo(
            "Tattoos Saved",
            f"Saved:\n{output_path}\n\nOnly chest and legs tattoo entries were changed.",
        )

    def write_tattoos_only_iff(self, output_path):
        output_dir = os.path.dirname(output_path)
        if output_dir:
            os.makedirs(output_dir, exist_ok=True)
        temp_handle = tempfile.NamedTemporaryFile(
            prefix=os.path.basename(output_path) + ".",
            suffix=".tmp",
            dir=output_dir or None,
            delete=False,
        )
        temp_path = temp_handle.name
        temp_handle.close()
        try:
            with zipfile.ZipFile(self.file_path, "r") as source, zipfile.ZipFile(temp_path, "w") as output:
                for info in source.infolist():
                    if info.is_dir():
                        output.writestr(self.copied_zip_info(info), b"")
                        continue
                    name = info.filename
                    if self.is_saved_tattoo_texture(name) and name in self.removed:
                        continue
                    data = source.read(name)
                    if self.is_saved_tattoo_texture(name) and name in self.modified:
                        data = self.modified[name]
                    output.writestr(self.copied_zip_info(info), data)
            os.replace(temp_path, output_path)
        except Exception:
            try:
                os.remove(temp_path)
            except OSError:
                pass
            raise

    def show_tattoos(self):
        self.tattoo_tree.delete(*self.tattoo_tree.get_children())
        if not self.file_path:
            self.tattoo_status_var.set("Open an .iff to scan for tattoo/body textures.")
            return

        textures = self.tattoo_texture_names()
        for name in textures:
            data = self.get_entry_data(name)
            ext = os.path.splitext(name)[1].lower().lstrip(".").upper()
            if self.protected_tattoo_texture(name):
                status = "Protected"
            else:
                status = f"{ext} edited" if name in self.modified else ext
            self.tattoo_tree.insert("", tk.END, iid=name, text=os.path.basename(name), values=(f"{len(data):,}", status))

        if textures:
            self.tattoo_status_var.set(f"Found {len(textures)} tattoo/body texture file(s).")
        else:
            self.tattoo_status_var.set("No chest_color, legs_color, or torso_color DDS/TXTR texture was found in this .iff.")

    def selected_tattoo_name(self):
        selection = self.tattoo_tree.selection()
        if not selection:
            messagebox.showinfo("Character Mod Tool", "Select a tattoo texture first.")
            return ""
        return selection[0]

    def photoshop_path(self):
        return app_settings.discover_photoshop(self.settings.get("photoshop_exe", ""))

    def export_texture_for_editing(self, name):
        os.makedirs(self.texture_export_dir, exist_ok=True)
        base = os.path.basename(name)
        target = os.path.join(self.texture_export_dir, base)
        with open(target, "wb") as handle:
            handle.write(self.get_entry_data(name))
        return target

    def open_selected_tattoo_texture(self):
        name = self.selected_tattoo_name()
        if not name:
            return
        if not name.lower().endswith(".dds"):
            messagebox.showinfo("Character Mod Tool", "Select a DDS texture to open externally. Use Edit TXTR Text for .TXTR rows.")
            return
        try:
            exported_path = self.export_texture_for_editing(name)
            photoshop = self.photoshop_path()
            if photoshop:
                subprocess.Popen([photoshop, exported_path])
                self.tattoo_status_var.set(f"Opened {os.path.basename(name)} in Photoshop.")
            else:
                os.startfile(exported_path)
                self.tattoo_status_var.set(f"Exported {os.path.basename(name)} and opened with the default app.")
        except Exception as exc:
            messagebox.showerror("Character Mod Tool", f"Could not open texture:\n{exc}")

    def replace_selected_tattoo_texture(self):
        name = self.selected_tattoo_name()
        if not name:
            return
        if not name.lower().endswith(".dds"):
            messagebox.showinfo("Character Mod Tool", "Select a DDS texture to replace. Use Edit TXTR Text for .TXTR rows.")
            return
        replacement_path = filedialog.askopenfilename(
            title="Choose replacement DDS texture",
            filetypes=[("DDS texture", "*.dds"), ("All files", "*.*")],
            initialdir=self.texture_export_dir if os.path.isdir(self.texture_export_dir) else os.path.dirname(self.file_path),
        )
        if not replacement_path:
            return
        if not replacement_path.lower().endswith(".dds"):
            if not messagebox.askyesno("Use Non-DDS File?", "The selected file does not end with .dds. Use it anyway?"):
                return
        try:
            with open(replacement_path, "rb") as handle:
                self.modified[name] = handle.read()
            self.removed.discard(name)
        except Exception as exc:
            messagebox.showerror("Character Mod Tool", f"Could not read replacement texture:\n{exc}")
            return

        self.refresh_entries()
        self.show_tattoos()
        if self.tattoo_tree.exists(name):
            self.tattoo_tree.selection_set(name)
        if self.current_name == name:
            data = self.get_entry_data(name)
            self.show_entry_hex(data)
            self.show_entry_raw(name, data)
        save_action = "the main Save As" if self.protected_tattoo_texture(name) else "Save Tattoos"
        self.tattoo_status_var.set(f"Replaced {os.path.basename(name)} in memory. Use {save_action}.")
        self.status_var.set(f"Texture replacement applied in memory. Use {save_action}.")

    def remove_selected_tattoo_pair(self):
        name = self.selected_tattoo_name()
        if not name:
            return
        if self.protected_tattoo_texture(name):
            messagebox.showinfo(
                "Character Mod Tool",
                "The torso texture is protected because baked glasses and accessories rely on its texture slot.",
            )
            self.tattoo_status_var.set("torso_color is protected and cannot be removed.")
            return
        logical_name = self.texture_logical_name(name)
        matches = [
            entry_name for entry_name in self.tattoo_texture_names()
            if self.texture_logical_name(entry_name) == logical_name
        ]
        if any(self.protected_tattoo_texture(entry_name) for entry_name in matches):
            messagebox.showinfo(
                "Character Mod Tool",
                "The torso texture pair is protected because baked glasses and accessories rely on it.",
            )
            self.tattoo_status_var.set("torso_color is protected and cannot be removed.")
            return
        if not matches:
            messagebox.showinfo("Character Mod Tool", "No matching tattoo texture pair was found.")
            return

        preview = "\n".join(os.path.basename(item) for item in matches)
        confirmed = messagebox.askyesno(
            "Remove Tattoo?",
            f"This will mark the selected tattoo texture pair for removal when you Save As.\n\n{preview}",
        )
        if not confirmed:
            return

        for entry_name in matches:
            self.removed.add(entry_name)
            self.modified.pop(entry_name, None)

        self.refresh_entries()
        self.show_tattoos()
        self.tattoo_status_var.set(f"Marked {len(matches)} tattoo texture file(s) for removal. Use Save Tattoos.")
        self.status_var.set("Tattoo texture pair marked for removal. Use Save Tattoos.")

    def edit_selected_tattoo_txtr(self):
        name = self.selected_tattoo_name()
        if not name:
            return
        if not name.lower().endswith(".txtr"):
            messagebox.showinfo("Character Mod Tool", "Select a .TXTR row to edit as text.")
            return

        try:
            text = self.get_entry_data(name).decode("utf-8-sig")
        except Exception as exc:
            messagebox.showerror("Character Mod Tool", f"Could not decode TXTR as text:\n{exc}")
            return

        editor = tk.Toplevel(self)
        editor.title(f"Edit {os.path.basename(name)}")
        editor.geometry("860x620")
        editor.minsize(620, 420)
        editor.transient(self)

        frame = ttk.Frame(editor, padding=8)
        frame.pack(fill=tk.BOTH, expand=True)

        ttk.Label(frame, text=name).pack(anchor=tk.W, pady=(0, 6))

        text_frame = ttk.Frame(frame)
        text_frame.pack(fill=tk.BOTH, expand=True)
        text_widget = tk.Text(text_frame, wrap=tk.NONE, undo=True)
        y_scroll = ttk.Scrollbar(text_frame, orient=tk.VERTICAL, command=text_widget.yview)
        x_scroll = ttk.Scrollbar(text_frame, orient=tk.HORIZONTAL, command=text_widget.xview)
        text_widget.configure(yscrollcommand=y_scroll.set, xscrollcommand=x_scroll.set)
        text_widget.grid(row=0, column=0, sticky="nsew")
        y_scroll.grid(row=0, column=1, sticky="ns")
        x_scroll.grid(row=1, column=0, sticky="ew")
        text_frame.rowconfigure(0, weight=1)
        text_frame.columnconfigure(0, weight=1)
        text_widget.insert("1.0", text)

        button_row = ttk.Frame(frame)
        button_row.pack(fill=tk.X, pady=(8, 0))

        def apply_txtr_text():
            new_text = text_widget.get("1.0", "end-1c")
            self.modified[name] = new_text.encode("utf-8")
            self.removed.discard(name)
            editor.destroy()
            self.refresh_entries()
            self.show_tattoos()
            if self.tattoo_tree.exists(name):
                self.tattoo_tree.selection_set(name)
            if self.current_name == name:
                data = self.get_entry_data(name)
                self.show_entry_raw(name, data)
                self.show_entry_json(name, data)
            save_action = "the main Save As" if self.protected_tattoo_texture(name) else "Save Tattoos"
            self.tattoo_status_var.set(f"Edited {os.path.basename(name)} in memory. Use {save_action}.")
            self.status_var.set(f"TXTR text edit applied in memory. Use {save_action}.")

        ttk.Button(button_row, text="Apply TXTR Text", command=apply_txtr_text).pack(side=tk.LEFT)
        ttk.Button(button_row, text="Cancel", command=editor.destroy).pack(side=tk.LEFT, padx=(6, 0))

    def on_tattoo_double_click(self, _event):
        name = self.selected_tattoo_name()
        if not name:
            return
        if name.lower().endswith(".txtr"):
            self.edit_selected_tattoo_txtr()
        elif name.lower().endswith(".dds"):
            self.open_selected_tattoo_texture()

    def current_iff_number(self):
        if not self.file_path:
            return ""
        stem = os.path.splitext(os.path.basename(self.file_path))[0]
        matches = re.findall(r"\d+", stem)
        return matches[-1] if matches else ""

    @staticmethod
    def character_number_from_path(path):
        stem = os.path.splitext(os.path.basename(path or ""))[0]
        match = re.search(r"(?i)png(\d+)", stem)
        if match:
            return match.group(1)
        matches = re.findall(r"\d+", stem)
        return matches[-1] if matches else ""

    @classmethod
    def config_files_for_character(cls, character_path):
        number = cls.character_number_from_path(character_path)
        folder = os.path.dirname(character_path or "")
        if not number or not os.path.isdir(folder):
            return []
        pattern = re.compile(rf"(?i)^png{re.escape(number)}_config(?:_.+)?\.iff$")
        return sorted(
            (
                os.path.join(folder, name)
                for name in os.listdir(folder)
                if pattern.fullmatch(name) and os.path.isfile(os.path.join(folder, name))
            ),
            key=lambda item: os.path.basename(item).lower(),
        )

    def face_config_options_for_character(self, character_path):
        loose_paths = self.config_files_for_character(character_path)
        loose_by_suffix = {}
        for path in loose_paths:
            match = re.search(r"(?i)_config_(.+)\.iff$", os.path.basename(path))
            if match:
                loose_by_suffix[match.group(1).lower()] = path

        options = {}
        used_paths = set()
        try:
            backend = self.load_hair_backend()
            summary = backend.parse_appearance_iff(backend.Path(character_path))
            folder = os.path.dirname(character_path)
            for configuration in summary.configs:
                config_name = str(configuration.name or "").strip()
                if not config_name:
                    continue
                suffix = re.sub(r"[^A-Za-z0-9_-]+", "_", config_name).strip("_") or "default"
                existing = loose_by_suffix.get(suffix.lower())
                path = existing or os.path.join(folder, f"png{summary.png_id}_config_{suffix.lower()}.iff")
                marks = []
                if config_name == summary.default_config:
                    marks.append("default")
                if not existing:
                    marks.append("create new")
                label = config_name + (f" ({', '.join(marks)})" if marks else "")
                options[label] = path
                if existing:
                    used_paths.add(os.path.abspath(existing).lower())
        except Exception:
            pass

        for path in loose_paths:
            if os.path.abspath(path).lower() in used_paths:
                continue
            options[os.path.basename(path)] = path
        return options

    def archived_face_config_entries(self, target_config):
        backend = self.load_hair_backend()
        archive_entry = f"char/sig/{os.path.basename(target_config)}"
        manifest = backend.parse_manifest_entries()
        if archive_entry.lower() not in manifest:
            raise ValueError(f"{os.path.basename(target_config)} was not found in the game manifest.")
        with tempfile.TemporaryDirectory(prefix="character_mod_face_config_") as temp_dir:
            extracted = backend.Path(temp_dir) / os.path.basename(target_config)
            backend.extract_clean_config_with_mod(archive_entry, extracted)
            with zipfile.ZipFile(extracted, "r") as archive:
                return {
                    info.filename: archive.read(info.filename)
                    for info in archive.infolist()
                    if not info.is_dir()
                }

    def refresh_face_swap_config_choices(self):
        character_path = self.face_swap_target_var.get().strip()
        variable = self.face_swap_target_config_var
        choices = self.face_config_options_for_character(character_path)
        previous = variable.get()
        self.face_swap_target_configs = choices
        labels = list(choices)
        self.face_swap_target_config_combo.configure(values=labels)
        if previous in choices:
            variable.set(previous)
        elif labels:
            variable.set(labels[0])
        else:
            variable.set("")

    def browse_face_swap_source(self):
        current = self.face_swap_source_var.get().strip()
        path = filedialog.askopenfilename(
            title="Choose legacy source face IFF",
            filetypes=[("NBA 2K character IFF", "*.iff"), ("All files", "*.*")],
            initialdir=os.path.dirname(current or self.file_path),
        )
        if path:
            self.face_swap_source_var.set(path)
            try:
                with zipfile.ZipFile(path, "r") as archive:
                    found = sum(
                        all(self.archive_texture_pair(archive.namelist(), logical))
                        for logical in ("face_color_o", "face_normal_o")
                    )
            except (OSError, zipfile.BadZipFile):
                found = 0
            self.face_status_var.set(
                f"Legacy face source selected: {os.path.basename(path)} ({found}/2 texture pairs found)."
            )

    def browse_face_swap_target(self):
        current = self.face_swap_target_var.get().strip()
        path = filedialog.askopenfilename(
            title="Choose target character IFF",
            filetypes=[("NBA 2K character IFF", "*.iff"), ("All files", "*.*")],
            initialdir=os.path.dirname(current or self.file_path),
        )
        if path:
            self.load_iff(path)
            if os.path.abspath(self.file_path or "") == os.path.abspath(path):
                self.face_status_var.set(
                    f"Found {len(self.face_swap_target_configs)} target config file(s) for {os.path.basename(path)}."
                )

    def swap_selected_face_config_textures(self):
        source_config = self.face_swap_source_var.get().strip()
        source_label = os.path.basename(source_config)
        target_label = self.face_swap_target_config_var.get().strip()
        target_config = self.face_swap_target_configs.get(target_label, "")
        if not source_config:
            messagebox.showinfo("Character Mod Tool", "Choose a legacy source face IFF first.")
            return
        if not target_config:
            messagebox.showinfo("Character Mod Tool", "Choose a target character and target config first.")
            return
        if os.path.abspath(source_config) == os.path.abspath(target_config):
            messagebox.showinfo("Character Mod Tool", "Choose different source and target config files.")
            return

        creating_target = not os.path.isfile(target_config)
        target_base_entries = {}
        target_existing_entries = {}
        try:
            with zipfile.ZipFile(source_config, "r") as source_archive:
                source_names = source_archive.namelist()
                if creating_target:
                    target_base_entries = self.archived_face_config_entries(target_config)
                    target_names = list(target_base_entries)
                else:
                    with zipfile.ZipFile(target_config, "r") as target_archive:
                        target_existing_entries = {
                            info.filename: target_archive.read(info.filename)
                            for info in target_archive.infolist()
                            if not info.is_dir()
                        }
                        target_names = list(target_existing_entries)
                required_target_maps = (
                    "face_bentnormal_o",
                    "face_detailnormal_o",
                    "face_wrinklecolor_o",
                    "face_wrinklenormal_o",
                )
                target_entries = dict(target_existing_entries)
                target_entries.update(target_base_entries)

                def incomplete_target_map(logical_name):
                    txtr_name, dds_name = self.archive_texture_pair(list(target_entries), logical_name)
                    if not txtr_name or not dds_name:
                        return True
                    try:
                        self.parse_fragment_object(target_entries[txtr_name])
                    except (ValueError, UnicodeError, json.JSONDecodeError):
                        return True
                    return False

                missing_target_maps = [
                    logical_name for logical_name in required_target_maps
                    if incomplete_target_map(logical_name)
                ]
                if missing_target_maps and not creating_target:
                    archived_entries = self.archived_face_config_entries(target_config)
                    target_base_entries.update(
                        (name, data)
                        for name, data in archived_entries.items()
                        if self.texture_logical_name(name) in required_target_maps
                    )
                    target_entries.update(target_base_entries)
                    target_names = list(target_entries)
                    missing_target_maps = [
                        logical_name for logical_name in required_target_maps
                        if incomplete_target_map(logical_name)
                    ]
                if missing_target_maps:
                    raise ValueError(
                        "The target config is missing complete editable TXTR/DDS pairs for: "
                        + ", ".join(missing_target_maps)
                    )
                swaps = []
                skipped = []
                for logical_name in ("face_color_o", "face_normal_o"):
                    source_txtr, source_dds = self.archive_texture_pair(source_names, logical_name)
                    if not source_txtr or not source_dds:
                        skipped.append(f"{logical_name}: missing from source face IFF")
                        continue
                    target_txtr, target_dds = self.archive_texture_pair(target_names, logical_name)
                    if not target_txtr:
                        skipped.append(f"{logical_name}: TXTR missing from target config")
                        continue
                    source_txtr_data = source_archive.read(source_txtr)
                    source_dds_data = source_archive.read(source_dds)
                    if target_dds:
                        final_txtr, final_dds, resized = self.prepare_texture_pair_for_target(
                            source_txtr_data,
                            source_dds_data,
                            target_entries[target_txtr],
                            target_entries[target_dds],
                            target_dds,
                            preserve_source_profile=True,
                        )
                    else:
                        target_dds = os.path.basename(source_dds)
                        final_dds = source_dds_data
                        final_txtr = self.synchronized_texture_txtr(
                            source_txtr_data,
                            final_dds,
                            target_dds,
                        )
                        resized = False
                    swaps.append((
                        logical_name,
                        target_txtr,
                        final_txtr,
                        target_dds,
                        final_dds,
                        resized,
                    ))
        except (OSError, zipfile.BadZipFile, KeyError, ValueError, UnicodeError, json.JSONDecodeError) as exc:
            messagebox.showerror("Character Mod Tool", f"Could not prepare the selected face texture swap:\n{exc}")
            return

        if not swaps:
            messagebox.showinfo("Character Mod Tool", "Nothing was swapped.\n\n" + "\n".join(skipped))
            return
        preview = "\n".join(
            f"{logical}: TXTR + DDS" + (" (resize to target)" if resized else "")
            for logical, _target_txtr, _txtr_data, _target_dds, _dds_data, resized in swaps
        )
        if skipped:
            preview += "\n\nSkipped:\n" + "\n".join(skipped)
        if not messagebox.askyesno(
            "Swap Face Textures?",
            f"Source: {source_label}\nTarget: {target_label}\n\n{preview}\n\n"
            "Only face_color and face_normal will be changed. The target bent normal, detail normal, "
            "wrinkle color, and wrinkle normal will be retained."
            + (f"\n\nA new {os.path.basename(target_config)} will be created from the original game config." if creating_target else ""),
        ):
            return

        mods = self.related_iff_mods.setdefault(target_config, {})
        if target_base_entries:
            mods.update(target_base_entries)
        for _logical, target_txtr, txtr_data, target_dds, dds_data, _resized in swaps:
            mods[target_txtr] = txtr_data
            mods[target_dds] = dds_data
        materialized_entries = dict(target_entries)
        materialized_entries.update(mods)
        materialized_entries = self.materialize_embedded_texture_txtrs(materialized_entries)
        mods.update(
            (name, data)
            for name, data in materialized_entries.items()
            if name.lower().endswith(".txtr")
        )

        self.show_face()
        for row_id, archive_path in self.face_archive_rows.items():
            if os.path.abspath(archive_path) == os.path.abspath(target_config):
                self.face_tree.selection_set(row_id)
                self.face_tree.focus(row_id)
                self.face_tree.see(row_id)
                break
        changed = ", ".join(logical.replace("face_", "").replace("_o", "") for logical, *_rest in swaps)
        self.face_status_var.set(
            f"Swapped {changed} into {target_label}. Use Save Face Config to write the target config."
        )
        messagebox.showinfo(
            "Face Texture Swap Complete",
            "Swap complete. Click Save Face Config to write the rebuilt config.",
        )

    def find_matching_face_iff(self):
        self.face_iff_path = ""
        number = self.current_iff_number()
        if not self.file_path or not number:
            return ""

        folder = os.path.dirname(self.file_path)
        exact = os.path.join(folder, f"face{number}.iff")
        if os.path.exists(exact):
            self.face_iff_path = exact
            return exact

        candidates = []
        for path in glob.glob(os.path.join(folder, "face*.iff")):
            stem = os.path.splitext(os.path.basename(path))[0].lower()
            if stem.startswith("face") and number in stem:
                candidates.append(path)
        if candidates:
            self.face_iff_path = sorted(candidates, key=lambda item: (len(os.path.basename(item)), item.lower()))[0]
        return self.face_iff_path

    def find_matching_config_files(self):
        self.config_file_paths = self.config_files_for_character(self.file_path)
        number = self.current_iff_number()
        folder = os.path.abspath(os.path.dirname(self.file_path or ""))
        for path, mods in self.related_iff_mods.items():
            if not mods or os.path.exists(path):
                continue
            if os.path.abspath(os.path.dirname(path)) != folder:
                continue
            if re.fullmatch(rf"(?i)png{re.escape(number)}_config(?:_.+)?\.iff", os.path.basename(path)):
                self.config_file_paths.append(path)
        self.config_file_paths.sort(key=lambda item: os.path.basename(item).lower())
        return self.config_file_paths

    def set_face_text(self, text):
        self.face_tree.delete(*self.face_tree.get_children())
        for index, line in enumerate(text.splitlines() or [""]):
            self.face_tree.insert("", tk.END, iid=f"face_message_{index}", text=line, values=("", ""))

    def texture_sort_key(self, filename):
        base = os.path.basename(filename)
        lower = base.lower()
        if lower.endswith(".txtr"):
            logical_name = lower[:-5]
            ext_order = 0
        elif lower.endswith(".dds"):
            stem = lower[:-4]
            parts = stem.rsplit(".", 1)
            if len(parts) == 2 and re.fullmatch(r"[0-9a-f]{8,32}", parts[1]):
                logical_name = parts[0]
            else:
                logical_name = stem
            ext_order = 1
        else:
            logical_name = lower
            ext_order = 9
        return (logical_name, ext_order, lower)

    def texture_logical_name(self, filename):
        base = os.path.basename(filename).lower()
        if base.endswith(".txtr"):
            return base[:-5]
        if base.endswith(".dds"):
            stem = base[:-4]
            parts = stem.rsplit(".", 1)
            if len(parts) == 2 and re.fullmatch(r"[0-9a-f]{8,32}", parts[1]):
                return parts[0]
            return stem
        return base

    def add_related_iff_group(self, path, group_type):
        parent_id = f"face_group_{len(self.face_tree.get_children())}"
        pending = self.related_iff_mods.get(path, {})
        is_new = not os.path.isfile(path)
        size = sum(len(data) for data in pending.values()) if is_new else os.path.getsize(path)
        modified_count = len(pending)
        label = os.path.basename(path)
        if is_new and modified_count:
            label = f"{label} (new config, {modified_count} entries pending)"
        elif modified_count:
            label = f"{label} ({modified_count} replacement pending)"
        parent = self.face_tree.insert(
            "",
            tk.END,
            iid=parent_id,
            text=label,
            values=(group_type, f"{size:,}"),
            open=True,
        )
        self.face_archive_rows[parent] = path
        if is_new and pending:
            self.face_tree.insert(parent, tk.END, text="New config archive", values=("info", ""))
            texture_folder = self.face_tree.insert(
                parent, tk.END, text=f"Textures ({len(pending)})", values=("group", ""), open=True
            )
            for entry_name in sorted(pending, key=self.texture_sort_key):
                data = pending[entry_name]
                ext = os.path.splitext(entry_name)[1].lower().lstrip(".").upper()
                texture_id = f"face_texture_{len(self.face_texture_rows)}"
                self.face_texture_rows[texture_id] = (path, entry_name)
                self.face_tree.insert(
                    texture_folder,
                    tk.END,
                    iid=texture_id,
                    text=entry_name,
                    values=(f"{ext} added", f"{len(data):,}"),
                )
            return len(pending)
        if not zipfile.is_zipfile(path):
            self.face_tree.insert(parent, tk.END, text="Not a ZIP-style .iff archive.", values=("status", ""))
            return 0

        try:
            with zipfile.ZipFile(path, "r") as archive:
                entries = [info for info in archive.infolist() if not info.is_dir()]
                texture_entries = [
                    info for info in entries
                    if os.path.basename(info.filename).lower().endswith((".dds", ".txtr"))
                ]
                texture_entries.sort(key=lambda info: self.texture_sort_key(info.filename))
                self.face_tree.insert(parent, tk.END, text=f"Archive entries: {len(entries)}", values=("info", ""))
                if texture_entries:
                    texture_folder = self.face_tree.insert(parent, tk.END, text=f"Textures ({len(texture_entries)})", values=("group", ""), open=True)
                    for info in texture_entries[:120]:
                        ext = os.path.splitext(info.filename)[1].lower().lstrip(".").upper()
                        data = self.related_iff_mods.get(path, {}).get(info.filename)
                        file_size = len(data) if data is not None else info.file_size
                        status = f"{ext} replaced" if data is not None else ext
                        texture_id = f"face_texture_{len(self.face_texture_rows)}"
                        self.face_texture_rows[texture_id] = (path, info.filename)
                        self.face_tree.insert(
                            texture_folder,
                            tk.END,
                            iid=texture_id,
                            text=info.filename,
                            values=(status, f"{file_size:,}"),
                        )
                    if len(texture_entries) > 120:
                        self.face_tree.insert(texture_folder, tk.END, text=f"...and {len(texture_entries) - 120} more", values=("info", ""))
                else:
                    self.face_tree.insert(parent, tk.END, text="No .dds or .txtr textures found.", values=("info", ""))
                return len(texture_entries)
        except Exception as exc:
            self.face_tree.insert(parent, tk.END, text=f"Could not inspect archive: {exc}", values=("error", ""))
        return 0

    def show_face(self):
        path = self.find_matching_face_iff()
        config_paths = self.find_matching_config_files()
        number = self.current_iff_number()
        self.face_tree.delete(*self.face_tree.get_children())
        self.face_archive_rows = {}
        self.face_texture_rows = {}
        if not self.file_path:
            self.face_status_var.set("Open a player .iff to find matching face/config files.")
            self.set_face_text("Open a player .iff first.")
            return
        if not number:
            self.face_status_var.set("No player number was found in the current .iff name.")
            self.set_face_text(f"Current file has no number in its name:\n{os.path.basename(self.file_path)}")
            return

        texture_total = 0
        if path:
            texture_total += self.add_related_iff_group(path, "Face IFF")

        for config_path in config_paths:
            texture_total += self.add_related_iff_group(config_path, "Config IFF")

        if path and config_paths:
            self.face_status_var.set(f"Found {os.path.basename(path)}, {len(config_paths)} config file(s), and {texture_total} texture(s).")
        elif path:
            self.face_status_var.set(f"Found {os.path.basename(path)} with {texture_total} texture(s). No matching config files found.")
        elif config_paths:
            self.face_status_var.set(f"Found {len(config_paths)} config file(s) with {texture_total} texture(s). No matching face .iff found.")
        else:
            self.face_status_var.set(f"No matching face/config files found for {number}.")

    def selected_face_texture(self):
        selection = self.face_tree.selection()
        if not selection:
            messagebox.showinfo("Character Mod Tool", "Select a DDS texture under a face/config file first.")
            return None
        row_id = selection[0]
        texture = self.face_texture_rows.get(row_id)
        if not texture:
            messagebox.showinfo("Character Mod Tool", "Select a DDS texture row, not the file group.")
            return None
        archive_path, entry_name = texture
        if not entry_name.lower().endswith(".dds"):
            messagebox.showinfo("Character Mod Tool", "Only DDS texture rows can be opened or replaced here.")
            return None
        return texture

    def selected_face_entry(self):
        selection = self.face_tree.selection()
        if not selection:
            messagebox.showinfo("Character Mod Tool", "Select a texture row under a face/config file first.")
            return None
        row_id = selection[0]
        entry = self.face_texture_rows.get(row_id)
        if not entry:
            messagebox.showinfo("Character Mod Tool", "Select a texture row, not the file group.")
            return None
        return entry

    def selected_related_archive_path(self):
        selection = self.face_tree.selection()
        if not selection:
            return ""
        row_id = selection[0]
        if row_id in self.face_archive_rows:
            return self.face_archive_rows[row_id]
        texture = self.face_texture_rows.get(row_id)
        if texture:
            return texture[0]
        parent = self.face_tree.parent(row_id)
        while parent:
            if parent in self.face_archive_rows:
                return self.face_archive_rows[parent]
            parent = self.face_tree.parent(parent)
        return ""

    def read_related_iff_entry(self, archive_path, entry_name):
        data = self.related_iff_mods.get(archive_path, {}).get(entry_name)
        if data is not None:
            return data
        with zipfile.ZipFile(archive_path, "r") as archive:
            return archive.read(entry_name)

    def export_face_texture_for_editing(self, archive_path, entry_name):
        archive_stem = os.path.splitext(os.path.basename(archive_path))[0]
        export_dir = os.path.join(self.texture_export_dir, archive_stem)
        os.makedirs(export_dir, exist_ok=True)
        target = os.path.join(export_dir, os.path.basename(entry_name))
        with open(target, "wb") as handle:
            handle.write(self.read_related_iff_entry(archive_path, entry_name))
        return target

    def open_selected_face_texture(self):
        texture = self.selected_face_texture()
        if not texture:
            return
        archive_path, entry_name = texture
        try:
            exported_path = self.export_face_texture_for_editing(archive_path, entry_name)
            photoshop = self.photoshop_path()
            if photoshop:
                subprocess.Popen([photoshop, exported_path])
                self.face_status_var.set(f"Opened {os.path.basename(entry_name)} in Photoshop.")
            else:
                os.startfile(exported_path)
                self.face_status_var.set(f"Exported {os.path.basename(entry_name)} and opened with the default app.")
        except Exception as exc:
            messagebox.showerror("Character Mod Tool", f"Could not open texture:\n{exc}")

    def replace_selected_face_texture(self):
        texture = self.selected_face_texture()
        if not texture:
            return
        archive_path, entry_name = texture
        replacement_path = filedialog.askopenfilename(
            title="Choose replacement DDS texture",
            filetypes=[("DDS texture", "*.dds"), ("All files", "*.*")],
            initialdir=self.texture_export_dir if os.path.isdir(self.texture_export_dir) else os.path.dirname(archive_path),
        )
        if not replacement_path:
            return
        if not replacement_path.lower().endswith(".dds"):
            if not messagebox.askyesno("Use Non-DDS File?", "The selected file does not end with .dds. Use it anyway?"):
                return
        try:
            with open(replacement_path, "rb") as handle:
                self.related_iff_mods.setdefault(archive_path, {})[entry_name] = handle.read()
        except Exception as exc:
            messagebox.showerror("Character Mod Tool", f"Could not read replacement texture:\n{exc}")
            return

        self.show_face()
        self.face_status_var.set(
            f"Replaced {os.path.basename(entry_name)} in {os.path.basename(archive_path)}. Use Save Face Config."
        )

    def edit_selected_face_txtr(self):
        entry = self.selected_face_entry()
        if not entry:
            return
        archive_path, entry_name = entry
        if not entry_name.lower().endswith(".txtr"):
            messagebox.showinfo("Character Mod Tool", "Select a .TXTR row to edit as text.")
            return

        try:
            text = self.read_related_iff_entry(archive_path, entry_name).decode("utf-8-sig")
        except Exception as exc:
            messagebox.showerror("Character Mod Tool", f"Could not decode TXTR as text:\n{exc}")
            return

        editor = tk.Toplevel(self)
        editor.title(f"Edit {os.path.basename(entry_name)}")
        editor.geometry("860x620")
        editor.minsize(620, 420)
        editor.transient(self)

        frame = ttk.Frame(editor, padding=8)
        frame.pack(fill=tk.BOTH, expand=True)

        label = ttk.Label(frame, text=f"{os.path.basename(archive_path)} / {entry_name}")
        label.pack(anchor=tk.W, pady=(0, 6))

        text_frame = ttk.Frame(frame)
        text_frame.pack(fill=tk.BOTH, expand=True)
        text_widget = tk.Text(text_frame, wrap=tk.NONE, undo=True)
        y_scroll = ttk.Scrollbar(text_frame, orient=tk.VERTICAL, command=text_widget.yview)
        x_scroll = ttk.Scrollbar(text_frame, orient=tk.HORIZONTAL, command=text_widget.xview)
        text_widget.configure(yscrollcommand=y_scroll.set, xscrollcommand=x_scroll.set)
        text_widget.grid(row=0, column=0, sticky="nsew")
        y_scroll.grid(row=0, column=1, sticky="ns")
        x_scroll.grid(row=1, column=0, sticky="ew")
        text_frame.rowconfigure(0, weight=1)
        text_frame.columnconfigure(0, weight=1)
        text_widget.insert("1.0", text)

        button_row = ttk.Frame(frame)
        button_row.pack(fill=tk.X, pady=(8, 0))

        def apply_txtr_text():
            new_text = text_widget.get("1.0", "end-1c")
            self.related_iff_mods.setdefault(archive_path, {})[entry_name] = new_text.encode("utf-8")
            editor.destroy()
            self.show_face()
            self.face_status_var.set(
                f"Edited {os.path.basename(entry_name)} in {os.path.basename(archive_path)}. Use Save Face Config."
            )

        ttk.Button(button_row, text="Apply TXTR Text", command=apply_txtr_text).pack(side=tk.LEFT)
        ttk.Button(button_row, text="Cancel", command=editor.destroy).pack(side=tk.LEFT, padx=(6, 0))

    def write_related_iff(self, source_path, output_path):
        mods = self.related_iff_mods.get(source_path, {})
        if not mods:
            raise ValueError("No pending replacements for this related .iff.")
        output_dir = os.path.dirname(output_path)
        if output_dir:
            os.makedirs(output_dir, exist_ok=True)

        tmp_handle = tempfile.NamedTemporaryFile(
            prefix=os.path.basename(output_path) + ".",
            suffix=".tmp",
            dir=output_dir or None,
            delete=False,
        )
        tmp_path = tmp_handle.name
        tmp_handle.close()

        try:
            if not os.path.isfile(source_path):
                with zipfile.ZipFile(tmp_path, "w") as dst:
                    for name in sorted(mods, key=self.texture_sort_key):
                        info = zipfile.ZipInfo(name)
                        info.compress_type = zipfile.ZIP_DEFLATED
                        info.external_attr = 0o600 << 16
                        dst.writestr(info, mods[name])
                os.replace(tmp_path, output_path)
                return
            with zipfile.ZipFile(source_path, "r") as src, zipfile.ZipFile(tmp_path, "w") as dst:
                written_names = set()
                for info in src.infolist():
                    out_info = zipfile.ZipInfo(filename=info.filename, date_time=info.date_time)
                    out_info.comment = info.comment
                    out_info.extra = info.extra
                    out_info.internal_attr = info.internal_attr
                    out_info.external_attr = info.external_attr
                    out_info.create_system = info.create_system
                    out_info.flag_bits = info.flag_bits
                    out_info.compress_type = info.compress_type or zipfile.ZIP_DEFLATED

                    if info.is_dir():
                        dst.writestr(out_info, b"")
                        written_names.add(info.filename.lower())
                        continue

                    data = mods.get(info.filename)
                    if data is None:
                        data = src.read(info.filename)
                    dst.writestr(out_info, data)
                    written_names.add(info.filename.lower())

                for name in sorted(mods, key=self.texture_sort_key):
                    if name.lower() in written_names:
                        continue
                    info = zipfile.ZipInfo(name)
                    info.compress_type = zipfile.ZIP_DEFLATED
                    info.external_attr = 0o600 << 16
                    dst.writestr(info, mods[name])

            os.replace(tmp_path, output_path)
        except Exception:
            try:
                os.remove(tmp_path)
            except OSError:
                pass
            raise

    def save_selected_related_iff_as(self):
        archive_path = self.selected_related_archive_path()
        if not archive_path:
            messagebox.showinfo("Character Mod Tool", "Select a related file or one of its texture rows first.")
            return
        self.save_related_iff_as(archive_path)

    def save_target_face_config_as(self):
        target_label = self.face_swap_target_config_var.get().strip()
        archive_path = self.face_swap_target_configs.get(target_label, "")
        if not archive_path:
            messagebox.showinfo("Character Mod Tool", "Choose a target config first.")
            return
        self.save_related_iff_as(archive_path)

    def save_related_iff_as(self, archive_path):
        if not self.related_iff_mods.get(archive_path):
            messagebox.showinfo("Character Mod Tool", "That face config has no pending texture replacements.")
            return
        base, ext = os.path.splitext(os.path.basename(archive_path))
        is_new = not os.path.isfile(archive_path)
        output_path = filedialog.asksaveasfilename(
            title="Save face config as",
            defaultextension=".iff",
            initialfile=f"{base}{ext or '.iff'}" if is_new else f"{base}_edited{ext or '.iff'}",
            initialdir=app_settings.ensure_output_dir(self.settings.get("output_dir", "")),
            filetypes=[("NBA 2K IFF", "*.iff"), ("All files", "*.*")],
        )
        if not output_path:
            return
        if os.path.abspath(output_path) == os.path.abspath(archive_path):
            if not messagebox.askyesno(
                "Overwrite original?",
                "This will overwrite the original related .iff. It is safer to save a new copy. Continue?",
            ):
                return
        try:
            self.write_related_iff(archive_path, output_path)
        except Exception as exc:
            messagebox.showerror("Character Mod Tool", f"Could not save face config:\n{exc}")
            return
        messagebox.showinfo("Character Mod Tool", f"Saved:\n{output_path}")
        self.face_status_var.set(f"Saved {os.path.basename(output_path)}.")

    def on_face_double_click(self, _event):
        if not self.face_tree.selection() or self.face_tree.selection()[0] not in self.face_texture_rows:
            return
        _archive_path, entry_name = self.face_texture_rows[self.face_tree.selection()[0]]
        if entry_name.lower().endswith(".txtr"):
            self.edit_selected_face_txtr()
        elif entry_name.lower().endswith(".dds"):
            self.open_selected_face_texture()

    def open_matching_face_iff(self):
        path = self.face_iff_path or self.find_matching_face_iff()
        if not path:
            messagebox.showinfo("Character Mod Tool", "No matching face .iff was found.")
            return
        self.load_iff(path)

    def open_face_folder(self):
        path = self.face_iff_path or self.find_matching_face_iff()
        folder = os.path.dirname(path or self.file_path)
        if not folder:
            return
        try:
            os.startfile(folder)
        except Exception as exc:
            messagebox.showerror("Character Mod Tool", f"Could not open folder:\n{exc}")

    def set_dynamic_body_text(self, text):
        self.dynamic_body_last_report = text
        if hasattr(self, "advanced_dynamic_body_text"):
            self.advanced_dynamic_body_text.configure(state=tk.NORMAL)
            self.advanced_dynamic_body_text.delete("1.0", tk.END)
            self.advanced_dynamic_body_text.insert("1.0", text)
            self.advanced_dynamic_body_text.configure(state=tk.DISABLED)

    def inspect_dynamic_body(self, show_popup=True):
        lines = ["Dynamic Body Package", ""]
        try:
            replacement_text, morphs = read_dynamic_body_package()
            lines.append(f"Bundled SCNE template: {os.path.basename(DYNAMIC_BODY_SCNE)}")
            lines.append(f"Bundled morph files: {len(morphs)}")
            lines.append(f"Template start marker present: {'yes' if DYNAMIC_BODY_START_MARKER in replacement_text else 'no'}")
            lines.append(f"Template end marker present: {'yes' if DYNAMIC_BODY_END_MARKER in replacement_text else 'no'}")
        except Exception as exc:
            self.dynamic_body_status_var.set("Dynamic-body package is missing or unreadable.")
            self.set_dynamic_body_text(f"Dynamic-body package problem:\n{exc}")
            if show_popup:
                messagebox.showerror("Character Mod Tool", str(exc))
            return

        if not self.file_path:
            lines.extend(["", "Open an .iff to inspect compatibility."])
            self.dynamic_body_status_var.set("Open an .iff to inspect dynamic-body compatibility.")
            self.set_dynamic_body_text("\n".join(lines))
            return

        scne_name = self.find_entry_name("hihead.SCNE")
        if not scne_name:
            lines.extend(["", "Current IFF: hihead.SCNE was not found."])
            self.dynamic_body_status_var.set("hihead.SCNE not found in current IFF.")
            self.set_dynamic_body_text("\n".join(lines))
            return

        scne_text = decode_text(self.get_entry_data(scne_name))
        has_start = DYNAMIC_BODY_START_MARKER in scne_text
        has_end = DYNAMIC_BODY_END_MARKER in scne_text
        existing = []
        missing = []
        lower_names = {name.lower(): name for name in self.active_entry_names()}
        for morph_name in sorted(morphs):
            if morph_name.lower() in lower_names:
                existing.append(morph_name)
            else:
                missing.append(morph_name)

        referenced = set()
        unused_count = 0
        active_morph_count = 0
        try:
            referenced = self.referenced_morph_names()
            referenced_lower = {name.lower() for name in referenced}
            active_morphs = [
                name for name in self.active_entry_names()
                if os.path.basename(name).lower().startswith("morph.")
                and os.path.basename(name).lower().endswith(".bin")
            ]
            active_morph_count = len(active_morphs)
            unused_count = sum(1 for name in active_morphs if os.path.basename(name).lower() not in referenced_lower)
        except Exception:
            pass

        lines.extend([
            "",
            f"Current IFF: {os.path.basename(self.file_path)}",
            f"SCNE target: {scne_name}",
            f"Start marker present: {'yes' if has_start else 'no'}",
            f"End marker present: {'yes' if has_end else 'no'}",
            f"Dynamic morphs already present: {len(existing)}",
            f"Dynamic morphs to add: {len(missing)}",
            f"Referenced morphs in SCNE: {len(referenced)}",
            f"Active morph files in IFF: {active_morph_count}",
            f"Unused active morph files: {unused_count}",
            f"Entries marked for removal: {len(self.removed)}",
        ])
        if missing:
            lines.extend(["", "Morphs to add:"])
            lines.extend(f"  {name}" for name in missing)
        if existing:
            lines.extend(["", "Morphs already present and replaced if applied:"])
            lines.extend(f"  {name}" for name in existing)

        if has_start and has_end:
            self.dynamic_body_status_var.set("Dynamic-body package is compatible with this IFF.")
        else:
            self.dynamic_body_status_var.set("This IFF is missing one or more dynamic-body SCNE markers.")
        self.set_dynamic_body_text("\n".join(lines))

    def apply_dynamic_body(self):
        if not self.file_path:
            messagebox.showinfo("Character Mod Tool", "Open an .iff file first.")
            return

        try:
            replacement_text, morphs = read_dynamic_body_package()
        except Exception as exc:
            messagebox.showerror("Character Mod Tool", str(exc))
            return

        scne_name = self.find_entry_name("hihead.SCNE")
        if not scne_name:
            messagebox.showerror("Character Mod Tool", "hihead.SCNE was not found in the opened .iff.")
            return

        try:
            original_scne = decode_text(self.get_entry_data(scne_name))
            patched_scne = build_dynamic_body_scne(original_scne, replacement_text)
        except Exception as exc:
            messagebox.showerror("Character Mod Tool", str(exc))
            return

        replaced = 0
        added = 0
        lower_names = {name.lower(): name for name in self.entry_order}
        self.modified[scne_name] = patched_scne.encode("utf-8")
        self.removed.discard(scne_name)

        for morph_name, data in sorted(morphs.items()):
            existing_name = lower_names.get(morph_name.lower())
            if existing_name:
                self.modified[existing_name] = data
                self.removed.discard(existing_name)
                replaced += 1
            else:
                self.entries[morph_name] = self.make_added_entry(morph_name, data)
                self.entry_order.append(morph_name)
                lower_names[morph_name.lower()] = morph_name
                self.modified[morph_name] = data
                self.removed.discard(morph_name)
                added += 1

        self.refresh_entries()
        if self.current_name == scne_name:
            data = self.get_entry_data(scne_name)
            self.show_entry_raw(scne_name, data)
            self.show_entry_json(scne_name, data)

        self.inspect_dynamic_body(show_popup=False)
        self.dynamic_body_status_var.set(
            f"Applied dynamic body: SCNE patched, {added} morphs added, {replaced} morphs replaced."
        )
        self.advanced_dynamic_body_save_button.configure(state=tk.NORMAL)
        self.status_var.set("Dynamic body changes applied. Use Save As to write a new .iff.")
        messagebox.showinfo(
            "Character Mod Tool",
            f"Dynamic body changes applied in memory.\n\nSCNE patched: {scne_name}\nMorphs added: {added}\nMorphs replaced: {replaced}\n\nUse Save As to create the new .iff.",
        )

    def referenced_morph_names(self):
        scne_name = self.find_entry_name("hihead.SCNE")
        if not scne_name or scne_name in self.removed:
            raise ValueError("hihead.SCNE was not found in the opened .iff.")
        scne_text = decode_text(self.get_entry_data(scne_name))
        return set(re.findall(r'"Binary"\s*:\s*"(Morph\.[^"]+\.bin)"', scne_text, flags=re.IGNORECASE))

    def remove_unused_morphs(self):
        if not self.file_path:
            messagebox.showinfo("Character Mod Tool", "Open an .iff file first.")
            return
        try:
            referenced_lower = {name.lower() for name in self.referenced_morph_names()}
        except Exception as exc:
            messagebox.showerror("Character Mod Tool", str(exc))
            return

        candidates = [
            name for name in self.active_entry_names()
            if os.path.basename(name).lower().startswith("morph.")
            and os.path.basename(name).lower().endswith(".bin")
            and os.path.basename(name).lower() not in referenced_lower
        ]

        if not candidates:
            self.dynamic_body_status_var.set("No unused morph files found.")
            self.inspect_dynamic_body(show_popup=False)
            messagebox.showinfo("Character Mod Tool", "No unused morph files were found.")
            return

        preview = "\n".join(os.path.basename(name) for name in candidates[:20])
        if len(candidates) > 20:
            preview += f"\n...and {len(candidates) - 20} more"
        confirmed = messagebox.askyesno(
            "Remove Unused Morphs?",
            f"This will mark {len(candidates)} unreferenced Morph.*.bin file(s) for removal when you Save As.\n\n{preview}",
        )
        if not confirmed:
            return

        for name in candidates:
            self.removed.add(name)
            self.modified.pop(name, None)

        if self.current_name in self.removed:
            self.current_name = ""
            self.raw_text.configure(state=tk.NORMAL)
            self.raw_text.delete("1.0", tk.END)
            self.raw_text.configure(state=tk.DISABLED)
            self.json_tree.delete(*self.json_tree.get_children())

        self.refresh_entries()
        self.inspect_dynamic_body(show_popup=False)
        self.dynamic_body_status_var.set(f"Marked {len(candidates)} unused morph file(s) for removal. Use Save As to write a new .iff.")
        self.status_var.set("Unused morph cleanup applied in memory. Use Save As to write a new .iff.")

    def on_entry_selected(self, _event=None):
        selection = self.entry_tree.selection()
        if not selection:
            return
        name = selection[0]
        if name in self.removed:
            self.current_name = name
            self.raw_text.configure(state=tk.NORMAL)
            self.raw_text.delete("1.0", tk.END)
            self.raw_text.insert("1.0", "This entry is marked for removal. Use Revert Entry to keep it.")
            self.raw_text.configure(state=tk.DISABLED)
            self.set_text(self.hex_text, "")
            self.json_tree.delete(*self.json_tree.get_children())
            self.status_var.set(f"{name} is marked for removal.")
            return
        self.current_name = name
        data = self.get_entry_data(name)
        self.show_entry_raw(name, data)
        self.show_entry_hex(data)
        self.show_entry_json(name, data)
        self.status_var.set(f"Selected {name}")

    @staticmethod
    def read_appearance_from_archive(path):
        with zipfile.ZipFile(path, "r") as archive:
            entry_name = next((
                name for name in archive.namelist()
                if os.path.basename(name).lower() in {item.lower() for item in APPEARANCE_ENTRY_NAMES}
            ), "")
            if not entry_name:
                raise ValueError(f"{os.path.basename(path)} does not contain appearance_info.")
            parsed, _wrapped, error = try_parse_structured_text(entry_name, archive.read(entry_name))
            if not isinstance(parsed, dict):
                raise ValueError(error or f"{entry_name} could not be parsed.")
            return entry_name, parsed

    @staticmethod
    def coerce_appearance_value(source_value, target_value):
        if isinstance(target_value, bool):
            if isinstance(source_value, str):
                return source_value.strip().lower() in {"1", "true", "yes", "on"}
            return bool(source_value)
        if isinstance(target_value, str):
            return str(source_value)
        if isinstance(target_value, int) and not isinstance(target_value, bool):
            return int(source_value)
        if isinstance(target_value, float):
            return float(source_value)
        if isinstance(target_value, (dict, list)) and isinstance(source_value, type(target_value)):
            return copy.deepcopy(source_value)
        return copy.deepcopy(source_value) if isinstance(source_value, type(target_value)) else target_value

    @classmethod
    def merge_same_named_appearance_fields(cls, source, target):
        merged = copy.deepcopy(target)
        source_appearance = source.get("appearance") if isinstance(source.get("appearance"), dict) else source
        source_body_fit = source.get("body_fit") if isinstance(source.get("body_fit"), dict) else source
        target_appearance = merged.get("appearance")
        target_body_fit = merged.get("body_fit")
        if not isinstance(target_appearance, dict) or not isinstance(target_body_fit, dict):
            raise ValueError("The target appearance_info does not contain appearance and body_fit sections.")

        appearance_matches = []
        body_fit_matches = []
        for key in list(target_appearance):
            if key in source_appearance:
                target_appearance[key] = cls.coerce_appearance_value(source_appearance[key], target_appearance[key])
                appearance_matches.append(key)
        for key in list(target_body_fit):
            if key in source_body_fit:
                target_body_fit[key] = cls.coerce_appearance_value(source_body_fit[key], target_body_fit[key])
                body_fit_matches.append(key)
        return merged, appearance_matches, body_fit_matches

    def browse_appearance_swap_source(self):
        current = self.appearance_swap_source_var.get().strip()
        path = filedialog.askopenfilename(
            title="Choose legacy appearance source IFF",
            filetypes=[("NBA 2K character IFF", "*.iff"), ("All files", "*.*")],
            initialdir=os.path.dirname(current or self.file_path),
        )
        if not path:
            return
        try:
            _entry_name, source = self.read_appearance_from_archive(path)
        except (OSError, zipfile.BadZipFile, ValueError) as exc:
            messagebox.showerror("Character Mod Tool", f"Could not read the legacy appearance source:\n{exc}")
            return
        self.appearance_swap_source_var.set(path)
        self.appearance_status_var.set(
            f"Legacy appearance source selected: {os.path.basename(path)} ({len(source)} top-level fields)."
        )

    def browse_appearance_swap_target(self):
        current = self.appearance_swap_target_var.get().strip()
        path = filedialog.askopenfilename(
            title="Choose target character IFF",
            filetypes=[("NBA 2K character IFF", "*.iff"), ("All files", "*.*")],
            initialdir=os.path.dirname(current or self.file_path),
        )
        if path:
            self.load_iff(path)

    def swap_appearance_and_body_fit(self):
        source_path = self.appearance_swap_source_var.get().strip()
        target_path = self.appearance_swap_target_var.get().strip()
        if not source_path:
            messagebox.showinfo("Character Mod Tool", "Choose a legacy source character first.")
            return
        if not target_path:
            messagebox.showinfo("Character Mod Tool", "Choose a target character first.")
            return
        if os.path.abspath(source_path) == os.path.abspath(target_path):
            messagebox.showinfo("Character Mod Tool", "Choose different source and target character IFFs.")
            return
        if os.path.abspath(self.file_path or "") != os.path.abspath(target_path):
            self.load_iff(target_path)
            if os.path.abspath(self.file_path or "") != os.path.abspath(target_path):
                return

        appearance_name = self.appearance_entry_name()
        if not appearance_name:
            messagebox.showerror("Character Mod Tool", "The target character does not contain appearance_info.")
            return
        try:
            _source_entry, source = self.read_appearance_from_archive(source_path)
            target, _wrapped, error = try_parse_structured_text(appearance_name, self.get_entry_data(appearance_name))
            if not isinstance(target, dict):
                raise ValueError(error or "The target appearance_info could not be parsed.")
            merged, appearance_matches, body_fit_matches = self.merge_same_named_appearance_fields(source, target)
        except (OSError, zipfile.BadZipFile, ValueError, TypeError) as exc:
            messagebox.showerror("Character Mod Tool", f"Could not prepare the appearance swap:\n{exc}")
            return

        if not appearance_matches and not body_fit_matches:
            messagebox.showinfo("Character Mod Tool", "No same-name appearance or body-fit fields were found.")
            return
        if not messagebox.askyesno(
            "Swap Appearance + Body Fit?",
            f"Source: {os.path.basename(source_path)}\nTarget: {os.path.basename(target_path)}\n\n"
            f"Appearance fields matched: {len(appearance_matches)}\n"
            f"Body-fit fields matched: {len(body_fit_matches)}\n\n"
            "Only exact same-name fields will be copied. Target configurations and accessory items will not change.",
        ):
            return

        self.appearance_json = merged
        if not self.mark_appearance_modified():
            return
        self.show_appearance()
        self.appearance_status_var.set(
            f"Swapped {len(appearance_matches)} appearance and {len(body_fit_matches)} body-fit fields. Use Save As."
        )
        self.status_var.set("Legacy appearance/body-fit swap applied in memory. Use Save As to write a new IFF.")
        messagebox.showinfo(
            "Appearance Swap Complete",
            "Swap complete. Target configurations were preserved. Click Save As to write the target IFF.",
        )

    def show_appearance(self):
        self.appearance_tree.delete(*self.appearance_tree.get_children())
        self.appearance_paths = {}
        self.appearance_json = None

        appearance_name = self.appearance_entry_name()
        if not appearance_name:
            self.appearance_status_var.set("appearance_info was not found in this .iff.")
            return

        parsed, _wrapped, error = try_parse_structured_text(appearance_name, self.get_entry_data(appearance_name))
        if not isinstance(parsed, dict):
            self.appearance_status_var.set(error or f"{appearance_name} could not be parsed.")
            return

        self.appearance_json = parsed
        for section, section_value in parsed.items():
            parent_iid = f"appearance_{len(self.appearance_paths)}"
            self.appearance_paths[parent_iid] = [section]
            if isinstance(section_value, dict):
                self.appearance_tree.insert(
                    "",
                    tk.END,
                    iid=parent_iid,
                    text=str(section),
                    values=(str(section), f"object ({len(section_value)} keys)"),
                    open=True,
                )
                for field, value in section_value.items():
                    iid = f"appearance_{len(self.appearance_paths)}"
                    self.appearance_paths[iid] = [section, field]
                    self.appearance_tree.insert(
                        parent_iid,
                        tk.END,
                        iid=iid,
                        text=str(field),
                        values=(str(section), json_value_preview(value)),
                    )
            else:
                self.appearance_tree.insert(
                    "",
                    tk.END,
                    iid=parent_iid,
                    text=str(section),
                    values=("", json_value_preview(section_value)),
                )

        count = sum(1 for path in self.appearance_paths.values() if len(path) == 2)
        self.appearance_status_var.set(f"Loaded {appearance_name} with {count} editable values.")

    def edit_selected_appearance_value(self):
        if self.appearance_json is None:
            messagebox.showinfo("Character Mod Tool", "Open an .iff with appearance_info first.")
            return
        selection = self.appearance_tree.selection()
        if not selection:
            return
        iid = selection[0]
        path = self.appearance_paths.get(iid, [])
        if len(path) < 2:
            messagebox.showinfo("Character Mod Tool", "Select a value row, not a section header.")
            return
        value = self.appearance_json
        for key in path:
            value = value[key]
        current = json.dumps(value) if not isinstance(value, str) else value
        answer = simpledialog.askstring(
            "Edit Appearance Value",
            f"{path[0]} / {path[1]}",
            initialvalue=current,
            parent=self,
        )
        if answer is None:
            return
        try:
            new_value = json.loads(answer)
        except json.JSONDecodeError:
            new_value = answer
        set_json_path(self.appearance_json, path, new_value)
        values = self.appearance_tree.item(iid, "values")
        section = values[0] if values else path[0]
        self.appearance_tree.item(iid, values=(section, json_value_preview(new_value)))
        self.appearance_tree.selection_set(iid)
        if self.mark_appearance_modified():
            self.appearance_status_var.set("Value changed and marked for Save As.")

    def on_appearance_double_click(self, event):
        row_id = self.appearance_tree.identify_row(event.y)
        if not row_id:
            return
        self.appearance_tree.selection_set(row_id)
        path = self.appearance_paths.get(row_id, [])
        if len(path) < 2:
            return
        self.edit_selected_appearance_value()

    def apply_appearance_change(self):
        if self.appearance_json is None:
            messagebox.showinfo("Character Mod Tool", "Open an .iff with appearance_info first.")
            return
        if not self.mark_appearance_modified():
            return
        self.appearance_status_var.set(f"Applied {self.appearance_entry_name()} changes. Use Save As to write a new .iff.")
        self.status_var.set("Applied appearance_info changes.")

    def apply_and_save_appearance_only(self):
        if not self.file_path or self.appearance_json is None:
            messagebox.showinfo("Character Mod Tool", "Open a target IFF with appearance_info first.")
            return
        appearance_name = self.appearance_entry_name()
        if not appearance_name:
            messagebox.showinfo("Character Mod Tool", "The target IFF does not contain appearance_info.")
            return

        base, ext = os.path.splitext(os.path.basename(self.file_path))
        output_path = filedialog.asksaveasfilename(
            title="Apply and save appearance_info",
            defaultextension=".iff",
            initialfile=f"{base}_appearance{ext or '.iff'}",
            initialdir=app_settings.ensure_output_dir(self.settings.get("output_dir", "")),
            filetypes=[("NBA 2K character IFF", "*.iff"), ("All files", "*.*")],
        )
        if not output_path:
            return
        if os.path.abspath(output_path) == os.path.abspath(self.file_path):
            if not messagebox.askyesno(
                "Overwrite target IFF?",
                "Only appearance_info will be replaced. All other target entries will be copied unchanged. Continue?",
            ):
                return

        appearance_data = serialize_structured_entry(
            appearance_name,
            self.entries[appearance_name].data,
            self.appearance_json,
            False,
        )
        try:
            self.write_appearance_only_iff(output_path, appearance_name, appearance_data)
        except Exception as exc:
            messagebox.showerror("Character Mod Tool", f"Could not save appearance_info:\n{exc}")
            return

        if os.path.abspath(output_path) == os.path.abspath(self.file_path):
            self.entries[appearance_name].data = appearance_data
            self.modified.pop(appearance_name, None)
            self.refresh_entries()
            self.show_appearance()
        self.appearance_status_var.set(
            f"Saved {os.path.basename(output_path)} with only {appearance_name} replaced."
        )
        self.status_var.set("Appearance-only save complete. Other IFF entries were copied unchanged.")
        messagebox.showinfo(
            "Appearance Saved",
            f"Saved:\n{output_path}\n\nOnly {appearance_name} was replaced.",
        )

    def write_appearance_only_iff(self, output_path, appearance_name, appearance_data):
        output_dir = os.path.dirname(output_path)
        if output_dir:
            os.makedirs(output_dir, exist_ok=True)
        temp_handle = tempfile.NamedTemporaryFile(
            prefix=os.path.basename(output_path) + ".",
            suffix=".tmp",
            dir=output_dir or None,
            delete=False,
        )
        temp_path = temp_handle.name
        temp_handle.close()
        found = False
        try:
            with zipfile.ZipFile(self.file_path, "r") as source, zipfile.ZipFile(temp_path, "w") as output:
                for info in source.infolist():
                    if info.is_dir():
                        output.writestr(self.copied_zip_info(info), b"")
                        continue
                    data = source.read(info.filename)
                    if info.filename == appearance_name:
                        data = appearance_data
                        found = True
                    output.writestr(self.copied_zip_info(info), data)
            if not found:
                raise ValueError(f"{appearance_name} was not found in the target IFF.")
            os.replace(temp_path, output_path)
        except Exception:
            try:
                os.remove(temp_path)
            except OSError:
                pass
            raise

    def mark_appearance_modified(self):
        appearance_name = self.appearance_entry_name()
        if not appearance_name:
            messagebox.showinfo("Character Mod Tool", "Open an .iff with appearance_info first.")
            return False
        self.modified[appearance_name] = serialize_structured_entry(
            appearance_name,
            self.entries[appearance_name].data,
            self.appearance_json,
            False,
        )
        self.refresh_entries()
        if self.current_name == appearance_name:
            data = self.get_entry_data(appearance_name)
            self.show_entry_raw(appearance_name, data)
            self.show_entry_json(appearance_name, data)
        return True

    def current_appearance_json(self):
        if self.appearance_json is None:
            self.show_appearance()
        if self.appearance_json is None:
            return None
        return self.appearance_json

    def extract_body_fit_values(self, appearance):
        if isinstance(appearance.get("body_fit"), dict):
            return copy.deepcopy(appearance["body_fit"]), "nested"
        body_fit = {
            key: copy.deepcopy(value)
            for key, value in appearance.items()
            if isinstance(key, str) and is_older_body_fit_key(key)
        }
        return body_fit, "flat"

    def replace_body_fit_values(self, appearance, body_fit):
        if isinstance(appearance.get("body_fit"), dict):
            previous_count = len(appearance["body_fit"])
            appearance["body_fit"] = copy.deepcopy(body_fit)
            return previous_count, "nested"

        previous_keys = [
            key for key in list(appearance.keys())
            if isinstance(key, str) and is_older_body_fit_key(key)
        ]
        for key in previous_keys:
            appearance.pop(key, None)

        rebuilt = {}
        inserted = False
        for key, value in appearance.items():
            rebuilt[key] = value
            if not inserted and key == "is gen5":
                for body_key, body_value in body_fit.items():
                    rebuilt[body_key] = copy.deepcopy(body_value)
                inserted = True
        if not inserted:
            for body_key, body_value in body_fit.items():
                rebuilt[body_key] = copy.deepcopy(body_value)
        appearance.clear()
        appearance.update(rebuilt)
        return len(previous_keys), "flat"

    def export_body_fit(self):
        appearance = self.current_appearance_json()
        if appearance is None:
            messagebox.showinfo("Character Mod Tool", "Open an .iff with appearance_info first.")
            return

        body_fit, body_fit_style = self.extract_body_fit_values(appearance)
        if not body_fit:
            messagebox.showinfo("Character Mod Tool", "This appearance_info does not contain body_fit values.")
            return

        number = self.current_iff_number() or "character"
        default_name = f"body_fit_{number}.json"
        output_path = filedialog.asksaveasfilename(
            title="Export body_fit values",
            defaultextension=".json",
            initialfile=default_name,
            initialdir=app_settings.ensure_output_dir(self.settings.get("output_dir", "")),
            filetypes=[("Body Fit JSON", "*.json"), ("All files", "*.*")],
        )
        if not output_path:
            return

        payload = {
            "tool": "Character Mod Tool",
            "type": "body_fit",
            "source_iff": os.path.basename(self.file_path),
            "style": body_fit_style,
            "body_fit": body_fit,
        }
        try:
            with open(output_path, "w", encoding="utf-8") as handle:
                json.dump(payload, handle, indent="\t", ensure_ascii=False)
                handle.write("\n")
        except Exception as exc:
            messagebox.showerror("Character Mod Tool", f"Could not export body fit:\n{exc}")
            return

        self.appearance_status_var.set(f"Exported {len(body_fit)} body_fit values.")
        messagebox.showinfo("Character Mod Tool", f"Exported body_fit values:\n{output_path}")

    def import_body_fit(self):
        appearance = self.current_appearance_json()
        if appearance is None:
            messagebox.showinfo("Character Mod Tool", "Open an .iff with appearance_info first.")
            return

        input_path = filedialog.askopenfilename(
            title="Import body_fit values",
            initialdir=os.path.dirname(self.file_path) if self.file_path else "",
            filetypes=[("Body Fit JSON", "*.json"), ("All files", "*.*")],
        )
        if not input_path:
            return

        try:
            with open(input_path, "r", encoding="utf-8-sig") as handle:
                payload = json.load(handle)
        except Exception as exc:
            messagebox.showerror("Character Mod Tool", f"Could not read body fit file:\n{exc}")
            return

        if isinstance(payload, dict) and isinstance(payload.get("body_fit"), dict):
            body_fit = payload["body_fit"]
        elif isinstance(payload, dict) and all(isinstance(key, str) for key in payload.keys()):
            body_fit = payload
        else:
            messagebox.showerror("Character Mod Tool", "The selected file does not contain body_fit values.")
            return

        previous_count, _style = self.replace_body_fit_values(appearance, body_fit)
        self.appearance_json = appearance
        if not self.mark_appearance_modified():
            return
        appearance_name = self.appearance_entry_name()

        self.refresh_entries()
        self.show_appearance()
        if self.current_name == appearance_name:
            data = self.get_entry_data(appearance_name)
            self.show_entry_raw(appearance_name, data)
            self.show_entry_json(appearance_name, data)

        self.appearance_status_var.set(
            f"Imported {len(body_fit)} body_fit values, replacing {previous_count}. Use Save As to write a new .iff."
        )
        self.status_var.set("Body fit import applied in memory. Use Save As to write a new .iff.")
        messagebox.showinfo(
            "Character Mod Tool",
            f"Imported {len(body_fit)} body_fit values into {appearance_name}.\n\nUse Save As to create the new .iff.",
        )

    def show_entry_raw(self, name, data):
        self.raw_text.configure(state=tk.NORMAL)
        self.raw_text.delete("1.0", tk.END)
        if is_probably_text(name, data):
            self.raw_text.insert("1.0", decode_entry_text(name, data))
            self.raw_status_var.set("Text entry is editable. Use Apply Text Change when finished.")
            self.raw_text.configure(state=tk.NORMAL)
        else:
            self.raw_text.insert("1.0", "Binary entry. Use Hex Preview for inspection.")
            self.raw_status_var.set("Binary entry is read-only in this version.")
            self.raw_text.configure(state=tk.DISABLED)

    def show_entry_hex(self, data):
        preview = data[:4096]
        rows = []
        for offset in range(0, len(preview), 16):
            chunk = preview[offset:offset + 16]
            hex_part = binascii.hexlify(chunk).decode("ascii")
            hex_pairs = " ".join(hex_part[i:i + 2] for i in range(0, len(hex_part), 2))
            ascii_part = "".join(chr(b) if 32 <= b <= 126 else "." for b in chunk)
            rows.append(f"{offset:08x}  {hex_pairs:<47}  {ascii_part}")
        if len(data) > len(preview):
            rows.append("")
            rows.append(f"Preview limited to {len(preview):,} of {len(data):,} bytes.")
        self.set_text(self.hex_text, "\n".join(rows))

    def show_entry_json(self, name, data):
        self.json_tree.delete(*self.json_tree.get_children())
        self.tree_paths = {}
        parsed, wrapped, error = try_parse_structured_text(name, data)
        self.current_json = parsed
        self.current_json_wrapped = wrapped
        if parsed is None:
            self.json_status_var.set(error or "This entry is not structured text.")
            return
        self.json_status_var.set("Structured entry loaded. Select a leaf value to edit.")
        self._insert_json_node("", "root", parsed, [])

    def _insert_json_node(self, parent, key, value, path):
        iid = f"node_{len(self.tree_paths)}"
        self.tree_paths[iid] = list(path)
        self.json_tree.insert(parent, tk.END, iid=iid, text=str(key), values=(json_value_preview(value),))
        if isinstance(value, dict):
            for child_key, child_value in value.items():
                self._insert_json_node(iid, child_key, child_value, path + [child_key])
        elif isinstance(value, list):
            for index, child_value in enumerate(value):
                self._insert_json_node(iid, index, child_value, path + [index])
        return iid

    def edit_selected_json_value(self):
        if self.current_json is None:
            messagebox.showinfo("Character Mod Tool", "Select a JSON or SCNE entry first.")
            return
        selection = self.json_tree.selection()
        if not selection:
            return
        iid = selection[0]
        path = self.tree_paths.get(iid, [])
        if not path:
            messagebox.showinfo("Character Mod Tool", "Select a field below the root.")
            return
        value = self.current_json
        for key in path:
            value = value[key]
        current = json.dumps(value) if not isinstance(value, str) else value
        answer = simpledialog.askstring(
            "Edit Value",
            "Enter a new value. JSON values like 5, true, null, or [1, 2] are accepted.",
            initialvalue=current,
            parent=self,
        )
        if answer is None:
            return
        try:
            new_value = json.loads(answer)
        except json.JSONDecodeError:
            new_value = answer
        set_json_path(self.current_json, path, new_value)
        name = self.current_name
        self.show_entry_json(
            name,
            serialize_structured_entry(
                name,
                self.entries[name].data,
                self.current_json,
                self.current_json_wrapped,
            ),
        )
        self.json_status_var.set("Value changed in memory. Use Apply GUI Changes to mark the archive entry edited.")

    def apply_json_change(self):
        if not self.current_name or self.current_json is None:
            messagebox.showinfo("Character Mod Tool", "Select a structured entry first.")
            return
        self.modified[self.current_name] = serialize_structured_entry(
            self.current_name,
            self.entries[self.current_name].data,
            self.current_json,
            self.current_json_wrapped,
        )
        self.refresh_entries()
        self.show_entry_raw(self.current_name, self.get_entry_data(self.current_name))
        if self.current_name == self.appearance_entry_name():
            self.show_appearance()
        self.status_var.set(f"Applied GUI changes to {self.current_name}")

    def apply_text_change(self):
        if not self.current_name:
            return
        data = self.get_entry_data(self.current_name)
        if not is_probably_text(self.current_name, data):
            messagebox.showinfo("Character Mod Tool", "Binary entries are read-only in this version.")
            return
        text = self.raw_text.get("1.0", "end-1c")
        if is_appearance_rdat(self.current_name):
            self.modified[self.current_name] = serialize_appearance_rdat(self.entries[self.current_name].data, text)
        else:
            self.modified[self.current_name] = text.encode("utf-8")
        self.refresh_entries()
        self.show_entry_json(self.current_name, self.modified[self.current_name])
        if self.current_name == self.appearance_entry_name():
            self.show_appearance()
        self.status_var.set(f"Applied text changes to {self.current_name}")

    def validate_current_json(self):
        if not self.current_name:
            return
        parsed, _wrapped, error = try_parse_structured_text(self.current_name, self.get_entry_data(self.current_name))
        if parsed is None:
            messagebox.showwarning("Character Mod Tool", error or "Selected entry is not valid structured JSON.")
        else:
            messagebox.showinfo("Character Mod Tool", "Structured data is valid.")

    def revert_current(self):
        if not self.current_name or (self.current_name not in self.modified and self.current_name not in self.removed):
            return
        self.modified.pop(self.current_name, None)
        self.removed.discard(self.current_name)
        self.refresh_entries()
        self.on_entry_selected()
        self.show_tattoos()
        if self.current_name == self.appearance_entry_name():
            self.show_appearance()
        self.status_var.set(f"Reverted {self.current_name}")

    def save_as(self):
        if not self.file_path:
            messagebox.showinfo("Character Mod Tool", "Open an .iff file first.")
            return
        default = os.path.splitext(os.path.basename(self.file_path))[0] + "_edited.iff"
        path = filedialog.asksaveasfilename(
            title="Save edited IFF as",
            defaultextension=".iff",
            initialfile=default,
            initialdir=app_settings.ensure_output_dir(self.settings.get("output_dir", "")),
            filetypes=[("NBA 2K character IFF", "*.iff"), ("All files", "*.*")],
        )
        if not path:
            return
        if os.path.abspath(path) == os.path.abspath(self.file_path):
            if not messagebox.askyesno(
                "Overwrite original?",
                "This will overwrite the original .iff. It is safer to save a new copy. Continue?",
            ):
                return
        try:
            self.write_iff(path)
        except Exception as exc:
            messagebox.showerror("Character Mod Tool", f"Could not save file:\n{exc}")
            return
        messagebox.showinfo("Character Mod Tool", f"Saved:\n{path}")
        self.status_var.set(f"Saved {os.path.basename(path)}")

    def write_iff(self, path):
        output_dir = os.path.dirname(path)
        if output_dir:
            os.makedirs(output_dir, exist_ok=True)

        tmp_handle = tempfile.NamedTemporaryFile(
            prefix=os.path.basename(path) + ".",
            suffix=".tmp",
            dir=output_dir or None,
            delete=False,
        )
        tmp_path = tmp_handle.name
        tmp_handle.close()

        try:
            with zipfile.ZipFile(tmp_path, "w") as archive:
                for name in self.entry_order:
                    if name in self.removed:
                        continue
                    entry = self.entries[name]
                    data = self.get_entry_data(name)
                    info = zipfile.ZipInfo(filename=name, date_time=entry.info.date_time)
                    info.comment = entry.info.comment
                    info.extra = entry.info.extra
                    info.internal_attr = entry.info.internal_attr
                    info.external_attr = entry.info.external_attr
                    info.create_system = entry.info.create_system
                    info.flag_bits = entry.info.flag_bits
                    info.compress_type = entry.info.compress_type or zipfile.ZIP_DEFLATED
                    archive.writestr(info, data)
            os.replace(tmp_path, path)
        except Exception:
            try:
                os.remove(tmp_path)
            except OSError:
                pass
            raise

    @staticmethod
    def set_text(widget, text):
        widget.configure(state=tk.NORMAL)
        widget.delete("1.0", tk.END)
        widget.insert("1.0", text)
        widget.configure(state=tk.DISABLED)

    def destroy(self):
        icon_handles = getattr(self, "_windows_icon_handles", ())
        self.clear_everything_swap_source_package()
        self.clear_manifest_target_cache()
        super().destroy()
        if sys.platform == "win32":
            for handle in icon_handles:
                try:
                    ctypes.windll.user32.DestroyIcon(handle)
                except (AttributeError, OSError):
                    pass


def run_package_self_test():
    try:
        ctypes.sizeof(ctypes.c_void_p)
        if getattr(sys, "frozen", False):
            test_window = tk.Tk()
            test_window.withdraw()
            test_window.update_idletasks()
            test_window.destroy()
        if not os.path.isfile(ROSTER_CLI):
            raise FileNotFoundError(f"Bundled Live Roster tool is missing: {ROSTER_CLI}")
        roster_digest = hashlib.sha256(Path(ROSTER_CLI).read_bytes()).hexdigest().upper()
        if roster_digest != ROSTER_CLI_SHA256:
            raise RuntimeError("Bundled Live Roster tool failed its integrity check.")
        if not os.path.isfile(HAIR_BACKEND_PATH):
            raise FileNotFoundError(f"Bundled Hair backend is missing: {HAIR_BACKEND_PATH}")
        if HAIR_TOOLS_DIR not in sys.path:
            sys.path.insert(0, HAIR_TOOLS_DIR)
        spec = importlib.util.spec_from_file_location("character_mod_hair_backend_self_test", HAIR_BACKEND_PATH)
        if spec is None or spec.loader is None:
            raise RuntimeError("Could not create the Hair backend loader.")
        module = importlib.util.module_from_spec(spec)
        sys.modules[spec.name] = module
        spec.loader.exec_module(module)
        for name in (
            "configure_environment",
            "discover_hairs",
            "discover_facial_hairs",
            "external_hair_conversion_plan",
            "convert_external_hair_to_target",
        ):
            if not callable(getattr(module, name, None)):
                raise RuntimeError(f"Hair backend is missing {name}().")
        return 0
    except Exception:
        report = Path(tempfile.gettempdir()) / "CharacterModTool-package-self-test.txt"
        report.write_text(traceback.format_exc(), encoding="utf-8")
        return 1


if __name__ == "__main__":
    if "--package-self-test" in sys.argv:
        raise SystemExit(run_package_self_test())
    app = CharacterModTool()
    app.mainloop()
