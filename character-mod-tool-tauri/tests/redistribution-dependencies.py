"""Audit the shipped payload without launching the app, Blender, or game tools.

Default exit status covers inventory and isolated probes. --require-self-contained
also fails on proven external framework/runtime prerequisites. This is not a VM.
"""
import argparse
import ast
import base64
import csv
import hashlib
import io
import json
import os
import re
import struct
import subprocess
import sys
import tempfile
import zipfile
from collections import defaultdict
from email.parser import Parser
from pathlib import Path, PurePosixPath

exec(compile(Path(__file__).with_name('redistribution-dependencies-pe.inc').read_text(),
             'redistribution-dependencies-pe.inc', 'exec'))

SYSTEM_DLLS = set('''advapi32 bcrypt bcryptprimitives cabinet cfgmgr32 clbcatq combase comctl32
comdlg32 crypt32 cryptbase cryptsp d2d1 d3d11 d3d12 d3dcompiler_47 dbghelp dhcpcsvc
dnsapi dwmapi dxgi gdi32 gdi32full imm32 iphlpapi kernel32 kernelbase mpr msasn1
logoncli msimg32 msi msvcp_win msvcrt ncrypt netapi32 netutils normaliz nsi ntdll ole32 oleacc oleaut32 powrprof
profapi propsys psapi rasapi32 rpcrt4 sechost secur32 setupapi shell32 shcore shlwapi
samcli sspicli ucrtbase user32 userenv usp10 uxtheme version win32u windowscodecs winhttp
wininet winmm winspool wintrust winusb wkscli ws2_32 wtsapi32'''.split())


def system_dll(name):
    name = name.lower()
    return name.startswith(('api-ms-win-', 'ext-ms-win-')) or name.removesuffix('.dll') in SYSTEM_DLLS


def pe_record(name, data):
    pe = PE(data)
    unicode_strings = '\n'.join(value.decode('utf-16le') for value in
                                re.findall(rb'(?:[ -~]\x00){5,}', data))
    hints = set(re.findall(r'(?i)\b[\w.-]+\.dll\b', data.decode('ascii', errors='ignore')))
    hints.update(re.findall(r'(?i)\b[\w.-]+\.dll\b', unicode_strings))
    return {'path': name, 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest(),
            'machine': hex(pe.machine), 'subsystem': pe.subsystem,
            'imports': pe.imports(), 'delayImports': pe.imports(True), 'clr': pe.clr(),
            'dllStringHintsNotNecessarilyLoaded': sorted(hints, key=str.lower)}


def source_inventory(archive):
    imports, calls, dynamic, errors = [], [], [], []
    for name in archive.namelist():
        if not name.endswith('.py') or name.startswith('Backend/python_runtime/'):
            continue
        source = archive.read(name).decode('utf-8-sig')
        try:
            tree = ast.parse(source, filename=name)
        except SyntaxError as error:
            errors.append({'file': name, 'error': str(error)})
            continue
        scope = 'blender' if '/blender_tools/' in name or any(
            isinstance(node, (ast.Import, ast.ImportFrom)) and
            ((isinstance(node, ast.Import) and any(x.name in {'bpy', 'bmesh', 'mathutils'} for x in node.names))
             or (isinstance(node, ast.ImportFrom) and (node.module or '').split('.')[0] in {'bpy', 'bmesh', 'mathutils'}))
            for node in ast.walk(tree)) else 'backend'
        for node in ast.walk(tree):
            if isinstance(node, (ast.Import, ast.ImportFrom)):
                names = [item.name for item in node.names] if isinstance(node, ast.Import) else [node.module or '']
                for module in names:
                    imports.append({'file': name, 'line': node.lineno, 'module': module,
                                    'relativeLevel': getattr(node, 'level', 0), 'scope': scope})
            if not isinstance(node, ast.Call):
                continue
            function = ast.unparse(node.func)
            if (function.startswith('subprocess.') or function == 'shutil.which'
                    or function in {'os.system', 'os.popen'}):
                calls.append({'file': name, 'line': node.lineno, 'call': ast.unparse(node)[:1800]})
            if any(value in function for value in ('CDLL', 'WinDLL', 'LoadLibrary', 'import_module', 'spec_from_file_location')):
                dynamic.append({'file': name, 'line': node.lineno, 'call': ast.unparse(node)[:1800]})
    return {'imports': imports, 'processAndPathCalls': calls, 'dynamicLoads': dynamic, 'parseErrors': errors}


def package_inventory(archive):
    packages = []
    prefix = 'Backend/python_runtime/Lib/site-packages/'
    for name in archive.namelist():
        if not name.startswith(prefix) or not name.endswith('.dist-info/METADATA'):
            continue
        metadata = Parser().parsestr(archive.read(name).decode('utf-8'))
        base = name.rsplit('/', 1)[0]
        record_name = base + '/RECORD'
        records, missing, omitted_bytecode = [], [], 0
        if record_name in archive.namelist():
            for row in csv.reader(io.StringIO(archive.read(record_name).decode('utf-8'))):
                relative = PurePosixPath(row[0])
                if '..' in relative.parts:
                    continue
                path = prefix + relative.as_posix()
                if path not in archive.namelist():
                    if path.endswith(('.pyc', '.pyo')):
                        omitted_bytecode += 1
                    else:
                        missing.append(row[0])
                if path.lower().endswith(('.dll', '.pyd')):
                    verified = False
                    if path in archive.namelist() and row[1]:
                        algorithm, digest = row[1].split('=', 1)
                        actual = base64.urlsafe_b64encode(hashlib.new(algorithm, archive.read(path)).digest()).decode().rstrip('=')
                        verified = actual == digest
                    records.append({'recordPath': row[0], 'recordHash': row[1],
                                    'bundled': path in archive.namelist(), 'recordHashVerified': verified})
        packages.append({'name': metadata['Name'], 'version': metadata['Version'],
                         'requiresDist': metadata.get_all('Requires-Dist', []),
                         'nativeRecords': records, 'missingRecordPaths': missing,
                         'intentionallyOmittedBytecodeRecords': omitted_bytecode})
    return packages


# Executed only by the extracted interpreter with -I -B. No Tk root, worker main,
# game discovery, settings migration, add-on installation, or GUI is invoked.
PYTHON_PROBE = r'''
import ctypes, importlib, io, json, os, pathlib, shutil, subprocess, sys, time
root, scratch = map(pathlib.Path, sys.argv[1:3])
sys.path.insert(0, str(root))
import app_settings
app_settings._USER_DATA_DIR = scratch
app_settings.SETTINGS_PATH = scratch / 'settings.json'
app_settings.LOG_DIR = scratch / 'logs'
app_settings.DIAGNOSTICS_DIR = scratch / 'diagnostics'
app_settings.load_settings = lambda: {}
modules = [
    'ssl', 'sqlite3', 'bz2', 'lzma', 'ctypes', 'tkinter', 'zstandard', 'rarfile',
    'PIL.Image', 'PIL._imaging', 'PIL._imagingcms', 'PIL._imagingft',
    'PIL._imagingmath', 'PIL._imagingmorph', 'PIL._imagingtk', 'PIL._webp', 'PIL._avif',
    'character_manifest', 'character_package', 'character_mod_tool', 'python_bridge',
    'configs_backend', 'nba2k25_support', 'nba2k25_facial_hair',
    'prototype_services', 'prototype_accessories', 'prototype_player_writes',
    'prototype_blender_snapshot', 'hair_tools.hair_picker',
    'hair_tools.convert_2k23_hair_to_2k26_lod0', 'hair_tools.convert_2k25_hair_to_2k26_static',
    'hair_tools.legacy_facial_hair', 'hair_tools.regular_hair_tangent_fit',
]
results = []
for name in modules:
    try:
        module = importlib.import_module(name)
        origin = getattr(module, '__file__', None)
        if origin:
            assert pathlib.Path(origin).resolve().is_relative_to(root), (name, origin)
        results.append({'module': name, 'ok': True, 'origin': origin})
    except Exception as error:
        results.append({'module': name, 'ok': False, 'error': repr(error)})
checks = []
def check(name, fn):
    try:
        fn()
        checks.append({'name': name, 'ok': True})
    except Exception as error:
        checks.append({'name': name, 'ok': False, 'error': repr(error)})
def codecs():
    import zstandard, bz2, lzma, ssl, sqlite3
    from PIL import Image
    raw = b'isolated redistribution codec probe' * 64
    assert zstandard.ZstdDecompressor().decompress(zstandard.ZstdCompressor().compress(raw)) == raw
    assert bz2.decompress(bz2.compress(raw)) == lzma.decompress(lzma.compress(raw)) == raw
    for fmt in ('PNG', 'JPEG', 'WEBP', 'DDS'):
        output = io.BytesIO()
        Image.new('RGB', (8, 8), (12, 34, 56)).save(output, format=fmt)
        output.seek(0)
        Image.open(output).load()
    assert sqlite3.connect(':memory:').execute('select 1').fetchone() == (1,)
    assert ssl.OPENSSL_VERSION
check('bundled codec roundtrips and in-memory sqlite', codecs)
def bundled_tools():
    import character_package
    from character_mod_tool import CharacterModTool
    assert character_package._rar_reader().resolve() == root / 'archive_reader/7z.exe'
    assert pathlib.Path(CharacterModTool.find_texconv_executable()).resolve() == root / 'tools/texconv.exe'
    assert shutil.which('node') is None
    assert shutil.which('dotnet') is None
    assert shutil.which('texconv') is None
    assert shutil.which('7z') is None
check('package-selected 7z and texconv, no development PATH tools', bundled_tools)
# Inspect actual DLL locations loaded by the isolated Python process. Windows
# system directories are not proof of stock OS presence (e.g. VC redistributable).
kernel = ctypes.WinDLL('kernel32', use_last_error=True)
psapi = ctypes.WinDLL('psapi', use_last_error=True)
kernel.GetCurrentProcess.restype = ctypes.c_void_p
psapi.EnumProcessModules.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_void_p), ctypes.c_ulong, ctypes.POINTER(ctypes.c_ulong)]
psapi.GetModuleFileNameExW.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_wchar_p, ctypes.c_ulong]
def loaded_modules(process):
    handles = (ctypes.c_void_p * 4096)()
    needed = ctypes.c_ulong()
    if not psapi.EnumProcessModules(process, handles, ctypes.sizeof(handles), ctypes.byref(needed)):
        return []
    assert needed.value <= ctypes.sizeof(handles)
    result = []
    for handle in handles[:needed.value // ctypes.sizeof(ctypes.c_void_p)]:
        buffer = ctypes.create_unicode_buffer(32768)
        if psapi.GetModuleFileNameExW(process, handle, buffer, len(buffer)):
            result.append(buffer.value)
    return result
loaded = loaded_modules(kernel.GetCurrentProcess())
assert loaded
optional = []
for name in ('zstandard.backend_cffi', 'peutils'):
    try:
        module = importlib.import_module(name)
        optional.append({'module': name, 'ok': True, 'origin': getattr(module, '__file__', None)})
    except Exception as error:
        optional.append({'module': name, 'ok': False, 'error': repr(error)})
# A small CPU-only conversion keeps this owned CLI alive long enough to record
# real loader origins. Both DDS input/output stay inside the disposable scratch.
def texconv_conversion():
    from PIL import Image
    source = scratch / 'dependency-input.dds'
    output = scratch / 'texconv-output'
    output.mkdir()
    raw = bytes((i * 31 + i // 97) % 256 for i in range(512 * 512 * 3))
    Image.frombytes('RGB', (512, 512), raw).save(source)
    command = [str(root / 'tools/texconv.exe'), '-f', 'BC7_UNORM', '-nogpu', '-y', '-o', str(output), str(source)]
    child = subprocess.Popen(command, cwd=scratch, stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, creationflags=subprocess.CREATE_NO_WINDOW)
    modules, handle = set(), None
    try:
        kernel.OpenProcess.argtypes = [ctypes.c_ulong, ctypes.c_int, ctypes.c_ulong]
        kernel.OpenProcess.restype = ctypes.c_void_p
        kernel.CloseHandle.argtypes = [ctypes.c_void_p]
        handle = kernel.OpenProcess(0x410, False, child.pid)
        deadline = time.monotonic() + 25
        while child.poll() is None and time.monotonic() < deadline:
            if handle:
                modules.update(loaded_modules(handle))
            time.sleep(0.01)
        stdout, stderr = child.communicate(timeout=2)
        return {'command': command, 'returnCode': child.returncode,
                'stdout': stdout.decode('utf-8', errors='replace'),
                'stderr': stderr.decode('utf-8', errors='replace'),
                'loadedModules': sorted(modules), 'outputCreated': (output / source.name).is_file()}
    finally:
        if child.poll() is None:
            child.kill()
            child.communicate()
        if handle:
            kernel.CloseHandle(handle)
try:
    texconv = texconv_conversion()
except Exception as error:
    texconv = {'error': repr(error)}
print('CMT_DEPENDENCY_PROBE=' + json.dumps({'python': sys.version, 'prefix': sys.prefix,
    'sysPath': sys.path, 'isolated': sys.flags.isolated, 'dontWriteBytecode': sys.dont_write_bytecode,
    'imports': results, 'checks': checks, 'loadedModules': loaded, 'texconvConversion': texconv,
    'optionalUnusedImports': optional, 'zstandardBackend': getattr(sys.modules.get('zstandard'), 'backend', None),
    'stdlibModuleNames': sorted(sys.stdlib_module_names),
    'path': os.environ['PATH']}))
'''


def run_probe(command, cwd, env):
    try:
        result = subprocess.run(command, cwd=cwd, env=env, stdin=subprocess.DEVNULL,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=40,
                                creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
        return {'command': list(map(str, command)), 'returnCode': result.returncode,
                'stdout': result.stdout.decode('utf-8', errors='replace')[:200000],
                'stderr': result.stderr.decode('utf-8', errors='replace')[:12000]}
    except (OSError, subprocess.TimeoutExpired) as error:
        return {'command': list(map(str, command)), 'error': str(error)}


def isolated_probes(archive):
    assert os.name == 'nt', 'CLI probes require Windows'
    with tempfile.TemporaryDirectory(prefix='cmt-redistribution-dependencies-') as temporary:
        work = Path(temporary).resolve()
        for item in archive.infolist():
            path = PurePosixPath(item.filename)
            assert not path.is_absolute() and '..' not in path.parts and ':' not in item.filename
        archive.extractall(work)
        backend = work / 'Backend'
        scratch = work / 'scratch'
        scratch.mkdir()
        windows = Path(os.environ['SystemRoot']).resolve()
        env = {'SystemRoot': str(windows), 'WINDIR': str(windows),
               'PATH': os.pathsep.join(map(str, [windows / 'System32', windows])),
               'TEMP': str(scratch), 'TMP': str(scratch), 'HOME': str(scratch),
               'USERPROFILE': str(scratch), 'LOCALAPPDATA': str(scratch), 'APPDATA': str(scratch),
               'PYTHONDONTWRITEBYTECODE': '1', 'PYTHONNOUSERSITE': '1',
               'DOTNET_CLI_TELEMETRY_OPTOUT': '1', 'DOTNET_SKIP_FIRST_TIME_EXPERIENCE': '1'}
        py = run_probe([str(backend / 'python_runtime/python.exe'), '-I', '-B', '-c', PYTHON_PROBE,
                        str(backend), str(scratch)], scratch, env)
        marker = 'CMT_DEPENDENCY_PROBE='
        decoded = None
        for line in py.get('stdout', '').splitlines():
            if line.startswith(marker):
                decoded = json.loads(line[len(marker):])
        if decoded:
            for label, paths in [('loadedModuleOrigins', decoded['loadedModules']),
                                 ('texconvModuleOrigins', decoded['texconvConversion'].get('loadedModules', []))]:
                for path in paths:
                    file = Path(path).resolve()
                    name = file.name.lower()
                    scope = ('bundled' if file.is_relative_to(backend) else
                             'windows-system' if file.is_relative_to(windows) else 'external')
                    if scope == 'windows-system' and not system_dll(name) and file.suffix.lower() == '.dll':
                        scope = 'system-installed-runtime-or-unclassified'
                    decoded.setdefault(label, []).append({'path': path, 'scope': scope})
            py['result'] = decoded
            py['stdout'] = '[structured probe result recorded separately]'
        helpers = []
        for name in archive.namelist():
            if not name.endswith('.exe') or '/python_runtime/' in name:
                continue
            if 'Decoder.exe' in name:
                arguments, token = [], 'Usage: Nba2kVczDecoder'
            elif name.endswith('/7z.exe'):
                arguments, token = ['i'], '7-Zip'
            elif name.endswith('/texconv.exe'):
                arguments, token = ['--help'], 'Microsoft (R) DirectX Texture Converter'
            elif name.endswith('/roster.exe'):
                arguments, token = ['--help'], 'Usage:'
            else:
                raise AssertionError(f'Unreviewed helper, do not execute automatically: {name}')
            row = run_probe([str(work / name), *arguments], scratch, env)
            row['path'] = name
            row['expectedOutput'] = token
            # Usage can intentionally return a nonzero code. The expected usage
            # text proves managed Main was reached; exit zero alone would not.
            row['ok'] = 'error' not in row and token.casefold() in (
                row.get('stdout', '') + row.get('stderr', '')).casefold()
            helpers.append(row)
        return {'environment': env, 'python': py, 'helpers': helpers,
                'temporaryPayloadRemovedOnExit': True,
                'originalReferenceExecuted': False, 'mainApplicationExecuted': False,
                'blenderExecuted': False, 'gameExecutablesExecuted': False}


def audit(args):
    root = Path(__file__).resolve().parents[1]
    metadata = json.loads((root / 'dist/BUILD.json').read_text(encoding='utf-8-sig'))
    exe = args.exe or root / f"dist/Character-Mod-Tool-Tauri-Preview-{metadata['version']}.exe"
    binary = exe.read_bytes()
    assert binary[-64:-48] == b'CMT-RUST-PAYLOAD', 'Missing release footer'
    offset, length = struct.unpack('<QQ', binary[-48:-32])
    assert offset > 0 and offset + length == len(binary) - 64, 'Invalid payload bounds'
    payload = binary[offset:offset + length]
    assert hashlib.sha256(payload).digest() == binary[-32:], 'Payload checksum mismatch'
    if not args.exe:
        assert metadata['payloadSha256'] == hashlib.sha256(payload).hexdigest()
        assert metadata['bytes'] == len(binary)
    with zipfile.ZipFile(io.BytesIO(payload)) as archive:
        names = archive.namelist()
        assert len(names) == len(set(names)), 'Duplicate payload members'
        if not args.exe:
            assert len(names) == metadata['files'], 'Payload inventory mismatch'
        pes = [pe_record('<release-host>', binary[:offset])]
        pes.extend(pe_record(name, archive.read(name)) for name in names
                   if name.lower().endswith(('.exe', '.dll', '.pyd')))
        by_name = defaultdict(list)
        for name in names:
            by_name[PurePosixPath(name).name.lower()].append(name)
        references = []
        for row in pes:
            for kind in ('imports', 'delayImports'):
                for dll in row[kind]:
                    lower = dll.lower()
                    candidates = by_name[lower]
                    same_directory = [name for name in candidates if PurePosixPath(name).parent == PurePosixPath(row['path']).parent]
                    python_consumer = row['path'].startswith('Backend/python_runtime/')
                    reachable = [name for name in candidates if name.startswith('Backend/python_runtime/')]
                    status = ('external-dotnet-framework' if lower == 'mscoree.dll' else
                              'bundled-same-directory' if same_directory else
                              'windows-api' if system_dll(lower) else
                              'bundled-python-loader-path' if python_consumer and reachable else
                              'bundled-outside-consumer-loader-path' if candidates else
                              'external-or-unresolved')
                    references.append({'image': row['path'], 'kind': kind, 'dll': dll,
                                       'classification': status, 'bundledCandidates': candidates})
        source = source_inventory(archive)
        packages = package_inventory(archive)
        reference_root = root.parent / 'character-mod-tool-webview2/dist/Character-Mod-Tool-WebView2-0.1.31/App/Backend'
        comparisons = []
        for row in pes:
            if not row['path'].startswith('Backend/'):
                continue
            original = reference_root / row['path'][len('Backend/'):]
            if row['path'] == 'Backend/electron/helpers/Nba2kVczDecoder.exe':
                original = reference_root.parent / 'Reference/src/helpers/Nba2kVczDecoder.exe'
            if row['path'] == 'Backend/tools/live_roster/roster.exe':
                original = reference_root.parent / 'tools/roster.exe'
            comparisons.append({'path': row['path'], 'referencePath': str(original), 'referenceExists': original.is_file(),
                                'identicalToReference': original.is_file() and
                                hashlib.sha256(original.read_bytes()).hexdigest() == row['sha256']})
        report = {'version': metadata['version'], 'exe': str(exe), 'exeSha256': hashlib.sha256(binary).hexdigest(),
                  'payloadSha256': hashlib.sha256(payload).hexdigest(), 'payloadChecksumVerified': True,
                  'payloadFiles': len(names), 'peImages': pes, 'dllReferences': references,
                  'referenceBinaryComparisons': comparisons, 'pythonPackages': packages,
                  'topLevelSitePackages': sorted({name[len('Backend/python_runtime/Lib/site-packages/'):].split('/')[0]
                      for name in names if name.startswith('Backend/python_runtime/Lib/site-packages/')}),
                  'sourceInventory': source, 'runtimeConfigurationFiles': [name for name in names if
                      name.endswith(('.runtimeconfig.json', '.deps.json', '.exe.config', '.pth', '._pth'))],
                  'managedHelpers': [row['path'] for row in pes if row['clr'] and row['path'].endswith('.exe')],
                  'bundledClrRuntimeFiles': [name for name in names if PurePosixPath(name).name.lower() in
                      {'clr.dll', 'coreclr.dll', 'mscoree.dll', 'hostfxr.dll', 'mscorlib.dll'}],
                  'limits': [
                      'Not a clean-PC/VM test. System-only PATH does not hide installed Framework, WinSxS, drivers, or System32 runtimes.',
                      'Usage probes prove loader/managed Main startup, not real decoder/Oodle/game/Blender operations.',
                      'PE imports, delay imports, CLR assembly references and source calls are inventoried; arbitrary computed dynamic loads cannot be exhaustively proven statically.',
                      'CLR metadata v4.0.30319 and assembly version 4.0.0.0 do not establish an exact minimum .NET Framework product version.',
                      'Blender Python modules/add-on dependencies are inventoried only; no Blender or add-on installer was run.',
                  ]}
        if not args.static_only:
            report['isolatedProbes'] = isolated_probes(archive)
    gaps = []
    if report['managedHelpers'] and not report['bundledClrRuntimeFiles']:
        gaps.append({'kind': 'external-framework-prerequisite', 'images': report['managedHelpers'],
                     'proof': 'CLR metadata and System/mscorlib assembly references; no CLR runtime bundled',
                     'safeNextAction': 'Parent should declare/check .NET Framework prerequisite or replace/repackage helpers; verify decoder flows on a clean supported Windows VM.'})
    unresolved = [row for row in references if row['classification'] in
                  {'external-or-unresolved', 'bundled-outside-consumer-loader-path'}]
    if unresolved:
        gaps.append({'kind': 'unresolved-native-imports', 'imports': unresolved,
                     'safeNextAction': 'Parent should deploy/check the matching Visual C++ redistributable or licensed app-local runtime set for texconv. Check NBAModel under the supported Blender distribution separately. Python runtime DLL copies do not establish resolution in standalone texconv or Blender.'})
    report['packagingGaps'] = gaps
    report['externalFeaturePrerequisites'] = [
        {'feature': 'Native game decoding', 'requires': ['selected game archives', 'game data/oodle/oo2core_9_win64.dll',
                                                      'game Oodle texture runtime oo2texrt_9_win64.dll for BC7Prep paths'],
         'note': 'Decoder strings and packaged source use game-owned DLL paths; add-on Link copies do not prove the selected-game dependency is satisfied.'},
        {'feature': 'Blender workflows', 'requires': ['user-configured Blender and its Python/bpy/mathutils/numpy runtime'],
         'note': 'Not executed by this sidecar.'},
        {'feature': 'Tauri UI', 'requires': ['Windows WebView2 runtime'],
         'note': 'Main app startup and native UI verification belong to parent; this audit does not launch it.'},
    ]
    failures = []
    if source['parseErrors']:
        failures.append('Packaged source parse errors')
    for package in packages:
        if package['missingRecordPaths']:
            failures.append('Missing package RECORD files: ' + package['name'])
        if any(not row['recordHashVerified'] for row in package['nativeRecords']):
            failures.append('Native package RECORD hash mismatch: ' + package['name'])
    if 'isolatedProbes' in report:
        probes = report['isolatedProbes']
        py = probes['python']
        result = py.get('result', {})
        if py.get('returnCode') != 0 or not result:
            failures.append('Isolated bundled Python probe failed')
        failures.extend('Import failed: ' + row['module'] for row in result.get('imports', []) if not row['ok'])
        failures.extend('Check failed: ' + row['name'] for row in result.get('checks', []) if not row['ok'])
        failures.extend('Helper probe failed: ' + row['path'] for row in probes['helpers'] if not row['ok'])
        failures.extend('External loaded module: ' + row['path'] for row in result.get('loadedModuleOrigins', [])
                        if row['scope'] == 'external')
        conversion = result.get('texconvConversion', {})
        if conversion.get('returnCode') != 0 or not conversion.get('outputCreated'):
            failures.append('Isolated texconv DDS-to-BC7 conversion failed')
        if not conversion.get('loadedModules'):
            failures.append('texconv loader origins were not captured')
        runtime_origins = [row for row in result.get('texconvModuleOrigins', [])
                           if Path(row['path']).name.lower() in
                           {'msvcp140.dll', 'vcomp140.dll', 'vcruntime140.dll', 'vcruntime140_1.dll'}]
        report['texconvRuntimeDependencyProof'] = runtime_origins
        report['pythonRuntimeDependencyProof'] = {
            'requiredImportProbes': len(result.get('imports', [])),
            'loadedOriginCounts': {scope: sum(row['scope'] == scope for row in result.get('loadedModuleOrigins', []))
                                  for scope in sorted({row['scope'] for row in result.get('loadedModuleOrigins', [])})},
            'zstandardBackend': result.get('zstandardBackend'),
            'optionalUnusedImports': result.get('optionalUnusedImports'),
            'note': 'The optional CFFI backend and unreferenced peutils are not backend runtime requirements; their absence must not be mistaken for a required codec failure.'}
        stdlib = set(result.get('stdlibModuleNames', []))
        site_roots = {Path(name).stem if name.endswith('.py') else name for name in report['topLevelSitePackages']}
        local = defaultdict(set)
        with zipfile.ZipFile(io.BytesIO(payload)) as inventory:
            for name in inventory.namelist():
                if name.startswith('Backend/python_runtime/') or not name.endswith('.py'):
                    continue
                local[PurePosixPath(name).stem].add(name)
                for directory in PurePosixPath(name).parts[1:-1]:
                    local[directory].add(name)
        dependencies = defaultdict(list)
        for row in source['imports']:
            dependencies[row['module'].split('.')[0]].append(row)
        report['pythonImportDependencies'] = []
        for module, sites in sorted(dependencies.items()):
            category = ('blender-provided' if module in {'bpy', 'bmesh', 'bpy_extras', 'mathutils', 'numpy'} else
                        'platform-conditional-posix' if module == 'fcntl' else
                        'bundled-python-stdlib' if module in stdlib else
                        'bundled-site-package' if module in site_roots else
                        'bundled-source' if module in local else
                        'relative-import' if not module else 'unclassified')
            report['pythonImportDependencies'].append({'module': module, 'classification': category,
                                                      'sites': len(sites), 'scopes': sorted({row['scope'] for row in sites})})
            if category == 'unclassified':
                failures.append('Unclassified packaged Python import: ' + module)
        report['helperConclusions'] = [
            {'helper': 'Nba2kVcz*Decoder.exe (three copies)',
             'conclusion': 'External .NET Framework CLR 4 family prerequisite; no exact minimum product version declared in helper metadata. Game Oodle DLLs are separate feature inputs.'},
            {'helper': 'texconv.exe',
             'conclusion': 'Depends on external VC++/OpenMP runtimes. Disposable BC7 conversion and captured System32 origins prove this PC supplied MSVCP140/VCOMP140/VCRUNTIME140/VCRUNTIME140_1.'},
            {'helper': 'archive_reader/7z.exe + 7z.dll',
             'conclusion': 'Bundled 7z.dll selected by CLI; PE imports disclose only Windows API dependencies. Clean-PC verification still pending.'},
            {'helper': 'tools/live_roster/roster.exe',
             'conclusion': 'Native image with Windows API imports only; --help works on system-only PATH. No game process was contacted.'},
            {'helper': 'python_runtime/python.exe + packages',
             'conclusion': 'Required backend imports and native codec roundtrips pass using bundled extension/runtime DLLs; no ambient Python packages or PATH tools used. Optional unused import failures are recorded separately.'},
            {'helper': 'Blender Link/NBAModel.dll + oo2core DLLs',
             'conclusion': 'VC runtime imports exist outside the bundled Python loader context; supported Blender distribution may supply them. Not loaded or tested here.'},
        ]
    report['verificationFailures'] = failures
    report['summary'] = {'peImages': len(pes), 'dllReferences': len(references),
                         'sourceImportSites': len(source['imports']),
                         'packageNativeRecordsVerified': sum(len(p['nativeRecords']) for p in packages),
                         'referenceBinariesIdentical': sum(row['identicalToReference'] for row in comparisons)}
    report['isolatedProbesPassed'] = not args.static_only and not failures
    report['selfContained'] = False if gaps else None
    report['cleanPcVerified'] = False
    report['auditInvocation'] = [sys.executable, *sys.argv]
    report['auditPassed'] = not failures and (not args.require_self_contained or not gaps)
    return report


def main():
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exe', type=Path)
    parser.add_argument('--report', type=Path, default=root / 'artifacts/redistribution-dependencies-report.json')
    parser.add_argument('--static-only', action='store_true')
    parser.add_argument('--require-self-contained', action='store_true')
    parser.add_argument('--self-test', action='store_true', help='Run parser fixtures without reading a release or writing a report')
    args = parser.parse_args()
    if args.self_test:
        pe_self_test()
        print('PE32/PE32+ normal/delay imports, RVA mapping and rejection fixtures passed')
        return 0
    report = audit(args)
    args.report.write_text(json.dumps(report, indent=2), encoding='utf-8')
    print(json.dumps({key: report[key] for key in ('version', 'payloadFiles', 'managedHelpers',
                     'packagingGaps', 'verificationFailures', 'isolatedProbesPassed',
                     'selfContained', 'cleanPcVerified', 'auditPassed')}, indent=2))
    return 0 if report['auditPassed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
