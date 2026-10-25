"""Desktop services formerly owned by Node; conversion/addon code is unchanged."""
import base64
import csv
import hashlib
import json
import os
import re
import shutil
import subprocess
import tempfile
import time
import zipfile
from pathlib import Path


def token(value):
    return re.sub(r"[^a-z0-9]+", "_", str(value or "").strip().lower()).strip("_")


def geometry_keys(accessories, items):
    definitions = accessories.get("items")
    definitions = definitions if isinstance(definitions, list) else []
    keys = set()
    for item in items:
        definition = next((row for row in definitions if isinstance(row, dict) and token(row.get("name")) == item), {})
        mode = str(definition.get("tangentspace") or "").strip().lower()
        if mode != "yes":
            keys.add(item)
        if mode != "no":
            keys.add(item + "_tangentspace")
    return keys


def atomic_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f"{path.name}.{os.getpid()}.tmp")
    temporary.write_text(json.dumps(value, indent=2), encoding="utf-8")
    try:
        for attempt in range(8):
            try:
                os.replace(temporary, path)
                return
            except PermissionError:
                if attempt == 7:
                    raise
                time.sleep(0.05 * (attempt + 1))
    finally:
        temporary.unlink(missing_ok=True)


def valid_player(path):
    with zipfile.ZipFile(path) as archive:
        if archive.testzip() or not any(name.lower().endswith(".scne") for name in archive.namelist()):
            raise ValueError("Blender did not create a valid character IFF.")


def file_digest(path):
    digest = hashlib.sha256()
    with Path(path).open('rb') as source:
        for block in iter(lambda: source.read(65536), b''):
            digest.update(block)
    return digest.hexdigest()


def prepare_highest_detail_references(player_path, companions, backend, stage_dir):
    """Align distance selectors with verified geometry inside the unpublished swap."""
    from hair_tools.hair_lod_policy import hair_models

    stage = Path(stage_dir).resolve(strict=True)
    player = Path(player_path).resolve(strict=True)
    player.relative_to(stage)

    def members(path):
        with zipfile.ZipFile(path) as archive:
            entries = archive.infolist()
            if (len(entries) > 5000 or sum(row.file_size for row in entries) > 512 * 1024 * 1024
                    or any(row.file_size > 128 * 1024 * 1024 for row in entries)
                    or len({row.filename.casefold() for row in entries}) != len(entries)):
                raise ValueError("Accessory distance validation exceeds archive limits or has duplicate members.")
            return entries, {row.filename: archive.read(row) for row in entries}, archive.comment

    entries, payloads, comment = members(player)
    names = [name for name in payloads if Path(name).name.casefold() == "appearance_info.json"]
    if len(names) != 1:
        return []
    appearance = backend.read_hair_document(payloads[names[0]])
    items = appearance.get("accessory_items", {}).get("items", [])
    candidates = [item for item in items if isinstance(item, dict)
                  and item.get("type") in ("hair", "facialhair")
                  and isinstance(item.get("meshlo"), str) and item["meshlo"]
                  and isinstance(item.get("mesh"), str) and item["mesh"]
                  and item["meshlo"] != item["mesh"]]
    if not candidates:
        return []
    wanted = {backend.item_asset_key(str(item.get("name", ""))).casefold() for item in candidates}
    geometry = {}
    for source, destination in companions:
        match = re.fullmatch(r"png\d+_geo_((?:hair|facialhair)(?:_.+)?)\.iff", Path(destination).name, re.I)
        if not match:
            continue
        key = match[1].casefold()
        tangent = key.endswith("_tangentspace")
        if tangent:
            key = key[:-len("_tangentspace")]
        if key not in wanted:
            continue
        path = Path(source).resolve(strict=True)
        path.relative_to(stage)
        _, data, _ = members(path)
        meshes = set()
        for name, value in data.items():
            if not name.lower().endswith(".scne"):
                continue
            scene = backend.read_hair_document(value)
            if backend.validate_hair_scene_buffers(scene, data):
                meshes.update(prim["Mesh"] for model in hair_models(scene)
                              for prim in model["Prim"] if isinstance(prim.get("Mesh"), str))
        # A duplicate destination must not make an ambiguous package look valid.
        variant = geometry.setdefault(key, {})
        if tangent in variant:
            raise ValueError("Duplicate accessory geometry in the staged swap.")
        variant[tangent] = meshes
    changes = []
    for item in candidates:
        key = backend.item_asset_key(str(item.get("name", ""))).casefold()
        mode = str(item.get("tangentspace") or "").strip().lower()
        required = [True] if mode == "yes" else [False] if mode == "no" else [False, True]
        variants = geometry.get(key, {})
        if not all(item["mesh"] in variants.get(tangent, set()) for tangent in required):
            continue
        changes.append({"item": item["name"], "previous": item["meshlo"], "mesh": item["mesh"]})
        item["meshlo"] = item["mesh"]
    if not changes:
        return []
    replacement = (json.dumps(appearance, indent=2, ensure_ascii=False) + "\n").encode("utf-8")
    fd, temporary = tempfile.mkstemp(prefix=".distance-", suffix=".iff", dir=player.parent)
    os.close(fd)
    try:
        with zipfile.ZipFile(temporary, "w") as archive:
            archive.comment = comment
            for row in entries:
                archive.writestr(row, replacement if row.filename == names[0] else payloads[row.filename])
        with zipfile.ZipFile(temporary) as archive:
            if archive.testzip():
                raise ValueError("Accessory distance update did not create a valid IFF.")
        os.replace(temporary, player)
    finally:
        Path(temporary).unlink(missing_ok=True)
    return changes


def validate_swap_distances(app, original, player_path, config_archives, companion_stages=()):
    changes = []
    if app.everything_swap_target_game_var.get().strip() == "NBA 2K27":
        if Path(player_path).resolve() == Path(app.everything_swap_final_output).resolve():
            raise ValueError("Accessory distance correction requires an unpublished swap.")
        changes = prepare_highest_detail_references(
            player_path, companion_stages, app.load_hair_backend(), app.everything_swap_stage_dir)
    report = original(app, player_path, config_archives, companion_stages)
    report["highestDetailReferences"] = changes
    return report


def build_rust_dds(tld, payload):
    executable = Path(os.environ["CMT_RUST_EXE"])
    result = subprocess.run([str(executable), "--build-dds"], input=tld[:32] + payload,
                            capture_output=True, timeout=30, creationflags=subprocess.CREATE_NO_WINDOW)
    if result.returncode or not result.stdout.startswith(b"DDS "):
        raise ValueError("Native DDS reconstruction failed: " + result.stderr.decode("utf-8", errors="replace"))
    return result.stdout


def fit_input_copy(source, output, game_root):
    """Expand fitting-only geometry; the original staged/archive files are not rewritten."""
    import nba2k25_support
    executable = os.environ["CMT_RUST_EXE"]
    def decode(arguments, packed):
        result = subprocess.run([executable, *arguments], input=packed, capture_output=True,
                                timeout=60, creationflags=subprocess.CREATE_NO_WINDOW)
        if result.returncode:
            raise ValueError("Fitting input could not be decoded: " + result.stderr.decode("utf-8", errors="replace"))
        return result.stdout
    with zipfile.ZipFile(source) as archive:
        entries = archive.infolist()
        if len(entries) > 10000 or sum(row.file_size for row in entries) > 512 * 1024 * 1024:
            raise ValueError("Fitting archive exceeds the supported size.")
        if len({row.filename.casefold() for row in entries}) != len(entries):
            raise ValueError("Fitting archive contains duplicate member names.")
        scenes, descriptors = {}, {}
        for row in entries:
            if row.filename.lower().endswith(".scne"):
                scene = archive.read(row)
                if scene.startswith(bytes.fromhex("91f81fe0")):
                    scene = decode(["--decode-scene"], scene)
                    scenes[row.filename] = scene
                _, resources = nba2k25_support.scene_resources(scene)
                for name, descriptor in resources.items():
                    if name in descriptors and descriptors[name][1] != descriptor[1]:
                        raise ValueError("Conflicting fitting buffer sizes.")
                    descriptors[name] = descriptor
        changed = 0
        created = False
        try:
            with zipfile.ZipFile(output, "x", compression=zipfile.ZIP_DEFLATED) as prepared:
                created = True
                total = 0
                for row in entries:
                    data = scenes.get(row.filename)
                    if data is None:
                        data = archive.read(row)
                    name = row.filename.lower()
                    descriptor = descriptors.get(name) or descriptors.get(name[:-4] + ".gz" if name.endswith(".bin") else name)
                    if descriptor and (data[12:16] == b"VCZ\0" or data.startswith(b"\x1f\x8b\x08")):
                        if not 0 < descriptor[1] <= 128 * 1024 * 1024:
                            raise ValueError("Fitting buffer exceeds the supported size.")
                        root = game_root() if callable(game_root) else game_root
                        data = decode(["--decode-geometry", str(root), str(descriptor[1])], data)
                        if len(data) != descriptor[1]:
                            raise ValueError("Fitting decoder returned an incorrect buffer size.")
                        changed += 1
                    total += len(data)
                    if total > 512 * 1024 * 1024:
                        raise ValueError("Expanded fitting archive exceeds the supported size.")
                    prepared.writestr(row, data)
        except Exception:
            if created:
                Path(output).unlink(missing_ok=True)
            raise
    return changed


def commands(bridge, engine, data):
    import prototype_accessories
    import prototype_staging
    session = Path(os.environ["CMT_SESSION"])
    cache = session / "preview-resources"
    cache.mkdir(parents=True, exist_ok=True)
    decoder = engine / "electron/helpers/Nba2kVczDecoder.exe"
    texture_decoder = decoder

    def preview_resources(payload):
        target = Path(payload["target"]).resolve()
        if not target.is_file() or target.suffix.lower() != ".iff":
            raise ValueError("Choose a readable staged player IFF before loading Preview.")
        with zipfile.ZipFile(target) as archive:
            try:
                appearance = json.loads(archive.read("appearance_info.json").decode("utf-8-sig"))
            except KeyError:
                appearance = {}
        body = appearance.get("appearance", appearance)
        accessories = appearance.get("accessory_items", {})
        config = str(payload.get("config") or accessories.get("default_config") or accessories.get("default_out_of_uniform_config") or "")
        selected = next((row for row in accessories.get("configurations", []) if str(row.get("name", "")).lower() == config.lower()), None)
        if payload.get("config") and selected is None:
            raise ValueError("The selected configuration no longer exists.")
        items = [token(value) for value in (selected or {}).get("items", []) if token(value)]
        geometry = geometry_keys(accessories, items)
        gender = token(body.get("gender", "male"))
        gender = "female" if gender.startswith("female") else "child" if gender.startswith("child") else "male"
        try:
            skin = int(body.get("skin color", body.get("skin_color")))
        except (TypeError, ValueError):
            skin = None
        body_files = {f"{gender}_skin.iff": "body-detail"}
        if skin is not None and 0 <= skin <= 99:
            for region in ("chest", "torso", "legs"):
                body_files[f"{'female_' if gender == 'female' else ''}{region}_color_{skin:02d}.iff"] = "body-color"
        companions = []
        prefix = target.stem.lower() + "_"
        for path in target.parent.iterdir():
            if not path.is_file() or path.suffix.lower() != ".iff":
                continue
            name = path.name.lower()
            kind = body_files.get(name, "")
            if not kind and name.startswith(prefix):
                suffix = token(name[len(prefix):-4])
                if suffix == "config_" + token(config):
                    kind = "config"
                elif suffix.startswith("item_") and suffix[5:] in items:
                    kind = "item"
                elif suffix.startswith("geo_") and suffix[4:] in geometry:
                    kind = "geometry"
            if kind:
                companions.append({"path": str(path), "name": path.name, "kind": kind})
        local_names = {Path(row["path"]).name.lower() for row in companions}
        requested = {"char/" + name: kind for name, kind in body_files.items() if name not in local_names}
        config_name = f"{target.stem}_config_{token(config)}.iff"
        if config and config_name.lower() not in local_names:
            requested[f"char/sig/{config_name}"] = "config"
        for item in items:
            for suffix, kind in ((f"item_{item}", "item"), (f"geo_{item}", "geometry"), (f"geo_{item}_tangentspace", "geometry")):
                if kind == "geometry" and suffix[4:] not in geometry:
                    continue
                name = f"{target.stem}_{suffix}.iff"
                if name.lower() not in local_names:
                    requested[f"char/sig/{name}"] = kind
        for name in ("jersey_nike_v.iff", "shorts_nike.iff"):
            requested["char/" + ("female_" if gender == "female" else "") + name] = "preview-jersey"
        eye_white = {
            "0": "shared/e6/eyewhite_color_00.e69c01fde495d855",
            "1": "shared/8b/eyewhite_color_01.8bcd8c19e6fb8f09",
            "2": "shared/1a/eyewhite_color_02.1aca11c38da970ca",
            "3": "shared/a2/eyewhite_color_03.a2f8647ad2256dee",
            "glow": "shared/16/eyewhite_color_glow.16423ad4da3a86fe",
        }
        lash = "shared/05/lash_light_color.05d7fb9877d4c4b7" if str(body.get("lash color", body.get("lash_color", "dark"))).lower() == "light" else "shared/17/lash_dark_color.1758d32561b833f3"
        shared = [
            (eye_white.get(str(body.get("eye white", body.get("eye_white", "0"))).lower(), eye_white["0"]), "BC1_UNORM", "eye", "color", False),
            ("shared/ad/iris_normalheight1.ad144d47f7f5520f", "BC7_UNORM", "eye", "normal", True),
            (lash, "BC7_UNORM", "eyelash", "color", False),
        ]
        names = set(name.lower() for name in requested)
        for name, _, _, _, inline in shared:
            names.add(name + ".tld")
            if not inline:
                names.add(name + ".mip0")
        records = {}
        warnings = []
        cache_stamp = "offline"
        try:
            game = Path(bridge.game_root_for(bridge.fast_settings(), "NBA 2K27"))
            manifest = game / "manifest"
            stat = manifest.stat()
            cache_stamp = hashlib.sha256(str((game, stat.st_size, stat.st_mtime_ns)).encode()).hexdigest()[:16]
            with manifest.open(encoding="utf-8-sig", newline="") as file:
                for row in csv.reader(file):
                    if len(row) == 4 and row[0].replace("\\", "/").lower() in names:
                        records[row[0].replace("\\", "/").lower()] = (row[0], row[1], int(row[2]), int(row[3]))
        except (OSError, ValueError) as error:
            records.clear()
            warnings.append(f"Staged appearance texture lookup: {error}")
        from character_manifest import extract_native_manifest_iff
        for name, kind in requested.items():
            record = records.get(name.lower())
            if record is None:
                if kind.startswith("body"):
                    warnings.append(f"Optional preview dependency not in manifest: {name}")
                continue
            destination = cache / (cache_stamp + "-" + hashlib.sha256(str(record).encode()).hexdigest()[:16] + "-" + Path(name).name)
            try:
                if not destination.is_file():
                    if not re.fullmatch(r"[0-9A-Z]{2}", record[1]) or record[2] < 0 or not 0 < record[3] <= 512 * 1024 * 1024:
                        raise ValueError("Invalid manifest resource range.")
                    if record[2] + record[3] > (game / record[1]).stat().st_size:
                        raise ValueError("Manifest resource extends beyond its archive.")
                    # Ordinary ZIP entries do not contain a VCZ stream to decode.
                    with (game / record[1]).open("rb") as archive:
                        archive.seek(record[2])
                        packed = archive.read(record[3])
                    import io
                    if packed[12:16] != b"VCZ\0" and zipfile.is_zipfile(io.BytesIO(packed)):
                        destination.write_bytes(packed)
                    else:
                        extract_native_manifest_iff(game, record, destination, decoder)
                companions.append({"path": str(destination), "name": name, "kind": kind})
            except Exception as error:
                warnings.append(f"Preview dependency {name}: {error}")
        textures = []
        for name, format_name, region, channel, inline in shared:
            tld = records.get(name + ".tld")
            mip = tld if inline else records.get(name + ".mip0")
            if not tld or not mip:
                continue
            output = cache / (cache_stamp + "-" + Path(name).name + ".dds")
            try:
                if not output.is_file():
                    temporary = output.with_suffix(".part")
                    args = [str(texture_decoder), "--texture", str(game / tld[1]), str(tld[2]), str(tld[3]), str(game / mip[1]), str(mip[2]), str(mip[3]), str(game / "data/oodle/oo2core_9_win64.dll"), str(temporary)]
                    subprocess.run(args, check=True, capture_output=True, timeout=120, creationflags=subprocess.CREATE_NO_WINDOW)
                    if temporary.read_bytes()[:4] != b"DDS ":
                        raise ValueError("Shared texture decoder returned no DDS.")
                    os.replace(temporary, output)
                textures.append({"path": str(output), "name": Path(name).name + ".dds", "source": "shared (decoded)", "format": format_name, "binding": {"target": region, "channel": channel}})
            except Exception as error:
                warnings.append(f"Shared texture {name}: {error}")
        order = {"config": 0, "item": 1, "geometry": 2, "body-detail": 3, "body-color": 4, "preview-jersey": 5}
        companions.sort(key=lambda row: (order[row["kind"]], row["name"]))
        fit = {}
        for container in (body, appearance):
            fit.update({key: value for key, value in container.get("body_fit", {}).items() if isinstance(value, (int, float))})
            for key, value in container.items():
                if re.search(r"(?:THIN|THICK)$", key, re.I) and isinstance(value, (int, float)):
                    fit.setdefault(key, value)
        return {"filePath": str(target), "options": {"companionPaths": companions, "externalTextures": textures, "projectTangent": "staged"}, "name": target.name, "activeItems": items, "warnings": warnings, "player": {
            "mode": "staged-player", "defaultConfig": config, "gender": gender, "skinColor": skin,
            "irisColor": str(body.get("iris color", "BROWN")), "irisSize": str(body.get("iris size", "REGULAR")),
            "bodyFit": fit, "companionsLoaded": len(companions), "companionsRequested": len(companions),
            "sharedTexturesLoaded": len(textures), "stagedBodyTextures": sum(row["kind"].startswith("body-") for row in companions),
            "jerseyAvailable": any(row["kind"] == "preview-jersey" for row in companions)}}

    def normalize_templates(value):
        if not isinstance(value, dict):
            return {}
        values = {}
        for slot, row in value.items():
            if not isinstance(row, dict) or not re.fullmatch(r"[0-9]+", str(row.get("png", "")).strip()):
                continue
            values[slot] = {key: str(row.get(key) or "").strip() for key in ("png", "name", "hair", "facialhair", "headband")}
            values[slot]["name"] = values[slot]["name"] or "png" + values[slot]["png"]
        return values

    def templates_load(_payload):
        path = data / "target-templates.json"
        if not path.exists():
            old = Path(os.environ["LOCALAPPDATA"]) / "CharacterModTool/target-templates.json"
            if old.is_file():
                shutil.copyfile(old, path)
        warning = ""
        try:
            value = json.loads(path.read_text(encoding="utf-8-sig")) if path.exists() else {}
            values = normalize_templates(value.get("templates", value) if isinstance(value, dict) else value)
        except (OSError, ValueError, TypeError) as error:
            values, warning = {}, str(error)
        return {"templates": values, "exists": path.exists(), "path": str(path), "warning": warning}

    def templates_save(payload):
        values = normalize_templates(payload)
        atomic_json(data / "target-templates.json", {"version": 1, "templates": values})
        return templates_load({})

    def cli_run(args):
        executable = engine / "tools/live_roster/roster.exe"
        expected = "6F09FCEB91CAF946FF82FEEEE677256D50958BA9879B01478CEF4E6E3D2A3FCC"
        if hashlib.sha256(executable.read_bytes()).hexdigest().upper() != expected:
            raise ValueError("Roster CLI failed its integrity check.")
        # Keep the CLI's working files out of the immutable runtime.
        working = data / "roster"
        working.mkdir(exist_ok=True)
        owned = working / "roster.exe"
        if not owned.is_file() or hashlib.sha256(owned.read_bytes()).hexdigest().upper() != expected:
            shutil.copyfile(executable, owned)
        executable = owned
        result = subprocess.run([str(executable), *args], cwd=working, capture_output=True, timeout=20, creationflags=subprocess.CREATE_NO_WINDOW)
        output = (result.stdout + result.stderr).decode("utf-8", errors="replace").strip()
        if result.returncode:
            raise ValueError(output or "Roster CLI failed.")
        return output

    def roster_game(payload):
        game = str(payload.get("game") or "NBA2K27").strip().upper()
        if game not in ("NBA2K25", "NBA2K26", "NBA2K27"):
            raise ValueError("Unsupported roster game.")
        return game

    def cli_read(payload):
        game = roster_game(payload)
        output = cli_run([game, "sp", "get"])
        fields = {}
        for line in output.splitlines():
            match = re.match(r"^\|\s*\d+\s*\|\s*([^|]+?)\s*\|\s*([^|]*?)\s*\|", line)
            if match:
                fields[match[1].strip()] = "" if match[2].strip() == "(empty)" else match[2].strip()
        if not fields.get("CyberfaceID", "").isdigit():
            raise ValueError("Roster CLI returned an unreadable player: " + output)
        fields = {key: fields.get(key, "") for key in ("FirstName", "LastName", "CyberfaceID", "HeadshotID", "PortraitID", "PlayerKey", "Nickname", "JerseyNickname")}
        fields["CyberfaceID"] = str(int(fields["CyberfaceID"]))
        identity = fields["PlayerKey"] or "|".join(fields[key] for key in ("FirstName", "LastName", "HeadshotID", "PortraitID"))
        return {"game": game, "fields": fields, "display": {key: (value.zfill(5) if key == "CyberfaceID" else value or "(empty)") for key, value in fields.items()}, "identity": identity}

    def cli_set(payload):
        field = str(payload.get("field") or "").strip()
        if field not in ("FirstName", "LastName", "CyberfaceID", "HeadshotID", "PortraitID"):
            raise ValueError("This roster field is read-only.")
        value = str(payload.get("value", "")).strip()
        if not value or len(value) > 64 or any(char in value for char in "\r\n\0"):
            raise ValueError("Unsupported roster value.")
        numeric = field.endswith("ID")
        if numeric:
            if not re.fullmatch(r"[0-9]+", value) or not 0 <= int(value) <= 65535:
                raise ValueError("ID must be between 0 and 65535.")
            value = str(int(value))
        before = cli_read(payload)
        same = lambda first, second: int(first or 0) == int(second or 0) if numeric else str(first) == str(second)
        if payload.get("expectedIdentity") and payload["expectedIdentity"] != before["identity"]:
            raise ValueError("Highlighted player changed; read the player again.")
        if "expectedValue" in payload and not same(before["fields"][field], payload["expectedValue"]):
            raise ValueError("The field changed; read the player again.")
        cli_run([before["game"], "sp", "set", field, value])
        after = cli_read(payload)
        if not same(after["fields"][field], value):
            raise ValueError("Roster change failed verification.")
        return {**after, "field": field, "previousValue": before["fields"][field], "previousDisplay": before["display"][field], "value": after["fields"][field], "valueDisplay": after["display"][field], "verified": True}

    def language_get(_payload):
        output = cli_run(["settings", "language", "get"])
        match = re.search(r"[:：]\s*(auto|en|zh_hans|zh_hant|ja|ko|es|fr|it|de)\s*$", output, re.I)
        if not match:
            raise ValueError("Could not read roster language: " + repr(output))
        return {"language": match[1].lower()}

    def language_set(payload):
        value = str(payload.get("language", "")).strip().lower()
        if value not in ("auto", "en", "zh_hans", "zh_hant", "ja", "ko", "es", "fr", "it", "de"):
            raise ValueError("Unsupported language.")
        cli_run(["settings", "language", "set", value])
        result = language_get({})
        if result["language"] != value:
            raise ValueError("Roster CLI language readback did not match the requested setting.")
        return result

    def chest_fix(payload):
        resources = preview_resources(payload)
        target = Path(resources["filePath"])
        valid_player(target)
        jersey = next((row["path"] for row in resources["options"]["companionPaths"] if row["kind"] == "preview-jersey" and "jersey" in row["name"]), None)
        if not jersey:
            raise ValueError("Static NBA 2K27 jersey could not be extracted.")
        settings = bridge.fast_settings()
        with tempfile.TemporaryDirectory(prefix="chest-", dir=session) as folder:
            output = Path(folder) / target.name
            args = [settings["blender_exe"], "--background", "--factory-startup", "--python", str(engine / "electron/blender_apply_chest_correction.py"), "--", "--iff", str(target), "--addon", settings["head_swap_tool"], "--jersey", jersey, "--output", str(output)]
            if payload.get("allowClean"):
                args.append("--allow-clean")
            result = subprocess.run(args, cwd=engine, capture_output=True, timeout=300, creationflags=subprocess.CREATE_NO_WINDOW)
            text = (result.stdout + result.stderr).decode("utf-8", errors="replace")
            for line in text.splitlines():
                if line.startswith("CHARMOD_CHEST_CLEAN|"):
                    return {**json.loads(line.split("|", 1)[1]), "target": str(target)}
            success = next((line for line in text.splitlines() if line.startswith("CHARMOD_CHEST_SUCCESS|")), "")
            if not success or not output.is_file():
                raise ValueError("Blender chest correction failed: " + text[-4000:])
            replace_player(output, target, Path(folder) / "backup.iff")
            return {**json.loads(success.split("|", 1)[1]), "output": str(target), "target": str(target)}

    def close_player_seams(payload):
        from prototype_seams import close_seams
        from prototype_player_writes import publish_player
        target = Path(payload["target"]).resolve(strict=True)
        if target.suffix.lower() != ".iff":
            raise ValueError("Choose a staged player IFF before closing seams.")
        valid_player(target)
        original_digest = file_digest(target)
        with tempfile.TemporaryDirectory(prefix="seams-", dir=session) as folder:
            output = Path(folder) / target.name
            report = close_seams(target, output)
            if report["applied"]:
                backup_dir = data / "Player Backups" / "seams"
                no_links(backup_dir, data)
                backup_dir.mkdir(parents=True, exist_ok=True)
                backup = backup_dir / f"{target.stem}-{original_digest[:20]}.iff"
                no_links(backup, data)
                publish_player(output, target, backup, validate=valid_player,
                               digest=file_digest, expected_digest=original_digest)
                report["backup"] = str(backup)
            return {**report, "output": str(target)}

    def replace_player(source, target, backup):
        from prototype_player_writes import publish_player
        publish_player(source, target, backup, validate=valid_player, digest=file_digest)

    def confirmed_blender_return(folder, returned):
        if not returned.is_file():
            return True
        try:
            marker = folder / '.cmt-return.json'
            no_links(marker, data)
            no_links(returned, data)
            if marker.stat().st_size > 32768 or returned.stat().st_size > 512 * 1024 * 1024:
                return False
            confirmation = json.loads(marker.read_text(encoding='utf-8'))
            if not isinstance(confirmation, dict):
                return False
            digest = file_digest(returned)
            if confirmation.get('appliedDigest') == digest:
                return True
            recovery = Path(confirmation.get('recoveryPath', ''))
            no_links(recovery, data)
            if not recovery.resolve().is_relative_to((data / 'Blender Recovery').resolve()):
                return False
            if recovery.stat().st_size != returned.stat().st_size:
                return False
            return confirmation.get('recoveryDigest') == digest and file_digest(recovery) == digest
        except (OSError, ValueError, TypeError):
            return False

    def blender_open(payload):
        from prototype_blender_snapshot import launch_inputs, copy_input
        snapshot = launch_inputs(payload, session) if payload.get("_blenderSnapshot") else None
        target = Path(snapshot["target"] if snapshot else payload["target"]).resolve()
        valid_player(Path(snapshot["source"]) if snapshot else target)
        settings = snapshot["settings"] if snapshot else bridge.fast_settings()
        folder = Path(tempfile.mkdtemp(prefix="blender-", dir=session))
        try:
            source_folder = folder / "input"
            source_folder.mkdir()
            source = source_folder / target.name
            if snapshot:
                copy_input(snapshot, snapshot["source"], source)
            else:
                shutil.copyfile(target, source)
            expected_digest = snapshot["expectedDigest"] if snapshot else file_digest(source)
            atomic_json(folder / 'handoff.json', {'target': str(target), 'expectedDigest': expected_digest})
            returned = folder / target.name
            signal = folder / "return.json"
            session_id = folder.name
            event = lambda state, message, **extra: bridge.emit_event("blender", message, state=state, sessionId=session_id, handoffId=payload.get("handoffId"), target=str(target), requestedTarget=str(payload["target"]), **extra)
            args = [settings["blender_exe"], "--python", str(engine / "electron/blender_open_output.py"), "--", "--iff", str(source), "--addon", settings["head_swap_tool"], "--return-iff", str(returned), "--signal", str(signal)]
            if payload.get("includeJersey") or payload.get("fitChest"):
                resources = snapshot["resources"] if snapshot else preview_resources(payload)
                jersey = next((row["path"] for row in resources["options"]["companionPaths"] if row["kind"] == "preview-jersey" and "jersey" in row["name"]), None)
                if not jersey:
                    raise ValueError("Static jersey could not be extracted.")
                reference = folder / "static-jersey.iff"
                if snapshot:
                    copy_input(snapshot, jersey, reference)
                else:
                    shutil.copyfile(jersey, reference)
                args.extend(["--jersey", str(reference)])
            if payload.get("fitChest"):
                args.append("--fit-chest")
            bundle = payload.get("referenceBundle")
            if bundle:
                textures = []
                for index, texture in enumerate(bundle.get("textures", [])):
                    path = folder / f"texture-{index}.dds"
                    decoded = base64.b64decode(texture.pop("bytes"), validate=True)
                    if len(decoded) > 16 * 1024 * 1024 or decoded[:4] != b"DDS ":
                        raise ValueError("Invalid Blender preview texture.")
                    path.write_bytes(decoded)
                    textures.append({**texture, "path": str(path)})
                bundle["textures"] = textures
                path = folder / "blender-preview-references.json"
                atomic_json(path, bundle)
                args.extend(["--reference-bundle", str(path)])
            child = subprocess.Popen(args, cwd=engine, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        except Exception:
            shutil.rmtree(folder, ignore_errors=True)
            raise
        event("opened", "Blender is opening. Use Send Tweaks Back to return changes.", jerseyLoaded=bool(payload.get("includeJersey")))
        try:
            while True:
                if signal.is_file():
                    value = json.loads(signal.read_text(encoding="utf-8"))
                    if value.get("status") == "success":
                        event("returning", "Validating Blender changes...")
                        if payload.get("managedReturn"):
                            event("return-ready", "Blender changes are ready.", returned=str(returned), backup=str(folder / "backup.iff"), expectedDigest=expected_digest, eyesRecalculated=bool(value.get("eyesRecalculated")))
                        else:
                            blender_apply_return({'target': str(target), 'returned': str(returned),
                                'backup': str(folder / 'backup.iff'), 'expectedDigest': expected_digest})
                            event("complete", f"{target.name} updated from Blender.", output=str(target), eyesRecalculated=bool(value.get("eyesRecalculated")))
                        # Keep the supervision process alive while Blender owns files.
                        child.wait()
                        return {"target": str(target), "sessionId": session_id}
                    if value.get("status") == "error":
                        raise ValueError(value.get("message", "Blender could not export the player."))
                if child.poll() is not None:
                    event("closed", "Blender closed without returning changes.")
                    return {"target": str(target), "sessionId": session_id}
                time.sleep(0.5)
        except Exception as error:
            event("error", str(error), returned=str(returned), expectedDigest=expected_digest)
            try:
                child.wait()
            except Exception:
                pass
            raise
        finally:
            if confirmed_blender_return(folder, returned):
                shutil.rmtree(folder, ignore_errors=True)

    def blender_apply_return(payload):
        returned = Path(payload["returned"]).resolve()
        backup = Path(payload["backup"]).resolve()
        if not returned.is_relative_to(session.resolve()) or not backup.is_relative_to(session.resolve()):
            raise ValueError("Blender return must belong to this app session.")
        no_links(Path(payload["returned"]), session)
        no_links(Path(payload["backup"]), session)
        target = Path(payload["target"]).resolve()
        if payload.get("expectedDigest") and file_digest(target) != payload["expectedDigest"]:
            raise ValueError("The staged player changed while Blender was open. Reopen it before returning tweaks.")
        replace_player(returned, target, backup)
        if returned.parent.name.startswith('blender-'):
            try:
                atomic_json(returned.parent / '.cmt-return.json', {'appliedDigest': file_digest(target)})
            except OSError:
                pass
        return {"output": str(target), "eyesRecalculated": bool(payload.get("eyesRecalculated"))}

    original_swap = bridge.command_run_swap
    def run_swap(payload):
        target_game = str(payload.get("targetGame") or "").strip()
        if target_game not in ("NBA 2K23", "NBA 2K26", "NBA 2K27"):
            target_game = bridge.detect_modern_target_game(payload.get("target"))
        result = original_swap(payload)
        if target_game != "NBA 2K27":
            return result
        output = result.get("output")
        if isinstance(output, dict):
            output = output.get("mainPath") or output.get("path")
        if not output or not Path(output).is_file():
            raise ValueError("Full Swap did not produce a readable player for final geometry checks.")
        if payload.get("sourceGame") in ("NBA 2K18", "NBA 2K19", "NBA 2K20"):
            bridge.emit_event("progress", "Checking chest clearance against the static jersey...")
            result["automaticChestCorrection"] = chest_fix({"target": output, "allowClean": True})
        bridge.emit_event("progress", "Closing neck/chest seams on the converted player...")
        try:
            report = close_player_seams({"target": output})
        except (ValueError, RuntimeError, OSError, zipfile.BadZipFile) as error:
            report = {"output": str(output), "applied": False, "skipped": True,
                      "warnings": [f"Automatic seam closing skipped: {error}"]}
        result["automaticSeamCorrection"] = report
        if report.get("applied"):
            bridge.emit_event("log", f"Closed neck/chest seams: {report['correctedVertices']} vertices adjusted; vertex count unchanged.")
            bridge.emit_event("log", f"Seam backup: {report['backup']}")
        elif not report.get("warnings"):
            bridge.emit_event("log", "Neck/chest seams already closed; no changes needed.")
        for warning in report.get("warnings", []):
            bridge.emit_event("warning", warning)
        return result

    def headband_apply(payload):
        from character_mod_tool import CharacterModTool
        original_fit = CharacterModTool.fit_built_in_headband_to_character
        def fit(app, stock, donor, target, output, work_dir, asset_name, head_bone=48):
            with tempfile.TemporaryDirectory(prefix=".headband-fit-", dir=work_dir) as temporary:
                prepared = Path(temporary) / "target.iff"
                expanded = fit_input_copy(target, prepared, lambda: bridge.game_root_for(bridge.fast_settings(), "NBA 2K27"))
                if expanded:
                    bridge.emit_event("progress", f"Prepared {expanded} compressed buffers for headband fitting.")
                return original_fit(app, stock, donor, str(prepared), output, work_dir, asset_name, head_bone)
        CharacterModTool.fit_built_in_headband_to_character = fit
        try:
            return bridge.command_headband_apply(payload)
        finally:
            CharacterModTool.fit_built_in_headband_to_character = original_fit

    original_health = bridge.command_health
    original_repair = bridge.command_health_repair
    cache_paths = [("manifest-cache", data / "manifest-cache", 256 * 1024 * 1024),
                   ("hair-cache", session / "hair-cache", 256 * 1024 * 1024),
                   ("preview-resources", cache, 512 * 1024 * 1024)]

    def no_links(path, root):
        # Normalize lexically before checking every ancestor for reparse points.
        # Windows package virtualization may resolve a child to an overlay while
        # its existing parent resolves to the physical AppData directory.
        path, root = Path(os.path.abspath(path)), Path(os.path.abspath(root))
        if not path.is_relative_to(root):
            raise ValueError("Cache path is outside this app's storage.")
        for part in (path, *path.parents):
            if part.exists() and part.stat(follow_symlinks=False).st_file_attributes & 0x400:
                raise ValueError("Cache links are not supported.")
            if part == root:
                break

    def health(_payload):
        result = original_health({})
        checks = []
        executable = engine / "tools/live_roster/roster.exe"
        digest = hashlib.sha256(executable.read_bytes()).hexdigest().upper() if executable.is_file() else ""
        checks.append({"id": "roster-cli", "label": "Roster CLI", "status": "pass" if digest == "6F09FCEB91CAF946FF82FEEEE677256D50958BA9879B01478CEF4E6E3D2A3FCC" else "error",
                       "message": "Bundled roster CLI integrity verified." if digest == "6F09FCEB91CAF946FF82FEEEE677256D50958BA9879B01478CEF4E6E3D2A3FCC" else "Bundled roster CLI integrity check failed.", "path": str(executable)})
        templates = templates_load({})
        checks.append({"id": "target-templates", "label": "Target templates", "status": "warning" if templates["warning"] else "pass",
                       "message": templates["warning"] or "Target templates are readable.", "path": templates["path"]})
        for cache_name, path, limit in cache_paths:
            try:
                no_links(path, data)
                size = 0
                for directory, folders, files in os.walk(path, followlinks=False):
                    folders[:] = [name for name in folders if not (Path(directory, name).stat(follow_symlinks=False).st_file_attributes & 0x400)]
                    for name in files:
                        file = Path(directory, name)
                        if not file.stat(follow_symlinks=False).st_file_attributes & 0x400:
                            size += file.stat().st_size
                path.mkdir(parents=True, exist_ok=True)
                with tempfile.NamedTemporaryFile(prefix=".health-", dir=path):
                    pass
                checks.append({"id": cache_name, "label": cache_name.replace("-", " ").title(), "status": "warning" if size > limit else "pass",
                               "message": f"{size / 1048576:.1f} MiB stored; limit {limit / 1048576:.0f} MiB.", "path": str(path), "repair": {"action": "clean-caches", "label": "Clear App Caches"}})
            except (OSError, ValueError) as error:
                checks.append({"id": cache_name, "label": cache_name, "status": "error", "message": str(error), "path": str(path)})
        result["groups"].append({"id": "application", "label": "Application And Caches", "checks": checks})
        result["overall"], result["counts"] = bridge.summarize_health(result["groups"])
        return result

    def health_repair(payload):
        if payload.get("action") == "clean-caches":
            for _, path, _ in cache_paths:
                no_links(path, data)
                shutil.rmtree(path, ignore_errors=False) if path.exists() else None
        original_repair(payload)
        return health({})

    return {"preview-resources": preview_resources, "preview-chest-fix": chest_fix,
            "stage-player": lambda payload: prototype_staging.stage_player(payload, bridge, session),
            "preview-close-seams": close_player_seams,
            "templates-load": templates_load, "templates-save": templates_save,
            "cli-read": cli_read, "cli-set": cli_set, "cli-language-get": language_get,
            "cli-language-set": language_set, "blender-open": blender_open,
            "blender-apply-return": blender_apply_return, "run-swap": run_swap,
            "headband-apply": headband_apply,
            "native-hair-install": lambda payload: prototype_accessories.install(payload, bridge),
            "accessory-recover-player": lambda payload: prototype_accessories.recover_request(payload, bridge.emit_event),
            "health": health, "health-repair": health_repair}
