//! Dev catalog entries are logical file choices, never native addresses.
use crate::{backend, blacktop_arena::Choice};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    os::windows::fs::MetadataExt,
    path::{Component, Path},
    sync::{Mutex, OnceLock, RwLock},
};

pub(crate) const fn enabled() -> bool {
    cfg!(feature = "dev-model-preview")
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Entry {
    pub id: String,
    pub name: String,
    pub mods_root: String,
    pub base_id: String,
    pub arena_source: Option<String>,
    pub floor_source: Option<String>,
    #[serde(default)]
    pub routing_version: u8,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Store {
    entries: Vec<Entry>,
    // Old identities remain valid for an active session's immutable receipt.
    #[serde(default)]
    retired: Vec<Entry>,
}
#[derive(Clone)]
struct Registered {
    entry: Entry,
    ready: bool,
}
fn registry() -> &'static RwLock<BTreeMap<String, Registered>> {
    static REGISTRY: OnceLock<RwLock<BTreeMap<String, Registered>>> = OnceLock::new();
    REGISTRY.get_or_init(Default::default)
}
fn relative(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 512
            && !value.contains(['\\', ':'])
            && value.to_ascii_lowercase().ends_with(".iff")
            && Path::new(value)
                .components()
                .all(|c| matches!(c, Component::Normal(_))),
        "Choose an IFF inside the mods folder"
    );
    ensure!(
        value
            .split('/')
            .all(|p| !p.is_empty() && !p.ends_with(['.', ' '])),
        "Invalid mod path"
    );
    Ok(())
}
pub(crate) fn source(root: &Path, value: &str) -> Result<std::path::PathBuf> {
    relative(value)?;
    ensure!(root.is_absolute(), "Select a mods folder first");
    let mut path = root.to_path_buf();
    let meta = fs::symlink_metadata(&path)?;
    ensure!(
        meta.is_dir() && meta.file_attributes() & 0x400 == 0,
        "Mods folder must be an ordinary directory"
    );
    for part in Path::new(value).components() {
        path.push(part);
        ensure!(
            fs::symlink_metadata(&path)?.file_attributes() & 0x400 == 0,
            "Linked mod paths are not supported"
        );
    }
    let meta = fs::metadata(&path)?;
    ensure!(
        meta.is_file() && meta.len() > 0 && meta.len() <= 4 * 1024 * 1024 * 1024,
        "Mod file is empty, missing or too large"
    );
    Ok(path)
}
impl Entry {
    fn validate(&self) -> Result<()> {
        ensure!(self.routing_version <= 1, "Unknown custom arena routing version");
        ensure!(
            enabled(),
            "Custom Blacktop catalog is available in the dev build"
        );
        ensure!(
            self.id.starts_with("custom-")
                && self.id.len() == 55
                && self.id[7..].bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid custom arena ID"
        );
        ensure!(
            !self.name.trim().is_empty()
                && self.name.len() <= 120
                && !self.name.chars().any(char::is_control),
            "Enter an arena name (up to 120 characters)"
        );
        ensure!(
            Path::new(&self.mods_root).is_absolute() && self.mods_root.len() <= 1024,
            "Invalid mods folder"
        );
        let base = self.base()?;
        ensure!(
            !base.disabled && base.file.is_some() && base.floor_file.is_some(),
            "Choose a supported base arena"
        );
        ensure!(
            self.arena_source.is_some() || self.floor_source.is_some(),
            "Choose an arena or court mod"
        );
        for file in [&self.arena_source, &self.floor_source]
            .into_iter()
            .flatten()
        {
            relative(file)?;
        }
        Ok(())
    }
    fn base(&self) -> Result<&'static Choice> {
        if self.base_id == "blacktop-stock" {
            static BASE: OnceLock<Choice> = OnceLock::new();
            return Ok(BASE.get_or_init(|| Choice {
                id: "blacktop-stock".into(),
                label: "Stock Blacktop court".into(),
                arena_id: Some(700),
                file: Some("arena_700_int".into()),
                floor_file: Some("arena_700_int_floor".into()),
                template_arena_id: Some(700),
                disabled: false,
            }));
        }
        crate::blacktop_arena::choices()
            .iter()
            .find(|c| c.id == self.base_id)
            .context("Unknown base arena")
    }
    pub(crate) fn choice(&self) -> Result<Choice> {
        self.validate()?;
        let base = self.base()?;
        // Keep legacy identities exact for recovery. New identities use the
        // whole ID, not its mostly-zero timestamp prefix, and short names.
        let alias = if self.routing_version == 0 {
            format!("arena_c{}", &self.id[7..31])
        } else {
            use sha2::{Digest, Sha256};
            format!("arena_c{}_int", &format!("{:x}", Sha256::digest(self.id.as_bytes()))[..12])
        };
        let choice = Choice {
            id: self.id.clone(),
            label: self.name.clone(),
            arena_id: Some(700),
            template_arena_id: Some(700),
            disabled: false,
            file: Some(if self.arena_source.is_some() {
                alias.clone()
            } else {
                base.file.clone().unwrap()
            }),
            floor_file: Some(if self.floor_source.is_some() {
                format!("{alias}_floor")
            } else {
                base.floor_file.clone().unwrap()
            }),
        };
        crate::blacktop_arena::filename_fields(
            choice.file.as_deref().unwrap(),
            choice.floor_file.as_deref().unwrap(),
        )?;
        Ok(choice)
    }
    pub(crate) fn aliases(&self) -> Result<Vec<(String, String)>> {
        let c = self.choice()?;
        Ok([
            (&self.arena_source, c.file),
            (&self.floor_source, c.floor_file),
        ]
        .into_iter()
        .filter_map(|(s, target)| {
            s.as_ref()
                .map(|s| (format!("levels/{}.iff", target.unwrap()), s.clone()))
        })
        .collect())
    }
    fn available(&self, mods: &Path) -> Result<()> {
        ensure!(
            Path::new(&self.mods_root) == mods,
            "This arena belongs to another mods folder"
        );
        for file in [&self.arena_source, &self.floor_source]
            .into_iter()
            .flatten()
        {
            source(mods, file)?;
        }
        Ok(())
    }
}
fn read(root: &Path) -> Result<Store> {
    let path = root.join("Data/custom-blacktop-catalog.json");
    if !path.try_exists()? {
        return Ok(Store::default());
    }
    let m = fs::symlink_metadata(&path)?;
    ensure!(
        m.is_file() && m.file_attributes() & 0x400 == 0 && m.len() <= 512 * 1024,
        "Invalid custom catalog file; left untouched"
    );
    let s: Store = serde_json::from_slice(&fs::read(path)?)?;
    ensure!(s.entries.len() <= 200, "Custom catalog is full");
    let mut ids = std::collections::BTreeSet::new();
    ensure!(s.retired.len() <= 1000, "Custom catalog edit history is full");
    for e in s.entries.iter().chain(&s.retired) {
        e.validate()?;
        ensure!(ids.insert(&e.id), "Duplicate custom arena ID");
    }
    Ok(s)
}
pub(crate) fn load(root: &Path) -> Result<Vec<Entry>> {
    if !enabled() {
        return Ok(Vec::new());
    }
    let store = read(root)?;
    let mut registered = registry()
        .write()
        .map_err(|_| anyhow::anyhow!("Custom catalog unavailable"))?;
    for entry in store.entries.iter().chain(&store.retired) {
        if let Some(old) = registered.get(&entry.id) {
            ensure!(
                serde_json::to_value(&old.entry)? == serde_json::to_value(entry)?,
                "Custom arena changed during this session; restart the launcher"
            );
        } else {
            registered.insert(
                entry.id.clone(),
                Registered {
                    entry: entry.clone(),
                    ready: false,
                },
            );
        }
    }
    Ok(store.entries)
}
pub(crate) fn resolve(id: &str) -> Result<Choice> {
    ensure!(enabled(), "Custom Blacktop catalog is dev only");
    registry()
        .read()
        .map_err(|_| anyhow::anyhow!("Custom catalog unavailable"))?
        .get(id)
        .context("Custom arena is not registered")?
        .entry
        .choice()
}
pub(crate) fn known_fields(bytes: &[u8]) -> bool {
    enabled()
        && registry().read().is_ok_and(|r| {
            r.values().any(|r| {
                r.entry.choice().is_ok_and(|c| {
                    crate::blacktop_arena::filename_fields(
                        c.file.as_deref().unwrap(),
                        c.floor_file.as_deref().unwrap(),
                    )
                    .is_ok_and(|b| b == bytes)
                })
            })
        })
}
pub(crate) fn verify_assets(c: &Choice) -> Result<()> {
    let r = registry()
        .read()
        .map_err(|_| anyhow::anyhow!("Custom catalog unavailable"))?;
    let r = r.get(&c.id).context("Custom arena is not registered")?;
    ensure!(
        r.ready && r.entry.choice()? == *c,
        "Custom arena files are not ready in this modded session"
    );
    r.entry.available(Path::new(&r.entry.mods_root))
}
pub(crate) fn publish(
    root: &Path,
    mods: &Path,
    assets: &crate::provider_assets::Service,
    only: Option<&str>,
) -> Result<()> {
    for e in load(root)?
        .into_iter()
        .filter(|e| only.map_or(Path::new(&e.mods_root) == mods, |id| e.id == id))
    {
        // Missing unused mods never hold up a launcher. Selecting one reports it.
        let outcome = (|| {
            e.available(mods)?;
            assets.publish_custom_arena_aliases(&e.aliases()?)
        })();
        if let Err(error) = outcome {
            if only.is_some() {
                return Err(error);
            } else {
                continue;
            }
        }
        registry()
            .write()
            .map_err(|_| anyhow::anyhow!("Custom catalog unavailable"))?
            .get_mut(&e.id)
            .unwrap()
            .ready = true;
    }
    Ok(())
}
pub(crate) fn rows(root: &Path, mods: &Path) -> Result<Vec<Value>> {
    load(root)?.into_iter().map(|e| {
        let issue = e.available(mods).err().map(|e| format!("{e:#}"));
        Ok(json!({"id":e.id,"name":e.name,"group":"Custom","file":e.arena_source,"floorFile":e.floor_source,"disabled":issue.is_some(),"note":issue}))
    }).collect()
}
pub(crate) fn request(root: &Path, mods: &Path, input: &Value) -> Result<Value> {
    ensure!(enabled(), "Custom Blacktop catalog is dev only");
    static LOCK: Mutex<()> = Mutex::new(());
    let _lock = LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("Custom catalog unavailable"))?;
    match input["action"].as_str() {
        Some("sources") => {
            let levels = mods.join("levels");
            let mut pending = if levels.try_exists()? { vec![(levels.clone(), 0)] } else { Vec::new() };
            let mut files = Vec::new();
            let mut visited = 0;
            while let Some((dir, depth)) = pending.pop() {
                ensure!(
                    depth <= 32 && visited <= 100_000,
                    "Mods folder exceeds browsing limits"
                );
                let metadata = fs::symlink_metadata(&dir)?;
                if metadata.file_attributes() & 0x400 != 0 {
                    continue;
                }
                for e in fs::read_dir(&dir)? {
                    let e = e?;
                    visited += 1;
                    ensure!(visited <= 100_000, "Mods folder exceeds browsing limits");
                    let meta = fs::symlink_metadata(e.path())?;
                    if meta.file_attributes() & 0x400 != 0 {
                        continue;
                    }
                    if meta.is_dir() {
                        pending.push((e.path(), depth + 1));
                    } else if meta.is_file()
                        && e.path()
                            .extension()
                            .is_some_and(|s| s.eq_ignore_ascii_case("iff"))
                    {
                        let relative = e
                            .path()
                            .strip_prefix(mods)?
                            .to_string_lossy()
                            .replace('\\', "/");
                        files.push(relative);
                    }
                }
            }
            files.sort();
            let mut bases = vec![
                json!({"id":"blacktop-stock","name":"Stock Blacktop court","file":"arena_700_int","floorFile":"arena_700_int_floor"}),
            ];
            bases.extend(crate::blacktop_arena::choices().iter().filter(|c| !c.disabled && c.file.is_some() && c.floor_file.is_some()).map(|c| json!({"id":c.id,"name":c.label,"file":c.file,"floorFile":c.floor_file})));
            Ok(json!({"files":files,"bases":bases,"modsRoot":mods,"levelsRoot":levels}))
        }
        Some("get") => {
            let store = read(root)?;
            let id = input["id"].as_str().context("Choose a custom arena")?;
            let entry = store.entries.iter().find(|e| e.id == id).context("Custom arena no longer exists")?;
            ensure!(Path::new(&entry.mods_root) == mods, "Switch to this arena's mods folder before editing");
            Ok(serde_json::to_value(entry)?)
        }
        Some("add" | "edit") => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Add {
                action: String,
                #[serde(default)]
                id: Option<String>,
                name: String,
                base_id: String,
                arena_source: Option<String>,
                floor_source: Option<String>,
            }
            let a: Add = serde_json::from_value(input.clone())?;
            ensure!((a.action == "edit") == a.id.is_some(), "Invalid catalog edit identity");
            let mut store = read(root)?;
            let previous = if let Some(id) = &a.id {
                let index = store.entries.iter().position(|e| &e.id == id).context("Custom arena changed; reopen the editor")?;
                ensure!(Path::new(&store.entries[index].mods_root) == mods, "Switch to this arena's mods folder before editing");
                ensure!(store.retired.len() < 1000, "Custom catalog edit history is full");
                Some(index)
            } else { None };
            let e = Entry {
                id: format!("custom-{}", backend::unique_id()),
                name: a.name.trim().into(),
                mods_root: mods.to_str().context("Invalid mods path")?.into(),
                base_id: a.base_id,
                arena_source: a.arena_source,
                floor_source: a.floor_source,
                routing_version: 1,
            };
            e.choice()?;
            for file in [&e.arena_source, &e.floor_source].into_iter().flatten() {
                ensure!(file.split('/').next().is_some_and(|part| part.eq_ignore_ascii_case("levels")),
                    "Choose arena and court files from the levels folder inside your mods folder");
            }
            e.available(mods)?;
            ensure!(previous.is_some() || store.entries.len() < 200, "Custom catalog is full");
            ensure!(
                !store
                    .entries
                    .iter()
                    .any(|v| Some(&v.id) != a.id.as_ref() && v.mods_root == e.mods_root && v.name.eq_ignore_ascii_case(&e.name)),
                "A custom arena already has that name"
            );
            if let Some(index) = previous {
                let old = std::mem::replace(&mut store.entries[index], e.clone());
                store.retired.push(old);
            } else {
                store.entries.push(e.clone());
            }
            ensure!(serde_json::to_vec_pretty(&store)?.len() <= 512 * 1024, "Custom catalog is full");
            fs::create_dir_all(root.join("Data"))?;
            backend::atomic_json(&root.join("Data/custom-blacktop-catalog.json"), &store)?;
            load(root)?;
            Ok(json!({"id":e.id,"name":e.name,"group":"Custom","previousId":a.id}))
        }
        _ => anyhow::bail!("Unknown custom catalog action"),
    }
}
