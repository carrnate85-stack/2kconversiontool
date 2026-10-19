use super::*;
use crate::blacktop_outfits::tests::game;
use crate::blacktop_outfits::{Composed, tests::Fixture};
use crate::outfit_journal::tests as fixture;
use std::cell::{Cell, RefCell};
const OWNER: &str = "abcdef0123456789abcdef0123456789abcdef0123456789";
struct Mock {
    boundary: Cell<Boundary>,
    alive: Cell<bool>,
    loading: Cell<bool>,
    ready_calls: Cell<usize>,
    mutate_at: Cell<Option<usize>>,
    players: RefCell<Vec<MatchPlayer>>,
    lineup: Vec<u64>,
}
impl Driver for Mock {
    fn check(&self) -> Result<()> {
        ensure!(self.alive.get(), "fixture game identity ended/changed");
        Ok(())
    }
    fn boundary(&self) -> Result<Boundary> {
        self.check()?;
        Ok(self.boundary.get())
    }
    fn lineup(&self) -> Result<Vec<u64>> {
        self.check()?;
        Ok(self.lineup.clone())
    }
    fn ready(&self, choices: &[Choice]) -> Result<Vec<MatchPlayer>> {
        self.check()?;
        ensure!(!self.loading.get(), "fixture managers still loading");
        let n = self.ready_calls.get() + 1;
        self.ready_calls.set(n);
        if self.mutate_at.get() == Some(n) {
            self.players.borrow_mut()[0].description += 0x10;
        }
        ensure!(
            choices.iter().all(|c| self.lineup.contains(&c.roster)),
            "fixture origin absent"
        );
        Ok(self.players.borrow().clone())
    }
}
fn transport(root: &Path, number: u32, ext: &str) -> PathBuf {
    root.join("Logs/game-session").join(format!(
        "nba2k27_saved_outfits_{}{}.{ext}",
        game().pid,
        if number > 1 {
            format!("_match{number}")
        } else {
            String::new()
        }
    ))
}
fn report(root: &Path, number: u32, state: &str, count: usize) {
    fs::write(transport(root,number,"log"),format!("SCHEMA=blacktop-saved-outfits-v1\r\nPID={}\r\nSTATE={state}\r\nREASON=Runtime fixture\r\nPLAYERS_REBUILT={count}\r\nGAME_CODE_HOOKS=0\r\nOBJECT_HOOKS=0\r\n",game().pid)).unwrap();
}
fn setup() -> (PathBuf, Store, Fixture, Composed, Mock, Coordinator) {
    let root = fixture::root();
    drop(loader_files::prepare_reports(&root).unwrap());
    let (mut memory, p, c) = fixture::fixture();
    let mut s = Store::create(&root, OWNER, &game()).unwrap();
    fixture::committed(&mut s, &mut memory, &p, &c);
    let mock = Mock {
        boundary: Cell::new(Boundary::Match),
        alive: Cell::new(true),
        loading: Cell::new(false),
        ready_calls: Cell::new(0),
        mutate_at: Cell::new(None),
        lineup: vec![p.key.roster],
        players: RefCell::new(vec![MatchPlayer {
            roster: p.key.roster,
            record: 0x300000000,
            roster_copy: 0x400000000,
            manager: 0x500000000,
            player: 0x600000000,
            description: 0x600000f20,
            array: 0x600001000,
            items: ["stock_top".into(), "stock_bottom".into()],
        }]),
    };
    report(&root, 1, "ARMED", 0);
    (root, s, memory, c, mock, Coordinator::default())
}
fn fresh_choice(s: &mut Store, m: &Fixture, c: &Composed) {
    let p = outfits::capture(m, &game(), s.pages())
        .unwrap()
        .players
        .remove(0);
    let r = s.reserve(&p, c).unwrap();
    let ready = s.existing(r).unwrap();
    let id = ready.intent().id.clone();
    drop(ready);
    s.verified(&id).unwrap();
}
fn submitted(root: &Path, s: &mut Store, d: &Mock, c: &mut Coordinator) {
    assert_eq!(c.tick(d, s, 0).unwrap(), Status::WaitingForMatch);
    assert_eq!(c.tick(d, s, 250).unwrap(), Status::WaitingForMatch);
    assert_eq!(
        c.tick(d, s, 500).unwrap(),
        Status::Applying {
            number: 1,
            rebuilt: 0
        }
    );
    assert!(transport(root, 1, "trigger").is_file());
    assert_eq!(s.ledger().consumed.len(), 1);
}
fn rebuilt(root: &Path, s: &mut Store, d: &Mock, c: &mut Coordinator, now: u64) {
    fs::remove_file(transport(root, 1, "trigger")).unwrap();
    report(root, 1, "REBUILT", 1);
    assert_eq!(
        c.tick(d, s, now).unwrap(),
        Status::Applied {
            number: 1,
            players: 1
        }
    );
    assert!(!c.busy());
}
#[test]
fn carry_timing_separates_loading_stability_and_durable_publication_without_early_trigger() {
    let (root, mut store, _memory, _composed, driver, mut coordinator) = setup();
    driver.loading.set(true);
    assert!(matches!(
        coordinator.tick(&driver, &mut store, 0).unwrap(),
        Status::NotReady { .. }
    ));
    driver.boundary.set(Boundary::Other);
    assert_eq!(
        coordinator.tick(&driver, &mut store, 250).unwrap(),
        Status::WaitingForMatch
    );
    driver.loading.set(false);
    driver.boundary.set(Boundary::Match);
    for now in [500, 750] {
        assert_eq!(
            coordinator.tick(&driver, &mut store, now).unwrap(),
            Status::WaitingForMatch
        );
        assert!(!transport(&root, 1, "trigger").exists());
    }
    assert_eq!(
        coordinator.tick(&driver, &mut store, 1000).unwrap(),
        Status::Applying {
            number: 1,
            rebuilt: 0
        }
    );
    let timing = coordinator.timing();
    assert!(timing.first_match_ms.is_some());
    assert!(timing.first_ready_ms.unwrap() >= 500);
    assert!(timing.stable_admission_ms.unwrap() >= 1000);
    assert_eq!(timing.ready_samples, 3);
    assert_eq!(timing.readiness_failures, 1);
    assert_eq!(timing.nonmatch_samples, 1);
    assert_eq!(timing.deferred_publications, 0);
    assert!(timing.freeze_begin_ms >= timing.stable_admission_ms);
    assert!(timing.freeze_end_ms >= timing.freeze_begin_ms);
    assert!(timing.publish_begin_ms >= timing.freeze_end_ms);
    assert!(timing.publish_end_ms >= timing.publish_begin_ms);
    assert!(timing.submitted_unix_ms.unwrap() > 0);
    assert!(transport(&root, 1, "trigger").exists());
    rebuilt(&root, &mut store, &driver, &mut coordinator, 1250);
    driver.boundary.set(Boundary::Selection);
    coordinator.tick(&driver, &mut store, 1500).unwrap();
    assert_eq!(coordinator.timing(), &Timing::default());
    coordinator.close();
    store.finish(|| Ok(())).unwrap();
}
#[test]
fn stable_match_submits_once_and_native_success_does_not_replay_on_more_match_ticks() {
    let (root, mut s, _, _, d, mut c) = setup();
    assert_eq!(c.tick(&d, &mut s, 0).unwrap(), Status::WaitingForMatch);
    for now in [1, 100, 249] {
        assert_eq!(c.tick(&d, &mut s, now).unwrap(), Status::WaitingForMatch);
    }
    assert!(!transport(&root, 1, "trigger").exists());
    assert!(s.ledger().consumed.is_empty());
    c.tick(&d, &mut s, 250).unwrap();
    assert!(matches!(
        c.tick(&d, &mut s, 500).unwrap(),
        Status::Applying { .. }
    ));
    assert_eq!(
        c.tick(&d, &mut s, 550).unwrap(),
        Status::Applying {
            number: 1,
            rebuilt: 0
        }
    );
    rebuilt(&root, &mut s, &d, &mut c, 600);
    for now in [1000, 2000, 3000] {
        assert_eq!(
            c.tick(&d, &mut s, now).unwrap(),
            Status::Rearming { number: 2 }
        );
    }
    assert!(!transport(&root, 2, "trigger").exists());
    assert_eq!(s.ledger().consumed.len(), 1);
    c.close();
    s.finish(|| Ok(())).unwrap();
}
#[test]
fn quick_play_or_main_menu_keeps_match_number_and_requires_four_spaced_selection_samples_and_next_armed_report()
 {
    let (root, mut s, memory, outfit, d, mut c) = setup();
    submitted(&root, &mut s, &d, &mut c);
    rebuilt(&root, &mut s, &d, &mut c, 600);
    d.boundary.set(Boundary::Menu);
    assert_eq!(
        c.tick(&d, &mut s, 700).unwrap(),
        Status::Rearming { number: 2 }
    );
    assert!(s.ledger().choices.is_empty());
    fresh_choice(&mut s, &memory, &outfit);
    d.boundary.set(Boundary::Selection);
    for now in [1000, 1100, 1500, 2000, 2500] {
        assert_eq!(
            c.tick(&d, &mut s, now).unwrap(),
            Status::Rearming { number: 2 }
        );
    }
    report(&root, 2, "ARMED", 0);
    assert_eq!(c.tick(&d, &mut s, 2501).unwrap(), Status::Selecting);
    d.boundary.set(Boundary::Match);
    c.tick(&d, &mut s, 3000).unwrap();
    c.tick(&d, &mut s, 3250).unwrap();
    assert_eq!(
        c.tick(&d, &mut s, 3500).unwrap(),
        Status::Applying {
            number: 2,
            rebuilt: 0
        }
    );
    assert!(transport(&root, 2, "trigger").exists());
    assert_eq!(s.ledger().consumed.len(), 2);
    c.close();
    s.finish(|| Ok(())).unwrap();
}
#[test]
fn interrupted_selection_samples_do_not_count_as_rearmed_even_if_a_next_report_exists() {
    let (root, mut s, _, _, d, mut c) = setup();
    submitted(&root, &mut s, &d, &mut c);
    rebuilt(&root, &mut s, &d, &mut c, 600);
    report(&root, 2, "ARMED", 0);
    d.boundary.set(Boundary::Selection);
    c.tick(&d, &mut s, 1000).unwrap();
    c.tick(&d, &mut s, 1500).unwrap();
    d.boundary.set(Boundary::Other);
    c.tick(&d, &mut s, 2000).unwrap();
    d.boundary.set(Boundary::Selection);
    for now in [2500, 3000, 3500] {
        assert_eq!(
            c.tick(&d, &mut s, now).unwrap(),
            Status::Rearming { number: 2 }
        );
    }
    assert_eq!(c.tick(&d, &mut s, 4000).unwrap(), Status::Selecting);
    c.close();
    s.finish(|| Ok(())).unwrap();
}
#[test]
fn loading_resets_stability_and_a_preconsumption_match_change_is_retryable() {
    let (root, mut s, _, _, d, mut c) = setup();
    c.tick(&d, &mut s, 0).unwrap();
    d.loading.set(true);
    // A refused carry source is reported with its reason, not a silent wait;
    // it is never held and nothing is consumed or written.
    assert_eq!(
        c.tick(&d, &mut s, 250).unwrap(),
        Status::NotReady {
            reason: "fixture managers still loading".into()
        }
    );
    assert!(c.held().is_none());
    assert!(s.ledger().consumed.is_empty());
    d.loading.set(false);
    c.tick(&d, &mut s, 500).unwrap();
    c.tick(&d, &mut s, 750).unwrap();
    assert!(!transport(&root, 1, "trigger").exists());
    d.mutate_at.set(Some(d.ready_calls.get() + 2));
    assert_eq!(c.tick(&d, &mut s, 1000).unwrap(), Status::WaitingForMatch);
    assert!(c.held().is_none());
    assert!(s.ledger().consumed.is_empty());
    c.tick(&d, &mut s, 1250).unwrap();
    c.tick(&d, &mut s, 1500).unwrap();
    assert!(matches!(
        c.tick(&d, &mut s, 1750).unwrap(),
        Status::Applying { .. }
    ));
    c.close();
    s.finish(|| Ok(())).unwrap();
}
#[test]
fn match_change_after_consumption_holds_without_trigger_or_later_replay() {
    let (root, mut s, _, _, d, mut c) = setup();
    d.mutate_at.set(Some(7));
    c.tick(&d, &mut s, 0).unwrap();
    c.tick(&d, &mut s, 250).unwrap();
    assert!(c.tick(&d, &mut s, 500).is_err());
    assert!(c.held().is_some());
    assert_eq!(s.ledger().consumed.len(), 1);
    assert!(!transport(&root, 1, "trigger").exists());
    assert!(c.tick(&d, &mut s, 1000).is_err());
    c.close();
    s.finish(|| Ok(())).unwrap();
}
#[test]
fn unarmed_worker_waits_but_confirmation_timeout_owner_change_and_clock_reversal_stop_without_retry()
 {
    for mode in [0, 1, 2, 3] {
        let (root, mut s, _, _, d, mut c) = setup();
        if mode == 0 {
            fs::remove_file(transport(&root, 1, "log")).unwrap();
            c.tick(&d, &mut s, 0).unwrap();
            c.tick(&d, &mut s, 250).unwrap();
            assert_eq!(
                c.tick(&d, &mut s, 500).unwrap(),
                Status::WaitingForWorker { number: 1 }
            );
            // Nothing was written: an unarmed worker is a visible wait, not a hold.
            assert!(matches!(
                c.tick(&d, &mut s, 10501).unwrap(),
                Status::NotReady { .. }
            ));
            assert!(c.held().is_none());
            assert!(s.ledger().consumed.is_empty());
        } else {
            submitted(&root, &mut s, &d, &mut c);
            match mode {
                1 => {
                    assert!(c.tick(&d, &mut s, 650501).is_err());
                }
                2 => {
                    d.alive.set(false);
                    assert!(c.tick(&d, &mut s, 600).is_err());
                }
                _ => {
                    assert!(c.tick(&d, &mut s, 499).is_err());
                }
            }
        }
        if mode != 0 {
            assert!(c.held().is_some());
            assert!(!c.busy());
            assert!(c.tick(&d, &mut s, 700000).is_err());
        }
        c.close();
        s.finish(|| Ok(())).unwrap();
    }
}
#[test]
fn an_unarmed_worker_is_a_visible_retryable_wait_not_a_hold() {
    // 0.51 live: "Native outfit worker did not arm within ten seconds" held the
    // session for the whole game although nothing had been written.
    let (root, mut s, _, _, d, mut c) = setup();
    fs::remove_file(transport(&root, 1, "log")).unwrap();
    // Stable readiness (0/250/500) then the ARMED wait.
    for now in [0, 250, 500, 750] {
        let status = c.tick(&d, &mut s, now).unwrap();
        assert!(
            matches!(
                status,
                Status::WaitingForMatch | Status::WaitingForWorker { .. }
            ),
            "{status:?}"
        );
    }
    match c.tick(&d, &mut s, 11_000).unwrap() {
        Status::NotReady { reason } => assert!(reason.contains("has not armed"), "{reason}"),
        other => panic!("{other:?}"),
    }
    assert!(c.held().is_none());
    assert!(s.ledger().consumed.is_empty());
    assert!(!transport(&root, 1, "trigger").exists());
    // The worker arms late: carry proceeds normally.
    report(&root, 1, "ARMED", 0);
    let mut submitted = false;
    for now in [11_250, 11_500, 11_750, 12_000] {
        if matches!(c.tick(&d, &mut s, now).unwrap(), Status::Applying { .. }) {
            submitted = true;
            break;
        }
    }
    assert!(submitted, "late ARMED did not submit");
    assert_eq!(s.ledger().consumed.len(), 1);
}

#[test]
fn initial_default_sample_preserves_legacy_order_and_rearm_never_chooses() {
    struct Ordered {
        calls: RefCell<Vec<&'static str>>,
    }
    impl Driver for Ordered {
        fn check(&self) -> Result<()> {
            self.calls.borrow_mut().push("check");
            Ok(())
        }
        fn boundary(&self) -> Result<Boundary> {
            self.calls.borrow_mut().push("boundary");
            Ok(Boundary::Match)
        }
        fn lineup(&self) -> Result<Vec<u64>> {
            self.calls.borrow_mut().push("lineup");
            Ok(vec![1])
        }
        fn ready(&self, _: &[Choice]) -> Result<Vec<MatchPlayer>> {
            self.calls.borrow_mut().push("ready");
            anyhow::bail!("fixture loading")
        }
    }
    let d = Ordered {
        calls: RefCell::new(vec![]),
    };
    let sample = d
        .sample(true, |_| {
            d.calls.borrow_mut().push("choose");
            Ok(vec![Choice {
                choice_id: "a".repeat(48),
                roster: 1,
                slot: 0,
                name: "fixture".into(),
                items: vec!["top".into(), "bottom".into(), "sock".into()],
            }])
        })
        .unwrap();
    assert!(matches!(sample.readiness, Some(Readiness::NotReady(_))));
    assert_eq!(
        &*d.calls.borrow(),
        &["check", "boundary", "lineup", "choose", "ready"]
    );
    d.calls.borrow_mut().clear();
    assert!(
        d.sample(false, |_| panic!("rearm cannot choose"))
            .unwrap()
            .readiness
            .is_none()
    );
    assert_eq!(&*d.calls.borrow(), &["check", "boundary"]);
    // Existing coordinator still requires 3 identical samples and >=500ms.
    let (root, mut store, _, _, driver, mut coordinator) = setup();
    for now in [0, 100, 249, 250, 499] {
        assert_eq!(
            coordinator.tick(&driver, &mut store, now).unwrap(),
            Status::WaitingForMatch
        );
        assert!(!transport(&root, 1, "trigger").exists());
    }
    assert!(matches!(
        coordinator.tick(&driver, &mut store, 500).unwrap(),
        Status::Applying { .. }
    ));
}
