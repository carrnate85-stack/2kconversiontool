//! Bounded music.iff / .2kplaylist import and export. Archives are never
//! extracted by member path. The caller commits a fully validated import only.
#![allow(dead_code)]
use crate::{
    jukebox_audio as audio,
    jukebox_catalog::{self as catalog, Music, Track},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read, Seek, SeekFrom, Write},
};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};
pub(crate) const MAX_SEGMENT: usize = 384 * 1024 * 1024;
const MAX_ENTRY: usize = 256 * 1024 * 1024;
pub(crate) type Entries = BTreeMap<String, Vec<u8>>;
fn u16_at(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes(b[i..i + 2].try_into().unwrap())
}
fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes(b[i..i + 4].try_into().unwrap())
}
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && name.is_ascii()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_./-".contains(&b))
        && name
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
}
/// ZIP64, spanned/encrypted archives, links, aliases, overlaps and zip bombs
/// are rejected before any decompression. WPF export uses ordinary ZIP only.
pub(crate) fn unpack(source: &[u8], limit: usize, count_limit: usize) -> Result<Entries> {
    ensure!(
        (22..=limit).contains(&source.len()) && limit <= MAX_SEGMENT,
        "Invalid playlist ZIP size"
    );
    let eocd = (source.len().saturating_sub(65557)..=source.len() - 22)
        .rev()
        .find(|i| {
            source[*i..].starts_with(b"PK\x05\x06")
                && *i + 22 + u16_at(source, *i + 20) as usize == source.len()
        })
        .context("Playlist ZIP end record is invalid")?;
    let count = u16_at(source, eocd + 10) as usize;
    let central = u32_at(source, eocd + 16) as usize;
    ensure!(
        u16_at(source, eocd + 4) == 0
            && u16_at(source, eocd + 6) == 0
            && u16_at(source, eocd + 8) as usize == count
            && (1..=count_limit).contains(&count)
            && count < 65535
            && central.checked_add(u32_at(source, eocd + 12) as usize) == Some(eocd),
        "Spanned, ZIP64 or oversized playlist archive"
    );
    let mut archive = ZipArchive::new(Cursor::new(source))?;
    ensure!(
        archive.len() == count
            && archive.offset() == 0
            && archive.central_directory_start() == central as u64,
        "Playlist ZIP structure differs"
    );
    let (mut names, mut spans, mut expanded) = (BTreeSet::new(), Vec::new(), 0usize);
    for i in 0..count {
        let entry = archive.by_index(i)?;
        let name = std::str::from_utf8(entry.name_raw())?;
        ensure!(
            safe_name(name)
                && !entry.is_dir()
                && !entry.encrypted()
                && entry
                    .unix_mode()
                    .is_none_or(|m| m & 0o170000 == 0 || m & 0o170000 == 0o100000)
                && names.insert(name.to_ascii_lowercase())
                && matches!(
                    entry.compression(),
                    CompressionMethod::Stored | CompressionMethod::Deflated
                ),
            "Unsafe, duplicate or unsupported playlist member"
        );
        let size = usize::try_from(entry.size())?;
        ensure!(size <= MAX_ENTRY, "Playlist member is oversized");
        expanded = expanded
            .checked_add(size)
            .context("Playlist expanded size overflow")?;
        ensure!(expanded <= limit, "Playlist expands beyond its bound");
        let head = usize::try_from(entry.header_start())?;
        let start = usize::try_from(entry.data_start())?;
        let mut end = start
            .checked_add(usize::try_from(entry.compressed_size())?)
            .context("Playlist payload overflow")?;
        ensure!(
            head + 30 <= central
                && source[head..].starts_with(b"PK\x03\x04")
                && start <= central
                && end <= central,
            "Invalid local ZIP member bounds"
        );
        let flags = u16_at(source, head + 6);
        let method = u16_at(source, head + 8);
        let n = u16_at(source, head + 26) as usize;
        let x = u16_at(source, head + 28) as usize;
        ensure!(
            flags & !0x0808 == 0
                && method
                    == if entry.compression() == CompressionMethod::Stored {
                        0
                    } else {
                        8
                    }
                && head + 30 + n + x == start
                && source[head + 30..head + 30 + n] == *entry.name_raw(),
            "Local/central ZIP member differs"
        );
        if flags & 8 == 0 {
            ensure!(
                u32_at(source, head + 14) == entry.crc32()
                    && u32_at(source, head + 18) as u64 == entry.compressed_size()
                    && u32_at(source, head + 22) as u64 == entry.size(),
                "Local ZIP sizes/CRC differ"
            );
        } else {
            if end + 4 <= central && source[end..].starts_with(b"PK\x07\x08") {
                end += 4;
            }
            ensure!(
                end + 12 <= central
                    && u32_at(source, end) == entry.crc32()
                    && u32_at(source, end + 4) as u64 == entry.compressed_size()
                    && u32_at(source, end + 8) as u64 == entry.size(),
                "ZIP data descriptor differs"
            );
            end += 12;
        }
        spans.push((head, end));
    }
    spans.sort_unstable();
    ensure!(
        spans.first().is_some_and(|s| s.0 == 0) && spans.windows(2).all(|s| s[0].1 <= s[1].0),
        "Overlapping/prefixed playlist members"
    );
    let mut entries = Entries::new();
    for i in 0..count {
        let mut entry = archive.by_index(i)?;
        let size = entry.size() as usize;
        let name = entry.name().to_ascii_lowercase();
        let mut data = Vec::with_capacity(size);
        entry
            .by_ref()
            .take(size as u64 + 1)
            .read_to_end(&mut data)?;
        ensure!(
            data.len() == size && crc32fast::hash(&data) == entry.crc32(),
            "Playlist decompression/CRC failed"
        );
        entries.insert(name, data);
    }
    Ok(entries)
}
pub(crate) fn pack(entries: &Entries, limit: usize, count_limit: usize) -> Result<Vec<u8>> {
    ensure!(
        !entries.is_empty() && entries.len() <= count_limit,
        "Invalid playlist member count"
    );
    let mut seen = BTreeSet::new();
    let mut total = 0usize;
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in entries {
        ensure!(
            safe_name(name) && seen.insert(name.to_ascii_lowercase()) && data.len() <= MAX_ENTRY,
            "Invalid playlist output member"
        );
        total = total
            .checked_add(data.len())
            .context("Playlist output overflow")?;
        ensure!(total <= limit, "Playlist is too large for one segment");
        zip.start_file(
            name,
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
        )?;
        zip.write_all(data)?;
    }
    let bytes = zip.finish()?.into_inner();
    unpack(&bytes, limit, count_limit)?;
    Ok(bytes)
}
pub(crate) fn read_music(bytes: &[u8]) -> Result<Music> {
    let entries = unpack(bytes, catalog::MAX_IFF, 512)?;
    let text = entries
        .get("music.json")
        .context("music.iff has no music.json")?;
    let text = text.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(text);
    let music: Music = serde_json::from_slice(text)?;
    music.validate()?;
    Ok(music)
}
pub(crate) fn build_music(music: &Music) -> Result<Vec<u8>> {
    music.validate()?;
    let mut text = serde_json::to_vec_pretty(music)?;
    text.push(b'\n');
    pack(
        &Entries::from([("music.json".into(), text)]),
        catalog::MAX_IFF,
        512,
    )
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Song {
    pub kind: String,
    pub source_key: String,
    pub metadata: Track,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artwork_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artwork_preview_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_codec: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_native_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_wave_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_pcm_bytes: Option<usize>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Multipart {
    pub id: String,
    pub part: usize,
    pub count: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct Manifest {
    pub schema: String,
    pub name: String,
    pub tracks: Vec<Song>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multipart: Option<Multipart>,
}
pub(crate) struct ImportedSong {
    pub metadata: Track,
    pub native_audio: Option<Vec<u8>>,
    pub artwork: Option<Vec<u8>>,
    pub artwork_preview: Option<Vec<u8>>,
    pub metadata_override: bool,
}
pub(crate) struct Import {
    pub name: String,
    pub songs: Vec<ImportedSong>,
    pub warnings: Vec<String>,
}
fn validate_manifest(manifest: &Manifest) -> Result<()> {
    ensure!(
        [
            "nba2k27-2kplaylist-v1",
            "nba2k27-2kplaylist-v2",
            "nba2k27-2kplaylist-v3",
            "nba2k27-2kplaylist-v4"
        ]
        .contains(&manifest.schema.as_str())
            && !manifest.name.trim().is_empty()
            && manifest.name.chars().count() <= 80
            && !manifest.name.chars().any(char::is_control)
            && !manifest.tracks.is_empty()
            && manifest.tracks.len() <= catalog::MAX_TRACKS
            && manifest.warnings.len() <= 128
            && manifest.warnings.iter().all(|w| w.len() <= 4096),
        "Unsupported or invalid playlist manifest"
    );
    let mut keys = BTreeSet::new();
    for song in &manifest.tracks {
        song.metadata.validate()?;
        ensure!(
            keys.insert(&song.source_key) && song.source_key == song.metadata.key(),
            "Duplicate/mismatched playlist song identity"
        );
    }
    Ok(())
}
fn reference<'a>(
    entries: &'a Entries,
    name: &Option<String>,
    prefix: &str,
) -> Result<Option<&'a [u8]>> {
    name.as_ref()
        .map(|n| {
            ensure!(
                safe_name(n) && n.starts_with(prefix),
                "Playlist resource has the wrong folder"
            );
            entries
                .get(&n.to_ascii_lowercase())
                .map(Vec::as_slice)
                .context("Playlist resource is missing")
        })
        .transpose()
}
fn tld(bytes: &[u8]) -> Result<()> {
    ensure!(
        bytes.len() == 32800
            && &bytes[..4] == b"TLD "
            && bytes[10] == 71
            && bytes[11] == 1
            && u16_at(bytes, 16) == 256
            && u16_at(bytes, 18) == 256
            && u32_at(bytes, 24) == 32768
            && u32_at(bytes, 28) == 32768,
        "Invalid 256x256 BC1 cover artwork"
    );
    Ok(())
}
/// FLAC remains lossless and hash-verified. The host supplies a bounded decoder;
/// absence/failure returns an error before any library or Mods publication.
pub(crate) fn import_segment(
    bytes: &[u8],
    stock: &Music,
    mut decode_flac: impl FnMut(&[u8], usize) -> Result<Vec<u8>>,
) -> Result<Import> {
    let entries = unpack(bytes, MAX_SEGMENT, 512)?;
    let manifest: Manifest = serde_json::from_slice(
        entries
            .get("playlist.json")
            .context("Playlist manifest is missing")?,
    )?;
    ensure!(
        manifest
            .multipart
            .as_ref()
            .is_none_or(|m| m.count == 1 && m.part == 1),
        "This is one part of a multipart playlist; import the complete container, not a partial library"
    );
    import_entries(&manifest, &entries, stock, &mut decode_flac)
}
pub(crate) fn import_entries(
    manifest: &Manifest,
    entries: &Entries,
    stock: &Music,
    decode_flac: &mut impl FnMut(&[u8], usize) -> Result<Vec<u8>>,
) -> Result<Import> {
    validate_manifest(manifest)?;
    stock.validate()?;
    let (mut songs, mut seen) = (Vec::new(), BTreeSet::new());
    for song in &manifest.tracks {
        song.metadata.validate()?;
        ensure!(
            seen.insert(song.source_key.clone()) && song.source_key == song.metadata.key(),
            "Duplicate/mismatched playlist song identity"
        );
        let native = match song.kind.as_str() {
            "stock-reference" => {
                ensure!(
                    song.audio_file.is_none()
                        && song.artwork_file.is_none()
                        && song.artwork_preview_file.is_none(),
                    "Stock reference must not embed custom media"
                );
                let actual = stock
                    .tracks
                    .iter()
                    .find(|t| t.key() == song.source_key)
                    .context("Stock playlist reference is absent in this game build")?;
                songs.push(ImportedSong {
                    metadata: actual.clone(),
                    native_audio: None,
                    artwork: None,
                    artwork_preview: None,
                    metadata_override: false,
                });
                continue;
            }
            "stock-override" => {
                ensure!(
                    manifest.schema == "nba2k27-2kplaylist-v4"
                        && song.audio_file.is_none()
                        && song.audio_codec.is_none()
                        && song.audio_native_sha256.is_none()
                        && song.audio_wave_sha256.is_none()
                        && song.audio_pcm_bytes.is_none(),
                    "Stock override requires v4 and must not embed or reassign audio"
                );
                let actual = stock
                    .tracks
                    .iter()
                    .find(|t| t.key() == song.source_key)
                    .context("Overridden stock song absent in this build")?;
                ensure!(
                    song.metadata.cover == actual.cover
                        && song.metadata.extra == actual.extra
                        && !song.metadata.name.trim().is_empty()
                        && song.metadata.name == song.metadata.name.trim()
                        && song.metadata.artist == song.metadata.artist.trim(),
                    "Stock override may change only printable title/artist and paired artwork"
                );
                let art = reference(entries, &song.artwork_file, "artwork/")?;
                let preview = reference(entries, &song.artwork_preview_file, "artwork/")?;
                ensure!(
                    art.is_some() == preview.is_some(),
                    "Stock override artwork must include both native cover and PNG preview"
                );
                if let Some(a) = art {
                    tld(a)?;
                }
                if let Some(p) = preview {
                    ensure!(
                        p.len() >= 33
                            && p.len() <= 4 * 1024 * 1024
                            && p.starts_with(b"\x89PNG\r\n\x1a\n")
                            && &p[12..16] == b"IHDR"
                            && u32::from_be_bytes(p[16..20].try_into()?) == 256
                            && u32::from_be_bytes(p[20..24].try_into()?) == 256,
                        "Invalid bounded stock override PNG"
                    );
                }
                songs.push(ImportedSong {
                    metadata: song.metadata.clone(),
                    native_audio: None,
                    artwork: art.map(<[u8]>::to_vec),
                    artwork_preview: preview.map(<[u8]>::to_vec),
                    metadata_override: true,
                });
                continue;
            }
            "custom" => {
                ensure!(
                    song.metadata.custom()
                        && song.metadata.bank < 999
                        && !song.metadata.name.trim().is_empty()
                        && !song.metadata.artist.trim().is_empty(),
                    "Custom song must use a library bank and named metadata, not overwrite a stock identity"
                );
                let data = reference(entries, &song.audio_file, "audio/")?
                    .context("Custom song has no audio")?;
                match song.audio_codec.as_deref().unwrap_or("wav") {
                    "wav" => audio::wrap(data)?,
                    "native-opus" => {
                        ensure!(
                            song.audio_native_sha256
                                .as_ref()
                                .is_some_and(|h| catalog::hex(h, 64)
                                    && h.eq_ignore_ascii_case(&catalog::digest(data))),
                            "Native Opus integrity failed"
                        );
                        audio::parse_opus(data)?;
                        data.to_vec()
                    }
                    "flac" => {
                        let pcm = song
                            .audio_pcm_bytes
                            .context("FLAC decoded size is missing")?;
                        ensure!(
                            data.len() >= 42
                                && data.len() <= MAX_ENTRY
                                && &data[..4] == b"fLaC"
                                && data[4] & 127 == 0
                                && data[5..8] == [0, 0, 34]
                                && pcm >= 4
                                && pcm <= audio::MAX_FRAMES as usize * 4
                                && pcm % 4 == 0
                                && song
                                    .audio_wave_sha256
                                    .as_ref()
                                    .is_some_and(|h| catalog::hex(h, 64)),
                            "Invalid FLAC metadata"
                        );
                        let info = u64::from_be_bytes(data[18..26].try_into()?);
                        let samples = info & 0xfffffffff;
                        ensure!(
                            info >> 44 == 48000
                                && ((info >> 41) & 7) + 1 == 2
                                && ((info >> 36) & 31) + 1 == 16
                                && (samples == 0 || samples * 4 == pcm as u64),
                            "FLAC sample format differs"
                        );
                        let decoded = decode_flac(data, pcm)?;
                        ensure!(decoded.len() == pcm, "FLAC decoded size differs");
                        let wave = audio::pcm_wave(&decoded)?;
                        ensure!(
                            song.audio_wave_sha256
                                .as_ref()
                                .unwrap()
                                .eq_ignore_ascii_case(&catalog::digest(&wave)),
                            "FLAC decoded hash differs"
                        );
                        audio::wrap(&wave)?
                    }
                    _ => anyhow::bail!("Unsupported playlist audio codec"),
                }
            }
            _ => anyhow::bail!("Unsupported playlist song kind"),
        };
        let art = reference(entries, &song.artwork_file, "artwork/")?;
        if let Some(a) = art {
            tld(a)?;
        }
        let preview = reference(entries, &song.artwork_preview_file, "artwork/")?;
        if let Some(p) = preview {
            ensure!(
                p.len() <= 4 * 1024 * 1024 && p.starts_with(b"\x89PNG\r\n\x1a\n"),
                "Invalid cover preview"
            );
        }
        songs.push(ImportedSong {
            metadata: song.metadata.clone(),
            native_audio: Some(native),
            artwork: art.map(<[u8]>::to_vec),
            artwork_preview: preview.map(<[u8]>::to_vec),
            metadata_override: false,
        });
    }
    Ok(Import {
        name: manifest.name.trim().into(),
        songs,
        warnings: manifest.warnings.clone(),
    })
}
/// Reassign custom IDs in import order; exact stock references remain stock.
/// The returned library/playlist are staged values; the caller atomically saves.
pub(crate) fn merge(library: &Music, import: &Import) -> Result<(Music, Vec<String>)> {
    let mut next = library.clone();
    let mut keys = Vec::new();
    for song in &import.songs {
        let t = if song.native_audio.is_some() {
            let mut t = next.add_custom(
                &song.metadata.name,
                &song.metadata.artist,
                &song.metadata.cover,
            )?;
            t.extra = song.metadata.extra.clone();
            *next.tracks.last_mut().unwrap() = t.clone();
            t
        } else {
            let t = next
                .tracks
                .iter_mut()
                .find(|t| t.key() == song.metadata.key())
                .context("Referenced stock song is not in this library")?;
            if song.metadata_override {
                t.name = song.metadata.name.clone();
                t.artist = song.metadata.artist.clone();
            }
            t.clone()
        };
        keys.push(t.key());
    }
    next.validate()?;
    Ok((next, keys))
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Map;
    fn track() -> Track {
        Track {
            artist: "Artist".into(),
            collection: 1,
            cover: "001_helicopter".into(),
            name: "Custom".into(),
            bank: 72,
            unique: 130,
            extra: Map::new(),
        }
    }
    #[test]
    fn music_roundtrip_preserves_unknown_metadata() {
        let mut music = Music {
            tracks: vec![track()],
            extra: Map::new(),
        };
        music.extra.insert("Version".into(), 9.into());
        music.tracks[0].extra.insert("Mood".into(), "Test".into());
        assert_eq!(read_music(&build_music(&music).unwrap()).unwrap(), music);
    }
    #[test]
    fn archive_rejects_bad_crc_names_and_declared_zipbomb() {
        let good = pack(
            &Entries::from([("music.json".into(), b"test".to_vec())]),
            1024,
            4,
        )
        .unwrap();
        assert!(unpack(&good, 1024, 4).is_ok());
        let mut bad = good.clone();
        bad[14] ^= 1;
        assert!(unpack(&bad, 1024, 4).is_err());
        assert!(pack(&Entries::from([("../bad".into(), vec![])]), 1024, 4).is_err());
        let mut bad = good.clone();
        let at = bad.windows(4).position(|w| w == b"PK\x01\x02").unwrap();
        bad[at + 24..at + 28].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(unpack(&bad, 1024, 4).is_err());
    }
    #[test]
    fn wav_package_import_remaps_custom_ids_without_mutating_stock() {
        let wave = audio::pcm_wave(&[1, 2, 3, 4]).unwrap();
        let song = Song {
            kind: "custom".into(),
            source_key: track().key(),
            metadata: track(),
            audio_file: Some("audio/0001.wav".into()),
            artwork_file: None,
            artwork_preview_file: None,
            audio_codec: None,
            audio_native_sha256: None,
            audio_wave_sha256: None,
            audio_pcm_bytes: None,
        };
        let manifest = Manifest {
            schema: "nba2k27-2kplaylist-v1".into(),
            name: "Test".into(),
            tracks: vec![song],
            warnings: vec![],
            multipart: None,
        };
        let package = pack(
            &Entries::from([
                (
                    "playlist.json".into(),
                    serde_json::to_vec(&manifest).unwrap(),
                ),
                ("audio/0001.wav".into(), wave),
            ]),
            MAX_SEGMENT,
            512,
        )
        .unwrap();
        let library = Music {
            tracks: vec![track()],
            extra: Map::new(),
        };
        let imported = import_segment(&package, &library, |_, _| {
            anyhow::bail!("No FLAC in fixture")
        })
        .unwrap();
        assert!(imported.songs[0].native_audio.is_some());
        let (next, keys) = merge(&library, &imported).unwrap();
        assert_eq!(library.tracks.len(), 1);
        assert_eq!(next.tracks[1].bank, 73);
        assert_eq!(keys, vec!["1:73:131"]);
    }
    #[test]
    fn missing_resource_and_stock_reference_fail_before_commit() {
        let mut song = Song {
            kind: "custom".into(),
            source_key: track().key(),
            metadata: track(),
            audio_file: Some("audio/missing.wav".into()),
            artwork_file: None,
            artwork_preview_file: None,
            audio_codec: None,
            audio_native_sha256: None,
            audio_wave_sha256: None,
            audio_pcm_bytes: None,
        };
        let stock = Music {
            tracks: vec![],
            extra: Map::new(),
        };
        let mut manifest = Manifest {
            schema: "nba2k27-2kplaylist-v1".into(),
            name: "Test".into(),
            tracks: vec![song.clone()],
            warnings: vec![],
            multipart: None,
        };
        assert!(
            import_entries(&manifest, &Entries::new(), &stock, &mut |_, _| Ok(vec![])).is_err()
        );
        song.kind = "stock-reference".into();
        song.metadata.bank = 1;
        song.metadata.unique = 0;
        song.source_key = song.metadata.key();
        song.audio_file = None;
        manifest.tracks = vec![song];
        assert!(
            import_entries(&manifest, &Entries::new(), &stock, &mut |_, _| Ok(vec![])).is_err()
        );
    }
}

const CONTAINER_MAGIC: &[u8] = b"2KPLAYLIST2\n";
const MAX_INDEX: usize = 4 * 1024 * 1024;
const MAX_FILE: u64 = 256 * 1024 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Part {
    offset: u64,
    length: usize,
    sha256: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Index {
    schema: String,
    manifest: Manifest,
    parts: Vec<Part>,
}
/// Own a retained read-only source handle here, not a path reopened per segment.
/// The filesystem adapter must deny concurrent writes/deletion and reject links.
pub(crate) struct Container<R> {
    source: R,
    index: Index,
    length: u64,
}
impl<R: Read + Seek> Container<R> {
    pub fn open(mut source: R) -> Result<Self> {
        let length = source.seek(SeekFrom::End(0))?;
        ensure!(
            length >= CONTAINER_MAGIC.len() as u64 + 24
                && length <= MAX_FILE + MAX_INDEX as u64 + 24,
            "Invalid playlist container length"
        );
        source.seek(SeekFrom::Start(0))?;
        let mut magic = [0; 12];
        source.read_exact(&mut magic)?;
        ensure!(
            magic == CONTAINER_MAGIC,
            "Not a segmented playlist container"
        );
        source.seek(SeekFrom::End(-24))?;
        let mut footer = [0; 24];
        source.read_exact(&mut footer)?;
        ensure!(
            &footer[..8] == b"2KPLIDX2",
            "Missing playlist container footer"
        );
        let offset = u64::from_le_bytes(footer[8..16].try_into()?);
        let size = u64::from_le_bytes(footer[16..].try_into()?);
        ensure!(
            offset >= 12
                && size > 0
                && size <= MAX_INDEX as u64
                && offset.checked_add(size) == Some(length - 24),
            "Invalid playlist container index bounds"
        );
        source.seek(SeekFrom::Start(offset))?;
        let mut data = vec![0; size as usize];
        source.read_exact(&mut data)?;
        let index: Index = serde_json::from_slice(&data)?;
        ensure!(
            index.schema == "nba2k27-playlist-container-v2"
                && (1..=4096).contains(&index.parts.len()),
            "Unsupported playlist container index"
        );
        validate_manifest(&index.manifest)?;
        let mut end = 12u64;
        for part in &index.parts {
            ensure!(
                part.offset == end
                    && (22..=MAX_SEGMENT).contains(&part.length)
                    && catalog::hex(&part.sha256, 64),
                "Invalid playlist segment declaration"
            );
            end = end
                .checked_add(part.length as u64)
                .context("Playlist segment overflow")?;
            ensure!(end <= offset, "Playlist segment overlaps its index");
        }
        ensure!(end == offset, "Playlist container has gaps/unindexed data");
        Ok(Self {
            source,
            index,
            length,
        })
    }
    pub fn manifest(&self) -> &Manifest {
        &self.index.manifest
    }
    pub fn parts(&self) -> usize {
        self.index.parts.len()
    }
    pub fn segment(&mut self, i: usize) -> Result<Entries> {
        let part = self
            .index
            .parts
            .get(i)
            .context("Playlist segment is absent")?;
        ensure!(
            self.source.seek(SeekFrom::End(0))? == self.length,
            "Playlist source length changed"
        );
        self.source.seek(SeekFrom::Start(part.offset))?;
        let mut data = vec![0; part.length];
        self.source.read_exact(&mut data)?;
        ensure!(
            catalog::digest(&data).eq_ignore_ascii_case(&part.sha256),
            "Playlist segment SHA256 failed"
        );
        unpack(&data, MAX_SEGMENT, 512)
    }
    /// Visits one validated segment at a time, bounded to 384MiB expanded data.
    /// The callback may stage private drafts, but MUST NOT commit the catalog
    /// until this method returns Ok. A later segment can still fail integrity.
    pub fn visit(
        &mut self,
        stock: &Music,
        mut decode: impl FnMut(&[u8], usize) -> Result<Vec<u8>>,
        mut accept: impl FnMut(ImportedSong) -> Result<()>,
    ) -> Result<()> {
        let mut offset = 0usize;
        let mut identity = None;
        for i in 0..self.parts() {
            let entries = self.segment(i)?;
            let manifest: Manifest = serde_json::from_slice(
                entries
                    .get("playlist.json")
                    .context("Segment has no manifest")?,
            )?;
            ensure!(
                manifest.name == self.index.manifest.name,
                "Playlist segment name differs"
            );
            let multi = manifest
                .multipart
                .as_ref()
                .context("Segment has no multipart identity")?;
            ensure!(
                multi.part == i + 1
                    && multi.count == self.parts()
                    && !multi.id.is_empty()
                    && multi.id.len() <= 128,
                "Playlist segment order/count differs"
            );
            if let Some(id) = &identity {
                ensure!(id == &multi.id, "Playlist segment identity differs");
            } else {
                identity = Some(multi.id.clone());
            }
            let end = offset
                .checked_add(manifest.tracks.len())
                .context("Playlist song count overflow")?;
            ensure!(
                self.index.manifest.tracks.get(offset..end) == Some(manifest.tracks.as_slice()),
                "Combined playlist order/metadata differs from segment"
            );
            let import = import_entries(&manifest, &entries, stock, &mut decode)?;
            for song in import.songs {
                accept(song)?;
            }
            offset = end;
        }
        ensure!(
            offset == self.index.manifest.tracks.len(),
            "Playlist is missing declared songs"
        );
        Ok(())
    }
}
/// Streaming container export; destination must be a fresh private file held by
/// the caller and published with a no-overwrite owned-handle rename on success.
pub(crate) fn write_container(
    mut out: impl Write,
    manifest: Manifest,
    segments: impl IntoIterator<Item = Result<Vec<u8>>>,
) -> Result<()> {
    out.write_all(CONTAINER_MAGIC)?;
    let (mut offset, mut parts) = (12u64, Vec::new());
    for bytes in segments {
        let bytes = bytes?;
        ensure!(
            (22..=MAX_SEGMENT).contains(&bytes.len()) && parts.len() < 4096,
            "Invalid playlist output segment"
        );
        unpack(&bytes, MAX_SEGMENT, 512)?;
        ensure!(
            offset
                .checked_add(bytes.len() as u64)
                .is_some_and(|n| n <= MAX_FILE),
            "Playlist container exceeds size bound"
        );
        parts.push(Part {
            offset,
            length: bytes.len(),
            sha256: catalog::digest(&bytes),
        });
        out.write_all(&bytes)?;
        offset += bytes.len() as u64;
    }
    ensure!(!parts.is_empty(), "Empty playlist container");
    validate_manifest(&manifest)?;
    let index = serde_json::to_vec(&Index {
        schema: "nba2k27-playlist-container-v2".into(),
        manifest,
        parts,
    })?;
    ensure!(
        index.len() <= MAX_INDEX,
        "Playlist output metadata exceeds bounds"
    );
    out.write_all(&index)?;
    out.write_all(b"2KPLIDX2")?;
    out.write_all(&offset.to_le_bytes())?;
    out.write_all(&(index.len() as u64).to_le_bytes())?;
    Ok(())
}

/// Produces a WPF-compatible ordinary ZIP segment, including exact stock
/// references. Native PCM exports as canonical WAV, native Opus stays lossless.
pub(crate) fn export_segment(
    name: &str,
    music: &Music,
    stock: &Music,
    audio: &BTreeMap<String, Vec<u8>>,
    multipart: Option<Multipart>,
) -> Result<(Manifest, Vec<u8>)> {
    music.validate()?;
    let (mut tracks, mut entries, mut schema) =
        (Vec::new(), Entries::new(), "nba2k27-2kplaylist-v1");
    stock.validate()?;
    let stock_keys: BTreeSet<_> = stock.tracks.iter().map(Track::key).collect();
    for (i, t) in music.tracks.iter().enumerate() {
        let custom = !stock_keys.contains(&t.key());
        ensure!(
            !custom || t.custom(),
            "Unknown song is not a stock reference or custom library identity"
        );
        let mut song = Song {
            kind: if custom { "custom" } else { "stock-reference" }.into(),
            source_key: t.key(),
            metadata: t.clone(),
            audio_file: None,
            artwork_file: None,
            artwork_preview_file: None,
            audio_codec: None,
            audio_native_sha256: None,
            audio_wave_sha256: None,
            audio_pcm_bytes: None,
        };
        if !custom {
            let original = stock.tracks.iter().find(|s| s.key() == t.key()).unwrap();
            ensure!(
                t.cover == original.cover && t.extra == original.extra,
                "Stock export may not change native cover or unknown metadata"
            );
            if t.name != original.name || t.artist != original.artist {
                song.kind = "stock-override".into();
                schema = "nba2k27-2kplaylist-v4";
            }
        }
        if custom {
            let source = audio
                .get(&t.key())
                .context("Custom song is missing; export refused rather than silently losing it")?;
            let native = audio::wrap(source)?;
            let info = audio::inspect(&native)?;
            let member = format!("audio/{:04}.wav", i + 1);
            let data = if info.codec == audio::Codec::Opus {
                if schema != "nba2k27-2kplaylist-v4" {
                    schema = "nba2k27-2kplaylist-v3";
                }
                song.audio_codec = Some("native-opus".into());
                song.audio_native_sha256 = Some(catalog::digest(&native));
                native
            } else if source.starts_with(b"RIFF") {
                source.clone()
            } else {
                audio::pcm_wave(&native[54..])?
            };
            song.audio_file = Some(member.clone());
            entries.insert(member, data);
        }
        tracks.push(song);
    }
    let manifest = Manifest {
        schema: schema.into(),
        name: name.into(),
        tracks,
        warnings: vec![],
        multipart,
    };
    validate_manifest(&manifest)?;
    entries.insert(
        "playlist.json".into(),
        serde_json::to_vec_pretty(&manifest)?,
    );
    let bytes = pack(&entries, MAX_SEGMENT, 512)?;
    Ok((manifest, bytes))
}

#[cfg(test)]
mod container_tests {
    use super::*;
    use serde_json::Map;
    fn track(bank: u16) -> Track {
        Track {
            artist: "Artist".into(),
            collection: 1,
            cover: "001_helicopter".into(),
            name: format!("Song {bank}"),
            bank,
            unique: bank + 60,
            extra: Map::new(),
        }
    }
    #[test]
    fn legacy_export_and_segmented_container_roundtrip_keep_order() {
        let music = Music {
            tracks: vec![track(73), track(72)],
            extra: Map::new(),
        };
        let native = audio::wrap(&audio::pcm_wave(&[0; 8]).unwrap()).unwrap();
        let audio = BTreeMap::from([(track(72).key(), native.clone()), (track(73).key(), native)]);
        let stock = Music {
            tracks: vec![],
            extra: Map::new(),
        };
        let mut combined = None;
        let mut segments = Vec::new();
        for (i, t) in music.tracks.iter().enumerate() {
            let (m, b) = export_segment(
                "Test",
                &Music {
                    tracks: vec![t.clone()],
                    extra: Map::new(),
                },
                &stock,
                &audio,
                Some(Multipart {
                    id: "stable-id".into(),
                    part: i + 1,
                    count: 2,
                }),
            )
            .unwrap();
            assert!(import_segment(&b, &music, |_, _| Ok(vec![])).is_err());
            if combined.is_none() {
                combined = Some(m.clone());
            } else {
                combined.as_mut().unwrap().tracks.extend(m.tracks);
            }
            segments.push(Ok(b));
        }
        let mut manifest = combined.unwrap();
        manifest.multipart = None;
        let mut bytes = Vec::new();
        write_container(&mut bytes, manifest, segments).unwrap();
        let mut container = Container::open(Cursor::new(bytes)).unwrap();
        assert_eq!(container.parts(), 2);
        let mut order = Vec::new();
        container
            .visit(
                &music,
                |_, _| Ok(vec![]),
                |s| {
                    order.push(s.metadata.bank);
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(order, vec![73, 72]);
    }
    #[test]
    fn container_hash_and_footer_are_enforced() {
        let music = Music {
            tracks: vec![track(72)],
            extra: Map::new(),
        };
        let audio = BTreeMap::from([(track(72).key(), audio::pcm_wave(&[0; 8]).unwrap())]);
        let (m, b) = export_segment(
            "Test",
            &music,
            &Music {
                tracks: vec![],
                extra: Map::new(),
            },
            &audio,
            Some(Multipart {
                id: "id".into(),
                part: 1,
                count: 1,
            }),
        )
        .unwrap();
        let mut bytes = Vec::new();
        write_container(&mut bytes, m, vec![Ok(b)]).unwrap();
        let mut bad = bytes.clone();
        bad[20] ^= 1;
        let mut container = Container::open(Cursor::new(bad)).unwrap();
        assert!(container.segment(0).is_err());
        let mut bad = bytes;
        let end = bad.len();
        bad[end - 24] ^= 1;
        assert!(Container::open(Cursor::new(bad)).is_err());
    }
    #[test]
    fn new_version2_stock_songs_share_as_references_not_embedded_custom() {
        let stock = Music {
            tracks: vec![track(93)],
            extra: Map::new(),
        };
        let (m, b) = export_segment("Stock", &stock, &stock, &BTreeMap::new(), None).unwrap();
        assert_eq!(m.tracks[0].kind, "stock-reference");
        let result = import_segment(&b, &stock, |_, _| Ok(vec![])).unwrap();
        assert!(result.songs[0].native_audio.is_none());
    }
}
