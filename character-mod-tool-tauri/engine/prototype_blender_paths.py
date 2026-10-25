"""Resolve a release-adjacent Blender without persisting its absolute location."""

from pathlib import Path


def bundled_blender(executable):
    if not executable:
        return None
    folder = Path(executable).resolve().parent / "Blender"
    candidate = folder / "blender.exe"
    if candidate.is_file() and (folder / "5.1/scripts").is_dir():
        return candidate
    return None


def select_blender(discovered, configured, executable):
    bundled = bundled_blender(executable)
    if bundled and (not configured or not Path(configured).is_file()):
        return str(bundled)
    return discovered


def portable_setting(selected, executable):
    bundled = bundled_blender(executable)
    if bundled and selected and Path(selected).resolve() == bundled:
        return ""
    return selected
