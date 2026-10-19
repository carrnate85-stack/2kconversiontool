//! Real Win32 Jukebox session publication/recovery. Unwired until the native
//! session guardian supplies fresh game/protected-process-closed checks.
//! Never overwrites: an original music.iff is renamed by its retained handle
//! to a same-volume private backup, then the fresh owned music is published.
//! All cleanup preflights identities AND hashes before touching any file.
#![allow(dead_code)]
use crate::{
    jukebox_audio,
    jukebox_catalog::{self as catalog, Plan},
    jukebox_cover_route as cover, jukebox_package,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
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
pub(crate) const MARKER: &str = "jukebox-session.json";
const SCHEMA: &str = "nba2k27-rust-jukebox-files-v1";
const MAX_RECEIPT: usize = 256 * 1024;
/// music.iff + 71 audio slots + the cover container + 71 cover textures.
const MAX_ENTRIES: usize = 1 + 71 + 1 + cover::MAX_COVERS;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Music,
    Audio,
    Container,
    Texture,
}
fn kind(target: &str) -> Option<Kind> {
    if target == "music.iff" {
        Some(Kind::Music)
    } else if (1..=71).any(|s| catalog::audio_path(s) == target) {
        Some(Kind::Audio)
    } else if target == cover::CONTAINER {
        Some(Kind::Container)
    } else if cover::texture_path(target) {
        Some(Kind::Texture)
    } else {
        None
    }
}
fn limit(target: &str) -> usize {
    match kind(target) {
        Some(Kind::Music) => catalog::MAX_IFF,
        Some(Kind::Container) => cover::MAX_CONTAINER,
        Some(Kind::Texture) => cover::MAX_TEXTURE,
        _ => jukebox_audio::MAX_AUDIO,
    }
}
/// `shared` or `shared/<hh>`: the only folders a cover publication creates.
fn cover_dir(path: &str) -> bool {
    path == "shared"
        || path.strip_prefix("shared/").is_some_and(|h| {
            h.len() == 2
                && h.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
}
/// Restore removes a folder it created only when it is an ordinary, empty one.
fn remove_if_empty(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
        Ok(m) => {
            if m.is_dir()
                && m.file_attributes() & 0x400 == 0
                && fs::read_dir(path)?.next().is_none()
            {
                fs::remove_dir(path)?;
            }
            Ok(())
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Id {
    volume: u64,
    index: [u8; 16],
}
fn info(file: &File) -> Result<BY_HANDLE_FILE_INFORMATION> {
    let mut out = BY_HANDLE_FILE_INFORMATION::default();
    unsafe {
        GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut out)?;
    }
    Ok(out)
}
fn identity(file: &File) -> Result<Id> {
    let mut i = FILE_ID_INFO::default();
    unsafe {
        GetFileInformationByHandleEx(
            HANDLE(file.as_raw_handle()),
            FileIdInfo,
            &mut i as *mut _ as _,
            size_of::<FILE_ID_INFO>() as u32,
        )?;
    }
    // CreationTime is NOT a file identity: NTFS tunneling may legitimately
    // transfer it when a retained file is renamed to a recently vacated name.
    // The kernel's volume + full 128-bit FileId is stable across that rename.
    Ok(Id {
        volume: i.VolumeSerialNumber,
        index: i.FileId.Identifier,
    })
}
fn ordinary(file: &File, dir: bool) -> Result<()> {
    let m = file.metadata()?;
    ensure!(
        m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 == 0
            && if dir {
                m.is_dir()
            } else {
                m.is_file() && info(file)?.nNumberOfLinks == 1
            },
        "Jukebox storage is linked/nonordinary"
    );
    Ok(())
}
struct Pins(Vec<File>);
impl Pins {
    fn pin(path: &Path) -> Result<Self> {
        ensure!(path.is_absolute(), "Absolute Jukebox storage path required");
        let (mut current, mut pins) = (PathBuf::new(), Vec::new());
        for part in path.components() {
            ensure!(
                !matches!(part, Component::ParentDir | Component::CurDir),
                "Unnormalized Jukebox storage path"
            );
            current.push(part.as_os_str());
            if matches!(part, Component::Prefix(_)) {
                continue;
            }
            let file = fs::OpenOptions::new()
                .read(true)
                .share_mode(3)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
                .open(&current)?;
            ordinary(&file, true)?;
            pins.push(file);
        }
        ensure!(!pins.is_empty(), "Empty Jukebox storage path");
        Ok(Self(pins))
    }
}
fn dir_id(path: &Path) -> Result<Id> {
    let p = Pins::pin(path)?;
    identity(p.0.last().unwrap())
}
fn absent(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(e.into()),
    }
}
fn open(path: &Path, write: bool, delete: bool) -> Result<File> {
    let f = fs::OpenOptions::new()
        .read(true)
        .access_mode(
            0x80000000 | if write { 0x40000000 } else { 0 } | if delete { 0x10000 } else { 0 },
        )
        .share_mode(1)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)?;
    ordinary(&f, false)?;
    Ok(f)
}
fn fresh(path: &Path) -> Result<File> {
    let f = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .access_mode(0xc0010000)
        .share_mode(1)
        .create_new(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)?;
    ordinary(&f, false)?;
    Ok(f)
}
fn unlink(file: &File) -> Result<()> {
    let i = FILE_DISPOSITION_INFO {
        DeleteFile: true.into(),
    };
    unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileDispositionInfo,
            &i as *const _ as _,
            size_of::<FILE_DISPOSITION_INFO>() as u32,
        )?;
    }
    Ok(())
}
fn rename(file: &File, path: &Path) -> Result<()> {
    let name = std::path::absolute(path)?
        .as_os_str()
        .encode_wide()
        .collect::<Vec<_>>();
    ensure!(
        !name.is_empty() && name.len() <= 32760 && !name.contains(&0),
        "Jukebox destination path exceeds bounds"
    );
    let len = size_of::<FILE_RENAME_INFO>() + (name.len() + 1) * 2;
    let mut data = vec![0u64; len.div_ceil(8)];
    let i = data.as_mut_ptr() as *mut FILE_RENAME_INFO;
    unsafe {
        (*i).Anonymous.ReplaceIfExists = false;
        (*i).FileNameLength = (name.len() * 2) as u32;
        std::ptr::copy_nonoverlapping(name.as_ptr(), (*i).FileName.as_mut_ptr(), name.len());
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileRenameInfo,
            data.as_mut_ptr() as _,
            len as u32,
        )
        .with_context(|| format!("Renaming exact owned Jukebox handle to {}", path.display()))?;
    }
    Ok(())
}
fn fingerprint(file: &mut File, limit: usize) -> Result<(usize, String)> {
    ordinary(file, false)?;
    let id = identity(file)?;
    let before = file.metadata()?;
    ensure!(
        before.len() <= limit as u64,
        "Jukebox file exceeds size bound"
    );
    file.seek(SeekFrom::Start(0))?;
    let mut sha = Sha256::new();
    let mut buffer = vec![0; 1024 * 1024];
    let mut total = 0usize;
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        total = total.checked_add(n).context("Jukebox size overflow")?;
        ensure!(total <= limit, "Jukebox file grew beyond its bound");
        sha.update(&buffer[..n]);
    }
    let after = file.metadata()?;
    ensure!(
        identity(file)? == id
            && before.len() == total as u64
            && after.len() == before.len()
            && after.last_write_time() == before.last_write_time(),
        "Jukebox file changed during verification"
    );
    Ok((total, format!("{:x}", sha.finalize())))
}
/// One read pass through a retained deny-write handle, proving the handle's
/// identity, size and time were stable. Callers hash the returned bytes.
fn read_once(file: &mut File, limit: usize) -> Result<Vec<u8>> {
    ordinary(file, false)?;
    let id = identity(file)?;
    let before = file.metadata()?;
    ensure!(
        before.len() <= limit as u64,
        "Jukebox file exceeds size bound"
    );
    file.seek(SeekFrom::Start(0))?;
    let mut data = Vec::with_capacity(before.len() as usize);
    file.take(limit as u64 + 1).read_to_end(&mut data)?;
    let after = file.metadata()?;
    ensure!(
        identity(file)? == id
            && data.len() as u64 == before.len()
            && after.len() == before.len()
            && after.last_write_time() == before.last_write_time(),
        "Jukebox file changed during read"
    );
    Ok(data)
}
fn bytes(file: &mut File, limit: usize) -> Result<Vec<u8>> {
    let (size, hash) = fingerprint(file, limit)?;
    file.seek(SeekFrom::Start(0))?;
    let mut data = Vec::with_capacity(size);
    file.take(limit as u64 + 1).read_to_end(&mut data)?;
    ensure!(
        data.len() == size && catalog::digest(&data) == hash,
        "Jukebox source changed while reading"
    );
    Ok(data)
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Owned {
    identity: Id,
    size: usize,
    sha256: String,
}
impl Owned {
    fn capture(file: &mut File, limit: usize) -> Result<Self> {
        let (size, sha256) = fingerprint(file, limit)?;
        Ok(Self {
            identity: identity(file)?,
            size,
            sha256,
        })
    }
    fn check(&self, file: &mut File, limit: usize) -> Result<()> {
        let actual = identity(file)?;
        let hash = fingerprint(file, limit)?;
        ensure!(
            actual == self.identity && hash == (self.size, self.sha256.clone()),
            "Jukebox owned file identity/bytes changed; left untouched (expected {:?} {} {}, actual {:?} {} {})",
            self.identity,
            self.size,
            self.sha256,
            actual,
            hash.0,
            hash.1
        );
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Entry {
    target: String,
    stage: String,
    size: usize,
    sha256: String,
    identity: Option<Id>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    schema: String,
    session: String,
    root: PathBuf,
    portable: Id,
    data: Id,
    mods: Id,
    audio: Id,
    original: Option<Owned>,
    backup: String,
    entries: Vec<Entry>,
    publishing: bool,
    /// Cover folders this session created (absent in older receipts).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    created: Vec<String>,
}
impl Record {
    fn validate(&self, mods: &Path, session: &str) -> Result<()> {
        ensure!(
            self.schema == SCHEMA
                && self.session == session
                && catalog::hex(session, 48)
                && self.root == mods
                && self.root.is_absolute()
                && self.backup == format!(".2k-launcher-jukebox-{session}-original.iff")
                && (1..=MAX_ENTRIES).contains(&self.entries.len()),
            "Jukebox recovery ownership/schema differs"
        );
        let mut names = std::collections::BTreeSet::new();
        // Order: music.iff, audio slots, then the cover container and its
        // textures (textures only with a container, at most 71).
        let mut previous = Kind::Music;
        let mut textures = 0;
        for (i, e) in self.entries.iter().enumerate() {
            let k = kind(&e.target).context("Unknown Jukebox target")?;
            ensure!(
                (i == 0) == (k == Kind::Music)
                    && (k >= previous)
                    && !(k == Kind::Container && previous == Kind::Container)
                    && (k != Kind::Texture || previous >= Kind::Container),
                "Jukebox publication order differs"
            );
            previous = k;
            textures += usize::from(k == Kind::Texture);
            ensure!(
                names.insert(&e.target)
                    && e.stage == format!(".2k-launcher-jukebox-{session}-{i}.tmp")
                    && e.size > 0
                    && e.size <= limit(&e.target)
                    && catalog::hex(&e.sha256, 64)
                    && e.identity.is_some() == self.publishing,
                "Invalid Jukebox publication entry"
            );
        }
        ensure!(
            textures <= cover::MAX_COVERS
                && self.created.len() <= 1 + 256
                && self.created.iter().all(|d| cover_dir(d))
                && self
                    .created
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    == self.created.len(),
            "Invalid Jukebox cover publication"
        );
        if let Some(o) = &self.original {
            ensure!(
                (22..=catalog::MAX_IFF).contains(&o.size) && catalog::hex(&o.sha256, 64),
                "Invalid original Jukebox backup"
            );
        }
        Ok(())
    }
}
fn read_record(file: &mut File) -> Result<Record> {
    let raw = bytes(file, MAX_RECEIPT)?;
    let mut records = serde_json::Deserializer::from_slice(&raw).into_iter::<Record>();
    let first = records.next().context("Empty Jukebox receipt")??;
    ensure!(
        !first.publishing && first.entries.iter().all(|e| e.identity.is_none()),
        "Jukebox receipt lacks original intent"
    );
    let Some(second) = records.next() else {
        return Ok(first);
    };
    let second = second?;
    ensure!(
        second.publishing
            && second.entries.iter().all(|e| e.identity.is_some())
            && records.next().is_none(),
        "Invalid Jukebox publication record count"
    );
    let mut base = second.clone();
    base.publishing = false;
    for e in &mut base.entries {
        e.identity = None;
    }
    ensure!(base == first, "Jukebox publication differs from intent");
    Ok(second)
}
#[derive(Clone, Debug)]
pub(crate) struct NativeSource {
    pub path: PathBuf,
    pub size: usize,
    pub sha256: String,
}
/// Receipt is owned by the outer durable native-session journal. No source
/// paths or arbitrary target paths are saved into this recovery authority.
pub(crate) struct Store {
    portable: PathBuf,
    record: Record,
    receipt: Id,
    _pins: Vec<Pins>,
}
impl Store {
    fn marker(&self) -> PathBuf {
        self.portable.join("Data").join(MARKER)
    }
    fn stage(&self, e: &Entry) -> PathBuf {
        self.record
            .root
            .join(&e.target)
            .parent()
            .unwrap()
            .join(&e.stage)
    }
    fn target(&self, e: &Entry) -> PathBuf {
        self.record.root.join(&e.target)
    }
    fn backup(&self) -> PathBuf {
        self.record.root.join(&self.record.backup)
    }
    fn dirs(&self) -> Result<()> {
        ensure!(
            dir_id(&self.portable)? == self.record.portable
                && dir_id(&self.portable.join("Data"))? == self.record.data
                && dir_id(&self.record.root)? == self.record.mods
                && dir_id(&self.record.root.join("newaudio/jukeboxmusic"))? == self.record.audio,
            "Jukebox directory identity changed"
        );
        Ok(())
    }
    fn receipt(&self, write: bool, delete: bool) -> Result<File> {
        let mut f = open(&self.marker(), write, delete)?;
        ensure!(
            identity(&f)? == self.receipt && read_record(&mut f)? == self.record,
            "Jukebox receipt changed"
        );
        Ok(f)
    }
    fn owned(&self, e: &Entry, mut f: File) -> Result<File> {
        let id = e
            .identity
            .as_ref()
            .context("Jukebox stage ownership was not committed; retained for inspection")?;
        Owned {
            identity: id.clone(),
            size: e.size,
            sha256: e.sha256.clone(),
        }
        .check(&mut f, limit(&e.target))?;
        Ok(f)
    }
    /// Creates durable intent, then stages and validates all files. Does not
    /// publish into logical Mods paths until activate() succeeds.
    pub fn create(
        portable: &Path,
        mods: &Path,
        session: &str,
        plan: &Plan,
        sources: &BTreeMap<String, NativeSource>,
        closed: impl Fn() -> Result<()>,
    ) -> Result<Self> {
        Self::create_with(
            portable,
            mods,
            session,
            plan,
            sources,
            closed,
            &Default::default(),
        )
    }
    /// `known` may skip only the early pre-intent re-hash of a source whose
    /// retained deny-write handle matches a prior full hash. Staging below
    /// always hashes every byte it writes against the expected SHA-256.
    pub fn create_with(
        portable: &Path,
        mods: &Path,
        session: &str,
        plan: &Plan,
        sources: &BTreeMap<String, NativeSource>,
        closed: impl Fn() -> Result<()>,
        known: &crate::jukebox_prepared::Verified,
    ) -> Result<Self> {
        Self::create_with_covers(portable, mods, session, plan, sources, &[], closed, known)
    }
    /// `covers` is the cover route's container (first) and textures, held in
    /// memory like music.iff and published under the same owned receipt.
    #[allow(clippy::too_many_arguments)]
    pub fn create_with_covers(
        portable: &Path,
        mods: &Path,
        session: &str,
        plan: &Plan,
        sources: &BTreeMap<String, NativeSource>,
        covers: &[(String, Vec<u8>)],
        closed: impl Fn() -> Result<()>,
        known: &crate::jukebox_prepared::Verified,
    ) -> Result<Self> {
        closed()?;
        ensure!(catalog::hex(session, 48), "Invalid Jukebox session ID");
        plan.policy.validate()?;
        let portable = portable.canonicalize()?;
        let mods = mods.canonicalize()?;
        let mut pins = vec![
            Pins::pin(&portable)?,
            Pins::pin(&portable.join("Data"))?,
            Pins::pin(&mods)?,
        ];
        ensure!(
            absent(&portable.join("Data").join(MARKER))?,
            "Jukebox recovery is pending"
        );
        for name in ["newaudio", "newaudio/jukeboxmusic"] {
            let p = mods.join(name);
            closed()?;
            if absent(&p)? {
                fs::create_dir(&p)?;
            }
            pins.push(Pins::pin(&p)?);
        }
        let original = if absent(&mods.join("music.iff"))? {
            None
        } else {
            let mut f = open(&mods.join("music.iff"), false, true)?;
            let b = bytes(&mut f, catalog::MAX_IFF)?;
            jukebox_package::read_music(&b)?;
            Some(Owned::capture(&mut f, catalog::MAX_IFF)?)
        };
        let music = jukebox_package::build_music(&plan.music)?;
        let mut entries = vec![Entry {
            target: "music.iff".into(),
            stage: format!(".2k-launcher-jukebox-{session}-0.tmp"),
            size: music.len(),
            sha256: catalog::digest(&music),
            identity: None,
        }];
        ensure!(
            sources.len() == plan.mappings.len(),
            "Prepared audio count differs from playlist"
        );
        let mut source_inputs = Vec::new();
        for (i, m) in plan.mappings.iter().enumerate() {
            ensure!(
                (1..=71).contains(&m.slot)
                    && m.target == catalog::audio_path(m.slot)
                    && m.source != m.target,
                "Invalid Jukebox audio mapping"
            );
            let source = sources.get(&m.source).context("Prepared song is missing")?;
            ensure!(
                source.path.is_absolute()
                    && (54..=jukebox_audio::MAX_AUDIO).contains(&source.size)
                    && catalog::hex(&source.sha256, 64),
                "Invalid prepared native audio"
            );
            // Validate EVERY source before durable intent or any stage file.
            // Retain deny-write/delete handles and parent pins until staging
            // finishes. A missing last song cannot strand an unowned stage;
            // a later path swap cannot substitute different bytes.
            closed()?;
            let source_pin = Pins::pin(
                source
                    .path
                    .parent()
                    .context("Native audio path has no parent")?,
            )?;
            let mut input = open(&source.path, false, false)?;
            if !known.matches(&input, source.size, &source.sha256)? {
                let data = bytes(&mut input, jukebox_audio::MAX_AUDIO)?;
                ensure!(
                    data.len() == source.size
                        && catalog::digest(&data).eq_ignore_ascii_case(&source.sha256),
                    "Prepared native source changed before session intent"
                );
                jukebox_audio::inspect(&data)?;
                known.record(&input, source.size, &source.sha256)?;
            }
            source_inputs.push((source_pin, input));
            entries.push(Entry {
                target: m.target.clone(),
                stage: format!(".2k-launcher-jukebox-{session}-{}.tmp", i + 1),
                size: source.size,
                sha256: source.sha256.to_ascii_lowercase(),
                identity: None,
            });
        }
        // Cover files: container then textures, each validated before intent.
        // Their folders are pinned only while staging, so restore can remove
        // the ones this session created.
        let (mut created, mut cover_pins) = (Vec::new(), Vec::new());
        for (j, (target, data)) in covers.iter().enumerate() {
            let k = kind(target);
            ensure!(
                (j == 0) == (k == Some(Kind::Container))
                    && matches!(k, Some(Kind::Container | Kind::Texture))
                    && (1..=limit(target)).contains(&data.len()),
                "Invalid Jukebox cover publication file"
            );
            if k == Some(Kind::Texture) {
                cover::validate_texture(data)?;
                for dir in ["shared".to_owned(), target[..9].to_owned()] {
                    let p = mods.join(&dir);
                    closed()?;
                    if absent(&p)? {
                        fs::create_dir(&p)?;
                        created.push(dir);
                    }
                }
                cover_pins.push(Pins::pin(&mods.join(&target[..9]))?);
            } else {
                cover::Container::parse(data.clone())?;
            }
            entries.push(Entry {
                target: target.clone(),
                stage: format!(".2k-launcher-jukebox-{session}-{}.tmp", entries.len()),
                size: data.len(),
                sha256: catalog::digest(data),
                identity: None,
            });
        }
        let audio_count = source_inputs.len();
        let record = Record {
            schema: SCHEMA.into(),
            session: session.into(),
            root: mods.clone(),
            portable: dir_id(&portable)?,
            data: dir_id(&portable.join("Data"))?,
            mods: dir_id(&mods)?,
            audio: dir_id(&mods.join("newaudio/jukeboxmusic"))?,
            original,
            backup: format!(".2k-launcher-jukebox-{session}-original.iff"),
            entries,
            publishing: false,
            created,
        };
        record.validate(&mods, session)?;
        let mut store = Self {
            portable,
            record,
            receipt: Id {
                volume: 0,
                index: [0; 16],
            },
            _pins: pins,
        };
        ensure!(
            absent(&store.backup())?,
            "Original Jukebox backup path already exists"
        );
        for (i, e) in store.record.entries.iter().enumerate() {
            ensure!(
                absent(&store.stage(e))? && (i == 0 || absent(&store.target(e))?),
                "Existing audio mod/stage was left untouched"
            );
        }
        let intent = serde_json::to_vec(&store.record)?;
        ensure!(
            intent.len() * 2 + 2 <= MAX_RECEIPT,
            "Jukebox receipt exceeds bounds"
        );
        closed()?;
        let mut receipt = fresh(&store.marker())?;
        receipt.write_all(&intent)?;
        receipt.sync_all()?;
        store.receipt = identity(&receipt)?;
        let mut stages: Vec<File> = Vec::new();
        // Until the publication record below commits stage identities,
        // restore() cannot prove (and so never deletes) a stage: a stranded
        // receipt would block every later launch. Any staging failure (source
        // proof, a protected process starting, disk full, readback) therefore
        // removes exactly the files this call created, through their retained
        // create_new handles, including a partially written stage.
        let staged = (|| -> Result<Record> {
            for (i, e) in store.record.entries.iter().enumerate() {
                closed()?;
                store.dirs()?;
                // Verify each payload BEFORE creating its stage. Single pass:
                // the retained deny-write handle cannot change, and the digest
                // proves every byte that will be staged.
                let data = if i == 0 {
                    music.clone()
                } else if i <= audio_count {
                    let data = read_once(&mut source_inputs[i - 1].1, jukebox_audio::MAX_AUDIO)?;
                    jukebox_audio::inspect(&data)?;
                    data
                } else {
                    covers[i - 1 - audio_count].1.clone()
                };
                ensure!(
                    data.len() == e.size && catalog::digest(&data) == e.sha256,
                    "Prepared song changed before staging"
                );
                stages.push(fresh(&store.stage(e))?);
                let stage = stages.last_mut().context("Jukebox stage handle missing")?;
                closed()?;
                stage.write_all(&data)?;
                stage.sync_all()?;
                ensure!(
                    fingerprint(stage, jukebox_audio::MAX_AUDIO)? == (e.size, e.sha256.clone()),
                    "Jukebox stage readback failed"
                );
            }
            let mut publication = store.record.clone();
            publication.publishing = true;
            for (e, f) in publication.entries.iter_mut().zip(&stages) {
                e.identity = Some(identity(f)?);
            }
            closed()?;
            store.dirs()?;
            ensure!(
                read_record(&mut receipt)? == store.record,
                "Jukebox intent changed"
            );
            let mut append = vec![b'\n'];
            append.extend(serde_json::to_vec(&publication)?);
            append.push(b'\n');
            receipt.seek(SeekFrom::End(0))?;
            receipt.write_all(&append)?;
            receipt.sync_all()?;
            ensure!(
                read_record(&mut receipt)? == publication,
                "Jukebox publication receipt readback failed"
            );
            Ok(publication)
        })();
        let publication = match staged {
            Ok(publication) => publication,
            Err(error) => {
                let rollback = (|| -> Result<()> {
                    for stage in &stages {
                        unlink(stage)?;
                    }
                    unlink(&receipt)
                })();
                return Err(match rollback {
                    Ok(()) => error.context(
                        "Jukebox staging failed; this launch's unpublished stages were removed",
                    ),
                    Err(cleanup) => error.context(format!(
                        "Unpublished Jukebox stage cleanup also failed: {cleanup:#}"
                    )),
                });
            }
        };
        store.record = publication;
        drop(stages);
        drop(receipt);
        drop(cover_pins);
        Ok(store)
    }
    pub fn activate(&self, closed: impl Fn() -> Result<()>) -> Result<()> {
        closed()?;
        self.dirs()?;
        self.record
            .validate(&self.record.root, &self.record.session)?;
        ensure!(self.record.publishing, "Jukebox stages were not committed");
        let _receipt = self.receipt(false, false)?;
        let mut stages = Vec::new();
        for e in &self.record.entries {
            stages.push(self.owned(e, open(&self.stage(e), false, true)?)?);
        }
        for e in self.record.entries.iter().skip(1) {
            ensure!(
                absent(&self.target(e))?,
                "An audio mod appeared; left untouched"
            );
        }
        ensure!(absent(&self.backup())?, "Original Jukebox backup appeared");
        let original = if let Some(o) = &self.record.original {
            let mut f = open(&self.record.root.join("music.iff"), false, true)?;
            o.check(&mut f, catalog::MAX_IFF)?;
            Some(f)
        } else {
            ensure!(
                absent(&self.record.root.join("music.iff"))?,
                "music.iff appeared; left untouched"
            );
            None
        };
        // Original's exact identity is held throughout the no-overwrite move.
        if let Some(original) = original {
            closed()?;
            self.dirs()?;
            rename(&original, &self.backup())?;
        }
        for (e, f) in self.record.entries.iter().zip(&stages).skip(1) {
            closed()?;
            self.dirs()?;
            rename(f, &self.target(e))?;
        }
        closed()?;
        self.dirs()?;
        rename(&stages[0], &self.target(&self.record.entries[0]))?;
        drop(stages);
        drop(_receipt);
        self.verify_current()
    }
    /// Owned cover route files (container first), for provider publication.
    pub fn cover_targets(&self) -> Vec<String> {
        self.record
            .entries
            .iter()
            .filter(|e| matches!(kind(&e.target), Some(Kind::Container | Kind::Texture)))
            .map(|e| e.target.clone())
            .collect()
    }
    pub fn verify_current(&self) -> Result<()> {
        self.dirs()?;
        let _r = self.receipt(false, false)?;
        ensure!(self.record.publishing, "Jukebox publication is incomplete");
        for e in &self.record.entries {
            ensure!(absent(&self.stage(e))?, "Jukebox stage remains unpublished");
            self.owned(e, open(&self.target(e), false, false)?)?;
        }
        if let Some(o) = &self.record.original {
            o.check(&mut open(&self.backup(), false, false)?, catalog::MAX_IFF)?;
        } else {
            ensure!(absent(&self.backup())?, "Unexpected Jukebox backup");
        }
        Ok(())
    }
    /// Recover root authority only from this bounded exact-session receipt,
    /// followed by its existing actual directory-identity validation.
    pub(crate) fn recorded_root(portable: &Path, session: &str) -> Result<Option<PathBuf>> {
        let _portable = Pins::pin(portable)?;
        let _data = Pins::pin(&portable.join("Data"))?;
        let marker = portable.join("Data").join(MARKER);
        if absent(&marker)? {
            return Ok(None);
        }
        let record = read_record(&mut open(&marker, false, false)?)?;
        let store = Self::recover(portable, &record.root, session)?;
        Ok(store.map(|s| s.record.root.clone()))
    }
    pub fn recover(portable: &Path, mods: &Path, session: &str) -> Result<Option<Self>> {
        let portable = portable.canonicalize()?;
        let _p = Pins::pin(&portable)?;
        let _d = Pins::pin(&portable.join("Data"))?;
        let marker = portable.join("Data").join(MARKER);
        if absent(&marker)? {
            return Ok(None);
        }
        let mut f = open(&marker, false, false)?;
        let receipt = identity(&f)?;
        let record = read_record(&mut f)?;
        let mods = mods.canonicalize()?;
        record.validate(&mods, session)?;
        let pins = vec![
            _p,
            _d,
            Pins::pin(&mods)?,
            Pins::pin(&mods.join("newaudio"))?,
            Pins::pin(&mods.join("newaudio/jukeboxmusic"))?,
        ];
        let store = Self {
            portable,
            record,
            receipt,
            _pins: pins,
        };
        store.dirs()?;
        Ok(Some(store))
    }
    /// Recognizes partially published and already restored states. Preflight
    /// ALL entries first, so a foreign last song never causes partial deletion.
    pub fn restore(&self, closed: impl Fn() -> Result<()>) -> Result<()> {
        closed()?;
        self.dirs()?;
        let receipt = self.receipt(false, true)?;
        let mut remove = Vec::new();
        let mut original_in_place = false;
        let mut current_original = None;
        let original_backup = if absent(&self.backup())? {
            None
        } else {
            let mut f = open(&self.backup(), false, true)?;
            self.record
                .original
                .as_ref()
                .context("Unexpected Jukebox backup; left untouched")?
                .check(&mut f, catalog::MAX_IFF)?;
            Some(f)
        };
        for (i, e) in self.record.entries.iter().enumerate() {
            let stage = self.stage(e);
            let target = self.target(e);
            let staged = if absent(&stage)? {
                None
            } else {
                Some(self.owned(e, open(&stage, false, true)?)?)
            };
            let published = if absent(&target)? {
                None
            } else {
                let mut f = open(&target, false, true)?;
                if i == 0
                    && self
                        .record
                        .original
                        .as_ref()
                        .is_some_and(|o| identity(&f).is_ok_and(|id| id == o.identity))
                {
                    self.record
                        .original
                        .as_ref()
                        .unwrap()
                        .check(&mut f, catalog::MAX_IFF)?;
                    original_in_place = true;
                    current_original = Some(f);
                    None
                } else {
                    Some(self.owned(e, f)?)
                }
            };
            ensure!(
                staged.is_none() || published.is_none(),
                "Jukebox file exists at both stage and target"
            );
            if let Some(f) = staged {
                remove.push(f);
            }
            if let Some(f) = published {
                remove.push(f);
            }
        }
        ensure!(
            !(original_in_place && original_backup.is_some()),
            "Original Jukebox is duplicated/ambiguous"
        );
        ensure!(
            self.record.original.is_none() || original_in_place || original_backup.is_some(),
            "Original Jukebox backup is missing; nothing changed"
        );
        for f in remove {
            closed()?;
            self.dirs()?;
            unlink(&f)?;
            drop(f);
        }
        if let Some(original) = original_backup {
            closed()?;
            self.dirs()?;
            ensure!(
                absent(&self.record.root.join("music.iff"))?,
                "A foreign music.iff appeared; original retained"
            );
            rename(&original, &self.record.root.join("music.iff"))?;
        }
        // Folders this session created for covers, deepest first, only if empty.
        for dir in self.record.created.iter().rev() {
            closed()?;
            self.dirs()?;
            remove_if_empty(&self.record.root.join(dir))?;
        }
        closed()?;
        self.dirs()?;
        unlink(&receipt)?;
        drop(current_original);
        Ok(())
    }
}

#[cfg(test)]
#[path = "jukebox_files_tests.rs"]
mod tests;
