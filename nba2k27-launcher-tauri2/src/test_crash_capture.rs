//! Diagnostic test executable only. Environment is supplied only by the
//! allowlisted nondebug supervisor. No production module or game access.
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

type Handle = isize;
#[repr(C)]
struct FileTime {
    low: u32,
    high: u32,
}
#[repr(C)]
struct Channel {
    signature: u64,
    version: u32,
    pid: u32,
    birth: u64,
    claimed: i32,
    tid: u32,
    pointers: u64,
}
// Exact shared tools/qa073-natural-crash-channel.hpp ABI; no Rust references
// to the mapped fields, which are read/written using volatile raw pointers.
const _: () = {
    assert!(std::mem::size_of::<Channel>() == 40);
    assert!(std::mem::offset_of!(Channel, signature) == 0);
    assert!(std::mem::offset_of!(Channel, version) == 8);
    assert!(std::mem::offset_of!(Channel, pid) == 12);
    assert!(std::mem::offset_of!(Channel, birth) == 16);
    assert!(std::mem::offset_of!(Channel, claimed) == 24);
    assert!(std::mem::offset_of!(Channel, tid) == 28);
    assert!(std::mem::offset_of!(Channel, pointers) == 32);
};
const MAGIC: u64 = 0x5141_3037_3343_5241;
static CHANNEL: AtomicPtr<Channel> = AtomicPtr::new(std::ptr::null_mut());
static INSTALLED: AtomicBool = AtomicBool::new(false);
// Assigned before CHANNEL release publication and never subsequently changed.
static mut REQUEST: Handle = 0;
static mut ACK: Handle = 0;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetEnvironmentVariableW(name: *const u16, value: *mut u16, capacity: u32) -> u32;
    fn GetCurrentProcess() -> Handle;
    fn GetCurrentProcessId() -> u32;
    fn GetCurrentThreadId() -> u32;
    fn GetProcessTimes(
        process: Handle,
        birth: *mut FileTime,
        exit: *mut FileTime,
        kernel: *mut FileTime,
        user: *mut FileTime,
    ) -> i32;
    fn MapViewOfFile(mapping: Handle, access: u32, high: u32, low: u32, size: usize)
    -> *mut c_void;
    fn UnmapViewOfFile(address: *const c_void) -> i32;
    fn WaitForSingleObject(handle: Handle, milliseconds: u32) -> u32;
    fn SetEvent(handle: Handle) -> i32;
    fn SetUnhandledExceptionFilter(
        filter: Option<unsafe extern "system" fn(*mut c_void) -> i32>,
    ) -> *mut c_void;
}

// No allocation, formatting, error conversion, panic, or Rust runtime dependency
// in the initializer or fatal filter. All Windows calls use raw stack arguments.
unsafe fn birth() -> u64 {
    let mut a = FileTime { low: 0, high: 0 };
    let mut b = FileTime { low: 0, high: 0 };
    let mut c = FileTime { low: 0, high: 0 };
    let mut d = FileTime { low: 0, high: 0 };
    if unsafe { GetProcessTimes(GetCurrentProcess(), &mut a, &mut b, &mut c, &mut d) } == 0 {
        return 0;
    }
    (u64::from(a.high) << 32) | u64::from(a.low)
}
unsafe fn env_handle(key: *const u16) -> Handle {
    let mut value = [0u16; 32];
    let n = unsafe { GetEnvironmentVariableW(key, value.as_mut_ptr(), 32) };
    if n == 0 || n >= 32 {
        return 0;
    }
    let mut number = 0usize;
    for i in 0..n {
        let c = unsafe { value.as_ptr().add(i as usize).read() };
        let digit = match c {
            48..=57 => usize::from(c - 48),
            97..=102 => usize::from(c - 97 + 10),
            _ => return 0,
        };
        if number > (usize::MAX - digit) / 16 {
            return 0;
        }
        number = number * 16 + digit;
    }
    number as Handle
}
unsafe fn matches(c: *mut Channel) -> bool {
    !c.is_null()
        && unsafe { std::ptr::addr_of!((*c).signature).read_volatile() } == MAGIC
        && unsafe { std::ptr::addr_of!((*c).version).read_volatile() } == 1
        && unsafe { std::ptr::addr_of!((*c).pid).read_volatile() }
            == unsafe { GetCurrentProcessId() }
        && unsafe { std::ptr::addr_of!((*c).birth).read_volatile() } != 0
        && unsafe { std::ptr::addr_of!((*c).birth).read_volatile() } == unsafe { birth() }
}
unsafe extern "system" fn fatal(pointers: *mut c_void) -> i32 {
    let c = CHANNEL.load(Ordering::Acquire);
    if !unsafe { matches(c) }
        || unsafe {
            std::sync::atomic::AtomicI32::from_ptr(std::ptr::addr_of_mut!((*c).claimed))
                .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
        }
    {
        return 0; // EXCEPTION_CONTINUE_SEARCH
    }
    unsafe {
        std::ptr::addr_of_mut!((*c).tid).write_volatile(GetCurrentThreadId());
        std::ptr::addr_of_mut!((*c).pointers).write_volatile(pointers as u64);
        std::sync::atomic::fence(Ordering::SeqCst);
        if SetEvent(REQUEST) != 0 {
            WaitForSingleObject(ACK, 20_000);
        }
    }
    0 // Preserve normal unhandled-exception termination; never resume the fault.
}
unsafe extern "C" fn initialize() {
    let mapping = unsafe { env_handle(windows::core::w!("QA073_CRASH_MAPPING").as_ptr()) };
    if mapping == 0 {
        return;
    }
    let ready = unsafe { env_handle(windows::core::w!("QA073_CRASH_READY").as_ptr()) };
    let request = unsafe { env_handle(windows::core::w!("QA073_CRASH_REQUEST").as_ptr()) };
    let ack = unsafe { env_handle(windows::core::w!("QA073_CRASH_ACK").as_ptr()) };
    if ready == 0 || request == 0 || ack == 0 {
        return;
    }
    let c = unsafe { MapViewOfFile(mapping, 6, 0, 0, 40) }.cast::<Channel>();
    if c.is_null() {
        return;
    }
    if unsafe { WaitForSingleObject(ready, 5_000) } != 0 || !unsafe { matches(c) } {
        unsafe {
            UnmapViewOfFile(c.cast());
        }
        return;
    }
    unsafe {
        REQUEST = request;
        ACK = ack;
    }
    CHANNEL.store(c, Ordering::Release);
    unsafe {
        SetUnhandledExceptionFilter(Some(fatal));
    }
    INSTALLED.store(true, Ordering::Release);
}
#[used]
#[unsafe(link_section = ".CRT$XCU")]
static INITIALIZE: unsafe extern "C" fn() = initialize;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "intentional synthetic fatal AV; requires exact supervised PID/birth channel"]
    fn qa074_synthetic_unhandled_av() {
        // Ordinary cargo test --ignored must never fault. A fresh, unconsumed,
        // exact installed supervisor channel is required immediately at effect.
        let c = CHANNEL.load(Ordering::Acquire);
        if !INSTALLED.load(Ordering::Acquire)
            || !unsafe { matches(c) }
            || unsafe { std::ptr::addr_of!((*c).claimed).read_volatile() } != 0
        {
            println!("QA074_SYNTHETIC_AV_REFUSED_NO_EXACT_SUPERVISOR_CHANNEL");
            return;
        }
        unsafe {
            std::ptr::write_volatile(std::hint::black_box(0xBAD000usize) as *mut u8, 1);
        }
        panic!("Synthetic hardware AV unexpectedly returned");
    }
}
