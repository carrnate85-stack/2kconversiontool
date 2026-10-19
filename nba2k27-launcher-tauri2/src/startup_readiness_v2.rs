//! Exact pinned V2 stock-manager startup observer. Reads only; no game calls.
//! The same nonnull manager, exact native vtable and stock default device must
//! remain stable for one second. Any profile/region/read doubt retains the old
//! 20-second maximum. Current native activation still independently checks
//! ownership, anti-cheat absence and stock default before registration.
//! V2 +1E0 mapping is independently confirmed in the exact-owned native report:
//! Dev/Portable/Logs/game-session/nba2k27_version_proxy.log (PID34220),
//! MANAGER_READY=PASS, DEFAULT_BEFORE=0xC7E442DC. This confirms the field;
//! fast-path timing remains a dev validation item, not a measured speed claim.
use crate::blacktop_arena::Inspect;
use crate::blacktop_native::Memory;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
    time::Duration,
};

pub(crate) const STAMP: u32 = 0x6AC5_3765;
pub(crate) const IMAGE_SIZE: u32 = 0x35BD_E000;
pub(crate) const GETTER: u64 = 0x04B9_C0A0;
pub(crate) const GETTER_BYTES: [u8; 16] = [
    0x55, 0x56, 0x48, 0x83, 0xEC, 0x58, 0x48, 0x8D, 0x6C, 0x24, 0x50, 0x48, 0xC7, 0x45, 0x00, 0xFE,
];
pub(crate) const ANCHOR: u64 = 0x04B9_C0B3;
/// mov rax,[rip+0x071E0CCE]; test rax,rax; jz +7
pub(crate) const ANCHOR_BYTES: [u8; 12] = [
    0x48, 0x8B, 0x05, 0xCE, 0x0C, 0x1E, 0x07, 0x48, 0x85, 0xC0, 0x74, 0x07,
];
pub(crate) const MANAGER: u64 = 0x0BD7_CD88;
pub(crate) const DEFAULT_DEVICE_OFFSET: u64 = 0x1E0;
pub(crate) const STOCK_DEFAULT_DEVICE: u32 = 0xC7E4_42DC;
pub(crate) const STABLE: Duration = Duration::from_millis(1000);

/// The anchor's RIP-relative load must resolve exactly to MANAGER.
fn anchor_target(code: &[u8]) -> Result<u64> {
    ensure!(
        code.len() >= 7 && code[..3] == [0x48, 0x8B, 0x05],
        "Readiness anchor is not a RIP-relative load"
    );
    let disp = i32::from_le_bytes(code[3..7].try_into()?) as i64;
    u64::try_from(ANCHOR as i64 + 7 + disp).context("Readiness anchor target overflow")
}

fn verify_code(getter: &[u8], anchor: &[u8]) -> Result<()> {
    ensure!(
        getter == GETTER_BYTES,
        "File-manager getter differs on this image"
    );
    ensure!(
        anchor == ANCHOR_BYTES,
        "File-manager readiness anchor differs on this image"
    );
    ensure!(
        anchor_target(anchor)? == MANAGER,
        "File-manager anchor does not resolve to the proven singleton"
    );
    Ok(())
}

struct Section {
    rva: u64,
    virtual_size: u64,
    raw: u64,
    raw_size: u64,
    flags: u32,
}
fn sections(header: &[u8]) -> Result<(u32, u32, Vec<Section>)> {
    ensure!(
        header.len() >= 1024 && header[..2] == *b"MZ",
        "Invalid executable header"
    );
    let pe = u32::from_le_bytes(header[60..64].try_into()?) as usize;
    ensure!(
        (64..=2048).contains(&pe) && header[pe..pe + 4] == *b"PE\0\0",
        "Invalid PE header"
    );
    let count = u16::from_le_bytes(header[pe + 6..pe + 8].try_into()?) as usize;
    let optional = u16::from_le_bytes(header[pe + 20..pe + 22].try_into()?) as usize;
    let stamp = u32::from_le_bytes(header[pe + 8..pe + 12].try_into()?);
    let size = u32::from_le_bytes(header[pe + 80..pe + 84].try_into()?);
    let table = pe + 24 + optional;
    ensure!(
        count <= 96 && table + count * 40 <= header.len(),
        "PE section table exceeds bounds"
    );
    let rows = (0..count)
        .map(|i| {
            let r = &header[table + i * 40..table + (i + 1) * 40];
            let d = |o: usize| u32::from_le_bytes(r[o..o + 4].try_into().unwrap()) as u64;
            Section {
                virtual_size: d(8),
                rva: d(12),
                raw_size: d(16),
                raw: d(20),
                flags: d(36) as u32,
            }
        })
        .collect();
    Ok((stamp, size, rows))
}
fn file_bytes(file: &mut File, rows: &[Section], rva: u64, len: usize) -> Result<Vec<u8>> {
    let s = rows
        .iter()
        .find(|s| rva >= s.rva && rva + len as u64 <= s.rva + s.raw_size.min(s.virtual_size))
        .context("RVA is not file-backed")?;
    let mut bytes = vec![0; len];
    file.seek(SeekFrom::Start(s.raw + (rva - s.rva)))?;
    file.read_exact(&mut bytes)?;
    Ok(bytes)
}

#[derive(Clone, Copy)]
struct Method {
    rva: u64,
    bytes: usize,
    hash: &'static str,
}
const METHODS: [Method; 11] = [
    Method {
        rva: GETTER,
        bytes: 258,
        hash: "D947A04A10E667AD452DE651CE0F634F7718FED56C507F9B56CF50D2412B326D",
    },
    Method {
        rva: 0x4B9C210,
        bytes: 206,
        hash: "25B3A3A6D1F64B07B8973B3C3D2A8BDCD8A8A0A0E56242FE5C28B57516ED1B77",
    },
    Method {
        rva: 0x4B9D120,
        bytes: 388,
        hash: "CB820A756B799EECF49F48DA552FF7158CD763A15D2517130E836D12B312DD82",
    },
    Method {
        rva: 0x4B9CE40,
        bytes: 148,
        hash: "25DA31F1E4BBAFE7B410A23C767B2308DFA13893B3B3218AE0B7CE80E601C1CE",
    },
    Method {
        rva: 0x4d6e500,
        bytes: 306,
        hash: "3BD2A2E6C9F343E1573E310C3DE50F277AC88E8A63B720DAA731877E5C84BAA3",
    },
    Method {
        rva: 0x4b75fa0,
        bytes: 197,
        hash: "4C0597E56C15618F95ED43722FF76EAAEBFBFC3C61584331F56DD204462D36D6",
    },
    Method {
        rva: 0x4bc03c0,
        bytes: 182,
        hash: "5A6FF87C591E290ED8114A9D36CB2AABC64EC50586CCEBE7F42F017C2F8380CD",
    },
    Method {
        rva: 0x4cc0c30,
        bytes: 1790,
        hash: "E4C780F2CC3B21B5AD78050AD917B171CB5E40E7F1F8F8829EFBBE0C04BEC520",
    },
    Method {
        rva: 0x4cc2720,
        bytes: 214,
        hash: "395DF74AB4B9847F0043A9371B789FDBF6E35A0898AB0F07D6C7377ACA9F63A3",
    },
    Method {
        rva: 0x4bbf830,
        bytes: 1558,
        hash: "4F8093C3A8412540D931EFDF33A171D14233B67AA3DBCE0DE809045CF7B1E615",
    },
    Method {
        rva: 0x4bbcf20,
        bytes: 1380,
        hash: "FE8A743E17B307F063BBC063B662A5E6B6B8F8F7436FFBCA518A328465022438",
    },
];
const VTABLE: u64 = 0x7F9D8A8;
const MANAGER_BYTES: usize = 0x1F8;
// Complete allocator getter initializes this exact native .bss arena. The
// manager's offset within it is dynamic; never adopt the observed +50 as fixed.
const ARENA_START: u64 = 0xBDC7A00;
const ARENA_BYTES: u64 = 0x40000;
const ALLOCATOR_VTABLE: u64 = 0x7F9FFF8;
const ALLOCATOR_VIRTUALS: [(u64, u64); 4] = [
    (0x200, 0x4CC0C30),
    (0x228, 0x4CC2720),
    (0x98, 0x4BBCF20),
    (0xB8, 0x4BBF830),
];
const MANAGER_VIRTUALS: [u64; 4] = [0x4B9C3A0, 0x4B9C400, 0x2459F0, 0x4B9C410];
fn digest(bytes: &[u8]) -> String {
    format!("{:X}", Sha256::digest(bytes))
}
fn validate_methods(mut read: impl FnMut(u64, usize) -> Result<Vec<u8>>) -> Result<()> {
    for method in METHODS {
        let bytes = read(method.rva, method.bytes)?;
        ensure!(
            bytes.len() == method.bytes && digest(&bytes) == method.hash,
            "Complete readiness method differs at {:#X}",
            method.rva
        );
    }
    Ok(())
}

/// Read-only disk proof of the exact V2 image (no process access).
pub(crate) fn disk_proof(exe: &Path) -> Result<()> {
    let mut file = File::open(exe)?;
    let mut header = vec![0; 4096];
    file.read_exact(&mut header)?;
    let (stamp, size, rows) = sections(&header)?;
    ensure!(
        (stamp, size) == (STAMP, IMAGE_SIZE),
        "Startup readiness is mapped only for the verified Version 2 image"
    );
    let getter = file_bytes(&mut file, &rows, GETTER, GETTER_BYTES.len())?;
    let anchor = file_bytes(&mut file, &rows, ANCHOR, ANCHOR_BYTES.len())?;
    verify_code(&getter, &anchor)?;
    validate_methods(|rva, bytes| file_bytes(&mut file, &rows, rva, bytes))?;
    ensure!(
        rows.iter().any(|s| ARENA_START >= s.rva
            && ARENA_START + ARENA_BYTES <= s.rva + s.virtual_size
            && s.flags & 0x80000000 != 0
            && s.flags & 0x20000000 == 0),
        "Native readiness arena is not writable image data"
    );
    for (slot, target) in ALLOCATOR_VIRTUALS {
        ensure!(
            file_bytes(&mut file, &rows, ALLOCATOR_VTABLE + slot, 8)?
                == (0x140000000u64 + target).to_le_bytes(),
            "Native readiness allocator slot changed"
        );
    }
    // The singleton must live in writable, non-executable data (.data tail).
    ensure!(
        rows.iter().any(|s| MANAGER >= s.rva
            && MANAGER + 8 <= s.rva + s.virtual_size
            && s.flags & 0x8000_0000 != 0
            && s.flags & 0x2000_0000 == 0),
        "File-manager singleton is not in writable data"
    );
    Ok(())
}

/// The retained game image matches the disk-proven profile. Read-only.
pub(crate) fn verify_live(m: &impl Inspect) -> Result<()> {
    m.checkpoint()?;
    let header = m.read(m.base(), 4096)?;
    let (stamp, size, _) = sections(&header)?;
    ensure!(
        (stamp, size) == (STAMP, IMAGE_SIZE),
        "Running image is not the verified Version 2 build"
    );
    image_span(m, MANAGER, 8, false)?;
    image_span(m, VTABLE, 32, false)?;
    let table = m.read(m.base() + VTABLE, 32)?;
    for (slot, target) in MANAGER_VIRTUALS.iter().enumerate() {
        ensure!(
            table
                .get(slot * 8..slot * 8 + 8)
                .is_some_and(|b| b == (m.base() + target).to_le_bytes()),
            "Readiness manager native vtable differs"
        );
    }
    for (slot, target) in ALLOCATOR_VIRTUALS {
        image_span(m, ALLOCATOR_VTABLE + slot, 8, false)?;
        ensure!(
            m.read(m.base() + ALLOCATOR_VTABLE + slot, 8)? == (m.base() + target).to_le_bytes(),
            "Native readiness allocator dispatch differs"
        );
    }
    validate_methods(|rva, bytes| {
        image_span(m, rva, bytes, true)?;
        m.read(m.base() + rva, bytes)
    })?;
    verify_code(
        &m.read(m.base() + GETTER, GETTER_BYTES.len())?,
        &m.read(m.base() + ANCHOR, ANCHOR_BYTES.len())?,
    )?;
    m.checkpoint()
}
fn image_span(m: &impl Inspect, rva: u64, bytes: usize, code: bool) -> Result<()> {
    let at = m
        .base()
        .checked_add(rva)
        .context("Readiness image address overflow")?;
    let region = m.region(at)?;
    let allowed = if code {
        matches!(region.protection, 0x20 | 0x80)
    } else {
        matches!(region.protection, 0x02 | 0x04 | 0x08)
    };
    ensure!(
        region.committed
            && region.kind == 0x1000000
            && region.allocation == m.base()
            && allowed
            && at >= region.start
            && at.checked_add(bytes as u64).is_some_and(|end| region
                .start
                .checked_add(region.len)
                .is_some_and(|limit| end <= limit)),
        "Readiness image span is not in its exact image allocation"
    );
    Ok(())
}
fn pointer(m: &impl Memory) -> Result<u64> {
    Ok(u64::from_le_bytes(
        m.read(m.base() + MANAGER, 8)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("Partial readiness pointer"))?,
    ))
}
fn manager_state(m: &impl Inspect, manager: u64) -> Result<(u64, u32)> {
    ensure!(
        manager >= 0x10000 && manager % 8 == 0,
        "Invalid native manager pointer"
    );
    let region = m.region(manager)?;
    ensure!(
        region.committed
            && region.protection == 0x04
            && (region.kind == 0x20000 && region.allocation <= manager
                || region.kind == 0x1000000
                    && region.allocation == m.base()
                    && manager >= m.base() + ARENA_START
                    && manager
                        .checked_add(MANAGER_BYTES as u64)
                        .is_some_and(|end| end <= m.base() + ARENA_START + ARENA_BYTES))
            && manager >= region.start
            && manager
                .checked_add(MANAGER_BYTES as u64)
                .is_some_and(|end| region
                    .start
                    .checked_add(region.len)
                    .is_some_and(|limit| end <= limit)),
        "Readiness manager is not a complete native private or exact static-arena object"
    );
    let vtable = u64::from_le_bytes(
        m.read(manager, 8)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("Partial native vtable"))?,
    );
    ensure!(
        vtable == m.base() + VTABLE,
        "Readiness manager class differs"
    );
    let id = u32::from_le_bytes(
        m.read(manager + DEFAULT_DEVICE_OFFSET, 4)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("Partial native default ID"))?,
    );
    Ok((vtable, id))
}
/// Reprove the fixed image and read only pointer/class/default metadata twice.
/// No object is adopted from a changed or partially initialized manager.
pub(crate) fn sample(m: &impl Inspect) -> Result<(u64, bool)> {
    verify_live(m)?;
    let manager = pointer(m)?;
    if manager == 0 {
        m.checkpoint()?;
        return Ok((0, false));
    }
    let first = manager_state(m, manager)?;
    let second = manager_state(m, manager)?;
    let unchanged = pointer(m)? == manager && first == second;
    // Reader::region checks retained handle identity before querying.
    image_span(m, MANAGER, 8, false)?;
    m.checkpoint()?;
    if !unchanged {
        return Ok((0, false));
    }
    Ok((manager, first.1 == STOCK_DEFAULT_DEVICE))
}

/// Port of WPF `Stable`: ready once the same non-null stock manager has been
/// observed continuously for `stable`. Any change or error restarts it.
#[derive(Default)]
pub(crate) struct Stable {
    pointer: u64,
    since: Option<Duration>,
}
impl Stable {
    pub(crate) fn update(
        &mut self,
        pointer: u64,
        stock: bool,
        now: Duration,
        stable: Duration,
    ) -> bool {
        if pointer == 0 || !stock {
            *self = Self::default();
            return false;
        }
        if self.pointer != pointer || self.since.is_none() {
            self.pointer = pointer;
            self.since = Some(now);
        }
        now.saturating_sub(self.since.unwrap()) >= stable
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blacktop_arena::Region;
    use std::{
        cell::{Cell, RefCell},
        collections::BTreeMap,
    };
    const FIXTURE: &[u8] = include_bytes!("../assets/startup-readiness-v2-fixture.bin");
    const HEAP: u64 = 0x2_0000_0000;
    struct Fake {
        bytes: RefCell<BTreeMap<u64, u8>>,
        heap_kind: Cell<u32>,
        arena_allocation: Cell<u64>,
        arena_protection: Cell<u32>,
        heap_protection: Cell<u32>,
        torn: Cell<bool>,
        torn_pointer: Cell<bool>,
        pointer_reads: Cell<usize>,
        reads: Cell<usize>,
        fail_checkpoint: Cell<bool>,
    }
    impl Fake {
        fn put(&self, at: u64, bytes: &[u8]) {
            for (i, byte) in bytes.iter().enumerate() {
                self.bytes.borrow_mut().insert(at + i as u64, *byte);
            }
        }
        fn new() -> Self {
            let f = Self {
                bytes: RefCell::new(BTreeMap::new()),
                heap_kind: Cell::new(0x20000),
                arena_allocation: Cell::new(0x1_4000_0000),
                arena_protection: Cell::new(4),
                heap_protection: Cell::new(4),
                torn: Cell::new(false),
                torn_pointer: Cell::new(false),
                pointer_reads: Cell::new(0),
                reads: Cell::new(0),
                fail_checkpoint: Cell::new(false),
            };
            let mut header = vec![0; 4096];
            header[..2].copy_from_slice(b"MZ");
            header[60..64].copy_from_slice(&0x100u32.to_le_bytes());
            header[0x100..0x104].copy_from_slice(b"PE\0\0");
            header[0x108..0x10C].copy_from_slice(&STAMP.to_le_bytes());
            header[0x150..0x154].copy_from_slice(&IMAGE_SIZE.to_le_bytes());
            f.put(f.base(), &header);
            let mut offset = 0;
            for method in METHODS {
                f.put(
                    f.base() + method.rva,
                    &FIXTURE[offset..offset + method.bytes],
                );
                offset += method.bytes;
            }
            f.put(f.base() + VTABLE, &FIXTURE[offset..offset + 32]);
            offset += 32;
            f.put(
                f.base() + ALLOCATOR_VTABLE,
                &FIXTURE[offset..offset + 0x230],
            );
            f.put(f.base() + MANAGER, &HEAP.to_le_bytes());
            f.put(HEAP, &(f.base() + VTABLE).to_le_bytes());
            f.put(
                HEAP + DEFAULT_DEVICE_OFFSET,
                &STOCK_DEFAULT_DEVICE.to_le_bytes(),
            );
            f
        }
    }
    impl Memory for Fake {
        fn base(&self) -> u64 {
            0x1_4000_0000
        }
        fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
            if at == self.base() + MANAGER {
                self.pointer_reads.set(self.pointer_reads.get() + 1);
                if self.torn_pointer.get() && self.pointer_reads.get() % 2 == 0 {
                    return Ok(0u64.to_le_bytes().to_vec());
                }
            }
            if at == HEAP + DEFAULT_DEVICE_OFFSET {
                self.reads.set(self.reads.get() + 1);
                if self.torn.get() && self.reads.get() % 2 == 0 {
                    return Ok(0u32.to_le_bytes().to_vec());
                }
            }
            (at..at + len as u64)
                .map(|p| {
                    self.bytes
                        .borrow()
                        .get(&p)
                        .copied()
                        .context("Unmapped fixture")
                })
                .collect()
        }
        fn checkpoint(&self) -> Result<()> {
            ensure!(!self.fail_checkpoint.get(), "Owner changed");
            Ok(())
        }
    }
    impl Inspect for Fake {
        fn region(&self, at: u64) -> Result<Region> {
            if (HEAP..HEAP + 4096).contains(&at) {
                return Ok(Region {
                    start: HEAP,
                    len: 4096,
                    allocation: HEAP,
                    kind: self.heap_kind.get(),
                    protection: self.heap_protection.get(),
                    committed: true,
                });
            }
            if (self.base() + ARENA_START..self.base() + ARENA_START + ARENA_BYTES).contains(&at) {
                return Ok(Region {
                    start: self.base() + ARENA_START,
                    len: ARENA_BYTES,
                    allocation: self.arena_allocation.get(),
                    kind: 0x1000000,
                    protection: self.arena_protection.get(),
                    committed: true,
                });
            }
            let code = METHODS
                .iter()
                .any(|x| (self.base() + x.rva..self.base() + x.rva + x.bytes as u64).contains(&at));
            Ok(Region {
                start: self.base(),
                len: IMAGE_SIZE as u64,
                allocation: self.base(),
                kind: 0x1000000,
                protection: if code { 0x20 } else { 0x04 },
                committed: true,
            })
        }
    }
    #[test]
    fn exact_native_manager_can_end_gate_after_one_second() {
        let f = Fake::new();
        let mut stable = Stable::default();
        for ms in [0, 100, 999, 1000] {
            let (pointer, stock) = sample(&f).unwrap();
            assert_eq!(
                stable.update(pointer, stock, Duration::from_millis(ms), STABLE),
                ms == 1000
            );
        }
        assert_eq!(anchor_target(&ANCHOR_BYTES).unwrap(), MANAGER);
    }
    #[test]
    fn wrong_class_foreign_region_and_protection_never_qualify() {
        let f = Fake::new();
        f.put(HEAP, &(f.base() + VTABLE + 8).to_le_bytes());
        assert!(sample(&f).is_err());
        let f = Fake::new();
        f.heap_kind.set(0x1000000);
        assert!(sample(&f).is_err());
        let f = Fake::new();
        f.heap_protection.set(0x40);
        assert!(sample(&f).is_err());
        let f = Fake::new();
        f.put(f.base() + MANAGER, &(HEAP + 4096 - 8).to_le_bytes());
        assert!(sample(&f).is_err());
    }
    #[test]
    fn torn_default_and_changed_pointer_restart_stability() {
        let f = Fake::new();
        f.torn.set(true);
        assert_eq!(sample(&f).unwrap(), (0, false));
        let g = Fake::new();
        g.torn_pointer.set(true);
        assert_eq!(sample(&g).unwrap(), (0, false));
        let mut s = Stable::default();
        let ms = Duration::from_millis;
        assert!(!s.update(HEAP, true, ms(0), STABLE));
        assert!(!s.update(0, false, ms(999), STABLE));
        assert!(!s.update(HEAP, true, ms(1000), STABLE));
        assert!(!s.update(HEAP + 8, true, ms(2000), STABLE));
        assert!(s.update(HEAP + 8, true, ms(3000), STABLE));
    }
    #[test]
    fn transient_and_profile_failures_keep_timeout_fallback() {
        let f = Fake::new();
        let mut s = Stable::default();
        for ms in (0..20000).step_by(100) {
            // Same reset policy as SettleProbe::ready: errors never become success.
            f.put(
                f.base() + METHODS[0].rva + METHODS[0].bytes as u64 - 1,
                &[FIXTURE[METHODS[0].bytes - 1] ^ 1],
            );
            match sample(&f) {
                Ok((p, stock)) => assert!(!s.update(p, stock, Duration::from_millis(ms), STABLE)),
                Err(_) => s = Stable::default(),
            }
        }
        let f = Fake::new();
        f.fail_checkpoint.set(true);
        assert!(sample(&f).is_err());
        let f = Fake::new();
        f.put(HEAP + DEFAULT_DEVICE_OFFSET, &0xC27D1610u32.to_le_bytes());
        assert_eq!(sample(&f).unwrap(), (HEAP, false));
        f.put(f.base() + MANAGER, &0u64.to_le_bytes());
        assert_eq!(sample(&f).unwrap(), (0, false));
    }
    #[test]
    fn exact_native_image_arena_retains_class_and_bounds_guards() {
        let f = Fake::new();
        let manager = f.base() + ARENA_START + 0x50;
        f.put(f.base() + MANAGER, &manager.to_le_bytes());
        f.put(manager, &(f.base() + VTABLE).to_le_bytes());
        f.put(
            manager + DEFAULT_DEVICE_OFFSET,
            &STOCK_DEFAULT_DEVICE.to_le_bytes(),
        );
        assert_eq!(sample(&f).unwrap(), (manager, true));
        let mut stable = Stable::default();
        assert!(!stable.update(manager, true, Duration::ZERO, STABLE));
        assert!(stable.update(manager, true, STABLE, STABLE));
        f.arena_allocation.set(f.base() + 4096);
        assert!(sample(&f).is_err());
        f.arena_allocation.set(f.base());
        f.arena_protection.set(0x40);
        assert!(sample(&f).is_err());
        f.arena_protection.set(4);
        f.put(manager, &(f.base() + VTABLE + 8).to_le_bytes());
        assert!(sample(&f).is_err());
        for pointer in [
            f.base() + ARENA_START - 8,
            f.base() + ARENA_START + ARENA_BYTES - MANAGER_BYTES as u64 + 8,
        ] {
            f.put(f.base() + MANAGER, &pointer.to_le_bytes());
            f.put(pointer, &(f.base() + VTABLE).to_le_bytes());
            f.put(
                pointer + DEFAULT_DEVICE_OFFSET,
                &STOCK_DEFAULT_DEVICE.to_le_bytes(),
            );
            assert!(sample(&f).is_err());
        }
    }
    #[test]
    fn native_arena_allocator_mutation_never_qualifies() {
        for (slot, target) in ALLOCATOR_VIRTUALS {
            let f = Fake::new();
            f.put(
                f.base() + ALLOCATOR_VTABLE + slot,
                &(f.base() + target + 1).to_le_bytes(),
            );
            assert!(sample(&f).is_err());
        }
        let f = Fake::new();
        let manager = f.base() + ARENA_START + 0x50;
        f.put(f.base() + MANAGER, &manager.to_le_bytes());
        f.put(manager, &(f.base() + VTABLE).to_le_bytes());
        f.put(
            manager + DEFAULT_DEVICE_OFFSET,
            &STOCK_DEFAULT_DEVICE.to_le_bytes(),
        );
        f.put(f.base() + 0x4D6E5E8, &[0x90]);
        assert!(sample(&f).is_err());
    }
    #[test]
    fn every_complete_native_method_is_mandatory() {
        for method in METHODS {
            let f = Fake::new();
            let at = f.base() + method.rva + method.bytes as u64 - 1;
            let value = f.read(at, 1).unwrap()[0] ^ 1;
            f.put(at, &[value]);
            assert!(verify_live(&f).is_err());
        }
        let f = Fake::new();
        f.put(
            f.base() + VTABLE,
            &(f.base() + MANAGER_VIRTUALS[0] + 1).to_le_bytes(),
        );
        assert!(verify_live(&f).is_err());
        let f = Fake::new();
        f.put(f.base() + 0x108, &0x6AA2013Du32.to_le_bytes());
        assert!(verify_live(&f).is_err());
    }
    #[test]
    #[ignore = "Explicit disk-only installed image audit"]
    fn startup_readiness_v2_installed_disk_proof() {
        let path = std::env::var("C27_READINESS_V2_DISK").expect("C27_READINESS_V2_DISK");
        disk_proof(Path::new(&path)).unwrap();
    }
}
