"""Emit the exact proposed integration patch without editing another worker's files."""
import difflib
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def wire_services(source):
    start = source.index('    def blender_open(payload):')
    end = source.index('    def blender_apply_return(payload):', start)
    block = source[start:end]
    if 'from prototype_blender_snapshot import launch_inputs, copy_input' in block:
        return source
    replacements = [
        ('    def blender_open(payload):\n',
         '    def blender_open(payload):\n'
         '        from prototype_blender_snapshot import launch_inputs, copy_input\n'
         '        snapshot = launch_inputs(payload, session) if payload.get("_blenderSnapshot") else None\n'),
        ('target = Path(payload["target"]).resolve()',
         'target = Path(snapshot["target"] if snapshot else payload["target"]).resolve()'),
        ('valid_player(target)', 'valid_player(Path(snapshot["source"]) if snapshot else target)'),
        ('settings = bridge.fast_settings()', 'settings = snapshot["settings"] if snapshot else bridge.fast_settings()'),
        ('            shutil.copyfile(target, source)',
         '            if snapshot:\n'
         '                copy_input(snapshot, snapshot["source"], source)\n'
         '            else:\n'
         '                shutil.copyfile(target, source)'),
        ('expected_digest = file_digest(source)',
         'expected_digest = snapshot["expectedDigest"] if snapshot else file_digest(source)'),
        ('resources = preview_resources(payload)',
         'resources = snapshot["resources"] if snapshot else preview_resources(payload)'),
        ('                shutil.copyfile(jersey, reference)',
         '                if snapshot:\n'
         '                    copy_input(snapshot, jersey, reference)\n'
         '                else:\n'
         '                    shutil.copyfile(jersey, reference)'),
    ]
    for old, new in replacements:
        if block.count(old) != 1:
            raise AssertionError('Service wiring context changed: ' + old)
        block = block.replace(old, new, 1)
    return source[:start] + block + source[end:]


def wire_bridge(source):
    if 'import prototype_blender_snapshot' not in source:
        source = source.replace('import prototype_services\n',
                                'import prototype_services\nimport prototype_blender_snapshot\n', 1)
    call = 'python_bridge.COMMANDS.update(prototype_blender_snapshot.commands(python_bridge, root, data))'
    if call not in source:
        old = 'python_bridge.COMMANDS.update(prototype_services.commands(python_bridge, root, data))'
        if source.count(old) != 1:
            raise AssertionError('Bridge registration context changed')
        source = source.replace(old, old + '\n' + call, 1)
    return source


if __name__ == '__main__':
    for name, transform in [('prototype_services.py', wire_services), ('prototype_bridge.py', wire_bridge)]:
        relative = 'engine/' + name
        before = (ROOT / relative).read_text(encoding='utf-8')
        sys.stdout.writelines(difflib.unified_diff(before.splitlines(True), transform(before).splitlines(True),
                             fromfile='a/' + relative, tofile='b/' + relative))
