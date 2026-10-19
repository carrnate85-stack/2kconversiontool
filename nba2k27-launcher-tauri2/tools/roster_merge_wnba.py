"""Build a mixed NBA + WNBA Learn2K roster offline (research; no game access).

Base   = decoded roster/learn2k_roster.iff (NBA Freestyle roster, 334 teams).
Source = decoded roster/learn2k_wnba_roster.iff (WNBA Freestyle roster, 136 teams).

Findings this relies on (artifacts/wnba-merge-20261017.md):
  * The WPF-era mixed roster (403 rows, research_2k27/blacktop-session-17264-
    readonly.json) is exactly: base rows 0-160, then the 69 named source teams
    whose team ID is not in the base (15 WNBA clubs, WNBA All-Star teams 357-360,
    USA 361, 42 Extras, a 2K Sports row, 6 WNBA expansion teams), then base rows
    161-333. Two rules differ from a plain copy: the WNBA East/West All-Stars
    carry type 33 (the source uses 4/5, the NBA All-Star types), and empty
    placeholder rows keep ID = 64000 + row index.
  * Players and staff that both files share sit at the same row index with the
    same ID; in the base they are unaffiliated (null team refs, zero contract
    fields) and unreferenced. They are affiliated in place. Source-only rows
    (national/All-Star copy players, their person rows) are appended with fresh
    unique IDs; appended players are not in the player hash index (table
    222e3502, key function unknown).
  * Every other reference lands on a row identical in both files at the same
    index (verified per reference).

Variants: wpf-era (default; the WPF structure), national (15 clubs + USA 361,
appended), clubs (15 clubs only, appended; no players added).
Writes only to an explicit output folder outside the game install.
"""
import argparse
import hashlib
import json
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from rdat_roster import (NULL_REF, REF_TAG, Roster, get, put, read_salt,  # noqa: E402
                         ref_slots, schema, wide)

V2 = {
    'team': 'e34fad55',
    'player': '72d60754',
    'staff': '4c575ccd',
    'header': 'cef84e6a',
    'team_type_names': (0x2CECF817, 0x0C732B9B),   # two 6-bit team type enums
    'team_name': 0x292A0287,                        # u16[25] team nickname
    'team_id': 0x2ABD43F2,                          # u16 team ID (WNBA 300-317, USA 69/361)
    'player_id': 0x31BD8AF6,                        # u16 ID (players and staff)
    'player_names': (0x51D55A69, 0xF45FB0F8),       # u16[20] last, first
    'wnba_type': 29,
    'national_type': 22,
    'national_ids': (69, 361),
    'wnba_club_ids': tuple(range(300, 312)) + (315, 316, 317),
    'appendable': ('c54beb40', '66bfbd42'),         # person tables copied for appended players
    # Contract/affiliation scalars that differ for rostered WNBA players.
    'player_affiliation': (0x8EBBAB92, 0x29EF21A7, 0x028C93E4, 0x524E6767,
                           0x254957F1, 0xBB2DC252, 0x5EE8ECF8),
    # WPF-era layout: insert after this base team ID ("Shai's Team", row 160),
    # retype source All-Star types 4/5 to 33, placeholders keep 64000 + index.
    'wpf_insert_after_id': 477,
    'wpf_retype': {4: 33, 5: 33},
    'placeholder_id_base': 64000,
    # In-memory element strides (NBA2K27.exe type descriptors) for the header
    # size field: sum(align16(rows * stride)) over tables + constant.
    'strides': {'e34fad55': 0x1550, '72d60754': 0x4F8, '4c575ccd': 0x1C8,
                'c54beb40': 0x3C, '66bfbd42': 0x68},
    'header_size_field': 0x31875000,
}
VARIANTS = ('wpf-era', 'national', 'clubs')


def field_by_name(table, name_hash):
    found = [f for f in schema(table) if f.name_hash == name_hash and not f.parent_index]
    if len(found) != 1:
        raise ValueError(f'{table.tid}: field {name_hash:08x} not unique')
    return found[0]


def align16(n):
    return (n + 15) & ~15


class MergeError(ValueError):
    pass


def team_ref_slots(roster, tid, team_tid):
    tm = roster.type_map()
    return [(f, o) for f, o in ref_slots(roster.tables[tid]) if tm.get(f.type_hash) == [team_tid]]


def shift_team_refs(roster, tid, rows, team_tid, at, n):
    """Indices >= at in every team reference of these rows move up by n."""
    if not n:
        return rows
    bits = roster.tables[tid].bits
    slots = team_ref_slots(roster, tid, team_tid)
    out = []
    for r in rows:
        for _, off in slots:
            v = get(r, bits, off, 64)
            if v != NULL_REF and (v >> 32) >= at:
                r = put(r, bits, off, 64, (((v >> 32) + n) << 32) | (v & 0xFFFFFFFF))
        out.append(r)
    return out


def merge(base, source, cfg=V2, variant='wpf-era'):
    if variant not in VARIANTS:
        raise MergeError(f'unknown variant {variant}')
    TEAM, PLAYER, STAFF = cfg['team'], cfg['player'], cfg['staff']
    tmap = source.type_map()

    def target(field):
        t = tmap.get(field.type_hash)
        if not t or len(t) != 1:
            raise MergeError(f'reference type {field.type_hash:08x} maps to {t}')
        return t[0]

    def layout(table):
        return table.bits, [(f.offset, f.width, f.type, f.type_hash, f.name_hash) for f in schema(table)]

    for tid in (TEAM, PLAYER, STAFF):
        if layout(base.tables[tid]) != layout(source.tables[tid]):
            raise MergeError(f'{tid}: schemas differ between base and source')

    # ---- team selection and placement ----
    bt, st = base.tables[TEAM], source.tables[TEAM]
    type_fields = [field_by_name(st, h) for h in cfg['team_type_names']]
    name_f = field_by_name(st, cfg['team_name'])
    id_f = field_by_name(st, cfg['team_id'])
    src_rows, base_rows = st.all_rows(), bt.all_rows()

    def kind(table, r):
        return get(r, table.bits, type_fields[0].offset, type_fields[0].width)

    def team_id(table, r):
        return get(r, table.bits, id_f.offset, id_f.width)

    def tname(table, r):
        return wide(r, table.bits, name_f.offset, name_f.count)

    if any(kind(bt, r) == cfg['wnba_type'] for r in base_rows):
        raise MergeError('base already contains WNBA-type teams')
    base_by_id = {team_id(bt, r): i for i, r in enumerate(base_rows)}
    if len(base_by_id) != len(base_rows):
        raise MergeError('base team IDs are not unique')
    clubs = [i for i, r in enumerate(src_rows)
             if all(get(r, st.bits, f.offset, f.width) == cfg['wnba_type'] for f in type_fields)]
    if sorted(team_id(st, src_rows[i]) for i in clubs) != sorted(cfg['wnba_club_ids']):
        raise MergeError('source does not hold the 15 expected WNBA clubs')
    if variant == 'clubs':
        selected = clubs
    elif variant == 'national':
        selected = clubs + [i for i, r in enumerate(src_rows) if kind(st, r) == cfg['national_type']
                            and team_id(st, r) in cfg['national_ids'] and team_id(st, r) not in base_by_id]
    else:
        selected = [i for i, r in enumerate(src_rows) if tname(st, r) and team_id(st, r) not in base_by_id]
    n = len(selected)
    if variant == 'wpf-era':
        if cfg['wpf_insert_after_id'] not in base_by_id:
            raise MergeError('WPF-era insertion anchor team not in base')
        at = base_by_id[cfg['wpf_insert_after_id']] + 1
    else:
        at = bt.rows

    def base_new(i):
        return i if i < at else i + n

    team_map = {s: at + k for k, s in enumerate(selected)}
    for i, r in enumerate(src_rows):
        if i not in team_map and team_id(st, r) in base_by_id:
            team_map[i] = base_new(base_by_id[team_id(st, r)])   # shared team, same ID

    # ---- identity helpers ----
    bp, sp = base.tables[PLAYER], source.tables[PLAYER]
    pid = field_by_name(sp, cfg['player_id'])
    pnames = [field_by_name(sp, h) for h in cfg['player_names']]
    staff_t, sst = base.tables[STAFF], source.tables[STAFF]
    sid = field_by_name(sst, cfg['player_id'])
    p_team_slots = team_ref_slots(source, PLAYER, TEAM)
    s_team_slots = team_ref_slots(source, STAFF, TEAM)

    def same_person(i):
        if i >= bp.rows or i >= sp.rows:
            return False
        a, b = bp.row(i), sp.row(i)
        return get(a, bp.bits, pid.offset, 16) == get(b, sp.bits, pid.offset, 16) and all(
            wide(a, bp.bits, f.offset, f.count) == wide(b, sp.bits, f.offset, f.count) for f in pnames)

    def staff_in_base(i):
        # Same index and staff ID, every non-team field equal, base team refs null.
        if i >= staff_t.rows:
            return False
        a, b = staff_t.row(i), sst.row(i)
        if get(a, staff_t.bits, sid.offset, 16) != get(b, sst.bits, sid.offset, 16):
            return False
        for _, off in s_team_slots:
            if get(a, staff_t.bits, off, 64) != NULL_REF:
                return False
            a, b = put(a, staff_t.bits, off, 64, 0), put(b, sst.bits, off, 64, 0)
        return a == b

    def identical(tid, i):
        b, s = base.tables[tid], source.tables[tid]
        return i < b.rows and i < s.rows and b.bits == s.bits and b.row(i) == s.row(i)

    shared, nulled = {}, {}
    appended = {t: {} for t in (PLAYER, STAFF) + tuple(cfg['appendable'])}
    appended_rows = {t: [] for t in appended}
    players_used, staff_used = set(), set()

    def append(tg, idx):
        if idx not in appended[tg]:
            slot = len(appended_rows[tg])
            appended[tg][idx] = base.tables[tg].rows + slot
            appended_rows[tg].append(None)
            appended_rows[tg][slot] = remap_refs(tg, source.tables[tg].row(idx), copied=True)
        return appended[tg][idx]

    def note(d, key):
        d[key] = d.get(key, 0) + 1

    def remap_refs(table_id, row, copied=False):
        table = source.tables[table_id]
        for f, off in ref_slots(table):
            v = get(row, table.bits, off, 64)
            if v == NULL_REF:
                continue
            idx, tag = v >> 32, v & 0xFFFFFFFF
            if tag != REF_TAG:
                raise MergeError(f'{table_id}: unexpected reference tag {tag:08x}')
            tg = target(f)
            if tg == TEAM:
                if idx in team_map:
                    new = team_map[idx]
                elif copied or table_id == TEAM:
                    note(nulled, f'{table_id}:{f.name_hash:08x}')
                    row = put(row, table.bits, off, 64, NULL_REF)
                    continue
                else:
                    raise MergeError(f'{table_id}: reference to unmerged team {idx}')
            elif tg == PLAYER:
                if same_person(idx):
                    players_used.add(idx)
                    new = idx
                else:
                    new = append(PLAYER, idx)
            elif tg == STAFF:
                if staff_in_base(idx):
                    staff_used.add(idx)
                    new = idx
                else:
                    new = append(STAFF, idx)
            elif identical(tg, idx):
                note(shared, tg)
                new = idx
            elif copied and tg in cfg['appendable']:
                new = append(tg, idx)
            else:
                raise MergeError(f'{table_id}: {tg} row {idx} differs between base and source')
            row = put(row, table.bits, off, 64, (new << 32) | tag)
        return row

    inserted = [remap_refs(TEAM, src_rows[s]) for s in selected]

    # ---- assemble ----
    out = Roster(base.iff)
    # Every base team reference in every table moves with the insertion.
    for tid in base.order:
        t = out.tables[tid]
        if t.rows and team_ref_slots(base, tid, TEAM) and n and at < bt.rows:
            t.set_rows(shift_team_refs(base, tid, t.all_rows(), TEAM, at, n))

    teams = out.tables[TEAM].all_rows()
    teams = teams[:at] + inserted + teams[at:]
    retyped = 0
    for k in range(at, at + n):
        old = kind(st, teams[k])
        if variant == 'wpf-era' and old in cfg['wpf_retype']:
            for f in type_fields:
                teams[k] = put(teams[k], st.bits, f.offset, f.width, cfg['wpf_retype'][old])
            retyped += 1
    renumbered = 0
    for i, r in enumerate(teams):
        tidv = team_id(st, r)
        if not tname(st, r) and tidv >= cfg['placeholder_id_base'] and tidv != cfg['placeholder_id_base'] + i:
            teams[i] = put(r, st.bits, id_f.offset, id_f.width, cfg['placeholder_id_base'] + i)
            renumbered += 1
    if len({team_id(st, r) for r in teams}) != len(teams):
        raise MergeError('merged team IDs are not unique')

    # Affiliate shared players/staff in place (team refs from the source, remapped).
    affil = [field_by_name(sp, h) for h in cfg['player_affiliation']]
    prows = out.tables[PLAYER].all_rows()
    for i in sorted(players_used):
        row, srow = prows[i], sp.row(i)
        for _, off in p_team_slots:
            v = get(srow, sp.bits, off, 64)
            if v == NULL_REF or (v >> 32) not in team_map:
                continue
            if get(row, bp.bits, off, 64) != NULL_REF:
                if get(row, bp.bits, off, 64) >> 32 == team_map[v >> 32]:
                    continue
                raise MergeError(f'player {i}: base team reference at {off} is not null')
            row = put(row, bp.bits, off, 64, (team_map[v >> 32] << 32) | (v & 0xFFFFFFFF))
        if row != prows[i]:
            for f in affil:
                row = put(row, bp.bits, f.offset, f.width, get(srow, sp.bits, f.offset, f.width))
        prows[i] = row
    affiliated = sum(1 for i in players_used if prows[i] != out.tables[PLAYER].row(i))
    srows = out.tables[STAFF].all_rows()
    for i in sorted(staff_used):
        row, srow = srows[i], sst.row(i)
        for _, off in s_team_slots:
            v = get(srow, sst.bits, off, 64)
            if v != NULL_REF and (v >> 32) in team_map:
                row = put(row, staff_t.bits, off, 64, (team_map[v >> 32] << 32) | (v & 0xFFFFFFFF))
        srows[i] = row

    # Appended players/staff get unused IDs (their source IDs belong to other base rows).
    def fresh_ids(rows, table, id_field):
        bits, off = table.bits, id_field.offset
        base_ids = {get(r, bits, off, 16) for r in table.all_rows()}
        free = (x for x in range(0xFFFE, 0, -1) if x not in base_ids)
        used, changes, collisions = set(base_ids), {}, 0
        for k, r in enumerate(rows):
            old = get(r, bits, off, 16)
            if old in used:
                collisions += 1
                new = next(x for x in free if x not in used)
                rows[k] = put(r, bits, off, 16, new)
                changes[old] = new
            used.add(get(rows[k], bits, off, 16))
        return collisions, changes

    # Each table's own ID field: players at bit 2242, staff at bit 2240.
    p_coll, p_changes = fresh_ids(appended_rows[PLAYER], bp, pid)
    s_coll, s_changes = fresh_ids(appended_rows[STAFF], staff_t, sid)

    out.tables[TEAM].set_rows(teams)
    out.tables[PLAYER].set_rows(prows + appended_rows[PLAYER])
    out.tables[STAFF].set_rows(srows + appended_rows[STAFF])
    for tg in cfg['appendable']:
        if appended_rows[tg]:
            out.tables[tg].set_rows(out.tables[tg].all_rows() + appended_rows[tg])
    hdr = out.tables[cfg['header']]
    size_f = field_by_name(hdr, cfg['header_size_field'])
    delta = sum(align16(out.tables[t].rows * s) - align16(base.tables[t].rows * s)
                for t, s in cfg['strides'].items() if t in base.tables)
    old_size = get(hdr.row(0), hdr.bits, size_f.offset, size_f.width)
    hdr.set_rows([put(hdr.row(0), hdr.bits, size_f.offset, size_f.width, old_size + delta)])

    report = {
        'variant': variant,
        'counts': {
            'baseTeams': bt.rows, 'teamsAdded': n, 'mergedTeams': len(teams), 'insertedAt': at,
            'wnbaClubs': len(clubs), 'playersAffiliatedInPlace': affiliated,
            'playersAppended': len(appended_rows[PLAYER]), 'players': out.tables[PLAYER].rows,
            'staffAffiliatedInPlace': len(staff_used), 'staffAppended': len(appended_rows[STAFF]),
            'personRowsAppended': {t: len(appended_rows[t]) for t in cfg['appendable']},
            'teamsRetypedWpfEra': retyped, 'placeholderIdsRenumbered': renumbered,
            'appendedPlayerIdCollisionsResolved': p_coll, 'appendedStaffIdCollisionsResolved': s_coll,
            'teamRefsNulledOutsideMerge': nulled, 'sharedReferencesVerified': shared,
            'playersNotInHashIndex': len(appended_rows[PLAYER]), 'headerSize': [old_size, old_size + delta],
        },
        'idChanges': {'players': p_changes, 'staff': s_changes},
        'teams': [{'sourceIndex': s, 'mergedIndex': team_map[s], 'id': team_id(st, src_rows[s]),
                   'type': kind(st, teams[team_map[s]]), 'name': tname(st, src_rows[s])} for s in selected],
    }
    plan = {'at': at, 'n': n}
    return out, report, plan


def validate(roster, salt, base=None, plan=None, cfg=V2):
    """Every reference in range, CRCs consistent, base rows kept (modulo the shift)."""
    tm = roster.type_map()
    problems = []
    for tid in roster.order:
        t = roster.tables[tid]
        if not t.rows:
            continue
        sl = ref_slots(t)
        for i, r in enumerate(t.all_rows()):
            for f, off in sl:
                v = get(r, t.bits, off, 64)
                if v == NULL_REF:
                    continue
                tg = tm[f.type_hash][0]
                if (v & 0xFFFFFFFF) != REF_TAG or (v >> 32) >= roster.tables[tg].rows:
                    problems.append(f'{tid}[{i}]@{off} -> {tg}[{v >> 32}]')
    if roster.check_crcs(salt):
        problems.append(f'crc mismatch {roster.check_crcs(salt)}')
    if base is not None and plan is not None:
        at, n, team = plan['at'], plan['n'], cfg['team']
        st = base.tables[team]
        id_f = field_by_name(st, cfg['team_id'])
        name_f = field_by_name(st, cfg['team_name'])
        expect = shift_team_refs(base, team, st.all_rows(), team, at, n)
        got = roster.tables[team].all_rows()
        got = got[:at] + got[at + n:]
        for i, (a, b) in enumerate(zip(expect, got)):
            if not wide(a, st.bits, name_f.offset, name_f.count):     # placeholder IDs renumber
                a = put(a, st.bits, id_f.offset, id_f.width, 0)
                b = put(b, st.bits, id_f.offset, id_f.width, 0)
            if a != b:
                problems.append(f'team base row {i} changed')
        touched = {team, cfg['player'], cfg['staff'], cfg['header']} | set(cfg['appendable'])
        for tid in base.order:
            if tid in touched:
                if tid in cfg['appendable'] and tid in base.tables:
                    if roster.tables[tid].all_rows()[:base.tables[tid].rows] != base.tables[tid].all_rows():
                        problems.append(f'{tid}: base rows changed')
                continue
            expect = shift_team_refs(base, tid, base.tables[tid].all_rows(), team, at, n) \
                if base.tables[tid].rows else []
            if roster.tables[tid].all_rows() != expect or roster.tables[tid].aux != base.tables[tid].aux:
                problems.append(f'{tid}: changed beyond the team-index shift')
    return problems


def main():
    p = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    p.add_argument('base', type=pathlib.Path, help='decoded learn2k_roster.iff')
    p.add_argument('source', type=pathlib.Path, help='decoded learn2k_wnba_roster.iff')
    p.add_argument('--exe', type=pathlib.Path, required=True, help='NBA2K27.exe (read-only, for the CRC salt)')
    p.add_argument('--out-dir', type=pathlib.Path, required=True)
    p.add_argument('--variant', choices=VARIANTS, default='wpf-era')
    p.add_argument('--wpf-snapshot', type=pathlib.Path,
                   help='research_2k27/blacktop-session-17264-readonly.json, to compare team order')
    p.add_argument('--wrap-header', type=pathlib.Path, help='stock wrapped learn2k_roster.iff (header source)')
    p.add_argument('--key-file', type=pathlib.Path)
    p.add_argument('--oodle', type=pathlib.Path)
    a = p.parse_args()
    out_dir = a.out_dir.resolve()
    if 'steamapps' in str(out_dir).lower() or 'nba 2k27' in str(out_dir).lower():
        raise SystemExit('Refusing to write inside the game install')
    salt = read_salt(a.exe)
    base, source = Roster(a.base.read_bytes()), Roster(a.source.read_bytes())
    for name, r in (('base', base), ('source', source)):
        if r.check_crcs(salt):
            raise SystemExit(f'{name}: stock CRCs do not verify with this executable')
    merged, report, plan = merge(base, source, variant=a.variant)
    iff = merged.iff_bytes(salt)
    reparsed = Roster(iff)
    problems = validate(reparsed, salt, base, plan)
    if reparsed.iff_bytes(salt) != iff:
        problems.append('serialization not stable')
    if a.wpf_snapshot:
        snap = json.loads(a.wpf_snapshot.read_text(encoding='utf-8-sig'))
        t = reparsed.tables[V2['team']]
        idf, nf, kf = (field_by_name(t, V2['team_id']), field_by_name(t, V2['team_name']),
                       field_by_name(t, V2['team_type_names'][0]))
        mine = [(get(r, t.bits, idf.offset, 16), get(r, t.bits, kf.offset, kf.width),
                 wide(r, t.bits, nf.offset, nf.count)) for r in t.all_rows()]
        old = [(x['id'], x['type'], x['name']) for x in snap['teams']]
        report['wpfSnapshotComparison'] = {
            'rows': [len(old), len(mine)], 'identicalRows': sum(1 for x, y in zip(old, mine) if x == y),
            'firstDifferences': [[i, x, y] for i, (x, y) in enumerate(zip(old, mine)) if x != y][:10],
            'playerCount': [snap.get('playerCount'), reparsed.tables[V2['player']].rows]}
    report['validation'] = problems or 'ok'
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / 'learn2k_roster.merged.decoded.iff').write_bytes(iff)
    report['decoded'] = {'bytes': len(iff), 'sha256': hashlib.sha256(iff).hexdigest()}
    if a.wrap_header and a.key_file and a.oodle:
        from roster_vcz_codec import decode, encode, oodle_compressor
        from roster_wrapper_offline import oodle_decoder
        key = a.key_file.read_bytes()
        wrapped = encode(iff, a.wrap_header.read_bytes()[:16], key, oodle_compressor(a.oodle))
        back, meta, _ = decode(wrapped, key, oodle_decoder(a.oodle))
        if back != iff:
            problems.append('wrapped round-trip differs')
        (out_dir / 'learn2k_roster.iff').write_bytes(wrapped)
        report['wrapped'] = {'bytes': len(wrapped), 'sha256': hashlib.sha256(wrapped).hexdigest(),
                             'roundTrip': back == iff, 'crc32': meta['crc32']}
    (out_dir / 'merge-report.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    print(json.dumps({k: report[k] for k in ('variant', 'counts', 'validation')}, indent=2))
    if 'wpfSnapshotComparison' in report:
        print(json.dumps(report['wpfSnapshotComparison'], indent=2))
    if problems:
        raise SystemExit(f'validation failed: {problems[:5]}')


if __name__ == '__main__':
    main()
