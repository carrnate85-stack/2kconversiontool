use super::*;
use crate::blacktop_arena::Region;
const WORLD: u64 = 0x50000010000;
const SYSTEMS: u64 = 0x50000020000;
const MANAGER: u64 = 0x50000030000;
const OWNER: u64 = 0x30000000000;
const RECORDS: u64 = 0x60000000000;
pub(crate) struct Fake {
    inner: crate::ball_v2::tests::Fake,
    protection: u32,
    allocation_delta: u64,
}
impl Memory for Fake {
    fn base(&self) -> u64 {
        self.inner.base()
    }
    fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
        self.inner.read(a, n)
    }
}
impl Inspect for Fake {
    fn region(&self, a: u64) -> Result<Region> {
        if (RECORDS..RECORDS + 500 * STRIDE).contains(&a) {
            // The court-record route publishes here; owner faults apply too.
            let mut r = self.inner.region(a)?;
            if self.protection != 4 {
                r.protection = self.protection;
            }
            r.allocation += self.allocation_delta;
            return Ok(r);
        }
        let start = if (OWNER..OWNER + 0x10000).contains(&a) {
            OWNER
        } else if (WORLD..WORLD + 0x30000).contains(&a) {
            a & !0xffff
        } else {
            anyhow::bail!("Unknown gameplay fixture owner")
        };
        Ok(Region {
            start,
            len: 0x10000,
            allocation: start + self.allocation_delta,
            kind: 0x20000,
            protection: self.protection,
            committed: true,
        })
    }
}
impl Fake {
    pub(crate) fn patch(&mut self, a: u64, b: &[u8]) {
        self.inner.patch(a, b);
    }
    fn q(&mut self, a: u64, v: u64) {
        self.patch(a, &v.to_le_bytes());
    }
    fn d(&mut self, a: u64, v: u32) {
        self.patch(a, &v.to_le_bytes());
    }
}
pub(crate) fn chosen() -> String {
    ball::choices()
        .iter()
        .find(|c| !ball::current(&c.id))
        .unwrap()
        .id
        .clone()
}
pub(crate) fn manifest() -> Vec<u8> {
    format!("{},0M,2967726,252485\n", chosen()).into_bytes()
}
#[test]
fn stock_source_manifest_requires_one_exact_bounded_row() {
    let id = chosen();
    let valid = format!("{id},0M,2967726,252485\n");
    validate_manifest(valid.as_bytes(), &id).unwrap();
    assert!(validate_manifest(format!("{valid}{valid}").as_bytes(), &id).is_err());
    assert!(validate_manifest(format!("{id},../0M,10,20\n").as_bytes(), &id).is_err());
    assert!(
        validate_manifest(format!("{id},0M,18446744073709551615,20\n").as_bytes(), &id).is_err()
    );
    assert!(validate_manifest(b"other.iff,0M,10,20\n", &id).is_err());
}
pub(crate) fn fixture() -> Fake {
    let mut f = Fake {
        inner: crate::ball_v2::tests::fixture(),
        protection: 4,
        allocation_delta: 0,
    };
    let base = f.base();
    for &(at, b) in ANCHORS {
        f.patch(base + at, b);
    }
    f.patch(
        base + 0x7C81750,
        &[
            96u64.to_le_bytes(),
            120u64.to_le_bytes(),
            96u64.to_le_bytes(),
        ]
        .concat(),
    );
    for (vt, col, ty, name) in [
        (WORLD_VT, 0x7E5E350, 0x9E01660, ".?AVAI_WORLD_FULL@@"),
        (
            MANAGER_VT,
            0x7E14B10,
            0x9DF9C10,
            ".?AVAI_BALL_MANAGER_BASKETBALL@@",
        ),
        (0x7C824D8, 0x7C87350, 0x9DD5210, ".?AVAI_BALL@@"),
    ] {
        f.q(base + vt - 8, base + col);
        f.patch(
            base + col,
            &[1u32, 0, 0, ty as u32, 0, col as u32]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        let mut name = name.as_bytes().to_vec();
        name.push(0);
        f.patch(base + ty + 16, &name);
    }
    f.q(base + WORLD_ROOT, WORLD);
    f.q(WORLD, base + WORLD_VT);
    f.q(WORLD + 0x348, SYSTEMS);
    f.q(SYSTEMS + 0x90, MANAGER);
    f.q(MANAGER, base + MANAGER_VT);
    f.q(MANAGER + 0x18, WORLD);
    f.patch(
        MANAGER + 0x60,
        &[0u64, MANAGER + 0x60, MANAGER + 0x60]
            .into_iter()
            .flat_map(u64::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    let model = 0x42A7A5EFu32;
    f.d(0x70000000000 + 16, model);
    let mut row = vec![0; 416];
    row[4..8].copy_from_slice(&model.to_le_bytes());
    let name = b"ball_gameplay_official_DEFAULT.iff";
    row[156..156 + name.len()].copy_from_slice(name);
    f.patch(RECORDS + 2 * STRIDE, &row);
    f.d(OWNER + MODEL, model);
    for index in 1..=2 {
        f.d(base + ball_v2::REFERENCES + 0x18 + index * 4, 0);
        f.patch(base + ball_v2::RENDERER + 0x28 + index * 48, &[0; 48]);
    }
    f
}
fn plan(f: &Fake) -> Plan {
    let mut s = Samples::new();
    for t in [0, 100, 200] {
        assert!(s.push(capture(f, &chosen()).unwrap(), t).unwrap().is_none());
    }
    s.push(capture(f, &chosen()).unwrap(), 300)
        .unwrap()
        .unwrap()
}
#[test]
fn no_forward_permit_without_unique_stock_source() {
    let f = fixture();
    let p = plan(&f);
    assert!(p.forward(&f, b"other.iff,0M,1,2\n").is_err());
    assert!(p.forward(&f, &[manifest(), manifest()].concat()).is_err());
    p.forward(&f, &manifest()).unwrap();
}
fn restore<'a>(p: &'a Plan, f: &Fake) -> Result<Permit<'a>> {
    let mut samples = RestoreSamples::new();
    for now in [0, 100, 200] {
        ensure!(
            p.restore_stable(f, &mut samples, now)?.is_none(),
            "Early restore permit"
        );
    }
    p.restore_stable(f, &mut samples, 300)?
        .context("Missing stable restore permit")
}
/// Simulates the one publication the permit authorizes.
fn publish(f: &mut Fake, p: &Plan) {
    let after = p.after.clone();
    f.patch(p.address(), &after);
}
#[test]
fn four_spaced_samples_and_only_the_court_record_filename_field() {
    let mut f = fixture();
    let p = plan(&f);
    let court = RECORDS + 2 * STRIDE;
    assert_eq!(p.address(), court + ball::FIELD as u64);
    let normal = read(&f, RECORDS, 416).unwrap();
    let original = read(&f, court, 416).unwrap();
    let chosen_record = read(&f, RECORDS + STRIDE, 416).unwrap();
    let name = ball::filename(&chosen_record).unwrap();
    let permit = p.forward(&f, &manifest()).unwrap();
    let c = permit.change().unwrap();
    assert_eq!(c.before.len(), COURT_DEFAULT_FILE.len().max(name.len()) + 1);
    assert_eq!(c.after.len(), c.before.len());
    assert!(c.before.starts_with(COURT_DEFAULT_FILE.as_bytes()));
    assert_eq!(&c.after[..name.len()], name.as_bytes());
    assert!(c.after[name.len()..].iter().all(|b| *b == 0));
    f.patch(c.address, &c.after);
    permit.verify_after(&f).unwrap();
    let swapped = read(&f, court, 416).unwrap();
    assert_eq!(ball::filename(&swapped).unwrap(), name);
    assert_eq!(swapped[..ball::FIELD], original[..ball::FIELD]);
    assert_eq!(read(&f, RECORDS, 416).unwrap(), normal);
    assert_eq!(read(&f, RECORDS + STRIDE, 416).unwrap(), chosen_record);
    // The resettable native setting is never written.
    assert_eq!(d(&f, OWNER + MODEL).unwrap(), 0x42A7A5EF);
    drop(permit);
    let restore = restore(&p, &f).unwrap();
    f.patch(restore.address(), restore.after());
    restore.verify_after(&f).unwrap();
    assert_eq!(read(&f, court, 416).unwrap(), original);
}
#[test]
fn repeated_unspaced_samples_do_not_authorize() {
    let f = fixture();
    let s = capture(&f, &chosen()).unwrap();
    let mut samples = Samples::new();
    for t in [0, 1, 20, 30, 99] {
        assert!(samples.push(s.clone(), t).unwrap().is_none());
    }
    for t in [100, 200] {
        assert!(samples.push(s.clone(), t).unwrap().is_none());
    }
    assert!(samples.push(s, 300).unwrap().is_some());
}
#[test]
fn every_new_disk_anchor_and_typed_rtti_are_required() {
    let mut f = fixture();
    verify_profile(&f).unwrap();
    for &(at, b) in ANCHORS {
        f.patch(f.base() + at, &[b[0] ^ 1]);
        assert!(verify_profile(&f).is_err());
        f.patch(f.base() + at, b);
    }
    for at in [WORLD_VT - 8, MANAGER_VT - 8, 0x7E14B10 + 12, 0x9DF9C10 + 16] {
        let old = read(&f, f.base() + at, 1).unwrap();
        f.patch(f.base() + at, &[old[0] ^ 1]);
        assert!(verify_profile(&f).is_err());
        f.patch(f.base() + at, &old);
    }
}
#[test]
fn court_resource_bytes_or_refs_block_prepare_but_a_loaded_choice_does_not() {
    for at in 0..48 {
        let mut f = fixture();
        let mut b = [0; 48];
        b[at] = 1;
        f.patch(f.base() + ball_v2::RENDERER + 0x28 + 2 * 48, &b);
        assert!(!capture(&f, &chosen()).unwrap().ready());
    }
    for count in [1u32, u32::MAX] {
        let mut f = fixture();
        f.d(f.base() + ball_v2::REFERENCES + 0x18 + 2 * 4, count);
        assert!(!capture(&f, &chosen()).unwrap().ready());
    }
    // WPF swapped the court filename regardless of the chosen record load state.
    let mut f = fixture();
    f.d(f.base() + ball_v2::REFERENCES + 0x18 + 4, 1);
    assert!(capture(&f, &chosen()).unwrap().ready());
}
#[test]
fn selection_world_state_is_irrelevant_but_the_court_record_must_be_private_rw() {
    // V2 selection may hold a lingering world with spectator balls or none at
    // all; the court record unload state is the gate, as in WPF.
    for fault in 0..5 {
        let mut f = fixture();
        match fault {
            0 => f.q(f.base() + WORLD_ROOT, 0),
            1 => f.q(WORLD, f.base() + WORLD_VT + 8),
            2 => f.q(MANAGER, f.base() + MANAGER_VT + 8),
            3 => f.q(MANAGER + 0x18, WORLD + 16),
            _ => f.q(MANAGER + 0x70, MANAGER + 0x200),
        };
        assert!(capture(&f, &chosen()).unwrap().ready(), "world fault {fault}");
    }
    let mut f = fixture();
    f.protection = 0x40;
    assert!(capture(&f, &chosen()).is_err());
}
#[test]
fn prepublication_change_of_owner_context_or_field_invalidates_permit() {
    for fault in 0..5 {
        let mut f = fixture();
        let p = plan(&f);
        match fault {
            0 => f.patch(RECORDS + 2 * STRIDE + ball::FIELD as u64, b"ball_x.iff\0"),
            1 => f.d(f.base() + 0xAF46870 + 0xEA8, 1),
            2 => f.d(f.base() + ball_v2::REFERENCES + 0x18 + 2 * 4, 1),
            3 => f.protection = 2,
            _ => f.allocation_delta = 0x1000,
        };
        assert!(
            p.forward(&f, &manifest()).is_err(),
            "Accepted stale forward {fault}"
        );
    }
}
#[test]
fn restore_needs_exact_owned_value_and_release_not_merely_main_menu() {
    for fault in 0..4 {
        let mut f = fixture();
        let p = plan(&f);
        publish(&mut f, &p);
        match fault {
            0 => f.patch(RECORDS + 2 * STRIDE + ball::FIELD as u64, b"ball_x.iff\0"),
            1 => f.d(f.base() + ball_v2::REFERENCES + 0x18 + 2 * 4, 1),
            2 => f.patch(RECORDS + 2 * STRIDE + 0x40, &[7]),
            _ => f.protection = 0x40,
        };
        assert!(restore(&p, &f).is_err());
    }
}
#[test]
fn court_record_follows_the_games_own_setting_like_native_normalization() {
    // 42A7A5EF loads its own record; the default alias loads NBA Official
    // (CB5691BE), as WPF relied on; any other stock choice is refused.
    assert_eq!(court_model(0x42A7A5EF).unwrap(), COURT_DEFAULT);
    assert_eq!(court_model(ball::DEFAULT).unwrap(), ball::NORMAL);
    assert_eq!(court_model(0).unwrap(), ball::NORMAL);
    assert_eq!(court_model(ball::NORMAL).unwrap(), ball::NORMAL);
    assert!(court_model(0xD1C5F442).is_err());
    let f = fixture();
    assert_eq!(capture(&f, &chosen()).unwrap().original.model, COURT_DEFAULT);
    let mut f = fixture();
    f.d(OWNER + MODEL, 0xD1C5F442);
    assert!(capture(&f, &chosen()).is_err());
}
#[test]
fn default_alias_setting_swaps_the_nba_official_record_like_wpf() {
    let mut f = fixture();
    f.d(OWNER + MODEL, ball::DEFAULT);
    let normal = RECORDS;
    let original = read(&f, normal, 416).unwrap();
    assert_eq!(ball::filename(&original).unwrap(), ball::NORMAL_FILE);
    let p = plan(&f);
    assert_eq!(p.court(), ball::NORMAL);
    assert_eq!(p.address(), normal + ball::FIELD as u64);
    let permit = p.forward(&f, &manifest()).unwrap();
    let c = permit.change().unwrap();
    assert!(c.before.starts_with(ball::NORMAL_FILE.as_bytes()));
    f.patch(c.address, &c.after);
    permit.verify_after(&f).unwrap();
    drop(permit);
    // The game changing its court-ball setting invalidates a pending forward.
    let mut g = fixture();
    g.d(OWNER + MODEL, ball::DEFAULT);
    let q = plan(&g);
    g.d(OWNER + MODEL, 0x42A7A5EF);
    assert!(q.forward(&g, &manifest()).is_err());
    let restore = restore(&p, &f).unwrap();
    f.patch(restore.address(), restore.after());
    restore.verify_after(&f).unwrap();
    assert_eq!(read(&f, normal, 416).unwrap(), original);
}
#[test]
fn prepared_receipt_reports_no_visual_or_gameplay_claim() {
    let f = fixture();
    let p = plan(&f);
    let value = p.receipt();
    assert_eq!(value["at"], RECORDS + 2 * STRIDE + ball::FIELD as u64);
    assert_eq!(value["courtModel"], COURT_DEFAULT);
    assert_eq!(value["liveAcceptanceRequired"], true);
    assert!(value.get("visualAppearanceProven").is_none());
}
#[test]
fn typed_simulation_model_confirms_court_ball_not_prop_resources() {
    let mut f = fixture();
    let node = MANAGER + 0x200;
    let object = MANAGER + 0x3000;
    f.patch(
        MANAGER + 0x60,
        &[0u64, node, node]
            .into_iter()
            .flat_map(u64::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    f.patch(
        node,
        &[object, MANAGER + 0x60, MANAGER + 0x60]
            .into_iter()
            .flat_map(u64::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    f.q(object, f.base() + 0x7C824D8);
    f.q(object + 0x5E0, WORLD);
    f.d(object + 0x34C, 0x42A7A5EF);
    f.d(f.base() + 0xAD07408 + 64, 0x3EEF98B9);
    f.d(f.base() + 0xAF46870 + 0xEA8, 1);
    assert_eq!(
        observe_gameplay(&f).unwrap(),
        (0x42A7A5EF, vec![0x42A7A5EF])
    );
    f.d(object + 0x34C, 0x12345678);
    assert_eq!(
        observe_gameplay(&f).unwrap(),
        (0x42A7A5EF, vec![0x12345678])
    );
    // Live 0.50: the court ball carries the default alias; it is the NBA
    // Official record, not an unresolved model.
    f.d(object + 0x34C, ball::DEFAULT);
    assert_eq!(
        observe_gameplay(&f).unwrap(),
        (0x42A7A5EF, vec![ball::NORMAL])
    );
    f.q(object + 0x5E0, WORLD + 8);
    assert!(observe_gameplay(&f).is_err());
}
#[test]
fn restore_four_samples_reset_on_resource_reload() {
    let mut f = fixture();
    let p = plan(&f);
    publish(&mut f, &p);
    let mut samples = RestoreSamples::new();
    for now in [0, 100, 200] {
        assert!(p.restore_stable(&f, &mut samples, now).unwrap().is_none());
    }
    f.d(f.base() + ball_v2::REFERENCES + 0x18 + 2 * 4, 1);
    assert!(p.restore_stable(&f, &mut samples, 300).is_err());
    f.d(f.base() + ball_v2::REFERENCES + 0x18 + 2 * 4, 0);
    for now in [400, 500, 600] {
        assert!(p.restore_stable(&f, &mut samples, now).unwrap().is_none());
    }
    assert!(p.restore_stable(&f, &mut samples, 700).unwrap().is_some());
}
#[test]
fn second_restore_capture_error_clears_previous_samples() {
    use std::cell::Cell;
    struct Fault<'a> {
        inner: &'a Fake,
        reads: Cell<usize>,
        fail: Cell<usize>,
    }
    impl Memory for Fault<'_> {
        fn base(&self) -> u64 {
            self.inner.base()
        }
        fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
            let count = self.reads.get() + 1;
            self.reads.set(count);
            ensure!(count != self.fail.get(), "Injected second capture failure");
            self.inner.read(a, n)
        }
    }
    impl Inspect for Fault<'_> {
        fn region(&self, a: u64) -> Result<Region> {
            self.inner.region(a)
        }
    }
    let mut f = fixture();
    let p = plan(&f);
    publish(&mut f, &p);
    let fault = Fault {
        inner: &f,
        reads: Cell::new(0),
        fail: Cell::new(usize::MAX),
    };
    p.verify(&fault, true, false).unwrap();
    let reads = fault.reads.get();
    let mut samples = RestoreSamples::new();
    for now in [0, 100, 200] {
        assert!(p.restore_stable(&f, &mut samples, now).unwrap().is_none());
    }
    fault.reads.set(0);
    fault.fail.set(reads + 1);
    assert!(p.restore_stable(&fault, &mut samples, 300).is_err());
    assert_eq!(samples.count, 0);
    assert!(samples.state.is_none());
    for now in [400, 500, 600] {
        assert!(p.restore_stable(&f, &mut samples, now).unwrap().is_none());
    }
    assert!(p.restore_stable(&f, &mut samples, 700).unwrap().is_some());
}
#[test]
fn unsafe_forward_sample_clears_previous_stability() {
    let mut f = fixture();
    let mut samples = Samples::new();
    for now in [0, 100, 200] {
        assert!(
            samples
                .push(capture(&f, &chosen()).unwrap(), now)
                .unwrap()
                .is_none()
        );
    }
    f.d(f.base() + ball_v2::REFERENCES + 0x18 + 2 * 4, 1);
    assert!(samples.push(capture(&f, &chosen()).unwrap(), 300).is_err());
    f.d(f.base() + ball_v2::REFERENCES + 0x18 + 2 * 4, 0);
    for now in [400, 500, 600] {
        assert!(
            samples
                .push(capture(&f, &chosen()).unwrap(), now)
                .unwrap()
                .is_none()
        );
    }
    assert!(
        samples
            .push(capture(&f, &chosen()).unwrap(), 700)
            .unwrap()
            .is_some()
    );
}

#[test]
#[ignore = "Explicit installed-image disk-only audit; never opens a process"]
fn installed_v2_gameplay_ball_profile_is_independently_verified() {
    use sha2::{Digest, Sha256};
    struct Disk {
        bytes: Vec<u8>,
        sections: Vec<(u64, u64, u64)>,
    }
    impl Memory for Disk {
        fn base(&self) -> u64 {
            0x140000000
        }
        fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
            let r = a.checked_sub(self.base()).context("Disk RVA below image")?;
            if r < 4096 {
                return Ok(self.bytes[r as usize..r as usize + n].to_vec());
            }
            let (v, raw, _) = self
                .sections
                .iter()
                .find(|(v, _, size)| r >= *v && r + n as u64 <= v + size)
                .context("Disk read outside raw PE section")?;
            let at = (raw + r - v) as usize;
            Ok(self.bytes[at..at + n].to_vec())
        }
    }
    let bytes = std::fs::read(
        std::env::var_os("C27_RULES_V2_DISK").expect("Explicit game EXE disk path required"),
    )
    .unwrap();
    assert_eq!(
        format!("{:X}", Sha256::digest(&bytes)),
        "29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1"
    );
    let u16at = |at| u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap()) as usize;
    let u32at = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    let pe = u32at(60);
    let count = u16at(pe + 6);
    let table = pe + 24 + u16at(pe + 20);
    let sections = (0..count)
        .map(|i| {
            let s = table + i * 40;
            (
                u32at(s + 12) as u64,
                u32at(s + 20) as u64,
                u32at(s + 16) as u64,
            )
        })
        .collect();
    verify_profile(&Disk { bytes, sections }).unwrap();
}
#[test]
fn version_two_selection_without_a_gameplay_world_is_an_empty_world() {
    // Live 0.48 probe: the gameplay world root is null at Blacktop selection.
    let mut f = fixture();
    f.q(f.base() + WORLD_ROOT, 0);
    let s = capture(&f, &chosen()).unwrap();
    assert!(s.ready());
    // Match-time confirmation still needs a real typed world.
    assert!(observe_gameplay(&f).is_err());
}
/// Counts the native operations a capture/plan makes, using the exact cost
/// model of ball_gameplay_native::Native (read = checkpoint + VirtualQueryEx
/// + ReadProcessMemory + checkpoint; region = checkpoint + VirtualQueryEx).
/// 0.52 paid one more checkpoint per read (through `region`).
pub(crate) struct Counting<'a, M> {
    pub inner: &'a M,
    pub reads: std::cell::Cell<usize>,
    pub regions: std::cell::Cell<usize>,
    pub checkpoints: std::cell::Cell<usize>,
}
impl<'a, M> Counting<'a, M> {
    pub(crate) fn new(inner: &'a M) -> Self {
        Self {
            inner,
            reads: Default::default(),
            regions: Default::default(),
            checkpoints: Default::default(),
        }
    }
    /// (ReadProcessMemory, VirtualQueryEx, checkpoints now, checkpoints in 0.52).
    pub(crate) fn native_cost(&self) -> (usize, usize, usize, usize) {
        let (r, g, c) = (self.reads.get(), self.regions.get(), self.checkpoints.get());
        (r, r + g, c + 2 * r + g, c + 3 * r + g)
    }
}
impl<M: Memory> Memory for Counting<'_, M> {
    fn base(&self) -> u64 {
        self.inner.base()
    }
    fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
        self.reads.set(self.reads.get() + 1);
        self.inner.read(a, n)
    }
    fn checkpoint(&self) -> Result<()> {
        self.checkpoints.set(self.checkpoints.get() + 1);
        self.inner.checkpoint()
    }
}
impl<M: Inspect> Inspect for Counting<'_, M> {
    fn region(&self, a: u64) -> Result<Region> {
        self.regions.set(self.regions.get() + 1);
        self.inner.region(a)
    }
}
/// Live V2 registry size (450 records, 2026-09-26): unrelated catalog rows.
pub(crate) fn full_registry(f: &mut Fake) {
    let count = 450u32;
    let mut entries = read(f, 0x70000000000, 24).unwrap();
    for index in 3..count {
        let model = 0x11000000 + index;
        entries.extend(model.to_le_bytes());
        entries.extend(index.to_le_bytes());
        let mut row = vec![0; 416];
        row[4..8].copy_from_slice(&model.to_le_bytes());
        let name = format!("ball_filler_{index}.iff");
        row[156..156 + name.len()].copy_from_slice(name.as_bytes());
        f.patch(RECORDS + index as u64 * STRIDE, &row);
    }
    f.patch(0x70000000000, &entries);
    f.d(f.base() + ball_v2::REGISTRY + 8, count);
}
#[test]
#[ignore = "Measurement: prints native operation counts per prepare stage"]
fn measure_native_operations_per_prepare_stage() {
    let mut f = fixture();
    full_registry(&mut f);
    let c = Counting::new(&f);
    capture(&c, &chosen()).unwrap();
    println!("capture: {:?}", c.native_cost());
    let p = plan(&f);
    let c = Counting::new(&f);
    p.forward(&c, &manifest()).unwrap();
    println!("forward: {:?}", c.native_cost());
    let c = Counting::new(&f);
    p.prepared(&c).ok();
    println!("verify: {:?}", c.native_cost());
}
#[test]
fn forward_checked_requires_the_source_proof_for_the_exact_choice() {
    let f = fixture();
    let p = plan(&f);
    let seen = std::cell::RefCell::new(Vec::new());
    p.forward_checked(&f, &|s| {
        seen.borrow_mut().push(s.to_string());
        Ok(())
    })
    .unwrap();
    assert_eq!(*seen.borrow(), vec![chosen()]);
    assert!(
        p.forward_checked(&f, &|_| anyhow::bail!("absent from stock manifest"))
            .is_err()
    );
}
