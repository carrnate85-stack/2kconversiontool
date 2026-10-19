//! Independently mapped V2 match descriptions and stock queue ownership.
//! This module only reads. No native function call, queue insertion or worker.
#![allow(dead_code)]
use crate::{
    blacktop_native::Memory,
    blacktop_outfits::{d, q, span},
    rules_v2,
};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
pub(crate) const TABLE: u64 = 0xAB5CE30;
pub(crate) const WRAPPER: u64 = 0x762D278;
pub(crate) const MANAGER: u64 = 0x68844D8;
pub(crate) const SECONDARY: u64 = 0x6884898;
pub(crate) const PLAYER: u64 = 0x6890D88;
pub(crate) const PRIVATE_DESCRIPTION: u64 = 0x68923C8;
pub(crate) const VARIABLE_OUTFIT: u64 = 0x68A2498;
pub(crate) const MANAGED_INSTANCE: u64 = 0x6884378;
pub(crate) const SENTINEL: u64 = 0x68840D8;
pub(crate) const ACTIVE: u64 = 0xA409E90;
pub(crate) const QUEUE: u64 = 0xA409FE8;
pub(crate) const QUEUE_SENTINEL: u64 = 0xA409FF0;
pub(crate) const QUEUE_LOCK: u64 = 0xA40A008;
pub(crate) const HEAP: u64 = 0xA40A0B8;
pub(crate) const MAIN_THREAD: u64 = 0x8A1F720;
pub(crate) const WRITES_SUPPORTED: bool = false;
#[derive(Deserialize)]
struct Method {
    rva: u64,
    bytes: usize,
    sha256: String,
    hex: String,
}
#[derive(Deserialize)]
struct Slot {
    offset: u64,
    method: u64,
}
#[derive(Deserialize)]
struct Table {
    rva: u64,
    slots: Vec<Slot>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Profile {
    schema: String,
    sha256: String,
    methods: Vec<Method>,
    virtual_tables: Vec<Table>,
    write_authority: bool,
}
fn profile() -> &'static Profile {
    static VALUE: OnceLock<Profile> = OnceLock::new();
    VALUE.get_or_init(|| {
        let p: Profile =
            serde_json::from_str(include_str!("../assets/outfit-match-v2-profile.json"))
                .expect("V2 match profile");
        assert_eq!(p.schema, "nba2k27-v2-outfit-match-method-profile-v1");
        assert_eq!(
            p.sha256,
            "29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1"
        );
        assert!(!p.write_authority && p.methods.len() == 25 && p.virtual_tables.len() == 10);
        p
    })
}
fn read(m: &impl Memory, at: u64, len: usize) -> Result<Vec<u8>> {
    span(at, len)?;
    ensure!(len > 0 && len <= 65536, "Bounded V2 match read");
    let mut out = Vec::with_capacity(len);
    for offset in (0..len).step_by(4096) {
        m.checkpoint()?;
        let n = (len - offset).min(4096);
        let b = m.read(at + offset as u64, n)?;
        ensure!(b.len() == n, "Partial V2 match read");
        out.extend(b);
    }
    Ok(out)
}

#[cfg(debug_assertions)]
#[path = "outfit_match_v2_shadow.rs"]
pub(crate) mod shadow;
#[cfg(test)]
#[path = "outfit_match_v2_tests.rs"]
mod tests;

pub(crate) fn verify_profile(m: &impl Memory) -> Result<()> {
    rules_v2::verify_profile(m)?;
    for row in &profile().methods {
        ensure!(
            row.rva >= 4096
                && row.rva < 0x5D00000
                && (1..=65536).contains(&row.bytes)
                && row.hex.len() == row.bytes * 2,
            "V2 method bounds"
        );
        ensure!(
            format!(
                "{:X}",
                Sha256::digest(read(m, m.base() + row.rva, row.bytes)?)
            ) == row.sha256,
            "V2 outfit native ownership method changed"
        );
    }
    for t in &profile().virtual_tables {
        let first = t
            .slots
            .iter()
            .map(|s| s.offset)
            .min()
            .context("Empty V2 class table")?;
        let end = t
            .slots
            .iter()
            .map(|s| s.offset)
            .max()
            .context("Empty V2 class table")?
            .checked_add(8)
            .context("V2 class table overflow")?;
        ensure!(end > first && end - first <= 4096, "V2 class table bounds");
        // One fresh bounded image read per table, with the ordinary retained
        // identity/region guards. Every expected slot is still compared on
        // every capture; no profile or live ownership proof is cached.
        let bytes = read(m, m.base() + t.rva + first, (end - first) as usize)?;
        for s in &t.slots {
            let offset = (s.offset - first) as usize;
            ensure!(
                u64::from_le_bytes(bytes[offset..offset + 8].try_into()?) == m.base() + s.method,
                "V2 match virtual class changed"
            );
        }
    }
    m.checkpoint()
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Description {
    pub address: u64,
    pub bytes: Vec<u8>,
    pub array: u64,
    pub owner: u64,
    pub owned_array: u64,
    pub count: u32,
    pub items: Vec<u64>,
    pub names: Vec<String>,
}
/// Private-clone ownership must be independently established before native
/// setters/disposal. Never dispose an alias into the actor or any live owner.
pub(crate) fn independent(source: &Description, copy: &Description, actor: u64) -> Result<()> {
    ensure!(
        source.bytes.len() == 0x2970
            && copy.bytes.len() == 0x2970
            && source.address != copy.address
            && !(actor..actor.checked_add(0x38C0).context("Actor span overflow")?)
                .contains(&copy.address)
            && source.array != copy.array
            && source.owner != copy.owner
            && source.owned_array != copy.owned_array
            && source.array != copy.owned_array
            && source.owned_array != copy.array,
        "V2 clone aliases a live actor/wardrobe owner"
    );
    let fields = [0x368, 0x370, 0x2940, 0x2948, 0x2950, 0x2958];
    for x in fields {
        for y in fields {
            let a = u64::from_le_bytes(source.bytes[x..x + 8].try_into()?);
            let b = u64::from_le_bytes(copy.bytes[y..y + 8].try_into()?);
            ensure!(
                a == 0 || a != b,
                "V2 clone destructor-owned allocations alias"
            );
        }
    }
    ensure!(
        source
            .items
            .iter()
            .all(|a| copy.items.iter().all(|b| a != b)),
        "V2 clone shares owned strings with source"
    );
    Ok(())
}
/// Expansion may change only pointer/count in the description. This compares
/// immutable snapshots; it does not mutate a native list or its header.
pub(crate) fn expanded(before: &Description, after: &Description, names: &[String]) -> Result<()> {
    ensure!(
        (2..=7).contains(&names.len())
            && before.address == after.address
            && before.owner == after.owner
            && before.bytes.len() == 0x2970
            && after.bytes.len() == 0x2970
            && after.count as usize == names.len()
            && after.names == names,
        "V2 expanded wardrobe identity differs"
    );
    let mut bytes = after.bytes.clone();
    bytes[0xC8..0xD4].copy_from_slice(&before.bytes[0xC8..0xD4]);
    ensure!(
        bytes == before.bytes,
        "V2 expansion changed unrelated description fields"
    );
    Ok(())
}
fn name(m: &impl Memory, at: u64) -> Result<String> {
    let b = read(m, at, 512)?;
    let w = b
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect::<Vec<_>>();
    let end = w
        .iter()
        .position(|n| *n == 0)
        .filter(|n| *n > 0)
        .context("Unterminated V2 clothing name")?;
    let s = String::from_utf16(&w[..end])?;
    ensure!(
        crate::blacktop_outfits::valid_item(&s),
        "Invalid V2 clothing asset"
    );
    Ok(s)
}
// One-call-only bounded transport. A failed wider read retries every original
// scalar field, never mixing partial bytes or retaining a proof for later use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WardrobeOwner {
    tag: u64,
    count: u32,
    array: u64,
    owned_array: u64,
}
fn wardrobe_owner(m: &impl Memory, at: u64) -> Result<WardrobeOwner> {
    if let Ok(bytes) = read(m, at, 40) {
        return Ok(WardrobeOwner {
            tag: u64::from_le_bytes(bytes[0..8].try_into()?),
            count: u32::from_le_bytes(bytes[8..12].try_into()?),
            array: u64::from_le_bytes(bytes[0x10..0x18].try_into()?),
            owned_array: u64::from_le_bytes(bytes[0x20..0x28].try_into()?),
        });
    }
    Ok(WardrobeOwner {
        tag: q(m, at)?,
        count: d(m, at + 8)?,
        array: q(m, at + 0x10)?,
        owned_array: q(m, at + 0x20)?,
    })
}
fn wardrobe_items(m: &impl Memory, at: u64, count: u32) -> Result<Vec<u64>> {
    ensure!((1..=7).contains(&count), "V2 wardrobe item bounds");
    if let Ok(bytes) = read(m, at, count as usize * 8) {
        return bytes
            .chunks_exact(8)
            .map(|word| Ok(u64::from_le_bytes(word.try_into()?)))
            .collect();
    }
    (0..u64::from(count)).map(|n| q(m, at + n * 8)).collect()
}
pub(crate) fn description(m: &impl Memory, at: u64) -> Result<Description> {
    let bytes = read(m, at, 0x2970)?;
    // These fields are already present in the complete initial snapshot. Prove
    // them BEFORE following any captured pointers; the complete ending image
    // below must still match byte-for-byte, including all opaque fields.
    let field = |offset: usize| -> Result<u64> {
        Ok(u64::from_le_bytes(bytes[offset..offset + 8].try_into()?))
    };
    ensure!(
        field(0)? == m.base() + PRIVATE_DESCRIPTION
            && field(0xB8)? == at + 0x378
            && bytes[0x7D] & 1 == 0,
        "V2 private player description differs"
    );
    let array = field(0xC8)?;
    let count = u32::from_le_bytes(bytes[0xD0..0xD4].try_into()?);
    let owner = field(0x368)?;
    ensure!((1..=7).contains(&count), "V2 owned wardrobe differs");
    let owner_fields = wardrobe_owner(m, owner)?;
    ensure!(
        owner_fields.tag == m.base() + VARIABLE_OUTFIT
            && owner_fields.array == array
            && owner_fields.count == count,
        "V2 owned wardrobe differs"
    );
    let owned_array = owner_fields.owned_array;
    let items = wardrobe_items(m, array, count)?;
    let owned_items = wardrobe_items(m, owned_array, count)?;
    ensure!(items == owned_items, "Exposed/owned wardrobe item differs");
    let names = items
        .iter()
        .map(|pointer| name(m, *pointer))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        names[0] != "VTXANI_JERSEY" && read(m, at, 0x2970)? == bytes,
        "Ordinary wardrobe changed during snapshot"
    );
    // Preserve fresh endpoint ownership even if external arrays changed while
    // names were read. Only the four previously proven owner fields matter;
    // unrelated owner padding is not promoted to a new stability requirement.
    ensure!(
        wardrobe_owner(m, owner)? == owner_fields
            && wardrobe_items(m, array, count)? == items
            && wardrobe_items(m, owned_array, count)? == owned_items,
        "V2 owned wardrobe changed during snapshot"
    );
    Ok(Description {
        address: at,
        bytes,
        array,
        owner,
        owned_array,
        count,
        items,
        names,
    })
}
/// Audited primary/secondary adjustment: do not add78 to fields twice.
pub(crate) fn idle(m: &impl Memory, primary: u64, actor: u64, label: u32) -> Result<()> {
    ensure!(
        q(m, primary)? == m.base() + MANAGER
            && q(m, primary + 0x78)? == m.base() + SECONDARY
            && q(m, primary + 0x110)? == actor
            && q(m, primary + 0x118)? == 0
            && d(m, primary + 0x90)? == 1
            && read(m, primary + 0x130, 1)? == [0],
        "V2 manager is not idle"
    );
    let sentinel = primary + 0xF8;
    ensure!(
        q(m, sentinel)? == m.base() + SENTINEL,
        "V2 managed-instance sentinel differs"
    );
    let mut node = q(m, sentinel + 0x10)?;
    let mut previous = sentinel;
    let mut matching = 0;
    for _ in 0..16 {
        if node == sentinel {
            ensure!(
                matching == 1 && q(m, sentinel + 8)? == previous,
                "V2 managed-instance list incomplete"
            );
            return Ok(());
        }
        ensure!(
            q(m, node)? == m.base() + MANAGED_INSTANCE
                && q(m, node + 8)? == previous
                && q(m, node + 0x38)? == 0,
            "V2 managed-instance node differs"
        );
        if d(m, node + 0x18)? == label {
            ensure!(
                q(m, node + 0x20)? == actor,
                "V2 managed-instance actor differs"
            );
            matching += 1;
        }
        previous = node;
        node = q(m, node + 0x10)?;
    }
    anyhow::bail!("V2 managed-instance list exceeds bounds")
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Player {
    pub row: u64,
    /// Raw manager pointer at row+0x10 and loaded+0x518. Live V2 stores the
    /// MANAGER_INSTANCE interface base (SECONDARY vtable) there; `primary` is
    /// always the full object, as in scaler_v2.
    pub table_manager: u64,
    pub primary: u64,
    pub actor: u64,
    pub loaded_roster: u64,
    pub original_roster: u64,
    pub state: u64,
    pub description: Description,
}
pub(crate) fn queue_ready(m: &impl Memory, empty: bool) -> Result<()> {
    let queue = m.base() + QUEUE;
    let head = m.base() + QUEUE_SENTINEL;
    ensure!(
        q(m, queue)? == m.base() + 0x68B86F8
            && q(m, head)? == m.base() + 0x6860B08
            && read(m, queue + 0x5A, 1)? == [1],
        "V2 create queue is uninitialized or changed"
    );
    if empty {
        ensure!(
            q(m, head + 8)? == head && q(m, head + 0x10)? == head,
            "V2 create queue is busy"
        );
    }
    Ok(())
}
pub(crate) fn off_main_thread(m: &impl Memory, current_thread: u32) -> Result<()> {
    let main = d(m, m.base() + MAIN_THREAD)?;
    ensure!(
        main != 0 && current_thread != 0 && main != current_thread,
        "V2 outfit worker is not on an independently created worker thread"
    );
    Ok(())
}
fn active_member(m: &impl Memory, primary: u64) -> Result<()> {
    let head = m.base() + ACTIVE;
    ensure!(
        q(m, head)? == m.base() + 0x687E1E8,
        "V2 active manager sentinel differs"
    );
    let mut node = q(m, head + 0x10)?;
    let mut previous = head;
    let mut matches = 0;
    for _ in 0..256 {
        if node == head {
            ensure!(
                matches == 1 && q(m, head + 8)? == previous,
                "V2 active manager membership differs"
            );
            return Ok(());
        }
        ensure!(
            q(m, node + 8)? == previous,
            "V2 active manager backlink differs"
        );
        if node == primary {
            matches += 1;
        }
        previous = node;
        node = q(m, node + 0x10)?;
    }
    anyhow::bail!("V2 active manager list exceeds bounds")
}
pub(crate) fn capture(m: &impl Memory, originals: &[u64]) -> Result<Vec<Player>> {
    ensure!(
        !originals.is_empty()
            && originals.len() <= 10
            && originals
                .iter()
                .enumerate()
                .all(|(i, v)| !originals[..i].contains(v)),
        "V2 match original roster identities invalid"
    );
    verify_profile(m)?;
    queue_ready(m, true)?;
    let before = rules_v2::context(m)?;
    ensure!(
        rules_v2::boundary(&before, m.base()) == "match",
        "Wait for loaded Blacktop match"
    );
    // The 30 wrapper records form one bounded image span. Read it once
    // instead of issuing a guarded process read for each slot/field. Fresh
    // ending context and per-player binding/description checks below remain
    // independent reads, so replacement while capturing still rejects.
    let table = read(m, m.base() + TABLE, 30 * 0x50)?;
    let mut out = Vec::new();
    for n in 0..30usize {
        let row = m.base() + TABLE + n as u64 * 0x50;
        let record = &table[n * 0x50..(n + 1) * 0x50];
        let word =
            |offset: usize| u64::from_le_bytes(record[offset..offset + 8].try_into().unwrap());
        if word(0) != m.base() + WRAPPER {
            continue;
        }
        let table_manager = word(0x10);
        let loaded = word(0x28);
        if table_manager == 0 || loaded == 0 {
            continue;
        }
        let original = q(m, loaded + 0x500)?;
        if !originals.contains(&original) {
            continue;
        }
        ensure!(
            q(m, loaded + 0x518)? == table_manager,
            "V2 roster-manager binding differs"
        );
        let primary = if q(m, table_manager)? == m.base() + SECONDARY {
            table_manager
                .checked_sub(0x78)
                .context("V2 manager interface underflow")?
        } else {
            table_manager
        };
        ensure!(
            q(m, primary)? == m.base() + MANAGER && q(m, primary + 0x78)? == m.base() + SECONDARY,
            "V2 roster-manager binding differs"
        );
        ensure!(
            !out.iter()
                .any(|p: &Player| p.original_roster == original || p.primary == primary),
            "V2 match identity ambiguous"
        );
        active_member(m, primary)?;
        let state = word(0x18);
        ensure!(
            state != 0 && q(m, loaded + 0x520)? == state && record[0x20] == 1,
            "V2 wrapper state differs"
        );
        let actor = q(m, primary + 0x110)?;
        ensure!(
            q(m, actor)? == m.base() + PLAYER,
            "V2 match actor class differs"
        );
        let description = description(m, actor + 0xF30)?;
        let label = d(m, description.address + 0x88)?;
        ensure!(label != 0, "V2 description label absent");
        idle(m, primary, actor, label)?;
        out.push(Player {
            row,
            table_manager,
            primary,
            actor,
            loaded_roster: loaded,
            original_roster: original,
            state,
            description,
        });
    }
    ensure!(
        out.len() == originals.len() && rules_v2::context(m)? == before,
        "V2 match players not stably loaded"
    );
    for p in &out {
        ensure!(
            q(m, p.row)? == m.base() + WRAPPER
                && q(m, p.row + 0x10)? == p.table_manager
                && q(m, p.row + 0x18)? == p.state
                && read(m, p.row + 0x20, 1)? == [1]
                && q(m, p.loaded_roster + 0x518)? == p.table_manager
                && q(m, p.row + 0x28)? == p.loaded_roster
                && q(m, p.loaded_roster + 0x500)? == p.original_roster
                && q(m, p.primary + 0x110)? == p.actor
                && description(m, p.description.address)? == p.description,
            "V2 match binding changed"
        );
    }
    queue_ready(m, true)?;
    Ok(out)
}
