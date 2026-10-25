import json
import math
import struct
import sys
import os
import types
import tempfile
import unittest
from unittest.mock import patch
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT / 'engine'), str(ROOT.parent / 'character-mod-tool-webview2/dist/Character-Mod-Tool-WebView2-0.1.31/App/Backend')]
from prototype_seams import seam_plan, close_seams


def fixture(gap=.3):
    positions, regions = [], {}
    for name, heights in [('face_shader', (50, 75)), ('arms_shader', (50-gap, 30)), ('torso_shader', (30-gap, 0))]:
        start = len(positions)
        for y in heights:
            positions.extend((10*math.cos(i*math.tau/12), y, 10*math.sin(i*math.tau/12)) for i in range(12))
        regions[name] = [(start+i, start+(i+1)%12, start+12+i) for i in range(12)] + [(start+(i+1)%12, start+12+(i+1)%12, start+12+i) for i in range(12)]
    positions.append((0, -100, 0))
    return positions, regions


def package(path, positions, regions):
    indices, prim = [], []
    for name, tris in regions.items():
        prim.append({'Material': name, 'Start':len(indices), 'Count':len(tris)*3})
        indices.extend(i for t in tris for i in t)
    model = {'Lods':[{'LodVerts':len(positions)}], 'Prim':prim,
             'VertexFormat':{'POSITION0':{'Format':'R32G32B32_FLOAT','Stream':0}, 'TEXCOORD0':{'Format':'R32G32_FLOAT','Stream':1}},
             'VertexStream':[{'Binary':'positions.bin','Stride':12,'Size':len(positions)*12}, {'Binary':'uv.bin','Stride':8,'Size':len(positions)*8}],
             'IndexBuffer':{'Format':'R16_UINT','Binary':'indices.bin','Size':len(indices)*2}}
    with zipfile.ZipFile(path, 'x', compression=zipfile.ZIP_DEFLATED) as archive:
        archive.comment = b'preserve me'
        archive.writestr('hihead.scne',json.dumps({'scene':{'Model':{'body':model}}}))
        archive.writestr('positions.bin',b''.join(struct.pack('<fff',*p) for p in positions))
        archive.writestr('uv.bin',b''.join(struct.pack('<ff',i/len(positions),.5) for i in range(len(positions))))
        archive.writestr('indices.bin',struct.pack('<'+'H'*len(indices),*indices))
        archive.writestr('untouched-morph.bin',b'morph/rig/texture payload must survive exactly')


class SeamTests(unittest.TestCase):
    def test_closes_two_strips_and_preserves_other_vertices(self):
        positions, regions = fixture()
        changes,pairs,report = seam_plan(positions,regions)
        self.assertEqual(len(pairs),24)
        self.assertEqual(len(changes),36)
        self.assertNotIn(12,changes)
        self.assertNotIn(24,changes)
        self.assertNotIn(72,changes)
        self.assertAlmostEqual(report['maxVertexMove'],.3)
        self.assertAlmostEqual(report['maxGapBefore'],.3)
        for aa,bb in pairs:
            self.assertEqual(changes.get(aa[0],positions[aa[0]]),changes.get(bb[0],positions[bb[0]]))

    def test_medium_nearby_border_is_not_guessed(self):
        positions,regions=fixture(1.2)
        self.assertFalse(seam_plan(positions,regions)[2]['applied'])

    def test_closed_is_noop(self):
        positions,regions=fixture(0)
        self.assertFalse(seam_plan(positions,regions)[2]['applied'])

    def test_transitive_nearby_borders_do_not_chain(self):
        positions,regions=[],{}
        for name,heights in [('face_shader',(50,75)),('arms_shader',(48.5,30)),('torso_shader',(47,0)),('chest_shader',(45.5,20))]:
            start=len(positions)
            for y in heights:
                positions.extend((10*math.cos(i*math.tau/12),y,10*math.sin(i*math.tau/12)) for i in range(12))
            regions[name]=[t for i in range(12) for t in ((start+i,start+(i+1)%12,start+12+i),(start+(i+1)%12,start+12+(i+1)%12,start+12+i))]
        positions.append((0,-100,0))
        self.assertFalse(seam_plan(positions,regions)[2]['applied'])

    def test_large_gap_is_not_guessed(self):
        positions,regions=fixture(10)
        self.assertFalse(seam_plan(positions,regions)[2]['applied'])
        self.assertTrue(seam_plan(positions,regions)[2]['warnings'])

    def test_eyes_mouth_and_hair_are_never_candidates(self):
        positions,regions=fixture()
        positions.extend([(0,49.9,10),(.1,49.9,10),(0,50,10)])
        for name in ('l_eye_shader','mouth_shader','hair_shader'):
            regions[name]=[(73,74,75)]
        changes,_,_=seam_plan(positions,regions)
        self.assertTrue(all(i not in changes for i in (73,74,75)))

    def test_nonfinite_and_wrong_scale_rejected(self):
        positions,regions=fixture()
        with self.assertRaises(ValueError): seam_plan([(x/100,y/100,z/100) for x,y,z in positions],regions)
        positions[0]=(math.nan,1,2)
        with self.assertRaises(ValueError): seam_plan(positions,regions)

    def test_isolated_matches_are_not_seams(self):
        positions,regions=fixture()
        regions['face_shader']=regions['face_shader'][:1]
        regions.pop('torso_shader')
        self.assertFalse(seam_plan(positions,regions)[2]['applied'])

    def test_face_seam_preserves_target_body_border(self):
        positions,regions=fixture()
        original=list(positions)
        changes,pairs,_=seam_plan(positions,regions)
        face_arm=[(aa,bb) for aa,bb in pairs if aa[0] < 24 and 24 <= bb[0] < 48]
        self.assertTrue(face_arm)
        for aa,bb in face_arm:
            self.assertTrue(all(i not in changes for i in bb))
            self.assertTrue(all(changes[i] == original[bb[0]] for i in aa))

    def test_archive_changes_only_position_bytes_and_is_idempotent(self):
        positions,regions=fixture()
        with tempfile.TemporaryDirectory() as folder:
            original,fixed,again=(Path(folder)/name for name in ('original.iff','fixed.iff','again.iff'))
            package(original,positions,regions)
            report=close_seams(original,fixed)
            self.assertTrue(report['applied'])
            self.assertEqual(report['maxGapAfter'],0)
            with zipfile.ZipFile(original) as a,zipfile.ZipFile(fixed) as b:
                self.assertEqual(a.namelist(),b.namelist())
                self.assertEqual(a.comment,b.comment)
                for name in a.namelist():
                    self.assertEqual(len(a.read(name)),len(b.read(name)))
                    if name!='positions.bin': self.assertEqual(a.read(name),b.read(name))
            self.assertFalse(close_seams(fixed,again)['applied'])
            self.assertFalse(again.exists())

    def test_output_is_never_overwritten(self):
        with tempfile.TemporaryDirectory() as folder:
            source,output=(Path(folder)/name for name in ('source.iff','existing.iff'))
            package(source,*fixture())
            output.write_bytes(b'keep')
            with self.assertRaises(FileExistsError): close_seams(source,output)
            self.assertEqual(output.read_bytes(),b'keep')

    def test_service_keeps_original_backup_and_noop_does_not_add_backups(self):
        from prototype_services import commands
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)
            target=root/'player.iff'
            package(target,*fixture())
            original=target.read_bytes()
            bridge=types.SimpleNamespace(command_run_swap=lambda _: {},command_health=lambda _: {},command_health_repair=lambda _: {})
            with patch.dict(os.environ,CMT_SESSION=folder):
                run=commands(bridge,root,root)['preview-close-seams']
                report=run({'target':str(target)})
                self.assertTrue(report['applied'])
                self.assertEqual(Path(report['backup']).read_bytes(),original)
                corrected=target.read_bytes()
                self.assertFalse(run({'target':str(target)})['applied'])
                self.assertEqual(target.read_bytes(),corrected)
                self.assertEqual(len(list((root/'Player Backups/seams').glob('*'))),1)

    def test_publication_failure_preserves_target(self):
        from prototype_services import commands
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)
            target=root/'player.iff'
            package(target,*fixture())
            original=target.read_bytes()
            bridge=types.SimpleNamespace(command_run_swap=lambda _: {},command_health=lambda _: {},command_health_repair=lambda _: {})
            with patch.dict(os.environ,CMT_SESSION=folder),patch('prototype_player_writes.publish_player',side_effect=PermissionError('locked')):
                with self.assertRaises(PermissionError): commands(bridge,root,root)['preview-close-seams']({'target':str(target)})
            self.assertEqual(target.read_bytes(),original)

    def test_full_swap_automatically_repairs_final_output_before_returning(self):
        from prototype_services import commands
        for game in ('NBA 2K23','NBA 2K25','NBA 2K26'):
            with self.subTest(game=game), tempfile.TemporaryDirectory() as folder:
                root=Path(folder)
                target=root/'converted.iff'
                events=[]
                def swap(_):
                    package(target,*fixture())
                    return {'output':str(target),'validation':{'errors':0}}
                bridge=types.SimpleNamespace(command_run_swap=swap,command_health=lambda _: {},command_health_repair=lambda _: {},emit_event=lambda *args:events.append(args))
                with patch.dict(os.environ,CMT_SESSION=folder):
                    result=commands(bridge,root,root)['run-swap']({'sourceGame':game,'targetGame':'NBA 2K27'})
                    self.assertTrue(result['automaticSeamCorrection']['applied'])
                    self.assertTrue(Path(result['automaticSeamCorrection']['backup']).is_file())
                    self.assertFalse(close_seams(target,root/'again.iff')['applied'])
                    self.assertTrue(any('vertex count unchanged' in args[1] for args in events))

    def test_automatic_seam_failure_is_visible_and_keeps_conversion(self):
        from prototype_services import commands
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)
            target=root/'converted.iff'
            package(target,*fixture())
            original=target.read_bytes()
            events=[]
            bridge=types.SimpleNamespace(command_run_swap=lambda _: {'output':{'mainPath':str(target)}},command_health=lambda _: {},command_health_repair=lambda _: {},emit_event=lambda *args:events.append(args))
            with patch.dict(os.environ,CMT_SESSION=folder),patch('prototype_player_writes.publish_player',side_effect=PermissionError('locked')):
                result=commands(bridge,root,root)['run-swap']({'sourceGame':'NBA 2K23','targetGame':'NBA 2K27'})
            self.assertTrue(result['automaticSeamCorrection']['skipped'])
            self.assertEqual(target.read_bytes(),original)
            self.assertTrue(any(args[0]=='warning' and 'locked' in args[1] for args in events))

    def test_other_target_and_failed_conversion_do_not_run_seam_pass(self):
        from prototype_services import commands
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)
            bridge=types.SimpleNamespace(command_run_swap=lambda _: {'output':'unchanged'},command_health=lambda _: {},command_health_repair=lambda _: {})
            with patch.dict(os.environ,CMT_SESSION=folder),patch('prototype_seams.close_seams',side_effect=AssertionError('must not run')):
                self.assertEqual(commands(bridge,root,root)['run-swap']({'targetGame':'NBA 2K26'}),{'output':'unchanged'})
                def fail(_): raise RuntimeError('conversion failed')
                bridge.command_run_swap=fail
                with self.assertRaisesRegex(RuntimeError,'conversion failed'):
                    commands(bridge,root,root)['run-swap']({'targetGame':'NBA 2K27'})

    def test_automatic_pass_does_not_duplicate_backup_for_closed_player(self):
        from prototype_services import commands
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)
            target=root/'converted.iff'
            package(target,*fixture(0))
            before=target.read_bytes()
            bridge=types.SimpleNamespace(command_run_swap=lambda _: {'output':str(target)},command_health=lambda _: {},command_health_repair=lambda _: {},emit_event=lambda *args:None)
            with patch.dict(os.environ,CMT_SESSION=folder):
                result=commands(bridge,root,root)['run-swap']({'sourceGame':'NBA 2K23','targetGame':'NBA 2K27'})
            self.assertFalse(result['automaticSeamCorrection']['applied'])
            self.assertEqual(target.read_bytes(),before)
            self.assertFalse((root/'Player Backups').exists())

    def test_browsed_target_uses_converter_game_detection(self):
        from prototype_services import commands
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)
            target=root/'converted.iff'
            package(target,*fixture())
            detections=[]
            def detect(path):
                detections.append(path)
                return 'NBA 2K27'
            bridge=types.SimpleNamespace(command_run_swap=lambda _: {'output':str(target)},detect_modern_target_game=detect,
                                         command_health=lambda _: {},command_health_repair=lambda _: {},emit_event=lambda *args:None)
            with patch.dict(os.environ,CMT_SESSION=folder):
                result=commands(bridge,root,root)['run-swap']({'sourceGame':'NBA 2K23','target':str(target),'targetGame':''})
            self.assertEqual(detections,[str(target)])
            self.assertTrue(result['automaticSeamCorrection']['applied'])


if __name__=='__main__': unittest.main()
