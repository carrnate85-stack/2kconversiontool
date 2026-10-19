//! Read-only installed-game selector resources. The native Oodle ABI and exact
//! VCZ trailer policy are reused from shared/nba2k-assets/src/codec.rs and the
//! exporter's buffer_decode.rs; no .NET/Node helper or game-memory access.
#![allow(dead_code)]
use crate::{loader_files::Directories, selector_tables, sigconfig};
use anyhow::{Context, Result, ensure};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{BufRead, BufReader, Read},
    os::windows::fs::{FileExt, MetadataExt, OpenOptionsExt},
    path::Path,
    sync::OnceLock,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Resource {
    pub archive: String,
    pub offset: u64,
    pub size: u64,
}
pub(crate) struct Manifest {
    pub entries: [Resource; 3],
    pub bases: BTreeSet<u32>,
    pub configs: Vec<(u32, BTreeSet<String>)>,
}
impl Manifest {
    pub fn read(root: &Path, check: impl Fn() -> Result<()>) -> Result<Self> {
        let _pins = Directories::pin(root)?;
        let file = open_plain(&root.join("manifest"))?;
        let before = file.metadata()?;
        ensure!(
            before.len() <= 256 * 1024 * 1024,
            "Installed asset manifest exceeds bounds"
        );
        let result = Self::parse(BufReader::new(&file), &check)?;
        let after = file.metadata()?;
        ensure!(
            before.len() == after.len() && before.last_write_time() == after.last_write_time(),
            "Installed asset manifest changed during inspection"
        );
        Ok(result)
    }
    fn parse(mut reader: impl BufRead, check: impl Fn() -> Result<()>) -> Result<Self> {
        static PATTERN: OnceLock<Regex> = OnceLock::new();
        let pattern = PATTERN.get_or_init(|| {
            Regex::new(r"^char/sig/png([0-9]{5})(?:_config_([a-z0-9_-]+))?\.iff$").unwrap()
        });
        let mut found: [Option<Resource>; 3] = [None, None, None];
        let mut bases = BTreeSet::new();
        let mut configs: Vec<(u32, BTreeSet<String>)> = Vec::new();
        let mut by_id = BTreeMap::new();
        let mut buffer = Vec::new();
        let mut total = 0usize;
        for row in 0..2_000_000 {
            // Read-only manifest work; retain a bounded cancellation checkpoint
            // without a full process scan every ~100 KB. Publication checks
            // remain separate and fresh at each filesystem mutation.
            if row % 16384 == 0 {
                check()?;
            }
            buffer.clear();
            let count = reader.by_ref().take(8193).read_until(b'\n', &mut buffer)?;
            if count == 0 {
                check()?;
                let entries = found
                    .into_iter()
                    .collect::<Option<Vec<_>>>()
                    .context("Stock player selector resources are missing from manifest")?;
                return Ok(Self {
                    entries: entries.try_into().unwrap(),
                    bases,
                    configs,
                });
            }
            total += count;
            ensure!(
                count <= 8192 && total <= 256 * 1024 * 1024,
                "Installed manifest row/size limit exceeded"
            );
            let line = std::str::from_utf8(&buffer)?.trim_end_matches(['\r', '\n']);
            let fields = line.split(',').collect::<Vec<_>>();
            if fields.len() != 4 {
                continue;
            }
            let name = fields[0].replace('\\', "/").to_ascii_lowercase();
            if let Some(index) = selector_tables::PATHS.iter().position(|s| *s == name) {
                let resource = Resource {
                    archive: fields[1].into(),
                    offset: fields[2].parse()?,
                    size: fields[3].parse()?,
                };
                resource.validate()?;
                ensure!(
                    found[index].replace(resource).is_none(),
                    "Duplicate stock player selector manifest entry"
                );
            }
            if let Some(capture) = pattern.captures(&name) {
                let id = capture[1].parse()?;
                if let Some(suffix) = capture.get(2) {
                    let index = *by_id.entry(id).or_insert_with(|| {
                        configs.push((id, BTreeSet::new()));
                        configs.len() - 1
                    });
                    configs[index].1.insert(suffix.as_str().to_owned());
                } else {
                    bases.insert(id);
                }
            }
        }
        anyhow::bail!("Installed manifest exceeds the row limit")
    }
}
fn open_plain(path: &Path) -> Result<File> {
    let file = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .custom_flags(0x00200000)
        .open(path)?;
    let meta = file.metadata()?;
    ensure!(
        meta.is_file() && meta.file_attributes() & 0x400 == 0,
        "Installed selector resource is not an ordinary file"
    );
    Ok(file)
}
impl Resource {
    /// The camera name route may read only this fixed localization member;
    /// this is not a renderer-supplied arbitrary archive lookup.
    pub(crate) fn camera_localization(root: &Path, check: impl Fn() -> Result<()>) -> Result<Self> {
        let _pins = Directories::pin(root)?;
        let file = open_plain(&root.join("manifest"))?;
        let before = file.metadata()?;
        ensure!(
            before.len() <= 256 * 1024 * 1024,
            "Installed asset manifest exceeds bounds"
        );
        let result = Self::parse_camera_manifest(BufReader::new(&file), &check)?;
        let after = file.metadata()?;
        ensure!(
            before.len() == after.len() && before.last_write_time() == after.last_write_time(),
            "Installed asset manifest changed during camera inspection"
        );
        check()?;
        Ok(result)
    }
    fn parse_camera_manifest(
        mut reader: impl BufRead,
        check: impl Fn() -> Result<()>,
    ) -> Result<Self> {
        let mut buffer = Vec::new();
        let mut found = None;
        let mut total = 0usize;
        for row in 0..2_000_000 {
            if row % 16384 == 0 {
                check()?;
            }
            buffer.clear();
            let count = reader.by_ref().take(8193).read_until(b'\n', &mut buffer)?;
            if count == 0 {
                check()?;
                return found.context("Stock English localization resource is missing");
            }
            total = total
                .checked_add(count)
                .context("Asset manifest size overflow")?;
            ensure!(
                count <= 8192 && total <= 256 * 1024 * 1024,
                "Installed manifest row/size limit exceeded"
            );
            let line = std::str::from_utf8(&buffer)?.trim_end_matches(['\r', '\n']);
            let fields: Vec<_> = line.split(',').collect();
            if fields.len() == 4
                && fields[0]
                    .replace('\\', "/")
                    .eq_ignore_ascii_case("loc/english.iff")
            {
                let resource = Self {
                    archive: fields[1].into(),
                    offset: fields[2].parse()?,
                    size: fields[3].parse()?,
                };
                resource.validate()?;
                ensure!(
                    found.replace(resource).is_none(),
                    "Duplicate English localization manifest entry"
                );
            }
        }
        anyhow::bail!("Installed manifest exceeds row limit")
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.archive.len() == 2
                && self.archive.bytes().all(|b| b.is_ascii_alphanumeric())
                && (24..=sigconfig::MAX_BYTES as u64).contains(&self.size)
                && self.offset.checked_add(self.size).is_some(),
            "Invalid stock selector archive range"
        );
        Ok(())
    }
    pub fn packed(&self, root: &Path, check: impl Fn() -> Result<()>) -> Result<Vec<u8>> {
        self.validate()?;
        check()?;
        let _pin = Directories::pin(root)?;
        let file = open_plain(&root.join(&self.archive))?;
        let before = file.metadata()?;
        ensure!(
            self.offset <= before.len() && self.size <= before.len() - self.offset,
            "Stock selector range exceeds archive bounds"
        );
        let mut bytes = vec![0; self.size as usize];
        let mut read = 0;
        while read < bytes.len() {
            check()?;
            let end = bytes.len().min(read + 1024 * 1024);
            let count = file.seek_read(&mut bytes[read..end], self.offset + read as u64)?;
            ensure!(count > 0, "Stock selector resource ended unexpectedly");
            read += count;
        }
        let after = file.metadata()?;
        ensure!(
            before.len() == after.len() && before.last_write_time() == after.last_write_time(),
            "Stock selector archive changed during read"
        );
        Ok(bytes)
    }
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
pub(crate) struct Decoder {
    _library: libloading::Library,
    decode: Decode,
    _pins: Directories,
    _file: File,
}
impl Decoder {
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join("data/oodle/oo2core_9_win64.dll");
        let pins = Directories::pin(path.parent().unwrap())?;
        let file = open_plain(&path)?;
        let path = path.canonicalize()?;
        // Use only this absolute, pinned installed-game DLL. Dependency search
        // is limited to its directory and System32, never the current directory.
        let library: libloading::Library = unsafe {
            libloading::os::windows::Library::load_with_flags(
                path.as_os_str(),
                0x00000100 | 0x00000800,
            )?
        }
        .into();
        let decode = *unsafe { library.get::<Decode>(b"OodleLZ_Decompress\0") }?;
        Ok(Self {
            _library: library,
            decode,
            _pins: pins,
            _file: file,
        })
    }
    pub fn decode(&self, bytes: &[u8], check: impl Fn() -> Result<()>) -> Result<Vec<u8>> {
        decode_with(bytes, check, |input, output| {
            // Same fuzz-safe, single-thread-phase call used by the native
            // exporter. The exact authored size/CRC are checked independently.
            unsafe {
                (self.decode)(
                    input.as_ptr(),
                    input.len() as i64,
                    output.as_mut_ptr(),
                    output.len() as i64,
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
            }
        })
    }
}
pub(crate) fn verify_decoded(packed: &[u8], decoded: &[u8]) -> Result<()> {
    ensure!(
        (25..=sigconfig::MAX_BYTES).contains(&packed.len())
            && packed[..4] == [0x1f, 0x8b, 0x21, 4]
            && packed[10..12] == [4, 0]
            && packed[12..16] == *b"VCZ\0",
        "Missing authored selector VCZ trailer"
    );
    let end = packed.len();
    let size = u32::from_le_bytes(packed[end - 4..].try_into()?) as usize;
    let crc = u32::from_le_bytes(packed[end - 8..end - 4].try_into()?);
    ensure!(
        (22..=sigconfig::MAX_BYTES).contains(&size)
            && decoded.len() == size
            && crc32fast::hash(decoded) == crc,
        "Cached stock selector does not match the installed archive's authored length/CRC"
    );
    selector_tables::archive(decoded)?;
    Ok(())
}
fn decode_with(
    bytes: &[u8],
    check: impl Fn() -> Result<()>,
    decode: impl FnOnce(&[u8], &mut [u8]) -> i64,
) -> Result<Vec<u8>> {
    check()?;
    ensure!(
        (25..=sigconfig::MAX_BYTES).contains(&bytes.len())
            && bytes[..4] == [0x1f, 0x8b, 0x21, 4]
            && bytes[10..12] == [4, 0]
            && bytes[12..16] == *b"VCZ\0",
        "Stock selector requires an authored VCZ length/CRC trailer"
    );
    let end = bytes.len();
    let size = u32::from_le_bytes(bytes[end - 4..].try_into()?) as usize;
    let crc = u32::from_le_bytes(bytes[end - 8..end - 4].try_into()?);
    ensure!(
        (22..=sigconfig::MAX_BYTES).contains(&size),
        "Stock selector decoded size exceeds bounds"
    );
    let mut output = vec![0; size];
    let count = decode(&bytes[16..end - 8], &mut output);
    check()?;
    ensure!(
        count == size as i64 && crc32fast::hash(&output) == crc,
        "Stock selector decoded length/CRC differs from authored trailer"
    );
    selector_tables::archive(&output)
        .context("Stock selector is not a complete CRC-verified IFF archive")?;
    Ok(output)
}
pub(crate) fn fingerprint(packed: &[Vec<u8>; 3]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"nba2k27-stock-selector-exact-v1\0");
    for (path, bytes) in selector_tables::PATHS.iter().zip(packed) {
        digest.update(path.as_bytes());
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
    }
    format!("{:X}", digest.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    fn manifest() -> String {
        "char/signatureconfigs_all.iff,0C,12,25\nchar/signatureconfigs_editplayer.iff,0C,37,25\nchar/fpplayer_global.iff,0C,62,25\nchar/sig/png04090.iff,0C,1,32\nchar/sig/png03000_config_AFRO.iff,0C,1,32\nchar/sig/png02000_config_AFRO.iff,0C,1,32\nchar/sig/png03000_config_HEADBAND.iff,0C,1,32\n".into()
    }
    #[test]
    fn manifest_read_batches_keep_final_refusal_and_late_duplicate_checks() {
        use std::cell::Cell;
        let text = "unrelated,0A,0,24\n".repeat(16384) + "loc/english.iff,0A,1,24\n";
        let calls = Cell::new(0);
        Resource::parse_camera_manifest(Cursor::new(&text), || {
            calls.set(calls.get() + 1);
            Ok(())
        })
        .unwrap();
        assert_eq!(calls.get(), 3);
        calls.set(0);
        assert!(
            Resource::parse_camera_manifest(Cursor::new(&text), || {
                calls.set(calls.get() + 1);
                ensure!(calls.get() != 3, "final cancellation");
                Ok(())
            })
            .is_err()
        );
        assert!(
            Resource::parse_camera_manifest(
                Cursor::new(text + "loc/english.iff,0A,2,24\n"),
                || Ok(())
            )
            .is_err()
        );
        let calls = Cell::new(0);
        assert!(
            Manifest::parse(Cursor::new(manifest()), || {
                calls.set(calls.get() + 1);
                ensure!(calls.get() != 2, "final cancellation");
                Ok(())
            })
            .is_err()
        );
    }
    #[test]
    fn camera_manifest_lookup_is_fixed_bounded_unique_and_cancellable() {
        let valid = b"unrelated,0A,0,24\nloc/english.iff,0A,57138912,3645021\n";
        let r = Resource::parse_camera_manifest(Cursor::new(valid), || Ok(())).unwrap();
        assert_eq!(r.archive, "0A");
        assert_eq!(r.offset, 57138912);
        assert_eq!(r.size, 3645021);
        for bytes in [
            b"loc/other.iff,0A,1,24\n".to_vec(),
            b"loc/english.iff,../0A,1,24\n".to_vec(),
            b"loc/english.iff,0A,1,0\n".to_vec(),
            b"loc/english.iff,0A,18446744073709551615,24\n".to_vec(),
            [valid.as_slice(), valid.as_slice()].concat(),
            vec![b'x'; 8193],
            vec![255],
        ] {
            assert!(Resource::parse_camera_manifest(Cursor::new(bytes), || Ok(())).is_err());
        }
        assert!(
            Resource::parse_camera_manifest(Cursor::new(valid), || anyhow::bail!("cancelled"))
                .is_err()
        );
    }
    #[test]
    fn manifest_is_bounded_and_preserves_first_stock_style_set_order() {
        let source = manifest();
        let parsed = Manifest::parse(Cursor::new(source.as_bytes()), || Ok(())).unwrap();
        assert_eq!(parsed.entries[0].offset, 12);
        assert!(parsed.bases.contains(&4090));
        assert_eq!(
            parsed.configs.iter().map(|p| p.0).collect::<Vec<_>>(),
            [3000, 2000]
        );
        assert_eq!(
            parsed.configs[0].1,
            BTreeSet::from(["afro".into(), "headband".into()])
        );
        assert!(Manifest::parse(Cursor::new(source.repeat(2)), || Ok(())).is_err());
        assert!(Manifest::parse(Cursor::new("x".repeat(8193)), || Ok(())).is_err());
        assert!(Manifest::parse(Cursor::new(source), || anyhow::bail!("closed")).is_err());
    }
    #[test]
    fn exact_trailer_size_crc_and_cancellation_gate_the_native_call() {
        let raw = crate::selector_tables::tests::bundle()[0].clone();
        let mut packed = vec![0; 16];
        packed[..4].copy_from_slice(&[0x1f, 0x8b, 0x21, 4]);
        packed[10] = 4;
        packed[12..].copy_from_slice(b"VCZ\0");
        packed.push(42);
        packed.extend(crc32fast::hash(&raw).to_le_bytes());
        packed.extend((raw.len() as u32).to_le_bytes());
        verify_decoded(&packed, &raw).unwrap();
        let mut damaged = raw.clone();
        damaged[0] ^= 1;
        assert!(verify_decoded(&packed, &damaged).is_err());
        assert!(verify_decoded(&packed, &raw[..raw.len() - 1]).is_err());
        assert_eq!(
            decode_with(
                &packed,
                || Ok(()),
                |input, output| {
                    assert_eq!(input, [42]);
                    output.copy_from_slice(&raw);
                    raw.len() as i64
                }
            )
            .unwrap(),
            raw
        );
        let calls = std::cell::Cell::new(0);
        for offset in [0, 10, 12, packed.len() - 1] {
            let mut bad = packed.clone();
            bad[offset] ^= 0x40;
            assert!(
                decode_with(
                    &bad,
                    || Ok(()),
                    |_, _| {
                        calls.set(calls.get() + 1);
                        0
                    }
                )
                .is_err()
            );
        }
        assert_eq!(calls.get(), 0);
        assert!(
            decode_with(
                &packed,
                || anyhow::bail!("closed"),
                |_, _| panic!("must not decode after cancellation")
            )
            .is_err()
        );
        assert!(
            decode_with(
                &packed,
                || Ok(()),
                |_, output| {
                    output.copy_from_slice(&raw);
                    0
                }
            )
            .is_err()
        );
        let mut bad = packed;
        let at = bad.len() - 8;
        bad[at] ^= 1;
        assert!(
            decode_with(
                &bad,
                || Ok(()),
                |_, output| {
                    output.copy_from_slice(&raw);
                    raw.len() as i64
                }
            )
            .is_err()
        );
    }
    #[test]
    fn invalid_archive_ranges_do_not_escape_the_selected_game() {
        for resource in [
            Resource {
                archive: "../outside".into(),
                offset: 0,
                size: 32,
            },
            Resource {
                archive: "0C".into(),
                offset: u64::MAX,
                size: 32,
            },
            Resource {
                archive: "0C".into(),
                offset: 0,
                size: 0,
            },
        ] {
            assert!(resource.validate().is_err());
        }
        assert_ne!(
            fingerprint(&[vec![1], vec![2], vec![3]]),
            fingerprint(&[vec![1], vec![2], vec![4]])
        );
    }
    #[test]
    #[ignore = "Explicit read-only installed-game selector decode audit; not part of ordinary isolated tests"]
    fn installed_stock_selector_decode_audit() {
        let root =
            std::path::PathBuf::from(std::env::var("NBA2K27_STOCK_SELECTOR_AUDIT_ROOT").unwrap());
        let manifest = Manifest::read(&root, || Ok(())).unwrap();
        let decoder = Decoder::load(&root).unwrap();
        let mut decoded = Vec::new();
        for (name, entry) in selector_tables::PATHS.iter().zip(&manifest.entries) {
            let packed = entry.packed(&root, || Ok(())).unwrap();
            let bytes = decoder.decode(&packed, || Ok(())).unwrap();
            println!(
                "{}",
                serde_json::json!({"name":name,"packedBytes":packed.len(),"decodedBytes":bytes.len(),"sha256":format!("{:X}",Sha256::digest(&bytes)),"stockFileChanged":false,"gameMemoryAccess":false})
            );
            assert_eq!(entry.packed(&root, || Ok(())).unwrap(), packed);
            decoded.push(bytes);
        }
        let prepared = selector_tables::Prepared::existing(decoded.try_into().unwrap()).unwrap();
        println!("{}", serde_json::to_string(&prepared.summary).unwrap());
    }
}
