//! Private Windows filesystem capabilities for one-time saved-media promotion.
//! No recursive deletion, replace-existing rename, arbitrary executable or game API.
#![allow(dead_code)]
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    mem::size_of,
    os::windows::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Component, Path, PathBuf, Prefix},
};
use windows::Win32::{Foundation::HANDLE, Storage::FileSystem::*};
pub(super) fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Id {
    pub volume: u64,
    pub index: [u8; 16],
}
pub(super) fn id(file: &File) -> Result<Id> {
    let mut v = FILE_ID_INFO::default();
    unsafe {
        GetFileInformationByHandleEx(
            HANDLE(file.as_raw_handle()),
            FileIdInfo,
            &mut v as *mut _ as _,
            size_of::<FILE_ID_INFO>() as u32,
        )?;
    }
    ensure!(
        v.FileId.Identifier.iter().any(|b| *b != 0),
        "Filesystem identity unavailable"
    );
    Ok(Id {
        volume: v.VolumeSerialNumber,
        index: v.FileId.Identifier,
    })
}
fn ordinary(file: &File, dir: bool) -> Result<()> {
    let m = file.metadata()?;
    let mut i = BY_HANDLE_FILE_INFORMATION::default();
    unsafe {
        GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut i)?;
    }
    ensure!(
        m.file_attributes() & 0x400 == 0
            && if dir {
                m.is_dir()
            } else {
                m.is_file() && i.nNumberOfLinks == 1
            },
        "Publication path is linked/nonordinary"
    );
    Ok(())
}
pub(super) struct Pins(pub Vec<File>);
impl Pins {
    pub fn open(path: &Path) -> Result<Self> {
        ensure!(path.is_absolute(), "Absolute publication root required");
        let mut current = PathBuf::new();
        let mut files = vec![];
        for c in path.components() {
            ensure!(
                !matches!(c, Component::ParentDir | Component::CurDir),
                "Unnormalized publication path"
            );
            if let Component::Prefix(p) = c {
                ensure!(
                    matches!(p.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)),
                    "Only local drive storage supported"
                );
            }
            current.push(c.as_os_str());
            if matches!(c, Component::Prefix(_)) {
                continue;
            }
            let f = fs::OpenOptions::new()
                .access_mode(FILE_LIST_DIRECTORY.0 | FILE_READ_ATTRIBUTES.0)
                .share_mode(3)
                .custom_flags(0x02200000)
                .open(&current)?;
            ordinary(&f, true)?;
            files.push(f);
        }
        ensure!(!files.is_empty(), "Empty publication root");
        Ok(Self(files))
    }
    pub fn identity(&self) -> Result<Id> {
        id(self.0.last().context("Missing root handle")?)
    }
}
pub(super) fn missing(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(e.into()),
    }
}
pub(super) fn open_node(path: &Path, dir: bool, delete: bool) -> Result<File> {
    let f = fs::OpenOptions::new()
        .access_mode(0x80000000 | if delete { 0x10000 } else { 0 })
        .share_mode(3)
        .custom_flags(0x00200000 | if dir { 0x02000000 } else { 0 })
        .open(path)?;
    ordinary(&f, dir)?;
    Ok(f)
}
pub(super) fn fresh(path: &Path) -> Result<File> {
    let f = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .access_mode(0xc0010000)
        .share_mode(1)
        .custom_flags(0x00200000)
        .create_new(true)
        .open(path)?;
    ordinary(&f, false)?;
    Ok(f)
}
pub(super) fn journal_open(path: &Path, write: bool) -> Result<File> {
    let f = fs::OpenOptions::new()
        .access_mode(if write { 0xc0010000 } else { 0x80000000 })
        .share_mode(1)
        .custom_flags(0x00200000)
        .open(path)?;
    ordinary(&f, false)?;
    Ok(f)
}
pub(super) fn rename(file: &File, to: &Path) -> Result<()> {
    ensure!(
        to.is_absolute(),
        "Absolute owned rename destination required"
    );
    let name = to.as_os_str().encode_wide().collect::<Vec<_>>();
    ensure!(
        !name.is_empty() && name.len() <= 32760 && !name.contains(&0),
        "Rename path exceeds bounds"
    );
    let length = size_of::<FILE_RENAME_INFO>() + (name.len() + 1) * 2;
    let mut buffer = vec![0u64; length.div_ceil(8)];
    let p = buffer.as_mut_ptr() as *mut FILE_RENAME_INFO;
    unsafe {
        (*p).Anonymous.ReplaceIfExists = false;
        (*p).FileNameLength = (name.len() * 2) as u32;
        std::ptr::copy_nonoverlapping(name.as_ptr(), (*p).FileName.as_mut_ptr(), name.len());
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileRenameInfo,
            buffer.as_mut_ptr() as _,
            length as u32,
        )?;
    }
    Ok(())
}
pub(super) fn relative(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty()
            && path.len() <= 512
            && !path.contains(['\\', ':'])
            && path.is_ascii()
            && path.split('/').all(|p| !p.is_empty()
                && p != "."
                && p != ".."
                && !p.ends_with(['.', ' '])
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_. ".contains(&b))),
        "Invalid fixed publication relative path"
    );
    Ok(())
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Owned {
    pub id: Id,
    pub bytes: u64,
    pub sha256: String,
}
pub(super) struct Source {
    pub file: File,
    pub owned: Owned,
    _pins: Pins,
}
impl Source {
    pub fn open(path: &Path) -> Result<Self> {
        let pins = Pins::open(path.parent().context("Missing source parent")?)?;
        let mut file = fs::OpenOptions::new()
            .access_mode(0x80000000)
            .share_mode(1)
            .custom_flags(0x00200000)
            .open(path)?;
        ordinary(&file, false)?;
        let owned = capture(&mut file)?;
        Ok(Self {
            file,
            owned,
            _pins: pins,
        })
    }
    pub fn bytes(&mut self, max: u64) -> Result<Vec<u8>> {
        ensure!(self.owned.bytes <= max, "Publication JSON exceeds bounds");
        self.file.seek(SeekFrom::Start(0))?;
        let mut b = Vec::new();
        Read::by_ref(&mut self.file)
            .take(max + 1)
            .read_to_end(&mut b)?;
        ensure!(
            b.len() as u64 == self.owned.bytes
                && hash(&b) == self.owned.sha256
                && id(&self.file)? == self.owned.id,
            "Pinned publication source changed"
        );
        Ok(b)
    }
    pub fn copy_new(&mut self, to: &Path, guard: &impl Fn() -> Result<()>) -> Result<Owned> {
        let _destination = Pins::open(to.parent().context("Missing copy parent")?)
            .context("Pinning ordinary new publication copy destination parent")?;
        guard().context("Closed-process guard before creating a new media copy")?;
        self.file.seek(SeekFrom::Start(0)).context("Seeking retained reviewed media source")?;
        let mut out = fresh(to).context("Creating new media copy without overwrite")?;
        let mut copied = 0u64;
        let mut h = Sha256::new();
        let mut block = vec![0; 1024 * 1024];
        loop {
            guard().context("Closed-process guard between media copy chunks")?;
            let n = self.file.read(&mut block).context("Reading retained reviewed media source")?;
            if n == 0 {
                break;
            }
            ensure!(copied + n as u64 <= self.owned.bytes, "Source grew");
            out.write_all(&block[..n]).context("Writing new owned media copy")?;
            copied += n as u64;
            h.update(&block[..n]);
        }
        out.sync_all().context("Flushing new owned media copy")?;
        ensure!(
            copied == self.owned.bytes
                && format!("{:x}", h.finalize()) == self.owned.sha256
                && id(&self.file)? == self.owned.id,
            "Partial/changed source copy"
        );
        let owned = capture(&mut out).context("Verifying new owned media copy identity and readback SHA")?;
        ensure!(
            owned.bytes == self.owned.bytes && owned.sha256 == self.owned.sha256,
            "Copy readback differs"
        );
        Ok(owned)
    }
}
pub(super) fn capture(file: &mut File) -> Result<Owned> {
    ordinary(file, false)?;
    let before = file.metadata()?;
    let identity = id(file)?;
    ensure!(
        before.len() <= 4 * 1024 * 1024 * 1024,
        "Publication file exceeds bounds"
    );
    file.seek(SeekFrom::Start(0))?;
    let mut h = Sha256::new();
    let mut count = 0u64;
    let mut block = vec![0; 1024 * 1024];
    loop {
        let n = file.read(&mut block)?;
        if n == 0 {
            break;
        }
        count += n as u64;
        ensure!(count <= before.len(), "Source grew during hash");
        h.update(&block[..n]);
    }
    let after = file.metadata()?;
    ensure!(
        count == before.len()
            && after.len() == before.len()
            && before.last_write_time() == after.last_write_time()
            && id(file)? == identity,
        "Publication file changed during read"
    );
    Ok(Owned {
        id: identity,
        bytes: count,
        sha256: format!("{:x}", h.finalize()),
    })
}
pub(super) struct Tree {
    pub files: BTreeMap<String, Source>,
    pub directories: BTreeMap<String, Id>,
    pub identity: Id,
    _pins: Vec<Pins>,
}
impl Tree {
    pub fn open(root: &Path) -> Result<Self> {
        let root_pins = Pins::open(root)?;
        let identity = root_pins.identity()?;
        let mut pins = vec![root_pins];
        let mut files = BTreeMap::new();
        let mut directories = BTreeMap::new();
        let mut lower = BTreeSet::new();
        let mut pending = vec![root.to_path_buf()];
        let mut bytes = 0u64;
        while let Some(dir) = pending.pop() {
            for entry in fs::read_dir(&dir)? {
                let entry = entry?;
                let path = entry.path();
                let meta = fs::symlink_metadata(&path)?;
                ensure!(
                    meta.file_attributes() & 0x400 == 0,
                    "Linked staged/publication entry refused"
                );
                if meta.is_dir() {
                    ensure!(pins.len() < 256, "Directory inventory exceeds bounds");
                    let pinned = Pins::open(&path)?;
                    let rel = path
                        .strip_prefix(root)?
                        .to_string_lossy()
                        .replace('\\', "/");
                    relative(&rel)?;
                    directories.insert(rel, pinned.identity()?);
                    pins.push(pinned);
                    pending.push(path);
                } else {
                    let rel = path
                        .strip_prefix(root)?
                        .to_string_lossy()
                        .replace('\\', "/");
                    relative(&rel)?;
                    ensure!(
                        files.len() < 2048 && lower.insert(rel.to_ascii_lowercase()),
                        "Too many/case-ambiguous publication files"
                    );
                    let source = Source::open(&path)?;
                    bytes = bytes
                        .checked_add(source.owned.bytes)
                        .context("Inventory overflow")?;
                    ensure!(bytes <= 4 * 1024 * 1024 * 1024, "Inventory exceeds 4 GiB");
                    files.insert(rel, source);
                }
            }
        }
        Ok(Self {
            files,
            directories,
            identity,
            _pins: pins,
        })
    }
    pub fn digest(&self) -> String {
        let rows = self
            .files
            .iter()
            .map(|(p, s)| format!("{p}|{}|{}", s.owned.bytes, s.owned.sha256))
            .collect::<Vec<_>>();
        hash(rows.join("\n").as_bytes())
    }
}
pub(super) fn create_parents(root: &Path, relative_file: &str) -> Result<()> {
    relative(relative_file)?;
    let mut current = root.to_path_buf();
    let mut pins = vec![Pins::open(root)?];
    let parts = relative_file.split('/').collect::<Vec<_>>();
    for part in &parts[..parts.len() - 1] {
        current.push(part);
        if missing(&current)? {
            fs::create_dir(&current)?;
        }
        pins.push(Pins::open(&current)?);
    }
    Ok(())
}
