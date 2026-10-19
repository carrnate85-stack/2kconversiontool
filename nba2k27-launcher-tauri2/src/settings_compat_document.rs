//! Private settings compare-and-swap document, adapted from the reviewed local
//! Jukebox transaction. All files are confined to the caller-pinned Data/Rust.
//! Existing/foreign replacements are never overwritten; interrupted commits
//! recover using exact stable file IDs and bytes, not creation timestamps.
#![allow(dead_code)]

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    mem::size_of,
    os::windows::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Component, Path, PathBuf},
};
use windows::Win32::{Foundation::HANDLE, Storage::FileSystem::*};
const MAX_DOC: usize = 64 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Id {
    volume: u64,
    index: [u8; 16],
}
fn id(f: &File) -> Result<Id> {
    let mut i = FILE_ID_INFO::default();
    unsafe {
        GetFileInformationByHandleEx(
            HANDLE(f.as_raw_handle()),
            FileIdInfo,
            &mut i as *mut _ as _,
            size_of::<FILE_ID_INFO>() as u32,
        )?;
    }
    ensure!(
        i.FileId.Identifier.iter().any(|b| *b != 0),
        "Filesystem did not provide a stable settings identity"
    );
    Ok(Id {
        volume: i.VolumeSerialNumber,
        index: i.FileId.Identifier,
    })
}
fn ordinary(f: &File, dir: bool) -> Result<()> {
    let m = f.metadata()?;
    let mut i = BY_HANDLE_FILE_INFORMATION::default();
    unsafe {
        GetFileInformationByHandle(HANDLE(f.as_raw_handle()), &mut i)?;
    }
    ensure!(
        m.file_attributes() & 0x400 == 0
            && if dir {
                m.is_dir()
            } else {
                m.is_file() && i.nNumberOfLinks == 1
            },
        "Rust settings local storage is linked/nonordinary"
    );
    Ok(())
}
pub(super) struct Pins(Vec<File>);
impl Pins {
    pub(super) fn pin(path: &Path) -> Result<Self> {
        ensure!(
            path.is_absolute(),
            "Absolute Rust settings local path required"
        );
        let (mut p, mut v) = (PathBuf::new(), Vec::new());
        for c in path.components() {
            ensure!(
                !matches!(c, Component::ParentDir | Component::CurDir),
                "Unnormalized Rust settings path"
            );
            p.push(c.as_os_str());
            if matches!(c, Component::Prefix(_)) {
                continue;
            }
            let f = fs::OpenOptions::new()
                .access_mode(FILE_LIST_DIRECTORY.0 | FILE_READ_ATTRIBUTES.0)
                .share_mode(3)
                .custom_flags(0x02000000 | 0x00200000)
                .open(&p)?;
            ordinary(&f, true)?;
            v.push(f);
        }
        Ok(Self(v))
    }
}
fn absent(p: &Path) -> Result<bool> {
    match fs::symlink_metadata(p) {
        Ok(_) => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(e.into()),
    }
}
fn open(p: &Path, delete: bool) -> Result<File> {
    let f = fs::OpenOptions::new()
        .read(true)
        .access_mode(0x80000000 | if delete { 0x10000 } else { 0 })
        .share_mode(1)
        .custom_flags(0x00200000)
        .open(p)?;
    ordinary(&f, false)?;
    Ok(f)
}
fn fresh(p: &Path) -> Result<File> {
    let f = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .access_mode(0xc0010000)
        .share_mode(1)
        .create_new(true)
        .custom_flags(0x00200000)
        .open(p)?;
    ordinary(&f, false)?;
    Ok(f)
}
fn read(f: &mut File, max: usize) -> Result<Vec<u8>> {
    ordinary(f, false)?;
    let before = f.metadata()?;
    let identity = id(f)?;
    ensure!(
        before.len() <= max as u64,
        "Rust settings local file exceeds bounds"
    );
    f.seek(SeekFrom::Start(0))?;
    let mut b = Vec::with_capacity(before.len() as usize);
    f.take(max as u64 + 1).read_to_end(&mut b)?;
    let after = f.metadata()?;
    ensure!(
        b.len() as u64 == before.len()
            && before.len() == after.len()
            && before.last_write_time() == after.last_write_time()
            && id(f)? == identity,
        "Rust settings local file changed during read"
    );
    Ok(b)
}
fn remove(f: &File) -> Result<()> {
    let i = FILE_DISPOSITION_INFO {
        DeleteFile: true.into(),
    };
    unsafe {
        SetFileInformationByHandle(
            HANDLE(f.as_raw_handle()),
            FileDispositionInfo,
            &i as *const _ as _,
            size_of::<FILE_DISPOSITION_INFO>() as u32,
        )?;
    }
    Ok(())
}
fn rename(f: &File, to: &Path) -> Result<()> {
    let name = to.as_os_str().encode_wide().collect::<Vec<_>>();
    ensure!(
        to.is_absolute() && !name.is_empty() && name.len() <= 32760 && !name.contains(&0),
        "Invalid local Rust settings destination"
    );
    let length = size_of::<FILE_RENAME_INFO>() + (name.len() + 1) * 2;
    let mut buffer = vec![0u64; length.div_ceil(8)];
    let i = buffer.as_mut_ptr() as *mut FILE_RENAME_INFO;
    unsafe {
        (*i).Anonymous.ReplaceIfExists = false;
        (*i).FileNameLength = (name.len() * 2) as u32;
        std::ptr::copy_nonoverlapping(name.as_ptr(), (*i).FileName.as_mut_ptr(), name.len());
        SetFileInformationByHandle(
            HANDLE(f.as_raw_handle()),
            FileRenameInfo,
            buffer.as_mut_ptr() as _,
            length as u32,
        )?;
    }
    Ok(())
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Owned {
    id: Id,
    size: usize,
    sha256: String,
}
impl Owned {
    fn capture(f: &mut File, max: usize) -> Result<Self> {
        let b = read(f, max)?;
        Ok(Self {
            id: id(f)?,
            size: b.len(),
            sha256: digest(&b),
        })
    }
    fn check(&self, f: &mut File, max: usize) -> Result<()> {
        ensure!(
            id(f)? == self.id && Self::capture(f, max)? == *self,
            "Rust settings local file changed/foreign; left untouched"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Revision(Option<Owned>);
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Transaction {
    schema: String,
    nonce: String,
    directory: Id,
    original: Option<Owned>,
    next: Owned,
}
pub(crate) struct Document {
    root: PathBuf,
    identity: Id,
    _pins: Pins,
}
impl Document {
    pub fn open(root: &Path) -> Result<Self> {
        let pins = Pins::pin(root)?;
        let identity = id(pins.0.last().context("Empty local Rust settings path")?)?;
        Ok(Self {
            root: root.canonicalize()?,
            identity,
            _pins: pins,
        })
    }
    fn dirs(&self) -> Result<()> {
        let pins = Pins::pin(&self.root)?;
        ensure!(
            id(pins.0.last().unwrap())? == self.identity,
            "Rust settings catalog directory changed"
        );
        Ok(())
    }
    fn target(&self) -> PathBuf {
        self.root.join("settings.json")
    }
    fn marker(&self) -> PathBuf {
        self.root.join("settings-update.json")
    }
    fn stage(&self, n: &str) -> PathBuf {
        self.root.join(format!(".2k-rust-settings-{n}.next"))
    }
    fn backup(&self, n: &str) -> PathBuf {
        self.root.join(format!(".2k-rust-settings-{n}.previous"))
    }
    pub fn load(&self) -> Result<(Option<Vec<u8>>, Revision)> {
        self.dirs()?;
        ensure!(
            absent(&self.marker())?,
            "Rust settings local transaction needs recovery"
        );
        if absent(&self.target())? {
            return Ok((None, Revision(None)));
        }
        let mut f = open(&self.target(), false)?;
        let data = read(&mut f, MAX_DOC)?;
        let revision = Owned::capture(&mut f, MAX_DOC)?;
        Ok((Some(data), Revision(Some(revision))))
    }
    /// Settings updates are compare-and-swap. Two open stores cannot overwrite
    /// one another's newer preferences or a same-byte foreign replacement.
    pub fn commit(&self, expected: &Revision, data: &[u8], nonce: &str) -> Result<Revision> {
        self.commit_inner(expected, data, nonce, |_| Ok(()))
    }
    fn commit_inner(
        &self,
        expected: &Revision,
        data: &[u8],
        nonce: &str,
        fault: impl Fn(u8) -> Result<()>,
    ) -> Result<Revision> {
        self.dirs()?;
        ensure!(
            hex(nonce, 48) && !data.is_empty() && data.len() <= MAX_DOC,
            "Invalid Rust settings catalog update"
        );
        serde_json::from_slice::<serde_json::Value>(data)?;
        ensure!(
            absent(&self.marker())? && absent(&self.stage(nonce))? && absent(&self.backup(nonce))?,
            "Rust settings local transaction is already pending"
        );
        let original = if let Some(owned) = &expected.0 {
            let mut f = open(&self.target(), true)?;
            owned.check(&mut f, MAX_DOC)?;
            Some(f)
        } else {
            ensure!(
                absent(&self.target())?,
                "Rust settings catalog appeared; refresh before saving"
            );
            None
        };
        let mut stage = fresh(&self.stage(nonce))?;
        stage.write_all(data)?;
        stage.sync_all()?;
        let next = Owned::capture(&mut stage, MAX_DOC)?;
        let transaction = Transaction {
            schema: "nba2k27-rust-settings-library-transaction-v1".into(),
            nonce: nonce.into(),
            directory: self.identity.clone(),
            original: expected.0.clone(),
            next: next.clone(),
        };
        let mut receipt = fresh(&self.marker())?;
        receipt.write_all(&serde_json::to_vec(&transaction)?)?;
        receipt.sync_all()?;
        fault(1)?;
        self.dirs()?;
        if let Some(original) = &original {
            rename(original, &self.backup(nonce))?;
        }
        fault(2)?;
        rename(&stage, &self.target())?;
        fault(3)?;
        next.check(&mut stage, MAX_DOC)?;
        if let Some(original) = &original {
            remove(original)?;
        }
        drop(original);
        fault(4)?;
        remove(&receipt)?;
        Ok(Revision(Some(next)))
    }
    #[cfg(any(test, settings_compat_offline))]
    pub(super) fn test_commit(
        &self,
        expected: &Revision,
        data: &[u8],
        nonce: &str,
        fault: impl Fn(u8) -> Result<()>,
    ) -> Result<Revision> {
        self.commit_inner(expected, data, nonce, fault)
    }
    /// If the new catalog reached its exact destination, finish commit.
    /// Otherwise roll back to the retained exact original. Foreign paths hold
    /// the entire transaction before any removal/rename.
    pub fn recover(&self) -> Result<()> {
        self.dirs()?;
        if absent(&self.marker())? {
            return Ok(());
        }
        let mut receipt = open(&self.marker(), true)?;
        let raw = read(&mut receipt, 64 * 1024)?;
        let t: Transaction = serde_json::from_slice(&raw)?;
        ensure!(
            t.schema == "nba2k27-rust-settings-library-transaction-v1"
                && hex(&t.nonce, 48)
                && t.directory == self.identity
                && t.next.size <= MAX_DOC
                && hex(&t.next.sha256, 64),
            "Invalid Rust settings local recovery receipt"
        );
        let backup = if absent(&self.backup(&t.nonce))? {
            None
        } else {
            let mut f = open(&self.backup(&t.nonce), true)?;
            t.original
                .as_ref()
                .context("Unexpected Rust settings catalog backup")?
                .check(&mut f, MAX_DOC)?;
            Some(f)
        };
        let staged = if absent(&self.stage(&t.nonce))? {
            None
        } else {
            let mut f = open(&self.stage(&t.nonce), true)?;
            t.next.check(&mut f, MAX_DOC)?;
            Some(f)
        };
        let (mut committed, mut old_current, mut current_new) = (false, None, None);
        if !absent(&self.target())? {
            let mut f = open(&self.target(), true)?;
            if id(&f)? == t.next.id {
                t.next.check(&mut f, MAX_DOC)?;
                committed = true;
                current_new = Some(f);
            } else {
                t.original
                    .as_ref()
                    .context("Foreign Rust settings catalog appeared")?
                    .check(&mut f, MAX_DOC)?;
                old_current = Some(f);
            }
        }
        ensure!(
            !(committed && staged.is_some()) && !(old_current.is_some() && backup.is_some()),
            "Ambiguous Rust settings catalog recovery state"
        );
        if committed {
            if let Some(f) = backup {
                remove(&f)?;
            }
        } else {
            ensure!(
                t.original.is_none() || old_current.is_some() || backup.is_some(),
                "Original Rust settings catalog is missing"
            );
            if let Some(f) = staged {
                remove(&f)?;
                drop(f);
            }
            if let Some(f) = backup {
                ensure!(
                    absent(&self.target())?,
                    "Foreign Rust settings catalog appeared"
                );
                rename(&f, &self.target())?;
            }
        }
        self.dirs()?;
        remove(&receipt)?;
        drop(current_new);
        Ok(())
    }
}

fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:X}", Sha256::digest(bytes))
}
fn hex(text: &str, length: usize) -> bool {
    text.len() == length && text.bytes().all(|b| b.is_ascii_hexdigit())
}
pub(super) fn bounded_read(path: &Path, max: usize) -> Result<Option<Vec<u8>>> {
    if absent(path)? {
        return Ok(None);
    }
    let mut file = open(path, false)?;
    Ok(Some(read(&mut file, max)?))
}
