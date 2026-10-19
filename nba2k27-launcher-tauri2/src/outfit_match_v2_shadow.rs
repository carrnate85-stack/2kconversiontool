//! DEV-only, opt-in facts-only outfit timing observer. Never supplies admission.
//! Its new process handle has no write, allocation, thread or control rights.
use super::*;
use crate::{
    outfit_runtime::Owner,
    session_safety::{self, Identity},
};
use serde::Serialize;
use std::{
    cell::Cell,
    fs::{File, OpenOptions},
    io::Write,
    os::windows::fs::OpenOptionsExt,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, FILETIME, HANDLE, WAIT_TIMEOUT},
        System::{Diagnostics::Debug::ReadProcessMemory, Threading::*},
    },
    core::PWSTR,
};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetTickCount64() -> u64;
}

const RIGHTS: PROCESS_ACCESS_RIGHTS =
    PROCESS_ACCESS_RIGHTS(PROCESS_QUERY_INFORMATION.0 | PROCESS_VM_READ.0 | PROCESS_SYNCHRONIZE.0);
const PROBE_BYTES: usize = 512 * 1024;
// Existing V2 observer admission reads 4096-byte PE header, eight RIP anchors,
// four field anchors and three 16-byte class spans: conservatively reserve 8 KiB.
const ADMISSION_BYTES: usize = 8192;
const WINDOW_BYTES: usize = 8 * 1024 * 1024;
const WINDOW_MS: u64 = 15_000;
const MAX_EVENTS: usize = 32;
const BURST_US: u64 = 25_000;
// Private IDs and roster pointers are never serialized or exposed to a writer.
#[derive(Clone, Default, PartialEq, Eq)]
struct Frozen(Vec<(u64, String)>);
#[derive(Default)]
struct Input {
    frozen: Frozen,
    revision: u64,
}
pub(crate) struct Monitor {
    input: Arc<Mutex<Input>>,
    cancel: Arc<AtomicBool>,
}
impl Monitor {
    /// Failure is diagnostic-only and cannot change real feature state.
    pub(crate) fn start(owner: Owner, stop: Arc<AtomicBool>) -> Option<Self> {
        if !cfg!(debug_assertions)
            || std::env::var("C27_OUTFIT_READY_SHADOW").ok().as_deref() != Some("1")
        {
            return None;
        }
        let input = Arc::new(Mutex::new(Input::default()));
        let received = input.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancelled = cancel.clone();
        thread::Builder::new()
            .name("outfit-v2-readonly-shadow".into())
            .spawn(move || {
                let _ = run(owner, stop, cancelled, received);
            })
            .ok()?;
        Some(Self { input, cancel })
    }
    pub(crate) fn disable(&self) {
        self.cancel.store(true, Ordering::Release);
        if let Ok(mut input) = self.input.try_lock() {
            input.frozen = Frozen::default();
            input.revision = input.revision.saturating_add(1);
        }
    }
    pub(crate) fn offer(&self, choices: Vec<(u64, String)>) {
        if self.cancel.load(Ordering::Acquire) {
            return;
        }
        if choices.len() > 10
            || choices.iter().any(|(p, id)| {
                *p < 65536
                    || *p % 8 != 0
                    || id.len() != 48
                    || !id.bytes().all(|c| c.is_ascii_hexdigit())
            })
        {
            return;
        }
        if let Ok(mut input) = self.input.try_lock() {
            let next = Frozen(choices);
            if input.frozen != next {
                input.frozen = next;
                input.revision = input.revision.saturating_add(1);
            }
        }
    }
}
impl Drop for Monitor {
    fn drop(&mut self) {
        self.disable();
    }
}

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
fn cpu_us() -> Result<u64> {
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe {
        GetThreadTimes(
            GetCurrentThread(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )?;
    }
    let ticks = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
    Ok(ticks(kernel)
        .checked_add(ticks(user))
        .context("shadow CPU overflow")?
        / 10)
}
fn retained(handle: HANDLE, id: &Identity) -> Result<()> {
    ensure!(
        unsafe { WaitForSingleObject(handle, 0) } == WAIT_TIMEOUT,
        "shadow game ended"
    );
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe {
        GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user)?;
    }
    let birth = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
    let mut text = vec![0u16; 32768];
    let mut len = text.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(text.as_mut_ptr()),
            &mut len,
        )?;
    }
    ensure!(
        len > 0 && (len as usize) <= text.len(),
        "shadow path length"
    );
    ensure!(
        id.started.parse::<u64>()? == birth
            && crate::native_session::same_path(
                &String::from_utf16(&text[..len as usize])?,
                &std::path::PathBuf::from(&id.path)
            ),
        "shadow retained identity changed"
    );
    Ok(())
}
/// No write handle or writer is stored, accepted or returned by this adapter.
struct Observer {
    handle: Handle,
    identity: Identity,
    base: u64,
    stop: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
}
impl Observer {
    fn open(
        owner: &Owner,
        stop: Arc<AtomicBool>,
        cancel: Arc<AtomicBool>,
        admitted_base: u64,
    ) -> Result<Self> {
        let value = Self {
            handle: Handle(unsafe { OpenProcess(RIGHTS, false, owner.identity.pid)? }),
            identity: owner.identity.clone(),
            base: admitted_base,
            stop,
            cancel,
        };
        value.checkpoint()?;
        Ok(value)
    }
}
impl Memory for Observer {
    fn base(&self) -> u64 {
        self.base
    }
    fn checkpoint(&self) -> Result<()> {
        ensure!(
            !self.stop.load(Ordering::Acquire) && !self.cancel.load(Ordering::Acquire),
            "shadow cancelled"
        );
        retained(self.handle.0, &self.identity)
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        ensure!((1..=4096).contains(&len), "shadow individual read bound");
        span(at, len)?;
        self.checkpoint()?;
        let mut bytes = vec![0; len];
        let mut done = 0;
        unsafe {
            ReadProcessMemory(
                self.handle.0,
                at as _,
                bytes.as_mut_ptr() as _,
                len,
                Some(&mut done),
            )?;
        }
        ensure!(done == len, "shadow partial read");
        Ok(bytes)
    }
}
struct Budget<'a, M> {
    source: &'a M,
    bytes: Cell<usize>,
    reads: Cell<usize>,
    start: Instant,
    cpu_start: u64,
    cpu_allowance: u64,
    byte_limit: usize,
    poisoned: Cell<bool>,
    wall_limit: Duration,
}
impl<'a, M: Memory> Budget<'a, M> {
    fn new(source: &'a M, allowance: u64) -> Result<Self> {
        Ok(Self {
            source,
            bytes: Cell::new(0),
            reads: Cell::new(0),
            start: Instant::now(),
            cpu_start: cpu_us()?,
            cpu_allowance: allowance,
            byte_limit: PROBE_BYTES,
            poisoned: Cell::new(false),
            wall_limit: Duration::from_millis(40),
        })
    }
    fn guarded<T>(&self, f: impl FnOnce() -> Result<T>) -> Result<T> {
        ensure!(!self.poisoned.get(), "shadow observation poisoned");
        let result = f();
        if result.is_err() {
            self.poisoned.set(true);
        }
        result
    }
    fn guard(&self) -> Result<()> {
        self.guarded(|| {
            ensure!(self.start.elapsed() <= self.wall_limit, "shadow wall quota");
            ensure!(
                cpu_us()?
                    .checked_sub(self.cpu_start)
                    .is_some_and(|n| n <= self.cpu_allowance),
                "shadow CPU quota"
            );
            self.source.checkpoint()
        })
    }
}
impl<M: Memory> Memory for Budget<'_, M> {
    fn base(&self) -> u64 {
        self.source.base()
    }
    fn checkpoint(&self) -> Result<()> {
        self.guard()
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        self.guarded(|| {
            ensure!(
                (1..=4096).contains(&len)
                    && self
                        .bytes
                        .get()
                        .checked_add(len)
                        .is_some_and(|n| n <= self.byte_limit),
                "shadow byte quota"
            );
            self.guard()?;
            self.bytes.set(self.bytes.get() + len);
            self.reads.set(self.reads.get() + 1);
            let bytes = self.source.read(at, len)?;
            ensure!(bytes.len() == len, "shadow partial budgeted read");
            self.guard()?;
            Ok(bytes)
        })
    }
}
#[derive(Default)]
struct CpuBucket {
    credit: i128,
    last_ms: u64,
}
impl CpuBucket {
    fn start(now: u64) -> Self {
        Self {
            credit: i128::from(BURST_US),
            last_ms: now,
        }
    }
    fn allowance(&mut self, now: u64) -> Result<u64> {
        let elapsed = now
            .checked_sub(self.last_ms)
            .context("shadow clock rollback")?;
        self.credit = self
            .credit
            .saturating_add(i128::from(elapsed) * 25)
            .min(i128::from(BURST_US));
        self.last_ms = now;
        Ok(self.credit.max(0) as u64)
    }
    fn debit(&mut self, used: u64) {
        self.credit = self.credit.saturating_sub(i128::from(used));
    }
}
fn offline(c: &crate::blacktop_native::ContextState, base: u64) -> bool {
    c.current == c.alternate && c.kind == 0 && c.game_mode == 6 && c.root_tag == base + 0x0791E158
}
// Exact duplicated context, rather than the majority helper alone, for a fact.
fn context(m: &impl Memory) -> Result<crate::blacktop_native::ContextState> {
    let a = rules_v2::context(m)?;
    let b = rules_v2::context(m)?;
    ensure!(a == b && a.current == a.alternate, "shadow torn context");
    Ok(a)
}
// A generation is private metadata identity, never a new-match ownership proof.
type Generation = Vec<(u64, u64, u64, u64)>;
fn generation(m: &impl Memory, originals: &[u64]) -> Result<Generation> {
    ensure!(
        (1..=10).contains(&originals.len())
            && originals
                .iter()
                .enumerate()
                .all(|(i, p)| !originals[..i].contains(p)),
        "shadow roster identities"
    );
    let before = context(m)?;
    let table = read(m, m.base() + TABLE, 30 * 0x50)?;
    let mut out = Vec::new();
    for row in table.chunks_exact(0x50) {
        let word = |n| u64::from_le_bytes(row[n..n + 8].try_into().unwrap());
        if word(0) != m.base() + WRAPPER || word(0x10) == 0 || word(0x28) == 0 {
            continue;
        }
        let loaded = word(0x28);
        span(loaded, 0x528)?;
        let original = q(m, loaded + 0x500)?;
        if !originals.contains(&original) {
            continue;
        }
        let manager = word(0x10);
        ensure!(q(m, loaded + 0x518)? == manager, "shadow manager binding");
        let primary = if q(m, manager)? == m.base() + SECONDARY {
            manager
                .checked_sub(0x78)
                .context("shadow interface underflow")?
        } else {
            manager
        };
        ensure!(
            q(m, primary)? == m.base() + MANAGER && q(m, primary + 0x78)? == m.base() + SECONDARY,
            "shadow manager class"
        );
        let actor = q(m, primary + 0x110)?;
        ensure!(
            q(m, actor)? == m.base() + PLAYER
                && !out.iter().any(|(o, l, p, a)| *o == original
                    || *l == loaded
                    || *p == primary
                    || *a == actor),
            "shadow actor class or duplicate"
        );
        out.push((original, loaded, primary, actor));
    }
    out.sort_unstable();
    ensure!(
        out.len() == originals.len()
            && context(m)? == before
            && read(m, m.base() + TABLE, 30 * 0x50)? == table,
        "shadow generation changed"
    );
    for (o, l, p, a) in &out {
        ensure!(
            q(m, *l + 0x500)? == *o && q(m, *p + 0x110)? == *a,
            "shadow actor replaced"
        );
    }
    Ok(out)
}
/// Mapped WPF subset, intentionally diagnostic only. This is not WPF binary parity.
fn wpf_like(m: &impl Memory, originals: &[u64]) -> Result<[u8; 32]> {
    let before = context(m)?;
    ensure!(
        offline(&before, m.base()),
        "shadow offline gameMode6 absent"
    );
    let observed_generation = generation(m, originals)?;
    let mut hash = Sha256::new();
    for (_, _, primary, actor) in &observed_generation {
        ensure!(
            q(m, *primary + 0x118)? == 0 && read(m, *primary + 0x130, 1)? == [0],
            "shadow actor busy"
        );
        let desc = description(m, *actor + 0xF30)?;
        ensure!(
            desc.count == 2 && desc.names.len() == 2,
            "shadow ordinary stock pair absent"
        );
        hash.update(primary.to_le_bytes());
        hash.update(actor.to_le_bytes());
        hash.update(&desc.bytes);
        for n in desc.names {
            hash.update(n.as_bytes());
            hash.update([0]);
        }
    }
    ensure!(
        context(m)? == before && generation(m, originals)? == observed_generation,
        "shadow WPF-like observation raced"
    );
    Ok(hash.finalize().into())
}
#[derive(Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Facts {
    tick_ms: u64,
    elapsed_ms: u64,
    window: u8,
    mode: u32,
    stage: u32,
    game_mode: u32,
    strict_boundary: bool,
    queued: u8,
    wpf_like: bool,
    deep: bool,
    stable_three: bool,
    generation: &'static str,
    generation_ordinal: u8,
    reason: &'static str,
    probe_kind: &'static str,
    reads: usize,
    bytes: usize,
}
struct Logger {
    _pins: crate::loader_files::Directories,
    file: File,
    events: usize,
}
impl Logger {
    fn open(owner: &Owner) -> Result<Self> {
        ensure!(
            owner.session.len() == 48 && owner.session.bytes().all(|c| c.is_ascii_hexdigit()),
            "shadow session nonce"
        );
        let folder = owner.root.join("Logs/game-session");
        let pins = crate::loader_files::Directories::pin(&folder)?;
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(1)
            .custom_flags(0x00200000)
            .open(folder.join(format!(
                "outfit-ready-shadow-{}-{}.jsonl",
                owner.identity.pid, owner.session
            )))?;
        Ok(Self {
            _pins: pins,
            file,
            events: 0,
        })
    }
    fn record(&mut self, facts: &Facts) -> Result<()> {
        record_into(&mut self.file, &mut self.events, facts)?;
        // Only this below-normal diagnostic thread flushes; failures disable it.
        self.file.sync_all()?;
        Ok(())
    }
}
fn record_into(writer: &mut impl Write, events: &mut usize, facts: &Facts) -> Result<()> {
    ensure!(*events < MAX_EVENTS, "shadow log cap");
    let mut bytes = serde_json::to_vec(facts)?;
    ensure!(bytes.len() < 1024, "shadow facts bound");
    bytes.push(b'\n');
    writer.write_all(&bytes)?;
    *events += 1;
    Ok(())
}
fn guard(owner: &Owner) -> Result<()> {
    ensure!(
        session_safety::identity(owner.parent.pid)? == owner.parent,
        "shadow owner changed"
    );
    let rows = session_safety::processes()?
        .into_iter()
        .filter(|p| {
            !(session_safety::launcher_process(p) && p.identity.as_ref() == Some(&owner.parent))
        })
        .collect::<Vec<_>>();
    crate::blacktop_native::guard(&rows, &owner.identity)
}
fn run(
    owner: Owner,
    stop: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    input: Arc<Mutex<Input>>,
) -> Result<()> {
    ensure!(
        unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL) }.is_ok(),
        "shadow priority"
    );
    // No game handle is opened until a private queued snapshot exists.
    let mut observer = None;
    let mut logger = None;
    let mut baseline: Option<Generation> = None;
    let mut baseline_attempted = false;
    let mut revision = 0;
    let mut frozen = Frozen::default();
    let mut window_start = None;
    let mut windows = 0u8;
    let mut bytes_left = WINDOW_BYTES;
    let mut last_deep = 0;
    let mut stable: Option<([u8; 32], u64, u8)> = None;
    let mut last_key = None;
    let mut last_generation: Option<Generation> = None;
    let mut generation_ordinal = 0u8;
    let mut bucket = CpuBucket::start(unsafe { GetTickCount64() });
    let mut last_attempt = 0;
    while !stop.load(Ordering::Acquire) && !cancel.load(Ordering::Acquire) {
        thread::sleep(Duration::from_millis(50));
        let (next, rev) = {
            let received = input
                .lock()
                .map_err(|_| anyhow::anyhow!("shadow input poisoned"))?;
            (received.frozen.clone(), received.revision)
        };
        if next.0.is_empty() {
            if window_start.is_some() {
                window_start = None;
                baseline = None;
            }
            continue;
        }
        if rev != revision {
            revision = rev;
            frozen = next;
            baseline = None;
            baseline_attempted = false;
            window_start = None;
            stable = None;
        }
        let now = unsafe { GetTickCount64() };
        if now.saturating_sub(last_attempt) < 250 {
            continue;
        }
        last_attempt = now;
        let allowance = bucket.allowance(now)?;
        if allowance < BURST_US {
            continue;
        }
        if window_start.is_some_and(|at| now.saturating_sub(at) >= WINDOW_MS)
            || bytes_left <= ADMISSION_BYTES
        {
            break;
        }
        let cpu_begin = cpu_us()?;
        guard(&owner)?;
        // Fresh anti-cheat module / first-image / exact observer admission each attempt.
        let admitted =
            crate::blacktop_native::Reader::open_readiness(&owner.identity, &owner.game)?;
        if observer.is_none() {
            observer = Some(Observer::open(
                &owner,
                stop.clone(),
                cancel.clone(),
                admitted.base(),
            )?);
        }
        let m = observer.as_ref().unwrap();
        ensure!(admitted.base() == m.base(), "shadow image replaced");
        if logger.is_none() {
            logger = Some(Logger::open(&owner)?);
        }
        let mut budget = Budget::new(
            m,
            allowance.saturating_sub(cpu_us()?.saturating_sub(cpu_begin)),
        )?;
        budget.byte_limit = PROBE_BYTES.min(bytes_left).saturating_sub(ADMISSION_BYTES);
        let originals = frozen.0.iter().map(|(p, _)| *p).collect::<Vec<_>>();
        let outcome = (|| -> Result<Option<Facts>> {
            verify_profile(&budget)?;
            let c = context(&budget)?;
            if rules_v2::boundary(&c, budget.base()) == "selection" {
                if window_start.is_some() {
                    window_start = None;
                    baseline = None;
                    baseline_attempted = false;
                    stable = None;
                }
                if !baseline_attempted {
                    baseline_attempted = true;
                    baseline = generation(&budget, &originals).ok();
                }
                return Ok(None);
            }
            if window_start.is_none() {
                if windows >= 8 {
                    return Ok(None);
                }
                windows += 1;
                window_start = Some(now);
                bytes_left = WINDOW_BYTES;
                stable = None;
                last_key = None;
                last_generation = None;
                generation_ordinal = 0;
                last_deep = 0;
            }
            let elapsed = now
                .checked_sub(window_start.unwrap())
                .context("shadow clock rollback")?;
            ensure!(elapsed < WINDOW_MS && bytes_left > 0, "shadow window ended");
            let observed_generation = generation(&budget, &originals).ok();
            if observed_generation != last_generation {
                generation_ordinal = generation_ordinal.saturating_add(1);
                last_generation = observed_generation.clone();
            }
            let cause = match (&baseline, &observed_generation) {
                (Some(a), Some(b)) if a == b => "reused-generation",
                (Some(_), Some(_)) => "changed-metadata-only",
                _ => "baseline-unknown",
            };
            // Keep the two full observations separate: a ten-player deep probe
            // must not inherit another wardrobe snapshot's byte/CPU consumption.
            let do_deep = now.saturating_sub(last_deep) >= 1000;
            let (wf, deep_ok) = if do_deep {
                last_deep = now;
                (None, deep(&budget, &originals).is_ok())
            } else {
                (wpf_like(&budget, &originals).ok(), false)
            };
            if let Some(sig) = wf {
                stable = Some(match stable {
                    Some((old, at, n)) if old == sig && now >= at + 250 => {
                        (sig, now, n.saturating_add(1).min(3))
                    }
                    _ => (sig, now, 1),
                });
            } else {
                stable = None;
            }
            ensure!(context(&budget)? == c, "shadow ending context changed");
            budget.guard()?;
            guard(&owner)?;
            budget.guard()?;
            let current_input = input
                .lock()
                .map_err(|_| anyhow::anyhow!("shadow input poisoned"))?;
            ensure!(
                current_input.revision == revision && current_input.frozen == frozen,
                "shadow choices changed"
            );
            Ok(Some(Facts {
                tick_ms: now,
                elapsed_ms: elapsed,
                window: windows,
                mode: c.current,
                stage: c.stage,
                game_mode: c.game_mode,
                strict_boundary: rules_v2::boundary(&c, budget.base()) == "match",
                queued: originals.len() as u8,
                wpf_like: wf.is_some(),
                deep: deep_ok,
                stable_three: stable.as_ref().is_some_and(|(_, _, n)| *n == 3),
                generation: cause,
                generation_ordinal,
                reason: "diagnostic-only-no-admission",
                probe_kind: if do_deep { "deep" } else { "wpf-like" },
                reads: budget.reads.get(),
                bytes: budget.bytes.get().saturating_add(ADMISSION_BYTES),
            }))
        })();
        bytes_left = bytes_left.saturating_sub(budget.bytes.get().saturating_add(ADMISSION_BYTES));
        let used = cpu_us()?
            .checked_sub(cpu_begin)
            .context("shadow CPU rollback")?;
        if let Ok(Some(facts)) = outcome {
            if used > allowance {
                bucket.debit(used);
                continue;
            }
            let key = (
                facts.window,
                facts.mode,
                facts.stage,
                facts.strict_boundary,
                facts.wpf_like,
                facts.deep,
                facts.stable_three,
                facts.generation,
                facts.probe_kind,
            );
            if last_key.as_ref() != Some(&key) {
                m.checkpoint()?;
                logger.as_mut().unwrap().record(&facts)?;
                last_key = Some(key);
            }
        } else if outcome.is_err() {
            // Quota/tearing failures cannot be mistaken for readiness. No raw errors logged.
            stable = None;
            if let Some(started) = window_start {
                let facts = Facts {
                    tick_ms: now,
                    elapsed_ms: now.saturating_sub(started),
                    window: windows,
                    mode: 0,
                    stage: 0,
                    game_mode: 0,
                    strict_boundary: false,
                    queued: originals.len() as u8,
                    wpf_like: false,
                    deep: false,
                    stable_three: false,
                    generation: "baseline-unknown",
                    generation_ordinal: 0,
                    reason: "observation-unknown",
                    probe_kind: "unknown",
                    reads: budget.reads.get(),
                    bytes: budget.bytes.get().saturating_add(ADMISSION_BYTES),
                };
                let key = (
                    windows,
                    0,
                    0,
                    false,
                    false,
                    false,
                    false,
                    "baseline-unknown",
                    "unknown",
                );
                if last_key.as_ref() != Some(&key) {
                    m.checkpoint()?;
                    logger.as_mut().unwrap().record(&facts)?;
                    last_key = Some(key);
                }
            }
            if window_start.is_some_and(|at| now.saturating_sub(at) >= WINDOW_MS) {
                break;
            }
        }
        bucket.debit(
            cpu_us()?
                .checked_sub(cpu_begin)
                .context("shadow CPU rollback")?,
        );
    }
    Ok(())
}

fn deep(m: &impl Memory, originals: &[u64]) -> Result<()> {
    ensure!(
        !originals.is_empty()
            && originals.len() <= 10
            && originals
                .iter()
                .enumerate()
                .all(|(i, v)| !originals[..i].contains(v)),
        "V2 match original roster identities invalid"
    );
    verify_profile(m)?;
    queue_ready(m, true)?;
    let before = rules_v2::context(m)?;
    ensure!(offline(&before, m.base()), "Wait for loaded Blacktop match");
    // The 30 wrapper records form one bounded image span. Read it once
    // instead of issuing a guarded process read for each slot/field. Fresh
    // ending context and per-player binding/description checks below remain
    // independent reads, so replacement while capturing still rejects.
    let table = read(m, m.base() + TABLE, 30 * 0x50)?;
    let mut out = Vec::new();
    for n in 0..30usize {
        let row = m.base() + TABLE + n as u64 * 0x50;
        let record = &table[n * 0x50..(n + 1) * 0x50];
        let word =
            |offset: usize| u64::from_le_bytes(record[offset..offset + 8].try_into().unwrap());
        if word(0) != m.base() + WRAPPER {
            continue;
        }
        let table_manager = word(0x10);
        let loaded = word(0x28);
        span(loaded, 0x528)?;
        if table_manager == 0 || loaded == 0 {
            continue;
        }
        let original = q(m, loaded + 0x500)?;
        if !originals.contains(&original) {
            continue;
        }
        ensure!(
            q(m, loaded + 0x518)? == table_manager,
            "V2 roster-manager binding differs"
        );
        let primary = if q(m, table_manager)? == m.base() + SECONDARY {
            table_manager
                .checked_sub(0x78)
                .context("V2 manager interface underflow")?
        } else {
            table_manager
        };
        ensure!(
            q(m, primary)? == m.base() + MANAGER && q(m, primary + 0x78)? == m.base() + SECONDARY,
            "V2 roster-manager binding differs"
        );
        ensure!(
            !out.iter()
                .any(|p: &Player| p.original_roster == original || p.primary == primary),
            "V2 match identity ambiguous"
        );
        active_member(m, primary)?;
        let state = word(0x18);
        ensure!(
            state != 0 && q(m, loaded + 0x520)? == state && record[0x20] == 1,
            "V2 wrapper state differs"
        );
        let actor = q(m, primary + 0x110)?;
        ensure!(
            q(m, actor)? == m.base() + PLAYER,
            "V2 match actor class differs"
        );
        let description = description(m, actor + 0xF30)?;
        let label = d(m, description.address + 0x88)?;
        ensure!(label != 0, "V2 description label absent");
        idle(m, primary, actor, label)?;
        out.push(Player {
            row,
            table_manager,
            primary,
            actor,
            loaded_roster: loaded,
            original_roster: original,
            state,
            description,
        });
    }
    ensure!(
        out.len() == originals.len() && rules_v2::context(m)? == before,
        "V2 match players not stably loaded"
    );
    for p in &out {
        ensure!(
            q(m, p.row)? == m.base() + WRAPPER
                && q(m, p.row + 0x10)? == p.table_manager
                && q(m, p.row + 0x18)? == p.state
                && read(m, p.row + 0x20, 1)? == [1]
                && q(m, p.loaded_roster + 0x518)? == p.table_manager
                && q(m, p.row + 0x28)? == p.loaded_roster
                && q(m, p.loaded_roster + 0x500)? == p.original_roster
                && q(m, p.primary + 0x110)? == p.actor
                && description(m, p.description.address)? == p.description,
            "V2 match binding changed"
        );
    }
    queue_ready(m, true)?;
    Ok(())
}

#[cfg(test)]
#[path = "outfit_match_v2_shadow_tests.rs"]
mod tests;
