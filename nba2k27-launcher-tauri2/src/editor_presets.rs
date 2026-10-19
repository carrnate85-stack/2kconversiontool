//! Portable original-editor presets. No memory access or caller-supplied paths.
use crate::{backend, player_dna, team_dna};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    fs,
    io::Read,
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use windows::Win32::{
    Security::Cryptography::{BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom},
    Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT,
};
const LIMIT: usize = 32 * 1024;
fn kind(v: &Value) -> Result<&str> {
    let s = v.as_str().context("Preset category missing")?;
    ensure!(
        ["signature", "equipment", "coaching"].contains(&s),
        "Invalid preset category"
    );
    Ok(s)
}
pub(crate) fn validate(v: &Value) -> Result<Value> {
    ensure!(
        v["format"] == "2k-editor-preset" && v["version"].as_f64() == Some(1.),
        "Invalid editor preset format"
    );
    let category = kind(&v["kind"])?;
    let name = v["name"].as_str().context("Preset name needs text")?;
    let trimmed = name.trim_matches(crate::player_changes::js_space);
    ensure!(
        !trimmed.is_empty()
            && name.encode_utf16().count() <= 60
            && !name.chars().any(|c| c <= '\u{1f}' || c == '\u{7f}'),
        "Invalid editor preset name"
    );
    let values = v["values"].as_object().context("Preset values missing")?;
    ensure!(!values.is_empty(), "Choose preset values");
    if category == "coaching" {
        team_dna::staff_values(&v["values"])?;
        for key in values.keys() {
            let f = team_dna::schema()["staff"]
                .as_array()
                .unwrap()
                .iter()
                .find(|f| f["key"] == *key)
                .context("Unknown coaching value")?;
            ensure!(
                f["text"] != true
                    && f["group"] != "Identity"
                    && f["group"] != "Contract"
                    && !["faceId", "soundId", "wage"].contains(&key.as_str()),
                "Staff identity and contracts cannot be included in coaching presets"
            );
        }
    } else {
        ensure!(
            values.len() <= player_dna::schema()["fields"].as_array().unwrap().len(),
            "Too many preset values"
        );
        for (key, value) in values {
            let field = player_dna::schema()["fields"]
                .as_array()
                .unwrap()
                .iter()
                .find(|f| f["key"] == *key && f["group"] == category)
                .context("Unsupported preset field")?;
            let max = field["maximum"].as_f64().unwrap();
            let n = value.as_f64().context("Preset values require numbers")?;
            ensure!(
                max >= 0. && n.is_finite() && n.fract() == 0. && n >= 0. && n <= max,
                "Preset value outside supported range"
            );
        }
    }
    let clean = json!({"format":"2k-editor-preset","version":1,"kind":category,"name":trimmed,"values":values});
    ensure!(
        serde_json::to_vec_pretty(&clean)?.len() <= LIMIT,
        "Preset exceeds 32 KB"
    );
    Ok(clean)
}
fn valid_id(id: &str) -> bool {
    id.len() == 36
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b) || b == b'-')
}
fn uuid() -> Result<String> {
    let mut b = [0u8; 16];
    unsafe {
        BCryptGenRandom(None, &mut b, BCRYPT_USE_SYSTEM_PREFERRED_RNG).ok()?;
    }
    b[6] = (b[6] & 15) | 0x40;
    b[8] = (b[8] & 63) | 0x80;
    let h = b.iter().map(|b| format!("{b:02x}")).collect::<String>();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    ))
}
pub(crate) struct Service {
    directory: PathBuf,
    operation: Mutex<()>,
    stop: AtomicBool,
}
impl Service {
    pub fn new(root: &Path) -> Self {
        Self {
            directory: root.join("Library/editor-presets"),
            operation: Mutex::new(()),
            stop: AtomicBool::new(false),
        }
    }
    fn check_root(&self) -> Result<()> {
        self.check_open()?;
        for dir in self.directory.ancestors() {
            match fs::symlink_metadata(dir) {
                Ok(m) => ensure!(
                    m.is_dir() && m.file_attributes() & 0x400 == 0,
                    "Preset storage must use regular, unlinked directories"
                ),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
    fn ready(&self) -> Result<()> {
        self.check_root()?;
        fs::create_dir_all(&self.directory)?;
        self.check_root()
    }
    fn read(&self, id: &str) -> Result<Value> {
        ensure!(valid_id(id), "Invalid preset ID");
        self.ready()?;
        let path = self.directory.join(format!("{id}.json"));
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(path)?;
        let before = file.metadata()?;
        ensure!(
            before.is_file()
                && before.file_attributes() & 0x400 == 0
                && before.len() <= LIMIT as u64,
            "Invalid preset file"
        );
        let mut bytes = Vec::new();
        (&file).take(LIMIT as u64 + 1).read_to_end(&mut bytes)?;
        let after = file.metadata()?;
        ensure!(
            bytes.len() <= LIMIT
                && before.len() == after.len()
                && before.modified()? == after.modified()?,
            "Preset changed while reading"
        );
        validate(&serde_json::from_slice(&bytes)?)
    }
    pub fn request(&self, r: &Value) -> Result<Value> {
        self.check_open()?;
        let category = kind(&r["kind"])?;
        let _operation = self
            .operation
            .lock()
            .map_err(|_| anyhow::anyhow!("Preset service unavailable"))?;
        self.check_open()?;
        match r["action"].as_str() {
            Some("read") => {
                let v = self.read(r["id"].as_str().context("Preset ID missing")?)?;
                ensure!(v["kind"] == category, "Preset category does not match");
                Ok(v)
            }
            Some("list") => {
                self.ready()?;
                let mut rows = Vec::new();
                let started = Instant::now();
                for (index, entry) in fs::read_dir(&self.directory)?.enumerate() {
                    self.check_open()?;
                    ensure!(
                        started.elapsed() < Duration::from_secs(3),
                        "Preset listing exceeded its time budget"
                    );
                    ensure!(
                        index < 4096,
                        "Preset directory exceeds the safe listing limit"
                    );
                    let entry = entry?;
                    let name = entry.file_name();
                    let Some(name) = name.to_str() else { continue };
                    let Some(id) = name.strip_suffix(".json").filter(|s| valid_id(s)) else {
                        continue;
                    };
                    if let Ok(v) = self.read(id) {
                        if v["kind"] == category {
                            rows.push(json!({"id":id,"name":v["name"]}));
                        }
                    }
                }
                // The browser adapter performs the exact original localeCompare
                // sort in the user's JS locale rather than approximating ICU.
                rows.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
                Ok(json!(rows))
            }
            Some("save") => {
                let mut input = r.clone();
                input["format"] = json!("2k-editor-preset");
                input["version"] = json!(1);
                let v = validate(&input)?;
                self.ready()?;
                let id = uuid()?;
                self.check_open()?;
                backend::atomic_json_new(&self.directory.join(format!("{id}.json")), &v)?;
                Ok(json!({"id":id,"name":v["name"]}))
            }
            _ => anyhow::bail!("Invalid preset request"),
        }
    }
    fn check_open(&self) -> Result<()> {
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Preset service is closing"
        );
        Ok(())
    }
    pub fn cancel(&self) {
        self.stop.store(true, Ordering::Release);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn value(kind: &str) -> Value {
        let key = if kind == "coaching" {
            "preferredSystemId"
        } else {
            player_dna::schema()["fields"]
                .as_array()
                .unwrap()
                .iter()
                .find(|f| f["group"] == kind && f["maximum"].as_i64().unwrap() >= 0)
                .unwrap()["key"]
                .as_str()
                .unwrap()
        };
        json!({"format":"2k-editor-preset","version":1,"kind":kind,"name":"  QA preset  ","values":{key:0}})
    }
    #[test]
    fn original_preset_validation_oracle_and_identity_exclusion() {
        let oracle: Value = serde_json::from_slice(include_bytes!(
            "../assets/browser-transfer-regressions.json"
        ))
        .unwrap();
        for (index, case) in oracle["presets"].as_array().unwrap().iter().enumerate() {
            let actual = validate(&case["input"]);
            if case["expected"].is_null() {
                assert!(actual.is_err(), "Unexpected acceptance {index}");
            } else {
                assert_eq!(
                    actual.unwrap_or_else(|e| panic!("Preset {index}: {e:#}")),
                    case["expected"],
                    "Preset {index}"
                );
            }
        }
        let mut v = value("coaching");
        v["values"] = json!({"faceId":1});
        assert!(validate(&v).is_err());
        v["values"] = json!({"wage":1});
        assert!(validate(&v).is_err());
    }
    #[test]
    fn portable_store_roundtrip_category_and_no_clobber_preserve_bad_files() {
        let root =
            std::env::temp_dir().join(format!("rust-editor-presets-{}", backend::unique_id()));
        fs::create_dir(&root).unwrap();
        let service = Service::new(&root);
        for category in ["signature", "equipment", "coaching"] {
            let mut request = value(category);
            request["action"] = json!("save");
            request["recordAddress"] = json!("0xDEADBEEF");
            request["path"] = json!("ignored");
            let saved = service.request(&request).unwrap();
            let id = saved["id"].as_str().unwrap();
            assert!(valid_id(id));
            let read = service
                .request(&json!({"kind":category,"action":"read","id":id}))
                .unwrap();
            assert_eq!(read, validate(&request).unwrap());
            assert!(read.get("recordAddress").is_none());
            let target = service.directory.join(format!("{id}.json"));
            let old = fs::read(&target).unwrap();
            assert!(backend::atomic_json_new(&target, &json!({"replacement":true})).is_err());
            assert_eq!(fs::read(target).unwrap(), old);
            assert!(service.request(&json!({"kind":if category=="equipment"{"signature"}else{"equipment"},"action":"read","id":id})).is_err());
            assert_eq!(
                service
                    .request(&json!({"kind":category,"action":"list"}))
                    .unwrap()
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
        }
        let damaged = service
            .directory
            .join("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa.json");
        fs::write(&damaged, b"{broken").unwrap();
        assert_eq!(
            service
                .request(&json!({"kind":"signature","action":"list"}))
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(service.request(&json!({"kind":"signature","action":"read","id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"})).is_err());
        assert_eq!(fs::read(&damaged).unwrap(), b"{broken");
        assert!(
            service
                .request(&json!({"kind":"signature","action":"read","id":"../outside"}))
                .is_err()
        );
        service.cancel();
        assert!(
            service
                .request(&json!({"kind":"signature","action":"list"}))
                .is_err()
        );
        for entry in fs::read_dir(&service.directory).unwrap() {
            let p = entry.unwrap().path();
            assert!(p.is_file());
            fs::remove_file(p).unwrap();
        }
        fs::remove_dir(&service.directory).unwrap();
        fs::remove_dir(root.join("Library")).unwrap();
        fs::remove_dir(root).unwrap();
    }
}
