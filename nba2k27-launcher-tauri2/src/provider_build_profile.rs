//! Disk-only Version 2 file-provider ABI gate. This never grants memory-write authority.
use anyhow::{Result, ensure};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
};

const BASE: u64 = 0x140000000;
const TABLE: u32 = 0x07FA3E18;
const TARGETS: [u32; 47] = [
    0x04CA10D0, 0x04CA1170, 0x001661A0, 0x04CA1180, 0x1580, 0x1550, 0x1590, 0x15A0, 0x15C0, 0x1580,
    0x1580, 0x15D0, 0x15E0, 0x15C0, 0x15C0, 0x1580, 0x1580, 0x1580, 0x15D0, 0x04CA10A0, 0x04CA0B80,
    0x04CA0EB0, 0x04CA0F10, 0x04CA0E20, 0x04CA0E60, 0x04CA0B90, 0x04CA0BA0, 0x04CA0BB0, 0x04CA0BC0,
    0x04CA0BD0, 0x04CA0FD0, 0x04CA0C90, 0x04CA0CF0, 0x04CA0CA0, 0x04CA0CC0, 0x04CA0BE0, 0x04CA0BF0,
    0x04CA0C00, 0x04CA0C10, 0x04CA0C20, 0x04CA0C40, 0x04CA0C60, 0x04CA0D70, 0x04CA0CD0, 0x04CA0CE0,
    0x04CA0C30, 0x04CA0C50,
];
// Independently checked against the Version 2 disk image and the native provider's ABI.
pub(crate) const ANCHORS: &[(u32, &[u8])] = &[
    (0x04B9D205, &[0x8B, 0x93, 0xE0, 0x01, 0, 0]), // default device +1E0
    (0x04B9839B, &[0x4C, 0x89, 0x77, 0x38]),       // native handle +38
    (
        0x04B97A8B,
        &[0x48, 0xC7, 0x81, 0xB0, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF],
    ),
    (
        0x04CA0B5B,
        &[
            0xC7, 0x46, 0x40, 0, 0, 0, 0, 0x48, 0x89, 0x76, 0x08, 0x48, 0x89, 0x76, 0x10,
        ],
    ),
    (
        0x04CA10B0,
        &[
            0x48, 0x89, 0xD0, 0x45, 0x85, 0xC0, 0x74, 0x0C, 0x44, 0x89, 0x41, 0x40, 0xB9, 0xA8,
            0x21, 0x45, 0x50, 0x89, 0x08, 0xC3,
        ],
    ),
    (
        0x04CA0D45,
        &[
            0xFF, 0x90, 0x08, 0x01, 0, 0, 0x81, 0x7C, 0x24, 0x2C, 0xA8, 0x21, 0x45, 0x50,
        ],
    ),
    (
        0x04BA156E,
        &[
            0x48, 0x8B, 0x80, 0x18, 0x01, 0, 0, 0x44, 0x89, 0x6C, 0x24, 0x30,
        ],
    ),
    (
        0x04CA11A0,
        &[
            0x48, 0xC7, 0x01, 0, 0, 0, 0, 0xC5, 0xF8, 0x57, 0xC0, 0xC5, 0xFC, 0x11, 0x41, 0x18,
        ],
    ),
];

/// The October 21 provider bodies moved together by 0x280. Keep the legacy
/// array above for the prior exact-build admission; the relocation catalog is
/// generated from these independently resolved current addresses.
pub(crate) fn current_anchors() -> Vec<(u32, &'static [u8])> {
    ANCHORS
        .iter()
        .map(|(rva, bytes)| (rva + 0x280, *bytes))
        .collect()
}

#[derive(Clone, Copy)]
struct Section {
    rva: u32,
    raw: u32,
    size: u32,
    flags: u32,
}
fn sections(h: &[u8; 4096], pe: usize, file_len: u64) -> Result<Vec<Section>> {
    let count = u16::from_le_bytes(h[pe + 6..pe + 8].try_into()?) as usize;
    let optional = u16::from_le_bytes(h[pe + 20..pe + 22].try_into()?) as usize;
    let start = pe
        .checked_add(24 + optional)
        .ok_or_else(|| anyhow::anyhow!("Invalid section table"))?;
    ensure!(
        optional >= 112
            && (1..=96).contains(&count)
            && start
                .checked_add(count * 40)
                .is_some_and(|end| end <= h.len()),
        "Invalid provider section table"
    );
    ensure!(
        u64::from_le_bytes(h[pe + 48..pe + 56].try_into()?) == BASE,
        "Unexpected provider preferred image base"
    );
    (0..count)
        .map(|i| {
            let s = &h[start + i * 40..start + (i + 1) * 40];
            let section = Section {
                rva: u32::from_le_bytes(s[12..16].try_into()?),
                size: u32::from_le_bytes(s[16..20].try_into()?),
                raw: u32::from_le_bytes(s[20..24].try_into()?),
                flags: u32::from_le_bytes(s[36..40].try_into()?),
            };
            ensure!(
                u64::from(section.raw) + u64::from(section.size) <= file_len
                    && section.rva.checked_add(section.size).is_some(),
                "Provider section outside image"
            );
            Ok(section)
        })
        .collect()
}
fn locate(sections: &[Section], rva: u32, len: usize, executable: bool) -> Result<u64> {
    let end = rva
        .checked_add(u32::try_from(len)?)
        .ok_or_else(|| anyhow::anyhow!("Provider RVA overflow"))?;
    let matches: Vec<_> = sections
        .iter()
        .filter(|s| rva >= s.rva && end <= s.rva + s.size)
        .collect();
    ensure!(matches.len() == 1, "Provider RVA absent or ambiguous");
    let s = matches[0];
    ensure!(
        s.flags & 0x40000000 != 0
            && s.flags & 0x80000000 == 0
            && (s.flags & 0x20000000 != 0) == executable,
        "Unexpected provider section permissions"
    );
    Ok(u64::from(s.raw) + u64::from(rva - s.rva))
}
fn validate(mut read: impl FnMut(u32, usize, bool) -> Result<Vec<u8>>) -> Result<()> {
    for &(rva, expected) in ANCHORS {
        ensure!(
            read(rva, expected.len(), true)? == expected,
            "Version 2 provider ABI mismatch at {rva:08X}"
        );
    }
    let table = read(TABLE, TARGETS.len() * 8, false)?;
    for (i, target) in TARGETS.iter().enumerate() {
        ensure!(
            u64::from_le_bytes(table[i * 8..i * 8 + 8].try_into()?) == BASE + u64::from(*target),
            "Version 2 provider virtual method mismatch at slot {i}"
        );
        ensure!(
            read(*target, 1, true)?.len() == 1,
            "Missing provider virtual method"
        );
    }
    Ok(())
}
pub(crate) fn verify_v2(file: &mut File, header: &[u8; 4096], pe: usize) -> Result<()> {
    let sections = sections(header, pe, file.metadata()?.len())?;
    validate(|rva, len, executable| {
        file.seek(SeekFrom::Start(locate(&sections, rva, len, executable)?))?;
        let mut bytes = vec![0; len];
        file.read_exact(&mut bytes)?;
        Ok(bytes)
    })
}

#[path = "provider_outfit_profile.rs"]
mod outfit;
pub(crate) fn verify_outfit_carry_v2_launch(
    file: &mut File,
    header: &[u8; 4096],
    pe: usize,
) -> Result<()> {
    outfit::verify_outfit_carry_v2_launch(file, header, pe)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(rva: u32, len: usize, executable: bool) -> Result<Vec<u8>> {
        if rva == TABLE {
            assert!(!executable);
            return Ok(TARGETS
                .iter()
                .flat_map(|r| (BASE + u64::from(*r)).to_le_bytes())
                .collect());
        }
        assert!(executable);
        if let Some((_, b)) = ANCHORS.iter().find(|(r, _)| *r == rva) {
            return Ok(b.to_vec());
        }
        ensure!(TARGETS.contains(&rva) && len == 1, "Unknown fixture RVA");
        Ok(vec![0xC3])
    }
    #[test]
    fn provider_v2_checks_every_abi_anchor_and_all_47_virtual_methods() {
        validate(fixture).unwrap();
        for &(bad, _) in ANCHORS {
            assert!(
                validate(|r, l, x| {
                    let mut b = fixture(r, l, x)?;
                    if r == bad {
                        b[0] ^= 1;
                    }
                    Ok(b)
                })
                .is_err()
            );
        }
        for i in 0..TARGETS.len() {
            assert!(
                validate(|r, l, x| {
                    let mut b = fixture(r, l, x)?;
                    if r == TABLE {
                        b[i * 8] ^= 1;
                    }
                    Ok(b)
                })
                .is_err()
            );
        }
    }
    #[test]
    fn provider_sections_reject_writable_wrong_kind_ambiguous_and_out_of_range() {
        let s = Section {
            rva: 0x1000,
            raw: 0x400,
            size: 0x100,
            flags: 0x60000000,
        };
        assert_eq!(locate(&[s], 0x1000, 1, true).unwrap(), 0x400);
        assert!(locate(&[s], 0x1000, 1, false).is_err());
        assert!(
            locate(
                &[Section {
                    flags: 0xE0000000,
                    ..s
                }],
                0x1000,
                1,
                true
            )
            .is_err()
        );
        assert!(locate(&[s, s], 0x1000, 1, true).is_err());
        assert!(locate(&[s], 0x1100, 1, true).is_err());
        assert!(locate(&[s], u32::MAX, 2, true).is_err());
        let mut h = [0; 4096];
        let pe = 0x80;
        h[pe + 6..pe + 8].copy_from_slice(&1u16.to_le_bytes());
        h[pe + 20..pe + 22].copy_from_slice(&240u16.to_le_bytes());
        h[pe + 48..pe + 56].copy_from_slice(&BASE.to_le_bytes());
        assert_eq!(sections(&h, pe, 4096).unwrap().len(), 1);
        h[pe + 48] ^= 1;
        assert!(sections(&h, pe, 4096).is_err());
        h[pe + 48] ^= 1;
        h[pe + 6..pe + 8].copy_from_slice(&97u16.to_le_bytes());
        assert!(sections(&h, pe, 4096).is_err());
    }
    #[test]
    #[ignore = "opt-in disk-only audit; never opens a game process"]
    fn provider_v2_actual_disk_gate() {
        let path =
            std::env::var_os("C27_PROVIDER_DISK_AUDIT").expect("explicit disk image path required");
        let verified = crate::provider_access::verify_build(std::path::Path::new(&path)).unwrap();
        assert_eq!(verified.build["label"], "Version 2");
        assert_eq!(verified.build["modProviderMapped"], true);
        assert_eq!(verified.build["nativeWritesMapped"], false);
        assert_eq!(verified.build["version2OutfitCarryMapped"], true);
        assert!(verified.build["version2OutfitCarryError"].is_null());
        drop(verified);
        // Both launch admissions skip the maintenance-only complete-file audit.
        let started = std::time::Instant::now();
        let again = crate::provider_access::verify_build(std::path::Path::new(&path)).unwrap();
        assert_eq!(again.build["version2OutfitCarryMapped"], true);
        eprintln!("repeat launch verification: {:?}", started.elapsed());
    }
}
