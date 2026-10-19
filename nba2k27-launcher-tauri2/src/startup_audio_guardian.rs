//! Owned CoreAudio session-mute guardian. UNWIRED: run in a separate child
//! process outside a kill-on-UI-close job, after authenticating its bootstrap.
//! Never changes volume levels, endpoints, Jukebox preferences, or game files.
//! A failed native mute may have applied: retain its intent until readback.
#![allow(dead_code)]
use crate::{
    startup_video_native::ExpectedGame,
    startup_video_policy::{GameKey, MuteLease, PLAYBACK_TIMEOUT_MS, Volume},
};
use anyhow::{Result, ensure};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    marker::PhantomData,
    path::{Path, PathBuf},
    rc::Rc,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const MAX_LEASES: usize = 64;
const MAX_ENDPOINTS: u32 = 128;
const MAX_SCANNED_SESSIONS: i32 = 8192;
const ENUM_BUDGET: Duration = Duration::from_millis(250);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SessionKey {
    pub endpoint: String,
    pub instance: String,
}
impl SessionKey {
    fn validate(&self) -> Result<()> {
        ensure!(
            [&self.endpoint, &self.instance]
                .into_iter()
                .all(|v| !v.is_empty() && v.len() <= 4096 && !v.contains('\0')),
            "Invalid exact CoreAudio endpoint/session identity"
        );
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SessionState {
    Inactive,
    Active,
    Expired,
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Sample {
    pub key: SessionKey,
    pub pid: u32,
    pub state: SessionState,
    pub volume: Volume,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WriteOutcome {
    Applied,
    /// Guard failed before SetMute was called. A first duck owns no mutation.
    BeforeWriteError(String),
    /// The exact session's value changed before our SetMute call; preserve it.
    CurrentChanged(bool),
    /// SetMute was called but failed, or its readback could not be confirmed.
    MayHaveApplied(String),
}
pub(crate) trait AudioApi {
    type Session;
    fn game_alive(&mut self) -> Result<bool>;
    fn sessions(&mut self) -> Result<Vec<Self::Session>>;
    fn inspect(&mut self, session: &Self::Session) -> Result<Sample>;
    fn set_mute(
        &mut self,
        session: &Self::Session,
        key: &SessionKey,
        expected_current: bool,
        value: bool,
    ) -> WriteOutcome;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Watching,
    Restoring,
    RestorationPending,
    Finished,
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Report {
    pub phase: Phase,
    pub volume: Option<Volume>,
    pub pending: usize,
    pub error: Option<String>,
}
struct Held<S> {
    session: S,
    lease: MuteLease,
    modified: bool,
}
pub(crate) struct Guardian<A: AudioApi> {
    api: A,
    game: GameKey,
    duck: bool,
    phase: Phase,
    started: u64,
    last_now: u64,
    held: BTreeMap<SessionKey, Held<A::Session>>,
    volume: Option<Volume>,
    error: Option<String>,
}
impl<A: AudioApi> Guardian<A> {
    pub fn new(api: A, game: GameKey, duck: bool, now: u64) -> Result<Self> {
        ensure!(
            game.pid > 0 && game.birth > 0,
            "Exact game identity required for audio guardian"
        );
        Ok(Self {
            api,
            game,
            duck,
            phase: Phase::Watching,
            started: now,
            last_now: now,
            held: BTreeMap::new(),
            volume: None,
            error: None,
        })
    }
    pub fn request_stop(&mut self) {
        if self.phase == Phase::Watching {
            self.phase = Phase::Restoring;
        }
    }
    fn fail(&mut self, error: String) {
        self.error = Some(error);
        self.request_stop();
    }
    fn validate_sample(game: GameKey, key: &SessionKey, sample: &Sample) -> Result<()> {
        key.validate()?;
        ensure!(
            sample.key == *key && sample.pid == game.pid,
            "Retained audio session ownership changed"
        );
        sample.volume.validate()
    }
    fn watch(&mut self) -> Result<()> {
        ensure!(self.api.game_alive()?, "Exact audio game exited");
        let sessions = self.api.sessions()?;
        ensure!(
            sessions.len() <= MAX_LEASES,
            "Too many candidate game audio sessions"
        );
        // Validate the complete discovery batch before the first new mutation.
        let mut discovered = Vec::new();
        let mut unique = BTreeSet::new();
        for session in sessions {
            let sample = self.api.inspect(&session)?;
            Self::validate_sample(self.game, &sample.key, &sample)?;
            ensure!(
                unique.insert(sample.key.clone()),
                "Duplicate CoreAudio session identity in discovery"
            );
            if sample.state != SessionState::Expired && !self.held.contains_key(&sample.key) {
                discovered.push((sample, session));
            }
        }
        ensure!(
            self.held.len() + discovered.len() <= MAX_LEASES,
            "Owned audio-session budget exhausted"
        );
        for (sample, session) in discovered {
            let key = sample.key;
            let lease = MuteLease::new(self.game, key.instance.clone(), sample.volume.muted)?;
            self.held.insert(
                key,
                Held {
                    session,
                    lease,
                    modified: false,
                },
            );
        }
        let mut samples = Vec::new();
        for (key, held) in &mut self.held {
            let mut sample = self.api.inspect(&held.session)?;
            Self::validate_sample(self.game, key, &sample)?;
            if sample.state == SessionState::Expired {
                held.modified = false;
                continue;
            }
            if self.duck {
                if let Some(value) = held.lease.duck(self.game, &key.instance)? {
                    // Intent precedes the call, including uncertain HRESULTs.
                    held.modified = true;
                    match self.api.set_mute(&held.session, key, false, value) {
                        WriteOutcome::Applied => {}
                        WriteOutcome::BeforeWriteError(error) => {
                            held.modified = false;
                            anyhow::bail!(error)
                        }
                        WriteOutcome::CurrentChanged(_) => {
                            held.modified = false;
                            anyhow::bail!("Game mute changed before duck; user's value preserved")
                        }
                        WriteOutcome::MayHaveApplied(error) => anyhow::bail!(error),
                    }
                    sample = self.api.inspect(&held.session)?;
                    Self::validate_sample(self.game, key, &sample)?;
                }
                let actual_muted = sample.volume.muted;
                sample.volume.muted = held.lease.observe(actual_muted);
                // User unmute is not remuted, and no longer belongs to us.
                if !actual_muted {
                    held.modified = false;
                }
            }
            samples.push(sample);
        }
        ensure!(
            self.api.game_alive()?,
            "Audio game exited during session observation"
        );
        let active = samples.iter().any(|s| s.state == SessionState::Active);
        let mut volume: Option<Volume> = None;
        for sample in samples
            .iter()
            .filter(|s| !active || s.state == SessionState::Active)
        {
            volume = Some(match volume {
                None => sample.volume,
                Some(prior) => Volume {
                    level: prior.level.min(sample.volume.level),
                    muted: prior.muted || sample.volume.muted,
                },
            });
        }
        self.volume = volume;
        Ok(())
    }
    fn restore(&mut self) {
        if !self.held.values().any(|held| held.modified) {
            self.held.clear();
            self.phase = Phase::Finished;
            return;
        }
        match self.api.game_alive() {
            Ok(false) => {
                self.held.clear();
                self.phase = Phase::Finished;
                return;
            }
            Err(error) => {
                self.error = Some(format!(
                    "Audio restore waiting for exact process presence: {error:#}"
                ));
                self.phase = Phase::RestorationPending;
                return;
            }
            Ok(true) => {}
        }
        for (key, held) in &mut self.held {
            if !held.modified {
                continue;
            }
            let result = (|| -> Result<()> {
                let sample = self.api.inspect(&held.session)?;
                Self::validate_sample(self.game, key, &sample)?;
                if sample.state == SessionState::Expired {
                    held.modified = false;
                    return Ok(());
                }
                if let Some(value) =
                    held.lease
                        .restore(self.game, &key.instance, sample.volume.muted)?
                {
                    match self.api.set_mute(&held.session, key, true, value) {
                        WriteOutcome::Applied => held.lease.confirm_restored(value)?,
                        WriteOutcome::CurrentChanged(false) => {
                            held.lease.confirm_restored(false)?
                        }
                        WriteOutcome::CurrentChanged(true) => {
                            anyhow::bail!("Audio restoration current value unexpectedly changed")
                        }
                        WriteOutcome::BeforeWriteError(error)
                        | WriteOutcome::MayHaveApplied(error) => anyhow::bail!(error),
                    }
                }
                held.modified = false;
                Ok(())
            })();
            if let Err(error) = result {
                self.error = Some(format!("Audio restoration pending: {error:#}"));
            }
        }
        self.phase = if self.held.values().any(|h| h.modified) {
            Phase::RestorationPending
        } else {
            Phase::Finished
        };
        if self.phase == Phase::Finished {
            self.held.clear();
        }
    }
    /// No native error can make the driver drop owned mutations. Even a
    /// backwards clock enters restoration rather than returning an error.
    pub fn step(&mut self, now: u64) -> Report {
        if self.phase == Phase::Finished {
            return self.report();
        }
        if now < self.last_now {
            self.fail("Audio guardian clock moved backwards".into());
        }
        self.last_now = self.last_now.max(now);
        if self.last_now - self.started >= PLAYBACK_TIMEOUT_MS {
            self.request_stop();
        }
        if self.phase == Phase::Watching {
            match self.api.game_alive() {
                Ok(false) => {
                    self.held.clear();
                    self.phase = Phase::Finished;
                }
                Err(error) => self.fail(format!("Exact audio game presence unknown: {error:#}")),
                Ok(true) => {
                    if let Err(error) = self.watch() {
                        self.fail(format!("Audio observation stopped: {error:#}"));
                    }
                }
            }
        }
        if matches!(self.phase, Phase::Restoring | Phase::RestorationPending) {
            self.restore();
        }
        self.report()
    }
    pub fn report(&self) -> Report {
        Report {
            phase: self.phase,
            volume: self.volume,
            pending: self.held.values().filter(|h| h.modified).count(),
            error: self.error.clone(),
        }
    }
}
impl<A: AudioApi> Drop for Guardian<A> {
    fn drop(&mut self) {
        // The driver normally reaches Finished. On unwinding, try once without
        // allowing an error to skip the remaining leases. Crucially release all
        // session COM interfaces BEFORE NativeAudio's apartment is destroyed.
        if self.phase != Phase::Finished {
            self.request_stop();
            self.restore();
        }
        self.held.clear();
    }
}

// This driver does not depend on the UI staying alive. The root child-process
// wrapper must authenticate its parent, omit kill-on-close jobs, and invoke it
// on the same thread on which NativeAudio was initialized. EOF is a stop, NOT
// permission to terminate before every owned mute is conditionally restored.
pub(crate) trait Driver {
    fn now_ms(&self) -> u64;
    fn stop_requested(&mut self) -> bool;
    fn emit(&mut self, report: &Report) -> Result<()>;
    fn pause(&mut self);
}
pub(crate) fn drive<A: AudioApi>(guardian: &mut Guardian<A>, driver: &mut impl Driver) {
    loop {
        if driver.stop_requested() {
            guardian.request_stop();
        }
        let report = guardian.step(driver.now_ms());
        if driver.emit(&report).is_err() {
            guardian.request_stop();
        }
        if report.phase == Phase::Finished {
            break;
        }
        driver.pause();
    }
}
struct StdioDriver<W: Write> {
    clock: Instant,
    stop: mpsc::Receiver<()>,
    output: W,
    broken_output: bool,
}
const OUTPUT_QUEUE: usize = 8;
const OUTPUT_LIMIT: usize = 16 * 1024;
struct QueuedOutput {
    sender: mpsc::SyncSender<Vec<u8>>,
    draft: Vec<u8>,
}
impl Write for QueuedOutput {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.draft.len().saturating_add(bytes.len()) > OUTPUT_LIMIT {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Audio diagnostic exceeds its bounded packet",
            ));
        }
        self.draft.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        if self.draft.is_empty() {
            return Ok(());
        }
        // Diagnostic delivery cannot gate audio restoration. A hung UI may
        // leave its pipe open without reading it; only the writer thread blocks.
        // Full queues drop diagnostics, not ownership or restoration work.
        match self.sender.try_send(std::mem::take(&mut self.draft)) {
            Ok(()) | Err(mpsc::TrySendError::Full(_)) => Ok(()),
            Err(mpsc::TrySendError::Disconnected(_)) => Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Audio output observer closed",
            )),
        }
    }
}
fn queued_stdout() -> QueuedOutput {
    let (sender, receiver) = mpsc::sync_channel::<Vec<u8>>(OUTPUT_QUEUE);
    thread::spawn(move || {
        let mut output = std::io::stdout();
        while let Ok(row) = receiver.recv() {
            if output.write_all(&row).and_then(|_| output.flush()).is_err() {
                break;
            }
        }
    });
    QueuedOutput {
        sender,
        draft: Vec::new(),
    }
}
impl<W: Write> Driver for StdioDriver<W> {
    fn now_ms(&self) -> u64 {
        self.clock.elapsed().as_millis().min(u64::MAX as u128) as u64
    }
    fn stop_requested(&mut self) -> bool {
        match self.stop.try_recv() {
            Ok(()) | Err(mpsc::TryRecvError::Disconnected) => true,
            Err(mpsc::TryRecvError::Empty) => false,
        }
    }
    fn emit(&mut self, r: &Report) -> Result<()> {
        if self.broken_output {
            return Ok(());
        }
        let event = match r.phase {
            Phase::Watching => "volume",
            Phase::Restoring | Phase::RestorationPending => "audio-restoration-pending",
            Phase::Finished => "audio-restored",
        };
        let value = serde_json::json!({"event":event,"found":r.volume.is_some(),"volume":r.volume.map(|v|v.level),"muted":r.volume.map(|v|v.muted),"pending":r.pending,"error":r.error});
        let result = (|| -> Result<()> {
            writeln!(self.output, "{value}")?;
            self.output.flush()?;
            Ok(())
        })();
        if result.is_err() {
            self.broken_output = true;
        }
        result
    }
    fn pause(&mut self) {
        thread::sleep(Duration::from_millis(100));
    }
}
/// Child-entry primitive only: caller must verify a private bootstrap and same
/// launcher parent before calling. No command-line branch is wired by this task.
pub(crate) fn run_stdio(expected: ExpectedGame, duck: bool) -> Result<()> {
    let audio = NativeAudio::bind(expected.clone())?;
    let mut guardian = Guardian::new(audio, expected.key, duck, 0)?;
    let (tx, rx) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let mut byte = [0];
        let _ = std::io::stdin().read(&mut byte);
        let _ = tx.send(());
    });
    let mut driver = StdioDriver {
        clock: Instant::now(),
        stop: rx,
        output: queued_stdout(),
        broken_output: false,
    };
    drive(&mut guardian, &mut driver);
    Ok(())
}

use windows::{
    Win32::{
        Foundation::{CloseHandle, FILETIME, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
        Media::Audio::*,
        System::{Com::*, Threading::*},
    },
    core::{GUID, Interface, PWSTR},
};
const MUTE_CONTEXT: GUID = GUID::from_u128(0x75c18c26_3154_4a83_9273_1842f7d7f371);
struct Process(HANDLE);
impl Drop for Process {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
struct Apartment(PhantomData<Rc<()>>);
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}
struct Wide(PWSTR);
impl Drop for Wide {
    fn drop(&mut self) {
        unsafe {
            CoTaskMemFree(Some(self.0.0 as _));
        }
    }
}
fn owned_string(value: PWSTR) -> Result<String> {
    let value = Wide(value);
    ensure!(!value.0.0.is_null(), "Null CoreAudio identity");
    let mut text = Vec::new();
    for index in 0..=4096 {
        let word = unsafe { *value.0.0.add(index) };
        if word == 0 {
            return Ok(String::from_utf16(&text)?);
        }
        ensure!(index < 4096, "CoreAudio identity exceeds bounds");
        text.push(word);
    }
    unreachable!()
}
pub(crate) struct NativeSession {
    endpoint: String,
    control: IAudioSessionControl2,
    volume: ISimpleAudioVolume,
}
pub(crate) struct NativeAudio {
    enumerator: IMMDeviceEnumerator,
    process: Process,
    expected: ExpectedGame,
    // Declared last so every COM interface drops before CoUninitialize.
    _apartment: Apartment,
}
impl NativeAudio {
    pub fn bind(expected: ExpectedGame) -> Result<Self> {
        ExpectedGame::new(expected.key, &expected.exe)?;
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        }
        let apartment = Apartment(PhantomData);
        let process = Process(unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                false,
                expected.key.pid,
            )?
        });
        let enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_INPROC_SERVER)? };
        let mut audio = Self {
            enumerator,
            process,
            expected,
            _apartment: apartment,
        };
        ensure!(
            audio.game_alive()?,
            "Exact audio game exited before binding"
        );
        Ok(audio)
    }
}
impl AudioApi for NativeAudio {
    type Session = NativeSession;
    fn game_alive(&mut self) -> Result<bool> {
        match unsafe { WaitForSingleObject(self.process.0, 0) } {
            WAIT_OBJECT_0 => return Ok(false),
            WAIT_TIMEOUT => {}
            _ => anyhow::bail!("Retained audio game presence is unknown"),
        }
        let pid = unsafe { GetProcessId(self.process.0) };
        let (mut created, mut exit, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        unsafe {
            GetProcessTimes(
                self.process.0,
                &mut created,
                &mut exit,
                &mut kernel,
                &mut user,
            )?;
        }
        let birth = ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64;
        let mut path = vec![0; 32768];
        let mut len = path.len() as u32;
        unsafe {
            QueryFullProcessImageNameW(
                self.process.0,
                PROCESS_NAME_WIN32,
                PWSTR(path.as_mut_ptr()),
                &mut len,
            )?;
        }
        ensure!(
            len > 0 && (len as usize) < path.len(),
            "Retained audio process path exceeds bounds"
        );
        let actual = PathBuf::from(String::from_utf16(&path[..len as usize])?);
        fn normal(path: &Path) -> String {
            let text = path.to_string_lossy();
            if let Some(text) = text.strip_prefix(r"\\?\UNC\") {
                format!(r"\\{text}")
            } else {
                text.strip_prefix(r"\\?\").unwrap_or(&text).into()
            }
        }
        ensure!(
            GameKey { pid, birth } == self.expected.key
                && normal(&actual).eq_ignore_ascii_case(&normal(&self.expected.exe)),
            "Retained audio game identity/executable changed"
        );
        match unsafe { WaitForSingleObject(self.process.0, 0) } {
            WAIT_OBJECT_0 => Ok(false),
            WAIT_TIMEOUT => Ok(true),
            _ => anyhow::bail!("Retained audio game presence became unknown"),
        }
    }
    fn sessions(&mut self) -> Result<Vec<NativeSession>> {
        ensure!(self.game_alive()?, "Audio game ended before discovery");
        let clock = Instant::now();
        let devices = unsafe {
            self.enumerator
                .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?
        };
        let count = unsafe { devices.GetCount()? };
        ensure!(
            count <= MAX_ENDPOINTS,
            "CoreAudio endpoint count exceeds bounds"
        );
        let mut result = Vec::new();
        let mut scanned = 0;
        for index in 0..count {
            ensure!(
                clock.elapsed() <= ENUM_BUDGET,
                "CoreAudio discovery exceeded its cooperative time budget"
            );
            let device = unsafe { devices.Item(index)? };
            let endpoint = owned_string(unsafe { device.GetId()? })?;
            let manager: IAudioSessionManager2 =
                unsafe { device.Activate(CLSCTX_INPROC_SERVER, None)? };
            let sessions = unsafe { manager.GetSessionEnumerator()? };
            let n = unsafe { sessions.GetCount()? };
            ensure!(
                n >= 0 && n <= 4096,
                "CoreAudio endpoint session count exceeds bounds"
            );
            scanned += n;
            ensure!(
                scanned <= MAX_SCANNED_SESSIONS,
                "CoreAudio total discovery count exceeds bounds"
            );
            for index in 0..n {
                ensure!(
                    clock.elapsed() <= ENUM_BUDGET,
                    "CoreAudio discovery exceeded its cooperative time budget"
                );
                let control: IAudioSessionControl2 =
                    unsafe { sessions.GetSession(index)? }.cast()?;
                if unsafe { control.GetProcessId()? } != self.expected.key.pid
                    || unsafe { control.GetState()? } == AudioSessionStateExpired
                {
                    continue;
                }
                let volume = control.cast()?;
                result.push(NativeSession {
                    endpoint: endpoint.clone(),
                    control,
                    volume,
                });
                ensure!(
                    result.len() <= MAX_LEASES,
                    "CoreAudio game session count exceeds bounds"
                );
            }
        }
        ensure!(self.game_alive()?, "Audio game ended during discovery");
        Ok(result)
    }
    fn inspect(&mut self, session: &NativeSession) -> Result<Sample> {
        let pid = unsafe { session.control.GetProcessId()? };
        let instance = owned_string(unsafe { session.control.GetSessionInstanceIdentifier()? })?;
        let key = SessionKey {
            endpoint: session.endpoint.clone(),
            instance,
        };
        key.validate()?;
        let state = unsafe { session.control.GetState()? };
        let state = if state == AudioSessionStateActive {
            SessionState::Active
        } else if state == AudioSessionStateExpired {
            SessionState::Expired
        } else if state == AudioSessionStateInactive {
            SessionState::Inactive
        } else {
            anyhow::bail!("Unknown CoreAudio session state")
        };
        let volume = if state == SessionState::Expired {
            Volume {
                level: 0.,
                muted: false,
            }
        } else {
            Volume {
                level: unsafe { session.volume.GetMasterVolume()? },
                muted: unsafe { session.volume.GetMute()? }.as_bool(),
            }
        };
        volume.validate()?;
        Ok(Sample {
            key,
            pid,
            state,
            volume,
        })
    }
    fn set_mute(
        &mut self,
        session: &NativeSession,
        key: &SessionKey,
        expected_current: bool,
        value: bool,
    ) -> WriteOutcome {
        let checked = (|| -> Result<Sample> {
            ensure!(self.game_alive()?, "Audio game ended before mute");
            let sample = self.inspect(session)?;
            Guardian::<Self>::validate_sample(self.expected.key, key, &sample)?;
            ensure!(
                sample.state != SessionState::Expired,
                "Audio session expired before mute"
            );
            Ok(sample)
        })();
        let sample = match checked {
            Ok(sample) => sample,
            Err(error) => return WriteOutcome::BeforeWriteError(format!("{error:#}")),
        };
        if sample.volume.muted != expected_current {
            return WriteOutcome::CurrentChanged(sample.volume.muted);
        }
        if let Err(error) = unsafe { session.volume.SetMute(value, &MUTE_CONTEXT) } {
            return WriteOutcome::MayHaveApplied(format!("SetMute failed after call: {error}"));
        }
        let readback = (|| -> Result<()> {
            ensure!(self.game_alive()?, "Audio game ended during mute readback");
            let after = self.inspect(session)?;
            Guardian::<Self>::validate_sample(self.expected.key, key, &after)?;
            ensure!(after.volume.muted == value, "Audio mute readback differs");
            Ok(())
        })();
        match readback {
            Ok(()) => WriteOutcome::Applied,
            Err(error) => WriteOutcome::MayHaveApplied(format!("{error:#}")),
        }
    }
}

#[cfg(test)]
#[path = "startup_audio_guardian_tests.rs"]
mod tests;
