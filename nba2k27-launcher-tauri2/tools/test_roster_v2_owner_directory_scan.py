"""Offline tests for roster_v2_owner_directory_scan (synthetic code + optional installed exe).

Run: python -I -m unittest discover -s tools -p "test_roster_v2_owner_directory_scan.py"
Set C27_GAME_EXE to also scan the installed Version 2 exe.
"""
import os
import pathlib
import struct
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import roster_v2_owner_directory_scan as scan  # noqa: E402

MOV = b"\x89\xd0"


def add8(disp):
    return b"\x48\x03\x41" + struct.pack("<b", disp + 8)


def add32(disp):
    return b"\x48\x03\x81" + struct.pack("<i", disp + 8)


class StrideTests(unittest.TestCase):
    def test_forms(self):
        cases = {
            b"\x48\x69\xc0" + struct.pack("<I", 0x4F8): 0x4F8,
            b"\x48\x6b\xc0\x38": 0x38,
            b"\x48\xc1\xe0\x05": 0x20,
            b"\x48\x8d\x04\xc0\x48\xc1\xe0\x04": 0x90,   # lea rax,[rax+rax*8]; shl 4
            b"\x48\x8d\x04\x40\x48\x01\xc0": 6,          # lea rax,[rax+rax*2]; add rax,rax
            b"\x48\x89\xc2\x48\xc1\xe2\x07\x48\x8d\x04\xc2": 0x88,  # rdx=rax<<7; rax=rdx+rax*8
        }
        for arith, want in cases.items():
            self.assertEqual(scan.stride_of(arith), want, arith.hex())

    def test_rejects_unknown_or_identity(self):
        self.assertIsNone(scan.stride_of(b"\x90"))
        self.assertIsNone(scan.stride_of(b""))
        self.assertIsNone(scan.stride_of(b"\x48\x8d\x04\x48"))  # rcx index is not modeled


class AnchorTests(unittest.TestCase):
    def test_anchor_mismatch_and_unbacked_rva_fail_closed(self):
        class Fake:
            def read(self, rva, n):
                if rva == scan.SERVICE_ANCHORS[0][1]:
                    raise ValueError("not file-backed")
                return bytes(n)
        out = scan.check_anchors(Fake())
        self.assertEqual(len(out), len(scan.SERVICE_ANCHORS))
        self.assertFalse(any(a["ok"] for a in out))

    def test_anchor_hex_is_well_formed(self):
        for name, rva, want in scan.SERVICE_ANCHORS:
            self.assertEqual(len(bytes.fromhex(want)) * 2, len(want), name)


class ScanTests(unittest.TestCase):
    def test_scan_code_finds_disp8_and_disp32(self):
        code = (b"\xcc" * 8 + b"\x44\x39\xc2\x73\x12" + MOV + b"\x48\x69\xc0" + struct.pack("<I", 0x1550) + add32(0x80)
                + b"\xcc" * 8 + MOV + b"\x48\xc1\xe0\x05" + add8(0x50)
                + b"\xcc" * 8 + add8(0x60))  # no mov eax,edx -> ignored
        found = [(d, s) for d, s, _ in scan.scan_code(code, 0x1000)]
        self.assertEqual(found, [(0x80, 0x1550), (0x50, 0x20)])

    def test_scan_code_rejects_misaligned_and_negative(self):
        code = MOV + b"\x48\x6b\xc0\x38" + add8(0x13) + MOV + b"\x48\x6b\xc0\x38" + b"\x48\x03\x41\x00"
        self.assertEqual(list(scan.scan_code(code, 0)), [])

    def test_family_window_excludes_far_sites(self):
        pairs = {(0x10, 0x4F8): [0x100000], (0x80, 0x1550): [0x101000], (0x200, 0x38): [0x102000], (0x200, 0x99): [0x900000]}
        self.assertEqual(scan.family(pairs), {0x10: [0x4F8], 0x80: [0x1550], 0x200: [0x38]})
        self.assertEqual(scan.family({}), {})


@unittest.skipUnless(os.environ.get("C27_GAME_EXE"), "set C27_GAME_EXE to the installed NBA2K27.exe")
class InstalledExeTests(unittest.TestCase):
    def test_installed_v2_family(self):
        img = scan.Image(pathlib.Path(os.environ["C27_GAME_EXE"]).read_bytes())
        out = scan.report(img, scan.scan(img))
        self.assertTrue(out["expectedBuild"])
        self.assertTrue(out["knownDirectoriesFound"], out["missingKnown"])
        self.assertEqual(out["familyConflicts"], [])
        fam = out["family"]
        for off, stride in [("0x10", "0x4F8"), ("0x60", "0x1278"), ("0x80", "0x1550"), ("0x290", "0x170")]:
            self.assertEqual(fam[off], [stride])
        self.assertGreaterEqual(len(fam), 80)
        # Old-build staff/season directories keep their offsets and strides in V2.
        self.assertEqual(fam["0x180"], ["0x1C8"])
        self.assertEqual(fam["0x1D0"], ["0x40"])
        bad = [a["name"] for a in out["serviceAnchors"] if not a["ok"]]
        self.assertEqual(bad, [])


if __name__ == "__main__":
    unittest.main()
