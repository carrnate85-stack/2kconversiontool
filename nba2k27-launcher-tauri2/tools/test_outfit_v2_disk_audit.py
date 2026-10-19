"""Offline tests for outfit_v2_disk_audit (synthetic images + real launcher source).

Run: python -I -m unittest discover -s tools -p "test_outfit_v2_disk_audit.py"
Set C27_GAME_EXE to also run the full audit against the installed Version 2 exe.
"""
import os
import pathlib
import struct
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import outfit_v2_disk_audit as audit  # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parent.parent
BASE = 0x140000000
TEXT_VA, DATA_VA = 0x1000, 0x5000
RAW_TEXT, RAW_DATA, SIZE = 0x400, 0x4400, 0x4000


def image(text: bytearray, data: bytearray) -> audit.Image:
    head = bytearray(RAW_TEXT)
    head[0:2] = b"MZ"
    struct.pack_into("<I", head, 0x3C, 0x80)
    head[0x80:0x84] = b"PE\0\0"
    struct.pack_into("<HHI", head, 0x84, 0x8664, 2, audit.EXPECTED_STAMP)
    struct.pack_into("<H", head, 0x94, 0xF0)
    struct.pack_into("<Q", head, 0x98 + 24, BASE)
    struct.pack_into("<I", head, 0x98 + 56, audit.EXPECTED_IMAGE_SIZE)
    sec = 0x98 + 0xF0
    for i, (name, va, raw, flags) in enumerate([(b".text", TEXT_VA, RAW_TEXT, 0x60000020), (b".data", DATA_VA, RAW_DATA, 0xC0000040)]):
        at = sec + i * 40
        head[at:at + 8] = name.ljust(8, b"\0")
        struct.pack_into("<IIII", head, at + 8, SIZE, va, SIZE, raw)
        struct.pack_into("<I", head, at + 36, flags)
    return audit.Image(bytes(head + text + data))


def with_rtti(name: bytes):
    """Data section with COL at DATA_VA+0x100, type descriptor at +0x200, vtable at +0x308."""
    data = bytearray(SIZE)
    col, td, vt = DATA_VA + 0x100, DATA_VA + 0x200, DATA_VA + 0x308
    struct.pack_into("<IIIIII", data, 0x100, 1, 0, 0, td, 0, col)
    data[0x210:0x210 + len(name) + 1] = name + b"\0"
    struct.pack_into("<Q", data, 0x300, BASE + col)
    return data, vt


class ImageTests(unittest.TestCase):
    def test_header_and_sections(self):
        img = image(bytearray(SIZE), bytearray(SIZE))
        self.assertEqual(img.stamp, audit.EXPECTED_STAMP)
        self.assertTrue(img.executable(TEXT_VA + 4))
        self.assertTrue(img.writable_data(DATA_VA + 4))
        self.assertFalse(img.writable_data(TEXT_VA + 4))
        with self.assertRaises(ValueError):
            img.read(DATA_VA + SIZE - 2, 8)

    def test_rtti_name_resolves_and_rejects_bad_locator(self):
        data, vt = with_rtti(b".?AVPLAYER_DESCRIPTION@CHARACTER@@")
        self.assertEqual(image(bytearray(SIZE), data).rtti_name(vt), ".?AVPLAYER_DESCRIPTION@CHARACTER@@")
        struct.pack_into("<I", data, 0x100, 0)  # signature must be 1
        self.assertIsNone(image(bytearray(SIZE), data).rtti_name(vt))
        data, vt = with_rtti(b"not-a-type")
        self.assertIsNone(image(bytearray(SIZE), data).rtti_name(vt))

    def test_rip_xrefs_finds_disp32_and_ignores_others(self):
        text = bytearray(SIZE)
        target = DATA_VA + 0x40
        # mov rax, [rip+disp] at TEXT_VA+0x10: 48 8B 05 disp32
        disp = target - (TEXT_VA + 0x13 + 4)
        text[0x10:0x17] = b"\x48\x8B\x05" + struct.pack("<i", disp)
        text[0x40:0x47] = b"\x48\x8B\x05" + struct.pack("<i", disp - 0x100)  # different target
        refs = audit.rip_xrefs(image(text, bytearray(SIZE)), {"t": target})
        self.assertEqual(refs["t"], [TEXT_VA + 0x13])


class SourceTests(unittest.TestCase):
    def test_parses_real_selection_source(self):
        consts, fields, virtuals = audit.parse_selection_source((ROOT / "src" / "outfit_selection_v2.rs").read_text(encoding="utf-8"))
        self.assertEqual(consts, {"ROOT": 0xAF46870, "LINEUP": 0x9C496C0, "MODEL": 0x67DD188, "DESCRIPTION": 0x68920C8})
        self.assertEqual(len(fields), 11)
        self.assertEqual(fields[0], (0x3B0006, bytes([0x80, 0x79, 0x18, 1])))
        self.assertEqual(len(virtuals), 10)
        self.assertIn((0x68920C8, 0x1B0, 0x827120), virtuals)

    def test_legacy_deltas_are_positive(self):
        consts, _, _ = audit.parse_selection_source((ROOT / "src" / "outfit_selection_v2.rs").read_text(encoding="utf-8"))
        for k, v in audit.LEGACY.items():
            self.assertGreater(consts[k], v)


@unittest.skipUnless(os.environ.get("C27_GAME_EXE"), "set C27_GAME_EXE to the installed Version 2 NBA2K27.exe")
class InstalledExeTests(unittest.TestCase):
    def test_full_audit_passes(self):
        img = audit.Image(pathlib.Path(os.environ["C27_GAME_EXE"]).read_bytes())
        result = audit.audit(img, ROOT, xrefs=True)
        self.assertEqual(result["failures"], [])
        self.assertEqual(result["rtti"]["68920C8"]["class"], ".?AVPLAYER_DESCRIPTION@CHARACTER@@")
        self.assertEqual(result["rtti"]["67DD188"]["class"], ".?AVCONTAINER@CHARACTER@@")


if __name__ == "__main__":
    unittest.main()


class ProducerLineTests(unittest.TestCase):
    def test_finds_stock_producer_line_immediate_for_the_allocate_call(self):
        text = bytearray(SIZE)
        at = 0x100
        # Live V2 order: line store, packet size, heap tag, then the call.
        code = (bytes.fromhex("C7442420") + struct.pack("<I", 0x67C) + bytes.fromhex("B940000000")
                + bytes.fromhex("4531C0") + bytes.fromhex("41B9") + struct.pack("<I", audit.REQUEST_HEAP_TAG))
        text[at:at + len(code)] = code
        call = at + len(code)
        allocate = TEXT_VA + 0x800
        text[call] = 0xE8
        struct.pack_into("<i", text, call + 1, allocate - (TEXT_VA + call + 5))
        img = image(text, bytearray(SIZE))
        self.assertEqual(audit.producer_allocation_lines(img, allocate), [(TEXT_VA + at, 0x67C)])
        # A different call target is not the allocator.
        self.assertEqual(audit.producer_allocation_lines(img, allocate + 0x10), [])
        # Another allocation size (not the 0x40 create request) is ignored.
        text[at + 9] = 0x18
        self.assertEqual(audit.producer_allocation_lines(image(text, bytearray(SIZE)), allocate), [])

    def test_worker_source_line_immediate_is_parsed(self):
        self.assertEqual(audit.worker_allocation_line(ROOT), 0x67C)
