"""Read representative historical exports and exercise installs only on TEMP copies."""
import importlib.util
import json
import os
import shutil
import tempfile
from pathlib import Path
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('accessory_fixture', Path(__file__).with_name('accessory-install.py'))
fixtures = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixtures)
fixtures.AccessoryInstall.setUpClass()
backend = fixtures.AccessoryInstall.backend
real_fit = backend.prepare_tangent_fitted_option
report = {'cases': [], 'sourceChanges': [], 'fullNativeAcceptance': False}
try:
    for folder, png, key, kind, tangent in (
        ('workflow-parity', '6720', 'hair_dreads', 'hair', False),
        ('workflow-parity', '6720', 'facialhair_beard', 'facialhair', True),
        ('tangent-only-fxDdnA', '06572', 'hair_curly_afro', 'hair', True),
    ):
        case = fixtures.AccessoryInstall()
        case.setUp()
        source = fixtures.PROJECT / 'artifacts' / folder
        relevant = list(source.glob(f'png{png}*.iff'))
        before = {str(p): fixtures.service.digest(p) for p in relevant}
        shutil.copy2(source / f'png{png}.iff', case.target)
        option = backend.HairOption(source_geo=source / f'png{png}_geo_{key}{"_tangentspace" if tangent else ""}.iff',
            source_item=source / f'png{png}_item_{key}.iff', source_config=None,
            archive_geo=None, archive_item=None, archive_config=None,
            source_png=png, player_name='', hair_key=key, root_name='', lod_verts=None,
            radius=None, center_y=None, origin='historical-export', asset_type=kind, tangentspace=tangent)
        case.option = option
        payload = {'target': str(case.target), 'assetType': kind, 'assetId': 'representative-export', 'targetKey': key}
        old = case.hashes()
        entry = {'source': str(option.source_geo), 'assetType': kind,
                 'realRegularFitter': kind == 'hair' and not tangent}
        try:
            with patch.object(backend, 'prepare_tangent_fitted_option', real_fit):
                result = fixtures.service.install(payload, case.bridge)
            entry.update(status='installed', outputs=[Path(p).name for p in result['outputs']])
        except (ValueError, RuntimeError) as error:
            entry.update(status='rejected-before-publication', reason=str(error))
            if old != case.hashes():
                raise AssertionError('A rejected representative install changed its TEMP player package.')
        finally:
            report['sourceChanges'].extend(p for p, value in before.items() if fixtures.service.digest(Path(p)) != value)
            case.doCleanups()
        report['cases'].append(entry)
finally:
    fixtures.AccessoryInstall.tearDownClass()

output = Path(tempfile.gettempdir()) / f'CmtAccessoryInstall-assets-{os.getpid()}.json'
output.write_text(json.dumps(report, indent=2), encoding='utf-8')
print(json.dumps(report, indent=2))
print('TEMP report: ' + str(output))
raise SystemExit(1 if report['sourceChanges'] else 0)
