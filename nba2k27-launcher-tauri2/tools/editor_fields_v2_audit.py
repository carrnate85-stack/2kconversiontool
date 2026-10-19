"""Disk-only proof of the Version 2 editor field layouts: staff fields, rotations, season stats.

Follow-up to artifacts/editor-services-v2-research-20261017.md. Reads the installed
NBA2K27.exe (and, when present, the frozen WPF reference + helper source) only.
Never opens the game process and never writes anything except the optional --json.

What it proves, from the game's own code:
  * staff   : the V2 StaffData serializer (0x6BBA00, the same 17,580-byte span the WPF
              helper hashed at old 0x6B5900) emits every staff field by name. Each
              field's load/shift/mask sequence is evaluated symbolically into an exact
              bit range, then compared with WPF's staff-field-schema.json /
              staff-controls.json / decode() offsets and TeamStaffPatch's editable masks.
  * seasons : PLAYERSTATDATA::SetByIndex (0x2E5D7C0, switch of 0x95 stat ids, each case
              asserts "PLAYERSTATDATA::SetX") and GetByIndex (0x2E07200, 577 bytes like
              old 0x2DD2500) give every stat bitfield twice; compared with WPF's
              TotalFields / AwardFields. Pool callbacks, getter, Clear/GetYear/Acquire
              and refcount helpers are pinned as byte anchors.
  * rotations: TEAMDATA advanced-rotation getter/commit/gameplan-apply anchors
              (team+0x1360 48x5 u16 player UniqueIds, team+0x154C bit 0 = has rotation).
  * menu    : menu-stack statics used by WPF's season import, re-derived for V2.

Usage: python -I tools/editor_fields_v2_audit.py <NBA2K27.exe> [--wpf-ref DIR] [--helper-src DIR] [--json out.json]
"""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import re
import struct
import sys

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE / "python-packages"))
import capstone  # type: ignore  # noqa: E402  vendored (see wnba_controlflow_audit.py)
from outfit_v2_disk_audit import EXPECTED_IMAGE_SIZE, EXPECTED_STAMP, Image  # noqa: E402

REPO = HERE.parent.parent
DEFAULT_WPF_REF = REPO / "nba2k27-launcher-wpf/release/quality-pass-qa-20261010/App/Reference/src"
DEFAULT_HELPER = REPO / "nba2k27-roster-research/src"

FIND = 0x4BEC9A0            # serializer: find/create child node by name (rdx = name)
TEXT_WRITER = 0x6AA040      # serializer: write UTF-16 name field (rdx = name, r8 = &field)
# Value writers (int, uint, bool, enum/formatted string, u64). sprintf (0x4B8E7E0) and
# array-append (0x4BEF4D0) are not writers: they prepare the next value or element.
WRITERS = {hex(x) for x in (0x4BED070, 0x4BED170, 0x4BED210, 0x4BED300, 0x4BED1C0)}
STAFF_SERIALIZER = (0x6BBA00, 0x6BFEAC, "rsi")    # .pdata bounds, 0x44AC = 17580 bytes
PLAYER_SERIALIZER = (0x6AE330, 0x6BB9B2, "rdi")
STAT_SET = (0x2E5D7C0, 0x2E5F6FA, 0x773298C, 0x95)  # function, end, jump table, cases
STAT_GET = (0x2E07200, 0x2E07441, 0x7731004, 0x108)
STAT_TAILS = {0x2E5F6E6, 0x2E5F6F0, 0x2E5F6F2}

# Byte-exact anchors. Each is a code site whose meaning is stated in the name.
ANCHORS = [
    # rotations
    ("rot: GetHasAdvancedRotation = team+0x154C & 1", 0x2F86CF0, "0fb6814c1500002401c3"),
    ("rot: GetAdvancedRotationPlayerIdByIndex seg<0x30, slot<5, u16 team+0x1360+seg*10+slot*2", 0x2F86D1D,
     "83fa3073204183f805734589d0488d0480488d04414489c10fb7844860130000"),
    ("rot: menu commit block base team+0x1360", 0x32D357C, "4881c160130000"),
    ("rot: menu commit value = player UniqueId (+0x128)", 0x32D3602, "4a8b4cf6080fb78928010000"),
    ("rot: menu commit stores u16 cell", 0x32D35E0, "66894c4500"),
    ("rot: menu commit sets team+0x154C bit 0", 0x32D39D8, "80884c15000001"),
    ("rot: gameplan apply clears team+0x154C bit 0", 0x22FA392, "80a64c150000fe"),
    ("rot: gameplan apply validates 0x1E0 bytes", 0x22FA6D1, "4981ffe0010000"),
    ("rot: gameplan apply sets team+0x154C bit 0", 0x22FA6DE, "808e4c15000001"),
    ("rot: gameplan apply copies u16 into team+0x1360", 0x22FA718, "66898c0660130000"),
    # seasons
    ("season: PLAYERSTATMANAGERDATA::Init pool+0x18/+0x20 callbacks", 0x2F0B760,
     "488d051902feff48894618488d05de01feff48894620"),
    ("season: PLAYERSTATMANAGERDATA::Init pool+0x28/+0x30 callbacks", 0x2F0B83A,
     "488d052f500c0048894628488d05e44f0c0048894630"),
    ("season: pool GetItem idx < owner+0x1D0, owner+0x1D8 + idx*64", 0x2EEB9AB,
     "3bb0d0010000731689f148c1e106480388d8010000"),
    ("season: PlayerStatData_Get private array guard player+0xE0", 0x2E06FCB, "4883b9e000000000"),
    ("season: private record = [player+0xE0] + slot*64", 0x2E0706E, "480fbfc748c1e006480386e00000004883c478"),
    ("season: slot < 0x21, index = i16 player+0x130+slot*2", 0x2E0708B, "6683ff21731489c00fb7844630010000"),
    ("season: PLAYERSTATDATA::Clear (WPF ClearSeasonForPool masks)", 0x2E5B330,
     "c5f857c0c5fc114110c5fc1101816130000000f0c741340000000048b8000000f8000000f048214138c5f877c3"),
    ("season: GetYear = byte +0x11 + 1900", 0x2E5B470, "0fb64111056c070000c3"),
    ("season: Acquire free chain head=u16 pool+0, next=u16 rec+0x12, table=pool+8", 0x2E5BE42,
     "c1e70648037e080fb74712668906"),
    ("season: Release refcount = (+0x3C >> 13) & 0xFF", 0x2FD0835,
     "8b413c89c2c1ea0d0fb6d283fa0183d200c1e20d25ff1fe0ff01"),
    ("season: player serializer SeasonStats[0] = i16 player+0x130", 0x6AF2FF, "0fbf9730010000"),
    ("season: staff serializer SeasonStats i16 staff+0xB8+k*2", 0x6BBCF3, "420fbf9466b8000000"),
    ("season: staff SeasonStats count 0x34", 0x6BBD07, "4983fc34"),
    # menu identities used by WPF's season-import SafeMenu()
    ("menu: id getter 0x079964C0", 0x317C380, "4889d0c702c0649907c3"),
    ("menu: id getter 0x0B888544", 0x35F7F70, "4889d0c7024485880bc3"),
    ("menu: id getter 0x6509837B", 0x3130120, "4889d0c7027b830965c3"),
    ("menu: stage = ROOT+0xEA8", 0x32FB9B6, "8b81a80e0000"),
]
# (name, rva of instruction, opcode prefix, expected RIP target)
RIP_ANCHORS = [
    ("menu: current index", 0x300D703, "486305", 0xAD07BD0),
    ("menu: modes table", 0x308C649, "488d05", 0xAD07408),
    ("menu: alternate index", 0x308C650, "48630d", 0xAD07BCC),
    ("menu: settings owner pointer", 0x25AA3D0, "488b05", 0xAB5CE20),
    ("season: pool cb +0x18 GetItem", 0x2F0B760, "488d05", 0x2EEB980),
    ("season: pool cb +0x20 GetCount", 0x2F0B76B, "488d05", 0x2EEB950),
    ("season: pool cb +0x28 AddRef", 0x2F0B83A, "488d05", 0x2FD0870),
    ("season: pool cb +0x30 Release", 0x2F0B845, "488d05", 0x2FD0830),
]
# "cmp byte ptr [rip+X], 0": the immediate follows the displacement.
ACTIVE_ANCHOR = ("menu: active-game byte", 0x24F7EC5, "803d", 0xAB5D790, 1)
CALL_ANCHORS = [
    ("rot: MENU_ROTATIONS_TABLE INIT applies gameplan rotation", 0x32CDB87, 0x22FA050),
    ("rot: rotations-table confirm commits advanced rotation", 0x32D2F87, 0x32D3520),
]
STRING_REFS = [  # (string, instruction rva that leas it)
    ("TEAMDATA::GetAdvancedRotationPlayerIdByIndex", 0x2F86DCC),
    ("TEAMDATA::SetAdvancedRotationPlayerIdByIndex", 0x32D35A2),
    ("MENU_ROTATIONS_TABLE::HandleEvent_INIT", 0x32CDFB3),
    ("PLAYERSTATMANAGERDATA::Init", 0x2F0B817),
    ("PLAYERSTATMANAGERDATA::Acquire", 0x2E5BC84),
    ("PlayerStatData_Get", 0x2E0705A),
    ("PLAYERDATA::GetSeasonStatsByIndex", 0x2E07104),
    ("PLAYERSTATDATA::SetReferenceCount", 0x2FD093F),
]
# Old (Sep11 0x6AA2013D) RVA -> V2 RVA, same byte length. From SeasonImportSnapshot.cs /
# SeasonStats.cs / TeamStaff.cs. Lengths are re-measured on V2 (pdata or ret position).
OLD_TO_V2 = [
    ("staff serializer", 0x6B5900, 17580, 0x6BBA00),
    ("PlayerStatData_Get", 0x2DD22C0, 561, 0x2E06FC0),
    ("PLAYERSTATDATA::GetByIndex (WPF 'native getter')", 0x2DD2500, 577, 0x2E07200),
    ("season team-pointer rebase", 0x2E25380, 40, 0x2E5B300),
    ("PLAYERSTATDATA::Clear", 0x2E253B0, 45, 0x2E5B330),
    ("PLAYERSTATDATA::GetYear", 0x2E254F0, 10, 0x2E5B470),
    ("PLAYERSTATMANAGERDATA::Acquire", 0x2E25CA0, 594, 0x2E5BC20),
    ("pool cb +0x30 Release", 0x2F9A760, 60, 0x2FD0830),
    ("pool cb +0x28 AddRef", 0x2F9A7A0, 232, 0x2FD0870),
    ("pool cb +0x18 GetItem", 0x2EB59E0, None, 0x2EEB980),
    ("pool cb +0x20 GetCount", 0x2EB59B0, None, 0x2EEB950),
]
# WPF menu statics (SeasonImportCommit.cs) -> V2 (see also src/blacktop_observer_profile.rs VERSION_TWO).
MENU_STATICS = {"modes": (0xAC77660, 0xAD07408), "current": (0xAC77E28, 0xAD07BD0), "alternate": (0xAC77E24, 0xAD07BCC),
                "settings": (0xAACB9A0, 0xAB5CE20), "active": (0xAACC310, 0xAB5D790), "root(stage=+0xEA8)": (0xAEB69D0, 0xAF46870)}

# WPF decode() / TeamStaffPatch numeric fields: (key, byte offset, shift, bits, signed).
WPF_STAFF_DECODE = [
    ("uniqueId", 0x120, 0, 16, False), ("faceId", 0x12A, 0, 15, False), ("soundId", 0x128, 0, 16, True),
    ("playerUniqueId", 0xA8, 0, 32, False), ("wage", 0x124, 0, 32, False), ("growthPotential", 0x131, 0, 8, False),
    ("yearsWithTeam", 0x134, 0, 7, False), ("yearsInLeague", 0x134, 7, 7, False),
    ("preferredSystem", 0x130, 20, 4, False), ("activeSystem", 0x130, 24, 4, False), ("slot/role", 0x182, 0, 5, False),
]
# SeasonImportModel.cs TotalFields / AwardFields (dword offset, shift, bits) and (byte, shift, bits, max).
WPF_SEASON_TOTALS = [("ftm", 0x20, 0, 11), ("fta", 0x20, 11, 11), ("fgm", 0x18, 13, 13), ("fga", 0x14, 15, 14),
                     ("threeMade", 0x24, 12, 12), ("threeAttempted", 0x24, 0, 12), ("oreb", 0x20, 22, 10), ("dreb", 0x28, 0, 12),
                     ("steals", 0x1C, 0, 11), ("blocks", 0x1C, 11, 11), ("assists", 0x38, 0, 12), ("turnovers", 0x28, 12, 11),
                     ("points", 0x2C, 0, 14), ("gp", 0x14, 0, 7), ("gs", 0x38, 12, 7), ("minutes", 0x18, 0, 13),
                     ("fouls", 0x1C, 22, 10), ("doubleDoubles", 0x34, 25, 7), ("tripleDoubles", 0x14, 8, 7), ("plusMinus", 0x2C, 14, 15)]
WPF_SEASON_TOTAL_SETTERS = {"ftm": "FreeThrowsMade", "fta": "FreeThrowsAttempted", "fgm": "FieldGoalsMade", "fga": "FieldGoalsAttempted",
                            "threeMade": "ThreePointShotsMade", "threeAttempted": "ThreePointShotsAttempted", "oreb": "OffensiveRebounds",
                            "dreb": "DefensiveRebounds", "steals": "Steals", "blocks": "Blocks", "assists": "Assists", "turnovers": "Turnovers",
                            "points": "TotalPoints", "gp": "GamesPlayed", "gs": "GamesStarted", "minutes": "MinutesPlayed", "fouls": "Fouls",
                            "doubleDoubles": "DoubleDoubles", "tripleDoubles": "TripleDoubles", "plusMinus": "PlusMinus"}
# SeasonStats.cs SeasonAwardFields (byte, shift, bits, max) with the setter each one maps to.
WPF_SEASON_AWARDS = [("mvp", 20, 7, 1, "MVP"), ("champion", 43, 6, 1, "Champion"), ("finalsMvp", 43, 7, 1, "FinalsMVP"),
                     ("roy", 27, 5, 1, "RookieOfTheYear"), ("allNba", 23, 5, 2, "AllNBATeamRank"), ("allDefense", 47, 5, 2, "AllDefensiveTeamRank"),
                     ("allRookie", 27, 2, 2, "AllRookieTeamRank"), ("sixthMan", 27, 6, 1, "SixthManOfTheYear"),
                     ("dpoy", 27, 7, 1, "DefensivePlayerOfTheYear"), ("mip", 47, 7, 1, "MostImprovedPlayer"),
                     ("allStarStarter", 39, 7, 1, "AllStarStarter"), ("allStarReserve", 27, 4, 1, "AllStarReserve")]

# --------------------------------------------------------------------------------------------
# Symbolic bit evaluation of x64 load/shift/mask sequences.

_FAMILIES = {}
for _f, names in {
    "a": ("rax", "eax", "ax", "al", "ah"), "b": ("rbx", "ebx", "bx", "bl", "bh"), "c": ("rcx", "ecx", "cx", "cl", "ch"),
    "d": ("rdx", "edx", "dx", "dl", "dh"), "si": ("rsi", "esi", "si", "sil", None), "di": ("rdi", "edi", "di", "dil", None),
    "bp": ("rbp", "ebp", "bp", "bpl", None),
}.items():
    for name, lo, width in zip(names, (0, 0, 0, 0, 8), (64, 32, 16, 8, 8)):
        if name:
            _FAMILIES[name] = (_f, lo, width)
for _n in range(8, 16):
    for suffix, width in (("", 64), ("d", 32), ("w", 16), ("b", 8)):
        _FAMILIES[f"r{_n}{suffix}"] = (f"r{_n}", 0, width)
_SIZES = {"byte": 8, "word": 16, "dword": 32, "qword": 64}
_IMM = r"(-?0x[0-9a-f]+|-?\d+)"


def _imm(text):
    return int(text, 16) if "0x" in text else int(text)


class BitState:
    """Per register family: list of 64 entries, each None (zero), int (source bit) or ('s', bit) (sign copy)."""

    def __init__(self, base):
        self.base = base
        self.regs = {}
        self.const = {}          # family -> immediate from "mov reg, imm"
        self.order = []          # families in the order they were last written
        self.loads = []          # (disp, size) loads from the base register
        self.text = []           # lea rX, [base + disp]

    def _touch(self, fam):
        if fam in self.order:
            self.order.remove(fam)
        self.order.append(fam)

    def _get(self, reg):
        fam, lo, width = _FAMILIES[reg]
        bits = self.regs.get(fam, [None] * 64)
        return bits[lo:lo + width]

    def _set(self, reg, value):
        fam, lo, width = _FAMILIES[reg]
        bits = list(self.regs.get(fam, [None] * 64))
        if width == 32:
            bits = [None] * 64
        bits[lo:lo + width] = value[:width] + [None] * (width - len(value))
        self.regs[fam] = bits
        self._touch(fam)

    def _drop(self, reg):
        fam = _FAMILIES[reg][0]
        self.regs[fam] = [None] * 64
        self._touch(fam)

    def step(self, mnemonic, ops):
        b = self.base
        mem = re.fullmatch(rf"(\w+) ptr \[{b}(?: \+ (0x[0-9a-f]+|\d+))?\]", ops.split(", ", 1)[1]) if ", " in ops else None
        dst = ops.split(", ", 1)[0]
        if mnemonic == "lea":
            m = re.fullmatch(rf"(\w+), \[{b}(?: \+ (0x[0-9a-f]+))?\]", ops)
            if m:
                self.text.append(int(m.group(2) or "0", 16))
            if dst in _FAMILIES:
                self._drop(dst)
            return
        if dst not in _FAMILIES:
            return
        width = _FAMILIES[dst][2]
        if mem and mnemonic == "and" and _FAMILIES[dst][0] in self.const:
            # mov reg, imm ; and reg, [base+disp]  ->  masked load
            mask, disp, size = self.const.pop(_FAMILIES[dst][0]), int(mem.group(2) or "0", 0), _SIZES[mem.group(1)]
            self.loads.append((disp, size))
            self._set(dst, [disp * 8 + i if (mask >> i) & 1 else None for i in range(size)])
            return
        imm = re.fullmatch(rf"\w+, {_IMM}", ops)
        if mnemonic == "mov" and imm:
            self._drop(dst)
            self.const[_FAMILIES[dst][0]] = _imm(imm.group(1)) & ((1 << width) - 1)
            return
        self.const.pop(_FAMILIES[dst][0], None)
        if mem and mnemonic in ("mov", "movzx", "movsx", "movsxd"):
            disp, size = int(mem.group(2) or "0", 0), _SIZES[mem.group(1)]
            self.loads.append((disp, size))
            src = [disp * 8 + i for i in range(size)]
            fill = ("s", src[-1]) if mnemonic.startswith("movsx") else None
            self._set(dst, src + [fill] * (width - size))
            return
        src = ops.split(", ", 1)[1] if ", " in ops else None
        if mnemonic in ("mov", "movzx", "movsx", "movsxd") and src in _FAMILIES:
            val = self._get(src)
            fill = None
            if mnemonic.startswith("movsx") and val:
                top = val[-1]
                fill = top if (isinstance(top, tuple) or top is None) else ("s", top)
            self._set(dst, val + [fill] * (width - len(val)))
            return
        m = re.fullmatch(_IMM, src or "")
        if m and mnemonic in ("shr", "sar", "shl", "and"):
            n, val = _imm(m.group(1)), self._get(dst)
            if mnemonic == "shr":
                val = val[n:] + [None] * n
            elif mnemonic == "sar":
                top = val[-1]
                fill = top if isinstance(top, tuple) or top is None else ("s", top)
                val = val[n:] + [fill] * n
            elif mnemonic == "shl":
                val = [None] * n + val[:width - n]
            else:
                mask = n & ((1 << width) - 1)
                val = [v if (mask >> i) & 1 else None for i, v in enumerate(val)]
            self._set(dst, val)
            return
        if mnemonic in ("test", "cmp", "bt"):
            return
        self._drop(dst)

    def field(self):
        """The most recently written family that still carries source bits -> bit range."""
        for fam in reversed(self.order):
            bits = self.regs.get(fam) or []
            src = [b for b in bits if isinstance(b, int)]
            if src:
                signed = any(isinstance(b, tuple) for b in bits)
                lo = min(src)
                contiguous = sorted(src) == list(range(lo, lo + len(src)))
                return {"bit": lo, "bits": len(src), "signed": signed, "contiguous": contiguous,
                        "scale": next(i for i, b in enumerate(bits) if b == lo)}
        return None


# --------------------------------------------------------------------------------------------

def decoder():
    md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_64)
    md.detail = False
    return md


def disasm(img, start, end):
    return list(decoder().disasm(img.read(start, end - start), start))


def rip_target(i):
    m = re.search(r"\[rip ([+-]) (0x[0-9a-f]+)\]", i.op_str)
    return None if not m else i.address + i.size + (int(m.group(2), 16) * (1 if m.group(1) == "+" else -1))


def cstr(img, rva, n=96):
    raw = img.read(rva, n).split(b"\0")[0]
    return raw.decode("ascii") if raw and all(32 <= c < 127 for c in raw) else None


def serializer_fields(img, start, end, base):
    """Split a named serializer into per-field segments and evaluate each one."""
    ins = disasm(img, start, end)
    names = []
    for k, i in enumerate(ins):
        if i.mnemonic == "lea" and i.op_str.startswith("rdx, [rip"):
            for j in range(k + 1, min(k + 7, len(ins))):
                if ins[j].mnemonic == "call":
                    target = int(ins[j].op_str, 16) if ins[j].op_str.startswith("0x") else None
                    if target in (FIND, TEXT_WRITER):
                        name = cstr(img, rip_target(i))
                        if name:
                            names.append((k, j, name, target))
                    break
    fields, prev = [], 0
    for n, (k, j, name, target) in enumerate(names):
        # A field ends at its last writer call before the next field's name: loads of the
        # next field may precede its name, and arrays append one element per call.
        nxt = names[n + 1][0] if n + 1 < len(names) else len(ins)
        calls = [x for x in range(j + 1, nxt) if ins[x].mnemonic == "call" and ins[x].op_str in WRITERS]
        w = j if target == TEXT_WRITER or not calls else calls[-1]
        st = BitState(base)
        for i in ins[prev:w + 1]:
            st.step(i.mnemonic, i.op_str)
        prev = w + 1
        entry = {"name": name, "site": f"0x{ins[k].address:X}"}
        if target == TEXT_WRITER and st.text:
            entry.update(kind="text", offset=st.text[-1])
        elif len(st.loads) > 2:
            entry.update(kind="array", offsets=[d for d, _ in st.loads], elementBits=st.loads[0][1])
        else:
            f = st.field()
            if f:
                entry.update(kind="bits", **f)
            elif st.loads:
                d, s = st.loads[-1]
                entry.update(kind="bits", bit=d * 8, bits=s, signed=False, contiguous=True, scale=0)
            else:
                entry.update(kind="node")
        fields.append(entry)
    return fields


def switch_cases(img, table, count):
    return [(n + 1, table + struct.unpack("<i", img.read(table + 4 * n, 4))[0]) for n in range(count)]


def stat_setters(img):
    start, end, table, count = STAT_SET
    ins = disasm(img, start, end)
    at = {i.address: k for k, i in enumerate(ins)}
    out = {}
    for sid, case in switch_cases(img, table, count):
        if case in STAT_TAILS or case not in at:
            continue
        k, store, imms, name = at[case], None, {}, None
        while k < len(ins):
            i = ins[k]
            m = re.fullmatch(r"(\w+), " + _IMM, i.op_str)
            if i.mnemonic in ("mov", "and") and m:
                imms[m.group(1)] = (i.mnemonic, _imm(m.group(2)) & 0xFFFFFFFF)
            m = re.fullmatch(r"dword ptr \[rcx(?: \+ (0x[0-9a-f]+))?\], (\w+)", i.op_str)
            if i.mnemonic == "mov" and m and store is None:
                store = (int(m.group(1) or "0", 16), m.group(2))
            if i.mnemonic == "lea" and i.op_str.startswith("r9, [rip"):
                name = cstr(img, rip_target(i))
            if i.mnemonic in ("jmp", "ret") and (i.mnemonic == "ret" or int(i.op_str, 16) in STAT_TAILS):
                break
            k += 1
        if not store or not name or store[1] not in imms:
            continue
        keep = imms[store[1]][1]
        mask = ~keep & 0xFFFFFFFF
        lo = (mask & -mask).bit_length() - 1
        bits = bin(mask).count("1")
        out[sid] = {"name": name.split("::Set", 1)[-1], "bit": store[0] * 8 + lo, "bits": bits,
                    "contiguous": mask == ((1 << bits) - 1) << lo}
    return out


def stat_getters(img):
    start, end, table, count = STAT_GET
    ins = disasm(img, start, end + 1)
    at = {i.address: k for k, i in enumerate(ins)}
    out = {}
    for sid, case in switch_cases(img, table, count):
        if case not in at:
            continue
        st, k, derived = BitState("rcx"), at[case], False
        while k < len(ins) and ins[k].mnemonic != "ret":
            i = ins[k]
            derived |= i.mnemonic.startswith("set") or i.mnemonic in ("cmp", "bt", "adc")
            st.step(i.mnemonic, i.op_str)
            k += 1
        f = st.field()
        if f and not derived:
            out[sid] = f
    return out


def function_length(img, rva, pdata_end=None):
    """.pdata length, or for a leaf function the offset of the first padding after a ret."""
    if pdata_end:
        return pdata_end - rva
    after_ret = False
    for i in decoder().disasm(img.read(rva, 0x400), rva):
        if after_ret and (i.mnemonic in ("int3", "nop")):
            return i.address - rva
        after_ret = i.mnemonic == "ret"
    return None


def pdata_end(img, rva):
    s = next(x for x in img.sections if x[0] == ".pdata")
    raw = img.read(s[1], s[4])
    for k in range(0, len(raw) - 11, 12):
        b, e, _ = struct.unpack_from("<III", raw, k)
        if b == rva:
            return e
    return None


# --------------------------------------------------------------------------------------------

def check_anchors(img):
    out = []
    for name, rva, want in ANCHORS:
        try:
            got = img.read(rva, len(want) // 2).hex()
        except ValueError:
            got = None
        out.append({"name": name, "rva": f"0x{rva:X}", "ok": got == want})
    for name, rva, prefix, target in RIP_ANCHORS + [ACTIVE_ANCHOR[:4]]:
        extra = ACTIVE_ANCHOR[4] if name == ACTIVE_ANCHOR[0] else 0
        p = bytes.fromhex(prefix)
        try:
            raw = img.read(rva, len(p) + 4)
            got = rva + len(p) + 4 + extra + struct.unpack_from("<i", raw, len(p))[0] if raw[:len(p)] == p else None
        except ValueError:
            got = None
        out.append({"name": name, "rva": f"0x{rva:X}", "target": f"0x{target:X}", "ok": got == target})
    for name, rva, target in CALL_ANCHORS:
        try:
            raw = img.read(rva, 5)
            got = rva + 5 + struct.unpack_from("<i", raw, 1)[0] if raw[0] == 0xE8 else None
        except ValueError:
            got = None
        out.append({"name": name, "rva": f"0x{rva:X}", "target": f"0x{target:X}", "ok": got == target})
    for text, rva in STRING_REFS:
        try:
            i = next(decoder().disasm(img.read(rva, 15), rva))
            got = cstr(img, rip_target(i), len(text) + 1) if i.mnemonic == "lea" else None
        except (ValueError, StopIteration):
            got = None
        out.append({"name": f"string ref '{text}'", "rva": f"0x{rva:X}", "ok": got == text})
    return out


# Enum name tables (absolute char* arrays) that the staff serializer indexes with the field value.
ENUM_TABLES = [("Job", 0x685D0A0, 32), ("PlaybookID", 0x685BF50, 369)]


def enum_tables(img, wpf_ref):
    start, end, _ = STAFF_SERIALIZER
    referenced = {rip_target(i) for i in disasm(img, start, end) if i.mnemonic == "lea" and i.op_str.startswith("rcx, [rip")}
    roles = None
    schema = wpf_ref / "data/staff-field-schema.json"
    if schema.exists():
        roles = json.loads(schema.read_text(encoding="utf-8")).get("roles")
    out = []
    for name, rva, count in ENUM_TABLES:
        values = []
        for k in range(count):
            p = struct.unpack("<Q", img.read(rva + 8 * k, 8))[0] - img.base
            values.append(cstr(img, p, 64) if 0 < p < img.size_of_image else None)
        row = {"field": name, "table": f"0x{rva:X}", "referenced": rva in referenced, "count": count,
               "allNamed": all(values), "first": values[:3], "last": values[-2:]}
        if name == "Job" and roles is not None:
            row["matchesWpfRoles"] = values == roles
        out.append(row)
    return out


def old_to_v2(img):
    out = []
    for name, old, length, rva in OLD_TO_V2:
        n = function_length(img, rva, pdata_end(img, rva))
        out.append({"name": name, "old": f"0x{old:X}", "v2": f"0x{rva:X}", "oldLength": length, "v2Length": n,
                    "sha256": hashlib.sha256(img.read(rva, n)).hexdigest() if n else None,
                    "ok": length is None or n == length})
    return out


def _range(offset, shift, bits):
    return offset * 8 + shift, bits


def compare_staff(fields, wpf_ref, helper):
    """Every WPF staff number/badge/control must equal (or sit inside) one V2 serializer field."""
    numeric = [f for f in fields if f.get("kind") == "bits"]

    def match(bit, bits):
        for f in numeric:
            if f["bit"] == bit and f["bits"] == bits:
                return f["name"], "exact"
        for f in numeric:
            if f["bit"] <= bit and bit + bits <= f["bit"] + f["bits"]:
                return f["name"], f"inside {f['bits']}-bit field"
        return None, "unmapped"

    rows = []
    for key, off, shift, bits, signed in WPF_STAFF_DECODE:
        name, how = match(*_range(off, shift, bits))
        rows.append({"source": "decode()", "key": key, "offset": off, "shift": shift, "bits": bits, "v2": name, "match": how})
    texts = {f["offset"]: f["name"] for f in fields if f.get("kind") == "text"}
    schema_path, controls_path = wpf_ref / "data/staff-field-schema.json", wpf_ref / "data/staff-controls.json"
    if controls_path.exists():
        for c in json.loads(controls_path.read_text(encoding="utf-8")):
            if c.get("text"):
                rows.append({"source": "staff-controls", "key": c["key"], "offset": c["offset"], "bits": c["bytes"] * 8,
                             "v2": texts.get(c["offset"]), "match": "exact" if c["offset"] in texts else "unmapped"})
                continue
            name, how = match(*_range(c["offset"], c.get("shift", 0), c["bits"]))
            rows.append({"source": "staff-controls", "key": c["key"], "offset": c["offset"], "shift": c.get("shift", 0),
                         "bits": c["bits"], "v2": name, "match": how})
    if schema_path.exists():
        schema = json.loads(schema_path.read_text(encoding="utf-8"))
        for b in schema.get("badges", []):
            name, how = match(*_range(b["offset"], b["shift"], b["bits"]))
            rows.append({"source": "schema.badges", "key": b["key"], "offset": b["offset"], "shift": b["shift"],
                         "bits": b["bits"], "v2": name, "match": how if name == b["key"] else f"{how}, name differs"})
    masks = None
    patch = helper / "TeamStaffPatch.cs"
    if patch.exists():
        m = re.search(r"Masks=new byte\[\]\{([0-9,]+)\}", patch.read_text(encoding="utf-8"))
        masks = [int(x) for x in m.group(1).split(",")] if m else None
    editable = None
    if masks and len(masks) == 0x1C8:
        covered, outside = {}, []
        locked = {"UniqueId", "Job", "PlayerUniqueId", "UniqueSignatureId", "AudioSignatureId", "NPCSignatureId"}
        for off, mbyte in enumerate(masks):
            for b in range(8):
                if mbyte >> b & 1:
                    bit = off * 8 + b
                    owner = next((f["name"] for f in numeric if f["bit"] <= bit < f["bit"] + f["bits"]), None)
                    owner = owner or next((f["name"] for f in fields if f.get("kind") == "text" and f["offset"] * 8 <= bit < f["offset"] * 8 + 320), None)
                    if owner is None or owner in locked:
                        outside.append(f"0x{off:X}.{b}")
                    else:
                        covered[owner] = covered.get(owner, 0) + 1
        editable = {"maskBits": sum(bin(x).count("1") for x in masks), "fields": covered, "outsideNamedFields": outside}
    return rows, editable


def compare_seasons(setters, getters):
    by_name = {v["name"]: (sid, v) for sid, v in setters.items()}
    rows = []
    for key, off, shift, bits in WPF_SEASON_TOTALS:
        sid, v = by_name.get(WPF_SEASON_TOTAL_SETTERS[key], (None, None))
        g = getters.get(sid) if sid else None
        rows.append({"key": key, "wpf": [off, shift, bits], "setter": v and v["name"], "statId": sid,
                     "setterOk": bool(v and v["bit"] == off * 8 + shift and v["bits"] == bits),
                     "getterOk": bool(g and g["bit"] == off * 8 + shift and g["bits"] == bits)})
    for key, byte, shift, bits, setter in WPF_SEASON_AWARDS:
        sid, v = by_name.get(setter, (None, None))
        g = getters.get(sid) if sid else None
        rows.append({"key": key, "wpf": [byte, shift, bits], "setter": v and v["name"], "statId": sid,
                     "setterOk": bool(v and v["bit"] == byte * 8 + shift and v["bits"] == bits),
                     "getterOk": bool(g and g["bit"] == byte * 8 + shift and g["bits"] == bits)})
    agree = sum(1 for sid, v in setters.items() if sid in getters and getters[sid]["bit"] == v["bit"] and getters[sid]["bits"] == v["bits"])
    return rows, {"setters": len(setters), "gettersWithPlainField": len(getters), "setterGetterAgree": agree}


def helper_constants_match(helper):
    """The embedded WPF season constants must equal the helper source they were copied from."""
    model = helper / "SeasonImportModel.cs"
    if not model.exists():
        return None
    m = re.search(r"TotalFields=\{(.*?)\};", model.read_text(encoding="utf-8"))
    triples = [tuple(int(x, 0) for x in t.split(",")) for t in re.findall(r"\{([0-9xA-Fa-f,]+)\}", m.group(1))] if m else []
    return triples == [(o, s, b) for _, o, s, b in WPF_SEASON_TOTALS]


def audit(img, wpf_ref=DEFAULT_WPF_REF, helper=DEFAULT_HELPER):
    staff = serializer_fields(img, *STAFF_SERIALIZER)
    player = serializer_fields(img, *PLAYER_SERIALIZER)
    staff_rows, editable = compare_staff(staff, wpf_ref, helper)
    setters, getters = stat_setters(img), stat_getters(img)
    season_rows, season_summary = compare_seasons(setters, getters)
    player_seasons = next((f for f in player if f["name"] == "SeasonStats"), None)
    report = {
        "identity": {"stamp": f"0x{img.stamp:X}", "sizeOfImage": f"0x{img.size_of_image:X}",
                     "ok": img.stamp == EXPECTED_STAMP and img.size_of_image == EXPECTED_IMAGE_SIZE},
        "anchors": check_anchors(img),
        "oldToV2": old_to_v2(img),
        "menuStatics": {k: {"old": f"0x{a:X}", "v2": f"0x{b:X}"} for k, (a, b) in MENU_STATICS.items()},
        "staffFields": staff,
        "staffVsWpf": staff_rows,
        "staffEditableMask": editable,
        "staffEnums": enum_tables(img, wpf_ref),
        "playerSeasonDirectory": player_seasons,
        "seasonSetters": {str(k): v for k, v in sorted(setters.items())},
        "seasonVsWpf": season_rows,
        "seasonSummary": season_summary,
        "helperConstantsMatch": helper_constants_match(helper),
    }
    report["ok"] = summary_ok(report)
    return report


def summary_ok(r):
    staff_ok = all(row["match"] != "unmapped" and "differs" not in row["match"] for row in r["staffVsWpf"])
    mask_ok = r["staffEditableMask"] is None or not r["staffEditableMask"]["outsideNamedFields"]
    season_ok = all(row["setterOk"] and row["getterOk"] for row in r["seasonVsWpf"])
    pd = r["playerSeasonDirectory"]
    dir_ok = bool(pd and pd.get("offsets") == [0x130 + 2 * k for k in range(33)] and pd.get("elementBits") == 16)
    enum_ok = all(e["referenced"] and e["allNamed"] and e.get("matchesWpfRoles", True) for e in r["staffEnums"])
    return (r["identity"]["ok"] and all(a["ok"] for a in r["anchors"]) and all(o["ok"] for o in r["oldToV2"])
            and staff_ok and mask_ok and season_ok and dir_ok and enum_ok and r["helperConstantsMatch"] is not False)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("exe")
    ap.add_argument("--wpf-ref", type=pathlib.Path, default=DEFAULT_WPF_REF)
    ap.add_argument("--helper-src", type=pathlib.Path, default=DEFAULT_HELPER)
    ap.add_argument("--json", type=pathlib.Path)
    a = ap.parse_args(argv)
    img = Image(pathlib.Path(a.exe).read_bytes())
    r = audit(img, a.wpf_ref, a.helper_src)
    bad = [x["name"] for x in r["anchors"] if not x["ok"]] + [x["name"] for x in r["oldToV2"] if not x["ok"]]
    print(f"identity ok={r['identity']['ok']}  anchors {sum(x['ok'] for x in r['anchors'])}/{len(r['anchors'])}"
          f"  old->V2 {sum(x['ok'] for x in r['oldToV2'])}/{len(r['oldToV2'])}")
    print(f"staff serializer fields: {len(r['staffFields'])}; WPF rows: "
          f"{sum(x['match'] == 'exact' for x in r['staffVsWpf'])} exact, "
          f"{sum(x['match'].startswith('inside') for x in r['staffVsWpf'])} inside, "
          f"{sum(x['match'] == 'unmapped' or 'differs' in x['match'] for x in r['staffVsWpf'])} unmapped/renamed")
    if r["staffEditableMask"]:
        print(f"WPF editable staff bits: {r['staffEditableMask']['maskBits']}, outside named V2 fields: "
              f"{r['staffEditableMask']['outsideNamedFields'] or 'none'}")
    print(f"season: {r['seasonSummary']}; WPF rows ok "
          f"{sum(x['setterOk'] and x['getterOk'] for x in r['seasonVsWpf'])}/{len(r['seasonVsWpf'])}")
    for x in bad:
        print("  MISMATCH:", x)
    for row in r["staffVsWpf"]:
        if row["match"] == "unmapped" or "differs" in row["match"]:
            print("  STAFF:", row)
    print("RESULT:", "PASS" if r["ok"] else "FAIL")
    if a.json:
        a.json.write_text(json.dumps(r, indent=1), encoding="utf-8")
    return 0 if r["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
