import io
import os
import pathlib
import re
import struct
import unittest
import zipfile

import jukebox_v2_cover_route_audit as audit

ROOT = pathlib.Path(__file__).resolve().parents[1]


def synthetic_txtr(name: str, hash16: str, compression=0x21, width=256) -> bytes:
    """Same shape the stock TXTR uses: magic, table size, CRC-32(name), typed records, strings."""
    leaf = f"{name}.{hash16}.tld"
    strings = b"\0".join([name.encode(), *[f.encode() for f in audit.FIELDS], b"BC1_UNORM", leaf.encode()]) + b"\0"
    values = {"Width": width, "Height": 256, "PixelDataSize": 32768, "Twiddled": 1,
              "CompressionMethod": compression}
    body = bytearray(0x30)
    body[0:4] = audit.TXTR_MAGIC
    struct.pack_into("<II", body, 4, 1, len(strings))
    struct.pack_into("<I", body, 0x18, audit.crc32(name.encode()))
    for field in audit.FIELDS:
        kind = b"\x06\x00\x00\x40" if field in values else b"\x03\x00\x00\x40"
        record = bytes(8) + struct.pack("<I", audit.crc32(field.encode())) + kind + bytes(8)
        record += struct.pack("<Q", values.get(field, 0))
        body += record
    return bytes(body) + strings


def stored_zip(members) -> bytes:
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_STORED) as z:
        for name, data in members:
            z.writestr(name, data)
    return buf.getvalue()


def gradient() -> bytes:
    return bytes(v for y in range(256) for x in range(256) for v in (x, y, (x + y) // 2))


class Hashes(unittest.TestCase):
    def test_container_name_is_crc32_of_context(self):
        self.assertEqual(audit.crc32(b"jukebox"), 0xFCB780E2)
        self.assertEqual(audit.container_name("jukebox"), "alb_cvr_cont_fcb780e2.iff")

    def test_crc64_xz_check_value(self):
        self.assertEqual(audit.crc64_xz(b"123456789"), 0x995DC9BBDF1939FA)

    def test_leaf_and_folder_follow_crc64(self):
        data = b"TLD example bytes"
        leaf = audit.tld_leaf("001_helicopter", data)
        h = f"{audit.crc64_xz(data):016x}"
        self.assertEqual(leaf, f"001_helicopter.{h}.tld")
        self.assertEqual(audit.shared_path(leaf), f"shared/{h[:2]}/{leaf}")

    def test_shared_path_rejects_bad_leaves(self):
        for bad in ("x.tld", "a/b.0123456789abcdef.tld", "x.0123456789ABCDEF.tld", "x.0123456789abcdef.dds"):
            with self.assertRaises(ValueError):
                audit.shared_path(bad)


class Txtr(unittest.TestCase):
    def setUp(self):
        self.data = synthetic_txtr("001_helicopter", "13b451cfdca9dda2")

    def test_parse_reads_name_leaf_and_fields(self):
        info = audit.txtr_parse(self.data)
        self.assertEqual(info["name"], "001_helicopter")
        self.assertEqual(info["leaf"], "001_helicopter.13b451cfdca9dda2.tld")
        self.assertEqual(info["fields"], {"Width": 256, "Height": 256, "PixelDataSize": 32768,
                                          "Twiddled": 1, "CompressionMethod": 0x21})

    def test_retarget_changes_only_the_16_hex_digits(self):
        out = audit.txtr_retarget(self.data, "711a66d148d9d178")
        diff = [i for i, (a, b) in enumerate(zip(self.data, out)) if a != b]
        self.assertEqual(len(out), len(self.data))
        self.assertTrue(diff and max(diff) - min(diff) < 16)
        self.assertEqual(out[0x18:0x1C], self.data[0x18:0x1C])  # object hash is CRC-32(name), unchanged
        self.assertEqual(audit.txtr_parse(out)["hash"], "711a66d148d9d178")

    def test_retarget_rejects_non_hex(self):
        for bad in ("711A66D148D9D178", "711a66d148d9d17", "zz1a66d148d9d178"):
            with self.assertRaises(ValueError):
                audit.txtr_retarget(self.data, bad)

    def test_wrong_object_hash_is_rejected(self):
        broken = bytearray(self.data)
        broken[0x18] ^= 1
        with self.assertRaises(ValueError):
            audit.txtr_parse(bytes(broken))

    def test_leaf_must_carry_the_object_name(self):
        other = synthetic_txtr("002_gemstone", "13b451cfdca9dda2")
        swapped = other.replace(b"002_gemstone.13b4", b"002_gemstonX.13b4")
        with self.assertRaises(ValueError):
            audit.txtr_parse(swapped)


class Container(unittest.TestCase):
    def setUp(self):
        self.members = [(f"{n}.TXTR", synthetic_txtr(n, "0123456789abcdef"))
                        for n in ("001_helicopter", "002_gemstone", "003_house_money")]
        self.zip = stored_zip(self.members)

    def test_replace_keeps_size_order_and_crcs(self):
        new = audit.txtr_retarget(self.members[1][1], "fedcba9876543210")
        out = audit.zip_replace_stored(self.zip, "002_gemstone.TXTR", new)
        self.assertEqual(len(out), len(self.zip))
        z = zipfile.ZipFile(io.BytesIO(out))
        self.assertIsNone(z.testzip())
        self.assertEqual([i.filename for i in z.infolist()], [m[0] for m in self.members])
        self.assertEqual(z.read("002_gemstone.TXTR"), new)
        self.assertEqual(z.read("001_helicopter.TXTR"), self.members[0][1])

    def test_replace_requires_same_size(self):
        with self.assertRaises(ValueError):
            audit.zip_replace_stored(self.zip, "002_gemstone.TXTR", b"short")

    def test_deflated_container_is_rejected(self):
        buf = io.BytesIO()
        with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
            z.writestr("a.TXTR", b"x" * 100)
        with self.assertRaises(ValueError):
            audit.zip_entries(buf.getvalue())


class Texture(unittest.TestCase):
    def test_planar_split_is_endpoints_then_indices_and_inverts(self):
        blocks = b"".join(struct.pack("<HHI", 0xF000 + i, i, 0xA0000000 + i) for i in range(audit.BLOCKS))
        planar = audit.planar_split(blocks)
        self.assertEqual(planar[:4], blocks[:4])
        self.assertEqual(planar[16384:16388], blocks[4:8])
        self.assertEqual(audit.planar_join(planar), blocks)

    def test_bc1_blocks_are_opaque_four_colour(self):
        flat = audit.bc1_block([(10, 20, 30)] * 16)
        black = audit.bc1_block([(0, 0, 0)] * 16)
        for block in (flat, black):
            a, b = struct.unpack_from("<HH", block)
            self.assertGreater(a, b)

    def test_layout_detector_prefers_the_true_layout(self):
        interleaved = audit.bc1_encode(gradient())
        self.assertEqual(len(interleaved), audit.RAW_BYTES)
        self.assertTrue(audit.layout_scores(audit.planar_split(interleaved))["planar"])
        self.assertFalse(audit.layout_scores(interleaved)["planar"])

    def test_current_rust_cover_header_is_not_stock_form(self):
        rust = audit.COVER_PREFIX + struct.pack("<II", 32768, 32768) + bytes(32768)
        with self.assertRaises(ValueError):
            audit.tld_validate_stock_form(rust)
        stock_like = audit.COVER_PREFIX + struct.pack("<II", 32768, 4) + b"\x8c\x0c\0\0"
        audit.tld_validate_stock_form(stock_like)

    def test_prefix_matches_rust_codec_header(self):
        src = ROOT / "src" / "jukebox_artwork.rs"
        if not src.exists():
            self.skipTest("Rust source not present")
        text = src.read_text(encoding="utf-8")
        self.assertIn("out[8..12].copy_from_slice(&[5, 33, 71, 1]);", text)
        self.assertIn("out[16..20].copy_from_slice(&[0, 1, 0, 1]);", text)
        self.assertIn("out[20..24].copy_from_slice(&[1, 0, 1, 0]);", text)
        self.assertEqual(audit.COVER_PREFIX[8:24], bytes([5, 33, 71, 1, 31, 0, 0, 0, 0, 1, 0, 1, 1, 0, 1, 0]))
        # The Rust codec still writes stored == raw (32768); the route needs stock form instead.
        self.assertTrue(re.search(r"out\[28\.\.32\]\.copy_from_slice\(&32768u32", text))


@unittest.skipUnless(os.environ.get("NBA2K27_ACTUAL_DISK_AUDIT") == "1", "set NBA2K27_ACTUAL_DISK_AUDIT=1")
class InstalledGame(unittest.TestCase):
    def test_installed_game_passes(self):
        report = audit.audit(pathlib.Path(audit.DEFAULT_GAME))
        failed = [c for c in report["checks"] if not c["ok"]]
        self.assertEqual(failed, [])
        self.assertEqual(report["container"]["name"], "alb_cvr_cont_fcb780e2.iff")

    def test_stock_form_tld_round_trips_through_game_oodle(self):
        oodle = audit.Oodle(pathlib.Path(audit.DEFAULT_GAME))
        planar = audit.planar_split(audit.bc1_encode(gradient()))
        tld = audit.tld_build(planar, oodle)
        audit.tld_validate_stock_form(tld)
        n, raw = oodle.decompress(tld[32:], audit.RAW_BYTES)
        self.assertEqual((n, raw), (audit.RAW_BYTES, planar))


if __name__ == "__main__":
    unittest.main()
