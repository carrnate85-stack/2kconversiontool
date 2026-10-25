//! Export conversion is isolated from every preview/material/colorway path.
use crate::{
    catalog::{Catalog, safe_relative},
    codec::{self, Archive, Oodle},
    textures,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    borrow::Cow,
    cell::Cell,
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    io::{Cursor, Read, Write},
    sync::atomic::AtomicBool,
};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};
const OUTPUT_MAX: u64 = crate::container::EXPANDED_MAX;
/// Immutable source payloads stay in their original compressed container.
/// Only descriptors and external dependencies own decoded buffers.
enum Payload {
    Source {
        index: usize,
        size: usize,
        prefix: Vec<u8>,
        verified: Cell<bool>,
    },
    Owned(Vec<u8>),
}
trait MemberData {
    fn len(&self) -> usize;
    fn prefix(&self) -> &[u8];
    fn copy(
        &self,
        archive: Option<&mut Archive>,
        max: usize,
        cancel: &AtomicBool,
        output: &mut dyn Write,
    ) -> Result<u64>;
}
impl MemberData for Vec<u8> {
    fn len(&self) -> usize {
        Vec::len(self)
    }
    fn prefix(&self) -> &[u8] {
        self
    }
    fn copy(
        &self,
        _: Option<&mut Archive>,
        max: usize,
        cancel: &AtomicBool,
        output: &mut dyn Write,
    ) -> Result<u64> {
        ensure!(self.len() <= max, "Member exceeds copy limit");
        for chunk in self.chunks(128 * 1024) {
            codec::cancelled(cancel)?;
            output.write_all(chunk)?;
        }
        codec::cancelled(cancel)?;
        Ok(self.len() as u64)
    }
}
impl MemberData for Payload {
    fn len(&self) -> usize {
        match self {
            Self::Source { size, .. } => *size,
            Self::Owned(bytes) => bytes.len(),
        }
    }
    fn prefix(&self) -> &[u8] {
        match self {
            Self::Source { prefix, .. } => prefix,
            Self::Owned(bytes) => bytes,
        }
    }
    fn copy(
        &self,
        archive: Option<&mut Archive>,
        max: usize,
        cancel: &AtomicBool,
        output: &mut dyn Write,
    ) -> Result<u64> {
        match self {
            Self::Source {
                index, verified, ..
            } => {
                let count = archive
                    .context("Missing original payload source")?
                    .copy_member(*index, max, cancel, output)?;
                verified.set(true);
                Ok(count)
            }
            Self::Owned(bytes) => bytes.copy(None, max, cancel, output),
        }
    }
}
impl Payload {
    fn bytes<'a>(&'a self, archive: &mut Archive, cancel: &AtomicBool) -> Result<Cow<'a, [u8]>> {
        match self {
            Self::Source {
                index, verified, ..
            } => {
                let bytes = archive.read_with_cancel(*index, codec::MAX, cancel)?;
                verified.set(true);
                Ok(Cow::Owned(bytes))
            }
            Self::Owned(bytes) => Ok(Cow::Borrowed(bytes)),
        }
    }
    fn validate_unused(&self, archive: &mut Archive, cancel: &AtomicBool) -> Result<()> {
        if matches!(self,Self::Source{verified,..} if !verified.get()) {
            self.copy(
                Some(archive),
                crate::container::STREAM_MEMBER_MAX,
                cancel,
                &mut std::io::sink(),
            )?;
        }
        Ok(())
    }
}
fn start_member(
    writer: &mut ZipWriter<Cursor<Vec<u8>>>,
    name: &str,
    size: usize,
    names: &mut BTreeSet<String>,
    total: &mut u64,
    cancel: &AtomicBool,
) -> Result<()> {
    codec::cancelled(cancel)?;
    safe_relative(name)?;
    ensure!(
        names.insert(normalized(name)),
        "Converted output name collision: {name}"
    );
    *total = total
        .checked_add(size as u64)
        .context("Converted size overflow")?;
    ensure!(
        *total <= OUTPUT_MAX,
        "Converted IFF exceeds 2 GiB safety limit"
    );
    writer.start_file(
        name,
        SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .compression_level(Some(6)),
    )?;
    Ok(())
}
struct CopyProgress<'a, W, F> {
    output: &'a mut W,
    progress: F,
    done: u64,
    last: std::time::Instant,
}
impl<W: Write, F: FnMut(u64)> Write for CopyProgress<'_, W, F> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let n = self.output.write(bytes)?;
        self.done += n as u64;
        if self.last.elapsed().as_millis() >= 35 {
            (self.progress)(self.done);
            self.last = std::time::Instant::now();
        }
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.output.flush()
    }
}
fn write_member(
    writer: &mut ZipWriter<Cursor<Vec<u8>>>,
    name: &str,
    data: &[u8],
    names: &mut BTreeSet<String>,
    total: &mut u64,
    cancel: &AtomicBool,
) -> Result<()> {
    start_member(writer, name, data.len(), names, total, cancel)?;
    for chunk in data.chunks(256 * 1024) {
        codec::cancelled(cancel)?;
        writer.write_all(chunk)?;
    }
    Ok(())
}
pub fn validate(
    archive: &mut Archive,
    cancel: &AtomicBool,
    mut progress: impl FnMut(f64),
) -> Result<usize> {
    let count = archive.shared()?.zip.len();
    let mut total = 0u64;
    let mut chunk = [0u8; 256 * 1024];
    for i in 0..count {
        codec::cancelled(cancel)?;
        let mut m = archive.shared()?.zip.by_index(i)?;
        ensure!(
            !m.encrypted() && m.size() <= crate::container::STREAM_MEMBER_MAX as u64,
            "Invalid converted member size or encryption"
        );
        safe_relative(m.name())?;
        let expected = m.size();
        let mut read = 0u64;
        loop {
            codec::cancelled(cancel)?;
            let n = m.read(&mut chunk)?;
            if n == 0 {
                break;
            }
            read += n as u64;
            ensure!(read <= expected, "Converted member size mismatch");
        }
        ensure!(read == expected, "Converted member truncated");
        total += read;
        ensure!(
            total <= OUTPUT_MAX,
            "Converted IFF exceeds 2 GiB safety limit"
        );
        progress((i + 1) as f64 / count.max(1) as f64);
    }
    Ok(count)
}

fn normalized(n: &str) -> String {
    n.replace('\\', "/").to_lowercase()
}
fn leaf(n: &str) -> &str {
    n.rsplit('/').next().unwrap_or(n)
}
fn renamed(n: &str, ext: &str) -> String {
    format!("{}.{ext}", n.rsplit_once('.').map(|p| p.0).unwrap_or(n))
}
#[cfg(test)]
fn legacy_resolve<'a>(catalog: &'a Catalog, name: &str) -> Result<&'a crate::catalog::Resource> {
    if let Ok(entry) = catalog.resolve(name) {
        return Ok(entry);
    }
    let normalized = normalized(name);
    let mut parts = leaf(&normalized).rsplit('.');
    let ext = parts.next().unwrap_or("");
    let hash = parts.next().context("Reference has no content hash")?;
    ensure!(
        hash.len() == 16 && hash.bytes().all(|n| n.is_ascii_hexdigit()),
        "Unresolved manifest reference: {name}"
    );
    let mapped: Value = serde_json::from_str(include_str!("../Data/renamed-mip-hashes.json"))?;
    let hash = if ext == "mip0" {
        mapped[hash].as_str().unwrap_or(hash)
    } else {
        hash
    };
    let suffix = format!(".{hash}.{ext}");
    let mut matches = catalog
        .entries
        .iter()
        .filter(|e| e.name.to_lowercase().ends_with(&suffix));
    let entry = matches
        .next()
        .context(format!("Unresolved manifest reference: {name}"))?;
    ensure!(
        matches.next().is_none(),
        "Ambiguous content-hash reference: {name}"
    );
    Ok(entry)
}

struct ManifestIndex {
    leaves: HashMap<String, Option<usize>>,
    suffixes: HashMap<String, Option<usize>>,
}
fn insert_unique(index: &mut HashMap<String, Option<usize>>, key: String, id: usize) {
    index
        .entry(key)
        .and_modify(|value| *value = None)
        .or_insert(Some(id));
}
impl ManifestIndex {
    fn build(catalog: &Catalog, cancel: &AtomicBool) -> Result<Self> {
        let mut index = Self {
            leaves: HashMap::new(),
            suffixes: HashMap::new(),
        };
        for (i, entry) in catalog.entries.iter().enumerate() {
            if i % 1024 == 0 {
                codec::cancelled(cancel)?;
            }
            // The previous leaf match is ASCII-case-insensitive. Hash fallback
            // used Unicode lowercase; retain those distinct matching rules.
            insert_unique(
                &mut index.leaves,
                leaf(&entry.name).to_ascii_lowercase(),
                entry.id,
            );
            let lower = entry.name.to_lowercase();
            let mut parts = leaf(&lower).rsplit('.');
            let ext = parts.next().unwrap_or("");
            if let Some(hash) = parts
                .next()
                .filter(|h| h.len() == 16 && h.bytes().all(|c| c.is_ascii_hexdigit()))
            {
                insert_unique(&mut index.suffixes, format!("{hash}.{ext}"), entry.id);
            }
        }
        Ok(index)
    }
}
/// Job-local names only, not an extracted-asset cache. Missing/ambiguous
/// references remain missing/ambiguous; indexing never changes their payloads.
pub struct DependencyResolver<'a> {
    catalog: &'a Catalog,
    cancel: &'a AtomicBool,
    index: Option<ManifestIndex>,
    renamed_mips: Value,
}
impl<'a> DependencyResolver<'a> {
    pub fn new(catalog: &'a Catalog, cancel: &'a AtomicBool) -> Result<Self> {
        Ok(Self {
            catalog,
            cancel,
            index: None,
            renamed_mips: serde_json::from_str(include_str!("../Data/renamed-mip-hashes.json"))?,
        })
    }
    fn resolve(&mut self, name: &str) -> Result<&'a crate::catalog::Resource> {
        codec::cancelled(self.cancel)?;
        let normalized = normalized(name);
        let basename = leaf(&normalized);
        let mut candidates = vec![normalized.clone()];
        if let Some(hash) = basename
            .rsplit('.')
            .nth(1)
            .filter(|h| h.len() == 16 && h.bytes().all(|c| c.is_ascii_hexdigit()))
        {
            candidates.push(format!("shared/{}/{basename}", &hash[..2]));
        }
        // Preserve the catalog's exact-path/shared-path priority.
        for candidate in candidates {
            if let Ok(position) = self.catalog.sorted.binary_search_by(|i| {
                crate::catalog::natural(&self.catalog.entries[*i].name, &candidate)
            }) {
                let entry = &self.catalog.entries[self.catalog.sorted[position]];
                if entry.name.eq_ignore_ascii_case(&candidate) {
                    return Ok(entry);
                }
            }
        }
        if self.index.is_none() {
            self.index = Some(ManifestIndex::build(self.catalog, self.cancel)?);
        }
        let index = self.index.as_ref().unwrap();
        if let Some(Some(id)) = index.leaves.get(basename) {
            return self.catalog.entry(*id);
        }
        let mut parts = basename.rsplit('.');
        let ext = parts.next().unwrap_or("");
        let hash = parts.next().context("Reference has no content hash")?;
        ensure!(
            hash.len() == 16 && hash.bytes().all(|c| c.is_ascii_hexdigit()),
            "Unresolved manifest reference: {name}"
        );
        let hash = if ext == "mip0" {
            self.renamed_mips[hash].as_str().unwrap_or(hash)
        } else {
            hash
        };
        let candidate = index
            .suffixes
            .get(&format!("{hash}.{ext}"))
            .context(format!("Unresolved manifest reference: {name}"))?;
        let id = candidate.context(format!("Ambiguous content-hash reference: {name}"))?;
        self.catalog.entry(id)
    }
}
fn refs(v: &Value, out: &mut BTreeMap<String, (String, Value)>, depth: usize) -> Result<()> {
    ensure!(depth <= 128, "Scene dependency nesting exceeds limit");
    if let Some(o) = v.as_object() {
        if let Some(n) = o.get("Binary").and_then(Value::as_str) {
            safe_relative(n)?;
            let key = normalized(n);
            merge_reference(out, key, n.replace('\\', "/"), v.clone())?;
        }
        for child in o.values() {
            refs(child, out, depth + 1)?;
        }
    }
    if let Some(a) = v.as_array() {
        for child in a {
            refs(child, out, depth + 1)?;
        }
    }
    Ok(())
}
fn merge_reference(
    out: &mut BTreeMap<String, (String, Value)>,
    key: String,
    name: String,
    value: Value,
) -> Result<()> {
    if let Some((_, old)) = out.get_mut(&key) {
        let previous = old.clone();
        if old["Segments"].as_array().map(Vec::len).unwrap_or(0)
            < value["Segments"].as_array().map(Vec::len).unwrap_or(0)
        {
            *old = value.clone();
        }
        for field in ["Size", "PixelDataSize"] {
            let a = previous[field].as_u64().filter(|n| *n > 0);
            let b = value[field].as_u64().filter(|n| *n > 0);
            ensure!(
                a.is_none() || b.is_none() || a == b,
                "Conflicting authored {field} for {name}"
            );
            if let Some(size) = a.or(b) {
                old[field] = json!(size);
            }
        }
    } else {
        out.insert(key, (name, value));
    }
    Ok(())
}
#[derive(Default)]
struct MemberIndex {
    leaves: HashMap<String, Option<String>>,
}
impl MemberIndex {
    fn build<T>(members: &BTreeMap<String, (String, T)>) -> Self {
        let mut index = Self::default();
        for key in members.keys() {
            index.insert(key);
        }
        index
    }
    fn insert(&mut self, key: &str) {
        self.leaves
            .entry(leaf(key).to_owned())
            .and_modify(|existing| {
                if existing.as_deref() != Some(key) {
                    *existing = None;
                }
            })
            .or_insert_with(|| Some(key.to_owned()));
    }
}
fn find_indexed<'a, T>(
    members: &'a BTreeMap<String, (String, T)>,
    index: &MemberIndex,
    name: &str,
) -> Result<Option<&'a String>> {
    let key = normalized(name);
    if let Some((key, _)) = members.get_key_value(&key) {
        return Ok(Some(key));
    }
    match index.leaves.get(leaf(&key)) {
        None => Ok(None),
        Some(None) => anyhow::bail!("Ambiguous embedded dependency: {name}"),
        Some(Some(found)) => Ok(members.get_key_value(found).map(|(key, _)| key)),
    }
}
#[cfg(test)]
fn find<'a>(
    members: &'a BTreeMap<String, (String, Vec<u8>)>,
    name: &str,
) -> Result<Option<&'a String>> {
    find_indexed(members, &MemberIndex::build(members), name)
}
/// Some authored scenes retain a GZ reference after its buffer was stored as
/// BIN in the same IFF. Accept only the same hashed filename and exact authored
/// decoded length; never infer a replacement from shape, size alone or preview.
fn embedded_bin_indexed<'a, T: MemberData>(
    members: &'a BTreeMap<String, (String, T)>,
    index: &MemberIndex,
    name: &str,
    info: &Value,
) -> Result<Option<&'a String>> {
    let key = normalized(name);
    if !key.ends_with(".gz") || find_indexed(members, index, name)?.is_some() {
        return Ok(None);
    }
    let hashed = leaf(&key)
        .rsplit('.')
        .nth(1)
        .is_some_and(|h| h.len() == 16 && h.bytes().all(|c| c.is_ascii_hexdigit()));
    let authored = info["Size"].as_u64().filter(|n| *n > 0);
    let pixels = info["PixelDataSize"].as_u64().filter(|n| *n > 0);
    let Some(size) = authored.or(pixels) else {
        return Ok(None);
    };
    if authored.zip(pixels).is_some_and(|(a, b)| a != b) {
        return Ok(None);
    }
    if !hashed {
        return Ok(None);
    }
    let Some(found) = find_indexed(members, index, &renamed(&key, "bin"))? else {
        return Ok(None);
    };
    let bytes = &members[found].1;
    if bytes.len() as u64 != size
        || codec::vcz(bytes.prefix())
        || bytes.prefix().starts_with(&[0x1f, 0x8b])
    {
        return Ok(None);
    }
    Ok(Some(found))
}
#[cfg(test)]
fn embedded_bin<'a>(
    members: &'a BTreeMap<String, (String, Vec<u8>)>,
    name: &str,
    info: &Value,
) -> Result<Option<&'a String>> {
    embedded_bin_indexed(members, &MemberIndex::build(members), name, info)
}
fn pack(
    members: impl IntoIterator<Item = (String, Vec<u8>)>,
    cancel: &AtomicBool,
    mut progress: impl FnMut(f64),
) -> Result<Vec<u8>> {
    let entries: Vec<_> = members.into_iter().collect();
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(6));
    let mut names = BTreeSet::new();
    let mut size = 0usize;
    for (i, (name, bytes)) in entries.iter().enumerate() {
        codec::cancelled(cancel)?;
        safe_relative(name)?;
        ensure!(
            names.insert(normalized(name)),
            "Converted output name collision: {name}"
        );
        size = size
            .checked_add(bytes.len())
            .context("Export size overflow")?;
        ensure!(size <= codec::MAX, "Converted IFF exceeds 512 MiB limit");
        writer.start_file(name, options)?;
        writer.write_all(bytes)?;
        progress((i + 1) as f64 / entries.len().max(1) as f64);
    }
    Ok(writer.finish()?.into_inner())
}
pub fn inspection_container(name: &str, bytes: Vec<u8>, cancel: &AtomicBool) -> Result<Vec<u8>> {
    pack([(name.into(), bytes)], cancel, |_| {})
}
pub fn single(
    name: &str,
    bytes: Vec<u8>,
    catalog: &Catalog,
    oodle: &Oodle,
    cancel: &AtomicBool,
) -> Result<(String, Vec<u8>, Vec<Value>)> {
    let lower = normalized(name);
    if lower.ends_with(".scne") || lower.ends_with(".txtr") {
        let bytes = if codec::vcz(&bytes) {
            crate::buffer_decode::decode(&bytes, None, oodle, cancel)?
        } else {
            bytes
        };
        return Ok((
            name.into(),
            codec::document(&bytes, &BTreeMap::new())?,
            vec![],
        ));
    }
    if lower.ends_with(".tld") {
        let container = pack([(name.into(), bytes.clone())], cancel, |_| {})?;
        let mut archive = Archive::open(container)?;
        match textures::reconstruct(
            &mut archive,
            catalog,
            oodle,
            name,
            &Value::Null,
            cancel,
            codec::MAX,
        ) {
            Ok(dds) => return Ok((renamed(name, "dds"), dds, vec![])),
            Err(e) => {
                return Ok((
                    name.into(),
                    bytes,
                    vec![
                        json!({"name":name,"reason":format!("Texture preserved unchanged: {e:#}")}),
                    ],
                ));
            }
        }
    }
    if lower.ends_with(".gz") {
        return match crate::buffer_decode::decode(&bytes, None, oodle, cancel) {
            Ok(decoded) => Ok((renamed(name, "bin"), decoded, vec![])),
            Err(e) => Ok((
                name.into(),
                bytes,
                vec![json!({"name":name,"reason":format!("Buffer preserved unchanged: {e:#}")})],
            )),
        };
    }
    Ok((name.into(), bytes, vec![]))
}
/// Preserve the original BTreeMap iteration's first-leaf fallback without
/// re-normalizing/scanning every reference for every (mostly binary) member.
fn metadata_by_leaf(references: &BTreeMap<String, (String, Value)>) -> HashMap<String, &Value> {
    let mut index = HashMap::new();
    for (name, info) in references.values() {
        index
            .entry(leaf(&normalized(name)).to_owned())
            .or_insert(info);
    }
    index
}
/// The shared decoder reads only the requested TLD and its optional mip0.
/// Keep its original first-in-archive, ASCII-insensitive lookup policy, but
/// provide those exact inputs in a tiny private archive instead of making
/// every texture scan thousands of unrelated ZIP entries. Output is unchanged.
struct TextureSources {
    keys: Vec<String>,
    names: HashMap<String, usize>,
}
impl TextureSources {
    fn new<T>(members: &BTreeMap<String, (String, T)>, cancel: &AtomicBool) -> Result<Self> {
        let mut keys = Vec::with_capacity(members.len());
        let mut names = HashMap::with_capacity(members.len());
        for (key, (name, _)) in members {
            codec::cancelled(cancel)?;
            names.entry(name.to_ascii_lowercase()).or_insert(keys.len());
            keys.push(key.clone());
        }
        Ok(Self { keys, names })
    }
    fn locate(&self, name: &str) -> Option<usize> {
        let name = name.replace('\\', "/");
        let leaf = leaf(&name);
        let alias = if leaf.to_ascii_lowercase().ends_with(".gz") {
            format!("{}.bin", &leaf[..leaf.len() - 3])
        } else if leaf.to_ascii_lowercase().ends_with(".bin") {
            format!("{}.gz", &leaf[..leaf.len() - 4])
        } else {
            String::new()
        };
        // The old predicate checks all candidates at each archive position;
        // an earlier leaf/alias wins even if an exact path occurs later.
        [name.as_str(), leaf, alias.as_str()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .filter_map(|s| self.names.get(&s.to_ascii_lowercase()).copied())
            .min()
    }
    fn decoder_input<T: MemberData>(
        &self,
        members: &BTreeMap<String, (String, T)>,
        name: &str,
        cancel: &AtomicBool,
        mut archive: Option<&mut Archive>,
    ) -> Result<Archive> {
        codec::cancelled(cancel)?;
        let mip = format!("{}.mip0", name.strip_suffix(".tld").unwrap_or(name));
        let mut selected: Vec<_> = [name, &mip]
            .into_iter()
            .filter_map(|n| self.locate(n))
            .collect();
        selected.sort_unstable();
        selected.dedup();
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let mut total = 0usize;
        for index in selected {
            codec::cancelled(cancel)?;
            let (name, data) = &members[&self.keys[index]];
            writer.start_file(
                name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )?;
            total = total
                .checked_add(data.len())
                .context("Texture inputs overflow")?;
            ensure!(
                total <= crate::container::MAX,
                "Texture inputs exceed retained-memory budget"
            );
            data.copy(archive.as_deref_mut(), codec::MAX, cancel, &mut writer)?;
        }
        Archive::open(writer.finish()?.into_inner())
    }
}
pub fn convert(
    bytes: Vec<u8>,
    catalog: &Catalog,
    oodle: &Oodle,
    cancel: &AtomicBool,
    manifest: &mut DependencyResolver<'_>,
    mut progress: impl FnMut(&str, f64),
) -> Result<(Vec<u8>, Value)> {
    let mut archive = Archive::open_with_cancel(bytes, cancel)?;
    archive.require_unique_members()?;
    let names = archive.names()?;
    let mut members = BTreeMap::<String, (String, Payload)>::new();
    let mut references = BTreeMap::new();
    let mut warnings = Vec::new();
    let mut total = 0u64;
    let mut retained = 0usize;
    for (i, m) in names.iter().enumerate() {
        codec::cancelled(cancel)?;
        let name = m["name"]
            .as_str()
            .context("Member name missing")?
            .replace('\\', "/");
        let source_index = m["id"].as_u64().context("Missing source member ID")? as usize;
        let source_size =
            usize::try_from(m["size"].as_u64().context("Missing source member size")?)?;
        ensure!(
            source_size <= crate::container::STREAM_MEMBER_MAX,
            "Source member exceeds streaming size limit"
        );
        if name.ends_with('/') {
            total = total
                .checked_add(source_size as u64)
                .context("Directory size overflow")?;
            ensure!(
                total <= OUTPUT_MAX,
                "IFF members exceed 2 GiB expanded-data budget"
            );
            archive.copy_member(
                source_index,
                crate::container::STREAM_MEMBER_MAX,
                cancel,
                &mut std::io::sink(),
            )?;
            continue;
        }
        safe_relative(&name)?;
        let key = normalized(&name);
        progress(
            &format!("Reading IFF member: {name}"),
            0.2 * i as f64 / names.len().max(1) as f64,
        );
        ensure!(
            !members.contains_key(&key),
            "Duplicate case-insensitive IFF member: {name}"
        );
        let document = key.ends_with(".scne") || key.ends_with(".txtr");
        let mut data = if document {
            Payload::Owned(archive.read_with_cancel(source_index, codec::MAX, cancel)?)
        } else {
            Payload::Source {
                index: source_index,
                size: source_size,
                prefix: archive.prefix(source_index, 16, cancel)?,
                verified: Cell::new(false),
            }
        };
        if let Payload::Owned(bytes) = &mut data {
            if codec::vcz(bytes) {
                match crate::buffer_decode::decode(&bytes, None, oodle, cancel) {
                Ok(decoded) => *bytes = decoded,
                Err(e) => warnings.push(json!({"name":name,"reason":format!("Compressed document preserved unchanged: {e:#}")})),
            }
            }
        }
        total = total
            .checked_add(data.len() as u64)
            .context("Member total overflow")?;
        ensure!(
            total <= OUTPUT_MAX,
            "IFF members exceed 2 GiB expanded-data budget"
        );
        if key.ends_with(".scne") || key.ends_with(".txtr") {
            let bytes = data.bytes(&mut archive, cancel)?;
            match codec::tree_exact(&bytes) {
                Ok(tree) => refs(&tree, &mut references, 0)?,
                Err(e) => warnings.push(
                    json!({"name":name,"reason":format!("Document preserved unchanged: {e:#}")}),
                ),
            };
        }
        if let Payload::Owned(bytes) = &data {
            retained = retained
                .checked_add(bytes.len())
                .context("Retained data overflow")?;
        }
        ensure!(
            retained <= crate::container::MAX,
            "Retained conversion data exceeds 1 GiB memory budget"
        );
        members.insert(key, (name, data));
        progress(
            "Reading and checking IFF members",
            0.2 * (i + 1) as f64 / names.len().max(1) as f64,
        );
    }
    // Unchanged members refer to this immutable container; never materialize
    // their entire expanded payloads just to recompress them later.
    // Keep exact-path priority and reject ambiguous leaves, without rescanning
    // every member for each external reference. Update as the closure grows.
    let mut member_index = MemberIndex::build(&members);
    let mut queue: VecDeque<_> = references.values().cloned().collect();
    let mut visited = BTreeSet::new();
    let mut dependencies = 0usize;
    let mut embedded_renames = BTreeMap::new();
    while let Some((name, info)) = queue.pop_front() {
        codec::cancelled(cancel)?;
        progress(
            &format!("Resolving dependency: {name}"),
            0.2 + 0.25 * visited.len() as f64 / (visited.len() + queue.len() + 1).max(1) as f64,
        );
        let key = normalized(&name);
        if !visited.insert(key.clone()) {
            continue;
        }
        ensure!(
            visited.len() <= 100_000,
            "Dependency closure exceeds 100,000 references"
        );
        // External mip streams are retained if any texture is unsupported; never discard data.
        if key.ends_with(".tld") {
            queue.push_back((renamed(&name, "mip0"), Value::Null));
        }
        if find_indexed(&members, &member_index, &name)?.is_some() {
            continue;
        }
        if let Some(existing) = embedded_bin_indexed(&members, &member_index, &name, &info)? {
            embedded_renames.insert(key, members[existing].0.clone());
            continue;
        }
        let entry = match manifest.resolve(&name) {
            Ok(e) => e,
            Err(e) => {
                if !key.ends_with(".mip0") {
                    warnings.push(
                        json!({"name":name,"reason":format!("Unresolved dependency: {e:#}")}),
                    );
                }
                continue;
            }
        };
        let mut data = codec::range(catalog, entry)?;
        if (key.ends_with(".scne") || key.ends_with(".txtr")) && codec::vcz(&data) {
            let expected = info["Size"]
                .as_u64()
                .filter(|n| *n > 0)
                .map(|n| usize::try_from(n))
                .transpose()?;
            match crate::buffer_decode::decode(&data, expected, oodle, cancel) {
                Ok(decoded) => data = decoded,
                Err(e) => warnings.push(json!({"name":name,"reason":format!("Compressed dependency document preserved: {e:#}")})),
            }
        }
        total = total
            .checked_add(data.len() as u64)
            .context("Dependency total overflow")?;
        ensure!(
            total <= OUTPUT_MAX,
            "IFF plus dependencies exceeds 2 GiB expanded-data budget"
        );
        dependencies += 1;
        if key.ends_with(".scne") || key.ends_with(".txtr") {
            match codec::tree_exact(&data) {
                Ok(tree) => {
                    let mut children = BTreeMap::new();
                    refs(&tree, &mut children, 0)?;
                    for (k, (child_name, child_info)) in children {
                        queue.push_back((child_name.clone(), child_info.clone()));
                        merge_reference(&mut references, k, child_name, child_info)?;
                    }
                }
                Err(e) => warnings.push(json!({"name":name,"reason":format!("Dependency document preserved unchanged: {e:#}")})),
            }
        }
        merge_reference(&mut references, key.clone(), name.clone(), info)?;
        member_index.insert(&key);
        retained = retained
            .checked_add(data.len())
            .context("Retained dependency overflow")?;
        ensure!(
            retained <= crate::container::MAX,
            "Retained conversion data exceeds 1 GiB memory budget"
        );
        members.insert(key, (name, Payload::Owned(data)));
        progress(
            "Resolving shared dependency closure",
            0.2 + 0.25 * visited.len() as f64 / (visited.len() + queue.len()).max(1) as f64,
        );
    }
    progress("Indexing resolved texture inputs", 0.45);
    let texture_sources = if members.keys().any(|name| name.ends_with(".tld")) {
        Some(TextureSources::new(&members, cancel)?)
    } else {
        None
    };
    // Resolve aliases once, not once for every converted member (quadratic on arenas).
    let mut aliases = BTreeMap::<String, Vec<(String, String)>>::new();
    progress("Indexing resolved reference aliases", 0.45);
    for (ref_key, (ref_name, _)) in &references {
        if let Some(key) = find_indexed(&members, &member_index, ref_name)? {
            aliases
                .entry(key.clone())
                .or_default()
                .push((ref_key.clone(), ref_name.clone()));
        }
    }
    let embedded_buffers_repaired = embedded_renames.len();
    let mut renames = embedded_renames;
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let mut output_names = BTreeSet::new();
    let mut output_size = 0u64;
    let (mut textures_count, mut buffers_count, mut documents_count) = (0, 0, 0);
    let metadata_by_leaf = metadata_by_leaf(&references);
    for (i, (key, (name, data))) in members.iter().enumerate() {
        codec::cancelled(cancel)?;
        progress(
            &format!("Converting member: {name}"),
            0.45 + 0.25 * i as f64 / members.len().max(1) as f64,
        );
        if key.ends_with(".scne") || key.ends_with(".txtr") {
            continue;
        }
        let mut output_name = name.clone();
        let mut converted = None;
        // Ordinary BIN/DDS/mip members never consume descriptor metadata.
        let info = if key.ends_with(".tld") || key.ends_with(".gz") {
            references
                .get(key)
                .map(|v| &v.1)
                .or_else(|| metadata_by_leaf.get(leaf(key)).copied())
                .unwrap_or(&Value::Null)
        } else {
            &Value::Null
        };
        if key.ends_with(".tld") {
            let mut source = texture_sources
                .as_ref()
                .context("Missing texture input index")?
                .decoder_input(&members, name, cancel, Some(&mut archive))?;
            match textures::reconstruct(&mut source,catalog,oodle,name,info,cancel,codec::MAX){
                Ok(dds)=>{output_name=renamed(name,"dds");converted=Some(dds);textures_count+=1;},
                Err(e)=>warnings.push(json!({"name":name,"reason":format!("Texture and authored reference preserved: {e:#}")}))
            }
        } else if key.ends_with(".gz") {
            let expected = info["Size"]
                .as_u64()
                .filter(|n| *n > 0)
                .or_else(|| info["PixelDataSize"].as_u64().filter(|n| *n > 0))
                .map(usize::try_from)
                .transpose()?;
            let decoded_result = data
                .bytes(&mut archive, cancel)
                .and_then(|bytes| crate::buffer_decode::decode(&bytes, expected, oodle, cancel));
            match decoded_result {
                Ok(decoded) => {
                    output_name = renamed(name, "bin");
                    converted = Some(decoded);
                    buffers_count += 1;
                }
                Err(e) => warnings.push(
                    json!({"name":name,"reason":format!("Buffer preserved unchanged: {e:#}")}),
                ),
            }
        }
        if output_name != *name {
            renames.insert(key.clone(), output_name.clone());
            if let Some(refs) = aliases.get(key) {
                for (ref_key, ref_name) in refs {
                    renames.insert(
                        ref_key.clone(),
                        renamed(ref_name, if key.ends_with(".tld") { "dds" } else { "bin" }),
                    );
                }
            }
        }
        if let Some(bytes) = converted.as_deref() {
            write_member(
                &mut writer,
                &output_name,
                bytes,
                &mut output_names,
                &mut output_size,
                cancel,
            )?;
        } else {
            start_member(
                &mut writer,
                &output_name,
                data.len(),
                &mut output_names,
                &mut output_size,
                cancel,
            )?;
            let mut copying = CopyProgress {
                output: &mut writer,
                done: 0,
                last: std::time::Instant::now(),
                progress: |done| {
                    progress(
                        "Streaming unchanged member data",
                        0.45 + 0.25 * (i as f64 + done as f64 / data.len().max(1) as f64)
                            / members.len().max(1) as f64,
                    )
                },
            };
            data.copy(
                Some(&mut archive),
                crate::container::STREAM_MEMBER_MAX,
                cancel,
                &mut copying,
            )?;
        }
        progress(
            "Converting authored textures and buffers",
            0.45 + 0.25 * (i + 1) as f64 / members.len().max(1) as f64,
        );
    }
    for (name, data) in members.values() {
        let key = normalized(name);
        if key.ends_with(".scne") || key.ends_with(".txtr") {
            progress(&format!("Rewriting document: {name}"), 0.7);
            let data = data.bytes(&mut archive, cancel)?;
            let converted = codec::document(&data, &renames).ok();
            if converted.is_some() {
                documents_count += 1;
            }
            write_member(
                &mut writer,
                name,
                converted.as_deref().unwrap_or(&data),
                &mut output_names,
                &mut output_size,
                cancel,
            )?;
        }
    }
    // A first-leaf texture alias can consume a different source TLD. The old
    // eager path checked every original member; keep that invariant even for
    // source bytes not selected by a successful transformation.
    for (_, data) in members.values() {
        codec::cancelled(cancel)?;
        data.validate_unused(&mut archive, cancel)?;
    }
    progress("Finishing converted IFF", 0.95);
    let output = writer.finish()?.into_inner();
    progress("Converted IFF packed", 1.);
    let stats = json!({"texturesConverted":textures_count,"buffersConverted":buffers_count,"embeddedBufferReferencesRepaired":embedded_buffers_repaired,"documentsConverted":documents_count,"dependenciesPacked":dependencies,"warnings":warnings});
    Ok((output, stats))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn borrowed_payload_repacking_is_byte_exact_and_source_is_immutable() {
        let cancel = AtomicBool::new(false);
        let bytes: Vec<u8> = (0..700_001usize)
            .map(|i| (i.wrapping_mul(17) % 251) as u8)
            .collect();
        let original = pack([("buffer.bin".into(), bytes.clone())], &cancel, |_| {}).unwrap();
        let mut source = Archive::open(original.clone()).unwrap();
        let payload = Payload::Source {
            index: 0,
            size: bytes.len(),
            prefix: source.prefix(0, 16, &cancel).unwrap(),
            verified: Cell::new(false),
        };
        let mut old = ZipWriter::new(Cursor::new(Vec::new()));
        write_member(
            &mut old,
            "buffer.bin",
            &bytes,
            &mut BTreeSet::new(),
            &mut 0,
            &cancel,
        )
        .unwrap();
        let mut streamed = ZipWriter::new(Cursor::new(Vec::new()));
        start_member(
            &mut streamed,
            "buffer.bin",
            payload.len(),
            &mut BTreeSet::new(),
            &mut 0,
            &cancel,
        )
        .unwrap();
        payload
            .copy(
                Some(&mut source),
                crate::container::STREAM_MEMBER_MAX,
                &cancel,
                &mut streamed,
            )
            .unwrap();
        assert_eq!(
            old.finish().unwrap().into_inner(),
            streamed.finish().unwrap().into_inner()
        );
        assert_eq!(source.into_bytes(), original);
    }
    #[test]
    fn texture_alias_does_not_hide_unselected_corrupt_source() {
        let cancel = AtomicBool::new(false);
        let mut original = crate::iff_directory::tests::fixture(&[
            ("texture.tld", b"aaaaaaaaaaaaaaaagood"),
            ("z/texture.tld", b"bbbbbbbbbbbbbbbbtail"),
        ]);
        let mut view = Archive::open(original.clone()).unwrap();
        let bad_offset = view.member(1).unwrap().data_offset as usize;
        original[bad_offset] ^= 1;
        let mut source = Archive::open(original.clone()).unwrap();
        let mut members = BTreeMap::new();
        for index in 0..2 {
            let member = source.member(index).unwrap();
            let payload = Payload::Source {
                index,
                size: member.size as usize,
                prefix: source.prefix(index, 16, &cancel).unwrap(),
                verified: Cell::new(false),
            };
            members.insert(normalized(&member.name), (member.name, payload));
        }
        let inputs = TextureSources::new(&members, &cancel).unwrap();
        let mut selected = inputs
            .decoder_input(&members, "z/texture.tld", &cancel, Some(&mut source))
            .unwrap();
        assert_eq!(
            selected.named("texture.tld").unwrap(),
            b"aaaaaaaaaaaaaaaagood"
        );
        members["texture.tld"]
            .1
            .validate_unused(&mut source, &cancel)
            .unwrap();
        assert!(
            members["z/texture.tld"]
                .1
                .validate_unused(&mut source, &cancel)
                .is_err()
        );
        assert_eq!(source.into_bytes(), original);
    }
    #[test]
    fn borrowed_buffers_retain_header_repair_guards_and_copy_cancellation() {
        let cancel = AtomicBool::new(false);
        let name = "VertexBuffer.0123456789abcdef.bin";
        let original = crate::iff_directory::tests::fixture(&[(name, b"plain authored buffer")]);
        let mut source = Archive::open(original.clone()).unwrap();
        let payload = Payload::Source {
            index: 0,
            size: 21,
            prefix: source.prefix(0, 16, &cancel).unwrap(),
            verified: Cell::new(false),
        };
        let members = BTreeMap::from([(normalized(name), (name.to_string(), payload))]);
        let index = MemberIndex::build(&members);
        assert!(
            embedded_bin_indexed(
                &members,
                &index,
                "VertexBuffer.0123456789abcdef.gz",
                &json!({"Size":21})
            )
            .unwrap()
            .is_some()
        );
        assert!(
            embedded_bin_indexed(
                &members,
                &index,
                "VertexBuffer.0123456789abcdef.gz",
                &json!({"Size":20})
            )
            .unwrap()
            .is_none()
        );
        cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        let mut output = Vec::new();
        assert!(
            members[&normalized(name)]
                .1
                .copy(Some(&mut source), codec::MAX, &cancel, &mut output)
                .is_err()
        );
        assert!(output.is_empty());
        assert_eq!(source.into_bytes(), original);
    }
    #[test]
    fn bounded_texture_inputs_match_shared_first_archive_match() {
        let cancel = AtomicBool::new(false);
        let names = [
            "a/texture.tld",
            "buffer.bin",
            "buffer.gz",
            "i.tld",
            "texture.mip0",
            "texture.tld",
            "z/texture.tld",
            "İ.tld",
        ];
        let members: BTreeMap<_, _> = names
            .into_iter()
            .enumerate()
            .map(|(n, name)| (name.to_owned(), (name.to_owned(), vec![n as u8; n + 1])))
            .collect();
        let before = members.clone();
        let sources = TextureSources::new(&members, &cancel).unwrap();
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, data) in members.values() {
            writer
                .start_file(
                    name,
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .unwrap();
            writer.write_all(data).unwrap();
        }
        let mut legacy =
            nba2k_assets::codec::Archive::open(writer.finish().unwrap().into_inner()).unwrap();
        for query in [
            "texture.tld",
            "a/texture.tld",
            "z/texture.tld",
            "Z\\TEXTURE.TLD",
            "unknown/texture.tld",
            "texture.mip0",
            "buffer.gz",
            "unknown/buffer.gz",
            "BUFFER.BIN",
            "i.tld",
            "İ.tld",
            "missing.tld",
        ] {
            let mut tiny = sources
                .decoder_input(&members, query, &cancel, None)
                .unwrap();
            assert!(tiny.indexed_member_count() <= 2);
            assert_eq!(tiny.named(query).ok(), legacy.named(query).ok(), "{query}");
            let mip = format!("{}.mip0", query.strip_suffix(".tld").unwrap_or(query));
            assert_eq!(
                tiny.named(&mip).ok(),
                legacy.named(&mip).ok(),
                "mip for {query}"
            );
        }
        assert_eq!(members, before);
        cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(TextureSources::new(&members, &cancel).is_err());
        assert!(
            sources
                .decoder_input(&members, "missing.tld", &cancel, None)
                .is_err()
        );
    }
    #[test]
    fn bounded_texture_inputs_keep_earlier_leaf_over_later_exact_path() {
        let members = BTreeMap::from([
            ("0".into(), ("texture.tld".into(), vec![1])),
            ("1".into(), ("folder/texture.tld".into(), vec![2])),
        ]);
        let cancel = AtomicBool::new(false);
        let sources = TextureSources::new(&members, &cancel).unwrap();
        let mut input = sources
            .decoder_input(&members, "folder/texture.tld", &cancel, None)
            .unwrap();
        assert_eq!(input.named("folder/texture.tld").unwrap(), [1]);
    }
    #[test]
    fn binary_document_keeps_full_source_double_precision() {
        // A one-element root array: the binary holds an IEEE double, not a
        // nine-significant-digit decimal. Export must not round it to G9.
        let source = 2.4394078400000003e-10_f64;
        let mut bytes = vec![0u8; 109];
        bytes[0..4].copy_from_slice(&0xe01ff891u32.to_le_bytes());
        bytes[4..8].copy_from_slice(&1u32.to_le_bytes());
        bytes[8..12].copy_from_slice(&5u32.to_le_bytes());
        bytes[12..16].copy_from_slice(&2u32.to_le_bytes());
        bytes[16..24].copy_from_slice(&89u64.to_le_bytes());
        bytes[28..32].copy_from_slice(&0x40000002u32.to_le_bytes());
        bytes[36..40].copy_from_slice(&1u32.to_le_bytes());
        bytes[68..72].copy_from_slice(&0x40000005u32.to_le_bytes());
        bytes[80..88].copy_from_slice(&source.to_le_bytes());
        bytes[104..109].copy_from_slice(b"root\0");
        let original = bytes.clone();
        let output = codec::document(&bytes, &BTreeMap::new()).unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "\"root\": [\r\n  2.4394078400000003e-10\r\n]"
        );
        assert_eq!(
            codec::tree_exact(&bytes).unwrap()["root"][0]
                .as_f64()
                .unwrap()
                .to_bits(),
            source.to_bits()
        );
        assert_eq!(bytes, original);
    }
    #[test]
    fn member_index_matches_linear_lookup_and_updates_ambiguity() {
        fn old_find(
            members: &BTreeMap<String, (String, Vec<u8>)>,
            name: &str,
        ) -> Result<Option<String>> {
            let key = normalized(name);
            if members.contains_key(&key) {
                return Ok(Some(key));
            }
            let mut matches = members.keys().filter(|k| leaf(k) == leaf(&key));
            let first = matches.next().cloned();
            ensure!(
                matches.next().is_none(),
                "Ambiguous embedded dependency: {name}"
            );
            Ok(first)
        }
        let mut members = BTreeMap::new();
        let mut index = MemberIndex::default();
        for key in [
            "a/x.bin",
            "a/unique.bin",
            "b/x.bin",
            "deep/folder/new.bin",
            "a/x.bin",
        ] {
            members.insert(key.into(), (key.into(), vec![1, 2]));
            index.insert(key);
            for query in [
                "x.bin",
                "a/x.bin",
                "A\\X.BIN",
                "other/x.bin",
                "unique.bin",
                "new.bin",
                "missing.bin",
            ] {
                let expected = old_find(&members, query);
                let actual = find_indexed(&members, &index, query).map(|key| key.cloned());
                assert_eq!(
                    actual.as_ref().ok(),
                    expected.as_ref().ok(),
                    "{key}: {query}"
                );
                assert_eq!(
                    actual.err().map(|e| e.to_string()),
                    expected.err().map(|e| e.to_string())
                );
            }
        }
        assert_eq!(index.leaves, MemberIndex::build(&members).leaves);
        assert_eq!(members["a/x.bin"].1, [1, 2]);
    }
    #[test]
    fn indexed_descriptor_fallback_preserves_original_first_match_and_exact_priority() {
        let references = BTreeMap::from([
            (
                "a/floor.tld".into(),
                ("A/FLOOR.TLD".into(), json!({"Size":1})),
            ),
            (
                "b/floor.tld".into(),
                ("B/floor.tld".into(), json!({"Size":2})),
            ),
            (
                "c/vertex.gz".into(),
                ("C/Vertex.GZ".into(), json!({"Size":3})),
            ),
        ]);
        let indexed = metadata_by_leaf(&references);
        for query in [
            "a/floor.tld",
            "b/floor.tld",
            "other/floor.tld",
            "vertex.gz",
            "missing.tld",
            "plain.bin",
        ] {
            let old = references.get(query).map(|v| &v.1).or_else(|| {
                references
                    .values()
                    .find(|(n, _)| leaf(&normalized(n)) == leaf(query))
                    .map(|v| &v.1)
            });
            let new = references
                .get(query)
                .map(|v| &v.1)
                .or_else(|| indexed.get(leaf(query)).copied());
            assert_eq!(old, new, "{query}");
        }
        assert_eq!(indexed["floor.tld"]["Size"], 1);
        assert_eq!(references["b/floor.tld"].1["Size"], 2);
        assert_eq!(metadata_by_leaf(&BTreeMap::new()).len(), 0);
    }
    #[test]
    fn embedded_bin_repairs_only_verified_hashed_authored_references() {
        let name = "VertexBuffer.0123456789abcdef.gz";
        let output = "VertexBuffer.0123456789abcdef.bin";
        let members = BTreeMap::from([(normalized(output), (output.into(), vec![1, 2, 3, 4]))]);
        let info = json!({"Binary":name,"Size":4,"CompressionMethod":33,"Stride":8});
        let key = embedded_bin(&members, name, &info).unwrap().unwrap();
        let renames = BTreeMap::from([(normalized(name), members[key].0.clone())]);
        let rewritten = codec::document(&serde_json::to_vec(&info).unwrap(), &renames).unwrap();
        let mut expected = info.clone();
        expected["Binary"] = json!(output);
        assert_eq!(codec::tree_exact(&rewritten).unwrap(), expected);
        assert_eq!(members[key].1, [1, 2, 3, 4]);
        for metadata in [Value::Null, json!({"Size":0}), json!({"Size":3})] {
            assert!(embedded_bin(&members, name, &metadata).unwrap().is_none());
        }
        for metadata in [
            json!({"PixelDataSize":4}),
            json!({"Size":4,"PixelDataSize":4}),
        ] {
            assert!(embedded_bin(&members, name, &metadata).unwrap().is_some());
        }
        assert!(
            embedded_bin(&members, name, &json!({"Size":4,"PixelDataSize":3}))
                .unwrap()
                .is_none()
        );
        assert!(
            embedded_bin(&members, "VertexBuffer.gz", &info)
                .unwrap()
                .is_none()
        );
        assert!(
            embedded_bin(&members, "VertexBuffer.1111111111111111.gz", &info)
                .unwrap()
                .is_none()
        );
        let mut compressed = members.clone();
        compressed.insert(normalized(name), (name.into(), vec![9]));
        assert!(embedded_bin(&compressed, name, &info).unwrap().is_none());
        let mut ambiguous = members.clone();
        ambiguous.insert(
            format!("other/{}", normalized(output)),
            (format!("other/{output}"), vec![1, 2, 3, 4]),
        );
        assert!(embedded_bin(&ambiguous, &format!("missing/{name}"), &info).is_err());
        let wrapped =
            BTreeMap::from([(normalized(output), (output.into(), vec![0x1f, 0x8b, 0, 0]))]);
        assert!(embedded_bin(&wrapped, name, &info).unwrap().is_none());
    }
    fn catalog_fixture(names: &[&str]) -> Catalog {
        let entries: Vec<_> = names
            .iter()
            .enumerate()
            .map(|(id, name)| crate::catalog::Resource {
                id,
                name: (*name).into(),
                archive: "0A".into(),
                offset: 0,
                size: 1,
            })
            .collect();
        Catalog::from_entries(
            std::path::PathBuf::new(),
            entries,
            0,
            &AtomicBool::new(false),
        )
        .unwrap()
    }
    #[test]
    fn indexed_resolver_preserves_exact_leaf_hash_and_ambiguity_rules() {
        let catalog = catalog_fixture(&[
            "a/plain.bin",
            "b/plain.bin",
            "a/unique.bin",
            "shared/ab/original.abcdef0123456789.tld",
            "a/left.1111111111111111.gz",
            "b/right.1111111111111111.gz",
        ]);
        let cancel = AtomicBool::new(false);
        let mut indexed = DependencyResolver::new(&catalog, &cancel).unwrap();
        for name in [
            "a/plain.bin",
            "A\\PLAIN.BIN",
            "plain.bin",
            "unique.bin",
            "original.abcdef0123456789.tld",
            "alias.abcdef0123456789.tld",
            "alias.1111111111111111.gz",
            "missing.0000000000000000.mip0",
            "missing.bin",
        ] {
            assert_eq!(
                indexed.resolve(name).ok().map(|e| e.id),
                legacy_resolve(&catalog, name).ok().map(|e| e.id),
                "{name}"
            );
        }
        assert!(indexed.index.is_some());
    }
    #[test]
    fn indexed_resolver_keeps_renamed_mip_mapping_and_cancellation() {
        let mapping: Value =
            serde_json::from_str(include_str!("../Data/renamed-mip-hashes.json")).unwrap();
        let (old, new) = mapping.as_object().unwrap().iter().next().unwrap();
        let file = format!(
            "shared/{}/source.{}.mip0",
            &new.as_str().unwrap()[..2],
            new.as_str().unwrap()
        );
        let catalog = catalog_fixture(&[&file]);
        let cancel = AtomicBool::new(false);
        let mut indexed = DependencyResolver::new(&catalog, &cancel).unwrap();
        let name = format!("alias.{old}.mip0");
        assert_eq!(
            indexed.resolve(&name).unwrap().id,
            legacy_resolve(&catalog, &name).unwrap().id
        );
        cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(indexed.resolve(&file).is_err());
        assert!(ManifestIndex::build(&catalog, &cancel).is_err());
    }
    #[test]
    #[ignore = "requires NBA2K27_TEST_GAME_ROOT; read-only manifest benchmark"]
    fn installed_missing_reference_lookup_comparison() {
        let root = std::env::var("NBA2K27_TEST_GAME_ROOT").unwrap();
        let catalog = Catalog::load(std::path::Path::new(&root)).unwrap();
        let names: Vec<_> = (0..64)
            .map(|n| format!("__missing.{n:016x}.mip0"))
            .collect();
        let started = std::time::Instant::now();
        let old: Vec<_> = names
            .iter()
            .map(|n| legacy_resolve(&catalog, n).ok().map(|e| e.id))
            .collect();
        let old_ms = started.elapsed().as_secs_f64() * 1000.;
        let started = std::time::Instant::now();
        let cancel = AtomicBool::new(false);
        let mut indexed = DependencyResolver::new(&catalog, &cancel).unwrap();
        let new: Vec<_> = names
            .iter()
            .map(|n| indexed.resolve(n).ok().map(|e| e.id))
            .collect();
        assert_eq!(old, new);
        println!(
            "64 installed missing references: legacy={old_ms:.2}ms indexed including build={:.2}ms",
            started.elapsed().as_secs_f64() * 1000.
        );
    }
    #[test]
    fn binary_refs_only_and_best_segment_metadata() {
        let mut out = BTreeMap::new();
        refs(&json!({"unrelated":"missing.tld","x":{"Binary":"shared/floor.tld"},"y":{"Binary":"shared/floor.tld","Segments":[{}]}}),&mut out,0).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(
            out["shared/floor.tld"].1["Segments"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn repeated_refs_retain_authored_sizes_and_reject_conflicts() {
        let mut out = BTreeMap::new();
        refs(
            &json!({"a":{"Binary":"buffer.gz"},"b":{"Binary":"buffer.gz","Size":96}}),
            &mut out,
            0,
        )
        .unwrap();
        assert_eq!(out["buffer.gz"].1["Size"], 96);
        refs(
            &json!({"Binary":"buffer.gz","Segments":[{},{}]}),
            &mut out,
            0,
        )
        .unwrap();
        assert_eq!(out["buffer.gz"].1["Size"], 96);
        assert!(refs(&json!({"Binary":"buffer.gz","Size":48}), &mut out, 0).is_err());
    }
    #[test]
    fn unsafe_dependency_rejected() {
        assert!(refs(&json!({"Binary":"../outside.bin"}), &mut BTreeMap::new(), 0).is_err());
    }
    #[test]
    fn collisions_and_ambiguous_leaves_rejected() {
        let cancel = AtomicBool::new(false);
        assert!(
            pack(
                [("x.bin".into(), vec![]), ("X.bin".into(), vec![])],
                &cancel,
                |_| {}
            )
            .is_err()
        );
        let m = BTreeMap::from([
            ("a/x.bin".into(), ("a/x.bin".into(), vec![])),
            ("b/x.bin".into(), ("b/x.bin".into(), vec![])),
        ]);
        assert!(find(&m, "x.bin").is_err());
        assert_eq!(find(&m, "a/x.bin").unwrap().unwrap(), "a/x.bin");
    }
    #[test]
    fn packing_round_trip_preserves_all_payloads_without_reports() {
        let c = AtomicBool::new(false);
        let out = pack(
            [
                ("test.bin".into(), vec![1, 2, 3]),
                ("test.scne".into(), b"{}".to_vec()),
            ],
            &c,
            |_| {},
        )
        .unwrap();
        let mut a = Archive::open(out).unwrap();
        assert_eq!(a.named("test.bin").unwrap(), [1, 2, 3]);
        assert_eq!(a.names().unwrap().len(), 2);
        assert_eq!(a.validate_with(&c, |_| {}).unwrap(), 2);
    }
    #[test]
    fn converted_validation_checks_crc_and_cancellation() {
        let c = AtomicBool::new(false);
        let bytes = pack([("fixture.bin".into(), vec![1, 2, 3, 4])], &c, |_| {}).unwrap();
        let mut archive = Archive::open(bytes.clone()).unwrap();
        assert_eq!(validate(&mut archive, &c, |_| {}).unwrap(), 1);
        let mut corrupt = bytes;
        let central = corrupt.windows(4).position(|b| b == b"PK\x01\x02").unwrap();
        corrupt[central + 16] ^= 1;
        let mut archive = Archive::open(corrupt).unwrap();
        assert!(validate(&mut archive, &c, |_| {}).is_err());
        c.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(pack([("fixture.bin".into(), vec![1])], &c, |_| {}).is_err());
    }
}
