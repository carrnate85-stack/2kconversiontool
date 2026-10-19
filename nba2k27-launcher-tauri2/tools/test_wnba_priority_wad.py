"""Offline-only native WAD layout tests; synthetic payload metadata only."""
import struct
import unittest

from wnba_priority_wad import (build_manifest, parse_manifest,
                               build_two_alias_manifest, parse_two_alias_manifest)


class PriorityWadTests(unittest.TestCase):
    def setUp(self):
        self.fields = dict(key=0x123456789ABCDEF0, decoded_size=10000,
                           packed_size=800, location_flags=0x210810,
                           payload_name='wnba-roster.bin', packed_flag=0)

    def test_native_layout_and_round_trip(self):
        data = build_manifest(**self.fields)
        self.assertEqual(len(data), 120)
        self.assertEqual(struct.unpack_from('<Q', data, 8)[0], 1)
        self.assertEqual(struct.unpack_from('<Q', data, 48)[0], 800)
        self.assertEqual(data[64:80], b'wnba-roster.bin\0')
        self.assertEqual(struct.unpack_from('<Q', data, 104)[0] >> 33, 800)
        self.assertEqual(struct.unpack_from('<Q', data, 104)[0] & ((1 << 33) - 1), 10000)
        self.assertEqual(struct.unpack_from('<Q', data, 112)[0] >> 23, 0)
        self.assertEqual(parse_manifest(data), self.fields)

    def test_flags_preserved(self):
        for flag in (0, 1):
            fields = {**self.fields, 'packed_flag': flag, 'location_flags': 0x7fffff}
            self.assertEqual(parse_manifest(build_manifest(**fields)), fields)

    def test_unsafe_names_rejected(self):
        for name in ('', '.', '..', '../0A', 'C:\\0A', 'a/b', 'x' * 28,
                     'a\0b', 'a.', 'a ', 'é'):
            with self.subTest(name=name), self.assertRaises(ValueError):
                build_manifest(**{**self.fields, 'payload_name': name})

    def test_integer_bounds_rejected(self):
        for key, value in [('key', -1), ('decoded_size', 0), ('packed_size', 1 << 31),
                           ('location_flags', 1 << 23), ('packed_flag', 2), ('packed_size', True)]:
            with self.subTest(key=key), self.assertRaises(ValueError):
                build_manifest(**{**self.fields, key: value})

    def test_corruption_rejected(self):
        original = build_manifest(**self.fields)
        for offset in (0, 4, 8, 16, 20, 32, 40, 48, 56, 91, 92, 115):
            data = bytearray(original)
            data[offset] ^= 1
            with self.subTest(offset=offset), self.assertRaises(ValueError):
                parse_manifest(data)
        for data in (original[:-1], original + b'\0'):
            with self.assertRaises(ValueError):
                parse_manifest(data)

    def test_two_aliases_are_sorted_and_share_one_payload(self):
        fields = {**self.fields, 'alias_key': 0xFEDCBA9876543210}
        data = build_two_alias_manifest(**fields)
        self.assertEqual(len(data), 144)
        self.assertEqual(parse_two_alias_manifest(data), fields)
        self.assertEqual(data[104:120], data[128:144])
        self.assertEqual(struct.unpack_from('<I', data, 16)[0], 1)
        self.assertEqual(struct.unpack_from('<I', data, 32)[0], 2)
        reverse = {**self.fields, 'key': fields['alias_key'], 'alias_key': fields['key']}
        self.assertEqual(build_two_alias_manifest(**reverse), data)
        with self.assertRaises(ValueError):
            parse_manifest(data)

    def test_alias_corruption_duplicates_and_different_payloads_rejected(self):
        data = build_two_alias_manifest(**self.fields, alias_key=0xFEDCBA9876543210)
        for offset in (0, 8, 16, 32, 48, 92, 104, 128, 143):
            changed = bytearray(data)
            changed[offset] ^= 1
            with self.subTest(offset=offset), self.assertRaises(ValueError):
                parse_two_alias_manifest(changed)
        for key in (self.fields['key'], -1, 1 << 64, True):
            with self.subTest(key=key), self.assertRaises(ValueError):
                build_two_alias_manifest(**self.fields, alias_key=key)
        for changed in (data[:-1], data + b'\0', data[:96] + data[120:] + data[96:120]):
            with self.assertRaises(ValueError):
                parse_two_alias_manifest(changed)


if __name__ == '__main__':
    unittest.main()
