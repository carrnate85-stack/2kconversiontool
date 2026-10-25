import os
import sys
import tempfile
import types
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'engine'))
import prototype_services


def output(fields):
    values = {'FirstName': 'Test', 'LastName': 'Player', 'CyberfaceID': '4090',
              'HeadshotID': '100', 'PortraitID': '200', 'PlayerKey': '123',
              'Nickname': '', 'JerseyNickname': ''}
    values.update(fields)
    return '\n'.join(f'| {i} | {key} | {value or "(empty)"} |' for i, (key, value) in enumerate(values.items()))


class RosterGuards(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory(prefix='cmt-roster-guards-')
        self.root = Path(self.folder.name)
        self.environment = patch.dict(os.environ, CMT_SESSION=str(self.root))
        self.environment.start()
        bridge = types.SimpleNamespace(command_run_swap=lambda _: {}, command_health=lambda _: {}, command_health_repair=lambda _: {})
        self.commands = prototype_services.commands(bridge, Path(os.environ['CMT_TEST_ENGINE']), self.root)

    def tearDown(self):
        self.environment.stop()
        self.folder.cleanup()

    def invoke(self, request, after=None):
        before = output({})
        after = output(after if after is not None else {request['field']: request['value']})
        responses = iter([before, 'OK', after])
        def run(args, **kwargs):
            return types.SimpleNamespace(returncode=0, stdout=next(responses).encode(), stderr=b'')
        with patch.object(prototype_services.subprocess, 'run', side_effect=run) as process:
            result = self.commands['cli-set'](request)
            return result, process.call_args_list

    def test_all_games_and_writable_fields(self):
        for game in ('NBA2K25', 'NBA2K26', 'NBA2K27'):
            for field, value, previous in [('FirstName', 'New', 'Test'), ('LastName', 'Name', 'Player'),
                                          ('CyberfaceID', '065535', '04090'), ('HeadshotID', '0', '100'), ('PortraitID', '42', '200')]:
                with self.subTest(game=game, field=field):
                    result, calls = self.invoke({'game': game, 'field': field, 'value': value, 'expectedIdentity': '123', 'expectedValue': previous})
                    self.assertTrue(result['verified'])
                    normalized = str(int(value)) if field.endswith('ID') else value
                    self.assertEqual(calls[1].args[0][1:], [game, 'sp', 'set', field, normalized])
                    self.assertEqual(result['previousValue'], str(int(previous)) if field == 'CyberfaceID' else previous)

    def test_changed_identity_and_value_do_not_write(self):
        for expected in ({'expectedIdentity': 'different'}, {'expectedValue': '999'}):
            with patch.object(prototype_services.subprocess, 'run', return_value=types.SimpleNamespace(returncode=0, stdout=output({}).encode(), stderr=b'')) as process:
                with self.assertRaisesRegex(ValueError, 'changed'):
                    self.commands['cli-set']({'field': 'CyberfaceID', 'value': '500', **expected})
                self.assertEqual(process.call_count, 1)

    def test_failed_readback_is_not_reported_as_verified(self):
        with self.assertRaisesRegex(ValueError, 'verification'):
            self.invoke({'field': 'CyberfaceID', 'value': '500'}, {'CyberfaceID': '4090'})

    def test_invalid_fields_values_and_games_never_launch_cli(self):
        with patch.object(prototype_services.subprocess, 'run') as process:
            for request in [{'field': 'PlayerKey', 'value': '1'}, {'field': 'CyberfaceID', 'value': '-1'},
                            {'field': 'CyberfaceID', 'value': '65536'}, {'field': 'CyberfaceID', 'value': '\u0661'},
                            {'field': 'FirstName', 'value': 'bad\nname'}, {'field': 'FirstName', 'value': 'x' * 65},
                            {'field': 'CyberfaceID', 'value': '10', 'game': 'NBA2K18'}]:
                with self.assertRaises(ValueError):
                    self.commands['cli-set'](request)
            process.assert_not_called()


if __name__ == '__main__':
    unittest.main()
