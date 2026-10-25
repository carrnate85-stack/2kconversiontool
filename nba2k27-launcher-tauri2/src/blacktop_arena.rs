//! Native-only arena decoding and plans, ported from the verified ArenaTool.
//! Exact catalog IDs only. No frontend addresses, process discovery or hooks.
#![allow(dead_code)]
use crate::blacktop_native::{self, Memory};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

pub(crate) const MAPPING: u64 = 0x787AFD0;
const ROSTER: u64 = 0x9A47EB8;
const STRIDE: u64 = 0x1278;
pub(crate) const STOCK: [u8; 20] = [
    0xb9, 3, 0, 0, 0xba, 3, 0, 0, 0x8b, 3, 0, 0, 0xc5, 2, 0, 0, 0xc5, 2, 0, 0,
];
const ANCHORS: &[(u64, &[u8])] = &[
    (0x32BD8EE, &[0x48, 0x8d, 0x0d, 0xdb, 0xd6, 0x5b, 0x04]),
    (0x32BE367, &[0x48, 0x8d, 0x0d, 0x62, 0xcc, 0x5b, 0x04]),
    (0x32BF419, &[0x48, 0x8d, 0x0d, 0xb0, 0xbb, 0x5b, 0x04]),
    (0x32BF7CE, &[0x48, 0x8d, 0x0d, 0xfb, 0xb7, 0x5b, 0x04]),
    (0x22DFAC0, &[0x8b, 0x05, 0x8a, 0xb9, 0x7c, 0x07, 0xc3]),
    (
        0x2EE8EDC,
        &[
            0x0f, 0xb7, 0x88, 0x92, 0x07, 0, 0, 0x39, 0xce, 0x49, 0x0f, 0x45, 0xc5,
        ],
    ),
];
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Choice {
    pub id: String,
    pub label: String,
    pub arena_id: Option<u32>,
    pub file: Option<String>,
    pub floor_file: Option<String>,
    pub template_arena_id: Option<u32>,
    #[serde(default)]
    pub disabled: bool,
}
#[derive(Deserialize)]
struct Catalog {
    choices: Vec<Choice>,
}
pub(crate) fn choices() -> &'static [Choice] {
    static C: OnceLock<Catalog> = OnceLock::new();
    &C.get_or_init(|| {
        serde_json::from_str(include_str!("../assets/arena-live.json"))
            .expect("Original arena catalog")
    })
    .choices
}
pub(crate) fn choice(id: &str) -> Result<Choice> {
    if id.starts_with("custom-") {
        let c = crate::custom_blacktop::resolve(id)?;
        choice_fields(&c)?;
        return Ok(c);
    }
    let mut rows = choices().iter().filter(|c| c.id == id);
    let c = rows
        .next()
        .context("Arena asset ID is not in the catalog")?;
    ensure!(
        rows.next().is_none() && !c.disabled,
        "Arena is disabled or ambiguous"
    );
    if c.id != "default" {
        ensure!(
            c.arena_id.is_some_and(|id| id <= u16::MAX as u32)
                && c.file.as_ref().is_some_and(|s| valid_file(s, 38)),
            "Arena has no verified roster mapping"
        );
        if c.template_arena_id.is_some() {
            ensure!(
                c.template_arena_id == Some(700) && c.arena_id == Some(700),
                "Unknown arena template"
            );
            template_fields(
                c.file.as_deref().unwrap(),
                c.floor_file.as_deref().context("Arena floor is missing")?,
            )?;
        }
    }
    Ok(c.clone())
}
fn valid_file(s: &str, max: usize) -> bool {
    !s.is_empty()
        && s.len() < max
        && s.bytes()
            .all(|b| b == b'_' || b.is_ascii_lowercase() || b.is_ascii_digit())
}
pub(crate) fn template_fields(file: &str, floor: &str) -> Result<Vec<u8>> {
    ensure!(floor == format!("{file}_floor"), "Invalid arena template filenames");
    filename_fields(file, floor)
}
pub(crate) fn choice_fields(c: &Choice) -> Result<Vec<u8>> {
    let file = c.file.as_deref().context("Arena file missing")?;
    let floor = c.floor_file.as_deref().context("Court file missing")?;
    if c.id.starts_with("custom-") {
        ensure!(crate::custom_blacktop::resolve(&c.id)? == *c, "Unregistered custom arena fields");
        filename_fields(file, floor)
    } else {
        template_fields(file, floor)
    }
}
pub(crate) fn filename_fields(file: &str, floor: &str) -> Result<Vec<u8>> {
    ensure!(
        valid_file(file, 38) && valid_file(floor, 40),
        "Invalid arena template filenames"
    );
    let mut bytes = vec![0; 78];
    bytes[..file.len()].copy_from_slice(file.as_bytes());
    bytes[38..38 + floor.len()].copy_from_slice(floor.as_bytes());
    Ok(bytes)
}
pub(crate) fn mapping(id: Option<u32>) -> Result<[u8; 20]> {
    let mut bytes = STOCK;
    if let Some(id) = id {
        ensure!(id <= u16::MAX as u32, "Invalid arena ID");
        bytes[..4].copy_from_slice(&id.to_le_bytes());
        bytes[4..8].copy_from_slice(&id.to_le_bytes());
    }
    Ok(bytes)
}
pub(crate) fn valid_mapping(bytes: &[u8; 20]) -> bool {
    *bytes == STOCK
        || choices()
            .iter()
            .filter(|c| !c.disabled && c.id != "default")
            .any(|c| mapping(c.arena_id).is_ok_and(|b| b == *bytes))
}
pub(crate) fn valid_template_fields(record: &Record, bytes: &[u8]) -> bool {
    record.id == 700
        && (bytes == record.fields
            || crate::custom_blacktop::known_fields(bytes)
            || choices()
                .iter()
                .filter(|c| !c.disabled && c.template_arena_id == Some(700))
                .any(|c| {
                    template_fields(
                        c.file.as_deref().unwrap_or(""),
                        c.floor_file.as_deref().unwrap_or(""),
                    )
                    .is_ok_and(|b| b == bytes)
                }))
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Region {
    pub start: u64,
    pub len: u64,
    pub allocation: u64,
    pub kind: u32,
    pub protection: u32,
    pub committed: bool,
}
impl Region {
    fn contains(self, at: u64, len: u64) -> bool {
        self.committed
            && self.protection & 0x101 == 0
            && at >= self.start
            && len > 0
            && at.checked_add(len).is_some_and(|end| {
                self.start
                    .checked_add(self.len)
                    .is_some_and(|limit| end <= limit)
            })
    }
}
pub(crate) trait Inspect: Memory {
    fn region(&self, at: u64) -> Result<Region>;
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Record {
    pub address: u64,
    pub header_address: u64,
    pub root: u64,
    pub owner: u64,
    pub allocation: u64,
    pub header: Vec<u8>,
    pub fields: Vec<u8>,
    pub id: u16,
}
#[derive(Clone, Debug)]
pub(crate) struct Snapshot {
    pub image: u64,
    pub context: blacktop_native::ContextState,
    pub mapping: [u8; 20],
    pub record: Option<Record>,
}
fn read(m: &impl Memory, at: u64, len: usize) -> Result<Vec<u8>> {
    m.checkpoint()?;
    ensure!(
        at >= 65536
            && len > 0
            && len <= 4096
            && at
                .checked_add(len as u64)
                .is_some_and(|end| end <= 0x7fff_ffff_ffff),
        "Invalid bounded arena read"
    );
    let bytes = m.read(at, len)?;
    ensure!(bytes.len() == len, "Partial arena read");
    Ok(bytes)
}
fn q(m: &impl Memory, at: u64) -> Result<u64> {
    Ok(u64::from_le_bytes(read(m, at, 8)?.try_into().unwrap()))
}
fn ascii(bytes: &[u8]) -> Result<String> {
    let end = bytes
        .iter()
        .position(|v| *v == 0)
        .context("Unterminated arena filename")?;
    ensure!(bytes[..end].is_ascii(), "Non-ASCII arena filename");
    Ok(String::from_utf8(bytes[..end].to_vec())?)
}
pub(crate) fn validate(m: &impl Inspect) -> Result<()> {
    blacktop_native::validate_build(m)?;
    for &(rva, bytes) in ANCHORS {
        ensure!(
            read(m, m.base() + rva, bytes.len())? == bytes,
            "Arena code anchor changed"
        );
    }
    let p = m.region(m.base() + MAPPING)?;
    ensure!(
        p.contains(m.base() + MAPPING, 20)
            && p.kind == 0x1000000
            && p.protection == 2
            && p.allocation == m.base(),
        "Arena mapping is not the verified read-only image data"
    );
    Ok(())
}
pub(crate) fn selection(m: &impl Memory) -> Result<blacktop_native::ContextState> {
    let c = blacktop_native::context(m)?;
    ensure!(
        blacktop_native::boundary(&c, m.base()) == "selection",
        "Return to Blacktop player selection before changing arenas"
    );
    Ok(c)
}
pub(crate) fn revalidate(m: &impl Inspect, r: &Record, expected: &[u8]) -> Result<()> {
    ensure!(
        r.header.len() == 16 && expected.len() == 78 && r.fields.len() == 78 && r.address >= 65536,
        "Invalid native arena record lease"
    );
    let count = u64::from_le_bytes(r.header[..8].try_into().unwrap());
    let table = u64::from_le_bytes(r.header[8..].try_into().unwrap());
    ensure!(
        (1..=2048).contains(&count)
            && r.header_address == r.owner.checked_add(0x60).context("Arena header overflow")?
            && r.owner
                == r.allocation
                    .checked_add(0x50)
                    .context("Arena owner overflow")?
            && r.address
                .checked_sub(table)
                .is_some_and(|off| off % STRIDE == 0 && off / STRIDE < count),
        "Arena lease is outside its recorded directory"
    );
    ensure!(
        q(m, m.base() + ROSTER)? == r.root
            && q(m, r.root + 0xa8)? == r.owner
            && read(m, r.header_address, 16)? == r.header,
        "Arena directory ownership changed"
    );
    ensure!(
        read(m, r.address + 0x792, 2)? == r.id.to_le_bytes()
            && read(m, r.address + 0x7e, 78)? == expected,
        "Arena fields changed outside this launcher"
    );
    let p = m.region(r.address + 0x7e)?;
    ensure!(
        p.contains(r.address + 0x7e, 78) && p.kind == 0x20000 && p.allocation == r.allocation,
        "Arena record left its private roster allocation"
    );
    Ok(())
}
pub(crate) fn capture(m: &impl Inspect, c: &Choice, expected: &[u8; 20]) -> Result<Snapshot> {
    ensure!(
        choice(&c.id)? == *c,
        "Arena request differs from the exact embedded catalog record"
    );
    validate(m)?;
    let before = selection(m)?;
    let current: [u8; 20] = read(m, m.base() + MAPPING, 20)?.try_into().unwrap();
    ensure!(
        &current == expected && current[8..] == STOCK[8..],
        "Arena mapping changed outside this session"
    );
    let record = if c.id == "default" {
        None
    } else {
        let wanted = c.arena_id.context("Arena record ID missing")?;
        let file = if c.template_arena_id.is_some() {
            "arena_700_int"
        } else {
            c.file.as_deref().context("Arena file missing")?
        };
        let root = q(m, m.base() + ROSTER)?;
        let owner = q(m, root + 0xa8)?;
        let p = m.region(owner)?;
        ensure!(
            p.contains(owner, 16) && p.kind == 0x20000 && owner == p.allocation + 0x50,
            "Active roster header layout changed"
        );
        let header_address = owner + 0x60;
        let header = read(m, header_address, 16)?;
        let count = u64::from_le_bytes(header[..8].try_into().unwrap());
        let table = u64::from_le_bytes(header[8..].try_into().unwrap());
        ensure!(
            (1..=2048).contains(&count) && table >= header_address + 16 && table < 0x7fff_ffff_0000,
            "Arena directory bounds changed"
        );
        let table_region = m.region(table)?;
        ensure!(
            table_region.contains(table, count * STRIDE)
                && table_region.kind == 0x20000
                && table_region.allocation == p.allocation,
            "Arena table is outside its private active roster"
        );
        let (mut known, mut matched, mut found) = (0, 0, None);
        for i in 0..count {
            let at = table + i * STRIDE;
            let row = read(m, at, 0x794)?;
            let id = u16::from_le_bytes(row[0x792..0x794].try_into().unwrap());
            let name = ascii(&row[0x7e..0x7e + 38])?;
            if id == 700 && name == "arena_700_int" {
                known += 1;
            }
            if id as u32 == wanted && name == file {
                matched += 1;
                found = Some(Record {
                    address: at,
                    header_address,
                    root,
                    owner,
                    allocation: p.allocation,
                    header: header.clone(),
                    fields: row[0x7e..0x7e + 78].to_vec(),
                    id,
                });
            }
        }
        ensure!(
            known == 1 && matched == 1,
            "Selected arena record is missing or ambiguous"
        );
        let r = found.unwrap();
        revalidate(m, &r, &r.fields)?;
        if c.template_arena_id.is_some() {
            ensure!(
                ascii(&r.fields[38..])? == "arena_700_int_floor",
                "Summer League template floor differs"
            );
        } else if let Some(floor) = &c.floor_file {
            ensure!(
                ascii(&r.fields[38..])? == *floor,
                "Arena floor differs from the verified catalog"
            );
        }
        Some(r)
    };
    ensure!(
        selection(m)? == before && read(m, m.base() + MAPPING, 20)? == current,
        "Arena context changed during preparation"
    );
    Ok(Snapshot {
        image: m.base(),
        context: before,
        mapping: current,
        record,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::{Cell, RefCell},
        collections::HashMap,
    };
    struct Fixture {
        data: HashMap<u64, Vec<u8>>,
        regions: Vec<Region>,
        cancel: Cell<bool>,
        torn: Option<u64>,
        reads: RefCell<HashMap<u64, usize>>,
    }
    impl Memory for Fixture {
        fn base(&self) -> u64 {
            0x140000000
        }
        fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
            let source = self
                .data
                .get(&at)
                .filter(|v| v.len() >= len)
                .map(|v| v[..len].to_vec())
                .or_else(|| {
                    self.data.iter().find_map(|(start, v)| {
                        at.checked_sub(*start).and_then(|off| {
                            let off = usize::try_from(off).ok()?;
                            v.get(off..off.checked_add(len)?).map(|v| v.to_vec())
                        })
                    })
                })
                .context("Missing arena fixture address")?;
            let mut seen = self.reads.borrow_mut();
            let n = seen.entry(at).or_default();
            *n += 1;
            let mut result = source;
            if self.torn == Some(at) && *n > 1 {
                result[0] ^= 1;
            }
            Ok(result)
        }
        fn checkpoint(&self) -> Result<()> {
            ensure!(!self.cancel.get(), "Fixture cancelled");
            Ok(())
        }
    }
    impl Inspect for Fixture {
        fn region(&self, at: u64) -> Result<Region> {
            self.regions
                .iter()
                .find(|r| r.contains(at, 1))
                .copied()
                .context("Missing fixture region")
        }
    }
    impl Fixture {
        fn put(&mut self, at: u64, data: &[u8]) {
            self.data.insert(at, data.to_vec());
        }
        fn row(&mut self, index: u64, id: u16, file: &str) {
            let mut r = vec![0; 0x794];
            let floor = format!("{file}_floor");
            r[0x7e..0x7e + file.len()].copy_from_slice(file.as_bytes());
            r[0xa4..0xa4 + floor.len()].copy_from_slice(floor.as_bytes());
            r[0x792..0x794].copy_from_slice(&id.to_le_bytes());
            self.put(0x300001000 + index * STRIDE, &r);
        }
    }
    fn fixture() -> Fixture {
        let mut f = Fixture {
            data: blacktop_native::selection_memory_fixture(),
            regions: vec![],
            cancel: Cell::new(false),
            torn: None,
            reads: RefCell::new(HashMap::new()),
        };
        for &(rva, bytes) in ANCHORS {
            f.put(f.base() + rva, bytes);
        }
        f.put(f.base() + MAPPING, &STOCK);
        f.regions.push(Region {
            start: (f.base() + MAPPING) & !4095,
            len: 4096,
            allocation: f.base(),
            kind: 0x1000000,
            protection: 2,
            committed: true,
        });
        f.regions.push(Region {
            start: 0x300000000,
            len: 0x100000,
            allocation: 0x300000000,
            kind: 0x20000,
            protection: 4,
            committed: true,
        });
        f.put(f.base() + ROSTER, &0x280000000u64.to_le_bytes());
        f.put(0x2800000a8, &0x300000050u64.to_le_bytes());
        let header = [3u64.to_le_bytes(), 0x300001000u64.to_le_bytes()].concat();
        f.put(0x3000000b0, &header);
        f.row(0, 700, "arena_700_int");
        f.row(1, 0, "arena_000_int");
        f.row(2, 1, "arena_001_int");
        f
    }
    #[test]
    fn exact_catalog_ids_exclusions_mapping_and_template_fields() {
        assert_eq!(choice("default").unwrap().id, "default");
        assert!(choice("arena_reccenter_lobbyint").is_err());
        assert!(choice("1976–77 / The Spectrum").is_err());
        assert!(choice("../arena_700_int").is_err());
        for c in choices().iter().filter(|c| !c.disabled) {
            let selected = choice(&c.id).unwrap();
            let m = mapping(selected.arena_id).unwrap();
            assert_eq!(&m[8..], &STOCK[8..]);
            if c.id != "default" {
                assert_eq!(&m[..4], &m[4..8]);
            }
            if c.template_arena_id.is_some() {
                let fields =
                    template_fields(c.file.as_deref().unwrap(), c.floor_file.as_deref().unwrap())
                        .unwrap();
                assert_eq!(fields.len(), 78);
                assert_eq!(ascii(&fields[..38]).unwrap(), *c.file.as_ref().unwrap());
                assert_eq!(
                    ascii(&fields[38..]).unwrap(),
                    *c.floor_file.as_ref().unwrap()
                );
            }
        }
    }
    #[test]
    fn filenames_ids_and_region_overflow_fail_closed() {
        for (a, b) in [
            ("../arena", "../arena_floor"),
            ("ARENA", "ARENA_floor"),
            ("arena", "floor"),
            ("", "_floor"),
            ("arena\0", "arena\0_floor"),
        ] {
            assert!(template_fields(a, b).is_err());
        }
        assert!(template_fields(&"a".repeat(34), &format!("{}_floor", "a".repeat(34))).is_err());
        assert_eq!(mapping(Some(0)).unwrap()[..8], [0; 8]);
        assert!(mapping(Some(65536)).is_err());
        let r = Region {
            start: 65536,
            len: 16,
            allocation: 65536,
            kind: 0x20000,
            protection: 4,
            committed: true,
        };
        assert!(r.contains(65536, 16));
        assert!(!r.contains(65536, 17));
        assert!(!r.contains(u64::MAX, 4));
        assert!(
            !Region {
                protection: 0x104,
                ..r
            }
            .contains(65536, 4)
        );
    }
    #[test]
    fn selection_capture_accepts_original_zero_id_and_template_without_writes() {
        let f = fixture();
        let c = choice("arena_000_int").unwrap();
        let s = capture(&f, &c, &STOCK).unwrap();
        assert_eq!(s.record.unwrap().id, 0);
        let c = choice("arena_brickleyfacility_int").unwrap();
        let s = capture(&f, &c, &STOCK).unwrap();
        assert_eq!(s.record.unwrap().id, 700);
        assert!(
            capture(&f, &choice("default").unwrap(), &STOCK)
                .unwrap()
                .record
                .is_none()
        );
        assert_eq!(f.data[&(f.base() + MAPPING)], STOCK);
    }
    #[test]
    fn wrong_mode_signature_region_directory_duplicate_and_torn_snapshots_fail() {
        let c = choice("arena_000_int").unwrap();
        let mut f = fixture();
        f.put(f.base() + 0xAC77660 + 64, &0x6509837Bu32.to_le_bytes());
        assert!(capture(&f, &c, &STOCK).is_err());
        let mut f = fixture();
        f.put(f.base() + ANCHORS[0].0, &[0; 7]);
        assert!(capture(&f, &c, &STOCK).is_err());
        let mut f = fixture();
        f.regions[0].protection = 4;
        assert!(capture(&f, &c, &STOCK).is_err());
        let mut f = fixture();
        f.regions[1].allocation += 0x1000;
        assert!(capture(&f, &c, &STOCK).is_err());
        let mut f = fixture();
        f.row(2, 700, "arena_700_int");
        assert!(capture(&f, &c, &STOCK).is_err());
        let mut f = fixture();
        f.row(2, 0, "arena_000_int");
        assert!(capture(&f, &c, &STOCK).is_err());
        let mut f = fixture();
        f.torn = Some(f.base() + MAPPING);
        assert!(capture(&f, &c, &STOCK).is_err());
        let mut f = fixture();
        f.torn = Some(0x3000000b0);
        assert!(capture(&f, &c, &STOCK).is_err());
        let f = fixture();
        f.cancel.set(true);
        assert!(capture(&f, &c, &STOCK).is_err());
    }
    #[test]
    fn forged_catalog_choices_and_stale_native_record_leases_are_rejected() {
        let mut f = fixture();
        let c = choice("arena_000_int").unwrap();
        let mut forged = c.clone();
        forged.file = Some("arena_001_int".into());
        assert!(capture(&f, &forged, &STOCK).is_err());
        let s = capture(&f, &c, &STOCK).unwrap();
        let r = s.record.unwrap();
        revalidate(&f, &r, &r.fields).unwrap();
        let mut forged = r.clone();
        forged.address += 4;
        assert!(revalidate(&f, &forged, &r.fields).is_err());
        f.row(1, 0, "arena_changed");
        assert!(revalidate(&f, &r, &r.fields).is_err());
    }
    #[test]
    fn original_arena_tool_pure_helpers_and_image_anchors_match() {
        let data: serde_json::Value =
            serde_json::from_str(include_str!("../assets/arena-plan-regressions.json")).unwrap();
        let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02X}")).collect::<String>();
        assert_eq!(data["gameAccess"], false);
        assert_eq!(data["stockHex"], hex(&STOCK));
        for c in data["cases"].as_array().unwrap() {
            let result = template_fields(
                c["file"].as_str().unwrap_or(""),
                c["floor"].as_str().unwrap_or(""),
            );
            assert_eq!(result.is_ok(), c["accepted"].as_bool().unwrap(), "{}", c);
            if let Ok(bytes) = result {
                assert_eq!(c["bytesHex"], hex(&bytes));
            }
        }
        let values = &data["builds"][1]["values"];
        assert_eq!(values["MappingRva"], MAPPING);
        assert_eq!(values["RosterRootRva"], ROSTER);
        let source = include_str!("../../nba2k27-mod-launcher/arena-helper/ArenaTool.cs");
        for &(rva, bytes) in ANCHORS {
            assert!(
                source.contains(&format!(
                    "Anchor(0x{rva:X},\"{}\")",
                    hex(bytes).to_ascii_lowercase()
                )),
                "Arena anchor 0x{rva:X} differs from the original"
            );
        }
    }
}
