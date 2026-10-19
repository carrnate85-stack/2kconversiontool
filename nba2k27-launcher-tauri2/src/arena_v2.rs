//! Version 2 Blacktop arena selection, restricted to exact bundled choices.
//! Pure inspection/planning only: no process discovery, writes, code hooks,
//! page-protection changes, file publication or engine calls are performed.
#![allow(dead_code)]
use crate::{
    blacktop_arena::{self as arena, Choice, Inspect, Record, STOCK, Snapshot},
    blacktop_native::Memory,
    roster_v2_profile as roster, rules_v2,
};
use anyhow::{Context, Result, ensure};

pub(crate) const MAPPING: u64 = 0x078EA630;
const STRIDE: u64 = 0x1278;
const SHA: &str = "29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1";
fn at(m: &impl Memory, offset: u64) -> Result<u64> {
    m.base().checked_add(offset).context("Arena RVA overflow")
}
fn read(m: &impl Memory, address: u64, len: usize) -> Result<Vec<u8>> {
    ensure!(
        address >= 0x10000
            && (1..=4096).contains(&len)
            && address
                .checked_add(len as u64)
                .is_some_and(|end| end < 0x7fff00000000),
        "Invalid bounded arena read"
    );
    m.checkpoint()?;
    let bytes = m.read(address, len)?;
    ensure!(bytes.len() == len, "Partial arena read");
    Ok(bytes)
}
fn q(m: &impl Memory, address: u64) -> Result<u64> {
    let value = u64::from_le_bytes(read(m, address, 8)?.as_slice().try_into()?);
    ensure!(
        value >= 0x10000 && value < 0x7fff00000000 && value % 8 == 0,
        "Invalid arena ownership pointer"
    );
    Ok(value)
}
fn hex(s: &str) -> Result<Vec<u8>> {
    ensure!(
        s.len() % 2 == 0 && s.len() <= 512,
        "Invalid arena anchor hex"
    );
    s.as_bytes()
        .chunks_exact(2)
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}
fn anchors() -> Result<serde_json::Value> {
    let value: serde_json::Value =
        serde_json::from_str(include_str!("../assets/arena-v2-anchors.json"))?;
    ensure!(
        value["schema"] == "nba2k27-v2-blacktop-arena-anchors-v1"
            && value["sha256"] == SHA
            && value["mappingRva"].as_u64() == Some(MAPPING),
        "Invalid Version 2 arena reference"
    );
    Ok(value)
}
pub(crate) fn verify_profile(m: &impl Memory) -> Result<()> {
    rules_v2::verify_profile(m)?;
    roster::verify_identity_profile(m)?;
    let value = anchors()?;
    let rows = value["anchors"]
        .as_array()
        .context("Missing arena anchors")?;
    ensure!(rows.len() == 9, "Arena proof count changed");
    for row in rows {
        let rva = row["rva"].as_u64().context("Missing arena code address")?;
        let bytes = hex(row["hex"].as_str().context("Missing arena code bytes")?)?;
        ensure!(
            (0x1000..0x5D00000).contains(&rva) && read(m, at(m, rva)?, bytes.len())? == bytes,
            "Version 2 arena code layout changed: {}",
            row["label"]
        );
    }
    // The ID-preserving constructor writes these actual native arena names to
    // +7E/+A4 with lengths38/40. Independent directory-backed copy code proves
    // they belong to the +60/+68 arena table, not merely similarly sized data.
    for (rva, text) in [
        (0x07369224, "arena_701_int"),
        (0x07369240, "arena_701_int_floor"),
    ] {
        let bytes: Vec<u8> = text
            .encode_utf16()
            .chain(std::iter::once(0))
            .flat_map(u16::to_le_bytes)
            .collect();
        ensure!(
            read(m, at(m, rva)?, bytes.len())? == bytes,
            "Native arena filename literal changed"
        );
    }
    m.checkpoint()
}
fn region(m: &impl Inspect, address: u64, len: u64, allocation: u64, image: bool) -> Result<()> {
    let p = m.region(address)?;
    ensure!(
        p.committed
            && p.allocation == allocation
            && p.kind == if image { 0x1000000 } else { 0x20000 }
            && p.protection == if image { 2 } else { 4 }
            && address >= p.start
            && address
                .checked_add(len)
                .zip(p.start.checked_add(p.len))
                .is_some_and(|(end, limit)| end <= limit),
        "Arena target left its exact read-only image/private RW non-executable allocation"
    );
    Ok(())
}
fn ascii(bytes: &[u8]) -> Result<String> {
    let end = bytes
        .iter()
        .position(|b| *b == 0)
        .context("Unterminated arena filename")?;
    ensure!(bytes[..end].is_ascii(), "Non-ASCII arena filename");
    Ok(String::from_utf8(bytes[..end].to_vec())?)
}
fn selection(m: &impl Memory) -> Result<crate::blacktop_native::ContextState> {
    let c = rules_v2::context(m)?;
    ensure!(
        rules_v2::boundary(&c, m.base()) == "selection",
        "Return to stable Blacktop player selection before changing arenas"
    );
    Ok(c)
}
pub(crate) fn revalidate(m: &impl Inspect, r: &Record, expected: &[u8]) -> Result<()> {
    verify_profile(m)?;
    ensure!(
        r.header.len() == 16 && r.fields.len() == 78 && expected.len() == 78,
        "Invalid arena record lease sizes"
    );
    let count = u64::from_le_bytes(r.header[..8].try_into()?);
    let table = u64::from_le_bytes(r.header[8..].try_into()?);
    ensure!(
        (1..=2048).contains(&count)
            && r.owner.checked_add(0x60) == Some(r.header_address)
            && r.owner.checked_sub(r.allocation) == Some(0x50)
            && r.address
                .checked_sub(table)
                .is_some_and(|off| off % STRIDE == 0 && off / STRIDE < count)
            && q(m, at(m, roster::ROOT)?)? == r.root
            && q(m, r.root)? == at(m, roster::ROOT_CLASS)?
            && q(m, r.root.checked_add(0xA8).context("Arena root overflow")?)? == r.owner
            && read(m, r.header_address, 16)? == r.header,
        "Arena roster directory ownership changed"
    );
    region(m, r.owner, 0x70, r.allocation, false)?;
    region(m, table, count * STRIDE, r.allocation, false)?;
    ensure!(
        read(m, r.address + 0x792, 2)? == r.id.to_le_bytes()
            && read(m, r.address + 0x7E, 78)? == expected,
        "Arena ID or fields changed outside this launcher"
    );
    region(m, r.address + 0x7E, 78, r.allocation, false)
}
pub(crate) fn capture(m: &impl Inspect, c: &Choice, expected: &[u8; 20]) -> Result<Snapshot> {
    ensure!(
        arena::choice(&c.id)? == c,
        "Arena differs from the exact embedded choice"
    );
    verify_profile(m)?;
    let before = selection(m)?;
    region(m, at(m, MAPPING)?, 20, m.base(), true)?;
    let mapping: [u8; 20] = read(m, at(m, MAPPING)?, 20)?.as_slice().try_into()?;
    ensure!(
        arena::valid_mapping(expected) && mapping == *expected && mapping[8..] == STOCK[8..],
        "Arena mapping changed outside this session"
    );
    let record = if c.id == "default" {
        None
    } else {
        let wanted = c.arena_id.context("Missing arena ID")?;
        let file = if c.template_arena_id.is_some() {
            "arena_700_int"
        } else {
            c.file.as_deref().context("Missing arena filename")?
        };
        let root = q(m, at(m, roster::ROOT)?)?;
        ensure!(
            q(m, root)? == at(m, roster::ROOT_CLASS)?,
            "Arena roster class changed"
        );
        let owner = q(m, root.checked_add(0xA8).context("Arena root overflow")?)?;
        let p = m.region(owner)?;
        ensure!(
            owner.checked_sub(p.allocation) == Some(0x50),
            "Arena roster owner placement changed"
        );
        region(m, owner, 0x70, p.allocation, false)?;
        let header_address = owner + 0x60;
        let header = read(m, header_address, 16)?;
        let count = u64::from_le_bytes(header[..8].try_into()?);
        let table = u64::from_le_bytes(header[8..].try_into()?);
        ensure!(
            (1..=2048).contains(&count) && table >= header_address + 16 && table % 8 == 0,
            "Arena directory is initializing or changed bounds"
        );
        region(m, table, count * STRIDE, p.allocation, false)?;
        let (mut known, mut matched, mut found) = (0, 0, None);
        for i in 0..count {
            let address = table + i * STRIDE;
            let row = read(m, address, 0x794)?;
            let id = u16::from_le_bytes(row[0x792..0x794].try_into()?);
            let name = ascii(&row[0x7E..0xA4])?;
            if id == 700 && name == "arena_700_int" {
                known += 1;
            }
            if u32::from(id) == wanted && name == file {
                matched += 1;
                found = Some(Record {
                    address,
                    header_address,
                    root,
                    owner,
                    allocation: p.allocation,
                    header: header.clone(),
                    fields: row[0x7E..0xCC].to_vec(),
                    id,
                });
            }
        }
        ensure!(
            known == 1 && matched == 1,
            "Selected arena is absent or ambiguous in this active roster"
        );
        let r = found.context("Selected arena missing")?;
        revalidate(m, &r, &r.fields)?;
        if c.template_arena_id.is_some() {
            ensure!(
                ascii(&r.fields[38..])? == "arena_700_int_floor",
                "Arena template floor changed"
            );
        } else if let Some(floor) = &c.floor_file {
            ensure!(
                ascii(&r.fields[38..])? == *floor,
                "Arena floor differs from the exact catalog"
            );
        }
        Some(r)
    };
    if let Some(r) = &record {
        revalidate(m, r, &r.fields)?;
    }
    ensure!(
        selection(m)? == before && read(m, at(m, MAPPING)?, 20)? == mapping,
        "Arena context changed during capture"
    );
    region(m, at(m, MAPPING)?, 20, m.base(), true)?;
    Ok(Snapshot {
        image: m.base(),
        context: before,
        mapping,
        record,
    })
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Edit {
    pub at: u64,
    pub before: Vec<u8>,
    pub after: Vec<u8>,
}
#[derive(Clone, Debug)]
pub(crate) struct Plan {
    pub selected: String,
    pub snapshot: Snapshot,
    pub target_mapping: [u8; 20],
    pub template: Option<Vec<u8>>,
}
pub(crate) fn verify_assets(c: &Choice, manifest: Option<&[u8]>) -> Result<()> {
    ensure!(
        arena::choice(&c.id)? == c,
        "Arena differs from the exact bundled choice"
    );
    if c.template_arena_id.is_none() {
        return Ok(());
    }
    let bytes = manifest.context("Arena template requires verified stock manifest bytes")?;
    ensure!(
        !bytes.is_empty() && bytes.len() <= 64 * 1024 * 1024,
        "Arena manifest size is invalid"
    );
    let text = std::str::from_utf8(bytes)?;
    for name in [c.file.as_deref().unwrap(), c.floor_file.as_deref().unwrap()] {
        let asset = format!("levels/{name}.iff");
        ensure!(
            text.lines().any(|line| line
                .split(',')
                .next()
                .is_some_and(|path| path.replace('\\', "/").eq_ignore_ascii_case(&asset))),
            "Arena template asset absent from stock manifest: {asset}"
        );
    }
    Ok(())
}
pub(crate) fn prepare(
    m: &impl Inspect,
    id: &str,
    expected: &[u8; 20],
    manifest: Option<&[u8]>,
) -> Result<Plan> {
    let c = arena::choice(id)?;
    verify_assets(c, manifest)?;
    let snapshot = capture(m, c, expected)?;
    let template = if c.template_arena_id.is_some() {
        Some(arena::template_fields(
            c.file.as_deref().unwrap(),
            c.floor_file.as_deref().unwrap(),
        )?)
    } else {
        None
    };
    Ok(Plan {
        selected: id.into(),
        snapshot,
        target_mapping: arena::mapping(c.arena_id)?,
        template,
    })
}
impl Plan {
    fn shape(&self) -> Result<()> {
        let c = arena::choice(&self.selected)?;
        ensure!(
            self.target_mapping == arena::mapping(c.arena_id)?
                && arena::valid_mapping(&self.snapshot.mapping),
            "Arena plan mapping changed"
        );
        match (&self.snapshot.record, &self.template) {
            (None, None) => ensure!(c.id == "default", "Arena plan missing its roster record"),
            (Some(r), template) => {
                ensure!(
                    Some(u32::from(r.id)) == c.arena_id && r.fields.len() == 78,
                    "Arena plan differs from its roster ID"
                );
                if c.template_arena_id.is_some() {
                    ensure!(
                        r.id == 700
                            && ascii(&r.fields[..38])? == "arena_700_int"
                            && ascii(&r.fields[38..])? == "arena_700_int_floor"
                            && template.as_ref()
                                == Some(&arena::template_fields(
                                    c.file.as_deref().unwrap(),
                                    c.floor_file.as_deref().unwrap()
                                )?),
                        "Arena template plan differs from the exact bundled fields"
                    );
                } else {
                    ensure!(template.is_none() && Some(ascii(&r.fields[..38])?) == c.file
                        && c.floor_file.as_ref().is_none_or(|f| ascii(&r.fields[38..]).is_ok_and(|name| &name == f)),
                        "Non-template arena plan changes roster fields");
                }
            }
            _ => anyhow::bail!("Invalid arena template plan"),
        }
        Ok(())
    }
    /// Apply in this order: private template names first, then image mapping.
    /// The adapter must durably journal intent before performing any edit.
    pub(crate) fn edits(&self) -> Result<Vec<Edit>> {
        self.shape()?;
        let mut edits = Vec::new();
        if let (Some(r), Some(next)) = (&self.snapshot.record, &self.template) {
            if r.fields != *next {
                edits.push(Edit {
                    at: r.address + 0x7E,
                    before: r.fields.clone(),
                    after: next.clone(),
                });
            }
        }
        if self.snapshot.mapping != self.target_mapping {
            edits.push(Edit {
                at: self.snapshot.image + MAPPING,
                before: self.snapshot.mapping.to_vec(),
                after: self.target_mapping.to_vec(),
            });
        }
        Ok(edits)
    }
    /// Revalidate the retained directory, exact expected fields and mapping
    /// immediately before/after each edit. Never write in match/loading/menu.
    pub(crate) fn verify_step(&self, m: &impl Inspect, applied: usize) -> Result<()> {
        let edits = self.edits()?;
        ensure!(
            applied <= edits.len() && m.base() == self.snapshot.image,
            "Invalid arena plan phase/image"
        );
        if applied == 0 {
            let fresh = capture(m, arena::choice(&self.selected)?, &self.snapshot.mapping)?;
            ensure!(
                fresh.context == self.snapshot.context && fresh.record == self.snapshot.record,
                "Arena complete preparation changed before its first write"
            );
        }
        verify_profile(m)?;
        ensure!(
            selection(m)? == self.snapshot.context,
            "Arena selection identity changed"
        );
        region(m, at(m, MAPPING)?, 20, m.base(), true)?;
        let mapping = if edits
            .iter()
            .take(applied)
            .any(|e| e.at == m.base() + MAPPING)
        {
            &self.target_mapping
        } else {
            &self.snapshot.mapping
        };
        ensure!(
            read(m, at(m, MAPPING)?, 20)? == *mapping,
            "Arena mapping changed outside this transaction"
        );
        if let Some(r) = &self.snapshot.record {
            let fields = if edits.iter().take(applied).any(|e| e.at == r.address + 0x7E) {
                self.template
                    .as_deref()
                    .context("Missing applied template")?
            } else {
                &r.fields
            };
            revalidate(m, r, fields)?;
        }
        ensure!(
            selection(m)? == self.snapshot.context,
            "Arena selection changed during verification"
        );
        Ok(())
    }
    /// Undo only proven-owned values, in reverse order, at idle player
    /// selection. At Main/Quick Play the runtime keeps the receipt/mapping and
    /// revalidates on the next selection instead of touching a loading court.
    pub(crate) fn restore(&self, m: &impl Inspect) -> Result<Vec<Edit>> {
        let edits = self.edits()?;
        self.verify_step(m, edits.len())?;
        Ok(edits
            .into_iter()
            .rev()
            .map(|e| Edit {
                at: e.at,
                before: e.after,
                after: e.before,
            })
            .collect())
    }
}

#[cfg(test)]
#[path = "arena_v2_tests.rs"]
pub(crate) mod tests;
