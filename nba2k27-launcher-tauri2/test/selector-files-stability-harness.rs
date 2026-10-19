//! Production selector/loader/profile file operations in uniquely named fake
//! directories only. No process, Steam or installed-game code is present.
#![allow(dead_code)]
mod native_session {
    pub(crate) fn same_path(a: &str, b: &std::path::Path) -> bool {
        a.trim_start_matches(r"\\?\")
            .eq_ignore_ascii_case(b.to_string_lossy().trim_start_matches(r"\\?\"))
    }
}
mod provider_protocol {
    pub(crate) const PIPE_PREFIX: &str = r"\\.\pipe\NBA2K27ModProvider-";
}
mod selector_generation {
    pub(crate) type Warning = serde_json::Value;
}
mod backend {
    use anyhow::{Context, Result};
    use serde::Serialize;
    use std::{
        io::Write,
        os::windows::ffi::OsStrExt,
        path::Path,
        sync::atomic::{AtomicU64, Ordering},
    };
    use windows::{Win32::Storage::FileSystem::*, core::PCWSTR};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    pub fn unique_id() -> String {
        format!(
            "{:016x}{:016x}{:016x}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            NEXT.fetch_add(1, Ordering::Relaxed)
        )
    }
    fn wide(path: &Path) -> Result<Vec<u16>> {
        let parent = path.parent().context("Missing parent")?.canonicalize()?;
        Ok(parent
            .join(path.file_name().context("Missing filename")?)
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect())
    }
    fn save(path: &Path, value: &impl Serialize, replace: bool) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(value)?;
        let temp = path.with_file_name(format!(".write-{}.tmp", unique_id()));
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
        drop(f);
        let from = wide(&temp)?;
        let to = wide(path)?;
        unsafe {
            MoveFileExW(
                PCWSTR(from.as_ptr()),
                PCWSTR(to.as_ptr()),
                if replace {
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH
                } else {
                    MOVEFILE_WRITE_THROUGH
                },
            )?;
        }
        Ok(())
    }
    pub fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
        save(path, value, true)
    }
    pub fn atomic_json_new(path: &Path, value: &impl Serialize) -> Result<()> {
        save(path, value, false)
    }
}
#[path = "../src/file_identity.rs"]
mod file_identity;
#[path = "../src/loader_files.rs"]
mod loader_files;
#[path = "../src/mod_profile.rs"]
mod mod_profile;
#[path = "../src/player_folders.rs"]
mod player_folders;
#[path = "../src/selector_files.rs"]
mod selector_files;

#[cfg(test)]
mod repeated_session_audit {
    use super::*;
    use std::{fs, os::windows::fs::MetadataExt, path::PathBuf};
    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let p = std::env::temp_dir()
                .join(format!("selector-adjacent-repeat-{}", backend::unique_id()));
            fs::create_dir_all(p.join("portable/Data")).unwrap();
            fs::create_dir(p.join("mods")).unwrap();
            Self(p)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let live = self.0.join("game/mods");
            if fs::symlink_metadata(&live).is_ok_and(|m| m.file_attributes() & 0x400 != 0) {
                let _ = fs::remove_dir(live);
            }
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn profile_repeated_fresh_sessions_preserve_the_same_original_directory() {
        let s = Scratch::new();
        let portable = s.0.join("portable");
        let mods = s.0.join("mods");
        let game = s.0.join("game");
        fs::create_dir_all(game.join("mods")).unwrap();
        fs::write(game.join("mods/original.iff"), b"original").unwrap();
        fs::write(mods.join("selected.iff"), b"selected").unwrap();
        for round in 0..12 {
            let session = backend::unique_id();
            let mut store =
                mod_profile::Store::create(&portable, &game, &mods, &session, || Ok(()))
                    .unwrap()
                    .unwrap();
            store
                .activate(|| Ok(()))
                .unwrap_or_else(|e| panic!("Profile round {round}: {e:#}"));
            drop(store);
            let mut store = mod_profile::Store::recover(&portable, &game)
                .unwrap()
                .unwrap();
            store.restore(|| Ok(())).unwrap();
            drop(store);
            assert_eq!(
                fs::read(game.join("mods/original.iff")).unwrap(),
                b"original"
            );
            assert_eq!(fs::read(mods.join("selected.iff")).unwrap(), b"selected");
            assert!(!game.join("mods/selected.iff").exists());
        }
    }
    #[test]
    fn player_links_repeated_fresh_sources_with_same_target_remain_owned() {
        let s = Scratch::new();
        let portable = s.0.join("portable");
        let mods = s.0.join("mods");
        for round in 0..12 {
            if round > 0 {
                fs::remove_file(mods.join(format!("players/Package{}/png1452.iff", round - 1)))
                    .unwrap();
            }
            let source = mods.join(format!("players/Package{round}/png1452.iff"));
            fs::create_dir_all(source.parent().unwrap()).unwrap();
            fs::write(&source, b"player").unwrap();
            let session = backend::unique_id();
            let mut store = player_folders::Store::create(&portable, &mods, &session, || Ok(()))
                .unwrap()
                .unwrap();
            let before_stable =
                file_identity::Stable::read(&fs::File::open(&source).unwrap()).unwrap();
            let before_birth = fs::metadata(&source).unwrap().creation_time();
            store
                .activate(|| Ok(()))
                .unwrap_or_else(|e| panic!("Player link round {round}: {e:#}"));
            store.verify_routes().unwrap_or_else(|e|panic!("Player link round {round}: {e:#}; source stable before/after: {before_stable:?} / {:?}; CreationTime before/after: {before_birth} / {}",file_identity::Stable::read(&fs::File::open(&source).unwrap()).unwrap(),fs::metadata(&source).unwrap().creation_time()));
            drop(store);
            let mut store = player_folders::Store::recover(&portable, &mods, &session)
                .unwrap()
                .unwrap();
            store.restore(|| Ok(())).unwrap();
            drop(store);
            assert!(!mods.join("char/sig/png1452.iff").exists());
            assert_eq!(fs::read(source).unwrap(), b"player");
        }
    }
}
#[path = "../src/selector_tables.rs"]
mod selector_tables;
#[path = "../src/sigconfig.rs"]
mod sigconfig;
