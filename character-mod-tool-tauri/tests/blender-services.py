import json
import hashlib
import os
import sys
import tempfile
import types
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'engine'))
import prototype_services


def player(path, label):
    with zipfile.ZipFile(path, 'w') as archive:
        archive.writestr('player.scne', label)


class BlenderServices(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory(prefix='cmt-blender-services-')
        self.root = Path(self.folder.name)
        self.environment = patch.dict(os.environ, CMT_SESSION=str(self.root))
        self.environment.start()
        self.bridge = types.SimpleNamespace(command_run_swap=lambda _: {},
            command_health=lambda _: {'groups': []}, command_health_repair=lambda _: {},
            fast_settings=lambda: {'blender_exe': 'blender.exe', 'head_swap_tool': 'addon'},
            emit_event=lambda *args, **kwargs: None)
        self.commands = prototype_services.commands(self.bridge, self.root, self.root)
        self.target = self.root / 'target.iff'
        player(self.target, 'original')

    def tearDown(self):
        self.environment.stop()
        self.folder.cleanup()

    def test_managed_return_leaves_replacement_to_host(self):
        original = self.target.read_bytes()
        events = []
        self.bridge.emit_event = lambda *args, **kwargs: events.append(kwargs)
        class Child:
            def __init__(child, args, **kwargs):
                returned = Path(args[args.index('--return-iff') + 1])
                child.returned = returned
                child.source = Path(args[args.index('--iff') + 1])
                player(returned, 'updated')
                Path(args[args.index('--signal') + 1]).write_text(json.dumps({'status': 'success', 'eyesRecalculated': True}))
            def wait(child):
                self.assertTrue(child.returned.is_file(), 'Active Blender return must remain available')
                self.assertTrue(child.source.is_file(), 'Active Blender input must remain available')
                self.assertEqual(self.target.read_bytes(), original)
                ready = next(row for row in events if row['state'] == 'return-ready')
                self.assertTrue(ready['eyesRecalculated'])
                self.commands['blender-apply-return'](ready)
            def poll(child):
                return None
        with patch.object(prototype_services.subprocess, 'Popen', Child):
            self.commands['blender-open']({'target': str(self.target), 'managedReturn': True})
        with zipfile.ZipFile(self.target) as archive:
            self.assertEqual(archive.read('player.scne'), b'updated')
        self.assertFalse(any(row['state'] == 'complete' for row in events))
        self.assertFalse(list(self.root.glob('blender-*')))

    def test_return_paths_cannot_escape_session(self):
        with self.assertRaisesRegex(ValueError, 'app session'):
            self.commands['blender-apply-return']({'target': str(self.target),
                'returned': str(self.root.parent / 'outside.iff'), 'backup': str(self.root / 'backup.iff')})

    def mock_handoff(self, on_wait, status='success'):
        events = []
        self.bridge.emit_event = lambda *args, **kwargs: events.append(kwargs)
        class Child:
            def __init__(child, args, **kwargs):
                child.returned = Path(args[args.index('--return-iff') + 1])
                child.source = Path(args[args.index('--iff') + 1])
                player(child.returned, 'returned edits')
                Path(args[args.index('--signal') + 1]).write_text(json.dumps({'status': status, 'message': 'export failed'}))
            def wait(child):
                self.assertTrue(child.returned.is_file())
                self.assertTrue(child.source.is_file())
                on_wait(child.returned, events)
            def poll(child):
                return None
        return patch.object(prototype_services.subprocess, 'Popen', Child)

    def test_unapplied_return_is_kept_after_blender_closes(self):
        def conflict(returned, events):
            player(self.target, 'newer app edits')
            ready = next(row for row in events if row['state'] == 'return-ready')
            with self.assertRaisesRegex(ValueError, 'changed while Blender'):
                self.commands['blender-apply-return'](ready)
        with self.mock_handoff(conflict):
            self.commands['blender-open']({'target': str(self.target), 'managedReturn': True})
        returned = next(self.root.glob('blender-*/target.iff'))
        with zipfile.ZipFile(returned) as archive:
            self.assertEqual(archive.read('player.scne'), b'returned edits')
        with zipfile.ZipFile(self.target) as archive:
            self.assertEqual(archive.read('player.scne'), b'newer app edits')

    def test_confirmed_recovery_allows_temporary_session_cleanup(self):
        recovery = self.root / 'Blender Recovery' / 'recovered.iff'
        recovery.parent.mkdir()
        def preserve(returned, events):
            recovery.write_bytes(returned.read_bytes())
            digest = hashlib.sha256(returned.read_bytes()).hexdigest()
            (returned.parent / '.cmt-return.json').write_text(json.dumps({'recoveryDigest': digest, 'recoveryPath': str(recovery)}))
        with self.mock_handoff(preserve):
            self.commands['blender-open']({'target': str(self.target), 'managedReturn': True})
        self.assertFalse(list(self.root.glob('blender-*')))
        with zipfile.ZipFile(recovery) as archive:
            self.assertEqual(archive.read('player.scne'), b'returned edits')

    def test_changed_recovery_does_not_allow_cleanup(self):
        recovery = self.root / 'Blender Recovery' / 'recovered.iff'
        recovery.parent.mkdir()
        def invalid_confirmation(returned, events):
            digest = hashlib.sha256(returned.read_bytes()).hexdigest()
            recovery.write_bytes(b'changed by user')
            (returned.parent / '.cmt-return.json').write_text(json.dumps({'recoveryDigest': digest, 'recoveryPath': str(recovery)}))
        with self.mock_handoff(invalid_confirmation):
            self.commands['blender-open']({'target': str(self.target), 'managedReturn': True})
        self.assertTrue(list(self.root.glob('blender-*/target.iff')))
        self.assertEqual(recovery.read_bytes(), b'changed by user')

    def test_export_error_keeps_input_alive_until_blender_closes(self):
        waited = []
        def closing(returned, events):
            self.assertTrue(any(row['state'] == 'error' for row in events))
            waited.append(True)
        with self.mock_handoff(closing, 'error'):
            with self.assertRaisesRegex(ValueError, 'export failed'):
                self.commands['blender-open']({'target': str(self.target), 'managedReturn': True})
        self.assertEqual(waited, [True])
        self.assertTrue(list(self.root.glob('blender-*/target.iff')))

    def test_applied_marker_matches_only_the_return_that_was_applied(self):
        folder = self.root / 'blender-test'
        folder.mkdir()
        returned = folder / 'returned.iff'
        player(returned, 'updated')
        self.commands['blender-apply-return']({'target': str(self.target), 'returned': str(returned), 'backup': str(folder / 'backup.iff')})
        marker = json.loads((folder / '.cmt-return.json').read_text())
        self.assertEqual(marker['appliedDigest'], hashlib.sha256(returned.read_bytes()).hexdigest())
        player(returned, 'later changes')
        self.assertNotEqual(marker['appliedDigest'], hashlib.sha256(returned.read_bytes()).hexdigest())

    def test_failed_launch_removes_prepared_assets(self):
        original = self.target.read_bytes()
        with patch.object(prototype_services.subprocess, 'Popen', side_effect=OSError('Blender executable is unavailable')):
            with self.assertRaisesRegex(OSError, 'unavailable'):
                self.commands['blender-open']({'target': str(self.target)})
        self.assertFalse(list(self.root.glob('blender-*')))
        self.assertEqual(self.target.read_bytes(), original)

    def test_invalid_texture_removes_prepared_assets_without_launching(self):
        with patch.object(prototype_services.subprocess, 'Popen') as launch:
            with self.assertRaisesRegex(ValueError, 'preview texture'):
                self.commands['blender-open']({'target': str(self.target), 'referenceBundle': {'textures': [{'bytes': 'bm90IGEgRERT'}]}})
            launch.assert_not_called()
        self.assertFalse(list(self.root.glob('blender-*')))

    def test_stale_return_does_not_replace_newer_edits(self):
        previous = hashlib.sha256(self.target.read_bytes()).hexdigest()
        player(self.target, 'newer app edits')
        current = self.target.read_bytes()
        returned = self.root / 'returned.iff'
        player(returned, 'old blender edits')
        with self.assertRaisesRegex(ValueError, 'changed while Blender'):
            self.commands['blender-apply-return']({'target': str(self.target),
                'returned': str(returned), 'backup': str(self.root / 'backup.iff'),
                'expectedDigest': previous})
        self.assertEqual(self.target.read_bytes(), current)

    def test_invalid_return_does_not_replace_target(self):
        original = self.target.read_bytes()
        returned = self.root / 'invalid.iff'
        returned.write_bytes(b'not an IFF')
        with self.assertRaises(zipfile.BadZipFile):
            self.commands['blender-apply-return']({'target': str(self.target), 'returned': str(returned), 'backup': str(self.root / 'backup.iff')})
        self.assertEqual(self.target.read_bytes(), original)

    def test_validation_failure_restores_backup(self):
        original = self.target.read_bytes()
        returned = self.root / 'returned.iff'
        player(returned, 'updated')
        valid = prototype_services.valid_player
        def validate(path):
            if Path(path) == self.target:
                raise ValueError('simulated post-copy failure')
            valid(path)
        with patch.object(prototype_services, 'valid_player', validate):
            with self.assertRaisesRegex(ValueError, 'post-copy'):
                self.commands['blender-apply-return']({'target': str(self.target), 'returned': str(returned), 'backup': str(self.root / 'backup.iff')})
        self.assertEqual(self.target.read_bytes(), original)


if __name__ == '__main__':
    unittest.main()
