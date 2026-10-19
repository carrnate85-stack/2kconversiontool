"""Structural refusal fixtures only; no actual salt or key access."""
import io
import struct
import unittest
import zipfile

from prepare_wnba_full_iff_research import index_span, layout


class LayoutTests(unittest.TestCase):
    @staticmethod
    def ordinary():
        b = io.BytesIO()
        with zipfile.ZipFile(b, 'w', compression=zipfile.ZIP_STORED) as z:
            z.writestr('roster.RDAT', b'example')
        return b.getvalue()

    def test_strict_zip(self):
        original = self.ordinary()
        start, central, data = layout(original)
        self.assertEqual(data, b'example')
        for offset, value, fmt in [(6, 8, '<H'), (8, 8, '<H'), (28, 1, '<H'),
                                    (central + 8, 8, '<H'), (central + 42, 1, '<I')]:
            bad = bytearray(original)
            struct.pack_into(fmt, bad, offset, value)
            with self.assertRaises(AssertionError):
                layout(bad)
        bad = bytearray(original)
        bad[start] ^= 1
        with self.assertRaises(AssertionError):
            layout(bad)
        with self.assertRaises(AssertionError):
            layout(original + b'junk')

    def test_index_crc_exact_range(self):
        h = [0] * 13
        h[0], h[1], h[5], h[6] = 0x222E3502, 37468, 0, 37468
        # Align full BNDL extent to eight bytes, with bounded zero padding.
        size = ((60 + 37468 + 7) & ~7) - 60
        rdat = bytes(40) + b'BNDL' + struct.pack('<I', size) + struct.pack('<13I', *h) + bytes(size)
        self.assertEqual(index_span(rdat)[1], 100)
        for word, value in [(5, 1), (6, 37467), (1, 37467)]:
            bad = bytearray(rdat)
            struct.pack_into('<I', bad, 48 + 4 * word, value)
            with self.assertRaises(AssertionError):
                index_span(bad)
        with self.assertRaises(AssertionError):
            index_span(rdat[:-1])
        with self.assertRaises(AssertionError):
            index_span(rdat + rdat[40:])


if __name__ == '__main__':
    unittest.main()
