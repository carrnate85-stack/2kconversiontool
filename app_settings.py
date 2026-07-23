import glob
import json
import os
import re
import shutil
import sys
import tempfile
from pathlib import Path


APP_NAME = "Character Mod Tool"
_USER_DATA_DIR = None


def resource_path(*parts):
    root = Path(getattr(sys, "_MEIPASS", Path(__file__).resolve().parent))
    return str(root.joinpath(*parts))


def install_dir():
    if getattr(sys, "frozen", False):
        return Path(sys.executable).resolve().parent
    return Path(__file__).resolve().parent


def user_data_dir():
    global _USER_DATA_DIR
    if _USER_DATA_DIR is not None:
        return _USER_DATA_DIR
    root = os.environ.get("LOCALAPPDATA") or os.environ.get("APPDATA") or str(Path.home())
    path = Path(root) / "CharacterModTool"
    try:
        path.mkdir(parents=True, exist_ok=True)
    except OSError:
        path = Path(tempfile.gettempdir()) / "CharacterModTool"
        path.mkdir(parents=True, exist_ok=True)
    _USER_DATA_DIR = path
    return path


SETTINGS_PATH = user_data_dir() / "settings.json"
LOG_DIR = user_data_dir() / "logs"
DIAGNOSTICS_DIR = user_data_dir() / "diagnostics"


def ensure_output_dir(preferred=""):
    candidates = [Path(preferred).expanduser()] if preferred else []
    candidates.extend((install_dir() / "outputs", user_data_dir() / "outputs"))
    for path in candidates:
        try:
            path.mkdir(parents=True, exist_ok=True)
            with tempfile.NamedTemporaryFile(prefix=".write_test_", dir=path):
                pass
            return os.path.normpath(str(path))
        except OSError:
            continue
    return ""


def _valid_game_root(path):
    root = Path(str(path or "")).expanduser()
    return root.is_dir() and (root / "manifest").is_file() and (root / "mod.exe").is_file()


def _steam_roots():
    roots = []
    try:
        import winreg

        for hive, key_name in (
            (winreg.HKEY_CURRENT_USER, r"Software\Valve\Steam"),
            (winreg.HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Valve\Steam"),
        ):
            try:
                with winreg.OpenKey(hive, key_name) as key:
                    value, _kind = winreg.QueryValueEx(key, "SteamPath" if hive == winreg.HKEY_CURRENT_USER else "InstallPath")
                    roots.append(Path(value))
            except OSError:
                pass
    except ImportError:
        pass

    roots.extend(
        Path(path)
        for path in (
            r"C:\Program Files (x86)\Steam",
            r"C:\Program Files\Steam",
        )
    )
    libraries = []
    for root in roots:
        if root not in libraries:
            libraries.append(root)
        vdf = root / "steamapps" / "libraryfolders.vdf"
        try:
            text = vdf.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for match in re.finditer(r'"path"\s+"([^"]+)"', text, re.IGNORECASE):
            library = Path(match.group(1).replace(r"\\", "\\"))
            if library not in libraries:
                libraries.append(library)
    return libraries


def discover_game_root(preferred=""):
    candidates = [preferred, os.environ.get("NBA2K26_ROOT", "")]
    candidates.extend(
        str(root / "steamapps" / "common" / "NBA 2K26")
        for root in _steam_roots()
    )
    for candidate in candidates:
        if _valid_game_root(candidate):
            return os.path.normpath(str(Path(candidate)))
    return os.path.normpath(str(preferred)) if preferred else ""


def discover_blender(preferred=""):
    candidates = [preferred, os.environ.get("BLENDER_EXE", ""), shutil.which("blender") or ""]
    for root in filter(None, (os.environ.get("ProgramFiles"), os.environ.get("ProgramW6432"))):
        candidates.extend(sorted(glob.glob(os.path.join(root, "Blender Foundation", "Blender *", "blender.exe")), reverse=True))
    for candidate in candidates:
        if candidate and os.path.isfile(candidate) and os.path.basename(candidate).lower() == "blender.exe":
            return os.path.normpath(candidate)
    return os.path.normpath(preferred) if preferred else ""


def discover_photoshop(preferred=""):
    candidates = [preferred, os.environ.get("PHOTOSHOP_EXE", "")]
    for root in (r"C:\Program Files\Adobe", r"C:\Program Files (x86)\Adobe"):
        candidates.extend(sorted(glob.glob(os.path.join(root, "Adobe Photoshop*", "Photoshop.exe")), reverse=True))
    for candidate in candidates:
        if candidate and os.path.isfile(candidate):
            return os.path.normpath(candidate)
    return os.path.normpath(preferred) if preferred else ""


def _installed_blender_tools(folder_name):
    appdata = os.environ.get("APPDATA", "")
    if not appdata:
        return []
    patterns = [
        os.path.join(appdata, "Blender Foundation", "Blender", "*", "scripts", "addons", folder_name),
        os.path.join(appdata, "Blender Foundation", "Blender", "*", "extensions", "user_default", folder_name),
    ]
    paths = []
    for pattern in patterns:
        paths.extend(sorted(glob.glob(pattern), reverse=True))
    return paths


def discover_head_swap_tool(preferred=""):
    candidates = [preferred, resource_path("blender_tools", "NBA_Character_HeadSwap")]
    candidates.extend(_installed_blender_tools("NBA_Character_HeadSwap"))
    for candidate in candidates:
        if candidate and os.path.isfile(os.path.join(candidate, "__init__.py")):
            return os.path.normpath(candidate)
    return os.path.normpath(preferred) if preferred else ""


def discover_mesh_data_transfer(preferred=""):
    candidates = [preferred, resource_path("blender_tools", "mesh_data_transfer")]
    candidates.extend(_installed_blender_tools("mesh_data_transfer"))
    for candidate in candidates:
        if candidate and os.path.isfile(os.path.join(candidate, "__init__.py")):
            return os.path.normpath(candidate)
    return os.path.normpath(preferred) if preferred else ""


DEFAULTS = {
    "game_root": "",
    "blender_exe": "",
    "photoshop_exe": "",
    "head_swap_tool": "",
    "mesh_data_transfer": "",
    "output_dir": "",
}


def load_settings():
    values = dict(DEFAULTS)
    try:
        loaded = json.loads(SETTINGS_PATH.read_text(encoding="utf-8"))
        if isinstance(loaded, dict):
            values.update({key: str(loaded.get(key, "") or "") for key in DEFAULTS})
    except (OSError, ValueError, TypeError):
        pass
    values["game_root"] = discover_game_root(values["game_root"])
    values["blender_exe"] = discover_blender(values["blender_exe"])
    values["photoshop_exe"] = discover_photoshop(values["photoshop_exe"])
    values["head_swap_tool"] = discover_head_swap_tool(values["head_swap_tool"])
    values["mesh_data_transfer"] = discover_mesh_data_transfer(values["mesh_data_transfer"])
    values["output_dir"] = ensure_output_dir(values["output_dir"])
    return values


def save_settings(values):
    clean = {key: str(values.get(key, "") or "") for key in DEFAULTS}
    SETTINGS_PATH.parent.mkdir(parents=True, exist_ok=True)
    temp = SETTINGS_PATH.with_suffix(".tmp")
    temp.write_text(json.dumps(clean, indent=2) + "\n", encoding="utf-8")
    os.replace(temp, SETTINGS_PATH)
    return clean
