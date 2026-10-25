//! WPF Blacktop preview services, read-only toward the game:
//! - render-on-demand clothing thumbnails (core/clothing-thumbnails.js):
//!   store/thumbnails/<asset>.iff (or the shoe model photo) decoded by
//!   fit_assets and converted to PNG (WPF used texconv) in an app cache;
//! - "Prepare all previews" (core/clothing-cache-job.js), batches of 64 with
//!   4 workers, browse requests keep priority;
//! - the mannequin preview's manifest index, cache keys and decoded resources
//!   for the reused WPF fast-clothing-preview worker (frontend/public/fit-preview).
use crate::{
    bc7,
    fit_assets::{self, FitAssets, Game},
};
use anyhow::{Context, Result, ensure};
use regex::Regex;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

pub type Emit = Arc<dyn Fn(&str, Value) + Send + Sync>;
const THUMB_LIMIT: u64 = 4 * 1024 * 1024;
const CACHE_LIMIT: u64 = 2 * 1024 * 1024 * 1024;

/// WPF PreviewQueue(4): at most four cold conversions, foreground first.
#[derive(Default)]
struct Gate {
    state: Mutex<(usize, usize)>,
    ready: Condvar,
}
impl Gate {
    fn run<T>(&self, background: bool, work: impl FnOnce() -> T) -> T {
        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if !background {
                state.1 += 1;
            }
            while state.0 >= 4 || (background && state.1 > 0) {
                state = self.ready.wait(state).unwrap_or_else(|e| e.into_inner());
            }
            state.0 += 1;
            if !background {
                state.1 -= 1;
            }
        }
        let result = work();
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.0 -= 1;
        self.ready.notify_all();
        result
    }
}

struct Thumb {
    name: String,
    model: bool,
    key: String,
}
pub struct Service {
    root: PathBuf,
    assets: FitAssets,
    gate: Gate,
    missing: Mutex<HashSet<String>>,
    pending: Mutex<HashSet<String>>,
    job: Mutex<Value>,
    job_running: AtomicBool,
    job_cancel: AtomicBool,
    fit_names: Mutex<Option<(String, Arc<Vec<String>>)>>,
    /// First failure reasons of the last Prepare-all run (diagnostics only).
    failures: Mutex<Vec<String>>,
}
pub fn service(root: &Path) -> &'static Service {
    static SERVICE: OnceLock<Service> = OnceLock::new();
    SERVICE.get_or_init(|| Service {
        root: root.to_path_buf(),
        assets: FitAssets::default(),
        gate: Gate::default(),
        missing: Mutex::new(HashSet::new()),
        pending: Mutex::new(HashSet::new()),
        job: Mutex::new(idle()),
        job_running: AtomicBool::new(false),
        job_cancel: AtomicBool::new(false),
        fit_names: Mutex::new(None),
        failures: Mutex::new(Vec::new()),
    })
}
fn idle() -> Value {
    json!({"phase":"idle","total":0,"completed":0,"ready":0,"missing":0,"failed":0,"batch":0,"batches":0,"current":"","error":""})
}
/// WPF ALLOWED: every catalog garment/shoe/accessory asset.
fn allowed_assets() -> Result<&'static HashSet<String>> {
    static ALLOWED: OnceLock<HashSet<String>> = OnceLock::new();
    if let Some(set) = ALLOWED.get() {
        return Ok(set);
    }
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(&include_bytes!("../assets/wardrobe.json.gz")[..])
        .read_to_end(&mut bytes)?;
    let wardrobe: Value = serde_json::from_slice(&bytes)?;
    let mut set: HashSet<String> = wardrobe
        .as_array()
        .context("Wardrobe missing")?
        .iter()
        .filter_map(|g| g["asset"].as_str().map(str::to_owned))
        .collect();
    for preset in &crate::outfits::catalog()?.presets {
        if matches!(preset.pool.as_deref(), Some("male" | "female")) {
            set.extend(preset.original.iter().cloned());
        }
    }
    set.retain(|a| a.len() <= 160 && a.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
    let _ = ALLOWED.set(set);
    Ok(ALLOWED.get().unwrap())
}
fn shoe_id(asset: &str) -> Option<&str> {
    static SHOE: OnceLock<Regex> = OnceLock::new();
    SHOE.get_or_init(|| Regex::new(r"^shoe_([0-9]{4})_[A-Za-z0-9_]+$").unwrap())
        .captures(asset)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str())
}
fn valid_png(bytes: &[u8]) -> bool {
    bytes.len() > 24
        && bytes.len() <= 1024 * 1024
        && bytes[..8] == [137, 80, 78, 71, 13, 10, 26, 10]
        && (1..=512).contains(&u32::from_be_bytes(bytes[16..20].try_into().unwrap()))
        && (1..=512).contains(&u32::from_be_bytes(bytes[20..24].try_into().unwrap()))
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
/// DDS top mip -> RGBA8 (texconv -f R8G8B8A8_UNORM for the formats used by
/// store thumbnails and shoe photos).
pub(crate) fn decode_dds(dds: &[u8]) -> Result<(u32, u32, Vec<u8>)> {
    ensure!(
        dds.len() >= 128 && &dds[..4] == b"DDS ",
        "Unsupported clothing preview texture."
    );
    let u32_at = |p: usize| u32::from_le_bytes(dds[p..p + 4].try_into().unwrap());
    let (height, width) = (u32_at(12), u32_at(16));
    ensure!(
        dds.len() <= 2 * 1024 * 1024 && (1..=512).contains(&width) && (1..=512).contains(&height),
        "Unsupported clothing preview texture."
    );
    let four_cc = &dds[84..88];
    let (format, data) = if four_cc == b"DX10" {
        ensure!(dds.len() >= 148, "Unsupported clothing preview texture.");
        (u32_at(128), &dds[148..])
    } else if four_cc == b"DXT1" {
        (71, &dds[128..])
    } else if four_cc == b"DXT5" {
        (77, &dds[128..])
    } else {
        anyhow::bail!("Unsupported clothing preview texture.")
    };
    let (w, h) = (width as usize, height as usize);
    let pixels = match format {
        97..=99 => bc7::image(data, w, h)?,
        70..=72 | 76..=78 => bc7::bc1_bc3(data, w, h, format >= 76)?,
        28 | 29 => {
            ensure!(
                data.len() >= w * h * 4,
                "Truncated clothing preview texture."
            );
            data[..w * h * 4].to_vec()
        }
        87 | 91 => {
            ensure!(
                data.len() >= w * h * 4,
                "Truncated clothing preview texture."
            );
            data[..w * h * 4]
                .chunks_exact(4)
                .flat_map(|p| [p[2], p[1], p[0], p[3]])
                .collect()
        }
        _ => anyhow::bail!("Unsupported clothing preview texture format {format}."),
    };
    Ok((width, height, pixels))
}
pub(crate) fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header()?.write_image_data(rgba)?;
    }
    Ok(out)
}

impl Service {
    fn game(&self, game_root: &Path) -> Result<Arc<Game>> {
        self.assets.game(game_root)
    }
    fn cache_dir(&self) -> PathBuf {
        self.root.join("Cache/PreviewImages/game-thumbnails")
    }
    fn thumb(&self, game: &Game, asset: &str) -> Result<Option<Thumb>> {
        let name = format!("store/thumbnails/{asset}.iff");
        let small = |n: &str| game.size(n).is_some_and(|s| s <= THUMB_LIMIT);
        let (name, model) = if small(&name) {
            (name, false)
        } else if let Some(id) = shoe_id(asset) {
            let model = format!("shoes/shoe_fe_{id}.iff");
            if !small(&model) {
                return Ok(None);
            }
            (model, true)
        } else {
            return Ok(None);
        };
        let mut digest = Sha256::new();
        digest.update(format!(
            "thumb-v1:{}:{}",
            game.key(&name)?,
            if model { name.as_str() } else { asset }
        ));
        Ok(Some(Thumb {
            name,
            model,
            key: hex(&digest.finalize()),
        }))
    }
    fn ready(&self, path: PathBuf, model: bool) -> Value {
        let mut result = json!({"state":"ready","imagePath":path});
        if model {
            result["previewKind"] = json!("shoe-model");
            result["note"] = json!("Model preview · colors may differ");
        }
        result
    }
    /// WPF ClothingThumbnails.get / getCached.
    pub fn thumbnail(
        &self,
        game_root: &Path,
        asset: &str,
        cached_only: bool,
        background: bool,
    ) -> Result<Value> {
        ensure!(
            allowed_assets()?.contains(asset),
            "Unknown clothing preview."
        );
        let game = self.game(game_root)?;
        let Some(thumb) = self.thumb(&game, asset)? else {
            return Ok(json!({"state":"missing"}));
        };
        if self
            .missing
            .lock()
            .map_err(|_| anyhow::anyhow!("Preview cache unavailable"))?
            .contains(&thumb.key)
        {
            return Ok(json!({"state":"missing"}));
        }
        let path = self.cache_dir().join(format!("{}.png", thumb.key));
        if fs::read(&path).is_ok_and(|b| valid_png(&b)) {
            return Ok(self.ready(path, thumb.model));
        }
        if cached_only {
            return Ok(json!({"state":"uncached"}));
        }
        {
            let mut pending = self
                .pending
                .lock()
                .map_err(|_| anyhow::anyhow!("Preview cache unavailable"))?;
            if pending.len() >= 64 && !pending.contains(&thumb.key) {
                return Ok(json!({"state":"busy"}));
            }
            pending.insert(thumb.key.clone());
        }
        let result = self
            .gate
            .run(background, || self.convert(&game, &thumb, &path));
        self.pending
            .lock()
            .map_err(|_| anyhow::anyhow!("Preview cache unavailable"))?
            .remove(&thumb.key);
        result
    }
    fn convert(&self, game: &Game, thumb: &Thumb, path: &Path) -> Result<Value> {
        if fs::read(path).is_ok_and(|b| valid_png(&b)) {
            return Ok(self.ready(path.to_path_buf(), thumb.model));
        }
        let container = game.export(&thumb.name, &|| Ok(()))?;
        ensure!(
            container.len() as u64 <= THUMB_LIMIT,
            "Clothing preview is too large."
        );
        let mut zip = zip::ZipArchive::new(Cursor::new(container))?;
        let textures: Vec<usize> = (0..zip.len())
            .filter(|i| {
                zip.name_for_index(*i)
                    .is_some_and(|n| n.to_lowercase().ends_with(".dds"))
            })
            .collect();
        if textures.len() != 1 {
            let mut missing = self
                .missing
                .lock()
                .map_err(|_| anyhow::anyhow!("Preview cache unavailable"))?;
            if missing.len() >= 4096 {
                missing.clear();
            }
            missing.insert(thumb.key.clone());
            return Ok(json!({"state":"missing"}));
        }
        let mut dds = Vec::new();
        zip.by_index(textures[0])?.read_to_end(&mut dds)?;
        let (width, height, rgba) = decode_dds(&dds)?;
        let png = encode_png(width, height, &rgba)?;
        ensure!(valid_png(&png), "Clothing preview conversion failed.");
        let dir = self.cache_dir();
        fs::create_dir_all(&dir)?;
        // Only our generated cache; never a game file.
        let temp = dir.join(format!(".{}.{}.tmp", thumb.key, std::process::id()));
        fs::write(&temp, &png)?;
        if fs::rename(&temp, path).is_err() {
            let _ = fs::remove_file(&temp);
        }
        Ok(self.ready(path.to_path_buf(), thumb.model))
    }
    fn prune(&self) {
        let Ok(entries) = fs::read_dir(self.cache_dir()) else {
            return;
        };
        let mut files: Vec<(PathBuf, u64, std::time::SystemTime)> = entries
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".png"))
            .filter_map(|e| {
                let m = e.metadata().ok()?;
                (m.is_file()).then(|| {
                    (
                        e.path(),
                        m.len(),
                        m.modified().unwrap_or(std::time::UNIX_EPOCH),
                    )
                })
            })
            .collect();
        let mut total: u64 = files.iter().map(|f| f.1).sum();
        files.sort_by_key(|f| f.2);
        for (path, size, _) in files {
            if total <= CACHE_LIMIT {
                break;
            }
            let _ = fs::remove_file(path);
            total -= size;
        }
    }
    /// WPF ClothingCacheJob.start.
    pub fn cache_start(&'static self, game_root: PathBuf, emit: Emit) -> Result<Value> {
        if self.job_running.swap(true, Ordering::SeqCst) {
            return self.cache_status();
        }
        self.job_cancel.store(false, Ordering::SeqCst);
        self.failures
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.publish(&emit, json!({"phase":"indexing","total":0,"completed":0,"ready":0,"missing":0,"failed":0,"batch":0,"batches":0,"current":"","error":""}));
        std::thread::spawn(move || {
            if let Err(error) = self.cache_run(&game_root, &emit) {
                let message: String = format!("{error:#}").chars().take(400).collect();
                self.publish(&emit, json!({"phase":"failed","error":message}));
            }
            self.prune();
            self.job_running.store(false, Ordering::SeqCst);
        });
        self.cache_status()
    }
    pub fn cache_cancel(&self, emit: &Emit) -> Result<Value> {
        if self.job_running.load(Ordering::SeqCst) {
            self.job_cancel.store(true, Ordering::SeqCst);
            self.publish(emit, json!({"phase":"cancelling"}));
        }
        self.cache_status()
    }
    pub fn cache_status(&self) -> Result<Value> {
        Ok(self
            .job
            .lock()
            .map_err(|_| anyhow::anyhow!("Preview cache unavailable"))?
            .clone())
    }
    fn publish(&self, emit: &Emit, patch: Value) {
        let snapshot = {
            let mut state = self.job.lock().unwrap_or_else(|e| e.into_inner());
            for (k, v) in patch.as_object().into_iter().flatten() {
                state[k] = v.clone();
            }
            state.clone()
        };
        emit("clothing-cache-status", snapshot);
    }
    /// Count one finished preview. Both counters change under one lock so the
    /// four workers never lose an increment (WPF's single JS thread could not).
    fn bump(&self, emit: &Emit, field: &str) {
        let snapshot = {
            let mut state = self.job.lock().unwrap_or_else(|e| e.into_inner());
            state[field] = json!(state[field].as_u64().unwrap_or(0) + 1);
            state["completed"] = json!(state["completed"].as_u64().unwrap_or(0) + 1);
            state.clone()
        };
        emit("clothing-cache-status", snapshot);
    }
    fn cache_run(&self, game_root: &Path, emit: &Emit) -> Result<()> {
        let game = self.game(game_root)?;
        let allowed = allowed_assets()?;
        // One preview per unique source (shoe photos are shared by colorways).
        let mut assets: Vec<&String> = allowed.iter().collect();
        assets.sort();
        let mut unique: HashMap<String, String> = HashMap::new();
        let mut order = Vec::new();
        for asset in assets {
            if self.job_cancel.load(Ordering::SeqCst) {
                break;
            }
            let store = format!("store/thumbnails/{asset}.iff");
            let small = |n: &str| game.size(n).is_some_and(|s| s <= THUMB_LIMIT);
            let source = if small(&store) {
                store
            } else if let Some(id) = shoe_id(asset) {
                let model = format!("shoes/shoe_fe_{id}.iff");
                if !small(&model) {
                    continue;
                }
                model
            } else {
                continue;
            };
            if !unique.contains_key(&source) {
                unique.insert(source, asset.clone());
                order.push(asset.clone());
            }
        }
        let batches = order.len().div_ceil(64);
        let cancelled = self.job_cancel.load(Ordering::SeqCst);
        self.publish(emit, json!({"total":order.len(),"batches":batches,"batch":0,"phase":if cancelled {"cancelling"} else {"running"}}));
        for (index, batch) in order.chunks(64).enumerate() {
            if self.job_cancel.load(Ordering::SeqCst) {
                break;
            }
            self.publish(emit, json!({"batch":index + 1}));
            let next = Mutex::new(0usize);
            std::thread::scope(|scope| {
                for _ in 0..4 {
                    scope.spawn(|| {
                        loop {
                            if self.job_cancel.load(Ordering::SeqCst) {
                                break;
                            }
                            let asset = {
                                let mut n = next.lock().unwrap_or_else(|e| e.into_inner());
                                let Some(asset) = batch.get(*n) else { break };
                                *n += 1;
                                asset.clone()
                            };
                            self.publish(emit, json!({"current":asset}));
                            let field = match self.thumbnail(game_root, &asset, false, true) {
                                Ok(v) if v["state"] == "ready" => "ready",
                                Ok(v) if v["state"] == "missing" => "missing",
                                other => {
                                    let reason = match other {
                                        Ok(v) => format!("{asset}: {}", v["state"]),
                                        Err(e) => format!("{asset}: {e:#}"),
                                    };
                                    let mut failures =
                                        self.failures.lock().unwrap_or_else(|e| e.into_inner());
                                    if failures.len() < 50 {
                                        failures.push(reason);
                                    }
                                    "failed"
                                }
                            };
                            self.bump(emit, field);
                        }
                    });
                }
            });
        }
        let phase = if self.job_cancel.load(Ordering::SeqCst) {
            "cancelled"
        } else {
            "complete"
        };
        self.publish(emit, json!({"phase":phase,"current":""}));
        Ok(())
    }
    /// WPF worker manifest(): names the mannequin preview may request.
    fn fit_index(&self, game_root: &Path, stamp: &str) -> Result<Value> {
        let game = self.game(game_root)?;
        let current = game.key("char/sig/png04090.iff")?;
        let mut cached = self
            .fit_names
            .lock()
            .map_err(|_| anyhow::anyhow!("Preview index unavailable"))?;
        if let Some((key, _)) = cached.as_ref().filter(|(k, _)| *k == current) {
            if key == stamp {
                return Ok(json!({"stamp":stamp,"names":[]}));
            }
        } else {
            static STYLE: OnceLock<Regex> = OnceLock::new();
            let style = STYLE.get_or_init(|| {
                Regex::new(r"^shoes/shoe_(?:style|ed_region)_[0-9]{4}\.iff$").unwrap()
            });
            let names: Vec<String> = ["clothing/", "char/sig/png04090.iff", "shoes/"]
                .iter()
                .flat_map(|prefix| game.names_with_prefix(prefix).collect::<Vec<_>>())
                .filter(|(name, size)| {
                    *size <= fit_assets::MAX_PACKED
                        && fit_assets::allowed(name)
                        && (name.starts_with("clothing/")
                            || *name == "char/sig/png04090.iff"
                            || *name == "shoes/shoe_global.iff"
                            || style.is_match(name))
                })
                .map(|(name, _)| name.to_owned())
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
            *cached = Some((current.clone(), Arc::new(names)));
        }
        let (key, names) = cached.as_ref().unwrap();
        Ok(json!({"stamp":key,"names":names.as_slice()}))
    }
    /// Items the mannequin previews (WPF FastClothingPreview.render ->
    /// composeStreetwear). WPF's creator had no "keep the player's shoes"
    /// choice: its default ('') was the style's original shoes, which the
    /// mannequin shows. Rust's "Currently installed" composes as `roster`
    /// (the shoe is removed for the game), so preview the style's shoes instead.
    fn fit_items(args: &Value) -> Result<Value> {
        let mut outfit: crate::backend::Outfit = serde_json::from_value(args.clone())?;
        ensure!(outfit.accessories.len() <= 3, "Maximum 3 accessories");
        ensure!(
            [
                &outfit.top,
                &outfit.bottom,
                &outfit.shoes,
                &outfit.socks,
                &outfit.street_id,
                &outfit.layer
            ]
            .into_iter()
            .chain(outfit.accessories.iter())
            .all(|s| s.len() <= 240),
            "Asset ID exceeds limit"
        );
        ensure!(
            outfit.pool == "male",
            "The mannequin preview currently supports men’s clothing."
        );
        if outfit.shoes == "Currently installed" {
            outfit.shoes.clear();
        }
        let catalog = crate::outfits::catalog()?;
        if let Ok(items) = catalog.selection(&outfit).and_then(|s| catalog.compose(&s)) {
            ensure!(items.len() <= 7, "Unsupported outfit preview.");
            return Ok(json!({"items":items,"warnings":[]}));
        }
        // Recover stale/missing IDs only when all compatible observed variants
        // produce the same preview. Do not choose between different layers.
        let top = if outfit.top == "Shirtless" {
            "NO SHIRT"
        } else {
            &outfit.top
        };
        let mut resolved: Option<Vec<String>> = None;
        let mut ambiguous = false;
        for preset in catalog.presets.iter().filter(|p| {
            p.pool.as_deref() == Some("male")
                && p.top.as_deref() == Some(top)
                && !p.id.starts_with("accessory-test-")
        }) {
            let mut candidate = outfit.clone();
            candidate.street_id = preset.id.clone();
            candidate.bottom = candidate.bottom.replace(' ', "_");
            if let Ok(items) = catalog
                .selection(&candidate)
                .and_then(|s| catalog.compose(&s))
            {
                if resolved.as_ref().is_some_and(|previous| previous != &items) {
                    ambiguous = true;
                    break;
                }
                resolved = Some(items);
            }
        }
        if !ambiguous {
            if let Some(items) = resolved {
                return Ok(json!({"items":items,"warnings":[],"variantRecovered":true}));
            }
        }
        // A visual draft is useful even before it has a verified game variant.
        // Do not rebind/save the draft or relax Catalog::selection/compose, which
        // remain authoritative for saving, export and game application.
        let mut warnings = vec!["Previewing your selected pieces. This combination still needs a compatible fit before it can be saved or applied.".to_string()];
        let base = catalog
            .base(&outfit.street_id)
            .ok()
            .filter(|p| p.pool.as_deref() == Some("male") && p.top.as_deref() == Some(top));
        let shoe = if outfit.shoes.is_empty() {
            base.and_then(|p| p.shoes.first())
                .map(String::as_str)
                .unwrap_or("")
        } else {
            &outfit.shoes
        };
        let mut items = Vec::new();
        for raw in [
            outfit.top.as_str(),
            outfit.bottom.as_str(),
            outfit.socks.as_str(),
            shoe,
            outfit.layer.as_str(),
        ]
        .into_iter()
        .chain(outfit.accessories.iter().map(String::as_str))
        {
            if matches!(
                raw,
                "" | "Shirtless" | "NO SHIRT" | "none" | "Currently installed"
            ) {
                continue;
            }
            // Older initial drafts used display labels instead of underscore IDs.
            let asset = raw.replace(' ', "_");
            if asset.len() > 200
                || !asset
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            {
                warnings.push(format!("Preview unavailable for {raw}."));
                continue;
            }
            if items.contains(&asset) {
                continue;
            }
            if items.len() == 7 {
                warnings.push(format!(
                    "{raw} is not shown: preview supports seven pieces."
                ));
                continue;
            }
            items.push(asset);
        }
        Ok(json!({"items":items,"warnings":warnings}))
    }
    /// Male preset originals for WPF previewNames (filtered in the worker).
    fn fit_originals() -> Result<Value> {
        let mut seen = HashSet::new();
        let originals: Vec<&String> = crate::outfits::catalog()?
            .presets
            .iter()
            .filter(|p| p.pool.as_deref() == Some("male"))
            .flat_map(|p| p.original.iter())
            .filter(|a| seen.insert(a.as_str()))
            .collect();
        Ok(json!(originals))
    }
    /// Decoded resource bytes for the mannequin worker (binary command).
    pub fn resource(&self, game_root: &Path, name: &str) -> Result<Vec<u8>> {
        ensure!(fit_assets::allowed(name), "Preview resource is not allowed");
        let game = self.game(game_root)?;
        let check = self.assets.checkpoint();
        game.export(name, &check)
    }
    pub fn dispatch(
        &'static self,
        channel: &str,
        args: &Value,
        game_root: &Path,
        emit: Emit,
    ) -> Result<Value> {
        let text = |key: &str| args[key].as_str().unwrap_or("").to_owned();
        match channel {
            "previews:ball-thumbnail" => {
                crate::ball_thumbnails::thumbnail(&self.root, game_root, &text("id"))
            }
            "previews:thumbnail" => {
                let asset = text("asset");
                self.thumbnail(
                    game_root,
                    &asset,
                    args["cachedOnly"].as_bool().unwrap_or(false),
                    false,
                )
            }
            "previews:cache-start" => self.cache_start(game_root.to_path_buf(), emit),
            "previews:cache-cancel" => self.cache_cancel(&emit),
            "previews:cache-status" => self.cache_status(),
            "previews:fit-index" => self.fit_index(game_root, &text("stamp")),
            "previews:fit-key" => {
                let name = text("name");
                Ok(json!({"key":self.game(game_root)?.key(&name)?}))
            }
            "previews:fit-originals" => Self::fit_originals(),
            "previews:fit-items" => Self::fit_items(args),
            "previews:fit-cancel" => {
                self.assets.cancel();
                Ok(json!(true))
            }
            _ => anyhow::bail!("Unknown preview request"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Read-only installed-game shoe identity investigation; requires C27_LIVE_GAME_DIR"]
    fn shoe_0215_installed_identity_read_only() {
        let root = PathBuf::from(std::env::var("C27_LIVE_GAME_DIR").unwrap());
        let assets = FitAssets::default();
        let game = assets.game(&root).unwrap();
        for id in ["0215", "0000", "1000"] {
            let name = format!("shoes/shoe_style_{id}.iff");
            println!("{name}: {:?}", game.size(&name));
            if id == "0215" && game.size(&name).is_some() {
                let bytes = game.export(&name, &assets.checkpoint()).unwrap();
                fs::write("artifacts/qa112-shoe-catalog/shoe_style_0215.iff", bytes).unwrap();
            }
        }
    }
    #[test]
    fn mannequin_unbound_and_stale_drafts_do_not_relax_game_validation() {
        let initial = json!({"name":"My outfit","top":"Shirtless","bottom":"basketball shorts solid blk","shoes":"Currently installed","accessories":[]});
        let preview = Service::fit_items(&initial).unwrap();
        assert_eq!(preview["items"], json!(["basketball_shorts_solid_blk"]));
        assert!(
            preview["variantRecovered"] == true
                || !preview["warnings"].as_array().unwrap().is_empty()
        );
        let catalog = crate::outfits::catalog().unwrap();
        let mut outfit: crate::backend::Outfit = serde_json::from_value(initial).unwrap();
        assert!(catalog.selection(&outfit).is_err());
        outfit.name.clear(); // Naming an outfit is required to save, not to view it.
        assert!(Service::fit_items(&serde_json::to_value(&outfit).unwrap()).is_ok());
        outfit.street_id = "removed-variant".into();
        outfit.top = "tanktop_solid_gry".into();
        outfit.shoes = "shoe_0000_default".into();
        let preview = Service::fit_items(&serde_json::to_value(&outfit).unwrap()).unwrap();
        for expected in [
            "tanktop_solid_gry",
            "basketball_shorts_solid_blk",
            "shoe_0000_default",
        ] {
            assert!(
                preview["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|item| item == expected)
            );
        }
        assert_eq!(outfit.street_id, "removed-variant");
        assert!(catalog.selection(&outfit).is_err());
        outfit.top = "../bad".into();
        let preview = Service::fit_items(&serde_json::to_value(&outfit).unwrap()).unwrap();
        assert!(
            !preview["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a == "../bad")
        );
        outfit.top = "Shirtless".into();
        outfit.bottom.clear();
        outfit.shoes.clear();
        assert_eq!(
            Service::fit_items(&serde_json::to_value(&outfit).unwrap()).unwrap()["items"],
            json!([])
        );
        outfit.pool = "female".into();
        assert!(Service::fit_items(&serde_json::to_value(&outfit).unwrap()).is_err());
    }
    #[test]
    fn mannequin_can_recover_an_unambiguous_variant_without_mutating_the_draft() {
        let catalog = crate::outfits::catalog().unwrap();
        for preset in catalog
            .presets
            .iter()
            .filter(|p| p.pool.as_deref() == Some("male"))
        {
            let Ok(mut outfit) = catalog.original_outfit(&preset.id, "Preview") else {
                continue;
            };
            let Ok(expected) = catalog.selection(&outfit).and_then(|s| catalog.compose(&s)) else {
                continue;
            };
            outfit.street_id = "stale-id".into();
            let args = serde_json::to_value(&outfit).unwrap();
            let result = Service::fit_items(&args).unwrap();
            if result["variantRecovered"] == true {
                assert_eq!(result["items"], json!(expected));
                assert_eq!(result["warnings"], json!([]));
                assert_eq!(args["streetId"], "stale-id");
                assert!(catalog.selection(&outfit).is_err());
                return;
            }
        }
        panic!("Expected at least one uniquely recoverable observed outfit");
    }
    #[test]
    fn gate_limits_concurrency() {
        let gate = Arc::new(Gate::default());
        let active = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let peak = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let threads: Vec<_> = (0..12)
            .map(|i| {
                let (gate, active, peak) = (gate.clone(), active.clone(), peak.clone());
                std::thread::spawn(move || {
                    gate.run(i % 2 == 0, || {
                        let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                        peak.fetch_max(now, Ordering::SeqCst);
                        std::thread::sleep(std::time::Duration::from_millis(5));
                        active.fetch_sub(1, Ordering::SeqCst);
                    })
                })
            })
            .collect();
        threads.into_iter().for_each(|t| t.join().unwrap());
        assert!(peak.load(Ordering::SeqCst) <= 4);
    }
    #[test]
    fn png_validation_matches_wpf() {
        let png = encode_png(2, 2, &[255; 16]).unwrap();
        assert!(valid_png(&png));
        assert!(!valid_png(&encode_png(600, 1, &vec![0; 2400]).unwrap()));
    }
    /// Read-only: C27_LIVE_GAME_DIR=<NBA 2K27> cargo test preview_service -- --ignored
    #[test]
    #[ignore]
    fn live_thumbnails_and_fit_index_read_only() {
        let game = PathBuf::from(std::env::var("C27_LIVE_GAME_DIR").unwrap());
        let root = std::env::temp_dir().join(format!("qa050-previews-{}", std::process::id()));
        let service = Box::leak(Box::new(Service {
            root: root.clone(),
            assets: FitAssets::default(),
            gate: Gate::default(),
            missing: Mutex::new(HashSet::new()),
            pending: Mutex::new(HashSet::new()),
            job: Mutex::new(idle()),
            job_running: AtomicBool::new(false),
            job_cancel: AtomicBool::new(false),
            fit_names: Mutex::new(None),
            failures: Mutex::new(Vec::new()),
        }));
        for asset in ["tanktop_solid_gry", "shoe_1042_default", "jogger_solid_blk"] {
            let started = std::time::Instant::now();
            let result = service.thumbnail(&game, asset, false, false).unwrap();
            println!("{asset}: {result} {} ms", started.elapsed().as_millis());
            if result["state"] == "ready" {
                assert!(valid_png(
                    &fs::read(result["imagePath"].as_str().unwrap()).unwrap()
                ));
                assert_eq!(
                    service.thumbnail(&game, asset, true, false).unwrap()["state"],
                    "ready"
                );
            }
        }
        let index = service.fit_index(&game, "").unwrap();
        let names = index["names"].as_array().unwrap();
        println!("fit index {} names", names.len());
        assert!(names.iter().any(|n| n == "clothing/clothing_itemtable.iff"));
        assert_eq!(
            service
                .fit_index(&game, index["stamp"].as_str().unwrap())
                .unwrap()["names"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn mannequin_keeps_style_shoes_for_currently_installed() {
        let catalog = crate::outfits::catalog().unwrap();
        let preset = catalog
            .presets
            .iter()
            .find(|p| {
                p.pool.as_deref() == Some("male")
                    && p.original.iter().filter(|a| shoe_id(a).is_some()).count() == 1
            })
            .unwrap();
        let mut outfit = catalog.original_outfit(&preset.id, "Outfit").unwrap();
        outfit.shoes = "Currently installed".into();
        // The game composition drops the shoe (player keeps theirs)...
        let game = catalog
            .compose(&catalog.selection(&outfit).unwrap())
            .unwrap();
        assert!(!game.iter().any(|a| shoe_id(a).is_some()), "{game:?}");
        // ...but the mannequin shows the style's original shoe, like WPF.
        let items = Service::fit_items(&serde_json::to_value(&outfit).unwrap()).unwrap();
        let items: Vec<String> = serde_json::from_value(items["items"].clone()).unwrap();
        assert!(items.iter().any(|a| shoe_id(a).is_some()), "{items:?}");
        // An explicit shoe choice is previewed as chosen.
        let shoe = preset
            .original
            .iter()
            .find(|a| shoe_id(a).is_some())
            .unwrap()
            .clone();
        outfit.shoes = shoe.clone();
        let items = Service::fit_items(&serde_json::to_value(&outfit).unwrap()).unwrap();
        assert!(
            items["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a == &json!(shoe))
        );
    }
    /// Read-only full "Prepare all previews" run into a temp cache:
    /// C27_LIVE_GAME_DIR=<NBA 2K27> cargo test prepare_all_live -- --ignored --nocapture
    #[test]
    #[ignore]
    fn prepare_all_live_read_only() {
        let game = PathBuf::from(std::env::var("C27_LIVE_GAME_DIR").unwrap());
        let root = std::env::temp_dir().join(format!("qa050-prepare-all-{}", std::process::id()));
        let service: &'static Service = Box::leak(Box::new(Service {
            root: root.clone(),
            assets: FitAssets::default(),
            gate: Gate::default(),
            missing: Mutex::new(HashSet::new()),
            pending: Mutex::new(HashSet::new()),
            job: Mutex::new(idle()),
            job_running: AtomicBool::new(false),
            job_cancel: AtomicBool::new(false),
            fit_names: Mutex::new(None),
            failures: Mutex::new(Vec::new()),
        }));
        let events = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = events.clone();
        let emit: Emit = Arc::new(move |_, _| {
            counter.fetch_add(1, Ordering::Relaxed);
        });
        let started = std::time::Instant::now();
        service.job_running.store(true, Ordering::SeqCst);
        service.cache_run(&game, &emit).unwrap();
        let elapsed = started.elapsed();
        let state = service.cache_status().unwrap();
        let (mut files, mut bytes) = (0u64, 0u64);
        for entry in fs::read_dir(service.cache_dir()).unwrap().flatten() {
            files += 1;
            bytes += entry.metadata().unwrap().len();
        }
        println!("FAILURES {:?}", service.failures.lock().unwrap());
        println!(
            "PREPARE-ALL {state} in {:.1} s, {files} PNG files, {:.1} MiB, {} events",
            elapsed.as_secs_f64(),
            bytes as f64 / 1048576.0,
            events.load(Ordering::Relaxed)
        );
        assert_eq!(state["phase"], "complete");
        assert_eq!(state["completed"], state["total"]);
        // Four store thumbnails need the C# one-block-less boundary fallback.
        assert_eq!(state["failed"], 0, "{:?}", service.failures.lock().unwrap());
        assert_eq!(state["ready"].as_u64().unwrap(), files);
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn shoe_ids() {
        assert_eq!(shoe_id("shoe_1042_default"), Some("1042"));
        assert_eq!(shoe_id("shoe_default"), None);
    }
}
