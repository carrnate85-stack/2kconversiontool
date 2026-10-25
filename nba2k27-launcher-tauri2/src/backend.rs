use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    io::Read,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};
use windows::{
    Win32::Storage::FileSystem::{MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW},
    core::PCWSTR,
};
static NONCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[path = "backend_launch.rs"]
mod launch;
pub fn unique_id() -> String {
    format!(
        "{:032x}{:016x}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        NONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

// Independent of wall-clock resolution: concurrent Random All calls must not
// share a stream merely because Windows reports the same timestamp.
static OUTFIT_RANDOM_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
struct OutfitRandom(u64);
impl OutfitRandom {
    fn new() -> Result<Self> {
        use windows::Win32::Security::Cryptography::{
            BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
        };
        let mut entropy = [0u8; 8];
        unsafe { BCryptGenRandom(None, &mut entropy, BCRYPT_USE_SYSTEM_PREFERRED_RNG).ok() }
            .context("Could not generate outfit randomness")?;
        let sequence = OUTFIT_RANDOM_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(Self::from_entropy(u64::from_le_bytes(entropy), sequence))
    }
    fn from_entropy(entropy: u64, sequence: u64) -> Self {
        Self(entropy.wrapping_add(sequence.wrapping_mul(0x9e3779b97f4a7c15)))
    }
    fn word(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
        value ^ (value >> 31)
    }
    fn index(&mut self, count: usize) -> usize {
        assert!(count > 0);
        let bound = count as u64;
        let threshold = bound.wrapping_neg() % bound;
        loop {
            let value = self.word();
            if value >= threshold {
                return (value % bound) as usize;
            }
        }
    }
}

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Settings {
    pub game_root: String,
    pub mods_root: String,
    pub profile: String,
    pub auto_show_overlay: bool,
    pub auto_hide_overlay: bool,
    pub remove_drone: bool,
    pub remove_dj: bool,
    pub remove_mc: bool,
    pub remove_crowd: bool,
    pub lock_camera: bool,
    pub cameras_enabled: bool,
    pub startup_video: bool,
    pub camera: String,
    pub arena: String,
    pub ball: String,
    pub scoreboard: String,
    pub theme: String,
    pub video: String,
    pub music: String,
    pub unlock_wnba: bool,
    pub unlock_wnba_freestyle: bool,
    /// Experimental V2 route (off by default; never migrated from WPF): serve a
    /// locally built mixed NBA+WNBA Learn2K roster and apply WPF's Freestyle
    /// category write once that roster is loaded (wnba_roster_v2.rs).
    pub wnba_freestyle_mixed: bool,
    /// "wpf-era" (default), "national" or "clubs".
    pub wnba_freestyle_variant: String,
    pub scaler: f64,
    pub scaler_coaches: bool,
    pub scaler_cheerleaders: bool,
    pub scaler_arena_personnel: bool,
    /// WPF autoRandomSelection parity: random outfits for new lineup players at
    /// Blacktop selection, after the per-game outfit acknowledgement.
    pub auto_random_outfits: bool,
    /// WPF usePlaylistInFreestyle (core\paths.js default true): the imported
    /// Jukebox playlist also replaces Freestyle's event-992 music.
    pub use_playlist_in_freestyle: bool,
    /// Catalog ids behind the saved arena/scoreboard names (names can repeat).
    /// Empty in older settings: the launch plan resolves a unique name.
    pub arena_id: String,
    pub scoreboard_id: String,
}
impl Default for Settings {
    fn default() -> Self {
        let game = r"C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27".to_string();
        Self {
            mods_root: format!("{game}\\mods"),
            game_root: game,
            profile: "Default mods".into(),
            auto_show_overlay: true,
            auto_hide_overlay: true,
            remove_drone: false,
            remove_dj: false,
            remove_mc: false,
            remove_crowd: false,
            lock_camera: false,
            cameras_enabled: false,
            startup_video: false,
            camera: "Broadcast Low 2K18".into(),
            arena: "Default · Day / Night".into(),
            ball: "Currently installed".into(),
            scoreboard: "Default".into(),
            theme: "dark".into(),
            video: String::new(),
            music: String::new(),
            unlock_wnba: false,
            unlock_wnba_freestyle: false,
            wnba_freestyle_mixed: false,
            wnba_freestyle_variant: "wpf-era".into(),
            scaler: 1.,
            // WPF defaults: every character group follows the saved scale.
            scaler_coaches: true,
            scaler_cheerleaders: true,
            scaler_arena_personnel: true,
            auto_random_outfits: true,
            use_playlist_in_freestyle: true,
            arena_id: String::new(),
            scoreboard_id: String::new(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Outfit {
    pub name: String,
    pub top: String,
    pub bottom: String,
    pub shoes: String,
    #[serde(default)]
    pub socks: String,
    #[serde(default = "default_pool")]
    pub pool: String,
    pub accessories: Vec<String>,
    #[serde(default)]
    pub street_id: String,
    #[serde(default)]
    pub layer: String,
}
fn default_pool() -> String {
    "male".into()
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ModFile {
    name: String,
    path: String,
    bytes: u64,
    folder: String,
}

pub struct Backend {
    pub root: PathBuf,
    settings: Mutex<Settings>,
    settings_store: Mutex<crate::settings_compat::Store>,
    settings_import: crate::settings_compat::Report,
    pub(crate) closing: std::sync::atomic::AtomicBool,
    videos: Mutex<Option<crate::video_library::Store>>,
    video_import: crate::startup_video_import::Coordinator,
    decode_leases: Mutex<Vec<crate::legacy_saved_media::ResolvedVideo>>,
    library: Mutex<crate::library::Library>,
    pub previews: crate::previews::Previews,
    pub curated: crate::curated_library::Service,
    catalog: OnceLock<Value>,
    catalog_init: Mutex<()>,
    catalog_browser_id: OnceLock<String>,
    wardrobe: OnceLock<Value>,
    wardrobe_init: Mutex<()>,
    pub session: crate::native_session::Service,
    pub reference: crate::reference_stats::Service,
    pub roster: crate::roster_native::Service,
    pub presets: crate::editor_presets::Service,
    pub arena_profiles: crate::custom_arena_profiles::Service,
    pub jukebox: crate::jukebox_runtime::Runtime,
    pub mod_install: crate::mod_install::Installer,
    #[cfg(feature = "dev-model-preview")]
    pub model_preview: crate::model_preview_dev::Service,
}
fn wide(path: &Path) -> Result<Vec<u16>> {
    // Rust's filesystem operations support extended Windows paths, but the raw
    // MoveFileExW API needs the verbatim prefix too. The target need not exist.
    let path = std::path::absolute(path)?;
    let parent = fs::canonicalize(path.parent().context("File parent missing")?)?;
    let native = parent.join(path.file_name().context("Filename missing")?);
    Ok(native.as_os_str().encode_wide().chain(Some(0)).collect())
}
pub fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    atomic_json_mode(path, value, true)
}
pub(crate) fn atomic_json_new(path: &Path, value: &impl Serialize) -> Result<()> {
    atomic_json_mode(path, value, false)
}
fn atomic_json_mode(path: &Path, value: &impl Serialize, replace: bool) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    let temp = path.with_file_name(format!(".write-{}-{}.tmp", std::process::id(), unique_id()));
    let result = (|| -> Result<()> {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        use std::io::Write;
        f.write_all(&bytes)?;
        f.sync_all()?;
        drop(f);
        let from = wide(&temp)?;
        let to = wide(path)?;
        let flags = if replace {
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH
        } else {
            MOVEFILE_WRITE_THROUGH
        };
        let rename = || unsafe { MoveFileExW(PCWSTR(from.as_ptr()), PCWSTR(to.as_ptr()), flags) };
        if replace {
            replace_with_retry(rename, |attempt| {
                std::thread::sleep(std::time::Duration::from_millis(10 * u64::from(attempt)))
            })
        } else {
            rename()
        }
        .with_context(|| {
            format!(
                "Replacing {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            )
        })?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}
/// A reader (the launcher UI, an indexer or antivirus) can hold the target
/// open without FILE_SHARE_DELETE for a moment, which makes the replacing
/// rename fail with access denied or a sharing violation. Bounded retry; any
/// other error, and the last transient one, is returned unchanged.
const REPLACE_ATTEMPTS: u32 = 8;
fn replace_with_retry(
    mut rename: impl FnMut() -> windows::core::Result<()>,
    mut pause: impl FnMut(u32),
) -> windows::core::Result<()> {
    use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_SHARING_VIOLATION};
    let mut attempt = 1;
    loop {
        match rename() {
            Err(e)
                if attempt < REPLACE_ATTEMPTS
                    && (e.code() == ERROR_ACCESS_DENIED.to_hresult()
                        || e.code() == ERROR_SHARING_VIOLATION.to_hresult()) =>
            {
                pause(attempt);
                attempt += 1;
            }
            other => return other,
        }
    }
}
fn read_json<T: serde::de::DeserializeOwned>(path: &Path, limit: u64) -> Result<T> {
    let file = fs::File::open(path)?;
    let meta = file.metadata()?;
    ensure!(
        meta.is_file() && meta.len() <= limit,
        "Saved data exceeds its size limit: {}",
        path.display()
    );
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "Saved data changed or exceeds its size limit"
    );
    Ok(serde_json::from_slice(&bytes)?)
}
impl Backend {
    pub fn new(root: PathBuf) -> Result<Self> {
        for dir in [
            "Data",
            "Library/Music",
            "Library/Videos",
            "Cache/WebView2",
            "Cache/Imports",
            "Logs",
        ] {
            fs::create_dir_all(root.join(dir))
                .with_context(|| format!("Prototype folder is not writable: {}", root.display()))?;
        }
        let (settings_store, mut settings, settings_import) =
            crate::settings_compat::Store::open(&root, validate_settings)?;
        // WPF requireExplicitBallSelection: a saved ball is never carried into
        // a new launcher run; choose it again at Blacktop player selection.
        settings.ball = Settings::default().ball;
        // The native proxy uses ordinary-file presence as a portable-layout
        // marker; it does NOT parse this old WPF settings file. Never overwrite
        // it. Rust preferences are authoritative only in Data/Rust/settings.
        let path = root.join("Data/settings.json");
        if !path.try_exists()? {
            atomic_json_new(
                &path,
                &json!({"schema":"nba2k27-portable-layout-v1","settings":"Data/Rust/settings.json"}),
            )?;
        }
        {
            use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
            let marker = fs::OpenOptions::new()
                .read(true)
                .share_mode(1)
                .custom_flags(0x00200000)
                .open(&path)?;
            let meta = marker.metadata()?;
            ensure!(
                meta.is_file() && meta.file_attributes() & 0x400 == 0,
                "Portable layout marker is not an ordinary file; original left untouched"
            );
        }
        let library_path = root.join("Data/library.json");
        let mut library: crate::library::Library = if library_path.exists() {
            read_json(&library_path, 16 * 1024 * 1024)
                .context("Prototype library is invalid; keep this file for recovery")?
        } else {
            crate::library::Library::default()
        };
        if !root.join("Data/library.json").exists() {
            library.profiles = vec![crate::library::Profile {
                name: settings.profile.clone(),
                folder: settings.mods_root.clone(),
            }];
        }
        library
            .validate(&root)
            .context("Prototype library is invalid; original file left untouched")?;
        let previews = crate::previews::Previews::new(&root)?;
        let _ = crate::custom_blacktop::load(&root);
        let session = crate::native_session::Service::new(root.clone())?;
        let presets = crate::editor_presets::Service::new(&root);
        let jukebox = crate::jukebox_runtime::Runtime::new(root.clone());
        let mod_install = crate::mod_install::Installer::new(&root);
        #[cfg(feature = "dev-model-preview")]
        let model_preview = crate::model_preview_dev::Service::new(&root);
        Ok(Self {
            root,
            presets,
            arena_profiles: crate::custom_arena_profiles::Service::default(),
            jukebox,
            settings: Mutex::new(settings),
            settings_store: Mutex::new(settings_store),
            settings_import,
            closing: std::sync::atomic::AtomicBool::new(false),
            videos: Mutex::new(None),
            video_import: crate::startup_video_import::Coordinator::default(),
            curated: crate::curated_library::Service::default(),
            decode_leases: Mutex::new(Vec::new()),
            library: Mutex::new(library),
            previews,
            catalog: OnceLock::new(),
            catalog_init: Mutex::new(()),
            catalog_browser_id: OnceLock::new(),
            wardrobe: OnceLock::new(),
            wardrobe_init: Mutex::new(()),
            session,
            reference: crate::reference_stats::Service::default(),
            roster: crate::roster_native::Service::new(),
            mod_install,
            #[cfg(feature = "dev-model-preview")]
            model_preview,
        })
    }
    fn catalog(&self) -> Result<&Value> {
        if let Some(value) = self.catalog.get() {
            return Ok(value);
        }
        let _init = self
            .catalog_init
            .lock()
            .map_err(|_| anyhow::anyhow!("Player catalog initialization failed"))?;
        if let Some(value) = self.catalog.get() {
            return Ok(value);
        }
        let mut bytes = Vec::new();
        flate2::read::GzDecoder::new(&include_bytes!("../assets/players.json.gz")[..])
            .read_to_end(&mut bytes)?;
        let value = serde_json::from_slice(&bytes)?;
        let _ = self.catalog.set(value);
        Ok(self.catalog.get().unwrap())
    }
    pub fn browser_catalog(&self, request: &Value) -> Result<Value> {
        use sha2::{Digest, Sha256};
        let catalog = self.catalog()?;
        // Immutable compiled source, hashed once instead of serializing the
        // entire multi-megabyte catalog for every browser click/search.
        let id = self.catalog_browser_id.get_or_init(|| {
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../assets/players.json.gz"))
            )
        });
        crate::player_catalog::browser_request_with_id(catalog, request, id)
    }
    fn wardrobe(&self) -> Result<&Value> {
        if let Some(value) = self.wardrobe.get() {
            return Ok(value);
        }
        let _init = self
            .wardrobe_init
            .lock()
            .map_err(|_| anyhow::anyhow!("Wardrobe initialization failed"))?;
        if let Some(value) = self.wardrobe.get() {
            return Ok(value);
        }
        let mut bytes = Vec::new();
        flate2::read::GzDecoder::new(&include_bytes!("../assets/wardrobe.json.gz")[..])
            .read_to_end(&mut bytes)?;
        let mut value: Value = serde_json::from_slice(&bytes)?;
        for row in value.as_array_mut().context("Wardrobe missing")? {
            crate::shoe_labels::enrich(row);
        }
        let _ = self.wardrobe.set(value);
        Ok(self.wardrobe.get().unwrap())
    }
    pub fn dispatch(&self, channel: &str, args: Value) -> Result<Value> {
        match channel {
            "native-rules-read" => {
                let game = self
                    .settings
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Settings unavailable"))?
                    .game_root
                    .clone();
                ensure!(
                    args == json!({"acknowledgement":"read-only-rules","gameRoot":game}),
                    "Confirm reading rules for the current game folder"
                );
                self.session.read_rules(Path::new(&game))
            }
            "native-rules-apply" => {
                #[derive(serde::Deserialize)]
                #[serde(rename_all = "camelCase", deny_unknown_fields)]
                struct Request {
                    acknowledgement: String,
                    game_root: String,
                    token: String,
                    field: String,
                    value: u32,
                }
                let r: Request = serde_json::from_value(args)?;
                let game = self
                    .settings
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Settings unavailable"))?
                    .game_root
                    .clone();
                ensure!(
                    r.acknowledgement == "apply-one-rule"
                        && r.game_root == game
                        && r.token.len() == 48
                        && r.field.len() <= 50,
                    "Explicit rule acknowledgement, snapshot or game folder invalid"
                );
                self.session
                    .apply_rule(Path::new(&game), &r.token, &r.field, r.value)
            }
            "native-outfit-command" => {
                #[derive(serde::Deserialize)]
                #[serde(rename_all = "camelCase", deny_unknown_fields)]
                struct Request {
                    acknowledgement: String,
                    game_root: String,
                    session: String,
                    command: crate::outfit_runtime::Command,
                }
                let r: Request = serde_json::from_value(args)?;
                let game = self.current_settings()?.game_root;
                ensure!(
                    r.game_root == game
                        && r.session.len() == 48
                        && r.acknowledgement
                            == if r.command.writes() {
                                "apply-one-outfit-for-current-game"
                            } else {
                                "read-blacktop-outfits"
                            },
                    "Confirm the outfit operation for this launched game"
                );
                self.session
                    .outfit_command(Path::new(&game), &r.session, r.command)
            }
            "native-arena-select" | "native-ball-select" | "native-scoreboard-select" => {
                #[derive(serde::Deserialize)]
                #[serde(rename_all = "camelCase", deny_unknown_fields)]
                struct Request {
                    acknowledgement: String,
                    game_root: String,
                    session: String,
                    asset_id: String,
                }
                let r: Request = serde_json::from_value(args)?;
                let game = self.current_settings()?.game_root;
                ensure!(
                    r.acknowledgement
                        == if channel == "native-arena-select" {
                            "apply-one-arena-for-current-game"
                        } else if channel == "native-ball-select" {
                            "apply-one-ball-for-current-game"
                        } else {
                            "apply-one-scoreboard-for-current-game"
                        }
                        && r.game_root == game
                        && r.session.len() == 48,
                    "Confirm a catalog change for the current launched game"
                );
                if channel == "native-arena-select" {
                    self.session
                        .select_arena(Path::new(&game), &r.session, &r.asset_id)
                } else if channel == "native-ball-select" {
                    self.session
                        .select_ball(Path::new(&game), &r.session, &r.asset_id)
                } else {
                    self.session
                        .select_scoreboard(Path::new(&game), &r.session, &r.asset_id)
                }
            }
            "native-camera-command" => {
                #[derive(serde::Deserialize)]
                #[serde(rename_all = "camelCase", deny_unknown_fields)]
                struct Request {
                    acknowledgement: String,
                    game_root: String,
                    session: String,
                    command: crate::feature_runtime::CameraCommand,
                }
                let r: Request = serde_json::from_value(args)?;
                let game = self.current_settings()?.game_root;
                ensure!(
                    r.acknowledgement == "control-cameras-for-current-game"
                        && r.game_root == game
                        && r.session.len() == 48,
                    "Confirm the camera action for the current launched game"
                );
                self.session
                    .camera_command(Path::new(&game), &r.session, r.command)
            }
            "native-session-status" => self.session.status(),
            "native-session-clear" => self.session.clear(),
            "native-blacktop-status" => self.session.blacktop_status(),
            "native-blacktop-enable" => {
                let game = self
                    .settings
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Settings unavailable"))?
                    .game_root
                    .clone();
                ensure!(
                    args == json!({"acknowledgement":"read-only-blacktop","gameRoot":game}),
                    "Read-only observer acknowledgement or game folder changed"
                );
                self.session.enable_blacktop(Path::new(&game))
            }
            "native-blacktop-disable" => self.session.disable_blacktop(),
            "native-launch-steam-only" | "native-launch-modded" => {
                crate::launcher_update::assert_idle_for_launch()?;
                self.launch_request(channel, args).map(|(value, _)| value)
            }
            "launch-preflight" => {
                let game = self
                    .settings
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Settings unavailable"))?
                    .game_root
                    .clone();
                Ok(crate::session_safety::preflight(
                    Path::new(&game),
                    &self.root,
                ))
            }
            "bootstrap" => {
                let s = self
                    .settings
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Settings unavailable"))?
                    .clone();
                let game = Path::new(&s.game_root).join("NBA2K27.exe");
                Ok(
                    json!({"version":VERSION,"settings":s,"settingsImport":self.settings_import,"root":self.root,"gameFound":game.is_file(),"runtime":"Rust + Tauri 2 + shared WebView2","liveGameEnabled":false,"capabilities":{"wnbaBlacktop":crate::feature_plan::dev_wnba_categories(),"debugMenus":crate::feature_plan::dev_debug_menus(),"developerTools":cfg!(feature="dev-curator"),"modelPreview":cfg!(feature="dev-model-preview"),"customBlacktop":crate::custom_blacktop::enabled()}}),
                )
            }
            #[cfg(feature = "dev-model-preview")]
            "model-preview-prepare" => {
                let settings = self.current_settings()?;
                self.model_preview.prepare(Path::new(&settings.game_root))
            }
            #[cfg(feature = "dev-model-preview")]
            "model-preview-load" => {
                let path = args["path"].as_str().context("Mod path required")?;
                let settings = self.current_settings()?;
                self.model_preview.load(
                    Path::new(&settings.game_root),
                    Path::new(&settings.mods_root),
                    path,
                    args.get("config").and_then(Value::as_str),
                )
            }
            #[cfg(feature = "dev-model-preview")]
            "model-preview-arena-textures" => {
                let session = args["session"]
                    .as_str()
                    .context("Preview session required")?;
                let cursor = args["cursor"].as_u64().unwrap_or(0) as usize;
                self.model_preview.arena_textures(session, cursor)
            }
            #[cfg(feature = "dev-model-preview")]
            "model-preview-cancel" => self.model_preview.cancel(),
            #[cfg(feature = "dev-model-preview")]
            "model-preview-status" => self.model_preview.status(),
            "support-bundle-create" => {
                ensure!(
                    args.is_null() || args == json!({}),
                    "Support bundle takes no arguments"
                );
                let settings = serde_json::to_value(self.current_settings()?)?;
                crate::support_bundle::create(&self.root, settings, VERSION)
            }
            "save-settings" => {
                let mut current = self
                    .settings
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Settings unavailable"))?;
                let patch = args
                    .as_object()
                    .context("Settings patch must be an object")?;
                let mut merged = serde_json::to_value(&*current)?;
                merged.as_object_mut().unwrap().extend(patch.clone());
                let next: Settings = serde_json::from_value(merged)?;
                validate_settings(&next)?;
                self.settings_store
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Settings storage unavailable"))?
                    .save(&next)?;
                *current = next;
                Ok(serde_json::to_value(&*current)?)
            }
            "mods" => {
                let root = self
                    .settings
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Settings unavailable"))?
                    .mods_root
                    .clone();
                scan_mods(Path::new(&root))
            }
            "media" => self.media(),
            "video-library-edit" => self.edit_video(&args),
            "arena-favorites" => self.arena_favorites(None),
            "arena-favorite" => {
                let id = args["id"].as_str().context("Arena id required")?;
                let favorite = args["favorite"]
                    .as_bool()
                    .context("Favorite flag required")?;
                self.arena_favorites(Some((id, favorite)))
            }
            "library" => Ok(json!(
                self.library
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Library unavailable"))?
                    .clone()
            )),
            "save-library" => {
                let next: crate::library::Library = serde_json::from_value(args)?;
                next.validate(&self.root)?;
                let mut current = self
                    .library
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Library unavailable"))?;
                atomic_json(&self.root.join("Data/library.json"), &next)?;
                *current = next;
                Ok(json!(true))
            }
            "preview-status" => self.previews.status(),
            "custom-blacktop" => {
                let settings = self.current_settings()?;
                crate::custom_blacktop::request(&self.root, Path::new(&settings.mods_root), &args)
            }
            "choices" => {
                let kind = args["kind"].as_str().context("Catalog kind required")?;
                ensure!(
                    matches!(kind, "ball" | "scoreboard" | "arena"),
                    "Invalid catalog"
                );
                let mut bytes = Vec::new();
                flate2::read::GzDecoder::new(&include_bytes!("../assets/choices.json.gz")[..])
                    .read_to_end(&mut bytes)?;
                let mut choices: Value = serde_json::from_slice(&bytes)?;
                if kind == "arena" && crate::custom_blacktop::enabled() {
                    let settings = self.current_settings()?;
                    choices["arena"].as_array_mut().context("Arena catalog missing")?
                        .extend(crate::custom_blacktop::rows(&self.root, Path::new(&settings.mods_root))?);
                }
                let q = args["query"].as_str().unwrap_or("");
                ensure!(q.len() <= 200, "Search is too long");
                // WPF arena-browser.js / catalog search: every normalised term
                // must appear in the label, id, file, floor or group.
                let terms: Vec<String> = catalog_search_text(q)
                    .split(' ')
                    .filter(|t| !t.is_empty())
                    .map(str::to_owned)
                    .collect();
                let group = args["group"].as_str().unwrap_or("All");
                let rows = choices[kind].as_array().context("Catalog missing")?;
                let mut groups: std::collections::BTreeSet<_> =
                    rows.iter().filter_map(|r| r["group"].as_str()).collect();
                if kind == "arena" && crate::custom_blacktop::enabled() { groups.insert("Custom"); }
                // WPF arena picker: a Favorites tab beside the catalog groups.
                let favorites = if kind == "arena" && group == "Favorites" {
                    Some(self.arena_favorites(None)?)
                } else {
                    None
                };
                let mut matches: Vec<_> = rows
                    .iter()
                    .filter(|r| {
                        (group == "All"
                            || r["group"] == group
                            || favorites
                                .as_ref()
                                .and_then(Value::as_array)
                                .is_some_and(|ids| ids.contains(&r["id"])))
                            && {
                                let text = catalog_search_text(
                                    &["name", "id", "file", "floorFile", "group"]
                                        .map(|k| r[k].as_str().unwrap_or(""))
                                        .join(" "),
                                );
                                terms.iter().all(|t| text.contains(t.as_str()))
                            }
                    })
                    .collect();
                // All-tab browsing: keep the stock court first, then outdoor
                // courts and gyms. Stable sorting preserves the catalog order
                // within each group and across the remaining arenas. Sort the
                // complete result before paging so later pages follow it too.
                if kind == "arena" && group == "All" {
                    matches.sort_by_key(|row| {
                        if row["id"] == "default" {
                            0
                        } else {
                            match row["group"].as_str() {
                                Some("Outdoor") => 1,
                                Some("Gyms") => 2,
                                _ => 3,
                            }
                        }
                    });
                }
                let offset = args["offset"].as_u64().unwrap_or(0) as usize;
                let mut page: Vec<Value> = matches
                    .iter()
                    .skip(offset)
                    .take(24)
                    .map(|r| (*r).clone())
                    .collect();
                for row in &mut page {
                    if let Some(key) = row["previewKey"].as_str() {
                        if let Some(path) = self.previews.image(key)? {
                            row["imagePath"] = json!(path)
                        }
                    }
                }
                Ok(json!({"items":page,"total":matches.len(),"groups":groups}))
            }
            "garments" => {
                let ids = args["ids"].as_array().context("Asset IDs required")?;
                ensure!(ids.len() <= 40, "Too many asset IDs");
                let items = self.wardrobe()?.as_array().context("Wardrobe missing")?;
                let mut rows = Vec::new();
                for id in ids {
                    if let Some(p) = items.iter().find(|p| p["asset"] == *id) {
                        let mut p = p.clone();
                        if let Some(key) = p["previewKey"].as_str() {
                            if let Some(path) = self.previews.image(key)? {
                                p["imagePath"] = json!(path)
                            }
                        }
                        rows.push(p);
                    }
                }
                Ok(json!(rows))
            }
            "players" => crate::player_catalog::search(self.catalog()?, &args),
            "player" => {
                let c = self.catalog()?;
                let id = args["id"].as_u64().context("Player ID required")?;
                let player = c["players"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|p| p["id"].as_u64() == Some(id))
                    .context("Player not found")?;
                let history = player["history"].as_array().context("History missing")?;
                let records: Vec<_> = c["records"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|r| history.contains(&r["id"]))
                    .collect();
                Ok(
                    json!({"player":player,"records":records,"teams":c["teams"],"fieldKeys":c["fieldKeys"],"unsupported":c["unsupported"]}),
                )
            }
            "player-dna-schema" => Ok(crate::player_dna::schema().clone()),
            "season-math-change" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Request {
                    totals: Value,
                    key: String,
                    value: f64,
                }
                let r: Request = serde_json::from_value(args)?;
                ensure!(r.key.len() <= 40, "Invalid season field");
                let totals = crate::season_math::change(&r.totals, &r.key, r.value)?;
                Ok(json!({"display":crate::season_math::display(&totals)?,"totals":totals}))
            }
            "team-dna-schema" => Ok(crate::team_dna::schema().clone()),
            "team-dna-validate" => crate::team_dna::validate(&args),
            "team-dna-subset" => crate::team_dna::subset(&args["document"], &args["sections"]),
            "team-dna-stage" => {
                crate::team_dna::stage(&args["destination"], &args["source"], &args["sections"])
            }
            "save-team-dna" => {
                let document = crate::team_dna::validate(&args)?;
                atomic_json(&self.root.join("Data/team-dna.json"), &document)?;
                Ok(document)
            }
            "team-dna" => {
                let file = self.root.join("Data/team-dna.json");
                if file.exists() {
                    crate::team_dna::validate(&read_json(&file, 512 * 1024)?)
                } else {
                    Ok(json!(null))
                }
            }
            "export-team-dna" => {
                let document = crate::team_dna::validate(&args)?;
                let directory = self.root.join("Library/Teams");
                fs::create_dir_all(&directory)?;
                let destination = directory.join(format!("Team-{}.2kteam", unique_id()));
                atomic_json(&destination, &document)?;
                Ok(json!({"path":destination}))
            }
            "player-dna-validate" => crate::player_dna::validate(&args),
            "player-dna-subset" => crate::player_dna::subset(&args["document"], &args["groups"]),
            "player-dna-stage-seasons" => {
                crate::player_dna::stage_catalog_seasons(&args["document"], self.catalog()?, &args)
            }
            "reference-stats-load" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Request {
                    url: String,
                    acknowledgement: String,
                }
                let r: Request = serde_json::from_value(args)?;
                ensure!(
                    r.acknowledgement == "fetch-public-player-page",
                    "Confirm downloading the public player page."
                );
                self.reference
                    .load(&r.url, &crate::reference_stats::WinHttp)
            }
            "reference-stats-stage" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Request {
                    document: Value,
                    token: String,
                    selected: Value,
                }
                let r: Request = serde_json::from_value(args)?;
                crate::reference_stats::stage(
                    &r.document,
                    &self.reference.snapshot(&r.token)?,
                    self.catalog()?,
                    &r.selected,
                )
            }
            "season-teams" => Ok(json!(
                self.catalog()?["teams"]
                    .as_array()
                    .context("Teams missing")?
                    .iter()
                    .filter(|t| t["category"] == "current")
                    .collect::<Vec<_>>()
            )),
            "catalog-player-dna" | "catalog-player-review" => {
                let c = self.catalog()?;
                let p = c["players"]
                    .as_array()
                    .context("Players missing")?
                    .iter()
                    .find(|p| p["id"] == args["id"])
                    .context("Player not found")?;
                let document =
                    crate::player_dna::from_catalog(p, &c["fieldKeys"], &c["targetSchemaId"])?;
                if channel == "catalog-player-review" {
                    Ok(
                        json!({"document":document,"unsupported":c["unsupported"],"sourceId":p["id"]}),
                    )
                } else {
                    Ok(document)
                }
            }
            "save-player-dna" => {
                let document = crate::player_dna::validate(&args)?;
                atomic_json(&self.root.join("Data/player-dna.json"), &document)?;
                Ok(document)
            }
            "player-dna" => {
                let p = self.root.join("Data/player-dna.json");
                if p.exists() {
                    crate::player_dna::validate(&read_json(&p, 256 * 1024)?)
                } else {
                    Ok(json!(null))
                }
            }
            "export-player-dna" => {
                let document = crate::player_dna::validate(&args)?;
                let directory = self.root.join("Library/Players");
                fs::create_dir_all(&directory)?;
                let destination = directory.join(format!("Player-{}.2kplayer", unique_id()));
                atomic_json(&destination, &document)?;
                Ok(json!({"path":destination}))
            }
            "wardrobe" => {
                let c = self.wardrobe()?;
                let slot = args["slot"].as_str().unwrap_or("top");
                let q = args["query"].as_str().unwrap_or("").to_lowercase();
                ensure!(q.len() <= 200, "Search is too long");
                let pool = args["pool"].as_str();
                let shirtless = json!({"asset":"Shirtless","category":"top","group":"Shirtless","pool":pool.unwrap_or("male")});
                let group = args["group"].as_str().unwrap_or("All");
                let items: Vec<_> = std::iter::once(&shirtless)
                    .filter(|_| slot == "top" && pool != Some("female"))
                    .chain(c.as_array().context("Wardrobe missing")?.iter())
                    .filter(|p| {
                        p["asset"] != "NO SHIRT"
                            && p["category"].as_str() == Some(slot)
                            && pool.is_none_or(|pool| garment_pool(p, pool))
                            && if slot == "shoes" {
                                let text = format!(
                                    "{} {} {}",
                                    p["asset"].as_str().unwrap_or(""),
                                    p["label"].as_str().unwrap_or(""),
                                    p["group"].as_str().unwrap_or("")
                                )
                                .to_lowercase();
                                q.split_whitespace().all(|term| text.contains(term))
                            } else {
                                p["asset"]
                                    .as_str()
                                    .unwrap_or("")
                                    .to_lowercase()
                                    .contains(&q)
                            }
                    })
                    .collect();
                let mut groups: Vec<_> = items.iter().filter_map(|p| p["group"].as_str()).collect();
                groups.sort_unstable();
                groups.dedup();
                let mut items: Vec<_> = items
                    .into_iter()
                    .filter(|p| group == "All" || p["group"] == group)
                    .collect();
                if slot == "shoes" && group == "All" {
                    // Stable partition before pagination keeps every identified shoe first.
                    items.sort_by_key(|p| p["group"] == "Unidentified");
                }
                let offset = args["offset"].as_u64().unwrap_or(0) as usize;
                let limit = args["limit"].as_u64().unwrap_or(40).clamp(1, 80) as usize;
                let mut page: Vec<Value> = items
                    .iter()
                    .skip(offset)
                    .take(limit)
                    .map(|p| (*p).clone())
                    .collect();
                for p in &mut page {
                    if let Some(key) = p["previewKey"].as_str() {
                        if let Some(path) = self.previews.image(key)? {
                            p["imagePath"] = json!(path);
                        }
                    }
                }
                Ok(json!({"total":items.len(),"items":page,"groups":groups}))
            }
            "outfit-for-pool" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Pool {
                    pool: String,
                }
                let p: Pool = serde_json::from_value(args)?;
                ensure!(
                    matches!(p.pool.as_str(), "male" | "female"),
                    "Unknown outfit pool"
                );
                let c = crate::outfits::catalog()?;
                let o = c
                    .presets
                    .iter()
                    .filter(|v| {
                        v.pool.as_deref() == Some(p.pool.as_str())
                            && v.separable
                            && !v.manual_only
                            && !v.original.iter().any(|s| s.starts_with("UNIFORM"))
                    })
                    .find_map(|v| c.original_outfit(&v.id, "Outfit").ok())
                    .context("No compatible outfit template")?;
                Ok(json!(o))
            }
            "random-outfit" => {
                let mut o: Outfit = serde_json::from_value(args["outfit"].clone())?;
                validate_outfit(&o)?;
                let mode = args["mode"].as_str().unwrap_or("both");
                ensure!(
                    matches!(mode, "clothes" | "accessories" | "both"),
                    "Invalid random mode"
                );
                let mut random = OutfitRandom::new()?;
                let items: Vec<_> = self
                    .wardrobe()?
                    .as_array()
                    .context("Wardrobe missing")?
                    .iter()
                    .filter(|p| garment_pool(p, &o.pool) && p["asset"] != "NO SHIRT")
                    .collect();
                if mode != "accessories" {
                    for slot in ["top", "bottom", "socks"] {
                        let choices: Vec<_> =
                            items.iter().filter(|p| p["category"] == slot).collect();
                        if !choices.is_empty() {
                            let asset = choices[random.index(choices.len())]["asset"]
                                .as_str()
                                .unwrap()
                                .to_string();
                            match slot {
                                "top" => o.top = asset,
                                "bottom" => o.bottom = asset,
                                _ => o.socks = asset,
                            }
                        }
                    }
                }
                if mode != "clothes" {
                    o.accessories.clear();
                    let mut used = std::collections::HashSet::new();
                    let choices: Vec<_> = items
                        .iter()
                        .filter(|p| {
                            !matches!(
                                p["category"].as_str().unwrap_or("other"),
                                "top" | "bottom" | "shoes" | "socks" | "other"
                            )
                        })
                        .collect();
                    for _ in 0..40 {
                        if o.accessories.len() == 3 || choices.is_empty() {
                            break;
                        }
                        let p = choices[random.index(choices.len())];
                        if used.insert(p["category"].as_str().unwrap().to_string()) {
                            o.accessories.push(p["asset"].as_str().unwrap().to_string());
                        }
                    }
                }
                if mode != "accessories" {
                    crate::outfits::catalog()?.bind_draft(&mut o);
                }
                Ok(json!(o))
            }
            // Read-only: which wardrobe slot (and preview) each draft piece is,
            // so the F8 editor can place accessories in their WPF slot tiles.
            "outfit-pieces" => {
                let o: Outfit = serde_json::from_value(args["outfit"].clone())?;
                validate_outfit(&o)?;
                let rows = self.wardrobe()?.as_array().context("Wardrobe missing")?;
                let mut pieces = Vec::new();
                for asset in [&o.top, &o.bottom, &o.shoes, &o.socks]
                    .into_iter()
                    .chain(o.accessories.iter())
                    .filter(|a| !a.is_empty())
                {
                    let Some(row) = rows
                        .iter()
                        .find(|p| p["asset"].as_str() == Some(asset.as_str()))
                    else {
                        continue;
                    };
                    let mut row = row.clone();
                    if let Some(key) = row["previewKey"].as_str() {
                        if let Some(path) = self.previews.image(key)? {
                            row["imagePath"] = json!(path)
                        }
                    }
                    pieces.push(row);
                }
                Ok(json!(pieces))
            }
            "select-outfit-piece" => {
                let mut o: Outfit = serde_json::from_value(args["outfit"].clone())?;
                validate_outfit(&o)?;
                let slot = args["slot"].as_str().context("Clothing slot required")?;
                let asset = args["asset"].as_str().context("Clothing asset required")?;
                let rows = self.wardrobe()?.as_array().context("Wardrobe missing")?;
                let shirtless = slot == "top" && asset == "Shirtless" && o.pool == "male";
                ensure!(
                    shirtless
                        || rows.iter().any(|p| p["asset"] == asset
                            && p["category"] == slot
                            && garment_pool(p, &o.pool)),
                    "Asset does not match this clothing slot and fit"
                );
                match slot {
                    "top" => o.top = asset.into(),
                    "bottom" => o.bottom = asset.into(),
                    "shoes" => o.shoes = asset.into(),
                    "socks" => o.socks = asset.into(),
                    _ => {
                        o.accessories.retain(|old| {
                            !rows.iter().any(|p| {
                                p["asset"].as_str() == Some(old.as_str()) && p["category"] == slot
                            })
                        });
                        o.accessories.push(asset.into());
                    }
                }
                validate_outfit(&o)?;
                if slot == "top" {
                    crate::outfits::catalog()?.bind_draft(&mut o);
                }
                Ok(json!(o))
            }
            "save-outfit" => {
                let outfit: Outfit = serde_json::from_value(args)?;
                validate_outfit(&outfit)?;
                atomic_json(&self.root.join("Data/outfit.json"), &outfit)?;
                Ok(json!(true))
            }
            "outfit-variants" => Ok(crate::outfits::catalog()?.variants(
                args["top"].as_str().unwrap_or(""),
                args["pool"].as_str().unwrap_or("male"),
            )),
            "outfit-original" => Ok(json!(crate::outfits::catalog()?.original_outfit(
                args["id"].as_str().context("Variant required")?,
                args["name"].as_str().unwrap_or("My outfit")
            )?)),
            "outfit-inspect" => {
                let outfit: Outfit = serde_json::from_value(args)?;
                let c = crate::outfits::catalog()?;
                let s = c.selection(&outfit)?;
                Ok(json!({"items":c.compose(&s)?,"share":c.encode(&outfit.name,&s)?}))
            }
            "outfit-decode" => {
                let document = serde_json::to_vec(&args)?;
                Ok(json!(
                    crate::outfits::catalog()?.imported_outfit(&document)?
                ))
            }
            "export-outfit" => {
                let outfit: Outfit = serde_json::from_value(args)?;
                validate_outfit(&outfit)?;
                let c = crate::outfits::catalog()?;
                let document = c.encode(&outfit.name, &c.selection(&outfit)?)?;
                let directory = self.root.join("Library/Outfits");
                fs::create_dir_all(&directory)?;
                let filename = outfit
                    .name
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                    .take(48)
                    .collect::<String>();
                let destination = directory.join(format!(
                    "{}-{}.2koutfit",
                    if filename.is_empty() {
                        "Outfit"
                    } else {
                        &filename
                    },
                    unique_id()
                ));
                atomic_json(&destination, &document)?;
                Ok(json!({"path":destination}))
            }
            "outfit" => {
                let p = self.root.join("Data/outfit.json");
                Ok(if p.exists() {
                    let outfit: Outfit = read_json(&p, 64 * 1024)?;
                    validate_outfit(&outfit)?;
                    json!(outfit)
                } else {
                    json!(null)
                })
            }
            "save-draft" => {
                ensure!(
                    serde_json::to_vec(&args)?.len() < 2 * 1024 * 1024,
                    "Draft exceeds limit"
                );
                atomic_json(&self.root.join("Data/roster-draft.json"), &args)?;
                Ok(json!(true))
            }
            "draft" => {
                let p = self.root.join("Data/roster-draft.json");
                Ok(if p.exists() {
                    read_json(&p, 2 * 1024 * 1024)?
                } else {
                    json!(null)
                })
            }
            c if c.starts_with("launcher-tools-") => crate::launcher_tools::dispatch(self, c, args),
            _ => anyhow::bail!("Prototype command is not implemented: {channel}"),
        }
    }
    pub fn current_settings(&self) -> Result<Settings> {
        Ok(self
            .settings
            .lock()
            .map_err(|_| anyhow::anyhow!("Settings unavailable"))?
            .clone())
    }
    pub(crate) fn begin_shutdown(&self) {
        self.closing
            .store(true, std::sync::atomic::Ordering::Release);
        self.video_import.cancel_current();
        self.curated.shutdown();
        #[cfg(feature = "dev-curator")]
        crate::curated_publisher::shutdown();
        self.previews.shutdown();
        self.session.stop_new_launches();
    }
    pub(crate) fn ensure_open_for_launch(&self) -> Result<()> {
        ensure!(
            !self.closing.load(std::sync::atomic::Ordering::Acquire),
            "Launcher is closing; no new Play launch"
        );
        Ok(())
    }
    pub fn import_media(&self, source: &Path, kind: &str) -> Result<Value> {
        ensure!(matches!(kind, "Music" | "Videos"), "Unsupported library");
        if kind == "Videos" {
            let display = source
                .file_stem()
                .and_then(|v| v.to_str())
                .context("Video name is unsupported")?;
            self.with_videos(|store| {
                store.import_mp4(
                    source,
                    display,
                    &unique_id(),
                    &std::sync::atomic::AtomicBool::new(false),
                    |_| Ok(()),
                )
            })?;
            return self.media();
        }
        let extension = source
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        ensure!(
            if kind == "Videos" {
                extension == "mp4"
            } else {
                matches!(extension.as_str(), "mp3" | "wav" | "m4a" | "ogg")
            },
            "Select an MP4 video or a supported audio file"
        );
        let meta = fs::metadata(source)?;
        ensure!(
            meta.is_file() && meta.len() > 0 && meta.len() <= 2 * 1024 * 1024 * 1024,
            "File is empty or exceeds 2 GB"
        );
        let name = source.file_name().context("Filename missing")?;
        let destination = self.root.join("Library").join(kind).join(name);
        // Copy to Cache first. An interrupted copy is never published as a complete library item.
        ensure!(!destination.exists(), "This filename is already imported");
        let temp = self.root.join("Cache/Imports").join(format!(
            "{}-{}.importing",
            std::process::id(),
            unique_id()
        ));
        let result = (|| -> Result<()> {
            let mut out = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)?;
            let mut input = fs::File::open(source)?;
            let count = std::io::copy(&mut input, &mut out)?;
            ensure!(count == meta.len(), "Source changed during import");
            out.sync_all()?;
            drop(out);
            let from = wide(&temp)?;
            let to = wide(&destination)?;
            // No REPLACE_EXISTING: even a competing import cannot overwrite a completed item.
            unsafe {
                MoveFileExW(
                    PCWSTR(from.as_ptr()),
                    PCWSTR(to.as_ptr()),
                    MOVEFILE_WRITE_THROUGH,
                )?
            };
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result?;
        self.media()
    }

    pub(crate) fn import_curated_video(
        &self,
        source: &Path,
        display: &str,
        cancel: &std::sync::atomic::AtomicBool,
        progress: &dyn Fn(&str),
    ) -> Result<String> {
        self.ensure_open_for_launch()?;
        let id = unique_id();
        let lease = self.video_import.begin(&id)?;
        let result = self.with_videos(|store| {
            store.import_mp4(source, display, &unique_id(), &lease.cancel, |update| {
                if matches!(
                    update.phase,
                    crate::video_library::Phase::Committing | crate::video_library::Phase::Done
                ) {
                    lease.commit_started();
                } else if cancel.load(std::sync::atomic::Ordering::Acquire) {
                    lease
                        .cancel
                        .store(true, std::sync::atomic::Ordering::Release);
                }
                progress("Checking video and adding it to Presentation");
                Ok(())
            })
        });
        self.video_import.finish(&id);
        Ok(result?.id)
    }
    pub(crate) fn import_video_with_progress(
        &self,
        source: &Path,
        operation_id: &str,
        mut progress: impl FnMut(&str, crate::video_library::Progress) -> Result<()>,
    ) -> Result<Value> {
        ensure!(
            !self.closing.load(std::sync::atomic::Ordering::Acquire),
            "Launcher is closing; no new startup-video import"
        );
        let display = source
            .file_stem()
            .and_then(|value| value.to_str())
            .context("Video name is unsupported")?;
        let lease = self.video_import.begin(operation_id)?;
        let result = self.with_videos(|store| {
            store.import_mp4(source, display, &unique_id(), &lease.cancel, |update| {
                if matches!(
                    update.phase,
                    crate::video_library::Phase::Committing | crate::video_library::Phase::Done
                ) {
                    lease.commit_started();
                }
                progress(operation_id, update)
            })
        });
        self.video_import.finish(operation_id);
        result?;
        self.media()
    }

    /// WPF presentation:library-edit parity for rename/remove. Removing the
    /// session's selected video also clears that selection so Play never
    /// holds on a missing ID.
    /// WPF arena-favorites.json: exact catalog ids only; unknown ids are
    /// dropped on read, a damaged file is reported and left unchanged.
    fn arena_favorites(&self, change: Option<(&str, bool)>) -> Result<Value> {
        static LOCK: Mutex<()> = Mutex::new(());
        let _guard = LOCK
            .lock()
            .map_err(|_| anyhow::anyhow!("Arena favorites unavailable"))?;
        let known = |id: &str| crate::blacktop_arena::choice(id).is_ok();
        let path = self.root.join("Data/arena-favorites.json");
        let mut ids: Vec<String> = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<Vec<String>>(&bytes)
                .ok()
                .filter(|v| v.len() <= 1000)
                .context("Could not read arena favorites. Your saved file was left unchanged.")?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e).context("Could not read arena favorites"),
        };
        let mut seen = std::collections::BTreeSet::new();
        ids.retain(|id| known(id) && seen.insert(id.clone()));
        if let Some((id, favorite)) = change {
            ensure!(known(id), "Invalid arena favorite.");
            ids.retain(|x| x != id);
            if favorite {
                ids.push(id.into());
            }
            fs::create_dir_all(self.root.join("Data"))?;
            atomic_json(&path, &ids)?;
        }
        Ok(json!(ids))
    }
    fn edit_video(&self, args: &Value) -> Result<Value> {
        ensure!(
            !self.closing.load(std::sync::atomic::Ordering::Acquire),
            "Launcher is closing; video library unchanged"
        );
        let action = args["action"].as_str().context("Video action required")?;
        let id = args["id"].as_str().context("Saved video ID required")?;
        let mut message = "Video renamed.".to_owned();
        match action {
            "rename" => {
                let name = args["name"].as_str().context("Video name required")?.trim();
                self.with_videos(|store| store.rename(id, name, &unique_id()))?;
            }
            "remove" => {
                let outcome = self.with_videos(|store| store.remove(id, &unique_id()))?;
                message = if outcome.file_removed {
                    "Video removed from the library.".into()
                } else {
                    "Video removed from the library; its file is in use and was kept in Library/Videos/mp4.".into()
                };
                let mut current = self
                    .settings
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Settings unavailable"))?;
                if current.video == id {
                    let mut next = current.clone();
                    next.video = String::new();
                    next.startup_video = false;
                    validate_settings(&next)?;
                    self.settings_store
                        .lock()
                        .map_err(|_| anyhow::anyhow!("Settings storage unavailable"))?
                        .save(&next)?;
                    *current = next;
                }
            }
            _ => anyhow::bail!("Unsupported video library action"),
        }
        Ok(json!({"media":self.media()?,"settings":self.current_settings()?,"message":message}))
    }
    pub(crate) fn cancel_video_import(&self, operation_id: &str) -> Result<bool> {
        self.video_import.cancel(operation_id)
    }
    fn media(&self) -> Result<Value> {
        let mut rows: Vec<Value> = self.with_videos(|store| {
            store.verify_current()?;
            store
                .rows()
                .into_iter()
                .map(serde_json::to_value)
                .collect::<std::result::Result<_, _>>()
                .map_err(Into::into)
        })?;
        for kind in ["Music"] {
            for entry in fs::read_dir(self.root.join("Library").join(kind))? {
                let entry = entry?;
                let meta = entry.metadata()?;
                if meta.is_file() {
                    rows.push(json!({"kind":kind,"name":entry.file_name().to_string_lossy(),"path":entry.path(),"bytes":meta.len()}));
                }
            }
        }
        rows.sort_by_key(|r| r["name"].as_str().unwrap_or("").to_lowercase());
        Ok(json!(rows))
    }
    pub(crate) fn with_videos<T>(
        &self,
        action: impl FnOnce(&mut crate::video_library::Store) -> Result<T>,
    ) -> Result<T> {
        let mut store = self
            .videos
            .lock()
            .map_err(|_| anyhow::anyhow!("Video library unavailable"))?;
        if store.is_none() {
            *store = Some(crate::video_library::Store::open(&self.root)?);
        }
        action(store.as_mut().unwrap())
    }
    pub(crate) fn decode_fixture(&self) -> Result<Value> {
        self.with_videos(|store| {
            store.verify_current()?;
            let cancel = std::sync::atomic::AtomicBool::new(false);
            let mut rows = Vec::new();
            let mut leases = Vec::new();
            for item in &store.view().items {
                let lease = store.resolve(&item.id, &cancel)?;
                rows.push(json!({"id":item.id,"name":item.name,"path":lease.path(),"durationMs":item.duration_ms,"codecs":item.codecs,"sha256":item.sha256}));
                leases.push(lease);
            }
            let mut retained = self.decode_leases.lock().map_err(|_| anyhow::anyhow!("Decode fixture pins unavailable"))?;
            ensure!(retained.is_empty(), "Decode fixture has already retained its sources");
            *retained = leases;
            Ok(json!({"rows":rows}))
        })
    }
}
fn garment_pool(piece: &Value, pool: &str) -> bool {
    if piece["category"] == "shoes" {
        piece["pools"]
            .as_array()
            .is_some_and(|p| p.iter().any(|p| p.as_str() == Some(pool)))
    } else {
        piece["pool"].as_str() == Some(pool)
    }
}
/// WPF normalize(): lowercase, anything but a-z/0-9 becomes a single space.
fn catalog_search_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push(' ');
    for c in text.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with(' ') {
            out.push(' ');
        }
    }
    out
}
pub(crate) fn validate_settings(s: &Settings) -> Result<()> {
    ensure!(
        s.game_root.len() <= 1000
            && s.mods_root.len() <= 1000
            && s.profile.len() <= 100
            && s.arena_id.len() <= 64
            && s.scoreboard_id.len() <= 64,
        "Settings exceed limits"
    );
    ensure!(
        matches!(s.theme.as_str(), "dark" | "light"),
        "Unsupported theme"
    );
    ensure!(
        s.scaler.is_finite() && (0.8..=1.2).contains(&s.scaler),
        "Scaler must be between 0.8 and 1.2"
    );
    Ok(())
}
pub fn validate_outfit(o: &Outfit) -> Result<()> {
    ensure!(
        !o.name.trim().is_empty()
            && o.name.encode_utf16().count() <= 80
            && !o.name.chars().any(|c| c <= '\u{1f}' || c == '\u{7f}'),
        "Outfit name required (maximum 80 characters)"
    );
    ensure!(
        matches!(o.pool.as_str(), "male" | "female"),
        "Invalid clothing fit"
    );
    ensure!(o.accessories.len() <= 3, "Maximum 3 accessories");
    ensure!(
        [
            &o.top,
            &o.bottom,
            &o.shoes,
            &o.socks,
            &o.street_id,
            &o.layer
        ]
        .into_iter()
        .chain(o.accessories.iter())
        .all(|s| s.len() <= 240),
        "Asset ID exceeds limit"
    );
    let mut seen = std::collections::HashSet::new();
    ensure!(
        o.accessories.iter().all(|a| seen.insert(a)),
        "Duplicate accessories"
    );
    Ok(())
}
/// WPF `core/catalog.js scanMods`: every file under the active mods folder,
/// skipping symbolic links and junctions; more than 100,000 files is an
/// error rather than a silently partial catalog.
pub const MAX_MOD_FILES: usize = 100_000;
pub fn scan_mods(root: &Path) -> Result<Value> {
    scan_mods_limited(root, MAX_MOD_FILES)
}
fn scan_mods_limited(root: &Path, maximum: usize) -> Result<Value> {
    if !root.is_dir() {
        return Ok(json!({"items":[],"total":0,"missing":true,"truncated":false}));
    }
    let mut rows = Vec::new();
    let mut pending = vec![(root.to_path_buf(), 0)];
    let mut skipped_links = 0usize;
    while let Some((dir, depth)) = pending.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let meta = fs::symlink_metadata(entry.path())?;
            if meta.file_type().is_symlink() {
                // Never traverse or list symbolic links and junctions.
                skipped_links += 1;
                continue;
            }
            if meta.is_dir() {
                ensure!(depth < 64, "Mod catalog folder tree is too deep.");
                pending.push((entry.path(), depth + 1));
            } else if meta.is_file() {
                if crate::mod_install::recovery_artifact(&entry.file_name().to_string_lossy()) {
                    continue;
                }
                ensure!(
                    rows.len() < maximum,
                    "Mod catalog exceeded its {} file safety limit.",
                    group_thousands(maximum)
                );
                rows.push(ModFile {
                    name: entry.file_name().to_string_lossy().into(),
                    path: entry.path().strip_prefix(root)?.to_string_lossy().into(),
                    bytes: meta.len(),
                    folder: dir.strip_prefix(root)?.to_string_lossy().into(),
                });
            }
        }
    }
    rows.sort_by(|a, b| a.path.to_lowercase().cmp(&b.path.to_lowercase()));
    Ok(
        json!({"total":rows.len(),"items":rows,"missing":false,"truncated":false,"skippedLinks":skipped_links,"root":root}),
    )
}
fn group_thousands(value: usize) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    out
}
#[cfg(test)]
mod tests {
    #[test]
    fn shoe_catalog_restores_names_brands_search_and_exact_ids() {
        let r = root();
        let b = Backend::new(r.clone()).unwrap();
        let all = b
            .dispatch("wardrobe", json!({"slot":"shoes","limit":80}))
            .unwrap();
        assert_eq!(all["total"], 503);
        for brand in ["Nike", "Jordan", "Adidas", "New Balance"] {
            assert!(all["groups"].as_array().unwrap().iter().any(|g| g == brand));
        }
        let found = b.dispatch("wardrobe", json!({"slot":"shoes","pool":"female","query":"Nike Air Force 1 High","group":"Nike"})).unwrap();
        assert!(
            found["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row["asset"] == "shoe_1000_default"
                    && row["label"] == "Nike Air Force 1 High · default")
        );
        let direct = b
            .dispatch(
                "garments",
                json!({"ids":["shoe_1000_default","shoe_0215_default"]}),
            )
            .unwrap();
        assert_eq!(direct[0]["group"], "Nike");
        assert_eq!(direct[1]["group"], "Unidentified");
        assert_eq!(direct[1]["asset"], "shoe_0215_default");
        assert_eq!(
            b.dispatch(
                "wardrobe",
                json!({"slot":"shoes","pool":"male","query":"shoe_1000_default"})
            )
            .unwrap()["total"],
            0
        );
        drop(b);
        std::fs::remove_dir_all(r).unwrap();
    }
    use super::*;
    #[test]
    fn outfit_pool_templates_are_valid_exact_and_never_enable_live_access() {
        let r = root();
        let b = Backend::new(r.clone()).unwrap();
        for pool in ["male", "female"] {
            let value = b.dispatch("outfit-for-pool", json!({"pool":pool})).unwrap();
            let outfit: Outfit = serde_json::from_value(value).unwrap();
            assert_eq!(outfit.pool, pool);
            validate_outfit(&outfit).unwrap();
            let c = crate::outfits::catalog().unwrap();
            c.compose(&c.selection(&outfit).unwrap()).unwrap();
        }
        assert!(
            b.dispatch("outfit-for-pool", json!({"pool":"other"}))
                .is_err()
        );
        assert!(
            b.dispatch("outfit-for-pool", json!({"pool":"male","description":1234}))
                .is_err()
        );
        assert!(!crate::outfit_journal::pending(&r).unwrap());
        drop(b);
        fs::remove_dir_all(r).unwrap();
    }
    fn root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "launcher-rust-test-{}-{}",
            std::process::id(),
            unique_id()
        ))
    }
    #[test]
    fn atomic_save_and_media_publish_support_long_windows_paths() {
        let r = root();
        let long = r
            .join("nested-directory-".repeat(8))
            .join("more-directory-".repeat(8));
        let b = Backend::new(long.clone()).unwrap();
        let destination = long
            .join("Data")
            .join(format!("{}.json", "long-name-".repeat(12)));
        assert!(destination.as_os_str().encode_wide().count() > 260);
        atomic_json(&destination, &json!({"revision":1})).unwrap();
        atomic_json(&destination, &json!({"revision":2})).unwrap();
        assert_eq!(
            read_json::<Value>(&destination, 1024).unwrap(),
            json!({"revision":2})
        );
        assert!(!fs::read_dir(long.join("Data")).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".write-")
        }));
        let source = r.join("test.mp3");
        fs::write(&source, b"isolated path fixture").unwrap();
        b.import_media(&source, "Music").unwrap();
        assert_eq!(
            fs::read(long.join("Library/Music/test.mp3")).unwrap(),
            b"isolated path fixture"
        );
        assert!(b.import_media(&source, "Music").is_err());
        drop(b);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn outfit_pieces_report_each_draft_piece_slot_for_the_f8_editor() {
        let b = Backend::new(root()).unwrap();
        let outfit = b
            .dispatch("outfit-for-pool", json!({"pool":"male"}))
            .unwrap();
        let phones = b
            .dispatch(
                "wardrobe",
                json!({"slot":"headphones","pool":"male","offset":0,"limit":1}),
            )
            .unwrap()["items"][0]["asset"]
            .clone();
        let outfit = b
            .dispatch(
                "select-outfit-piece",
                json!({"outfit":outfit,"slot":"headphones","asset":phones}),
            )
            .unwrap();
        let pieces = b
            .dispatch("outfit-pieces", json!({"outfit":outfit}))
            .unwrap();
        let pieces = pieces.as_array().unwrap();
        assert!(
            pieces
                .iter()
                .any(|p| p["asset"] == phones && p["category"] == "headphones")
        );
        assert!(pieces.iter().all(|p| p["category"].is_string()));
    }
    #[test]
    fn catalog_exclusions_and_paging() {
        let r = root();
        let b = Backend::new(r.clone()).unwrap();
        let balls = b.dispatch("choices", json!({"kind":"ball"})).unwrap();
        assert_eq!(balls["total"], 426);
        assert_eq!(balls["items"].as_array().unwrap().len(), 24);
        let arenas = b
            .dispatch(
                "choices",
                json!({"kind":"arena","query":"arena_reccenter_lobbyint"}),
            )
            .unwrap();
        // WPF lists disabled scenes ("Needs work"); selection still refuses them.
        assert_eq!(arenas["total"], 1);
        assert_eq!(arenas["items"][0]["disabled"], true);
        assert!(crate::blacktop_arena::choice("arena_reccenter_lobbyint").is_err());
        let all = b.dispatch("choices", json!({"kind":"arena"})).unwrap();
        assert_eq!(all["total"], 165);
        assert_eq!(all["items"][0]["id"], "default");
        // WPF terms: every normalised word, in any order, across label, id,
        // file, floor and derived group.
        for (query, id) in [
            ("oracle 637", "arena_637_int"),
            ("ARENA_000_INT_floor", "arena_000_int"),
            ("76ers nba", "arena_000_int"),
        ] {
            let page = b
                .dispatch("choices", json!({"kind":"arena","query":query}))
                .unwrap();
            assert!(
                page["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r["id"] == id),
                "{query}"
            );
        }
        let scores = b
            .dispatch(
                "choices",
                json!({"kind":"scoreboard","query":"MyCareer story"}),
            )
            .unwrap();
        assert_eq!(scores["total"], 0);
        let a = b
            .dispatch(
                "wardrobe",
                json!({"slot":"top","pool":"male","offset":0,"limit":24}),
            )
            .unwrap();
        let c = b
            .dispatch(
                "wardrobe",
                json!({"slot":"top","pool":"male","offset":24,"limit":24}),
            )
            .unwrap();
        assert_ne!(a["items"][0]["asset"], c["items"][0]["asset"]);
        drop(b);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn release_version_matches_ui_backend_and_package_manifests() {
        let ui: Value = serde_json::from_str(include_str!("../package.json")).unwrap();
        let native: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(ui["version"].as_str(), Some(VERSION));
        assert_eq!(native["version"].as_str(), Some(VERSION));
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
    }
    #[test]
    fn defaults() {
        let s = Settings::default();
        assert!(!s.lock_camera);
        assert!(s.auto_show_overlay && s.auto_hide_overlay);
        validate_settings(&s).unwrap();
    }
    #[test]
    fn wpf_settings_are_imported_once_without_overwriting_the_original() {
        let r = root();
        fs::create_dir_all(r.join("Data")).unwrap();
        let original = serde_json::to_vec(&json!({"gameRoot":"C:\\Games\\NBA 2K27","modsRoot":"D:\\Mods","modsProfiles":[],"provider":"local","startCameras":true,"autoOpenBlacktop":false,"startupVideoEnabled":false})).unwrap();
        fs::write(r.join("Data/settings.json"), &original).unwrap();
        let b = Backend::new(r.clone()).unwrap();
        let initial = b.current_settings().unwrap();
        assert_eq!(initial.game_root, "C:\\Games\\NBA 2K27");
        assert!(!initial.auto_show_overlay);
        assert!(!initial.cameras_enabled);
        assert!(!initial.startup_video);
        b.dispatch("save-settings", json!({"theme":"light"}))
            .unwrap();
        assert_eq!(fs::read(r.join("Data/settings.json")).unwrap(), original);
        drop(b);
        let b = Backend::new(r.clone()).unwrap();
        assert_eq!(b.current_settings().unwrap().theme, "light");
        assert_eq!(fs::read(r.join("Data/settings.json")).unwrap(), original);
        drop(b);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn stale_settings_and_invalid_selected_video_hold_before_any_launch() {
        let r = root();
        let b = Backend::new(r.clone()).unwrap();
        let game = r.join("not-a-game").to_string_lossy().into_owned();
        b.dispatch(
            "save-settings",
            json!({"gameRoot":game,"startupVideo":true,"video":"not-a-video-id"}),
        )
        .unwrap();
        for channel in ["native-launch-modded", "native-launch-steam-only"] {
            let settings = b.current_settings().unwrap();
            let args = if channel == "native-launch-modded" {
                json!({"acknowledgement":"native-modded-qa-explicit-blacktop","gameRoot":settings.game_root,"modsRoot":settings.mods_root,"debugMenus":false})
            } else {
                json!({"acknowledgement":"steam-only-no-features","gameRoot":settings.game_root})
            };
            // WPF parity: an invalid startup video never blocks Play; this
            // launch is still refused, but only for its missing game.
            let error = b.launch_request(channel, args).err().unwrap().to_string();
            assert!(!error.to_lowercase().contains("video"), "{error}");
            assert!(b.selected_intro("not-a-video-id").is_err());
        }
        let path = r.join("Data/Rust/settings.json");
        fs::write(&path, b"{}").unwrap();
        let error = b
            .launch_request("native-launch-steam-only", Value::Null)
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("settings"), "{error}");
        assert!(!r.join("Data/session-journal.json").exists());
        assert_eq!(fs::read(path).unwrap(), b"{}");
        drop(b);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn closing_cancels_video_validation_and_both_new_play_paths_without_a_journal() {
        let r = root();
        let b = Backend::new(r.clone()).unwrap();
        let import = b.video_import.begin("shutdown-test").unwrap();
        b.begin_shutdown();
        assert!(b.closing.load(std::sync::atomic::Ordering::Acquire));
        assert!(import.cancel.load(std::sync::atomic::Ordering::Acquire));
        for channel in ["native-launch-modded", "native-launch-steam-only"] {
            let error = b
                .launch_request(channel, Value::Null)
                .err()
                .unwrap()
                .to_string();
            assert!(error.contains("closing"), "{error}");
        }
        assert!(!r.join("Data/session-journal.json").exists());
        assert!(!r.join("Data/loader-session.json").exists());
        drop(b);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn unused_jukebox_shutdown_is_immediate_idempotent_and_does_not_initialize_music() {
        let r = root();
        let b = Backend::new(r.clone()).unwrap();
        assert!(!r.join("Data/Jukebox").exists());
        assert!(!r.join("Library/Jukebox").exists());
        let started = std::time::Instant::now();
        b.jukebox.cancel();
        b.jukebox.shutdown();
        b.jukebox.shutdown();
        assert!(started.elapsed() < std::time::Duration::from_millis(150));
        assert!(!r.join("Data/Jukebox").exists());
        assert!(!r.join("Library/Jukebox").exists());
        drop(b);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn first_start_persists_default_settings_marker_without_touching_main_data() {
        let r = root();
        let b = Backend::new(r.clone()).unwrap();
        let path = r.join("Data/settings.json");
        let saved: Settings =
            serde_json::from_slice(&fs::read(r.join("Data/Rust/settings.json")).unwrap()).unwrap();
        assert_eq!(
            read_json::<Value>(&path, 65536).unwrap()["schema"],
            "nba2k27-portable-layout-v1"
        );
        assert_eq!(saved.game_root, Settings::default().game_root);
        assert!(!saved.lock_camera);
        drop(b);
        let before = fs::read(&path).unwrap();
        let b = Backend::new(r.clone()).unwrap();
        assert_eq!(fs::read(&path).unwrap(), before);
        drop(b);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn concurrent_cold_catalog_requests_share_single_cached_documents() {
        let r = root();
        let b = std::sync::Arc::new(Backend::new(r.clone()).unwrap());
        let threads = (0..8)
            .map(|_| {
                let b = b.clone();
                std::thread::spawn(move || {
                    (
                        b.catalog().unwrap() as *const Value as usize,
                        b.wardrobe().unwrap() as *const Value as usize,
                    )
                })
            })
            .collect::<Vec<_>>();
        let pointers = threads
            .into_iter()
            .map(|t| t.join().unwrap())
            .collect::<Vec<_>>();
        assert!(pointers.iter().all(|p| *p == pointers[0]));
        drop(b);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn complete_wpf_accessories_and_exact_shoe_pools_are_selectable() {
        let r = root();
        let b = Backend::new(r.clone()).unwrap();
        let oracle: Value =
            serde_json::from_str(include_str!("../assets/wardrobe-regressions.json")).unwrap();
        let rows = b.wardrobe().unwrap().as_array().unwrap();
        let by_asset: std::collections::HashMap<_, _> = rows
            .iter()
            .map(|p| (p["asset"].as_str().unwrap(), p))
            .collect();
        assert_eq!(by_asset.len(), rows.len(), "Duplicate selectable asset");
        for accessory in oracle["accessories"].as_array().unwrap() {
            let piece = by_asset
                .get(accessory["asset"].as_str().unwrap())
                .expect("WPF accessory missing");
            assert_eq!(piece["category"], accessory["slot"]);
            assert!(garment_pool(piece, accessory["pool"].as_str().unwrap()));
        }
        for shoe in oracle["shoes"].as_array().unwrap() {
            let piece = by_asset
                .get(shoe["asset"].as_str().unwrap())
                .expect("WPF shoe missing");
            for pool in ["male", "female"] {
                assert_eq!(
                    garment_pool(piece, pool),
                    shoe["pools"].as_array().unwrap().iter().any(|p| p == pool),
                    "Incorrect fit for {}",
                    shoe["asset"]
                );
            }
        }
        for slot in [
            "headwear",
            "headphones",
            "faceMask",
            "neckwear",
            "leftHand",
            "rightHand",
            "leftWrist",
            "rightWrist",
            "waist",
            "bodywear",
        ] {
            let items = b
                .dispatch("wardrobe", json!({"slot":slot,"pool":"male","limit":1}))
                .unwrap();
            assert!(items["total"].as_u64().unwrap() > 0, "Empty {slot}");
            let asset = items["items"][0]["asset"].as_str().unwrap();
            let outfit = json!({"name":"Catalog QA","top":"Shirtless","bottom":"basketball_shorts_solid_blk","shoes":"Currently installed","socks":"","accessories":[],"pool":"male"});
            let chosen = b
                .dispatch(
                    "select-outfit-piece",
                    json!({"outfit":outfit,"slot":slot,"asset":asset}),
                )
                .unwrap();
            assert!(
                chosen["accessories"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|a| a == asset)
            );
        }
        let item = b
            .dispatch(
                "wardrobe",
                json!({"slot":"shoes","pool":"male","query":"shoe_0000_default"}),
            )
            .unwrap();
        assert_eq!(item["total"], 1);
        let chosen = b
            .dispatch(
                "select-outfit-piece",
                json!({"outfit":{"name":"Shoe QA","top":"Shirtless","bottom":"basketball_shorts_solid_blk","shoes":"Currently installed","socks":"","accessories":[],"pool":"male"},"slot":"shoes","asset":"shoe_0000_default"}),
            )
            .unwrap();
        assert_eq!(chosen["shoes"], "shoe_0000_default");
        drop(b);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn team_dna_round_trip_and_invalid_file_preservation() {
        let r = root();
        let b = Backend::new(r.clone()).unwrap();
        let document = json!({"format":"2kteam","version":1,"game":"NBA2K27","name":"QA destination","info":{"name":"Test","cityName":"City"},"arena":{"name":"Venue","arenaFile":"arena_test","floorFile":"floor_test"},"staff":[{"slot":0,"values":{"firstName":"QA","faceId":123,"preferredSystemId":15}}],"rotations":(0..48).map(|_|vec!["A","B","C","D","E"]).collect::<Vec<_>>(),"uniforms":[{"match":{"typeId":0,"sideId":0,"designationId":1},"values":{"primaryColor":4294967295u64,"logo":3}}]});
        let clean = b.dispatch("save-team-dna", document).unwrap();
        assert_eq!(b.dispatch("team-dna", json!(null)).unwrap(), clean);
        let exported = b.dispatch("export-team-dna", clean.clone()).unwrap();
        let path = Path::new(exported["path"].as_str().unwrap());
        assert_eq!(crate::team_dna::read_file(path).unwrap(), clean);
        let invalid = r.join("Cache/Imports/bad.2kteam");
        fs::write(&invalid, b"{bad").unwrap();
        assert!(crate::team_dna::read_file(&invalid).is_err());
        assert_eq!(fs::read(&invalid).unwrap(), b"{bad");
        fs::write(&invalid, vec![b' '; 512 * 1024 + 1]).unwrap();
        assert!(crate::team_dna::read_file(&invalid).is_err());
        let mut bad = clean.clone();
        bad["info"]["teamId"] = json!(42);
        assert!(b.dispatch("save-team-dna", bad).is_err());
        assert_eq!(b.dispatch("team-dna", json!(null)).unwrap(), clean);
        fs::write(r.join("Data/team-dna.json"), b"[]").unwrap();
        assert!(b.dispatch("team-dna", json!(null)).is_err());
        assert_eq!(fs::read(r.join("Data/team-dna.json")).unwrap(), b"[]");
        drop(b);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn settings_persist_and_replace() {
        let r = root();
        let b = Backend::new(r.clone()).unwrap();
        let mut s = Settings::default();
        s.profile = "First".into();
        b.dispatch("save-settings", json!(s)).unwrap();
        s.profile = "Second".into();
        b.dispatch("save-settings", json!(s)).unwrap();
        assert_eq!(
            Backend::new(r.clone())
                .unwrap()
                .dispatch("bootstrap", json!(null))
                .unwrap()["settings"]["profile"],
            "Second"
        );
        assert!(!r.join("Data/settings.tmp").exists());
        drop(b);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn invalid_settings_preserved() {
        let r = root();
        let b = Backend::new(r.clone()).unwrap();
        fs::write(r.join("Data/Rust/settings.json"), "bad").unwrap();
        assert!(Backend::new(r.clone()).is_err());
        assert_eq!(
            fs::read_to_string(r.join("Data/Rust/settings.json")).unwrap(),
            "bad"
        );
        drop(b);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn malformed_saved_state_is_bounded_and_preserved() {
        let r = root();
        let b = Backend::new(r.clone()).unwrap();
        let p = r.join("Data/outfit.json");
        fs::write(&p, "{\"name\":\"not an outfit\"}").unwrap();
        assert!(b.dispatch("outfit", json!(null)).is_err());
        assert_eq!(
            fs::read_to_string(&p).unwrap(),
            "{\"name\":\"not an outfit\"}"
        );
        let p = r.join("Data/Rust/settings.json");
        let bad = json!({"scaler":-99});
        fs::write(&p, serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(Backend::new(r.clone()).is_err());
        assert_eq!(read_json::<Value>(&p, 64 * 1024).unwrap(), bad);
        fs::write(&p, vec![b' '; 64 * 1024 + 1]).unwrap();
        assert!(Backend::new(r.clone()).is_err());
        fs::remove_file(p).unwrap();
        let mut l = crate::library::Library::default();
        l.cameras[0].zoom = -101.;
        atomic_json(&r.join("Data/library.json"), &l).unwrap();
        assert!(Backend::new(r.clone()).is_err());
        drop(b);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn accessory_limit() {
        let o = Outfit {
            name: "Test".into(),
            top: String::new(),
            bottom: String::new(),
            shoes: String::new(),
            socks: String::new(),
            pool: default_pool(),
            accessories: vec!["a".into(); 4],
            street_id: String::new(),
            layer: String::new(),
        };
        assert!(validate_outfit(&o).is_err());
    }
    #[test]
    fn library_round_trip_and_validation() {
        let r = root();
        let b = Backend::new(r.clone()).unwrap();
        let mut l = b.dispatch("library", json!(null)).unwrap();
        l["profiles"][0]["name"] = json!("QA profile");
        l["cameras"][0]["height"] = json!(1.5);
        b.dispatch("save-library", l.clone()).unwrap();
        assert_eq!(
            Backend::new(r.clone())
                .unwrap()
                .dispatch("library", json!(null))
                .unwrap(),
            l
        );
        l["cameras"][0]["zoom"] = json!(-101);
        assert!(b.dispatch("save-library", l).is_err());
        assert_eq!(
            b.dispatch("library", json!(null)).unwrap()["cameras"][0]["zoom"],
            1.
        );
        drop(b);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn saved_ball_resets_each_run_but_arena_and_scoreboard_are_kept() {
        let r = root();
        let b = Backend::new(r.clone()).unwrap();
        b.dispatch(
            "save-settings",
            json!({"ball":"Allstar","arena":"Aaugym","arenaId":"arena_aaugym_int","scoreboard":"Broadcast"}),
        )
        .unwrap();
        assert_eq!(b.current_settings().unwrap().ball, "Allstar");
        drop(b);
        let b = Backend::new(r.clone()).unwrap();
        let s = b.current_settings().unwrap();
        assert_eq!(s.ball, Settings::default().ball);
        assert_eq!(
            (s.arena.as_str(), s.arena_id.as_str()),
            ("Aaugym", "arena_aaugym_int")
        );
        assert_eq!(s.scoreboard, "Broadcast");
        drop(b);
        let _ = fs::remove_dir_all(r);
    }
    #[test]
    fn settings_patches_preserve_other_window_changes() {
        let r = root();
        let b = Backend::new(r.clone()).unwrap();
        b.dispatch("save-settings", json!({"theme":"light"}))
            .unwrap();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                b.dispatch("save-settings", json!({"ball":"Allstar"}))
                    .unwrap()
            });
            scope.spawn(|| {
                b.dispatch("save-settings", json!({"removeCrowd":true}))
                    .unwrap()
            });
        });
        let saved = b.dispatch("bootstrap", json!(null)).unwrap()["settings"].clone();
        assert_eq!(saved["theme"], "light");
        assert_eq!(saved["ball"], "Allstar");
        assert_eq!(saved["removeCrowd"], true);
        assert!(
            b.dispatch("save-settings", json!({"unknownField":true}))
                .is_err()
        );
        assert_eq!(
            b.dispatch("bootstrap", json!(null)).unwrap()["settings"],
            saved
        );
        drop(b);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn accessory_selection_replaces_slot_and_keeps_shoes() {
        let r = root();
        let b = Backend::new(r.clone()).unwrap();
        let pieces = b
            .dispatch(
                "wardrobe",
                json!({"slot":"headband","pool":"male","limit":2}),
            )
            .unwrap();
        let a = &pieces["items"][0]["asset"];
        let replacement = &pieces["items"][1]["asset"];
        let outfit = json!({"name":"Test","top":"Shirtless","bottom":"","shoes":"Keep","pool":"male","accessories":[a,"other-slot-a","other-slot-b"]});
        let next = b
            .dispatch(
                "select-outfit-piece",
                json!({"outfit":outfit,"slot":"headband","asset":replacement}),
            )
            .unwrap();
        assert_eq!(next["accessories"].as_array().unwrap().len(), 3);
        assert!(!next["accessories"].as_array().unwrap().contains(a));
        assert_eq!(next["shoes"], "Keep");
        let full = json!({"name":"Test","top":"Shirtless","bottom":"","shoes":"Keep","pool":"male","accessories":["other-a","other-b","other-c"]});
        assert!(
            b.dispatch(
                "select-outfit-piece",
                json!({"outfit":full,"slot":"headband","asset":a})
            )
            .is_err()
        );
        drop(b);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn random_outfit_parallel_calls_do_not_share_a_same_clock_stream() {
        use std::sync::atomic::{AtomicU64, Ordering};
        // The previous clock-only seed repeats every selection when a batch
        // arrives in one clock tick. Fixed entropy also tests the stronger case
        // where even the entropy input repeats, without a probabilistic test.
        let clock = 0x12345678u64;
        let old_stream = || {
            let mut seed = clock | 1;
            (0..3)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    seed % 1009
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(old_stream(), old_stream());
        let sequence = AtomicU64::new(0);
        let streams = std::thread::scope(|scope| {
            let workers = (0..64)
                .map(|_| {
                    let sequence = &sequence;
                    scope.spawn(move || {
                        let mut random = OutfitRandom::from_entropy(
                            clock,
                            sequence.fetch_add(1, Ordering::Relaxed),
                        );
                        let first = random.word();
                        let choices = [random.index(103), random.index(47), random.index(11)];
                        (first, choices)
                    })
                })
                .collect::<Vec<_>>();
            workers
                .into_iter()
                .map(|worker| worker.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert_eq!(sequence.load(Ordering::Relaxed), 64);
        assert_eq!(
            streams
                .iter()
                .map(|s| s.0)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            64
        );
        assert!(
            streams
                .iter()
                .map(|s| s.1)
                .collect::<std::collections::HashSet<_>>()
                .len()
                > 48
        );
        assert!(
            streams
                .iter()
                .all(|s| s.1[0] < 103 && s.1[1] < 47 && s.1[2] < 11)
        );
        let mut zero = OutfitRandom::from_entropy(0, 0);
        assert_ne!(zero.word(), 0);
        assert_eq!(zero.index(1), 0);
    }
    #[test]
    fn random_clothes_retains_initial_streetwear_without_accessories() {
        let r = root();
        let b = Backend::new(r.clone()).unwrap();
        for pool in ["male", "female"] {
            let o = json!({"name":"Initial streetwear","top":"","bottom":"","shoes":"Keep these shoes","accessories":[],"pool":pool});
            for _ in 0..10 {
                let next = b
                    .dispatch("random-outfit", json!({"mode":"clothes","outfit":o}))
                    .unwrap();
                assert_eq!(next["pool"], pool);
                assert_eq!(next["shoes"], o["shoes"]);
                assert_eq!(next["accessories"], json!([]));
                for slot in ["top", "bottom", "socks"] {
                    let asset = next[slot].as_str().unwrap();
                    assert!(!asset.is_empty());
                    assert!(!asset.starts_with("UNIFORM"));
                    let rows = b.dispatch("garments", json!({"ids":[asset]})).unwrap();
                    assert!(garment_pool(&rows[0], pool));
                }
            }
        }
        drop(b);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn random_respects_fit_and_shoes() {
        let r = root();
        let b = Backend::new(r.clone()).unwrap();
        let o = json!({"name":"Test","top":"","bottom":"","shoes":"Keep these shoes","accessories":[],"pool":"female"});
        for _ in 0..20 {
            let next = b
                .dispatch("random-outfit", json!({"mode":"both","outfit":o}))
                .unwrap();
            assert_eq!(next["shoes"], o["shoes"]);
            assert!(next["accessories"].as_array().unwrap().len() <= 3);
            for asset in [next["top"].clone(), next["bottom"].clone()]
                .iter()
                .chain(next["accessories"].as_array().unwrap().iter())
            {
                let rows = b.dispatch("garments", json!({"ids":[asset]})).unwrap();
                assert_eq!(rows[0]["pool"], "female");
            }
        }
        drop(b);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn shirtless_is_paginated_once_and_searchable() {
        let r = root();
        let b = Backend::new(r.clone()).unwrap();
        let first = b
            .dispatch("wardrobe", json!({"slot":"top","pool":"male","limit":24}))
            .unwrap();
        assert_eq!(first["items"].as_array().unwrap().len(), 24);
        assert_eq!(first["items"][0]["asset"], "Shirtless");
        let next = b
            .dispatch(
                "wardrobe",
                json!({"slot":"top","pool":"male","limit":24,"offset":24}),
            )
            .unwrap();
        assert!(
            !next["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["asset"] == "Shirtless")
        );
        let search = b
            .dispatch(
                "wardrobe",
                json!({"slot":"top","pool":"female","query":"shirtless"}),
            )
            .unwrap();
        assert_eq!(search["items"].as_array().unwrap().len(), 0);
        let male = b
            .dispatch(
                "wardrobe",
                json!({"slot":"top","pool":"male","query":"shirtless"}),
            )
            .unwrap();
        assert_eq!(male["items"].as_array().unwrap().len(), 1);
        let raw = b
            .dispatch("wardrobe", json!({"slot":"top","query":"NO SHIRT"}))
            .unwrap();
        assert_eq!(raw["total"], 0);
        let tanks = b
            .dispatch(
                "wardrobe",
                json!({"slot":"top","pool":"male","group":"Tank tops","limit":24}),
            )
            .unwrap();
        assert!(tanks["total"].as_u64().unwrap() > 24);
        assert!(
            tanks["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|p| p["group"] == "Tank tops")
        );
        drop(b);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn catalog_and_history() {
        let r = root();
        let b = Backend::new(r.clone()).unwrap();
        let result = b
            .dispatch("players", json!({"query":"Stephen Curry"}))
            .unwrap();
        assert!(result["total"].as_u64().unwrap() > 0);
        let p = b
            .dispatch("player", json!({"id":result["items"][0]["id"]}))
            .unwrap();
        assert!(!p["records"].as_array().unwrap().is_empty());
        let dna = b
            .dispatch("catalog-player-dna", json!({"id":result["items"][0]["id"]}))
            .unwrap();
        assert_eq!(dna["info"]["birthDate"], "1988-03-14");
        b.dispatch("save-player-dna", dna.clone()).unwrap();
        assert_eq!(b.dispatch("player-dna", json!(null)).unwrap(), dna);
        let exported = b.dispatch("export-player-dna", dna.clone()).unwrap();
        assert_eq!(
            crate::player_dna::validate(
                &serde_json::from_slice(&fs::read(exported["path"].as_str().unwrap()).unwrap())
                    .unwrap()
            )
            .unwrap(),
            dna
        );
        drop(b);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn all_bundled_players_have_valid_native_dna() {
        let r = root();
        let b = Backend::new(r.clone()).unwrap();
        let c = b.catalog().unwrap();
        for p in c["players"].as_array().unwrap() {
            crate::player_dna::from_catalog(p, &c["fieldKeys"], &c["targetSchemaId"])
                .unwrap_or_else(|e| panic!("Player {}: {e:#}", p["id"]));
        }
        drop(b);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn media_import_uses_stable_ids_and_preserves_same_named_sources() {
        let r = root();
        let b = Backend::new(r.clone()).unwrap();
        let source = r.join("sample.mp4");
        let bytes = crate::startup_mp4::tests::valid(b"avc1", false);
        fs::write(&source, &bytes).unwrap();
        let first = b.import_media(&source, "Videos").unwrap();
        let id = first[0]["id"].as_str().unwrap();
        assert_eq!(first[0]["name"], "sample");
        assert_eq!(
            fs::read(r.join(format!("Library/Videos/mp4/{id}.mp4"))).unwrap(),
            bytes
        );
        let second = b.import_media(&source, "Videos").unwrap();
        assert_eq!(second.as_array().unwrap().len(), 2);
        assert_ne!(second[0]["id"], second[1]["id"]);
        assert_eq!(fs::read(&source).unwrap(), bytes);
        assert!(!r.join("Library/Videos/sample.mp4").exists());
        assert!(b.import_media(&source, "../Data").is_err());
        drop(b);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn status_replace_retries_only_transient_denials_with_a_bound() {
        use windows::Win32::Foundation::{
            ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_SHARING_VIOLATION,
        };
        let denied = || windows::core::Error::from(ERROR_ACCESS_DENIED.to_hresult());
        // A reader holding the old status for two attempts: third rename wins.
        let (mut calls, mut pauses) = (0, Vec::new());
        replace_with_retry(
            || {
                calls += 1;
                match calls {
                    1 => Err(denied()),
                    2 => Err(ERROR_SHARING_VIOLATION.to_hresult().into()),
                    _ => Ok(()),
                }
            },
            |a| pauses.push(a),
        )
        .unwrap();
        assert_eq!((calls, pauses), (3, vec![1, 2]));
        // Persistent denial is bounded and still reported as access denied.
        let mut calls = 0;
        let error = replace_with_retry(
            || {
                calls += 1;
                Err(denied())
            },
            |_| {},
        )
        .unwrap_err();
        assert_eq!(calls, REPLACE_ATTEMPTS);
        assert_eq!(error.code(), ERROR_ACCESS_DENIED.to_hresult());
        // Other failures are never retried.
        let mut calls = 0;
        assert!(
            replace_with_retry(
                || {
                    calls += 1;
                    Err(ERROR_FILE_NOT_FOUND.to_hresult().into())
                },
                |_| {},
            )
            .is_err()
        );
        assert_eq!(calls, 1);
    }
    #[test]
    fn status_json_round_trips_en_dash_as_utf8() {
        let r = root();
        fs::create_dir_all(&r).unwrap();
        let message = "native range 0.95\u{2013}1.00";
        let value = json!({"v2Features":{"features":{"scaler":{"message":message}}}});
        let path = r.join("provider-state.json");
        atomic_json(&path, &value).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert!(bytes.windows(3).any(|w| w == [0xe2, 0x80, 0x93]));
        assert!(!bytes.windows(2).any(|w| w == [0xc3, 0xa2]));
        let back: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back, value);
        assert_eq!(back["v2Features"]["features"]["scaler"]["message"], message);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn arena_favorites_keep_exact_catalog_ids_and_leave_a_damaged_file_unchanged() {
        let r = root();
        let b = Backend::new(r.clone()).unwrap();
        assert_eq!(b.dispatch("arena-favorites", json!({})).unwrap(), json!([]));
        for id in ["arena_000_int", "arena_aaugym_int", "arena_000_int"] {
            b.dispatch("arena-favorite", json!({"id":id,"favorite":true}))
                .unwrap();
        }
        assert_eq!(
            b.dispatch("arena-favorites", json!({})).unwrap(),
            json!(["arena_aaugym_int", "arena_000_int"])
        );
        assert_eq!(
            b.dispatch(
                "arena-favorite",
                json!({"id":"arena_aaugym_int","favorite":false})
            )
            .unwrap(),
            json!(["arena_000_int"])
        );
        for bad in [
            json!({"id":"../x","favorite":true}),
            json!({"id":"arena_000_int"}),
        ] {
            assert!(b.dispatch("arena-favorite", bad).is_err());
        }
        let file = r.join("Data/arena-favorites.json");
        fs::write(&file, br#"["arena_000_int","gone"]"#).unwrap();
        assert_eq!(
            b.dispatch("arena-favorites", json!({})).unwrap(),
            json!(["arena_000_int"])
        );
        fs::write(&file, b"{broken").unwrap();
        assert!(b.dispatch("arena-favorites", json!({})).is_err());
        assert!(
            b.dispatch(
                "arena-favorite",
                json!({"id":"arena_000_int","favorite":false})
            )
            .is_err()
        );
        assert_eq!(fs::read(&file).unwrap(), b"{broken");
        fs::write(&file, br#"["arena_aaugym_int"]"#).unwrap();
        let page = b
            .dispatch("choices", json!({"kind":"arena","group":"Favorites"}))
            .unwrap();
        assert_eq!(page["total"], 1);
        assert_eq!(page["items"][0]["id"], "arena_aaugym_int");
        drop(b);
        let _ = fs::remove_dir_all(r);
    }
    #[test]
    fn video_library_edit_renames_and_remove_clears_the_selected_video() {
        let r = root();
        let b = Backend::new(r.clone()).unwrap();
        let source = r.join("intro.mp4");
        let bytes = crate::startup_mp4::tests::valid(b"avc1", false);
        fs::write(&source, &bytes).unwrap();
        let media = b
            .import_video_with_progress(&source, "edit-generation", |_, _| Ok(()))
            .unwrap();
        let id = media[0]["id"].as_str().unwrap().to_owned();
        assert!(media[0]["durationMs"].as_u64().unwrap() > 0);
        b.dispatch("save-settings", json!({"video":id,"startupVideo":true}))
            .unwrap();
        let renamed = b
            .dispatch(
                "video-library-edit",
                json!({"action":"rename","id":id,"name":"  Finals intro "}),
            )
            .unwrap();
        assert_eq!(renamed["media"][0]["name"], "Finals intro");
        assert!(
            b.dispatch("video-library-edit", json!({"action":"select","id":id}))
                .is_err()
        );
        let removed = b
            .dispatch("video-library-edit", json!({"action":"remove","id":id}))
            .unwrap();
        assert!(removed["media"].as_array().unwrap().is_empty());
        assert_eq!(removed["settings"]["video"], "");
        assert_eq!(removed["settings"]["startupVideo"], false);
        assert_eq!(fs::read(&source).unwrap(), bytes);
        assert_eq!(
            fs::read_dir(r.join("Library/Videos/mp4")).unwrap().count(),
            0
        );
        drop(b);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn cancellable_video_import_reports_progress_and_publishes_nothing() {
        let r = root();
        let b = Backend::new(r.clone()).unwrap();
        let source = r.join("cancel.mp4");
        let bytes = crate::startup_mp4::tests::valid(b"avc1", false);
        fs::write(&source, &bytes).unwrap();
        let mut phases = Vec::new();
        let error = b
            .import_video_with_progress(&source, "cancel-generation", |id, update| {
                assert_eq!(id, "cancel-generation");
                phases.push(update.phase);
                if matches!(update.phase, crate::video_library::Phase::Copying) {
                    assert!(b.cancel_video_import(id).unwrap());
                }
                Ok(())
            })
            .unwrap_err()
            .to_string();
        assert!(error.to_lowercase().contains("cancel"), "{error}");
        assert!(phases.contains(&crate::video_library::Phase::Checking));
        assert!(phases.contains(&crate::video_library::Phase::Copying));
        assert_eq!(fs::read(&source).unwrap(), bytes);
        assert!(b.media().unwrap().as_array().unwrap().is_empty());
        assert_eq!(
            fs::read_dir(r.join("Library/Videos/mp4")).unwrap().count(),
            0
        );
        drop(b);
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn scan_read_only() {
        let r = root();
        fs::create_dir_all(r.join("char/sig")).unwrap();
        fs::write(r.join("char/sig/test.iff"), b"abc").unwrap();
        let result = scan_mods(&r).unwrap();
        assert_eq!(result["total"], 1);
        assert_eq!(result["items"][0]["bytes"], 3);
        assert_eq!(fs::read(r.join("char/sig/test.iff")).unwrap(), b"abc");
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn scan_matches_wpf_limit_and_skips_junctions() {
        let r = root();
        fs::create_dir_all(r.join("char")).unwrap();
        for name in ["a.iff", "b.iff", "c.iff"] {
            fs::write(r.join("char").join(name), b"x").unwrap();
        }
        let target = root();
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("outside.iff"), b"y").unwrap();
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(r.join("linked"))
            .arg(&target)
            .output()
            .unwrap();
        assert!(made.status.success());
        let result = scan_mods_limited(&r, 3).unwrap();
        assert_eq!(result["total"], 3);
        assert_eq!(result["skippedLinks"], 1);
        let error = scan_mods_limited(&r, 2).unwrap_err();
        assert!(format!("{error:#}").contains("2 file safety limit"));
        assert_eq!(group_thousands(100_000), "100,000");
        assert!(
            scan_mods(&r.join("missing")).unwrap()["missing"]
                .as_bool()
                .unwrap()
        );
        fs::remove_dir(r.join("linked")).unwrap();
        fs::remove_dir_all(r).unwrap();
        fs::remove_dir_all(target).unwrap();
    }
}
