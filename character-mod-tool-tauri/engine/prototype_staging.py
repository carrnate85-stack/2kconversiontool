"""Create durable, isolated NBA 2K27 editing copies without converting players."""

import copy
import hashlib
import json
import os
import re
import shutil
import stat
import subprocess
import tempfile
from contextlib import ExitStack
from pathlib import Path
import zipfile


MAX_FILE = 512 * 1024 * 1024
MAX_TOTAL = 2 * 1024 * 1024 * 1024
MAX_FILES = 500
MAX_DOCUMENT = 16 * 1024 * 1024
BINARY_DOCUMENT = bytes.fromhex("91f81fe0")


def _safe_path(value):
    path = Path(os.path.abspath(Path(value).expanduser()))
    for part in (path, *path.parents):
        try:
            metadata = part.lstat()
        except FileNotFoundError:
            continue
        if stat.S_ISLNK(metadata.st_mode) or getattr(metadata, "st_file_attributes", 0) & 0x400:
            raise ValueError("Player staging does not follow links or reparse points.")
    return path


def _identity(path):
    metadata = _safe_path(path).stat()
    if not stat.S_ISREG(metadata.st_mode) or not 0 < metadata.st_size <= MAX_TOTAL:
        raise ValueError("Choose a regular, nonempty player package within the 2 GB limit.")
    return metadata.st_dev, metadata.st_ino, metadata.st_size, metadata.st_mtime_ns


def _digest(path):
    value = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def _snapshot(path):
    identity = _identity(path)
    digest = _digest(path)
    if _identity(path) != identity:
        raise ValueError("The source player changed while staging; try again.")
    return identity, digest


def _verify_source(path, snapshot):
    if _identity(path) != snapshot[0] or _digest(path) != snapshot[1]:
        raise ValueError("The source player changed while staging; no editing copy was published.")


def _copy_file(source, destination, snapshots):
    from character_package import _copy_bounded
    snapshot = snapshots.setdefault(source, _snapshot(source))
    if snapshot[0][2] > MAX_FILE:
        raise ValueError("A player companion exceeds the 512 MB limit.")
    with source.open("rb") as incoming, destination.open("xb") as outgoing:
        _copy_bounded(incoming, outgoing, snapshot[0][2])
        outgoing.flush()
        os.fsync(outgoing.fileno())
    if _digest(destination) != snapshot[1]:
        raise ValueError("The editing copy did not match its source.")
    _verify_source(source, snapshot)


def _decode(arguments, data):
    executable = os.environ.get("CMT_RUST_EXE")
    if not executable:
        raise ValueError("Native player decoding is unavailable. Re-extract the application.")
    result = subprocess.run(
        [executable, *arguments], input=data, capture_output=True, timeout=90,
        creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
    )
    if result.returncode:
        raise ValueError("Player metadata/geometry could not be decoded: "
                         + result.stderr.decode("utf-8", errors="replace")[-1500:])
    if len(result.stdout) > MAX_FILE:
        raise ValueError("Decoded player data exceeds the supported limit.")
    return result.stdout


def _document(data):
    if len(data) > MAX_DOCUMENT:
        raise ValueError("Player metadata exceeds the 16 MB limit.")
    decoded = _decode(["--decode-scene"], data) if data.startswith(BINARY_DOCUMENT) else data
    if len(decoded) > MAX_DOCUMENT:
        raise ValueError("Decoded player metadata exceeds the 16 MB limit.")
    text = decoded.decode("utf-8-sig").strip()
    text = "\n".join(line for line in text.splitlines()
                     if not line.lstrip().startswith("//") and line.strip().rstrip(",").lower() != "null")
    parsed = json.loads(text if text.startswith("{") else "{" + text + "}")
    if not isinstance(parsed, dict):
        raise ValueError("Player metadata must contain an object.")
    return parsed, decoded


def _walk(value):
    pending = [(value, 0)]
    count = 0
    while pending:
        item, depth = pending.pop()
        count += 1
        if depth > 128 or count > 1000000:
            raise ValueError("Player metadata exceeds the traversal limit.")
        yield item
        if isinstance(item, dict):
            pending.extend((child, depth + 1) for child in item.values())
        elif isinstance(item, list):
            pending.extend((child, depth + 1) for child in item)


def _reference(value):
    from character_package import _check_name
    normalized = value.replace("\\", "/")
    _check_name(normalized)
    return normalized.rsplit("/", 1)[-1]


def _inspect_copy(path, bridge):
    """Keep text-ready files byte exact; expand only editor-incompatible encodings."""
    import nba2k25_support
    documents, replacements, resources = {}, {}, {}
    with zipfile.ZipFile(path) as archive:
        entries = archive.infolist()
        if (len(entries) > 10000 or sum(row.file_size for row in entries) > MAX_FILE
                or any(row.file_size > MAX_FILE or row.flag_bits & 1
                       or stat.S_IFMT(row.external_attr >> 16) not in (0, stat.S_IFREG, stat.S_IFDIR)
                       for row in entries)
                or len({row.filename.casefold() for row in entries}) != len(entries)):
            raise ValueError("Player IFF exceeds archive limits or contains duplicate/unsafe members.")
        if archive.testzip():
            raise ValueError("Player IFF checksum validation failed.")
        for row in entries:
            lower = row.filename.casefold()
            if not lower.endswith((".scne", ".txtr", ".json")):
                continue
            if row.file_size > MAX_DOCUMENT:
                raise ValueError("Player metadata exceeds the supported size.")
            data = archive.read(row)
            document, decoded = _document(data)
            documents[lower] = document
            if decoded != data:
                if lower.endswith(".json") and not decoded.lstrip().startswith(b"{"):
                    decoded = b"{\n" + decoded + b"\n}"
                replacements[row.filename] = decoded
            if lower.endswith(".scne"):
                _, descriptors = nba2k25_support.scene_resources(decoded)
                for name, descriptor in descriptors.items():
                    if name in resources and resources[name][1] != descriptor[1]:
                        raise ValueError("Conflicting player geometry buffer sizes.")
                    resources[name] = descriptor
        for row in entries:
            lower = row.filename.casefold()
            descriptor = resources.get(lower) or resources.get(lower[:-4] + ".gz" if lower.endswith(".bin") else lower)
            if not descriptor:
                continue
            data = archive.read(row)
            if data[12:16] != b"VCZ\0" and not data.startswith(b"\x1f\x8b\x08"):
                continue
            if not 0 < descriptor[1] <= 128 * 1024 * 1024:
                raise ValueError("Expanded player geometry exceeds the supported size.")
            game = bridge.game_root_for(bridge.fast_settings(), "NBA 2K27")
            decoded = _decode(["--decode-geometry", str(game), str(descriptor[1])], data)
            if len(decoded) != descriptor[1]:
                raise ValueError("Decoded player geometry has an incorrect size.")
            replacements[row.filename] = decoded
        if replacements:
            if sum(len(replacements.get(row.filename, b"")) if row.filename in replacements else row.file_size
                   for row in entries) > MAX_FILE:
                raise ValueError("Expanded player archive exceeds the 512 MB limit.")
            temporary = path.with_name(path.name + ".decode.tmp")
            with zipfile.ZipFile(temporary, "x") as prepared:
                prepared.comment = archive.comment
                for row in entries:
                    prepared.writestr(copy.copy(row), replacements[row.filename] if row.filename in replacements else archive.read(row))
            with zipfile.ZipFile(temporary) as verified:
                if verified.namelist() != archive.namelist() or verified.testzip():
                    raise ValueError("Decoded editing copy failed verification.")
                for row in entries:
                    expected = replacements[row.filename] if row.filename in replacements else archive.read(row)
                    if verified.read(row.filename) != expected:
                        raise ValueError("Decoding changed an unrelated player member.")
    if replacements:
        os.replace(temporary, path)
    return documents, len(replacements)


def _verify_character(documents, hint):
    scenes = [value for name, value in documents.items() if name.endswith(".scne")]
    models = [model for scene in scenes for root in scene.values() if isinstance(root, dict)
              and isinstance(root.get("Model"), dict) for model in root["Model"].values() if isinstance(model, dict)]
    full_models = [model for model in models if isinstance(model.get("VertexFormat"), dict)
                   and "POSITION0" in model["VertexFormat"] and isinstance(model.get("IndexBuffer"), dict)
                   and any(isinstance(prim, dict) and str(prim.get("Material", "")).casefold() == "face_shader"
                           for prim in model.get("Prim", []))]
    materials = {str(item.get("Material", "")).casefold()
                 for scene in scenes for item in _walk(scene) if isinstance(item, dict)}
    appearance = documents.get("appearance_info.json")
    if (not full_models or "face_shader" not in materials or not materials.intersection({"arms_shader", "arm_shader", "chest_shader", "torso_shader"})
            or not isinstance(appearance, dict) or not isinstance(appearance.get("body_fit"), dict)):
        raise ValueError("Choose a full NBA 2K27 player with appearance/body-fit data. Convert legacy players with Full Swap first.")
    current = {"l_eye_shader", "r_eye_shader"}.issubset(materials)
    hashed_textures = any(isinstance(item, str) and re.search(r"\.[0-9a-f]{16}\.tld$", item, re.I)
                          for name, value in documents.items() if name.endswith(".txtr") for item in _walk(value))
    if "eyes_shader" in materials and not current:
        raise ValueError("This player uses the older unified-eye layout. Convert it to NBA 2K27 with Full Swap first.")
    if not current and not hashed_textures:
        if hint != "NBA 2K27":
            raise ValueError("Could not verify this player's NBA 2K27 generation. Convert older players with Full Swap first.")
        return ["Player structure verified; generation has no definitive marker and uses the supplied NBA 2K27 selection."]
    return []


def _dependencies(documents):
    names = set()
    for value in documents.values():
        for item in _walk(value):
            if isinstance(item, str) and item.casefold().endswith(".iff"):
                names.add(_reference(item))
    appearance = documents.get("appearance_info.json", {})
    body = appearance.get("appearance", appearance)
    if isinstance(body, dict) and body:
        gender = str(body.get("gender", "male")).casefold()
        gender = "female" if gender.startswith("female") else "child" if gender.startswith("child") else "male"
        names.add(gender + "_skin.iff")
        try:
            skin = int(body.get("skin color", body.get("skin_color")))
        except (ValueError, TypeError):
            skin = -1
        if 0 <= skin <= 99:
            names.update(f"{'female_' if gender == 'female' else ''}{region}_color_{skin:02d}.iff"
                         for region in ("chest", "torso", "legs"))
    return names


def _package_index(stack, source):
    import character_package as package
    with source.open("rb") as stream:
        rar = stream.read(8).startswith(b"Rar!\x1a\x07")
    archive = stack.enter_context(package._load_rarfile().RarFile(source, errors="strict") if rar else zipfile.ZipFile(source))
    rows = archive.infolist()
    if len(rows) > package.PACKAGE_MAX_MEMBERS:
        raise ValueError("Character package contains too many members.")
    index = {}
    for row in rows:
        if not row.is_dir():
            index.setdefault(package._basename(row.filename).casefold(), []).append(row)
    return archive, index, rar


def _extract_dependency(package_data, name, directory):
    import character_package as package
    archive, index, rar = package_data
    rows = index.get(name.casefold(), [])
    if not rows:
        return None
    if len(rows) != 1:
        raise ValueError("The package contains duplicate referenced companion names.")
    row = rows[0]
    package._check_name(row.filename)
    ordinary = row.is_file() and not row.file_redir if rar else stat.S_IFMT(row.external_attr >> 16) in (0, stat.S_IFREG)
    encrypted = row.needs_password() if rar else bool(row.flag_bits & 1)
    solid = rar and archive.is_solid()
    if (not ordinary or encrypted or not 0 < row.file_size <= MAX_FILE
            or (not solid and row.file_size / max(1, row.compress_size) > package.PACKAGE_MAX_COMPRESSION_RATIO)):
        raise ValueError("Referenced package companions must be bounded, unencrypted regular files.")
    destination = directory / package._basename(row.filename)
    with destination.open("xb") as output:
        if rar:
            package._extract_rar_member(Path(archive.filename), row, output)
        else:
            with archive.open(row) as incoming:
                package._copy_bounded(incoming, output, row.file_size)
    return destination


def stage_player(payload, bridge, session):
    from character_package import inspect_character_package
    source_value = payload.get("filePath")
    if not isinstance(source_value, str) or not source_value.strip():
        raise ValueError("Choose a player IFF, ZIP or RAR file.")
    source = _safe_path(source_value)
    if source.suffix.casefold() not in (".iff", ".zip", ".rar"):
        raise ValueError("Choose a player IFF, ZIP or RAR file.")
    original = _snapshot(source)
    hint = re.sub(r"[^A-Z0-9]", "", str(payload.get("gameLabel") or "").upper())
    if hint and hint not in ("NBA2K27", "2K27"):
        raise ValueError("Editing copies support NBA 2K27 only. Convert older players with Full Swap first.")
    hint = "NBA 2K27" if hint else ""
    settings = bridge.fast_settings()
    output = payload.get("outputDir") or settings.get("output_dir")
    if not isinstance(output, str) or not output.strip():
        raise ValueError("Choose an output folder in Settings before staging a player.")
    session = _safe_path(session)
    base = _safe_path(output) / "Edited Players"
    if base.is_relative_to(session):
        raise ValueError("Editing copies require a durable output folder outside the temporary session.")
    _safe_path(base)
    base.mkdir(parents=True, exist_ok=True)
    snapshots = {source: original}
    owned = None
    owned_identity = None
    try:
        with tempfile.TemporaryDirectory(prefix="player-stage-", dir=session) as private, ExitStack() as stack:
            info = inspect_character_package(source, private)
            main = _safe_path(info["path"])
            siblings = {}
            for count, path in enumerate(main.parent.iterdir(), 1):
                if count > 10000:
                    raise ValueError("The source folder contains too many entries; use a dedicated player folder.")
                siblings.setdefault(path.name.casefold(), []).append(path)
            package_data = _package_index(stack, source) if info.get("packagePath") else None
            extras = Path(private) / "dependencies"
            extras.mkdir()
            stem = main.stem
            png = re.fullmatch(r"png(\d+)", stem, re.I)
            companion = re.compile(rf"^{re.escape(stem)}_(?:config|item|geo)_.+\.iff$", re.I)
            selected = {main.name.casefold(): main.name}
            for names in siblings.values():
                name = names[0].name
                if companion.fullmatch(name) or (png and re.fullmatch(rf"face{png.group(1)}(?:_.+)?\.iff", name, re.I)):
                    selected[name.casefold()] = name
            owned = Path(tempfile.mkdtemp(prefix=".player-stage-", dir=base))
            owned_identity = (owned.stat().st_dev, owned.stat().st_ino)
            warnings, completed, missing = [], set(), set()
            total, staged_total = 0, 0
            while selected:
                key, name = selected.popitem()
                if key in completed or key in missing:
                    continue
                if len(completed) + len(selected) >= MAX_FILES:
                    raise ValueError("The player has too many local companions.")
                matches = siblings.get(key, [])
                if len(matches) > 1:
                    raise ValueError("The source folder contains duplicate companion filenames.")
                candidate = matches[0] if matches else _extract_dependency(package_data, name, extras) if package_data else None
                if candidate is None:
                    missing.add(key)
                    continue
                candidate = _safe_path(candidate)
                total += _identity(candidate)[2]
                if total > MAX_TOTAL:
                    raise ValueError("The selected player and companions exceed the 2 GB limit.")
                destination = owned / candidate.name
                _copy_file(candidate, destination, snapshots)
                documents, changed = _inspect_copy(destination, bridge)
                staged_total += destination.stat().st_size
                if staged_total > MAX_TOTAL:
                    raise ValueError("Expanded editing copies exceed the 2 GB limit.")
                if key == main.name.casefold():
                    warnings.extend(_verify_character(documents, hint))
                if changed:
                    warnings.append(f"{candidate.name}: expanded {changed} binary metadata/geometry member(s) in the editing copy; source unchanged.")
                completed.add(key)
                for dependency in _dependencies(documents):
                    selected.setdefault(dependency.casefold(), dependency)
            if main.name.casefold() not in completed:
                raise ValueError("The main player was not staged.")
            for path, snapshot in snapshots.items():
                _verify_source(path, snapshot)
            if missing:
                sample = ", ".join(sorted(missing)[:12])
                warnings.append(f"Dependencies not present beside the source/package were not copied: {sample}"
                                + (f" (+{len(missing)-12} more)" if len(missing) > 12 else "")
                                + ". The installed game may provide these.")
            _safe_path(base)
            final = base / (re.sub(r"[^A-Za-z0-9_-]", "_", stem)[:60] + "-" + owned.name.removeprefix(".player-stage-"))
            if final.exists():
                raise FileExistsError("The unique editing destination already exists.")
            os.rename(owned, final)
            owned = None
            return {"mainPath": str(final / main.name), "sourcePath": str(source),
                    "gameLabel": "NBA 2K27", "companions": len(completed) - 1,
                    "warnings": warnings, "directory": str(final)}
    finally:
        if owned is not None:
            _safe_path(owned)
            metadata = owned.stat()
            if (metadata.st_dev, metadata.st_ino) == owned_identity:
                shutil.rmtree(owned)
