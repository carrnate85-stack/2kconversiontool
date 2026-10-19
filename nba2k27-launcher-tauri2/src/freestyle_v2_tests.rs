use super::*;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
};

const BASE: u64 = 0x180000000;
const ROOT: u64 = 0x200000000;
const OWNER: u64 = ROOT + 0x1000;
const TABLE: u64 = ROOT + 0x10000;
struct Fixture {
    bytes: RefCell<BTreeMap<u64, Vec<u8>>>,
    private_allocation: Cell<u64>,
    reads: Cell<usize>,
    row_reads: Cell<usize>,
    mutate_at: Cell<usize>,
    exited: Cell<bool>,
}
impl Fixture {
    fn put(&self, at: u64, bytes: &[u8]) {
        self.bytes.borrow_mut().insert(at, bytes.to_vec());
    }
    fn rva(&self, rva: u64, bytes: &[u8]) {
        self.put(BASE + rva, bytes);
    }
    fn stock() -> Self {
        let f = Self {
            bytes: RefCell::new(BTreeMap::new()),
            private_allocation: Cell::new(ROOT),
            reads: Cell::new(0),
            row_reads: Cell::new(0),
            mutate_at: Cell::new(usize::MAX),
            exited: Cell::new(false),
        };
        let mut h = vec![0; 4096];
        h[..2].copy_from_slice(b"MZ");
        h[60..64].copy_from_slice(&128u32.to_le_bytes());
        h[128..132].copy_from_slice(b"PE\0\0");
        h[132..134].copy_from_slice(&0x8664u16.to_le_bytes());
        h[152..154].copy_from_slice(&0x20Bu16.to_le_bytes());
        h[136..140].copy_from_slice(&0x6AC53765u32.to_le_bytes());
        h[208..212].copy_from_slice(&0x35BDE000u32.to_le_bytes());
        f.rva(0, &h);
        for &(rva, prefix, target) in RIP_ANCHORS {
            let mut b = prefix.to_vec();
            b.extend_from_slice(
                &((target as i64 - rva as i64 - prefix.len() as i64 - 4) as i32).to_le_bytes(),
            );
            f.rva(rva, &b);
        }
        for &(rva, b) in FIELD_ANCHORS {
            f.rva(rva, b);
        }
        f.rva(
            MASK_PAGES,
            &CATEGORY_MASKS
                .into_iter()
                .flat_map(u64::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        f.rva(
            PICKER_VTABLE,
            &[0x23120u64, 0x38299A0, 0x24EB5C0]
                .into_iter()
                .flat_map(|n| (BASE + n).to_le_bytes())
                .collect::<Vec<_>>(),
        );
        for (rva, row) in [PREVIOUS, NEXT].into_iter().zip(ORIGINAL) {
            f.rva(
                rva,
                &row.into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            );
        }
        f.rva(CURRENT, &2u32.to_le_bytes());
        f.rva(ALTERNATE, &2u32.to_le_bytes());
        f.rva(MODES + 128, &FREESTYLE.to_le_bytes());
        let mut setup = vec![0; 24];
        for (at, v) in [(0, 6u32), (12, 1), (16, 1), (20, 4)] {
            setup[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
        f.rva(SETUP, &setup);
        f.rva(PICKER, &(BASE + PICKER_VTABLE).to_le_bytes());
        f.rva(ROSTER_ROOT, &ROOT.to_le_bytes());
        f.put(ROOT + 0xA8, &OWNER.to_le_bytes());
        let rows = roster();
        let count = rows.len() / TEAM_STRIDE;
        let mut header = vec![0; 16];
        header[..4].copy_from_slice(&(count as u32).to_le_bytes());
        header[8..].copy_from_slice(&TABLE.to_le_bytes());
        f.put(OWNER + 0x80, &header);
        f.put(TABLE, &rows);
        let mut selected = vec![0; 20];
        selected[..8].copy_from_slice(&TABLE.to_le_bytes());
        f.rva(SELECTED, &selected);
        let mut other = vec![0; 12];
        other[..8].copy_from_slice(&(TABLE + TEAM_STRIDE as u64).to_le_bytes());
        f.rva(OTHER_SELECTED, &other);
        let mut mask = vec![0; 24];
        mask[8..16].copy_from_slice(&STOCK_MASK.to_le_bytes());
        f.rva(MASK_WINDOW, &mask);
        f
    }
}
impl Memory for Fixture {
    fn base(&self) -> u64 {
        BASE
    }
    fn checkpoint(&self) -> Result<()> {
        ensure!(!self.exited.get(), "Fixture process ended");
        Ok(())
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        assert!(len <= 4096, "Native read bound must be retained");
        self.reads.set(self.reads.get() + 1);
        if (TABLE..TABLE + (1024 * TEAM_STRIDE) as u64).contains(&at) {
            self.row_reads.set(self.row_reads.get() + 1);
        }
        if self.reads.get() == self.mutate_at.get() {
            self.rva(CURRENT, &99u32.to_le_bytes());
        }
        for (start, b) in self.bytes.borrow().range(..=at).rev() {
            if at
                .checked_add(len as u64)
                .is_some_and(|end| end <= *start + b.len() as u64)
            {
                let off = (at - start) as usize;
                return Ok(b[off..off + len].to_vec());
            }
        }
        anyhow::bail!("Fixture unmapped read {at:X}+{len}")
    }
}
impl Regions for Fixture {
    fn region(&self, at: u64, _len: usize) -> Result<Region> {
        let image = at >= BASE && at < BASE + 0x35BDE000;
        Ok(Region {
            allocation: if image {
                BASE
            } else {
                self.private_allocation.get()
            },
            kind: if image {
                RegionKind::Image
            } else {
                RegionKind::Private
            },
            readable: true,
            writable: true,
            executable: false,
        })
    }
}
fn roster() -> Vec<u8> {
    let mut rows = Vec::new();
    for (id, kind) in (0..30u16)
        .map(|id| (id, 0u32))
        .chain([(100, 1), (200, 27)])
        .chain((300..312u16).chain([315, 316, 317]).map(|id| (id, 29)))
        .chain([(69, 22), (361, 22)])
    {
        let mut row = vec![0; TEAM_STRIDE];
        row[0xF20..0xF22].copy_from_slice(&id.to_le_bytes());
        row[0x123C..0x1240].copy_from_slice(&(kind << 26).to_le_bytes());
        let name: Vec<u8> = format!("Team {id}")
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        row[0x2FA..0x2FA + name.len()].copy_from_slice(&name);
        rows.extend(row);
    }
    rows
}
fn wnba_roster() -> Vec<u8> {
    let mut rows = Vec::new();
    for id in (300..312u16).chain([315, 316, 317]) {
        let mut row = vec![0; TEAM_STRIDE];
        row[0xF20..0xF22].copy_from_slice(&id.to_le_bytes());
        row[0x123C..0x1240].copy_from_slice(&(29u32 << 26).to_le_bytes());
        let name: Vec<u8> = format!("WNBA Team {id}")
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        row[0x2FA..0x2FA + name.len()].copy_from_slice(&name);
        rows.extend(row);
    }
    rows
}
#[test]
fn exact_profile_and_four_dword_plan_are_aslr_relative() {
    let f = Fixture::stock();
    validate_profile(&f).unwrap();
    let a = capture(&f, &f).unwrap();
    let b = capture(&f, &f).unwrap();
    assert_eq!(a.team_count, 49);
    assert_eq!(a.eligible, 49);
    let p = plan(Tables::read(&f).unwrap(), &a, &b).unwrap();
    p.validate().unwrap();
    assert_eq!(
        p.changes.iter().map(|c| c.rva).collect::<Vec<_>>(),
        vec![0x7A9FB1C, 0x7A9FB20, 0x7A9FB38, 0x7A9FB3C]
    );
    assert_eq!(p.changes.len() * 4, 16);
    assert!(
        p.changes
            .iter()
            .all(|c| (PAGE..PAGE + 4096).contains(&c.rva))
    );
    assert_eq!(p.before, Tables::original());
    assert_eq!(p.after, Tables::enabled());
}
#[test]
fn sibling_stock_loader_may_use_wnba_only_roster_but_category_only_path_may_not() {
    let f = Fixture::stock();
    let rows = wnba_roster();
    let count = rows.len() / TEAM_STRIDE;
    let mut header = vec![0; 16];
    header[..4].copy_from_slice(&(count as u32).to_le_bytes());
    header[8..].copy_from_slice(&TABLE.to_le_bytes());
    f.put(OWNER + 0x80, &header);
    f.put(TABLE, &rows);
    f.rva(OTHER_SELECTED, &{
        let mut other = vec![0; 12];
        other[..8].copy_from_slice(&(TABLE + TEAM_STRIDE as u64).to_le_bytes());
        other
    });
    assert!(capture(&f, &f).is_err());
    let a = capture_after_wnba_loader(&f, &f).unwrap();
    let b = capture_after_wnba_loader(&f, &f).unwrap();
    assert_eq!(a.team_count, 15);
    assert_eq!(a.profile, RosterProfile::WnbaOnly);
    plan(Tables::read(&f).unwrap(), &a, &b).unwrap();
}
#[test]
fn lightweight_fingerprint_changes_with_roster_generation_without_reading_rows() {
    let f = Fixture::stock();
    let first = fingerprint(&f, &f).unwrap();
    let reads = f.reads.get();
    let mut header = f.read(OWNER + 0x80, 16).unwrap();
    header[..4].copy_from_slice(&48u32.to_le_bytes());
    f.put(OWNER + 0x80, &header);
    let second = fingerprint(&f, &f).unwrap();
    assert_ne!(first, second);
    assert!(
        f.reads.get() - reads < 64,
        "fingerprint unexpectedly read team rows"
    );
}
#[test]
fn category_navigation_visits_all_five_categories_both_directions() {
    for row in ENABLED {
        let mut at = 0usize;
        let mut seen = BTreeSet::new();
        for _ in 0..5 {
            assert!(seen.insert(at));
            at = row[at] as usize;
        }
        assert_eq!(at, 0);
        assert_eq!(seen, (0..5).collect());
        assert_eq!(row[5], 5);
    }
}
#[test]
fn every_owned_partial_state_is_recoverable_but_never_adopted() {
    let f = Fixture::stock();
    let a = capture(&f, &f).unwrap();
    for bits in 0..16 {
        let mut t = Tables::original();
        for (i, c) in CHANGES.into_iter().enumerate() {
            if bits & (1 << i) != 0 {
                let row = usize::from(c.rva >= NEXT);
                let start = if row == 0 { PREVIOUS } else { NEXT };
                t.0[row][((c.rva - start) / 4) as usize] = c.after;
            }
        }
        assert!(t.allowed_mixture());
        assert_eq!(plan(t, &a, &a).is_ok(), bits == 0);
        assert_eq!(observe(t).is_ok(), bits == 15);
    }
    let mut foreign = Tables::enabled();
    foreign.0[0][0] = 7;
    assert!(!foreign.allowed_mixture());
    assert!(observe(foreign).is_err());
}
#[test]
fn profile_rejects_prior_build_rip_target_vtable_and_patched_filter() {
    for mutation in 0..5 {
        let f = Fixture::stock();
        match mutation {
            0 => {
                let mut h = image(&f, 0, 4096).unwrap();
                h[136..140].copy_from_slice(&0x6AA2013Du32.to_le_bytes());
                f.rva(0, &h)
            }
            1 => {
                let (rva, prefix, _) = RIP_ANCHORS[0];
                let mut b = image(&f, rva, prefix.len() + 4).unwrap();
                b[prefix.len()] ^= 1;
                f.rva(rva, &b)
            }
            2 => f.rva(PICKER_VTABLE, &[0; 24]),
            3 => f.rva(MASK_PAGES, &[0; 40]),
            _ => f.rva(0x2DDA683, &[0; 7]),
        }
        assert!(validate_profile(&f).is_err(), "mutation {mutation}");
    }
}
#[test]
fn all_15_women_and_both_nationals_are_required_and_unique() {
    let good = roster();
    let count = good.len() / TEAM_STRIDE;
    verify_teams(&good, count).unwrap();
    for index in [0, 30, 31, 32, 46, 47, 48] {
        let mut b = good.clone();
        let at = index * TEAM_STRIDE;
        b[at + 0x123C..at + 0x1240].copy_from_slice(&(63u32 << 26).to_le_bytes());
        assert!(verify_teams(&b, count).is_err(), "required index {index}");
    }
    let mut duplicate = good.clone();
    let at = 33 * TEAM_STRIDE;
    duplicate[at + 0xF20..at + 0xF22].copy_from_slice(&300u16.to_le_bytes());
    assert!(verify_teams(&duplicate, count).is_err());
    let mut unready = good.clone();
    unready[32 * TEAM_STRIDE + 0x2FA..32 * TEAM_STRIDE + 0x32A].fill(0);
    assert!(verify_teams(&unready, count).is_err());
    assert!(verify_teams(&good, 1025).is_err());
    assert!(verify_teams(&good[..good.len() - 1], count).is_err());
}

#[test]
fn default_nba_only_roster_is_explicitly_unavailable_not_false_readiness() {
    let mut rows = roster();
    for row in rows.chunks_exact_mut(TEAM_STRIDE) {
        let kind = d(row, 0x123C) >> 26;
        let id = u16::from_le_bytes(row[0xF20..0xF22].try_into().unwrap());
        if kind == 29 || id == 361 {
            row[0x123C..0x1240].copy_from_slice(&(33u32 << 26).to_le_bytes());
        }
    }
    rows.resize(334 * TEAM_STRIDE, 0);
    let error = verify_teams(&rows, 334).unwrap_err();
    let missing = error.downcast_ref::<MissingRosterTeams>().unwrap();
    assert_eq!(
        (missing.teams, missing.women, missing.national),
        (334, 0, 1)
    );
    assert!(error.to_string().contains("not applied"));
    assert!(!error.to_string().contains("Waiting"));
    // Keep the verified mixed-roster path; do not bypass roster validation.
    let good = roster();
    verify_teams(&good, good.len() / TEAM_STRIDE).unwrap();
}
#[test]
fn names_validate_utf16_and_reject_control_characters() {
    for units in [vec![0xD800u16, 0], vec![10u16, 0]] {
        let mut b = roster();
        let at = 32 * TEAM_STRIDE + 0x2FA;
        for (i, c) in units.into_iter().enumerate() {
            b[at + i * 2..at + i * 2 + 2].copy_from_slice(&c.to_le_bytes());
        }
        assert!(verify_teams(&b, b.len() / TEAM_STRIDE).is_err());
    }
}
#[test]
fn picker_context_foreign_filter_unaligned_choice_and_exit_fail_closed() {
    for mutation in 0..7 {
        let f = Fixture::stock();
        match mutation {
            0 => f.rva(CURRENT, &128u32.to_le_bytes()),
            1 => f.rva(PICKER, &BASE.to_le_bytes()),
            2 => {
                let mut setup = image(&f, SETUP, 24).unwrap();
                setup[0] = 5;
                f.rva(SETUP, &setup)
            }
            3 => {
                let mut b = image(&f, SELECTED, 20).unwrap();
                b[..8].copy_from_slice(&(TABLE + 1).to_le_bytes());
                f.rva(SELECTED, &b)
            }
            4 => {
                let mut b = image(&f, MASK_WINDOW, 24).unwrap();
                b[8..16].copy_from_slice(&0x68400033u64.to_le_bytes());
                f.rva(MASK_WINDOW, &b)
            }
            5 => {
                let mut b = image(&f, OTHER_SELECTED, 12).unwrap();
                b[8..12].copy_from_slice(&5u32.to_le_bytes());
                f.rva(OTHER_SELECTED, &b)
            }
            _ => f.exited.set(true),
        }
        assert!(capture(&f, &f).is_err(), "mutation {mutation}");
    }
}
#[test]
fn recapture_detects_mode_transition_and_deep_roster_mutation() {
    let f = Fixture::stock();
    let a = capture(&f, &f).unwrap();
    let mut rows = read(&f, TABLE, a.team_count * TEAM_STRIDE).unwrap();
    rows[100] ^= 1;
    f.put(TABLE, &rows);
    let b = capture(&f, &f).unwrap();
    assert!(plan(Tables::original(), &a, &b).is_err());
    let f = Fixture::stock();
    f.mutate_at.set(RIP_ANCHORS.len() + FIELD_ANCHORS.len() + 8);
    assert!(capture(&f, &f).is_err());
}
#[test]
fn plans_cannot_expand_the_allowlist() {
    let mut p = ApplyPlan {
        changes: CHANGES,
        before: Tables::original(),
        after: Tables::enabled(),
    };
    p.changes[0].rva = SETUP;
    assert!(p.validate().is_err());
    p.changes = CHANGES;
    p.changes[0].after = 99;
    assert!(p.validate().is_err());
    p.changes = CHANGES;
    p.before = Tables::enabled();
    assert!(p.validate().is_err());
}
#[test]
fn applied_tables_do_not_require_picker_and_never_rearm_on_menu_return() {
    let f = Fixture::stock();
    f.rva(CURRENT, &99u32.to_le_bytes());
    assert!(capture(&f, &f).is_err());
    observe(Tables::enabled()).unwrap();
    assert!(observe(Tables::original()).is_err());
}

#[test]
fn policy_matches_unchanged_wpf_oracle_for_all_52_states() {
    let v: serde_json::Value = serde_json::from_str(include_str!(
        "../assets/freestyle-category-regressions.json"
    ))
    .unwrap();
    assert_eq!(v["schema"], "nba2k27-freestyle-category-oracle-v1");
    assert_eq!(v["gameAccess"], false);
    for (key, expected) in [
        ("original", Tables::original()),
        ("enabled", Tables::enabled()),
    ] {
        let parsed: Vec<Vec<u32>> = serde_json::from_value(v[key].clone()).unwrap();
        assert_eq!(
            parsed,
            expected.0.iter().map(|r| r.to_vec()).collect::<Vec<_>>()
        );
    }
    let f = Fixture::stock();
    let snap = capture(&f, &f).unwrap();
    for row in v["mixtures"].as_array().unwrap() {
        let bits = row["bits"].as_u64().unwrap();
        let mut t = Tables::original();
        for (i, c) in CHANGES.into_iter().enumerate() {
            if bits & (1 << i) != 0 {
                let r = usize::from(c.rva >= NEXT);
                let start = if r == 0 { PREVIOUS } else { NEXT };
                t.0[r][((c.rva - start) / 4) as usize] = c.after;
            }
        }
        assert_eq!(t.allowed_mixture(), row["owned"].as_bool().unwrap());
        assert_eq!(
            plan(t, &snap, &snap).is_ok(),
            row["original"].as_bool().unwrap()
        );
        assert_eq!(observe(t).is_ok(), row["enabled"].as_bool().unwrap());
    }
    for row in v["foreign"].as_array().unwrap() {
        let mut t = Tables::original();
        t.0[row["table"].as_u64().unwrap() as usize][row["index"].as_u64().unwrap() as usize] =
            row["value"].as_u64().unwrap() as u32;
        assert_eq!(t.allowed_mixture(), row["owned"].as_bool().unwrap());
        assert!(plan(t, &snap, &snap).is_err());
    }
}

#[test]
fn allocation_types_guards_and_mismatched_owners_reject_without_reads() {
    struct BadRegions {
        fault: u8,
    }
    impl Regions for BadRegions {
        fn region(&self, at: u64, _n: usize) -> Result<Region> {
            let image = at >= BASE && at < BASE + 0x35BDE000;
            Ok(Region {
                allocation: if image {
                    BASE
                } else if at == TABLE {
                    ROOT + u64::from(self.fault == 0) * 0x1000
                } else {
                    ROOT
                },
                kind: if image {
                    RegionKind::Image
                } else if self.fault == 1 {
                    RegionKind::Image
                } else {
                    RegionKind::Private
                },
                readable: self.fault != 2,
                writable: self.fault != 3,
                executable: self.fault == 4,
            })
        }
    }
    for fault in 0..5 {
        let f = Fixture::stock();
        assert!(capture(&f, &BadRegions { fault }).is_err(), "fault {fault}");
    }
    for bad in [0, 0xFFFF, u64::MAX, 0x7FFFFFFFFFF0] {
        let f = Fixture::stock();
        f.rva(ROSTER_ROOT, &bad.to_le_bytes());
        assert!(capture(&f, &f).is_err(), "root {bad:X}");
    }
}

#[test]
#[ignore = "explicit installed Version 2 disk audit; no game launch or process memory access"]
fn actual_installed_version_two_profile_is_disk_only() {
    use std::{
        fs::File,
        io::{Read, Seek, SeekFrom},
        path::PathBuf,
    };
    struct Disk {
        file: RefCell<File>,
        base: u64,
        sections: Vec<(u64, u64, u64)>,
    }
    impl Memory for Disk {
        fn base(&self) -> u64 {
            self.base
        }
        fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
            let rva = at.checked_sub(self.base).context("Disk RVA underflow")?;
            let offset = if rva < 4096 {
                rva
            } else {
                let s = self
                    .sections
                    .iter()
                    .find(|(r, _, n)| {
                        rva >= *r && rva.checked_add(len as u64).is_some_and(|end| end <= r + n)
                    })
                    .context("Disk range is not backed by an image section")?;
                s.1 + rva - s.0
            };
            let mut f = self.file.borrow_mut();
            f.seek(SeekFrom::Start(offset))?;
            let mut b = vec![0; len];
            f.read_exact(&mut b)?;
            Ok(b)
        }
    }
    let path = PathBuf::from(
        std::env::var("C27_FREESTYLE_GAME_EXE").expect("Explicit installed executable required"),
    );
    let verified = crate::provider_access::verify_build(&path).unwrap();
    assert_eq!(verified.build["label"], "Version 2");
    let mut file = File::open(path).unwrap();
    let mut h = [0; 4096];
    file.read_exact(&mut h).unwrap();
    let pe = d(&h, 60) as usize;
    let count = u16::from_le_bytes(h[pe + 6..pe + 8].try_into().unwrap()) as usize;
    let optional = u16::from_le_bytes(h[pe + 20..pe + 22].try_into().unwrap()) as usize;
    let start = pe + 24 + optional;
    assert!(count <= 96 && start + count * 40 <= 4096);
    let sections = (0..count)
        .map(|i| {
            let at = start + i * 40;
            (
                u64::from(d(&h, at + 12)),
                u64::from(d(&h, at + 20)),
                u64::from(d(&h, at + 16)),
            )
        })
        .collect();
    let disk = Disk {
        file: RefCell::new(file),
        base: q(&h, pe + 48),
        sections,
    };
    validate_profile(&disk).unwrap();
    assert_eq!(Tables::read(&disk).unwrap(), Tables::original());
    println!(
        "{}",
        serde_json::json!({"schema":"nba2k27-v2-freestyle-disk-proof-v1","profilePassed":true,"ripAnchors":RIP_ANCHORS.len(),"fieldAnchors":FIELD_ANCHORS.len(),"categoryDwords":4,"changedBytes":0,"gameFilesChanged":false,"gameProcessAccess":false,"livePickerVerified":false})
    );
}

#[test]
fn compact_guard_keeps_all_context_fields_and_cannot_prove_mutable_rows() {
    let f = Fixture::stock();
    let baseline = capture(&f, &f).unwrap();
    let before = f.row_reads.get();
    verify_compact(&f, &f, &baseline).unwrap();
    assert_eq!(
        f.row_reads.get(),
        before,
        "compact guard must not scan rows"
    );
    let mut rows = read(&f, TABLE, baseline.team_count * TEAM_STRIDE).unwrap();
    rows[100] ^= 1; // Same owner/header/membership; mutable content changed.
    f.put(TABLE, &rows);
    verify_compact(&f, &f, &baseline).unwrap();
    let post = capture(&f, &f).unwrap();
    let error = verify_deep_equal(&baseline, &post).unwrap_err();
    assert_eq!(
        error.downcast_ref::<ReadinessChanged>().unwrap().components,
        0x1000
    );
}
#[test]
fn compact_guard_rejects_valid_changed_picker_windows_without_relaxation() {
    for (rva, byte, expected) in [
        (SETUP, 4, 8),
        (SELECTED, 16, 512),
        (OTHER_SELECTED, 8, 1024),
        (MASK_WINDOW, 0, 2048),
    ] {
        let f = Fixture::stock();
        let baseline = capture(&f, &f).unwrap();
        let length = if rva == OTHER_SELECTED {
            12
        } else if rva == SELECTED {
            20
        } else {
            24
        };
        let mut bytes = image(&f, rva, length).unwrap();
        bytes[byte] ^= 1;
        f.rva(rva, &bytes);
        let error = verify_compact(&f, &f, &baseline).unwrap_err();
        assert_eq!(
            error.downcast_ref::<ReadinessChanged>().unwrap().components,
            expected
        );
    }
    let f = Fixture::stock();
    let baseline = capture(&f, &f).unwrap();
    let mut header = read(&f, OWNER + 0x80, 16).unwrap();
    header[4] ^= 1;
    f.put(OWNER + 0x80, &header);
    assert_eq!(
        verify_compact(&f, &f, &baseline)
            .unwrap_err()
            .downcast_ref::<ReadinessChanged>()
            .unwrap()
            .components,
        64
    );
}
#[test]
fn component_telemetry_covers_every_field_and_never_exposes_addresses() {
    let f = Fixture::stock();
    let snapshot = capture(&f, &f).unwrap();
    for field in 0..12 {
        let mut b = snapshot.context.clone();
        match field {
            0 => b.base ^= 1,
            1 => b.mode ^= 1,
            2 => b.alternate ^= 1,
            3 => b.setup[4] ^= 1,
            4 => b.root ^= 1,
            5 => b.owner ^= 1,
            6 => b.header[4] ^= 1,
            7 => b.table ^= 1,
            8 => b.count ^= 1,
            9 => b.category[16] ^= 1,
            10 => b.other[8] ^= 1,
            _ => b.mask[0] ^= 1,
        }
        let error = snapshot.context.require_same(&b).unwrap_err();
        assert_eq!(
            error.downcast_ref::<ReadinessChanged>().unwrap().components,
            1 << field
        );
        assert!(!error.to_string().contains(&format!("{TABLE:X}")));
    }
}
#[test]
fn compact_profile_identity_and_region_failures_are_not_readiness_deferrals() {
    for fault in 0..3 {
        let f = Fixture::stock();
        let baseline = capture(&f, &f).unwrap();
        match fault {
            0 => f.rva(RIP_ANCHORS[0].0, &[0; 7]),
            1 => f.exited.set(true),
            _ => f.private_allocation.set(0),
        }
        let error = verify_compact(&f, &f, &baseline).unwrap_err();
        assert!(!error.is::<ReadinessChanged>());
    }
}
