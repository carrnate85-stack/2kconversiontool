use super::*;
use std::cell::Cell;
const CTX: u64 = 0x30000100000;
const BUCKETS: u64 = 0x30000200000;
const NODE: u64 = 0x30000300000;
const OWNER: u64 = 0x30000400000;
const REGISTRY: u64 = 0x30000500000;
const PACKAGE: u64 = 0x30000600000;
const STORAGE: u64 = 0x30000700000;
const FIELD: u64 = 0x30000800000;
pub(crate) struct Fake {
    inner: crate::rules_v2::tests::Fake,
    broken_region: Option<u64>,
    torn: Option<u64>,
    reads: Cell<usize>,
    writes: usize,
    fail_write: Option<(usize, bool)>,
}
impl Memory for Fake {
    fn base(&self) -> u64 {
        self.inner.base()
    }
    fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
        let mut b = self.inner.read(a, n)?;
        if self.torn == Some(a) {
            let reads = self.reads.get();
            self.reads.set(reads + 1);
            if reads > 0 {
                b[0] ^= 1;
            }
        }
        Ok(b)
    }
}
impl Inspect for Fake {
    fn region(&self, a: u64) -> Result<Region> {
        let image = (self.base()..self.base() + 0x35BDE000).contains(&a);
        Ok(Region {
            start: if image { self.base() } else { a & !0xFFFFF },
            len: if image { 0x35BDE000 } else { 0x100000 },
            allocation: if image { self.base() } else { a & !0xFFFFF },
            kind: if image { 0x1000000 } else { 0x20000 },
            protection: if self.broken_region == Some(a) {
                0x40
            } else {
                4
            },
            committed: true,
        })
    }
}
impl Fake {
    fn put(&mut self, a: u64, b: &[u8]) {
        self.inner.put(a, b)
    }
    fn d(&mut self, a: u64, v: u32) {
        self.put(a, &v.to_le_bytes())
    }
    fn q(&mut self, a: u64, v: u64) {
        self.put(a, &v.to_le_bytes())
    }
    fn menu(&mut self, name: &str) {
        let mode = match name {
            "main-menu" => 0x6509837Bu32,
            "quick-play" => 0x079964C0,
            "match" => 0xB4D37B22,
            _ => 0x92DF06DE,
        };
        self.inner.d(0x0AD07408 + 64, mode);
        self.inner
            .d(0x0AF46870 + 0xEA8, if name == "selection" { 3 } else { 1 });
    }
}
pub(crate) fn fixture() -> Fake {
    let mut f = Fake {
        inner: crate::rules_v2::tests::fixture(),
        broken_region: None,
        torn: None,
        reads: Cell::new(0),
        writes: 0,
        fail_write: None,
    };
    let base = f.base();
    let anchors: serde_json::Value =
        serde_json::from_str(include_str!("../assets/scoreboard-v2-anchors.json")).unwrap();
    for a in anchors["anchors"].as_array().unwrap() {
        f.put(
            base + a["rva"].as_u64().unwrap(),
            &decode(a["hex"].as_str().unwrap()).unwrap(),
        );
    }
    let lifecycle = include_bytes!("../assets/scoreboard-v2-context-lifecycle.bin");
    let mut offset = 0;
    for (rva, bytes, _) in LIFECYCLE_METHODS {
        f.put(base + rva, &lifecycle[offset..offset + bytes]);
        offset += bytes;
    }
    assert_eq!(offset, lifecycle.len());
    f.q(base + VTABLE - 8, base + LOCATOR);
    f.q(base + VTABLE + 0xC8, base + METHOD);
    f.q(base + ACTIVE, 0);
    f.q(base + OTHER, 0);
    f.q(base + RESOURCES, CTX);
    f.put(CTX, &vec![0; 0xB0]);
    f.q(CTX, base + CONTEXT_VTABLE);
    f.d(CTX + 0xA0, 1);
    f.q(CTX + 0xA8, BUCKETS);
    f.q(BUCKETS, NODE);
    f.put(NODE, &vec![0; 64]);
    f.d(NODE, 0x988E62C4);
    f.d(NODE + 4, 0x7DE5F718);
    f.q(NODE + 0x28, OWNER);
    f.q(NODE + 0x18, REGISTRY);
    f.put(OWNER, &vec![0; 0xB8]);
    f.q(OWNER, base + VTABLE);
    f.d(OWNER + 0x54, 0xDCE5273B);
    f.q(OWNER + 0x80, REGISTRY);
    let choices = profile::catalog().unwrap();
    let mut registry = vec![0; 4 + 260 * choices.len()];
    registry[..4].copy_from_slice(&(choices.len() as u32).to_le_bytes());
    for (i, c) in choices.iter().enumerate() {
        let start = 4 + 260 * i;
        registry[start..start + 4].copy_from_slice(&c.resource.to_le_bytes());
        for (j, w) in c.file.encode_utf16().enumerate() {
            registry[start + 4 + j * 2..start + 6 + j * 2].copy_from_slice(&w.to_le_bytes());
        }
    }
    f.put(REGISTRY, &registry);
    f.put(base + PACKAGES, &vec![0; 128 * 24]);
    f.d(base + PACKAGES, 0x0A2ADB76);
    f.q(base + PACKAGES + 8, PACKAGE);
    f.put(PACKAGE, &vec![0; 32]);
    f.q(PACKAGE, STORAGE);
    f.q(PACKAGE + 8, STORAGE);
    f.d(PACKAGE + 16, 1);
    f.d(PACKAGE + 20, 300);
    f.put(STORAGE, &vec![0; 0x88]);
    f.d(STORAGE, profile::STOCK);
    f.q(STORAGE + 0x58, FIELD);
    let stock = profile::choice("default").unwrap();
    for (i, (key, value)) in [
        (0xFE11D138, profile::STOCK),
        (0x2C8A369B, stock.profile[0]),
        (stock.profile[1], stock.profile[2]),
    ]
    .into_iter()
    .enumerate()
    {
        let a = FIELD + i as u64 * 0x100;
        let mut field = vec![0; 96];
        field[..4].copy_from_slice(&key.to_le_bytes());
        field[4..8].copy_from_slice(&0x800005u32.to_le_bytes());
        field[8..12].copy_from_slice(&value.to_le_bytes());
        field[0x50..0x58]
            .copy_from_slice(&if i == 2 { STORAGE + 8 } else { a + 0x100 }.to_le_bytes());
        f.put(a, &field);
    }
    f
}
#[test]
fn exact_v2_reader_preflights_all_19_choices_without_writes() {
    let f = fixture();
    for c in profile::catalog()
        .unwrap()
        .into_iter()
        .filter(|c| c.selectable)
    {
        let s = capture(&f, &c.id, true).unwrap();
        assert_eq!(s.plan().unwrap().wanted, c.resource);
        assert_eq!(s.fields.len(), 3);
    }
    assert!(capture(&f, "mycourt-scrimmage", false).is_err());
    assert!(capture(&f, "flashback-openrun", true).is_err());
}
#[test]
fn native_profile_and_resource_owner_changes_are_rejected() {
    for target in [0x11950CD, VTABLE + 0xC8] {
        let mut f = fixture();
        let a = f.base() + target;
        f.put(a, &[0xCC]);
        assert!(capture(&f, "simple", true).is_err());
    }
    for (a, v) in [
        (OWNER, 0x12345678),
        (OWNER + 0x54, 0x12345678),
        (NODE + 4, 0x12345678),
        (REGISTRY, 4097),
    ] {
        let mut f = fixture();
        f.d(a, v);
        assert!(capture(&f, "simple", true).is_err(), "{a:X}");
    }
}
#[test]
fn resource_chains_duplicate_names_and_partial_table_are_rejected() {
    let mut f = fixture();
    f.q(NODE + 0x38, NODE);
    assert!(capture(&f, "simple", true).is_err());
    let mut f = fixture();
    f.put(REGISTRY + 4 + 260 + 4, &[b'X', 0]);
    assert!(capture(&f, "simple", true).is_err());
    let mut f = fixture();
    f.d(REGISTRY, 22);
    assert!(capture(&f, "simple", true).is_err());
}
#[test]
fn package_loading_ambiguity_bounds_and_cycles_are_rejected() {
    for case in 0..5 {
        let mut f = fixture();
        match case {
            0 => f.put(f.base() + PACKAGES + 16, &[1]),
            1 => {
                f.d(f.base() + PACKAGES + 24, 0x0A2ADB76);
                f.q(f.base() + PACKAGES + 32, PACKAGE);
            }
            2 => f.d(PACKAGE + 20, 301),
            3 => {
                f.d(STORAGE, 0);
                f.q(STORAGE + 0x80, STORAGE);
            }
            _ => f.q(FIELD + 0x50, FIELD),
        }
        assert!(capture(&f, "simple", true).is_err(), "case{case}");
    }
}
#[test]
fn capture_rejects_active_instances_wrong_modes_executable_and_torn_memory() {
    let mut f = fixture();
    f.q(f.base() + ACTIVE, OWNER);
    assert!(capture(&f, "simple", true).is_err());
    let mut f = fixture();
    f.menu("main-menu");
    assert!(capture(&f, "simple", true).is_err());
    let mut f = fixture();
    f.broken_region = Some(FIELD);
    assert!(capture(&f, "simple", true).is_err());
    let mut f = fixture();
    f.torn = Some(NODE);
    assert!(capture(&f, "simple", true).is_err());
}
#[test]
fn full_owned_prefix_is_rechecked_before_every_scalar() {
    let mut f = fixture();
    let s = capture(&f, "era-1990", true).unwrap();
    let p = s.plan().unwrap();
    assert_eq!(p.changes.len(), 3);
    for i in 0..p.changes.len() {
        guard_progress(&f, &s, &p, i).unwrap();
        let c = &p.changes[i];
        f.put(c.address, &c.after);
    }
    guard_progress(&f, &s, &p, 3).unwrap();
    f.put(FIELD + 30, &[9]);
    assert!(guard_progress(&f, &s, &p, 3).is_err());
}
#[test]
fn interrupted_partial_group_restores_only_owned_prefix_at_main_and_quick_play() {
    for menu in ["main-menu", "quick-play"] {
        for applied in 1..=3 {
            let mut f = fixture();
            let s = capture(&f, "era-1990", true).unwrap();
            let p = s.plan().unwrap();
            for c in p.changes.iter().take(applied) {
                f.put(c.address, &c.after);
            }
            f.menu(menu);
            let (_, reverse) = restore_plan(&f, &s, &p, applied).unwrap();
            assert_eq!(reverse.changes.len(), applied);
            for (i, c) in reverse.changes.iter().enumerate() {
                guard_restore(&f, &s, &p, applied, i).unwrap();
                f.put(c.address, &c.after);
            }
            guard_restore(&f, &s, &p, applied, applied).unwrap();
            f.menu("selection");
            assert_eq!(capture(&f, "era-1990", true).unwrap().fields, s.fields);
        }
    }
}
#[test]
fn restore_never_adopts_foreign_owner_or_replacement_allocation() {
    let mut f = fixture();
    let s = capture(&f, "simple", true).unwrap();
    let p = s.plan().unwrap();
    for c in &p.changes {
        f.put(c.address, &c.after);
    }
    f.menu("main-menu");
    f.d(FIELD + 8, 0x12345678);
    assert!(restore_plan(&f, &s, &p, p.changes.len()).is_err());
    let mut f = fixture();
    let s = capture(&f, "simple", true).unwrap();
    let p = s.plan().unwrap();
    f.put(FIELD, &p.changes[0].after);
    f.menu("match");
    assert!(restore_plan(&f, &s, &p, 1).is_err());
}
impl crate::scoreboard_session::Executor for Fake {
    fn write_header(
        &mut self,
        permit: &crate::scoreboard_session::Permit<'_>,
        durable: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        durable()?;
        permit.verify_before(self)?;
        let c = permit.change()?;
        self.writes += 1;
        let fail = self.fail_write.is_some_and(|(n, _)| n == self.writes);
        if !fail || self.fail_write.unwrap().1 {
            assert_eq!(c.after.len(), 12);
            self.put(c.address, &c.after);
        }
        ensure!(!fail, "Injected scoreboard write failure");
        Ok(())
    }
}
#[derive(Default)]
struct Journal {
    intents: usize,
    steps: Vec<(usize, usize)>,
    resolved: usize,
    fail_intent: bool,
    fail_step: bool,
    fail_verify: bool,
}
impl crate::scoreboard_session::Journal for Journal {
    fn intent(&mut self, _: &Snapshot, _: &Plan) -> Result<()> {
        self.intents += 1;
        ensure!(!self.fail_intent, "Injected durable intent failure");
        Ok(())
    }
    fn progress(&mut self, a: usize, r: usize) -> Result<()> {
        self.steps.push((a, r));
        ensure!(!self.fail_step, "Injected progress failure");
        Ok(())
    }
    fn restore_intent(&mut self, _: &Snapshot, _: &Plan, _: usize, _: usize) -> Result<()> {
        Ok(())
    }
    fn verify(&self, _: &crate::scoreboard_session::Permit<'_>) -> Result<()> {
        ensure!(!self.fail_verify, "Injected durable proof readback failure");
        Ok(())
    }
    fn resolved(&mut self) -> Result<()> {
        self.resolved += 1;
        Ok(())
    }
}
#[test]
fn lost_durable_proof_and_parent_eof_prevent_new_effects() {
    use crate::scoreboard_session::{Service, Status};
    let mut f = fixture();
    let mut j = Journal {
        fail_verify: true,
        ..Default::default()
    };
    let mut s = Service::new("era-1990").unwrap();
    s.tick(&mut f, &mut j, true);
    assert!(matches!(s.tick(&mut f, &mut j, true), Status::Held(_)));
    assert_eq!(f.writes, 0);
    let mut f = fixture();
    let mut j = Journal::default();
    let mut s = Service::new("simple").unwrap();
    s.tick_owned(&mut f, &mut j, true, false);
    assert_eq!(s.tick_owned(&mut f, &mut j, true, false), Status::Waiting);
    assert_eq!(f.writes, 0);
    assert_eq!(j.intents, 0);
}
#[test]
fn service_applies_only_stable_selection_restores_both_menus_and_rearms() {
    use crate::scoreboard_session::{Service, Status};
    for menu in ["main-menu", "quick-play"] {
        let mut f = fixture();
        let mut j = Journal::default();
        let mut s = Service::new("era-1990").unwrap();
        assert_eq!(s.tick(&mut f, &mut j, true), Status::Waiting);
        assert_eq!(s.tick(&mut f, &mut j, true), Status::Ready);
        assert_eq!(f.writes, 3);
        assert_eq!(j.intents, 1);
        f.menu("match");
        assert_eq!(s.tick(&mut f, &mut j, true), Status::Waiting);
        assert_eq!(f.writes, 3);
        f.menu(menu);
        assert_eq!(s.tick(&mut f, &mut j, true), Status::Waiting);
        assert_eq!(s.tick(&mut f, &mut j, true), Status::Restored);
        assert_eq!(f.writes, 6);
        assert!(!s.owns_mutation());
        f.menu("selection");
        assert_eq!(s.tick(&mut f, &mut j, true), Status::Waiting);
        assert_eq!(s.tick(&mut f, &mut j, true), Status::Ready);
        assert_eq!(f.writes, 9);
    }
}
#[test]
fn service_intent_failure_makes_no_write_and_keeps_recovery_state() {
    use crate::scoreboard_session::{Service, Status};
    let mut f = fixture();
    let mut j = Journal {
        fail_intent: true,
        ..Default::default()
    };
    let mut s = Service::new("simple").unwrap();
    s.tick(&mut f, &mut j, true);
    assert!(matches!(s.tick(&mut f, &mut j, true), Status::Held(_)));
    assert_eq!(f.writes, 0);
    assert!(s.owns_mutation());
}
#[test]
fn service_write_and_journal_failures_restore_confirmed_owned_prefix_only() {
    use crate::scoreboard_session::{Service, Status};
    for after in [false, true] {
        let mut f = fixture();
        f.fail_write = Some((2, after));
        let mut j = Journal::default();
        let mut s = Service::new("era-1990").unwrap();
        s.tick(&mut f, &mut j, true);
        assert!(matches!(s.tick(&mut f, &mut j, true), Status::Held(_)));
        assert!(s.owns_mutation());
        f.fail_write = None;
        f.menu("quick-play");
        s.tick(&mut f, &mut j, true);
        assert_eq!(s.tick(&mut f, &mut j, true), Status::Restored);
        f.menu("selection");
        assert_eq!(
            capture(&f, "default", true)
                .unwrap()
                .plan()
                .unwrap()
                .changes
                .len(),
            0
        );
    }
    let mut f = fixture();
    let mut j = Journal {
        fail_step: true,
        ..Default::default()
    };
    let mut s = Service::new("simple").unwrap();
    s.tick(&mut f, &mut j, true);
    assert!(matches!(s.tick(&mut f, &mut j, true), Status::Held(_)));
    assert_eq!(f.writes, 1);
    j.fail_step = false;
    f.menu("main-menu");
    s.tick(&mut f, &mut j, true);
    assert_eq!(s.tick(&mut f, &mut j, true), Status::Restored);
}
#[test]
fn selection_changes_restore_then_fresh_capture_and_foreign_edits_never_reapply() {
    use crate::scoreboard_session::{Service, Status};
    let mut f = fixture();
    let mut j = Journal::default();
    let mut s = Service::new("era-1990").unwrap();
    s.tick(&mut f, &mut j, true);
    assert_eq!(s.tick(&mut f, &mut j, true), Status::Ready);
    s.choose("simple").unwrap();
    assert_eq!(s.tick(&mut f, &mut j, true), Status::Restored);
    assert_eq!(f.writes, 6);
    assert_eq!(s.tick(&mut f, &mut j, true), Status::Waiting);
    assert_eq!(s.tick(&mut f, &mut j, true), Status::Ready);
    assert_eq!(f.writes, 7);
    f.put(FIELD + 40, &[9]);
    let before = f.writes;
    assert!(matches!(s.tick(&mut f, &mut j, true), Status::Held(_)));
    assert!(matches!(s.tick(&mut f, &mut j, true), Status::Held(_)));
    assert_eq!(f.writes, before);
    assert!(s.owns_mutation());
}
#[test]
fn restoration_rejects_replaced_rules_owner_even_with_matching_descriptor() {
    let mut f = fixture();
    let s = capture(&f, "simple", true).unwrap();
    let p = s.plan().unwrap();
    for c in &p.changes {
        f.put(c.address, &c.after);
    }
    f.menu("main-menu");
    f.q(f.base() + 0xAB5CE20, 0x30000900000);
    f.put(0x30000900000, &vec![0; 0xA9C]);
    assert!(restore_plan(&f, &s, &p, p.changes.len()).is_err());
    assert!(guard_restore(&f, &s, &p, p.changes.len(), 0).is_err());
}
#[test]
#[ignore = "Explicit installed PE-only native scoreboard verifier; no process/game-file changes"]
fn actual_installed_v2_scoreboard_profile_is_verified_by_real_rust_reader() {
    use sha2::{Digest, Sha256};
    use std::{
        cell::RefCell,
        fs::File,
        io::{Read, Seek, SeekFrom},
    };
    let path = std::env::var_os("C27_SCOREBOARD_V2_DISK").expect("Explicit game EXE path required");
    let mut file = File::open(path).unwrap();
    let mut hash = Sha256::new();
    let mut bytes = vec![0; 1024 * 1024];
    loop {
        let n = file.read(&mut bytes).unwrap();
        if n == 0 {
            break;
        }
        hash.update(&bytes[..n]);
    }
    assert_eq!(format!("{:X}", hash.finalize()), SHA);
    file.seek(SeekFrom::Start(0)).unwrap();
    let mut header = vec![0; 4096];
    file.read_exact(&mut header).unwrap();
    let pe = d(&header, 0x3C).unwrap() as usize;
    let count = u16::from_le_bytes(header[pe + 6..pe + 8].try_into().unwrap()) as usize;
    let optional = u16::from_le_bytes(header[pe + 20..pe + 22].try_into().unwrap()) as usize;
    let mut sections = Vec::new();
    for i in 0..count {
        let a = pe + 24 + optional + i * 40;
        sections.push((
            d(&header, a + 12).unwrap() as u64,
            d(&header, a + 16).unwrap() as u64,
            d(&header, a + 20).unwrap() as u64,
        ));
    }
    struct Disk {
        file: RefCell<File>,
        sections: Vec<(u64, u64, u64)>,
    }
    impl Memory for Disk {
        fn base(&self) -> u64 {
            0x140000000
        }
        fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
            let rva = a.checked_sub(self.base()).context("Below PE base")?;
            let pos = if rva < 4096 {
                rva
            } else {
                let (start, _, raw) = self
                    .sections
                    .iter()
                    .find(|(r, s, _)| {
                        rva >= *r && rva.checked_add(n as u64).is_some_and(|e| e <= r + s)
                    })
                    .context("PE RVA not disk backed")?;
                raw + rva - start
            };
            let mut f = self.file.borrow_mut();
            f.seek(SeekFrom::Start(pos))?;
            let mut b = vec![0; n];
            f.read_exact(&mut b)?;
            Ok(b)
        }
    }
    verify_profile(&Disk {
        file: RefCell::new(file),
        sections,
    })
    .unwrap();
    println!(
        "Actual Rust scoreboard verifier passed all18 independently identified native consumers,39 rule getters/context class and exact resource vtable/dispatch against SHA-bound Version2 disk image; no process access"
    );
}
#[test]
fn version_two_circular_resource_bucket_wraps_to_head_with_exact_back_links() {
    // Live 0.48: buckets are circular doubly-linked (+0x30 prev, +0x38 next).
    let other = NODE + 0x100;
    let circular = || {
        let mut f = fixture();
        f.put(other, &vec![0; 64]);
        f.d(other, 0x744E62C4);
        f.q(NODE + 0x38, other);
        f.q(other + 0x38, NODE);
        f.q(NODE + 0x30, other);
        f.q(other + 0x30, NODE);
        f
    };
    capture(&circular(), "simple", true).unwrap();
    let mut broken = circular();
    broken.q(other + 0x30, other);
    assert!(capture(&broken, "simple", true).is_err());
    let mut tail = circular();
    tail.q(NODE + 0x30, 0);
    assert!(capture(&tail, "simple", true).is_err());
}

#[test]
fn restore_descriptor_mismatch_reports_only_first_span_metadata_and_never_writes() {
    let mut f = fixture();
    let mut s = capture(&f, "simple", true).unwrap();
    let p = s.plan().unwrap();
    for change in &p.changes {
        f.put(change.address, &change.after);
    }
    f.menu("main-menu");
    // A retained snapshot mismatch cannot authorize adopting the live owner.
    // Corrupt just its opaque byte proof so fresh capture remains well formed.
    s.spans[0].bytes[0] ^= 1;
    let writes = f.writes;
    let message = restore_plan(&f, &s, &p, p.changes.len())
        .unwrap_err()
        .to_string();
    assert!(message.contains("descriptor ownership data changed: span 0"));
    assert!(message.contains("changed byte offset Some(0)"));
    assert!(message.contains("previous Some(("));
    assert!(!message.contains("bytes: ["));
    assert_eq!(f.writes, writes);
}

#[test]
fn class_bound_context_bookkeeping_changes_do_not_block_owned_progress_or_restore() {
    let mut f = fixture();
    let s = capture(&f, "era-1990", true).unwrap();
    let p = s.plan().unwrap();
    assert!(s.spans.iter().all(|span| {
        span.address + span.bytes.len() as u64 <= CTX + 8 || span.address >= CTX + 0xA0
    }));
    for (i, c) in p.changes.iter().enumerate() {
        // Exact live failures occurred at +48 and +68. Deliberately opaque
        // values across that block are never adopted or dereferenced.
        f.q(
            CTX + if i % 2 == 0 { 0x48 } else { 0x68 },
            0xAABBCCDD00000001 + i as u64,
        );
        guard_progress(&f, &s, &p, i).unwrap();
        f.put(c.address, &c.after);
    }
    f.menu("main-menu");
    f.q(CTX + 0x48, 0);
    f.q(CTX + 0x68, 0);
    let (_, reverse) = restore_plan(&f, &s, &p, p.changes.len()).unwrap();
    for (i, c) in reverse.changes.iter().enumerate() {
        f.q(
            CTX + if i % 2 == 0 { 0x48 } else { 0x68 },
            0xF0E0D0C000000001 + i as u64,
        );
        guard_restore(&f, &s, &p, p.changes.len(), i).unwrap();
        f.put(c.address, &c.after);
    }
    guard_restore(&f, &s, &p, p.changes.len(), p.changes.len()).unwrap();
    assert_eq!(f.writes, 0); // This fixture only simulates effects, no Executor call.
}
#[test]
fn lifecycle_exception_keeps_neighbors_class_header_buckets_and_semantic_code_exact() {
    for offset in [0u64, 0xA0, 0xA4, 0xA8] {
        let mut f = fixture();
        let s = capture(&f, "simple", true).unwrap();
        let p = s.plan().unwrap();
        let before = f.inner.read(CTX + offset, 1).unwrap()[0];
        f.put(CTX + offset, &[before ^ 1]);
        assert!(guard_progress(&f, &s, &p, 0).is_err(), "offset{offset:X}");
        assert!(restore_plan(&f, &s, &p, 0).is_err(), "offset{offset:X}");
    }
    for (rva, bytes, _) in LIFECYCLE_METHODS {
        let mut f = fixture();
        let at = f.base() + rva + bytes as u64 - 1;
        let byte = f.inner.read(at, 1).unwrap()[0];
        f.put(at, &[byte ^ 1]);
        assert!(capture(&f, "simple", true).is_err(), "method{rva:X}");
    }
    let mut f = fixture();
    f.q(CTX, f.base() + CONTEXT_VTABLE + 8);
    assert!(capture(&f, "simple", true).is_err());
}

#[test]
fn every_opaque_context_bookkeeping_byte_is_non_authoritative_but_instance_serial_stays_exact() {
    for offset in 0x08..0xA0 {
        let mut f = fixture();
        let s = capture(&f, "simple", true).unwrap();
        let p = s.plan().unwrap();
        let before = f.inner.read(CTX + offset, 1).unwrap()[0];
        f.put(CTX + offset, &[before ^ 0xA5]);
        guard_progress(&f, &s, &p, 0).unwrap();
        restore_plan(&f, &s, &p, 0).unwrap();
        assert_eq!(f.writes, 0, "offset{offset:X}");
    }
    // The per-resource serial assigned by context+90 remains part of the
    // selected resource's exact B8-byte owner proof, unlike the global serial.
    for address in [OWNER + 0xB0, OWNER + 0x28, FIELD + 8] {
        let mut f = fixture();
        let s = capture(&f, "simple", true).unwrap();
        let p = s.plan().unwrap();
        let before = f.inner.read(address, 1).unwrap()[0];
        f.put(address, &[before ^ 1]);
        assert!(guard_progress(&f, &s, &p, 0).is_err());
        assert!(restore_plan(&f, &s, &p, 0).is_err());
    }
    let mut f = fixture();
    let s = capture(&f, "simple", true).unwrap();
    let p = s.plan().unwrap();
    f.broken_region = Some(CTX);
    assert!(guard_progress(&f, &s, &p, 0).is_err());
    assert!(restore_plan(&f, &s, &p, 0).is_err());
    let mut f = fixture();
    let s = capture(&f, "simple", true).unwrap();
    let p = s.plan().unwrap();
    f.q(f.base() + RESOURCES, CTX + 8);
    assert!(guard_progress(&f, &s, &p, 0).is_err());
    assert!(restore_plan(&f, &s, &p, 0).is_err());
}

#[test]
fn unrelated_registry_rows_may_load_after_apply_but_not_during_forward_publication() {
    let other_index = profile::catalog()
        .unwrap()
        .iter()
        .position(|c| c.resource != profile::STOCK && c.id != "era-1990")
        .unwrap();
    let unrelated_name = REGISTRY + 4 + other_index as u64 * 260 + 4;

    let mut forward = fixture();
    let snapshot = capture(&forward, "era-1990", true).unwrap();
    let plan = snapshot.plan().unwrap();
    let byte = forward.inner.read(unrelated_name, 1).unwrap()[0];
    forward.put(unrelated_name, &[byte ^ 1]);
    assert!(guard_progress(&forward, &snapshot, &plan, 0).is_err());

    let mut ready = fixture();
    let snapshot = capture(&ready, "era-1990", true).unwrap();
    let plan = snapshot.plan().unwrap();
    for change in &plan.changes {
        ready.put(change.address, &change.after);
    }
    let byte = ready.inner.read(unrelated_name, 1).unwrap()[0];
    ready.put(unrelated_name, &[byte ^ 1]);
    guard_progress(&ready, &snapshot, &plan, plan.changes.len()).unwrap();
    ready.menu("main-menu");
    let (_, reverse) = restore_plan(&ready, &snapshot, &plan, plan.changes.len()).unwrap();
    assert_eq!(reverse.changes.len(), plan.changes.len());
}

#[test]
fn selected_registry_row_and_uniqueness_remain_exact_after_apply() {
    let choice = profile::choice("era-1990").unwrap();
    let choices = profile::catalog().unwrap();
    let selected_index = choices
        .iter()
        .position(|c| c.resource == choice.resource)
        .unwrap();
    let other_index = choices
        .iter()
        .position(|c| c.resource != profile::STOCK && c.resource != choice.resource)
        .unwrap();

    let completed = |mut f: Fake| {
        let snapshot = capture(&f, "era-1990", true).unwrap();
        let plan = snapshot.plan().unwrap();
        for change in &plan.changes {
            f.put(change.address, &change.after);
        }
        (f, snapshot, plan)
    };

    let (mut changed, snapshot, plan) = completed(fixture());
    let selected_name = REGISTRY + 4 + selected_index as u64 * 260 + 4;
    let byte = changed.inner.read(selected_name, 1).unwrap()[0];
    changed.put(selected_name, &[byte ^ 1]);
    assert!(guard_progress(&changed, &snapshot, &plan, plan.changes.len()).is_err());

    let (mut duplicate, snapshot, plan) = completed(fixture());
    let other = REGISTRY + 4 + other_index as u64 * 260;
    duplicate.d(other, choice.resource);
    assert!(guard_progress(&duplicate, &snapshot, &plan, plan.changes.len()).is_err());

    let (mut changed_id, snapshot, plan) = completed(fixture());
    changed_id.d(other, 0x12345678);
    assert!(guard_progress(&changed_id, &snapshot, &plan, plan.changes.len()).is_err());
    changed_id.menu("main-menu");
    assert!(restore_plan(&changed_id, &snapshot, &plan, plan.changes.len()).is_err());
}
