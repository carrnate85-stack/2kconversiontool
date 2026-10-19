"""Offline tests for editor_fields_v2_audit (synthetic code + optional installed exe).

Run: python -I -m unittest discover -s tools -p "test_editor_fields_v2_audit.py"
Set C27_GAME_EXE to also audit the installed Version 2 exe.
"""
import json
import os
import pathlib
import struct
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import editor_fields_v2_audit as audit  # noqa: E402


def run(base, lines):
    st = audit.BitState(base)
    for line in lines:
        mnemonic, _, ops = line.partition(" ")
        st.step(mnemonic, ops)
    return st.field()


class BitStateTests(unittest.TestCase):
    def test_byte_shift_mask(self):  # WORK_ETHICBadge
        f = run("rsi", ["movzx ebx, byte ptr [rsi + 0x1a4]", "test rax, rax", "shr ebx, 1", "and ebx, 7", "mov edx, ebx"])
        self.assertEqual((f["bit"], f["bits"], f["signed"]), (0x1A4 * 8 + 1, 3, False))

    def test_signed_shl_sar(self):  # PreferredSystem: dword 0x130 bits 20..23, signed
        f = run("rsi", ["mov eax, dword ptr [rsi + 0x130]", "shl eax, 8", "sar eax, 0x1c"])
        self.assertEqual((f["bit"], f["bits"], f["signed"]), (0x130 * 8 + 20, 4, True))

    def test_high_byte_and_sub_register(self):
        f = run("rsi", ["mov ebx, dword ptr [rsi + 0x130]", "movzx edx, bh"])           # GrowthPotential
        self.assertEqual((f["bit"], f["bits"]), (0x131 * 8, 8))
        f = run("rsi", ["mov ebx, dword ptr [rsi + 0x134]", "shr ebx, 7", "movzx edx, bl"])  # YearsInLeague
        self.assertEqual((f["bit"], f["bits"]), (0x134 * 8 + 7, 8))
        f = run("rsi", ["movzx ebx, byte ptr [rsi + 0x180]", "and bl, 0x40", "shr bl, 6"])  # IsGenerated
        self.assertEqual((f["bit"], f["bits"]), (0x180 * 8 + 6, 1))

    def test_signed_word_and_scaled_enum_index(self):
        f = run("rsi", ["mov ebx, dword ptr [rsi + 0x128]", "movsx edx, bx"])          # SoundId
        self.assertEqual((f["bit"], f["bits"], f["signed"]), (0x128 * 8, 16, True))
        f = run("rsi", ["mov eax, dword ptr [rsi + 0x130]", "shr eax, 0x19", "and eax, 0x38"])  # PatienceImportance
        self.assertEqual((f["bit"], f["bits"], f["scale"]), (0x130 * 8 + 28, 3, 3))

    def test_immediate_mask_and_memory(self):  # getter form: mov eax, imm ; and eax, [rcx+X]
        f = run("rcx", ["mov eax, 0x7ff", "and eax, dword ptr [rcx + 0x20]"])
        self.assertEqual((f["bit"], f["bits"]), (0x20 * 8, 11))

    def test_unrelated_writes_and_other_bases_are_ignored(self):
        self.assertIsNone(run("rsi", ["mov eax, dword ptr [rdi + 0x130]", "shr eax, 3"]))
        f = run("rsi", ["mov ebx, dword ptr [rsi + 0xa8]", "mov rcx, rdi", "xor r8d, r8d", "mov r9d, 0xcb3c461c", "mov edx, ebx"])
        self.assertEqual((f["bit"], f["bits"]), (0xA8 * 8, 32))


class FakeImage:
    """Sparse image: rva -> bytes blocks, base 0x140000000."""

    def __init__(self, blocks, size=0x10000000):
        self.blocks, self.base, self.size_of_image = blocks, 0x140000000, size
        self.sections = []

    def read(self, rva, n):
        for at, data in self.blocks.items():
            if at <= rva and rva + n <= at + len(data):
                return data[rva - at:rva - at + n]
        raise ValueError(f"RVA {rva:X} not file-backed")


def rel(at, size, target):
    return struct.pack("<i", target - (at + size))


class SerializerTests(unittest.TestCase):
    def test_two_field_serializer_segments(self):
        start, strings = 0x1000, 0x8000
        code = bytearray()

        def emit(b):
            code.extend(b)

        def lea_rdx(target):
            at = start + len(code)
            emit(b"\x48\x8d\x15" + rel(at, 7, target))

        def call(target):
            at = start + len(code)
            emit(b"\xe8" + rel(at, 5, target))

        # FirstName: lea r8,[rsi+0x50]; lea rdx,"FirstName"; call TEXT_WRITER
        emit(b"\x4c\x8d\x46\x50"); lea_rdx(strings); call(audit.TEXT_WRITER)
        # WORK_ETHICBadge: movzx ebx,[rsi+0x1a4]; lea rdx,name; call FIND; shr ebx,1; and ebx,7; mov edx,ebx; call writer
        emit(b"\x0f\xb6\x9e\xa4\x01\x00\x00"); lea_rdx(strings + 0x10); call(audit.FIND)
        emit(b"\xd1\xeb\x83\xe3\x07\x89\xda"); call(0x4BED070)
        # Wage: mov ebx,[rsi+0x124]; lea rdx,name; call FIND; mov edx,ebx; call writer
        emit(b"\x8b\x9e\x24\x01\x00\x00"); lea_rdx(strings + 0x20); call(audit.FIND)
        emit(b"\x89\xda"); call(0x4BED170)
        img = FakeImage({start: bytes(code) + b"\xcc" * 16,
                         strings: (b"FirstName\0".ljust(16, b"\0") + b"WORK_ETHICBadge\0" + b"Wage\0").ljust(256, b"\0")})
        fields = audit.serializer_fields(img, start, start + len(code), "rsi")
        by = {f["name"]: f for f in fields}
        self.assertEqual(by["FirstName"]["kind"], "text")
        self.assertEqual(by["FirstName"]["offset"], 0x50)
        self.assertEqual((by["WORK_ETHICBadge"]["bit"], by["WORK_ETHICBadge"]["bits"]), (0x1A4 * 8 + 1, 3))
        self.assertEqual((by["Wage"]["bit"], by["Wage"]["bits"]), (0x124 * 8, 32))


class CompareTests(unittest.TestCase):
    FIELDS = [{"name": "FirstName", "kind": "text", "offset": 0x50}, {"name": "LastName", "kind": "text", "offset": 0x78},
              {"name": "Wage", "kind": "bits", "bit": 0x124 * 8, "bits": 32},
              {"name": "UniqueId", "kind": "bits", "bit": 0x120 * 8, "bits": 16},
              {"name": "WORK_ETHICBadge", "kind": "bits", "bit": 0x1A4 * 8 + 1, "bits": 3}]

    def write_ref(self, d, controls, badges, masks=None):
        data = pathlib.Path(d, "ref/data")
        data.mkdir(parents=True)
        (data / "staff-controls.json").write_text(json.dumps(controls), encoding="utf-8")
        (data / "staff-field-schema.json").write_text(json.dumps({"badges": badges, "roles": []}), encoding="utf-8")
        helper = pathlib.Path(d, "helper")
        helper.mkdir()
        if masks is not None:
            (helper / "TeamStaffPatch.cs").write_text("static readonly byte[] Masks=new byte[]{" + ",".join(map(str, masks)) + "};", encoding="utf-8")
        return pathlib.Path(d, "ref"), helper

    def test_exact_unmapped_and_renamed(self):
        with tempfile.TemporaryDirectory() as d:
            ref, helper = self.write_ref(d, [{"key": "wage", "offset": 0x124, "shift": 0, "bits": 32},
                                             {"key": "firstName", "offset": 0x50, "bytes": 40, "text": True},
                                             {"key": "ghost", "offset": 0x30, "shift": 0, "bits": 8}],
                                         [{"key": "WORK_ETHICBadge", "offset": 0x1A4, "shift": 1, "bits": 3},
                                          {"key": "RENAMEDBadge", "offset": 0x1A4, "shift": 1, "bits": 3}])
            rows, editable = audit.compare_staff(self.FIELDS, ref, helper)
            by = {(r["source"], r["key"]): r["match"] for r in rows}
            self.assertEqual(by[("staff-controls", "wage")], "exact")
            self.assertEqual(by[("staff-controls", "firstName")], "exact")
            self.assertEqual(by[("staff-controls", "ghost")], "unmapped")
            self.assertEqual(by[("schema.badges", "WORK_ETHICBadge")], "exact")
            self.assertIn("name differs", by[("schema.badges", "RENAMEDBadge")])
            self.assertIsNone(editable)

    def test_editable_mask_outside_named_or_locked_field_is_reported(self):
        masks = [0] * 0x1C8
        masks[0x124] = 0xFF        # Wage: allowed
        masks[0x120] = 0x01        # UniqueId: locked identity
        masks[0x30] = 0x80         # no field
        with tempfile.TemporaryDirectory() as d:
            ref, helper = self.write_ref(d, [], [], masks)
            _, editable = audit.compare_staff(self.FIELDS, ref, helper)
            self.assertEqual(editable["fields"], {"Wage": 8})
            self.assertEqual(editable["outsideNamedFields"], ["0x30.7", "0x120.0"])

    def test_season_rows_need_setter_and_getter(self):
        setters = {1: {"name": "FreeThrowsMade", "bit": 0x20 * 8, "bits": 11}, 118: {"name": "MVP", "bit": 0x14 * 8 + 7, "bits": 1}}
        getters = {1: {"bit": 0x20 * 8, "bits": 11}}
        rows, summary = audit.compare_seasons(setters, getters)
        by = {r["key"]: r for r in rows}
        self.assertTrue(by["ftm"]["setterOk"] and by["ftm"]["getterOk"])
        self.assertTrue(by["mvp"]["setterOk"])
        self.assertFalse(by["mvp"]["getterOk"])
        self.assertFalse(by["fta"]["setterOk"])
        self.assertEqual(summary["setterGetterAgree"], 1)

    def test_helper_constants_parse(self):
        with tempfile.TemporaryDirectory() as d:
            body = ",".join("{0x%X,%d,%d}" % (o, s, b) for _, o, s, b in audit.WPF_SEASON_TOTALS)
            pathlib.Path(d, "SeasonImportModel.cs").write_text(f"static readonly int[,] TotalFields={{{body}}};", encoding="utf-8")
            self.assertTrue(audit.helper_constants_match(pathlib.Path(d)))
            pathlib.Path(d, "SeasonImportModel.cs").write_text("static readonly int[,] TotalFields={{0x20,0,11}};", encoding="utf-8")
            self.assertFalse(audit.helper_constants_match(pathlib.Path(d)))
            self.assertIsNone(audit.helper_constants_match(pathlib.Path(d, "missing")))

    def test_anchor_failure_fails_closed(self):
        img = FakeImage({0: bytes(16)})
        out = audit.check_anchors(img)
        self.assertEqual(len(out), len(audit.ANCHORS) + len(audit.RIP_ANCHORS) + 1 + len(audit.CALL_ANCHORS) + len(audit.STRING_REFS))
        self.assertFalse(any(a["ok"] for a in out))

    def test_anchor_hex_is_well_formed(self):
        for name, rva, want in audit.ANCHORS:
            self.assertEqual(len(bytes.fromhex(want)) * 2, len(want), name)


@unittest.skipUnless(os.environ.get("C27_GAME_EXE"), "set C27_GAME_EXE to audit the installed exe")
class InstalledExeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.img = audit.Image(pathlib.Path(os.environ["C27_GAME_EXE"]).read_bytes())
        cls.report = audit.audit(cls.img)

    def test_full_audit_passes(self):
        self.assertTrue(self.report["identity"]["ok"])
        self.assertEqual([a["name"] for a in self.report["anchors"] if not a["ok"]], [])
        self.assertEqual([o["name"] for o in self.report["oldToV2"] if not o["ok"]], [])
        self.assertTrue(self.report["ok"])

    def test_key_staff_fields(self):
        by = {f["name"]: f for f in self.report["staffFields"]}
        expect = {"UniqueId": (0x120 * 8, 16), "Wage": (0x124 * 8, 32), "GraphicId": (0x12A * 8, 15), "SoundId": (0x128 * 8, 16),
                  "PlayerUniqueId": (0xA8 * 8, 32), "PreferredSystem": (0x130 * 8 + 20, 4), "ActiveSystem": (0x130 * 8 + 24, 4),
                  "YearsInLeague": (0x134 * 8 + 7, 8), "Job": (0x182 * 8, 5), "BenchDepth": (0x158 * 8, 8),
                  "Analytics": (0x16F * 8, 8), "SystemDefenseFirst": (0x177 * 8, 8), "CHAT_CONNOISSEURBadge": (0x1C1 * 8 + 4, 1)}
        for name, (bit, bits) in expect.items():
            self.assertEqual((by[name]["bit"], by[name]["bits"]), (bit, bits), name)
        self.assertTrue(by["PreferredSystem"]["signed"])
        self.assertEqual(by["FirstName"]["offset"], 0x50)
        self.assertEqual(by["LastName"]["offset"], 0x78)

    def test_seasons(self):
        self.assertEqual(self.report["seasonSummary"]["setterGetterAgree"], self.report["seasonSummary"]["setters"])
        self.assertTrue(all(r["setterOk"] and r["getterOk"] for r in self.report["seasonVsWpf"]))

    def test_job_names_are_wpf_roles(self):
        job = next(e for e in self.report["staffEnums"] if e["field"] == "Job")
        self.assertTrue(job["referenced"])
        self.assertTrue(job.get("matchesWpfRoles", True))


if __name__ == "__main__":
    unittest.main()
