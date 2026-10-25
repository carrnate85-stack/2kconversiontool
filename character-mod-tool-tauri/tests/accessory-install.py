"""TEMP-only install fixtures using the shipped readers, LOD policy and ZIP writer."""
import copy
import ast
import json
import os
import shutil
import struct
import subprocess
import sys
import tempfile
import types
import unittest
import zipfile
from dataclasses import replace
from pathlib import Path
from unittest.mock import patch

PROJECT = Path(__file__).resolve().parents[1]
PRODUCTION = PROJECT.parent / 'character-mod-tool-webview2/dist/Character-Mod-Tool-WebView2-0.1.31/App/Backend'
sys.path[:0] = [str(PROJECT / 'engine'), str(PRODUCTION)]
import prototype_accessories as service


def write_zip(path, values):
    path.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(path, 'w') as archive:
        archive.comment = b'preserved comment'
        for name, value in values.items():
            archive.writestr(name, json.dumps(value) if isinstance(value, dict) else value)


class AccessoryInstall(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.private = tempfile.TemporaryDirectory(prefix='CmtAccessoryInstallSettings-')
        cls.settings = types.ModuleType('app_settings')
        cls.settings.load_settings = lambda: {}
        cls.settings.user_data_dir = lambda: Path(cls.private.name)
        cls.settings.LOG_DIR = Path(cls.private.name) / 'logs'
        cls.modules = patch.dict(sys.modules, app_settings=cls.settings)
        cls.modules.start()
        from hair_tools import hair_picker
        cls.backend = hair_picker

    @classmethod
    def tearDownClass(cls):
        cls.modules.stop()
        cls.private.cleanup()

    def setUp(self):
        if os.environ.get('CMT_ACCESSORY_TEST_ROOT'):
            self.root = Path(os.environ['CMT_ACCESSORY_TEST_ROOT'])
            self.root.mkdir()
        else:
            self.temporary = tempfile.TemporaryDirectory(prefix='CmtAccessoryInstall-')
            self.addCleanup(self.temporary.cleanup)
            self.root = Path(self.temporary.name)
        self.stage = self.root / 'stage'
        self.stage.mkdir()
        self.target = self.stage / 'png1.iff'
        self.source = self.root / 'source'
        self.source.mkdir()
        self.patches = []
        self.patch('WORKSPACE_BACKUP_DIR', self.root / 'backups')
        self.patch('GAME_ROOT', self.root / 'no-game')
        self.patch('parse_manifest_entries', lambda: {})
        self.patch('extract_archive_iff_fallback', self.extract)
        self.fit = self.patch('prepare_tangent_fitted_option', side_effect=self.fit_regular)
        self.bridge = types.SimpleNamespace(require_file=lambda p, label: str(Path(p).resolve(strict=True)),
            native_hair_backend=lambda: self.backend, selected_native_hair_option=lambda *a: self.option,
            emit_event=lambda *a, **kw: None)
        self.fit_inputs = []

    def patch(self, name, value=None, **kwargs):
        context = patch.object(self.backend, name, value, **kwargs) if value is not None else patch.object(self.backend, name, **kwargs)
        result = context.start()
        self.addCleanup(context.stop)
        return result

    def extract(self, name, output):
        path = self.source / Path(name).name
        if not path.is_file():
            raise FileNotFoundError('fixture has no archive companion: ' + name)
        shutil.copy2(path, output)

    def fit_regular(self, option, png, target_head_path, status_callback):
        self.fit_inputs.append((option.tangentspace, service.digest(Path(target_head_path))))
        return option, None

    def geometry(self, path, tangent=False, mesh='donorShape', shader='donor_shader', missing_lod=False):
        fmt = 'R16G16B16A16_UINT' if tangent else 'R21G21B22_UINT'
        scene = {'source': {'Material': {shader: None}, 'Model': {'hihead': {
            'VertexFormat': {'POSITION0': {'Format': fmt}},
            'VertexStream': [{'Stride': 8, 'Size': 24, 'Binary': 'vertex.bin'}],
            'IndexBuffer': {'Format': 'R16_UINT', 'Size': 6, 'Binary': 'index.bin'},
            'Lods': [] if missing_lod else [{'LodVerts': 3}],
            'Prim': [{'Start': 0, 'Count': 3, 'Mesh': mesh, 'Material': shader,
                      'LodList': [{'Start': 0, 'Count': 3}]}]}}}}
        write_zip(path, {'source.SCNE': scene, 'vertex.bin': b'v' * 24,
                          'index.bin': struct.pack('<3H', 0, 1, 2), 'physics.bin': b'untouched physics'})

    def texture(self, path, prefix='donor', missing=None):
        values = {}
        for channel in ('color', 'tangent'):
            logical = prefix + '_' + channel + '_o'
            binary = logical + '.0123456789abcdef.dds'
            values[logical + '.TXTR'] = {logical: {'Binary': binary, 'Width': 4, 'Height': 4, 'Mips': 1}}
            if missing != channel:
                values[binary] = b'DDS ' + bytes(124) + bytes(16)
        write_zip(path, values)

    def fixture(self, kind='hair', mode='both', source_mode=None, selected=None, config=False, custom=False):
        source_mode = source_mode or mode
        selected = (source_mode == 'yes') if selected is None else selected
        self.kind = kind
        self.key = ('AFRO_MEDIUM_facialHair_01' if custom and kind == 'facialhair' else kind + '_custom')
        self.donor_key = kind + '_donor'
        self.item = {'name': self.donor_key, 'type': kind, 'mesh': 'donorShape', 'meshlo': 'absentLowShape',
            'shader': 'donor_shader', 'tweaks': 'source_custom', 'tangentspace': source_mode,
            'texture': {'color': 'donor_color', 'tangent': 'donor_tangent'}}
        old = dict(copy.deepcopy(self.item), name=self.key, mesh='oldShape', shader='old_shader', tangentspace=mode)
        self.appearance = {'body_fit': {'neckTHIN': 0.25}, 'accessory_items': {
            'default_config': 'TEST', 'default_out_of_uniform_config': 'OTHER',
            'items': [old, {'name': 'unrelated', 'type': 'headband', 'mesh': 'band'}],
            'configurations': [{'name': 'TEST', 'items': [self.key, 'unrelated'], 'facetexture': {'color': 'face'}},
                               {'name': 'OTHER', 'items': [self.key, 'unrelated']}]}}
        write_zip(self.target, {'appearance_info.json': self.appearance, 'hihead.SCNE': b'head stays exact',
                                'appearance_info.rdat': b'unchanged', 'physics.bin': b'player physics'})
        write_zip(self.source / 'png2.iff', {'appearance_info.json': {'accessory_items': {'items': [self.item]}}})
        for tangent in service.variants(self.item):
            self.geometry(self.source / f'png2_geo_{self.donor_key}{"_tangentspace" if tangent else ""}.iff', tangent)
        for tangent in (False, True):
            self.geometry(self.stage / f'png1_geo_{self.key}{"_tangentspace" if tangent else ""}.iff', tangent, 'oldShape', 'old_shader')
        self.texture(self.source / f'png2_item_{self.donor_key}.iff')
        self.texture(self.stage / f'png1_item_{self.key}.iff', 'target')
        write_zip(self.stage / 'png1_config_test.iff', {'face.bin': b'face stays exact'})
        self.option = self.backend.HairOption(source_geo=self.source / f'png2_geo_{self.donor_key}{"_tangentspace" if selected else ""}.iff',
            source_item=self.source / f'png2_item_{self.donor_key}.iff', source_config=None,
            archive_geo=f'char/sig/png2_geo_{self.donor_key}{"_tangentspace" if selected else ""}.iff',
            archive_item=None, archive_config=None, source_png='2', player_name='', hair_key=self.donor_key,
            root_name='source', lod_verts=3, radius=None, center_y=None, origin='fixture', asset_type=kind, tangentspace=selected)
        self.payload = {'target': str(self.target), 'assetType': kind, 'assetId': 'fixture', 'targetKey': self.key}
        if config:
            self.payload.update(config='TEST', configScoped=True)

    def installed_item(self, key=None):
        value, _ = service.read_appearance(self.target, self.backend)
        return next(i for i in value['accessory_items']['items'] if i['name'] == (key or self.key))

    def hashes(self):
        return {p.name: service.digest(p) for p in self.stage.glob('*.iff')}

    def test_native_and_config_modes_for_both_types(self):
        for kind in ('hair', 'facialhair'):
            for mode in ('no', 'yes', 'both'):
                for config in (False, True):
                    with self.subTest(kind=kind, mode=mode, config=config):
                        self.fixture(kind, mode, config=config, custom=True)
                        before_source = {p.name: service.digest(p) for p in self.source.glob('*.iff')}
                        original = copy.deepcopy(self.appearance)
                        face = (self.stage / 'png1_config_test.iff').read_bytes()
                        result = service.install(self.payload, self.bridge)
                        item = self.installed_item(result['targetKey'])
                        self.assertEqual(item['meshlo'], item['mesh'])
                        self.assertEqual(item['mesh'], 'donorShape')
                        self.assertEqual(item['shader'], 'donor_shader')
                        self.assertEqual(item['texture'], self.item['texture'])
                        self.assertEqual(item['tangentspace'], mode)
                        self.assertEqual(Path(result['outputs'][-1]), self.target)
                        for tangent in service.variants(item):
                            name = f'png1_geo_{result["targetKey"]}{"_tangentspace" if tangent else ""}.iff'
                            service.validate_geometry(self.stage / name, item, tangent, self.backend)
                            with zipfile.ZipFile(self.stage / name) as archive:
                                self.assertEqual(archive.read('physics.bin'), b'untouched physics')
                        after, _ = service.read_appearance(self.target, self.backend)
                        self.assertEqual(after['body_fit'], original['body_fit'])
                        self.assertEqual(after['accessory_items']['configurations'][1], original['accessory_items']['configurations'][1])
                        self.assertEqual((self.stage / 'png1_config_test.iff').read_bytes(), face)
                        with zipfile.ZipFile(self.target) as archive:
                            self.assertEqual(archive.comment, b'preserved comment')
                            self.assertEqual(archive.read('appearance_info.rdat'), b'unchanged')
                            self.assertEqual(archive.read('physics.bin'), b'player physics')
                        self.assertEqual(before_source, {p.name: service.digest(p) for p in self.source.glob('*.iff')})

    def test_both_replaces_stale_tangent_and_regular(self):
        self.fixture('hair')
        before = self.hashes()
        service.install(self.payload, self.bridge)
        after = self.hashes()
        for suffix in ('', '_tangentspace'):
            name = f'png1_geo_{self.key}{suffix}.iff'
            self.assertNotEqual(before[name], after[name])
        self.assertEqual(self.fit_inputs, [(False, before['png1.iff'])])

    def test_paired_sources_support_either_catalog_selection_and_all_target_modes(self):
        for kind in ('hair', 'facialhair'):
            for mode in ('no', 'yes', 'both'):
                for selected in (False, True):
                    with self.subTest(kind=kind, mode=mode, selected=selected):
                        self.fixture(kind, mode, source_mode='both', selected=selected)
                        service.install(self.payload, self.bridge)
                        self.assertEqual(self.installed_item()['tangentspace'], mode)

    def test_config_paired_tangent_selection_installs_both(self):
        self.fixture('facialhair', 'no', source_mode='both', selected=True, config=True)
        result = service.install(self.payload, self.bridge)
        self.assertEqual(self.installed_item(result['targetKey'])['tangentspace'], 'both')
        self.assertEqual(len([p for p in result['outputs'] if '_geo_' in p]), 2)
        self.assertEqual(len(result['configs']), 2)

    def test_missing_required_variant_does_not_publish(self):
        for kind in ('hair', 'facialhair'):
            with self.subTest(kind=kind):
                self.fixture(kind, 'both', source_mode='no')
                before = self.hashes()
                with self.assertRaisesRegex(ValueError, 'no verified tangent companion'):
                    service.install(self.payload, self.bridge)
                self.assertEqual(before, self.hashes())

    def test_primary_shader_lod_and_vertex_failures_do_not_publish(self):
        for problem in ('primary', 'shader', 'lod', 'vertex'):
            with self.subTest(problem=problem):
                self.fixture('facialhair', 'no')
                path = self.option.source_geo
                if problem == 'primary':
                    self.geometry(path, mesh='wrongShape')
                elif problem == 'shader':
                    self.geometry(path, shader='wrong_shader')
                elif problem == 'lod':
                    self.geometry(path, missing_lod=True)
                else:
                    with zipfile.ZipFile(path) as archive:
                        values = {n: archive.read(n) for n in archive.namelist()}
                    values['vertex.bin'] = b'short'
                    write_zip(path, values)
                before = self.hashes()
                with self.assertRaises((ValueError, RuntimeError)):
                    service.install(self.payload, self.bridge)
                self.assertEqual(before, self.hashes())

    def test_mislabeled_variant_is_rejected(self):
        self.fixture('facialhair', 'yes')
        self.geometry(self.option.source_geo, tangent=False)
        before = self.hashes()
        with self.assertRaisesRegex(ValueError, 'variant'):
            service.install(self.payload, self.bridge)
        self.assertEqual(before, self.hashes())

    def test_texture_failure_and_unresolved_binding_do_not_publish(self):
        for problem in ('pixels', 'binding', 'copy'):
            with self.subTest(problem=problem):
                self.fixture('facialhair', 'yes', config=True)
                if problem == 'pixels':
                    self.texture(self.option.source_item, missing='color')
                elif problem == 'binding':
                    self.item['texture']['normal'] = 'absent_normal'
                    write_zip(self.source / 'png2.iff', {'appearance_info.json': {'accessory_items': {'items': [self.item]}}})
                else:
                    self.option = replace(self.option, source_item=self.source / 'missing.iff')
                before = self.hashes()
                with self.assertRaises(ValueError):
                    service.install(self.payload, self.bridge)
                self.assertEqual(before, self.hashes())

    def test_authored_optional_occlusion_without_texture_is_preserved(self):
        for kind in ('hair', 'facialhair'):
            with self.subTest(kind=kind):
                self.fixture(kind, 'both', config=True)
                self.item['texture']['occlusion'] = self.donor_key + '_occlusion'
                write_zip(self.source / 'png2.iff', {'appearance_info.json': {'accessory_items': {'items': [self.item]}}})
                before = {p.name: service.digest(p) for p in self.source.glob('*.iff')}
                result = service.install(self.payload, self.bridge)
                self.assertEqual(self.installed_item(result['targetKey'])['texture']['occlusion'], self.item['texture']['occlusion'])
                self.assertEqual(before, {p.name: service.digest(p) for p in self.source.glob('*.iff')})

    def test_partial_texture_copy_failure_does_not_publish(self):
        self.fixture('hair', 'both')
        before = self.hashes()
        original = shutil.copy2
        def partial(source, destination, *a, **kw):
            if Path(source) == self.option.source_item:
                Path(destination).write_bytes(b'partial pixels')
                raise OSError('fixture texture copy failure')
            return original(source, destination, *a, **kw)
        with patch.object(service.shutil, 'copy2', side_effect=partial):
            with self.assertRaisesRegex(OSError, 'texture copy failure'):
                service.install(self.payload, self.bridge)
        self.assertEqual(before, self.hashes())

    def test_unique_texture_channel_aliases_are_rebound(self):
        self.fixture('hair', 'no')
        self.texture(self.option.source_item, 'different')
        service.install(self.payload, self.bridge)
        self.assertEqual(self.installed_item()['texture'], {'color': 'different_color', 'tangent': 'different_tangent'})

    def test_keep_staged_textures_preserves_binding_and_pixels(self):
        self.fixture('hair', 'no')
        self.appearance['accessory_items']['items'][0]['texture'] = {'color': 'target_color', 'tangent': 'target_tangent'}
        write_zip(self.target, {'appearance_info.json': self.appearance})
        item_path = self.stage / f'png1_item_{self.key}.iff'
        before = item_path.read_bytes()
        self.payload['copyItem'] = False
        result = service.install(self.payload, self.bridge)
        self.assertFalse(result['texturesCopied'])
        self.assertEqual(item_path.read_bytes(), before)
        self.assertEqual(self.installed_item()['texture'], {'color': 'target_color', 'tangent': 'target_tangent'})

    def test_fitting_failure_does_not_publish(self):
        self.fixture('hair', 'no')
        before = self.hashes()
        with patch.object(self.backend, 'prepare_tangent_fitted_option', side_effect=ValueError('fixture fit failed')):
            with self.assertRaisesRegex(ValueError, 'fit failed'):
                service.install(self.payload, self.bridge)
        self.assertEqual(before, self.hashes())

    def test_fitting_uses_private_staged_donor_and_target_snapshots(self):
        self.fixture('hair', 'no')
        donor_digest = service.digest(self.source / 'png2.iff')
        target_digest = service.digest(self.target)
        def fit(option, png, target_head_path, status_callback):
            target_input = Path(target_head_path)
            self.assertNotEqual(target_input, self.target)
            self.assertEqual(service.digest(target_input), target_digest)
            donor_input = target_input.parent / 'fit-donor.iff'
            self.backend.extract_archive_iff_fallback('char/sig/png2.iff', donor_input)
            self.assertEqual(service.digest(donor_input), donor_digest)
            return option, None
        with patch.object(self.backend, 'prepare_tangent_fitted_option', side_effect=fit), \
             patch.object(self.backend, 'extract_archive_iff_fallback', side_effect=AssertionError('unexpected base-game donor')):
            service.install(self.payload, self.bridge)

    def test_external_package_change_during_preparation_is_not_overwritten(self):
        self.fixture('hair', 'no')
        real = self.backend.copy_geo_with_target_names
        def external(source, output, *names):
            real(source, output, *names)
            self.target.write_bytes(b'independent player edit')
        with patch.object(self.backend, 'copy_geo_with_target_names', side_effect=external):
            with self.assertRaisesRegex(RuntimeError, 'changed during accessory preparation'):
                service.install(self.payload, self.bridge)
        self.assertEqual(self.target.read_bytes(), b'independent player edit')

    def test_selected_local_geometry_wins_over_manifest(self):
        self.fixture('facialhair', 'both')
        with patch.object(self.backend, 'extract_archive_iff_fallback', side_effect=AssertionError('unexpected manifest fallback')):
            service.install(self.payload, self.bridge)

    def test_manifest_only_sources_use_private_extraction(self):
        self.fixture('facialhair', 'both')
        paths = list(self.source.glob('*.iff'))
        manifest = {'char/sig/' + p.name: ('char/sig/' + p.name, '0A', 0, p.stat().st_size) for p in paths}
        self.option = replace(self.option, source_geo=None, source_item=None,
                              archive_item=f'char/sig/png2_item_{self.donor_key}.iff')
        with patch.object(self.backend, 'parse_manifest_entries', return_value=manifest):
            service.install(self.payload, self.bridge)
        self.assertEqual(self.installed_item()['tangentspace'], 'both')

    def test_shared_texture_materialization_uses_existing_decoder(self):
        self.fixture('facialhair', 'no')
        texture = self.option.source_item
        content_hash = '0123456789abcdef'
        binary = 'donor_color_o.' + content_hash + '.tld'
        with zipfile.ZipFile(texture) as archive:
            values = {n: archive.read(n) for n in archive.namelist() if 'color' not in n}
        values['donor_color_o.TXTR'] = {'donor_color_o': {'Binary': binary}}
        write_zip(texture, values)
        game = self.root / 'no-game'
        game.mkdir()
        (game / '0A').write_bytes(bytes(64))
        tld_name = 'shared/01/' + binary
        mip_name = str(Path(tld_name).with_suffix('.mip0')).replace('\\', '/')
        manifest = {tld_name: (tld_name, '0A', 0, 32), mip_name: (mip_name, '0A', 32, 32)}
        def decode(arguments, **kwargs):
            self.assertEqual(arguments[1], '--texture')
            self.assertEqual(arguments[2:5], [str(game / '0A'), '0', '32'])
            Path(arguments[-1]).write_bytes(b'DDS ' + bytes(140))
            return types.SimpleNamespace(returncode=0)
        with patch.object(self.backend, 'parse_manifest_entries', return_value=manifest), \
             patch.object(self.backend, 'native_vcz_decoder_path', return_value=Path('verified-decoder.exe')), \
             patch.object(service.subprocess, 'run', side_effect=decode):
            service.install(self.payload, self.bridge)
        with zipfile.ZipFile(self.stage / f'png1_item_{self.key}.iff') as archive:
            self.assertIn(str(Path(binary).with_suffix('.dds')), archive.namelist())

    def test_shared_texture_decoder_failure_does_not_publish(self):
        self.fixture('facialhair', 'no')
        texture = self.option.source_item
        binary = 'donor_color_o.0123456789abcdef.tld'
        write_zip(texture, {'donor_color_o.TXTR': {'donor_color_o': {'Binary': binary}}})
        before = self.hashes()
        with self.assertRaisesRegex(ValueError, 'pixels'):
            service.install(self.payload, self.bridge)
        self.assertEqual(before, self.hashes())

    def test_missing_local_source_is_not_silently_replaced(self):
        self.fixture('hair', 'no')
        self.option = replace(self.option, source_geo=self.source / 'missing.iff')
        before = self.hashes()
        with self.assertRaisesRegex(ValueError, 'staged accessory source is missing'):
            service.install(self.payload, self.bridge)
        self.assertEqual(before, self.hashes())

    def test_publication_failure_restores_all_existing_files(self):
        self.fixture('hair', 'both')
        before = self.hashes()
        real_replace = os.replace
        def fail(source, dest):
            if Path(dest) == self.target and Path(source).parent.name == 'prepared':
                raise OSError('fixture appearance publication failure')
            return real_replace(source, dest)
        with patch.object(service.os, 'replace', side_effect=fail):
            with self.assertRaisesRegex(OSError, 'publication failure'):
                service.install(self.payload, self.bridge)
        self.assertEqual(before, self.hashes())
        self.assertEqual(list(self.stage.glob('.cmt-accessory-*')), [])

    def test_interruption_recovery_restores_files_and_removes_new_companions(self):
        for config in (False, True):
            with self.subTest(config=config):
                self.fixture('facialhair', 'both', config=config)
                before = self.hashes()
                existing_transactions = len(list(self.stage.glob('.cmt-accessory-*')))
                real_replace = os.replace
                def interrupt(source, dest):
                    if Path(dest) == self.target and Path(source).parent.name == 'prepared':
                        raise KeyboardInterrupt('fixture interruption')
                    return real_replace(source, dest)
                with patch.object(service.os, 'replace', side_effect=interrupt):
                    with self.assertRaises(KeyboardInterrupt):
                        service.install(self.payload, self.bridge)
                self.assertEqual(len(list(self.stage.glob('.cmt-accessory-*'))), existing_transactions + 1)
                report = service.recover_pending(self.target)
                self.assertEqual(before, self.hashes())
                self.assertTrue(report['changed'])
                self.assertTrue(all(t['state'] == 'rolled_back' and t['preserved'] for t in report['transactions']))

    def test_recovery_after_replace_before_journal_complete(self):
        self.fixture('hair', 'no')
        before = self.hashes()
        original = service.atomic_json
        def interrupt(path, value):
            if value.get('state') == 'complete':
                raise KeyboardInterrupt('after appearance replacement')
            return original(path, value)
        with patch.object(service, 'atomic_json', side_effect=interrupt):
            with self.assertRaises(KeyboardInterrupt):
                service.install(self.payload, self.bridge)
        service.recover_pending(self.target)
        self.assertEqual(before, self.hashes())

    def test_recovery_preserves_concurrent_changes_and_recovery_data(self):
        self.fixture('facialhair', 'no')
        real_replace = os.replace
        def interrupt(source, dest):
            if Path(dest) == self.target and Path(source).parent.name == 'prepared':
                raise KeyboardInterrupt()
            return real_replace(source, dest)
        with patch.object(service.os, 'replace', side_effect=interrupt):
            with self.assertRaises(KeyboardInterrupt):
                service.install(self.payload, self.bridge)
        geo = self.stage / f'png1_geo_{self.key}.iff'
        geo.write_bytes(b'independent user change')
        with self.assertRaisesRegex(RuntimeError, 'changed file'):
            service.recover_pending(self.target)
        self.assertEqual(geo.read_bytes(), b'independent user change')
        self.assertEqual(len(list(self.stage.glob('.cmt-accessory-*'))), 1)

    def test_backups_are_bounded_and_use_existing_retention(self):
        self.fixture('hair', 'no')
        with patch.object(self.backend, 'prune_workspace_backups', wraps=self.backend.prune_workspace_backups) as prune:
            result = service.install(self.payload, self.bridge)
        self.assertGreater(prune.call_count, 0)
        self.assertEqual(len(result['backups']), 3)
        self.assertTrue(all(Path(p).is_file() for p in result['backups']))

    def test_config_scoped_requires_selection(self):
        self.fixture('hair', 'no')
        self.payload['configScoped'] = True
        before = self.hashes()
        with self.assertRaisesRegex(ValueError, 'Select a configuration'):
            service.install(self.payload, self.bridge)
        self.assertEqual(before, self.hashes())

    def test_legacy_or_active_lock_is_not_stolen(self):
        self.fixture('hair', 'no')
        lock = self.stage / '.png1.config.lock'
        lock.write_text('')
        with self.assertRaisesRegex(RuntimeError, 'already running'):
            service.install(self.payload, self.bridge)
        self.assertEqual(lock.read_text(), '')

    def test_dead_accessory_lock_is_recovered(self):
        self.fixture('hair', 'no')
        lock = self.stage / '.png1.config.lock'
        lock.write_text(json.dumps({'kind': 'cmt-accessory-install', 'pid': 99999999, 'token': 'old'}))
        with patch.object(service, 'process_alive', return_value=False):
            service.install(self.payload, self.bridge)
        self.assertFalse(lock.exists())

    def test_hard_process_exit_recovers_on_next_lock(self):
        child_root = self.root / 'hard-exit'
        code = '''import importlib.util, json, os
from pathlib import Path
spec = importlib.util.spec_from_file_location('accessory_install_fixture', os.environ['CMT_ACCESSORY_TEST_SCRIPT'])
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
case = module.AccessoryInstall()
case.setUpClass()
case.setUp()
case.fixture('facialhair', 'both', config=True)
(case.root / 'before.json').write_text(json.dumps(case.hashes()))
original = module.service.os.replace
def crash(source, destination):
    if Path(destination) == case.target and Path(source).parent.name == 'prepared':
        os._exit(71)
    return original(source, destination)
module.service.os.replace = crash
module.service.install(case.payload, case.bridge)
'''
        environment = dict(os.environ, CMT_ACCESSORY_TEST_ROOT=str(child_root), CMT_ACCESSORY_TEST_SCRIPT=str(Path(__file__).resolve()))
        child = subprocess.run([sys.executable, '-B', '-c', code], env=environment,
                               capture_output=True, text=True, timeout=45)
        self.assertEqual(child.returncode, 71, child.stdout + child.stderr)
        target = child_root / 'stage/png1.iff'
        self.assertTrue((target.parent / '.png1.config.lock').is_file())
        calls = []
        hooks = service.recovery_hooks({'inspect': lambda payload: calls.append(
            {p.name: service.digest(p) for p in target.parent.glob('*.iff')})}, lambda *a: None)
        hooks['inspect']({'filePath': str(target)})
        after = {p.name: service.digest(p) for p in target.parent.glob('*.iff')}
        self.assertEqual(json.loads((child_root / 'before.json').read_text()), after)
        self.assertEqual(calls, [after])
        self.assertFalse((target.parent / '.png1.config.lock').exists())
        retained = list(target.parent.glob('.cmt-accessory-*'))
        self.assertEqual(len(retained), 1)
        self.assertEqual(json.loads((retained[0] / 'journal.json').read_text())['state'], 'rolled_back')

    def test_retained_unknown_work_is_preserved_for_every_terminal_state(self):
        self.fixture('hair', 'no')
        for state in ('preparing', 'complete', 'rolled_back'):
            with self.subTest(state=state):
                folder = self.stage / ('.cmt-accessory-png1-unknown-' + state)
                folder.mkdir()
                (folder / 'inputs').mkdir()
                protected = folder / 'inputs/user-work.blend'
                protected.write_bytes(b'protected Blender work')
                notes = folder / 'unknown.txt'
                notes.write_bytes(b'protected notes')
                service.atomic_json(folder / 'journal.json', {'version': 1, 'owner': 'cmt-accessory-install',
                    'target': self.target.name, 'state': state, 'records': [],
                    'ownedFiles': ['journal.json', 'inputs']})
                report = service.recover_player(self.target)
                transaction = next(t for t in report['transactions'] if t['path'] == str(folder))
                self.assertTrue(transaction['preserved'])
                self.assertIn('inputs/user-work.blend', transaction['unexpected'])
                self.assertIn('unknown.txt', transaction['unexpected'])
                self.assertTrue(any(str(folder) in warning for warning in report['warnings']))
                self.assertEqual(protected.read_bytes(), b'protected Blender work')
                self.assertEqual(notes.read_bytes(), b'protected notes')

    def test_unknown_work_in_interrupted_commit_is_preserved_after_rollback(self):
        self.fixture('facialhair', 'both', config=True)
        before = self.hashes()
        original = os.replace
        def interrupt(source, destination):
            if Path(destination) == self.target and Path(source).parent.name == 'prepared':
                raise KeyboardInterrupt()
            return original(source, destination)
        with patch.object(service.os, 'replace', side_effect=interrupt):
            with self.assertRaises(KeyboardInterrupt):
                service.install(self.payload, self.bridge)
        folder = next(self.stage.glob('.cmt-accessory-*'))
        protected = folder / 'user-work.blend'
        protected.write_bytes(b'protected work in a retained commit')
        old_restore = folder / f'restore-png1_geo_{self.key}.iff'
        old_restore.write_bytes(b'protected file at the old restore name')
        old_journal_temp = folder / 'journal.json.preparing'
        old_journal_temp.write_bytes(b'protected file at the old journal temporary name')
        events = []
        report = service.recover_player(self.target, lambda *args: events.append(args))
        self.assertEqual(before, self.hashes())
        self.assertEqual(protected.read_bytes(), b'protected work in a retained commit')
        self.assertEqual(old_restore.read_bytes(), b'protected file at the old restore name')
        self.assertEqual(old_journal_temp.read_bytes(), b'protected file at the old journal temporary name')
        self.assertIn('user-work.blend', report['transactions'][0]['unexpected'])
        self.assertTrue(events)
        again = service.recover_player(self.target)
        self.assertFalse(again['changed'])
        self.assertEqual(protected.read_bytes(), b'protected work in a retained commit')

    def test_fresh_job_preserves_and_reports_unknown_work(self):
        self.fixture('hair', 'no')
        original = service.publish
        def publish_with_user_file(folder, *args):
            result = original(folder, *args)
            (folder / 'user-work.blend').write_bytes(b'new user Blender work')
            return result
        with patch.object(service, 'publish', side_effect=publish_with_user_file):
            result = service.install(self.payload, self.bridge)
        folder = next(self.stage.glob('.cmt-accessory-*'))
        self.assertEqual((folder / 'user-work.blend').read_bytes(), b'new user Blender work')
        self.assertTrue(any('user-work.blend' in warning for warning in result['warnings']))
        self.assertEqual(self.installed_item()['meshlo'], 'donorShape')

    def test_first_read_hooks_restore_before_preview_and_edit(self):
        for command, field in (('preview-resources', 'target'), ('configs-list', 'target'),
                               ('configs-change', 'target'), ('inspect', 'filePath'), ('read-archive', 'path')):
            with self.subTest(command=command):
                self.fixture('facialhair', 'no')
                before = self.hashes()
                original = os.replace
                def interrupt(source, destination):
                    if Path(destination) == self.target and Path(source).parent.name == 'prepared':
                        raise KeyboardInterrupt()
                    return original(source, destination)
                with patch.object(service.os, 'replace', side_effect=interrupt):
                    with self.assertRaises(KeyboardInterrupt):
                        service.install(self.payload, self.bridge)
                def read(payload):
                    self.assertEqual(before, self.hashes())
                    return 'read recovered package'
                handlers = service.recovery_hooks({command: read}, lambda *a: None)
                value = self.target if field != 'path' else self.stage / f'png1_geo_{self.key}.iff'
                self.assertEqual(handlers[command]({field: str(value)}), 'read recovered package')

    def test_first_read_recovery_conflict_blocks_the_read(self):
        self.fixture('hair', 'no')
        folder = self.stage / '.cmt-accessory-png1-conflict'
        folder.mkdir()
        service.atomic_json(folder / 'journal.json', {'version': 1, 'owner': 'cmt-accessory-install',
            'target': self.target.name, 'state': 'committing',
            'records': [{'name': self.target.name, 'before': '0' * 64, 'after': '1' * 64}]})
        operation = unittest.mock.Mock()
        handler = service.recovery_hooks({'configs-list': operation}, lambda *a: None)['configs-list']
        with self.assertRaisesRegex(RuntimeError, 'changed file'):
            handler({'target': str(self.target)})
        operation.assert_not_called()

    def test_command_registration_and_player_publication_wiring(self):
        tree = ast.parse((PROJECT / 'engine/prototype_services.py').read_text(encoding='utf-8-sig'))
        commands = next(n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == 'commands')
        returned = commands.body[-1].value
        install_node = next(v for k, v in zip(returned.keys, returned.values)
                            if isinstance(k, ast.Constant) and k.value == 'native-hair-install')
        expression = compile(ast.Expression(body=install_node), 'accessory-registration', 'eval')
        with patch.object(service, 'install', return_value={'installed': True}) as install:
            call = eval(expression, {'prototype_accessories': service, 'bridge': self.bridge})
            self.assertEqual(call({'fixture': True}), {'installed': True})
            install.assert_called_once_with({'fixture': True}, self.bridge)
        function = next(n for n in commands.body if isinstance(n, ast.FunctionDef) and n.name == 'replace_player')
        helper = types.ModuleType('prototype_player_writes')
        helper.publish_player = unittest.mock.Mock()
        namespace = {'valid_player': object(), 'file_digest': object()}
        with patch.dict(sys.modules, prototype_player_writes=helper):
            exec(compile(ast.Module(body=[function], type_ignores=[]), 'player-wiring', 'exec'), namespace)
            namespace['replace_player']('prepared', 'target', 'backup')
        helper.publish_player.assert_called_once_with('prepared', 'target', 'backup',
            validate=namespace['valid_player'], digest=namespace['file_digest'])
        recovery_node = next(v for k, v in zip(returned.keys, returned.values)
                             if isinstance(k, ast.Constant) and k.value == 'accessory-recover-player')
        expression = compile(ast.Expression(body=recovery_node), 'accessory-recovery-registration', 'eval')
        with patch.object(service, 'recover_request', return_value={'changed': True}) as recovery:
            call = eval(expression, {'prototype_accessories': service, 'bridge': self.bridge})
            self.assertEqual(call({'target': 'png1.iff'}), {'changed': True})
            recovery.assert_called_once_with({'target': 'png1.iff'}, self.bridge.emit_event)
        bridge_tree = ast.parse((PROJECT / 'engine/prototype_bridge.py').read_text(encoding='utf-8-sig'))
        self.assertTrue(any(isinstance(n, ast.Call) and isinstance(n.func, ast.Attribute)
                            and n.func.attr == 'recovery_hooks' for n in ast.walk(bridge_tree)))


if __name__ == '__main__':
    result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(AccessoryInstall))
    report_path = Path(tempfile.gettempdir()) / ('CmtAccessoryInstall-result-' + str(os.getpid()) + '.json')
    report_path.write_text(json.dumps({'tests': result.testsRun, 'failures': len(result.failures),
        'errors': len(result.errors), 'skipped': len(result.skipped)}, indent=2), encoding='utf-8')
    print('TEMP report: ' + str(report_path))
    raise SystemExit(0 if result.wasSuccessful() else 1)
