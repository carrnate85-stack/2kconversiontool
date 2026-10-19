//! Explicit disk-only acceptance against the one frozen saved-media stage.
//! No Service::open/recover, create/write/delete, game process, decoder child,
//! HTTP server, UI, Steam or audio endpoint is called. All derived selections
//! and Ogg/music.iff/config payloads exist in memory only.
#![allow(dead_code)]
#[path = "../../../src/jukebox_api.rs"]
mod jukebox_api;
#[path = "../../../src/jukebox_audio.rs"]
mod jukebox_audio;
#[path = "../../../src/jukebox_catalog.rs"]
mod jukebox_catalog;
#[path = "../../../src/jukebox_decode_worker.rs"]
mod jukebox_decode_worker;
#[path = "../../../src/jukebox_decoder.rs"]
mod jukebox_decoder;
#[path = "../../../src/jukebox_files.rs"]
mod jukebox_files;
#[path = "../../../src/jukebox_launch.rs"]
mod jukebox_launch;
#[path = "../../../src/jukebox_local.rs"]
mod jukebox_local;
#[path = "../../../src/jukebox_package.rs"]
mod jukebox_package;
#[path = "../../../src/jukebox_service.rs"]
mod jukebox_service;
#[path = "../../../src/jukebox_v2_resources.rs"]
mod jukebox_v2_resources;

use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Read,
    mem::size_of,
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Component, Path, PathBuf, Prefix},
    time::Instant,
};
use windows::Win32::{Foundation::HANDLE, Storage::FileSystem::*};
const STAGE: &str = "artifacts/legacy-media-staging-20261016-native";
const EXPECTED_INVENTORY: &str = "76b3d222e19e7c6f577452a3aee3f3e8964ab52fb5199b0b69e1778832ca8ef0";

fn ordinary(file: &File, dir: bool) -> Result<()> {
    let meta = file.metadata()?;
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    unsafe {
        GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info)?;
    }
    ensure!(
        meta.file_attributes() & 0x400 == 0
            && if dir {
                meta.is_dir()
            } else {
                meta.is_file() && info.nNumberOfLinks == 1
            },
        "Read-only acceptance refuses linked/nonordinary paths"
    );
    Ok(())
}
fn pins(path: &Path) -> Result<Vec<File>> {
    ensure!(path.is_absolute(), "Acceptance root must be absolute");
    let (mut path_part, mut result) = (PathBuf::new(), vec![]);
    for component in path.components() {
        if let Component::Prefix(prefix) = component {
            ensure!(
                matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)),
                "No network/device source"
            );
        }
        ensure!(
            !matches!(component, Component::ParentDir | Component::CurDir),
            "No ambiguous source"
        );
        path_part.push(component.as_os_str());
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        let file = fs::OpenOptions::new()
            .access_mode(FILE_LIST_DIRECTORY.0 | FILE_READ_ATTRIBUTES.0)
            .share_mode(3)
            .custom_flags(0x02200000)
            .open(&path_part)?;
        ordinary(&file, true)?;
        result.push(file);
    }
    Ok(result)
}
fn file_hash(path: &Path) -> Result<(u64, String)> {
    let mut file = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .custom_flags(0x00200000)
        .open(path)?;
    ordinary(&file, false)?;
    let before = file.metadata()?;
    let mut id = FILE_ID_INFO::default();
    unsafe {
        GetFileInformationByHandleEx(
            HANDLE(file.as_raw_handle()),
            FileIdInfo,
            &mut id as *mut _ as _,
            size_of::<FILE_ID_INFO>() as u32,
        )?;
    }
    ensure!(
        id.FileId.Identifier.iter().any(|b| *b != 0),
        "Unknown source identity"
    );
    let (mut bytes, mut hash) = (0u64, Sha256::new());
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
        bytes += n as u64;
    }
    let after = file.metadata()?;
    ensure!(
        bytes == before.len()
            && before.len() == after.len()
            && before.last_write_time() == after.last_write_time(),
        "Stage changed during inventory read"
    );
    Ok((bytes, format!("{:x}", hash.finalize())))
}
#[derive(Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Inventory {
    files: usize,
    bytes: u64,
    sha256: String,
}
fn inventory(root: &Path) -> Result<Inventory> {
    fn walk(
        root: &Path,
        directory: &Path,
        rows: &mut BTreeMap<String, (u64, String)>,
    ) -> Result<()> {
        let _pins = pins(directory)?;
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            let meta = fs::symlink_metadata(&path)?;
            ensure!(
                meta.file_attributes() & 0x400 == 0,
                "Stage contains a reparse point"
            );
            if meta.is_dir() {
                walk(root, &path, rows)?;
            } else {
                ensure!(meta.is_file(), "Unexpected stage object");
                let relative = path
                    .strip_prefix(root)?
                    .to_str()
                    .context("Non-UTF8 stage path")?
                    .replace('\\', "/");
                // This fixed stage has ASCII hash/leaves. Rust byte ordering
                // is exactly StringComparer.Ordinal for this verified set.
                ensure!(
                    relative.is_ascii(),
                    "Inventory requires explicit UTF16 ordering for non-ASCII paths"
                );
                ensure!(
                    rows.insert(relative, file_hash(&path)?).is_none(),
                    "Duplicate stage leaf"
                );
            }
        }
        Ok(())
    }
    let mut rows = BTreeMap::new();
    walk(root, root, &mut rows)?;
    let text = rows
        .iter()
        .map(|(path, (length, hash))| format!("{path}|{length}|{hash}"))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Inventory {
        files: rows.len(),
        bytes: rows.values().map(|r| r.0).sum(),
        sha256: jukebox_catalog::digest(text.as_bytes()),
    })
}
fn ogg_pages(bytes: &[u8], expected_samples: u32) -> Result<usize> {
    let (mut at, mut sequence, mut final_granule) = (0usize, 0u32, 0u64);
    ensure!(
        bytes.windows(8).any(|v| v == b"OpusHead") && bytes.windows(8).any(|v| v == b"OpusTags"),
        "Preview lacks Opus headers"
    );
    while at < bytes.len() {
        ensure!(
            at + 27 <= bytes.len() && &bytes[at..at + 4] == b"OggS" && bytes[at + 4] == 0,
            "Preview has an invalid Ogg page"
        );
        let segments = bytes[at + 26] as usize;
        ensure!(
            at + 27 + segments <= bytes.len(),
            "Preview lacing escapes payload"
        );
        let size: usize = bytes[at + 27..at + 27 + segments]
            .iter()
            .map(|b| *b as usize)
            .sum();
        let end = at
            .checked_add(27 + segments)
            .and_then(|v| v.checked_add(size))
            .context("Preview size overflow")?;
        ensure!(
            end <= bytes.len()
                && u32::from_le_bytes(bytes[at + 18..at + 22].try_into()?) == sequence,
            "Preview sequence/size differs"
        );
        ensure!(
            sequence > 0 || bytes[at + 5] & 2 != 0,
            "Preview has no beginning page"
        );
        if end == bytes.len() {
            ensure!(bytes[at + 5] & 4 != 0, "Preview lacks end-of-stream");
        }
        final_granule = u64::from_le_bytes(bytes[at + 6..at + 14].try_into()?);
        sequence += 1;
        ensure!(sequence <= 100_000, "Preview page bound exceeded");
        at = end;
    }
    ensure!(
        sequence >= 3 && final_granule == expected_samples as u64,
        "Preview sample duration differs"
    );
    Ok(sequence as usize)
}
fn run() -> Result<()> {
    let started = Instant::now();
    let project = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .canonicalize()?;
    let stage = project.join(STAGE).canonicalize()?;
    ensure!(
        stage.starts_with(project.join("artifacts")),
        "Only the fixed workspace stage may be audited"
    );
    let game = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .context("Pass the installed game directory for disk-only stock verification")?,
    );
    let _root = pins(&stage)?;
    eprintln!("Inventory before (read-only)");
    let before = inventory(&stage)?;
    ensure!(
        before.files == 382 && before.bytes == 939811052 && before.sha256 == EXPECTED_INVENTORY,
        "Frozen stage inventory differs; no acceptance was attempted"
    );
    let document = jukebox_local::Document::open(&stage.join("Data/Jukebox"))?;
    let (raw, revision) = document.load()?;
    let raw = raw.context("Complete saved catalog absent")?;
    let state: jukebox_service::State = serde_json::from_slice(&raw)?;
    ensure!(
        state.schema == "nba2k27-rust-jukebox-library-v1"
            && state.music.tracks.len() == 321
            && state.audio.len() == 157
            && state.playlists.len() == 6
            && state.active_playlist_id.is_none(),
        "Saved catalog counts/default differ"
    );
    state.music.validate()?;
    let stock = jukebox_v2_resources::read_stock(&game, || Ok(()))?.music;
    ensure!(
        stock.tracks.len() == 164
            && stock.tracks.iter().filter(|t| t.collection == 1).count() == 93,
        "Installed stock counts differ"
    );
    ensure!(
        state.stock_sha256 == jukebox_catalog::digest(&serde_json::to_vec(&stock)?)
            && state.music.extra == stock.extra,
        "Stock metadata/fingerprint differs"
    );
    let by_key: BTreeMap<_, _> = state.music.tracks.iter().map(|t| (t.key(), t)).collect();
    let stock_keys: BTreeSet<_> = stock.tracks.iter().map(|t| t.key()).collect();
    for original in &stock.tracks {
        ensure!(
            by_key.get(&original.key()).is_some_and(|t| *t == original),
            "A current stock track was altered by import"
        );
    }
    ensure!(
        state
            .music
            .tracks
            .iter()
            .filter(|t| !stock_keys.contains(&t.key()))
            .count()
            == 157,
        "Custom song classification differs"
    );
    ensure!(
        !jukebox_launch::freeze_saved(&stage)?.enabled(),
        "Saved Default was not preserved"
    );
    let expected = [20, 55, 28, 29, 14, 11];
    let mut plans = vec![];
    for (index, playlist) in state.playlists.iter().enumerate() {
        ensure!(
            playlist.track_keys.len() == expected[index]
                && playlist
                    .track_keys
                    .iter()
                    .all(|k| state.audio.contains_key(k)),
            "Visible saved order/length differs"
        );
        let mut selected = state.clone();
        selected.active_playlist_id = Some(playlist.id.clone());
        let fingerprint = jukebox_catalog::digest(&serde_json::to_vec(&selected)?);
        let seal = jukebox_launch::Plan::from_state(&selected, &fingerprint)?;
        seal.validate()?;
        let selection = seal.selection.as_ref().context("Selected plan absent")?;
        ensure!(
            selection.track_keys == playlist.track_keys
                && seal.stock_overrides() == vec!["music.iff"],
            "Sealed order/override differs"
        );
        let music = state.music.selected(&playlist.track_keys)?;
        let prepared = jukebox_catalog::plan_with_stock(&music, &stock)?;
        ensure!(
            prepared.mappings.len() == expected[index]
                && prepared.music.tracks.len() == expected[index],
            "Native mapping count differs"
        );
        prepared.policy.validate()?;
        ensure!(
            prepared.policy.main.bytes().filter(|b| *b == b'1').count() == expected[index]
                && prepared
                    .policy
                    .custom
                    .bytes()
                    .filter(|b| *b == b'1')
                    .count()
                    == expected[index]
                && prepared.policy.park.bytes().all(|b| b == b'0'),
            "Native fixed masks differ"
        );
        for ((original, mapped), mapping) in music
            .tracks
            .iter()
            .zip(&prepared.music.tracks)
            .zip(&prepared.mappings)
        {
            ensure!(
                !mapping.source_is_stock
                    && mapping.source == jukebox_catalog::audio_path(original.bank)
                    && mapping.target == jukebox_catalog::audio_path(mapped.bank)
                    && mapping.slot == mapped.bank,
                "Mapping source/target authority differs"
            );
            let mut restored = mapped.clone();
            restored.bank = original.bank;
            restored.unique = original.unique;
            ensure!(&restored == original, "Native remap changed metadata/order");
        }
        let iff = jukebox_package::build_music(&prepared.music)?;
        ensure!(
            jukebox_package::read_music(&iff)? == prepared.music,
            "Native music.iff readback differs"
        );
        let config = jukebox_launch::append_loader_config(b"SCHEMA=1\r\n", Some(&prepared.policy))?;
        ensure!(
            config.len() <= 2047
                && config
                    .windows(b"AUDIO_SLOT_COUNT=71\r\n".len())
                    .any(|v| v == b"AUDIO_SLOT_COUNT=71\r\n"),
            "Native loader fixed count absent"
        );
        plans.push(json!({"playlistIndex":index+1,"songs":expected[index],"savedOrderPreserved":true,"immutableSealSha256":jukebox_catalog::digest(&serde_json::to_vec(&seal)?),"nativeMusicIffSha256":jukebox_catalog::digest(&iff),"mainMask":prepared.policy.main,"customMask":prepared.policy.custom,"parkMask":prepared.policy.park,"loaderConfigBytes":config.len(),"gamePublication":false}));
    }
    let (mut native_bytes, mut preview_bytes, mut pages, mut maximum) =
        (0u64, 0u64, 0usize, 0usize);
    let mut preview_hash = Sha256::new();
    let mut art = BTreeSet::new();
    for (index, (key, blob)) in state.audio.iter().enumerate() {
        let bytes = jukebox_local::read_cached_bytes(
            &stage.join("Library/Jukebox/Audio"),
            &blob.native.sha256,
            blob.native.size,
        )?;
        let info = jukebox_audio::inspect(&bytes)?;
        ensure!(
            info.codec == jukebox_audio::Codec::Opus,
            "Saved audio unexpectedly requires conversion"
        );
        let ogg = jukebox_audio::preview_ogg(&bytes, 0)?;
        pages += ogg_pages(&ogg, info.frames)?;
        native_bytes += bytes.len() as u64;
        preview_bytes += ogg.len() as u64;
        maximum = maximum.max(bytes.len());
        preview_hash.update(key.as_bytes());
        preview_hash.update(jukebox_catalog::digest(&ogg).as_bytes());
        for (png, blob) in [(false, &blob.artwork), (true, &blob.preview)] {
            if let Some(blob) = blob {
                let bytes = jukebox_local::read_art(
                    &stage.join("Library/Jukebox/Artwork"),
                    &blob.sha256,
                    blob.size,
                    png,
                )?;
                ensure!(
                    if png {
                        bytes.starts_with(b"\x89PNG\r\n\x1a\n")
                    } else {
                        bytes.starts_with(b"TLD ")
                    },
                    "Artwork native signature differs"
                );
                art.insert((png, blob.sha256.clone()));
            }
        }
        if (index + 1) % 20 == 0 {
            eprintln!(
                "Validated/remuxed {} of157 saved songs (no output/audio/child)",
                index + 1
            );
        }
    }
    ensure!(
        native_bytes == 855007836 && art.len() == 216,
        "Saved audio/art sizes differ"
    );
    let (current, current_revision) = document.load()?;
    ensure!(
        current.as_ref() == Some(&raw) && current_revision == revision,
        "Saved catalog changed during acceptance"
    );
    ensure!(
        !jukebox_launch::freeze_saved(&stage)?.enabled(),
        "Derived selections escaped into saved catalog"
    );
    eprintln!("Inventory after (read-only)");
    let after = inventory(&stage)?;
    ensure!(
        before == after,
        "Frozen stage changed during read-only acceptance"
    );
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"schema":"nba2k27-actual-saved-jukebox-acceptance-v1","passed":true,"readOnly":true,"stage":STAGE,"inventoryBefore":before,"inventoryAfter":after,"stockTracks":stock.tracks.len(),"stockMainTracks":93,"stockMetadataPreserved":true,"catalogTracks":state.music.tracks.len(),"savedPlaylists":state.playlists.len(),"activePlaylistId":state.active_playlist_id,"derivedPlansInMemoryOnly":plans,"nativeOpusTracksValidated":state.audio.len(),"nativeAudioBytes":native_bytes,"oggPreviewTracksRemuxed":state.audio.len(),"oggPreviewBytes":preview_bytes,"oggPagesValidated":pages,"previewManifestSha256":format!("{:x}",preview_hash.finalize()),"maximumSingleNativeAudioBytes":maximum,"uniqueArtworkBlobsValidated":art.len(),"audioDecodedOrReencoded":false,"gameProcessAccess":false,"gamePublication":false,"steamLaunch":false,"httpServer":false,"audioOutput":false,"browserPlaybackVerified":false,"sourceOrStageWrites":false,"elapsedMs":started.elapsed().as_millis()})
        )?
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}
