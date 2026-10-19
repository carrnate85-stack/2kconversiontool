use super::*;
use std::{cell::Cell, collections::BTreeMap};
pub(crate) struct Fake {
    base: u64,
    bytes: BTreeMap<u64, u8>,
    owner_reads: Cell<usize>,
    field_reads: Cell<usize>,
    torn_owner: bool,
    torn_field: bool,
    partial: Option<u64>,
    reads: Cell<usize>,
}
impl Memory for Fake {
    fn base(&self) -> u64 {
        self.base
    }
    fn read(&self, address: u64, len: usize) -> Result<Vec<u8>> {
        self.reads.set(self.reads.get() + 1);
        let mut bytes: Vec<u8> = (0..len)
            .map(|i| {
                self.bytes
                    .get(&(address + i as u64))
                    .copied()
                    .context("Missing fixture address")
            })
            .collect::<Result<_>>()?;
        if address == self.base + OWNER {
            let n = self.owner_reads.get();
            self.owner_reads.set(n + 1);
            if self.torn_owner && n > 0 {
                bytes[0] ^= 8;
            }
        }
        if address == 0x30000000000 + 0x816 {
            let n = self.field_reads.get();
            self.field_reads.set(n + 1);
            if self.torn_field && n > 0 {
                bytes[0] ^= 1;
            }
        }
        if self.partial == Some(address) {
            bytes.pop();
        }
        Ok(bytes)
    }
}
impl Fake {
    pub(crate) fn put(&mut self, address: u64, bytes: &[u8]) {
        for (i, b) in bytes.iter().enumerate() {
            self.bytes.insert(address + i as u64, *b);
        }
    }
    pub(crate) fn d(&mut self, offset: u64, value: u32) {
        self.put(self.base + offset, &value.to_le_bytes());
    }
    pub(crate) fn q(&mut self, offset: u64, value: u64) {
        self.put(self.base + offset, &value.to_le_bytes());
    }
}
pub(crate) fn fixture() -> Fake {
    let base = 0x180000000;
    let mut f = Fake {
        base,
        bytes: BTreeMap::new(),
        owner_reads: Cell::new(0),
        field_reads: Cell::new(0),
        torn_owner: false,
        torn_field: false,
        partial: None,
        reads: Cell::new(0),
    };
    let mut h = vec![0; 4096];
    h[..2].copy_from_slice(b"MZ");
    h[60..64].copy_from_slice(&0x80u32.to_le_bytes());
    h[0x80..0x84].copy_from_slice(b"PE\0\0");
    h[0x84..0x86].copy_from_slice(&0x8664u16.to_le_bytes());
    h[0x98..0x9A].copy_from_slice(&0x20Bu16.to_le_bytes());
    h[0x88..0x8C].copy_from_slice(&0x6AC53765u32.to_le_bytes());
    h[0xD0..0xD4].copy_from_slice(&(IMAGE_BYTES as u32).to_le_bytes());
    f.put(base, &h);
    for getter in catalog().unwrap()["fields"].as_array().unwrap() {
        f.put(
            base + getter["rva"].as_u64().unwrap(),
            &hex(getter["hex"].as_str().unwrap()).unwrap(),
        );
    }
    for &(rva, prefix, target) in RIP_ANCHORS {
        let mut code = prefix.to_vec();
        code.extend_from_slice(
            &((target as i64 - rva as i64 - prefix.len() as i64 - 4) as i32).to_le_bytes(),
        );
        f.put(base + rva, &code);
    }
    for &(rva, bytes) in FIELD_ANCHORS {
        f.put(base + rva, bytes);
    }
    f.put(
        base + ROOT_TAG,
        &[base + 0x23120, base + 0x346B350]
            .into_iter()
            .flat_map(u64::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    f.d(CURRENT, 1);
    f.d(ALTERNATE, 1);
    f.d(MODES + 64, 0x92DF06DE);
    f.d(ROOT + 0xEA8, 3);
    f.put(base + ACTIVE, &[0]);
    f.q(ROOT, base + ROOT_TAG);
    f.q(OWNER, 0x30000000000);
    f.put(0x30000000000, &vec![0; 0xA9C]);
    f.put(0x30000000000 + 0xA04, &6u32.to_le_bytes());
    f
}
#[test]
fn rules_v2_every_original_mapping_has_an_independent_native_getter() {
    let mut f = fixture();
    verify_profile(&f).unwrap();
    let table = catalog().unwrap();
    for getter in table["fields"].as_array().unwrap() {
        let address = f.base + getter["rva"].as_u64().unwrap();
        let original = f.bytes[&address];
        f.put(address, &[original ^ 1]);
        assert!(
            verify_profile(&f).is_err(),
            "Accepted changed getter {}",
            getter["id"]
        );
        f.put(address, &[original]);
    }
}
#[test]
fn rules_v2_old_or_unknown_builds_stop_before_any_offset_read() {
    let mut f = fixture();
    f.d(0x88, 0x6AA2013D);
    f.reads.set(0);
    assert!(verify_profile(&f).is_err());
    assert_eq!(f.reads.get(), 1);
    f.d(0x88, 0x6AC53765);
    f.d(0xD0, 0x36558000);
    f.reads.set(0);
    assert!(verify_profile(&f).is_err());
    assert_eq!(f.reads.get(), 1);
}
#[test]
fn rules_v2_all_39_rules_keep_original_values_until_an_explicit_supported_change() {
    let mut f = fixture();
    let snapshot = capture(&f).unwrap();
    assert_eq!(snapshot.values, vec![0; 39]);
    assert_eq!(boundary(&snapshot.context, f.base), "selection");
    for field in FIELDS {
        for value in 1..=6 {
            if !field.valid(value) {
                continue;
            }
            let change = plan(&snapshot, field.id, value).unwrap();
            assert_eq!(change.at, snapshot.context.owner + field.offset);
            assert_eq!(change.after, value.to_le_bytes()[..field.width]);
            assert_eq!(change.before, vec![0; field.width]);
            verify_expected(&f, &snapshot).unwrap();
            f.put(change.at, &change.after);
            verify_expected(&f, &change.expected).unwrap();
            let actual = capture(&f).unwrap();
            assert_eq!(actual, change.expected);
            assert_eq!(actual.values.iter().filter(|v| **v != 0).count(), 1);
            f.put(change.at, &change.before);
        }
    }
}
#[test]
fn rules_v2_numeric_choices_unknown_values_noops_and_neighbors_are_guarded() {
    let mut f = fixture();
    let snapshot = capture(&f).unwrap();
    for (field, value) in [
        ("ThreePointShotRestriction", 3),
        ("OffensiveViolationInKey", 3),
        ("DefensiveViolationInKey", 3),
        ("FoulOut", 7),
        ("ShotClock", 2),
        ("ShotClock", 0),
        ("unknown", 1),
    ] {
        assert!(plan(&snapshot, field, value).is_err());
    }
    for field in [
        "ThreePointShotRestriction",
        "OffensiveViolationInKey",
        "DefensiveViolationInKey",
    ] {
        for value in 1..=2 {
            assert!(plan(&snapshot, field, value).is_ok());
        }
    }
    for value in 1..=6 {
        assert!(plan(&snapshot, "FoulOut", value).is_ok());
    }
    let change = plan(&snapshot, "ShotClock", 1).unwrap();
    f.put(change.at, &change.after);
    f.put(snapshot.context.owner + 0x824, &[1]);
    assert!(verify_expected(&f, &change.expected).is_err());
}
#[test]
fn rules_v2_selection_match_transition_freestyle_and_menu_are_not_confused() {
    let mut f = fixture();
    assert!(capture(&f).is_ok());
    for mode in [0xB4D37B22, 0x3EEF98B9, 0x7C602DF8, 0x50BAD898] {
        f.d(MODES + 64, mode);
        f.d(ROOT + 0xEA8, 1);
        let snapshot = capture(&f).unwrap();
        assert_eq!(boundary(&snapshot.context, f.base), "match");
    }
    f.put(0x30000000000 + 0xA04, &5u32.to_le_bytes());
    assert!(capture(&f).is_err());
    f.put(0x30000000000 + 0xA04, &6u32.to_le_bytes());
    for mode in [0x6509837B, 0x079964C0, 0xF54D5D12, 0] {
        f.d(MODES + 64, mode);
        assert!(capture(&f).is_err());
    }
    f.d(MODES + 64, 0x92DF06DE);
    f.d(ROOT + 0xEA8, 3);
    f.d(ALTERNATE, 2);
    f.d(MODES + 128, 0x6509837B);
    assert!(capture(&f).is_err());
}
#[test]
fn rules_v2_torn_owner_field_group_and_partial_reads_are_never_leased() {
    let mut f = fixture();
    f.torn_owner = true;
    assert!(capture(&f).is_err());
    let mut f = fixture();
    f.torn_field = true;
    assert!(capture(&f).is_err());
    let mut f = fixture();
    f.partial = Some(f.base + OWNER);
    assert!(capture(&f).is_err());
    let mut f = fixture();
    f.q(OWNER, f.base + 0x10000);
    assert!(capture(&f).is_err());
    for owner in [0, 0xFFFF, 0x30000000001, u64::MAX] {
        let mut f = fixture();
        f.q(OWNER, owner);
        assert!(capture(&f).is_err());
    }
}
#[test]
#[ignore = "explicit installed Version 2 disk audit; no game process access or file changes"]
fn rules_v2_actual_installed_disk_getters_and_context_profile() {
    use sha2::{Digest, Sha256};
    use std::io::{Read, Seek, SeekFrom};
    let path = std::env::var_os("C27_RULES_V2_DISK").expect("Explicit game EXE path required");
    let mut file = std::fs::File::open(path).unwrap();
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let n = file.read(&mut buffer).unwrap();
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    assert_eq!(format!("{:X}", hasher.finalize()), SHA);
    struct Disk(std::cell::RefCell<std::fs::File>);
    impl Memory for Disk {
        fn base(&self) -> u64 {
            0x140000000
        }
        fn read(&self, address: u64, len: usize) -> Result<Vec<u8>> {
            let rva = address - self.base();
            let position = if rva == 0 {
                0
            } else if rva >= 0x898C000 {
                144222720 + rva - 0x898C000
            } else if rva >= 97464320 {
                97460224 + rva - 97464320
            } else {
                1536 + rva - 4096
            };
            let mut f = self.0.borrow_mut();
            f.seek(SeekFrom::Start(position))?;
            let mut bytes = vec![0; len];
            f.read_exact(&mut bytes)?;
            Ok(bytes)
        }
    }
    verify_profile(&Disk(std::cell::RefCell::new(file))).unwrap();
    println!(
        "Verified all39 independent Version2 Game Rules getters, mode/owner/context anchors and full game SHA; no process access or game file changes"
    );
}
#[test]
fn rules_v2_boundary_accepts_observed_version_two_main_stage_zero_only_when_inactive() {
    let base = 0x7FF6_0000_0000;
    let menu = |current: u32, stage: u32, active: u8, kind: u8| ContextState {
        current_index: 2,
        alternate_index: 2,
        current,
        alternate: current,
        stage,
        active,
        owner: 0x3000_0000_000,
        kind,
        game_mode: 4,
        root_tag: 0,
    };
    // Live Version 2 Main after Freestyle: stage 0, stale gameMode 4.
    assert_eq!(boundary(&menu(0x6509837B, 0, 0, 0), base), "main-menu");
    assert_eq!(boundary(&menu(0x6509837B, 1, 0, 0), base), "main-menu");
    assert_eq!(boundary(&menu(0x079964C0, 1, 0, 0), base), "quick-play");
    for unsafe_menu in [
        menu(0x6509837B, 2, 0, 0),
        menu(0x6509837B, 0, 1, 0),
        menu(0x6509837B, 0, 0, 1),
        menu(0x079964C0, 0, 0, 0),
    ] {
        assert_ne!(boundary(&unsafe_menu, base), "main-menu");
        assert_ne!(boundary(&unsafe_menu, base), "quick-play");
    }
    let mut moving = menu(0x6509837B, 0, 0, 0);
    moving.alternate = 0x92DF06DE;
    assert_eq!(boundary(&moving, base), "transition");
}
#[test]
fn rules_v2_context_stops_after_two_identical_samples() {
    let f = fixture();
    f.reads.set(0);
    let once = context_once(&f).unwrap();
    let per_sample = f.reads.get();
    f.reads.set(0);
    assert_eq!(context(&f).unwrap(), once);
    assert_eq!(f.reads.get(), 2 * per_sample);
}

// Populate code padding for real contiguous page-span reads; padding itself is
// not authoritative, while every complete catalog getter remains exact.
fn dense_getter_fixture() -> Fake {
    let mut f = fixture();
    for getter in catalog().unwrap()["fields"].as_array().unwrap() {
        let page = getter["rva"].as_u64().unwrap() & !4095;
        for i in 0..4096 {
            f.bytes.entry(f.base + page + i).or_insert(0);
        }
    }
    f
}
#[test]
fn rules_v2_grouped_getters_read_six_fresh_spans_and_reject_late_mutation() {
    let mut f = dense_getter_fixture();
    verify_profile(&f).unwrap();
    // header + six getter spans + six RIP anchors + three field anchors + class
    assert_eq!(f.reads.get(), 17);
    let getters = catalog().unwrap()["fields"].as_array().unwrap();
    for getter in [getters.last().unwrap(), &getters[getters.len() / 2]] {
        let at = f.base + getter["rva"].as_u64().unwrap();
        let expected = hex(getter["hex"].as_str().unwrap()).unwrap();
        let end = at + expected.len() as u64 - 1;
        f.put(end, &[expected[expected.len() - 1] ^ 1]);
        assert!(
            verify_profile(&f).is_err(),
            "accepted changed complete getter"
        );
        f.put(end, &expected[expected.len() - 1..]);
    }
    // A successful earlier invocation grants no authority to a later call.
    let at = f.base + getters[0]["rva"].as_u64().unwrap();
    f.put(at, &[0x90]);
    assert!(verify_profile(&f).is_err());
}
#[test]
fn rules_v2_group_read_partial_and_error_retry_all_scalar_getters() {
    struct Fault {
        inner: Fake,
        partial: bool,
        faults: Cell<usize>,
    }
    impl Memory for Fault {
        fn base(&self) -> u64 {
            self.inner.base
        }
        fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
            // Second grouped span has padding; no scalar getter has this size.
            if at == self.inner.base + 0x25A8630 && len == 2367 {
                self.faults.set(self.faults.get() + 1);
                if !self.partial {
                    anyhow::bail!("injected grouped read error");
                }
                let mut bytes = self.inner.read(at, len)?;
                bytes.pop();
                return Ok(bytes);
            }
            self.inner.read(at, len)
        }
    }
    for partial in [false, true] {
        let mut f = Fault {
            inner: dense_getter_fixture(),
            partial,
            faults: Cell::new(0),
        };
        verify_profile(&f).unwrap();
        assert_eq!(f.faults.get(), 1);
        // The last scalar getter must still be checked after grouped failure.
        let getter = catalog().unwrap()["fields"]
            .as_array()
            .unwrap()
            .last()
            .unwrap();
        let at = f.inner.base + getter["rva"].as_u64().unwrap();
        f.inner.put(at, &[0x90]);
        assert!(verify_profile(&f).is_err());
    }
}

#[test]
fn rules_v2_observed_group_mismatch_cannot_be_erased_by_later_transport_error() {
    struct RecoveringCode {
        inner: Fake,
        reads: Cell<usize>,
        late_error: Cell<bool>,
    }
    impl Memory for RecoveringCode {
        fn base(&self) -> u64 {
            self.inner.base
        }
        fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
            self.reads.set(self.reads.get() + 1);
            if at == self.inner.base + 0x25A8630 && len == 2367 {
                self.late_error.set(true);
                anyhow::bail!("later group transport failure; scalar code would now be good");
            }
            let mut bytes = self.inner.read(at, len)?;
            if at == self.inner.base + 0x25A7510 && self.reads.get() == 2 {
                // First complete group is temporarily bad; any scalar retry
                // would return the pristine fixture and hide this observation.
                bytes[0] ^= 1;
            }
            Ok(bytes)
        }
    }
    let f = RecoveringCode {
        inner: dense_getter_fixture(),
        reads: Cell::new(0),
        late_error: Cell::new(false),
    };
    assert!(verify_profile(&f).is_err());
    assert_eq!(
        f.reads.get(),
        2,
        "continued after complete mismatching code"
    );
    assert!(
        !f.late_error.get(),
        "attempted later transport/fallback after mismatch"
    );
}
