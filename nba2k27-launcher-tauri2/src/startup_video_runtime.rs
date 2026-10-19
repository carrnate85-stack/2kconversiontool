//! Executable startup-intro orchestration. Native/Tauri operations are an
//! explicit port, so the real ordering, failures and cancellation are fixture
//! tested without opening windows, touching audio or launching any game.
#![allow(dead_code)]
use crate::{
    startup_video_audio::Notice,
    startup_video_native::{Observation, OwnedWindow, PadSample},
    startup_video_policy::{self as policy, Effect, Event, GameKey, Phase, Reason, Volume},
};
use anyhow::{Result, ensure};
use std::collections::VecDeque;

pub(crate) trait Port {
    fn observe(&mut self) -> Result<Observation>;
    fn foreign_foreground(&mut self) -> Result<bool> {
        Ok(false)
    }
    fn create_hidden(&mut self, window: OwnedWindow, volume: Volume) -> Result<()>;
    fn start_audio(&mut self) -> Result<()>;
    fn poll_audio(&mut self) -> Result<Option<Notice>>;
    fn set_volume(&mut self, volume: Volume) -> Result<()>;
    fn show_fullscreen(&mut self) -> Result<()>;
    fn start_controllers(&mut self, now: u64) -> Result<()>;
    fn controllers(&mut self, now: u64) -> Result<Vec<PadSample>>;
    fn stop_controllers(&mut self);
    fn play(&mut self) -> Result<()>;
    /// Must ACK actual native mute/controller-close before returning success.
    /// On failure the engine MUST NOT restore game audio or claim completion.
    fn silence(&mut self) -> Result<()>;
    fn stop_audio(&mut self);
    fn destroy(&mut self) -> Result<()>;
    /// No unconditional focus: only if the user's foreground is still this
    /// exact intro and the retained game HWND/process identity remains valid.
    fn return_focus_if_owned(&mut self, game: GameKey) -> Result<()>;
    fn diagnostic(&mut self, message: &str);
}
pub(crate) struct Engine<P: Port> {
    session: policy::Session,
    token: u64,
    port: P,
    window: Option<OwnedWindow>,
    audio_started: bool,
    audio_waiting: bool,
    media_ready: bool,
    audio_ready: bool,
    silence_pending: bool,
    stop_after_silence: bool,
    restored_after_silence: bool,
    destroy_pending: bool,
    prepared_at: Option<u64>,
    last_observe: Option<u64>,
    last_now: u64,
}
impl<P: Port> Engine<P> {
    pub fn new(token: u64, game: GameKey, port: P, now: u64) -> Result<Self> {
        let (session, _) = policy::Session::game(token, game, now)?;
        Ok(Self {
            session,
            token,
            port,
            window: None,
            audio_started: false,
            audio_waiting: false,
            media_ready: false,
            audio_ready: false,
            silence_pending: false,
            stop_after_silence: false,
            restored_after_silence: false,
            destroy_pending: false,
            prepared_at: None,
            last_observe: None,
            last_now: now,
        })
    }
    pub fn phase(&self) -> Phase {
        if self.destroy_pending {
            Phase::RestorationPending
        } else {
            self.session.phase()
        }
    }
    pub fn finished(&self) -> bool {
        self.session.phase() == Phase::Finished && !self.destroy_pending
    }
    fn effects(&mut self, effects: Vec<Effect>, now: u64) -> Result<()> {
        let mut pending: VecDeque<_> = effects.into();
        let mut count = 0;
        while let Some(effect) = pending.pop_front() {
            count += 1;
            ensure!(
                count <= 64,
                "Startup-intro effect recursion exceeded its bound"
            );
            let result = match effect {
                Effect::WatchOwnedWindow { .. } | Effect::CancelWindowWatch => Ok(()),
                Effect::CreateHiddenFullscreen {
                    monitor,
                    initial_volume,
                } => {
                    self.prepared_at = Some(now);
                    let window = self.window.filter(|w| matches!(monitor, policy::MonitorTarget::GameWindow(h) if h == w.window.hwnd));
                    match window {
                        Some(w) => self.port.create_hidden(w, initial_volume),
                        None => Err(anyhow::anyhow!("Intro has no fresh exact-game monitor")),
                    }
                }
                Effect::StartAudioGuardian { game, duck_game } => {
                    ensure!(
                        game.is_some() && duck_game,
                        "Launch-time intro requires the owned game audio guardian"
                    );
                    // Hidden WebView loading is not consent to mute the game.
                    // The exact intro must ACK decodable canplay first.
                    self.audio_waiting = true;
                    Ok(())
                }
                Effect::SetVideoVolume(volume) => self.port.set_volume(volume),
                Effect::ShowTrueFullscreen => self.port.show_fullscreen(),
                Effect::StartControllerObserver => {
                    if let Err(e) = self.port.start_controllers(now) {
                        self.port.diagnostic(&format!(
                            "Xbox skip unavailable; mouse/keyboard remain available: {e:#}"
                        ));
                    }
                    Ok(())
                }
                Effect::PlayVideo => self.port.play(),
                Effect::SilenceVideo => {
                    self.silence_pending = true;
                    match self.port.silence() {
                        Ok(()) => {
                            self.silence_pending = false;
                            Ok(())
                        }
                        Err(e) => {
                            self.port
                                .diagnostic(&format!("Intro audio silence is pending: {e:#}"));
                            Ok(())
                        }
                    }
                }
                Effect::StopControllerObserver => {
                    self.port.stop_controllers();
                    Ok(())
                }
                Effect::StopAudioGuardianAndRestore => {
                    if self.silence_pending {
                        self.stop_after_silence = true;
                    } else if self.audio_started {
                        self.port.stop_audio();
                    } else {
                        pending.extend(self.session.handle(
                            self.token,
                            Event::AudioRestored,
                            now,
                        )?);
                    }
                    Ok(())
                }
                Effect::DestroyVideoWindow => {
                    match self.port.destroy() {
                        Ok(()) => self.destroy_pending = false,
                        Err(error) => {
                            self.destroy_pending = true;
                            self.port.diagnostic(&format!(
                                "Owned intro window cleanup pending; replacement held: {error:#}"
                            ));
                        }
                    }
                    Ok(())
                }
                Effect::FocusOwnedGame(game) => {
                    if self.destroy_pending {
                        Ok(())
                    } else {
                        self.port.return_focus_if_owned(game)
                    }
                }
                Effect::FocusOwnerWindow(_) => Err(anyhow::anyhow!(
                    "Game intro cannot focus an arbitrary owner"
                )),
            };
            if let Err(error) = result {
                self.port
                    .diagnostic(&format!("Startup intro stopped: {error:#}"));
                // Discard remaining play/show effects after a failed prepare.
                pending.clear();
                pending.extend(self.session.handle(
                    self.token,
                    Event::Finish(Reason::Error),
                    now,
                )?);
            }
        }
        Ok(())
    }
    pub fn event(&mut self, token: u64, event: Event, now: u64) -> Result<()> {
        if token != self.token || self.finished() {
            return Ok(());
        }
        if matches!(event, Event::VideoPrepared) && self.phase() == Phase::Preparing {
            self.media_ready = true;
            if self.audio_waiting && !self.audio_started {
                match self.port.start_audio() {
                    Ok(()) => {
                        self.audio_started = true;
                        self.audio_waiting = false;
                    }
                    Err(error) => {
                        self.port.diagnostic(&format!(
                            "Intro audio guardian could not start: {error:#}"
                        ));
                        return self.event(token, Event::Finish(Reason::AudioFailed), now);
                    }
                }
            }
        }
        let effects = self.session.handle(token, event, now)?;
        self.effects(effects, now)
    }
    pub fn step(&mut self, now: u64) -> Result<()> {
        ensure!(now >= self.last_now, "Startup intro clock moved backwards");
        self.last_now = now;
        if self.finished() {
            return Ok(());
        }
        if self.destroy_pending {
            // A failed close is not completion and never permits a replacement
            // or a late focus operation. Audio has already acknowledged restore.
            if self.port.destroy().is_ok() {
                self.destroy_pending = false;
            }
            return Ok(());
        }
        if self.silence_pending {
            if self.port.silence().is_ok() {
                self.silence_pending = false;
                if self.stop_after_silence {
                    self.stop_after_silence = false;
                    if self.audio_started {
                        self.port.stop_audio();
                    } else {
                        self.event(self.token, Event::AudioRestored, now)?;
                    }
                }
                if self.restored_after_silence {
                    self.restored_after_silence = false;
                    self.event(self.token, Event::AudioRestored, now)?;
                }
            }
        }
        if self.last_observe.is_none_or(|prior| now - prior >= 100) {
            self.last_observe = Some(now);
            match self.port.observe() {
                Ok(Observation::GameEnded) => {
                    self.event(self.token, Event::Finish(Reason::GameEnded), now)?
                }
                Ok(Observation::Window(window)) => {
                    if self.phase() == Phase::WaitingWindow {
                        self.window = window;
                        self.event(
                            self.token,
                            Event::WindowObserved(window.map(|w| w.window)),
                            now,
                        )?;
                    }
                }
                Err(error) => {
                    self.port.diagnostic(&format!(
                        "Exact-game window observation unavailable: {error:#}"
                    ));
                    if self.phase() == Phase::WaitingWindow {
                        self.window = None;
                        self.event(self.token, Event::WindowObserved(None), now)?;
                    } else {
                        self.event(self.token, Event::Finish(Reason::Error), now)?;
                    }
                }
            }
        }
        if self.audio_started {
            match self.port.poll_audio().map_err(|error| format!("{error:#}")) {
                Ok(Some(Notice::Watching(volume))) => {
                    let volume = volume.unwrap_or_else(Volume::fallback);
                    if !self.audio_ready {
                        self.audio_ready = true;
                        self.event(self.token, Event::AudioReady(volume), now)?;
                    } else {
                        self.event(self.token, Event::MixerChanged(volume), now)?;
                    }
                }
                Ok(Some(Notice::Restored)) => {
                    if !matches!(self.phase(), Phase::Restoring | Phase::RestorationPending) {
                        self.event(self.token, Event::Finish(Reason::AudioFailed), now)?;
                    }
                    if self.silence_pending {
                        self.restored_after_silence = true;
                    } else {
                        self.event(self.token, Event::AudioRestored, now)?;
                    }
                }
                Ok(Some(Notice::Pending)) => {
                    if !matches!(self.phase(), Phase::Restoring | Phase::RestorationPending) {
                        self.event(self.token, Event::Finish(Reason::AudioFailed), now)?;
                    }
                    self.event(self.token, Event::AudioRestorationPending, now)?;
                }
                Ok(Some(Notice::NotStarted(error))) => {
                    if self.audio_ready {
                        self.port.diagnostic("Rejected no-mutation audio claim after drive readiness; ownership held");
                        self.event(self.token, Event::Finish(Reason::AudioFailed), now)?;
                        self.event(self.token, Event::AudioRestorationPending, now)?;
                        return Ok(());
                    }
                    self.port.diagnostic(&format!(
                        "Audio guardian could not initialize; verified no audio mutation: {error}"
                    ));
                    self.audio_started = false;
                    self.event(self.token, Event::Finish(Reason::AudioFailed), now)?;
                    if self.silence_pending {
                        self.restored_after_silence = true;
                    } else {
                        self.event(self.token, Event::AudioRestored, now)?;
                    }
                }
                Ok(Some(Notice::Failed(error))) | Err(error) => {
                    self.port
                        .diagnostic(&format!("Owned audio guardian unavailable: {error:#}"));
                    self.event(self.token, Event::Finish(Reason::AudioFailed), now)?;
                    self.event(self.token, Event::AudioRestorationPending, now)?;
                }
                Ok(None) => {}
            }
        }
        if self.phase() == Phase::Preparing && self.prepared_at.is_some_and(|at| now - at >= 15_000)
        {
            self.event(self.token, Event::Finish(Reason::Timeout), now)?;
        }
        if self.phase() == Phase::Playing {
            match self.port.foreign_foreground() {
                Ok(true) => {
                    self.event(self.token, Event::Finish(Reason::WindowClosed), now)?;
                    return Ok(());
                }
                Err(error) => {
                    self.port.diagnostic(&format!(
                        "Intro foreground ownership unavailable: {error:#}"
                    ));
                    self.event(self.token, Event::Finish(Reason::Error), now)?;
                    return Ok(());
                }
                Ok(false) => {}
            }
            match self.port.controllers(now) {
                Ok(samples) => {
                    ensure!(samples.len() <= 4, "Unbounded controller sample batch");
                    for sample in samples {
                        self.event(self.token, sample.event(), now)?;
                    }
                }
                Err(error) => self
                    .port
                    .diagnostic(&format!("Xbox observer failed: {error:#}")),
            }
        }
        // A page that is not yet verified silent is never released early.
        if self.silence_pending
            && matches!(self.phase(), Phase::Restoring | Phase::RestorationPending)
        {
            return Ok(());
        }
        self.event(self.token, Event::Tick, now)
    }
}

#[cfg(test)]
#[path = "startup_video_runtime_tests.rs"]
mod tests;
