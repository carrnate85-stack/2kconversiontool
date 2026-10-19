//! Fresh-file-only physical camera localization, prepared before Steam starts.
//! Never adopts/overwrites an installed localization mod. Publication and
//! removal use the exact owned handle, and every mutation requires a fresh
//! caller-supplied game-closed check. No game/process access lives here.
#![allow(dead_code)]
use crate::{
    backend,
    loader_files::{self, Directories},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Path, PathBuf},
};
use windows::Win32::{Foundation::HANDLE, Storage::FileSystem::*};

pub(crate) const MARKER: &str = "camera-names-session.json";
const SCHEMA: &str = "nba2k27-rust-camera-name-files-v1";
const TARGET: &str = "loc/english.iff";
const MAX_BYTES: usize = 16 * 1024 * 1024;
const RECEIPT_LIMIT: usize = 64 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Id {
    volume: u32,
    index: u64,
    birth: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stable: Option<crate::file_identity::Stable>,
}
impl Id {
    fn matches(&self, actual: &Self) -> bool {
        match &self.stable {
            Some(expected) => actual.stable.as_ref() == Some(expected),
            None => {
                self.volume == actual.volume
                    && self.index == actual.index
                    && self.birth == actual.birth
            }
        }
    }
}
fn identity(file: &File) -> Result<Id> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    unsafe {
        GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info)?;
    }
    Ok(Id {
        volume: info.dwVolumeSerialNumber,
        index: ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64,
        birth: ((info.ftCreationTime.dwHighDateTime as u64) << 32)
            | info.ftCreationTime.dwLowDateTime as u64,
        stable: Some(crate::file_identity::Stable::read(file)?),
    })
}
fn ordinary(file: &File, directory: bool) -> Result<()> {
    let meta = file.metadata()?;
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    unsafe {
        GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info)?;
    }
    ensure!(
        meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 == 0
            && if directory {
                meta.is_dir()
            } else {
                meta.is_file() && info.nNumberOfLinks == 1
            },
        "Camera localization storage is linked or not an ordinary owned file/directory"
    );
    Ok(())
}
fn open_mode(path: &Path, write: bool, delete: bool) -> Result<File> {
    let file = fs::OpenOptions::new()
        .read(true)
        .access_mode(
            0x80000000 | if write { 0x40000000 } else { 0 } | if delete { 0x10000 } else { 0 },
        )
        .share_mode(1)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)?;
    ordinary(&file, false)?;
    Ok(file)
}
fn open(path: &Path, delete: bool) -> Result<File> {
    open_mode(path, false, delete)
}
fn dir_id(path: &Path) -> Result<Id> {
    let file = fs::OpenOptions::new()
        .read(true)
        .share_mode(3)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)?;
    ordinary(&file, true)?;
    identity(&file)
}
fn missing(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(e.into()),
    }
}
fn bytes(file: &mut File, limit: usize) -> Result<Vec<u8>> {
    ordinary(file, false)?;
    let before_id = identity(file)?;
    let before = file.metadata()?;
    ensure!(
        before.len() <= limit as u64,
        "Camera localization/receipt exceeds its bound"
    );
    file.seek(SeekFrom::Start(0))?;
    let mut result = Vec::new();
    file.take((limit + 1) as u64).read_to_end(&mut result)?;
    let after = file.metadata()?;
    ordinary(file, false)?;
    ensure!(
        identity(file)? == before_id
            && result.len() as u64 == before.len()
            && before.len() == after.len()
            && before.last_write_time() == after.last_write_time(),
        "Camera localization/receipt changed during verification"
    );
    Ok(result)
}
fn hex(text: &str, length: usize) -> bool {
    text.len() == length && text.bytes().all(|b| b.is_ascii_hexdigit())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    schema: String,
    session: String,
    root: PathBuf,
    portable_identity: Id,
    data_identity: Id,
    root_identity: Id,
    loc_identity: Id,
    target: String,
    staged: String,
    size: usize,
    sha256: String,
    publishing: bool,
    file_identity: Option<Id>,
}
impl Record {
    fn validate(&self, root: &Path, session: &str) -> Result<()> {
        ensure!(
            self.schema == SCHEMA
                && self.session == session
                && hex(session, 48)
                && self.root.is_absolute()
                && crate::native_session::same_path(&self.root.to_string_lossy(), root)
                && self.target == TARGET
                && self.staged == format!(".2k-launcher-camera-{session}.tmp")
                && (64..=MAX_BYTES).contains(&self.size)
                && hex(&self.sha256, 64)
                && self.publishing == self.file_identity.is_some(),
            "Camera localization receipt ownership/schema differs; left untouched"
        );
        Ok(())
    }
}
fn read_receipt(file: &mut File) -> Result<Record> {
    let data = bytes(file, RECEIPT_LIMIT)?;
    let mut records = Vec::new();
    for record in serde_json::Deserializer::from_slice(&data).into_iter::<Record>() {
        ensure!(
            records.len() < 2,
            "Camera localization receipt has extra records; left untouched"
        );
        records.push(record?);
    }
    ensure!(
        !records.is_empty() && !records[0].publishing && records[0].file_identity.is_none(),
        "Camera localization receipt lacks its original intent"
    );
    if records.len() == 2 {
        let mut base = records[1].clone();
        ensure!(
            base.publishing && base.file_identity.is_some(),
            "Invalid camera localization publication record"
        );
        base.publishing = false;
        base.file_identity = None;
        ensure!(
            base == records[0],
            "Camera localization publication differs from its original intent"
        );
    }
    Ok(records.pop().unwrap())
}

pub(crate) struct Store {
    portable: PathBuf,
    record: Record,
    receipt_identity: Id,
    _pins: Vec<Directories>,
}
impl Store {
    pub fn root(&self) -> &Path {
        &self.record.root
    }
    fn marker(&self) -> PathBuf {
        self.portable.join("Data").join(MARKER)
    }
    fn stage(&self) -> PathBuf {
        self.root().join("loc").join(&self.record.staged)
    }
    fn target(&self) -> PathBuf {
        self.root().join(TARGET)
    }
    fn directories(&self) -> Result<()> {
        ensure!(
            self.record
                .portable_identity
                .matches(&dir_id(&self.portable)?)
                && self
                    .record
                    .data_identity
                    .matches(&dir_id(&self.portable.join("Data"))?)
                && self.record.root_identity.matches(&dir_id(self.root())?)
                && self
                    .record
                    .loc_identity
                    .matches(&dir_id(&self.root().join("loc"))?),
            "Camera localization directory ownership changed; left untouched"
        );
        Ok(())
    }
    fn receipt(&self, write: bool, delete: bool) -> Result<File> {
        let mut file = open_mode(&self.marker(), write, delete)?;
        let current = read_receipt(&mut file)?;
        ensure!(
            identity(&file)? == self.receipt_identity && current == self.record,
            "Camera localization receipt changed; left untouched"
        );
        Ok(file)
    }
    fn append_publication(&mut self, receipt: &mut File, next: Record) -> Result<()> {
        let mut base = next.clone();
        base.publishing = false;
        base.file_identity = None;
        ensure!(
            !self.record.publishing
                && next.publishing
                && next.file_identity.is_some()
                && base == self.record,
            "Camera localization publication intent differs"
        );
        ensure!(
            identity(receipt)? == self.receipt_identity && read_receipt(receipt)? == self.record,
            "Camera localization receipt changed before publication"
        );
        let mut append = vec![b'\n'];
        append.extend(serde_json::to_vec(&next)?);
        append.push(b'\n');
        ensure!(
            receipt.metadata()?.len() + append.len() as u64 <= RECEIPT_LIMIT as u64,
            "Camera localization publication receipt exceeds bounds"
        );
        // Retain this exact deny-write/delete handle. No path-based replacement
        // can clobber a receipt that appeared after verification. A torn append
        // is deliberately untrusted and preserved by recovery.
        receipt.seek(SeekFrom::End(0))?;
        receipt.write_all(&append)?;
        receipt.sync_all()?;
        ensure!(
            read_receipt(receipt)? == next,
            "Camera localization publication intent readback failed"
        );
        self.record = next;
        Ok(())
    }
    fn check_prepared(&self, prepared: &[u8]) -> Result<()> {
        ensure!(
            prepared.len() == self.record.size
                && loader_files::digest(prepared) == self.record.sha256,
            "Prepared camera localization differs from its durable intent"
        );
        Ok(())
    }
    fn verify_file(&self, file: &mut File) -> Result<()> {
        ensure!(
            self.record
                .file_identity
                .as_ref()
                .is_some_and(|id| identity(file).is_ok_and(|actual| id.matches(&actual))),
            "Camera localization file identity changed/unrecorded; left untouched"
        );
        let data = bytes(file, MAX_BYTES)?;
        ensure!(
            data.len() == self.record.size && loader_files::digest(&data) == self.record.sha256,
            "Camera localization bytes changed; left untouched"
        );
        Ok(())
    }
    pub fn create(
        portable: &Path,
        selected_root: &Path,
        session: &str,
        prepared: &[u8],
        closed: impl Fn() -> Result<()>,
    ) -> Result<Self> {
        closed()?;
        ensure!(
            hex(session, 48) && (64..=MAX_BYTES).contains(&prepared.len()),
            "Invalid camera localization session/data size"
        );
        let mut pins = vec![
            Directories::pin(portable)?,
            Directories::pin(&portable.join("Data"))?,
            Directories::pin(selected_root)?,
        ];
        ensure!(
            missing(&portable.join("Data").join(MARKER))?,
            "Camera localization recovery is pending"
        );
        let root = selected_root.canonicalize()?;
        let loc = root.join("loc");
        closed()?;
        if missing(&loc)? {
            fs::create_dir(&loc)?;
        }
        pins.push(Directories::pin(&loc)?);
        let record = Record {
            schema: SCHEMA.into(),
            session: session.into(),
            portable_identity: dir_id(portable)?,
            data_identity: dir_id(&portable.join("Data"))?,
            root_identity: dir_id(&root)?,
            loc_identity: dir_id(&loc)?,
            root,
            target: TARGET.into(),
            staged: format!(".2k-launcher-camera-{session}.tmp"),
            size: prepared.len(),
            sha256: loader_files::digest(prepared),
            publishing: false,
            file_identity: None,
        };
        record.validate(&record.root, session)?;
        ensure!(
            missing(&record.root.join(TARGET))?,
            "Existing loc/english.iff mod was left untouched; camera labels require an absent file"
        );
        ensure!(
            missing(&loc.join(&record.staged))?,
            "Camera localization stage already exists; left untouched"
        );
        closed()?;
        let marker = portable.join("Data").join(MARKER);
        backend::atomic_json_new(&marker, &record)?;
        let receipt_identity = identity(&open(&marker, false)?)?;
        Ok(Self {
            portable: portable.into(),
            record,
            receipt_identity,
            _pins: pins,
        })
    }
    pub fn activate(&mut self, prepared: &[u8], closed: impl Fn() -> Result<()>) -> Result<()> {
        closed()?;
        ensure!(
            !self.record.publishing,
            "Camera localization publication already began"
        );
        self.check_prepared(prepared)?;
        self.directories()?;
        let mut receipt = self.receipt(true, false)?;
        ensure!(
            missing(&self.target())? && missing(&self.stage())?,
            "Camera localization destination/stage appeared; left untouched"
        );
        closed()?;
        let mut stage = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .access_mode(0x80000000 | 0x40000000 | 0x10000)
            .create_new(true)
            .share_mode(1)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(self.stage())?;
        // A crash before the identity commit deliberately holds this unknown
        // stage. Hash-only adoption could remove a user's replacement file.
        let preparation = (|| -> Result<_> {
            closed()?;
            stage.write_all(prepared)?;
            closed()?;
            stage.sync_all()?;
            ordinary(&stage, false)?;
            let mut next = self.record.clone();
            next.file_identity = Some(identity(&stage)?);
            next.publishing = true;
            closed()?;
            self.directories()?;
            Ok(next)
        })();
        let next = match preparation {
            Ok(next) => next,
            Err(error) => {
                // Only this retained create_new handle grants cleanup. Unknown
                // crash stages still require the existing recorded identity.
                return match loader_files::unlink(&stage) {
                    Ok(()) => Err(error),
                    Err(cleanup) => Err(anyhow::anyhow!(
                        "{error:#}; owned camera stage cleanup failed: {cleanup:#}"
                    )),
                };
            }
        };
        self.append_publication(&mut receipt, next)?;
        self.verify_file(&mut stage)?;
        closed()?;
        self.directories()?;
        ensure!(
            identity(&receipt)? == self.receipt_identity
                && read_receipt(&mut receipt)? == self.record,
            "Camera localization receipt changed before rename"
        );
        ensure!(
            missing(&self.target())?,
            "loc/english.iff appeared; existing mod left untouched"
        );
        closed()?;
        // Canonicalizing the still-pinned parent supplies a verbatim Windows
        // destination; long portable/mod paths must not fall back to MAX_PATH.
        let target = self.root().join("loc").canonicalize()?.join("english.iff");
        loader_files::publish(&stage, &target)?;
        drop(stage);
        drop(receipt);
        self.verify_prepared(prepared)
    }
    pub fn verify_prepared(&self, prepared: &[u8]) -> Result<()> {
        self.check_prepared(prepared)?;
        self.verify_current()
    }
    /// Recheck after potentially long selector preparation, immediately before
    /// dispatch. Expected identities/hash come only from this retained lease.
    pub fn verify_current(&self) -> Result<()> {
        ensure!(
            self.record.publishing,
            "Camera localization has not been published"
        );
        self.record.validate(self.root(), &self.record.session)?;
        self.directories()?;
        drop(self.receipt(false, false)?);
        ensure!(
            missing(&self.stage())?,
            "Camera localization stage remains; publication incomplete"
        );
        self.verify_file(&mut open(&self.target(), false)?)
    }
    /// Recover root authority only from this bounded exact-session receipt,
    /// followed by its existing actual directory-identity validation.
    pub(crate) fn recorded_root(portable: &Path, session: &str) -> Result<Option<PathBuf>> {
        let _portable = Directories::pin(portable)?;
        let _data = Directories::pin(&portable.join("Data"))?;
        let marker = portable.join("Data").join(MARKER);
        if missing(&marker)? {
            return Ok(None);
        }
        let record = read_receipt(&mut open(&marker, false)?)?;
        let store = Self::recover(portable, &record.root, session)?;
        Ok(store.map(|s| s.record.root.clone()))
    }
    pub fn recover(portable: &Path, selected_root: &Path, session: &str) -> Result<Option<Self>> {
        let mut pins = vec![
            Directories::pin(portable)?,
            Directories::pin(&portable.join("Data"))?,
        ];
        let marker = portable.join("Data").join(MARKER);
        if missing(&marker)? {
            return Ok(None);
        }
        // A restored profile may have removed an originally absent game/mods
        // junction already. No camera receipt means no selected-root authority
        // or filesystem traversal is needed merely to clear a finished launch.
        pins.push(Directories::pin(selected_root)?);
        let mut file = open(&marker, false)?;
        let receipt_identity = identity(&file)?;
        let record = read_receipt(&mut file)?;
        record.validate(selected_root, session)?;
        pins.push(Directories::pin(&selected_root.join("loc"))?);
        let store = Self {
            portable: portable.into(),
            record,
            receipt_identity,
            _pins: pins,
        };
        store.directories()?;
        Ok(Some(store))
    }
    pub fn restore(&mut self, closed: impl Fn() -> Result<()>) -> Result<()> {
        closed()?;
        self.record.validate(self.root(), &self.record.session)?;
        self.directories()?;
        // Hold the validated receipt against writes/deletion throughout cleanup.
        let marker = self.receipt(false, true)?;
        let stage_present = !missing(&self.stage())?;
        let target_present = !missing(&self.target())?;
        let mut owned = None;
        if self.record.publishing {
            ensure!(
                !(stage_present && target_present),
                "Both camera stage and destination exist; conflict left untouched"
            );
            let path = if stage_present {
                Some(self.stage())
            } else if target_present {
                Some(self.target())
            } else {
                None
            };
            if let Some(path) = path {
                let mut file = open(&path, true)?;
                self.verify_file(&mut file)?;
                owned = Some((path, file));
            }
        } else {
            ensure!(
                !stage_present,
                "Unrecorded interrupted camera stage was preserved; recovery needs review"
            );
            // No publication began. A later foreign target is never owned.
        }
        if let Some((path, mut file)) = owned {
            closed()?;
            self.verify_file(&mut file)?;
            closed()?;
            loader_files::unlink(&file)?;
            drop(file);
            ensure!(
                missing(&path)?,
                "Camera localization removal pending; receipt retained"
            );
        }
        closed()?;
        loader_files::unlink(&marker)?;
        drop(marker);
        ensure!(
            missing(&self.marker())?,
            "Camera localization receipt removal pending"
        );
        Ok(())
    }
}

#[cfg(test)]
#[path = "camera_name_files_tests.rs"]
mod tests;
