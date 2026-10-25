use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

#[derive(PartialEq, Eq, serde::Serialize)]
struct FileStamp {
    bytes: u64,
    modified: SystemTime,
}

impl FileStamp {
    fn read(path: &Path) -> Option<Self> {
        let metadata = fs::metadata(path).ok()?;
        metadata.is_file().then_some(Self {
            bytes: metadata.len(),
            modified: metadata.modified().ok()?,
        })
    }
}

pub struct CachedCatalog {
    value: Value,
    watched: Vec<(PathBuf, Option<FileStamp>)>,
}

impl CachedCatalog {
    pub fn new(value: Value, settings: PathBuf, read_started: SystemTime) -> Option<Self> {
        let label = value["gameLabel"].as_str().unwrap_or_default().to_owned();
        Self::for_game(value, settings, read_started, &label)
    }

    pub fn accessories(value: Value, settings: PathBuf, read_started: SystemTime) -> Option<Self> {
        if !matches!(value["assetType"].as_str(), Some("hair" | "facialhair"))
            || !value["assets"].is_array()
            || serde_json::to_vec(&value).ok()?.len() > 8 * 1024 * 1024
        {
            return None;
        }
        let mut cache = Self::for_game(value, settings, read_started, "NBA 2K27")?;
        let snapshot = serde_json::to_vec(&cache.watched).ok()?;
        cache.value["sourceRevision"] = Value::String(format!("{:x}", Sha256::digest(snapshot)));
        Some(cache)
    }

    fn for_game(
        value: Value,
        settings: PathBuf,
        read_started: SystemTime,
        label: &str,
    ) -> Option<Self> {
        let root = PathBuf::from(value["gameRoot"].as_str()?);
        let manifest = root.join("manifest");
        let mut watched = vec![
            (manifest.clone(), Some(FileStamp::read(&manifest)?)),
            (settings.clone(), Some(FileStamp::read(&settings)?)),
        ];
        let required = if matches!(label, "NBA 2K25" | "NBA 2K26" | "NBA 2K27") {
            "mod.exe"
        } else {
            "0A"
        };
        let stored: Value = serde_json::from_slice(&fs::read(&settings).ok()?)
            .ok()
            .unwrap_or(Value::Null);
        let field = match label {
            "NBA 2K27" => "game_root_2k27",
            "NBA 2K26" => "game_root",
            _ => "",
        };
        let preferred = stored[field]
            .as_str()
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os(label.replace(' ', "") + "_ROOT").map(PathBuf::from));
        let roots = std::iter::once(root).chain(preferred);
        let mut required_files = vec!["manifest", required];
        if label == "NBA 2K27" {
            required_files.extend(["NBA2K27.exe", "data/oodle/oo2core_9_win64.dll"]);
        }
        for root in roots {
            for name in &required_files {
                let path = root.join(name);
                if !watched.iter().any(|(existing, _)| existing == &path) {
                    let stamp = FileStamp::read(&path);
                    watched.push((path, stamp));
                }
            }
        }
        // Do not label an older worker result with files edited during its read.
        if watched.iter().any(|(_, stamp)| {
            stamp
                .as_ref()
                .is_some_and(|stamp| stamp.modified > read_started)
        }) {
            return None;
        }
        Some(Self { value, watched })
    }

    pub fn current(&self) -> Option<&Value> {
        self.watched
            .iter()
            .all(|(path, stamp)| FileStamp::read(path) == *stamp)
            .then_some(&self.value)
    }

    pub fn value(&self) -> &Value {
        &self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{
        fs::FileTimes,
        time::{Duration, UNIX_EPOCH},
    };

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "cmt-catalog-test-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir(&path).unwrap();
            let fixture = Self(path);
            fixture.write("manifest", b"first", 100);
            fixture.write("settings.json", b"first", 100);
            fixture
        }
        fn write(&self, name: &str, bytes: &[u8], seconds: u64) {
            let path = self.0.join(name);
            fs::write(&path, bytes).unwrap();
            fs::OpenOptions::new()
                .write(true)
                .open(path)
                .unwrap()
                .set_times(FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(seconds)))
                .unwrap();
        }
        fn cache(&self) -> CachedCatalog {
            CachedCatalog::new(
                json!({"gameRoot":self.0,"rows":[{"png":"29990"}]}),
                self.0.join("settings.json"),
                SystemTime::now(),
            )
            .unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_file(self.0.join("manifest"));
            let _ = fs::remove_file(self.0.join("settings.json"));
            let _ = fs::remove_dir(&self.0);
        }
    }

    #[test]
    fn unchanged_files_keep_catalog_cached() {
        let fixture = Fixture::new();
        let cache = fixture.cache();
        assert_eq!(cache.current().unwrap()["rows"][0]["png"], "29990");
        assert!(cache.current().is_some());
    }

    #[test]
    fn size_and_same_size_timestamp_changes_both_invalidate() {
        for (bytes, modified) in [(b"longer".as_slice(), 100), (b"other".as_slice(), 101)] {
            let fixture = Fixture::new();
            let cache = fixture.cache();
            fixture.write("manifest", bytes, modified);
            assert!(cache.current().is_none());
        }
    }

    #[test]
    fn external_settings_change_and_missing_manifest_invalidate() {
        let fixture = Fixture::new();
        let cache = fixture.cache();
        fixture.write("settings.json", b"other", 101);
        assert!(cache.current().is_none());
        let cache = fixture.cache();
        fs::remove_file(fixture.0.join("manifest")).unwrap();
        assert!(cache.current().is_none());
    }

    #[test]
    fn missing_inputs_and_changes_during_read_are_not_cached() {
        let fixture = Fixture::new();
        let value = json!({"gameRoot":fixture.0,"rows":[]});
        assert!(
            CachedCatalog::new(
                value.clone(),
                fixture.0.join("settings.json"),
                UNIX_EPOCH + Duration::from_secs(99)
            )
            .is_none()
        );
        assert!(CachedCatalog::new(value, fixture.0.join("missing"), SystemTime::now()).is_none());
        assert!(
            CachedCatalog::new(
                Value::Null,
                fixture.0.join("settings.json"),
                SystemTime::now()
            )
            .is_none()
        );
    }

    #[test]
    fn preferred_manifest_reappearance_and_extractor_change_invalidate() {
        let fixture = Fixture::new();
        let preferred = fixture.0.join("preferred");
        fixture.write(
            "settings.json",
            &serde_json::to_vec(&json!({"game_root_2k27":preferred})).unwrap(),
            100,
        );
        let value = json!({"gameLabel":"NBA 2K27","gameRoot":fixture.0,"rows":[]});
        let cache = CachedCatalog::new(
            value.clone(),
            fixture.0.join("settings.json"),
            SystemTime::now(),
        )
        .unwrap();
        fs::create_dir(&preferred).unwrap();
        fs::write(preferred.join("manifest"), b"restored").unwrap();
        assert!(cache.current().is_none());
        let cache =
            CachedCatalog::new(value, fixture.0.join("settings.json"), SystemTime::now()).unwrap();
        fs::write(fixture.0.join("mod.exe"), b"catalog-only fixture").unwrap();
        assert!(cache.current().is_none());
        fs::remove_file(fixture.0.join("mod.exe")).unwrap();
        fs::remove_file(preferred.join("manifest")).unwrap();
        fs::remove_dir(preferred).unwrap();
    }

    #[test]
    fn accessory_lists_preserve_descriptors_and_watch_their_manifest() {
        let fixture = Fixture::new();
        for kind in ["hair", "facialhair"] {
            let value = json!({"gameRoot":fixture.0,"assetType":kind,"assets":[{"id":"example"}],
                "previewDescriptors":[{"geoEntry":{"offset":42}}]});
            let cache = CachedCatalog::accessories(
                value.clone(),
                fixture.0.join("settings.json"),
                SystemTime::now(),
            )
            .unwrap();
            assert_eq!(cache.current().unwrap()["assets"], value["assets"]);
            assert_eq!(
                cache.current().unwrap()["previewDescriptors"],
                value["previewDescriptors"]
            );
            assert_eq!(
                cache.current().unwrap()["sourceRevision"]
                    .as_str()
                    .unwrap()
                    .len(),
                64
            );
            fixture.write("manifest", b"changed", 101);
            assert!(cache.current().is_none());
            fixture.write("manifest", b"first", 100);
        }
    }

    #[test]
    fn accessory_revision_tracks_source_not_category_or_rows() {
        let fixture = Fixture::new();
        let revision = |kind, rows| {
            CachedCatalog::accessories(
                json!({"gameRoot":fixture.0,"assetType":kind,"assets":rows}),
                fixture.0.join("settings.json"),
                SystemTime::now(),
            )
            .unwrap()
            .value()["sourceRevision"]
                .clone()
        };
        let first = revision("hair", json!([]));
        assert_eq!(first, revision("facialhair", json!([{"id":"different"}])));
        fixture.write("manifest", b"other", 101);
        let manifest = revision("hair", json!([]));
        assert_ne!(first, manifest);
        fixture.write("settings.json", b"other", 101);
        let settings = revision("hair", json!([]));
        assert_ne!(manifest, settings);
        fixture.write("mod.exe", b"fixture", 100);
        assert_ne!(settings, revision("hair", json!([])));
        fs::remove_file(fixture.0.join("mod.exe")).unwrap();
    }

    #[test]
    fn native_prerequisite_presence_versions_and_removal_invalidate_accessory_lists() {
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.0.join("data/oodle")).unwrap();
        let cache = || {
            CachedCatalog::accessories(
                json!({"gameRoot":fixture.0,"assetType":"hair","assets":[]}),
                fixture.0.join("settings.json"),
                SystemTime::now(),
            )
            .unwrap()
        };
        for name in ["NBA2K27.exe", "data/oodle/oo2core_9_win64.dll"] {
            let before = cache();
            fixture.write(name, b"first", 100);
            assert!(before.current().is_none());
            let present = cache();
            assert_ne!(
                present.value()["sourceRevision"],
                before.value()["sourceRevision"]
            );
            fixture.write(name, b"other", 101);
            assert!(present.current().is_none());
            let changed = cache();
            assert_ne!(
                present.value()["sourceRevision"],
                changed.value()["sourceRevision"]
            );
            fs::remove_file(fixture.0.join(name)).unwrap();
            assert!(changed.current().is_none());
        }
        fs::remove_dir(fixture.0.join("data/oodle")).unwrap();
        fs::remove_dir(fixture.0.join("data")).unwrap();
    }

    #[test]
    fn invalid_and_oversized_accessory_results_are_not_cached() {
        let fixture = Fixture::new();
        for value in [
            json!({"gameRoot":fixture.0,"assetType":"invalid","assets":[]}),
            json!({"gameRoot":fixture.0,"assetType":"hair","assets":null}),
            json!({"gameRoot":fixture.0,"assetType":"hair","assets":[],
                "previewDescriptors":["x".repeat(8 * 1024 * 1024)]}),
        ] {
            assert!(
                CachedCatalog::accessories(
                    value,
                    fixture.0.join("settings.json"),
                    SystemTime::now()
                )
                .is_none()
            );
        }
    }
}
