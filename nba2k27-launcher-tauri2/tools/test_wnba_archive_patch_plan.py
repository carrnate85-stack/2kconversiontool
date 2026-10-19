import hashlib
import pathlib
import struct
import sys
import unittest
import zlib
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from wnba_archive_patch_plan import plan_ranges
from wnba_delivery_audit import decode_index


class PlanTests(unittest.TestCase):
    def setUp(self):
        self.raw = b'audited decoded roster fixture'
        self.header = b'\x1f\x8b\x21\x84' + bytes(12)
        self.original = self.header + bytes(64) + bytes(8)
        self.wrapped = self.header + bytes(8) + struct.pack('<II', zlib.crc32(self.raw), len(self.raw))
        self.record = struct.pack('<QIIQ', 123, 567, len(self.original) << 1, (444 << 23) | 0x1008)

    def call(self, record=None, original=None, wrapped=None, raw=None):
        return plan_ranges(record if record is not None else self.record,
            original if original is not None else self.original,
            wrapped if wrapped is not None else self.wrapped,
            raw if raw is not None else self.raw,
            expected_record=self.record, expected_original_sha=hashlib.sha256(self.original).hexdigest(),
            expected_decoded_sha=hashlib.sha256(self.raw).hexdigest())

    def test_plan_keeps_resource_position_flags_and_file_lengths(self):
        result = self.call()
        self.assertFalse(result['applyImplemented'])
        self.assertFalse(result['gameplayCompatibilityVerified'])
        self.assertEqual(result['payload']['slotBytes'], len(self.original))
        after = decode_index(bytes.fromhex(result['index']['replacementHex']))
        before = decode_index(self.record)
        for key in ('position', 'locationFlags', 'key', 'packedFlag'):
            self.assertEqual(after[key], before[key])
        self.assertEqual(after['decodedBytes'], len(self.raw))
        self.assertEqual(after['packedBytes'], len(self.wrapped))
        self.assertFalse(result['keyIncluded'])

    def test_changed_stock_index_payload_or_decoded_identity_refused(self):
        for kwargs in ({'record': bytes(24)}, {'original': bytes(len(self.original))},
                       {'raw': b'other decoded roster'}):
            with self.assertRaises(ValueError): self.call(**kwargs)

    def test_oversized_payload_or_bad_wrapper_authored_trailer_refused(self):
        oversized = self.header + bytes(96) + self.wrapped[-8:]
        for wrapped in (oversized, self.wrapped[:-1], self.wrapped[:-8] + bytes(8),
                        bytes(16) + self.wrapped[16:]):
            with self.assertRaises(ValueError): self.call(wrapped=wrapped)


if __name__ == '__main__':
    unittest.main()
