//! Transactional native profile junctions. Never copies mod-library contents,
//! follows a foreign link, or restores while game presence is unknown/live.
#![allow(dead_code)]
use crate::{
    backend,
    loader_files::{self, Directories},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::Read,
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Path, PathBuf},
};
use windows::Win32::{Foundation::HANDLE, Storage::FileSystem::*, System::IO::DeviceIoControl};
const SCHEMA: &str = "nba2k27-rust-mod-profile-v1";
const MARKER: &str = "mod-profile-session.json";
const DELETE_ACCESS: u32 = 0x10000;
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DirectoryId {
    volume: u32,
    index: u64,
    birth: u64,
}
fn identity(file: &File) -> Result<DirectoryId> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    unsafe {
        GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info)?;
    }
    ensure!(
        info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0,
        "Expected profile directory"
    );
    Ok(DirectoryId {
        volume: info.dwVolumeSerialNumber,
        index: ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64,
        birth: ((info.ftCreationTime.dwHighDateTime as u64) << 32)
            | info.ftCreationTime.dwLowDateTime as u64,
    })
}
fn open_dir(path: &Path, write: bool) -> Result<File> {
    let file = fs::OpenOptions::new()
        .read(true)
        .access_mode(0x80000000 | DELETE_ACCESS | if write { 0x40000000 } else { 0 })
        .share_mode(1)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)?;
    // An OPEN_REPARSE_POINT junction is a symlink file type to Rust, even
    // though its native directory attribute is set. Do not follow it to test.
    ensure!(
        file.metadata()?.file_attributes() & FILE_ATTRIBUTE_DIRECTORY.0 != 0,
        "Expected profile directory"
    );
    Ok(file)
}
fn missing(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(e.into()),
    }
}
fn ordinary(path: &Path) -> Result<bool> {
    let m = fs::symlink_metadata(path)?;
    Ok(m.is_dir() && m.file_attributes() & 0x400 == 0)
}
fn same(a: &Path, b: &Path) -> bool {
    crate::native_session::same_path(&a.to_string_lossy(), b)
}
fn inside(a: &Path, b: &Path) -> bool {
    let aa = a.components().collect::<Vec<_>>();
    let bb = b.components().collect::<Vec<_>>();
    aa.len() < bb.len()
        && aa.iter().zip(&bb).all(|(a, b)| {
            a.as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(&b.as_os_str().to_string_lossy())
        })
}
fn no_cycle(game: &Path, selected: &Path) -> Result<()> {
    let live = game.join("mods");
    ensure!(
        !same(&live, selected) && !inside(&live, selected) && !inside(selected, &live),
        "A profile cannot be inside or contain the live mods folder"
    );
    Ok(())
}
fn set_junction(file: &File, target: &Path) -> Result<()> {
    let text = target.to_string_lossy();
    let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
    ensure!(
        text.len() >= 3 && text.as_bytes()[1] == b':' && text.as_bytes()[2] == b'\\',
        "Native profile links require a local-drive target"
    );
    let substitute = format!(r"\??\{text}").encode_utf16().collect::<Vec<_>>();
    let print = text.encode_utf16().collect::<Vec<_>>();
    let sub_bytes = substitute.len() * 2;
    let print_bytes = print.len() * 2;
    let data_length = 8 + sub_bytes + 2 + print_bytes + 2;
    ensure!(
        data_length <= 16376,
        "Profile link path exceeds native reparse bounds"
    );
    let mut b = Vec::with_capacity(data_length + 8);
    b.extend(0xa0000003u32.to_le_bytes());
    b.extend((data_length as u16).to_le_bytes());
    b.extend(0u16.to_le_bytes());
    b.extend(0u16.to_le_bytes());
    b.extend((sub_bytes as u16).to_le_bytes());
    b.extend(((sub_bytes + 2) as u16).to_le_bytes());
    b.extend((print_bytes as u16).to_le_bytes());
    for c in substitute
        .into_iter()
        .chain(Some(0))
        .chain(print)
        .chain(Some(0))
    {
        b.extend(c.to_le_bytes());
    }
    let mut returned = 0;
    unsafe {
        DeviceIoControl(
            HANDLE(file.as_raw_handle()),
            0x000900a4,
            Some(b.as_ptr() as _),
            b.len() as u32,
            None,
            0,
            Some(&mut returned),
            None,
        )?;
    }
    Ok(())
}
#[cfg(test)]
pub(crate) fn fixture_junction(path: &Path, target: &Path) -> Result<()> {
    fs::create_dir(path)?;
    set_junction(&open_dir(path, true)?, target)
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    schema: String,
    session: String,
    game: PathBuf,
    selected: PathBuf,
    backup: String,
    original: Option<DirectoryId>,
    link: Option<DirectoryId>,
    stage: String,
}
impl Record {
    fn validate(&self, game: &Path) -> Result<()> {
        ensure!(
            self.schema == SCHEMA
                && self.session.len() == 48
                && self.session.bytes().all(|b| b.is_ascii_hexdigit())
                && same(&self.game, game)
                && self.selected.is_absolute()
                && self.backup == format!(".2k-launcher-mods-{}.bak", self.session)
                && ["planned", "moved", "link-created", "active", "restoring"]
                    .contains(&self.stage.as_str()),
            "Invalid profile recovery ownership"
        );
        no_cycle(game, &self.selected)
    }
}
pub(crate) struct Store {
    portable: PathBuf,
    game: PathBuf,
    _pins: Vec<Directories>,
    record: Record,
}
impl Store {
    pub fn selected_root(&self) -> &Path {
        &self.record.selected
    }
    pub fn session(&self) -> &str {
        &self.record.session
    }
    fn path(&self) -> PathBuf {
        self.portable.join("Data").join(MARKER)
    }
    fn live(&self) -> PathBuf {
        self.game.join("mods")
    }
    fn backup(&self) -> PathBuf {
        self.game.join(&self.record.backup)
    }
    fn commit(&self) -> Result<()> {
        self.record.validate(&self.game)?;
        backend::atomic_json(&self.path(), &self.record)
    }
    pub fn create(
        portable: &Path,
        game: &Path,
        selected: &Path,
        session: &str,
        closed: impl Fn() -> Result<()>,
    ) -> Result<Option<Self>> {
        closed()?;
        let game = game.canonicalize()?;
        let selected = selected.canonicalize()?;
        if same(&selected, &game.join("mods")) {
            ensure!(ordinary(&selected)?, "Live mods is a foreign link");
            return Ok(None);
        }
        no_cycle(&game, &selected)?;
        let pins = vec![
            Directories::pin(portable)?,
            Directories::pin(&portable.join("Data"))?,
            Directories::pin(&game)?,
            Directories::pin(&selected)?,
        ];
        let live = game.join("mods");
        let original = if missing(&live)? {
            None
        } else {
            ensure!(
                ordinary(&live)?,
                "Existing live mods link/non-directory was left untouched"
            );
            Some(identity(&open_dir(&live, false)?)?)
        };
        let record = Record {
            schema: SCHEMA.into(),
            session: session.into(),
            game: game.clone(),
            selected,
            backup: format!(".2k-launcher-mods-{session}.bak"),
            original,
            link: None,
            stage: "planned".into(),
        };
        let store = Self {
            portable: portable.into(),
            game,
            _pins: pins,
            record,
        };
        store.record.validate(&store.game)?;
        ensure!(
            missing(&store.backup())?,
            "Preserved mods backup already exists; left untouched"
        );
        closed()?;
        backend::atomic_json_new(&store.path(), &store.record)?;
        Ok(Some(store))
    }
    pub fn activate(&mut self, closed: impl Fn() -> Result<()>) -> Result<()> {
        ensure!(
            self.record.stage == "planned",
            "Profile activation was already attempted"
        );
        closed()?;
        if let Some(expected) = &self.record.original {
            let original = open_dir(&self.live(), false)?;
            ensure!(
                ordinary(&self.live())? && identity(&original)? == *expected,
                "Original mods folder changed"
            );
            closed()?;
            loader_files::publish(&original, &self.backup())?;
            drop(original);
        } else {
            ensure!(
                missing(&self.live())?,
                "Live mods appeared after intent; left untouched"
            );
        }
        self.record.stage = "moved".into();
        self.commit()?;
        closed()?;
        fs::create_dir(self.live())?;
        let link = open_dir(&self.live(), true)?;
        self.record.link = Some(identity(&link)?);
        self.record.stage = "link-created".into();
        if let Err(error) = self.commit() {
            let _ = loader_files::unlink(&link);
            return Err(error);
        }
        closed()?;
        set_junction(&link, &self.record.selected)?;
        drop(link);
        ensure!(
            same(&self.live().canonicalize()?, &self.record.selected),
            "Profile junction target differs"
        );
        self.record.stage = "active".into();
        self.commit()?;
        Ok(())
    }
    pub fn recover(portable: &Path, game: &Path) -> Result<Option<Self>> {
        let game = game.canonicalize()?;
        let pins = vec![
            Directories::pin(portable)?,
            Directories::pin(&portable.join("Data"))?,
            Directories::pin(&game)?,
        ];
        let path = portable.join("Data").join(MARKER);
        if missing(&path)? {
            return Ok(None);
        }
        let file = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(&path)?;
        let m = file.metadata()?;
        ensure!(
            m.is_file() && m.file_attributes() & 0x400 == 0 && m.len() <= 65536,
            "Invalid profile marker file"
        );
        let mut bytes = Vec::new();
        file.take(65537).read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 65536, "Profile marker exceeds limit");
        let record: Record = serde_json::from_slice(&bytes)?;
        record.validate(&game)?;
        // Recovery never needs to traverse or modify the selected profile.
        Ok(Some(Self {
            portable: portable.into(),
            game,
            _pins: pins,
            record,
        }))
    }
    pub fn restore(&mut self, closed: impl Fn() -> Result<()>) -> Result<()> {
        closed()?;
        let backup = if let Some(expected) = &self.record.original {
            if missing(&self.backup())? {
                if !missing(&self.live())?
                    && ordinary(&self.live())?
                    && identity(&open_dir(&self.live(), false)?)? == *expected
                {
                    return self.clear_marker(closed);
                }
                anyhow::bail!(
                    "Preserved original mods folder is missing; live path left untouched"
                );
            }
            ensure!(
                ordinary(&self.backup())?,
                "Preserved original mods is a foreign link"
            );
            let backup = open_dir(&self.backup(), false)?;
            ensure!(
                identity(&backup)? == *expected,
                "Preserved original mods identity changed"
            );
            Some(backup)
        } else {
            ensure!(
                missing(&self.backup())?,
                "Unexpected preserved mods folder; left untouched"
            );
            None
        };
        if !missing(&self.live())? {
            let link = open_dir(&self.live(), false)?;
            ensure!(
                Some(identity(&link)?) == self.record.link,
                "Live mods path no longer belongs to this session; left untouched"
            );
            if link.metadata()?.file_attributes() & 0x400 != 0 {
                let target = fs::read_link(self.live())?;
                ensure!(
                    same(&target, &self.record.selected),
                    "Live mods link points elsewhere; left untouched"
                );
            } else {
                ensure!(
                    fs::read_dir(self.live())?.next().is_none(),
                    "Interrupted profile directory contains foreign files; left untouched"
                );
            }
            self.record.stage = "restoring".into();
            self.commit()?;
            closed()?;
            loader_files::unlink(&link)?;
            drop(link);
        }
        if let Some(backup) = backup {
            closed()?;
            loader_files::publish(&backup, &self.live())?;
            drop(backup);
        }
        self.clear_marker(closed)
    }
    fn clear_marker(&self, closed: impl Fn() -> Result<()>) -> Result<()> {
        let mut file = fs::OpenOptions::new()
            .read(true)
            .access_mode(0x80000000 | DELETE_ACCESS)
            .share_mode(1)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(self.path())?;
        let m = file.metadata()?;
        ensure!(
            m.is_file() && m.file_attributes() & 0x400 == 0 && m.len() <= 65536,
            "Profile marker changed"
        );
        let mut b = Vec::new();
        (&mut file).take(65537).read_to_end(&mut b)?;
        ensure!(
            serde_json::from_slice::<Record>(&b)? == self.record,
            "Profile marker changed; left untouched"
        );
        closed()?;
        loader_files::unlink(&file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture {
        root: PathBuf,
        game: PathBuf,
        portable: PathBuf,
        selected: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("rust-mod-profile-{}", backend::unique_id()));
            let game = root.join("Game");
            let portable = root.join("Portable");
            let selected = root.join("Selected");
            for p in [&game, &portable.join("Data"), &selected] {
                fs::create_dir_all(p).unwrap();
            }
            fs::write(selected.join("user-mod.iff"), b"selected bytes").unwrap();
            Self {
                root,
                game,
                portable,
                selected,
            }
        }
        fn store(&self) -> Store {
            Store::create(
                &self.portable,
                &self.game,
                &self.selected,
                &backend::unique_id(),
                || Ok(()),
            )
            .unwrap()
            .unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let live = self.game.join("mods");
            if fs::symlink_metadata(&live).is_ok_and(|m| m.file_attributes() & 0x400 != 0) {
                let _ = fs::remove_dir(&live);
            }
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    #[test]
    fn native_profile_switch_is_a_junction_not_a_copy_and_preserves_original() {
        let f = Fixture::new();
        fs::create_dir(f.game.join("mods")).unwrap();
        fs::write(f.game.join("mods/original.iff"), b"original bytes").unwrap();
        let mut s = f.store();
        s.activate(|| Ok(())).unwrap();
        assert_eq!(
            fs::read(f.game.join("mods/user-mod.iff")).unwrap(),
            b"selected bytes"
        );
        assert_ne!(
            fs::symlink_metadata(f.game.join("mods"))
                .unwrap()
                .file_attributes()
                & 0x400,
            0
        );
        assert!(s.backup().join("original.iff").exists());
        s.restore(|| Ok(())).unwrap();
        assert_eq!(
            fs::read(f.game.join("mods/original.iff")).unwrap(),
            b"original bytes"
        );
        assert!(!f.game.join("mods/user-mod.iff").exists());
        assert_eq!(
            fs::read(f.selected.join("user-mod.iff")).unwrap(),
            b"selected bytes"
        );
        assert!(!s.path().exists());
    }
    #[test]
    fn missing_original_restores_to_missing_and_same_live_profile_is_noop() {
        let f = Fixture::new();
        let mut s = f.store();
        s.activate(|| Ok(())).unwrap();
        s.restore(|| Ok(())).unwrap();
        assert!(!f.game.join("mods").exists());
        fs::create_dir(f.game.join("mods")).unwrap();
        assert!(
            Store::create(
                &f.portable,
                &f.game,
                &f.game.join("mods"),
                &backend::unique_id(),
                || Ok(())
            )
            .unwrap()
            .is_none()
        );
    }
    #[test]
    fn ancestry_cycles_and_foreign_existing_links_are_refused_before_marker() {
        let f = Fixture::new();
        fs::create_dir_all(f.game.join("mods/inside")).unwrap();
        for p in [&f.game, &f.root, &f.game.join("mods/inside")] {
            assert!(
                Store::create(&f.portable, &f.game, p, &backend::unique_id(), || Ok(())).is_err()
            );
        }
        assert!(!f.portable.join("Data").join(MARKER).exists());
        fs::remove_dir(f.game.join("mods/inside")).unwrap();
        let link = open_dir(&f.game.join("mods"), true).unwrap();
        set_junction(&link, &f.selected).unwrap();
        drop(link);
        assert!(
            Store::create(
                &f.portable,
                &f.game,
                &f.selected,
                &backend::unique_id(),
                || Ok(())
            )
            .is_err()
        );
    }
    #[test]
    fn recovery_handles_intent_move_link_creation_and_complete_activation() {
        for stage in 0..4 {
            let f = Fixture::new();
            fs::create_dir(f.game.join("mods")).unwrap();
            fs::write(f.game.join("mods/keep.iff"), b"keep").unwrap();
            let mut s = f.store();
            if stage == 3 {
                s.activate(|| Ok(())).unwrap();
            } else if stage > 0 {
                let original = open_dir(&s.live(), false).unwrap();
                loader_files::publish(&original, &s.backup()).unwrap();
                drop(original);
                s.record.stage = "moved".into();
                s.commit().unwrap();
                if stage == 2 {
                    fs::create_dir(s.live()).unwrap();
                    s.record.link = Some(identity(&open_dir(&s.live(), false).unwrap()).unwrap());
                    s.record.stage = "link-created".into();
                    s.commit().unwrap();
                }
            }
            drop(s);
            let mut r = Store::recover(&f.portable, &f.game).unwrap().unwrap();
            r.restore(|| Ok(())).unwrap();
            assert_eq!(fs::read(f.game.join("mods/keep.iff")).unwrap(), b"keep");
            assert!(!r.path().exists());
        }
    }
    #[test]
    fn recovery_refuses_changed_live_directory_and_missing_original() {
        let f = Fixture::new();
        fs::create_dir(f.game.join("mods")).unwrap();
        let mut s = f.store();
        s.activate(|| Ok(())).unwrap();
        fs::remove_dir(s.live()).unwrap();
        fs::create_dir(s.live()).unwrap();
        fs::write(s.live().join("foreign.iff"), b"foreign").unwrap();
        assert!(s.restore(|| Ok(())).is_err());
        assert_eq!(fs::read(s.live().join("foreign.iff")).unwrap(), b"foreign");
        assert!(s.backup().is_dir());
    }
    #[test]
    fn game_presence_blocks_activation_and_restoration_without_losing_marker() {
        let f = Fixture::new();
        let mut s = f.store();
        assert!(
            s.activate(|| anyhow::bail!("Fixture live/unknown"))
                .is_err()
        );
        assert!(!s.live().exists());
        s.activate(|| Ok(())).unwrap();
        assert!(s.restore(|| anyhow::bail!("Fixture live/unknown")).is_err());
        assert!(s.path().exists());
        assert!(s.live().join("user-mod.iff").exists());
        s.restore(|| Ok(())).unwrap();
    }
    #[test]
    fn corrupt_marker_and_invalid_ownership_remain_unchanged() {
        let f = Fixture::new();
        let s = f.store();
        let mut r = s.record.clone();
        r.backup = "../foreign".into();
        assert!(r.validate(&s.game).is_err());
        drop(s);
        let path = f.portable.join("Data").join(MARKER);
        fs::write(&path, b"corrupt").unwrap();
        assert!(Store::recover(&f.portable, &f.game).is_err());
        assert_eq!(fs::read(path).unwrap(), b"corrupt");
    }
}
