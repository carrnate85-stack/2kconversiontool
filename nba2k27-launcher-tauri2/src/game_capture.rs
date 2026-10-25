//! User-requested game hang capture (WPF "Capture game hang" / Ctrl+Shift+F12).
//!
//! Read-only toward the game: the exact configured NBA2K27.exe is opened with
//! PROCESS_QUERY_INFORMATION | PROCESS_VM_READ only (no write, VM operation,
//! thread creation or injection) and Windows' dbghelp writes a small minidump.
//! Output stays in the portable Logs/game-captures folder. Snapshots may hold
//! private memory, so dumps are never added to support bundles; the JSON
//! state files beside them are path/credential redacted like support bundles.
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::{
    fs,
    os::windows::io::AsRawHandle,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, FILETIME, HANDLE, HWND, LPARAM},
        System::{Diagnostics::ToolHelp::*, Threading::*},
        UI::WindowsAndMessaging::{
            EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsHungAppWindow, IsWindowVisible,
        },
    },
    core::{BOOL, PWSTR},
};

pub(crate) const SCHEMA: &str = "2k-game-capture-v1";
const GAME_EXE: &str = "NBA2K27.exe";
const MAX_DUMPS: u32 = 3;
const MAX_DUMP_BYTES: u64 = 64 * 1024 * 1024;
const KEEP_SESSIONS: usize = 3;
// MiniDumpWithUnloadedModules | MiniDumpWithThreadInfo: stacks, modules and
// thread state for a stuck loading screen without full process memory.
const DUMP_TYPE: u32 = 0x0000_0020 | 0x0000_1000;
const PRIVACY: &str = "Local memory snapshot; may contain private data. Not included in support exports. No debugger attachment, injection or automatic upload.";

static CAPTURE: Mutex<()> = Mutex::new(());

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub(crate) fn captures_root(root: &Path) -> PathBuf {
    root.join("Logs").join("game-captures")
}

fn normalized(path: &Path) -> String {
    let text = path.to_string_lossy();
    let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
    text.replace('/', "\\").to_lowercase()
}

fn image_path(process: HANDLE) -> Result<PathBuf> {
    let mut buffer = vec![0u16; 32768];
    let mut len = buffer.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut len,
        )?;
    }
    Ok(PathBuf::from(String::from_utf16(&buffer[..len as usize])?))
}

fn game_pids() -> Result<Vec<u32>> {
    let snapshot = Handle(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)? });
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut pids = Vec::new();
    let mut more = unsafe { Process32FirstW(snapshot.0, &mut entry) }.is_ok();
    while more {
        let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(260);
        if String::from_utf16_lossy(&entry.szExeFile[..len]).eq_ignore_ascii_case(GAME_EXE) {
            pids.push(entry.th32ProcessID);
        }
        more = unsafe { Process32NextW(snapshot.0, &mut entry) }.is_ok();
    }
    Ok(pids)
}

fn filetime_ms(time: FILETIME) -> u64 {
    let ticks = ((time.dwHighDateTime as u64) << 32) | time.dwLowDateTime as u64;
    ticks.saturating_sub(116_444_736_000_000_000) / 10_000
}

fn duration_ms(time: FILETIME) -> u64 {
    (((time.dwHighDateTime as u64) << 32) | time.dwLowDateTime as u64) / 10_000
}

struct Game {
    handle: Handle,
    pid: u32,
    started_ms: u64,
    kernel_ms: u64,
    user_ms: u64,
}

/// Opens the one running process whose image is exactly `<game_root>\NBA2K27.exe`.
fn open_game(game_root: &Path) -> Result<Game> {
    let expected = game_root
        .join(GAME_EXE)
        .canonicalize()
        .context("NBA2K27.exe was not found in the configured game folder")?;
    let expected = normalized(&expected);
    let mut found = Vec::new();
    for pid in game_pids()? {
        let Ok(query) =
            (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }).map(Handle)
        else {
            continue;
        };
        if image_path(query.0).is_ok_and(|path| normalized(&path) == expected) {
            found.push(pid);
        }
    }
    ensure!(
        !found.is_empty(),
        "Start NBA 2K27 first; a running game from the configured folder is required for a snapshot"
    );
    ensure!(
        found.len() == 1,
        "More than one matching NBA 2K27 process is running; no snapshot was taken"
    );
    let pid = found[0];
    // Read-only rights only: no PROCESS_VM_WRITE / VM_OPERATION / CREATE_THREAD.
    let handle = Handle(unsafe {
        OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid)
            .context("Windows refused read-only access to the game process")?
    });
    // Re-verify identity on the handle we will dump (PID reuse guard).
    ensure!(
        normalized(&image_path(handle.0)?) == expected,
        "The game process changed before the snapshot"
    );
    let (mut created, mut exited, mut kernel, mut user) = Default::default();
    unsafe { GetProcessTimes(handle.0, &mut created, &mut exited, &mut kernel, &mut user)? };
    Ok(Game {
        handle,
        pid,
        started_ms: filetime_ms(created),
        kernel_ms: duration_ms(kernel),
        user_ms: duration_ms(user),
    })
}

struct WindowScan {
    pid: u32,
    rows: Vec<(String, bool)>,
}

unsafe extern "system" fn window_row(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let scan = unsafe { &mut *(lparam.0 as *mut WindowScan) };
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == scan.pid && unsafe { IsWindowVisible(hwnd) }.as_bool() && scan.rows.len() < 16 {
        let mut title = [0u16; 256];
        let len = unsafe { GetWindowTextW(hwnd, &mut title) }.max(0) as usize;
        let hung = unsafe { IsHungAppWindow(hwnd) }.as_bool();
        scan.rows
            .push((String::from_utf16_lossy(&title[..len.min(256)]), hung));
    }
    BOOL(1)
}

fn game_windows(pid: u32) -> Vec<(String, bool)> {
    let mut scan = WindowScan {
        pid,
        rows: Vec::new(),
    };
    unsafe {
        let _ = EnumWindows(Some(window_row), LPARAM(&mut scan as *mut _ as isize));
    }
    scan.rows
}

fn session_name(started_ms: u64, pid: u32) -> String {
    format!("session-{started_ms:013}-{pid}")
}

fn valid_session(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("session-") else {
        return false;
    };
    let mut parts = rest.splitn(2, '-');
    let (Some(ms), Some(pid)) = (parts.next(), parts.next()) else {
        return false;
    };
    ms.len() == 13
        && ms.bytes().all(|b| b.is_ascii_digit())
        && !pid.is_empty()
        && pid.len() <= 10
        && pid.bytes().all(|b| b.is_ascii_digit())
}

fn owned_file(name: &str) -> bool {
    if name == "capture.json" {
        return true;
    }
    let slot = |prefix: &str, suffixes: &[&str]| {
        name.strip_prefix(prefix).is_some_and(|rest| {
            suffixes.iter().any(|suffix| {
                rest.strip_suffix(suffix)
                    .is_some_and(|n| matches!(n, "1" | "2" | "3"))
            })
        })
    };
    slot("hang-", &[".dmp", ".partial"]) || slot("state-", &[".json"])
}

fn plain_dir(path: &Path) -> Result<bool> {
    use std::os::windows::fs::MetadataExt;
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            ensure!(
                meta.is_dir() && meta.file_attributes() & 0x400 == 0,
                "Capture folder is not a plain directory"
            );
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// Keeps the newest `keep` capture sessions. Older folders are removed only
/// when they carry this schema and contain nothing but capture-owned files.
pub(crate) fn retain_sessions(root: &Path, keep: usize) -> Result<()> {
    if !plain_dir(root)? {
        return Ok(());
    }
    let mut sessions = fs::read_dir(root)?
        .flatten()
        .filter_map(|row| row.file_name().to_str().map(str::to_owned))
        .filter(|name| valid_session(name))
        .collect::<Vec<_>>();
    sessions.sort();
    sessions.reverse();
    for name in sessions.into_iter().skip(keep) {
        let folder = root.join(&name);
        if !plain_dir(&folder).unwrap_or(false) {
            continue;
        }
        let marker = fs::read_to_string(folder.join("capture.json"))
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok());
        if marker.as_ref().and_then(|m| m["schema"].as_str()) != Some(SCHEMA) {
            continue;
        }
        let children = fs::read_dir(&folder)?.collect::<std::io::Result<Vec<_>>>()?;
        let owned = children.iter().all(|child| {
            fs::symlink_metadata(child.path()).is_ok_and(|m| m.is_file())
                && child.file_name().to_str().is_some_and(owned_file)
        });
        if !owned {
            continue;
        }
        for child in children {
            fs::remove_file(child.path())?;
        }
        fs::remove_dir(&folder)?;
    }
    Ok(())
}

/// First unused snapshot slot (1..=3). A leftover .partial counts as used.
fn next_slot(folder: &Path) -> Option<u32> {
    (1..=MAX_DUMPS).find(|n| {
        !folder.join(format!("hang-{n}.dmp")).exists()
            && !folder.join(format!("hang-{n}.partial")).exists()
    })
}

fn redacted(mut value: Value, root: &Path, game_root: &Path) -> Value {
    let portable = root.to_string_lossy().into_owned();
    let game = game_root.to_string_lossy().into_owned();
    crate::support_bundle::redact_value(
        &mut value,
        &[
            (portable.as_str(), "<portable-root>"),
            (game.as_str(), "<game-root>"),
        ],
    );
    value
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(&serde_json::to_vec_pretty(value)?)?;
    file.sync_all()?;
    Ok(())
}

type MiniDumpWriteDump = unsafe extern "system" fn(
    HANDLE,
    u32,
    HANDLE,
    u32,
    *const core::ffi::c_void,
    *const core::ffi::c_void,
    *const core::ffi::c_void,
) -> BOOL;

fn write_dump(game: &Game, partial: &Path) -> Result<u64> {
    const LOAD_LIBRARY_SEARCH_SYSTEM32: u32 = 0x0000_0800;
    // System32 only, never the launcher/game folder (DLL planting).
    let library = unsafe {
        libloading::os::windows::Library::load_with_flags(
            "dbghelp.dll",
            LOAD_LIBRARY_SEARCH_SYSTEM32,
        )
    }
    .context("Windows dbghelp.dll is unavailable")?;
    let dump: libloading::os::windows::Symbol<MiniDumpWriteDump> =
        unsafe { library.get(b"MiniDumpWriteDump\0") }
            .context("MiniDumpWriteDump is unavailable")?;
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(partial)?;
    let ok = unsafe {
        dump(
            game.handle.0,
            game.pid,
            HANDLE(file.as_raw_handle()),
            DUMP_TYPE,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if !ok.as_bool() {
        let error = std::io::Error::last_os_error();
        drop(file);
        let _ = fs::remove_file(partial);
        bail!("Game snapshot unavailable ({error})");
    }
    file.sync_all()?;
    let bytes = file.metadata()?.len();
    drop(file);
    if bytes > MAX_DUMP_BYTES {
        let _ = fs::remove_file(partial);
        bail!("Game snapshot exceeded the 64 MB limit and was discarded");
    }
    Ok(bytes)
}

fn state_document(
    index: u32,
    game: &Game,
    windows: &[(String, bool)],
    dump: std::result::Result<u64, String>,
    version: &str,
) -> Value {
    json!({
        "schema": SCHEMA,
        "launcherVersion": version,
        "capturedUnixMs": SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0),
        "snapshot": index,
        "pid": game.pid,
        "processStartedUnixMs": game.started_ms,
        "cpuMs": {"kernel": game.kernel_ms, "user": game.user_ms},
        "windows": windows.iter().map(|(title, hung)| json!({"title": title, "hung": hung})).collect::<Vec<_>>(),
        "windowHung": windows.iter().any(|(_, hung)| *hung),
        "dump": match dump {
            Ok(bytes) => json!({"file": format!("hang-{index}.dmp"), "bytes": bytes}),
            Err(error) => json!({"error": error}),
        },
    })
}

/// Captures one read-only snapshot of the running configured game.
pub(crate) fn capture(root: &Path, game_root: &Path, version: &str) -> Result<Value> {
    let _gate = CAPTURE
        .try_lock()
        .map_err(|_| anyhow::anyhow!("A game snapshot is already in progress"))?;
    let game = open_game(game_root)?;
    let captures = captures_root(root);
    plain_dir(&captures)?;
    fs::create_dir_all(&captures)?;
    let folder = captures.join(session_name(game.started_ms, game.pid));
    if !plain_dir(&folder)? {
        fs::create_dir(&folder)?;
        write_json(
            &folder.join("capture.json"),
            &redacted(
                json!({
                    "schema": SCHEMA,
                    "createdUnixMs": SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64,
                    "game": game_root.join(GAME_EXE),
                    "maxDumps": MAX_DUMPS,
                    "maxDumpBytes": MAX_DUMP_BYTES,
                    "privacy": PRIVACY,
                }),
                root,
                game_root,
            ),
        )?;
    }
    let _ = retain_sessions(&captures, KEEP_SESSIONS);
    let index = next_slot(&folder)
        .context("This game session already has three snapshots; restart the game for more")?;
    let windows = game_windows(game.pid);
    let partial = folder.join(format!("hang-{index}.partial"));
    let dump = write_dump(&game, &partial);
    let file = folder.join(format!("hang-{index}.dmp"));
    let dump = dump.and_then(|bytes| {
        fs::rename(&partial, &file)?;
        Ok(bytes)
    });
    let state = redacted(
        state_document(
            index,
            &game,
            &windows,
            dump.as_ref().map(|b| *b).map_err(|e| format!("{e:#}")),
            version,
        ),
        root,
        game_root,
    );
    write_json(&folder.join(format!("state-{index}.json")), &state)?;
    let bytes = dump?;
    Ok(json!({
        "folder": folder,
        "file": file,
        "snapshot": index,
        "bytes": bytes,
        "pid": game.pid,
        "windowHung": state["windowHung"],
    }))
}

/// Opens a launcher-owned folder in Explorer.
pub(crate) fn shell_open(hwnd: HWND, path: &Path) -> Result<()> {
    use windows::{
        Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
        core::{PCWSTR, w},
    };
    let wide: Vec<u16> = path
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let result = unsafe {
        ShellExecuteW(
            Some(hwnd),
            w!("open"),
            PCWSTR(wide.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    ensure!(result.0 as isize > 32, "Windows could not open this folder");
    Ok(())
}

// ---------- Ctrl+Shift+F12 ----------
//
// WPF registered Ctrl+Shift+F12 with RegisterHotKey on its main window at Play
// (NativeWindow.cs:335, main.js:1813). Games can suppress application hotkeys
// while they are foreground (raw input with RIDEV_NOHOTKEYS), and another app
// may already own the chord, so Rust also polls the physical key state while
// the game window is foreground: the same mechanism WPF's in-game F8 watcher
// used (GetAsyncKeyState every 20 ms, foreground = game). One press fires once.

const HOTKEY_ID: i32 = 0x2B27;
const POLL_MS: u32 = 20;
const IDLE_MS: u32 = 1500;
const DEBOUNCE_MS: u64 = 1500;

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HotkeyState {
    /// off | waiting-for-game | registered | held
    pub phase: &'static str,
    pub message: String,
    pub registered: bool,
    pub error_code: Option<u32>,
    pub polling: bool,
    pub presses: u64,
    pub last_press_ms: Option<u64>,
    pub last_source: Option<&'static str>,
}
static HOTKEY: Mutex<Option<HotkeyState>> = Mutex::new(None);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
fn hotkey_view(running: bool, registered: bool, error: Option<u32>) -> (&'static str, String) {
    match (running, registered, error) {
        (false, _, _) => (
            "waiting-for-game",
            "Ctrl+Shift+F12 turns on when NBA2K27.exe is running.".into(),
        ),
        (true, true, _) => (
            "registered",
            "Ctrl+Shift+F12 is ready while the game runs.".into(),
        ),
        (true, false, Some(1409)) => (
            "held",
            "Another app already uses Ctrl+Shift+F12. The launcher still watches the keys while the game is in front; Capture game hang always works.".into(),
        ),
        (true, false, code) => (
            "held",
            format!(
                "Windows refused Ctrl+Shift+F12{}. The launcher still watches the keys while the game is in front; Capture game hang always works.",
                code.map(|c| format!(" (error {c})")).unwrap_or_default()
            ),
        ),
    }
}
/// Current registration state for the Settings page.
pub(crate) fn hotkey_status() -> Value {
    let state = HOTKEY
        .lock()
        .ok()
        .and_then(|s| s.clone())
        .unwrap_or(HotkeyState {
            phase: "off",
            message: "Ctrl+Shift+F12 is not active in this launcher window.".into(),
            registered: false,
            error_code: None,
            polling: false,
            presses: 0,
            last_press_ms: None,
            last_source: None,
        });
    json!(state)
}
fn publish(update: impl FnOnce(&mut HotkeyState)) {
    if let Ok(mut s) = HOTKEY.lock() {
        let mut next = s.clone().unwrap_or(HotkeyState {
            phase: "waiting-for-game",
            message: String::new(),
            registered: false,
            error_code: None,
            polling: false,
            presses: 0,
            last_press_ms: None,
            last_source: None,
        });
        update(&mut next);
        *s = Some(next);
    }
}

/// Edge detection plus de-duplication across the two press sources.
#[derive(Default)]
pub(crate) struct Presses {
    chord_down: bool,
    last_fire_ms: Option<u64>,
}
impl Presses {
    /// A key-state sample; true on the down edge of the full chord.
    pub fn sample(&mut self, chord_down: bool, now: u64) -> bool {
        let edge = chord_down && !self.chord_down;
        self.chord_down = chord_down;
        edge && self.fire(now)
    }
    /// One press from any source; false when it repeats the last press.
    pub fn fire(&mut self, now: u64) -> bool {
        if self
            .last_fire_ms
            .is_some_and(|last| now.saturating_sub(last) < DEBOUNCE_MS)
        {
            return false;
        }
        self.last_fire_ms = Some(now);
        true
    }
}

fn log_line(root: &Path, line: &str) {
    use std::io::Write;
    let dir = root.join("Logs");
    let path = dir.join("game-capture-hotkey.log");
    let _ = fs::create_dir_all(&dir);
    if fs::metadata(&path).is_ok_and(|m| m.len() > 64 * 1024) {
        let _ = fs::remove_file(&path);
    }
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{} {line}", now_ms());
    }
}

fn game_foreground(pids: &[u32]) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return false;
    }
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid != 0 && pids.contains(&pid)
}
fn chord_down() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_F12, VK_SHIFT,
    };
    let down = |vk: u16| unsafe { GetAsyncKeyState(vk as i32) } as u16 & 0x8000 != 0;
    down(VK_CONTROL.0) && down(VK_SHIFT.0) && down(VK_F12.0)
}

/// Global Ctrl+Shift+F12, active only while NBA2K27.exe is running (as WPF
/// did during Play) so the chord is not held from other apps otherwise.
pub(crate) fn start_hotkey(root: PathBuf, on_press: impl Fn() + Send + 'static) {
    use windows::Win32::UI::{
        Input::KeyboardAndMouse::{
            MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey, UnregisterHotKey, VK_F12,
        },
        WindowsAndMessaging::{
            MSG, MsgWaitForMultipleObjects, PM_REMOVE, PeekMessageW, QS_HOTKEY, WM_HOTKEY,
        },
    };
    let (phase, message) = hotkey_view(false, false, None);
    publish(|s| {
        s.phase = phase;
        s.message = message;
    });
    let _ = std::thread::Builder::new()
        .name("game-capture-hotkey".into())
        .spawn(move || {
            let mut registered = false;
            let mut error: Option<u32> = None;
            let mut pids: Vec<u32> = vec![];
            let mut next_scan = 0u64;
            let mut presses = Presses::default();
            loop {
                let now = now_ms();
                if now >= next_scan {
                    next_scan = now + IDLE_MS as u64;
                    pids = game_pids().unwrap_or_default();
                    let running = !pids.is_empty();
                    let before = (registered, error);
                    if running && !registered && error.is_none() {
                        match unsafe {
                            RegisterHotKey(None, HOTKEY_ID, MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT, VK_F12.0 as u32)
                        } {
                            Ok(()) => registered = true,
                            Err(e) => error = Some((e.code().0 as u32) & 0xFFFF),
                        }
                    } else if !running && (registered || error.is_some()) {
                        if registered {
                            unsafe {
                                let _ = UnregisterHotKey(None, HOTKEY_ID);
                            }
                        }
                        registered = false;
                        error = None;
                    }
                    if before != (registered, error) {
                        let (phase, message) = hotkey_view(running, registered, error);
                        log_line(&root, &format!("state {phase} registered={registered} error={error:?} game_pids={pids:?}"));
                        publish(|s| {
                            s.phase = phase;
                            s.message = message;
                            s.registered = registered;
                            s.error_code = error;
                            s.polling = running;
                        });
                    }
                }
                let running = !pids.is_empty();
                unsafe {
                    MsgWaitForMultipleObjects(None, false, if running { POLL_MS } else { IDLE_MS }, QS_HOTKEY)
                };
                let now = now_ms();
                let mut source = None;
                let mut msg = MSG::default();
                while unsafe { PeekMessageW(&mut msg, None, WM_HOTKEY, WM_HOTKEY, PM_REMOVE) }.as_bool() {
                    if msg.wParam.0 == HOTKEY_ID as usize && presses.fire(now) {
                        source = Some("hotkey");
                    }
                }
                if running && game_foreground(&pids) {
                    if presses.sample(chord_down(), now) {
                        source.get_or_insert("keyboard-poll");
                    }
                } else {
                    presses.sample(false, now);
                }
                if let Some(source) = source {
                    log_line(&root, &format!("press source={source}"));
                    publish(|s| {
                        s.presses += 1;
                        s.last_press_ms = Some(now);
                        s.last_source = Some(source);
                    });
                    on_press();
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_press_fires_once_across_hotkey_and_key_polling() {
        let mut p = Presses::default();
        assert!(p.fire(10_000), "WM_HOTKEY fires");
        assert!(
            !p.sample(true, 10_020),
            "polling the same press is de-duplicated"
        );
        assert!(!p.sample(true, 10_040), "held keys do not repeat");
        assert!(!p.sample(false, 12_000));
        assert!(p.sample(true, 12_020), "a new press after release fires");
        assert!(!p.fire(12_030), "its WM_HOTKEY is de-duplicated");
        let mut q = Presses::default();
        assert!(
            q.sample(true, 1),
            "polling alone works when RegisterHotKey is unavailable"
        );
    }
    #[test]
    fn hotkey_states_explain_why_the_chord_may_not_fire() {
        assert_eq!(hotkey_view(false, false, None).0, "waiting-for-game");
        assert_eq!(hotkey_view(true, true, None).0, "registered");
        let (phase, message) = hotkey_view(true, false, Some(1409));
        assert_eq!(phase, "held");
        assert!(message.contains("Another app"));
        assert!(hotkey_view(true, false, Some(5)).1.contains("error 5"));
    }

    fn temp(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "launcher-game-capture-{name}-{}",
            crate::backend::unique_id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn fake_game() -> Game {
        Game {
            handle: Handle(HANDLE::default()),
            pid: 4242,
            started_ms: 1_760_000_000_000,
            kernel_ms: 10,
            user_ms: 20,
        }
    }

    #[test]
    fn state_and_marker_are_redacted() {
        let root = PathBuf::from(r"C:\Users\Alice\2k Launcher");
        let game_root = PathBuf::from(r"D:\SteamLibrary\steamapps\common\NBA 2K27");
        let windows = vec![(r"NBA 2K27 - C:\Users\Alice\Documents".to_string(), true)];
        let state = redacted(
            state_document(
                2,
                &fake_game(),
                &windows,
                Err(format!("failed at {}", game_root.display())),
                "test",
            ),
            &root,
            &game_root,
        );
        let marker = redacted(
            json!({"game": game_root.join(GAME_EXE), "logs": root.join("Logs"), "token": "secret-value"}),
            &root,
            &game_root,
        );
        let text = format!("{state}{marker}");
        assert!(!text.contains("Alice"));
        assert!(!text.contains("SteamLibrary"));
        assert!(!text.contains("secret-value"));
        assert!(text.contains("<game-root>"));
        assert!(text.contains("<portable-root>"));
        assert_eq!(state["windowHung"], true);
        assert_eq!(state["snapshot"], 2);
        assert_eq!(state["pid"], 4242);
    }

    #[test]
    fn slots_and_names_are_bounded() {
        let folder = temp("slots");
        assert_eq!(next_slot(&folder), Some(1));
        fs::write(folder.join("hang-1.dmp"), b"x").unwrap();
        fs::write(folder.join("hang-2.partial"), b"x").unwrap();
        assert_eq!(next_slot(&folder), Some(3));
        fs::write(folder.join("hang-3.dmp"), b"x").unwrap();
        assert_eq!(next_slot(&folder), None);
        assert!(valid_session(&session_name(1_760_000_000_000, 4242)));
        assert!(!valid_session("session-123-4"));
        assert!(!valid_session("session-1760000000000-"));
        assert!(!valid_session("other-1760000000000-1"));
        assert!(
            owned_file("hang-3.partial")
                && owned_file("state-1.json")
                && owned_file("capture.json")
        );
        assert!(
            !owned_file("hang-4.dmp") && !owned_file("notes.txt") && !owned_file("state-1.dmp")
        );
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn retention_removes_only_old_owned_sessions() {
        let root = temp("retain");
        let marker = json!({"schema": SCHEMA}).to_string();
        for ms in 1..=5u64 {
            let folder = root.join(session_name(1_760_000_000_000 + ms, 7));
            fs::create_dir_all(&folder).unwrap();
            fs::write(folder.join("capture.json"), &marker).unwrap();
            fs::write(folder.join("hang-1.dmp"), b"dump").unwrap();
        }
        // Oldest: foreign file inside → kept. Second oldest: wrong schema → kept.
        fs::write(
            root.join(session_name(1_760_000_000_001, 7))
                .join("notes.txt"),
            b"user",
        )
        .unwrap();
        fs::write(
            root.join(session_name(1_760_000_000_002, 7))
                .join("capture.json"),
            b"{}",
        )
        .unwrap();
        fs::create_dir_all(root.join("unrelated")).unwrap();
        let older = root.join(session_name(1_759_000_000_000, 8));
        fs::create_dir_all(&older).unwrap();
        fs::write(older.join("capture.json"), &marker).unwrap();
        retain_sessions(&root, 3).unwrap();
        let mut left = fs::read_dir(&root)
            .unwrap()
            .flatten()
            .map(|r| r.file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        left.sort();
        assert_eq!(
            left,
            vec![
                session_name(1_760_000_000_001, 7),
                session_name(1_760_000_000_002, 7),
                session_name(1_760_000_000_003, 7),
                session_name(1_760_000_000_004, 7),
                session_name(1_760_000_000_005, 7),
                "unrelated".to_string(),
            ]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn capture_refuses_without_matching_game_and_writes_nothing() {
        let root = temp("refuse");
        let game_root = root.join("Game");
        fs::create_dir_all(&game_root).unwrap();
        fs::write(game_root.join(GAME_EXE), b"fixture, never executed").unwrap();
        assert!(capture(&root, &game_root, "test").is_err());
        assert!(!captures_root(&root).exists());
        assert!(capture(&root, &root.join("Missing"), "test").is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
