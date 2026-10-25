//! Version 2 native normal-ball filename substitution. No process discovery,
//! memory writes, allocation, code hooks, loader calls or counter/model edits.
//! The runtime journals before applying the sole bounded filename edit, using
//! an exact retained process identity and four spaced unloaded samples.
#![allow(dead_code)]
use crate::{
    blacktop_arena::Inspect,
    blacktop_ball as ball,
    blacktop_native::{ContextState, Memory},
    rules_v2,
};
use anyhow::{Context, Result, ensure};
pub(crate) const REGISTRY: u64 = 0x0AB12880;
pub(crate) const HOLDER: u64 = 0x0AAE33E8;
pub(crate) const RENDERER: u64 = 0x0AB0C4F8;
pub(crate) const REFERENCES: u64 = 0x0AD068E8;
const RECORD_OFFSET: u64 = 0xF4D28;
const SHA: &str = "B673EB04593D6D5A4DFB9E270178E04C017BD9F277514D256EA98FFB792ED27B";
fn at(m: &impl Memory, rva: u64) -> Result<u64> {
    m.base().checked_add(rva).context("Ball RVA overflow")
}
fn read(m: &impl Memory, address: u64, len: usize) -> Result<Vec<u8>> {
    ensure!(
        address >= 0x10000
            && (1..=8192).contains(&len)
            && address
                .checked_add(len as u64)
                .is_some_and(|end| end <= 0x7fff00000000),
        "Invalid bounded ball read"
    );
    let mut bytes = Vec::with_capacity(len);
    for offset in (0..len).step_by(4096) {
        m.checkpoint()?;
        let count = (len - offset).min(4096);
        let next = m.read(address + offset as u64, count)?;
        ensure!(next.len() == count, "Partial ball read");
        bytes.extend(next);
    }
    Ok(bytes)
}
fn d(m: &impl Memory, address: u64) -> Result<u32> {
    Ok(u32::from_le_bytes(
        read(m, address, 4)?.as_slice().try_into()?,
    ))
}
fn q(m: &impl Memory, address: u64) -> Result<u64> {
    Ok(u64::from_le_bytes(
        read(m, address, 8)?.as_slice().try_into()?,
    ))
}
fn hex(s: &str) -> Result<Vec<u8>> {
    ensure!(
        s.len() % 2 == 0 && s.len() <= 512,
        "Invalid ball anchor hex"
    );
    s.as_bytes()
        .chunks_exact(2)
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}
fn catalog() -> Result<serde_json::Value> {
    let value: serde_json::Value =
        serde_json::from_str(include_str!("../assets/ball-v2-anchors.json"))?;
    ensure!(
        value["schema"] == "nba2k27-v2-normal-ball-anchors-v1"
            && value["sha256"] == SHA
            && value["registryRva"].as_u64() == Some(REGISTRY)
            && value["holderRva"].as_u64() == Some(HOLDER)
            && value["rendererRva"].as_u64() == Some(RENDERER)
            && value["referencesRva"].as_u64() == Some(REFERENCES)
            && value["recordOffset"].as_u64() == Some(RECORD_OFFSET)
            && value["recordStride"].as_u64() == Some(ball::STRIDE as u64)
            && value["filenameOffset"].as_u64() == Some(ball::FIELD as u64),
        "Invalid Version 2 ball reference"
    );
    Ok(value)
}
pub(crate) fn verify_profile(m: &impl Memory) -> Result<()> {
    rules_v2::verify_profile(m)?;
    let value = catalog()?;
    let anchors = value["anchors"]
        .as_array()
        .context("Missing ball anchors")?;
    ensure!(anchors.len() == 16, "Ball proof count changed");
    for anchor in anchors {
        let rva = anchor["rva"]
            .as_u64()
            .context("Missing ball anchor address")?;
        let bytes = hex(anchor["hex"]
            .as_str()
            .context("Missing ball anchor bytes")?)?;
        ensure!(
            (0x1000..0x5D00000).contains(&rva) && read(m, at(m, rva)?, bytes.len())? == bytes,
            "Version 2 ball native layout changed: {}",
            anchor["label"]
        );
    }
    m.checkpoint()
}
fn private_rw(m: &impl Inspect, address: u64, len: usize) -> Result<u64> {
    let r = m.region(address)?;
    ensure!(
        r.committed
            && r.kind == 0x20000
            && r.protection == 4
            && address >= r.start
            && address
                .checked_add(len as u64)
                .zip(r.start.checked_add(r.len))
                .is_some_and(|(end, limit)| end <= limit)
            && r.allocation >= 0x10000
            && r.allocation <= address,
        "Ball record is not one committed private RW/non-executable allocation"
    );
    Ok(r.allocation)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub image: u64,
    pub context: ContextState,
    pub lease: ball::Lease,
    pub record: Vec<u8>,
    pub references: i32,
    pub resource: Vec<u8>,
    pub native_ball: u32,
    entries: Vec<u8>,
}
impl Snapshot {
    pub(crate) fn can_prepare(&self) -> bool {
        rules_v2::boundary(&self.context, self.image) == "selection"
            && ball::unloaded(self.references, &self.resource)
            && self.native_ball == ball::NORMAL
    }
    pub(crate) fn can_restore(&self) -> bool {
        matches!(
            rules_v2::boundary(&self.context, self.image),
            "selection" | "main-menu" | "quick-play"
        ) && ball::unloaded(self.references, &self.resource)
    }
    pub(crate) fn owns(&self, lease: &ball::Lease, expected: &[u8]) -> Result<()> {
        ensure!(
            self.lease.holder == lease.holder
                && self.lease.records == lease.records
                && self.lease.address == lease.address
                && self.lease.allocation == lease.allocation
                && self.lease.index == lease.index
                && self.lease.registry == lease.registry
                && self.lease.registry_count == lease.registry_count
                && self.record == expected,
            "Ball complete record or retained ownership changed"
        );
        Ok(())
    }
}
pub(crate) fn capture(m: &impl Inspect) -> Result<Snapshot> {
    verify_profile(m)?;
    let context = rules_v2::context(m)?;
    let holder = q(m, at(m, HOLDER)?)?;
    ensure!(
        holder >= 0x10000 && holder % 8 == 0,
        "Ball record holder is initializing"
    );
    let records = q(m, holder.checked_add(0x18).context("Ball holder overflow")?)?
        .checked_add(RECORD_OFFSET)
        .context("Ball records overflow")?;
    let registry = q(m, at(m, REGISTRY)?)?;
    let count = d(m, at(m, REGISTRY + 8)?)?;
    ensure!(
        registry >= 0x10000 && registry % 8 == 0 && (1..=1024).contains(&count),
        "Ball registry is initializing"
    );
    let entries = read(m, registry, count as usize * 8)?;
    let mut normal = None;
    for entry in entries.chunks_exact(8) {
        if u32::from_le_bytes(entry[..4].try_into()?) == ball::NORMAL {
            ensure!(
                normal.is_none(),
                "Normal ball registry identity is ambiguous"
            );
            normal = Some(u32::from_le_bytes(entry[4..].try_into()?));
        }
    }
    let index = normal.context("Normal ball is absent from the live registry")?;
    ensure!(index < 500, "Normal ball registry index changed");
    let address = records
        .checked_add(index as u64 * ball::STRIDE as u64)
        .context("Ball record address overflow")?;
    let allocation = private_rw(m, address, ball::STRIDE)?;
    let record = read(m, address, ball::STRIDE)?;
    ensure!(
        u32::from_le_bytes(record[4..8].try_into()?) == ball::NORMAL,
        "Normal ball record ID changed"
    );
    let ref_at = at(m, REFERENCES + 0x18 + index as u64 * 4)?;
    let resource_at = at(m, RENDERER + 0x28 + index as u64 * 48)?;
    let references = d(m, ref_at)? as i32;
    let resource = read(m, resource_at, 48)?;
    let native_ball = d(m, context.owner + 0xDA4)?;
    ensure!(
        context == rules_v2::context(m)?
            && holder == q(m, at(m, HOLDER)?)?
            && q(m, holder + 0x18)?.checked_add(RECORD_OFFSET) == Some(records)
            && registry == q(m, at(m, REGISTRY)?)?
            && count == d(m, at(m, REGISTRY + 8)?)?
            && entries == read(m, registry, count as usize * 8)?
            && record == read(m, address, ball::STRIDE)?
            && references == d(m, ref_at)? as i32
            && resource == read(m, resource_at, 48)?
            && native_ball == d(m, context.owner + 0xDA4)?
            && allocation == private_rw(m, address, ball::STRIDE)?
            && context == rules_v2::context(m)?,
        "Ball state became torn during capture"
    );
    m.checkpoint()?;
    Ok(Snapshot {
        image: m.base(),
        context,
        lease: ball::Lease {
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
        entries,
    })
}
#[derive(Clone, Debug)]
pub(crate) struct Resolved {
    pub model: u32,
    pub index: u32,
    pub address: u64,
    pub record: Vec<u8>,
    pub filename: String,
}
pub(crate) fn resolve(m: &impl Inspect, snapshot: &Snapshot, id: &str) -> Result<Resolved> {
    resolve_excluding(m, snapshot, id, None)
}
/// Version 2 court-record route: once the court record names the chosen file,
/// that record must not count as a second catalog source for the same name.
pub(crate) fn resolve_excluding(
    m: &impl Inspect,
    snapshot: &Snapshot,
    id: &str,
    exclude: Option<u32>,
) -> Result<Resolved> {
    ball::choice(id)?;
    ensure!(
        !ball::current(id),
        "Currently installed ball needs no substitution"
    );
    let wanted = id
        .strip_prefix("balls/")
        .context("Invalid catalog ball ID")?;
    let entries = read(
        m,
        snapshot.lease.registry,
        snapshot.lease.registry_count as usize * 8,
    )?;
    ensure!(
        entries == snapshot.entries,
        "Ball registry changed before resolution"
    );
    let mut found = None;
    // One capture-local cache only: never reuse live records between samples
    // or publications. Eight contiguous rows fit below one bounded 4096-byte
    // read, avoiding hundreds of small native syscalls per registry walk.
    // A sparse/unreadable span falls back to the original per-row read.
    let mut blocks = std::collections::BTreeMap::new();
    for entry in entries.chunks_exact(8) {
        m.checkpoint()?;
        let model = u32::from_le_bytes(entry[..4].try_into()?);
        let index = u32::from_le_bytes(entry[4..].try_into()?);
        if index >= 500 {
            continue;
        }
        let address = snapshot
            .lease
            .records
            .checked_add(index as u64 * ball::STRIDE as u64)
            .context("Ball source overflow")?;
        let Ok(record) = source_record(m, snapshot.lease.records, index, &mut blocks) else {
            continue;
        };
        if u32::from_le_bytes(record[4..8].try_into()?) != model || Some(model) == exclude {
            continue;
        }
        let Ok(filename) = ball::filename(&record) else {
            continue;
        };
        if !filename.eq_ignore_ascii_case(wanted) {
            continue;
        }
        ensure!(
            model != 0
                && model != ball::NORMAL
                && found.is_none()
                && ball::valid_filename(&filename),
            "Selected ball is missing, ambiguous or already the normal model"
        );
        ensure!(
            private_rw(m, address, ball::STRIDE)? == snapshot.lease.allocation,
            "Selected ball left the normal record allocation"
        );
        found = Some(Resolved {
            model,
            index,
            address,
            record,
            filename,
        });
    }
    let source = found.context("This ball is not uniquely available in the live registry")?;
    ensure!(
        capture(m)? == *snapshot && read(m, source.address, ball::STRIDE)? == source.record,
        "Ball registry, source or context changed during resolution"
    );
    Ok(source)
}
type RecordBlocks = std::collections::BTreeMap<u32, Option<Vec<u8>>>;
fn source_record(
    m: &impl Memory,
    records: u64,
    index: u32,
    blocks: &mut RecordBlocks,
) -> Result<Vec<u8>> {
    ensure!(index < 500, "Ball source index escaped registry bounds");
    const ROWS: u32 = 8;
    let first = index / ROWS * ROWS;
    let offset = (index - first) as usize * ball::STRIDE;
    let address = records
        .checked_add(first as u64 * ball::STRIDE as u64)
        .context("Ball source block overflow")?;
    let block = blocks
        .entry(first)
        .or_insert_with(|| read(m, address, (500 - first).min(ROWS) as usize * ball::STRIDE).ok());
    if let Some(bytes) = block {
        return Ok(bytes[offset..offset + ball::STRIDE].to_vec());
    }
    read(
        m,
        records
            .checked_add(index as u64 * ball::STRIDE as u64)
            .context("Ball source address overflow")?,
        ball::STRIDE,
    )
}
fn manifest_has(bytes: &[u8], filename: &str) -> Result<()> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= 64 * 1024 * 1024,
        "Ball manifest size is invalid"
    );
    let text = std::str::from_utf8(bytes)?;
    let wanted = format!("balls/{filename}");
    ensure!(
        text.lines().any(|line| line
            .split(',')
            .next()
            .is_some_and(|path| path.replace('\\', "/").eq_ignore_ascii_case(&wanted))),
        "Selected ball is absent from the stock manifest"
    );
    Ok(())
}
#[derive(Clone, Debug)]
pub(crate) struct Edit {
    pub at: u64,
    pub before: Vec<u8>,
    pub after: Vec<u8>,
}
#[derive(Clone, Debug)]
pub(crate) struct Plan {
    pub selected: String,
    pub original: Snapshot,
    pub source: Resolved,
    pub replacement: Vec<u8>,
    pub edit: Edit,
}
pub(crate) fn prepare(
    m: &impl Inspect,
    snapshot: &Snapshot,
    id: &str,
    manifest: &[u8],
) -> Result<Plan> {
    ensure!(
        snapshot.can_prepare(),
        "Wait for unloaded Blacktop selection with the native normal/default ball"
    );
    let source = resolve(m, snapshot, id)?;
    manifest_has(manifest, &source.filename)?;
    let replacement = ball::replacement(&snapshot.record, &source.filename)?;
    ball::plan(&snapshot.lease, &replacement)?;
    let len = ball::field_len(&snapshot.record, &replacement)?;
    let edit = Edit {
        at: snapshot.lease.address + ball::FIELD as u64,
        before: snapshot.record[ball::FIELD..ball::FIELD + len].to_vec(),
        after: replacement[ball::FIELD..ball::FIELD + len].to_vec(),
    };
    Ok(Plan {
        selected: id.into(),
        original: snapshot.clone(),
        source,
        replacement,
        edit,
    })
}
impl Plan {
    fn validate_shape(&self) -> Result<()> {
        ball::choice(&self.selected)?;
        ensure!(
            !ball::current(&self.selected),
            "Current ball has no replacement plan"
        );
        ball::plan(&self.original.lease, &self.replacement)?;
        ensure!(
            self.original.record == self.original.lease.original
                && self.source.record.len() == ball::STRIDE
                && self.source.model != 0
                && self.source.model != ball::NORMAL
                && self.source.index < 500
                && self
                    .original
                    .lease
                    .records
                    .checked_add(self.source.index as u64 * ball::STRIDE as u64)
                    == Some(self.source.address)
                && u32::from_le_bytes(self.source.record[4..8].try_into()?) == self.source.model
                && ball::filename(&self.source.record)? == self.source.filename
                && self
                    .selected
                    .strip_prefix("balls/")
                    .is_some_and(|name| name.eq_ignore_ascii_case(&self.source.filename))
                && ball::filename(&self.replacement)? == self.source.filename,
            "Ball selected source or original ownership changed"
        );
        ensure!(
            self.original
                .entries
                .chunks_exact(8)
                .filter(
                    |entry| u32::from_le_bytes(entry[..4].try_into().expect("Exact entry"))
                        == self.source.model
                        && u32::from_le_bytes(entry[4..].try_into().expect("Exact entry"))
                            == self.source.index
                )
                .count()
                == 1,
            "Ball selected source no longer matches the captured registry"
        );
        let len = ball::field_len(&self.original.record, &self.replacement)?;
        ensure!(
            self.edit.at == self.original.lease.address + ball::FIELD as u64
                && self.edit.before == self.original.record[ball::FIELD..ball::FIELD + len]
                && self.edit.after == self.replacement[ball::FIELD..ball::FIELD + len],
            "Ball edit is not the exact bounded filename change"
        );
        Ok(())
    }
    pub(crate) fn validate_before(&self, m: &impl Inspect) -> Result<()> {
        self.validate_shape()?;
        let current = capture(m)?;
        ensure!(
            current == self.original
                && current.can_prepare()
                && read(m, self.source.address, ball::STRIDE)? == self.source.record,
            "Ball preparation is stale or became loaded"
        );
        ensure!(
            private_rw(m, self.source.address, ball::STRIDE)? == self.original.lease.allocation,
            "Ball selected source allocation changed"
        );
        Ok(())
    }
    pub(crate) fn validate_after(&self, m: &impl Inspect) -> Result<Snapshot> {
        self.validate_shape()?;
        let current = capture(m)?;
        current.owns(&self.original.lease, &self.replacement)?;
        Ok(current)
    }
    pub(crate) fn restore(&self, m: &impl Inspect) -> Result<Edit> {
        self.validate_shape()?;
        let current = capture(m)?;
        current.owns(&self.original.lease, &self.replacement)?;
        ensure!(
            current.can_restore(),
            "Ball restoration waits for an unloaded stable selection/main/Quick Play menu"
        );
        Ok(Edit {
            at: self.edit.at,
            before: self.edit.after.clone(),
            after: self.edit.before.clone(),
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Prepare,
    Restore,
}
/// Four unchanged samples separated by >=100ms, without sleeping the UI.
#[derive(Default)]
pub(crate) struct Stable {
    snapshot: Option<Snapshot>,
    last_ms: u64,
    count: u8,
    action: Option<Action>,
}
impl Stable {
    pub(crate) fn observe(
        &mut self,
        snapshot: &Snapshot,
        action: Action,
        now_ms: u64,
    ) -> Result<bool> {
        ensure!(
            now_ms >= self.last_ms,
            "Ball stability clock moved backwards"
        );
        let safe = match action {
            Action::Prepare => snapshot.can_prepare(),
            Action::Restore => snapshot.can_restore(),
        };
        if !safe {
            self.snapshot = None;
            self.count = 0;
            self.action = None;
            self.last_ms = now_ms;
            return Ok(false);
        }
        if self.snapshot.as_ref() != Some(snapshot) || self.action != Some(action) {
            self.snapshot = Some(snapshot.clone());
            self.last_ms = now_ms;
            self.count = 1;
            self.action = Some(action);
            return Ok(false);
        }
        if now_ms - self.last_ms >= 100 {
            self.count = self.count.saturating_add(1).min(4);
            self.last_ms = now_ms;
        }
        Ok(self.count >= 4)
    }
}

#[cfg(test)]
#[path = "ball_v2_tests.rs"]
pub(crate) mod tests;
