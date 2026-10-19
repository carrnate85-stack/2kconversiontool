//! NEW standalone large-intent cost fixture; actual production module, synthetic owned children only.
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
    let reverse = spec["case"] == "reverse52";
    let at = spec["address"].as_u64().context("self address")? as usize;
    let h = open(ctx.game())?;
    let dir = ctx.root().join("sealed");
    fs::create_dir(&dir)?;
    let path = dir.join("large-intent.bin");
    let mut f = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .share_mode(1)
        .open(path)?;
    let header = serde_json::to_vec(spec)?;
    let mut raw = Vec::with_capacity(16 * 1024 * 1024);
    raw.extend((header.len() as u32).to_le_bytes());
    raw.extend(header);
    raw.resize(16 * 1024 * 1024, 0xa7);
    f.write_all(&raw)?;
    f.sync_all()?;
    use sha2::{Digest, Sha256};
    let hash = format!("{:x}", Sha256::digest(&raw));
    let prepare = Instant::now();
    ctx.retain_intent(&f, &hash)?;
    let retain_ms = prepare.elapsed().as_secs_f64() * 1000.;
    ctx.freeze()?;
    let before = read(h.0, at)?;
    ensure!(before[1] == 0x1122334455667788, "initial synthetic word");
    let started = Instant::now();
    let mut times = Vec::new();
    let mut completed = 0usize;
    let mut last = before[1];
    let mut failure = None;
    for i in 0..52 {
        let begin = Instant::now();
        if let Err(e) = ctx.before_effect(reverse) {
            failure = Some(format!("{e:#}"));
            break;
        }
        times.push(begin.elapsed().as_secs_f64() * 1000.);
        last = if i % 2 == 0 {
            0xaabbccdd12345678
        } else {
            0x1122334455667788
        };
        write(h.0, at + 8, last, 8)?;
        ensure!(
            read(h.0, at)? == [before[0], last],
            "synthetic effect readback"
        );
        completed += 1;
    }
    let frozen_ms = started.elapsed().as_secs_f64() * 1000.;
    ensure!(
        read(h.0, at)? == [before[0], last],
        "whole self graph final"
    );
    let consistent = ctx.mark_consistent().map_err(|e| format!("{e:#}"));
    let record = json!({"case":spec["case"],"intentBytes":raw.len(),"retainedIntentMilliseconds":retain_ms,"completedChecks":completed,"requestedChecks":52,"hashMilliseconds":times,"frozenLoopMilliseconds":frozen_ms,"failure":failure,"consistent":consistent.is_ok(),"consistentError":consistent.err(),"forwardBudgetMs":5000,"reverseBudgetMs":8000});
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(ctx.root().join("timings.json"))?;
    serde_json::to_writer_pretty(&mut output, &record)?;
    output.sync_all()?;
    ensure!(
        failure.is_none() && completed == 52,
        "52-check budget exhausted: {failure:?}"
    );
    Ok(record)
}
fn verify_inside(ctx: &mut ReadContext, op: &Operation, _receipt: &Value) -> Result<()> {
    let spec = match op {
        Operation::Season(v) => v,
        _ => anyhow::bail!("fixture operation"),
    };
    let raw = ctx.read_intent()?;
    ensure!(raw.len() == 16 * 1024 * 1024, "retained large intent size");
    let n = u32::from_le_bytes(raw[..4].try_into()?) as usize;
    ensure!(
        n < 65536
            && serde_json::from_slice::<Value>(&raw[4..4 + n])? == *spec
            && raw[4 + n..].iter().all(|&b| b == 0xa7),
        "original retained large intent differs"
    );
    if !ctx.effect_attempted() {
        return Ok(());
    }
    ctx.checkpoint_frozen()?;
    let h = open(ctx.game())?;
    let at = spec["address"].as_u64().context("self address")? as usize;
    let first = read(h.0, at)?;
    ensure!(
        [0x1122334455667788, 0xaabbccdd12345678].contains(&first[1]),
        "unknown self state"
    );
    std::thread::sleep(Duration::from_millis(15));
    ensure!(read(h.0, at)? == first, "self graph not frozen");
    ctx.checkpoint_frozen()
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
fn run(root: &Path) -> Result<()> {
    ensure!(
        !root.exists(),
        "fresh standalone artifact directory required"
    );
    fs::create_dir_all(root)?;
    owned_transaction::available()?;
    let parent = identity(-1)?;
    let exe = std::env::current_exe()?;
    let mut rows = Vec::new();
    for case in ["forward52", "reverse52"] {
        let mut target = OwnedChild(
            Command::new(&exe)
                .arg("--synthetic-target")
                .creation_flags(0x08000000)
                .stdout(Stdio::piped())
                .spawn()?,
        );
        let game = identity(target.as_raw_handle() as H)?;
        let mut line = String::new();
        BufReader::new(target.stdout.take().context("target pipe")?).read_line(&mut line)?;
        let address: Value = serde_json::from_str(&line)?;
        let out = root.join(case);
        fs::create_dir(&out)?;
        let begin = Instant::now();
        let result = owned_transaction::execute(
            &out,
            &game,
            &parent,
            &"0".repeat(48),
            Operation::Season(json!({"case":case,"address":address["address"]})),
            true,
        );
        let ended = target.try_wait()?.is_some();
        if !ended {
            let h = open(&game)?;
            let at = address["address"].as_u64().context("self address")? as usize;
            let first = read(h.0, at)?;
            std::thread::sleep(Duration::from_millis(30));
            ensure!(
                read(h.0, at)?[0] != first[0],
                "synthetic child remains frozen"
            );
            target.kill()?;
            target.wait()?;
        }
        let metrics = fs::read(out.join("timings.json"))
            .ok()
            .and_then(|v| serde_json::from_slice::<Value>(&v).ok());
        rows.push(json!({"case":case,"success":result.is_ok(),"totalMilliseconds":begin.elapsed().as_secs_f64()*1000.,"error":result.err().map(|e|format!("{e:#}")),"syntheticChildEmergencyClosed":ended,"metrics":metrics,"closedReliably":true}));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join("results.json"))?;
    serde_json::to_writer_pretty(
        &mut file,
        &json!({"scope":"ACTUAL production helper/watchdog/retainedimmutable16MiBintent; --cfgtest exactsameimage directsyntheticchild only; no game access","qualifications":"52wholeintent proofs+actual8Bselfwrites/readbacks, no structuraldirectory/profile costs included; no deadline changes","results":rows}),
    )?;
    file.sync_all()?;
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
        let root = std::env::args()
            .nth(1)
            .expect("fresh artifact output directory");
        if let Err(e) = run(Path::new(&root)) {
            eprintln!("{e:#}");
            std::process::exit(1);
        }
    }
}
