//! Immutable, read-only NAMEDATA tables for the exact V2 roster owner.
//! No native calls, pointer resolution, effects, or write capabilities.
use crate::{blacktop_native::Memory, roster_v2_profile as owner};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
const STRIDE: usize = 48;
const MAX_ROWS: usize = 20000;
const PROFILE: &[(u64, usize, &str)] = &[
    (
        0x2d39980,
        691,
        "e78286b15fc41a433c8894e2100cd68df23b1d960096e4f82604d41f70e89ae8",
    ),
    (
        0x2d3a470,
        624,
        "f33808d81c2b47c5b5285f85e18f03504569bc6d34d94856aaaac12afa0b44ec",
    ),
    (
        0x2d94920,
        44,
        "edb68d84ddf852eb9f332ff0ce1bdab1dbecddc5164c3279e2350f0989804ad1",
    ),
    (
        0x2d3d240,
        33,
        "e0513cca9b2470e67858a9de387386af7637eb374e4f4e388ca9802a104a872e",
    ),
    (
        0x2d3d270,
        33,
        "7c3ebc366c9b04fe3dc0ea030de91ed070758f0b69faf524706632dee89c7db8",
    ),
    (
        0x2d3d330,
        33,
        "e4aa8dfc25d67f4cf537bc76ae9d407edd911e94be328aea2de1e2fea90cf0b4",
    ),
    (
        0x2d3d300,
        33,
        "e6dddf004a026758bc20e0d75152f0a37da763d00df85ce2e7f7ae64c9deea89",
    ),
    (
        0x2d39d40,
        707,
        "791799f118c6f83fc6aeab3e7cbba5f14f6f7151847257454f54d09d6c146f54",
    ),
    (
        0x2d39c80,
        184,
        "d543f51641df6213a87d5cd09151f03378b96c011af3b90824a463ba5d12662d",
    ),
    (
        0x2d3a050,
        184,
        "a67364405dd3d8e6982425c997d56122cede29d8a1df834639d68b1522b8b75b",
    ),
];

#[derive(Clone, Debug)]
struct Table {
    header: [u8; 16],
    at: u64,
    bytes: Vec<u8>,
    names: BTreeMap<String, Option<(i16, i16)>>,
}
#[derive(Clone, Debug)]
pub(crate) struct Snapshot {
    generation: owner::OwnerSnapshot,
    headers: [[u8; 16]; 2],
    first: Option<Table>,
    last: Option<Table>,
}
struct Bounds {
    owner: u64,
    start: u64,
    end: u64,
    other: Vec<(u64, u64)>,
}
impl Bounds {
    fn new(state: &owner::OwnerSnapshot) -> Result<Self> {
        let mut other = vec![(
            state.owner,
            state
                .owner
                .checked_add(0x2a0)
                .context("Name owner overflow")?,
        )];
        for d in [&state.players, &state.teams, &state.arenas]
            .into_iter()
            .chain(state.uniforms.as_ref())
        {
            other.push((
                d.pointer,
                d.pointer
                    .checked_add(
                        d.count
                            .checked_mul(d.stride)
                            .context("Name dependency overflow")? as u64,
                    )
                    .context("Name dependency overflow")?,
            ));
        }
        Ok(Self {
            owner: state.owner,
            start: state
                .region
                .base
                .checked_add(0x680)
                .context("Name bounds overflow")?,
            end: state
                .region
                .base
                .checked_add(state.region.len)
                .context("Name bounds overflow")?,
            other,
        })
    }
    fn descriptor(&self, h: &[u8; 16]) -> Option<Option<(u64, usize)>> {
        let count = u32::from_le_bytes(h[..4].try_into().ok()?) as usize;
        let at = u64::from_le_bytes(h[8..].try_into().ok()?);
        if count == 0 && at == 0 {
            return Some(None);
        }
        let len = count.checked_mul(STRIDE)?;
        let end = at.checked_add(len as u64)?;
        if count == 0
            || count > MAX_ROWS
            || at % 8 != 0
            || at < self.start
            || end > self.end
            || self.other.iter().any(|(a, b)| at < *b && *a < end)
        {
            return None;
        }
        Some(Some((at, len)))
    }
}
fn read(m: &impl Memory, at: u64, len: usize) -> Result<Vec<u8>> {
    ensure!((1..=4096).contains(&len), "Name audio read exceeds bound");
    m.checkpoint()?;
    let b = m.read(at, len)?;
    ensure!(b.len() == len, "Partial name audio read");
    m.checkpoint()?;
    Ok(b)
}
fn profile(m: &impl Memory) -> Result<bool> {
    for &(rva, len, hash) in PROFILE {
        let bytes = read(
            m,
            m.base().checked_add(rva).context("Name profile overflow")?,
            len,
        )?;
        if format!("{:x}", Sha256::digest(&bytes)) != hash {
            return Ok(false);
        }
    }
    Ok(true)
}
#[cfg(test)]
pub(crate) fn verify_disk_profile(m: &impl Memory) -> Result<()> {
    ensure!(profile(m)?, "Name/audio dictionary profile differs");
    Ok(())
}
fn headers(m: &impl Memory, b: &Bounds) -> Result<[[u8; 16]; 2]> {
    Ok([
        read(
            m,
            b.owner.checked_add(0x240).context("Name header overflow")?,
            16,
        )?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Name header length"))?,
        read(
            m,
            b.owner.checked_add(0x250).context("Name header overflow")?,
            16,
        )?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Name header length"))?,
    ])
}
fn row_name(b: &[u8]) -> Option<String> {
    let units = b
        .get(..40)?
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .take_while(|n| *n != 0)
        .collect::<Vec<_>>();
    let name = String::from_utf16(&units).ok()?;
    if name.is_empty() { None } else { Some(name) }
}
fn names(bytes: &[u8]) -> BTreeMap<String, Option<(i16, i16)>> {
    let mut out = BTreeMap::new();
    for r in bytes.chunks_exact(STRIDE) {
        let Some(name) = row_name(r) else { continue };
        let pair = (
            i16::from_le_bytes([r[40], r[41]]),
            i16::from_le_bytes([r[42], r[43]]),
        );
        out.entry(name)
            .and_modify(|old| {
                if *old != Some(pair) {
                    *old = None
                }
            })
            .or_insert(Some(pair));
    }
    out
}
fn table(m: &impl Memory, header: [u8; 16], span: Option<(u64, usize)>) -> Result<Option<Table>> {
    let Some((at, len)) = span else {
        return Ok(None);
    };
    let mut bytes = Vec::with_capacity(len);
    for offset in (0..len).step_by(4096) {
        bytes.extend(read(m, at + offset as u64, (len - offset).min(4096))?);
    }
    Ok(Some(Table {
        header,
        at,
        names: names(&bytes),
        bytes,
    }))
}
fn capture_tables(
    m: &impl Memory,
    b: &Bounds,
    h: [[u8; 16]; 2],
    state: &owner::OwnerSnapshot,
) -> Result<Option<Snapshot>> {
    let Some(first) = b.descriptor(&h[0]) else {
        return Ok(None);
    };
    let Some(last) = b.descriptor(&h[1]) else {
        return Ok(None);
    };
    if let (Some((a, al)), Some((z, zl))) = (first, last) {
        if a < z + zl as u64 && z < a + al as u64 {
            return Ok(None);
        }
    }
    Ok(Some(Snapshot {
        generation: state.clone(),
        headers: h,
        first: table(m, h[0], first)?,
        last: table(m, h[1], last)?,
    }))
}
/// Unsupported optional tables/code produce None. Read/checkpoint/owner errors
/// are never converted to optional success. This is a PROVISIONAL capture: the
/// caller must revalidate it after all base row validation, before publishing.
pub(crate) fn capture(m: &impl Memory, state: &owner::OwnerSnapshot) -> Result<Option<Snapshot>> {
    owner::revalidate(m, state)?;
    let b = Bounds::new(state)?;
    let h = headers(m, &b)?;
    if h.iter().all(|h| {
        u32::from_le_bytes(h[..4].try_into().unwrap()) == 0
            && u64::from_le_bytes(h[8..].try_into().unwrap()) == 0
    }) {
        owner::revalidate(m, state)?;
        return Ok(None);
    }
    if !profile(m)? {
        owner::revalidate(m, state)?;
        return Ok(None);
    }
    let result = capture_tables(m, &b, h, state)?;
    owner::revalidate(m, state)?;
    Ok(result)
}
impl Snapshot {
    pub(crate) fn revalidate(&self, m: &impl Memory, state: &owner::OwnerSnapshot) -> Result<()> {
        ensure!(
            state == &self.generation,
            "Name audio snapshot belongs to another roster generation"
        );
        owner::revalidate(m, state)?;
        ensure!(profile(m)?, "Name audio profile changed during capture");
        let h = headers(m, &Bounds::new(state)?)?;
        ensure!(h == self.headers, "Name audio headers changed");
        for (index, t) in [&self.first, &self.last].iter().enumerate() {
            if let Some(t) = t {
                ensure!(h[index] == t.header, "Name audio header changed");
                for (offset, expected) in t.bytes.chunks(4096).enumerate() {
                    ensure!(
                        read(m, t.at + offset as u64 * 4096, expected.len())? == expected,
                        "Name audio rows changed"
                    );
                }
            } else {
                ensure!(
                    u32::from_le_bytes(h[index][..4].try_into()?) == 0
                        && u64::from_le_bytes(h[index][8..].try_into()?) == 0,
                    "Absent name table changed"
                );
            }
        }
        ensure!(
            headers(m, &Bounds::new(state)?)? == self.headers,
            "Name audio headers changed during ending table read"
        );
        ensure!(
            profile(m)?,
            "Name audio profile changed during ending table read"
        );
        owner::revalidate(m, state)
    }
}
/// Rebuild this projection after every verified PLAYER replacement. No reads.
pub(crate) fn project(snapshot: Option<&Snapshot>, row: &[u8]) -> Result<Value> {
    ensure!(
        row.len() == owner::PLAYER_STRIDE,
        "Incomplete player audio row"
    );
    let first = row_name(&row[40..80]);
    let last = row_name(&row[..40]);
    let localized = u64::from_le_bytes(row[0x50..0x58].try_into()?) != 0;
    let lookup = |table: Option<&Table>, name: Option<String>| -> Value {
        table
            .and_then(|t| name.and_then(|n| t.names.get(&n).copied().flatten()))
            .map(|(commentary, pa)| json!({"commentaryId":commentary,"paId":pa}))
            .unwrap_or(Value::Null)
    };
    Ok(
        json!({"fullNameId":i16::from_le_bytes([row[0x2e8],row[0x2e9]]),"firstName":lookup(snapshot.and_then(|s|s.first.as_ref()),first),"lastName":if localized {Value::Null}else{lookup(snapshot.and_then(|s|s.last.as_ref()),last)},"nameOverrideActive":localized}),
    )
}
#[cfg(test)]
#[path = "roster_v2_name_audio_tests.rs"]
mod tests;
