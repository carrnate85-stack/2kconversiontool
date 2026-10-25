import copy
import json
import os
import runpy
import sys
import tempfile
import types
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'engine'))
import prototype_blender_snapshot as snapshots
import prototype_services


def player(path, label='original', appearance=None):
    with zipfile.ZipFile(path, 'w') as archive:
        archive.writestr('player.scne', label)
        if appearance is not None:
            archive.writestr('appearance_info.json', json.dumps(appearance))


class SnapshotTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='cmt-blender-snapshot-python-')
        self.root = Path(self.temporary.name)
        self.session = self.root / 'session'
        self.session.mkdir()
        self.source = self.root / 'staged'
        self.source.mkdir()
        self.settings = {'blender_exe':'configured-blender.exe','head_swap_tool':'custom-addon'}
        (self.root / 'settings.json').write_text(json.dumps(self.settings))
        self.bridge = types.SimpleNamespace(fast_settings=lambda:copy.deepcopy(self.settings),
            game_root_for=lambda *_:self.root / 'missing-game', command_run_swap=lambda _: {},
            command_health=lambda _: {'groups':[]},command_health_repair=lambda _: {}, emit_event=lambda *a,**kw:None)
        self.target = self.source / 'png123.iff'
        self.appearance = {'appearance':{'gender':'male'},'accessory_items':{'default_config':'default',
            'configurations':[{'name':'default','items':[]},{'name':'Custom Fit','items':['hair']}],
            'items':[{'name':'hair','tangentspace':'no'}]}}
        player(self.target, appearance=self.appearance)
        self.config = self.source / 'png123_config_custom_fit.iff'
        self.hair = self.source / 'png123_geo_hair.iff'
        player(self.config, 'custom config')
        player(self.hair, 'old hair')
        player(self.source / 'png123_geo_hair_tangentspace.iff', 'unselected tangent')
        self.folder = self.session / 'blender-snapshot-test-1'
        self.folder.mkdir()
        self.payload = {'target':str(self.target),'config':'Custom Fit','_blenderSnapshot':str(self.folder)}
        self.environment = patch.dict(os.environ, CMT_SESSION=str(self.session))
        self.environment.start()
        # The tested preview is fully staged and does not need the original conversion tools.
        self.manifest = patch.dict(sys.modules, character_manifest=types.SimpleNamespace(extract_native_manifest_iff=lambda *a:None))
        self.manifest.start()

    def tearDown(self):
        self.manifest.stop()
        self.environment.stop()
        self.temporary.cleanup()

    def prepare(self):
        return snapshots.prepare(self.payload,self.bridge,self.root,self.root,self.session)

    def test_custom_config_exact_geometry_and_original_digest_are_frozen(self):
        expected = snapshots.digest(self.target)
        resources = self.prepare()
        rows = resources['options']['companionPaths']
        self.assertEqual({row['name'] for row in rows},{self.config.name,self.hair.name})
        self.assertEqual(resources['activeItems'],['hair'])
        self.assertEqual(os.environ['CMT_SESSION'],str(self.session))
        self.assertFalse((self.folder / 'preview-resources').exists())
        manifest = snapshots.launch_inputs(self.payload,self.session)
        self.assertEqual(manifest['expectedDigest'],expected)
        self.assertEqual(manifest['settings'],self.settings)
        self.assertEqual(Path(manifest['source']).read_bytes(),self.target.read_bytes())
        snapshots.validate(self.payload,self.bridge,self.root,self.session)

    def test_full_swap_after_capture_cannot_rebase_expected_return_digest(self):
        self.prepare()
        before = snapshots.launch_inputs(self.payload,self.session)
        player(self.target,'full swap result',self.appearance)
        with self.assertRaisesRegex(ValueError,'sources or settings changed'):
            snapshots.validate(self.payload,self.bridge,self.root,self.session)
        captured = snapshots.launch_inputs(self.payload,self.session)
        self.assertEqual(captured['expectedDigest'],before['expectedDigest'])
        returned = self.session / 'returned.iff'
        player(returned,'edits')
        commands = prototype_services.commands(self.bridge,self.root,self.root)
        with self.assertRaisesRegex(ValueError,'changed while Blender'):
            commands['blender-apply-return']({'target':str(self.target),'returned':str(returned),
                'backup':str(self.session/'backup.iff'),'expectedDigest':captured['expectedDigest']})
        self.assertFalse((self.session/'backup.iff').exists())

    def test_same_size_dependency_replacement_with_restored_mtime_is_detected(self):
        resources = self.prepare()
        frozen = next(Path(row['path']) for row in resources['options']['companionPaths'] if row['name']==self.hair.name)
        before = frozen.read_bytes()
        info = self.hair.stat()
        self.hair.write_bytes(before.replace(b'old hair',b'new hair'))
        os.utime(self.hair,ns=(info.st_atime_ns,info.st_mtime_ns))
        with self.assertRaisesRegex(ValueError,'sources or settings changed'):
            snapshots.validate(self.payload,self.bridge,self.root,self.session)
        self.assertEqual(frozen.read_bytes(),before)

    def test_dependency_added_during_held_decode_is_detected(self):
        self.prepare()
        player(self.source / 'png123_item_hair.iff','new staged override')
        with self.assertRaisesRegex(ValueError,'sources or settings changed'):
            snapshots.validate(self.payload,self.bridge,self.root,self.session)

    def test_settings_file_and_effective_settings_changes_are_detected(self):
        self.prepare()
        self.settings['head_swap_tool']='another-addon'
        with self.assertRaisesRegex(ValueError,'sources or settings changed'):
            snapshots.validate(self.payload,self.bridge,self.root,self.session)

    def test_lexical_alias_and_target_ownership(self):
        self.prepare()
        alias = {**self.payload,'target':str(self.source/'./png123.iff')}
        snapshots.validate(alias,self.bridge,self.root,self.session)
        other = self.source/'other.iff'
        player(other)
        with self.assertRaisesRegex(ValueError,'different target'):
            snapshots.launch_inputs({**self.payload,'target':str(other)},self.session)

    def test_frozen_input_tampering_is_rejected(self):
        resources=self.prepare()
        Path(resources['filePath']).write_bytes(b'changed snapshot')
        with self.assertRaisesRegex(ValueError,'input changed'):
            snapshots.launch_inputs(self.payload,self.session)

    def test_source_change_during_capture_fails_and_cleans_only_its_folder(self):
        sentinel = self.session/'another-preparation'
        sentinel.mkdir()
        real_commands = prototype_services.commands
        def changing_commands(*args):
            commands = real_commands(*args)
            original = commands['preview-resources']
            def resources(payload):
                value=original(payload)
                player(self.hair,'replacement during capture')
                return value
            commands['preview-resources']=resources
            return commands
        with patch.object(prototype_services,'commands',changing_commands):
            with self.assertRaisesRegex(ValueError,'sources or settings changed'):
                self.prepare()
        self.assertFalse(self.folder.exists())
        self.assertTrue(sentinel.is_dir())
        self.assertEqual(os.environ['CMT_SESSION'],str(self.session))

    def test_bounds_and_invalid_configuration_cleanup(self):
        with patch.object(snapshots,'MAX_BYTES',1):
            with self.assertRaisesRegex(ValueError,'budget'):
                self.prepare()
        # Pre-capture validation errors do not delete a directory they did not create.
        self.assertTrue(self.folder.exists())
        self.payload['config']='deleted config'
        with self.assertRaisesRegex(ValueError,'configuration no longer exists'):
            self.prepare()
        self.assertFalse(self.folder.exists())

    def test_foreign_snapshot_directory_is_never_read_or_deleted(self):
        with self.assertRaisesRegex(ValueError,'app session'):
            snapshots.prepare({**self.payload,'_blenderSnapshot':str(self.root/'blender-snapshot-foreign')},
                self.bridge,self.root,self.root,self.session)
        self.assertTrue(self.target.is_file())

    def wired_commands(self):
        wiring = runpy.run_path(str(Path(__file__).with_name('blender-snapshot-wiring.py')))
        source = Path(prototype_services.__file__).read_text(encoding='utf-8')
        module = types.ModuleType('snapshot_wired_services_under_test')
        exec(compile(wiring['wire_services'](source), '<snapshot-service-wiring>', 'exec'),module.__dict__)
        return module, module.commands(self.bridge,self.root,self.root)

    def test_proposed_launch_wiring_preserves_main_bundle_settings_and_return_backup(self):
        self.prepare()
        frozen = snapshots.launch_inputs(self.payload,self.session)
        original = self.target.read_bytes()
        module, commands = self.wired_commands()
        events = []
        def event(_kind,_message,**row):
            events.append(row)
            if row['state']=='return-ready':
                self.assertEqual(row['expectedDigest'],snapshots.digest(self.target))
                result=commands['blender-apply-return'](row)
                self.assertEqual(result['output'],str(self.target))
                self.assertEqual(Path(row['backup']).read_bytes(),original)
        self.bridge.emit_event=event
        class Child:
            def __init__(child,args,**kwargs):
                self.assertEqual(args[0],self.settings['blender_exe'])
                self.assertEqual(args[args.index('--addon')+1],self.settings['head_swap_tool'])
                self.assertEqual(Path(args[args.index('--iff')+1]).read_bytes(),Path(frozen['source']).read_bytes())
                returned=Path(args[args.index('--return-iff')+1])
                player(returned,'returned edits')
                Path(args[args.index('--signal')+1]).write_text(json.dumps({'status':'success'}))
            def wait(child): return 0
            def poll(child): return None
        with patch.object(module.subprocess,'Popen',Child):
            commands['blender-open']({**self.payload,'managedReturn':True,'handoffId':'owner'})
        self.assertTrue(any(row['state']=='return-ready' for row in events))
        with zipfile.ZipFile(self.target) as archive:
            self.assertEqual(archive.read('player.scne'),b'returned edits')
        self.assertEqual(list(self.session.glob('blender-*/handoff.json')),[])
        self.assertTrue(self.folder.exists())

    def test_proposed_launch_never_recopies_new_main_or_new_jersey(self):
        # Populate a captured jersey without needing a game archive.
        real_commands = prototype_services.commands
        jersey=self.source/'jersey.iff'
        player(jersey,'captured jersey')
        def with_jersey(*args):
            commands=real_commands(*args)
            original=commands['preview-resources']
            def resources(payload):
                value=original(payload)
                value['options']['companionPaths'].append({'name':'jersey.iff','path':str(jersey),'kind':'preview-jersey'})
                return value
            commands['preview-resources']=resources
            return commands
        with patch.object(prototype_services,'commands',with_jersey): self.prepare()
        frozen=snapshots.launch_inputs(self.payload,self.session)
        expected_main=Path(frozen['source']).read_bytes()
        expected_jersey=Path(next(row['path'] for row in frozen['resources']['options']['companionPaths'] if row['kind']=='preview-jersey')).read_bytes()
        # Safe even if a source changes immediately after host validation: copied inputs stay pinned,
        # and the unchanged return guard will reject publication to the newer target.
        player(self.target,'new target')
        player(jersey,'new jersey')
        self.settings['blender_exe']='changed-setting.exe'
        module, commands=self.wired_commands()
        class Child:
            def __init__(child,args,**kwargs):
                self.assertEqual(args[0],'configured-blender.exe')
                self.assertEqual(Path(args[args.index('--iff')+1]).read_bytes(),expected_main)
                self.assertEqual(Path(args[args.index('--jersey')+1]).read_bytes(),expected_jersey)
            def poll(child): return 0
        with patch.object(module.subprocess,'Popen',Child):
            commands['blender-open']({**self.payload,'includeJersey':True})

    def test_copy_input_refuses_unowned_or_modified_data(self):
        self.prepare()
        manifest=snapshots.launch_inputs(self.payload,self.session)
        with self.assertRaisesRegex(ValueError,'Unowned'):
            snapshots.copy_input(manifest,self.target,self.session/'foreign.iff')
        source=Path(manifest['source'])
        source.write_bytes(b'changed')
        with self.assertRaisesRegex(ValueError,'input changed'):
            snapshots.copy_input(manifest,source,self.session/'rejected.iff')

    def test_manifest_input_paths_cannot_escape_the_snapshot_lexically(self):
        foreign=self.session/'foreign.iff'
        player(foreign)
        with self.assertRaisesRegex(ValueError,'escapes its owner'):
            snapshots.regular(self.folder/'..'/'foreign.iff',self.folder)

    def test_game_archive_replacement_is_a_source_generation_change(self):
        game=self.root/'game'
        game.mkdir()
        (game/'manifest').write_text('')
        archive=game/'0A'
        archive.write_bytes(b'original archive')
        self.settings['game_root_2k27']=str(game)
        self.prepare()
        replacement=game/'replacement'
        replacement.write_bytes(b'new archive content')
        os.replace(replacement,archive)
        with self.assertRaisesRegex(ValueError,'sources or settings changed'):
            snapshots.validate(self.payload,self.bridge,self.root,self.session)

    def test_external_texture_bytes_and_binding_are_captured_without_rebinding(self):
        texture=self.source/'shared.dds'
        texture.write_bytes(b'DDS '+b'fixture pixels')
        real_commands=prototype_services.commands
        binding={'target':'eye','channel':'color'}
        def with_texture(*args):
            commands=real_commands(*args)
            original=commands['preview-resources']
            def resources(payload):
                value=original(payload)
                value['options']['externalTextures']=[{'path':str(texture),'name':'eye.dds','binding':binding,'format':'BC1_UNORM'}]
                return value
            commands['preview-resources']=resources
            return commands
        with patch.object(prototype_services,'commands',with_texture): resources=self.prepare()
        captured=resources['options']['externalTextures'][0]
        texture.write_bytes(b'DDS '+b'new pixels')
        self.assertEqual(captured['binding'],binding)
        self.assertEqual(captured['format'],'BC1_UNORM')
        self.assertEqual(Path(captured['path']).read_bytes(),b'DDS '+b'fixture pixels')
        snapshots.launch_inputs(self.payload,self.session)

    def test_hundreds_of_inactive_companions_do_not_consume_selected_file_budget(self):
        for color in range(100):
            for region in ('chest','torso','legs'):
                (self.source/f'female_{region}_color_{color:02d}.iff').write_bytes(b'unrelated body color')
        for index in range(230):
            (self.source/f'png123_config_inactive_{index}.iff').write_bytes(b'inactive configuration')
        resources=self.prepare()
        manifest=snapshots.launch_inputs(self.payload,self.session)
        self.assertEqual({Path(row['path']).name for row in manifest['files']},
                         {self.target.name,self.config.name,self.hair.name})
        self.assertEqual({row['name'] for row in resources['options']['companionPaths']},
                         {self.config.name,self.hair.name})
        snapshots.validate(self.payload,self.bridge,self.root,self.session)

    def test_unrelated_color_bytes_are_neither_hashed_nor_copied_or_budgeted(self):
        unrelated=[]
        for index in range(8):
            path=self.source/f'torso_color_{90+index:02d}.iff'
            path.write_bytes(b'x'*65536)
            unrelated.append(path.resolve())
        selected_bytes=sum(path.stat().st_size for path in (self.target,self.config,self.hair))
        real_digest=snapshots.digest
        digested=[]
        def recording_digest(path):
            digested.append(Path(path).resolve())
            return real_digest(path)
        with patch.object(snapshots,'MAX_BYTES',selected_bytes), patch.object(snapshots,'digest',recording_digest):
            self.prepare()
            manifest=snapshots.launch_inputs(self.payload,self.session)
            snapshots.validate(self.payload,self.bridge,self.root,self.session)
        self.assertEqual(sum(row['size'] for row in manifest['files']),selected_bytes)
        self.assertFalse(set(unrelated).intersection(digested))
        self.assertEqual(len(list((self.folder/'input').iterdir())),3)

    def test_only_selected_gender_skin_and_color_are_captured_with_staged_precedence(self):
        self.appearance['appearance'].update({'gender':'female','skin color':7})
        player(self.target,appearance=self.appearance)
        selected={'female_skin.iff',*(f'female_{region}_color_07.iff' for region in ('chest','torso','legs'))}
        for name in selected:
            player(self.source/name,'staged '+name)
        for name in ('male_skin.iff','child_skin.iff','chest_color_07.iff','female_chest_color_08.iff'):
            (self.source/name).write_bytes(b'unrelated')
        expected={self.target.name,self.config.name,self.hair.name,*selected}
        budget=sum((self.source/name).stat().st_size for name in expected)
        with patch.object(snapshots,'MAX_FILES',len(expected)),patch.object(snapshots,'MAX_BYTES',budget):
            resources=self.prepare()
            manifest=snapshots.launch_inputs(self.payload,self.session)
            snapshots.validate(self.payload,self.bridge,self.root,self.session)
        self.assertEqual({Path(row['path']).name for row in manifest['files']},expected)
        body_rows=[row for row in resources['options']['companionPaths'] if row['kind'].startswith('body-')]
        self.assertEqual({row['name'] for row in body_rows},selected)
        for row in body_rows:
            self.assertEqual(Path(row['path']).read_bytes(),(self.source/row['name']).read_bytes())

    def test_inactive_colors_configs_items_and_tangent_changes_do_not_retire_snapshot(self):
        inactive=('female_skin.iff','torso_color_99.iff','png123_config_inactive.iff',
                  'png123_item_unused.iff','png123_geo_hair_tangentspace.iff')
        for name in inactive:
            (self.source/name).write_bytes(b'inactive before')
        self.prepare()
        for name in inactive:
            (self.source/name).write_bytes(b'changed inactive data')
        (self.source/'female_chest_color_98.iff').write_bytes(b'new unrelated color')
        (self.source/'png123_config_new_inactive.iff').write_bytes(b'new inactive config')
        snapshots.validate(self.payload,self.bridge,self.root,self.session)

    def test_new_selected_local_overrides_are_detected_for_every_dependency_kind(self):
        self.appearance['appearance']['skin color']=7
        player(self.target,appearance=self.appearance)
        for index,name in enumerate(('male_skin.iff','chest_color_07.iff',self.config.name,
                                     'png123_item_hair.iff',self.hair.name)):
            with self.subTest(name=name):
                path=self.source/name
                path.unlink(missing_ok=True)
                if index:
                    self.folder=self.session/f'blender-snapshot-override-{index}'
                    self.folder.mkdir()
                    self.payload['_blenderSnapshot']=str(self.folder)
                self.prepare()
                player(path,'new local override')
                with self.assertRaisesRegex(ValueError,'sources or settings changed'):
                    snapshots.validate(self.payload,self.bridge,self.root,self.session)

    def test_selected_override_appearing_during_capture_rejects_and_cleans(self):
        real_commands=prototype_services.commands
        def changing_commands(*args):
            commands=real_commands(*args)
            original=commands['preview-resources']
            def resources(payload):
                value=original(payload)
                player(self.source/'png123_item_hair.iff','new selected override')
                return value
            commands['preview-resources']=resources
            return commands
        with patch.object(prototype_services,'commands',changing_commands):
            with self.assertRaisesRegex(ValueError,'sources or settings changed'):
                self.prepare()
        self.assertFalse(self.folder.exists())

    def test_selected_dependencies_still_obey_unchanged_file_and_byte_limits(self):
        selected_bytes=sum(path.stat().st_size for path in (self.target,self.config,self.hair))
        with patch.object(snapshots,'MAX_FILES',2):
            with self.assertRaisesRegex(ValueError,'Too many local'):
                self.prepare()
        with patch.object(snapshots,'MAX_BYTES',selected_bytes-1):
            with self.assertRaisesRegex(ValueError,'budget'):
                self.prepare()
        self.assertFalse((self.folder/'snapshot.json').exists())

    def test_default_configuration_is_resolved_without_capturing_inactive_custom_config(self):
        default=self.source/'png123_config_default.iff'
        player(default,'selected default configuration')
        self.payload.pop('config')
        resources=self.prepare()
        manifest=snapshots.launch_inputs(self.payload,self.session)
        self.assertEqual(resources['player']['defaultConfig'],'default')
        self.assertEqual({Path(row['path']).name for row in manifest['files']},{self.target.name,default.name})
        snapshots.validate(self.payload,self.bridge,self.root,self.session)

    def test_selected_geometry_tangent_modes_match_existing_preview_resolver(self):
        for index,mode in enumerate(('no','yes','')):
            with self.subTest(mode=mode):
                self.appearance['accessory_items']['items'][0]['tangentspace']=mode
                player(self.target,appearance=self.appearance)
                if index:
                    self.folder=self.session/f'blender-snapshot-tangent-{index}'
                    self.folder.mkdir()
                    self.payload['_blenderSnapshot']=str(self.folder)
                resources=self.prepare()
                names={row['name'] for row in resources['options']['companionPaths'] if row['kind']=='geometry'}
                expected=({self.hair.name} if mode=='no' else {'png123_geo_hair_tangentspace.iff'}
                          if mode=='yes' else {self.hair.name,'png123_geo_hair_tangentspace.iff'})
                self.assertEqual(names,expected)
                manifest=snapshots.launch_inputs(self.payload,self.session)
                self.assertEqual({Path(row['path']).name for row in manifest['files']},
                                 {self.target.name,self.config.name,*expected})
                snapshots.validate(self.payload,self.bridge,self.root,self.session)


if __name__ == '__main__':
    unittest.main()
