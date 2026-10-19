//! Read-only COLLEGEDATA snapshot. No pointer setter/native calls/write leases.
use crate::{blacktop_native::Memory, roster_v2_profile as owner};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
const STRIDE: usize = 72;
const MAX_ROWS: usize = 4096;
const PROFILE: &[(u64, usize, &str)] = &[
    (
        48107120,
        83,
        "6e367717cee6608cea195c8c55ac7bc18f875dfc15d351f88712e4f9f1d62e10",
    ),
    (
        48107264,
        214,
        "31454249e561831d510e9d2f7264292faac6009922484c2ef094a16c00d6132a",
    ),
    (
        46948832,
        99,
        "d0f49b0aa4f6118b81f2276622e0c1b2a7e4222179cbb54224d6564fb9070d28",
    ),
    (
        46943280,
        980,
        "e02709a8c17865a088006ce0c79949660bbaf375235b74b453d6b6c31a55b6a5",
    ),
    (
        48107488,
        220,
        "fe089022d88ad093c4f832e6b24c834d458a89956c0bc899314e481860c311f7",
    ),
    (
        48107712,
        220,
        "89f84c7573d61d1fc21ba72c8be74b06a7bc90caef9ca3ef63cd66f629a2a123",
    ),
    (
        79452208,
        222,
        "0eeb3301e76b137734b2ca65ec4dfdbe61c1dd854bb4ac1ffdf599fe2962066e",
    ),
    (
        49390544,
        288,
        "3d4dadabe0f92da852b5bad3e75a97a16c772f04df5d992fd5c118947ef1ca38",
    ),
];

#[derive(Clone, Debug)]
pub(crate) struct Snapshot {
    generation: owner::OwnerSnapshot,
    id: String,
    header: [u8; 16],
    pointer: u64,
    bytes: Vec<u8>,
    rows: Vec<(Option<String>, bool)>,
    choices_available: bool,
}
pub(crate) fn proof() -> Result<()> {
    let v: Value = serde_json::from_str(include_str!("../assets/roster-v2-origins-proof.json"))?;
    ensure!(
        v["schema"] == "nba2k27-v2-origins-readonly-proof-v1"
            && v["imageSha256"]
                == "29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1"
            && v["readOnly"] == true,
        "Origin proof identity differs"
    );
    ensure!(
        v["layout"]
            == json!({"countOffset":144,"pointerOffset":152,"countWidth":4,"stride":72,"nameBytes":34,"flagsOffset":52,"playerPointerOffset":176,"selectableMask":201326592,"nullGetterSentinel":514,"setterMinusOne":"no-op"}),
        "Origin proof layout differs"
    );
    let methods = v["methods"].as_array().context("Origin methods missing")?;
    ensure!(
        methods.len() == PROFILE.len(),
        "Origin method count differs"
    );
    for (m, &(rva, len, sha)) in methods.iter().zip(PROFILE) {
        ensure!(
            m["rva"] == rva
                && m["bytes"] == len
                && m["sha256"] == sha
                && m["completePdata"] == true,
            "Origin method proof differs"
        );
    }
    Ok(())
}
fn read(m: &impl Memory, at: u64, len: usize) -> Result<Vec<u8>> {
    ensure!((1..=4096).contains(&len), "Origin read exceeds bound");
    m.checkpoint()?;
    let bytes = m.read(at, len)?;
    ensure!(bytes.len() == len, "Partial origin read");
    m.checkpoint()?;
    Ok(bytes)
}
fn profile(m: &impl Memory) -> Result<bool> {
    proof()?;
    for &(rva, len, hash) in PROFILE {
        if format!(
            "{:x}",
            Sha256::digest(read(
                m,
                m.base()
                    .checked_add(rva)
                    .context("Origin profile overflow")?,
                len
            )?)
        ) != hash
        {
            return Ok(false);
        }
    }
    Ok(true)
}
pub(crate) fn verify_profile(m: &impl Memory) -> Result<()> {
    ensure!(profile(m)?, "Origins native profile differs");
    Ok(())
}
fn catalog_id(state: &owner::OwnerSnapshot, header: &[u8; 16], bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(b"v2-native-origin-catalog-v1");
    for v in [state.root, state.owner, state.region.base, state.region.len] {
        h.update(v.to_le_bytes());
    }
    for d in [&state.players, &state.teams, &state.arenas]
        .into_iter()
        .chain(state.uniforms.as_ref())
    {
        h.update(d.pointer.to_le_bytes());
        h.update((d.count as u64).to_le_bytes());
        h.update((d.stride as u64).to_le_bytes());
    }
    h.update(header);
    h.update(bytes);
    h.update(crate::backend::unique_id().as_bytes());
    format!("{:x}", h.finalize())
}
fn header(m: &impl Memory, s: &owner::OwnerSnapshot) -> Result<[u8; 16]> {
    read(
        m,
        s.owner
            .checked_add(0x90)
            .context("Origin header overflow")?,
        16,
    )?
    .try_into()
    .map_err(|_| anyhow::anyhow!("Origin header length"))
}
fn span(h: &[u8; 16], stride: usize, max: usize) -> Option<Option<(u64, usize)>> {
    let count = u32::from_le_bytes(h[..4].try_into().ok()?) as usize;
    let at = u64::from_le_bytes(h[8..].try_into().ok()?);
    if count == 0 && at == 0 {
        return Some(None);
    }
    if count == 0 || count > max || at % 8 != 0 {
        return None;
    }
    Some(Some((at, count.checked_mul(stride)?)))
}
fn bounds(m: &impl Memory, s: &owner::OwnerSnapshot, at: u64, len: usize) -> Result<bool> {
    let start = s
        .region
        .base
        .checked_add(0x680)
        .context("Origin bounds overflow")?;
    let limit = s
        .region
        .base
        .checked_add(s.region.len)
        .context("Origin bounds overflow")?;
    let Some(end) = at.checked_add(len as u64) else {
        return Ok(false);
    };
    if at < start || end > limit {
        return Ok(false);
    }
    let mut spans = vec![(
        s.owner,
        s.owner
            .checked_add(0x2a0)
            .context("Origin owner overflow")?,
    )];
    for d in [&s.players, &s.teams, &s.arenas]
        .into_iter()
        .chain(s.uniforms.as_ref())
    {
        spans.push((
            d.pointer,
            d.pointer
                .checked_add(
                    d.count
                        .checked_mul(d.stride)
                        .context("Origin dependency overflow")? as u64,
                )
                .context("Origin dependency overflow")?,
        ));
    }
    // Reject overlap with supported adjacent name dictionaries as well. Unknown
    // dependency geometry is unavailable, never an invented writable target.
    for offset in [0x240, 0x250] {
        let h: [u8; 16] = read(
            m,
            s.owner
                .checked_add(offset)
                .context("Origin dependency overflow")?,
            16,
        )?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Origin dependency header"))?;
        let Some(value) = span(&h, 48, 20000) else {
            return Ok(false);
        };
        if let Some((p, n)) = value {
            let Some(e) = p.checked_add(n as u64) else {
                return Ok(false);
            };
            if p < start || e > limit {
                return Ok(false);
            }
            spans.push((p, e));
        }
    }
    Ok(!spans.iter().any(|(a, b)| at < *b && *a < end))
}
fn decode(bytes: &[u8]) -> Vec<(Option<String>, bool)> {
    bytes
        .chunks_exact(STRIDE)
        .map(|row| {
            let name = row[..34]
                .iter()
                .position(|b| *b == 0)
                .and_then(|end| std::str::from_utf8(&row[..end]).ok())
                .filter(|s| !s.is_empty() && !s.chars().any(char::is_control))
                .map(str::to_owned);
            let flags = u32::from_le_bytes(row[0x34..0x38].try_into().unwrap());
            let selectable = flags & 0x0c000000 != 0x0c000000;
            (name, selectable)
        })
        .collect()
}
/// Provisional witness: caller MUST revalidate after its base table comparisons
/// before publishing any joined document. Optional unsupported data is None;
/// all reads/checkpoints/owner failures remain errors.
pub(crate) fn capture(m: &impl Memory, s: &owner::OwnerSnapshot) -> Result<Option<Snapshot>> {
    owner::revalidate(m, s)?;
    let h = header(m, s)?;
    let Some(Some((at, len))) = span(&h, STRIDE, MAX_ROWS) else {
        owner::revalidate(m, s)?;
        return Ok(None);
    };
    if !profile(m)? || !bounds(m, s, at, len)? {
        owner::revalidate(m, s)?;
        return Ok(None);
    }
    let mut bytes = Vec::with_capacity(len);
    for offset in (0..len).step_by(4096) {
        bytes.extend(read(m, at + offset as u64, (len - offset).min(4096))?);
    }
    owner::revalidate(m, s)?;
    let rows = decode(&bytes);
    let choices_available = rows
        .iter()
        .any(|(name, selectable)| *selectable && name.is_some());
    Ok(Some(Snapshot {
        generation: s.clone(),
        id: catalog_id(s, &h, &bytes),
        header: h,
        pointer: at,
        rows,
        choices_available,
        bytes,
    }))
}
impl Snapshot {
    pub(crate) fn id(&self) -> &str {
        &self.id
    }
    pub(crate) fn catalog_id(&self) -> &str {
        &self.id
    }
    pub(crate) fn span(&self) -> (u64, usize) {
        (self.pointer, self.bytes.len())
    }
    pub(crate) fn revalidate(&self, m: &impl Memory, s: &owner::OwnerSnapshot) -> Result<()> {
        ensure!(
            s == &self.generation,
            "Origins belong to another roster generation"
        );
        owner::revalidate(m, s)?;
        ensure!(
            profile(m)?
                && header(m, s)? == self.header
                && bounds(m, s, self.pointer, self.bytes.len())?,
            "Origins profile/header/dependencies changed"
        );
        for (offset, expected) in self.bytes.chunks(4096).enumerate() {
            ensure!(
                read(m, self.pointer + offset as u64 * 4096, expected.len())? == expected,
                "Origin rows changed"
            );
        }
        ensure!(
            header(m, s)? == self.header
                && profile(m)?
                && bounds(m, s, self.pointer, self.bytes.len())?,
            "Origin header/profile changed during ending read"
        );
        owner::revalidate(m, s)
    }
    pub(crate) fn choices(&self) -> Value {
        json!(
            self.rows
                .iter()
                .enumerate()
                .filter_map(|(index, (name, selectable))| if *selectable {
                    name.as_ref().map(|name| json!({"index":index,"name":name}))
                } else {
                    None
                })
                .collect::<Vec<_>>()
        )
    }
    pub(crate) fn project(&self, row: &[u8]) -> Result<Value> {
        ensure!(
            row.len() == owner::PLAYER_STRIDE,
            "Incomplete player origin row"
        );
        let p = u64::from_le_bytes(row[0xb0..0xb8].try_into()?);
        if p == 0 {
            return Ok(
                json!({"originChoicesAvailable":self.choices_available,"nativeOriginCatalogId":self.id,"nativeOriginIndex":null,"originState":"unassigned","collegeName":null,"originSelectable":false}),
            );
        }
        let index = p
            .checked_sub(self.pointer)
            .filter(|d| *d % STRIDE as u64 == 0)
            .map(|d| (d / STRIDE as u64) as usize)
            .filter(|i| *i < self.rows.len());
        Ok(match index {
            Some(i) => {
                json!({"originChoicesAvailable":self.choices_available,"nativeOriginCatalogId":self.id,"nativeOriginIndex":i,"originState":"member","collegeName":self.rows[i].0,"originSelectable":self.rows[i].1&&self.rows[i].0.is_some()})
            }
            None => {
                json!({"originChoicesAvailable":self.choices_available,"nativeOriginCatalogId":self.id,"nativeOriginIndex":null,"originState":"invalid","collegeName":null,"originSelectable":false})
            }
        })
    }
}

/// Native-only external dependencies for a future existing-pointer Plan. This
/// is not a write lease and does not execute or authorize a memory write.
/// No PLAYER preimage is retained: the ordinary record Plan owns its evolving
/// before/after guards, so its own name/From effects do not invalidate this view.
#[derive(Clone, Debug)]
pub(crate) struct Selection {
    generation: Option<owner::OwnerSnapshot>,
    catalog_id: String,
    header: [u8; 16],
    pointer: u64,
    length: usize,
    original: u64,
    target: u64,
    index: usize,
    original_row: Option<Vec<u8>>,
    target_row: Vec<u8>,
}
impl Snapshot {
    /// Both rows must come from the native-owned snapshot and fresh live capture;
    /// caller first revalidates this same source generation. No browser row/path
    /// can mint a write capability, and this result alone is never a write lease.
    pub(crate) fn prepare_change(
        &self,
        cached_player: &[u8],
        live_player: &[u8],
        target_index: usize,
    ) -> Result<Selection> {
        ensure!(
            cached_player.len() == owner::PLAYER_STRIDE
                && live_player.len() == owner::PLAYER_STRIDE
                && cached_player == live_player,
            "Player changed before origin selection"
        );
        ensure!(
            target_index < self.rows.len()
                && self.rows[target_index].1
                && self.rows[target_index].0.is_some(),
            "Selected origin is unavailable or not selectable"
        );
        let original = u64::from_le_bytes(live_player[0xb0..0xb8].try_into()?);
        let original_row = if original == 0 {
            None
        } else {
            let offset = original
                .checked_sub(self.pointer)
                .filter(|offset| *offset % STRIDE as u64 == 0)
                .and_then(|offset| usize::try_from(offset).ok())
                .filter(|offset| *offset < self.bytes.len())
                .context("Current origin is not an exact member of this directory")?;
            Some(self.bytes[offset..offset + STRIDE].to_vec())
        };
        let offset = target_index
            .checked_mul(STRIDE)
            .context("Origin target overflow")?;
        Ok(Selection {
            generation: Some(self.generation.clone()),
            catalog_id: self.id.clone(),
            header: self.header,
            pointer: self.pointer,
            length: self.bytes.len(),
            original,
            target: self
                .pointer
                .checked_add(offset as u64)
                .context("Origin target overflow")?,
            index: target_index,
            original_row,
            target_row: self.bytes[offset..offset + STRIDE].to_vec(),
        })
    }
}
impl Selection {
    pub(crate) fn validate_catalog(&self, id: &str) -> Result<()> {
        ensure!(
            !id.is_empty() && id == self.catalog_id,
            "Origin catalog changed; refresh before saving"
        );
        Ok(())
    }
    pub(crate) fn validate_request(&self, index: u64, raw: &[u8]) -> Result<u64> {
        ensure!(
            raw.len() == owner::PLAYER_STRIDE && usize::try_from(index).ok() == Some(self.index),
            "Origin selection request differs"
        );
        ensure!(
            u64::from_le_bytes(raw[0xb0..0xb8].try_into()?) == self.original,
            "Current player origin changed"
        );
        Ok(self.target)
    }
    #[cfg(test)]
    pub(crate) fn for_test(old: u64, target: u64, index: usize) -> Self {
        Self {
            generation: None,
            catalog_id: "pure-planner-fixture".into(),
            header: [0; 16],
            pointer: 0,
            length: 0,
            original: old,
            target,
            index,
            original_row: None,
            target_row: vec![0; STRIDE],
        }
    }
    pub(crate) fn original_pointer(&self) -> u64 {
        self.original
    }
    pub(crate) fn target_pointer(&self) -> u64 {
        self.target
    }
    pub(crate) fn target_index(&self) -> usize {
        self.index
    }
    fn rows(&self, m: &impl Memory) -> Result<()> {
        if let Some(original) = &self.original_row {
            ensure!(
                read(m, self.original, STRIDE)? == *original,
                "Original origin row changed"
            );
        }
        ensure!(
            read(m, self.target, STRIDE)? == self.target_row,
            "Selected origin row changed"
        );
        Ok(())
    }
    /// Retain this guard through forward/final/reverse ordinary-record checks.
    /// Executor supplies its appropriate retained forward/rollback Memory context;
    /// no new-work consent or writable bound is minted by this read-only witness.
    pub(crate) fn revalidate(&self, m: &impl Memory, state: &owner::OwnerSnapshot) -> Result<()> {
        ensure!(
            self.generation.as_ref() == Some(state),
            "Origin selection belongs to another roster generation"
        );
        owner::revalidate(m, state)?;
        ensure!(
            profile(m)?
                && header(m, state)? == self.header
                && bounds(m, state, self.pointer, self.length)?,
            "Origin selection source changed"
        );
        self.rows(m)?;
        ensure!(
            header(m, state)? == self.header
                && profile(m)?
                && bounds(m, state, self.pointer, self.length)?,
            "Origin selection dependency changed during verification"
        );
        self.rows(m)?;
        ensure!(
            header(m, state)? == self.header
                && profile(m)?
                && bounds(m, state, self.pointer, self.length)?,
            "Origin selection changed during final row read"
        );
        owner::revalidate(m, state)
    }
}
#[cfg(test)]
#[path = "roster_v2_origins_tests.rs"]
mod tests;
