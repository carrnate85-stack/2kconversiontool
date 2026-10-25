//! Explicit opt-in, game-local oracle. Never invoked by native exports/previews.
use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    os::windows::{fs::MetadataExt, io::AsRawHandle, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
        System::{
            JobObjects::*,
            Threading::{CreateSemaphoreW, ReleaseSemaphore, WaitForSingleObject},
        },
    },
    core::PCWSTR,
};
pub const MAX_OUTPUT: u64 = 2 * 1024 * 1024 * 1024;

fn present(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}
// Includes broken links; Path::exists() alone would silently miss those.
pub fn reject_links(root: &Path, target: &Path) -> Result<()> {
    let relative = target
        .strip_prefix(root)
        .context("Path outside owned root")?;
    let mut path = root.to_path_buf();
    for part in std::iter::once(None).chain(relative.components().map(Some)) {
        if let Some(part) = part {
            ensure!(
                matches!(part, std::path::Component::Normal(_)),
                "Unsafe output path"
            );
            path.push(part.as_os_str());
        }
        match fs::symlink_metadata(&path) {
            Ok(meta) => ensure!(
                meta.file_attributes() & 0x400 == 0,
                "Reparse point protected: {}",
                path.display()
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

struct Gate(HANDLE);
impl Drop for Gate {
    fn drop(&mut self) {
        unsafe {
            let _ = ReleaseSemaphore(self.0, 1, None);
            let _ = CloseHandle(self.0);
        }
    }
}
fn gate_name(root: &Path) -> String {
    // Same name as stable WPF, so both apps serialize their mod.exe jobs.
    let normalized = root
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .trim_end_matches(['\\', '/'])
        .to_uppercase();
    format!(
        "Local\\NBA2K27ExporterMod-{:X}",
        Sha256::digest(normalized.as_bytes())
    )
}
fn acquire(root: &Path, cancel: &AtomicBool) -> Result<Gate> {
    let name: Vec<u16> = gate_name(root).encode_utf16().chain(Some(0)).collect();
    let handle = unsafe { CreateSemaphoreW(None, 1, 1, PCWSTR(name.as_ptr()))? };
    let result = (|| {
        let started = Instant::now();
        loop {
            crate::codec::cancelled(cancel)?;
            ensure!(
                started.elapsed() < Duration::from_secs(120),
                "Another exporter holds the mod.exe gate; try again when it finishes"
            );
            match unsafe { WaitForSingleObject(handle, 100) } {
                WAIT_OBJECT_0 => return Ok(()),
                WAIT_TIMEOUT => {}
                _ => bail!("Could not acquire the game-local mod.exe gate"),
            }
        }
    })();
    if let Err(error) = result {
        unsafe {
            let _ = CloseHandle(handle);
        }
        return Err(error);
    }
    Ok(Gate(handle))
}

/// Only this newly created private directory is eligible for recursive cleanup.
pub struct Work {
    pub root: PathBuf,
}
impl Work {
    pub fn new() -> Result<Self> {
        Self::new_in(
            &std::env::temp_dir().canonicalize()?,
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
        )
    }
    fn new_in(parent: &Path, stamp: u128) -> Result<Self> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let parent = parent.canonicalize()?;
        for _ in 0..32 {
            let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let root = parent.join(format!(
                "nba2k27-exporter-oracle-{}-{stamp}-{sequence}",
                std::process::id()
            ));
            match fs::create_dir(&root) {
                Ok(()) => return Ok(Self { root }),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.into()),
            }
        }
        bail!("Could not allocate a unique private oracle directory")
    }
    pub fn cleanup(&self) -> Result<()> {
        fn clear(root: &Path, path: &Path) -> Result<()> {
            reject_links(root, path)?;
            for entry in fs::read_dir(path)? {
                let path = entry?.path();
                reject_links(root, &path)?;
                if fs::symlink_metadata(&path)?.is_dir() {
                    clear(root, &path)?;
                } else {
                    fs::remove_file(&path)?;
                }
            }
            fs::remove_dir(path)?;
            Ok(())
        }
        if present(&self.root)? {
            clear(&self.root, &self.root)?;
        }
        Ok(())
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

struct Process {
    child: Child,
    job: HANDLE,
}
impl Process {
    fn launch(exe: &Path, root: &Path, name: &str) -> Result<Self> {
        let mut command = Command::new(exe);
        command.arg(name).current_dir(root);
        Self::launch_command(command)
    }
    fn launch_command(mut command: Command) -> Result<Self> {
        let job = unsafe { CreateJobObjectW(None, None)? };
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as _,
                std::mem::size_of_val(&limits) as u32,
            )
        };
        if let Err(e) = configured {
            unsafe {
                let _ = CloseHandle(job);
            }
            return Err(e.into());
        }
        // No pipe-reader threads that could outlive a cancelled descendant.
        // Exit status and bounded file validation are the authoritative result.
        let child = command
            .creation_flags(0x08000000)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        let mut child = match child {
            Ok(child) => child,
            Err(e) => {
                unsafe {
                    let _ = CloseHandle(job);
                }
                return Err(e.into());
            }
        };
        if let Err(e) = unsafe { AssignProcessToJobObject(job, HANDLE(child.as_raw_handle())) } {
            let _ = child.kill();
            let _ = child.wait();
            unsafe {
                let _ = CloseHandle(job);
            }
            return Err(e.into());
        }
        Ok(Self { child, job })
    }
    fn wait(&mut self, cancel: &AtomicBool, timeout: Duration) -> Result<()> {
        let start = Instant::now();
        loop {
            crate::codec::cancelled(cancel)?;
            ensure!(
                start.elapsed() < timeout,
                "mod.exe exceeded its two-minute timeout"
            );
            if let Some(status) = self.child.try_wait()? {
                ensure!(
                    status.success(),
                    "mod.exe failed (exit code {:?}); native exports are unaffected",
                    status.code()
                );
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(40));
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        // Kill the entire owned process tree before touching its output.
        unsafe {
            let _ = TerminateJobObject(self.job, 1);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        // Job termination is asynchronous for descendants. Wait until the job
        // is empty before game-output cleanup can run.
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(2) {
            let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
            let query = unsafe {
                QueryInformationJobObject(
                    Some(self.job),
                    JobObjectBasicAccountingInformation,
                    &mut info as *mut _ as _,
                    std::mem::size_of_val(&info) as u32,
                    None,
                )
            };
            if query.is_err() || info.ActiveProcesses == 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        unsafe {
            let _ = CloseHandle(self.job);
        }
    }
}

pub struct Oracle {
    root: PathBuf,
    output: PathBuf,
    _gate: Gate,
    created: Vec<PathBuf>,
    started: bool,
}
impl Oracle {
    pub fn prepare(root: &Path, name: &str, cancel: &AtomicBool) -> Result<Self> {
        ensure!(
            name.to_lowercase().ends_with(".iff"),
            "Game-local mod.exe currently supports IFF resources only"
        );
        let root = root.canonicalize()?;
        let output = root.join("mods").join(crate::catalog::safe_relative(name)?);
        let exe = root.join("mod.exe");
        reject_links(&root, &exe)?;
        ensure!(
            fs::metadata(exe).is_ok_and(|m| m.is_file()),
            "The optional game-local mod.exe is unavailable"
        );
        let gate = acquire(&root, cancel)?;
        reject_links(&root, &output)?;
        ensure!(
            !present(&output)?,
            "Protected existing mod: {name}. It will not be overwritten or removed"
        );
        Ok(Self {
            root,
            output,
            _gate: gate,
            created: Vec::new(),
            started: false,
        })
    }
    fn create_parents(&mut self) -> Result<()> {
        let mut missing = Vec::new();
        let mut parent = self.output.parent().context("Output parent missing")?;
        while !present(parent)? {
            missing.push(parent.to_path_buf());
            parent = parent.parent().context("Missing game root")?;
        }
        for path in missing.into_iter().rev() {
            reject_links(&self.root, &path)?;
            match fs::create_dir(&path) {
                Ok(()) => self.created.push(path),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => reject_links(&self.root, &path)?,
                Err(e) => return Err(e).context("mod.exe needs write access to the game's mods folder; changing the export destination alone does not fix game-folder permissions"),
            }
        }
        Ok(())
    }
    pub fn run(&mut self, name: &str, work: &Path, cancel: &AtomicBool) -> Result<PathBuf> {
        self.create_parents()?;
        reject_links(&self.root, &self.output)?;
        ensure!(
            !present(&self.output)?,
            "Protected existing game-side output"
        );
        crate::codec::cancelled(cancel)?;
        // Mark ownership only after successful launch. Process guard always drops
        // before game-output cleanup (including cancellation and failed copies).
        let mut process = Process::launch(&self.root.join("mod.exe"), &self.root, name)?;
        self.started = true;
        process.wait(cancel, Duration::from_secs(120))?;
        drop(process);
        reject_links(&self.root, &self.output)?;
        let metadata = fs::symlink_metadata(&self.output)
            .context("mod.exe did not produce the requested IFF")?;
        ensure!(
            metadata.is_file() && metadata.len() > 0 && metadata.len() <= MAX_OUTPUT,
            "Invalid or oversized mod.exe output"
        );
        let target = work.join("oracle.iff");
        let mut input = fs::File::open(&self.output)?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)?;
        let mut buffer = vec![0; 128 * 1024];
        let mut total = 0u64;
        loop {
            crate::codec::cancelled(cancel)?;
            let count = input.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            total += count as u64;
            ensure!(total <= MAX_OUTPUT, "mod.exe output exceeds budget");
            output.write_all(&buffer[..count])?;
        }
        ensure!(
            total == metadata.len(),
            "mod.exe output changed during copy"
        );
        output.sync_all()?;
        Ok(target)
    }
    pub fn cleanup(&mut self) -> Result<()> {
        if self.started {
            reject_links(&self.root, &self.output)?;
            if present(&self.output)? {
                ensure!(
                    fs::symlink_metadata(&self.output)?.is_file(),
                    "Unexpected game-side directory retained for safety"
                );
                fs::remove_file(&self.output)
                    .context("Could not clean this operation's temporary game-side output")?;
            }
            self.started = false;
        }
        for path in self.created.iter().rev() {
            reject_links(&self.root, path)?;
            // Remove only directories we created, and only if still empty.
            if present(path)? && fs::read_dir(path)?.next().is_none() {
                fs::remove_dir(path)?;
            }
        }
        self.created.clear();
        Ok(())
    }
}
impl Drop for Oracle {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gate_matches_wpf_path_identity() {
        assert_eq!(
            gate_name(Path::new(r"C:\Games\NBA 2K27")),
            gate_name(Path::new(r"\\?\C:\Games\NBA 2K27\"))
        );
    }
    #[test]
    fn temporary_session_cleans_only_itself() {
        let work = Work::new().unwrap();
        fs::create_dir(work.root.join("nested")).unwrap();
        fs::write(work.root.join("nested/asset"), b"temporary").unwrap();
        let path = work.root.clone();
        work.cleanup().unwrap();
        assert!(!path.exists());
    }
    #[test]
    fn protected_existing_file_and_directory() {
        for directory in [false, true] {
            let work = Work::new().unwrap();
            fs::write(work.root.join("mod.exe"), b"not executed").unwrap();
            fs::create_dir_all(work.root.join("mods/balls")).unwrap();
            let target = work.root.join("mods/balls/test.iff");
            if directory {
                fs::create_dir(&target).unwrap();
            } else {
                fs::write(&target, b"user mod").unwrap();
            }
            assert!(
                Oracle::prepare(&work.root, "balls/test.iff", &AtomicBool::new(false)).is_err()
            );
            assert!(target.exists());
            if !directory {
                assert_eq!(fs::read(target).unwrap(), b"user mod");
            }
        }
    }
    #[test]
    fn traversal_and_cancel_refuse_launch() {
        let work = Work::new().unwrap();
        fs::write(work.root.join("mod.exe"), b"not executed").unwrap();
        assert!(Oracle::prepare(&work.root, "../escape.iff", &AtomicBool::new(false)).is_err());
        assert!(Oracle::prepare(&work.root, "test.iff", &AtomicBool::new(true)).is_err());
        assert!(!work.root.join("mods").exists());
    }
    #[test]
    fn partial_output_cleanup_preserves_neighbors() {
        let game = Work::new().unwrap();
        fs::write(game.root.join("mod.exe"), b"not executed").unwrap();
        let mut oracle =
            Oracle::prepare(&game.root, "balls/test.iff", &AtomicBool::new(false)).unwrap();
        oracle.create_parents().unwrap();
        fs::write(&oracle.output, b"partial").unwrap();
        oracle.started = true;
        let neighbor = oracle.output.parent().unwrap().join("user.iff");
        fs::write(&neighbor, b"existing user asset").unwrap();
        oracle.cleanup().unwrap();
        assert!(!oracle.output.exists());
        assert_eq!(fs::read(neighbor).unwrap(), b"existing user asset");
        assert!(game.root.join("mods/balls").exists());
    }
    #[test]
    fn same_clock_parallel_workspaces_are_distinct_and_preserve_neighbors() {
        let parent = Work::new().unwrap();
        let neighbor = parent.root.join("unowned.txt");
        fs::write(&neighbor, b"leave intact").unwrap();
        let workspaces = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..32)
                .map(|_| {
                    let root = &parent.root;
                    scope.spawn(move || Work::new_in(root, 123456).unwrap())
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().unwrap())
                .collect::<Vec<_>>()
        });
        let paths: std::collections::HashSet<_> =
            workspaces.iter().map(|w| w.root.clone()).collect();
        assert_eq!(paths.len(), 32);
        assert!(
            paths
                .iter()
                .all(|p| p.parent() == Some(parent.root.as_path()))
        );
        drop(workspaces);
        assert_eq!(fs::read(neighbor).unwrap(), b"leave intact");
        assert!(paths.iter().all(|p| !p.exists()));
    }
    #[test]
    fn failed_launch_cleans_only_created_empty_directories() {
        let game = Work::new().unwrap();
        fs::write(game.root.join("mod.exe"), b"invalid exe").unwrap();
        let work = Work::new().unwrap();
        let mut oracle =
            Oracle::prepare(&game.root, "balls/test.iff", &AtomicBool::new(false)).unwrap();
        assert!(
            oracle
                .run("balls/test.iff", &work.root, &AtomicBool::new(false))
                .is_err()
        );
        oracle.cleanup().unwrap();
        assert!(!game.root.join("mods").exists());
        assert!(game.root.join("mod.exe").exists());
    }
    #[test]
    fn junction_targets_are_never_followed_or_cleaned() {
        let game = Work::new().unwrap();
        let neighbor = Work::new().unwrap();
        fs::write(game.root.join("mod.exe"), b"not executed").unwrap();
        fs::write(neighbor.root.join("user.iff"), b"protected neighbor").unwrap();
        let junction = game.root.join("mods");
        let status = Command::new("cmd.exe")
            .args(["/D", "/C", "mklink", "/J"])
            .arg(&junction)
            .arg(&neighbor.root)
            .creation_flags(0x08000000)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(
            status.success(),
            "Could not construct operation-owned junction fixture"
        );
        assert!(Oracle::prepare(&game.root, "balls/test.iff", &AtomicBool::new(false)).is_err());
        assert!(
            game.cleanup().is_err(),
            "Recursive private-session cleanup followed a junction"
        );
        assert_eq!(
            fs::read(neighbor.root.join("user.iff")).unwrap(),
            b"protected neighbor"
        );
        // Non-recursive RemoveDirectory removes only this test-created junction.
        fs::remove_dir(&junction).unwrap();
        game.cleanup().unwrap();
        assert!(neighbor.root.join("user.iff").exists());
    }
    #[test]
    #[ignore]
    fn process_fixture() {
        // Spawned only by the test below, hidden and with an operation-owned
        // marker directory. No game installation or UI command can reach it.
        let root = PathBuf::from(std::env::var("NBA2K_ORACLE_TEST_ROOT").unwrap());
        assert!(
            root.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("nba2k27-exporter-oracle-")
        );
        if std::env::var_os("NBA2K_ORACLE_TEST_CHILD").is_some() {
            fs::write(root.join("child-pid"), std::process::id().to_string()).unwrap();
        } else {
            let mut child = Command::new(std::env::current_exe().unwrap());
            child
                .args(["--ignored", "--exact", "mod_runner::tests::process_fixture"])
                .env("NBA2K_ORACLE_TEST_CHILD", "1")
                .creation_flags(0x08000000)
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let _child = child.spawn().unwrap();
            let start = Instant::now();
            while !root.join("child-pid").exists() && start.elapsed() < Duration::from_secs(10) {
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(root.join("child-pid").exists());
            fs::write(root.join("ready"), b"started").unwrap();
        }
        std::thread::sleep(Duration::from_secs(30));
        fs::write(root.join("late"), b"must not survive").unwrap();
    }
    #[test]
    fn process_timeout_and_cancel_kill_before_cleanup() {
        for cancel_now in [false, true] {
            let work = Work::new().unwrap();
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args(["--ignored", "--exact", "mod_runner::tests::process_fixture"])
                .env("NBA2K_ORACLE_TEST_ROOT", &work.root);
            let mut process = Process::launch_command(command).unwrap();
            let start = Instant::now();
            while !work.root.join("ready").exists() && start.elapsed() < Duration::from_secs(10) {
                std::thread::sleep(Duration::from_millis(20));
            }
            assert!(work.root.join("ready").exists());
            let cancel = AtomicBool::new(cancel_now);
            assert!(process.wait(&cancel, Duration::from_millis(80)).is_err());
            let child_pid = process.child.id();
            let descendant_pid: u32 = fs::read_to_string(work.root.join("child-pid"))
                .unwrap()
                .parse()
                .unwrap();
            drop(process);
            assert!(!work.root.join("late").exists());
            use windows::Win32::System::Threading::{
                GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
            };
            for pid in [child_pid, descendant_pid] {
                if let Ok(handle) =
                    unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
                {
                    let mut code = 0;
                    unsafe {
                        GetExitCodeProcess(handle, &mut code).unwrap();
                        let _ = CloseHandle(handle);
                    }
                    assert_ne!(code, 259, "Owned mod process survived cancellation/timeout");
                }
            }
            work.cleanup().unwrap();
        }
    }
}
