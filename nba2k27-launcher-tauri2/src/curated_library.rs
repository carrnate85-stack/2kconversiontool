//! Optional, on-demand downloads. No startup networking, selection changes, or game writes.
use crate::{
    curated_catalog::{self, Entry},
    curated_http,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Component, Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Default)]
pub struct Service {
    state: Mutex<State>,
    cancel: AtomicBool,
    closing: AtomicBool,
}
#[cfg(test)]
mod preview_tests {
    use super::*;
    #[test]
    fn preview_delegate_uses_owned_progress_and_cancellation() {
        let service = Service::default();
        service.state.lock().unwrap().entries = vec![curated_catalog::preview_pack()];
        service
            .run_preview(|_, progress, cancel| {
                progress(20, 100, "Downloading previews…");
                assert_eq!(service.state.lock().unwrap().phase, "downloading");
                assert!(service.cancel("other").is_err());
                service.cancel(curated_catalog::PREVIEW_ID)?;
                assert!(cancel.load(Ordering::Acquire));
                anyhow::bail!("Cancelled before import")
            })
            .unwrap_err();
        assert!(!service.state.lock().unwrap().busy);
        service
            .run_preview(|_, progress, _| {
                progress(3, 4, "Validating images");
                assert_eq!(service.state.lock().unwrap().phase, "importing");
                Ok(Value::Null)
            })
            .unwrap();
        assert_eq!(service.state.lock().unwrap().phase, "done");
    }
}
#[derive(Default)]
struct State {
    busy: bool,
    entries: Vec<Entry>,
    raw_entries: Vec<Entry>,
    phase: String,
    id: String,
    message: String,
    done: u64,
    total: u64,
    error: Option<String>,
    warning: String,
}
#[derive(Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
struct Receipt {
    id: String,
    sha256: String,
    kind: String,
    local_id: String,
}
struct Pins {
    _files: Vec<File>,
}
impl Pins {
    fn new(path: &Path) -> Result<Self> {
        ensure!(path.is_absolute(), "Library path must be absolute");
        let mut p = PathBuf::new();
        let mut files = vec![];
        for c in path.components() {
            ensure!(
                !matches!(c, Component::ParentDir | Component::CurDir),
                "Invalid library path"
            );
            p.push(c.as_os_str());
            if matches!(c, Component::Prefix(_)) {
                continue;
            }
            let f = fs::OpenOptions::new()
                .read(true)
                .share_mode(3)
                .custom_flags(0x02000000 | 0x00200000)
                .open(&p)?;
            let m = f.metadata()?;
            ensure!(
                m.is_dir() && m.file_attributes() & 0x400 == 0,
                "Download directory is linked or nonordinary"
            );
            files.push(f);
        }
        Ok(Self { _files: files })
    }
}
fn read_json(path: &Path) -> Result<Value> {
    let _pins = Pins::new(path.parent().context("Missing library parent")?)?;
    let mut f = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .custom_flags(0x00200000)
        .open(path)?;
    let m = f.metadata()?;
    ensure!(
        m.is_file() && m.file_attributes() & 0x400 == 0 && m.len() <= 16 * 1024 * 1024,
        "Invalid library document"
    );
    let mut bytes = vec![];
    (&mut f)
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 16 * 1024 * 1024,
        "Library document too large"
    );
    Ok(serde_json::from_slice(&bytes)?)
}
fn receipt_path(root: &Path) -> PathBuf {
    root.join("Data/curated-downloads.json")
}
fn receipts(root: &Path) -> Result<Vec<Receipt>> {
    let p = receipt_path(root);
    if !p.try_exists()? {
        return Ok(vec![]);
    }
    let r: Vec<Receipt> = serde_json::from_value(read_json(&p)?)?;
    ensure!(r.len() <= 2000, "Download history exceeds bounds");
    Ok(r)
}
fn present(root: &Path, r: &Receipt) -> bool {
    let (path, key) = if r.kind == "videos" {
        ("Data/Rust/startup-videos.json", "items")
    } else {
        ("Data/Jukebox/library.json", "playlists")
    };
    read_json(&root.join(path))
        .ok()
        .and_then(|v| v[key].as_array().cloned())
        .is_some_and(|rows| {
            rows.iter()
                .any(|row| row["id"].as_str() == Some(&r.local_id))
        })
}
fn existing_video(root: &Path, e: &Entry) -> bool {
    e.kind == "videos"
        && read_json(&root.join("Data/Rust/startup-videos.json"))
            .ok()
            .and_then(|v| v["items"].as_array().cloned())
            .is_some_and(|items| {
                items.iter().any(|i| {
                    i["sha256"].as_str() == Some(&e.sha256) && i["bytes"].as_u64() == Some(e.bytes)
                })
            })
}
impl Service {
    pub fn shutdown(&self) {
        self.closing.store(true, Ordering::Release);
        self.cancel.store(true, Ordering::Release);
    }
    pub fn cancel(&self, id: &str) -> Result<()> {
        let s = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Download state unavailable"))?;
        ensure!(s.busy && s.id == id, "This download is no longer active");
        self.cancel.store(true, Ordering::Release);
        Ok(())
    }
    fn begin(&self, id: &str, phase: &str) -> Result<()> {
        let mut s = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Download state unavailable"))?;
        ensure!(
            !s.busy && !self.closing.load(Ordering::Acquire),
            "Wait for the current download or refresh to finish"
        );
        self.cancel.store(false, Ordering::Release);
        s.busy = true;
        s.id = id.into();
        s.phase = phase.into();
        s.message = String::new();
        s.error = None;
        s.done = 0;
        s.total = 0;
        Ok(())
    }
    fn update(&self, phase: &str, message: &str, done: u64, total: u64) {
        if let Ok(mut s) = self.state.lock() {
            s.phase = phase.into();
            s.message = message.into();
            s.done = done;
            s.total = total;
        }
    }
    fn finish<T>(&self, result: &Result<T>) {
        if let Ok(mut s) = self.state.lock() {
            s.busy = false;
            s.phase = if result.is_ok() { "done" } else { "failed" }.into();
            s.error = result.as_ref().err().map(|e| format!("{e:#}"));
        }
    }
    pub fn snapshot(&self, root: &Path) -> Result<Value> {
        let (entries, status) = {
            let s = self
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("Download state unavailable"))?;
            (
                s.entries.clone(),
                json!({"busy":s.busy,"id":s.id,"phase":s.phase,"message":s.message,"done":s.done,"total":s.total,"error":s.error,"warning":s.warning}),
            )
        };
        let r = receipts(root)?;
        let rows: Vec<_> = entries
            .iter()
            .map(|e| {
                let installed = existing_video(root, e)
                    || r.iter()
                        .any(|r| r.id == e.id && r.sha256 == e.sha256 && present(root, r));
                let mut v = serde_json::to_value(e).unwrap();
                v["installed"] = json!(installed);
                v
            })
            .collect();
        Ok(json!({"items":rows,"status":status,"curator":cfg!(feature="dev-curator")}))
    }
    pub fn refresh(&self) -> Result<()> {
        self.begin("refresh", "refreshing")?;
        let result = (|| {
            let mut entries = vec![curated_catalog::preview_pack()];
            let mut ids = std::collections::BTreeSet::new();
            for (kind, folder) in curated_catalog::FOLDERS {
                for chunk in 1..=10 {
                    let url = format!(
                        "https://www.mediafire.com/api/1.5/folder/get_content.php?folder_key={folder}&content_type=files&chunk={chunk}&response_format=json"
                    );
                    let mut bytes = vec![];
                    curated_http::fetch(
                        &url,
                        &mut bytes,
                        2 * 1024 * 1024,
                        "",
                        true,
                        &self.cancel,
                        |_, _| {},
                    )?;
                    let (page, more) = curated_catalog::page(
                        &serde_json::from_slice(&bytes)?,
                        kind,
                        folder,
                        chunk,
                    )?;
                    for e in page {
                        ensure!(
                            ids.insert(e.id.clone()),
                            "Duplicate item in MediaFire listing; refresh again"
                        );
                        entries.push(e);
                    }
                    if !more {
                        break;
                    }
                    ensure!(chunk < 10, "Curated folder exceeds 1000 files");
                }
            }
            let mut raw = vec![];
            let remote = curated_http::fetch(
                crate::curated_metadata::URL,
                &mut raw,
                2 * 1024 * 1024,
                "",
                true,
                &self.cancel,
                |_, _| {},
            );
            let (catalog, source, warning) = match remote {
                Ok(()) => (
                    serde_json::from_slice(&raw)?,
                    Some(crate::curated_metadata::URL),
                    String::new(),
                ),
                Err(e) if format!("{e:#}").contains("HTTP 404") => (
                    crate::curated_metadata::bundled()?,
                    None,
                    "Using the included catalog. The GitHub catalog has not been published yet."
                        .into(),
                ),
                Err(e) => {
                    return Err(e.context(
                        "Could not refresh the GitHub catalog; the previous listing was kept",
                    ));
                }
            };
            let raw_entries = entries.clone();
            crate::curated_metadata::apply(&mut entries, &catalog, source)?;
            self.state
                .lock()
                .map_err(|_| anyhow::anyhow!("Download state unavailable"))?
                .raw_entries = raw_entries;
            self.state
                .lock()
                .map_err(|_| anyhow::anyhow!("Download state unavailable"))?
                .warning = warning;
            self.state
                .lock()
                .map_err(|_| anyhow::anyhow!("Download state unavailable"))?
                .entries = entries;
            Ok(())
        })();
        self.finish(&result);
        result
    }
    pub fn preview(&self, catalog: &crate::curated_metadata::Catalog) -> Result<()> {
        let mut s = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Download state unavailable"))?;
        ensure!(!s.busy, "Wait for the current download first");
        let mut entries = s.raw_entries.clone();
        crate::curated_metadata::apply(&mut entries, catalog, None)?;
        s.entries = entries;
        s.warning = "Local curator preview. Publish to GitHub to share these changes.".into();
        Ok(())
    }
    pub fn run_preview(
        &self,
        work: impl FnOnce(&Entry, &dyn Fn(usize, usize, &str), &AtomicBool) -> Result<Value>,
    ) -> Result<()> {
        self.begin(curated_catalog::PREVIEW_ID, "downloading")?;
        let result = (|| {
            let entry = self
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("Download state unavailable"))?
                .entries
                .iter()
                .find(|e| e.id == curated_catalog::PREVIEW_ID)
                .cloned()
                .context("Refresh the library before downloading previews")?;
            work(
                &entry,
                &|done, total, message| {
                    let phase = if message.starts_with("Downloading")
                        || message.starts_with("Connecting")
                    {
                        "downloading"
                    } else {
                        "importing"
                    };
                    self.update(phase, message, done as u64, total as u64);
                },
                &self.cancel,
            )?;
            self.update("done", "Blacktop previews are ready", 1, 1);
            Ok(())
        })();
        self.finish(&result);
        result
    }
    pub fn install(
        &self,
        root: &Path,
        id: &str,
        import: impl FnOnce(&Entry, &Path, &AtomicBool, &dyn Fn(&str)) -> Result<String>,
    ) -> Result<()> {
        self.begin(id, "downloading")?;
        let result = (|| {
            let entry = self
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("Download state unavailable"))?
                .entries
                .iter()
                .find(|e| e.id == id)
                .cloned()
                .context("Refresh the download library before choosing an item")?;
            ensure!(entry.supported, "{}", entry.reason);
            ensure!(
                entry.kind != "previews",
                "Preview packs require the pinned preview importer"
            );
            let mut history = receipts(root)?;
            ensure!(
                !existing_video(root, &entry)
                    && !history
                        .iter()
                        .any(|r| r.id == id && r.sha256 == entry.sha256 && present(root, r)),
                "This download is already in your library"
            );
            let _root = Pins::new(root)?;
            let cache = root.join("Cache");
            fs::create_dir_all(&cache)?;
            let _cache = Pins::new(&cache)?;
            let downloads = cache.join("CuratedDownloads");
            fs::create_dir_all(&downloads)?;
            let _downloads = Pins::new(&downloads)?;
            let dir = downloads.join(crate::backend::unique_id());
            fs::create_dir(&dir)?;
            let _dir = Pins::new(&dir)?;
            let path = dir.join(if entry.kind == "videos" {
                "download.mp4"
            } else {
                "download.2kplaylist"
            });
            let work = (|| {
                let mut out = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .share_mode(1)
                    .open(&path)?;
                curated_http::fetch(
                    &entry.url,
                    &mut out,
                    entry.bytes,
                    &entry.sha256,
                    false,
                    &self.cancel,
                    |done, total| self.update("downloading", "Downloading", done, total),
                )?;
                out.sync_all()?;
                drop(out);
                curated_http::check(&self.cancel)?;
                // Keep the verified payload immutable through the entire existing importer.
                let mut source = fs::OpenOptions::new()
                    .read(true)
                    .share_mode(1)
                    .custom_flags(0x00200000)
                    .open(&path)?;
                ensure!(
                    source.metadata()?.file_attributes() & 0x400 == 0,
                    "Downloaded file became linked"
                );
                let mut hash = Sha256::new();
                let mut buf = vec![0; 256 * 1024];
                loop {
                    curated_http::check(&self.cancel)?;
                    let n = source.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    hash.update(&buf[..n]);
                }
                ensure!(
                    format!("{:x}", hash.finalize()) == entry.sha256,
                    "Downloaded file changed before import"
                );
                self.update(
                    "importing",
                    "Validating and adding to your library",
                    entry.bytes,
                    entry.bytes,
                );
                let local_id = import(&entry, &path, &self.cancel, &|message| {
                    self.update("importing", message, entry.bytes, entry.bytes)
                })?;
                history.retain(|r| r.id != id);
                history.push(Receipt {
                    id: id.into(),
                    sha256: entry.sha256.clone(),
                    kind: entry.kind.clone(),
                    local_id,
                });
                let data = root.join("Data");
                fs::create_dir_all(&data)?;
                let _data = Pins::new(&data)?;
                crate::backend::atomic_json(&receipt_path(root),&history).context("Content was added, but download history could not be saved. Check your library before retrying")?;
                self.update(
                    "done",
                    "Added to your library. Select it in Presentation or Jukebox when ready.",
                    entry.bytes,
                    entry.bytes,
                );
                Ok(())
            })();
            // Only our unique payload and empty directory; never recurse into user data.
            let _ = fs::remove_file(&path);
            drop(_dir);
            let _ = fs::remove_dir(&dir);
            work
        })();
        self.finish(&result);
        result
    }
}
