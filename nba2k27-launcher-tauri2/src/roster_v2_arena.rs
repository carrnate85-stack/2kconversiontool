//! Existing-arena readonly catalog. No game calls, process discovery, or writer.
//! OwnedRegion comes from the retained native reader; no browser address is accepted.
use crate::{blacktop_native::Memory, roster_v2_profile as owner};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::OnceLock};
pub(crate) const STRIDE: usize = 4728;
pub(crate) const TEAM_POINTER: usize = 0xee0;
const PROOF_HASH: &str = "f5fae5c88d96cfcb091298a5cabfbc50edaf2785392c27d1899108853643dc8d";
const IMAGE: &str = "29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1";
const SPANS: &[(u64, usize, &str)] = &[
    (
        0x2ed9aa0,
        275,
        "08e3cc94ccaf88726af0ff89b0cf0bf2d8e4a6b7ce5d5e718a8517ac843da292",
    ),
    (
        0x44380,
        67,
        "75f733cb3f251b2b20a7e3c9bfc01dfa80508acfa89c55fd446deae1795f3e2d",
    ),
    (
        0x2f88ab0,
        471,
        "3922dd9acfed1f74a4767975295eb1a491ff82cd13a7b6ea6009eaa7ae00e4b8",
    ),
];
fn bytes(hex: &str) -> Result<Vec<u8>> {
    ensure!(
        hex.len() % 2 == 0 && hex.len() <= 16384,
        "Invalid arena proof hex"
    );
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).context("Invalid arena proof byte"))
        .collect()
}
pub(crate) fn proof() -> Result<&'static Value> {
    static P: OnceLock<std::result::Result<Value, String>> = OnceLock::new();
    P.get_or_init(|| {
        let raw = include_bytes!("../assets/roster-v2-arena-proof.json");
        (|| -> Result<Value> {
            ensure!(
                format!("{:x}", Sha256::digest(raw)) == PROOF_HASH,
                "Arena proof content changed"
            );
            let p: Value = serde_json::from_slice(raw)?;
            ensure!(
                p["schema"] == "nba2k27-v2-existing-arena-readonly-v1"
                    && p["imageSha256"] == IMAGE
                    && p["rowBytes"] == STRIDE
                    && p["readOnly"] == true
                    && p["arenaWrites"] == false,
                "Unsupported arena proof"
            );
            Ok(p)
        })()
        .map_err(|e| format!("{e:#}"))
    })
    .as_ref()
    .map_err(|e| anyhow::anyhow!("Arena proof unavailable: {e}"))
}
fn read(m: &impl Memory, at: u64, len: usize) -> Result<Vec<u8>> {
    ensure!(
        at >= 0x10000
            && (1..=4096).contains(&len)
            && at
                .checked_add(len as u64)
                .is_some_and(|e| e <= 0x7fff_ffff_ffff),
        "Invalid arena read range"
    );
    m.checkpoint()?;
    let b = m.read(at, len)?;
    ensure!(b.len() == len, "Partial arena read");
    Ok(b)
}
fn address(base: u64, rva: u64) -> Result<u64> {
    base.checked_add(rva).context("Arena address overflow")
}
fn profile(m: &impl Memory) -> Result<bool> {
    let p = proof()?;
    for &(rva, len, hash) in SPANS {
        if format!(
            "{:x}",
            Sha256::digest(read(m, address(m.base(), rva)?, len)?)
        ) != hash
        {
            return Ok(false);
        }
    }
    for field in p["fields"].as_array().context("Arena fields missing")? {
        let at = address(
            m.base(),
            field["rva"].as_u64().context("Arena field RVA missing")?,
        )?;
        if read(m, at, 24)?
            != bytes(
                field["prefixHex"]
                    .as_str()
                    .context("Arena field proof missing")?,
            )?
        {
            return Ok(false);
        }
        let meta = field["metadataRva"]
            .as_u64()
            .context("Arena metadata RVA missing")?;
        if meta != 0 {
            if u64::from_le_bytes(read(m, at + 24, 8)?.as_slice().try_into()?)
                != address(m.base(), meta)?
            {
                return Ok(false);
            }
            if read(m, address(m.base(), meta)?, 32)?
                != bytes(
                    field["metadataHex"]
                        .as_str()
                        .context("Arena metadata missing")?,
                )?
            {
                return Ok(false);
            }
        }
    }
    m.checkpoint()?;
    Ok(true)
}
pub(crate) fn verify_profile(m: &impl Memory) -> Result<()> {
    ensure!(profile(m)?, "Arena native profile differs");
    Ok(())
}
fn bounds(state: &owner::OwnerSnapshot) -> Result<usize> {
    let d = &state.arenas;
    let len = d.count.checked_mul(STRIDE).context("Arena size overflow")?;
    let end = state
        .region
        .base
        .checked_add(state.region.len)
        .context("Arena allocation overflow")?;
    ensure!(
        (1..=2048).contains(&d.count)
            && d.stride == STRIDE
            && d.pointer % 8 == 0
            && d.pointer
                >= state
                    .region
                    .base
                    .checked_add(0x680)
                    .context("Arena allocation overflow")?
            && d.pointer.checked_add(len as u64).is_some_and(|v| v <= end),
        "Arena directory is outside its native allocation"
    );
    let stop = d.pointer + len as u64;
    let mut spans = vec![(
        state.owner,
        state
            .owner
            .checked_add(0x2a0)
            .context("Arena owner overflow")?,
    )];
    for other in [&state.players, &state.teams]
        .into_iter()
        .chain(state.uniforms.as_ref())
    {
        let size = other
            .count
            .checked_mul(other.stride)
            .context("Arena dependency overflow")?;
        spans.push((
            other.pointer,
            other
                .pointer
                .checked_add(size as u64)
                .context("Arena dependency overflow")?,
        ));
    }
    ensure!(
        spans.into_iter().all(|(a, b)| stop <= a || d.pointer >= b),
        "Arena table overlaps roster dependencies"
    );
    Ok(len)
}
fn table(m: &impl Memory, state: &owner::OwnerSnapshot) -> Result<Vec<u8>> {
    let n = bounds(state)?;
    let mut out = Vec::with_capacity(n);
    for offset in (0..n).step_by(4096) {
        out.extend(read(
            m,
            state.arenas.pointer + offset as u64,
            (n - offset).min(4096),
        )?);
    }
    Ok(out)
}
fn utf16(b: &[u8]) -> Option<String> {
    let units: Vec<u16> = b
        .chunks_exact(2)
        .map(|v| u16::from_le_bytes([v[0], v[1]]))
        .collect();
    let end = units.iter().position(|&v| v == 0)?;
    let s = String::from_utf16(&units[..end]).ok()?;
    (!s.chars().any(char::is_control)).then_some(s)
}
fn ascii(b: &[u8]) -> Option<String> {
    let end = b.iter().position(|&v| v == 0)?;
    let s = &b[..end];
    (s.iter().all(|v| (32..=126).contains(v))).then(|| String::from_utf8(s.to_vec()).unwrap())
}
#[derive(Clone, Debug)]
struct Row {
    id: u16,
    display: Option<Value>,
}
fn decode(row: &[u8], index: usize) -> Result<Row> {
    ensure!(row.len() == STRIDE, "Incomplete arena row");
    let id = u16::from_le_bytes(row[0x792..0x794].try_into()?);
    let display = (|| {
        let name = utf16(&row[..70])?;
        let city = utf16(&row[0x46..0x76])?;
        let model = ascii(&row[0x7e..0xa4])?;
        let floor = ascii(&row[0xa4..0xcc])?;
        if name.trim().is_empty() || model.trim().is_empty() {
            return None;
        }
        // An empty actual floor is displayed but not a selectable existing-floor target.
        if floor.trim().is_empty() {
            return None;
        }
        Some(json!({"id":id,"index":index,"name":name,"city":city,"model":model,"floorFile":floor}))
    })();
    Ok(Row { id, display })
}
#[derive(Clone, Debug)]
pub(crate) struct Snapshot {
    generation: owner::OwnerSnapshot,
    id: String,
    raw: Vec<u8>,
    rows: Vec<Row>,
    by_id: BTreeMap<u16, Option<usize>>,
}
/// Internal read dependency only. Never serialized and never a writable record kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Witness {
    generation: String,
    index: usize,
    id: u16,
    at: u64,
    raw: Vec<u8>,
}
impl Witness {
    pub(crate) fn address(&self) -> u64 {
        self.at
    }
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.raw
    }
    pub(crate) fn id(&self) -> u16 {
        self.id
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Selection {
    old: Witness,
    target: Witness,
}
impl Selection {
    pub(crate) fn validate_catalog(&self, id: &str) -> Result<()> {
        ensure!(
            self.old.generation == id && self.target.generation == id,
            "Arena selection catalog is stale"
        );
        Ok(())
    }
    pub(crate) fn old(&self) -> &Witness {
        &self.old
    }
    pub(crate) fn target(&self) -> &Witness {
        &self.target
    }
    pub(crate) fn validate_request(&self, id: u64, raw: &[u8]) -> Result<u64> {
        ensure!(
            raw.len() == owner::TEAM_STRIDE
                && id == self.target.id as u64
                && self.old.generation == self.target.generation,
            "Arena selection does not match this request"
        );
        ensure!(
            u64::from_le_bytes(raw[TEAM_POINTER..TEAM_POINTER + 8].try_into()?) == self.old.at,
            "Original team arena changed before Save"
        );
        Ok(self.target.at)
    }
    #[cfg(test)]
    pub(crate) fn for_test(old: u64, new: u64, id: u16) -> Self {
        let w = |at, id| Witness {
            generation: "pure-planner-fixture".into(),
            index: 0,
            id,
            at,
            raw: vec![0; STRIDE],
        };
        Self {
            old: w(old, 0),
            target: w(new, id),
        }
    }
}
impl Snapshot {
    fn from_rows(state: &owner::OwnerSnapshot, raw: Vec<u8>) -> Result<Self> {
        ensure!(raw.len() == bounds(state)?, "Incomplete arena catalog");
        let mut rows = Vec::new();
        let mut by_id = BTreeMap::new();
        for (i, row) in raw.chunks_exact(STRIDE).enumerate() {
            let r = decode(row, i)?;
            by_id
                .entry(r.id)
                .and_modify(|v| *v = None)
                .or_insert(Some(i));
            rows.push(r);
        }
        let mut h = Sha256::new();
        for v in [
            state.root,
            state.owner,
            state.region.base,
            state.region.len,
            state.arenas.pointer,
            state.arenas.count as u64,
        ] {
            h.update(v.to_le_bytes());
        }
        h.update(&raw);
        Ok(Self {
            generation: state.clone(),
            id: format!("{:x}", h.finalize()),
            raw,
            rows,
            by_id,
        })
    }
    pub(crate) fn id(&self) -> &str {
        &self.id
    }
    /// Immutable capture bytes for the separate reserved-slot clone admission.
    /// This does not create an arena write capability.
    pub(crate) fn raw_rows(&self) -> &[u8] {
        &self.raw
    }
    pub(crate) fn clone_choices(&self) -> Value {
        json!(self.raw.chunks_exact(STRIDE).enumerate().filter_map(|(index,row)| {
            let id=self.rows[index].id;
            if self.by_id.get(&id)!=Some(&Some(index)){return None;}
            if let Some(display)=&self.rows[index].display {
                let mut value=display.clone();value["arenaId"]=json!(id);value["arenaFile"]=value["model"].clone();value["emptyCandidate"]=json!(false);Some(value)
            } else if (996..=1045).contains(&id)&&crate::roster_v2_arena_clone::empty(row) {
                Some(json!({"id":id,"arenaId":id,"index":index,"name":"","arenaFile":"","floorFile":"","emptyCandidate":true}))
            } else {None}
        }).collect::<Vec<_>>())
    }
    pub(crate) fn document(&self) -> Value {
        json!({"catalogId":self.id,"arenas":self.rows.iter().enumerate().filter(|(i,r)|self.by_id.get(&r.id)==Some(&Some(*i))).filter_map(|(_,r)|r.display.as_ref()).collect::<Vec<_>>(),"count":self.rows.len()})
    }
    pub(crate) fn witness(&self, id: u16) -> Result<Witness> {
        let index = self
            .by_id
            .get(&id)
            .copied()
            .flatten()
            .context("Arena ID is unavailable or ambiguous")?;
        ensure!(
            self.rows[index].display.is_some(),
            "Arena text/floor is unavailable"
        );
        Ok(Witness {
            generation: self.id.clone(),
            index,
            id,
            at: self.generation.arenas.pointer + (index * STRIDE) as u64,
            raw: self.raw[index * STRIDE..(index + 1) * STRIDE].to_vec(),
        })
    }
    pub(crate) fn witness_at(&self, pointer: u64) -> Result<Witness> {
        let offset = pointer
            .checked_sub(self.generation.arenas.pointer)
            .context("Current arena is outside its catalog")?;
        ensure!(
            offset % STRIDE as u64 == 0 && offset / (STRIDE as u64) < self.rows.len() as u64,
            "Current arena is not a complete catalog row"
        );
        let index = (offset / STRIDE as u64) as usize;
        let w = self.witness(self.rows[index].id)?;
        ensure!(
            w.index == index && w.at == pointer,
            "Current arena ID is ambiguous"
        );
        Ok(w)
    }
    pub(crate) fn selection(
        &self,
        current_pointer: u64,
        id: u16,
        catalog_id: &str,
    ) -> Result<Selection> {
        ensure!(catalog_id == self.id, "Arena catalog selection is stale");
        Ok(Selection {
            old: self.witness_at(current_pointer)?,
            target: self.witness(id)?,
        })
    }
    pub(crate) fn revalidate(&self, m: &impl Memory, state: &owner::OwnerSnapshot) -> Result<()> {
        ensure!(
            state == &self.generation,
            "Arena catalog belongs to another roster owner"
        );
        owner::revalidate(m, state)?;
        ensure!(profile(m)?, "Arena native profile changed");
        ensure!(table(m, state)? == self.raw, "Arena catalog rows changed");
        owner::revalidate(m, state)?;
        ensure!(
            table(m, state)? == self.raw,
            "Arena catalog rows changed during verification"
        );
        ensure!(
            profile(m)?,
            "Arena native profile changed during verification"
        );
        owner::revalidate(m, state)
    }
    fn revalidate_dependencies(
        &self,
        m: &impl Memory,
        state: &owner::OwnerSnapshot,
        witnesses: &[&Witness],
    ) -> Result<()> {
        ensure!(
            state == &self.generation,
            "Arena dependencies belong to another roster owner"
        );
        for w in witnesses {
            ensure!(
                w.generation == self.id && self.witness(w.id)? == **w,
                "Arena dependency is not from this immutable catalog"
            );
        }
        owner::revalidate(m, state)?;
        verify_profile(m)?;
        for pass in 0..2 {
            for (index, w) in witnesses.iter().enumerate() {
                if witnesses[..index].iter().any(|old| old.at == w.at) {
                    continue;
                }
                for (chunk, expected) in w.raw.chunks(4096).enumerate() {
                    ensure!(
                        read(m, w.at + chunk as u64 * 4096, expected.len())? == expected,
                        "Retained arena row changed"
                    );
                }
            }
            owner::revalidate(m, state)?;
            if pass == 1 {
                verify_profile(m)?;
                owner::revalidate(m, state)?;
            }
        }
        Ok(())
    }
    pub(crate) fn revalidate_witness(
        &self,
        m: &impl Memory,
        state: &owner::OwnerSnapshot,
        w: &Witness,
    ) -> Result<()> {
        self.revalidate_dependencies(m, state, &[w])
    }
    /// IDs are resolved uniquely at stable capture and full-catalog admission.
    /// Every forward/final/reverse owner check then verifies the sealed source
    /// and target addresses/full rows twice. No lookup or address rebinding.
    pub(crate) fn revalidate_selection(
        &self,
        m: &impl Memory,
        state: &owner::OwnerSnapshot,
        s: &Selection,
    ) -> Result<()> {
        s.validate_catalog(&self.id)?;
        self.revalidate_dependencies(m, state, &[&s.old, &s.target])
    }
}
pub(crate) fn capture(m: &impl Memory, state: &owner::OwnerSnapshot) -> Result<Option<Snapshot>> {
    owner::revalidate(m, state)?;
    bounds(state)?;
    if !profile(m)? {
        owner::revalidate(m, state)?;
        return Ok(None);
    }
    let a = table(m, state)?;
    owner::revalidate(m, state)?;
    let b = table(m, state)?;
    ensure!(a == b, "Arena rows changed during capture");
    ensure!(profile(m)?, "Arena native profile changed during capture");
    owner::revalidate(m, state)?;
    Ok(Some(Snapshot::from_rows(state, a)?))
}
#[cfg(test)]
#[path = "roster_v2_arena_tests.rs"]
mod tests;
