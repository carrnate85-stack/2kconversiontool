use super::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
fn snapshot() -> policy::Snapshot {
    let rules = crate::rules_v2::capture(&crate::rules_v2::tests::fixture()).unwrap();
    let mut window = vec![0; 16];
    window[1] = 1;
    policy::Snapshot {
        base: rules.base,
        context: rules.context,
        owner: 0x40000000000,
        preload_table: 0x50000000000,
        window,
        sources: (175..=179)
            .map(|index| {
                let mut front = vec![0; 24];
                front[1] = 0x4C;
                front[8] = 1;
                let mut ambient = vec![0; 24];
                ambient[1] = 0x4D;
                ambient[8] = 1;
                policy::Source {
                    index,
                    address: 0x50000000000 + index as u64 * 0xC810 + 8,
                    front,
                    ambient,
                }
            })
            .collect(),
    }
}
#[derive(Default)]
struct Faults {
    saves: Cell<usize>,
    writes: Cell<usize>,
    captures: Cell<usize>,
    fail_save: Cell<usize>,
    fail_write: Cell<usize>,
    fail_verify: Cell<bool>,
    foreign_after_intent: Cell<bool>,
    capture_wait: Cell<bool>,
    drift_after_capture: Cell<usize>,
    transition_on_failed_write: Cell<bool>,
    events: RefCell<Vec<String>>,
}
struct Fake {
    state: Rc<RefCell<policy::Snapshot>>,
    faults: Rc<Faults>,
}
impl Access for Fake {
    fn boundary(&self) -> Result<Boundary> {
        let s = self.state.borrow();
        Ok(Boundary {
            base: s.base,
            context: s.context.clone(),
        })
    }
    fn capture(&self, crowd: bool) -> Result<policy::Snapshot> {
        self.faults.captures.set(self.faults.captures.get() + 1);
        ensure!(
            !self.faults.capture_wait.get(),
            "Waiting for preset initialization"
        );
        let mut s = self.state.borrow().clone();
        if self.faults.drift_after_capture.get() == self.faults.captures.get() {
            // The game touches an unowned neighbor right after this sample.
            self.state.borrow_mut().window[3] ^= 1;
        }
        ensure!(
            matches!(
                rules_v2::boundary(&s.context, s.base),
                "selection" | "main-menu" | "quick-play"
            ),
            "Transient capture forbidden"
        );
        if !crowd {
            s.sources.clear();
            s.preload_table = 0;
        }
        Ok(s)
    }
    fn publish(
        &self,
        p: &Permit,
        parent_required: bool,
        durable: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        p.valid()?;
        durable()?;
        ensure!(
            self.capture(!p.before.sources.is_empty())? == p.before,
            "Foreign context or neighbor"
        );
        let n = self.faults.writes.get() + 1;
        self.faults.writes.set(n);
        self.faults
            .events
            .borrow_mut()
            .push(format!("write:{:?}:{parent_required}", p.change.kind));
        apply_to(&mut self.state.borrow_mut(), &p.change)?;
        if self.faults.fail_write.get() == n {
            if self.faults.transition_on_failed_write.get() {
                let mut state = self.state.borrow_mut();
                state.context.current = 0xB4D37B22;
                state.context.alternate = 0xB4D37B22;
                state.context.stage = 1;
            }
            anyhow::bail!("Reported partial-operation failure");
        }
        Ok(())
    }
}
struct FakeLog {
    state: Rc<RefCell<policy::Snapshot>>,
    faults: Rc<Faults>,
}
impl Log for FakeLog {
    fn intent(&mut self, operation: &str, _: &policy::Transaction) -> Result<Proof> {
        let n = self.faults.saves.get() + 1;
        self.faults.saves.set(n);
        ensure!(self.faults.fail_save.get() != n, "Disk full");
        self.faults
            .events
            .borrow_mut()
            .push(format!("intent:{operation}"));
        if self.faults.foreign_after_intent.replace(false) {
            self.state.borrow_mut().owner += 8;
        }
        Ok(Proof {
            offset: n as u64,
            bytes: operation.as_bytes().to_vec(),
        })
    }
    fn verify(&self, _: &Proof) -> Result<()> {
        ensure!(!self.faults.fail_verify.get(), "Durable readback mismatch");
        Ok(())
    }
    fn note(&mut self, event: &str, operation: &str, _: &str) -> Result<()> {
        self.faults
            .events
            .borrow_mut()
            .push(format!("{event}:{operation}"));
        Ok(())
    }
}
fn fixture(options: Options) -> (Engine, Fake, FakeLog, Rc<Faults>) {
    let state = Rc::new(RefCell::new(snapshot()));
    let faults = Rc::new(Faults::default());
    (
        Engine::new(options),
        Fake {
            state: state.clone(),
            faults: faults.clone(),
        },
        FakeLog {
            state,
            faults: faults.clone(),
        },
        faults,
    )
}
fn settle(engine: &mut Engine, access: &Fake, log: &mut FakeLog, parent: bool) -> Result<()> {
    engine.tick(access, log, parent)?;
    engine.tick(access, log, parent)
}
fn menu(access: &Fake, mode: u32) {
    let mut s = access.state.borrow_mut();
    s.context.current = mode;
    s.context.alternate = mode;
    s.context.stage = 1;
}
fn selection(access: &Fake) {
    menu(access, 0x92DF06DE);
    access.state.borrow_mut().context.stage = 3;
}
#[test]
fn presentation_runner_all_masks_and_all_five_sizes_prepare_exactly_once_per_selection() {
    for mask in 0..8u8 {
        let options = Options {
            remove_drone: mask & 1 != 0,
            remove_dj: mask & 2 != 0,
            remove_mc: mask & 4 != 0,
            remove_crowd: true,
        };
        let (mut e, a, mut l, f) = fixture(options);
        let original = a.state.borrow().clone();
        settle(&mut e, &a, &mut l, true).unwrap();
        let expected_actor =
            (mask & 1 != 0) as usize + (mask & 2 != 0) as usize + 2 * (mask & 4 != 0) as usize;
        assert_eq!(f.writes.get(), 6 + expected_actor);
        assert_eq!(a.state.borrow().window[1], 0);
        assert!(
            a.state.borrow().sources.iter().all(|r| r.front[8] == 0
                && r.ambient == original.sources[(r.index - 175) as usize].ambient)
        );
        assert_eq!(a.state.borrow().window[8], a.state.borrow().window[11]);
        let captures = f.captures.get();
        for _ in 0..12 {
            e.tick(&a, &mut l, true).unwrap();
        }
        assert_eq!(f.captures.get(), captures);
        assert_eq!(f.writes.get(), 6 + expected_actor);
        assert_eq!(e.status()["preparedVisits"], 1);
    }
}
#[test]
fn presentation_runner_restores_and_rearms_at_both_main_and_quick_play() {
    for exit in [0x6509837B, 0x079964C0] {
        let (mut e, a, mut l, f) = fixture(Options {
            remove_crowd: true,
            remove_mc: true,
            ..Options::default()
        });
        settle(&mut e, &a, &mut l, true).unwrap();
        let applied = a.state.borrow().clone();
        menu(&a, 0xB4D37B22);
        let read_count = f.captures.get();
        for _ in 0..5 {
            e.tick(&a, &mut l, true).unwrap();
        }
        assert_eq!(f.captures.get(), read_count);
        assert_eq!(a.state.borrow().sources, applied.sources);
        menu(&a, exit);
        settle(&mut e, &a, &mut l, true).unwrap();
        assert_eq!(a.state.borrow().window[1], 1);
        assert!(a.state.borrow().sources.iter().all(|r| r.front[8] == 1));
        assert_eq!(a.state.borrow().window[8], 1);
        assert_eq!(a.state.borrow().window[11], 1);
        assert!(e.crowd.is_none());
        selection(&a);
        settle(&mut e, &a, &mut l, true).unwrap();
        assert_eq!(e.visits, 2);
        assert_eq!(a.state.borrow().window[1], 0);
        assert!(a.state.borrow().sources.iter().all(|r| r.front[8] == 0));
    }
}
#[test]
fn presentation_runner_reasserts_owned_crowd_after_direct_match_to_selection_reset() {
    let (mut e, a, mut l, f) = fixture(Options {
        remove_crowd: true,
        ..Options::default()
    });
    settle(&mut e, &a, &mut l, true).unwrap();
    menu(&a, 0xB4D37B22);
    e.tick(&a, &mut l, true).unwrap();
    {
        let mut s = a.state.borrow_mut();
        s.window[1] = 1;
        for row in &mut s.sources {
            row.front[8] = 1;
        }
    }
    selection(&a);
    settle(&mut e, &a, &mut l, true).unwrap();
    assert_eq!(f.writes.get(), 12);
    assert_eq!(e.visits, 1);
    menu(&a, 0x079964C0);
    settle(&mut e, &a, &mut l, true).unwrap();
    assert_eq!(f.writes.get(), 18);
}
#[test]
fn presentation_runner_missing_preloads_defer_and_never_read_during_loading_or_other_modes() {
    let (mut e, a, mut l, f) = fixture(Options {
        remove_crowd: true,
        ..Options::default()
    });
    f.capture_wait.set(true);
    settle(&mut e, &a, &mut l, true).unwrap();
    assert!(!e.held);
    assert_eq!(f.writes.get(), 0);
    assert!(!e.prepared_selection);
    f.capture_wait.set(false);
    e.tick(&a, &mut l, true).unwrap();
    assert_eq!(f.writes.get(), 6);
    menu(&a, 0xF54D5D12);
    let count = f.captures.get();
    for _ in 0..5 {
        e.tick(&a, &mut l, true).unwrap();
    }
    assert_eq!(f.captures.get(), count);
}
#[test]
fn presentation_runner_no_parent_cannot_prepare_but_can_restore_owned_crowd_at_safe_exit() {
    let (mut e, a, mut l, f) = fixture(Options {
        remove_crowd: true,
        ..Options::default()
    });
    settle(&mut e, &a, &mut l, false).unwrap();
    assert_eq!(f.writes.get(), 0);
    settle(&mut e, &a, &mut l, true).unwrap();
    menu(&a, 0x6509837B);
    settle(&mut e, &a, &mut l, false).unwrap();
    assert_eq!(f.writes.get(), 12);
    assert!(
        f.events
            .borrow()
            .iter()
            .filter(|v| v.starts_with("write:"))
            .skip(6)
            .all(|v| v.ends_with(":false"))
    );
}
#[test]
fn presentation_runner_journal_faults_foreign_owner_and_partial_effect_hold_without_more_writes() {
    for fault in 0..4 {
        let (mut e, a, mut l, f) = fixture(Options {
            remove_crowd: true,
            ..Options::default()
        });
        match fault {
            0 => f.fail_save.set(1),
            1 => f.fail_verify.set(true),
            2 => f.foreign_after_intent.set(true),
            _ => f.fail_write.set(2),
        }
        assert!(settle(&mut e, &a, &mut l, true).is_err());
        assert!(e.held);
        let count = f.writes.get();
        assert!(count <= 2);
        for _ in 0..5 {
            e.tick(&a, &mut l, true).unwrap();
        }
        assert_eq!(f.writes.get(), count);
        assert!(e.crowd.is_some());
    }
}
#[test]
fn presentation_runner_foreign_preset_or_window_neighbor_is_never_restored() {
    for field in 0..3 {
        let (mut e, a, mut l, f) = fixture(Options {
            remove_crowd: true,
            ..Options::default()
        });
        settle(&mut e, &a, &mut l, true).unwrap();
        match field {
            0 => a.state.borrow_mut().sources[0].ambient[8] ^= 1,
            1 => a.state.borrow_mut().window[4] ^= 1,
            _ => a.state.borrow_mut().preload_table += 8,
        }
        menu(&a, 0x079964C0);
        assert!(settle(&mut e, &a, &mut l, true).is_err());
        assert_eq!(f.writes.get(), 6);
        assert!(e.held && e.crowd.is_some());
    }
}
#[test]
fn presentation_partial_owned_preparation_restores_at_exit_but_never_reprepares() {
    let (mut e, a, mut l, f) = fixture(Options {
        remove_crowd: true,
        ..Options::default()
    });
    f.fail_write.set(2);
    assert!(settle(&mut e, &a, &mut l, true).is_err());
    assert_eq!(f.writes.get(), 2);
    assert!(e.held && e.crowd.is_some());
    menu(&a, 0x079964C0);
    settle(&mut e, &a, &mut l, true).unwrap();
    assert_eq!(f.writes.get(), 4);
    assert!(e.held && e.crowd.is_none());
    assert_eq!(a.state.borrow().window[1], 1);
    assert!(a.state.borrow().sources.iter().all(|s| s.front[8] == 1));
    selection(&a);
    settle(&mut e, &a, &mut l, true).unwrap();
    assert_eq!(f.writes.get(), 4);
}
#[test]
fn presentation_permit_cannot_escape_fixed_byte_context_or_size() {
    let original = snapshot();
    let tx = policy::crowd(&original).unwrap();
    let change = tx.changes[0].clone();
    let mut after = original.clone();
    apply_to(&mut after, &change).unwrap();
    let p = Permit {
        before: original.clone(),
        after,
        change,
    };
    p.valid().unwrap();
    let mut bad = p.clone();
    bad.change.at += 1;
    assert!(bad.valid().is_err());
    let mut bad = p.clone();
    bad.after.window[2] ^= 1;
    assert!(bad.valid().is_err());
    let mut bad = p.clone();
    bad.before.context.current = 0;
    assert!(bad.valid().is_err());
    let mut bad = p.clone();
    bad.change.after = 2;
    assert!(bad.valid().is_err());
}

fn binding(portable: PathBuf) -> Binding {
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
        portable,
        "a".repeat(48),
    )
    .unwrap()
}
#[test]
fn presentation_real_journal_syncs_reads_back_denies_foreign_writer_and_recovers_only_ended_owner()
{
    let root = std::env::temp_dir().join(format!(
        "presentation-v2-log-{}",
        crate::backend::unique_id()
    ));
    fs::create_dir_all(root.join("Data")).unwrap();
    let b = binding(root.clone());
    let mut journal = Journal::create(
        &b,
        Options {
            remove_crowd: true,
            ..Options::default()
        },
    )
    .unwrap();
    let tx = policy::crowd(&snapshot()).unwrap();
    let proof = journal.intent("crowd-prepare", &tx).unwrap();
    journal.verify(&proof).unwrap();
    assert!(
        fs::OpenOptions::new()
            .write(true)
            .open(root.join("Data").join(MARKER))
            .is_err()
    );
    let mut changed = proof.clone();
    changed.bytes[0] ^= 1;
    assert!(journal.verify(&changed).is_err());
    journal
        .note("applied", "crowd-prepare", "read back")
        .unwrap();
    journal.verify(&proof).unwrap();
    drop(journal);
    assert!(pending(&root).unwrap());
    let mut wrong = b.game.clone();
    wrong.started = "101".into();
    assert!(remove_ended(&root, &wrong, &b.session, &[]).is_err());
    assert!(pending(&root).unwrap());
    let rows = [session_safety::Process {
        name: "NBA2K27.exe".into(),
        pid: 11,
        parent: 22,
        identity: Some(b.game.clone()),
    }];
    assert!(remove_ended(&root, &b.game, &b.session, &rows).is_err());
    assert!(pending(&root).unwrap());
    remove_ended(&root, &b.game, &b.session, &[]).unwrap();
    assert!(!pending(&root).unwrap());
    assert!(
        root.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("presentation-v2-log-")
    );
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn presentation_default_runner_never_opens_processes_or_creates_a_journal() {
    let root = PathBuf::from(r"C:\NeverCreatedPresentationFixture");
    let mut r = Runner::new(binding(root), Options::default());
    r.tick(true).unwrap();
    assert!(r.native.is_none() && r.journal.is_none());
    assert_eq!(r.status()["crowd"], "disabled");
}
#[test]
fn presentation_selection_that_moves_before_intent_waits_and_retries_without_holding() {
    for options in [
        Options {
            remove_drone: true,
            remove_dj: true,
            remove_mc: true,
            ..Options::default()
        },
        Options {
            remove_crowd: true,
            ..Options::default()
        },
    ] {
        let (mut e, a, mut l, f) = fixture(options);
        f.drift_after_capture.set(1);
        settle(&mut e, &a, &mut l, true).unwrap();
        assert!(!e.held && !e.prepared_selection && e.crowd.is_none());
        assert_eq!(f.saves.get() + f.writes.get(), 0);
        assert!(e.status()["message"].as_str().unwrap().contains("settling"));
        e.tick(&a, &mut l, true).unwrap();
        assert!(!e.held && e.prepared_selection);
        assert_eq!(f.saves.get(), 1);
        if options.remove_crowd {
            assert_eq!(f.writes.get(), 6);
            assert_eq!(e.visits, 1);
        } else {
            assert_eq!(f.writes.get(), 4);
            let s = a.state.borrow();
            assert!([6, 7, 8, 11].iter().all(|i| s.window[*i] == 1));
        }
    }
}
#[test]
fn presentation_runner_restores_crowd_at_observed_version_two_main_stage_zero() {
    let (mut e, a, mut l, f) = fixture(Options {
        remove_crowd: true,
        remove_drone: true,
        ..Options::default()
    });
    settle(&mut e, &a, &mut l, true).unwrap();
    assert_eq!(f.writes.get(), 7);
    menu(&a, 0x6509837B);
    {
        let mut s = a.state.borrow_mut();
        s.context.stage = 0;
        s.context.game_mode = 4;
    }
    settle(&mut e, &a, &mut l, true).unwrap();
    assert!(!e.held && e.crowd.is_none());
    assert_eq!(f.writes.get(), 13);
    let s = a.state.borrow();
    assert_eq!(s.window[1], 1);
    assert!(s.sources.iter().all(|r| r.front[8] == 1));
    // Drone/DJ/MC stay session-long, as in WPF.
    assert_eq!(s.window[6], 1);
}
#[test]
fn presentation_interrupted_actor_write_retries_only_when_owned_bytes_moved() {
    let all = Options {
        remove_drone: true,
        remove_dj: true,
        remove_mc: true,
        ..Options::default()
    };
    let (mut e, a, mut l, f) = fixture(all);
    f.fail_write.set(2);
    settle(&mut e, &a, &mut l, true).unwrap();
    assert!(!e.held && !e.prepared_selection);
    assert_eq!(f.writes.get(), 2);
    assert!(e.status()["message"].as_str().unwrap().contains("retrying"));
    e.tick(&a, &mut l, true).unwrap();
    assert!(!e.held && e.prepared_selection);
    assert_eq!(f.writes.get(), 4);
    let s = a.state.borrow().clone();
    assert!([6, 7, 8, 11].iter().all(|i| s.window[*i] == 1));
    assert_eq!(s.window[1], 1);
    // A foreign owner after the intent is still a hold, with no retry.
    let (mut e, _a, mut l2, f) = fixture(all);
    f.foreign_after_intent.set(true);
    assert!(settle(&mut e, &_a, &mut l2, true).is_err());
    assert!(e.held);
    // Retries are bounded per game.
    let (mut e, a, mut l, f) = fixture(all);
    e.tick(&a, &mut l, true).unwrap();
    for n in 0..=ACTOR_RETRIES as usize {
        {
            // The game clears these at match end; start each attempt fresh.
            let mut s = a.state.borrow_mut();
            for i in [6, 7, 8, 11] {
                s.window[i] = 0;
            }
        }
        f.fail_write.set(f.writes.get() + 1);
        let r = e.tick(&a, &mut l, true);
        if n < ACTOR_RETRIES as usize {
            r.unwrap();
            assert!(!e.held && !e.prepared_selection);
        } else {
            assert!(r.is_err() && e.held);
        }
    }
}
/// What the game does at every match end (live 0.47/0.48 probes): drone/DJ/MC
/// bytes cleared and the stand-crowd global set back to its stock value.
fn game_match_end_reset(a: &Fake) {
    let mut s = a.state.borrow_mut();
    for i in [6, 7, 8, 11] {
        s.window[i] = 0;
    }
    s.window[1] = 1;
}
#[test]
fn presentation_every_blacktop_visit_reapplies_after_the_games_own_match_end_reset() {
    let all = Options {
        remove_drone: true,
        remove_dj: true,
        remove_mc: true,
        remove_crowd: true,
    };
    let removed = |a: &Fake| {
        let s = a.state.borrow();
        s.window[1] == 0
            && [6, 7, 8, 11].iter().all(|i| s.window[*i] == 1)
            && s.sources.iter().all(|r| r.front[8] == 0)
    };
    let (mut e, a, mut l, _f) = fixture(all);
    settle(&mut e, &a, &mut l, true).unwrap();
    assert!(removed(&a));
    for visit in 2..=4u32 {
        // Match, then the game's reset, then straight back to selection.
        menu(&a, 0xB4D37B22);
        e.tick(&a, &mut l, true).unwrap();
        game_match_end_reset(&a);
        selection(&a);
        settle(&mut e, &a, &mut l, true).unwrap();
        assert!(!e.held, "visit {visit}: {}", e.status()["message"]);
        assert!(removed(&a), "visit {visit} not re-applied");
    }
    // Leaving through Main after a reset still restores only the crowd.
    menu(&a, 0xB4D37B22);
    e.tick(&a, &mut l, true).unwrap();
    game_match_end_reset(&a);
    menu(&a, 0x6509837B);
    a.state.borrow_mut().context.stage = 0;
    settle(&mut e, &a, &mut l, true).unwrap();
    assert!(!e.held && e.crowd.is_none());
    assert!(a.state.borrow().sources.iter().all(|r| r.front[8] == 1));
    assert_eq!(a.state.borrow().window[1], 1);
    // And the next visit prepares again.
    selection(&a);
    settle(&mut e, &a, &mut l, true).unwrap();
    assert!(!e.held && removed(&a));
    assert_eq!(e.visits, 2);
}
#[test]
fn presentation_interrupted_quick_play_restore_rearms_only_after_fresh_selection_proof() {
    let all = Options {
        remove_drone: true,
        remove_dj: true,
        remove_mc: true,
        remove_crowd: true,
    };
    let (mut e, a, mut l, f) = fixture(all);
    settle(&mut e, &a, &mut l, true).unwrap();
    assert_eq!(e.visits, 1);

    menu(&a, 0x079964C0);
    // The global restore completes, then the game leaves Quick Play after the
    // first standing-crowd byte is published.
    f.fail_write.set(f.writes.get() + 2);
    f.transition_on_failed_write.set(true);
    settle(&mut e, &a, &mut l, true).unwrap();
    assert!(!e.held && e.restore_rearm_pending && e.crowd.is_some());
    let interrupted_writes = f.writes.get();
    assert!(
        e.status()["message"]
            .as_str()
            .unwrap()
            .contains("waiting for a stable Blacktop selection")
    );

    game_match_end_reset(&a);
    selection(&a);
    settle(&mut e, &a, &mut l, true).unwrap();
    assert!(!e.held && !e.restore_rearm_pending && e.prepared_selection);
    assert!(f.writes.get() > interrupted_writes);
    let s = a.state.borrow();
    assert_eq!(s.window[1], 0);
    assert!([6, 7, 8, 11].iter().all(|i| s.window[*i] == 1));
    assert!(s.sources.iter().all(|row| row.front[8] == 0));
}
#[test]
fn presentation_interrupted_restore_foreign_neighbor_holds_before_reentry_writes() {
    let (mut e, a, mut l, f) = fixture(Options {
        remove_crowd: true,
        remove_dj: true,
        ..Options::default()
    });
    settle(&mut e, &a, &mut l, true).unwrap();
    menu(&a, 0x079964C0);
    f.fail_write.set(f.writes.get() + 2);
    f.transition_on_failed_write.set(true);
    settle(&mut e, &a, &mut l, true).unwrap();
    assert!(e.restore_rearm_pending && !e.held);
    a.state.borrow_mut().window[4] ^= 1;
    game_match_end_reset(&a);
    selection(&a);
    let writes = f.writes.get();
    assert!(settle(&mut e, &a, &mut l, true).is_err());
    assert!(e.held);
    assert_eq!(f.writes.get(), writes);
}
#[test]
fn presentation_restore_failure_at_same_boundary_remains_a_hard_hold() {
    let (mut e, a, mut l, f) = fixture(Options {
        remove_crowd: true,
        ..Options::default()
    });
    settle(&mut e, &a, &mut l, true).unwrap();
    menu(&a, 0x079964C0);
    f.fail_write.set(f.writes.get() + 2);
    assert!(settle(&mut e, &a, &mut l, true).is_err());
    assert!(e.held && !e.restore_rearm_pending && e.crowd.is_some());
}
#[test]
fn presentation_foreign_window_neighbor_after_reset_still_holds() {
    let (mut e, a, mut l, f) = fixture(Options {
        remove_crowd: true,
        remove_mc: true,
        ..Options::default()
    });
    settle(&mut e, &a, &mut l, true).unwrap();
    menu(&a, 0xB4D37B22);
    e.tick(&a, &mut l, true).unwrap();
    game_match_end_reset(&a);
    a.state.borrow_mut().window[3] = 1;
    let writes = f.writes.get();
    menu(&a, 0x079964C0);
    assert!(settle(&mut e, &a, &mut l, true).is_err());
    assert!(e.held);
    assert_eq!(f.writes.get(), writes);
    // An actor byte that is not a boolean is foreign too.
    let (mut e, a, mut l, _f) = fixture(Options {
        remove_crowd: true,
        ..Options::default()
    });
    settle(&mut e, &a, &mut l, true).unwrap();
    a.state.borrow_mut().window[7] = 2;
    menu(&a, 0x079964C0);
    assert!(settle(&mut e, &a, &mut l, true).is_err());
}
