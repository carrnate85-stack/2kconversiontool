use anyhow::{Context, Result, ensure};
use base64::Engine;
use nba2k_assets::{
    catalog::Catalog,
    codec::{self, Archive, Oodle},
    textures,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
    time::UNIX_EPOCH,
};

const CACHE_BUDGET: usize = 192 * 1024 * 1024;
const MEMBER_BUDGET: usize = 128 * 1024 * 1024;
const RESOURCE_LIMIT: usize = 512;
const OWNER_LIMIT: usize = 128;
#[derive(Clone)]
enum Source {
    Local(PathBuf),
    Manifest(String),
    Shared(String),
}
struct Prepared {
    packet: Arc<Vec<u8>>,
    bytes: usize,
    touched: u64,
    local: bool,
}
pub struct Assets {
    catalog: Option<Catalog>,
    source_stamp: String,
    sources: HashMap<String, Source>,
    source_owners: HashMap<String, usize>,
    leases: HashMap<u64, BTreeSet<String>>,
    active_lease: Option<u64>,
    next_lease: u64,
    archives: HashMap<String, Prepared>,
    blobs: HashMap<String, Arc<Vec<u8>>>,
    bytes: usize,
    clock: u64,
    pub cancel: Arc<AtomicBool>,
    hits: usize,
    misses: usize,
    heads: crate::catalog_native::Heads,
}
fn hash(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}
fn stamp(path: &Path) -> Result<String> {
    file_stamp(path, Some(codec::MAX as u64))
}
fn file_stamp(path: &Path, maximum: Option<u64>) -> Result<String> {
    let m = fs::metadata(path)
        .with_context(|| format!("Preview source unavailable: {}", path.display()))?;
    ensure!(
        m.is_file() && maximum.is_none_or(|maximum| m.len() <= maximum),
        "Invalid or oversized preview file"
    );
    Ok(format!(
        "{}:{}:{}",
        path.to_string_lossy(),
        m.len(),
        m.modified()?.duration_since(UNIX_EPOCH)?.as_nanos()
    ))
}
fn game_source_stamp(root: &Path) -> Result<String> {
    let files = [
        stamp(&root.join("manifest"))?,
        file_stamp(&root.join("NBA2K27.exe"), None)?,
        file_stamp(&root.join("data/oodle/oo2core_9_win64.dll"), None)?,
    ];
    Ok(hash(serde_json::to_vec(&files)?))
}
fn token(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_owned()
}
fn skin_color(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| {
            value
                .as_f64()
                .filter(|n| n.is_finite() && n.fract() == 0.0 && (0.0..=99.0).contains(n))
                .map(|n| n as i64)
        })
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
        .filter(|n| (0..=99).contains(n))
}
fn expanded_dds<'a>(name: &str, members: &'a [Value]) -> Option<&'a Value> {
    if !name.to_ascii_lowercase().ends_with(".tld") {
        return None;
    }
    let decoded = format!("{}.dds", &name[..name.len() - 4]);
    members.iter().find(|row| {
        row["name"]
            .as_str()
            .is_some_and(|name| name.eq_ignore_ascii_case(&decoded))
    })
}
fn geometry_keys(accessories: &Value, items: &[String]) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    for item in items {
        let mode = accessories["items"]
            .as_array()
            .and_then(|rows| {
                rows.iter().find(|row| {
                    row["name"]
                        .as_str()
                        .is_some_and(|name| token(name) == *item)
                })
            })
            .and_then(|row| row["tangentspace"].as_str())
            .map(|mode| mode.trim().to_ascii_lowercase())
            .unwrap_or_default();
        if mode != "yes" {
            keys.insert(item.clone());
        }
        if mode != "no" {
            keys.insert(format!("{item}_tangentspace"));
        }
    }
    keys
}
fn visit(value: &Value, refs: &mut BTreeMap<String, Value>, depth: usize) -> Result<()> {
    ensure!(depth <= 128, "Preview metadata is too deeply nested");
    if let Some(map) = value.as_object() {
        if let Some(name) = map.get("Binary").and_then(Value::as_str) {
            let name = name.replace('\\', "/");
            let old_has_segments = refs
                .get(&name)
                .is_some_and(|v| v["Segments"].as_array().is_some_and(|s| !s.is_empty()));
            let new_has_segments = value["Segments"].as_array().is_some_and(|s| !s.is_empty());
            if !old_has_segments || new_has_segments {
                refs.insert(name, value.clone());
            }
        }
        for v in map.values() {
            visit(v, refs, depth + 1)?;
        }
    } else if let Some(array) = value.as_array() {
        for v in array {
            visit(v, refs, depth + 1)?;
        }
    }
    Ok(())
}
fn insert_member(
    members: &mut BTreeMap<String, Vec<u8>>,
    name: String,
    bytes: Vec<u8>,
) -> Result<()> {
    let used: usize = members.values().map(Vec::len).sum();
    let previous = members.get(&name).map(Vec::len).unwrap_or(0);
    ensure!(
        used - previous + bytes.len() <= MEMBER_BUDGET,
        "Decoded preview archive exceeds 128 MiB"
    );
    members.insert(name, bytes);
    Ok(())
}
impl Assets {
    pub fn new(cancel: Arc<AtomicBool>) -> Self {
        Self {
            catalog: None,
            source_stamp: String::new(),
            sources: HashMap::new(),
            source_owners: HashMap::new(),
            leases: HashMap::new(),
            active_lease: None,
            next_lease: 1,
            archives: HashMap::new(),
            blobs: HashMap::new(),
            bytes: 0,
            clock: 0,
            cancel,
            hits: 0,
            misses: 0,
            heads: crate::catalog_native::Heads::default(),
        }
    }
    pub fn recovery_payload(&self, payload: &Value) -> Value {
        let mut resolved = payload.clone();
        for field in crate::backend::accessory_recovery::PATH_FIELDS {
            if let Some(Source::Local(path)) = payload[*field]
                .as_str()
                .and_then(|uri| self.sources.get(uri))
            {
                resolved[*field] = json!(path.to_string_lossy());
            }
        }
        resolved
    }
    pub fn clear(&mut self) {
        self.heads.clear();
        self.sources.clear();
        self.source_owners.clear();
        self.leases.clear();
        self.active_lease = None;
        self.archives.clear();
        self.blobs.clear();
        self.bytes = 0;
    }
    pub fn reset_game(&mut self) {
        self.clear();
        self.catalog = None;
        self.source_stamp.clear();
    }
    pub fn player_files_changed(&mut self) {
        let expired: BTreeSet<_> = self
            .sources
            .iter()
            .filter(|(_, source)| matches!(source, Source::Local(_)))
            .map(|(uri, _)| uri.clone())
            .collect();
        for uri in &expired {
            self.sources.remove(uri);
            self.source_owners.remove(uri);
        }
        // Old mixed leases may still release after fresh local handles register.
        for resources in self.leases.values_mut() {
            resources.retain(|uri| !expired.contains(uri));
        }
        self.archives.retain(|_, archive| !archive.local);
        self.bytes = self
            .archives
            .values()
            .map(|archive| archive.bytes)
            .sum::<usize>()
            + self.blobs.values().map(|blob| blob.len()).sum::<usize>();
    }
    pub fn configure(&mut self, data: &Path) -> Result<()> {
        let result = self.configure_game(data);
        if result.is_err() {
            self.reset_game();
        }
        result
    }
    fn configure_game(&mut self, data: &Path) -> Result<()> {
        let settings: Value = serde_json::from_slice(&fs::read(data.join("settings.json"))?)?;
        let root = settings["game_root_2k27"]
            .as_str()
            .filter(|s| !s.is_empty())
            .context("Configure the NBA 2K27 install folder first")?;
        let root = PathBuf::from(root).canonicalize()?;
        let current = game_source_stamp(&root)?;
        if current != self.source_stamp {
            self.reset_game();
            self.catalog = Some(Catalog::load(&root)?);
            self.source_stamp = current;
        }
        Ok(())
    }
    pub fn accessory_catalog(
        &self,
        kind: &str,
        names: &Path,
        cancel: &AtomicBool,
    ) -> Result<Value> {
        let catalog = self
            .catalog
            .as_ref()
            .context("Native catalog unavailable")?;
        ensure!(
            catalog.root.join("mod.exe").is_file(),
            "Game extractor is unavailable"
        );
        let names: Value = fs::read(names)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or(Value::Null);
        crate::accessory_catalog::build(catalog, &names, kind, cancel)
    }
    fn with_resources(
        &mut self,
        prepare: impl FnOnce(&mut Self) -> Result<Value>,
    ) -> Result<Value> {
        ensure!(
            self.active_lease.is_none(),
            "Nested preview resource preparation"
        );
        ensure!(
            self.leases.len() < OWNER_LIMIT,
            "Too many live preview resource owners"
        );
        let lease = self.next_lease;
        self.next_lease = lease
            .checked_add(1)
            .context("Preview resource ID exhausted")?;
        self.leases.insert(lease, BTreeSet::new());
        self.active_lease = Some(lease);
        let result = prepare(self);
        self.active_lease = None;
        match result {
            Ok(mut value) => {
                value["resourceLease"] = json!(lease.to_string());
                Ok(value)
            }
            Err(error) => {
                self.release_resources(lease);
                Err(error)
            }
        }
    }

    pub fn release_resources(&mut self, lease: u64) {
        let Some(resources) = self.leases.remove(&lease) else {
            return;
        };
        for uri in resources {
            if let Some(owners) = self.source_owners.get_mut(&uri) {
                *owners -= 1;
                if *owners == 0 {
                    self.source_owners.remove(&uri);
                    self.sources.remove(&uri);
                }
            }
        }
    }

    fn resource_uri(&self, source: &Source, name: &str) -> Result<String> {
        let key = match &source {
            Source::Local(p) => stamp(p)?,
            Source::Manifest(n) | Source::Shared(n) => format!("{}:{n}", self.source_stamp),
        };
        Ok(format!(
            "cmt-preview://{}/{}",
            hash(key),
            name.rsplit('/').next().unwrap_or(name)
        ))
    }

    fn register(&mut self, source: Source, name: &str) -> Result<String> {
        let lease = self.active_lease.context("Preview resource has no owner")?;
        let uri = self.resource_uri(&source, name)?;
        ensure!(
            self.sources.len() < RESOURCE_LIMIT || self.sources.contains_key(&uri),
            "Preview session resource limit reached; start a new swap"
        );
        if self
            .leases
            .get_mut(&lease)
            .context("Preview resource owner expired")?
            .insert(uri.clone())
        {
            *self.source_owners.entry(uri.clone()).or_default() += 1;
        }
        self.sources.insert(uri.clone(), source);
        Ok(uri)
    }
    fn archive(&self, source: &Source) -> Result<Archive> {
        codec::cancelled(&self.cancel)?;
        let c = self
            .catalog
            .as_ref()
            .context("Native preview catalog unavailable")?;
        let packed = match source {
            Source::Local(p) => {
                stamp(p)?;
                fs::read(p)?
            }
            Source::Manifest(n) => codec::range(c, c.resolve(n)?)?,
            Source::Shared(_) => {
                let w = zip::ZipWriter::new(Cursor::new(Vec::<u8>::new()));
                return Archive::open(w.finish()?.into_inner());
            }
        };
        let bytes = if codec::vcz(&packed) {
            Oodle::load(&c.root)?.archive_preview(&packed, &self.cancel)?
        } else {
            packed
        };
        Archive::open(bytes)
    }
    fn evict(&mut self, incoming: usize) {
        while self.bytes + incoming > CACHE_BUDGET && !self.archives.is_empty() {
            let oldest = self
                .archives
                .iter()
                .min_by_key(|(_, p)| p.touched)
                .map(|(k, _)| k.clone())
                .unwrap();
            self.bytes -= self.archives.remove(&oldest).unwrap().bytes;
        }
        if self.bytes + incoming > CACHE_BUDGET {
            self.blobs.clear();
            self.bytes = self.archives.values().map(|p| p.bytes).sum();
        }
    }
    pub fn stats(&self) -> Value {
        json!({"native":true,"cacheBytes":self.bytes + self.heads.bytes,"cacheBudget":CACHE_BUDGET + crate::catalog_native::HEAD_BUDGET,"archiveCacheBytes":self.bytes,"headCacheBytes":self.heads.bytes,"headCacheBudget":crate::catalog_native::HEAD_BUDGET,"heads":self.heads.len(),"headCacheHits":self.heads.hits,"tangentFrameBuilds":self.heads.builds,"archives":self.archives.len(),"hits":self.hits,"misses":self.misses,"manifestIndexed":self.catalog.is_some(),"resources":self.catalog.as_ref().map(|c| c.entries.len()).unwrap_or(0),"registeredResources":self.sources.len(),"resourceLeases":self.leases.len(),"resourceLimit":RESOURCE_LIMIT,"resourceOwnerLimit":OWNER_LIMIT})
    }
    pub fn read_file(&mut self, uri: &str) -> Result<Value> {
        let bytes = self.file_bytes(uri)?;
        Ok(json!({"data":base64::engine::general_purpose::STANDARD.encode(bytes.as_slice())}))
    }
    pub fn binary_file(&mut self, uri: &str) -> Result<Vec<u8>> {
        crate::preview_packet::file(self.file_bytes(uri)?.as_slice())
    }
    fn file_bytes(&mut self, uri: &str) -> Result<Arc<Vec<u8>>> {
        let source = self
            .sources
            .get(uri)
            .cloned()
            .context("Preview resource expired; refresh the render")?;
        let Source::Shared(name) = source else {
            anyhow::bail!("Not a shared preview texture");
        };
        let bytes = if let Some(hit) = self.blobs.get(uri) {
            hit.clone()
        } else {
            let mut archive = self.archive(&Source::Shared(name.clone()))?;
            let c = self.catalog.as_ref().unwrap();
            let bytes = textures::reconstruct(
                &mut archive,
                c,
                &Oodle::load(&c.root)?,
                &name,
                &Value::Null,
                &self.cancel,
                16 * 1024 * 1024,
            )?;
            self.evict(bytes.len());
            self.bytes += bytes.len();
            let bytes = Arc::new(bytes);
            self.blobs.insert(uri.to_owned(), bytes.clone());
            bytes
        };
        Ok(bytes)
    }
    pub fn file_stat(&mut self, uri: &str) -> Result<Value> {
        ensure!(self.sources.contains_key(uri), "Preview resource expired");
        Ok(json!({"path":uri,"size":0,"mtimeMs":0}))
    }
    pub fn read_archive(&mut self, path: &str) -> Result<Value> {
        crate::preview_packet::archive_json(&self.archive_packet(path)?)
    }
    pub fn binary_archive(&mut self, path: &str) -> Result<Vec<u8>> {
        Ok(self.archive_packet(path)?.as_ref().clone())
    }
    fn archive_packet(&mut self, path: &str) -> Result<Arc<Vec<u8>>> {
        let (uri, source) = if path.starts_with("cmt-preview://") {
            (
                path.to_owned(),
                self.sources
                    .get(path)
                    .cloned()
                    .context("Preview resource expired")?,
            )
        } else {
            let p = PathBuf::from(path).canonicalize()?;
            ensure!(
                p.extension().is_some_and(|e| e.eq_ignore_ascii_case("iff")),
                "Only IFF archives can be previewed"
            );
            let source = Source::Local(p.clone());
            let uri = self.resource_uri(&source, &p.file_name().unwrap().to_string_lossy())?;
            (uri, source)
        };
        self.clock += 1;
        if let Some(hit) = self.archives.get_mut(&uri) {
            hit.touched = self.clock;
            self.hits += 1;
            return Ok(hit.packet.clone());
        }
        self.misses += 1;
        let mut archive = self.archive(&source)?;
        let garment = match &source {
            Source::Local(p) => p.to_string_lossy().to_lowercase(),
            Source::Manifest(n) | Source::Shared(n) => n.to_lowercase(),
        };
        let garment = garment.contains("jersey") || garment.contains("shorts");
        let mut warnings = Vec::new();
        let c = self
            .catalog
            .as_ref()
            .context("Native catalog unavailable")?;
        let oodle = Oodle::load(&c.root)?;
        let names = archive.names()?;
        let mut members = BTreeMap::<String, Vec<u8>>::new();
        let mut refs = BTreeMap::new();
        for row in &names {
            codec::cancelled(&self.cancel)?;
            let name = row["name"].as_str().unwrap_or("");
            let lower = name.to_lowercase();
            if lower.ends_with(".scne") || lower.ends_with(".txtr") {
                let bytes = archive.read(row["id"].as_u64().unwrap() as usize, 16 * 1024 * 1024)?;
                let document = codec::tree(&bytes)?;
                visit(&document, &mut refs, 0)?;
                insert_member(
                    &mut members,
                    name.to_owned(),
                    serde_json::to_vec(&document)?,
                )?;
            }
        }
        for row in &names {
            let name = row["name"].as_str().unwrap_or("");
            if name.to_lowercase().ends_with(".tld") {
                refs.entry(name.to_owned()).or_insert(Value::Null);
            }
        }
        for (name, node) in refs {
            codec::cancelled(&self.cancel)?;
            let lower = name.to_lowercase();
            if lower.ends_with(".tld") || lower.ends_with(".dds") {
                // Expanded player IFFs retain TXTR .tld references beside edited .dds data.
                let expanded = expanded_dds(&name, &names);
                let decoded = if let Some(member) = expanded {
                    archive
                        .read(
                            member["id"].as_u64().context("Missing DDS member ID")? as usize,
                            16 * 1024 * 1024,
                        )
                        .and_then(|bytes| codec::dds(&bytes))
                } else {
                    textures::reconstruct(
                        &mut archive,
                        c,
                        &oodle,
                        &name,
                        &node,
                        &self.cancel,
                        16 * 1024 * 1024,
                    )
                }
                .with_context(|| format!("Native preview texture {name}"));
                let bytes = match decoded {
                    Ok(bytes) => bytes,
                    Err(error) if garment => {
                        warnings.push(format!("Optional garment map skipped: {error:#}"));
                        continue;
                    }
                    Err(error) => return Err(error),
                };
                let decoded_name = if let Some(member) = expanded {
                    member["name"].as_str().unwrap().to_owned()
                } else if lower.ends_with(".tld") {
                    format!("{}.dds", &name[..name.len() - 4])
                } else {
                    name
                };
                insert_member(&mut members, decoded_name, bytes)?;
            } else if lower.ends_with(".bin") || lower.ends_with(".gz") {
                let expected = node["Size"].as_u64().unwrap_or(0) as usize;
                ensure!(expected <= MEMBER_BUDGET, "Geometry exceeds preview budget");
                let packed = archive
                    .named(&name)
                    .or_else(|_| codec::range(c, c.resolve(&name)?))?;
                let bytes = if codec::vcz(&packed) {
                    ensure!(expected > 0, "Compressed geometry needs a declared size");
                    oodle.buffer(&packed, expected)?
                } else if packed.starts_with(&[0x1f, 0x8b, 8]) {
                    let mut out = Vec::new();
                    flate2::read::GzDecoder::new(&packed[..])
                        .take(MEMBER_BUDGET as u64 + 1)
                        .read_to_end(&mut out)?;
                    ensure!(
                        out.len() <= MEMBER_BUDGET,
                        "Geometry exceeds preview budget"
                    );
                    out
                } else {
                    packed
                };
                ensure!(
                    expected == 0 || expected == bytes.len(),
                    "Geometry member size mismatch: {name}"
                );
                insert_member(&mut members, name, bytes)?;
            }
        }
        for row in &names {
            let name = row["name"].as_str().unwrap_or("");
            if name.to_lowercase().ends_with(".dds")
                && !members
                    .keys()
                    .any(|existing| existing.eq_ignore_ascii_case(name))
            {
                insert_member(
                    &mut members,
                    name.to_owned(),
                    archive.read(row["id"].as_u64().unwrap() as usize, 16 * 1024 * 1024)?,
                )?;
            }
        }
        let total: usize = members.values().map(Vec::len).sum();
        ensure!(
            total <= MEMBER_BUDGET,
            "Decoded preview archive exceeds 128 MiB"
        );
        let mut geometry = Sha256::new();
        let mut content = Sha256::new();
        for (name, data) in &members {
            content.update(name.as_bytes());
            content.update(&data);
            if !name.to_lowercase().ends_with(".dds") && !name.to_lowercase().ends_with(".txtr") {
                geometry.update(name.as_bytes());
                geometry.update(&data);
            }
        }
        let geometry = format!("{:x}", geometry.finalize());
        let content = format!("{:x}", content.finalize());
        let packet = Arc::new(crate::preview_packet::archive(
            &members, &geometry, &content, &warnings,
        )?);
        let bytes = packet.len();
        ensure!(
            bytes <= CACHE_BUDGET,
            "Preview response exceeds memory budget"
        );
        self.evict(bytes);
        self.bytes += bytes;
        self.archives.insert(
            uri,
            Prepared {
                packet: packet.clone(),
                bytes,
                touched: self.clock,
                local: matches!(source, Source::Local(_)),
            },
        );
        Ok(packet)
    }
    pub fn prepare(&mut self, payload: &Value) -> Result<Value> {
        self.with_resources(|assets| assets.prepare_owned(payload))
    }
    fn prepare_owned(&mut self, payload: &Value) -> Result<Value> {
        codec::cancelled(&self.cancel)?;
        let target = PathBuf::from(
            payload["target"]
                .as_str()
                .context("Choose a staged player")?,
        )
        .canonicalize()?;
        ensure!(
            target
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("iff")),
            "Choose a staged player IFF"
        );
        let mut main = self.archive(&Source::Local(target.clone()))?;
        let appearance = match main.named("appearance_info.json") {
            Ok(bytes) => codec::tree(&bytes)?,
            Err(_) => json!({}),
        };
        let body = appearance.get("appearance").unwrap_or(&appearance);
        let accessories = &appearance["accessory_items"];
        let config = payload["config"]
            .as_str()
            .filter(|s| !s.is_empty())
            .or_else(|| accessories["default_config"].as_str())
            .or_else(|| accessories["default_out_of_uniform_config"].as_str())
            .unwrap_or("")
            .to_owned();
        let selected = accessories["configurations"].as_array().and_then(|rows| {
            rows.iter().find(|row| {
                row["name"]
                    .as_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case(&config))
            })
        });
        ensure!(
            payload["config"]
                .as_str()
                .filter(|s| !s.is_empty())
                .is_none()
                || selected.is_some(),
            "Selected configuration no longer exists"
        );
        let items: Vec<String> = selected
            .and_then(|s| s["items"].as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(token)
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let gender = token(body["gender"].as_str().unwrap_or("male"));
        let geometry = geometry_keys(accessories, &items);
        let gender = if gender.starts_with("female") {
            "female"
        } else if gender.starts_with("child") {
            "child"
        } else {
            "male"
        };
        let skin = body
            .get("skin color")
            .or_else(|| body.get("skin_color"))
            .and_then(skin_color);
        let mut body_files =
            BTreeMap::from([(format!("{gender}_skin.iff"), "body-detail".to_owned())]);
        if let Some(skin) = skin.filter(|s| (0..=99).contains(s)) {
            for region in ["chest", "torso", "legs"] {
                body_files.insert(
                    format!(
                        "{}{region}_color_{skin:02}.iff",
                        if gender == "female" { "female_" } else { "" }
                    ),
                    "body-color".to_owned(),
                );
            }
        }
        let stem = target.file_stem().unwrap().to_string_lossy().to_string();
        let prefix = format!("{}_", stem.to_lowercase());
        let mut companions = Vec::<Value>::new();
        let mut local = std::collections::HashSet::new();
        for entry in fs::read_dir(target.parent().unwrap())? {
            let p = entry?.path();
            if !p.is_file() || !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("iff")) {
                continue;
            }
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            let lower = name.to_lowercase();
            let kind = body_files.get(&lower).cloned().or_else(|| {
                let suffix = lower.strip_prefix(&prefix)?.strip_suffix(".iff")?;
                if suffix == format!("config_{}", token(&config)) {
                    Some("config".to_owned())
                } else if suffix
                    .strip_prefix("item_")
                    .is_some_and(|s| items.iter().any(|i| i == s))
                {
                    Some("item".to_owned())
                } else if suffix
                    .strip_prefix("geo_")
                    .is_some_and(|s| geometry.contains(s))
                {
                    Some("geometry".to_owned())
                } else {
                    None
                }
            });
            if let Some(kind) = kind {
                let uri = self.register(Source::Local(p), &name)?;
                local.insert(lower);
                companions.push(json!({"path":uri,"name":name,"kind":kind}));
            }
        }
        let mut requested = BTreeMap::new();
        for (name, kind) in body_files {
            if !local.contains(&name) {
                requested.insert(format!("char/{name}"), kind);
            }
        }
        let config_name = format!("{stem}_config_{}.iff", token(&config));
        if !config.is_empty() && !local.contains(&config_name.to_lowercase()) {
            requested.insert(format!("char/sig/{config_name}"), "config".to_owned());
        }
        for item in &items {
            for (suffix, kind) in [
                (format!("item_{item}"), "item"),
                (format!("geo_{item}"), "geometry"),
                (format!("geo_{item}_tangentspace"), "geometry"),
            ] {
                if kind == "geometry" && !geometry.contains(suffix.strip_prefix("geo_").unwrap()) {
                    continue;
                }
                let name = format!("{stem}_{suffix}.iff");
                if !local.contains(&name.to_lowercase()) {
                    requested.insert(format!("char/sig/{name}"), kind.to_owned());
                }
            }
        }
        for name in ["jersey_nike_v.iff", "shorts_nike.iff"] {
            requested.insert(
                format!(
                    "char/{}{name}",
                    if gender == "female" { "female_" } else { "" }
                ),
                "preview-jersey".to_owned(),
            );
        }
        let mut warnings = Vec::new();
        for (name, kind) in requested {
            codec::cancelled(&self.cancel)?;
            if self.catalog.as_ref().unwrap().resolve(&name).is_err() {
                if kind.starts_with("body") {
                    warnings.push(format!(
                        "Optional preview dependency not in manifest: {name}"
                    ));
                }
                continue;
            }
            let uri = self.register(Source::Manifest(name.clone()), &name)?;
            companions.push(json!({"path":uri,"name":name,"kind":kind}));
        }
        let order = |kind: &str| match kind {
            "config" => 0,
            "item" => 1,
            "geometry" => 2,
            "body-detail" => 3,
            "body-color" => 4,
            _ => 5,
        };
        companions.sort_by(|a, b| {
            order(a["kind"].as_str().unwrap())
                .cmp(&order(b["kind"].as_str().unwrap()))
                .then_with(|| a["name"].as_str().cmp(&b["name"].as_str()))
        });
        let white = body
            .get("eye white")
            .or_else(|| body.get("eye_white"))
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| v.as_f64().map(|n| n.to_string()).unwrap_or_default())
            })
            .unwrap_or_else(|| "0".into());
        let white = match white.as_str() {
            "1" => "shared/8b/eyewhite_color_01.8bcd8c19e6fb8f09",
            "2" => "shared/1a/eyewhite_color_02.1aca11c38da970ca",
            "3" => "shared/a2/eyewhite_color_03.a2f8647ad2256dee",
            "glow" => "shared/16/eyewhite_color_glow.16423ad4da3a86fe",
            _ => "shared/e6/eyewhite_color_00.e69c01fde495d855",
        };
        let lash = if body
            .get("lash color")
            .or_else(|| body.get("lash_color"))
            .and_then(Value::as_str)
            .is_some_and(|s| s.eq_ignore_ascii_case("light"))
        {
            "shared/05/lash_light_color.05d7fb9877d4c4b7"
        } else {
            "shared/17/lash_dark_color.1758d32561b833f3"
        };
        let mut external = Vec::new();
        for (name, format, region, channel) in [
            (white, "BC1_UNORM", "eye", "color"),
            (
                "shared/ad/iris_normalheight1.ad144d47f7f5520f",
                "BC7_UNORM",
                "eye",
                "normal",
            ),
            (lash, "BC7_UNORM", "eyelash", "color"),
        ] {
            let name = format!("{name}.tld");
            if self.catalog.as_ref().unwrap().resolve(&name).is_err() {
                warnings.push(format!("Shared preview texture unavailable: {name}"));
                continue;
            }
            let leaf = name
                .rsplit('/')
                .next()
                .unwrap()
                .trim_end_matches(".tld")
                .to_owned()
                + ".dds";
            let uri = self.register(Source::Shared(name), &leaf)?;
            external.push(json!({"path":uri,"name":leaf,"source":"shared (native memory)","format":format,"binding":{"target":region,"channel":channel}}));
        }
        let mut fit = BTreeMap::new();
        for container in [body, &appearance] {
            if let Some(map) = container["body_fit"].as_object() {
                for (k, v) in map {
                    if v.is_number() {
                        fit.insert(k.clone(), v.clone());
                    }
                }
            }
            if let Some(map) = container.as_object() {
                for (k, v) in map {
                    if v.is_number()
                        && (k.to_uppercase().ends_with("THIN")
                            || k.to_uppercase().ends_with("THICK"))
                    {
                        fit.entry(k.clone()).or_insert(v.clone());
                    }
                }
            }
        }
        let main_uri = self.register(
            Source::Local(target.clone()),
            &target.file_name().unwrap().to_string_lossy(),
        )?;
        let fingerprint = hash(serde_json::to_vec(&json!([
            main_uri, companions, external, appearance, config
        ]))?);
        Ok(
            json!({"filePath":main_uri,"name":target.file_name().unwrap().to_string_lossy(),"activeItems":items,"warnings":warnings,
            "fingerprint":fingerprint,"native":true,
            "options":{"companionPaths":companions,"externalTextures":external,"projectTangent":"staged","preferHighestDetail":true},
            "player":{"mode":"staged-player","defaultConfig":config,"gender":gender,"skinColor":skin,"irisColor":body.get("iris color").and_then(Value::as_str).unwrap_or("BROWN"),"irisSize":body.get("iris size").and_then(Value::as_str).unwrap_or("REGULAR"),"bodyFit":fit,
                "companionsLoaded":companions.len(),"companionsRequested":companions.len(),"sharedTexturesLoaded":external.len(),
                "stagedBodyTextures":companions.iter().filter(|c| c["kind"].as_str().is_some_and(|s| s.starts_with("body-")) && c["name"].as_str().is_some_and(|s| !s.starts_with("char/"))).count(),
                "jerseyAvailable":companions.iter().any(|c| c["kind"] == "preview-jersey")}}),
        )
    }
    pub fn hair(&mut self, payload: &Value) -> Result<Value> {
        self.with_resources(|assets| assets.hair_owned(payload))
    }
    fn hair_owned(&mut self, payload: &Value) -> Result<Value> {
        let asset_type = payload["assetType"].as_str().unwrap_or("hair");
        ensure!(
            matches!(asset_type, "hair" | "facialhair"),
            "Unsupported accessory type"
        );
        let name = payload["assetId"]
            .as_str()
            .context("Select a manifest asset")?
            .replace('\\', "/");
        let captures = regex::Regex::new(r"(?i)^char/sig/png([0-9]+)_geo_(.+)\.iff$")?
            .captures(&name)
            .context("Native catalog preview requires a player accessory")?;
        let png = &captures[1];
        let key = captures[2].trim_end_matches("_tangentspace");
        ensure!(
            if asset_type == "facialhair" {
                key.starts_with("facialhair")
            } else {
                key.starts_with("hair")
            },
            "Asset type does not match geometry"
        );
        self.catalog.as_ref().unwrap().resolve(&name)?;
        let geometry = self.register(Source::Manifest(name.clone()), &name)?;
        let player = format!("char/sig/png{png}.iff");
        self.catalog.as_ref().unwrap().resolve(&player)?;
        let player = self.register(Source::Manifest(player.clone()), &player)?;
        let item = format!("char/sig/png{png}_item_{key}.iff");
        let item = if self.catalog.as_ref().unwrap().resolve(&item).is_ok() {
            self.register(Source::Manifest(item.clone()), &item)?
        } else {
            String::new()
        };
        Ok(
            json!({"assetType":asset_type,"sourcePng":png,"key":key,"tangentspace":name.contains("_tangentspace.iff"),"geometryPath":geometry,"sourcePlayerPath":player,"itemPath":item,"native":true}),
        )
    }

    pub fn catalog_geometry(&mut self, payload: &Value) -> Result<Value> {
        self.catalog_geometry_data(payload)?.json()
    }

    pub fn binary_catalog_geometry(&mut self, payload: &Value) -> Result<Vec<u8>> {
        self.catalog_geometry_data(payload)?.packet()
    }

    fn catalog_geometry_data(
        &mut self,
        payload: &Value,
    ) -> Result<crate::catalog_native::CatalogGeometry> {
        use crate::catalog_native::{
            CatalogGeometry, Head, PackedPreview, frames::Frame, geometry,
        };
        let started = std::time::Instant::now();
        let geometry_uri = payload["geometryPath"]
            .as_str()
            .context("Missing catalog geometry")?;
        let head_uri = payload["sourcePlayerPath"]
            .as_str()
            .context("Missing donor head")?;
        let source = self
            .sources
            .get(geometry_uri)
            .cloned()
            .context("Catalog resource expired")?;
        let head_source = self
            .sources
            .get(head_uri)
            .cloned()
            .context("Donor resource expired")?;
        let (Source::Manifest(name), Source::Manifest(head_name)) = (&source, &head_source) else {
            anyhow::bail!("Native catalog geometry requires manifest assets");
        };
        let pattern =
            regex::Regex::new(r"(?i)^char/sig/(png[0-9]+)_geo_(?:facialhair|hair)_.+\.iff$")?;
        let capture = pattern
            .captures(name)
            .context("Unsupported catalog geometry")?;
        ensure!(
            head_name.eq_ignore_ascii_case(&format!("char/sig/{}.iff", &capture[1])),
            "Catalog donor does not match attachment"
        );
        let oodle = Oodle::load(&self.catalog.as_ref().context("Catalog unavailable")?.root)?;
        let hit = self.heads.get(head_uri);
        let head_cached = hit.is_some();
        let head = if let Some(head) = hit {
            head
        } else {
            let mut archive = self.archive(&head_source)?;
            let preview = geometry::build(
                &mut archive,
                self.catalog.as_ref().unwrap(),
                &oodle,
                head_uri,
                true,
                &self.cancel,
            )?;
            let first = preview["meshes"]
                .as_array()
                .and_then(|meshes| meshes.first())
                .context("No donor face-shader geometry")?;
            let frame = Frame::new(first, &self.cancel)?;
            self.heads
                .insert(head_uri.to_owned(), Head::new(preview, frame)?)
        };
        let mut archive = self.archive(&source)?;
        let mut asset = geometry::build(
            &mut archive,
            self.catalog.as_ref().unwrap(),
            &oodle,
            geometry_uri,
            false,
            &self.cancel,
        )?;
        let mut projected = 0usize;
        for mesh in asset["meshes"]
            .as_array_mut()
            .context("Missing attachment meshes")?
        {
            if mesh["tangentSpaceAttachment"] == true {
                projected += head.frame.project(mesh, &self.cancel)?;
                mesh["geometryKey"] = json!(format!(
                    "{}:head:{}",
                    mesh["geometryKey"].as_str().unwrap_or(""),
                    head_uri
                ));
            }
        }
        let asset = PackedPreview::new(asset)?;
        Ok(CatalogGeometry {
            asset,
            head: (payload["includeBase"] != false).then_some(head),
            stats: json!({"nativeGeometry":true,"headCacheHit":head_cached,"tangentFrameCacheHit":head_cached,"tangentHairProjectedVertices":projected,"facialHairProjectedVertices":if payload["assetType"] == "facialhair" {projected} else {0},"nativeGeometryMs":started.elapsed().as_secs_f64()*1000.}),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn owned(assets: &mut Assets, names: &[&str]) -> Result<Value> {
        assets.with_resources(|assets| {
            let paths = names
                .iter()
                .map(|name| assets.register(Source::Manifest((*name).into()), name))
                .collect::<Result<Vec<_>>>()?;
            Ok(json!({"paths":paths}))
        })
    }
    fn lease(value: &Value) -> u64 {
        value["resourceLease"].as_str().unwrap().parse().unwrap()
    }

    #[test]
    fn changed_or_missing_game_prerequisites_retire_native_cache_state() {
        struct Fixture(PathBuf);
        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let fixture = Fixture(std::env::temp_dir().join(
            format!("cmt-source-inputs-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()),
        ));
        let game = fixture.0.join("game");
        fs::create_dir_all(game.join("data/oodle")).unwrap();
        fs::write(game.join("manifest"), b"char/sig/png1.iff,0A,0,1\n").unwrap();
        fs::write(game.join("NBA2K27.exe"), b"exe fixture").unwrap();
        fs::write(
            game.join("data/oodle/oo2core_9_win64.dll"),
            b"decoder fixture",
        )
        .unwrap();
        fs::write(
            fixture.0.join("settings.json"),
            serde_json::to_vec(&json!({"game_root_2k27":game})).unwrap(),
        )
        .unwrap();
        let mut assets = Assets::new(Arc::new(AtomicBool::new(false)));
        assets.configure(&fixture.0).unwrap();
        let initial = assets.source_stamp.clone();
        assets.configure(&fixture.0).unwrap();
        assert_eq!(initial, assets.source_stamp);
        for name in ["NBA2K27.exe", "data/oodle/oo2core_9_win64.dll"] {
            let before = owned(&mut assets, &["shared"]).unwrap();
            assets.blobs.insert("cached".into(), Arc::new(vec![1]));
            assets.bytes = 1;
            let stamp = assets.source_stamp.clone();
            fs::write(game.join(name), b"changed prerequisite").unwrap();
            assets.configure(&fixture.0).unwrap();
            assert_ne!(stamp, assets.source_stamp);
            assert!(
                assets.sources.is_empty() && assets.leases.is_empty() && assets.blobs.is_empty()
            );
            assert_eq!(assets.bytes, 0);
            let after = owned(&mut assets, &["shared"]).unwrap();
            assert_ne!(before["paths"], after["paths"]);
            fs::remove_file(game.join(name)).unwrap();
            assert!(assets.configure(&fixture.0).is_err());
            assert!(
                assets.catalog.is_none() && assets.sources.is_empty() && assets.leases.is_empty()
            );
            fs::write(game.join(name), b"restored prerequisite").unwrap();
            assets.configure(&fixture.0).unwrap();
        }
        let _ = owned(&mut assets, &["shared"]).unwrap();
        fs::rename(&game, fixture.0.join("moved-game")).unwrap();
        assert!(assets.configure(&fixture.0).is_err());
        assert!(assets.catalog.is_none() && assets.sources.is_empty());
    }

    #[test]
    fn sequential_browsing_does_not_accumulate_old_resource_handles() {
        let mut assets = Assets::new(Arc::new(AtomicBool::new(false)));
        for index in 0..2000 {
            let value = owned(
                &mut assets,
                &[&format!("hair-{index}"), &format!("head-{index}")],
            )
            .unwrap();
            assert_eq!(assets.sources.len(), 2);
            assets.release_resources(lease(&value));
            assert!(
                assets.sources.is_empty()
                    && assets.leases.is_empty()
                    && assets.source_owners.is_empty()
            );
        }
    }

    #[test]
    fn overlapping_previews_hold_shared_sources_until_the_last_owner_releases() {
        let mut assets = Assets::new(Arc::new(AtomicBool::new(false)));
        let catalog = owned(&mut assets, &["shared-head", "catalog-hair", "shared-head"]).unwrap();
        let blender = owned(&mut assets, &["shared-head", "blender-hair"]).unwrap();
        let shared = catalog["paths"][0].as_str().unwrap();
        assert_eq!(assets.source_owners[shared], 2);
        assets.release_resources(lease(&catalog));
        assets.release_resources(lease(&catalog));
        assert!(assets.file_stat(shared).is_ok());
        assert!(
            assets
                .file_stat(catalog["paths"][1].as_str().unwrap())
                .is_err()
        );
        assets.release_resources(lease(&blender));
        assert!(assets.file_stat(shared).is_err());
    }

    #[test]
    fn failed_preparation_rolls_back_only_its_own_registrations() {
        let mut assets = Assets::new(Arc::new(AtomicBool::new(false)));
        let existing = owned(&mut assets, &["shared"]).unwrap();
        let result = assets.with_resources(|assets| {
            assets.register(Source::Manifest("shared".into()), "shared")?;
            assets.register(Source::Manifest("partial".into()), "partial")?;
            anyhow::bail!("Expected preparation failure")
        });
        assert!(result.is_err());
        assert_eq!(assets.sources.len(), 1);
        assert_eq!(assets.leases.len(), 1);
        assert_eq!(
            assets.source_owners[existing["paths"][0].as_str().unwrap()],
            1
        );
        assert!(assets.active_lease.is_none());
    }

    #[test]
    fn delayed_release_after_reset_cannot_release_a_new_owner() {
        let mut assets = Assets::new(Arc::new(AtomicBool::new(false)));
        let before = owned(&mut assets, &["same"]).unwrap();
        assets.clear();
        let after = owned(&mut assets, &["same"]).unwrap();
        assert_ne!(lease(&before), lease(&after));
        assets.release_resources(lease(&before));
        assert!(
            assets
                .file_stat(after["paths"][0].as_str().unwrap())
                .is_ok()
        );
    }

    #[test]
    fn cached_bytes_do_not_authorize_released_resource_reads() {
        let mut assets = Assets::new(Arc::new(AtomicBool::new(false)));
        let value = owned(&mut assets, &["cached"]).unwrap();
        let uri = value["paths"][0].as_str().unwrap().to_owned();
        assets.archives.insert(
            uri.clone(),
            Prepared {
                packet: Arc::new(vec![1, 2, 3]),
                bytes: 3,
                touched: 0,
                local: false,
            },
        );
        assert_eq!(assets.binary_archive(&uri).unwrap(), vec![1, 2, 3]);
        assets.release_resources(lease(&value));
        assert!(assets.binary_archive(&uri).is_err());
        let shared = assets.with_resources(|assets| Ok(json!({"path":assets.register(Source::Shared("shared.tld".into()), "shared.dds")?}))).unwrap();
        let uri = shared["path"].as_str().unwrap();
        assets.blobs.insert(uri.into(), Arc::new(vec![4, 5]));
        assert_eq!(assets.file_bytes(uri).unwrap().as_slice(), &[4, 5]);
        assets.release_resources(lease(&shared));
        assert!(assets.file_bytes(uri).is_err());
    }

    #[test]
    fn player_edits_retire_local_data_without_revoking_manifest_owners() {
        struct Fixture(PathBuf);
        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = fs::remove_file(&self.0);
            }
        }
        let fixture = Fixture(std::env::temp_dir().join(format!(
                "cmt-edit-cache-{}-{}.iff",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )));
        fs::write(&fixture.0, b"private local fixture").unwrap();
        let mut assets = Assets::new(Arc::new(AtomicBool::new(false)));
        let catalog = owned(&mut assets, &["hair", "head"]).unwrap();
        let mixed = assets
            .with_resources(|assets| {
                Ok(json!({
                    "local":assets.register(Source::Local(fixture.0.clone()), "player.iff")?,
                    "hair":assets.register(Source::Manifest("hair".into()), "hair")?,
                    "shared":assets.register(Source::Shared("shared.tld".into()), "shared.dds")?
                }))
            })
            .unwrap();
        let old_local = mixed["local"].as_str().unwrap().to_owned();
        let manifest = mixed["hair"].as_str().unwrap().to_owned();
        let shared = mixed["shared"].as_str().unwrap().to_owned();
        let packet = Arc::new(vec![1, 2, 3]);
        for (uri, local) in [
            (old_local.clone(), true),
            ("unregistered-local".into(), true),
            (manifest.clone(), false),
        ] {
            assets.archives.insert(
                uri,
                Prepared {
                    packet: packet.clone(),
                    bytes: 3,
                    touched: 0,
                    local,
                },
            );
        }
        assets.blobs.insert(shared.clone(), Arc::new(vec![4, 5]));
        assets.bytes = 11;
        let mesh = json!({"positions":[0,0,0,1,0,0,0,1,0],"uvs":[0,0,1,0,0,1],"indices":[0,1,2],"sourceVertexIds":[0,1,2]});
        let frame =
            crate::catalog_native::frames::Frame::new(&mesh, &AtomicBool::new(false)).unwrap();
        assets.heads.insert(
            "head".into(),
            crate::catalog_native::Head::new(json!({"meshes":[mesh]}), frame).unwrap(),
        );
        let head_bytes = assets.heads.bytes;
        assets.player_files_changed();
        assets.player_files_changed();
        assert_eq!(assets.bytes, 5);
        assert_eq!(assets.archives.len(), 1);
        assert_eq!(assets.heads.bytes, head_bytes);
        assert_eq!(assets.heads.len(), 1);
        assert!(!assets.sources.contains_key(&old_local));
        assert!(!assets.source_owners.contains_key(&old_local));
        assert!(assets.binary_archive(&old_local).is_err());
        assert_eq!(assets.binary_archive(&manifest).unwrap(), *packet);
        assert_eq!(assets.file_bytes(&shared).unwrap().as_slice(), &[4, 5]);
        assert_eq!(assets.source_owners[&manifest], 2);
        let fresh = assets.with_resources(|assets| Ok(json!({"local":assets.register(Source::Local(fixture.0.clone()), "player.iff")?}))).unwrap();
        assert_eq!(fresh["local"], old_local);
        assets.release_resources(lease(&mixed));
        assert!(assets.sources.contains_key(&old_local));
        assert_eq!(assets.source_owners[&old_local], 1);
        assert_eq!(assets.source_owners[&manifest], 1);
        assert!(!assets.sources.contains_key(&shared));
        assets.release_resources(lease(&catalog));
        assets.release_resources(lease(&fresh));
        assert!(
            assets.sources.is_empty()
                && assets.leases.is_empty()
                && assets.source_owners.is_empty()
        );
        assets.clear();
        assert_eq!(assets.bytes, 0);
        assert_eq!(assets.heads.bytes, 0);
    }

    #[test]
    fn live_resource_limits_are_bounded_and_recover_after_release() {
        let mut assets = Assets::new(Arc::new(AtomicBool::new(false)));
        let names: Vec<String> = (0..512).map(|index| format!("resource-{index}")).collect();
        let value = owned(
            &mut assets,
            &names.iter().map(String::as_str).collect::<Vec<_>>(),
        )
        .unwrap();
        assert!(owned(&mut assets, &["one-too-many"]).is_err());
        assert_eq!(assets.sources.len(), 512);
        assert_eq!(assets.leases.len(), 1);
        assets.release_resources(lease(&value));
        assert!(owned(&mut assets, &["new"]).is_ok());
        assets.clear();
        let values: Vec<Value> = (0..128)
            .map(|_| owned(&mut assets, &["shared"]).unwrap())
            .collect();
        assert!(owned(&mut assets, &["shared"]).is_err());
        assets.release_resources(lease(&values[0]));
        assert!(owned(&mut assets, &["shared"]).is_ok());
    }

    #[test]
    fn expanded_dds_is_selected_before_resolving_original_tld() {
        let members = vec![
            json!({"id":0,"name":"chest_color.hash.tld"}),
            json!({"id":1,"name":"Chest_Color.hash.DDS"}),
            json!({"id":2,"name":"different.dds"}),
        ];
        assert_eq!(
            expanded_dds("chest_color.hash.tld", &members).unwrap()["id"],
            1
        );
        assert!(expanded_dds("missing.hash.tld", &members).is_none());
        assert!(expanded_dds("chest_color.hash.dds", &members).is_none());
        assert!(expanded_dds("chest_color.hash.TXTR", &members).is_none());
    }
    #[test]
    fn skin_ids_preserve_zero_and_integral_floats() {
        assert_eq!(skin_color(&json!(0.0)), Some(0));
        assert_eq!(skin_color(&json!(3.0)), Some(3));
        assert_eq!(skin_color(&json!("3")), Some(3));
        assert_eq!(skin_color(&json!(3.5)), None);
        assert_eq!(skin_color(&Value::Null), None);
    }
    #[test]
    fn clearing_preview_drops_all_resource_handles_and_bytes() {
        let mut assets = Assets::new(Arc::new(AtomicBool::new(false)));
        assets
            .sources
            .insert("test".into(), Source::Manifest("test".into()));
        assets.blobs.insert("test".into(), Arc::new(vec![1, 2]));
        assets.bytes = 2;
        assets.clear();
        assert_eq!(assets.bytes, 0);
        assert!(assets.sources.is_empty());
        assert!(assets.blobs.is_empty());
    }
    #[test]
    fn eviction_preserves_recent_archives() {
        let mut assets = Assets::new(Arc::new(AtomicBool::new(false)));
        for (name, touched) in [("old", 1), ("recent", 2)] {
            assets.archives.insert(
                name.into(),
                Prepared {
                    packet: Arc::new(Vec::new()),
                    bytes: CACHE_BUDGET / 2,
                    touched,
                    local: false,
                },
            );
        }
        assets.bytes = CACHE_BUDGET;
        assets.evict(1);
        assert!(!assets.archives.contains_key("old"));
        assert!(assets.archives.contains_key("recent"));
        assert!(assets.bytes + 1 <= CACHE_BUDGET);
    }
    #[test]
    fn scene_references_keep_original_anchor_buffer_sizes() {
        let mut refs = BTreeMap::new();
        visit(
            &json!({"VertexStream":{"Binary":"VertexBuffer.test.gz","Size":64}}),
            &mut refs,
            0,
        )
        .unwrap();
        assert_eq!(refs["VertexBuffer.test.gz"]["Size"], 64);
        assert!(visit(&Value::Null, &mut refs, 129).is_err());
    }
    #[test]
    fn accessory_tokens_match_existing_pipeline() {
        assert_eq!(token("Hair + Facial Hair"), "hair_facial_hair");
        assert_eq!(token("DREADS"), "dreads");
    }
    #[test]
    fn staged_geometry_modes_exclude_opposite_companions() {
        let items = vec![
            "hair_afro".to_owned(),
            "facialhair_beard".to_owned(),
            "headband".to_owned(),
        ];
        let accessories = json!({"items":[
            {"name":"Hair_AFRO","tangentspace":" NO "},
            {"name":"facialHair_beard","tangentspace":"YES"},
            {"name":"inactive","tangentspace":"both"}]});
        let keys = geometry_keys(&accessories, &items);
        assert!(keys.contains("hair_afro"));
        assert!(!keys.contains("hair_afro_tangentspace"));
        assert!(keys.contains("facialhair_beard_tangentspace"));
        assert!(!keys.contains("facialhair_beard"));
        assert!(keys.contains("headband") && keys.contains("headband_tangentspace"));
        assert!(!keys.contains("inactive"));
        let both = geometry_keys(
            &json!({"items":[{"name":"hair_afro","tangentspace":"both"}]}),
            &items[..1],
        );
        assert_eq!(both.len(), 2);
        assert_eq!(geometry_keys(&Value::Null, &items[..1]), both);
    }
}
