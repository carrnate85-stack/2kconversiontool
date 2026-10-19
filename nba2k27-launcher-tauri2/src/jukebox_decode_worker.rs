//! Bounded same-EXE audio helper protocol and exact-owned Win32 Job guardian.
//! A codec failure/timeout/cancel kills only our retained child; no process-name
//! enumeration, game access, Steam, external encoder, hidden persistent worker,
//! temporary music files, registry, AppData or volume changes.
#![allow(dead_code)]
use crate::{jukebox_audio, jukebox_catalog as catalog, jukebox_decoder};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, BufRead, BufReader, Read, Write},
    mem::size_of,
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
        process::CommandExt,
    },
    path::Path,
    path::{Component, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::{CloseHandle, FILETIME, HANDLE},
    System::{JobObjects::*, Threading::GetProcessTimes},
};
#[path = "jukebox_decode_admission.rs"]
pub(crate) mod admission;
const SCHEMA: &str = "nba2k27-jukebox-decode-v1";
const MAX_LINE: usize = 4096;
const MAX_OUTPUT: usize = jukebox_audio::MAX_FRAMES as usize * 4;
fn declared_output_limit(input: &[u8]) -> Result<usize> {
    if input.len() >= 42
        && input.starts_with(b"fLaC")
        && input[4] & 127 == 0
        && input[5..8] == [0, 0, 34]
    {
        let info = u64::from_be_bytes(input[18..26].try_into()?);
        let samples = info & 0xfffffffff;
        if samples > 0
            && info >> 44 == 48000
            && ((info >> 41) & 7) + 1 == 2
            && ((info >> 36) & 31) + 1 == 16
        {
            let bytes = samples.checked_mul(4).context("FLAC size overflow")?;
            ensure!(
                bytes <= MAX_OUTPUT as u64,
                "FLAC decoded size exceeds bound"
            );
            return Ok(bytes as usize);
        }
    }
    Ok(MAX_OUTPUT)
}
fn effective_output_limit(input: &[u8], requested: usize) -> Result<usize> {
    ensure!(
        (4..=MAX_OUTPUT).contains(&requested) && requested % 4 == 0,
        "Invalid private audio output bound"
    );
    Ok(declared_output_limit(input)?.min(requested))
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    schema: String,
    nonce: String,
    size: usize,
    sha256: String,
    #[serde(default)]
    output_limit: Option<usize>,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum Message {
    Progress {
        nonce: String,
        frames: u32,
    },
    Complete {
        nonce: String,
        size: usize,
        sha256: String,
    },
    Error {
        nonce: String,
        message: String,
    },
}
fn line(reader: &mut impl BufRead) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    reader
        .by_ref()
        .take(MAX_LINE as u64 + 1)
        .read_until(b'\n', &mut out)?;
    ensure!(
        !out.is_empty() && out.len() <= MAX_LINE && out.last() == Some(&b'\n'),
        "Audio helper header is missing/oversized"
    );
    Ok(out)
}
fn send(writer: &mut impl Write, message: &Message) -> Result<()> {
    let b = serde_json::to_vec(message)?;
    ensure!(b.len() < MAX_LINE, "Audio helper header exceeds bounds");
    writer.write_all(&b)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}
/// Main dispatch: `--jukebox-decode-worker <48hex nonce>` before Tauri init.
/// Success exits0; return errors through normal stderr+nonzero process exit.
pub(crate) fn run(nonce: &str) -> Result<()> {
    ensure!(
        catalog::hex(nonce, 48),
        "Invalid private audio worker identity"
    );
    let stdin = io::stdin();
    let stdout = io::stdout();
    let (mut input, mut output) = (BufReader::new(stdin.lock()), stdout.lock());
    let result = (|| -> Result<()> {
        let request: Request = serde_json::from_slice(&line(&mut input)?)?;
        ensure!(
            request.schema == SCHEMA
                && request.nonce == nonce
                && (1..=jukebox_audio::MAX_AUDIO).contains(&request.size)
                && catalog::hex(&request.sha256, 64),
            "Invalid private audio worker request"
        );
        let requested_limit = request.output_limit.unwrap_or(MAX_OUTPUT);
        ensure!(
            (4..=MAX_OUTPUT).contains(&requested_limit) && requested_limit % 4 == 0,
            "Invalid private audio output bound"
        );
        let mut source = vec![0; request.size];
        input.read_exact(&mut source)?;
        let mut trailing = [0];
        ensure!(
            input.read(&mut trailing)? == 0
                && catalog::digest(&source).eq_ignore_ascii_case(&request.sha256),
            "Audio worker input length/hash differs"
        );
        let start = Instant::now();
        let output_limit = effective_output_limit(&source, requested_limit)?;
        let mut last = Instant::now();
        let pcm = jukebox_decoder::decode_pcm(
            &source,
            || {
                ensure!(
                    start.elapsed() < Duration::from_secs(120),
                    "Audio decode timed out"
                );
                Ok(())
            },
            |frames| {
                ensure!(
                    (frames as usize).saturating_mul(4) <= output_limit,
                    "Audio exceeds declared FLAC decoded size"
                );
                if last.elapsed() >= Duration::from_millis(200) {
                    send(
                        &mut output,
                        &Message::Progress {
                            nonce: nonce.into(),
                            frames,
                        },
                    )?;
                    last = Instant::now();
                }
                Ok(())
            },
        )?;
        ensure!(
            !pcm.is_empty() && pcm.len() <= output_limit && pcm.len() % 4 == 0,
            "Invalid audio worker PCM output"
        );
        send(
            &mut output,
            &Message::Complete {
                nonce: nonce.into(),
                size: pcm.len(),
                sha256: catalog::digest(&pcm),
            },
        )?;
        output.write_all(&pcm)?;
        output.flush()?;
        Ok(())
    })();
    if let Err(error) = &result {
        let message = error.to_string().chars().take(1024).collect::<String>();
        let _ = send(
            &mut output,
            &Message::Error {
                nonce: nonce.into(),
                message,
            },
        );
    }
    result
}
struct Job(HANDLE);
unsafe impl Send for Job {}
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            let _ = TerminateJobObject(self.0, 1);
            let _ = CloseHandle(self.0);
        }
    }
}
fn job(child: &Child) -> Result<Job> {
    let handle = unsafe { CreateJobObjectW(None, None)? };
    let job = Job(handle);
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
        | JOB_OBJECT_LIMIT_PROCESS_MEMORY;
    limits.BasicLimitInformation.ActiveProcessLimit = 1;
    limits.ProcessMemoryLimit = 1024 * 1024 * 1024;
    unsafe {
        SetInformationJobObject(
            job.0,
            JobObjectExtendedLimitInformation,
            &limits as *const _ as _,
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )?;
        AssignProcessToJobObject(job.0, HANDLE(child.as_raw_handle()))?;
    }
    Ok(job)
}
struct OwnedChild {
    child: Child,
    _job: Option<Job>,
    registered: Option<ChildIdentity>,
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        self._job.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(identity) = self.registered.take() {
            if let Ok(mut registry) = children().lock() {
                if registry.get(&identity.pid) == Some(&identity) {
                    registry.remove(&identity.pid);
                }
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct ChildIdentity {
    pid: u32,
    started: u64,
    path: std::path::PathBuf,
}
fn children() -> &'static Mutex<BTreeMap<u32, ChildIdentity>> {
    static CHILDREN: OnceLock<Mutex<BTreeMap<u32, ChildIdentity>>> = OnceLock::new();
    CHILDREN.get_or_init(|| Mutex::new(BTreeMap::new()))
}
fn register(child: &Child, path: &Path) -> Result<ChildIdentity> {
    let (mut creation, mut exit, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    unsafe {
        GetProcessTimes(
            HANDLE(child.as_raw_handle()),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )?;
    }
    let identity = ChildIdentity {
        pid: child.id(),
        started: (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime),
        path: path.into(),
    };
    let mut registry = children()
        .lock()
        .map_err(|_| anyhow::anyhow!("Audio helper ownership registry unavailable"))?;
    ensure!(
        !registry.contains_key(&identity.pid),
        "Audio helper PID already registered"
    );
    registry.insert(identity.pid, identity.clone());
    Ok(identity)
}
/// The global OTHER-LAUNCHER guard may exclude only this exact retained child
/// identity, never a name/PID-only exception. Call in the same GUI process that
/// owns decoding; a provider worker does not inherit this process-local registry.
pub(crate) fn owns_process(pid: u32, started: &str, path: &Path) -> bool {
    let Ok(started) = started.parse::<u64>() else {
        return false;
    };
    let Ok(registry) = children().lock() else {
        return false;
    };
    let Some(identity) = registry.get(&pid) else {
        return false;
    };
    fn path_text(path: &Path) -> String {
        let p = path.to_string_lossy().replace('/', "\\");
        p.strip_prefix(r"\\?\").unwrap_or(&p).to_string()
    }
    identity.started == started && path_text(&identity.path).eq_ignore_ascii_case(&path_text(path))
}
enum Event {
    Progress(u32),
    Complete(Vec<u8>),
    Error(anyhow::Error),
}
struct ExecutableParents(Vec<File>);
fn pin_parents(path: &Path) -> Result<ExecutableParents> {
    let mut current = PathBuf::new();
    let mut files = Vec::new();
    for c in path
        .parent()
        .context("Audio helper path has no parent")?
        .components()
    {
        ensure!(
            !matches!(c, Component::ParentDir | Component::CurDir),
            "Unnormalized audio helper path"
        );
        current.push(c.as_os_str());
        if matches!(c, Component::Prefix(_)) {
            continue;
        }
        let f = fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .custom_flags(0x02000000 | 0x00200000)
            .open(&current)?;
        let m = f.metadata()?;
        ensure!(
            m.is_dir() && m.file_attributes() & 0x400 == 0,
            "Audio helper has a linked/non-directory parent"
        );
        files.push(f);
    }
    Ok(ExecutableParents(files))
}
fn read_output(
    reader: impl Read,
    nonce: &str,
    sender: &mpsc::Sender<Event>,
    output_limit: usize,
) -> Result<()> {
    let mut input = BufReader::new(reader);
    let mut last = 0u32;
    for _ in 0..1024 {
        let message: Message = serde_json::from_slice(&line(&mut input)?)?;
        match message {
            Message::Progress { nonce: n, frames } => {
                ensure!(
                    n == nonce && frames >= last && frames <= jukebox_audio::MAX_FRAMES,
                    "Audio helper progress identity/value differs"
                );
                last = frames;
                sender.send(Event::Progress(frames))?;
            }
            Message::Complete {
                nonce: n,
                size,
                sha256,
            } => {
                ensure!(
                    n == nonce
                        && (4..=output_limit).contains(&size)
                        && size % 4 == 0
                        && catalog::hex(&sha256, 64),
                    "Invalid audio helper result declaration"
                );
                let mut pcm = vec![0; size];
                input.read_exact(&mut pcm)?;
                let mut trailing = [0];
                ensure!(
                    input.read(&mut trailing)? == 0
                        && catalog::digest(&pcm).eq_ignore_ascii_case(&sha256),
                    "Audio helper result hash/trailing bytes differ"
                );
                sender.send(Event::Complete(pcm))?;
                return Ok(());
            }
            Message::Error { nonce: n, message } => {
                ensure!(
                    n == nonce && message.len() <= 4096,
                    "Audio helper error identity differs"
                );
                anyhow::bail!("Audio conversion failed: {message}");
            }
        }
    }
    anyhow::bail!("Audio helper exceeded progress message bound")
}
/// Call on a bounded application worker, never UI. `executable` MUST be the
/// current installed launcher executable; tests use their explicit fixture EXE.
/// Cancellation is sampled25ms and closes a kill-on-close exact-child Job.
#[cfg(test)]
pub(crate) fn decode(
    executable: &Path,
    input: &[u8],
    nonce: &str,
    cancel: Arc<AtomicBool>,
    mut progress: impl FnMut(u32) -> Result<()>,
) -> Result<Vec<u8>> {
    decode_bounded(
        executable,
        input,
        nonce,
        cancel,
        &mut progress,
        Duration::from_secs(130),
        || Ok(()),
        None,
    )
}
/// The only production child-creation entry point. Admission is revalidated
/// after executable/parent pinning immediately adjacent to CreateProcess.
pub(crate) fn decode_guarded(
    executable: &Path,
    input: &[u8],
    nonce: &str,
    cancel: Arc<AtomicBool>,
    mut progress: impl FnMut(u32) -> Result<()>,
    guard: impl FnMut() -> Result<()>,
) -> Result<Vec<u8>> {
    let _exclusive = admission::hold()?;
    decode_bounded(
        executable,
        input,
        nonce,
        cancel,
        &mut progress,
        Duration::from_secs(130),
        guard,
        None,
    )
}
pub(crate) fn decode_guarded_limit(
    executable: &Path,
    input: &[u8],
    nonce: &str,
    cancel: Arc<AtomicBool>,
    progress: impl FnMut(u32) -> Result<()>,
    guard: impl FnMut() -> Result<()>,
    limit: usize,
) -> Result<Vec<u8>> {
    let _exclusive = admission::hold()?;
    decode_bounded(
        executable,
        input,
        nonce,
        cancel,
        progress,
        Duration::from_secs(130),
        guard,
        Some(limit),
    )
}
fn decode_bounded(
    executable: &Path,
    input: &[u8],
    nonce: &str,
    cancel: Arc<AtomicBool>,
    mut progress: impl FnMut(u32) -> Result<()>,
    deadline: Duration,
    mut guard: impl FnMut() -> Result<()>,
    requested_limit: Option<usize>,
) -> Result<Vec<u8>> {
    ensure!(
        executable.is_absolute()
            && catalog::hex(nonce, 48)
            && !input.is_empty()
            && input.len() <= jukebox_audio::MAX_AUDIO,
        "Invalid bounded audio helper request"
    );
    ensure!(!cancel.load(Ordering::Acquire), "Audio import cancelled");
    let _parents = pin_parents(executable)?;
    let requested = requested_limit.unwrap_or(MAX_OUTPUT);
    ensure!(
        (4..=MAX_OUTPUT).contains(&requested) && requested % 4 == 0,
        "Invalid private audio output bound"
    );
    let output_limit = effective_output_limit(input, requested)?;
    let pinned = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .custom_flags(0x00200000)
        .open(executable)?;
    let meta = pinned.metadata()?;
    ensure!(
        meta.is_file() && meta.file_attributes() & 0x400 == 0,
        "Audio helper executable must be an ordinary pinned file"
    );
    let executable = executable.canonicalize()?;
    guard().context("Fresh audio decoder admission refused; no child was created")?;
    ensure!(!cancel.load(Ordering::Acquire), "Audio import cancelled");
    let child = Command::new(&executable)
        .arg("--jukebox-decode-worker")
        .arg(nonce)
        .creation_flags(0x08000000)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Starting the owned native audio helper")?;
    let mut owned = OwnedChild {
        child,
        _job: None,
        registered: None,
    };
    owned._job = Some(job(&owned.child)?);
    owned.registered = Some(register(&owned.child, &executable)?);
    // The child blocks on input until Job assignment succeeds. It cannot begin
    // native decoding in the brief spawn→job ownership window.
    let mut stdin = owned
        .child
        .stdin
        .take()
        .context("Missing audio helper input pipe")?;
    let stdout = owned
        .child
        .stdout
        .take()
        .context("Missing audio helper output pipe")?;
    let mut stderr = owned
        .child
        .stderr
        .take()
        .context("Missing audio helper error pipe")?;
    let request = Request {
        schema: SCHEMA.into(),
        nonce: nonce.into(),
        size: input.len(),
        sha256: catalog::digest(input),
        output_limit: Some(output_limit),
    };
    let header = serde_json::to_vec(&request)?;
    let payload = input.to_vec();
    let writer = thread::spawn(move || -> Result<()> {
        stdin.write_all(&header)?;
        stdin.write_all(b"\n")?;
        stdin.write_all(&payload)?;
        stdin.flush()?;
        Ok(())
    });
    let (error_tx, error_rx) = mpsc::channel();
    let errors = thread::spawn(move || {
        let mut retained = Vec::new();
        let mut buffer = [0u8; 512];
        loop {
            match stderr.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if retained.len() < 8192 {
                        retained.extend_from_slice(&buffer[..n.min(8192 - retained.len())]);
                    }
                }
            }
        }
        let _ = error_tx.send(String::from_utf8_lossy(&retained).into_owned());
    });
    let (tx, rx) = mpsc::channel();
    let nonce = nonce.to_string();
    let reader = thread::spawn(move || {
        if let Err(error) = read_output(stdout, &nonce, &tx, output_limit) {
            let _ = tx.send(Event::Error(error));
        }
    });
    let start = Instant::now();
    let result = (|| -> Result<Vec<u8>> {
        loop {
            ensure!(!cancel.load(Ordering::Acquire), "Audio import cancelled");
            ensure!(
                start.elapsed() < deadline,
                "Native audio helper timed out; original source retained"
            );
            match rx.recv_timeout(Duration::from_millis(25)) {
                Ok(Event::Progress(frames)) => progress(frames)?,
                Ok(Event::Complete(pcm)) => {
                    let end = Instant::now();
                    loop {
                        ensure!(
                            !cancel.load(Ordering::Acquire) && start.elapsed() < deadline,
                            "Audio helper canceled or timed out after output"
                        );
                        if let Some(status) = owned.child.try_wait()? {
                            ensure!(status.success(), "Audio helper failed after output");
                            return Ok(pcm);
                        }
                        ensure!(
                            end.elapsed() < Duration::from_secs(2),
                            "Audio helper did not exit after output"
                        );
                        thread::sleep(Duration::from_millis(10));
                    }
                }
                Ok(Event::Error(error)) => return Err(error),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    anyhow::bail!("Audio helper output channel closed unexpectedly")
                }
            }
        }
    })();
    // Always terminate/reap BEFORE joining pipes: no blocked writer/read thread
    // can prolong application shutdown behind an unresponsive native codec.
    drop(owned);
    let write_result = writer.join();
    let _ = reader.join();
    let _ = errors.join();
    drop(pinned);
    match result {
        Ok(pcm) => {
            write_result.map_err(|_| anyhow::anyhow!("Audio helper writer panicked"))??;
            Ok(pcm)
        }
        Err(error) => {
            let tail = error_rx.try_recv().unwrap_or_default();
            if tail.trim().is_empty() {
                Err(error)
            } else {
                Err(error.context(tail.chars().take(1024).collect::<String>()))
            }
        }
    }
}
#[cfg(test)]
pub(crate) fn decode_test_deadline(
    executable: &Path,
    input: &[u8],
    nonce: &str,
    deadline: Duration,
) -> Result<Vec<u8>> {
    ensure!(
        deadline <= Duration::from_secs(130),
        "Invalid test deadline"
    );
    decode_bounded(
        executable,
        input,
        nonce,
        Arc::new(AtomicBool::new(false)),
        |_| Ok(()),
        deadline,
        || Ok(()),
        None,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flac_declared_size_bounds_output_before_allocating_pcm() {
        let mut input = vec![0; 42];
        input[..4].copy_from_slice(b"fLaC");
        input[7] = 34;
        let info = (48000u64 << 44) | (1 << 41) | (15 << 36) | 2;
        input[18..26].copy_from_slice(&info.to_be_bytes());
        assert_eq!(declared_output_limit(&input).unwrap(), 8);
        let nonce = "a".repeat(48);
        let msg = Message::Complete {
            nonce: nonce.clone(),
            size: 12,
            sha256: "a".repeat(64),
        };
        let mut encoded = serde_json::to_vec(&msg).unwrap();
        encoded.push(b'\n');
        let (tx, _) = mpsc::channel();
        assert_eq!(
            read_output(encoded.as_slice(), &nonce, &tx, 8)
                .unwrap_err()
                .to_string(),
            "Invalid audio helper result declaration"
        );
        let unknown = (48000u64 << 44) | (1 << 41) | (15 << 36);
        input[18..26].copy_from_slice(&unknown.to_be_bytes());
        assert_eq!(declared_output_limit(&input).unwrap(), MAX_OUTPUT);
        assert_eq!(effective_output_limit(&input, 8).unwrap(), 8);
        assert!(effective_output_limit(&input, 0).is_err());
        let info =
            (48000u64 << 44) | (1 << 41) | (15 << 36) | (jukebox_audio::MAX_FRAMES as u64 + 1);
        input[18..26].copy_from_slice(&info.to_be_bytes());
        assert!(declared_output_limit(&input).is_err());
    }
    #[test]
    fn final_admission_guard_runs_after_executable_pinning_and_refuses_child_creation() {
        let called = AtomicBool::new(false);
        let result = decode_bounded(
            &std::env::current_exe().unwrap(),
            b"encoded fixture",
            &"a".repeat(48),
            Arc::new(AtomicBool::new(false)),
            |_| Ok(()),
            Duration::from_millis(10),
            || {
                called.store(true, Ordering::Release);
                anyhow::bail!("authoritative game became active")
            },
            None,
        );
        assert!(called.load(Ordering::Acquire));
        assert!(format!("{:#}", result.unwrap_err()).contains("no child was created"));
    }
    #[test]
    fn helper_headers_and_output_are_bounded_and_identity_checked() {
        let (tx, _) = mpsc::channel();
        assert!(read_output(&b"not-json\n"[..], &"a".repeat(48), &tx, MAX_OUTPUT).is_err());
        let msg = Message::Progress {
            nonce: "b".repeat(48),
            frames: 1,
        };
        let mut data = serde_json::to_vec(&msg).unwrap();
        data.push(b'\n');
        assert!(read_output(&data[..], &"a".repeat(48), &tx, MAX_OUTPUT).is_err());
        let oversized = vec![b'x'; MAX_LINE + 1];
        let mut reader = BufReader::new(oversized.as_slice());
        assert!(line(&mut reader).is_err());
    }
    #[test]
    fn process_exception_requires_full_registered_birth_and_path() {
        let identity = ChildIdentity {
            pid: u32::MAX,
            started: 123,
            path: std::path::PathBuf::from(r"C:\Fixture\Launcher.exe"),
        };
        children()
            .lock()
            .unwrap()
            .insert(identity.pid, identity.clone());
        assert!(owns_process(
            identity.pid,
            "123",
            Path::new(r"\\?\C:\Fixture\Launcher.exe")
        ));
        assert!(!owns_process(identity.pid, "124", &identity.path));
        assert!(!owns_process(
            identity.pid,
            "123",
            Path::new(r"C:\Other\Launcher.exe")
        ));
        assert!(!owns_process(1, "123", &identity.path));
        children().lock().unwrap().remove(&identity.pid);
    }
}
