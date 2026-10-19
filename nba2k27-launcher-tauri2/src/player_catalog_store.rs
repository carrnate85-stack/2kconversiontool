//! Local immutable source catalogs. No caller-selected paths and no game access.
//! The selected pointer follows the existing settings atomic-rename model, not a
//! cross-process filesystem compare/exchange. Every import is serialized here.
use crate::{
    file_identity::Stable,
    loader_files::Directories,
    player_catalog::import::{self, LIMIT, Validated},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Pointer {
    #[serde(default = "pointer_version")]
    version: u32,
    catalog_id: String,
}
fn pointer_version() -> u32 {
    1
}
struct Source {
    data: Validated,
    id: String,
    hash: Option<String>,
    _file: Option<File>,
}
struct State {
    root: PathBuf,
    source: Arc<Source>,
    pointer: Option<(Stable, Vec<u8>)>,
    held: bool,
}
#[derive(Default)]
pub(crate) struct Store {
    state: Mutex<Option<State>>,
    import: Mutex<()>,
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn hash_valid(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn generation(hash: &str) -> String {
    digest(format!("{hash}:{}", crate::backend::unique_id()).as_bytes())
}
fn read(
    path: &Path,
    max: usize,
    check: &dyn Fn() -> Result<()>,
) -> Result<(File, Stable, Vec<u8>)> {
    check()?;
    let _dirs = Directories::pin(path.parent().context("Source parent missing")?)?;
    let mut f = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .custom_flags(0x00200000)
        .open(path)?;
    let id = Stable::read(&f)?;
    let m = f.metadata()?;
    ensure!(
        m.is_file() && m.file_attributes() & 0x400 == 0 && m.len() <= max as u64,
        "Catalog source must be a bounded ordinary file"
    );
    let mut a = Vec::new();
    (&mut f).take(max as u64 + 1).read_to_end(&mut a)?;
    check()?;
    ensure!(
        a.len() == m.len() as usize,
        "Catalog source changed or was only partially read"
    );
    f.seek(SeekFrom::Start(0))?;
    let mut b = Vec::new();
    (&mut f).take(max as u64 + 1).read_to_end(&mut b)?;
    let end = f.metadata()?;
    ensure!(
        a == b
            && end.len() == m.len()
            && end.last_write_time() == m.last_write_time()
            && Stable::read(&f)? == id,
        "Catalog source changed during verification"
    );
    check()?;
    Ok((f, id, a))
}
fn directory(root: &Path) -> Result<(PathBuf, Vec<Directories>)> {
    let mut path = root.to_owned();
    let mut pins = vec![Directories::pin(&path)?];
    for name in ["Library", "Player Imports"] {
        path.push(name);
        match fs::create_dir(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
        pins.push(Directories::pin(&path)?);
    }
    Ok((path, pins))
}
fn pointer(
    path: &Path,
    check: &dyn Fn() -> Result<()>,
) -> Result<Option<(Stable, Vec<u8>, Pointer)>> {
    match read(path, 4096, check) {
        Ok((_, id, b)) => {
            let p: Pointer = serde_json::from_slice(&b)?;
            ensure!(
                p.version == 1 && hash_valid(&p.catalog_id),
                "Selected catalog pointer is unsupported"
            );
            Ok(Some((id, b, p)))
        }
        Err(e)
            if e.downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
        {
            Ok(None)
        }
        Err(e) => Err(e),
    }
}
impl Store {
    fn source(&self, root: &Path, check: &dyn Fn() -> Result<()>) -> Result<Arc<Source>> {
        let mut lock = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Catalog state unavailable"))?;
        if let Some(s) = lock.as_ref() {
            ensure!(
                s.root == root && !s.held,
                "Catalog storage changed or publication needs recovery"
            );
            return Ok(s.source.clone());
        }
        let (dir, _pins) = directory(root)?;
        let selected = pointer(&dir.join("selected.json"), check)?;
        let (source, witness) = if let Some((id, b, p)) = selected {
            let (f, _, bytes) = read(
                &dir.join(format!("{}.2k23catalog", p.catalog_id)),
                LIMIT,
                check,
            )?;
            ensure!(
                digest(&bytes) == p.catalog_id,
                "Stored catalog hash changed"
            );
            let data = import::parse(&bytes, check)?;
            (
                Source {
                    data,
                    id: generation(&p.catalog_id),
                    hash: Some(p.catalog_id),
                    _file: Some(f),
                },
                Some((id, b)),
            )
        } else {
            let mut bytes = Vec::new();
            flate2::read::GzDecoder::new(&include_bytes!("../assets/players.json.gz")[..])
                .take(LIMIT as u64 + 1)
                .read_to_end(&mut bytes)?;
            let data = import::parse(&bytes, check)?;
            let hash = digest(&bytes);
            (
                Source {
                    data,
                    id: generation(&hash),
                    hash: None,
                    _file: None,
                },
                None,
            )
        };
        check()?;
        let source = Arc::new(source);
        *lock = Some(State {
            root: root.to_owned(),
            source: source.clone(),
            pointer: witness,
            held: false,
        });
        Ok(source)
    }
    pub(crate) fn request(
        &self,
        root: &Path,
        args: &Value,
        check: &dyn Fn() -> Result<()>,
        choose: &mut dyn FnMut() -> Result<Option<PathBuf>>,
    ) -> Result<Value> {
        check()?;
        let action = args["action"].as_str().unwrap_or("status");
        if action == "import" {
            ensure!(
                args.as_object().is_some_and(|o| o.len() == 1),
                "Catalog import accepts no caller path or options"
            );
            let _serial = self
                .import
                .try_lock()
                .map_err(|_| anyhow::anyhow!("Another catalog import is in progress"))?;
            let previous = self.source(root, check)?;
            check()?;
            let Some(path) = choose()? else {
                return Ok(Value::Null);
            };
            check()?;
            let (_, _, bytes) = read(&path, LIMIT, check)?;
            let validated = import::parse(&bytes, check)?;
            let canonical = serde_json::to_vec(validated.value())?;
            let hash = digest(&canonical);
            let (dir, _pins) = directory(root)?;
            let object = dir.join(format!("{hash}.2k23catalog"));
            check()?;
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .share_mode(1)
                .open(&object)
            {
                Ok(mut f) => {
                    f.write_all(&canonical)?;
                    f.sync_all()?
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            }
            let (f, _, stored) = read(&object, LIMIT, check)?;
            ensure!(
                stored == canonical && digest(&stored) == hash,
                "Stored immutable catalog does not match its content hash"
            );
            let next = Arc::new(Source {
                data: validated,
                id: generation(&hash),
                hash: Some(hash.clone()),
                _file: Some(f),
            });
            let mut lock = self
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("Catalog state unavailable"))?;
            let state = lock.as_mut().context("Catalog state missing")?;
            ensure!(
                !state.held && state.source.id == previous.id && state.root == root,
                "Catalog source changed during import"
            );
            let current = pointer(&dir.join("selected.json"), check)?;
            ensure!(
                match (&state.pointer, &current) {
                    (None, None) => true,
                    (Some((id, b)), Some((now, bytes, _))) => id == now && b == bytes,
                    _ => false,
                },
                "Selected catalog changed outside this request"
            );
            check()?;
            crate::backend::atomic_json(
                &dir.join("selected.json"),
                &Pointer {
                    version: 1,
                    catalog_id: hash,
                },
            )?;
            // Publication is the commit point. Do not turn later cancellation into an
            // apparent failed import with the old in-memory source still active.
            state.source = next.clone();
            match pointer(&dir.join("selected.json"), &|| Ok(())) {
                Ok(Some((id, b, p))) if Some(&p.catalog_id) == next.hash.as_ref() => {
                    state.pointer = Some((id, b))
                }
                _ => {
                    state.held = true;
                    anyhow::bail!(
                        "Catalog selection was committed but its witness could not be retained. Reopen the editor after resolving storage."
                    )
                }
            }
            return status(&next);
        }
        let source = self.source(root, check)?;
        let mut out =
            crate::player_catalog::browser_request_with_id(source.data.value(), args, &source.id)?;
        if action == "status" {
            out["bundled"] = json!(source.hash.is_none());
        }
        check()?;
        let lock = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Catalog state unavailable"))?;
        ensure!(
            lock.as_ref()
                .is_some_and(|s| !s.held && s.source.id == source.id),
            "Source catalog changed. Reopen the player search."
        );
        Ok(out)
    }
}
fn status(s: &Source) -> Result<Value> {
    let mut v = crate::player_catalog::browser_request_with_id(
        s.data.value(),
        &json!({"action":"status"}),
        &s.id,
    )?;
    v["bundled"] = json!(s.hash.is_none());
    Ok(v)
}
#[cfg(test)]
#[path = "player_catalog_store_tests.rs"]
mod tests;
