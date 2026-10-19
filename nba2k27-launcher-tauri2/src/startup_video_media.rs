//! Trusted catalog ID -> retained immutable ordinary MP4. Not an IPC request
//! type; renderer strings never authorize filesystem paths. Both the file and
//! every ancestor stay pinned until the owned intro has closed.
#![allow(dead_code)]
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Component, Path, PathBuf},
};
use windows::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
};
pub(crate) struct CatalogEntry {
    pub id: String,
    pub name: String,
    pub relative_path: PathBuf,
    pub sha256: String,
    pub bytes: u64,
}
pub(crate) struct VideoLease {
    pub id: String,
    pub name: String,
    path: PathBuf,
    _file: File,
    _parents: Vec<File>,
    pub duration_ms: u64,
}
fn validate_entry(entry: &CatalogEntry) -> Result<()> {
    ensure!(
        !entry.id.is_empty()
            && entry.id.len() <= 128
            && entry
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')),
        "Invalid trusted startup-video ID"
    );
    ensure!(
        !entry.name.is_empty()
            && entry.name.len() <= 1024
            && !entry.name.chars().any(char::is_control),
        "Invalid startup-video title"
    );
    ensure!(
        entry.sha256.len() == 64
            && entry.sha256.bytes().all(|b| b.is_ascii_hexdigit())
            && (24..=crate::startup_mp4::MAX_BYTES).contains(&entry.bytes),
        "Invalid startup-video catalog fingerprint"
    );
    let pieces = entry.relative_path.components().collect::<Vec<_>>();
    ensure!(
        (3..=8).contains(&pieces.len()) && pieces.iter().all(|p| matches!(p, Component::Normal(_))),
        "Startup-video catalog path is not a bounded relative path"
    );
    ensure!(
        pieces[0].as_os_str().eq_ignore_ascii_case("Library")
            && pieces[1].as_os_str().eq_ignore_ascii_case("Videos"),
        "Startup video is outside the portable video library"
    );
    for piece in pieces {
        let name = piece
            .as_os_str()
            .to_str()
            .context("Non-Unicode video catalog path")?;
        ensure!(
            !name.is_empty()
                && !name.ends_with([' ', '.'])
                && !name.chars().any(|c| c.is_control()
                    || matches!(c, ':' | '/' | '\\' | '"' | '<' | '>' | '|' | '?' | '*')),
            "Unsupported startup-video path component"
        );
    }
    ensure!(
        entry
            .relative_path
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("mp4")),
        "Startup intros must be MP4, not game MOV replacements"
    );
    Ok(())
}
impl VideoLease {
    pub fn from_trusted_catalog(
        root: &Path,
        entry: CatalogEntry,
        cancelled: impl Fn() -> bool,
    ) -> Result<Self> {
        validate_entry(&entry)?;
        ensure!(
            root.is_absolute()
                && !root
                    .components()
                    .any(|c| matches!(c, Component::CurDir | Component::ParentDir)),
            "Normalized absolute portable root required"
        );
        let path = root.join(&entry.relative_path);
        let mut parents = Vec::new();
        let mut current = PathBuf::new();
        for c in path
            .parent()
            .context("Video library parent missing")?
            .components()
        {
            ensure!(!cancelled(), "Startup-video validation cancelled");
            current.push(c.as_os_str());
            if matches!(c, Component::Prefix(_)) {
                continue;
            }
            let file = fs::OpenOptions::new()
                .read(true)
                .share_mode(3)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
                .open(&current)?;
            let m = file.metadata()?;
            ensure!(
                m.is_dir() && m.file_attributes() & 0x400 == 0,
                "Linked startup-video ancestry rejected"
            );
            parents.push(file);
        }
        let mut file = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(&path)?;
        let before = file.metadata()?;
        ensure!(
            before.is_file()
                && before.file_attributes() & 0x400 == 0
                && before.len() == entry.bytes,
            "Startup MP4 no longer matches its catalog"
        );
        let mut sha = Sha256::new();
        let mut total = 0u64;
        let mut bytes = [0; 256 * 1024];
        loop {
            ensure!(!cancelled(), "Startup-video validation cancelled");
            let n = file.read(&mut bytes)?;
            if n == 0 {
                break;
            }
            total += n as u64;
            ensure!(
                total <= entry.bytes,
                "Startup MP4 changed during validation"
            );
            sha.update(&bytes[..n]);
        }
        ensure!(
            total == entry.bytes
                && format!("{:x}", sha.finalize()).eq_ignore_ascii_case(&entry.sha256),
            "Startup MP4 hash differs; nothing played"
        );
        file.seek(SeekFrom::Start(0))?;
        let info = crate::startup_mp4::inspect(&mut file, "mp4")?;
        let after = file.metadata()?;
        ensure!(
            before.len() == after.len()
                && before.last_write_time() == after.last_write_time()
                && !cancelled(),
            "Startup MP4 changed or was cancelled"
        );
        Ok(Self {
            id: entry.id,
            name: entry.name,
            path,
            _file: file,
            _parents: parents,
            duration_ms: info.duration_ms,
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn entry(path: &str) -> CatalogEntry {
        CatalogEntry {
            id: "123e4567-e89b-12d3-a456-426614174000".into(),
            name: "NBA 2K12".into(),
            relative_path: path.into(),
            sha256: "a".repeat(64),
            bytes: 100,
        }
    }
    #[test]
    fn trusted_catalog_allows_nested_mp4_and_rejects_escape_ads_mov_or_arbitrary_absolute_paths() {
        validate_entry(&entry("Library/Videos/mp4/intro.mp4")).unwrap();
        for path in [
            "C:/anything.mp4",
            "Library/Videos/../intro.mp4",
            "Library/Music/intro.mp4",
            "Library/Videos/intro.mp4:stream",
            "Library/Videos/intro.mov",
            "Library/Videos/trailing./intro.mp4",
        ] {
            assert!(validate_entry(&entry(path)).is_err(), "{path}");
        }
    }
    fn bx(kind: &[u8; 4], p: Vec<u8>) -> Vec<u8> {
        [
            ((p.len() + 8) as u32).to_be_bytes().to_vec(),
            kind.to_vec(),
            p,
        ]
        .concat()
    }
    fn mp4(codec: &[u8; 4]) -> Vec<u8> {
        let mut sample = vec![0; 78];
        sample[24..26].copy_from_slice(&16u16.to_be_bytes());
        sample[26..28].copy_from_slice(&16u16.to_be_bytes());
        sample.extend(bx(
            if codec == b"av01" { b"av1C" } else { b"avcC" },
            vec![1; 8],
        ));
        let mut stsd = vec![0; 4];
        stsd.extend(1u32.to_be_bytes());
        stsd.extend(bx(codec, sample));
        let mut hdlr = vec![0; 12];
        hdlr[8..].copy_from_slice(b"vide");
        let track = bx(
            b"trak",
            bx(
                b"mdia",
                [
                    bx(b"hdlr", hdlr),
                    bx(b"minf", bx(b"stbl", bx(b"stsd", stsd))),
                ]
                .concat(),
            ),
        );
        let mut mvhd = vec![0; 20];
        mvhd[12..16].copy_from_slice(&1000u32.to_be_bytes());
        mvhd[16..20].copy_from_slice(&1000u32.to_be_bytes());
        [
            bx(
                b"ftyp",
                [b"isom".to_vec(), vec![0; 4], b"mp42".to_vec()].concat(),
            ),
            bx(b"moov", [bx(b"mvhd", mvhd), track].concat()),
            bx(b"mdat", vec![1; 64]),
        ]
        .concat()
    }
    #[test]
    fn resolved_h264_and_av1_sources_remain_immutable_and_hash_or_cancel_failure_never_plays() {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/startup-media-fixtures");
        fs::create_dir_all(&base).unwrap();
        let root = base.join(format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("Library/Videos/mp4")).unwrap();
        for codec in [b"avc1", b"av01"] {
            let path = root.join("Library/Videos/mp4/intro.mp4");
            let bytes = mp4(codec);
            fs::write(&path, &bytes).unwrap();
            let catalog = || {
                let mut e = entry("Library/Videos/mp4/intro.mp4");
                e.bytes = bytes.len() as u64;
                e.sha256 = format!("{:x}", Sha256::digest(&bytes));
                e
            };
            let owned = VideoLease::from_trusted_catalog(&root, catalog(), || false).unwrap();
            assert_eq!(owned.path(), path);
            assert_eq!(owned.duration_ms, 1000);
            assert!(fs::write(&path, b"foreign").is_err());
            assert!(fs::remove_file(&path).is_err());
            drop(owned);
            let mut changed = catalog();
            changed.sha256 = "b".repeat(64);
            assert!(VideoLease::from_trusted_catalog(&root, changed, || false).is_err());
            assert!(VideoLease::from_trusted_catalog(&root, catalog(), || true).is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        assert_eq!(root.parent(), Some(base.as_path()));
        fs::remove_dir_all(root).unwrap();
    }
}
