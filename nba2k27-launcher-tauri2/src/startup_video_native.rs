//! Read-only Win32 adapters for startup MP4 readiness and Xbox skip input.
//! Intentionally unwired: no game memory, focus/input synthesis, process
//! control, audio writes, game files, or UI dependency. Callers drive bounded
//! polling and pass samples to startup_video_policy with its generation token.
#![allow(dead_code)]
use crate::startup_video_policy::{self as policy, GameKey};
use anyhow::{Context, Result, ensure};
use std::{
    path::{Component, Path, PathBuf},
    time::Instant,
};

const MAX_WINDOWS: usize = 4096;
const SCAN_BUDGET_MS: u64 = 100;
const PAD_INTERVAL_MS: u64 = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExpectedGame {
    pub key: GameKey,
    pub exe: PathBuf,
}
impl ExpectedGame {
    pub fn new(key: GameKey, exe: &Path) -> Result<Self> {
        ensure!(
            key.pid > 0
                && key.birth > 0
                && exe.is_absolute()
                && !exe
                    .components()
                    .any(|c| matches!(c, Component::CurDir | Component::ParentDir))
                && exe
                    .file_name()
                    .is_some_and(|n| n.eq_ignore_ascii_case("NBA2K27.exe")),
            "Startup video requires the exact normalized game executable and birth identity"
        );
        let text = exe.to_str().context("Non-Unicode game executable path")?;
        ensure!(
            !text.contains('\0')
                && !text.starts_with(r"\\.\")
                && !text.to_ascii_lowercase().contains("globalroot"),
            "Unsupported game executable namespace"
        );
        Ok(Self {
            key,
            exe: exe.into(),
        })
    }
}
fn same_path(a: &Path, b: &Path) -> bool {
    fn normalize(path: &Path) -> String {
        let text = path.to_string_lossy();
        if let Some(text) = text.strip_prefix(r"\\?\UNC\") {
            format!(r"\\{text}")
        } else {
            text.strip_prefix(r"\\?\").unwrap_or(&text).into()
        }
    }
    // Matches the launcher identity convention. Unusual Unicode-only case
    // differences fail closed rather than broadening executable ownership.
    normalize(a).eq_ignore_ascii_case(&normalize(b))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WindowSample {
    pub hwnd: u64,
    pub pid: u32,
    pub thread: u32,
    pub visible: bool,
    pub minimized: bool,
    pub root: u64,
    pub owner: u64,
    pub client: [i32; 4],
    pub monitor: Bounds,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OwnedWindow {
    pub window: policy::Window,
    pub monitor: Bounds,
}
impl WindowSample {
    fn owned(&self, key: GameKey) -> Option<OwnedWindow> {
        let width = self.client[2].checked_sub(self.client[0])?;
        let height = self.client[3].checked_sub(self.client[1])?;
        if self.pid != key.pid
            || self.hwnd == 0
            || self.thread == 0
            || !self.visible
            || self.minimized
            || self.root != self.hwnd
            || self.owner != 0
            || !(640..=16384).contains(&width)
            || !(360..=16384).contains(&height)
            || !(640..=32768).contains(&self.monitor.width)
            || !(360..=32768).contains(&self.monitor.height)
            || self
                .monitor
                .x
                .checked_add_unsigned(self.monitor.width)
                .is_none()
            || self
                .monitor
                .y
                .checked_add_unsigned(self.monitor.height)
                .is_none()
        {
            return None;
        }
        Some(OwnedWindow {
            window: policy::Window {
                game: key,
                hwnd: self.hwnd,
                visible: true,
                minimized: false,
                top_level: true,
                width: width as u32,
                height: height as u32,
            },
            monitor: self.monitor,
        })
    }
}

/// Native implementation retains a query/synchronize-only process handle.
/// None means that exact retained process has exited, never "look up PID again".
pub(crate) trait WindowApi {
    type Process;
    fn retain(&mut self, pid: u32) -> Result<Self::Process>;
    fn process(&mut self, process: &Self::Process) -> Result<Option<ExpectedGame>>;
    fn now_ms(&self) -> u64;
    fn enumerate(&mut self, max: usize, budget_ms: u64) -> Result<Vec<u64>>;
    fn window(&mut self, hwnd: u64) -> Result<Option<WindowSample>>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Observation {
    Window(Option<OwnedWindow>),
    GameEnded,
}

pub(crate) struct OwnedGame<A: WindowApi = Win32Windows> {
    api: A,
    process: A::Process,
    expected: ExpectedGame,
}
impl<A: WindowApi> OwnedGame<A> {
    fn bind(mut api: A, expected: ExpectedGame) -> Result<Self> {
        ExpectedGame::new(expected.key, &expected.exe)?;
        let process = api.retain(expected.key.pid)?;
        let mut game = Self {
            api,
            process,
            expected,
        };
        ensure!(
            game.alive()?,
            "Startup-video game exited before window observation"
        );
        Ok(game)
    }
    pub fn key(&self) -> GameKey {
        self.expected.key
    }
    fn alive(&mut self) -> Result<bool> {
        let Some(actual) = self.api.process(&self.process)? else {
            return Ok(false);
        };
        ensure!(
            actual.key == self.expected.key && same_path(&actual.exe, &self.expected.exe),
            "Startup-video retained game identity/executable changed"
        );
        Ok(true)
    }
    fn within_budget(&self, start: u64) -> Result<()> {
        let elapsed = self
            .api
            .now_ms()
            .checked_sub(start)
            .context("Window observer clock moved backwards")?;
        ensure!(
            elapsed <= SCAN_BUDGET_MS,
            "Window enumeration exceeded its bounded scan budget"
        );
        Ok(())
    }
    pub fn observe(&mut self) -> Result<Observation> {
        if !self.alive()? {
            return Ok(Observation::GameEnded);
        }
        let start = self.api.now_ms();
        let handles = self.api.enumerate(MAX_WINDOWS, SCAN_BUDGET_MS)?;
        ensure!(
            handles.len() <= MAX_WINDOWS,
            "Window enumeration exceeded its handle budget"
        );
        self.within_budget(start)?;
        let mut found = None;
        let mut ambiguous = false;
        for hwnd in handles {
            self.within_budget(start)?;
            if let Some(sample) = self.api.window(hwnd)? {
                // The API sample must still describe the enumerated handle.
                if sample.hwnd != hwnd {
                    continue;
                }
                if sample.owned(self.key()).is_some() {
                    if found.is_some() {
                        ambiguous = true;
                    } else {
                        found = Some(sample);
                    }
                }
            }
        }
        self.within_budget(start)?;
        if !self.alive()? {
            return Ok(Observation::GameEnded);
        }
        if ambiguous {
            return Ok(Observation::Window(None));
        }
        let Some(first) = found else {
            return Ok(Observation::Window(None));
        };
        let second = self.api.window(first.hwnd)?;
        self.within_budget(start)?;
        if second.as_ref() != Some(&first) {
            return Ok(Observation::Window(None));
        }
        if !self.alive()? {
            return Ok(Observation::GameEnded);
        }
        Ok(Observation::Window(first.owned(self.key())))
    }
    /// Recheck this immediately before using an HWND for monitor placement or
    /// a separately authorized focus operation. This method itself is read-only.
    pub fn verify_window(&mut self, hwnd: u64) -> Result<Option<OwnedWindow>> {
        if !self.alive()? {
            return Ok(None);
        }
        let first = self.api.window(hwnd)?;
        let second = self.api.window(hwnd)?;
        if !self.alive()? || first != second {
            return Ok(None);
        }
        Ok(first
            .filter(|w| w.hwnd == hwnd)
            .and_then(|w| w.owned(self.key())))
    }
}

use windows::{
    Win32::{
        Foundation::{
            CloseHandle, FILETIME, FreeLibrary, HANDLE, HMODULE, HWND, LPARAM, RECT, WAIT_OBJECT_0,
            WAIT_TIMEOUT,
        },
        Graphics::Gdi::{
            GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
        },
        System::{
            LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW},
            Threading::*,
        },
        UI::WindowsAndMessaging::*,
    },
    core::{BOOL, PCSTR, PCWSTR, PWSTR},
};
pub(crate) struct NativeProcess(HANDLE);
impl Drop for NativeProcess {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
pub(crate) struct Win32Windows {
    clock: Instant,
}
impl OwnedGame<Win32Windows> {
    pub fn retain(expected: ExpectedGame) -> Result<Self> {
        Self::bind(
            Win32Windows {
                clock: Instant::now(),
            },
            expected,
        )
    }
}
impl WindowApi for Win32Windows {
    type Process = NativeProcess;
    fn retain(&mut self, pid: u32) -> Result<NativeProcess> {
        Ok(NativeProcess(unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                false,
                pid,
            )?
        }))
    }
    fn process(&mut self, process: &NativeProcess) -> Result<Option<ExpectedGame>> {
        match unsafe { WaitForSingleObject(process.0, 0) } {
            WAIT_OBJECT_0 => return Ok(None),
            WAIT_TIMEOUT => {}
            _ => anyhow::bail!("Startup-video retained process presence is unknown"),
        }
        let pid = unsafe { GetProcessId(process.0) };
        ensure!(pid > 0, "Startup-video retained process ID is unavailable");
        let (mut created, mut exit, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        unsafe {
            GetProcessTimes(process.0, &mut created, &mut exit, &mut kernel, &mut user)?;
        }
        let birth = ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64;
        let mut path = vec![0u16; 32768];
        let mut len = path.len() as u32;
        unsafe {
            QueryFullProcessImageNameW(
                process.0,
                PROCESS_NAME_WIN32,
                PWSTR(path.as_mut_ptr()),
                &mut len,
            )?;
        }
        ensure!(
            len > 0 && (len as usize) < path.len(),
            "Startup-video process executable exceeds its path budget"
        );
        let expected = ExpectedGame::new(
            GameKey { pid, birth },
            Path::new(&String::from_utf16(&path[..len as usize])?),
        )?;
        match unsafe { WaitForSingleObject(process.0, 0) } {
            WAIT_OBJECT_0 => Ok(None),
            WAIT_TIMEOUT => Ok(Some(expected)),
            _ => anyhow::bail!("Startup-video retained process presence became unknown"),
        }
    }
    fn now_ms(&self) -> u64 {
        self.clock.elapsed().as_millis().min(u64::MAX as u128) as u64
    }
    fn enumerate(&mut self, max: usize, budget_ms: u64) -> Result<Vec<u64>> {
        struct Scan {
            handles: Vec<u64>,
            max: usize,
            budget_ms: u64,
            clock: Instant,
            stopped: bool,
        }
        unsafe extern "system" fn visit(hwnd: HWND, param: LPARAM) -> BOOL {
            let scan = unsafe { &mut *(param.0 as *mut Scan) };
            if scan.handles.len() >= scan.max
                || scan.clock.elapsed().as_millis() > scan.budget_ms as u128
            {
                scan.stopped = true;
                return BOOL(0);
            }
            scan.handles.push(hwnd.0 as u64);
            BOOL(1)
        }
        ensure!(
            max <= MAX_WINDOWS && max > 0 && budget_ms <= SCAN_BUDGET_MS,
            "Invalid native window budget"
        );
        let mut scan = Scan {
            handles: Vec::new(),
            max,
            budget_ms,
            clock: Instant::now(),
            stopped: false,
        };
        let result = unsafe { EnumWindows(Some(visit), LPARAM(&mut scan as *mut _ as isize)) };
        ensure!(
            !scan.stopped,
            "Native window enumeration reached its handle/time budget"
        );
        result?;
        Ok(scan.handles)
    }
    fn window(&mut self, hwnd: u64) -> Result<Option<WindowSample>> {
        if hwnd == 0 {
            return Ok(None);
        }
        let native = HWND(hwnd as *mut _);
        if !unsafe { IsWindow(Some(native)) }.as_bool() {
            return Ok(None);
        }
        let mut pid = 0;
        let thread = unsafe { GetWindowThreadProcessId(native, Some(&mut pid)) };
        if pid == 0 || thread == 0 {
            return Ok(None);
        }
        let mut rect = RECT::default();
        if unsafe { GetClientRect(native, &mut rect) }.is_err() {
            return Ok(None);
        }
        let monitor = unsafe { MonitorFromWindow(native, MONITOR_DEFAULTTONEAREST) };
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if monitor.0.is_null() || !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
            return Ok(None);
        }
        let Some(width) = info.rcMonitor.right.checked_sub(info.rcMonitor.left) else {
            return Ok(None);
        };
        let Some(height) = info.rcMonitor.bottom.checked_sub(info.rcMonitor.top) else {
            return Ok(None);
        };
        if width <= 0 || height <= 0 {
            return Ok(None);
        }
        let sample = WindowSample {
            hwnd,
            pid,
            thread,
            visible: unsafe { IsWindowVisible(native) }.as_bool(),
            minimized: unsafe { IsIconic(native) }.as_bool(),
            root: unsafe { GetAncestor(native, GA_ROOT) }.0 as u64,
            // Unlike GetWindow's projected Result, a null owner is normal here.
            owner: unsafe { GetWindowLongPtrW(native, GWLP_HWNDPARENT) } as u64,
            client: [rect.left, rect.top, rect.right, rect.bottom],
            monitor: Bounds {
                x: info.rcMonitor.left,
                y: info.rcMonitor.top,
                width: width as u32,
                height: height as u32,
            },
        };
        let mut after_pid = 0;
        let after_thread = unsafe { GetWindowThreadProcessId(native, Some(&mut after_pid)) };
        if !unsafe { IsWindow(Some(native)) }.as_bool()
            || after_pid != pid
            || after_thread != thread
        {
            return Ok(None);
        }
        Ok(Some(sample))
    }
}

pub(crate) trait PadApi {
    fn read(&mut self, slot: u32) -> Result<Option<u16>>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PadSample {
    pub slot: usize,
    pub connected: bool,
    pub buttons: u16,
}
impl PadSample {
    pub fn event(self) -> policy::Event {
        policy::Event::Controller {
            slot: self.slot,
            connected: self.connected,
            buttons: self.buttons,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PadPoll {
    pub samples: [PadSample; 4],
    pub unavailable_mask: u8,
}
pub(crate) struct ControllerObserver<A: PadApi = Win32Pads> {
    api: A,
    started: u64,
    last_now: u64,
    last_poll: Option<u64>,
    active: bool,
}
impl<A: PadApi> ControllerObserver<A> {
    fn with_api(api: A, now: u64) -> Self {
        Self {
            api,
            started: now,
            last_now: now,
            last_poll: None,
            active: true,
        }
    }
    pub fn stop(&mut self) {
        self.active = false;
    }
    pub fn active(&self) -> bool {
        self.active
    }
    pub fn poll(&mut self, now: u64) -> Result<Option<PadPoll>> {
        if !self.active {
            return Ok(None);
        }
        ensure!(
            now >= self.last_now,
            "Controller observer requires a monotonic clock"
        );
        self.last_now = now;
        if now - self.started >= policy::PLAYBACK_TIMEOUT_MS {
            self.stop();
            return Ok(None);
        }
        if self
            .last_poll
            .is_some_and(|prior| now - prior < PAD_INTERVAL_MS)
        {
            return Ok(None);
        }
        self.last_poll = Some(now);
        let mut unavailable_mask = 0;
        let samples = std::array::from_fn(|slot| {
            let buttons = match self.api.read(slot as u32) {
                Ok(value) => value,
                Err(_) => {
                    unavailable_mask |= 1 << slot;
                    None
                }
            };
            PadSample {
                slot,
                connected: buttons.is_some(),
                buttons: buttons.unwrap_or(0),
            }
        });
        Ok(Some(PadPoll {
            samples,
            unavailable_mask,
        }))
    }
}

#[repr(C)]
#[derive(Default)]
struct XinputPad {
    buttons: u16,
    left_trigger: u8,
    right_trigger: u8,
    lx: i16,
    ly: i16,
    rx: i16,
    ry: i16,
}
#[repr(C)]
#[derive(Default)]
struct XinputState {
    packet: u32,
    pad: XinputPad,
}
type GetState = unsafe extern "system" fn(u32, *mut XinputState) -> u32;
struct Module(HMODULE);
impl Drop for Module {
    fn drop(&mut self) {
        unsafe {
            let _ = FreeLibrary(self.0);
        }
    }
}
pub(crate) struct Win32Pads {
    _module: Module,
    get_state: GetState,
}
impl Win32Pads {
    pub fn load() -> Result<Self> {
        // System32-only search prevents a portable-folder/CWD xinput DLL from
        // being loaded. Retain that DLL for the full function-pointer lifetime.
        for name in ["xinput1_4.dll", "xinput9_1_0.dll"] {
            let name: Vec<_> = name.encode_utf16().chain(Some(0)).collect();
            if let Ok(module) =
                unsafe { LoadLibraryExW(PCWSTR(name.as_ptr()), None, LOAD_LIBRARY_SEARCH_SYSTEM32) }
            {
                let module = Module(module);
                if let Some(address) =
                    unsafe { GetProcAddress(module.0, PCSTR(c"XInputGetState".as_ptr() as _)) }
                {
                    let get_state = unsafe {
                        std::mem::transmute::<unsafe extern "system" fn() -> isize, GetState>(
                            address,
                        )
                    };
                    return Ok(Self {
                        _module: module,
                        get_state,
                    });
                }
            }
        }
        anyhow::bail!(
            "Readonly XInput observer is unavailable; mouse/keyboard skip remains available"
        )
    }
}
impl PadApi for Win32Pads {
    fn read(&mut self, slot: u32) -> Result<Option<u16>> {
        ensure!(
            slot < 4,
            "Xbox controller slot exceeds the bounded observer"
        );
        let mut state = XinputState::default();
        let status = unsafe { (self.get_state)(slot, &mut state) };
        match status {
            0 => Ok(Some(state.pad.buttons)),
            1167 => Ok(None),
            _ => anyhow::bail!("Readonly XInput slot {slot} returned status {status}"),
        }
    }
}
impl ControllerObserver<Win32Pads> {
    pub fn load(now: u64) -> Result<Self> {
        Ok(Self::with_api(Win32Pads::load()?, now))
    }
}

#[cfg(test)]
#[path = "startup_video_native_tests.rs"]
mod tests;
