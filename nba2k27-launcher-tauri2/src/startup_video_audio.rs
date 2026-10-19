//! Fixed purpose-built, pipe-owned CoreAudio guardian. Not in a kill-on-close
//! job: EOF requests conditional restoration before the child exits. No files,
//! game memory, endpoint volume setters, injected input or external executables.
#![allow(dead_code)]
#[cfg(feature = "startup-audio-worker")]
use crate::startup_audio_guardian;
use crate::{
    startup_video_helper::Binary,
    startup_video_native::ExpectedGame,
    startup_video_policy::{GameKey, Volume},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[cfg(feature = "startup-audio-worker")]
use std::{fs, os::windows::fs::OpenOptionsExt};
use std::{
    io::{BufRead, BufReader, Read, Write},
    os::windows::{io::AsRawHandle, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, FILETIME, HANDLE, WAIT_TIMEOUT},
        Security::Cryptography::{BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom},
        System::{Diagnostics::ToolHelp::*, JobObjects::IsProcessInJob, Threading::*},
    },
    core::PWSTR,
};

const SCHEMA: &str = "nba2k27-startup-audio-child-v1";
const MAX_PACKET: usize = 64 * 1024;
pub(crate) const FLAG: &str = "--startup-audio-guardian";
fn hex(s: &str) -> bool {
    s.len() == 48 && s.bytes().all(|b| b.is_ascii_hexdigit())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Identity {
    pid: u32,
    birth: u64,
    exe: PathBuf,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Bootstrap {
    schema: String,
    nonce: String,
    session: String,
    helper_sha256: String,
    parent: Identity,
    game: Identity,
}
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
fn identity(handle: HANDLE) -> Result<Identity> {
    ensure!(
        unsafe { WaitForSingleObject(handle, 0) } == WAIT_TIMEOUT,
        "Audio guardian process is not alive"
    );
    let (mut birth, mut exit, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    unsafe {
        GetProcessTimes(handle, &mut birth, &mut exit, &mut kernel, &mut user)?;
    }
    let mut buf = vec![0u16; 32768];
    let mut len = buf.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )?;
    }
    ensure!(
        len > 0 && (len as usize) < buf.len(),
        "Audio guardian executable path is invalid"
    );
    let result = Identity {
        pid: unsafe { GetProcessId(handle) },
        birth: ((birth.dwHighDateTime as u64) << 32) | birth.dwLowDateTime as u64,
        exe: String::from_utf16(&buf[..len as usize])?.into(),
    };
    ensure!(
        result.pid > 0
            && result.birth > 0
            && unsafe { WaitForSingleObject(handle, 0) } == WAIT_TIMEOUT,
        "Audio guardian identity changed"
    );
    Ok(result)
}
fn normalize(path: &Path) -> String {
    path.to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_ascii_lowercase()
}
fn same_identity(a: &Identity, b: &Identity) -> bool {
    a.pid == b.pid && a.birth == b.birth && normalize(&a.exe) == normalize(&b.exe)
}
fn read_packet(reader: &mut impl Read) -> Result<Vec<u8>> {
    let mut size = [0; 4];
    reader.read_exact(&mut size)?;
    let size = u32::from_le_bytes(size) as usize;
    ensure!(
        (1..=MAX_PACKET).contains(&size),
        "Private audio bootstrap exceeds bounds"
    );
    let mut bytes = vec![0; size];
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}
fn validate_bootstrap(
    b: &Bootstrap,
    nonce: &str,
    actual_parent: &Identity,
    self_exe: &Path,
    actual_parent_pid: u32,
) -> Result<ExpectedGame> {
    ensure!(
        b.schema == SCHEMA && hex(nonce) && b.nonce == nonce && hex(&b.session),
        "Untrusted audio guardian bootstrap"
    );
    ensure!(
        actual_parent_pid == b.parent.pid && same_identity(&b.parent, actual_parent),
        "Audio guardian parent identity differs"
    );
    let helper_name = format!("startup-audio-{}.exe", b.helper_sha256.to_ascii_lowercase());
    ensure!(
        b.helper_sha256.len() == 64
            && b.helper_sha256.bytes().all(|b| b.is_ascii_hexdigit())
            && self_exe
                .file_name()
                .is_some_and(|n| n.eq_ignore_ascii_case(&helper_name)),
        "Unexpected fixed audio guardian executable"
    );
    let runtime = self_exe.parent().context("Audio helper cache missing")?;
    let cache = runtime
        .parent()
        .context("Audio helper cache root missing")?;
    let root = cache
        .parent()
        .context("Audio helper portable root missing")?;
    ensure!(
        runtime
            .file_name()
            .is_some_and(|n| n.eq_ignore_ascii_case("Runtime"))
            && cache
                .file_name()
                .is_some_and(|n| n.eq_ignore_ascii_case("Cache")),
        "Audio helper must be inside portable Cache/Runtime"
    );
    let parent_dir = actual_parent
        .exe
        .parent()
        .context("Launcher parent folder missing")?;
    let parent_root = if parent_dir
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case("App"))
    {
        parent_dir
            .parent()
            .context("Launcher portable root missing")?
    } else {
        parent_dir
    };
    ensure!(
        normalize(root) == normalize(parent_root)
            && actual_parent.exe.file_name().is_some_and(|name| [
                "2k Launcher Rust Prototype.exe",
                "nba2k27-launcher-tauri2.exe",
                "2k Launcher.exe"
            ]
            .iter()
            .any(|n| name.eq_ignore_ascii_case(n))),
        "Audio guardian creator is not its portable launcher"
    );
    ensure!(
        b.game.pid != b.parent.pid,
        "Audio guardian game cannot be its parent"
    );
    ExpectedGame::new(
        GameKey {
            pid: b.game.pid,
            birth: b.game.birth,
        },
        &b.game.exe,
    )
}
fn actual_parent_pid() -> Result<u32> {
    let snapshot = Handle(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)? });
    let mut row = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    unsafe {
        Process32FirstW(snapshot.0, &mut row)?;
    }
    for _ in 0..100_000 {
        if row.th32ProcessID == std::process::id() {
            return Ok(row.th32ParentProcessID);
        }
        if unsafe { Process32NextW(snapshot.0, &mut row) }.is_err() {
            break;
        }
    }
    anyhow::bail!("Audio guardian could not identify its actual creator")
}
/// Dispatch before Tauri/storage/instance initialization. The only argument is
/// a random nonce; the exact game and parent arrive over private standard input.
#[cfg(feature = "startup-audio-worker")]
pub(crate) fn run(nonce: &str) -> Result<()> {
    ensure!(hex(nonce), "Invalid private audio guardian nonce");
    let b: Bootstrap = serde_json::from_slice(&read_packet(&mut std::io::stdin().lock())?)?;
    ensure!(
        b.nonce == nonce && hex(&b.session),
        "Invalid private audio session bootstrap"
    );
    // All fallible setup below precedes the only drive/SetMute entrypoint.
    // Only this lexical pre-drive scope may attest ownsMutation:false. A crash
    // or failed pipe after drive starts can never be translated into that ACK.
    let prepared = (|| -> Result<_> {
        let parent = Handle(unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                false,
                b.parent.pid,
            )?
        });
        let actual = identity(parent.0)?;
        let exe = std::env::current_exe()?;
        let mut pin = fs::OpenOptions::new().read(true).share_mode(1).open(&exe)?;
        let expected = validate_bootstrap(&b, nonce, &actual, &exe, actual_parent_pid()?)?;
        ensure!(
            crate::startup_video_helper::hash(&mut pin)?.eq_ignore_ascii_case(&b.helper_sha256),
            "Audio guardian binary changed"
        );
        // Deny an inherited kill-on-parent job before any audio mutation. The
        // parent uses BREAKAWAY, and cannot fall back to a job-bound mute guardian.
        let mut in_job = windows::core::BOOL(0);
        unsafe {
            IsProcessInJob(GetCurrentProcess(), None, &mut in_job)?;
        }
        ensure!(
            !in_job.as_bool(),
            "Audio guardian cannot run inside a parent-owned job"
        );
        let audio = startup_audio_guardian::NativeAudio::bind(expected.clone())?;
        let guardian = startup_audio_guardian::Guardian::new(audio, expected.key, true, 0)?;
        Ok((parent, actual, pin, guardian))
    })();
    let (parent, actual, _pin, mut guardian) = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            let packet = not_started(nonce, &b.session, &format!("{error:#}"))?;
            let mut output = std::io::stdout();
            output.write_all(&packet)?;
            output.flush()?;
            return Ok(());
        }
    };
    let (stop_tx, stop_rx) = std::sync::mpsc::sync_channel(1);
    thread::spawn(move || {
        let mut byte = [0];
        let _ = std::io::stdin().read(&mut byte);
        let _ = stop_tx.send(());
    });
    let (output, reader) =
        std::sync::mpsc::sync_channel::<(Vec<u8>, Option<std::sync::mpsc::SyncSender<bool>>)>(8);
    thread::spawn(move || {
        let mut stdout = std::io::stdout();
        while let Ok((row, ack)) = reader.recv() {
            let ok = stdout.write_all(&row).and_then(|_| stdout.flush()).is_ok();
            if let Some(ack) = ack {
                let _ = ack.send(ok);
            }
            if !ok {
                break;
            }
        }
    });
    let mut driver = ChildDriver {
        parent,
        expected_parent: actual,
        stop: stop_rx,
        output,
        clock: std::time::Instant::now(),
    };
    startup_audio_guardian::drive(&mut guardian, &mut driver);
    Ok(())
}

#[cfg(feature = "startup-audio-worker")]
struct ChildDriver {
    parent: Handle,
    expected_parent: Identity,
    stop: std::sync::mpsc::Receiver<()>,
    output: std::sync::mpsc::SyncSender<(Vec<u8>, Option<std::sync::mpsc::SyncSender<bool>>)>,
    clock: std::time::Instant,
}
#[cfg(feature = "startup-audio-worker")]
impl startup_audio_guardian::Driver for ChildDriver {
    fn now_ms(&self) -> u64 {
        self.clock.elapsed().as_millis().min(u64::MAX as u128) as u64
    }
    fn stop_requested(&mut self) -> bool {
        !matches!(
            self.stop.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ) || identity(self.parent.0).map_or(true, |actual| {
            !same_identity(&actual, &self.expected_parent)
        })
    }
    fn emit(&mut self, r: &startup_audio_guardian::Report) -> Result<()> {
        use startup_audio_guardian::Phase;
        let event = match r.phase {
            Phase::Watching => "volume",
            Phase::Restoring | Phase::RestorationPending => "audio-restoration-pending",
            Phase::Finished => "audio-restored",
        };
        let mut bytes = serde_json::to_vec(
            &serde_json::json!({"event":event,"found":r.volume.is_some(),"volume":r.volume.map(|v|v.level),"muted":r.volume.map(|v|v.muted),"pending":r.pending,"error":r.error}),
        )?;
        ensure!(bytes.len() < 16 * 1024, "Audio diagnostic exceeds bounds");
        bytes.push(b'\n');
        // No output backpressure can postpone restoration. The only ACK wait
        // is after all owned mutations are restored, preventing process exit
        // from killing its stdout writer before the final completion reaches UI.
        let finished = r.phase == Phase::Finished;
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        match self.output.try_send((bytes, finished.then_some(tx))) {
            Ok(()) => {}
            Err(std::sync::mpsc::TrySendError::Full(_)) => return Ok(()),
            Err(_) => anyhow::bail!("Audio UI observer closed"),
        }
        if finished {
            ensure!(
                rx.recv_timeout(std::time::Duration::from_secs(1))
                    .unwrap_or(false),
                "Audio restore ACK could not be flushed"
            );
        }
        Ok(())
    }
    fn pause(&mut self) {
        thread::sleep(std::time::Duration::from_millis(100));
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Notice {
    Watching(Option<Volume>),
    Pending,
    Restored,
    Failed(String),
    NotStarted(String),
}
fn not_started(nonce: &str, session: &str, error: &str) -> Result<Vec<u8>> {
    ensure!(
        hex(nonce) && hex(session),
        "Invalid no-mutation audio response identity"
    );
    let mut packet = serde_json::to_vec(
        &serde_json::json!({"event":"audio-start-failed","nonce":nonce,"session":session,"ownsMutation":false,"pending":0,"error":error.chars().take(4096).collect::<String>()}),
    )?;
    packet.push(b'\n');
    Ok(packet)
}
fn parse_owned(bytes: &[u8], nonce: &str, session: &str) -> Result<Notice> {
    let notice = parse(bytes)?;
    if matches!(notice, Notice::NotStarted(_)) {
        let v: Value = serde_json::from_slice(bytes)?;
        ensure!(
            v["nonce"] == nonce && v["session"] == session,
            "Foreign no-mutation audio response"
        );
    }
    Ok(notice)
}
fn accept_drive_scope(notice: Notice, drive_seen: &mut bool) -> Result<Notice> {
    if matches!(notice, Notice::NotStarted(_)) {
        ensure!(
            !*drive_seen,
            "No-mutation response arrived after the audio drive began"
        );
    } else if matches!(
        notice,
        Notice::Watching(_) | Notice::Pending | Notice::Restored
    ) {
        *drive_seen = true;
    }
    Ok(notice)
}
fn parse(bytes: &[u8]) -> Result<Notice> {
    ensure!(
        bytes.len() <= MAX_PACKET,
        "Audio guardian output exceeds bounds"
    );
    let v: Value = serde_json::from_slice(bytes)?;
    let pending = v["pending"].as_u64().context("Missing owned audio count")?;
    ensure!(pending <= 64, "Invalid owned audio count");
    if let Some(error) = v["error"].as_str() {
        ensure!(error.len() <= 16 * 1024, "Audio error exceeds bounds");
    }
    match v["event"].as_str() {
        Some("audio-start-failed") => {
            ensure!(
                pending == 0
                    && v["ownsMutation"] == false
                    && v["nonce"].as_str().is_some_and(hex)
                    && v["session"].as_str().is_some_and(hex),
                "Unverified no-mutation audio response"
            );
            Ok(Notice::NotStarted(
                v["error"]
                    .as_str()
                    .context("Missing pre-drive audio failure")?
                    .into(),
            ))
        }
        Some("audio-restored") => {
            ensure!(pending == 0, "Restored audio still owns mutations");
            Ok(Notice::Restored)
        }
        Some("audio-restoration-pending") => Ok(Notice::Pending),
        Some("volume") => {
            if let Some(error) = v["error"].as_str() {
                anyhow::bail!("Audio guardian observation failed: {error}");
            }
            let volume = if v["found"] == true {
                let level = v["volume"].as_f64().context("Invalid mixer level")? as f32;
                let muted = v["muted"].as_bool().context("Invalid mixer mute")?;
                let volume = Volume { level, muted };
                volume.validate()?;
                Some(volume)
            } else {
                ensure!(v["found"] == false, "Invalid mixer presence");
                None
            };
            Ok(Notice::Watching(volume))
        }
        _ => anyhow::bail!("Unexpected audio guardian event"),
    }
}
#[derive(Default)]
struct Mailbox {
    serial: u64,
    latest: Option<Notice>,
    terminal: Option<Notice>,
}
impl Mailbox {
    fn put(&mut self, notice: Notice) {
        self.serial = self.serial.saturating_add(1);
        if matches!(
            notice,
            Notice::Restored | Notice::Failed(_) | Notice::NotStarted(_)
        ) {
            self.terminal = Some(notice.clone());
        }
        self.latest = Some(notice);
    }
}
pub(crate) struct OwnedAudio {
    child: Child,
    _exe_pin: Arc<Binary>,
    mailbox: Arc<Mutex<Mailbox>>,
    seen: u64,
    restored: bool,
}
impl OwnedAudio {
    pub fn start(expected: &ExpectedGame, binary: Arc<Binary>, session: &str) -> Result<Self> {
        ExpectedGame::new(expected.key, &expected.exe)?;
        ensure!(hex(session), "Exact startup audio session required");
        let exe = binary.path();
        let parent = identity(unsafe { GetCurrentProcess() })?;
        let mut random = [0u8; 24];
        unsafe {
            BCryptGenRandom(None, &mut random, BCRYPT_USE_SYSTEM_PREFERRED_RNG).ok()?;
        }
        let nonce = random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let b = Bootstrap {
            schema: SCHEMA.into(),
            nonce: nonce.clone(),
            session: session.into(),
            helper_sha256: binary.sha256.clone(),
            parent,
            game: Identity {
                pid: expected.key.pid,
                birth: expected.key.birth,
                exe: expected.exe.clone(),
            },
        };
        let mut child = Command::new(exe)
            .args([FLAG, &nonce])
            .creation_flags(CREATE_NO_WINDOW.0 | CREATE_BREAKAWAY_FROM_JOB.0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let result = (|| -> Result<_> {
            let actual_child = identity(HANDLE(child.as_raw_handle()))?;
            ensure!(
                normalize(&actual_child.exe) == normalize(exe),
                "Audio guardian executable differs"
            );
            let mut in_job = windows::core::BOOL(0);
            unsafe {
                IsProcessInJob(HANDLE(child.as_raw_handle()), None, &mut in_job)?;
            }
            ensure!(
                !in_job.as_bool(),
                "Audio guardian inherited a kill-on-close job"
            );
            Ok(child.stdout.take().context("Missing audio output pipe")?)
        })();
        let stdout = match result {
            Ok(stdout) => stdout,
            Err(error) => {
                child.stdin.take();
                return Err(error);
            }
        };
        let mailbox = Arc::new(Mutex::new(Mailbox::default()));
        let output = mailbox.clone();
        let expected_nonce = nonce.clone();
        let expected_session = session.to_string();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut drive_seen = false;
            loop {
                let mut bytes = Vec::new();
                let result = reader
                    .by_ref()
                    .take(MAX_PACKET as u64 + 1)
                    .read_until(b'\n', &mut bytes);
                let notice = match result {
                    Ok(0) => break,
                    Ok(_) if bytes.len() <= MAX_PACKET && bytes.last() == Some(&b'\n') => {
                        parse_owned(&bytes, &expected_nonce, &expected_session)
                            .and_then(|notice| accept_drive_scope(notice, &mut drive_seen))
                            .unwrap_or_else(|e| Notice::Failed(format!("{e:#}")))
                    }
                    _ => Notice::Failed("Audio guardian output is truncated or unavailable".into()),
                };
                let terminal = matches!(
                    notice,
                    Notice::Restored | Notice::Failed(_) | Notice::NotStarted(_)
                );
                if let Ok(mut out) = output.lock() {
                    out.put(notice);
                }
                if terminal {
                    break;
                }
            }
            if let Ok(mut out) = output.lock() {
                if out.terminal.is_none() {
                    out.put(Notice::Failed(
                        "Audio guardian exited without restoration acknowledgement".into(),
                    ));
                }
            }
        });
        let mut owned = Self {
            child,
            _exe_pin: binary,
            mailbox,
            seen: 0,
            restored: false,
        };
        // Once a complete bootstrap MAY have reached the child, never return a
        // bare error that drops restoration ownership. Keep its exact handle,
        // EOF it, and observe the restoration ACK even after a pipe failure.
        let sent = (|| -> Result<()> {
            let bytes = serde_json::to_vec(&b)?;
            ensure!(bytes.len() <= MAX_PACKET, "Audio bootstrap exceeds bounds");
            let input = owned
                .child
                .stdin
                .as_mut()
                .context("Missing audio control pipe")?;
            input.write_all(&(bytes.len() as u32).to_le_bytes())?;
            input.write_all(&bytes)?;
            input.flush()?;
            Ok(())
        })();
        if let Err(error) = sent {
            owned.stop();
            if let Ok(mut out) = owned.mailbox.lock() {
                out.put(Notice::Failed(format!(
                    "Owned audio bootstrap publication uncertain: {error:#}"
                )));
            }
        }
        Ok(owned)
    }
    pub fn stop(&mut self) {
        self.child.stdin.take();
    }
    pub fn poll(&mut self) -> Result<Option<Notice>> {
        let out = self
            .mailbox
            .lock()
            .map_err(|_| anyhow::anyhow!("Audio observer poisoned"))?;
        if out.serial == self.seen {
            return Ok(None);
        }
        self.seen = out.serial;
        let notice = out.terminal.clone().or_else(|| out.latest.clone());
        if matches!(notice, Some(Notice::Restored)) {
            self.restored = true;
        }
        Ok(notice)
    }
}
impl Drop for OwnedAudio {
    fn drop(&mut self) {
        self.child.stdin.take(); // Never kill a potentially owned mute mutation.
        let _ = self.child.try_wait();
    }
}

#[cfg(test)]
#[path = "startup_video_audio_tests.rs"]
mod tests;
