//! Private whitelist: only this executable's inert QA probe can run here.
//! No user-controlled executable/PID arguments and no game-memory helper yet.
use crate::{
    backend,
    session_safety::{self, HelperLease, Journal, Phase},
};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    os::windows::{io::AsRawHandle, process::CommandExt},
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::JobObjects::*,
    },
    core::PCWSTR,
};
struct Job(HANDLE);
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
fn job() -> Result<Job> {
    let job = Job(unsafe { CreateJobObjectW(None, PCWSTR::null())? });
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    unsafe {
        SetInformationJobObject(
            job.0,
            JobObjectExtendedLimitInformation,
            &limits as *const _ as _,
            std::mem::size_of_val(&limits) as u32,
        )?;
    }
    Ok(job)
}
pub(crate) fn hash(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    ensure!(
        file.metadata()?.is_file() && file.metadata()?.len() <= 64 * 1024 * 1024,
        "Unexpected helper size"
    );
    let mut sha = Sha256::new();
    let mut buf = [0u8; 65536];
    let mut read = 0;
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        read += n;
        ensure!(read <= 64 * 1024 * 1024, "Helper changed or exceeds limit");
        sha.update(&buf[..n]);
    }
    Ok(format!("{:x}", sha.finalize()))
}
struct OwnedProbe {
    child: Child,
    job: Option<Job>,
    lease: HelperLease,
}
impl OwnedProbe {
    fn spawn(root: &Path, session: &str) -> Result<Self> {
        let exe = std::env::current_exe()?.canonicalize()?;
        use std::os::windows::fs::OpenOptionsExt;
        // Deny write/delete sharing while the approved binary is hashed and launched.
        let _pin = fs::OpenOptions::new().read(true).share_mode(1).open(&exe)?;
        let expected = hash(&exe)?;
        let job = job()?;
        let mut child = Command::new(&exe)
            .args(["--smoke", "--smoke-root"])
            .arg(root)
            .args(["--native-helper-probe", session])
            .creation_flags(0x08000000)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        // Never assign Steam or NBA 2K27 to this job. This child is the private inert probe.
        let assigned = unsafe { AssignProcessToJobObject(job.0, HANDLE(child.as_raw_handle())) };
        if let Err(e) = assigned {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e.into());
        }
        let identity = session_safety::identity(child.id())?;
        ensure!(
            crate::native_session::same_path(&identity.path, &exe) && hash(&exe)? == expected,
            "Helper executable changed"
        );
        let lease = HelperLease {
            session: session.into(),
            role: "provider".into(),
            identity,
            binary_sha256: expected,
        };
        Ok(Self {
            child,
            job: Some(job),
            lease,
        })
    }
    fn wait_exit(&mut self, timeout: Duration) -> Result<bool> {
        let start = Instant::now();
        loop {
            if self.child.try_wait()?.is_some() {
                return Ok(true);
            }
            if start.elapsed() >= timeout {
                return Ok(false);
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
    fn stop(&mut self, journal: &Journal) -> Result<()> {
        if self.child.try_wait()?.is_some() {
            return Ok(());
        }
        let current = session_safety::identity(self.child.id())?;
        let digest = hash(Path::new(&current.path))?;
        ensure!(
            session_safety::owns_helper(journal, &self.lease, &current, &digest),
            "Helper ownership did not match; stop refused"
        );
        self.child.stdin.take(); // EOF asks the trusted probe to exit gracefully.
        if !self.wait_exit(Duration::from_millis(500))? {
            self.job.take(); // Only retained, launcher-created job handles are closed.
            ensure!(
                self.wait_exit(Duration::from_secs(1))?,
                "Owned helper exit could not be confirmed"
            );
        }
        Ok(())
    }
}
impl Drop for OwnedProbe {
    fn drop(&mut self) {
        self.child.stdin.take();
        self.job.take();
        let _ = self.wait_exit(Duration::from_secs(1));
    }
}
pub fn probe(root: &Path, nonce: &str) -> Result<()> {
    ensure!(
        nonce.len() == 48 && nonce.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid QA probe nonce"
    );
    fs::create_dir_all(root.join("Logs"))?;
    backend::atomic_json(
        &root.join("Logs/probe-ready.json"),
        &json!({"session":nonce,"pid":std::process::id(),"gameWrites":false}),
    )?;
    let mut input = std::io::stdin().lock();
    let mut buf = [0u8; 256];
    while input.read(&mut buf)? > 0 {}
    Ok(())
}
pub fn smoke(root: &Path) -> Result<Value> {
    let session = backend::unique_id();
    let probe_root = root.join("Cache/HelperQA").join(&session);
    let mut owned = OwnedProbe::spawn(&probe_root, &session)?;
    let ready = probe_root.join("Logs/probe-ready.json");
    let start = Instant::now();
    while !ready.try_exists()? {
        ensure!(
            start.elapsed() < Duration::from_secs(5),
            "Probe handshake timeout"
        );
        ensure!(
            owned.child.try_wait()?.is_none(),
            "Probe exited before handshake"
        );
        thread::sleep(Duration::from_millis(10));
    }
    let handshake: Value = serde_json::from_slice(&fs::read(&ready)?)?;
    ensure!(
        handshake["session"] == session && handshake["pid"] == owned.child.id(),
        "Unexpected probe handshake"
    );
    let mut journal = Journal {
        version: 1,
        session: session.clone(),
        phase: Phase::Running,
        game: None,
        helpers: vec![owned.lease.clone()],
        files: vec![],
        launch: None,
    };
    let pid = owned.child.id();
    journal.session = "b".repeat(48);
    ensure!(
        owned.stop(&journal).is_err(),
        "Foreign session was allowed to stop a helper"
    );
    ensure!(
        owned.child.try_wait()?.is_none(),
        "Rejected stop affected helper"
    );
    journal.session = session;
    owned.stop(&journal)?;
    ensure!(
        owned.child.try_wait()?.is_some(),
        "Helper exit not confirmed"
    );
    let session = backend::unique_id();
    let probe_root = root.join("Cache/HelperQA").join(&session);
    let mut killed = OwnedProbe::spawn(&probe_root, &session)?;
    let forced_pid = killed.child.id();
    killed.job.take();
    ensure!(
        killed.wait_exit(Duration::from_secs(1))?,
        "Job close failed to stop owned probe"
    );
    Ok(
        json!({"pass":true,"whitelist":"self-executable inert probe only","gracefulPid":pid,"jobClosePid":forced_pid,"foreignStopRefused":true,"remainingOwnedHelpers":0,"gameWrites":false}),
    )
}
