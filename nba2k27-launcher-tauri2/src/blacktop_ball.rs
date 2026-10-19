//! Data-only normal-ball filename policy and native record inspection.
//! No rejected ball hooks, counter edits, allocations or native engine calls.
#![allow(dead_code)]
use crate::blacktop_arena::Inspect;
use crate::blacktop_native::{self, Memory};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{io::Read, sync::OnceLock};
pub(crate) const NORMAL: u32 = 0xCB5691BE;
pub(crate) const DEFAULT: u32 = 0xDC1BFEC5;
pub(crate) const NORMAL_FILE: &str = "ball_gameplay_official_NBA_NBA_Official.iff";
pub(crate) const STRIDE: usize = 0x1A0;
pub(crate) const FIELD: usize = 0x9C;
const REGISTRY: u64 = 0xAA81400;
const HOLDER: u64 = 0xAA51F68;
const RECORD_OFFSET: u64 = 0xF4D28;
const REFERENCES: u64 = 0xAC76B98;
const RENDERER: u64 = 0xAA7B078;
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Choice {
    pub id: String,
    pub name: String,
}
pub(crate) fn choices() -> &'static [Choice] {
    static C: OnceLock<Vec<Choice>> = OnceLock::new();
    C.get_or_init(|| {
        #[derive(Deserialize)]
        struct Catalog {
            ball: Vec<Choice>,
        }
        let mut bytes = Vec::new();
        flate2::read::GzDecoder::new(&include_bytes!("../assets/choices.json.gz")[..])
            .take(4 * 1024 * 1024)
            .read_to_end(&mut bytes)
            .expect("Bundled choices");
        serde_json::from_slice::<Catalog>(&bytes)
            .expect("Bundled ball catalog")
            .ball
    })
}
pub(crate) fn choice(id: &str) -> Result<&'static Choice> {
    let mut found = choices().iter().filter(|c| c.id == id);
    let c = found
        .next()
        .context("Ball asset ID is not in the enabled catalog")?;
    ensure!(
        found.next().is_none()
            && (id == "current" || id.strip_prefix("balls/").is_some_and(valid_filename)),
        "Ball ID is ambiguous or invalid"
    );
    Ok(c)
}
pub(crate) fn canonical(id: u32) -> u32 {
    if id == 0 || id == DEFAULT { NORMAL } else { id }
}
pub(crate) fn current(id: &str) -> bool {
    id.eq_ignore_ascii_case("current")
        || id.eq_ignore_ascii_case(&format!("balls/{NORMAL_FILE}"))
        || id.eq_ignore_ascii_case("balls/ball_gameplay_official_default.iff")
}
pub(crate) fn valid_filename(s: &str) -> bool {
    s.len() <= 127
        && !s.contains("..")
        && s.strip_prefix("ball_")
            .and_then(|s| s.strip_suffix(".iff"))
            .is_some_and(|s| !s.is_empty())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
}
pub(crate) fn filename(record: &[u8]) -> Result<String> {
    ensure!(record.len() == STRIDE, "Invalid ball record size");
    let tail = &record[FIELD..];
    let end = tail
        .iter()
        .position(|b| *b == 0)
        .context("Unterminated ball filename")?;
    ensure!(tail[..end].is_ascii(), "Non-ASCII ball filename");
    Ok(String::from_utf8(tail[..end].to_vec())?)
}
pub(crate) fn field_len(original: &[u8], replacement: &[u8]) -> Result<usize> {
    Ok(filename(original)?.len().max(filename(replacement)?.len()) + 1)
}
pub(crate) fn replacement(original: &[u8], name: &str) -> Result<Vec<u8>> {
    ensure!(
        original.len() == STRIDE && u32::from_le_bytes(original[4..8].try_into()?) == NORMAL,
        "Unexpected normal ball record"
    );
    ensure!(valid_filename(name), "Invalid ball asset filename");
    ensure!(
        filename(original)? == NORMAL_FILE,
        "Normal ball filename was changed outside this session"
    );
    let len = NORMAL_FILE.len().max(name.len()) + 1;
    ensure!(
        FIELD + len <= STRIDE
            && original[FIELD + NORMAL_FILE.len()..FIELD + len]
                .iter()
                .all(|b| *b == 0),
        "Ball filename has no verified free space"
    );
    let mut next = original.to_vec();
    next[FIELD..FIELD + len].fill(0);
    next[FIELD..FIELD + name.len()].copy_from_slice(name.as_bytes());
    Ok(next)
}
pub(crate) fn unloaded(references: i32, resource: &[u8]) -> bool {
    references == 0 && resource.len() == 48 && resource.iter().all(|b| *b == 0)
}
pub(crate) fn observed_load(references: i32, resource: &[u8]) -> bool {
    references > 0 || resource.iter().any(|b| *b != 0)
}
pub(crate) fn selection(a: u32, b: u32, stage: u32, active: u8, kind: u8) -> bool {
    a == 0x92DF06DE && a == b && stage == 3 && active == 0 && kind == 0
}
pub(crate) fn quiet_menu(a: u32, b: u32, stage: u32, active: u8, kind: u8) -> bool {
    active == 0
        && kind == 0
        && (selection(a, b, stage, active, kind)
            || (a == b && [0x6509837B, 0x079964C0].contains(&a)))
}
pub(crate) fn restore_now(
    used: bool,
    changed: bool,
    stopping: bool,
    recovery: bool,
    main: bool,
    quiet: bool,
    references: i32,
    resource: &[u8],
) -> bool {
    (used || changed || stopping || recovery || main) && quiet && unloaded(references, resource)
}
pub(crate) fn quiet(c: &blacktop_native::ContextState, _base: u64) -> bool {
    let (a, b, stage, active, kind) = c.mode_identity();
    quiet_menu(a, b, stage, active, kind)
}
pub(crate) fn main_menu(s: &Snapshot) -> bool {
    let (a, b, _, active, kind) = s.context.mode_identity();
    a == b && [0x6509837B, 0x079964C0].contains(&a) && active == 0 && kind == 0
}
pub(crate) fn plan(lease: &Lease, next: &[u8]) -> Result<()> {
    ensure!(
        lease.index < 500
            && lease.address.checked_sub(lease.records)
                == Some(u64::from(lease.index) * STRIDE as u64)
            && lease.address >= 65536
            && lease.address < 0x7fff_0000_0000
            && lease.allocation >= 65536
            && lease.allocation <= lease.address
            && lease.holder >= 65536
            && lease.registry >= 65536
            && (1..=1024).contains(&lease.registry_count),
        "Invalid ball record lease"
    );
    ensure!(
        replacement(&lease.original, &filename(next)?)? == next,
        "Ball plan changes data outside its filename"
    );
    Ok(())
}
#[cfg(test)]
mod policy_tests {
    use super::*;
    #[test]
    fn ball_original_policy_oracle_matches_filename_scope_and_every_restore_gate() {
        let j: serde_json::Value =
            serde_json::from_str(include_str!("../assets/ball-policy-regressions.json")).unwrap();
        let hex = |s: &str| -> Vec<u8> {
            s.as_bytes()
                .chunks_exact(2)
                .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
                .collect()
        };
        for c in j["restore"].as_array().unwrap() {
            let mask = c["mask"].as_u64().unwrap();
            let mut r = [0; 48];
            if c["loaded"] == true {
                r[16] = 1;
            }
            assert_eq!(
                restore_now(
                    mask & 1 != 0,
                    mask & 2 != 0,
                    mask & 4 != 0,
                    mask & 8 != 0,
                    mask & 16 != 0,
                    c["quiet"] == true,
                    c["refs"].as_i64().unwrap() as i32,
                    &r
                ),
                c["expected"] == true
            );
        }
        for c in j["menus"].as_array().unwrap() {
            assert_eq!(
                quiet_menu(
                    c["a"].as_u64().unwrap() as u32,
                    c["b"].as_u64().unwrap() as u32,
                    c["stage"].as_u64().unwrap() as u32,
                    c["active"].as_u64().unwrap() as u8,
                    c["kind"].as_u64().unwrap() as u8
                ),
                c["quiet"] == true
            );
        }
        for c in j["replacements"].as_array().unwrap() {
            let original = hex(c["originalHex"].as_str().unwrap());
            let result = replacement(&original, c["filename"].as_str().unwrap_or(""));
            assert_eq!(result.is_ok(), c["accepted"] == true, "{c}");
            if let Ok(next) = result {
                assert_eq!(next, hex(c["replacementHex"].as_str().unwrap()));
                assert_eq!(
                    field_len(&original, &next).unwrap(),
                    c["fieldLength"].as_u64().unwrap() as usize
                );
            }
        }
    }
    #[test]
    fn ball_catalog_is_exact_and_resource_liveness_checks_every_byte() {
        assert_eq!(choices().len(), 426);
        choice("current").unwrap();
        for c in choices() {
            choice(&c.id).unwrap();
        }
        for id in ["../ball_x.iff", "balls/ball_unknown.iff", "CURRENT"] {
            assert!(choice(id).is_err());
        }
        assert_eq!(canonical(0), NORMAL);
        assert_eq!(canonical(DEFAULT), NORMAL);
        assert_eq!(canonical(123), 123);
        for i in 0..48 {
            let mut r = [0; 48];
            r[i] = 1;
            assert!(!unloaded(0, &r));
            assert!(observed_load(0, &r));
        }
        for refs in [-1, 1, 2, i32::MAX] {
            assert!(!unloaded(refs, &[0; 48]));
        }
        assert!(!unloaded(0, &[0; 47]));
        assert!(!observed_load(0, &[0; 48]));
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Lease {
    pub holder: u64,
    pub records: u64,
    pub address: u64,
    pub allocation: u64,
    pub index: u32,
    pub registry: u64,
    pub registry_count: u32,
    pub original: Vec<u8>,
}
#[derive(Clone, Debug)]
pub(crate) struct Snapshot {
    pub image: u64,
    pub context: blacktop_native::ContextState,
    pub lease: Lease,
    pub record: Vec<u8>,
    pub references: i32,
    pub resource: Vec<u8>,
    pub native_ball: u32,
}
impl Snapshot {
    pub fn native_normal(&self) -> bool {
        if self.image.checked_add(0x0791E158) == Some(self.context.root_tag) {
            // Version 2 independently selects the gameplay model. An unset
            // choice or the legacy Default alias cannot authorize changing
            // Normal: that record can belong to spectator props instead.
            self.native_ball == NORMAL
        } else {
            canonical(self.native_ball) == NORMAL
        }
    }
    pub fn can_prepare(&self) -> bool {
        let boundary = if self.image.checked_add(0x0791E158) == Some(self.context.root_tag) {
            crate::rules_v2::boundary(&self.context, self.image)
        } else {
            blacktop_native::boundary(&self.context, self.image)
        };
        boundary == "selection" && unloaded(self.references, &self.resource) && self.native_normal()
    }
    pub fn can_restore(&self) -> bool {
        let safe = if self.image.checked_add(0x0791E158) == Some(self.context.root_tag) {
            matches!(
                crate::rules_v2::boundary(&self.context, self.image),
                "selection" | "main-menu" | "quick-play"
            )
        } else {
            quiet(&self.context, self.image)
        };
        safe && unloaded(self.references, &self.resource)
    }
    pub fn owns(&self, l: &Lease, expected: &[u8]) -> Result<()> {
        ensure!(
            self.lease.holder == l.holder
                && self.lease.records == l.records
                && self.lease.address == l.address
                && self.lease.allocation == l.allocation
                && self.lease.index == l.index
                && self.lease.registry == l.registry
                && self.lease.registry_count == l.registry_count
                && self.record == expected,
            "Ball record ownership or complete contents changed; no filename write attempted"
        );
        Ok(())
    }
}
fn read(m: &impl Memory, at: u64, len: usize) -> Result<Vec<u8>> {
    ensure!(
        at >= 65536
            && len > 0
            && len <= 8192
            && at
                .checked_add(len as u64)
                .is_some_and(|end| end <= 0x7fff_ffff_ffff),
        "Invalid bounded ball read"
    );
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        m.checkpoint()?;
        let n = (len - out.len()).min(4096);
        let b = m.read(at + out.len() as u64, n)?;
        ensure!(b.len() == n, "Partial ball read");
        out.extend(b);
    }
    Ok(out)
}
fn u32_at(m: &impl Memory, at: u64) -> Result<u32> {
    Ok(u32::from_le_bytes(read(m, at, 4)?.as_slice().try_into()?))
}
fn u64_at(m: &impl Memory, at: u64) -> Result<u64> {
    Ok(u64::from_le_bytes(read(m, at, 8)?.as_slice().try_into()?))
}
pub(crate) fn validate(m: &impl Memory) -> Result<()> {
    blacktop_native::validate_build(m)?;
    // Only verify these old experiment sites. Never recreate or execute them.
    for (at, to) in [
        (0x2E40B6u64, 0x3045C50u64),
        (0x2E4444, 0x30462B0),
        (0x2E40A4, 0x257C040),
        (0x2E4432, 0x257C040),
        (0x40FEEA4, 0x257C030),
    ] {
        let delta = i32::try_from(to as i64 - at as i64 - 5)?;
        let mut expected = vec![0xE8];
        expected.extend(delta.to_le_bytes());
        ensure!(
            read(m, m.base() + at, 5)? == expected,
            "Native ball loading code changed; restart before retrying"
        );
    }
    for (at, b) in [
        (0x4138B79, &[0x41, 0x8B, 0x8E, 0x4C, 3, 0, 0][..]),
        (0x21BFED8, &[0x48, 0x81, 0xC7, 0x9C, 0, 0, 0]),
        (0x21BE198, &[0x81, 0xFF, 0xC5, 0xFE, 0x1B, 0xDC]),
        (0x21BE1A0, &[0xC7, 0x44, 0x24, 0x48, 0xBE, 0x91, 0x56, 0xCB]),
        (0x31436C0, &[0x48, 0x8D, 5, 0xA1, 0x5C, 0x6B, 4, 0xC3]),
        (
            0x31436D0,
            &[0x48, 0x89, 0xD0, 0xC7, 2, 0xC0, 0x64, 0x99, 7, 0xC3],
        ),
        (0x77F9368, b"NBA_MainMenu_PlayNow\0"),
    ] {
        ensure!(
            read(m, m.base() + at, b.len())? == b,
            "Ball asset layout or menu identity differs from the supported build"
        );
    }
    Ok(())
}
fn writable(m: &impl Inspect, at: u64, n: usize) -> Result<u64> {
    let r = m.region(at)?;
    ensure!(
        r.committed
            && r.kind == 0x20000
            && r.protection == 4
            && at >= r.start
            && at
                .checked_add(n as u64)
                .zip(r.start.checked_add(r.len))
                .is_some_and(|(end, limit)| end <= limit),
        "Ball row is not one private writable allocation"
    );
    Ok(r.allocation)
}
pub(crate) fn capture(m: &impl Inspect) -> Result<Snapshot> {
    validate(m)?;
    let before = blacktop_native::context(m)?;
    let holder = u64_at(m, m.base() + HOLDER)?;
    let records = u64_at(m, holder.checked_add(0x18).context("Invalid ball holder")?)?
        .checked_add(RECORD_OFFSET)
        .context("Ball table overflow")?;
    let registry = u64_at(m, m.base() + REGISTRY)?;
    let count = u32_at(m, m.base() + REGISTRY + 8)?;
    ensure!(
        (1..=1024).contains(&count),
        "Waiting for a stable stock ball registry"
    );
    let entries = read(m, registry, count as usize * 8)?;
    let mut found = None;
    for row in entries.chunks_exact(8) {
        if u32::from_le_bytes(row[..4].try_into()?) == NORMAL {
            ensure!(
                found.is_none(),
                "Normal ball registry identity is not unique"
            );
            found = Some(u32::from_le_bytes(row[4..].try_into()?));
        }
    }
    let index = found.context("Normal ball is absent from the live registry")?;
    ensure!(index < 500, "Invalid normal ball registry index");
    let address = records
        .checked_add(u64::from(index) * STRIDE as u64)
        .context("Ball row overflow")?;
    let allocation = writable(m, address, STRIDE)?;
    let record = read(m, address, STRIDE)?;
    ensure!(
        u32::from_le_bytes(record[4..8].try_into()?) == NORMAL,
        "Normal ball row identity differs"
    );
    let references = i32::from_le_bytes(
        read(m, m.base() + REFERENCES + 0x18 + u64::from(index) * 4, 4)?
            .as_slice()
            .try_into()?,
    );
    let resource = read(m, m.base() + RENDERER + 0x28 + u64::from(index) * 48, 48)?;
    let native_ball = u32_at(m, before.owner + 0xDA4)?;
    ensure!(
        before == blacktop_native::context(m)?
            && u64_at(m, m.base() + HOLDER)? == holder
            && u64_at(m, holder + 0x18)?.checked_add(RECORD_OFFSET) == Some(records)
            && u64_at(m, m.base() + REGISTRY)? == registry
            && u32_at(m, m.base() + REGISTRY + 8)? == count
            && read(m, registry, count as usize * 8)? == entries,
        "Ball context changed during observation"
    );
    Ok(Snapshot {
        image: m.base(),
        context: before,
        lease: Lease {
            holder,
            records,
            address,
            allocation,
            index,
            registry,
            registry_count: count,
            original: record.clone(),
        },
        record,
        references,
        resource,
        native_ball,
    })
}
pub(crate) fn resolve(m: &impl Inspect, s: &Snapshot, id: &str) -> Result<(u32, String)> {
    choice(id)?;
    ensure!(
        !current(id),
        "Currently installed ball needs no replacement"
    );
    let wanted = id
        .strip_prefix("balls/")
        .context("Invalid catalog ball ID")?;
    let entries = read(m, s.lease.registry, s.lease.registry_count as usize * 8)?;
    let mut found = None;
    for entry in entries.chunks_exact(8) {
        let model = u32::from_le_bytes(entry[..4].try_into()?);
        let index = u32::from_le_bytes(entry[4..].try_into()?);
        if index >= 500 {
            continue;
        }
        let at = s
            .lease
            .records
            .checked_add(u64::from(index) * STRIDE as u64)
            .context("Ball row overflow")?;
        let Ok(record) = read(m, at, STRIDE) else {
            continue;
        };
        if u32::from_le_bytes(record[4..8].try_into()?) != model {
            continue;
        }
        let Ok(name) = filename(&record) else {
            continue;
        };
        if !name.eq_ignore_ascii_case(wanted) {
            continue;
        }
        ensure!(
            valid_filename(&name) && model != 0 && found.is_none(),
            "Selected ball does not have one unique live model"
        );
        found = Some((model, name));
    }
    let result = found.context("This stock ball is not available in the live game registry")?;
    let after = capture(m)?;
    after.owns(&s.lease, &s.record)?;
    ensure!(
        after.context == s.context
            && after.native_ball == s.native_ball
            && read(m, s.lease.registry, s.lease.registry_count as usize * 8)? == entries,
        "Ball source changed during resolution"
    );
    ensure!(
        result.0 != NORMAL,
        "Selected ball already uses the normal model"
    );
    Ok(result)
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use crate::blacktop_arena::Region;
    use std::{
        cell::{Cell, RefCell},
        collections::HashMap,
    };
    pub struct Fixture {
        pub data: HashMap<u64, Vec<u8>>,
        pub protection: u32,
        pub torn: Option<u64>,
        reads: RefCell<HashMap<u64, u32>>,
        pub cancelled: Cell<bool>,
    }
    impl Memory for Fixture {
        fn base(&self) -> u64 {
            0x140000000
        }
        fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
            let mut value = self
                .data
                .get(&at)
                .filter(|v| v.len() >= len)
                .map(|v| v[..len].to_vec())
                .or_else(|| {
                    self.data.iter().find_map(|(start, data)| {
                        at.checked_sub(*start).and_then(|offset| {
                            data.get(offset as usize..(offset as usize).checked_add(len)?)
                                .map(|v| v.to_vec())
                        })
                    })
                })
                .context("Missing ball fixture address")?;
            let mut reads = self.reads.borrow_mut();
            let n = reads.entry(at).or_default();
            *n += 1;
            if self.torn == Some(at) && *n > 1 {
                value[0] ^= 1;
            }
            Ok(value)
        }
        fn checkpoint(&self) -> Result<()> {
            ensure!(!self.cancelled.get(), "Ball fixture cancelled");
            Ok(())
        }
    }
    impl Inspect for Fixture {
        fn region(&self, at: u64) -> Result<Region> {
            ensure!(
                (0x300000000..0x300100000).contains(&at),
                "Not fixture private data"
            );
            Ok(Region {
                start: 0x300000000,
                len: 0x100000,
                allocation: 0x300000000,
                kind: 0x20000,
                protection: self.protection,
                committed: true,
            })
        }
    }
    impl Fixture {
        pub fn put(&mut self, at: u64, bytes: &[u8]) {
            self.data.insert(at, bytes.to_vec());
        }
        pub fn mode(&mut self, hash: u32, stage: u32) {
            let a = u32_at(self, self.base() + 0xAC77E28).unwrap();
            let b = u32_at(self, self.base() + 0xAC77E24).unwrap();
            for i in [a, b] {
                self.put(
                    self.base() + 0xAC77660 + u64::from(i) * 64,
                    &hash.to_le_bytes(),
                );
            }
            self.put(self.base() + 0xAEB69D0 + 0xEA8, &stage.to_le_bytes());
        }
    }
    pub fn id() -> String {
        choices()
            .iter()
            .find(|c| !current(&c.id))
            .unwrap()
            .id
            .clone()
    }
    pub fn original() -> Vec<u8> {
        let mut row = vec![0; STRIDE];
        for i in 0..FIELD {
            row[i] = (i as u8).wrapping_add(3);
        }
        row[4..8].copy_from_slice(&NORMAL.to_le_bytes());
        row[FIELD..FIELD + NORMAL_FILE.len()].copy_from_slice(NORMAL_FILE.as_bytes());
        row[STRIDE - 4] = 98;
        row
    }
    pub fn fixture() -> Fixture {
        let mut f = Fixture {
            data: blacktop_native::selection_memory_fixture(),
            protection: 4,
            torn: None,
            reads: RefCell::new(HashMap::new()),
            cancelled: Cell::new(false),
        };
        for (at, to) in [
            (0x2E40B6u64, 0x3045C50u64),
            (0x2E4444, 0x30462B0),
            (0x2E40A4, 0x257C040),
            (0x2E4432, 0x257C040),
            (0x40FEEA4, 0x257C030),
        ] {
            let bytes = [
                vec![0xE8],
                i32::try_from(to as i64 - at as i64 - 5)
                    .unwrap()
                    .to_le_bytes()
                    .to_vec(),
            ]
            .concat();
            f.put(f.base() + at, &bytes);
        }
        for (at, b) in [
            (0x4138B79, &[0x41, 0x8B, 0x8E, 0x4C, 3, 0, 0][..]),
            (0x21BFED8, &[0x48, 0x81, 0xC7, 0x9C, 0, 0, 0]),
            (0x21BE198, &[0x81, 0xFF, 0xC5, 0xFE, 0x1B, 0xDC]),
            (0x21BE1A0, &[0xC7, 0x44, 0x24, 0x48, 0xBE, 0x91, 0x56, 0xCB]),
            (0x31436C0, &[0x48, 0x8D, 5, 0xA1, 0x5C, 0x6B, 4, 0xC3]),
            (
                0x31436D0,
                &[0x48, 0x89, 0xD0, 0xC7, 2, 0xC0, 0x64, 0x99, 7, 0xC3],
            ),
            (0x77F9368, b"NBA_MainMenu_PlayNow\0"),
        ] {
            f.put(f.base() + at, b);
        }
        f.put(f.base() + HOLDER, &0x280000000u64.to_le_bytes());
        f.put(0x280000018, &(0x300000000u64 - RECORD_OFFSET).to_le_bytes());
        f.put(f.base() + REGISTRY, &0x320000000u64.to_le_bytes());
        f.put(f.base() + REGISTRY + 8, &2u32.to_le_bytes());
        f.put(
            0x320000000,
            &[
                NORMAL.to_le_bytes(),
                0u32.to_le_bytes(),
                0x11223344u32.to_le_bytes(),
                1u32.to_le_bytes(),
            ]
            .concat(),
        );
        f.put(0x300000000, &original());
        let mut source = vec![0; STRIDE];
        source[4..8].copy_from_slice(&0x11223344u32.to_le_bytes());
        let name = id();
        let name = name.strip_prefix("balls/").unwrap();
        source[FIELD..FIELD + name.len()].copy_from_slice(name.as_bytes());
        f.put(0x300000000 + STRIDE as u64, &source);
        f.put(f.base() + REFERENCES + 0x18, &0i32.to_le_bytes());
        f.put(f.base() + RENDERER + 0x28, &[0; 48]);
        let c = blacktop_native::context(&f).unwrap();
        f.put(c.owner + 0xDA4, &0u32.to_le_bytes());
        f
    }
}
#[cfg(test)]
mod native_tests {
    use super::{test_support::*, *};
    #[test]
    fn selection_source_resolution_and_exact_filename_plan_are_bounded() {
        let f = fixture();
        let s = capture(&f).unwrap();
        assert!(s.can_prepare());
        let (model, name) = resolve(&f, &s, &id()).unwrap();
        assert_eq!(model, 0x11223344);
        let next = replacement(&s.record, &name).unwrap();
        plan(&s.lease, &next).unwrap();
        assert_eq!(&next[..FIELD], &s.record[..FIELD]);
        assert_eq!(next[STRIDE - 4], 98);
        assert!(resolve(&f, &s, "balls/ball_unknown.iff").is_err());
    }
    #[test]
    fn loaded_references_each_resource_byte_other_native_ball_and_menu_prevent_prepare() {
        for at in 0..48 {
            let mut f = fixture();
            let mut resource = [0; 48];
            resource[at] = 1;
            f.put(f.base() + RENDERER + 0x28, &resource);
            assert!(!capture(&f).unwrap().can_prepare());
        }
        let mut f = fixture();
        f.put(f.base() + REFERENCES + 0x18, &(-1i32).to_le_bytes());
        assert!(!capture(&f).unwrap().can_prepare());
        let mut f = fixture();
        f.mode(0x079964C0, 3);
        let s = capture(&f).unwrap();
        assert!(!s.can_prepare());
        assert!(s.can_restore() && main_menu(&s));
        let mut f = fixture();
        let s = capture(&f).unwrap();
        f.put(s.context.owner + 0xDA4, &123u32.to_le_bytes());
        assert!(!capture(&f).unwrap().can_prepare());
    }
    #[test]
    fn wrong_build_code_private_protection_duplicate_registry_and_torn_context_fail() {
        for at in [0x2E40B6, 0x21BFED8, 0x31436C0, 0x77F9368] {
            let mut f = fixture();
            f.put(f.base() + at, &[0; 32]);
            assert!(capture(&f).is_err());
        }
        let mut f = fixture();
        f.protection = 0x40;
        assert!(capture(&f).is_err());
        let mut f = fixture();
        f.put(
            0x320000000,
            &[
                NORMAL.to_le_bytes(),
                0u32.to_le_bytes(),
                NORMAL.to_le_bytes(),
                1u32.to_le_bytes(),
            ]
            .concat(),
        );
        assert!(capture(&f).is_err());
        let mut f = fixture();
        f.torn = Some(f.base() + REGISTRY);
        assert!(capture(&f).is_err());
        let f = fixture();
        f.cancelled.set(true);
        assert!(capture(&f).is_err());
    }
    #[test]
    fn record_and_directory_ownership_cannot_be_reused_or_modify_metadata() {
        let f = fixture();
        let s = capture(&f).unwrap();
        let mut lease = s.lease.clone();
        lease.registry += 8;
        assert!(s.owns(&lease, &s.record).is_err());
        let mut lease = s.lease.clone();
        lease.allocation += 4096;
        assert!(s.owns(&lease, &s.record).is_err());
        let mut next = replacement(&s.record, id().strip_prefix("balls/").unwrap()).unwrap();
        next[8] ^= 1;
        assert!(plan(&s.lease, &next).is_err());
        let mut lease = s.lease.clone();
        lease.address += STRIDE as u64;
        assert!(plan(&lease, &s.record).is_err());
    }
}
