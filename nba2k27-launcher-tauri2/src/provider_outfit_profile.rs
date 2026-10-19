//! Independent disk-only admission for the manual Version 2 outfit worker.
//! This proof is not the legacy nativeWritesMapped gate, and does not authorize
//! automatic randomization or any unrelated memory writer.
use super::*;
use anyhow::Context;
use serde::Deserialize;
use sha2::{Digest, Sha256};

const GAME_SHA: &str = "29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1";
const GAME_BYTES: u64 = 870548760;
const DLL_SHA: &str = "2E921A4E8B408996C20DEAD7412835A9197D67B83C05CE840130300A903120DD";
const DLL: &[u8] = include_bytes!("../assets/native/version-outfits-v2-qa084.dll");
const EXTRA_METHODS: &[(u32, usize, &str)] = &[
    (
        0x6FD8C0,
        407,
        "D6CE042EEFA908FBFE444AB6823486A91910A8E3BF9162CE347C4BCAFA72FD43",
    ),
    (
        0x72A390,
        374,
        "98B4448E448857EA1D91695AB206648C7482510030DE0483716ECA821D438056",
    ),
    (
        0x4B75FA0,
        197,
        "4C0597E56C15618F95ED43722FF76EAAEBFBFC3C61584331F56DD204462D36D6",
    ),
    (
        0x4B76210,
        16,
        "1CD5757FEA907C28D91E7AC693D732A0ED987DD2512B59996D19FF26E32726AB",
    ),
];
#[derive(Deserialize)]
struct Method {
    rva: u32,
    bytes: usize,
    sha256: String,
    hex: String,
}
#[derive(Deserialize)]
struct Slot {
    offset: u32,
    method: u32,
}
#[derive(Deserialize)]
struct Table {
    rva: u32,
    slots: Vec<Slot>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Profile {
    schema: String,
    sha256: String,
    timestamp: String,
    image_bytes: String,
    methods: Vec<Method>,
    virtual_tables: Vec<Table>,
}
fn hash(data: &[u8]) -> String {
    format!("{:X}", Sha256::digest(data))
}
fn profile() -> Result<Profile> {
    let p: Profile = serde_json::from_str(include_str!("../assets/outfit-match-v2-profile.json"))?;
    ensure!(
        p.schema == "nba2k27-v2-outfit-match-method-profile-v1"
            && p.sha256 == GAME_SHA
            && p.timestamp == "6AC53765"
            && p.image_bytes == "35BDE000"
            && p.methods.len() == 25
            && p.virtual_tables.len() == 10
            && p.virtual_tables
                .iter()
                .map(|t| t.slots.len())
                .sum::<usize>()
                == 32,
        "Version 2 outfit proof inventory differs"
    );
    for m in &p.methods {
        ensure!(
            (1..=65536).contains(&m.bytes) && m.hex.len() == m.bytes * 2,
            "Invalid outfit method proof bounds"
        );
        let bytes = (0..m.hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&m.hex[i..i + 2], 16))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ensure!(
            hash(&bytes) == m.sha256,
            "Outfit method proof body/hash differs"
        );
    }
    Ok(p)
}
fn validate_methods(
    mut method_hash: impl FnMut(u32, usize) -> Result<String>,
    mut virtual_value: impl FnMut(u32) -> Result<u64>,
) -> Result<()> {
    let p = profile()?;
    for m in &p.methods {
        ensure!(
            method_hash(m.rva, m.bytes)? == m.sha256,
            "Version 2 outfit method mismatch at {:08X}",
            m.rva
        );
    }
    for &(rva, len, sha) in EXTRA_METHODS {
        ensure!(
            method_hash(rva, len)? == sha,
            "Version 2 outfit worker method mismatch at {rva:08X}"
        );
    }
    for t in &p.virtual_tables {
        for s in &t.slots {
            ensure!(
                virtual_value(
                    t.rva
                        .checked_add(s.offset)
                        .context("Outfit virtual slot overflow")?
                )? == BASE + u64::from(s.method),
                "Version 2 outfit virtual method mismatch"
            );
        }
    }
    ensure!(
        virtual_value(0x6884898 + 0xA0)? == BASE + 0x6FD8C0,
        "Version 2 outfit clone virtual mismatch"
    );
    Ok(())
}
fn header(h: &[u8; 4096], pe: usize, len: u64) -> Result<()> {
    ensure!(
        pe >= 0x40 && pe.checked_add(0x108).is_some_and(|n| n <= h.len()),
        "Invalid outfit PE header bounds"
    );
    ensure!(
        &h[..2] == b"MZ"
            && &h[pe..pe + 4] == b"PE\0\0"
            && u16::from_le_bytes(h[pe + 4..pe + 6].try_into()?) == 0x8664
            && u16::from_le_bytes(h[pe + 24..pe + 26].try_into()?) == 0x20b
            && u32::from_le_bytes(h[pe + 8..pe + 12].try_into()?) == 0x6AC53765
            && u32::from_le_bytes(h[pe + 80..pe + 84].try_into()?) == 0x35BDE000
            && u64::from_le_bytes(h[pe + 48..pe + 56].try_into()?) == BASE
            && len == GAME_BYTES,
        "Manual outfit support is not verified for this complete game build"
    );
    Ok(())
}
#[cfg(test)]
pub(crate) fn verify_outfit_carry_v2(file: &mut File, h: &[u8; 4096], pe: usize) -> Result<()> {
    let before = file.metadata()?;
    header(h, pe, before.len())?;
    full_hash(file)?;
    verify(file, h, pe)?;
    let after = file.metadata()?;
    ensure!(
        before.len() == after.len() && before.modified()? == after.modified()?,
        "Game image changed during build-update audit"
    );
    Ok(())
}
/// Normal launch checks the mapped build tuple, payload, every native method
/// and virtual slot. The complete 870 MB audit belongs to explicit build-update
/// maintenance, never launcher startup, Play, or provider binding.
pub(crate) fn verify_outfit_carry_v2_launch(
    file: &mut File,
    h: &[u8; 4096],
    pe: usize,
) -> Result<()> {
    verify(file, h, pe)
}
fn verify(file: &mut File, h: &[u8; 4096], pe: usize) -> Result<()> {
    let before = file.metadata()?;
    header(h, pe, before.len())?;
    ensure!(
        hash(DLL) == DLL_SHA && crate::outfit_worker_v2_loader::PROXY_SHA == DLL_SHA,
        "Manual outfit native payload differs from its independent build proof"
    );
    let sections = sections(h, pe, before.len())?;
    validate_image(file, &sections, &before)
}
#[cfg(test)]
fn full_hash(file: &mut File) -> Result<()> {
    file.seek(SeekFrom::Start(0))?;
    let mut sha = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    let mut count = 0u64;
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        sha.update(&buffer[..n]);
        count += n as u64;
    }
    ensure!(
        count == GAME_BYTES && format!("{:X}", sha.finalize()) == GAME_SHA,
        "Full Version 2 outfit game hash differs"
    );
    Ok(())
}
fn validate_image(file: &mut File, sections: &[Section], before: &std::fs::Metadata) -> Result<()> {
    // Two closures share one retained handle, not a path reopened between proofs.
    let file = std::cell::RefCell::new(file);
    validate_methods(
        |rva, len| {
            let mut f = file.borrow_mut();
            f.seek(SeekFrom::Start(locate(sections, rva, len, true)?))?;
            let mut b = vec![0; len];
            f.read_exact(&mut b)?;
            Ok(hash(&b))
        },
        |rva| {
            let mut f = file.borrow_mut();
            f.seek(SeekFrom::Start(locate(sections, rva, 8, false)?))?;
            let mut b = [0; 8];
            f.read_exact(&mut b)?;
            let target = u64::from_le_bytes(b);
            let offset = target
                .checked_sub(BASE)
                .context("Outfit method pointer below image")?;
            locate(sections, u32::try_from(offset)?, 1, true)?;
            Ok(target)
        },
    )?;
    let after = file.borrow().metadata()?;
    ensure!(
        before.len() == after.len() && before.modified()? == after.modified()?,
        "Game image changed during outfit proof"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn expected_method(r: u32, n: usize) -> Result<String> {
        let p = profile()?;
        if let Some(m) = p.methods.iter().find(|m| m.rva == r && m.bytes == n) {
            return Ok(m.sha256.clone());
        }
        Ok(EXTRA_METHODS
            .iter()
            .find(|(a, b, _)| *a == r && *b == n)
            .context("Unknown test method")?
            .2
            .into())
    }
    fn expected_virtual(r: u32) -> Result<u64> {
        if r == 0x6884898 + 0xA0 {
            return Ok(BASE + 0x6FD8C0);
        }
        for t in profile()?.virtual_tables {
            for s in t.slots {
                if t.rva + s.offset == r {
                    return Ok(BASE + u64::from(s.method));
                }
            }
        }
        anyhow::bail!("Unknown test virtual")
    }
    #[test]
    fn all_29_methods_and_33_virtual_slots_are_independent_mandatory_proofs() {
        validate_methods(expected_method, expected_virtual).unwrap();
        let mut methods = profile()
            .unwrap()
            .methods
            .into_iter()
            .map(|m| (m.rva, m.bytes))
            .collect::<Vec<_>>();
        methods.extend(EXTRA_METHODS.iter().map(|(r, n, _)| (*r, *n)));
        assert_eq!(methods.len(), 29);
        for (bad, _) in methods {
            assert!(
                validate_methods(
                    |r, n| if r == bad {
                        Ok("0".repeat(64))
                    } else {
                        expected_method(r, n)
                    },
                    expected_virtual
                )
                .is_err()
            );
        }
        let mut slots = profile()
            .unwrap()
            .virtual_tables
            .into_iter()
            .flat_map(|t| t.slots.into_iter().map(move |s| t.rva + s.offset))
            .collect::<Vec<_>>();
        slots.push(0x6884898 + 0xA0);
        assert_eq!(slots.len(), 33);
        for bad in slots {
            assert!(
                validate_methods(expected_method, |r| Ok(
                    expected_virtual(r)? ^ if r == bad { 1 } else { 0 }
                ))
                .is_err()
            );
        }
    }
    #[test]
    fn complete_build_header_and_new_dll_are_required() {
        let pe = 0x80;
        let mut h = [0; 4096];
        h[..2].copy_from_slice(b"MZ");
        h[pe..pe + 4].copy_from_slice(b"PE\0\0");
        h[pe + 4..pe + 6].copy_from_slice(&0x8664u16.to_le_bytes());
        h[pe + 24..pe + 26].copy_from_slice(&0x20bu16.to_le_bytes());
        h[pe + 8..pe + 12].copy_from_slice(&0x6AC53765u32.to_le_bytes());
        h[pe + 80..pe + 84].copy_from_slice(&0x35BDE000u32.to_le_bytes());
        h[pe + 48..pe + 56].copy_from_slice(&BASE.to_le_bytes());
        header(&h, pe, GAME_BYTES).unwrap();
        for at in [0, pe, pe + 4, pe + 8, pe + 24, pe + 48, pe + 80] {
            h[at] ^= 1;
            assert!(header(&h, pe, GAME_BYTES).is_err());
            h[at] ^= 1;
        }
        assert!(header(&h, usize::MAX, GAME_BYTES).is_err());
        assert!(header(&h, pe, GAME_BYTES - 1).is_err());
        assert_eq!(hash(DLL), DLL_SHA);
        assert_eq!(crate::outfit_worker_v2_loader::PROXY_SHA, DLL_SHA);
    }
    #[test]
    fn launch_admission_still_rejects_an_invalid_image_without_a_full_audit() {
        let path = std::env::temp_dir().join(format!(
            "outfit-proof-cache-{}",
            crate::backend::unique_id()
        ));
        std::fs::write(&path, vec![0u8; 4096]).unwrap();
        let mut f = File::open(&path).unwrap();
        let mut h = [0; 4096];
        f.read_exact(&mut h).unwrap();
        assert!(verify_outfit_carry_v2_launch(&mut f, &h, 0x80).is_err());
        drop(f);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    #[ignore = "opt-in retained disk-only audit, no game process APIs"]
    fn exact_installed_disk_build() {
        let path = std::env::var_os("C27_OUTFIT_DISK_AUDIT").expect("Explicit disk path required");
        let mut f = File::open(path).unwrap();
        let mut h = [0; 4096];
        f.read_exact(&mut h).unwrap();
        let pe = u32::from_le_bytes(h[0x3c..0x40].try_into().unwrap()) as usize;
        verify_outfit_carry_v2(&mut f, &h, pe).unwrap();
    }
}
