//! Read-only boundary profiles. These are NOT accepted by validate_build(),
//! which remains the separate legacy guard for roster/outfit/rule/arena/ball
//! access. Disk-derived Version 2 candidates still require live acceptance.
use super::{Memory, REFRESH, validate_build};
use anyhow::{Context, Result, ensure};

fn at(m: &impl Memory, rva: u64) -> Result<u64> {
    m.base().checked_add(rva).context("Observer RVA overflow")
}

#[derive(Clone, Copy)]
pub(super) struct Profile {
    pub root: u64,
    pub root_tag: u64,
    pub lineup: u64,
    pub modes: u64,
    pub current: u64,
    pub alternate: u64,
    pub owner: u64,
    pub active: u64,
    pub model: u64,
    pub description: u64,
}
pub(super) const LEGACY: Profile = Profile {
    root: super::ROOT,
    root_tag: 0x78AEA48,
    lineup: super::LINEUP,
    modes: super::MODES,
    current: super::CURRENT,
    alternate: super::ALTERNATE,
    owner: super::OWNER,
    active: super::ACTIVE,
    model: 0x6778FC8,
    description: 0x682D858,
};
pub(super) const VERSION_TWO: Profile = Profile {
    root: 0xAF46870,
    root_tag: 0x791E158,
    lineup: 0x9C496C0,
    modes: 0xAD07408,
    current: 0xAD07BD0,
    alternate: 0xAD07BCC,
    owner: 0xAB5CE20,
    active: 0xAB5D790,
    model: 0x67DD188,
    description: 0x68920C8,
};
// Cross-checked against disk disassembly, not a uniform offset adjustment.
// All RIP displacements must still resolve to the exact mapped target.
pub(crate) const RIP_ANCHORS: &[(u64, &[u8], u64)] = &[
    (0x3457F00, &[0x48, 0x8D, 0x05], VERSION_TWO.root_tag),
    (0x3457F07, &[0x48, 0x89, 0x05], VERSION_TWO.root),
    (0x25AA480, &[0x48, 0x8B, 0x05], VERSION_TWO.owner),
    (0x308C6F9, &[0x48, 0x8D, 0x05], VERSION_TWO.modes),
    (0x308C700, &[0x48, 0x63, 0x0D], VERSION_TWO.alternate),
    (0x300D7B3, &[0x48, 0x63, 0x05], VERSION_TWO.current),
    (0x32F7B1A, &[0x89, 0x05], VERSION_TWO.lineup + 4),
    (0x32F7B20, &[0x89, 0x05], VERSION_TWO.lineup + 8),
];
pub(crate) const FIELD_ANCHORS: &[(u64, &[u8])] = &[
    (0x3B2050, REFRESH),
    (0x32FBB36, &[0x8B, 0x81, 0xA8, 0x0E, 0, 0]),
    (0x32FBB60, &[0x48, 0x8B, 0x8E, 0x80, 0x10, 0, 0]),
    // Active-game flag check. Include the immediate in the RIP length.
    (0x24F7F75, &[0x80, 0x3D, 0x14, 0x58, 0x66, 0x08, 0]),
];
pub(super) fn validate(m: &impl Memory) -> Result<Profile> {
    let h = m.read(m.base(), 4096)?;
    ensure!(
        h.len() == 4096 && &h[..2] == b"MZ",
        "Invalid observer executable header"
    );
    let pe = u32::from_le_bytes(h[60..64].try_into()?) as usize;
    ensure!(
        (64..=3800).contains(&pe)
            && &h[pe..pe + 4] == b"PE\0\0"
            && u16::from_le_bytes(h[pe + 4..pe + 6].try_into()?) == 0x8664,
        "Unsupported observer executable architecture"
    );
    let stamp = u32::from_le_bytes(h[pe + 8..pe + 12].try_into()?);
    let size = u32::from_le_bytes(h[pe + 80..pe + 84].try_into()?);
    if (stamp, size) == (0x6AA2013D, 0x36558000) {
        validate_build(m)?;
        return Ok(LEGACY);
    }
    ensure!(
        (stamp, size) == (0x6AD2E48C, 0x36532000),
        "This game build has no verified read-only observer profile"
    );
    ensure!(
        u16::from_le_bytes(h[pe + 24..pe + 26].try_into()?) == 0x20B,
        "Version 2 observer requires PE32+"
    );
    for &(rva, prefix, target) in RIP_ANCHORS {
        let len = prefix.len() + 4;
        let b = m.read(at(m, rva)?, len)?;
        ensure!(
            b.len() == len && b[..prefix.len()] == *prefix,
            "Version 2 observer code anchor differs"
        );
        let disp = i32::from_le_bytes(b[prefix.len()..].try_into()?);
        ensure!(
            rva as i64 + len as i64 + disp as i64 == target as i64,
            "Version 2 observer anchor targets a different object"
        );
    }
    for &(rva, expected) in FIELD_ANCHORS {
        ensure!(
            m.read(at(m, rva)?, expected.len())? == expected,
            "Version 2 observer field/refresh signature differs"
        );
    }
    // Independently proved RTTI primary tables: PLAYNOW_BLACKTOP and
    // CHARACTER::CONTAINER, not a player-type label invented by the UI.
    for (rva, methods) in [
        (VERSION_TWO.root_tag, [0x23120u64, 0x346B4D0]),
        (VERSION_TWO.model, [0x3D8A90, 0x3D8AF0]),
        (VERSION_TWO.description, [0x14F0, 0x827040]),
    ] {
        let bytes = m.read(at(m, rva)?, 16)?;
        ensure!(
            bytes.len() == 16
                && bytes
                    .chunks_exact(8)
                    .zip(methods)
                    .all(|(b, method)| m.base().checked_add(method)
                        == Some(u64::from_le_bytes(b.try_into().unwrap()))),
            "Version 2 observer primary class table differs"
        );
    }
    Ok(VERSION_TWO)
}
