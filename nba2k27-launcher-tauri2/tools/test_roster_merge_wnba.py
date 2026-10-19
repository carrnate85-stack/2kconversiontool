"""Tests for the offline roster codec and WNBA merge (no game process access).

Synthetic tests always run. Stock-file tests run when the decoded rosters and
the installed executable exist (override with C27_ROSTER_DECODED / C27_GAME_EXE)
and are skipped otherwise. Run: python -I tools/test_roster_merge_wnba.py
"""
import io
import os
import pathlib
import struct
import sys
import unittest
import zipfile
import zlib

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import rdat_roster as rr  # noqa: E402
import roster_merge_wnba as rm  # noqa: E402
from vcblowfish import Blowfish  # noqa: E402

SALT = b'unit-test-salt-22bytes'
DECODED = pathlib.Path(os.environ.get(
    'C27_ROSTER_DECODED', r'C:\Users\carrn\Documents\Codex\2026-10-17\task\roster-scratch\decoded'))
EXE = pathlib.Path(os.environ.get(
    'C27_GAME_EXE', r'C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27\NBA2K27.exe'))
FF = 0xFFFFFFFF


# ---------- synthetic roster builder ----------

def field(off, width, ftype, type_hash, name, index, count=None, parent=0):
    return [off, width, ftype, type_hash, name, parent, index, 1 if count else 0, 0, 0, 0,
            0x1451DAB1 if count else 0, 0, 0, 0, count if count else FF, FF, FF, FF]


def table_chunk(tid, type_hash, fields, rows, bits):
    data = rr.pack_rows(rows, bits)
    total = len(rows) * bits
    aux = [total >> 32, total & FF, 0, type_hash, tid, FF, 0, 1, 0, 0, 0, 0x1451DAB1, 0, 0, 0, len(rows), FF, FF, FF]
    for f in fields:
        aux += f
    aux = struct.pack(f'>{len(aux)}I', *aux)
    hdr = struct.pack('<13I', tid, len(data), len(aux), 0, zlib.crc32(aux + SALT), 0, len(data),
                      zlib.crc32(data + SALT), 0, 0, 0, 0x12345678, 0)
    body = data + aux
    return hdr + body + bytes(rr.padded(len(body)) - len(body))


def build_iff(tables):
    """tables: list of (tid, type_hash, fields, rows, bits)."""
    chunks = [table_chunk(*t) for t in tables]
    body = b'\x00' * 6 + b''.join(struct.pack('>II', t[0], len(t[3])) for t in tables) + b'\x00\x00'
    dhdr = struct.pack('<13I', 0x8C76B363, 0, 0, 0, 0, 0, len(body), zlib.crc32(body + SALT), 0, 0, 0, 0, 0)
    chunks.insert(0, dhdr + body + bytes(rr.padded(len(body)) - len(body)))
    out = b''.join(b'BNDL' + struct.pack('<I', len(c) - rr.HEADER) + c for c in chunks)
    total = 0x28 + len(out)
    prefix = struct.pack('<IIII', total - 0x10, 0, 9, 0) + b'BNH(' + struct.pack('<I', total - 0x18) + bytes(16)
    zbuf = io.BytesIO()
    with zipfile.ZipFile(zbuf, 'w', zipfile.ZIP_STORED) as z:
        z.writestr('roster.RDAT', prefix + out)
    return zbuf.getvalue()


def ref(i):
    return FF << 32 | FF if i is None else (i << 32) | rr.REF_TAG


TEAM, PLAYER, STAFF, PERSON, HEADER = 0xE34FAD55, 0x72D60754, 0x4C575CCD, 0x66BFBD42, 0xCEF84E6A
T_TEAM, T_PLAYER, T_STAFF, T_PERSON, T_HEADER = 0x7C38FC71, 0x9626B685, 0x2A566FBF, 0xE0C11099, 0xB1F081E7
# team: name u16[2] | typeA 6 | typeB 6 | players ref[2] | staff ref | self ref | team id u16
TEAM_FIELDS = [field(0, 32, 22, 0x43A97F0F, 0x292A0287, 1, count=2), field(32, 6, 20, 1, 0x2CECF817, 2),
               field(38, 6, 20, 1, 0x0C732B9B, 3), field(44, 128, 21, T_PLAYER, 0xA1, 4, count=2),
               field(172, 64, 21, T_STAFF, 0xA2, 5), field(236, 64, 21, T_TEAM, 0xA3, 6),
               field(300, 16, 4, 0xA49CE182, 0x2ABD43F2, 7)]
TEAM_BITS = 316
# player: last u16[1] | first u16[1] | id 16 | team ref | contract 10
PLAYER_FIELDS = [field(0, 16, 22, 0x43A97F0F, 0x51D55A69, 1, count=1),
                 field(16, 16, 22, 0x43A97F0F, 0xF45FB0F8, 2, count=1),
                 field(32, 16, 4, 0xA49CE182, 0x31BD8AF6, 3), field(48, 64, 21, T_TEAM, 0xB1, 4),
                 field(112, 10, 5, 0x91C74719, 0x29EF21A7, 5)]
PLAYER_BITS = 122
# staff: id u16 | person ref | team ref
STAFF_FIELDS = [field(0, 16, 4, 0xA49CE182, 0x31BD8AF6, 1), field(16, 64, 21, T_PERSON, 0xC1, 2),
                field(80, 64, 21, T_TEAM, 0xC2, 3)]
STAFF_BITS = 144
PERSON_FIELDS = [field(0, 13, 5, 0x91C74719, 0xD1, 1)]
HEADER_FIELDS = [field(0, 32, 5, 0x91C74719, 0x31875000, 1), field(32, 8, 3, 0x3B9327D2, 0xA1944F81, 2)]

CFG = dict(rm.V2, player_affiliation=(0x29EF21A7,), strides={'e34fad55': 100, '72d60754': 10},
           wnba_club_ids=(300, 301), wpf_insert_after_id=1)
SNAPSHOT = pathlib.Path(__file__).resolve().parents[2] / 'research_2k27' / 'blacktop-session-17264-readonly.json'


def team_row(name, kind, players, staff, self_ref, tid):
    r = 0
    vals = [(0, 16, ord(name[0])), (16, 16, ord(name[1])), (32, 6, kind), (38, 6, kind),
            (44, 64, ref(players[0])), (108, 64, ref(players[1])), (172, 64, ref(staff)), (236, 64, ref(self_ref)),
            (300, 16, tid)]
    for off, w, v in vals:
        r = rr.put(r, TEAM_BITS, off, w, v)
    return r


def player_row(last, first, pid, team, contract):
    r = 0
    for off, w, v in [(0, 16, ord(last)), (16, 16, ord(first)), (32, 16, pid), (48, 64, ref(team)), (112, 10, contract)]:
        r = rr.put(r, PLAYER_BITS, off, w, v)
    return r


def staff_row(sid, person, team):
    r = rr.put(0, STAFF_BITS, 0, 16, sid)
    return rr.put(rr.put(r, STAFF_BITS, 16, 64, ref(person)), STAFF_BITS, 80, 64, ref(team))


def synthetic(base_variant=True, mismatch_staff=False):
    """Base mirrors NBA Learn2K (WNBA players/staff present but unaffiliated);
    source mirrors WNBA Learn2K (clubs AC/DR, national team WU with a copy player)."""
    persons = [7, 8, 9, 10]
    header = [rr.put(rr.put(0, 40, 0, 32, 1000), 40, 32, 8, 30)]
    if base_variant:
        teams = [team_row('BO', 0, (0, 1), 0, None, 1), team_row('LA', 0, (2, None), 1, 0, 2),
                 team_row('US', 22, (None, None), None, None, 69)]
        players = [player_row('a', 'b', 11, 0, 5), player_row('c', 'd', 12, 0, 6), player_row('e', 'f', 13, 1, 7),
                   player_row('g', 'h', 14, None, 0), player_row('i', 'j', 15, None, 0),
                   player_row('k', 'l', 21, None, 0)]
        staff = [staff_row(100, 0, 0), staff_row(101, 1, 1), staff_row(102, 2, None), staff_row(103, 3, None)]
    else:
        teams = [team_row('AC', 29, (3, 4), 2, 1, 300), team_row('DR', 29, (None, None), 3, None, 301),
                 team_row('XX', 5, (0, None), None, None, 9), team_row('WU', 22, (5, None), None, None, 361)]
        players = [player_row('a', 'b', 11, None, 5), player_row('c', 'd', 12, None, 6), player_row('e', 'f', 13, None, 7),
                   player_row('g', 'h', 14, 0, 90), player_row('i', 'j', 15, 0, 91), player_row('z', 'y', 21, 3, 0)]
        staff = [staff_row(100, 0, None), staff_row(101, 1, None), staff_row(999 if mismatch_staff else 102, 2, 0),
                 staff_row(103, 3, 1)]
    return build_iff([
        (HEADER, T_HEADER, HEADER_FIELDS, header, 40),
        (PLAYER, T_PLAYER, PLAYER_FIELDS, players, PLAYER_BITS),
        (TEAM, T_TEAM, TEAM_FIELDS, teams, TEAM_BITS),
        (STAFF, T_STAFF, STAFF_FIELDS, staff, STAFF_BITS),
        (PERSON, T_PERSON, PERSON_FIELDS, persons, 13),
    ])


class CodecTests(unittest.TestCase):
    def test_blowfish_vectors(self):
        self.assertEqual(Blowfish(bytes(8)).encrypt(bytes(8)).hex(), '4ef997456198dd78')
        self.assertEqual(Blowfish(b'\xff' * 8).encrypt(b'\xff' * 8).hex(), '51866fd5b85ecb8a')
        key = bytes(range(32))
        data = bytes(range(64))
        self.assertEqual(Blowfish(key).decrypt(Blowfish(key).encrypt(data)), data)

    def test_bit_packing(self):
        for bits in (1, 7, 13, 64, 495, 9721):
            rows = [(i * 0x9E3779B97F4A7C15 ^ (i << 3)) & ((1 << bits) - 1) for i in range(23)]
            packed = rr.pack_rows(rows, bits)
            self.assertEqual(len(packed), (23 * bits + 7) // 8)
            self.assertEqual(rr.unpack_rows(packed, 23, bits), rows)
        r = rr.put(0, 100, 37, 9, 0x1AB)
        self.assertEqual(rr.get(r, 100, 37, 9), 0x1AB)

    def test_chunk_padding_rule(self):
        # Observed in the stock rosters: (dataLen + auxLen) mod 8 -> extra bytes.
        for content, extra in ((0, 4), (4, 0), (1, 3), (5, 7), (2, 2), (6, 6), (3, 1), (7, 5)):
            self.assertEqual(rr.padded(800 + content) - (800 + content), extra)

    def test_synthetic_round_trip(self):
        iff = synthetic()
        r = rr.Roster(iff)
        self.assertEqual(r.check_crcs(SALT), [])
        self.assertEqual(r.iff_bytes(SALT), iff)
        for tid in r.order:
            t = r.tables[tid]
            t.set_rows(t.all_rows())
        self.assertEqual(r.iff_bytes(SALT), iff)


class MergeTests(unittest.TestCase):
    def test_clubs_merge_appends_teams_and_affiliates_in_place(self):
        base, src = rr.Roster(synthetic()), rr.Roster(synthetic(False))
        out, report, plan = rm.merge(base, src, CFG, variant='clubs')
        merged = rr.Roster(out.iff_bytes(SALT))
        self.assertEqual(rm.validate(merged, SALT, base, plan, CFG), [])
        teams = merged.tables['e34fad55'].all_rows()
        self.assertEqual(len(teams), 5)
        ac = teams[3]
        self.assertEqual(rr.get(ac, TEAM_BITS, 44, 64) >> 32, 3)            # same player index
        self.assertEqual(rr.get(ac, TEAM_BITS, 172, 64) >> 32, 2)           # same staff index
        self.assertEqual(rr.get(ac, TEAM_BITS, 236, 64) >> 32, 4)           # self ref DR 1 -> 4
        players = merged.tables['72d60754'].all_rows()
        self.assertEqual(len(players), 6)
        self.assertEqual(rr.get(players[3], PLAYER_BITS, 48, 64) >> 32, 3)  # affiliated with AC
        self.assertEqual(rr.get(players[3], PLAYER_BITS, 112, 10), 90)       # contract copied
        self.assertEqual(players[0], base.tables['72d60754'].row(0))         # NBA player untouched
        staff = merged.tables['4c575ccd'].all_rows()
        self.assertEqual(len(staff), 4)
        self.assertEqual(rr.get(staff[2], STAFF_BITS, 80, 64) >> 32, 3)     # staff affiliated in place
        self.assertEqual(staff[0], base.tables['4c575ccd'].row(0))
        hdr = merged.tables['cef84e6a'].row(0)
        self.assertEqual(rr.get(hdr, 40, 0, 32), 1000 + (512 - 304))        # +2 teams
        self.assertEqual(rr.get(hdr, 40, 32, 8), 30)                         # league team count kept

    def test_wpf_merge_adds_national_team_with_unique_ids(self):
        base, src = rr.Roster(synthetic()), rr.Roster(synthetic(False))
        out, report, plan = rm.merge(base, src, CFG, variant='national')
        merged = rr.Roster(out.iff_bytes(SALT))
        self.assertEqual(rm.validate(merged, SALT, base, plan, CFG), [])
        teams = merged.tables['e34fad55'].all_rows()
        self.assertEqual([rr.get(t, TEAM_BITS, 300, 16) for t in teams], [1, 2, 69, 300, 301, 361])
        players = merged.tables['72d60754'].all_rows()
        self.assertEqual(len(players), 7)                                    # national copy appended
        self.assertEqual(rr.get(teams[5], TEAM_BITS, 44, 64) >> 32, 6)
        self.assertEqual(rr.get(players[6], PLAYER_BITS, 48, 64) >> 32, 5)
        ids = [rr.get(p, PLAYER_BITS, 32, 16) for p in players]
        self.assertEqual(len(set(ids)), len(ids))                            # colliding id 21 reassigned
        self.assertEqual(report['counts']['appendedPlayerIdCollisionsResolved'], 1)

    def test_source_only_staff_is_appended_not_overwritten(self):
        base, src = rr.Roster(synthetic()), rr.Roster(synthetic(False, mismatch_staff=True))
        out, report, plan = rm.merge(base, src, CFG, variant='clubs')
        merged = rr.Roster(out.iff_bytes(SALT))
        self.assertEqual(rm.validate(merged, SALT, base, plan, CFG), [])
        staff = merged.tables['4c575ccd'].all_rows()
        self.assertEqual((report['counts']['staffAppended'], len(staff)), (1, 5))
        self.assertEqual(staff[2], base.tables['4c575ccd'].row(2))           # base staff kept
        self.assertEqual(rr.get(merged.tables['e34fad55'].row(3), TEAM_BITS, 172, 64) >> 32, 4)
        self.assertEqual(rr.get(staff[4], STAFF_BITS, 80, 64) >> 32, 3)

    def test_wpf_era_inserts_after_anchor_and_shifts_base_refs(self):
        base, src = rr.Roster(synthetic()), rr.Roster(synthetic(False))
        out, report, plan = rm.merge(base, src, CFG, variant='wpf-era')
        merged = rr.Roster(out.iff_bytes(SALT))
        self.assertEqual(rm.validate(merged, SALT, base, plan, CFG), [])
        teams = merged.tables['e34fad55'].all_rows()
        # BO, then every named source team with a new ID (AC, DR, XX, WU), then LA, US.
        self.assertEqual([rr.get(t, TEAM_BITS, 300, 16) for t in teams], [1, 300, 301, 9, 361, 2, 69])
        self.assertEqual(rr.get(teams[3], TEAM_BITS, 32, 6), 33)            # type 5 -> 33 (WPF era)
        self.assertEqual(rr.get(teams[5], TEAM_BITS, 236, 64) >> 32, 0)     # LA -> BO unchanged
        self.assertEqual(rr.get(teams[1], TEAM_BITS, 236, 64) >> 32, 2)     # AC -> DR (src 1 -> 2)
        players = merged.tables['72d60754'].all_rows()
        self.assertEqual(rr.get(players[2], PLAYER_BITS, 48, 64) >> 32, 5)  # base player on LA shifted
        self.assertEqual(rr.get(players[3], PLAYER_BITS, 48, 64) >> 32, 1)  # WNBA player on AC
        staff = merged.tables['4c575ccd'].all_rows()
        self.assertEqual(rr.get(staff[1], STAFF_BITS, 80, 64) >> 32, 5)     # base staff on LA shifted

    def test_merge_rejects_base_with_wnba(self):
        src = rr.Roster(synthetic(False))
        with self.assertRaises(rm.MergeError):
            rm.merge(src, src, CFG, variant='clubs')


@unittest.skipUnless((DECODED / 'learn2k_roster.decoded.iff').exists() and EXE.exists(), 'stock rosters not present')
class StockTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.salt = rr.read_salt(EXE)

    def test_stock_round_trip_and_crcs(self):
        for name in ('roster', 'wnba_roster', 'learn2k_roster', 'learn2k_wnba_roster'):
            path = DECODED / f'{name}.decoded.iff'
            if not path.exists():
                continue
            src = path.read_bytes()
            r = rr.Roster(src)
            self.assertEqual(r.check_crcs(self.salt), [], name)
            self.assertEqual(r.iff_bytes(self.salt), src, name)
            for tid in r.order:
                rr.schema(r.tables[tid])

    def test_stock_wpf_era_merge_matches_snapshot_and_verifier(self):
        base = rr.Roster((DECODED / 'learn2k_roster.decoded.iff').read_bytes())
        src = rr.Roster((DECODED / 'learn2k_wnba_roster.decoded.iff').read_bytes())
        out, report, plan = rm.merge(base, src, variant='wpf-era')
        merged = rr.Roster(out.iff_bytes(self.salt))
        self.assertEqual(rm.validate(merged, self.salt, base, plan), [])
        c = report['counts']
        self.assertEqual((c['mergedTeams'], c['teamsAdded'], c['insertedAt'], c['playersAffiliatedInPlace']),
                         (403, 69, 161, 209))
        t = merged.tables['e34fad55']
        rows = t.all_rows()
        kinds = [rr.get(r, t.bits, 37177, 6) for r in rows]
        ids = [rr.get(r, t.bits, 30880, 16) for r in rows]
        # freestyle_v2.rs verify_teams_with_profile (mixed): 15 WNBA ids, national {69, 361}, 30 NBA.
        self.assertEqual(sorted(i for i, k in zip(ids, kinds) if k == 29), list(range(300, 312)) + [315, 316, 317])
        self.assertEqual(sorted(i for i, k in zip(ids, kinds) if k == 22), [69, 361])
        self.assertEqual(kinds.count(0), 30)
        p = merged.tables['72d60754']
        pids = [rr.get(r, p.bits, 2242, 16) for r in p.all_rows()]
        self.assertEqual(len(set(pids)), len(pids))
        s = merged.tables['4c575ccd']
        sids = [rr.get(r, s.bits, 2240, 16) for r in s.all_rows()]
        self.assertEqual(len(set(sids)), len(sids))                         # staff ID field is bit 2240
        if SNAPSHOT.exists():   # WPF-era 403-row generation: same IDs, types and names in order
            import json
            snap = json.loads(SNAPSHOT.read_text(encoding='utf-8-sig'))
            names = [rr.wide(r, t.bits, 0x2FA * 8, 24) for r in rows]
            self.assertEqual([(x['id'], x['type'], x['name']) for x in snap['teams']], list(zip(ids, kinds, names)))


if __name__ == '__main__':
    unittest.main()
