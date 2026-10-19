//! Child capability of presentation_v2_runtime. Actual native APIs; no public
//! address writer, process control, executable protection or engine calls.
use super::*;
use crate::blacktop_native::{self, Memory, Reader};
use std::mem::size_of;
use windows::{
    Win32::{
        Foundation::*,
        System::{Diagnostics::Debug::WriteProcessMemory, Memory::*, Threading::*},
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Region {
    base: u64,
    allocation: u64,
    len: u64,
    state: u32,
    kind: u32,
    protection: u32,
}
fn contains(r: Region, at: u64, len: usize) -> bool {
    len > 0
        && at >= r.base
        && at
            .checked_add(len as u64)
            .zip(r.base.checked_add(r.len))
            .is_some_and(|(a, b)| a <= b)
}
fn private(r: Region, at: u64, len: usize) -> Result<Region> {
    ensure!(
        at >= 0x10000
            && at
                .checked_add(len as u64)
                .is_some_and(|v| v <= 0x7fff_ffff_ffff)
            && r.state == MEM_COMMIT.0
            && r.kind == MEM_PRIVATE.0
            && r.protection == PAGE_READWRITE.0
            && r.allocation >= 0x10000
            && r.allocation % 4096 == 0
            && r.allocation <= r.base
            && contains(r, at, len),
        "Presentation owner span is not committed private non-executable RW data"
    );
    Ok(r)
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
            "Presentation launch owner exited or changed"
        );
    }
    let others: Vec<_> = rows
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
        .collect();
    blacktop_native::guard(&others, &b.game)
}
fn handle_identity(handle: HANDLE, expected: &Identity) -> Result<bool> {
    let alive = match unsafe { WaitForSingleObject(handle, 0) } {
        WAIT_TIMEOUT => true,
        WAIT_OBJECT_0 => false,
        _ => anyhow::bail!("Presentation retained game wait status is unknown"),
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
        ((c.dwHighDateTime as u64) << 32 | c.dwLowDateTime as u64).to_string() == expected.started,
        "Presentation retained game birth changed"
    );
    // GetProcessTimes exitTime is undefined while live. Only the retained
    // SYNCHRONIZE handle's wait status proves live/ended.
    if !alive {
        return Ok(false);
    }
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
        len > 0
            && (len as usize) < text.len()
            && crate::native_session::same_path(
                &String::from_utf16(&text[..len as usize])?,
                Path::new(&expected.path)
            ),
        "Presentation retained game path changed"
    );
    Ok(true)
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
            handle_identity(query.0, &binding.game)?,
            "Presentation game already ended"
        );
        let native = Self {
            reader,
            query,
            binding: binding.clone(),
        };
        policy::verify_actor_profile(&native)?;
        native.guard(true)?;
        Ok(native)
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
            "Presentation first module identity changed"
        );
        Ok(())
    }
    fn query(&self, at: u64) -> Result<Region> {
        self.checkpoint()?;
        ensure!(
            (0x10000..=0x7fff_ffff_ffff).contains(&at),
            "Invalid presentation region address"
        );
        let mut info = MEMORY_BASIC_INFORMATION::default();
        let n = unsafe {
            VirtualQueryEx(
                self.query.0,
                Some(at as _),
                &mut info,
                size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        ensure!(
            n == size_of::<MEMORY_BASIC_INFORMATION>(),
            "Presentation VirtualQueryEx failed"
        );
        Ok(Region {
            base: info.BaseAddress as u64,
            allocation: info.AllocationBase as u64,
            len: info.RegionSize as u64,
            state: info.State.0,
            kind: info.Type.0,
            protection: info.Protect.0,
        })
    }
    fn span(&self, p: &Permit) -> Result<Region> {
        p.valid()?;
        match p.change.kind {
            policy::Kind::CrowdStanding(index) => {
                let row = &p.before.sources[(index - 175) as usize];
                private(self.query(row.address)?, row.address, 0x7448)
            }
            _ => private(self.query(p.before.owner)?, p.before.owner, 0x2C58),
        }
    }
    pub(super) fn ended(&self) -> Result<()> {
        ensure!(
            !handle_identity(self.query.0, &self.binding.game)?,
            "Retained presentation game is still live"
        );
        Ok(())
    }
}
impl Memory for Native {
    fn base(&self) -> u64 {
        self.reader.base()
    }
    fn checkpoint(&self) -> Result<()> {
        ensure!(
            handle_identity(self.query.0, &self.binding.game)?,
            "Presentation retained game ended"
        );
        Ok(())
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        ensure!(
            len > 0
                && len <= 4096
                && at >= 0x10000
                && at
                    .checked_add(len as u64)
                    .is_some_and(|v| v <= 0x7fff_ffff_ffff),
            "Invalid bounded presentation read"
        );
        let r = self.query(at)?;
        ensure!(
            r.state == MEM_COMMIT.0
                && contains(r, at, len)
                && ((r.kind == MEM_IMAGE.0
                    && r.allocation == self.base()
                    && matches!(r.protection, 2 | 4 | 8 | 32))
                    || (r.kind == MEM_PRIVATE.0 && r.protection == PAGE_READWRITE.0)),
            "Presentation read escaped readable image/private data"
        );
        let bytes = self.reader.read(at, len)?;
        ensure!(bytes.len() == len, "Partial presentation read");
        self.checkpoint()?;
        Ok(bytes)
    }
}
impl Access for Native {
    fn boundary(&self) -> Result<Boundary> {
        self.guard(false)?;
        rules_v2::verify_profile(self)?;
        let context = policy::settled_context(self)?;
        ensure!(
            policy::settled_context(self)? == context,
            "Presentation boundary changed during reading"
        );
        Ok(Boundary {
            base: self.base(),
            context,
        })
    }
    fn capture(&self, crowd: bool) -> Result<policy::Snapshot> {
        self.guard(false)?;
        let s = if crowd {
            policy::capture(self)?
        } else {
            policy::capture_actors(self)?
        };
        private(self.query(s.owner)?, s.owner, 0x2C58)?;
        for source in &s.sources {
            private(self.query(source.address)?, source.address, 0x7448)?;
        }
        self.guard(false)?;
        Ok(s)
    }
    fn publish(
        &self,
        p: &Permit,
        parent_required: bool,
        durable: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        p.valid()?;
        self.guard(parent_required)?;
        ensure!(
            self.capture(!p.before.sources.is_empty())? == p.before,
            "Presentation changed before write intent use"
        );
        let region = self.span(p)?;
        durable()?;
        // The one native writer is acquired only here, after exact durable
        // intent checks. It never outlives this one-byte publication.
        let writer = Handle(unsafe { OpenProcess(WRITE, false, self.binding.game.pid)? });
        ensure!(
            handle_identity(writer.0, &self.binding.game)?,
            "Presentation writer game ended"
        );
        self.guard(parent_required)?;
        if p.before.sources.is_empty() {
            policy::verify_actor_profile(self)?;
        } else {
            policy::verify_profile(self)?;
        }
        ensure!(
            self.span(p)? == region && self.capture(!p.before.sources.is_empty())? == p.before,
            "Presentation owner/allocation/context/neighbors changed before effect"
        );
        durable()?;
        ensure!(
            handle_identity(writer.0, &self.binding.game)?,
            "Presentation writer identity ended before effect"
        );
        let byte = [p.change.after];
        let mut done = 0;
        unsafe {
            WriteProcessMemory(
                writer.0,
                p.change.at as _,
                byte.as_ptr() as _,
                1,
                Some(&mut done),
            )?;
        }
        ensure!(
            done == 1
                && self.span(p)? == region
                && self.capture(!p.after.sources.is_empty())? == p.after,
            "Presentation readback or unchanged-neighbor verification failed"
        );
        self.guard(parent_required)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_presentation_span_rejects_executable_foreign_holes_and_short_owners() {
        let r = Region {
            base: 0x30000000000,
            allocation: 0x30000000000,
            len: 0x10000,
            state: MEM_COMMIT.0,
            kind: MEM_PRIVATE.0,
            protection: PAGE_READWRITE.0,
        };
        assert!(private(r, r.base, 0x7448).is_ok());
        for wrong in [
            Region { len: 0x1000, ..r },
            Region {
                state: MEM_RESERVE.0,
                ..r
            },
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
                protection: PAGE_READWRITE.0 | PAGE_GUARD.0,
                ..r
            },
            Region { allocation: 0, ..r },
        ] {
            assert!(private(wrong, r.base, 0x7448).is_err());
        }
    }
    #[test]
    fn presentation_guard_only_exempts_exact_parent_and_allows_owned_restore_after_parent_exit() {
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
            PathBuf::from(r"C:\Fixture"),
            PathBuf::from(r"C:\Portable"),
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
        let foreign = session_safety::Process {
            name: "2k Launcher.exe".into(),
            pid: 99,
            parent: 0,
            identity: None,
        };
        assert!(guard_rows(&[game(), foreign], &b, false, None).is_err());
        let protected = session_safety::Process {
            name: "EasyAntiCheat_EOS.exe".into(),
            pid: 99,
            parent: 0,
            identity: None,
        };
        assert!(guard_rows(&[game(), protected], &b, true, Some(&b.parent)).is_err());
        let mut changed = game();
        changed.identity.as_mut().unwrap().started = "101".into();
        assert!(guard_rows(&[changed], &b, false, None).is_err());
    }
}
