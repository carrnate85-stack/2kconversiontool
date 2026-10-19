import os
import pathlib
import struct
import unittest

import wnba_v2_route_audit as audit

BASE = 0x140000000
SECTION_VA = 0x1000
SECTION_RAW = 0x400
SECTION_SIZE = 0x4000


def synthetic_image(body: bytearray) -> audit.Image:
    """One-section PE whose section bytes are `body` (VA 0x1000, raw 0x400)."""
    head = bytearray(SECTION_RAW)
    head[0:2] = b"MZ"
    struct.pack_into("<I", head, 0x3C, 0x80)
    head[0x80:0x84] = b"PE\0\0"
    struct.pack_into("<HH", head, 0x84, 0x8664, 1)
    struct.pack_into("<I", head, 0x88, audit.EXPECTED_STAMP)
    struct.pack_into("<H", head, 0x94, 0xF0)
    struct.pack_into("<Q", head, 0x98 + 24, BASE)
    struct.pack_into("<I", head, 0x98 + 56, audit.EXPECTED_IMAGE_SIZE)
    sec = 0x98 + 0xF0
    head[sec : sec + 8] = b".text\0\0\0"
    struct.pack_into("<IIII", head, sec + 8, SECTION_SIZE, SECTION_VA, SECTION_SIZE, SECTION_RAW)
    struct.pack_into("<I", head, sec + 36, 0x60000020)
    return audit.Image(bytes(head + body))


def at(rva: int) -> int:
    return rva - SECTION_VA


class HelperTests(unittest.TestCase):
    def test_header_identity(self):
        img = synthetic_image(bytearray(SECTION_SIZE))
        self.assertEqual(img.stamp, audit.EXPECTED_STAMP)
        self.assertEqual(img.size_of_image, audit.EXPECTED_IMAGE_SIZE)
        self.assertEqual(img.image_base, BASE)
        with self.assertRaises(ValueError):
            img.read(0x9000, 1)

    def test_rel32_lea_and_jump_table(self):
        body = bytearray(SECTION_SIZE)
        # lea rax,[rip+disp] at 0x1100 -> 0x1200
        body[at(0x1100) : at(0x1103)] = b"\x48\x8D\x05"
        struct.pack_into("<i", body, at(0x1103), 0x1200 - 0x1107)
        # jump table at 0x1300 with targets 0x1100 and 0x1180
        struct.pack_into("<ii", body, at(0x1300), 0x1100 - 0x1300, 0x1180 - 0x1300)
        img = synthetic_image(body)
        self.assertEqual(audit.lea_rax_rip(img, 0x1100), 0x1200)
        self.assertEqual(audit.decode_jump_table(img, 0x1300, 2), [0x1100, 0x1180])
        with self.assertRaises(ValueError):
            audit.lea_rax_rip(img, 0x1180)

    def test_vtable_diff_reports_only_changed_slots(self):
        body = bytearray(SECTION_SIZE)
        for i in range(8):
            struct.pack_into("<Q", body, at(0x2000) + i * 8, BASE + 0x1000 + i)
            struct.pack_into("<Q", body, at(0x2100) + i * 8, BASE + 0x1000 + i)
        struct.pack_into("<Q", body, at(0x2100) + 3 * 8, BASE + 0x1FFF)
        img = synthetic_image(body)
        self.assertEqual(audit.vtable_diff(img, 0x2000, 0x2100, 8), [0x18])
        self.assertEqual(audit.slot(img, 0x2100, 0x18), 0x1FFF)

    def test_rtti_vtable_and_static_object_class(self):
        body = bytearray(SECTION_SIZE)
        td, col, vt, obj = 0x3000, 0x3100, 0x3208, 0x3300
        name = b".?AVBLACKTOP_DELEGATE@@\0"
        body[at(td + 16) : at(td + 16) + len(name)] = name
        struct.pack_into("<IIII", body, at(col), 1, 0, 0, td)
        struct.pack_into("<Q", body, at(vt - 8), BASE + col)
        struct.pack_into("<Q", body, at(obj), BASE + vt)
        img = synthetic_image(body)
        self.assertEqual(audit.find_vtable(img, "BLACKTOP_DELEGATE"), vt)
        self.assertEqual(audit.class_of_object(img, obj), ".?AVBLACKTOP_DELEGATE@@")
        with self.assertRaises(ValueError):
            audit.find_vtable(img, "TKU_DELEGATE")

    def test_check_collects_failures(self):
        failures = []
        self.assertTrue(audit.check(True, "unused", failures))
        self.assertFalse(audit.check(False, "recorded", failures))
        self.assertEqual(failures, ["recorded"])

    def test_wpf_old_build_layout_matches_v2_relative_layout(self):
        old = audit.WPF_OLD_BUILD
        self.assertEqual(old["callbackSlot"] - old["blacktopDelegateVtable"], 0xD10)
        rel = [t - old["categorySwitch"] for t in old["categoryTargets"]]
        self.assertEqual(rel, [0x48, 0x56, 0x4F, 0xD4, 0xAA, 0xB3])
        self.assertEqual(old["categoryContinue"] - old["categorySwitch"],
                         audit.V2["categoryContinue"] - audit.V2["categorySwitch"])
        self.assertEqual(audit.V2["categoryUnusedCase"] - audit.V2["categorySwitch"], 0xD4)


EXE = pathlib.Path(os.environ.get("C27_GAME_EXE", audit.DEFAULT_EXE))


@unittest.skipUnless(EXE.is_file(), "installed Version 2 NBA2K27.exe not present")
class InstalledExecutableTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.report = audit.audit(audit.Image(EXE.read_bytes()))

    def test_all_pinned_checks_pass(self):
        self.assertEqual(self.report["failures"], [])
        self.assertTrue(self.report["ok"])

    def test_wpf_slot_maps_to_d30_not_d10(self):
        slots = self.report["rosterHookSlots"]["BLACKTOP_DELEGATE"]
        self.assertEqual(slots["+0xD30"], "0x236E510")
        self.assertEqual(slots["+0xD10"], "0x23531A0")

    def test_freestyle_league_selects_tku_family_only(self):
        types = self.report["playNowTypes"]
        self.assertEqual(types[7]["class"], ".?AVTKU_DELEGATE@@")
        self.assertEqual(types[8]["class"], ".?AVTKU_WNBA_DELEGATE@@")
        self.assertTrue(self.report["freestyleLaunch"]["verified"])
        self.assertFalse(self.report["blacktopLaunch"]["consultsIsWnba"])

    def test_tku_pair_differs_only_in_identity_and_roster(self):
        self.assertEqual(self.report["tkuVsTkuWnbaDifferingSlots"], ["+0x8", "+0x18", "+0xD30"])


if __name__ == "__main__":
    unittest.main()
