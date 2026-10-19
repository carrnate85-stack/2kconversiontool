import struct
import unittest
import zlib

from roster_wrapper_offline import decode_wrapper, inspect


def wrapper(payload, output, flags=4):
    return (bytes.fromhex('1f8b21') + bytes([flags]) + bytes.fromhex('0000000000ff040056435a00')
            + payload + struct.pack('<II', zlib.crc32(output), len(output)))


class WrapperTests(unittest.TestCase):
    def test_plain_crc_length(self):
        result, info = decode_wrapper(wrapper(b'packed', b'valid'), lambda p, n: b'valid')
        self.assertEqual(result, b'valid')
        self.assertFalse(info['encrypted'])

    def test_encrypted_without_key_never_calls_decoder(self):
        def forbidden(*args):
            self.fail('Encrypted bytes reached the decoder without a key')
        with self.assertRaisesRegex(ValueError, 'requires.*key'):
            decode_wrapper(wrapper(bytes(8), b'valid', 0x84), forbidden)

    def test_blowfish_known_answer_and_round_trip(self):
        # Standard zero-key ECB known answer: decrypt to eight zero bytes.
        payload = bytes.fromhex('4ef997456198dd78')
        def verify(payload, size):
            self.assertEqual(payload, bytes(8))
            return b'verified'
        self.assertEqual(decode_wrapper(wrapper(payload, b'verified', 0x84), verify, bytes(8))[0], b'verified')

    def test_invalid_keys(self):
        for size in [0, 4, 9, 57, 64, 128]:
            with self.subTest(size=size), self.assertRaises(ValueError):
                decode_wrapper(wrapper(bytes(8), b'valid', 0x84), lambda *a: self.fail(), bytes(size))

    def test_wrong_key_cannot_pass_crc(self):
        with self.assertRaisesRegex(ValueError, 'CRC'):
            decode_wrapper(wrapper(bytes.fromhex('4ef997456198dd78'), bytes(8), 0x84),
                           lambda p, n: p, bytes([1]) * 8)

    def test_wrong_length(self):
        with self.assertRaisesRegex(ValueError, 'length'):
            decode_wrapper(wrapper(b'packed', b'valid'), lambda *a: b'shorter!')

    def test_wrong_crc(self):
        with self.assertRaisesRegex(ValueError, 'CRC'):
            decode_wrapper(wrapper(b'packed', b'valid'), lambda *a: b'other')

    def test_unaligned_ciphertext(self):
        with self.assertRaisesRegex(ValueError, 'aligned'):
            inspect(wrapper(bytes(9), b'valid', 0x84))

    def test_bad_headers(self):
        valid = wrapper(bytes(8), b'valid')
        for data in [b'', valid[:20], b'bad' + valid[3:], valid[:3] + b'\x85' + valid[4:],
                     valid[:12] + b'BAD!' + valid[16:]]:
            with self.subTest(data=data), self.assertRaises(ValueError):
                inspect(data)

    def test_size_bounds(self):
        for size in (0, 64 * 1024 * 1024 + 1):
            data = wrapper(bytes(8), b'valid')[:-4] + struct.pack('<I', size)
            with self.subTest(size=size), self.assertRaises(ValueError):
                inspect(data)

    def test_plain_refuses_key(self):
        with self.assertRaisesRegex(ValueError, 'unencrypted'):
            decode_wrapper(wrapper(bytes(8), b'valid'), lambda *a: self.fail(), bytes(8))


if __name__ == '__main__':
    unittest.main()
