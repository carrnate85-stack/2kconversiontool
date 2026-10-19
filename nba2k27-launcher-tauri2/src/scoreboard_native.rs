//! Actual native child capability of scoreboard_runtime. No arbitrary writer,
//! remote code/allocation, protection changes, hooks or game engine calls.
use super::*;
use crate::{
    blacktop_arena::{Inspect, Region},
    blacktop_native::{self, Memory, Reader},
    scoreboard_session::{Executor, Permit},
};
use std::mem::size_of;
use windows::{
    Win32::{
        Foundation::*,
        System::{
            Diagnostics::Debug::{ReadProcessMemory, WriteProcessMemory},
            Memory::*,
            Threading::*,
        },
    },
    core::PWSTR,
};
const READ: PROCESS_ACCESS_RIGHTS =
    PROCESS_ACCESS_RIGHTS(PROCESS_QUERY_INFORMATION.0 | PROCESS_VM_READ.0 | PROCESS_SYNCHRONIZE.0);
const WRITE: PROCESS_ACCESS_RIGHTS =
    PROCESS_ACCESS_RIGHTS(READ.0 | PROCESS_VM_WRITE.0 | PROCESS_VM_OPERATION.0);
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
fn retained(handle: HANDLE, id: &Identity) -> Result<bool> {
    let alive = match unsafe { WaitForSingleObject(handle, 0) } {
        WAIT_TIMEOUT => true,
        WAIT_OBJECT_0 => false,
        _ => anyhow::bail!("Scoreboard retained process wait is unknown"),
    };
    let (mut c, mut e, mut k, mut u) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    unsafe {
        GetProcessTimes(handle, &mut c, &mut e, &mut k, &mut u)?;
    }
    ensure!(
        (((c.dwHighDateTime as u64) << 32) | c.dwLowDateTime as u64).to_string() == id.started,
        "Scoreboard retained game birth differs"
    );
    if !alive {
        return Ok(false);
    }
    let mut text = vec![0u16; 32768];
    let mut n = text.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(text.as_mut_ptr()), &mut n)?;
    }
    ensure!(
        n > 0
            && (n as usize) < text.len()
            && crate::native_session::same_path(
                &String::from_utf16(&text[..n as usize])?,
                Path::new(&id.path)
            ),
        "Scoreboard retained game executable differs"
    );
    Ok(true)
}
fn guard_rows(
    rows: &[session_safety::Process],
    b: &Binding,
    parent_required: bool,
    parent: Option<&Identity>,
) -> Result<()> {
    if parent_required {
        ensure!(
            parent == Some(&b.parent),
            "Scoreboard launch owner exited or changed"
        );
    }
    let others = rows
        .iter()
        .filter(|p| {
            !(session_safety::launcher_process(p) && p.identity.as_ref() == Some(&b.parent))
        })
        .map(|p| session_safety::Process {
            name: p.name.clone(),
            pid: p.pid,
            parent: p.parent,
            identity: p.identity.clone(),
        })
        .collect::<Vec<_>>();
    blacktop_native::guard(&others, &b.game)
}
pub(super) struct Native {
    reader: Reader,
    query: Handle,
    binding: Binding,
}
impl Native {
    pub(super) fn open(binding: &Binding) -> Result<Self> {
        guard_rows(
            &session_safety::processes()?,
            binding,
            true,
            session_safety::identity(binding.parent.pid).ok().as_ref(),
        )?;
        let reader = Reader::open_observer(&binding.game, &binding.game_root)?;
        let query = Handle(unsafe { OpenProcess(READ, false, binding.game.pid)? });
        ensure!(
            retained(query.0, &binding.game)?,
            "Scoreboard game ended before native initialization"
        );
        let n = Self {
            reader,
            query,
            binding: binding.clone(),
        };
        n.guard(true)?;
        crate::scoreboard_v2::verify_profile(&n)?;
        Ok(n)
    }
    fn guard(&self, parent_required: bool) -> Result<()> {
        self.checkpoint()?;
        guard_rows(
            &session_safety::processes()?,
            &self.binding,
            parent_required,
            session_safety::identity(self.binding.parent.pid)
                .ok()
                .as_ref(),
        )?;
        let fresh = Reader::open_observer(&self.binding.game, &self.binding.game_root)?;
        ensure!(
            fresh.base() == self.base(),
            "Scoreboard first module identity changed"
        );
        Ok(())
    }
    pub(super) fn ended(&self) -> Result<()> {
        ensure!(
            !retained(self.query.0, &self.binding.game)?,
            "Exact retained scoreboard game is still alive"
        );
        Ok(())
    }
    fn writable(&self, p: &Permit<'_>) -> Result<Region> {
        let c = p.change()?;
        let r = self.region(c.address)?;
        ensure!(
            c.before.len() == 96
                && c.after.len() == 12
                && c.before[..12] != c.after
                && writable_span(r, c.address, p.snapshot().allocation(c.address)?),
            "Scoreboard publication escaped exact private RW scalar ownership"
        );
        Ok(r)
    }
}
fn writable_span(r: Region, address: u64, allocation: u64) -> bool {
    address >= 0x10000
        && address % 4 == 0
        && r.committed
        && r.kind == MEM_PRIVATE.0
        && r.protection == PAGE_READWRITE.0
        && r.allocation == allocation
        && allocation >= 0x10000
        && allocation % 4096 == 0
        && allocation <= r.start
        && address >= r.start
        && address
            .checked_add(96)
            .zip(r.start.checked_add(r.len))
            .is_some_and(|(a, b)| a <= b)
}
impl Memory for Native {
    fn base(&self) -> u64 {
        self.reader.base()
    }
    fn checkpoint(&self) -> Result<()> {
        ensure!(
            retained(self.query.0, &self.binding.game)?,
            "Scoreboard retained game ended"
        );
        Ok(())
    }
    fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
        ensure!(
            a >= 0x10000
                && (1..=2 * 1024 * 1024).contains(&n)
                && a.checked_add(n as u64).is_some_and(|e| e < 0x7fff00000000),
            "Invalid bounded scoreboard native read"
        );
        self.checkpoint()?;
        let r = self.region(a)?;
        ensure!(
            r.committed
                && a >= r.start
                && a.checked_add(n as u64)
                    .zip(r.start.checked_add(r.len))
                    .is_some_and(|(x, y)| x <= y)
                && ((r.kind == MEM_IMAGE.0
                    && r.allocation == self.base()
                    && matches!(r.protection, 2 | 4 | 8 | 32))
                    || (r.kind == MEM_PRIVATE.0 && matches!(r.protection, 2 | 4))),
            "Scoreboard read escaped bounded image/private data"
        );
        let mut bytes = vec![0; n];
        let mut done = 0;
        unsafe {
            ReadProcessMemory(
                self.query.0,
                a as _,
                bytes.as_mut_ptr() as _,
                n,
                Some(&mut done),
            )?;
        }
        ensure!(done == n, "Partial native scoreboard read");
        self.checkpoint()?;
        Ok(bytes)
    }
}
impl Inspect for Native {
    fn region(&self, a: u64) -> Result<Region> {
        self.checkpoint()?;
        let mut info = MEMORY_BASIC_INFORMATION::default();
        let n = unsafe {
            VirtualQueryEx(
                self.query.0,
                Some(a as _),
                &mut info,
                size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        ensure!(
            n == size_of::<MEMORY_BASIC_INFORMATION>(),
            "Scoreboard VirtualQueryEx failed"
        );
        Ok(Region {
            start: info.BaseAddress as u64,
            len: info.RegionSize as u64,
            allocation: info.AllocationBase as u64,
            kind: info.Type.0,
            protection: info.Protect.0,
            committed: info.State == MEM_COMMIT,
        })
    }
}
impl Executor for Native {
    fn write_header(&mut self, p: &Permit<'_>, durable: &dyn Fn() -> Result<()>) -> Result<()> {
        let parent_required = !p.undo();
        self.guard(parent_required)?;
        p.verify_before(self)?;
        let original = self.writable(p)?;
        durable()?;
        // Short-lived write handle is acquired only after complete snapshot+intent.
        let writer = Handle(unsafe { OpenProcess(WRITE, false, self.binding.game.pid)? });
        ensure!(
            retained(writer.0, &self.binding.game)?,
            "Scoreboard write-handle identity ended"
        );
        self.guard(parent_required)?;
        crate::scoreboard_v2::verify_profile(self)?;
        p.verify_before(self)?;
        ensure!(
            same_region(self.writable(p)?, original),
            "Scoreboard allocation/protection changed after write-handle acquisition"
        );
        durable()?;
        let c = p.change()?;
        ensure!(
            retained(writer.0, &self.binding.game)? && self.read(c.address, 96)? == c.before,
            "Scoreboard owner changed at publication boundary"
        );
        p.verify_before(self)?;
        durable()?;
        let mut done = 0;
        unsafe {
            WriteProcessMemory(
                writer.0,
                c.address as _,
                c.after.as_ptr() as _,
                12,
                Some(&mut done),
            )?;
        }
        ensure!(
            done == 12 && same_region(self.writable(p)?, original),
            "Scoreboard 12-byte publication/allocation is uncertain"
        );
        p.verify_after(self)?;
        self.guard(parent_required)
    }
}
fn same_region(a: Region, b: Region) -> bool {
    a.start == b.start
        && a.len == b.len
        && a.allocation == b.allocation
        && a.kind == b.kind
        && a.protection == b.protection
        && a.committed == b.committed
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scalar_span_rejects_foreign_executable_short_or_unaligned_ownership() {
        let r = Region {
            start: 0x30000000000,
            len: 0x10000,
            allocation: 0x30000000000,
            kind: MEM_PRIVATE.0,
            protection: PAGE_READWRITE.0,
            committed: true,
        };
        assert!(writable_span(r, r.start, r.allocation));
        assert!(!writable_span(r, r.start + 1, r.allocation));
        assert!(!writable_span(r, r.start, r.allocation + 0x1000));
        for bad in [
            Region { len: 95, ..r },
            Region {
                kind: MEM_IMAGE.0,
                ..r
            },
            Region {
                protection: PAGE_EXECUTE_READWRITE.0,
                ..r
            },
            Region {
                protection: PAGE_READONLY.0,
                ..r
            },
            Region {
                committed: false,
                ..r
            },
            Region {
                protection: PAGE_READWRITE.0 | PAGE_GUARD.0,
                ..r
            },
        ] {
            assert!(!writable_span(bad, r.start, r.allocation));
        }
    }
    #[test]
    fn process_guard_exempts_only_exact_retained_launcher_parent() {
        let b = Binding::from_launched(
            Identity {
                pid: 11,
                started: "100".into(),
                path: r"C:\Fixture\NBA2K27.exe".into(),
            },
            Identity {
                pid: 22,
                started: "200".into(),
                path: r"C:\Portable\2k Launcher.exe".into(),
            },
            r"C:\Fixture".into(),
            r"C:\Portable".into(),
            "a".repeat(48),
        )
        .unwrap();
        let game = || session_safety::Process {
            name: "NBA2K27.exe".into(),
            pid: 11,
            parent: 22,
            identity: Some(b.game.clone()),
        };
        assert!(guard_rows(&[game()], &b, true, Some(&b.parent)).is_ok());
        assert!(guard_rows(&[game()], &b, true, None).is_err());
        assert!(guard_rows(&[game()], &b, false, None).is_ok());
        for name in ["node.exe", "EasyAntiCheat_EOS.exe"] {
            let foreign = session_safety::Process {
                name: name.into(),
                pid: 99,
                parent: 0,
                identity: None,
            };
            assert!(guard_rows(&[game(), foreign], &b, false, None).is_err());
        }
    }
}
