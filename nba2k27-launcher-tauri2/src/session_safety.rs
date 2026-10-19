//! Native read-only preflight and fail-closed session policies.
//! No launch, termination, injection or game restoration executor is exposed.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::{Component, Path},
};
use windows::{
    Win32::{
        Foundation::{
            CloseHandle, ERROR_NO_MORE_FILES, FILETIME, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT,
        },
        System::{Diagnostics::ToolHelp::*, Threading::*},
    },
    core::PWSTR,
};
// Serialize explicit native data writes and recovery completion across the
// independent browser and F8 services. Their journals must not race each other.
static GAME_WRITE_GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());
pub(crate) fn write_gate() -> Result<std::sync::MutexGuard<'static, ()>> {
    GAME_WRITE_GATE.lock().map_err(|_| {
        anyhow::anyhow!("Native transaction gate failed; restart the launcher before another write")
    })
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Identity {
    pub pid: u32,
    pub started: String,
    pub path: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Process {
    pub name: String,
    pub pid: u32,
    pub parent: u32,
    pub identity: Option<Identity>,
}
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
enum IdentityQuery {
    Live(Identity),
    Exited,
}
const METADATA_EXIT_GRACE_MS: u32 = 30;
fn metadata_ended_wait(h: HANDLE, waitable: bool, timeout_ms: u32) -> Result<bool> {
    if waitable {
        match unsafe { WaitForSingleObject(h, timeout_ms) } {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            _ => anyhow::bail!("Process liveness could not be confirmed"),
        }
    } else {
        // If SYNCHRONIZE is denied, preserve query-only compatibility. Only a
        // definite non-STILL_ACTIVE exit code proves death; 259 stays unresolved.
        let mut code = 259;
        unsafe {
            GetExitCodeProcess(h, &mut code)?;
        }
        Ok(code != 259)
    }
}
fn metadata_ended(h: HANDLE, waitable: bool) -> Result<bool> {
    metadata_ended_wait(h, waitable, 0)
}
fn finish_metadata<T>(
    result: Result<T>,
    ended: impl FnOnce(u32) -> Result<bool>,
) -> Result<Option<T>> {
    // Windows may deny metadata during teardown before signaling the retained
    // process handle. A short, bounded wait can prove that exact process ended.
    // A still-live/unknown process keeps its ORIGINAL error and blocks launch;
    // this never exempts a Node path, PID, tool or parent by assumption.
    let timeout_ms = if result.is_err() {
        METADATA_EXIT_GRACE_MS
    } else {
        0
    };
    if ended(timeout_ms)? {
        return Ok(None);
    }
    result.map(Some)
}
fn queried_identity(pid: u32) -> Result<IdentityQuery> {
    ensure!(pid > 0, "Invalid process ID");
    let queried = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            false,
            pid,
        )
    };
    let (queried, waitable) = match queried {
        Err(e) if e.code() == windows::Win32::Foundation::ERROR_ACCESS_DENIED.to_hresult() => (
            unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) },
            false,
        ),
        other => (other, true),
    };
    let h = match queried {
        Ok(h) => Handle(h),
        // A Toolhelp row may outlive its process. Access denied is NOT absence.
        Err(e) if e.code() == windows::Win32::Foundation::ERROR_INVALID_PARAMETER.to_hresult() => {
            return Ok(IdentityQuery::Exited);
        }
        Err(e) => return Err(e).context("OpenProcess for process metadata"),
    };
    if metadata_ended(h.0, waitable).context("Initial process liveness check")? {
        return Ok(IdentityQuery::Exited);
    }
    let (mut created, mut exited, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    let times = unsafe {
        GetProcessTimes(h.0, &mut created, &mut exited, &mut kernel, &mut user)
            .context("GetProcessTimes for process identity")
    };
    // GetProcessTimes exit time is undefined while active. Never use it as
    // liveness authority; use the retained process handle instead.
    if finish_metadata(times, |timeout| {
        metadata_ended_wait(h.0, waitable, timeout)
            .context("Process liveness after birth-time query")
    })?
    .is_none()
    {
        return Ok(IdentityQuery::Exited);
    }
    let mut buf = vec![0u16; 32768];
    let mut len = buf.len() as u32;
    let path_result = unsafe {
        QueryFullProcessImageNameW(h.0, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len)
    };
    // Recheck the SAME retained kernel process, not a potentially reused PID.
    if finish_metadata(
        path_result.context("QueryFullProcessImageNameW for process identity"),
        |timeout| {
            metadata_ended_wait(h.0, waitable, timeout)
                .context("Process liveness after image-path query")
        },
    )?
    .is_none()
    {
        return Ok(IdentityQuery::Exited);
    }
    let started = ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64;
    ensure!(pid > 0 && started > 0, "Invalid process identity");
    Ok(IdentityQuery::Live(Identity {
        pid,
        started: started.to_string(),
        path: String::from_utf16(&buf[..len as usize])?,
    }))
}
pub fn identity(pid: u32) -> Result<Identity> {
    match queried_identity(pid)? {
        IdentityQuery::Live(identity) => Ok(identity),
        IdentityQuery::Exited => anyhow::bail!("Process ended before its identity was confirmed"),
    }
}
fn snapshot_identity(result: Result<IdentityQuery>) -> Option<Option<Identity>> {
    match result {
        Ok(IdentityQuery::Live(identity)) => Some(Some(identity)),
        Ok(IdentityQuery::Exited) => None,
        Err(_) => Some(None), // Unknown ACTIVE identity still blocks where relevant.
    }
}
pub fn processes() -> Result<Vec<Process>> {
    Ok(process_scan()?.rows)
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IdentityIssue {
    pub name: String,
    pub pid: u32,
    pub error: String,
}
pub(crate) struct ProcessScan {
    pub rows: Vec<Process>,
    pub issues: Vec<IdentityIssue>,
    pub prior_issues: Vec<IdentityIssue>,
    pub refreshed: bool,
}
#[derive(Clone)]
struct SnapshotRow {
    name: String,
    pid: u32,
    parent: u32,
}
pub(crate) fn process_scan() -> Result<ProcessScan> {
    refresh_unresolved(|| {
        Ok(inspect_snapshot(
            snapshot_rows()?,
            queried_identity,
            crate::provider_worker::owned,
        ))
    })
}
fn refresh_unresolved(mut scan: impl FnMut() -> Result<ProcessScan>) -> Result<ProcessScan> {
    let first = scan()?;
    if first.issues.is_empty() {
        return Ok(first);
    }
    // No sleep or unbounded retry. Re-enumerate ALL relevant names, not only
    // the old PID: a PID can be reused or another game/launcher can appear.
    // Only a successful fresh snapshot can supersede the stale one. A still
    // unreadable process remains a blocking row, and snapshot errors fail closed.
    let mut fresh = scan().context("Fresh ownership snapshot after an unreadable identity")?;
    fresh.prior_issues = first.issues;
    fresh.refreshed = true;
    Ok(fresh)
}
fn snapshot_rows() -> Result<Vec<SnapshotRow>> {
    // Every caller still gets a NEW kernel snapshot, including immediately
    // before each player-link mutation. Toolhelp's richer thread bookkeeping
    // costs tens of milliseconds on some machines; hundreds of link guards
    // compounded that into a ~24 s launch phase. Only the enumeration transport
    // changes: identity, liveness, unknown-owner and retry policy stay below.
    // The documented NT layout may change: reject malformed/unsupported output
    // and fall back to the existing Toolhelp implementation, never to old rows.
    #[cfg(target_pointer_width = "64")]
    return snapshot_transport(
        basic_snapshot_rows,
        system_snapshot_rows,
        toolhelp_snapshot_rows,
    );
    #[cfg(not(target_pointer_width = "64"))]
    toolhelp_snapshot_rows()
}

// Windows SDK 10.0.26100.0 um/winternl.h: SYSTEM_BASICPROCESS_INFORMATION
// lines 286..292 and SystemBasicProcessInformation = 252 at line 403.
// https://learn.microsoft.com/windows/win32/api/winternl/nf-winternl-ntquerysysteminformation
// This is only a fresh enumeration transport. SequenceNumber never substitutes
// for the retained birth/path identity queries performed by inspect_snapshot.
#[cfg(target_pointer_width = "64")]
fn snapshot_transport(
    basic: impl FnOnce() -> Result<Vec<SnapshotRow>>,
    classic: impl FnOnce() -> Result<Vec<SnapshotRow>>,
    toolhelp: impl FnOnce() -> Result<Vec<SnapshotRow>>,
) -> Result<Vec<SnapshotRow>> {
    basic().or_else(|_| classic()).or_else(|_| toolhelp())
}

#[cfg(target_pointer_width = "64")]
fn basic_snapshot_rows() -> Result<Vec<SnapshotRow>> {
    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn NtQuerySystemInformation(
            class: u32,
            buffer: *mut std::ffi::c_void,
            length: u32,
            returned: *mut u32,
        ) -> i32;
    }
    basic_snapshot_with(std::process::id(), |storage, returned| unsafe {
        NtQuerySystemInformation(
            252,
            storage.as_mut_ptr().cast(),
            (storage.len() * 8) as u32,
            returned,
        )
    })
}

#[cfg(target_pointer_width = "64")]
fn basic_snapshot_with(
    own: u32,
    mut query: impl FnMut(&mut [u64], &mut u32) -> i32,
) -> Result<Vec<SnapshotRow>> {
    const MAX: usize = 16 * 1024 * 1024;
    let mut size = 65536usize;
    for _ in 0..4 {
        let mut storage = vec![0u64; size.div_ceil(8)];
        let mut returned = 0u32;
        let status = query(&mut storage, &mut returned);
        if status == 0 {
            let length = returned as usize;
            ensure!(
                length > 0 && length <= storage.len() * 8,
                "Invalid basic process snapshot length"
            );
            let bytes =
                unsafe { std::slice::from_raw_parts(storage.as_ptr().cast::<u8>(), length) };
            return decode_basic_snapshot(bytes, storage.as_ptr() as usize, own);
        }
        ensure!(
            status == 0xC0000004u32 as i32 || status == 0xC0000023u32 as i32,
            "Basic process snapshot query failed: {status:#x}"
        );
        let needed = returned as usize;
        ensure!(
            needed > storage.len() * 8 && needed <= MAX - 65536,
            "Basic process snapshot retry exceeds bounds"
        );
        size = needed + 65536;
    }
    anyhow::bail!("Basic process snapshot changed too often")
}

#[cfg(target_pointer_width = "64")]
fn decode_basic_snapshot(bytes: &[u8], base: usize, own: u32) -> Result<Vec<SnapshotRow>> {
    const HEADER: usize = 48;
    ensure!(
        own != 0 && (HEADER..=16 * 1024 * 1024).contains(&bytes.len()),
        "Invalid basic process snapshot size"
    );
    let word = |at: usize| -> Result<u16> {
        Ok(u16::from_le_bytes(
            bytes
                .get(at..at + 2)
                .context("Truncated basic process field")?
                .try_into()?,
        ))
    };
    let dword = |at: usize| -> Result<u32> {
        Ok(u32::from_le_bytes(
            bytes
                .get(at..at + 4)
                .context("Truncated basic process field")?
                .try_into()?,
        ))
    };
    let qword = |at: usize| -> Result<u64> {
        Ok(u64::from_le_bytes(
            bytes
                .get(at..at + 8)
                .context("Truncated basic process field")?
                .try_into()?,
        ))
    };
    let mut rows = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut at = 0usize;
    loop {
        ensure!(
            rows.len() < 65536 && at.is_multiple_of(8) && bytes.len().saturating_sub(at) >= HEADER,
            "Invalid basic process entry bounds"
        );
        let next = dword(at)? as usize;
        ensure!(
            next == 0 || (next >= HEADER && next.is_multiple_of(8)),
            "Invalid basic process stride"
        );
        let end = if next == 0 {
            bytes.len()
        } else {
            at.checked_add(next)
                .context("Basic process stride overflow")?
        };
        ensure!(end <= bytes.len(), "Basic process entry exceeds buffer");
        let pid = u32::try_from(qword(at + 8)?)?;
        let parent = u32::try_from(qword(at + 16)?)?;
        ensure!(
            seen.insert(pid) && (pid == 0 || qword(at + 24)? != 0),
            "Duplicate basic process ID or missing sequence"
        );
        let length = word(at + 32)? as usize;
        let maximum = word(at + 34)? as usize;
        ensure!(
            length.is_multiple_of(2)
                && maximum.is_multiple_of(2)
                && length <= maximum
                && length <= 518
                && maximum <= 520,
            "Invalid basic process name length"
        );
        let pointer = usize::try_from(qword(at + 40)?)?;
        let (name, consumed) = if length == 0 {
            ensure!(
                pid == 0 && pointer == 0 && maximum == 0,
                "Missing basic process name"
            );
            (String::new(), at + HEADER)
        } else {
            let start = pointer
                .checked_sub(base)
                .context("Basic name precedes buffer")?;
            ensure!(
                start == at + HEADER
                    && pointer.is_multiple_of(2)
                    && start.checked_add(maximum).is_some_and(|n| n <= end),
                "Basic process name exceeds its record"
            );
            let utf16 = bytes[start..start + length]
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect::<Vec<_>>();
            let name = String::from_utf16(&utf16)?;
            ensure!(
                !name.chars().any(|c| c == '\0'
                    || (c as u32) < 32
                    || c == '/'
                    || c == '\\'
                    || c == ':'),
                "Invalid basic process basename"
            );
            (name, start + maximum)
        };
        ensure!(
            consumed <= end && end - consumed <= 7,
            "Invalid basic process trailing bytes"
        );
        rows.push(SnapshotRow { name, pid, parent });
        if next == 0 {
            break;
        }
        at = end;
    }
    ensure!(
        seen.contains(&own),
        "Current process absent from basic snapshot"
    );
    Ok(rows)
}

#[cfg(all(test, target_pointer_width = "64"))]
#[path = "session_safety_basic_tests.rs"]
mod basic_tests;

#[cfg(target_pointer_width = "64")]
fn system_snapshot_rows() -> Result<Vec<SnapshotRow>> {
    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn NtQuerySystemInformation(
            class: u32,
            buffer: *mut std::ffi::c_void,
            length: u32,
            returned: *mut u32,
        ) -> i32;
    }
    const MISMATCH: i32 = 0xC0000004u32 as i32;
    const MAX: usize = 32 * 1024 * 1024;
    let mut size = 256 * 1024usize;
    for _ in 0..6 {
        // u64 storage guarantees the native buffer's eight-byte alignment.
        let mut storage = vec![0u64; size.div_ceil(8)];
        let mut returned = 0u32;
        let status = unsafe {
            NtQuerySystemInformation(
                5,
                storage.as_mut_ptr().cast(),
                (storage.len() * 8) as u32,
                &mut returned,
            )
        };
        if status == MISMATCH {
            size = size
                .checked_mul(2)
                .context("Process snapshot size overflow")?
                .max(returned as usize + 65536);
            ensure!(size <= MAX, "Process snapshot exceeds bounds");
            continue;
        }
        ensure!(status >= 0, "Process snapshot query failed: {status:#x}");
        let length = returned as usize;
        ensure!(
            length > 0 && length <= storage.len() * 8,
            "Invalid process snapshot length"
        );
        let bytes = unsafe { std::slice::from_raw_parts(storage.as_ptr().cast::<u8>(), length) };
        let rows = decode_system_snapshot(bytes, storage.as_ptr() as usize)?;
        ensure!(
            rows.iter().any(|row| row.pid == std::process::id()),
            "Current process absent from process snapshot"
        );
        return Ok(rows);
    }
    anyhow::bail!("Process snapshot changed too often")
}

/// The public 64-bit SYSTEM_PROCESS_INFORMATION prefix from winternl.h.
/// Parse by checked byte ranges rather than dereferencing returned pointers.
/// Reference: https://learn.microsoft.com/windows/win32/api/winternl/nf-winternl-ntquerysysteminformation
#[cfg(target_pointer_width = "64")]
fn decode_system_snapshot(bytes: &[u8], base: usize) -> Result<Vec<SnapshotRow>> {
    const HEADER: usize = 96;
    let u16_at = |at| -> Result<u16> {
        Ok(u16::from_le_bytes(
            bytes
                .get(at..at + 2)
                .context("Truncated process field")?
                .try_into()?,
        ))
    };
    let u32_at = |at| -> Result<u32> {
        Ok(u32::from_le_bytes(
            bytes
                .get(at..at + 4)
                .context("Truncated process field")?
                .try_into()?,
        ))
    };
    let u64_at = |at| -> Result<u64> {
        Ok(u64::from_le_bytes(
            bytes
                .get(at..at + 8)
                .context("Truncated process field")?
                .try_into()?,
        ))
    };
    let mut at = 0usize;
    let mut rows = Vec::new();
    loop {
        ensure!(
            rows.len() < 100_000 && bytes.len().saturating_sub(at) >= HEADER,
            "Invalid process snapshot entry bounds"
        );
        let next = u32_at(at)? as usize;
        let end = if next == 0 {
            bytes.len()
        } else {
            ensure!(
                next >= HEADER && next.is_multiple_of(8),
                "Invalid process snapshot stride"
            );
            at.checked_add(next)
                .context("Process snapshot stride overflow")?
        };
        ensure!(end <= bytes.len(), "Process snapshot entry exceeds buffer");
        let length = u16_at(at + 56)? as usize;
        let maximum = u16_at(at + 58)? as usize;
        ensure!(
            length.is_multiple_of(2) && length <= maximum && maximum <= 65534,
            "Invalid process image-name length"
        );
        let name = if length == 0 {
            String::new()
        } else {
            let pointer = usize::try_from(u64_at(at + 64)?)?;
            let start = pointer
                .checked_sub(base)
                .context("Process image-name pointer precedes buffer")?;
            let finish = start
                .checked_add(length)
                .context("Process image-name pointer overflow")?;
            ensure!(
                start >= at + HEADER && finish <= end && start.is_multiple_of(2),
                "Process image-name pointer exceeds entry"
            );
            let utf16 = bytes[start..finish]
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect::<Vec<_>>();
            let name = String::from_utf16(&utf16)?;
            ensure!(!name.contains('\0'), "Embedded NUL in process image name");
            name
        };
        rows.push(SnapshotRow {
            name,
            pid: u32::try_from(u64_at(at + 80)?)?,
            parent: u32::try_from(u64_at(at + 88)?)?,
        });
        if next == 0 {
            break;
        }
        at = end;
    }
    Ok(rows)
}

fn toolhelp_snapshot_rows() -> Result<Vec<SnapshotRow>> {
    let h = Handle(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)? });
    let mut e = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    unsafe {
        Process32FirstW(h.0, &mut e)?;
    }
    let mut rows = Vec::new();
    loop {
        let n = e
            .szExeFile
            .iter()
            .position(|c| *c == 0)
            .unwrap_or(e.szExeFile.len());
        let name = String::from_utf16(&e.szExeFile[..n])?;
        rows.push(SnapshotRow {
            name,
            pid: e.th32ProcessID,
            parent: e.th32ParentProcessID,
        });
        match unsafe { Process32NextW(h.0, &mut e) } {
            Ok(()) => {}
            Err(error) if error.code() == ERROR_NO_MORE_FILES.to_hresult() => break,
            Err(error) => return Err(error.into()),
        }
    }
    // Release the snapshot before querying process metadata or taking a retry.
    Ok(rows)
}
fn inspect_snapshot(
    entries: Vec<SnapshotRow>,
    mut query: impl FnMut(u32) -> Result<IdentityQuery>,
    owned: impl Fn(&Identity) -> bool,
) -> ProcessScan {
    let mut scan = ProcessScan {
        rows: Vec::new(),
        issues: Vec::new(),
        prior_issues: Vec::new(),
        refreshed: false,
    };
    for e in entries {
        let name = e.name;
        let relevant = [
            "nba2k27.exe",
            "steam.exe",
            "easyanticheat.exe",
            "easyanticheat_eos.exe",
            "eaclaunch.exe",
        ]
        .iter()
        .any(|s| name.eq_ignore_ascii_case(s))
            || protected_process(&name);
        if e.pid != std::process::id()
            && (relevant || launcher_name(&name) || name.eq_ignore_ascii_case("node.exe"))
        {
            let result = query(e.pid);
            if let Err(error) = &result {
                // Windows metadata errors only: no command line, environment or
                // process image contents are copied into these diagnostics.
                scan.issues.push(IdentityIssue {
                    name: name.clone(),
                    pid: e.pid,
                    error: format!("{error:#}")
                        .chars()
                        .filter(|c| !c.is_control())
                        .take(512)
                        .collect(),
                });
            }
            let identity = snapshot_identity(result);
            if let Some(identity) = identity {
                let process = Process {
                    name,
                    pid: e.pid,
                    parent: e.parent,
                    identity,
                };
                if (relevant || launcher_process(&process))
                    && !process.identity.as_ref().is_some_and(&owned)
                {
                    scan.rows.push(process);
                }
            }
        }
    }
    scan
}
fn launcher_name(name: &str) -> bool {
    [
        "2k Launcher WPF.exe",
        "2k Launcher.exe",
        "2k Launcher Rust Prototype.exe",
        "2k Launcher Rust.exe",
        "nba2k27-launcher-tauri2.exe",
    ]
    .iter()
    .any(|n| name.eq_ignore_ascii_case(n))
        || (name.to_ascii_lowercase().starts_with("2k-launcher-")
            && name.to_ascii_lowercase().ends_with(".exe"))
}
pub(crate) fn launcher_process(p: &Process) -> bool {
    launcher_name(&p.name)
        || (p.name.eq_ignore_ascii_case("node.exe")
            && p.identity.as_ref().is_none_or(|i| {
                i.path
                    .replace('/', "\\")
                    .to_ascii_lowercase()
                    .ends_with("\\app\\backend\\node.exe")
            }))
}
pub(crate) fn protected_process(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.starts_with("easyanticheat") || n == "eaclaunch.exe" || n == "start_protected_game.exe"
}
fn bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let f = fs::File::open(path)?;
    ensure!(
        f.metadata()?.is_file() && f.metadata()?.len() <= limit,
        "File exceeds its limit"
    );
    let mut bytes = Vec::new();
    f.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "File changed or exceeds its limit"
    );
    Ok(bytes)
}
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub app_id: String,
    pub name: String,
    pub install_dir: String,
}
pub fn manifest(text: &str) -> Result<Manifest> {
    // Parse quoted VDF tokens, not a regex that could select nested/user values.
    ensure!(text.len() <= 1024 * 1024, "Manifest too large");
    let mut tokens = Vec::new();
    let mut chars = text.trim_start_matches('\u{feff}').chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_whitespace() {
            continue;
        }
        if c == '/' && chars.peek() == Some(&'/') {
            for c in chars.by_ref() {
                if c == '\n' {
                    break;
                }
            }
            continue;
        }
        if c == '{' || c == '}' {
            tokens.push(c.to_string());
            continue;
        }
        ensure!(c == '"', "Malformed Steam manifest");
        let mut value = String::new();
        let mut ended = false;
        while let Some(c) = chars.next() {
            if c == '"' {
                ended = true;
                break;
            }
            if c == '\\' {
                let n = chars.next().context("Invalid manifest escape")?;
                ensure!(n == '"' || n == '\\', "Unsupported manifest escape");
                value.push(n);
            } else {
                ensure!(!c.is_control(), "Invalid manifest text");
                value.push(c);
            }
        }
        ensure!(ended, "Unterminated manifest text");
        tokens.push(value);
    }
    ensure!(
        tokens.len() >= 3 && tokens[0].eq_ignore_ascii_case("AppState") && tokens[1] == "{",
        "Missing AppState"
    );
    let (mut depth, mut at) = (1usize, 2usize);
    let mut values = std::collections::HashMap::new();
    let mut root_keys = HashSet::new();
    while at < tokens.len() {
        if tokens[at] == "}" {
            depth = depth.checked_sub(1).context("Invalid manifest nesting")?;
            at += 1;
            if depth == 0 {
                break;
            }
            continue;
        }
        let key = &tokens[at];
        ensure!(key != "{", "Missing manifest key");
        if depth == 1 {
            ensure!(
                root_keys.insert(key.to_lowercase()),
                "Duplicate manifest key"
            );
        }
        at += 1;
        let value = tokens.get(at).context("Missing manifest value")?;
        if value == "{" {
            depth += 1;
            ensure!(depth <= 16, "Manifest nesting exceeds limit");
            at += 1;
            continue;
        }
        ensure!(value != "}", "Missing manifest value");
        if depth == 1 {
            ensure!(
                values.insert(key.to_lowercase(), value.clone()).is_none(),
                "Duplicate manifest key"
            );
        }
        at += 1;
    }
    ensure!(
        depth == 0 && at == tokens.len(),
        "Incomplete or trailing manifest data"
    );
    let m = Manifest {
        app_id: values.remove("appid").context("App ID missing")?,
        name: values.remove("name").context("Game name missing")?,
        install_dir: values
            .remove("installdir")
            .context("Install directory missing")?,
    };
    ensure!(
        m.app_id
            .parse::<u32>()
            .is_ok_and(|id| id > 0 && id.to_string() == m.app_id),
        "Invalid Steam app ID"
    );
    ensure!(
        !m.install_dir.is_empty()
            && Path::new(&m.install_dir).components().count() == 1
            && !m.install_dir.contains(['/', '\\', ':'])
            && m.install_dir != "."
            && m.install_dir != "..",
        "Invalid install directory"
    );
    Ok(m)
}
pub(crate) fn resolve(game: &Path) -> Result<Value> {
    resolve_with(game, crate::steam_libraries::steam_roots)
}
/// `steam_roots` is consulted only when no steam.exe sits above the library,
/// i.e. the game lives in a secondary Steam library folder.
fn resolve_with(
    game: &Path,
    steam_roots: impl FnOnce() -> Vec<std::path::PathBuf>,
) -> Result<Value> {
    ensure!(game.is_absolute(), "Choose an absolute game folder");
    let game = game.canonicalize().context("Game folder not found")?;
    ensure!(game.join("NBA2K27.exe").is_file(), "NBA2K27.exe not found");
    let common = game.parent().context("Steam common folder missing")?;
    ensure!(
        common
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case("common")),
        "Game is not in a Steam common folder"
    );
    let apps = common.parent().context("Steam apps folder missing")?;
    ensure!(
        apps.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case("steamapps")),
        "Steam apps folder missing"
    );
    let library = apps.parent().context("Steam root missing")?;
    let (steam, listed_apps) = if library.join("steam.exe").is_file() {
        (library.join("steam.exe"), None)
    } else {
        crate::steam_libraries::steam_for_library(library, &steam_roots())?
    };
    let mut matches = Vec::new();
    let mut count = 0;
    for e in fs::read_dir(apps)? {
        let e = e?;
        let filename = e.file_name().to_string_lossy().to_lowercase();
        let Some(id) = filename
            .strip_prefix("appmanifest_")
            .and_then(|s| s.strip_suffix(".acf"))
        else {
            continue;
        };
        if !id.bytes().all(|b| b.is_ascii_digit()) || id.is_empty() {
            continue;
        }
        count += 1;
        ensure!(count <= 512, "Too many Steam manifests");
        let m = manifest(std::str::from_utf8(&bounded(&e.path(), 1024 * 1024)?)?)?;
        if m.name.eq_ignore_ascii_case("NBA 2K27") {
            ensure!(m.app_id == id, "Manifest app ID differs from filename");
            ensure!(
                game.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.eq_ignore_ascii_case(&m.install_dir)),
                "NBA 2K27 manifest points to a different folder"
            );
            ensure!(
                listed_apps
                    .as_ref()
                    .is_none_or(|apps| apps.contains(&m.app_id)),
                "Steam does not list NBA 2K27 in this library"
            );
            matches.push(json!({"appId":m.app_id,"steamExe":steam,"gameRoot":game,"steamUrl":format!("steam://launch/{}/option1",m.app_id),"label":"NBA 2K27 without EAC (offline only)"}));
        }
    }
    ensure!(
        matches.len() == 1,
        "No unique matching NBA 2K27 Steam manifest"
    );
    Ok(matches.remove(0))
}
pub fn preflight(game: &Path, portable: &Path) -> Value {
    preflight_with(game, portable, processes())
}
fn preflight_with(game: &Path, portable: &Path, snapshot: Result<Vec<Process>>) -> Value {
    let mut reasons = Vec::new();
    let launch = match resolve(game) {
        Ok(v) => Some(v),
        Err(e) => {
            reasons.push(format!("{e:#}"));
            None
        }
    };
    let known = snapshot.is_ok();
    let rows = snapshot.unwrap_or_else(|e| {
        reasons.push(format!("Process presence unknown: {e:#}"));
        Vec::new()
    });
    let running = rows
        .iter()
        .any(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe"));
    if running {
        reasons.push("NBA 2K27 is already running; do not start a second session".into());
    }
    if rows.iter().any(launcher_process) {
        reasons.push("Another launcher/backend is running or its identity is unknown; close it before a Steam-only QA launch".into());
    }
    if rows.iter().any(|p| protected_process(&p.name)) {
        reasons.push("An anti-cheat process is present; live helpers must stay disabled".into());
    }
    match game.join("version.dll").try_exists() {
        Ok(true) => reasons.push(
            "Existing version.dll is not owned by this Rust prototype; left untouched".into(),
        ),
        Ok(false) => {}
        Err(e) => reasons.push(format!("Existing proxy presence unknown: {e}")),
    }
    let journal = portable.join("Data/session-journal.json");
    let mut recovery = match journal.try_exists() {
        Ok(true) => match read_journal(&journal) {
            Ok(j) => j.phase != Phase::Closed,
            Err(e) => {
                reasons.push(format!("Session journal needs review: {e:#}"));
                true
            }
        },
        Ok(false) => false,
        Err(e) => {
            reasons.push(format!("Session journal presence unknown: {e}"));
            true
        }
    };
    match crate::game_rules::pending(portable) {
        Ok(true) => {
            recovery = true;
            reasons.push("Unresolved native Game Rules transaction; close the game and clear finished launch".into());
        }
        Ok(false) => {}
        Err(e) => {
            recovery = true;
            reasons.push(format!(
                "Game Rules journal needs review; left untouched: {e:#}"
            ));
        }
    }
    match crate::player_transaction::pending(portable) {
        Ok(true) => {
            recovery = true;
            reasons.push(
                "Unresolved native player transaction; close the game and clear finished launch"
                    .into(),
            );
        }
        Ok(false) => {}
        Err(e) => {
            recovery = true;
            reasons.push(format!(
                "Player transaction journal needs review; left untouched: {e:#}"
            ));
        }
    }
    match crate::team_transaction::pending(portable) {
        Ok(true) => {
            recovery = true;
            reasons.push(
                "Unresolved native team transaction; close the game and clear finished launch"
                    .into(),
            );
        }
        Ok(false) => {}
        Err(e) => {
            recovery = true;
            reasons.push(format!(
                "Team transaction journal needs review; left untouched: {e:#}"
            ));
        }
    }
    match crate::uniform_transaction::pending(portable) {
        Ok(true) => {
            recovery = true;
            reasons.push(
                "Unresolved native uniform transaction; close the game and clear finished launch"
                    .into(),
            );
        }
        Ok(false) => {}
        Err(e) => {
            recovery = true;
            reasons.push(format!(
                "Uniform transaction journal needs review; left untouched: {e:#}"
            ));
        }
    }
    match crate::arena_session::pending(portable) {
        Ok(true) => {
            recovery = true;
            reasons.push(
                "Unresolved native arena transaction; close the game and clear finished launch"
                    .into(),
            );
        }
        Ok(false) => {}
        Err(e) => {
            recovery = true;
            reasons.push(format!("Arena recovery presence is unknown: {e:#}"));
        }
    }
    match crate::ball_session::pending(portable) {
        Ok(true) => {
            recovery = true;
            reasons.push(
                "Unresolved native ball transaction; close the game and clear finished launch"
                    .into(),
            );
        }
        Ok(false) => {}
        Err(e) => {
            recovery = true;
            reasons.push(format!("Ball recovery presence is unknown: {e:#}"));
        }
    }
    match crate::ball_gameplay_runtime::pending(portable) {
        Ok(false) => {}
        Ok(true) => {
            recovery = true;
            reasons.push(
                "Gameplay-ball recovery is pending; close the game and clear finished launch"
                    .into(),
            );
        }
        Err(e) => {
            recovery = true;
            reasons.push(format!("Gameplay-ball recovery presence is unknown: {e:#}"));
        }
    }
    match crate::outfit_journal::pending(portable) {
        Ok(true) => {
            recovery = true;
            reasons.push(
                "Unresolved native outfit transaction; close the game and clear finished launch"
                    .into(),
            );
        }
        Ok(false) => {}
        Err(e) => {
            recovery = true;
            reasons.push(format!("Outfit recovery presence is unknown: {e:#}"));
        }
    }
    match crate::outfit_worker_v2_lease::pending(portable) {
        Ok(false) => {}
        Ok(true) => {
            recovery = true;
            reasons.push("Version 2 outfit worker recovery is pending; close the game and clear its owned launch".into());
        }
        Err(e) => {
            recovery = true;
            reasons.push(format!(
                "Version 2 outfit worker recovery presence is unknown: {e:#}"
            ));
        }
    }
    match crate::outfit_worker_v2_loader::pending(portable) {
        Ok(false) => {}
        Ok(true) => {
            recovery = true;
            reasons.push("Version 2 outfit loader recovery is pending; close the game and clear its owned launch".into());
        }
        Err(e) => {
            recovery = true;
            reasons.push(format!(
                "Version 2 outfit loader recovery presence is unknown: {e:#}"
            ));
        }
    }
    if recovery {
        reasons.push("Pending session recovery must be resolved before a new launch".into());
    }
    for name in [
        "loader-session.json",
        "loader-outfit-v2-session.json",
        "mod-profile-session.json",
        crate::player_folders::MARKER,
        crate::selector_files::MARKER,
        crate::camera_name_files::MARKER,
        crate::feature_journal::MARKER,
    ] {
        match portable.join("Data").join(name).try_exists() {
            Ok(true) => {
                recovery = true;
                reasons.push("Native mod-loader/profile recovery is pending; close the game and clear finished launch".into());
            }
            Ok(false) => {}
            Err(e) => {
                recovery = true;
                reasons.push(format!("Native mod recovery presence is unknown: {e}"));
            }
        }
    }
    match crate::presentation_v2_runtime::pending(portable) {
        Ok(false) => {}
        Ok(true) => {
            recovery = true;
            reasons.push("Blacktop presentation recovery is pending; close the game and clear finished launch".into());
        }
        Err(e) => {
            recovery = true;
            reasons.push(format!("Presentation recovery presence is unknown: {e:#}"));
        }
    }
    match crate::scoreboard_runtime::pending(portable) {
        Ok(false) => {}
        Ok(true) => {
            recovery = true;
            reasons.push(
                "Blacktop scoreboard recovery is pending; close the game and clear finished launch"
                    .into(),
            );
        }
        Err(e) => {
            recovery = true;
            reasons.push(format!("Scoreboard recovery presence is unknown: {e:#}"));
        }
    }
    match crate::jukebox_launch::pending(portable) {
        Ok(false) => {}
        Ok(true) => {
            recovery = true;
            reasons.push(
                "Jukebox file recovery is pending; close the game and clear finished launch".into(),
            );
        }
        Err(e) => {
            recovery = true;
            reasons.push(format!("Jukebox recovery presence is unknown: {e:#}"));
        }
    }
    json!({"readOnly":true,"processPresenceKnown":known,"gameRunning":if known{Some(running)}else{None},"launchPlan":launch,"checksPassed":reasons.is_empty(),"blockers":reasons,"processes":rows,"recoveryRequired":recovery,"liveGameEnabled":false,"note":"Read-only eligibility only. Steam-only QA dispatch is separate and requires confirmation. Game-build compatibility and game-memory helpers are not enabled. Process absence alone does not prove offline mode or anti-cheat status."})
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    Waiting,
    Running,
    CleanupPending,
    Closed,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HelperLease {
    pub session: String,
    pub role: String,
    pub identity: Identity,
    pub binary_sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileLease {
    pub target: String,
    pub backup: Option<String>,
    pub original_sha256: Option<String>,
    pub installed_sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Journal {
    pub version: u32,
    pub session: String,
    pub phase: Phase,
    pub game: Option<Identity>,
    pub helpers: Vec<HelperLease>,
    pub files: Vec<FileLease>,
    #[serde(default)]
    pub launch: Option<LaunchContext>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LaunchContext {
    pub game_root: String,
    pub app_id: String,
    pub requested_ms: u64,
    pub kind: String,
}
fn hex(s: &str, n: usize) -> bool {
    s.len() == n && s.bytes().all(|b| b.is_ascii_hexdigit())
}
fn relative(s: &str) -> bool {
    !s.is_empty()
        && !s.contains([':', '\\'])
        && !s.chars().any(|c| c.is_control())
        && s.split('/').all(|p| {
            let device = p.split('.').next().unwrap_or_default().to_ascii_uppercase();
            !p.is_empty()
                && p != "."
                && p != ".."
                && !p.ends_with(['.', ' '])
                && !p.contains(['<', '>', '"', '|', '?', '*'])
                && !["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"].contains(&device.as_str())
                && !((device.starts_with("COM") || device.starts_with("LPT"))
                    && device.chars().count() == 4
                    && device
                        .chars()
                        .last()
                        .is_some_and(|c| "123456789¹²³".contains(c)))
        })
        && Path::new(s)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}
impl Journal {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1 && hex(&self.session, 48),
            "Unsupported session journal"
        );
        ensure!(
            self.helpers.len() <= 16 && self.files.len() <= 64,
            "Session journal exceeds ownership limits"
        );
        let valid_identity = |i: &Identity| {
            i.pid > 0
                && i.started.parse::<u64>().is_ok_and(|s| s > 0)
                && Path::new(&i.path).is_absolute()
                && !i.path.chars().any(|c| c.is_control())
        };
        ensure!(
            self.game.as_ref().is_none_or(valid_identity),
            "Invalid game identity"
        );
        if let Some(l) = &self.launch {
            ensure!(
                Path::new(&l.game_root).is_absolute()
                    && !l.game_root.chars().any(|c| c.is_control())
                    && l.app_id
                        .parse::<u32>()
                        .is_ok_and(|n| n > 0 && n.to_string() == l.app_id)
                    && l.requested_ms > 0
                    && matches!(l.kind.as_str(), "steam-only-qa" | "native-modded-qa"),
                "Invalid launch context"
            );
        }
        let mut pids = HashSet::new();
        let mut roles = HashSet::new();
        for h in &self.helpers {
            ensure!(
                h.session == self.session
                    && valid_identity(&h.identity)
                    && hex(&h.binary_sha256, 64)
                    && pids.insert(h.identity.pid)
                    && roles.insert(&h.role),
                "Invalid helper ownership"
            );
            ensure!(
                [
                    "camera",
                    "outfit",
                    "ball",
                    "arena",
                    "scoreboard",
                    "unlocks",
                    "crowd",
                    "provider"
                ]
                .contains(&h.role.as_str()),
                "Unknown helper role"
            );
        }
        let mut targets = HashSet::new();
        for f in &self.files {
            ensure!(
                relative(&f.target)
                    && targets.insert(f.target.to_ascii_lowercase())
                    && hex(&f.installed_sha256, 64),
                "Invalid file ownership"
            );
            ensure!(
                f.backup.is_some() == f.original_sha256.is_some(),
                "Incomplete backup ownership"
            );
            ensure!(
                f.backup.as_ref().is_none_or(|b| relative(b))
                    && f.original_sha256.as_ref().is_none_or(|h| hex(h, 64)),
                "Invalid backup ownership"
            );
        }
        Ok(())
    }
}
pub fn read_journal(path: &Path) -> Result<Journal> {
    let j: Journal = serde_json::from_slice(&bounded(path, 1024 * 1024)?)?;
    j.validate()?;
    Ok(j)
}
#[derive(Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum RestorePlan {
    AlreadyOriginal,
    RestoreBackup,
    RemoveCreated,
    Hold,
}
fn file_hash(root: &Path, relative_path: &str) -> Result<Option<String>> {
    use sha2::{Digest, Sha256};
    use std::os::windows::fs::MetadataExt;
    ensure!(relative(relative_path), "Unsafe ownership path");
    ensure!(
        fs::symlink_metadata(root)?.file_attributes() & 0x400 == 0,
        "Reparse-point root not allowed"
    );
    let mut path = root.to_path_buf();
    for component in Path::new(relative_path).components() {
        path.push(component.as_os_str());
        match fs::symlink_metadata(&path) {
            Ok(m) => ensure!(
                m.file_attributes() & 0x400 == 0,
                "Reparse-point ownership path not allowed"
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(Some(format!(
        "{:x}",
        Sha256::digest(bounded(&path, 64 * 1024 * 1024)?)
    )))
}
/// A read-only plan, never an automatic file delete/overwrite. The future executor
/// must repeat hash/path/process checks immediately before each operation.
#[allow(dead_code)]
pub fn plan_file_restore(
    j: &Journal,
    f: &FileLease,
    game: &Path,
    portable: &Path,
) -> Result<RestorePlan> {
    j.validate()?;
    ensure!(
        j.files.iter().any(|r| r.target == f.target
            && r.installed_sha256 == f.installed_sha256
            && r.original_sha256 == f.original_sha256
            && r.backup == f.backup),
        "File lease is not in this session"
    );
    let current = file_hash(game, &f.target)?;
    if current
        .as_ref()
        .zip(f.original_sha256.as_ref())
        .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
        || (current.is_none() && f.original_sha256.is_none())
    {
        return Ok(RestorePlan::AlreadyOriginal);
    }
    if !current
        .as_ref()
        .is_some_and(|h| h.eq_ignore_ascii_case(&f.installed_sha256))
    {
        return Ok(RestorePlan::Hold);
    }
    if let (Some(backup), Some(original)) = (&f.backup, &f.original_sha256) {
        ensure!(
            backup.starts_with(&format!("Cache/SessionBackups/{}/", j.session)),
            "Backup is outside this session's vault"
        );
        if file_hash(portable, backup)?.is_some_and(|h| h.eq_ignore_ascii_case(original)) {
            Ok(RestorePlan::RestoreBackup)
        } else {
            Ok(RestorePlan::Hold)
        }
    } else {
        Ok(RestorePlan::RemoveCreated)
    }
}

#[derive(Default)]
#[allow(dead_code)]
pub struct CleanupGate {
    running: bool,
    complete: bool,
}
#[derive(Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum CleanupDecision {
    Run,
    Pending,
    Completed,
}
#[allow(dead_code)]
impl CleanupGate {
    pub fn begin(&mut self) -> CleanupDecision {
        if self.complete {
            CleanupDecision::Completed
        } else if self.running {
            CleanupDecision::Pending
        } else {
            self.running = true;
            CleanupDecision::Run
        }
    }
    pub fn finish(&mut self, success: bool) -> Result<()> {
        ensure!(self.running, "No cleanup is in progress");
        self.running = false;
        self.complete = success;
        Ok(())
    }
}

// These policies are not game actions. A future private native executor must take
// fresh observations, validate the journal, and recheck ownership before execution.
#[allow(dead_code)]
pub fn owns_helper(j: &Journal, lease: &HelperLease, current: &Identity, hash: &str) -> bool {
    j.validate().is_ok()
        && j.phase != Phase::Closed
        && lease.session == j.session
        && lease.identity == *current
        && lease.binary_sha256.eq_ignore_ascii_case(hash)
        && j.helpers.iter().any(|h| {
            h.role == lease.role
                && h.identity == lease.identity
                && h.binary_sha256 == lease.binary_sha256
        })
}
#[allow(dead_code)]
pub fn restoration_batch<T, F, G>(steps: &[T], mut closed: F, mut restore: G) -> Result<Vec<String>>
where
    F: FnMut() -> Result<bool>,
    G: FnMut(&T) -> Result<()>,
{
    let mut failures = Vec::new();
    for step in steps {
        let blocked = match closed() {
            Ok(true) => None,
            Ok(false) => Some("Game is running; restoration aborted".to_string()),
            Err(e) => Some(format!("Game presence unknown; restoration aborted: {e:#}")),
        };
        if let Some(blocked) = blocked {
            anyhow::bail!(
                "{blocked}. Prior restoration failures: {}",
                failures.join("; ")
            );
        }
        if let Err(e) = restore(step) {
            failures.push(format!("{e:#}"));
        }
    }
    Ok(failures)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Boundary {
    Unknown,
    MainMenu,
    QuickPlay,
    Selection,
    Match,
    OtherMode,
}
#[derive(Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Decision {
    Hold,
    Arm(u64),
    Active,
    Restore,
    Waiting,
}
#[derive(Default)]
#[allow(dead_code)]
pub struct Rearm {
    epoch: u64,
    pending: bool,
    armed: bool,
    active: bool,
    restore_pending: bool,
}
#[allow(dead_code)]
impl Rearm {
    pub fn observe(&mut self, b: Boundary) -> Decision {
        if b == Boundary::Unknown {
            return Decision::Hold;
        }
        if matches!(
            b,
            Boundary::MainMenu | Boundary::QuickPlay | Boundary::OtherMode
        ) {
            if self.pending || self.armed || self.active {
                self.restore_pending = true;
            }
            return if self.restore_pending {
                Decision::Restore
            } else {
                Decision::Waiting
            };
        }
        if self.restore_pending {
            return Decision::Restore;
        }
        if b == Boundary::Match {
            return if self.armed {
                self.active = true;
                Decision::Active
            } else {
                Decision::Hold
            };
        }
        if self.active {
            self.restore_pending = true;
            return Decision::Restore;
        }
        if self.armed || self.pending {
            return Decision::Hold;
        }
        self.epoch += 1;
        self.pending = true;
        Decision::Arm(self.epoch)
    }
    pub fn armed(&mut self, epoch: u64, success: bool) -> Result<()> {
        ensure!(
            self.pending && self.epoch == epoch && !self.restore_pending,
            "Stale arm acknowledgement"
        );
        self.pending = false;
        self.armed = success;
        Ok(())
    }
    pub fn restored(&mut self, success: bool) -> Result<()> {
        ensure!(self.restore_pending, "No pending restoration");
        if success {
            self.pending = false;
            self.armed = false;
            self.active = false;
            self.restore_pending = false;
        }
        Ok(())
    }
}
pub fn smoke_fixture(root: &Path) -> Result<Value> {
    let dir = root.join("Cache/SessionQA");
    let game = dir.join("Steam/steamapps/common/NBA 2K27");
    fs::create_dir_all(&game)?;
    fs::write(
        game.join("NBA2K27.exe"),
        b"QA fixture only, never executable",
    )?;
    fs::write(
        dir.join("Steam/steam.exe"),
        b"QA fixture only, never executable",
    )?;
    fs::write(
        dir.join("Steam/steamapps/appmanifest_123456.acf"),
        b"\"AppState\" { \"appid\" \"123456\" \"name\" \"NBA 2K27\" \"installdir\" \"NBA 2K27\" }",
    )?;
    let mut report = preflight_with(&game, root, Ok(vec![]));
    report["fixture"] = json!(true);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn promoted_main_name_remains_a_launcher_not_a_guard_exception() {
        assert!(launcher_name("2k Launcher Rust.exe"));
        assert!(launcher_name("2K LAUNCHER RUST.EXE"));
        assert!(!launcher_name("startup-audio-guardian.exe"));
        assert!(!launcher_name("not-2k Launcher Rust.exe"));
    }
    fn owned() -> Journal {
        Journal {
            version: 1,
            session: "a".repeat(48),
            phase: Phase::Running,
            game: None,
            files: vec![],
            launch: None,
            helpers: vec![HelperLease {
                session: "a".repeat(48),
                role: "ball".into(),
                identity: Identity {
                    pid: 42,
                    started: "123456789012345678".into(),
                    path: r"C:\Launcher\App\ball.exe".into(),
                },
                binary_sha256: "b".repeat(64),
            }],
        }
    }
    #[test]
    fn secondary_library_resolves_only_through_listing_steam_install() {
        let root = std::env::temp_dir().join(format!(
            "launcher-secondary-library-{}",
            crate::backend::unique_id()
        ));
        let steam = root.join("Steam");
        let library = root.join("SteamLibrary");
        let game = library.join("steamapps/common/NBA 2K27");
        fs::create_dir_all(steam.join("steamapps")).unwrap();
        fs::create_dir_all(&game).unwrap();
        fs::write(steam.join("steam.exe"), b"QA fixture only").unwrap();
        fs::write(game.join("NBA2K27.exe"), b"QA fixture only").unwrap();
        fs::write(
            library.join("steamapps/appmanifest_123456.acf"),
            b"\"AppState\" { \"appid\" \"123456\" \"name\" \"NBA 2K27\" \"installdir\" \"NBA 2K27\" }",
        )
        .unwrap();
        let listing = |apps: &str| {
            let escaped = library.to_string_lossy().replace('\\', "\\\\");
            fs::write(
                steam.join("steamapps/libraryfolders.vdf"),
                format!("\"libraryfolders\" {{ \"0\" {{ \"path\" \"{}\" }} \"1\" {{ \"path\" \"{escaped}\" \"apps\" {{ {apps} }} }} }}", steam.to_string_lossy().replace('\\', "\\\\")),
            )
            .unwrap();
        };
        // Not listed by any Steam install: still refused.
        assert!(resolve_with(&game, Vec::new).is_err());
        listing("\"123456\" \"1\"");
        let plan = resolve_with(&game, || vec![steam.clone()]).unwrap();
        assert_eq!(plan["appId"], "123456");
        assert_eq!(
            plan["steamExe"].as_str().unwrap(),
            steam
                .join("steam.exe")
                .canonicalize()
                .unwrap()
                .to_str()
                .unwrap()
        );
        // Listed library whose app list omits the game: refused.
        listing("\"999\" \"1\"");
        assert!(resolve_with(&game, || vec![steam.clone()]).is_err());
        // Exact install-dir identity still applies in secondary libraries.
        listing("\"123456\" \"1\"");
        let renamed = library.join("steamapps/common/NBA 2K27 Copy");
        fs::create_dir_all(&renamed).unwrap();
        fs::write(renamed.join("NBA2K27.exe"), b"QA fixture only").unwrap();
        assert!(resolve_with(&renamed, || vec![steam.clone()]).is_err());
        assert_eq!(
            crate::steam_libraries::candidate_game_roots(&[steam.clone()])
                .iter()
                .map(|p| p.canonicalize().unwrap())
                .collect::<Vec<_>>(),
            vec![game.canonicalize().unwrap()]
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn manifest_parsing_and_ambiguous_fields() {
        let text = "\"AppState\" { \"appid\" \"123\" \"name\" \"NBA 2K27\" \"installdir\" \"NBA 2K27\" \"nested\" {\"appid\" \"999\"} }";
        assert_eq!(manifest(text).unwrap().app_id, "123");
        assert_eq!(manifest(&format!("\u{feff}{text}")).unwrap().app_id, "123");
        for t in [
            text.replace("\"123\"", "\"12x\""),
            text.replace("\"123\"", "\"0\""),
            text.replace("\"123\"", "\"0123\""),
            text.replace("\"123\"", "\"4294967296\""),
            text.replace(
                "\"installdir\" \"NBA 2K27\"",
                "\"installdir\" \"../NBA 2K27\"",
            ),
            text.trim_end_matches('}').into(),
            format!("{text} garbage"),
            text.replace("\"appid\" \"123\"", "\"appid\" \"123\" \"APPID\" \"456\""),
            text.replace(
                "\"appid\" \"123\"",
                "\"appid\" { \"x\" \"1\" } \"APPID\" \"123\"",
            ),
        ] {
            assert!(manifest(&t).is_err(), "{t}");
        }
        assert!(
            manifest("\"AppState\" {\"appid\" \"1\" \"name\" \"NBA 2K27\" \"installdir\" \"..\"}")
                .is_err()
        );
    }
    #[test]
    fn process_identity_is_read_only_and_birth_time_is_exact() {
        let i = identity(std::process::id()).unwrap();
        assert_eq!(i.pid, std::process::id());
        assert_eq!(i, identity(i.pid).unwrap());
        assert!(i.started.parse::<u64>().unwrap() > 0);
        assert!(processes().is_ok());
    }
    #[test]
    fn helper_pid_reuse_foreign_path_hash_and_session_are_rejected() {
        let mut j = owned();
        let lease = j.helpers[0].clone();
        assert!(owns_helper(
            &j,
            &lease,
            &lease.identity,
            &lease.binary_sha256
        ));
        for (pid, started, path) in [
            (43, "123456789012345678", lease.identity.path.as_str()),
            (42, "123456789012345679", lease.identity.path.as_str()),
            (42, "123456789012345678", r"C:\Other\ball.exe"),
        ] {
            assert!(!owns_helper(
                &j,
                &lease,
                &Identity {
                    pid,
                    started: started.into(),
                    path: path.into()
                },
                &lease.binary_sha256
            ));
        }
        assert!(!owns_helper(&j, &lease, &lease.identity, &"c".repeat(64)));
        j.session = "c".repeat(48);
        assert!(!owns_helper(
            &j,
            &lease,
            &lease.identity,
            &lease.binary_sha256
        ));
        j = owned();
        j.phase = Phase::Closed;
        assert!(!owns_helper(
            &j,
            &lease,
            &lease.identity,
            &lease.binary_sha256
        ));
    }
    #[test]
    fn journal_paths_limits_and_original_preservation() {
        let mut j = owned();
        for target in [
            "../version.dll",
            "C:/version.dll",
            "version.dll:stream",
            "/version.dll",
            "a/../../b",
            "a\\b",
            "version.dll.",
            "version.dll ",
            "NUL.bin",
            "data/COM1",
            "LPT¹.txt",
            "a/*.bin",
        ] {
            j.files = vec![FileLease {
                target: target.into(),
                backup: None,
                original_sha256: None,
                installed_sha256: "a".repeat(64),
            }];
            assert!(j.validate().is_err(), "{target}");
        }
        j.files = vec![FileLease {
            target: "version.dll".into(),
            backup: Some("backups/original.bin".into()),
            original_sha256: None,
            installed_sha256: "a".repeat(64),
        }];
        assert!(j.validate().is_err());
        let root =
            std::env::temp_dir().join(format!("launcher-journal-{}", crate::backend::unique_id()));
        fs::create_dir_all(&root).unwrap();
        let file = root.join("session.json");
        fs::write(&file, b"{broken").unwrap();
        assert!(read_journal(&file).is_err());
        assert_eq!(fs::read(&file).unwrap(), b"{broken");
        fs::write(&file, vec![b' '; 1024 * 1024 + 1]).unwrap();
        assert!(read_journal(&file).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn restoration_rechecks_each_step_and_collects_independent_errors() {
        let checks = std::cell::Cell::new(0);
        let mut done = Vec::new();
        let result = restoration_batch(
            &[1, 2, 3],
            || {
                checks.set(checks.get() + 1);
                Ok(checks.get() < 3)
            },
            |i| {
                done.push(*i);
                if *i == 1 {
                    anyhow::bail!("First restoration failed");
                }
                Ok(())
            },
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("First restoration failed")
        );
        assert_eq!(done, vec![1, 2]);
        assert_eq!(checks.get(), 3);
        assert_eq!(
            restoration_batch(&[1, 2], || Ok(true), |_| anyhow::bail!("failed"))
                .unwrap()
                .len(),
            2
        );
        let mut writes = 0;
        assert!(
            restoration_batch(
                &[1],
                || anyhow::bail!("Unknown process state"),
                |_| {
                    writes += 1;
                    Ok(())
                }
            )
            .is_err()
        );
        assert_eq!(writes, 0);
    }
    #[test]
    fn rearm_at_main_quick_play_and_next_selection() {
        for exit in [
            Boundary::MainMenu,
            Boundary::QuickPlay,
            Boundary::OtherMode,
            Boundary::Selection,
        ] {
            let mut r = Rearm::default();
            assert_eq!(r.observe(Boundary::Unknown), Decision::Hold);
            assert_eq!(r.observe(Boundary::Selection), Decision::Arm(1));
            assert_eq!(r.observe(Boundary::Selection), Decision::Hold);
            r.armed(1, true).unwrap();
            assert_eq!(r.observe(Boundary::Match), Decision::Active);
            assert_eq!(r.observe(exit), Decision::Restore);
            r.restored(false).unwrap();
            assert_eq!(r.observe(Boundary::Selection), Decision::Restore);
            r.restored(true).unwrap();
            assert_eq!(r.observe(Boundary::Selection), Decision::Arm(2));
            assert!(r.armed(1, true).is_err());
        }
    }
    #[test]
    fn failed_arm_unknown_boundary_and_stale_ack_never_activate() {
        let mut r = Rearm::default();
        assert_eq!(r.observe(Boundary::Match), Decision::Hold);
        assert!(r.restored(true).is_err());
        assert_eq!(r.observe(Boundary::Selection), Decision::Arm(1));
        r.armed(1, false).unwrap();
        assert_eq!(r.observe(Boundary::Selection), Decision::Arm(2));
        assert_eq!(r.observe(Boundary::Unknown), Decision::Hold);
        assert_eq!(r.observe(Boundary::QuickPlay), Decision::Restore);
        assert!(r.armed(2, true).is_err());
    }
    #[test]
    fn preflight_fixture_unknown_process_and_foreign_proxy() {
        let root = std::env::temp_dir().join(format!(
            "launcher-preflight-{}",
            crate::backend::unique_id()
        ));
        fs::create_dir_all(root.join("Data")).unwrap();
        let report = smoke_fixture(&root).unwrap();
        assert_eq!(report["checksPassed"], true);
        assert_eq!(report["liveGameEnabled"], false);
        let game = root.join("Cache/SessionQA/Steam/steamapps/common/NBA 2K27");
        let unknown = preflight_with(&game, &root, Err(anyhow::anyhow!("probe failure")));
        assert_eq!(unknown["checksPassed"], false);
        assert!(unknown["gameRunning"].is_null());
        fs::write(game.join("version.dll"), b"foreign user file").unwrap();
        assert_eq!(
            preflight_with(&game, &root, Ok(vec![]))["checksPassed"],
            false
        );
        assert_eq!(
            fs::read(game.join("version.dll")).unwrap(),
            b"foreign user file"
        );
        fs::write(root.join("Data/session-journal.json"), b"{bad").unwrap();
        assert_eq!(
            preflight_with(&game, &root, Ok(vec![]))["recoveryRequired"],
            true
        );
        assert_eq!(
            fs::read(root.join("Data/session-journal.json")).unwrap(),
            b"{bad"
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn journal_roundtrip_keeps_exact_process_birth_and_pending_recovery() {
        let root = std::env::temp_dir().join(format!(
            "launcher-journal-roundtrip-{}",
            crate::backend::unique_id()
        ));
        fs::create_dir_all(root.join("Data")).unwrap();
        smoke_fixture(&root).unwrap();
        let file = root.join("Data/session-journal.json");
        let mut j = owned();
        j.phase = Phase::CleanupPending;
        crate::backend::atomic_json(&file, &serde_json::to_value(&j).unwrap()).unwrap();
        let read = read_journal(&file).unwrap();
        assert_eq!(read.helpers[0].identity.started, "123456789012345678");
        assert_eq!(read.phase, Phase::CleanupPending);
        let game = root.join("Cache/SessionQA/Steam/steamapps/common/NBA 2K27");
        assert_eq!(
            preflight_with(&game, &root, Ok(vec![]))["recoveryRequired"],
            true
        );
        j.phase = Phase::Closed;
        crate::backend::atomic_json(&file, &serde_json::to_value(&j).unwrap()).unwrap();
        assert_eq!(
            preflight_with(&game, &root, Ok(vec![]))["checksPassed"],
            true
        );
        let mut json = serde_json::to_value(&j).unwrap();
        json["extra"] = json!("not allowed");
        fs::write(&file, serde_json::to_vec(&json).unwrap()).unwrap();
        assert!(read_journal(&file).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn malformed_rule_journal_blocks_preflight_and_stays_untouched() {
        let root =
            std::env::temp_dir().join(format!("rules-preflight-{}", crate::backend::unique_id()));
        fs::create_dir_all(root.join("Data")).unwrap();
        smoke_fixture(&root).unwrap();
        let game = root.join("Cache/SessionQA/Steam/steamapps/common/NBA 2K27");
        for p in [
            crate::game_rules::path(&root),
            crate::player_transaction::path(&root),
            crate::team_transaction::path(&root),
            crate::uniform_transaction::path(&root),
            root.join("Data/blacktop-arena-session.json"),
            root.join("Data").join(crate::outfit_journal::MARKER),
            root.join("Data/loader-outfit-v2-session.json"),
            root.join("Data")
                .join(format!("blacktop-outfit-v2-{}.journal", "a".repeat(48))),
            root.join("Data")
                .join(format!("blacktop-outfit-v2-{}.lease", "a".repeat(48))),
        ] {
            fs::write(&p, b"{unknown").unwrap();
            let report = preflight_with(&game, &root, Ok(vec![]));
            assert_eq!(report["checksPassed"], false);
            assert_eq!(report["recoveryRequired"], true);
            assert_eq!(fs::read(&p).unwrap(), b"{unknown");
            fs::remove_file(p).unwrap();
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn preflight_running_game_anticheat_and_duplicate_manifest_are_blocked() {
        let root = std::env::temp_dir().join(format!(
            "launcher-preflight-blocks-{}",
            crate::backend::unique_id()
        ));
        fs::create_dir_all(&root).unwrap();
        smoke_fixture(&root).unwrap();
        let game = root.join("Cache/SessionQA/Steam/steamapps/common/NBA 2K27");
        for name in ["NBA2K27.exe", "EasyAntiCheat_EOS.exe", "EACLaunch.exe"] {
            let report = preflight_with(
                &game,
                &root,
                Ok(vec![Process {
                    name: name.into(),
                    pid: 123,
                    parent: 1,
                    identity: None,
                }]),
            );
            assert_eq!(report["checksPassed"], false);
            assert_eq!(report["liveGameEnabled"], false);
            assert!(!report["blockers"].as_array().unwrap().is_empty());
        }
        let apps = game.parent().unwrap().parent().unwrap();
        fs::write(apps.join("appmanifest_123457.acf"), b"\"AppState\" { \"appid\" \"123457\" \"name\" \"NBA 2K27\" \"installdir\" \"NBA 2K27\" }").unwrap();
        assert!(resolve(&game).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn another_launcher_and_known_backend_block_preflight() {
        let node = |path: Option<&str>| Process {
            name: "node.exe".into(),
            pid: 77,
            parent: 1,
            identity: path.map(|p| Identity {
                pid: 77,
                started: "123".into(),
                path: p.into(),
            }),
        };
        assert!(launcher_process(&node(Some(
            r"C:\Portable\App\Backend\node.exe"
        ))));
        assert!(!launcher_process(&node(Some(r"C:\Tools\Node\node.exe"))));
        assert!(launcher_process(&node(None)));
        let root =
            std::env::temp_dir().join(format!("launcher-overlap-{}", crate::backend::unique_id()));
        fs::create_dir_all(&root).unwrap();
        smoke_fixture(&root).unwrap();
        let game = root.join("Cache/SessionQA/Steam/steamapps/common/NBA 2K27");
        let report = preflight_with(
            &game,
            &root,
            Ok(vec![Process {
                name: "2k Launcher WPF.exe".into(),
                pid: 88,
                parent: 1,
                identity: None,
            }]),
        );
        assert_eq!(report["checksPassed"], false);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn snapshot_skips_only_proved_dead_processes_and_preserves_unreadable_owners() {
        assert!(snapshot_identity(Ok(IdentityQuery::Exited)).is_none());
        assert!(
            snapshot_identity(Err(anyhow::anyhow!("access denied")))
                .unwrap()
                .is_none()
        );
        let live = identity(std::process::id()).unwrap();
        assert_eq!(
            snapshot_identity(Ok(IdentityQuery::Live(live.clone()))).unwrap(),
            Some(live)
        );
        let unresolved = Process {
            name: "node.exe".into(),
            pid: 77,
            parent: 1,
            identity: None,
        };
        assert!(launcher_process(&unresolved));
    }
    #[test]
    fn metadata_teardown_grace_only_clears_a_proven_retained_handle_exit() {
        let gone = finish_metadata::<()>(Err(anyhow::anyhow!("path denied")), |timeout| {
            assert_eq!(timeout, 30);
            Ok(true)
        })
        .unwrap();
        assert!(gone.is_none());
        let active = finish_metadata::<()>(Err(anyhow::anyhow!("path denied")), |timeout| {
            assert_eq!(timeout, METADATA_EXIT_GRACE_MS);
            Ok(false)
        });
        assert_eq!(active.err().unwrap().to_string(), "path denied");
        let unknown = finish_metadata::<()>(Err(anyhow::anyhow!("path denied")), |_| {
            anyhow::bail!("retained wait failed")
        });
        assert_eq!(unknown.err().unwrap().to_string(), "retained wait failed");
        assert_eq!(
            finish_metadata(Ok(123), |timeout| {
                assert_eq!(timeout, 0);
                Ok(false)
            })
            .unwrap(),
            Some(123)
        );
        assert!(
            finish_metadata(Ok(123), |timeout| {
                assert_eq!(timeout, 0);
                Ok(true)
            })
            .unwrap()
            .is_none()
        );
    }
    #[test]
    fn metadata_error_on_a_real_owned_wait_handle_keeps_live_process_blocked() {
        use std::{
            os::windows::{io::AsRawHandle, process::CommandExt},
            process::{Command, Stdio},
        };
        let mut child = Command::new("cmd.exe")
            .args(["/d", "/q", "/c", "set /p fixture_value="])
            .creation_flags(0x08000000)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let handle = HANDLE(child.as_raw_handle());
        let live = finish_metadata::<()>(Err(anyhow::anyhow!("synthetic denied")), |timeout| {
            metadata_ended_wait(handle, true, timeout)
        });
        assert_eq!(live.err().unwrap().to_string(), "synthetic denied");
        assert!(matches!(
            unsafe { WaitForSingleObject(handle, 0) },
            WAIT_TIMEOUT
        ));
        // Closing this fixture's own input ends it normally; no termination API.
        drop(child.stdin.take());
        child.wait().unwrap();
        assert!(
            finish_metadata::<()>(Err(anyhow::anyhow!("synthetic denied")), |timeout| {
                metadata_ended_wait(handle, true, timeout)
            })
            .unwrap()
            .is_none()
        );
    }
    fn test_snapshot(name: &str, pid: u32) -> Vec<SnapshotRow> {
        vec![SnapshotRow {
            name: name.into(),
            pid,
            parent: 1,
        }]
    }
    #[cfg(target_pointer_width = "64")]
    #[test]
    fn native_snapshot_decoder_bounds_names_offsets_and_process_ids() {
        let base = 0x10000usize;
        let mut b = vec![0u8; 120];
        b[56..58].copy_from_slice(&8u16.to_le_bytes());
        b[58..60].copy_from_slice(&10u16.to_le_bytes());
        b[64..72].copy_from_slice(&((base + 96) as u64).to_le_bytes());
        b[80..88].copy_from_slice(&77u64.to_le_bytes());
        b[88..96].copy_from_slice(&12u64.to_le_bytes());
        for (i, c) in "test".encode_utf16().enumerate() {
            b[96 + i * 2..98 + i * 2].copy_from_slice(&c.to_le_bytes());
        }
        let rows = decode_system_snapshot(&b, base).unwrap();
        assert_eq!(
            (&*rows[0].name, rows[0].pid, rows[0].parent),
            ("test", 77, 12)
        );
        for (offset, bad) in [
            (0, 4u64),
            (0, 128),
            (56, 9),
            (58, 6),
            (64, (base - 1) as u64),
            (64, (base + 10) as u64),
            (64, (base + 118) as u64),
            (80, u32::MAX as u64 + 1),
        ] {
            let mut bad_bytes = b.clone();
            let width = if offset == 0 {
                4
            } else if offset == 56 || offset == 58 {
                2
            } else {
                8
            };
            bad_bytes[offset..offset + width].copy_from_slice(&bad.to_le_bytes()[..width]);
            assert!(
                decode_system_snapshot(&bad_bytes, base).is_err(),
                "accepted bad field {offset}"
            );
        }
        assert!(decode_system_snapshot(&b[..90], base).is_err());
        let mut nul = b.clone();
        nul[96..98].fill(0);
        assert!(decode_system_snapshot(&nul, base).is_err());
    }
    #[cfg(target_pointer_width = "64")]
    #[test]
    fn native_snapshot_transport_matches_toolhelp_for_stable_live_processes() {
        // Metadata only: no process writes, allocation, termination or launch.
        let native = system_snapshot_rows().unwrap();
        let toolhelp = toolhelp_snapshot_rows().unwrap();
        let own = std::process::id();
        let a = native.iter().find(|r| r.pid == own).unwrap();
        let b = toolhelp.iter().find(|r| r.pid == own).unwrap();
        assert_eq!((&a.name, a.parent), (&b.name, b.parent));
        assert!(native.len() > 1);
        // Rows may legitimately start/exit between the two fresh snapshots.
        // At minimum the current process and its retained identity must agree.
        let exact = identity(own).unwrap();
        assert!(
            exact
                .path
                .to_ascii_lowercase()
                .ends_with(&a.name.to_ascii_lowercase())
        );
    }
    #[cfg(target_pointer_width = "64")]
    #[test]
    #[ignore = "explicit offline process-enumeration benchmark; no game access"]
    fn qa054_process_snapshot_benchmark() {
        let sample = |native: bool| {
            let start = std::time::Instant::now();
            let mut count = 0;
            for _ in 0..100 {
                let rows = if native {
                    system_snapshot_rows()
                } else {
                    toolhelp_snapshot_rows()
                }
                .unwrap();
                count += rows.len();
                let scan = inspect_snapshot(rows, queried_identity, crate::provider_worker::owned);
                assert!(!scan.rows.iter().any(|r| r.pid == std::process::id()));
            }
            (start.elapsed().as_micros(), count)
        };
        let old = sample(false);
        let new = sample(true);
        println!(
            "QA054_SNAPSHOT_BENCH {}",
            serde_json::json!({"samples":100,
            "toolhelpMicroseconds":old.0,"nativeMicroseconds":new.0,
            "toolhelpRows":old.1,"nativeRows":new.1,
            "freshBeforeEveryMutation":true,"identityQueriesUnchanged":true})
        );
    }
    #[test]
    fn unreadable_stale_node_is_removed_only_by_a_successful_fresh_snapshot() {
        let mut count = 0;
        let scan = refresh_unresolved(|| {
            count += 1;
            Ok(inspect_snapshot(
                if count == 1 {
                    test_snapshot("node.exe", 8312)
                } else {
                    vec![]
                },
                |_| anyhow::bail!("QueryFullProcessImageNameW: access denied (0x80070005)"),
                |_| false,
            ))
        })
        .unwrap();
        assert_eq!(count, 2);
        assert!(scan.rows.is_empty());
        assert!(scan.refreshed);
        assert!(scan.issues.is_empty());
        assert_eq!(scan.prior_issues[0].pid, 8312);
        assert!(scan.prior_issues[0].error.contains("0x80070005"));
    }
    #[test]
    fn refresh_does_not_exempt_an_unreadable_node_that_still_exists() {
        let mut count = 0;
        let scan = refresh_unresolved(|| {
            count += 1;
            Ok(inspect_snapshot(
                test_snapshot("node.exe", 8312),
                |_| anyhow::bail!("denied"),
                |_| false,
            ))
        })
        .unwrap();
        assert_eq!(count, 2); // Bounded, never spin until a blocker disappears.
        assert!(scan.refreshed);
        assert_eq!(scan.issues.len(), 1);
        assert_eq!(scan.rows.len(), 1);
        assert!(launcher_process(&scan.rows[0]));
        assert!(scan.rows[0].identity.is_none());
    }
    #[test]
    fn refresh_inspects_new_owners_and_reused_pids_instead_of_whitelisting_old_pid() {
        for replacement in [
            "2k Launcher WPF.exe",
            "NBA2K27.exe",
            "EasyAntiCheat_EOS.exe",
        ] {
            let mut count = 0;
            let scan = refresh_unresolved(|| {
                count += 1;
                Ok(inspect_snapshot(
                    test_snapshot(if count == 1 { "node.exe" } else { replacement }, 8312),
                    |_| anyhow::bail!("denied"),
                    |_| false,
                ))
            })
            .unwrap();
            assert_eq!(scan.rows.len(), 1);
            assert_eq!(scan.rows[0].name, replacement);
            assert_eq!(scan.rows[0].pid, 8312);
        }
        let mut count = 0;
        let scan = refresh_unresolved(|| {
            count += 1;
            Ok(inspect_snapshot(
                test_snapshot("node.exe", if count == 1 { 8312 } else { 9000 }),
                |_| anyhow::bail!("denied"),
                |_| false,
            ))
        })
        .unwrap();
        assert_eq!(scan.rows[0].pid, 9000);
        assert!(launcher_process(&scan.rows[0]));
    }
    #[test]
    fn failed_fresh_snapshot_cannot_clear_the_original_unknown_owner() {
        let mut count = 0;
        let result = refresh_unresolved(|| {
            count += 1;
            ensure!(count == 1, "Toolhelp enumeration failed");
            Ok(inspect_snapshot(
                test_snapshot("node.exe", 8312),
                |_| anyhow::bail!("denied"),
                |_| false,
            ))
        });
        assert!(format!("{:#}", result.err().unwrap()).contains("Toolhelp enumeration failed"));
        assert_eq!(count, 2);
    }
    #[test]
    fn refreshed_node_uses_its_new_exact_identity_to_classify_backend() {
        for path in [r"C:\Tools\node.exe", r"C:\Launcher\App\Backend\node.exe"] {
            let mut count = 0;
            let scan = refresh_unresolved(|| {
                count += 1;
                Ok(inspect_snapshot(
                    test_snapshot("node.exe", 8312),
                    |pid| {
                        ensure!(count == 2, "transient path query failure");
                        Ok(IdentityQuery::Live(Identity {
                            pid,
                            started: "12345".into(),
                            path: path.into(),
                        }))
                    },
                    |_| false,
                ))
            })
            .unwrap();
            assert_eq!(count, 2);
            assert!(scan.issues.is_empty());
            assert_eq!(scan.rows.len(), usize::from(path.contains("Backend")));
        }
    }
    #[test]
    fn known_live_and_confirmed_exited_scans_do_not_retry_or_sleep() {
        for exited in [true, false] {
            let mut count = 0;
            let scan = refresh_unresolved(|| {
                count += 1;
                Ok(inspect_snapshot(
                    test_snapshot("node.exe", 8312),
                    |pid| {
                        Ok(if exited {
                            IdentityQuery::Exited
                        } else {
                            IdentityQuery::Live(Identity {
                                pid,
                                started: "12345".into(),
                                path: r"C:\Tools\node.exe".into(),
                            })
                        })
                    },
                    |_| false,
                ))
            })
            .unwrap();
            assert_eq!(count, 1);
            assert!(!scan.refreshed);
            assert!(scan.rows.is_empty());
        }
    }
    #[test]
    fn snapshot_inspection_queries_only_relevant_nonself_processes() {
        let scan = inspect_snapshot(
            vec![
                SnapshotRow {
                    name: "node.exe".into(),
                    pid: std::process::id(),
                    parent: 1,
                },
                SnapshotRow {
                    name: "notepad.exe".into(),
                    pid: 100,
                    parent: 1,
                },
            ],
            |_| panic!("must not inspect unrelated or self metadata"),
            |_| false,
        );
        assert!(scan.rows.is_empty());
        assert!(scan.issues.is_empty());
    }
    #[test]
    fn exited_owned_fixture_is_not_reported_as_an_unknown_active_process() {
        use std::os::windows::process::CommandExt;
        for code in [0, 259] {
            let mut child = std::process::Command::new("cmd.exe")
                .args(["/c", "exit", &code.to_string()])
                .creation_flags(0x08000000)
                .spawn()
                .unwrap();
            assert_eq!(child.wait().unwrap().code(), Some(code));
            assert!(matches!(
                queried_identity(child.id()).unwrap(),
                IdentityQuery::Exited
            ));
            assert!(identity(child.id()).is_err());
        }
    }
    #[test]
    fn failed_liveness_checks_never_prove_process_absence() {
        assert!(metadata_ended(HANDLE::default(), true).is_err());
        assert!(metadata_ended(HANDLE::default(), false).is_err());
        let h = Handle(unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                false,
                std::process::id(),
            )
            .unwrap()
        });
        assert!(!metadata_ended(h.0, true).unwrap());
        assert!(!metadata_ended(h.0, false).unwrap());
    }
    #[test]
    fn cleanup_is_shared_retryable_and_success_is_not_repeated() {
        let mut g = CleanupGate::default();
        assert_eq!(g.begin(), CleanupDecision::Run);
        assert_eq!(g.begin(), CleanupDecision::Pending);
        g.finish(false).unwrap();
        assert_eq!(g.begin(), CleanupDecision::Run);
        g.finish(true).unwrap();
        assert_eq!(g.begin(), CleanupDecision::Completed);
        assert!(g.finish(true).is_err());
    }
    #[test]
    fn file_restoration_plans_preserve_user_edits_and_validate_backups() {
        use sha2::{Digest, Sha256};
        let hash = |b: &[u8]| format!("{:x}", Sha256::digest(b));
        let root = std::env::temp_dir().join(format!(
            "launcher-restore-plan-{}",
            crate::backend::unique_id()
        ));
        let game = root.join("game");
        let portable = root.join("portable");
        fs::create_dir_all(&game).unwrap();
        let mut j = owned();
        let backup = format!("Cache/SessionBackups/{}/original.bin", j.session);
        let full = portable.join(&backup);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(&full, b"original").unwrap();
        fs::write(game.join("version.dll"), b"owned patch").unwrap();
        let f = FileLease {
            target: "version.dll".into(),
            backup: Some(backup),
            original_sha256: Some(hash(b"original")),
            installed_sha256: hash(b"owned patch"),
        };
        j.files = vec![f.clone()];
        assert_eq!(
            plan_file_restore(&j, &f, &game, &portable).unwrap(),
            RestorePlan::RestoreBackup
        );
        fs::write(game.join("version.dll"), b"user changed").unwrap();
        assert_eq!(
            plan_file_restore(&j, &f, &game, &portable).unwrap(),
            RestorePlan::Hold
        );
        fs::write(game.join("version.dll"), b"original").unwrap();
        assert_eq!(
            plan_file_restore(&j, &f, &game, &portable).unwrap(),
            RestorePlan::AlreadyOriginal
        );
        fs::write(game.join("version.dll"), b"owned patch").unwrap();
        fs::write(&full, b"damaged backup").unwrap();
        assert_eq!(
            plan_file_restore(&j, &f, &game, &portable).unwrap(),
            RestorePlan::Hold
        );
        assert_eq!(fs::read(game.join("version.dll")).unwrap(), b"owned patch");
        let new = FileLease {
            target: "version.dll".into(),
            backup: None,
            original_sha256: None,
            installed_sha256: hash(b"owned patch"),
        };
        j.files = vec![new.clone()];
        assert_eq!(
            plan_file_restore(&j, &new, &game, &portable).unwrap(),
            RestorePlan::RemoveCreated
        );
        assert!(game.join("version.dll").exists());
        fs::remove_file(game.join("version.dll")).unwrap();
        assert_eq!(
            plan_file_restore(&j, &new, &game, &portable).unwrap(),
            RestorePlan::AlreadyOriginal
        );
        fs::remove_dir_all(root).unwrap();
    }
}
