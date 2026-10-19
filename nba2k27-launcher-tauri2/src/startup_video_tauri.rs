//! Isolated Tauri startup MP4 service. Integrator supplies only a trusted
//! launched-game binding and an ID-resolved immutable media lease. The intro
//! gets a dedicated tiny command, NEVER the general launcher command.
#![allow(dead_code)]
use crate::{
    startup_video_audio::{Notice, OwnedAudio},
    startup_video_helper::Binary,
    startup_video_media::VideoLease,
    startup_video_native::{
        ControllerObserver, ExpectedGame, Observation, OwnedGame, OwnedWindow, PadSample,
    },
    startup_video_policy::{Event, GameKey, Reason, Volume},
    startup_video_runtime::{Engine, Port},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs::{self, File},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use windows::{
    Win32::{
        Foundation::HWND,
        Storage::FileSystem::{FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT},
        UI::WindowsAndMessaging::{GetForegroundWindow, IsWindow, SetForegroundWindow},
    },
    core::Interface,
};
const LABEL: &str = "startup-intro";
const CONTROL: &str = "startup-intro-control";
fn local(url: &tauri::Url) -> bool {
    url.scheme() == "https"
        && url.host_str() == Some("tauri.localhost")
        && matches!(url.path(), "/" | "/index.html")
        && url.query() == Some("startupIntro=1")
        && url.fragment().is_none()
}
#[derive(Clone)]
pub(crate) struct Binding {
    session: String,
    game: ExpectedGame,
}
impl Binding {
    /// Call from the retained launch/provider owner, not renderer arguments or
    /// persisted PID/address preferences. No game version/pointer mapping needed.
    pub fn from_launched(session: &str, pid: u32, birth: u64, exe: &Path) -> Result<Self> {
        ensure!(
            session.len() == 48 && session.bytes().all(|b| b.is_ascii_hexdigit()),
            "Trusted launched session required for startup intro"
        );
        Ok(Self {
            session: session.into(),
            game: ExpectedGame::new(GameKey { pid, birth }, exe)?,
        })
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    generation: u64,
    phase: String,
    source: PathBuf,
    title: String,
    volume: f32,
    muted: bool,
    audio_status: String,
    finished: bool,
    message: String,
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum Request {
    Bootstrap {},
    Prepared { generation: u64 },
    PointerReleased { generation: u64 },
    KeyReleased { generation: u64, key: String },
    Finish { generation: u64, reason: String },
}
enum Command {
    Event(u64, Event),
    Cancel(Reason),
}
struct Active {
    session: String,
    generation: u64,
    tx: mpsc::SyncSender<Command>,
    view: Arc<Mutex<Snapshot>>,
    cancel: Arc<AtomicU8>,
}
pub(crate) struct Runtime {
    app: tauri::AppHandle,
    root: PathBuf,
    helper: Arc<Binary>,
    active: Mutex<Option<Active>>,
    generation: AtomicU64,
}
impl Runtime {
    pub fn new(app: tauri::AppHandle, root: PathBuf, helper: Arc<Binary>) -> Self {
        Self {
            app,
            root,
            helper,
            active: Mutex::new(None),
            generation: AtomicU64::new(0),
        }
    }
    pub fn start(&self, binding: Binding, media: VideoLease) -> Result<Value> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| anyhow::anyhow!("Startup intro ownership is poisoned"))?;
        if let Some(prior) = active.as_ref() {
            ensure!(
                prior
                    .view
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Intro status is unavailable"))?
                    .finished,
                "Previous intro/audio restoration is still active"
            );
        }
        let generation = self
            .generation
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |v| v.checked_add(1))
            .map_err(|_| anyhow::anyhow!("Intro generations exhausted"))?
            + 1;
        let (tx, rx) = mpsc::sync_channel(64);
        let cancel = Arc::new(AtomicU8::new(0));
        let cancelled = cancel.clone();
        let view = Arc::new(Mutex::new(Snapshot {
            generation,
            phase: "waiting-for-game-window".into(),
            source: media.path().into(),
            title: media.name.clone(),
            volume: 0.25,
            muted: false,
            audio_status: "Waiting for exact game mixer".into(),
            finished: false,
            message: String::new(),
        }));
        let app = self.app.clone();
        let root = self.root.clone();
        let helper = self.helper.clone();
        let session = binding.session.clone();
        let output = view.clone();
        let close_tx = tx.clone();
        let game = binding.game.clone();
        thread::Builder::new()
            .name("startup-mp4".into())
            .spawn(move || {
                let result = (|| -> Result<()> {
                    let owned = OwnedGame::retain(game.clone())?;
                    let port = NativePort {
                        app,
                        root,
                        helper,
                        session,
                        game,
                        owned,
                        media,
                        _profile_pins: vec![],
                        window: None,
                        window_hwnd: 0,
                        game_hwnd: 0,
                        shown: false,
                        destroyed: Arc::new(AtomicBool::new(false)),
                        return_owned: false,
                        source_generation: generation,
                        view: output.clone(),
                        close_tx,
                        audio: None,
                        pads: None,
                        sequence: 0,
                    };
                    let mut engine = Engine::new(generation, port.game.key, port, 0)?;
                    let clock = Instant::now();
                    while !engine.finished() {
                        let now = clock.elapsed().as_millis().min(u64::MAX as u128) as u64;
                        let reason = match cancelled.load(Ordering::Acquire) {
                            1 => Some(Reason::ParentClosed),
                            2 => Some(Reason::GameEnded),
                            3 => Some(Reason::LaunchFailed),
                            _ => None,
                        };
                        if let Some(reason) = reason {
                            let _ = engine.event(generation, Event::Finish(reason), now);
                        }
                        // A failed/dropped UI cannot postpone cancellation forever.
                        for _ in 0..64 {
                            match rx.try_recv() {
                                Ok(Command::Event(token, event)) => {
                                    if engine.event(token, event, now).is_err() {
                                        let _ = engine.event(
                                            generation,
                                            Event::Finish(Reason::Error),
                                            now,
                                        );
                                    }
                                }
                                Ok(Command::Cancel(reason)) => {
                                    let _ = engine.event(generation, Event::Finish(reason), now);
                                }
                                Err(mpsc::TryRecvError::Disconnected) => {
                                    let _ = engine.event(
                                        generation,
                                        Event::Finish(Reason::ParentClosed),
                                        now,
                                    );
                                    break;
                                }
                                Err(mpsc::TryRecvError::Empty) => break,
                            }
                        }
                        if let Err(error) = engine.step(now) {
                            if let Ok(mut state) = output.lock() {
                                state.message =
                                    format!("Intro lifecycle held for restoration: {error:#}");
                            }
                            let _ = engine.event(generation, Event::Finish(Reason::Error), now);
                        }
                        if let Ok(mut state) = output.lock() {
                            state.phase = format!("{:?}", engine.phase());
                            state.finished = engine.finished();
                        }
                        thread::sleep(Duration::from_millis(16));
                    }
                    Ok(())
                })();
                if let Ok(mut state) = output.lock() {
                    if let Err(error) = result {
                        state.phase = "failed".into();
                        state.message = format!("Startup MP4 stopped: {error:#}");
                    }
                    state.finished = true;
                }
            })?;
        *active = Some(Active {
            session: binding.session,
            generation,
            tx,
            view,
            cancel,
        });
        Ok(
            json!({"generation":generation,"phase":"waiting-for-game-window","gameFilesChanged":false}),
        )
    }
    pub fn cancel(&self, session: &str, reason: Reason) -> Result<()> {
        let active = self
            .active
            .lock()
            .map_err(|_| anyhow::anyhow!("Intro ownership unavailable"))?;
        if let Some(active) = active.as_ref().filter(|a| a.session == session) {
            active.cancel.store(
                match reason {
                    Reason::GameEnded => 2,
                    Reason::LaunchFailed => 3,
                    _ => 1,
                },
                Ordering::Release,
            );
            active
                .tx
                .try_send(Command::Cancel(reason))
                .map_err(|_| anyhow::anyhow!("Intro cancellation queue unavailable"))?;
        }
        Ok(())
    }
    pub fn shutdown(&self) {
        if let Ok(active) = self.active.lock() {
            if let Some(active) = active.as_ref() {
                active.cancel.store(1, Ordering::Release);
                let _ = active.tx.try_send(Command::Cancel(Reason::ParentClosed));
            }
        }
    }
    pub fn status(&self) -> Value {
        self.active
            .lock()
            .ok()
            .and_then(|v| {
                v.as_ref()
                    .and_then(|a| a.view.lock().ok().map(|s| json!(&*s)))
            })
            .unwrap_or_else(|| json!({"phase":"idle","finished":true}))
    }
    pub fn request(&self, window: &WebviewWindow, request: Request) -> Result<Value> {
        ensure!(
            window.label() == LABEL && local(&window.url()?),
            "Only the exact owned intro may use intro IPC"
        );
        let active = self
            .active
            .lock()
            .map_err(|_| anyhow::anyhow!("Intro ownership unavailable"))?;
        let active = active.as_ref().context("No owned startup intro")?;
        let state = active
            .view
            .lock()
            .map_err(|_| anyhow::anyhow!("Intro state unavailable"))?;
        ensure!(!state.finished, "Startup intro has already finished");
        let (generation, event) = match request {
            Request::Bootstrap {} => return Ok(json!(&*state)),
            Request::Prepared { generation } => (generation, Event::VideoPrepared),
            Request::PointerReleased { generation } => (generation, Event::PointerReleased),
            Request::KeyReleased { generation, key } => {
                ensure!(
                    matches!(key.as_str(), "Escape" | "Enter" | " "),
                    "Unsupported intro skip key"
                );
                (
                    generation,
                    Event::KeyReleased {
                        supported_key: true,
                    },
                )
            }
            Request::Finish { generation, reason } => {
                let reason = match reason.as_str() {
                    "ended" => Reason::Ended,
                    "error" | "autoplay-error" => Reason::Error,
                    _ => anyhow::bail!("Unsupported intro finish reason"),
                };
                (generation, Event::Finish(reason))
            }
        };
        ensure!(
            generation == active.generation,
            "Stale intro callback ignored"
        );
        active
            .tx
            .try_send(Command::Event(generation, event))
            .map_err(|_| anyhow::anyhow!("Intro event queue unavailable"))?;
        Ok(json!({"accepted":true}))
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        self.shutdown();
    }
}
struct NativePort {
    app: tauri::AppHandle,
    root: PathBuf,
    helper: Arc<Binary>,
    session: String,
    game: ExpectedGame,
    owned: OwnedGame,
    media: VideoLease,
    _profile_pins: Vec<File>,
    window: Option<WebviewWindow>,
    window_hwnd: u64,
    game_hwnd: u64,
    shown: bool,
    destroyed: Arc<AtomicBool>,
    return_owned: bool,
    source_generation: u64,
    view: Arc<Mutex<Snapshot>>,
    close_tx: mpsc::SyncSender<Command>,
    audio: Option<OwnedAudio>,
    pads: Option<ControllerObserver>,
    sequence: u64,
}
fn profile(root: &Path) -> Result<(PathBuf, Vec<File>)> {
    let mut pins = Vec::new();
    for path in [root.join("Cache"), root.join("Cache/StartupIntroWebView")] {
        match fs::create_dir(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
        let f = fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(&path)?;
        let m = f.metadata()?;
        ensure!(
            m.is_dir() && m.file_attributes() & 0x400 == 0,
            "Linked startup WebView cache rejected"
        );
        pins.push(f);
    }
    Ok((root.join("Cache/StartupIntroWebView"), pins))
}
impl NativePort {
    fn send(&mut self, op: &str, volume: Option<Volume>) -> Result<()> {
        self.sequence = self
            .sequence
            .checked_add(1)
            .context("Intro control sequence exhausted")?;
        let v = volume.map(|v| json!({"volume":v.level,"muted":v.muted}));
        self.window.as_ref().context("Owned intro window missing")?.emit(CONTROL,json!({"generation":self.source_generation,"sequence":self.sequence,"op":op,"volume":v}))?;
        Ok(())
    }
}
impl Port for NativePort {
    fn observe(&mut self) -> Result<Observation> {
        let sample = self.owned.observe()?;
        // Do not steal another app's foreground merely because NBA created a
        // background window. Stable readiness starts once the game is actually
        // the user's foreground; a later Alt-Tab cancels at the play guard.
        if self.window.is_none() {
            if let Observation::Window(Some(window)) = sample {
                if unsafe { GetForegroundWindow() }.0 as u64 != window.window.hwnd {
                    return Ok(Observation::Window(None));
                }
            }
        }
        Ok(sample)
    }
    fn foreign_foreground(&mut self) -> Result<bool> {
        if self.destroyed.load(Ordering::Acquire) {
            return Ok(true);
        }
        if !self.shown {
            return Ok(false);
        }
        let hwnd = unsafe { GetForegroundWindow() }.0 as u64;
        let own = self.owned.observe()?;
        let game = matches!(own,Observation::Window(Some(w)) if w.window.hwnd==hwnd);
        if hwnd != self.window_hwnd && !game {
            self.return_owned = false;
            if let Some(w) = &self.window {
                let _ = w.hide();
            }
            return Ok(true);
        }
        Ok(false)
    }
    fn create_hidden(&mut self, window: OwnedWindow, volume: Volume) -> Result<()> {
        let fresh = self
            .owned
            .verify_window(window.window.hwnd)?
            .context("Exact game window disappeared before intro creation")?;
        ensure!(
            fresh == window,
            "Game display/window changed before intro placement"
        );
        self.game_hwnd = window.window.hwnd;
        ensure!(
            self.window.is_none() && self.app.get_webview_window(LABEL).is_none(),
            "An intro window already exists"
        );
        let (profile, pins) = profile(&self.root)?;
        self._profile_pins = pins;
        let tx = self.close_tx.clone();
        let generation = self.source_generation;
        let destroyed = self.destroyed.clone();
        let w = WebviewWindowBuilder::new(
            &self.app,
            LABEL,
            WebviewUrl::App("index.html?startupIntro=1".into()),
        )
        .title("2K Startup Video")
        .decorations(false)
        .resizable(false)
        .visible(false)
        .focused(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .fullscreen(false)
        .data_directory(profile)
        .additional_browser_args("--autoplay-policy=no-user-gesture-required")
        .use_https_scheme(true)
        .disable_drag_drop_handler()
        .devtools(false)
        .on_navigation(local)
        .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
        .on_permission_request(|_, _| tauri::webview::PermissionResponse::Deny)
        .build()?;
        w.on_window_event(move |event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = tx.try_send(Command::Event(
                    generation,
                    Event::Finish(Reason::WindowClosed),
                ));
            }
            tauri::WindowEvent::Destroyed => {
                destroyed.store(true, Ordering::Release);
                let _ = tx.try_send(Command::Event(
                    generation,
                    Event::Finish(Reason::WindowClosed),
                ));
            }
            _ => {}
        });
        // Keep ownership before any fallible geometry/HWND operation, so a
        // partially prepared window cannot be orphaned by a later error.
        self.window = Some(w.clone());
        self.window_hwnd = w.hwnd()?.0 as u64;
        w.set_position(tauri::PhysicalPosition::new(
            window.monitor.x,
            window.monitor.y,
        ))?;
        w.set_size(tauri::PhysicalSize::new(
            window.monitor.width,
            window.monitor.height,
        ))?;
        self.set_volume(volume)
    }
    fn start_audio(&mut self) -> Result<()> {
        ensure!(self.audio.is_none(), "Intro already has an audio guardian");
        self.audio = Some(OwnedAudio::start(
            &self.game,
            self.helper.clone(),
            &self.session,
        )?);
        Ok(())
    }
    fn poll_audio(&mut self) -> Result<Option<Notice>> {
        let notice = match self.audio.as_mut() {
            Some(a) => a.poll()?,
            None => None,
        };
        if let Some(Notice::Watching(volume)) = &notice {
            if let Ok(mut state) = self.view.lock() {
                state.audio_status = if volume.is_some() {
                    "Following NBA 2K27 Windows Volume Mixer; game audio temporarily held"
                } else {
                    "Game mixer not available yet; intro uses quiet 25% fallback"
                }
                .into();
            }
        }
        Ok(notice)
    }
    fn set_volume(&mut self, volume: Volume) -> Result<()> {
        volume.validate()?;
        if let Ok(mut state) = self.view.lock() {
            state.volume = volume.level;
            state.muted = volume.muted;
        }
        if self.window.is_some() {
            self.send("volume", Some(volume))?;
        }
        Ok(())
    }
    fn show_fullscreen(&mut self) -> Result<()> {
        let window = self
            .owned
            .verify_window(self.game_hwnd)?
            .context("Exact game HWND no longer ready for fullscreen intro")?;
        let w = self.window.as_ref().context("Owned intro missing")?;
        let foreground = unsafe { GetForegroundWindow() }.0 as u64;
        ensure!(
            foreground == self.game_hwnd || foreground == self.window_hwnd,
            "User changed foreground before intro playback; no focus stolen"
        );
        w.set_position(tauri::PhysicalPosition::new(
            window.monitor.x,
            window.monitor.y,
        ))?;
        w.set_size(tauri::PhysicalSize::new(
            window.monitor.width,
            window.monitor.height,
        ))?;
        w.set_fullscreen(true)?;
        ensure!(w.is_fullscreen()?, "Intro did not enter true fullscreen");
        w.show()?;
        w.set_focus()?;
        self.shown = true;
        Ok(())
    }
    fn start_controllers(&mut self, now: u64) -> Result<()> {
        self.pads = Some(ControllerObserver::load(now)?);
        Ok(())
    }
    fn controllers(&mut self, now: u64) -> Result<Vec<PadSample>> {
        Ok(match self.pads.as_mut() {
            Some(p) => p.poll(now)?.map(|p| p.samples.to_vec()).unwrap_or_default(),
            None => vec![],
        })
    }
    fn stop_controllers(&mut self) {
        if let Some(p) = self.pads.as_mut() {
            p.stop();
        }
        self.pads = None;
    }
    fn play(&mut self) -> Result<()> {
        self.send("play", None)
    }
    fn silence(&mut self) -> Result<()> {
        if self.destroyed.load(Ordering::Acquire) {
            return Ok(());
        }
        let Some(w) = self.window.clone() else {
            return Ok(());
        };
        let foreground = unsafe { GetForegroundWindow() }.0 as u64;
        self.return_owned |= foreground == self.window_hwnd;
        let (tx, rx) = mpsc::sync_channel(1);
        w.with_webview(move |view| {
            let controller = view.controller();
            let result = (|| -> windows::core::Result<()> {
                unsafe {
                    let core: webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_8 =
                        controller.CoreWebView2()?.cast()?;
                    core.SetIsMuted(true)?;
                    let mut muted = windows::core::BOOL(0);
                    core.IsMuted(&mut muted)?;
                    if !muted.as_bool() {
                        return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                            0x80004005u32 as i32,
                        )));
                    }
                }
                Ok(())
            })()
            .or_else(|_| unsafe { controller.Close() });
            let _ = tx.send(result.map_err(|e| e.to_string()));
        })?;
        rx.recv_timeout(Duration::from_secs(1))
            .context("Native intro silence ACK timed out")?
            .map_err(|e| anyhow::anyhow!(e))?;
        let _ = self.send("silence", None);
        let _ = w.hide();
        Ok(())
    }
    fn stop_audio(&mut self) {
        if let Some(audio) = self.audio.as_mut() {
            audio.stop();
        }
    }
    fn destroy(&mut self) -> Result<()> {
        if self.destroyed.load(Ordering::Acquire) {
            self.window.take();
            self.shown = false;
            return Ok(());
        }
        if let Some(w) = self.window.as_ref() {
            w.destroy()?;
            ensure!(
                self.destroyed.load(Ordering::Acquire),
                "Waiting for exact owned intro window-destroy ACK"
            );
        }
        self.window.take();
        self.shown = false;
        Ok(())
    }
    fn return_focus_if_owned(&mut self, game: GameKey) -> Result<()> {
        if !self.return_owned || game != self.game.key {
            return Ok(());
        }
        let current = unsafe { GetForegroundWindow() }.0 as u64;
        if let Observation::Window(Some(window)) = self.owned.observe()? {
            let hwnd = window.window.hwnd;
            if (current == self.window_hwnd || current == hwnd)
                && self.owned.verify_window(hwnd)?.is_some()
                && unsafe { IsWindow(Some(HWND(hwnd as *mut _))) }.as_bool()
            {
                if current != hwnd {
                    let _ = unsafe { SetForegroundWindow(HWND(hwnd as *mut _)) };
                }
            }
        }
        Ok(())
    }
    fn diagnostic(&mut self, message: &str) {
        if let Ok(mut state) = self.view.lock() {
            state.message = message.chars().take(2048).collect();
        }
    }
}
impl Drop for NativePort {
    fn drop(&mut self) {
        // Panic/error fallback: native silence before EOF restoration. No child
        // termination, arbitrary process lookup, or game-file undo operations.
        let _ = self.silence();
        self.stop_controllers();
        self.stop_audio();
        let _ = self.destroy();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn intro_origin_and_route_do_not_admit_general_launcher_views() {
        assert!(local(
            &"https://tauri.localhost/index.html?startupIntro=1"
                .parse()
                .unwrap()
        ));
        for url in [
            "https://tauri.localhost/index.html",
            "https://tauri.localhost/index.html?overlay=1",
            "https://evil.invalid/index.html?startupIntro=1",
            "http://tauri.localhost/index.html?startupIntro=1",
            "https://tauri.localhost/index.html?startupIntro=1#x",
        ] {
            assert!(!local(&url.parse().unwrap()));
        }
    }
    #[test]
    fn intro_requests_cannot_choose_paths_games_or_launcher_operations() {
        for request in [
            json!({"op":"launch"}),
            json!({"op":"prepared","generation":1,"path":"C:/x.mp4"}),
            json!({"op":"bootstrap","pid":10}),
            json!({"op":"write"}),
        ] {
            assert!(serde_json::from_value::<Request>(request).is_err());
        }
        assert!(serde_json::from_value::<Request>(json!({"op":"prepared","generation":1})).is_ok());
    }
}
