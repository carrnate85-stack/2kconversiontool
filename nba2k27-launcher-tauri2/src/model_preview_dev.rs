//! Staging bridge for the release-intended 2K Explorer renderer worker.
//!
//! The service is deliberately compiled only with `dev-model-preview`. It is
//! read-only: one canonical loose IFF inside the active mod-library root is
//! decoded while the installed-game catalog supplies referenced dependencies.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::windows::process::CommandExt,
    path::{Component, Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

const WORKER: &str = "NBA2K27-ModelPreviewWorker.exe";
const SHARED_RENDERER_REVISION: &str =
    "4CBC0AB8F699DCA4D5B56D2FF1E926008461B9C7E086921D977ABD4AAE4ECB15";
const MAX_REQUEST: usize = 1024 * 1024 + 1024;
const MAX_REPLY: usize = 256 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(120);
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

struct WorkerHandle {
    child: Mutex<Child>,
    writer: Arc<Mutex<ChildStdin>>,
    replies: Mutex<mpsc::Receiver<std::result::Result<Value, String>>>,
    progress: Arc<Mutex<Option<Value>>>,
}
impl Drop for WorkerHandle {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[derive(Default)]
struct State {
    worker: Option<Arc<WorkerHandle>>,
    loaded_game: Option<PathBuf>,
    active_session: Option<String>,
    next_id: u64,
}

pub struct Service {
    portable: PathBuf,
    state: Mutex<State>,
    serial: Mutex<()>,
}

impl Service {
    pub fn new(portable: &Path) -> Self {
        Self {
            portable: portable.to_path_buf(),
            state: Mutex::new(State::default()),
            serial: Mutex::new(()),
        }
    }

    fn executable() -> Result<PathBuf> {
        let app = std::env::current_exe()?
            .parent()
            .context("Launcher executable folder missing")?
            .to_path_buf();
        let worker = app.join("tools").join(WORKER);
        ensure!(
            worker.is_file(),
            "Model preview worker is not installed in this test build"
        );
        Ok(worker)
    }

    fn spawn(&self) -> Result<Arc<WorkerHandle>> {
        fs::create_dir_all(self.portable.join("Logs"))?;
        let stderr = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.portable.join("Logs/model-preview-worker.log"))?;
        let executable = Self::executable()?;
        let mut child = Command::new(&executable)
            .arg("--asset-worker")
            .current_dir(executable.parent().unwrap())
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(stderr))
            .spawn()
            .context("Could not start the model preview worker")?;
        let writer = Arc::new(Mutex::new(
            child.stdin.take().context("Worker input unavailable")?,
        ));
        let mut output = child.stdout.take().context("Worker output unavailable")?;
        let (send, replies) = mpsc::channel();
        let progress = Arc::new(Mutex::new(None));
        let progress_reader = progress.clone();
        std::thread::Builder::new()
            .name("model-preview-worker-output".into())
            .spawn(move || {
                loop {
                    let mut header = [0u8; 4];
                    if let Err(error) = output.read_exact(&mut header) {
                        let _ = send.send(Err(format!("Preview worker stopped: {error}")));
                        break;
                    }
                    let length = u32::from_le_bytes(header) as usize;
                    if length == 0 || length > MAX_REPLY {
                        let _ = send.send(Err("Preview worker returned an invalid frame".into()));
                        break;
                    }
                    let mut body = vec![0u8; length];
                    if let Err(error) = output.read_exact(&mut body) {
                        let _ =
                            send.send(Err(format!("Preview worker reply was incomplete: {error}")));
                        break;
                    }
                    match serde_json::from_slice::<Value>(&body) {
                        Ok(value) => {
                            if value["kind"] == "event" && value["channel"] == "progress" {
                                if let Ok(mut latest) = progress_reader.lock() {
                                    *latest = Some(value["payload"].clone());
                                }
                                continue;
                            }
                            if send.send(Ok(value)).is_err() {
                                break;
                            }
                        }
                        Err(error) => {
                            let _ = send
                                .send(Err(format!("Preview worker reply was invalid: {error}")));
                            break;
                        }
                    }
                }
            })?;
        Ok(Arc::new(WorkerHandle {
            child: Mutex::new(child),
            writer,
            replies: Mutex::new(replies),
            progress,
        }))
    }

    fn handle(&self) -> Result<Arc<WorkerHandle>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Model preview state unavailable"))?;
        if state.worker.is_none() {
            state.worker = Some(self.spawn()?);
            state.loaded_game = None;
            state.active_session = None;
        }
        Ok(state.worker.as_ref().unwrap().clone())
    }

    fn write(writer: &Arc<Mutex<ChildStdin>>, value: &Value) -> Result<()> {
        let bytes = serde_json::to_vec(value)?;
        ensure!(
            bytes.len() <= MAX_REQUEST,
            "Model preview request is too large"
        );
        let mut writer = writer
            .lock()
            .map_err(|_| anyhow::anyhow!("Model preview worker input unavailable"))?;
        writer.write_all(&(bytes.len() as u32).to_le_bytes())?;
        writer.write_all(&bytes)?;
        writer.flush()?;
        Ok(())
    }

    fn request(&self, channel: &str, args: Vec<Value>) -> Result<Value> {
        let handle = self.handle()?;
        if let Ok(mut progress) = handle.progress.lock() {
            *progress = None;
        }
        let id = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("Model preview state unavailable"))?;
            state.next_id = state.next_id.wrapping_add(1).max(1);
            state.next_id
        };
        Self::write(
            &handle.writer,
            &json!({"id":id,"channel":channel,"args":args}),
        )?;
        let replies = handle
            .replies
            .lock()
            .map_err(|_| anyhow::anyhow!("Model preview reply queue unavailable"))?;
        loop {
            let message = replies
                .recv_timeout(TIMEOUT)
                .context("Model preview worker timed out")?
                .map_err(anyhow::Error::msg)?;
            if message["kind"] != "reply" || message["id"].as_u64() != Some(id) {
                continue;
            }
            ensure!(
                message["ok"] == true,
                "{}",
                message["error"].as_str().unwrap_or("Model preview failed")
            );
            return Ok(message["value"].clone());
        }
    }

    fn reset(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.worker = None;
            state.loaded_game = None;
            state.active_session = None;
        }
    }

    fn load_game(&self, game_root: &Path) -> Result<()> {
        let canonical = game_root
            .canonicalize()
            .context("NBA 2K27 folder is not accessible")?;
        let loaded = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Model preview state unavailable"))?
            .loaded_game
            .as_ref()
            == Some(&canonical);
        if loaded {
            return Ok(());
        }
        self.request("game:load", vec![json!(canonical)])?;
        self.state
            .lock()
            .map_err(|_| anyhow::anyhow!("Model preview state unavailable"))?
            .loaded_game = Some(canonical);
        Ok(())
    }

    pub fn prepare(&self, game_root: &Path) -> Result<Value> {
        let _serial = self
            .serial
            .lock()
            .map_err(|_| anyhow::anyhow!("Model preview queue unavailable"))?;
        self.load_game(game_root)?;
        Ok(json!({
            "ready": true,
            "renderer": "NBA2K27-Exporter shared model preview",
            "rendererRevision": SHARED_RENDERER_REVISION
        }))
    }

    fn selected(mods_root: &Path, relative: &str) -> Result<(PathBuf, PathBuf, String)> {
        ensure!(relative.len() <= 1024, "Mod path is too long");
        let relative_path = Path::new(relative);
        ensure!(
            relative_path.components().count() > 0
                && relative_path
                    .components()
                    .all(|part| matches!(part, Component::Normal(_))),
            "Invalid mod path"
        );
        let root = mods_root
            .canonicalize()
            .context("The active mod folder does not exist")?;
        let file = root
            .join(relative_path)
            .canonicalize()
            .context("The selected mod no longer exists; scan again")?;
        ensure!(
            file.starts_with(&root) && file.is_file(),
            "The selected mod is outside the active mod folder"
        );
        let lower = file
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        ensure!(
            lower.ends_with(".iff") || lower.ends_with(".iff.bin"),
            "This Mod Library entry is not a 3D IFF resource"
        );
        let virtual_name = relative.replace('\\', "/");
        Ok((root, file, virtual_name))
    }

    pub fn load(
        &self,
        game_root: &Path,
        mods_root: &Path,
        relative: &str,
        config: Option<&str>,
    ) -> Result<Value> {
        let _serial = self
            .serial
            .lock()
            .map_err(|_| anyhow::anyhow!("Model preview queue unavailable"))?;
        let (root, file, virtual_name) = Self::selected(mods_root, relative)?;
        if let Some(config) = config {
            ensure!(config.len() <= 128, "Player configuration name is too long");
        }
        let operation = || -> Result<Value> {
            self.load_game(game_root)?;
            let mut args = vec![json!(file), json!(root), json!(virtual_name)];
            if let Some(config) = config {
                args.push(json!(config));
            }
            self.request("preview:load-loose", args)
        };
        let payload = match operation() {
            Ok(value) => value,
            Err(first) => {
                self.reset();
                operation().with_context(|| format!("Initial preview attempt: {first:#}"))?
            }
        };
        let session = crate::backend::unique_id();
        self.state
            .lock()
            .map_err(|_| anyhow::anyhow!("Model preview state unavailable"))?
            .active_session = Some(session.clone());
        Ok(json!({
            "session": session,
            "rendererRevision": SHARED_RENDERER_REVISION,
            "payload": payload
        }))
    }

    pub fn arena_textures(&self, session: &str, cursor: usize) -> Result<Value> {
        let _serial = self
            .serial
            .lock()
            .map_err(|_| anyhow::anyhow!("Model preview queue unavailable"))?;
        let active = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Model preview state unavailable"))?
            .active_session
            .as_deref()
            == Some(session);
        ensure!(active, "This model preview was replaced; reopen it");
        self.request(
            "preview:material-textures",
            vec![json!(usize::MAX), json!(cursor)],
        )
    }

    pub fn cancel(&self) -> Result<Value> {
        let worker = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Model preview state unavailable"))?
            .worker
            .clone();
        let running = worker.is_some();
        if let Some(worker) = worker {
            Self::write(&worker.writer, &json!({"channel":"cancel","args":[]}))?;
        }
        Ok(json!({"cancelled":running}))
    }

    pub fn status(&self) -> Result<Value> {
        let (worker, game_loaded) = {
            let state = self
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("Model preview state unavailable"))?;
            (state.worker.clone(), state.loaded_game.is_some())
        };
        let progress = worker
            .as_ref()
            .and_then(|worker| worker.progress.lock().ok()?.clone());
        Ok(json!({
            "stagedTestBuild": true,
            "workerRunning": worker.is_some(),
            "gameLoaded": game_loaded,
            "renderer": "/model-preview-dev/",
            "reference": "NBA2K27-Exporter shared model preview",
            "rendererRevision": SHARED_RENDERER_REVISION,
            "progress": progress
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_traversal_before_touching_the_worker() {
        let root = std::env::temp_dir().join(format!("model-preview-path-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        assert!(Service::selected(&root, "../outside.iff").is_err());
        assert!(Service::selected(&root, "C:\\outside.iff").is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
