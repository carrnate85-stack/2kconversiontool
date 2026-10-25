"""Exercise distance routing without changing source geometry or published outputs."""
import copy
import json
import os
import shutil
import struct
import sys
import tempfile
import types
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch

project = Path(__file__).resolve().parents[1]
production = project.parent / 'character-mod-tool-webview2/dist/Character-Mod-Tool-WebView2-0.1.31/App/Backend'
sys.path[:0] = [str(project / 'engine'), str(production)]
import prototype_services as services


class AccessoryDistance(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.private = tempfile.TemporaryDirectory(prefix='CmtDistanceSettings-')
        import app_settings
        cls.settings = patch.multiple(app_settings, _USER_DATA_DIR=Path(cls.private.name),
                                     SETTINGS_PATH=Path(cls.private.name) / 'settings.json',
                                     LOG_DIR=Path(cls.private.name) / 'logs')
        cls.settings.start()
        from hair_tools import hair_picker
        from hair_tools.hair_lod_policy import force_highest_detail
        cls.backend = hair_picker
        cls.force = staticmethod(force_highest_detail)

    @classmethod
    def tearDownClass(cls):
        cls.settings.stop()
        cls.private.cleanup()

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='CmtDistance-')
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.stage = self.root / '.character_mod_full_swap_test'
        self.stage.mkdir()
        self.player = self.stage / 'png0001.iff'
        self.companions = []
        self.appearance = {'accessory_items': {'default_config': 'BEARD',
            'items': [{'name': 'hair', 'type': 'hair', 'mesh': 'hairShape', 'meshlo': 'hair_loShape',
                       'tangentspace': 'no', 'texture': {'color': 'hair_color'}},
                      {'name': 'facialHair_beard', 'type': 'facialhair', 'mesh': 'beardShape',
                       'meshlo': 'beard_loShape', 'tangentspace': 'both'}],
            'configurations': [{'name': 'BEARD', 'items': ['hair', 'facialHair_beard']}]}}

    def write_player(self):
        with zipfile.ZipFile(self.player, 'w') as archive:
            archive.comment = b'player comment'
            archive.writestr('appearance_info.json', json.dumps(self.appearance))
            archive.writestr('hihead.SCNE', b'{"untouched":"head"}')
            archive.writestr('appearance_info.rdat', b'unchanged configuration paths')
            archive.writestr('physics.bin', b'untouched physics')

    def geometry(self, key='hair', mesh='hairShape', tangent=False, highest=True, bad_index=False, low_mesh=None):
        name = f'png0001_geo_{key}{"_tangentspace" if tangent else ""}.iff'
        path = self.stage / name
        scene = {'hair': {'Model': {'hihead': {
            'VertexStream': [{'Stride': 12, 'Size': 48, 'Binary': 'vertex.bin'}],
            'IndexBuffer': {'Format': 'R16_UINT', 'Size': 18, 'Binary': 'index.bin'},
            'Lods': [{'LodVerts': 4}, {'LodVerts': 3}],
            'Prim': [{'Start': 0, 'Count': 3, 'Mesh': mesh,
                      'LodList': [{'Start': 3, 'Count': 6}, {'Start': 0, 'Count': 3}]}]}}}}
        if low_mesh:
            extra = copy.deepcopy(scene['hair']['Model']['hihead']['Prim'][0])
            extra['Mesh'] = low_mesh
            scene['hair']['Model']['hihead']['Prim'].append(extra)
        if highest:
            self.force(scene)
        with zipfile.ZipFile(path, 'w') as archive:
            archive.writestr('hair.SCNE', json.dumps(scene))
            archive.writestr('vertex.bin', b'v' * 48)
            archive.writestr('index.bin', struct.pack('<9H', *([99] * 9 if bad_index else [0, 1, 2, 0, 1, 2, 1, 2, 3])))
        self.companions.append((path, self.root / name))
        return path

    def prepare(self):
        return services.prepare_highest_detail_references(self.player, self.companions, self.backend, self.stage)

    def item(self, index=0):
        with zipfile.ZipFile(self.player) as archive:
            return json.loads(archive.read('appearance_info.json'))['accessory_items']['items'][index]

    def test_regular_and_both_route_to_retained_mesh(self):
        self.geometry()
        self.geometry('facialhair_beard', 'beardShape')
        self.geometry('facialhair_beard', 'beardShape', tangent=True)
        self.write_player()
        before = {p: p.read_bytes() for p, _ in self.companions}
        self.assertEqual(len(self.prepare()), 2)
        for index in (0, 1):
            self.assertEqual(self.item(index)['meshlo'], self.item(index)['mesh'])
        self.assertEqual(before, {p: p.read_bytes() for p, _ in self.companions})

    def test_tangent_only_supports_custom_name(self):
        self.appearance['accessory_items']['items'][0].update(name='hair_custom', tangentspace='yes')
        self.geometry('hair_custom', tangent=True)
        self.write_player()
        self.assertEqual(self.prepare()[0]['item'], 'hair_custom')

    def test_both_requires_each_companion(self):
        self.appearance['accessory_items']['items'][0]['tangentspace'] = 'both'
        self.geometry()
        self.write_player()
        before = self.player.read_bytes()
        self.assertEqual(self.prepare(), [])
        self.assertEqual(self.player.read_bytes(), before)

    def test_primary_must_exist_in_each_required_variant(self):
        self.appearance['accessory_items']['items'][0]['tangentspace'] = 'both'
        self.geometry()
        self.geometry(mesh='otherShape', tangent=True)
        self.write_player()
        self.assertEqual(self.prepare(), [])

    def test_valid_low_mesh_still_obeys_highest_detail_policy(self):
        self.appearance['accessory_items']['items'][0]['meshlo'] = 'otherLowShape'
        self.geometry(low_mesh='otherLowShape')
        self.write_player()
        self.assertEqual(self.prepare()[0]['previous'], 'otherLowShape')
        self.assertEqual(self.item()['meshlo'], 'hairShape')

    def test_does_not_add_absent_distance_selector(self):
        del self.appearance['accessory_items']['items'][0]['meshlo']
        self.geometry()
        self.write_player()
        self.assertEqual(self.prepare(), [])
        self.assertNotIn('meshlo', self.item())

    def test_unrelated_members_and_zip_metadata_are_preserved(self):
        self.geometry()
        self.write_player()
        with zipfile.ZipFile(self.player) as archive:
            before = {n: archive.read(n) for n in archive.namelist() if n != 'appearance_info.json'}
            infos = [(i.filename, i.date_time, i.compress_type, i.external_attr) for i in archive.infolist()]
        self.prepare()
        with zipfile.ZipFile(self.player) as archive:
            self.assertEqual(before, {n: archive.read(n) for n in before})
            self.assertEqual(infos, [(i.filename, i.date_time, i.compress_type, i.external_attr) for i in archive.infolist()])
            self.assertEqual(archive.comment, b'player comment')
        expected = copy.deepcopy(self.appearance)
        expected['accessory_items']['items'][0]['meshlo'] = 'hairShape'
        with zipfile.ZipFile(self.player) as archive:
            self.assertEqual(json.loads(archive.read('appearance_info.json')), expected)

    def test_idempotent_does_not_rewrite(self):
        self.geometry()
        self.write_player()
        self.prepare()
        before = self.player.read_bytes(), self.player.stat().st_mtime_ns
        self.assertEqual(self.prepare(), [])
        self.assertEqual((self.player.read_bytes(), self.player.stat().st_mtime_ns), before)

    def test_lower_detail_geometry_is_rejected_before_write(self):
        self.geometry(highest=False)
        self.write_player()
        before = self.player.read_bytes()
        with self.assertRaisesRegex(ValueError, 'full LOD0'):
            self.prepare()
        self.assertEqual(self.player.read_bytes(), before)

    def test_missing_vertex_is_rejected_before_write(self):
        self.geometry(bad_index=True)
        self.write_player()
        before = self.player.read_bytes()
        with self.assertRaisesRegex(ValueError, 'missing vertex'):
            self.prepare()
        self.assertEqual(self.player.read_bytes(), before)

    def test_duplicate_geometry_is_rejected(self):
        self.geometry()
        self.companions *= 2
        self.write_player()
        with self.assertRaisesRegex(ValueError, 'Duplicate accessory'):
            self.prepare()

    def test_published_or_external_source_is_not_modified(self):
        path = self.geometry()
        outside = self.root / path.name
        shutil.copyfile(path, outside)
        self.companions[0] = outside, outside
        self.write_player()
        before = self.player.read_bytes(), outside.read_bytes()
        with self.assertRaises(ValueError):
            self.prepare()
        self.assertEqual((self.player.read_bytes(), outside.read_bytes()), before)

    def test_failed_replace_preserves_player_and_removes_temporary(self):
        self.geometry()
        self.write_player()
        before = self.player.read_bytes()
        with patch.object(services.os, 'replace', side_effect=PermissionError('fixture lock')):
            with self.assertRaises(PermissionError):
                self.prepare()
        self.assertEqual(self.player.read_bytes(), before)
        self.assertEqual(list(self.stage.glob('.distance-*')), [])

    def test_wrapper_validates_rewritten_stage_before_publish(self):
        self.geometry()
        self.write_player()
        final = self.root / self.player.name
        final.write_bytes(b'previous published player')
        app = types.SimpleNamespace(everything_swap_target_game_var=types.SimpleNamespace(get=lambda: 'NBA 2K27'),
            everything_swap_final_output=final, everything_swap_stage_dir=self.stage,
            load_hair_backend=lambda: self.backend)
        def validation(*args):
            self.assertEqual(self.item()['meshlo'], 'hairShape')
            raise ValueError('other validation failed')
        with self.assertRaisesRegex(ValueError, 'other validation failed'):
            services.validate_swap_distances(app, validation, self.player, {}, self.companions)
        self.assertEqual(final.read_bytes(), b'previous published player')

    def test_non_2k27_passes_through_without_modification(self):
        self.write_player()
        before = self.player.read_bytes()
        app = types.SimpleNamespace(everything_swap_target_game_var=types.SimpleNamespace(get=lambda: 'NBA 2K26'))
        result = services.validate_swap_distances(app, lambda *args: {'errors': 0}, self.player, {})
        self.assertEqual(result['highestDetailReferences'], [])
        self.assertEqual(self.player.read_bytes(), before)

    def test_real_export_copies_preserve_every_geometry_buffer(self):
        source = project / 'artifacts/ui-conversion-output/png06827.iff'
        if not source.exists():
            self.skipTest('Representative conversion not yet generated')
        sources = [source, *sorted(source.parent.glob(source.stem + '_*.iff'))]
        hashes = {p: services.file_digest(p) for p in sources}
        shutil.copyfile(source, self.player)
        for path in sources[1:]:
            staged = self.stage / path.name
            shutil.copyfile(path, staged)
            self.companions.append((staged, self.root / path.name))
        changes = self.prepare()
        with zipfile.ZipFile(source) as before, zipfile.ZipFile(self.player) as after:
            for name in before.namelist():
                if name != 'appearance_info.json':
                    self.assertEqual(before.read(name), after.read(name), name)
            expected = json.loads(before.read('appearance_info.json'))
            for item in expected['accessory_items']['items']:
                if item.get('type') in ('hair', 'facialhair') and item.get('meshlo'):
                    item['meshlo'] = item['mesh']
            self.assertEqual(json.loads(after.read('appearance_info.json')), expected)
        self.assertEqual(hashes, {p: services.file_digest(p) for p in sources})
        for staged, destination in self.companions:
            self.assertEqual(services.file_digest(staged), hashes[source.parent / destination.name])
        self.assertEqual(self.backend.audit_hair_mesh_references(self.player, self.companions), [])
        self.assertLessEqual(len(changes), 2)


if __name__ == '__main__':
    result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(AccessoryDistance))
    report = {'tests': result.testsRun, 'skipped': len(result.skipped), 'failures': len(result.failures),
              'errors': len(result.errors), 'realExport': str(project / 'artifacts/ui-conversion-output/png06827.iff')}
    (project / 'artifacts/accessory-distance-report.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    raise SystemExit(0 if result.wasSuccessful() else 1)
