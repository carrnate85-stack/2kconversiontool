//! Version 2 camera data capability. This module never opens a process, allocates
//! remote memory or writes. The owned runtime must journal/initialize all private
//! bytes before publishing these bounded compare-and-write operations. Private
//! allocations remain committed until the exact game process has exited.
#![allow(dead_code)]
use crate::{
    blacktop_native::Memory,
    camera_policy::{self as policy, Steps},
};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
};

pub(crate) const REGISTRY: u64 = 0x0992C830;
pub(crate) const MODES: u64 = 0x0992C6B0;
pub(crate) const LABELS: u64 = 0x06C27DC0;
pub(crate) const STATIC_AVAILABILITY: u64 = 0x06C27EE0;
pub(crate) const RUNTIME_AVAILABILITY: u64 = 0x0A67C430;
pub(crate) const IMAGE_BYTES: u64 = 0x35BDE000;
const PREFERRED: u64 = 0x140000000;
const SLOT_STRIDE: usize = 0xB000;
const SHA: &str = "29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1";
// Independent instruction-boundary proofs: ID*64 registry, ID*16 modes,
// ID*4 labels, byte eligibility, and both native runtime-availability loops.
pub(crate) const ANCHORS: &[(u64, &[u8])] = &[
    (
        0xC2DA40,
        &[
            0x48, 0x63, 0xC1, 0x48, 0xC1, 0xE0, 0x06, 0x48, 0x8D, 0x0D, 0xE2, 0xED, 0xCF, 0x08,
            0x48, 0x8B, 0x04, 0x08, 0xC3,
        ],
    ),
    (
        0xC2F8A0,
        &[
            0x48, 0x63, 0xC9, 0x48, 0xC1, 0xE1, 0x04, 0x48, 0x8D, 0x05, 0x02, 0xCE, 0xCF, 0x08,
            0x48, 0x01, 0xC8, 0xC3,
        ],
    ),
    (
        0xC2C8E0,
        &[
            0x48, 0xC1, 0xE1, 0x04, 0x48, 0x8D, 0x15, 0xC5, 0xFD, 0xCF, 0x08, 0x4C, 0x63, 0x44,
            0x11, 0x08,
        ],
    ),
    (
        0xC2D02E,
        &[
            0x83, 0xFF, 0x47, 0x0F, 0x87, 0xB3, 0x00, 0x00, 0x00, 0x48, 0x63, 0xDF, 0x48, 0x8D,
            0x05, 0x9F, 0xAE, 0xFF, 0x05, 0x80, 0x3C, 0x03, 0x00,
        ],
    ),
    (
        0xC2D334,
        &[
            0x48, 0x8D, 0x05, 0x85, 0xAA, 0xFF, 0x05, 0x48, 0x8D, 0x04, 0x98,
        ],
    ),
    (
        0xC2CA72,
        &[
            0x48, 0x8D, 0x0D, 0x47, 0xB3, 0xFF, 0x05, 0x48, 0x8D, 0x04, 0x81,
        ],
    ),
    (0xC2C91A, &[0x4C, 0x8D, 0x15, 0x0F, 0xFB, 0xA4, 0x09]),
    (
        0xC2C930,
        &[0x4E, 0x63, 0x1C, 0x8A, 0x43, 0xC6, 0x04, 0x13, 0x01],
    ),
    (0xC2C96C, &[0x4C, 0x8D, 0x0D, 0xBD, 0xFA, 0xA4, 0x09]),
    (
        0xC2C980,
        &[0x4E, 0x63, 0x14, 0x82, 0x43, 0xC6, 0x04, 0x0A, 0x01],
    ),
];
const SOURCE_IDS: [u32; 9] = [10, 21, 51, 52, 53, 54, 55, 57, 59];
const PARAMETER_IDS: [u32; 6] = [10, 21, 51, 52, 54, 59];
const MODE_IDS: [&[u32]; 13] = [
    &[1, 2, 3, 4, 15, 8, 9, 16, 17, 19, 20, 22, 23, 0],
    &[1, 2, 3, 4, 15, 8, 9, 16, 17, 19, 20, 22, 23, 0],
    &[1, 2, 3, 4, 15, 8, 9, 16, 17, 19, 20, 22, 23, 0],
    &[1, 2, 3, 4, 15, 11, 12, 16, 17, 19, 35, 20, 22, 23, 33],
    &[1, 2, 3, 4, 15, 11, 12, 16, 17, 19, 35, 20, 22, 23, 33],
    &[1, 2, 7, 3, 4, 16, 18, 35, 25, 20, 22],
    &[1, 2, 3, 4, 11, 12, 16, 22],
    &[1, 2, 3, 4, 11, 12, 16],
    &[1, 2, 3, 4, 11, 12, 16, 35, 22],
    &[5, 6, 13, 14],
    &[1, 2, 3, 4, 17, 20, 22],
    &[1, 2, 3, 7, 4, 15, 10, 12, 16, 17, 35, 20, 22],
    &[1, 2, 3, 4, 12],
];
fn d(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}
fn q(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}
fn put_q(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn read(m: &impl Memory, at: u64, len: usize) -> Result<Vec<u8>> {
    ensure!(
        len > 0
            && len <= 0x10000
            && at >= 0x10000
            && at
                .checked_add(len as u64)
                .is_some_and(|end| end <= 0x7fff00000000),
        "Invalid camera read range"
    );
    let mut bytes = Vec::with_capacity(len);
    for offset in (0..len).step_by(4096) {
        m.checkpoint()?;
        let count = (len - offset).min(4096);
        let chunk = m.read(at + offset as u64, count)?;
        ensure!(chunk.len() == count, "Partial camera read");
        bytes.extend(chunk);
    }
    Ok(bytes)
}
fn at(base: u64, rva: u64) -> Result<u64> {
    base.checked_add(rva).context("Camera RVA overflow")
}
fn hex(s: &str) -> Result<Vec<u8>> {
    ensure!(
        s.len() % 2 == 0 && s.len() <= 65536,
        "Invalid camera fixture hex"
    );
    s.as_bytes()
        .chunks_exact(2)
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}
fn stock() -> Result<serde_json::Value> {
    let v: serde_json::Value =
        serde_json::from_str(include_str!("../assets/camera-v2-stock.json"))?;
    ensure!(
        v["schema"] == "nba2k27-v2-camera-stock-v1" && v["sha256"] == SHA,
        "Invalid camera stock reference"
    );
    Ok(v)
}
fn stock_bytes(v: &serde_json::Value, key: &str, len: usize) -> Result<Vec<u8>> {
    let b = hex(v[key].as_str().context("Missing camera stock record")?)?;
    ensure!(b.len() == len, "Invalid camera stock reference length");
    Ok(b)
}
fn relocate(mut entry: Vec<u8>, base: u64, offsets: &[usize]) -> Result<Vec<u8>> {
    for &offset in offsets {
        let value = q(&entry, offset);
        ensure!(
            (PREFERRED..PREFERRED + IMAGE_BYTES).contains(&value),
            "Camera stock pointer outside image"
        );
        put_q(&mut entry, offset, at(base, value - PREFERRED)?);
    }
    Ok(entry)
}
#[derive(Clone, Copy)]
struct Section {
    rva: u64,
    virtual_bytes: u64,
    raw: u64,
    raw_bytes: u64,
    flags: u32,
}
fn header(bytes: &[u8], loaded_base: u64) -> Result<Vec<Section>> {
    ensure!(
        loaded_base >= 0x10000
            && loaded_base & 0xffff == 0
            && loaded_base
                .checked_add(IMAGE_BYTES)
                .is_some_and(|end| end < 0x7fff_0000_0000),
        "Invalid retained camera image base"
    );
    ensure!(
        bytes.len() == 4096 && &bytes[..2] == b"MZ",
        "Invalid camera executable header"
    );
    let pe = d(bytes, 0x3c) as usize;
    ensure!(
        (64..=3500).contains(&pe)
            && &bytes[pe..pe + 4] == b"PE\0\0"
            && &bytes[pe + 4..pe + 6] == [0x64, 0x86]
            && &bytes[pe + 24..pe + 26] == [0x0b, 0x02]
            && d(bytes, pe + 8) == 0x6AC53765
            && d(bytes, pe + 80) == IMAGE_BYTES as u32
            // The loader can rewrite this header field to the relocated base.
            // Accept only the disk preference or this retained process's base,
            // never an arbitrary third address. Disk validation passes PREFERRED.
            && [PREFERRED, loaded_base].contains(&q(bytes, pe + 48)),
        "Camera feature is not mapped for this game build"
    );
    let count = u16::from_le_bytes(bytes[pe + 6..pe + 8].try_into()?) as usize;
    let optional = u16::from_le_bytes(bytes[pe + 20..pe + 22].try_into()?) as usize;
    let start = pe + 24 + optional;
    ensure!(
        optional >= 112
            && (1..=96).contains(&count)
            && start.checked_add(count * 40).is_some_and(|end| end <= 4096),
        "Invalid camera section table"
    );
    (0..count)
        .map(|index| {
            let s = &bytes[start + index * 40..start + (index + 1) * 40];
            let result = Section {
                rva: d(s, 12) as u64,
                virtual_bytes: d(s, 8) as u64,
                raw: d(s, 20) as u64,
                raw_bytes: d(s, 16) as u64,
                flags: d(s, 36),
            };
            ensure!(
                result
                    .rva
                    .checked_add(result.virtual_bytes.max(result.raw_bytes))
                    .is_some_and(|end| end <= IMAGE_BYTES),
                "Camera section exceeds image"
            );
            Ok(result)
        })
        .collect()
}
fn section(
    sections: &[Section],
    rva: u64,
    len: usize,
    writable: bool,
    executable: bool,
) -> Result<Section> {
    let end = rva
        .checked_add(len as u64)
        .context("Camera section overflow")?;
    let found: Vec<_> = sections
        .iter()
        .filter(|s| rva >= s.rva && end <= s.rva + s.virtual_bytes.max(s.raw_bytes))
        .collect();
    ensure!(
        found.len() == 1,
        "Camera image range is absent or ambiguous"
    );
    let s = *found[0];
    ensure!(
        s.flags & 0x40000000 != 0
            && (s.flags & 0x80000000 != 0) == writable
            && (s.flags & 0x20000000 != 0) == executable,
        "Unexpected camera section permissions"
    );
    Ok(s)
}
/// This immutable-only gate is safe to repeat after owned image data changes.
pub(crate) fn verify_profile(m: &impl Memory) -> Result<()> {
    let base = m.base();
    ensure!(
        base >= 0x10000
            && base
                .checked_add(IMAGE_BYTES)
                .is_some_and(|end| end < 0x7fff00000000),
        "Invalid camera image base"
    );
    let sections = header(&read(m, base, 4096)?, base)?;
    for &(rva, bytes) in ANCHORS {
        section(&sections, rva, bytes.len(), false, true)?;
        ensure!(
            read(m, at(base, rva)?, bytes.len())? == bytes,
            "Version 2 camera instruction signature changed at {rva:08X}"
        );
    }
    for (rva, len, writable) in [
        (REGISTRY, 72 * 64, true),
        (MODES, 13 * 16, true),
        (LABELS, 72 * 4, false),
        (STATIC_AVAILABILITY, 72, false),
        (RUNTIME_AVAILABILITY, 72, true),
    ] {
        section(&sections, rva, len, writable, false)?;
    }
    m.checkpoint()
}
#[derive(Clone, Debug)]
pub(crate) struct Selection {
    pub id: u32,
    pub steps: Steps,
}
fn selected_ids(selected: &[Selection]) -> Result<Vec<u32>> {
    ensure!(
        !selected.is_empty() && selected.len() <= policy::MAX_EXTRA,
        "Choose between one and nine cameras"
    );
    let mut ids = Vec::new();
    for s in selected {
        ensure!(
            policy::CURATED.iter().any(|(id, _)| *id == s.id) && !ids.contains(&s.id),
            "Unknown or duplicate selected camera"
        );
        Steps::new(s.steps.height, s.steps.zoom)?;
        ids.push(s.id);
    }
    Ok(ids)
}
pub(crate) fn required_bytes(selected: &[Selection]) -> Result<usize> {
    selected_ids(selected)?;
    Ok(selected.len() * SLOT_STRIDE + 13 * 0x1000)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EditKind {
    Registry,
    Label,
    StaticAvailability,
    RuntimeAvailability,
    ModePointer,
    ModeCount,
    PrivateGeometry,
}
#[derive(Clone, Debug)]
pub(crate) struct Edit {
    pub at: u64,
    pub before: Vec<u8>,
    pub after: Vec<u8>,
    pub kind: EditKind,
}
#[derive(Clone, Debug)]
pub(crate) struct PrivateInit {
    pub at: u64,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug)]
struct Slot {
    id: u32,
    entry_at: u64,
    entry: Vec<u8>,
    parameters: Vec<(u64, Vec<u8>)>,
    steps: Steps,
}
#[derive(Clone, Debug)]
pub(crate) struct Plan {
    pub private_base: u64,
    pub allocation_bytes: usize,
    pub initialization: Vec<PrivateInit>,
    pub edits: Vec<Edit>,
    slots: Vec<Slot>,
    guards: Vec<(u64, Vec<u8>)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Publication {
    Active,
    FullyReset,
    Transitional,
    Conflict,
}
#[derive(Clone)]
pub(crate) struct Capture {
    base: u64,
    registry: Vec<u8>,
    labels: Vec<u8>,
    availability: Vec<u8>,
    runtime: Vec<u8>,
    modes: Vec<u8>,
    parameters: BTreeMap<(u32, bool), Vec<u8>>,
    metadata: BTreeMap<u64, Vec<u8>>,
}
/// Captures only the stock layout, never adopts a previous launcher mapping.
pub(crate) fn capture(m: &impl Memory) -> Result<Capture> {
    verify_profile(m)?;
    let base = m.base();
    let v = stock()?;
    let registry = read(m, at(base, REGISTRY)?, 72 * 64)?;
    let expected = stock_bytes(&v, "registry", 72 * 64)?;
    for id in SOURCE_IDS.into_iter().chain(63..72) {
        let source = &registry[id as usize * 64..id as usize * 64 + 64];
        let want = relocate(
            expected[id as usize * 64..id as usize * 64 + 64].to_vec(),
            base,
            &[0x10, 0x18, 0x30, 0x38],
        )?;
        // Native code replaces the entire sixteen-byte settings block at +20.
        ensure!(
            source[..0x20] == want[..0x20] && source[0x30..] == want[0x30..],
            "Camera source/extra slot {id} was changed by another owner"
        );
    }
    let labels = read(m, at(base, LABELS)?, 72 * 4)?;
    ensure!(
        labels == stock_bytes(&v, "labels", 72 * 4)?,
        "Camera labels are not stock"
    );
    let availability = read(m, at(base, STATIC_AVAILABILITY)?, 72)?;
    ensure!(
        availability == stock_bytes(&v, "staticAvailability", 72)?,
        "Camera availability is not stock"
    );
    let runtime = read(m, at(base, RUNTIME_AVAILABILITY)?, 72)?;
    ensure!(
        runtime.iter().all(|b| *b <= 1),
        "Unsupported native camera availability state"
    );
    let modes = read(m, at(base, MODES)?, 13 * 16)?;
    let expected_modes = stock_bytes(&v, "modes", 13 * 16)?;
    for index in 0..13 {
        let descriptor = &modes[index * 16..index * 16 + 12];
        let expected = relocate(
            expected_modes[index * 16..index * 16 + 12].to_vec(),
            base,
            &[0],
        )?;
        ensure!(
            descriptor == expected,
            "Camera mode {index} is already owned or changed"
        );
        let ids: Vec<u8> = MODE_IDS[index]
            .iter()
            .flat_map(|id| id.to_le_bytes())
            .collect();
        ensure!(
            read(m, q(descriptor, 0), ids.len())? == ids,
            "Stock camera mode {index} list changed"
        );
    }
    let mut parameters = BTreeMap::new();
    for id in PARAMETER_IDS {
        let entry = &registry[id as usize * 64..id as usize * 64 + 64];
        for secondary in [false, true] {
            let address = q(entry, if secondary { 0x38 } else { 0x30 });
            let bytes = read(m, address, policy::PARAMETER_BYTES)?;
            for (index, start) in [0x10, 0x30].into_iter().enumerate() {
                let wanted = hex(v["framing"][id.to_string()][index]
                    .as_str()
                    .context("Missing camera framing reference")?)?;
                ensure!(
                    bytes[start..start + 20] == wanted,
                    "Camera {id} stock framing changed"
                );
            }
            parameters.insert((id, secondary), bytes);
        }
    }
    let mut metadata = BTreeMap::new();
    for id in [10, 51, 52, 53, 55] {
        let entry = &registry[id as usize * 64..id as usize * 64 + 64];
        let pointer = q(entry, 0x10);
        ensure!(
            q(entry, 0x18) == pointer + 0x24
                && pointer >= base
                && pointer + 52 <= base + IMAGE_BYTES,
            "Camera eligibility pointer changed"
        );
        let bytes = read(m, pointer, policy::METADATA_BYTES)?;
        policy::unrestricted_metadata(&bytes)?;
        metadata.insert(pointer, bytes);
    }
    // A mode descriptor changing while source data is copied is not a snapshot.
    ensure!(
        modes == read(m, at(base, MODES)?, 13 * 16)?
            && registry == read(m, at(base, REGISTRY)?, 72 * 64)?
            && labels == read(m, at(base, LABELS)?, 72 * 4)?
            && availability == read(m, at(base, STATIC_AVAILABILITY)?, 72)?
            && runtime == read(m, at(base, RUNTIME_AVAILABILITY)?, 72)?,
        "Camera state changed during capture; retry at a safe boundary"
    );
    m.checkpoint()?;
    Ok(Capture {
        base,
        registry,
        labels,
        availability,
        runtime,
        modes,
        parameters,
        metadata,
    })
}
impl Capture {
    fn entry(&self, id: u32) -> &[u8] {
        &self.registry[id as usize * 64..id as usize * 64 + 64]
    }
    pub(crate) fn plan(&self, selected: &[Selection], private_base: u64) -> Result<Plan> {
        let ids = selected_ids(selected)?;
        let size = required_bytes(selected)?;
        let end = private_base
            .checked_add(size as u64)
            .context("Camera private allocation overflow")?;
        ensure!(
            private_base >= 0x10000
                && private_base % 4096 == 0
                && end <= 0x7fff00000000
                && (end <= self.base || private_base >= self.base + IMAGE_BYTES),
            "Camera allocation is unaligned, outside user space or overlaps the game image"
        );
        let mut guards = vec![
            (at(self.base, REGISTRY)?, self.registry.clone()),
            (at(self.base, LABELS)?, self.labels.clone()),
            (
                at(self.base, STATIC_AVAILABILITY)?,
                self.availability.clone(),
            ),
            (at(self.base, RUNTIME_AVAILABILITY)?, self.runtime.clone()),
            (at(self.base, MODES)?, self.modes.clone()),
        ];
        guards.extend(
            self.metadata
                .iter()
                .map(|(pointer, bytes)| (*pointer, bytes.clone())),
        );
        for ((id, secondary), bytes) in &self.parameters {
            let pointer = q(self.entry(*id), if *secondary { 0x38 } else { 0x30 });
            for offset in [0x10, 0x30] {
                guards.push((pointer + offset as u64, bytes[offset..offset + 20].to_vec()));
            }
        }
        let mut plan = Plan {
            private_base,
            allocation_bytes: size,
            initialization: Vec::new(),
            edits: Vec::new(),
            slots: Vec::new(),
            guards,
        };
        for (index, selection) in selected.iter().enumerate() {
            let id = selection.id;
            let slot = 63 + index as u32;
            let offset = index * SLOT_STRIDE;
            let primary = private_base + offset as u64;
            let secondary = primary + 0x5000;
            let original = self.entry(slot).to_vec();
            let mut tracking = if id == 53 {
                policy::press_27_tracking(
                    self.entry(id),
                    self.entry(54),
                    &self.parameters[&(21, false)],
                    &self.parameters[&(54, false)],
                    &self.parameters[&(54, true)],
                    primary,
                    secondary,
                )?
            } else if matches!(id, 51 | 52) {
                let donor = if id == 52 { 59 } else { 10 };
                policy::single_tracking(
                    id,
                    self.entry(id),
                    self.entry(donor),
                    &self.parameters[&(id, false)],
                    &self.parameters[&(donor, false)],
                    primary,
                )?
            } else {
                policy::Tracking {
                    entry: self.entry(id).to_vec(),
                    primary: Vec::new(),
                    secondary: None,
                }
            };
            let mut private_parameters = Vec::new();
            if !tracking.primary.is_empty() {
                tracking.primary =
                    policy::stepped_parameters(id, &tracking.primary, selection.steps)?;
                private_parameters.push((primary, tracking.primary.clone()));
                plan.initialization.push(PrivateInit {
                    at: primary,
                    bytes: tracking.primary,
                });
                if let Some(bytes) = tracking.secondary {
                    let bytes = policy::stepped_parameters(id, &bytes, selection.steps)?;
                    private_parameters.push((secondary, bytes.clone()));
                    plan.initialization.push(PrivateInit {
                        at: secondary,
                        bytes,
                    });
                }
            }
            if matches!(id, 51 | 52 | 53 | 55) {
                let pointer = q(&tracking.entry, 0x10);
                let source = self
                    .metadata
                    .get(&pointer)
                    .context("Eligibility donor was not captured")?;
                let clone = primary + 0xA000;
                plan.initialization.push(PrivateInit {
                    at: clone,
                    bytes: policy::unrestricted_metadata(source)?,
                });
                put_q(&mut tracking.entry, 0x10, clone);
                put_q(&mut tracking.entry, 0x18, clone + 0x24);
            }
            let mut entry = original.clone();
            entry[8..].copy_from_slice(&tracking.entry[8..]);
            put_q(&mut entry, 0, crate::camera_names::key(id)?);
            entry[0x20..0x24].copy_from_slice(&selection.steps.zoom.to_le_bytes());
            entry[0x24..0x28].copy_from_slice(&selection.steps.height.to_le_bytes());
            let entry_at = at(self.base, REGISTRY + slot as u64 * 64)?;
            plan.edits.push(Edit {
                at: entry_at,
                before: original,
                after: entry.clone(),
                kind: EditKind::Registry,
            });
            let donor = match id {
                51 => 38,
                57 => 37,
                _ => id,
            };
            plan.edits.push(Edit {
                at: at(self.base, LABELS + slot as u64 * 4)?,
                before: self.labels[slot as usize * 4..slot as usize * 4 + 4].to_vec(),
                after: self.labels[donor as usize * 4..donor as usize * 4 + 4].to_vec(),
                kind: EditKind::Label,
            });
            plan.edits.push(Edit {
                at: at(self.base, STATIC_AVAILABILITY + slot as u64)?,
                before: vec![self.availability[slot as usize]],
                after: vec![0],
                kind: EditKind::StaticAvailability,
            });
            plan.edits.push(Edit {
                at: at(self.base, RUNTIME_AVAILABILITY + slot as u64)?,
                before: vec![self.runtime[slot as usize]],
                after: vec![1],
                kind: EditKind::RuntimeAvailability,
            });
            plan.slots.push(Slot {
                id,
                entry_at,
                entry,
                parameters: private_parameters,
                steps: selection.steps,
            });
        }
        let lists = private_base + (selected.len() * SLOT_STRIDE) as u64;
        for (index, source) in MODE_IDS.iter().enumerate() {
            let ids = policy::mode_ids(source, &ids)?;
            ensure!(
                ids.len() >= source.len(),
                "Camera list transition could expose an out-of-bounds old count"
            );
            let pointer = lists + index as u64 * 4096;
            plan.initialization.push(PrivateInit {
                at: pointer,
                bytes: ids.iter().flat_map(|id| id.to_le_bytes()).collect(),
            });
            let descriptor = at(self.base, MODES + index as u64 * 16)?;
            plan.edits.push(Edit {
                at: descriptor,
                before: self.modes[index * 16..index * 16 + 8].to_vec(),
                after: pointer.to_le_bytes().to_vec(),
                kind: EditKind::ModePointer,
            });
            plan.edits.push(Edit {
                at: descriptor + 8,
                before: self.modes[index * 16 + 8..index * 16 + 12].to_vec(),
                after: (ids.len() as u32).to_le_bytes().to_vec(),
                kind: EditKind::ModeCount,
            });
        }
        ensure!(
            plan.initialization
                .iter()
                .all(|init| init.at >= private_base && init.at + init.bytes.len() as u64 <= end),
            "Private camera initialization escaped allocation"
        );
        Ok(plan)
    }
}
impl Plan {
    fn private_parameters_owned(id: u32, expected: &[u8], current: &[u8]) -> Result<bool> {
        ensure!(
            expected.len() == policy::PARAMETER_BYTES && current.len() == policy::PARAMETER_BYTES,
            "Private camera parameter length changed"
        );
        const GEOMETRY: [usize; 6] = [0x14, 0x18, 0x20, 0x34, 0x38, 0x40];
        for offset in (0..expected.len()).step_by(4) {
            if !GEOMETRY.contains(&offset)
                && current[offset..offset + 4] != expected[offset..offset + 4]
            {
                return Ok(false);
            }
        }
        policy::owns_stepped_geometry(id, current)
    }

    /// Re-publication must not point the native game back at a damaged retained
    /// allocation. Mode lists and metadata stay byte-exact. Parameter clones
    /// may differ only by one of the bounded native height/zoom combinations
    /// that maintenance itself can produce.
    pub(crate) fn compare_private(&self, m: &impl Memory) -> Result<()> {
        verify_profile(m)?;
        for init in &self.initialization {
            let current = read(m, init.at, init.bytes.len())?;
            let parameter = self.slots.iter().find_map(|slot| {
                slot.parameters
                    .iter()
                    .find(|(at, _)| *at == init.at)
                    .map(|(_, expected)| (slot.id, expected))
            });
            let owned = match parameter {
                Some((id, expected)) => {
                    ensure!(
                        expected == &init.bytes,
                        "Private camera initialization record differs"
                    );
                    Self::private_parameters_owned(id, expected, &current)?
                }
                None => current == init.bytes,
            };
            ensure!(
                owned,
                "Retained private camera data changed before re-publication"
            );
        }
        m.checkpoint()
    }

    /// Classifies every public edit without writing. Height/zoom are the only
    /// native-owned bytes allowed to vary while a published registry slot is
    /// active. A mixed before/after generation is transitional, not active.
    pub(crate) fn publication(&self, m: &impl Memory) -> Result<Publication> {
        verify_profile(m)?;
        let mut all_active = true;
        let mut all_reset = true;
        for edit in &self.edits {
            let current = read(m, edit.at, edit.after.len())?;
            // Game-owned bytes: the native settings block at +20 (restore and
            // maintenance already adopt it) and runtime availability, which the
            // game clears/sets itself on mode changes. Neither is ownership.
            let (active, reset) = match edit.kind {
                EditKind::Registry => (
                    current[..0x20] == edit.after[..0x20] && current[0x30..] == edit.after[0x30..],
                    current[..0x20] == edit.before[..0x20]
                        && current[0x30..] == edit.before[0x30..],
                ),
                EditKind::RuntimeAvailability => {
                    if current.len() != 1 || current[0] > 1 {
                        m.checkpoint()?;
                        return Ok(Publication::Conflict);
                    }
                    continue;
                }
                _ => (current == edit.after, current == edit.before),
            };
            if !active && !reset {
                m.checkpoint()?;
                return Ok(Publication::Conflict);
            }
            all_active &= active;
            all_reset &= reset;
        }
        m.checkpoint()?;
        Ok(if all_active {
            Publication::Active
        } else if all_reset {
            Publication::FullyReset
        } else {
            Publication::Transitional
        })
    }

    /// A fresh stock capture re-planned into the retained allocation must
    /// describe byte-identical private data before it can be republished.
    pub(crate) fn same_private_layout(&self, other: &Plan) -> bool {
        self.private_base == other.private_base
            && self.allocation_bytes == other.allocation_bytes
            && self.initialization.len() == other.initialization.len()
            && self
                .initialization
                .iter()
                .zip(&other.initialization)
                .all(|(a, b)| a.at == b.at && a.bytes == b.bytes)
    }
    /// Initial publication must still match the complete captured source state.
    pub(crate) fn compare_before(&self, m: &impl Memory) -> Result<()> {
        verify_profile(m)?;
        for (at, bytes) in &self.guards {
            ensure!(
                read(m, *at, bytes.len())? == *bytes,
                "Camera source capture changed before publication"
            );
        }
        for e in &self.edits {
            ensure!(
                read(m, e.at, e.before.len())? == e.before,
                "Camera publication precondition changed"
            );
        }
        m.checkpoint()
    }
    /// Returns reverse-ordered restores (mode count BEFORE mode pointer). Only
    /// native settings block changes are adopted. A foreign key/pointer/class,
    /// label or static mask refuses the entire restore before any write.
    pub(crate) fn restore(&self, m: &impl Memory) -> Result<Vec<Edit>> {
        verify_profile(m)?;
        let mut output = Vec::new();
        for e in self.edits.iter().rev() {
            let current = read(m, e.at, e.after.len())?;
            if current == e.before {
                continue;
            }
            let owned = match e.kind {
                EditKind::Registry => {
                    current[..0x20] == e.after[..0x20] && current[0x30..] == e.after[0x30..]
                }
                // The native game may clear runtime flags when changing modes.
                EditKind::RuntimeAvailability => current.len() == 1 && current[0] <= 1,
                _ => current == e.after,
            };
            ensure!(
                owned,
                "Camera restore conflict; foreign changes left untouched"
            );
            output.push(Edit {
                at: e.at,
                before: current,
                after: e.before.clone(),
                kind: e.kind.clone(),
            });
        }
        m.checkpoint()?;
        Ok(output)
    }
    /// Native H/Z changes update only the six known geometry DWORDs in our
    /// private parameter copies. Locking re-applies the user's launch settings;
    /// when disabled (default), native H/Z is used without overwriting it.
    pub(crate) fn maintenance(&self, m: &impl Memory, lock: bool) -> Result<Vec<Edit>> {
        verify_profile(m)?;
        let mut output = Vec::new();
        for slot in &self.slots {
            let current = read(m, slot.entry_at, 64)?;
            ensure!(
                current[..0x20] == slot.entry[..0x20] && current[0x30..] == slot.entry[0x30..],
                "Camera slot ownership changed; maintenance stopped"
            );
            let steps = if lock {
                slot.steps
            } else {
                Steps::new(d(&current, 0x24) as i32, d(&current, 0x20) as i32)?
            };
            if lock && current[0x20..0x28] != slot.entry[0x20..0x28] {
                output.push(Edit {
                    at: slot.entry_at + 0x20,
                    before: current[0x20..0x28].to_vec(),
                    after: slot.entry[0x20..0x28].to_vec(),
                    kind: EditKind::Registry,
                });
            }
            for (pointer, original) in &slot.parameters {
                let desired = policy::stepped_parameters(slot.id, original, steps)?;
                for offset in [0x14, 0x18, 0x20, 0x34, 0x38, 0x40] {
                    let before = read(m, pointer + offset as u64, 4)?;
                    let after = desired[offset..offset + 4].to_vec();
                    if before != after {
                        output.push(Edit {
                            at: pointer + offset as u64,
                            before,
                            after,
                            kind: EditKind::PrivateGeometry,
                        });
                    }
                }
            }
        }
        m.checkpoint()?;
        Ok(output)
    }
}

/// Full disk pin plus independent camera anchors/layout. Does not open a game
/// process. The caller retains its read/share-only file handle for the session.
pub(crate) fn verify_disk(file: &mut File) -> Result<Capture> {
    ensure!(
        file.metadata()?.len() == 870_548_760,
        "Unexpected Version 2 camera image size"
    );
    file.seek(SeekFrom::Start(0))?;
    let mut hasher = Sha256::new();
    let mut block = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut block)?;
        if n == 0 {
            break;
        }
        hasher.update(&block[..n]);
    }
    ensure!(
        format!("{:X}", hasher.finalize()) == SHA,
        "Camera feature executable hash is not mapped"
    );
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = vec![0; 4096];
    file.read_exact(&mut bytes)?;
    let sections = header(&bytes, PREFERRED)?;
    struct Disk<'a> {
        file: std::cell::RefCell<&'a mut File>,
        sections: Vec<Section>,
    }
    impl Memory for Disk<'_> {
        fn base(&self) -> u64 {
            PREFERRED
        }
        fn read(&self, address: u64, len: usize) -> Result<Vec<u8>> {
            let rva = address
                .checked_sub(PREFERRED)
                .context("Disk camera RVA underflow")?;
            if rva == 0 && len == 4096 {
                let mut f = self.file.borrow_mut();
                f.seek(SeekFrom::Start(0))?;
                let mut b = vec![0; len];
                f.read_exact(&mut b)?;
                return Ok(b);
            }
            let end = rva
                .checked_add(len as u64)
                .context("Disk camera range overflow")?;
            let found: Vec<_> = self
                .sections
                .iter()
                .filter(|s| rva >= s.rva && end <= s.rva + s.virtual_bytes.max(s.raw_bytes))
                .collect();
            ensure!(found.len() == 1, "Disk camera RVA is absent or ambiguous");
            let s = found[0];
            let relative = rva - s.rva;
            let available = s.raw_bytes.saturating_sub(relative).min(len as u64) as usize;
            let mut bytes = vec![0; len];
            if available > 0 {
                let mut f = self.file.borrow_mut();
                f.seek(SeekFrom::Start(s.raw + relative))?;
                f.read_exact(&mut bytes[..available])?;
            }
            Ok(bytes)
        }
    }
    capture(&Disk {
        file: std::cell::RefCell::new(file),
        sections,
    })
}

#[cfg(test)]
#[path = "camera_v2_tests.rs"]
mod tests;
