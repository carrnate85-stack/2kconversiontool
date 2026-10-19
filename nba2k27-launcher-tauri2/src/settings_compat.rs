//! Isolated Rust settings storage and conservative legacy compatibility.
//! Root wiring passes backend::validate_settings as a function value; this
//! module never changes its visibility, portable root or native write gates.
#![allow(dead_code)]
#[path = "settings_compat_document.rs"]
mod document;
use crate::backend::Settings;
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::{Map, Value};
use std::{
    fs,
    path::{Component, Path, PathBuf, Prefix},
};

// serde_json::Value normally accepts repeated object keys. Settings are not
// allowed to pick whichever duplicate happened to be last, including nested
// WPF data that will only be reported/preserved rather than imported.
struct Strict(Value);
impl<'de> serde::Deserialize<'de> for Strict {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Strict;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("JSON without duplicate keys")
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> std::result::Result<Strict, E> {
                Ok(Strict(Value::Bool(v)))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> std::result::Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> std::result::Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> std::result::Result<Strict, E> {
                serde_json::Number::from_f64(v)
                    .map(|v| Strict(Value::Number(v)))
                    .ok_or_else(|| E::custom("Nonfinite JSON number"))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> std::result::Result<Strict, E> {
                Ok(Strict(Value::String(v.into())))
            }
            fn visit_string<E: serde::de::Error>(
                self,
                v: String,
            ) -> std::result::Result<Strict, E> {
                Ok(Strict(Value::String(v)))
            }
            fn visit_none<E: serde::de::Error>(self) -> std::result::Result<Strict, E> {
                Ok(Strict(Value::Null))
            }
            fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Strict, E> {
                Ok(Strict(Value::Null))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Strict, A::Error> {
                let mut out = Vec::new();
                while let Some(Strict(v)) = a.next_element()? {
                    out.push(v);
                }
                Ok(Strict(Value::Array(out)))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Strict, A::Error> {
                let mut out = Map::new();
                while let Some(k) = a.next_key::<String>()? {
                    if out.contains_key(&k) {
                        return Err(serde::de::Error::custom("Duplicate JSON object key"));
                    }
                    let Strict(v) = a.next_value()?;
                    out.insert(k, v);
                }
                Ok(Strict(Value::Object(out)))
            }
        }
        d.deserialize_any(Visitor)
    }
}
fn strict_json(bytes: &[u8]) -> Result<Value> {
    Ok(serde_json::from_slice::<Strict>(bytes)?.0)
}
fn rust_signature(map: &Map<String, Value>) -> bool {
    map.contains_key("profile")
        && map.contains_key("autoShowOverlay")
        && map.contains_key("autoHideOverlay")
}

type Validator = fn(&Settings) -> Result<()>;
const MAX: usize = 64 * 1024;
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Preserved {
    pub key: String,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Report {
    pub source: String,
    pub imported: Vec<String>,
    pub preserved_not_imported: Vec<Preserved>,
    pub original_sha256: Option<String>,
    pub original_untouched: bool,
}
fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:X}", Sha256::digest(bytes))
}
fn local_path(value: &str) -> Result<()> {
    ensure!(
        value.len() > 3
            && value.as_bytes()[0].is_ascii_alphabetic()
            && value.as_bytes()[1] == b':'
            && value.as_bytes()[2] == b'\\'
            && value.len() <= 1000
            && !value.chars().any(char::is_control)
            && !value.contains(['/', '*', '?', '"', '<', '>', '|', '%'])
            && !value[2..].contains(':'),
        "Legacy game/mod path is unsupported"
    );
    let path = Path::new(value);
    let mut parts = path.components();
    ensure!(
        matches!(parts.next(),Some(Component::Prefix(p)) if matches!(p.kind(),Prefix::Disk(_)))
            && matches!(parts.next(), Some(Component::RootDir))
            && parts.all(|p| matches!(p, Component::Normal(_)))
            && value.len() > 3,
        "Legacy game/mod path must be a normalized local drive directory"
    );
    // Path::components normalizes embedded '.', so reject it explicitly.
    ensure!(
        value[3..]
            .split('\\')
            .all(|p| !p.is_empty() && p != "." && p != ".."),
        "Legacy game/mod path contains an ambiguous component"
    );
    Ok(())
}
fn boolean(map: &Map<String, Value>, key: &str, required: bool) -> Result<Option<bool>> {
    match map.get(key) {
        Some(v) => {
            Ok(Some(v.as_bool().with_context(|| {
                format!("Legacy WPF {key} is not boolean")
            })?))
        }
        None => {
            ensure!(!required, "Legacy WPF {key} is missing");
            Ok(None)
        }
    }
}
fn classify_legacy(bytes: &[u8], validate: Validator) -> Result<(Settings, Report)> {
    let value: Value = strict_json(bytes)
        .context("Original settings JSON is corrupt; preserved without import")?;
    let map = value
        .as_object()
        .context("Original settings are not an object; preserved without import")?;
    let mut report = Report {
        source: String::new(),
        imported: vec![],
        preserved_not_imported: vec![],
        original_sha256: Some(digest(bytes)),
        original_untouched: true,
    };
    // Actual Rust saves include these three Rust-only discriminators. Never
    // reinterpret an unknown/failed Rust decode as WPF or fresh defaults.
    if rust_signature(map) {
        let settings: Settings = serde_json::from_value(value.clone())
            .context("Older Rust settings are invalid/unknown; original preserved")?;
        validate(&settings).context("Older Rust settings failed validation; original preserved")?;
        report.source = "older-rust".into();
        report.imported = map.keys().cloned().collect();
        return Ok((settings, report));
    }
    ensure!(
        !map.contains_key("autoShowOverlay") && !map.contains_key("autoHideOverlay"),
        "Incomplete/unsupported Rust settings; original preserved"
    );
    // This is the actual WPF/Node defaults signature in core/paths.js, not a
    // loose 'has gameRoot' heuristic. Unknown shapes fail, not silently reset.
    ensure!(
        map.get("provider").and_then(Value::as_str) == Some("local")
            && map.contains_key("startCameras")
            && map.contains_key("autoOpenBlacktop")
            && map.contains_key("modsProfiles"),
        "Original settings have an unsupported engine/schema; preserved without import"
    );
    boolean(map, "startCameras", true)?;
    let show = boolean(map, "autoOpenBlacktop", true)?.unwrap();
    for key in [
        "usePlaylistInFreestyle",
        "unlockWnbaFreestyle",
        "unlockWnbaBlacktop",
        "blacktopCrowdEnabled",
        "startupVideoEnabled",
    ] {
        boolean(map, key, false)?;
    }
    let mut removals = [false; 3];
    if let Some(presentation) = map.get("blacktopPresentation") {
        let presentation = presentation
            .as_object()
            .context("Legacy WPF presentation flags are corrupt")?;
        ensure!(
            presentation
                .keys()
                .all(|k| matches!(k.as_str(), "drone" | "dj" | "mc")),
            "Legacy WPF presentation shape is unsupported"
        );
        for (slot, key) in removals.iter_mut().zip(["drone", "dj", "mc"]) {
            *slot = boolean(presentation, key, false)?.unwrap_or(false);
        }
    }
    // WPF stored "crowd shown"; absent means the original crowd.
    let remove_crowd = !boolean(map, "blacktopCrowdEnabled", false)?.unwrap_or(true);
    let profiles = map["modsProfiles"]
        .as_array()
        .context("Legacy WPF mod profiles are corrupt")?;
    ensure!(
        profiles.len() <= 100,
        "Legacy WPF mod profiles exceed bounds"
    );
    for profile in profiles {
        let p = profile
            .as_object()
            .context("Legacy WPF mod profile is not an object")?;
        ensure!(
            p.keys().all(|k| matches!(k.as_str(), "name" | "path")),
            "Legacy WPF mod profile shape unsupported"
        );
        let name = p
            .get("name")
            .and_then(Value::as_str)
            .context("Legacy WPF mod profile name missing")?;
        ensure!(
            !name.is_empty() && name.len() <= 100 && !name.chars().any(char::is_control),
            "Legacy WPF mod profile name invalid"
        );
        local_path(
            p.get("path")
                .and_then(Value::as_str)
                .context("Legacy WPF mod profile path missing")?,
        )?;
    }
    let mut settings = Settings::default();
    settings.game_root = map
        .get("gameRoot")
        .and_then(Value::as_str)
        .context("Legacy WPF gameRoot is missing/invalid")?
        .into();
    settings.mods_root = map
        .get("modsRoot")
        .and_then(Value::as_str)
        .context("Legacy WPF modsRoot is missing/invalid")?
        .into();
    local_path(&settings.game_root)?;
    local_path(&settings.mods_root)?;
    settings.auto_show_overlay = show;
    // Court-presentation choices are memory-only next-Play options; the Rust
    // runtime still applies them only at verified Blacktop selection.
    [
        settings.remove_drone,
        settings.remove_dj,
        settings.remove_mc,
    ] = removals;
    settings.remove_crowd = remove_crowd;
    // WPF core\paths.js defaults usePlaylistInFreestyle to true; only an
    // explicit false turns it off (renderer `!== false`).
    let freestyle_playlist = boolean(map, "usePlaylistInFreestyle", false)?;
    settings.use_playlist_in_freestyle = freestyle_playlist.unwrap_or(true);
    // No WPF theme setting or independent auto-hide preference is established
    // in the audited defaults/renderer. They stay at Rust defaults. No other
    // native toggles, DLL/helper paths, camera/scaler values or media routes are copied.
    validate(&settings)?;
    report.source = "wpf-compatible-preferences".into();
    report.imported = vec![
        "gameRoot".into(),
        "modsRoot".into(),
        "autoOpenBlacktop -> autoShowOverlay".into(),
    ];
    for key in [
        "blacktopPresentation",
        "blacktopCrowdEnabled",
        "usePlaylistInFreestyle",
    ] {
        if map.contains_key(key) {
            report.imported.push(
                match key {
                    "blacktopPresentation" => {
                        "blacktopPresentation -> removeDrone/removeDj/removeMc"
                    }
                    "blacktopCrowdEnabled" => "blacktopCrowdEnabled -> removeCrowd",
                    _ => "usePlaylistInFreestyle",
                }
                .into(),
            );
        }
    }
    for key in map.keys().filter(|k| {
        !matches!(
            k.as_str(),
            "gameRoot"
                | "modsRoot"
                | "autoOpenBlacktop"
                | "blacktopPresentation"
                | "blacktopCrowdEnabled"
                | "usePlaylistInFreestyle"
        )
    }) {
        report.preserved_not_imported.push(Preserved {
            key: key.clone(),
            reason: if matches!(key.as_str(), "provider" | "startCameras" | "modsProfiles") {
                "Recognized WPF engine metadata; Rust uses its own configuration."
            } else {
                "No proven safe compatible mapping; retained only in the original WPF file."
            }
            .into(),
        });
    }
    Ok((settings, report))
}
fn child(parent: &Path, name: &str) -> Result<PathBuf> {
    let _pins = document::Pins::pin(parent)?;
    let path = parent.join(name);
    match fs::create_dir(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.into()),
    }
    let _pin = document::Pins::pin(&path)?;
    Ok(path)
}
pub(crate) struct Store {
    document: document::Document,
    revision: document::Revision,
    validate: Validator,
    path: PathBuf,
}
impl Store {
    pub fn open(root: &Path, validate: Validator) -> Result<(Self, Settings, Report)> {
        let _root_pin = document::Pins::pin(root)?;
        let data = child(root, "Data")?;
        let _data_pin = document::Pins::pin(&data)?;
        let directory = child(&data, "Rust")?;
        let document = document::Document::open(&directory)?;
        document.recover()?;
        let (current, revision) = document.load()?;
        let mut store = Self {
            document,
            revision,
            validate,
            path: directory.join("settings.json"),
        };
        if let Some(bytes) = current {
            let value =
                strict_json(&bytes).context("Rust settings JSON is malformed; file preserved")?;
            ensure!(
                value.as_object().is_some_and(rust_signature),
                "Rust settings have an incomplete/unknown schema; file preserved"
            );
            let settings: Settings = serde_json::from_value(value)
                .context("Rust settings are invalid/unknown; file preserved")?;
            validate(&settings).context("Rust settings failed validation; file preserved")?;
            return Ok((
                store,
                settings,
                Report {
                    source: "rust-current".into(),
                    imported: vec![],
                    preserved_not_imported: vec![],
                    original_sha256: None,
                    original_untouched: true,
                },
            ));
        }
        let (settings, report) = match document::bounded_read(&data.join("settings.json"), MAX)? {
            Some(bytes) => classify_legacy(&bytes, validate)?,
            None => {
                let settings = Settings::default();
                validate(&settings)?;
                (
                    settings,
                    Report {
                        source: "fresh-defaults".into(),
                        imported: vec![],
                        preserved_not_imported: vec![],
                        original_sha256: None,
                        original_untouched: true,
                    },
                )
            }
        };
        store.save(&settings)?;
        Ok((store, settings, report))
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Read-only launch admission. Never recover a pending save, silently
    /// refresh a stale revision, or adopt a same-byte replacement. Call before
    /// freezing in-memory settings for a new launch under the settings mutex.
    pub fn verify_current(&self) -> Result<()> {
        let (bytes, revision) = self.document.load()?;
        ensure!(
            revision == self.revision,
            "Rust settings changed or disappeared; reload before launching"
        );
        let bytes = bytes.context("Rust settings are missing; launch held")?;
        let value = strict_json(&bytes).context("Rust settings JSON is malformed; launch held")?;
        ensure!(
            value.as_object().is_some_and(rust_signature),
            "Rust settings schema is unsupported; launch held"
        );
        let settings: Settings = serde_json::from_value(value)
            .context("Rust settings are invalid/unknown; launch held")?;
        (self.validate)(&settings).context("Rust settings failed validation; launch held")?;
        Ok(())
    }
    pub fn save(&mut self, settings: &Settings) -> Result<()> {
        (self.validate)(settings)?;
        let bytes = serde_json::to_vec_pretty(settings)?;
        ensure!(bytes.len() <= MAX, "Rust settings exceed bounds");
        let nonce = crate::backend::unique_id();
        self.revision = self.document.commit(&self.revision, &bytes, &nonce)?;
        Ok(())
    }
}

#[cfg(any(test, settings_compat_offline))]
#[path = "settings_compat_tests.rs"]
pub(crate) mod tests;
#[cfg(test)]
#[test]
fn integrated_settings_compatibility_and_fault_regressions() {
    tests::run().unwrap();
}
