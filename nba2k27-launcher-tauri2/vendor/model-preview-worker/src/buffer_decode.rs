//! Standalone buffers are not ZIP archives. Use authored lengths or a verified
//! VCZ gzip-style size/CRC trailer; never infer raw length from zero padding.
use crate::codec::{self, Oodle};
use anyhow::{Context, Result, ensure};
use std::{io::Read, sync::atomic::AtomicBool};

#[derive(Debug)]
pub(crate) struct TrailerChecksumMismatch;
impl std::fmt::Display for TrailerChecksumMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VCZ trailer checksum mismatch")
    }
}
impl std::error::Error for TrailerChecksumMismatch {}

fn trailer(bytes: &[u8]) -> Result<(usize, u32)> {
    ensure!(
        bytes.len() > 24 && codec::vcz(bytes),
        "Missing VCZ wrapper/trailer"
    );
    ensure!(
        bytes[..4] == [0x1f, 0x8b, 0x21, 4] && bytes[10..12] == [4, 0],
        "Unsupported standalone VCZ wrapper; owning-IFF size metadata required"
    );
    let end = bytes.len();
    let size = codec::u32_at(bytes, end - 4)? as usize;
    ensure!(
        size > 0 && size <= codec::MAX,
        "Invalid standalone VCZ trailer size"
    );
    Ok((size, codec::u32_at(bytes, end - 8)?))
}

fn checked_crc(bytes: Vec<u8>, size: usize, crc: u32) -> Result<Vec<u8>> {
    ensure!(bytes.len() == size, "VCZ trailer decoded-size mismatch");
    if crc32fast::hash(&bytes) != crc {
        return Err(TrailerChecksumMismatch.into());
    }
    Ok(bytes)
}

pub fn decode(
    bytes: &[u8],
    expected: Option<usize>,
    oodle: &Oodle,
    cancel: &AtomicBool,
) -> Result<Vec<u8>> {
    codec::cancelled(cancel)?;
    ensure!(bytes.len() <= codec::MAX, "Packed buffer exceeds 512 MiB");
    let result = if codec::vcz(bytes) {
        if let Some(size) = expected {
            ensure!(
                size > 0 && size <= codec::MAX,
                "Invalid authored buffer size"
            );
            oodle.buffer(bytes, size)?
        } else {
            let (size, crc) = trailer(bytes)?;
            // Exclude the uncompressed wrapper trailer from the Oodle stream.
            let (output, count) = oodle.decode(&bytes[16..bytes.len() - 8], size)?;
            ensure!(
                count == size as i64,
                "Incomplete standalone VCZ decode: {count}/{size}"
            );
            checked_crc(output, size, crc)?
        }
    } else if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut output = Vec::new();
        // MultiGzDecoder validates member CRCs and rejects non-gzip tails.
        flate2::read::MultiGzDecoder::new(bytes)
            .take(codec::MAX as u64 + 1)
            .read_to_end(&mut output)
            .context("Invalid or incomplete gzip buffer")?;
        ensure!(output.len() <= codec::MAX, "Gzip buffer exceeds 512 MiB");
        if let Some(size) = expected {
            ensure!(output.len() == size, "Gzip authored-size mismatch");
        }
        output
    } else {
        anyhow::bail!("Not a VCZ or gzip buffer")
    };
    codec::cancelled(cancel)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    // Read-only installed-game acceptance; not a dependency of ordinary tests.
    #[test]
    #[ignore = "requires NBA2K27_TEST_GAME_ROOT and installed game-owned Oodle DLL"]
    fn installed_standalone_buffers_match_exact_authored_length_decoder() {
        let root = std::env::var("NBA2K27_TEST_GAME_ROOT").unwrap();
        let catalog = crate::catalog::Catalog::load(std::path::Path::new(&root)).unwrap();
        let oodle = Oodle::load(&catalog.root).unwrap();
        let cancel = AtomicBool::new(false);
        let entries: Vec<_> = catalog
            .entries
            .iter()
            .filter(|e| e.name.ends_with(".gz"))
            .take(4)
            .collect();
        assert_eq!(entries.len(), 4);
        for entry in entries {
            let packed = codec::range(&catalog, entry).unwrap();
            let (size, _) = trailer(&packed).unwrap();
            let actual = decode(&packed, None, &oodle, &cancel).unwrap();
            let expected = oodle.buffer(&packed, size).unwrap();
            assert_eq!(actual, expected, "{}", entry.name);
            assert_eq!(codec::range(&catalog, entry).unwrap(), packed);
            println!(
                "Verified {}: {} bytes, exact size and CRC",
                entry.name,
                actual.len()
            );
        }
    }
    #[test]
    fn trailer_does_not_accept_unknown_wrappers_or_unbounded_sizes() {
        let mut b = vec![0; 32];
        b[12..16].copy_from_slice(b"VCZ\0");
        assert!(trailer(&b).is_err());
        b[..4].copy_from_slice(&[0x1f, 0x8b, 0x21, 4]);
        b[10] = 4;
        b[28..].copy_from_slice(&17u32.to_le_bytes());
        assert_eq!(trailer(&b).unwrap().0, 17);
        b[28..].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(trailer(&b).is_err());
    }
    #[test]
    fn exact_size_and_checksum_are_both_required() {
        let bytes = b"unchanged raw bytes".to_vec();
        let crc = crc32fast::hash(&bytes);
        assert_eq!(checked_crc(bytes.clone(), bytes.len(), crc).unwrap(), bytes);
        assert!(checked_crc(bytes.clone(), bytes.len() - 1, crc).is_err());
        assert!(checked_crc(bytes.clone(), bytes.len(), crc ^ 1).is_err());
    }
}
