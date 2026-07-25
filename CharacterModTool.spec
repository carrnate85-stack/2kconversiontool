from pathlib import Path


root = Path(SPECPATH)


def data_tree(relative):
    base = root / relative
    items = []
    if not base.exists():
        return items
    for path in base.rglob("*"):
        if not path.is_file():
            continue
        lowered = {part.lower() for part in path.parts}
        if "__pycache__" in lowered or path.suffix.lower() in {".pyc", ".log", ".bak"}:
            continue
        if "hair_picker_backups" in lowered:
            continue
        items.append((str(path), str(path.parent.relative_to(root))))
    return items


datas = []
for folder in (
    "accessory_templates",
    "built_in_glasses",
    "built_in_headbands",
    "dynamic_body_package",
    "hair_tools",
    "tools",
    "blender_tools",
    "assets",
):
    datas.extend(data_tree(folder))

for filename in (
    "blender_full_swap_bridge.py",
    "blender_headband_swap_bridge.py",
    "blender_open_output.py",
    "ASSET_DISTRIBUTION_POLICY.txt",
    "THIRD_PARTY_NOTICES.txt",
):
    path = root / filename
    if path.is_file():
        datas.append((str(path), "."))


a = Analysis(
    [str(root / "character_mod_tool.py")],
    pathex=[str(root)],
    binaries=[],
    datas=datas,
    hiddenimports=["ctypes", "_ctypes"],
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[str(root / "pyi_rth_tkinter_fallback.py")],
    excludes=[],
    noarchive=False,
    optimize=0,
)
pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name="CharacterModTool",
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    console=False,
    icon=str(root / "assets" / "character_mod_tool_icon.ico"),
    disable_windowed_traceback=False,
)

coll = COLLECT(
    exe,
    a.binaries,
    a.datas,
    strip=False,
    upx=True,
    upx_exclude=[],
    name="CharacterModTool",
)
