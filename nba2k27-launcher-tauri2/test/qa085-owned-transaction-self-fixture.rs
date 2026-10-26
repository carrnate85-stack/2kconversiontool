//! Standalone --cfg test synthetic-only driver for the ACTUAL production module.
//! Compile directly with root's already-built dependencies; never a shipped bin.
#![allow(dead_code, unused_imports)]
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    os::windows::{fs::OpenOptionsExt, io::AsRawHandle, process::CommandExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
#[path = "../src/game_write_gate.rs"]
mod game_write_gate;
mod session_safety {
    use serde::{Deserialize, Serialize};
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    pub struct Identity {
        pub pid: u32,
        pub started: String,
        pub path: String,
    }
    pub(crate) use crate::game_write_gate::WriteGuard;
    pub fn write_gate() -> anyhow::Result<WriteGuard> {
        crate::game_write_gate::lock()
    }
}
#[path = "../src/owned_transaction.rs"]
mod owned_transaction;
use owned_transaction::{Context as Tx, Operation, ReadContext};
type H = isize;
#[repr(C)]
#[derive(Default)]
struct Ft {
    lo: u32,
    hi: u32,
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentThreadId() -> u32;
    fn OpenThread(access: u32, inherit: i32, id: u32) -> H;
    fn SuspendThread(h: H) -> u32;
    fn ResumeThread(h: H) -> u32;
    fn OpenProcess(a: u32, b: i32, pid: u32) -> H;
    fn CloseHandle(h: H) -> i32;
    fn GetProcessId(h: H) -> u32;
    fn GetProcessTimes(h: H, a: *mut Ft, b: *mut Ft, c: *mut Ft, d: *mut Ft) -> i32;
    fn QueryFullProcessImageNameW(h: H, flags: u32, b: *mut u16, n: *mut u32) -> i32;
    fn ReadProcessMemory(h: H, p: usize, b: *mut u8, n: usize, out: *mut usize) -> i32;
    fn WriteProcessMemory(h: H, p: usize, b: *const u8, n: usize, out: *mut usize) -> i32;
    fn TerminateProcess(h: H, status: u32) -> i32;
    fn WaitForSingleObject(h: H, ms: u32) -> u32;
}
struct Handle(H);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
fn identity(h: H) -> Result<session_safety::Identity> {
    let (mut a, mut b, mut c, mut d) = (Ft::default(), Ft::default(), Ft::default(), Ft::default());
    ensure!(
        unsafe { GetProcessTimes(h, &mut a, &mut b, &mut c, &mut d) } != 0,
        "birth"
    );
    let mut buf = vec![0u16; 32768];
    let mut n = buf.len() as u32;
    ensure!(
        unsafe { QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut n) } != 0,
        "path"
    );
    Ok(session_safety::Identity {
        pid: unsafe { GetProcessId(h) },
        started: (((a.hi as u64) << 32) | a.lo as u64).to_string(),
        path: String::from_utf16(&buf[..n as usize])?,
    })
}
fn open(expected: &session_safety::Identity) -> Result<Handle> {
    let h = Handle(unsafe { OpenProcess(0x101e39, 0, expected.pid) });
    ensure!(h.0 != 0 && identity(h.0)? == *expected, "exact self child");
    Ok(h)
}
fn read(h: H, at: usize) -> Result<[u64; 2]> {
    let mut b = [0u8; 16];
    let mut n = 0;
    ensure!(
        unsafe { ReadProcessMemory(h, at, b.as_mut_ptr(), 16, &mut n) } != 0 && n == 16,
        "counter read"
    );
    Ok([
        u64::from_le_bytes(b[..8].try_into()?),
        u64::from_le_bytes(b[8..].try_into()?),
    ])
}
fn write(h: H, at: usize, value: u64, n: usize) -> Result<()> {
    let mut done = 0;
    ensure!(
        unsafe { WriteProcessMemory(h, at, value.to_le_bytes().as_ptr(), n, &mut done) } != 0
            && done == n,
        "self counter write"
    );
    Ok(())
}
fn target() -> ! {
    let words = Box::leak(Box::new([
        AtomicU64::new(0),
        AtomicU64::new(0x1122334455667788),
    ]));
    println!(
        "{}",
        json!({"address":words.as_ptr()as usize,"thread":unsafe{GetCurrentThreadId()}})
    );
    std::io::stdout().flush().unwrap();
    loop {
        words[0].fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn execute_inside(ctx: &mut Tx, op: &Operation) -> Result<Value> {
    let spec = match op {
        Operation::Season(v) => v,
        _ => anyhow::bail!("fixture operation"),
    };
    let case = spec["case"].as_str().context("case")?;
    let at = spec["address"].as_u64().context("address")? as usize;
    let h = open(ctx.game())?;
    let dir = ctx.root().join("sealed");
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.bin", std::process::id()));
    let mut f = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .share_mode(1)
        .open(path)?;
    let raw = serde_json::to_vec(spec)?;
    f.write_all(&raw)?;
    f.sync_all()?;
    use sha2::{Digest, Sha256};
    let hash = format!("{:x}", Sha256::digest(&raw));
    ctx.retain_intent(&f, &hash)?;
    if case == "no_freeze_error" {
        anyhow::bail!("expected zero effect before freeze");
    }
    if case == "crash_before_freeze" {
        std::process::exit(77);
    }
    if case == "intent_tamper" {
        use std::io::{Seek, SeekFrom};
        f.seek(SeekFrom::Start(0))?;
        f.write_all(b"X")?;
        f.sync_all()?;
        ensure!(ctx.freeze().is_err(), "tampered intent must refuse freeze");
        anyhow::bail!("expected tampered immutable intent");
    }
    if case == "delayed_bootstrap_rollback" {
        std::thread::sleep(Duration::from_millis(3500));
    }
    ctx.freeze()?;
    let before = read(h.0, at)?;
    std::thread::sleep(Duration::from_millis(25));
    ensure!(read(h.0, at)? == before, "child did not freeze");
    if case == "no_effect_error" {
        anyhow::bail!("expected no-effect frozen error");
    }
    if case == "deadline_before_effect" {
        std::thread::sleep(Duration::from_millis(5100));
        ensure!(ctx.before_effect(false).is_err(), "deadline gate");
        anyhow::bail!("expected deadline no effect");
    }
    ctx.before_effect(false)?;
    write(
        h.0,
        at,
        0xaabbccdd12345678,
        if case == "partial_write" { 4 } else { 8 },
    )?;
    if case == "delayed_bootstrap_rollback" {
        std::thread::sleep(Duration::from_millis(7500));
        ctx.before_effect(true)?;
        write(h.0, at, before[0], 8)?;
        std::fs::write(
            ctx.root().join("delayed-before.json"),
            serde_json::to_vec(&before)?,
        )?;
        ensure!(read(h.0, at)? == before, "complete rollback");
        ctx.mark_consistent()?;
        anyhow::bail!("expected restored before after delayed bootstrap");
    }
    if case == "watchdog_timeout" {
        loop {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    if case == "parent_death" {
        let signal = ctx.root().join("effect-ready.json");
        let value = json!({"game":ctx.game(),"companions":ctx.trusted_companions()?,"address":at});
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(signal)?;
        serde_json::to_writer(&mut file, &value)?;
        file.sync_all()?;
        loop {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    if case == "crash_after_write" || case == "partial_write" {
        std::process::exit(78);
    }
    if case == "watchdog_death_rollback" || case == "watchdog_death_unknown" {
        let self_pid = std::process::id();
        let companions = ctx.trusted_companions()?;
        let other = companions
            .into_iter()
            .find(|p| p.pid != self_pid)
            .context("watchdog")?;
        let other = open(&other)?;
        ensure!(
            unsafe { TerminateProcess(other.0, 0xe084) } != 0,
            "kill exact owned watchdog"
        );
        ensure!(
            unsafe { WaitForSingleObject(other.0, 2000) } == 0,
            "watchdog exit"
        );
        ensure!(
            ctx.before_effect(false).is_err(),
            "forward after watchdog death"
        );
        if case == "watchdog_death_rollback" {
            ctx.before_effect(true)?;
            write(h.0, at, before[0], 8)?;
            ensure!(read(h.0, at)? == before, "rollback");
            ctx.mark_consistent()?;
        }
        anyhow::bail!("expected stopped watchdog");
    }
    ensure!(
        read(h.0, at)? == [0xaabbccdd12345678, before[1]],
        "full effect readback"
    );
    ctx.mark_consistent()?;
    Ok(json!({"case":case,"after":0xaabbccdd12345678u64}))
}
fn verify_inside(ctx: &mut ReadContext, op: &Operation, _receipt: &Value) -> Result<()> {
    let spec = match op {
        Operation::Season(v) => v,
        _ => anyhow::bail!("fixture operation"),
    };
    let raw = ctx.read_intent()?;
    ensure!(
        serde_json::from_slice::<Value>(&raw)? == *spec,
        "original retained intent differs"
    );
    let case = spec["case"].as_str().context("case")?;
    let path = ctx.root().join(format!("{}.verified", case));
    if !ctx.effect_attempted() {
        fs::write(path, b"no-effect")?;
        return Ok(());
    }
    ctx.checkpoint_frozen()?;
    let h = open(ctx.game())?;
    let at = spec["address"].as_u64().context("address")? as usize;
    let first = read(h.0, at)?;
    std::thread::sleep(Duration::from_millis(25));
    ensure!(
        read(h.0, at)? == first
            && first
                == if case == "delayed_bootstrap_rollback" {
                    serde_json::from_slice::<[u64; 2]>(&std::fs::read(
                        ctx.root().join("delayed-before.json"),
                    )?)?
                } else {
                    [0xaabbccdd12345678, 0x1122334455667788]
                },
        "unknown or partial graph"
    );
    ctx.checkpoint_frozen()?;
    fs::write(path, b"after")?;
    Ok(())
}
struct OwnedChild(std::process::Child);
impl std::ops::Deref for OwnedChild {
    type Target = std::process::Child;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for OwnedChild {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn doomed_launcher(root: &Path) -> Result<()> {
    fs::create_dir_all(root)?;
    let mut target = OwnedChild(
        Command::new(std::env::current_exe()?)
            .arg("--synthetic-target")
            .creation_flags(0x08000000)
            .stdout(Stdio::piped())
            .spawn()?,
    );
    let game = identity(target.as_raw_handle() as H)?;
    let parent = identity(-1)?;
    let mut line = String::new();
    BufReader::new(target.stdout.take().context("target output")?).read_line(&mut line)?;
    let v: Value = serde_json::from_str(&line)?;
    let _ = owned_transaction::execute(
        root,
        &game,
        &parent,
        &"0".repeat(48),
        Operation::Season(json!({"case":"parent_death","address":v["address"]})),
        true,
    );
    anyhow::bail!("outer fixture must kill this launcher")
}
fn parent_death_fixture(root: &Path) -> Result<Value> {
    let dir = root.join("parent-death");
    let mut launcher = OwnedChild(
        Command::new(std::env::current_exe()?)
            .arg("--synthetic-launcher")
            .arg(&dir)
            .creation_flags(0x08000000)
            .stderr(Stdio::null())
            .spawn()?,
    );
    let signal = dir.join("effect-ready.json");
    let end = Instant::now() + Duration::from_secs(5);
    while !signal.exists() {
        ensure!(Instant::now() < end, "parent-death setup timeout");
        ensure!(
            launcher.try_wait()?.is_none(),
            "synthetic launcher ended before effect"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let value: Value = serde_json::from_slice(&fs::read(signal)?)?;
    let game: session_safety::Identity = serde_json::from_value(value["game"].clone())?;
    ensure!(
        Path::new(&game.path) == std::env::current_exe()?.as_path(),
        "synthetic descendant image"
    );
    let handle = open(&game)?;
    let companions: Vec<session_safety::Identity> =
        serde_json::from_value(value["companions"].clone())?;
    let companions: Vec<Handle> = companions.iter().map(open).collect::<Result<_>>()?;
    launcher.kill()?;
    launcher.wait()?;
    for h in companions {
        ensure!(
            unsafe { WaitForSingleObject(h.0, 5000) } == 0,
            "orphan helper/watchdog after launcher death"
        );
    }
    ensure!(
        unsafe { WaitForSingleObject(handle.0, 0) } == 258,
        "independently verified complete graph should survive parent death"
    );
    let at = value["address"].as_u64().context("counter")? as usize;
    let a = read(handle.0, at)?;
    std::thread::sleep(Duration::from_millis(40));
    ensure!(
        read(handle.0, at)?[0] != a[0],
        "descendant did not resume after parent death"
    );
    ensure!(
        unsafe { TerminateProcess(handle.0, 0xe084) } != 0
            && unsafe { WaitForSingleObject(handle.0, 2000) } == 0,
        "close exact owned descendant"
    );
    Ok(
        json!({"case":"parent_death","passed":true,"gameClosed":false,"helperAndWatchdogExited":true}),
    )
}
fn run(root: &Path) -> Result<()> {
    ensure!(!root.exists(), "fresh self fixture output required");
    fs::create_dir_all(root)?;
    let parent = identity(-1)?;
    let exe = std::env::current_exe()?;
    let mut results = Vec::new();
    let source = root.join("archive-source.pending");
    let target = root.join("archive-source.ended");
    let mut journal = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .access_mode(0xc0010000)
        .share_mode(1)
        .open(&source)?;
    journal.write_all(b"exact immutable bytes")?;
    journal.sync_all()?;
    let existing = root.join("foreign.ended");
    fs::write(&existing, b"must remain")?;
    ensure!(
        owned_transaction::archive_intent(&journal, &existing).is_err(),
        "archive overwrote foreign destination"
    );
    owned_transaction::archive_intent(&journal, &target)?;
    ensure!(
        !source.exists()
            && fs::read(&target)? == b"exact immutable bytes"
            && fs::read(existing)? == b"must remain",
        "exact handle archive mismatch"
    );

    for case in [
        "normal",
        "delayed_bootstrap_rollback",
        "foreign_suspend",
        "intent_tamper",
        "watchdog_timeout",
        "no_freeze_error",
        "crash_before_freeze",
        "no_effect_error",
        "deadline_before_effect",
        "crash_after_write",
        "partial_write",
        "watchdog_death_rollback",
        "watchdog_death_unknown",
    ] {
        let mut child = OwnedChild(
            Command::new(&exe)
                .arg("--synthetic-target")
                .creation_flags(0x08000000)
                .stdout(Stdio::piped())
                .spawn()?,
        );
        let id = identity(child.as_raw_handle() as H)?;
        let mut stdout = BufReader::new(child.stdout.take().context("counter output")?);
        let mut line = String::new();
        stdout.read_line(&mut line)?;
        let v: Value = serde_json::from_str(&line)?;
        let address = v["address"].as_u64().context("counter address")?;
        let foreign = if case == "foreign_suspend" {
            let tid = v["thread"].as_u64().context("thread")?;
            let thread = Handle(unsafe { OpenThread(0x42, 0, tid as u32) });
            ensure!(
                thread.0 != 0 && unsafe { SuspendThread(thread.0) } == 0,
                "owned foreign count setup"
            );
            Some(thread)
        } else {
            None
        };
        let output = root.join(case);
        fs::create_dir(&output)?;
        let begin = Instant::now();
        let result = owned_transaction::execute(
            &output,
            &id,
            &parent,
            &"0".repeat(48),
            Operation::Season(json!({"case":case,"address":address})),
            true,
        );
        if let Some(thread) = foreign {
            ensure!(
                unsafe { ResumeThread(thread.0) } == 1,
                "service disturbed preexisting foreign suspend count"
            );
        }
        if output.join("sealed").is_dir() {
            use sha2::{Digest, Sha256};
            let item = fs::read_dir(output.join("sealed"))?
                .next()
                .context("sealed fixture intent")??;
            let raw = fs::read(item.path())?;
            let sha = format!("{:x}", Sha256::digest(&raw));
            let known = owned_transaction::verified_intent(&id, &"0".repeat(48), &sha);
            let expected = !matches!(
                case,
                "intent_tamper"
                    | "partial_write"
                    | "watchdog_death_rollback"
                    | "watchdog_death_unknown"
            );
            ensure!(
                known == expected,
                "{case}: private completion provenance mismatch known={known}"
            );
        }
        let h = open(&id).ok();
        let ended = unsafe { WaitForSingleObject(child.as_raw_handle() as H, 0) } == 0;
        let should_close = matches!(case, "partial_write" | "watchdog_death_unknown");
        ensure!(
            ended == should_close,
            "{case}: unexpected child lifecycle ended={ended} result={result:?}"
        );
        if let Some(h) = h {
            if !ended {
                let a = read(h.0, address as usize)?;
                std::thread::sleep(Duration::from_millis(40));
                ensure!(
                    read(h.0, address as usize)?[0] != a[0],
                    "{case}: synthetic child remained frozen"
                );
            }
        }
        if case == "normal" {
            ensure!(result.is_ok(), "normal: {result:?}");
        } else {
            ensure!(result.is_err(), "fault should not report normal success");
        }
        if !ended {
            child.kill()?;
        }
        child.wait()?;
        results.push(json!({"case":case,"passed":true,"milliseconds":begin.elapsed().as_millis(),"gameClosed":ended,"error":result.err().map(|e|e.to_string())}));
    }
    results.push(parent_death_fixture(root)?);
    let path = root.join("results.json");
    let mut f = OpenOptions::new().write(true).create_new(true).open(path)?;
    serde_json::to_writer_pretty(
        &mut f,
        &json!({"scope":"ACTUAL production module with compile-time test-only exact same-image direct synthetic child admission; no NBA handle/calls","results":results,"exactHandleArchivePassed":true,"archiveNoOverwritePassed":true}),
    )?;
    f.sync_all()?;
    Ok(())
}
fn main() {
    #[cfg(not(test))]
    {
        eprintln!("Compile this standalone fixture with --cfg test; it is never shipped");
        std::process::exit(2);
    }
    #[cfg(test)]
    {
        if let Some(code) = owned_transaction::maybe_run_helper(execute_inside, verify_inside) {
            std::process::exit(code);
        }
        if std::env::args().nth(1).as_deref() == Some("--synthetic-target") {
            target();
        }
        if std::env::args().nth(1).as_deref() == Some("--synthetic-launcher") {
            let path = std::env::args().nth(2).expect("synthetic root");
            if let Err(e) = doomed_launcher(Path::new(&path)) {
                eprintln!("{e:#}");
                std::process::exit(1);
            }
        }
        let root = std::env::args()
            .nth(1)
            .expect("fresh artifact output directory");
        if let Err(e) = run(Path::new(&root)) {
            eprintln!("{e:#}");
            std::process::exit(1);
        }
    }
}
