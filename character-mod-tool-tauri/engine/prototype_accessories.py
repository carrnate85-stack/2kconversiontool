"""Validated, recoverable native accessory installs; geometry codecs stay in hair_tools."""
import contextlib
import copy
import ctypes
import hashlib
import json
import os
import re
import shutil
import subprocess
import tempfile
import uuid
import zipfile
from dataclasses import replace
from datetime import datetime
from pathlib import Path


def digest(path):
    value = hashlib.sha256()
    with Path(path).open('rb') as handle:
        for block in iter(lambda: handle.read(65536), b''):
            value.update(block)
    return value.hexdigest()


def archive_members(path):
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        if (len(infos) > 5000 or sum(i.file_size for i in infos) > 512 * 1024 * 1024
                or any(i.file_size > 128 * 1024 * 1024 for i in infos)
                or len({i.filename.casefold() for i in infos}) != len(infos)):
            raise ValueError(f'Accessory archive has duplicate members or exceeds limits: {Path(path).name}')
        return infos, {i.filename: archive.read(i) for i in infos}, archive.comment


def read_appearance(path, backend):
    infos, values, comment = archive_members(path)
    names = [n for n in values if Path(n).name.casefold() == 'appearance_info.json']
    if len(names) != 1:
        raise ValueError('Accessory installation requires one decoded appearance_info.json.')
    value = backend.read_hair_document(values[names[0]])
    if not isinstance(value, dict) or not isinstance(value.get('accessory_items'), dict):
        raise ValueError('The player has no supported accessory definitions.')
    return value, (infos, values, comment, names[0])


def write_appearance(path, value, snapshot):
    infos, values, comment, name = snapshot
    data = (json.dumps(value, indent=2, ensure_ascii=False) + '\n').encode('utf-8')
    with zipfile.ZipFile(path, 'x') as archive:
        archive.comment = comment
        for info in infos:
            archive.writestr(info, data if info.filename == name else values[info.filename])


def variants(item):
    mode = str(item.get('tangentspace') or 'both').strip().lower()
    if mode not in ('no', 'yes', 'both'):
        raise ValueError(f'Unsupported accessory geometry mode: {mode}')
    return (False,) if mode == 'no' else (True,) if mode == 'yes' else (False, True)


def atomic_json(path, value):
    fd, name = tempfile.mkstemp(prefix='.' + path.name + '-', suffix='.preparing', dir=path.parent)
    temporary = Path(name)
    with os.fdopen(fd, 'w', encoding='utf-8') as handle:
        json.dump(value, handle, indent=2)
        handle.flush()
        os.fsync(handle.fileno())
    os.replace(temporary, path)


def flush_file(path):
    # Windows FlushFileBuffers requires a writable handle, even after the writer closed.
    with path.open('r+b') as handle:
        os.fsync(handle.fileno())


def no_link(path):
    if path.is_symlink() or (path.exists() and getattr(path.stat(follow_symlinks=False), 'st_file_attributes', 0) & 0x400):
        raise ValueError('Accessory transactions do not follow file or directory links.')


def transaction_contents(folder):
    pending, result = [folder], []
    while pending:
        parent = pending.pop()
        for path in parent.iterdir():
            result.append(path)
            metadata = path.stat(follow_symlinks=False)
            linked = path.is_symlink() or getattr(metadata, 'st_file_attributes', 0) & 0x400
            if not linked and path.is_dir():
                pending.append(path)
    return result


class OwnedFiles:
    """Only the current job may retire files whose identity and bytes it recorded."""
    def __init__(self, folder):
        self.folder = folder
        self.entries = {}
        self.remember(folder)

    def remember(self, path):
        no_link(path)
        metadata = path.stat(follow_symlinks=False)
        self.entries[path.relative_to(self.folder).as_posix()] = (
            metadata.st_dev, metadata.st_ino, None if path.is_dir() else digest(path))

    def changed(self, path):
        relative = path.relative_to(self.folder).as_posix()
        expected = self.entries.get(relative)
        if expected is None:
            return True
        try:
            no_link(path)
            metadata = path.stat(follow_symlinks=False)
            actual = (metadata.st_dev, metadata.st_ino, None if path.is_dir() else digest(path))
        except (OSError, ValueError):
            return True
        return actual != expected

    def cleanup(self):
        contents = transaction_contents(self.folder)
        unexpected = [p.relative_to(self.folder).as_posix() for p in contents
                      if p.relative_to(self.folder).as_posix() not in self.entries]
        changed = [p.relative_to(self.folder).as_posix() for p in contents
                   if p.relative_to(self.folder).as_posix() in self.entries and self.changed(p)]
        if unexpected or changed:
            return {'path': str(self.folder), 'unexpected': unexpected, 'changed': changed, 'preserved': True}
        for path in sorted(contents, key=lambda p: len(p.parts), reverse=True):
            if self.changed(path):
                return {'path': str(self.folder), 'unexpected': [], 'changed': [str(path)], 'preserved': True}
            try:
                if path.is_dir():
                    path.rmdir()
                else:
                    path.unlink()
            except OSError:
                return {'path': str(self.folder), 'unexpected': [], 'changed': [str(path)], 'preserved': True}
        if self.changed(self.folder):
            raise RuntimeError('The accessory transaction directory changed; it was preserved.')
        try:
            self.folder.rmdir()
        except OSError:
            return {'path': str(self.folder), 'unexpected': [p.name for p in self.folder.iterdir()],
                    'changed': [], 'preserved': True}
        return None


def process_alive(pid):
    if pid <= 0:
        return True
    if os.name != 'nt':
        try:
            os.kill(pid, 0)
            return True
        except ProcessLookupError:
            return False
        except PermissionError:
            return True
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.OpenProcess.restype = ctypes.c_void_p
    kernel.OpenProcess.argtypes = (ctypes.c_ulong, ctypes.c_int, ctypes.c_ulong)
    kernel.CloseHandle.argtypes = (ctypes.c_void_p,)
    kernel.GetExitCodeProcess.argtypes = (ctypes.c_void_p, ctypes.POINTER(ctypes.c_ulong))
    handle = kernel.OpenProcess(0x1000, False, pid)
    if not handle:
        return ctypes.get_last_error() != 87
    try:
        code = ctypes.c_ulong()
        return not kernel.GetExitCodeProcess(handle, ctypes.byref(code)) or code.value == 259
    finally:
        kernel.CloseHandle(handle)


@contextlib.contextmanager
def mutation_gate(target):
    if os.name != 'nt':
        import fcntl
        with target.with_name(f'.{target.stem}.accessory-install.lock').open('a+b') as handle:
            try:
                fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                raise RuntimeError('An accessory install is already running for this player.')
            yield
        return
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.CreateMutexW.argtypes = (ctypes.c_void_p, ctypes.c_int, ctypes.c_wchar_p)
    kernel.CreateMutexW.restype = ctypes.c_void_p
    kernel.WaitForSingleObject.argtypes = (ctypes.c_void_p, ctypes.c_ulong)
    kernel.ReleaseMutex.argtypes = (ctypes.c_void_p,)
    kernel.CloseHandle.argtypes = (ctypes.c_void_p,)
    key = hashlib.sha256(str(target).casefold().encode('utf-8')).hexdigest()
    handle = kernel.CreateMutexW(None, False, 'Local\\CmtAccessoryInstall-' + key)
    if not handle:
        raise OSError('Could not create the accessory installation lock.')
    acquired = False
    try:
        # A named OS mutex serializes stale-lock reclamation and dies with its owner.
        acquired = kernel.WaitForSingleObject(handle, 0) in (0, 0x80)
        if not acquired:
            raise RuntimeError('An accessory install is already running for this player.')
        yield
    finally:
        if acquired:
            kernel.ReleaseMutex(handle)
        kernel.CloseHandle(handle)


@contextlib.contextmanager
def config_lock(target):
    # Share the existing config-edit lock, but never steal an unknown/legacy lock.
    lock = target.with_name(f'.{target.stem}.config.lock')
    owner = {'kind': 'cmt-accessory-install', 'pid': os.getpid(), 'token': uuid.uuid4().hex}
    for attempt in range(2):
        try:
            with lock.open('x', encoding='utf-8') as handle:
                json.dump(owner, handle)
                handle.flush()
                os.fsync(handle.fileno())
            break
        except FileExistsError:
            try:
                previous_bytes = lock.read_bytes()
                previous = json.loads(previous_bytes)
                stale = (previous.get('kind') == owner['kind']
                         and not process_alive(int(previous['pid'])))
            except (OSError, ValueError, KeyError, TypeError):
                stale = False
            if attempt or not stale:
                raise RuntimeError('An accessory/config edit is already running for this player.')
            if lock.read_bytes() != previous_bytes:
                raise RuntimeError('The player edit lock changed. Retry the installation.')
            lock.unlink()
    try:
        recovery = recover_pending(target)
        yield recovery
    finally:
        try:
            owned = json.loads(lock.read_text(encoding='utf-8')).get('token') == owner['token']
        except (OSError, ValueError, AttributeError):
            owned = False
        if owned:
            lock.unlink()


@contextlib.contextmanager
def player_lock(target):
    with mutation_gate(target), config_lock(target) as recovery:
        yield recovery


def checked_transaction(folder, target):
    no_link(folder)
    for path in (folder / 'journal.json', folder / 'rollback', folder / 'prepared'):
        no_link(path)
    if folder.resolve().parent != target.parent:
        raise ValueError('Accessory recovery folder is outside its player directory.')
    journal = json.loads((folder / 'journal.json').read_text(encoding='utf-8'))
    if (journal.get('version') != 1 or journal.get('target') != target.name
            or journal.get('owner') != 'cmt-accessory-install'):
        raise ValueError('Unrecognized accessory recovery journal; files were preserved.')
    names = set()
    for record in journal['records']:
        name = record['name']
        if (Path(name).name != name or '/' in name or '\\' in name
                or not (name == target.name or name.startswith(target.stem + '_'))
                or not name.lower().endswith('.iff') or name.casefold() in names):
            raise ValueError('Invalid accessory recovery destination; files were preserved.')
        names.add(name.casefold())
        for path in (target.parent / name, folder / 'rollback' / name):
            no_link(path)
    return journal


def rollback(folder, target, owned=None):
    journal = checked_transaction(folder, target)
    changed = False
    for record in reversed(journal['records']):
        destination = target.parent / record['name']
        current = digest(destination) if destination.exists() else None
        if current == record['before']:
            continue
        if current != record['after']:
            raise RuntimeError(f'Accessory recovery found a changed file: {destination.name}. Recovery data was retained.')
        if record['before'] is None:
            destination.unlink()
        else:
            saved = folder / 'rollback' / record['name']
            if not saved.is_file() or digest(saved) != record['before']:
                raise RuntimeError('Accessory rollback backup is missing or changed; recovery data was retained.')
            # Keep the durable backup until the rollback itself is journaled complete.
            fd, name = tempfile.mkstemp(prefix='.restore-', suffix='.iff', dir=folder)
            temporary = Path(name)
            with os.fdopen(fd, 'wb') as output, saved.open('rb') as source:
                shutil.copyfileobj(source, output)
                output.flush()
                os.fsync(output.fileno())
            if owned:
                owned.remember(temporary)
            os.replace(temporary, destination)
        changed = True
    journal['state'] = 'rolled_back'
    atomic_json(folder / 'journal.json', journal)
    return changed


def recover_pending(target):
    report = {'changed': False, 'transactions': [], 'warnings': []}
    for folder in sorted(target.parent.glob(f'.cmt-accessory-{target.stem}-*')):
        if not folder.is_dir():
            continue
        journal = checked_transaction(folder, target)
        recovered = False
        if journal['state'] not in ('complete', 'rolled_back'):
            report['changed'] |= rollback(folder, target)
            journal = checked_transaction(folder, target)
            recovered = True
        expected = set(journal.get('ownedFiles', [])) | {'journal.json'}
        unexpected = sorted(p.relative_to(folder).as_posix() for p in transaction_contents(folder)
                            if p.relative_to(folder).as_posix() not in expected)
        report['transactions'].append({'path': str(folder), 'state': journal['state'],
            'recovered': recovered, 'preserved': True, 'unexpected': unexpected})
        if unexpected:
            report['warnings'].append(f'Accessory transaction contents were preserved at {folder}; '
                                      f'unexpected/unverified files: {", ".join(unexpected)}')
        elif recovered:
            report['warnings'].append(f'Interrupted accessory install was rolled back; recovery evidence was retained at {folder}.')
    return report


def recover_player(path, emit=None):
    if not isinstance(path, (str, Path)) or not str(path):
        raise ValueError('Choose a player path for accessory recovery.')
    target = Path(path).resolve()
    if not re.fullmatch(r'png\d+\.iff', target.name, re.I):
        raise ValueError('Accessory recovery requires a png player IFF.')
    if not target.parent.is_dir() or not any(target.parent.glob(f'.cmt-accessory-{target.stem}-*')):
        return {'changed': False, 'transactions': [], 'warnings': []}
    with player_lock(target) as report:
        if emit:
            for warning in report['warnings']:
                emit('warning', warning)
        return report


def recover_request(payload, emit=None):
    report = {'changed': False, 'transactions': [], 'warnings': [], 'targets': []}
    seen = set()
    for field in ('target', 'source', 'filePath', 'path', 'geometryPath', 'sourcePlayerPath'):
        value = payload.get(field) if isinstance(payload, dict) else None
        if not isinstance(value, str) or '://' in value:
            continue
        path = Path(value)
        match = re.fullmatch(r'(png\d+)(?:_.*)?\.iff', path.name, re.I)
        if not match:
            continue
        target = path.with_name(match[1] + '.iff').resolve()
        if target not in seen:
            recovery = recover_player(target, emit)
            report['changed'] |= recovery['changed']
            report['transactions'].extend(recovery['transactions'])
            report['warnings'].extend(recovery['warnings'])
            report['targets'].append(str(target))
            seen.add(target)
    return report


def recovery_hooks(commands, emit):
    def wrap(command, operation):
        if command in ('native-hair-install', 'accessory-recover-player'):
            return operation
        def recovered(payload):
            recover_request(payload, emit)
            return operation(payload)
        return recovered
    return {command: wrap(command, operation) for command, operation in commands.items()}


def publish(folder, target, outputs, watched, backend, owned):
    rollback_dir = folder / 'rollback'
    rollback_dir.mkdir()
    owned.remember(rollback_dir)
    records = []
    for prepared in sorted(outputs, key=lambda p: p.name == target.name):
        destination = target.parent / prepared.name
        before = watched.get(destination.name)
        if (digest(destination) if destination.exists() else None) != before:
            raise RuntimeError(f'The staged package changed during accessory preparation: {destination.name}')
        if before is not None:
            shutil.copy2(destination, rollback_dir / destination.name)
            flush_file(rollback_dir / destination.name)
            owned.remember(rollback_dir / destination.name)
        flush_file(prepared)
        records.append({'name': prepared.name, 'before': before, 'after': digest(prepared)})
    for name, before in watched.items():
        destination = target.parent / name
        if (digest(destination) if destination.exists() else None) != before:
            raise RuntimeError(f'The staged package changed during accessory preparation: {name}')
    journal = {'version': 1, 'owner': 'cmt-accessory-install', 'target': target.name,
               'state': 'prepared', 'active': None, 'records': records,
               'ownedFiles': sorted(owned.entries)}
    atomic_json(folder / 'journal.json', journal)
    backups = []
    stamp = datetime.now().strftime('%Y%m%d_%H%M%S') + '_' + uuid.uuid4().hex[:12]
    try:
        for record in records:
            destination = target.parent / record['name']
            backup = backend.backup_existing(destination, stamp)
            if backup:
                backups.append(str(backup))
        for record in records:
            destination = target.parent / record['name']
            if (digest(destination) if destination.exists() else None) != record['before']:
                raise RuntimeError(f'The staged package changed before accessory publication: {destination.name}')
            journal.update(state='committing', active=record['name'])
            atomic_json(folder / 'journal.json', journal)
            os.replace(folder / 'prepared' / record['name'], destination)
        journal.update(state='complete', active=None)
        atomic_json(folder / 'journal.json', journal)
    except Exception:
        rollback(folder, target, owned)
        raise
    return backups


def material_names(value):
    result = set()
    if isinstance(value, dict):
        material = value.get('Material')
        if isinstance(material, dict):
            result.update(material)
        elif isinstance(material, str):
            result.add(material)
        for child in value.values():
            result.update(material_names(child))
    elif isinstance(value, list):
        for child in value:
            result.update(material_names(child))
    return result


def validate_geometry(path, item, tangent, backend):
    from hair_tools.hair_lod_policy import hair_models
    _, values, _ = archive_members(path)
    meshes, materials, checked = set(), set(), 0
    for name, data in values.items():
        if not name.lower().endswith('.scne'):
            continue
        scene = backend.read_hair_document(data)
        checked += backend.validate_hair_scene_buffers(scene, values)
        materials.update(material_names(scene))
        for model in hair_models(scene):
            position = model.get('VertexFormat', {}).get('POSITION0', {}).get('Format')
            if not position or (position == 'R16G16B16A16_UINT') != tangent:
                raise ValueError('The selected geometry does not match its regular/tangent variant; no codec relabeling is supported.')
            for stream in model['VertexStream']:
                binary = stream['Binary']
                candidates = {binary, binary.replace('.gz', '.bin'), binary.replace('.bin', '.gz')}
                found = [values[n] for n in candidates if n in values]
                if len(found) != 1 or len(backend.decode_vcz_buffer(found[0], int(stream['Size']))) != int(stream['Size']):
                    raise ValueError('Accessory vertex stream is missing, ambiguous or truncated.')
            meshes.update(p['Mesh'] for p in model['Prim'] if isinstance(p.get('Mesh'), str))
            if any(p.get('Mesh') == item.get('mesh') and p.get('Material')
                   and item.get('shader') and p['Material'] != item['shader'] for p in model['Prim']):
                raise ValueError('Accessory primary mesh and shader bindings disagree.')
    if not checked:
        raise ValueError('Accessory geometry has no supported highest-detail model.')
    if not isinstance(item.get('mesh'), str) or item['mesh'] not in meshes:
        raise ValueError(f'Accessory primary mesh {item.get("mesh")!r} is absent from {path.name}.')
    if item.get('shader') and item['shader'] not in materials:
        raise ValueError(f'Accessory shader {item["shader"]!r} is absent from {path.name}.')
    return meshes


def materialize_texture_pixels(path, backend, manifest, scratch, owned):
    infos, values, comment = archive_members(path)
    additions = {}
    for name, data in values.items():
        if not name.lower().endswith('.txtr'):
            continue
        document = backend.read_hair_document(data)
        if not isinstance(document, dict) or len(document) != 1:
            raise ValueError('Accessory texture metadata must define exactly one texture.')
        descriptor = next(iter(document.values()))
        binary = descriptor.get('Binary') if isinstance(descriptor, dict) else None
        if not isinstance(binary, str) or not re.fullmatch(r'[A-Za-z0-9_.-]+', binary):
            raise ValueError('Accessory texture has an unsupported pixel resource name.')
        dds_name = str(Path(binary).with_suffix('.dds'))
        if any(n.casefold() in (binary.casefold(), dds_name.casefold()) for n in values):
            continue
        match = re.fullmatch(r'.+\.([0-9a-f]{16})\.tld', binary, re.I)
        if not match:
            continue
        content_hash = match[1].lower()
        tld = manifest.get(f'shared/{content_hash[:2]}/{binary.lower()}')
        if tld is None:
            aliases = [row for key, row in manifest.items() if key.startswith('shared/')
                       and key.endswith('.' + content_hash + '.tld')]
            tld = aliases[0] if len(aliases) == 1 else None
        mip = manifest.get(Path(tld[0]).with_suffix('.mip0').as_posix().lower()) if tld else None
        if not tld or not mip:
            continue
        for row in (tld, mip):
            if (not re.fullmatch(r'[0-9A-Z]{2}', row[1]) or row[2] < 0
                    or not 0 < row[3] <= 128 * 1024 * 1024
                    or row[2] + row[3] > (backend.GAME_ROOT / row[1]).stat().st_size):
                raise ValueError('Accessory shared texture has an invalid archive range.')
        decoder = backend.native_vcz_decoder_path()
        if not decoder:
            raise ValueError('The verified native texture decoder is unavailable.')
        output = scratch / (uuid.uuid4().hex + '.dds')
        arguments = [str(decoder), '--texture', str(backend.GAME_ROOT / tld[1]), str(tld[2]), str(tld[3]),
                     str(backend.GAME_ROOT / mip[1]), str(mip[2]), str(mip[3]), str(backend.OODLE_DLL), str(output)]
        result = subprocess.run(arguments, capture_output=True, timeout=120,
                                creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
        if result.returncode or not output.is_file():
            raise ValueError(f'The verified native texture decoder could not materialize {binary}.')
        pixels = output.read_bytes()
        if not pixels.startswith(b'DDS ') or not 128 <= len(pixels) <= 128 * 1024 * 1024:
            raise ValueError('The native texture decoder returned invalid accessory pixels.')
        owned.remember(output)
        if dds_name in additions and additions[dds_name] != pixels:
            raise ValueError('Conflicting accessory texture pixel resources.')
        additions[dds_name] = pixels
    if additions:
        temporary = path.with_name(path.name + '.textures')
        with zipfile.ZipFile(temporary, 'x', compression=zipfile.ZIP_DEFLATED) as archive:
            archive.comment = comment
            for info in infos:
                archive.writestr(info, values[info.filename])
            for name, data in additions.items():
                archive.writestr(name, data)
        owned.remember(temporary)
        os.replace(temporary, path)
        owned.remember(path)


def texture_bindings(path, item, backend, rebind):
    _, values, _ = archive_members(path)
    textures = {}
    for name, data in values.items():
        if not name.lower().endswith('.txtr'):
            continue
        document = backend.read_hair_document(data)
        if not isinstance(document, dict) or len(document) != 1:
            raise ValueError('Accessory texture metadata must define exactly one texture.')
        logical, descriptor = next(iter(document.items()))
        binary = descriptor.get('Binary') if isinstance(descriptor, dict) else None
        if not isinstance(binary, str):
            raise ValueError(f'Accessory texture {name} has no pixel resource.')
        candidates = {binary.casefold(), str(Path(binary).with_suffix('.dds')).casefold()}
        payloads = [v for n, v in values.items() if n.casefold() in candidates]
        if len(payloads) != 1 or not payloads[0].startswith(b'DDS ') or len(payloads[0]) < 128:
            raise ValueError(f'Accessory texture {name} has missing or unsupported decoded pixels.')
        key = re.sub(r'_o$', '', logical, flags=re.I)
        if key.casefold() in textures:
            raise ValueError('Accessory texture package has ambiguous logical bindings.')
        textures[key.casefold()] = key
    bindings = item.get('texture')
    if not isinstance(bindings, dict) or not bindings or not textures:
        raise ValueError('Accessory installation requires a complete texture package and donor bindings.')
    for channel, logical in list(bindings.items()):
        if not isinstance(logical, str) or not logical:
            raise ValueError('Accessory texture binding is not a nonempty name.')
        key = re.sub(r'_o$', '', logical, flags=re.I).casefold()
        if key in textures:
            continue
        matches = [actual for key, actual in textures.items() if key.endswith('_' + channel.casefold())]
        # Stock 2K27 hair/beard appearances name an optional occlusion input even
        # when their authored packages contain only color, tangent and lightmap.
        if channel.casefold() == 'occlusion' and not matches:
            continue
        if not rebind or len(matches) != 1:
            raise ValueError(f'Accessory texture binding {logical!r} ({channel}) cannot be resolved without guessing.')
        bindings[channel] = matches[0]


def source_copy(local, archive, destination, backend, manifest, required=True, owned=None):
    if local:
        if not Path(local).is_file():
            raise ValueError(f'The selected staged accessory source is missing: {local}')
        shutil.copy2(local, destination)
    elif archive and (required or archive.casefold() in manifest):
        backend.extract_archive_iff_fallback(archive, destination)
    elif required:
        raise ValueError(f'The selected accessory companion is unavailable: {archive or destination.name}')
    else:
        return None
    archive_members(destination)
    if owned:
        owned.remember(destination)
    return destination


def prepare(payload, bridge, backend, target, folder, watched, owned):
    import configs_backend
    appearance, snapshot = read_appearance(target, backend)
    kind = str(payload.get('assetType') or 'hair').strip().lower()
    if kind not in ('hair', 'facialhair'):
        raise ValueError('Asset type must be hair or facialhair.')
    if payload.get('configScoped') and not payload.get('config'):
        raise ValueError('Select a configuration in the Configs tab first.')
    config = configs_backend.config_named(appearance, payload['config']) if payload.get('config') else None
    asset_id = str(payload.get('assetId') or '').strip()
    if not asset_id:
        raise ValueError('Choose a source asset from the NBA 2K27 manifest.')
    option = bridge.selected_native_hair_option(backend, kind, asset_id)
    manifest = backend.parse_manifest_entries()
    inputs, prepared = folder / 'inputs', folder / 'prepared'
    inputs.mkdir()
    prepared.mkdir()
    owned.remember(inputs)
    owned.remember(prepared)
    target_input = inputs / target.name
    shutil.copy2(target, target_input)
    owned.remember(target_input)
    source_head = (Path(option.source_geo).parent / f'png{option.source_png}.iff') if option.source_geo else None
    if source_head and not source_head.exists():
        source_head = None
    donor = source_copy(source_head, f'char/sig/png{option.source_png}.iff', inputs / 'donor.iff', backend, manifest, owned=owned)
    donor_appearance, _ = read_appearance(donor, backend)
    donors = [i for i in donor_appearance['accessory_items'].get('items', []) if isinstance(i, dict)
              and i.get('type') == kind
              and backend.item_asset_key(str(i.get('name', ''))).casefold() == option.hair_key.casefold()]
    if len(donors) != 1:
        raise ValueError('The donor appearance does not uniquely define the selected accessory.')
    item = copy.deepcopy(donors[0])
    definitions = appearance['accessory_items'].get('items', [])
    if config is not None:
        key = configs_backend.unique_item(kind, config['name'])
        original_items = []
        required = variants(item)
    else:
        key = backend.validate_target_asset_key(str(payload.get('targetKey') or '').strip())
        active = {n for c in appearance['accessory_items'].get('configurations', []) for n in c.get('items', [])}
        original_items = [i for i in definitions if isinstance(i, dict) and i.get('type') == kind
                          and i.get('name') in active and backend.item_asset_key(str(i.get('name', ''))).casefold() == key.casefold()]
        if not original_items or len({variants(i) for i in original_items}) != 1:
            raise ValueError('Choose an existing accessory slot with one consistent geometry mode.')
        key = backend.item_asset_key(str(original_items[0]['name']))
        required = variants(original_items[0])
    png = re.fullmatch(r'png(\d+)\.iff', target.name, re.I)[1]
    item.update(name=key, type=kind, tangentspace='both' if len(required) == 2 else 'yes' if required[0] else 'no')
    outputs = []
    for tangent in required:
        suffix = '_tangentspace' if tangent else ''
        local = option.source_geo if bool(option.tangentspace) == tangent else None
        if not local and option.source_geo:
            selected = Path(option.source_geo)
            base = re.sub(r'_tangentspace$', '', selected.stem, flags=re.I)
            sibling = selected.with_name(base + suffix + selected.suffix)
            if sibling.is_file() and sibling != selected:
                local = sibling
        archive = (option.archive_geo if bool(option.tangentspace) == tangent else
                   f'char/sig/png{option.source_png}_geo_{option.hair_key}{suffix}.iff')
        source = source_copy(local, archive, inputs / f'source{suffix}.iff', backend, manifest,
                             required=bool(option.tangentspace) == tangent, owned=owned)
        if source is None:
            raise ValueError(f'The selected accessory has no verified {"tangent" if tangent else "regular"} companion. Existing codecs cannot generate this combination.')
        installed = replace(option, source_geo=source, archive_geo=None, tangentspace=tangent)
        fitted = None
        try:
            if kind == 'hair' and not tangent:
                original_extract = backend.extract_archive_iff_fallback
                def snapshot_donor(archive, destination):
                    if archive.casefold() == f'char/sig/png{option.source_png}.iff'.casefold():
                        shutil.copy2(donor, destination)
                        return destination
                    return original_extract(archive, destination)
                backend.extract_archive_iff_fallback = snapshot_donor
                try:
                    installed, fitted = backend.prepare_tangent_fitted_option(
                        installed, png, target_head_path=str(target_input),
                        status_callback=lambda message: bridge.emit_event('progress', message))
                finally:
                    backend.extract_archive_iff_fallback = original_extract
            destination = prepared / f'{target.stem}_geo_{key}{suffix}.iff'
            backend.copy_geo_with_target_names(installed.source_geo, destination,
                backend.scne_asset_name(option.hair_key, tangent), backend.scne_asset_name(key, tangent))
            owned.remember(destination)
            validate_geometry(destination, item, tangent, backend)
            outputs.append(destination)
        finally:
            if fitted:
                shutil.rmtree(fitted)
    if item.get('meshlo'):
        item['meshlo'] = item['mesh']
    texture_output = prepared / f'{target.stem}_item_{key}.iff'
    copy_item = config is not None or kind == 'facialhair' or bool(payload.get('copyItem', True))
    existing_texture = target.parent / texture_output.name
    if copy_item:
        source_copy(option.source_item, option.archive_item, texture_output, backend, manifest, owned=owned)
    else:
        watched[existing_texture.name] = digest(existing_texture) if existing_texture.is_file() else None
        source_copy(existing_texture if existing_texture.is_file() else None,
                    f'char/sig/{existing_texture.name}', texture_output, backend, manifest, owned=owned)
        if len({json.dumps(i.get('texture'), sort_keys=True) for i in original_items}) != 1:
            raise ValueError('Keeping textures requires one consistent existing slot binding.')
        item['texture'] = copy.deepcopy(original_items[0].get('texture'))
    materialize_texture_pixels(texture_output, backend, manifest, inputs, owned)
    texture_bindings(texture_output, item, backend, rebind=copy_item)
    outputs.append(texture_output)
    if config is not None:
        configs_backend.replace_type(appearance, config, item)
    else:
        for previous in original_items:
            replacement = copy.deepcopy(item)
            replacement['name'] = previous['name']
            definitions[definitions.index(previous)] = replacement
    output_player = prepared / target.name
    write_appearance(output_player, appearance, snapshot)
    owned.remember(output_player)
    outputs.append(output_player)
    for output in outputs:
        archive_members(output)
    result = {'target': str(target), 'targetPng': png, 'assetType': kind,
        'sourcePng': option.source_png, 'sourceKey': option.hair_key, 'targetKey': key,
        'texturesCopied': copy_item, 'activeConfig': config['name'] if config is not None else '',
        'warnings': []}
    if config is not None:
        accessory = appearance['accessory_items']
        result.update(items=accessory['items'], defaultConfig=accessory.get('default_config', ''),
                      outOfUniformConfig=accessory.get('default_out_of_uniform_config', ''), configs=[])
        for row in accessory.get('configurations', []):
            texture = target.with_name(f'{target.stem}_config_{row["name"].lower()}.iff')
            result['configs'].append({'name': row['name'], 'items': row.get('items', []),
                                     'textureFile': texture.name, 'texturePresent': texture.is_file()})
    return outputs, result


def install(payload, bridge):
    target = Path(bridge.require_file(payload.get('target'), 'Working player')).resolve()
    if not re.fullmatch(r'png\d+\.iff', target.name, re.I):
        raise ValueError('Choose a staged png player IFF.')
    backend = bridge.native_hair_backend()
    with player_lock(target) as recovery:
        watched = {p.name: digest(p) for p in target.parent.iterdir() if p.is_file()
                   and (p.name == target.name or p.name.startswith(target.stem + '_')) and p.suffix.lower() == '.iff'}
        folder = Path(tempfile.mkdtemp(prefix=f'.cmt-accessory-{target.stem}-', dir=target.parent))
        owned = OwnedFiles(folder)
        result = None
        atomic_json(folder / 'journal.json', {'version': 1, 'owner': 'cmt-accessory-install',
            'target': target.name, 'state': 'preparing', 'records': []})
        try:
            outputs, result = prepare(payload, bridge, backend, target, folder, watched, owned)
            result['warnings'].extend(recovery['warnings'])
            result['backups'] = publish(folder, target, outputs, watched, backend, owned)
            result['outputs'] = [str(target.parent / p.name) for p in sorted(outputs, key=lambda p: p.name == target.name)]
            return result
        finally:
            journal = checked_transaction(folder, target)
            if journal['state'] in ('preparing', 'complete', 'rolled_back'):
                owned.remember(folder / 'journal.json')
                preserved = owned.cleanup()
                if preserved:
                    message = (f'Accessory transaction files were preserved at {folder}; '
                               f'unexpected or changed contents: {preserved["unexpected"] + preserved["changed"]}')
                    bridge.emit_event('warning', message)
                    if result is not None:
                        result['warnings'].append(message)
