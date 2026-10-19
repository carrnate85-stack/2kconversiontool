"""Offline fake-memory tests only; never opens a live process."""
import pathlib
import struct
import unittest
from wnba_priority_loaded_roster_probe import capture, inventory, parse_lease, TEAM_STRIDE


class LoadedRosterProbeTests(unittest.TestCase):
    def memory(self):
        base, root, owner, table = 0x140000000, 0x200000, 0x300000, 0x400000
        teams = tuple([(i, 0) for i in range(30)] + [(i, 29) for i in list(range(300, 312)) + [315, 316, 317]])
        values = {base + 0xAD07BD0: 2, base + 0xAD07BCC: 3,
                  base + 0xAD07408 + 2 * 64: 0xF54D5D12,
                  base + 0xAD07408 + 3 * 64: 0xF54D5D12,
                  base + 0xB1F9050: base + 0x7B6B768,
                  base + 0x9ACE338: root, root + 0xA8: owner,
                  owner + 0x80: len(teams), owner + 0x88: table}
        for offset, value in zip((0, 12, 16, 20), (6, 1, 1, 4)):
            values[base + 0xB1EA768 + offset] = value
        for i, (team_id, kind) in enumerate(teams):
            values[table + i * TEAM_STRIDE + 0xF20] = team_id | (0xABCD << 16)
            values[table + i * TEAM_STRIDE + 0x123C] = kind << 26
        return base, values, teams

    def test_two_dword_fields_only_and_inventory(self):
        base, values, teams = self.memory()
        calls = []
        def read(at, size):
            calls.append((at, size))
            self.assertIn(size, (4, 8))
            return values[at]
        result = capture(read, base)
        self.assertEqual(result[1], teams)
        self.assertTrue(inventory(teams)['matches30Nba15Wnba'])
        self.assertEqual(sum(at >= 0x400000 and at < 0x500000 for at, _ in calls), 90)

    def test_blacktop_requires_its_own_inactive_selection_boundary(self):
        base, values, teams = self.memory()
        for i in (2, 3):
            values[base + 0xAD07408 + i * 64] = 0x92DF06DE
        values.update({base + 0xAF46870: base + 0x791E158,
                       base + 0xAF46870 + 0xEA8: 3, base + 0xAB5CE20: 0x80000,
                       base + 0xAB5D790: 0, 0x80000: 0})
        read = lambda at, size: values[at]
        self.assertEqual(capture(read, base, True)[1], teams)
        for key, value in [(base + 0xAF46870 + 0xEA8, 1),
                           (base + 0xAB5D790, 1), (0x80000, 1)]:
            original = values[key]
            values[key] = value
            with self.assertRaises(ValueError):
                capture(read, base, True)
            values[key] = original

    def test_mode_picker_count_guards(self):
        for key, value in [(0xAD07BD0, 128), (0xB1F9050, 0)]:
            base, values, _ = self.memory()
            values[base + key] = value
            with self.assertRaises(ValueError):
                capture(lambda at, size: values[at], base)
        base, values, _ = self.memory()
        values[0x300000 + 0x80] = 1025
        with self.assertRaises(ValueError):
            capture(lambda at, size: values[at], base)

    def test_changed_header_discarded(self):
        base, values, _ = self.memory()
        reads = 0
        def read(at, size):
            nonlocal reads
            if at == base + 0x9ACE338:
                reads += 1
                if reads > 1:
                    return 0
            return values[at]
        with self.assertRaises(ValueError):
            capture(read, base)

    def test_duplicate_club_rejected_in_inventory(self):
        _, _, teams = self.memory()
        altered = teams[:-1] + ((300, 29),)
        self.assertFalse(inventory(altered)['matches30Nba15Wnba'])
        self.assertTrue(inventory(altered)['duplicateWnbaClubIds'])

    def test_exact_lease_nonce(self):
        nonce = 'a' * 48
        path = pathlib.Path(f'blacktop-outfit-v2-{nonce}.lease')
        data = bytearray(1632)
        data[:8] = b'2KOV2L01'
        struct.pack_into('<I', data, 8, 123)
        struct.pack_into('<Q', data, 24, 456)
        data[48:96] = nonce.encode()
        game = r'C:\Games\NBA2K27.exe'.encode('utf-16le')
        data[96:96 + len(game)] = game
        self.assertEqual(parse_lease(path, bytes(data))[:2], (123, 456))
        data[48] ^= 1
        with self.assertRaises(ValueError):
            parse_lease(path, bytes(data))


if __name__ == '__main__':
    unittest.main()
