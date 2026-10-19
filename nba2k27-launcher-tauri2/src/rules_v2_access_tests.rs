use super::*;
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

fn binding() -> Binding {
    Binding::from_launched(
        Identity {
            pid: 11,
            started: "100".into(),
            path: r"C:\Fixture\NBA2K27.exe".into(),
        },
        Identity {
            pid: 22,
            started: "200".into(),
            path: r"C:\Portable\2k Launcher.exe".into(),
        },
        PathBuf::from(r"C:\Fixture"),
        PathBuf::from(r"C:\Portable"),
        "a".repeat(48),
    )
    .unwrap()
}
fn row(name: &str, pid: u32) -> Process {
    let b = binding();
    Process {
        name: name.into(),
        pid,
        parent: 0,
        identity: match pid {
            11 => Some(b.game),
            22 => Some(b.parent),
            _ => None,
        },
    }
}
fn snapshot() -> Snapshot {
    rules_v2::capture(&rules_v2::tests::fixture()).unwrap()
}
#[derive(Default)]
struct Faults {
    document: RefCell<Option<Value>>,
    events: RefCell<Vec<String>>,
    saves: Cell<usize>,
    reads: Cell<usize>,
    fail_before_save: Cell<usize>,
    fail_after_save: Cell<usize>,
    tamper_read: Cell<usize>,
    after_save: Cell<u8>,
    write_fault: Cell<u8>,
    journal_during_write: Cell<bool>,
}
struct FakeIo {
    state: Rc<RefCell<Snapshot>>,
    faults: Rc<Faults>,
    writes: Cell<usize>,
}
impl RulesIo for FakeIo {
    fn capture(&self) -> Result<Snapshot> {
        Ok(self.state.borrow().clone())
    }
    fn change(
        &self,
        expected: &Snapshot,
        index: usize,
        value: u32,
        durable: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        ensure!(*self.state.borrow() == *expected, "Fake state changed");
        if self.faults.journal_during_write.replace(false) {
            self.faults.document.borrow_mut().as_mut().unwrap()["owner"] =
                json!(expected.context.owner + 8);
        }
        durable()?;
        self.writes.set(self.writes.get() + 1);
        self.faults
            .events
            .borrow_mut()
            .push(format!("write:{index}:{value}"));
        self.state.borrow_mut().values[index] = value;
        match self.faults.write_fault.replace(0) {
            1 => anyhow::bail!("reported failure after exact write"),
            2 => {
                self.state.borrow_mut().values[index] = 99;
                anyhow::bail!("third value");
            }
            3 => {
                self.state.borrow_mut().values[(index + 1) % 39] ^= 1;
            }
            4 => {
                self.state.borrow_mut().context.owner += 8;
            }
            5 => {
                self.state.borrow_mut().context.stage = 99;
            }
            _ => {}
        }
        Ok(())
    }
}
struct FakeJournal {
    faults: Rc<Faults>,
    state: Rc<RefCell<Snapshot>>,
}
impl Journal for FakeJournal {
    fn pending(&self) -> Result<bool> {
        self.faults
            .document
            .borrow()
            .as_ref()
            .map(|d| d["pending"].as_bool().context("Malformed journal"))
            .transpose()
            .map(|v| v.unwrap_or(false))
    }
    fn save(&self, intent: &Intent) -> Result<()> {
        let n = self.faults.saves.get() + 1;
        self.faults.saves.set(n);
        if self.faults.fail_before_save.get() == n {
            anyhow::bail!("save failed before publication");
        }
        *self.faults.document.borrow_mut() = Some(serde_json::to_value(intent)?);
        self.faults
            .events
            .borrow_mut()
            .push(format!("save:{}", intent.pending));
        match self.faults.after_save.replace(0) {
            1 => self.state.borrow_mut().context.owner += 8,
            2 => self.state.borrow_mut().values[1] ^= 1,
            3 => self.state.borrow_mut().context.current ^= 1,
            _ => {}
        }
        if self.faults.fail_after_save.get() == n {
            anyhow::bail!("save reported failure after publication");
        }
        Ok(())
    }
    fn read(&self) -> Result<Value> {
        let n = self.faults.reads.get() + 1;
        self.faults.reads.set(n);
        if self.faults.tamper_read.get() == n {
            self.faults.document.borrow_mut().as_mut().unwrap()["before"] = json!(99);
        }
        self.faults
            .document
            .borrow()
            .clone()
            .context("Missing rule journal")
    }
}
fn fixture() -> (RulesAccessV2<FakeIo>, Lease, Rc<Faults>) {
    let b = binding();
    let s = snapshot();
    let state = Rc::new(RefCell::new(s.clone()));
    let faults = Rc::new(Faults::default());
    let access = RulesAccessV2 {
        io: FakeIo {
            state: state.clone(),
            faults: faults.clone(),
            writes: Cell::new(0),
        },
        binding: b.clone(),
        journal: Box::new(FakeJournal {
            state,
            faults: faults.clone(),
        }),
        permit: RefCell::new(None),
    };
    (access, Lease::new(b.game, b.session, s), faults)
}
#[test]
fn rules_access_only_exact_retained_game_and_parent_are_accepted() {
    let b = binding();
    assert!(guard_with_current(&[row("NBA2K27.exe", 11)], &b, Some(&b.parent)).is_ok());
    assert!(
        guard_with_current(
            &[row("NBA2K27.exe", 11), row("2k Launcher.exe", 22)],
            &b,
            None
        )
        .is_ok()
    );
    for rows in [
        vec![row("NBA2K27.exe", 11)],
        vec![row("NBA2K27.exe", 11), row("NBA2K27.exe", 99)],
        vec![row("NBA2K27.exe", 11), row("node.exe", 99)],
        vec![row("NBA2K27.exe", 11), row("EasyAntiCheat_EOS.exe", 99)],
        vec![row("NBA2K27.exe", 11), row("start_protected_game.exe", 99)],
    ] {
        assert!(
            guard_with_current(
                &rows,
                &b,
                if rows.len() == 1 {
                    None
                } else {
                    Some(&b.parent)
                }
            )
            .is_err()
        );
    }
    let mut changed = row("NBA2K27.exe", 11);
    changed.identity.as_mut().unwrap().started = "101".into();
    assert!(guard_with_current(&[changed], &b, Some(&b.parent)).is_err());
    let mut changed = b.parent.clone();
    changed.started = "201".into();
    assert!(guard_with_current(&[row("NBA2K27.exe", 11)], &b, Some(&changed)).is_err());
    let mut changed = b.game.clone();
    changed.path = r"C:\Other\NBA2K27.exe".into();
    assert!(
        Binding::from_launched(changed, b.parent, b.game_root, b.portable_root, b.session).is_err()
    );
}
fn region() -> Region {
    Region {
        base: 0x30000000000,
        allocation: 0x30000000000,
        len: 4096,
        state: MEM_COMMIT.0,
        kind: MEM_PRIVATE.0,
        protection: PAGE_READWRITE.0,
    }
}
#[test]
fn rules_access_entire_owner_must_be_private_rw_committed_canonical_and_contained() {
    let r = region();
    assert!(owner_region(r, r.base).is_ok());
    for changed in [
        Region {
            len: OWNER_BYTES as u64 - 1,
            ..r
        },
        Region {
            kind: MEM_IMAGE.0,
            ..r
        },
        Region {
            protection: PAGE_EXECUTE_READWRITE.0,
            ..r
        },
        Region {
            protection: PAGE_READONLY.0,
            ..r
        },
        Region {
            protection: PAGE_READWRITE.0 | PAGE_GUARD.0,
            ..r
        },
        Region {
            state: MEM_RESERVE.0,
            ..r
        },
        Region { allocation: 0, ..r },
        Region {
            allocation: r.base + 4096,
            ..r
        },
        Region {
            allocation: r.base + 8,
            ..r
        },
        Region {
            base: u64::MAX - 4,
            len: 4096,
            ..r
        },
    ] {
        assert!(owner_region(changed, r.base).is_err(), "{changed:?}");
    }
    for at in [
        0,
        0xFFFF,
        r.base + 1,
        r.base + 4096 - OWNER_BYTES as u64 + 8,
        u64::MAX - 7,
    ] {
        assert!(owner_region(r, at).is_err());
    }
    assert!(readable(
        Region {
            kind: MEM_IMAGE.0,
            protection: PAGE_EXECUTE_READ.0,
            ..r
        },
        r.base,
        r.base,
        100
    ));
    assert!(!readable(
        Region {
            kind: MEM_IMAGE.0,
            protection: PAGE_EXECUTE_READ.0,
            ..r
        },
        r.base + 4096,
        r.base,
        100
    ));
    assert!(!readable(
        Region {
            protection: PAGE_EXECUTE_READWRITE.0,
            ..r
        },
        r.base,
        r.base,
        100
    ));
}
#[test]
fn rules_access_native_open_rejects_unowned_parent_before_any_game_read() {
    let result = RulesAccessV2::open(&binding());
    assert!(result.is_err());
    let error = format!("{:#}", result.err().unwrap());
    // In production the real process scanner rejects this fabricated parent.
    // The standalone agent harness intentionally forbids even enumeration.
    assert!(
        error == "Owning launcher is absent or changed" || error.contains("forbidden"),
        "{error}"
    );
}
#[test]
fn rules_access_retained_handle_liveness_never_uses_undefined_exit_time() {
    let expected = binding().game;
    live_birth(WAIT_TIMEOUT, 100, &expected).unwrap();
    assert!(live_birth(WAIT_TIMEOUT, 101, &expected).is_err());
    for state in [WAIT_OBJECT_0, WAIT_FAILED, WAIT_EVENT(7)] {
        assert!(live_birth(state, 100, &expected).is_err());
    }
    assert_ne!(QUERY_RIGHTS.0 & PROCESS_SYNCHRONIZE.0, 0);
    assert_ne!(WRITE_RIGHTS.0 & PROCESS_SYNCHRONIZE.0, 0);
}
#[test]
fn rules_access_unjournaled_direct_shared_apply_and_write_cannot_acquire_writer() {
    let (a, l, f) = fixture();
    assert!(game_rules::Access::write(&a, &l.snapshot, 0, 1).is_err());
    assert!(game_rules::apply(&a, &l, "ShotClock", 1, |_| Ok(())).is_err());
    assert_eq!(a.io.writes.get(), 0);
    assert_eq!(f.saves.get(), 0);
    assert!(a.permit.borrow().is_none());
}
#[test]
fn rules_access_all_39_fields_and_every_numeric_choice_preserve_38_neighbors() {
    let mut choices = 0;
    for (index, field) in FIELDS.iter().enumerate() {
        for before in (0..=6).filter(|v| field.valid(*v)) {
            for after in (0..=6).filter(|v| field.valid(*v) && *v != before) {
                let (a, mut l, f) = fixture();
                l.snapshot.values[index] = before;
                a.io.state.borrow_mut().values[index] = before;
                a.apply_inner(&l, field.id, after).unwrap();
                let mut expected = l.snapshot.clone();
                expected.values[index] = after;
                assert_eq!(a.capture().unwrap(), expected);
                assert_eq!(a.io.writes.get(), 1);
                assert_eq!(
                    f.events.borrow().as_slice(),
                    [
                        "save:true",
                        format!("write:{index}:{after}").as_str(),
                        "save:false"
                    ]
                );
                assert_eq!(f.document.borrow().as_ref().unwrap()["pending"], false);
                assert!(a.permit.borrow().is_none());
                // No reusable write capability survives completion.
                assert!(game_rules::Access::write(&a, &expected, index, before).is_err());
                choices += 1;
            }
        }
    }
    assert_eq!(choices, 130);
}
#[test]
fn rules_access_initial_journal_failure_or_corruption_never_writes() {
    for fault in 0..4 {
        let (a, l, f) = fixture();
        match fault {
            0 => f.fail_before_save.set(1),
            1 => f.fail_after_save.set(1),
            2 => f.tamper_read.set(1),
            _ => f.tamper_read.set(2),
        }
        assert!(a.apply_inner(&l, "ShotClock", 1).is_err());
        assert_eq!(a.io.writes.get(), 0);
        assert_eq!(a.capture().unwrap(), l.snapshot);
        assert!(a.permit.borrow().is_none());
    }
}
#[test]
fn rules_access_changed_owner_mode_or_neighbor_after_intent_holds_without_writes() {
    for fault in [1, 2, 3] {
        let (a, l, f) = fixture();
        f.after_save.set(fault);
        assert!(a.apply_inner(&l, "ShotClock", 1).is_err());
        assert_eq!(a.io.writes.get(), 0);
        assert_eq!(f.document.borrow().as_ref().unwrap()["pending"], true);
    }
}
#[test]
fn rules_access_journal_is_rechecked_inside_delayed_writer_before_effect() {
    let (a, l, f) = fixture();
    f.journal_during_write.set(true);
    assert!(a.apply_inner(&l, "ShotClock", 1).is_err());
    assert_eq!(a.io.writes.get(), 0);
    assert_eq!(a.capture().unwrap(), l.snapshot);
}
#[test]
fn rules_access_exact_reported_failure_restores_only_owned_value() {
    let (a, l, f) = fixture();
    f.write_fault.set(1);
    assert!(a.apply_inner(&l, "ShotClock", 1).is_err());
    assert_eq!(a.io.writes.get(), 2);
    assert_eq!(a.capture().unwrap(), l.snapshot);
    assert_eq!(f.document.borrow().as_ref().unwrap()["pending"], false);
    assert_eq!(
        f.events.borrow().as_slice(),
        ["save:true", "write:0:1", "write:0:0", "save:false"]
    );
}
#[test]
fn rules_access_foreign_value_neighbor_owner_or_mode_is_never_rolled_back() {
    for fault in [2, 3, 4, 5] {
        let (a, l, f) = fixture();
        f.write_fault.set(fault);
        assert!(a.apply_inner(&l, "ShotClock", 1).is_err());
        assert_eq!(a.io.writes.get(), 1);
        assert_eq!(f.document.borrow().as_ref().unwrap()["pending"], true);
        assert!(a.apply_inner(&l, "ShotClock", 1).is_err());
        assert_eq!(a.io.writes.get(), 1);
    }
}
#[test]
fn rules_access_failed_commit_rearms_same_intent_before_owned_reverse_write() {
    for after_publication in [false, true] {
        let (a, l, f) = fixture();
        if after_publication {
            f.fail_after_save.set(2);
        } else {
            f.fail_before_save.set(2);
        }
        assert!(a.apply_inner(&l, "ShotClock", 1).is_err());
        assert_eq!(a.io.writes.get(), 2);
        assert_eq!(a.capture().unwrap(), l.snapshot);
        assert_eq!(f.document.borrow().as_ref().unwrap()["pending"], false);
        let events = f.events.borrow();
        if after_publication {
            assert_eq!(
                events.as_slice(),
                [
                    "save:true",
                    "write:0:1",
                    "save:false",
                    "save:true",
                    "write:0:0",
                    "save:false"
                ]
            );
        } else {
            assert_eq!(
                events.as_slice(),
                ["save:true", "write:0:1", "write:0:0", "save:false"]
            );
        }
    }
}
#[test]
fn rules_access_journal_ownership_change_after_effect_prevents_reverse_write() {
    let (a, l, f) = fixture();
    f.write_fault.set(1);
    f.tamper_read.set(4);
    assert!(a.apply_inner(&l, "ShotClock", 1).is_err());
    assert_eq!(a.io.writes.get(), 1);
    assert_eq!(f.document.borrow().as_ref().unwrap()["pending"], true);
}
#[test]
fn rules_access_invalid_noop_stale_and_expired_leases_do_not_write() {
    let (a, mut l, f) = fixture();
    for (field, value) in [
        ("Unknown", 1),
        ("ShotClock", 2),
        ("ShotClock", 0),
        ("FoulOut", 7),
    ] {
        assert!(a.apply_inner(&l, field, value).is_err());
    }
    a.io.state.borrow_mut().values[1] = 1;
    assert!(a.apply_inner(&l, "ShotClock", 1).is_err());
    static GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let gate = GATE.lock().unwrap();
    assert!(
        a.apply_under_gate(&l, "wrong", "ShotClock", 1, &gate)
            .is_err()
    );
    l.created = Instant::now() - Duration::from_secs(30);
    assert!(
        a.apply_under_gate(&l, &l.token, "ShotClock", 1, &gate)
            .is_err()
    );
    assert_eq!(a.io.writes.get(), 0);
    assert_eq!(f.saves.get(), 0);
}
#[test]
fn rules_access_ui_document_keeps_existing_choices_and_correct_version2_boundary() {
    let (_, l, _) = fixture();
    let old = l.view();
    let mut current = lease_view(&l);
    assert_eq!(current["boundary"], "selection");
    current["boundary"] = old["boundary"].clone();
    assert_eq!(current, old);
    assert_eq!(current["rules"].as_array().unwrap().len(), 39);
}

#[test]
fn rules_access_actual_journal_reader_rejects_oversize_malformed_and_nonordinary_files() {
    let dir = std::env::temp_dir().join(format!("rules-v2-journal-{}", backend::unique_id()));
    std::fs::create_dir_all(dir.join("Data")).unwrap();
    // Directory pin construction is covered by the existing loader store;
    // this isolated fixture exercises the real Win32 file-reader policy only.
    let journal = DiskJournal {
        root: dir.clone(),
        _pins: vec![],
    };
    assert!(!journal.pending().unwrap());
    let (a, l, f) = fixture();
    a.apply_inner(&l, "ShotClock", 1).unwrap();
    let intent: Intent =
        serde_json::from_value(f.document.borrow().as_ref().unwrap().clone()).unwrap();
    journal.save(&intent).unwrap();
    assert_eq!(
        journal.read().unwrap(),
        f.document.borrow().as_ref().unwrap().clone()
    );
    assert!(!journal.pending().unwrap());
    let path = game_rules::path(&dir);
    for bytes in [b"{}".to_vec(), vec![b' '; 16385], Vec::new()] {
        std::fs::write(&path, &bytes).unwrap();
        assert!(journal.read().is_err());
        assert!(journal.pending().is_err());
        assert!(journal.save(&intent).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert!(journal.read().is_err());
    assert!(journal.pending().is_err());
    assert!(journal.save(&intent).is_err());
    assert!(path.is_dir());
    // Only the uniquely named synthetic temp fixture is removed.
    assert!(
        dir.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("rules-v2-journal-")
    );
    std::fs::remove_dir_all(dir).unwrap();
}
