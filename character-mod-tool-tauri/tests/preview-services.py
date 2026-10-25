import csv
import io
import json
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


def iff(label):
    stream = io.BytesIO()
    with zipfile.ZipFile(stream, 'w') as archive:
        archive.writestr('fixture.scne', label)
    return stream.getvalue()


class PreviewServices(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory(prefix='cmt-preview-services-')
        self.root = Path(self.folder.name)
        self.game = self.root / 'game'
        self.game.mkdir()
        self.target = self.root / 'png0001.iff'
        with zipfile.ZipFile(self.target, 'w') as archive:
            archive.writestr('player.scne', '{}')
            archive.writestr('appearance_info.json', json.dumps({'appearance': {'gender': 'male', 'skin color': 0},
                'accessory_items': {'default_config': 'AFRO', 'configurations': [{'name': 'AFRO', 'items': ['hair_afro', 'headband']}]}}))
        bridge = types.SimpleNamespace(command_run_swap=lambda _: {}, command_health=lambda _: {}, command_health_repair=lambda _: {},
            fast_settings=lambda: {}, game_root_for=lambda *_: str(self.game))
        self.environment = patch.dict(os.environ, CMT_SESSION=str(self.root))
        self.environment.start()
        self.modules = patch.dict(sys.modules, character_manifest=types.SimpleNamespace(extract_native_manifest_iff=lambda *_: self.fail('Stored ZIP must not use VCZ decoding')))
        self.modules.start()
        self.commands = prototype_services.commands(bridge, self.root, self.root)

    def tearDown(self):
        self.modules.stop()
        self.environment.stop()
        self.folder.cleanup()

    def manifest(self, resources):
        rows = []
        with (self.game / '0A').open('wb') as archive:
            for name, label in resources.items():
                data = iff(label)
                rows.append([name, '0A', archive.tell(), len(data)])
                archive.write(data)
        with (self.game / 'manifest').open('w', newline='', encoding='utf-8') as file:
            csv.writer(file).writerows(rows)

    def test_all_active_companions_and_config_are_resolved(self):
        names = {'char/sig/png0001_config_afro.iff': 'face', 'char/sig/png0001_item_hair_afro.iff': 'hair texture',
            'char/sig/png0001_geo_hair_afro.iff': 'hair', 'char/sig/png0001_geo_hair_afro_tangentspace.iff': 'tangent hair',
            'char/sig/png0001_geo_headband.iff': 'headband', 'char/chest_color_00.iff': 'chest',
            'char/torso_color_00.iff': 'torso', 'char/legs_color_00.iff': 'legs'}
        self.manifest(names)
        result = self.commands['preview-resources']({'target': str(self.target)})
        companions = result['options']['companionPaths']
        self.assertTrue(set(names).issubset({row['name'] for row in companions}))
        self.assertEqual(result['player']['skinColor'], 0)
        self.assertEqual(next(row['kind'] for row in companions if row['name'].endswith('_config_afro.iff')), 'config')

    def test_staged_body_files_take_precedence(self):
        names = ['chest_color_00.iff', 'torso_color_00.iff', 'legs_color_00.iff', 'male_skin.iff']
        self.manifest({'char/' + name: 'manifest' for name in names})
        for name in names:
            (self.root / name).write_bytes(iff('staged'))
        result = self.commands['preview-resources']({'target': str(self.target)})
        companions = result['options']['companionPaths']
        for name in names:
            rows = [row for row in companions if Path(row['name']).name == name]
            self.assertEqual(len(rows), 1)
            self.assertEqual(Path(rows[0]['path']), self.root / name)

    def test_manifest_is_optional_for_a_local_staged_preview(self):
        result = self.commands['preview-resources']({'target': str(self.target)})
        self.assertEqual(result['filePath'], str(self.target))
        self.assertTrue(any('texture lookup' in row for row in result['warnings']))

    def test_single_mode_excludes_opposite_local_and_manifest_geometry(self):
        names = ['png0001_geo_hair_afro.iff', 'png0001_geo_hair_afro_tangentspace.iff']
        self.manifest({'char/sig/' + name: 'manifest geometry' for name in names})
        for name in names:
            (self.root / name).write_bytes(iff('local geometry'))
        for mode, expected in [(' NO ', names[0]), ('YES', names[1]), ('both', None)]:
            with self.subTest(mode=mode):
                with zipfile.ZipFile(self.target) as archive:
                    members = {name: archive.read(name) for name in archive.namelist()}
                appearance = json.loads(members['appearance_info.json'])
                appearance['accessory_items']['items'] = [{'name': 'Hair_AFRO', 'tangentspace': mode}]
                members['appearance_info.json'] = json.dumps(appearance).encode()
                with zipfile.ZipFile(self.target, 'w') as archive:
                    for name, data in members.items():
                        archive.writestr(name, data)
                result = self.commands['preview-resources']({'target': str(self.target)})
                resolved = [Path(row['name']).name for row in result['options']['companionPaths'] if row['kind'] == 'geometry']
                self.assertEqual(sorted(resolved), sorted(names if expected is None else [expected]))

    def test_changing_manifest_does_not_reuse_stale_resources(self):
        name = 'char/sig/png0001_config_afro.iff'
        self.manifest({name: 'old'})
        first = self.commands['preview-resources']({'target': str(self.target)})
        self.manifest({name: 'new content with a different length'})
        second = self.commands['preview-resources']({'target': str(self.target)})
        find = lambda value: next(row['path'] for row in value['options']['companionPaths'] if row['name'] == name)
        self.assertNotEqual(find(first), find(second))
        with zipfile.ZipFile(find(second)) as archive:
            self.assertEqual(archive.read('fixture.scne'), b'new content with a different length')


if __name__ == '__main__':
    unittest.main()
