//! Complete concatenated VCZ streams used by large arenas. No invented ZIP tails.
use crate::codec::{self, Oodle};
use anyhow::{Context, Result, ensure};
use std::{path::Path, sync::atomic::AtomicBool};
type Step = unsafe extern "C" fn(*const u8, i64, i64, i64, *mut i64, *mut i32) -> i64;
pub fn strict(oodle: &Oodle, root: &Path, bytes: &[u8], cancel: &AtomicBool) -> Result<Vec<u8>> {
    oodle.archive(bytes, cancel).or_else(|original| {
        recover(oodle, root, bytes, cancel).context(format!("Primary decode: {original:#}"))
    })
}
pub fn preview(oodle: &Oodle, root: &Path, bytes: &[u8], cancel: &AtomicBool) -> Result<Vec<u8>> {
    oodle.archive_preview(bytes, cancel).or_else(|original| {
        recover(oodle, root, bytes, cancel).context(format!("Primary preview decode: {original:#}"))
    })
}
fn checked_exact(bytes: Vec<u8>, cancel: &AtomicBool) -> Result<Vec<u8>> {
    // The authored trailer describes this ENTIRE frame. Do not crop padding,
    // invent an end record or select a name-map subset of physical entries.
    let mut archive = crate::container::Archive::open_with_cancel(bytes, cancel)?;
    archive.validate_with(cancel, |_| {})?;
    Ok(archive.into_bytes())
}
fn recover(oodle: &Oodle, root: &Path, bytes: &[u8], cancel: &AtomicBool) -> Result<Vec<u8>> {
    codec::cancelled(cancel)?;
    match crate::buffer_decode::decode(bytes, None, oodle, cancel) {
        Ok(raw) => {
            checked_exact(raw, cancel).context("Trailer-verified frame is not a complete valid IFF")
        }
        // A successful exact-length decode with a bad authored CRC must not be
        // retried through a path that only validates ZIP member CRCs.
        Err(error) if error.is::<crate::buffer_decode::TrailerChecksumMismatch>() => Err(error),
        Err(trailer) => {
            codec::cancelled(cancel)?;
            concatenated(oodle, root, bytes, cancel)
                .context(format!("Exact trailer recovery unavailable: {trailer:#}"))
        }
    }
}
fn exact_prefix(oodle: &Oodle, input: &[u8], upper: usize, cancel: &AtomicBool) -> Result<usize> {
    let (mut low, mut high, mut best) = (1, upper, 0);
    while low <= high {
        codec::cancelled(cancel)?;
        let n = low + (high - low) / 2;
        let (_, count) = oodle.decode(input, n)?;
        if count == n as i64 {
            best = n;
            low = n + 1;
        } else {
            high = n - 1;
        }
    }
    ensure!(best > 0, "No complete native VCZ prefix");
    Ok(best)
}
fn checked(mut bytes: Vec<u8>, cancel: &AtomicBool) -> Result<Vec<u8>> {
    let n = crate::container::boundary(&bytes)?;
    ensure!(n > 0 && n <= bytes.len(), "Incomplete concatenated IFF");
    // Only validated ZIP length. Bytes after EOCD may be authored safe-read padding.
    bytes.truncate(n);
    // Borrow while checking CRCs: avoid a second full-container allocation.
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes.as_slice()))?;
    crate::container::validate_zip(&mut archive, cancel, |_| {})?;
    drop(archive);
    Ok(bytes)
}
fn concatenated(oodle: &Oodle, root: &Path, bytes: &[u8], cancel: &AtomicBool) -> Result<Vec<u8>> {
    codec::cancelled(cancel)?;
    ensure!(
        codec::vcz(bytes) && bytes.len() <= codec::MAX,
        "Not a bounded VCZ resource"
    );
    let input = &bytes[16..];
    let path = root.join("data/oodle/oo2core_9_win64.dll").canonicalize()?;
    let library = unsafe { libloading::Library::new(path) }?;
    let step = *unsafe { library.get::<Step>(b"OodleLZ_GetCompressedStepForRawStep\0") }?;
    let (mut cursor, mut total) = (0usize, 0usize);
    let mut chunks = Vec::new();
    while cursor < input.len() {
        codec::cancelled(cancel)?;
        let remaining = &input[cursor..];
        if remaining.len() <= 16 && !chunks.is_empty() {
            break;
        }
        let (mut raw, mut independent) = (0, 0);
        let mut used = unsafe {
            step(
                remaining.as_ptr(),
                remaining.len() as i64,
                0,
                1 << 26,
                &mut raw,
                &mut independent,
            )
        };
        if used <= 0 || raw <= 0 {
            let (mut low, mut high, mut best_raw, mut best_used) = (1i64, (1 << 26) - 1, 0, 0);
            while low <= high {
                codec::cancelled(cancel)?;
                let n = low + (high - low) / 2;
                let (mut end, mut independent) = (0, 0);
                let consumed = unsafe {
                    step(
                        remaining.as_ptr(),
                        remaining.len() as i64,
                        0,
                        n,
                        &mut end,
                        &mut independent,
                    )
                };
                if consumed > 0 && end > 0 {
                    best_raw = end;
                    best_used = consumed;
                    low = n + 1;
                } else {
                    high = n - 1;
                }
            }
            raw = best_raw;
            used = best_used;
            if raw <= 0 || used <= 0 {
                raw = exact_prefix(oodle, remaining, 1 << 18, cancel)? as i64;
                used = remaining.len() as i64;
            }
        }
        ensure!(
            used > 0 && used <= remaining.len() as i64 && raw > 0 && raw <= 1 << 26,
            "Invalid concatenated VCZ chunk at {cursor}"
        );
        total = total
            .checked_add(raw as usize)
            .context("VCZ size overflow")?;
        ensure!(
            total <= crate::container::MAX,
            "Concatenated VCZ exceeds 1 GiB"
        );
        chunks.push((cursor, raw as usize));
        cursor += used as usize;
    }
    ensure!(
        !chunks.is_empty() && input.len() - cursor <= 16,
        "VCZ chunks did not consume source"
    );
    let (mut combined, count) = decode_container(oodle, &library, input, total, cancel)?;
    if count == total as i64 {
        return checked(combined, cancel);
    }
    if count > 0 && count <= total as i64 {
        combined.truncate(count as usize);
    }
    // A rounded final Oodle block can return zero after populating the entire
    // authored ZIP. Accept only the walked original ZIP boundary plus every
    // member's exact size/CRC, never an inferred or synthesized end record.
    let validation_error = match checked(combined, cancel) {
        Ok(raw) => return Ok(raw),
        Err(error) => format!("Combined VCZ decoded {count}/{total}: {error:#}"),
    };
    if chunks.len() == 1 {
        let n = exact_prefix(oodle, input, total.saturating_sub(1), cancel)?;
        let (raw, count) = oodle.decode(input, n)?;
        ensure!(count == n as i64, "Incomplete final VCZ block");
        return checked(raw, cancel);
    }
    let mut combined = Vec::with_capacity(total);
    for (offset, size) in chunks {
        codec::cancelled(cancel)?;
        let (raw, count) = oodle.decode(&input[offset..], size)?;
        ensure!(
            count == size as i64,
            "Incomplete concatenated chunk ({offset}: {count}/{size}); {validation_error}"
        );
        combined.extend(raw);
    }
    checked(combined, cancel)
}
type Decode = unsafe extern "C" fn(
    *const u8,
    i64,
    *mut u8,
    i64,
    i32,
    i32,
    i32,
    *mut u8,
    i64,
    *mut u8,
    *mut u8,
    *mut u8,
    i64,
    i32,
) -> i64;
fn decode_container(
    oodle: &Oodle,
    library: &libloading::Library,
    input: &[u8],
    n: usize,
    cancel: &AtomicBool,
) -> Result<(Vec<u8>, i64)> {
    codec::cancelled(cancel)?;
    if n <= codec::MAX {
        return oodle.decode(input, n);
    }
    ensure!(
        n <= crate::container::MAX,
        "Decoded container exceeds 1 GiB"
    );
    let decode = unsafe { library.get::<Decode>(b"OodleLZ_Decompress\0") }?;
    let mut out = Vec::new();
    out.try_reserve_exact(n)
        .context("Cannot allocate bounded decoded container")?;
    out.resize(n, 0);
    let count = unsafe {
        decode(
            input.as_ptr(),
            input.len() as i64,
            out.as_mut_ptr(),
            n as i64,
            1,
            0,
            0,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
            3,
        )
    };
    codec::cancelled(cancel)?;
    Ok((out, count))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concat_never_invents_a_zip_footer() {
        assert!(checked(b"not a zip".to_vec(), &AtomicBool::new(false)).is_err());
    }
    #[test]
    fn exact_frame_requires_every_physical_crc_and_complete_original_directory() {
        let source = crate::iff_directory::tests::fixture(&[
            ("same.txt", b"first"),
            ("same.txt", b"second"),
        ]);
        let cancel = AtomicBool::new(false);
        assert_eq!(checked_exact(source.clone(), &cancel).unwrap(), source);
        let mut bad = source.clone();
        bad[30 + 8] ^= 1;
        assert!(checked_exact(bad, &cancel).is_err());
        assert!(checked_exact(source[..source.len() - 1].to_vec(), &cancel).is_err());
        let mut padded = source.clone();
        padded.extend([0; 16]);
        assert!(checked_exact(padded, &cancel).is_err());
        assert!(checked_exact(source, &AtomicBool::new(true)).is_err());
    }
    #[test]
    fn exact_frame_preserves_zip_comments_and_empty_containers_without_new_records() {
        let mut source = crate::iff_directory::tests::fixture(&[]);
        let end = source.len() - 22;
        source[end + 20..end + 22].copy_from_slice(&8u16.to_le_bytes());
        source.extend(b"authored");
        assert_eq!(
            checked_exact(source.clone(), &AtomicBool::new(false)).unwrap(),
            source
        );
    }
    #[test]
    fn populated_rounded_destination_requires_original_boundary_and_crc() {
        use std::io::{Cursor, Write};
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file(
                "authored.bin",
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(b"original").unwrap();
        let original = writer.finish().unwrap().into_inner();
        let mut rounded = original.clone();
        rounded.resize(256 * 1024, 0);
        assert_eq!(checked(rounded, &AtomicBool::new(false)).unwrap(), original);
        let payload = 30
            + codec::u16_at(&original, 26).unwrap() as usize
            + codec::u16_at(&original, 28).unwrap() as usize;
        let mut corrupt = original.clone();
        corrupt[payload] ^= 1;
        corrupt.resize(256 * 1024, 0);
        assert!(checked(corrupt, &AtomicBool::new(false)).is_err());
        let central = codec::u32_at(&original, original.len() - 22 + 16).unwrap() as usize;
        let mut truncated = original[..central].to_vec();
        truncated.resize(256 * 1024, 0);
        // Padding may not stand in for missing central/end records.
        assert!(checked(truncated, &AtomicBool::new(false)).is_err());
        assert!(checked(original, &AtomicBool::new(true)).is_err());
    }
}
