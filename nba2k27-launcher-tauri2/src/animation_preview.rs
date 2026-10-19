//! WPF player animation preview backend, in-process. Replaces WPF's
//! core/player-animation-preview.js service and its PlayerAnimationPreview.exe
//! worker (nba2k27-animation-lab portable/profile.py, engine.py, worker.py).
//! The browser roster editor keeps WPF's verbatim renderer/animation-preview JS
//! and roster-browser-bridge.js; this module answers the same calls:
//! openAnimationPreview, animationPreviewCatalog, animationPreviewRequest,
//! animationPreviewProgress, closeAnimationPreview, warmAnimationPreview and
//! the bridge's /animation-payload/<token> fetch.
//!
//! Game files are only read (manifest members via fit_assets, Oodle from the
//! installed DLL). Decoded archives live in <launcher>/Cache/player-animation-
//! preview/profiles/<id>/assets, prepared in a staging folder and published
//! atomically. No game memory is touched.
//!
//! Revision gate: WPF refused every executable except the one its mappings
//! were captured from. The 2026-10-16 game update changed NBA2K27.exe while
//! the mapping sources (ai_metadata/sigtable.iff, shotdata.iff) stayed byte-
//! identical, so this port gates on those verified mapping sources instead and
//! reports which motion archives differ from the verified set.
use crate::animation_codec::{self as codec, Archive, HandLibrary};
use crate::fit_assets::{FitAssets, Game};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, UNIX_EPOCH},
};

const CATALOG_GZ: &[u8] = include_bytes!("../assets/animation-preview/catalog.json.gz");
const SCHEMA: &[u8] = include_bytes!("../assets/animation-preview/schema.json");
const TIMING: &[u8] = include_bytes!("../assets/animation-preview/timing.json");
const REVISION: &[u8] = include_bytes!("../assets/animation-preview/revision.json");
const PROFILE_VERSION: u32 = 1;
const PRODUCER: &str = "rust-animation-preview-1";
const MAX_PAYLOAD_BYTES: usize = 32 * 1024 * 1024;
const MAX_CACHE_BYTES: usize = 96 * 1024 * 1024;
const MAX_TOKENS: usize = 16;
const TOKEN_TTL: Duration = Duration::from_secs(120);
const IDLE: Duration = Duration::from_secs(300);
const MAX_SESSIONS: usize = 8;
const MAX_WARM: usize = 64;
const CHARACTER: &str = "char/sig/png04090.iff";
const HAND_CLIP: &str = "hand_pose_male.SingleMoCap";
/// WPF portable bundle: motion archives indexed small-first, primary last.
const MOTION_ARCHIVES: [&str; 5] = [
    "animations_blacktop.iff",
    "animations_pregame_multi.iff",
    "animations_pregame_single.iff",
    "animations_slamball.iff",
    "nba_animations.iff",
];
const MAPPING_SOURCES: [&str; 2] = ["sigtable.iff", "shotdata.iff"];

struct Bundle {
    revision: Value,
    catalog: Value,
    schema: Value,
    timing: Value,
}
fn sha(bytes: &[u8]) -> String {
    codec::sha256_hex(bytes)
}
fn bundle() -> Result<&'static Bundle> {
    static BUNDLE: OnceLock<std::result::Result<Bundle, String>> = OnceLock::new();
    BUNDLE
        .get_or_init(|| {
            (|| -> Result<Bundle> {
                let revision: Value = serde_json::from_slice(REVISION)?;
                let hashes = &revision["metadata_sha256"];
                for (name, bytes) in [
                    ("catalog.json.gz", CATALOG_GZ),
                    ("schema.json", SCHEMA),
                    ("timing.json", TIMING),
                ] {
                    ensure!(
                        hashes[name].as_str() == Some(&sha(bytes)),
                        "The bundled animation mappings are damaged."
                    );
                }
                let mut text = Vec::new();
                flate2::read::GzDecoder::new(CATALOG_GZ)
                    .take(64 * 1024 * 1024)
                    .read_to_end(&mut text)?;
                let catalog: Value = serde_json::from_slice(&text)?;
                let timing: Value = serde_json::from_slice(TIMING)?;
                let choices = timing["choices"]
                    .as_array()
                    .context("Invalid verified release-speed table")?;
                ensure!(
                    choices.len() == 5
                        && choices.iter().enumerate().all(|(i, c)| c["value"].as_u64()
                            == Some(i as u64)
                            && c["multiplier"]
                                .as_f64()
                                .is_some_and(|m| (0.1..=10.0).contains(&m))),
                    "Invalid verified release-speed table"
                );
                ensure!(
                    catalog["fields"].as_array().is_some_and(|f| f.len() == 61),
                    "Unsupported animation catalog revision."
                );
                Ok(Bundle {
                    revision,
                    catalog,
                    schema: serde_json::from_slice(SCHEMA)?,
                    timing,
                })
            })()
            .map_err(|e| format!("{e:#}"))
        })
        .as_ref()
        .map_err(|e| anyhow::anyhow!("{e}"))
}
const MEMBER_DIGESTS: &[u8] = include_bytes!("../assets/animation-preview/member-digests.json");
fn member_digests() -> Result<&'static Value> {
    static DIGESTS: OnceLock<Value> = OnceLock::new();
    Ok(DIGESTS.get_or_init(|| serde_json::from_slice(MEMBER_DIGESTS).unwrap_or(Value::Null)))
}
/// tools/animation_preview_member_digests.py: sha256 over sorted members of
/// name + LF + sha256(content) + LF.
fn member_digest(zip_bytes: &[u8], only: Option<&str>) -> Result<String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes))?;
    let mut names: Vec<String> = zip.file_names().map(str::to_owned).collect();
    names.sort();
    let mut digest = Sha256::new();
    for name in names
        .iter()
        .filter(|n| only.is_none_or(|o| o == n.as_str()))
    {
        let mut content = Vec::new();
        zip.by_name(name)?
            .take(256 * 1024 * 1024)
            .read_to_end(&mut content)?;
        digest.update(format!("{name}\n{}\n", sha(&content)).as_bytes());
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}
fn asset_meta(name: &str) -> Result<&'static Value> {
    let meta = &bundle()?.revision["assets"][name];
    ensure!(
        meta.is_object(),
        "Animation asset is outside the verified profile"
    );
    Ok(meta)
}
fn resource(name: &str) -> Result<&'static str> {
    asset_meta(name)?["resource"]
        .as_str()
        .context("Invalid animation asset table")
}
fn stamp(path: &Path) -> Result<(u64, u128)> {
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        meta.is_file(),
        "Animation preview refuses redirected files or folders."
    );
    let modified = meta
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    Ok((meta.len(), modified))
}
fn file_sha(path: &Path, cancelled: &dyn Fn() -> bool) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        ensure!(!cancelled(), "Animation preview closed.");
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}
fn random_hex(bytes: usize) -> Result<String> {
    use windows::Win32::Security::Cryptography::{
        BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
    };
    let mut buffer = vec![0u8; bytes];
    unsafe { BCryptGenRandom(None, &mut buffer, BCRYPT_USE_SYSTEM_PREFERRED_RNG).ok()? };
    Ok(buffer.iter().map(|b| format!("{b:02x}")).collect())
}
fn uuid() -> Result<String> {
    let h = random_hex(16)?;
    Ok(format!(
        "{}-{}-4{}-a{}-{}",
        &h[..8],
        &h[8..12],
        &h[13..16],
        &h[17..20],
        &h[20..32]
    ))
}
fn plain_dir(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        meta.is_dir() && !meta.file_type().is_symlink(),
        "Animation preview refuses redirected files or folders."
    );
    Ok(())
}

/// Prepared profile: decoded archives published under profiles/<id>.
fn prepare(
    game: &Game,
    cache: &Path,
    progress: &dyn Fn(Value),
    cancelled: &dyn Fn() -> bool,
) -> Result<PathBuf> {
    let b = bundle()?;
    let profiles = cache.join("profiles");
    fs::create_dir_all(&profiles)?;
    plain_dir(cache)?;
    plain_dir(&profiles)?;
    let mut rows = serde_json::Map::new();
    for (name, meta) in b.revision["assets"]
        .as_object()
        .context("Invalid animation asset table")?
    {
        let resource = meta["resource"]
            .as_str()
            .context("Invalid animation asset table")?;
        let row = if resource == CHARACTER {
            json!({"key": game.key(CHARACTER)?})
        } else {
            let (archive, offset, size) = game
                .animation_row(resource)
                .context("Game manifest is missing required animation assets.")?;
            let source = stamp(&game.root.join(&archive))?;
            json!({"archive": archive, "offset": offset, "size": size, "archive_size": source.0, "archive_modified_ns": source.1.to_string()})
        };
        rows.insert(name.clone(), row);
    }
    let exe = stamp(&game.root.join("NBA2K27.exe"))?;
    let identity = json!({
        "version": PROFILE_VERSION, "producer": PRODUCER, "game": game.root.to_string_lossy(),
        "executable": [exe.0, exe.1.to_string()], "mapping_sha256": sha(REVISION), "rows": rows,
    });
    let key = sha(serde_json::to_string(&identity)?.as_bytes());
    let final_dir = profiles.join(&key);
    if let Ok(profile) = read_profile(&final_dir) {
        if profile["identity"] == identity {
            return Ok(final_dir);
        }
    }
    let staging = profiles.join(format!(".preparing-{}", random_hex(8)?));
    fs::create_dir_all(staging.join("assets"))?;
    let result = (|| -> Result<()> {
        let check = || -> Result<()> {
            ensure!(!cancelled(), "Animation preparation cancelled.");
            Ok(())
        };
        let exe_verified = file_sha(&game.root.join("NBA2K27.exe"), cancelled)?
            == b.revision["executable_sha256"].as_str().unwrap_or("");
        let names: Vec<&String> = b.revision["assets"].as_object().unwrap().keys().collect();
        let mut assets = serde_json::Map::new();
        for (index, name) in names.iter().enumerate() {
            check()?;
            progress(
                json!({"phase":"extracting","asset":name,"percent":(index * 100 / names.len())}),
            );
            let meta = asset_meta(name)?;
            let resource = resource(name)?;
            // WPF's decoder re-zipped members (.NET Optimal deflate); member
            // content is compared against WPF's verified profile instead.
            let expected = member_digests()?["assets"][name.as_str()]
                .as_str()
                .unwrap_or("");
            // (stored file, its sha256, its size, member digest)
            let (stored, digest, size, members) = if resource == CHARACTER {
                let bytes = game.export(CHARACTER, &check)?;
                fs::write(staging.join("assets/character.iff"), &bytes)?;
                (
                    Some("character.iff".to_owned()),
                    sha(&bytes),
                    bytes.len(),
                    member_digest(&bytes, None)?,
                )
            } else {
                let (bytes, _) = game.export_animation_source(resource, &check)?;
                if MAPPING_SOURCES.contains(&name.as_str()) {
                    // The roster value -> clip catalog was mined from these members.
                    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes[..]))?;
                    for (member, verified) in meta["verified_member_sha256"]
                        .as_object()
                        .context("Invalid animation asset table")?
                    {
                        let mut content = Vec::new();
                        zip.by_name(member)
                            .context("Signature mapping member is missing")?
                            .take(64 * 1024 * 1024)
                            .read_to_end(&mut content)?;
                        ensure!(
                            verified.as_str() == Some(&sha(&content)),
                            "This game revision's signature mappings differ from the verified animation catalog. The animation preview is unavailable; other editor functions still work."
                        );
                    }
                    (None, sha(&bytes), bytes.len(), member_digest(&bytes, None)?)
                } else if name.as_str() == "hands.iff" {
                    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
                    let mut member = zip
                        .by_name(HAND_CLIP)
                        .context("Hand pose library is missing from the game")?;
                    ensure!(
                        member.size() <= 16 * 1024 * 1024,
                        "Hand pose library exceeds its byte budget"
                    );
                    let mut clip = Vec::new();
                    member.read_to_end(&mut clip)?;
                    HandLibrary::from_clip_bytes(clip.clone())?;
                    fs::write(staging.join("assets").join(HAND_CLIP), &clip)?;
                    // Only the hand pose clip is kept from the 188 MB localbuildglobal.
                    let members = sha(format!("{HAND_CLIP}\n{}\n", sha(&clip)).as_bytes());
                    (Some(HAND_CLIP.to_owned()), sha(&clip), clip.len(), members)
                } else {
                    ensure!(
                        MOTION_ARCHIVES.contains(&name.as_str()),
                        "Unexpected animation asset {name}"
                    );
                    fs::write(staging.join("assets").join(name), &bytes)?;
                    let members = member_digest(&bytes, None)?;
                    (Some(name.to_string()), sha(&bytes), bytes.len(), members)
                }
            };
            assets.insert(
                name.to_string(),
                json!({"file": stored, "sha256": digest, "size": size, "member_digest": members,
                       "verified": members == expected}),
            );
        }
        check()?;
        // Character payload must build before the profile is published.
        let character = codec::read_character(&staging.join("assets/character.iff"))?;
        codec::character_payload(&character)?;
        let profile = json!({"version": PROFILE_VERSION, "id": key, "ready": true, "identity": identity,
                             "executable_verified": exe_verified, "assets": assets});
        fs::write(
            staging.join("profile.json"),
            serde_json::to_vec_pretty(&profile)?,
        )?;
        check()?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    if final_dir.exists() {
        let _ = fs::rename(
            &final_dir,
            profiles.join(format!("{key}.invalid-{}", random_hex(8)?)),
        );
    }
    if let Err(error) = fs::rename(&staging, &final_dir) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error.into());
    }
    // Earlier Rust-prepared profiles (older game revisions) are ours to prune.
    if let Ok(entries) = fs::read_dir(&profiles) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path == final_dir || !entry.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let ours = read_profile(&path).is_ok_and(|p| p["identity"]["producer"] == PRODUCER)
                || path
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().contains(".invalid-"));
            if ours {
                let _ = fs::remove_dir_all(&path);
            }
        }
    }
    Ok(final_dir)
}
fn read_profile(dir: &Path) -> Result<Value> {
    plain_dir(dir)?;
    let profile: Value = serde_json::from_slice(&fs::read(dir.join("profile.json"))?)?;
    ensure!(
        profile["version"] == PROFILE_VERSION && profile["ready"] == true,
        "Animation cache preparation is incomplete."
    );
    Ok(profile)
}

struct Work {
    archives: HashMap<String, Archive>,
    cache: VecDeque<(String, Arc<Vec<u8>>)>,
    cache_bytes: usize,
}
pub struct Engine {
    pub profile_id: String,
    dir: PathBuf,
    profile: Value,
    fields: HashMap<String, usize>,
    library: HandLibrary,
    stamps: Vec<(PathBuf, (u64, u128))>,
    work: Mutex<Work>,
    closed: Mutex<VecDeque<String>>,
}
impl Engine {
    fn open(
        dir: &Path,
        game_root: &Path,
        progress: &dyn Fn(Value),
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Self> {
        let b = bundle()?;
        let profile = read_profile(dir)?;
        let profile_id = profile["id"]
            .as_str()
            .context("Invalid animation profile")?
            .to_owned();
        ensure!(
            dir.file_name()
                .is_some_and(|n| n.to_string_lossy() == profile_id),
            "Animation profile is outside the managed cache."
        );
        let assets = profile["assets"]
            .as_object()
            .context("Invalid animation profile")?;
        // Every new engine re-hashes its assets (WPF: proofs are process-local).
        let files: Vec<(&String, &Value)> = assets
            .iter()
            .filter(|(_, a)| a["file"].is_string())
            .collect();
        let mut stamps = vec![
            (
                game_root.join("NBA2K27.exe"),
                stamp(&game_root.join("NBA2K27.exe"))?,
            ),
            (
                game_root.join("manifest"),
                stamp(&game_root.join("manifest"))?,
            ),
            (dir.join("profile.json"), stamp(&dir.join("profile.json"))?),
        ];
        for (i, (name, asset)) in files.iter().enumerate() {
            progress(json!({"phase":"checking","asset":name,"percent":i * 100 / files.len()}));
            let file = dir.join("assets").join(asset["file"].as_str().unwrap());
            ensure!(
                file_sha(&file, cancelled)? == asset["sha256"].as_str().unwrap_or(""),
                "Animation cache is missing or damaged: {name}. Reopen the preview to prepare it again."
            );
            stamps.push((file.clone(), stamp(&file)?));
        }
        let mut archives = HashMap::new();
        for (i, name) in MOTION_ARCHIVES.iter().enumerate() {
            ensure!(!cancelled(), "Animation preparation cancelled.");
            progress(
                json!({"phase":"indexing","asset":name,"percent":i * 100 / MOTION_ARCHIVES.len()}),
            );
            let mut archive = Archive::open(&dir.join("assets").join(name))?;
            archive.index(cancelled)?;
            archives.insert(name.to_string(), archive);
        }
        progress(json!({"phase":"model","percent":95}));
        let library = HandLibrary::from_clip_bytes(fs::read(dir.join("assets").join(HAND_CLIP))?)?;
        let fields = b.catalog["fields"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .filter_map(|(i, f)| f["key"].as_str().map(|k| (k.to_owned(), i)))
            .collect();
        let engine = Self {
            profile_id,
            dir: dir.to_owned(),
            profile,
            fields,
            library,
            stamps,
            work: Mutex::new(Work {
                archives,
                cache: VecDeque::new(),
                cache_bytes: 0,
            }),
            closed: Mutex::new(VecDeque::new()),
        };
        engine.character()?;
        Ok(engine)
    }
    fn check_current(&self) -> Result<()> {
        for (path, saved) in &self.stamps {
            ensure!(
                stamp(path).ok().as_ref() == Some(saved),
                "Game or animation cache changed. Prepare the preview again."
            );
        }
        Ok(())
    }
    fn work(&self) -> Result<std::sync::MutexGuard<'_, Work>> {
        self.work
            .lock()
            .map_err(|_| anyhow::anyhow!("Animation preview is unavailable"))
    }
    fn variants(choice: &'static Value) -> Result<&'static Vec<Value>> {
        let b = bundle()?;
        let list = match choice["variant_set"].as_str() {
            Some(set) => &b.catalog["variant_sets"][set],
            None => &choice["variants"],
        };
        list.as_array()
            .context("Invalid animation catalog variants")
    }
    fn previewable(v: &Value) -> bool {
        v["previewable"].as_bool().unwrap_or(true)
            && v["member"].as_str().is_some_and(|m| !m.is_empty())
    }
    pub fn menu(&self) -> Result<Value> {
        self.check_current()?;
        let b = bundle()?;
        let fields = b.catalog["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| {
                let choices = f["choices"].as_array().context("Invalid animation catalog")?.iter().map(|c| {
                    Ok(json!({"value":c["value"],"label":c["label"],"status":c["status"],
                              "preview_count":Self::variants(c)?.iter().filter(|v| Self::previewable(v)).count()}))
                }).collect::<Result<Vec<_>>>()?;
                Ok(json!({"key":f["key"],"label":f["label"],"choices":choices}))
            })
            .collect::<Result<Vec<_>>>()?;
        let settings: Vec<&Value> = b
            .schema
            .as_array()
            .context("Invalid animation schema")?
            .iter()
            .filter(|f| matches!(f["key"].as_str(), Some("ShotTiming" | "ShotBlendingRatio")))
            .collect();
        let assets = &self.profile["assets"];
        let differing: Vec<&String> = assets
            .as_object()
            .unwrap()
            .iter()
            .filter(|(_, a)| a["verified"] != true && a["file"].is_string())
            .map(|(n, _)| n)
            .collect();
        Ok(json!({
            "profile_id": self.profile_id, "fields": fields,
            "release_timing": {"available": true, "choices": b.timing["choices"], "default": 2, "field": "ShotTiming"},
            "setting_fields": settings,
            "revision": {"executable_verified": self.profile["executable_verified"], "mapping_sources_verified": true,
                         "assets_differing_from_verified": differing},
        }))
    }
    pub fn resolve(&self, field: &str, value: i64) -> Result<Value> {
        self.check_current()?;
        let b = bundle()?;
        let index = *self
            .fields
            .get(field)
            .context("Unknown player animation field")?;
        let choices = b.catalog["fields"][index]["choices"]
            .as_array()
            .context("Invalid animation catalog")?;
        ensure!(
            value >= 0 && (value as usize) < choices.len(),
            "Invalid roster choice"
        );
        let choice = &choices[value as usize];
        let variants: Vec<Value> = Self::variants(choice)?
            .iter()
            .filter(|v| Self::previewable(v))
            .enumerate()
            .map(|(i, v)| {
                let mut v = v.clone();
                v["variant_index"] = json!(i);
                v
            })
            .collect();
        let status = choice["status"].as_str().unwrap_or("");
        let reason = match choice["reason"].as_str().filter(|r| !r.is_empty()) {
            Some(r) => r.to_owned(),
            None if status == "default" => "The game chooses a contextual default".into(),
            None if status == "none" => "No signature animation selected".into(),
            None => String::new(),
        };
        Ok(
            json!({"field":field,"value":value,"label":choice["label"],"status":status,"reason":reason,
                  "variants":variants,"profile_id":self.profile_id}),
        )
    }
    fn check_session(&self, session: &str) -> Result<()> {
        ensure!(
            (1..=128).contains(&session.len()),
            "Invalid preview session"
        );
        let closed = self
            .closed
            .lock()
            .map_err(|_| anyhow::anyhow!("Animation preview is unavailable"))?;
        ensure!(
            !closed.iter().any(|s| s == session),
            "Preview session is closed"
        );
        Ok(())
    }
    fn cached(
        &self,
        work: &mut Work,
        key: String,
        create: impl FnOnce(&mut Work) -> Result<Value>,
    ) -> Result<Arc<Vec<u8>>> {
        if let Some(i) = work.cache.iter().position(|(k, _)| *k == key) {
            let item = work.cache.remove(i).unwrap();
            let raw = item.1.clone();
            work.cache.push_back(item);
            return Ok(raw);
        }
        let raw = Arc::new(serde_json::to_vec(&create(work)?)?);
        ensure!(
            raw.len() <= MAX_PAYLOAD_BYTES,
            "This motion exceeds the preview payload budget."
        );
        while work.cache_bytes + raw.len() > MAX_CACHE_BYTES {
            let Some((_, old)) = work.cache.pop_front() else {
                break;
            };
            work.cache_bytes -= old.len();
        }
        work.cache_bytes += raw.len();
        work.cache.push_back((key, raw.clone()));
        Ok(raw)
    }
    /// engine.decode: one roster selection's motion as the lab's clip JSON.
    pub fn decode(
        &self,
        session: &str,
        field: &str,
        value: i64,
        variant: i64,
        shot: bool,
    ) -> Result<Arc<Vec<u8>>> {
        let selected = self.resolve(field, value)?;
        let variants = selected["variants"].as_array().unwrap();
        ensure!(
            !variants.is_empty(),
            "{}",
            selected["reason"]
                .as_str()
                .filter(|r| !r.is_empty())
                .unwrap_or("This selection has no preview")
        );
        ensure!(
            variant >= 0 && (variant as usize) < variants.len(),
            "Invalid package variant"
        );
        let v = &variants[variant as usize];
        let archive = Path::new(v["archive"].as_str().unwrap_or("inputs/nba_animations.iff"))
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        ensure!(
            MOTION_ARCHIVES.contains(&archive.as_str()),
            "Animation archive is outside the verified profile"
        );
        let name = v["member"].as_str().unwrap().to_owned();
        let mut work = self.work()?;
        self.check_session(session)?;
        let key = format!(
            "{}|{archive}|{name}|{field}|{value}|{shot}|60",
            self.profile_id
        );
        let path = self.dir.join("assets").join(&archive);
        let library = &self.library;
        let profile_id = self.profile_id.clone();
        self.cached(&mut work, key, |work| {
            let entry = work.archives.get_mut(&archive).context("Animation archive is unavailable")?;
            let container = if name.ends_with(".MultipleMoCap") { Some(entry.load_multiple(&name)?) } else { None };
            let (clip, actor) = match &container {
                Some(container) => {
                    let actor = container
                        .actors
                        .iter()
                        .find(|a| a.joint_count == 22)
                        .map(|a| a.actor_index)
                        .context("This animation contains no player actor")?;
                    (container.actor(actor)?, actor)
                }
                None => (entry.load(&name)?, 0),
            };
            ensure!(clip.bone_count == 22, "This animation contains no player actor");
            let seconds = clip.ticks.last().unwrap() / (clip.rate as f64 * clip.speed);
            ensure!(seconds.is_finite() && seconds > 0.0 && seconds <= 180.0, "Motion duration exceeds the preview budget");
            let mut data = codec::decode_loaded_clip(
                &clip,
                &path.to_string_lossy(),
                &name,
                60,
                container.as_ref().map(|c| (c, actor)),
                Some(library),
            )?;
            if let Some(container) = &container {
                let first = codec::sample_trajectory(&clip, 0.0)?;
                let info = &container.actors[actor];
                let word = info.orientation_word_raw as i64;
                let position = if first.0 == 0.0 && first.1 == 0.0 {
                    json!([info.position_xz_raw.0, info.position_xz_raw.1])
                } else {
                    json!([0, 0])
                };
                data.insert("use_container_placement".into(), json!(true));
                data.insert(
                    "preview_anchor_placement".into(),
                    json!({"position_xz_cm": position, "heading_units": if word & 32768 != 0 { word - 65536 } else { word }}),
                );
            }
            if shot {
                // Shot composer uses actor movement in the lab's model-space convention.
                data.remove("use_container_placement");
                data.insert("shot_events".into(), json!(codec::shot_events(&clip)?));
            }
            data.insert("roster_selection".into(), json!({"field":selected["field"],"value":selected["value"],"label":selected["label"]}));
            data.insert("profile_id".into(), json!(profile_id));
            Ok(Value::Object(data))
        })
    }
    pub fn character(&self) -> Result<Arc<Vec<u8>>> {
        self.check_current()?;
        let mut work = self.work()?;
        let path = self.dir.join("assets/character.iff");
        self.cached(&mut work, format!("character|{}", self.profile_id), |_| {
            codec::character_payload(&codec::read_character(&path)?)
        })
    }
    fn close_session(&self, session: &str) {
        if let Ok(mut closed) = self.closed.lock() {
            closed.push_back(session.to_owned());
            while closed.len() > 4096 {
                closed.pop_front();
            }
        }
    }
    fn stats(&self) -> Value {
        let work = self.work();
        json!({"cache_bytes": work.as_ref().map(|w| w.cache_bytes).unwrap_or(0), "cache_count": work.as_ref().map(|w| w.cache.len()).unwrap_or(0)})
    }
}

const OWNER_TTL: Duration = Duration::from_secs(120);
type Clock = Arc<dyn Fn() -> Instant + Send + Sync>;

struct Token {
    owner: String,
    session: String,
    bytes: Arc<Vec<u8>>,
    expires: Instant,
}
#[derive(Default)]
struct State {
    sessions: HashMap<String, String>,
    warm: HashSet<String>,
    tokens: HashMap<String, Token>,
    progress: Option<Value>,
    closing: bool,
    last_error: Option<String>,
    owners: HashMap<String, Instant>,
    engine: Option<(PathBuf, Arc<Engine>)>,
    idle_since: Option<Instant>,
    warm_running: bool,
    warm_revision: u64,
}
impl State {
    fn needed(&self) -> bool {
        !self.closing && (!self.sessions.is_empty() || !self.warm.is_empty())
    }
    fn settled(&mut self, now: Instant) {
        self.owners.retain(|owner, _| {
            self.warm.contains(owner) || self.sessions.values().any(|o| o == owner)
        });
        if self.needed() {
            self.idle_since = None;
        } else if self.idle_since.is_none() {
            self.idle_since = Some(now);
        }
    }
    /// No engine calls while state is locked. Revoke every expired owner first.
    fn reap(&mut self, now: Instant) -> Cleanup {
        let expired: HashSet<_> = self
            .owners
            .iter()
            .filter(|(_, seen)| now.saturating_duration_since(**seen) >= OWNER_TTL)
            .map(|(o, _)| o.clone())
            .collect();
        let removed: Vec<_> = self
            .sessions
            .iter()
            .filter(|(_, owner)| expired.contains(*owner))
            .map(|(s, _)| s.clone())
            .collect();
        for session in &removed {
            self.sessions.remove(session);
        }
        self.warm.retain(|o| !expired.contains(o));
        self.owners.retain(|o, _| !expired.contains(o));
        let dead: Vec<_> = self
            .tokens
            .iter()
            .filter(|(_, t)| t.expires <= now || !self.sessions.contains_key(&t.session))
            .map(|(id, _)| id.clone())
            .collect();
        let tokens = dead
            .into_iter()
            .filter_map(|id| self.tokens.remove(&id))
            .collect();
        self.settled(now);
        let sessions = self
            .engine
            .as_ref()
            .map(|(_, e)| removed.into_iter().map(|s| (e.clone(), s)).collect())
            .unwrap_or_default();
        let engine = if !self.needed()
            && self
                .idle_since
                .is_some_and(|since| now.saturating_duration_since(since) >= IDLE)
        {
            self.engine.take()
        } else {
            None
        };
        Cleanup {
            sessions,
            _tokens: tokens,
            _engine: engine,
        }
    }
}
#[derive(Default)]
struct Cleanup {
    sessions: Vec<(Arc<Engine>, String)>,
    _tokens: Vec<Token>,
    _engine: Option<(PathBuf, Arc<Engine>)>,
}
impl Cleanup {
    fn finish(self) {
        for (engine, session) in &self.sessions {
            engine.close_session(session);
        }
    }
}
pub type GameRoot = Arc<dyn Fn() -> Result<PathBuf> + Send + Sync>;
pub struct Service {
    cache: PathBuf,
    game_root: GameRoot,
    fit: FitAssets,
    state: Mutex<State>,
    /// Serializes preparation; holders publish `engine`.
    prepare: Mutex<()>,
    clock: Clock,
    sweep_stop: AtomicBool,
}
fn valid_owner(owner: &str) -> bool {
    (1..=64).contains(&owner.len()) && owner.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
}
impl Service {
    pub fn new(launcher_root: &Path, game_root: GameRoot) -> Arc<Self> {
        Self::with_clock(launcher_root, game_root, Arc::new(Instant::now), true)
    }
    fn with_clock(
        launcher_root: &Path,
        game_root: GameRoot,
        clock: Clock,
        sweep: bool,
    ) -> Arc<Self> {
        let service = Arc::new(Self {
            cache: launcher_root.join("Cache/player-animation-preview"),
            game_root,
            fit: FitAssets::default(),
            state: Mutex::new(State::default()),
            prepare: Mutex::new(()),
            clock,
            sweep_stop: AtomicBool::new(false),
        });
        if sweep {
            let weak = Arc::downgrade(&service);
            let worker = std::thread::Builder::new()
                .name("animation-preview-lifecycle".into())
                .spawn(move || {
                    loop {
                        std::thread::sleep(Duration::from_secs(1));
                        let Some(service) = weak.upgrade() else { break };
                        if service.sweep_stop.load(Ordering::Acquire) {
                            break;
                        }
                        service.maintain();
                    }
                });
            if let Err(error) = worker {
                if let Ok(mut state) = service.state() {
                    state.closing = true;
                    state.last_error =
                        Some(format!("Animation lifecycle worker unavailable: {error}"));
                }
            }
        }
        service
    }
    #[cfg(test)]
    pub(crate) fn test_with_clock(clock: Arc<dyn Fn() -> Instant + Send + Sync>) -> Arc<Self> {
        Self::with_clock(
            &std::env::temp_dir().join("c27-animation-clock-fixture"),
            Arc::new(|| bail!("No game in clock fixture")),
            clock,
            false,
        )
    }
    fn maintain(&self) {
        let cleanup = self
            .state()
            .map(|mut s| s.reap((self.clock)()))
            .unwrap_or_default();
        cleanup.finish();
    }
    /// Reap and admission share one lock and one fresh monotonic sample.
    fn update<T>(&self, apply: impl FnOnce(&mut State, Instant) -> Result<T>) -> Result<T> {
        let mut state = self.state()?;
        let now = (self.clock)();
        let cleanup = state.reap(now);
        let result = apply(&mut state, now);
        drop(state);
        cleanup.finish();
        result
    }
    /// Authenticated browser heartbeat: never registers an unknown/expired owner.
    pub fn heartbeat(&self, owner: &str) {
        let _ = self.update(|s, now| {
            if !s.closing {
                if let Some(seen) = s.owners.get_mut(owner) {
                    *seen = now;
                }
            }
            Ok(())
        });
    }
    fn state(&self) -> Result<std::sync::MutexGuard<'_, State>> {
        self.state
            .lock()
            .map_err(|_| anyhow::anyhow!("Animation preview is unavailable"))
    }
    fn needed(&self) -> bool {
        self.maintain();
        self.state().is_ok_and(|s| s.needed())
    }
    fn session(&self, owner: &str, session: &str) -> Result<()> {
        self.update(|state, now| {
            ensure!(
                !state.closing && state.sessions.get(session).is_some_and(|o| o == owner),
                "Animation preview expired. Reopen it to retry."
            );
            state.owners.insert(owner.to_owned(), now);
            Ok(())
        })
    }
    /// animation:open
    pub fn open(&self, owner: &str) -> Result<Value> {
        ensure!(
            valid_owner(owner),
            "Open the roster editor from the launcher."
        );
        self.update(|state, now| {
            ensure!(
                !state.closing && state.sessions.len() < MAX_SESSIONS,
                "Close another animation preview before opening this one."
            );
            let id = uuid()?;
            state.sessions.insert(id.clone(), owner.to_owned());
            state.owners.insert(owner.to_owned(), now);
            state.idle_since = None;
            Ok(json!({"session": id}))
        })
    }
    /// animation:warm (WPF setWarm): background preparation while an editor is open.
    pub fn warm(self: &Arc<Self>, owner: &str, enabled: bool) -> Result<Value> {
        ensure!(
            valid_owner(owner),
            "Open the roster editor from the launcher."
        );
        let spawn = self.update(|s, now| {
            if !enabled {
                if s.warm.remove(owner) {
                    s.warm_revision = s.warm_revision.wrapping_add(1);
                }
                s.settled(now);
                return Ok(false);
            }
            ensure!(!s.closing, "Animation preview is closing.");
            ensure!(
                s.warm.contains(owner) || s.warm.len() < MAX_WARM,
                "Close another editor first."
            );
            if s.warm.insert(owner.to_owned()) {
                s.warm_revision = s.warm_revision.wrapping_add(1);
            }
            s.owners.insert(owner.to_owned(), now);
            s.idle_since = None;
            if s.warm_running {
                return Ok(false);
            }
            s.warm_running = true;
            Ok(true)
        })?;
        if !spawn {
            return Ok(json!({"warming":enabled}));
        }
        let service = self.clone();
        if let Err(e) = std::thread::Builder::new()
            .name("animation-preview-warm".into())
            .spawn(move || {
                loop {
                    let revision = match service.state() {
                        Ok(s) => s.warm_revision,
                        Err(_) => break,
                    };
                    let result = service.ensure();
                    let Ok(mut s) = service.state() else { break };
                    if let Err(error) = result {
                        if !s.closing {
                            s.last_error = Some(format!("{error:#}"));
                        }
                    }
                    if s.needed() && s.warm_revision != revision {
                        continue;
                    }
                    s.warm_running = false;
                    break;
                }
            })
        {
            self.state()?.warm_running = false;
            return Err(e.into());
        }
        Ok(json!({"warming":true}))
    }
    fn publish(&self, row: Value) {
        if let Ok(mut state) = self.state() {
            if !state.closing {
                state.progress = Some(row);
            }
        }
    }
    fn ensure(&self) -> Result<Arc<Engine>> {
        let game_root = (self.game_root)()?;
        ensure!(
            game_root.is_absolute(),
            "Select your NBA 2K27 folder in Settings first."
        );
        let _preparing = self
            .prepare
            .lock()
            .map_err(|_| anyhow::anyhow!("Animation preview is unavailable"))?;
        let cached = { self.state()?.engine.clone() };
        if let Some((root, engine)) = cached {
            if root == game_root && engine.check_current().is_ok() {
                return Ok(engine);
            }
        }
        let retired = { self.state()?.engine.take() };
        drop(retired);
        ensure!(self.needed(), "Animation preview closed.");
        let cancelled = || !self.needed();
        let progress = |row: Value| self.publish(row);
        self.publish(json!({"phase":"checking","percent":0}));
        let result = (|| {
            let game = self.fit.game(&game_root)?;
            let dir = prepare(&game, &self.cache, &progress, &cancelled)?;
            Engine::open(&dir, &game.root, &progress, &cancelled)
        })();
        let engine = match result {
            Ok(engine) => Arc::new(engine),
            Err(error) => {
                self.publish(json!({"phase":"error","percent":0,"message":format!("{error:#}")}));
                return Err(error);
            }
        };
        self.publish_engine(game_root, engine.clone())?;
        Ok(engine)
    }
    fn publish_engine(&self, game_root: PathBuf, engine: Arc<Engine>) -> Result<()> {
        // One lock covers fresh expiry, interest, shutdown and slot publication.
        self.update(|state, _| {
            ensure!(state.needed(), "Animation preview closed.");
            state.progress = Some(json!({"phase":"ready","percent":100}));
            state.engine = Some((game_root, engine));
            Ok(())
        })
    }
    /// animation:catalog (WPF capabilities): waits for preparation.
    pub fn catalog(&self, owner: &str, session: &str) -> Result<Value> {
        self.session(owner, session)?;
        let engine = self.ensure()?;
        self.session(owner, session)?;
        let catalog = engine.menu()?;
        ensure!(
            catalog["fields"].as_array().is_some_and(|f| f.len() == 61)
                && catalog["release_timing"]["choices"]
                    .as_array()
                    .is_some_and(|c| c.len() == 5),
            "Unsupported animation catalog revision."
        );
        Ok(catalog)
    }
    /// animation:progress-status
    pub fn progress(&self, owner: &str, session: &str) -> Result<Value> {
        self.session(owner, session)?;
        let state = self.state()?;
        let mut row = state.progress.clone().unwrap_or_else(|| json!({}));
        row["session"] = json!(session);
        Ok(row)
    }
    /// animation:request {op, args}
    pub fn request(&self, owner: &str, input: &Value) -> Result<Value> {
        let op = input["op"].as_str().unwrap_or("");
        ensure!(
            matches!(op, "resolve" | "load" | "shot-part" | "character" | "stats"),
            "Unsupported animation operation."
        );
        let args = &input["args"];
        let session = args["session"].as_str().unwrap_or("");
        self.session(owner, session)?;
        let engine = self.ensure()?;
        self.session(owner, session)?;
        let field = args["field"].as_str().unwrap_or("");
        let value = args["value"].as_i64();
        let integer = |key: &str, max: i64| -> Result<i64> {
            match &args[key] {
                Value::Null => Ok(0),
                v => v
                    .as_i64()
                    .filter(|n| (0..=max).contains(n))
                    .context("Invalid animation request."),
            }
        };
        let (variant, request) = (
            integer("variant", 10000)?,
            integer("request", 9_007_199_254_740_991)?,
        );
        if op != "character" && op != "stats" {
            ensure!(
                engine.fields.contains_key(field)
                    && value.is_some_and(|v| (0..=65535).contains(&v)),
                "Invalid animation field or roster value."
            );
            ensure!(
                op != "shot-part" || matches!(field, "ShotJumper" | "ShotRelease" | "ShotRelease2"),
                "Invalid jump-shot part."
            );
        }
        let bytes = match op {
            "resolve" => return engine.resolve(field, value.unwrap()),
            "stats" => return Ok(engine.stats()),
            "load" => engine.decode(session, field, value.unwrap(), variant, false)?,
            "shot-part" => engine.decode(session, field, value.unwrap(), 0, true)?,
            _ => engine.character()?,
        };
        self.session(owner, session)?;
        let token = random_hex(24)?;
        self.insert_token(owner, session, token.clone(), bytes)?;
        Ok(
            json!({"url": format!("preview-image://images/__animation/{token}"), "session": session,
                  "request": request, "profile": engine.profile_id}),
        )
    }
    fn insert_token(
        &self,
        owner: &str,
        session: &str,
        token: String,
        bytes: Arc<Vec<u8>>,
    ) -> Result<()> {
        ensure!(
            bytes.len() <= MAX_PAYLOAD_BYTES,
            "This motion exceeds the preview payload budget."
        );
        self.update(|state, now| {
            ensure!(
                !state.closing && state.sessions.get(session).is_some_and(|o| o == owner),
                "Animation preview expired. Reopen it to retry."
            );
            while state.tokens.len() >= MAX_TOKENS {
                let oldest = state
                    .tokens
                    .iter()
                    .min_by_key(|(_, t)| t.expires)
                    .map(|(k, _)| k.clone())
                    .unwrap();
                state.tokens.remove(&oldest);
            }
            state.tokens.insert(
                token,
                Token {
                    owner: owner.to_owned(),
                    session: session.to_owned(),
                    bytes,
                    expires: now + TOKEN_TTL,
                },
            );
            Ok(())
        })
    }
    /// The bridge's /animation-payload/<token> fetch (WPF readToken).
    pub fn payload(&self, owner: &str, token: &str) -> Option<Arc<Vec<u8>>> {
        self.update(|state, now| {
            let Some(row) = state.tokens.get(token) else {
                return Ok(None);
            };
            if row.owner != owner
                || row.expires <= now
                || state.closing
                || state.sessions.get(&row.session) != Some(&row.owner)
            {
                return Ok(None);
            }
            let bytes = row.bytes.clone();
            state.owners.insert(owner.to_owned(), now);
            Ok(Some(bytes))
        })
        .ok()
        .flatten()
    }
    /// animation:close
    pub fn close(self: &Arc<Self>, owner: &str, session: &str) -> Result<Value> {
        self.maintain();
        let engine = {
            let mut state = self.state()?;
            match state.sessions.get(session) {
                None => return Ok(json!({"closed": true})),
                Some(o) if o != owner => bail!("Animation preview expired. Reopen it to retry."),
                _ => {}
            }
            state.sessions.remove(session);
            state.tokens.retain(|_, t| t.session != session);
            state.settled((self.clock)());
            state.engine.clone()
        };
        if let Some((_, engine)) = engine {
            engine.close_session(session);
        }
        Ok(json!({"closed": true}))
    }
    pub fn shutdown(&self) {
        self.sweep_stop.store(true, Ordering::Release);
        let engine = if let Ok(mut s) = self.state() {
            s.closing = true;
            s.sessions.clear();
            s.warm.clear();
            s.tokens.clear();
            s.owners.clear();
            s.engine.take()
        } else {
            None
        };
        drop(engine);
    }
}

#[cfg(test)]
#[path = "animation_preview_tests.rs"]
mod tests;
