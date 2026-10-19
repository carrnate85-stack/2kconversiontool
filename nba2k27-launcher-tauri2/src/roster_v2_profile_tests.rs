use super::*;
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashMap},
};
const BASE: u64 = 0x18000_0000;
const REGION: OwnedRegion = OwnedRegion {
    base: 0x22000_0000,
    len: 0x800000,
};
const CONTAINER: u64 = 0x24000_0000;
const OWNER_AT: u64 = REGION.base + 0x50;

struct Fake {
    bytes: RefCell<BTreeMap<u64, u8>>,
    short: Cell<u64>,
    ended: Cell<bool>,
    counts: RefCell<HashMap<u64, usize>>,
    mutation: RefCell<Option<(u64, usize, u64, Vec<u8>)>>,
}
impl Fake {
    fn put(&self, at: u64, bytes: &[u8]) {
        for (i, byte) in bytes.iter().enumerate() {
            self.bytes.borrow_mut().insert(at + i as u64, *byte);
        }
    }
    fn q(&self, at: u64, value: u64) {
        self.put(at, &value.to_le_bytes());
    }
    fn header(&self, offset: u64, count: usize, table: u64) {
        self.q(OWNER_AT + offset, count as u64);
        self.q(OWNER_AT + offset + 8, table);
    }
    fn fixture() -> Self {
        let m = Self {
            bytes: RefCell::new(BTreeMap::new()),
            short: Cell::new(0),
            ended: Cell::new(false),
            counts: RefCell::new(HashMap::new()),
            mutation: RefCell::new(None),
        };
        let mut h = vec![0; 4096];
        h[..2].copy_from_slice(b"MZ");
        h[60..64].copy_from_slice(&128u32.to_le_bytes());
        h[128..132].copy_from_slice(b"PE\0\0");
        h[132..134].copy_from_slice(&0x8664u16.to_le_bytes());
        h[136..140].copy_from_slice(&0x6ac5_3765u32.to_le_bytes());
        h[152..154].copy_from_slice(&0x20bu16.to_le_bytes());
        h[208..212].copy_from_slice(&0x35bd_e000u32.to_le_bytes());
        m.put(BASE, &h);
        for &(rva, prefix, target) in RIP_ANCHORS {
            let mut b = prefix.to_vec();
            b.extend_from_slice(
                &((target as i64 - rva as i64 - prefix.len() as i64 - 4) as i32).to_le_bytes(),
            );
            m.put(BASE + rva, &b);
        }
        for &(rva, b) in FIELD_ANCHORS {
            m.put(BASE + rva, b);
        }
        m.q(BASE + ROOT_CLASS, BASE + 0x024e_65f0);
        m.q(BASE + ROOT_CLASS + 8, BASE + 0x024e_6660);
        m.q(BASE + ROOT, CONTAINER);
        m.q(CONTAINER, BASE + ROOT_CLASS);
        m.q(CONTAINER + 0xa8, OWNER_AT);
        m.header(0x10, 200, REGION.base + 0x680);
        m.header(0x60, 16, REGION.base + 0x100000);
        m.header(0x80, 40, REGION.base + 0x200000);
        m.header(0x290, 100, REGION.base + 0x300000);
        m
    }
}
impl Memory for Fake {
    fn base(&self) -> u64 {
        BASE
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        let count = {
            let mut c = self.counts.borrow_mut();
            let n = c.entry(at).or_default();
            *n += 1;
            *n
        };
        if let Some((trigger, nth, target, bytes)) = self.mutation.borrow().clone() {
            if at == trigger && count == nth {
                self.put(target, &bytes);
            }
        }
        let b = self.bytes.borrow();
        let mut result = Vec::new();
        for i in 0..len {
            result.push(
                *b.get(&(at + i as u64))
                    .context("Unmapped roster fixture read")?,
            );
        }
        if self.short.get() == at {
            result.pop();
        }
        Ok(result)
    }
    fn checkpoint(&self) -> Result<()> {
        ensure!(!self.ended.get(), "Game ended");
        Ok(())
    }
}

#[test]
fn captures_only_read_only_directory_ownership() {
    let m = Fake::fixture();
    let before = m.bytes.borrow().clone();
    let s = inspect_owner(&m, REGION).unwrap();
    assert_eq!(s.root, CONTAINER);
    assert_eq!(s.owner, OWNER_AT);
    assert_eq!(s.players.count, 200);
    assert_eq!(s.players.stride, 0x4f8);
    assert_eq!(s.teams.count, 40);
    assert_eq!(s.arenas.stride, 0x1278);
    assert_eq!(s.uniforms.as_ref().unwrap().pointer, REGION.base + 0x300000);
    assert_eq!(*m.bytes.borrow(), before);
    revalidate(&m, &s).unwrap();
    assert!(!PLAYER_FIELDS_VERIFIED);
    assert!(!TEAM_FIELDS_VERIFIED);
    assert!(!WRITES_SUPPORTED);
}

#[test]
fn current_uniform_header_is_not_the_old_allocation_relative_header() {
    let m = Fake::fixture();
    // A plausible obsolete header must not replace the independently located
    // owner+290 descriptor. It describes entirely different storage.
    m.q(REGION.base + 0x290, 4);
    m.q(REGION.base + 0x298, REGION.base + 0x400000);
    let s = inspect_owner(&m, REGION).unwrap();
    assert_eq!(s.uniforms.unwrap().count, 100);
    m.header(0x290, 0, 0);
    assert_eq!(inspect_owner(&m, REGION).unwrap().uniforms, None);
}

#[test]
fn all_directory_bounds_and_nonoverlap_are_mandatory() {
    for fault in 0..10 {
        let m = Fake::fixture();
        match fault {
            0 => m.header(0x10, 99, REGION.base + 0x680),
            1 => m.header(0x10, 8193, REGION.base + 0x680),
            2 => m.header(0x80, 1025, REGION.base + 0x200000),
            3 => m.header(0x60, 2049, REGION.base + 0x100000),
            4 => m.header(0x290, 8193, REGION.base + 0x300000),
            5 => m.header(0x290, 1, REGION.base + REGION.len - 8),
            6 => m.header(0x80, 1, REGION.base + 0x680),
            7 => m.header(0x80, 1, REGION.base + 0x678),
            8 => m.header(0x80, 1, u64::MAX),
            9 => m.header(0x290, 0, REGION.base + 0x300000),
            _ => unreachable!(),
        }
        assert!(inspect_owner(&m, REGION).is_err(), "Fault {fault} accepted");
    }
    for region in [
        OwnedRegion {
            base: REGION.base + 8,
            ..REGION
        },
        OwnedRegion {
            len: 0x7fffff,
            ..REGION
        },
        OwnedRegion {
            len: 0x8000001,
            ..REGION
        },
    ] {
        assert!(inspect_owner(&Fake::fixture(), region).is_err());
    }
}

#[test]
fn obsolete_roots_foreign_classes_and_stale_headers_fail_closed() {
    let m = Fake::fixture();
    m.q(BASE + ROOT, 0);
    m.q(BASE + 0x09a4_7eb8, CONTAINER);
    assert!(inspect_owner(&m, REGION).is_err());
    let m = Fake::fixture();
    m.q(CONTAINER, BASE + ROOT_CLASS + 8);
    assert!(inspect_owner(&m, REGION).is_err());
    let m = Fake::fixture();
    m.q(CONTAINER + 0xa8, 0);
    assert!(inspect_owner(&m, REGION).is_err());
    for offset in [0x10, 0x60, 0x80, 0x290] {
        let m = Fake::fixture();
        let s = inspect_owner(&m, REGION).unwrap();
        m.q(OWNER_AT + offset, 1);
        assert!(revalidate(&m, &s).is_err());
    }
}

#[test]
fn code_targets_headers_and_short_reads_are_never_inferred() {
    for &(rva, prefix, _) in RIP_ANCHORS {
        for offset in [0, prefix.len()] {
            let m = Fake::fixture();
            let b = m.read(BASE + rva + offset as u64, 1).unwrap()[0];
            m.put(BASE + rva + offset as u64, &[b ^ 1]);
            assert!(verify_identity_profile(&m).is_err());
        }
    }
    for &(rva, _) in FIELD_ANCHORS {
        let m = Fake::fixture();
        let b = m.read(BASE + rva, 1).unwrap()[0];
        m.put(BASE + rva, &[b ^ 1]);
        assert!(verify_identity_profile(&m).is_err());
    }
    for address in [
        BASE,
        BASE + ROOT,
        CONTAINER,
        OWNER_AT + 0x10,
        OWNER_AT + 0x60,
        OWNER_AT + 0x80,
        OWNER_AT + 0x290,
    ] {
        let m = Fake::fixture();
        m.short.set(address);
        assert!(inspect_owner(&m, REGION).is_err());
    }
    let m = Fake::fixture();
    m.ended.set(true);
    assert!(inspect_owner(&m, REGION).is_err());
}

#[test]
fn mid_capture_owner_and_table_changes_are_rejected() {
    for (trigger, target, bytes) in [
        (
            BASE + ROOT,
            BASE + ROOT,
            (CONTAINER + 8).to_le_bytes().to_vec(),
        ),
        (
            OWNER_AT + 0x10,
            OWNER_AT + 0x10,
            201u64.to_le_bytes().to_vec(),
        ),
    ] {
        let m = Fake::fixture();
        *m.mutation.borrow_mut() = Some((trigger, 2, target, bytes));
        assert!(inspect_owner(&m, REGION).is_err());
    }
    let m = Fake::fixture();
    m.put(BASE + SERIALIZER, &vec![0; SERIALIZER_BYTES]);
    assert!(verify_profile(&m).is_err());
}

#[test]
#[ignore]
fn exact_installed_version_two_roster_profile_is_disk_only() {
    use std::{
        fs::File,
        io::{Read, Seek, SeekFrom},
        path::PathBuf,
    };
    let image = PathBuf::from(
        std::env::var_os("C27_ROSTER_GAME_EXE").expect("Explicit disk image required"),
    );
    let mut file = File::open(image).unwrap();
    let mut hash = Sha256::new();
    let mut chunk = vec![0; 1024 * 1024];
    loop {
        let n = file.read(&mut chunk).unwrap();
        if n == 0 {
            break;
        }
        hash.update(&chunk[..n]);
    }
    assert_eq!(
        format!("{:X}", hash.finalize()),
        "29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1"
    );
    file.seek(SeekFrom::Start(0)).unwrap();
    let mut header = vec![0; 4096];
    file.read_exact(&mut header).unwrap();
    let pe = u32::from_le_bytes(header[60..64].try_into().unwrap()) as usize;
    let count = u16::from_le_bytes(header[pe + 6..pe + 8].try_into().unwrap()) as usize;
    let opt = u16::from_le_bytes(header[pe + 20..pe + 22].try_into().unwrap()) as usize;
    let base = u64::from_le_bytes(header[pe + 48..pe + 56].try_into().unwrap());
    assert!((1..=32).contains(&count));
    let mut sections = Vec::new();
    for i in 0..count {
        let at = pe + 24 + opt + i * 40;
        sections.push((
            u32::from_le_bytes(header[at + 12..at + 16].try_into().unwrap()) as u64,
            u32::from_le_bytes(header[at + 16..at + 20].try_into().unwrap()) as u64,
            u32::from_le_bytes(header[at + 20..at + 24].try_into().unwrap()) as u64,
        ));
    }
    struct Disk {
        file: RefCell<File>,
        header: Vec<u8>,
        base: u64,
        sections: Vec<(u64, u64, u64)>,
    }
    impl Memory for Disk {
        fn base(&self) -> u64 {
            self.base
        }
        fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
            let rva = at.checked_sub(self.base).context("Below image")?;
            if rva == 0 && len == 4096 {
                return Ok(self.header.clone());
            }
            let (start, _, raw) = self
                .sections
                .iter()
                .find(|(start, len2, _)| {
                    rva >= *start
                        && rva
                            .checked_add(len as u64)
                            .is_some_and(|end| end <= start + len2)
                })
                .context("Not disk-backed")?;
            let mut b = vec![0; len];
            let mut f = self.file.borrow_mut();
            f.seek(SeekFrom::Start(raw + rva - start))?;
            f.read_exact(&mut b)?;
            Ok(b)
        }
    }
    let disk = Disk {
        file: RefCell::new(file),
        header,
        base,
        sections,
    };
    verify_profile(&disk).unwrap();
    println!(
        "{}",
        serde_json::json!({"pass":true,"profile":"version-two-read-only-roster-owner","root":"09ACE338","rootClass":"074EBE98","serializer":"006AE330","serializerSha256":SERIALIZER_SHA256,"playerFieldsVerified":PLAYER_FIELDS_VERIFIED,"teamFieldsVerified":TEAM_FIELDS_VERIFIED,"writesSupported":WRITES_SUPPORTED,"gameProcessAccess":false,"gameFilesChanged":false})
    );
}
