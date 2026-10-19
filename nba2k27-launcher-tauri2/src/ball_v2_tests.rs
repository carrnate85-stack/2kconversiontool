use super::*;
use crate::{
    blacktop_arena::Region,
    rules_v2::tests::{Fake as RulesFake, fixture as rules_fixture},
};
use std::cell::Cell;
#[test]
fn source_records_batch_eight_rows_without_caching_across_captures() {
    struct Counted {
        calls: Cell<usize>,
        value: Cell<u8>,
        sparse: bool,
    }
    impl Memory for Counted {
        fn base(&self) -> u64 {
            RECORDS
        }
        fn read(&self, _: u64, n: usize) -> Result<Vec<u8>> {
            self.calls.set(self.calls.get() + 1);
            ensure!(!self.sparse || n == ball::STRIDE, "Sparse block");
            Ok(vec![self.value.get(); n])
        }
    }
    let m = Counted {
        calls: Cell::new(0),
        value: Cell::new(7),
        sparse: false,
    };
    let mut cache = RecordBlocks::new();
    for i in 0..500 {
        assert_eq!(
            source_record(&m, RECORDS, i, &mut cache).unwrap(),
            vec![7; ball::STRIDE]
        );
    }
    assert_eq!(m.calls.get(), 63);
    assert!(source_record(&m, RECORDS, 500, &mut cache).is_err());
    // Each new stability observation starts a fresh cache, observing mutations.
    m.value.set(9);
    assert_eq!(
        source_record(&m, RECORDS, 1, &mut RecordBlocks::new()).unwrap()[0],
        9
    );
    let m = Counted {
        calls: Cell::new(0),
        value: Cell::new(3),
        sparse: true,
    };
    let mut cache = RecordBlocks::new();
    for i in 0..8 {
        assert_eq!(source_record(&m, RECORDS, i, &mut cache).unwrap()[0], 3);
    }
    // A failed bulk span is attempted once, not once per individual row.
    assert_eq!(m.calls.get(), 9);
}
const RECORDS: u64 = 0x60000000000;
const SOURCE_MODEL: u32 = 0x12345678;
pub(crate) struct Fake {
    inner: RulesFake,
    protection: u32,
    kind: u32,
    torn_ref: bool,
    ref_reads: Cell<usize>,
}
impl Memory for Fake {
    fn base(&self) -> u64 {
        self.inner.base()
    }
    fn read(&self, address: u64, len: usize) -> Result<Vec<u8>> {
        let mut bytes = self.inner.read(address, len)?;
        if address == self.base() + REFERENCES + 0x18 {
            let n = self.ref_reads.get();
            self.ref_reads.set(n + 1);
            if self.torn_ref && n > 0 {
                bytes[0] ^= 1;
            }
        }
        Ok(bytes)
    }
}
impl Inspect for Fake {
    fn region(&self, address: u64) -> Result<Region> {
        ensure!(
            (RECORDS..RECORDS + 500 * ball::STRIDE as u64).contains(&address),
            "Unknown fixture ball allocation"
        );
        Ok(Region {
            start: RECORDS,
            len: 500 * ball::STRIDE as u64,
            allocation: RECORDS,
            kind: self.kind,
            protection: self.protection,
            committed: true,
        })
    }
}
fn chosen() -> &'static str {
    &ball::choices()
        .iter()
        .find(|c| !ball::current(&c.id))
        .unwrap()
        .id
}
fn original() -> Vec<u8> {
    let mut row = vec![0; ball::STRIDE];
    row[4..8].copy_from_slice(&ball::NORMAL.to_le_bytes());
    row[ball::FIELD..ball::FIELD + ball::NORMAL_FILE.len()]
        .copy_from_slice(ball::NORMAL_FILE.as_bytes());
    row
}
fn record(model: u32, name: &str) -> Vec<u8> {
    let mut row = vec![0; ball::STRIDE];
    row[4..8].copy_from_slice(&model.to_le_bytes());
    row[ball::FIELD..ball::FIELD + name.len()].copy_from_slice(name.as_bytes());
    row
}
pub(crate) fn fixture() -> Fake {
    let mut inner = rules_fixture();
    let value = catalog().unwrap();
    for anchor in value["anchors"].as_array().unwrap() {
        inner.put(
            inner.base() + anchor["rva"].as_u64().unwrap(),
            &hex(anchor["hex"].as_str().unwrap()).unwrap(),
        );
    }
    inner.q(HOLDER, 0x50000000000);
    inner.put(0x50000000018, &(RECORDS - RECORD_OFFSET).to_le_bytes());
    inner.q(REGISTRY, 0x70000000000);
    inner.d(REGISTRY + 8, 3);
    inner.put(
        0x70000000000,
        &[
            ball::NORMAL.to_le_bytes(),
            0u32.to_le_bytes(),
            SOURCE_MODEL.to_le_bytes(),
            1u32.to_le_bytes(),
            (SOURCE_MODEL + 1).to_le_bytes(),
            2u32.to_le_bytes(),
        ]
        .concat(),
    );
    inner.put(RECORDS, &original());
    inner.put(
        RECORDS + ball::STRIDE as u64,
        &record(SOURCE_MODEL, chosen().strip_prefix("balls/").unwrap()),
    );
    inner.put(
        RECORDS + 2 * ball::STRIDE as u64,
        &record(SOURCE_MODEL + 1, "ball_unselected_test.iff"),
    );
    inner.put(inner.base() + REFERENCES + 0x18, &0u32.to_le_bytes());
    inner.put(inner.base() + RENDERER + 0x28, &[0; 48]);
    inner.put(0x30000000000 + 0xDA4, &ball::NORMAL.to_le_bytes());
    Fake {
        inner,
        protection: 4,
        kind: 0x20000,
        torn_ref: false,
        ref_reads: Cell::new(0),
    }
}
impl Fake {
    pub(crate) fn patch(&mut self, at: u64, bytes: &[u8]) {
        self.inner.put(at, bytes);
    }
}
fn manifest() -> Vec<u8> {
    format!("{},0A,100,50\n", chosen()).into_bytes()
}
fn apply(f: &mut Fake, edit: &Edit) {
    assert_eq!(read(f, edit.at, edit.before.len()).unwrap(), edit.before);
    f.inner.put(edit.at, &edit.after);
}
#[test]
fn ball_v2_all_native_layout_and_lifetime_anchors_are_required() {
    let mut f = fixture();
    verify_profile(&f).unwrap();
    for anchor in catalog().unwrap()["anchors"].as_array().unwrap() {
        let at = f.base() + anchor["rva"].as_u64().unwrap();
        let bytes = hex(anchor["hex"].as_str().unwrap()).unwrap();
        f.inner.put(at, &[bytes[0] ^ 1]);
        assert!(verify_profile(&f).is_err(), "Accepted {}", anchor["label"]);
        f.inner.put(at, &bytes);
    }
}
#[test]
fn ball_v2_catalog_resolution_changes_only_unloaded_normal_filename() {
    let mut f = fixture();
    let before = capture(&f).unwrap();
    assert!(before.can_prepare());
    let p = prepare(&f, &before, chosen(), &manifest()).unwrap();
    p.validate_before(&f).unwrap();
    assert_eq!(p.edit.at, RECORDS + ball::FIELD as u64);
    assert!(p.edit.after.len() <= 128);
    assert_eq!(p.source.model, SOURCE_MODEL);
    apply(&mut f, &p.edit);
    let after = p.validate_after(&f).unwrap();
    assert_eq!(&after.record[..ball::FIELD], &before.record[..ball::FIELD]);
    assert_eq!(
        &after.record[ball::FIELD + p.edit.after.len()..],
        &before.record[ball::FIELD + p.edit.after.len()..]
    );
    assert_eq!(
        u32::from_le_bytes(after.record[4..8].try_into().unwrap()),
        ball::NORMAL
    );
    assert_eq!(after.references, 0);
    assert_eq!(after.resource, vec![0; 48]);
    assert_eq!(after.native_ball, ball::NORMAL);
}
#[test]
fn ball_v2_modified_plan_cannot_redirect_or_enlarge_the_filename_write() {
    let mut f = fixture();
    let s = capture(&f).unwrap();
    let plan = prepare(&f, &s, chosen(), &manifest()).unwrap();
    for mutation in 0..8 {
        let mut p = plan.clone();
        match mutation {
            0 => p.edit.at += 1,
            1 => p.edit.after.push(0),
            2 => p.edit.before[0] ^= 1,
            3 => p.source.model += 1,
            4 => p.source.index += 1,
            5 => p.original.record[0] ^= 1,
            6 => p.selected = "current".into(),
            _ => p.source.address += ball::STRIDE as u64,
        }
        assert!(
            p.validate_before(&f).is_err(),
            "Accepted altered plan {mutation}"
        );
        assert!(
            p.restore(&f).is_err(),
            "Accepted altered restore {mutation}"
        );
    }
    apply(&mut f, &plan.edit);
    for mutation in 0..2 {
        let mut p = plan.clone();
        if mutation == 0 {
            p.edit.at += 1;
        } else {
            p.edit.after.push(0);
        }
        assert!(p.validate_after(&f).is_err());
        assert!(p.restore(&f).is_err());
    }
}
#[test]
fn ball_v2_two_matches_rearm_after_main_and_quick_play_without_touching_counters() {
    let mut f = fixture();
    for exit in [0x6509837Bu32, 0x079964C0] {
        f.inner.d(0xAD07408 + 64, 0x92DF06DE);
        f.inner.d(0xAF46870 + 0xEA8, 3);
        let s = capture(&f).unwrap();
        let p = prepare(&f, &s, chosen(), &manifest()).unwrap();
        apply(&mut f, &p.edit);
        f.inner.d(0xAD07408 + 64, 0xB4D37B22);
        f.inner.d(0xAF46870 + 0xEA8, 1);
        f.inner
            .put(f.base() + REFERENCES + 0x18, &1u32.to_le_bytes());
        let mut resource = [0; 48];
        resource[16] = 1;
        f.inner.put(f.base() + RENDERER + 0x28, &resource);
        assert!(p.restore(&f).is_err());
        f.inner.d(0xAD07408 + 64, exit);
        assert!(p.restore(&f).is_err());
        f.inner
            .put(f.base() + REFERENCES + 0x18, &0u32.to_le_bytes());
        f.inner.put(f.base() + RENDERER + 0x28, &[0; 48]);
        let s = capture(&f).unwrap();
        assert!(s.can_restore());
        let e = p.restore(&f).unwrap();
        apply(&mut f, &e);
        assert_eq!(capture(&f).unwrap().record, original());
    }
}
#[test]
fn ball_v2_every_resource_byte_nonzero_and_negative_refs_refuse_filename_changes() {
    let mut f = fixture();
    for index in 0..48 {
        let mut resource = [0; 48];
        resource[index] = 1;
        f.inner.put(f.base() + RENDERER + 0x28, &resource);
        assert!(!capture(&f).unwrap().can_prepare());
    }
    f.inner.put(f.base() + RENDERER + 0x28, &[0; 48]);
    for count in [-1i32, 1, i32::MAX] {
        f.inner
            .put(f.base() + REFERENCES + 0x18, &count.to_le_bytes());
        assert!(!capture(&f).unwrap().can_prepare());
    }
}
#[test]
fn ball_v2_unavailable_duplicate_manifest_and_unknown_choices_never_produce_edits() {
    let mut f = fixture();
    let s = capture(&f).unwrap();
    for id in [
        "current",
        "CURRENT",
        "balls/ball_does_not_exist.iff",
        "../ball_bad.iff",
    ] {
        assert!(prepare(&f, &s, id, &manifest()).is_err());
    }
    assert!(prepare(&f, &s, chosen(), b"balls/ball_other.iff,0A,0,0").is_err());
    f.inner.put(
        RECORDS + 2 * ball::STRIDE as u64,
        &record(SOURCE_MODEL + 1, chosen().strip_prefix("balls/").unwrap()),
    );
    let s = capture(&f).unwrap();
    assert!(resolve(&f, &s, chosen()).is_err());
    f.inner.put(
        RECORDS + ball::STRIDE as u64,
        &record(SOURCE_MODEL, "ball_absent_test.iff"),
    );
    f.inner.put(
        RECORDS + 2 * ball::STRIDE as u64,
        &record(SOURCE_MODEL + 1, "ball_other_test.iff"),
    );
    let s = capture(&f).unwrap();
    assert!(resolve(&f, &s, chosen()).is_err());
}
#[test]
fn ball_v2_wrong_allocations_torn_lifetimes_and_other_native_balls_are_not_adopted() {
    let mut f = fixture();
    f.protection = 2;
    assert!(capture(&f).is_err());
    let mut f = fixture();
    f.kind = 0x1000000;
    assert!(capture(&f).is_err());
    let mut f = fixture();
    f.torn_ref = true;
    assert!(capture(&f).is_err());
    let mut f = fixture();
    f.inner
        .put(0x30000000000 + 0xDA4, &0xDEADBEEFu32.to_le_bytes());
    assert!(!capture(&f).unwrap().can_prepare());
    f.inner
        .put(0x30000000000 + 0xDA4, &ball::DEFAULT.to_le_bytes());
    assert!(!capture(&f).unwrap().can_prepare());
    f.inner.put(0x30000000000 + 0xDA4, &0u32.to_le_bytes());
    assert!(!capture(&f).unwrap().can_prepare());
    f.inner
        .put(0x30000000000 + 0xDA4, &0x42A7A5EFu32.to_le_bytes());
    assert!(!capture(&f).unwrap().can_prepare());
    f.inner
        .put(0x30000000000 + 0xDA4, &ball::NORMAL.to_le_bytes());
    assert!(capture(&f).unwrap().can_prepare());
}
#[test]
fn ball_v2_four_spaced_stable_samples_reject_fast_polls_state_changes_and_clock_rewind() {
    let mut f = fixture();
    let s = capture(&f).unwrap();
    let mut stable = Stable::default();
    assert!(!stable.observe(&s, Action::Prepare, 0).unwrap());
    assert!(!stable.observe(&s, Action::Prepare, 99).unwrap());
    assert!(!stable.observe(&s, Action::Prepare, 100).unwrap());
    assert!(!stable.observe(&s, Action::Prepare, 200).unwrap());
    assert!(stable.observe(&s, Action::Prepare, 300).unwrap());
    assert!(stable.observe(&s, Action::Prepare, 299).is_err());
    f.inner
        .put(f.base() + REFERENCES + 0x18, &1u32.to_le_bytes());
    assert!(
        !stable
            .observe(&capture(&f).unwrap(), Action::Prepare, 400)
            .unwrap()
    );
    f.inner
        .put(f.base() + REFERENCES + 0x18, &0u32.to_le_bytes());
    assert!(
        !stable
            .observe(&capture(&f).unwrap(), Action::Prepare, 500)
            .unwrap()
    );
    assert!(!stable.observe(&s, Action::Restore, 600).unwrap());
}
#[test]
fn ball_v2_foreign_normal_or_source_bytes_and_loaded_state_block_old_plans() {
    let mut f = fixture();
    let p = prepare(&f, &capture(&f).unwrap(), chosen(), &manifest()).unwrap();
    f.inner.put(p.source.address + 8, &[1]);
    assert!(p.validate_before(&f).is_err());
    let mut f = fixture();
    let p = prepare(&f, &capture(&f).unwrap(), chosen(), &manifest()).unwrap();
    apply(&mut f, &p.edit);
    f.inner.put(RECORDS + 8, &[1]);
    assert!(p.restore(&f).is_err());
    let mut f = fixture();
    let p = prepare(&f, &capture(&f).unwrap(), chosen(), &manifest()).unwrap();
    f.inner
        .put(f.base() + REFERENCES + 0x18, &1u32.to_le_bytes());
    assert!(p.validate_before(&f).is_err());
}
#[test]
#[ignore = "explicit pinned Version 2 ball disk audit; no game process access or file changes"]
fn ball_v2_actual_installed_disk_profile() {
    use sha2::{Digest, Sha256};
    use std::io::{Read, Seek, SeekFrom};
    let path = std::env::var_os("C27_RULES_V2_DISK").expect("Explicit game EXE path required");
    let mut f = std::fs::File::open(path).unwrap();
    let mut h = Sha256::new();
    let mut b = vec![0; 1024 * 1024];
    loop {
        let n = f.read(&mut b).unwrap();
        if n == 0 {
            break;
        }
        h.update(&b[..n]);
    }
    assert_eq!(format!("{:X}", h.finalize()), SHA);
    struct Disk(std::cell::RefCell<std::fs::File>);
    impl Memory for Disk {
        fn base(&self) -> u64 {
            0x140000000
        }
        fn read(&self, address: u64, len: usize) -> Result<Vec<u8>> {
            let rva = address - self.base();
            let offset = if rva == 0 {
                0
            } else if rva >= 0x898C000 {
                144222720 + rva - 0x898C000
            } else if rva >= 97464320 {
                97460224 + rva - 97464320
            } else {
                1536 + rva - 4096
            };
            let mut f = self.0.borrow_mut();
            f.seek(SeekFrom::Start(offset))?;
            let mut b = vec![0; len];
            f.read_exact(&mut b)?;
            Ok(b)
        }
    }
    verify_profile(&Disk(std::cell::RefCell::new(f))).unwrap();
    println!(
        "Verified Version2 normal-ball registry, holder, stride, filename, native model normalization, renderer and native reference-count code; disk only, no process access or game changes"
    );
}
