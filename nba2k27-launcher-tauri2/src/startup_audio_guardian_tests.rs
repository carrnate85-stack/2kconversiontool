use super::*;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
};

fn game() -> GameKey {
    GameKey {
        pid: 55,
        birth: 987654321,
    }
}
fn row(endpoint: &str, instance: &str, level: f32, muted: bool, state: SessionState) -> Sample {
    Sample {
        key: SessionKey {
            endpoint: endpoint.into(),
            instance: instance.into(),
        },
        pid: 55,
        state,
        volume: Volume { level, muted },
    }
}
struct FakeSession {
    state: Rc<RefCell<FakeSessionState>>,
    apartment_gone: Rc<Cell<bool>>,
    released_after_apartment: Rc<Cell<bool>>,
}
impl Drop for FakeSession {
    fn drop(&mut self) {
        if self.apartment_gone.get() {
            self.released_after_apartment.set(true);
        }
    }
}
struct FakeSessionState {
    sample: Sample,
    read_errors: usize,
    outcomes: VecDeque<WriteOutcome>,
    uncertain_applies: bool,
    writes: Vec<bool>,
}
#[derive(Clone, Copy)]
enum Presence {
    Alive,
    Exited,
    Unknown,
}
struct FakeAudio {
    presence: Rc<Cell<Presence>>,
    rows: Vec<Rc<RefCell<FakeSessionState>>>,
    apartment_gone: Rc<Cell<bool>>,
    released_after_apartment: Rc<Cell<bool>>,
    enumeration_error: bool,
}
impl FakeAudio {
    fn new(samples: Vec<Sample>) -> Self {
        Self {
            presence: Rc::new(Cell::new(Presence::Alive)),
            rows: samples
                .into_iter()
                .map(|sample| {
                    Rc::new(RefCell::new(FakeSessionState {
                        sample,
                        read_errors: 0,
                        outcomes: VecDeque::new(),
                        uncertain_applies: true,
                        writes: Vec::new(),
                    }))
                })
                .collect(),
            apartment_gone: Rc::new(Cell::new(false)),
            released_after_apartment: Rc::new(Cell::new(false)),
            enumeration_error: false,
        }
    }
    fn one() -> Self {
        Self::new(vec![row(
            "speaker",
            "game-session",
            0.6,
            false,
            SessionState::Active,
        )])
    }
}
impl Drop for FakeAudio {
    fn drop(&mut self) {
        self.apartment_gone.set(true);
    }
}
impl AudioApi for FakeAudio {
    type Session = FakeSession;
    fn game_alive(&mut self) -> Result<bool> {
        match self.presence.get() {
            Presence::Alive => Ok(true),
            Presence::Exited => Ok(false),
            Presence::Unknown => anyhow::bail!("fake process query denied"),
        }
    }
    fn sessions(&mut self) -> Result<Vec<FakeSession>> {
        ensure!(!self.enumeration_error, "fake enumeration failure");
        Ok(self
            .rows
            .iter()
            .map(|state| FakeSession {
                state: state.clone(),
                apartment_gone: self.apartment_gone.clone(),
                released_after_apartment: self.released_after_apartment.clone(),
            })
            .collect())
    }
    fn inspect(&mut self, session: &FakeSession) -> Result<Sample> {
        let mut state = session.state.borrow_mut();
        if state.read_errors > 0 {
            state.read_errors -= 1;
            anyhow::bail!("fake readback failure");
        }
        Ok(state.sample.clone())
    }
    fn set_mute(
        &mut self,
        session: &FakeSession,
        key: &SessionKey,
        expected: bool,
        value: bool,
    ) -> WriteOutcome {
        if !matches!(self.presence.get(), Presence::Alive) {
            return WriteOutcome::BeforeWriteError("exact game unavailable".into());
        }
        let mut state = session.state.borrow_mut();
        if state.sample.key != *key || state.sample.pid != game().pid {
            return WriteOutcome::BeforeWriteError("ownership changed".into());
        }
        if state.sample.volume.muted != expected {
            return WriteOutcome::CurrentChanged(state.sample.volume.muted);
        }
        let outcome = state.outcomes.pop_front().unwrap_or(WriteOutcome::Applied);
        match &outcome {
            WriteOutcome::Applied => {
                state.writes.push(value);
                state.sample.volume.muted = value;
            }
            WriteOutcome::MayHaveApplied(_) => {
                state.writes.push(value);
                if state.uncertain_applies {
                    state.sample.volume.muted = value;
                }
            }
            WriteOutcome::CurrentChanged(current) => state.sample.volume.muted = *current,
            WriteOutcome::BeforeWriteError(_) => {}
        }
        outcome
    }
}

#[test]
fn duck_follows_game_mixer_without_mirroring_our_mute_and_restores_once() {
    let api = FakeAudio::one();
    let state = api.rows[0].clone();
    let mut guardian = Guardian::new(api, game(), true, 0).unwrap();
    let report = guardian.step(0);
    assert_eq!(report.phase, Phase::Watching);
    assert_eq!(
        report.volume,
        Some(Volume {
            level: 0.6,
            muted: false
        })
    );
    assert!(state.borrow().sample.volume.muted);
    assert_eq!(state.borrow().writes, [true]);
    state.borrow_mut().sample.volume.level = 0.22;
    assert_eq!(guardian.step(200).volume.unwrap().level, 0.22);
    assert_eq!(state.borrow().writes, [true]);
    guardian.request_stop();
    let report = guardian.step(300);
    assert_eq!(report.phase, Phase::Finished);
    assert_eq!(report.pending, 0);
    assert!(!state.borrow().sample.volume.muted);
    assert_eq!(state.borrow().writes, [true, false]);
    guardian.step(400);
    assert_eq!(state.borrow().writes, [true, false]);
}

#[test]
fn originally_muted_and_watch_only_sessions_are_never_changed() {
    for (duck, muted) in [(true, true), (false, false), (false, true)] {
        let api = FakeAudio::new(vec![row(
            "speaker",
            "instance",
            0.7,
            muted,
            SessionState::Active,
        )]);
        let state = api.rows[0].clone();
        let mut guardian = Guardian::new(api, game(), duck, 0).unwrap();
        assert_eq!(guardian.step(0).volume.unwrap().muted, muted);
        guardian.request_stop();
        assert_eq!(guardian.step(1).phase, Phase::Finished);
        assert!(state.borrow().writes.is_empty());
        assert_eq!(state.borrow().sample.volume.muted, muted);
    }
}

#[test]
fn active_sessions_prefer_quieter_level_and_original_mute_across_outputs() {
    let api = FakeAudio::new(vec![
        row("a", "inactive", 0.01, false, SessionState::Inactive),
        row("b", "active1", 0.7, false, SessionState::Active),
        row("c", "active2", 0.4, true, SessionState::Active),
    ]);
    let states = api.rows.clone();
    let mut guardian = Guardian::new(api, game(), true, 0).unwrap();
    assert_eq!(
        guardian.step(0).volume,
        Some(Volume {
            level: 0.4,
            muted: true
        })
    );
    guardian.request_stop();
    assert_eq!(guardian.step(1).phase, Phase::Finished);
    for state in states {
        assert_eq!(
            state.borrow().sample.volume.level,
            if state.borrow().sample.key.endpoint == "a" {
                0.01
            } else if state.borrow().sample.key.endpoint == "b" {
                0.7
            } else {
                0.4
            }
        );
    }
}

#[test]
fn user_unmute_is_not_remuted_or_overwritten_by_cleanup() {
    let api = FakeAudio::one();
    let state = api.rows[0].clone();
    let mut guardian = Guardian::new(api, game(), true, 0).unwrap();
    guardian.step(0);
    state.borrow_mut().sample.volume.muted = false;
    assert_eq!(guardian.step(200).volume.unwrap().muted, false);
    // Even a later user mute is theirs, not adopted as our old mutation.
    state.borrow_mut().sample.volume.muted = true;
    guardian.step(400);
    guardian.request_stop();
    assert_eq!(guardian.step(500).phase, Phase::Finished);
    assert!(state.borrow().sample.volume.muted);
    assert_eq!(state.borrow().writes, [true]);
}

#[test]
fn errors_before_setmute_do_not_adopt_a_user_value_but_uncertain_writes_are_restored() {
    for mode in 0..3 {
        let api = FakeAudio::one();
        let state = api.rows[0].clone();
        state.borrow_mut().outcomes.push_back(match mode {
            0 => WriteOutcome::BeforeWriteError("guard refused".into()),
            1 => WriteOutcome::CurrentChanged(true),
            _ => WriteOutcome::MayHaveApplied("failed HRESULT after effect".into()),
        });
        let mut guardian = Guardian::new(api, game(), true, 0).unwrap();
        let report = guardian.step(0);
        assert_eq!(report.phase, Phase::Finished);
        assert!(report.error.is_some());
        if mode == 0 {
            assert!(!state.borrow().sample.volume.muted);
            assert!(state.borrow().writes.is_empty());
        }
        if mode == 1 {
            assert!(state.borrow().sample.volume.muted);
            assert!(state.borrow().writes.is_empty());
        }
        if mode == 2 {
            assert!(!state.borrow().sample.volume.muted);
            assert_eq!(state.borrow().writes, [true, false]);
        }
    }
}

#[test]
fn uncertain_restore_stays_owned_until_later_readback_and_never_reducks() {
    let api = FakeAudio::one();
    let state = api.rows[0].clone();
    let mut guardian = Guardian::new(api, game(), true, 0).unwrap();
    guardian.step(0);
    state
        .borrow_mut()
        .outcomes
        .push_back(WriteOutcome::MayHaveApplied(
            "restore readback denied".into(),
        ));
    guardian.request_stop();
    let report = guardian.step(100);
    assert_eq!(report.phase, Phase::RestorationPending);
    assert_eq!(report.pending, 1);
    assert!(!state.borrow().sample.volume.muted);
    assert_eq!(guardian.step(200).phase, Phase::Finished);
    assert_eq!(state.borrow().writes, [true, false]);
}

#[test]
fn all_owned_sessions_are_attempted_even_when_one_restore_fails() {
    let api = FakeAudio::new(vec![
        row("a", "one", 0.5, false, SessionState::Active),
        row("b", "two", 0.6, false, SessionState::Active),
    ]);
    let one = api.rows[0].clone();
    let two = api.rows[1].clone();
    let mut guardian = Guardian::new(api, game(), true, 0).unwrap();
    guardian.step(0);
    one.borrow_mut().read_errors = 3;
    guardian.request_stop();
    assert_eq!(guardian.step(100).phase, Phase::RestorationPending);
    assert_eq!(two.borrow().writes, [true, false]);
    assert_eq!(guardian.step(200).phase, Phase::RestorationPending);
    assert_eq!(guardian.step(300).phase, Phase::RestorationPending);
    assert_eq!(guardian.step(400).phase, Phase::Finished);
    assert_eq!(one.borrow().writes, [true, false]);
}

#[test]
fn changed_session_or_process_ownership_holds_without_writing_any_replacement() {
    let api = FakeAudio::one();
    let state = api.rows[0].clone();
    let presence = api.presence.clone();
    let mut guardian = Guardian::new(api, game(), true, 0).unwrap();
    guardian.step(0);
    state.borrow_mut().sample.key.instance = "replacement".into();
    guardian.request_stop();
    assert_eq!(guardian.step(100).phase, Phase::RestorationPending);
    assert_eq!(state.borrow().writes, [true]);
    state.borrow_mut().sample.key.instance = "game-session".into();
    presence.set(Presence::Unknown);
    assert_eq!(guardian.step(200).phase, Phase::RestorationPending);
    assert_eq!(state.borrow().writes, [true]);
    presence.set(Presence::Alive);
    assert_eq!(guardian.step(300).phase, Phase::Finished);
    assert_eq!(state.borrow().writes, [true, false]);
}

#[test]
fn exited_game_or_expired_session_requires_no_write_to_a_new_game() {
    for exited in [false, true] {
        let api = FakeAudio::one();
        let state = api.rows[0].clone();
        let presence = api.presence.clone();
        let mut guardian = Guardian::new(api, game(), true, 0).unwrap();
        guardian.step(0);
        if exited {
            presence.set(Presence::Exited);
        } else {
            state.borrow_mut().sample.state = SessionState::Expired;
        }
        guardian.request_stop();
        assert_eq!(guardian.step(100).phase, Phase::Finished);
        assert_eq!(state.borrow().writes, [true]);
    }
}

#[test]
fn invalid_duplicate_or_unbounded_discovery_is_rejected_before_the_first_new_mute() {
    for mode in 0..5 {
        let first = row("speaker", "instance", 0.7, false, SessionState::Active);
        let mut second = row("other", "two", 0.5, false, SessionState::Active);
        if mode == 0 {
            second.pid = 999;
        }
        if mode == 1 {
            second.key = first.key.clone();
        }
        if mode == 2 {
            second.volume.level = f32::NAN;
        }
        if mode == 3 {
            second.key.instance = String::new();
        }
        let api = FakeAudio::new(if mode == 4 {
            (0..65)
                .map(|i| row("speaker", &format!("{i}"), 0.5, false, SessionState::Active))
                .collect()
        } else {
            vec![first, second]
        });
        let states = api.rows.clone();
        let mut guardian = Guardian::new(api, game(), true, 0).unwrap();
        let report = guardian.step(0);
        assert_eq!(report.phase, Phase::Finished);
        assert!(report.error.is_some());
        assert!(states.iter().all(|s| s.borrow().writes.is_empty()));
    }
}

struct FakeDriver {
    now: u64,
    turn: usize,
    eof_at: usize,
    broken_at: Option<usize>,
    reports: Vec<Report>,
    recover: Option<Rc<RefCell<FakeSessionState>>>,
}
impl Driver for FakeDriver {
    fn now_ms(&self) -> u64 {
        self.now
    }
    fn stop_requested(&mut self) -> bool {
        self.turn >= self.eof_at
    }
    fn emit(&mut self, report: &Report) -> Result<()> {
        self.reports.push(report.clone());
        ensure!(self.broken_at != Some(self.turn), "UI output pipe closed");
        Ok(())
    }
    fn pause(&mut self) {
        self.now += 100;
        self.turn += 1;
        assert!(
            self.turn < 20,
            "guardian driver unexpectedly never finished"
        );
        if self.turn == 4 {
            if let Some(state) = &self.recover {
                state.borrow_mut().read_errors = 0;
            }
        }
    }
}
#[test]
fn parent_eof_or_broken_ui_pipe_does_not_end_guardian_before_owned_restoration() {
    for broken in [false, true] {
        let api = FakeAudio::one();
        let state = api.rows[0].clone();
        let mut guardian = Guardian::new(api, game(), true, 0).unwrap();
        guardian.step(0);
        state.borrow_mut().read_errors = 10;
        let mut driver = FakeDriver {
            now: 100,
            turn: 0,
            eof_at: if broken { 100 } else { 0 },
            broken_at: if broken { Some(0) } else { None },
            reports: Vec::new(),
            recover: Some(state.clone()),
        };
        drive(&mut guardian, &mut driver);
        assert!(
            driver
                .reports
                .iter()
                .any(|r| r.phase == Phase::RestorationPending)
        );
        assert_eq!(driver.reports.last().unwrap().phase, Phase::Finished);
        assert!(driver.turn >= 4);
        assert!(!state.borrow().sample.volume.muted);
        assert_eq!(state.borrow().writes, [true, false]);
    }
}

#[test]
fn timeout_backwards_clock_and_drop_restore_without_extending_the_game_session() {
    for backwards in [false, true] {
        let api = FakeAudio::one();
        let state = api.rows[0].clone();
        let mut guardian = Guardian::new(api, game(), true, 10).unwrap();
        guardian.step(10);
        assert_eq!(
            guardian
                .step(if backwards {
                    9
                } else {
                    PLAYBACK_TIMEOUT_MS + 10
                })
                .phase,
            Phase::Finished
        );
        assert_eq!(state.borrow().writes, [true, false]);
    }
    let api = FakeAudio::one();
    let state = api.rows[0].clone();
    let apartment_gone = api.apartment_gone.clone();
    let late = api.released_after_apartment.clone();
    let mut guardian = Guardian::new(api, game(), true, 0).unwrap();
    guardian.step(0);
    drop(guardian);
    assert_eq!(state.borrow().writes, [true, false]);
    assert!(apartment_gone.get());
    assert!(!late.get());
}

#[test]
fn stdio_eof_is_stop_and_broken_stdout_does_not_panic_or_retry_writes() {
    let (tx, rx) = mpsc::sync_channel(1);
    drop(tx);
    let mut driver = StdioDriver {
        clock: Instant::now(),
        stop: rx,
        output: Vec::<u8>::new(),
        broken_output: false,
    };
    assert!(driver.stop_requested());
    driver
        .emit(&Report {
            phase: Phase::Finished,
            volume: None,
            pending: 0,
            error: None,
        })
        .unwrap();
    let text = String::from_utf8(driver.output).unwrap();
    assert!(text.contains("audio-restored"));
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "gone"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let (_tx, rx) = mpsc::sync_channel(1);
    let mut driver = StdioDriver {
        clock: Instant::now(),
        stop: rx,
        output: Broken,
        broken_output: false,
    };
    let report = Report {
        phase: Phase::RestorationPending,
        volume: None,
        pending: 1,
        error: None,
    };
    assert!(driver.emit(&report).is_err());
    assert!(driver.broken_output);
    driver.emit(&report).unwrap();
}

#[test]
fn saturated_diagnostic_pipe_cannot_block_restoration_or_grow_without_bound() {
    let (sender, receiver) = mpsc::sync_channel(OUTPUT_QUEUE);
    let mut output = QueuedOutput {
        sender,
        draft: Vec::new(),
    };
    for _ in 0..100 {
        output.write_all(b"a bounded status\n").unwrap();
        output.flush().unwrap();
        assert!(output.draft.is_empty());
    }
    assert_eq!(receiver.try_iter().count(), OUTPUT_QUEUE);
    assert!(output.write_all(&vec![1; OUTPUT_LIMIT + 1]).is_err());
    drop(receiver);
    output.write_all(b"last status").unwrap();
    assert_eq!(
        output.flush().unwrap_err().kind(),
        std::io::ErrorKind::BrokenPipe
    );
}

#[test]
fn no_owned_mutation_never_requires_waiting_for_an_unavailable_process() {
    let api = FakeAudio::one();
    api.presence.set(Presence::Unknown);
    let state = api.rows[0].clone();
    let mut guardian = Guardian::new(api, game(), true, 0).unwrap();
    let report = guardian.step(0);
    assert_eq!(report.phase, Phase::Finished);
    assert_eq!(report.pending, 0);
    assert!(report.error.is_some() && state.borrow().writes.is_empty());
}
