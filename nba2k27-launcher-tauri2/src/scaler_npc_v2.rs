//! Version 2 scaler character groups: coaches, cheerleaders and arena
//! personnel. Read-only scan and bounded write planning; this module cannot
//! open processes or write. Parity with the WPF helper: a selected character
//! opts into the game's own shared size input (description +0x7D bit 2, the
//! bit players and natively opted-in referees already carry). Already-loaded
//! cheerleaders/arena characters get one cache seed, exactly what the native
//! evaluator computes (authored scale x shared input; proved live: an opted-in
//! referee caches 0.93937 = stock 0.98819 x 0.9506). Coaches are left to the
//! game's own refresh, as in WPF. Characters already carrying the bit (e.g.
//! referees) are never adopted or changed.
//!
//! Version 2 layout (probed live, read-only, and pinned to disk RTTI): each
//! MANAGER_INSTANCE is its own list node (+8 previous, +0x10 next) under an
//! image sentinel; actor at +0x110, state byte at +0x130. All three classes'
//! interface vtable slot +0x190 is the same evaluator players use
//! (0x6E4630), which tests the opt-in bit before reading the shared input.
#![allow(dead_code)]
use crate::{blacktop_native::Memory, scaler_policy, scaler_v2};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::collections::BTreeMap;

pub(crate) const LIST_SENTINEL: u64 = 0x0A40_9E90;
pub(crate) const NODE_ACTOR: u64 = 0x110;
pub(crate) const NODE_STATE: u64 = 0x130;
pub(crate) const OPT_IN: u64 = 0x7D;
pub(crate) const OPT_IN_BIT: u8 = 2;
const INTERFACE: u64 = 0x78;
const DESCRIPTION_GETTER: u64 = 0x158;
const EVALUATOR_SLOT: u64 = 0x190;
const EVALUATOR: u64 = 0x006E_4630;
const MAX_NODES: usize = 1024;
const MANAGER_NAME: &str = ".?AVMANAGER_INSTANCE@CHARACTER@@";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Group {
    Coach,
    Cheerleader,
    Arena,
    /// Reserved diagnostic class; NPC adoption is withheld until a positive
    /// celebrity identity is proved. Absence from wrappers is insufficient:
    /// basketball PLAYER actors can exist before wrapper binding.
    Celebrity,
}
impl Group {
    /// The native evaluator refreshes coaches itself; arena characters that
    /// are already loaded keep a stale cache until seeded once (WPF parity).
    fn seeded(self) -> bool {
        !matches!(self, Self::Coach)
    }
}
pub(crate) struct Class {
    pub group: Group,
    pub actor: u64,
    pub interface: u64,
    pub description: u64,
    /// Second accepted description class (PLAYER uses public and private).
    pub alt_description: u64,
    pub description_offset: u64,
    pub getter: u64,
    pub actor_name: &'static str,
    pub description_name: &'static str,
    pub alt_description_name: &'static str,
}
pub(crate) const CLASSES: [Class; 4] = [
    Class {
        group: Group::Coach,
        actor: 0x0686_C518,
        interface: 0x0686_CD28,
        description: 0x0686_BED8,
        alt_description: 0,
        description_offset: 0xF30,
        getter: 0x0081_1790,
        actor_name: ".?AVCOACH@CHARACTER@@",
        description_name: ".?AVCOACH_DESCRIPTION_PRIVATE@CHARACTER@@",
        alt_description_name: "",
    },
    Class {
        group: Group::Cheerleader,
        actor: 0x0686_6488,
        interface: 0x0686_6C58,
        description: 0x068B_B028,
        alt_description: 0,
        description_offset: 0xED0,
        getter: 0x0080_EED0,
        actor_name: ".?AVCHEERLEADER@CHARACTER@@",
        description_name: ".?AVCHEERLEADER_DESCRIPTION_PRIVATE@CHARACTER@@",
        alt_description_name: "",
    },
    Class {
        group: Group::Arena,
        actor: 0x0687_C888,
        interface: 0x0687_D098,
        description: 0x0687_C5B8,
        alt_description: 0,
        description_offset: 0xF30,
        getter: 0x0081_1790,
        actor_name: ".?AVGENERIC@CHARACTER@@",
        description_name: ".?AVGENERIC_DESCRIPTION_PRIVATE@CHARACTER@@",
        alt_description_name: "",
    },
    // Disk-pinned: PLAYER interface +0x190 is the same evaluator (0x6E4630),
    // +0x158 the same getter as COACH/GENERIC (description at actor +0xF30).
    Class {
        group: Group::Celebrity,
        actor: scaler_v2::PLAYER,
        interface: scaler_v2::PLAYER_INTERFACE,
        description: scaler_v2::DESCRIPTION_PRIVATE,
        alt_description: scaler_v2::DESCRIPTION,
        description_offset: 0xF30,
        getter: 0x0081_1790,
        actor_name: ".?AVPLAYER@CHARACTER@@",
        description_name: ".?AVPLAYER_DESCRIPTION_PRIVATE@CHARACTER@@",
        alt_description_name: ".?AVPLAYER_DESCRIPTION@CHARACTER@@",
    },
];

fn image(m: &impl Memory, rva: u64) -> Result<u64> {
    m.base()
        .checked_add(rva)
        .context("Scaler group RVA overflow")
}
fn object(address: u64, offset: u64, len: usize) -> Result<u64> {
    let at = address
        .checked_add(offset)
        .context("Scaler group object overflow")?;
    ensure!(
        at >= 0x10000
            && at
                .checked_add(len as u64)
                .is_some_and(|end| end <= 0x7fff_0000_0000),
        "Invalid scaler group object span"
    );
    Ok(at)
}
fn bytes(m: &impl Memory, at: u64, len: usize) -> Result<Vec<u8>> {
    object(at, 0, len)?;
    let b = m.read(at, len)?;
    ensure!(b.len() == len, "Partial scaler group read");
    Ok(b)
}
fn q(m: &impl Memory, at: u64) -> Result<u64> {
    Ok(u64::from_le_bytes(bytes(m, at, 8)?.try_into().unwrap()))
}
fn d(m: &impl Memory, at: u64) -> Result<u32> {
    Ok(u32::from_le_bytes(bytes(m, at, 4)?.try_into().unwrap()))
}
fn in_image(m: &impl Memory, at: u64) -> bool {
    at >= m.base() && at < m.base() + crate::camera_v2::IMAGE_BYTES
}
/// MSVC x64 RTTI: vtable[-1] -> complete object locator (signature 1),
/// +12 = type descriptor RVA, decorated name at descriptor +16.
fn class_name(m: &impl Memory, vtable_rva: u64) -> Result<String> {
    let col = q(m, image(m, vtable_rva)? - 8)?;
    ensure!(
        in_image(m, col) && d(m, col)? == 1,
        "Scaler group class locator differs"
    );
    let descriptor = image(m, u64::from(d(m, col + 12)?))?;
    let raw = bytes(m, descriptor + 16, 96)?;
    let end = raw.iter().position(|b| *b == 0).unwrap_or(raw.len());
    Ok(String::from_utf8_lossy(&raw[..end]).into_owned())
}

/// Immutable proof on top of the shared-input scaler profile: class names,
/// shared evaluator and description getters for all three groups.
pub(crate) fn verify_profile(m: &impl Memory) -> Result<()> {
    scaler_v2::verify_profile(m)?;
    ensure!(
        class_name(m, scaler_v2::MANAGER)? == MANAGER_NAME,
        "Scaler group manager class differs"
    );
    for class in &CLASSES {
        ensure!(
            class_name(m, class.actor)? == class.actor_name
                && class_name(m, class.description)? == class.description_name
                && (class.alt_description == 0
                    || class_name(m, class.alt_description)? == class.alt_description_name),
            "Scaler group class identity differs"
        );
        ensure!(
            q(m, image(m, class.interface + EVALUATOR_SLOT)?)? == image(m, EVALUATOR)?,
            "Scaler group does not use the shared size evaluator"
        );
        ensure!(
            q(m, image(m, class.interface + DESCRIPTION_GETTER)?)? == image(m, class.getter)?,
            "Scaler group description getter differs"
        );
        // lea rax,[rcx+disp32]; ret  — interface + disp == actor + offset.
        let code = bytes(m, image(m, class.getter)?, 8)?;
        let disp = (class.description_offset - INTERFACE) as i32;
        ensure!(
            code[..3] == [0x48, 0x8D, 0x81] && code[3..7] == disp.to_le_bytes() && code[7] == 0xC3,
            "Scaler group description offset differs"
        );
    }
    m.checkpoint()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Character {
    pub group: Group,
    pub node: u64,
    pub actor: u64,
    pub description: u64,
    pub flag: u8,
    pub scale: u32,
    pub inverse: u32,
}
fn class_of(m: &impl Memory, vtable: u64) -> Result<Option<&'static Class>> {
    for class in &CLASSES {
        if vtable == image(m, class.actor)? {
            return Ok(Some(class));
        }
    }
    Ok(None)
}
/// Re-verifies one character's full object chain. Used by the scan and again
/// immediately before every write (WPF ActiveCoach/ActiveArena parity).
pub(crate) fn identity(m: &impl Memory, node: u64) -> Result<Option<Character>> {
    ensure!(
        q(m, object(node, 0, 0x138)?)? == image(m, scaler_v2::MANAGER)?,
        "Arena characters are loading; waiting for stable data"
    );
    let actor = q(m, node + NODE_ACTOR)?;
    if actor < 0x10000 || bytes(m, node + NODE_STATE, 1)?[0] != 0 {
        return Ok(None);
    }
    let Some(class) = class_of(m, q(m, object(actor, 0, 0x1000)?)?)? else {
        return Ok(None);
    };
    // PLAYER is also the basketball-player class. A manager not currently
    // bound to a roster wrapper is not positive celebrity identity. Never let
    // this NPC lane change a PLAYER flag/cache, including before binding.
    if class.group == Group::Celebrity {
        return Ok(None);
    }
    ensure!(
        q(m, actor + INTERFACE)? == image(m, class.interface)?,
        "Scaler group interface differs"
    );
    let description = object(actor, class.description_offset, 0x80)?;
    let description_class = q(m, description)?;
    ensure!(
        description_class == image(m, class.description)?
            || (class.alt_description != 0
                && description_class == image(m, class.alt_description)?),
        "Scaler group description differs"
    );
    let pair = bytes(m, object(actor, scaler_v2::SCALE_PAIR, 8)?, 8)?;
    let scale = u32::from_le_bytes(pair[..4].try_into()?);
    let inverse = u32::from_le_bytes(pair[4..].try_into()?);
    ensure!(
        scaler_policy::scale_pair(scale, inverse),
        "Arena character sizes are loading; waiting for stable data"
    );
    ensure!(
        q(m, node + NODE_ACTOR)? == actor,
        "Arena character changed during inspection"
    );
    Ok(Some(Character {
        group: class.group,
        node,
        actor,
        description,
        flag: bytes(m, description + OPT_IN, 1)?[0],
        scale,
        inverse,
    }))
}
/// Managers of the game's rostered players (the scaler wrapper table). Those
/// opt in natively and are never adopted as courtside celebrities, even while
/// a freshly loaded player has not set its own opt-in bit yet.
fn rostered_managers(m: &impl Memory) -> Result<Vec<u64>> {
    let table = bytes(m, image(m, scaler_v2::TABLE)?, scaler_v2::TABLE_BYTES)?;
    let mut managers = Vec::new();
    for row in table.chunks_exact(0x50) {
        let mut manager = u64::from_le_bytes(row[0x10..0x18].try_into()?);
        if manager < 0x10000 {
            continue;
        }
        if q(m, manager)? == image(m, scaler_v2::MANAGER_INTERFACE)? {
            manager -= 0x78;
        }
        managers.push(manager);
    }
    Ok(managers)
}
/// Bounded walk of the character list; a changing list is NotReady.
pub(crate) fn scan(m: &impl Memory) -> Result<Vec<Character>> {
    m.checkpoint()?;
    let rostered = rostered_managers(m)?;
    let sentinel = image(m, LIST_SENTINEL)?;
    let mut previous = sentinel;
    let mut node = q(m, sentinel + 0x10)?;
    let mut found = Vec::new();
    for _ in 0..MAX_NODES {
        if node == sentinel {
            break;
        }
        ensure!(
            node >= 0x10000 && q(m, object(node, 8, 8)?)? == previous,
            "Arena characters are loading; waiting for stable data"
        );
        if let Some(character) = identity(m, node)?
            && !(character.group == Group::Celebrity && rostered.contains(&node))
        {
            found.push(character);
        }
        previous = node;
        node = q(m, node + 0x10)?;
    }
    ensure!(
        node == sentinel && q(m, sentinel + 8)? == previous,
        "Arena character list changed during inspection"
    );
    ensure!(
        rostered_managers(m)? == rostered,
        "Rostered players changed during inspection"
    );
    m.checkpoint()?;
    Ok(found)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Groups {
    pub coaches: bool,
    pub cheerleaders: bool,
    pub arena_personnel: bool,
}
impl Groups {
    pub fn any(self) -> bool {
        self.coaches || self.cheerleaders || self.arena_personnel
    }
    pub fn includes(self, group: Group) -> bool {
        match group {
            Group::Coach => self.coaches,
            Group::Cheerleader => self.cheerleaders,
            Group::Arena => self.arena_personnel,
            Group::Celebrity => false,
        }
    }
}
/// Ownership receipt for one adopted character in this game session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Owned {
    pub group: Group,
    pub node: u64,
    pub actor: u64,
    pub description: u64,
    pub original_flag: u8,
    pub original: (u32, u32),
    pub seed: Option<(u32, u32)>,
    pub seeded: bool,
}
impl Owned {
    fn same(&self, c: &Character) -> bool {
        self.group == c.group
            && self.node == c.node
            && self.actor == c.actor
            && self.description == c.description
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Write {
    pub node: u64,
    pub actor: u64,
    pub at: u64,
    pub before: Vec<u8>,
    pub after: Vec<u8>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Plan {
    pub writes: Vec<Write>,
    pub conflicts: usize,
}
fn seed_pair(original: u32, input: u32) -> (u32, u32) {
    let scale = f32::from_bits(original) * f32::from_bits(input);
    (scale.to_bits(), (1.0f32 / scale).to_bits())
}
fn pair_bytes(pair: (u32, u32)) -> Vec<u8> {
    ((u64::from(pair.1) << 32) | u64::from(pair.0))
        .to_le_bytes()
        .to_vec()
}
/// Pure planner. Drops receipts for characters that are no longer loaded
/// (never written again), adopts only characters without the native bit,
/// and refuses (counts, skips) any owned value changed by someone else.
pub(crate) fn plan(
    found: &[Character],
    groups: Groups,
    input: u32,
    owned: &mut BTreeMap<u64, Owned>,
) -> Plan {
    // Defense in depth: even a forged Character or retained old receipt must
    // not bypass identity's PLAYER exclusion. Dropping is observation only;
    // no unsupported PLAYER restoration/adoption is authorized.
    owned.retain(|_, o| o.group != Group::Celebrity && found.iter().any(|c| o.same(c)));
    let mut out = Plan::default();
    for c in found
        .iter()
        .filter(|c| c.group != Group::Celebrity && groups.includes(c.group))
    {
        let receipt = match owned.get_mut(&c.node) {
            Some(o) => o,
            None => {
                if c.flag & OPT_IN_BIT != 0 {
                    continue;
                }
                owned.insert(
                    c.node,
                    Owned {
                        group: c.group,
                        node: c.node,
                        actor: c.actor,
                        description: c.description,
                        original_flag: c.flag,
                        original: (c.scale, c.inverse),
                        seed: c.group.seeded().then(|| seed_pair(c.scale, input)),
                        seeded: !c.group.seeded(),
                    },
                );
                owned.get_mut(&c.node).unwrap()
            }
        };
        let target = receipt.original_flag | OPT_IN_BIT;
        if c.flag == receipt.original_flag {
            out.writes.push(Write {
                node: c.node,
                actor: c.actor,
                at: c.description + OPT_IN,
                before: vec![c.flag],
                after: vec![target],
            });
        } else if c.flag != target {
            out.conflicts += 1;
            continue;
        }
        if let (false, Some(seed)) = (receipt.seeded, receipt.seed) {
            let current = (c.scale, c.inverse);
            if current == receipt.original {
                out.writes.push(Write {
                    node: c.node,
                    actor: c.actor,
                    at: c.actor + scaler_v2::SCALE_PAIR,
                    before: pair_bytes(current),
                    after: pair_bytes(seed),
                });
            }
            // Already seeded, or the game recomputed it after opting in:
            // either way the native cache now owns the value.
            receipt.seeded = true;
        }
    }
    out
}
pub(crate) fn summary(owned: &BTreeMap<u64, Owned>) -> BTreeMap<Group, usize> {
    let mut counts = BTreeMap::new();
    for o in owned.values() {
        *counts.entry(o.group).or_insert(0) += 1;
    }
    counts
}

#[cfg(test)]
#[path = "scaler_npc_v2_tests.rs"]
mod tests;
