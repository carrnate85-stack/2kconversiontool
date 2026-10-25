//! WPF library import for the categories `settings_compat` and
//! `legacy_saved_media` do not cover: outfits, preview packs and advanced
//! settings. Reference: core\launcher-storage.js (where WPF libraries live),
//! core\launcher-storage-migration.js `destinationFor` (legacy -> organized
//! paths) and core\launcher-portable-migration.js `commitFile` (conflict rule:
//! missing -> copy, identical -> skip, different -> conflict, never overwrite).
//!
//! The previous library is only read. Each category goes through the Rust
//! launcher's own validated paths (outfit catalog, preview import,
//! save-settings), so nothing unverifiable is written.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::backend::Backend;

const MARKER: &str = "Data/Rust/wpf-migration.json";

#[derive(Clone, Copy, PartialEq)]
enum Layout {
    /// `2k Launcher` folders (portable, AppData, or inside the game folder).
    Organized,
    /// `%APPDATA%\nba2k27-mod-launcher` before the storage migration.
    Legacy,
}
struct Source {
    label: &'static str,
    root: PathBuf,
    layout: Layout,
}
impl Source {
    fn path(&self, organized: &str, legacy: &str) -> PathBuf {
        self.root.join(if self.layout == Layout::Organized {
            organized
        } else {
            legacy
        })
    }
    fn outfits(&self) -> PathBuf {
        self.path(
            "Library/Outfits/outfit-presets.json",
            "blacktop/outfit-presets.json",
        )
    }
    fn previews(&self) -> PathBuf {
        self.path("Library/Preview Packs", "blacktop/preview-packs")
    }
    fn settings(&self) -> PathBuf {
        self.path("Data/settings.json", "settings.json")
    }
    fn scaler(&self) -> PathBuf {
        self.path("Data/scaler/preferences.json", "scaler/preferences.json")
    }
    fn favorites(&self) -> PathBuf {
        self.path(
            "Data/blacktop/arena-favorites.json",
            "blacktop/arena-favorites.json",
        )
    }
}
/// WPF `resolveStorage` order, after the Rust launcher's own folder (a WPF
/// library promoted in place keeps its WPF files beside the Rust data).
fn sources(root: &Path, game_root: &str) -> Vec<Source> {
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
    let mut list = vec![Source {
        label: "This launcher folder",
        root: root.into(),
        layout: Layout::Organized,
    }];
    if let Some(a) = &appdata {
        list.push(Source {
            label: "AppData library (2k Launcher)",
            root: a.join("2k Launcher"),
            layout: Layout::Organized,
        });
    }
    if !game_root.is_empty() {
        list.push(Source {
            label: "Game-folder library (2k Launcher)",
            root: Path::new(game_root).join("2k Launcher"),
            layout: Layout::Organized,
        });
    }
    if let Some(a) = &appdata {
        list.push(Source {
            label: "Original library (nba2k27-mod-launcher)",
            root: a.join("nba2k27-mod-launcher"),
            layout: Layout::Legacy,
        });
    }
    let mut seen = vec![];
    list.retain(|s| {
        let key = s.root.canonicalize().ok();
        let keep = key.as_ref().is_some_and(|k| !seen.contains(k))
            && (s.outfits().is_file()
                || s.previews().is_dir()
                || s.settings().is_file()
                || s.scaler().is_file());
        if let Some(k) = key {
            seen.push(k);
        }
        keep
    });
    list
}
fn read_json(path: &Path, limit: u64) -> Option<Value> {
    let meta = fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || meta.len() > limit {
        return None;
    }
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()
}

// ---------- Outfits ----------

struct Planned {
    name: String,
    file: PathBuf,
    document: Option<Value>,
    status: String,
}
fn file_stem(name: &str) -> String {
    let s: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(40)
        .collect();
    if s.is_empty() { "Outfit".into() } else { s }
}
fn plan_outfits(root: &Path, src: &Source) -> Result<Vec<Planned>> {
    let Some(doc) = read_json(&src.outfits(), 4 * 1024 * 1024) else {
        return Ok(vec![]);
    };
    ensure!(
        doc["schema"] == "nba2k27-outfit-presets-v1",
        "The WPF outfit library has an unsupported format; it was left unchanged."
    );
    let catalog = crate::outfits::catalog()?;
    let mut out = vec![];
    for preset in doc["outfits"]
        .as_array()
        .context("WPF outfit list missing")?
        .iter()
        .take(1000)
    {
        let name = preset["name"]
            .as_str()
            .unwrap_or("Outfit")
            .trim()
            .to_string();
        let id: String = preset["id"]
            .as_str()
            .unwrap_or("")
            .chars()
            .filter(|c| c.is_ascii_hexdigit())
            .take(8)
            .collect();
        let file = root.join("Library/Outfits").join(format!(
            "{}-wpf-{}.2koutfit",
            file_stem(&name),
            if id.is_empty() { "0" } else { &id }
        ));
        // The WPF preset body is the shared-outfit Selection (streetId,
        // bottom, shoe, sock, layer, accessory/accessories).
        let mut selection = preset.clone();
        if let Some(m) = selection.as_object_mut() {
            for k in ["id", "name", "pool", "createdAt", "updatedAt"] {
                m.remove(k);
            }
        }
        let encoded = serde_json::from_value::<crate::outfits::Selection>(selection)
            .map_err(anyhow::Error::from)
            .and_then(|s| catalog.encode(&name, &s));
        let (document, status) = match encoded {
            Err(e) => (None, format!("Not importable: {e:#}")),
            Ok(d) => {
                let status = match fs::read(&file)
                    .ok()
                    .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                {
                    None if file.exists() => {
                        "Conflict: a different file already uses this name".to_string()
                    }
                    None => "Ready".into(),
                    Some(existing) if existing == d => "Already imported".into(),
                    Some(_) => "Conflict: a different file already uses this name".into(),
                };
                (Some(d), status)
            }
        };
        out.push(Planned {
            name,
            file,
            document,
            status,
        });
    }
    Ok(out)
}

// ---------- Previews ----------

fn preview_packs(src: &Source) -> Vec<(PathBuf, u64, std::time::SystemTime)> {
    let mut packs: Vec<_> = fs::read_dir(src.previews())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let m = fs::symlink_metadata(&p).ok()?;
            (m.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("zip")))
                .then(|| (p, m.len(), m.modified().unwrap_or(std::time::UNIX_EPOCH)))
        })
        .collect();
    packs.sort_by_key(|p| std::cmp::Reverse(p.2));
    packs
}
fn preview_sha(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_string_lossy().to_ascii_lowercase();
    let sha = stem.strip_prefix("preview-")?;
    (sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_hexdigit())).then(|| sha.to_string())
}

// ---------- Advanced settings ----------

fn advanced_patch(b: &Backend, src: &Source) -> Result<Vec<(String, &'static str, Value, Value)>> {
    let current = serde_json::to_value(b.current_settings()?)?;
    let mut rows = vec![];
    let mut push = |key: &str, label: &'static str, value: Value| {
        if !value.is_null() && current[key] != value {
            rows.push((key.to_string(), label, current[key].clone(), value));
        }
    };
    if let Some(s) = read_json(&src.settings(), 1024 * 1024).filter(|v| v.is_object()) {
        // Same WPF keys settings_compat preserved without import, now with
        // proven one-to-one Rust meanings.
        for (wpf, key, label) in [
            ("startCameras", "camerasEnabled", "Start cameras with Play"),
            (
                "unlockWnbaFreestyle",
                "unlockWnbaFreestyle",
                "WNBA teams in Freestyle",
            ),
            ("unlockWnbaBlacktop", "unlockWnba", "WNBA teams in Blacktop"),
            ("startupVideoEnabled", "startupVideo", "Startup video"),
        ] {
            if let Some(v) = s[wpf].as_bool() {
                push(key, label, json!(v));
            }
        }
    }
    if let Some(p) = read_json(&src.scaler(), 64 * 1024).filter(|v| v.is_object()) {
        let percent = p["percent"].as_f64().filter(|v| (50.0..=150.0).contains(v));
        if let (Some(enabled), Some(percent)) = (p["enabled"].as_bool(), percent) {
            push(
                "scaler",
                "Player scale",
                json!(if enabled {
                    (percent * 100.).round() / 10000.
                } else {
                    1.0
                }),
            );
        }
        for (wpf, key, label) in [
            ("coaches", "scalerCoaches", "Scale coaches"),
            ("cheerleaders", "scalerCheerleaders", "Scale cheerleaders"),
            (
                "arenaPersonnel",
                "scalerArenaPersonnel",
                "Scale arena personnel",
            ),
        ] {
            if let Some(v) = p[wpf].as_bool() {
                push(key, label, json!(v));
            }
        }
    }
    Ok(rows)
}
fn favorites(src: &Source) -> Vec<String> {
    read_json(&src.favorites(), 64 * 1024)
        .and_then(|v| serde_json::from_value::<Vec<String>>(v).ok())
        .map(|v| v.into_iter().take(1000).collect())
        .unwrap_or_default()
}

/// Rust names its active pack by the ZIP's SHA-256 (previews.rs import), the
/// same digest WPF put in `preview-<sha>.zip`.
fn active_preview_id(root: &Path) -> Option<String> {
    read_json(&root.join("Data/preview-pack.json"), 64 * 1024)?["id"]
        .as_str()
        .map(|s| s.to_ascii_lowercase())
}
fn marker(root: &Path) -> Value {
    read_json(&root.join(MARKER), 1024 * 1024).unwrap_or(json!({"schema":1,"categories":{}}))
}

/// Dry run: what each category would import from every previous library.
pub(crate) fn scan(b: &Backend) -> Result<Value> {
    let settings = b.current_settings()?;
    let active = b.previews.status()?;
    let active_id = active_preview_id(&b.root);
    let current_favorites: Vec<String> =
        serde_json::from_value(b.dispatch("arena-favorites", json!({}))?).unwrap_or_default();
    let mut list = vec![];
    for (i, src) in sources(&b.root, &settings.game_root).iter().enumerate() {
        let outfits = plan_outfits(&b.root, src);
        let packs = preview_packs(src);
        let newest = packs.first();
        let advanced = advanced_patch(b, src)?;
        let new_favorites: Vec<String> = favorites(src)
            .into_iter()
            .filter(|f| !current_favorites.contains(f))
            .collect();
        list.push(json!({
            "index": i,
            "label": src.label,
            "path": src.root,
            "outfits": match &outfits {
                Ok(p) => json!({
                    "total": p.len(),
                    "ready": p.iter().filter(|o| o.status == "Ready").count(),
                    "items": p.iter().map(|o| json!({"name":o.name,"status":o.status})).collect::<Vec<_>>(),
                }),
                Err(e) => json!({"total":0,"ready":0,"error":format!("{e:#}"),"items":[]}),
            },
            "previews": {
                "packs": packs.len(),
                "newest": newest.map(|(p, bytes, _)| json!({
                    "name": p.file_name().map(|n| n.to_string_lossy().to_string()),
                    "bytes": bytes,
                    "alreadyActive": preview_sha(p).is_some_and(|sha| active_id.as_deref() == Some(sha.as_str())),
                })),
                "rustPackActive": !active.is_null(),
            },
            "advanced": advanced.iter().map(|(k, label, cur, wpf)| json!({"key":k,"label":label,"current":cur,"wpf":wpf})).collect::<Vec<_>>(),
            "arenaFavorites": new_favorites,
        }));
    }
    Ok(json!({"sources": list, "imported": marker(&b.root)["categories"]}))
}

/// Import the chosen categories from one scanned source.
pub(crate) fn apply(b: &Backend, args: &Value) -> Result<Value> {
    let index = args["index"]
        .as_u64()
        .context("Choose a previous library")? as usize;
    let categories: Vec<&str> = args["categories"]
        .as_array()
        .context("Choose what to import")?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    ensure!(
        !categories.is_empty()
            && categories
                .iter()
                .all(|c| matches!(*c, "outfits" | "previews" | "advanced")),
        "Choose what to import"
    );
    ensure!(
        !crate::session_safety::processes()?
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe")),
        "Close NBA 2K27 before importing the previous launcher library. The original data remains untouched."
    );
    let settings = b.current_settings()?;
    let all = sources(&b.root, &settings.game_root);
    let src = all
        .get(index)
        .context("That previous library is no longer available.")?;
    ensure!(
        args["path"]
            .as_str()
            .is_some_and(|p| Path::new(p) == src.root),
        "The previous library changed; scan again."
    );
    let mut done = serde_json::Map::new();
    if categories.contains(&"outfits") {
        let plan = plan_outfits(&b.root, src)?;
        fs::create_dir_all(b.root.join("Library/Outfits"))?;
        let (mut imported, mut skipped, mut conflicts, mut invalid) = (0, 0, 0, 0);
        let mut first: Option<Value> = None;
        for o in &plan {
            match (&o.document, o.status.as_str()) {
                (Some(d), "Ready") => {
                    crate::backend::atomic_json_new(&o.file, d)?;
                    imported += 1;
                    first.get_or_insert_with(|| d.clone());
                }
                (Some(d), "Already imported") => {
                    skipped += 1;
                    first.get_or_insert_with(|| d.clone());
                }
                (Some(_), _) => conflicts += 1,
                (None, _) => invalid += 1,
            }
        }
        // The Rust editor keeps one saved outfit; seed it only when empty.
        let mut saved = false;
        if !b.root.join("Data/outfit.json").exists() {
            if let Some(d) = first {
                let outfit =
                    crate::outfits::catalog()?.imported_outfit(&serde_json::to_vec(&d)?)?;
                b.dispatch("save-outfit", serde_json::to_value(outfit)?)?;
                saved = true;
            }
        }
        done.insert("outfits".into(), json!({"imported":imported,"skipped":skipped,"conflicts":conflicts,"notImportable":invalid,"savedOutfitSeeded":saved,"folder":b.root.join("Library/Outfits")}));
    }
    if categories.contains(&"previews") {
        let active = b.previews.status()?;
        let active_id = active_preview_id(&b.root);
        let result = match preview_packs(src).first() {
            None => {
                json!({"imported":false,"message":"No preview pack was found in the previous library."})
            }
            Some((p, _, _))
                if preview_sha(p).is_some_and(|sha| active_id.as_deref() == Some(sha.as_str())) =>
            {
                json!({"imported":false,"message":"This preview pack is already active."})
            }
            Some(_) if !active.is_null() => {
                json!({"imported":false,"message":"A preview pack is already active in this launcher; it was kept."})
            }
            Some((p, _, _)) => {
                let r = b.previews.import(p, |_, _, _| {})?;
                json!({"imported":true,"name":p.file_name().map(|n| n.to_string_lossy().to_string()),"result":r})
            }
        };
        done.insert("previews".into(), result);
    }
    if categories.contains(&"advanced") {
        let rows = advanced_patch(b, src)?;
        let patch: serde_json::Map<String, Value> = rows
            .iter()
            .map(|(k, _, _, v)| (k.clone(), v.clone()))
            .collect();
        if !patch.is_empty() {
            b.dispatch("save-settings", Value::Object(patch))?;
        }
        let current: Vec<String> =
            serde_json::from_value(b.dispatch("arena-favorites", json!({}))?).unwrap_or_default();
        let mut added = 0;
        for id in favorites(src).into_iter().filter(|f| !current.contains(f)) {
            // Unknown ids are refused by the Rust catalog check; skip them.
            if b.dispatch("arena-favorite", json!({"id":id,"favorite":true}))
                .is_ok()
            {
                added += 1;
            }
        }
        done.insert("advanced".into(), json!({"settings":rows.iter().map(|(k, _, _, _)| k).collect::<Vec<_>>(),"arenaFavoritesAdded":added}));
    }
    let mut m = marker(&b.root);
    for (k, v) in &done {
        m["categories"][k] =
            json!({"source":src.root,"at":crate::launcher_health::now_iso(),"result":v});
    }
    m["schema"] = json!(1);
    m["sourcePreserved"] = json!(true);
    fs::create_dir_all(b.root.join("Data/Rust"))?;
    crate::backend::atomic_json(&b.root.join(MARKER), &m)?;
    Ok(
        json!({"source":src.root,"results":done,"message":"Previous launcher data imported. The old library was preserved."}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_and_organized_paths_follow_destination_for() {
        let o = Source {
            label: "",
            root: "C:\\L".into(),
            layout: Layout::Organized,
        };
        let l = Source {
            label: "",
            root: "C:\\L".into(),
            layout: Layout::Legacy,
        };
        assert!(o.outfits().ends_with("Library/Outfits/outfit-presets.json"));
        assert!(l.outfits().ends_with("blacktop/outfit-presets.json"));
        assert!(o.previews().ends_with("Library/Preview Packs"));
        assert!(l.previews().ends_with("blacktop/preview-packs"));
        assert!(
            l.settings().ends_with("settings.json")
                && !l.settings().to_string_lossy().contains("Data")
        );
    }
    #[test]
    fn preview_sha_comes_only_from_exact_wpf_names() {
        let sha = "34e5ab928969668401b8cd186f5614a372cdfbadb898c686bb390677e5742821";
        assert_eq!(
            preview_sha(Path::new(&format!("C:\\p\\preview-{sha}.zip"))).as_deref(),
            Some(sha)
        );
        assert!(preview_sha(Path::new("C:\\p\\preview-abc.zip")).is_none());
        assert!(preview_sha(Path::new("C:\\p\\other.zip")).is_none());
    }
    #[test]
    fn file_names_are_safe() {
        assert_eq!(file_stem("Drip"), "Drip");
        assert_eq!(file_stem("../..\\"), "Outfit");
        assert_eq!(file_stem("Jumpman 1!"), "Jumpman1");
    }
}
