use super::*;
use std::{
    cell::Cell,
    collections::{BTreeMap, VecDeque},
    rc::Rc,
};

fn expected() -> ExpectedGame {
    ExpectedGame::new(
        GameKey {
            pid: 501,
            birth: 0x12345678,
        },
        Path::new(r"C:\Games\NBA 2K27\NBA2K27.exe"),
    )
    .unwrap()
}
fn sample(hwnd: u64) -> WindowSample {
    WindowSample {
        hwnd,
        pid: 501,
        thread: 601,
        visible: true,
        minimized: false,
        root: hwnd,
        owner: 0,
        client: [0, 0, 1920, 1080],
        monitor: Bounds {
            x: -1920,
            y: 0,
            width: 1920,
            height: 1080,
        },
    }
}
struct FakeProcess(Rc<Cell<u32>>);
impl Drop for FakeProcess {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
struct FakeWindows {
    identities: VecDeque<Option<ExpectedGame>>,
    handles: Vec<u64>,
    samples: BTreeMap<u64, VecDeque<Option<WindowSample>>>,
    clock: Cell<u64>,
    enum_cost: u64,
    window_cost: u64,
    reads: usize,
    process_reads: usize,
    retains: usize,
    drops: Rc<Cell<u32>>,
    fail_process: bool,
}
impl FakeWindows {
    fn new() -> Self {
        Self {
            identities: VecDeque::from([Some(expected())]),
            handles: vec![10],
            samples: BTreeMap::from([(10, VecDeque::from([Some(sample(10))]))]),
            clock: Cell::new(0),
            enum_cost: 0,
            window_cost: 0,
            reads: 0,
            process_reads: 0,
            retains: 0,
            drops: Rc::new(Cell::new(0)),
            fail_process: false,
        }
    }
}
impl WindowApi for FakeWindows {
    type Process = FakeProcess;
    fn retain(&mut self, pid: u32) -> Result<FakeProcess> {
        assert_eq!(pid, 501);
        self.retains += 1;
        Ok(FakeProcess(self.drops.clone()))
    }
    fn process(&mut self, _: &FakeProcess) -> Result<Option<ExpectedGame>> {
        self.process_reads += 1;
        ensure!(!self.fail_process, "fake query denied");
        if self.identities.len() > 1 {
            Ok(self.identities.pop_front().unwrap())
        } else {
            Ok(self.identities.front().unwrap().clone())
        }
    }
    fn now_ms(&self) -> u64 {
        self.clock.get()
    }
    fn enumerate(&mut self, max: usize, budget: u64) -> Result<Vec<u64>> {
        assert_eq!(max, MAX_WINDOWS);
        assert_eq!(budget, SCAN_BUDGET_MS);
        self.clock.set(self.clock.get() + self.enum_cost);
        Ok(self.handles.clone())
    }
    fn window(&mut self, hwnd: u64) -> Result<Option<WindowSample>> {
        self.reads += 1;
        self.clock.set(self.clock.get() + self.window_cost);
        let Some(rows) = self.samples.get_mut(&hwnd) else {
            return Ok(None);
        };
        if rows.len() > 1 {
            Ok(rows.pop_front().unwrap())
        } else {
            Ok(rows.front().cloned().flatten())
        }
    }
}

#[test]
fn invalid_or_unretained_game_identity_never_enumerates_a_window() {
    for key in [GameKey { pid: 0, birth: 1 }, GameKey { pid: 1, birth: 0 }] {
        assert!(ExpectedGame::new(key, &expected().exe).is_err());
    }
    for path in [
        r"relative\NBA2K27.exe",
        r"C:\Games\Other.exe",
        r"C:\Games\..\NBA2K27.exe",
        r"\\.\C:\NBA2K27.exe",
    ] {
        assert!(
            ExpectedGame::new(expected().key, Path::new(path)).is_err(),
            "{path}"
        );
    }
    for mode in 0..4 {
        let mut api = FakeWindows::new();
        let drops = api.drops.clone();
        if mode == 0 {
            api.identities[0].as_mut().unwrap().key.birth += 1;
        }
        if mode == 1 {
            api.identities[0].as_mut().unwrap().exe = PathBuf::from(r"C:\Other\NBA2K27.exe");
        }
        if mode == 2 {
            api.identities[0] = None;
        }
        if mode == 3 {
            api.fail_process = true;
        }
        assert!(OwnedGame::bind(api, expected()).is_err());
        assert_eq!(drops.get(), 1);
    }
}

#[test]
fn retained_handle_exact_executable_and_unique_window_feed_existing_stability_policy() {
    let mut api = FakeWindows::new();
    api.identities[0].as_mut().unwrap().exe = PathBuf::from(r"\\?\C:\GAMES\NBA 2K27\NBA2K27.EXE");
    let drops = api.drops.clone();
    let mut game = OwnedGame::bind(api, expected()).unwrap();
    let Observation::Window(Some(window)) = game.observe().unwrap() else {
        panic!("owned window not found")
    };
    assert_eq!(
        window.monitor,
        Bounds {
            x: -1920,
            y: 0,
            width: 1920,
            height: 1080
        }
    );
    assert_eq!((window.window.width, window.window.height), (1920, 1080));
    let mut coordinator = policy::Coordinator::default();
    let (token, _) = coordinator.start_game(expected().key, 0).unwrap();
    assert!(
        coordinator
            .handle(
                token,
                policy::Event::WindowObserved(Some(window.window)),
                10
            )
            .unwrap()
            .is_empty()
    );
    assert!(
        coordinator
            .handle(
                token,
                policy::Event::WindowObserved(Some(window.window)),
                259
            )
            .unwrap()
            .is_empty()
    );
    let effects = coordinator
        .handle(
            token,
            policy::Event::WindowObserved(Some(window.window)),
            260,
        )
        .unwrap();
    assert!(effects.iter().any(|e| matches!(
        e,
        policy::Effect::CreateHiddenFullscreen {
            monitor: policy::MonitorTarget::GameWindow(10),
            ..
        }
    )));
    assert_eq!(game.api.retains, 1); // no PID-based reacquisition
    drop(game);
    assert_eq!(drops.get(), 1);
}

#[test]
fn foreign_hidden_minimized_owned_dialog_child_splash_and_bad_bounds_are_never_ready() {
    for mode in 0..14 {
        let mut api = FakeWindows::new();
        let mut row = sample(10);
        match mode {
            0 => row.pid = 999,
            1 => row.visible = false,
            2 => row.minimized = true,
            3 => row.owner = 9,
            4 => row.root = 9,
            5 => row.thread = 0,
            6 => row.client[2] = 639,
            7 => row.client[3] = 359,
            8 => row.client = [i32::MIN, 0, i32::MAX, 1080],
            9 => row.client[2] = 16385,
            10 => row.monitor.width = 0,
            11 => row.monitor.x = i32::MAX,
            12 => row.hwnd = 11,
            13 => row.monitor.height = 32769,
            _ => unreachable!(),
        }
        api.samples.insert(10, VecDeque::from([Some(row)]));
        let mut game = OwnedGame::bind(api, expected()).unwrap();
        assert_eq!(
            game.observe().unwrap(),
            Observation::Window(None),
            "mode {mode}"
        );
    }
}

#[test]
fn ambiguous_windows_and_changed_hwnd_ownership_or_geometry_wait_without_choosing() {
    let mut api = FakeWindows::new();
    api.handles.push(20);
    api.samples.insert(20, VecDeque::from([Some(sample(20))]));
    assert_eq!(
        OwnedGame::bind(api, expected()).unwrap().observe().unwrap(),
        Observation::Window(None)
    );
    for mode in 0..4 {
        let mut api = FakeWindows::new();
        let mut after = sample(10);
        match mode {
            0 => after.pid = 999,
            1 => after.client[2] = 1280,
            2 => after.monitor.x = 0,
            3 => after.owner = 8,
            _ => unreachable!(),
        }
        api.samples
            .insert(10, VecDeque::from([Some(sample(10)), Some(after)]));
        let mut game = OwnedGame::bind(api, expected()).unwrap();
        assert_eq!(game.observe().unwrap(), Observation::Window(None));
    }
}

#[test]
fn exiting_or_reused_pid_cannot_become_a_new_video_window_owner() {
    let mut api = FakeWindows::new();
    api.identities = VecDeque::from([Some(expected()), Some(expected()), None]);
    let mut game = OwnedGame::bind(api, expected()).unwrap();
    assert_eq!(game.observe().unwrap(), Observation::GameEnded);
    assert_eq!(game.api.retains, 1);
    let mut api = FakeWindows::new();
    let mut changed = expected();
    changed.key.birth += 1;
    api.identities = VecDeque::from([Some(expected()), Some(expected()), Some(changed)]);
    let mut game = OwnedGame::bind(api, expected()).unwrap();
    assert!(game.observe().is_err());
    assert_eq!(game.api.retains, 1);
    let mut api = FakeWindows::new();
    api.identities = VecDeque::from([Some(expected()), None]);
    let mut game = OwnedGame::bind(api, expected()).unwrap();
    assert_eq!(game.observe().unwrap(), Observation::GameEnded);
    assert_eq!(game.api.reads, 0);
}

#[test]
fn window_number_and_cooperative_time_budgets_stop_unbounded_scans() {
    let mut api = FakeWindows::new();
    api.handles = vec![10; MAX_WINDOWS + 1];
    let mut game = OwnedGame::bind(api, expected()).unwrap();
    assert!(game.observe().is_err());
    assert_eq!(game.api.reads, 0);
    let mut api = FakeWindows::new();
    api.enum_cost = 101;
    let mut game = OwnedGame::bind(api, expected()).unwrap();
    assert!(game.observe().is_err());
    assert_eq!(game.api.reads, 0);
    let mut api = FakeWindows::new();
    api.window_cost = 51;
    let mut game = OwnedGame::bind(api, expected()).unwrap();
    assert!(game.observe().is_err());
    assert_eq!(game.api.reads, 2);
}

#[test]
fn placement_verification_requires_the_same_live_process_and_recaptured_hwnd() {
    let mut game = OwnedGame::bind(FakeWindows::new(), expected()).unwrap();
    assert!(game.verify_window(10).unwrap().is_some());
    assert!(game.verify_window(999).unwrap().is_none());
    let mut api = FakeWindows::new();
    api.identities = VecDeque::from([Some(expected()), None]);
    let mut game = OwnedGame::bind(api, expected()).unwrap();
    assert!(game.verify_window(10).unwrap().is_none());
    assert_eq!(game.api.reads, 0);
}

#[derive(Clone, Copy)]
enum PadAnswer {
    Buttons(u16),
    Disconnected,
    Error,
}
struct FakePads {
    values: [PadAnswer; 4],
    reads: Vec<u32>,
}
impl FakePads {
    fn new() -> Self {
        Self {
            values: [PadAnswer::Disconnected; 4],
            reads: Vec::new(),
        }
    }
}
impl PadApi for FakePads {
    fn read(&mut self, slot: u32) -> Result<Option<u16>> {
        self.reads.push(slot);
        match self.values[slot as usize] {
            PadAnswer::Buttons(b) => Ok(Some(b)),
            PadAnswer::Disconnected => Ok(None),
            PadAnswer::Error => anyhow::bail!("fake read error"),
        }
    }
}

#[test]
fn every_poll_reads_exactly_four_slots_and_errors_become_disconnected_not_presses() {
    let mut api = FakePads::new();
    api.values = [
        PadAnswer::Buttons(0x1000),
        PadAnswer::Error,
        PadAnswer::Disconnected,
        PadAnswer::Buttons(0x10),
    ];
    let mut observer = ControllerObserver::with_api(api, 0);
    let poll = observer.poll(0).unwrap().unwrap();
    assert_eq!(observer.api.reads, [0, 1, 2, 3]);
    assert_eq!(poll.unavailable_mask, 2);
    assert_eq!(
        poll.samples[0],
        PadSample {
            slot: 0,
            connected: true,
            buttons: 0x1000
        }
    );
    assert_eq!(
        poll.samples[1],
        PadSample {
            slot: 1,
            connected: false,
            buttons: 0
        }
    );
    assert_eq!(
        poll.samples[2],
        PadSample {
            slot: 2,
            connected: false,
            buttons: 0
        }
    );
    assert_eq!(
        poll.samples[3],
        PadSample {
            slot: 3,
            connected: true,
            buttons: 0x10
        }
    );
}

#[test]
fn observer_is_throttled_bounded_cancelable_and_never_polls_after_stop() {
    let mut observer = ControllerObserver::with_api(FakePads::new(), 10);
    assert!(observer.poll(10).unwrap().is_some());
    assert!(observer.poll(25).unwrap().is_none());
    assert_eq!(observer.api.reads.len(), 4);
    assert!(observer.poll(26).unwrap().is_some());
    assert_eq!(observer.api.reads.len(), 8);
    assert!(observer.poll(25).is_err());
    observer.stop();
    assert!(!observer.active());
    assert!(observer.poll(100).unwrap().is_none());
    assert_eq!(observer.api.reads.len(), 8);
    let mut observer = ControllerObserver::with_api(FakePads::new(), 10);
    assert!(
        observer
            .poll(policy::PLAYBACK_TIMEOUT_MS + 10)
            .unwrap()
            .is_none()
    );
    assert!(!observer.active() && observer.api.reads.is_empty());
}

fn playing() -> (policy::Coordinator, u64) {
    let mut coordinator = policy::Coordinator::default();
    let (token, _) = coordinator.start_preview(44, 0).unwrap();
    coordinator
        .handle(token, policy::Event::VideoPrepared, 0)
        .unwrap();
    coordinator
        .handle(
            token,
            policy::Event::AudioReady(policy::Volume::fallback()),
            0,
        )
        .unwrap();
    assert_eq!(coordinator.phase(), Some(policy::Phase::Playing));
    (coordinator, token)
}
fn feed(
    observer: &mut ControllerObserver<FakePads>,
    coordinator: &mut policy::Coordinator,
    token: u64,
    now: u64,
) -> Vec<policy::Effect> {
    let Some(poll) = observer.poll(now).unwrap() else {
        return Vec::new();
    };
    poll.samples
        .into_iter()
        .flat_map(|s| coordinator.handle(token, s.event(), now).unwrap())
        .collect()
}
#[test]
fn readonly_samples_drive_original_fresh_a_b_start_release_policy_on_every_slot() {
    for slot in 0..4 {
        for button in [0x1000, 0x2000, 0x10] {
            let (mut coordinator, token) = playing();
            let mut api = FakePads::new();
            api.values[slot] = PadAnswer::Buttons(button);
            let mut observer = ControllerObserver::with_api(api, 0);
            assert!(feed(&mut observer, &mut coordinator, token, 0).is_empty()); // held when opened
            observer.api.values[slot] = PadAnswer::Buttons(0);
            assert!(feed(&mut observer, &mut coordinator, token, 16).is_empty());
            observer.api.values[slot] = PadAnswer::Buttons(button);
            assert!(feed(&mut observer, &mut coordinator, token, 800).is_empty());
            observer.api.values[slot] = PadAnswer::Buttons(0);
            assert!(feed(&mut observer, &mut coordinator, token, 816).is_empty());
            assert!(feed(&mut observer, &mut coordinator, token, 1015).is_empty());
            // Poll cadence is >=16 ms. The release is observed >200 ms later.
            let effects = feed(&mut observer, &mut coordinator, token, 1031);
            assert!(effects.contains(&policy::Effect::SilenceVideo));
            assert_eq!(coordinator.phase(), Some(policy::Phase::Restoring));
        }
    }
}

#[test]
fn disconnected_or_failed_slot_resets_pending_skip_and_other_buttons_are_ignored() {
    let (mut coordinator, token) = playing();
    let mut api = FakePads::new();
    api.values[0] = PadAnswer::Buttons(0);
    let mut observer = ControllerObserver::with_api(api, 0);
    feed(&mut observer, &mut coordinator, token, 0);
    observer.api.values[0] = PadAnswer::Buttons(0x1000);
    feed(&mut observer, &mut coordinator, token, 800);
    observer.api.values[0] = PadAnswer::Error;
    feed(&mut observer, &mut coordinator, token, 816);
    observer.api.values[0] = PadAnswer::Buttons(0);
    feed(&mut observer, &mut coordinator, token, 1032);
    assert_eq!(coordinator.phase(), Some(policy::Phase::Playing));
    observer.api.values[0] = PadAnswer::Buttons(0x8000);
    feed(&mut observer, &mut coordinator, token, 1100);
    observer.api.values[0] = PadAnswer::Buttons(0);
    feed(&mut observer, &mut coordinator, token, 1120);
    feed(&mut observer, &mut coordinator, token, 1400);
    assert_eq!(coordinator.phase(), Some(policy::Phase::Playing));
}

#[test]
fn xinput_abi_matches_native_struct_sizes_without_loading_or_calling_any_dll() {
    assert_eq!(std::mem::size_of::<XinputPad>(), 12);
    assert_eq!(std::mem::align_of::<XinputPad>(), 2);
    assert_eq!(std::mem::size_of::<XinputState>(), 16);
    assert_eq!(std::mem::offset_of!(XinputState, pad), 4);
    assert_eq!(
        std::mem::size_of::<GetState>(),
        std::mem::size_of::<usize>()
    );
}
