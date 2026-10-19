//! Fresh-bundle-only selector publication. Never adopts/replaces an existing
//! mod. A durable intent and staged file identities precede no-replace renames;
//! restoration is game-closed, exact-session, byte/identity owned and bounded.
#![allow(dead_code)]
use crate::{
    backend,
    loader_files::{self, Directories},
    selector_tables::{PATHS, Prepared},
    sigconfig,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{Read, Write},
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Path, PathBuf},
};
use windows::Win32::{Foundation::HANDLE, Storage::FileSystem::*};
pub(crate) const MARKER: &str = "selector-files-session.json";
const SCHEMA: &str = "nba2k27-rust-generated-selectors-v1";
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
            // Legacy receipts keep their original strict capability. Never
            // retroactively weaken old creation-time/file-index ownership.
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
fn ordinary(meta: &fs::Metadata) -> bool {
    meta.file_attributes() & 0x400 == 0
}
fn missing(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(e.into()),
    }
}
fn open(path: &Path, delete: bool) -> Result<File> {
    let file = fs::OpenOptions::new()
        .read(true)
        .access_mode(0x80000000 | if delete { 0x10000 } else { 0 })
        .share_mode(1)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)?;
    ensure!(
        file.metadata()?.is_file() && ordinary(&file.metadata()?),
        "Generated selector path is not an ordinary file"
    );
    Ok(file)
}
fn dir_id(path: &Path) -> Result<Id> {
    let file = fs::OpenOptions::new()
        .read(true)
        .share_mode(3)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)?;
    ensure!(
        file.metadata()?.is_dir() && ordinary(&file.metadata()?),
        "Selector directory ownership changed"
    );
    identity(&file)
}
fn bytes(file: &mut File, limit: usize) -> Result<Vec<u8>> {
    let before = file.metadata()?;
    ensure!(
        before.len() <= limit as u64,
        "Generated selector/receipt exceeds bounds"
    );
    let mut result = Vec::new();
    file.take((limit + 1) as u64).read_to_end(&mut result)?;
    let after = file.metadata()?;
    ensure!(
        result.len() as u64 == before.len()
            && before.len() == after.len()
            && before.last_write_time() == after.last_write_time(),
        "Generated selector changed during verification"
    );
    Ok(result)
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Entry {
    name: String,
    staged: String,
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
    root_identity: Id,
    char_identity: Id,
    publishing: bool,
    entries: Vec<Entry>,
}
impl Record {
    fn validate(&self, root: &Path, session: &str) -> Result<()> {
        ensure!(
            self.schema == SCHEMA
                && self.session == session
                && session.len() == 48
                && session.bytes().all(|b| b.is_ascii_hexdigit())
                && self.root.is_absolute()
                && crate::native_session::same_path(&self.root.to_string_lossy(), root)
                && self.entries.len() == 3,
            "Generated selector receipt ownership differs"
        );
        for (index, entry) in self.entries.iter().enumerate() {
            ensure!(
                entry.name == PATHS[index]
                    && entry.staged == format!(".2k-launcher-selector-{session}-{index}.tmp")
                    && (22..=sigconfig::MAX_BYTES).contains(&entry.size)
                    && entry.sha256.len() == 64
                    && entry.sha256.bytes().all(|b| b.is_ascii_hexdigit())
                    && (!self.publishing || entry.identity.is_some()),
                "Invalid generated selector receipt entry"
            );
        }
        Ok(())
    }
}
pub(crate) struct Store {
    portable: PathBuf,
    record: Record,
    _pins: Vec<Directories>,
}
impl Store {
    fn marker(&self) -> PathBuf {
        self.portable.join("Data").join(MARKER)
    }
    fn stage(&self, entry: &Entry) -> PathBuf {
        self.record.root.join("char").join(&entry.staged)
    }
    fn target(&self, entry: &Entry) -> PathBuf {
        self.record.root.join(&entry.name)
    }
    fn commit(&self) -> Result<()> {
        backend::atomic_json(&self.marker(), &self.record)
    }
    pub fn root(&self) -> &Path {
        &self.record.root
    }
    pub fn create(
        portable: &Path,
        mods: &Path,
        session: &str,
        prepared: &Prepared,
        closed: impl Fn() -> Result<()>,
    ) -> Result<Self> {
        closed()?;
        let mut pins = vec![
            Directories::pin(portable)?,
            Directories::pin(&portable.join("Data"))?,
            Directories::pin(mods)?,
        ];
        ensure!(
            missing(&portable.join("Data").join(MARKER))?,
            "Generated selector recovery is pending"
        );
        let root = mods.canonicalize()?;
        for name in PATHS {
            ensure!(
                missing(&root.join(name))?,
                "Existing selector mod was left untouched; generation requires an absent bundle"
            );
        }
        let char_dir = root.join("char");
        closed()?;
        if missing(&char_dir)? {
            fs::create_dir(&char_dir)?;
        }
        pins.push(Directories::pin(&char_dir)?);
        let record = Record {
            schema: SCHEMA.into(),
            session: session.into(),
            root_identity: dir_id(&root)?,
            char_identity: dir_id(&char_dir)?,
            root,
            publishing: false,
            entries: PATHS
                .iter()
                .enumerate()
                .map(|(index, name)| Entry {
                    name: (*name).into(),
                    staged: format!(".2k-launcher-selector-{session}-{index}.tmp"),
                    size: prepared.bytes[index].len(),
                    sha256: loader_files::digest(&prepared.bytes[index]),
                    identity: None,
                })
                .collect(),
        };
        record.validate(&record.root, session)?;
        for entry in &record.entries {
            ensure!(
                missing(&char_dir.join(&entry.staged))?,
                "Generated selector staging path already exists; left untouched"
            );
        }
        let store = Self {
            portable: portable.into(),
            record,
            _pins: pins,
        };
        closed()?;
        backend::atomic_json_new(&store.marker(), &store.record)?;
        Ok(store)
    }
    pub fn activate(&mut self, prepared: &Prepared, closed: impl Fn() -> Result<()>) -> Result<()> {
        ensure!(
            !self.record.publishing,
            "Generated selector publication already began"
        );
        for (entry, data) in self.record.entries.iter().zip(&prepared.bytes) {
            ensure!(
                data.len() == entry.size
                    && loader_files::digest(data) == entry.sha256
                    && missing(&self.target(entry))?
                    && missing(&self.stage(entry))?,
                "Generated selector plan/destination changed"
            );
        }
        let mut identities = Vec::new();
        for (entry, data) in self.record.entries.iter().zip(&prepared.bytes) {
            closed()?;
            let mut file = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .share_mode(1)
                .open(self.stage(entry))?;
            file.write_all(data)?;
            file.sync_all()?;
            identities.push(identity(&file)?);
        }
        for (entry, id) in self.record.entries.iter_mut().zip(identities) {
            entry.identity = Some(id);
        }
        self.record.publishing = true;
        closed()?;
        self.commit()?;
        for entry in &self.record.entries {
            closed()?;
            let mut file = open(&self.stage(entry), true)?;
            self.verify(&mut file, entry)?;
            closed()?;
            loader_files::publish(&file, &self.target(entry))?;
        }
        self.verify_prepared(prepared)
    }
    fn verify(&self, file: &mut File, entry: &Entry) -> Result<()> {
        ensure!(
            entry
                .identity
                .as_ref()
                .is_none_or(|expected| identity(file).is_ok_and(|actual| expected.matches(&actual))),
            "Generated selector file identity changed; left untouched"
        );
        let data = bytes(file, sigconfig::MAX_BYTES)?;
        ensure!(
            data.len() == entry.size && loader_files::digest(&data) == entry.sha256,
            "Generated selector bytes changed; left untouched"
        );
        Ok(())
    }
    pub fn verify_prepared(&self, prepared: &Prepared) -> Result<()> {
        ensure!(
            self.record.publishing,
            "Generated selector files are not published"
        );
        for (entry, data) in self.record.entries.iter().zip(&prepared.bytes) {
            ensure!(
                data.len() == entry.size && loader_files::digest(data) == entry.sha256,
                "Prepared selector differs from owned files"
            );
            self.verify(&mut open(&self.target(entry), false)?, entry)?;
        }
        Ok(())
    }
    pub fn recover(portable: &Path, mods: &Path, session: &str) -> Result<Option<Self>> {
        let marker = portable.join("Data").join(MARKER);
        if missing(&marker)? {
            return Ok(None);
        }
        let record: Record =
            serde_json::from_slice(&bytes(&mut open(&marker, false)?, 64 * 1024)?)?;
        record.validate(mods, session)?;
        let pins = vec![
            Directories::pin(portable)?,
            Directories::pin(&portable.join("Data"))?,
            Directories::pin(mods)?,
            Directories::pin(&mods.join("char"))?,
        ];
        ensure!(
            record.root_identity.matches(&dir_id(mods)?)
                && record.char_identity.matches(&dir_id(&mods.join("char"))?),
            "Generated selector recovery directory identity changed"
        );
        Ok(Some(Self {
            portable: portable.into(),
            record,
            _pins: pins,
        }))
    }
    pub fn restore(&mut self, closed: impl Fn() -> Result<()>) -> Result<()> {
        closed()?;
        self.record.validate(self.root(), &self.record.session)?;
        ensure!(
            self.record.root_identity.matches(&dir_id(self.root())?)
                && self
                    .record
                    .char_identity
                    .matches(&dir_id(&self.record.root.join("char"))?),
            "Generated selector directory ownership changed"
        );
        let mut candidates = Vec::new();
        // Validate the entire remaining owned bundle before deleting any member.
        for entry in &self.record.entries {
            let stage = self.stage(entry);
            let path = if !missing(&stage)? {
                Some(stage)
            } else if self.record.publishing {
                Some(self.target(entry))
            } else {
                None
            };
            let Some(path) = path else {
                continue;
            };
            if missing(&path)? {
                continue;
            }
            let mut file = open(&path, true)?;
            self.verify(&mut file, entry)?;
            candidates.push((path, file));
        }
        let mut paths = Vec::new();
        for (path, file) in candidates {
            closed()?;
            loader_files::unlink(&file)?;
            drop(file);
            paths.push(path);
        }
        for path in paths {
            ensure!(
                missing(&path)?,
                "Generated selector removal is still pending; receipt retained"
            );
        }
        let mut marker = open(&self.marker(), true)?;
        let current: Record = serde_json::from_slice(&bytes(&mut marker, 64 * 1024)?)?;
        ensure!(
            current == self.record,
            "Generated selector receipt changed; left untouched"
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
        prepared: Prepared,
    }
    impl Fixture {
        fn new() -> Self {
            let base =
                std::env::temp_dir().join(format!("rust-selector-files-{}", backend::unique_id()));
            let portable = base.join("portable");
            let mods = base.join("mods");
            fs::create_dir_all(portable.join("Data")).unwrap();
            fs::create_dir(&mods).unwrap();
            Self {
                base,
                portable,
                mods,
                session: backend::unique_id(),
                prepared: Prepared::existing(crate::selector_tables::tests::bundle()).unwrap(),
            }
        }
        fn store(&self) -> Store {
            Store::create(
                &self.portable,
                &self.mods,
                &self.session,
                &self.prepared,
                || Ok(()),
            )
            .unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }
    #[test]
    fn fresh_bundle_intent_precedes_publication_and_exact_restore_preserves_other_mods() {
        let f = Fixture::new();
        fs::write(f.mods.join("other.iff"), b"user").unwrap();
        let mut store = f.store();
        assert!(store.marker().exists());
        for name in PATHS {
            assert!(!f.mods.join(name).exists());
        }
        store.activate(&f.prepared, || Ok(())).unwrap();
        for (name, data) in PATHS.iter().zip(&f.prepared.bytes) {
            assert_eq!(fs::read(f.mods.join(name)).unwrap(), *data);
        }
        assert!(store.restore(|| anyhow::bail!("game running")).is_err());
        store.restore(|| Ok(())).unwrap();
        for name in PATHS {
            assert!(!f.mods.join(name).exists());
        }
        assert_eq!(fs::read(f.mods.join("other.iff")).unwrap(), b"user");
        assert!(!store.marker().exists());
    }
    #[test]
    fn foreign_bundle_and_appearing_target_are_never_overwritten_or_removed() {
        let f = Fixture::new();
        fs::create_dir(f.mods.join("char")).unwrap();
        fs::write(f.mods.join(PATHS[0]), b"foreign").unwrap();
        assert!(Store::create(&f.portable, &f.mods, &f.session, &f.prepared, || Ok(())).is_err());
        assert_eq!(fs::read(f.mods.join(PATHS[0])).unwrap(), b"foreign");
        let f = Fixture::new();
        let mut store = f.store();
        fs::write(f.mods.join(PATHS[1]), b"appeared").unwrap();
        assert!(store.activate(&f.prepared, || Ok(())).is_err());
        store.restore(|| Ok(())).unwrap();
        assert_eq!(fs::read(f.mods.join(PATHS[1])).unwrap(), b"appeared");
    }
    #[test]
    fn interrupted_staging_and_half_publication_recover_only_matching_session() {
        for phase in 0..3 {
            let f = Fixture::new();
            let mut store = f.store();
            if phase > 0 {
                for (entry, data) in store.record.entries.iter().zip(&f.prepared.bytes) {
                    fs::write(store.stage(entry), data).unwrap();
                }
            }
            if phase == 2 {
                let ids = store
                    .record
                    .entries
                    .iter()
                    .map(|e| identity(&open(&store.stage(e), false).unwrap()).unwrap())
                    .collect::<Vec<_>>();
                for (entry, id) in store.record.entries.iter_mut().zip(ids) {
                    entry.identity = Some(id);
                }
                store.record.publishing = true;
                store.commit().unwrap();
                let entry = &store.record.entries[0];
                fs::rename(store.stage(entry), store.target(entry)).unwrap();
            }
            drop(store);
            assert!(Store::recover(&f.portable, &f.mods, &backend::unique_id()).is_err());
            let mut store = Store::recover(&f.portable, &f.mods, &f.session)
                .unwrap()
                .unwrap();
            store.restore(|| Ok(())).unwrap();
            assert_eq!(fs::read_dir(f.mods.join("char")).unwrap().count(), 0);
        }
    }
    #[test]
    fn alternate_profile_receipt_retains_selector_recovery_root_before_and_after_activation() {
        for active in [false, true] {
            let f = Fixture::new();
            let game = f.base.join("game");
            fs::create_dir_all(game.join("mods")).unwrap();
            fs::write(game.join("mods/original.iff"), b"original").unwrap();
            let mut profile =
                crate::mod_profile::Store::create(&f.portable, &game, &f.mods, &f.session, || {
                    Ok(())
                })
                .unwrap()
                .unwrap();
            let mut store = f.store();
            store.activate(&f.prepared, || Ok(())).unwrap();
            if active {
                profile.activate(|| Ok(())).unwrap();
            }
            drop(store);
            drop(profile);
            let mut profile = crate::mod_profile::Store::recover(&f.portable, &game)
                .unwrap()
                .unwrap();
            let mut store = Store::recover(&f.portable, profile.selected_root(), &f.session)
                .unwrap()
                .unwrap();
            store.restore(|| Ok(())).unwrap();
            drop(store);
            profile.restore(|| Ok(())).unwrap();
            assert_eq!(
                fs::read(game.join("mods/original.iff")).unwrap(),
                b"original"
            );
            for name in PATHS {
                assert!(!f.mods.join(name).exists());
            }
        }
    }
    #[test]
    fn changed_bytes_or_identity_hold_whole_bundle_and_tampered_receipts_survive() {
        let f = Fixture::new();
        let mut store = f.store();
        store.activate(&f.prepared, || Ok(())).unwrap();
        fs::write(f.mods.join(PATHS[1]), b"user edit").unwrap();
        assert!(store.restore(|| Ok(())).is_err());
        assert!(
            f.mods.join(PATHS[0]).exists()
                && f.mods.join(PATHS[2]).exists()
                && store.marker().exists()
        );
        let f = Fixture::new();
        let store = f.store();
        let mut bad = store.record.clone();
        bad.entries[0].name = "../outside".into();
        backend::atomic_json(&store.marker(), &bad).unwrap();
        drop(store);
        assert!(Store::recover(&f.portable, &f.mods, &f.session).is_err());
        assert!(f.portable.join("Data").join(MARKER).exists());
    }
    #[test]
    fn fresh_sessions_repeat_the_same_three_targets_without_false_identity_failure() {
        let f = Fixture::new();
        fs::write(f.mods.join("other.iff"), b"user").unwrap();
        for round in 0..12 {
            let session = backend::unique_id();
            let mut store =
                Store::create(&f.portable, &f.mods, &session, &f.prepared, || Ok(())).unwrap();
            let activated = store.activate(&f.prepared, || Ok(()));
            if let Err(error) = activated {
                let identities: Vec<_> = store
                    .record
                    .entries
                    .iter()
                    .map(|entry| {
                        (
                            entry.name.clone(),
                            entry.identity.clone(),
                            open(&store.target(entry), false)
                                .and_then(|file| identity(&file))
                                .ok(),
                        )
                    })
                    .collect();
                panic!(
                    "Fresh selector session {round} failed: {error:#}; staged/target identities: {identities:?}"
                );
            }
            store.verify_prepared(&f.prepared).unwrap();
            for (path, expected) in PATHS.iter().zip(&f.prepared.bytes) {
                assert_eq!(fs::read(f.mods.join(path)).unwrap(), *expected);
            }
            drop(store);
            let mut recovered = Store::recover(&f.portable, &f.mods, &session)
                .unwrap()
                .unwrap();
            recovered.restore(|| Ok(())).unwrap();
            drop(recovered);
            for path in PATHS {
                assert!(!f.mods.join(path).exists());
            }
            assert!(!f.portable.join("Data").join(MARKER).exists());
            assert_eq!(fs::read(f.mods.join("other.iff")).unwrap(), b"user");
        }
    }
    #[test]
    fn stable_identity_ignores_timestamp_tunneling_but_legacy_receipts_stay_strict() {
        let f = Fixture::new();
        let mut store = f.store();
        store.activate(&f.prepared, || Ok(())).unwrap();
        let expected = store.record.entries[0].identity.as_ref().unwrap().clone();
        let mut metadata_only = expected.clone();
        metadata_only.birth ^= 1;
        assert!(expected.matches(&metadata_only));
        let mut legacy = expected.clone();
        legacy.stable = None;
        assert!(legacy.matches(&expected));
        assert!(!legacy.matches(&metadata_only));
        let mut missing_stable = expected.clone();
        missing_stable.stable = None;
        assert!(!expected.matches(&missing_stable));
        let mut changed_legacy = legacy.clone();
        changed_legacy.index ^= 1;
        assert!(!legacy.matches(&changed_legacy));
        let mut changed_legacy = legacy.clone();
        changed_legacy.volume ^= 1;
        assert!(!legacy.matches(&changed_legacy));
        // Simulate a real old on-disk receipt with the new optional field
        // absent in every ID, without changing its schema or other fields.
        store.record.root_identity.stable = None;
        store.record.char_identity.stable = None;
        for entry in &mut store.record.entries {
            entry.identity.as_mut().unwrap().stable = None;
        }
        let serialized = serde_json::to_string(&store.record).unwrap();
        assert!(!serialized.contains("stable"));
        store.commit().unwrap();
        drop(store);
        let mut recovered = Store::recover(&f.portable, &f.mods, &f.session)
            .unwrap()
            .unwrap();
        recovered.restore(|| Ok(())).unwrap();
    }
    #[test]
    fn equal_bytes_in_a_new_file_are_not_owned_even_after_target_name_tunneling() {
        let f = Fixture::new();
        let mut store = f.store();
        store.activate(&f.prepared, || Ok(())).unwrap();
        let target = f.mods.join(PATHS[0]);
        let preserved = f.mods.join("char/preserved-owned.iff");
        fs::rename(&target, &preserved).unwrap();
        fs::write(&target, &f.prepared.bytes[0]).unwrap();
        let actual = identity(&open(&target, false).unwrap()).unwrap();
        assert!(
            !store.record.entries[0]
                .identity
                .as_ref()
                .unwrap()
                .matches(&actual)
        );
        assert!(store.restore(|| Ok(())).is_err());
        for path in PATHS {
            assert!(f.mods.join(path).is_file());
        }
        assert!(store.marker().is_file() && preserved.is_file());
        assert_eq!(fs::read(target).unwrap(), f.prepared.bytes[0]);
    }
}
