use super::*;
use crate::{blacktop_arena::Region, session_safety::Identity};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
const IMAGE: u64 = 0x140000000;
const REGION: u64 = 0x220000000;
const TABLE: u64 = REGION + 0x680;
const APPEARANCE: u64 = 0x230001000;
struct Fake {
    generation: RefCell<Generation>,
    row: RefCell<Vec<u8>>,
    words: RefCell<[u8; 4]>,
    peer: Cell<u64>,
    person: Cell<u64>,
    cancel: Rc<Cell<bool>>,
    profile_checks: Cell<usize>,
    cancel_at_profile: Cell<usize>,
    mode: Cell<u8>,
    writes: Cell<usize>,
    events: Rc<RefCell<Vec<&'static str>>>,
}
impl Fake {
    fn new() -> Self {
        let mut row = vec![0; Kind::Player.stride()];
        row[0x128..0x12a].copy_from_slice(&7u16.to_le_bytes());
        row[0x78..0x80].copy_from_slice(&APPEARANCE.to_le_bytes());
        let owner = Owner {
            game: Identity {
                pid: 7,
                started: "42".into(),
                path: r"C:\QA\NBA2K27.exe".into(),
            },
            session: "a".repeat(48),
            image: IMAGE,
            root: 0x240000000,
            owner: REGION + 0x50,
            region_base: REGION,
            region_len: 0x800000,
            kind: Kind::Player,
            table: TABLE,
            count: 100,
            index: 0,
            identity: writer::identity(Kind::Player, &row).unwrap(),
            proof: writer::proof_tag().into(),
        };
        Self {
            generation: RefCell::new(
                Generation::from_native(owner, [1; 32], vec![(REGION, 0x680)]).unwrap(),
            ),
            row: RefCell::new(row),
            words: RefCell::new([0xa0, 0xce, 0x08, 0xd2]),
            peer: Cell::new(0),
            person: Cell::new(0),
            cancel: Rc::new(Cell::new(false)),
            profile_checks: Cell::new(0),
            cancel_at_profile: Cell::new(0),
            mode: Cell::new(0),
            writes: Cell::new(0),
            events: Rc::default(),
        }
    }
    fn capture(&self) -> Snapshot {
        Snapshot::capture(self, &self.generation.borrow()).unwrap()
    }
    fn journal(&self) -> J {
        J {
            proof: None,
            fail: false,
            fail_restored: false,
            cancel_on_intent: None,
            events: self.events.clone(),
        }
    }
}
impl Memory for Fake {
    fn base(&self) -> u64 {
        IMAGE
    }
    fn checkpoint(&self) -> Result<()> {
        Ok(()) // Retained-source proof, independent from new-work cancellation.
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        if at == APPEARANCE && len == 4 {
            return Ok(self.words.borrow().to_vec());
        }
        if at == TABLE + Kind::Player.stride() as u64 + 0x78 && len == 8 {
            return Ok(self.peer.get().to_le_bytes().to_vec());
        }
        if at >= TABLE && at + len as u64 <= TABLE + 100 * Kind::Player.stride() as u64 {
            let mut out = vec![0; len];
            let row = self.row.borrow();
            for (i, b) in out.iter_mut().enumerate() {
                if at + (i as u64) < TABLE + row.len() as u64 {
                    *b = row[(at - TABLE) as usize + i];
                }
            }
            return Ok(out);
        }
        anyhow::bail!("unmapped fixture read")
    }
}
impl Inspect for Fake {
    fn region(&self, at: u64) -> Result<Region> {
        ensure!(at == APPEARANCE, "Unexpected fixture region");
        Ok(Region {
            start: APPEARANCE,
            len: 4096,
            allocation: APPEARANCE,
            kind: 0x20000,
            protection: 4,
            committed: true,
        })
    }
}
impl Access for Fake {
    fn other_appearance_pointers(&self) -> Result<Vec<u64>> {
        Ok(vec![self.person.get()])
    }
    fn generation(&self) -> Result<Generation> {
        Ok(self.generation.borrow().clone())
    }
    fn retained_check(&self) -> Result<()> {
        Ok(())
    }
    fn forward_check(&self) -> Result<()> {
        ensure!(!self.cancel.get(), "new work canceled");
        Ok(())
    }
    fn verify_native_profile(&self) -> Result<()> {
        let count = self.profile_checks.get() + 1;
        self.profile_checks.set(count);
        if self.cancel_at_profile.get() == count {
            self.cancel.set(true);
        }
        Ok(())
    } // Native byte proofs independently disk-audited.
    fn compare_exchange_masked(&self, p: &Permit<'_>) -> Result<CasOutcome> {
        assert_eq!(self.events.borrow().last().copied(), Some("verified"));
        ensure!(*self.words.borrow() == p.before(), "CAS preimage changed");
        self.writes.set(self.writes.get() + 1);
        self.events.borrow_mut().push("effect");
        match self.mode.replace(0) {
            1 => Ok(CasOutcome::NoEffect),
            2 => {
                *self.words.borrow_mut() = p.after();
                Ok(CasOutcome::FailedAfterApplied(
                    p.applied_receipt(),
                    anyhow::anyhow!("failed after owned effect"),
                ))
            }
            3 => {
                self.words.borrow_mut()[0] = p.after()[0];
                Ok(CasOutcome::Uncertain)
            }
            4 => {
                // Another writer published our desired value, while this CAS
                // is known to have made no effect. Equal bytes are not ours.
                *self.words.borrow_mut() = p.after();
                Ok(CasOutcome::NoEffect)
            }
            5 => {
                *self.words.borrow_mut() = p.after();
                anyhow::bail!("unknown failure with identical foreign after-image")
            }
            _ => {
                let mut words = self.words.borrow_mut();
                for i in 0..4 {
                    words[i] = (words[i] & !p.mask()[i]) | (p.after()[i] & p.mask()[i]);
                }
                Ok(CasOutcome::Applied(p.applied_receipt()))
            }
        }
    }
}
#[test]
fn foreign_exact_after_on_conflict_or_unknown_error_never_grants_undo() {
    for mode in [4, 5] {
        let f = Fake::new();
        let snapshot = f.capture();
        let plan = Plan::new(&snapshot, Some(205.0), None).unwrap();
        let desired = plan.after;
        let mut tx = Transaction::new(snapshot, plan).unwrap();
        let mut journal = f.journal();
        f.mode.set(mode);
        assert!(tx.apply(&f, &mut journal).is_err());
        assert_eq!(tx.phase(), Phase::Held);
        assert!(tx.rollback(&f, &mut journal).is_err());
        assert_eq!(*f.words.borrow(), desired);
        assert_eq!(f.writes.get(), 1);
    }
}
#[test]
fn uncertain_undo_consumes_receipt_before_any_possible_foreign_aba() {
    let f = Fake::new();
    let snapshot = f.capture();
    let plan = Plan::new(&snapshot, Some(205.0), None).unwrap();
    let desired = plan.after;
    let mut tx = Transaction::new(snapshot, plan).unwrap();
    let mut journal = f.journal();
    tx.apply(&f, &mut journal).unwrap();
    f.mode.set(3);
    assert!(tx.rollback(&f, &mut journal).is_err());
    *f.words.borrow_mut() = desired;
    let attempted = f.writes.get();
    assert!(tx.rollback(&f, &mut journal).is_err());
    assert_eq!(f.writes.get(), attempted);
    assert_eq!(*f.words.borrow(), desired);
}
struct J {
    proof: Option<(Snapshot, Plan, Phase)>,
    fail: bool,
    fail_restored: bool,
    cancel_on_intent: Option<Rc<Cell<bool>>>,
    events: Rc<RefCell<Vec<&'static str>>>,
}
impl Journal for J {
    fn persist(&mut self, s: &Snapshot, p: &Plan, phase: Phase) -> Result<()> {
        ensure!(!self.fail, "fixture durable failure");
        ensure!(
            !(self.fail_restored && phase == Phase::Restored),
            "fixture restored journal failure"
        );
        self.proof = Some((s.clone(), p.clone(), phase));
        self.events.borrow_mut().push("persisted");
        if phase == Phase::Intent {
            if let Some(cancel) = &self.cancel_on_intent {
                cancel.set(true);
            }
        }
        Ok(())
    }
    fn verify(&self, s: &Snapshot, p: &Plan, phase: Phase) -> Result<()> {
        ensure!(
            self.proof.as_ref() == Some(&(s.clone(), p.clone(), phase)),
            "fixture journal proof differs"
        );
        self.events.borrow_mut().push("verified");
        Ok(())
    }
}
#[test]
fn units_and_flags_preserved_under_masked_publish_and_cancelled_owned_restore() {
    let f = Fake::new();
    let snapshot = f.capture();
    assert_eq!(snapshot.cm(), [201.28, 210.0]);
    let plan = Plan::new(&snapshot, Some(203.2), Some(220.0)).unwrap();
    assert_eq!(plan.mask, [255, 127, 255, 127]);
    assert_eq!(
        u16::from_le_bytes(plan.after[..2].try_into().unwrap()),
        0x8000 | 20320
    );
    let original = snapshot.words;
    let mut tx = Transaction::new(snapshot, plan).unwrap();
    let mut j = f.journal();
    tx.apply(&f, &mut j).unwrap();
    assert_eq!(tx.phase(), Phase::Applied);
    assert!(tx.apply(&f, &mut j).is_err());
    f.cancel.set(true);
    tx.rollback(&f, &mut j).unwrap();
    assert_eq!(tx.phase(), Phase::Restored);
    assert_eq!(*f.words.borrow(), original);
}
#[test]
fn duplicate_partial_alias_generation_pointer_and_owner_spans_fail_before_effects() {
    for delta in [0, 2] {
        let f = Fake::new();
        f.peer.set(APPEARANCE + delta);
        assert!(Snapshot::capture(&f, &f.generation.borrow()).is_err());
        assert_eq!(f.writes.get(), 0);
    }
    let f = Fake::new();
    f.generation
        .borrow_mut()
        .known_owner_spans
        .push((APPEARANCE + 2, 8));
    assert!(Snapshot::capture(&f, &f.generation.borrow()).is_err());
    let f = Fake::new();
    let snapshot = f.capture();
    let plan = Plan::new(&snapshot, Some(205.0), None).unwrap();
    let mut tx = Transaction::new(snapshot, plan).unwrap();
    f.generation.borrow_mut().stamp[0] ^= 1;
    assert!(tx.apply(&f, &mut f.journal()).is_err());
    assert_eq!(f.writes.get(), 0);
    let f = Fake::new();
    let snapshot = f.capture();
    let plan = Plan::new(&snapshot, Some(205.0), None).unwrap();
    let mut tx = Transaction::new(snapshot, plan).unwrap();
    f.row.borrow_mut()[0x78] ^= 2;
    assert!(tx.apply(&f, &mut f.journal()).is_err());
    assert_eq!(f.writes.get(), 0);
}
#[test]
fn failed_durability_never_writes_and_partial_foreign_effect_never_rolls_back() {
    let f = Fake::new();
    let snapshot = f.capture();
    let plan = Plan::new(&snapshot, Some(205.0), None).unwrap();
    let mut tx = Transaction::new(snapshot, plan).unwrap();
    let mut j = f.journal();
    j.fail = true;
    assert!(tx.apply(&f, &mut j).is_err());
    assert_eq!(tx.phase(), Phase::Held);
    assert_eq!(f.writes.get(), 0);
    assert!(tx.apply(&f, &mut j).is_err());
    let f = Fake::new();
    let snapshot = f.capture();
    let plan = Plan::new(&snapshot, Some(205.0), None).unwrap();
    let mut tx = Transaction::new(snapshot, plan).unwrap();
    let mut j = f.journal();
    f.mode.set(3);
    assert!(tx.apply(&f, &mut j).is_err());
    assert_eq!(tx.phase(), Phase::Held);
    assert!(tx.rollback(&f, &mut j).is_err());
    assert_eq!(f.writes.get(), 1);
}
#[test]
fn failure_after_exact_effect_rolls_back_once_but_never_retries_forward() {
    let f = Fake::new();
    let snapshot = f.capture();
    let original = snapshot.words;
    let plan = Plan::new(&snapshot, Some(205.0), None).unwrap();
    let mut tx = Transaction::new(snapshot, plan).unwrap();
    let mut j = f.journal();
    f.mode.set(2);
    assert!(tx.apply(&f, &mut j).is_err());
    assert_eq!(tx.phase(), Phase::Restored);
    assert_eq!(*f.words.borrow(), original);
    assert_eq!(f.writes.get(), 2);
    assert!(tx.apply(&f, &mut j).is_err());
    for cm in [f64::NAN, f64::INFINITY, 119.0, 261.0] {
        assert!(Plan::new(&f.capture(), Some(cm), None).is_err());
    }
}
#[test]
fn restored_journal_failure_cannot_reuse_undo_receipt_after_foreign_aba() {
    let f = Fake::new();
    let snapshot = f.capture();
    let plan = Plan::new(&snapshot, Some(205.0), None).unwrap();
    let desired = plan.after;
    let mut tx = Transaction::new(snapshot, plan).unwrap();
    let mut journal = f.journal();
    tx.apply(&f, &mut journal).unwrap();
    journal.fail_restored = true;
    assert!(tx.rollback(&f, &mut journal).is_err());
    *f.words.borrow_mut() = desired;
    journal.fail_restored = false;
    let attempted = f.writes.get();
    assert!(tx.rollback(&f, &mut journal).is_err());
    assert_eq!(f.writes.get(), attempted);
    assert_eq!(*f.words.borrow(), desired);
}

#[test]
fn uninitialized_and_implausible_masked_sources_fail_before_any_effect() {
    for words in [[0, 0, 0, 0], [0, 128, 0, 128], [0xff, 0xff, 0xff, 0xff]] {
        let f = Fake::new();
        *f.words.borrow_mut() = words;
        assert!(Snapshot::capture(&f, &f.generation.borrow()).is_err());
        assert_eq!(f.writes.get(), 0);
    }
    let f = Fake::new();
    let mut words = *f.words.borrow();
    words[2..].copy_from_slice(&11999u16.to_le_bytes());
    *f.words.borrow_mut() = words;
    assert!(Snapshot::capture(&f, &f.generation.borrow()).is_err());
}

#[test]
fn cancellation_during_intent_or_last_profile_check_never_publishes() {
    for cancel_in_journal in [true, false] {
        let f = Fake::new();
        let snapshot = f.capture();
        let original = snapshot.words;
        let plan = Plan::new(&snapshot, Some(205.0), None).unwrap();
        let mut tx = Transaction::new(snapshot, plan).unwrap();
        let mut journal = f.journal();
        if cancel_in_journal {
            journal.cancel_on_intent = Some(f.cancel.clone());
        } else {
            // apply guard then Permit::verify guard: cancel in the final
            // native profile check after the durable forward intent exists.
            f.cancel_at_profile.set(f.profile_checks.get() + 2);
        }
        assert!(tx.apply(&f, &mut journal).is_err());
        assert!(f.cancel.get());
        assert_eq!(tx.phase(), Phase::Held);
        assert_eq!(f.writes.get(), 0);
        assert_eq!(*f.words.borrow(), original);
        assert!(!f.events.borrow().contains(&"effect"));
        assert!(tx.apply(&f, &mut journal).is_err());
    }
}

#[test]
fn plans_cannot_move_between_generations_players_or_preimages() {
    let first = Fake::new();
    let original = first.capture();
    for changed_owner in [true, false] {
        let second = Fake::new();
        if changed_owner {
            second.row.borrow_mut()[0x128..0x12a].copy_from_slice(&8u16.to_le_bytes());
            let mut generation = second.generation.borrow_mut();
            generation.stamp[0] = 2;
            generation.owner.identity =
                writer::identity(Kind::Player, &second.row.borrow()).unwrap();
        } else {
            second.words.borrow_mut()[..2].copy_from_slice(&(0x8000u16 | 20320).to_le_bytes());
        }
        let plan = Plan::new(&original, Some(205.0), None).unwrap();
        assert!(Transaction::new(second.capture(), plan).is_err());
        assert_eq!(second.writes.get(), 0);
    }
}

#[test]
fn legacy_roster_evidence_cannot_mint_an_appearance_generation() {
    let m = Fake::new();
    let generation = m.generation.borrow();
    let mut owner = generation.owner.clone();
    let mut legacy = Sha256::new();
    legacy.update(include_str!("../assets/roster-v2-field-proof.json"));
    legacy.update(include_str!("../assets/roster-v2-write-proof.json"));
    owner.proof = format!("{:x}", legacy.finalize());
    assert_ne!(owner.proof, writer::proof_tag());
    assert!(Generation::from_native(owner, [1; 32], vec![(REGION, 0x680)]).is_err());
    assert_eq!(m.writes.get(), 0);
}

#[test]
fn native_person_appearance_alias_and_late_person_projection_change_refuse_before_cas() {
    let m = Fake::new();
    m.person.set(APPEARANCE);
    assert!(Snapshot::capture(&m, &m.generation.borrow()).is_err());
    assert_eq!(m.writes.get(), 0);
    m.person.set(0);
    let snapshot = m.capture();
    let plan = Plan::new(&snapshot, Some(201.0), None).unwrap();
    m.person.set(APPEARANCE);
    let mut transaction = Transaction::new(snapshot, plan).unwrap();
    assert!(transaction.apply(&m, &mut m.journal()).is_err());
    assert_eq!(m.writes.get(), 0);
}

#[test]
fn composite_rebind_requires_exact_record_after_and_preserves_appearance_origin() {
    let m = Fake::new();
    let snap = m.capture();
    let plan = Plan::new(&snap, Some(190.0), None).unwrap();
    let mut after = snap.row.clone();
    after[0x130] = 4;
    let record = writer::Plan {
        kind: Kind::Player,
        index: 0,
        before: snap.row.clone(),
        after: after.clone(),
        patches: vec![writer::Patch {
            offset: 0x130,
            before: vec![snap.row[0x130]],
            after: vec![4],
            mask: vec![255],
        }],
        guards: vec![(0, Kind::Player.stride())],
        changed: vec!["fixture".into()],
    };
    assert!(snap.after_record(&m, &record).is_err());
    *m.row.borrow_mut() = after;
    let next = snap.after_record(&m, &record).unwrap();
    let bound = plan.after_record(&next).unwrap();
    assert_eq!(bound.after, plan.after);
    assert_eq!(bound.mask, plan.mask);
    assert!(Transaction::new(next.clone(), plan).is_err());
    assert!(Transaction::new(next.clone(), bound).is_ok());
    let mut foreign = next;
    foreign.native_metadata = Some([3; 32]);
    assert!(
        Plan::new(&snap, Some(190.0), None)
            .unwrap()
            .after_record(&foreign)
            .is_err()
    );
}
#[test]
fn native_journal_is_immutable_and_abandonment_never_clears_pending() {
    use std::{fs, path::PathBuf};
    let root = std::env::temp_dir().join(format!(
        "appearance-journal-{}",
        crate::backend::unique_id()
    ));
    fs::create_dir(&root).unwrap();
    fs::create_dir(root.join("Data")).unwrap();
    let m = Fake::new();
    let mut snap = m.capture();
    snap.native_metadata = Some([3; 32]);
    let plan = Plan::new(&snap, Some(190.0), None).unwrap();
    let record = writer::Plan {
        kind: Kind::Player,
        index: 0,
        before: snap.row.clone(),
        after: snap.row.clone(),
        patches: vec![],
        guards: vec![(0, Kind::Player.stride())],
        changed: vec![],
    };
    let mut journal =
        journal::Durable::begin(&root, &snap.generation.owner, &record, &snap, &plan).unwrap();
    assert!(journal::pending(&root).unwrap());
    assert!(journal::Durable::begin(&root, &snap.generation.owner, &record, &snap, &plan).is_err());
    journal.persist(&snap, &plan, Phase::Intent).unwrap();
    let seal = journal.current_seal().unwrap();
    assert!(
        seal.digest_for(&journal.id().unwrap(), &snap, &plan, 1)
            .is_ok()
    );
    let mut altered = plan.clone();
    altered.after[0] ^= 1;
    assert!(
        seal.digest_for(&journal.id().unwrap(), &snap, &altered, 1)
            .is_err()
    );
    assert!(
        seal.digest_for(&journal.id().unwrap(), &snap, &plan, 2)
            .is_err()
    );
    let phase = root
        .join("Data")
        .join(format!("ap2-{}.intent", journal.id().unwrap()));
    assert!(fs::write(&phase, b"foreign").is_err());
    drop(seal);
    drop(journal);
    assert!(journal::pending(&root).unwrap());
    let unknown = crate::session_safety::Process {
        pid: 7,
        name: "NBA2K27.exe".into(),
        parent: 0,
        identity: None,
    };
    // Recovery is separately authorized only with a complete no-game snapshot.
    assert!(journal::resolve_ended(&root, &[unknown]).is_err());
    assert!(journal::pending(&root).unwrap());
    // Simulate a crash after receipt publication but before marker deletion.
    // A foreign same-path receipt is never accepted, even after game closure.
    let marker: Value =
        serde_json::from_slice(&fs::read(root.join("Data").join(journal::MARKER)).unwrap())
            .unwrap();
    let nonce = marker["id"].as_str().unwrap();
    let receipt_path = root.join("Data").join(format!("ap2-{nonce}.game-ended"));
    fs::write(&receipt_path, b"foreign").unwrap();
    assert!(journal::resolve_ended(&root, &[]).is_err());
    assert!(journal::pending(&root).unwrap());
    fs::write(&receipt_path,serde_json::to_vec(&serde_json::json!({"owner":marker["owner"],"id":nonce,"reason":"game ended; never replay"})).unwrap()).unwrap();
    journal::resolve_ended(&root, &[]).unwrap();
    assert!(!journal::pending(&root).unwrap());
    fs::remove_dir_all(PathBuf::from(root)).unwrap();
}
