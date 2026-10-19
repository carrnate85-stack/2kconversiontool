//! Version 2 rule capability for explicit Refresh/Apply. No legacy profile or
//! global native-write gate is enabled.
//! Query handles are retained; a VM_WRITE handle exists only inside an explicit
//! journal-authorized fixed-field operation and is immediately closed.
#![allow(dead_code)]
use crate::{
    backend,
    blacktop_native::{self, Memory, Reader},
    game_rules::{self, FIELDS, Intent, Lease, Snapshot},
    rules_v2,
    session_safety::{self, Identity, Process},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    io::Read,
    mem::size_of,
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::MutexGuard,
};
use windows::{
    Win32::{
        Foundation::*,
        System::{Diagnostics::Debug::WriteProcessMemory, Memory::*, Threading::*},
    },
    core::PWSTR,
};

const QUERY_RIGHTS: PROCESS_ACCESS_RIGHTS =
    PROCESS_ACCESS_RIGHTS(PROCESS_QUERY_INFORMATION.0 | PROCESS_VM_READ.0 | PROCESS_SYNCHRONIZE.0);
const WRITE_RIGHTS: PROCESS_ACCESS_RIGHTS =
    PROCESS_ACCESS_RIGHTS(QUERY_RIGHTS.0 | PROCESS_VM_WRITE.0 | PROCESS_VM_OPERATION.0);
const OWNER_BYTES: usize = 0xA9C;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Binding {
    game: Identity,
    parent: Identity,
    game_root: PathBuf,
    portable_root: PathBuf,
    session: String,
}
impl Binding {
    /// Trusted native launch record only: never accept a deserialized browser
    /// identity, session, game root, or journal root.
    pub fn from_launched(
        game: Identity,
        parent: Identity,
        game_root: PathBuf,
        portable_root: PathBuf,
        session: String,
    ) -> Result<Self> {
        ensure!(
            game_root.is_absolute()
                && portable_root.is_absolute()
                && Path::new(&game.path).is_absolute()
                && Path::new(&parent.path).is_absolute()
                && game.pid > 0
                && parent.pid > 0
                && game.pid != parent.pid
                && game.started.parse::<u64>().is_ok_and(|v| v > 0)
                && parent.started.parse::<u64>().is_ok_and(|v| v > 0)
                && session.len() == 48
                && session.bytes().all(|v| v.is_ascii_hexdigit())
                && crate::native_session::same_path(&game.path, &game_root.join("NBA2K27.exe")),
            "Invalid owned Version 2 Game Rules binding"
        );
        Ok(Self {
            game,
            parent,
            game_root,
            portable_root,
            session,
        })
    }
}
fn guard_with_current(rows: &[Process], b: &Binding, current: Option<&Identity>) -> Result<()> {
    // The scanner intentionally omits the calling launcher. Its exact retained
    // identity is checked separately, not mistaken for an absent owner.
    ensure!(
        current == Some(&b.parent) || rows.iter().any(|p| p.identity.as_ref() == Some(&b.parent)),
        "Owning launcher is absent or changed"
    );
    let others: Vec<_> = rows
        .iter()
        .filter(|p| {
            !(session_safety::launcher_process(p) && p.identity.as_ref() == Some(&b.parent))
        })
        .map(|p| Process {
            name: p.name.clone(),
            pid: p.pid,
            parent: p.parent,
            identity: p.identity.clone(),
        })
        .collect();
    blacktop_native::guard(&others, &b.game)
}
fn guard(b: &Binding) -> Result<()> {
    guard_with_current(
        &session_safety::processes()?,
        b,
        session_safety::identity(std::process::id()).ok().as_ref(),
    )
}
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
fn live_birth(wait: WAIT_EVENT, birth: u64, expected: &Identity) -> Result<()> {
    ensure!(
        wait == WAIT_TIMEOUT && expected.started == birth.to_string(),
        "Retained Game Rules game ended or birth identity changed"
    );
    Ok(())
}
fn handle_matches(handle: HANDLE, expected: &Identity) -> Result<()> {
    let (mut c, mut e, mut k, mut u) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    unsafe {
        GetProcessTimes(handle, &mut c, &mut e, &mut k, &mut u)?;
    }
    let birth = (u64::from(c.dwHighDateTime) << 32) | u64::from(c.dwLowDateTime);
    // Exit time is undefined for a live process. Check the SAME retained,
    // waitable kernel handle, not its PID or unspecified FILETIME output.
    live_birth(unsafe { WaitForSingleObject(handle, 0) }, birth, expected)?;
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
        "Retained Game Rules image path changed"
    );
    live_birth(unsafe { WaitForSingleObject(handle, 0) }, birth, expected)
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
fn owner_region(r: Region, owner: u64) -> Result<Region> {
    ensure!(
        owner >= 0x10000
            && owner % 8 == 0
            && owner
                .checked_add(OWNER_BYTES as u64)
                .is_some_and(|v| v <= 0x7fff_ffff_ffff)
            && r.state == MEM_COMMIT.0
            && r.kind == MEM_PRIVATE.0
            && r.protection == PAGE_READWRITE.0
            && r.allocation >= 0x10000
            && r.allocation % 4096 == 0
            && r.allocation <= r.base
            && contains(r, owner, OWNER_BYTES),
        "Whole Game Rules owner is not contained in committed private read/write data"
    );
    Ok(r)
}
fn readable(r: Region, base: u64, at: u64, len: usize) -> bool {
    r.state == MEM_COMMIT.0
        && contains(r, at, len)
        && ((r.kind == MEM_IMAGE.0
            && r.allocation == base
            && matches!(r.protection, 2 | 4 | 8 | 32))
            || (r.kind == MEM_PRIVATE.0 && r.protection == PAGE_READWRITE.0))
}
pub(crate) trait RulesIo {
    fn capture(&self) -> Result<Snapshot>;
    /// Private implementation surface. RulesAccessV2 checks a durable receipt
    /// first; no raw address or variable-width write can reach this method.
    fn change(
        &self,
        expected: &Snapshot,
        index: usize,
        value: u32,
        durable: &dyn Fn() -> Result<()>,
    ) -> Result<()>;
}
pub(crate) struct NativeIo {
    reader: Reader,
    query: Handle,
    binding: Binding,
}
impl NativeIo {
    fn open(binding: &Binding) -> Result<Self> {
        guard(binding)?;
        // Fixed first-module path and protected-module checks, with the
        // independent observer build profile, never validate_build's legacy gate.
        let reader = Reader::open_observer(&binding.game, &binding.game_root)?;
        let query = Handle(unsafe { OpenProcess(QUERY_RIGHTS, false, binding.game.pid)? });
        handle_matches(query.0, &binding.game)?;
        let io = Self {
            reader,
            query,
            binding: binding.clone(),
        };
        io.check()?;
        rules_v2::verify_profile(&io)?;
        io.check()?;
        Ok(io)
    }
    fn check(&self) -> Result<()> {
        guard(&self.binding)?;
        handle_matches(self.query.0, &self.binding.game)?;
        let fresh = Reader::open_observer(&self.binding.game, &self.binding.game_root)?;
        ensure!(
            fresh.base() == self.base(),
            "Game Rules image module base changed"
        );
        Ok(())
    }
    fn query(&self, at: u64) -> Result<Region> {
        self.checkpoint()?;
        ensure!(
            (0x10000..=0x7fff_ffff_ffff).contains(&at),
            "Invalid Game Rules query address"
        );
        let mut r = MEMORY_BASIC_INFORMATION::default();
        let n = unsafe {
            VirtualQueryEx(
                self.query.0,
                Some(at as _),
                &mut r,
                size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        ensure!(
            n == size_of::<MEMORY_BASIC_INFORMATION>(),
            "Game Rules VirtualQueryEx failed"
        );
        Ok(Region {
            base: r.BaseAddress as u64,
            allocation: r.AllocationBase as u64,
            len: r.RegionSize as u64,
            state: r.State.0,
            kind: r.Type.0,
            protection: r.Protect.0,
        })
    }
    fn owner(&self, owner: u64) -> Result<Region> {
        owner_region(self.query(owner)?, owner)
    }
}
impl Memory for NativeIo {
    fn base(&self) -> u64 {
        self.reader.base()
    }
    fn checkpoint(&self) -> Result<()> {
        handle_matches(self.query.0, &self.binding.game)
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        ensure!(
            len > 0
                && len <= 4096
                && at >= 0x10000
                && at
                    .checked_add(len as u64)
                    .is_some_and(|v| v <= 0x7fff_ffff_ffff),
            "Invalid bounded Game Rules read"
        );
        ensure!(
            readable(self.query(at)?, self.base(), at, len),
            "Game Rules read is outside readable image/private data"
        );
        let bytes = self.reader.read(at, len)?;
        ensure!(bytes.len() == len, "Partial Game Rules read");
        self.checkpoint()?;
        Ok(bytes)
    }
}
impl RulesIo for NativeIo {
    fn capture(&self) -> Result<Snapshot> {
        self.check()?;
        let snapshot = rules_v2::capture(self)?;
        self.owner(snapshot.context.owner)?;
        self.check()?;
        Ok(snapshot)
    }
    fn change(
        &self,
        expected: &Snapshot,
        index: usize,
        value: u32,
        durable: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        let field = FIELDS.get(index).context("Unknown Game Rules index")?;
        let plan = rules_v2::plan(expected, field.id, value)?;
        ensure!(
            self.capture()? == *expected,
            "Game Rules changed before writer acquisition"
        );
        let original_region = self.owner(expected.context.owner)?;
        // This is the only VM_WRITE acquisition. The transaction capability has
        // already checked the existing durable rule intent and all 39 values.
        durable()?;
        let writer = Handle(unsafe { OpenProcess(WRITE_RIGHTS, false, self.binding.game.pid)? });
        handle_matches(writer.0, &self.binding.game)?;
        self.check()?;
        rules_v2::verify_profile(self)?;
        ensure!(
            self.capture()? == *expected && self.owner(expected.context.owner)? == original_region,
            "Game Rules owner, allocation, mode or neighbors changed before write"
        );
        // Revalidate identity immediately before the sole fixed 1/4-byte write.
        handle_matches(writer.0, &self.binding.game)?;
        durable()?;
        let mut done = 0;
        unsafe {
            WriteProcessMemory(
                writer.0,
                plan.at as _,
                plan.after.as_ptr() as _,
                plan.after.len(),
                Some(&mut done),
            )?;
        }
        ensure!(
            done == plan.after.len(),
            "Partial rule write; transaction verification required"
        );
        ensure!(
            self.owner(expected.context.owner)? == original_region
                && self.capture()? == plan.expected,
            "Game Rules read-back, owner allocation or unchanged-neighbor verification failed"
        );
        handle_matches(writer.0, &self.binding.game)?;
        self.check()
    }
}

trait Journal {
    fn pending(&self) -> Result<bool>;
    fn save(&self, intent: &Intent) -> Result<()>;
    fn read(&self) -> Result<Value>;
}
struct DiskJournal {
    root: PathBuf,
    _pins: Vec<crate::loader_files::Directories>,
}
impl DiskJournal {
    fn open(root: &Path) -> Result<Self> {
        Ok(Self {
            root: root.into(),
            _pins: vec![
                crate::loader_files::Directories::pin(root)?,
                crate::loader_files::Directories::pin(&root.join("Data"))?,
            ],
        })
    }
    fn exists(&self) -> Result<bool> {
        match std::fs::symlink_metadata(game_rules::path(&self.root)) {
            Ok(meta) => {
                ensure!(
                    meta.is_file() && meta.file_attributes() & 0x400 == 0,
                    "Linked or nonordinary rule journal left untouched"
                );
                Ok(true)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }
}
impl Journal for DiskJournal {
    fn pending(&self) -> Result<bool> {
        // Reject even dangling links before invoking the existing schema's
        // validator (try_exists alone would mistake one for a missing journal).
        let document = if self.exists()? {
            Some(self.read()?)
        } else {
            None
        };
        let pending = game_rules::pending(&self.root)?;
        if let Some(document) = document {
            ensure!(
                document["pending"] == pending && self.read()? == document,
                "Rule journal changed during validation"
            );
        } else {
            ensure!(!self.exists()?, "Rule journal appeared during validation");
        }
        Ok(pending)
    }
    fn save(&self, intent: &Intent) -> Result<()> {
        if self.exists()? {
            self.read()?;
        }
        backend::atomic_json(&game_rules::path(&self.root), intent)
    }
    fn read(&self) -> Result<Value> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .custom_flags(0x00200000)
            .open(game_rules::path(&self.root))?;
        let before = file.metadata()?;
        ensure!(
            before.is_file()
                && before.file_attributes() & 0x400 == 0
                && (1..=16384).contains(&before.len()),
            "Rule journal is not one bounded ordinary file"
        );
        let mut bytes = vec![];
        (&file).take(16385).read_to_end(&mut bytes)?;
        let after = file.metadata()?;
        ensure!(
            bytes.len() as u64 == before.len()
                && after.len() == before.len()
                && after.last_write_time() == before.last_write_time()
                && after.file_attributes() & 0x400 == 0,
            "Rule journal changed or exceeds limits"
        );
        // The existing schema rejects unknown fields; no separate journal format.
        let intent: Intent = serde_json::from_slice(&bytes)?;
        Ok(serde_json::to_value(intent)?)
    }
}
struct Permit {
    original: Snapshot,
    changed: Snapshot,
    index: usize,
    intent: Intent,
    pending_value: Value,
    completion_attempt: Option<Value>,
    forward_attempted: bool,
    rollback_attempted: bool,
}
pub(crate) struct RulesAccessV2<I: RulesIo = NativeIo> {
    io: I,
    binding: Binding,
    journal: Box<dyn Journal>,
    permit: RefCell<Option<Permit>>,
}
impl RulesAccessV2<NativeIo> {
    pub fn open(binding: &Binding) -> Result<Self> {
        Ok(Self {
            io: NativeIo::open(binding)?,
            binding: binding.clone(),
            journal: Box::new(DiskJournal::open(&binding.portable_root)?),
            permit: RefCell::new(None),
        })
    }
}
impl<I: RulesIo> RulesAccessV2<I> {
    pub fn capture(&self) -> Result<Snapshot> {
        self.io.capture()
    }
    /// Call while holding the native session controller and its EXISTING
    /// session_safety::write_gate. Retain its player/team/uniform pending checks.
    /// The one-use Lease must already have been removed from controller state.
    pub fn apply_under_gate(
        &self,
        lease: &Lease,
        token: &str,
        field: &str,
        value: u32,
        _gate: &MutexGuard<'static, ()>,
    ) -> Result<()> {
        lease.validate(token, &self.binding.game, &self.binding.session)?;
        self.apply_inner(lease, field, value)
    }
    fn apply_inner(&self, lease: &Lease, field: &str, after: u32) -> Result<()> {
        ensure!(
            self.permit.borrow().is_none(),
            "A rule transaction is already active"
        );
        ensure!(
            !self.journal.pending()?,
            "Unresolved rule transaction; further writes blocked"
        );
        let plan = rules_v2::plan(&lease.snapshot, field, after)?;
        let before = lease.snapshot.values[plan.index];
        let immutable = json!({"version":1,"session":self.binding.session,"game":self.binding.game,
            "base":lease.snapshot.base,"owner":lease.snapshot.context.owner,"field":field,"before":before,"after":after});
        let result = game_rules::apply(self, lease, field, after, |intent| {
            let value = serde_json::to_value(intent)?;
            let mut keys = value.clone();
            keys.as_object_mut()
                .context("Rule intent is not an object")?
                .remove("pending");
            keys.as_object_mut().unwrap().remove("result");
            ensure!(
                keys == immutable,
                "Rule intent does not match the owned request"
            );
            if intent.pending {
                ensure!(
                    self.permit.borrow().is_none(),
                    "Duplicate rule intent rejected"
                );
                self.journal.save(intent)?;
                ensure!(
                    self.journal.read()? == value,
                    "Durable rule intent read-back differs; no writer authorized"
                );
                *self.permit.borrow_mut() = Some(Permit {
                    original: lease.snapshot.clone(),
                    changed: plan.expected.clone(),
                    index: plan.index,
                    intent: intent.clone(),
                    pending_value: value,
                    completion_attempt: None,
                    forward_attempted: false,
                    rollback_attempted: false,
                });
            } else {
                ensure!(
                    [
                        "applied and verified",
                        "failed transaction; original value verified"
                    ]
                    .contains(&intent.result.as_str()),
                    "Unsupported rule completion result"
                );
                let mut permit = self.permit.borrow_mut();
                let p = permit.as_mut().context("Missing rule receipt")?;
                let document = self.journal.read()?;
                ensure!(
                    document == p.pending_value || p.completion_attempt.as_ref() == Some(&document),
                    "Rule journal ownership changed; completion held"
                );
                p.completion_attempt = Some(value.clone());
                drop(permit);
                self.journal.save(intent)?;
                ensure!(
                    self.journal.read()? == value,
                    "Rule completion read-back differs"
                );
            }
            Ok(())
        });
        // Receipt is never reusable after success OR failure. An unresolved
        // journal continues to block future transactions until safe game exit.
        self.permit.borrow_mut().take();
        result
    }
}
impl<I: RulesIo> game_rules::Access for RulesAccessV2<I> {
    fn capture(&self) -> Result<Snapshot> {
        self.io.capture()
    }
    fn write(&self, expected: &Snapshot, index: usize, value: u32) -> Result<()> {
        ensure!(
            self.io.capture()? == *expected,
            "Game Rules changed immediately before receipt check"
        );
        let mut receipt = self.permit.borrow_mut();
        let p = receipt
            .as_mut()
            .context("No durable rule intent; direct rule writes are forbidden")?;
        ensure!(index == p.index, "Rule receipt does not own this field");
        let rollback = expected == &p.changed && value == p.original.values[index];
        if rollback {
            ensure!(
                p.forward_attempted && !p.rollback_attempted,
                "Rule rollback receipt was not acquired or was consumed"
            );
            let document = self.journal.read()?;
            ensure!(
                document == p.pending_value || p.completion_attempt.as_ref() == Some(&document),
                "Rule journal ownership changed; rollback held"
            );
            // A failed completion save can have published pending=false before
            // its error. Re-arm the SAME existing intent before reverse writes.
            if document != p.pending_value {
                self.journal.save(&p.intent)?;
            }
            ensure!(
                self.journal.read()? == p.pending_value,
                "Rollback intent is not durable"
            );
            p.rollback_attempted = true;
        } else {
            ensure!(
                expected == &p.original
                    && value == p.changed.values[index]
                    && !p.forward_attempted
                    && !p.rollback_attempted,
                "Rule receipt is stale or does not own this value"
            );
            ensure!(
                self.journal.read()? == p.pending_value,
                "Rule intent changed before writer acquisition"
            );
            p.forward_attempted = true;
        }
        // NativeIo repeats the full profile, private span, exact game/module,
        // protected-process and 39-value checks around delayed writer creation.
        self.io.change(expected, index, value, &|| {
            ensure!(
                self.journal.read()? == p.pending_value,
                "Durable rule intent changed during writer checks"
            );
            Ok(())
        })?;
        let mut next = expected.clone();
        next.values[index] = value;
        ensure!(
            self.io.capture()? == next,
            "Rule read-back or unchanged-neighbor verification failed"
        );
        Ok(())
    }
}
/// Preserve every existing UI field/choice/token and correct only the obsolete
/// legacy context classifier. No label, destination ID or styling change.
pub(crate) fn lease_view(lease: &Lease) -> Value {
    let mut view = lease.view();
    view["boundary"] = rules_v2::boundary(&lease.snapshot.context, lease.snapshot.base).into();
    view
}

#[cfg(test)]
#[path = "rules_v2_access_tests.rs"]
mod tests;
