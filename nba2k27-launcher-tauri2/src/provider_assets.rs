//! Read-only profile asset engine. No game access, loader installation or pipe.
//! The native provider will supply client authorization before invoking it.
#![allow(dead_code)] // Unreleased provider; deliberately not a launch capability.
use crate::provider_protocol::{self as wire, Request};
use anyhow::{Context, Result, ensure};
use std::{
    cmp::Ordering as Compare,
    collections::BTreeMap,
    fs::{self, File},
    os::windows::{
        ffi::OsStrExt,
        fs::{FileExt, MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::HANDLE,
    Globalization::CompareStringOrdinal,
    Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_NAME_NORMALIZED,
        GetFinalPathNameByHandleW, VOLUME_NAME_DOS,
    },
};
const MAX_ASSETS: usize = 100_000;
const MAX_ENTRIES: usize = 500_000;
const MAX_CACHED_FILES: usize = 512;
const MAX_DEPTH: usize = 1024;
const MAX_ALIAS_BYTES: u64 = 64 * 1024 * 1024;
#[derive(Debug, Clone, Copy)]
#[repr(u32)]
pub(crate) enum Failure {
    Unauthorized = 1,
    Missing = 2,
    Invalid = 3,
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unauthorized => "Provider operation is not authorized",
            Self::Missing => "Asset was not found in selected profile",
            Self::Invalid => "Invalid provider request",
        })
    }
}
impl std::error::Error for Failure {}
pub(crate) fn status(error: &anyhow::Error) -> u32 {
    if let Some(e) = error.downcast_ref::<Failure>() {
        return *e as u32;
    }
    if error.downcast_ref::<wire::Invalid>().is_some() {
        return 3;
    }
    if let Some(e) = error.downcast_ref::<std::io::Error>() {
        return match e.kind() {
            std::io::ErrorKind::NotFound => 2,
            std::io::ErrorKind::PermissionDenied => 1,
            _ => 4,
        };
    }
    4
}
#[derive(Clone, Debug, Eq)]
struct Key(Vec<u16>);
fn ordinal_fold(c: char) -> char {
    if c.is_ascii() {
        return c.to_ascii_uppercase();
    }
    static FOLDS: OnceLock<BTreeMap<u32, u32>> = OnceLock::new();
    let folds = FOLDS.get_or_init(|| {
        let data: serde_json::Value =
            serde_json::from_str(include_str!("../assets/provider-ordinal.json"))
                .expect("Original provider ordinal table");
        assert_eq!(data["schema"], "nba2k27-provider-ordinal-v1");
        let rows = data["folds"].as_array().expect("Ordinal rows");
        assert!((1000..=4096).contains(&rows.len()));
        let mut folds = BTreeMap::new();
        for row in rows {
            let c = u32::try_from(row[0].as_u64().unwrap()).unwrap();
            let value = u32::try_from(row[1].as_u64().unwrap()).unwrap();
            assert!(value < c && char::from_u32(c).is_some() && char::from_u32(value).is_some());
            assert!(folds.insert(c, value).is_none());
        }
        folds
    });
    char::from_u32(folds.get(&(c as u32)).copied().unwrap_or(c as u32)).unwrap()
}
impl Key {
    fn new(value: &str) -> Result<Self> {
        let folded: String = value.chars().map(ordinal_fold).collect();
        let text: Vec<_> = folded.encode_utf16().collect();
        ensure!(!text.is_empty() && text.len() <= 32767, Failure::Invalid);
        Ok(Self(text))
    }
}
impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Compare::Equal
    }
}
impl PartialOrd for Key {
    fn partial_cmp(&self, other: &Self) -> Option<Compare> {
        Some(self.cmp(other))
    }
}
impl Ord for Key {
    fn cmp(&self, other: &Self) -> Compare {
        self.0.cmp(&other.0)
    }
}
fn logical(value: &str, alias: bool) -> Result<String> {
    let path = value.replace('\\', "/");
    let path = if alias { path.as_str() } else { path.trim() };
    ensure!(
        !path.trim().is_empty()
            && !path.starts_with('/')
            && !path.contains(['\0', ':'])
            && !path
                .split('/')
                .any(|p| p.is_empty() || p == "." || p == ".." || (alias && p.trim().is_empty())),
        Failure::Invalid
    );
    Key::new(path)?;
    Ok(path.into())
}
fn final_path(file: &File) -> Result<PathBuf> {
    let mut path = vec![0u16; 512];
    let mut n = unsafe {
        GetFinalPathNameByHandleW(
            HANDLE(file.as_raw_handle()),
            &mut path,
            windows::Win32::Storage::FileSystem::GETFINALPATHNAMEBYHANDLE_FLAGS(
                FILE_NAME_NORMALIZED.0 | VOLUME_NAME_DOS.0,
            ),
        )
    } as usize;
    if n >= path.len() {
        ensure!(n < 32768, "Provider final path exceeds Windows bounds");
        path.resize(n + 1, 0);
        n = unsafe {
            GetFinalPathNameByHandleW(
                HANDLE(file.as_raw_handle()),
                &mut path,
                windows::Win32::Storage::FileSystem::GETFINALPATHNAMEBYHANDLE_FLAGS(
                    FILE_NAME_NORMALIZED.0 | VOLUME_NAME_DOS.0,
                ),
            )
        } as usize;
    }
    ensure!(
        n > 0 && n < path.len(),
        "Could not verify the provider file's final path"
    );
    Ok(PathBuf::from(String::from_utf16(&path[..n])?))
}
fn same_path(left: &Path, right: &Path) -> Result<bool> {
    // Native filesystem containment must not use the broader .NET logical-key
    // equivalence (e.g. final sigma). Check final Win32 paths independently.
    let left: Vec<_> = left.as_os_str().encode_wide().collect();
    let right: Vec<_> = right.as_os_str().encode_wide().collect();
    ensure!(
        !left.is_empty() && !right.is_empty() && left.len() <= 32767 && right.len() <= 32767,
        "Physical provider path exceeds bounds"
    );
    let result = unsafe { CompareStringOrdinal(&left, &right, true) }.0;
    ensure!(
        (1..=3).contains(&result),
        "Physical provider path comparison failed"
    );
    Ok(result == 2)
}
fn contained(root: &Path, file: &Path) -> Result<bool> {
    let a: Vec<_> = root.components().collect();
    let b: Vec<_> = file.components().collect();
    if b.len() <= a.len() {
        return Ok(false);
    }
    for (a, b) in a.iter().zip(&b) {
        if !same_path(Path::new(a.as_os_str()), Path::new(b.as_os_str()))? {
            return Ok(false);
        }
    }
    Ok(true)
}
fn unlinked(meta: &fs::Metadata) -> bool {
    meta.file_attributes() & 0x400 == 0
}
#[derive(Clone, Debug)]
pub(crate) struct Asset {
    pub logical: String,
    pub size: u64,
    path: PathBuf,
}
struct Pinned {
    file: File,
    size: u64,
}
#[derive(Default)]
struct State {
    assets: BTreeMap<Key, Asset>,
    virtuals: BTreeMap<Key, Arc<Vec<u8>>>,
    cache: BTreeMap<Key, Arc<Pinned>>,
    // These at most72 exact-owned audio files must not be evicted/reopened
    // under the ordinary user-mod cache's permissive sharing mode.
    jukebox: BTreeMap<Key, Arc<Pinned>>,
    mutable: BTreeMap<Key, Option<Asset>>,
    aliases: usize,
}
pub(crate) struct Service {
    root: PathBuf,
    root_handle: File,
    state: Mutex<State>,
    stop: Arc<AtomicBool>,
}
impl Service {
    pub(crate) fn preflight_jukebox(&self) -> Result<()> {
        self.verify_root()?;
        let s = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Provider state unavailable"))?;
        ensure!(s.jukebox.is_empty(), "Jukebox publication is already owned");
        for name in std::iter::once("music.iff".into())
            .chain((1..=71).map(crate::jukebox_catalog::audio_path))
        {
            let key = Key::new(&name)?;
            ensure!(
                !s.mutable.contains_key(&key)
                    && !s.virtuals.contains_key(&key)
                    && s.assets.get(&key).is_none_or(|a| same_path(
                        &a.path,
                        &self.root.join(&name)
                    )
                    .unwrap_or(false)),
                "An existing Jukebox alias/virtual route was preserved; choose Default music for this profile"
            );
        }
        Ok(())
    }
    /// The session supplies only internally derived owned paths. Retain READ
    /// handles denying writes/deletion before publishing the entire group,
    /// and keep them until shutdown_transport has drained all asset reads.
    pub(crate) fn publish_jukebox(&self, session: &crate::jukebox_launch::Session) -> Result<()> {
        self.preflight_jukebox()?;
        let publication = session.publication()?;
        let expected = std::iter::once("music.iff".to_owned())
            .chain(
                session
                    .policy
                    .custom
                    .bytes()
                    .enumerate()
                    .filter(|(_, b)| *b == b'1')
                    .map(|(i, _)| crate::jukebox_catalog::audio_path((i + 1) as u16)),
            )
            .chain(session.cover_paths().into_iter().filter(|p| {
                p == crate::jukebox_cover_route::CONTAINER
                    || crate::jukebox_cover_route::texture_path(p)
            }))
            .collect::<std::collections::BTreeSet<_>>();
        ensure!(
            publication.len() == expected.len()
                && (1..=72 + 1 + crate::jukebox_cover_route::MAX_COVERS)
                    .contains(&publication.len())
                && publication
                    .iter()
                    .map(|p| p.logical.clone())
                    .collect::<std::collections::BTreeSet<_>>()
                    == expected,
            "Jukebox publication differs from its fixed native mask"
        );
        let mut entries = Vec::new();
        for p in publication {
            self.check()?;
            ensure!(
                same_path(&p.path, &self.root.join(&p.logical))?,
                "Jukebox file belongs to another profile"
            );
            self.check_components(&p.path)?;
            let file = fs::OpenOptions::new()
                .read(true)
                .share_mode(1)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
                .open(&p.path)?;
            let meta = file.metadata()?;
            ensure!(
                meta.is_file()
                    && unlinked(&meta)
                    && meta.len() == p.size as u64
                    && same_path(&final_path(&file)?, &p.path)?,
                "Jukebox publication changed before pinning"
            );
            entries.push((
                Key::new(&p.logical)?,
                Asset {
                    logical: p.logical,
                    size: p.size as u64,
                    path: p.path,
                },
                Arc::new(Pinned {
                    file,
                    size: p.size as u64,
                }),
            ));
        }
        // All handles are now pinned. The session rechecks its full kernel IDs
        // and bytes; a same-size or same-byte foreign replacement is rejected.
        session.verify_current()?;
        self.verify_root()?;
        let mut s = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Provider state unavailable"))?;
        ensure!(
            s.jukebox.is_empty() && s.assets.len() + entries.len() <= MAX_ASSETS,
            "Jukebox publication exceeds provider bounds or is already active"
        );
        ensure!(
            entries
                .iter()
                .all(|(key, a, _)| !s.mutable.contains_key(key)
                    && !s.virtuals.contains_key(key)
                    && !s.cache.contains_key(key)
                    && s.assets
                        .get(key)
                        .is_none_or(|old| same_path(&old.path, &a.path).unwrap_or(false))),
            "Jukebox routes changed while owned files were being pinned"
        );
        for (key, asset, pinned) in entries {
            s.assets.insert(key.clone(), asset);
            s.jukebox.insert(key, pinned);
        }
        Ok(())
    }
    /// Publish only the five source-verified scoreboard text adapters. The
    /// pinned loader already has an exact SCOREBOARD_TEXT_TEST allowlist for
    /// both full paths and leaves; no general alias or disk write is granted.
    pub(crate) fn publish_scoreboard_layouts(
        &self,
        prepared: &crate::scoreboard_layout::Prepared,
    ) -> Result<()> {
        self.publish_scoreboard_files(prepared.files())
    }
    fn publish_scoreboard_files(&self, files: &BTreeMap<String, Vec<u8>>) -> Result<()> {
        self.verify_root()?;
        ensure!(
            files.len() == crate::scoreboard_layout::PATHS.len()
                && crate::scoreboard_layout::PATHS
                    .iter()
                    .all(|p| files.contains_key(*p)),
            "Scoreboard bundle differs from the fixed five paths"
        );
        let mut entries = Vec::new();
        for (path, bytes) in files {
            ensure!(
                (1..=16 * 1024 * 1024).contains(&bytes.len()),
                "Scoreboard layout exceeds bounds"
            );
            let data = Arc::new(bytes.clone());
            for name in [
                path.as_str(),
                path.strip_prefix("gooey/")
                    .context("Invalid scoreboard path")?,
            ] {
                entries.push((
                    Key::new(name)?,
                    Asset {
                        logical: name.into(),
                        size: data.len() as u64,
                        path: self.root.join(name),
                    },
                    data.clone(),
                ));
            }
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Provider state unavailable"))?;
        self.verify_root()?;
        ensure!(
            state.assets.len() + entries.len() <= MAX_ASSETS,
            "Scoreboard layouts exceed provider index bounds"
        );
        ensure!(
            entries
                .iter()
                .all(|(key, _, _)| !state.assets.contains_key(key)
                    && !state.virtuals.contains_key(key)
                    && !state.mutable.contains_key(key)),
            "An existing scoreboard mod or alias was left untouched; text-adapted stock layouts are unavailable for this launch"
        );
        // All collisions and source bytes are checked before publishing any
        // entry. A failed preparation leaves every original mod route intact.
        for (key, asset, bytes) in entries {
            state.assets.insert(key.clone(), asset);
            state.virtuals.insert(key, bytes);
        }
        Ok(())
    }
    /// Freeze the same bytes already published for early localization reads.
    /// Existing localization/aliases in the original index still block this
    /// route, and another profile's lease never grants publication authority.
    pub(crate) fn publish_owned_camera_names(
        &self,
        bytes: Vec<u8>,
        names: &BTreeMap<u32, String>,
        store: &crate::camera_name_files::Store,
    ) -> Result<()> {
        ensure!(
            same_path(&self.root, store.root())?,
            "Camera labels belong to another profile"
        );
        store.verify_prepared(&bytes)?;
        self.publish_camera_names(bytes, names)
    }
    /// Internal immutable-byte primitive. It never mutates the filesystem.
    pub(crate) fn publish_camera_names(
        &self,
        bytes: Vec<u8>,
        names: &BTreeMap<u32, String>,
    ) -> Result<()> {
        crate::camera_names::verify(&bytes, names)?;
        ensure!(
            bytes.len() <= crate::sigconfig::MAX_BYTES,
            "Camera localization exceeds bounds"
        );
        self.check()?;
        let name = "loc/english.iff";
        let key = Key::new(name)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Provider state unavailable"))?;
        self.check()?;
        ensure!(
            !state.assets.contains_key(&key)
                && !state.virtuals.contains_key(&key)
                && !state.mutable.contains_key(&key),
            "Existing English localization mod or alias was left untouched; turn Cameras off for this profile"
        );
        ensure!(
            state.assets.len() < MAX_ASSETS,
            "Camera labels exceed provider index bounds"
        );
        state.assets.insert(
            key.clone(),
            Asset {
                logical: name.into(),
                size: bytes.len() as u64,
                path: self.root.join(name),
            },
        );
        state.virtuals.insert(key, Arc::new(bytes));
        Ok(())
    }
    /// Publish the locally built, hash-pinned mixed Learn2K roster at its one
    /// fixed path. A user's own roster mod or alias at that path is preserved.
    /// May run mid-session: until then the forced OVERRIDE reads stock.
    pub(crate) fn publish_wnba_roster(&self, prepared: &crate::wnba_roster_v2::Prepared) -> Result<()> {
        ensure!(
            (1..=64 * 1024 * 1024).contains(&prepared.bytes.len()),
            "Mixed roster exceeds bounds"
        );
        self.check()?;
        let name = crate::wnba_roster_v2::PATH;
        let key = Key::new(name)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Provider state unavailable"))?;
        self.check()?;
        ensure!(
            !state.assets.contains_key(&key)
                && !state.virtuals.contains_key(&key)
                && !state.mutable.contains_key(&key),
            "The selected mod profile already provides roster/learn2k_roster.iff; it was left untouched and the mixed WNBA roster is not used"
        );
        ensure!(
            state.assets.len() < MAX_ASSETS,
            "Mixed roster exceeds provider index bounds"
        );
        state.assets.insert(
            key.clone(),
            Asset {
                logical: name.into(),
                size: prepared.bytes.len() as u64,
                path: self.root.join(name),
            },
        );
        state.virtuals.insert(key, Arc::new(prepared.bytes.clone()));
        Ok(())
    }
    /// Install only the fixed internally generated bundle in the provider's
    /// immutable view, after its physical files are owned and verified. Early
    /// game reads and later provider reads therefore see the same generation.
    pub(crate) fn publish_generated_selectors(
        &self,
        prepared: &crate::selector_tables::Prepared,
        store: &crate::selector_files::Store,
    ) -> Result<()> {
        ensure!(
            same_path(&self.root, store.root())?,
            "Generated selectors belong to another profile"
        );
        self.check()?;
        store.verify_prepared(prepared)?;
        let mut entries = Vec::new();
        for (name, data) in crate::selector_tables::PATHS.iter().zip(&prepared.bytes) {
            let asset = Asset {
                logical: (*name).into(),
                size: data.len() as u64,
                path: self.root.join(name),
            };
            self.open_asset(&asset)?;
            entries.push((Key::new(name)?, asset, Arc::new(data.clone())));
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Provider state unavailable"))?;
        self.check()?;
        store.verify_prepared(prepared)?;
        ensure!(
            state.assets.len() + entries.len() <= MAX_ASSETS,
            "Generated selectors exceed provider index bounds"
        );
        ensure!(
            entries
                .iter()
                .all(|(key, _, _)| !state.assets.contains_key(key)
                    && !state.virtuals.contains_key(key)
                    && !state.mutable.contains_key(key)),
            "Generated selectors collide with an existing mod or alias"
        );
        for (key, asset, bytes) in entries {
            state.assets.insert(key.clone(), asset);
            state.virtuals.insert(key, bytes);
        }
        Ok(())
    }
    pub(crate) fn selector_inputs(&self) -> Result<crate::selector_generation::Inputs> {
        self.check()?;
        let paths = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Provider state unavailable"))?
            .assets
            .values()
            .map(|a| a.logical.clone())
            .collect::<Vec<_>>();
        let mut inputs = crate::selector_generation::inventory(paths)?;
        for group in inputs.values_mut() {
            for path in &group.base_paths {
                self.check()?;
                let asset = self.query(path)?;
                self.check_components(&asset.path)?;
                ensure!(
                    asset.size <= 128 * 1024 * 1024,
                    "Player archive exceeds selector inspection bounds"
                );
                let mut file = fs::OpenOptions::new()
                    .read(true)
                    .share_mode(1)
                    .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
                    .open(&asset.path)?;
                let before = file.metadata()?;
                ensure!(
                    before.is_file()
                        && unlinked(&before)
                        && before.len() == asset.size
                        && same_path(&final_path(&file)?, &asset.path)?,
                    "Player archive changed before selector inspection"
                );
                let result = crate::selector_generation::appearance(&mut file);
                let after = file.metadata()?;
                self.check_components(&asset.path)?;
                ensure!(
                    before.len() == after.len()
                        && before.last_write_time() == after.last_write_time()
                        && same_path(&final_path(&file)?, &asset.path)?,
                    "Player definition changed during selector inspection"
                );
                let (info, error) = match result {
                    Ok(info) => (Some(info), None),
                    Err(error) => (None, Some(format!("{error:#}"))),
                };
                group.bases.push(crate::selector_generation::Base {
                    path: path.clone(),
                    info,
                    error,
                });
            }
        }
        self.check()?;
        Ok(inputs)
    }
    /// Make only the internally planned, already-published player hard links
    /// visible at their canonical native paths. No public alias capability is
    /// widened; source data remains in the selected profile and is not copied.
    pub fn index_player_links(&self, store: &crate::player_folders::Store) -> Result<()> {
        ensure!(
            same_path(&self.root, store.root())?,
            "Player-folder links belong to another profile"
        );
        let routes = store.routes();
        store.verify_routes()?;
        ensure!(routes.len() <= 10_000, "Too many player-folder routes");
        let mut prepared = Vec::new();
        for (target, source) in routes {
            self.check()?;
            let source = self.query(&source)?;
            let target = logical(&target, true)?;
            let asset = Asset {
                logical: target.clone(),
                size: source.size,
                path: self.root.join(target.replace('/', "\\")),
            };
            self.open_asset(&asset)?;
            prepared.push((Key::new(&target)?, asset));
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Provider state unavailable"))?;
        self.check()?;
        store.verify_routes()?;
        ensure!(
            state.assets.len() + prepared.len() <= MAX_ASSETS,
            "Published player routes exceed the asset index limit"
        );
        ensure!(
            prepared
                .iter()
                .all(|(key, _)| !state.assets.contains_key(key)
                    && !state.mutable.contains_key(key)
                    && !state.virtuals.contains_key(key)),
            "Player route collides with an indexed mod"
        );
        for (key, asset) in prepared {
            state.assets.insert(key, asset);
        }
        Ok(())
    }
    /// Only the three fixed, coordinated player-selector paths can be frozen.
    /// All original handles deny writes/deletion until validation and atomic
    /// publication finish. No user files are installed, replaced or removed.
    pub fn prepare_selectors(&self) -> Result<crate::selector_tables::Summary> {
        self.prepare_selectors_with(|| Ok(()))
    }
    fn prepare_selectors_with(
        &self,
        before_publish: impl FnOnce() -> Result<()>,
    ) -> Result<crate::selector_tables::Summary> {
        use crate::selector_tables::{PATHS, Prepared, Summary};
        self.check()?;
        let keys = PATHS
            .iter()
            .map(|name| Key::new(name))
            .collect::<Result<Vec<_>>>()?;
        let indexed = {
            let state = self
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("Provider state unavailable"))?;
            ensure!(
                keys.iter().all(
                    |key| !state.mutable.contains_key(key) && !state.virtuals.contains_key(key)
                ),
                "Player selectors are already frozen or aliased"
            );
            keys.iter()
                .map(|key| state.assets.get(key).cloned())
                .collect::<Vec<_>>()
        };
        if indexed.iter().all(Option::is_none) {
            return Ok(Summary::absent());
        }
        ensure!(
            indexed.iter().all(Option::is_some),
            "Incomplete player selector bundle: all three char/signatureconfigs_all.iff, signatureconfigs_editplayer.iff and fpplayer_global.iff files are required; nothing was changed"
        );
        let indexed = indexed.into_iter().map(Option::unwrap).collect::<Vec<_>>();
        let mut pinned = Vec::new();
        for asset in &indexed {
            self.check_components(&asset.path)?;
            ensure!(
                (22..=crate::sigconfig::MAX_BYTES as u64).contains(&asset.size),
                "Player selector archive exceeds safe bounds"
            );
            let file = fs::OpenOptions::new()
                .read(true)
                .share_mode(1)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
                .open(&asset.path)
                .with_context(|| format!("Pinning player selector {}", asset.logical))?;
            let metadata = file.metadata()?;
            ensure!(
                metadata.is_file()
                    && unlinked(&metadata)
                    && metadata.len() == asset.size
                    && same_path(&final_path(&file)?, &asset.path)?,
                "Player selector changed after indexing"
            );
            pinned.push(file);
        }
        let mut bytes = Vec::new();
        for (asset, file) in indexed.iter().zip(&pinned) {
            self.check()?;
            let mut data = vec![0; asset.size as usize];
            let mut offset = 0;
            while offset < data.len() {
                self.check()?;
                let end = data.len().min(offset + wire::MAX_READ as usize);
                let received = file.seek_read(&mut data[offset..end], offset as u64)?;
                ensure!(
                    received > 0,
                    "Player selector ended before its indexed size"
                );
                offset += received;
            }
            self.check_components(&asset.path)?;
            ensure!(
                same_path(&final_path(file)?, &asset.path)?,
                "Player selector moved during validation"
            );
            bytes.push(data);
        }
        let prepared = Prepared::existing(
            bytes
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid selector bundle size"))?,
        )?;
        before_publish()?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Provider state unavailable"))?;
        self.check()?;
        for (key, original) in keys.iter().zip(&indexed) {
            ensure!(
                !state.mutable.contains_key(key)
                    && !state.virtuals.contains_key(key)
                    && state
                        .assets
                        .get(key)
                        .is_some_and(|current| current.path == original.path
                            && current.size == original.size),
                "Player selector route changed before publication"
            );
        }
        // Readers take this same lock: the whole bundle becomes immutable in
        // one operation, never a mixture of new and original archive bytes.
        for (key, data) in keys.into_iter().zip(prepared.bytes) {
            state.cache.remove(&key);
            state.virtuals.insert(key, Arc::new(data));
        }
        Ok(prepared.summary)
    }
    /// The current bytes of each indexed selector path (None when absent),
    /// read through the same pinned, reparse-free checks as frozen bundles.
    pub(crate) fn indexed_selectors(&self) -> Result<[Option<(Asset, Vec<u8>)>; 3]> {
        use crate::selector_tables::PATHS;
        self.check()?;
        let indexed = {
            let state = self
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("Provider state unavailable"))?;
            PATHS
                .iter()
                .map(|name| {
                    let key = Key::new(name)?;
                    ensure!(
                        !state.mutable.contains_key(&key) && !state.virtuals.contains_key(&key),
                        "Player selectors are already frozen or aliased"
                    );
                    Ok(state.assets.get(&key).cloned())
                })
                .collect::<Result<Vec<_>>>()?
        };
        let mut out: [Option<(Asset, Vec<u8>)>; 3] = [None, None, None];
        for (slot, asset) in out.iter_mut().zip(indexed) {
            let Some(asset) = asset else { continue };
            self.check_components(&asset.path)?;
            ensure!(
                (22..=crate::sigconfig::MAX_BYTES as u64).contains(&asset.size),
                "Player selector archive exceeds safe bounds"
            );
            let file = fs::OpenOptions::new()
                .read(true)
                .share_mode(1)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
                .open(&asset.path)
                .with_context(|| format!("Pinning player selector {}", asset.logical))?;
            let metadata = file.metadata()?;
            ensure!(
                metadata.is_file()
                    && unlinked(&metadata)
                    && metadata.len() == asset.size
                    && same_path(&final_path(&file)?, &asset.path)?,
                "Player selector changed after indexing"
            );
            let mut data = vec![0; asset.size as usize];
            let mut offset = 0;
            while offset < data.len() {
                self.check()?;
                let end = data.len().min(offset + wire::MAX_READ as usize);
                let received = file.seek_read(&mut data[offset..end], offset as u64)?;
                ensure!(
                    received > 0,
                    "Player selector ended before its indexed size"
                );
                offset += received;
            }
            *slot = Some((asset, data));
        }
        Ok(out)
    }
    /// WPF regenerated selectors over its own or equivalent files. The port
    /// never rewrites user files: the generated bundle becomes the provider's
    /// immutable view of all three paths (the loader forces them through the
    /// provider), and files on disk stay exactly as they were.
    pub(crate) fn override_selectors(
        &self,
        prepared: &crate::selector_tables::Prepared,
        expected: &[Option<(Asset, Vec<u8>)>; 3],
    ) -> Result<()> {
        use crate::selector_tables::PATHS;
        self.check()?;
        let mut entries = Vec::new();
        for ((name, data), current) in PATHS.iter().zip(&prepared.bytes).zip(expected) {
            ensure!(
                (22..=crate::sigconfig::MAX_BYTES).contains(&data.len()),
                "Generated player selector exceeds safe bounds"
            );
            let path = current
                .as_ref()
                .map(|(asset, _)| asset.path.clone())
                .unwrap_or_else(|| self.root.join(name.replace('/', "\\")));
            entries.push((
                Key::new(name)?,
                Asset {
                    logical: (*name).into(),
                    size: data.len() as u64,
                    path,
                },
                Arc::new(data.clone()),
            ));
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Provider state unavailable"))?;
        self.check()?;
        for ((key, _, _), current) in entries.iter().zip(expected) {
            ensure!(
                !state.mutable.contains_key(key) && !state.virtuals.contains_key(key),
                "Player selectors are already frozen or aliased"
            );
            let unchanged = match (state.assets.get(key), current) {
                (None, None) => true,
                (Some(now), Some((before, _))) => {
                    now.path == before.path && now.size == before.size
                }
                _ => false,
            };
            ensure!(
                unchanged,
                "Player selector route changed before publication"
            );
        }
        let added = expected.iter().filter(|e| e.is_none()).count();
        ensure!(
            state.assets.len() + added <= MAX_ASSETS,
            "Generated selectors exceed provider index bounds"
        );
        for (key, asset, bytes) in entries {
            state.cache.remove(&key);
            state.assets.insert(key.clone(), asset);
            state.virtuals.insert(key, bytes);
        }
        Ok(())
    }
    /// A private immutable startup probe supports an empty user Mods folder
    /// without leaving fixture files there. No RPC accepts arbitrary bytes.
    pub fn add_probe(&self, session: &str) -> Result<(String, u64, String)> {
        use sha2::{Digest, Sha256};
        self.check()?;
        ensure!(
            session.len() == 48 && session.bytes().all(|b| b.is_ascii_hexdigit()),
            Failure::Invalid
        );
        let name = format!("__2k_launcher__/provider-{session}.bin");
        let key = Key::new(&name)?;
        let bytes = Arc::new(
            format!("2k Launcher native provider preflight\r\n{session}\r\n").into_bytes(),
        );
        let size = bytes.len() as u64;
        let sha = format!("{:X}", Sha256::digest(bytes.as_slice()));
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Provider state unavailable"))?;
        ensure!(
            state.virtuals.is_empty()
                && !state.assets.contains_key(&key)
                && !state.mutable.contains_key(&key),
            "Startup probe already exists"
        );
        self.check()?;
        state.assets.insert(
            key.clone(),
            Asset {
                logical: name.clone(),
                size,
                path: self.root.join(&name),
            },
        );
        state.virtuals.insert(key, bytes);
        Ok((name, size, sha))
    }
    pub fn open(
        root: &Path,
        aliases: &[(String, String)],
        mutable: &[String],
        stop: Arc<AtomicBool>,
    ) -> Result<Self> {
        ensure!(!stop.load(Ordering::Acquire), "Provider is closing");
        ensure!(root.is_absolute(), "Select an absolute mod profile folder");
        ensure!(aliases.len() <= 8 && mutable.len() <= 4, Failure::Invalid);
        let original = fs::symlink_metadata(root)?;
        ensure!(
            original.is_dir() && unlinked(&original),
            "Mod profile must be a regular, unlinked directory"
        );
        let root_handle = fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(root)?;
        ensure!(
            root_handle.metadata()?.is_dir() && unlinked(&root_handle.metadata()?),
            "Mod profile changed while opening"
        );
        let root = final_path(&root_handle)?;
        let mut service = Self {
            root,
            root_handle,
            state: Mutex::new(State::default()),
            stop,
        };
        let mut state = State::default();
        let started = Instant::now();
        let mut pending = vec![(service.root.clone(), 0usize)];
        let mut entries = 0usize;
        while let Some((directory, depth)) = pending.pop() {
            service.check()?;
            ensure!(
                started.elapsed() < Duration::from_secs(30),
                "Profile indexing exceeded its time budget"
            );
            ensure!(depth <= MAX_DEPTH, "Profile directory depth exceeds limits");
            service.check_components(&directory)?;
            for entry in fs::read_dir(&directory)? {
                service.check()?;
                entries += 1;
                ensure!(
                    entries <= MAX_ENTRIES && started.elapsed() < Duration::from_secs(30),
                    "Profile indexing exceeds limits"
                );
                let entry = entry?;
                let meta = fs::symlink_metadata(entry.path())?;
                if !unlinked(&meta) {
                    continue;
                }
                if meta.is_dir() {
                    pending.push((entry.path(), depth + 1));
                } else if meta.is_file() {
                    ensure!(
                        state.assets.len() < MAX_ASSETS,
                        "Profile exceeds 100,000 indexed assets"
                    );
                    let relative = entry
                        .path()
                        .strip_prefix(&service.root)?
                        .to_str()
                        .context("Non-Unicode asset path")?
                        .replace('\\', "/");
                    let relative = logical(&relative, true)?;
                    ensure!(
                        state
                            .assets
                            .insert(
                                Key::new(&relative)?,
                                Asset {
                                    logical: relative,
                                    size: meta.len(),
                                    path: entry.path()
                                }
                            )
                            .is_none(),
                        "Case-insensitive duplicate profile asset"
                    );
                }
            }
        }
        for target in mutable {
            let target = logical(target, true)?;
            let key = Key::new(&target)?;
            let before = state.assets.get(&key).cloned();
            state.mutable.insert(key, before);
        }
        for (target, source) in aliases {
            let target = logical(target, true)?;
            let source = logical(source, true)?;
            let key = Key::new(&target)?;
            ensure!(
                !state.assets.contains_key(&key) || state.mutable.contains_key(&key),
                Failure::Invalid
            );
            let mut asset = state
                .assets
                .get(&Key::new(&source)?)
                .ok_or(Failure::Missing)?
                .clone();
            asset.logical = target;
            state.assets.insert(key, asset);
            state.aliases += 1;
        }
        service.state = Mutex::new(state);
        service.verify_root()?;
        Ok(service)
    }
    fn check(&self) -> Result<()> {
        ensure!(!self.stop.load(Ordering::Acquire), "Provider is closing");
        Ok(())
    }
    fn verify_root(&self) -> Result<()> {
        self.check()?;
        ensure!(
            same_path(&final_path(&self.root_handle)?, &self.root)?,
            "Mod profile root moved"
        );
        Ok(())
    }
    fn check_components(&self, path: &Path) -> Result<()> {
        self.verify_root()?;
        let relative = path
            .strip_prefix(&self.root)
            .context("Asset escaped selected profile")?;
        let mut current = self.root.clone();
        ensure!(
            unlinked(&fs::symlink_metadata(&current)?),
            "Profile root became linked"
        );
        for p in relative.components() {
            current.push(p);
            ensure!(unlinked(&fs::symlink_metadata(&current)?), Failure::Invalid);
        }
        Ok(())
    }
    pub fn counts(&self) -> Result<(usize, usize, usize)> {
        self.check()?;
        let s = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Provider state unavailable"))?;
        Ok((s.assets.len(), s.aliases, s.cache.len() + s.jukebox.len()))
    }
    pub fn query(&self, name: &str) -> Result<Asset> {
        self.check()?;
        let key = Key::new(&logical(name, false)?)?;
        self.state
            .lock()
            .map_err(|_| anyhow::anyhow!("Provider state unavailable"))?
            .assets
            .get(&key)
            .cloned()
            .ok_or_else(|| Failure::Missing.into())
    }
    fn open_asset(&self, asset: &Asset) -> Result<Arc<Pinned>> {
        self.check_components(&asset.path)?;
        let file = fs::OpenOptions::new()
            .read(true)
            .share_mode(7)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(&asset.path)?;
        let metadata = file.metadata()?;
        ensure!(
            metadata.is_file() && unlinked(&metadata) && metadata.len() == asset.size,
            "Asset changed after indexing"
        );
        let actual = final_path(&file)?;
        ensure!(
            contained(&self.root, &actual)? && same_path(&actual, &asset.path)?,
            "Opened asset escaped its indexed profile path"
        );
        self.check_components(&asset.path)?;
        Ok(Arc::new(Pinned {
            file,
            size: asset.size,
        }))
    }
    pub fn read(&self, name: &str, offset: u64, length: u32) -> Result<(Asset, Vec<u8>)> {
        self.read_with(name, offset, length, || Ok(()))
    }
    fn read_with(
        &self,
        name: &str,
        offset: u64,
        length: u32,
        before_read: impl FnOnce() -> Result<()>,
    ) -> Result<(Asset, Vec<u8>)> {
        self.check()?;
        ensure!(length <= wire::MAX_READ, Failure::Invalid);
        let key = Key::new(&logical(name, false)?)?;
        let virtual_asset = {
            let state = self
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("Provider state unavailable"))?;
            state.virtuals.get(&key).map(|data| {
                (
                    state
                        .assets
                        .get(&key)
                        .cloned()
                        .expect("Immutable provider metadata"),
                    data.clone(),
                )
            })
        };
        if let Some((asset, data)) = virtual_asset {
            ensure!(offset <= asset.size, Failure::Invalid);
            before_read()?;
            self.check()?;
            let end = (offset + length as u64).min(asset.size);
            return Ok((asset, data[offset as usize..end as usize].to_vec()));
        }
        let (asset, pinned) = {
            let mut s = self
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("Provider state unavailable"))?;
            let asset = s.assets.get(&key).ok_or(Failure::Missing)?.clone();
            ensure!(offset <= asset.size, Failure::Invalid);
            let pinned = if let Some(pinned) = s.jukebox.get(&key) {
                pinned.clone()
            } else if s.mutable.contains_key(&key) {
                self.open_asset(&asset)?
            } else if let Some(cached) = s.cache.get(&key) {
                cached.clone()
            } else {
                let file = self.open_asset(&asset)?;
                // Cached handles are an optimization, never a new asset limit.
                // Eviction is safe because active requests retain their own Arc.
                self.check()?;
                if s.cache.len() >= MAX_CACHED_FILES {
                    s.cache.pop_first();
                }
                s.cache.insert(key.clone(), file.clone());
                file
            };
            (asset, pinned)
        };
        self.check()?;
        let before = pinned.file.metadata()?;
        ensure!(
            before.len() == pinned.size && before.is_file(),
            "Asset size changed after indexing"
        );
        before_read()?;
        let mut data = vec![0; (asset.size - offset).min(length as u64) as usize];
        let mut received = 0;
        while received < data.len() {
            self.check()?;
            let count = pinned
                .file
                .seek_read(&mut data[received..], offset + received as u64)?;
            if count == 0 {
                break;
            }
            received += count;
        }
        let after = pinned.file.metadata()?;
        ensure!(
            before.len() == after.len() && before.last_write_time() == after.last_write_time(),
            "Asset changed during provider read"
        );
        ensure!(
            received == data.len(),
            "Asset ended before its indexed size"
        );
        self.check()?;
        Ok((asset, data))
    }
    pub fn set_alias(&self, target: &str, source: Option<&str>) -> Result<Option<Asset>> {
        self.set_alias_checked(target, source, || Ok(()))
    }
    pub fn set_alias_checked(
        &self,
        target: &str,
        source: Option<&str>,
        publish: impl FnOnce() -> Result<()>,
    ) -> Result<Option<Asset>> {
        self.check()?;
        let target = logical(target, true)?;
        let key = Key::new(&target)?;
        let mut s = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Provider state unavailable"))?;
        let original = s.mutable.get(&key).ok_or(Failure::Unauthorized)?.clone();
        let next = if let Some(source) = source {
            let source = logical(source, true)?;
            let file = self.root.join(source.replace('/', "\\"));
            self.check_components(&file)?;
            let meta = fs::symlink_metadata(&file)?;
            ensure!(
                meta.is_file() && meta.len() > 0 && meta.len() <= MAX_ALIAS_BYTES,
                Failure::Invalid
            );
            let asset = Asset {
                logical: target,
                size: meta.len(),
                path: file,
            };
            self.open_asset(&asset)?; // Validate the final native path before publication.
            Some(asset)
        } else {
            original
        };
        self.check()?;
        publish()?;
        if let Some(asset) = &next {
            s.assets.insert(key.clone(), asset.clone());
        } else {
            s.assets.remove(&key);
        }
        s.cache.remove(&key);
        Ok(next)
    }
    pub fn binary(&self, request: &Request) -> Result<Vec<u8>> {
        match request.operation {
            1 => {
                ensure!(request.offset == 0 && request.length == 0, Failure::Invalid);
                let a = self.query(&request.path)?;
                Ok(wire::encode_success(1, a.size, &[])?)
            }
            2 => {
                let (a, data) = self.read(&request.path, request.offset, request.length)?;
                Ok(wire::encode_success(2, a.size, &data)?)
            }
            _ => Err(Failure::Invalid.into()),
        }
    }
    /// Caller must authorize the native pipe client before invoking this.
    pub fn binary_reply(&self, payload: &[u8]) -> Vec<u8> {
        match wire::parse_request(payload)
            .map_err(anyhow::Error::from)
            .and_then(|r| self.binary(&r))
        {
            Ok(bytes) => bytes,
            Err(error) => wire::encode_error(status(&error)),
        }
    }
    pub fn cancel(&self) {
        self.stop.store(true, Ordering::Release);
    }
    /// Called only after the transport thread and every read lease drained.
    /// Cancellation alone is not handle release: retained cache handles can
    /// otherwise defer NTFS link deletion past receipt/profile restoration.
    pub(crate) fn release_closed_handles(&self) -> Result<()> {
        ensure!(
            self.stop.load(Ordering::Acquire),
            "Stop the provider before releasing cached files"
        );
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Provider state unavailable"))?;
        state.cache.clear();
        state.jukebox.clear();
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::{
        os::windows::{ffi::OsStrExt, process::CommandExt},
        process::Command,
    };
    use windows::{
        Win32::Storage::FileSystem::{
            FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_SYSTEM, SetFileAttributesW,
        },
        core::PCWSTR,
    };
    fn bytes(value: &Value) -> Vec<u8> {
        value
            .as_str()
            .unwrap()
            .as_bytes()
            .chunks_exact(2)
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect()
    }
    fn root() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "rust-provider-files-{}",
            crate::backend::unique_id()
        ));
        fs::create_dir(&p).unwrap();
        p
    }
    fn write(root: &Path, name: &str, data: &[u8]) {
        let path = root.join(name.replace('/', "\\"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, data).unwrap();
    }
    fn stop() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }
    fn fixture() -> Value {
        serde_json::from_str(include_str!("../assets/provider-assets-regressions.json")).unwrap()
    }
    fn selector_fixture(root: &Path) -> [Vec<u8>; 3] {
        let bytes = crate::selector_tables::tests::bundle();
        for (name, data) in crate::selector_tables::PATHS.iter().zip(&bytes) {
            write(root, name, data);
        }
        bytes
    }
    #[test]
    fn owned_jukebox_chunks_are_immutable_across_cache_eviction_and_release_before_restore() {
        use crate::jukebox_launch::tests::{Fixture, prepare_fixture, selected, stock};
        let f = Fixture::new();
        let original = crate::jukebox_package::build_music(&stock()).unwrap();
        fs::write(f.mods.join("music.iff"), &original).unwrap();
        let assets = Service::open(&f.mods, &[], &[], stop()).unwrap();
        assets.preflight_jukebox().unwrap();
        let (_, plan) = selected(&f, true);
        let session = prepare_fixture(&f, &plan, &"e".repeat(48))
            .unwrap()
            .unwrap();
        assets.publish_jukebox(&session).unwrap();
        assert!(assets.publish_jukebox(&session).is_err());
        let music = fs::read(f.mods.join("music.iff")).unwrap();
        let audio = crate::jukebox_catalog::audio_path(1);
        let bytes = fs::read(f.mods.join(&audio)).unwrap();
        assert_ne!(music, original);
        assert_eq!(
            assets.read("music.iff", 0, wire::MAX_READ).unwrap().1,
            music
        );
        assert_eq!(assets.read(&audio, 0, wire::MAX_READ).unwrap().1, bytes);
        assets.state.lock().unwrap().cache.clear();
        assert_eq!(assets.read(&audio, 4, 17).unwrap().1, bytes[4..21]);
        assert!(fs::write(f.mods.join(&audio), b"foreign").is_err());
        assert!(fs::remove_file(f.mods.join("music.iff")).is_err());
        assert!(session.restore(|| Ok(())).is_err());
        assert!(crate::jukebox_launch::pending(&f.root).unwrap());
        assets.cancel();
        assets.release_closed_handles().unwrap();
        session.restore(|| Ok(())).unwrap();
        assert_eq!(fs::read(f.mods.join("music.iff")).unwrap(), original);
        assert!(!f.mods.join(&audio).exists());
        assert!(!crate::jukebox_launch::pending(&f.root).unwrap());
        drop(session);
        drop(assets);
    }
    #[test]
    fn jukebox_preflight_preserves_aliases_and_mutable_routes_without_any_publication() {
        let root = root();
        write(&root, "user.iff", b"user data");
        let assets = Service::open(
            &root,
            &[("music.iff".into(), "user.iff".into())],
            &[],
            stop(),
        )
        .unwrap();
        assert!(assets.preflight_jukebox().is_err());
        assert_eq!(assets.read("music.iff", 0, 100).unwrap().1, b"user data");
        assert!(!root.join("music.iff").exists());
        drop(assets);
        let assets =
            Service::open(&root, &[], &[crate::jukebox_catalog::audio_path(1)], stop()).unwrap();
        assert!(assets.preflight_jukebox().is_err());
        assert_eq!(fs::read(root.join("user.iff")).unwrap(), b"user data");
        drop(assets);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn fixed_scoreboard_virtual_bundle_is_atomic_case_safe_and_never_publishes_files() {
        let files: BTreeMap<_, _> = crate::scoreboard_layout::PATHS
            .iter()
            .map(|name| ((*name).to_owned(), format!("fixture:{name}").into_bytes()))
            .collect();
        for collision in [
            None,
            Some("GOOEY/MYPARKSCOREBUGSIMPLE.IFF"),
            Some("myparkscorebugsimple.iff"),
        ] {
            let root = root();
            if let Some(name) = collision {
                write(&root, name, b"foreign mod");
            }
            let service = Service::open(&root, &[], &[], stop()).unwrap();
            let before = service.counts().unwrap();
            let result = service.publish_scoreboard_files(&files);
            assert_eq!(result.is_ok(), collision.is_none());
            if let Some(name) = collision {
                assert_eq!(service.counts().unwrap(), before);
                assert_eq!(service.read(name, 0, 100).unwrap().1, b"foreign mod");
                assert!(service.query("gooey/thewscrimmagescorebug.iff").is_err());
            } else {
                for (path, bytes) in &files {
                    assert_eq!(service.read(path, 0, 1024).unwrap().1, *bytes);
                    assert_eq!(
                        service
                            .read(path.strip_prefix("gooey/").unwrap(), 0, 1024)
                            .unwrap()
                            .1,
                        *bytes
                    );
                    assert!(!root.join(path).exists());
                }
                assert!(service.publish_scoreboard_files(&files).is_err());
            }
            drop(service);
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn player_package_and_companions_are_served_at_native_paths_then_cleaned_without_copies() {
        let base = root();
        let profile = base.join("mods");
        fs::create_dir(&profile).unwrap();
        fs::create_dir_all(base.join("portable/Data")).unwrap();
        write(&profile, "players/Amir Johnson/png1452.iff", b"player data");
        write(
            &profile,
            "players/Amir Johnson/png1452_geo_hair.iff",
            b"hair data",
        );
        let service = Service::open(&profile, &[], &[], stop()).unwrap();
        assert!(service.query("char/sig/png1452.iff").is_err());
        let session = crate::backend::unique_id();
        let mut store = crate::player_folders::Store::create(
            &base.join("portable"),
            &profile,
            &session,
            || Ok(()),
        )
        .unwrap()
        .unwrap();
        store.activate(|| Ok(())).unwrap();
        service.index_player_links(&store).unwrap();
        assert_eq!(
            service.read("char/sig/png1452.iff", 0, 100).unwrap().1,
            b"player data"
        );
        assert_eq!(
            service
                .read("char/sig/png1452_geo_hair.iff", 0, 100)
                .unwrap()
                .1,
            b"hair data"
        );
        assert!(service.index_player_links(&store).is_err());
        // Match production shutdown: cancel the server while cached provider
        // file handles still exist, then restore links before the Arc drops.
        service.cancel();
        service.release_closed_handles().unwrap();
        store.restore(|| Ok(())).unwrap();
        drop(store);
        assert!(!profile.join("char/sig/png1452.iff").exists());
        assert_eq!(
            fs::read(profile.join("players/Amir Johnson/png1452.iff")).unwrap(),
            b"player data"
        );
        drop(service);
        fs::remove_dir_all(base).unwrap();
    }
    #[test]
    #[ignore = "Manual read-only audit of an explicitly supplied mod folder; never run by the ordinary suite"]
    fn explicit_read_only_selected_mod_folder_audit() {
        use sha2::{Digest, Sha256};
        let root = PathBuf::from(
            std::env::var("NBA2K27_SELECTOR_AUDIT_ROOT")
                .expect("Explicit audit mod-folder path required"),
        );
        let started = std::time::Instant::now();
        let service = Service::open(&root, &[], &[], stop()).unwrap();
        let indexed = service.counts().unwrap().0;
        let summary = service.prepare_selectors().unwrap();
        assert!(summary.enabled);
        for name in crate::selector_tables::PATHS {
            let data = fs::read(root.join(name)).unwrap();
            assert_eq!(summary.sha256[name], format!("{:X}", Sha256::digest(&data)));
            let mut output = Vec::new();
            while output.len() < data.len() {
                output.extend(
                    service
                        .read(name, output.len() as u64, wire::MAX_READ)
                        .unwrap()
                        .1,
                );
            }
            assert_eq!(output, data);
        }
        println!(
            "{}",
            json!({"summary":summary,"indexedAssets":indexed,"elapsedMs":started.elapsed().as_millis(),"gameMemoryAccess":false,"gameFilesChanged":false,"userModFilesChanged":false})
        );
    }
    #[test]
    fn coordinated_selectors_are_session_frozen_without_touching_mods() {
        let root = root();
        let bytes = selector_fixture(&root);
        write(&root, "other/mod.iff", b"unchanged ordinary mod");
        let service = Service::open(&root, &[], &[], stop()).unwrap();
        let (probe, _, _) = service.add_probe(&crate::backend::unique_id()).unwrap();
        let summary = service.prepare_selectors().unwrap();
        assert!(summary.enabled);
        assert_eq!(summary.players, 1);
        assert_eq!(
            service.read("other/mod.iff", 0, 100).unwrap().1,
            b"unchanged ordinary mod"
        );
        assert!(!service.read(&probe, 0, 100).unwrap().1.is_empty());
        for (name, expected) in crate::selector_tables::PATHS.iter().zip(&bytes) {
            assert_eq!(fs::read(root.join(name)).unwrap(), *expected);
            // A later editor changes the user's file, not this game's frozen
            // bundle. Provider requests cannot observe mixed generations.
            fs::write(root.join(name), b"later user edit").unwrap();
            assert_eq!(service.read(name, 0, wire::MAX_READ).unwrap().1, *expected);
        }
        assert!(service.prepare_selectors().is_err());
        drop(service);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn selector_preflight_refuses_partial_or_corrupt_bundle_atomically() {
        let root = root();
        let service = Service::open(&root, &[], &[], stop()).unwrap();
        assert!(!service.prepare_selectors().unwrap().enabled);
        drop(service);
        let bytes = selector_fixture(&root);
        fs::remove_file(root.join(crate::selector_tables::PATHS[2])).unwrap();
        let service = Service::open(&root, &[], &[], stop()).unwrap();
        assert!(service.prepare_selectors().is_err());
        assert!(service.state.lock().unwrap().virtuals.is_empty());
        drop(service);
        write(&root, crate::selector_tables::PATHS[2], &bytes[2]);
        write(&root, crate::selector_tables::PATHS[1], b"bad zip");
        let service = Service::open(&root, &[], &[], stop()).unwrap();
        assert!(service.prepare_selectors().is_err());
        assert!(service.state.lock().unwrap().virtuals.is_empty());
        assert_eq!(
            fs::read(root.join(crate::selector_tables::PATHS[0])).unwrap(),
            bytes[0]
        );
        drop(service);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn selector_snapshot_denies_source_edits_and_cancelled_publication() {
        let root = root();
        let bytes = selector_fixture(&root);
        let service = Service::open(&root, &[], &[], stop()).unwrap();
        assert!(
            service
                .prepare_selectors_with(|| {
                    for name in crate::selector_tables::PATHS {
                        assert!(
                            fs::write(root.join(name), b"cannot replace pinned source").is_err()
                        );
                    }
                    service.cancel();
                    Ok(())
                })
                .is_err()
        );
        assert!(service.state.lock().unwrap().virtuals.is_empty());
        for (name, expected) in crate::selector_tables::PATHS.iter().zip(bytes) {
            assert_eq!(fs::read(root.join(name)).unwrap(), expected);
        }
        drop(service);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn validated_bundle_enables_only_the_native_selector_allowlist() {
        let root = root();
        selector_fixture(&root);
        fs::create_dir(root.join("App")).unwrap();
        write(&root, "Data/settings.json", b"{}");
        let service = Service::open(&root, &[], &[], stop()).unwrap();
        let (name, size, sha) = service.add_probe(&crate::backend::unique_id()).unwrap();
        let selectors = service.prepare_selectors().unwrap();
        let endpoint = format!(
            "{}{}",
            crate::provider_protocol::PIPE_PREFIX,
            "a".repeat(64)
        );
        let config = crate::loader_files::configuration(
            &endpoint,
            &name,
            size,
            &sha,
            &root,
            &[],
            selectors.enabled,
        )
        .unwrap();
        let config = String::from_utf8(config).unwrap();
        assert!(config.lines().any(|s| s == "SELECTOR_OVERRIDE=1"));
        assert!(!config.lines().any(|s| s.starts_with("OVERRIDE=")));
        drop(service);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn immutable_memory_probe_supports_empty_profiles_without_writing_files() {
        use sha2::{Digest, Sha256};
        let root = root();
        let s = Service::open(&root, &[], &[], stop()).unwrap();
        let (name, size, sha) = s.add_probe(&crate::backend::unique_id()).unwrap();
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
        assert_eq!(s.query(&name).unwrap().size, size);
        let (_, data) = s.read(&name, 0, wire::MAX_READ).unwrap();
        assert_eq!(format!("{:X}", Sha256::digest(&data)), sha);
        for offset in [0, 5, size] {
            let (_, part) = s.read(&name, offset, 7).unwrap();
            assert_eq!(
                part,
                &data[offset as usize..(offset + 7).min(size) as usize]
            );
        }
        assert!(s.read(&name, size + 1, 1).is_err());
        assert!(s.add_probe(&crate::backend::unique_id()).is_err());
        assert!(s.set_alias("test.iff", Some(&name)).is_err());
        assert!(s.add_probe("wrong").is_err());
        drop(s);
        fs::remove_dir(root).unwrap();
    }
    #[test]
    fn cancelled_memory_probe_does_not_return_data_or_publish_another_asset() {
        let root = root();
        let s = Service::open(&root, &[], &[], stop()).unwrap();
        let (name, _, _) = s.add_probe(&crate::backend::unique_id()).unwrap();
        assert!(
            s.read_with(&name, 0, 10, || {
                s.cancel();
                Ok(())
            })
            .is_err()
        );
        assert!(s.query(&name).is_err());
        drop(s);
        fs::remove_dir(root).unwrap();
    }
    #[test]
    fn original_provider_file_routing_aliases_and_reads_match() {
        let c = fixture();
        let root = root();
        assert_eq!(c["steps"].as_array().unwrap().len(), 71);
        for f in c["files"].as_array().unwrap() {
            let seed = bytes(&f["data"]);
            let count = f["count"].as_u64().unwrap() as usize;
            let data: Vec<_> = (0..count).map(|i| seed[i % seed.len()]).collect();
            write(&root, f["path"].as_str().unwrap(), &data);
        }
        let hidden: Vec<_> = root
            .join("hidden.iff")
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        unsafe {
            SetFileAttributesW(
                PCWSTR(hidden.as_ptr()),
                FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM,
            )
            .unwrap();
        }
        let aliases: Vec<_> = c["aliases"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap().into()))
            .collect();
        let mutable: Vec<_> = c["mutable"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().into())
            .collect();
        let service = Service::open(&root, &aliases, &mutable, stop()).unwrap();
        assert_eq!(service.counts().unwrap().0, 12);
        assert_eq!(service.counts().unwrap().1, 2);
        for (i, step) in c["steps"].as_array().unwrap().iter().enumerate() {
            let spec = &step["input"];
            let path = spec["path"].as_str().unwrap();
            let result = (|| -> Result<Value> {
                Ok(match spec["kind"].as_str().unwrap() {
                    "query" => {
                        let a = service.query(path)?;
                        json!({"logicalPath":a.logical,"size":a.size})
                    }
                    "read" => {
                        let (a, data) = service.read(
                            path,
                            spec["offset"].as_str().unwrap().parse()?,
                            spec["length"].as_u64().unwrap() as u32,
                        )?;
                        json!({"logicalPath":a.logical,"size":a.size,"data":data.iter().map(|b|format!("{b:02X}")).collect::<String>()})
                    }
                    "alias" => {
                        let a = service.set_alias(path, spec["source"].as_str())?;
                        json!({"logicalPath":path.replace('\\',"/"),"size":a.as_ref().map_or(0,|a|a.size),"available":a.is_some()})
                    }
                    "create" => {
                        write(
                            &root,
                            path,
                            &[0x99, 0xAA, 0xBB, 0xCC]
                                .into_iter()
                                .cycle()
                                .take(17)
                                .collect::<Vec<_>>(),
                        );
                        json!({"created":true})
                    }
                    _ => unreachable!(),
                })
            })();
            assert_eq!(
                result.is_ok(),
                step["ok"] == true,
                "step {i} {spec}: {result:?}"
            );
            match result {
                Ok(result) => assert_eq!(result, step["expected"], "step {i}"),
                Err(error) => assert_eq!(
                    status(&error) as u64,
                    step["status"].as_u64().unwrap(),
                    "step {i}: {error:#}"
                ),
            }
        }
        for pair in c["comparisons"].as_array().unwrap() {
            assert_eq!(
                Key::new(pair["left"].as_str().unwrap()).unwrap()
                    == Key::new(pair["right"].as_str().unwrap()).unwrap(),
                pair["equal"] == true,
                "{pair}"
            );
        }
        drop(service);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn binary_reads_are_bounded_positioned_and_concurrent() {
        let root = root();
        let data: Vec<_> = (0..4097).map(|i| (i % 251) as u8).collect();
        write(&root, "char/test.iff", &data);
        let s = Arc::new(Service::open(&root, &[], &[], stop()).unwrap());
        let mut threads = vec![];
        for i in 0..16 {
            let (s, data) = (s.clone(), data.clone());
            threads.push(std::thread::spawn(move || {
                for n in 0..30 {
                    let at = (i * 97 + n * 17) % 4000;
                    let (_, actual) = s.read("CHAR\\TEST.IFF", at as u64, 97).unwrap();
                    assert_eq!(actual, data[at..at + 97]);
                }
            }));
        }
        for t in threads {
            t.join().unwrap();
        }
        assert!(s.read("char/test.iff", 4098, 0).is_err());
        assert!(s.read("char/test.iff", 0, wire::MAX_READ + 1).is_err());
        assert!(s.read("../char/test.iff", 0, 1).is_err());
        assert!(s.read("char/test.iff:stream", 0, 1).is_err());
        assert!(s.query("test.iff").is_err());
        assert!(
            s.read("char/test.iff", 4097, wire::MAX_READ)
                .unwrap()
                .1
                .is_empty()
        );
        let r = s
            .binary(&Request {
                operation: 2,
                offset: 4090,
                length: 17,
                path: "char/test.iff".into(),
            })
            .unwrap();
        assert_eq!(r, wire::encode_success(2, 4097, &data[4090..]).unwrap());
        let r = s
            .binary(&Request {
                operation: 1,
                offset: 0,
                length: 0,
                path: "char/test.iff".into(),
            })
            .unwrap();
        assert_eq!(r, wire::encode_success(1, 4097, &[]).unwrap());
        assert!(
            s.binary(&Request {
                operation: 1,
                offset: 1,
                length: 0,
                path: "char/test.iff".into()
            })
            .is_err()
        );
        assert_eq!(fs::read(root.join("char/test.iff")).unwrap(), data);
        drop(s);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn cancellation_and_changed_files_never_return_unverified_payload() {
        let root = root();
        write(&root, "test.iff", &[1; 17]);
        let stop = stop();
        let s = Service::open(&root, &[], &[], stop.clone()).unwrap();
        assert!(
            s.read_with("test.iff", 0, 17, || {
                fs::write(root.join("test.iff"), [2; 18])?;
                Ok(())
            })
            .is_err()
        );
        assert!(s.read("test.iff", 0, 17).is_err());
        stop.store(true, Ordering::Release);
        assert!(s.query("test.iff").is_err());
        assert!(s.counts().is_err());
        assert!(s.set_alias("x", None).is_err());
        assert!(Service::open(&root, &[], &[], stop.clone()).is_err());
        drop(s);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn native_containment_does_not_reuse_logical_unicode_equivalence() {
        assert_eq!(Key::new("Σ").unwrap(), Key::new("ς").unwrap());
        assert!(!same_path(Path::new(r"C:\profile-Σ"), Path::new(r"C:\profile-ς")).unwrap());
        assert!(
            !contained(
                Path::new(r"C:\profile-Σ"),
                Path::new(r"C:\profile-ς\secret.iff")
            )
            .unwrap()
        );
        assert!(
            contained(
                Path::new(r"C:\Profile"),
                Path::new(r"c:\profile\char\file.iff")
            )
            .unwrap()
        );
        assert!(
            !contained(
                Path::new(r"C:\Profile"),
                Path::new(r"C:\Profile-other\file.iff")
            )
            .unwrap()
        );
    }
    #[test]
    fn original_binary_failure_codes_are_preserved() {
        fn request(op: u16, offset: u64, length: u32, path: &str) -> Vec<u8> {
            let mut bytes = Vec::new();
            bytes.extend(0x31504232u32.to_le_bytes());
            bytes.extend(1u16.to_le_bytes());
            bytes.extend(op.to_le_bytes());
            bytes.extend(offset.to_le_bytes());
            bytes.extend(length.to_le_bytes());
            bytes.extend((path.len() as u32).to_le_bytes());
            bytes.extend(path.as_bytes());
            bytes
        }
        let root = root();
        write(&root, "test.iff", &[7; 9]);
        let stop = stop();
        let s = Service::open(&root, &[], &[], stop.clone()).unwrap();
        let valid = request(2, 0, 9, "test.iff");
        assert_eq!(
            s.binary_reply(&valid),
            wire::encode_success(2, 9, &[7; 9]).unwrap()
        );
        assert_eq!(
            s.binary_reply(&request(1, 0, 0, "missing.iff")),
            wire::encode_error(2)
        );
        for bytes in [
            request(2, 10, 1, "test.iff"),
            request(2, 0, wire::MAX_READ + 1, "test.iff"),
            request(1, 0, 0, "../test.iff"),
            request(3, 0, 0, "test.iff"),
        ] {
            assert_eq!(s.binary_reply(&bytes), wire::encode_error(3));
        }
        for n in 0..valid.len() {
            assert_eq!(s.binary_reply(&valid[..n]), wire::encode_error(3));
        }
        assert_eq!(
            status(
                &s.set_alias("not-mutable.iff", Some("test.iff"))
                    .unwrap_err()
            ),
            1
        );
        stop.store(true, Ordering::Release);
        assert_eq!(s.binary_reply(&valid), wire::encode_error(4));
        drop(s);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn immutable_cache_and_mutable_alias_match_wpf_replacement_behavior() {
        let root = root();
        write(&root, "a.iff", &[1; 17]);
        let s = Service::open(
            &root,
            &[("mutable.iff".into(), "a.iff".into())],
            &["mutable.iff".into()],
            stop(),
        )
        .unwrap();
        assert_eq!(s.read("a.iff", 0, 17).unwrap().1, [1; 17]);
        assert_eq!(s.read("mutable.iff", 0, 17).unwrap().1, [1; 17]);
        fs::rename(root.join("a.iff"), root.join("preserved.iff")).unwrap();
        write(&root, "a.iff", &[2; 17]);
        assert_eq!(s.read("a.iff", 0, 17).unwrap().1, [1; 17]);
        assert_eq!(s.read("mutable.iff", 0, 17).unwrap().1, [2; 17]);
        assert_eq!(s.counts().unwrap().2, 1);
        assert!(s.set_alias("a.iff", Some("preserved.iff")).is_err());
        assert!(s.set_alias("mutable.iff", None).unwrap().is_none());
        assert!(s.query("mutable.iff").is_err());
        drop(s);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn file_cache_is_bounded_without_restricting_indexed_assets() {
        let root = root();
        for i in 0..600 {
            write(
                &root,
                &format!("nested/file{i:04}.iff"),
                &(i as u32).to_le_bytes(),
            );
        }
        let started = Instant::now();
        let s = Service::open(&root, &[], &[], stop()).unwrap();
        let indexed_ms = started.elapsed().as_secs_f64() * 1000.;
        let started = Instant::now();
        for n in 0..10000 {
            assert_eq!(
                s.query(&format!("NESTED/FILE{:04}.IFF", n % 600))
                    .unwrap()
                    .size,
                4
            );
        }
        let query_ms = started.elapsed().as_secs_f64() * 1000.;
        for i in 0..600 {
            assert_eq!(
                s.read(&format!("nested/file{i:04}.iff"), 0, 4).unwrap().1,
                (i as u32).to_le_bytes()
            );
        }
        assert_eq!(s.counts().unwrap(), (600, 0, MAX_CACHED_FILES));
        assert_eq!(s.read("nested/file0000.iff", 0, 4).unwrap().1, [0; 4]);
        assert_eq!(s.counts().unwrap().2, MAX_CACHED_FILES);
        println!(
            "{}",
            json!({"providerAssets":600,"indexMs":indexed_ms,"queries":10000,"queryMs":query_ms,"cacheLimit":MAX_CACHED_FILES,"gameAccess":false})
        );
        drop(s);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn empty_profiles_alias_limits_and_cancelled_indexing_are_explicit() {
        let root = root();
        let s = Service::open(&root, &[], &[], stop()).unwrap();
        assert_eq!(s.counts().unwrap(), (0, 0, 0));
        assert_eq!(status(&s.query("missing.iff").unwrap_err()), 2);
        drop(s);
        write(&root, "a.iff", &[1; 3]);
        assert!(
            Service::open(
                &root,
                &vec![("x.iff".into(), "a.iff".into()); 9],
                &[],
                stop()
            )
            .is_err()
        );
        assert!(Service::open(&root, &[], &vec!["x.iff".into(); 5], stop()).is_err());
        assert!(Service::open(&root, &[("a.iff".into(), "a.iff".into())], &[], stop()).is_err());
        assert!(
            Service::open(
                &root,
                &[
                    ("x.iff".into(), "a.iff".into()),
                    ("X.IFF".into(), "a.iff".into())
                ],
                &[],
                stop()
            )
            .is_err()
        );
        let s = Service::open(
            &root,
            &[
                ("z.iff".into(), "a.iff".into()),
                ("chain.iff".into(), "z.iff".into()),
            ],
            &[],
            stop(),
        )
        .unwrap();
        assert_eq!(s.read("chain.iff", 0, 3).unwrap().1, [1; 3]);
        drop(s);
        let stop = Arc::new(AtomicBool::new(true));
        assert!(
            Service::open(&root.join("not-created"), &[], &[], stop)
                .err()
                .unwrap()
                .to_string()
                .contains("closing")
        );
        assert_eq!(fs::read(root.join("a.iff")).unwrap(), [1; 3]);
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }
    fn junction(link: &Path, target: &Path) {
        // Fixed script, native parameter values via environment (not shell
        // interpolation), and only unique private fixture paths.
        let r=Command::new("powershell.exe").args(["-NoProfile","-NonInteractive","-WindowStyle","Hidden","-Command",
            "$ErrorActionPreference='Stop'; New-Item -ItemType Junction -Path $env:NBA_TEST_PROVIDER_LINK -Target $env:NBA_TEST_PROVIDER_TARGET | Out-Null"])
            .env("NBA_TEST_PROVIDER_LINK",link).env("NBA_TEST_PROVIDER_TARGET",target).creation_flags(0x08000000).output().unwrap();
        assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    }
    #[test]
    fn reparse_points_cannot_escape_index_open_or_mutable_alias() {
        let base = root();
        let profile = base.join("profile");
        let outside = base.join("outside");
        fs::create_dir(&profile).unwrap();
        fs::create_dir(&outside).unwrap();
        write(&profile, "normal/a.iff", &[1; 3]);
        write(&outside, "secret.iff", &[9; 3]);
        write(&outside, "a.iff", &[7; 3]);
        let linked = profile.join("linked");
        junction(&linked, &outside);
        let s = Service::open(&profile, &[], &["mutable.iff".into()], stop()).unwrap();
        assert_eq!(s.counts().unwrap().0, 1);
        assert!(s.query("linked/secret.iff").is_err());
        assert!(
            s.set_alias("mutable.iff", Some("linked/secret.iff"))
                .is_err()
        );
        fs::remove_dir(&linked).unwrap();
        fs::remove_dir_all(profile.join("normal")).unwrap();
        junction(&profile.join("normal"), &outside);
        assert!(s.read("normal/a.iff", 0, 3).is_err());
        assert!(Service::open(&profile.join("normal"), &[], &[], stop()).is_err());
        drop(s);
        fs::remove_dir(profile.join("normal")).unwrap();
        assert_eq!(fs::read(outside.join("secret.iff")).unwrap(), [9; 3]);
        fs::remove_dir_all(base).unwrap();
    }
}
