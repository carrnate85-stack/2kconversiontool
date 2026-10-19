//! Pure camera data transforms. No process handles, game discovery, executable
//! patches or automatic writes. Runtime must independently verify its build,
//! source records, private allocations and immutable launch ownership.
#![allow(dead_code)]
use anyhow::{Result, ensure};
use std::collections::HashSet;

pub(crate) const ENTRY_BYTES: usize = 0x40;
pub(crate) const PARAMETER_BYTES: usize = 0x4990;
pub(crate) const METADATA_BYTES: usize = 0x34;
pub(crate) const FIRST_EXTRA: u32 = 63;
pub(crate) const MAX_EXTRA: usize = 9;
pub(crate) const CURATED: [(u32, &str); 7] = [
    (51, "Broadcast Low 2K18"),
    (53, "Press 2K17"),
    (54, "Press 2K27"),
    (52, "Beluba 2K17"),
    (57, "Beluba 2K27"),
    (59, "Side"),
    (55, "Nosebleeds"),
];
const PRESS: [f32; 5] = [-1350., 400., 0., 1., 45.];
const PRESS_27: [f32; 5] = [-1300., 400., 0., 1., 50.];
const LOW_18: [f32; 5] = [-1900., 900., 0., 1., 32.];
const BELUBA: [f32; 5] = [-1900., 1200., 1700., 1., 38.];
const SIDE: [f32; 5] = [-1750., 675., 0., 1., 70.];
const BELUBA_HYBRID: [f32; 5] = [-1900., 400., 0., 1., 67.041786];

fn d(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}
fn q(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}
fn f(bytes: &[u8], at: usize) -> f32 {
    f32::from_bits(d(bytes, at))
}
fn set_f(bytes: &mut [u8], at: usize, value: f32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn set_q(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn near(value: f32, expected: f32) -> bool {
    value.is_finite() && (value - expected).abs() < 0.01
}
fn framing(bytes: &[u8], expected: [f32; 5]) -> Result<()> {
    ensure!(
        bytes.len() == PARAMETER_BYTES,
        "Camera parameter length changed"
    );
    for start in [0x10, 0x30] {
        for (i, value) in expected.iter().enumerate() {
            ensure!(
                near(f(bytes, start + i * 4), *value),
                "Camera framing signature changed"
            );
        }
    }
    Ok(())
}
fn set_framing(bytes: &mut [u8], values: [f32; 5]) {
    for start in [0x10, 0x30] {
        for (i, value) in values.iter().enumerate() {
            set_f(bytes, start + i * 4, *value);
        }
    }
}
fn set_pose_preserving_w(bytes: &mut [u8], values: [f32; 5]) {
    for start in [0x10, 0x30] {
        for i in [0, 1, 2, 4] {
            set_f(bytes, start + i * 4, values[i]);
        }
    }
}
fn span(address: u64, length: usize) -> Result<(u64, u64)> {
    let end = address
        .checked_add(u64::try_from(length)?)
        .ok_or_else(|| anyhow::anyhow!("Camera address overflow"))?;
    ensure!(
        address >= 65536 && end <= 0x7fff_0000_0000 && address % 16 == 0,
        "Invalid private camera range"
    );
    Ok((address, end))
}
fn disjoint(a: (u64, u64), b: (u64, u64)) -> bool {
    a.1 <= b.0 || b.1 <= a.0
}
pub(crate) fn unrestricted_metadata(source: &[u8]) -> Result<Vec<u8>> {
    ensure!(
        source.len() == METADATA_BYTES,
        "Camera eligibility descriptor length changed"
    );
    for (i, expected) in [1, 1, 1, 10, 1, 10].into_iter().enumerate() {
        ensure!(
            d(source, i * 4) == expected,
            "Camera eligibility descriptor changed"
        );
    }
    let mut output = source.to_vec();
    output[4..8].copy_from_slice(&0u32.to_le_bytes());
    Ok(output)
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Tracking {
    pub entry: Vec<u8>,
    pub primary: Vec<u8>,
    pub secondary: Option<Vec<u8>>,
}
pub(crate) fn press_27_tracking(
    target: &[u8],
    donor: &[u8],
    press_parameters: &[u8],
    primary: &[u8],
    secondary: &[u8],
    primary_clone: u64,
    secondary_clone: u64,
) -> Result<Tracking> {
    ensure!(
        target.len() == ENTRY_BYTES && donor.len() == ENTRY_BYTES && d(donor, 8) == 3,
        "Press 2K27 behavior entry changed"
    );
    let a = span(primary_clone, PARAMETER_BYTES)?;
    let b = span(secondary_clone, PARAMETER_BYTES)?;
    ensure!(disjoint(a, b), "Private camera ranges overlap");
    let source_a = span(q(donor, 0x30), PARAMETER_BYTES)?;
    let source_b = span(q(donor, 0x38), PARAMETER_BYTES)?;
    ensure!(
        source_a.0 != source_b.0
            && disjoint(a, source_a)
            && disjoint(a, source_b)
            && disjoint(b, source_a)
            && disjoint(b, source_b),
        "Camera clone overlaps a source record"
    );
    // Stock donor records may intentionally overlap; never write them.
    framing(press_parameters, PRESS)?;
    framing(primary, PRESS_27)?;
    framing(secondary, PRESS_27)?;
    let mut output = Tracking {
        entry: target.to_vec(),
        primary: primary.to_vec(),
        secondary: Some(secondary.to_vec()),
    };
    set_pose_preserving_w(&mut output.primary, PRESS);
    set_pose_preserving_w(output.secondary.as_mut().unwrap(), PRESS);
    output.entry[8..16].copy_from_slice(&donor[8..16]);
    output.entry[0x28..0x30].copy_from_slice(&donor[0x28..0x30]);
    set_q(&mut output.entry, 0x30, primary_clone);
    set_q(&mut output.entry, 0x38, secondary_clone);
    Ok(output)
}
pub(crate) fn beluba_side_parameters(source: &[u8], side: &[u8]) -> Result<Vec<u8>> {
    framing(source, BELUBA)?;
    framing(side, SIDE)?;
    let mut output = side.to_vec();
    set_framing(&mut output, BELUBA_HYBRID);
    Ok(output)
}
pub(crate) fn single_tracking(
    id: u32,
    target: &[u8],
    donor: &[u8],
    source: &[u8],
    tracking: &[u8],
    clone: u64,
) -> Result<Tracking> {
    ensure!(
        target.len() == ENTRY_BYTES && donor.len() == ENTRY_BYTES && d(donor, 8) == 8,
        "Camera tracking class changed"
    );
    let private = span(clone, PARAMETER_BYTES)?;
    for entry in [target, donor] {
        for offset in [0x30, 0x38] {
            ensure!(
                disjoint(private, span(q(entry, offset), PARAMETER_BYTES)?),
                "Private camera range overlaps a source"
            );
        }
    }
    ensure!(
        tracking.len() == PARAMETER_BYTES,
        "Camera tracking record length changed"
    );
    let mut parameters = match id {
        52 => beluba_side_parameters(source, tracking)?,
        51 | 21 => {
            framing(source, if id == 51 { LOW_18 } else { PRESS })?;
            let mut output = tracking.to_vec();
            set_pose_preserving_w(
                &mut output,
                if id == 51 {
                    [-2100., 450., 0., 1., 64.023117]
                } else {
                    [-1400., 400., 90.5612, 1., 41.11708]
                },
            );
            output
        }
        _ => anyhow::bail!("No verified tracking conversion for this camera"),
    };
    // Geometry is the only changed part of the private parameter copy.
    ensure!(
        parameters.len() == PARAMETER_BYTES,
        "Incomplete private tracking record"
    );
    let mut entry = target.to_vec();
    let copy_end = if id == 52 { 0x10 } else { 0x20 };
    entry[8..copy_end].copy_from_slice(&donor[8..copy_end]);
    entry[0x28..0x30].copy_from_slice(&donor[0x28..0x30]);
    set_q(&mut entry, 0x30, clone);
    set_q(&mut entry, 0x38, clone);
    Ok(Tracking {
        entry,
        primary: std::mem::take(&mut parameters),
        secondary: None,
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Steps {
    pub height: i32,
    pub zoom: i32,
}
impl Steps {
    pub fn new(height: i32, zoom: i32) -> Result<Self> {
        ensure!(
            (-100..=100).contains(&height) && (-100..=100).contains(&zoom),
            "Camera height and zoom must be whole values from -100 to 100"
        );
        Ok(Self { height, zoom })
    }
    pub fn from_stored(height: f64, zoom: f64) -> Result<Self> {
        ensure!(
            height.is_finite()
                && zoom.is_finite()
                && height.fract() == 0.
                && zoom.fract() == 0.
                && (-100.0..=100.0).contains(&height)
                && (-100.0..=100.0).contains(&zoom),
            "Choose whole camera height and zoom values from -100 to 100 before native application"
        );
        Self::new(height as i32, zoom as i32)
    }
}
pub(crate) fn stepped_parameters(id: u32, source: &[u8], steps: Steps) -> Result<Vec<u8>> {
    ensure!(
        source.len() == PARAMETER_BYTES,
        "Incomplete private camera parameters"
    );
    Steps::new(steps.height, steps.zoom)?;
    let (base_y, height_step, target_z, base_fov, zoom_step): (f32, f32, f32, f32, f32) = match id {
        21 => (400., 22.22222, 90.5612, 41.11708, 1.111111),
        53 => (400., 22.22222, 0., 45., 1.111111),
        51 => (450., 55.55556, 0., 64.023117, 4.444444),
        52 => (400., 88.88889, 0., 67.041786, 6.666667),
        _ => anyhow::bail!("This camera uses native height and zoom, not a mapped legacy step"),
    };
    let y = base_y + ((steps.height - 1) as f32 * height_step);
    let fov = base_fov + ((steps.zoom - 1) as f32 * zoom_step);
    ensure!(
        y.is_finite() && fov.is_finite(),
        "Invalid camera step geometry"
    );
    let mut output = source.to_vec();
    for start in [0x10, 0x30] {
        set_f(&mut output, start + 4, y);
        set_f(&mut output, start + 8, target_z);
        set_f(&mut output, start + 16, fov);
    }
    Ok(output)
}

/// True only for geometry that this policy could have produced from one
/// bounded height and one bounded zoom selection. The caller separately owns
/// and compares every non-geometry byte in the private parameter record.
pub(crate) fn owns_stepped_geometry(id: u32, current: &[u8]) -> Result<bool> {
    ensure!(
        current.len() == PARAMETER_BYTES,
        "Incomplete private camera parameters"
    );
    let (base_y, height_step, target_z, base_fov, zoom_step): (f32, f32, f32, f32, f32) = match id {
        21 => (400., 22.22222, 90.5612, 41.11708, 1.111111),
        53 => (400., 22.22222, 0., 45., 1.111111),
        51 => (450., 55.55556, 0., 64.023117, 4.444444),
        52 => (400., 88.88889, 0., 67.041786, 6.666667),
        _ => anyhow::bail!("This camera uses native height and zoom, not mapped geometry"),
    };
    let y = d(current, 0x14);
    let target = d(current, 0x18);
    let fov = d(current, 0x20);
    Ok(d(current, 0x34) == y
        && d(current, 0x38) == target
        && d(current, 0x40) == fov
        && target == target_z.to_bits()
        && (-100..=100).any(|height| (base_y + ((height - 1) as f32 * height_step)).to_bits() == y)
        && (-100..=100).any(|zoom| (base_fov + ((zoom - 1) as f32 * zoom_step)).to_bits() == fov))
}
pub(crate) fn mode_ids(source: &[u32], selected: &[u32]) -> Result<Vec<u32>> {
    ensure!(
        !source.is_empty() && source.len() <= 64 && source.iter().all(|id| *id < 72),
        "Invalid stock camera list"
    );
    ensure!(
        !selected.is_empty() && selected.len() <= MAX_EXTRA,
        "Choose between one and nine cameras"
    );
    let mut unique = HashSet::new();
    ensure!(
        selected.iter().all(
            |id| (CURATED.iter().any(|(known, _)| id == known) || *id == 21) && unique.insert(*id)
        ),
        "Unknown or duplicate camera selection"
    );
    let mut seen = HashSet::new();
    let mut output: Vec<u32> = source
        .iter()
        .copied()
        .filter(|id| !selected.contains(id) && *id < FIRST_EXTRA && seen.insert(*id))
        .collect();
    output.extend((0..selected.len()).map(|i| FIRST_EXTRA + i as u32));
    ensure!(
        output.len() <= 72,
        "Expanded camera list exceeds native slots"
    );
    Ok(output)
}

#[cfg(test)]
#[path = "camera_policy_tests.rs"]
mod tests;
