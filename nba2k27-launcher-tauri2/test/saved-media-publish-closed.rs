//! Engineering publisher process guard; metadata only, no process termination.
//! A stale runtime row may be retried ONLY with exact fresh PID absence and,
//! when we retained a handle, same-handle signaled exit. Every fresh snapshot is
//! revalidated in full. Unknown/live metadata failures remain fail-closed.
use anyhow::{Context, Result, bail, ensure};

#[derive(Clone)]
pub(crate) struct Row {
    pub pid: u32,
    pub name: String,
}
pub(crate) enum Runtime {
    Path(String),
    Unknown {
        // None means opening a retained handle failed. Only a complete fresh
        // snapshot proving this exact PID absent can resolve that stale row.
        exited: Option<bool>,
        error: anyhow::Error,
    },
}
pub(crate) trait Access {
    fn snapshot(&mut self) -> Result<Vec<Row>>;
    fn runtime(&mut self, pid: u32) -> Result<Runtime>;
}
fn blocked(name: &str) -> bool {
    name == "nba2k27.exe"
        || name.starts_with("easyanticheat")
        || matches!(
            name,
            "eaclaunch.exe"
                | "start_protected_game.exe"
                | "2k launcher.exe"
                | "2k launcher wpf.exe"
                | "2k launcher rust.exe"
                | "2k launcher rust prototype.exe"
                | "nba2k27-launcher-tauri2.exe"
        )
        || (name.starts_with("2k-launcher-") && name.ends_with(".exe"))
}
fn runtime_path(name: &str, path: &str) -> Result<()> {
    let path = path.replace('/', "\\").to_ascii_lowercase();
    ensure!(
        path.len() > 3
            && path.as_bytes()[1] == b':'
            && path.as_bytes()[2] == b'\\'
            && path.rsplit('\\').next() == Some(name),
        "Runtime snapshot name differs from retained image; ownership held"
    );
    ensure!(
        !path.ends_with("\\app\\backend\\node.exe")
            && !path.contains("\\nba2k27-mod-launcher\\")
            && !path.contains("\\2k launcher\\app\\"),
        "Close the old launcher runtime before publishing media"
    );
    Ok(())
}
pub(crate) fn check(access: &mut impl Access, own_pid: u32) -> Result<()> {
    let mut rows = access.snapshot().context("Taking publisher closed-process snapshot")?;
    // At most three proven vanished-row races per guard call. Never hide an
    // endless unstable process inventory by retrying without a bound.
    for retries in 0..=3 {
        let mut restart = None;
        for row in rows.iter().filter(|r| r.pid != own_pid) {
            let name = row.name.to_ascii_lowercase();
            ensure!(
                !blocked(&name),
                "Close game/protected process/launcher before publication: {name} PID {}",
                row.pid
            );
            if name != "node.exe" && name != "electron.exe" {
                continue;
            }
            match access.runtime(row.pid).with_context(|| {
                format!("Inspecting retained {name} PID {} metadata", row.pid)
            })? {
                Runtime::Path(path) => runtime_path(&name, &path).with_context(|| {
                    format!("Classifying retained {name} PID {}", row.pid)
                })?,
                Runtime::Unknown { exited, error } => {
                    if exited == Some(false) {
                        return Err(error).with_context(|| {
                            format!("Live {name} PID {} identity unavailable; publication held", row.pid)
                        });
                    }
                    let fresh = access.snapshot().with_context(|| {
                        format!("Proving exact failed {name} PID {} absent", row.pid)
                    })?;
                    if fresh.iter().any(|r| r.pid == row.pid) {
                        return Err(error).with_context(|| {
                            format!("Present {name} PID {} identity unavailable; publication held", row.pid)
                        });
                    }
                    ensure!(retries < 3, "Publisher process inventory remained unstable after three exact vanished-row retries");
                    // Do NOT merely skip the disappeared row. A newly started
                    // game/launcher/unknown Node in this fresh snapshot must be
                    // checked before any filesystem effect is authorized.
                    restart = Some(fresh);
                    break;
                }
            }
        }
        match restart {
            Some(fresh) => rows = fresh,
            None => return Ok(()),
        }
    }
    bail!("Unreachable bounded publisher process guard")
}

#[cfg(not(publisher_guard_offline))]
mod native {
    use super::*;
    use windows::Win32::{
        Foundation::{CloseHandle, ERROR_NO_MORE_FILES, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
        System::{Diagnostics::ToolHelp::*, Threading::*},
    };
    struct H(HANDLE);
    impl Drop for H {
        fn drop(&mut self) {
            unsafe { let _ = CloseHandle(self.0); }
        }
    }
    pub(super) struct Native;
    impl Access for Native {
        fn snapshot(&mut self) -> Result<Vec<Row>> {
            let h = H(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)? });
            let mut row = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
            unsafe { Process32FirstW(h.0, &mut row)?; }
            let mut out = vec![];
            loop {
                ensure!(out.len() < 65536, "Publisher process snapshot exceeds bound");
                let length = row.szExeFile.iter().position(|c| *c == 0).unwrap_or(row.szExeFile.len());
                out.push(Row { pid: row.th32ProcessID, name: String::from_utf16(&row.szExeFile[..length])? });
                match unsafe { Process32NextW(h.0, &mut row) } {
                    Ok(()) => {},
                    Err(e) if e.code() == ERROR_NO_MORE_FILES.to_hresult() => break,
                    Err(e) => return Err(e.into()),
                }
            }
            Ok(out)
        }
        fn runtime(&mut self, pid: u32) -> Result<Runtime> {
            let process = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE, false, pid) } {
                Ok(h) => H(h),
                Err(e) => return Ok(Runtime::Unknown { exited: None, error: e.into() }),
            };
            let mut path = vec![0u16; 32768];
            let mut count = path.len() as u32;
            match unsafe { QueryFullProcessImageNameW(process.0, PROCESS_NAME_WIN32, windows::core::PWSTR(path.as_mut_ptr()), &mut count) } {
                Ok(()) => {
                    ensure!(count > 0 && count as usize <= path.len(), "Invalid runtime image-name length");
                    Ok(Runtime::Path(String::from_utf16(&path[..count as usize])?))
                },
                Err(error) => {
                    // QueryFullProcessImageName on a retained but naturally
                    // exited process returns ERROR_GEN_FAILURE (31). Exit is
                    // proved by the retained wait object, not exitTime==0 or
                    // a magic process exit code. Unknown wait states block.
                    let wait = unsafe { WaitForSingleObject(process.0, 30) };
                    ensure!(wait == WAIT_OBJECT_0 || wait == WAIT_TIMEOUT, "Cannot establish retained runtime liveness (wait {:x})", wait.0);
                    Ok(Runtime::Unknown { exited: Some(wait == WAIT_OBJECT_0), error: error.into() })
                }
            }
        }
    }
}
#[cfg(not(publisher_guard_offline))]
pub(crate) fn closed() -> Result<()> { check(&mut native::Native, std::process::id()) }

#[cfg(publisher_guard_offline)]
pub(crate) fn run_tests() -> Result<()> {
    use std::collections::VecDeque;
    struct Fake { snapshots: VecDeque<Vec<Row>>, probes: VecDeque<(u32, Runtime)>, snapshots_taken: usize }
    impl Access for Fake {
        fn snapshot(&mut self) -> Result<Vec<Row>> { self.snapshots_taken += 1; self.snapshots.pop_front().context("Unexpected extra snapshot") }
        fn runtime(&mut self, pid:u32) -> Result<Runtime> { let (expected,p)=self.probes.pop_front().context("Unexpected process query")?;ensure!(pid==expected,"Wrong retained runtime PID");Ok(p) }
    }
    fn row(pid:u32,name:&str)->Row{Row{pid,name:name.into()}}
    fn unknown(exited:Option<bool>)->Runtime{Runtime::Unknown{exited,error:anyhow::anyhow!("synthetic Win32 31")}}
    fn fake(snapshots:Vec<Vec<Row>>, probes:Vec<(u32,Runtime)>)->Fake{Fake{snapshots:snapshots.into(),probes:probes.into(),snapshots_taken:0}}
    for name in ["NBA2K27.exe","EasyAntiCheat_EOS.exe","2k Launcher WPF.exe","2k Launcher Rust.exe","2k Launcher Rust Prototype.exe","2k-Launcher-0.36.45.exe"] {
        ensure!(check(&mut fake(vec![vec![row(1,name)]],vec![]),999).is_err(),"Known unsafe process allowed");
    }
    for name in ["node.exe","electron.exe"] {
        check(&mut fake(vec![vec![row(1,name)]],vec![(1,Runtime::Path(format!(r"C:\unrelated\{name}")))]),999)?;
        ensure!(check(&mut fake(vec![vec![row(1,name)]],vec![(1,Runtime::Path(r"C:\old\App\backend\node.exe".into()))]),999).is_err(),"Old runtime allowed");
        ensure!(check(&mut fake(vec![vec![row(1,name)]],vec![(1,Runtime::Path(r"C:\game\NBA2K27.exe".into()))]),999).is_err(),"Reused PID/name mismatch allowed");
    }
    println!("PASS known games/launchers, old runtime paths, unrelated runtime allowance and PID-reuse basename mismatch");
    for state in [Some(true),None] {
        let mut f=fake(vec![vec![row(1,"node.exe")],vec![]],vec![(1,unknown(state))]);
        check(&mut f,999)?;ensure!(f.snapshots_taken==2,"Missing exact fresh snapshot");
    }
    println!("PASS retained exited-query failure and failed-open require exact fresh PID absence");
    for state in [Some(true),None,Some(false)] {
        let mut f=fake(vec![vec![row(1,"node.exe")],vec![row(1,"node.exe")]],vec![(1,unknown(state))]);
        let text=format!("{:#}",check(&mut f,999).unwrap_err());
        ensure!(text.contains("PID 1") && text.contains("Win32 31"),"Lost diagnostic context");
    }
    let mut f=fake(vec![vec![row(1,"node.exe")],vec![]],vec![(1,unknown(Some(false)))]);
    ensure!(check(&mut f,999).is_err() && f.snapshots_taken==1,"Live process metadata failure was ignored or retried");
    println!("PASS live/present/unknown identity errors remain blocking and carry PID/name/error context");
    for fresh in [vec![row(2,"NBA2K27.exe")],vec![row(2,"2k Launcher Rust.exe")],vec![row(2,"node.exe")]] {
        let mut f=fake(vec![vec![row(1,"node.exe")],fresh.clone(),fresh],vec![(1,unknown(Some(true))),(2,unknown(None))]);
        ensure!(check(&mut f,999).is_err(),"Fresh dangerous row not revalidated");
    }
    println!("PASS full fresh snapshot revalidation detects newly appeared game, launcher or unknown runtime");
    let snapshots=(1..=5).map(|p|vec![row(p,"node.exe")]).collect();
    let probes=(1..=4).map(|p|(p,unknown(Some(true)))).collect();
    let mut f=fake(snapshots,probes);
    ensure!(format!("{:#}",check(&mut f,999).unwrap_err()).contains("three exact vanished-row retries"),"Unbounded disappearing-process retries");
    ensure!(f.snapshots_taken==5,"Retry bound changed");
    println!("PASS repeated proven disappearance races stop at a fixed bound");
    let mut f=fake(vec![vec![row(999,"2k Launcher Rust.exe")]],vec![]);check(&mut f,999)?;
    let mut f=fake(vec![vec![row(1,"node.exe")]],vec![(1,unknown(Some(true)))]);
    ensure!(check(&mut f,999).is_err(),"Failed fresh snapshot allowed publication");
    println!("PASS own PID exemption only and failed fresh snapshot fail-closed");
    Ok(())
}
