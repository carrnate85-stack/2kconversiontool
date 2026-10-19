//! Launch-owned MP4 coordinator. Selection is validated and retained before
//! Steam dispatch; only the exact resulting native session can supply a game.
//! No renderer PID, path or remembered process identity authorizes playback.
use crate::{
    backend::Backend,
    startup_video_media::VideoLease,
    startup_video_tauri::{Binding, Request, Runtime},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

pub(crate) struct Coordinator {
    app: tauri::AppHandle,
    root: PathBuf,
    runtime: Mutex<Option<Arc<Runtime>>>,
    waiting: Mutex<Option<Arc<AtomicBool>>>,
    stopped: AtomicBool,
    message: Mutex<Value>,
}
impl Coordinator {
    pub fn new(app: tauri::AppHandle, root: PathBuf) -> Self {
        Self {
            app,
            root,
            runtime: Mutex::new(None),
            waiting: Mutex::new(None),
            stopped: AtomicBool::new(false),
            message: Mutex::new(json!({"phase":"idle","finished":true})),
        }
    }
    fn start_bound(&self, binding: Binding, media: VideoLease, cancel: &AtomicBool) -> Result<()> {
        let mut runtime = self
            .runtime
            .lock()
            .map_err(|_| anyhow::anyhow!("Intro service unavailable"))?;
        ensure!(
            !self.stopped.load(Ordering::Acquire) && !cancel.load(Ordering::Acquire),
            "Launcher is closing"
        );
        if runtime.is_none() {
            let helper = crate::startup_video_helper::prepare(
                &self.root,
                include_bytes!("../assets/startup-audio-guardian.exe"),
                include_str!("../assets/startup-audio-guardian.sha256").trim(),
            )?;
            *runtime = Some(Arc::new(Runtime::new(
                self.app.clone(),
                self.root.clone(),
                Arc::new(helper),
            )));
        }
        ensure!(
            !self.stopped.load(Ordering::Acquire) && !cancel.load(Ordering::Acquire),
            "Launcher is closing"
        );
        // Serialized with shutdown: it either sees this newly started runtime
        // and cancels it, or this admission observes the stop and never starts.
        runtime.as_ref().unwrap().start(binding, media)?;
        Ok(())
    }
    pub fn arm(
        self: &Arc<Self>,
        backend: Weak<Backend>,
        session: String,
        media: VideoLease,
    ) -> Result<()> {
        ensure!(
            session.len() == 48 && session.bytes().all(|b| b.is_ascii_hexdigit()),
            "Launch did not return an owned session"
        );
        ensure!(!self.stopped.load(Ordering::Acquire), "Launcher is closing");
        let mut waiting = self
            .waiting
            .lock()
            .map_err(|_| anyhow::anyhow!("Intro launch ownership unavailable"))?;
        ensure!(
            waiting.as_ref().is_none_or(|v| v.load(Ordering::Acquire)),
            "An intro launch is already waiting"
        );
        let cancel = Arc::new(AtomicBool::new(false));
        *waiting = Some(cancel.clone());
        let owner = Arc::downgrade(self);
        *self
            .message
            .lock()
            .map_err(|_| anyhow::anyhow!("Intro status unavailable"))? =
            json!({"phase":"waiting-for-launched-game","finished":false});
        let spawned = thread::Builder::new().name("startup-launch-owner".into()).spawn(move || {
            let result = (|| -> Result<()> {
                let started = Instant::now();
                while !cancel.load(Ordering::Acquire) && started.elapsed() < Duration::from_secs(120) {
                    let coordinator = owner.upgrade().context("Launcher closed before intro")?;
                    ensure!(!coordinator.stopped.load(Ordering::Acquire), "Launcher closed before intro");
                    let backend = backend.upgrade().context("Launch owner closed before intro")?;
                    if let Some((identity, _root)) = backend.session.intro_binding(&session)? {
                        let binding = Binding::from_launched(&session, identity.pid, identity.started.parse()?, std::path::Path::new(&identity.path))?;
                        ensure!(!cancel.load(Ordering::Acquire), "Intro cancelled before binding");
                        coordinator.start_bound(binding, media, &cancel)?;
                        return Ok(());
                    }
                    drop(backend);
                    drop(coordinator);
                    thread::sleep(Duration::from_millis(100));
                }
                anyhow::bail!("Startup video cancelled or timed out waiting for its launched game")
            })();
            if let Some(owner) = owner.upgrade() {
                if let Err(error) = result {
                    if let Ok(mut message) = owner.message.lock() {
                        *message = json!({"phase":"not-played","finished":true,"message":format!("{error:#}")});
                    }
                }
            }
            cancel.store(true, Ordering::Release);
        });
        if let Err(error) = spawned {
            if let Some(cancel) = waiting.take() {
                cancel.store(true, Ordering::Release);
            }
            if let Ok(mut message) = self.message.lock() {
                *message =
                    json!({"phase":"not-played","finished":true,"message":error.to_string()});
            }
            return Err(error.into());
        }
        Ok(())
    }
    pub fn ensure_available(&self) -> Result<()> {
        ensure!(!self.stopped.load(Ordering::Acquire), "Launcher is closing");
        let waiting = self
            .waiting
            .lock()
            .map_err(|_| anyhow::anyhow!("Intro launch ownership unavailable"))?;
        ensure!(
            waiting.as_ref().is_none_or(|v| v.load(Ordering::Acquire)),
            "An intro launch is still waiting"
        );
        if let Some(runtime) = self
            .runtime
            .lock()
            .map_err(|_| anyhow::anyhow!("Intro restoration unavailable"))?
            .as_ref()
        {
            ensure!(
                runtime.status()["finished"] == true,
                "Previous startup intro/audio restoration needs to finish before another Play"
            );
        }
        Ok(())
    }
    pub fn request(&self, window: &tauri::WebviewWindow, request: Request) -> Result<Value> {
        let runtime = self
            .runtime
            .lock()
            .map_err(|_| anyhow::anyhow!("Intro unavailable"))?
            .clone()
            .context("No owned startup intro")?;
        runtime.request(window, request)
    }
    pub fn status(&self) -> Value {
        if self
            .waiting
            .lock()
            .is_ok_and(|v| v.as_ref().is_some_and(|w| !w.load(Ordering::Acquire)))
        {
            return self
                .message
                .lock()
                .map(|v| v.clone())
                .unwrap_or_else(|_| json!({"phase":"waiting-for-launched-game","finished":false}));
        }
        if let Ok(runtime) = self.runtime.lock() {
            if let Some(runtime) = runtime.as_ref() {
                let status = runtime.status();
                let latest = self
                    .message
                    .lock()
                    .map(|v| v.clone())
                    .unwrap_or(Value::Null);
                if status["finished"] == true && latest["phase"] == "not-played" {
                    return latest;
                }
                return status;
            }
        }
        self.message
            .lock()
            .map(|v| v.clone())
            .unwrap_or_else(|_| json!({"phase":"unavailable","finished":true}))
    }
    pub fn shutdown(&self) {
        self.stopped.store(true, Ordering::Release);
        if let Ok(waiting) = self.waiting.lock() {
            if let Some(cancel) = waiting.as_ref() {
                cancel.store(true, Ordering::Release);
            }
        }
        if let Ok(runtime) = self.runtime.lock() {
            if let Some(runtime) = runtime.as_ref() {
                runtime.shutdown();
            }
        }
    }
}
impl Drop for Coordinator {
    fn drop(&mut self) {
        self.shutdown();
    }
}
