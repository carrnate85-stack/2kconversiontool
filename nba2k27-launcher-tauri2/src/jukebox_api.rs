//! Native Jukebox command service for JukeboxNative.tsx. The production host
//! dispatches on a blocking pool, injects cancellable pickers, and owns the
//! separate game-session guardian. No Steam/game-process access here.
#![allow(dead_code)]
use crate::{
    jukebox_audio as audio, jukebox_catalog as catalog, jukebox_decode_worker,
    jukebox_package as package,
    jukebox_service::{Prepared, Service, State},
    jukebox_v2_resources,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    mem::size_of,
    os::windows::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Component, Path, PathBuf, Prefix},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::HANDLE,
    Security::Cryptography::{BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom},
    Storage::FileSystem::*,
};

const MAX_BATCH: usize = 100;
const MAX_PREVIEW_TOTAL: usize = 256 * 1024 * 1024;
const PREVIEW_TTL: Duration = Duration::from_secs(30 * 60);
pub(crate) use jukebox_decode_worker::admission;
#[path = "jukebox_api_artwork.rs"]
mod artwork;

/// Implement with owned native pickers. A picker MUST observe cancellation and
/// close its own dialog; no callback may start an unowned process. The default
/// decoder is the existing exact-owned, kill-on-close same-EXE guardian.
pub(crate) trait Host: Send + Sync + 'static {
    fn choose_audio(&self, cancel: &AtomicBool) -> Result<Option<Vec<PathBuf>>>;
    fn choose_single_audio(&self, cancel: &AtomicBool) -> Result<Option<Vec<PathBuf>>> {
        let paths = self.choose_audio(cancel)?;
        ensure!(
            paths.as_ref().is_none_or(|p| p.len() == 1),
            "Choose exactly one audio file in single-song mode"
        );
        Ok(paths)
    }
    fn choose_playlist(&self, cancel: &AtomicBool) -> Result<Option<PathBuf>>;
    fn choose_export(&self, suggested: &str, cancel: &AtomicBool) -> Result<Option<PathBuf>>;
    fn choose_artwork(&self, _cancel: &AtomicBool) -> Result<Option<PathBuf>> {
        anyhow::bail!("Artwork picker is unavailable")
    }
    fn open_library_folder(&self, _folder: &Path) -> Result<()> {
        anyhow::bail!("Library folder action is unavailable")
    }
    fn prepare_artwork(
        &self,
        bytes: &[u8],
        cancel: &AtomicBool,
    ) -> Result<crate::jukebox_artwork::Prepared> {
        crate::jukebox_artwork::prepare(bytes, cancel)
    }
    fn lookup_artwork(
        &self,
        artist: &str,
        title: &str,
        cancel: &AtomicBool,
    ) -> Result<Vec<crate::jukebox_artwork::Candidate>> {
        crate::jukebox_artwork::lookup(artist, title, cancel)
    }
    fn artwork_lookup_enabled(&self) -> bool {
        true
    }
    /// Native-only remains usable without a trusted backend callback. An
    /// error/unknown controller state must never authorize a decoder child.
    fn decode_admission(&self) -> Result<admission::Admission> {
        Ok(admission::Admission::NativeOnly)
    }
    fn decode(
        &self,
        bytes: &[u8],
        nonce: &str,
        cancel: Arc<AtomicBool>,
        progress: &mut dyn FnMut(u32) -> Result<()>,
    ) -> Result<Vec<u8>> {
        jukebox_decode_worker::decode_guarded(
            &std::env::current_exe()?,
            bytes,
            nonce,
            cancel,
            progress,
            || self.decode_admission()?.require_decoder(),
        )
    }
    fn preview_enabled(&self) -> bool {
        false
    }
    /// Only return the host's owned loopback token route. Never a path/URL
    /// selected by the renderer. The UI independently validates this address.
    fn preview_url(&self, _token: &str) -> Option<String> {
        None
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Job {
    pub id: String,
    pub kind: String,
    pub phase: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frames: Option<u32>,
    pub cancellable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
impl Job {
    fn active(&self) -> bool {
        !["completed", "cancelled", "failed"].contains(&self.phase.as_str())
    }
}
#[derive(Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Playback {
    pub phase: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playlist_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    pub gameplay_verified: bool,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Capabilities {
    import_audio: bool,
    import_playlists: bool,
    export_playlists: bool,
    custom_preview: bool,
    stock_preview: bool,
    native_playback: bool,
    max_session_slots: usize,
    artwork_editing: bool,
    artwork_lookup: bool,
    artwork_preview: bool,
    playlist_reordering: bool,
    native_artwork_publication: bool,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct View {
    schema: &'static str,
    pub sequence: u64,
    pub revision: String,
    pub state: State,
    stock_keys: Vec<String>,
    capabilities: Capabilities,
    pub playback: Playback,
    pub job: Option<Job>,
    pub warnings: Vec<String>,
}
struct Lease {
    mime: &'static str,
    data: Arc<[u8]>,
    expires: Instant,
    kind: artwork::Kind,
}
struct Status {
    view: View,
    busy: bool,
    cancel: Option<Arc<AtomicBool>>,
    leases: BTreeMap<String, Lease>,
    prepared_art: BTreeMap<String, artwork::PreparedLease>,
}
struct Shared {
    service: Mutex<Service>,
    status: Mutex<Status>,
    done: Condvar,
    closing: AtomicBool,
    stock: catalog::Music,
    game_root: Option<PathBuf>,
    host: Arc<dyn Host>,
}
pub(crate) struct Api {
    shared: Arc<Shared>,
    worker: Mutex<Option<JoinHandle<()>>>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Preview {
    pub token: String,
    pub mime_type: &'static str,
    pub size: usize,
}
pub(crate) struct PreviewChunk {
    pub mime_type: &'static str,
    pub total: usize,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct LaunchSeal {
    pub revision: String,
    pub playlist_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Args {
    expected_revision: Option<String>,
    name: Option<String>,
    playlist_id: Option<String>,
    track_keys: Option<Vec<String>>,
    job_id: Option<String>,
    track_key: Option<String>,
    token: Option<String>,
    title: Option<String>,
    artist: Option<String>,
    artwork_token: Option<String>,
    playlist_ids: Option<Vec<String>>,
    add_default_songs: Option<bool>,
    single: Option<bool>,
}
#[derive(Default)]
struct AudioOptions {
    single: bool,
    title: Option<String>,
    artist: Option<String>,
}
impl AudioOptions {
    fn new(single: bool, title: Option<String>, artist: Option<String>) -> Result<Self> {
        ensure!(
            single || (title.is_none() && artist.is_none()),
            "Song metadata is supported only for single-song import, not a batch"
        );
        for text in [&title, &artist].into_iter().flatten() {
            ensure!(
                !text.is_empty()
                    && text == text.trim()
                    && text.chars().count() <= 120
                    && !text.chars().any(char::is_control),
                "Use printable song metadata up to 120 characters"
            );
        }
        Ok(Self {
            single,
            title,
            artist,
        })
    }
}
fn nonce() -> Result<String> {
    let mut bytes = [0; 24];
    unsafe {
        BCryptGenRandom(None, &mut bytes, BCRYPT_USE_SYSTEM_PREFERRED_RNG).ok()?;
    }
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
fn lock<T>(value: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>> {
    value.lock().map_err(|_| {
        anyhow::anyhow!("Native Jukebox ownership lock was poisoned; restart before editing")
    })
}
/// WPF core\audio-metadata.js inferFromFilename: underscores become spaces,
/// "Artist - Title" (hyphen, en or em dash) splits, otherwise artist "Custom".
pub(crate) fn names_from_filename(stem: &str) -> (String, String) {
    let tidy = |s: &str| {
        s.chars()
            .filter(|c| !c.is_control())
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(120)
            .collect::<String>()
    };
    let stem = tidy(&stem.replace('_', " "));
    for dash in [" - ", " \u{2013} ", " \u{2014} "] {
        if let Some((artist, title)) = stem.split_once(dash) {
            let (artist, title) = (tidy(artist), tidy(title));
            if !artist.is_empty() && !title.is_empty() {
                return (artist, title);
            }
        }
    }
    let title = if stem.is_empty() {
        "Custom Song".into()
    } else {
        stem
    };
    ("Custom".into(), title)
}
fn cancelled(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Acquire),
        "Native Jukebox operation cancelled"
    );
    Ok(())
}
fn selected(state: &State) -> Playback {
    Playback {
        phase: if state.active_playlist_id.is_some() {
            "selected"
        } else {
            "default"
        }
        .into(),
        message: if state.active_playlist_id.is_some() {
            "Playlist saved for the next Play launch; no running game was changed."
        } else {
            "Default leaves the installed soundtrack unchanged."
        }
        .into(),
        playlist_id: state.active_playlist_id.clone(),
        session: None,
        gameplay_verified: false,
    }
}
impl Shared {
    fn bump(status: &mut Status) {
        status.view.sequence = status.view.sequence.saturating_add(1);
    }
    fn refresh(&self, service: &Service) -> Result<()> {
        let mut s = lock(&self.status)?;
        s.view.revision = service.fingerprint()?;
        s.view.state = service.view().clone();
        if ["selected", "default", "unavailable"].contains(&s.view.playback.phase.as_str())
            && s.view.playback.session.is_none()
        {
            s.view.playback = selected(&s.view.state);
        }
        Self::bump(&mut s);
        Ok(())
    }
    fn progress(
        &self,
        phase: &str,
        message: impl Into<String>,
        done: Option<usize>,
        total: Option<usize>,
        frames: Option<u32>,
    ) -> Result<()> {
        let mut s = lock(&self.status)?;
        let j = s
            .view
            .job
            .as_mut()
            .context("Native Jukebox job disappeared")?;
        j.phase = phase.into();
        j.message = message.into();
        j.done = done;
        j.total = total;
        j.frames = frames;
        j.cancellable = phase != "saving";
        Self::bump(&mut s);
        Ok(())
    }
    /// Under the same status lock used to accept cancellation: once saving
    /// starts no new cancel can be acknowledged until that atomic commit ends.
    fn commit_boundary(&self, cancel: &AtomicBool, message: &str) -> Result<()> {
        let mut s = lock(&self.status)?;
        cancelled(cancel)?;
        let j = s
            .view
            .job
            .as_mut()
            .context("Native Jukebox job disappeared")?;
        j.phase = "saving".into();
        j.message = message.into();
        j.cancellable = false;
        j.frames = None;
        Self::bump(&mut s);
        Ok(())
    }
    fn finish(&self, result: Result<String>, cancel: &AtomicBool) {
        if let Ok(mut s) = self.status.lock() {
            if let Some(j) = s.view.job.as_mut() {
                j.cancellable = false;
                j.frames = None;
                match result {
                    Ok(message) => {
                        j.phase = "completed".into();
                        j.message = message;
                        j.done = j.total;
                        j.error = None;
                    }
                    Err(error) => {
                        j.phase = if cancel.load(Ordering::Acquire) {
                            "cancelled"
                        } else {
                            "failed"
                        }
                        .into();
                        j.message = format!(
                            "{} of {} songs/files completed. Existing successful library edits remain saved; uncommitted work was discarded.",
                            j.done.unwrap_or(0),
                            j.total.unwrap_or(0)
                        );
                        j.error = Some(format!("{error:#}").chars().take(4096).collect());
                    }
                }
            }
            s.busy = false;
            s.cancel = None;
            Self::bump(&mut s);
            self.done.notify_all();
        }
    }
}
struct Reservation<'a>(&'a Shared);
impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        if let Ok(mut s) = self.0.status.lock() {
            s.busy = false;
            self.0.done.notify_all();
        }
    }
}
impl Api {
    /// Call off the UI thread. `stock` MUST come from pinned V2 read_stock(),
    /// never renderer metadata. API opens only the supplied portable library.
    pub fn open(
        portable: &Path,
        stock: catalog::Music,
        game_root: Option<PathBuf>,
        host: Arc<dyn Host>,
    ) -> Result<Self> {
        let service = Service::open(portable, stock.clone(), &nonce()?)?;
        let state = service.view().clone();
        let preview = host.preview_enabled();
        let view = View {
            schema: "nba2k27-jukebox-view-v1",
            sequence: 1,
            revision: service.fingerprint()?,
            playback: selected(&state),
            state,
            stock_keys: stock.tracks.iter().map(|t| t.key()).collect(),
            capabilities: Capabilities {
                import_audio: true,
                import_playlists: true,
                export_playlists: true,
                custom_preview: preview,
                stock_preview: preview && game_root.is_some(),
                native_playback: false,
                max_session_slots: catalog::MAIN_SLOTS,
                artwork_editing: true,
                artwork_lookup: host.artwork_lookup_enabled(),
                artwork_preview: preview,
                playlist_reordering: true,
                native_artwork_publication: false,
            },
            job: None,
            warnings: vec![],
        };
        Ok(Self {
            shared: Arc::new(Shared {
                service: Mutex::new(service),
                status: Mutex::new(Status {
                    view,
                    busy: false,
                    cancel: None,
                    leases: BTreeMap::new(),
                    prepared_art: BTreeMap::new(),
                }),
                done: Condvar::new(),
                closing: AtomicBool::new(false),
                stock,
                game_root,
                host,
            }),
            worker: Mutex::new(None),
        })
    }
    pub fn view(&self) -> Result<View> {
        Ok(lock(&self.shared.status)?.view.clone())
    }
    fn reserve(&self, revision: &str) -> Result<Reservation<'_>> {
        let mut s = lock(&self.shared.status)?;
        ensure!(
            !self.shared.closing.load(Ordering::Acquire),
            "Native Jukebox is shutting down"
        );
        ensure!(
            !s.busy,
            "Wait for the current native Jukebox action or cancel it first"
        );
        ensure!(
            revision == s.view.revision,
            "Jukebox library changed; refresh before editing"
        );
        s.busy = true;
        Ok(Reservation(&self.shared))
    }
    fn reply(&self, kind: &str, message: &str) -> Result<Value> {
        Ok(json!({"kind":kind,"view":self.view()?,"message":message}))
    }
    /// Metadata commands/preview generation perform bounded filesystem work;
    /// root dispatches those on spawn_blocking. get/cancel/release are fast and
    /// never wait for a decoding worker's service lock. One long job at a time.
    pub fn dispatch(&self, channel: &str, args: Value) -> Result<Value> {
        if channel == "jukebox:get" {
            return Ok(serde_json::to_value(self.view()?)?);
        }
        ensure!(
            serde_json::to_vec(&args)?.len() <= 1024 * 1024,
            "Jukebox command exceeds bounds"
        );
        let a: Args = serde_json::from_value(args.clone())?;
        if channel == "jukebox:release-artwork" {
            self.release_artwork(a.token.as_deref().context("Artwork token is required")?)?;
            return Ok(Value::Null);
        }
        if channel == "jukebox:open-folder" {
            let folder = lock(&self.shared.service)?.library_folder();
            self.shared.host.open_library_folder(&folder)?;
            return Ok(Value::Null);
        }
        if channel == "jukebox:find-missing" {
            let service = lock(&self.shared.service)?;
            let missing: Vec<Value> = service
                .missing_custom_tracks()
                .iter()
                .map(|t| json!({"key":t.key(),"title":t.name,"artist":t.artist}))
                .collect();
            return Ok(json!({"revision":service.fingerprint()?,"missing":missing}));
        }
        if channel == "jukebox:cancel-import" {
            return self.cancel(a.job_id.as_deref().context("Job identity is required")?);
        }
        if channel == "jukebox:release-preview" {
            self.release_preview(a.token.as_deref().context("Preview token is required")?)?;
            return Ok(Value::Null);
        }
        let revision = a
            .expected_revision
            .as_deref()
            .context("Expected Jukebox revision is required")?;
        if let Some(reply) = self.dispatch_artwork(channel, &a, revision)? {
            return Ok(reply);
        }
        if channel == "jukebox:preview" {
            ensure!(
                self.shared.host.preview_enabled(),
                "Native audio preview route is not connected yet; no preview was started"
            );
            let preview = self.prepare_preview(
                a.track_key
                    .as_deref()
                    .context("Track identity is required")?,
                revision,
            )?;
            if let Some(url) = self.shared.host.preview_url(&preview.token) {
                return Ok(json!({"token":preview.token,"url":url,"mimeType":preview.mime_type}));
            }
            self.release_preview(&preview.token)?;
            anyhow::bail!("Native preview route is unavailable");
        }
        match channel {
            "jukebox:import-audio" => {
                let options = AudioOptions::new(a.single.unwrap_or(false), a.title, a.artist)?;
                return self.start_job("audio-import", revision, a.playlist_id, options);
            }
            "jukebox:import-playlist" => {
                return self.start_job("playlist-import", revision, None, AudioOptions::default());
            }
            "jukebox:export-playlist" => {
                return self.start_job(
                    "playlist-export",
                    revision,
                    a.playlist_id,
                    AudioOptions::default(),
                );
            }
            "jukebox:refresh-bundle" => {
                return self.start_job(
                    "playlist-bundle",
                    revision,
                    a.playlist_id,
                    AudioOptions::default(),
                );
            }
            "jukebox:create-playlist"
            | "jukebox:repair-missing"
            | "jukebox:rename-playlist"
            | "jukebox:set-order"
            | "jukebox:set-active"
            | "jukebox:delete-playlist"
            | "jukebox:edit-track"
            | "jukebox:reorder-playlists" => {}
            _ => anyhow::bail!("Unsupported native Jukebox command"),
        }
        let _reservation = self.reserve(revision)?;
        let mut service = lock(&self.shared.service)?;
        ensure!(
            service.fingerprint()? == revision,
            "Native Jukebox snapshot differs; refresh before editing"
        );
        let n = nonce()?;
        if channel == "jukebox:repair-missing" {
            let removed = service.remove_missing_tracks(
                &a.track_keys
                    .context("Missing song identities are required")?,
                &n,
            )?;
            self.shared.refresh(&service)?;
            return self.reply(
                "state",
                &format!("Removed {removed} missing song(s) from the library and its playlists. Audio files were already absent; nothing else was deleted."),
            );
        }
        match channel {
            "jukebox:create-playlist" => service.create_playlist_with_defaults(
                a.name.as_deref().context("Playlist name is required")?,
                &n,
                a.add_default_songs.unwrap_or(false),
            )?,
            "jukebox:reorder-playlists" => service
                .reorder_playlists(&a.playlist_ids.context("Playlist order is required")?, &n)?,
            "jukebox:edit-track" => {
                let key = a
                    .track_key
                    .as_deref()
                    .context("Track identity is required")?;
                let title = a.title.as_deref().context("Song title is required")?;
                let artist = a.artist.as_deref().context("Song artist is required")?;
                service.check_track_edit(key, title, artist)?;
                let art = self.consume_artwork(key, revision, a.artwork_token.as_deref())?;
                service.edit_track(
                    key,
                    title,
                    artist,
                    art.as_ref().map(|p| (&p.native[..], &p.preview[..])),
                    &n,
                )?;
            }
            "jukebox:rename-playlist" => service.rename_playlist(
                a.playlist_id
                    .as_deref()
                    .context("Playlist identity is required")?,
                a.name.as_deref().context("Playlist name is required")?,
                &n,
            )?,
            "jukebox:set-order" => service.set_order(
                a.playlist_id
                    .as_deref()
                    .context("Playlist identity is required")?,
                a.track_keys.context("Song order is required")?,
                &n,
            )?,
            "jukebox:set-active" => {
                ensure!(
                    args.as_object()
                        .is_some_and(|o| o.contains_key("playlistId")),
                    "Explicit playlist or null is required"
                );
                if let Some(id) = a.playlist_id.as_deref() {
                    let playlist = service
                        .view()
                        .playlists
                        .iter()
                        .find(|p| p.id == id)
                        .context("Playlist is missing")?;
                    ensure!(
                        !playlist.track_keys.is_empty(),
                        "Add a song before choosing this playlist for Play"
                    );
                    let music = service.view().music.selected(&playlist.track_keys)?;
                    catalog::plan_with_stock(&music, &self.shared.stock)?;
                }
                service.set_active(a.playlist_id.as_deref(), &n)?;
            }
            "jukebox:delete-playlist" => service.delete_playlist(
                a.playlist_id
                    .as_deref()
                    .context("Playlist identity is required")?,
                &n,
            )?,
            _ => unreachable!(),
        }
        self.shared.refresh(&service)?;
        self.reply("state", "Saved to the launcher library. Changes apply on the next Play launch; a running game was not changed.")
    }
    fn start_job(
        &self,
        kind: &str,
        revision: &str,
        playlist: Option<String>,
        options: AudioOptions,
    ) -> Result<Value> {
        // A job admitted during a running/held game stays native-only even if
        // that game later exits while the file picker is open. Retry a codec
        // conversion explicitly after cleanup; do not silently upgrade it.
        let decode_admission = self
            .shared
            .host
            .decode_admission()
            .unwrap_or(admission::Admission::NativeOnly);
        let mut worker = lock(&self.worker)?;
        if let Some(previous) = worker.as_ref() {
            ensure!(
                previous.is_finished(),
                "Previous native worker is still stopping"
            );
        }
        if let Some(previous) = worker.take() {
            previous
                .join()
                .map_err(|_| anyhow::anyhow!("Previous native worker failed"))?;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let id = nonce()?;
        {
            let mut s = lock(&self.shared.status)?;
            ensure!(
                !self.shared.closing.load(Ordering::Acquire) && !s.busy,
                "Native Jukebox is busy or shutting down"
            );
            ensure!(
                s.view.revision == revision,
                "Jukebox library changed; refresh before importing/exporting"
            );
            if kind != "playlist-import" {
                ensure!(
                    s.view
                        .state
                        .playlists
                        .iter()
                        .any(|p| Some(&p.id) == playlist.as_ref()),
                    "Choose a playlist before importing/exporting"
                );
            }
            s.busy = true;
            s.cancel = Some(cancel.clone());
            s.view.job = Some(Job {
                id: id.clone(),
                kind: kind.into(),
                phase: "choosing".into(),
                message: "Choose a local file in the native dialog.".into(),
                done: None,
                total: None,
                frames: None,
                cancellable: true,
                error: None,
            });
            Shared::bump(&mut s);
        }
        let shared = self.shared.clone();
        let kind = kind.to_string();
        let expected = revision.to_string();
        let worker_cancel = cancel.clone();
        let thread = thread::Builder::new()
            .name("native-jukebox-job".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_job(
                        &shared,
                        &kind,
                        playlist.as_deref(),
                        &expected,
                        &id,
                        &worker_cancel,
                        decode_admission,
                        &options,
                    )
                }));
                match result {
                    Ok(JobResult::PickerCancelled) => {
                        if let Ok(mut s) = shared.status.lock() {
                            if let Some(j) = s.view.job.as_mut() {
                                j.phase = "cancelled".into();
                                j.message = "File selection cancelled; library unchanged.".into();
                                j.cancellable = false;
                            }
                            s.busy = false;
                            s.cancel = None;
                            Shared::bump(&mut s);
                            shared.done.notify_all();
                        }
                    }
                    Ok(JobResult::Finished(result)) => shared.finish(result, &worker_cancel),
                    Err(_) => shared.finish(
                        Err(anyhow::anyhow!(
                            "Native Jukebox worker failed; no success was assumed"
                        )),
                        &worker_cancel,
                    ),
                }
            });
        match thread {
            Ok(thread) => *worker = Some(thread),
            Err(error) => {
                self.shared.finish(Err(error.into()), &cancel);
                anyhow::bail!("Unable to start the bounded native Jukebox worker");
            }
        }
        // The worker may complete before this reply. That is still truthful:
        // return state for a terminal outcome rather than a fake started job.
        let view = self.view()?;
        let active = view.job.as_ref().is_some_and(Job::active);
        let cancelled_picker = view.job.as_ref().is_some_and(|j| j.phase == "cancelled");
        if let Some(job) = view.job.as_ref().filter(|j| j.phase == "failed") {
            anyhow::bail!("{}", job.error.as_deref().unwrap_or(&job.message));
        }
        Ok(
            json!({"kind":if active {"started"} else if cancelled_picker {"cancelled"} else {"state"},"view":view,
            "message":if active {"Native import/export started."} else {"Native operation finished; inspect its final status below."}}),
        )
    }
    fn cancel(&self, id: &str) -> Result<Value> {
        let mut s = lock(&self.shared.status)?;
        let j = s
            .view
            .job
            .as_ref()
            .context("No native import/export is active")?;
        ensure!(
            j.id == id && j.active() && j.cancellable,
            "This native job is complete, foreign, or already committing; no cancellation was accepted"
        );
        s.cancel
            .as_ref()
            .context("Native cancellation owner is missing")?
            .store(true, Ordering::Release);
        if let Some(j) = s.view.job.as_mut() {
            j.message =
                "Cancellation requested; waiting for the owned helper/dialog to stop.".into();
            j.cancellable = false;
        }
        Shared::bump(&mut s);
        Ok(
            json!({"kind":"state","view":s.view,"message":"Cancellation requested; the operation is not yet confirmed stopped."}),
        )
    }
    /// Root enables this only after its real prepare/activate/recover lifecycle
    /// is wired. Library functionality alone never advertises game playback.
    pub fn set_native_playback_available(&self, available: bool) -> Result<()> {
        let mut s = lock(&self.shared.status)?;
        if s.view.capabilities.native_playback == available {
            return Ok(());
        }
        s.view.capabilities.native_playback = available;
        Shared::bump(&mut s);
        Ok(())
    }
    pub fn set_preview_available(&self) -> Result<()> {
        let mut s = lock(&self.shared.status)?;
        s.view.capabilities.custom_preview = self.shared.host.preview_enabled();
        s.view.capabilities.stock_preview =
            self.shared.host.preview_enabled() && self.shared.game_root.is_some();
        s.view.capabilities.artwork_preview = self.shared.host.preview_enabled();
        Shared::bump(&mut s);
        Ok(())
    }
    pub fn add_warning(&self, warning: &str) -> Result<()> {
        ensure!(warning.len() <= 4096, "Jukebox warning exceeds bounds");
        let mut s = lock(&self.shared.status)?;
        if !s.view.warnings.iter().any(|w| w == warning) {
            s.view.warnings.push(warning.into());
            Shared::bump(&mut s);
        }
        Ok(())
    }
    /// Nonblocking close-request hook. Dialog closure must additionally happen
    /// on its owning UI apartment through the host runtime.
    pub fn cancel_pending(&self) {
        self.shared.closing.store(true, Ordering::Release);
        if let Ok(mut s) = self.shared.status.lock() {
            s.leases.clear();
            s.prepared_art.clear();
            if let Some(cancel) = &s.cancel {
                cancel.store(true, Ordering::Release);
            }
        }
    }
    /// Only the owning provider calls this after its verified lifecycle step.
    /// It is not a renderer command and cannot perform/invent a game write.
    pub fn set_playback(
        &self,
        phase: &str,
        message: &str,
        session: Option<&str>,
        playlist: Option<&str>,
    ) -> Result<()> {
        ensure!(
            [
                "unavailable",
                "default",
                "selected",
                "preparing",
                "prepared",
                "active",
                "restoring",
                "held"
            ]
            .contains(&phase),
            "Unknown Jukebox session phase"
        );
        ensure!(
            message.len() <= 4096,
            "Jukebox session message is oversized"
        );
        let mut s = lock(&self.shared.status)?;
        s.view.playback = Playback {
            phase: phase.into(),
            message: message.into(),
            session: session.map(str::to_owned),
            playlist_id: playlist.map(str::to_owned),
            gameplay_verified: false,
        };
        Shared::bump(&mut s);
        Ok(())
    }
    /// `native` is passed directly by the trusted main command from
    /// Backend.session.status(), never from renderer arguments or a path read
    /// by this API. Status is display authority only, never file-write authority.
    pub fn sync_native_status(&self, native: &Value, integrated: bool) -> Result<()> {
        let mut s = lock(&self.shared.status)?;
        let phase = native["phase"].as_str().unwrap_or("unknown");
        let reported = &native["nativeProvider"]["jukebox"];
        let reported_phase = reported["phase"].as_str();
        let outer = native["session"].as_str();
        let provider = native["nativeProvider"]["session"].as_str();
        let owned = outer.is_some_and(|id| catalog::hex(id, 48))
            && outer == provider
            && native["nativeProvider"]["schema"] == "nba2k27-native-provider-session-v1";
        let next = if phase == "needs-review" || owned && reported_phase == Some("held") {
            let message=(if owned {reported["message"].as_str()} else {None}).or(native["error"].as_str()).unwrap_or("Native session recovery needs attention; game playlist activation is not confirmed.");
            Playback {
                phase: "held".into(),
                message: message.chars().take(4096).collect(),
                playlist_id: None,
                session: None,
                gameplay_verified: false,
            }
        } else if matches!(phase, "idle" | "closed") {
            selected(&s.view.state)
        } else if owned && reported_phase == Some("default") {
            Playback { phase:"default".into(),message:reported["message"].as_str().unwrap_or("This game uses the original soundtrack. Saved playlist changes apply on the next Play launch.").chars().take(4096).collect(),playlist_id:None,session:outer.map(str::to_owned),gameplay_verified:false }
        } else if let Some(p) =
            reported_phase.filter(|p| ["preparing", "prepared", "active", "restoring"].contains(p))
        {
            let playlist = reported["playlistId"].as_str();
            let message = reported["message"].as_str();
            if owned
                && playlist.is_some_and(|id| catalog::hex(id, 48))
                && message.is_some_and(|m| m.len() <= 4096)
            {
                Playback {
                    phase: p.into(),
                    message: message.unwrap().into(),
                    playlist_id: playlist.map(str::to_owned),
                    session: outer.map(str::to_owned),
                    gameplay_verified: reported["gameplayVerified"] == true,
                }
            } else {
                Playback{phase:"held".into(),message:"Native playlist status has no matching retained session identity; activation is not confirmed.".into(),playlist_id:None,session:None,gameplay_verified:false}
            }
        } else {
            selected(&s.view.state)
        };
        if s.view.playback != next || s.view.capabilities.native_playback != integrated {
            s.view.playback = next;
            s.view.capabilities.native_playback = integrated;
            Shared::bump(&mut s);
        }
        Ok(())
    }
    pub fn seal_launch(&self) -> Result<LaunchSeal> {
        let s = lock(&self.shared.status)?;
        ensure!(
            !s.busy && !self.shared.closing.load(Ordering::Acquire),
            "Wait for native Jukebox preparation before Play"
        );
        Ok(LaunchSeal {
            revision: s.view.revision.clone(),
            playlist_id: s.view.state.active_playlist_id.clone(),
        })
    }
    /// Off-UI, before Steam, with fresh exact game-closed callback. This creates
    /// a durable owned Store, not activation. Root must record its outer marker
    /// and activate/verify it, then restore before detaching the Mods junction.
    pub fn prepare_session(
        &self,
        seal: &LaunchSeal,
        game: &Path,
        mods: &Path,
        session: &str,
        closed: impl Fn() -> Result<()>,
    ) -> Result<Option<Prepared>> {
        let _reservation = self.reserve(&seal.revision)?;
        let service = lock(&self.shared.service)?;
        ensure!(
            service.fingerprint()? == seal.revision
                && service.view().active_playlist_id == seal.playlist_id,
            "Jukebox changed after Play; launch cancelled before game writes"
        );
        service.prepare_session(game, mods, session, closed)
    }
    pub fn prepare_preview(&self, key: &str, revision: &str) -> Result<Preview> {
        let _reservation = self.reserve(revision)?;
        let (mime, bytes) = if self.shared.stock.tracks.iter().any(|t| t.key() == key) {
            let game = self
                .shared
                .game_root
                .as_ref()
                .context("Stock preview requires the verified game folder")?;
            let (_, native) =
                jukebox_v2_resources::read_stock_native(game, &self.shared.stock, key)?;
            match audio::inspect(&native)?.codec {
                audio::Codec::Pcm => ("audio/wav", audio::pcm_wave(&native[54..])?),
                audio::Codec::Opus => ("audio/ogg", audio::preview_ogg(&native, 0)?),
            }
        } else {
            lock(&self.shared.service)?.preview_audio(key)?
        };
        ensure!(
            bytes.len() <= MAX_PREVIEW_TOTAL,
            "Audio preview exceeds its bounded cache"
        );
        let token = nonce()?;
        let now = Instant::now();
        let mut s = lock(&self.shared.status)?;
        s.leases.retain(|_, l| l.expires > now);
        ensure!(
            s.leases
                .values()
                .filter(|l| matches!(l.kind, artwork::Kind::Audio))
                .count()
                < 2
                && s.leases
                    .values()
                    .filter(|l| matches!(l.kind, artwork::Kind::Audio))
                    .map(|l| l.data.len())
                    .sum::<usize>()
                    + bytes.len()
                    <= MAX_PREVIEW_TOTAL,
            "Release the previous audio preview before opening another"
        );
        let preview = Preview {
            token: token.clone(),
            mime_type: mime,
            size: bytes.len(),
        };
        s.leases.insert(
            token,
            Lease {
                mime,
                data: bytes.into(),
                expires: now + PREVIEW_TTL,
                kind: artwork::Kind::Audio,
            },
        );
        Ok(preview)
    }
    /// Host HTTP handler supplies parsed bounded byte ranges. Opaque token only:
    /// there is no path, arbitrary URL, decompressor or filesystem read here.
    pub fn read_preview(&self, token: &str, start: usize, length: usize) -> Result<PreviewChunk> {
        ensure!(
            catalog::hex(token, 48) && length <= 4 * 1024 * 1024,
            "Invalid preview token/range"
        );
        let mut s = lock(&self.shared.status)?;
        s.leases.retain(|_, l| l.expires > Instant::now());
        let lease = s
            .leases
            .get_mut(token)
            .context("Preview expired or was released")?;
        lease.expires = Instant::now()
            + if matches!(lease.kind, artwork::Kind::Audio) {
                PREVIEW_TTL
            } else {
                artwork::TTL
            };
        ensure!(
            start <= lease.data.len(),
            "Preview range is outside the audio"
        );
        let end = start.saturating_add(length).min(lease.data.len());
        Ok(PreviewChunk {
            mime_type: lease.mime,
            total: lease.data.len(),
            bytes: lease.data[start..end].to_vec(),
        })
    }
    pub fn release_preview(&self, token: &str) -> Result<()> {
        ensure!(catalog::hex(token, 48), "Invalid preview token");
        lock(&self.shared.status)?.leases.remove(token);
        Ok(())
    }
    /// Returns within the supplied budget (max2sec). A held picker/I/O is an
    /// explicit error, never an unbounded UI close delay. Host picker adapters
    /// must honour the token; native codec children already have kill-on-close.
    pub fn shutdown(&self, budget: Duration) -> Result<()> {
        ensure!(
            budget <= Duration::from_secs(2),
            "Shutdown budget exceeds two seconds"
        );
        self.shared.closing.store(true, Ordering::Release);
        let start = Instant::now();
        let mut s = lock(&self.shared.status)?;
        s.leases.clear();
        s.prepared_art.clear();
        if let Some(c) = &s.cancel {
            c.store(true, Ordering::Release);
        }
        while s.busy {
            let left = budget.saturating_sub(start.elapsed());
            ensure!(
                !left.is_zero(),
                "Native Jukebox is still stopping its owned dialog/I/O; shutdown budget reached"
            );
            s = self
                .shared
                .done
                .wait_timeout(s, left)
                .map_err(|_| anyhow::anyhow!("Native Jukebox shutdown lock poisoned"))?
                .0;
        }
        drop(s);
        let mut worker = lock(&self.worker)?;
        // busy=false is a status notification, not thread termination: the
        // worker may still own the final Shared and its pinned library handles.
        // Do not report shutdown complete until those handles have been dropped.
        while worker.as_ref().is_some_and(|w| !w.is_finished()) {
            let left = budget.saturating_sub(start.elapsed());
            ensure!(
                !left.is_zero(),
                "Native Jukebox worker is still releasing its owned resources; shutdown budget reached"
            );
            thread::sleep(left.min(Duration::from_millis(1)));
        }
        if let Some(worker) = worker.take() {
            worker
                .join()
                .map_err(|_| anyhow::anyhow!("Native Jukebox worker panicked"))?;
        }
        Ok(())
    }
}
impl Drop for Api {
    fn drop(&mut self) {
        self.shared.closing.store(true, Ordering::Release);
        if let Ok(s) = self.shared.status.lock() {
            if let Some(c) = &s.cancel {
                c.store(true, Ordering::Release);
            }
        }
    }
}

enum JobResult {
    PickerCancelled,
    Finished(Result<String>),
}
fn run_job(
    shared: &Shared,
    kind: &str,
    playlist: Option<&str>,
    expected: &str,
    id: &str,
    cancel: &Arc<AtomicBool>,
    admitted: admission::Admission,
    options: &AudioOptions,
) -> JobResult {
    let result = (|| -> Result<Option<String>> {
        cancelled(cancel)?;
        // Revision was reserved before opening the picker and cannot be edited
        // by this API while the job is active. Service's disk CAS also detects
        // a second launcher modifying the portable library behind our back.
        ensure!(
            lock(&shared.service)?.fingerprint()? == expected,
            "Jukebox library changed before preparation"
        );
        match kind {
            "audio-import" => {
                let Some(paths) = (if options.single {
                    shared.host.choose_single_audio(cancel)
                } else {
                    shared.host.choose_audio(cancel)
                })?
                else {
                    return Ok(None);
                };
                ensure!(
                    !paths.is_empty() && paths.len() <= MAX_BATCH,
                    "Choose between one and100 audio files per import"
                );
                ensure!(
                    !options.single || paths.len() == 1,
                    "Choose exactly one file in single-song mode"
                );
                // WPF batch add: a bad file is reported and the rest continue
                // ("Added X of Y"). Cancellation still stops the whole job.
                let mut failures = Vec::new();
                for (i, path) in paths.iter().enumerate() {
                    cancelled(cancel)?;
                    let one = (|| -> Result<()> {
                        shared.progress(
                            "reading",
                            format!("Reading song {} of {}", i + 1, paths.len()),
                            Some(i),
                            Some(paths.len()),
                            None,
                        )?;
                        ensure!(
                            path.extension().and_then(|s| s.to_str()).is_some_and(|s| [
                                "mp3", "wav", "m4a", "aac", "flac", "ogg", "opus"
                            ]
                            .iter()
                            .any(|e| s.eq_ignore_ascii_case(e))),
                            "Use MP3, WAV, M4A, AAC, FLAC, OGG or Opus audio; original retained"
                        );
                        let mut source = Source::open(path, audio::MAX_AUDIO as u64)?;
                        let bytes = source.read_all(audio::MAX_AUDIO, cancel)?;
                        let native = match audio::wrap(&bytes) {
                            Ok(native) => native,
                            Err(_) => {
                                admitted.require_decoder()?;
                                shared.host.decode_admission()?.require_decoder()?;
                                shared.progress(
                                    "decoding",
                                    format!("Decoding song {} of {}", i + 1, paths.len()),
                                    Some(i),
                                    Some(paths.len()),
                                    Some(0),
                                )?;
                                let pcm = shared.host.decode(
                                    &bytes,
                                    &nonce()?,
                                    cancel.clone(),
                                    &mut |frames| {
                                        cancelled(cancel)?;
                                        shared.progress(
                                            "decoding",
                                            format!("Decoding song {} of {}", i + 1, paths.len()),
                                            Some(i),
                                            Some(paths.len()),
                                            Some(frames),
                                        )
                                    },
                                )?;
                                audio::native_pcm(&pcm)?
                            }
                        };
                        let (artist, title) = names_from_filename(
                            &path
                                .file_stem()
                                .context("Audio filename is missing")?
                                .to_string_lossy(),
                        );
                        let title = options.title.as_deref().unwrap_or(&title);
                        let artist = options.artist.as_deref().unwrap_or(&artist);
                        shared.commit_boundary(
                            cancel,
                            "Saving validated native audio to the portable library.",
                        )?;
                        let mut service = lock(&shared.service)?;
                        service.import_audio(
                            playlist.context("Playlist is missing")?,
                            title,
                            artist,
                            &native,
                            &nonce()?,
                        )?;
                        shared.refresh(&service)?;
                        drop(service);
                        shared.progress(
                            "reading",
                            format!("Saved {} of {} songs", i + 1, paths.len()),
                            Some(i + 1),
                            Some(paths.len()),
                            None,
                        )?;
                        Ok(())
                    })();
                    if let Err(error) = one {
                        cancelled(cancel)?;
                        if options.single {
                            return Err(error);
                        }
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        failures.push(format!("{name}: {error:#}"));
                    }
                }
                let added = paths.len() - failures.len();
                ensure!(
                    added > 0,
                    "{}",
                    failures
                        .first()
                        .map_or("No songs were added", String::as_str)
                );
                Ok(Some(if let Some(first) = failures.first() {
                    format!("Added {added} of {} songs. {first}", paths.len())
                } else {
                    format!(
                        "Imported {added} songs into the launcher library. Game playback changes only on the next Play launch."
                    )
                }))
            }
            "playlist-import" => {
                let Some(path) = shared.host.choose_playlist(cancel)? else {
                    return Ok(None);
                };
                ensure!(
                    path.extension()
                        .and_then(|s| s.to_str())
                        .is_some_and(|s| s.eq_ignore_ascii_case("2kplaylist")),
                    "Choose a complete .2kplaylist file"
                );
                shared.progress(
                    "reading",
                    "Validating the complete playlist before any catalog commit.",
                    Some(0),
                    Some(1),
                    None,
                )?;
                let mut source = Source::open(&path, 256 * 1024 * 1024 * 1024 + 4 * 1024 * 1024)?;
                let mut magic = [0; 12];
                source.file.read_exact(&mut magic)?;
                source.file.seek(SeekFrom::Start(0))?;
                let mut decode = |encoded: &[u8], _: usize| {
                    cancelled(cancel)?;
                    admitted.require_decoder()?;
                    shared.host.decode_admission()?.require_decoder()?;
                    shared.progress(
                        "decoding",
                        "Decoding a validated playlist song.",
                        Some(0),
                        Some(1),
                        Some(0),
                    )?;
                    shared
                        .host
                        .decode(encoded, &nonce()?, cancel.clone(), &mut |frames| {
                            cancelled(cancel)?;
                            shared.progress(
                                "decoding",
                                "Decoding a validated playlist song.",
                                Some(0),
                                Some(1),
                                Some(frames),
                            )
                        })
                };
                let imported = if &magic == b"2KPLAYLIST2\n" {
                    let mut container = package::Container::open(CheckedRead {
                        file: &mut source.file,
                        cancel,
                    })?;
                    let title = container.manifest().name.clone();
                    let mut songs = vec![];
                    container.visit(&shared.stock, &mut decode, |song| {
                        cancelled(cancel)?;
                        songs.push(song);
                        Ok(())
                    })?;
                    package::Import {
                        name: title,
                        songs,
                        warnings: vec![],
                    }
                } else {
                    let bytes = source.read_all(package::MAX_SEGMENT, cancel)?;
                    package::import_segment(&bytes, &shared.stock, &mut decode)?
                };
                source.verify()?;
                cancelled(cancel)?;
                shared.commit_boundary(
                    cancel,
                    "Saving the complete validated playlist to the portable library.",
                )?;
                let warnings = imported.warnings.clone();
                let count = imported.songs.len();
                let mut service = lock(&shared.service)?;
                service.accept_import(imported, id)?;
                shared.refresh(&service)?;
                {
                    let mut s = lock(&shared.status)?;
                    s.view.warnings = warnings;
                    Shared::bump(&mut s);
                }
                Ok(Some(format!(
                    "Imported {count} playlist songs. The complete playlist is now saved in the launcher library."
                )))
            }
            "playlist-export" => {
                let title = {
                    let service = lock(&shared.service)?;
                    service
                        .view()
                        .playlists
                        .iter()
                        .find(|p| Some(p.id.as_str()) == playlist)
                        .context("Playlist is missing")?
                        .name
                        .clone()
                };
                let safe: String = title
                    .chars()
                    .filter(|c| !c.is_control() && !"<>:\"/\\|?*".contains(*c))
                    .take(70)
                    .collect();
                let Some(path) = shared.host.choose_export(
                    &format!(
                        "{}.2kplaylist",
                        if safe.is_empty() { "Playlist" } else { &safe }
                    ),
                    cancel,
                )?
                else {
                    return Ok(None);
                };
                cancelled(cancel)?;
                let mut output = Export::new(&path, id)?;
                shared.progress(
                    "exporting",
                    "Verifying saved songs and building the portable playlist.",
                    Some(0),
                    Some(1),
                    None,
                )?;
                let bytes = lock(&shared.service)?
                    .export_package(playlist.context("Playlist is missing")?)?;
                for chunk in bytes.chunks(1024 * 1024) {
                    cancelled(cancel)?;
                    output.file.write_all(chunk)?;
                }
                output.file.sync_all()?;
                shared.commit_boundary(
                    cancel,
                    "Publishing the completed playlist without replacing any existing file.",
                )?;
                output.publish(&path)?;
                Ok(Some(
                    "Playlist export completed. The source library was not changed.".into(),
                ))
            }
            "playlist-bundle" => {
                let (title, folder) = {
                    let service = lock(&shared.service)?;
                    let title = service
                        .view()
                        .playlists
                        .iter()
                        .find(|p| Some(p.id.as_str()) == playlist)
                        .context("Playlist is missing")?
                        .name
                        .clone();
                    (title, service.library_folder())
                };
                let folder = crate::jukebox_local::ensure_child(&folder, "Playlists")?;
                let path = folder.join(bundle_name(&title));
                cancelled(cancel)?;
                let mut output = Export::new_replacing(&path, id)?;
                shared.progress(
                    "exporting",
                    "Verifying saved songs and rebuilding the playlist share bundle.",
                    Some(0),
                    Some(1),
                    None,
                )?;
                let bytes = lock(&shared.service)?
                    .export_package(playlist.context("Playlist is missing")?)?;
                for chunk in bytes.chunks(1024 * 1024) {
                    cancelled(cancel)?;
                    output.file.write_all(chunk)?;
                }
                output.file.sync_all()?;
                shared.commit_boundary(
                    cancel,
                    "Replacing the launcher-owned share bundle in Library/Jukebox/Playlists.",
                )?;
                output.publish_replacing(&path)?;
                Ok(Some(format!(
                    "Share bundle refreshed: Library/Jukebox/Playlists/{}. The library was not changed.",
                    bundle_name(&title)
                )))
            }
            _ => anyhow::bail!("Unknown native Jukebox job kind"),
        }
    })();
    match result {
        Ok(None) => JobResult::PickerCancelled,
        other => JobResult::Finished(other.map(|s| s.unwrap())),
    }
}
/// Launcher-owned bundle filename: printable, no path/stream characters.
fn bundle_name(title: &str) -> String {
    let safe: String = title
        .chars()
        .filter(|c| !c.is_control() && !"<>:\"/\\|?*".contains(*c))
        .take(70)
        .collect();
    let safe = safe.trim().trim_end_matches('.').trim();
    format!(
        "{}.2kplaylist",
        if safe.is_empty() { "Playlist" } else { safe }
    )
}
struct CheckedRead<'a> {
    file: &'a mut File,
    cancel: &'a AtomicBool,
}
impl Read for CheckedRead<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.cancel.load(Ordering::Acquire) {
            return Err(std::io::Error::other("Playlist import cancelled"));
        }
        self.file.read(buf)
    }
}
impl Seek for CheckedRead<'_> {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        if self.cancel.load(Ordering::Acquire) {
            return Err(std::io::Error::other("Playlist import cancelled"));
        }
        self.file.seek(to)
    }
}

// Local ordinary-file adapters retain ancestors and the exact file handle.
// Imports deny concurrent write/deletion. Exports publish a fresh temporary
// handle with ReplaceIfExists=false, and cleanup deletes only that same handle.
struct Parents(Vec<File>);
impl Parents {
    fn pin(path: &Path) -> Result<Self> {
        ensure!(path.is_absolute(), "Choose an absolute local file path");
        let mut current = PathBuf::new();
        let mut pins = vec![];
        for c in path.components() {
            ensure!(
                !matches!(c, Component::ParentDir | Component::CurDir),
                "Unnormalized Jukebox file path"
            );
            if let Component::Prefix(p) = c {
                ensure!(
                    matches!(p.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)),
                    "Network/device paths are not supported for native Jukebox imports"
                );
            }
            current.push(c.as_os_str());
            if matches!(c, Component::Prefix(_)) {
                continue;
            }
            let f = fs::OpenOptions::new()
                .read(true)
                .share_mode(3)
                .custom_flags(0x02000000 | 0x00200000)
                .open(&current)?;
            ordinary(&f, true)?;
            pins.push(f);
        }
        Ok(Self(pins))
    }
}
fn ordinary(f: &File, dir: bool) -> Result<()> {
    let m = f.metadata()?;
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    unsafe {
        GetFileInformationByHandle(HANDLE(f.as_raw_handle()), &mut info)?;
    }
    ensure!(
        m.file_attributes() & 0x400 == 0
            && if dir {
                m.is_dir()
            } else {
                m.is_file() && info.nNumberOfLinks == 1
            },
        "Jukebox file is linked/nonordinary; left untouched"
    );
    Ok(())
}
fn file_id(f: &File) -> Result<(u64, [u8; 16])> {
    let mut id = FILE_ID_INFO::default();
    unsafe {
        GetFileInformationByHandleEx(
            HANDLE(f.as_raw_handle()),
            FileIdInfo,
            &mut id as *mut _ as _,
            size_of::<FILE_ID_INFO>() as u32,
        )?;
    }
    Ok((id.VolumeSerialNumber, id.FileId.Identifier))
}
struct Source {
    file: File,
    _parents: Parents,
    id: (u64, [u8; 16]),
    length: u64,
    modified: u64,
}
impl Source {
    fn open(path: &Path, max: u64) -> Result<Self> {
        let parents = Parents::pin(path.parent().context("Source folder is missing")?)?;
        let file = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .custom_flags(0x00200000)
            .open(path)?;
        ordinary(&file, false)?;
        let meta = file.metadata()?;
        ensure!(
            (1..=max).contains(&meta.len()),
            "Source file exceeds the supported bounded import size"
        );
        Ok(Self {
            id: file_id(&file)?,
            length: meta.len(),
            modified: meta.last_write_time(),
            file,
            _parents: parents,
        })
    }
    fn verify(&self) -> Result<()> {
        ordinary(&self.file, false)?;
        let m = self.file.metadata()?;
        ensure!(
            file_id(&self.file)? == self.id
                && m.len() == self.length
                && m.last_write_time() == self.modified,
            "Source file changed during native import; no catalog commit attempted"
        );
        Ok(())
    }
    fn read_all(&mut self, max: usize, cancel: &AtomicBool) -> Result<Vec<u8>> {
        ensure!(
            self.length <= max as u64,
            "Source exceeds the bounded import size"
        );
        self.file.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::with_capacity(self.length as usize);
        let mut buffer = vec![0; 1024 * 1024];
        loop {
            cancelled(cancel)?;
            let n = self.file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            ensure!(
                bytes.len() + n <= max,
                "Source grew beyond its import bound"
            );
            bytes.extend_from_slice(&buffer[..n]);
        }
        self.verify()?;
        ensure!(bytes.len() as u64 == self.length, "Source length differs");
        Ok(bytes)
    }
}
struct Export {
    file: File,
    _parents: Parents,
    published: bool,
}
impl Export {
    fn new(path: &Path, id: &str) -> Result<Self> {
        ensure!(
            path.extension()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("2kplaylist")),
            "Export filename must end with .2kplaylist"
        );
        let parent = path.parent().context("Export folder is missing")?;
        let parents = Parents::pin(parent)?;
        match fs::symlink_metadata(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
            Ok(_) => anyhow::bail!(
                "Export destination already exists; choose a new filename. Nothing was replaced"
            ),
        }
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .access_mode(0xc0010000)
            .share_mode(1)
            .create_new(true)
            .custom_flags(0x00200000)
            .open(parent.join(format!(".jukebox-export-{id}.tmp")))?;
        ordinary(&file, false)?;
        Ok(Self {
            file,
            _parents: parents,
            published: false,
        })
    }
    /// Refresh-bundle variant: the destination is inside the launcher-owned
    /// Playlists folder and may already hold this playlist's previous bundle.
    /// Only an ordinary single-link file is ever replaced.
    fn new_replacing(path: &Path, id: &str) -> Result<Self> {
        match fs::symlink_metadata(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::new(path, id),
            Err(e) => Err(e.into()),
            Ok(meta) => {
                ensure!(
                    meta.is_file() && meta.file_attributes() & 0x400 == 0,
                    "Existing share bundle is not an ordinary file; nothing was replaced"
                );
                let existing = fs::OpenOptions::new()
                    .read(true)
                    .share_mode(1)
                    .custom_flags(0x00200000)
                    .open(path)?;
                ordinary(&existing, false)?;
                drop(existing);
                let parent = path.parent().context("Export folder is missing")?;
                let parents = Parents::pin(parent)?;
                let file = fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .access_mode(0xc0010000)
                    .share_mode(1)
                    .create_new(true)
                    .custom_flags(0x00200000)
                    .open(parent.join(format!(".jukebox-export-{id}.tmp")))?;
                ordinary(&file, false)?;
                Ok(Self {
                    file,
                    _parents: parents,
                    published: false,
                })
            }
        }
    }
    fn publish_replacing(&mut self, to: &Path) -> Result<()> {
        self.rename_to(to, true)
    }
    fn publish(&mut self, to: &Path) -> Result<()> {
        self.rename_to(to, false)
    }
    fn rename_to(&mut self, to: &Path, replace: bool) -> Result<()> {
        ordinary(&self.file, false)?;
        let name: Vec<u16> = to.as_os_str().encode_wide().collect();
        ensure!(
            to.is_absolute() && !name.is_empty() && name.len() <= 32760 && !name.contains(&0),
            "Invalid playlist export destination"
        );
        let size = size_of::<FILE_RENAME_INFO>() + (name.len() + 1) * 2;
        let mut buffer = vec![0u64; size.div_ceil(8)];
        let info = buffer.as_mut_ptr() as *mut FILE_RENAME_INFO;
        unsafe {
            (*info).Anonymous.ReplaceIfExists = replace;
            (*info).FileNameLength = (name.len() * 2) as u32;
            std::ptr::copy_nonoverlapping(name.as_ptr(), (*info).FileName.as_mut_ptr(), name.len());
            SetFileInformationByHandle(
                HANDLE(self.file.as_raw_handle()),
                FileRenameInfo,
                buffer.as_mut_ptr() as _,
                size as u32,
            )?;
        }
        self.published = true;
        Ok(())
    }
}
impl Drop for Export {
    fn drop(&mut self) {
        if !self.published {
            let info = FILE_DISPOSITION_INFO {
                DeleteFile: true.into(),
            };
            unsafe {
                let _ = SetFileInformationByHandle(
                    HANDLE(self.file.as_raw_handle()),
                    FileDispositionInfo,
                    &info as *const _ as _,
                    size_of::<FILE_DISPOSITION_INFO>() as u32,
                );
            }
        }
    }
}

#[cfg(test)]
#[path = "jukebox_api_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "jukebox_edit_tests.rs"]
mod edit_tests;
