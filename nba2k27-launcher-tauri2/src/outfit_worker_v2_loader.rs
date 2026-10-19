//! Separate explicit V2-outfit loader payload admission. The original loader
//! hash, schema, journal and public memory-write gate are never changed.
//! Crash-recoverable, no-overwrite installation of the proven native loader.
//! Not a public launch command. Tests operate only in their own synthetic roots.
//! Restoration deletes only exact owned bytes through the verified file handle.
#![allow(dead_code)]
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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
pub const PROXY_SHA: &str = "2E921A4E8B408996C20DEAD7412835A9197D67B83C05CE840130300A903120DD";
const PROXY: &[u8] = include_bytes!("../assets/native/version-outfits-v2-qa084.dll");
// Recovery-only admission for the previous exact owned payload. Updating the
// embedded DLL must not make an interrupted 0.53.1 session unrecoverable.
// New publication below still accepts ONLY PROXY/PROXY_SHA.
fn known_recovery_payload(sha: &str, size: u64) -> bool {
    (sha == PROXY_SHA && size == PROXY.len() as u64)
        || (sha == "83AF5D0613D1E90D51933F382C22D215F766A61A198CAFEB41E80606A1722518"
            && size == 1_215_488)
        || (sha == "1E3A0E67030D0D2421C75797659AC7E6F390115921228F14A18CC84D0B5E0EB1"
            && size == 1_194_496)
        || (sha == "D9A465F814AD3405E0B65B43F418F97B705ED8AC30D8CA5FEBC64A4A562A2B07"
            && size == 1_193_472)
        || (sha == "4FB74B375BE6DE36411574FAB624722F85121AEF1F0DE286E9C9EEC6B879A3FA"
            && size == 1_190_912)
        || (sha == "DE06738A948896D8EAC2C1F4147B65D34068314B32D26F061305EBA4A972EB0C"
            && size == 1_179_136)
        || (sha == "92B28E137E5D3336F1859EB16CE6D6C492366822E07DD19082D6F29ED22E718E"
            && size == 1_144_832)
        || (sha == "E1BB63BF598B9FDE1B28F83E82F240E2FD5CB603505DF8A08F2921DB959BB204"
            && size == 1_142_784)
        || (sha == "EE2D90AF316717231C455346F62E0F3190CF2B48B3682CAA7189F7408E82C0B9"
            && size == 1_142_784)
        || (sha == "81952F0CF8D13226AF8F664A226E617B21F2EC278E6F7BF94844246E8D80CB04"
            && size == 1_137_152)
        || (sha == "72984DCC84C2B768F2B00802708A546409652255FD5AC81754448BA23D0BBE53"
            && size == 1_137_152)
        || (sha == "B0574314A93911A90918F8D55429D03BC32B71A6C7589B6D670C874BB42E90B9"
            && size == 1_122_304)
        || (sha == "376492792C2812947BB4200368C4737BDE5C59A9BF7A9375169E4FF6EF6EBEFB"
            && size == 1_095_680)
        || (sha == "4164D1D8DB3161CA30688F41AD3ECDDDE152DB71C13D6458146F67741CDD9B71"
            && size == 1_037_312)
        || (sha == "D39E5745655C8FE6EDACBA1C56BC12B232A2F101DC18A292D300EC8A2B27136A"
            && size == 1_035_264)
        || (sha == "4B03D0EAD15EB5CE562C434CD304017ADAD09DEFA96C1B4AAF30AA656830886F"
            && size == 1_029_632)
        || (sha == "6437DA71FCB153FB864A066DBC65359CD78AF32721CB47A59456FC28B9FC450C"
            && size == 1_005_568)
        || (sha == "99B7249F0F8004C433E17DA04FCCA15FEF2B56962EFD529DFE56F87407691101"
            && size == 1_004_544)
        || (sha == "5ED5246D8D1081D0CA136D1B59631D56DB549BB9A67000A6D55597FA7E4D155C"
            && size == 1_004_544)
        || (sha == "645984B76691111224089F73A11B0BD8A9F8C7BECC22BB8E826A6EC4BB2CFF7C"
            && size == 1_003_520)
}
const SCHEMA: &str = "nba2k27-rust-outfit-v2-loader-files-v1";
const JOURNAL: &str = "loader-outfit-v2-session.json";
const NAMES: [&str; 3] = [
    "version.dll",
    "nba2k27_provider_audit.cfg",
    "nba2k27_version_proxy_audit.trigger",
];
const DELETE_ACCESS: u32 = 0x10000;
const RECEIPT_LIMIT: u64 = 65536;
type Id = crate::file_identity::Stable;
fn identity(file: &File) -> Result<Id> {
    Id::read(file)
}
fn directory_id(path: &Path) -> Result<Id> {
    let pins = Directories::pin(path)?;
    identity(pins.0.last().context("Empty loader directory path")?)
}
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
    file_identity: Option<Id>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    schema: String,
    session: String,
    game_root: PathBuf,
    entries: Vec<Entry>,
    closed: bool,
    receipt_identity: Id,
    portable_identity: Id,
    data_identity: Id,
    game_identity: Id,
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
                    known_recovery_payload(&e.sha256, e.size),
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
            ensure!(
                !e.published || e.file_identity.is_some(),
                "Published loader file lacks identity"
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
    receipt_hash: String,
}
fn missing(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(e.into()),
    }
}
fn open_owned(path: &Path) -> Result<File> {
    open_receipt(path, false)
}
fn open_receipt(path: &Path, write: bool) -> Result<File> {
    let file = fs::OpenOptions::new()
        .read(true)
        .access_mode(0x80000000 | DELETE_ACCESS | if write { 0x40000000 } else { 0 })
        .share_mode(1)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)?;
    let meta = file.metadata()?;
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    unsafe {
        GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info)?;
    }
    ensure!(
        meta.is_file()
            && plain(&meta)
            && meta.len() <= 64 * 1024 * 1024
            && info.nNumberOfLinks == 1,
        "Unrecognized loader-owned file"
    );
    Ok(file)
}
fn bytes(file: &mut File, limit: u64) -> Result<Vec<u8>> {
    let before = file.metadata()?;
    ensure!(before.len() <= limit, "Loader-owned file exceeds bounds");
    let before_id = identity(file)?;
    file.seek(SeekFrom::Start(0))?;
    let mut b = Vec::new();
    file.take(limit + 1).read_to_end(&mut b)?;
    let after = file.metadata()?;
    ensure!(
        b.len() as u64 == before.len()
            && after.len() == before.len()
            && after.last_write_time() == before.last_write_time()
            && identity(file)? == before_id,
        "Loader-owned file changed while checking"
    );
    Ok(b)
}
fn read_receipt(file: &mut File, game: &Path) -> Result<(Record, String)> {
    let data = bytes(file, RECEIPT_LIMIT)?;
    let mut records = serde_json::Deserializer::from_slice(&data).into_iter::<Record>();
    let mut current = records
        .next()
        .context("Loader receipt has no durable intent")??;
    current.validate(game)?;
    ensure!(
        identity(file)? == current.receipt_identity,
        "Loader receipt identity changed; left untouched"
    );
    let mut count = 1;
    for next in records {
        count += 1;
        ensure!(count <= 16, "Loader receipt has too many updates");
        let next = next?;
        transition(&current, &next)?;
        next.validate(game)?;
        current = next;
    }
    Ok((current, digest(&data)))
}
fn transition(previous: &Record, next: &Record) -> Result<()> {
    let mut normalized = next.clone();
    normalized.closed = previous.closed;
    ensure!(!previous.closed || next.closed, "Loader receipt reopened");
    for (a, b) in previous.entries.iter().zip(&mut normalized.entries) {
        ensure!(
            !a.published || b.published,
            "Loader publication went backwards"
        );
        ensure!(!a.removed || b.removed, "Loader removal went backwards");
        ensure!(
            a.file_identity.is_none() || a.file_identity == b.file_identity,
            "Loader file identity changed"
        );
        b.published = a.published;
        b.removed = a.removed;
        b.file_identity = a.file_identity.clone();
    }
    ensure!(
        normalized == *previous,
        "Loader receipt immutable ownership changed"
    );
    Ok(())
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
            missing(&portable.join("Data/loader-session.json"))?,
            "Original native loader recovery is pending"
        );
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
        // The outer launch journal stores the canonical target. Bind this
        // receipt to that same target representation after pinning every
        // ordinary input ancestor. Do not loosen same_path or persisted IDs
        // merely to accept a forward-slash spelling from saved preferences.
        let canonical_game = game.canonicalize()?;
        let game = canonical_game.as_path();
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
                file_identity: None,
            })
            .collect();
        let marker = portable.join("Data").join(JOURNAL);
        // Create and identify the receipt through the same no-share-write/delete
        // handle. Its identity is part of the durable intent, so even a byte-for-
        // byte replacement after a crash cannot be adopted by recovery.
        check()?;
        let mut receipt = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .access_mode(0xC0000000 | DELETE_ACCESS)
            .share_mode(1)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
            .create_new(true)
            .open(&marker)?;
        let mut store = Self {
            portable: portable.into(),
            game: game.into(),
            _pins: pins,
            record: Record {
                schema: SCHEMA.into(),
                session: session.into(),
                game_root: game.into(),
                entries,
                closed: false,
                receipt_identity: identity(&receipt)?,
                portable_identity: directory_id(portable)?,
                data_identity: directory_id(&portable.join("Data"))?,
                game_identity: directory_id(game)?,
            },
            receipt_hash: String::new(),
        };
        store.record.validate(game)?;
        check()?;
        let data = serde_json::to_vec(&store.record)?;
        receipt.write_all(&data)?;
        receipt.sync_all()?;
        store.receipt_hash = digest(&data);
        store.verify_receipt(&mut receipt)?;
        Ok(store)
    }
    fn path(&self) -> PathBuf {
        self.portable.join("Data").join(JOURNAL)
    }
    fn directories(&self) -> Result<()> {
        ensure!(
            directory_id(&self.portable)? == self.record.portable_identity
                && directory_id(&self.portable.join("Data"))? == self.record.data_identity
                && directory_id(&self.game)? == self.record.game_identity,
            "Loader parent directory identity changed; left untouched"
        );
        Ok(())
    }
    fn verify_receipt(&self, file: &mut File) -> Result<()> {
        self.directories()?;
        let (record, hash) = read_receipt(file, &self.game)?;
        ensure!(
            record == self.record && hash == self.receipt_hash,
            "Loader receipt changed; left untouched"
        );
        Ok(())
    }
    fn receipt(&self, write: bool) -> Result<File> {
        let mut file = open_receipt(&self.path(), write)?;
        self.verify_receipt(&mut file)?;
        Ok(file)
    }
    fn commit(&mut self, file: &mut File, next: Record) -> Result<()> {
        self.verify_receipt(file)?;
        transition(&self.record, &next)?;
        next.validate(&self.game)?;
        let mut data = vec![b'\n'];
        data.extend(serde_json::to_vec(&next)?);
        data.push(b'\n');
        ensure!(
            file.metadata()?.len() + data.len() as u64 <= RECEIPT_LIMIT,
            "Loader receipt exceeds its update bound"
        );
        // Append-only CAS: retain the exact verified file while appending and
        // publishing/removing physical files. A torn append is held, never
        // replayed or repaired by overwriting somebody else's receipt.
        file.seek(SeekFrom::End(0))?;
        file.write_all(&data)?;
        file.sync_all()?;
        let (actual, hash) = read_receipt(file, &self.game)?;
        ensure!(actual == next, "Loader receipt update readback differs");
        self.record = next;
        self.receipt_hash = hash;
        Ok(())
    }
    pub fn recover(portable: &Path, game: &Path) -> Result<Option<Self>> {
        let pins = vec![
            Directories::pin(portable)?,
            Directories::pin(&portable.join("Data"))?,
            Directories::pin(game)?,
        ];
        let canonical_game = game.canonicalize()?;
        let game = canonical_game.as_path();
        let path = portable.join("Data").join(JOURNAL);
        if missing(&path)? {
            return Ok(None);
        }
        let mut file = open_owned(&path)?;
        let (record, receipt_hash) = read_receipt(&mut file, game)?;
        drop(file);
        let store = Self {
            portable: portable.into(),
            game: game.into(),
            _pins: pins,
            record,
            receipt_hash,
        };
        store.directories()?;
        Ok(Some(store))
    }
    fn install(
        &mut self,
        index: usize,
        content: &[u8],
        check: impl Fn() -> Result<()>,
    ) -> Result<()> {
        ensure!(!self.record.closed, "Loader session is already closing");
        let mut receipt = self.receipt(true)?;
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
            entry.file_identity.is_none(),
            "Loader publication already began; recover before retrying"
        );
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
            let mut next = self.record.clone();
            next.entries[index].file_identity = Some(identity(&file)?);
            self.commit(&mut receipt, next)?;
            check()?;
            self.verify_receipt(&mut receipt)?;
            publish(&file, &target)
        })(); // no-replace, exact handle
        if let Err(error) = written {
            let _ = unlink(&file);
            return Err(error);
        }
        drop(file);
        let mut next = self.record.clone();
        next.entries[index].published = true;
        self.commit(&mut receipt, next)?;
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
        let mut receipt = self.receipt(true)?;
        if !self.record.closed {
            let mut next = self.record.clone();
            next.closed = true;
            self.commit(&mut receipt, next)?;
        }
        for index in (0..self.record.entries.len()).rev() {
            let entry = self.record.entries[index].clone();
            for name in [&entry.name, &entry.staged] {
                closed()?;
                let path = self.game.join(name);
                if missing(&path)? {
                    continue;
                }
                let mut file = open_owned(&path)?;
                let content = bytes(&mut file, 64 * 1024 * 1024)?;
                ensure!(
                    entry.file_identity.as_ref() == Some(&identity(&file)?)
                        && content.len() as u64 == entry.size
                        && digest(&content) == entry.sha256,
                    "Loader recovery found changed/foreign bytes; left untouched: {name}"
                );
                closed()?;
                self.verify_receipt(&mut receipt)?;
                unlink(&file)?;
                drop(file);
            }
            if !self.record.entries[index].removed {
                let mut next = self.record.clone();
                next.entries[index].removed = true;
                self.commit(&mut receipt, next)?;
            }
        }
        closed()?;
        self.verify_receipt(&mut receipt)?;
        closed()?;
        unlink(&receipt)?;
        Ok(())
    }
}

pub(crate) fn pending(portable: &Path) -> Result<bool> {
    Ok(!missing(&portable.join("Data").join(JOURNAL))?)
}

#[cfg(test)]
#[path = "outfit_worker_v2_loader_tests.rs"]
mod tests;
#[cfg(test)]
mod payload_compatibility_tests {
    #[test]
    fn previous_exact_payload_is_recovery_only_and_unknown_bytes_remain_foreign() {
        assert!(super::known_recovery_payload(
            super::PROXY_SHA,
            super::PROXY.len() as u64
        ));
        let qa077 = "1E3A0E67030D0D2421C75797659AC7E6F390115921228F14A18CC84D0B5E0EB1";
        assert!(super::known_recovery_payload(qa077, 1_194_496));
        assert!(!super::known_recovery_payload(qa077, 1_194_495));
        assert!(!super::known_recovery_payload(qa077, 1_194_497));
        let qa076 = "D9A465F814AD3405E0B65B43F418F97B705ED8AC30D8CA5FEBC64A4A562A2B07";
        assert!(super::known_recovery_payload(qa076, 1_193_472));
        assert!(!super::known_recovery_payload(qa076, 1_193_471));
        assert!(!super::known_recovery_payload(qa076, 1_193_473));
        let qa073 = "4FB74B375BE6DE36411574FAB624722F85121AEF1F0DE286E9C9EEC6B879A3FA";
        assert!(super::known_recovery_payload(qa073, 1_190_912));
        assert!(!super::known_recovery_payload(qa073, 1_190_911));
        assert!(!super::known_recovery_payload(qa073, 1_190_913));
        let qa072 = "DE06738A948896D8EAC2C1F4147B65D34068314B32D26F061305EBA4A972EB0C";
        assert!(super::known_recovery_payload(qa072, 1_179_136));
        assert!(!super::known_recovery_payload(qa072, 1_179_135));
        assert!(!super::known_recovery_payload(qa072, 1_179_137));
        let qa071 = "92B28E137E5D3336F1859EB16CE6D6C492366822E07DD19082D6F29ED22E718E";
        assert!(super::known_recovery_payload(qa071, 1_144_832));
        assert!(!super::known_recovery_payload(qa071, 1_144_831));
        assert!(!super::known_recovery_payload(qa071, 1_144_833));
        let qa070 = "E1BB63BF598B9FDE1B28F83E82F240E2FD5CB603505DF8A08F2921DB959BB204";
        assert!(super::known_recovery_payload(qa070, 1_142_784));
        assert!(!super::known_recovery_payload(qa070, 1_142_783));
        let qa069 = "EE2D90AF316717231C455346F62E0F3190CF2B48B3682CAA7189F7408E82C0B9";
        assert!(super::known_recovery_payload(qa069, 1_142_784));
        assert!(!super::known_recovery_payload(qa069, 1_142_783));
        let qa068 = "81952F0CF8D13226AF8F664A226E617B21F2EC278E6F7BF94844246E8D80CB04";
        assert!(super::known_recovery_payload(qa068, 1_137_152));
        assert!(!super::known_recovery_payload(qa068, 1_137_151));
        let qa067 = "72984DCC84C2B768F2B00802708A546409652255FD5AC81754448BA23D0BBE53";
        assert!(super::known_recovery_payload(qa067, 1_137_152));
        assert!(!super::known_recovery_payload(qa067, 1_137_151));
        let qa066 = "B0574314A93911A90918F8D55429D03BC32B71A6C7589B6D670C874BB42E90B9";
        assert!(super::known_recovery_payload(qa066, 1_122_304));
        assert!(!super::known_recovery_payload(qa066, 1_122_303));
        let qa065 = "376492792C2812947BB4200368C4737BDE5C59A9BF7A9375169E4FF6EF6EBEFB";
        assert!(super::known_recovery_payload(qa065, 1_095_680));
        assert!(!super::known_recovery_payload(qa065, 1_095_679));
        let qa064 = "4164D1D8DB3161CA30688F41AD3ECDDDE152DB71C13D6458146F67741CDD9B71";
        assert!(super::known_recovery_payload(qa064, 1_037_312));
        assert!(!super::known_recovery_payload(qa064, 1_037_311));
        let qa061 = "D39E5745655C8FE6EDACBA1C56BC12B232A2F101DC18A292D300EC8A2B27136A";
        assert!(super::known_recovery_payload(qa061, 1_035_264));
        assert!(!super::known_recovery_payload(qa061, 1_035_263));
        let qa058 = "4B03D0EAD15EB5CE562C434CD304017ADAD09DEFA96C1B4AAF30AA656830886F";
        assert!(super::known_recovery_payload(qa058, 1_029_632));
        assert!(!super::known_recovery_payload(qa058, 1_029_631));
        let timing_only = "6437DA71FCB153FB864A066DBC65359CD78AF32721CB47A59456FC28B9FC450C";
        assert!(super::known_recovery_payload(timing_only, 1_005_568));
        assert!(!super::known_recovery_payload(timing_only, 1_005_567));
        let qa055 = "99B7249F0F8004C433E17DA04FCCA15FEF2B56962EFD529DFE56F87407691101";
        assert!(super::known_recovery_payload(qa055, 1_004_544));
        assert!(!super::known_recovery_payload(qa055, 1_004_543));
        let qa054 = "5ED5246D8D1081D0CA136D1B59631D56DB549BB9A67000A6D55597FA7E4D155C";
        assert!(super::known_recovery_payload(qa054, 1_004_544));
        assert!(!super::known_recovery_payload(qa054, 1_004_543));
        let old = "645984B76691111224089F73A11B0BD8A9F8C7BECC22BB8E826A6EC4BB2CFF7C";
        assert!(super::known_recovery_payload(old, 1_003_520));
        assert!(!super::known_recovery_payload(old, 1_003_521));
        assert!(!super::known_recovery_payload(&"A".repeat(64), 1_003_520));
        assert_eq!(super::digest(super::PROXY), super::PROXY_SHA);
    }
}
