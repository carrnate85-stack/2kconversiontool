//! Exact Version 2 stock Freestyle roster-loader mapping.
//!
//! NBA 2K27 Version 2 split the 2KU/Freestyle roster loaders into sibling
//! `TKU_DELEGATE` and `TKU_WNBA_DELEGATE` virtual entries. Both entries occupy
//! slot `+0xD30` and tail-call the same native loader with different stock
//! roster assets. The mapping is retained for read-only research only. Runtime
//! swapping is withdrawn because sibling identity does not prove that invoking
//! one thunk through the other delegate is safe.
use crate::blacktop_native::Memory;
use anyhow::{Context, Result, ensure};

/// The sibling thunk and its addresses are useful for read-only Version 2
/// identification, but invoking it through the NBA Freestyle delegate is not a
/// proven stock call path. A live first-entry crash occurred while this route
/// was armed. Keep the saved launcher preference intact, but fail closed before
/// opening a writer until the callback ABI, roster lifetime and invocation
/// boundary are independently proven.
pub(crate) const RUNTIME_ENABLED: bool = false;
pub(crate) const RUNTIME_BLOCKER: &str = "WNBA Freestyle is unavailable in this QA build: the Version 2 sibling roster-loader route was withdrawn after a Freestyle entry crash. No WNBA memory write will be attempted.";

pub(crate) fn require_runtime_enabled() -> Result<()> {
    ensure!(RUNTIME_ENABLED, RUNTIME_BLOCKER);
    Ok(())
}

pub(crate) const TKU_VTABLE: u64 = 0x74DB678;
pub(crate) const TKU_SLOT: u64 = TKU_VTABLE + 0xD30;
pub(crate) const TKU_OBJECT: u64 = 0x9B31E80;
pub(crate) const TKU_WNBA_VTABLE: u64 = 0x74DDE38;
pub(crate) const TKU_WNBA_SLOT: u64 = TKU_WNBA_VTABLE + 0xD30;
pub(crate) const TKU_WNBA_OBJECT: u64 = 0x9B31F00;
pub(crate) const GENERAL_LOADER: u64 = 0x236FB30;
pub(crate) const WNBA_LOADER: u64 = 0x236FF70;
pub(crate) const COMMON_LOADER: u64 = 0x2F1E0B0;

const GENERAL_BYTES: &[u8] = &[
    0x48, 0x8D, 0x0D, 0x63, 0x0C, 0x17, 0x05, 0xB2, 0x01, 0x41, 0xB8, 0xD2, 0xB8, 0x9C, 0x50, 0x41,
    0xB9, 0x2F, 0, 0, 0, 0xE9, 0x66, 0xE5, 0xBA, 0,
];
const WNBA_BYTES: &[u8] = &[
    0x48, 0x8D, 0x0D, 0x3D, 0x08, 0x17, 0x05, 0xB2, 0x01, 0x41, 0xB8, 0x19, 0x15, 0xAA, 0xA1, 0x41,
    0xB9, 0x1A, 0, 0, 0, 0xE9, 0x26, 0xE1, 0xBA, 0,
];
const COMMON_PREFIX: &[u8] = &[
    0x55, 0x56, 0x48, 0x81, 0xEC, 0x18, 0x01, 0, 0, 0x48, 0x8D, 0xAC, 0x24, 0x80, 0, 0, 0,
];

fn address(m: &impl Memory, rva: u64) -> Result<u64> {
    m.base()
        .checked_add(rva)
        .context("WNBA loader RVA overflow")
}
fn read_image(m: &impl Memory, rva: u64, len: usize) -> Result<Vec<u8>> {
    ensure!((1..=4096).contains(&len), "WNBA loader read bound rejected");
    m.checkpoint()?;
    m.read(address(m, rva)?, len)
}
fn q(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes[..8].try_into().unwrap())
}

/// Revalidate every native object and function involved in the one-pointer
/// transaction. The slot itself may contain either stock endpoint so the same
/// proof is valid during conditional restoration.
pub(crate) fn validate_profile(m: &impl Memory) -> Result<()> {
    let header = read_image(m, 0, 4096)?;
    ensure!(&header[..2] == b"MZ", "Invalid WNBA loader image header");
    let pe = u32::from_le_bytes(header[60..64].try_into()?) as usize;
    ensure!(
        (64..=3800).contains(&pe)
            && &header[pe..pe + 4] == b"PE\0\0"
            && u16::from_le_bytes(header[pe + 4..pe + 6].try_into()?) == 0x8664
            && u16::from_le_bytes(header[pe + 24..pe + 26].try_into()?) == 0x20B
            && u32::from_le_bytes(header[pe + 8..pe + 12].try_into()?) == 0x6AC53765
            && u32::from_le_bytes(header[pe + 80..pe + 84].try_into()?) == 0x35BDE000,
        "WNBA roster loader requires the verified Version 2 executable"
    );
    ensure!(
        read_image(m, GENERAL_LOADER, GENERAL_BYTES.len())? == GENERAL_BYTES
            && read_image(m, WNBA_LOADER, WNBA_BYTES.len())? == WNBA_BYTES
            && read_image(m, COMMON_LOADER, COMMON_PREFIX.len())? == COMMON_PREFIX,
        "Stock Freestyle roster-loader code differs"
    );
    ensure!(
        q(&read_image(m, TKU_OBJECT, 8)?) == address(m, TKU_VTABLE)?
            && q(&read_image(m, TKU_WNBA_OBJECT, 8)?) == address(m, TKU_WNBA_VTABLE)?
            && q(&read_image(m, TKU_WNBA_SLOT, 8)?) == address(m, WNBA_LOADER)?,
        "Stock Freestyle roster-loader delegate identity differs"
    );
    ensure!(
        matches!(
            q(&read_image(m, TKU_SLOT, 8)?).checked_sub(m.base()),
            Some(GENERAL_LOADER | WNBA_LOADER)
        ),
        "Freestyle roster-loader slot is not a stock endpoint"
    );
    m.checkpoint()
}

pub(crate) fn current(m: &impl Memory) -> Result<u64> {
    validate_profile(m)?;
    q(&read_image(m, TKU_SLOT, 8)?)
        .checked_sub(m.base())
        .context("Freestyle roster-loader pointer is outside the game image")
}

pub(crate) fn permitted(before: u64, after: u64) -> bool {
    matches!(
        (before, after),
        (GENERAL_LOADER, WNBA_LOADER) | (WNBA_LOADER, GENERAL_LOADER)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    const BASE: u64 = 0x180000000;
    struct Fake(BTreeMap<u64, Vec<u8>>);
    impl Memory for Fake {
        fn base(&self) -> u64 {
            BASE
        }
        fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
            for (start, bytes) in self.0.range(..=at).rev() {
                let offset = usize::try_from(at - *start)?;
                if offset
                    .checked_add(len)
                    .is_some_and(|end| end <= bytes.len())
                {
                    return Ok(bytes[offset..offset + len].to_vec());
                }
            }
            anyhow::bail!("Unmapped fake loader read")
        }
    }
    fn fixture() -> Fake {
        let mut rows = BTreeMap::new();
        let mut header = vec![0; 4096];
        header[..2].copy_from_slice(b"MZ");
        header[60..64].copy_from_slice(&128u32.to_le_bytes());
        header[128..132].copy_from_slice(b"PE\0\0");
        header[132..134].copy_from_slice(&0x8664u16.to_le_bytes());
        header[136..140].copy_from_slice(&0x6AC53765u32.to_le_bytes());
        header[152..154].copy_from_slice(&0x20Bu16.to_le_bytes());
        header[208..212].copy_from_slice(&0x35BDE000u32.to_le_bytes());
        rows.insert(BASE, header);
        rows.insert(BASE + GENERAL_LOADER, GENERAL_BYTES.to_vec());
        rows.insert(BASE + WNBA_LOADER, WNBA_BYTES.to_vec());
        rows.insert(BASE + COMMON_LOADER, COMMON_PREFIX.to_vec());
        rows.insert(
            BASE + TKU_OBJECT,
            (BASE + TKU_VTABLE).to_le_bytes().to_vec(),
        );
        rows.insert(
            BASE + TKU_WNBA_OBJECT,
            (BASE + TKU_WNBA_VTABLE).to_le_bytes().to_vec(),
        );
        rows.insert(
            BASE + TKU_SLOT,
            (BASE + GENERAL_LOADER).to_le_bytes().to_vec(),
        );
        rows.insert(
            BASE + TKU_WNBA_SLOT,
            (BASE + WNBA_LOADER).to_le_bytes().to_vec(),
        );
        Fake(rows)
    }
    #[test]
    fn exact_v2_sibling_loader_profile_accepts_only_the_two_stock_endpoints() {
        let mut f = fixture();
        validate_profile(&f).unwrap();
        assert_eq!(current(&f).unwrap(), GENERAL_LOADER);
        f.0.insert(BASE + TKU_SLOT, (BASE + WNBA_LOADER).to_le_bytes().to_vec());
        validate_profile(&f).unwrap();
        assert_eq!(current(&f).unwrap(), WNBA_LOADER);
        f.0.insert(BASE + TKU_SLOT, (BASE + 0x1234).to_le_bytes().to_vec());
        assert!(validate_profile(&f).is_err());
    }
    #[test]
    fn transition_allowlist_is_bidirectional_but_never_accepts_ui_addresses() {
        assert!(permitted(GENERAL_LOADER, WNBA_LOADER));
        assert!(permitted(WNBA_LOADER, GENERAL_LOADER));
        for value in [0, TKU_SLOT, COMMON_LOADER, u64::MAX] {
            assert!(!permitted(GENERAL_LOADER, value));
            assert!(!permitted(value, WNBA_LOADER));
        }
    }
    #[test]
    fn withdrawn_runtime_gate_rejects_before_any_loader_transaction() {
        let error = require_runtime_enabled().unwrap_err().to_string();
        assert!(error.contains("entry crash"));
        assert!(error.contains("No WNBA memory write"));
    }
}
