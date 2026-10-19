//! Win11-build26200 state-object backend. Private same-image children and
//! duplicated retained handles only. No caller-supplied addresses or raw-PID resume.
use super::*;
use anyhow::{Context as AnyhowContext, bail};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    mem::{size_of, zeroed},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle},
        process::CommandExt,
    },
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Duration,
};
type Raw = isize;
const NO_WINDOW: u32 = 0x0800_0000;
const BREAKAWAY: u32 = 0x0100_0000;
const TIMEOUT: u32 = 258;
const CURRENT: Raw = -1;
const GAME_ACCESS: u32 = 0x0010_1e39; // synchronize, query, set-info, suspend, VM and terminate
const COMPANION_ACCESS: u32 = 0x0010_1041; // synchronize, query, duplicate, terminate
const NTDLL_SHA: &str = "651ac9041af8e6b5ebd331d53163244c0a27d36d1433f0f15cac1f28c5f2e82c";
#[repr(C)]
#[derive(Default)]
struct Ft {
    lo: u32,
    hi: u32,
}
#[repr(C)]
struct Basic {
    status: i32,
    peb: usize,
    affinity: usize,
    priority: i32,
    pid: usize,
    parent: usize,
}
#[repr(C)]
struct Extended {
    size: usize,
    basic: Basic,
    flags: u32,
}
#[repr(C)]
struct Version {
    size: u32,
    major: u32,
    minor: u32,
    build: u32,
    platform: u32,
    service: [u16; 128],
}
#[repr(C)]
#[derive(Default)]
struct ThreadRow {
    size: u32,
    usage: u32,
    tid: u32,
    pid: u32,
    base: i32,
    delta: i32,
    flags: u32,
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CloseHandle(h: Raw) -> i32;
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Raw;
    fn GetCurrentProcessId() -> u32;
    fn GetProcessId(h: Raw) -> u32;
    fn GetProcessTimes(h: Raw, a: *mut Ft, b: *mut Ft, c: *mut Ft, d: *mut Ft) -> i32;
    fn QueryFullProcessImageNameW(h: Raw, flags: u32, buffer: *mut u16, size: *mut u32) -> i32;
    fn WaitForSingleObject(h: Raw, ms: u32) -> u32;
    fn TerminateProcess(h: Raw, status: u32) -> i32;
    fn DuplicateHandle(
        source_process: Raw,
        source: Raw,
        target_process: Raw,
        out: *mut Raw,
        access: u32,
        inherit: i32,
        options: u32,
    ) -> i32;
    fn GetModuleHandleW(name: *const u16) -> Raw;
    fn GetModuleFileNameW(h: Raw, out: *mut u16, len: u32) -> u32;
    fn GetProcAddress(h: Raw, name: *const u8) -> *const core::ffi::c_void;
    fn GetTickCount64() -> u64;
    fn CheckRemoteDebuggerPresent(h: Raw, out: *mut i32) -> i32;
    fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> Raw;
    fn Thread32First(h: Raw, row: *mut ThreadRow) -> i32;
    fn Thread32Next(h: Raw, row: *mut ThreadRow) -> i32;
    fn OpenThread(access: u32, inherit: i32, tid: u32) -> Raw;
    fn GetLastError() -> u32;
    fn CreateEventW(
        attr: *const core::ffi::c_void,
        manual: i32,
        initial: i32,
        name: *const u16,
    ) -> Raw;
    fn SetEvent(h: Raw) -> i32;
    fn GetStdHandle(kind: u32) -> Raw;
    fn PeekNamedPipe(
        h: Raw,
        buffer: *mut u8,
        len: u32,
        read: *mut u32,
        available: *mut u32,
        left: *mut u32,
    ) -> i32;
    fn ReadFile(
        h: Raw,
        buffer: *mut u8,
        len: u32,
        read: *mut u32,
        overlapped: *mut core::ffi::c_void,
    ) -> i32;
}
type CreateState =
    unsafe extern "system" fn(*mut Raw, u32, *const core::ffi::c_void, Raw, u32) -> i32;
type ChangeState =
    unsafe extern "system" fn(Raw, Raw, u32, *const core::ffi::c_void, usize, u32) -> i32;
type QueryProcess =
    unsafe extern "system" fn(Raw, u32, *mut core::ffi::c_void, u32, *mut u32) -> i32;
type QueryThread =
    unsafe extern "system" fn(Raw, u32, *mut core::ffi::c_void, u32, *mut u32) -> i32;
type GetVersion = unsafe extern "system" fn(*mut Version) -> i32;
struct Api {
    create: CreateState,
    change: ChangeState,
    query: QueryProcess,
    thread: QueryThread,
}
fn hash(path: &Path) -> Result<String> {
    let mut f = fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut b = [0u8; 65536];
    loop {
        let n = f.read(&mut b)?;
        if n == 0 {
            break;
        }
        h.update(&b[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}
fn os_error(label: &str) -> anyhow::Error {
    anyhow::anyhow!("{}: {}", label, std::io::Error::last_os_error())
}
struct Handle(Raw);
impl Handle {
    fn new(raw: Raw) -> Result<Self> {
        ensure!(raw != 0 && raw != -1, "Invalid retained handle");
        Ok(Self(raw))
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
fn live(h: Raw) -> bool {
    unsafe { WaitForSingleObject(h, 0) == TIMEOUT }
}
/// A retained process object was authenticated at bootstrap. During teardown,
/// its image path can disappear before the process handle becomes signaled.
/// Identity failure NEVER permits terminating an unknown process: only a bounded
/// wait proving this exact retained writer has exited can bypass that error.
pub(super) fn helper_stopped_on_identity_error(
    identity: Result<()>,
    wait: impl FnOnce(u32) -> u32,
) -> Result<bool> {
    match identity {
        Ok(()) => Ok(false),
        Err(original) => {
            if wait(250) == 0 {
                Ok(true)
            } else {
                Err(original)
            }
        }
    }
}
fn tick() -> u64 {
    unsafe { GetTickCount64() }
}
fn id(h: Raw) -> Result<Identity> {
    ensure!(live(h), "Retained process ended");
    let (mut a, mut b, mut c, mut d) = (Ft::default(), Ft::default(), Ft::default(), Ft::default());
    ensure!(
        unsafe { GetProcessTimes(h, &mut a, &mut b, &mut c, &mut d) } != 0,
        "Process birth unavailable"
    );
    let mut path = vec![0u16; 32768];
    let mut n = path.len() as u32;
    ensure!(
        unsafe { QueryFullProcessImageNameW(h, 0, path.as_mut_ptr(), &mut n) } != 0 && n > 0,
        "Process image unavailable"
    );
    let result = Identity {
        pid: unsafe { GetProcessId(h) },
        started: (((a.hi as u64) << 32) | a.lo as u64).to_string(),
        path: String::from_utf16(&path[..n as usize])?,
    };
    ensure!(
        result.pid > 0 && result.started != "0" && live(h),
        "Process identity changed"
    );
    Ok(result)
}
fn same(h: Raw, expected: &Identity) -> Result<()> {
    ensure!(id(h)? == *expected, "Retained process identity mismatch");
    Ok(())
}
fn duplicate(source: Raw, target: Raw) -> Result<usize> {
    let mut out = 0;
    ensure!(
        unsafe { DuplicateHandle(CURRENT, source, target, &mut out, 0, 0, 2) } != 0,
        "Cannot duplicate retained capability"
    );
    Ok(out as usize)
}
impl Api {
    fn load() -> Result<Self> {
        unsafe {
            let name: Vec<u16> = "ntdll.dll\0".encode_utf16().collect();
            let h = GetModuleHandleW(name.as_ptr());
            ensure!(h != 0, "ntdll unavailable");
            let mut path = vec![0u16; 32768];
            let n = GetModuleFileNameW(h, path.as_mut_ptr(), path.len() as u32);
            ensure!(
                n > 0 && (n as usize) < path.len(),
                "ntdll image path unavailable"
            );
            ensure!(
                hash(Path::new(&String::from_utf16(&path[..n as usize])?))? == NTDLL_SHA,
                "Owned freeze is unsupported on this ntdll build"
            );
            macro_rules! symbol {
                ($name:literal,$ty:ty) => {{
                    let p = GetProcAddress(h, concat!($name, "\0").as_ptr());
                    ensure!(!p.is_null(), concat!($name, " unavailable"));
                    std::mem::transmute::<*const core::ffi::c_void, $ty>(p)
                }};
            }
            let version = symbol!("RtlGetVersion", GetVersion);
            let mut v: Version = zeroed();
            v.size = size_of::<Version>() as u32;
            ensure!(
                version(&mut v) >= 0 && (v.major, v.minor, v.build) == (10, 0, 26200),
                "Owned freeze requires the tested OS capability"
            );
            Ok(Self {
                create: symbol!("NtCreateProcessStateChange", CreateState),
                change: symbol!("NtChangeProcessState", ChangeState),
                query: symbol!("NtQueryInformationProcess", QueryProcess),
                thread: symbol!("NtQueryInformationThread", QueryThread),
            })
        }
    }
    fn basic(&self, h: Raw) -> Result<Extended> {
        let mut e: Extended = unsafe { zeroed() };
        e.size = size_of::<Extended>();
        ensure!(
            unsafe {
                (self.query)(
                    h,
                    0,
                    (&mut e as *mut Extended).cast(),
                    size_of::<Extended>() as u32,
                    std::ptr::null_mut(),
                )
            } >= 0,
            "Process freeze state unavailable"
        );
        Ok(e)
    }
    fn frozen(&self, h: Raw) -> Result<bool> {
        Ok(self.basic(h)?.flags & 16 != 0)
    }
    fn create(&self, game: Raw) -> Result<Handle> {
        let mut out = 0;
        ensure!(
            unsafe { (self.create)(&mut out, 1, std::ptr::null(), game, 0) } >= 0,
            "Owned state-object creation failed"
        );
        Handle::new(out)
    }
    fn assert_unowned(&self, game: Raw, pid: u32) -> Result<()> {
        let mut debug = 0;
        ensure!(
            unsafe { CheckRemoteDebuggerPresent(game, &mut debug) } != 0 && debug == 0,
            "Debugger or unknown debugger state refuses structural edits"
        );
        ensure!(
            !self.frozen(game)?,
            "Target already frozen by another owner"
        );
        let snap = Handle::new(unsafe { CreateToolhelp32Snapshot(4, 0) })?;
        let mut row = ThreadRow::default();
        row.size = size_of::<ThreadRow>() as u32;
        ensure!(
            unsafe { Thread32First(snap.0, &mut row) } != 0,
            "Thread ownership snapshot unavailable"
        );
        let mut seen = 0usize;
        loop {
            if row.pid == pid {
                seen += 1;
                ensure!(seen <= 8192, "Thread snapshot exceeds bound");
                let t = Handle::new(unsafe { OpenThread(0x40, 0, row.tid) })?;
                let mut count = 0u32;
                ensure!(
                    unsafe {
                        (self.thread)(
                            t.0,
                            35,
                            (&mut count as *mut u32).cast(),
                            4,
                            std::ptr::null_mut(),
                        )
                    } >= 0
                        && count == 0,
                    "Foreign or unknown thread suspension refuses structural edits"
                );
            }
            row.size = size_of::<ThreadRow>() as u32;
            if unsafe { Thread32Next(snap.0, &mut row) } == 0 {
                ensure!(
                    unsafe { GetLastError() } == 18,
                    "Thread snapshot ended unexpectedly"
                );
                break;
            }
        }
        ensure!(
            seen > 0 && !self.frozen(game)?,
            "Target thread state changed"
        );
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HelperRequest {
    request: Request,
    watchdog: Identity,
    watchdog_handle: usize,
    state_handle: usize,
    ready_handle: usize,
    start_handle: usize,
    watch_ready_handle: usize,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WatchRequest {
    request: Request,
    helper: Identity,
    helper_handle: usize,
    state_handle: usize,
    ready_handle: usize,
    started: u64,
    control_handle: usize,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
enum WatchMessage {
    FreezeAttempt {
        ack: usize,
    },
    RetainIntent {
        file: usize,
        sha256: String,
        bytes: u64,
        ack: usize,
    },
    Finished(Finished),
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Finished {
    result: Option<Value>,
    error: Option<String>,
    effect_attempted: bool,
    verified_intent: Option<String>,
}
fn frame<W: Write, T: Serialize>(w: &mut W, value: &T, limit: usize) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    ensure!(bytes.len() <= limit, "Transaction message exceeds bound");
    w.write_all(&(bytes.len() as u32).to_le_bytes())?;
    w.write_all(&bytes)?;
    w.flush()?;
    Ok(())
}
fn unframe<R: Read, T: for<'de> Deserialize<'de>>(r: &mut R, limit: usize) -> Result<T> {
    let mut size = [0; 4];
    r.read_exact(&mut size)?;
    let n = u32::from_le_bytes(size) as usize;
    ensure!(n > 0 && n <= limit, "Invalid transaction message bound");
    let mut bytes = vec![0; n];
    r.read_exact(&mut bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}
fn complete_frame<T: for<'de> Deserialize<'de>>(bytes: &[u8], limit: usize) -> Result<T> {
    ensure!(bytes.len() >= 4, "Incomplete transaction frame");
    let n = u32::from_le_bytes(bytes[..4].try_into()?) as usize;
    ensure!(
        n > 0 && n <= limit && n + 4 == bytes.len(),
        "Incomplete or trailing transaction frame"
    );
    let mut input: &[u8] = bytes;
    unframe(&mut input, limit)
}
fn new_child(mode: &str) -> Result<Child> {
    let exe = std::env::current_exe()?;
    let mut c = Command::new(&exe);
    c.arg(mode)
        .creation_flags(NO_WINDOW | BREAKAWAY)
        .stdin(Stdio::piped())
        .stderr(Stdio::null());
    c.stdout(Stdio::piped());
    #[cfg(test)]
    c.stderr(Stdio::inherit());
    c.spawn()
        .context("Cannot start independent owned transaction child (job breakaway is required)")
}
fn validate_child(child: &Child, exe: &Path) -> Result<Identity> {
    let got = id(child.as_raw_handle() as Raw)?;
    ensure!(
        Path::new(&got.path) == exe && hash(exe)? == hash(Path::new(&got.path))?,
        "Transaction child image mismatch"
    );
    Ok(got)
}
fn check_parent(api: &Api, handle: Raw, expected: &Identity) -> Result<()> {
    same(handle, expected)?;
    let b = api.basic(CURRENT)?;
    ensure!(
        b.basic.parent == expected.pid as usize,
        "Transaction creator mismatch"
    );
    ensure!(
        Path::new(&expected.path) == std::env::current_exe()?.as_path(),
        "Transaction creator image mismatch"
    );
    Ok(())
}
fn check_game(h: Raw, expected: &Identity, parent: &Identity) -> Result<()> {
    same(h, expected)?;
    #[cfg(test)]
    if Path::new(&expected.path) == std::env::current_exe()?.as_path() {
        ensure!(
            Api::load()?.basic(h)?.basic.parent == parent.pid as usize,
            "Synthetic target is not the exact owned child"
        );
        ensure!(
            expected.pid != parent.pid,
            "Synthetic target must differ from owner"
        );
        return Ok(());
    }
    let _ = parent;
    ensure!(
        hash(Path::new(&expected.path))?.eq_ignore_ascii_case(GAME_SHA),
        "Unsupported game image"
    );
    Ok(())
}

pub(crate) struct Context {
    request: Request,
    game: Handle,
    parent: Handle,
    state: Handle,
    api: Api,
    watchdog: Handle,
    watchdog_identity: Identity,
    lifecycle: Lifecycle,
    intent_sealed: bool,
    own_intent: Option<RetainedIntent>,
}
impl Context {
    pub(crate) fn root(&self) -> &Path {
        Path::new(&self.request.root)
    }
    pub(crate) fn game(&self) -> &Identity {
        &self.request.game
    }
    pub(crate) fn parent(&self) -> &Identity {
        &self.request.parent
    }
    pub(crate) fn session(&self) -> &str {
        &self.request.session
    }
    pub(crate) fn trusted_companions(&self) -> Result<Vec<Identity>> {
        same(self.watchdog.0, &self.watchdog_identity)?;
        Ok(vec![id(CURRENT)?, self.watchdog_identity.clone()])
    }
    /// Call ONLY after operation-specific intent has been CREATE_NEW, fsynced,
    /// read back and retained. No native effect is allowed before mark_effect().
    pub(crate) fn retain_intent(&mut self, file: &fs::File, expected_sha256: &str) -> Result<()> {
        ensure!(
            self.lifecycle.phase == Phase::BeforeFreeze && !self.intent_sealed,
            "Intent must be sealed exactly once before freeze"
        );
        ensure!(
            expected_sha256.len() == 64 && expected_sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid intent digest"
        );
        let size = file.metadata()?.len();
        ensure!(
            file.metadata()?.is_file() && (1..=67_108_864).contains(&size),
            "Invalid immutable intent file"
        );
        let own = RetainedIntent {
            file: file.try_clone()?,
            sha256: expected_sha256.to_ascii_lowercase(),
            bytes: size,
        };
        own.read()?;
        let ready = Handle::new(unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) })?;
        let request = WatchMessage::RetainIntent {
            file: duplicate(file.as_raw_handle() as Raw, self.watchdog.0)?,
            sha256: expected_sha256.to_ascii_lowercase(),
            bytes: size,
            ack: duplicate(ready.0, self.watchdog.0)?,
        };
        frame(&mut std::io::stdout(), &request, MAX_RESPONSE)?;
        ensure!(
            unsafe { WaitForSingleObject(ready.0, 2000) } == 0 && live(self.watchdog.0),
            "Watchdog did not retain exact durable intent"
        );
        self.own_intent = Some(own);
        self.intent_sealed = true;
        Ok(())
    }
    pub(crate) fn freeze(&mut self) -> Result<()> {
        ensure!(
            self.lifecycle.phase == Phase::BeforeFreeze,
            "Freeze cannot be repeated"
        );
        ensure!(
            self.intent_sealed,
            "Durable intent not retained by watchdog"
        );
        same(self.parent.0, &self.request.parent)?;
        same(self.game.0, &self.request.game)?;
        same(self.watchdog.0, &self.watchdog_identity)?;
        self.api
            .assert_unowned(self.game.0, self.request.game.pid)?;
        self.own_intent
            .as_ref()
            .context("Original intent unavailable")?
            .read()?;
        let ready = Handle::new(unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) })?;
        let msg = WatchMessage::FreezeAttempt {
            ack: duplicate(ready.0, self.watchdog.0)?,
        };
        frame(&mut std::io::stdout(), &msg, MAX_RESPONSE)?;
        ensure!(
            unsafe { WaitForSingleObject(ready.0, 2000) } == 0 && live(self.watchdog.0),
            "Watchdog freeze phase acknowledgement failed"
        );
        let start = tick();
        self.lifecycle.freeze(start)?;
        // On uncertain syscall completion, watchdog retains the same object and
        // independently checks the process flag before it can release anything.
        ensure!(
            unsafe { (self.api.change)(self.state.0, self.game.0, 0, std::ptr::null(), 0, 0) } >= 0,
            "Owned freeze call failed"
        );
        self.checkpoint_frozen(false)
    }
    pub(crate) fn checkpoint_read(&mut self, reverse: bool) -> Result<()> {
        if self.lifecycle.phase == Phase::BeforeFreeze {
            same(self.game.0, &self.request.game)?;
            same(self.parent.0, &self.request.parent)?;
            same(self.watchdog.0, &self.watchdog_identity)?;
            Ok(())
        } else {
            self.checkpoint_frozen(reverse)
        }
    }
    pub(crate) fn checkpoint_frozen(&mut self, reverse: bool) -> Result<()> {
        same(self.game.0, &self.request.game)?;
        ensure!(
            self.api.frozen(self.game.0)?,
            "Owned target is no longer frozen"
        );
        self.lifecycle
            .check(tick(), reverse, live(self.watchdog.0))?;
        if !reverse {
            same(self.parent.0, &self.request.parent)?;
            same(self.watchdog.0, &self.watchdog_identity)?;
        }
        Ok(())
    }
    pub(crate) fn before_effect(&mut self, reverse: bool) -> Result<()> {
        self.own_intent
            .as_ref()
            .context("Original intent unavailable")?
            .read()?;
        self.checkpoint_frozen(reverse)?;
        self.lifecycle.before_effect()
    }
    /// Receipt validation belongs to the operation implementation AND the separate
    /// watchdog. This local marker alone can never unfreeze a native graph.
    pub(crate) fn mark_effect(&mut self, reverse: bool) -> Result<()> {
        self.before_effect(reverse)
    }
    pub(crate) fn mark_consistent(&mut self) -> Result<()> {
        self.own_intent
            .as_ref()
            .context("Original intent unavailable")?
            .read()?;
        self.checkpoint_frozen(true)?;
        self.lifecycle.consistent()
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        // If the other holder died, this process is now the sole consistency owner.
        // A local full verified rollback/completion is sufficient; an unknown partial
        // state must close the exact game BEFORE our last state handle is dropped.
        if live(self.watchdog.0)
            || !self.lifecycle.attempted
            || self.lifecycle.phase == Phase::Consistent
        {
            return;
        }
        while live(self.game.0) {
            if same(self.game.0, &self.request.game).is_ok() {
                let _ = unsafe { TerminateProcess(self.game.0, 0xe087) };
            }
            if unsafe { WaitForSingleObject(self.game.0, 1000) } == 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }
}

/// Stateless fresh full-byte SHA256, identical wire digest to sha2. The CNG
/// pseudo-algorithm handle is immutable; BCryptHash creates/destroys a fresh hash
/// internally for EVERY call. No live-byte/digest/provider-object cache exists.
/// https://learn.microsoft.com/en-us/windows/win32/api/bcrypt/nf-bcrypt-bcrypthash
pub(super) fn intent_sha256(bytes: &[u8]) -> Result<String> {
    ensure!(bytes.len() <= 67_108_864, "Retained intent hash bound");
    use windows::Win32::Security::Cryptography::{BCRYPT_SHA256_ALG_HANDLE, BCryptHash};
    let mut out = [0u8; 32];
    unsafe {
        BCryptHash(BCRYPT_SHA256_ALG_HANDLE, None, bytes, &mut out).ok()?;
    }
    Ok(out.iter().map(|value| format!("{value:02x}")).collect())
}
struct RetainedIntent {
    file: fs::File,
    sha256: String,
    bytes: u64,
}
impl RetainedIntent {
    fn read(&self) -> Result<Vec<u8>> {
        let before = self.file.metadata()?;
        ensure!(
            before.is_file()
                && before.len() == self.bytes
                && (1..=67_108_864).contains(&self.bytes),
            "Retained intent shape changed"
        );
        let mut reader = self.file.try_clone()?;
        reader.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        reader.take(self.bytes + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 == self.bytes
                && intent_sha256(&bytes)? == self.sha256
                && self.file.metadata()?.len() == self.bytes,
            "Retained intent bytes changed"
        );
        Ok(bytes)
    }
}
pub(crate) struct ReadContext {
    request: Request,
    game: Handle,
    parent: Handle,
    helper: Handle,
    state: Handle,
    api: Api,
    helper_identity: Identity,
    release_verified: bool,
    intent: Option<RetainedIntent>,
    effects_possible: bool,
}
impl ReadContext {
    pub(crate) fn has_intent(&self) -> bool {
        self.intent.is_some()
    }
    pub(crate) fn effect_attempted(&self) -> bool {
        self.effects_possible
    }
    pub(crate) fn read_intent(&self) -> Result<Vec<u8>> {
        ensure!(
            !live(self.helper.0),
            "Intent verification requires stopped helper"
        );
        if self.effects_possible {
            self.checkpoint_frozen()?;
        }
        self.intent
            .as_ref()
            .context("No retained immutable intent")?
            .read()
    }
    pub(crate) fn root(&self) -> &Path {
        Path::new(&self.request.root)
    }
    pub(crate) fn game(&self) -> &Identity {
        &self.request.game
    }
    pub(crate) fn parent(&self) -> &Identity {
        &self.request.parent
    }
    pub(crate) fn session(&self) -> &str {
        &self.request.session
    }
    pub(crate) fn trusted_companions(&self) -> Result<Vec<Identity>> {
        // Helper has exited before this verifier is invoked; include its retained
        // identity only as provenance, never exempt a PID reused by another process.
        ensure!(
            !live(self.helper.0),
            "Helper must stop before independent verification"
        );
        Ok(vec![id(CURRENT)?])
    }
    pub(crate) fn checkpoint_frozen(&self) -> Result<()> {
        same(self.game.0, &self.request.game)?;
        ensure!(
            !live(self.helper.0) && self.api.frozen(self.game.0)?,
            "Independent verifier requires stopped helper and retained freeze"
        );
        Ok(())
    }
}

pub(super) fn available() -> Result<()> {
    Api::load().map(|_| ())
}

pub(super) fn execute(
    root: &Path,
    game: &Identity,
    parent: &Identity,
    session: &str,
    operation: Operation,
    consent: bool,
) -> Result<Value> {
    let api = Api::load()?;
    ensure!(
        id(CURRENT)? == *parent,
        "Only the current owned launcher may request a transaction"
    );
    operation.validate()?;
    ensure!(consent, "Emergency-close consent is required");
    let gh = Handle::new(unsafe { OpenProcess(GAME_ACCESS, 0, game.pid) })?;
    check_game(gh.0, game, parent)?;
    api.assert_unowned(gh.0, game.pid)?;
    let ph = Handle::new(unsafe { OpenProcess(COMPANION_ACCESS, 0, parent.pid) })?;
    same(ph.0, parent)?;
    let state = api.create(gh.0)?; // inactive until both independent holders are ready
    let ready = Handle::new(unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) })?;
    let helper_ready =
        Handle::new(unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) })?;
    let start = Handle::new(unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) })?;
    let mut child = new_child("--owned-transaction-helper")?;
    let helper_id = validate_child(&child, &std::env::current_exe()?)?;
    let control = child.stdout.take().context("Missing helper control pipe")?;
    let mut watcher = new_child("--owned-transaction-watchdog")?;
    let watcher_id = validate_child(&watcher, &std::env::current_exe()?)?;
    let request = Request {
        root: root.to_string_lossy().into_owned(),
        game: game.clone(),
        parent: parent.clone(),
        session: session.into(),
        operation,
        emergency_close_consent: consent,
        game_handle: duplicate(gh.0, child.as_raw_handle() as Raw)?,
        parent_handle: duplicate(ph.0, child.as_raw_handle() as Raw)?,
        prior_verified: VERIFIED
            .lock()
            .map_err(|_| anyhow::anyhow!("Completion witnesses held"))?
            .iter()
            .filter(|(g, s, _)| g == game && s == session)
            .map(|(_, _, h)| h.clone())
            .collect(),
    };
    request.validate()?;
    let bootstrap = HelperRequest {
        request: request.clone(),
        watchdog: watcher_id,
        watchdog_handle: duplicate(watcher.as_raw_handle() as Raw, child.as_raw_handle() as Raw)?,
        state_handle: duplicate(state.0, child.as_raw_handle() as Raw)?,
        ready_handle: duplicate(helper_ready.0, child.as_raw_handle() as Raw)?,
        start_handle: duplicate(start.0, child.as_raw_handle() as Raw)?,
        watch_ready_handle: duplicate(ready.0, child.as_raw_handle() as Raw)?,
    };
    let mut watch_request = request.clone();
    watch_request.game_handle = duplicate(gh.0, watcher.as_raw_handle() as Raw)?;
    watch_request.parent_handle = duplicate(ph.0, watcher.as_raw_handle() as Raw)?;
    let watch = WatchRequest {
        request: watch_request,
        helper: helper_id,
        helper_handle: duplicate(child.as_raw_handle() as Raw, watcher.as_raw_handle() as Raw)?,
        state_handle: duplicate(state.0, watcher.as_raw_handle() as Raw)?,
        ready_handle: duplicate(ready.0, watcher.as_raw_handle() as Raw)?,
        started: tick(),
        control_handle: duplicate(
            control.as_raw_handle() as Raw,
            watcher.as_raw_handle() as Raw,
        )?,
    };
    frame(
        watcher
            .stdin
            .as_mut()
            .context("Watchdog bootstrap missing")?,
        &watch,
        MAX_REQUEST,
    )?;
    drop(watcher.stdin.take());
    frame(
        child.stdin.as_mut().context("Helper bootstrap missing")?,
        &bootstrap,
        MAX_REQUEST,
    )?;
    drop(child.stdin.take());
    drop(control);
    ensure!(
        unsafe { WaitForSingleObject(ready.0, 5000) } == 0
            && unsafe { WaitForSingleObject(helper_ready.0, 5000) } == 0
            && live(child.as_raw_handle() as Raw)
            && live(watcher.as_raw_handle() as Raw),
        "Independent transaction children failed to become ready"
    );
    drop(state); // only helper + independent watchdog retain this SAME object
    ensure!(
        unsafe { SetEvent(start.0) } != 0,
        "Cannot start prepared transaction helper"
    );
    let mut output = watcher
        .stdout
        .take()
        .context("Missing independent response")?;
    let mut bytes = Vec::new();
    output
        .by_ref()
        .take((MAX_RESPONSE + 5) as u64)
        .read_to_end(&mut bytes)?;
    let helper_status = child.wait()?;
    let watch_status = watcher.wait()?;
    ensure!(
        bytes.len() <= MAX_RESPONSE + 4,
        "Transaction response exceeds bound"
    );
    let result: Finished = complete_frame(&bytes, MAX_RESPONSE)?;
    if let Some(sha) = &result.verified_intent {
        remember_verified(game, session, sha)?;
    }
    ensure!(
        helper_status.success() && watch_status.success(),
        "Transaction child ended unexpectedly"
    );
    match (result.result, result.error) {
        (Some(v), None) => Ok(v),
        (_, Some(e)) => bail!("{}", e),
        _ => bail!("Transaction did not complete"),
    }
}

fn helper(execute_inside: ExecuteInside) -> Result<i32> {
    let boot: HelperRequest = unframe(&mut std::io::stdin(), MAX_REQUEST)?;
    let req = boot.request;
    req.validate()?;
    let api = Api::load()?;
    let game = Handle::new(req.game_handle as Raw)?;
    let parent = Handle::new(req.parent_handle as Raw)?;
    check_parent(&api, parent.0, &req.parent)?;
    for sha in &req.prior_verified {
        remember_verified(&req.game, &req.session, sha)?;
    }
    check_game(game.0, &req.game, &req.parent)?;
    api.assert_unowned(game.0, req.game.pid)?;
    let state = Handle::new(boot.state_handle as Raw)?;
    let watchdog = Handle::new(boot.watchdog_handle as Raw)?;
    same(watchdog.0, &boot.watchdog)?;
    let ready = Handle::new(boot.ready_handle as Raw)?;
    let start = Handle::new(boot.start_handle as Raw)?;
    let watch_ready = Handle::new(boot.watch_ready_handle as Raw)?;
    ensure!(
        unsafe { SetEvent(ready.0) } != 0,
        "Helper ready signal failed"
    );
    ensure!(
        unsafe { WaitForSingleObject(start.0, 10000) } == 0
            && unsafe { WaitForSingleObject(watch_ready.0, 0) } == 0
            && live(parent.0)
            && live(watchdog.0),
        "Parent did not authorize prepared holders"
    );
    let mut cx = Context {
        request: req.clone(),
        game,
        parent,
        state,
        api,
        watchdog,
        watchdog_identity: boot.watchdog,
        lifecycle: Lifecycle::new(),
        intent_sealed: false,
        own_intent: None,
    };
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        execute_inside(&mut cx, &req.operation)
    }));
    let message = match outcome {
        Ok(Ok(v)) => Finished {
            result: Some(v),
            error: None,
            effect_attempted: cx.lifecycle.attempted,
            verified_intent: None,
        },
        Ok(Err(e)) => Finished {
            result: None,
            error: Some(format!("{e:#}")),
            effect_attempted: cx.lifecycle.attempted,
            verified_intent: None,
        },
        Err(_) => Finished {
            result: None,
            error: Some("Transaction helper failed; independent recovery required".into()),
            effect_attempted: cx.lifecycle.attempted,
            verified_intent: None,
        },
    };
    frame(
        &mut std::io::stdout(),
        &WatchMessage::Finished(message),
        MAX_RESPONSE,
    )?;
    Ok(0)
}
// Every post-bootstrap unwind/error is fail-safe too. The watchdog retains its
// state object while stopping the ONLY writer, then closing an unverified game.
// No destructor returns while a live game might be released inconsistent.
impl Drop for ReadContext {
    fn drop(&mut self) {
        if self.release_verified {
            return;
        }
        while live(self.helper.0) {
            let _ = unsafe { TerminateProcess(self.helper.0, 0xe086) };
            if unsafe { WaitForSingleObject(self.helper.0, 1000) } == 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if !live(self.game.0) {
            return;
        }
        if matches!(self.api.frozen(self.game.0), Ok(false)) {
            return;
        }
        while emergency_close(self).is_err() {
            std::thread::sleep(Duration::from_millis(250));
        }
    }
}
fn emergency_close(cx: &ReadContext) -> Result<()> {
    ensure!(
        cx.request.emergency_close_consent,
        "Emergency-close consent missing"
    );
    if !live(cx.game.0) {
        return Ok(());
    }
    same(cx.game.0, &cx.request.game)?;
    ensure!(
        unsafe { TerminateProcess(cx.game.0, 0xe084) } != 0,
        "Exact game emergency close failed"
    );
    ensure!(
        unsafe { WaitForSingleObject(cx.game.0, 5000) } == 0,
        "Exact game exit was not confirmed"
    );
    Ok(())
}
fn watchdog(verify: VerifyInside) -> Result<i32> {
    let wr: WatchRequest = unframe(&mut std::io::stdin(), MAX_REQUEST)?;
    wr.request.validate()?;
    let game = Handle::new(wr.request.game_handle as Raw)?;
    let parent = Handle::new(wr.request.parent_handle as Raw)?;
    let helper = Handle::new(wr.helper_handle as Raw)?;
    let state = Handle::new(wr.state_handle as Raw)?;
    let ready = Handle::new(wr.ready_handle as Raw)?;
    let api = Api::load()?;
    check_parent(&api, parent.0, &wr.request.parent)?;
    same(helper.0, &wr.helper)?;
    check_game(game.0, &wr.request.game, &wr.request.parent)?;
    same(parent.0, &wr.request.parent)?;
    ensure!(
        wr.started <= tick() && tick() - wr.started < 2000,
        "Watchdog bootstrap expired"
    );
    let mut cx = ReadContext {
        request: wr.request,
        game,
        parent,
        helper,
        state,
        api,
        helper_identity: wr.helper,
        release_verified: false,
        intent: None,
        effects_possible: true,
    };
    ensure!(
        unsafe { SetEvent(ready.0) } != 0,
        "Watchdog ready signal failed"
    );
    let control = Handle::new(wr.control_handle as Raw)?;
    let pipe = control.0;
    let mut received = Vec::new();
    let mut pipe_failed = false;
    let mut completion: Option<Finished> = None;
    let mut freeze_attempted = false;
    while live(cx.helper.0) {
        // Never block on partial frames: drain only the bytes PeekNamedPipe reports.
        let mut available = 0u32;
        if unsafe {
            PeekNamedPipe(
                pipe,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        } == 0
        {
            pipe_failed = true;
        } else if available > 0 {
            let n = (available as usize).min(MAX_RESPONSE + 5 - received.len());
            if n == 0 {
                pipe_failed = true;
            } else {
                let mut chunk = vec![0; n];
                let mut got = 0;
                if unsafe {
                    ReadFile(
                        pipe,
                        chunk.as_mut_ptr(),
                        n as u32,
                        &mut got,
                        std::ptr::null_mut(),
                    )
                } == 0
                    || got == 0
                {
                    pipe_failed = true;
                } else {
                    received.extend_from_slice(&chunk[..got as usize]);
                }
            }
        }
        while received.len() >= 4 {
            let n = u32::from_le_bytes(received[..4].try_into().unwrap()) as usize;
            if n == 0 || n > MAX_RESPONSE {
                pipe_failed = true;
                break;
            }
            if received.len() < n + 4 {
                break;
            }
            let message = serde_json::from_slice::<WatchMessage>(&received[4..n + 4]);
            received.drain(..n + 4);
            match message {
                Ok(WatchMessage::FreezeAttempt { ack })
                    if cx.intent.is_some() && !freeze_attempted && completion.is_none() =>
                {
                    freeze_attempted = true;
                    let ready = Handle::new(ack as Raw)?;
                    ensure!(
                        unsafe { SetEvent(ready.0) } != 0,
                        "Freeze phase acknowledgement failed"
                    );
                }
                Ok(WatchMessage::RetainIntent {
                    file,
                    sha256,
                    bytes,
                    ack,
                }) if cx.intent.is_none() && completion.is_none() => {
                    let ready = Handle::new(ack as Raw)?;
                    let intent = RetainedIntent {
                        file: unsafe { fs::File::from_raw_handle(file as *mut _) },
                        sha256,
                        bytes,
                    };
                    intent.read()?;
                    cx.intent = Some(intent);
                    ensure!(
                        unsafe { SetEvent(ready.0) } != 0,
                        "Intent retention acknowledgement failed"
                    );
                }
                Ok(WatchMessage::Finished(value)) if completion.is_none() => {
                    completion = Some(value);
                }
                _ => {
                    pipe_failed = true;
                    break;
                }
            }
        }
        let now = tick();
        if pipe_failed || now < wr.started || now - wr.started >= WATCHDOG_MS || !live(cx.parent.0)
        {
            let already_stopped = helper_stopped_on_identity_error(
                same(cx.helper.0, &cx.helper_identity),
                |ms| unsafe { WaitForSingleObject(cx.helper.0, ms) },
            )?;
            if !already_stopped {
                ensure!(
                    unsafe { TerminateProcess(cx.helper.0, 0xe085) } != 0 || !live(cx.helper.0),
                    "Cannot stop exact transaction helper"
                );
            }
            ensure!(
                unsafe { WaitForSingleObject(cx.helper.0, 5000) } == 0,
                "Exact helper exit was not confirmed"
            );
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    // Drain the finite remaining bytes after the only writer has exited.
    loop {
        let mut n = 0u32;
        if unsafe {
            PeekNamedPipe(
                pipe,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut n,
                std::ptr::null_mut(),
            )
        } == 0
            || n == 0
        {
            break;
        }
        let take = (n as usize).min(MAX_RESPONSE + 5 - received.len());
        if take == 0 {
            pipe_failed = true;
            break;
        }
        let mut b = vec![0; take];
        let mut got = 0;
        if unsafe {
            ReadFile(
                pipe,
                b.as_mut_ptr(),
                take as u32,
                &mut got,
                std::ptr::null_mut(),
            )
        } == 0
            || got == 0
        {
            break;
        }
        received.extend_from_slice(&b[..got as usize]);
    }
    let parsed = if !pipe_failed && received.is_empty() {
        completion
    } else if !pipe_failed && completion.is_none() {
        match complete_frame::<WatchMessage>(&received, MAX_RESPONSE) {
            Ok(WatchMessage::Finished(v)) => Some(v),
            _ => None,
        }
    } else {
        None
    };
    let frozen = cx.api.frozen(cx.game.0).unwrap_or(true);
    let no_effect = !freeze_attempted || parsed.as_ref().is_some_and(|m| !m.effect_attempted);
    cx.effects_possible = !no_effect;
    let mut bookkeeping_verified = false;
    if no_effect && cx.intent.is_some() {
        let operation = cx.request.operation.clone();
        // Bookkeeping only: failure never converts known NO_EFFECT into a native
        // graph mutation or emergency close. Immutable intent remains held.
        bookkeeping_verified = matches!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                verify(&mut cx, &operation, &Value::Null)
            })),
            Ok(Ok(()))
        );
    }
    let verify_result = if !live(cx.game.0) {
        Ok(())
    } else if !freeze_attempted || parsed.as_ref().is_some_and(|m| !m.effect_attempted) {
        Ok(())
    } else if !frozen {
        Err(anyhow::anyhow!("Unverified effect state is not frozen"))
    } else {
        // No completion means verify receives null and must prove FULL before state
        // from its retained durable operation intent, never guess from a prefix.
        let receipt = parsed
            .as_ref()
            .and_then(|m| m.result.as_ref())
            .unwrap_or(&Value::Null);
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            cx.checkpoint_frozen()?;
            let operation = cx.request.operation.clone();
            verify(&mut cx, &operation, receipt)?;
            cx.checkpoint_frozen()
        }))
        .unwrap_or_else(|_| Err(anyhow::anyhow!("Independent verifier failed")))
    };
    let verified = verify_result.is_ok();
    let mut final_message = if verified {
        parsed.unwrap_or(Finished {
            result: None,
            error: Some(
                "Transaction ended before completion; full consistent state independently verified"
                    .into(),
            ),
            effect_attempted: true,
            verified_intent: None,
        })
    } else {
        // Keep this state reference alive if closure cannot be confirmed. Never
        // unwind/drop it onto an unverified graph merely to let the UI continue.
        while emergency_close(&cx).is_err() {
            std::thread::sleep(Duration::from_millis(250));
        }
        Finished{result:None,error:Some("Structural transaction could not be verified. The owned game was closed without saving to protect the roster.".into()),effect_attempted:true,verified_intent:None}
    };
    final_message.verified_intent = if verified && (!no_effect || bookkeeping_verified) {
        cx.intent.as_ref().map(|i| i.sha256.clone())
    } else {
        None
    };
    cx.release_verified = true;
    frame(&mut std::io::stdout(), &final_message, MAX_RESPONSE)?;
    Ok(0)
}
pub(super) fn maybe_run_helper(
    execute_inside: ExecuteInside,
    verify_inside: VerifyInside,
) -> Option<i32> {
    match std::env::args().nth(1).as_deref() {
        Some("--owned-transaction-helper") => {
            Some(helper(execute_inside).unwrap_or_else(|error| {
                #[cfg(test)]
                eprintln!("helper: {error:#}");
                111
            }))
        }
        Some("--owned-transaction-watchdog") => {
            Some(watchdog(verify_inside).unwrap_or_else(|error| {
                #[cfg(test)]
                eprintln!("watchdog: {error:#}");
                112
            }))
        }
        _ => None,
    }
}

static VERIFIED: std::sync::Mutex<Vec<(Identity, String, String)>> =
    std::sync::Mutex::new(Vec::new());
fn remember_verified(game: &Identity, session: &str, sha: &str) -> Result<()> {
    ensure!(
        sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid private completion witness"
    );
    let mut values = VERIFIED
        .lock()
        .map_err(|_| anyhow::anyhow!("Completion witness store is held"))?;
    let value = (game.clone(), session.to_owned(), sha.to_ascii_lowercase());
    if !values.contains(&value) {
        ensure!(
            values.len() < 512,
            "Completion witness limit reached; close game before more structural edits"
        );
        values.push(value);
    }
    Ok(())
}
pub(super) fn verified_intent(game: &Identity, session: &str, sha: &str) -> bool {
    VERIFIED.lock().ok().is_some_and(|v| {
        v.iter()
            .any(|(g, s, h)| g == game && s == session && h == &sha.to_ascii_lowercase())
    })
}

pub(super) fn archive_intent(file: &fs::File, target: &Path) -> Result<()> {
    use windows::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{FILE_INFO_BY_HANDLE_CLASS, SetFileInformationByHandle},
    };
    ensure!(
        target.is_absolute() && !target.try_exists()? && file.metadata()?.is_file(),
        "Invalid structural journal archive target"
    );
    let name: Vec<u16> = target.as_os_str().encode_wide().collect();
    ensure!(
        !name.is_empty() && name.len() <= 32760,
        "Archive path exceeds bound"
    );
    let count = name.len().checked_mul(2).context("Archive name overflow")?;
    let size = 20 + count;
    let mut words = vec![0u64; (size + 7) / 8];
    let buffer =
        unsafe { std::slice::from_raw_parts_mut(words.as_mut_ptr().cast::<u8>(), words.len() * 8) };
    buffer[16..20].copy_from_slice(&(count as u32).to_le_bytes());
    for (i, v) in name.into_iter().enumerate() {
        buffer[20 + i * 2..22 + i * 2].copy_from_slice(&v.to_le_bytes());
    }
    unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FILE_INFO_BY_HANDLE_CLASS(3),
            buffer.as_ptr().cast(),
            size as u32,
        )
    }?;
    Ok(())
}
