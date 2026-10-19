//! Non-destructive WPF media bridge. Main is read-only; publication is allowed
//! only into a brand-new, disjoint staging root. No settings, game/session
//! files, decoder processes or launch preferences are changed here.
#![allow(dead_code)]
use crate::{
    jukebox_audio as audio, jukebox_catalog as catalog, jukebox_package as package,
    jukebox_service::Service, startup_mp4,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    mem::size_of,
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Component, Path, PathBuf, Prefix},
    sync::atomic::{AtomicBool, Ordering},
};
use windows::Win32::{Foundation::HANDLE, Storage::FileSystem::*};

const VIDEO_SCHEMA: &str = "nba2k27-rust-startup-video-library-v1";
fn check(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Acquire),
        "Saved-media import cancelled; original library untouched"
    );
    Ok(())
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn same_path(a: &Path, b: &Path) -> bool {
    let normalized = |p: &Path| {
        let text = p.to_string_lossy();
        text.strip_prefix(r"\\?\")
            .unwrap_or(&text)
            .replace('/', "\\")
            .to_ascii_lowercase()
    };
    normalized(a) == normalized(b)
}
fn ordinary(file: &File, directory: bool) -> Result<()> {
    let m = file.metadata()?;
    let mut i = BY_HANDLE_FILE_INFORMATION::default();
    unsafe {
        GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut i)?;
    }
    ensure!(
        m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 == 0
            && if directory {
                m.is_dir()
            } else {
                m.is_file() && i.nNumberOfLinks == 1
            },
        "Saved-media source is linked/nonordinary; left untouched"
    );
    Ok(())
}
struct Parents(Vec<File>);
impl Parents {
    fn pin(path: &Path) -> Result<Self> {
        ensure!(path.is_absolute(), "Saved-media root must be absolute");
        let mut current = PathBuf::new();
        let mut pins = Vec::new();
        for c in path.components() {
            ensure!(
                !matches!(c, Component::ParentDir | Component::CurDir),
                "Unnormalized saved-media path"
            );
            if let Component::Prefix(p) = c {
                ensure!(
                    matches!(p.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)),
                    "Network/device media paths are unsupported"
                );
            }
            current.push(c.as_os_str());
            if matches!(c, Component::Prefix(_)) {
                continue;
            }
            let file = fs::OpenOptions::new()
                .read(true)
                .share_mode(3)
                .custom_flags(0x02200000)
                .open(&current)?;
            ordinary(&file, true)?;
            pins.push(file);
        }
        Ok(Self(pins))
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Stamp {
    volume: u64,
    id: [u8; 16],
    size: u64,
    modified: u64,
}
fn stamp(file: &File) -> Result<Stamp> {
    ordinary(file, false)?;
    let m = file.metadata()?;
    let mut id = FILE_ID_INFO::default();
    unsafe {
        GetFileInformationByHandleEx(
            HANDLE(file.as_raw_handle()),
            FileIdInfo,
            &mut id as *mut _ as _,
            size_of::<FILE_ID_INFO>() as u32,
        )?;
    }
    Ok(Stamp {
        volume: id.VolumeSerialNumber,
        id: id.FileId.Identifier,
        size: m.len(),
        modified: m.last_write_time(),
    })
}
struct Source {
    file: File,
    path: PathBuf,
    stamp: Stamp,
    _parents: Parents,
}
impl Source {
    fn open(path: &Path, max: u64) -> Result<Self> {
        let parents = Parents::pin(path.parent().context("Missing source parent")?)?;
        let file = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .custom_flags(0x00200000)
            .open(path)?;
        let value = stamp(&file)?;
        ensure!(
            (1..=max).contains(&value.size),
            "Saved-media source exceeds its bound"
        );
        Ok(Self {
            file,
            path: path.to_path_buf(),
            stamp: value,
            _parents: parents,
        })
    }
    fn verify(&self) -> Result<()> {
        ensure!(
            stamp(&self.file)? == self.stamp,
            "Saved-media retained source changed"
        );
        Ok(())
    }
    fn bytes(&mut self, max: usize, cancel: &AtomicBool) -> Result<Vec<u8>> {
        check(cancel)?;
        self.verify()?;
        ensure!(
            self.stamp.size <= max as u64,
            "Saved-media source exceeds bounded parser"
        );
        self.file.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::with_capacity(self.stamp.size as usize);
        let mut block = vec![0; 1024 * 1024];
        loop {
            check(cancel)?;
            let n = self.file.read(&mut block)?;
            if n == 0 {
                break;
            }
            ensure!(bytes.len() + n <= max, "Source grew past bounded parser");
            bytes.extend_from_slice(&block[..n]);
        }
        self.verify()?;
        ensure!(
            bytes.len() as u64 == self.stamp.size,
            "Partial saved-media source"
        );
        Ok(bytes)
    }
    fn sha(&mut self, cancel: &AtomicBool) -> Result<String> {
        check(cancel)?;
        self.verify()?;
        self.file.seek(SeekFrom::Start(0))?;
        let mut h = Sha256::new();
        let mut block = vec![0; 1024 * 1024];
        let mut size = 0u64;
        loop {
            check(cancel)?;
            let n = self.file.read(&mut block)?;
            if n == 0 {
                break;
            }
            h.update(&block[..n]);
            size += n as u64;
        }
        ensure!(size == self.stamp.size, "Partial saved-media source hash");
        self.verify()?;
        Ok(format!("{:x}", h.finalize()))
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OldLibrary {
    schema: String,
    archive_storage_version: u32,
    music: catalog::Music,
    playlists: Vec<OldPlaylist>,
    #[serde(default)]
    active_playlist_id: Option<String>,
    #[serde(default)]
    last_playlist_id: Option<String>,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OldPlaylist {
    id: String,
    name: String,
    track_keys: Vec<String>,
    #[serde(default)]
    recovery_playlist: bool,
    #[serde(default)]
    bundle_path: Option<String>,
    #[serde(default)]
    bundle_paths: Vec<String>,
}
#[derive(Deserialize)]
struct OldVideos {
    schema: u32,
    selected: Option<String>,
    items: Vec<OldVideo>,
}
#[derive(Deserialize)]
struct OldVideo {
    id: String,
    name: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct VideoItem {
    pub id: String,
    pub name: String,
    pub relative_path: String,
    pub sha256: String,
    pub bytes: u64,
    pub duration_ms: u64,
    pub codecs: Vec<String>,
    pub width: u16,
    pub height: u16,
    pub playback_probe_required: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct VideoCatalog {
    pub schema: String,
    pub selected: Option<String>,
    pub items: Vec<VideoItem>,
}
fn video_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}
fn codecs(m: &startup_mp4::Metadata) -> Vec<String> {
    m.video_codecs
        .iter()
        .map(|c| {
            match c {
                startup_mp4::Codec::H264 => "H.264",
                startup_mp4::Codec::Av1 => "AV1",
                startup_mp4::Codec::Vp9 => "VP9",
            }
            .into()
        })
        .collect()
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Report {
    pub schema: String,
    pub visible_playlists: usize,
    pub hidden_recovery_playlists: usize,
    pub ordered_tracks: usize,
    pub unique_source_tracks: usize,
    pub unique_audio_blobs: usize,
    pub native_audio_bytes: u64,
    pub duplicate_audio_bytes_avoided: u64,
    pub unique_artwork_blobs: usize,
    pub artwork_bytes: u64,
    pub duplicate_artwork_bytes_avoided: u64,
    pub reserved_stock_main_banks: usize,
    pub legacy_custom_bank_collisions: usize,
    pub opus_tracks: usize,
    pub pcm_tracks: usize,
    pub stock_references: usize,
    pub bundle_bytes: u64,
    pub videos: usize,
    pub video_bytes: u64,
    pub video_codecs: BTreeMap<String, usize>,
    pub warnings: Vec<String>,
}
struct Bundle {
    source: Source,
    playlist: OldPlaylist,
    sha256: String,
    signatures: Vec<String>,
}
struct Video {
    source: Source,
    item: VideoItem,
}
pub(crate) struct Plan {
    root: PathBuf,
    _root_pins: Parents,
    legacy: Source,
    legacy_bytes: Vec<u8>,
    legacy_sha: String,
    music: BTreeMap<String, catalog::Track>,
    stock: catalog::Music,
    bundles: Vec<Bundle>,
    videos: Vec<Video>,
    video_catalog: VideoCatalog,
    video_metadata: Option<Source>,
    report: Report,
}
fn signature(song: &package::ImportedSong) -> Result<String> {
    Ok(digest(&serde_json::to_vec(&(
        &song.metadata,
        song.native_audio.as_ref().map(|b| digest(b)),
        song.artwork.as_ref().map(|b| digest(b)),
        song.artwork_preview.as_ref().map(|b| digest(b)),
    ))?))
}
fn imported(
    bundle: &mut Bundle,
    music: &BTreeMap<String, catalog::Track>,
    stock: &catalog::Music,
    cancel: &AtomicBool,
) -> Result<package::Import> {
    let bytes = bundle.source.bytes(package::MAX_SEGMENT, cancel)?;
    if !bundle.sha256.is_empty() {
        ensure!(
            digest(&bytes) == bundle.sha256,
            "Legacy bundle changed after dry-run"
        );
    }
    let mut import = package::import_segment(&bytes, stock, |_, _| {
        anyhow::bail!(
            "Legacy FLAC needs an explicitly owned decoder; no conversion or source change attempted"
        )
    })?;
    let mut songs: BTreeMap<_, _> = import
        .songs
        .drain(..)
        .map(|s| (s.metadata.key(), s))
        .collect();
    ensure!(
        songs.len() == bundle.playlist.track_keys.len(),
        "Legacy playlist/bundle song set differs"
    );
    for key in &bundle.playlist.track_keys {
        check(cancel)?;
        let mut song = songs
            .remove(key)
            .context("Legacy bundle is missing a saved playlist song")?;
        if song.native_audio.is_some() {
            song.metadata = music
                .get(key)
                .context("Saved custom song metadata is absent")?
                .clone();
        }
        import.songs.push(song);
    }
    ensure!(songs.is_empty(), "Legacy bundle contains unselected songs");
    import.name = bundle.playlist.name.clone();
    Ok(import)
}
/// Fully validates every selected package/resource and MP4 while holding only
/// read-only ordinary source handles. The profile is an exact 16-hex leaf,
/// never an archiveDirectory/bundlePath supplied by an untrusted manifest.
pub(crate) fn inspect(
    source: &Path,
    profile: &str,
    stock: catalog::Music,
    cancel: &AtomicBool,
) -> Result<Plan> {
    check(cancel)?;
    stock.validate()?;
    ensure!(catalog::hex(profile, 16), "Invalid legacy profile key");
    let pins = Parents::pin(source)?;
    let root = source.canonicalize()?;
    let directory = root.join("Library/Music/archive-library").join(profile);
    let mut legacy = Source::open(&directory.join("library.json"), 4 * 1024 * 1024)?;
    let legacy_bytes = legacy.bytes(4 * 1024 * 1024, cancel)?;
    let legacy_sha = digest(&legacy_bytes);
    let old: OldLibrary = serde_json::from_slice(&legacy_bytes)?;
    ensure!(
        old.schema == "nba2k27-jukebox-library-v1"
            && old.archive_storage_version == 1
            && old.playlists.len() <= 100,
        "Unsupported legacy Jukebox schema"
    );
    old.music.validate()?;
    let music: BTreeMap<_, _> = old.music.tracks.into_iter().map(|t| (t.key(), t)).collect();
    let mut report = Report {
        schema: "nba2k27-legacy-media-dry-run-v1".into(),
        visible_playlists: 0,
        hidden_recovery_playlists: 0,
        ordered_tracks: 0,
        unique_source_tracks: 0,
        unique_audio_blobs: 0,
        native_audio_bytes: 0,
        duplicate_audio_bytes_avoided: 0,
        unique_artwork_blobs: 0,
        artwork_bytes: 0,
        duplicate_artwork_bytes_avoided: 0,
        reserved_stock_main_banks: stock.tracks.iter().filter(|t| t.collection == 1).count(),
        legacy_custom_bank_collisions: 0,
        opus_tracks: 0,
        pcm_tracks: 0,
        stock_references: 0,
        bundle_bytes: 0,
        videos: 0,
        video_bytes: 0,
        video_codecs: BTreeMap::new(),
        warnings: vec![],
    };
    let mut bundles = vec![];
    let mut seen_ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut source_songs = BTreeMap::new();
    let mut audio_hashes = BTreeSet::new();
    let mut art_hashes = BTreeSet::new();
    let mut colliding_keys = BTreeSet::new();
    for playlist in old.playlists {
        if playlist.recovery_playlist {
            report.hidden_recovery_playlists += 1;
            continue;
        }
        ensure!(
            !playlist.id.is_empty()
                && playlist.id.len() <= 128
                && seen_ids.insert(playlist.id.clone())
                && names.insert(playlist.name.to_lowercase())
                && !playlist.name.trim().is_empty()
                && playlist.name == playlist.name.trim()
                && playlist.name.chars().count() <= 80
                && !playlist.name.chars().any(char::is_control),
            "Invalid visible legacy playlist identity/name"
        );
        ensure!(
            !playlist.track_keys.is_empty()
                && playlist.track_keys.len() <= catalog::MAX_TRACKS
                && playlist.track_keys.iter().collect::<BTreeSet<_>>().len()
                    == playlist.track_keys.len(),
            "Invalid visible legacy song order"
        );
        let declared = PathBuf::from(
            playlist
                .bundle_path
                .as_ref()
                .context("Visible playlist has no bundle")?,
        );
        ensure!(
            declared
                .parent()
                .is_some_and(|p| same_path(p, &directory.join("playlists")))
                && declared.extension().is_some_and(|s| s == "2kplaylist")
                && playlist
                    .bundle_paths
                    .iter()
                    .all(|p| same_path(Path::new(p), &declared)),
            "Legacy bundle is outside its exact managed profile or multipart import is required"
        );
        let fixed = directory
            .join("playlists")
            .join(declared.file_name().context("Legacy bundle leaf missing")?);
        let file = Source::open(&fixed, package::MAX_SEGMENT as u64)?;
        let mut bundle = Bundle {
            source: file,
            playlist,
            sha256: String::new(),
            signatures: vec![],
        };
        let import = imported(&mut bundle, &music, &stock, cancel)?;
        bundle.sha256 = bundle.source.sha(cancel)?;
        for song in &import.songs {
            let sig = signature(song)?;
            let key = song.metadata.key();
            if let Some(previous) = source_songs.insert(key, sig.clone()) {
                ensure!(
                    previous == sig,
                    "A legacy song has conflicting metadata/audio between playlists"
                );
            }
            bundle.signatures.push(sig);
            if let Some(bytes) = &song.native_audio {
                if stock.tracks.iter().any(|t| {
                    t.collection == song.metadata.collection && t.bank == song.metadata.bank
                }) {
                    colliding_keys.insert(song.metadata.key());
                }
                let sha = digest(bytes);
                if audio_hashes.insert(sha) {
                    report.native_audio_bytes += bytes.len() as u64;
                } else {
                    report.duplicate_audio_bytes_avoided += bytes.len() as u64;
                }
                if audio::parse_opus(bytes).is_ok() {
                    report.opus_tracks += 1;
                } else {
                    report.pcm_tracks += 1;
                }
            } else {
                report.stock_references += 1;
            }
            for (kind, bytes) in [("tld", &song.artwork), ("png", &song.artwork_preview)] {
                if let Some(bytes) = bytes {
                    if art_hashes.insert((kind, digest(bytes))) {
                        report.artwork_bytes += bytes.len() as u64;
                    } else {
                        report.duplicate_artwork_bytes_avoided += bytes.len() as u64;
                    }
                }
            }
        }
        report.warnings.extend(import.warnings);
        report.visible_playlists += 1;
        report.ordered_tracks += import.songs.len();
        report.bundle_bytes += bundle.source.stamp.size;
        bundles.push(bundle);
    }
    report.unique_source_tracks = source_songs.len();
    report.unique_audio_blobs = audio_hashes.len();
    report.unique_artwork_blobs = art_hashes.len();
    report.legacy_custom_bank_collisions = colliding_keys.len();
    if report.hidden_recovery_playlists > 0 {
        report.warnings.push(format!(
            "{} hidden recovery playlist(s) were preserved at source and not imported.",
            report.hidden_recovery_playlists
        ));
    }
    if old.active_playlist_id.is_some() {
        report.warnings.push("An old active playlist was recorded, but import leaves Default selected until explicit review.".into());
    }
    if old.last_playlist_id.is_some() {
        report.warnings.push(
            "Remembered playlist UI selection was not treated as active game playback.".into(),
        );
    }
    let mut videos = vec![];
    let mut video_catalog = VideoCatalog {
        schema: VIDEO_SCHEMA.into(),
        selected: None,
        items: vec![],
    };
    let mut video_metadata = None;
    let video_path = root.join("Library/Videos/mp4-library.json");
    if video_path.try_exists()? {
        let mut metadata = Source::open(&video_path, 1024 * 1024)?;
        let old: OldVideos = serde_json::from_slice(&metadata.bytes(1024 * 1024, cancel)?)?;
        ensure!(
            old.schema == 1 && old.items.len() <= 1000,
            "Unsupported saved MP4 catalog"
        );
        let mut ids = BTreeSet::new();
        for entry in old.items {
            check(cancel)?;
            ensure!(
                video_id(&entry.id)
                    && ids.insert(entry.id.clone())
                    && !entry.name.trim().is_empty()
                    && entry.name.chars().count() <= 120
                    && !entry.name.chars().any(char::is_control),
                "Invalid saved MP4 identity/name"
            );
            let mut file = Source::open(
                &root
                    .join("Library/Videos/mp4")
                    .join(format!("{}.mp4", entry.id)),
                startup_mp4::MAX_BYTES,
            )?;
            let info = startup_mp4::inspect(&mut file.file, "mp4")?;
            file.verify()?;
            let item = VideoItem {
                id: entry.id.clone(),
                name: entry.name,
                relative_path: format!("Library/Videos/mp4/{}.mp4", entry.id),
                sha256: file.sha(cancel)?,
                bytes: info.bytes,
                duration_ms: info.duration_ms,
                codecs: codecs(&info),
                width: info.max_width,
                height: info.max_height,
                playback_probe_required: true,
            };
            for codec in &item.codecs {
                *report.video_codecs.entry(codec.clone()).or_insert(0) += 1;
            }
            report.video_bytes += item.bytes;
            video_catalog.items.push(item.clone());
            videos.push(Video { source: file, item });
        }
        ensure!(
            old.selected.as_ref().is_none_or(|id| ids.contains(id)),
            "Saved selected MP4 ID is missing"
        );
        video_catalog.selected = old.selected;
        video_metadata = Some(metadata);
    }
    report.videos = videos.len();
    Ok(Plan {
        root,
        _root_pins: pins,
        legacy,
        legacy_bytes,
        legacy_sha,
        music,
        stock,
        bundles,
        videos,
        video_catalog,
        video_metadata,
        report,
    })
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Staged {
    pub schema: String,
    pub source_catalog_sha256: String,
    pub report: Report,
    pub playlist_ids: BTreeMap<String, String>,
    pub track_keys: BTreeMap<String, String>,
    pub videos: VideoCatalog,
    pub active_playlist_id: Option<String>,
    pub startup_video_enabled: bool,
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
impl Plan {
    pub fn report(&self) -> &Report {
        &self.report
    }
    pub fn videos(&self) -> &VideoCatalog {
        &self.video_catalog
    }
    /// A failed stage is intentionally retained, never promoted or reused as
    /// complete. The destination must not exist and must be disjoint from Main.
    pub fn stage_new(
        &mut self,
        destination: &Path,
        cancel: &AtomicBool,
        mut progress: impl FnMut(usize, usize) -> Result<()>,
    ) -> Result<Staged> {
        check(cancel)?;
        ensure!(
            destination.is_absolute() && !destination.exists(),
            "Saved-media destination must be a brand-new absolute staging root"
        );
        let parent = destination.parent().context("Missing staging parent")?;
        let _parent_pins = Parents::pin(parent)?;
        let parent = parent.canonicalize()?;
        let leaf = destination.file_name().context("Missing stage name")?;
        let destination = parent.join(leaf);
        ensure!(
            !destination.starts_with(&self.root) && !self.root.starts_with(&destination),
            "Never stage saved media into or around the source portable"
        );
        self.legacy.verify()?;
        ensure!(
            digest(&self.legacy.bytes(4 * 1024 * 1024, cancel)?) == self.legacy_sha,
            "Legacy catalog changed after dry-run"
        );
        if let Some(meta) = &self.video_metadata {
            meta.verify()?;
        }
        fs::create_dir(&destination)?;
        let _stage_pins = Parents::pin(&destination)?;
        let nonce = digest(format!("{}:{}", self.legacy_sha, destination.display()).as_bytes())
            [..48]
            .to_string();
        let mut service = Service::open(&destination, self.stock.clone(), &nonce)?;
        let data = crate::jukebox_local::ensure_child(&destination.join("Data"), "Rust")?;
        let provenance = crate::jukebox_local::ensure_child(&data, "LegacyImport")?;
        write_new(
            &provenance.join("legacy-jukebox-library.json"),
            &self.legacy_bytes,
        )?;
        if let Some(metadata) = &mut self.video_metadata {
            write_new(
                &provenance.join("legacy-mp4-library.json"),
                &metadata.bytes(1024 * 1024, cancel)?,
            )?;
        }
        let mut mapping = BTreeMap::<String, (String, String)>::new();
        let mut playlist_ids = BTreeMap::new();
        let total = self.bundles.len() + self.videos.len();
        let mut done = 0;
        for bundle in &mut self.bundles {
            check(cancel)?;
            let import = imported(bundle, &self.music, &self.stock, cancel)?;
            ensure!(
                import
                    .songs
                    .iter()
                    .map(signature)
                    .collect::<Result<Vec<_>>>()?
                    == bundle.signatures,
                "Legacy resources differ from the reviewed dry-run"
            );
            let id = digest(format!("{}:{}", self.legacy_sha, bundle.playlist.id).as_bytes())[..48]
                .to_string();
            let mut fresh = vec![];
            let mut fresh_keys = vec![];
            for song in import.songs {
                let key = song.metadata.key();
                let sig = signature(&song)?;
                if let Some((previous, _)) = mapping.get(&key) {
                    ensure!(previous == &sig, "Legacy shared song changed");
                    continue;
                }
                if song.native_audio.is_none() {
                    mapping.insert(key.clone(), (sig, key));
                } else {
                    fresh_keys.push((key, sig));
                    fresh.push(song);
                }
            }
            service.accept_import(
                package::Import {
                    name: bundle.playlist.name.clone(),
                    songs: fresh,
                    warnings: vec![],
                },
                &id,
            )?;
            let added = service
                .view()
                .playlists
                .iter()
                .find(|p| p.id == id)
                .context("Staged playlist was not saved")?
                .track_keys
                .clone();
            ensure!(
                added.len() == fresh_keys.len(),
                "Imported song count differs"
            );
            for ((old, sig), new) in fresh_keys.into_iter().zip(added) {
                mapping.insert(old, (sig, new));
            }
            let ordered = bundle
                .playlist
                .track_keys
                .iter()
                .map(|key| {
                    mapping
                        .get(key)
                        .map(|(_, v)| v.clone())
                        .context("Missing imported song mapping")
                })
                .collect::<Result<Vec<_>>>()?;
            service.set_order(&id, ordered, &nonce)?;
            playlist_ids.insert(bundle.playlist.id.clone(), id);
            done += 1;
            progress(done, total)?;
        }
        ensure!(
            service.view().active_playlist_id.is_none(),
            "Migration must leave Default selected"
        );
        for stock in &self.stock.tracks {
            ensure!(
                service
                    .view()
                    .music
                    .tracks
                    .iter()
                    .find(|t| t.key() == stock.key())
                    == Some(stock),
                "Current stock identity was overwritten"
            );
        }
        drop(service);
        let videos_root =
            crate::jukebox_local::ensure_child(&destination.join("Library"), "Videos")?;
        let _mp4 = crate::jukebox_local::ensure_child(&videos_root, "mp4")?;
        for video in &mut self.videos {
            check(cancel)?;
            video.source.verify()?;
            video.source.file.seek(SeekFrom::Start(0))?;
            let path = destination.join(&video.item.relative_path);
            let mut output = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .share_mode(1)
                .open(&path)?;
            let mut h = Sha256::new();
            let mut bytes = 0u64;
            let mut block = vec![0; 1024 * 1024];
            loop {
                check(cancel)?;
                let n = video.source.file.read(&mut block)?;
                if n == 0 {
                    break;
                }
                output.write_all(&block[..n])?;
                h.update(&block[..n]);
                bytes += n as u64;
            }
            output.sync_all()?;
            video.source.verify()?;
            ensure!(
                bytes == video.item.bytes && format!("{:x}", h.finalize()) == video.item.sha256,
                "Saved MP4 copy/source hash differs"
            );
            output.seek(SeekFrom::Start(0))?;
            let mut verify = Sha256::new();
            loop {
                check(cancel)?;
                let n = output.read(&mut block)?;
                if n == 0 {
                    break;
                }
                verify.update(&block[..n]);
            }
            ensure!(
                format!("{:x}", verify.finalize()) == video.item.sha256,
                "Saved MP4 copy readback differs"
            );
            done += 1;
            progress(done, total)?;
        }
        write_new(
            &data.join("startup-videos.json"),
            &serde_json::to_vec_pretty(&self.video_catalog)?,
        )?;
        let result = Staged {
            schema: "nba2k27-legacy-media-staged-v1".into(),
            source_catalog_sha256: self.legacy_sha.clone(),
            report: self.report.clone(),
            playlist_ids,
            track_keys: mapping.into_iter().map(|(k, (_, v))| (k, v)).collect(),
            videos: self.video_catalog.clone(),
            active_playlist_id: None,
            startup_video_enabled: false,
        };
        check(cancel)?;
        write_new(
            &data.join("legacy-media-import-complete.json"),
            &serde_json::to_vec_pretty(&result)?,
        )?;
        Ok(result)
    }
}
pub(crate) struct ResolvedVideo {
    source: Source,
    pub item: VideoItem,
}
impl ResolvedVideo {
    pub fn path(&self) -> &Path {
        &self.source.path
    }
}
/// Catalog-only UI view. No blind top-level directory scan and no playback
/// claim; selected source bytes are checked separately by resolve_video().
pub(crate) fn video_catalog(portable: &Path, cancel: &AtomicBool) -> Result<VideoCatalog> {
    let _root = Parents::pin(portable)?;
    let mut doc = Source::open(&portable.join("Data/Rust/startup-videos.json"), 1024 * 1024)?;
    let value: VideoCatalog = serde_json::from_slice(&doc.bytes(1024 * 1024, cancel)?)?;
    ensure!(
        value.schema == VIDEO_SCHEMA && value.items.len() <= 1000,
        "Unsupported native video catalog"
    );
    let mut ids = BTreeSet::new();
    for item in &value.items {
        ensure!(
            video_id(&item.id)
                && ids.insert(item.id.clone())
                && !item.name.trim().is_empty()
                && item.name.chars().count() <= 120
                && !item.name.chars().any(char::is_control)
                && item.relative_path == format!("Library/Videos/mp4/{}.mp4", item.id)
                && catalog::hex(&item.sha256, 64)
                && (1..=startup_mp4::MAX_BYTES).contains(&item.bytes)
                && (1..=600000).contains(&item.duration_ms)
                && item.width > 0
                && item.height > 0
                && !item.codecs.is_empty()
                && item.codecs.len() <= 3
                && item
                    .codecs
                    .iter()
                    .all(|c| ["H.264", "AV1", "VP9"].contains(&c.as_str()))
                && item.codecs.iter().collect::<BTreeSet<_>>().len() == item.codecs.len()
                && item.playback_probe_required,
            "Native video catalog has an invalid or ambiguous entry"
        );
    }
    ensure!(
        value.selected.as_ref().is_none_or(|id| ids.contains(id)),
        "Selected video ID is absent from its catalog"
    );
    Ok(value)
}
/// Returns a retained ordinary source lease. The player may acquire its own
/// pin before this lease drops; callers must never accept a renderer path.
pub(crate) fn resolve_video(
    portable: &Path,
    id: &str,
    cancel: &AtomicBool,
) -> Result<ResolvedVideo> {
    ensure!(video_id(id), "Invalid selected video ID");
    let catalog = video_catalog(portable, cancel)?;
    let matches: Vec<_> = catalog.items.into_iter().filter(|i| i.id == id).collect();
    ensure!(matches.len() == 1, "Selected video is missing or ambiguous");
    let item = matches.into_iter().next().unwrap();
    ensure!(
        item.relative_path == format!("Library/Videos/mp4/{id}.mp4")
            && catalog::hex(&item.sha256, 64),
        "Selected video escaped fixed catalog path"
    );
    let mut source = Source::open(&portable.join(&item.relative_path), startup_mp4::MAX_BYTES)?;
    ensure!(
        source.stamp.size == item.bytes && source.sha(cancel)? == item.sha256,
        "Selected MP4 no longer matches its retained catalog"
    );
    let info = startup_mp4::inspect(&mut source.file, "mp4")?;
    source.verify()?;
    ensure!(
        info.bytes == item.bytes
            && info.duration_ms == item.duration_ms
            && codecs(&info) == item.codecs
            && info.max_width == item.width
            && info.max_height == item.height,
        "Selected MP4 metadata differs"
    );
    Ok(ResolvedVideo { source, item })
}

#[cfg(test)]
#[path = "legacy_saved_media_tests.rs"]
mod tests;
