//! Fixed embedded, purpose-built audio guardian extraction. Cache only,
//! no-overwrite exact-handle publication; a foreign cache entry is preserved.
#![allow(dead_code)]
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    os::windows::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Component, Path, PathBuf},
};
use windows::Win32::{Foundation::HANDLE, Storage::FileSystem::*};
pub(crate) const MAX_HELPER: u64 = 16 * 1024 * 1024;
pub(crate) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(crate) fn hash(file: &mut File) -> Result<String> {
    ensure!(
        file.metadata()?.len() <= MAX_HELPER,
        "Audio guardian exceeds size bound"
    );
    file.seek(SeekFrom::Start(0))?;
    let mut sha = Sha256::new();
    let mut b = [0; 65536];
    let mut count = 0u64;
    loop {
        let n = file.read(&mut b)?;
        if n == 0 {
            break;
        }
        count += n as u64;
        ensure!(count <= MAX_HELPER, "Audio guardian grew beyond bound");
        sha.update(&b[..n]);
    }
    Ok(format!("{:x}", sha.finalize()))
}
pub(crate) struct Binary {
    path: PathBuf,
    pub sha256: String,
    _file: File,
    _pins: Vec<File>,
}
impl Binary {
    pub fn path(&self) -> &Path {
        &self.path
    }
}
fn pin(path: &Path) -> Result<File> {
    let f = fs::OpenOptions::new()
        .read(true)
        .share_mode(3)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)?;
    let m = f.metadata()?;
    ensure!(
        m.is_dir() && m.file_attributes() & 0x400 == 0,
        "Linked audio guardian cache rejected"
    );
    Ok(f)
}
fn publish(file: &File, to: &Path) -> Result<()> {
    let name = to.as_os_str().encode_wide().collect::<Vec<_>>();
    ensure!(
        !name.is_empty() && name.len() <= 32760 && !name.contains(&0),
        "Audio guardian destination invalid"
    );
    let size = std::mem::size_of::<FILE_RENAME_INFO>() + (name.len() + 1) * 2;
    let mut data = vec![0u64; size.div_ceil(8)];
    let info = data.as_mut_ptr() as *mut FILE_RENAME_INFO;
    unsafe {
        (*info).Anonymous.ReplaceIfExists = false;
        (*info).FileNameLength = (name.len() * 2) as u32;
        std::ptr::copy_nonoverlapping(name.as_ptr(), (*info).FileName.as_mut_ptr(), name.len());
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileRenameInfo,
            info as _,
            size as u32,
        )?;
    }
    Ok(())
}
fn unlink(file: &File) -> Result<()> {
    let mut info = FILE_DISPOSITION_INFO { DeleteFile: true };
    unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileDispositionInfo,
            &mut info as *mut _ as _,
            std::mem::size_of_val(&info) as u32,
        )?;
    }
    Ok(())
}
pub(crate) fn prepare(root: &Path, embedded: &[u8], expected: &str) -> Result<Binary> {
    ensure!(
        root.is_absolute()
            && !root
                .components()
                .any(|p| matches!(p, Component::CurDir | Component::ParentDir)),
        "Normalized portable root required"
    );
    ensure!(
        (64..=MAX_HELPER as usize).contains(&embedded.len())
            && embedded.starts_with(b"MZ")
            && expected.len() == 64
            && expected.bytes().all(|b| b.is_ascii_hexdigit())
            && digest(embedded).eq_ignore_ascii_case(expected),
        "Embedded audio guardian hash/type differs"
    );
    let mut pins = Vec::new();
    let mut part = PathBuf::new();
    for c in root.components() {
        part.push(c.as_os_str());
        if !matches!(c, Component::Prefix(_)) {
            pins.push(pin(&part)?);
        }
    }
    for path in [root.join("Cache"), root.join("Cache/Runtime")] {
        match fs::create_dir(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
        pins.push(pin(&path)?);
    }
    let path = root.join("Cache/Runtime").join(format!(
        "startup-audio-{}.exe",
        expected.to_ascii_lowercase()
    ));
    let open = || -> Result<File> {
        let mut f = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(&path)?;
        let m = f.metadata()?;
        ensure!(
            m.is_file()
                && m.file_attributes() & 0x400 == 0
                && m.len() == embedded.len() as u64
                && hash(&mut f)?.eq_ignore_ascii_case(expected),
            "Existing guardian cache file is not ours; preserved"
        );
        Ok(f)
    };
    if fs::symlink_metadata(&path).is_ok() {
        return Ok(Binary {
            path: path.clone(),
            sha256: expected.into(),
            _file: open()?,
            _pins: pins,
        });
    }
    let stage = root.join("Cache/Runtime").join(format!(
        ".startup-audio-{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    let mut f = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .access_mode(0x80000000 | 0x40000000 | 0x10000)
        .share_mode(1)
        .create_new(true)
        .open(&stage)?;
    let result = (|| -> Result<()> {
        f.write_all(embedded)?;
        f.sync_all()?;
        ensure!(
            hash(&mut f)?.eq_ignore_ascii_case(expected),
            "Staged guardian readback differs"
        );
        publish(&f, &path)
    })(); // Only this exact fresh handle may be cleaned on failure.
    if let Err(e) = result {
        let _ = unlink(&f);
        return Err(e).context("Preparing fixed audio guardian cache");
    }
    drop(f);
    Ok(Binary {
        path: path.clone(),
        sha256: expected.into(),
        _file: open()?,
        _pins: pins,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let base =
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/startup-helper-fixtures");
            fs::create_dir_all(&base).unwrap();
            let path = base.join(format!(
                "{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            assert!(
                self.0
                    .parent()
                    .unwrap()
                    .ends_with("startup-helper-fixtures")
            );
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn bytes() -> Vec<u8> {
        let mut b = vec![0; 256];
        b[..2].copy_from_slice(b"MZ");
        b
    }
    #[test]
    fn fixed_helper_is_hash_pinned_cached_and_cannot_be_overwritten_or_deleted_while_owned() {
        let f = Fixture::new();
        let b = bytes();
        let sha = digest(&b);
        let binary = prepare(&f.0, &b, &sha).unwrap();
        assert_eq!(fs::read(binary.path()).unwrap(), b);
        assert!(fs::write(binary.path(), b"foreign").is_err());
        assert!(fs::remove_file(binary.path()).is_err());
        let cached = prepare(&f.0, &b, &sha).unwrap();
        assert_eq!(cached.path(), binary.path());
    }
    #[test]
    fn foreign_cache_file_and_wrong_embedded_hash_are_preserved() {
        let f = Fixture::new();
        let b = bytes();
        let sha = digest(&b);
        assert!(prepare(&f.0, &b, &"b".repeat(64)).is_err());
        fs::create_dir_all(f.0.join("Cache/Runtime")).unwrap();
        let path =
            f.0.join("Cache/Runtime")
                .join(format!("startup-audio-{sha}.exe"));
        fs::write(&path, b"foreign").unwrap();
        assert!(prepare(&f.0, &b, &sha).is_err());
        assert_eq!(fs::read(path).unwrap(), b"foreign");
    }
}
