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
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use windows::{
    Win32::{
        Foundation::HWND,
        Storage::FileSystem::{FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT},
        UI::WindowsAndMessaging::{
            GetForegroundWindow, GetWindowThreadProcessId, IsWindow, SetForegroundWindow,
        },
    },
    core::Interface,
};
const LABEL: &str = "startup-intro";
const CONTROL: &str = "startup-intro-control";
fn append_trace(root: &Path, session: &str, phase: &str, message: &str) {
    use std::io::Write;
    if session.len() != 48 || !session.bytes().all(|b| b.is_ascii_hexdigit()) {
        return;
    }
    let _ = fs::create_dir_all(root.join("Logs"));
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(
        root.join("Logs")
            .join(format!("startup-video-{session}.jsonl")),
    ) {
        let row = json!({"phase":phase,"message":message.chars().take(2048).collect::<String>(),"updatedMs":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis()});
        let _ = writeln!(file, "{row}");
        let _ = file.sync_data();
    }
}
pub(crate) fn write_trace(root: &Path, session: &str, phase: &str, finished: bool, message: &str) {
    if session.len() != 48 || !session.bytes().all(|b| b.is_ascii_hexdigit()) {
        return;
    }
    let logs = root.join("Logs");
    if fs::create_dir_all(&logs).is_err() {
        return;
    }
    append_trace(root, session, phase, message);
    let updated_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64;
    let _ = crate::backend::atomic_json(
        &logs.join("startup-video-last.json"),
        &json!({
            "schema": "nba2k27-startup-video-status-v1",
            "session": session,
            "phase": phase,
            "finished": finished,
            "message": message.chars().take(2048).collect::<String>(),
            "updatedMs": updated_ms
        }),
    );
}
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
    launch_status: Value,
}
#[derive(Deserialize, Serialize, Debug)]
#[serde(tag = "op", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum Request {
    Bootstrap {},
    LaunchStatus {
        generation: u64,
    },
    Prepared {
        generation: u64,
    },
    PointerReleased {
        generation: u64,
    },
    KeyReleased {
        generation: u64,
        key: String,
    },
    Finish {
        generation: u64,
        reason: String,
    },
    Progress {
        generation: u64,
        stage: ProgressStage,
        detail: String,
    },
}
#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ProgressStage {
    Bootstrap,
    Metadata,
    Canplay,
    Playing,
    Frame,
    Launch,
    Error,
}
enum Command {
    Event(u64, Event),
    Cancel(Reason),
    Bind(ExpectedGame),
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
    pub fn start(&self, session: String, media: VideoLease) -> Result<Value> {
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
            phase: "preparing-on-play".into(),
            source: media.path().into(),
            title: media.name.clone(),
            volume: 0.25,
            muted: false,
            audio_status: "Waiting for exact game mixer".into(),
            finished: false,
            message: String::new(),
            launch_status: json!({"phase":"preparing","providerPhase":"queued","failed":false}),
        }));
        let app = self.app.clone();
        let root = self.root.clone();
        let helper = self.helper.clone();
        let trace_session_id = session.clone();
        let output = view.clone();
        let close_tx = tx.clone();
        let owner = self
            .app
            .get_webview_window("main")
            .context("Launcher window unavailable")?
            .hwnd()?
            .0 as u64;
        write_trace(
            &self.root,
            &session,
            "preparing-on-play",
            false,
            "Preparing video immediately from Play",
        );
        thread::Builder::new()
            .name("startup-mp4".into())
            .spawn(move || {
                let trace_root = root.clone();
                let trace_session = session.clone();
                let result = (|| -> Result<()> {
                    let port = NativePort {
                        app,
                        root,
                        helper,
                        session,
                        game: None,
                        owned: None,
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
                    let mut engine = Engine::launch(generation, owner, port, 0)?;
                    let clock = Instant::now();
                    let mut last_trace = String::new();
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
                                Ok(Command::Bind(game)) => {
                                    if let Err(error) = engine.bind_launched_game(game, now) {
                                        if let Ok(mut state) = output.lock() {
                                            state.message =
                                                format!("Intro launch binding failed: {error:#}");
                                        }
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
                            let trace = format!(
                                "{}\u{1f}{}\u{1f}{}",
                                state.phase, state.finished, state.message
                            );
                            if trace != last_trace {
                                write_trace(
                                    &trace_root,
                                    &trace_session,
                                    &state.phase,
                                    state.finished,
                                    &state.message,
                                );
                                last_trace = trace;
                            }
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
                    write_trace(
                        &trace_root,
                        &trace_session,
                        &state.phase,
                        true,
                        &state.message,
                    );
                }
            })?;
        *active = Some(Active {
            session: trace_session_id,
            generation,
            tx,
            view,
            cancel,
        });
        Ok(json!({"generation":generation,"phase":"preparing-on-play","gameFilesChanged":false}))
    }
    pub fn bind(&self, token: &str, binding: Binding) -> Result<()> {
        let active = self
            .active
            .lock()
            .map_err(|_| anyhow::anyhow!("Intro ownership unavailable"))?;
        let active = active
            .as_ref()
            .filter(|a| a.session == token)
            .context("Stale Play binding")?;
        if active
            .view
            .lock()
            .map_err(|_| anyhow::anyhow!("Intro status unavailable"))?
            .finished
        {
            return Ok(());
        }
        append_trace(
            &self.root,
            token,
            "game-bound",
            &format!(
                "nativeSession={} pid={}",
                binding.session, binding.game.key.pid
            ),
        );
        active
            .tx
            .try_send(Command::Bind(binding.game))
            .map_err(|_| anyhow::anyhow!("Intro binding queue unavailable"))?;
        Ok(())
    }
    /// Cached, bounded launch-stage metadata only; no IPC can choose a session.
    pub fn update_launch_status(&self, token: &str, status: &Value) -> Result<()> {
        let active = self
            .active
            .lock()
            .map_err(|_| anyhow::anyhow!("Intro ownership unavailable"))?;
        let active = active
            .as_ref()
            .filter(|a| a.session == token)
            .context("Stale intro progress")?;
        let mut view = active
            .view
            .lock()
            .map_err(|_| anyhow::anyhow!("Intro state unavailable"))?;
        if view.finished {
            return Ok(());
        }
        let phase = |v: &Value| {
            v.as_str()
                .filter(|s| s.len() <= 64 && s.bytes().all(|c| c.is_ascii_lowercase() || c == b'-'))
                .unwrap_or("unknown")
                .to_owned()
        };
        view.launch_status = json!({"phase":phase(&status["phase"]),"providerPhase":phase(&status["nativeProvider"]["phase"]),"steamOnly":status["steamOnly"]==true,"failed":status["error"].as_str().is_some_and(|s|!s.is_empty()) || status["nativeProvider"]["error"].as_str().is_some_and(|s|!s.is_empty())});
        Ok(())
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
        let mut state = active
            .view
            .lock()
            .map_err(|_| anyhow::anyhow!("Intro state unavailable"))?;
        ensure!(!state.finished, "Startup intro has already finished");
        let (generation, event) = match request {
            Request::Bootstrap {} => return Ok(json!(&*state)),
            Request::LaunchStatus { generation } => {
                ensure!(
                    generation == active.generation,
                    "Stale intro launch progress"
                );
                return Ok(state.launch_status.clone());
            }
            Request::Progress {
                generation,
                stage,
                detail,
            } => {
                ensure!(
                    generation == active.generation && detail.len() <= 2048,
                    "Invalid intro progress"
                );
                let message = format!(
                    "Media {stage:?}: {}",
                    detail.chars().take(512).collect::<String>()
                );
                append_trace(&self.root, &active.session, &state.phase, &message);
                if matches!(stage, ProgressStage::Error) {
                    state.message = message;
                }
                return Ok(json!({"accepted":true}));
            }
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
    game: Option<ExpectedGame>,
    owned: Option<OwnedGame>,
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
pub(crate) fn build_intro_window(
    app: &tauri::AppHandle,
    root: &Path,
    bootstrap: &str,
) -> Result<(WebviewWindow, Vec<File>)> {
    let (profile, pins) = profile(root)?;
    let initialization = format!(
        "Object.defineProperty(window,'__startupIntroBootstrap',{{value:Object.freeze({bootstrap}),writable:false,configurable:false}});"
    );
    let w = WebviewWindowBuilder::new(
        app,
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
    .initialization_script(initialization)
    .additional_browser_args("--autoplay-policy=no-user-gesture-required")
    .use_https_scheme(true)
    .disable_drag_drop_handler()
    .devtools(false)
    .on_navigation(local)
    .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
    .on_permission_request(|_, _| tauri::webview::PermissionResponse::Deny)
    .build()?;
    Ok((w, pins))
}
impl NativePort {
    fn create_at(
        &mut self,
        monitor: crate::startup_video_native::Bounds,
        volume: Volume,
    ) -> Result<()> {
        ensure!(
            self.window.is_none() && self.app.get_webview_window(LABEL).is_none(),
            "An intro window already exists"
        );
        let tx = self.close_tx.clone();
        let generation = self.source_generation;
        let destroyed = self.destroyed.clone();
        // Freeze the trusted bootstrap into this one-purpose view before its
        // first script runs. The old page requested bootstrap over IPC during
        // initial navigation; in production that can race the window ACL and
        // leave a decodable MP4 in an invisible window until prepare timeout.
        // All later callbacks still cross the exact-label/route/generation gate.
        let bootstrap = serde_json::to_string(&json!({
            "generation": self.source_generation,
            "source": self.media.path(),
            "title": self.media.name,
            "volume": volume.level,
            "muted": volume.muted,
            "audioStatus": "Waiting for exact game mixer"
        }))?;
        let (w, pins) = build_intro_window(&self.app, &self.root, &bootstrap)?;
        self._profile_pins = pins;
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
        w.set_position(tauri::PhysicalPosition::new(monitor.x, monitor.y))?;
        w.set_size(tauri::PhysicalSize::new(monitor.width, monitor.height))?;
        self.set_volume(volume)
    }
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
        let Some(owned) = self.owned.as_mut() else {
            return Ok(Observation::Window(None));
        };
        let sample = owned.observe()?;
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
        let own = self
            .owned
            .as_mut()
            .map(|o| o.observe())
            .transpose()?
            .unwrap_or(Observation::Window(None));
        let mut foreground_pid = 0;
        if hwnd != 0 {
            unsafe {
                GetWindowThreadProcessId(HWND(hwnd as *mut _), Some(&mut foreground_pid));
            }
        }
        let allowed = crate::startup_video_policy::playback_foreground_owned(
            hwnd,
            self.window_hwnd,
            foreground_pid,
            self.game.as_ref().map(|g| g.key.pid),
            self.owned.is_some() && !matches!(own, Observation::GameEnded),
        );
        if !allowed {
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
            .as_mut()
            .context("Owned game unavailable")?
            .verify_window(window.window.hwnd)?
            .context("Exact game window disappeared before intro creation")?;
        ensure!(
            fresh == window,
            "Game display/window changed before intro placement"
        );
        self.game_hwnd = window.window.hwnd;
        self.create_at(window.monitor, volume)
    }
    fn create_owner(&mut self, owner: u64, volume: Volume) -> Result<()> {
        let main = self
            .app
            .get_webview_window("main")
            .context("Launcher closed")?;
        ensure!(main.hwnd()?.0 as u64 == owner, "Launcher window changed");
        let monitor = main
            .current_monitor()?
            .context("Launcher monitor unavailable")?;
        self.create_at(
            crate::startup_video_native::Bounds {
                x: monitor.position().x,
                y: monitor.position().y,
                width: monitor.size().width,
                height: monitor.size().height,
            },
            volume,
        )
    }
    fn bind_game(&mut self, game: ExpectedGame) -> Result<()> {
        ensure!(
            self.game.is_none() && self.owned.is_none(),
            "Game already attached"
        );
        self.owned = Some(OwnedGame::retain(game.clone())?);
        self.game = Some(game);
        Ok(())
    }
    fn focus_owner(&mut self, owner: u64) -> Result<()> {
        if !self.return_owned {
            return Ok(());
        }
        if let Some(main) = self.app.get_webview_window("main") {
            let hwnd = main.hwnd()?.0 as u64;
            let current = unsafe { GetForegroundWindow() }.0 as u64;
            if hwnd == owner && current == self.window_hwnd {
                let _ = main.set_focus();
            }
        }
        Ok(())
    }
    fn start_audio(&mut self) -> Result<()> {
        ensure!(self.audio.is_none(), "Intro already has an audio guardian");
        self.audio = Some(OwnedAudio::start(
            self.game
                .as_ref()
                .context("No exact launched game for audio")?,
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
        let w = self.window.as_ref().context("Owned intro missing")?;
        // Launching/building the intro can return activation to this launcher's
        // main/overlay window. These exact owned HWNDs are not an Alt-Tab away.
        // Windows can also report NULL briefly during activation; wait boundedly
        // for that gap, but never treat NULL or an unrelated app as authorization.
        let own_hwnd = |label: &str| {
            self.app
                .get_webview_window(label)
                .and_then(|view| view.hwnd().ok())
                .map(|hwnd| hwnd.0 as u64)
                .unwrap_or(0)
        };
        let main = own_hwnd("main");
        let overlay = own_hwnd("live-overlay");
        let deadline = Instant::now() + Duration::from_millis(500);
        let foreground = loop {
            let hwnd = unsafe { GetForegroundWindow() }.0 as u64;
            if hwnd != 0 || Instant::now() >= deadline {
                break hwnd;
            }
            thread::sleep(Duration::from_millis(16));
        };
        let mut foreground_pid = 0;
        if foreground != 0 {
            unsafe {
                GetWindowThreadProcessId(HWND(foreground as *mut _), Some(&mut foreground_pid));
            }
        }
        // Revalidate after the activation gap. The game may replace its startup
        // window with its real render window; only the retained game identity
        // can authorize the new HWND and monitor.
        // Playback is owned by the Play click, not by game-window readiness.
        // A late game binding may authorize its current foreground HWND, but
        // absence of a game window cannot delay the intro.
        if let (Some(game), Some(owned)) = (&self.game, &mut self.owned) {
            if foreground_pid == game.key.pid {
                if let Some(window) = owned.verify_window(foreground)? {
                    self.game_hwnd = window.window.hwnd;
                }
            }
        }
        let allowed = crate::startup_video_policy::launch_foreground_owned(
            foreground,
            self.game_hwnd,
            self.window_hwnd,
            main,
            overlay,
        );
        append_trace(
            &self.root,
            &self.session,
            "foreground-before-play",
            &format!(
                "foreground={foreground:#x} pid={foreground_pid} game={:#x} intro={:#x} main={main:#x} overlay={overlay:#x} allowed={allowed}",
                self.game_hwnd, self.window_hwnd
            ),
        );
        ensure!(
            allowed,
            "Startup video cancelled because another application has focus (HWND {foreground:#x}, PID {foreground_pid})"
        );
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
            if let Ok(mut state) = self.view.lock() {
                if state
                    .message
                    .starts_with("Owned intro window cleanup pending;")
                {
                    state.message = "Startup video closed".into();
                }
            }
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
        if !self.return_owned || self.game.as_ref().is_none_or(|g| game != g.key) {
            return Ok(());
        }
        let current = unsafe { GetForegroundWindow() }.0 as u64;
        let owned = self.owned.as_mut().context("Owned game unavailable")?;
        if let Observation::Window(Some(window)) = owned.observe()? {
            let hwnd = window.window.hwnd;
            if (current == self.window_hwnd || current == hwnd)
                && owned.verify_window(hwnd)?.is_some()
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
        append_trace(&self.root, &self.session, "diagnostic", message);
        if let Ok(mut state) = self.view.lock() {
            // A normal asynchronous destroy ACK must not erase the playback
            // failure that caused cleanup. Keep all transitions in the trace.
            if !message.starts_with("Owned intro window cleanup pending;")
                || state.message.is_empty()
            {
                state.message = message.chars().take(2048).collect();
            }
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
