"""Immutable Blender inputs. The host owns leases and serializes capture/validation."""
import copy
import hashlib
import json
import os
import re
import shutil
import stat
import zipfile
from pathlib import Path

MAX_FILE = 512 * 1024 * 1024
MAX_BYTES = 1024 * 1024 * 1024
MAX_FILES = 512


def is_link(path):
    return (path.is_symlink() or getattr(path, 'is_junction', lambda: False)()
            or bool(getattr(path.lstat(), 'st_file_attributes', 0)
                    & getattr(stat, 'FILE_ATTRIBUTE_REPARSE_POINT', 0)))


def digest(path):
    value = hashlib.sha256()
    with Path(path).open('rb') as source:
        for block in iter(lambda: source.read(1024 * 1024), b''):
            value.update(block)
    return value.hexdigest()


def owned_root(payload, session):
    session = Path(session).resolve(strict=True)
    root = Path(payload['_blenderSnapshot'])
    if root.parent.resolve(strict=True) != session or not re.fullmatch(r'blender-snapshot-[a-zA-Z0-9-]+', root.name):
        raise ValueError('Blender snapshot must belong to this app session.')
    if is_link(root):
        raise ValueError('Blender snapshot cannot be a link.')
    return root


def regular(path, root=None):
    path = Path(path)
    if root is not None:
        relative = path.relative_to(root)
        if '..' in relative.parts:
            raise ValueError('Blender snapshot input escapes its owner.')
        for part in (path, *path.parents):
            if part == root:
                break
            if is_link(part):
                raise ValueError('Blender snapshot cannot contain links.')
    info = path.stat()
    if not stat.S_ISREG(info.st_mode) or info.st_size > MAX_FILE:
        raise ValueError('Invalid or oversized Blender snapshot input.')
    return info


def revision(path, content=False):
    path = Path(path)
    info = path.stat()
    value = [str(path.resolve()), info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns, info.st_ctime_ns]
    if content:
        regular(path)
        value.append(digest(path))
    return value


def selected_local_paths(target, config):
    from prototype_services import geometry_keys, token
    with zipfile.ZipFile(target) as archive:
        try:
            appearance = json.loads(archive.read('appearance_info.json').decode('utf-8-sig'))
        except KeyError:
            appearance = {}
    body = appearance.get('appearance', appearance)
    accessories = appearance.get('accessory_items', {})
    config = str(config or accessories.get('default_config') or accessories.get('default_out_of_uniform_config') or '')
    selected = next((row for row in accessories.get('configurations', [])
                     if str(row.get('name', '')).lower() == config.lower()), None)
    items = {token(value) for value in (selected or {}).get('items', []) if token(value)}
    geometry = geometry_keys(accessories, items)
    gender = token(body.get('gender', 'male'))
    gender = 'female' if gender.startswith('female') else 'child' if gender.startswith('child') else 'male'
    try:
        skin = int(body.get('skin color', body.get('skin_color')))
    except (TypeError, ValueError):
        skin = None
    body_files = {f'{gender}_skin.iff'}
    if skin is not None and 0 <= skin <= 99:
        body_files.update(f"{'female_' if gender == 'female' else ''}{region}_color_{skin:02d}.iff"
                          for region in ('chest', 'torso', 'legs'))
    prefix = target.stem.lower() + '_'
    paths = [target]
    for path in target.parent.iterdir():
        if path == target or path.suffix.lower() != '.iff':
            continue
        name = path.name.lower()
        relevant = name in body_files
        if not relevant and name.startswith(prefix):
            suffix = token(name[len(prefix):-4])
            relevant = (suffix == 'config_' + token(config)
                        or suffix.startswith('item_') and suffix[5:] in items
                        or suffix.startswith('geo_') and suffix[4:] in geometry)
        if relevant and path.is_file():
            paths.append(path)
    return sorted(paths, key=lambda path: path.name.casefold()), config


def generation(target, data, settings, config=None):
    target = Path(target).resolve(strict=True)
    main = revision(target, True)
    # Watch only this graph, but rescan absent candidates so new staged overrides retire it.
    paths, config = selected_local_paths(target, config)
    if revision(target, True) != main:
        raise ValueError('Blender sources or settings changed while resolving dependencies. Reopen the preview.')
    local = []
    local_bytes = 0
    for path in paths:
        if len(local) >= MAX_FILES:
            raise ValueError('Too many local Blender snapshot dependencies.')
        local_bytes += regular(path).st_size
        if local_bytes > MAX_BYTES:
            raise ValueError('Local Blender sources exceed the snapshot budget.')
        local.append(main if path == target else revision(path, True))
    settings_path = Path(data) / 'settings.json'
    stored = revision(settings_path, True) if settings_path.exists() else None
    game = Path(str(settings.get('game_root_2k27') or ''))
    source = []
    if settings.get('game_root_2k27') and game.is_dir():
        for path in sorted(game.iterdir(), key=lambda p: p.name.casefold()):
            if path.name.casefold() == 'manifest' or re.fullmatch(r'[0-9A-Z]{2}', path.name, re.I):
                if path.is_file():
                    source.append(revision(path, path.name.casefold() == 'manifest'))
        for name in ('NBA2K27.exe', 'data/oodle/oo2core_9_win64.dll'):
            path = game / name
            if path.is_file():
                source.append(revision(path))
    return {'target': str(target), 'config': config, 'local': local,
            'localPaths': [str(path) for path in paths], 'settings': stored, 'game': source,
            'effectiveSettings': settings}


def prepare(payload, bridge, engine, data, session):
    root = owned_root(payload, session)
    target = Path(payload['target']).resolve(strict=True)
    settings = copy.deepcopy(bridge.fast_settings())
    before = generation(target, data, settings, payload.get('config'))
    local_revisions = {row[0]: row for row in before['local']}
    files = []
    used = 0

    def capture(source, destination):
        nonlocal used
        source = Path(source)
        info = regular(source)
        used += info.st_size
        if used > MAX_BYTES or len(files) >= MAX_FILES:
            raise ValueError('Blender snapshot exceeds its resource budget.')
        first = revision(source, True)
        if first[0] in local_revisions and first != local_revisions[first[0]]:
            raise ValueError('A Blender source changed before snapshot capture. Reopen the preview.')
        destination.parent.mkdir(parents=True, exist_ok=True)
        with source.open('rb') as reader, destination.open('xb') as writer:
            remaining = info.st_size
            while remaining:
                block = reader.read(min(1024 * 1024, remaining))
                if not block:
                    raise ValueError('A Blender source changed during snapshot preparation.')
                writer.write(block)
                remaining -= len(block)
            if reader.read(1):
                raise ValueError('A Blender source grew during snapshot preparation.')
        if revision(source, True) != first or digest(destination) != first[-1]:
            raise ValueError('A Blender source changed during snapshot preparation. Reopen the preview.')
        files.append({'path': str(destination), 'digest': first[-1], 'size': info.st_size})
        return str(destination)

    try:
        # Resolve appearance/configuration only from the captured main and local generation.
        source = capture(target, root / 'input' / target.name)
        for path in map(Path, before['localPaths']):
            if path != target:
                capture(path, root / 'input' / path.name)
        frozen_request = {**payload, 'target': source}
        # A private extraction cache cannot reuse data from an older game archive.
        previous_session = os.environ.get('CMT_SESSION')
        os.environ['CMT_SESSION'] = str(root)
        try:
            from prototype_services import commands
            resources = copy.deepcopy(commands(bridge, Path(engine), Path(data))['preview-resources'](frozen_request))
        finally:
            if previous_session is None:
                os.environ.pop('CMT_SESSION', None)
            else:
                os.environ['CMT_SESSION'] = previous_session
        resources['filePath'] = source
        captured = {Path(row['path']).resolve(): row['path'] for row in files}
        destinations = {Path(row['path']).name.casefold() for row in files}
        for row in resources['options']['companionPaths']:
            canonical = Path(row['path']).resolve()
            if canonical in captured:
                row['path'] = captured[canonical]
                continue
            leaf = Path(row['name'].replace('\\', '/')).name
            if not leaf or leaf.casefold() in destinations:
                raise ValueError('Conflicting Blender snapshot companion names.')
            destinations.add(leaf.casefold())
            row['path'] = capture(row['path'], root / 'input' / leaf)
        for index, row in enumerate(resources['options'].get('externalTextures', [])):
            row['path'] = capture(row['path'], root / 'textures' / f'{index}.dds')
        if generation(target, data, copy.deepcopy(bridge.fast_settings()), payload.get('config')) != before:
            raise ValueError('Blender sources or settings changed during snapshot preparation. Reopen the preview.')
        resources.pop('resourceLease', None)
        resources.pop('fingerprint', None)
        resources['native'] = False
        manifest = {'target': str(target), 'source': resources['filePath'],
                    'expectedDigest': files[0]['digest'], 'settings': settings,
                    'generation': before, 'files': files, 'resources': resources}
        encoded = json.dumps(manifest)
        if len(encoded.encode('utf-8')) > 4 * 1024 * 1024:
            raise ValueError('Blender snapshot metadata exceeds its budget.')
        (root / 'snapshot.json').write_text(encoded, encoding='utf-8')
        shutil.rmtree(root / 'preview-resources', ignore_errors=True)
        return resources
    except BaseException:
        shutil.rmtree(root, ignore_errors=True)
        raise


def load(payload, session):
    root = owned_root(payload, session)
    path = root / 'snapshot.json'
    regular(path, root)
    if path.stat().st_size > 4 * 1024 * 1024:
        raise ValueError('Blender snapshot metadata exceeds its budget.')
    manifest = json.loads(path.read_text(encoding='utf-8'))
    if Path(payload['target']).resolve(strict=True) != Path(manifest['target']):
        raise ValueError('Blender snapshot belongs to a different target.')
    if len(manifest['files']) > MAX_FILES:
        raise ValueError('Too many Blender snapshot inputs.')
    total = 0
    for row in manifest['files']:
        path = Path(row['path'])
        info = regular(path, root)
        total += info.st_size
        if total > MAX_BYTES or info.st_size != row['size'] or digest(path) != row['digest']:
            raise ValueError('Blender snapshot input changed or exceeded its budget.')
    sources = {row['path']: row['digest'] for row in manifest['files']}
    if sources.get(manifest['source']) != manifest['expectedDigest']:
        raise ValueError('Invalid Blender snapshot return digest.')
    for row in manifest['resources']['options']['companionPaths']:
        if row['path'] not in sources:
            raise ValueError('Unowned Blender snapshot companion.')
    for row in manifest['resources']['options'].get('externalTextures', []):
        if row['path'] not in sources:
            raise ValueError('Unowned Blender snapshot texture.')
    if manifest['resources']['filePath'] != manifest['source']:
        raise ValueError('Unowned Blender snapshot main input.')
    return manifest


def validate(payload, bridge, data, session):
    manifest = load(payload, session)
    if generation(manifest['target'], data, copy.deepcopy(bridge.fast_settings()),
                  manifest['generation'].get('config')) != manifest['generation']:
        raise ValueError('Blender sources or settings changed while preparing. Reopen the preview.')
    return True


def launch_inputs(payload, session):
    """Called by blender_open only with the host-injected, claimed snapshot path."""
    return load(payload, session)


def copy_input(manifest, source, destination):
    """Copy only a sealed input into the existing return/recovery handoff folder."""
    source = Path(source)
    expected = next((row for row in manifest['files'] if row['path'] == str(source)), None)
    if expected is None:
        raise ValueError('Unowned Blender launch input.')
    remaining = expected['size']
    value = hashlib.sha256()
    with source.open('rb') as reader, Path(destination).open('xb') as writer:
        while remaining:
            block = reader.read(min(1024 * 1024, remaining))
            if not block:
                raise ValueError('Blender launch input changed.')
            writer.write(block)
            value.update(block)
            remaining -= len(block)
        if reader.read(1) or value.hexdigest() != expected['digest']:
            raise ValueError('Blender launch input changed.')


def commands(bridge, engine, data):
    session = Path(os.environ['CMT_SESSION'])
    return {
        'blender-snapshot-prepare': lambda payload: prepare(payload, bridge, engine, data, session),
        'blender-snapshot-validate': lambda payload: validate(payload, bridge, data, session),
    }
