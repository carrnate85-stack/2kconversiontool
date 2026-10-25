use crate::catalog_cache::CachedCatalog;
use crate::preview_jobs::{Kind as PreviewKind, PreviewJobs};
use crate::runtime;
#[path = "accessory_recovery.rs"]
pub(crate) mod accessory_recovery;
#[path = "blender_snapshot.rs"]
mod blender_snapshot;
use crate::worker_diagnostics::{Failure as WorkerFailure, push_tail};
use anyhow::{Context, Result, bail, ensure};
use blender_snapshot::{Snapshots, ValueOrBytes};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Write},
    os::windows::{io::AsRawHandle, process::CommandExt},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::{JobObjects::*, Threading::CREATE_NO_WINDOW},
};

struct Job(HANDLE, AtomicBool, Option<String>, Option<PreviewKind>);
unsafe impl Send for Job {}
unsafe impl Sync for Job {}
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
impl Job {
    #[cfg(test)]
    fn new() -> Result<Self> {
        Self::for_request(None)
    }
    #[cfg(test)]
    fn for_request(request_id: Option<String>) -> Result<Self> {
        Self::for_preview_request(request_id, None)
    }
    fn for_preview_request(
        request_id: Option<String>,
        preview_kind: Option<PreviewKind>,
    ) -> Result<Self> {
        unsafe {
            let job = Self(
                CreateJobObjectW(None, None)?,
                AtomicBool::new(false),
                request_id,
                preview_kind,
            );
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                std::mem::size_of_val(&limits) as u32,
            )?;
            Ok(job)
        }
    }
    fn cancel(&self) {
        self.1.store(true, Ordering::SeqCst);
        self.terminate();
    }
    fn terminate(&self) {
        unsafe {
            let _ = TerminateJobObject(self.0, 1);
        }
    }
}

type ActiveJobs = Mutex<HashMap<u64, (String, Arc<Job>)>>;
struct ActiveRegistration<'a> {
    jobs: &'a ActiveJobs,
    id: u64,
}
impl Drop for ActiveRegistration<'_> {
    fn drop(&mut self) {
        self.jobs.lock().unwrap().remove(&self.id);
    }
}

pub struct Backend {
    pub data: PathBuf,
    pub session: PathBuf,
    engine: Mutex<Option<runtime::PreparedRuntime>>,
    active: ActiveJobs,
    mutation: RwLock<()>,
    catalog: Mutex<HashMap<String, CachedCatalog>>,
    assets: Mutex<crate::assets::Assets>,
    preview_jobs: PreviewJobs,
    blender_snapshots: Snapshots,
    accessory_recovery: Mutex<accessory_recovery::Recovery>,
    next: AtomicU64,
    closing: AtomicBool,
    cancellation: AtomicU64,
    command_cancellations: Mutex<HashMap<String, u64>>,
}

impl Backend {
    pub fn new() -> Result<Arc<Self>> {
        let data = runtime::data_root()?;
        runtime::cleanup_dead_sessions(&data)?;
        let session = data.join("Sessions").join(runtime::stamp());
        fs::create_dir_all(&session)?;
        Ok(Arc::new(Self {
            data,
            session,
            engine: Mutex::new(None),
            active: Mutex::new(HashMap::new()),
            mutation: RwLock::new(()),
            catalog: Mutex::new(HashMap::new()),
            assets: Mutex::new(crate::assets::Assets::new(Arc::new(AtomicBool::new(false)))),
            preview_jobs: PreviewJobs::default(),
            blender_snapshots: Snapshots::default(),
            accessory_recovery: Mutex::new(accessory_recovery::Recovery::default()),
            next: AtomicU64::new(1),
            closing: AtomicBool::new(false),
            cancellation: AtomicU64::new(0),
            command_cancellations: Mutex::new(HashMap::new()),
        }))
    }

    pub fn engine(&self, event: &mut (impl FnMut(Value) + ?Sized)) -> Result<PathBuf> {
        let mut engine = self
            .engine
            .lock()
            .map_err(|_| anyhow::anyhow!("Runtime lock failed"))?;
        if engine.is_none() {
            *engine = Some(runtime::prepare(&self.data, |message| {
                event(json!({"kind":"event","message":message}))
            })?);
        }
        if let Some(runtime) = engine.as_ref() {
            for message in runtime.maintenance_events() {
                event(json!({"kind":"event","message":message}));
            }
        }
        engine
            .as_ref()
            .map(|engine| engine.backend.clone())
            .context("Conversion engine is unavailable")
    }

    pub fn python(
        &self,
        command: &str,
        mut payload: Value,
        mut event: impl FnMut(Value),
    ) -> Result<Value> {
        ensure!(
            allowed_command(command),
            "Unsupported prototype command: {command}"
        );
        ensure!(
            !self.closing.load(Ordering::SeqCst),
            "The prototype is closing"
        );
        let cancellation = self.cancellation.load(Ordering::SeqCst);
        let command_cancellation = self.command_cancellation(command);
        let request_id = payload
            .as_object_mut()
            .and_then(|object| object.remove("_cmtJobId"))
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .context("Invalid conversion job ID")
            })
            .transpose()?;
        ensure!(
            request_id
                .as_ref()
                .is_none_or(|id| !id.is_empty() && id.len() <= 64),
            "Invalid conversion job ID"
        );
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let preview_kind = matches!(command, "preview-resources" | "native-hair-preview")
            .then(|| PreviewKind::request(command, &payload));
        let preview_ticket = preview_kind.map(|kind| self.preview_jobs.ticket(kind));
        let job = Arc::new(Job::for_preview_request(request_id.clone(), preview_kind)?);
        self.active
            .lock()
            .unwrap()
            .insert(id, (command.to_owned(), job.clone()));
        let _registration = ActiveRegistration {
            jobs: &self.active,
            id,
        };
        if command == "run-swap" {
            event(
                json!({"kind":"event","command":command,"type":"job-started","jobId":request_id}),
            );
        }
        let root = self.engine(&mut event)?;
        let _settings_write = if matches!(command, "save-settings" | "health-repair")
            || (command == "bootstrap" && !self.data.join("settings.json").exists())
        {
            Some(runtime::settings_write_guard(&self.data)?)
        } else {
            None
        };
        ensure!(
            !self.closing.load(Ordering::SeqCst)
                && self.cancellation.load(Ordering::SeqCst) == cancellation
                && self.command_cancellation(command) == command_cancellation
                && !job.1.load(Ordering::SeqCst),
            "Operation cancelled"
        );
        ensure!(
            preview_ticket.is_none_or(|ticket| self.preview_jobs.current(ticket)),
            "Operation cancelled"
        );
        if matches!(command, "inspect" | "extract") {
            payload["stageRoot"] = json!(self.session.to_string_lossy());
        }
        let mut process = Command::new(root.join("python_runtime/python.exe"));
        process
            .arg("-I")
            .arg("-B")
            .arg(root.join("prototype_bridge.py"))
            .arg(command)
            .current_dir(&root)
            .env("CHARACTER_MOD_TOOL_ROOT", &root)
            .env("CMT_RUST_DATA", &self.data)
            .env("CMT_SESSION", &self.session)
            .env("CMT_RUST_EXE", std::env::current_exe()?)
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .env_remove("CHARACTER_MOD_NODE")
            .env("TEMP", &self.session)
            .env("TMP", &self.session)
            .creation_flags(CREATE_NO_WINDOW.0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = process
            .spawn()
            .context("Could not start the bundled Python engine")?;
        if let Err(error) =
            unsafe { AssignProcessToJobObject(job.0, HANDLE(child.as_raw_handle())) }
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error.into());
        }
        {
            let _active = self.active.lock().unwrap();
            if self.closing.load(Ordering::SeqCst)
                || self.cancellation.load(Ordering::SeqCst) != cancellation
                || self.command_cancellation(command) != command_cancellation
                || job.1.load(Ordering::SeqCst)
                || preview_ticket.is_some_and(|ticket| !self.preview_jobs.current(ticket))
            {
                job.cancel();
            }
        }
        let stderr = child.stderr.take().context("Missing worker stderr")?;
        let error_reader = std::thread::spawn(move || {
            let mut tail = String::new();
            for line in BufReader::new(stderr)
                .lines()
                .map_while(std::result::Result::ok)
            {
                push_tail(&mut tail, &line, 4096);
                push_tail(&mut tail, "\n", 4096);
            }
            tail
        });
        let mut unrecognized_stdout = String::new();
        let parsed = (|| -> Result<Value> {
            let mut input = child.stdin.take().context("Missing worker stdin")?;
            input.write_all(serde_json::to_string(&payload)?.as_bytes())?;
            drop(input);
            let output = child.stdout.take().context("Missing worker stdout")?;
            let mut result = None;
            for line in BufReader::new(output).lines() {
                let line = line?;
                ensure!(
                    line.len() < 32 * 1024 * 1024,
                    "Worker response exceeded the protocol limit"
                );
                if let Some(value) = line.strip_prefix("CMT_EVENT|") {
                    let row: Value = serde_json::from_str(value)?;
                    let mut row = row;
                    row["kind"] = json!("event");
                    row["command"] = json!(command);
                    if let Some(request_id) = &request_id {
                        row["jobId"] = json!(request_id);
                    }
                    event(row);
                } else if let Some(value) = line.strip_prefix("CMT_RESULT|") {
                    result = Some(serde_json::from_str::<Value>(value)?);
                } else {
                    push_tail(&mut unrecognized_stdout, &line, 2048);
                    push_tail(&mut unrecognized_stdout, "\n", 2048);
                }
            }
            result.context("The conversion engine ended without a result")
        })();
        let terminated_after_protocol_failure =
            parsed.is_err() && !matches!(child.try_wait(), Ok(Some(_)));
        if terminated_after_protocol_failure {
            job.terminate();
        }
        let status = child.wait();
        let errors = error_reader.join().unwrap_or_default();
        ensure!(!job.1.load(Ordering::SeqCst), "Operation cancelled");
        let failure = |reason: String| {
            let failure = WorkerFailure::new(
                command,
                &reason,
                &status,
                terminated_after_protocol_failure,
                &errors,
                &unrecognized_stdout,
            );
            failure.write_latest(&self.data);
            anyhow::anyhow!(failure.message())
        };
        let result = parsed.map_err(|error| failure(format!("{error:#}")))?;
        if let Some(error) = result.get("error").and_then(Value::as_str) {
            bail!("{error}");
        }
        if !status.as_ref().is_ok_and(|status| status.success()) {
            return Err(failure(
                "Conversion worker exited unsuccessfully".to_owned(),
            ));
        }
        Ok(result)
    }

    pub fn invoke(
        &self,
        command: &str,
        payload: Value,
        mut event: impl FnMut(Value),
    ) -> Result<Value> {
        let result = self.invoke_inner(command, payload, &mut event);
        if result.is_err() && changes_player_files(command) {
            self.assets.lock().unwrap().player_files_changed();
        }
        result
    }

    fn player_read<'a>(
        &'a self,
        payload: &Value,
        event: &mut dyn FnMut(Value),
    ) -> Result<std::sync::RwLockReadGuard<'a, ()>> {
        loop {
            let read = self
                .mutation
                .read()
                .map_err(|_| anyhow::anyhow!("Engine lock failed"))?;
            let resolved = self.assets.lock().unwrap().recovery_payload(payload);
            if self
                .accessory_recovery
                .lock()
                .unwrap()
                .pending(&resolved)?
                .is_empty()
            {
                return Ok(read);
            }
            drop(read);
            let _write = self.mutation.try_write().map_err(|_| {
                anyhow::anyhow!(
                    "Another edit is running. Finish it before recovering accessory files."
                )
            })?;
            let targets = self.accessory_recovery.lock().unwrap().pending(&resolved)?;
            for target in targets {
                let recovered = self.python(
                    "accessory-recover-player",
                    json!({"target":target}),
                    &mut *event,
                );
                // Even failed rollback can change files before a later conflict is detected.
                self.assets.lock().unwrap().player_files_changed();
                recovered?;
                self.accessory_recovery.lock().unwrap().remember(&target)?;
            }
        }
    }

    fn native_accessory_catalog(&self, kind: &str, event: &mut dyn FnMut(Value)) -> Result<Value> {
        let command = "native-hair-catalog";
        let cancellation = self.cancellation.load(Ordering::SeqCst);
        let command_cancellation = self.command_cancellation(command);
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let job = Arc::new(Job::for_preview_request(None, None)?);
        self.active
            .lock()
            .unwrap()
            .insert(id, (command.into(), job.clone()));
        let _registration = ActiveRegistration {
            jobs: &self.active,
            id,
        };
        let root = self.engine(event)?;
        let mut assets = self
            .assets
            .lock()
            .map_err(|_| anyhow::anyhow!("Native catalog lock failed"))?;
        ensure!(
            !self.closing.load(Ordering::SeqCst)
                && self.cancellation.load(Ordering::SeqCst) == cancellation
                && self.command_cancellation(command) == command_cancellation
                && !job.1.load(Ordering::SeqCst),
            "Operation cancelled"
        );
        event(json!({"kind":"event","command":command,"type":"progress",
            "message":format!("Reading NBA 2K27 {kind} manifest assets...")}));
        assets.configure(&self.data)?;
        let value =
            assets.accessory_catalog(kind, &root.join("hair_tools/player_names.json"), &job.1)?;
        ensure!(
            !self.closing.load(Ordering::SeqCst)
                && self.cancellation.load(Ordering::SeqCst) == cancellation
                && self.command_cancellation(command) == command_cancellation
                && !job.1.load(Ordering::SeqCst),
            "Operation cancelled"
        );
        Ok(value)
    }

    fn with_preview_assets<T>(
        &self,
        command: &str,
        payload: &Value,
        configure: bool,
        read: impl FnOnce(&mut crate::assets::Assets) -> Result<T>,
    ) -> Result<T> {
        let ticket = self
            .preview_jobs
            .ticket(PreviewKind::request(command, payload));
        let mut assets = self
            .assets
            .lock()
            .map_err(|_| anyhow::anyhow!("Native preview cache failed"))?;
        ensure!(
            !self.closing.load(Ordering::SeqCst),
            "The prototype is closing"
        );
        let registration = self.preview_jobs.begin(ticket)?;
        assets.cancel = registration.cancel.clone();
        if configure {
            assets.configure(&self.data)?;
        }
        read(&mut assets)
    }

    pub fn preview_binary(&self, command: &str, payload: &Value) -> Result<Vec<u8>> {
        ensure!(
            matches!(
                command,
                "read-archive" | "read-file" | "native-catalog-geometry"
            ),
            "Unsupported binary preview command"
        );
        if let Some(uri) = payload["path"].as_str().filter(|s| Snapshots::owns_uri(s)) {
            ensure!(
                !self.closing.load(Ordering::SeqCst),
                "The prototype is closing"
            );
            return match self.blender_snapshots.read(command, uri, true)? {
                ValueOrBytes::Bytes(bytes) => Ok(bytes),
                _ => bail!("Invalid Blender snapshot binary response"),
            };
        }
        let keys: &[&str] = if command == "native-catalog-geometry" {
            &["geometryPath", "sourcePlayerPath"]
        } else {
            &["path"]
        };
        for key in keys {
            ensure!(
                payload[*key]
                    .as_str()
                    .is_some_and(|uri| uri.starts_with("cmt-preview://")),
                "Binary preview requires registered resources"
            );
        }
        let _read = self.player_read(payload, &mut |_| {})?;
        self.with_preview_assets(command, payload, command != "read-file", |assets| {
            if command == "read-archive" {
                assets.binary_archive(payload["path"].as_str().unwrap())
            } else if command == "native-catalog-geometry" {
                assets.binary_catalog_geometry(payload)
            } else {
                assets.binary_file(payload["path"].as_str().unwrap())
            }
        })
    }

    fn invoke_inner(
        &self,
        command: &str,
        mut payload: Value,
        event: &mut dyn FnMut(Value),
    ) -> Result<Value> {
        // Registry-only cleanup must not contend with New Swap's player-file lock.
        if command == "blender-snapshot-release" {
            self.blender_snapshots.release(&payload)?;
            return Ok(json!(true));
        }
        if command == "release-preview-resources" {
            let lease = payload["lease"]
                .as_str()
                .context("Missing preview resource owner")?
                .parse::<u64>()?;
            self.assets
                .lock()
                .map_err(|_| anyhow::anyhow!("Native preview cache failed"))?
                .release_resources(lease);
            return Ok(json!(true));
        }
        if command == "cancel" {
            self.cancel();
            return Ok(json!(true));
        }
        if command == "cancel-command" {
            let name = payload["command"]
                .as_str()
                .context("Missing cancellation command")?;
            ensure!(allowed_command(name), "Unsupported cancellation command");
            let request_id = payload["jobId"].as_str();
            if request_id.is_none() {
                *self
                    .command_cancellations
                    .lock()
                    .unwrap()
                    .entry(name.to_owned())
                    .or_default() += 1;
            }
            let mut cancelled = false;
            for (active_command, job) in self.active.lock().unwrap().values() {
                if active_command == name
                    && request_id.is_none_or(|id| job.2.as_deref() == Some(id))
                {
                    job.cancel();
                    cancelled = true;
                }
            }
            return Ok(json!(cancelled));
        }
        if command == "cancel-preview" {
            let kind = match payload["kind"].as_str() {
                Some("staged") => PreviewKind::Staged,
                Some("catalog") => PreviewKind::Catalog,
                _ => bail!("Choose a preview category"),
            };
            self.preview_jobs.cancel(Some(kind));
            for (name, job) in self.active.lock().unwrap().values() {
                if matches!(name.as_str(), "preview-resources" | "native-hair-preview")
                    && job
                        .3
                        .unwrap_or_else(|| PreviewKind::request(name, &Value::Null))
                        == kind
                {
                    job.cancel();
                }
            }
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            loop {
                let active = self.active.lock().unwrap().values().any(|(name, job)| {
                    matches!(name.as_str(), "preview-resources" | "native-hair-preview")
                        && job
                            .3
                            .unwrap_or_else(|| PreviewKind::request(name, &Value::Null))
                            == kind
                });
                if !active && !self.preview_jobs.active(kind) {
                    break;
                }
                ensure!(
                    std::time::Instant::now() < deadline,
                    "Preview cancellation timed out"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            return Ok(json!(true));
        }
        ensure!(
            !self.closing.load(Ordering::SeqCst),
            "The prototype is closing"
        );
        if command == "blender-snapshot-fallback" {
            return self.blender_snapshots.fallback(&payload);
        }
        if command == "blender-snapshot-prepare" {
            return self.blender_snapshots.prepare(
                &self.session,
                &self.data,
                &payload,
                |command, payload| {
                    let _read = self.player_read(&payload, &mut *event)?;
                    self.python(command, payload, &mut *event)
                },
            );
        }
        if let Some(uri) = payload["path"].as_str().filter(|s| Snapshots::owns_uri(s)) {
            return match self.blender_snapshots.read(command, uri, false)? {
                ValueOrBytes::Json(value) => Ok(value),
                _ => bail!("Invalid Blender snapshot response"),
            };
        }
        // Claim and validate briefly, then release the gate before Python supervises Blender.
        let _blender_snapshot = if command == "blender-open" {
            let _read = self.player_read(&payload, &mut *event)?;
            let snapshot = self.blender_snapshots.claim(&payload)?;
            payload = snapshot.worker_payload(&payload);
            self.python("blender-snapshot-validate", payload.clone(), &mut *event)?;
            Some(snapshot)
        } else {
            None
        };
        let write = matches!(
            command,
            "run-swap"
                | "stage-player"
                | "reset"
                | "save-settings"
                | "body-fit-save"
                | "configs-change"
                | "headband-apply"
                | "native-hair-install"
                | "body-chest-fix"
                | "preview-chest-fix"
                | "preview-close-seams"
                | "templates-save"
                | "health-repair"
                | "cli-set"
                | "cli-language-set"
                | "blender-apply-return"
                | "accessory-recover-player"
        );
        let _read = if !write && command != "blender-open" {
            Some(self.player_read(&payload, &mut *event)?)
        } else {
            None
        };
        let _write = if write {
            Some(self.mutation.try_write().map_err(|_| {
                anyhow::anyhow!(
                    "Another edit or conversion is running. Finish it before applying changes."
                )
            })?)
        } else {
            None
        };
        match command {
            "native-preview-resources"
            | "native-preview-hair"
            | "native-catalog-geometry"
            | "read-archive" => {
                return self.with_preview_assets(command, &payload, true, |assets| {
                    if command == "read-archive" {
                        assets
                            .read_archive(payload["path"].as_str().context("Missing archive path")?)
                    } else if command == "native-preview-hair" {
                        assets.hair(&payload)
                    } else if command == "native-catalog-geometry" {
                        assets.catalog_geometry(&payload)
                    } else {
                        assets.prepare(&payload)
                    }
                });
            }
            "preview-cache-stats" => {
                return Ok(self
                    .assets
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Native preview cache failed"))?
                    .stats());
            }
            "read-file" | "file-stat" => {
                use base64::Engine;
                if let Some(uri) = payload["path"]
                    .as_str()
                    .filter(|s| s.starts_with("cmt-preview://"))
                {
                    return self.with_preview_assets(command, &payload, false, |assets| {
                        if command == "file-stat" {
                            assets.file_stat(uri)
                        } else {
                            assets.read_file(uri)
                        }
                    });
                }
                let path = PathBuf::from(payload["path"].as_str().context("Missing file path")?);
                let extension = path
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default()
                    .to_lowercase();
                ensure!(
                    matches!(extension.as_str(), "iff" | "dds"),
                    "Only preview IFF and DDS files may be read"
                );
                let metadata = fs::metadata(&path)?;
                ensure!(
                    metadata.is_file() && metadata.len() <= 512 * 1024 * 1024,
                    "Invalid or oversized preview file"
                );
                if command == "file-stat" {
                    return Ok(
                        json!({"size":metadata.len(),"mtimeMs":metadata.modified()?.duration_since(std::time::UNIX_EPOCH)?.as_millis(),"path":fs::canonicalize(path)?.to_string_lossy()}),
                    );
                }
                return Ok(
                    json!({"data":base64::engine::general_purpose::STANDARD.encode(fs::read(path)?)}),
                );
            }
            "reset" => {
                self.blender_snapshots.clear();
                ensure!(
                    self.active
                        .lock()
                        .unwrap()
                        .values()
                        .all(|(name, _)| name == "blender-open"),
                    "A worker is still running"
                );
                for entry in fs::read_dir(&self.session)? {
                    let entry = entry?;
                    let name = entry.file_name();
                    if name == "WebView2" || name.to_string_lossy().starts_with("blender-") {
                        continue;
                    }
                    if runtime::session_has_accessory_evidence(&entry.path()) {
                        continue;
                    }
                    if entry.file_type()?.is_dir() {
                        fs::remove_dir_all(entry.path())?;
                    } else {
                        fs::remove_file(entry.path())?;
                    }
                }
                self.assets.lock().unwrap().clear();
                return Ok(json!(true));
            }
            "catalog" | "native-hair-catalog" => {
                let accessory = command == "native-hair-catalog";
                let key = if accessory {
                    let kind = payload["assetType"]
                        .as_str()
                        .unwrap_or("hair")
                        .trim()
                        .to_lowercase();
                    ensure!(
                        matches!(kind.as_str(), "hair" | "facialhair"),
                        "Asset type must be hair or facialhair."
                    );
                    payload["assetType"] = json!(kind);
                    format!(
                        "accessory:{kind}{}",
                        if payload["nativeCatalog"] == false {
                            ":compatibility"
                        } else {
                            ""
                        }
                    )
                } else {
                    payload["gameLabel"]
                        .as_str()
                        .context("Choose a game")?
                        .to_owned()
                };
                if let Some(value) = self
                    .catalog
                    .lock()
                    .unwrap()
                    .get(&key)
                    .and_then(CachedCatalog::current)
                {
                    return Ok(value.clone());
                }
                let started = std::time::SystemTime::now();
                let mut value = if accessory && payload["nativeCatalog"] != false {
                    let cancellation = self.cancellation.load(Ordering::SeqCst);
                    let command_cancellation = self.command_cancellation(command);
                    match self
                        .native_accessory_catalog(payload["assetType"].as_str().unwrap(), event)
                    {
                        Ok(value) => value,
                        Err(error) => {
                            ensure!(
                                !self.closing.load(Ordering::SeqCst)
                                    && self.cancellation.load(Ordering::SeqCst) == cancellation
                                    && self.command_cancellation(command) == command_cancellation,
                                "Operation cancelled"
                            );
                            let mut value = self.python(command, payload, event)?;
                            value
                                .as_object_mut()
                                .context("Invalid accessory catalog")?
                                .remove("previewDescriptors");
                            value["native"] = json!(false);
                            value["nativeFallback"] = json!(error.to_string());
                            value
                        }
                    }
                } else {
                    self.python(command, payload, event)?
                };
                let settings = self.data.join("settings.json");
                let cached = if accessory {
                    CachedCatalog::accessories(value.clone(), settings, started)
                } else {
                    CachedCatalog::new(value.clone(), settings, started)
                };
                if let Some(cached) = cached {
                    value = cached.value().clone();
                    self.catalog.lock().unwrap().insert(key, cached);
                } else {
                    self.catalog.lock().unwrap().remove(&key);
                    if accessory {
                        value["sourceRevision"] = json!(format!(
                            "uncached:{}:{}",
                            std::process::id(),
                            self.next.fetch_add(1, Ordering::SeqCst)
                        ));
                    }
                }
                return Ok(value);
            }
            _ => {}
        }
        if command == "blender-open" {
            payload["managedReturn"] = json!(true);
        }
        let mut value = self.python(command, payload, |mut row| {
            if command == "blender-open" && row["state"] == "return-ready" {
                let result = self.invoke_inner("blender-apply-return", row.clone(), &mut |_| {});
                crate::blender_recovery::finish_return(&self.data, &self.session, &mut row, result);
            } else if command == "blender-open"
                && row["state"] == "error"
                && row["returned"]
                    .as_str()
                    .is_some_and(|path| std::path::Path::new(path).is_file())
            {
                crate::blender_recovery::preserve_failure(&self.data, &self.session, &mut row);
            }
            event(row);
        })?;
        if command == "health" {
            let root = self.engine(event)?;
            value["groups"]
                .as_array_mut()
                .context("Invalid health report")?
                .push(crate::prerequisites::health(&root));
            let maintenance = self
                .engine
                .lock()
                .unwrap()
                .as_ref()
                .map(|runtime| runtime.maintenance_report());
            if let Some(maintenance) = maintenance {
                let status = if maintenance["warnings"] == true {
                    "warning"
                } else {
                    "info"
                };
                value["groups"].as_array_mut().context("Invalid health report")?.push(json!({"id":"runtime-maintenance","label":"Runtime Maintenance","checks":[{"id":"runtime-maintenance-state","label":"Background cleanup","status":status,"statusLabel":if status == "info" {Some("Info")} else {None},"message":format!("{}; verified retention only, unknown contents preserved",maintenance["status"].as_str().unwrap_or("unknown")),"details":maintenance}]}));
            }
            let (status, message) = match runtime::verify(&root) {
                Ok(count) => (
                    "pass",
                    format!("All {count} bundled runtime files match the sharing EXE."),
                ),
                Err(error) => ("error", format!("{error:#}")),
            };
            value["groups"].as_array_mut().context("Invalid health report")?.push(json!({
                "id":"runtime", "label":"Runtime Integrity", "checks":[{
                    "id":"runtime-integrity", "label":"Packaged runtime", "status":status, "message":message, "path":root
                }]
            }));
            if let Some(recovery) = crate::blender_recovery::health(&self.data) {
                value["groups"].as_array_mut().unwrap().push(recovery);
            }
            let mut counts = json!({"pass":0,"warning":0,"error":0,"info":0});
            for group in value["groups"].as_array().unwrap() {
                for check in group["checks"]
                    .as_array()
                    .context("Invalid health checks")?
                {
                    let status = check["status"].as_str().unwrap_or("info");
                    counts[status] = json!(counts[status].as_u64().unwrap_or(0) + 1);
                }
            }
            value["overall"] = json!(if counts["error"].as_u64().unwrap_or(0) > 0 {
                "error"
            } else if counts["warning"].as_u64().unwrap_or(0) > 0 {
                "warning"
            } else {
                "pass"
            });
            value["counts"] = counts;
            value["nativePreviewCache"] = self.assets.lock().unwrap().stats();
        }
        if command == "save-settings" || command == "health-repair" {
            self.catalog.lock().unwrap().clear();
            self.assets.lock().unwrap().reset_game();
        } else if changes_player_files(command) {
            self.assets.lock().unwrap().player_files_changed();
        }
        Ok(value)
    }

    pub fn cancel(&self) {
        self.preview_jobs.cancel(None);
        self.cancellation.fetch_add(1, Ordering::SeqCst);
        for (_, job) in self.active.lock().unwrap().values() {
            job.cancel();
        }
    }
    fn command_cancellation(&self, command: &str) -> u64 {
        self.command_cancellations
            .lock()
            .unwrap()
            .get(command)
            .copied()
            .unwrap_or(0)
    }
    pub fn shutdown(&self) {
        self.closing.store(true, Ordering::SeqCst);
        self.cancel();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if let Ok(_guard) = self.mutation.try_write() {
                if self.active.lock().unwrap().is_empty() {
                    self.retire_session();
                }
                break;
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }
}
impl Backend {
    fn retire_session(&self) {
        if runtime::session_has_accessory_evidence(&self.session) {
            return;
        }
        if crate::blender_recovery::recover_session(&self.data, &self.session).is_ok() {
            let _ = fs::remove_dir_all(&self.session);
        }
    }
}
impl Drop for Backend {
    fn drop(&mut self) {
        self.shutdown();
        self.retire_session();
    }
}

fn changes_player_files(command: &str) -> bool {
    matches!(
        command,
        "run-swap"
            | "body-fit-save"
            | "configs-change"
            | "headband-apply"
            | "native-hair-install"
            | "body-chest-fix"
            | "preview-chest-fix"
            | "preview-close-seams"
            | "blender-apply-return"
            | "accessory-recover-player"
    )
}

fn allowed_command(command: &str) -> bool {
    matches!(
        command,
        "bootstrap"
            | "inspect"
            | "catalog"
            | "extract"
            | "stage-player"
            | "hair-options"
            | "facial-hair-options"
            | "run-swap"
            | "save-settings"
            | "body-fit"
            | "body-fit-save"
            | "configs-list"
            | "configs-change"
            | "headband-options"
            | "headband-apply"
            | "native-hair-catalog"
            | "native-hair-target"
            | "native-hair-install"
            | "native-hair-preview"
            | "preview-resources"
            | "preview-chest-fix"
            | "preview-close-seams"
            | "health"
            | "health-repair"
            | "body-chest-fix"
            | "templates-load"
            | "templates-save"
            | "cli-read"
            | "cli-set"
            | "cli-language-get"
            | "cli-language-set"
            | "blender-open"
            | "blender-snapshot-prepare"
            | "blender-snapshot-validate"
            | "blender-apply-return"
            | "accessory-recover-player"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_invalidation_follows_player_edits_not_unrelated_writes() {
        for command in [
            "run-swap",
            "body-fit-save",
            "configs-change",
            "headband-apply",
            "native-hair-install",
            "body-chest-fix",
            "preview-chest-fix",
            "preview-close-seams",
            "blender-apply-return",
        ] {
            assert!(changes_player_files(command), "{command}");
            assert!(allowed_command(command), "{command}");
        }
        for command in [
            "templates-save",
            "cli-set",
            "cli-language-set",
            "blender-open",
            "native-hair-catalog",
            "body-fit",
            "configs-list",
        ] {
            assert!(!changes_player_files(command), "{command}");
        }
    }
    #[test]
    fn stage_player_is_serialized_with_edits_and_reset() {
        assert!(allowed_command("stage-player"));
        assert!(!changes_player_files("stage-player"));
        let backend = Backend::new().unwrap();
        let _write = backend.mutation.write().unwrap();
        let error = backend
            .invoke("stage-player", json!({"filePath":"unused.iff"}), |_| {})
            .unwrap_err();
        assert!(error.to_string().contains("Another edit or conversion"));
    }

    #[test]
    fn resource_cleanup_does_not_contend_with_player_mutation_or_shutdown() {
        let backend = Arc::new(Backend::new().unwrap());
        let write = backend.mutation.write().unwrap();
        let copy = backend.clone();
        let (send, receive) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            let result = copy.invoke("release-preview-resources", json!({"lease":"1"}), |_| {});
            send.send(result).unwrap();
        });
        let result = receive.recv_timeout(std::time::Duration::from_secs(1));
        drop(write);
        thread.join().unwrap();
        assert!(
            result.is_ok(),
            "Resource cleanup waited for the player mutation lock"
        );
        assert_eq!(result.unwrap().unwrap(), json!(true));
        backend.closing.store(true, Ordering::SeqCst);
        assert_eq!(
            backend
                .invoke("release-preview-resources", json!({"lease":"1"}), |_| {})
                .unwrap(),
            json!(true)
        );
    }
    #[test]
    fn resource_release_is_idempotent_and_does_not_require_a_game_install() {
        let backend = Backend::new().unwrap();
        for _ in 0..2 {
            assert_eq!(
                backend
                    .invoke("release-preview-resources", json!({"lease":"1"}), |_| {})
                    .unwrap(),
                json!(true)
            );
        }
        for payload in [
            json!({}),
            json!({"lease":1}),
            json!({"lease":"invalid"}),
            json!({"lease":"-1"}),
        ] {
            assert!(
                backend
                    .invoke("release-preview-resources", payload, |_| {})
                    .is_err()
            );
        }
        assert!(
            backend
                .preview_binary("release-preview-resources", &json!({"lease":"1"}))
                .is_err()
        );
    }
    #[test]
    fn blender_snapshot_release_does_not_wait_for_player_mutation_or_shutdown() {
        let backend = Backend::new().unwrap();
        let write = backend.mutation.write().unwrap();
        let copy = backend.clone();
        let (send, receive) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            send.send(copy.invoke(
                "blender-snapshot-release",
                json!({"snapshotId":"already-released","handoffId":"owner"}),
                |_| {},
            ))
            .unwrap();
        });
        let result = receive.recv_timeout(std::time::Duration::from_secs(1));
        drop(write);
        thread.join().unwrap();
        assert_eq!(result.unwrap().unwrap(), json!(true));
        backend.closing.store(true, Ordering::SeqCst);
        assert_eq!(
            backend
                .invoke(
                    "blender-snapshot-release",
                    json!({"snapshotId":"already-released","handoffId":"owner"}),
                    |_| {}
                )
                .unwrap(),
            json!(true)
        );
    }
    #[test]
    fn engine_commands_are_allowlisted() {
        assert!(allowed_command("run-swap"));
        assert!(!allowed_command("eval"));
        assert!(!allowed_command("preview:load-staged"));
    }
    #[test]
    fn binary_preview_rejects_mutations_unregistered_local_paths_and_closing_state() {
        let backend = Backend::new().unwrap();
        assert!(
            backend
                .preview_binary("run-swap", &json!({"path":"cmt-preview://test"}))
                .is_err()
        );
        assert!(
            backend
                .preview_binary("read-archive", &json!({"path":"C:/test/player.iff"}))
                .is_err()
        );
        assert!(
            backend
                .preview_binary("read-file", &json!({"path":"../secret.dds"}))
                .is_err()
        );
        for payload in [
            json!({"geometryPath":"C:/private.iff","sourcePlayerPath":"cmt-preview://head"}),
            json!({"geometryPath":"cmt-preview://hair","sourcePlayerPath":"C:/private.iff"}),
        ] {
            assert!(
                backend
                    .preview_binary("native-catalog-geometry", &payload)
                    .is_err()
            );
        }
        backend.closing.store(true, Ordering::SeqCst);
        assert!(
            backend
                .preview_binary("read-archive", &json!({"path":"cmt-preview://missing"}))
                .unwrap_err()
                .to_string()
                .contains("closing")
        );
        assert!(backend.preview_binary("native-catalog-geometry",&json!({"geometryPath":"cmt-preview://hair","sourcePlayerPath":"cmt-preview://head"})).unwrap_err().to_string().contains("closing"));
    }
    #[test]
    fn targeted_cancellation_cannot_kill_a_replacement_job() {
        let backend = Backend::new().unwrap();
        let old = Arc::new(Job::for_request(Some("old".into())).unwrap());
        let new = Arc::new(Job::for_request(Some("new".into())).unwrap());
        backend
            .active
            .lock()
            .unwrap()
            .insert(1, ("run-swap".into(), old.clone()));
        backend
            .active
            .lock()
            .unwrap()
            .insert(2, ("run-swap".into(), new.clone()));
        assert_eq!(
            backend
                .invoke(
                    "cancel-command",
                    json!({"command":"run-swap","jobId":"old"}),
                    |_| {}
                )
                .unwrap(),
            json!(true)
        );
        assert!(old.1.load(Ordering::SeqCst));
        assert!(!new.1.load(Ordering::SeqCst));
        assert_eq!(backend.command_cancellation("run-swap"), 0);
        assert_eq!(
            backend
                .invoke(
                    "cancel-command",
                    json!({"command":"run-swap","jobId":"missing"}),
                    |_| {}
                )
                .unwrap(),
            json!(false)
        );
        assert!(!new.1.load(Ordering::SeqCst));
    }
    #[test]
    fn active_registration_is_removed_on_error() {
        let jobs = Mutex::new(HashMap::new());
        jobs.lock()
            .unwrap()
            .insert(1, ("run-swap".into(), Arc::new(Job::new().unwrap())));
        let operation = || -> Result<()> {
            let _registration = ActiveRegistration { jobs: &jobs, id: 1 };
            bail!("Failed preparation");
        };
        assert!(operation().is_err());
        assert!(jobs.lock().unwrap().is_empty());
    }
    #[test]
    fn reset_preserves_live_blender_handoff_and_browser_profile() {
        let backend = Backend::new().unwrap();
        let handoff = backend.session.join("blender-reset-test");
        let profile = backend.session.join("WebView2");
        let selection = backend.session.join("png0000-test");
        for path in [&handoff, &profile, &selection] {
            fs::create_dir_all(path).unwrap();
            fs::write(path.join("sentinel"), b"owned test data").unwrap();
        }
        backend
            .active
            .lock()
            .unwrap()
            .insert(0, ("blender-open".into(), Arc::new(Job::new().unwrap())));
        assert_eq!(
            backend.invoke("reset", json!({}), |_| {}).unwrap(),
            json!(true)
        );
        assert!(handoff.join("sentinel").is_file());
        assert!(profile.join("sentinel").is_file());
        assert!(!selection.exists());
    }
    #[test]
    fn windows_job_owns_and_terminates_child_processes() {
        let job = Job::new().unwrap();
        let mut child = Command::new("C:\\Windows\\System32\\ping.exe")
            .args(["-n", "30", "127.0.0.1"])
            .creation_flags(CREATE_NO_WINDOW.0)
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        unsafe {
            AssignProcessToJobObject(job.0, HANDLE(child.as_raw_handle())).unwrap();
        }
        job.cancel();
        assert!(!child.wait().unwrap().success());
    }
}
