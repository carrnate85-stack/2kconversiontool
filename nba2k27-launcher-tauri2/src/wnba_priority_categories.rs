//! DEV-only category admission proof. This module never calls native code or
//! writes game memory. Source registration alone does not authorize categories.
use crate::{
    blacktop_native::Memory, file_identity::Stable, loader_files::Directories,
    session_safety::Identity,
};
use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
use windows::Win32::{
    Foundation::HANDLE,
    Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FILE_FLAG_OPEN_REPARSE_POINT, GetFileInformationByHandle,
    },
};

const INDEX_SHA: &str = "6630BEDCBF708145FDFB55A48DE467265F3DE29CD76081E9C24A1C8EFD7B84BD";
const PAYLOAD_SHA: &str = "D42B4E4DDFA03A83DF799A49A9BAEF1526A74393F4CFC44A5CE01AA2BF7CE7EE";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PackProfile {
    Freestyle,
    Blacktop,
}
impl PackProfile {
    pub(crate) fn for_blacktop(blacktop: bool) -> Self {
        if blacktop {
            Self::Blacktop
        } else {
            Self::Freestyle
        }
    }
    fn folder(self) -> &'static str {
        match self {
            Self::Freestyle => "qa058-wpf-era",
            Self::Blacktop => "qa064-blacktop-source",
        }
    }
    fn index_len(self) -> usize {
        match self {
            Self::Freestyle => 120,
            Self::Blacktop => 144,
        }
    }
    fn entries(self) -> u32 {
        match self {
            Self::Freestyle => 1,
            Self::Blacktop => 2,
        }
    }
    fn index_sha(self) -> &'static str {
        match self {
            Self::Freestyle => INDEX_SHA,
            Self::Blacktop => "0C25D0839B11B4AF085F587C1BAF4E74F821CD34D2B902FC950DBC91E5123CC9",
        }
    }
    fn pack(self, root: &Path) -> PathBuf {
        root.join("Data/wnba-priority-v2").join(self.folder())
    }
}
pub(crate) struct PreparedPack {
    _files: Vec<Pinned>,
    _directories: Directories,
}
/// Validate and retain the already prepared local roster before Steam dispatch.
pub(crate) fn prepare_pack(root: &Path, profile: PackProfile) -> Result<PreparedPack> {
    let pack = profile.pack(root);
    let directories = Directories::pin(&pack).context(
        "The verified local WNBA roster must be prepared before enabling this dev feature",
    )?;
    let (index, index_bytes) = Pinned::open(
        &pack.join("0A"),
        Some(profile.index_len() as u64),
        profile.index_len(),
    )?;
    let (payload, payload_bytes) = Pinned::open(&pack.join("0B"), Some(1762144), 1762144)?;
    ensure!(
        crate::loader_files::digest(&index_bytes) == profile.index_sha()
            && crate::loader_files::digest(&payload_bytes) == PAYLOAD_SHA,
        "Prepared local WNBA roster differs from the tested pack"
    );
    Ok(PreparedPack {
        _files: vec![index, payload],
        _directories: directories,
    })
}
fn nonce(session: &str) -> Result<()> {
    ensure!(
        session.len() == 48 && session.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid WNBA category session"
    );
    Ok(())
}
fn token_path(root: &Path, session: &str) -> PathBuf {
    root.join("Data/wnba-priority-v2")
        .join(format!("categories-{session}.bin"))
}
fn token(bytes: &[u8], session: &str, identity: &Identity) -> Result<()> {
    nonce(session)?;
    let birth: u64 = identity
        .started
        .parse()
        .context("Invalid game birth for WNBA category token")?;
    ensure!(
        birth > 0
            && identity.pid > 0
            && bytes.len() == 72
            && &bytes[..8] == b"C27WNPC1"
            && bytes[8..12] == identity.pid.to_le_bytes()
            && bytes[12..16] == [0; 4]
            && bytes[16..24] == birth.to_le_bytes()
            && &bytes[24..] == session.as_bytes(),
        "WNBA categories require a separate exact-session category approval"
    );
    Ok(())
}
struct Pinned {
    file: File,
    id: Stable,
    length: u64,
    modified: u64,
}
impl Pinned {
    fn open(path: &Path, length: Option<u64>, limit: usize) -> Result<(Self, Vec<u8>)> {
        let file = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(path)?;
        let meta = file.metadata()?;
        ensure!(
            meta.len() <= limit as u64 && length.is_none_or(|n| n == meta.len()),
            "WNBA proof file length differs"
        );
        let mut value = Self {
            id: Stable::read(&file)?,
            length: meta.len(),
            modified: meta.last_write_time(),
            file,
        };
        value.check()?;
        let bytes = value.bytes(limit)?;
        Ok((value, bytes))
    }
    fn check(&self) -> Result<()> {
        let meta = self.file.metadata()?;
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        unsafe {
            GetFileInformationByHandle(HANDLE(self.file.as_raw_handle()), &mut info)?;
        }
        ensure!(
            meta.is_file()
                && meta.file_attributes() & 0x400 == 0
                && info.nNumberOfLinks == 1
                && meta.len() == self.length
                && meta.last_write_time() == self.modified
                && Stable::read(&self.file)? == self.id,
            "WNBA proof file changed or is linked/nonordinary"
        );
        Ok(())
    }
    fn bytes(&mut self, limit: usize) -> Result<Vec<u8>> {
        self.check()?;
        self.file.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        (&mut self.file)
            .take((limit + 1) as u64)
            .read_to_end(&mut bytes)?;
        self.check()?;
        ensure!(
            bytes.len() as u64 == self.length && bytes.len() <= limit,
            "WNBA proof read changed"
        );
        Ok(bytes)
    }
    fn same_path(&self, path: &Path) -> Result<()> {
        self.check()?;
        let current = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(path)?;
        ensure!(
            Stable::read(&current)? == self.id,
            "WNBA proof path no longer identifies the retained file"
        );
        Ok(())
    }
}
pub(crate) fn approval_requested(root: &Path, session: &str, identity: &Identity) -> Result<bool> {
    nonce(session)?;
    let path = token_path(root, session);
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.into()),
        Ok(_) => {}
    }
    let _dirs = Directories::pin(path.parent().unwrap())?;
    let (_, bytes) = Pinned::open(&path, Some(72), 72)?;
    token(&bytes, session, identity)?;
    Ok(true)
}
fn receipt(bytes: &[u8], session: &str, identity: &Identity) -> Result<()> {
    let text = std::str::from_utf8(bytes)?;
    let lines: Vec<_> = text.lines().collect();
    let expected = [
        "SCHEMA=wnba-native-source-admission-test-v1".to_owned(),
        "STATE=REGISTERED".to_owned(),
        format!("PID={}", identity.pid),
        format!("SESSION={session}"),
    ];
    ensure!(
        lines.len() == 8
            && lines[..4] == expected
            && lines[4].starts_with("REASON=")
            && lines[5..]
                == [
                    "CATEGORY_WRITES=0",
                    "GAME_CODE_HOOKS=0",
                    "WNBA_GAMEPLAY_VERIFIED=0"
                ],
        "Exact-session native WNBA source registration is not proven"
    );
    Ok(())
}
struct Proof {
    root: PathBuf,
    session: String,
    identity: Identity,
    _dirs: Vec<Directories>,
    index: Pinned,
    payload: Pinned,
    source: Pinned,
    index_bytes: Vec<u8>,
    profile: PackProfile,
    source_permission: Pinned,
}
fn source_profile(bytes: &[u8], session: &str, identity: &Identity) -> Result<PackProfile> {
    nonce(session)?;
    let birth: u64 = identity
        .started
        .parse()
        .context("Invalid WNBA source game birth")?;
    ensure!(
        bytes.len() == 72
            && identity.pid > 0
            && birth > 0
            && bytes[8..12] == identity.pid.to_le_bytes()
            && bytes[12..16] == [0; 4]
            && bytes[16..24] == birth.to_le_bytes()
            && &bytes[24..] == session.as_bytes(),
        "WNBA source permission does not match the owned game"
    );
    match &bytes[..8] {
        b"C27WNPA1" => Ok(PackProfile::Freestyle),
        b"C27WNBT1" => Ok(PackProfile::Blacktop),
        _ => anyhow::bail!("WNBA source permission type is invalid"),
    }
}
impl Proof {
    fn open(root: &Path, session: &str, identity: &Identity) -> Result<Self> {
        let permissions = root.join("Data/wnba-priority-v2");
        let permission_dirs = Directories::pin(&permissions)?;
        let (source_permission, permission_bytes) = Pinned::open(
            &permissions.join(format!("approval-{session}.bin")),
            Some(72),
            72,
        )?;
        let profile = source_profile(&permission_bytes, session, identity)?;
        let pack = profile.pack(root);
        let logs = root.join("Logs/game-session");
        let dirs = vec![
            permission_dirs,
            Directories::pin(&pack)?,
            Directories::pin(&logs)?,
        ];
        let (index, index_bytes) = Pinned::open(
            &pack.join("0A"),
            Some(profile.index_len() as u64),
            profile.index_len(),
        )?;
        let (payload, payload_bytes) = Pinned::open(&pack.join("0B"), Some(1762144), 1762144)?;
        ensure!(
            crate::loader_files::digest(&index_bytes) == profile.index_sha()
                && crate::loader_files::digest(&payload_bytes) == PAYLOAD_SHA,
            "WNBA private pack does not match the prepared source"
        );
        let (source, bytes) = Pinned::open(
            &logs.join(format!("wnba-source-test-{}.log", identity.pid)),
            None,
            4096,
        )?;
        receipt(&bytes, session, identity)?;
        Ok(Self {
            root: root.to_owned(),
            session: session.to_owned(),
            identity: identity.clone(),
            _dirs: dirs,
            index,
            payload,
            source,
            index_bytes,
            profile,
            source_permission,
        })
    }
    fn check(&mut self, m: &impl Memory) -> Result<()> {
        ensure!(
            approval_requested(&self.root, &self.session, &self.identity)?,
            "WNBA category approval missing"
        );
        self.check_source(m)
    }
    fn check_source(&mut self, m: &impl Memory) -> Result<()> {
        let pack = self.profile.pack(&self.root);
        self.source_permission.same_path(
            &self
                .root
                .join("Data/wnba-priority-v2")
                .join(format!("approval-{}.bin", self.session)),
        )?;
        ensure!(
            source_profile(
                &self.source_permission.bytes(72)?,
                &self.session,
                &self.identity
            )? == self.profile,
            "WNBA source permission profile changed"
        );
        self.index.check()?;
        self.payload.check()?;
        // Existing handles deny write/delete sharing; identities are re-opened
        // rather than treating an equal filename or content hash as ownership.
        self.index.same_path(&pack.join("0A"))?;
        self.payload.same_path(&pack.join("0B"))?;
        self.source.same_path(
            &self
                .root
                .join("Logs/game-session")
                .join(format!("wnba-source-test-{}.log", self.identity.pid)),
        )?;
        receipt(&self.source.bytes(4096)?, &self.session, &self.identity)?;
        let prefix = format!("{}/", pack.to_string_lossy().replace('/', "\\"));
        ensure!(prefix.is_ascii(), "WNBA native source prefix must be ASCII");
        m.checkpoint()?;
        let first = source_snapshot(m, prefix.as_bytes(), &self.index_bytes, self.profile)?;
        ensure!(
            first == source_snapshot(m, prefix.as_bytes(), &self.index_bytes, self.profile)?,
            "WNBA priority source changed during verification"
        );
        m.checkpoint()
    }
}
fn read(m: &impl Memory, at: u64, n: usize) -> Result<Vec<u8>> {
    ensure!(
        n > 0
            && n <= 4096
            && at >= 0x10000
            && at
                .checked_add(n as u64)
                .is_some_and(|end| end < 0x800000000000),
        "Invalid WNBA source pointer"
    );
    let bytes = m.read(at, n)?;
    ensure!(bytes.len() == n, "Partial WNBA source read");
    Ok(bytes)
}
fn q(m: &impl Memory, at: u64) -> Result<u64> {
    Ok(u64::from_le_bytes(read(m, at, 8)?.try_into().unwrap()))
}
fn d(m: &impl Memory, at: u64) -> Result<u32> {
    Ok(u32::from_le_bytes(read(m, at, 4)?.try_into().unwrap()))
}
fn source_snapshot(
    m: &impl Memory,
    prefix: &[u8],
    index: &[u8],
    profile: PackProfile,
) -> Result<Vec<u64>> {
    ensure!(
        index.len() == profile.index_len() && prefix.len() < 4096,
        "Invalid WNBA expected source metadata"
    );
    let base = m.base();
    ensure!(
        base >= 0x10000 && base < 0x800000000000 - 0xBB9A920,
        "Invalid WNBA image base"
    );
    let context = q(m, base + 0xBB9A8E8 + 0x38)?;
    ensure!(
        context >= 0x10000 && context < 0x800000000000 - 0x108,
        "Invalid WNBA resource context"
    );
    let manager = q(m, context + 0x100)?;
    ensure!(
        manager >= 0x10000 && manager < 0x800000000000 - 0x150,
        "Invalid WNBA resource manager"
    );
    ensure!(
        q(m, manager)? == base + 0x812C7C8,
        "WNBA resource manager type differs"
    );
    let priority = q(m, manager + 0x48)?;
    ensure!(
        priority >= 0x10000 && priority < 0x800000000000 - 0x38,
        "Invalid WNBA priority object"
    );
    ensure!(
        priority != 0 && q(m, priority)? == base + 0x812D868 && q(m, priority + 8)? == manager,
        "WNBA native priority ownership differs"
    );
    ensure!(
        d(m, priority + 0x30)? == profile.entries() && d(m, priority + 0x34)? == 1,
        "WNBA priority index/file counts differ"
    );
    let raw = q(m, priority + 0x10)?;
    let header = q(m, priority + 0x18)?;
    let files = q(m, priority + 0x20)?;
    let path = q(m, priority + 0x28)?;
    ensure!(
        read(m, raw, profile.entries() as usize * 24)? == index[96..],
        "WNBA retained priority index entry differs"
    );
    let mut expected = prefix.to_vec();
    expected.push(0);
    ensure!(
        read(m, path, expected.len())? == expected,
        "WNBA priority refers to a foreign directory"
    );
    ensure!(
        read(m, header, 48)? == index[..48],
        "WNBA retained priority header differs"
    );
    // Native initialization installs a handle at descriptor+8 and a tag at
    // +0x2c. Compare only the immutable payload length and filename fields.
    let descriptor = read(m, files, 48)?;
    ensure!(
        descriptor[..8] == index[48..56] && descriptor[0x10..0x2c] == index[64..92],
        "WNBA retained payload descriptor differs"
    );
    Ok(vec![context, manager, priority, raw, header, files, path])
}
pub(crate) fn verify(
    root: &Path,
    session: &str,
    identity: &Identity,
    m: &impl Memory,
) -> Result<()> {
    nonce(session)?;
    ensure!(
        approval_requested(root, session, identity)?,
        "WNBA category approval missing"
    );
    static PROOF: OnceLock<Mutex<Option<Proof>>> = OnceLock::new();
    let mut guard = PROOF
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| anyhow::anyhow!("WNBA source proof lock failed"))?;
    if guard
        .as_ref()
        .is_none_or(|p| p.root != root || p.session != session || p.identity != *identity)
    {
        *guard = None;
        *guard = Some(Proof::open(root, session, identity)?);
    }
    guard.as_mut().unwrap().check(m)
}

/// Test-only observation: returns no write capability and deliberately omits
/// category consent because every operation is a bounded file/memory read.
#[cfg(test)]
pub(crate) fn verify_source_readonly(
    root: &Path,
    session: &str,
    identity: &Identity,
    m: &impl Memory,
) -> Result<()> {
    nonce(session)?;
    Proof::open(root, session, identity)?.check_source(m)
}

#[cfg(test)]
#[path = "wnba_priority_categories_tests.rs"]
mod tests;
