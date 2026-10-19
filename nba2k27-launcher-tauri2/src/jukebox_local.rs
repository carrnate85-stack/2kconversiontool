//! Portable, actual local Jukebox persistence. Private content-addressed native
//! audio and crash-recoverable compare-and-swap catalog updates. No AppData,
//! game files, subprocesses, registry, volume or game-process access.
#![allow(dead_code)]
use crate::{jukebox_audio, jukebox_catalog as catalog, jukebox_files::NativeSource};
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
const MAX_DOC: usize = 16 * 1024 * 1024;
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
        "Jukebox local storage is linked/nonordinary"
    );
    Ok(())
}
struct Pins(Vec<File>);
impl Pins {
    fn pin(path: &Path) -> Result<Self> {
        ensure!(path.is_absolute(), "Absolute Jukebox local path required");
        let (mut p, mut v) = (PathBuf::new(), Vec::new());
        for c in path.components() {
            ensure!(
                !matches!(c, Component::ParentDir | Component::CurDir),
                "Unnormalized Jukebox path"
            );
            p.push(c.as_os_str());
            if matches!(c, Component::Prefix(_)) {
                continue;
            }
            let f = fs::OpenOptions::new()
                .read(true)
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
        "Jukebox local file exceeds bounds"
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
        "Jukebox local file changed during read"
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
        "Invalid local Jukebox destination"
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
            sha256: catalog::digest(&b),
        })
    }
    fn check(&self, f: &mut File, max: usize) -> Result<()> {
        ensure!(
            id(f)? == self.id && Self::capture(f, max)? == *self,
            "Jukebox local file changed/foreign; left untouched"
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
        let identity = id(pins.0.last().context("Empty local Jukebox path")?)?;
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
            "Jukebox catalog directory changed"
        );
        Ok(())
    }
    fn target(&self) -> PathBuf {
        self.root.join("library.json")
    }
    fn marker(&self) -> PathBuf {
        self.root.join("library-update.json")
    }
    fn stage(&self, n: &str) -> PathBuf {
        self.root.join(format!(".2k-jukebox-{n}.next"))
    }
    fn backup(&self, n: &str) -> PathBuf {
        self.root.join(format!(".2k-jukebox-{n}.previous"))
    }
    pub fn load(&self) -> Result<(Option<Vec<u8>>, Revision)> {
        self.dirs()?;
        ensure!(
            absent(&self.marker())?,
            "Jukebox local transaction needs recovery"
        );
        if absent(&self.target())? {
            return Ok((None, Revision(None)));
        }
        let mut f = open(&self.target(), false)?;
        let data = read(&mut f, MAX_DOC)?;
        let revision = Owned::capture(&mut f, MAX_DOC)?;
        Ok((Some(data), Revision(Some(revision))))
    }
    /// Catalog updates are compare-and-swap. Two open editor windows cannot
    /// overwrite one another's newer playlist/order/import changes.
    pub fn commit(&self, expected: &Revision, data: &[u8], nonce: &str) -> Result<Revision> {
        self.dirs()?;
        ensure!(
            catalog::hex(nonce, 48) && !data.is_empty() && data.len() <= MAX_DOC,
            "Invalid Jukebox catalog update"
        );
        serde_json::from_slice::<serde_json::Value>(data)?;
        ensure!(
            absent(&self.marker())? && absent(&self.stage(nonce))? && absent(&self.backup(nonce))?,
            "Jukebox local transaction is already pending"
        );
        let original = if let Some(owned) = &expected.0 {
            let mut f = open(&self.target(), true)?;
            owned.check(&mut f, MAX_DOC)?;
            Some(f)
        } else {
            ensure!(
                absent(&self.target())?,
                "Jukebox catalog appeared; refresh before saving"
            );
            None
        };
        let mut stage = fresh(&self.stage(nonce))?;
        stage.write_all(data)?;
        stage.sync_all()?;
        let next = Owned::capture(&mut stage, MAX_DOC)?;
        let transaction = Transaction {
            schema: "nba2k27-jukebox-library-transaction-v1".into(),
            nonce: nonce.into(),
            directory: self.identity.clone(),
            original: expected.0.clone(),
            next: next.clone(),
        };
        let mut receipt = fresh(&self.marker())?;
        receipt.write_all(&serde_json::to_vec(&transaction)?)?;
        receipt.sync_all()?;
        self.dirs()?;
        if let Some(original) = &original {
            rename(original, &self.backup(nonce))?;
        }
        rename(&stage, &self.target())?;
        next.check(&mut stage, MAX_DOC)?;
        if let Some(original) = &original {
            remove(original)?;
        }
        drop(original);
        remove(&receipt)?;
        Ok(Revision(Some(next)))
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
            t.schema == "nba2k27-jukebox-library-transaction-v1"
                && catalog::hex(&t.nonce, 48)
                && t.directory == self.identity
                && t.next.size <= MAX_DOC
                && catalog::hex(&t.next.sha256, 64),
            "Invalid Jukebox local recovery receipt"
        );
        let backup = if absent(&self.backup(&t.nonce))? {
            None
        } else {
            let mut f = open(&self.backup(&t.nonce), true)?;
            t.original
                .as_ref()
                .context("Unexpected Jukebox catalog backup")?
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
                    .context("Foreign Jukebox catalog appeared")?
                    .check(&mut f, MAX_DOC)?;
                old_current = Some(f);
            }
        }
        ensure!(
            !(committed && staged.is_some()) && !(old_current.is_some() && backup.is_some()),
            "Ambiguous Jukebox catalog recovery state"
        );
        if committed {
            if let Some(f) = backup {
                remove(&f)?;
            }
        } else {
            ensure!(
                t.original.is_none() || old_current.is_some() || backup.is_some(),
                "Original Jukebox catalog is missing"
            );
            if let Some(f) = staged {
                remove(&f)?;
                drop(f);
            }
            if let Some(f) = backup {
                ensure!(absent(&self.target())?, "Foreign Jukebox catalog appeared");
                rename(&f, &self.target())?;
            }
        }
        self.dirs()?;
        remove(&receipt)?;
        drop(current_new);
        Ok(())
    }
}
/// Creates/reuses an integrity-checked private native-audio blob. Existing
/// content is only read, never adopted for deletion or overwritten.
pub(crate) fn cache_native(root: &Path, bytes: &[u8], nonce: &str) -> Result<NativeSource> {
    jukebox_audio::inspect(bytes)?;
    ensure!(
        catalog::hex(nonce, 48),
        "Invalid Jukebox audio import nonce"
    );
    let _pins = Pins::pin(root)?;
    let root = root.canonicalize()?;
    let hash = catalog::digest(bytes);
    let target = root.join(format!("{hash}.wav"));
    let verify = || -> Result<NativeSource> {
        let mut f = open(&target, false)?;
        let data = read(&mut f, jukebox_audio::MAX_AUDIO)?;
        ensure!(
            data.len() == bytes.len() && catalog::digest(&data) == hash,
            "Existing audio cache blob differs; retained untouched"
        );
        jukebox_audio::inspect(&data)?;
        Ok(NativeSource {
            path: target.clone(),
            size: data.len(),
            sha256: hash.clone(),
        })
    };
    if !absent(&target)? {
        return verify();
    }
    let stage = root.join(format!(".2k-jukebox-{nonce}.tmp"));
    let mut f = fresh(&stage)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    let owner = Owned::capture(&mut f, jukebox_audio::MAX_AUDIO)?;
    if let Err(error) = rename(&f, &target) {
        if !absent(&target)? && verify().is_ok() {
            owner.check(&mut f, jukebox_audio::MAX_AUDIO)?;
            remove(&f)?;
        } else {
            return Err(error);
        }
    }
    drop(f);
    verify()
}
pub(crate) fn read_cached(root: &Path, sha: &str, size: usize) -> Result<NativeSource> {
    read_cached_with(root, sha, size, &Default::default())
}
/// Same proof as read_cached, except a deny-write handle on the exact file a
/// previous launch fully hashed (identity/size/time unchanged) skips this
/// early re-hash. Staging still hashes every byte before publication.
pub(crate) fn read_cached_with(
    root: &Path,
    sha: &str,
    size: usize,
    known: &crate::jukebox_prepared::Verified,
) -> Result<NativeSource> {
    ensure!(
        catalog::hex(sha, 64) && (54..=jukebox_audio::MAX_AUDIO).contains(&size),
        "Invalid Jukebox audio reference"
    );
    let _pins = Pins::pin(root)?;
    let path = root
        .canonicalize()?
        .join(format!("{}.wav", sha.to_ascii_lowercase()));
    let mut file = open(&path, false)?;
    if !known.matches(&file, size, sha)? {
        let bytes = read(&mut file, jukebox_audio::MAX_AUDIO)?;
        ensure!(
            bytes.len() == size && catalog::digest(&bytes).eq_ignore_ascii_case(sha),
            "Saved Jukebox audio is missing/changed; library reference retained"
        );
        jukebox_audio::inspect(&bytes)?;
        known.record(&file, size, sha)?;
    }
    Ok(NativeSource {
        path,
        size,
        sha256: sha.to_ascii_lowercase(),
    })
}
pub(crate) fn read_cached_bytes(root: &Path, sha: &str, size: usize) -> Result<Vec<u8>> {
    ensure!(
        catalog::hex(sha, 64) && (54..=jukebox_audio::MAX_AUDIO).contains(&size),
        "Invalid Jukebox audio reference"
    );
    let _pins = Pins::pin(root)?;
    let path = root
        .canonicalize()?
        .join(format!("{}.wav", sha.to_ascii_lowercase()));
    let mut file = open(&path, false)?;
    let bytes = read(&mut file, jukebox_audio::MAX_AUDIO)?;
    ensure!(
        bytes.len() == size && catalog::digest(&bytes).eq_ignore_ascii_case(sha),
        "Saved Jukebox audio is missing/changed"
    );
    jukebox_audio::inspect(&bytes)?;
    Ok(bytes)
}
pub(crate) fn read_art(root: &Path, sha: &str, size: usize, png: bool) -> Result<Vec<u8>> {
    ensure!(
        catalog::hex(sha, 64) && size > 0 && size <= 4 * 1024 * 1024,
        "Invalid artwork reference"
    );
    let _pins = Pins::pin(root)?;
    let path = root.canonicalize()?.join(format!(
        "{}.{}",
        sha.to_ascii_lowercase(),
        if png { "png" } else { "tld" }
    ));
    let mut f = open(&path, false)?;
    let data = read(&mut f, 4 * 1024 * 1024)?;
    ensure!(
        data.len() == size && catalog::digest(&data).eq_ignore_ascii_case(sha),
        "Saved artwork is missing/changed; original reference retained"
    );
    Ok(data)
}
pub(crate) fn ensure_child(root: &Path, name: &str) -> Result<PathBuf> {
    ensure!(
        !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'),
        "Invalid Jukebox owned folder name"
    );
    let _pins = Pins::pin(root)?;
    let child = root.join(name);
    if absent(&child)? {
        fs::create_dir(&child)?;
    }
    let _child = Pins::pin(&child)?;
    Ok(child.canonicalize()?)
}
/// Artwork is retained privately on import even before game-cover publication
/// is wired. Only the two established 256x256 picker formats are accepted.
pub(crate) fn cache_art(
    root: &Path,
    data: &[u8],
    png: bool,
    nonce: &str,
) -> Result<(String, usize)> {
    ensure!(catalog::hex(nonce, 48), "Invalid artwork nonce");
    let extension = if png {
        ensure!(
            data.len() >= 33
                && data.len() <= 4 * 1024 * 1024
                && data.starts_with(b"\x89PNG\r\n\x1a\n")
                && &data[12..16] == b"IHDR"
                && u32::from_be_bytes(data[16..20].try_into()?) == 256
                && u32::from_be_bytes(data[20..24].try_into()?) == 256,
            "Invalid 256x256 cover PNG"
        );
        "png"
    } else {
        ensure!(
            data.len() == 32800
                && data.starts_with(b"TLD ")
                && data[10] == 71
                && data[11] == 1
                && data[16..20] == [0, 1, 0, 1]
                && data[24..32] == [0, 128, 0, 0, 0, 128, 0, 0],
            "Invalid 256x256 BC1 cover"
        );
        "tld"
    };
    let _pins = Pins::pin(root)?;
    let root = root.canonicalize()?;
    let hash = catalog::digest(data);
    let target = root.join(format!("{hash}.{extension}"));
    if !absent(&target)? {
        let mut f = open(&target, false)?;
        ensure!(
            read(&mut f, 4 * 1024 * 1024)? == data,
            "Existing artwork blob differs; retained untouched"
        );
        return Ok((hash, data.len()));
    }
    let stage = root.join(format!(".2k-jukebox-{nonce}.art.tmp"));
    let mut f = fresh(&stage)?;
    f.write_all(data)?;
    f.sync_all()?;
    let owned = Owned::capture(&mut f, 4 * 1024 * 1024)?;
    rename(&f, &target)?;
    owned.check(&mut f, 4 * 1024 * 1024)?;
    Ok((hash, data.len()))
}

#[cfg(test)]
mod tests {
    include!("jukebox_test_paths.rs");
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let p = fixture_artifacts().join(format!(
                "2k-jukebox-local-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::SeqCst)
            ));
            fs::create_dir(&p).unwrap();
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
                    .starts_with("2k-jukebox-local-")
            );
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    #[test]
    fn catalog_updates_are_durable_and_stale_editors_cannot_overwrite() {
        let f = Fixture::new();
        let d = Document::open(&f.0).unwrap();
        let (_, initial) = d.load().unwrap();
        let one = d
            .commit(&initial, br#"{"songs":1}"#, &"a".repeat(48))
            .unwrap();
        assert!(d.commit(&initial, b"{}", &"b".repeat(48)).is_err());
        let two = d.commit(&one, br#"{"songs":2}"#, &"c".repeat(48)).unwrap();
        let (b, r) = d.load().unwrap();
        assert_eq!(b.unwrap(), br#"{"songs":2}"#);
        assert_eq!(r, two);
        assert!(!d.marker().exists());
    }
    #[test]
    fn local_recovery_rolls_back_interrupted_replace() {
        let f = Fixture::new();
        let d = Document::open(&f.0).unwrap();
        let (_, initial) = d.load().unwrap();
        let rev = d.commit(&initial, b"{}", &"a".repeat(48)).unwrap();
        let n = "b".repeat(48);
        let mut staged = fresh(&d.stage(&n)).unwrap();
        staged.write_all(br#"{"next":1}"#).unwrap();
        staged.sync_all().unwrap();
        let t = Transaction {
            schema: "nba2k27-jukebox-library-transaction-v1".into(),
            nonce: n.clone(),
            directory: d.identity.clone(),
            original: rev.0.clone(),
            next: Owned::capture(&mut staged, MAX_DOC).unwrap(),
        };
        let mut receipt = fresh(&d.marker()).unwrap();
        receipt.write_all(&serde_json::to_vec(&t).unwrap()).unwrap();
        receipt.sync_all().unwrap();
        let original = open(&d.target(), true).unwrap();
        rename(&original, &d.backup(&n)).unwrap();
        drop(original);
        drop(receipt);
        drop(staged);
        d.recover().unwrap();
        assert_eq!(d.load().unwrap().0.unwrap(), b"{}");
    }
    #[test]
    fn private_cache_deduplicates_without_overwriting_foreign_bytes() {
        let f = Fixture::new();
        let native = jukebox_audio::wrap(&jukebox_audio::pcm_wave(&[0; 8]).unwrap()).unwrap();
        let a = cache_native(&f.0, &native, &"a".repeat(48)).unwrap();
        let b = cache_native(&f.0, &native, &"b".repeat(48)).unwrap();
        assert_eq!(a.path, b.path);
        fs::write(&a.path, b"foreign").unwrap();
        assert!(cache_native(&f.0, &native, &"c".repeat(48)).is_err());
        assert_eq!(fs::read(&a.path).unwrap(), b"foreign");
    }
}
