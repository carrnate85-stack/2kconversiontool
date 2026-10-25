//! Larger container budgets and ZIP64 counts for this app only.
use crate::codec;
use anyhow::{Context, Result, ensure};
use std::{
    io::{Cursor, Read, Seek, Write},
    sync::atomic::AtomicBool,
};
pub const MAX: usize = 1024 * 1024 * 1024;
// Streaming validation does not retain expanded payloads. Keep its work bound
// separate from allocation limits, matching the converter's existing 2 GiB
// expanded-output envelope. Individual buffered reads retain caller limits.
pub const EXPANDED_MAX: u64 = 2 * 1024 * 1024 * 1024;
pub const STREAM_MEMBER_MAX: usize = i32::MAX as usize;
pub const MEMBER_COUNT_MAX: usize = 250_000;
/// Retain shared member/texture APIs without editing sibling tools' codec.
pub struct Archive {
    indexed: nba2k_assets::codec::Archive,
    count: usize,
    indexed_count: usize,
    physical: Option<crate::iff_directory::Physical>,
    ambiguous: bool,
}
pub struct MemberInfo {
    pub name: String,
    pub size: u64,
    pub packed_size: u64,
    pub crc32: u32,
    pub data_offset: u64,
    pub compression: String,
    pub encrypted: bool,
}
impl MemberInfo {
    pub fn is_dir(&self) -> bool {
        self.name.ends_with('/') || self.name.ends_with('\\')
    }
    fn json(&self, index: usize) -> serde_json::Value {
        serde_json::json!({"id":index,"name":self.name,"size":self.size,"packedSize":self.packed_size,"crc32":format!("{:08X}",self.crc32)})
    }
}
impl Archive {
    pub fn open(bytes: Vec<u8>) -> Result<Self> {
        Self::open_with_cancel(bytes, &AtomicBool::new(false))
    }
    pub fn open_with_cancel(bytes: Vec<u8>, cancel: &AtomicBool) -> Result<Self> {
        codec::cancelled(cancel)?;
        let (count, policy_names) = crate::iff_directory::name_policy(&bytes, cancel)?;
        if let Some(names) = policy_names {
            let unique: std::collections::HashSet<_> = names.iter().collect();
            let ambiguous = unique.len() != count;
            // ZIP Unicode extras can rename, merge or reject otherwise valid
            // raw WPF names. This view owns the ORIGINAL bytes and indexes the
            // physical directory directly, without parsing those name overrides.
            let physical = crate::iff_directory::Physical::open(bytes, cancel)?;
            let zip =
                zip::ZipArchive::new(Cursor::new([b"PK\x05\x06".as_slice(), &[0; 18]].concat()))?;
            return Ok(Self {
                indexed: nba2k_assets::codec::Archive { zip },
                count,
                indexed_count: count,
                physical: Some(physical),
                ambiguous,
            });
        }
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes))?;
        ensure!(
            zip.len() <= MEMBER_COUNT_MAX,
            "IFF exceeds 250,000-member safety limit"
        );
        let indexed_count = zip.len();
        let ambiguous = indexed_count != count;
        let physical = if indexed_count != count {
            let bytes = zip.into_inner().into_inner();
            let physical = crate::iff_directory::Physical::open(bytes, cancel)?;
            // Shared consumers are accessed only through shared(), which
            // explicitly refuses ambiguity. They cannot see a partial archive.
            zip = zip::ZipArchive::new(Cursor::new([b"PK\x05\x06".as_slice(), &[0; 18]].concat()))?;
            Some(physical)
        } else {
            None
        };
        Ok(Self {
            indexed: nba2k_assets::codec::Archive { zip },
            count,
            indexed_count,
            physical,
            ambiguous,
        })
    }
    pub fn has_shadowed_members(&self) -> bool {
        self.ambiguous
    }
    pub fn needs_name_adapter(&self) -> bool {
        self.physical.is_some()
    }
    pub fn source_member_count(&self) -> usize {
        self.count
    }
    pub fn indexed_member_count(&self) -> usize {
        self.indexed_count
    }
    pub fn require_unique_members(&self) -> Result<()> {
        ensure!(
            !self.has_shadowed_members(),
            "IFF contains repeated identical member names. Inspect/save each occurrence or use original-container export to retain every entry; name-based model/texture preview and mod-ready conversion cannot safely choose one."
        );
        Ok(())
    }
    pub fn shared(&mut self) -> Result<&mut nba2k_assets::codec::Archive> {
        self.require_unique_members()?;
        ensure!(
            !self.needs_name_adapter(),
            "Authored IFF naming requires the app-owned member adapter"
        );
        Ok(&mut self.indexed)
    }
    pub fn member(&mut self, index: usize) -> Result<MemberInfo> {
        if let Some(physical) = &self.physical {
            return physical.member(index);
        }
        let m = self.indexed.zip.by_index(index)?;
        Ok(MemberInfo {
            name: m.name().to_string(),
            size: m.size(),
            packed_size: m.compressed_size(),
            crc32: m.crc32(),
            data_offset: m.data_start(),
            compression: match m.compression() {
                zip::CompressionMethod::Stored => "Stored".into(),
                zip::CompressionMethod::Deflated => "Deflate".into(),
                method => format!("Unsupported ({method:?})"),
            },
            encrypted: m.encrypted(),
        })
    }
    pub fn names(&mut self) -> Result<Vec<serde_json::Value>> {
        if !self.needs_name_adapter() {
            return self.indexed.names();
        }
        let mut names = Vec::with_capacity(self.count);
        let mut counts = std::collections::BTreeMap::<String, usize>::new();
        for i in 0..self.count {
            let info = self.member(i)?;
            *counts.entry(info.name.clone()).or_default() += 1;
            names.push(info.json(i));
        }
        let mut seen = std::collections::BTreeMap::<String, usize>::new();
        for value in &mut names {
            let name = value["name"].as_str().unwrap().to_string();
            let count = counts[&name];
            if count > 1 {
                let n = seen.entry(name).or_default();
                *n += 1;
                value["occurrence"] = (*n).into();
                value["occurrences"] = count.into();
            }
        }
        Ok(names)
    }
    pub fn named(&mut self, name: &str) -> Result<Vec<u8>> {
        self.require_unique_members()?;
        if !self.needs_name_adapter() {
            return self.shared()?.named(name);
        }
        let index = self
            .locate(name)?
            .context(format!("Missing IFF member {name}"))?;
        self.read(index, codec::MAX)
    }
    fn locate(&mut self, name: &str) -> Result<Option<usize>> {
        let normalized = name.replace('\\', "/");
        let leaf = normalized.rsplit('/').next().unwrap_or(&normalized);
        let alias = if leaf.to_ascii_lowercase().ends_with(".gz") {
            format!("{}.bin", &leaf[..leaf.len() - 3])
        } else if leaf.to_ascii_lowercase().ends_with(".bin") {
            format!("{}.gz", &leaf[..leaf.len() - 4])
        } else {
            String::new()
        };
        for index in 0..self.count {
            let candidate = self.member(index)?.name;
            if candidate.eq_ignore_ascii_case(&normalized)
                || candidate.eq_ignore_ascii_case(leaf)
                || (!alias.is_empty() && candidate.eq_ignore_ascii_case(&alias))
            {
                return Ok(Some(index));
            }
        }
        Ok(None)
    }
    /// Only unusual source naming uses this bounded two-resource view. Copy
    /// unchanged selected payloads, never clone/repack the whole source archive.
    /// Canonical UTF8 names allow the unchanged shared texture decoder to resolve
    /// TLD + external mip consistently. Preserve authored first-match order.
    pub fn decoder_input(&mut self, names: &[&str], cancel: &AtomicBool) -> Result<Self> {
        self.require_unique_members()?;
        ensure!(
            names.len() <= 2,
            "Texture adapter expects at most two resources"
        );
        let mut selected = Vec::new();
        for name in names {
            codec::cancelled(cancel)?;
            if let Some(index) = self.locate(name)? {
                selected.push(index);
            }
        }
        selected.sort_unstable();
        selected.dedup();
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let mut total = 0u64;
        for index in selected {
            codec::cancelled(cancel)?;
            let member = self.member(index)?;
            total = total
                .checked_add(member.size)
                .context("Texture adapter size overflow")?;
            ensure!(
                total <= MAX as u64,
                "Texture adapter exceeds 1 GiB source budget"
            );
            writer.start_file(
                &member.name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )?;
            self.copy_member(index, codec::MAX, cancel, &mut writer)?;
        }
        Self::open_with_cancel(writer.finish()?.into_inner(), cancel)
    }
    pub fn read(&mut self, index: usize, max: usize) -> Result<Vec<u8>> {
        if self.physical.is_none() {
            return self.indexed.read(index, max);
        }
        self.read_with_cancel(index, max, &AtomicBool::new(false))
    }
    pub fn read_with_cancel(
        &mut self,
        index: usize,
        max: usize,
        cancel: &AtomicBool,
    ) -> Result<Vec<u8>> {
        let info = self.member(index)?;
        ensure!(
            info.size <= max as u64,
            "IFF member exceeds {max} byte limit"
        );
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(info.size as usize)
            .context("Cannot allocate bounded member read")?;
        self.copy_member(index, max, cancel, &mut bytes)?;
        Ok(bytes)
    }
    pub fn prefix(&mut self, index: usize, limit: usize, cancel: &AtomicBool) -> Result<Vec<u8>> {
        if let Some(physical) = &self.physical {
            return physical.prefix(index, limit, cancel);
        }
        codec::cancelled(cancel)?;
        let mut bytes = Vec::new();
        self.indexed
            .zip
            .by_index(index)?
            .take(limit as u64)
            .read_to_end(&mut bytes)?;
        Ok(bytes)
    }
    pub fn copy_member(
        &mut self,
        index: usize,
        max: usize,
        cancel: &AtomicBool,
        output: &mut dyn Write,
    ) -> Result<u64> {
        if let Some(physical) = &self.physical {
            return physical.copy(index, max, cancel, output);
        }
        let mut member = self.indexed.zip.by_index(index)?;
        let size = member.size();
        ensure!(
            size <= max as u64 && !member.encrypted(),
            "Invalid member size or encryption"
        );
        let mut count = 0u64;
        let mut buffer = [0; 128 * 1024];
        loop {
            codec::cancelled(cancel)?;
            let n = member.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            count += n as u64;
            ensure!(count <= size, "IFF member exceeds declared size");
            output.write_all(&buffer[..n])?;
        }
        ensure!(count == size, "Incomplete member read/Save");
        Ok(count)
    }
    #[cfg(test)]
    pub fn validate(&mut self, cancel: &AtomicBool) -> Result<usize> {
        self.validate_with(cancel, |_| {})
    }
    pub fn validate_with(
        &mut self,
        cancel: &AtomicBool,
        progress: impl FnMut(f64),
    ) -> Result<usize> {
        if let Some(physical) = &self.physical {
            return physical.validate(cancel, progress);
        }
        validate_zip(&mut self.indexed.zip, cancel, progress)?;
        Ok(self.count)
    }
    pub fn into_bytes(self) -> Vec<u8> {
        self.physical
            .map(|p| p.into_bytes())
            .unwrap_or_else(|| self.indexed.zip.into_inner().into_inner())
    }
}
pub fn validate_zip<R: Read + Seek>(
    zip: &mut zip::ZipArchive<R>,
    cancel: &AtomicBool,
    mut progress: impl FnMut(f64),
) -> Result<usize> {
    ensure!(
        zip.len() <= MEMBER_COUNT_MAX,
        "IFF exceeds 250,000-member safety limit"
    );
    let mut total = 0u64;
    let mut chunk = [0; 256 * 1024];
    let mut last = std::time::Instant::now();
    for i in 0..zip.len() {
        codec::cancelled(cancel)?;
        let mut member = zip.by_index(i)?;
        ensure!(
            !member.encrypted() && member.size() <= STREAM_MEMBER_MAX as u64,
            "Invalid IFF member size or encryption"
        );
        if !member.is_dir() {
            crate::catalog::safe_relative(member.name())?;
        }
        total = total
            .checked_add(member.size())
            .context("IFF size overflow")?;
        ensure!(
            total <= EXPANDED_MAX,
            "IFF members exceed 2 GiB streaming validation budget"
        );
        let mut read = 0u64;
        loop {
            codec::cancelled(cancel)?;
            let n = member.read(&mut chunk)?;
            if n == 0 {
                break;
            }
            read += n as u64;
            ensure!(read <= member.size(), "IFF member size mismatch");
        }
        ensure!(read == member.size(), "IFF member truncated");
        drop(member);
        if last.elapsed().as_millis() >= 35 || i + 1 == zip.len() {
            progress((i + 1) as f64 / zip.len().max(1) as f64);
            last = std::time::Instant::now();
        }
    }
    Ok(zip.len())
}
// Walk records, never search for an arbitrary EOCD signature inside payloads.
// ZIP64 directory counts are required by the installed animation IFF. Large
// individual ZIP64 members and streaming data descriptors stay unsupported.
pub fn boundary(bytes: &[u8]) -> Result<usize> {
    let mut p = 0usize;
    let mut count = 0usize;
    while p + 30 <= bytes.len() && codec::u32_at(bytes, p)? == 0x04034b50 {
        ensure!(
            codec::u16_at(bytes, p + 6)? & 8 == 0,
            "Streaming ZIP not supported"
        );
        let size = codec::u32_at(bytes, p + 18)?;
        ensure!(size != u32::MAX, "ZIP64 individual member unsupported");
        p = p
            .checked_add(
                30 + codec::u16_at(bytes, p + 26)? as usize
                    + codec::u16_at(bytes, p + 28)? as usize
                    + size as usize,
            )
            .context("ZIP local size overflow")?;
        ensure!(p <= bytes.len(), "Truncated ZIP local record");
        count += 1;
        ensure!(
            count <= MEMBER_COUNT_MAX,
            "IFF exceeds 250,000-member safety limit"
        );
    }
    ensure!(count > 0, "No ZIP members");
    let central = p;
    let mut directories = 0usize;
    while p + 46 <= bytes.len() && codec::u32_at(bytes, p)? == 0x02014b50 {
        p = p
            .checked_add(
                46 + codec::u16_at(bytes, p + 28)? as usize
                    + codec::u16_at(bytes, p + 30)? as usize
                    + codec::u16_at(bytes, p + 32)? as usize,
            )
            .context("ZIP central size overflow")?;
        ensure!(p <= bytes.len(), "Truncated ZIP central record");
        directories += 1;
    }
    ensure!(directories == count, "Inconsistent ZIP member counts");
    let directory_bytes = p - central;
    let zip64 = p + 56 <= bytes.len() && codec::u32_at(bytes, p)? == 0x06064b50;
    if zip64 {
        let start = p;
        let record_size = usize::try_from(codec::u64_at(bytes, p + 4)?)?;
        ensure!(
            record_size >= 44 && record_size <= 1024 * 1024,
            "Invalid ZIP64 end size"
        );
        ensure!(
            codec::u32_at(bytes, p + 16)? == 0 && codec::u32_at(bytes, p + 20)? == 0,
            "Multi-disk ZIP64 unsupported"
        );
        ensure!(
            codec::u64_at(bytes, p + 24)? == count as u64
                && codec::u64_at(bytes, p + 32)? == count as u64
                && codec::u64_at(bytes, p + 40)? == directory_bytes as u64
                && codec::u64_at(bytes, p + 48)? == central as u64,
            "Inconsistent ZIP64 directory"
        );
        p = p
            .checked_add(12 + record_size)
            .context("ZIP64 end overflow")?;
        ensure!(
            p + 20 <= bytes.len()
                && codec::u32_at(bytes, p)? == 0x07064b50
                && codec::u32_at(bytes, p + 4)? == 0
                && codec::u64_at(bytes, p + 8)? == start as u64
                && codec::u32_at(bytes, p + 16)? == 1,
            "Invalid ZIP64 locator"
        );
        p += 20;
    }
    ensure!(
        p + 22 <= bytes.len() && codec::u32_at(bytes, p)? == 0x06054b50,
        "Incomplete ZIP end record"
    );
    ensure!(
        codec::u16_at(bytes, p + 4)? == 0 && codec::u16_at(bytes, p + 6)? == 0,
        "Multi-disk ZIP unsupported"
    );
    let recorded_size = codec::u32_at(bytes, p + 12)?;
    let recorded_offset = codec::u32_at(bytes, p + 16)?;
    // ZIP64's verified 64-bit fields are authoritative. Legacy fields may be
    // either their exact values or the standard 0xFFFFFFFF sentinels.
    ensure!(
        codec::u16_at(bytes, p + 8)? as usize == count.min(65535)
            && codec::u16_at(bytes, p + 10)? as usize == count.min(65535)
            && (recorded_size as usize == directory_bytes || zip64 && recorded_size == u32::MAX)
            && (recorded_offset as usize == central || zip64 && recorded_offset == u32::MAX),
        "Inconsistent ZIP directory"
    );
    ensure!(count <= 65535 || zip64, "Missing ZIP64 member counts");
    let end = p + 22 + codec::u16_at(bytes, p + 20)? as usize;
    ensure!(end <= bytes.len(), "Truncated ZIP comment");
    Ok(end)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    fn fixture(count: usize) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for i in 0..count {
            writer
                .start_file(
                    format!("member{i}.bin"),
                    zip::write::SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Stored),
                )
                .unwrap();
            writer.write_all(b"authored").unwrap();
        }
        writer.finish().unwrap().into_inner()
    }
    #[test]
    fn zip64_counts_and_exact_boundary_are_verified() {
        let source = fixture(100001);
        let n = source.len();
        let mut padded = source.clone();
        padded.extend_from_slice(b"PK\x05\x06fake padding");
        assert_eq!(boundary(&padded).unwrap(), n);
        let mut archive = Archive::open(source.clone()).unwrap();
        assert_eq!(archive.validate(&AtomicBool::new(false)).unwrap(), 100001);
        assert_eq!(archive.into_bytes(), source);
        let mut sentinels = source.clone();
        sentinels[n - 22 + 12..n - 22 + 20].fill(0xff);
        assert_eq!(boundary(&sentinels).unwrap(), n);
        // Never accept a contradictory ZIP64 count, locator, or truncated EOCD.
        let zip64 = n - 22 - 20 - 56;
        for offset in [
            zip64 + 24,
            zip64 + 32,
            zip64 + 40,
            zip64 + 48,
            n - 22 - 20 + 8,
        ] {
            let mut corrupt = source.clone();
            corrupt[offset] ^= 1;
            assert!(boundary(&corrupt).is_err());
        }
        assert!(boundary(&source[..n - 1]).is_err());
    }
    #[test]
    fn streaming_validation_checks_crc_paths_and_cancel() {
        let bytes = fixture(1);
        assert_eq!(
            boundary(&bytes).unwrap(),
            codec::zip_length(&bytes).unwrap()
        );
        let mut corrupt = bytes.clone();
        let payload = 30
            + codec::u16_at(&corrupt, 26).unwrap() as usize
            + codec::u16_at(&corrupt, 28).unwrap() as usize;
        corrupt[payload] ^= 1;
        assert!(
            Archive::open(corrupt)
                .unwrap()
                .validate(&AtomicBool::new(false))
                .is_err()
        );
        assert!(
            Archive::open(bytes)
                .unwrap()
                .validate(&AtomicBool::new(true))
                .is_err()
        );
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file("../unsafe.bin", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"original").unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        assert!(Archive::open(bytes).is_err());
    }
}
