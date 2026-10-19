//! Version 2 scaler read-only verifier and fixed-address write planner.
//! This module cannot open processes, change protections or write memory.
//! The owner worker must persist intent, recapture readiness, verify exact
//! before bytes and use the separate four-byte non-executable image capability.
#![allow(dead_code)]
use crate::{
    blacktop_native::Memory,
    scaler_policy::{self, Ownership},
};
use anyhow::{Context, Result, ensure};

pub(crate) const INPUT: u64 = 0x0a41_d07c;
pub(crate) const OWNER: u64 = 0x0ab5_ce20;
pub(crate) const TABLE: u64 = 0x0ab5_ce30;
pub(crate) const WRAPPER: u64 = 0x0762_d278;
pub(crate) const MANAGER: u64 = 0x0688_44d8;
pub(crate) const MANAGER_INTERFACE: u64 = 0x0688_4898;
pub(crate) const PLAYER: u64 = 0x0689_0d88;
pub(crate) const PLAYER_INTERFACE: u64 = 0x0689_1598;
pub(crate) const DESCRIPTION: u64 = 0x0689_20c8;
pub(crate) const DESCRIPTION_PRIVATE: u64 = 0x0689_23c8;
pub(crate) const MANAGER_ACTOR: u64 = 0x110;
pub(crate) const ROSTER_OFFSET: usize = 0x28;
pub(crate) const SCALE_PAIR: u64 = 0xbcc;
pub(crate) const TABLE_BYTES: usize = 30 * 0x50;

// Individually proved on the installed Version 2 disk image. These are not
// uniform offsets from an earlier build. Prefix+disp anchors also verify the
// ASLR-independent target; exact signatures below prove the structure fields.
pub(crate) const RIP_ANCHORS: &[(u64, &[u8], u64)] = &[
    (0x0060_023b, &[0xc5, 0xfa, 0x11, 0x05], INPUT),
    (0x006e_42ce, &[0xc5, 0xfa, 0x10, 0x05], INPUT),
    (0x006e_476a, &[0xc5, 0xfa, 0x10, 0x05], INPUT),
    (0x006e_4a68, &[0xc5, 0xfa, 0x10, 0x05], INPUT),
    (0x024f_5450, &[0x48, 0x8b, 0x05], OWNER),
    (0x024f_546c, &[0x48, 0x8d, 0x05], TABLE),
    (0x024f_8b00, &[0x4c, 0x8d, 0x35], TABLE),
    (0x024f_9955, &[0x48, 0x8d, 0x05], WRAPPER),
];
pub(crate) const FIELD_ANCHORS: &[(u64, &[u8])] = &[
    // The wrapper may retain MANAGER_INSTANCE's +0x78 interface. Its native
    // forwarding method adjusts back to the same primary object.
    (
        0x0081_e2f0,
        &[0x48, 0x83, 0xc1, 0x88, 0xe9, 0x57, 0xf8, 0xff, 0xff],
    ),
    // PLAYER_DESCRIPTION_PRIVATE keeps the public description at offset zero;
    // its constructor copies the 0x68..0x87 region containing the opt-in byte.
    (0x0072_9998, &[0xc5, 0xfc, 0x10, 0x5a, 0x68]),
    (0x0072_99ac, &[0xc5, 0xfc, 0x11, 0x59, 0x68]),
    (
        0x0072_9a27,
        &[0x48, 0x8d, 0x05, 0x9a, 0x89, 0x16, 0x06, 0x48, 0x89, 0x06],
    ),
    // Owner mode, bounded 30 x 0x50 wrapper walk, manager/roster slots.
    (0x024f_5457, &[0x8b, 0x80, 0x04, 0x0a, 0, 0, 0xc3]),
    (
        0x024f_8b10,
        &[
            0x49, 0x83, 0xc7, 0x50, 0x49, 0x81, 0xff, 0x60, 0x09, 0, 0, 0x74, 0x33, 0x4b, 0x83,
            0x7c, 0x37, 0x10, 0, 0x74, 0xeb, 0x4b, 0x83, 0x7c, 0x37, 0x28, 0,
        ],
    ),
    // Manager-instance's primary virtual getter reads actor at +0x110.
    (0x0080_f260, &[0x48, 0x8b, 0x81, 0x10, 0x01, 0, 0, 0xc3]),
    (
        0x006f_c20e,
        &[0x48, 0x8b, 0x8e, 0x10, 0x01, 0, 0, 0x48, 0x85, 0xc9],
    ),
    // Wrapper's roster at +0x28 and roster's exact manager backreference.
    (
        0x007c_5e0c,
        &[0x4c, 0x89, 0x49, 0x28, 0x49, 0x89, 0x91, 0x18, 0x05, 0, 0],
    ),
    // Descriptor getter: actor interface+0x78 plus0xEB8 => actor+0xF30.
    (0x0081_1790, &[0x48, 0x8d, 0x81, 0xb8, 0x0e, 0, 0, 0xc3]),
    // Game populates player opt-in bit; this port does not write this flag.
    (0x024f_54ae, &[0x80, 0x4f, 0x7d, 0x02]),
    // Evaluation consults that opt-in bit before loading shared input.
    (
        0x006e_4762,
        &[0x0f, 0xb6, 0x40, 0x7d, 0x24, 0x02, 0x74, 0x30],
    ),
    // Native cache computes reciprocal into BD0 from actor scale BCC.
    (
        0x006c_4941,
        &[
            0xc5, 0xfa, 0x5e, 0x86, 0xcc, 0x0b, 0, 0, 0xc5, 0xfa, 0x11, 0x86, 0xd0, 0x0b, 0, 0,
        ],
    ),
];

fn at(m: &impl Memory, offset: u64) -> Result<u64> {
    let address = m
        .base()
        .checked_add(offset)
        .context("Scaler address overflow")?;
    ensure!(
        address >= 0x10000 && address < 0x7fff_0000_0000,
        "Invalid scaler address"
    );
    Ok(address)
}
fn checked_address(address: u64, offset: u64, len: usize) -> Result<u64> {
    let value = address
        .checked_add(offset)
        .context("Scaler object address overflow")?;
    ensure!(
        value >= 0x10000
            && value
                .checked_add(len as u64)
                .is_some_and(|end| end <= 0x7fff_0000_0000),
        "Invalid scaler object span"
    );
    Ok(value)
}
fn read(m: &impl Memory, address: u64, length: usize) -> Result<Vec<u8>> {
    checked_address(address, 0, length)?;
    ensure!((1..=8192).contains(&length), "Invalid scaler read size");
    let bytes = m.read(address, length)?;
    ensure!(
        bytes.len() == length,
        "Partial scaler snapshot; no write planned"
    );
    Ok(bytes)
}
fn q(m: &impl Memory, address: u64) -> Result<u64> {
    Ok(u64::from_le_bytes(read(m, address, 8)?.try_into().unwrap()))
}
fn d(m: &impl Memory, address: u64) -> Result<u32> {
    Ok(u32::from_le_bytes(read(m, address, 4)?.try_into().unwrap()))
}
fn dq(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

/// A narrow feature profile, not permission to use legacy/native writers.
pub(crate) fn verify_profile(m: &impl Memory) -> Result<()> {
    m.checkpoint()?;
    let h = read(m, m.base(), 4096)?;
    ensure!(&h[..2] == b"MZ", "Invalid scaler executable header");
    let pe = u32::from_le_bytes(h[60..64].try_into()?) as usize;
    ensure!(
        (64..=3800).contains(&pe) && &h[pe..pe + 4] == b"PE\0\0",
        "Invalid scaler PE header"
    );
    ensure!(
        u16::from_le_bytes(h[pe + 4..pe + 6].try_into()?) == 0x8664
            && u16::from_le_bytes(h[pe + 24..pe + 26].try_into()?) == 0x20b,
        "Scaler requires x64 PE32+"
    );
    ensure!(
        u32::from_le_bytes(h[pe + 8..pe + 12].try_into()?) == 0x6ac5_3765
            && u32::from_le_bytes(h[pe + 80..pe + 84].try_into()?) == 0x35bd_e000,
        "This game build has no verified scaler profile"
    );
    for &(rva, prefix, target) in RIP_ANCHORS {
        let len = prefix.len() + 4;
        let bytes = read(m, at(m, rva)?, len)?;
        ensure!(
            &bytes[..prefix.len()] == prefix,
            "Scaler code anchor differs"
        );
        let disp = i32::from_le_bytes(bytes[prefix.len()..].try_into()?);
        ensure!(
            rva as i64 + len as i64 + i64::from(disp) == target as i64,
            "Scaler anchor has a different target"
        );
    }
    for &(rva, bytes) in FIELD_ANCHORS {
        ensure!(
            read(m, at(m, rva)?, bytes.len())? == bytes,
            "Scaler object-field signature differs"
        );
    }
    for (rva, methods) in [
        (WRAPPER, [0x0292_3180, 0x0292_31e0]),
        (MANAGER, [0x0081_da20, 0x0081_db40]),
        (MANAGER_INTERFACE, [0x0081_e1e0, 0x0081_db40]),
        (PLAYER, [0x0082_60b0, 0x0082_6160]),
        (PLAYER_INTERFACE, [0x0082_6cb0, 0x0082_6160]),
        (DESCRIPTION, [0x14f0, 0x0082_7040]),
        (DESCRIPTION_PRIVATE, [0x0082_7130, 0x0082_7190]),
    ] {
        let bytes = read(m, at(m, rva)?, 16)?;
        for (index, method) in methods.into_iter().enumerate() {
            ensure!(
                dq(&bytes, index * 8) == at(m, method)?,
                "Scaler class-table signature differs"
            );
        }
    }
    for (rva, expected) in [
        (MANAGER + 0xc8, 0x0080_f260),
        (PLAYER_INTERFACE + 0x158, 0x0081_1790),
        (PLAYER_INTERFACE + 0x190, 0x006e_4630),
    ] {
        ensure!(
            q(m, at(m, rva)?)? == at(m, expected)?,
            "Scaler virtual method differs"
        );
    }
    m.checkpoint()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PlayerIdentity {
    manager: u64,
    roster: u64,
    actor: u64,
    description: u64,
    description_class: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Readiness {
    owner: u64,
    mode: u32,
    table: Vec<u8>,
    players: Vec<PlayerIdentity>,
}
impl Readiness {
    /// Already-captured mode for informational timing; performs no reads.
    pub(crate) fn mode(&self) -> u32 {
        self.mode
    }
    pub(crate) fn loaded(&self) -> bool {
        !self.players.is_empty() || self.mode != 0
    }
    pub(crate) fn loaded_players(&self) -> usize {
        self.players.len()
    }
}

/// Inspect only; the native cache contains each player's authored size as well
/// as the shared input. Do not call a cache value a percent of stock height.
#[derive(Debug, PartialEq)]
pub(crate) struct Observation {
    pub players: usize,
    pub minimum_cache: Option<f32>,
    pub maximum_cache: Option<f32>,
}
pub(crate) fn observe(m: &impl Memory, percent: u8) -> Result<Observation> {
    let expected = scaler_policy::target_bits(percent)?;
    ensure!(
        d(m, at(m, INPUT)?)? == expected,
        "Shared player-size input differs"
    );
    let state = snapshot(m, true)?;
    let mut minimum: Option<f32> = None;
    let mut maximum: Option<f32> = None;
    for player in &state.players {
        let pair = read(m, checked_address(player.actor, SCALE_PAIR, 8)?, 8)?;
        let scale = u32::from_le_bytes(pair[..4].try_into()?);
        let inverse = u32::from_le_bytes(pair[4..].try_into()?);
        ensure!(
            scaler_policy::scale_pair(scale, inverse),
            "Native size cache is changing"
        );
        let scale = f32::from_bits(scale);
        minimum = Some(minimum.map_or(scale, |old| old.min(scale)));
        maximum = Some(maximum.map_or(scale, |old| old.max(scale)));
    }
    ensure!(
        snapshot(m, true)? == state,
        "Scaler context changed during observation"
    );
    ensure!(
        d(m, at(m, INPUT)?)? == expected,
        "Shared player-size input changed during observation"
    );
    Ok(Observation {
        players: state.loaded_players(),
        minimum_cache: minimum,
        maximum_cache: maximum,
    })
}

/// Write planning and rearm only happen at Main, Freestyle or Blacktop.
pub(crate) fn snapshot_readiness(m: &impl Memory) -> Result<Readiness> {
    snapshot(m, false)
}
/// Observation may run in any mode (e.g. a Quick Play NBA game, where the
/// shared input is equally in effect); it never plans or writes.
fn snapshot(m: &impl Memory, any_mode: bool) -> Result<Readiness> {
    m.checkpoint()?;
    let owner = q(m, at(m, OWNER)?)?;
    checked_address(owner, 0, 0xa08)?;
    let mode_address = checked_address(owner, 0xa04, 4)?;
    let mode = d(m, mode_address)?;
    ensure!(
        any_mode || matches!(mode, 0 | 4 | 6),
        "Waiting for the main menu, Freestyle or Blacktop"
    );
    let table = read(m, at(m, TABLE)?, TABLE_BYTES)?;
    let mut players = Vec::new();
    let mut unique_managers = std::collections::HashSet::new();
    let mut unique_actors = std::collections::HashSet::new();
    for row in table.chunks_exact(0x50) {
        let vt = dq(row, 0);
        let table_manager = dq(row, 0x10);
        let mut manager = table_manager;
        let roster = dq(row, ROSTER_OFFSET);
        // Unknown wrappers/transitions are not treated as an empty table.
        if vt == 0 && manager == 0 && roster == 0 {
            continue;
        }
        ensure!(
            vt == at(m, WRAPPER)?,
            "Waiting for a stable player wrapper table"
        );
        if manager == 0 && roster == 0 {
            continue;
        }
        checked_address(manager, 0, 0x118)?;
        checked_address(roster, 0, 0x520)?;
        if q(m, manager)? == at(m, MANAGER_INTERFACE)? {
            manager = manager
                .checked_sub(0x78)
                .context("Scaler manager interface underflow")?;
            checked_address(manager, 0, 0x118)?;
        }
        ensure!(
            unique_managers.insert(manager),
            "Duplicate scaler player manager"
        );
        ensure!(
            q(m, manager)? == at(m, MANAGER)?,
            "Waiting for verified player managers"
        );
        let actor = q(m, checked_address(manager, MANAGER_ACTOR, 8)?)?;
        checked_address(actor, 0, 0xf38)?;
        ensure!(unique_actors.insert(actor), "Duplicate scaler player actor");
        let description = checked_address(actor, 0xf30, 8)?;
        let description_class = q(m, description)?;
        ensure!(
            q(m, actor)? == at(m, PLAYER)?
                && q(m, checked_address(actor, 0x78, 8)?)? == at(m, PLAYER_INTERFACE)?
                && [at(m, DESCRIPTION)?, at(m, DESCRIPTION_PRIVATE)?].contains(&description_class),
            "Waiting for verified player objects"
        );
        ensure!(
            [manager, table_manager].contains(&q(m, checked_address(roster, 0x518, 8)?)?),
            "Player roster-manager ownership differs"
        );
        ensure!(
            read(m, checked_address(description, 0x7d, 1)?, 1)?[0] & 2 != 0,
            "Player has not initialized its native shared-size opt-in"
        );
        let pair = read(m, checked_address(actor, SCALE_PAIR, 8)?, 8)?;
        ensure!(
            scaler_policy::scale_pair(
                u32::from_le_bytes(pair[..4].try_into()?),
                u32::from_le_bytes(pair[4..].try_into()?)
            ),
            "Waiting for stable native player size data"
        );
        ensure!(
            q(m, checked_address(manager, MANAGER_ACTOR, 8)?)? == actor,
            "Player changed during scaler snapshot"
        );
        players.push(PlayerIdentity {
            manager,
            roster,
            actor,
            description,
            description_class,
        });
    }
    ensure!(
        q(m, at(m, OWNER)?)? == owner
            && d(m, mode_address)? == mode
            && read(m, at(m, TABLE)?, TABLE_BYTES)? == table,
        "The game is changing players or modes; no scaler write planned"
    );
    m.checkpoint()?;
    Ok(Readiness {
        owner,
        mode,
        table,
        players,
    })
}

pub(crate) fn revalidate(m: &impl Memory, expected: &Readiness) -> Result<()> {
    ensure!(
        snapshot_readiness(m)? == *expected,
        "Scaler context changed after preparation"
    );
    Ok(())
}

#[derive(Debug, Clone)]
pub(crate) struct SharedInputPlan {
    pub address: u64,
    pub before: [u8; 4],
    pub after: [u8; 4],
    pub reload_required: bool,
    pub readiness: Readiness,
}

pub(crate) fn plan_shared_input(
    m: &impl Memory,
    percent: u8,
    ownership: Option<Ownership>,
) -> Result<Option<SharedInputPlan>> {
    verify_profile(m)?;
    let readiness = snapshot_readiness(m)?;
    let address = at(m, INPUT)?;
    let current = d(m, address)?;
    let Some(change) = scaler_policy::plan(current, percent, ownership)? else {
        return Ok(None);
    };
    Ok(Some(SharedInputPlan {
        address,
        before: change.before.to_le_bytes(),
        after: change.after.to_le_bytes(),
        reload_required: readiness.loaded(),
        readiness,
    }))
}

/// Restoration remains bounded to the same profile and currently stable mode.
/// No ownership => no plan; an unknown value is never restored as if ours.
pub(crate) fn plan_restore(
    m: &impl Memory,
    ownership: Option<Ownership>,
) -> Result<Option<SharedInputPlan>> {
    if ownership.is_none() {
        return Ok(None);
    }
    verify_profile(m)?;
    let readiness = snapshot_readiness(m)?;
    let address = at(m, INPUT)?;
    let Some(change) = scaler_policy::restore(d(m, address)?, ownership)? else {
        return Ok(None);
    };
    Ok(Some(SharedInputPlan {
        address,
        before: change.before.to_le_bytes(),
        after: change.after.to_le_bytes(),
        reload_required: readiness.loaded(),
        readiness,
    }))
}

#[cfg(test)]
#[path = "scaler_v2_tests.rs"]
mod tests;
