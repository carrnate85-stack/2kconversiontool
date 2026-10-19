//! Disk-derived Version 2 Blacktop court-presentation planners. Never opens a
//! process or writes. Consumers must validate exact retained game identity,
//! private committed RW/non-executable allocations, journal before each write,
//! and compare the complete snapshot immediately before/after each operation.
//! Drone/DJ/MC choices are immutable for the game process. Crowd is visit-scoped:
//! prepare only at selection, restore only at inactive main/Quick Play menus.
#![allow(dead_code)]
use crate::{
    blacktop_native::{ContextState, Memory},
    rules_v2,
};
use anyhow::{Context, Result, ensure};

pub(crate) const SETTINGS_OWNER: u64 = 0x0AAE33D0;
pub(crate) const PRELOAD_TABLE: u64 = 0x0AB5C468;
const WINDOW: u64 = 0x2C48;
const FRONT: u64 = 0x7428;
const AMBIENT: u64 = 0x73F8;
const STRIDE: u64 = 0xC810;
const SHA: &str = "29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1";
fn at(m: &impl Memory, offset: u64) -> Result<u64> {
    m.base()
        .checked_add(offset)
        .context("Presentation RVA overflow")
}
fn read(m: &impl Memory, address: u64, len: usize) -> Result<Vec<u8>> {
    ensure!(
        len > 0
            && len <= 4096
            && address >= 0x10000
            && address
                .checked_add(len as u64)
                .is_some_and(|end| end <= 0x7fff00000000),
        "Invalid bounded presentation read"
    );
    m.checkpoint()?;
    let bytes = m.read(address, len)?;
    ensure!(bytes.len() == len, "Partial presentation read");
    Ok(bytes)
}
fn q(m: &impl Memory, offset: u64) -> Result<u64> {
    Ok(u64::from_le_bytes(
        read(m, at(m, offset)?, 8)?.as_slice().try_into()?,
    ))
}
fn valid_pointer(address: u64, span: u64) -> bool {
    address >= 0x10000
        && address % 8 == 0
        && address
            .checked_add(span)
            .is_some_and(|end| end <= 0x7fff00000000)
}
fn hex(s: &str) -> Result<Vec<u8>> {
    ensure!(
        s.len() % 2 == 0 && s.len() <= 512,
        "Invalid presentation anchor hex"
    );
    s.as_bytes()
        .chunks_exact(2)
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}
fn catalog() -> Result<serde_json::Value> {
    let value: serde_json::Value =
        serde_json::from_str(include_str!("../assets/presentation-v2-anchors.json"))?;
    ensure!(
        value["schema"] == "nba2k27-v2-blacktop-presentation-anchors-v1"
            && value["sha256"] == SHA
            && value["settingsOwnerRva"].as_u64() == Some(SETTINGS_OWNER)
            && value["preloadTableRva"].as_u64() == Some(PRELOAD_TABLE)
            && value["recordStride"].as_u64() == Some(STRIDE)
            && value["gameProcessAccess"] == false,
        "Invalid presentation reference"
    );
    Ok(value)
}
/// Independent direct/native/global getters plus native preload-row arithmetic,
/// five-size range check, 175+size preset selection and front-actor consumption.
pub(crate) fn verify_profile(m: &impl Memory) -> Result<()> {
    verify_anchors(m, 12)
}
pub(crate) fn verify_actor_profile(m: &impl Memory) -> Result<()> {
    verify_anchors(m, 5)
}
fn verify_anchors(m: &impl Memory, count: usize) -> Result<()> {
    rules_v2::verify_profile(m)?;
    let value = catalog()?;
    let anchors = value["anchors"]
        .as_array()
        .context("Missing presentation anchors")?;
    ensure!(anchors.len() == 12, "Presentation proof count changed");
    for anchor in &anchors[..count] {
        let rva = anchor["rva"]
            .as_u64()
            .context("Missing presentation anchor address")?;
        let bytes = hex(anchor["hex"]
            .as_str()
            .context("Missing presentation anchor bytes")?)?;
        ensure!(
            (0x1000..0x5D00000).contains(&rva) && read(m, at(m, rva)?, bytes.len())? == bytes,
            "Version 2 court-presentation anchor changed: {}",
            anchor["label"]
        );
    }
    m.checkpoint()
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Source {
    pub index: u32,
    pub address: u64,
    pub front: Vec<u8>,
    pub ambient: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub base: u64,
    pub context: ContextState,
    pub owner: u64,
    pub preload_table: u64,
    pub window: Vec<u8>,
    pub sources: Vec<Source>,
}
fn safe_boundary(snapshot: &Snapshot) -> &'static str {
    rules_v2::boundary(&snapshot.context, snapshot.base)
}
fn shape(snapshot: &Snapshot, require_crowd: bool) -> Result<()> {
    ensure!(
        snapshot.base >= 0x10000
            && snapshot
                .base
                .checked_add(0x35BDE000)
                .is_some_and(|end| end <= 0x7fff00000000)
            && valid_pointer(snapshot.owner, WINDOW + 16)
            && snapshot.window.len() == 16,
        "Incomplete presentation snapshot"
    );
    ensure!(
        snapshot.window[1] <= 1 && snapshot.window[6..12].iter().all(|v| *v <= 1),
        "Invalid presentation boolean snapshot"
    );
    if !require_crowd && snapshot.sources.is_empty() {
        ensure!(
            snapshot.preload_table == 0,
            "Unexpected actor-only preload pointer"
        );
        return Ok(());
    }
    ensure!(
        valid_pointer(snapshot.preload_table, 180 * STRIDE + 8 + 0x7448)
            && snapshot.sources.len() == 5,
        "Incomplete crowd snapshot"
    );
    for (index, source) in snapshot.sources.iter().enumerate() {
        ensure!(
            source.index == 175 + index as u32
                && source.address == snapshot.preload_table + source.index as u64 * STRIDE + 8
                && source.front.len() == 24
                && source.ambient.len() == 24,
            "Incomplete crowd preset snapshot"
        );
        ensure!(
            source.front[0] == 0
                && source.front[1] == 0x4C
                && source.front[8] <= 1
                && source.ambient[1] == 0x4D
                && source.ambient[8] <= 1,
            "Invalid crowd preset snapshot"
        );
    }
    Ok(())
}
fn capture_data(m: &impl Memory, context: ContextState, crowd: bool) -> Result<Snapshot> {
    let owner = q(m, SETTINGS_OWNER)?;
    let table = if crowd { q(m, PRELOAD_TABLE)? } else { 0 };
    ensure!(
        valid_pointer(owner, WINDOW + 16)
            && (!crowd || valid_pointer(table, 180 * STRIDE + 8 + 0x7448))
            && !(m.base()..m.base() + 0x35BDE000).contains(&owner)
            && (!crowd || !(m.base()..m.base() + 0x35BDE000).contains(&table)),
        "Presentation owners are not supported private pointers"
    );
    ensure!(
        read(m, owner, 1)? == [0],
        "Presentation settings owner is unsupported"
    );
    let window = read(m, owner + WINDOW, 16)?;
    ensure!(
        window[1] <= 1 && window[6..12].iter().all(|value| *value <= 1),
        "Presentation boolean layout changed"
    );
    let mut sources = Vec::new();
    for index in (175u32..=179).filter(|_| crowd) {
        let address = table + index as u64 * STRIDE + 8;
        ensure!(
            u32::from_le_bytes(read(m, address, 4)?.as_slice().try_into()?) == index,
            "Waiting for Blacktop crowd preset {index} to initialize"
        );
        let front = read(m, address + FRONT, 24)?;
        let ambient = read(m, address + AMBIENT, 24)?;
        ensure!(
            front[0] == 0
                && front[1] == 0x4C
                && front[8] <= 1
                && ambient[1] == 0x4D
                && ambient[8] <= 1,
            "Blacktop standing-crowd preset layout changed"
        );
        sources.push(Source {
            index,
            address,
            front,
            ambient,
        });
    }
    Ok(Snapshot {
        base: m.base(),
        context,
        owner,
        preload_table: table,
        window,
        sources,
    })
}
/// Transient preload owners are never read during match/loading. A caller can
/// use the separate read-only boundary observer to wait between these samples.
pub(crate) fn capture(m: &impl Memory) -> Result<Snapshot> {
    capture_kind(m, true)
}
pub(crate) fn capture_actors(m: &impl Memory) -> Result<Snapshot> {
    capture_kind(m, false)
}
/// Debounced in rules_v2::context (sub-millisecond selection mode blips).
pub(crate) fn settled_context(m: &impl Memory) -> Result<ContextState> {
    rules_v2::context(m)
}
fn capture_kind(m: &impl Memory, crowd: bool) -> Result<Snapshot> {
    if crowd {
        verify_profile(m)?;
    } else {
        verify_actor_profile(m)?;
    }
    let context = settled_context(m)?;
    let boundary = rules_v2::boundary(&context, m.base());
    ensure!(
        matches!(boundary, "selection" | "main-menu" | "quick-play"),
        "Presentation waits for stable Blacktop selection or an inactive exit menu"
    );
    let before = capture_data(m, context, crowd)?;
    ensure!(
        settled_context(m)? == before.context
            && capture_data(m, before.context.clone(), crowd)? == before,
        "Presentation owners, flags or preset rows changed during reading"
    );
    ensure!(
        settled_context(m)? == before.context,
        "Presentation mode changed after reading"
    );
    m.checkpoint()?;
    Ok(before)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Drone,
    Dj,
    Mc,
    McCoupled,
    CrowdGlobal,
    CrowdStanding(u32),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Change {
    pub at: u64,
    pub before: u8,
    pub after: u8,
    pub kind: Kind,
}
#[derive(Clone, Debug)]
pub(crate) struct Transaction {
    pub before: Snapshot,
    pub after: Snapshot,
    pub changes: Vec<Change>,
    pub mask: Option<u8>,
}
pub(crate) fn presentation(snapshot: &Snapshot, mask: u8) -> Result<Transaction> {
    shape(snapshot, false)?;
    ensure!(mask <= 7, "Invalid court-presentation selection");
    ensure!(
        safe_boundary(snapshot) == "selection",
        "Court presentation applies only at Blacktop player selection"
    );
    let mut result = Transaction {
        before: snapshot.clone(),
        after: snapshot.clone(),
        changes: Vec::new(),
        mask: Some(mask),
    };
    for (bit, index, kind) in [
        (1, 6, Kind::Drone),
        (2, 7, Kind::Dj),
        (4, 8, Kind::Mc),
        (4, 11, Kind::McCoupled),
    ] {
        if mask & bit != 0 && snapshot.window[index] != 1 {
            result.changes.push(Change {
                at: snapshot.owner + WINDOW + index as u64,
                before: snapshot.window[index],
                after: 1,
                kind,
            });
            result.after.window[index] = 1;
        }
    }
    Ok(result)
}
pub(crate) fn crowd(snapshot: &Snapshot) -> Result<Transaction> {
    shape(snapshot, true)?;
    ensure!(
        safe_boundary(snapshot) == "selection",
        "Crowd preparation applies only at Blacktop player selection"
    );
    ensure!(
        snapshot.sources.len() == 5
            && snapshot
                .sources
                .iter()
                .enumerate()
                .all(|(i, s)| s.index == 175 + i as u32
                    && s.address == snapshot.preload_table + s.index as u64 * STRIDE + 8),
        "Crowd preset set changed"
    );
    let mut result = Transaction {
        before: snapshot.clone(),
        after: snapshot.clone(),
        changes: Vec::new(),
        mask: None,
    };
    if snapshot.window[1] != 0 {
        result.changes.push(Change {
            at: snapshot.owner + WINDOW + 1,
            before: snapshot.window[1],
            after: 0,
            kind: Kind::CrowdGlobal,
        });
        result.after.window[1] = 0;
    }
    for (source, after) in snapshot.sources.iter().zip(&mut result.after.sources) {
        if source.front[8] != 0 {
            result.changes.push(Change {
                at: source.address + FRONT + 8,
                before: source.front[8],
                after: 0,
                kind: Kind::CrowdStanding(source.index),
            });
            after.front[8] = 0;
        }
    }
    Ok(result)
}
/// Drone/DJ/MC bytes in the same 16-byte window. The game clears them at
/// every match end (live 0.47/0.48), so crowd ownership treats them as
/// game-owned booleans, not neighbors. WPF's crowd restore never compared the
/// window neighbors at all; every other byte stays exact.
const ACTOR_BYTES: [usize; 4] = [6, 7, 8, 11];
pub(crate) fn crowd_window_neighbors(original: &[u8], current: &[u8]) -> bool {
    original.len() == 16
        && current.len() == 16
        && (0..16).all(|i| {
            i == 1
                || if ACTOR_BYTES.contains(&i) {
                    original[i] <= 1 && current[i] <= 1
                } else {
                    original[i] == current[i]
                }
        })
}
fn unselected_equal(before: &[u8], after: &[u8], selected: usize) -> bool {
    before.len() == after.len()
        && before
            .iter()
            .zip(after)
            .enumerate()
            .all(|(index, (a, b))| index == selected || a == b)
}
pub(crate) fn restore_crowd(current: &Snapshot, owned: &Transaction) -> Result<Transaction> {
    shape(current, true)?;
    shape(&owned.before, true)?;
    shape(&owned.after, true)?;
    ensure!(
        owned.mask.is_none() && matches!(safe_boundary(current), "main-menu" | "quick-play"),
        "Crowd restoration waits for the inactive main or Quick Play menu"
    );
    let original = &owned.before;
    ensure!(
        safe_boundary(original) == "selection"
            && owned.after.context == original.context
            && owned.after.owner == original.owner
            && owned.after.preload_table == original.preload_table
            && owned.after.window[1] == 0
            && unselected_equal(&original.window, &owned.after.window, 1)
            && owned
                .after
                .sources
                .iter()
                .zip(&original.sources)
                .all(|(after, before)| after.address == before.address
                    && after.ambient == before.ambient
                    && after.front[8] == 0
                    && unselected_equal(&after.front, &before.front, 8)),
        "Invalid crowd ownership transaction"
    );
    ensure!(
        current.base == original.base
            && current.owner == original.owner
            && current.preload_table == original.preload_table
            && current.sources.len() == 5
            && crowd_window_neighbors(&original.window, &current.window)
            && (current.window[1] == 0 || current.window[1] == original.window[1]),
        "Crowd global ownership or neighboring values changed"
    );
    let mut result = Transaction {
        before: current.clone(),
        after: current.clone(),
        changes: Vec::new(),
        mask: None,
    };
    if current.window[1] != original.window[1] {
        result.changes.push(Change {
            at: current.owner + WINDOW + 1,
            before: current.window[1],
            after: original.window[1],
            kind: Kind::CrowdGlobal,
        });
        result.after.window[1] = original.window[1];
    }
    for ((source, first), after) in current
        .sources
        .iter()
        .zip(&original.sources)
        .zip(&mut result.after.sources)
    {
        ensure!(
            source.index == first.index
                && source.address == first.address
                && source.ambient == first.ambient
                && unselected_equal(&first.front, &source.front, 8)
                && (source.front[8] == 0 || source.front[8] == first.front[8]),
            "Standing-crowd owner or neighboring preset bytes changed"
        );
        if source.front[8] != first.front[8] {
            result.changes.push(Change {
                at: source.address + FRONT + 8,
                before: source.front[8],
                after: first.front[8],
                kind: Kind::CrowdStanding(source.index),
            });
            after.front[8] = first.front[8];
        }
    }
    Ok(result)
}
pub(crate) fn verify_expected(m: &impl Memory, expected: &Snapshot) -> Result<()> {
    ensure!(
        capture_kind(m, !expected.sources.is_empty())? == *expected,
        "Presentation context or owners changed before write/readback"
    );
    Ok(())
}

#[cfg(test)]
#[path = "presentation_v2_tests.rs"]
mod tests;
