//! Separate Version 2 data capability. This does NOT relax the legacy editor
//! gate or grant write rights to the observer. No executable memory, code
//! patches, remote calls, threads, or addresses from the UI are accepted.
use super::*;
use crate::feature_journal::{Event, Intent, Journal};

#[derive(Clone, Copy, Debug)]
pub(crate) enum Feature {
    Camera,
    Scaler,
    Freestyle,
    Debug,
}
impl Feature {
    pub fn name(self) -> &'static str {
        match self {
            Self::Camera => "cameras",
            Self::Scaler => "scaler",
            Self::Freestyle => "wnba-freestyle",
            Self::Debug => "debug-menus",
        }
    }
    fn verify(self, m: &impl Memory) -> Result<()> {
        match self {
            Self::Camera => crate::camera_v2::verify_profile(m),
            Self::Scaler => crate::scaler_v2::verify_profile(m),
            Self::Freestyle => crate::freestyle_v2::validate_profile(m),
            Self::Debug => crate::debug_menu_v2::verify_profile(m),
        }
    }
}
fn contains(start: u64, size: usize, at: u64, len: usize) -> bool {
    len > 0
        && at >= start
        && at
            .checked_add(len as u64)
            .zip(start.checked_add(size as u64))
            .is_some_and(|(end, limit)| end <= limit)
}
fn image_allowed(feature: Feature, rva: u64, len: usize) -> bool {
    match feature {
        Feature::Camera => {
            let mode = rva
                .checked_sub(crate::camera_v2::MODES)
                .filter(|n| *n < 13 * 16)
                .is_some_and(|n| matches!((n % 16, len), (0, 8) | (8, 4)));
            let registry = rva
                .checked_sub(crate::camera_v2::REGISTRY + 63 * 64)
                .filter(|n| *n < 9 * 64)
                .is_some_and(|n| matches!((n % 64, len), (0, 64) | (0x20, 8)));
            let label = len == 4
                && rva
                    .checked_sub(crate::camera_v2::LABELS + 63 * 4)
                    .is_some_and(|n| n < 9 * 4 && n % 4 == 0);
            let flags = len == 1
                && [
                    crate::camera_v2::STATIC_AVAILABILITY + 63,
                    crate::camera_v2::RUNTIME_AVAILABILITY + 63,
                ]
                .into_iter()
                .any(|start| contains(start, 9, rva, 1));
            mode || registry || label || flags
        }
        Feature::Scaler => rva == 0x0A41D07C && len == 4,
        Feature::Freestyle => len == 4 && crate::freestyle_v2::CHANGES.iter().any(|c| c.rva == rva),
        Feature::Debug => false,
    }
}
pub(crate) struct Access {
    reader: Reader,
    writer: Option<Handle>,
    owner: Identity,
    allocations: Vec<(u64, usize)>,
    batch: Option<(Feature, std::time::Instant)>,
}
/// A verified transaction is short: one ownership scan and profile proof
/// cover its bounded writes, like the WPF helper's single validated handle.
/// Every write still checks liveness, retained identity, region kind and its
/// exact before-bytes. The proof expires instead of outliving a transaction.
const BATCH_PROOF: std::time::Duration = std::time::Duration::from_secs(2);
impl Access {
    pub fn open(id: &Identity, game: &Path, owner: &Identity) -> Result<Self> {
        arena_guard(&session_safety::processes()?, id, owner)?;
        // This first handle validates module path, protected modules and the
        // independently gated Version 2 read-only observer identity.
        let observed = Reader::open_observer(id, game)?;
        let handle = Handle(unsafe {
            OpenProcess(
                PROCESS_QUERY_INFORMATION | PROCESS_VM_READ | PROCESS_SYNCHRONIZE,
                false,
                id.pid,
            )?
        });
        ensure!(
            handle_identity(handle.0, id.pid)? == *id,
            "Feature read handle identity changed"
        );
        let access = Self {
            reader: Reader {
                handle,
                identity: id.clone(),
                base: observed.base,
            },
            writer: None,
            owner: owner.clone(),
            allocations: Vec::new(),
            batch: None,
        };
        access.check()?;
        Ok(access)
    }
    /// Integrity/profile failures remain errors. Ordinary menu transitions and
    /// still-loaded players are a non-error NotReady state for bounded retry.
    pub fn camera_ready_boundary(&self) -> Result<Option<String>> {
        // Initial Home can be stage zero, before Blacktop has ever opened.
        // Keep this camera-only rule separate from the global F8 observer.
        let profile = observer_profile::validate(&self.reader)?;
        let before = context_with_profile(&self.reader, &profile)?;
        let Some(boundary) = inactive_camera_menu(&before) else {
            return Ok(None);
        };
        crate::scaler_v2::verify_profile(self)?;
        if crate::scaler_v2::snapshot_readiness(self)?.loaded() {
            return Ok(None);
        }
        if context_with_profile(&self.reader, &profile)? != before {
            return Ok(None);
        }
        Ok(Some(boundary.into()))
    }
    /// Loader preparation uses the same fully inactive Home/Quick Play proof as
    /// camera publication, without coupling it to player-model scale readiness.
    pub fn inactive_launch_menu(&self) -> Result<Option<&'static str>> {
        let profile = observer_profile::validate(&self.reader)?;
        let before = context_with_profile(&self.reader, &profile)?;
        let menu = inactive_camera_menu(&before);
        ensure!(
            context_with_profile(&self.reader, &profile)? == before,
            "Launch menu changed during loader readiness capture"
        );
        Ok(menu)
    }
    fn check(&self) -> Result<()> {
        ensure!(
            unsafe { WaitForSingleObject(self.reader.handle.0, 0) } == WAIT_TIMEOUT,
            "Feature game process ended or liveness is unknown"
        );
        ensure!(
            handle_identity(self.reader.handle.0, self.reader.identity.pid)?
                == self.reader.identity,
            "Feature retained identity changed"
        );
        Ok(())
    }
    pub fn guard(&self) -> Result<()> {
        self.check()?;
        arena_guard(
            &session_safety::processes()?,
            &self.reader.identity,
            &self.owner,
        )
    }
    fn open_writer(&mut self) -> Result<HANDLE> {
        if self.writer.is_none() {
            let h = Handle(unsafe {
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
                handle_identity(h.0, self.reader.identity.pid)? == self.reader.identity
                    && unsafe { WaitForSingleObject(h.0, 0) } == WAIT_TIMEOUT,
                "Feature write handle identity changed"
            );
            self.writer = Some(h);
        }
        Ok(self.writer.as_ref().unwrap().0)
    }
    fn writer(&mut self, feature: Feature) -> Result<HANDLE> {
        let proven = self.batch.is_some_and(|(owner, started)| {
            owner.name() == feature.name() && started.elapsed() < BATCH_PROOF
        });
        if !proven {
            self.guard()?;
            feature.verify(self)?;
        }
        self.open_writer()
    }
    /// Full ownership scan and feature profile once for a bounded transaction.
    pub fn begin_batch(&mut self, feature: Feature) -> Result<()> {
        self.batch = None;
        self.guard()?;
        feature.verify(self)?;
        self.batch = Some((feature, std::time::Instant::now()));
        Ok(())
    }
    pub fn end_batch(&mut self) {
        self.batch = None;
    }
    fn loader_writer(&mut self) -> Result<HANDLE> {
        self.guard()?;
        crate::wnba_loader_v2::validate_profile(self)?;
        self.open_writer()
    }
    pub fn wnba_loader_current(&self) -> Result<u64> {
        crate::wnba_loader_v2::current(self)
    }
    /// Change exactly one stock TKU virtual entry between the game's own NBA
    /// and WNBA roster-loader thunks. This is deliberately separate from the
    /// generic data writer and its four-DWORD Freestyle capability.
    pub fn swap_wnba_loader(
        &mut self,
        before: u64,
        after: u64,
        journal: &mut Journal,
    ) -> Result<()> {
        // Crash-safety withdrawal is enforced at the writer itself as well as
        // by the runtime scheduler. This precedes profile reads, journal intent,
        // protection changes and the retained write handle.
        crate::wnba_loader_v2::require_runtime_enabled()?;
        ensure!(
            crate::wnba_loader_v2::permitted(before, after),
            "WNBA loader transition escaped the two stock endpoints"
        );
        crate::wnba_loader_v2::validate_profile(self)?;
        ensure!(
            self.wnba_loader_current()? == before,
            "Freestyle roster-loader ownership changed"
        );
        let at = self
            .base()
            .checked_add(crate::wnba_loader_v2::TKU_SLOT)
            .context("WNBA loader slot overflow")?;
        let before_bytes = self
            .base()
            .checked_add(before)
            .context("WNBA loader source overflow")?
            .to_le_bytes();
        let after_bytes = self
            .base()
            .checked_add(after)
            .context("WNBA loader target overflow")?
            .to_le_bytes();
        let change = crate::feature_journal::Change {
            at,
            before: before_bytes.to_vec(),
            after: after_bytes.to_vec(),
        };
        let intent = journal.write_intent("wnba-freestyle-loader", vec![change])?;
        ensure!(
            intent.permits("wnba-freestyle-loader", at, &before_bytes, &after_bytes),
            "WNBA loader intent token differs"
        );
        let _gate = session_safety::write_gate()?;
        let handle = self.loader_writer()?;
        let info = self.region(at, 8)?;
        ensure!(
            info.Type == MEM_IMAGE
                && info.AllocationBase as u64 == self.base()
                && info.Protect == PAGE_READONLY,
            "WNBA loader slot protection differs"
        );
        ensure!(
            self.read(at, 8)? == before_bytes,
            "WNBA loader changed before protection transition"
        );
        journal.append(Event::ProtectionIntent {
            at,
            bytes: 8,
            original: info.Protect.0,
        })?;
        let protection = Protection {
            handle,
            at,
            len: 8,
            original: info.Protect,
            changed: false,
        };
        let mut operation = NativeData {
            access: self,
            handle,
            at,
            before: &before_bytes,
            after: &after_bytes,
            protection,
        };
        protected_write(&mut operation, PAGE_READONLY.0)?;
        crate::wnba_loader_v2::validate_profile(self)?;
        ensure!(
            self.wnba_loader_current()? == after,
            "WNBA loader pointer readback differs"
        );
        journal.append(Event::Applied {
            feature: "wnba-freestyle-loader".into(),
        })
    }
    fn region(&self, at: u64, len: usize) -> Result<MEMORY_BASIC_INFORMATION> {
        self.check()?;
        ensure!(
            len > 0 && len <= 8 * 1024 * 1024 && at >= 0x10000,
            "Invalid feature region bounds"
        );
        let mut info = MEMORY_BASIC_INFORMATION::default();
        let size = unsafe {
            VirtualQueryEx(
                self.reader.handle.0,
                Some(at as _),
                &mut info,
                size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        ensure!(
            size == size_of::<MEMORY_BASIC_INFORMATION>()
                && info.State == MEM_COMMIT
                && data_protection(info.Type, info.Protect)
                && contains(info.BaseAddress as u64, info.RegionSize, at, len),
            "Feature span is not committed readable non-executable data"
        );
        Ok(info)
    }
    pub fn allocate_camera(&mut self, bytes: usize, journal: &mut Journal) -> Result<u64> {
        ensure!(
            (4096..=1024 * 1024).contains(&bytes) && bytes % 4096 == 0,
            "Camera allocation exceeds bounds"
        );
        let _gate = session_safety::write_gate()?;
        journal.append(Event::AllocationIntent { bytes })?;
        let h = self.writer(Feature::Camera)?;
        let p = unsafe { VirtualAllocEx(h, None, bytes, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE) }
            as u64;
        ensure!(p >= 0x10000, "Camera data allocation failed");
        // Record ownership immediately, including errors while recording the
        // address. No Drop frees possibly referenced game memory.
        self.allocations.push((p, bytes));
        journal.append(Event::Allocated { at: p, bytes })?;
        let info = self.region(p, bytes)?;
        ensure!(
            info.Type == MEM_PRIVATE
                && info.AllocationBase as u64 == p
                && info.Protect == PAGE_READWRITE,
            "Camera allocation protection changed"
        );
        self.check()?;
        Ok(p)
    }
    pub fn initialize(&mut self, at: u64, bytes: &[u8], journal: &mut Journal) -> Result<()> {
        ensure!(
            !bytes.is_empty()
                && bytes.len() <= 0x10000
                && self.allocations.iter().any(|(start, size)| contains(
                    *start,
                    *size,
                    at,
                    bytes.len()
                )),
            "Private camera initialization escaped its retained allocation"
        );
        // Allocations are zero-initialized; check instead of adopting unknown
        // pre-existing bytes. Runtime writes cannot route through this method.
        for offset in (0..bytes.len()).step_by(4096) {
            let chunk = &bytes[offset..(offset + 4096).min(bytes.len())];
            self.write_inner(
                Feature::Camera,
                at + offset as u64,
                &vec![0; chunk.len()],
                chunk,
                journal,
            )?;
        }
        Ok(())
    }
    pub fn write(
        &mut self,
        feature: Feature,
        at: u64,
        before: &[u8],
        after: &[u8],
        intent: &Intent,
        journal: &mut Journal,
    ) -> Result<()> {
        ensure!(
            intent.permits(feature.name(), at, before, after),
            "No durable intent authorizes this exact feature write"
        );
        self.write_inner(feature, at, before, after, journal)
    }
    /// Scaler character groups only: one opt-in byte or one 8-byte scale pair
    /// in a live game heap object. `verify` re-proves that object's identity
    /// immediately before the write (WPF ActiveCoach/ActiveArena parity); the
    /// image, executable or non-private memory is never a valid target.
    pub fn write_object(
        &mut self,
        at: u64,
        before: &[u8],
        after: &[u8],
        intent: &Intent,
        journal: &mut Journal,
        verify: impl Fn(&Self) -> Result<()>,
    ) -> Result<()> {
        let _ = journal;
        ensure!(
            intent.permits(Feature::Scaler.name(), at, before, after),
            "No durable intent authorizes this exact character write"
        );
        ensure!(
            before.len() == after.len() && matches!(after.len(), 1 | 8),
            "Invalid bounded character write"
        );
        ensure!(
            at.checked_add(after.len() as u64).is_some_and(
                |end| end <= self.base() || at >= self.base() + crate::camera_v2::IMAGE_BYTES
            ),
            "Character write targets the game image"
        );
        let _gate = session_safety::write_gate()?;
        let h = self.writer(Feature::Scaler)?;
        let info = self.region(at, after.len())?;
        ensure!(
            info.Type == MEM_PRIVATE && info.Protect == PAGE_READWRITE,
            "Character data is not private read/write memory"
        );
        // A live character can unload, swap or refresh its size cache between
        // planning and this write. That is a normal transition, not a conflict:
        // report it as a typed, retryable outcome with nothing overwritten.
        if let Err(error) = verify(self) {
            return Err(anyhow::Error::new(CharacterMoved).context(format!("{error:#}")));
        }
        if self.read(at, before.len())? != before {
            return Err(CharacterMoved.into());
        }
        let protection = Protection {
            handle: h,
            at,
            len: after.len(),
            original: info.Protect,
            changed: false,
        };
        NativeData {
            access: self,
            handle: h,
            at,
            before,
            after,
            protection,
        }
        .write_verify()
    }
    /// One Boolean value in the exact-build feature registry. The address is
    /// accepted only when two full captures identify the same allowlisted
    /// entry and the image page remains ordinary writable data.
    pub fn write_debug(
        &mut self,
        change: &crate::debug_menu_v2::Change,
        before: &crate::debug_menu_v2::Snapshot,
        after: &crate::debug_menu_v2::Snapshot,
        intent: &Intent,
        journal: &mut Journal,
    ) -> Result<()> {
        let _ = journal;
        let before_byte = [change.before];
        let after_byte = [change.after];
        ensure!(
            intent.permits(
                Feature::Debug.name(),
                change.address,
                &before_byte,
                &after_byte
            ) && change.before <= 1
                && change.after <= 1
                && change.before != change.after,
            "No durable intent authorizes this debug-menu Boolean"
        );
        ensure!(
            crate::debug_menu_v2::capture(self)? == *before,
            "Debug-menu ownership changed before write"
        );
        let _gate = session_safety::write_gate()?;
        let handle = self.writer(Feature::Debug)?;
        let info = self.region(change.address, 1)?;
        ensure!(
            info.Type == MEM_IMAGE
                && info.AllocationBase as u64 == self.base()
                && info.Protect == PAGE_READWRITE,
            "Debug-menu value is not writable game-image data"
        );
        ensure!(
            crate::debug_menu_v2::capture(self)? == *before,
            "Debug-menu ownership changed at publication boundary"
        );
        NativeData {
            access: self,
            handle,
            at: change.address,
            before: &before_byte,
            after: &after_byte,
            protection: Protection {
                handle,
                at: change.address,
                len: 1,
                original: info.Protect,
                changed: false,
            },
        }
        .write_verify()?;
        ensure!(
            crate::debug_menu_v2::capture(self)? == *after,
            "Debug-menu readback or unchanged-entry verification failed"
        );
        Ok(())
    }
    fn write_inner(
        &mut self,
        feature: Feature,
        at: u64,
        before: &[u8],
        after: &[u8],
        journal: &mut Journal,
    ) -> Result<()> {
        ensure!(
            before.len() == after.len() && !after.is_empty() && after.len() <= 4096,
            "Invalid bounded feature write"
        );
        let image = at
            .checked_sub(self.base())
            .is_some_and(|rva| image_allowed(feature, rva, after.len()));
        let private = matches!(feature, Feature::Camera)
            && self
                .allocations
                .iter()
                .any(|(start, size)| contains(*start, *size, at, after.len()));
        ensure!(
            image || private,
            "Feature write escaped its fixed data allowlist"
        );
        let _gate = session_safety::write_gate()?;
        let h = self.writer(feature)?;
        let info = self.region(at, after.len())?;
        ensure!(
            if image {
                info.Type == MEM_IMAGE && info.AllocationBase as u64 == self.base()
            } else {
                info.Type == MEM_PRIVATE && info.Protect == PAGE_READWRITE
            },
            "Feature allocation kind changed"
        );
        ensure!(
            self.read(at, before.len())? == before,
            "Feature write precondition changed; nothing overwritten"
        );
        let protection = Protection {
            handle: h,
            at,
            len: after.len(),
            original: info.Protect,
            changed: false,
        };
        let mut operation = NativeData {
            access: self,
            handle: h,
            at,
            before,
            after,
            protection,
        };
        if info.Protect == PAGE_READONLY {
            journal.append(Event::ProtectionIntent {
                at,
                bytes: after.len(),
                original: info.Protect.0,
            })?;
            protected_write(&mut operation, info.Protect.0)
        } else {
            operation.write_verify()
        }
    }
}
/// Untouched image .data pages are copy-on-write until the game first writes
/// them (observed live: camera tables at ~16 s after launch). WriteProcessMemory
/// completes the copy like the WPF helper's plain write did. Copy-on-write is
/// accepted only for game-image data; private memory must be read/write and
/// executable protections are never accepted.
const PAGE_WRITECOPY_DATA: PAGE_PROTECTION_FLAGS = PAGE_PROTECTION_FLAGS(0x08);
/// A planned character write found its object changed (unloaded, swapped or
/// refreshed) and wrote nothing. Typed so callers never infer it from text.
#[derive(Debug)]
pub(crate) struct CharacterMoved;
impl std::fmt::Display for CharacterMoved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Character data changed before write; nothing overwritten")
    }
}
impl std::error::Error for CharacterMoved {}
fn data_protection(kind: PAGE_TYPE, protect: PAGE_PROTECTION_FLAGS) -> bool {
    matches!(protect, PAGE_READONLY | PAGE_READWRITE)
        || (kind == MEM_IMAGE && protect == PAGE_WRITECOPY_DATA)
}
fn inactive_camera_menu(c: &ContextState) -> Option<&'static str> {
    if c.current != c.alternate || c.active != 0 || c.kind != 0 || c.game_mode != 0 {
        return None;
    }
    match (c.current, c.stage) {
        (0x6509837B, 0 | 1) => Some("main-menu"),
        (0x079964C0, 1) => Some("quick-play"),
        _ => None,
    }
}
/// Shared by the real Win32 adapter and fault-injection tests. Restoration is
/// attempted even when the returned original protection or data write differs.
trait ProtectedData {
    fn make_writable(&mut self) -> Result<u32>;
    fn write_verify(&mut self) -> Result<()>;
    fn restore(&mut self, original: u32) -> Result<()>;
}
fn protected_write(operation: &mut impl ProtectedData, expected: u32) -> Result<()> {
    let original = operation.make_writable()?;
    let result = if original == expected {
        operation.write_verify()
    } else {
        Err(anyhow::anyhow!(
            "Feature page changed before protection transition"
        ))
    };
    let restored = operation.restore(original);
    match (result, restored) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => {
            Err(error.context("Data written but original page protection could not be verified"))
        }
        (Err(error), Err(restore)) => anyhow::bail!(
            "Feature write held: {error:#}; original protection restore also failed: {restore:#}"
        ),
    }
}
struct NativeData<'a> {
    access: &'a Access,
    handle: HANDLE,
    at: u64,
    before: &'a [u8],
    after: &'a [u8],
    protection: Protection,
}
impl ProtectedData for NativeData<'_> {
    fn make_writable(&mut self) -> Result<u32> {
        let mut old = PAGE_PROTECTION_FLAGS(0);
        unsafe {
            VirtualProtectEx(
                self.handle,
                self.at as _,
                self.after.len(),
                PAGE_READWRITE,
                &mut old,
            )?;
        }
        self.protection.changed = true;
        self.protection.original = old;
        Ok(old.0)
    }
    fn write_verify(&mut self) -> Result<()> {
        self.access.check()?;
        ensure!(
            self.access.read(self.at, self.before.len())? == self.before,
            "Feature data changed during protection transition"
        );
        let mut count = 0;
        unsafe {
            WriteProcessMemory(
                self.handle,
                self.at as _,
                self.after.as_ptr() as _,
                self.after.len(),
                Some(&mut count),
            )?;
        }
        ensure!(
            count == self.after.len(),
            "Partial feature write; held until game exit"
        );
        ensure!(
            self.access.read(self.at, self.after.len())? == self.after,
            "Feature write readback failed"
        );
        self.access.check()
    }
    fn restore(&mut self, original: u32) -> Result<()> {
        ensure!(
            self.protection.original.0 == original,
            "Protection receipt differs"
        );
        self.protection.restore()
    }
}
struct Protection {
    handle: HANDLE,
    at: u64,
    len: usize,
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
                    && contains(info.BaseAddress as u64, info.RegionSize, self.at, self.len),
                "Restored feature protection could not be verified"
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
impl Memory for Access {
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
impl crate::freestyle_v2::Regions for Access {
    fn region(&self, at: u64, len: usize) -> Result<crate::freestyle_v2::Region> {
        use crate::freestyle_v2::{Region, RegionKind};
        let info = Access::region(self, at, len)?;
        ensure!(
            info.Type == MEM_IMAGE || info.Type == MEM_PRIVATE,
            "Unsupported Freestyle region kind"
        );
        Ok(Region {
            allocation: info.AllocationBase as u64,
            kind: if info.Type == MEM_IMAGE {
                RegionKind::Image
            } else {
                RegionKind::Private
            },
            readable: true,
            writable: matches!(info.Protect, PAGE_READWRITE | PAGE_WRITECOPY_DATA),
            executable: false,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn feature_allowlists_exclude_code_other_features_and_original_cameras() {
        for f in [Feature::Camera, Feature::Scaler, Feature::Freestyle] {
            for rva in [0x1000, 0x6E476A, 0x0992C830, 0, u64::MAX] {
                assert!(!image_allowed(f, rva, 4));
            }
        }
        assert!(image_allowed(Feature::Scaler, 0x0A41D07C, 4));
        assert!(!image_allowed(Feature::Scaler, 0x0A41D07C, 8));
        assert!(!image_allowed(Feature::Freestyle, 0x0A41D07C, 4));
        for c in crate::freestyle_v2::CHANGES {
            assert!(image_allowed(Feature::Freestyle, c.rva, 4));
        }
        assert!(image_allowed(
            Feature::Camera,
            crate::camera_v2::REGISTRY + 63 * 64,
            64
        ));
        assert!(!image_allowed(
            Feature::Camera,
            crate::camera_v2::REGISTRY + 72 * 64 - 1,
            2
        ));
        assert!(!image_allowed(
            Feature::Camera,
            crate::camera_v2::MODES + 12,
            4
        ));
        assert!(!image_allowed(
            Feature::Camera,
            crate::camera_v2::REGISTRY + 63 * 64 + 0x10,
            8
        ));
        assert!(!contains(0x10000, 4096, u64::MAX, 4));
    }
    #[test]
    fn copy_on_write_is_accepted_only_for_image_data_never_private_or_executable() {
        assert!(data_protection(MEM_IMAGE, PAGE_WRITECOPY_DATA));
        assert!(data_protection(MEM_IMAGE, PAGE_READWRITE));
        assert!(data_protection(MEM_IMAGE, PAGE_READONLY));
        assert!(!data_protection(MEM_PRIVATE, PAGE_WRITECOPY_DATA));
        assert!(data_protection(MEM_PRIVATE, PAGE_READWRITE));
        for executable in [0x10u32, 0x20, 0x40, 0x80] {
            assert!(!data_protection(
                MEM_IMAGE,
                PAGE_PROTECTION_FLAGS(executable)
            ));
            assert!(!data_protection(
                MEM_PRIVATE,
                PAGE_PROTECTION_FLAGS(executable)
            ));
        }
        assert!(!data_protection(MEM_IMAGE, PAGE_PROTECTION_FLAGS(0x01)));
        assert!(!data_protection(
            MEM_IMAGE,
            PAGE_PROTECTION_FLAGS(0x08 | 0x100)
        ));
    }
    #[test]
    fn protection_faults_always_restore_the_actual_original_and_report_failures() {
        struct Fake {
            original: u32,
            fail_write: bool,
            fail_restore: bool,
            trace: Vec<String>,
        }
        impl ProtectedData for Fake {
            fn make_writable(&mut self) -> Result<u32> {
                self.trace.push("protect".into());
                Ok(self.original)
            }
            fn write_verify(&mut self) -> Result<()> {
                self.trace.push("write-verify".into());
                ensure!(!self.fail_write, "partial write/readback fault");
                Ok(())
            }
            fn restore(&mut self, old: u32) -> Result<()> {
                self.trace.push(format!("restore-{old}"));
                ensure!(!self.fail_restore, "restore fault");
                Ok(())
            }
        }
        for (original, fail_write, fail_restore) in [
            (2, false, false),
            (4, false, false),
            (2, true, false),
            (2, false, true),
            (2, true, true),
        ] {
            let mut fake = Fake {
                original,
                fail_write,
                fail_restore,
                trace: Vec::new(),
            };
            let result = protected_write(&mut fake, 2);
            assert_eq!(
                result.is_ok(),
                original == 2 && !fail_write && !fail_restore
            );
            assert_eq!(fake.trace.last().unwrap(), &format!("restore-{original}"));
            assert_eq!(fake.trace.contains(&"write-verify".into()), original == 2);
            if fail_write && fail_restore {
                let message = format!("{:#}", result.unwrap_err());
                assert!(message.contains("partial") && message.contains("restore fault"));
            }
        }
    }
    #[test]
    fn camera_initial_home_zero_is_safe_but_loaded_active_or_transitional_contexts_are_not() {
        let mut c = ContextState {
            current_index: 0,
            alternate_index: 0,
            current: 0x6509837B,
            alternate: 0x6509837B,
            stage: 0,
            active: 0,
            owner: 0x200000000,
            kind: 0,
            game_mode: 0,
            root_tag: 0,
        };
        assert_eq!(inactive_camera_menu(&c), Some("main-menu"));
        c.stage = 1;
        assert_eq!(inactive_camera_menu(&c), Some("main-menu"));
        c.current = 0x079964C0;
        c.alternate = c.current;
        assert_eq!(inactive_camera_menu(&c), Some("quick-play"));
        c.stage = 0;
        assert_eq!(inactive_camera_menu(&c), None);
        c.stage = 1;
        c.active = 1;
        assert_eq!(inactive_camera_menu(&c), None);
        c.active = 0;
        c.game_mode = 6;
        assert_eq!(inactive_camera_menu(&c), None);
        c.game_mode = 0;
        c.alternate = 0x6509837B;
        assert_eq!(inactive_camera_menu(&c), None);
    }
}
