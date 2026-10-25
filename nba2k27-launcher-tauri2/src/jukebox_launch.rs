//! Pointer-free, immutable Jukebox next-Play/provider bridge. Intentionally
//! unwired until the native provider records this marker, indexes publication
//! files and releases its cached handles before owned restoration. No HTTP,
//! dialogs, volume control, Steam, process opens or Freestyle addresses here.
#![allow(dead_code)]
use crate::{
    jukebox_api::Api,
    jukebox_catalog::{self as catalog, AudioPolicy, Music},
    jukebox_files::{self, Store},
    jukebox_local::Document,
    jukebox_service::{Service, State},
    jukebox_v2_resources,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::Read,
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Path, PathBuf},
};
use windows::Win32::{
    Foundation::HANDLE,
    Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle},
};

const SCHEMA: &str = "nba2k27-rust-jukebox-launch-v1";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Selection {
    pub revision: String,
    pub stock_sha256: String,
    pub playlist_id: String,
    pub track_keys: Vec<String>,
    pub selected_sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Plan {
    schema: String,
    #[serde(default)]
    pub selection: Option<Selection>,
}
impl Default for Plan {
    fn default() -> Self {
        Self {
            schema: SCHEMA.into(),
            selection: None,
        }
    }
}
fn digest_state(state: &State) -> Result<String> {
    Ok(catalog::digest(&serde_json::to_vec(state)?))
}
fn digest_music(music: &Music) -> Result<String> {
    Ok(catalog::digest(&serde_json::to_vec(music)?))
}
/// Launch preparation can freeze an existing saved library without creating
/// the UI runtime or HTTP server. If the API is already loaded, root first
/// calls Plan::freeze(api), which additionally rejects an active import job.
pub(crate) fn freeze_saved(portable: &Path) -> Result<Plan> {
    let path = portable.join("Data/Jukebox");
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Plan::default()),
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    let document = Document::open(&path)?;
    let (bytes, _) = document.load()?;
    let state: State = serde_json::from_slice(&bytes.context(
        "Saved Jukebox folder has no complete library document; no default was created",
    )?)?;
    Plan::from_state(&state, &digest_state(&state)?)
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SelectionSummary {
    pub library_ready: bool,
    pub playlist_count: usize,
    pub playlist_id: Option<String>,
    pub playlist_name: Option<String>,
    pub song_count: usize,
    pub revision: Option<String>,
    pub playlists: Vec<PlaylistChoice>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlaylistChoice {
    pub id: String,
    pub name: String,
    pub song_count: usize,
}

pub(crate) fn summarize_state(state: &State) -> Result<SelectionSummary> {
    ensure!(
        state.schema == "nba2k27-rust-jukebox-library-v1" && state.playlists.len() <= 100,
        "Saved Jukebox summary has an unsupported library schema"
    );
    let mut ids = BTreeSet::new();
    for playlist in &state.playlists {
        ensure!(
            catalog::hex(&playlist.id, 48)
                && ids.insert(playlist.id.as_str())
                && !playlist.name.trim().is_empty()
                && playlist.name == playlist.name.trim()
                && playlist.name.chars().count() <= 80
                && !playlist.name.chars().any(char::is_control)
                && playlist.track_keys.len() <= catalog::MAX_TRACKS,
            "Saved Jukebox summary contains an invalid playlist"
        );
        let mut keys = BTreeSet::new();
        ensure!(
            playlist
                .track_keys
                .iter()
                .all(|track| key(track) && keys.insert(track.as_str())),
            "Saved Jukebox summary contains invalid song identities"
        );
    }
    let selected = match state.active_playlist_id.as_deref() {
        Some(id) => Some(
            state
                .playlists
                .iter()
                .find(|playlist| playlist.id == id)
                .context("Saved next-Play Jukebox playlist is missing")?,
        ),
        None => None,
    };
    if selected.is_some() {
        // A displayed next-Play choice must satisfy the same exact main-bank,
        // count, catalog-membership and fingerprint checks used by Play.
        Plan::from_state(state, &digest_state(state)?)?;
    }
    Ok(SelectionSummary {
        library_ready: true,
        playlist_count: state.playlists.len(),
        playlist_id: selected.map(|playlist| playlist.id.clone()),
        playlist_name: selected.map(|playlist| playlist.name.clone()),
        song_count: selected.map_or(0, |playlist| playlist.track_keys.len()),
        revision: Some(digest_state(state)?),
        playlists: state
            .playlists
            .iter()
            .map(|playlist| PlaylistChoice {
                id: playlist.id.clone(),
                name: playlist.name.clone(),
                song_count: playlist.track_keys.len(),
            })
            .collect(),
    })
}

/// Lightweight Play-page summary. It reads only the existing bounded portable
/// library document; it never decodes the game's stock music, starts preview
/// HTTP, opens a picker, creates storage, or touches a game process.
pub(crate) fn selection_summary(portable: &Path) -> Result<SelectionSummary> {
    let path = portable.join("Data/Jukebox");
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(SelectionSummary {
                library_ready: false,
                playlist_count: 0,
                playlist_id: None,
                playlist_name: None,
                song_count: 0,
                revision: None,
                playlists: vec![],
            });
        }
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    let document = Document::open(&path)?;
    let (bytes, _) = document.load()?;
    let state: State = serde_json::from_slice(&bytes.context(
        "Saved Jukebox folder has no complete library document; open Jukebox to review it",
    )?)?;
    summarize_state(&state)
}

/// Save only the next-Play choice in an existing library. A running session
/// retains its sealed plan; no game resources or heavy music runtime are opened.
pub(crate) fn select_saved(
    portable: &Path,
    playlist: Option<&str>,
    expected: &str,
) -> Result<SelectionSummary> {
    ensure!(
        catalog::hex(expected, 64),
        "Refresh the saved Jukebox choices before selecting a playlist"
    );
    let document = Document::open(&portable.join("Data/Jukebox"))?;
    let (bytes, revision) = document.load()?;
    let mut state: State = serde_json::from_slice(
        &bytes.context("Open Jukebox to create or import a playlist first")?,
    )?;
    summarize_state(&state)?;
    ensure!(
        digest_state(&state)? == expected,
        "Jukebox library changed; refresh before editing"
    );
    state.active_playlist_id = playlist.map(str::to_owned);
    let summary = summarize_state(&state)?;
    if summary.revision.as_deref() != Some(expected) {
        document.commit(
            &revision,
            &serde_json::to_vec_pretty(&state)?,
            &crate::backend::unique_id(),
        )?;
    }
    Ok(summary)
}
fn key(value: &str) -> bool {
    let pieces = value.split(':').collect::<Vec<_>>();
    pieces.len() == 3
        && pieces[0] == "1"
        && pieces.iter().all(|p| {
            !p.is_empty()
                && p.len() <= 5
                && p.bytes().all(|b| b.is_ascii_digit())
                && p.parse::<u16>().is_ok()
        })
}
impl Plan {
    pub fn freeze(api: &Api) -> Result<Self> {
        let seal = api.seal_launch()?;
        let view = api.view()?;
        ensure!(
            view.revision == seal.revision && view.state.active_playlist_id == seal.playlist_id,
            "Jukebox changed while freezing Play; retry before launching"
        );
        Self::from_state(&view.state, &seal.revision)
    }
    /// The native backend calls this on its authoritative Service state, never
    /// renderer JSON. Every field is rederived on the provider side before I/O.
    pub fn from_state(state: &State, revision: &str) -> Result<Self> {
        ensure!(
            digest_state(state)? == revision,
            "Jukebox snapshot fingerprint differs"
        );
        let Some(id) = &state.active_playlist_id else {
            return Ok(Self::default());
        };
        let playlist = state
            .playlists
            .iter()
            .find(|p| &p.id == id)
            .context("Selected Jukebox playlist is missing")?;
        let music = state.music.selected(&playlist.track_keys)?;
        let result = Self {
            schema: SCHEMA.into(),
            selection: Some(Selection {
                revision: revision.into(),
                stock_sha256: state.stock_sha256.clone(),
                playlist_id: id.clone(),
                track_keys: playlist.track_keys.clone(),
                selected_sha256: digest_music(&music)?,
            }),
        };
        result.validate()?;
        Ok(result)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema == SCHEMA, "Unsupported Jukebox launch schema");
        if let Some(s) = &self.selection {
            ensure!(
                [&s.revision, &s.stock_sha256, &s.selected_sha256]
                    .into_iter()
                    .all(|h| catalog::hex(h, 64))
                    && catalog::hex(&s.playlist_id, 48),
                "Invalid Jukebox launch identity"
            );
            let mut keys = BTreeSet::new();
            ensure!(
                (1..=71).contains(&s.track_keys.len())
                    && s.track_keys.iter().all(|k| key(k) && keys.insert(k)),
                "Jukebox Play requires1–71 distinct main soundtrack songs; no truncation occurred"
            );
        }
        ensure!(
            serde_json::to_vec(self)?.len() <= 16 * 1024,
            "Jukebox launch plan exceeds bounds"
        );
        Ok(())
    }
    pub fn enabled(&self) -> bool {
        self.selection.is_some()
    }
    /// Exact full-path stock-first exception. It consumes one of the existing
    /// four general override positions; scoreboard fixed paths use their own
    /// proven flag. No directory_listing.adb2 or leaf-filename override.
    pub fn stock_overrides(&self) -> Vec<String> {
        if self.enabled() {
            vec!["music.iff".into()]
        } else {
            vec![]
        }
    }
    fn compare(&self, state: &State, stock: &Music) -> Result<AudioPolicy> {
        self.validate()?;
        let s = self
            .selection
            .as_ref()
            .context("Jukebox launch is disabled")?;
        ensure!(
            digest_state(state)? == s.revision
                && state.stock_sha256 == s.stock_sha256
                && catalog::digest(&serde_json::to_vec(stock)?) == s.stock_sha256,
            "Jukebox library/stock changed after Play; launch stopped before publication"
        );
        ensure!(
            state.active_playlist_id.as_ref() == Some(&s.playlist_id),
            "Jukebox playlist choice changed after Play"
        );
        let p = state
            .playlists
            .iter()
            .find(|p| p.id == s.playlist_id)
            .context("Sealed Jukebox playlist is missing")?;
        ensure!(
            p.track_keys == s.track_keys,
            "Jukebox song order changed after Play"
        );
        let music = state.music.selected(&s.track_keys)?;
        ensure!(
            digest_music(&music)? == s.selected_sha256,
            "Sealed Jukebox song metadata differs"
        );
        Ok(catalog::plan_with_stock(&music, stock)?.policy)
    }
}
/// Independent authorizations: prepare checks parent/cancel AND game-closed;
/// rollback checks game/protected-process closure but must not be vetoed merely
/// because the launch was cancelled. Neither callback is supplied by the UI.
pub(crate) struct Checks<'a> {
    pub prepare: &'a dyn Fn() -> Result<()>,
    /// Bounded read-only cancellation/source-seal check. Mutations must still
    /// use `prepare` immediately before publication.
    pub poll: &'a dyn Fn() -> Result<()>,
    pub cleanup: &'a dyn Fn() -> Result<()>,
}
struct SealGuard {
    portable: PathBuf,
    revision: String,
}
impl SealGuard {
    fn state(&self) -> Result<State> {
        // Existing-only document access. A missing/half-written library cannot
        // initialize a default catalog during a sealed provider launch.
        let document = Document::open(&self.portable.join("Data/Jukebox"))?;
        let (bytes, _) = document.load()?;
        let bytes = bytes.context("Sealed Jukebox library is missing; no default was created")?;
        let state: State = serde_json::from_slice(&bytes)?;
        ensure!(
            digest_state(&state)? == self.revision,
            "Jukebox library changed during launch preparation"
        );
        Ok(state)
    }
    fn verify(&self) -> Result<()> {
        self.state().map(|_| ())
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Publication {
    pub logical: String,
    pub path: PathBuf,
    pub size: usize,
    pub sha256: String,
}
pub(crate) struct Session {
    store: Store,
    seal: SealGuard,
    mods: PathBuf,
    pub policy: AudioPolicy,
    pub warnings: Vec<String>,
    /// Prepared-playlist cache reuse for this launch (diagnostic only).
    pub cache: crate::jukebox_prepared::Report,
}
impl Session {
    pub fn verify_before_launch(&self, check: impl Fn() -> Result<()>) -> Result<()> {
        check()?;
        self.seal.verify()?;
        self.store.verify_current()?;
        check()?;
        self.seal.verify()
    }
    pub fn verify_current(&self) -> Result<()> {
        self.store.verify_current()
    }
    /// Provider indexes these ordinary immutable published files using bounded
    /// chunk reads, not set_alias (whose64MiB bound is smaller than valid PCM).
    /// Call only after activation. Revalidate before adding the complete group;
    /// never adopt an existing same-name foreign asset as a Jukebox payload.
    pub fn publication(&self) -> Result<Vec<Publication>> {
        self.store.verify_current()?;
        let mut paths = vec!["music.iff".to_string()];
        for (i, b) in self.policy.custom.bytes().enumerate() {
            if b == b'1' {
                paths.push(catalog::audio_path((i + 1) as u16));
            }
        }
        paths.extend(self.store.cover_targets());
        let mut out = vec![];
        for logical in paths {
            let path = self.mods.join(&logical);
            let mut f = fs::OpenOptions::new()
                .read(true)
                .share_mode(1)
                .custom_flags(0x00200000)
                .open(&path)?;
            ordinary(&f)?;
            let before = f.metadata()?;
            let max = if logical == "music.iff" {
                catalog::MAX_IFF
            } else if logical == crate::jukebox_cover_route::CONTAINER {
                crate::jukebox_cover_route::MAX_CONTAINER
            } else if crate::jukebox_cover_route::texture_path(&logical) {
                crate::jukebox_cover_route::MAX_TEXTURE
            } else {
                crate::jukebox_audio::MAX_AUDIO
            };
            ensure!(
                (1..=max as u64).contains(&before.len()),
                "Jukebox publication exceeds its native size bound"
            );
            let mut sha = Sha256::new();
            let mut buf = vec![0; 1024 * 1024];
            let mut size = 0;
            loop {
                let n = f.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                size += n;
                ensure!(size <= max, "Jukebox publication grew during indexing");
                sha.update(&buf[..n]);
            }
            let after = f.metadata()?;
            ensure!(
                size as u64 == before.len()
                    && after.len() == before.len()
                    && after.last_write_time() == before.last_write_time(),
                "Jukebox publication changed during indexing"
            );
            out.push(Publication {
                logical,
                path,
                size,
                sha256: format!("{:x}", sha.finalize()),
            });
        }
        self.store.verify_current()?;
        Ok(out)
    }
    /// Owned cover route files (container first); empty without custom art.
    pub fn cover_paths(&self) -> Vec<String> {
        self.store.cover_targets()
    }
    /// The container keeps its stock name, so the stock-first loader must ask
    /// the provider first for it. Textures sit at paths stock lacks.
    pub fn cover_overrides(&self) -> Vec<String> {
        if self.cover_paths().is_empty() {
            vec![]
        } else {
            vec![crate::jukebox_cover_route::CONTAINER.into()]
        }
    }
    /// Root must stop the exact game/DLL provider reads and release any cached
    /// publication handles before this call, while the physical Mods identity
    /// still exists. Never detach its mod-profile junction first.
    pub fn restore(&self, closed: impl Fn() -> Result<()>) -> Result<()> {
        self.store.restore(closed)
    }
}
fn ordinary(f: &File) -> Result<()> {
    let m = f.metadata()?;
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    unsafe {
        GetFileInformationByHandle(HANDLE(f.as_raw_handle()), &mut info)?;
    }
    ensure!(
        m.is_file() && m.file_attributes() & 0x400 == 0 && info.nNumberOfLinks == 1,
        "Jukebox publication is linked/nonordinary"
    );
    Ok(())
}
pub(crate) fn pending(portable: &Path) -> Result<bool> {
    match fs::symlink_metadata(portable.join("Data").join(jukebox_files::MARKER)) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}
/// Root records its outer Jukebox pending bit BEFORE this call. Independently
/// pinned Version2 disk evidence is required, never a renderer-enabled gate.
/// All published files are restored on a preparation error when independently
/// safe; a foreign file/running game leaves exact owned receipts for recovery.
pub(crate) fn prepare(
    portable: &Path,
    game: &Path,
    mods: &Path,
    session: &str,
    plan: &Plan,
    checks: Checks<'_>,
) -> Result<Option<Session>> {
    plan.validate()?;
    (checks.prepare)()?;
    ensure!(
        !pending(portable)?,
        "Previous Jukebox session needs exact owned recovery before another launch"
    );
    if !plan.enabled() {
        return Ok(None);
    }
    let selection = plan.selection.as_ref().unwrap();
    let seal = SealGuard {
        portable: portable.to_owned(),
        revision: selection.revision.clone(),
    };
    seal.verify()?;
    // Full process/ownership proof brackets the large read-only image hash.
    // The 16 MiB polling callback remains bounded but deliberately performs no
    // process enumeration; prepare_with_stock repeats the full proof before I/O.
    // A validated prepared-playlist cache may skip only proofs an earlier
    // launch completed on the identical files (see jukebox_prepared).
    (checks.prepare)()?;
    let cache = crate::jukebox_prepared::Cache::load(portable);
    let (stock, proof, stock_reused) = jukebox_v2_resources::read_stock_with(
        game,
        || {
            (checks.poll)()?;
            seal.verify()
        },
        cache.stock(),
    )?;
    (checks.prepare)()?;
    let mut prepared = prepare_inner(
        portable,
        game,
        mods,
        session,
        plan,
        &stock.music,
        checks,
        &cache.verified,
    )?;
    if let Some(prepared) = &mut prepared {
        // Best effort: a cache that cannot be saved only costs the next launch
        // a full rebuild; it never blocks or alters this owned publication.
        let saved = cache.save(portable, proof, session).is_ok();
        prepared.cache = cache.report(stock_reused, saved);
    }
    Ok(prepared)
}
fn prepare_with_stock(
    portable: &Path,
    game: &Path,
    mods: &Path,
    session: &str,
    plan: &Plan,
    stock: &Music,
    checks: Checks<'_>,
) -> Result<Option<Session>> {
    prepare_inner(
        portable,
        game,
        mods,
        session,
        plan,
        stock,
        checks,
        &Default::default(),
    )
}
#[allow(clippy::too_many_arguments)]
fn prepare_inner(
    portable: &Path,
    game: &Path,
    mods: &Path,
    session: &str,
    plan: &Plan,
    stock: &Music,
    checks: Checks<'_>,
    known: &crate::jukebox_prepared::Verified,
) -> Result<Option<Session>> {
    plan.validate()?;
    (checks.prepare)()?;
    ensure!(
        !pending(portable)?,
        "Previous Jukebox session needs exact owned recovery before another launch"
    );
    let Some(selection) = &plan.selection else {
        return Ok(None);
    };
    ensure!(
        catalog::hex(session, 48),
        "Invalid Jukebox provider session"
    );
    let seal = SealGuard {
        portable: portable.to_owned(),
        revision: selection.revision.clone(),
    };
    let state = seal.state()?;
    let expected_policy = plan.compare(&state, stock)?;
    let service = Service::open(portable, stock.clone(), session)?;
    plan.compare(service.view(), stock)?;
    let checked = || {
        (checks.prepare)()?;
        seal.verify()?;
        (checks.prepare)()
    };
    let result = (|| -> Result<Session> {
        let prepared = service
            .prepare_session_with(game, mods, session, checked, known)?
            .context("Sealed playlist unexpectedly became Default")?;
        ensure!(
            prepared.policy == expected_policy,
            "Prepared native Jukebox masks differ from sealed selection"
        );
        prepared.store.activate(checked)?;
        checked()?;
        prepared.store.verify_current()?;
        Ok(Session {
            store: prepared.store,
            seal: SealGuard {
                portable: portable.to_owned(),
                revision: selection.revision.clone(),
            },
            mods: mods.canonicalize()?,
            policy: prepared.policy,
            warnings: prepared.warnings,
            cache: Default::default(),
        })
    })();
    match result {
        Ok(session) => Ok(Some(session)),
        Err(error) => {
            let recovery = (|| -> Result<()> {
                (checks.cleanup)()?;
                if let Some(store) = Store::recover(portable, mods, session)? {
                    store.restore(checks.cleanup)?;
                }
                Ok(())
            })();
            match recovery {
                Ok(()) => {
                    Err(error
                        .context("Jukebox preparation failed; verified owned files were restored"))
                }
                Err(cleanup) => Err(anyhow::anyhow!(
                    "Jukebox preparation failed: {error:#}. Exact owned recovery remains pending: {cleanup:#}"
                )),
            }
        }
    }
}
/// Ended/cancelled/restarted-worker recovery uses the original outer session
/// and physical Mods root. A wrong session/root cannot adopt the receipt.
pub(crate) fn recover_ended(
    portable: &Path,
    mods: &Path,
    session: &str,
    closed: impl Fn() -> Result<()>,
) -> Result<bool> {
    ensure!(
        catalog::hex(session, 48),
        "Invalid Jukebox recovery session"
    );
    closed()?;
    let Some(store) = Store::recover(portable, mods, session)? else {
        return Ok(false);
    };
    closed()?;
    store.restore(closed)?;
    Ok(true)
}
/// Append only the four proven fixed loader fields. The source parser in
/// version_proxy.cpp enforces71/54/71 masks and custom⊆main; never combine the
/// unproven Freestyle experiment or directory-listing override with this path.
pub(crate) fn append_loader_config(base: &[u8], policy: Option<&AudioPolicy>) -> Result<Vec<u8>> {
    ensure!(
        !base.is_empty()
            && base.len() <= 2047
            && base.is_ascii()
            && base.ends_with(b"\r\n")
            && !base.contains(&0),
        "Invalid bounded native loader configuration"
    );
    let Some(policy) = policy else {
        return Ok(base.to_vec());
    };
    policy.validate()?;
    let text = std::str::from_utf8(base)?;
    for line in text.lines() {
        let key = line
            .split('=')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_uppercase();
        ensure!(
            !key.starts_with("AUDIO_") && !key.starts_with("FREESTYLE_"),
            "Existing audio/Freestyle loader directive conflicts with native Jukebox"
        );
    }
    let mut result = base.to_vec();
    result.extend(policy.loader_config()?.bytes());
    ensure!(
        result.len() <= 2047,
        "Jukebox loader fields exceed2047bytes; shorten portable path/overrides before Play"
    );
    Ok(result)
}
/// WPF core\freestyle-playlist.js wording, shown when the playlist mixes in
/// stock songs. Like WPF (main.js:1750) it stops Play before Steam.
pub(crate) const FREESTYLE_MIXED: &str = "Use playlist in Freestyle currently supports imported songs without stock songs mixed in. Uncheck it in Jukebox to play a mixed playlist in the main menu only.";
/// WPF useFreestylePlaylist: only a playlist of 1–71 imported songs
/// (`custom == main`) can replace Freestyle's event-992 music.
pub(crate) fn freestyle_eligible(policy: &AudioPolicy) -> Result<()> {
    policy.validate()?;
    let count = policy.custom.bytes().filter(|&b| b == b'1').count();
    ensure!(
        policy.custom == policy.main && (1..=71).contains(&count),
        FREESTYLE_MIXED
    );
    Ok(())
}
/// WPF sampleFreestyleMask: every imported slot, or a uniform random 58 of
/// them when there are more (the native parser accepts at most 58).
pub(crate) fn freestyle_selection(
    policy: &AudioPolicy,
    mut random: impl FnMut(usize) -> Result<usize>,
) -> Result<String> {
    freestyle_eligible(policy)?;
    let mut slots = policy
        .custom
        .bytes()
        .enumerate()
        .filter_map(|(i, b)| (b == b'1').then_some(i))
        .collect::<Vec<_>>();
    if slots.len() > 58 {
        for i in (1..slots.len()).rev() {
            let j = random(i + 1)?;
            ensure!(j <= i, "Freestyle sample index out of range");
            slots.swap(i, j);
        }
    }
    let mut mask = [b'0'; 71];
    for &i in slots.iter().take(58) {
        mask[i] = b'1';
    }
    Ok(String::from_utf8(mask.to_vec())?)
}
/// Uniform integer in 0..bound from the OS RNG (WPF crypto.randomInt).
fn os_random(bound: usize) -> Result<usize> {
    use windows::Win32::Security::Cryptography::{
        BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
    };
    ensure!(
        (1..=u32::MAX as usize).contains(&bound),
        "Invalid random bound"
    );
    let bound = bound as u64;
    let limit = (1u64 << 32) - (1u64 << 32) % bound;
    loop {
        let mut bytes = [0; 4];
        unsafe { BCryptGenRandom(None, &mut bytes, BCRYPT_USE_SYSTEM_PREFERRED_RNG).ok()? };
        let value = u32::from_le_bytes(bytes) as u64;
        if value < limit {
            return Ok((value % bound) as usize);
        }
    }
}
/// WPF version-proxy-session.js:293 order: the two Freestyle lines follow the
/// AUDIO_* lines from append_loader_config. Only the V2 outfit proxy carries
/// the Version 2 FreestyleBuildProfile, so the caller gates on it.
pub(crate) fn append_freestyle_config(config: &[u8], policy: &AudioPolicy) -> Result<Vec<u8>> {
    append_freestyle_with(config, policy, os_random)
}
fn append_freestyle_with(
    config: &[u8],
    policy: &AudioPolicy,
    random: impl FnMut(usize) -> Result<usize>,
) -> Result<Vec<u8>> {
    let text = std::str::from_utf8(config)?;
    ensure!(
        config.is_ascii()
            && text.ends_with("\r\n")
            && text
                .lines()
                .any(|l| l == format!("AUDIO_CUSTOM_MAIN={}", policy.custom))
            && !text
                .lines()
                .any(|l| l.to_ascii_uppercase().starts_with("FREESTYLE_")),
        "Freestyle playlist needs the prepared Jukebox loader fields first"
    );
    let selection = freestyle_selection(policy, random)?;
    let mut result = config.to_vec();
    result.extend(format!("FREESTYLE_PLAYLIST=992\r\nFREESTYLE_SELECTION={selection}\r\n").bytes());
    ensure!(
        result.len() <= 2047,
        "Jukebox and Freestyle loader fields exceed 2047 bytes; shorten portable path/overrides before Play"
    );
    Ok(result)
}

#[cfg(test)]
#[path = "jukebox_launch_tests.rs"]
pub(crate) mod tests;
