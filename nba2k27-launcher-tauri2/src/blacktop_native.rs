//! Native, opt-in Blacktop observer and separate short-lived rules access.
//! Updated build's offsets/signatures are copied from the verified WPF detector.
use crate::{
    native_session::same_path,
    session_safety::{self, Identity, Process},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{mem::size_of, path::Path};
use windows::{
    Win32::{
        Foundation::*,
        System::{
            Diagnostics::{
                Debug::{ReadProcessMemory, WriteProcessMemory},
                ToolHelp::*,
            },
            Memory::*,
            Threading::*,
        },
    },
    core::PWSTR,
};

const ROOT: u64 = 0xAEB69D0;
const LINEUP: u64 = 0x9BBF0A0;
const MODES: u64 = 0xAC77660;
const CURRENT: u64 = 0xAC77E28;
const ALTERNATE: u64 = 0xAC77E24;
const OWNER: u64 = 0xAACB9A0;
const ACTIVE: u64 = 0xAACC310;
const REFRESH: &[u8] = &[
    0x83, 0xfa, 0x1e, 0x77, 0x0e, 0x89, 0xd0, 0x48, 0x69, 0xc0, 0xb0, 7, 0, 0, 0xc6, 0x44, 1, 0x30,
    1, 0xc3,
];
#[path = "arena_ball_v2_access.rs"]
pub(crate) mod arena_ball_v2_access;
#[path = "feature_writer.rs"]
pub(crate) mod feature_writer;
#[path = "blacktop_observer_profile.rs"]
pub(crate) mod observer_profile;
#[path = "outfit_selection_v2_writer.rs"]
pub(crate) mod outfit_selection_v2_writer;
#[path = "blacktop_outfit_writer.rs"]
pub(crate) mod outfit_writer;

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
// A retained handle, not repeated PID-based memory access. PID reuse cannot
// redirect reads to a replacement process. Query + VM_READ only.
pub struct Reader {
    handle: Handle,
    pub identity: Identity,
    base: u64,
}
/// Short-lived player-data capability. Addresses come only from the verified
/// native snapshot, and the only write entry point accepts a bounded relative
/// offset in one of two fixed spans. No code pages, protection changes or hooks.
pub(crate) struct PlayerWriter {
    reader: Reader,
    record: u64,
    appearance: u64,
}
impl PlayerWriter {
    pub(crate) fn open(id: &Identity, game: &Path, record: u64, appearance: u64) -> Result<Self> {
        let reader = Reader::open_access(
            id,
            game,
            PROCESS_QUERY_INFORMATION | PROCESS_VM_READ | PROCESS_VM_WRITE | PROCESS_VM_OPERATION,
        )?;
        let result = Self {
            reader,
            record,
            appearance,
        };
        result.writable(record, crate::roster_snapshot::PLAYER_STRIDE)?;
        result.writable(appearance, 4)?;
        Ok(result)
    }
    pub(crate) fn reader(&self) -> &Reader {
        &self.reader
    }
    fn writable(&self, at: u64, len: usize) -> Result<()> {
        address(at, len)?;
        self.reader.check()?;
        let mut info = MEMORY_BASIC_INFORMATION::default();
        let done = unsafe {
            VirtualQueryEx(
                self.reader.handle.0,
                Some(at as _),
                &mut info,
                size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        ensure!(
            done == size_of::<MEMORY_BASIC_INFORMATION>()
                && info.State == MEM_COMMIT
                && info.Type == MEM_PRIVATE
                && info.Protect == PAGE_READWRITE
                && (info.BaseAddress as u64)
                    .checked_add(info.RegionSize as u64)
                    .is_some_and(|end| at.checked_add(len as u64).is_some_and(|p| p <= end)),
            "Player destination is not a private, non-executable read/write data span"
        );
        Ok(())
    }
    pub(crate) fn write(
        &self,
        area: crate::player_changes::Area,
        offset: usize,
        bytes: &[u8],
    ) -> Result<()> {
        use crate::player_changes::Area;
        let (base, limit) = match area {
            Area::Record => (self.record, crate::roster_snapshot::PLAYER_STRIDE),
            Area::Appearance => (self.appearance, 4),
        };
        ensure!(
            !bytes.is_empty()
                && bytes.len() <= 40
                && offset.checked_add(bytes.len()).is_some_and(|n| n <= limit),
            "Player write is outside its owned span"
        );
        let at = base
            .checked_add(offset as u64)
            .context("Player write address overflow")?;
        self.writable(at, bytes.len())?;
        let mut done = 0;
        unsafe {
            WriteProcessMemory(
                self.reader.handle.0,
                at as _,
                bytes.as_ptr() as _,
                bytes.len(),
                Some(&mut done),
            )?;
        }
        ensure!(
            done == bytes.len(),
            "Partial player write; transaction verification required"
        );
        self.reader.check()?;
        Ok(())
    }
}
unsafe impl Send for Reader {} // HANDLE is owned; access is confined to one worker.
/// Separate fixed-span team capability. The clone span, when present, is an
/// unused reserved arena selected by native code, never a browser address.
pub(crate) struct TeamWriter {
    reader: Reader,
    record: u64,
    clone_record: Option<u64>,
}
impl TeamWriter {
    pub(crate) fn open(
        id: &Identity,
        game: &Path,
        record: u64,
        clone_record: Option<u64>,
    ) -> Result<Self> {
        let reader = Reader::open_access(
            id,
            game,
            PROCESS_QUERY_INFORMATION | PROCESS_VM_READ | PROCESS_VM_WRITE | PROCESS_VM_OPERATION,
        )?;
        let result = Self {
            reader,
            record,
            clone_record,
        };
        result.writable(record, crate::team_changes::TEAM_STRIDE)?;
        if let Some(at) = clone_record {
            result.writable(at, crate::team_changes::ARENA_STRIDE)?;
        }
        Ok(result)
    }
    pub(crate) fn reader(&self) -> &Reader {
        &self.reader
    }
    fn writable(&self, at: u64, len: usize) -> Result<()> {
        address(at, len.min(4096))?;
        self.reader.check()?;
        let mut info = MEMORY_BASIC_INFORMATION::default();
        let n = unsafe {
            VirtualQueryEx(
                self.reader.handle.0,
                Some(at as _),
                &mut info,
                size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        ensure!(
            n == size_of::<MEMORY_BASIC_INFORMATION>()
                && info.State == MEM_COMMIT
                && info.Type == MEM_PRIVATE
                && info.Protect == PAGE_READWRITE
                && (info.BaseAddress as u64)
                    .checked_add(info.RegionSize as u64)
                    .is_some_and(|end| at.checked_add(len as u64).is_some_and(|p| p <= end)),
            "Team destination is not a private, non-executable read/write data span"
        );
        Ok(())
    }
    pub(crate) fn write(
        &self,
        area: crate::team_changes::Area,
        offset: usize,
        bytes: &[u8],
    ) -> Result<()> {
        use crate::team_changes::{ARENA_STRIDE, Area, TEAM_STRIDE};
        let (base, limit) = match area {
            Area::Team => (self.record, TEAM_STRIDE),
            Area::Clone => (
                self.clone_record.context("No owned arena clone")?,
                ARENA_STRIDE,
            ),
        };
        ensure!(
            !bytes.is_empty()
                && bytes.len() <= 128
                && offset
                    .checked_add(bytes.len())
                    .is_some_and(|end| end <= limit),
            "Team write is outside its owned span"
        );
        let at = base
            .checked_add(offset as u64)
            .context("Team address overflow")?;
        self.writable(at, bytes.len())?;
        let mut done = 0;
        unsafe {
            WriteProcessMemory(
                self.reader.handle.0,
                at as _,
                bytes.as_ptr() as _,
                bytes.len(),
                Some(&mut done),
            )?;
        }
        ensure!(
            done == bytes.len(),
            "Partial team write; transaction verification required"
        );
        self.reader.check()?;
        Ok(())
    }
}
/// Uniform capability: exactly one native table row, only 25 mapped u32s.
pub(crate) struct UniformWriter {
    reader: Reader,
    record: u64,
}
/// Arena-only data capability: the two fixed image mapping IDs and the
/// independently verified reserved template's two filename fields. No code
/// writes, arbitrary addresses, allocations, engine calls or thread rights.
pub(crate) struct ArenaWriter(Reader, Identity);
/// Ball-only capability. Its only write is the bounded ASCII filename in one
/// verified, unloaded private normal-ball row. No protection or counter edits.
pub(crate) struct BallWriter(Reader, Identity);
impl BallWriter {
    pub(crate) fn open(id: &Identity, game: &Path, owner: &Identity) -> Result<Self> {
        arena_guard(&session_safety::processes()?, id, owner)?;
        let r = Reader::open_access(
            id,
            game,
            PROCESS_QUERY_INFORMATION | PROCESS_VM_READ | PROCESS_VM_WRITE | PROCESS_VM_OPERATION,
        )?;
        crate::blacktop_ball::validate(&r)?;
        Ok(Self(r, owner.clone()))
    }
    pub(crate) fn reader(&self) -> &Reader {
        &self.0
    }
    pub(crate) fn write(
        &self,
        lease: &crate::blacktop_ball::Lease,
        replacement: &[u8],
        restore: bool,
    ) -> Result<()> {
        use crate::blacktop_ball as ball;
        ball::plan(lease, replacement)?;
        let (expected, next) = if restore {
            (replacement, lease.original.as_slice())
        } else {
            (lease.original.as_slice(), replacement)
        };
        let mut context = None;
        for sample in 0..4 {
            self.0.check()?;
            arena_guard(&session_safety::processes()?, &self.0.identity, &self.1)?;
            if !restore {
                ensure!(
                    session_safety::identity(self.1.pid)? == self.1,
                    "Ball request owner exited before preparation"
                );
            }
            let s = ball::capture(&self.0)?;
            s.owns(lease, expected)?;
            if !(if restore {
                s.can_restore()
            } else {
                s.can_prepare()
            }) {
                return Err(crate::ball_session::Deferred(
                    "Ball is busy or outside its safe menu; no filename write attempted".into(),
                )
                .into());
            }
            if let Some(c) = &context {
                if c != &s.context {
                    return Err(crate::ball_session::Deferred(
                        "Ball menu changed during stable preparation; no write attempted".into(),
                    )
                    .into());
                }
            }
            context = Some(s.context);
            if sample < 3 {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        self.0.check()?;
        let s = ball::capture(&self.0)?;
        s.owns(lease, expected)?;
        if !(context.as_ref() == Some(&s.context)
            && if restore {
                s.can_restore()
            } else {
                s.can_prepare()
            })
        {
            return Err(crate::ball_session::Deferred(
                "Ball became busy before its filename write".into(),
            )
            .into());
        }
        ball_filename_data(&self.0, lease, replacement, restore)?;
        ball::capture(&self.0)?.owns(lease, next)?;
        self.0.check()?;
        Ok(())
    }
}
// Private fixed-span primitive. The capability above additionally requires
// the supported game, safe menu and four unloaded observations.
fn ball_filename_data(
    reader: &Reader,
    lease: &crate::blacktop_ball::Lease,
    replacement: &[u8],
    restore: bool,
) -> Result<()> {
    use crate::{blacktop_arena::Inspect, blacktop_ball as ball};
    ball::plan(lease, replacement)?;
    reader.check()?;
    let (expected, next) = if restore {
        (replacement, lease.original.as_slice())
    } else {
        (lease.original.as_slice(), replacement)
    };
    let region = reader.region(lease.address)?;
    ensure!(
        region.committed
            && region.kind == MEM_PRIVATE.0
            && region.protection == PAGE_READWRITE.0
            && region.allocation == lease.allocation
            && region.start <= lease.address
            && region
                .start
                .checked_add(region.len)
                .zip(lease.address.checked_add(ball::STRIDE as u64))
                .is_some_and(|(end, limit)| limit <= end),
        "Ball filename is not private writable data in its owned allocation"
    );
    ensure!(
        reader.read(lease.address, ball::STRIDE)? == expected,
        "Ball row changed immediately before filename publication"
    );
    let len = ball::field_len(&lease.original, replacement)?;
    let mut done = 0;
    unsafe {
        WriteProcessMemory(
            reader.handle.0,
            (lease.address + ball::FIELD as u64) as _,
            next[ball::FIELD..ball::FIELD + len].as_ptr() as _,
            len,
            Some(&mut done),
        )?;
    }
    ensure!(
        done == len && reader.read(lease.address, ball::STRIDE)? == next,
        "Ball filename write/readback uncertain; close the game"
    );
    reader.check()
}
#[allow(dead_code)]
impl ArenaWriter {
    pub(crate) fn open(id: &Identity, game: &Path, owner: &Identity) -> Result<Self> {
        arena_guard(&session_safety::processes()?, id, owner)?;
        let r = Reader::open_access(
            id,
            game,
            PROCESS_QUERY_INFORMATION | PROCESS_VM_READ | PROCESS_VM_WRITE | PROCESS_VM_OPERATION,
        )?;
        crate::blacktop_arena::validate(&r)?;
        Ok(Self(r, owner.clone()))
    }
    pub(crate) fn reader(&self) -> &Reader {
        &self.0
    }
    fn guard(&self) -> Result<()> {
        self.0.check()?;
        arena_guard(&session_safety::processes()?, &self.0.identity, &self.1)?;
        crate::blacktop_arena::validate(&self.0)?;
        crate::blacktop_arena::selection(&self.0)?;
        Ok(())
    }
    pub(crate) fn write_mapping(&self, expected: &[u8; 20], next: &[u8; 20]) -> Result<()> {
        use crate::blacktop_arena as arena;
        ensure!(
            arena::valid_mapping(expected) && arena::valid_mapping(next),
            "Arena mapping is not a catalog-owned data plan"
        );
        self.guard()?;
        let context = arena::selection(&self.0)?;
        let at = self.0.base + arena::MAPPING;
        ensure!(
            self.0.read(at, 20)? == expected,
            "Arena mapping changed before publication"
        );
        let mut old = PAGE_PROTECTION_FLAGS::default();
        unsafe {
            VirtualProtectEx(self.0.handle.0, at as _, 8, PAGE_READWRITE, &mut old)?;
        }
        let operation = (|| -> Result<()> {
            ensure!(
                old == PAGE_READONLY,
                "Arena image page had an unexpected protection"
            );
            ensure!(
                arena::selection(&self.0)? == context && self.0.read(at, 20)? == expected,
                "Arena mapping/context changed during preparation"
            );
            let mut done = 0;
            unsafe {
                WriteProcessMemory(
                    self.0.handle.0,
                    at as _,
                    next.as_ptr() as _,
                    8,
                    Some(&mut done),
                )?;
            }
            ensure!(
                done == 8 && self.0.read(at, 20)? == next,
                "Arena mapping write/readback is uncertain; close the game"
            );
            Ok(())
        })();
        let mut ignored = PAGE_PROTECTION_FLAGS::default();
        let protection =
            unsafe { VirtualProtectEx(self.0.handle.0, at as _, 8, old, &mut ignored) };
        // Do not hide a failed protection restore behind an earlier write error.
        protection.context("Arena page protection restoration failed; close the game")?;
        arena::validate(&self.0)?;
        self.0.check()?;
        operation
    }
    pub(crate) fn write_template(
        &self,
        record: &crate::blacktop_arena::Record,
        expected: &[u8],
        next: &[u8],
    ) -> Result<()> {
        use crate::blacktop_arena::{self as arena, Inspect};
        ensure!(
            arena::valid_template_fields(record, expected)
                && arena::valid_template_fields(record, next)
                && expected.len() == 78
                && next.len() == 78,
            "Arena template field plan is not allowlisted"
        );
        self.guard()?;
        let context = arena::selection(&self.0)?;
        arena::revalidate(&self.0, record, expected)?;
        let at = record.address + 0x7e;
        let region = self.0.region(at)?;
        ensure!(
            region.protection == PAGE_READWRITE.0 && region.kind == MEM_PRIVATE.0,
            "Arena template is not non-executable writable private data"
        );
        ensure!(
            arena::selection(&self.0)? == context,
            "Selection changed before template preparation"
        );
        let mut done = 0;
        unsafe {
            WriteProcessMemory(
                self.0.handle.0,
                at as _,
                next.as_ptr() as _,
                78,
                Some(&mut done),
            )?;
        }
        ensure!(done == 78, "Partial arena template write; close the game");
        arena::revalidate(&self.0, record, next)?;
        self.0.check()?;
        Ok(())
    }
}
// Only the provider's authenticated parent is exempt. Never ignore a launcher
// merely because its name, path or PID matches; reused and unknown births hold.
fn arena_guard(rows: &[Process], game: &Identity, owner: &Identity) -> Result<()> {
    let others = rows
        .iter()
        .filter(|p| !(session_safety::launcher_process(p) && p.identity.as_ref() == Some(owner)))
        .map(|p| Process {
            name: p.name.clone(),
            pid: p.pid,
            parent: p.parent,
            identity: p.identity.clone(),
        })
        .collect::<Vec<_>>();
    guard(&others, game)
}
impl crate::blacktop_arena::Inspect for Reader {
    fn region(&self, at: u64) -> Result<crate::blacktop_arena::Region> {
        self.check()?;
        address(at, 1)?;
        let mut info = MEMORY_BASIC_INFORMATION::default();
        let done = unsafe {
            VirtualQueryEx(
                self.handle.0,
                Some(at as _),
                &mut info,
                size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        ensure!(
            done == size_of::<MEMORY_BASIC_INFORMATION>(),
            "Arena memory region cannot be verified"
        );
        Ok(crate::blacktop_arena::Region {
            start: info.BaseAddress as u64,
            len: info.RegionSize as u64,
            allocation: info.AllocationBase as u64,
            kind: info.Type.0,
            protection: info.Protect.0,
            committed: info.State == MEM_COMMIT,
        })
    }
}
impl UniformWriter {
    pub(crate) fn open(id: &Identity, game: &Path, record: u64) -> Result<Self> {
        let reader = Reader::open_access(
            id,
            game,
            PROCESS_QUERY_INFORMATION | PROCESS_VM_READ | PROCESS_VM_WRITE | PROCESS_VM_OPERATION,
        )?;
        let s = Self { reader, record };
        s.writable(record, crate::uniform_changes::STRIDE)?;
        Ok(s)
    }
    pub(crate) fn reader(&self) -> &Reader {
        &self.reader
    }
    fn writable(&self, at: u64, len: usize) -> Result<()> {
        address(at, len)?;
        self.reader.check()?;
        let mut info = MEMORY_BASIC_INFORMATION::default();
        let n = unsafe {
            VirtualQueryEx(
                self.reader.handle.0,
                Some(at as _),
                &mut info,
                size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        ensure!(
            n == size_of::<MEMORY_BASIC_INFORMATION>()
                && info.State == MEM_COMMIT
                && info.Type == MEM_PRIVATE
                && info.Protect == PAGE_READWRITE
                && (info.BaseAddress as u64)
                    .checked_add(info.RegionSize as u64)
                    .is_some_and(|end| at.checked_add(len as u64).is_some_and(|p| p <= end)),
            "Uniform destination is not private non-executable read/write data"
        );
        Ok(())
    }
    pub(crate) fn write(&self, offset: usize, bytes: &[u8]) -> Result<()> {
        let masks = crate::uniform_changes::masks();
        let mask = *masks
            .get(&offset)
            .context("Unsupported uniform write offset")?;
        ensure!(
            bytes.len() == 4
                && offset
                    .checked_add(4)
                    .is_some_and(|n| n <= crate::uniform_changes::STRIDE),
            "Uniform write is outside its mapped data field"
        );
        let at = self
            .record
            .checked_add(offset as u64)
            .context("Uniform address overflow")?;
        self.writable(at, 4)?;
        let before = u32::from_le_bytes(
            self.reader
                .read(at, 4)?
                .try_into()
                .map_err(|_| anyhow::anyhow!("Incomplete uniform word"))?,
        );
        let after = u32::from_le_bytes(bytes.try_into()?);
        ensure!(
            (before ^ after) & !mask == 0,
            "Uniform write changes unmapped neighboring bits"
        );
        let mut done = 0;
        unsafe {
            WriteProcessMemory(
                self.reader.handle.0,
                at as _,
                bytes.as_ptr() as _,
                4,
                Some(&mut done),
            )?;
        }
        ensure!(
            done == 4,
            "Partial uniform write; transaction verification required"
        );
        self.reader.check()?;
        Ok(())
    }
}
// Transport storage only: every call still queries the live kernel identity.
// Taking the buffer out of the thread-local slot also permits reentrant calls
// without a RefCell panic or sharing mutable query storage across invocations.
fn fresh_image_path(query: impl FnOnce(&mut [u16], &mut u32) -> Result<()>) -> Result<String> {
    std::thread_local! {
        static PATH_BUFFER: std::cell::Cell<Option<Box<[u16]>>> = const { std::cell::Cell::new(None) };
    }
    PATH_BUFFER.with(|slot| {
        let mut buffer = slot
            .take()
            .unwrap_or_else(|| vec![0u16; 32768].into_boxed_slice());
        let mut len = buffer.len() as u32;
        let result = query(&mut buffer, &mut len).and_then(|()| {
            ensure!(
                len > 0 && len as usize <= buffer.len(),
                "Invalid live process image path length"
            );
            Ok(String::from_utf16(&buffer[..len as usize])?)
        });
        slot.set(Some(buffer));
        result
    })
}
fn handle_identity(h: HANDLE, pid: u32) -> Result<Identity> {
    let (mut c, mut e, mut k, mut u) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    unsafe {
        GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u)?;
    }
    let path = fresh_image_path(|name, len| {
        unsafe {
            QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(name.as_mut_ptr()), len)?;
        }
        Ok(())
    })?;
    Ok(Identity {
        pid,
        started: (((c.dwHighDateTime as u64) << 32) | c.dwLowDateTime as u64).to_string(),
        path,
    })
}
fn address(at: u64, len: usize) -> Result<()> {
    ensure!(
        len > 0
            && len <= 4096
            && at >= 0x10000
            && at
                .checked_add(len as u64)
                .is_some_and(|end| end <= 0x7fffffffffff),
        "Invalid bounded read address"
    );
    Ok(())
}
fn observer_rights(rights: PROCESS_ACCESS_RIGHTS) -> bool {
    rights == (PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ)
        || rights == (PROCESS_QUERY_INFORMATION | PROCESS_VM_READ)
}
#[cfg(test)]
mod readiness_rights_tests {
    use super::*;
    #[test]
    fn observer_masks_allow_only_read_and_query() {
        assert!(observer_rights(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ));
        assert!(observer_rights(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ
        ));
        for extra in [
            PROCESS_VM_WRITE,
            PROCESS_VM_OPERATION,
            PROCESS_CREATE_THREAD,
            PROCESS_TERMINATE,
            PROCESS_SUSPEND_RESUME,
        ] {
            assert!(!observer_rights(
                PROCESS_QUERY_INFORMATION | PROCESS_VM_READ | extra
            ));
        }
    }
}
impl Reader {
    pub(crate) fn open_observer(id: &Identity, game: &Path) -> Result<Self> {
        Self::open_access_kind(
            id,
            game,
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
            true,
        )
    }
    /// Exact-owned startup readiness observer, with VirtualQueryEx rights only.
    pub(crate) fn open_readiness(id: &Identity, game: &Path) -> Result<Self> {
        Self::open_access_kind(id, game, PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, true)
    }
    // VirtualQueryEx needs QUERY_INFORMATION. Still no VM_WRITE/VM_OPERATION,
    // process-control or thread rights. The observer also remains read-only.
    pub(crate) fn open_roster(id: &Identity, game: &Path) -> Result<Self> {
        Self::open_access(id, game, PROCESS_QUERY_INFORMATION | PROCESS_VM_READ)
    }
    pub(crate) fn roster_region(&self, pointer: u64) -> Result<(u64, u64)> {
        self.check()?;
        address(pointer, 1)?;
        let mut region = MEMORY_BASIC_INFORMATION::default();
        let size = unsafe {
            VirtualQueryEx(
                self.handle.0,
                Some(pointer as _),
                &mut region,
                size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        ensure!(
            size == size_of::<MEMORY_BASIC_INFORMATION>()
                && region.State == MEM_COMMIT
                && region.Protect.0 & (PAGE_NOACCESS.0 | PAGE_GUARD.0) == 0,
            "Roster allocation is not readable committed memory"
        );
        self.check()?;
        Ok((region.BaseAddress as u64, region.RegionSize as u64))
    }
    fn open_access(id: &Identity, game: &Path, rights: PROCESS_ACCESS_RIGHTS) -> Result<Self> {
        Self::open_access_kind(id, game, rights, false)
    }
    fn open_access_kind(
        id: &Identity,
        game: &Path,
        rights: PROCESS_ACCESS_RIGHTS,
        observer: bool,
    ) -> Result<Self> {
        ensure!(
            !observer || observer_rights(rights),
            "Observer capability must remain read-only"
        );
        ensure!(
            same_path(&id.path, &game.join("NBA2K27.exe")),
            "Game path differs from launch"
        );
        let handle = Handle(unsafe { OpenProcess(rights, false, id.pid)? });
        let actual = handle_identity(handle.0, id.pid)?;
        ensure!(
            actual == *id,
            "Game identity changed before opening read-only handle"
        );
        let modules = Handle(unsafe {
            CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, id.pid)?
        });
        let mut entry = MODULEENTRY32W {
            dwSize: size_of::<MODULEENTRY32W>() as u32,
            ..Default::default()
        };
        unsafe {
            Module32FirstW(modules.0, &mut entry)?;
        }
        let end = entry
            .szExePath
            .iter()
            .position(|v| *v == 0)
            .unwrap_or(entry.szExePath.len());
        let image = String::from_utf16(&entry.szExePath[..end])?;
        ensure!(
            same_path(&image, &game.join("NBA2K27.exe")),
            "Unexpected first process module"
        );
        let base = entry.modBaseAddr as u64;
        loop {
            let end = entry
                .szModule
                .iter()
                .position(|v| *v == 0)
                .unwrap_or(entry.szModule.len());
            let name = String::from_utf16(&entry.szModule[..end])?.to_ascii_lowercase();
            ensure!(
                !name.contains("easyanticheat"),
                "Anti-cheat module present; read-only observer blocked"
            );
            match unsafe { Module32NextW(modules.0, &mut entry) } {
                Ok(()) => {}
                Err(e) if e.code() == ERROR_NO_MORE_FILES.to_hresult() => break,
                Err(e) => return Err(e.into()),
            }
        }
        let r = Self {
            handle,
            identity: actual,
            base,
        };
        r.check()?;
        if observer {
            observer_profile::validate(&r)?;
        } else {
            validate_build(&r)?;
        }
        r.check()?;
        Ok(r)
    }
    fn check(&self) -> Result<()> {
        let mut exit = 0;
        unsafe {
            GetExitCodeProcess(self.handle.0, &mut exit)?;
        }
        ensure!(
            exit == 259 && handle_identity(self.handle.0, self.identity.pid)? == self.identity,
            "Observed game ended or changed"
        );
        Ok(())
    }
    pub(crate) fn observer_snapshot(&self) -> Result<Snapshot> {
        self.check()?;
        let profile = observer_profile::validate(self)?;
        let value = snapshot_with_profile(self, &profile)?;
        self.check()?;
        Ok(value)
    }
    // Native editors keep their strict legacy gate, independent of the observer.
    pub(crate) fn snapshot(&self) -> Result<Snapshot> {
        self.check()?;
        let value = snapshot(self)?;
        self.check()?;
        Ok(value)
    }
}
pub(crate) trait Memory {
    fn base(&self) -> u64;
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>>;
    fn checkpoint(&self) -> Result<()> {
        Ok(())
    }
}
impl Memory for Reader {
    fn base(&self) -> u64 {
        self.base
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        address(at, len)?;
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
        ensure!(done == len, "Partial game read; snapshot discarded");
        Ok(bytes)
    }
}
fn u32_at(m: &impl Memory, at: u64) -> Result<u32> {
    Ok(u32::from_le_bytes(
        m.read(at, 4)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("Partial u32 read"))?,
    ))
}
fn u64_at(m: &impl Memory, at: u64) -> Result<u64> {
    Ok(u64::from_le_bytes(
        m.read(at, 8)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("Partial pointer read"))?,
    ))
}
fn rva(m: &impl Memory, off: u64) -> Result<u64> {
    m.base().checked_add(off).context("RVA overflow")
}
fn d(m: &impl Memory, off: u64) -> Result<u32> {
    u32_at(m, rva(m, off)?)
}
fn q(m: &impl Memory, off: u64) -> Result<u64> {
    u64_at(m, rva(m, off)?)
}
pub(crate) fn validate_build(m: &impl Memory) -> Result<()> {
    let b = m.read(m.base(), 4096)?;
    ensure!(
        b.len() == 4096 && &b[..2] == b"MZ",
        "Invalid executable header"
    );
    let pe = u32::from_le_bytes(b[60..64].try_into()?) as usize;
    ensure!(
        (64..=3800).contains(&pe)
            && &b[pe..pe + 4] == b"PE\0\0"
            && u16::from_le_bytes(b[pe + 4..pe + 6].try_into()?) == 0x8664,
        "Unsupported executable architecture"
    );
    ensure!(
        u32::from_le_bytes(b[pe + 8..pe + 12].try_into()?) == 0x6AA2013D
            && u32::from_le_bytes(b[pe + 80..pe + 84].try_into()?) == 0x36558000,
        "Blacktop observer supports a different game build"
    );
    ensure!(
        m.read(rva(m, 0x3ACB20)?, REFRESH.len())? == REFRESH,
        "Native refresh signature differs; observation blocked"
    );
    // RIP-relative owner anchor: verify the exact target before dereferencing.
    let code = m.read(rva(m, 0x2610815)?, 7)?;
    ensure!(
        code.len() == 7
            && code[..3] == [0x48, 0x8b, 0x05]
            && 0x2610815i64 + 7 + i32::from_le_bytes(code[3..7].try_into()?) as i64 == OWNER as i64,
        "Mode owner anchor differs; observation blocked"
    );
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ContextState {
    pub(crate) current_index: u32,
    pub(crate) alternate_index: u32,
    pub(crate) current: u32,
    pub(crate) alternate: u32,
    pub(crate) stage: u32,
    pub(crate) active: u8,
    pub(crate) owner: u64,
    pub(crate) kind: u8,
    pub(crate) game_mode: u32,
    pub(crate) root_tag: u64,
}
impl ContextState {
    pub(crate) fn mode_identity(&self) -> (u32, u32, u32, u8, u8) {
        (
            self.current,
            self.alternate,
            self.stage,
            self.active,
            self.kind,
        )
    }
}
pub(crate) fn context(m: &impl Memory) -> Result<ContextState> {
    context_with_profile(m, &observer_profile::LEGACY)
}
/// Same strict-majority debounce as rules_v2::context: Version 2 Blacktop
/// selection blips active/kind/gameMode for sub-millisecond samples. The
/// legacy build keeps its exact single read.
fn context_with_profile(m: &impl Memory, p: &observer_profile::Profile) -> Result<ContextState> {
    if p.current != observer_profile::VERSION_TWO.current {
        return context_once(m, p);
    }
    let mut samples = Vec::with_capacity(crate::rules_v2::CONTEXT_SAMPLES);
    for _ in 0..crate::rules_v2::CONTEXT_SAMPLES {
        samples.push(context_once(m, p)?);
        if samples.len() == 2 && samples[0] == samples[1] {
            return Ok(samples.swap_remove(1));
        }
    }
    let count = |c: &ContextState| samples.iter().filter(|s| *s == c).count();
    let best = samples
        .iter()
        .max_by_key(|c| count(c))
        .context("Missing mode sample")?;
    ensure!(
        count(best) * 2 > crate::rules_v2::CONTEXT_SAMPLES,
        "Mode is still changing"
    );
    Ok(best.clone())
}
fn context_once(m: &impl Memory, p: &observer_profile::Profile) -> Result<ContextState> {
    let a = d(m, p.current)?;
    let b = d(m, p.alternate)?;
    ensure!(a < 128 && b < 128, "Mode indexes are transitioning");
    let owner = q(m, p.owner)?;
    address(owner, 0xA08)?;
    ensure!(owner % 8 == 0, "Mode owner alignment changed");
    Ok(ContextState {
        current_index: a,
        alternate_index: b,
        current: d(m, p.modes + a as u64 * 64)?,
        alternate: d(m, p.modes + b as u64 * 64)?,
        stage: d(m, p.root + 0xEA8)?,
        active: m.read(rva(m, p.active)?, 1)?[0],
        owner,
        kind: m.read(owner, 1)?[0],
        game_mode: u32_at(m, owner + 0xA04)?,
        root_tag: q(m, p.root)?,
    })
}
pub(crate) fn boundary(c: &ContextState, base: u64) -> &'static str {
    boundary_with_tag(c, base, observer_profile::LEGACY.root_tag)
}
fn boundary_with_tag(c: &ContextState, base: u64, root_tag: u64) -> &'static str {
    if c.current != c.alternate {
        return "transition";
    }
    if c.current == 0x92DF06DE
        && c.stage == 3
        && c.active == 0
        && c.kind == 0
        && c.root_tag == base + root_tag
    {
        return "selection";
    }
    if c.stage == 1 && c.active == 0 && c.kind == 0 {
        if c.current == 0x6509837B {
            return "main-menu";
        }
        if c.current == 0x079964C0 {
            return "quick-play";
        }
    }
    if c.game_mode == 6
        && c.stage == 1
        && [0xB4D37B22, 0x3EEF98B9, 0x7C602DF8, 0x50BAD898].contains(&c.current)
    {
        return "match";
    }
    "transition-or-other-mode"
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Snapshot {
    pub boundary: String,
    pub required_players: u32,
    pub selected_players: u32,
    pub loaded_players: u32,
    pub lineup_ready: bool,
    pub players_ready: bool,
    pub current_hash: String,
    pub alternate_hash: String,
    pub stage: u32,
    pub writes: u32,
}
fn snapshot(m: &impl Memory) -> Result<Snapshot> {
    // Validating immutable code/header on every sample also catches other modifications.
    validate_build(m)?;
    snapshot_with_profile(m, &observer_profile::LEGACY)
}
fn snapshot_with_profile(m: &impl Memory, p: &observer_profile::Profile) -> Result<Snapshot> {
    let before = context_with_profile(m, p)?;
    let state = boundary_with_tag(&before, m.base(), p.root_tag);
    let (mut required, mut selected, mut loaded, mut ready) = (0, 0, 0, false);
    if state == "selection" {
        required = d(m, p.lineup + 4)?;
        selected = d(m, p.lineup + 0xb8)?;
        let controller = q(m, p.root + 0x1080)?;
        ensure!(
            (2..=10).contains(&required)
                && required % 2 == 0
                && selected <= required
                && d(m, p.lineup + 8)? == required,
            "Lineup counts are not stable"
        );
        address(controller, 16)?;
        ensure!(
            m.read(controller + 0x18, 1)?[0] == 1,
            "Player models are initializing"
        );
        for slot in 0..10u64 {
            let at = controller + 0x20 + slot * 0x7b0;
            let model = u64_at(m, at)?;
            let desc = u64_at(m, at + 8)?;
            if model == 0 && desc == 0 {
                continue;
            }
            if model == 0 || desc == 0 {
                continue;
            }
            if u64_at(m, model)? == m.base() + p.model
                && u64_at(m, desc)? == m.base() + p.description
            {
                loaded += 1;
            }
        }
        ensure!(
            d(m, p.lineup + 4)? == required
                && d(m, p.lineup + 8)? == required
                && d(m, p.lineup + 0xb8)? == selected
                && q(m, p.root + 0x1080)? == controller,
            "Lineup changed during observation"
        );
        ready = loaded == selected;
    }
    ensure!(
        context_with_profile(m, p)? == before,
        "Mode changed during observation; sample discarded"
    );
    Ok(Snapshot {
        boundary: state.into(),
        required_players: required,
        selected_players: selected,
        loaded_players: loaded,
        players_ready: ready,
        lineup_ready: ready && selected == required && required >= 2,
        current_hash: format!("{:08X}", before.current),
        alternate_hash: format!("{:08X}", before.alternate),
        stage: before.stage,
        writes: 0,
    })
}
// Bounded diagnostic only; the admission predicate and exact-parent exemption
// remain unchanged. Never expose command lines, paths or arbitrary identity data.
fn launcher_blocker_detail(rows: &[Process]) -> String {
    let blockers = rows
        .iter()
        .filter(|p| session_safety::launcher_process(p))
        .collect::<Vec<_>>();
    let mut descriptions = blockers
        .iter()
        .take(4)
        .map(|p| {
            let name = p
                .name
                .chars()
                .filter(|c| !c.is_control())
                .take(96)
                .collect::<String>();
            format!(
                "{} (PID {}, parent {}, identity {})",
                name,
                p.pid,
                p.parent,
                if p.identity.is_some() {
                    "read"
                } else {
                    "unavailable"
                }
            )
        })
        .collect::<Vec<_>>();
    if blockers.len() > 4 {
        descriptions.push(format!("{} more", blockers.len() - 4));
    }
    descriptions.join("; ")
}
pub fn guard(rows: &[Process], expected: &Identity) -> Result<()> {
    ensure!(
        !rows.iter().any(session_safety::launcher_process),
        "Another launcher is active; observer held: {}",
        launcher_blocker_detail(rows)
    );
    ensure!(
        !rows.iter().any(|p| {
            let name = p.name.to_ascii_lowercase();
            name.starts_with("easyanticheat")
                || name == "eaclaunch.exe"
                || name == "start_protected_game.exe"
        }),
        "Protected process present; no memory access"
    );
    let games: Vec<_> = rows
        .iter()
        .filter(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe"))
        .collect();
    ensure!(
        games.len() == 1 && games[0].identity.as_ref() == Some(expected),
        "No unique unchanged launch-session game"
    );
    Ok(())
}
pub fn idle() -> Value {
    json!({"phase":"disabled","message":"Read-only Blacktop observer is off.","readOnly":true,"writes":0,"sample":null})
}
pub fn status(phase: &str, message: &str, sample: Option<&Snapshot>) -> Value {
    json!({"phase":phase,"message":message,"readOnly":true,"writes":0,"sample":sample})
}

/// Never retained by the observer. A fresh, exact-identity handle per explicit command.
pub(crate) struct RulesAccess(Reader);
impl RulesAccess {
    pub(crate) fn open(id: &Identity, game: &Path, writable: bool) -> Result<Self> {
        guard(&session_safety::processes()?, id)?;
        // VirtualQueryEx requires QUERY_INFORMATION, not just QUERY_LIMITED_INFORMATION.
        let rights = PROCESS_QUERY_INFORMATION
            | PROCESS_QUERY_LIMITED_INFORMATION
            | PROCESS_VM_READ
            | if writable {
                PROCESS_VM_WRITE | PROCESS_VM_OPERATION
            } else {
                PROCESS_ACCESS_RIGHTS(0)
            };
        Ok(Self(Reader::open_access(id, game, rights)?))
    }
}
impl Memory for RulesAccess {
    fn base(&self) -> u64 {
        self.0.base()
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        self.0.read(at, len)
    }
}
impl crate::game_rules::Access for RulesAccess {
    fn capture(&self) -> Result<crate::game_rules::Snapshot> {
        self.0.check()?;
        guard(&session_safety::processes()?, &self.0.identity)?;
        let s = crate::game_rules::capture(self)?;
        rules_allocation(self.0.handle.0, s.context.owner)?;
        self.0.check()?;
        guard(&session_safety::processes()?, &self.0.identity)?;
        Ok(s)
    }
    fn write(
        &self,
        expected: &crate::game_rules::Snapshot,
        index: usize,
        value: u32,
    ) -> Result<()> {
        ensure!(
            self.capture()? == *expected,
            "Rules changed immediately before writing"
        );
        let field = crate::game_rules::FIELDS
            .get(index)
            .context("Unknown rule")?;
        ensure!(field.valid(value), "Unsupported rule value");
        let bytes = value.to_le_bytes();
        let mut done = 0;
        unsafe {
            WriteProcessMemory(
                self.0.handle.0,
                (expected.context.owner + field.offset) as _,
                bytes.as_ptr() as _,
                field.width,
                Some(&mut done),
            )?;
        }
        ensure!(done == field.width, "Partial rule write");
        Ok(())
    }
}
fn rules_allocation(handle: HANDLE, owner: u64) -> Result<()> {
    address(owner, 0xA9C)?;
    let mut region = MEMORY_BASIC_INFORMATION::default();
    let count = unsafe {
        VirtualQueryEx(
            handle,
            Some(owner as _),
            &mut region,
            size_of::<MEMORY_BASIC_INFORMATION>(),
        )
    };
    ensure!(
        count == size_of::<MEMORY_BASIC_INFORMATION>()
            && region.State == MEM_COMMIT
            && region.Type == MEM_PRIVATE
            && region.Protect == PAGE_READWRITE
            && region.BaseAddress as u64 <= owner
            && (region.BaseAddress as u64)
                .checked_add(region.RegionSize as u64)
                .is_some_and(|end| end >= owner + 0xA9C),
        "Rules owner is not a supported private writable allocation"
    );
    Ok(())
}

mod tests {
    use super::*;
    use std::{cell::Cell, collections::HashMap};
    #[test]
    fn ball_writer_changes_only_owned_filename_bytes_in_its_own_private_allocation() {
        struct Buffer(*mut std::ffi::c_void);
        impl Drop for Buffer {
            fn drop(&mut self) {
                unsafe {
                    VirtualFree(self.0, 0, MEM_RELEASE).unwrap();
                }
            }
        }
        let memory =
            Buffer(unsafe { VirtualAlloc(None, 4096, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE) });
        assert!(!memory.0.is_null());
        let original = crate::blacktop_ball::test_support::original();
        unsafe {
            std::ptr::copy_nonoverlapping(original.as_ptr(), memory.0 as *mut u8, original.len());
        }
        let reader = Reader {
            handle: Handle(unsafe {
                OpenProcess(
                    PROCESS_QUERY_INFORMATION
                        | PROCESS_VM_READ
                        | PROCESS_VM_WRITE
                        | PROCESS_VM_OPERATION,
                    false,
                    std::process::id(),
                )
                .unwrap()
            }),
            identity: session_safety::identity(std::process::id()).unwrap(),
            base: 0,
        };
        let lease = crate::blacktop_ball::Lease {
            holder: 65536,
            records: memory.0 as u64,
            address: memory.0 as u64,
            allocation: memory.0 as u64,
            index: 0,
            registry: 65536,
            registry_count: 2,
            original: original.clone(),
        };
        let id = crate::blacktop_ball::test_support::id();
        let next = crate::blacktop_ball::replacement(&original, id.strip_prefix("balls/").unwrap())
            .unwrap();
        ball_filename_data(&reader, &lease, &next, false).unwrap();
        assert_eq!(reader.read(lease.address, original.len()).unwrap(), next);
        let mut bad = next.clone();
        bad[8] ^= 1;
        assert!(ball_filename_data(&reader, &lease, &bad, true).is_err());
        ball_filename_data(&reader, &lease, &next, true).unwrap();
        assert_eq!(
            reader.read(lease.address, original.len()).unwrap(),
            original
        );
        let mut old = PAGE_PROTECTION_FLAGS::default();
        unsafe {
            VirtualProtect(memory.0, 4096, PAGE_READONLY, &mut old).unwrap();
        }
        assert!(ball_filename_data(&reader, &lease, &next, false).is_err());
    }
    struct Fixture {
        data: HashMap<u64, Vec<u8>>,
        torn: bool,
        torn_rule: bool,
        reads: Cell<usize>,
    }
    impl Memory for Fixture {
        fn base(&self) -> u64 {
            0x140000000
        }
        fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
            address(at, len)?;
            let n = self.reads.get();
            self.reads.set(n + 1);
            let mut value = self
                .data
                .get(&at)
                .context("Missing fixture address")?
                .clone();
            ensure!(value.len() == len, "Wrong fixture length");
            if self.torn
                && (at == self.base() + CURRENT
                    || at == self.base() + observer_profile::VERSION_TWO.current)
                && n > 20
            {
                value = 2u32.to_le_bytes().to_vec();
            }
            if self.torn_rule && at == self.base() + 0x2816 && n > 40 {
                value = vec![1];
            }
            Ok(value)
        }
    }
    impl Fixture {
        fn put(&mut self, off: u64, value: &[u8]) {
            self.data.insert(self.base() + off, value.to_vec());
        }
        fn d(&mut self, off: u64, value: u32) {
            self.put(off, &value.to_le_bytes());
        }
        fn q(&mut self, off: u64, value: u64) {
            self.put(off, &value.to_le_bytes());
        }
    }
    fn fixture(hash: u32) -> Fixture {
        let mut f = Fixture {
            data: HashMap::new(),
            torn: false,
            torn_rule: false,
            reads: Cell::new(0),
        };
        let mut pe = vec![0; 4096];
        pe[..2].copy_from_slice(b"MZ");
        pe[60..64].copy_from_slice(&128u32.to_le_bytes());
        pe[128..132].copy_from_slice(b"PE\0\0");
        pe[132..134].copy_from_slice(&0x8664u16.to_le_bytes());
        pe[136..140].copy_from_slice(&0x6AA2013Du32.to_le_bytes());
        pe[208..212].copy_from_slice(&0x36558000u32.to_le_bytes());
        f.put(0, &pe);
        f.put(0x3ACB20, REFRESH);
        let mut anchor = vec![0x48, 0x8b, 0x05];
        anchor.extend_from_slice(&((OWNER - 0x2610815 - 7) as i32).to_le_bytes());
        f.put(0x2610815, &anchor);
        f.d(CURRENT, 1);
        f.d(ALTERNATE, 1);
        f.d(MODES + 64, hash);
        f.d(ROOT + 0xEA8, if hash == 0x92DF06DE { 3 } else { 1 });
        f.put(ACTIVE, &[0]);
        let owner = f.base() + 0x2000;
        f.q(OWNER, owner);
        f.put(0x2000, &[0]);
        f.d(0x2000 + 0xA04, 6);
        f.q(ROOT, f.base() + 0x78AEA48);
        f.d(LINEUP + 4, 4);
        f.d(LINEUP + 8, 4);
        f.d(LINEUP + 0xb8, 4);
        let controller = f.base() + 0x5000;
        f.q(ROOT + 0x1080, controller);
        f.put(0x5018, &[1]);
        for slot in 0..10u64 {
            let model = if slot < 4 {
                f.base() + 0x20000 + slot * 0x1000
            } else {
                0
            };
            let desc = if slot < 4 { model + 0x800 } else { 0 };
            f.q(0x5020 + slot * 0x7b0, model);
            f.q(0x5028 + slot * 0x7b0, desc);
            if model != 0 {
                f.data
                    .insert(model, (f.base() + 0x6778FC8).to_le_bytes().to_vec());
                f.data
                    .insert(desc, (f.base() + 0x682D858).to_le_bytes().to_vec());
            }
        }
        f
    }
    pub fn smoke() -> Result<Value> {
        let s = snapshot(&fixture(0x92DF06DE))?;
        ensure!(
            s.boundary == "selection" && s.lineup_ready && s.writes == 0,
            "Selection fixture failed"
        );
        let main = snapshot(&fixture(0x6509837B))?;
        let quick = snapshot(&fixture(0x079964C0))?;
        ensure!(
            main.boundary == "main-menu" && quick.boundary == "quick-play",
            "Exit boundary fixture failed"
        );
        let mut torn = fixture(0x92DF06DE);
        torn.torn = true;
        ensure!(snapshot(&torn).is_err(), "Torn snapshot was accepted");
        Ok(
            json!({"pass":true,"readOnly":true,"writes":0,"selection":s,"quickPlay":quick,"tornRejected":true,"source":"sparse-memory fixture; live acceptance pending"}),
        )
    }
    pub(super) fn rules_fixture() -> Result<crate::game_rules::Snapshot> {
        let mut f = fixture(0x92DF06DE);
        for rule in crate::game_rules::FIELDS {
            f.put(0x2000 + rule.offset, &0u32.to_le_bytes()[..rule.width]);
        }
        crate::game_rules::capture(&f)
    }
    #[cfg(test)]
    pub(super) fn selection_memory() -> HashMap<u64, Vec<u8>> {
        fixture(0x92DF06DE).data
    }
    #[test]
    fn rule_parser_refuses_wrong_mode_torn_group_and_new_owner() {
        let mut f = fixture(0x6509837B);
        for rule in crate::game_rules::FIELDS {
            f.put(0x2000 + rule.offset, &0u32.to_le_bytes()[..rule.width]);
        }
        assert!(crate::game_rules::capture(&f).is_err());
        f.d(MODES + 64, 0x92DF06DE);
        f.d(ROOT + 0xEA8, 3);
        assert!(crate::game_rules::capture(&f).is_ok());
        f.torn = true;
        assert!(crate::game_rules::capture(&f).is_err());
        f.torn = false;
        f.torn_rule = true;
        f.reads.set(0);
        assert!(crate::game_rules::capture(&f).is_err());
        f.torn_rule = false;
        f.q(OWNER, f.base() + 0x3000);
        assert!(crate::game_rules::capture(&f).is_err());
    }
    #[test]
    fn real_rules_permissions_and_private_allocation() {
        // Only this unit-test process and its owned allocation; never NBA 2K27.
        let h = Handle(unsafe {
            OpenProcess(
                PROCESS_QUERY_INFORMATION
                    | PROCESS_QUERY_LIMITED_INFORMATION
                    | PROCESS_VM_READ
                    | PROCESS_VM_WRITE
                    | PROCESS_VM_OPERATION,
                false,
                std::process::id(),
            )
            .unwrap()
        });
        let buffer = unsafe { VirtualAlloc(None, 4096, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE) };
        assert!(!buffer.is_null());
        struct Owned(*mut std::ffi::c_void);
        impl Drop for Owned {
            fn drop(&mut self) {
                unsafe {
                    let _ = VirtualFree(self.0, 0, MEM_RELEASE);
                }
            }
        }
        let _owned = Owned(buffer);
        rules_allocation(h.0, buffer as u64).unwrap();
        for width in [1, 4] {
            let replacement = 6u32.to_le_bytes();
            let mut done = 0;
            unsafe {
                WriteProcessMemory(
                    h.0,
                    buffer,
                    replacement.as_ptr() as _,
                    width,
                    Some(&mut done),
                )
                .unwrap();
            }
            assert_eq!(done, width);
            let mut output = [0; 4];
            unsafe {
                ReadProcessMemory(
                    h.0,
                    buffer,
                    output.as_mut_ptr() as _,
                    width,
                    Some(&mut done),
                )
                .unwrap();
            }
            assert_eq!(output[..width], replacement[..width]);
        }
        let mut old = PAGE_PROTECTION_FLAGS(0);
        unsafe {
            VirtualProtect(buffer, 4096, PAGE_READONLY, &mut old).unwrap();
        }
        assert!(rules_allocation(h.0, buffer as u64).is_err());
    }
    #[test]
    fn exact_selection_and_complete_lineup() {
        let s = snapshot(&fixture(0x92DF06DE)).unwrap();
        assert_eq!(s.boundary, "selection");
        assert!(s.lineup_ready);
        assert_eq!(s.writes, 0);
    }
    #[test]
    fn incomplete_lineup_not_ready() {
        let mut f = fixture(0x92DF06DE);
        f.d(LINEUP + 0xb8, 3);
        let s = snapshot(&f).unwrap();
        assert!(!s.players_ready && !s.lineup_ready);
    }
    #[test]
    fn menu_and_quickplay_boundaries() {
        assert_eq!(
            snapshot(&fixture(0x6509837B)).unwrap().boundary,
            "main-menu"
        );
        assert_eq!(
            snapshot(&fixture(0x079964C0)).unwrap().boundary,
            "quick-play"
        );
    }
    #[test]
    fn shared_match_hash_not_freestyle() {
        let mut f = fixture(0xB4D37B22);
        assert_eq!(snapshot(&f).unwrap().boundary, "match");
        f.d(0x2A04, 5);
        assert_eq!(snapshot(&f).unwrap().boundary, "transition-or-other-mode");
    }
    #[test]
    fn torn_mode_and_lineup_rejected() {
        let mut f = fixture(0x92DF06DE);
        f.torn = true;
        assert!(snapshot(&f).is_err());
        f.torn = false;
        f.d(LINEUP + 8, 6);
        assert!(snapshot(&f).is_err());
    }
    #[test]
    fn october_version_two_is_rejected_before_any_old_rva_is_read() {
        // Actual installed October 16 PE identity. Matching old code bytes in
        // the fixture cannot grant access to a newer, unmapped game layout.
        for (stamp, size) in [
            (0x6AC53765u32, 0x35BDE000u32),
            (0x6AC53765, 0x36558000),
            (0x6AA2013D, 0x35BDE000),
        ] {
            let mut f = fixture(0x92DF06DE);
            let base = f.base();
            let header = f.data.get_mut(&base).unwrap();
            header[136..140].copy_from_slice(&stamp.to_le_bytes());
            header[208..212].copy_from_slice(&size.to_le_bytes());
            assert!(validate_build(&f).is_err());
            assert_eq!(f.reads.get(), 1, "New build must stop at its PE header");
        }
    }
    #[test]
    fn wrong_build_signatures_and_partial_reads_blocked() {
        let mut f = fixture(0x92DF06DE);
        f.put(0x3ACB20, &[0; 20]);
        assert!(snapshot(&f).is_err());
        f.put(0, &[0; 4]);
        assert!(snapshot(&f).is_err());
        assert!(address(u64::MAX, 8).is_err());
        assert!(address(0, 8).is_err());
        assert!(address(0x10000, 4097).is_err());
    }
    #[test]
    fn mismatched_modes_or_root_never_selection() {
        let mut f = fixture(0x92DF06DE);
        f.q(ROOT, 0);
        assert_ne!(snapshot(&f).unwrap().boundary, "selection");
        f.d(ALTERNATE, 2);
        f.d(MODES + 128, 0x6509837B);
        assert_eq!(snapshot(&f).unwrap().boundary, "transition");
    }
    #[test]
    fn player_writer_only_changes_owned_private_data_spans_in_its_own_process() {
        use crate::player_changes::Area;
        struct Allocation(*mut std::ffi::c_void);
        impl Drop for Allocation {
            fn drop(&mut self) {
                unsafe {
                    VirtualFree(self.0, 0, MEM_RELEASE).unwrap();
                }
            }
        }
        let buffer = Allocation(unsafe {
            VirtualAlloc(None, 4096, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE)
        });
        assert!(!buffer.0.is_null());
        let address = buffer.0 as u64;
        let reader = Reader {
            handle: Handle(unsafe {
                OpenProcess(
                    PROCESS_QUERY_INFORMATION
                        | PROCESS_VM_READ
                        | PROCESS_VM_WRITE
                        | PROCESS_VM_OPERATION,
                    false,
                    std::process::id(),
                )
                .unwrap()
            }),
            identity: session_safety::identity(std::process::id()).unwrap(),
            base: 0,
        };
        let writer = PlayerWriter {
            reader,
            record: address,
            appearance: address + 2048,
        };
        writer.write(Area::Record, 0x110, &[9, 8]).unwrap();
        writer.write(Area::Appearance, 0, &[1, 2, 3, 4]).unwrap();
        assert_eq!(writer.reader().read(address + 0x110, 2).unwrap(), [9, 8]);
        assert_eq!(
            writer.reader().read(address + 2048, 4).unwrap(),
            [1, 2, 3, 4]
        );
        for (area, offset, bytes) in [
            (Area::Record, 0, vec![]),
            (Area::Record, 0, vec![1; 41]),
            (Area::Record, 0x4F8, vec![1]),
            (Area::Appearance, 3, vec![1, 2]),
            (Area::Record, usize::MAX, vec![1]),
        ] {
            assert!(writer.write(area, offset, &bytes).is_err());
        }
        let mut previous = PAGE_PROTECTION_FLAGS::default();
        for protection in [PAGE_READONLY, PAGE_EXECUTE_READWRITE] {
            unsafe {
                VirtualProtect(buffer.0, 4096, protection, &mut previous).unwrap();
            }
            assert!(writer.write(Area::Record, 0, &[7]).is_err());
        }
        unsafe {
            VirtualProtect(buffer.0, 4096, PAGE_READWRITE, &mut previous).unwrap();
        }
        assert_eq!(writer.reader().read(address, 1).unwrap(), [0]);
    }
    #[test]
    fn real_read_handle_has_no_write_rights() {
        use windows::Win32::System::Diagnostics::Debug::WriteProcessMemory;
        for rights in [
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
            PROCESS_QUERY_INFORMATION | PROCESS_VM_READ,
        ] {
            let h = Handle(unsafe { OpenProcess(rights, false, std::process::id()).unwrap() });
            let mut target = [1u8, 2, 3, 4];
            let mut out = [0u8; 4];
            let mut done = 0;
            unsafe {
                ReadProcessMemory(
                    h.0,
                    target.as_ptr() as _,
                    out.as_mut_ptr() as _,
                    4,
                    Some(&mut done),
                )
                .unwrap();
            }
            assert_eq!(out, target);
            assert_eq!(done, 4);
            let replacement = [9u8; 4];
            assert!(
                unsafe {
                    WriteProcessMemory(
                        h.0,
                        target.as_mut_ptr() as _,
                        replacement.as_ptr() as _,
                        4,
                        Some(&mut done),
                    )
                }
                .is_err()
            );
            assert_eq!(target, [1, 2, 3, 4]);
            if rights == PROCESS_QUERY_INFORMATION | PROCESS_VM_READ {
                let reader = Reader {
                    handle: h,
                    identity: crate::session_safety::identity(std::process::id()).unwrap(),
                    base: 0,
                };
                let address = target.as_ptr() as u64;
                let (base, len) = reader.roster_region(address).unwrap();
                assert!(base <= address && base + len >= address + 4);
            }
        }
    }
    #[test]
    fn team_writer_is_fixed_span_private_nonexecutable_data_only() {
        use crate::team_changes::{ARENA_STRIDE, Area, TEAM_STRIDE};
        struct Buffer(*mut std::ffi::c_void);
        impl Drop for Buffer {
            fn drop(&mut self) {
                unsafe {
                    VirtualFree(self.0, 0, MEM_RELEASE).unwrap();
                }
            }
        }
        let b =
            Buffer(unsafe { VirtualAlloc(None, 16384, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE) });
        assert!(!b.0.is_null());
        let at = b.0 as u64;
        let reader = Reader {
            handle: Handle(unsafe {
                OpenProcess(
                    PROCESS_QUERY_INFORMATION
                        | PROCESS_VM_READ
                        | PROCESS_VM_WRITE
                        | PROCESS_VM_OPERATION,
                    false,
                    std::process::id(),
                )
                .unwrap()
            }),
            identity: session_safety::identity(std::process::id()).unwrap(),
            base: 0,
        };
        let mut writer = TeamWriter {
            reader,
            record: at,
            clone_record: Some(at + 8192),
        };
        writer.write(Area::Team, 0x2FA, &[1, 2]).unwrap();
        writer
            .write(Area::Clone, ARENA_STRIDE - 128, &[9; 128])
            .unwrap();
        assert_eq!(writer.reader().read(at + 0x2FA, 2).unwrap(), [1, 2]);
        for (a, o, bytes) in [
            (Area::Team, TEAM_STRIDE, vec![1]),
            (Area::Clone, ARENA_STRIDE - 1, vec![1, 2]),
            (Area::Team, usize::MAX, vec![1]),
            (Area::Clone, 0, vec![1; 129]),
            (Area::Team, 0, vec![]),
        ] {
            assert!(writer.write(a, o, &bytes).is_err());
        }
        writer.clone_record = None;
        assert!(writer.write(Area::Clone, 0, &[1]).is_err());
        let mut previous = PAGE_PROTECTION_FLAGS::default();
        for p in [PAGE_READONLY, PAGE_EXECUTE_READWRITE] {
            unsafe {
                VirtualProtect(b.0, 16384, p, &mut previous).unwrap();
            }
            assert!(writer.write(Area::Team, 0, &[7]).is_err());
        }
        unsafe {
            VirtualProtect(b.0, 16384, PAGE_READWRITE, &mut previous).unwrap();
        }
        assert_eq!(writer.reader().read(at, 1).unwrap(), [0]);
    }
    #[test]
    fn uniform_writer_only_accepts_mapped_words_and_preserves_neighbor_bits() {
        struct Buffer(*mut std::ffi::c_void);
        impl Drop for Buffer {
            fn drop(&mut self) {
                unsafe {
                    VirtualFree(self.0, 0, MEM_RELEASE).unwrap();
                }
            }
        }
        let b =
            Buffer(unsafe { VirtualAlloc(None, 4096, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE) });
        assert!(!b.0.is_null());
        let at = b.0 as u64;
        let reader = Reader {
            handle: Handle(unsafe {
                OpenProcess(
                    PROCESS_QUERY_INFORMATION
                        | PROCESS_VM_READ
                        | PROCESS_VM_WRITE
                        | PROCESS_VM_OPERATION,
                    false,
                    std::process::id(),
                )
                .unwrap()
            }),
            identity: session_safety::identity(std::process::id()).unwrap(),
            base: 0,
        };
        let writer = UniformWriter { reader, record: at };
        writer.write(0x10C, &0x7F123456u32.to_le_bytes()).unwrap();
        writer.write(0x160, &20u32.to_le_bytes()).unwrap();
        assert_eq!(
            writer.reader().read(at + 0x10C, 4).unwrap(),
            0x7F123456u32.to_le_bytes()
        );
        for (offset, bytes) in [
            (0, vec![1; 4]),
            (0x50, vec![1; 4]),
            (usize::MAX, vec![1; 4]),
            (0x160, vec![255; 4]),
            (0x164, 1u32.to_le_bytes().to_vec()),
            (0x16C, 0x08000000u32.to_le_bytes().to_vec()),
            (0x10C, vec![1; 3]),
            (0x170, vec![1; 4]),
        ] {
            assert!(writer.write(offset, &bytes).is_err());
        }
        let mut previous = PAGE_PROTECTION_FLAGS::default();
        for p in [PAGE_READONLY, PAGE_EXECUTE_READWRITE] {
            unsafe {
                VirtualProtect(b.0, 4096, p, &mut previous).unwrap();
            }
            assert!(writer.write(0x10C, &0u32.to_le_bytes()).is_err());
        }
        unsafe {
            VirtualProtect(b.0, 4096, PAGE_READWRITE, &mut previous).unwrap();
        }
        assert_eq!(
            writer.reader().read(at + 0x160, 4).unwrap(),
            20u32.to_le_bytes()
        );
    }
    #[test]
    fn guard_refuses_foreign_reused_unknown_and_protected_process() {
        let id = Identity {
            pid: 2,
            started: "100".into(),
            path: "C:\\Game\\NBA2K27.exe".into(),
        };
        let game = || Process {
            name: "NBA2K27.exe".into(),
            pid: 2,
            parent: 1,
            identity: Some(id.clone()),
        };
        assert!(guard(&[game()], &id).is_ok());
        let mut reused = game();
        reused.identity.as_mut().unwrap().started = "101".into();
        assert!(guard(&[reused], &id).is_err());
        let mut unknown = game();
        unknown.identity = None;
        assert!(guard(&[unknown], &id).is_err());
        for name in [
            "start_protected_game.exe",
            "EasyAntiCheat_EOS.exe",
            "2k Launcher WPF.exe",
        ] {
            assert!(
                guard(
                    &[
                        game(),
                        Process {
                            name: name.into(),
                            pid: 3,
                            parent: 1,
                            identity: None
                        }
                    ],
                    &id
                )
                .is_err()
            );
        }
    }
    #[test]
    fn arena_worker_exempts_only_its_exact_launcher_parent_not_foreign_or_reused_owners() {
        let id = Identity {
            pid: 2,
            started: "100".into(),
            path: r"C:\Game\NBA2K27.exe".into(),
        };
        let owner = Identity {
            pid: 3,
            started: "200".into(),
            path: r"C:\QA\App\2k Launcher Rust Prototype.exe".into(),
        };
        let game = || Process {
            name: "NBA2K27.exe".into(),
            pid: id.pid,
            parent: 1,
            identity: Some(id.clone()),
        };
        let parent = || Process {
            name: "2k Launcher Rust Prototype.exe".into(),
            pid: owner.pid,
            parent: 1,
            identity: Some(owner.clone()),
        };
        // The old generic guard rejects the legitimate UI in the worker.
        assert!(guard(&[game(), parent()], &id).is_err());
        arena_guard(&[game(), parent()], &id, &owner).unwrap();
        // Owner exit alone does not stop safe selection-bound restoration.
        arena_guard(&[game()], &id, &owner).unwrap();
        let mut reused = parent();
        reused.identity.as_mut().unwrap().started = "201".into();
        assert!(arena_guard(&[game(), reused], &id, &owner).is_err());
        let mut unknown = parent();
        unknown.identity = None;
        assert!(arena_guard(&[game(), unknown], &id, &owner).is_err());
        for name in [
            "2k Launcher WPF.exe",
            "EasyAntiCheat_EOS.exe",
            "start_protected_game.exe",
        ] {
            let mut other = parent();
            other.name = name.into();
            other.pid = 4;
            other.identity.as_mut().unwrap().pid = 4;
            assert!(arena_guard(&[game(), parent(), other], &id, &owner).is_err());
        }
        let mut protected = parent();
        protected.name = "EasyAntiCheat_EOS.exe".into();
        assert!(arena_guard(&[game(), protected], &id, &owner).is_err());
    }
    #[cfg(test)]
    include!("blacktop_observer_profile_tests.rs");
}
pub fn smoke() -> Result<Value> {
    tests::smoke()
}
pub(crate) fn rules_fixture() -> Result<crate::game_rules::Snapshot> {
    tests::rules_fixture()
}
#[cfg(test)]
pub(crate) fn selection_memory_fixture() -> std::collections::HashMap<u64, Vec<u8>> {
    tests::selection_memory()
}
