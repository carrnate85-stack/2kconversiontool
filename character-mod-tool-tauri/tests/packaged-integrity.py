import argparse
import hashlib
import fnmatch
import io
import json
import struct
import zipfile
from pathlib import Path


root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument('--metadata', type=Path, default=root / 'dist/BUILD.json')
parser.add_argument('--exe', type=Path)
args = parser.parse_args()
metadata = json.loads(args.metadata.read_text(encoding='utf-8-sig'))
exe = args.exe or root / f"dist/Character-Mod-Tool-Tauri-Preview-{metadata['version']}.exe"
binary = exe.read_bytes()
footer = binary[-64:]
assert footer[:16] == b'CMT-RUST-PAYLOAD', 'Missing engine payload footer'
offset, length = struct.unpack('<QQ', footer[16:32])
assert offset > 0 and offset + length == len(binary) - 64, 'Invalid payload bounds'
payload = binary[offset:offset + length]
digest = hashlib.sha256(payload)
assert digest.digest() == footer[32:], 'Payload checksum mismatch'
assert digest.hexdigest() == metadata['payloadSha256'], 'Build metadata checksum mismatch'
assert len(binary) == metadata['bytes'], 'Build size mismatch'
production = root.parent / 'character-mod-tool-webview2/dist/Character-Mod-Tool-WebView2-0.1.31/App'
with zipfile.ZipFile(io.BytesIO(payload)) as archive:
    names = archive.namelist()
    assert len(names) == len(set(names)) == metadata['files'], 'Payload inventory mismatch'
    assert not any(Path(name).name.lower() == 'node.exe' for name in names), 'Node runtime must not be packaged'
    helpers = {'Backend/' + path.name: path for path in (root / 'engine').glob('prototype_*.py') if path.is_file()}
    packaged_helpers = {name for name in names if name.startswith('Backend/prototype_') and name.endswith('.py')}
    assert packaged_helpers == set(helpers), 'Tauri engine module inventory mismatch'
    for name, path in helpers.items():
        assert archive.read(name) == path.read_bytes(), f'Tauri engine module changed or missing: {name}'
    tools = [name for name in names if name.startswith('Backend/blender_tools/')]
    expected = [path for path in (production / 'Backend/blender_tools').rglob('*') if path.is_file()
                and '__pycache__' not in path.parts and path.suffix not in {'.pyc', '.pyo'}]
    assert len(tools) == len(expected) == metadata['blenderFiles'], 'Blender tool inventory mismatch'
    for path in expected:
        name = path.relative_to(production).as_posix()
        assert name in tools and archive.read(name) == path.read_bytes(), f'Blender tool changed: {name}'
    runtime_files = 0
    if metadata.get('runtimePolicyVersion'):
        policy = json.loads((root / 'build-tools/runtime-policy.json').read_text())
        assert policy['version'] == metadata['runtimePolicyVersion']
        base = production / 'Backend/python_runtime'
        expected_runtime = set()
        for path in base.rglob('*'):
            if not path.is_file() or '__pycache__' in path.parts or path.suffix in {'.pyc', '.pyo'}:
                continue
            relative = path.relative_to(base).as_posix()
            prefix = 'Lib/site-packages/'
            excluded = relative.startswith(prefix) and any(fnmatch.fnmatchcase(
                relative[len(prefix):].split('/')[0].lower(), pattern.lower()
            ) for pattern in policy['excludedSitePackages'])
            name = 'Backend/python_runtime/' + relative
            if excluded:
                assert name not in names, f'Development payload retained: {name}'
            else:
                assert name in names and archive.read(name) == path.read_bytes(), f'Runtime file changed or missing: {name}'
                expected_runtime.add(name)
        actual_runtime = {name for name in names if name.startswith('Backend/python_runtime/')}
        assert actual_runtime == expected_runtime, 'Runtime policy inventory differs from packaged files'
        runtime_files = len(expected_runtime)
report = {'version': metadata['version'], 'payloadHashVerified': True,
          'prototypeModulesVerified': len(helpers),
          'blenderFiles': len(tools), 'allBlenderFilesIdentical': True, 'nodeRuntimeBundled': False,
          'retainedRuntimeFilesVerified': runtime_files}
(root / 'artifacts/packaged-integrity-report.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
print(json.dumps(report))
