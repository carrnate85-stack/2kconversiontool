//! Cooperative cross-process serialization for native writers. This is not
//! memory authority: every caller still needs its own identity/journal guards.
use anyhow::{Result, ensure};
use std::sync::{
    Mutex, MutexGuard, OnceLock,
    atomic::{AtomicBool, Ordering},
};
type Raw = isize;
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateMutexW(a: *const core::ffi::c_void, owned: i32, name: *const u16) -> Raw;
    fn CreateEventW(
        a: *const core::ffi::c_void,
        manual: i32,
        initial: i32,
        name: *const u16,
    ) -> Raw;
    fn WaitForSingleObject(h: Raw, ms: u32) -> u32;
    fn ReleaseMutex(h: Raw) -> i32;
    fn SetEvent(h: Raw) -> i32;
    fn CloseHandle(h: Raw) -> i32;
}
const WAIT_MS: u32 = 60_000;
static LOCAL: Mutex<()> = Mutex::new(());
static FAILED: AtomicBool = AtomicBool::new(false);
struct Kernel {
    mutex: Raw,
    poison: Raw,
}
impl Drop for Kernel {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.poison);
            CloseHandle(self.mutex);
        }
    }
}
static KERNEL: OnceLock<Result<Kernel, String>> = OnceLock::new();
fn names() -> Result<(Vec<u16>, Vec<u16>)> {
    let base = if cfg!(test) {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        std::env::current_exe()?.hash(&mut h);
        format!("Local\\NBA2K27.NativeWrite.Test.{:016X}", h.finish())
    } else {
        "Local\\NBA2K27.NativeWrite.v1".into()
    };
    Ok((
        format!("{base}.Mutex\0").encode_utf16().collect(),
        format!("{base}.Abandoned\0").encode_utf16().collect(),
    ))
}
fn kernel() -> Result<&'static Kernel> {
    KERNEL
        .get_or_init(|| {
            let (mutex, poison) = names().map_err(|e| e.to_string())?;
            let m = unsafe { CreateMutexW(std::ptr::null(), 0, mutex.as_ptr()) };
            if m == 0 {
                return Err(format!(
                    "Native write mutex unavailable: {}",
                    std::io::Error::last_os_error()
                ));
            }
            let e = unsafe { CreateEventW(std::ptr::null(), 1, 0, poison.as_ptr()) };
            if e == 0 {
                unsafe {
                    CloseHandle(m);
                }
                return Err(format!(
                    "Native write safety latch unavailable: {}",
                    std::io::Error::last_os_error()
                ));
            }
            Ok(Kernel {
                mutex: m,
                poison: e,
            })
        })
        .as_ref()
        .map_err(|e| anyhow::anyhow!("{e}"))
}
fn poison(k: &Kernel) {
    FAILED.store(true, Ordering::Release);
    unsafe {
        SetEvent(k.poison);
    }
}
fn healthy(k: &Kernel) -> Result<()> {
    ensure!(
        !FAILED.load(Ordering::Acquire) && unsafe { WaitForSingleObject(k.poison, 0) } == 258,
        "A native writer ended unexpectedly. Close NBA 2K27 and restart all launcher/provider processes before another write"
    );
    Ok(())
}
/// Private fields and !Send local guard prove actual ownership, not a caller
/// permit or an unlocked old std::MutexGuard. Retained through helper exits.
pub(crate) struct WriteGuard {
    local: MutexGuard<'static, ()>,
    kernel: &'static Kernel,
}
impl Drop for WriteGuard {
    fn drop(&mut self) {
        if std::thread::panicking() {
            poison(self.kernel);
        }
        if unsafe { ReleaseMutex(self.kernel.mutex) } == 0 {
            poison(self.kernel);
        }
    }
}
fn acquire(local: MutexGuard<'static, ()>, timeout: u32) -> Result<Option<WriteGuard>> {
    let k = kernel()?;
    healthy(k)?;
    match unsafe { WaitForSingleObject(k.mutex, timeout) } {
        0 => {
            if let Err(e) = healthy(k) {
                if unsafe { ReleaseMutex(k.mutex) } == 0 {
                    poison(k);
                }
                return Err(e);
            }
            Ok(Some(WriteGuard { local, kernel: k }))
        }
        0x80 => {
            poison(k);
            let released = unsafe { ReleaseMutex(k.mutex) };
            ensure!(
                released != 0,
                "Abandoned native mutex release failed; restart all launcher/provider processes"
            );
            anyhow::bail!(
                "A native writer was abandoned; no new forward writes are allowed. Close NBA 2K27 and restart all launcher/provider processes"
            )
        }
        258 if timeout == 0 => Ok(None),
        258 => anyhow::bail!(
            "Another native transaction did not finish within60seconds; no write was started"
        ),
        _ => anyhow::bail!(
            "Native transaction wait failed: {}",
            std::io::Error::last_os_error()
        ),
    }
}
pub(crate) fn lock() -> Result<WriteGuard> {
    let start = std::time::Instant::now();
    let local = loop {
        match LOCAL.try_lock() {
            Ok(g) => break g,
            Err(std::sync::TryLockError::Poisoned(_)) => {
                anyhow::bail!("Native local transaction gate failed; restart the launcher")
            }
            Err(std::sync::TryLockError::WouldBlock) => {
                ensure!(
                    start.elapsed().as_millis() < WAIT_MS as u128,
                    "Another local native transaction did not finish within60seconds"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    };
    let left = WAIT_MS.saturating_sub(start.elapsed().as_millis().min(WAIT_MS as u128) as u32);
    acquire(local, left)?
        .ok_or_else(|| anyhow::anyhow!("Native transaction gate timedout; no write was started"))
}
pub(crate) fn try_lock() -> Result<Option<WriteGuard>> {
    let local = match LOCAL.try_lock() {
        Ok(g) => g,
        Err(std::sync::TryLockError::WouldBlock) => return Ok(None),
        Err(_) => anyhow::bail!("Native local transaction gate failed; restart the launcher"),
    };
    acquire(local, 0)
}
