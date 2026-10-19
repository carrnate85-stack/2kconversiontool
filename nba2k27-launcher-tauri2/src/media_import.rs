//! Portable, no-overwrite media import. Source bytes are pinned and validated;
//! only a newly created staging handle can be removed on failure. No game or
//! user-library mutation occurs until the final no-replace handle rename.
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    os::windows::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Component, Path, PathBuf, Prefix},
};
use windows::Win32::{
    Foundation::HANDLE,
    Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FILE_FLAG_OPEN_REPARSE_POINT, GetFileInformationByHandle,
    },
};

const MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const BLOCK: usize = 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Phase {
    Checking,
    Copying,
    VerifyingSource,
    VerifyingCopy,
    Publishing,
    Done,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Progress {
    pub phase: Phase,
    pub bytes: u64,
    pub total: u64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VideoInfo {
    pub duration_ms: u64,
    pub codecs: Vec<String>,
    pub video_tracks: usize,
    pub audio_tracks: usize,
    pub width: u16,
    pub height: u16,
    pub playback_probe_required: bool,
}
impl From<crate::startup_mp4::Metadata> for VideoInfo {
    fn from(m: crate::startup_mp4::Metadata) -> Self {
        Self {
            duration_ms: m.duration_ms,
            codecs: m
                .video_codecs
                .iter()
                .map(|c| {
                    match c {
                        crate::startup_mp4::Codec::H264 => "H.264",
                        crate::startup_mp4::Codec::Av1 => "AV1",
                        crate::startup_mp4::Codec::Vp9 => "VP9",
                    }
                    .into()
                })
                .collect(),
            video_tracks: m.video_tracks,
            audio_tracks: m.audio_tracks,
            width: m.max_width,
            height: m.max_height,
            playback_probe_required: m.playback_probe_required,
        }
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Imported {
    pub kind: String,
    pub name: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub sha256: String,
    pub video: Option<VideoInfo>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Fingerprint {
    volume: u32,
    index: u64,
    size: u64,
    modified: u64,
    attributes: u32,
    links: u32,
}
fn fingerprint(file: &File) -> Result<Fingerprint> {
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.file_attributes() & 0x400 == 0,
        "Media must be an ordinary file, not a link or reparse point"
    );
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    unsafe {
        GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info)?;
    }
    Ok(Fingerprint {
        volume: info.dwVolumeSerialNumber,
        index: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
        size: metadata.len(),
        modified: metadata.last_write_time(),
        attributes: metadata.file_attributes(),
        links: info.nNumberOfLinks,
    })
}
fn same_source(file: &File, before: Fingerprint) -> Result<()> {
    stable_fingerprint(fingerprint(file)?, before)
}
fn stable_fingerprint(actual: Fingerprint, before: Fingerprint) -> Result<()> {
    ensure!(
        actual == before,
        "Source media changed while importing; nothing was published"
    );
    Ok(())
}
fn valid_size(bytes: u64) -> Result<()> {
    ensure!(
        (1..=MAX_BYTES).contains(&bytes),
        "Media file is empty or exceeds 2 GiB"
    );
    Ok(())
}
fn normalized(path: &Path) -> Result<()> {
    ensure!(path.is_absolute(), "Media import requires absolute paths");
    let mut parts = path.components();
    ensure!(
        matches!(parts.next(),Some(Component::Prefix(prefix)) if matches!(prefix.kind(),Prefix::Disk(_)|Prefix::VerbatimDisk(_)|Prefix::UNC(_,_)|Prefix::VerbatimUNC(_,_))),
        "Device namespace media paths are unsupported"
    );
    ensure!(
        !parts.any(|p| matches!(p, Component::ParentDir | Component::CurDir)),
        "Unnormalized media path rejected"
    );
    Ok(())
}
fn filename(source: &Path, kind: &str) -> Result<String> {
    ensure!(
        matches!(kind, "Music" | "Videos"),
        "Unsupported media library"
    );
    let name = source.file_name().context("Media filename missing")?;
    let wide = name.encode_wide().collect::<Vec<_>>();
    let text = name
        .to_str()
        .context("Media filename is not valid Unicode")?;
    ensure!(
        !wide.is_empty()
            && wide.len() <= 255
            && !text.ends_with(['.', ' '])
            && !text
                .chars()
                .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c)),
        "Unsupported media filename"
    );
    let stem = text.split('.').next().unwrap_or("").to_ascii_uppercase();
    ensure!(
        !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            && !(stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9')),
        "Reserved Windows media filename"
    );
    let extension = source
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    ensure!(
        if kind == "Videos" {
            extension == "mp4"
        } else {
            matches!(extension.as_str(), "mp3" | "wav" | "m4a" | "ogg")
        },
        "Choose an MP4 video or MP3/WAV/M4A/OGG audio file"
    );
    Ok(text.to_owned())
}
fn missing(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(e.into()),
    }
}
fn prepare(root: &Path, kind: &str) -> Result<Vec<crate::loader_files::Directories>> {
    let mut pins = vec![crate::loader_files::Directories::pin(root)?];
    for relative in [
        "Library".to_owned(),
        format!("Library/{kind}"),
        "Cache".to_owned(),
        "Cache/Imports".to_owned(),
    ] {
        let path = root.join(relative);
        match fs::create_dir(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e).context("Preparing portable media storage"),
        }
        pins.push(crate::loader_files::Directories::pin(&path)?);
    }
    Ok(pins)
}
fn digest(
    file: &mut File,
    total: u64,
    phase: Phase,
    check: &mut impl FnMut(&Progress) -> Result<()>,
) -> Result<String> {
    file.seek(SeekFrom::Start(0))?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0; BLOCK];
    let mut done = 0u64;
    loop {
        check(&Progress {
            phase,
            bytes: done,
            total,
        })?;
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        done = done
            .checked_add(n as u64)
            .context("Media byte count overflow")?;
        ensure!(
            done <= total && done <= MAX_BYTES,
            "Media grew while importing"
        );
        hash.update(&buffer[..n]);
    }
    ensure!(done == total, "Media was truncated while importing");
    Ok(format!("{:X}", hash.finalize()))
}
struct Staged {
    file: File,
    published: bool,
}
impl Staged {
    fn discard(&mut self) -> Result<()> {
        if !self.published {
            crate::loader_files::unlink(&self.file)?;
        }
        Ok(())
    }
}
impl Drop for Staged {
    fn drop(&mut self) {
        if !self.published {
            let _ = crate::loader_files::unlink(&self.file);
        }
    }
}

pub(crate) fn import(root: &Path, source: &Path, kind: &str) -> Result<Imported> {
    import_with(root, source, kind, |_| Ok(()))
}
/// Cancellation is an error from `check`, called before each bounded copy/hash
/// chunk and before publication. After successful publication, a failed final
/// progress notification cannot delete or misreport a committed library item.
pub(crate) fn import_with(
    root: &Path,
    source: &Path,
    kind: &str,
    mut check: impl FnMut(&Progress) -> Result<()>,
) -> Result<Imported> {
    import_inner(root, source, kind, &mut check, |_| Ok(()))
}
fn import_inner(
    root: &Path,
    source: &Path,
    kind: &str,
    check: &mut impl FnMut(&Progress) -> Result<()>,
    mut after_copy: impl FnMut(&mut File) -> Result<()>,
) -> Result<Imported> {
    normalized(root)?;
    normalized(source)?;
    let name = filename(source, kind)?;
    let _root_pin = crate::loader_files::Directories::pin(root)?;
    let _source_pins = crate::loader_files::Directories::pin(
        source.parent().context("Media source directory missing")?,
    )?;
    check(&Progress {
        phase: Phase::Checking,
        bytes: 0,
        total: 0,
    })?;
    let mut input = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(source)
        .with_context(|| format!("Pinning source media {}", source.display()))?;
    let before = fingerprint(&input)?;
    valid_size(before.size)?;
    let video = if kind == "Videos" {
        Some(crate::startup_mp4::inspect(&mut input, "mp4")?)
    } else {
        None
    };
    same_source(&input, before)?;
    let _storage_pins = prepare(root, kind)?;
    let target = root.join("Library").join(kind).join(&name);
    // Explicit verbatim destination avoids relying on the host executable's
    // longPathAware manifest. The still-pinned parent cannot be redirected.
    let native_target = target
        .parent()
        .context("Media destination directory missing")?
        .canonicalize()?
        .join(&name);
    ensure!(
        missing(&target)?,
        "This media filename is already imported; existing file left untouched"
    );
    check(&Progress {
        phase: Phase::Copying,
        bytes: 0,
        total: before.size,
    })?;
    let stage_path = root.join("Cache/Imports").join(format!(
        "media-{}-{}.importing",
        std::process::id(),
        crate::backend::unique_id()
    ));
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .access_mode(0xC0000000 | 0x10000)
        .share_mode(1)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .create_new(true)
        .open(&stage_path)
        .context("Creating owned media staging file")?;
    let mut stage = Staged {
        file,
        published: false,
    };
    let result = (|| -> Result<Imported> {
        let stage_owner = fingerprint(&stage.file)?;
        ensure!(
            stage_owner.links == 1 && stage_owner.size == 0,
            "New media staging file has unexpected ownership"
        );
        input.seek(SeekFrom::Start(0))?;
        let mut hash = Sha256::new();
        let mut buffer = vec![0; BLOCK];
        let mut copied = 0u64;
        loop {
            check(&Progress {
                phase: Phase::Copying,
                bytes: copied,
                total: before.size,
            })?;
            let n = input.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            copied = copied
                .checked_add(n as u64)
                .context("Media copy length overflow")?;
            ensure!(
                copied <= before.size && copied <= MAX_BYTES,
                "Source media grew while copying"
            );
            stage.file.write_all(&buffer[..n])?;
            hash.update(&buffer[..n]);
        }
        ensure!(
            copied == before.size,
            "Source media was truncated while copying"
        );
        stage.file.sync_all()?;
        let expected_hash = format!("{:X}", hash.finalize());
        same_source(&input, before)?;
        ensure!(
            digest(&mut input, before.size, Phase::VerifyingSource, check)? == expected_hash,
            "Source media fingerprint changed during copy"
        );
        same_source(&input, before)?;
        // A test-only seam below is a no-op in the public service; it simulates
        // staged corruption without touching a user file or weakening sharing.
        after_copy(&mut stage.file)?;
        ensure!(
            digest(&mut stage.file, before.size, Phase::VerifyingCopy, check)? == expected_hash,
            "Staged media fingerprint differs; nothing was published"
        );
        let after = fingerprint(&stage.file)?;
        ensure!(
            after.volume == stage_owner.volume
                && after.index == stage_owner.index
                && after.links == 1
                && after.size == before.size,
            "Media staging ownership changed"
        );
        if let Some(expected) = &video {
            ensure!(
                crate::startup_mp4::inspect(&mut stage.file, "mp4")? == *expected,
                "Completed MP4 copy validation differs from source"
            );
        }
        same_source(&input, before)?;
        check(&Progress {
            phase: Phase::Publishing,
            bytes: before.size,
            total: before.size,
        })?;
        ensure!(
            missing(&target)?,
            "Another import published this media filename; existing file left untouched"
        );
        crate::loader_files::publish(&stage.file, &native_target)
            .context("Publishing verified media without overwriting an existing item")?;
        stage.published = true;
        let imported = Imported {
            kind: kind.into(),
            name: name.clone(),
            path: target,
            bytes: before.size,
            sha256: expected_hash,
            video: video.clone().map(Into::into),
        };
        let _ = check(&Progress {
            phase: Phase::Done,
            bytes: before.size,
            total: before.size,
        });
        Ok(imported)
    })();
    if let Err(error) = result {
        if let Err(cleanup) = stage.discard() {
            return Err(error.context(format!(
                "Owned incomplete stage retained at {} because cleanup failed: {cleanup:#}",
                stage_path.display()
            )));
        }
        return Err(error);
    }
    result
}

#[cfg(test)]
#[path = "media_import_tests.rs"]
mod tests;
