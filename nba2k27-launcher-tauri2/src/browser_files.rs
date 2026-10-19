//! Native Player DNA file dialogs. Browser requests never carry a file path.
//! COM lives on the main STA. HTTP workers have cancellable bounded waits, so
//! closing a modal cannot deadlock browser shutdown on that same UI thread.
use crate::{backend, player_dna, team_dna};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::os::windows::fs::OpenOptionsExt;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use tauri::Manager;
use windows::{
    Win32::{
        Foundation::HWND,
        System::{
            Com::*,
            Threading::{GetCurrentProcessId, GetCurrentThreadId},
        },
        UI::{
            Shell::Common::COMDLG_FILTERSPEC,
            Shell::*,
            WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow},
        },
    },
    core::{HRESULT, Interface, PCWSTR},
};
const CANCEL: HRESULT = HRESULT(0x800704c7u32 as i32);
const MAX: u64 = 256 * 1024;
#[derive(Clone, Debug)]
pub(crate) enum Choice {
    Import,
    Export(String),
    TeamImport,
    CatalogImport,
    ArtworkImport,
    TeamExport(String),
    /// WPF player-research:export ("Export live NBA 2K27 player catalog").
    Catalog,
}
struct Active {
    pointer: usize,
    thread: u32,
    epoch: String,
}
pub(crate) struct Service {
    catalog: crate::player_catalog_store::Store,
    artwork: crate::roster_artwork_import::Store,
    stop: AtomicBool,
    busy: Mutex<()>,
    active: Mutex<Option<Active>>,
}
impl Service {
    pub fn new() -> Self {
        Self {
            catalog: crate::player_catalog_store::Store::default(),
            artwork: crate::roster_artwork_import::Store::default(),
            stop: AtomicBool::new(false),
            busy: Mutex::new(()),
            active: Mutex::new(None),
        }
    }
    pub(crate) fn catalog(
        &self,
        root: &Path,
        request: &Value,
        mut choose: impl FnMut(Choice) -> Result<Option<PathBuf>>,
    ) -> Result<Value> {
        if request["action"] == "import" {
            let _busy = self
                .busy
                .try_lock()
                .map_err(|_| anyhow::anyhow!("Another file dialog is active"))?;
            self.catalog
                .request(root, request, &|| self.check(), &mut || {
                    choose(Choice::CatalogImport)
                })
        } else {
            self.catalog
                .request(root, request, &|| self.check(), &mut || {
                    anyhow::bail!("Catalog query cannot open a file dialog")
                })
        }
    }
    pub(crate) fn artwork(
        &self,
        root: &Path,
        tab: &str,
        request: &Value,
        mut choose: impl FnMut(Choice) -> Result<Option<PathBuf>>,
    ) -> Result<Value> {
        if request["choose"] == true {
            let _busy = self
                .busy
                .try_lock()
                .map_err(|_| anyhow::anyhow!("Another file dialog is active"))?;
            self.artwork
                .request(root, tab, request, &|| self.check(), &mut || {
                    choose(Choice::ArtworkImport)
                })
        } else {
            self.artwork
                .request(root, tab, request, &|| self.check(), &mut || {
                    anyhow::bail!("Artwork commit cannot open a dialog")
                })
        }
    }
    fn check(&self) -> Result<()> {
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Player DNA file request cancelled because the launcher is closing"
        );
        Ok(())
    }
    /// Only close the exact COM object, on the apartment that owns it. The
    /// pointer is never supplied by a caller and stays alive through Show().
    fn close_active(&self, epoch: Option<&str>) {
        if let Ok(active) = self.active.lock() {
            if let Some(a) = active
                .as_ref()
                .filter(|a| epoch.is_none_or(|e| e == a.epoch))
            {
                if a.thread == unsafe { GetCurrentThreadId() } {
                    let raw = a.pointer as *mut std::ffi::c_void;
                    if let Some(dialog) = unsafe { IFileDialog::from_raw_borrowed(&raw) } {
                        let _ = unsafe { dialog.Close(CANCEL) };
                    }
                }
            }
        }
    }
    pub fn cancel(&self) {
        self.stop.store(true, Ordering::Release);
        self.close_active(None);
    }
    fn show(&self, owner: HWND, choice: Choice, epoch: String) -> Result<Option<PathBuf>> {
        self.check()?;
        let mut pid = 0;
        let thread = unsafe { GetWindowThreadProcessId(owner, Some(&mut pid)) };
        ensure!(
            unsafe { IsWindow(Some(owner)) }.as_bool()
                && pid == unsafe { GetCurrentProcessId() }
                && thread == unsafe { GetCurrentThreadId() },
            "File dialog owner is not this launcher's UI window"
        );
        let dialog: IFileDialog = unsafe {
            match &choice {
                Choice::Import
                | Choice::TeamImport
                | Choice::CatalogImport
                | Choice::ArtworkImport => CoCreateInstance::<_, IFileOpenDialog>(
                    &FileOpenDialog,
                    None,
                    CLSCTX_INPROC_SERVER,
                )?
                .cast()?,
                Choice::Export(_) | Choice::TeamExport(_) | Choice::Catalog => {
                    CoCreateInstance::<_, IFileSaveDialog>(
                        &FileSaveDialog,
                        None,
                        CLSCTX_INPROC_SERVER,
                    )?
                    .cast()?
                }
            }
        };
        let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let catalog = matches!(choice, Choice::Catalog);
        let catalog_import = matches!(choice, Choice::CatalogImport);
        let artwork = matches!(choice, Choice::ArtworkImport);
        let team = matches!(choice, Choice::TeamImport | Choice::TeamExport(_));
        let title = wide(match choice {
            Choice::Import => "Import player DNA",
            Choice::ArtworkImport => "Choose player portrait or team logo",
            Choice::CatalogImport => "Import NBA 2K23 source catalog",
            Choice::Export(_) => "Export player DNA",
            Choice::TeamImport => "Import Team DNA",
            Choice::TeamExport(_) => "Export Team DNA",
            Choice::Catalog => "Export live NBA 2K27 player catalog",
        });
        let filter = wide(if artwork {
            "Images"
        } else if catalog_import {
            "NBA 2K23 catalog"
        } else if catalog {
            "JSON catalog"
        } else if team {
            "Team DNA"
        } else {
            "Player DNA"
        });
        let extension = wide(if artwork {
            "png"
        } else if catalog_import {
            "2k23catalog"
        } else if catalog {
            "json"
        } else if team {
            "2kteam"
        } else {
            "2kplayer"
        });
        let pattern = wide(match choice {
            Choice::Import => "*.2kplayer;*.json",
            Choice::ArtworkImport => "*.png;*.jpg;*.jpeg;*.webp",
            Choice::CatalogImport => "*.2k23catalog;*.2k23stats",
            Choice::Export(_) => "*.2kplayer",
            Choice::TeamImport | Choice::TeamExport(_) => "*.2kteam",
            Choice::Catalog => "*.json",
        });
        unsafe {
            dialog.SetOptions(
                FOS_FORCEFILESYSTEM
                    | FOS_PATHMUSTEXIST
                    | FOS_NOCHANGEDIR
                    | FOS_DONTADDTORECENT
                    | if matches!(
                        choice,
                        Choice::Import
                            | Choice::TeamImport
                            | Choice::CatalogImport
                            | Choice::ArtworkImport
                    ) {
                        FOS_FILEMUSTEXIST
                    } else {
                        FOS_OVERWRITEPROMPT
                    },
            )?;
            dialog.SetTitle(PCWSTR(title.as_ptr()))?;
            dialog.SetFileTypes(&[COMDLG_FILTERSPEC {
                pszName: PCWSTR(filter.as_ptr()),
                pszSpec: PCWSTR(pattern.as_ptr()),
            }])?;
            dialog.SetDefaultExtension(PCWSTR(extension.as_ptr()))?;
            if let Choice::Export(ref name) | Choice::TeamExport(ref name) = choice {
                let name = wide(name);
                dialog.SetFileName(PCWSTR(name.as_ptr()))?;
            }
            if catalog {
                let name = wide("NBA2K27-Player-Cyberface-Catalog.json");
                dialog.SetFileName(PCWSTR(name.as_ptr()))?;
            }
        }
        struct Clear<'a> {
            service: &'a Service,
            epoch: String,
        }
        impl Drop for Clear<'_> {
            fn drop(&mut self) {
                if let Ok(mut a) = self.service.active.lock() {
                    if a.as_ref().is_some_and(|a| a.epoch == self.epoch) {
                        *a = None;
                    }
                }
            }
        }
        *self
            .active
            .lock()
            .map_err(|_| anyhow::anyhow!("File dialog ownership unavailable"))? = Some(Active {
            pointer: dialog.as_raw() as usize,
            thread,
            epoch: epoch.clone(),
        });
        let _clear = Clear {
            service: self,
            epoch,
        };
        self.check()?;
        if let Err(e) = unsafe { dialog.Show(Some(owner)) } {
            if e.code() == CANCEL {
                return Ok(None);
            }
            return Err(e.into());
        }
        self.check()?;
        let path = unsafe { dialog.GetResult()?.GetDisplayName(SIGDN_FILESYSPATH)? };
        let value = unsafe { path.to_string() };
        unsafe {
            CoTaskMemFree(Some(path.0 as _));
        }
        Ok(Some(PathBuf::from(value?)))
    }
    pub fn ask(
        self: &Arc<Self>,
        app: &tauri::AppHandle,
        choice: Choice,
    ) -> Result<Option<PathBuf>> {
        self.check()?;
        let main = app
            .get_webview_window("main")
            .context("Launcher is closed")?;
        let owner = main.hwnd()?.0 as usize;
        let epoch = backend::unique_id();
        let task_epoch = epoch.clone();
        let (tx, rx) = mpsc::channel();
        let service = self.clone();
        let dialog_owner = main.clone();
        main.run_on_main_thread(move || {
            let result = (|| {
                service.check()?;
                dialog_owner.show()?;
                dialog_owner.set_focus()?;
                service.show(HWND(owner as _), choice, task_epoch)
            })();
            let _ = tx.send(result);
        })?;
        let started = Instant::now();
        loop {
            self.check()?;
            if started.elapsed() > Duration::from_secs(300) {
                let service = self.clone();
                let _ = main.run_on_main_thread(move || service.close_active(Some(&epoch)));
                anyhow::bail!(
                    "Player DNA file dialog timed out. No file was imported or exported."
                );
            }
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(value) => {
                    self.check()?;
                    return value;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(e) => return Err(e.into()),
            }
        }
    }
    /// WPF exportCatalog(): pretty JSON + newline at a user-chosen path.
    pub fn export_catalog(
        &self,
        catalog: &Value,
        mut choose: impl FnMut(Choice) -> Result<Option<PathBuf>>,
    ) -> Result<Value> {
        self.check()?;
        let _busy = self
            .busy
            .try_lock()
            .map_err(|_| anyhow::anyhow!("A file dialog is already open"))?;
        let Some(path) = choose(Choice::Catalog)? else {
            return Ok(Value::Null);
        };
        self.check()?;
        ensure!(
            path.extension()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("json")),
            "Choose a .json catalog file"
        );
        let mut text = serde_json::to_string_pretty(catalog)?;
        text.push('\n');
        let temp = path.with_extension(format!("json.{}.tmp", backend::unique_id()));
        fs::write(&temp, text)?;
        fs::rename(&temp, &path).inspect_err(|_| {
            let _ = fs::remove_file(&temp);
        })?;
        Ok(json!({"path":path,"count":catalog["count"]}))
    }
    pub fn transfer(
        &self,
        request: &Value,
        mut choose: impl FnMut(Choice) -> Result<Option<PathBuf>>,
    ) -> Result<Value> {
        self.check()?;
        let _busy = self
            .busy
            .try_lock()
            .map_err(|_| anyhow::anyhow!("A Player DNA file dialog is already open"))?;
        let sections = request["sections"]
            .as_array()
            .context("Choose Player DNA sections")?;
        ensure!(
            !sections.is_empty()
                && sections.iter().all(|s| player_dna::schema()["groups"]
                    .as_array()
                    .unwrap()
                    .contains(s)),
            "Choose at least one valid DNA section"
        );
        match request["action"].as_str() {
            Some("import") => {
                let Some(path) = choose(Choice::Import)? else {
                    return Ok(Value::Null);
                };
                self.check()?;
                ensure!(
                    path.extension()
                        .and_then(|s| s.to_str())
                        .is_some_and(|s| s.eq_ignore_ascii_case("2kplayer")
                            || s.eq_ignore_ascii_case("json")),
                    "Select a .2kplayer or .json file"
                );
                let file = fs::File::open(path)?;
                let before = file.metadata()?;
                ensure!(
                    before.is_file() && before.len() <= MAX,
                    "Player DNA must be smaller than 256 KB"
                );
                let mut bytes = Vec::new();
                (&file).take(MAX + 1).read_to_end(&mut bytes)?;
                let after = file.metadata()?;
                self.check()?;
                ensure!(
                    bytes.len() as u64 <= MAX
                        && before.len() == after.len()
                        && before.modified()? == after.modified()?,
                    "Player DNA changed while importing or exceeds 256 KB"
                );
                player_dna::subset(&serde_json::from_slice(&bytes)?, &request["sections"])
            }
            Some("export") => {
                let data = player_dna::subset(&request["data"], &request["sections"])?;
                let filter = regex::Regex::new(r"[^\p{L}\p{N} _-]")?;
                let name = filter.replace_all(data["name"].as_str().unwrap(), "");
                let mut length = 0;
                let name = name
                    .trim_matches(crate::player_changes::js_space)
                    .chars()
                    .take_while(|c| {
                        length += c.len_utf16();
                        length <= 60
                    })
                    .collect::<String>();
                let name = if name.is_empty() {
                    "Player".into()
                } else {
                    name
                };
                let Some(path) = choose(Choice::Export(format!("{name}.2kplayer")))? else {
                    return Ok(Value::Null);
                };
                self.check()?;
                backend::atomic_json(&path, &data)?;
                Ok(json!({"saved":true}))
            }
            _ => anyhow::bail!("Invalid Player DNA file action"),
        }
    }
    /// File-only Team DNA transfer. No renderer path, record ID, or game-write
    /// capability is accepted; both DNA dialogs share the same busy/cancel gate.
    pub fn transfer_team(
        &self,
        request: &Value,
        mut choose: impl FnMut(Choice) -> Result<Option<PathBuf>>,
    ) -> Result<Value> {
        self.check()?;
        let _busy = self
            .busy
            .try_lock()
            .map_err(|_| anyhow::anyhow!("A DNA file dialog is already open"))?;
        let object = request.as_object().context("Invalid Team DNA request")?;
        ensure!(
            object
                .keys()
                .all(|k| matches!(k.as_str(), "action" | "sections" | "data")),
            "Team DNA requests cannot supply file paths or native identities"
        );
        let sections = request["sections"]
            .as_array()
            .context("Choose Team DNA sections")?;
        ensure!(
            !sections.is_empty()
                && sections.len() <= 5
                && sections.iter().all(|s| team_dna::schema()["sections"]
                    .as_array()
                    .unwrap()
                    .contains(s)),
            "Choose valid Team DNA sections"
        );
        match request["action"].as_str() {
            Some("import") => {
                let Some(path) = choose(Choice::TeamImport)? else {
                    return Ok(Value::Null);
                };
                self.check()?;
                ensure!(
                    path.extension()
                        .and_then(|s| s.to_str())
                        .is_some_and(|s| s.eq_ignore_ascii_case("2kteam")),
                    "Select a .2kteam file"
                );
                let file = fs::OpenOptions::new()
                    .read(true)
                    .share_mode(windows::Win32::Storage::FileSystem::FILE_SHARE_READ.0)
                    .open(path)?;
                let before = file.metadata()?;
                const LIMIT: u64 = 512 * 1024;
                ensure!(
                    before.is_file() && before.len() <= LIMIT,
                    "Team DNA exceeds 512 KB"
                );
                let mut bytes = Vec::new();
                (&file).take(LIMIT + 1).read_to_end(&mut bytes)?;
                let after = file.metadata()?;
                self.check()?;
                ensure!(
                    bytes.len() as u64 <= LIMIT
                        && bytes.len() as u64 == before.len()
                        && before.len() == after.len()
                        && before.modified()? == after.modified()?,
                    "Team DNA changed while reading or exceeds 512 KB"
                );
                team_dna::subset(&serde_json::from_slice(&bytes)?, &request["sections"])
            }
            Some("export") => {
                let data = team_dna::subset(&request["data"], &request["sections"])?;
                ensure!(
                    serde_json::to_vec_pretty(&data)?.len() + 1 <= 512 * 1024,
                    "Team DNA export exceeds 512 KB"
                );
                let filter = regex::Regex::new(r"[^\p{L}\p{N} _-]")?;
                let name = filter.replace_all(data["name"].as_str().unwrap(), "");
                let name: String = name
                    .trim_matches(crate::player_changes::js_space)
                    .chars()
                    .take(50)
                    .collect();
                let name = if name.is_empty() {
                    "Team".to_owned()
                } else {
                    name
                };
                let Some(path) = choose(Choice::TeamExport(format!("{name}.2kteam")))? else {
                    return Ok(Value::Null);
                };
                self.check()?;
                ensure!(
                    path.extension()
                        .and_then(|s| s.to_str())
                        .is_some_and(|s| s.eq_ignore_ascii_case("2kteam")),
                    "Choose a .2kteam file"
                );
                backend::atomic_json(&path, &data)?;
                Ok(json!({"saved":true}))
            }
            _ => anyhow::bail!("Invalid Team DNA file action"),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn data() -> Value {
        json!({"format":"2kplayer","version":1,"game":"NBA2K27","schemaId":player_dna::schema()["schemaId"],"name":"QA / 🏀 Player","info":{"firstName":"QA","birthDate":"2000-02-29","yearsPro":3},"fields":{},"recordAddress":"0xDEADBEEF","pid":7})
    }
    #[test]
    fn original_file_contract_roundtrip_cancel_and_foreign_paths_are_not_from_rpc() {
        let root =
            std::env::temp_dir().join(format!("rust-browser-files-{}", backend::unique_id()));
        fs::create_dir(&root).unwrap();
        let target = root.join("chosen.2kplayer");
        let service = Service::new();
        let request =
            json!({"action":"export","data":data(),"sections":["info"],"path":"never-used"});
        let saved = service
            .transfer(&request, |choice| {
                let Choice::Export(name) = choice else {
                    panic!("wrong dialog")
                };
                assert_eq!(name, "QA   Player.2kplayer");
                Ok(Some(target.clone()))
            })
            .unwrap();
        assert_eq!(saved, json!({"saved":true}));
        let bytes = fs::read(&target).unwrap();
        let exported: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(exported.get("pid").is_none());
        assert!(exported.get("recordAddress").is_none());
        let imported = service
            .transfer(&json!({"action":"import","sections":["info"]}), |choice| {
                assert!(matches!(choice, Choice::Import));
                Ok(Some(target.clone()))
            })
            .unwrap();
        assert_eq!(imported, exported);
        assert_eq!(
            service.transfer(&request, |_| Ok(None)).unwrap(),
            Value::Null
        );
        assert_eq!(fs::read(&target).unwrap(), bytes);
        let bad = root.join("bad.json");
        fs::write(&bad, b"{broken").unwrap();
        assert!(
            service
                .transfer(&json!({"action":"import","sections":["info"]}), |_| Ok(
                    Some(bad.clone())
                ))
                .is_err()
        );
        assert_eq!(fs::read(&bad).unwrap(), b"{broken");
        let canceled_target = root.join("cancel.2kplayer");
        assert!(
            service
                .transfer(&request, |_| {
                    service.cancel();
                    Ok(Some(canceled_target.clone()))
                })
                .is_err()
        );
        assert!(!canceled_target.exists());
        assert!(
            service
                .transfer(&request, |_| panic!(
                    "Cancelled request must not open another dialog"
                ))
                .is_err()
        );
        fs::remove_file(target).unwrap();
        fs::remove_file(bad).unwrap();
        fs::remove_dir(root).unwrap();
    }
    #[test]
    fn invalid_and_busy_requests_never_open_a_dialog() {
        let service = Service::new();
        for request in [
            json!({"action":"export","data":data(),"sections":[]}),
            json!({"action":"export","data":{},"sections":["info"]}),
            json!({"action":"import","sections":["bad"]}),
            json!({"action":"launch","sections":["info"]}),
        ] {
            assert!(
                service
                    .transfer(&request, |_| panic!("Invalid request opened a chooser"))
                    .is_err()
            );
        }
        let _busy = service.busy.lock().unwrap();
        assert!(
            service
                .transfer(&json!({"action":"import","sections":["info"]}), |_| panic!(
                    "Busy chooser"
                ))
                .is_err()
        );
    }
    #[test]
    fn missing_owner_is_refused_before_creating_or_showing_a_dialog() {
        let service = Service::new();
        assert!(
            service
                .show(HWND::default(), Choice::Import, "own-fixture".into())
                .unwrap_err()
                .to_string()
                .contains("owner")
        );
        assert!(service.active.lock().unwrap().is_none());
    }
    #[test]
    fn team_dna_native_chooser_subset_roundtrip_and_refusals() {
        let root = std::env::temp_dir().join(format!("rust-team-files-{}", backend::unique_id()));
        fs::create_dir(&root).unwrap();
        let path = root.join("chosen.2kteam");
        let service = Service::new();
        let data = json!({"format":"2kteam","version":1,"game":"NBA2K27","name":"QA Team",
            "info":{"name":"QA Team","mainLogo":"logo"},"arena":null,"staff":[],"rotations":null,"uniforms":[],"teamId":777});
        let exported = json!({"action":"export","sections":["info"],"data":data});
        assert_eq!(
            service
                .transfer_team(&exported, |choice| {
                    assert!(matches!(choice, Choice::TeamExport(_)));
                    Ok(Some(path.clone()))
                })
                .unwrap(),
            json!({"saved":true})
        );
        let before = fs::read(&path).unwrap();
        let result = service
            .transfer_team(&json!({"action":"import","sections":["info"]}), |choice| {
                assert!(matches!(choice, Choice::TeamImport));
                Ok(Some(path.clone()))
            })
            .unwrap();
        assert!(result.get("teamId").is_none());
        assert_eq!(result["info"]["name"], "QA Team");
        assert_eq!(result["staff"], json!([]));
        let writer = fs::OpenOptions::new().write(true).open(&path).unwrap();
        assert!(
            service
                .transfer_team(&json!({"action":"import","sections":["info"]}), |_| Ok(
                    Some(path.clone())
                ))
                .is_err(),
            "Team import must refuse a concurrently writable file"
        );
        drop(writer);
        assert_eq!(
            service.transfer_team(&exported, |_| Ok(None)).unwrap(),
            Value::Null
        );
        assert_eq!(fs::read(&path).unwrap(), before);
        for invalid in [
            json!({"action":"import","sections":["info"],"path":"caller.2kteam"}),
            json!({"action":"import","sections":["bad"]}),
            json!({"action":"write","sections":["info"]}),
        ] {
            assert!(
                service
                    .transfer_team(&invalid, |_| panic!("invalid request opened dialog"))
                    .is_err()
            );
        }
        let busy = service.busy.lock().unwrap();
        assert!(
            service
                .transfer_team(&exported, |_| panic!("busy request opened dialog"))
                .is_err()
        );
        drop(busy);
        fs::remove_file(path).unwrap();
        fs::remove_dir(root).unwrap();
    }
}
