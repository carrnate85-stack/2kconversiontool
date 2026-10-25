//! Launcher update / rollback, ported from WPF Backend\app-updates.cjs,
//! app-update-runner.cjs, app-update-transaction.cjs and app-package.cjs,
//! adapted to the Rust portable folder (`<root>\App` holds one native exe).
//!
//! Source: a downloaded, extracted release folder the user picks (no network,
//! no auto-check, as WPF). Verification is size + SHA-256 per file against the
//! package manifest. Layout (WPF `locations()`):
//!   <root>\App                         live launcher
//!   <root>\Recovery\Updates\           Incoming-<id>, Staged\App, Displaced\App,
//!                                      Runner\, transaction.json, request.json,
//!                                      last-result.json
//!   <root>\Recovery\PreviousApp\App    the single rollback copy
//! The swap runs from a copy of the current exe in Runner\ after the launcher
//! has exited, because the live App folder cannot be renamed while it runs.
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const PRODUCT: &str = "2k-launcher-rust";
const MANIFEST: &str = "release.json";
const RUNNER_EXE: &str = "launcher-update-runner.exe";
pub const RUNNER_FLAG: &str = "--launcher-update-runner";
const MAX_TOTAL: u64 = 4 * 1024 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Entry {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Manifest {
    pub schema: u32,
    pub product: String,
    pub version: String,
    pub channel: String,
    pub notes: Vec<String>,
    pub built_at: String,
    pub files: Vec<Entry>,
}
impl Manifest {
    fn exe(&self) -> Option<&str> {
        let mut exes = self
            .files
            .iter()
            .filter(|f| !f.path.contains('/') && f.path.to_ascii_lowercase().ends_with(".exe"));
        let first = exes.next()?;
        exes.next().is_none().then_some(first.path.as_str())
    }
}

pub(crate) struct Locations {
    pub root: PathBuf,
    pub app: PathBuf,
    pub updates: PathBuf,
    pub staged: PathBuf,
    pub displaced: PathBuf,
    pub runner: PathBuf,
    pub journal: PathBuf,
    pub request: PathBuf,
    pub result: PathBuf,
    pub previous: PathBuf,
}
pub(crate) fn locations(root: &Path) -> Locations {
    let updates = root.join("Recovery/Updates");
    Locations {
        root: root.into(),
        app: root.join("App"),
        staged: updates.join("Staged/App"),
        displaced: updates.join("Displaced/App"),
        runner: updates.join("Runner"),
        journal: updates.join("transaction.json"),
        request: updates.join("request.json"),
        result: updates.join("last-result.json"),
        previous: root.join("Recovery/PreviousApp/App"),
        updates,
    }
}

// ---------- Package verification (app-package.cjs) ----------

fn safe_name(p: &str) -> bool {
    const RESERVED: [&str; 22] = [
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
        "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    ];
    !p.is_empty()
        && p.len() <= 400
        && !p.contains('\\')
        && !p.contains(':')
        && p.split('/').all(|s| {
            !s.is_empty()
                && s != "."
                && s != ".."
                && !s.ends_with('.')
                && !s.ends_with(' ')
                && !s.chars().any(|c| c < ' ' || "<>\"|?*".contains(c))
                && !RESERVED.contains(
                    &s.split('.')
                        .next()
                        .unwrap_or("")
                        .to_ascii_lowercase()
                        .as_str(),
                )
        })
}
fn version_ok(v: &str) -> bool {
    let (core, pre) = v.split_once('-').unwrap_or((v, ""));
    let parts: Vec<&str> = core.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.len() <= 6 && p.bytes().all(|b| b.is_ascii_digit()))
        && pre.len() <= 32
        && pre.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.')
        && v.len() <= 48
}
fn version_policy(current: &Manifest, incoming: &Manifest) -> Result<()> {
    validate_manifest(current)?;
    version_policy_values(&current.version, &current.channel, incoming)
}
fn version_policy_values(
    current_version: &str,
    current_channel: &str,
    incoming: &Manifest,
) -> Result<()> {
    ensure!(
        version_ok(current_version),
        "Current launcher version is invalid"
    );
    validate_manifest(incoming)?;
    let key = |s: &str| -> Vec<u32> {
        s.split('-')
            .next()
            .unwrap()
            .split('.')
            .map(|p| p.parse().unwrap())
            .collect()
    };
    let old = key(current_version);
    let new = key(&incoming.version);
    let prerelease = |version: &str| version.split_once('-').map(|(_, p)| p.to_owned());
    let mut order = new.cmp(&old);
    if order.is_eq() {
        order = match (prerelease(&incoming.version), prerelease(current_version)) {
            (None, None) => std::cmp::Ordering::Equal,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (Some(_), None) => std::cmp::Ordering::Less,
            (Some(a), Some(b)) => {
                let a = a.split('.').collect::<Vec<_>>();
                let b = b.split('.').collect::<Vec<_>>();
                let mut result = std::cmp::Ordering::Equal;
                for (x, y) in a.iter().zip(&b) {
                    result = match (x.parse::<u128>(), y.parse::<u128>()) {
                        (Ok(x), Ok(y)) => x.cmp(&y),
                        (Ok(_), Err(_)) => std::cmp::Ordering::Less,
                        (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
                        _ => x.cmp(y),
                    };
                    if !result.is_eq() {
                        break;
                    }
                }
                if result.is_eq() {
                    a.len().cmp(&b.len())
                } else {
                    result
                }
            }
        };
    }
    ensure!(
        !order.is_lt(),
        "Downgrade updates are refused. Use explicit PreviousApp rollback instead."
    );
    ensure!(
        !order.is_eq() || (current_channel == "QA" && incoming.channel == "QA"),
        "Same-version replacement is available only for QA builds."
    );
    Ok(())
}
fn publisher_authentication(_incoming: &Manifest) -> Result<()> {
    // There is deliberately no trust anchor read from a chosen package, user
    // settings, or adjacent signature file. Provisioning must be reviewed first.
    bail!(
        "External launcher updates are disabled: this build has no provisioned publisher verification key. Package SHA256 proves integrity, not publisher authenticity. Explicit PreviousApp rollback remains available."
    )
}
fn validate_manifest(m: &Manifest) -> Result<()> {
    let unsupported = "This folder is not a supported launcher update.";
    ensure!(
        m.schema == 1
            && m.product == PRODUCT
            && version_ok(&m.version)
            && matches!(m.channel.as_str(), "Main" | "QA")
            && m.notes.len() <= 100
            && m.notes.iter().all(|n| n.chars().count() <= 1000)
            && (1..=10000).contains(&m.files.len()),
        unsupported
    );
    let mut seen = std::collections::BTreeSet::new();
    let mut total = 0u64;
    for f in &m.files {
        ensure!(
            safe_name(&f.path)
                && seen.insert(f.path.to_ascii_lowercase())
                && f.sha256.len() == 64
                && f.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid update file list."
        );
        total = total.saturating_add(f.size);
    }
    ensure!(total <= MAX_TOTAL, "Update package exceeds its size limit.");
    ensure!(m.exe().is_some(), "Update is missing a launcher component.");
    ensure!(
        !m.files.iter().any(|f| {
            let top = f.path.split('/').next().unwrap_or("").to_ascii_lowercase();
            matches!(
                top.as_str(),
                "data" | "library" | "cache" | "logs" | "recovery" | "runtime" | "smoke"
            )
        }),
        "An App update cannot contain user data."
    );
    Ok(())
}
fn linked(path: &Path) -> Result<bool> {
    use std::os::windows::fs::MetadataExt;
    let m = fs::symlink_metadata(path)?;
    Ok(m.file_type().is_symlink() || m.file_attributes() & 0x400 != 0)
}
fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, u64)>) -> Result<()> {
    for e in fs::read_dir(dir)? {
        let e = e?;
        let p = e.path();
        ensure!(!linked(&p)?, "Linked package files are not supported.");
        let rel = p.strip_prefix(base)?.to_string_lossy().replace('\\', "/");
        if e.file_type()?.is_dir() {
            walk(&p, base, out)?;
        } else {
            ensure!(
                out.len() < 10000,
                "Update contains missing or unexpected files."
            );
            out.push((rel, e.metadata()?.len()));
        }
    }
    Ok(())
}
fn sha256(path: &Path) -> Result<String> {
    let mut f = fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 4 * 1024 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:X}", h.finalize()))
}
fn inventory(app: &Path) -> Result<Vec<(String, u64)>> {
    ensure!(!linked(app)?, "Linked update paths are not supported.");
    let mut files = vec![];
    walk(app, app, &mut files)?;
    files.retain(|(p, _)| p != MANIFEST);
    files.sort();
    Ok(files)
}
/// `verifyApp`: the files on disk equal the manifest exactly, by size and SHA-256.
pub(crate) fn verify(app: &Path, m: &Manifest, progress: &dyn Fn(&str)) -> Result<()> {
    validate_manifest(m)?;
    let disk = inventory(app)?;
    let mut want: Vec<(String, u64)> = m.files.iter().map(|f| (f.path.clone(), f.size)).collect();
    want.sort();
    let lower = |v: &[(String, u64)]| {
        v.iter()
            .map(|(p, s)| (p.to_ascii_lowercase(), *s))
            .collect::<Vec<_>>()
    };
    ensure!(
        lower(&disk) == lower(&want),
        "Update contains missing or unexpected files."
    );
    for (i, f) in m.files.iter().enumerate() {
        if i % 20 == 0 {
            progress(&format!("Checking update filesâ€¦ {}/{}", i, m.files.len()));
        }
        let p = app.join(&f.path);
        ensure!(
            fs::metadata(&p)?.len() == f.size && sha256(&p)?.eq_ignore_ascii_case(&f.sha256),
            "Update verification failed: {}",
            f.path
        );
    }
    Ok(())
}
fn read_manifest(app: &Path) -> Result<Option<Manifest>> {
    let p = app.join(MANIFEST);
    if !p.is_file() {
        return Ok(None);
    }
    ensure!(
        fs::metadata(&p)?.len() <= 8 * 1024 * 1024,
        "This folder is not a supported launcher update."
    );
    let m: Manifest = serde_json::from_slice(&fs::read(&p)?)
        .context("This folder is not a supported launcher update.")?;
    validate_manifest(&m)?;
    Ok(Some(m))
}
fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
/// Rust adaptation: Build.ps1 publishes `BUILD-INFO.json` (version, sha256,
/// bytes) next to or inside App instead of WPF's release.json. The exe must
/// match it exactly; the only other files allowed are small text documents.
fn manifest_from_build_info(app: &Path, info: &Path) -> Result<Manifest> {
    let unsupported = "This folder is not a supported launcher update.";
    ensure!(fs::metadata(info)?.len() <= 64 * 1024, unsupported);
    let text = fs::read_to_string(info)?;
    let v: Value =
        serde_json::from_str(text.trim_start_matches('\u{feff}')).context(unsupported)?;
    let version = v["version"].as_str().context(unsupported)?.to_string();
    let exe_sha = v["sha256"].as_str().context(unsupported)?.to_string();
    let exe_bytes = v["bytes"].as_u64().context(unsupported)?;
    let disk = inventory(app)?;
    let mut files = vec![];
    for (path, size) in disk {
        let lower = path.to_ascii_lowercase();
        let doc = !path.contains('/')
            && size <= 4 * 1024 * 1024
            && [".json", ".md", ".txt"].iter().any(|e| lower.ends_with(e));
        let exe = !path.contains('/') && lower.ends_with(".exe");
        ensure!(doc || exe, "Update contains missing or unexpected files.");
        let sha = sha256(&app.join(&path))?;
        if exe {
            ensure!(
                size == exe_bytes && sha.eq_ignore_ascii_case(&exe_sha),
                "Update verification failed: {path}"
            );
        }
        files.push(Entry {
            path,
            size,
            sha256: sha,
        });
    }
    let m = Manifest {
        schema: 1,
        product: PRODUCT.into(),
        version,
        channel: "QA".into(),
        notes: v["notes"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|n| n.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        built_at: v["builtUtc"].as_str().unwrap_or("").into(),
        files,
    };
    validate_manifest(&m)?;
    Ok(m)
}
/// Resolve and verify a chosen package folder (UPD:19-26).
fn package(choice: &Path, l: &Locations) -> Result<(PathBuf, Manifest)> {
    ensure!(
        choice.is_absolute() && choice.is_dir(),
        "Choose the extracted update folder."
    );
    ensure!(!linked(choice)?, "Linked update paths are not supported.");
    let app = if choice.join("App").is_dir() {
        choice.join("App")
    } else {
        choice.to_path_buf()
    };
    let canon = app.canonicalize()?;
    if let Ok(live) = l.app.canonicalize() {
        ensure!(
            canon != live,
            "Choose a downloaded update, not the running App."
        );
    }
    if let Ok(updates) = l.updates.canonicalize() {
        ensure!(
            !canon.starts_with(&updates),
            "Choose the original download, not update working files."
        );
    }
    let manifest = match read_manifest(&app)? {
        Some(m) => m,
        None => {
            let info = [
                app.join("BUILD-INFO.json"),
                app.parent()
                    .map(|p| p.join("BUILD-INFO.json"))
                    .unwrap_or_default(),
            ]
            .into_iter()
            .find(|p| p.is_file())
            .context("This folder is not a supported launcher update.")?;
            manifest_from_build_info(&app, &info)?
        }
    };
    Ok((app, manifest))
}

// ---------- Copy / rename helpers ----------

fn copy_tree(from: &Path, to: &Path, files: &[Entry]) -> Result<()> {
    for f in files {
        let dst = to.join(&f.path);
        fs::create_dir_all(dst.parent().unwrap())?;
        fs::copy(from.join(&f.path), &dst)?;
    }
    Ok(())
}
/// TX:5-10: rename with 6 retries on sharing errors and a growing backoff.
fn rename(from: &Path, to: &Path) -> Result<()> {
    if let Some(p) = to.parent() {
        fs::create_dir_all(p)?;
    }
    let mut last = None;
    for attempt in 0..7u64 {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) if matches!(e.raw_os_error(), Some(5 | 32 | 33)) && attempt < 6 => {
                last = Some(e);
                std::thread::sleep(Duration::from_millis(100 * (attempt + 1)));
            }
            Err(e) => return Err(e).with_context(|| format!("Could not move {}", from.display())),
        }
    }
    Err(last.unwrap().into())
}
fn write_json(path: &Path, v: &impl Serialize) -> Result<()> {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p)?;
    }
    crate::backend::atomic_json(path, v)
}
fn remove_dir(path: &Path, l: &Locations) -> Result<()> {
    // Only folders inside Recovery are ever deleted.
    ensure!(
        path.starts_with(l.root.join("Recovery")),
        "Unsafe update cleanup path."
    );
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

// ---------- Launcher-side state (app-updates.cjs) ----------

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Pending {
    version: String,
    channel: String,
    notes: Vec<String>,
    rollback: bool,
}
#[derive(Default)]
struct UpdateState {
    busy: bool,
    pending: Option<Pending>,
}
static STATE: Mutex<UpdateState> = Mutex::new(UpdateState {
    busy: false,
    pending: None,
});

/// HOST:104: no game launch while an update is being prepared or pending.
pub(crate) fn assert_idle_for_launch() -> Result<()> {
    let s = STATE
        .lock()
        .map_err(|_| anyhow::anyhow!("Update state unavailable"))?;
    ensure!(
        !s.busy && s.pending.is_none(),
        "Finish or cancel the pending launcher update before starting the game."
    );
    Ok(())
}
fn game_running() -> Result<bool> {
    Ok(crate::session_safety::processes()?
        .iter()
        .any(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe")))
}
fn current_manifest(l: &Locations) -> Result<Manifest> {
    if let Some(m) = read_manifest(&l.app)? {
        return Ok(m);
    }
    // Builds before this updater have no release.json: record the live App
    // exactly as it is now, so the swap and rollback can verify it.
    let files = inventory(&l.app)?
        .into_iter()
        .map(|(path, size)| {
            Ok(Entry {
                sha256: sha256(&l.app.join(&path))?,
                path,
                size,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let m = Manifest {
        schema: 1,
        product: PRODUCT.into(),
        version: crate::backend::VERSION.into(),
        channel: "QA".into(),
        notes: vec!["This build predates the update catalog.".into()],
        built_at: String::new(),
        files,
    };
    validate_manifest(&m)?;
    write_json(&l.app.join(MANIFEST), &m)?;
    Ok(m)
}
/// `launcher:build-info` (UPD:9 / UI:9-13).
pub(crate) fn build_info(root: &Path) -> Value {
    let l = locations(root);
    let current = read_manifest(&l.app).ok().flatten();
    let previous = read_manifest(&l.previous).ok().flatten();
    let last: Option<Value> = fs::read(&l.result)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    let s = STATE.lock().ok();
    json!({
        "version": current.as_ref().map(|m| m.version.clone()).unwrap_or_else(|| crate::backend::VERSION.into()),
        "channel": current.as_ref().map(|m| m.channel.clone()).unwrap_or_else(|| "QA".into()),
        "builtAt": current.as_ref().map(|m| m.built_at.clone()).unwrap_or_default(),
        "notes": current.as_ref().map(|m| m.notes.clone()).unwrap_or_else(|| vec!["This build predates the update catalog.".into()]),
        "previous": previous.map(|m| json!({"version":m.version,"channel":m.channel})),
        "portable": l.app.join(std::env::current_exe().ok().and_then(|e| e.file_name().map(|n| n.to_owned())).unwrap_or_default()).is_file(),
        "pending": s.as_ref().and_then(|s| s.pending.clone()),
        "busy": s.as_ref().is_some_and(|s| s.busy),
        "recoveryNeeded": l.journal.exists(),
        "lastResult": last,
        "externalUpdatesEnabled":false,
        "updateTrustMessage":"External updates are disabled until a publisher verification key is provisioned. PreviousApp rollback remains available.",
    })
}
/// `prepare` (UPD:15-35). `rollback` stages PreviousApp through the same path.
pub(crate) fn prepare(
    root: &Path,
    choice: Option<&Path>,
    progress: &dyn Fn(&str),
) -> Result<Value> {
    let l = locations(root);
    {
        let mut s = STATE
            .lock()
            .map_err(|_| anyhow::anyhow!("Update state unavailable"))?;
        ensure!(!s.busy, "An update is already being prepared.");
        ensure!(s.pending.is_none(), "Cancel the pending update first.");
        s.busy = true;
    }
    let result = (|| -> Result<Value> {
        ensure!(
            !l.journal.exists(),
            "An interrupted update needs recovery. Use Recovery/Updates/Runner/Recover launcher.cmd."
        );
        ensure!(
            !game_running()?,
            "Close NBA 2K27 before updating the launcher."
        );
        let exe = std::env::current_exe()?;
        ensure!(
            exe.parent().is_some_and(|p| p == l.app),
            "Launcher updates need the portable layout: the launcher must run from its App folder."
        );
        let rollback = choice.is_none();
        let (source, manifest) = match choice {
            Some(c) => package(c, &l)?,
            None => {
                let m = read_manifest(&l.previous)?
                    .context("There is no previous version to restore.")?;
                (l.previous.clone(), m)
            }
        };
        if !rollback {
            match read_manifest(&l.app)? {
                Some(current) => version_policy(&current, &manifest)?,
                None => version_policy_values(crate::backend::VERSION, "QA", &manifest)?,
            }
            publisher_authentication(&manifest)?;
        }
        progress("Checking update filesâ€¦");
        verify(&source, &manifest, progress)?;
        current_manifest(&l)?;
        progress("Preparing updateâ€¦");
        let incoming = l
            .updates
            .join(format!("Incoming-{}", crate::backend::unique_id()));
        let staged = (|| -> Result<()> {
            copy_tree(&source, &incoming, &manifest.files)?;
            write_json(&incoming.join(MANIFEST), &manifest)?;
            verify(&incoming, &manifest, progress)?;
            // Runner = a copy of the CURRENT launcher exe (UPD:28-29).
            fs::create_dir_all(&l.runner)?;
            fs::copy(&exe, l.runner.join(RUNNER_EXE))?;
            fs::write(
                l.runner.join("Recover launcher.cmd"),
                format!("@echo off\r\n\"%~dp0{RUNNER_EXE}\" {RUNNER_FLAG} --recover\r\n"),
            )?;
            remove_dir(l.staged.parent().unwrap(), &l)?;
            rename(&incoming, &l.staged)?;
            Ok(())
        })();
        if staged.is_err() {
            let _ = remove_dir(&incoming, &l);
        }
        staged?;
        let pending = Pending {
            version: manifest.version.clone(),
            channel: manifest.channel.clone(),
            notes: manifest.notes.clone(),
            rollback,
        };
        STATE.lock().unwrap().pending = Some(pending.clone());
        Ok(
            json!({"pending": pending, "message": "Verified and ready. Install and restart when ready."}),
        )
    })();
    STATE.lock().unwrap().busy = false;
    result
}
/// `cancel` (UPD:36): clears pending; the staged copy stays until replaced.
pub(crate) fn cancel() -> Result<Value> {
    let mut s = STATE
        .lock()
        .map_err(|_| anyhow::anyhow!("Update state unavailable"))?;
    ensure!(!s.busy, "Wait for update preparation to finish.");
    s.pending = None;
    Ok(json!({"message":"Update canceled."}))
}
/// `launcher:update-restart` + `startOnExit` (MAIN:188, UPD:37-43). Writes the
/// request and starts the detached runner; the caller then exits the app.
pub(crate) fn start_runner(root: &Path) -> Result<()> {
    let l = locations(root);
    {
        let s = STATE
            .lock()
            .map_err(|_| anyhow::anyhow!("Update state unavailable"))?;
        ensure!(s.pending.is_some() && !s.busy, "Choose an update first.");
    }
    ensure!(
        !game_running()?,
        "Close NBA 2K27 before updating the launcher."
    );
    let me = std::process::id();
    let started = crate::session_safety::processes()?
        .into_iter()
        .find(|p| p.pid == me)
        .and_then(|p| p.identity.map(|i| i.started))
        .context("Launcher identity unavailable")?;
    write_json(
        &l.request,
        &json!({"schema":1,"parents":[{"pid":me,"started":started}],"created":now_ms() as u64}),
    )?;
    use std::os::windows::process::CommandExt;
    std::process::Command::new(l.runner.join(RUNNER_EXE))
        .arg(RUNNER_FLAG)
        .current_dir(&l.runner)
        .creation_flags(0x00000008 | 0x00000200 | 0x08000000) // detached, new group, no window
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context(
            "Update could not start. The current launcher is still installed. Reopen it to retry.",
        )?;
    Ok(())
}

// ---------- Runner + transaction (app-update-runner.cjs / app-update-transaction.cjs) ----------

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DirectoryId {
    volume: u64,
    index: [u8; 16],
}
fn directory_id(path: &Path) -> Result<DirectoryId> {
    use std::os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    };
    use windows::Win32::{Foundation::HANDLE, Storage::FileSystem::*};
    let f = fs::OpenOptions::new()
        .read(true)
        .share_mode(7)
        .custom_flags(0x02000000 | 0x00200000)
        .open(path)?;
    let m = f.metadata()?;
    ensure!(
        m.is_dir() && m.file_attributes() & 0x400 == 0,
        "Update directory identity is nonordinary"
    );
    let mut id = FILE_ID_INFO::default();
    unsafe {
        GetFileInformationByHandleEx(
            HANDLE(f.as_raw_handle()),
            FileIdInfo,
            &mut id as *mut _ as _,
            std::mem::size_of::<FILE_ID_INFO>() as u32,
        )?;
    }
    Ok(DirectoryId {
        volume: id.VolumeSerialNumber,
        index: id.FileId.Identifier,
    })
}
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Snapshot {
    manifest: Manifest,
    directory: DirectoryId,
}
fn snapshot(dir: &Path) -> Result<Snapshot> {
    let directory = directory_id(dir)?;
    let manifest = verify_dir(dir)?;
    ensure!(
        directory_id(dir)? == directory,
        "Update directory changed during verification"
    );
    Ok(Snapshot {
        manifest,
        directory,
    })
}
fn verify_snapshot(dir: &Path, expected: &Snapshot) -> Result<()> {
    ensure!(
        directory_id(dir)? == expected.directory,
        "Update original directory identity differs"
    );
    ensure!(
        verify_dir(dir)? == expected.manifest && directory_id(dir)? == expected.directory,
        "Update original content or identity differs"
    );
    Ok(())
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Journal {
    schema: u32,
    phase: String,
    version: String,
    had_previous: bool,
    #[serde(default)]
    original_app: Option<Snapshot>,
    #[serde(default)]
    original_previous: Option<Snapshot>,
    #[serde(default)]
    incoming: Option<Snapshot>,
}
fn phase(l: &Locations, j: &mut Journal, value: &str) -> Result<()> {
    j.phase = value.into();
    write_json(&l.journal, j)
}
fn begin_journal(l: &Locations) -> Result<Journal> {
    let incoming = snapshot(&l.staged)?;
    let original_app = snapshot(&l.app)?;
    let original_previous = if l.previous.exists() {
        Some(snapshot(&l.previous)?)
    } else {
        None
    };
    Ok(Journal {
        schema: 2,
        phase: "prepared".into(),
        version: incoming.manifest.version.clone(),
        had_previous: original_previous.is_some(),
        original_app: Some(original_app),
        original_previous,
        incoming: Some(incoming),
    })
}
fn verify_dir(dir: &Path) -> Result<Manifest> {
    let m = read_manifest(dir)?.context("Launcher files have no release manifest.")?;
    verify(dir, &m, &|_| {})?;
    Ok(m)
}
fn rollback_originals(l: &Locations, j: &Journal) -> Result<()> {
    let original = j
        .original_app
        .as_ref()
        .context("Original App witness missing; recovery retained")?;
    let incoming = j
        .incoming
        .as_ref()
        .context("Incoming witness missing; recovery retained")?;
    if l.app.exists() && directory_id(&l.app)? != original.directory {
        ensure!(
            !l.staged.exists(),
            "Conflicting staged App; recovery retained"
        );
        verify_snapshot(&l.app, incoming)?;
        rename(&l.app, &l.staged)?;
    }
    if !l.app.exists() {
        verify_snapshot(&l.previous, original)?;
        rename(&l.previous, &l.app)?;
    }
    if let Some(previous) = &j.original_previous {
        if !l.previous.exists() {
            verify_snapshot(&l.displaced, previous)?;
            rename(&l.displaced, &l.previous)?;
        }
        verify_snapshot(&l.previous, previous)?;
    } else {
        ensure!(
            !l.previous.exists(),
            "Unexpected PreviousApp; recovery retained"
        );
    }
    verify_snapshot(&l.app, original)?;
    ensure!(
        !l.displaced.exists(),
        "Displaced App remains; recovery retained"
    );
    Ok(())
}
/// Only an explicit, independently revalidated PreviousApp can be applied while
/// this build has no publisher trust anchor. Incoming self-manifests are not keys.
pub(crate) fn apply(l: &Locations) -> Result<()> {
    let incoming = verify_dir(&l.staged)?;
    let previous=snapshot(&l.previous).context("External updates are disabled without a provisioned publisher verification key; only PreviousApp rollback is available")?;
    if incoming != previous.manifest {
        publisher_authentication(&incoming)?;
    }
    apply_transaction_checked(l, Some(&previous))
}
#[cfg(test)]
fn apply_transaction(l: &Locations) -> Result<()> {
    apply_transaction_checked(l, None)
}
fn apply_transaction_checked(l: &Locations, rollback: Option<&Snapshot>) -> Result<()> {
    ensure!(
        l.root
            .file_name()
            .is_none_or(|n| !n.eq_ignore_ascii_case("App")),
        "Choose the portable launcher root."
    );
    ensure!(
        !l.journal.exists() && !l.displaced.exists(),
        "An earlier update needs recovery before another update."
    );
    let mut j = begin_journal(l)?;
    if let Some(expected) = rollback {
        let previous = j
            .original_previous
            .as_ref()
            .context("Rollback source disappeared")?;
        ensure!(
            previous.directory == expected.directory
                && previous.manifest == expected.manifest
                && j.incoming.as_ref().unwrap().manifest == expected.manifest,
            "Rollback source changed after authorization"
        );
    }
    phase(l, &mut j, "prepared")?;
    #[cfg(feature = "dev-hub-update-fixtures")]
    dev_hub::interrupt("prepared");
    let swap = (|| -> Result<()> {
        if j.had_previous {
            verify_snapshot(&l.previous, j.original_previous.as_ref().unwrap())?;
            rename(&l.previous, &l.displaced)?;
        }
        verify_snapshot(&l.app, j.original_app.as_ref().unwrap())?;
        rename(&l.app, &l.previous)?;
    #[cfg(feature = "dev-hub-update-fixtures")]
    dev_hub::interrupt("old-renamed");
        phase(l, &mut j, "old-moved")?;
        verify_snapshot(&l.staged, j.incoming.as_ref().unwrap())?;
        rename(&l.staged, &l.app)?;
        verify_snapshot(&l.app, j.incoming.as_ref().unwrap())?;
        verify_snapshot(&l.previous, j.original_app.as_ref().unwrap())?;
        phase(l, &mut j, "committed")?;
    #[cfg(feature = "dev-hub-update-fixtures")]
    dev_hub::interrupt("committed");
        Ok(())
    })();
    if let Err(error) = swap {
        rollback_originals(l,&j).context(format!("Update failed ({error:#}); exact original restoration incomplete, transaction journal retained"))?;
        fs::remove_file(&l.journal)?;
        return Err(error);
    }
    if l.displaced.exists() {
        verify_snapshot(
            &l.displaced,
            j.original_previous
                .as_ref()
                .context("Unexpected displaced App")?,
        )?;
    }
    remove_dir(l.displaced.parent().unwrap(), l)?;
    fs::remove_file(&l.journal)?;
    Ok(())
}
/// Legacy journals lack original identity/content witnesses and must remain held.
pub(crate) fn recover(l: &Locations) -> Result<bool> {
    if !l.journal.exists() {
        return Ok(false);
    }
    let file = fs::File::open(&l.journal)?;
    ensure!(
        file.metadata()?.len() <= 64 * 1024 * 1024,
        "Update recovery journal exceeds bounds"
    );
    let mut bytes = Vec::new();
    file.take(64 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 64 * 1024 * 1024,
        "Update recovery journal exceeds bounds"
    );
    let j: Journal = serde_json::from_slice(&bytes).context("Unknown update recovery state")?;
    ensure!(
        j.schema == 2
            && j.original_app.is_some()
            && j.incoming.is_some()
            && j.had_previous == j.original_previous.is_some(),
        "Legacy or incomplete update journal has no exact original identity witnesses; recovery retained for manual review"
    );
    match j.phase.as_str() {
        "committed" => {
            verify_snapshot(&l.app, j.incoming.as_ref().unwrap())?;
            verify_snapshot(&l.previous, j.original_app.as_ref().unwrap())?;
            if l.displaced.exists() {
                verify_snapshot(
                    &l.displaced,
                    j.original_previous
                        .as_ref()
                        .context("Unexpected displaced App")?,
                )?;
            }
            remove_dir(l.displaced.parent().unwrap(), l)?;
        }
        "prepared" | "old-moved" => rollback_originals(l, &j)?,
        _ => bail!("Unknown update recovery state; journal retained"),
    }
    fs::remove_file(&l.journal)?;
    Ok(true)
}
fn launcher_processes(l: &Locations) -> Result<Vec<String>> {
    let app = l.app.to_string_lossy().to_ascii_lowercase();
    let me = std::process::id();
    Ok(crate::session_safety::processes()?
        .into_iter()
        .filter(|p| p.pid != me)
        .filter(|p| {
            p.name.eq_ignore_ascii_case("NBA2K27.exe")
                || p.identity
                    .as_ref()
                    .is_some_and(|i| i.path.to_ascii_lowercase().starts_with(&app))
        })
        .map(|p| p.name)
        .collect())
}
fn relaunch(l: &Locations) {
    let exe = read_manifest(&l.app)
        .ok()
        .flatten()
        .and_then(|m| m.exe().map(|e| l.app.join(e)));
    if let Some(exe) = exe.filter(|e| e.is_file()) {
        let _ = std::process::Command::new(&exe).current_dir(&l.app).spawn();
    }
}
/// Entry point for `<exe> --launcher-update-runner [--recover]` (RUN:6-35).
pub(crate) fn runner_main(args: &[String]) -> i32 {
    let recover_only = args.iter().any(|a| a == "--recover");
    let exe = std::env::current_exe().ok();
    let root = exe.as_ref().and_then(|e| {
        e.parent()?
            .parent()?
            .parent()?
            .parent()
            .map(Path::to_path_buf)
    });
    let Some(root) = root.filter(|r| {
        exe.as_ref()
            .is_some_and(|e| e.parent() == Some(&locations(r).runner))
    }) else {
        return 2; // 'Updater is outside its portable launcher folder.'
    };
    let l = locations(&root);
    let outcome = (|| -> Result<&'static str> {
        if !recover_only {
            let req: Value = serde_json::from_slice(&fs::read(&l.request)?)
                .context("Update request expired or is invalid.")?;
            let age = now_ms().saturating_sub(req["created"].as_u64().unwrap_or(0) as u128);
            ensure!(
                req["schema"] == 1 && age < 300_000,
                "Update request expired or is invalid."
            );
            let _ = fs::remove_file(&l.request);
            let parents: Vec<(u32, String)> = req["parents"]
                .as_array()
                .context("Update request expired or is invalid.")?
                .iter()
                .filter_map(|p| {
                    Some((
                        p["pid"].as_u64()? as u32,
                        p["started"].as_str()?.to_string(),
                    ))
                })
                .collect();
            let deadline = Instant::now() + Duration::from_secs(120);
            loop {
                let rows = crate::session_safety::processes()?;
                let alive = parents.iter().any(|(pid, started)| {
                    rows.iter().any(|r| {
                        r.pid == *pid && r.identity.as_ref().is_none_or(|i| &i.started == started)
                    })
                });
                if !alive {
                    break;
                }
                ensure!(
                    Instant::now() < deadline,
                    "Launcher cleanup has not finished. Open the launcher and try again."
                );
                std::thread::sleep(Duration::from_millis(250));
            }
        }
        ensure!(
            launcher_processes(&l)?.is_empty(),
            "Close NBA 2K27 and every launcher window before updating."
        );
        if recover(&l)? {
            return Ok("An interrupted update was recovered.");
        }
        ensure!(!recover_only, "No interrupted update needs recovery.");
        apply(&l)?;
        Ok("Launcher update completed.")
    })();
    let (ok, message) = match &outcome {
        Ok(m) => (true, m.to_string()),
        Err(e) => (false, format!("{e:#}")),
    };
    let _ = write_json(
        &l.result,
        &json!({"ok":ok,"message":message,"at":crate::launcher_health::now_iso()}),
    );
    // A failed recovery may leave a foreign or partially-swapped App. Do not
    // execute whatever self-manifest happens to be present while evidence holds.
    if ok && !l.journal.exists() {
        relaunch(&l);
    }
    if ok { 0 } else { 1 }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn temp() -> PathBuf {
        let p =
            std::env::temp_dir().join(format!("launcher-update-{}", crate::backend::unique_id()));
        fs::create_dir_all(&p).unwrap();
        p
    }
    fn app(dir: &Path, version: &str, body: &[u8]) -> Manifest {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("2k Launcher Rust.exe"), body).unwrap();
        fs::write(dir.join("README.md"), b"notes").unwrap();
        let files = inventory(dir)
            .unwrap()
            .into_iter()
            .map(|(path, size)| Entry {
                sha256: sha256(&dir.join(&path)).unwrap(),
                path,
                size,
            })
            .collect();
        let m = Manifest {
            schema: 1,
            product: PRODUCT.into(),
            version: version.into(),
            channel: "QA".into(),
            notes: vec![],
            built_at: String::new(),
            files,
        };
        write_json(&dir.join(MANIFEST), &m).unwrap();
        m
    }
    #[test]
    fn names_versions_and_user_data_are_rejected_like_wpf() {
        assert!(safe_name("App.exe") && safe_name("docs/a.md"));
        for bad in ["..\\x", "a/../b", "con.txt", "a.", "C:x", "a\\b"] {
            assert!(!safe_name(bad), "{bad}");
        }
        assert!(version_ok("0.50.0") && version_ok("0.1.0-wpf.24") && !version_ok("1.2"));
        let root = temp();
        let mut m = app(&root.join("App"), "0.50.0", b"exe");
        m.files.push(Entry {
            path: "Data/settings.json".into(),
            size: 1,
            sha256: "A".repeat(64),
        });
        assert!(
            validate_manifest(&m)
                .unwrap_err()
                .to_string()
                .contains("user data")
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn incoming_authentication_is_closed_and_versions_cannot_downgrade() {
        let root = temp();
        let mut current = app(&root.join("Current"), "0.50.0-wpf.10", b"current");
        let mut incoming = current.clone();
        for (version, ok) in [
            ("0.49.9", false),
            ("0.50.0-wpf.9", false),
            ("0.50.0-wpf.10", true),
            ("0.50.0-wpf.11", true),
            ("0.50.0", true),
            ("0.51.0", true),
        ] {
            incoming.version = version.into();
            assert_eq!(version_policy(&current, &incoming).is_ok(), ok, "{version}");
        }
        current.version = "0.50.0".into();
        current.channel = "Main".into();
        incoming = current.clone();
        assert!(version_policy(&current, &incoming).is_err());
        incoming.version = "0.50.0-wpf.99".into();
        assert!(version_policy(&current, &incoming).is_err());
        assert!(
            publisher_authentication(&incoming)
                .unwrap_err()
                .to_string()
                .contains("no provisioned publisher")
        );
        let l = locations(&root.join("Portable"));
        app(&l.app, "0.50.0", b"original");
        app(&l.previous, "0.49.0", b"previous");
        app(&l.staged, "9.0.0", b"self-manifest is not authenticity");
        assert!(apply(&l).is_err());
        assert!(!l.journal.exists());
        assert_eq!(
            fs::read(l.app.join("2k Launcher Rust.exe")).unwrap(),
            b"original"
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn recovery_retains_journal_for_valid_but_foreign_app_or_previous() {
        let root = temp();
        let l = locations(&root);
        app(&l.app, "0.49.0", b"original");
        app(&l.previous, "0.48.0", b"older");
        app(&l.staged, "0.50.0", b"incoming");
        let mut j = begin_journal(&l).unwrap();
        phase(&l, &mut j, "old-moved").unwrap();
        rename(&l.previous, &l.displaced).unwrap();
        rename(&l.app, &l.previous).unwrap();
        rename(&l.staged, &l.app).unwrap();
        // A different self-consistent release must never count as restored.
        let foreign = root.join("Foreign");
        app(&foreign, "0.49.0", b"original");
        rename(&l.app, &l.staged).unwrap();
        rename(&foreign, &l.app).unwrap();
        assert!(recover(&l).is_err());
        assert!(l.journal.exists());
        assert_eq!(
            fs::read(l.previous.join("2k Launcher Rust.exe")).unwrap(),
            b"original"
        );
        // Restore the exact incoming directory, then independently finish undo.
        rename(&l.app, &foreign).unwrap();
        rename(&l.staged, &l.app).unwrap();
        assert!(recover(&l).unwrap());
        assert!(!l.journal.exists());
        assert_eq!(
            directory_id(&l.app).unwrap(),
            j.original_app.unwrap().directory
        );
        assert_eq!(
            directory_id(&l.previous).unwrap(),
            j.original_previous.unwrap().directory
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn legacy_recovery_without_preimage_keeps_all_evidence() {
        let root = temp();
        let l = locations(&root);
        app(&l.app, "0.50.0", b"current");
        write_json(
            &l.journal,
            &json!({"schema":1,"phase":"old-moved","version":"0.50.0","hadPrevious":false}),
        )
        .unwrap();
        assert!(recover(&l).is_err());
        assert!(l.journal.exists());
        assert_eq!(
            fs::read(l.app.join("2k Launcher Rust.exe")).unwrap(),
            b"current"
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn tampered_or_extra_files_fail_verification() {
        let root = temp();
        let dir = root.join("Pkg/App");
        let m = app(&dir, "0.50.0", b"new exe");
        verify(&dir, &m, &|_| {}).unwrap();
        fs::write(dir.join("2k Launcher Rust.exe"), b"NEW EXE").unwrap();
        assert!(
            verify(&dir, &m, &|_| {})
                .unwrap_err()
                .to_string()
                .contains("verification failed")
        );
        fs::write(dir.join("2k Launcher Rust.exe"), b"new exe").unwrap();
        fs::write(dir.join("extra.dll"), b"x").unwrap();
        assert!(
            verify(&dir, &m, &|_| {})
                .unwrap_err()
                .to_string()
                .contains("unexpected")
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn apply_swaps_keeps_one_previous_and_rollback_round_trips() {
        let root = temp();
        let l = locations(&root);
        app(&l.app, "0.49.0", b"old");
        app(&l.previous, "0.48.0", b"older");
        app(&l.staged, "0.50.0", b"new");
        apply_transaction(&l).unwrap();
        assert_eq!(read_manifest(&l.app).unwrap().unwrap().version, "0.50.0");
        assert_eq!(
            read_manifest(&l.previous).unwrap().unwrap().version,
            "0.49.0"
        );
        assert!(!l.journal.exists() && !l.displaced.exists() && !l.staged.exists());
        // Manual rollback = stage PreviousApp and apply again.
        let m = read_manifest(&l.previous).unwrap().unwrap();
        copy_tree(&l.previous, &l.staged, &m.files).unwrap();
        write_json(&l.staged.join(MANIFEST), &m).unwrap();
        apply(&l).unwrap();
        assert_eq!(read_manifest(&l.app).unwrap().unwrap().version, "0.49.0");
        assert_eq!(
            read_manifest(&l.previous).unwrap().unwrap().version,
            "0.50.0"
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn interrupted_swap_recovers_the_old_app() {
        let root = temp();
        let l = locations(&root);
        app(&l.app, "0.49.0", b"old");
        app(&l.staged, "0.50.0", b"new");
        // Simulate a crash after the old App moved but before the new one landed.
        let mut j = begin_journal(&l).unwrap();
        phase(&l, &mut j, "old-moved").unwrap();
        rename(&l.app, &l.previous).unwrap();
        assert!(recover(&l).unwrap());
        assert_eq!(read_manifest(&l.app).unwrap().unwrap().version, "0.49.0");
        assert!(!l.journal.exists());
        // Crash after commit: the new App stays.
        app(&l.staged, "0.50.0", b"new");
        let mut j = begin_journal(&l).unwrap();
        apply_transaction(&l).unwrap();
        phase(&l, &mut j, "committed").unwrap();
        assert!(recover(&l).unwrap());
        assert_eq!(read_manifest(&l.app).unwrap().unwrap().version, "0.50.0");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn build_info_packages_verify_the_exe_hash() {
        let root = temp();
        let pkg = root.join("Portable");
        fs::create_dir_all(pkg.join("App")).unwrap();
        fs::write(pkg.join("App/2k Launcher Rust Prototype.exe"), b"binary").unwrap();
        let sha = sha256(&pkg.join("App/2k Launcher Rust Prototype.exe")).unwrap();
        fs::write(
            pkg.join("BUILD-INFO.json"),
            format!("\u{feff}{{\"version\":\"0.50.0\",\"sha256\":\"{sha}\",\"bytes\":6}}"),
        )
        .unwrap();
        let l = locations(&root.join("Live"));
        let (app, m) = package(&pkg, &l).unwrap();
        assert_eq!(app, pkg.join("App"));
        assert_eq!(m.version, "0.50.0");
        fs::write(
            pkg.join("BUILD-INFO.json"),
            "{\"version\":\"0.50.0\",\"sha256\":\"00\",\"bytes\":6}",
        )
        .unwrap();
        assert!(package(&pkg, &l).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(feature = "dev-hub-updates")]
#[path = "launcher_update_dev_hub.rs"]
pub(crate) mod dev_hub;
