//! Production lazy Jukebox runtime, owned STA file dialogs and preview server.
//! Initialization occurs on opening Jukebox, not application startup. Never
//! launches Steam or touches a game process/session file.
use crate::{
    jukebox_api::{Api, Host, admission},
    jukebox_http::Server,
    jukebox_v2_resources,
};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU16, Ordering},
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
// Flip only after the provider's owned prepare/index/config/restore route is
// integrated and its end-to-end offline tests pass. Never renderer-controlled.
const PROVIDER_JUKEBOX_INTEGRATED: bool = true;
#[path = "jukebox_smoke.rs"]
mod smoke;
struct ActiveDialog {
    pointer: usize,
    thread: u32,
    epoch: String,
}
#[derive(Clone)]
struct NativeHost {
    app: tauri::AppHandle,
    active: Arc<Mutex<Option<ActiveDialog>>>,
    stop: Arc<AtomicBool>,
    port: Arc<AtomicU16>,
    decode_admission: Arc<OnceLock<admission::Check>>,
}
enum Choice {
    Audio,
    SingleAudio,
    Artwork,
    Playlist,
    Export(String),
}
impl NativeHost {
    fn new(app: tauri::AppHandle, decode_admission: Arc<OnceLock<admission::Check>>) -> Self {
        Self {
            app,
            active: Arc::new(Mutex::new(None)),
            stop: Arc::new(AtomicBool::new(false)),
            port: Arc::new(AtomicU16::new(0)),
            decode_admission,
        }
    }
    fn close_active(&self, epoch: Option<&str>) {
        if let Ok(a) = self.active.lock() {
            if let Some(a) = a.as_ref().filter(|a| epoch.is_none_or(|e| e == a.epoch)) {
                if a.thread == unsafe { GetCurrentThreadId() } {
                    let raw = a.pointer as *mut std::ffi::c_void;
                    if let Some(dialog) = unsafe { IFileDialog::from_raw_borrowed(&raw) } {
                        let _ = unsafe { dialog.Close(CANCEL) };
                    }
                }
            }
        }
    }
    fn cancel(&self) {
        self.stop.store(true, Ordering::Release);
        self.close_active(None);
    }
    fn show(&self, owner: HWND, choice: Choice, epoch: String) -> Result<Option<Vec<PathBuf>>> {
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Jukebox dialog cancelled"
        );
        let mut pid = 0;
        let thread = unsafe { GetWindowThreadProcessId(owner, Some(&mut pid)) };
        ensure!(
            unsafe { IsWindow(Some(owner)) }.as_bool()
                && pid == unsafe { GetCurrentProcessId() }
                && thread == unsafe { GetCurrentThreadId() },
            "Jukebox dialog owner is not this launcher's UI window"
        );
        let dialog: IFileDialog = unsafe {
            match &choice {
                Choice::Export(_) => CoCreateInstance::<_, IFileSaveDialog>(
                    &FileSaveDialog,
                    None,
                    CLSCTX_INPROC_SERVER,
                )?
                .cast()?,
                _ => CoCreateInstance::<_, IFileOpenDialog>(
                    &FileOpenDialog,
                    None,
                    CLSCTX_INPROC_SERVER,
                )?
                .cast()?,
            }
        };
        let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let (label, pattern, title) = match &choice {
            Choice::Audio | Choice::SingleAudio => (
                "Audio files",
                "*.mp3;*.wav;*.m4a;*.aac;*.flac;*.ogg;*.opus",
                "Import Jukebox songs",
            ),
            Choice::Artwork => (
                "Cover image",
                "*.png;*.jpg;*.jpeg;*.bmp;*.webp",
                "Choose Jukebox cover artwork",
            ),
            Choice::Playlist => (
                "2k Playlist",
                "*.2kplaylist",
                "Import complete Jukebox playlist",
            ),
            Choice::Export(_) => (
                "2k Playlist",
                "*.2kplaylist",
                "Export Jukebox playlist to a new file",
            ),
        };
        let label = wide(label);
        let pattern = wide(pattern);
        let title = wide(title);
        let extension = wide("2kplaylist");
        unsafe {
            dialog.SetOptions(
                FOS_FORCEFILESYSTEM
                    | FOS_PATHMUSTEXIST
                    | FOS_NOCHANGEDIR
                    | FOS_DONTADDTORECENT
                    | match choice {
                        Choice::Audio => FOS_FILEMUSTEXIST | FOS_ALLOWMULTISELECT,
                        Choice::SingleAudio | Choice::Artwork => FOS_FILEMUSTEXIST,
                        Choice::Playlist => FOS_FILEMUSTEXIST,
                        Choice::Export(_) => FOS_NOREADONLYRETURN,
                    },
            )?;
            dialog.SetTitle(PCWSTR(title.as_ptr()))?;
            dialog.SetFileTypes(&[COMDLG_FILTERSPEC {
                pszName: PCWSTR(label.as_ptr()),
                pszSpec: PCWSTR(pattern.as_ptr()),
            }])?;
            if matches!(choice, Choice::Playlist | Choice::Export(_)) {
                dialog.SetDefaultExtension(PCWSTR(extension.as_ptr()))?;
            }
            if let Choice::Export(ref name) = choice {
                let name = wide(name);
                dialog.SetFileName(PCWSTR(name.as_ptr()))?;
            }
        }
        struct Clear {
            active: Arc<Mutex<Option<ActiveDialog>>>,
            epoch: String,
        }
        impl Drop for Clear {
            fn drop(&mut self) {
                if let Ok(mut a) = self.active.lock() {
                    if a.as_ref().is_some_and(|a| a.epoch == self.epoch) {
                        *a = None;
                    }
                }
            }
        }
        *self
            .active
            .lock()
            .map_err(|_| anyhow::anyhow!("Jukebox dialog ownership unavailable"))? =
            Some(ActiveDialog {
                pointer: dialog.as_raw() as usize,
                thread,
                epoch: epoch.clone(),
            });
        let _clear = Clear {
            active: self.active.clone(),
            epoch,
        };
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Jukebox dialog cancelled"
        );
        if let Err(e) = unsafe { dialog.Show(Some(owner)) } {
            if e.code() == CANCEL {
                return Ok(None);
            }
            return Err(e.into());
        }
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Jukebox dialog cancelled"
        );
        fn path(item: IShellItem) -> Result<PathBuf> {
            let p = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH)? };
            let result = unsafe { p.to_string() };
            unsafe {
                CoTaskMemFree(Some(p.0 as _));
            }
            Ok(PathBuf::from(result?))
        }
        let paths = if matches!(choice, Choice::Audio) {
            let items = unsafe { dialog.cast::<IFileOpenDialog>()?.GetResults()? };
            let count = unsafe { items.GetCount()? };
            ensure!(
                (1..=100).contains(&count),
                "Choose between one and100 songs per import"
            );
            let mut paths = vec![];
            for i in 0..count {
                paths.push(path(unsafe { items.GetItemAt(i)? })?);
            }
            paths
        } else {
            vec![path(unsafe { dialog.GetResult()? })?]
        };
        Ok(Some(paths))
    }
    fn ask(&self, choice: Choice, cancel: &AtomicBool) -> Result<Option<Vec<PathBuf>>> {
        ensure!(
            !self.stop.load(Ordering::Acquire) && !cancel.load(Ordering::Acquire),
            "Jukebox dialog cancelled"
        );
        let main = self
            .app
            .get_webview_window("main")
            .context("Launcher is closed")?;
        let owner = main.hwnd()?.0 as usize;
        let epoch = crate::backend::unique_id();
        let owned_epoch = epoch.clone();
        let host = self.clone();
        let dialog_owner = main.clone();
        let (tx, rx) = mpsc::channel();
        main.run_on_main_thread(move || {
            let result = (|| {
                ensure!(!host.stop.load(Ordering::Acquire), "Jukebox is closing");
                dialog_owner.show()?;
                dialog_owner.set_focus()?;
                host.show(HWND(owner as _), choice, owned_epoch)
            })();
            let _ = tx.send(result);
        })?;
        let started = Instant::now();
        loop {
            if self.stop.load(Ordering::Acquire)
                || cancel.load(Ordering::Acquire)
                || started.elapsed() > Duration::from_secs(300)
            {
                let host = self.clone();
                let _ = main.run_on_main_thread(move || host.close_active(Some(&epoch)));
                anyhow::bail!(
                    "Jukebox file selection cancelled or timed out; no file was imported/exported"
                );
            }
            match rx.recv_timeout(Duration::from_millis(25)) {
                Ok(result) => {
                    ensure!(
                        !cancel.load(Ordering::Acquire),
                        "Jukebox file selection cancelled"
                    );
                    return result;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(e) => return Err(e.into()),
            }
        }
    }
}
impl Host for NativeHost {
    fn decode_admission(&self) -> Result<admission::Admission> {
        match self.decode_admission.get() {
            Some(check) => check(),
            None => Ok(admission::Admission::NativeOnly),
        }
    }
    fn choose_audio(&self, cancel: &AtomicBool) -> Result<Option<Vec<PathBuf>>> {
        self.ask(Choice::Audio, cancel)
    }
    fn choose_single_audio(&self, cancel: &AtomicBool) -> Result<Option<Vec<PathBuf>>> {
        self.ask(Choice::SingleAudio, cancel)
    }
    fn choose_artwork(&self, cancel: &AtomicBool) -> Result<Option<PathBuf>> {
        Ok(self.ask(Choice::Artwork, cancel)?.and_then(|mut p| p.pop()))
    }
    fn open_library_folder(&self, folder: &Path) -> Result<()> {
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        let _pins = crate::loader_files::Directories::pin(folder)?;
        ensure!(!self.stop.load(Ordering::Acquire), "Jukebox is closing");
        let action = "open".encode_utf16().chain([0]).collect::<Vec<_>>();
        let path = folder
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain([0])
            .collect::<Vec<_>>();
        let result = unsafe {
            ShellExecuteW(
                None,
                PCWSTR(action.as_ptr()),
                PCWSTR(path.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        ensure!(
            result.0 as usize > 32,
            "Unable to open the fixed Jukebox library folder"
        );
        Ok(())
    }
    fn choose_playlist(&self, cancel: &AtomicBool) -> Result<Option<PathBuf>> {
        Ok(self
            .ask(Choice::Playlist, cancel)?
            .and_then(|mut p| p.pop()))
    }
    fn choose_export(&self, name: &str, cancel: &AtomicBool) -> Result<Option<PathBuf>> {
        Ok(self
            .ask(Choice::Export(name.into()), cancel)?
            .and_then(|mut p| p.pop()))
    }
    fn preview_enabled(&self) -> bool {
        self.port.load(Ordering::Acquire) != 0 && !self.stop.load(Ordering::Acquire)
    }
    fn preview_url(&self, token: &str) -> Option<String> {
        let port = self.port.load(Ordering::Acquire);
        (port != 0).then(|| format!("http://127.0.0.1:{port}/api/jukebox/preview/{token}"))
    }
}
struct Ready {
    game: PathBuf,
    api: Arc<Api>,
    host: Arc<NativeHost>,
    server: Option<Server>,
}
pub(crate) struct Runtime {
    root: PathBuf,
    ready: Mutex<Option<Arc<Ready>>>,
    initialize: Mutex<()>,
    stop: AtomicBool,
    smoke: smoke::Runtime,
    decode_admission: Arc<OnceLock<admission::Check>>,
}
impl Runtime {
    pub fn new(root: PathBuf) -> Self {
        Self {
            smoke: smoke::Runtime::new(root.clone()),
            root,
            ready: Mutex::new(None),
            initialize: Mutex::new(()),
            stop: AtomicBool::new(false),
            decode_admission: Arc::new(OnceLock::new()),
        }
    }
    /// Inject exactly once after Backend is inside Arc. Capture Weak<Backend>,
    /// never Arc<Backend>, to avoid Runtime->Host->Backend ownership cycles.
    pub fn set_decode_admission(&self, check: admission::Check) -> Result<()> {
        self.decode_admission
            .set(check)
            .map_err(|_| anyhow::anyhow!("Jukebox decoder admission was already configured"))
    }
    /// Retain until BOTH kinds of Play have finished their Steam dispatch.
    /// No decoder may appear in the freeze-to-dispatch ownership interval.
    pub fn hold_launch_admission(&self) -> Result<admission::Guard> {
        let lease = admission::hold()?;
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Jukebox is shutting down"
        );
        if let Some(ready) = self
            .ready
            .lock()
            .map_err(|_| anyhow::anyhow!("Jukebox runtime unavailable"))?
            .as_ref()
        {
            ready.api.seal_launch()?;
        }
        Ok(lease)
    }
    fn ready(&self, game: &Path, app: tauri::AppHandle) -> Result<Arc<Ready>> {
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Jukebox is shutting down"
        );
        if let Some(r) = self
            .ready
            .lock()
            .map_err(|_| anyhow::anyhow!("Jukebox runtime unavailable"))?
            .as_ref()
        {
            ensure!(
                r.game == game,
                "Game folder changed; restart the launcher before editing Jukebox"
            );
            return Ok(r.clone());
        }
        let _init = self
            .initialize
            .lock()
            .map_err(|_| anyhow::anyhow!("Jukebox initialization unavailable"))?;
        if let Some(r) = self
            .ready
            .lock()
            .map_err(|_| anyhow::anyhow!("Jukebox runtime unavailable"))?
            .as_ref()
        {
            ensure!(
                r.game == game,
                "Game folder changed; restart the launcher before editing Jukebox"
            );
            return Ok(r.clone());
        }
        let stock = jukebox_v2_resources::read_stock(game, || {
            ensure!(
                !self.stop.load(Ordering::Acquire),
                "Jukebox initialization cancelled"
            );
            Ok(())
        })?;
        let host = Arc::new(NativeHost::new(app, self.decode_admission.clone()));
        let api = Arc::new(Api::open(
            &self.root,
            stock.music,
            Some(game.to_owned()),
            host.clone(),
        )?);
        let server = match Server::start(api.clone()) {
            Ok(server) => {
                host.port.store(server.port, Ordering::Release);
                Some(server)
            }
            Err(error) => {
                api.add_warning(&format!("Audio preview is unavailable because its owned local playback endpoint could not start: {error}. Playlist imports and library edits remain available."))?;
                None
            }
        };
        api.set_preview_available()?;
        let ready = Arc::new(Ready {
            game: game.to_owned(),
            api,
            host,
            server,
        });
        let mut state = self
            .ready
            .lock()
            .map_err(|_| anyhow::anyhow!("Jukebox runtime unavailable"))?;
        if self.stop.load(Ordering::Acquire) {
            ready.host.cancel();
            ready.api.cancel_pending();
            if let Some(server) = &ready.server {
                server.stop();
            }
            anyhow::bail!("Jukebox initialization cancelled");
        }
        *state = Some(ready.clone());
        Ok(ready)
    }
    pub fn dispatch(
        &self,
        channel: &str,
        args: Value,
        game: &Path,
        app: tauri::AppHandle,
        smoke: bool,
        native_status: &Value,
    ) -> Result<Value> {
        if smoke {
            return self.dispatch_smoke(channel, args);
        }
        let ready = self.ready(game, app)?;
        ready
            .api
            .sync_native_status(native_status, PROVIDER_JUKEBOX_INTEGRATED)?;
        ready.api.dispatch(channel, args)
    }
    pub fn selection_summary(&self) -> Result<Value> {
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Jukebox is shutting down"
        );
        let ready = self
            .ready
            .lock()
            .map_err(|_| anyhow::anyhow!("Jukebox runtime unavailable"))?
            .clone();
        let summary = if let Some(ready) = ready {
            crate::jukebox_launch::summarize_state(&ready.api.view()?.state)?
        } else {
            crate::jukebox_launch::selection_summary(&self.root)?
        };
        Ok(serde_json::to_value(summary)?)
    }
    pub fn select_for_play(&self, args: Value) -> Result<Value> {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Request {
            playlist_id: Option<String>,
            revision: String,
        }
        ensure!(
            args.as_object()
                .is_some_and(|map| map.contains_key("playlistId")),
            "Explicit playlist or null is required"
        );
        let request: Request = serde_json::from_value(args)?;
        // Serialize against first editor initialization: a cold disk update
        // must never leave a concurrently loaded API with an obsolete revision.
        let _init = self
            .initialize
            .lock()
            .map_err(|_| anyhow::anyhow!("Jukebox initialization unavailable"))?;
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Jukebox is shutting down"
        );
        let ready = self
            .ready
            .lock()
            .map_err(|_| anyhow::anyhow!("Jukebox runtime unavailable"))?
            .clone();
        let summary = if let Some(ready) = ready {
            ready.api.dispatch(
                "jukebox:set-active",
                serde_json::json!({"playlistId":request.playlist_id,"expectedRevision":request.revision}),
            )?;
            crate::jukebox_launch::summarize_state(&ready.api.view()?.state)?
        } else {
            crate::jukebox_launch::select_saved(
                &self.root,
                request.playlist_id.as_deref(),
                &request.revision,
            )?
        };
        Ok(serde_json::to_value(summary)?)
    }
    /// Only the trusted main window's --smoke branch calls this. It does not
    /// resolve settings, inspect native session status, or accept a game path.
    pub fn dispatch_smoke(&self, channel: &str, args: Value) -> Result<Value> {
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Jukebox is shutting down"
        );
        self.smoke.dispatch(channel, args)
    }
    /// Freeze real authoritative saved state without starting HTTP, a picker
    /// or a UI. A loaded API additionally refuses Play while a job is active.
    pub fn freeze_launch_plan(&self) -> Result<crate::jukebox_launch::Plan> {
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Jukebox is shutting down"
        );
        let ready = self
            .ready
            .lock()
            .map_err(|_| anyhow::anyhow!("Jukebox runtime unavailable"))?
            .clone();
        if let Some(ready) = ready {
            crate::jukebox_launch::Plan::freeze(&ready.api)
        } else {
            crate::jukebox_launch::freeze_saved(&self.root)
        }
    }
    pub fn cancel(&self) {
        self.stop.store(true, Ordering::Release);
        self.smoke.cancel();
        if let Ok(r) = self.ready.lock() {
            if let Some(r) = r.as_ref() {
                r.host.cancel();
                r.api.cancel_pending();
                if let Some(server) = &r.server {
                    server.stop();
                }
            }
        }
    }
    pub fn shutdown(&self) {
        self.cancel();
        self.smoke.shutdown();
        let ready = self.ready.lock().ok().and_then(|r| r.clone());
        if let Some(r) = ready {
            let _ = r.api.shutdown(Duration::from_secs(1));
            if let Some(server) = &r.server {
                server.shutdown();
            }
        }
    }
}
