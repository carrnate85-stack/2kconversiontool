//! Startup MP4 lifecycle only. No game files, input interception, process control
//! or native media APIs. Adapters execute ordered effects and acknowledge them.
use anyhow::{Result, ensure};
pub(crate) const WINDOW_TIMEOUT_MS: u64 = 120_000;
pub(crate) const WINDOW_STABLE_MS: u64 = 250;
pub(crate) const PLAYBACK_TIMEOUT_MS: u64 = 615_000;
pub(crate) const OPENING_GUARD_MS: u64 = 700;
pub(crate) const PAD_RELEASE_MS: u64 = 200;
/// WPF GameVolume.stop: wait up to 2 s for the ducking helper, then let the
/// intro close and return focus while that helper finishes restoring alone.
pub(crate) const RESTORE_RELEASE_MS: u64 = 2_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GameKey {
    pub pid: u32,
    pub birth: u64,
}
impl GameKey {
    fn validate(self) -> Result<()> {
        ensure!(
            self.pid > 0 && self.birth > 0,
            "Exact game birth identity required for startup video"
        );
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Volume {
    pub level: f32,
    pub muted: bool,
}
impl Volume {
    pub(crate) fn validate(self) -> Result<()> {
        ensure!(
            self.level.is_finite() && (0.0..=1.0).contains(&self.level),
            "Invalid Windows mixer sample"
        );
        Ok(())
    }
    pub(crate) fn fallback() -> Self {
        Self {
            level: 0.25,
            muted: false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Window {
    pub game: GameKey,
    pub hwnd: u64,
    pub visible: bool,
    pub minimized: bool,
    pub top_level: bool,
    pub width: u32,
    pub height: u32,
}
impl Window {
    fn usable(self, key: GameKey) -> bool {
        self.game == key
            && self.hwnd != 0
            && self.visible
            && !self.minimized
            && self.top_level
            && self.width >= 640
            && self.height >= 360
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reason {
    Ended,
    Skip,
    Error,
    WindowClosed,
    ParentClosed,
    GameEnded,
    Timeout,
    AudioFailed,
    LaunchFailed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    WaitingWindow,
    Preparing,
    Playing,
    Restoring,
    RestorationPending,
    Finished,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MonitorTarget {
    GameWindow(u64),
    OwnerWindow(u64),
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Effect {
    WatchOwnedWindow {
        game: GameKey,
        timeout_ms: u64,
    },
    CancelWindowWatch,
    /// Create hidden, frameless and fullscreen-capable. Select the actual game
    /// window's display. Set volume BEFORE assigning src or beginning playback.
    CreateHiddenFullscreen {
        monitor: MonitorTarget,
        initial_volume: Volume,
    },
    /// The owned audio guardian must outlive a crashing UI until it conditionally
    /// restores its retained game sessions. Never hard-kill that guardian.
    StartAudioGuardian {
        game: Option<GameKey>,
        duck_game: bool,
    },
    SetVideoVolume(Volume),
    ShowTrueFullscreen,
    StartControllerObserver,
    PlayVideo,
    SilenceVideo,
    StopControllerObserver,
    StopAudioGuardianAndRestore,
    DestroyVideoWindow,
    FocusOwnedGame(GameKey),
    FocusOwnerWindow(u64),
}
#[derive(Clone, Debug)]
pub(crate) enum Event {
    WindowObserved(Option<Window>),
    VideoPrepared,
    AudioReady(Volume),
    MixerChanged(Volume),
    PointerReleased,
    KeyReleased {
        supported_key: bool,
    },
    Controller {
        slot: usize,
        connected: bool,
        buttons: u16,
    },
    Finish(Reason),
    AudioRestored,
    AudioRestorationPending,
    Tick,
}
#[derive(Clone, Copy, Default, Debug)]
struct PadEdge {
    neutral: bool,
    pressed: bool,
    release_at: Option<u64>,
}
impl PadEdge {
    fn step(&mut self, connected: bool, down: bool, elapsed: u64) -> bool {
        if !connected {
            *self = Self::default();
            return false;
        }
        if !down {
            self.neutral = true;
        }
        if self.neutral && down && elapsed >= OPENING_GUARD_MS {
            self.pressed = true;
            self.release_at = None;
        }
        if self.pressed && !down {
            let since = *self.release_at.get_or_insert(elapsed);
            return elapsed - since >= PAD_RELEASE_MS;
        }
        false
    }
}
pub(crate) struct Session {
    token: u64,
    game: Option<GameKey>,
    preview_owner: Option<u64>,
    phase: Phase,
    started: u64,
    phase_started: u64,
    window_started: u64,
    last_now: u64,
    window: Option<(u64, u64)>,
    video_ready: bool,
    audio_ready: bool,
    guardian_started: bool,
    reason: Option<Reason>,
    volume: Volume,
    mixer_updated: bool,
    pads: [PadEdge; 4],
    restore_started: u64,
    window_released: bool,
}
impl Session {
    pub(crate) fn game(token: u64, game: GameKey, now: u64) -> Result<(Self, Vec<Effect>)> {
        game.validate()?;
        ensure!(token > 0, "Unique video generation required");
        Ok((
            Self::new(token, Some(game), None, Phase::WaitingWindow, now),
            vec![Effect::WatchOwnedWindow {
                game,
                timeout_ms: WINDOW_TIMEOUT_MS,
            }],
        ))
    }
    pub(crate) fn preview(token: u64, owner_window: u64, now: u64) -> Result<(Self, Vec<Effect>)> {
        ensure!(
            token > 0 && owner_window > 0,
            "Owned preview window and generation required"
        );
        let mut session = Self::new(token, None, Some(owner_window), Phase::Preparing, now);
        let effects = session.prepare(MonitorTarget::OwnerWindow(owner_window), now);
        Ok((session, effects))
    }
    fn new(
        token: u64,
        game: Option<GameKey>,
        preview_owner: Option<u64>,
        phase: Phase,
        now: u64,
    ) -> Self {
        Self {
            token,
            game,
            preview_owner,
            phase,
            started: now,
            phase_started: now,
            window_started: now,
            last_now: now,
            window: None,
            video_ready: false,
            audio_ready: false,
            guardian_started: false,
            reason: None,
            volume: Volume::fallback(),
            mixer_updated: false,
            pads: [PadEdge::default(); 4],
            restore_started: now,
            window_released: false,
        }
    }
    pub(crate) fn phase(&self) -> Phase {
        self.phase
    }
    pub(crate) fn may_replace(&self) -> bool {
        self.phase == Phase::Finished
    }
    fn prepare(&mut self, monitor: MonitorTarget, now: u64) -> Vec<Effect> {
        self.phase = Phase::Preparing;
        self.phase_started = now;
        self.window_started = now;
        self.guardian_started = true;
        vec![
            Effect::CreateHiddenFullscreen {
                monitor,
                initial_volume: self.volume,
            },
            Effect::StartAudioGuardian {
                game: self.game,
                duck_game: self.game.is_some(),
            },
        ]
    }
    fn maybe_play(&mut self, now: u64) -> Vec<Effect> {
        if self.phase != Phase::Preparing || !self.audio_ready || !self.video_ready {
            return vec![];
        }
        self.phase = Phase::Playing;
        self.phase_started = now;
        self.pads = [PadEdge::default(); 4];
        vec![
            Effect::SetVideoVolume(self.volume),
            Effect::ShowTrueFullscreen,
            Effect::StartControllerObserver,
            Effect::PlayVideo,
        ]
    }
    fn finish(&mut self, reason: Reason) -> Vec<Effect> {
        if matches!(
            self.phase,
            Phase::Restoring | Phase::RestorationPending | Phase::Finished
        ) {
            if matches!(
                reason,
                Reason::ParentClosed | Reason::GameEnded | Reason::LaunchFailed
            ) {
                self.reason = Some(reason);
            }
            return vec![];
        }
        self.reason = Some(reason);
        if !self.guardian_started {
            self.phase = Phase::Finished;
            return vec![Effect::CancelWindowWatch];
        }
        self.phase = Phase::Restoring;
        self.restore_started = self.last_now;
        // The adapter MUST silence/pause the intro before restoring game audio.
        // Destruction and game focus wait for the guardian's restoration ACK,
        // for at most RESTORE_RELEASE_MS (the guardian is never killed and a
        // replacement intro still waits for the ACK).
        vec![
            Effect::SilenceVideo,
            Effect::StopControllerObserver,
            Effect::StopAudioGuardianAndRestore,
        ]
    }
    /// Destroys the intro and returns focus once per session.
    fn release_window(&mut self) -> Vec<Effect> {
        if self.window_released {
            return vec![];
        }
        self.window_released = true;
        let mut effects = vec![Effect::DestroyVideoWindow];
        if !matches!(
            self.reason,
            Some(Reason::ParentClosed | Reason::GameEnded | Reason::LaunchFailed)
        ) {
            if let Some(game) = self.game {
                effects.push(Effect::FocusOwnedGame(game));
            } else if let Some(owner) = self.preview_owner {
                effects.push(Effect::FocusOwnerWindow(owner));
            }
        }
        effects
    }
    pub(crate) fn handle(&mut self, token: u64, event: Event, now: u64) -> Result<Vec<Effect>> {
        // Late callbacks from an older window/helper cannot control this video.
        if token != self.token || self.phase == Phase::Finished {
            return Ok(vec![]);
        }
        ensure!(
            now >= self.last_now,
            "Video lifecycle requires a monotonic clock"
        );
        self.last_now = now;
        if let Event::Finish(reason) = event {
            return Ok(self.finish(reason));
        }
        if matches!(self.phase, Phase::Restoring | Phase::RestorationPending) {
            match event {
                Event::AudioRestorationPending => self.phase = Phase::RestorationPending,
                Event::AudioRestored => {
                    self.phase = Phase::Finished;
                    return Ok(self.release_window());
                }
                Event::Tick if now - self.restore_started >= RESTORE_RELEASE_MS => {
                    return Ok(self.release_window());
                }
                _ => {}
            }
            return Ok(vec![]);
        }
        if (self.phase == Phase::WaitingWindow && now - self.started >= WINDOW_TIMEOUT_MS)
            || (matches!(self.phase, Phase::Preparing | Phase::Playing)
                && now - self.window_started >= PLAYBACK_TIMEOUT_MS)
        {
            return Ok(self.finish(Reason::Timeout));
        }
        match event {
            Event::WindowObserved(candidate) if self.phase == Phase::WaitingWindow => {
                if let Some(window) =
                    candidate.filter(|w| self.game.is_some_and(|game| w.usable(game)))
                {
                    let (prior, since) = self.window.get_or_insert((window.hwnd, now));
                    if *prior != window.hwnd {
                        *prior = window.hwnd;
                        *since = now;
                    }
                    if now - *since >= WINDOW_STABLE_MS {
                        return Ok(self.prepare(MonitorTarget::GameWindow(window.hwnd), now));
                    }
                } else {
                    self.window = None;
                }
            }
            Event::VideoPrepared if self.phase == Phase::Preparing => {
                self.video_ready = true;
                return Ok(self.maybe_play(now));
            }
            Event::AudioReady(volume) if self.phase == Phase::Preparing => {
                volume.validate()?;
                if !self.mixer_updated {
                    self.volume = volume;
                }
                self.audio_ready = true;
                return Ok(self.maybe_play(now));
            }
            Event::MixerChanged(volume)
                if matches!(self.phase, Phase::Preparing | Phase::Playing) =>
            {
                volume.validate()?;
                self.volume = volume;
                self.mixer_updated = true;
                return Ok(vec![Effect::SetVideoVolume(volume)]);
            }
            Event::PointerReleased
            | Event::KeyReleased {
                supported_key: true,
            } if self.phase == Phase::Playing => {
                if now - self.phase_started >= OPENING_GUARD_MS {
                    return Ok(self.finish(Reason::Skip));
                }
            }
            Event::Controller {
                slot,
                connected,
                buttons,
            } if self.phase == Phase::Playing => {
                ensure!(
                    slot < 4,
                    "Xbox controller slot is outside the bounded observer"
                );
                // XInput A, B, Start only. Read input; never intercept or inject it.
                if self.pads[slot].step(connected, buttons & 0x3010 != 0, now - self.phase_started)
                {
                    return Ok(self.finish(Reason::Skip));
                }
            }
            Event::Tick => {
                if self.phase == Phase::WaitingWindow && now - self.started >= WINDOW_TIMEOUT_MS {
                    return Ok(self.finish(Reason::Timeout));
                }
                if matches!(self.phase, Phase::Preparing | Phase::Playing)
                    && now - self.window_started >= PLAYBACK_TIMEOUT_MS
                {
                    return Ok(self.finish(Reason::Timeout));
                }
            }
            _ => {}
        }
        Ok(vec![])
    }
}

/// One launcher-owned media session at a time. A replacement cannot bypass
/// pending restoration, and generations are never reused during this lifetime.
#[derive(Default)]
pub(crate) struct Coordinator {
    generation: u64,
    current: Option<Session>,
}
impl Coordinator {
    fn reserve(&mut self) -> Result<u64> {
        ensure!(
            self.current.as_ref().is_none_or(Session::may_replace),
            "Previous startup video or audio restoration is still active"
        );
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("Startup-video generations exhausted"))?;
        Ok(self.generation)
    }
    pub(crate) fn start_game(&mut self, game: GameKey, now: u64) -> Result<(u64, Vec<Effect>)> {
        game.validate()?;
        let token = self.reserve()?;
        let (session, effects) = Session::game(token, game, now)?;
        self.current = Some(session);
        Ok((token, effects))
    }
    pub(crate) fn start_preview(&mut self, owner: u64, now: u64) -> Result<(u64, Vec<Effect>)> {
        ensure!(owner > 0, "Owned preview window required");
        let token = self.reserve()?;
        let (session, effects) = Session::preview(token, owner, now)?;
        self.current = Some(session);
        Ok((token, effects))
    }
    pub(crate) fn handle(&mut self, token: u64, event: Event, now: u64) -> Result<Vec<Effect>> {
        match &mut self.current {
            Some(session) => session.handle(token, event, now),
            None => Ok(vec![]),
        }
    }
    pub(crate) fn phase(&self) -> Option<Phase> {
        self.current.as_ref().map(Session::phase)
    }
}

/// Retain the COM session instance and exact game identity. Commands apply only
/// to that instance, never to an endpoint, master mixer or PID-only replacement.
pub(crate) struct MuteLease {
    game: GameKey,
    instance: String,
    original: bool,
    owns: bool,
    attempted: bool,
}
impl MuteLease {
    pub(crate) fn new(game: GameKey, instance: String, original: bool) -> Result<Self> {
        game.validate()?;
        ensure!(
            !instance.is_empty() && instance.len() <= 4096 && !instance.contains('\0'),
            "Invalid audio session instance"
        );
        Ok(Self {
            game,
            instance,
            original,
            owns: false,
            attempted: false,
        })
    }
    fn matches(&self, game: GameKey, instance: &str) -> Result<()> {
        ensure!(
            self.game == game && self.instance == instance,
            "Audio ownership changed; no mute write permitted"
        );
        Ok(())
    }
    pub(crate) fn duck(&mut self, game: GameKey, instance: &str) -> Result<Option<bool>> {
        self.matches(game, instance)?;
        if self.original || self.attempted {
            return Ok(None);
        }
        // Record before SetMute, since a failed native call can have taken effect.
        self.owns = true;
        self.attempted = true;
        Ok(Some(true))
    }
    pub(crate) fn observe(&mut self, current: bool) -> bool {
        if self.owns && !current {
            self.owns = false;
        }
        if self.owns { self.original } else { current }
    }
    pub(crate) fn restore(
        &mut self,
        game: GameKey,
        instance: &str,
        current: bool,
    ) -> Result<Option<bool>> {
        self.matches(game, instance)?;
        if !self.owns {
            return Ok(None);
        }
        if !current {
            self.owns = false;
            return Ok(None);
        }
        // Keep ownership until native SetMute and readback ACK restoration.
        Ok(Some(self.original))
    }
    pub(crate) fn confirm_restored(&mut self, current: bool) -> Result<()> {
        ensure!(
            !self.owns || current == self.original,
            "Audio restoration readback failed"
        );
        self.owns = false;
        Ok(())
    }
}

#[cfg(test)]
#[path = "startup_video_policy_tests.rs"]
mod tests;
