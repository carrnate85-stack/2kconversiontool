//! Crash-recoverable, no-overwrite installation of the proven native loader.
//! Not a public launch command. Tests operate only in their own synthetic roots.
//! Restoration deletes only exact owned bytes through the verified file handle.
#![allow(dead_code)]
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    mem::size_of,
    os::windows::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Component, Path, PathBuf},
};
use windows::Win32::{Foundation::HANDLE, Storage::FileSystem::*};
pub const PROXY_SHA: &str = "ECEF8C6C856A60AF3387BE50D76BD30B53A80F80F404476F5C31D5A5453E91DF";
const PROXY: &[u8] = include_bytes!("../../nba2k27-version-proxy/dist-scoreboard-text/version.dll");
const SCHEMA: &str = "nba2k27-rust-loader-files-v1";
const JOURNAL: &str = "loader-session.json";
const NAMES: [&str; 3] = [
    "version.dll",
    "nba2k27_provider_audit.cfg",
    "nba2k27_version_proxy_audit.trigger",
];
const DELETE_ACCESS: u32 = 0x10000;
pub fn digest(b: &[u8]) -> String {
    format!("{:X}", Sha256::digest(b))
}
fn hex(s: &str, n: usize) -> bool {
    s.len() == n && s.bytes().all(|b| b.is_ascii_hexdigit())
}
fn plain(meta: &fs::Metadata) -> bool {
    meta.file_attributes() & 0x400 == 0
}
pub(crate) struct Directories(Vec<File>);
impl Directories {
    pub fn pin(path: &Path) -> Result<Self> {
        ensure!(path.is_absolute(), "Absolute loader storage path required");
        let mut pinned = Vec::new();
        let mut current = PathBuf::new();
        for part in path.components() {
            ensure!(
                !matches!(part, Component::ParentDir | Component::CurDir),
                "Unnormalized loader storage path"
            );
            current.push(part.as_os_str());
            if matches!(part, Component::Prefix(_)) {
                continue;
            }
            let file = fs::OpenOptions::new()
                .read(true)
                .share_mode(3)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
                .open(&current)
                .with_context(|| {
                    format!("Pinning loader storage directory {}", current.display())
                })?;
            let meta = file.metadata()?;
            ensure!(
                meta.is_dir() && plain(&meta),
                "Linked/non-directory loader storage is unsupported"
            );
            pinned.push(file);
        }
        ensure!(!pinned.is_empty(), "Empty loader storage path");
        Ok(Self(pinned))
    }
}
/// The proven DLL starts two workers which both create this directory. Its
/// helper treats a concurrent ERROR_ALREADY_EXISTS as failure. Match WPF:
/// create and pin the plain diagnostics tree BEFORE publishing/launching.
pub(crate) fn prepare_reports(portable: &Path) -> Result<Vec<Directories>> {
    let mut pins = vec![Directories::pin(portable)?];
    for name in ["Logs", "Logs/game-session"] {
        let path = portable.join(name);
        match fs::create_dir(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => {
                return Err(e)
                    .with_context(|| format!("Preparing native reports {}", path.display()));
            }
        }
        // Do not create a child through a foreign junction or symlink. Each
        // parent remains pinned before the next directory is considered.
        pins.push(Directories::pin(&path)?);
    }
    Ok(pins)
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Entry {
    name: String,
    staged: String,
    sha256: String,
    size: u64,
    published: bool,
    removed: bool,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    schema: String,
    session: String,
    game_root: PathBuf,
    entries: Vec<Entry>,
    closed: bool,
}
impl Record {
    fn validate(&self, game: &Path) -> Result<()> {
        ensure!(
            self.schema == SCHEMA
                && hex(&self.session, 48)
                && crate::native_session::same_path(&self.game_root.to_string_lossy(), game),
            "Loader recovery ownership differs"
        );
        ensure!(
            self.entries.len() == 3,
            "Unexpected loader recovery file count"
        );
        for (i, e) in self.entries.iter().enumerate() {
            ensure!(
                e.name == NAMES[i]
                    && e.staged == format!(".2k-launcher-{}-{i}.tmp", self.session)
                    && hex(&e.sha256, 64)
                    && (1..=64 * 1024 * 1024).contains(&e.size),
                "Invalid loader file lease"
            );
            if i == 0 {
                ensure!(
                    e.sha256 == PROXY_SHA && e.size == PROXY.len() as u64,
                    "Unknown native loader lease"
                );
            }
            if i == 2 {
                ensure!(
                    e.sha256 == digest(b"trigger\r\n") && e.size == 9,
                    "Unknown native trigger lease"
                );
            }
            ensure!(
                !e.removed || self.closed || !e.published,
                "Invalid loader recovery phase"
            );
        }
        Ok(())
    }
}
pub(crate) struct Store {
    portable: PathBuf,
    game: PathBuf,
    _pins: Vec<Directories>,
    record: Record,
}
fn missing(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(e.into()),
    }
}
fn open_owned(path: &Path) -> Result<File> {
    let file = fs::OpenOptions::new()
        .read(true)
        .access_mode(0x80000000 | DELETE_ACCESS)
        .share_mode(1)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)?;
    let meta = file.metadata()?;
    ensure!(
        meta.is_file() && plain(&meta) && meta.len() <= 64 * 1024 * 1024,
        "Unrecognized loader-owned file"
    );
    Ok(file)
}
fn bytes(file: &mut File, limit: u64) -> Result<Vec<u8>> {
    let before = file.metadata()?;
    ensure!(before.len() <= limit, "Loader-owned file exceeds bounds");
    let mut b = Vec::new();
    file.take(limit + 1).read_to_end(&mut b)?;
    let after = file.metadata()?;
    ensure!(
        b.len() as u64 == before.len()
            && after.len() == before.len()
            && after.last_write_time() == before.last_write_time(),
        "Loader-owned file changed while checking"
    );
    Ok(b)
}
pub(crate) fn unlink(file: &File) -> Result<()> {
    let info = FILE_DISPOSITION_INFO {
        DeleteFile: true.into(),
    };
    unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileDispositionInfo,
            &info as *const _ as _,
            size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
        .context("Removing an exact owned file handle")?;
    }
    Ok(())
}
fn rename_buffer(name: &[u16]) -> Result<(Vec<u64>, usize)> {
    ensure!(
        !name.is_empty() && name.len() <= 32760 && !name.contains(&0),
        "Loader destination path exceeds bounds"
    );
    // FileNameLength excludes the terminator, but the Win32 wrapper still
    // requires a NUL-terminated FileName. Allocator alignment padding is not a
    // terminator: 66 UTF-16 chars (the installed game's version.dll path) left
    // zero spare bytes and the wrapper read past the provided name buffer.
    let length = size_of::<FILE_RENAME_INFO>() + (name.len() + 1) * 2;
    let mut aligned = vec![0u64; length.div_ceil(8)];
    let info = aligned.as_mut_ptr() as *mut FILE_RENAME_INFO;
    unsafe {
        (*info).Anonymous.ReplaceIfExists = false;
        (*info).FileNameLength = (name.len() * 2) as u32;
        std::ptr::copy_nonoverlapping(name.as_ptr(), (*info).FileName.as_mut_ptr(), name.len());
    }
    Ok((aligned, length))
}
pub(crate) fn publish(file: &File, target: &Path) -> Result<()> {
    let target = std::path::absolute(target)?;
    let name = target.as_os_str().encode_wide().collect::<Vec<_>>();
    let (mut aligned, length) = rename_buffer(&name)?;
    unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileRenameInfo,
            aligned.as_mut_ptr() as _,
            length as u32,
        )
        .with_context(|| format!("Publishing an owned handle to {}", target.display()))?;
    }
    Ok(())
}
impl Store {
    pub fn session(&self) -> &str {
        &self.record.session
    }
    pub fn create(
        portable: &Path,
        game: &Path,
        session: &str,
        config: &[u8],
        check: impl Fn() -> Result<()>,
    ) -> Result<Self> {
        ensure!(hex(session, 48), "Invalid loader session ID");
        ensure!(
            digest(PROXY) == PROXY_SHA,
            "Bundled native loader hash differs"
        );
        ensure!(
            !config.is_empty() && config.len() <= 2047 && config.is_ascii(),
            "Invalid native loader configuration"
        );
        let pins = vec![
            Directories::pin(portable)?,
            Directories::pin(&portable.join("Data"))?,
            Directories::pin(game)?,
        ];
        check()?;
        ensure!(
            missing(&portable.join("Data").join(JOURNAL))?,
            "Loader recovery is pending"
        );
        for n in NAMES {
            ensure!(
                missing(&game.join(n))?,
                "Existing game-side loader file was left untouched: {n}"
            );
        }
        let contents = [PROXY, config, &b"trigger\r\n"[..]];
        let entries = contents
            .into_iter()
            .enumerate()
            .map(|(i, b)| Entry {
                name: NAMES[i].into(),
                staged: format!(".2k-launcher-{session}-{i}.tmp"),
                sha256: digest(b),
                size: b.len() as u64,
                published: false,
                removed: false,
            })
            .collect();
        let store = Self {
            portable: portable.into(),
            game: game.into(),
            _pins: pins,
            record: Record {
                schema: SCHEMA.into(),
                session: session.into(),
                game_root: game.into(),
                entries,
                closed: false,
            },
        };
        store.record.validate(game)?;
        check()?;
        crate::backend::atomic_json_new(&store.path(), &store.record)?;
        Ok(store)
    }
    fn path(&self) -> PathBuf {
        self.portable.join("Data").join(JOURNAL)
    }
    fn commit(&self) -> Result<()> {
        self.record.validate(&self.game)?;
        crate::backend::atomic_json(&self.path(), &self.record)
    }
    pub fn recover(portable: &Path, game: &Path) -> Result<Option<Self>> {
        let pins = vec![
            Directories::pin(portable)?,
            Directories::pin(&portable.join("Data"))?,
            Directories::pin(game)?,
        ];
        let path = portable.join("Data").join(JOURNAL);
        if missing(&path)? {
            return Ok(None);
        }
        let mut file = open_owned(&path)?;
        let record: Record = serde_json::from_slice(&bytes(&mut file, 65536)?)?;
        record.validate(game)?;
        drop(file);
        Ok(Some(Self {
            portable: portable.into(),
            game: game.into(),
            _pins: pins,
            record,
        }))
    }
    fn install(
        &mut self,
        index: usize,
        content: &[u8],
        check: impl Fn() -> Result<()>,
    ) -> Result<()> {
        ensure!(!self.record.closed, "Loader session is already closing");
        let entry = self
            .record
            .entries
            .get(index)
            .context("Unexpected loader file index")?;
        ensure!(
            !entry.published
                && !entry.removed
                && entry.sha256 == digest(content)
                && entry.size == content.len() as u64,
            "Loader file does not match committed intent"
        );
        check()?;
        let staged = self.game.join(&entry.staged);
        let target = self.game.join(&entry.name);
        ensure!(
            missing(&target)?,
            "Game-side loader file appeared; left untouched"
        );
        let mut file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .access_mode(0xC0000000 | DELETE_ACCESS)
            .share_mode(1)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
            .create_new(true)
            .open(&staged)
            .with_context(|| format!("Creating owned staging file {}", staged.display()))?;
        // Only this still-open, just-created staging handle can be removed on
        // a write failure; recovery never assumes a partial file is ours.
        let written = (|| -> Result<()> {
            file.write_all(content)?;
            file.sync_all()?;
            check()?;
            publish(&file, &target)
        })(); // no-replace, exact handle
        if let Err(error) = written {
            let _ = unlink(&file);
            return Err(error);
        }
        drop(file);
        self.record.entries[index].published = true;
        self.commit()?;
        Ok(())
    }
    pub fn install_loader(&mut self, config: &[u8], check: impl Fn() -> Result<()>) -> Result<()> {
        self.install(0, PROXY, &check)?;
        self.install(1, config, check)
    }
    pub fn trigger(&mut self, check: impl Fn() -> Result<()>) -> Result<()> {
        // Trigger publication is allowed with a live game only by the owned
        // lifecycle's separate identity/build/readiness check.
        ensure!(
            self.record.entries[..2].iter().all(|e| e.published),
            "Loader was not prepared"
        );
        self.install(2, b"trigger\r\n", check)
    }
    pub fn restore(&mut self, closed: impl Fn() -> Result<()>) -> Result<()> {
        closed()?;
        self.record.closed = true;
        self.commit()?;
        for index in (0..self.record.entries.len()).rev() {
            let entry = self.record.entries[index].clone();
            for name in [&entry.name, &entry.staged] {
                closed()?;
                let path = self.game.join(name);
                if missing(&path)? {
                    continue;
                }
                if !entry.published && name == &entry.name {
                    let meta = fs::symlink_metadata(&path)?;
                    if !meta.is_file()
                        || meta.file_attributes() & 0x400 != 0
                        || meta.len() != entry.size
                    {
                        continue;
                    }
                }
                // A failed publication may have renamed the stage before its
                // published bit became durable. Exact bytes still own that
                // target; an unpublished mismatching target is foreign.
                let mut file = open_owned(&path)?;
                let content = bytes(&mut file, 64 * 1024 * 1024)?;
                let matches =
                    content.len() as u64 == entry.size && digest(&content) == entry.sha256;
                if !entry.published && name == &entry.name && !matches {
                    continue;
                }
                ensure!(
                    matches,
                    "Loader recovery found changed/foreign bytes; left untouched: {name}"
                );
                closed()?;
                unlink(&file)?;
                drop(file);
            }
            self.record.entries[index].removed = true;
            self.commit()?;
        }
        closed()?;
        let mut file = open_owned(&self.path())?;
        let content = bytes(&mut file, 65536)?;
        ensure!(
            serde_json::from_slice::<Record>(&content)? == self.record,
            "Loader recovery marker changed; left untouched"
        );
        closed()?;
        unlink(&file)?;
        Ok(())
    }
}

pub fn configuration(
    endpoint: &str,
    logical: &str,
    size: u64,
    sha: &str,
    portable: &Path,
    overrides: &[String],
    selector: bool,
) -> Result<Vec<u8>> {
    use crate::provider_protocol::PIPE_PREFIX;
    ensure!(
        endpoint
            .strip_prefix(PIPE_PREFIX)
            .is_some_and(|n| hex(n, 64)),
        "Native loader endpoint differs"
    );
    fn valid_path(p: &str) -> bool {
        !p.is_empty()
            && p.len() <= 260
            && p.is_ascii()
            && p.bytes()
                .all(|b| (0x20..=0x7e).contains(&b) && !b"=:*?\"<>|\\".contains(&b))
            && p.split('/').all(|v| !v.is_empty() && v != "." && v != "..")
    }
    ensure!(
        valid_path(logical) && (1..=64 * 1024 * 1024).contains(&size) && hex(sha, 64),
        "Invalid native preflight asset"
    );
    ensure!(
        overrides.len() <= 4 && overrides.iter().all(|p| valid_path(p)),
        "Invalid stock override paths"
    );
    let mut unique = std::collections::HashSet::new();
    ensure!(
        overrides
            .iter()
            .all(|p| unique.insert(p.to_ascii_lowercase())),
        "Duplicate stock override"
    );
    let canonical = portable.canonicalize()?;
    let text = canonical.to_string_lossy();
    let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
    ensure!(
        text.len() >= 3
            && text.as_bytes()[1] == b':'
            && text.as_bytes()[2] == b'\\'
            && !text.chars().any(|c| c < ' '),
        "Portable native diagnostics require a local drive path"
    );
    let logs = format!("{text}\\Logs\\game-session");
    ensure!(
        logs.encode_utf16().count() <= 400,
        "Portable diagnostics path is too long"
    );
    ensure!(
        portable.join("App").is_dir() && portable.join("Data/settings.json").is_file(),
        "Portable diagnostics layout is incomplete"
    );
    let encoded = logs
        .encode_utf16()
        .map(|c| format!("{c:04X}"))
        .collect::<String>();
    let mut lines = vec![
        format!("PIPE={endpoint}"),
        format!("PATH={logical}"),
        format!("SIZE={size}"),
        format!("SHA256={}", sha.to_ascii_uppercase()),
        format!("PORTABLE_LOG_DIR_HEX={encoded}"),
    ];
    for (i, p) in overrides.iter().enumerate() {
        lines.push(format!(
            "{}={p}",
            if i == 0 {
                "OVERRIDE".into()
            } else {
                format!("OVERRIDE_{}", i + 1)
            }
        ));
    }
    if selector {
        lines.push("SELECTOR_OVERRIDE=1".into());
    }
    lines.push("SCOREBOARD_TEXT_TEST=1".into());
    lines.push(String::new());
    let bytes = lines.join("\r\n").into_bytes();
    ensure!(
        bytes.len() <= 2047,
        "Native session configuration exceeds 2047 bytes"
    );
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::offset_of;
    struct Fixture {
        root: PathBuf,
        game: PathBuf,
        portable: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir()
                .join(format!("rust-loader-files-{}", crate::backend::unique_id()));
            let game = root.join("Game");
            let portable = root.join("Portable");
            for p in [&game, &portable.join("Data"), &portable.join("App")] {
                fs::create_dir_all(p).unwrap();
            }
            fs::write(portable.join("Data/settings.json"), b"{}").unwrap();
            Self {
                root,
                game,
                portable,
            }
        }
        fn store(&self) -> Store {
            Store::create(
                &self.portable,
                &self.game,
                &crate::backend::unique_id(),
                b"fixture\r\n",
                || Ok(()),
            )
            .unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    #[test]
    fn proven_proxy_hash_and_native_configuration_are_exact_and_bounded() {
        assert_eq!(digest(PROXY), PROXY_SHA);
        assert_eq!(PROXY.len(), 956416);
        let f = Fixture::new();
        let endpoint = format!(
            "{}{}",
            crate::provider_protocol::PIPE_PREFIX,
            "01".repeat(32)
        );
        let sha = digest(b"fixture");
        let config = configuration(
            &endpoint,
            "char/sig/test.iff",
            7,
            &sha,
            &f.portable,
            &["loc/english.iff".into()],
            true,
        )
        .unwrap();
        let text = String::from_utf8(config).unwrap();
        assert!(text.contains("\r\nSELECTOR_OVERRIDE=1\r\nSCOREBOARD_TEXT_TEST=1\r\n"));
        assert!(text.contains("OVERRIDE=loc/english.iff\r\n"));
        assert!(text.contains("PORTABLE_LOG_DIR_HEX="));
        assert!(!text.contains("sessionToken"));
        for path in [
            "../escape.iff",
            "/root.iff",
            "char\nSCOREBOARD_TEXT_TEST=0",
            "char/foo=bar",
            "C:/test.iff",
            "ä.iff",
        ] {
            assert!(configuration(&endpoint, path, 7, &sha, &f.portable, &[], false).is_err());
        }
        assert!(configuration(&endpoint, "test", 0, &sha, &f.portable, &[], false).is_err());
        assert!(
            configuration(
                &endpoint,
                "test",
                7,
                &sha,
                &f.portable,
                &["a".into(), "A".into()],
                false
            )
            .is_err()
        );
    }
    #[test]
    fn fresh_report_directories_are_ready_before_both_loader_workers_and_are_pinned() {
        let f = Fixture::new();
        let logs = f.portable.join("Logs/game-session");
        assert!(!logs.exists());
        let prepared = prepare_reports(&f.portable).unwrap();
        assert!(logs.is_dir());
        assert_eq!(fs::read_dir(&logs).unwrap().count(), 0);
        let other = prepare_reports(&f.portable).unwrap();
        assert!(fs::rename(&logs, f.portable.join("moved-logs")).is_err());
        drop(other);
        drop(prepared);
        assert!(!f.game.join("version.dll").exists());
        assert!(!f.portable.join("Data/loader-session.json").exists());
    }
    #[test]
    fn report_preparation_is_concurrent_idempotent_and_does_not_replace_foreign_files() {
        let f = Fixture::new();
        std::thread::scope(|s| {
            let a = s.spawn(|| prepare_reports(&f.portable));
            let b = s.spawn(|| prepare_reports(&f.portable));
            let a = a.join().unwrap().unwrap();
            let b = b.join().unwrap().unwrap();
            assert!(f.portable.join("Logs/game-session").is_dir());
            drop((a, b));
        });
        let foreign = Fixture::new();
        fs::create_dir(foreign.portable.join("Logs")).unwrap();
        let path = foreign.portable.join("Logs/game-session");
        fs::write(&path, b"foreign file").unwrap();
        assert!(prepare_reports(&foreign.portable).is_err());
        assert_eq!(fs::read(path).unwrap(), b"foreign file");
    }
    #[test]
    fn linked_report_parent_is_rejected_before_creating_any_child_through_it() {
        let f = Fixture::new();
        let outside = f.root.join("foreign-report-target");
        fs::create_dir(&outside).unwrap();
        let link = f.portable.join("Logs");
        let output = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&link)
            .arg(&outside)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(prepare_reports(&f.portable).is_err());
        assert!(!outside.join("game-session").exists());
        fs::remove_dir(&link).unwrap();
    }
    #[test]
    fn staged_exact_handle_publication_restores_only_owned_files() {
        let f = Fixture::new();
        fs::write(f.game.join("user-mod.txt"), b"preserve").unwrap();
        let mut s = f.store();
        s.install_loader(b"fixture\r\n", || Ok(())).unwrap();
        s.trigger(|| Ok(())).unwrap();
        assert_eq!(fs::read(f.game.join("version.dll")).unwrap(), PROXY);
        s.restore(|| Ok(())).unwrap();
        for n in NAMES {
            assert!(!f.game.join(n).exists());
        }
        assert!(!s.path().exists());
        assert_eq!(fs::read(f.game.join("user-mod.txt")).unwrap(), b"preserve");
    }
    #[test]
    fn rename_buffer_has_explicit_terminator_for_every_alignment_including_game_path() {
        let game: Vec<u16> = r"C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27\version.dll"
            .encode_utf16()
            .collect();
        assert_eq!(game.len() % 4, 2);
        for name in std::iter::once(game).chain((64..68).map(|len| vec![b'a' as u16; len])) {
            let (buffer, length) = rename_buffer(&name).unwrap();
            let required = offset_of!(FILE_RENAME_INFO, FileName) + (name.len() + 1) * 2;
            assert!(
                length >= required,
                "Missing NUL for {} UTF-16 characters: {length} bytes, needs {required}",
                name.len()
            );
            assert!(buffer.len() * 8 >= required);
            let info = buffer.as_ptr() as *const FILE_RENAME_INFO;
            unsafe {
                assert_eq!((*info).FileNameLength as usize, name.len() * 2);
                assert_eq!(*(*info).FileName.as_ptr().add(name.len()), 0);
            }
        }
    }
    #[test]
    fn publication_accepts_space_parentheses_and_mixed_verbatim_paths() {
        for verbatim in [false, true] {
            let f = Fixture::new();
            let game = f
                .root
                .join("Program Files (x86)/Steam/steamapps/common/NBA 2K27");
            fs::create_dir_all(&game).unwrap();
            let game = if verbatim {
                game.canonicalize().unwrap()
            } else {
                game
            };
            let mut s = Store::create(
                &f.portable,
                &game,
                &crate::backend::unique_id(),
                b"fixture\r\n",
                || Ok(()),
            )
            .unwrap();
            s.install_loader(b"fixture\r\n", || Ok(())).unwrap();
            s.trigger(|| Ok(())).unwrap();
            assert_eq!(fs::read(game.join("version.dll")).unwrap(), PROXY);
            s.restore(|| Ok(())).unwrap();
            assert!(!game.join("version.dll").exists());
        }
    }
    #[test]
    fn actual_handle_publication_supports_every_path_alignment_and_unicode() {
        let f = Fixture::new();
        for prefix in ["ASCII folder (test)", "Unicode 出版 (test)"] {
            for suffix in 0..4 {
                let game = f
                    .root
                    .join(prefix)
                    .join(format!("Game{}", "x".repeat(suffix)));
                fs::create_dir_all(&game).unwrap();
                let path = game.join("version.dll");
                let mut s = Store::create(
                    &f.portable,
                    &game,
                    &crate::backend::unique_id(),
                    b"fixture\r\n",
                    || Ok(()),
                )
                .unwrap();
                s.install_loader(b"fixture\r\n", || Ok(())).unwrap();
                s.trigger(|| Ok(())).unwrap();
                assert_eq!(
                    fs::read(&path).unwrap(),
                    PROXY,
                    "{} UTF-16 chars",
                    path.as_os_str().encode_wide().count()
                );
                s.restore(|| Ok(())).unwrap();
                assert!(!path.exists());
            }
        }
    }
    #[test]
    fn existing_proxy_or_config_is_never_overwritten_or_adopted() {
        for name in NAMES {
            let f = Fixture::new();
            fs::write(f.game.join(name), b"user-owned").unwrap();
            assert!(
                Store::create(
                    &f.portable,
                    &f.game,
                    &crate::backend::unique_id(),
                    b"cfg",
                    || Ok(())
                )
                .is_err()
            );
            assert_eq!(fs::read(f.game.join(name)).unwrap(), b"user-owned");
            assert!(!f.portable.join("Data").join(JOURNAL).exists());
        }
    }
    #[test]
    fn committed_intent_recovers_before_or_after_publication() {
        for installed in [false, true] {
            let f = Fixture::new();
            let mut s = f.store();
            if installed {
                s.install_loader(b"fixture\r\n", || Ok(())).unwrap();
            }
            drop(s);
            let mut recovered = Store::recover(&f.portable, &f.game).unwrap().unwrap();
            recovered.restore(|| Ok(())).unwrap();
            assert!(!f.game.join("version.dll").exists());
            assert!(Store::recover(&f.portable, &f.game).unwrap().is_none());
        }
    }
    #[test]
    fn game_appearing_before_publication_leaves_no_installed_file() {
        use std::cell::Cell;
        let f = Fixture::new();
        let mut s = f.store();
        let calls = Cell::new(0);
        assert!(
            s.install(0, PROXY, || {
                calls.set(calls.get() + 1);
                ensure!(calls.get() < 2, "Fixture game opened");
                Ok(())
            })
            .is_err()
        );
        assert!(!f.game.join("version.dll").exists());
        assert!(!f.game.join(&s.record.entries[0].staged).exists());
        s.restore(|| Ok(())).unwrap();
    }
    #[test]
    fn live_or_unknown_game_blocks_restore_and_journal_survives() {
        let f = Fixture::new();
        let mut s = f.store();
        s.install_loader(b"fixture\r\n", || Ok(())).unwrap();
        assert!(
            s.restore(|| anyhow::bail!("Fixture game might be live"))
                .is_err()
        );
        assert!(s.path().exists());
        assert_eq!(fs::read(f.game.join("version.dll")).unwrap(), PROXY);
        s.restore(|| Ok(())).unwrap();
    }
    #[test]
    fn changed_file_and_corrupt_journal_are_left_for_review() {
        let f = Fixture::new();
        let mut s = f.store();
        s.install_loader(b"fixture\r\n", || Ok(())).unwrap();
        fs::write(f.game.join(NAMES[0]), b"foreign").unwrap();
        assert!(s.restore(|| Ok(())).is_err());
        assert_eq!(fs::read(f.game.join(NAMES[0])).unwrap(), b"foreign");
        assert!(s.path().exists());
        drop(s);
        fs::write(f.portable.join("Data").join(JOURNAL), b"broken").unwrap();
        assert!(Store::recover(&f.portable, &f.game).is_err());
        assert_eq!(
            fs::read(f.portable.join("Data").join(JOURNAL)).unwrap(),
            b"broken"
        );
    }
    #[test]
    fn a_target_appearing_after_intent_is_not_replaced() {
        let f = Fixture::new();
        let mut s = f.store();
        fs::write(f.game.join(NAMES[0]), b"foreign after intent").unwrap();
        assert!(s.install_loader(b"fixture\r\n", || Ok(())).is_err());
        assert_eq!(
            fs::read(f.game.join(NAMES[0])).unwrap(),
            b"foreign after intent"
        );
        s.restore(|| Ok(())).unwrap();
        assert_eq!(
            fs::read(f.game.join(NAMES[0])).unwrap(),
            b"foreign after intent"
        );
        assert!(!s.path().exists());
    }
    #[test]
    fn partially_written_or_foreign_staging_file_is_not_deleted_by_recovery() {
        let f = Fixture::new();
        let mut s = f.store();
        let staged = f.game.join(&s.record.entries[0].staged);
        fs::write(&staged, b"partial or foreign").unwrap();
        assert!(s.restore(|| Ok(())).is_err());
        assert_eq!(fs::read(&staged).unwrap(), b"partial or foreign");
        assert!(s.path().exists());
    }
    #[test]
    fn journal_cannot_retarget_arbitrary_files_and_directory_pins_block_rename() {
        let f = Fixture::new();
        let s = f.store();
        let mut record = s.record.clone();
        record.entries[0].name = "user-mod.txt".into();
        assert!(record.validate(&f.game).is_err());
        record = s.record.clone();
        record.game_root = f.root.clone();
        assert!(record.validate(&f.game).is_err());
        assert!(fs::rename(&f.game, f.root.join("moved")).is_err());
        drop(s);
    }

    #[test]
    fn unpublished_flag_does_not_adopt_foreign_stage_or_strand_exact_renamed_payload() {
        let f = Fixture::new();
        let mut s = f.store();
        fs::write(f.game.join(NAMES[0]), PROXY).unwrap();
        s.restore(|| Ok(())).unwrap();
        assert!(!f.game.join(NAMES[0]).exists());
        let mut s = f.store();
        fs::write(f.game.join(NAMES[0]), b"foreign").unwrap();
        let stage = f.game.join(&s.record.entries[0].staged);
        fs::write(&stage, b"foreign stage").unwrap();
        assert!(s.restore(|| Ok(())).is_err());
        assert_eq!(fs::read(&stage).unwrap(), b"foreign stage");
        assert!(s.path().exists());
        assert_eq!(fs::read(f.game.join(NAMES[0])).unwrap(), b"foreign");
    }
}
