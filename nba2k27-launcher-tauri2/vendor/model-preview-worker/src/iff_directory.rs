//! App-owned physical ZIP directory checks matching the preserved WPF reader.
//! Do not rely on a name-keyed ZIP map to validate every authored occurrence.
use crate::{catalog::safe_relative, codec};
use anyhow::{Context, Result, ensure};
use std::{
    io::{Cursor, Read, Write},
    sync::atomic::AtomicBool,
};

#[derive(Clone, Copy)]
struct Record {
    name_start: usize,
    name_len: usize,
    data: usize,
    packed: u64,
    raw: u64,
    crc: u32,
    method: u16,
    name_adapter: bool,
}
fn span(bytes: &[u8], start: usize, size: usize) -> Result<&[u8]> {
    ensure!(
        start <= bytes.len() && size <= bytes.len() - start,
        "Truncated IFF structure"
    );
    Ok(&bytes[start..start + size])
}
fn position(value: u64) -> Result<usize> {
    Ok(usize::try_from(value).context("IFF offset exceeds address space")?)
}

#[cfg(test)]
pub fn validate(bytes: &[u8], cancel: &AtomicBool) -> Result<usize> {
    walk(bytes, |_| codec::cancelled(cancel))
}
/// WPF uses the physical UTF8 name, not ZIP's CP437/Unicode-extra override.
/// Common ASCII and flagged UTF8 names need no additional name table.
pub fn name_policy(bytes: &[u8], cancel: &AtomicBool) -> Result<(usize, Option<Vec<String>>)> {
    let mut adapter = false;
    let count = walk(bytes, |record| {
        codec::cancelled(cancel)?;
        adapter |= record.name_adapter;
        Ok(())
    })?;
    let names = if adapter {
        let mut names = Vec::with_capacity(count);
        walk(bytes, |record| {
            codec::cancelled(cancel)?;
            names.push(
                String::from_utf8_lossy(span(bytes, record.name_start, record.name_len)?)
                    .replace('\\', "/"),
            );
            Ok(())
        })?;
        Some(names)
    } else {
        None
    };
    Ok((count, names))
}
fn walk(bytes: &[u8], mut record: impl FnMut(Record) -> Result<()>) -> Result<usize> {
    ensure!(bytes.len() >= 22, "IFF end record is missing");
    let first = bytes.len().saturating_sub(65557);
    let end = (first..=bytes.len() - 22)
        .rev()
        .find(|p| {
            bytes[*p..].starts_with(b"PK\x05\x06")
                && *p + 22 + u16::from_le_bytes([bytes[*p + 20], bytes[*p + 21]]) as usize
                    == bytes.len()
        })
        .context("IFF does not contain a complete ZIP end record")?;
    ensure!(
        codec::u16_at(bytes, end + 4)? == 0 && codec::u16_at(bytes, end + 6)? == 0,
        "Multi-disk IFF is unsupported"
    );
    let mut count = codec::u16_at(bytes, end + 10)? as u64;
    let mut size = codec::u32_at(bytes, end + 12)? as u64;
    let mut offset = codec::u32_at(bytes, end + 16)? as u64;
    let mut central_end = end;
    if count == u16::MAX as u64 || size == u32::MAX as u64 || offset == u32::MAX as u64 {
        let loc = end.checked_sub(20).context("Missing ZIP64 locator")?;
        ensure!(
            codec::u32_at(bytes, loc)? == 0x07064b50
                && codec::u32_at(bytes, loc + 4)? == 0
                && codec::u32_at(bytes, loc + 16)? == 1,
            "Invalid ZIP64 locator"
        );
        let zip64 = position(codec::u64_at(bytes, loc + 8)?)?;
        span(bytes, zip64, 56)?;
        ensure!(
            codec::u32_at(bytes, zip64)? == 0x06064b50
                && codec::u32_at(bytes, zip64 + 16)? == 0
                && codec::u32_at(bytes, zip64 + 20)? == 0,
            "Invalid ZIP64 end record"
        );
        let record_size = position(codec::u64_at(bytes, zip64 + 4)?)?;
        ensure!(
            record_size >= 44
                && zip64
                    .checked_add(12)
                    .and_then(|p| p.checked_add(record_size))
                    == Some(loc),
            "ZIP64 end size is inconsistent"
        );
        count = codec::u64_at(bytes, zip64 + 32)?;
        ensure!(
            codec::u64_at(bytes, zip64 + 24)? == count,
            "ZIP64 member counts differ"
        );
        size = codec::u64_at(bytes, zip64 + 40)?;
        offset = codec::u64_at(bytes, zip64 + 48)?;
        central_end = zip64;
    } else {
        ensure!(
            codec::u16_at(bytes, end + 8)? as u64 == count,
            "IFF member counts differ"
        );
    }
    let central = position(offset)?;
    ensure!(
        central <= central_end && size == (central_end - central) as u64 && count <= size / 46,
        "IFF central directory size/offset is inconsistent"
    );
    ensure!(
        count <= crate::container::MEMBER_COUNT_MAX as u64,
        "IFF exceeds 250,000-member safety limit"
    );
    let mut p = central;
    for _ in 0..count {
        ensure!(
            p <= central_end && 46 <= central_end - p,
            "Truncated IFF central directory"
        );
        ensure!(
            codec::u32_at(bytes, p)? == 0x02014b50,
            "Invalid central directory signature"
        );
        let flags = codec::u16_at(bytes, p + 8)?;
        let method = codec::u16_at(bytes, p + 10)?;
        ensure!(
            flags & 1 == 0 && matches!(method, 0 | 8),
            "Encrypted or unsupported IFF compression"
        );
        let name_len = codec::u16_at(bytes, p + 28)? as usize;
        let extra_len = codec::u16_at(bytes, p + 30)? as usize;
        let comment_len = codec::u16_at(bytes, p + 32)? as usize;
        let length = 46 + name_len + extra_len + comment_len;
        ensure!(length <= central_end - p, "Truncated IFF directory entry");
        let name = span(bytes, p + 46, name_len)?;
        let decoded = String::from_utf8_lossy(name);
        safe_relative(decoded.trim_end_matches(['/', '\\']))?;
        let mut extra = p + 46 + name_len;
        let extra_end = extra + extra_len;
        let mut unicode_extra = false;
        while extra + 4 <= extra_end {
            let tag = codec::u16_at(bytes, extra)?;
            let n = codec::u16_at(bytes, extra + 2)? as usize;
            if n > extra_end - extra - 4 {
                break;
            }
            unicode_extra |= tag == 0x7075;
            extra += 4 + n;
        }
        let name_adapter =
            name.contains(&b'\\') || (flags & 0x800 == 0 && !name.is_ascii()) || unicode_extra;
        let mut packed = codec::u32_at(bytes, p + 20)? as u64;
        let mut raw = codec::u32_at(bytes, p + 24)? as u64;
        let mut local = codec::u32_at(bytes, p + 42)? as u64;
        if [packed, raw, local].contains(&(u32::MAX as u64)) {
            let mut q = p + 46 + name_len;
            let extra_end = q + extra_len;
            let mut found = false;
            while q + 4 <= extra_end {
                let tag = codec::u16_at(bytes, q)?;
                let n = codec::u16_at(bytes, q + 2)? as usize;
                ensure!(n <= extra_end - q - 4, "Truncated ZIP64 extra field");
                if tag == 1 {
                    let block_end = q + 4 + n;
                    q += 4;
                    for value in [&mut raw, &mut packed, &mut local] {
                        if *value == u32::MAX as u64 {
                            ensure!(
                                q <= block_end && 8 <= block_end - q,
                                "Missing ZIP64 size/offset"
                            );
                            *value = codec::u64_at(bytes, q)?;
                            q += 8;
                        }
                    }
                    found = true;
                    break;
                }
                q += 4 + n;
            }
            ensure!(found, "Missing ZIP64 extra field");
        }
        let local = position(local)?;
        span(bytes, local, 30)?;
        ensure!(
            codec::u32_at(bytes, local)? == 0x04034b50
                && codec::u16_at(bytes, local + 8)? == method,
            "Local IFF compression/header differs from directory"
        );
        let local_name_len = codec::u16_at(bytes, local + 26)? as usize;
        let local_name = span(bytes, local + 30, local_name_len)?;
        ensure!(
            local_name == name
                || local_name.len() == name.len()
                    && local_name.iter().zip(name).all(|(a, b)| (if *a == b'\\' {
                        b'/'
                    } else {
                        *a
                    }) == (if *b == b'\\' {
                        b'/'
                    } else {
                        *b
                    })),
            "Local and central IFF names differ"
        );
        let data = local
            .checked_add(30 + local_name_len + codec::u16_at(bytes, local + 28)? as usize)
            .context("IFF member offset overflow")?;
        ensure!(
            data <= central && packed <= (central - data) as u64,
            "IFF member exceeds its data region"
        );
        record(Record {
            name_start: p + 46,
            name_len,
            data,
            packed,
            raw,
            crc: codec::u32_at(bytes, p + 16)?,
            method,
            name_adapter,
        })?;
        p += length;
    }
    ensure!(
        p == central_end,
        "IFF directory byte count differs from its entries"
    );
    Ok(count as usize)
}

/// Only archives whose physical occurrences are hidden by the ZIP name map use
/// this storage. It OWNS the original Vec; no full-container clone or on-disk
/// extraction is needed, and occurrence IDs stay in central-directory order.
pub struct Physical {
    bytes: Vec<u8>,
    records: Vec<Record>,
}
impl Physical {
    pub fn open(bytes: Vec<u8>, cancel: &AtomicBool) -> Result<Self> {
        let mut records = Vec::new();
        walk(&bytes, |record| {
            codec::cancelled(cancel)?;
            records.push(record);
            Ok(())
        })?;
        // Listing metadata does not decompress payloads (same as WPF). Every
        // selected read/Save checks its own CRC; full exports validate ALL.
        Ok(Self { bytes, records })
    }
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
    pub fn member(&self, index: usize) -> Result<crate::container::MemberInfo> {
        let r = self
            .records
            .get(index)
            .context("IFF member index is outside the physical directory")?;
        Ok(crate::container::MemberInfo {
            name: String::from_utf8_lossy(span(&self.bytes, r.name_start, r.name_len)?)
                .replace('\\', "/"),
            size: r.raw,
            packed_size: r.packed,
            crc32: r.crc,
            data_offset: r.data as u64,
            compression: if r.method == 0 { "Stored" } else { "Deflate" }.into(),
            encrypted: false,
        })
    }
    fn reader(&self, index: usize) -> Result<Box<dyn Read + '_>> {
        let r = self
            .records
            .get(index)
            .context("IFF member index is outside the physical directory")?;
        let data = span(&self.bytes, r.data, position(r.packed)?)?;
        Ok(if r.method == 0 {
            Box::new(Cursor::new(data))
        } else {
            Box::new(flate2::read::DeflateDecoder::new(data))
        })
    }
    pub fn prefix(&self, index: usize, limit: usize, cancel: &AtomicBool) -> Result<Vec<u8>> {
        codec::cancelled(cancel)?;
        let mut out = Vec::new();
        self.reader(index)?
            .take(limit as u64)
            .read_to_end(&mut out)?;
        Ok(out)
    }
    pub fn copy(
        &self,
        index: usize,
        max: usize,
        cancel: &AtomicBool,
        output: &mut dyn Write,
    ) -> Result<u64> {
        let r = self
            .records
            .get(index)
            .context("IFF member index is outside the physical directory")?;
        ensure!(r.raw <= max as u64, "IFF member exceeds {max} byte limit");
        let mut reader = self.reader(index)?;
        let mut crc = crc32fast::Hasher::new();
        let mut count = 0u64;
        let mut chunk = [0; 128 * 1024];
        loop {
            codec::cancelled(cancel)?;
            let n = reader.read(&mut chunk)?;
            if n == 0 {
                break;
            }
            count += n as u64;
            ensure!(
                count <= r.raw,
                "Physical IFF member exceeds its declared size"
            );
            crc.update(&chunk[..n]);
            output.write_all(&chunk[..n])?;
        }
        ensure!(
            count == r.raw && crc.finalize() == r.crc,
            "Physical IFF member size/CRC differs"
        );
        Ok(count)
    }
    pub fn validate(&self, cancel: &AtomicBool, mut progress: impl FnMut(f64)) -> Result<usize> {
        let mut total = 0u64;
        let mut last = std::time::Instant::now();
        for (i, r) in self.records.iter().enumerate() {
            total = total
                .checked_add(r.raw)
                .context("IFF member total overflow")?;
            ensure!(
                total <= crate::container::EXPANDED_MAX,
                "IFF members exceed 2 GiB streaming validation budget"
            );
            self.copy(
                i,
                crate::container::STREAM_MEMBER_MAX,
                cancel,
                &mut std::io::sink(),
            )?;
            if last.elapsed().as_millis() >= 35 || i + 1 == self.records.len() {
                progress((i + 1) as f64 / self.records.len().max(1) as f64);
                last = std::time::Instant::now();
            }
        }
        Ok(self.records.len())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::container::Archive;
    // Physical records, intentionally independent of the ZIP library's name map.
    fn u16le(b: &mut [u8], p: usize, n: u16) {
        b[p..p + 2].copy_from_slice(&n.to_le_bytes());
    }
    fn u32le(b: &mut [u8], p: usize, n: u32) {
        b[p..p + 4].copy_from_slice(&n.to_le_bytes());
    }
    pub(crate) fn fixture(members: &[(&str, &[u8])]) -> Vec<u8> {
        fixture_method(members, 0)
    }
    fn fixture_method(members: &[(&str, &[u8])], method: u16) -> Vec<u8> {
        let mut data = Vec::new();
        let mut central = Vec::new();
        for (name, payload) in members {
            let offset = data.len();
            let n = name.as_bytes();
            let crc = crc32fast::hash(payload);
            let packed = if method == 8 {
                let mut encoder =
                    flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
                encoder.write_all(payload).unwrap();
                encoder.finish().unwrap()
            } else {
                payload.to_vec()
            };
            let mut local = [0; 30];
            u32le(&mut local, 0, 0x04034b50);
            u16le(&mut local, 4, 20);
            u16le(&mut local, 8, method);
            u32le(&mut local, 14, crc);
            u32le(&mut local, 18, packed.len() as u32);
            u32le(&mut local, 22, payload.len() as u32);
            u16le(&mut local, 26, n.len() as u16);
            data.extend(local);
            data.extend(n);
            data.extend(packed.iter().copied());
            let mut entry = [0; 46];
            u32le(&mut entry, 0, 0x02014b50);
            u16le(&mut entry, 4, 20);
            u16le(&mut entry, 6, 20);
            u16le(&mut entry, 10, method);
            u32le(&mut entry, 16, crc);
            u32le(&mut entry, 20, packed.len() as u32);
            u32le(&mut entry, 24, payload.len() as u32);
            u16le(&mut entry, 28, n.len() as u16);
            u32le(&mut entry, 42, offset as u32);
            central.extend(entry);
            central.extend(n);
        }
        let offset = data.len();
        let size = central.len();
        data.extend(central);
        let mut end = [0; 22];
        u32le(&mut end, 0, 0x06054b50);
        u16le(&mut end, 8, members.len() as u16);
        u16le(&mut end, 10, members.len() as u16);
        u32le(&mut end, 12, size as u32);
        u32le(&mut end, 16, offset as u32);
        data.extend(end);
        data
    }
    fn count(bytes: &[u8]) -> Result<usize> {
        validate(bytes, &AtomicBool::new(false))
    }
    #[test]
    fn unique_empty_directory_and_comments_remain_supported() {
        for members in [
            vec![],
            vec![("folder/", &b""[..]), ("folder/file", &b"authored"[..])],
        ] {
            let mut bytes = fixture(&members);
            assert_eq!(count(&bytes).unwrap(), members.len());
            let end = bytes.len() - 22;
            u16le(&mut bytes, end + 20, 13);
            bytes.extend(b"PK\x05\x06 comment!");
            assert_eq!(count(&bytes).unwrap(), members.len());
            assert_eq!(Archive::open(bytes.clone()).unwrap().into_bytes(), bytes);
        }
    }
    #[test]
    fn unsafe_directory_is_checked_even_without_file_payload() {
        for name in ["../", "C:/", "/absolute/", "folder/../../"] {
            assert!(Archive::open(fixture(&[(name, b"")])).is_err(), "{name}");
        }
    }
    #[test]
    fn contradictory_directory_and_local_metadata_fail_before_publication() {
        let bytes = fixture(&[("safe.bin", b"authored")]);
        let end = bytes.len() - 22;
        let central = codec::u32_at(&bytes, end + 16).unwrap() as usize;
        for (offset, value) in [
            (end + 8, 0),
            (end + 10, 0),
            (end + 12, 0),
            (end + 16, 0),
            (30, b'X'),
            (8, 8),
            (central + 28, 0),
        ] {
            let mut bad = bytes.clone();
            bad[offset] = value;
            assert!(Archive::open(bad).is_err(), "offset {offset}");
        }
        for length in [0, 12, bytes.len() - 1] {
            assert!(Archive::open(bytes[..length].to_vec()).is_err());
        }
    }
    #[test]
    fn data_descriptor_uses_authoritative_central_sizes() {
        let mut bytes = fixture(&[("safe.bin", b"authored")]);
        let old_end = bytes.len() - 22;
        let central = codec::u32_at(&bytes, old_end + 16).unwrap() as usize;
        let mut descriptor = [0; 16];
        u32le(&mut descriptor, 0, 0x08074b50);
        u32le(&mut descriptor, 4, crc32fast::hash(b"authored"));
        u32le(&mut descriptor, 8, 8);
        u32le(&mut descriptor, 12, 8);
        bytes.splice(central..central, descriptor);
        u16le(&mut bytes, 6, 8);
        bytes[14..26].fill(0);
        u16le(&mut bytes, central + 16 + 8, 8);
        u32le(&mut bytes, old_end + 16 + 16, (central + 16) as u32);
        let mut archive = Archive::open(bytes.clone()).unwrap();
        assert_eq!(archive.validate(&AtomicBool::new(false)).unwrap(), 1);
        assert_eq!(archive.into_bytes(), bytes);
    }
    #[test]
    fn repeated_names_preserve_original_bytes_and_never_silently_convert() {
        let bytes = fixture(&[("same.bin", b"first"), ("same.bin", b"second")]);
        let mut archive = Archive::open(bytes.clone()).unwrap();
        assert!(archive.has_shadowed_members());
        assert!(archive.require_unique_members().is_err());
        assert_eq!(archive.validate(&AtomicBool::new(false)).unwrap(), 2);
        let report = crate::validation::archive(
            &mut archive,
            "original",
            &serde_json::Value::Null,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(report["summary"]["entries"], 2);
        assert_eq!(report["summary"]["indexedEntries"], 1);
        assert_eq!(report["status"], "warning");
        assert_eq!(archive.into_bytes(), bytes);
    }
    #[test]
    fn hidden_duplicate_corruption_and_forged_lengths_are_rejected() {
        let bytes = fixture(&[("same.bin", b"first"), ("same.bin", b"second")]);
        let central = codec::u32_at(&bytes, bytes.len() - 22 + 16).unwrap() as usize;
        for offset in [30 + 8, central + 16, central + 24] {
            let mut bad = bytes.clone();
            bad[offset] ^= 1;
            let mut archive = Archive::open(bad.clone()).unwrap();
            assert_eq!(archive.names().unwrap().len(), 2);
            assert!(archive.read(0, 1024).is_err());
            assert_eq!(archive.read(1, 1024).unwrap(), b"second");
            assert!(archive.validate(&AtomicBool::new(false)).is_err());
            assert_eq!(archive.into_bytes(), bad);
        }
        assert!(Archive::open_with_cancel(bytes, &AtomicBool::new(true)).is_err());
    }
    #[test]
    fn structural_truncation_and_adversarial_offsets_return_errors_not_panics() {
        let bytes = fixture(&[("safe.bin", b"authored")]);
        for len in 0..bytes.len() {
            assert!(count(&bytes[..len]).is_err());
        }
        for p in 0..bytes.len() {
            let mut bad = bytes.clone();
            bad[p] ^= 255;
            let result = std::panic::catch_unwind(|| count(&bad));
            assert!(result.is_ok(), "offset {p}");
        }
    }
    #[test]
    fn physical_occurrence_reads_preserve_order_metadata_and_original_for_both_methods() {
        for method in [0, 8] {
            let source = fixture_method(
                &[
                    ("nested\\same.txt", b"first authored"),
                    ("nested\\same.txt", b"second different"),
                    ("last.bin", b"last"),
                ],
                method,
            );
            let mut a = Archive::open(source.clone()).unwrap();
            let names = a.names().unwrap();
            assert_eq!(names.len(), 3);
            assert_eq!(names[0]["name"], "nested/same.txt");
            assert_eq!(names[0]["occurrence"], 1);
            assert_eq!(names[1]["occurrence"], 2);
            assert_eq!(names[0]["occurrences"], 2);
            assert!(names[2].get("occurrences").is_none());
            assert_ne!(
                a.member(0).unwrap().data_offset,
                a.member(1).unwrap().data_offset
            );
            for (i, expected) in [b"first authored".as_slice(), b"second different", b"last"]
                .iter()
                .enumerate()
            {
                assert_eq!(a.read(i, 1024).unwrap(), *expected);
                assert_eq!(a.member(i).unwrap().crc32, crc32fast::hash(expected));
            }
            assert!(a.shared().is_err());
            assert!(a.named("nested/same.txt").is_err());
            assert!(a.read(3, 1024).is_err());
            assert_eq!(a.into_bytes(), source);
        }
    }
    #[test]
    fn authored_utf8_and_slashes_are_used_without_altering_source_bytes() {
        let cancel = AtomicBool::new(false);
        let source = fixture(&[("nested\\éye.txt", b"authored"), ("plain.txt", b"other")]);
        let mut a = Archive::open(source.clone()).unwrap();
        assert!(a.needs_name_adapter());
        assert!(!a.has_shadowed_members());
        assert_eq!(a.names().unwrap()[0]["name"], "nested/éye.txt");
        assert_eq!(a.member(0).unwrap().name, "nested/éye.txt");
        assert_eq!(a.named("nested/éye.txt").unwrap(), b"authored");
        assert_eq!(a.named("nested\\éye.txt").unwrap(), b"authored");
        assert!(a.shared().is_err());
        assert_eq!(a.validate(&cancel).unwrap(), 2);
        assert_eq!(a.into_bytes(), source);
    }
    #[test]
    fn normalized_duplicate_spellings_keep_independent_occurrences() {
        let source = fixture(&[
            ("nested\\same.txt", b"first"),
            ("nested/same.txt", b"second"),
        ]);
        let mut a = Archive::open(source.clone()).unwrap();
        assert!(a.has_shadowed_members());
        let names = a.names().unwrap();
        assert_eq!(names[0]["name"], names[1]["name"]);
        assert_eq!(names[0]["occurrence"], 1);
        assert_eq!(names[1]["occurrence"], 2);
        assert_eq!(names[1]["occurrences"], 2);
        assert_eq!(a.read(0, 128).unwrap(), b"first");
        assert_eq!(a.read(1, 128).unwrap(), b"second");
        assert!(a.named("nested/same.txt").is_err());
        assert_eq!(a.validate(&AtomicBool::new(false)).unwrap(), 2);
        assert_eq!(a.into_bytes(), source);
    }
    #[test]
    fn naming_adapter_is_limited_to_selected_payloads_and_keeps_lookup_order() {
        let source = fixture(&[
            ("texture.tld", b"earlier leaf"),
            ("nested\\texture.tld", b"later exact"),
            ("nested\\texture.mip0", b"mip"),
            ("unrelated.txt", b"not copied"),
        ]);
        let mut a = Archive::open(source.clone()).unwrap();
        let cancel = AtomicBool::new(false);
        let mut input = a
            .decoder_input(&["nested/texture.tld", "nested/texture.mip0"], &cancel)
            .unwrap();
        assert_eq!(input.source_member_count(), 2);
        assert!(!input.needs_name_adapter());
        assert_eq!(input.named("nested/texture.tld").unwrap(), b"earlier leaf");
        assert_eq!(input.named("nested/texture.mip0").unwrap(), b"mip");
        assert!(input.named("unrelated.txt").is_err());
        assert!(a.decoder_input(&["texture.tld"; 3], &cancel).is_err());
        assert!(
            a.decoder_input(&["texture.tld"], &AtomicBool::new(true))
                .is_err()
        );
        assert_eq!(a.into_bytes(), source);
    }
    #[test]
    fn standard_ascii_and_flagged_utf8_keep_the_normal_reader() {
        for (name, flags) in [("plain.txt", 0), ("éclair.txt", 0x800)] {
            let mut source = fixture(&[(name, b"authored")]);
            let central = codec::u32_at(&source, source.len() - 22 + 16).unwrap() as usize;
            u16le(&mut source, 6, flags);
            u16le(&mut source, central + 8, flags);
            let mut a = Archive::open(source.clone()).unwrap();
            assert!(!a.needs_name_adapter());
            assert!(a.shared().is_ok());
            assert_eq!(a.names().unwrap()[0]["name"], name);
            assert_eq!(a.named(name).unwrap(), b"authored");
            assert_eq!(a.into_bytes(), source);
        }
    }
    #[test]
    fn unicode_extra_never_renames_merges_or_rejects_raw_authored_names() {
        for aliases in [vec!["../unsafe.txt"], vec!["alias.txt", "alias.txt"]] {
            let members = if aliases.len() == 1 {
                vec![("first.txt", b"first".as_slice())]
            } else {
                vec![
                    ("first.txt", b"first".as_slice()),
                    ("second.txt", b"second".as_slice()),
                ]
            };
            for bad_crc in [false, true] {
                let mut source = fixture(&members);
                let central = codec::u32_at(&source, source.len() - 22 + 16).unwrap() as usize;
                let old_size = codec::u32_at(&source, source.len() - 22 + 12).unwrap();
                let mut cursor = central;
                let mut extras = Vec::new();
                for ((name, _), alias) in members.iter().zip(&aliases) {
                    let mut extra = vec![0; 9];
                    u16le(&mut extra, 0, 0x7075);
                    u16le(&mut extra, 2, (5 + alias.len()) as u16);
                    extra[4] = 1;
                    u32le(
                        &mut extra,
                        5,
                        crc32fast::hash(name.as_bytes()) ^ u32::from(bad_crc),
                    );
                    extra.extend(alias.as_bytes());
                    extras.push((cursor, cursor + 46 + name.len(), extra));
                    cursor += 46 + name.len();
                }
                let added: usize = extras.iter().map(|(_, _, v)| v.len()).sum();
                for (header, insert, extra) in extras.into_iter().rev() {
                    u16le(&mut source, header + 30, extra.len() as u16);
                    source.splice(insert..insert, extra);
                }
                let end = source.len() - 22;
                u32le(&mut source, end + 12, old_size + added as u32);
                let mut a = Archive::open(source.clone()).unwrap();
                assert!(!a.has_shadowed_members());
                for (index, (name, payload)) in members.iter().enumerate() {
                    assert_eq!(a.member(index).unwrap().name, *name);
                    assert_eq!(a.named(name).unwrap(), *payload);
                }
                assert!(a.named("alias.txt").is_err());
                assert_eq!(a.validate(&AtomicBool::new(false)).unwrap(), members.len());
                assert_eq!(a.into_bytes(), source);
            }
        }
    }
    #[test]
    fn naming_decoder_adapter_checks_selected_crc_before_returning_a_view() {
        let mut source = fixture(&[
            ("nested\\éye.tld", b"selected bytes"),
            ("safe.txt", b"sibling"),
        ]);
        source[30 + "nested\\éye.tld".len()] ^= 1;
        let mut a = Archive::open(source.clone()).unwrap();
        assert!(
            a.decoder_input(&["nested/éye.tld"], &AtomicBool::new(false))
                .is_err()
        );
        assert_eq!(a.named("safe.txt").unwrap(), b"sibling");
        assert_eq!(a.into_bytes(), source);
    }
    #[test]
    fn physical_copy_is_bounded_cancellable_and_recovers_after_writer_error() {
        struct Fail;
        impl Write for Fail {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("fixture write fault"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let payload = vec![b'a'; 300000];
        let source = fixture(&[("same.bin", &payload), ("same.bin", b"independent")]);
        let mut a = Archive::open(source.clone()).unwrap();
        let mut out = Vec::new();
        assert!(
            a.copy_member(0, 299999, &AtomicBool::new(false), &mut out)
                .is_err()
        );
        assert!(out.is_empty());
        assert!(
            a.copy_member(0, 300000, &AtomicBool::new(true), &mut out)
                .is_err()
        );
        assert!(out.is_empty());
        assert!(
            a.copy_member(0, 300000, &AtomicBool::new(false), &mut Fail)
                .is_err()
        );
        assert_eq!(a.read(1, 1024).unwrap(), b"independent");
        assert_eq!(
            a.copy_member(0, 300000, &AtomicBool::new(false), &mut out)
                .unwrap(),
            300000
        );
        assert_eq!(out, payload);
        assert_eq!(a.into_bytes(), source);
    }
    #[test]
    fn original_report_inspects_every_physical_document_not_only_last_name() {
        let source = fixture(&[("same.txtr", b"{broken"), ("same.txtr", b"{\"marker\":2}")]);
        let mut a = Archive::open(source.clone()).unwrap();
        a.validate(&AtomicBool::new(false)).unwrap();
        let report = crate::validation::archive(
            &mut a,
            "original",
            &serde_json::Value::Null,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(report["summary"]["scneTxtr"], 2);
        assert_eq!(report["errors"].as_array().unwrap().len(), 1);
        assert_eq!(a.into_bytes(), source);
    }
}
