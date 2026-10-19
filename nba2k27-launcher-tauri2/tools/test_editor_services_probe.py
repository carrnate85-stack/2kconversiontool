"""Offline tests for tools/Read-EditorServicesProbe.ps1 using synthetic memory fixtures.

Builds a fake V2 image (pinned PE header, roster code anchors, root class) plus a
private 8 MiB roster allocation, runs the probe with -Fixture (no process access)
and checks each live check's verdict. Set C27_GAME_EXE to also copy the anchor
bytes from the installed exe instead of synthesizing them (proves the probe's
anchors match the real file).

Run: python -I -m unittest discover -s tools -p "test_editor_services_probe.py"
"""
import base64
import json
import os
import pathlib
import struct
import subprocess
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
SCRIPT = HERE / "Read-EditorServicesProbe.ps1"
BASE = 0x140000000
IMAGE_SIZE = 0x35BDE000
ROOT, ROOT_CLASS = 0x09ACE338, 0x074EBE98
RIP = [(0x0211A0FB, "488b05", ROOT), (0x0211A120, "488b05", ROOT), (0x02406BF9, "488d3d", ROOT_CLASS)]
FIELD = [
    (0x0211A102, "488b80a8000000"),
    (0x02CC96F1, "3b5110731289d04869c0f8040000480341" "18"),
    (0x02DDD3B1, "3b9180000000731589d04869c05015000048038188000000"),
    (0x02ED9BD1, "3b5160731289d04869c078120000480341" "68"),
    (0x02EDCB31, "3b9190020000731589d04869c070010000480381980200" "00"),
]
RB, RL = 0x200000000, 8 * 1024 * 1024
OWNER, ROOTOBJ = RB + 0x1000, RB + 0x100
PT, TT, ST, SEAS = RB + 0x10000, RB + 0x80000, RB + 0xA0000, RB + 0xC0000
NPLAYERS, NTEAMS, NSTAFF, NSEASONS = 120, 3, 6, 10
PLAYER, TEAM, STAFF, SEASON = 0x4F8, 0x1550, 0x1C8, 0x40


def utf16(s, n):
    b = s.encode("utf-16-le")
    return b + bytes(n - len(b))


class Fixture:
    def __init__(self, stamp=0x6AC53765):
        self.blocks = []
        head = bytearray(4096)
        head[0:2] = b"MZ"
        struct.pack_into("<I", head, 0x3C, 0x100)
        head[0x100:0x104] = b"PE\0\0"
        struct.pack_into("<HHI", head, 0x104, 0x8664, 1, stamp)
        struct.pack_into("<H", head, 0x118, 0x20B)
        struct.pack_into("<I", head, 0x150, IMAGE_SIZE)
        self.put(BASE, bytes(head))
        exe = os.environ.get("C27_GAME_EXE")
        img = None
        if exe:
            sys.path.insert(0, str(HERE))
            from outfit_v2_disk_audit import Image
            img = Image(pathlib.Path(exe).read_bytes())
        for rva, pre, target in RIP:
            pre = bytes.fromhex(pre)
            code = img.read(rva, len(pre) + 4) if img else pre + struct.pack("<i", target - (rva + len(pre) + 4))
            self.put(BASE + rva, code)
        for rva, want in FIELD:
            want = bytes.fromhex(want)
            self.put(BASE + rva, img.read(rva, len(want)) if img else want)
        self.put(BASE + ROOT_CLASS, struct.pack("<QQ", BASE + 0x024E65F0, BASE + 0x024E6660))
        self.put(BASE + ROOT, struct.pack("<Q", ROOTOBJ))
        root = bytearray(0xB0)
        struct.pack_into("<Q", root, 0, BASE + ROOT_CLASS)
        struct.pack_into("<Q", root, 0xA8, OWNER)
        self.put(ROOTOBJ, bytes(root))
        self.owner = bytearray(0x540)
        for off, count, table in [(0x10, NPLAYERS, PT), (0x60, 1, RB + 0x70000), (0x80, NTEAMS, TT), (0x180, NSTAFF, ST)]:
            struct.pack_into("<QQ", self.owner, off, count, table)
        struct.pack_into("<IIQ", self.owner, 0x1C0, 1, 0, RB + 0xB0000)
        struct.pack_into("<iiQ", self.owner, 0x1D0, NSEASONS, 0, SEAS)
        self.teams = [bytearray(TEAM) for _ in range(NTEAMS)]
        self.players = [bytearray(PLAYER) for _ in range(NPLAYERS)]
        self.staff = [bytearray(STAFF) for _ in range(NSTAFF)]
        self.seasons = [bytearray(SEASON) for _ in range(NSEASONS)]
        for t, row in enumerate(self.teams):
            row[0x2FA:0x2FA + 48] = utf16(f"Team {t}", 48)
        for i, row in enumerate(self.players):
            row[0:40] = utf16(f"Last{i}", 40)
            row[40:80] = utf16(f"First{i}", 40)
            struct.pack_into("<H", row, 0x128, 1000 + i)
            struct.pack_into("<Q", row, 0x60, TT + (i % NTEAMS) * TEAM)
            struct.pack_into("<33h", row, 0x130, *([i % NSEASONS] + [-1] * 32))
        for t, row in enumerate(self.teams):
            members = [1000 + i for i in range(NPLAYERS) if i % NTEAMS == t][:10]
            for k in range(48):
                struct.pack_into("<5H", row, 0x1360 + k * 10, *members[k % 2 * 5:k % 2 * 5 + 5])
        for s, row in enumerate(self.staff):
            team, slot = s % NTEAMS, s // NTEAMS
            struct.pack_into("<Q", self.teams[team], 0xF70 + slot * 8, ST + s * STAFF)
            struct.pack_into("<Q", row, 0x18, TT + team * TEAM)
            row[0x182] = slot
            row[0x50:0x50 + 40] = utf16(f"Coach{s}", 40)
            row[0x78:0x78 + 40] = utf16(f"Staff{s}", 40)
            struct.pack_into("<H", row, 0x120, 500 + s)
            struct.pack_into("<I", row, 0x130, 0xFFF00000)          # preferred/active system = -1 (unset)
            row[0x158:0x170] = bytes(range(40, 64))
        refs = [sum(1 for i in range(NPLAYERS) if i % NSEASONS == s) for s in range(NSEASONS)]
        for s, row in enumerate(self.seasons):
            struct.pack_into("<Q", row, 0, TT + (s % NTEAMS) * TEAM)
            row[0x11] = 120                                         # 2020
            struct.pack_into("<I", row, 0x3C, refs[s] << 13)          # reference count
        self.pool = bytearray(0x38)
        struct.pack_into("<HxxxxxxQi", self.pool, 0, 0xFFFF, SEAS, NSEASONS)
        struct.pack_into("<4Q", self.pool, 0x18, *(BASE + rva for rva in (0x2EEB980, 0x2EEB950, 0x2FD0870, 0x2FD0830)))

    def put(self, at, data):
        self.blocks.append({"at": f"{at:X}", "data": base64.b64encode(data).decode()})

    def write(self, path):
        self.put(OWNER, bytes(self.owner))
        self.put(RB + 0xB0000, bytes(self.pool))
        for base, rows, stride in [(PT, self.players, PLAYER), (TT, self.teams, TEAM), (ST, self.staff, STAFF), (SEAS, self.seasons, SEASON)]:
            self.put(base, b"".join(bytes(r) for r in rows))
        doc = {
            "base": f"{BASE:X}",
            "regions": [
                {"base": f"{BASE:X}", "allocation": f"{BASE:X}", "len": f"{IMAGE_SIZE:X}", "state": 0x1000, "type": 0x1000000, "protect": 0x20},
                {"base": f"{RB:X}", "allocation": f"{RB:X}", "len": f"{RL:X}", "state": 0x1000, "type": 0x20000, "protect": 4},
            ],
            "blocks": self.blocks,
        }
        pathlib.Path(path).write_text(json.dumps(doc), encoding="utf-8")


def run(fx):
    with tempfile.TemporaryDirectory() as d:
        fixture, out = pathlib.Path(d, "fixture.json"), pathlib.Path(d, "out.json")
        fx.write(fixture)
        p = subprocess.run(["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", str(SCRIPT),
                            "-Fixture", str(fixture), "-Out", str(out)], capture_output=True, text=True, timeout=180)
        doc = json.loads(out.read_text(encoding="utf-8-sig")) if out.exists() else None
        return p.returncode, p.stdout + p.stderr, doc


def status(doc, key):
    return doc["checks"][key]["status"]


class ProbeTests(unittest.TestCase):
    def test_consistent_fixture_passes_all_checks(self):
        code, text, doc = run(Fixture())
        self.assertEqual(code, 0, text)
        self.assertTrue(doc["stable"])
        self.assertEqual(doc["players"], NPLAYERS)
        for key in ["1_staff", "2_rotations", "3_player_team_pointer", "4_seasons", "5_staff_fields", "6_season_pool"]:
            self.assertEqual(status(doc, key), "PASS", (key, doc["checks"][key]))
        self.assertEqual(doc["checks"]["1_staff"]["teamSlots"], NSTAFF)
        self.assertEqual(doc["checks"]["2_rotations"]["sameTeamViaPlus60"], doc["checks"]["2_rotations"]["teamCheckedViaPlus60"])
        self.assertIn("PASS   1_staff", text)
        s = doc["checks"]["5_staff_fields"]
        self.assertEqual(s["records"], NSTAFF)
        self.assertEqual(s["samples"][0]["preferredSystem"], -1)
        self.assertEqual(s["samples"][0]["ratings158"]["BenchDepth"], 40)
        self.assertEqual(s["byteRanges158"]["Analytics"], "63..63")
        p = doc["checks"]["6_season_pool"]
        self.assertEqual((p["callbacksMatch"], p["usedRecords"], p["freeChain"], p["poolTableMatches1D8"]), (4, NSEASONS, 0, True))
        m = doc["checks"]["7_menu_statics"]
        self.assertEqual((m["status"], m["wpfSafeMenu"]), ("INFO", False))

    def test_flagged_advanced_rotation_is_checked_against_members(self):
        fx = Fixture()
        fx.teams[0][0x154C] |= 1
        code, text, doc = run(fx)
        c = doc["checks"]["2_rotations"]
        self.assertEqual((c["status"], c["advancedRotationTeams"], c["advancedRotationValid"]), ("PASS", 1, 1), c)
        struct.pack_into("<H", fx.teams[0], 0x1360 + 7 * 10 + 4, 1001)   # player 1 belongs to team 1
        code, text, doc = run(fx)
        c = doc["checks"]["2_rotations"]
        self.assertEqual((c["status"], c["advancedRotationDetail"][0]["firstBadSegment"]), ("FAIL", 7), c)

    def test_season_pool_refcount_callback_and_staff_id_faults(self):
        fx = Fixture()
        struct.pack_into("<I", fx.seasons[3], 0x3C, 1 << 13)
        struct.pack_into("<Q", fx.pool, 0x28, BASE + 0x2F9A7A0)              # old-build AddRef
        struct.pack_into("<H", fx.staff[1], 0x120, 500)                       # duplicate staff id
        code, text, doc = run(fx)
        p = doc["checks"]["6_season_pool"]
        self.assertEqual((p["status"], p["callbacksMatch"], p["refcountMismatches"]), ("FAIL", 3, 1), p)
        self.assertEqual(p["callbacks"][2], "0x2F9A7A0")
        s = doc["checks"]["5_staff_fields"]
        self.assertEqual((s["status"], s["uniqueIdDuplicates"]), ("FAIL", 1))

    def test_broken_back_pointer_and_out_of_range_season_fail(self):
        fx = Fixture()
        struct.pack_into("<Q", fx.staff[0], 0x18, TT + TEAM)  # staff 0 belongs to team 0
        struct.pack_into("<h", fx.players[5], 0x130, NSEASONS + 3)
        code, text, doc = run(fx)
        self.assertEqual(code, 0, text)
        self.assertEqual(status(doc, "1_staff"), "FAIL")
        self.assertEqual(status(doc, "4_seasons"), "FAIL")
        self.assertEqual(status(doc, "3_player_team_pointer"), "PASS")

    def test_wrong_team_pointer_and_unknown_rotation_ids_fail(self):
        fx = Fixture()
        for row in fx.players:
            struct.pack_into("<Q", row, 0x60, RB + 0x123458)
        for row in fx.teams:
            struct.pack_into("<240H", row, 0x1360, *([7] * 240))
        code, text, doc = run(fx)
        self.assertEqual(status(doc, "3_player_team_pointer"), "FAIL")
        self.assertEqual(status(doc, "2_rotations"), "FAIL")

    def test_staff_shared_with_another_team_is_pass_shared(self):
        fx = Fixture()
        struct.pack_into("<Q", fx.staff[0], 0x18, TT + 2 * TEAM)       # staff 0 owned by team 2 ...
        struct.pack_into("<Q", fx.teams[2], 0xF70, ST)                # ... which also lists it in slot 0
        code, text, doc = run(fx)
        c = doc["checks"]["1_staff"]
        self.assertEqual(c["status"], "PASS-SHARED", c)
        self.assertEqual(c["sharedWithOtherTeam"], 1)
        self.assertEqual(c["backPointerMismatches"][0]["plus18Team"], 2)

    def test_empty_unflagged_rotation_block_is_auto_and_member_scan_finds_moved_ids(self):
        fx = Fixture()
        for t, row in enumerate(fx.teams):
            row[0x1360:0x1540] = bytes(0x1E0)
            members = [1000 + i for i in range(NPLAYERS) if i % NTEAMS == t][:5]
            struct.pack_into("<5H", row, 0x400, *members)
        code, text, doc = run(fx)
        c = doc["checks"]["2_rotations"]
        self.assertEqual(c["status"], "AUTO")
        self.assertEqual(c["advancedRotationTeams"], 0)
        self.assertEqual(c["blockZeroU16"], NTEAMS * 240)
        self.assertEqual(c["memberUniqueIdOffsets"][0], "0x400:3")

    def test_wrong_build_refuses_before_reading_roster(self):
        code, text, doc = run(Fixture(stamp=0x6AA2013D))
        self.assertEqual(code, 2, text)
        self.assertIn("Not the pinned Version 2 build", doc["error"])
        self.assertNotIn("checks", doc)

    def test_script_requests_only_read_rights(self):
        src = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("OpenProcess(0x0400 | 0x0010", src)
        for bad in ["WriteProcessMemory", "VirtualProtectEx", "CreateRemoteThread", "0x1F0FFF", "0x0020", "0x0008 |"]:
            self.assertNotIn(bad, src)


if __name__ == "__main__":
    unittest.main()
