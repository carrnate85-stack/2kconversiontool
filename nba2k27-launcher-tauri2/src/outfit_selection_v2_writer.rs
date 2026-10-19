//! Session-owned outfit writes. A renderer can never supply addresses here.
//! Each allocation/publication consumes a non-cloneable, flushed journal proof.
//! Only fresh private pages, the 12-byte clothing source and one refresh byte
//! can be written. Existing uniform context and executable pages stay untouched.
#![allow(dead_code)]
#![allow(unexpected_cfgs)] // Standalone self-owned-page harness opt-in only.
use super::*;
use crate::{
    blacktop_outfits::{self as outfits, Composed, Page, Player},
    outfit_journal::{Kind, Prepared, Ready, Reservation, Submission, Witness},
};

pub(crate) struct Writer {
    reader: Reader,
    owner: Identity,
    folder: std::path::PathBuf,
}
enum Held {
    Existing(Ready),
    Changed(Submission),
}
pub(crate) struct Receipt {
    held: Held,
    after: Player,
}
impl Receipt {
    pub fn id(&self) -> &str {
        match &self.held {
            Held::Existing(r) => &r.intent().id,
            Held::Changed(s) => &s.intent().id,
        }
    }
    pub fn player(&self) -> &Player {
        &self.after
    }
    fn witness(&self) -> &Witness {
        match &self.held {
            Held::Existing(r) => r.witness(),
            Held::Changed(s) => s.witness(),
        }
    }
}
impl Writer {
    /// Trusted provider launch identity only. Query/read/SYNCHRONIZE, no
    /// VM_WRITE or VM_OPERATION until a durable one-use journal proof is pinned.
    pub fn open(game: &Identity, folder: &Path, owner: &Identity) -> Result<Self> {
        arena_guard(&session_safety::processes()?, game, owner)?;
        ensure!(
            session_safety::identity(owner.pid)? == *owner,
            "Outfit launch owner ended"
        );
        let observed = Reader::open_observer(game, folder)?;
        let handle = Handle(unsafe { OpenProcess(READ, false, game.pid)? });
        retained(handle.0, game)?;
        let reader = Reader {
            handle,
            identity: game.clone(),
            base: observed.base(),
        };
        let value = Self {
            reader,
            owner: owner.clone(),
            folder: folder.to_owned(),
        };
        value.check_owner()?;
        Ok(value)
    }
    fn writer(&self, proof: &Witness) -> Result<Handle> {
        self.gate(proof)?;
        let handle = Handle(unsafe { OpenProcess(WRITE, false, self.reader.identity.pid)? });
        retained(handle.0, &self.reader.identity)?;
        ensure!(
            handle_identity(handle.0, self.reader.identity.pid)? == self.reader.identity,
            "Outfit delayed writer identity changed"
        );
        self.check_owner()?;
        Ok(handle)
    }
    pub fn reader(&self) -> &Reader {
        &self.reader
    }
    fn gate(&self, proof: &Witness) -> Result<()> {
        ensure!(
            *proof.game() == self.reader.identity,
            "Outfit proof belongs to another game"
        );
        self.check_owner()
    }
    pub fn check_owner(&self) -> Result<()> {
        retained(self.reader.handle.0, &self.reader.identity)?;
        arena_guard(
            &session_safety::processes()?,
            &self.reader.identity,
            &self.owner,
        )?;
        ensure!(
            session_safety::identity(self.owner.pid)? == self.owner,
            "Outfit request owner ended"
        );
        let fresh = Reader::open_observer(&self.reader.identity, &self.folder)?;
        ensure!(
            fresh.base() == self.base(),
            "Outfit first-module identity changed"
        );
        crate::outfit_selection_v2::verify_profile(self)?;
        Ok(())
    }
    fn current(&self, p: &Player, pages: &[Page]) -> Result<()> {
        crate::outfit_selection_v2::revalidate(self, &self.reader.identity, p, pages)?;
        ensure!(
            crate::outfit_selection_v2::dirty(self, &p.key)? == 0,
            "Player is still refreshing; no outfit write attempted"
        );
        Ok(())
    }
    pub fn prepare(
        &self,
        r: Reservation,
        p: &Player,
        c: &Composed,
        pages: &[Page],
    ) -> Result<Prepared> {
        let _write = session_safety::write_gate()?;
        let page = {
            let _pin = r.witness().pin()?;
            self.gate(r.witness())?;
            ensure!(
                r.intent().kind == Kind::Compose && r.intent().matches(p, c),
                "Outfit reservation differs from native composition"
            );
            self.current(p, pages)?;
            let writer = self.writer(r.witness())?;
            // The durable reservation budget includes failed allocations. A page
            // is never reused/freed while the game might still reference it.
            let allocation = allocate(writer.0)?;
            let bytes = outfits::page_bytes(
                &self.reader.identity,
                &p.key,
                p.original_pointer,
                allocation,
                c.street_index,
                c.roster_shoes,
                &c.items,
            )?;
            write_private(writer.0, allocation, &bytes)?;
            ensure!(
                self.reader.read(allocation, outfits::PAGE_BYTES)? == bytes,
                "New outfit page readback differs"
            );
            seal(writer.0, allocation)?;
            let page = Page {
                allocation,
                owner: p.key.clone(),
                original_pointer: p.original_pointer,
                street_index: c.street_index,
                roster_shoes: c.roster_shoes,
                items: c.items.clone(),
                sha256: crate::loader_files::digest(&bytes),
            };
            page.verify(
                self,
                &self.reader.identity,
                &p.key,
                &r.intent().target(Some(&page))?,
            )?;
            self.gate(r.witness())?;
            self.current(p, pages)?;
            page
        };
        r.prepared(page)
    }
    /// No source write for an already matching selection. This adapter never
    /// starts a match worker or claims match carry-over.
    pub fn unchanged(&self, r: Ready, p: &Player, c: &Composed, pages: &[Page]) -> Result<Receipt> {
        {
            let _pin = r.witness().pin()?;
            self.gate(r.witness())?;
            ensure!(
                r.intent().kind == Kind::Existing && r.intent().matches(p, c),
                "Unchanged outfit capability differs"
            );
            self.current(p, pages)?;
        }
        Ok(Receipt {
            held: Held::Existing(r),
            after: p.clone(),
        })
    }
    pub fn publish(&self, s: Submission, p: &Player, pages: &[Page]) -> Result<Receipt> {
        let _write = session_safety::write_gate()?;
        let after = {
            let _pin = s.witness().pin()?;
            self.gate(s.witness())?;
            let i = s.intent();
            ensure!(
                i.key == p.key
                    && i.before == p.source
                    && i.original_pointer == p.original_pointer
                    && i.name == p.name
                    && i.pool == p.pool,
                "Outfit source capability differs from captured player"
            );
            self.current(p, pages)?;
            let target = i.target(s.page())?;
            match i.kind {
                Kind::Compose => {
                    let page = s.page().context("Composed outfit has no durable page")?;
                    ensure!(
                        pages.contains(page),
                        "Prepared outfit page is not in the session journal"
                    );
                    page.verify(self, &self.reader.identity, &p.key, &target)?;
                }
                Kind::Restore => ensure!(
                    s.page().is_none()
                        && p.stock
                            .iter()
                            .any(|v| v.pointer == target.pointer && v.names.as_slice() == i.items),
                    "Original stock outfit changed"
                ),
                Kind::Existing => anyhow::bail!("Unchanged outfits must not write their source"),
            }
            ensure!(
                target.uniform == p.source.uniform,
                "Uniform context must stay unchanged"
            );
            let writer = self.writer(s.witness())?;
            let source_at = p.key.description + 0xC8;
            let dirty_at = p.key.controller + 0x30 + p.key.slot as u64 * 0x7b0;
            // Both complete native objects must still occupy ordinary private
            // non-executable RW spans, not merely the two destination bytes.
            private_rw(writer.0, p.key.description, 0xF8)?;
            private_rw(
                writer.0,
                p.key.controller + 0x20 + p.key.slot as u64 * 0x7b0,
                0x18,
            )?;
            private_rw(writer.0, source_at, 12)?;
            private_rw(writer.0, dirty_at, 1)?;
            // Final fresh proof before the first source write. No stale journal
            // recovery path is allowed to construct this Submission capability.
            self.gate(s.witness())?;
            self.current(p, pages)?;
            write_private(writer.0, source_at, &target.bytes())?;
            ensure!(
                crate::outfit_selection_v2::source(self, &p.key)? == target,
                "Outfit source readback differs; stop this request"
            );
            let after = Player {
                source: target,
                items: i.items.clone(),
                ..p.clone()
            };
            self.gate(s.witness())?;
            self.current(&after, pages)?;
            write_private(writer.0, dirty_at, &[1])?;
            // The game may consume the refresh byte immediately (0), or on its
            // next tick (1). Only poll-confirmed zero confirms selection fit.
            crate::outfit_selection_v2::revalidate(self, &self.reader.identity, &after, pages)?;
            ensure!(
                crate::outfit_selection_v2::dirty(self, &p.key)? <= 1,
                "Outfit refresh byte is unsupported"
            );
            retained(self.reader.handle.0, &self.reader.identity)?;
            after
        };
        Ok(Receipt {
            held: Held::Changed(s),
            after,
        })
    }
    /// Read-only, nonblocking refresh confirmation; the owning engine supplies
    /// the timeout and must drop the receipt before closing the journal.
    pub fn consumed(&self, r: &Receipt, pages: &[Page]) -> Result<bool> {
        let _pin = r.witness().pin()?;
        self.gate(r.witness())?;
        crate::outfit_selection_v2::revalidate(self, &self.reader.identity, &r.after, pages)?;
        let dirty = crate::outfit_selection_v2::dirty(self, &r.after.key)?;
        ensure!(dirty <= 1, "Outfit refresh byte is unsupported");
        if dirty != 0 {
            return Ok(false);
        }
        crate::outfit_selection_v2::revalidate(self, &self.reader.identity, &r.after, pages)?;
        Ok(crate::outfit_selection_v2::dirty(self, &r.after.key)? == 0)
    }
}
fn private_rw(handle: HANDLE, at: u64, len: usize) -> Result<()> {
    address(at, len)?;
    let mut i = MEMORY_BASIC_INFORMATION::default();
    let n = unsafe {
        VirtualQueryEx(
            handle,
            Some(at as _),
            &mut i,
            size_of::<MEMORY_BASIC_INFORMATION>(),
        )
    };
    ensure!(
        n == size_of::<MEMORY_BASIC_INFORMATION>()
            && i.State == MEM_COMMIT
            && i.Type == MEM_PRIVATE
            && i.Protect == PAGE_READWRITE
            && at >= i.BaseAddress as u64
            && (i.BaseAddress as u64)
                .checked_add(i.RegionSize as u64)
                .is_some_and(|end| at.checked_add(len as u64).is_some_and(|v| v <= end)),
        "Outfit write is not contained in private non-executable read/write data"
    );
    Ok(())
}
fn write_private(handle: HANDLE, at: u64, bytes: &[u8]) -> Result<()> {
    ensure!(
        [1, 12, outfits::PAGE_BYTES].contains(&bytes.len()),
        "Unsupported outfit write width"
    );
    private_rw(handle, at, bytes.len())?;
    let mut done = 0;
    unsafe {
        WriteProcessMemory(
            handle,
            at as _,
            bytes.as_ptr() as _,
            bytes.len(),
            Some(&mut done),
        )?;
    }
    ensure!(done == bytes.len(), "Partial outfit write; do not retry");
    Ok(())
}
fn allocation(handle: HANDLE, at: u64, protection: PAGE_PROTECTION_FLAGS) -> Result<()> {
    address(at, outfits::PAGE_BYTES)?;
    ensure!(at % 4096 == 0, "Unaligned outfit allocation");
    let mut i = MEMORY_BASIC_INFORMATION::default();
    let n = unsafe {
        VirtualQueryEx(
            handle,
            Some(at as _),
            &mut i,
            size_of::<MEMORY_BASIC_INFORMATION>(),
        )
    };
    ensure!(
        n == size_of::<MEMORY_BASIC_INFORMATION>()
            && i.BaseAddress as u64 == at
            && i.AllocationBase as u64 == at
            && i.RegionSize == 4096
            && i.Type == MEM_PRIVATE
            && i.State == MEM_COMMIT
            && i.Protect == protection,
        "Outfit page allocation changed"
    );
    Ok(())
}
fn allocate(handle: HANDLE) -> Result<u64> {
    let at = unsafe {
        VirtualAllocEx(
            handle,
            None,
            outfits::PAGE_BYTES,
            MEM_RESERVE | MEM_COMMIT,
            PAGE_READWRITE,
        )
    };
    ensure!(
        !at.is_null(),
        "Cannot reserve a private outfit page: {}",
        windows::core::Error::from_thread()
    );
    allocation(handle, at as u64, PAGE_READWRITE)?;
    Ok(at as u64)
}
fn seal(handle: HANDLE, at: u64) -> Result<()> {
    allocation(handle, at, PAGE_READWRITE)?;
    let mut old = PAGE_PROTECTION_FLAGS(0);
    unsafe {
        VirtualProtectEx(
            handle,
            at as _,
            outfits::PAGE_BYTES,
            PAGE_READONLY,
            &mut old,
        )?;
    }
    ensure!(
        old == PAGE_READWRITE,
        "New outfit page had unexpected protection"
    );
    allocation(handle, at, PAGE_READONLY)
}

// This module must be a child of blacktop_native (same as the legacy writer),
// but does not call open_roster, validate_build, or the legacy match worker.
const READ: PROCESS_ACCESS_RIGHTS =
    PROCESS_ACCESS_RIGHTS(PROCESS_QUERY_INFORMATION.0 | PROCESS_VM_READ.0 | PROCESS_SYNCHRONIZE.0);
const WRITE: PROCESS_ACCESS_RIGHTS =
    PROCESS_ACCESS_RIGHTS(READ.0 | PROCESS_VM_WRITE.0 | PROCESS_VM_OPERATION.0);
fn retained(h: HANDLE, game: &Identity) -> Result<()> {
    ensure!(
        unsafe { WaitForSingleObject(h, 0) } == WAIT_TIMEOUT,
        "Retained outfit game ended or wait state is unknown"
    );
    ensure!(
        handle_identity(h, game.pid)? == *game,
        "Retained outfit game identity changed"
    );
    Ok(())
}
impl Memory for Writer {
    fn base(&self) -> u64 {
        self.reader.base()
    }
    fn checkpoint(&self) -> Result<()> {
        retained(self.reader.handle.0, &self.reader.identity)
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        address(at, len)?;
        // One retained-identity proof per read (region() used to repeat it).
        self.checkpoint()?;
        let r = self.region_checked(at)?;
        ensure!(
            r.committed
                && at >= r.start
                && at
                    .checked_add(len as u64)
                    .zip(r.start.checked_add(r.len))
                    .is_some_and(|(a, b)| a <= b)
                && ((r.kind == MEM_IMAGE.0
                    && r.allocation == self.base()
                    && matches!(r.protection, 2 | 4 | 8 | 32))
                    || (r.kind == MEM_PRIVATE.0 && matches!(r.protection, 2 | 4))),
            "Outfit read escaped verified image or non-executable private data"
        );
        let bytes = self.reader.read(at, len)?;
        ensure!(bytes.len() == len, "Partial Version 2 outfit read");
        self.checkpoint()?;
        Ok(bytes)
    }
}
impl crate::blacktop_arena::Inspect for Writer {
    fn region(&self, at: u64) -> Result<crate::blacktop_arena::Region> {
        address(at, 1)?;
        self.checkpoint()?;
        self.region_checked(at)
    }
}
impl Writer {
    /// VirtualQueryEx only; the caller has just proven the retained identity.
    fn region_checked(&self, at: u64) -> Result<crate::blacktop_arena::Region> {
        let mut i = MEMORY_BASIC_INFORMATION::default();
        ensure!(
            unsafe {
                VirtualQueryEx(
                    self.reader.handle.0,
                    Some(at as _),
                    &mut i,
                    size_of::<MEMORY_BASIC_INFORMATION>(),
                )
            } == size_of::<MEMORY_BASIC_INFORMATION>(),
            "Outfit VirtualQueryEx failed"
        );
        Ok(crate::blacktop_arena::Region {
            start: i.BaseAddress as u64,
            len: i.RegionSize as u64,
            allocation: i.AllocationBase as u64,
            kind: i.Type.0,
            protection: i.Protect.0,
            committed: i.State == MEM_COMMIT,
        })
    }
}
impl crate::outfit_selection_session::Driver for Writer {
    type Receipt = Receipt;
    fn identity(&self) -> &Identity {
        &self.reader.identity
    }
    fn capture(&self, pages: &[Page]) -> Result<outfits::Selection> {
        self.check_owner()?;
        crate::outfit_selection_v2::capture(self, &self.reader.identity, pages)
    }
    fn prepare(
        &self,
        r: Reservation,
        p: &Player,
        c: &Composed,
        pages: &[Page],
    ) -> Result<Prepared> {
        Writer::prepare(self, r, p, c, pages)
    }
    fn unchanged(&self, r: Ready, p: &Player, c: &Composed, pages: &[Page]) -> Result<Receipt> {
        Writer::unchanged(self, r, p, c, pages)
    }
    fn publish(&self, s: Submission, p: &Player, pages: &[Page]) -> Result<Receipt> {
        Writer::publish(self, s, p, pages)
    }
    fn consumed(&self, r: &Receipt, pages: &[Page]) -> Result<bool> {
        Writer::consumed(self, r, pages)
    }
}

#[cfg(test)]
mod read_cost_probe {
    use super::*;
    #[test]
    #[ignore = "explicit current-game READ-ONLY selection timing; no writer handles or game calls"]
    fn selected_players_guarded_read_costs_live_read_only() {
        let folder =
            std::path::PathBuf::from(std::env::var_os("C27_LIVE_GAME_DIR").expect("game folder"));
        let rows = session_safety::processes().unwrap();
        let id = rows
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe"))
            .and_then(|p| p.identity.clone())
            .expect("one current game");
        assert_eq!(
            rows.iter()
                .filter(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe"))
                .count(),
            1
        );
        // This probe exercises the exact per-read identity/region checks using
        // only an observer handle. It does NOT create a write session, journal,
        // admission, allocation or command, nor bypass any production guard.
        let observed = Reader::open_observer(&id, &folder).unwrap();
        // Match the production read-only handle, including SYNCHRONIZE for
        // retained identity checks and QUERY_INFORMATION for VirtualQueryEx.
        let handle = Handle(unsafe { OpenProcess(READ, false, id.pid).unwrap() });
        retained(handle.0, &id).unwrap();
        let reader = Reader {
            handle,
            identity: id.clone(),
            base: observed.base(),
        };
        let value = Writer {
            reader,
            owner: id.clone(),
            folder,
        };
        for n in 0..3 {
            let started = std::time::Instant::now();
            let selection = crate::outfit_selection_v2::capture(&value, &id, &[]).unwrap();
            eprintln!(
                "guarded-capture #{n}: {:?} players={}",
                started.elapsed(),
                selection.players.len()
            );
            let started = std::time::Instant::now();
            let catalog = crate::outfits::catalog().unwrap();
            eprintln!("catalog-open #{n}: {:?}", started.elapsed());
            let started = std::time::Instant::now();
            for p in &selection.players {
                let result = p
                    .pool
                    .as_deref()
                    .and_then(|pool| catalog.worn_outfit(&p.items, pool));
                eprintln!(
                    "editable-draft slot={} available={}",
                    p.key.slot,
                    result.is_some()
                );
            }
            eprintln!("drafts #{n}: {:?}", started.elapsed());
        }
    }
}

// Explicit standalone-harness opt-in only. Uses this test process and its own
// fresh pages; this function does not exist in the production launcher.
#[cfg(outfit_offline_native_tests)]
pub(crate) fn offline_native_pages() -> Result<()> {
    struct Own {
        handle: Handle,
        at: u64,
    }
    impl Drop for Own {
        fn drop(&mut self) {
            unsafe {
                let _ = VirtualFreeEx(self.handle.0, self.at as _, 0, MEM_RELEASE);
            }
        }
    }
    fn bytes(o: &Own) -> Result<Vec<u8>> {
        let mut b = vec![0; 4096];
        let mut done = 0;
        unsafe {
            ReadProcessMemory(
                o.handle.0,
                o.at as _,
                b.as_mut_ptr() as _,
                b.len(),
                Some(&mut done),
            )?;
        }
        ensure!(done == 4096, "Own page readback");
        Ok(b)
    }
    ensure!(
        READ.0 & (PROCESS_VM_WRITE.0 | PROCESS_VM_OPERATION.0) == 0
            && READ.0 & PROCESS_SYNCHRONIZE.0 != 0,
        "Outfit query rights broadened"
    );
    let handle = Handle(unsafe { OpenProcess(WRITE, false, std::process::id())? });
    let at = allocate(handle.0)?;
    let own = Own { handle, at };
    let mut expected = vec![0x5A; 4096];
    write_private(own.handle.0, at, &expected)?;
    write_private(own.handle.0, at + 128, &[7; 12])?;
    expected[128..140].fill(7);
    write_private(own.handle.0, at + 512, &[1])?;
    expected[512] = 1;
    ensure!(
        bytes(&own)? == expected,
        "Native fixed writes changed adjacent bytes"
    );
    ensure!(
        write_private(own.handle.0, at, &[0; 2]).is_err(),
        "Unsupported native width"
    );
    ensure!(
        write_private(own.handle.0, at + 4090, &[0; 12]).is_err(),
        "Native span escaped allocation"
    );
    seal(own.handle.0, at)?;
    allocation(own.handle.0, at, PAGE_READONLY)?;
    ensure!(
        write_private(own.handle.0, at, &[1]).is_err() && seal(own.handle.0, at).is_err(),
        "Immutable page was writable/resealed"
    );
    ensure!(bytes(&own)? == expected, "Sealing changed bytes");
    println!(
        "PASS actual Win32 self-owned pages: fixed widths/neighbors/bounds/sealing/query rights"
    );
    Ok(())
}
