//! Version 2 arena/ball access for the launch-owned provider worker.
//! Retains read and separately acquired write handles for one exact birth/path.
//! No callbacks, arbitrary address API, executable protection or allocation.
#![allow(dead_code)]
use super::*;
use crate::arena_ball_v2_adapter::{self as adapter, Host, Kind, Permit, ProtectedData};
use std::{
    cell::RefCell,
    fs::{self, File},
    io::Read,
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::PathBuf,
};

pub(crate) type ArenaAccess = adapter::ArenaAccess<Native>;
pub(crate) type BallAccess = adapter::BallAccess<Native>;
pub(crate) fn open_arena(
    root: &Path,
    game: &Path,
    session: &str,
    id: &Identity,
    owner: &Identity,
) -> Result<ArenaAccess> {
    Ok(adapter::ArenaAccess {
        host: Native::open(root, game, session, id, owner, Kind::Arena)?,
        profiled: Default::default(),
    })
}
pub(crate) fn open_ball(
    root: &Path,
    game: &Path,
    session: &str,
    id: &Identity,
    owner: &Identity,
) -> Result<BallAccess> {
    Ok(adapter::BallAccess {
        host: Native::open(root, game, session, id, owner, Kind::Ball)?,
    })
}
pub(crate) struct Native {
    reader: Reader,
    writer: RefCell<Option<Handle>>,
    owner: Identity,
    root: PathBuf,
    game: PathBuf,
    session: String,
    kind: Kind,
    clock: std::time::Instant,
    _pins: Vec<crate::loader_files::Directories>,
}
impl Native {
    fn open(
        root: &Path,
        game: &Path,
        session: &str,
        id: &Identity,
        owner: &Identity,
        kind: Kind,
    ) -> Result<Self> {
        ensure!(
            session.len() == 48 && session.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid native arena/ball session"
        );
        arena_guard(&session_safety::processes()?, id, owner)?;
        ensure!(
            session_safety::identity(owner.pid)? == *owner,
            "Native request owner changed before opening"
        );
        // Validates exact module path, anti-cheat modules and Version 2's
        // independently gated READ-ONLY observer. Never relax validate_build.
        let observed = Reader::open_observer(id, game)?;
        let handle = Handle(unsafe {
            OpenProcess(
                PROCESS_QUERY_INFORMATION | PROCESS_VM_READ | PROCESS_SYNCHRONIZE,
                false,
                id.pid,
            )?
        });
        ensure!(
            handle_identity(handle.0, id.pid)? == *id
                && unsafe { WaitForSingleObject(handle.0, 0) } == WAIT_TIMEOUT,
            "Native read handle identity/liveness changed"
        );
        let value = Self {
            reader: Reader {
                handle,
                identity: id.clone(),
                base: observed.base,
            },
            writer: RefCell::new(None),
            owner: owner.clone(),
            root: root.into(),
            game: game.into(),
            session: session.into(),
            kind,
            clock: std::time::Instant::now(),
            _pins: vec![
                crate::loader_files::Directories::pin(root)?,
                crate::loader_files::Directories::pin(&root.join("Data"))?,
            ],
        };
        value.guard(true)?;
        value.profile()?;
        Ok(value)
    }
    fn check(&self) -> Result<()> {
        ensure!(
            unsafe { WaitForSingleObject(self.reader.handle.0, 0) } == WAIT_TIMEOUT
                && handle_identity(self.reader.handle.0, self.reader.identity.pid)?
                    == self.reader.identity,
            "Native retained game identity ended or became unknown"
        );
        Ok(())
    }
    fn profile(&self) -> Result<()> {
        match self.kind {
            Kind::Arena => crate::arena_v2::verify_profile(self),
            Kind::Ball => crate::ball_v2::verify_profile(self),
        }
    }
    fn writer(&self, parent_required: bool) -> Result<HANDLE> {
        self.guard(parent_required)?;
        self.profile()?;
        // Repeat the read-only protected-module scan immediately before the
        // first/every retained write-handle use, not only when the UI opened.
        let observed = Reader::open_observer(&self.reader.identity, &self.game)?;
        ensure!(
            observed.base == self.reader.base && observed.identity == self.reader.identity,
            "Native module identity changed before write access"
        );
        let mut slot = self.writer.borrow_mut();
        if slot.is_none() {
            let handle = Handle(unsafe {
                OpenProcess(
                    PROCESS_QUERY_INFORMATION
                        | PROCESS_VM_READ
                        | PROCESS_VM_WRITE
                        | PROCESS_VM_OPERATION
                        | PROCESS_SYNCHRONIZE,
                    false,
                    self.reader.identity.pid,
                )?
            });
            ensure!(
                handle_identity(handle.0, self.reader.identity.pid)? == self.reader.identity
                    && unsafe { WaitForSingleObject(handle.0, 0) } == WAIT_TIMEOUT,
                "Native write handle identity/liveness changed"
            );
            *slot = Some(handle);
        }
        Ok(slot.as_ref().context("Native writer unavailable")?.0)
    }
    fn file_bytes(&self, path: &Path, limit: u64, receipt: bool) -> Result<Vec<u8>> {
        Self::open_file_bytes(path, limit, receipt)
    }
    fn open_file_bytes(path: &Path, limit: u64, receipt: bool) -> Result<Vec<u8>> {
        // A session Engine already holds its receipt open with READ sharing
        // only. Our READ handle must share its existing WRITE/DELETE access;
        // that Engine's handle continues to deny every new writer/deleter.
        let file = fs::OpenOptions::new()
            .read(true)
            .share_mode(if receipt { 7 } else { 1 })
            .custom_flags(0x00200000)
            .open(path)?;
        Self::read_file(&file, limit)
    }
    fn read_file(file: &File, limit: u64) -> Result<Vec<u8>> {
        let before = file.metadata()?;
        ensure!(
            before.is_file()
                && before.file_attributes() & 0x400 == 0
                && (1..=limit).contains(&before.len()),
            "Native intent/manifest is not one bounded ordinary file"
        );
        let mut bytes = Vec::new();
        file.take(limit + 1).read_to_end(&mut bytes)?;
        let after = file.metadata()?;
        ensure!(
            bytes.len() as u64 == before.len()
                && after.len() == before.len()
                && after.last_write_time() == before.last_write_time()
                && after.file_attributes() & 0x400 == 0,
            "Native intent/manifest changed while being read"
        );
        Ok(bytes)
    }
    fn span(&self, permit: &Permit) -> Result<crate::blacktop_arena::Region> {
        ensure!(
            permit.kind() == self.kind && permit.valid(self.base()),
            "Native publication escaped its fixed capability"
        );
        let region = crate::blacktop_arena::Inspect::region(self, permit.at())?;
        ensure!(
            region.committed
                && region.allocation == permit.allocation()
                && region.kind
                    == if permit.image() {
                        MEM_IMAGE.0
                    } else {
                        MEM_PRIVATE.0
                    }
                && region.protection
                    == if permit.image() {
                        PAGE_READONLY.0
                    } else {
                        PAGE_READWRITE.0
                    }
                && permit.at() >= region.start
                && permit
                    .at()
                    .checked_add(permit.len() as u64)
                    .zip(region.start.checked_add(region.len))
                    .is_some_and(|(end, limit)| end <= limit),
            "Native publication is not exact owned non-executable image/private data"
        );
        Ok(region)
    }
}
impl Memory for Native {
    fn base(&self) -> u64 {
        self.reader.base
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        self.check()?;
        self.reader.read(at, len)
    }
    fn checkpoint(&self) -> Result<()> {
        self.check()
    }
}
impl crate::blacktop_arena::Inspect for Native {
    fn region(&self, at: u64) -> Result<crate::blacktop_arena::Region> {
        self.check()?;
        self.reader.region(at)
    }
}
impl Host for Native {
    fn kind(&self) -> Kind {
        self.kind
    }
    fn identity(&self) -> &Identity {
        &self.reader.identity
    }
    fn session(&self) -> &str {
        &self.session
    }
    fn guard(&self, parent_required: bool) -> Result<()> {
        self.check()?;
        arena_guard(
            &session_safety::processes()?,
            &self.reader.identity,
            &self.owner,
        )?;
        if parent_required {
            ensure!(
                session_safety::identity(self.owner.pid)? == self.owner,
                "Native preparation owner exited or changed"
            );
        }
        self.check()
    }
    fn receipt(&self) -> Result<Vec<u8>> {
        self.file_bytes(
            &self.root.join("Data").join(self.kind.marker()),
            16384,
            true,
        )
    }
    fn manifest(&self) -> Result<Vec<u8>> {
        self.file_bytes(&self.game.join("manifest"), 64 * 1024 * 1024, false)
    }
    fn next_sample(&self) -> Result<()> {
        self.check()?;
        std::thread::sleep(std::time::Duration::from_millis(100));
        self.check()
    }
    fn clock_ms(&self) -> Result<u64> {
        self.clock
            .elapsed()
            .as_millis()
            .try_into()
            .context("Native ball stability clock overflow")
    }
    fn publish(
        &self,
        permit: &Permit,
        before: &[u8],
        after: &[u8],
        parent_required: bool,
        verify: impl Fn() -> Result<()>,
    ) -> Result<()> {
        ensure!(
            before.len() == permit.len() && after.len() == permit.len() && before != after,
            "Native publication size/no-op differs from permit"
        );
        let _gate = session_safety::write_gate()?;
        // Everything up to here touches neither page protection nor data, so
        // a failure is marked NotPublished and the caller may retry.
        let handle = (|| -> Result<HANDLE> {
            self.guard(parent_required)?;
            self.profile()?;
            self.span(permit)?;
            verify()?;
            ensure!(
                self.read(permit.at(), permit.len())? == before,
                "Native publication precondition changed"
            );
            // Initial snapshot, durable intent, complete preflight and all four
            // ball observations happened BEFORE any VM_WRITE handle acquisition.
            let handle = self.writer(parent_required)?;
            self.guard(parent_required)?;
            self.profile()?;
            self.span(permit)?;
            verify()?;
            Ok(handle)
        })()
        .map_err(|e| e.context(adapter::NotPublished))?;
        let write = || -> Result<()> {
            self.guard(parent_required)?;
            self.profile()?;
            verify()?;
            ensure!(
                self.read(permit.at(), permit.len())? == before,
                "Native data changed at publication boundary"
            );
            let mut count = 0;
            unsafe {
                WriteProcessMemory(
                    handle,
                    permit.at() as _,
                    after.as_ptr() as _,
                    after.len(),
                    Some(&mut count),
                )?;
            }
            ensure!(
                count == after.len() && self.read(permit.at(), permit.len())? == after,
                "Native data write/readback uncertain; retained intent must be held until exact game exit"
            );
            self.check()
        };
        if permit.image() {
            let mut operation = MappingWrite {
                native: self,
                handle,
                permit,
                write: &write,
                protection: Protection {
                    handle,
                    at: permit.at(),
                    len: permit.len(),
                    allocation: self.reader.base,
                    original: PAGE_READONLY,
                    changed: false,
                },
            };
            adapter::protected_write(&mut operation)?;
            self.span(permit)?;
        } else {
            write()?;
            self.span(permit)?;
        }
        self.check()
    }
}
struct MappingWrite<'a, F: Fn() -> Result<()>> {
    native: &'a Native,
    handle: HANDLE,
    permit: &'a Permit,
    write: &'a F,
    protection: Protection,
}
impl<F: Fn() -> Result<()>> ProtectedData for MappingWrite<'_, F> {
    fn make_writable(&mut self) -> Result<u32> {
        self.native.check()?;
        let mut old = PAGE_PROTECTION_FLAGS(0);
        unsafe {
            VirtualProtectEx(
                self.handle,
                self.permit.at() as _,
                self.permit.len(),
                PAGE_READWRITE,
                &mut old,
            )?;
        }
        self.protection.original = old;
        self.protection.changed = true;
        Ok(old.0)
    }
    fn write_verify(&mut self) -> Result<()> {
        (self.write)()
    }
    fn restore(&mut self, original: u32) -> Result<()> {
        ensure!(
            original == self.protection.original.0,
            "Arena protection receipt changed"
        );
        self.protection.restore()
    }
}
struct Protection {
    handle: HANDLE,
    at: u64,
    len: usize,
    allocation: u64,
    original: PAGE_PROTECTION_FLAGS,
    changed: bool,
}
impl Protection {
    fn restore(&mut self) -> Result<()> {
        if self.changed {
            let mut previous = PAGE_PROTECTION_FLAGS(0);
            unsafe {
                VirtualProtectEx(
                    self.handle,
                    self.at as _,
                    self.len,
                    self.original,
                    &mut previous,
                )?;
            }
            let mut info = MEMORY_BASIC_INFORMATION::default();
            let count = unsafe {
                VirtualQueryEx(
                    self.handle,
                    Some(self.at as _),
                    &mut info,
                    size_of::<MEMORY_BASIC_INFORMATION>(),
                )
            };
            ensure!(
                count == size_of::<MEMORY_BASIC_INFORMATION>()
                    && info.State == MEM_COMMIT
                    && info.Protect == self.original
                    && info.Type == MEM_IMAGE
                    && info.AllocationBase as u64 == self.allocation
                    && self.at >= info.BaseAddress as u64
                    && self
                        .at
                        .checked_add(self.len as u64)
                        .is_some_and(|end| end <= info.BaseAddress as u64 + info.RegionSize as u64),
                "Exact original arena page protection restoration could not be verified"
            );
            self.changed = false;
        }
        Ok(())
    }
}
impl Drop for Protection {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn v2_adapter_native_receipt_reader_coexists_with_real_engine_share_lock_and_never_writes() {
        let token = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "c27-v2-intent-share-{}-{token}.json",
            std::process::id()
        ));
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_file(&self.0);
            }
        }
        let _cleanup = Cleanup(path.clone());
        // Same rights/sharing as arena_session/ball_session Receipt::create.
        let mut engine = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .access_mode(0xC0010000)
            .create_new(true)
            .share_mode(1)
            .custom_flags(0x00200000)
            .open(&path)
            .unwrap();
        engine.write_all(b"{\"owned\":true}").unwrap();
        engine.sync_all().unwrap();
        assert_eq!(
            Native::open_file_bytes(&path, 16384, true).unwrap(),
            b"{\"owned\":true}"
        );
        assert!(
            fs::OpenOptions::new()
                .write(true)
                .share_mode(7)
                .open(&path)
                .is_err()
        );
        assert!(fs::remove_file(&path).is_err());
        assert!(Native::open_file_bytes(&path, 4, true).is_err());
        // The launcher's immutable Engine handle—not this reader—owns deletion.
        drop(engine);
        fs::remove_file(&path).unwrap();
    }
}
