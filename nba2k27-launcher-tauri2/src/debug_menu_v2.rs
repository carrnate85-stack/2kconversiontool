//! Exact-build policy for NBA 2K27's three Boolean debug-menu features.
//!
//! The game owns the feature manager and entries. This module only locates the
//! three allowlisted IDs through the game's sorted pointer table and produces
//! one-byte transitions. It never accepts an address or feature ID from UI.
use crate::blacktop_native::Memory;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};

pub(crate) const IMAGE_BYTES: u64 = 0x3653_2000;
const STAMP: u32 = 0x6AD2_E48C;
const DEFAULT_MANAGER: u64 = 0xA1F_1248;
const OVERRIDE_MANAGER: u64 = 0xA1F_1250;
const TABLE_OFFSET: u64 = 0xF8;
const COUNT_OFFSET: u64 = 0x110;
const MAX_FEATURES: u32 = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Definition {
    pub name: &'static str,
    pub key: u32,
    object_rva: u64,
    vtable_rva: u64,
    registration_rva: u64,
    registration_hex: &'static str,
}
pub(crate) const DEFINITIONS: [Definition; 3] = [
    Definition {
        name: "ENABLE_DEBUG_MENUS",
        key: 0x8EB3_D32B,
        object_rva: 0xB52_46A8,
        vtable_rva: 0x7C0_4398,
        registration_rva: 0x39E_37B7,
        registration_hex: "488D0DEA0EB407BA2BD3B38E4531C0E8958466FC488D05C60B2204488905CF0EB407",
    },
    Definition {
        name: "MENU_ENABLE_DEBUG_MENU_OPTION_IN_CLEAN",
        key: 0x9763_69F2,
        object_rva: 0xB52_4738,
        vtable_rva: 0x7C0_49F8,
        registration_rva: 0x39E_3883,
        registration_hex: "488D0DAE0EB407BAF26963974531C0E8C98366FC488D055A112204488905930EB407",
    },
    Definition {
        name: "NAVIGATION_MENU_SHOW_ALL_ITEMS",
        key: 0x5B4D_D268,
        object_rva: 0xB52_4750,
        vtable_rva: 0x7C0_4B08,
        registration_rva: 0x39E_38A5,
        registration_hex: "488D0DA40EB407BA68D24D5B4531C0E8A78366FC488D0548122204488905890EB407",
    },
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    pub definition: usize,
    pub address: u64,
    pub packed: u32,
    pub value: u8,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub manager: u64,
    pub table: u64,
    pub count: u32,
    pub entries: Vec<Entry>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Change {
    pub definition: usize,
    pub address: u64,
    pub before: u8,
    pub after: u8,
}

fn read<const N: usize>(m: &impl Memory, at: u64) -> Result<[u8; N]> {
    m.read(at, N)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Incomplete debug-menu read"))
}
fn q(m: &impl Memory, at: u64) -> Result<u64> {
    Ok(u64::from_le_bytes(read(m, at)?))
}
fn d(m: &impl Memory, at: u64) -> Result<u32> {
    Ok(u32::from_le_bytes(read(m, at)?))
}
fn image(m: &impl Memory, rva: u64) -> Result<u64> {
    m.base().checked_add(rva).context("Debug-menu RVA overflow")
}
fn hex(text: &str) -> Result<Vec<u8>> {
    ensure!(
        text.len() % 2 == 0 && text.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid debug-menu anchor encoding"
    );
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).map_err(Into::into))
        .collect()
}
fn hash(m: &impl Memory, rva: u64, bytes: usize, expected: &str) -> Result<()> {
    let observed = format!("{:x}", Sha256::digest(m.read(image(m, rva)?, bytes)?));
    ensure!(
        observed == expected,
        "Debug-menu method proof differs at RVA {rva:X}"
    );
    Ok(())
}

pub(crate) fn verify_profile(m: &impl Memory) -> Result<()> {
    let header = m.read(m.base(), 4096)?;
    ensure!(
        header.len() == 4096 && &header[..2] == b"MZ",
        "Invalid debug-menu image"
    );
    let pe = u32::from_le_bytes(header[0x3c..0x40].try_into()?) as usize;
    ensure!(
        pe >= 0x40 && pe + 24 + 60 <= header.len() && &header[pe..pe + 4] == b"PE\0\0",
        "Invalid debug-menu PE header"
    );
    let stamp = u32::from_le_bytes(header[pe + 8..pe + 12].try_into()?);
    let size = u32::from_le_bytes(header[pe + 24 + 56..pe + 24 + 60].try_into()?);
    ensure!(
        stamp == STAMP && u64::from(size) == IMAGE_BYTES,
        "Debug menus are not mapped for this game build"
    );
    for (rva, bytes, digest) in [
        (
            0x4AF80,
            134,
            "c380850d0b954eba9d559e92744c593b52268c5dc33c4c2379a8a9bc476010fc",
        ),
        (
            0x4BC60,
            828,
            "7d3303a90dd4bdb6f24e94bce7a81ed07e8c2a7bc22c1118c89b3548ceaa090d",
        ),
        (
            0x4E110,
            77,
            "202139bed737a919591230ad5449213625e7d69009aa10f79c05eced26a129ae",
        ),
        (
            0x4BB0E30,
            242,
            "be9e00bc85e3861548f782ed69d92451e03dbc6e26daf70d8b3b5e250f1d91c3",
        ),
        (
            0x4CD3140,
            43,
            "7a6b01498c88a76316504f83c3673ff553b0329d40400190f9dcfdc9fa10c603",
        ),
    ] {
        hash(m, rva, bytes, digest)?;
    }
    for row in DEFINITIONS {
        let registration = hex(row.registration_hex)?;
        ensure!(
            m.read(image(m, row.registration_rva)?, registration.len())? == registration,
            "Debug-menu registration proof differs: {}",
            row.name
        );
        ensure!(
            q(m, image(m, row.object_rva)?)? == image(m, row.vtable_rva)?,
            "Debug-menu feature class differs: {}",
            row.name
        );
    }
    Ok(())
}

fn headers(m: &impl Memory) -> Result<(u64, u64, u32)> {
    let default = q(m, image(m, DEFAULT_MANAGER)?)?;
    let override_manager = q(m, image(m, OVERRIDE_MANAGER)?)?;
    let manager = if override_manager != 0 {
        override_manager
    } else {
        default
    };
    ensure!(
        manager != 0,
        "Debug-menu feature manager has not initialized"
    );
    let table = q(
        m,
        manager
            .checked_add(TABLE_OFFSET)
            .context("Debug-menu manager overflow")?,
    )?;
    let count = d(
        m,
        manager
            .checked_add(COUNT_OFFSET)
            .context("Debug-menu manager overflow")?,
    )?;
    ensure!(
        table >= 0x10000 && (1..=MAX_FEATURES).contains(&count),
        "Debug-menu feature table is outside fixed bounds"
    );
    Ok((manager, table, count))
}

fn find(m: &impl Memory, table: u64, count: u32, definition: usize) -> Result<Entry> {
    let wanted = DEFINITIONS[definition].key;
    let (mut low, mut high) = (0u32, count);
    let (mut lower, mut upper) = (None::<u32>, None::<u32>);
    let mut found = None;
    for _ in 0..32 {
        if low >= high {
            break;
        }
        let middle = low + (high - low) / 2;
        let slot = table
            .checked_add(u64::from(middle) * 8)
            .context("Debug-menu table overflow")?;
        let pointer = q(m, slot)?;
        ensure!(pointer >= 0x10000, "Debug-menu entry pointer is invalid");
        let key = d(m, pointer)?;
        ensure!(
            lower.is_none_or(|v| key > v) && upper.is_none_or(|v| key < v),
            "Debug-menu sorted lookup ordering changed"
        );
        if key < wanted {
            low = middle + 1;
            lower = Some(key);
        } else if key > wanted {
            high = middle;
            upper = Some(key);
        } else {
            found = Some((middle, pointer));
            break;
        }
    }
    let (middle, address) = found.context("Allowlisted debug-menu feature is not registered")?;
    for neighbor in [
        middle.checked_sub(1),
        middle.checked_add(1).filter(|v| *v < count),
    ]
    .into_iter()
    .flatten()
    {
        let pointer = q(m, table + u64::from(neighbor) * 8)?;
        let key = d(m, pointer)?;
        ensure!(
            key != wanted && (neighbor < middle) == (key < wanted),
            "Debug-menu key duplicate/order changed"
        );
    }
    let packed = d(m, address + 4)?;
    let value = read::<1>(m, address + 8)?[0];
    ensure!(
        packed & 31 == 11 && (packed >> 5) & 0x03ff_ffff == 1 && value <= 1,
        "Allowlisted debug-menu Boolean metadata differs"
    );
    Ok(Entry {
        definition,
        address,
        packed,
        value,
    })
}

pub(crate) fn capture(m: &impl Memory) -> Result<Snapshot> {
    verify_profile(m)?;
    let (manager, table, count) = headers(m)?;
    let entries = (0..DEFINITIONS.len())
        .map(|i| find(m, table, count, i))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        entries
            .iter()
            .map(|e| e.address)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            == DEFINITIONS.len(),
        "Debug-menu entries alias"
    );
    ensure!(
        headers(m)? == (manager, table, count),
        "Debug-menu manager generation changed during capture"
    );
    Ok(Snapshot {
        manager,
        table,
        count,
        entries,
    })
}

pub(crate) fn enable_plan(snapshot: &Snapshot) -> Result<Vec<Change>> {
    ensure!(
        snapshot.entries.len() == DEFINITIONS.len(),
        "Incomplete debug-menu snapshot"
    );
    let mut changes = Vec::new();
    for entry in &snapshot.entries {
        ensure!(
            entry.definition < DEFINITIONS.len()
                && entry.value <= 1
                && entry.packed & 31 == 11
                && (entry.packed >> 5) & 0x03ff_ffff == 1,
            "Invalid debug-menu plan input"
        );
        if entry.value == 0 {
            changes.push(Change {
                definition: entry.definition,
                address: entry.address + 8,
                before: 0,
                after: 1,
            });
        }
    }
    Ok(changes)
}

pub(crate) fn apply(snapshot: &mut Snapshot, change: &Change) -> Result<()> {
    let entry = snapshot
        .entries
        .get_mut(change.definition)
        .context("Unknown debug-menu definition")?;
    ensure!(
        change.address == entry.address + 8
            && entry.value == change.before
            && change.before <= 1
            && change.after <= 1,
        "Debug-menu transition escaped its entry"
    );
    entry.value = change.after;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plan_changes_only_the_three_boolean_value_bytes() {
        let mut snapshot = Snapshot {
            manager: 0x20000,
            table: 0x21000,
            count: 1235,
            entries: DEFINITIONS
                .iter()
                .enumerate()
                .map(|(i, _)| Entry {
                    definition: i,
                    address: 0x30000 + i as u64 * 0x20,
                    packed: 0x2b,
                    value: if i == 1 { 1 } else { 0 },
                })
                .collect(),
        };
        let plan = enable_plan(&snapshot).unwrap();
        assert_eq!(plan.len(), 2);
        assert_eq!(
            plan.iter().map(|c| c.address).collect::<Vec<_>>(),
            vec![0x30008, 0x30048]
        );
        for change in &plan {
            apply(&mut snapshot, change).unwrap();
        }
        assert!(snapshot.entries.iter().all(|e| e.value == 1));
    }
    #[test]
    fn malformed_type_value_and_alias_are_rejected_by_plan_or_capture_contract() {
        let mut snapshot = Snapshot {
            manager: 1,
            table: 2,
            count: 3,
            entries: (0..3)
                .map(|i| Entry {
                    definition: i,
                    address: 0x10000 + i as u64 * 16,
                    packed: 0x2b,
                    value: 0,
                })
                .collect(),
        };
        snapshot.entries[0].packed = 0x29;
        assert!(enable_plan(&snapshot).is_err());
        snapshot.entries[0].packed = 0x2b;
        snapshot.entries[1].value = 2;
        assert!(enable_plan(&snapshot).is_err());
    }
}
