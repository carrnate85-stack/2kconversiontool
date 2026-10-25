import json
import os
import sys
import tempfile
import types
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch

project = Path(__file__).resolve().parents[1]
production = project.parent / 'character-mod-tool-webview2/dist/Character-Mod-Tool-WebView2-0.1.31/App/Backend'
sys.path.insert(0, str(production))
sys.path.insert(0, str(project / 'engine'))
import prototype_services


class FitInputTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='CmtHeadbandInput-')
        self.folder = Path(self.temporary.name)
        self.source = self.folder / 'source.iff'
        self.output = self.folder / 'prepared.iff'
        self.name = 'VertexBuffer.0123456789abcdef.bin'
        self.environment = patch.dict(os.environ, CMT_RUST_EXE='native.exe', CMT_SESSION=str(self.folder))
        self.environment.start()
    def tearDown(self):
        self.environment.stop()
        self.temporary.cleanup()
    def archive(self, data):
        with zipfile.ZipFile(self.source, 'w') as archive:
            archive.writestr('head.SCNE', json.dumps({'head': {'Model': {'hihead': {'VertexStream': [{'Binary': self.name.replace('.bin', '.gz'), 'Size': 24, 'Stride': 8}]}}}}))
            archive.writestr(self.name, data)
            archive.writestr('appearance_info.json', b'{"stock":true}')
            archive.writestr('shader.bin', b'unchanged shader')
    def test_raw_copy_needs_neither_game_nor_decoder(self):
        self.archive(bytes(24))
        before = self.source.read_bytes()
        with patch.object(prototype_services.subprocess, 'run') as run:
            self.assertEqual(prototype_services.fit_input_copy(self.source, self.output, lambda: self.fail('Raw copies need no game')), 0)
        run.assert_not_called()
        self.assertEqual(self.source.read_bytes(), before)
        with zipfile.ZipFile(self.source) as a, zipfile.ZipFile(self.output) as b:
            for name in a.namelist(): self.assertEqual(a.read(name), b.read(name))
    def test_vcz_expands_only_the_referenced_private_buffer(self):
        self.archive(bytes(12) + b'VCZ\0packed')
        before = self.source.read_bytes()
        result = types.SimpleNamespace(returncode=0, stdout=bytes(range(24)), stderr=b'')
        with patch.object(prototype_services.subprocess, 'run', return_value=result) as run:
            self.assertEqual(prototype_services.fit_input_copy(self.source, self.output, 'game'), 1)
        self.assertEqual(run.call_args.args[0], ['native.exe', '--decode-geometry', 'game', '24'])
        self.assertEqual(self.source.read_bytes(), before)
        with zipfile.ZipFile(self.output) as archive:
            self.assertEqual(archive.read(self.name), bytes(range(24)))
            self.assertEqual(archive.read('shader.bin'), b'unchanged shader')
            self.assertEqual(archive.read('appearance_info.json'), b'{"stock":true}')
    def test_failed_decode_removes_partial_copy(self):
        self.archive(bytes(12) + b'VCZ\0packed')
        before = self.source.read_bytes()
        with patch.object(prototype_services.subprocess, 'run', return_value=types.SimpleNamespace(returncode=1, stdout=b'', stderr=b'bad buffer')):
            with self.assertRaisesRegex(ValueError, 'bad buffer'):
                prototype_services.fit_input_copy(self.source, self.output, 'game')
        self.assertFalse(self.output.exists()); self.assertEqual(self.source.read_bytes(), before)
    def test_existing_output_is_not_overwritten_or_deleted(self):
        self.archive(bytes(24)); self.output.write_bytes(b'existing')
        with self.assertRaises(FileExistsError): prototype_services.fit_input_copy(self.source, self.output, 'game')
        self.assertEqual(self.output.read_bytes(), b'existing')
    def test_wrong_decoded_size_is_rejected(self):
        self.archive(bytes(12) + b'VCZ\0packed')
        with patch.object(prototype_services.subprocess, 'run', return_value=types.SimpleNamespace(returncode=0, stdout=b'short', stderr=b'')):
            with self.assertRaisesRegex(ValueError, 'incorrect buffer size'): prototype_services.fit_input_copy(self.source, self.output, 'game')
        self.assertFalse(self.output.exists())
    def test_wrapper_restores_original_fitter_and_cleans_on_failure(self):
        self.archive(bytes(24))
        class Tool:
            def fit_built_in_headband_to_character(app, *args):
                self.assertNotEqual(Path(args[2]), self.source)
                self.assertTrue(Path(args[2]).is_file())
                raise RuntimeError('fit failed')
        original = Tool.fit_built_in_headband_to_character
        bridge = types.SimpleNamespace(command_run_swap=lambda _: {}, command_health=lambda _: {}, command_health_repair=lambda _: {},
            command_headband_apply=lambda _: Tool().fit_built_in_headband_to_character('stock', 'donor', self.source, self.output, self.folder, 'headband'))
        commands = prototype_services.commands(bridge, self.folder, self.folder)
        with patch.dict(sys.modules, character_mod_tool=types.SimpleNamespace(CharacterModTool=Tool)):
            with self.assertRaisesRegex(RuntimeError, 'fit failed'): commands['headband-apply']({})
        self.assertIs(Tool.fit_built_in_headband_to_character, original)
        self.assertFalse(list(self.folder.glob('.headband-fit-*')))


if __name__ == '__main__': unittest.main(verbosity=2)
