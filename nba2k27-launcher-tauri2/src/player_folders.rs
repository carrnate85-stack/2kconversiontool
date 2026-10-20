//! Session-owned NTFS links for mods/players/<player> and char/sig/<player>.
//! The original WPF behavior is retained: loose char/sig files win, duplicate
//! packages fail before publication, and no player data is copied. A durable
//! intent plus nonce-named staged links supports conditional crash recovery.
#![allow(dead_code)]
use crate::{
    backend,
    loader_files::{self, Directories},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Read,
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Path, PathBuf},
};
use windows::Win32::{Foundation::HANDLE, Storage::FileSystem::*};

pub(crate) const MARKER: &str = "player-folders-session.json";
const SCHEMA: &str = "nba2k27-rust-player-folders-v1";
const LIMIT: usize = 10_000;
const RECEIPT_LIMIT: u64 = 8 * 1024 * 1024;
// Read-only discovery/prevalidation may inspect thousands of pinned files.
// Keep cancellation bounded without taking a full process snapshot per file.
// This is NOT a write authorization cache: every hard-link, rename and receipt
// publication still receives a fresh closed() immediately before mutation.
const READ_CHECKPOINT: usize = 64;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FileId {
    volume: u32,
    index: u64,
    birth: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stable: Option<crate::file_identity::Stable>,
}
impl FileId {
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
fn identity(file: &File) -> Result<FileId> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    unsafe {
        GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info)?;
    }
    Ok(FileId {
        volume: info.dwVolumeSerialNumber,
        index: ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64,
        birth: ((info.ftCreationTime.dwHighDateTime as u64) << 32)
            | info.ftCreationTime.dwLowDateTime as u64,
        stable: Some(crate::file_identity::Stable::read(file)?),
    })
}
fn ordinary(meta: &fs::Metadata) -> bool {
    meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 == 0
}
fn missing(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(e.into()),
    }
}
fn component(s: &str) -> bool {
    !s.is_empty()
        && s.encode_utf16().count() <= 255
        && s != "."
        && s != ".."
        && !s.ends_with(['.', ' '])
        && !s.contains(['/', '\\', ':', '<', '>', '"', '|', '?', '*'])
        && !s.chars().any(char::is_control)
}
fn iff(s: &str) -> bool {
    component(s) && s.to_ascii_lowercase().ends_with(".iff")
}
fn open_file(path: &Path, delete: bool) -> Result<File> {
    let file = fs::OpenOptions::new()
        .read(true)
        .access_mode(0x80000000 | if delete { 0x10000 } else { 0 })
        .share_mode(1)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)?;
    ensure!(
        file.metadata()?.is_file() && ordinary(&file.metadata()?),
        "Player-folder file is no longer an ordinary file"
    );
    Ok(file)
}
fn directory_id(path: &Path) -> Result<FileId> {
    let file = fs::OpenOptions::new()
        .read(true)
        .share_mode(3)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)?;
    ensure!(
        file.metadata()?.is_dir() && ordinary(&file.metadata()?),
        "Player-folder directory was replaced or linked"
    );
    identity(&file)
}
fn read_record(file: &mut File) -> Result<Record> {
    ensure!(
        file.metadata()?.len() <= RECEIPT_LIMIT,
        "Player-folder receipt exceeds bounds"
    );
    let mut bytes = Vec::new();
    file.take(RECEIPT_LIMIT + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= RECEIPT_LIMIT,
        "Player-folder receipt exceeds bounds"
    );
    Ok(serde_json::from_slice(&bytes)?)
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Entry {
    name: String,
    source: String,
    staged: String,
    identity: FileId,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    schema: String,
    session: String,
    root: PathBuf,
    root_identity: FileId,
    sig_identity: FileId,
    publish_started: bool,
    entries: Vec<Entry>,
}
impl Record {
    fn validate(&self, expected: &Path, session: &str) -> Result<()> {
        ensure!(
            self.schema == SCHEMA
                && self.session == session
                && session.len() == 48
                && session.bytes().all(|b| b.is_ascii_hexdigit())
                && self.root.is_absolute()
                && crate::native_session::same_path(&self.root.to_string_lossy(), expected)
                && (1..=LIMIT).contains(&self.entries.len()),
            "Player-folder receipt ownership differs"
        );
        let mut names = BTreeSet::new();
        for (index, entry) in self.entries.iter().enumerate() {
            let parts = entry.source.split('/').collect::<Vec<_>>();
            let valid = match parts.as_slice() {
                [root, folder, leaf] => {
                    root.eq_ignore_ascii_case("players")
                        && component(folder)
                        && leaf.eq_ignore_ascii_case(&entry.name)
                }
                [root, sig, folder, leaf] => {
                    root.eq_ignore_ascii_case("char")
                        && sig.eq_ignore_ascii_case("sig")
                        && component(folder)
                        && leaf.eq_ignore_ascii_case(&entry.name)
                }
                _ => false,
            };
            ensure!(
                iff(&entry.name)
                    && valid
                    && parts.iter().all(|s| component(s))
                    && names.insert(entry.name.to_ascii_lowercase())
                    && entry.staged == format!(".2k-launcher-player-{session}-{index}.tmp"),
                "Invalid player-folder receipt path"
            );
        }
        Ok(())
    }
}
pub(crate) struct Store {
    portable: PathBuf,
    record: Record,
    _pins: Vec<Directories>,
    pub loose_priority: usize,
}
impl Store {
    fn marker(&self) -> PathBuf {
        self.portable.join("Data").join(MARKER)
    }
    fn sig(&self) -> PathBuf {
        self.record.root.join("char/sig")
    }
    pub fn root(&self) -> &Path {
        &self.record.root
    }
    pub fn linked(&self) -> usize {
        self.record.entries.len()
    }
    pub fn routes(&self) -> Vec<(String, String)> {
        self.record
            .entries
            .iter()
            .map(|e| (format!("char/sig/{}", e.name), e.source.clone()))
            .collect()
    }
    pub fn verify_routes(&self) -> Result<()> {
        ensure!(
            self.record.publish_started,
            "Player-folder publication was not committed"
        );
        for entry in &self.record.entries {
            ensure!(
                entry.identity.matches(&identity(&open_file(
                    &self.sig().join(&entry.name),
                    false
                )?)?),
                "Published player-folder file was replaced before indexing"
            );
        }
        Ok(())
    }
    fn commit(&self) -> Result<()> {
        ensure!(
            serde_json::to_vec(&self.record)?.len() as u64 <= RECEIPT_LIMIT,
            "Player-folder receipt exceeds bounds"
        );
        backend::atomic_json(&self.marker(), &self.record)
    }
    pub fn create(
        portable: &Path,
        mods: &Path,
        session: &str,
        closed: impl Fn() -> Result<()>,
    ) -> Result<Option<Self>> {
        closed()?;
        let mut pins = vec![
            Directories::pin(portable)?,
            Directories::pin(&portable.join("Data"))?,
            Directories::pin(mods)?,
        ];
        let root = mods.canonicalize()?;
        ensure!(
            missing(&portable.join("Data").join(MARKER))?,
            "Player-folder recovery is pending"
        );
        let sig = root.join("char/sig");
        let mut loose = BTreeSet::new();
        let mut roots = Vec::new();
        if !missing(&root.join("char"))? {
            pins.push(Directories::pin(&root.join("char"))?);
        }
        if !missing(&sig)? {
            pins.push(Directories::pin(&sig)?);
            for entry in fs::read_dir(&sig)? {
                let entry = entry?;
                if !entry.file_type()?.is_dir() {
                    loose.insert(entry.file_name().to_string_lossy().to_ascii_lowercase());
                }
            }
            roots.push((sig.clone(), "char/sig"));
        }
        let players = root.join("players");
        if !missing(&players)? {
            pins.push(Directories::pin(&players)?);
            roots.push((players, "players"));
        }
        let mut candidates = BTreeMap::new();
        let mut loose_priority = 0;
        let mut inspected = 0usize;
        for (source, logical) in roots {
            let mut folders = fs::read_dir(&source)?.collect::<std::io::Result<Vec<_>>>()?;
            folders.sort_by_key(|e| e.file_name());
            for folder in folders {
                let meta = fs::symlink_metadata(folder.path())?;
                if !meta.is_dir() || !ordinary(&meta) {
                    continue;
                }
                pins.push(Directories::pin(&folder.path())?);
                let label = folder
                    .file_name()
                    .to_str()
                    .context("Non-Unicode player-folder name")?
                    .to_owned();
                ensure!(component(&label), "Unsupported player-folder name");
                for entry in fs::read_dir(folder.path())? {
                    inspected += 1;
                    if inspected.is_multiple_of(READ_CHECKPOINT) {
                        closed()?;
                    }
                    let entry = entry?;
                    let meta = fs::symlink_metadata(entry.path())?;
                    if !meta.is_file() || !ordinary(&meta) {
                        continue;
                    }
                    let name = entry
                        .file_name()
                        .to_str()
                        .context("Non-Unicode player asset name")?
                        .to_owned();
                    if !name.to_ascii_lowercase().ends_with(".iff") {
                        continue;
                    }
                    ensure!(iff(&name), "Unsafe player-folder IFF filename");
                    let key = name.to_ascii_lowercase();
                    if loose.contains(&key) {
                        loose_priority += 1;
                        continue;
                    }
                    ensure!(
                        !candidates.contains_key(&key),
                        "Player-folder conflict: {name} exists in multiple packages; keep one or put the preferred file loose in char/sig"
                    );
                    ensure!(
                        candidates.len() < LIMIT,
                        "Player folders exceed the 10,000-file limit"
                    );
                    let file = open_file(&entry.path(), false)?;
                    candidates.insert(
                        key,
                        (
                            name.clone(),
                            format!("{logical}/{label}/{name}"),
                            identity(&file)?,
                        ),
                    );
                }
            }
        }
        if candidates.is_empty() {
            return Ok(None);
        }
        for directory in [root.join("char"), sig.clone()] {
            closed()?;
            if missing(&directory)? {
                fs::create_dir(&directory)?;
            }
            pins.push(Directories::pin(&directory)?);
        }
        let record = Record {
            schema: SCHEMA.into(),
            session: session.into(),
            root_identity: directory_id(&root)?,
            sig_identity: directory_id(&sig)?,
            root,
            publish_started: false,
            entries: candidates
                .into_values()
                .enumerate()
                .map(|(index, (name, source, identity))| Entry {
                    name,
                    source,
                    identity,
                    staged: format!(".2k-launcher-player-{session}-{index}.tmp"),
                })
                .collect(),
        };
        let store = Self {
            portable: portable.into(),
            record,
            _pins: pins,
            loose_priority,
        };
        store.record.validate(store.root(), session)?;
        ensure!(
            serde_json::to_vec(&store.record)?.len() as u64 <= RECEIPT_LIMIT,
            "Player-folder receipt exceeds bounds"
        );
        closed()?;
        backend::atomic_json_new(&store.marker(), &store.record)?;
        Ok(Some(store))
    }
    pub fn activate(&mut self, closed: impl Fn() -> Result<()>) -> Result<()> {
        ensure!(
            !self.record.publish_started,
            "Player links were already activated"
        );
        let mut files = Vec::new();
        for (index, entry) in self.record.entries.iter().enumerate() {
            if index.is_multiple_of(READ_CHECKPOINT) {
                closed()?;
            }
            let source = self.record.root.join(&entry.source);
            let file = open_file(&source, false)?;
            ensure!(
                entry.identity.matches(&identity(&file)?)
                    && missing(&self.sig().join(&entry.name))?
                    && missing(&self.sig().join(&entry.staged))?,
                "Player-folder source/destination changed before staging"
            );
            files.push(file);
        }
        // Deny source write/delete and keep every ancestor pinned while making
        // nonce-named links. Never make a data copy or overwrite a loose mod.
        for entry in &self.record.entries {
            closed()?;
            fs::hard_link(
                self.record.root.join(&entry.source),
                self.sig().join(&entry.staged),
            )?;
            ensure!(
                entry.identity.matches(&identity(&open_file(
                    &self.sig().join(&entry.staged),
                    false
                )?)?),
                "Staged player link identity differs"
            );
        }
        drop(files);
        closed()?;
        self.record.publish_started = true;
        self.commit()?;
        for entry in &self.record.entries {
            let file = open_file(&self.sig().join(&entry.staged), true)?;
            ensure!(
                entry.identity.matches(&identity(&file)?),
                "Staged player file was replaced"
            );
            closed()?;
            loader_files::publish(&file, &self.sig().join(&entry.name))?;
        }
        Ok(())
    }
    pub fn recover(portable: &Path, mods: &Path, session: &str) -> Result<Option<Self>> {
        let marker = portable.join("Data").join(MARKER);
        if missing(&marker)? {
            return Ok(None);
        }
        let pins = vec![
            Directories::pin(portable)?,
            Directories::pin(&portable.join("Data"))?,
            Directories::pin(mods)?,
            Directories::pin(&mods.join("char/sig"))?,
        ];
        let record = read_record(&mut open_file(&marker, false)?)?;
        record.validate(mods, session)?;
        ensure!(
            record.root_identity.matches(&directory_id(mods)?)
                && record
                    .sig_identity
                    .matches(&directory_id(&mods.join("char/sig"))?),
            "Player-folder recovery directory ownership changed"
        );
        Ok(Some(Self {
            portable: portable.into(),
            record,
            _pins: pins,
            loose_priority: 0,
        }))
    }
    pub fn restore(&mut self, closed: impl Fn() -> Result<()>) -> Result<()> {
        closed()?;
        self.record.validate(self.root(), &self.record.session)?;
        ensure!(
            self.record
                .root_identity
                .matches(&directory_id(self.root())?)
                && self
                    .record
                    .sig_identity
                    .matches(&directory_id(&self.sig())?),
            "Player-folder directory ownership changed"
        );
        let mut held = false;
        for entry in &self.record.entries {
            let staged = self.sig().join(&entry.staged);
            let staged_exists = !missing(&staged)?;
            // A staged link still present means the atomic no-replace rename
            // did not consume it. An identical loose user link is NOT ours.
            let target = if staged_exists {
                Some(staged)
            } else if self.record.publish_started {
                Some(self.sig().join(&entry.name))
            } else {
                None
            };
            let Some(target) = target else {
                continue;
            };
            if missing(&target)? {
                continue;
            }
            let meta = fs::symlink_metadata(&target)?;
            if !meta.is_file() || !ordinary(&meta) {
                ensure!(!staged_exists, "Foreign staged player file left untouched");
                continue;
            }
            let file = open_file(&target, true)?;
            if !entry.identity.matches(&identity(&file)?) {
                ensure!(!staged_exists, "Foreign staged player file left untouched");
                continue;
            }
            // Never delete what became the last copy after a user removed or
            // replaced the original source. Keep both receipt and data for review.
            let source = self.record.root.join(&entry.source);
            let _source_pin = Directories::pin(source.parent().unwrap())?;
            let Ok(source_meta) = fs::symlink_metadata(&source) else {
                held = true;
                continue;
            };
            if !source_meta.is_file() || !ordinary(&source_meta) {
                held = true;
                continue;
            }
            let Ok(source) = fs::OpenOptions::new()
                .read(true)
                .share_mode(1 | 4)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
                .open(&source)
            else {
                held = true;
                continue;
            };
            if !entry.identity.matches(&identity(&source)?) {
                held = true;
                continue;
            }
            closed()?;
            loader_files::unlink(&file)?;
        }
        if held {
            // Other owned links have still been restored. Retain the receipt
            // and the last-copy links so a later recovery can finish safely.
            return Ok(());
        }
        let mut marker = open_file(&self.marker(), true)?;
        ensure!(
            read_record(&mut marker)? == self.record,
            "Player-folder receipt changed; left untouched"
        );
        closed()?;
        loader_files::unlink(&marker)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture {
        base: PathBuf,
        portable: PathBuf,
        mods: PathBuf,
        session: String,
    }
    impl Fixture {
        fn new() -> Self {
            let base =
                std::env::temp_dir().join(format!("rust-player-folders-{}", backend::unique_id()));
            let portable = base.join("portable");
            let mods = base.join("mods");
            fs::create_dir_all(portable.join("Data")).unwrap();
            fs::create_dir(&mods).unwrap();
            Self {
                base,
                portable,
                mods,
                session: backend::unique_id(),
            }
        }
        fn write(&self, name: &str, bytes: &[u8]) {
            let file = self.mods.join(name);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, bytes).unwrap();
        }
        fn store(&self) -> Option<Store> {
            Store::create(&self.portable, &self.mods, &self.session, || Ok(())).unwrap()
        }
        fn recover(&self) -> Result<Option<Store>> {
            Store::recover(&self.portable, &self.mods, &self.session)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }
    #[test]
    fn both_player_folder_layouts_publish_shared_data_and_restore_without_copies() {
        let f = Fixture::new();
        f.write("players/Amir Johnson/png1452.iff", b"player");
        f.write("char/sig/Legacy/body.iff", b"body");
        f.write("players/Amir Johnson/nested/ignore.iff", b"nested");
        f.write("players/Amir Johnson/readme.txt", b"readme");
        let mut store = f.store().unwrap();
        assert_eq!(store.linked(), 2);
        store.activate(|| Ok(())).unwrap();
        for (target, source) in store.routes() {
            assert_eq!(
                identity(&open_file(&f.mods.join(&target), false).unwrap()).unwrap(),
                identity(&open_file(&f.mods.join(&source), false).unwrap()).unwrap()
            );
        }
        store.restore(|| Ok(())).unwrap();
        assert!(!f.mods.join("char/sig/png1452.iff").exists());
        assert!(!f.mods.join("char/sig/body.iff").exists());
        assert_eq!(
            fs::read(f.mods.join("players/Amir Johnson/png1452.iff")).unwrap(),
            b"player"
        );
        assert!(!f.portable.join("Data").join(MARKER).exists());
    }
    #[test]
    fn loose_overrides_win_and_duplicate_packages_fail_before_journal() {
        let f = Fixture::new();
        f.write("players/One/png1.iff", b"one");
        f.write("char/sig/Two/PNG1.iff", b"two");
        assert!(Store::create(&f.portable, &f.mods, &f.session, || Ok(())).is_err());
        assert!(!f.portable.join("Data").join(MARKER).exists());
        f.write("char/sig/png1.iff", b"preferred");
        assert!(f.store().is_none());
        assert_eq!(
            fs::read(f.mods.join("char/sig/png1.iff")).unwrap(),
            b"preferred"
        );
    }
    #[test]
    fn game_guard_and_interrupted_publication_are_recoverable() {
        let f = Fixture::new();
        f.write("Players/Amir/png1452.iff", b"player");
        assert!(
            Store::create(&f.portable, &f.mods, &f.session, || anyhow::bail!(
                "game running"
            ))
            .is_err()
        );
        let mut store = f.store().unwrap();
        store.activate(|| Ok(())).unwrap();
        assert!(store.restore(|| anyhow::bail!("game running")).is_err());
        assert!(f.mods.join("char/sig/png1452.iff").exists());
        drop(store);
        let mut recovered = f.recover().unwrap().unwrap();
        recovered.restore(|| Ok(())).unwrap();
        assert!(!f.mods.join("char/sig/png1452.iff").exists());
    }
    #[test]
    fn staged_same_identity_loose_race_never_deletes_the_foreign_loose_link() {
        let f = Fixture::new();
        f.write("players/Amir/png1452.iff", b"player");
        let mut store = f.store().unwrap();
        let entry = store.record.entries[0].clone();
        fs::hard_link(f.mods.join(&entry.source), store.sig().join(&entry.staged)).unwrap();
        fs::hard_link(f.mods.join(&entry.source), store.sig().join(&entry.name)).unwrap();
        store.record.publish_started = true;
        store.commit().unwrap();
        store.restore(|| Ok(())).unwrap();
        assert!(store.sig().join(&entry.name).exists());
        assert!(!store.sig().join(&entry.staged).exists());
    }
    #[test]
    fn replaced_loose_files_survive_cleanup_and_missing_sources_hold_the_last_copy() {
        let f = Fixture::new();
        f.write("players/Amir/png1452.iff", b"player");
        let mut store = f.store().unwrap();
        store.activate(|| Ok(())).unwrap();
        fs::remove_file(f.mods.join("char/sig/png1452.iff")).unwrap();
        f.write("char/sig/png1452.iff", b"user replacement");
        store.restore(|| Ok(())).unwrap();
        assert_eq!(
            fs::read(f.mods.join("char/sig/png1452.iff")).unwrap(),
            b"user replacement"
        );
        drop(store);
        let f = Fixture::new();
        f.write("players/Amir/png1452.iff", b"last copy");
        f.write("players/Amir/other.iff", b"independent");
        let mut store = f.store().unwrap();
        store.activate(|| Ok(())).unwrap();
        fs::remove_file(f.mods.join("players/Amir/png1452.iff")).unwrap();
        store.restore(|| Ok(())).unwrap();
        assert!(store.marker().exists());
        assert_eq!(
            fs::read(f.mods.join("char/sig/png1452.iff")).unwrap(),
            b"last copy"
        );
        assert!(!f.mods.join("char/sig/other.iff").exists());
    }
    #[test]
    fn tampered_receipt_foreign_root_and_junction_sources_are_refused() {
        let f = Fixture::new();
        f.write("players/Amir/png1452.iff", b"player");
        let store = f.store().unwrap();
        let mut bad = store.record.clone();
        bad.entries[0].source = "../outside.iff".into();
        backend::atomic_json(&store.marker(), &bad).unwrap();
        drop(store);
        assert!(f.recover().is_err());
        let f = Fixture::new();
        fs::create_dir(f.base.join("outside")).unwrap();
        crate::mod_profile::fixture_junction(&f.mods.join("players"), &f.base.join("outside"))
            .unwrap();
        assert!(Store::create(&f.portable, &f.mods, &f.session, || Ok(())).is_err());
    }
    #[test]
    fn planned_staged_and_half_published_batches_restore_only_owned_entries() {
        for phase in 0..3 {
            let f = Fixture::new();
            f.write("players/Amir/a.iff", b"a");
            f.write("players/Amir/b.iff", b"b");
            let mut store = f.store().unwrap();
            if phase > 0 {
                for entry in &store.record.entries {
                    fs::hard_link(f.mods.join(&entry.source), store.sig().join(&entry.staged))
                        .unwrap();
                }
            }
            if phase == 2 {
                store.record.publish_started = true;
                store.commit().unwrap();
                let entry = &store.record.entries[0];
                fs::rename(
                    store.sig().join(&entry.staged),
                    store.sig().join(&entry.name),
                )
                .unwrap();
            }
            drop(store);
            let mut recovered = f.recover().unwrap().unwrap();
            recovered.restore(|| Ok(())).unwrap();
            assert_eq!(fs::read_dir(recovered.sig()).unwrap().count(), 0);
            assert_eq!(fs::read(f.mods.join("players/Amir/a.iff")).unwrap(), b"a");
        }
    }
    #[test]
    fn alternate_profile_intent_retains_the_player_recovery_root_before_and_after_activation() {
        for active in [false, true] {
            let f = Fixture::new();
            f.write("players/Amir/png1452.iff", b"player");
            let game = f.base.join("game");
            fs::create_dir_all(game.join("mods")).unwrap();
            fs::write(game.join("mods/original.iff"), b"original").unwrap();
            let mut profile =
                crate::mod_profile::Store::create(&f.portable, &game, &f.mods, &f.session, || {
                    Ok(())
                })
                .unwrap()
                .unwrap();
            let mut players = f.store().unwrap();
            players.activate(|| Ok(())).unwrap();
            if active {
                profile.activate(|| Ok(())).unwrap();
            }
            drop(players);
            drop(profile);
            let mut profile = crate::mod_profile::Store::recover(&f.portable, &game)
                .unwrap()
                .unwrap();
            let mut players = Store::recover(&f.portable, profile.selected_root(), &f.session)
                .unwrap()
                .unwrap();
            players.restore(|| Ok(())).unwrap();
            drop(players);
            profile.restore(|| Ok(())).unwrap();
            assert_eq!(
                fs::read(game.join("mods/original.iff")).unwrap(),
                b"original"
            );
            assert_eq!(
                fs::read(f.mods.join("players/Amir/png1452.iff")).unwrap(),
                b"player"
            );
            assert!(!f.mods.join("char/sig/png1452.iff").exists());
        }
    }
    #[test]
    fn repeated_fresh_player_sources_with_same_loose_name_keep_stable_ownership() {
        let f = Fixture::new();
        for round in 0..12 {
            if round > 0 {
                fs::remove_file(
                    f.mods
                        .join(format!("players/Package{}/png1452.iff", round - 1)),
                )
                .unwrap();
            }
            let name = format!("players/Package{round}/png1452.iff");
            f.write(&name, b"player");
            let session = backend::unique_id();
            let mut store = Store::create(&f.portable, &f.mods, &session, || Ok(()))
                .unwrap()
                .unwrap();
            store
                .activate(|| Ok(()))
                .unwrap_or_else(|e| panic!("Player source round {round}: {e:#}"));
            store
                .verify_routes()
                .unwrap_or_else(|e| panic!("Player source round {round}: {e:#}"));
            drop(store);
            let mut recovered = Store::recover(&f.portable, &f.mods, &session)
                .unwrap()
                .unwrap();
            recovered.verify_routes().unwrap();
            recovered.restore(|| Ok(())).unwrap();
            drop(recovered);
            assert!(!f.mods.join("char/sig/png1452.iff").exists());
            assert_eq!(fs::read(f.mods.join(name)).unwrap(), b"player");
        }
    }
    #[test]
    fn legacy_player_receipts_remain_strict_and_recover_without_stable_fields() {
        let f = Fixture::new();
        f.write("players/Amir/png1452.iff", b"player");
        let mut store = f.store().unwrap();
        store.activate(|| Ok(())).unwrap();
        let expected = store.record.entries[0].identity.clone();
        let mut metadata_only = expected.clone();
        metadata_only.birth ^= 1;
        assert!(expected.matches(&metadata_only));
        let mut legacy = expected.clone();
        legacy.stable = None;
        assert!(legacy.matches(&expected));
        assert!(!legacy.matches(&metadata_only));
        assert!(!expected.matches(&legacy));
        let mut foreign = legacy.clone();
        foreign.index ^= 1;
        assert!(!legacy.matches(&foreign));
        let mut foreign = legacy.clone();
        foreign.volume ^= 1;
        assert!(!legacy.matches(&foreign));
        store.record.root_identity.stable = None;
        store.record.sig_identity.stable = None;
        for entry in &mut store.record.entries {
            entry.identity.stable = None;
        }
        assert!(
            !serde_json::to_string(&store.record)
                .unwrap()
                .contains("stable")
        );
        store.commit().unwrap();
        drop(store);
        let mut recovered = f.recover().unwrap().unwrap();
        recovered.verify_routes().unwrap();
        recovered.restore(|| Ok(())).unwrap();
    }
    #[test]
    fn same_bytes_in_a_foreign_file_never_replace_owned_player_identity() {
        let f = Fixture::new();
        f.write("players/Amir/png1452.iff", b"player");
        let mut store = f.store().unwrap();
        store.activate(|| Ok(())).unwrap();
        let loose = f.mods.join("char/sig/png1452.iff");
        fs::remove_file(&loose).unwrap();
        fs::write(&loose, b"player").unwrap();
        assert!(store.verify_routes().is_err());
        store.restore(|| Ok(())).unwrap();
        assert_eq!(fs::read(&loose).unwrap(), b"player");
        assert_eq!(
            fs::read(f.mods.join("players/Amir/png1452.iff")).unwrap(),
            b"player"
        );
    }
    #[test]
    fn startup_checks_are_bounded_for_readonly_work_but_fresh_for_every_mutation() {
        let f = Fixture::new();
        for i in 0..361 {
            f.write(&format!("players/Player{i:04}/png{i:04}.iff"), b"player");
        }
        let calls = std::cell::Cell::new(0usize);
        let check = || {
            calls.set(calls.get() + 1);
            Ok(())
        };
        let mut s = Store::create(&f.portable, &f.mods, &f.session, check)
            .unwrap()
            .unwrap();
        assert_eq!(calls.get(), 9); // initial, five read checkpoints, two directories, intent
        s.activate(check).unwrap();
        // 361 fresh hard-link guards +361 rename guards +one journal guard,
        // and six bounded source-read checkpoints. No write authority cached.
        assert_eq!(calls.get(), 738);
        s.verify_routes().unwrap();
        let restore_calls = std::cell::Cell::new(0usize);
        s.restore(|| {
            restore_calls.set(restore_calls.get() + 1);
            Ok(())
        })
        .unwrap();
        // One initial proof, one fresh proof immediately before each owned
        // unlink, and one fresh proof before the receipt unlink. Read-only
        // inspection does not repeat the expensive process scan.
        assert_eq!(restore_calls.get(), 363);
        assert_eq!(fs::read_dir(s.sig()).unwrap().count(), 0);
    }
    #[test]
    fn late_process_refusal_prevents_next_staging_journal_or_rename_and_recovers() {
        for fail_phase in 0..3 {
            let f = Fixture::new();
            for name in ["a.iff", "b.iff", "c.iff"] {
                f.write(&format!("players/One/{name}"), b"source");
            }
            let mut s = f.store().unwrap();
            let sig = s.sig();
            let marker = s.marker();
            let error = s
                .activate(|| {
                    let names = fs::read_dir(&sig)?
                        .map(|e| e.map(|e| e.file_name().to_string_lossy().into_owned()))
                        .collect::<std::io::Result<Vec<_>>>()?;
                    let staged = names
                        .iter()
                        .filter(|n| n.starts_with(".2k-launcher-player-"))
                        .count();
                    let published = names.iter().filter(|n| n.ends_with(".iff")).count();
                    let record: Record = serde_json::from_slice(&fs::read(&marker)?)?;
                    ensure!(
                        !match fail_phase {
                            0 => staged == 1 && !record.publish_started,
                            1 => staged == 3 && !record.publish_started,
                            2 => published == 1,
                            _ => unreachable!(),
                        },
                        "New process appeared immediately before the next mutation"
                    );
                    Ok(())
                })
                .unwrap_err();
            assert!(error.to_string().contains("New process"));
            let record: Record = serde_json::from_slice(&fs::read(&marker).unwrap()).unwrap();
            assert_eq!(record.publish_started, fail_phase == 2);
            let published = fs::read_dir(&sig)
                .unwrap()
                .filter(|e| {
                    e.as_ref()
                        .unwrap()
                        .file_name()
                        .to_string_lossy()
                        .ends_with(".iff")
                })
                .count();
            assert_eq!(published, usize::from(fail_phase == 2));
            drop(s);
            let mut recovered = f.recover().unwrap().unwrap();
            recovered.restore(|| Ok(())).unwrap();
            assert!(!marker.exists());
            assert_eq!(fs::read_dir(&sig).unwrap().count(), 0);
            for name in ["a.iff", "b.iff", "c.iff"] {
                assert_eq!(
                    fs::read(f.mods.join(format!("players/One/{name}"))).unwrap(),
                    b"source"
                );
            }
        }
    }
}
