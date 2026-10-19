//! Version 2 read-only roster ownership research. This is NOT a player/team
//! write capability and does not validate the old decoder's field layout.
//! Kept separate from VerifiedLayout and every existing native write gate.
#![allow(dead_code)]
use crate::blacktop_native::Memory;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};

pub(crate) const ROOT: u64 = 0x09ac_e338;
pub(crate) const ROOT_CLASS: u64 = 0x074e_be98;
pub(crate) const PLAYER_STRIDE: usize = 0x4f8;
pub(crate) const TEAM_STRIDE: usize = 0x1550;
pub(crate) const ARENA_STRIDE: usize = 0x1278;
pub(crate) const UNIFORM_STRIDE: usize = 0x170;
pub(crate) const SERIALIZER: u64 = 0x006a_e330;
pub(crate) const SERIALIZER_BYTES: usize = 50325;
pub(crate) const SERIALIZER_SHA256: &str =
    "b5f0449fac3b3e2cd0d6009212a6b39062bdfd6568aada1f7704425923c739bf";
pub(crate) const PLAYER_FIELDS_VERIFIED: bool = false;
pub(crate) const TEAM_FIELDS_VERIFIED: bool = false;
pub(crate) const WRITES_SUPPORTED: bool = false;

pub(crate) const RIP_ANCHORS: &[(u64, &[u8], u64)] = &[
    (0x0211_a0fb, &[0x48, 0x8b, 0x05], ROOT),
    (0x0211_a120, &[0x48, 0x8b, 0x05], ROOT),
    (0x0240_6bf9, &[0x48, 0x8d, 0x3d], ROOT_CLASS),
];
pub(crate) const FIELD_ANCHORS: &[(u64, &[u8])] = &[
    (0x0211_a102, &[0x48, 0x8b, 0x80, 0xa8, 0, 0, 0]),
    // Bounded owner-relative player count/table, exact stride.
    (
        0x02cc_96f1,
        &[
            0x3b, 0x51, 0x10, 0x73, 0x12, 0x89, 0xd0, 0x48, 0x69, 0xc0, 0xf8, 0x04, 0, 0, 0x48,
            0x03, 0x41, 0x18,
        ],
    ),
    (
        0x02dd_d3b1,
        &[
            0x3b, 0x91, 0x80, 0, 0, 0, 0x73, 0x15, 0x89, 0xd0, 0x48, 0x69, 0xc0, 0x50, 0x15, 0, 0,
            0x48, 0x03, 0x81, 0x88, 0, 0, 0,
        ],
    ),
    (
        0x02ed_9bd1,
        &[
            0x3b, 0x51, 0x60, 0x73, 0x12, 0x89, 0xd0, 0x48, 0x69, 0xc0, 0x78, 0x12, 0, 0, 0x48,
            0x03, 0x41, 0x68,
        ],
    ),
    // This is owner-relative +290, NOT the current old decoder's allocation-
    // relative +290. No caller may silently substitute one for the other.
    (
        0x02ed_cb31,
        &[
            0x3b, 0x91, 0x90, 0x02, 0, 0, 0x73, 0x15, 0x89, 0xd0, 0x48, 0x69, 0xc0, 0x70, 0x01, 0,
            0, 0x48, 0x03, 0x81, 0x98, 0x02, 0, 0,
        ],
    ),
];

fn pointer(value: u64) -> Result<u64> {
    ensure!(
        (0x10000..=0x7fff_ffff_fff8).contains(&value) && value % 8 == 0,
        "Invalid roster ownership pointer"
    );
    Ok(value)
}
fn at(m: &impl Memory, rva: u64) -> Result<u64> {
    m.base()
        .checked_add(rva)
        .context("Roster profile address overflow")
}
fn read(m: &impl Memory, address: u64, len: usize) -> Result<Vec<u8>> {
    ensure!(
        address >= 0x10000
            && (1..=8192).contains(&len)
            && address
                .checked_add(len as u64)
                .is_some_and(|end| end <= 0x7fff_ffff_ffff),
        "Invalid roster profile read"
    );
    m.checkpoint()?;
    let bytes = m.read(address, len)?;
    ensure!(bytes.len() == len, "Partial roster ownership read");
    Ok(bytes)
}
fn q(m: &impl Memory, address: u64) -> Result<u64> {
    pointer(u64::from_le_bytes(
        read(m, address, 8)?.as_slice().try_into()?,
    ))
}

pub(crate) fn verify_identity_profile(m: &impl Memory) -> Result<()> {
    let header = read(m, m.base(), 4096)?;
    ensure!(&header[..2] == b"MZ", "Invalid Version 2 roster image");
    let pe = u32::from_le_bytes(header[60..64].try_into()?) as usize;
    ensure!(
        (64..=3800).contains(&pe) && &header[pe..pe + 4] == b"PE\0\0",
        "Invalid roster PE header"
    );
    ensure!(
        u16::from_le_bytes(header[pe + 4..pe + 6].try_into()?) == 0x8664
            && u16::from_le_bytes(header[pe + 24..pe + 26].try_into()?) == 0x20b,
        "Roster profile requires x64 PE32+"
    );
    ensure!(
        u32::from_le_bytes(header[pe + 8..pe + 12].try_into()?) == 0x6ac5_3765
            && u32::from_le_bytes(header[pe + 80..pe + 84].try_into()?) == 0x35bd_e000,
        "Unsupported read-only roster build"
    );
    for &(rva, prefix, target) in RIP_ANCHORS {
        let length = prefix.len() + 4;
        let bytes = read(m, at(m, rva)?, length)?;
        ensure!(
            &bytes[..prefix.len()] == prefix,
            "Roster identity code anchor differs"
        );
        let disp = i32::from_le_bytes(bytes[prefix.len()..].try_into()?);
        ensure!(
            rva as i64 + length as i64 + i64::from(disp) == target as i64,
            "Roster identity anchor targets another object"
        );
    }
    for &(rva, bytes) in FIELD_ANCHORS {
        ensure!(
            read(m, at(m, rva)?, bytes.len())? == bytes,
            "Roster table descriptor differs"
        );
    }
    let bytes = read(m, at(m, ROOT_CLASS)?, 16)?;
    for (i, method) in [0x024e_65f0, 0x024e_6660].into_iter().enumerate() {
        ensure!(
            u64::from_le_bytes(bytes[i * 8..i * 8 + 8].try_into()?) == at(m, method)?,
            "Roster container class differs"
        );
    }
    Ok(())
}

/// Fingerprinting a newly located serializer pins the code range. It does not
/// by itself prove the 455 old bitfields or authorize reads through that layout.
pub(crate) fn verify_profile(m: &impl Memory) -> Result<()> {
    verify_identity_profile(m)?;
    let mut digest = Sha256::new();
    for offset in (0..SERIALIZER_BYTES).step_by(4096) {
        digest.update(read(
            m,
            at(m, SERIALIZER + offset as u64)?,
            (SERIALIZER_BYTES - offset).min(4096),
        )?);
    }
    ensure!(
        format!("{:x}", digest.finalize()) == SERIALIZER_SHA256,
        "Version 2 player serializer differs"
    );
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OwnedRegion {
    pub base: u64,
    pub len: u64,
}
impl OwnedRegion {
    fn valid(self) -> bool {
        self.base >= 0x10000
            && self.base % 0x1000 == 0
            && (0x800000..=0x8000000).contains(&self.len)
            && self
                .base
                .checked_add(self.len)
                .is_some_and(|end| end <= 0x7fff_ffff_ffff)
    }
    fn contains(self, address: u64, len: usize) -> bool {
        self.valid()
            && address >= self.base
            && address
                .checked_add(len as u64)
                .is_some_and(|end| end <= self.base + self.len)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Directory {
    pub count: usize,
    pub pointer: u64,
    pub stride: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OwnerSnapshot {
    pub root: u64,
    pub owner: u64,
    pub region: OwnedRegion,
    pub players: Directory,
    pub arenas: Directory,
    pub teams: Directory,
    pub uniforms: Option<Directory>,
    headers: Vec<(u64, Vec<u8>)>,
}

fn directory(
    m: &impl Memory,
    region: OwnedRegion,
    owner: u64,
    offset: u64,
    stride: usize,
    min: usize,
    max: usize,
    optional: bool,
) -> Result<(Option<Directory>, (u64, Vec<u8>))> {
    let address = owner
        .checked_add(offset)
        .context("Roster header overflow")?;
    ensure!(
        region.contains(address, 16),
        "Roster descriptor is outside its allocation"
    );
    let header = read(m, address, 16)?;
    let count = u64::from_le_bytes(header[..8].try_into()?);
    let table = u64::from_le_bytes(header[8..].try_into()?);
    if optional && count == 0 && table == 0 {
        return Ok((None, (address, header)));
    }
    ensure!(
        count >= min as u64 && count <= max as u64 && pointer(table).is_ok(),
        "Roster descriptor is not initialized"
    );
    let bytes = (count as usize)
        .checked_mul(stride)
        .context("Roster directory size overflow")?;
    ensure!(
        table >= region.base + 0x680 && region.contains(table, bytes),
        "Roster table exceeds its owned allocation"
    );
    Ok((
        Some(Directory {
            count: count as usize,
            pointer: table,
            stride,
        }),
        (address, header),
    ))
}

/// The caller must get this region from VirtualQueryEx as a committed private
/// readable roster allocation. No renderer/RPC is allowed to choose it.
/// This captures descriptors only; no player/team fields are decoded or edited.
pub(crate) fn inspect_owner(m: &impl Memory, region: OwnedRegion) -> Result<OwnerSnapshot> {
    verify_identity_profile(m)?;
    ensure!(region.valid(), "Invalid native roster allocation");
    let root = q(m, at(m, ROOT)?)?;
    ensure!(
        q(m, root)? == at(m, ROOT_CLASS)?,
        "Active roster container has a different class"
    );
    let owner = q(m, root.checked_add(0xa8).context("Roster owner overflow")?)?;
    ensure!(
        region.contains(owner, 0x2a0),
        "Active roster owner is outside its native allocation"
    );
    let (players, ph) = directory(m, region, owner, 0x10, PLAYER_STRIDE, 100, 8192, false)?;
    let (arenas, ah) = directory(m, region, owner, 0x60, ARENA_STRIDE, 1, 2048, false)?;
    let (teams, th) = directory(m, region, owner, 0x80, TEAM_STRIDE, 1, 1024, false)?;
    let (uniforms, uh) = directory(m, region, owner, 0x290, UNIFORM_STRIDE, 1, 8192, true)?;
    let players = players.unwrap();
    let arenas = arenas.unwrap();
    let teams = teams.unwrap();
    let mut spans = vec![&players, &arenas, &teams];
    if let Some(uniforms) = &uniforms {
        spans.push(uniforms);
    }
    spans.sort_by_key(|t| t.pointer);
    for pair in spans.windows(2) {
        ensure!(
            pair[0].pointer + (pair[0].count * pair[0].stride) as u64 <= pair[1].pointer,
            "Roster directories overlap"
        );
    }
    let snapshot = OwnerSnapshot {
        root,
        owner,
        region,
        players,
        arenas,
        teams,
        uniforms,
        headers: vec![ph, ah, th, uh],
    };
    revalidate(m, &snapshot)?;
    Ok(snapshot)
}
pub(crate) fn revalidate(m: &impl Memory, expected: &OwnerSnapshot) -> Result<()> {
    ensure!(
        q(m, at(m, ROOT)?)? == expected.root
            && q(m, expected.root)? == at(m, ROOT_CLASS)?
            && q(
                m,
                expected
                    .root
                    .checked_add(0xa8)
                    .context("Roster owner overflow")?
            )? == expected.owner,
        "Active roster changed owner"
    );
    for (address, header) in &expected.headers {
        ensure!(
            read(m, *address, 16)? == *header,
            "Roster directories changed during capture"
        );
    }
    m.checkpoint()
}

#[cfg(test)]
#[path = "roster_v2_profile_tests.rs"]
mod tests;
