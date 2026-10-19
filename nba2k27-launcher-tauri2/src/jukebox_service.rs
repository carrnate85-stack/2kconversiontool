//! End-to-end Rust Jukebox backend. Unwired feature service: portable library
//! import/order/persistence, package sharing and owned native-session files.
//! No UI, Node/.NET processes, OS volume control, or game-memory pointers.
#![allow(dead_code)]
use crate::{
    jukebox_audio,
    jukebox_catalog::{self as catalog, Music},
    jukebox_files::{self, NativeSource},
    jukebox_local::{self as local, Document, Revision},
    jukebox_package::{self as package, Import},
    jukebox_v2_resources,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Playlist {
    pub id: String,
    pub name: String,
    pub track_keys: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Blob {
    pub sha256: String,
    pub size: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Audio {
    pub native: Blob,
    #[serde(default)]
    pub artwork: Option<Blob>,
    #[serde(default)]
    pub preview: Option<Blob>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Artwork {
    pub native: Blob,
    pub preview: Blob,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct State {
    pub schema: String,
    pub stock_sha256: String,
    pub music: Music,
    pub playlists: Vec<Playlist>,
    pub active_playlist_id: Option<String>,
    pub audio: BTreeMap<String, Audio>,
    /// User cover edits are independent of audio ownership, including stock
    /// tracks. Old libraries omit this field and retain identical fingerprints.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub artwork_overrides: BTreeMap<String, Artwork>,
}
fn name(value: &str) -> Result<()> {
    ensure!(
        !value.trim().is_empty()
            && value == value.trim()
            && value.chars().count() <= 80
            && !value.chars().any(char::is_control),
        "Use a printable playlist name of1–80 characters"
    );
    Ok(())
}
impl State {
    fn validate(&self, stock: &Music) -> Result<()> {
        ensure!(
            self.schema == "nba2k27-rust-jukebox-library-v1"
                && self.stock_sha256 == catalog::digest(&serde_json::to_vec(stock)?)
                && self.playlists.len() <= 100,
            "Jukebox catalog/game stock version differs; saved content retained"
        );
        self.music.validate()?;
        let keys: BTreeSet<_> = self.music.tracks.iter().map(|t| t.key()).collect();
        let stock_keys: BTreeSet<_> = stock.tracks.iter().map(|t| t.key()).collect();
        ensure!(
            stock_keys.is_subset(&keys),
            "Stock catalog identities were removed; library preserved for review"
        );
        let mut banks = BTreeSet::new();
        for t in &self.music.tracks {
            if t.collection == 1 {
                ensure!(
                    banks.insert(t.bank),
                    "Duplicate main soundtrack bank in library"
                );
            }
            if !stock_keys.contains(&t.key()) {
                ensure!(
                    t.custom() && t.bank < 999 && self.audio.contains_key(&t.key()),
                    "Custom song lacks a saved audio reference"
                );
            }
        }
        let (mut ids, mut names) = (BTreeSet::new(), BTreeSet::new());
        for p in &self.playlists {
            name(&p.name)?;
            ensure!(
                catalog::hex(&p.id, 48)
                    && ids.insert(&p.id)
                    && names.insert(p.name.to_lowercase())
                    && p.track_keys.len() <= catalog::MAX_TRACKS,
                "Invalid/duplicate playlist identity"
            );
            let mut selected = BTreeSet::new();
            for k in &p.track_keys {
                ensure!(
                    keys.contains(k) && selected.insert(k),
                    "Playlist has a missing/duplicate song"
                );
            }
        }
        ensure!(
            self.active_playlist_id
                .as_ref()
                .is_none_or(|id| ids.contains(id)),
            "Active playlist is missing"
        );
        for (k, a) in &self.audio {
            ensure!(
                keys.contains(k)
                    && !stock_keys.contains(k)
                    && catalog::hex(&a.native.sha256, 64)
                    && (54..=jukebox_audio::MAX_AUDIO).contains(&a.native.size),
                "Invalid private audio reference"
            );
            for b in [&a.artwork, &a.preview].into_iter().flatten() {
                ensure!(
                    catalog::hex(&b.sha256, 64) && b.size > 0 && b.size <= 4 * 1024 * 1024,
                    "Invalid private cover reference"
                );
            }
        }
        for (key, art) in &self.artwork_overrides {
            ensure!(
                keys.contains(key)
                    && catalog::hex(&art.native.sha256, 64)
                    && art.native.size == 32800
                    && catalog::hex(&art.preview.sha256, 64)
                    && (33..=4 * 1024 * 1024).contains(&art.preview.size),
                "Invalid private artwork override"
            );
        }
        Ok(())
    }
}
pub(crate) struct Service {
    portable: PathBuf,
    stock: Music,
    state: State,
    doc: Document,
    revision: Revision,
    audio: PathBuf,
    art: PathBuf,
}
pub(crate) struct Prepared {
    pub store: jukebox_files::Store,
    pub policy: catalog::AudioPolicy,
    pub warnings: Vec<String>,
}
impl Service {
    /// `stock` comes from independently pinned read_stock(), not renderer JSON.
    /// All meaningful data stays under the supplied portable folder.
    pub fn open(portable: &Path, stock: Music, nonce: &str) -> Result<Self> {
        stock.validate()?;
        let portable = portable.canonicalize()?;
        let data = local::ensure_child(&portable, "Data")?;
        let data = local::ensure_child(&data, "Jukebox")?;
        let library = local::ensure_child(&portable, "Library")?;
        let library = local::ensure_child(&library, "Jukebox")?;
        let audio = local::ensure_child(&library, "Audio")?;
        let art = local::ensure_child(&library, "Artwork")?;
        let doc = Document::open(&data)?;
        doc.recover()?;
        let (raw, revision) = doc.load()?;
        let initial = raw.is_none();
        let state = if let Some(raw) = raw {
            serde_json::from_slice(&raw)?
        } else {
            State {
                schema: "nba2k27-rust-jukebox-library-v1".into(),
                stock_sha256: catalog::digest(&serde_json::to_vec(&stock)?),
                music: stock.clone(),
                playlists: vec![],
                active_playlist_id: None,
                audio: BTreeMap::new(),
                artwork_overrides: BTreeMap::new(),
            }
        };
        let mut service = Self {
            portable,
            stock,
            state,
            doc,
            revision,
            audio,
            art,
        };
        service.state.validate(&service.stock)?;
        if initial {
            service.commit(service.state.clone(), nonce)?;
        }
        Ok(service)
    }
    pub fn view(&self) -> &State {
        &self.state
    }
    /// Seal this into the launch feature snapshot. A worker must compare it
    /// before preparing files, so edits made after Play never change a launch
    /// already being prepared; mismatch cancels without dispatching Steam.
    pub fn fingerprint(&self) -> Result<String> {
        Ok(catalog::digest(&serde_json::to_vec(&self.state)?))
    }
    fn commit(&mut self, next: State, nonce: &str) -> Result<()> {
        next.validate(&self.stock)?;
        let bytes = serde_json::to_vec_pretty(&next)?;
        let revision = self.doc.commit(&self.revision, &bytes, nonce)?;
        self.state = next;
        self.revision = revision;
        Ok(())
    }
    fn playlist<'a>(state: &'a mut State, id: &str) -> Result<&'a mut Playlist> {
        state
            .playlists
            .iter_mut()
            .find(|p| p.id == id)
            .context("Playlist is missing")
    }
    pub fn create_playlist(&mut self, title: &str, id: &str) -> Result<()> {
        self.create_playlist_with_defaults(title, id, false)
    }
    pub fn create_playlist_with_defaults(
        &mut self,
        title: &str,
        id: &str,
        default_songs: bool,
    ) -> Result<()> {
        name(title)?;
        ensure!(catalog::hex(id, 48), "Invalid playlist identity");
        let mut next = self.state.clone();
        next.playlists.push(Playlist {
            id: id.into(),
            name: title.into(),
            track_keys: if default_songs {
                self.stock
                    .tracks
                    .iter()
                    .filter(|t| t.collection == 1)
                    .map(|t| t.key())
                    .collect()
            } else {
                vec![]
            },
        });
        self.commit(next, id)
    }
    pub fn reorder_playlists(&mut self, ids: &[String], nonce: &str) -> Result<()> {
        let expected = self
            .state
            .playlists
            .iter()
            .map(|p| p.id.as_str())
            .collect::<BTreeSet<_>>();
        let given = ids.iter().map(String::as_str).collect::<BTreeSet<_>>();
        ensure!(
            ids.len() == self.state.playlists.len() && given == expected,
            "Playlist order must contain every existing ID exactly once"
        );
        let mut next = self.state.clone();
        next.playlists = ids
            .iter()
            .map(|id| {
                self.state
                    .playlists
                    .iter()
                    .find(|p| &p.id == id)
                    .unwrap()
                    .clone()
            })
            .collect();
        self.commit(next, nonce)
    }
    pub fn check_track_edit(&self, key: &str, title: &str, artist: &str) -> Result<()> {
        ensure!(
            self.state.music.tracks.iter().any(|t| t.key() == key),
            "Song is missing"
        );
        ensure!(
            !title.is_empty()
                && title == title.trim()
                && title.chars().count() <= 120
                && !artist.is_empty()
                && artist == artist.trim()
                && artist.chars().count() <= 120
                && !title.chars().chain(artist.chars()).any(char::is_control),
            "Use a printable song title and artist (up to 120 characters)"
        );
        Ok(())
    }
    pub fn edit_track(
        &mut self,
        key: &str,
        title: &str,
        artist: &str,
        art: Option<(&[u8], &[u8])>,
        nonce: &str,
    ) -> Result<()> {
        self.check_track_edit(key, title, artist)?;
        let mut next = self.state.clone();
        let track = next
            .music
            .tracks
            .iter_mut()
            .find(|t| t.key() == key)
            .context("Song is missing")?;
        track.name = title.into();
        track.artist = artist.into();
        if let Some((native, preview)) = art {
            let (sha256, size) = local::cache_art(&self.art, native, false, nonce)?;
            let native = Blob { sha256, size };
            let (sha256, size) = local::cache_art(&self.art, preview, true, nonce)?;
            next.artwork_overrides.insert(
                key.into(),
                Artwork {
                    native,
                    preview: Blob { sha256, size },
                },
            );
        }
        self.commit(next, nonce)
    }
    pub fn artwork_preview(&self, key: &str) -> Result<Option<Vec<u8>>> {
        ensure!(
            self.state.music.tracks.iter().any(|t| t.key() == key),
            "Song is missing"
        );
        let preview = self
            .state
            .artwork_overrides
            .get(key)
            .map(|a| &a.preview)
            .or_else(|| self.state.audio.get(key).and_then(|a| a.preview.as_ref()));
        preview
            .map(|b| local::read_art(&self.art, &b.sha256, b.size, true))
            .transpose()
    }
    pub fn artwork_native(&self, key: &str) -> Result<Option<Vec<u8>>> {
        ensure!(
            self.state.music.tracks.iter().any(|t| t.key() == key),
            "Song is missing"
        );
        let art = self
            .state
            .artwork_overrides
            .get(key)
            .map(|a| &a.native)
            .or_else(|| self.state.audio.get(key).and_then(|a| a.artwork.as_ref()));
        art.map(|b| local::read_art(&self.art, &b.sha256, b.size, false))
            .transpose()
    }
    /// Custom songs whose private audio blob is absent or no longer the
    /// recorded size. Bounded metadata checks only; full hashes stay at launch.
    pub fn missing_custom_tracks(&self) -> Vec<catalog::Track> {
        self.state
            .music
            .tracks
            .iter()
            .filter(|t| {
                self.state.audio.get(&t.key()).is_some_and(|a| {
                    let path = self
                        .audio
                        .join(format!("{}.wav", a.native.sha256.to_ascii_lowercase()));
                    !std::fs::symlink_metadata(path)
                        .is_ok_and(|m| m.is_file() && m.len() == a.native.size as u64)
                })
            })
            .cloned()
            .collect()
    }
    /// WPF "Repair missing tracks": drops only the listed songs, and only while
    /// each is still missing. Playlists keep their other songs; a Play choice
    /// left empty falls back to the default soundtrack.
    pub fn remove_missing_tracks(&mut self, keys: &[String], nonce: &str) -> Result<usize> {
        let missing: BTreeSet<String> = self
            .missing_custom_tracks()
            .iter()
            .map(|t| t.key())
            .collect();
        ensure!(
            !keys.is_empty() && keys.len() <= catalog::MAX_TRACKS,
            "Choose missing songs to repair"
        );
        ensure!(
            keys.iter().all(|k| missing.contains(k)),
            "A song's audio is available again; refresh before repairing"
        );
        let keys: BTreeSet<&String> = keys.iter().collect();
        let mut next = self.state.clone();
        next.music.tracks.retain(|t| !keys.contains(&t.key()));
        next.audio.retain(|k, _| !keys.contains(k));
        next.artwork_overrides.retain(|k, _| !keys.contains(k));
        for p in &mut next.playlists {
            p.track_keys.retain(|k| !keys.contains(k));
        }
        if let Some(active) = next.active_playlist_id.clone() {
            if next
                .playlists
                .iter()
                .any(|p| p.id == active && p.track_keys.is_empty())
            {
                next.active_playlist_id = None;
            }
        }
        self.commit(next, nonce)?;
        Ok(keys.len())
    }
    pub fn library_folder(&self) -> PathBuf {
        self.portable.join("Library/Jukebox")
    }
    pub fn rename_playlist(&mut self, id: &str, title: &str, nonce: &str) -> Result<()> {
        name(title)?;
        let mut next = self.state.clone();
        Self::playlist(&mut next, id)?.name = title.into();
        self.commit(next, nonce)
    }
    pub fn set_order(&mut self, id: &str, keys: Vec<String>, nonce: &str) -> Result<()> {
        let mut next = self.state.clone();
        Self::playlist(&mut next, id)?.track_keys = keys;
        self.commit(next, nonce)
    }
    pub fn set_active(&mut self, id: Option<&str>, nonce: &str) -> Result<()> {
        let mut next = self.state.clone();
        next.active_playlist_id = id.map(str::to_owned);
        self.commit(next, nonce)
    }
    pub fn delete_playlist(&mut self, id: &str, nonce: &str) -> Result<()> {
        let mut next = self.state.clone();
        ensure!(
            next.playlists.iter().any(|p| p.id == id),
            "Playlist is missing"
        );
        next.playlists.retain(|p| p.id != id);
        if next.active_playlist_id.as_deref() == Some(id) {
            next.active_playlist_id = None;
        }
        self.commit(next, nonce)
    }
    /// Input is a validated decoded WAV or established native Opus/PCM. The
    /// original source is never changed. MP3/AAC/etc decoding is a host adapter.
    pub fn import_audio(
        &mut self,
        playlist: &str,
        title: &str,
        artist: &str,
        input: &[u8],
        nonce: &str,
    ) -> Result<String> {
        let native = jukebox_audio::wrap(input)?;
        let cached = local::cache_native(&self.audio, &native, nonce)?;
        let mut next = self.state.clone();
        let fallback = self
            .stock
            .tracks
            .iter()
            .find(|t| t.collection == 1 && !t.cover.is_empty())
            .map(|t| t.cover.as_str())
            .unwrap_or("001_helicopter");
        let track = next.music.add_custom(title, artist, fallback)?;
        let key = track.key();
        next.audio.insert(
            key.clone(),
            Audio {
                native: Blob {
                    sha256: cached.sha256,
                    size: cached.size,
                },
                artwork: None,
                preview: None,
            },
        );
        Self::playlist(&mut next, playlist)?
            .track_keys
            .push(key.clone());
        self.commit(next, nonce)?;
        Ok(key)
    }
    pub fn import_package(
        &mut self,
        bytes: &[u8],
        nonce: &str,
        decode: impl FnMut(&[u8], usize) -> Result<Vec<u8>>,
    ) -> Result<String> {
        let imported = package::import_segment(bytes, &self.stock, decode)?;
        self.accept_import(imported, nonce)
    }
    /// Direct usable encoded-import path: root passes current_exe(), an owned
    /// per-request cancellation token and UI-safe progress event callback.
    pub fn import_encoded(
        &mut self,
        playlist: &str,
        title: &str,
        artist: &str,
        input: &[u8],
        nonce: &str,
        executable: &Path,
        cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
        progress: impl FnMut(u32) -> Result<()>,
        guard: impl FnMut() -> Result<()>,
    ) -> Result<String> {
        let native = match jukebox_audio::wrap(input) {
            Ok(native) => native,
            Err(_) => {
                let pcm = crate::jukebox_decode_worker::decode_guarded(
                    executable, input, nonce, cancel, progress, guard,
                )?;
                jukebox_audio::native_pcm(&pcm)?
            }
        };
        self.import_audio(playlist, title, artist, &native, nonce)
    }
    pub fn import_package_native(
        &mut self,
        bytes: &[u8],
        nonce: &str,
        executable: &Path,
        cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
        mut progress: impl FnMut(u32) -> Result<()>,
        mut guard: impl FnMut() -> Result<()>,
    ) -> Result<String> {
        let imported = package::import_segment(bytes, &self.stock, |encoded, _| {
            crate::jukebox_decode_worker::decode_guarded(
                executable,
                encoded,
                nonce,
                cancel.clone(),
                &mut progress,
                &mut guard,
            )
        })?;
        self.accept_import(imported, nonce)
    }
    /// Called after every segment of a large container has been validated. Root
    /// may stage private blobs per segment; no catalog commit before completion.
    fn stage_song(
        &self,
        next: &mut State,
        song: package::ImportedSong,
        nonce: &str,
    ) -> Result<String> {
        if let Some(native) = song.native_audio {
            let source = local::cache_native(&self.audio, &native, nonce)?;
            let mut t = next.music.add_custom(
                &song.metadata.name,
                &song.metadata.artist,
                &song.metadata.cover,
            )?;
            t.extra = song.metadata.extra;
            *next.music.tracks.last_mut().unwrap() = t.clone();
            let key = t.key();
            let artwork = song
                .artwork
                .as_ref()
                .map(|b| {
                    local::cache_art(&self.art, b, false, nonce)
                        .map(|(sha256, size)| Blob { sha256, size })
                })
                .transpose()?;
            let preview = song
                .artwork_preview
                .as_ref()
                .map(|b| {
                    local::cache_art(&self.art, b, true, nonce)
                        .map(|(sha256, size)| Blob { sha256, size })
                })
                .transpose()?;
            next.audio.insert(
                key.clone(),
                Audio {
                    native: Blob {
                        sha256: source.sha256,
                        size: source.size,
                    },
                    artwork,
                    preview,
                },
            );
            Ok(key)
        } else {
            let key = song.metadata.key();
            ensure!(
                self.stock.tracks.iter().any(|t| t.key() == key),
                "Stock reference cannot be relinked"
            );
            if song.metadata_override {
                let track = next
                    .music
                    .tracks
                    .iter_mut()
                    .find(|t| t.key() == key)
                    .context("Stock song missing")?;
                track.name = song.metadata.name;
                track.artist = song.metadata.artist;
                if let (Some(native), Some(preview)) = (song.artwork, song.artwork_preview) {
                    let (sha256, size) = local::cache_art(&self.art, &native, false, nonce)?;
                    let native = Blob { sha256, size };
                    let (sha256, size) = local::cache_art(&self.art, &preview, true, nonce)?;
                    next.artwork_overrides.insert(
                        key.clone(),
                        Artwork {
                            native,
                            preview: Blob { sha256, size },
                        },
                    );
                }
            }
            Ok(key)
        }
    }
    fn finish_import(
        &mut self,
        mut next: State,
        keys: Vec<String>,
        base: &str,
        nonce: &str,
    ) -> Result<String> {
        let base = base.trim();
        name(base)?;
        let mut title = base.to_string();
        for i in 2..=102 {
            if !next
                .playlists
                .iter()
                .any(|p| p.name.eq_ignore_ascii_case(&title))
            {
                break;
            }
            let suffix = format!(" ({i})");
            title = base
                .chars()
                .take(80 - suffix.chars().count())
                .collect::<String>()
                + &suffix;
        }
        next.playlists.push(Playlist {
            id: nonce.into(),
            name: title,
            track_keys: keys,
        });
        self.commit(next, nonce)?;
        Ok(nonce.into())
    }
    pub fn accept_import(&mut self, imported: Import, nonce: &str) -> Result<String> {
        ensure!(catalog::hex(nonce, 48), "Invalid playlist import identity");
        let (mut next, mut keys) = (self.state.clone(), Vec::new());
        for song in imported.songs {
            keys.push(self.stage_song(&mut next, song, nonce)?);
        }
        self.finish_import(next, keys, &imported.name, nonce)
    }
    /// Large WPF combined containers are streamed one bounded segment at a
    /// time. A bad later segment never commits a partial playlist/catalog.
    pub fn import_container<R: std::io::Read + std::io::Seek>(
        &mut self,
        source: R,
        nonce: &str,
        decode: impl FnMut(&[u8], usize) -> Result<Vec<u8>>,
    ) -> Result<String> {
        ensure!(catalog::hex(nonce, 48), "Invalid playlist import identity");
        let mut container = package::Container::open(source)?;
        let title = container.manifest().name.clone();
        let (mut next, mut keys) = (self.state.clone(), Vec::new());
        container.visit(&self.stock, decode, |song| {
            keys.push(self.stage_song(&mut next, song, nonce)?);
            Ok(())
        })?;
        self.finish_import(next, keys, &title, nonce)
    }
    pub fn export_package(&self, id: &str) -> Result<Vec<u8>> {
        let p = self
            .state
            .playlists
            .iter()
            .find(|p| p.id == id)
            .context("Playlist is missing")?;
        let music = self.state.music.selected(&p.track_keys)?;
        let mut expanded = 64 * 1024usize;
        for key in &p.track_keys {
            if let Some(a) = self.state.audio.get(key) {
                for size in [
                    a.native.size,
                    a.artwork.as_ref().map_or(0, |b| b.size),
                    a.preview.as_ref().map_or(0, |b| b.size),
                ] {
                    expanded = expanded
                        .checked_add(size)
                        .context("Playlist export size overflow")?;
                    ensure!(
                        expanded <= 320 * 1024 * 1024,
                        "Playlist needs streaming multipart export; this bounded ordinary export left the library untouched"
                    );
                }
            }
            if let Some(a) = self.state.artwork_overrides.get(key) {
                expanded = expanded
                    .checked_add(a.native.size + a.preview.size)
                    .context("Artwork export size overflow")?;
                ensure!(
                    expanded <= 320 * 1024 * 1024,
                    "Playlist needs streaming multipart export; artwork retained"
                );
            }
        }
        let mut audio = BTreeMap::new();
        for key in &p.track_keys {
            if let Some(a) = self.state.audio.get(key) {
                audio.insert(
                    key.clone(),
                    local::read_cached_bytes(&self.audio, &a.native.sha256, a.native.size)?,
                );
            }
        }
        let (mut manifest, bytes) =
            package::export_segment(&p.name, &music, &self.stock, &audio, None)?;
        let mut entries = package::unpack(&bytes, package::MAX_SEGMENT, 512)?;
        for (i, song) in manifest.tracks.iter_mut().enumerate() {
            if let Some(a) = self.state.artwork_overrides.get(&song.source_key) {
                if song.kind == "stock-reference" {
                    song.kind = "stock-override".into();
                    manifest.schema = "nba2k27-2kplaylist-v4".into();
                }
                let member = format!("artwork/{:04}.tld", i + 1);
                entries.insert(
                    member.clone(),
                    local::read_art(&self.art, &a.native.sha256, a.native.size, false)?,
                );
                song.artwork_file = Some(member);
                let member = format!("artwork/{:04}.png", i + 1);
                entries.insert(
                    member.clone(),
                    local::read_art(&self.art, &a.preview.sha256, a.preview.size, true)?,
                );
                song.artwork_preview_file = Some(member);
                continue;
            }
            if let Some(a) = self.state.audio.get(&song.source_key) {
                if let Some(b) = &a.artwork {
                    let member = format!("artwork/{:04}.tld", i + 1);
                    entries.insert(
                        member.clone(),
                        local::read_art(&self.art, &b.sha256, b.size, false)?,
                    );
                    song.artwork_file = Some(member);
                }
                if let Some(b) = &a.preview {
                    let member = format!("artwork/{:04}.png", i + 1);
                    entries.insert(
                        member.clone(),
                        local::read_art(&self.art, &b.sha256, b.size, true)?,
                    );
                    song.artwork_preview_file = Some(member);
                }
            }
        }
        entries.insert(
            "playlist.json".into(),
            serde_json::to_vec_pretty(&manifest)?,
        );
        package::pack(&entries, package::MAX_SEGMENT, 512)
    }
    pub fn source(&self, key: &str) -> Result<NativeSource> {
        self.source_with(key, &Default::default())
    }
    fn source_with(
        &self,
        key: &str,
        known: &crate::jukebox_prepared::Verified,
    ) -> Result<NativeSource> {
        let a = self
            .state
            .audio
            .get(key)
            .context("Song has no private audio")?;
        local::read_cached_with(&self.audio, &a.native.sha256, a.native.size, known)
    }
    /// A root media endpoint can cache/serve these bytes under an opaque token;
    /// never expose arbitrary filesystem reads to the renderer for previews.
    pub fn preview_audio(&self, key: &str) -> Result<(&'static str, Vec<u8>)> {
        let a = self
            .state
            .audio
            .get(key)
            .context("Custom song audio is missing")?;
        let native = local::read_cached_bytes(&self.audio, &a.native.sha256, a.native.size)?;
        let info = jukebox_audio::inspect(&native)?;
        match info.codec {
            jukebox_audio::Codec::Pcm => Ok(("audio/wav", jukebox_audio::pcm_wave(&native[54..])?)),
            jukebox_audio::Codec::Opus => {
                Ok(("audio/ogg", jukebox_audio::preview_ogg(&native, 0)?))
            }
        }
    }
    /// Fully prepares owned files but DOES NOT launch Steam. Root records the
    /// Jukebox marker in its durable session before activation, supplies fresh
    /// closed guards, merges policy.loader_config into typed loader config,
    /// activates once, then verifies immediately before its normal launch.
    pub fn prepare_session(
        &self,
        game_root: &Path,
        mods: &Path,
        session: &str,
        closed: impl Fn() -> Result<()>,
    ) -> Result<Option<Prepared>> {
        self.prepare_session_with(game_root, mods, session, closed, &Default::default())
    }
    /// `known` is the launch's validated prepared-playlist cache view.
    pub fn prepare_session_with(
        &self,
        game_root: &Path,
        mods: &Path,
        session: &str,
        closed: impl Fn() -> Result<()>,
        known: &crate::jukebox_prepared::Verified,
    ) -> Result<Option<Prepared>> {
        self.state.validate(&self.stock)?;
        let Some(id) = &self.state.active_playlist_id else {
            return Ok(None);
        };
        let p = self
            .state
            .playlists
            .iter()
            .find(|p| &p.id == id)
            .context("Active playlist is missing")?;
        let selected = self.state.music.selected(&p.track_keys)?;
        let mut plan = catalog::plan_with_stock(&selected, &self.stock)?;
        let (mut sources, mut warnings) = (BTreeMap::new(), Vec::new());
        for m in &plan.mappings {
            closed()?;
            let source = if m.source_is_stock {
                let t = selected
                    .tracks
                    .iter()
                    .find(|t| catalog::audio_path(t.bank) == m.source)
                    .context("Remapped stock identity is missing")?;
                let (_, bytes) =
                    jukebox_v2_resources::read_stock_native(game_root, &self.stock, &t.key())?;
                local::cache_native(&self.audio, &bytes, session)?
            } else {
                let t = selected
                    .tracks
                    .iter()
                    .find(|t| catalog::audio_path(t.bank) == m.source)
                    .context("Custom source identity is missing")?;
                self.source_with(&t.key(), known)?
            };
            sources.insert(m.source.clone(), source);
        }
        let mut covers = Vec::new();
        if selected.tracks.iter().any(|t| {
            self.state
                .audio
                .get(&t.key())
                .is_some_and(|a| a.artwork.is_some())
                || self.state.artwork_overrides.contains_key(&t.key())
        }) {
            // WPF jukebox-artwork-session: each song with new art borrows a
            // stock cover slot. V2 delivers it through the cover route
            // (retargeted container + stock-form textures at new paths).
            let route = (|| -> Result<Option<crate::jukebox_cover_route::Route>> {
                ensure!(
                    plan.music.tracks.len() == selected.tracks.len(),
                    "Published music differs from the playlist"
                );
                let art = selected
                    .tracks
                    .iter()
                    .map(|t| self.artwork_native(&t.key()))
                    .collect::<Result<Vec<_>>>()?;
                let mut music = plan.music.clone();
                let Some(route) = crate::jukebox_cover_route::plan(
                    game_root,
                    &self.stock,
                    &mut music,
                    &art,
                    &|| closed(),
                )?
                else {
                    return Ok(None);
                };
                // Never adopt or overwrite a file already in Mods.
                for (logical, _) in route.files() {
                    ensure!(
                        !mods.join(&logical).try_exists()?,
                        "{logical} already exists in Mods"
                    );
                }
                plan.music = music;
                Ok(Some(route))
            })();
            match route {
                Ok(Some(route)) => {
                    if route.skipped > 0 {
                        warnings.push(format!(
                            "{} song(s) with custom artwork found no free cover slot and keep their current cover.",
                            route.skipped
                        ));
                    }
                    covers = route.files();
                }
                Ok(None) => {}
                Err(error) => {
                    warnings.push(format!(
                        "Custom cover artwork could not be prepared for the game ({error:#}); imported songs use a stock cover this session."
                    ));
                    let fallback = self
                        .stock
                        .tracks
                        .iter()
                        .find(|t| t.collection == 1 && !t.cover.is_empty())
                        .context("No stock cover fallback")?
                        .cover
                        .clone();
                    for t in &mut plan.music.tracks {
                        if plan.policy.custom.as_bytes()[t.bank as usize - 1] == b'1' {
                            t.cover = fallback.clone();
                        }
                    }
                }
            }
        }
        let store = jukebox_files::Store::create_with_covers(
            &self.portable,
            mods,
            session,
            &plan,
            &sources,
            &covers,
            closed,
            known,
        )?;
        Ok(Some(Prepared {
            store,
            policy: plan.policy,
            warnings,
        }))
    }
}
#[cfg(test)]
mod tests {
    include!("jukebox_test_paths.rs");
    use super::*;
    use serde_json::Map;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let p = fixture_artifacts().join(format!(
                "2k-jukebox-service-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::SeqCst)
            ));
            std::fs::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            assert!(
                self.0
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("2k-jukebox-service-")
            );
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn stock() -> Music {
        Music {
            tracks: vec![catalog::Track {
                name: "Stock".into(),
                artist: "Artist".into(),
                cover: "001_helicopter".into(),
                collection: 1,
                bank: 1,
                unique: 0,
                extra: Map::new(),
            }],
            extra: Map::new(),
        }
    }
    #[test]
    fn actual_service_import_order_persistence_export_and_owned_session() {
        let f = Fixture::new();
        let mut s = Service::open(&f.0, stock(), &"a".repeat(48)).unwrap();
        let id = "b".repeat(48);
        s.create_playlist("Playlist", &id).unwrap();
        let wav = jukebox_audio::pcm_wave(&[0; 8]).unwrap();
        let one = s
            .import_audio(&id, "One", "Artist", &wav, &"c".repeat(48))
            .unwrap();
        let two = s
            .import_audio(&id, "Two", "Artist", &wav, &"d".repeat(48))
            .unwrap();
        s.set_order(&id, vec![two.clone(), one.clone()], &"e".repeat(48))
            .unwrap();
        s.set_active(Some(&id), &"f".repeat(48)).unwrap();
        let package = s.export_package(&id).unwrap();
        drop(s);
        let s = Service::open(&f.0, stock(), &"0".repeat(48)).unwrap();
        assert_eq!(s.view().playlists[0].track_keys, vec![two, one]);
        let imported = package::import_segment(&package, &stock(), |_, _| Ok(vec![])).unwrap();
        assert_eq!(imported.songs[0].metadata.name, "Two");
        let mods = f.0.join("SyntheticMods");
        std::fs::create_dir(&mods).unwrap();
        let prepared = s
            .prepare_session(&f.0, &mods, &"1".repeat(48), || Ok(()))
            .unwrap()
            .unwrap();
        prepared.store.activate(|| Ok(())).unwrap();
        prepared.store.verify_current().unwrap();
        assert!(mods.join("music.iff").exists());
        prepared.store.restore(|| Ok(())).unwrap();
        assert!(!mods.join("music.iff").exists());
    }
    #[test]
    fn selecting_default_and_deleting_playlist_never_delete_saved_audio() {
        let f = Fixture::new();
        let mut s = Service::open(&f.0, stock(), &"a".repeat(48)).unwrap();
        let id = "b".repeat(48);
        s.create_playlist("Playlist", &id).unwrap();
        let key = s
            .import_audio(
                &id,
                "One",
                "Artist",
                &jukebox_audio::pcm_wave(&[0; 8]).unwrap(),
                &"c".repeat(48),
            )
            .unwrap();
        let source = s.source(&key).unwrap();
        s.set_active(None, &"d".repeat(48)).unwrap();
        assert!(
            s.prepare_session(&f.0, &f.0, &"e".repeat(48), || Ok(()))
                .unwrap()
                .is_none()
        );
        s.delete_playlist(&id, &"f".repeat(48)).unwrap();
        assert!(source.path.exists());
        assert!(s.view().music.tracks.iter().any(|t| t.key() == key));
    }
    #[test]
    fn streamed_container_commits_only_after_every_segment_passes() {
        let f = Fixture::new();
        let mut service = Service::open(&f.0, stock(), &"a".repeat(48)).unwrap();
        let wav = jukebox_audio::pcm_wave(&[0; 8]).unwrap();
        let mut all = Music {
            tracks: vec![],
            extra: Map::new(),
        };
        let mut segments = Vec::new();
        let mut combined = None;
        let mut lengths = Vec::new();
        for (i, title) in ["First", "Second"].into_iter().enumerate() {
            let mut t = stock().tracks[0].clone();
            t.bank = 72 + i as u16;
            t.unique = 130 + i as u16;
            t.name = title.into();
            all.tracks.push(t.clone());
            let (m, bytes) = package::export_segment(
                "Streamed",
                &Music {
                    tracks: vec![t.clone()],
                    extra: Map::new(),
                },
                &stock(),
                &BTreeMap::from([(t.key(), wav.clone())]),
                Some(package::Multipart {
                    id: "fixture".into(),
                    part: i + 1,
                    count: 2,
                }),
            )
            .unwrap();
            lengths.push(bytes.len());
            if let Some(manifest) = &mut combined {
                let manifest: &mut package::Manifest = manifest;
                manifest.tracks.extend(m.tracks);
            } else {
                combined = Some(m);
            }
            segments.push(Ok(bytes));
        }
        let mut manifest = combined.unwrap();
        manifest.multipart = None;
        let mut bytes = Vec::new();
        package::write_container(&mut bytes, manifest, segments).unwrap();
        let before = service.fingerprint().unwrap();
        let mut bad = bytes.clone();
        bad[12 + lengths[0] + 20] ^= 1;
        assert!(
            service
                .import_container(std::io::Cursor::new(bad), &"b".repeat(48), |_, _| Ok(
                    vec![]
                ))
                .is_err()
        );
        assert_eq!(service.fingerprint().unwrap(), before);
        assert!(service.view().playlists.is_empty());
        service
            .import_container(std::io::Cursor::new(bytes), &"c".repeat(48), |_, _| {
                Ok(vec![])
            })
            .unwrap();
        assert_eq!(service.view().playlists.len(), 1);
        let keys = &service.view().playlists[0].track_keys;
        assert_eq!(
            service
                .view()
                .music
                .selected(keys)
                .unwrap()
                .tracks
                .iter()
                .map(|t| t.name.as_str())
                .collect::<Vec<_>>(),
            vec!["First", "Second"]
        );
    }
}
