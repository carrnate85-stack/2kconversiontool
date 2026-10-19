//! Read-only clothing preview resources: the Rust port of WPF's
//! Nba2kFastFitDecoder.exe (nba2k27-electron-exporter Nba2kVczDecoder.cs,
//! full-export mode) used by fast-clothing-preview-worker.js and
//! clothing-thumbnails.js. One manifest member is read from the selected
//! game's archives, VCZ/Oodle-decoded with the installed oo2core DLL, TLD
//! textures become DDS, binary SCNE/TXTR become text, .gz buffers become .bin
//! and external SCNE dependencies are packed, exactly like the C# WriteExport.
//! The result is returned as an uncompressed ZIP for the reused WPF JS
//! (vendor/archive-preview iff.js/preview.js). No game file is ever written,
//! no game memory is touched, and only an allow-listed name set is readable.
use anyhow::{Context, Result, bail, ensure};
use regex::Regex;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs::{File, OpenOptions},
    io::{Cursor, Read},
    os::windows::fs::{FileExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::UNIX_EPOCH,
};

/// WPF fast-clothing-preview-worker.js manifest filter and size limit.
pub const MAX_PACKED: u64 = 32 * 1024 * 1024;
/// WPF: "Preview resource is too large." above a 64 MiB decoder output file.
const MAX_OUTPUT: usize = 64 * 1024 * 1024;
/// Decoded container/texture working budget (the C# decoder only bounded
/// these by Int32; animated texture arrays reach ~150 MiB).
const MAX_WORK: usize = 256 * 1024 * 1024;
/// preview.js never reads a DDS member above 16 MiB (gallery skips it), so
/// only those are deflated, keeping the output within WPF's 64 MiB file limit.
const STORE_LIMIT: usize = 16 * 1024 * 1024;
const MAX_RAW: i64 = 1 << 30;
const VCZ_HEADER: usize = 16;
/// Oodle raw step (C# OodleBlockSize).
const OODLE_BLOCK: usize = 256 * 1024;

/// Player editor artwork (WPF roster-artwork.js / badge-icons.js inputs).
pub fn artwork_allowed(name: &str) -> bool {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        Regex::new(r"^(?:player_images/chr_(?:g[0-9]{4}|r[0-9]{4}_a1)\.iff|logos/logo[a-z0-9_]{1,90}\.iff|badgeicons\.iff)$").unwrap()
    });
    name.len() <= 120 && pattern.is_match(name)
}
/// WPF animation-preview profile assets (raw decode only; 1 GiB packed cap,
/// nba_animations.iff is ~410 MB packed).
const MAX_ANIMATION_PACKED: u64 = 1024 * 1024 * 1024;
pub fn animation_allowed(name: &str) -> bool {
    matches!(
        name,
        "animations/nba_animations.iff"
            | "animations/animations_pregame_single.iff"
            | "animations/animations_pregame_multi.iff"
            | "animations/animations_blacktop.iff"
            | "animations/animations_slamball.iff"
            | "ai_metadata/sigtable.iff"
            | "ai_metadata/shotdata.iff"
            | "localbuildglobal.iff"
    )
}
pub fn allowed(name: &str) -> bool {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        Regex::new(concat!(
            r"^(?:clothing/[a-z0-9_]+(?:/[a-z0-9_]+){0,3}\.iff",
            r"|char/sig/png04090\.iff",
            r"|shoes/shoe_global\.iff",
            r"|shoes/shoe_(?:style|ed_region|fe)_[0-9]{4}\.iff",
            r"|store/thumbnails/[A-Za-z0-9_]{1,160}\.iff)$"
        ))
        .unwrap()
    });
    name.len() <= 240 && pattern.is_match(name)
}

#[derive(Clone)]
struct Entry {
    name: Box<str>,
    archive: [u8; 2],
    offset: u64,
    size: u64,
}
struct Index {
    entries: Vec<Entry>,
    exact: HashMap<String, u32>,
    leaf: HashMap<String, u32>,
    hash: HashMap<String, u32>,
    logical_mip: HashMap<String, u32>,
}
fn leaf_of(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}
/// C# ContentId: the 16-hex content hash in `name.<hash>.<ext>`.
fn content_id(file: &str) -> Option<(usize, usize)> {
    let extension = file.rfind('.')?;
    if extension == 0 {
        return None;
    }
    let separator = file[..extension].rfind('.')?;
    let hash = &file[separator + 1..extension];
    (hash.len() == 16 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
        .then_some((separator, extension))
}
fn content_hash_key(file: &str) -> Option<String> {
    let (separator, extension) = content_id(file)?;
    Some(format!("{}{}", &file[separator + 1..extension], &file[extension..]).to_lowercase())
}
fn logical_mip_key(file: &str) -> Option<String> {
    if !file.to_lowercase().ends_with(".mip0") {
        return None;
    }
    let (separator, _) = content_id(file)?;
    (separator > 0).then(|| file[..separator].to_lowercase())
}
impl Index {
    fn parse(text: &str) -> Result<Self> {
        let mut index = Index {
            entries: Vec::new(),
            exact: HashMap::new(),
            leaf: HashMap::new(),
            hash: HashMap::new(),
            logical_mip: HashMap::new(),
        };
        for line in text.lines() {
            let parts: Vec<_> = line.rsplitn(4, ',').collect();
            if parts.len() != 4 {
                continue;
            }
            let (Ok(size), Ok(offset)) = (parts[0].parse::<u64>(), parts[1].parse::<u64>()) else {
                continue;
            };
            let archive = parts[2].as_bytes();
            if archive.len() != 2 || !archive.iter().all(|b| b.is_ascii_alphanumeric()) || size == 0
            {
                continue;
            }
            ensure!(
                index.entries.len() < 4_000_000,
                "Installed manifest exceeds row limit"
            );
            let name = parts[3].replace('\\', "/");
            let id = index.entries.len() as u32;
            let file = leaf_of(&name).to_owned();
            index.exact.entry(name.to_lowercase()).or_insert(id);
            index.leaf.entry(file.to_lowercase()).or_insert(id);
            if let Some(hash) = content_hash_key(&file) {
                index.hash.entry(hash).or_insert(id);
            }
            if let Some(logical) = logical_mip_key(&file) {
                index
                    .logical_mip
                    .entry(format!("{logical}|{size}"))
                    .or_insert(id);
            }
            index.entries.push(Entry {
                name: name.into(),
                archive: [archive[0], archive[1]],
                offset,
                size,
            });
        }
        ensure!(
            !index.entries.is_empty(),
            "Installed manifest has no resources"
        );
        Ok(index)
    }
    fn exact(&self, name: &str) -> Option<&Entry> {
        self.exact
            .get(&name.replace('\\', "/").to_lowercase())
            .map(|i| &self.entries[*i as usize])
    }
    /// C# FindManifestResources: exact path, then leaf, then content hash.
    fn find(&self, reference: &str) -> Option<&Entry> {
        let file = leaf_of(reference);
        self.exact(reference)
            .or_else(|| {
                self.leaf
                    .get(&file.to_lowercase())
                    .map(|i| &self.entries[*i as usize])
            })
            .or_else(|| {
                content_hash_key(file)
                    .and_then(|h| self.hash.get(&h))
                    .map(|i| &self.entries[*i as usize])
            })
    }
}

type Step = unsafe extern "C" fn(*const u8, i64, i64, i64, *mut i64, *mut i32) -> i64;
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
type ReadHeader = unsafe extern "C" fn(*const u8, *mut i64, *mut i64) -> i64;
type Scratch = unsafe extern "C" fn(i64) -> i64;
type Bc7 = unsafe extern "C" fn(*mut u8, i64, *const u8, i64, *const u8, i32, *mut u8, i64) -> i64;
struct Oodle {
    _core: libloading::Library,
    _texture: libloading::Library,
    step: Step,
    decode: Decode,
    header: ReadHeader,
    scratch: Scratch,
    bc7: Bc7,
}
fn load_game_dll(path: &Path) -> Result<libloading::Library> {
    let file = open_plain(path)?;
    drop(file);
    let path = path.canonicalize()?;
    // Absolute installed-game DLL only; dependencies from its directory and
    // System32, never the current directory (same flags as stock_selectors).
    Ok(unsafe {
        libloading::os::windows::Library::load_with_flags(
            path.as_os_str(),
            0x00000100 | 0x00000800,
        )?
    }
    .into())
}
impl Oodle {
    fn load(root: &Path) -> Result<Self> {
        let core = load_game_dll(&root.join("data/oodle/oo2core_9_win64.dll"))
            .context("Could not load NBA 2K27's Oodle DLL")?;
        let texture = load_game_dll(&root.join("data/oodle/oo2texrt_9_win64.dll"))
            .context("Could not load NBA 2K27's Oodle Texture runtime")?;
        unsafe {
            Ok(Self {
                step: *core.get::<Step>(b"OodleLZ_GetCompressedStepForRawStep\0")?,
                decode: *core.get::<Decode>(b"OodleLZ_Decompress\0")?,
                header: *texture.get::<ReadHeader>(b"OodleTexRT_BC7Prep_ReadHeader\0")?,
                scratch: *texture.get::<Scratch>(b"OodleTexRT_BC7Prep_MinDecodeScratchSize\0")?,
                bc7: *texture.get::<Bc7>(b"OodleTexRT_BC7Prep_Decode\0")?,
                _core: core,
                _texture: texture,
            })
        }
    }
    fn decode_into(&self, input: &[u8], out: &mut [u8]) -> i64 {
        unsafe {
            (self.decode)(
                input.as_ptr(),
                input.len() as i64,
                out.as_mut_ptr(),
                out.len() as i64,
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
    }
    fn decode(&self, input: &[u8], n: usize) -> Result<(Vec<u8>, i64)> {
        ensure!(n > 0 && n as i64 <= MAX_RAW, "Invalid decoded size");
        let mut out = vec![0; n];
        let count = self.decode_into(input, &mut out);
        Ok((out, count))
    }
    fn step(&self, input: &[u8], raw: i64) -> (i64, i64) {
        let (mut end, mut independent) = (0i64, 0i32);
        let used = unsafe {
            (self.step)(
                input.as_ptr(),
                input.len() as i64,
                0,
                raw,
                &mut end,
                &mut independent,
            )
        };
        (used, end)
    }
    /// C# DecodeOodleBytes.
    fn exact(&self, source: &[u8], raw: usize) -> Result<Vec<u8>> {
        ensure!(
            !source.is_empty() && raw > 0 && raw <= MAX_WORK,
            "Invalid embedded Oodle segment range"
        );
        let (out, count) = self.decode(source, raw)?;
        ensure!(
            count == raw as i64,
            "Embedded Oodle segment decoded {count} bytes; expected {raw}"
        );
        Ok(out)
    }
    /// C# DecodeBc7Prepared via the installed oo2texrt runtime.
    fn bc7_prepared(&self, prepared: &[u8]) -> Result<Vec<u8>> {
        ensure!(
            prepared.len() >= 48,
            "BC7Prep stream is smaller than its header"
        );
        let (mut blocks, mut encoded) = (0i64, 0i64);
        let status = unsafe { (self.header)(prepared.as_ptr(), &mut blocks, &mut encoded) };
        ensure!(
            status == 0
                && blocks > 0
                && blocks <= (MAX_WORK / 16) as i64
                && encoded > 0
                && 48 + encoded <= prepared.len() as i64,
            "Invalid BC7Prep header (error {status})"
        );
        let scratch = unsafe { (self.scratch)(blocks) };
        ensure!(
            scratch > 0 && scratch <= i32::MAX as i64,
            "Invalid BC7Prep scratch size"
        );
        let mut work = vec![0u8; scratch as usize];
        let mut output = vec![0u8; blocks as usize * 16];
        let result = unsafe {
            (self.bc7)(
                output.as_mut_ptr(),
                output.len() as i64,
                prepared[48..].as_ptr(),
                encoded,
                prepared.as_ptr(),
                0,
                work.as_mut_ptr(),
                scratch,
            )
        };
        ensure!(
            result == output.len() as i64,
            "BC7Prep decode returned {result} bytes"
        );
        Ok(output)
    }
    /// VCZ container decode (shared/nba2k-assets Oodle::archive_preview, the
    /// Rust port of the C# Run boundary logic used by Archive Studio).
    fn archive(&self, bytes: &[u8], check: &dyn Fn() -> Result<()>) -> Result<Vec<u8>> {
        if bytes.get(12..16) != Some(b"VCZ\0") {
            zip_length(bytes).context("Resource is neither VCZ nor a ZIP/IFF")?;
            return Ok(bytes.to_vec());
        }
        let input = &bytes[VCZ_HEADER..];
        let (mut used, mut raw_end) = self.step(input, MAX_RAW);
        if used <= 0 || raw_end <= 0 {
            let (mut low, mut high, mut best_size, mut best_raw) = (1i64, (1 << 26) - 1, 0, 0);
            while low <= high {
                check()?;
                let candidate = low + (high - low) / 2;
                let (size, end) = self.step(input, candidate);
                if size > 0 && end > 0 {
                    (best_size, best_raw) = (size, end);
                    low = candidate + 1;
                } else {
                    high = candidate - 1;
                }
            }
            (used, raw_end) = (best_size, best_raw);
            if used <= 0 || raw_end <= 0 {
                let (mut low, mut high, mut best) = (1usize, 1 << 18, 0);
                while low <= high {
                    check()?;
                    let candidate = low + (high - low) / 2;
                    let (_, count) = self.decode(input, candidate)?;
                    if count == candidate as i64 {
                        best = candidate;
                        low = candidate + 1;
                    } else {
                        high = candidate - 1;
                    }
                }
                raw_end = best as i64;
                used = input.len() as i64;
            }
        }
        ensure!(
            used > 0 && used <= input.len() as i64 && raw_end > 0 && raw_end <= MAX_RAW,
            "Unsupported VCZ stream (boundary {used}/{}, raw {raw_end})",
            input.len()
        );
        check()?;
        let (raw, decoded) = self.decode(input, raw_end as usize)?;
        // Oodle returns the exact populated prefix for a final short block.
        if decoded > 0 && decoded < raw.len() as i64 {
            let prefix = &raw[..decoded as usize];
            if zip_length(prefix).ok() == Some(prefix.len()) {
                return Ok(prefix.to_vec());
            }
        }
        let exact = match zip_length(&raw) {
            Ok(exact) => exact,
            Err(error) => {
                // C# Run: when the rounded raw step decodes nothing usable,
                // decode one 256 KiB Oodle block less and infer the exact ZIP
                // end from that prefix (small store thumbnails need this).
                let prefix = raw_end as usize - (raw_end as usize).min(OODLE_BLOCK);
                if prefix > 0 {
                    let (bytes, count) = self.decode(&input[..used as usize], prefix)?;
                    if count == prefix as i64 {
                        if let Ok(exact) = zip_length(&bytes) {
                            let (out, count) = self.decode(&input[..used as usize], exact)?;
                            ensure!(
                                count == exact as i64 && zip_length(&out)? == exact,
                                "Native Oodle decode did not produce a complete ZIP/IFF"
                            );
                            return Ok(out);
                        }
                    }
                }
                if used < input.len() as i64 && input.len() as i64 - used > 16 {
                    if let Ok(recovered) = self.recover(input, raw_end as usize, check) {
                        return Ok(recovered);
                    }
                }
                return Err(error).context(format!(
                    "VCZ container boundary unavailable (decoded {decoded}, rounded {raw_end})"
                ));
            }
        };
        if exact == raw.len() {
            return Ok(raw);
        }
        let (out, count) = self.decode(input, exact)?;
        ensure!(
            count == exact as i64 && zip_length(&out)? == exact,
            "Native Oodle decode did not produce a complete ZIP/IFF"
        );
        Ok(out)
    }
    /// C# Nba2kVczDecoder.Run raw stage (used by the animation preview's large
    /// archives): decode only the step-reported compressed span, prefer the
    /// exact populated prefix, else infer the ZIP end (with the one-block-short
    /// prefix fallback). Clothing keeps archive() above unchanged.
    fn run(&self, bytes: &[u8], check: &dyn Fn() -> Result<()>) -> Result<Vec<u8>> {
        ensure!(
            bytes.get(12..16) == Some(b"VCZ\0"),
            "Animation source is not a VCZ stream"
        );
        let input = &bytes[VCZ_HEADER..];
        let (used, rounded) = self.step(input, MAX_RAW);
        let concatenated = used <= 0 || rounded <= 0 || rounded > i32::MAX as i64;
        check()?;
        let (mut raw, decoded) = if concatenated {
            let raw = self.concatenated(input, check)?;
            let n = raw.len() as i64;
            (raw, n)
        } else {
            ensure!(used as usize <= input.len(), "Invalid VCZ stream boundary");
            self.decode(&input[..used as usize], rounded as usize)?
        };
        let span = if concatenated {
            input
        } else {
            &input[..used as usize]
        };
        if !concatenated
            && decoded > 0
            && decoded < raw.len() as i64
            && infer_zip_length(&raw[..decoded as usize]).ok() == Some(decoded as usize)
        {
            raw.truncate(decoded as usize);
            return Ok(raw);
        }
        if decoded == raw.len() as i64 && infer_zip_length(&raw).ok() == Some(raw.len()) {
            return Ok(raw);
        }
        check()?;
        let exact = match infer_zip_length(&raw) {
            Ok(exact) => exact,
            Err(_) => {
                let prefix = raw
                    .len()
                    .checked_sub(1 << 18)
                    .filter(|p| *p > 0)
                    .context("VCZ stream is too small")?;
                let (out, count) = self.decode(span, prefix)?;
                ensure!(
                    count == prefix as i64,
                    "Could not decode the complete VCZ prefix."
                );
                infer_zip_length(&out).context("VCZ ZIP boundary inference failed")?
            }
        };
        ensure!(
            exact > 0 && exact <= raw.len(),
            "Inferred ZIP/IFF size is outside the VCZ stream."
        );
        if concatenated {
            raw.truncate(exact);
            return Ok(raw);
        }
        drop(raw);
        let (out, count) = self.decode(span, exact)?;
        ensure!(
            count == exact as i64 && infer_zip_length(&out)? == exact,
            "Native Oodle decode did not produce a complete ZIP/IFF."
        );
        Ok(out)
    }
    /// C# DecodeConcatenatedVcz, as Nba2kVczDecoder.Run uses it.
    fn concatenated(&self, input: &[u8], check: &dyn Fn() -> Result<()>) -> Result<Vec<u8>> {
        const CHUNK: i64 = 1 << 26;
        let mut chunks: Vec<(usize, usize)> = Vec::new();
        let (mut cursor, mut total) = (0usize, 0usize);
        while cursor < input.len() {
            check()?;
            let remaining = &input[cursor..];
            // Oodle's small safe-read tail after the final stream is not another stream.
            if remaining.len() <= 16 && !chunks.is_empty() {
                break;
            }
            let (mut packed, mut raw) = self.step(remaining, CHUNK);
            if packed <= 0 {
                let (mut low, mut high, mut best_raw, mut best_packed) = (1i64, CHUNK - 1, 0, 0);
                while low <= high {
                    check()?;
                    let candidate = low + (high - low) / 2;
                    let (consumed, end) = self.step(remaining, candidate);
                    if consumed > 0 && end > 0 {
                        (best_raw, best_packed) = (end, consumed);
                        low = candidate + 1;
                    } else {
                        high = candidate - 1;
                    }
                }
                if best_raw <= 0 || best_packed <= 0 {
                    best_packed = remaining.len() as i64;
                    let (mut low, mut high, mut best) = (1usize, 1usize << 18, 0usize);
                    while low <= high {
                        check()?;
                        let candidate = low + (high - low) / 2;
                        let (_, count) = self.decode(remaining, candidate)?;
                        if count == candidate as i64 {
                            best = candidate;
                            low = candidate + 1;
                        } else {
                            high = candidate - 1;
                        }
                    }
                    best_raw = best as i64;
                }
                (packed, raw) = (best_packed, best_raw);
            }
            ensure!(
                packed > 0 && packed as usize <= remaining.len() && raw > 0 && raw <= CHUNK,
                "Could not scan concatenated VCZ chunk {} at packed offset {cursor}",
                chunks.len()
            );
            chunks.push((cursor, raw as usize));
            cursor += packed as usize;
            total += raw as usize;
            ensure!(
                total <= i32::MAX as usize,
                "Concatenated VCZ output exceeds the decoder limit."
            );
        }
        ensure!(
            input.len() - cursor <= 16 && !chunks.is_empty(),
            "Concatenated VCZ chunks did not consume the packed stream."
        );
        let (output, combined) = self.decode(input, total)?;
        if combined == output.len() as i64 {
            return Ok(output);
        }
        if chunks.len() == 1 {
            if let Ok(size) = infer_zip_length(&output) {
                if size > 0 && size <= output.len() {
                    return Ok(output[..size].to_vec());
                }
            }
            if output.len() > 1 << 18 {
                let prefix = output.len() - (1 << 18);
                let (bytes, count) = self.decode(input, prefix)?;
                if count == prefix as i64 {
                    let exact = match infer_zip_length(&bytes) {
                        Ok(size) => size,
                        Err(_) => {
                            let (mut low, mut high, mut best) = (1usize, output.len() - 1, 0usize);
                            while low <= high {
                                check()?;
                                let candidate = low + (high - low) / 2;
                                if self.decode(input, candidate)?.1 == candidate as i64 {
                                    best = candidate;
                                    low = candidate + 1;
                                } else {
                                    high = candidate - 1;
                                }
                            }
                            best
                        }
                    };
                    if exact > 0 && exact <= output.len() {
                        let (bytes, count) = self.decode(input, exact)?;
                        if count == exact as i64 {
                            return Ok(bytes);
                        }
                    }
                }
            }
        }
        // WPF ran localbuildglobal.iff through --oodle-segment with a raw size
        // pinned to one game revision. Without that size, take the ZIP end from
        // the populated whole-stream buffer and require an exact decode of it.
        if chunks.len() > 1 {
            if let Ok(size) = infer_zip_length(&output) {
                drop(output);
                let (bytes, count) = self.decode(input, size)?;
                if count == size as i64 && infer_zip_length(&bytes).ok() == Some(size) {
                    return Ok(bytes);
                }
            }
        }
        let mut combined = Vec::with_capacity(total);
        for (start, size) in chunks {
            check()?;
            let (chunk, count) = self.decode(&input[start..], size)?;
            ensure!(
                count == size as i64,
                "Concatenated VCZ chunk decoded {count} bytes; expected {size}."
            );
            combined.extend_from_slice(&chunk);
        }
        Ok(combined)
    }
    /// Concatenated VCZ chunks (C# DecodeConcatenatedVcz / shared
    /// recover_preview): every chunk decoded, ZIP records must validate.
    fn recover(
        &self,
        input: &[u8],
        boundary: usize,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<Vec<u8>> {
        let limit = boundary + (1 << 18);
        let mut cursor = 0usize;
        let mut chunks = Vec::new();
        let mut total = 0usize;
        while cursor < input.len() {
            check()?;
            let remaining = &input[cursor..];
            if remaining.len() <= 16 && !chunks.is_empty() {
                break;
            }
            let (mut packed, mut raw) = self.step(remaining, 1 << 26);
            if packed <= 0 || raw <= 0 {
                let (mut low, mut high, mut best_raw, mut best_packed) =
                    (1i64, (1 << 26) - 1, 0, 0);
                while low <= high {
                    check()?;
                    let candidate = low + (high - low) / 2;
                    let (consumed, end) = self.step(remaining, candidate);
                    if consumed > 0 && end > 0 {
                        (best_raw, best_packed) = (end, consumed);
                        low = candidate + 1;
                    } else {
                        high = candidate - 1;
                    }
                }
                (raw, packed) = (best_raw, best_packed);
            }
            ensure!(
                packed > 0 && packed <= remaining.len() as i64 && raw > 0 && raw <= 1 << 26,
                "Invalid VCZ chunk"
            );
            total += raw as usize;
            ensure!(total <= limit, "VCZ chunks exceed the container window");
            chunks.push((cursor, raw as usize));
            cursor += packed as usize;
        }
        ensure!(
            !chunks.is_empty() && input.len() - cursor <= 16,
            "VCZ chunks did not consume stream"
        );
        let mut combined = Vec::with_capacity(total);
        for (start, size) in chunks {
            check()?;
            let (chunk, count) = self.decode(&input[start..], size)?;
            ensure!(count > 0 && count <= size as i64, "Incomplete VCZ chunk");
            combined.extend_from_slice(&chunk[..count as usize]);
        }
        let exact = zip_length(&combined)?;
        combined.truncate(exact);
        Ok(combined)
    }
}

fn u16_at(b: &[u8], p: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        b.get(p..p + 2).context("Truncated u16")?.try_into()?,
    ))
}
fn u32_at(b: &[u8], p: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        b.get(p..p + 4).context("Truncated u32")?.try_into()?,
    ))
}
fn u64_at(b: &[u8], p: usize) -> Result<u64> {
    Ok(u64::from_le_bytes(
        b.get(p..p + 8).context("Truncated u64")?.try_into()?,
    ))
}
/// Verified ZIP/IFF end (shared codec::zip_length): local records, central
/// directory and end record must agree.
fn zip_length(bytes: &[u8]) -> Result<usize> {
    let mut p = 0;
    let mut locals = 0;
    while p + 30 <= bytes.len() && u32_at(bytes, p)? == 0x04034b50 {
        ensure!(
            u16_at(bytes, p + 6)? & 8 == 0,
            "Streaming ZIP not supported"
        );
        let size = u32_at(bytes, p + 18)?;
        ensure!(size != u32::MAX, "ZIP64 local record not supported");
        let next = p
            + 30
            + u16_at(bytes, p + 26)? as usize
            + u16_at(bytes, p + 28)? as usize
            + size as usize;
        ensure!(next <= bytes.len() && next > p, "Invalid local record");
        p = next;
        locals += 1;
    }
    ensure!(locals > 0, "No ZIP members");
    let central = p;
    let mut count = 0;
    while p + 46 <= bytes.len() && u32_at(bytes, p)? == 0x02014b50 {
        let next = p
            + 46
            + u16_at(bytes, p + 28)? as usize
            + u16_at(bytes, p + 30)? as usize
            + u16_at(bytes, p + 32)? as usize;
        ensure!(
            next <= bytes.len() && next > p,
            "Incomplete central directory"
        );
        p = next;
        count += 1;
    }
    ensure!(
        count == locals && p + 22 <= bytes.len() && u32_at(bytes, p)? == 0x06054b50,
        "Incomplete ZIP end record"
    );
    ensure!(
        u32_at(bytes, p + 12)? as usize == p - central
            && u32_at(bytes, p + 16)? as usize == central,
        "Inconsistent ZIP directory"
    );
    let end = p + 22 + u16_at(bytes, p + 20)? as usize;
    ensure!(end <= bytes.len(), "Truncated ZIP comment");
    Ok(end)
}

/// C# Nba2kVczDecoder.InferZipLength: like zip_length, plus the ZIP64 end
/// record nba_animations.iff needs (more than 65535 members).
fn infer_zip_length(bytes: &[u8]) -> Result<usize> {
    let mut p = 0;
    let mut locals = 0u64;
    while p + 30 <= bytes.len() && u32_at(bytes, p)? == 0x04034b50 {
        ensure!(
            u16_at(bytes, p + 6)? & 8 == 0,
            "Streaming ZIP not supported"
        );
        let size = u32_at(bytes, p + 18)?;
        ensure!(size != u32::MAX, "ZIP64 local record not supported");
        let next = p
            + 30
            + u16_at(bytes, p + 26)? as usize
            + u16_at(bytes, p + 28)? as usize
            + size as usize;
        ensure!(next <= bytes.len() && next > p, "Invalid local record");
        p = next;
        locals += 1;
    }
    ensure!(
        locals > 0 && p + 4 <= bytes.len() && u32_at(bytes, p)? == 0x02014b50,
        "No ZIP central directory"
    );
    let central = p;
    let mut count = 0u64;
    while p + 46 <= bytes.len() && u32_at(bytes, p)? == 0x02014b50 {
        ensure!(
            u32_at(bytes, p + 20)? != u32::MAX && u32_at(bytes, p + 42)? != u32::MAX,
            "ZIP64 members not supported"
        );
        let next = p
            + 46
            + u16_at(bytes, p + 28)? as usize
            + u16_at(bytes, p + 30)? as usize
            + u16_at(bytes, p + 32)? as usize;
        ensure!(
            next <= bytes.len() && next > p,
            "Incomplete central directory"
        );
        p = next;
        count += 1;
    }
    ensure!(count == locals, "Incomplete ZIP central directory");
    let central_size = (p - central) as u64;
    let zip64 = p + 4 <= bytes.len() && u32_at(bytes, p)? == 0x06064b50;
    if zip64 {
        let record = p;
        ensure!(p + 56 <= bytes.len(), "ZIP64 end record is truncated");
        let payload = u64_at(bytes, p + 4)?;
        ensure!(
            (44..=i32::MAX as u64).contains(&payload),
            "Unsupported ZIP64 end record"
        );
        let length = 12 + payload as usize;
        ensure!(
            p + length + 20 <= bytes.len(),
            "ZIP64 end record is truncated"
        );
        ensure!(
            u32_at(bytes, p + 16)? == 0
                && u32_at(bytes, p + 20)? == 0
                && u64_at(bytes, p + 24)? == count
                && u64_at(bytes, p + 32)? == count
                && u64_at(bytes, p + 40)? == central_size
                && u64_at(bytes, p + 48)? == central as u64,
            "ZIP64 central directory metadata is inconsistent"
        );
        p += length;
        ensure!(
            u32_at(bytes, p)? == 0x07064b50
                && u32_at(bytes, p + 4)? == 0
                && u64_at(bytes, p + 8)? == record as u64
                && u32_at(bytes, p + 16)? == 1,
            "ZIP64 end locator is inconsistent"
        );
        p += 20;
    }
    ensure!(
        p + 22 <= bytes.len() && u32_at(bytes, p)? == 0x06054b50,
        "Incomplete ZIP end record"
    );
    let (disk, central_disk) = (u16_at(bytes, p + 4)?, u16_at(bytes, p + 6)?);
    let (disk_entries, total) = (u16_at(bytes, p + 8)? as u64, u16_at(bytes, p + 10)? as u64);
    let (size, offset) = (u32_at(bytes, p + 12)?, u32_at(bytes, p + 16)?);
    let counts = if zip64 {
        disk_entries == 0xffff && total == 0xffff
    } else {
        disk_entries == count && total == count
    };
    let location = if zip64 {
        (offset == u32::MAX || offset as usize == central)
            && (size == u32::MAX || size as u64 == central_size)
    } else {
        offset as usize == central && size as u64 == central_size
    };
    ensure!(
        disk == 0 && central_disk == 0 && counts && location,
        "Inconsistent ZIP directory"
    );
    let end = p + 22 + u16_at(bytes, p + 20)? as usize;
    ensure!(end <= bytes.len(), "Truncated ZIP comment");
    Ok(end)
}

fn open_plain(path: &Path) -> Result<File> {
    // FILE_SHARE_READ | FILE_SHARE_WRITE (the game may hold archives open);
    // FILE_FLAG_OPEN_REPARSE_POINT so a link is refused, not followed.
    let file = OpenOptions::new()
        .read(true)
        .share_mode(3)
        .custom_flags(0x00200000)
        .open(path)
        .with_context(|| format!("A required game file is missing: {}", path.display()))?;
    let meta = file.metadata()?;
    ensure!(
        meta.is_file() && meta.file_attributes() & 0x400 == 0,
        "Game resource is not an ordinary file"
    );
    Ok(file)
}
fn stamp(meta: &std::fs::Metadata) -> String {
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("{}:{}:{}", meta.len(), modified, meta.creation_time())
}

pub struct Game {
    pub root: PathBuf,
    manifest_stamp: String,
    index: Index,
    oodle: OnceLock<std::result::Result<Oodle, String>>,
}
impl Game {
    fn oodle(&self) -> Result<&Oodle> {
        self.oodle
            .get_or_init(|| Oodle::load(&self.root).map_err(|e| format!("{e:#}")))
            .as_ref()
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
    fn read(&self, entry: &Entry) -> Result<(Vec<u8>, String)> {
        self.read_limited(entry, MAX_PACKED)
    }
    fn read_limited(&self, entry: &Entry, limit: u64) -> Result<(Vec<u8>, String)> {
        ensure!(entry.size <= limit, "Preview resource is too large.");
        let archive = std::str::from_utf8(&entry.archive)?;
        let file = open_plain(&self.root.join(archive))?;
        let before = file.metadata()?;
        ensure!(
            entry.offset <= before.len() && entry.size <= before.len() - entry.offset,
            "Preview archive range is invalid."
        );
        let mut bytes = vec![0u8; entry.size as usize];
        let mut read = 0;
        while read < bytes.len() {
            let count = file.seek_read(&mut bytes[read..], entry.offset + read as u64)?;
            ensure!(count > 0, "Manifest resource ended unexpectedly");
            read += count;
        }
        Ok((bytes, stamp(&before)))
    }
    /// Packed size of an allow-listed manifest member.
    pub fn size(&self, name: &str) -> Option<u64> {
        allowed(name)
            .then(|| self.index.exact(name).map(|e| e.size))
            .flatten()
    }
    /// WPF cacheFile(): sha256 of the manifest/archive identity.
    pub fn key(&self, name: &str) -> Result<String> {
        ensure!(allowed(name), "Preview resource is not allowed");
        self.key_member(name)
    }
    /// Player editor artwork equivalents of size()/key() (separate name set).
    pub fn artwork_size(&self, name: &str) -> Option<u64> {
        artwork_allowed(name)
            .then(|| self.index.exact(name).map(|e| e.size))
            .flatten()
    }
    pub fn artwork_key(&self, name: &str) -> Result<String> {
        ensure!(artwork_allowed(name), "Artwork resource is not allowed");
        self.key_member(name)
    }
    fn key_member(&self, name: &str) -> Result<String> {
        let entry = self
            .index
            .exact(name)
            .with_context(|| format!("Preview resource unavailable: {name}"))?;
        ensure!(entry.size <= MAX_PACKED, "Preview resource is too large.");
        let archive = std::str::from_utf8(&entry.archive)?;
        let file = open_plain(&self.root.join(archive))?;
        let meta = file.metadata()?;
        let identity = crate::file_identity::Stable::read(&file)?;
        ensure!(
            entry.offset + entry.size <= meta.len(),
            "Preview archive range is invalid."
        );
        let mut digest = Sha256::new();
        digest.update(format!(
            "fit-v2:{}:{}:{}:{}:{}:{}",
            self.manifest_stamp,
            name,
            archive,
            entry.offset,
            entry.size,
            stamp(&meta)
        ));
        digest.update(serde_json::to_vec(&identity)?);
        Ok(hex(&digest.finalize()))
    }
    /// Names matching a prefix, for WPF's manifest scans (thumbnails, shoes).
    pub fn names_with_prefix<'a>(
        &'a self,
        prefix: &'a str,
    ) -> impl Iterator<Item = (&'a str, u64)> + 'a {
        self.index
            .entries
            .iter()
            .filter(move |e| e.name.starts_with(prefix))
            .map(|e| (&*e.name, e.size))
    }
    /// Decode one allow-listed member to the same ZIP layout the WPF decoder
    /// wrote (members stored uncompressed).
    pub fn export(&self, name: &str, check: &dyn Fn() -> Result<()>) -> Result<Vec<u8>> {
        ensure!(allowed(name), "Preview resource is not allowed");
        self.export_member(name, check)
    }
    /// Player editor artwork (WPF roster-artwork.js / badge-icons.js inputs):
    /// a separate, fixed name set; never reachable through the preview route.
    pub fn export_artwork(&self, name: &str, check: &dyn Fn() -> Result<()>) -> Result<Vec<u8>> {
        ensure!(artwork_allowed(name), "Artwork resource is not allowed");
        self.export_member(name, check)
    }
    /// Player animation preview sources (WPF PlayerAnimationPreview
    /// revision.json): the plain VCZ/Oodle-decoded ZIP, exactly what
    /// Nba2kVczDecoder.exe wrote for these archives (no member rewriting).
    /// Returns the decoded bytes and the source archive stamp.
    pub fn export_animation_source(
        &self,
        name: &str,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<(Vec<u8>, String)> {
        ensure!(animation_allowed(name), "Animation resource is not allowed");
        let entry = self
            .index
            .exact(name)
            .with_context(|| format!("Animation resource unavailable: {name}"))?;
        let (packed, stamp) = self.read_limited(entry, MAX_ANIMATION_PACKED)?;
        check()?;
        let raw = self.oodle()?.run(&packed, check)?;
        drop(packed);
        Ok((raw, stamp))
    }
    /// Manifest row (archive, offset, packed size) of an animation source.
    pub fn animation_row(&self, name: &str) -> Option<(String, u64, u64)> {
        animation_allowed(name)
            .then(|| self.index.exact(name))
            .flatten()
            .map(|e| {
                (
                    String::from_utf8_lossy(&e.archive).into_owned(),
                    e.offset,
                    e.size,
                )
            })
    }
    fn export_member(&self, name: &str, check: &dyn Fn() -> Result<()>) -> Result<Vec<u8>> {
        let entry = self
            .index
            .exact(name)
            .with_context(|| format!("Preview resource unavailable: {name}"))?;
        let (packed, _) = self.read(entry)?;
        check()?;
        let oodle = self.oodle()?;
        let raw = oodle.archive(&packed, check)?;
        ensure!(raw.len() <= MAX_WORK, "Preview resource is too large.");
        check()?;
        let out = self.write_export(raw, check)?;
        ensure!(out.len() <= MAX_OUTPUT, "Preview resource is too large.");
        Ok(out)
    }
    fn dependency(&self, reference: &str) -> Result<Option<Vec<u8>>> {
        match self.index.find(reference) {
            Some(entry) => Ok(Some(self.read(entry)?.0)),
            None => Ok(None),
        }
    }
    /// C# WriteExport (previewTextures == null).
    fn write_export(&self, raw: Vec<u8>, check: &dyn Fn() -> Result<()>) -> Result<Vec<u8>> {
        let oodle = self.oodle()?;
        let mut zip = zip::ZipArchive::new(Cursor::new(raw))?;
        ensure!(zip.len() <= 100_000, "IFF has too many members");
        let mut names: Vec<String> = Vec::with_capacity(zip.len());
        let mut members: Vec<Vec<u8>> = Vec::with_capacity(zip.len());
        let mut total = 0usize;
        for i in 0..zip.len() {
            let mut member = zip.by_index(i)?;
            ensure!(
                !member.encrypted(),
                "Encrypted IFF members are not supported"
            );
            total = total.saturating_add(member.size() as usize);
            ensure!(total <= MAX_WORK, "Preview resource is too large.");
            let mut bytes = Vec::with_capacity(member.size() as usize);
            member.read_to_end(&mut bytes)?;
            names.push(member.name().replace('\\', "/"));
            members.push(bytes);
        }
        let lower = |s: &str| s.to_lowercase();
        let mut lookup: HashMap<String, usize> = HashMap::new();
        for (i, name) in names.iter().enumerate() {
            lookup.entry(lower(name)).or_insert(i);
        }
        for (i, name) in names.iter().enumerate() {
            lookup.entry(lower(leaf_of(name))).or_insert(i);
        }
        let ends = |name: &str, ext: &str| name.to_lowercase().ends_with(ext);
        let mut reserved: HashSet<String> = names
            .iter()
            .filter(|n| !ends(n, ".tld") && !ends(n, ".gz") && !ends(n, ".mip0"))
            .map(|n| lower(n))
            .collect();
        let mut converted: HashSet<String> = HashSet::new();
        let mut dependencies: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut info: HashMap<String, Info> = HashMap::new();
        let mut replacements: HashMap<usize, (String, Vec<u8>)> = HashMap::new();
        let mut skipped: HashSet<usize> = HashSet::new();
        for (i, name) in names.iter().enumerate() {
            check()?;
            if ends(name, ".tld") {
                if let Some(dds) = try_convert_tld(&members[i]) {
                    let dds_name = format!("{}.dds", &name[..name.len() - 4]);
                    ensure!(
                        !reserved.contains(&lower(&dds_name)),
                        "DDS output name collides with an existing IFF member: {dds_name}"
                    );
                    converted.insert(lower(name));
                    converted.insert(lower(leaf_of(name)));
                    reserved.insert(lower(&dds_name));
                    replacements.insert(i, (dds_name, dds));
                }
            } else if (ends(name, ".scne") || ends(name, ".txtr")) && is_tree(&members[i]) {
                let tree = Tree::parse(&members[i])?;
                let mut found = Vec::new();
                tree.collect(0, &mut found)?;
                for item in found {
                    if seen.insert(lower(&item.name)) {
                        dependencies.push(item.name.clone());
                    }
                    let key = lower(&item.name);
                    if info
                        .get(&key)
                        .is_none_or(|e| e.segments.len() < item.segments.len())
                    {
                        info.insert(key, item);
                    }
                }
            }
        }
        let mut prepared: Vec<(String, Vec<u8>)> = Vec::new();
        for dependency in &dependencies {
            check()?;
            let safe = safe_dependency_name(dependency)?;
            let file = leaf_of(&safe).to_owned();
            let embedded_index = lookup
                .get(&lower(&safe))
                .or_else(|| lookup.get(&lower(&file)))
                .copied();
            let source = match embedded_index {
                Some(i) => members[i].clone(),
                None => match self.dependency(dependency)? {
                    Some(bytes) => bytes,
                    None => continue,
                },
            };
            if ends(dependency, ".tld") {
                if converted.contains(&lower(dependency)) || converted.contains(&lower(&file)) {
                    continue;
                }
                let mut item = info
                    .get(&lower(dependency))
                    .with_context(|| format!("SCNE texture metadata is missing for {dependency}."))?
                    .clone();
                let mip_reference = format!("{}.mip0", &dependency[..dependency.len() - 4]);
                let mip_leaf = leaf_of(&mip_reference).to_owned();
                let embedded_mip = lookup
                    .get(&lower(&mip_reference))
                    .or_else(|| lookup.get(&lower(&mip_leaf)))
                    .copied();
                let mip = match embedded_mip {
                    Some(i) => Some(members[i].clone()),
                    None => self.mip0(&source, &item, &mip_reference)?,
                };
                match decode_segmented_tld(oodle, &source, mip.as_deref(), &mut item) {
                    Ok(dds) => {
                        let dds_name = format!("{}.dds", &safe[..safe.len() - 4]);
                        converted.insert(lower(dependency));
                        converted.insert(lower(&file));
                        if let Some(i) = embedded_index {
                            replacements.insert(i, (dds_name, dds));
                            if let Some(m) = embedded_mip {
                                skipped.insert(m);
                            }
                        } else {
                            prepared.push((dds_name, dds));
                        }
                    }
                    Err(_) => {
                        // C#: keep the unsupported TLD and its mip byte-for-byte.
                        if embedded_index.is_none() {
                            prepared.push((safe.clone(), source));
                        }
                        if embedded_mip.is_none() {
                            if let Some(mip) = mip {
                                prepared.push((mip_reference.clone(), mip));
                            }
                        }
                    }
                }
            } else if ends(dependency, ".gz") {
                let expected = info
                    .get(&lower(dependency))
                    .map(|i| i.expected_size)
                    .unwrap_or(0);
                ensure!(
                    expected > 0,
                    "SCNE buffer size is missing for {dependency}."
                );
                ensure!(
                    source.len() >= VCZ_HEADER && &source[12..16] == b"VCZ\0",
                    "Shared GZ buffer is not a VCZ stream: {dependency}."
                );
                let decoded = oodle.exact(&source[VCZ_HEADER..], expected)?;
                let bin = format!("{}.bin", &safe[..safe.len() - 3]);
                converted.insert(lower(dependency));
                converted.insert(lower(&file));
                match embedded_index {
                    Some(i) => {
                        replacements.insert(i, (bin, decoded));
                    }
                    None => prepared.push((bin, decoded)),
                }
            } else if embedded_index.is_none() {
                prepared.push((safe, source));
            }
            ensure!(
                prepared.iter().map(|p| p.1.len()).sum::<usize>() <= MAX_WORK,
                "Preview resource is too large."
            );
        }
        let mut writer = StoredZip::default();
        for (i, name) in names.iter().enumerate() {
            check()?;
            if skipped.contains(&i) {
                continue;
            }
            let (out_name, bytes) = match replacements.remove(&i) {
                Some((n, b)) => (n, b),
                None => (name.clone(), std::mem::take(&mut members[i])),
            };
            // The deployed WPF decoder converts binary SCNE only; TXTR stays
            // binary for preview.js textureTxtrMetadata (parseBinaryTree).
            let bytes = if ends(name, ".scne") && is_tree(&bytes) {
                scne_text(&bytes, &converted)?
            } else {
                bytes
            };
            writer.add(&out_name, &bytes)?;
        }
        for (name, bytes) in prepared {
            writer.add(&name, &bytes)?;
        }
        writer.finish()
    }
    /// External mip0 (exact/leaf/hash, then the logical-name + size index).
    fn mip0(&self, tld: &[u8], info: &Info, reference: &str) -> Result<Option<Vec<u8>>> {
        let inline_end = 32 + u32_at(tld, 28)? as u64;
        let required = info
            .segments
            .iter()
            .filter(|s| s.file_offset >= inline_end)
            .map(|s| s.file_offset - inline_end + s.file_size)
            .max()
            .unwrap_or(0);
        let mut entry = self.index.find(reference);
        if entry.is_none_or(|e| e.size < required) {
            entry = logical_mip_key(leaf_of(reference))
                .filter(|_| required > 0)
                .and_then(|k| self.index.logical_mip.get(&format!("{k}|{required}")))
                .map(|i| &self.index.entries[*i as usize]);
        }
        match entry {
            Some(e) => Ok(Some(self.read(e)?.0)),
            None => Ok(None),
        }
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn safe_dependency_name(reference: &str) -> Result<String> {
    let normalized = reference.replace('\\', "/");
    let normalized = normalized.trim_start_matches('/');
    for part in normalized.split('/') {
        ensure!(
            !part.is_empty() && part != "." && part != ".." && !part.contains(':'),
            "Unsafe SCNE dependency path: {reference}"
        );
    }
    Ok(normalized.to_owned())
}

#[derive(Default)]
struct StoredZip {
    data: Vec<u8>,
    central: Vec<u8>,
    count: usize,
}
impl StoredZip {
    fn add(&mut self, name: &str, raw: &[u8]) -> Result<()> {
        let crc = crc32fast::hash(raw);
        let deflated;
        let (method, bytes) = if raw.len() > STORE_LIMIT && name.to_lowercase().ends_with(".dds") {
            let mut encoder =
                flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
            std::io::Write::write_all(&mut encoder, raw)?;
            deflated = encoder.finish()?;
            (8u16, &deflated[..])
        } else {
            (0u16, raw)
        };
        ensure!(
            self.count < 65535 && self.data.len() + bytes.len() <= MAX_OUTPUT && name.len() < 65535,
            "Preview resource is too large."
        );
        let offset = self.data.len() as u32;
        let name = name.as_bytes();
        let header = |out: &mut Vec<u8>, central: bool| {
            out.extend_from_slice(
                &(if central { 0x02014b50u32 } else { 0x04034b50 }).to_le_bytes(),
            );
            if central {
                out.extend_from_slice(&20u16.to_le_bytes());
            }
            out.extend_from_slice(&20u16.to_le_bytes()); // version needed
            out.extend_from_slice(&0x0800u16.to_le_bytes()); // UTF-8 names
            out.extend_from_slice(&method.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes()); // DOS time/date
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            if central {
                out.extend_from_slice(&[0; 6]); // comment, disk, internal attributes
                out.extend_from_slice(&0u32.to_le_bytes());
                out.extend_from_slice(&offset.to_le_bytes());
            }
            out.extend_from_slice(name);
        };
        header(&mut self.data, false);
        self.data.extend_from_slice(bytes);
        header(&mut self.central, true);
        self.count += 1;
        Ok(())
    }
    fn finish(mut self) -> Result<Vec<u8>> {
        ensure!(self.count > 0, "Decoded preview resource is empty");
        let start = self.data.len() as u32;
        self.data.extend_from_slice(&self.central);
        self.data.extend_from_slice(&0x06054b50u32.to_le_bytes());
        self.data.extend_from_slice(&[0; 4]);
        self.data
            .extend_from_slice(&(self.count as u16).to_le_bytes());
        self.data
            .extend_from_slice(&(self.count as u16).to_le_bytes());
        self.data
            .extend_from_slice(&(self.central.len() as u32).to_le_bytes());
        self.data.extend_from_slice(&start.to_le_bytes());
        self.data.extend_from_slice(&0u16.to_le_bytes());
        Ok(self.data)
    }
}

// ---- DDS (C# TryBuildDds / TryConvertTld / TryDecodeSegmentedTld) ----
fn block_bytes(format: u8) -> Option<u32> {
    match format {
        70..=72 | 79..=81 => Some(8),
        73..=78 | 82..=84 | 94..=99 => Some(16),
        _ => None,
    }
}
fn bytes_per_pixel(format: u8) -> Option<u32> {
    match format {
        10 | 11 => Some(8),
        26 | 28 => Some(4),
        49 => Some(2),
        61 => Some(1),
        _ => None,
    }
}
fn build_dds(tld: &[u8], payload: &[u8]) -> Option<Vec<u8>> {
    if tld.len() < 32 || &tld[..3] != b"TLD" || payload.is_empty() {
        return None;
    }
    let format = tld[10];
    let block = block_bytes(format);
    let pixel = if block.is_none() {
        bytes_per_pixel(format)?
    } else {
        0
    };
    let width = u16::from_le_bytes([tld[16], tld[17]]) as u32;
    let height = u16::from_le_bytes([tld[18], tld[19]]) as u32;
    let depth = (u16::from_le_bytes([tld[20], tld[21]]) as u32).max(1);
    let layers = (u16::from_le_bytes([tld[22], tld[23]]) as u32).max(1);
    let mips = (tld[11] as u32).max(1);
    if width == 0 || height == 0 {
        return None;
    }
    let cube = depth == 1 && width == height && layers >= 6 && layers % 6 == 0;
    let top = match block {
        Some(b) => width
            .div_ceil(4)
            .checked_mul(height.div_ceil(4))?
            .checked_mul(b)?
            .checked_mul(depth)?,
        None => width.checked_mul(pixel)?,
    };
    let legacy = match format {
        71 => Some(*b"DXT1"),
        74 => Some(*b"DXT3"),
        77 => Some(*b"DXT5"),
        _ => None,
    };
    let dx10 = (layers > 1 && !cube) || depth > 1 || block.is_none() || legacy.is_none();
    let mut out = Vec::with_capacity(148 + payload.len());
    let mut put = |v: u32| out.extend_from_slice(&v.to_le_bytes());
    put(u32::from_le_bytes(*b"DDS "));
    put(124);
    let mut flags = 0x1 | 0x2 | 0x4 | 0x1000 | if block.is_some() { 0x80000 } else { 0x8 };
    if mips > 1 {
        flags |= 0x20000;
    }
    if depth > 1 {
        flags |= 0x800000;
    }
    put(flags);
    put(height);
    put(width);
    put(top);
    put(if depth > 1 { depth } else { 0 });
    put(if mips > 1 { mips } else { 0 });
    for _ in 0..11 {
        put(0);
    }
    put(32);
    put(0x4);
    put(u32::from_le_bytes(if dx10 {
        *b"DX10"
    } else {
        legacy.unwrap()
    }));
    for _ in 0..5 {
        put(0);
    }
    let mut caps = 0x1000;
    if mips > 1 {
        caps |= 0x8 | 0x400000;
    }
    if layers > 1 || depth > 1 {
        caps |= 0x8;
    }
    put(caps);
    put(if depth > 1 {
        0x200000
    } else if cube {
        0xfe00
    } else {
        0
    });
    put(0);
    put(0);
    put(0);
    if dx10 {
        put(format as u32);
        put(if depth > 1 {
            4
        } else if height == 1 {
            2
        } else {
            3
        });
        put(if cube { 4 } else { 0 });
        put(if depth > 1 {
            1
        } else if cube {
            layers / 6
        } else {
            layers
        });
        put(0);
    }
    out.extend_from_slice(payload);
    Some(out)
}
fn try_convert_tld(tld: &[u8]) -> Option<Vec<u8>> {
    if tld.len() < 32 || &tld[..3] != b"TLD" || block_bytes(tld[10]).is_none() {
        return None;
    }
    let size = u32::from_le_bytes(tld[24..28].try_into().ok()?) as usize;
    let repeated = u32::from_le_bytes(tld[28..32].try_into().ok()?) as usize;
    if size != tld.len() - 32 || repeated != size {
        return None;
    }
    build_dds(tld, &tld[32..])
}
#[derive(Clone)]
struct Segment {
    file_offset: u64,
    file_size: u64,
    memory_offset: usize,
    memory_size: usize,
    inflated_size: usize,
}
#[derive(Clone)]
struct Info {
    name: String,
    expected_size: usize,
    twiddled: bool,
    segments: Vec<Segment>,
}
fn decode_segmented_tld(
    oodle: &Oodle,
    tld: &[u8],
    mip: Option<&[u8]>,
    info: &mut Info,
) -> Result<Vec<u8>> {
    ensure!(tld.len() >= 32, "Truncated TLD");
    if info.segments.is_empty() {
        let inline = u32_at(tld, 28)? as usize;
        let memory = if info.expected_size > 0 {
            info.expected_size
        } else {
            u32_at(tld, 24)? as usize
        };
        ensure!(
            inline > 0 && memory > 0 && 32 + inline <= tld.len(),
            "Unsupported TLD segment layout"
        );
        info.segments.push(Segment {
            file_offset: 32,
            file_size: inline as u64,
            memory_offset: 0,
            memory_size: memory,
            inflated_size: if inline == memory { 0 } else { memory },
        });
    }
    let format = tld[10];
    let element = match block_bytes(format) {
        Some(b) => b as usize,
        None => bytes_per_pixel(format).context("Unsupported TLD format")? as usize,
    };
    let size = info
        .segments
        .iter()
        .map(|s| s.memory_offset + s.memory_size)
        .max()
        .unwrap_or(0);
    ensure!(
        size > 0 && size <= MAX_WORK,
        "Texture exceeds decoding budget"
    );
    let mut payload = vec![0u8; size];
    let inline_end = 32 + u32_at(tld, 28)? as u64;
    for segment in &info.segments {
        let (source, start) = if segment.file_offset < inline_end {
            (tld, segment.file_offset)
        } else {
            (
                mip.context("Texture segment is outside its TLD/mip0 data.")?,
                segment.file_offset - inline_end,
            )
        };
        let end = start
            .checked_add(segment.file_size)
            .context("Texture segment overflow")?;
        ensure!(
            end <= source.len() as u64,
            "Texture segment is outside its TLD/mip0 data."
        );
        let source = &source[start as usize..end as usize];
        let inflated =
            if segment.inflated_size == 0 && segment.file_size as usize == segment.memory_size {
                source.to_vec()
            } else {
                let n = if segment.inflated_size > 0 {
                    segment.inflated_size
                } else {
                    segment.memory_size
                };
                oodle.exact(source, n)?
            };
        let decoded = if format == 98 && inflated.len() >= 48 && u32_at(&inflated, 0)? == 0x07bc {
            oodle.bc7_prepared(&inflated)?
        } else if info.twiddled && element >= 4 {
            let n = segment.memory_size;
            ensure!(
                element % 4 == 0 && n % element == 0 && inflated.len() >= n,
                "Unsupported twiddled texture segment layout."
            );
            let blocks = n / element;
            let plane_size = blocks * 4;
            let mut out = vec![0u8; n];
            for b in 0..blocks {
                for plane in 0..element / 4 {
                    let from = plane * plane_size + b * 4;
                    out[b * element + plane * 4..b * element + plane * 4 + 4]
                        .copy_from_slice(&inflated[from..from + 4]);
                }
            }
            out
        } else {
            ensure!(
                inflated.len() >= segment.memory_size,
                "Texture segment is shorter than its declared memory size."
            );
            inflated[..segment.memory_size].to_vec()
        };
        ensure!(
            decoded.len() == segment.memory_size,
            "Decoded texture segment size does not match SCNE metadata."
        );
        payload[segment.memory_offset..segment.memory_offset + decoded.len()]
            .copy_from_slice(&decoded);
    }
    build_dds(tld, &payload).context("Unsupported TLD format or segment layout.")
}

// ---- Binary SCNE/TXTR tree (C# ScneFormatter) ----
fn is_tree(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && u32::from_le_bytes(bytes[..4].try_into().unwrap()) == 0xE01FF891
}
struct Node {
    name: String,
    tag: u32,
    text: String,
    number: u64,
    children: Vec<Node>,
}
struct Tree<'a> {
    data: &'a [u8],
    count: usize,
    strings: usize,
    root: Node,
}
impl<'a> Tree<'a> {
    fn parse(data: &'a [u8]) -> Result<Self> {
        ensure!(
            data.len() >= 24 && is_tree(data) && u32_at(data, 4)? == 1,
            "Unsupported binary SCNE header."
        );
        let string_bytes = u32_at(data, 8)? as usize;
        let count = u32_at(data, 12)? as usize;
        ensure!(
            string_bytes > 0 && string_bytes <= data.len() && count > 0 && count <= 4_000_000,
            "Invalid binary SCNE table sizes."
        );
        let strings = data.len() - string_bytes;
        ensure!(
            24 + count * 40 <= strings,
            "Binary SCNE node records overlap its string table."
        );
        let mut tree = Tree {
            data,
            count,
            strings,
            root: Node {
                name: String::new(),
                tag: 0,
                text: String::new(),
                number: 0,
                children: Vec::new(),
            },
        };
        let mut index = 0;
        tree.root = tree.node(&mut index, false, 0)?;
        ensure!(
            index == count,
            "Binary SCNE dependency scan did not consume every node record."
        );
        Ok(tree)
    }
    fn string(&self, pointer: usize) -> Result<String> {
        let relative = u64_at(self.data, pointer)? as i64;
        let target = (pointer as i64)
            .checked_add(relative)
            .and_then(|v| v.checked_sub(1))
            .context("String offset overflow")?;
        ensure!(
            target >= self.strings as i64 && target < self.data.len() as i64,
            "Binary SCNE string pointer is outside its string table."
        );
        let tail = &self.data[target as usize..];
        let end = tail
            .iter()
            .position(|b| *b == 0)
            .context("Unterminated binary SCNE string.")?;
        Ok(std::str::from_utf8(&tail[..end])?.to_owned())
    }
    fn record(&self, index: usize) -> Result<usize> {
        ensure!(
            index < self.count,
            "Binary SCNE node count is inconsistent."
        );
        Ok(24 + index * 40)
    }
    fn name(&self, index: usize) -> Result<String> {
        self.string(if index == 0 {
            16
        } else {
            self.record(index - 1)? + 32
        })
    }
    fn node(&self, index: &mut usize, unnamed: bool, depth: usize) -> Result<Node> {
        ensure!(depth <= 256, "Binary SCNE nesting exceeds limit");
        let current = *index;
        *index += 1;
        let offset = self.record(current)?;
        let full = u32_at(self.data, offset + 4)?;
        ensure!(
            full & 0xFFFFFF00 == 0x40000000,
            "Unknown binary SCNE value tag."
        );
        let tag = full & 0xff;
        let mut node = Node {
            name: if unnamed {
                String::new()
            } else {
                self.name(current)?
            },
            tag,
            text: String::new(),
            number: 0,
            children: Vec::new(),
        };
        match tag {
            3 => node.text = self.string(offset + 16)?,
            5 | 6 => node.number = u64_at(self.data, offset + 16)?,
            1 | 2 => {
                let children = u32_at(self.data, offset + 12)? as usize;
                ensure!(
                    children <= self.count - *index,
                    "Invalid binary SCNE child count"
                );
                for _ in 0..children {
                    node.children.push(self.node(index, tag == 2, depth + 1)?);
                }
            }
            0 => {}
            _ => bail!("Unsupported binary SCNE value tag {tag}."),
        }
        Ok(node)
    }
    fn collect(&self, _depth: usize, out: &mut Vec<Info>) -> Result<()> {
        fn child<'n>(node: &'n Node, name: &str) -> Option<&'n Node> {
            node.children
                .iter()
                .find(|c| c.name.eq_ignore_ascii_case(name))
        }
        fn number(node: Option<&Node>) -> usize {
            match node {
                Some(n) if n.tag == 6 && n.number <= i32::MAX as u64 => n.number as usize,
                _ => 0,
            }
        }
        fn walk(node: &Node, out: &mut Vec<Info>) {
            if node.tag == 1 {
                if let Some(binary) =
                    child(node, "Binary").filter(|b| b.tag == 3 && !b.text.is_empty())
                {
                    let mut info = Info {
                        name: binary.text.clone(),
                        expected_size: number(child(node, "Size")),
                        twiddled: number(child(node, "Twiddled")) != 0,
                        segments: Vec::new(),
                    };
                    if info.expected_size == 0 {
                        info.expected_size = number(child(node, "PixelDataSize"));
                    }
                    if let Some(segments) = child(node, "Segments").filter(|s| s.tag == 2) {
                        for s in segments.children.iter().filter(|s| s.tag == 1) {
                            let segment = Segment {
                                file_offset: number(child(s, "FileOffset")) as u64,
                                file_size: number(child(s, "FileSize")) as u64,
                                memory_offset: number(child(s, "MemoryOffset")),
                                memory_size: number(child(s, "MemorySize")),
                                inflated_size: number(child(s, "InflatedSize")),
                            };
                            if segment.file_size > 0 && segment.memory_size > 0 {
                                info.segments.push(segment);
                            }
                        }
                    }
                    out.push(info);
                }
            }
            for c in &node.children {
                walk(c, out);
            }
        }
        walk(&self.root, out);
        Ok(())
    }
}
/// C# FormatFloatingPoint: "G9", invariant, lowercase exponent.
fn format_float(value: f64) -> Result<String> {
    ensure!(
        value.is_finite(),
        "Binary SCNE contains a non-finite number."
    );
    if value == 0.0 {
        return Ok(if value.is_sign_negative() {
            "-0.0".into()
        } else {
            "0.0".into()
        });
    }
    let scientific = format!("{value:.8e}");
    let (mantissa, exponent) = scientific.split_once('e').unwrap();
    let exponent: i32 = exponent.parse()?;
    let negative = mantissa.starts_with('-');
    let digits: String = mantissa.chars().filter(|c| c.is_ascii_digit()).collect();
    let digits = digits.trim_end_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    if exponent > -5 && exponent < 9 {
        if exponent < 0 {
            out.push_str("0.");
            out.push_str(&"0".repeat((-exponent - 1) as usize));
            out.push_str(digits);
        } else {
            let point = exponent as usize + 1;
            if digits.len() <= point {
                out.push_str(digits);
                out.push_str(&"0".repeat(point - digits.len()));
                out.push_str(".0");
            } else {
                out.push_str(&digits[..point]);
                out.push('.');
                out.push_str(&digits[point..]);
            }
        }
    } else {
        out.push_str(&digits[..1]);
        if digits.len() > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        out.push('e');
        out.push(if exponent < 0 { '-' } else { '+' });
        out.push_str(&exponent.abs().to_string());
    }
    Ok(out)
}
/// C# ScneFormatter.Convert: binary SCNE/TXTR as text, preserving order.
fn scne_text(bytes: &[u8], converted: &HashSet<String>) -> Result<Vec<u8>> {
    let tree = Tree::parse(bytes)?;
    let mut out = String::with_capacity(bytes.len().min(16 * 1024 * 1024));
    fn quote(out: &mut String, value: &str) -> Result<()> {
        out.push_str(&serde_json::to_string(value)?);
        Ok(())
    }
    fn write(
        node: &Node,
        depth: usize,
        unnamed: bool,
        converted: &HashSet<String>,
        out: &mut String,
    ) -> Result<()> {
        if !unnamed {
            quote(out, &node.name)?;
            out.push_str(": ");
        }
        match node.tag {
            0 => out.push_str("null"),
            3 => {
                let mut value = node.text.clone();
                if node.name == "Binary" {
                    let lower = value.to_lowercase();
                    if lower.ends_with(".gz") {
                        value = format!("{}.bin", &value[..value.len() - 3]);
                    } else if (converted.contains(&lower)
                        || converted.contains(&leaf_of(&lower).to_owned()))
                        && lower.ends_with(".tld")
                    {
                        value = format!("{}.dds", &value[..value.len() - 4]);
                    }
                }
                quote(out, &value)?;
            }
            5 => out.push_str(&format_float(f64::from_bits(node.number))?),
            6 => out.push_str(&node.number.to_string()),
            _ => {
                let array = node.tag == 2;
                out.push(if array { '[' } else { '{' });
                for (i, child) in node.children.iter().enumerate() {
                    out.push_str(if i == 0 { "\r\n" } else { ",\r\n" });
                    out.push_str(&"	".repeat(depth + 1));
                    write(child, depth + 1, array, converted, out)?;
                }
                if !node.children.is_empty() {
                    out.push_str("\r\n");
                    out.push_str(&"	".repeat(depth));
                }
                out.push(if array { ']' } else { '}' });
            }
        }
        ensure!(out.len() <= MAX_OUTPUT, "Decoded document exceeds limit");
        Ok(())
    }
    write(&tree.root, 0, false, converted, &mut out)?;
    Ok(out.into_bytes())
}

/// One game-root index, rebuilt when the manifest changes (WPF manifest()).
pub struct FitAssets {
    game: Mutex<Option<Arc<Game>>>,
    epoch: AtomicU64,
}
impl Default for FitAssets {
    fn default() -> Self {
        Self {
            game: Mutex::new(None),
            epoch: AtomicU64::new(0),
        }
    }
}
impl FitAssets {
    pub fn game(&self, root: &Path) -> Result<Arc<Game>> {
        ensure!(
            !root.as_os_str().is_empty(),
            "Select the NBA 2K27 folder in Settings first."
        );
        let root = root
            .canonicalize()
            .context("Select the NBA 2K27 folder in Settings first.")?;
        ensure!(
            root.join("NBA2K27.exe").is_file(),
            "Select the NBA 2K27 folder in Settings first."
        );
        let file = open_plain(&root.join("manifest"))?;
        let meta = file.metadata()?;
        ensure!(
            meta.len() <= 128 * 1024 * 1024,
            "Unsupported game manifest."
        );
        let manifest_identity = crate::file_identity::Stable::read(&file)?;
        let manifest_stamp = format!(
            "{}:{}:{}",
            root.display(),
            stamp(&meta),
            serde_json::to_string(&manifest_identity)?
        );
        let mut slot = self
            .game
            .lock()
            .map_err(|_| anyhow::anyhow!("Preview index unavailable"))?;
        if let Some(game) = slot.as_ref().filter(|g| g.manifest_stamp == manifest_stamp) {
            return Ok(game.clone());
        }
        let mut text = String::with_capacity(meta.len() as usize);
        (&file).read_to_string(&mut text)?;
        let game = Arc::new(Game {
            root: root.clone(),
            manifest_stamp,
            index: Index::parse(&text)?,
            oodle: OnceLock::new(),
        });
        *slot = Some(game.clone());
        Ok(game)
    }
    /// WPF FastClothingPreview.cancel(): stop running decodes.
    pub fn cancel(&self) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
    }
    pub fn checkpoint(&self) -> impl Fn() -> Result<()> + '_ {
        let start = self.epoch.load(Ordering::SeqCst);
        move || {
            ensure!(
                self.epoch.load(Ordering::SeqCst) == start,
                "Preview cancelled."
            );
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn allow_list_is_narrow() {
        for ok in [
            "clothing/clothing_itemtable.iff",
            "clothing/geo/standard/male_jeans_geo.iff",
            "char/sig/png04090.iff",
            "shoes/shoe_style_0123.iff",
            "shoes/shoe_global.iff",
            "store/thumbnails/tshirt_solid_blk.iff",
        ] {
            assert!(allowed(ok), "{ok}");
        }
        for bad in [
            "char/sig/png04091.iff",
            "clothing/../x.iff",
            "clothing/a.dds",
            "audio/x.iff",
            "shoes/shoe_style_12345.iff",
            "store/thumbnails/a/b.iff",
        ] {
            assert!(!allowed(bad), "{bad}");
        }
    }
    #[test]
    fn floats_follow_dotnet_g9() {
        assert_eq!(format_float(1.0).unwrap(), "1.0");
        assert_eq!(format_float(-0.0).unwrap(), "-0.0");
        assert_eq!(format_float(0.5).unwrap(), "0.5");
        assert_eq!(format_float(123456789.0).unwrap(), "123456789.0");
        assert_eq!(format_float(1234567890.0).unwrap(), "1.23456789e+9");
        assert_eq!(format_float(0.00001).unwrap(), "1e-5");
        assert_eq!(format_float(0.0001).unwrap(), "0.0001");
        assert_eq!(format_float(0.1f32 as f64).unwrap(), "0.100000001");
    }
    #[test]
    fn stored_zip_round_trips_and_validates() {
        let mut zip = StoredZip::default();
        zip.add("a.scne", b"hello").unwrap();
        zip.add("dir/b.dds", b"DDS data").unwrap();
        let bytes = zip.finish().unwrap();
        assert_eq!(zip_length(&bytes).unwrap(), bytes.len());
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut text = String::new();
        archive
            .by_name("dir/b.dds")
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert_eq!(text, "DDS data");
    }
    #[test]
    fn inline_tld_becomes_dxt1_dds() {
        let mut tld = vec![0u8; 32 + 8];
        tld[..3].copy_from_slice(b"TLD");
        tld[10] = 71;
        tld[11] = 1;
        tld[16] = 4;
        tld[18] = 4;
        tld[24] = 8;
        tld[28] = 8;
        let dds = try_convert_tld(&tld).unwrap();
        assert_eq!(&dds[..4], b"DDS ");
        assert_eq!(&dds[84..88], b"DXT1");
        assert_eq!(dds.len(), 128 + 8);
    }
    #[test]
    fn content_hash_keys_match_csharp() {
        assert_eq!(
            content_hash_key("a.0123456789abcdef.tld").unwrap(),
            "0123456789abcdef.tld"
        );
        assert!(content_hash_key("a.tld").is_none());
        assert_eq!(logical_mip_key("tex.0123456789ABCDEF.mip0").unwrap(), "tex");
    }
}

#[cfg(test)]
mod live {
    use super::*;
    /// Read-only: C27_LIVE_GAME_DIR=<NBA 2K27> C27_FIT_OUT=<empty dir>
    /// C27_FIT_NAMES=a.iff;b.iff cargo test fit_assets::live -- --ignored
    #[test]
    #[ignore]
    fn export_live_resources_read_only() {
        let root = std::env::var("C27_LIVE_GAME_DIR").unwrap();
        let out = PathBuf::from(std::env::var("C27_FIT_OUT").unwrap());
        let assets = FitAssets::default();
        let game = assets.game(Path::new(&root)).unwrap();
        for name in std::env::var("C27_FIT_NAMES")
            .unwrap()
            .split(';')
            .filter(|n| !n.is_empty())
        {
            let started = std::time::Instant::now();
            let result = game.export(name, &|| Ok(()));
            match result {
                Ok(bytes) => {
                    let file = out.join(name.replace('/', "__"));
                    std::fs::write(&file, &bytes).unwrap();
                    println!(
                        "OK {name} {} bytes {} ms key {}",
                        bytes.len(),
                        started.elapsed().as_millis(),
                        game.key(name).unwrap()
                    );
                }
                Err(e) => println!("ERR {name}: {e:#}"),
            }
        }
    }
}
