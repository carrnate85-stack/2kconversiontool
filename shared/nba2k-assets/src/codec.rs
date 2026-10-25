use crate::catalog::{Catalog, Resource, safe_relative};
use anyhow::{Context, Result, bail, ensure};
use libloading::Library;
use serde_json::{Map, Value, json};
use std::{
    fs::File,
    io::{Cursor, Read, Seek, SeekFrom},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
pub const MAX: usize = 512 * 1024 * 1024;
pub fn u16_at(b: &[u8], p: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        b.get(p..p + 2).context("Truncated u16")?.try_into()?,
    ))
}
pub fn u32_at(b: &[u8], p: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        b.get(p..p + 4).context("Truncated u32")?.try_into()?,
    ))
}
pub fn u64_at(b: &[u8], p: usize) -> Result<u64> {
    Ok(u64::from_le_bytes(
        b.get(p..p + 8).context("Truncated u64")?.try_into()?,
    ))
}
pub fn cancelled(flag: &AtomicBool) -> Result<()> {
    ensure!(!flag.load(Ordering::Relaxed), "Operation cancelled");
    Ok(())
}
pub fn vcz(b: &[u8]) -> bool {
    b.get(12..16) == Some(b"VCZ\0")
}
pub fn range(c: &Catalog, e: &Resource) -> Result<Vec<u8>> {
    ensure!(
        e.size <= MAX as u64,
        "Resource exceeds prototype's 512 MiB limit"
    );
    let mut file = File::open(c.path(e)?)?;
    let len = file.metadata()?.len();
    ensure!(
        e.offset <= len && e.size <= len - e.offset,
        "Resource exceeds archive bounds"
    );
    file.seek(SeekFrom::Start(e.offset))?;
    let mut bytes = vec![0; e.size as usize];
    file.read_exact(&mut bytes)?;
    Ok(bytes)
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
pub struct Oodle {
    _library: Library,
    step: Step,
    decode: Decode,
}
impl Oodle {
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join("data/oodle/oo2core_9_win64.dll").canonicalize()?;
        // Only the explicitly selected installed game's runtime; never DLL search-path lookup.
        unsafe {
            let library = Library::new(path).context("Cannot load installed Oodle runtime")?;
            let step = *library.get::<Step>(b"OodleLZ_GetCompressedStepForRawStep\0")?;
            let decode = *library.get::<Decode>(b"OodleLZ_Decompress\0")?;
            Ok(Self {
                _library: library,
                step,
                decode,
            })
        }
    }
    pub fn decode(&self, input: &[u8], n: usize) -> Result<(Vec<u8>, i64)> {
        ensure!(n > 0 && n <= MAX, "Invalid decoded size");
        let mut out = vec![0; n];
        let ret = unsafe {
            (self.decode)(
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
        Ok((out, ret))
    }
    pub fn buffer(&self, bytes: &[u8], expected: usize) -> Result<Vec<u8>> {
        if !vcz(bytes) {
            return Ok(bytes.to_vec());
        }
        let (out, ret) = self.decode(&bytes[16..], expected)?;
        ensure!(
            ret == expected as i64,
            "Compressed geometry decoded {ret} bytes; expected {expected}"
        );
        Ok(out)
    }
    pub fn archive(&self, bytes: &[u8], cancel: &AtomicBool) -> Result<Vec<u8>> {
        self.archive_impl(bytes, cancel, false)
    }
    pub fn archive_preview(&self, bytes: &[u8], cancel: &AtomicBool) -> Result<Vec<u8>> {
        self.archive_impl(bytes, cancel, true)
    }
    fn archive_impl(&self, bytes: &[u8], cancel: &AtomicBool, preview: bool) -> Result<Vec<u8>> {
        if !vcz(bytes) {
            return Ok(bytes.to_vec());
        }
        let input = &bytes[16..];
        let (mut raw_end, mut independent) = (0, 0);
        let mut used = unsafe {
            (self.step)(
                input.as_ptr(),
                input.len() as i64,
                0,
                1 << 30,
                &mut raw_end,
                &mut independent,
            )
        };
        if used <= 0 || raw_end <= 0 {
            // Small streams do not always return a boundary for a large seek request.
            // Match the proven prefix scan used by the production decoder.
            let (mut low, mut high, mut best_size, mut best_raw) = (1i64, (1 << 26) - 1, 0, 0);
            while low <= high {
                cancelled(cancel)?;
                let candidate = low + (high - low) / 2;
                let (mut end, mut independent) = (0, 0);
                let size = unsafe {
                    (self.step)(
                        input.as_ptr(),
                        input.len() as i64,
                        0,
                        candidate,
                        &mut end,
                        &mut independent,
                    )
                };
                if size > 0 && end > 0 {
                    best_size = size;
                    best_raw = end;
                    low = candidate + 1;
                } else {
                    high = candidate - 1;
                }
            }
            used = best_size;
            raw_end = best_raw;
            if used <= 0 || raw_end <= 0 {
                let (mut low, mut high, mut best) = (1usize, 1 << 18, 0);
                while low <= high {
                    cancelled(cancel)?;
                    let candidate = low + (high - low) / 2;
                    let (_, ret) = self.decode(input, candidate)?;
                    if ret == candidate as i64 {
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
            used > 0 && used <= input.len() as i64 && raw_end > 0 && raw_end <= MAX as i64,
            "Unsupported VCZ stream (boundary {used}/{}, raw {raw_end})",
            input.len()
        );
        cancelled(cancel)?;
        let (raw, decoded) = self.decode(input, raw_end as usize)?;
        cancelled(cancel)?;
        // Parse original local/central records to find the *verified* real ZIP boundary,
        // rather than guessing from Oodle's rounded raw block size.
        let exact = match zip_length(&raw) {
            Ok(exact) => exact,
            Err(error) => {
                if preview
                    && used < input.len() as i64
                    && input.len() as i64 - used > 16
                    && raw_end <= (MAX - (1 << 18)) as i64
                {
                    if let Ok(recovered) = self.recover_preview(input, raw_end as usize, cancel) {
                        return Ok(recovered);
                    }
                }
                return Err(error).context(format!("VCZ container boundary unavailable (decoded {decoded}, rounded {raw_end}); use stable exporter for this resource"));
            }
        };
        let (out, count) = self.decode(input, exact)?;
        ensure!(
            count == exact as i64 && zip_length(&out)? == exact,
            "VCZ did not produce a complete container"
        );
        Ok(out)
    }
    // Read-only preview/inspection recovery. Same one-extra-block window as
    // the stable decoder; ZIP records, every member CRC and any footer must
    // validate. No invented end records, no truncated geometry acceptance.
    fn recover_preview(
        &self,
        input: &[u8],
        boundary: usize,
        cancel: &AtomicBool,
    ) -> Result<Vec<u8>> {
        let limit = boundary
            .checked_add(1 << 18)
            .context("Preview tail size overflow")?;
        ensure!(limit <= MAX, "Preview tail exceeds budget");
        let mut cursor = 0usize;
        let mut total = 0usize;
        let mut chunks = Vec::new();
        while cursor < input.len() {
            cancelled(cancel)?;
            let remaining = &input[cursor..];
            if remaining.len() <= 16 && !chunks.is_empty() {
                break;
            }
            let (mut raw, mut independent) = (0i64, 0);
            let mut packed = unsafe {
                (self.step)(
                    remaining.as_ptr(),
                    remaining.len() as i64,
                    0,
                    1 << 26,
                    &mut raw,
                    &mut independent,
                )
            };
            if packed <= 0 {
                let (mut low, mut high, mut best_raw, mut best_packed) =
                    (1i64, (1 << 26) - 1, 0, 0);
                while low <= high {
                    cancelled(cancel)?;
                    let candidate = low + (high - low) / 2;
                    let (mut end, mut independent) = (0, 0);
                    let consumed = unsafe {
                        (self.step)(
                            remaining.as_ptr(),
                            remaining.len() as i64,
                            0,
                            candidate,
                            &mut end,
                            &mut independent,
                        )
                    };
                    if consumed > 0 && end > 0 {
                        best_raw = end;
                        best_packed = consumed;
                        low = candidate + 1;
                    } else {
                        high = candidate - 1;
                    }
                }
                raw = best_raw;
                packed = best_packed;
                if raw <= 0 || packed <= 0 {
                    let (mut low, mut high, mut best) =
                        (1usize, (1 << 18).min(limit.saturating_sub(total)), 0);
                    while low <= high {
                        cancelled(cancel)?;
                        let candidate = low + (high - low) / 2;
                        let (_, decoded) = self.decode(remaining, candidate)?;
                        if decoded == candidate as i64 {
                            best = candidate;
                            low = candidate + 1;
                        } else {
                            high = candidate - 1;
                        }
                    }
                    raw = best as i64;
                    packed = remaining.len() as i64;
                }
            }
            ensure!(
                packed > 0 && packed <= remaining.len() as i64 && raw > 0 && raw <= 1 << 26,
                "Invalid preview VCZ chunk"
            );
            total = total
                .checked_add(raw as usize)
                .context("Preview chunk overflow")?;
            ensure!(total <= limit, "Preview tail exceeds one-block window");
            chunks.push((cursor, raw as usize));
            cursor += packed as usize;
        }
        ensure!(
            !chunks.is_empty() && input.len() - cursor <= 16,
            "Preview chunks did not consume stream"
        );
        let (combined, count) = self.decode(input, total)?;
        if count == total as i64 {
            return trim_preview(combined, boundary, cancel);
        }
        if count > 0 && count <= total as i64 {
            if let Ok(out) = trim_preview(combined[..count as usize].to_vec(), boundary, cancel) {
                return Ok(out);
            }
        }
        let mut combined = Vec::with_capacity(total);
        for (cursor, size) in chunks {
            cancelled(cancel)?;
            let (chunk, count) = self.decode(&input[cursor..], size)?;
            ensure!(count == size as i64, "Incomplete preview VCZ chunk");
            combined.extend(chunk);
        }
        trim_preview(combined, boundary, cancel)
    }
}
fn trim_preview(mut bytes: Vec<u8>, boundary: usize, cancel: &AtomicBool) -> Result<Vec<u8>> {
    ensure!(
        boundary > 0 && boundary <= MAX - (1 << 18),
        "Invalid preview recovery boundary"
    );
    let exact = zip_length(&bytes)?;
    ensure!(
        exact > boundary && exact <= boundary + (1 << 18),
        "Recovered ZIP outside preview window"
    );
    let trailing = bytes.len() - exact;
    if trailing != 0 {
        ensure!(
            trailing == 8 && u32_at(&bytes, exact + 4)? as usize == exact,
            "Invalid preview ZIP footer"
        );
        let mut crc = !0u32;
        for byte in &bytes[..exact] {
            crc ^= *byte as u32;
            for _ in 0..8 {
                crc = (crc >> 1) ^ if crc & 1 != 0 { 0xedb88320 } else { 0 };
            }
        }
        ensure!(
            !crc == u32_at(&bytes, exact)?,
            "Preview ZIP footer CRC mismatch"
        );
    }
    bytes.truncate(exact);
    Archive::open(bytes.clone())?.validate_with(cancel, |_| {})?;
    Ok(bytes)
}
pub fn zip_length(bytes: &[u8]) -> Result<usize> {
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
        u16_at(bytes, p + 4)? == 0
            && u16_at(bytes, p + 6)? == 0
            && u16_at(bytes, p + 8)? as usize == count
            && u16_at(bytes, p + 10)? as usize == count,
        "Inconsistent ZIP counts"
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

pub struct Archive {
    pub zip: zip::ZipArchive<Cursor<Vec<u8>>>,
}
impl Archive {
    pub fn open(bytes: Vec<u8>) -> Result<Self> {
        let zip = zip::ZipArchive::new(Cursor::new(bytes))?;
        ensure!(zip.len() <= 100_000, "IFF has too many members");
        Ok(Self { zip })
    }
    pub fn names(&mut self) -> Result<Vec<Value>> {
        let mut members = Vec::new();
        for i in 0..self.zip.len() {
            let m = self.zip.by_index(i)?;
            members.push(json!({"id":i,"name":m.name(),"size":m.size(),"packedSize":m.compressed_size(),"crc32":format!("{:08X}",m.crc32())}));
        }
        Ok(members)
    }
    pub fn read(&mut self, index: usize, max: usize) -> Result<Vec<u8>> {
        let mut m = self.zip.by_index(index)?;
        ensure!(m.size() <= max as u64, "Member exceeds {max} byte limit");
        ensure!(!m.encrypted(), "Encrypted IFF member unsupported");
        let mut out = Vec::with_capacity(m.size() as usize);
        m.by_ref().take(max as u64 + 1).read_to_end(&mut out)?;
        ensure!(
            out.len() <= max && out.len() as u64 == m.size(),
            "IFF member size mismatch"
        );
        Ok(out)
    }
    pub fn named(&mut self, name: &str) -> Result<Vec<u8>> {
        let normalized = name.replace('\\', "/");
        let leaf = normalized.rsplit('/').next().unwrap_or(&normalized);
        let alias = if leaf.to_lowercase().ends_with(".gz") {
            format!("{}.bin", &leaf[..leaf.len() - 3])
        } else if leaf.to_lowercase().ends_with(".bin") {
            format!("{}.gz", &leaf[..leaf.len() - 4])
        } else {
            String::new()
        };
        let i = (0..self.zip.len())
            .find(|i| {
                self.zip
                    .by_index(*i)
                    .map(|m| {
                        m.name().eq_ignore_ascii_case(&normalized)
                            || m.name().eq_ignore_ascii_case(leaf)
                            || !alias.is_empty() && m.name().eq_ignore_ascii_case(&alias)
                    })
                    .unwrap_or(false)
            })
            .context(format!("Missing IFF member {name}"))?;
        self.read(i, MAX)
    }
    pub fn validate(&mut self, cancel: &AtomicBool) -> Result<usize> {
        self.validate_with(cancel, |_| {})
    }
    pub fn validate_with(
        &mut self,
        cancel: &AtomicBool,
        mut progress: impl FnMut(f64),
    ) -> Result<usize> {
        let mut total = 0usize;
        let mut last = std::time::Instant::now();
        for i in 0..self.zip.len() {
            cancelled(cancel)?;
            let m = self.zip.by_index(i)?;
            let name = m.name().to_string();
            if !m.is_dir() {
                safe_relative(&name)?;
            }
            total = total
                .checked_add(m.size() as usize)
                .context("IFF size overflow")?;
            ensure!(total <= MAX, "IFF members exceed 512 MiB validation budget");
            drop(m);
            self.read(i, MAX)?;
            if last.elapsed().as_millis() >= 35 || i + 1 == self.zip.len() {
                progress((i + 1) as f64 / self.zip.len() as f64);
                last = std::time::Instant::now();
            }
        }
        Ok(self.zip.len())
    }
}

pub fn tree(bytes: &[u8]) -> Result<Value> {
    tree_impl(bytes, true)
}
pub fn tree_exact(bytes: &[u8]) -> Result<Value> {
    tree_impl(bytes, false)
}
fn tree_impl(bytes: &[u8], js_safe: bool) -> Result<Value> {
    if bytes.len() < 24 || u32_at(bytes, 0)? != 0xe01ff891 {
        let text = std::str::from_utf8(bytes)?
            .trim_start_matches('\u{feff}')
            .trim();
        return serde_json::from_str(text)
            .or_else(|_| serde_json::from_str(&format!("{{{text}}}")))
            .context("Member is not readable JSON or binary SCNE/TXTR");
    }
    ensure!(u32_at(bytes, 4)? == 1, "Unsupported binary-tree version");
    let nodes = u32_at(bytes, 12)? as usize;
    let strings = u32_at(bytes, 8)? as usize;
    ensure!(
        strings <= bytes.len() && nodes <= 1_000_000 && 24 + nodes * 40 <= bytes.len() - strings,
        "Invalid tree bounds"
    );
    let strings_start = bytes.len() - strings;
    fn text(b: &[u8], p: usize, ss: usize) -> Result<String> {
        let rel = u64_at(b, p)? as i64;
        let start = (p as i64)
            .checked_add(rel)
            .and_then(|v| v.checked_sub(1))
            .context("String offset overflow")?;
        ensure!(
            start >= ss as i64 && start < (b.len() as i64),
            "Invalid binary-tree string pointer"
        );
        let suffix = &b[start as usize..];
        let end = suffix
            .iter()
            .position(|x| *x == 0)
            .context("Unterminated string")?;
        Ok(std::str::from_utf8(&suffix[..end])?.to_string())
    }
    fn node(
        b: &[u8],
        cursor: &mut usize,
        n: usize,
        ss: usize,
        depth: usize,
        unnamed: bool,
        js_safe: bool,
    ) -> Result<(String, Value)> {
        ensure!(depth <= 128 && *cursor < n, "Invalid tree traversal");
        let index = *cursor;
        *cursor += 1;
        let p = 24 + index * 40;
        let name = if unnamed {
            String::new()
        } else {
            text(
                b,
                if index == 0 {
                    16
                } else {
                    24 + (index - 1) * 40 + 32
                },
                ss,
            )?
        };
        let tag = u32_at(b, p + 4)?;
        ensure!(
            tag & 0xffffff00 == 0x40000000,
            "Unsupported binary-tree value tag"
        );
        let tag = tag & 0xff;
        let value = match tag {
            0 => Value::Null,
            3 => Value::String(text(b, p + 16, ss)?),
            5 => {
                let x = f64::from_bits(u64_at(b, p + 16)?);
                ensure!(x.is_finite(), "Nonfinite SCNE number");
                json!(x)
            }
            6 => {
                let x = u64_at(b, p + 16)?;
                if !js_safe || x <= 9_007_199_254_740_991 {
                    json!(x)
                } else {
                    json!(x.to_string())
                }
            }
            1 | 2 => {
                let count = u32_at(b, p + 12)? as usize;
                ensure!(count <= n - *cursor, "Invalid child count");
                let mut obj = Map::new();
                let mut arr = Vec::new();
                for _ in 0..count {
                    let (key, child) = node(b, cursor, n, ss, depth + 1, tag == 2, js_safe)?;
                    if tag == 1 {
                        obj.insert(key, child);
                    } else {
                        arr.push(child);
                    }
                }
                if tag == 1 {
                    Value::Object(obj)
                } else {
                    Value::Array(arr)
                }
            }
            _ => bail!("Unsupported tree tag {tag}"),
        };
        Ok((name, value))
    }
    let mut cursor = 0;
    let (name, value) = node(bytes, &mut cursor, nodes, strings_start, 0, false, js_safe)?;
    ensure!(cursor == nodes, "Unconsumed tree nodes");
    Ok(json!({name:value}))
}

/// Emit binary-tree records directly: preserve object order, duplicate keys,
/// integer types and full floating-point precision. Never serialize preview data.
pub fn document(
    bytes: &[u8],
    renames: &std::collections::BTreeMap<String, String>,
) -> Result<Vec<u8>> {
    // Validate the entire tree before visiting record pointers.
    let mut parsed = tree_exact(bytes)?;
    if bytes.len() < 24 || u32_at(bytes, 0)? != 0xe01ff891 {
        fn rewrite(v: &mut Value, names: &std::collections::BTreeMap<String, String>) {
            if let Some(obj) = v.as_object_mut() {
                if let Some(Value::String(name)) = obj.get_mut("Binary") {
                    if let Some(new) = names.get(&name.replace('\\', "/").to_lowercase()) {
                        *name = new.clone();
                    }
                }
                for child in obj.values_mut() {
                    rewrite(child, names);
                }
            } else if let Some(arr) = v.as_array_mut() {
                for child in arr {
                    rewrite(child, names);
                }
            }
        }
        rewrite(&mut parsed, renames);
        let out = serde_json::to_vec_pretty(&parsed)?;
        return Ok(out);
    }
    let ss = bytes.len() - u32_at(bytes, 8)? as usize;
    fn string(b: &[u8], p: usize, ss: usize) -> Result<String> {
        let start = p as i64 + u64_at(b, p)? as i64 - 1;
        ensure!(
            start >= ss as i64 && start < b.len() as i64,
            "Invalid string pointer"
        );
        let tail = &b[start as usize..];
        let end = tail
            .iter()
            .position(|x| *x == 0)
            .context("Unterminated string")?;
        Ok(std::str::from_utf8(&tail[..end])?.to_string())
    }
    fn emit(
        b: &[u8],
        ss: usize,
        cursor: &mut usize,
        depth: usize,
        unnamed: bool,
        names: &std::collections::BTreeMap<String, String>,
        out: &mut String,
    ) -> Result<()> {
        let i = *cursor;
        *cursor += 1;
        let p = 24 + i * 40;
        let key = if unnamed {
            String::new()
        } else {
            string(b, if i == 0 { 16 } else { 24 + (i - 1) * 40 + 32 }, ss)?
        };
        if !unnamed {
            out.push_str(&serde_json::to_string(&key)?);
            out.push_str(": ");
        }
        match u32_at(b, p + 4)? & 255 {
            0 => out.push_str("null"),
            3 => {
                let mut value = string(b, p + 16, ss)?;
                if key == "Binary" {
                    if let Some(new) = names.get(&value.replace('\\', "/").to_lowercase()) {
                        value = new.clone();
                    }
                }
                out.push_str(&serde_json::to_string(&value)?);
            }
            5 => out.push_str(&serde_json::to_string(&f64::from_bits(u64_at(b, p + 16)?))?),
            6 => out.push_str(&u64_at(b, p + 16)?.to_string()),
            1 | 2 => {
                let array = u32_at(b, p + 4)? & 255 == 2;
                let count = u32_at(b, p + 12)? as usize;
                out.push(if array { '[' } else { '{' });
                for child in 0..count {
                    if child > 0 {
                        out.push(',');
                    }
                    out.push_str("\r\n");
                    out.push_str(&"  ".repeat(depth + 1));
                    emit(b, ss, cursor, depth + 1, array, names, out)?;
                }
                if count > 0 {
                    out.push_str("\r\n");
                    out.push_str(&"  ".repeat(depth));
                }
                out.push(if array { ']' } else { '}' });
            }
            _ => bail!("Unsupported document node"),
        }
        ensure!(out.len() <= MAX, "Decoded document exceeds limit");
        Ok(())
    }
    let mut out = String::new();
    let mut cursor = 0;
    emit(bytes, ss, &mut cursor, 0, false, renames, &mut out)?;
    Ok(out.into_bytes())
}

pub fn dds(bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.starts_with(b"DDS ") {
        ensure!(bytes.len() >= 128, "Truncated DDS");
        return Ok(bytes.to_vec());
    }
    ensure!(
        bytes.len() >= 32 && bytes.starts_with(b"TLD"),
        "Not an inline TLD/DDS texture"
    );
    let format = bytes[10] as u32;
    let mips = (bytes[11] as u32).max(1);
    let w = u16_at(bytes, 16)? as u32;
    let h = u16_at(bytes, 18)? as u32;
    let depth = (u16_at(bytes, 20)? as u32).max(1);
    let layers = (u16_at(bytes, 22)? as u32).max(1);
    ensure!(
        w > 0
            && h > 0
            && u32_at(bytes, 24)? as usize == bytes.len() - 32
            && u32_at(bytes, 28)? as usize == bytes.len() - 32,
        "Segmented/external TLD conversion is not yet ported; use stable exporter"
    );
    let block = match format {
        70..=72 | 79..=81 => 8,
        73..=78 | 82..=84 | 94..=99 => 16,
        _ => 0,
    };
    let pixel = match format {
        10 | 11 => 8,
        26 | 28 => 4,
        49 => 2,
        61 => 1,
        _ => 0,
    };
    ensure!(block > 0 || pixel > 0, "Unsupported TLD format {format}");
    ensure!(
        mips <= 1 + w.max(h).max(depth).ilog2(),
        "Invalid TLD mip count"
    );
    let (mut mw, mut mh, mut md) = (w as u64, h as u64, depth as u64);
    let mut required = 0u64;
    for _ in 0..mips {
        required = required
            .checked_add(if block > 0 {
                mw.div_ceil(4) * mh.div_ceil(4) * block as u64 * md
            } else {
                mw * mh * pixel as u64 * md
            })
            .context("TLD mip size overflow")?;
        mw = (mw / 2).max(1);
        mh = (mh / 2).max(1);
        md = (md / 2).max(1);
    }
    ensure!(
        required
            .checked_mul(if depth > 1 { 1 } else { layers as u64 })
            .context("TLD layer size overflow")?
            == bytes.len() as u64 - 32,
        "TLD dimensions and mip payload size disagree"
    );
    let cube = depth == 1 && w == h && layers >= 6 && layers % 6 == 0;
    let four_cc = match format {
        71 => *b"DXT1",
        74 => *b"DXT3",
        77 => *b"DXT5",
        _ => *b"DX10",
    };
    let dx10 = (layers > 1 && !cube) || depth > 1 || pixel > 0 || four_cc == *b"DX10";
    let mut out = vec![0u8; if dx10 { 148 } else { 128 }];
    out[..4].copy_from_slice(b"DDS ");
    let mut put = |p: usize, v: u32| out[p..p + 4].copy_from_slice(&v.to_le_bytes());
    put(4, 124);
    put(
        8,
        0x1 | 0x2
            | 0x4
            | 0x1000
            | if block > 0 { 0x80000 } else { 8 }
            | if mips > 1 { 0x20000 } else { 0 }
            | if depth > 1 { 0x800000 } else { 0 },
    );
    put(12, h);
    put(16, w);
    put(
        20,
        if block > 0 {
            w.div_ceil(4) * h.div_ceil(4) * block * depth
        } else {
            w * pixel
        },
    );
    put(24, if depth > 1 { depth } else { 0 });
    put(28, if mips > 1 { mips } else { 0 });
    put(76, 32);
    put(80, 4);
    put(
        84,
        u32::from_le_bytes(if dx10 { *b"DX10" } else { four_cc }),
    );
    put(
        108,
        0x1000 | if mips > 1 { 0x400008 } else { 0 } | if layers > 1 || depth > 1 { 8 } else { 0 },
    );
    put(
        112,
        if depth > 1 {
            0x200000
        } else if cube {
            0xfe00
        } else {
            0
        },
    );
    if dx10 {
        put(128, format);
        put(
            132,
            if depth > 1 {
                4
            } else if h == 1 {
                2
            } else {
                3
            },
        );
        put(136, if cube { 4 } else { 0 });
        put(
            140,
            if depth > 1 {
                1
            } else if cube {
                layers / 6
            } else {
                layers
            },
        );
    }
    out.extend_from_slice(&bytes[32..]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_tree_rejected() {
        let mut b = vec![0; 24];
        b[..4].copy_from_slice(&0xe01ff891u32.to_le_bytes());
        assert!(tree(&b).is_err());
    }
    #[test]
    fn normal_json_is_readable() {
        assert_eq!(tree(br#"{"hello":[1,2]}"#).unwrap()["hello"][1], 2);
    }
    #[test]
    fn truncated_container_rejected() {
        assert!(zip_length(b"PK\x03\x04").is_err());
    }
    #[test]
    fn tld_does_not_guess_external_payload() {
        let mut b = vec![0; 32];
        b[..3].copy_from_slice(b"TLD");
        b[10] = 71;
        assert!(dds(&b).is_err());
    }
    #[test]
    fn unnamed_array_nodes_are_valid() {
        let mut b = vec![0; 149];
        b[..4].copy_from_slice(&0xe01ff891u32.to_le_bytes());
        b[4..8].copy_from_slice(&1u32.to_le_bytes());
        b[8..12].copy_from_slice(&5u32.to_le_bytes());
        b[12..16].copy_from_slice(&3u32.to_le_bytes());
        b[16..24].copy_from_slice(&129i64.to_le_bytes());
        b[144..].copy_from_slice(b"root\0");
        b[28..32].copy_from_slice(&0x40000002u32.to_le_bytes());
        b[36..40].copy_from_slice(&2u32.to_le_bytes());
        for (p, v) in [(64, 10u64), (104, 20u64)] {
            b[p + 4..p + 8].copy_from_slice(&0x40000006u32.to_le_bytes());
            b[p + 16..p + 24].copy_from_slice(&v.to_le_bytes());
        }
        assert_eq!(tree(&b).unwrap(), json!({"root":[10,20]}));
    }
    fn zip_fixture() -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file(
                "a.txt",
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        std::io::Write::write_all(&mut writer, b"fixture bytes").unwrap();
        writer.finish().unwrap().into_inner()
    }
    #[test]
    fn preview_recovery_requires_complete_crc_valid_zip() {
        let bytes = zip_fixture();
        let boundary = bytes.len() - 1;
        assert_eq!(
            trim_preview(bytes.clone(), boundary, &AtomicBool::new(false)).unwrap(),
            bytes
        );
        let mut corrupt = bytes.clone();
        corrupt[35] ^= 1;
        assert!(trim_preview(corrupt, boundary, &AtomicBool::new(false)).is_err());
    }
    #[test]
    fn preview_recovery_rejects_invented_footer() {
        let mut bytes = zip_fixture();
        let exact = bytes.len();
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&(exact as u32).to_le_bytes());
        assert!(trim_preview(bytes, exact - 1, &AtomicBool::new(false)).is_err());
    }
    #[test]
    fn preview_recovery_rejects_invalid_window() {
        let bytes = zip_fixture();
        let exact = bytes.len();
        assert!(trim_preview(bytes.clone(), 0, &AtomicBool::new(false)).is_err());
        assert!(trim_preview(bytes, exact, &AtomicBool::new(false)).is_err());
    }
    #[test]
    fn preview_recovery_respects_cancellation() {
        let bytes = zip_fixture();
        let boundary = bytes.len() - 1;
        assert!(trim_preview(bytes, boundary, &AtomicBool::new(true)).is_err());
    }
    #[test]
    fn member_crc_verified() {
        let bytes = zip_fixture();
        assert_eq!(zip_length(&bytes).unwrap(), bytes.len());
        let mut archive = Archive::open(bytes).unwrap();
        assert_eq!(archive.validate(&AtomicBool::new(false)).unwrap(), 1);
        assert_eq!(archive.read(0, 100).unwrap(), b"fixture bytes");
    }
    #[test]
    fn corrupt_member_rejected() {
        let mut bytes = zip_fixture();
        let p = bytes
            .windows(13)
            .position(|w| w == b"fixture bytes")
            .unwrap();
        bytes[p] ^= 1;
        let mut archive = Archive::open(bytes).unwrap();
        assert!(archive.read(0, 100).is_err());
    }
    #[test]
    fn member_budget_enforced() {
        let mut archive = Archive::open(zip_fixture()).unwrap();
        assert!(archive.read(0, 3).is_err());
    }
    #[test]
    fn cancellation_stops_validation() {
        let mut archive = Archive::open(zip_fixture()).unwrap();
        assert!(archive.validate(&AtomicBool::new(true)).is_err());
    }
}
