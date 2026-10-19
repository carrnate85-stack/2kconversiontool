//! Session-owned outfit writes. A renderer can never supply addresses here.
//! Each allocation/publication consumes a non-cloneable, flushed journal proof.
//! Only fresh private pages, the 12-byte clothing source and one refresh byte
//! can be written. Existing uniform context and executable pages stay untouched.
#![allow(dead_code)]
use super::*;
use crate::{
    blacktop_outfits::{self as outfits, Composed, Page, Player},
    outfit_journal::{Kind, Prepared, Ready, Reservation, Submission, Witness},
};

pub(crate) struct Writer {
    reader: Reader,
    owner: Identity,
    writable: bool,
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
    pub fn open(game: &Identity, folder: &Path, owner: &Identity) -> Result<Self> {
        arena_guard(&session_safety::processes()?, game, owner)?;
        ensure!(
            session_safety::identity(owner.pid)? == *owner,
            "Outfit request owner ended"
        );
        let reader = Reader::open_access(
            game,
            folder,
            PROCESS_QUERY_INFORMATION | PROCESS_VM_READ | PROCESS_VM_WRITE | PROCESS_VM_OPERATION,
        )?;
        Ok(Self {
            reader,
            owner: owner.clone(),
            writable: true,
        })
    }
    pub fn open_readonly(game: &Identity, folder: &Path, owner: &Identity) -> Result<Self> {
        arena_guard(&session_safety::processes()?, game, owner)?;
        ensure!(
            session_safety::identity(owner.pid)? == *owner,
            "Outfit request owner ended"
        );
        let reader = Reader::open_roster(game, folder)?;
        Ok(Self {
            reader,
            owner: owner.clone(),
            writable: false,
        })
    }
    pub fn allow_writes(&mut self, folder: &Path) -> Result<()> {
        self.check_owner()?;
        if !self.writable {
            self.reader = Reader::open_access(
                &self.reader.identity,
                folder,
                PROCESS_QUERY_INFORMATION
                    | PROCESS_VM_READ
                    | PROCESS_VM_WRITE
                    | PROCESS_VM_OPERATION,
            )?;
            self.writable = true;
        }
        Ok(())
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
        self.reader.check()?;
        arena_guard(
            &session_safety::processes()?,
            &self.reader.identity,
            &self.owner,
        )?;
        ensure!(
            session_safety::identity(self.owner.pid)? == self.owner,
            "Outfit request owner ended"
        );
        validate_build(&self.reader)?;
        Ok(())
    }
    fn current(&self, p: &Player, pages: &[Page]) -> Result<()> {
        outfits::revalidate(&self.reader, &self.reader.identity, p, pages)?;
        ensure!(
            outfits::dirty(&self.reader, &p.key)? == 0,
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
        ensure!(
            self.writable,
            "Outfit writes have not been explicitly enabled for this operation"
        );
        let _write = session_safety::write_gate()?;
        let page = {
            let _pin = r.witness().pin()?;
            self.gate(r.witness())?;
            ensure!(
                r.intent().kind == Kind::Compose && r.intent().matches(p, c),
                "Outfit reservation differs from native composition"
            );
            self.current(p, pages)?;
            // The durable reservation budget includes failed allocations. A page
            // is never reused/freed while the game might still reference it.
            let allocation = allocate(self.reader.handle.0)?;
            let bytes = outfits::page_bytes(
                &self.reader.identity,
                &p.key,
                p.original_pointer,
                allocation,
                c.street_index,
                c.roster_shoes,
                &c.items,
            )?;
            write_private(self.reader.handle.0, allocation, &bytes)?;
            ensure!(
                self.reader.read(allocation, outfits::PAGE_BYTES)? == bytes,
                "New outfit page readback differs"
            );
            seal(self.reader.handle.0, allocation)?;
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
                &self.reader,
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
    /// No source write for an already matching selection, but revalidate it
    /// before recording a fresh match carry-over choice.
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
        ensure!(
            self.writable,
            "Outfit writes have not been explicitly enabled for this operation"
        );
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
                    page.verify(&self.reader, &self.reader.identity, &p.key, &target)?;
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
            let source_at = p.key.description + 0xC8;
            let dirty_at = p.key.controller + 0x30 + p.key.slot as u64 * 0x7b0;
            private_rw(self.reader.handle.0, source_at, 12)?;
            private_rw(self.reader.handle.0, dirty_at, 1)?;
            // Final fresh proof before the first source write. No stale journal
            // recovery path is allowed to construct this Submission capability.
            self.gate(s.witness())?;
            self.current(p, pages)?;
            write_private(self.reader.handle.0, source_at, &target.bytes())?;
            ensure!(
                outfits::source(&self.reader, &p.key)? == target,
                "Outfit source readback differs; stop this request"
            );
            let after = Player {
                source: target,
                items: i.items.clone(),
                ..p.clone()
            };
            self.gate(s.witness())?;
            self.current(&after, pages)?;
            write_private(self.reader.handle.0, dirty_at, &[1])?;
            // The game may consume the refresh byte immediately (0), or on its
            // next tick (1). Only poll-confirmed zero permits a match choice.
            outfits::revalidate(&self.reader, &self.reader.identity, &after, pages)?;
            ensure!(
                outfits::dirty(&self.reader, &p.key)? <= 1,
                "Outfit refresh byte is unsupported"
            );
            self.reader.check()?;
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
        outfits::revalidate(&self.reader, &self.reader.identity, &r.after, pages)?;
        let dirty = outfits::dirty(&self.reader, &r.after.key)?;
        ensure!(dirty <= 1, "Outfit refresh byte is unsupported");
        if dirty != 0 {
            return Ok(false);
        }
        outfits::revalidate(&self.reader, &self.reader.identity, &r.after, pages)?;
        Ok(outfits::dirty(&self.reader, &r.after.key)? == 0)
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn readonly_player_scan_capability_refuses_allocation_and_source_publication_before_native_access()
     {
        use crate::blacktop_outfits::tests::{Fixture, game};
        let mut f = Fixture::selection();
        let g = game();
        let p = outfits::capture(&f, &g, &[]).unwrap().players.remove(0);
        let c = crate::outfits::catalog().unwrap();
        let composed = c
            .presets
            .iter()
            .filter(|v| v.pool == p.pool && v.separable && !v.manual_only)
            .find_map(|v| {
                outfits::compose(&p, &c.original_outfit(&v.id, "Readonly test").ok()?).ok()
            })
            .unwrap();
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("artifacts")
            .join(format!("outfit-readonly-{}", crate::backend::unique_id()));
        std::fs::create_dir_all(root.join("Data")).unwrap();
        let mut store =
            crate::outfit_journal::Store::create(&root, &crate::backend::unique_id(), &g).unwrap();
        // A deliberately non-game Reader backed only by this test process with
        // no VM_WRITE/VM_OPERATION rights. Reject before any game/build reads.
        let handle = Handle(unsafe {
            OpenProcess(
                PROCESS_QUERY_INFORMATION | PROCESS_VM_READ,
                false,
                std::process::id(),
            )
            .unwrap()
        });
        let writer = Writer {
            reader: Reader {
                handle,
                identity: g.clone(),
                base: 0,
            },
            owner: g.clone(),
            writable: false,
        };
        let reservation = store.reserve(&p, &composed).unwrap();
        assert!(
            writer
                .prepare(reservation, &p, &composed, &[])
                .err()
                .unwrap()
                .to_string()
                .contains("explicitly enabled")
        );
        let r = store.reserve(&p, &composed).unwrap();
        let page = f.private(&g, &p, &composed);
        let prepared = r.prepared(page).unwrap();
        let ready = store.prepared(prepared).unwrap();
        let submitted = store.attempting(ready).unwrap();
        assert!(
            writer
                .publish(submitted, &p, store.pages())
                .err()
                .unwrap()
                .to_string()
                .contains("explicitly enabled")
        );
        assert!(store.ledger().choices.is_empty());
        store.finish(|| Ok(())).unwrap();
    }
    fn own() -> Handle {
        Handle(unsafe {
            OpenProcess(
                PROCESS_QUERY_INFORMATION
                    | PROCESS_VM_READ
                    | PROCESS_VM_WRITE
                    | PROCESS_VM_OPERATION,
                false,
                std::process::id(),
            )
            .unwrap()
        })
    }
    struct Owned {
        handle: Handle,
        at: u64,
    }
    impl Owned {
        fn new() -> Self {
            let handle = own();
            let at = allocate(handle.0).unwrap();
            Self { handle, at }
        }
    }
    impl Drop for Owned {
        fn drop(&mut self) {
            unsafe {
                let _ = VirtualFreeEx(self.handle.0, self.at as _, 0, MEM_RELEASE);
            }
        }
    }
    fn read(o: &Owned, at: u64, n: usize) -> Vec<u8> {
        let mut b = vec![0; n];
        let mut done = 0;
        unsafe {
            ReadProcessMemory(o.handle.0, at as _, b.as_mut_ptr() as _, n, Some(&mut done))
                .unwrap();
        }
        assert_eq!(done, n);
        b
    }
    #[test]
    fn own_process_page_is_exact_readonly_and_full_bytes_match() {
        // Only this unit test's process/allocation; never open NBA 2K27.
        let o = Owned::new();
        let bytes = (0..4096).map(|i| (i % 251) as u8).collect::<Vec<_>>();
        write_private(o.handle.0, o.at, &bytes).unwrap();
        assert_eq!(read(&o, o.at, 4096), bytes);
        seal(o.handle.0, o.at).unwrap();
        allocation(o.handle.0, o.at, PAGE_READONLY).unwrap();
        assert!(write_private(o.handle.0, o.at, &[1]).is_err());
        assert!(seal(o.handle.0, o.at).is_err());
        assert!(allocation(o.handle.0, o.at + 8, PAGE_READONLY).is_err());
        assert_eq!(read(&o, o.at, 4096), bytes);
    }
    #[test]
    fn fixed_source_and_refresh_writes_preserve_every_adjacent_byte() {
        let o = Owned::new();
        write_private(o.handle.0, o.at, &vec![0x5a; 4096]).unwrap();
        let target = outfits::Source {
            pointer: 0x123456780,
            count: 5,
            uniform: 0x444444440,
        };
        write_private(o.handle.0, o.at + 0xc8, &target.bytes()).unwrap();
        write_private(o.handle.0, o.at + 0x30, &[1]).unwrap();
        let data = read(&o, o.at, 4096);
        let mut expected = vec![0x5a; 4096];
        expected[0xc8..0xd4].copy_from_slice(&target.bytes());
        expected[0x30] = 1;
        assert_eq!(data, expected);
        assert_eq!(&data[0xf0..0xf8], &[0x5a; 8]);
        for n in [0, 2, 4, 8, 13, 4095, 4097] {
            assert!(write_private(o.handle.0, o.at, &vec![0; n]).is_err());
        }
        assert!(write_private(o.handle.0, o.at + 4090, &[0; 12]).is_err());
        for protection in [
            PAGE_READONLY,
            PAGE_NOACCESS,
            PAGE_EXECUTE_READ,
            PAGE_EXECUTE_READWRITE,
        ] {
            let mut old = PAGE_PROTECTION_FLAGS(0);
            unsafe {
                VirtualProtectEx(o.handle.0, o.at as _, 4096, protection, &mut old).unwrap();
            }
            assert!(write_private(o.handle.0, o.at, &[1]).is_err());
        }
    }
}
