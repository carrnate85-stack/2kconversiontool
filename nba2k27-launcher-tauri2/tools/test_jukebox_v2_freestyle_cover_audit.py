import os
import pathlib
import struct
import unittest

import jukebox_v2_freestyle_cover_audit as audit

BASE = 0x140000000


def synthetic_image(sections) -> audit.Image:
    """PE with (name, va, vsize, raw bytes) sections; raw data packed after 0x400."""
    head = bytearray(0x400)
    head[0:2] = b"MZ"
    struct.pack_into("<I", head, 0x3C, 0x80)
    head[0x80:0x84] = b"PE\0\0"
    struct.pack_into("<HH", head, 0x84, 0x8664, len(sections))
    struct.pack_into("<I", head, 0x88, audit.EXPECTED_STAMP)
    struct.pack_into("<H", head, 0x94, 0xF0)
    struct.pack_into("<Q", head, 0x98 + 24, BASE)
    struct.pack_into("<I", head, 0x98 + 56, audit.EXPECTED_IMAGE_SIZE)
    body, raw = bytearray(), 0x400
    for i, (name, va, vsize, data) in enumerate(sections):
        o = 0x98 + 0xF0 + 40 * i
        head[o : o + 8] = name.encode().ljust(8, b"\0")
        struct.pack_into("<IIII", head, o + 8, vsize, va, len(data), raw + len(body))
        body += data
    return audit.Image(bytes(head + body))


def lea(at, prefix, target):
    return prefix + struct.pack("<i", target - (at + len(prefix) + 4))


class Crc(unittest.TestCase):
    def test_crc64_xz_check_value(self):
        self.assertEqual(audit.archive_key("123456789"), 0x995DC9BBDF1939FA)

    def test_key_is_case_insensitive(self):
        self.assertEqual(audit.archive_key("newaudio/x.wav"), audit.archive_key("NEWAUDIO/X.WAV"))

    def test_audio_path_grammar_matches_native(self):
        self.assertEqual(audit.audio_path("eventmusic", 4400),
                         "newaudio/eventmusic/004400-000000-000000-000000-000000-000000.wav")

    def test_track_list_matches_native_header(self):
        self.assertEqual(len(audit.FREESTYLE_TRACKS), 58)
        self.assertEqual(len(set(audit.FREESTYLE_TRACKS)), 58)
        header = pathlib.Path(__file__).resolve().parents[2] / "nba2k27-version-proxy/src/freestyle_audio.hpp"
        if header.exists():
            text = header.read_text()
            body = text[text.index("kFreestyleTracks={") + 18 : text.index("};", text.index("kFreestyleTracks"))]
            self.assertEqual(tuple(int(x) for x in body.replace("\n", "").split(",")), audit.FREESTYLE_TRACKS)


class Rip(unittest.TestCase):
    def setUp(self):
        text = bytearray(0x400)
        text[0x10:0x17] = lea(0x1010, b"\x48\x8d\x0d", 0x5000)
        text[0x20:0x2B] = lea(0x1020, b"\xc7\x05", 0x514C - 4) + struct.pack("<I", 0x22)
        text[0x30:0x35] = b"\xe8" + struct.pack("<i", 0x1100 - 0x1035)
        self.image = synthetic_image([(".text", 0x1000, 0x400, bytes(text)),
                                      (".data", 0x4000, 0x2000, bytes(0x200))])

    def test_lea_target(self):
        self.assertEqual(audit.rip_target(self.image, 0x1010, b"\x48\x8d\x0d"), 0x5000)

    def test_wrong_prefix_is_rejected(self):
        self.assertIsNone(audit.rip_target(self.image, 0x1010, b"\x48\x8d\x15"))

    def test_immediate_store_target_accounts_for_trailing_imm(self):
        self.assertEqual(audit.rip_target(self.image, 0x1020, b"\xc7\x05", 4), 0x514C)

    def test_branch(self):
        self.assertEqual(audit.branch_target(self.image, 0x1030), 0x1100)
        self.assertIsNone(audit.branch_target(self.image, 0x1010))

    def test_bss_is_not_file_backed(self):
        self.assertTrue(self.image.file_backed(0x4100))
        self.assertFalse(self.image.file_backed(0x5000))
        self.assertEqual(self.image.section_of(0x5000)[0], ".data")
        with self.assertRaises(ValueError):
            self.image.read(0x5000, 4)


class Header(unittest.TestCase):
    def test_cover_header_is_the_rust_codec_header(self):
        rust = bytearray(32)
        rust[:4] = b"TLD "
        rust[8:12] = bytes([5, 33, 71, 1])
        rust[12:16] = (31).to_bytes(4, "little")
        rust[16:24] = bytes([0, 1, 0, 1, 1, 0, 1, 0])
        rust[24:28] = (32768).to_bytes(4, "little")
        self.assertEqual(bytes(rust[:28]), audit.COVER_HEADER)

    def test_cover_grammar(self):
        self.assertTrue(audit.COVER_LOGICAL.match("shared/13/001_helicopter.13b451cfdca9dda2.tld"))
        self.assertFalse(audit.COVER_LOGICAL.match("shared/13/helicopter.13b451cfdca9dda2.tld"))
        self.assertFalse(audit.COVER_LOGICAL.match("shared/13/001_x.13b451cfdca9dda.tld"))


@unittest.skipUnless(os.environ.get("NBA2K27_ACTUAL_DISK_AUDIT") == "1", "set NBA2K27_ACTUAL_DISK_AUDIT=1")
class ActualDisk(unittest.TestCase):
    def test_installed_version2_passes(self):
        self.assertEqual(audit.main(["--skip-hash"]), 0)


if __name__ == "__main__":
    unittest.main()
