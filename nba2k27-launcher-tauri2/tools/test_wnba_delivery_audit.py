import struct
import unittest
import pathlib
import sys
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from wnba_delivery_audit import decode_index


class IndexTests(unittest.TestCase):
    def test_provenance_record(self):
        record = bytes.fromhex('55c65c9b516d3f67e01f1901c0c935001008210ba0ce2000')
        result = decode_index(record)
        self.assertEqual(result['key'], '673F6D519B5CC655')
        self.assertEqual(result['decodedBytes'], 18423776)
        self.assertEqual(result['packedBytes'], 1762528)
        self.assertEqual(result['position'], 1100824598)
        self.assertEqual(result['packedFlag'], 0)

    def test_size_and_location_flags_are_not_offsets(self):
        r = decode_index(struct.pack('<QIIQ', 1, 2, (3 << 1) | 1, (400 << 23) | 0x1008))
        self.assertEqual((r['packedBytes'], r['packedFlag']), (3, 1))
        self.assertEqual((r['position'], r['locationFlags']), (400, 0x1008))

    def test_short_or_trailing_records_refused(self):
        for size in (0, 23, 25):
            with self.assertRaises(ValueError):
                decode_index(bytes(size))


if __name__ == '__main__':
    unittest.main()
