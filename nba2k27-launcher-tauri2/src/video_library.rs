//! Strict, portable MP4 catalog and no-clobber import. Local catalog changes
//! never enable startup playback or touch a game. Root owns the service mutex
//! and uses settings.video only as a validated stable ID, never as a path.
#![allow(dead_code)]
#[path = "video_library_document.rs"]
mod document;
use crate::{
    legacy_saved_media::{self as legacy, VideoCatalog, VideoItem},
    startup_mp4,
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
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
const SCHEMA: &str = "nba2k27-rust-startup-video-library-v1";
fn check(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Acquire),
        "MP4 import cancelled; original source unchanged"
    );
    Ok(())
}
fn valid_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}
fn local_source(path: &Path) -> Result<()> {
    ensure!(path.is_absolute(), "Choose an absolute local MP4 path");
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => ensure!(
                matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)),
                "Network/device MP4 paths are unsupported"
            ),
            Component::RootDir => {}
            Component::Normal(value) => {
                let value = value.to_string_lossy();
                ensure!(
                    !value.contains(':') && !value.chars().any(char::is_control),
                    "MP4 path contains an unsupported stream/control component"
                );
            }
            _ => anyhow::bail!("Choose a normalized local MP4 path"),
        }
    }
    Ok(())
}
fn name(value: &str) -> Result<()> {
    ensure!(
        !value.trim().is_empty()
            && value == value.trim()
            && value.encode_utf16().count() <= 120
            && !value.chars().any(char::is_control),
        "Use a printable video name of1–120 characters"
    );
    Ok(())
}
fn validate(catalog: &VideoCatalog) -> Result<()> {
    ensure!(
        catalog.schema == SCHEMA && catalog.items.len() <= 1000,
        "Unsupported video catalog"
    );
    let mut ids = BTreeSet::new();
    for item in &catalog.items {
        name(&item.name)?;
        ensure!(
            valid_id(&item.id)
                && ids.insert(item.id.to_ascii_lowercase())
                && item.relative_path == format!("Library/Videos/mp4/{}.mp4", item.id)
                && item.sha256.len() == 64
                && item.sha256.bytes().all(|b| b.is_ascii_hexdigit())
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
            "Invalid/duplicate video metadata"
        );
    }
    ensure!(
        catalog
            .selected
            .as_ref()
            .is_none_or(|id| catalog.items.iter().any(|item| &item.id == id)),
        "Remembered video ID is absent"
    );
    Ok(())
}
fn child(parent: &Path, leaf: &str) -> Result<PathBuf> {
    let _pins = document::Pins::pin(parent)?;
    let path = parent.join(leaf);
    match fs::create_dir(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.into()),
    };
    let _pin = document::Pins::pin(&path)?;
    Ok(path)
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Row {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub kind: &'static str,
    pub codecs: Vec<String>,
    pub duration_ms: u64,
}
pub(crate) struct RemoveOutcome {
    pub item: VideoItem,
    pub file_removed: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Phase {
    Checking,
    Copying,
    Verifying,
    Committing,
    Done,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Progress {
    pub phase: Phase,
    pub bytes: u64,
    pub total: u64,
}
#[derive(Clone, PartialEq, Eq)]
struct Stamp {
    volume: u64,
    id: [u8; 16],
    bytes: u64,
    modified: u64,
}
fn stamp(file: &File) -> Result<Stamp> {
    let m = file.metadata()?;
    let mut basic = BY_HANDLE_FILE_INFORMATION::default();
    let mut id = FILE_ID_INFO::default();
    unsafe {
        GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut basic)?;
        GetFileInformationByHandleEx(
            HANDLE(file.as_raw_handle()),
            FileIdInfo,
            &mut id as *mut _ as _,
            size_of::<FILE_ID_INFO>() as u32,
        )?;
    }
    ensure!(
        m.is_file()
            && m.file_attributes() & 0x400 == 0
            && basic.nNumberOfLinks == 1
            && id.FileId.Identifier.iter().any(|b| *b != 0),
        "MP4 source/storage is linked or lacks stable ownership"
    );
    Ok(Stamp {
        volume: id.VolumeSerialNumber,
        id: id.FileId.Identifier,
        bytes: m.len(),
        modified: m.last_write_time(),
    })
}
fn remove_owned(file: &File) -> Result<()> {
    let info = FILE_DISPOSITION_INFO {
        DeleteFile: true.into(),
    };
    unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileDispositionInfo,
            &info as *const _ as _,
            size_of::<FILE_DISPOSITION_INFO>() as u32,
        )?;
    }
    Ok(())
}
struct Copied {
    file: File,
    catalog_attempted: bool,
}
impl Drop for Copied {
    fn drop(&mut self) {
        if !self.catalog_attempted {
            let _ = remove_owned(&self.file);
        }
    }
}
fn uuid(nonce: &str) -> Result<String> {
    ensure!(
        nonce.len() == 48 && nonce.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid video import operation identity"
    );
    let mut text = nonce[..32].to_ascii_lowercase().into_bytes();
    text[12] = b'4';
    let digit = (text[16] as char).to_digit(16).unwrap() as u8;
    text[16] = b"0123456789abcdef"[((digit & 3) | 8) as usize];
    let s = String::from_utf8(text)?;
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &s[..8],
        &s[8..12],
        &s[12..16],
        &s[16..20],
        &s[20..]
    ))
}
fn codec_names(info: &startup_mp4::Metadata) -> Vec<String> {
    info.video_codecs
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
pub(crate) struct Store {
    root: PathBuf,
    document: document::Document,
    revision: document::Revision,
    catalog: VideoCatalog,
}
impl Store {
    pub fn open(root: &Path) -> Result<Self> {
        let _root = document::Pins::pin(root)?;
        let root = root.canonicalize()?;
        let data = child(&root, "Data")?;
        let data = child(&data, "Rust")?;
        let doc = document::Document::open(&data)?;
        doc.recover()?;
        let (bytes, revision) = doc.load()?;
        let catalog = if let Some(bytes) = bytes {
            serde_json::from_slice::<VideoCatalog>(&bytes)
                .context("Video catalog is corrupt; restore it before editing")?
        } else {
            VideoCatalog {
                schema: SCHEMA.into(),
                selected: None,
                items: vec![],
            }
        };
        validate(&catalog)?;
        Ok(Self {
            root,
            document: doc,
            revision,
            catalog,
        })
    }
    pub fn view(&self) -> &VideoCatalog {
        &self.catalog
    }
    pub fn rows(&self) -> Vec<Row> {
        self.catalog
            .items
            .iter()
            .map(|item| Row {
                id: item.id.clone(),
                name: item.name.clone(),
                path: self.root.join(&item.relative_path),
                bytes: item.bytes,
                kind: "Videos",
                codecs: item.codecs.clone(),
                duration_ms: item.duration_ms,
            })
            .collect()
    }
    /// No implicit recovery or stale-state adoption during launch/preview.
    pub fn verify_current(&self) -> Result<()> {
        let (bytes, revision) = self.document.load()?;
        ensure!(
            revision == self.revision,
            "Video catalog changed; reload before playback/import"
        );
        if let Some(bytes) = bytes {
            let value: VideoCatalog = serde_json::from_slice(&bytes)?;
            validate(&value)?;
            ensure!(
                serde_json::to_vec(&value)? == serde_json::to_vec(&self.catalog)?,
                "Video catalog differs from authoritative snapshot"
            );
        } else {
            ensure!(
                self.catalog.items.is_empty() && self.catalog.selected.is_none(),
                "Video catalog disappeared"
            );
        }
        Ok(())
    }
    pub fn resolve(&self, id: &str, cancel: &AtomicBool) -> Result<legacy::ResolvedVideo> {
        self.verify_current()?;
        ensure!(
            self.catalog.items.iter().any(|i| i.id == id),
            "Select a saved video ID, not a path"
        );
        let lease = legacy::resolve_video(&self.root, id, cancel)?;
        self.verify_current()?;
        Ok(lease)
    }
    pub fn rename(&mut self, id: &str, new_name: &str, nonce: &str) -> Result<VideoItem> {
        name(new_name)?;
        self.verify_current()?;
        let mut next = self.catalog.clone();
        let item = next
            .items
            .iter_mut()
            .find(|i| i.id == id)
            .context("Saved video ID is absent")?;
        item.name = new_name.into();
        let result = item.clone();
        validate(&next)?;
        self.revision =
            self.document
                .commit(&self.revision, &serde_json::to_vec_pretty(&next)?, nonce)?;
        self.catalog = next;
        Ok(result)
    }
    /// Removes the catalog entry first, then deletes only the launcher-owned
    /// Library/Videos/mp4 copy. Imported originals are never touched. A copy
    /// that is in use, linked or no longer matches its recorded size is kept.
    pub fn remove(&mut self, id: &str, nonce: &str) -> Result<RemoveOutcome> {
        self.verify_current()?;
        let mut next = self.catalog.clone();
        let index = next
            .items
            .iter()
            .position(|i| i.id == id)
            .context("Saved video ID is absent")?;
        let item = next.items.remove(index);
        if next.selected.as_deref() == Some(id) {
            next.selected = None;
        }
        validate(&next)?;
        self.revision =
            self.document
                .commit(&self.revision, &serde_json::to_vec_pretty(&next)?, nonce)?;
        self.catalog = next;
        let file_removed = self.remove_copy(&item).is_ok();
        Ok(RemoveOutcome { item, file_removed })
    }
    fn remove_copy(&self, item: &VideoItem) -> Result<()> {
        ensure!(
            valid_id(&item.id)
                && item.relative_path == format!("Library/Videos/mp4/{}.mp4", item.id),
            "Video copy is outside the launcher library"
        );
        let mp4 = self.root.join("Library/Videos/mp4");
        let _pins = document::Pins::pin(&mp4)?;
        let file = fs::OpenOptions::new()
            .read(true)
            .access_mode(0x00010000 | 0x80000000)
            .share_mode(0)
            .custom_flags(0x00200000)
            .open(self.root.join(&item.relative_path))?;
        ensure!(
            stamp(&file)?.bytes == item.bytes,
            "Video copy size changed; retained"
        );
        remove_owned(&file)
    }
    pub fn import_mp4(
        &mut self,
        source: &Path,
        display_name: &str,
        nonce: &str,
        cancel: &AtomicBool,
        progress: impl FnMut(Progress) -> Result<()>,
    ) -> Result<VideoItem> {
        self.import_inner(
            source,
            display_name,
            nonce,
            cancel,
            progress,
            |doc, revision, bytes, nonce| doc.commit(revision, bytes, nonce),
        )
    }
    fn import_inner(
        &mut self,
        source: &Path,
        display_name: &str,
        nonce: &str,
        cancel: &AtomicBool,
        mut progress: impl FnMut(Progress) -> Result<()>,
        commit: impl FnOnce(
            &document::Document,
            &document::Revision,
            &[u8],
            &str,
        ) -> Result<document::Revision>,
    ) -> Result<VideoItem> {
        check(cancel)?;
        name(display_name)?;
        local_source(source)?;
        self.verify_current()?;
        ensure!(self.catalog.items.len() < 1000, "Video catalog is full");
        let id = uuid(nonce)?;
        ensure!(
            !self.catalog.items.iter().any(|i| i.id == id),
            "Video ID already exists; existing media left untouched"
        );
        ensure!(
            source
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("mp4")),
            "Use an MP4 source"
        );
        let _source_pins = document::Pins::pin(source.parent().context("Source parent missing")?)?;
        let mut input = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .custom_flags(0x00200000)
            .open(source)?;
        let before = stamp(&input)?;
        ensure!(
            (1..=startup_mp4::MAX_BYTES).contains(&before.bytes),
            "MP4 exceeds2GiB bound"
        );
        progress(Progress {
            phase: Phase::Checking,
            bytes: 0,
            total: before.bytes,
        })?;
        let info = startup_mp4::inspect(&mut input, "mp4")?;
        ensure!(
            stamp(&input)? == before,
            "MP4 source changed during validation"
        );
        check(cancel)?;
        let library = child(&self.root, "Library")?;
        let videos = child(&library, "Videos")?;
        let mp4 = child(&videos, "mp4")?;
        let _destination = document::Pins::pin(&mp4)?;
        let relative = format!("Library/Videos/mp4/{id}.mp4");
        let path = self.root.join(&relative);
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .access_mode(0xc0010000)
            .create_new(true)
            .share_mode(1)
            .custom_flags(0x00200000)
            .open(&path)
            .context("Video ID/file collision; existing file left untouched")?;
        let mut copied = Copied {
            file,
            catalog_attempted: false,
        };
        let owner = stamp(&copied.file)?;
        ensure!(owner.bytes == 0, "New MP4 destination is not empty");
        input.seek(SeekFrom::Start(0))?;
        let mut hash = Sha256::new();
        let mut bytes = 0u64;
        let mut block = vec![0; 1024 * 1024];
        loop {
            check(cancel)?;
            progress(Progress {
                phase: Phase::Copying,
                bytes,
                total: before.bytes,
            })?;
            let n = input.read(&mut block)?;
            if n == 0 {
                break;
            }
            bytes += n as u64;
            ensure!(bytes <= before.bytes, "MP4 source grew during copy");
            copied.file.write_all(&block[..n])?;
            hash.update(&block[..n]);
        }
        copied.file.sync_all()?;
        ensure!(
            bytes == before.bytes && stamp(&input)? == before,
            "MP4 source changed during copy"
        );
        let sha = format!("{:x}", hash.finalize());
        copied.file.seek(SeekFrom::Start(0))?;
        let mut readback = Sha256::new();
        let mut verified = 0;
        loop {
            check(cancel)?;
            progress(Progress {
                phase: Phase::Verifying,
                bytes: verified,
                total: before.bytes,
            })?;
            let n = copied.file.read(&mut block)?;
            if n == 0 {
                break;
            }
            readback.update(&block[..n]);
            verified += n as u64;
        }
        ensure!(
            verified == before.bytes && format!("{:x}", readback.finalize()) == sha,
            "MP4 copy hash differs"
        );
        let after = stamp(&copied.file)?;
        ensure!(
            after.volume == owner.volume && after.id == owner.id && after.bytes == bytes,
            "MP4 destination ownership changed"
        );
        ensure!(
            startup_mp4::inspect(&mut copied.file, "mp4")? == info,
            "MP4 copied metadata differs"
        );
        self.verify_current()?;
        check(cancel)?;
        progress(Progress {
            phase: Phase::Committing,
            bytes,
            total: bytes,
        })?;
        let item = VideoItem {
            id,
            name: display_name.into(),
            relative_path: relative,
            sha256: sha,
            bytes,
            duration_ms: info.duration_ms,
            codecs: codec_names(&info),
            width: info.max_width,
            height: info.max_height,
            playback_probe_required: true,
        };
        let mut next = self.catalog.clone();
        next.items.push(item.clone());
        validate(&next)?;
        // Once catalog publication may occur, retain this exact verified MP4
        // even on uncertainty: a recovered new catalog must not reference a
        // deleted file. The old/source files are never rollback targets.
        check(cancel)?;
        copied.catalog_attempted = true;
        self.revision = commit(
            &self.document,
            &self.revision,
            &serde_json::to_vec_pretty(&next)?,
            nonce,
        )
        .context("Verified MP4 retained; catalog publication needs recovery before retrying")?;
        self.catalog = next;
        let _ = progress(Progress {
            phase: Phase::Done,
            bytes,
            total: bytes,
        });
        Ok(item)
    }
}
#[cfg(test)]
#[path = "video_library_tests.rs"]
mod tests;
