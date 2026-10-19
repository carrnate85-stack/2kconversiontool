//! Session-scoped native outfit admission. The provider creates private events
//! before Steam; only an exact game bind publishes the fixed portable lease.
//! Recovery removes owned files, never replays native memory addresses.
#![allow(dead_code)]
use crate::{file_identity::Stable, loader_files, provider_access::Acl, session_safety::Identity};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use windows::{
    Win32::{
        Foundation::*,
        Security::{Authorization::*, *},
        Storage::FileSystem::*,
        System::Threading::*,
    },
    core::PCWSTR,
};
const SCHEMA: &str = "blacktop-outfit-v2-native-lease-v1";
const PREFIX: &str = "blacktop-outfit-v2-";
struct Event(HANDLE);
unsafe impl Send for Event {}
unsafe impl Sync for Event {}
impl Drop for Event {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
impl Event {
    fn reader(&self) -> Result<Self> {
        let mut handle = HANDLE::default();
        unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                self.0,
                GetCurrentProcess(),
                &mut handle,
                0x00100000,
                false,
                DUPLICATE_HANDLE_OPTIONS(0),
            )?;
        }
        Ok(Self(handle))
    }
    fn create(session: &str, suffix: &str) -> Result<Self> {
        let acl = Acl::current_user()?;
        let name = event_name(session, suffix)?
            .encode_utf16()
            .chain([0])
            .collect::<Vec<_>>();
        // Explicit owner is important under elevation: the default token owner
        // can be Administrators even though the DACL names only TokenUser.
        let text = format!("O:{}D:P(A;;GA;;;{})", acl.sid, acl.sid)
            .encode_utf16()
            .chain([0])
            .collect::<Vec<_>>();
        struct Descriptor(PSECURITY_DESCRIPTOR);
        impl Drop for Descriptor {
            fn drop(&mut self) {
                unsafe {
                    let _ = LocalFree(Some(HLOCAL(self.0.0)));
                }
            }
        }
        let mut descriptor = Descriptor(PSECURITY_DESCRIPTOR::default());
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(text.as_ptr()),
                SDDL_REVISION_1,
                &mut descriptor.0,
                None,
            )?;
        }
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0.0,
            bInheritHandle: false.into(),
        };
        let handle =
            unsafe { CreateEventW(Some(&attributes), true, false, PCWSTR(name.as_ptr()))? };
        let result = Self(handle);
        ensure!(
            unsafe { GetLastError() } != ERROR_ALREADY_EXISTS,
            "Native outfit event already exists; never adopt it"
        );
        Ok(result)
    }
    fn set(&self) -> Result<()> {
        unsafe {
            SetEvent(self.0)?;
        }
        Ok(())
    }
    fn reset(&self) -> Result<()> {
        unsafe {
            ResetEvent(self.0)?;
        }
        Ok(())
    }
}
fn nonce(s: &str) -> bool {
    s.len() == 48 && s.bytes().all(|b| b.is_ascii_hexdigit())
}
fn not_cancelled(cancelled: &AtomicBool) -> Result<()> {
    ensure!(
        !cancelled.load(Ordering::Acquire),
        "Native outfit lease is terminally cancelled"
    );
    Ok(())
}
fn revoke(
    cancelled: &AtomicBool,
    reset_permit: impl FnOnce() -> Result<()>,
    signal_cancel: impl FnOnce() -> Result<()>,
) -> Result<()> {
    // Local capabilities become terminal before either fallible OS operation.
    // Always try BOTH channels: a signalled Cancel denies native authority even
    // when resetting Permit fails, and a reset Permit denies it if Set fails.
    cancelled.store(true, Ordering::Release);
    let reset = reset_permit();
    let signal = signal_cancel();
    match (reset, signal) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => {
            Err(error.context("Native outfit Permit reset failed; Cancel signalled"))
        }
        (Ok(()), Err(error)) => {
            Err(error.context("Native outfit Cancel signal failed; Permit reset"))
        }
        (Err(reset), Err(signal)) => anyhow::bail!(
            "Native outfit revocation uncertain; Permit reset failed: {reset:#}; Cancel signal failed: {signal:#}"
        ),
    }
}
pub(crate) fn event_name(session: &str, suffix: &str) -> Result<String> {
    ensure!(
        nonce(session) && matches!(suffix, "Cancel" | "Permit"),
        "Invalid private outfit event name"
    );
    Ok(format!("Local\\NBA2K27OutfitsV2-{session}-{suffix}"))
}
pub(crate) fn configuration(base: &[u8], session: &str) -> Result<Vec<u8>> {
    ensure!(
        nonce(session) && base.is_ascii() && !base.contains(&0),
        "Invalid V2 outfit configuration"
    );
    let text = std::str::from_utf8(base)?;
    // FREESTYLE_PLAYLIST/FREESTYLE_SELECTION are allowed: the V2 proxy has a
    // Version 2 FreestyleBuildProfile. The 600 s event test stays unsupported.
    ensure!(
        !text
            .lines()
            .any(|l| l.starts_with("OUTFIT_V2_SESSION=") || l.starts_with("FREESTYLE_EVENT_TEST=")),
        "Duplicate V2 outfit or unsupported legacy Freestyle VM configuration"
    );
    let text = format!(
        "{}\r\nOUTFIT_V2_SESSION={session}\r\n",
        text.trim_end_matches(['\r', '\n'])
    );
    ensure!(
        text.len() <= 2047,
        "Native V2 outfit configuration exceeds 2047 bytes"
    );
    Ok(text.into_bytes())
}
fn identity(i: &Identity) -> Result<()> {
    ensure!(
        i.pid > 0
            && i.started.parse::<u64>().is_ok_and(|n| n > 0)
            && i.path.encode_utf16().count() < 256
            && i.path.len() > 3
            && i.path.as_bytes()[1] == b':'
            && i.path.as_bytes()[2] == b'\\'
            && !i.path.chars().any(|c| c < ' '),
        "Invalid exact native lease process identity"
    );
    Ok(())
}
fn paths(root: &Path, session: &str) -> Result<(PathBuf, PathBuf)> {
    ensure!(nonce(session), "Invalid native outfit lease session");
    Ok((
        root.join("Data").join(format!("{PREFIX}{session}.lease")),
        root.join("Data").join(format!("{PREFIX}{session}.journal")),
    ))
}
fn ordinary(file: &File, max: u64) -> Result<Stable> {
    let m = file.metadata()?;
    ensure!(
        m.is_file() && m.file_attributes() & 0x400 == 0 && m.len() <= max,
        "Native outfit lease is linked/nonordinary/oversized"
    );
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    unsafe {
        GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info)?;
    }
    ensure!(
        info.nNumberOfLinks == 1,
        "Hard-linked native outfit lease rejected"
    );
    Stable::read(file)
}
fn read(file: &File, max: usize) -> Result<Vec<u8>> {
    use std::os::windows::fs::FileExt;
    let count = file.metadata()?.len() as usize;
    ensure!(count <= max, "Native lease size limit");
    let mut bytes = vec![0; count];
    let mut offset = 0;
    while offset < count {
        let n = file.seek_read(&mut bytes[offset..], offset as u64)?;
        ensure!(n > 0, "Partial native lease read");
        offset += n;
    }
    Ok(bytes)
}
fn delete(file: &File) -> Result<()> {
    let disposition = FILE_DISPOSITION_INFO {
        DeleteFile: true.into(),
    };
    unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileDispositionInfo,
            &disposition as *const _ as _,
            std::mem::size_of_val(&disposition) as u32,
        )?;
    }
    Ok(())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "kebab-case", deny_unknown_fields)]
enum Record {
    Header {
        schema: String,
        session: String,
        data: Stable,
        worker: Identity,
        parent: Identity,
        target: String,
    },
    Creating {
        game: Identity,
        sha256: String,
    },
    Owned {
        file: Stable,
    },
    Ended {},
}
fn encode(session: &str, game: &Identity, worker: &Identity, parent: &Identity) -> Result<Vec<u8>> {
    ensure!(nonce(session), "Invalid native outfit lease nonce");
    for i in [game, worker, parent] {
        identity(i)?;
    }
    ensure!(
        game.pid != worker.pid && game.pid != parent.pid && worker.pid != parent.pid,
        "Native lease process aliases"
    );
    let mut bytes = Vec::with_capacity(1632);
    bytes.extend_from_slice(b"2KOV2L01");
    for n in [game.pid, worker.pid, parent.pid, 0] {
        bytes.extend_from_slice(&n.to_le_bytes());
    }
    for i in [game, worker, parent] {
        bytes.extend_from_slice(&i.started.parse::<u64>()?.to_le_bytes());
    }
    bytes.extend_from_slice(session.as_bytes());
    for i in [game, worker, parent] {
        let mut words = [0u16; 256];
        let name = i.path.encode_utf16().collect::<Vec<_>>();
        words[..name.len()].copy_from_slice(&name);
        for n in words {
            bytes.extend_from_slice(&n.to_le_bytes());
        }
    }
    ensure!(bytes.len() == 1632, "Native lease schema length");
    Ok(bytes)
}
pub(crate) struct Lease {
    _pins: Vec<loader_files::Directories>,
    root: PathBuf,
    session: String,
    data: Stable,
    worker: Identity,
    parent: Identity,
    target: String,
    cancel: Event,
    permit: Event,
    cancelled: Arc<AtomicBool>,
    journal: Option<File>,
    journal_id: Stable,
    events: Vec<Record>,
    published: Option<(File, Stable, Vec<u8>)>,
    ended: bool,
}
/// Non-renderer capability for the Rust queue worker. Only a published,
/// permitted, durable launch lease can mint it. It grants no native addresses.
pub(crate) struct Admission {
    root: PathBuf,
    session: String,
    game: Identity,
    parent: Identity,
    worker: Identity,
    cancel: Event,
    permit: Event,
    cancelled: Arc<AtomicBool>,
    file: File,
    id: Stable,
    bytes: Vec<u8>,
}
impl Admission {
    pub fn check(
        &self,
        root: &Path,
        session: &str,
        game: &Identity,
        parent: &Identity,
    ) -> Result<()> {
        not_cancelled(&self.cancelled)?;
        ensure!(
            crate::native_session::same_path(&self.root.to_string_lossy(), root)
                && self.session == session
                && self.game == *game
                && self.parent == *parent
                && self.worker.pid == std::process::id(),
            "Native outfit admission belongs to another launch"
        );
        ensure!(
            unsafe { WaitForSingleObject(self.cancel.0, 0) } == WAIT_TIMEOUT
                && unsafe { WaitForSingleObject(self.permit.0, 0) } == WAIT_OBJECT_0,
            "Native outfit permission ended/cancelled"
        );
        ensure!(
            ordinary(&self.file, 1632)? == self.id && read(&self.file, 1632)? == self.bytes,
            "Native outfit admission file changed"
        );
        not_cancelled(&self.cancelled)
    }
}
impl Lease {
    pub fn prepare(
        root: &Path,
        session: &str,
        worker: &Identity,
        parent: &Identity,
        target: &Path,
        check: impl Fn() -> Result<()>,
    ) -> Result<Self> {
        check()?;
        identity(worker)?;
        identity(parent)?;
        ensure!(
            worker.pid == std::process::id() && worker.pid != parent.pid,
            "Native outfit lease is not the current provider worker"
        );
        let root = root.canonicalize()?;
        let text = root.to_string_lossy();
        let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
        let root = PathBuf::from(text);
        let pins = vec![
            loader_files::Directories::pin(&root)?,
            loader_files::Directories::pin(&root.join("App"))?,
            loader_files::Directories::pin(&root.join("Data"))?,
            loader_files::Directories::pin(&root.join("Library"))?,
        ];
        let target = target
            .to_str()
            .context("Native outfit game path must be Unicode")?
            .to_owned();
        let worker_under = PathBuf::from(&worker.path).parent().is_some_and(|p| {
            crate::native_session::same_path(&p.to_string_lossy(), &root.join("App"))
        });
        let parent_under = PathBuf::from(&parent.path).parent().is_some_and(|p| {
            crate::native_session::same_path(&p.to_string_lossy(), &root.join("App"))
        });
        ensure!(
            worker_under && parent_under,
            "Outfit owner executables must belong to this portable App"
        );
        let (_, journal_path) = paths(&root, session)?;
        let data_file = OpenOptions::new()
            .read(true)
            .share_mode(3)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(root.join("Data"))?;
        let data = Stable::read(&data_file)?;
        let cancel = Event::create(session, "Cancel")?;
        let permit = Event::create(session, "Permit")?;
        let journal = OpenOptions::new()
            .read(true)
            .write(true)
            .access_mode(0xc0010000)
            .share_mode(1)
            .create_new(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0 | FILE_FLAG_WRITE_THROUGH.0)
            .open(journal_path)?;
        let journal_id = ordinary(&journal, 65536)?;
        let mut result = Self {
            _pins: pins,
            root,
            session: session.into(),
            data: data.clone(),
            worker: worker.clone(),
            parent: parent.clone(),
            target: target.clone(),
            cancel,
            permit,
            cancelled: Arc::new(AtomicBool::new(false)),
            journal: Some(journal),
            journal_id,
            events: Vec::new(),
            published: None,
            ended: false,
        };
        result.append(Record::Header {
            schema: SCHEMA.into(),
            session: session.into(),
            data,
            worker: worker.clone(),
            parent: parent.clone(),
            target,
        })?;
        check()?;
        Ok(result)
    }
    fn append(&mut self, event: Record) -> Result<()> {
        ensure!(
            !self.ended && self.events.len() < 4,
            "Native lease journal is terminal"
        );
        let f = self
            .journal
            .as_mut()
            .context("Native lease journal is closed")?;
        ensure!(
            ordinary(f, 65536)? == self.journal_id,
            "Native lease journal identity changed"
        );
        let expected = self
            .events
            .iter()
            .map(serde_json::to_vec)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .flat_map(|mut b| {
                b.push(b'\n');
                b
            })
            .collect::<Vec<_>>();
        ensure!(
            read(f, 65536)? == expected,
            "Native lease journal changed before append"
        );
        let mut bytes = serde_json::to_vec(&event)?;
        bytes.push(b'\n');
        ensure!(
            expected.len() + bytes.len() <= 65536,
            "Native lease journal limit"
        );
        f.seek(SeekFrom::End(0))?;
        f.write_all(&bytes)?;
        f.sync_all()?;
        let mut after = expected;
        after.extend(bytes);
        ensure!(
            ordinary(f, 65536)? == self.journal_id && read(f, 65536)? == after,
            "Native lease append readback differs"
        );
        self.events.push(event);
        Ok(())
    }
    pub fn bind(&mut self, game: &Identity, check: impl Fn() -> Result<()>) -> Result<()> {
        not_cancelled(&self.cancelled)?;
        check()?;
        ensure!(
            self.published.is_none() && self.events.len() == 1,
            "Native outfit game binding is single-use"
        );
        ensure!(
            crate::native_session::same_path(&game.path, Path::new(&self.target)),
            "Native outfit target differs from exact retained game"
        );
        let bytes = encode(&self.session, game, &self.worker, &self.parent)?;
        self.append(Record::Creating {
            game: game.clone(),
            sha256: loader_files::digest(&bytes),
        })?;
        let (path, _) = paths(&self.root, &self.session)?;
        let mut f = OpenOptions::new()
            .read(true)
            .write(true)
            .access_mode(0xc0010000)
            .share_mode(1)
            .create_new(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0 | FILE_FLAG_WRITE_THROUGH.0)
            .open(&path)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
        let id = ordinary(&f, 1632)?;
        ensure!(
            read(&f, 1632)? == bytes,
            "Native outfit lease readback differs"
        );
        self.append(Record::Owned { file: id.clone() })?;
        check()?;
        not_cancelled(&self.cancelled)?;
        drop(f); // The native worker's read-only/share-read open becomes legal.
        let file = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(path)?;
        ensure!(
            ordinary(&file, 1632)? == id && read(&file, 1632)? == bytes,
            "Native lease publication identity changed"
        );
        self.published = Some((file, id, bytes));
        check()?;
        not_cancelled(&self.cancelled)
    }
    pub fn allow(&self, check: impl FnOnce() -> Result<()>) -> Result<()> {
        let result = (|| {
            not_cancelled(&self.cancelled)?;
            check()?;
            ensure!(
                unsafe { WaitForSingleObject(self.cancel.0, 0) } == WAIT_TIMEOUT,
                "Native outfit lease is cancelled"
            );
            let (f, id, bytes) = self
                .published
                .as_ref()
                .context("Game has not bound the native outfit lease")?;
            ensure!(
                ordinary(f, 1632)? == *id && read(f, 1632)? == *bytes,
                "Native outfit lease changed"
            );
            not_cancelled(&self.cancelled)?;
            self.permit.set()?;
            not_cancelled(&self.cancelled)
        })();
        if result.is_err() {
            let _ = self.cancel();
        }
        result
    }
    pub fn cancel(&self) -> Result<()> {
        revoke(
            &self.cancelled,
            || self.permit.reset(),
            || self.cancel.set(),
        )
    }
    pub fn event_state(&self) -> Result<(bool, bool)> {
        let c = unsafe { WaitForSingleObject(self.cancel.0, 0) };
        let p = unsafe { WaitForSingleObject(self.permit.0, 0) };
        ensure!(
            [WAIT_OBJECT_0, WAIT_TIMEOUT].contains(&c)
                && [WAIT_OBJECT_0, WAIT_TIMEOUT].contains(&p),
            "Native event wait state is unknown"
        );
        Ok((c == WAIT_OBJECT_0, p == WAIT_OBJECT_0))
    }
    pub fn admission(&self) -> Result<Admission> {
        not_cancelled(&self.cancelled)?;
        ensure!(
            self.event_state()? == (false, true),
            "Native outfit admission is not permitted"
        );
        let (file, id, bytes) = self
            .published
            .as_ref()
            .context("Native outfit lease has not bound a game")?;
        let game = self
            .events
            .iter()
            .find_map(|e| {
                if let Record::Creating { game, .. } = e {
                    Some(game.clone())
                } else {
                    None
                }
            })
            .context("Native outfit game intent missing")?;
        Ok(Admission {
            root: self.root.clone(),
            session: self.session.clone(),
            game,
            parent: self.parent.clone(),
            worker: self.worker.clone(),
            cancel: self.cancel.reader()?,
            permit: self.permit.reader()?,
            cancelled: self.cancelled.clone(),
            file: file.try_clone()?,
            id: id.clone(),
            bytes: bytes.clone(),
        })
    }
    pub fn finish_ended(&mut self, closed: impl Fn() -> Result<()>) -> Result<()> {
        closed()?;
        if self.ended {
            return Ok(());
        }
        self.cancel()?;
        let saved = self.published.take();
        drop(saved);
        let owned = self.events.iter().find_map(|e| {
            if let Record::Owned { file } = e {
                Some(file.clone())
            } else {
                None
            }
        });
        if owned.is_none() {
            let (path, _) = paths(&self.root, &self.session)?;
            ensure!(
                !path.try_exists()?,
                "Unconfirmed native lease file remains; retain creation journal"
            );
        }
        if let Some(id) = owned {
            let (path, _) = paths(&self.root, &self.session)?;
            let file = OpenOptions::new()
                .read(true)
                .access_mode(0x80010000)
                .share_mode(1)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
                .open(path)?;
            let creating = self
                .events
                .iter()
                .find_map(|e| {
                    if let Record::Creating { sha256, .. } = e {
                        Some(sha256)
                    } else {
                        None
                    }
                })
                .context("Native lease intent missing")?;
            ensure!(
                ordinary(&file, 1632)? == id
                    && loader_files::digest(&read(&file, 1632)?) == *creating,
                "Foreign native lease left untouched"
            );
            delete(&file)?;
        }
        self.append(Record::Ended {})?;
        let file = self
            .journal
            .take()
            .context("Native lease journal missing")?;
        ensure!(
            ordinary(&file, 65536)? == self.journal_id,
            "Native lease journal replaced"
        );
        delete(&file)?;
        self.ended = true;
        Ok(())
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        let _ = self.cancel();
    }
}
pub(crate) fn pending(root: &Path) -> Result<bool> {
    let data = root.join("Data");
    if !data.exists() {
        return Ok(false);
    }
    let _pin = loader_files::Directories::pin(&data)?;
    for entry in fs::read_dir(data)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if let Some(n) = name.strip_prefix(PREFIX).and_then(|n| {
            n.strip_suffix(".journal")
                .or_else(|| n.strip_suffix(".lease"))
        }) {
            if nonce(n) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
pub(crate) fn resolve_ended(
    root: &Path,
    session: &str,
    closed: impl Fn() -> Result<()>,
) -> Result<()> {
    resolve_ended_inner(root, session, None, closed)
}
/// Root recovery additionally binds the fixed lease header and any published
/// game identity to the retained launch journal. No saved address is replayed.
pub(crate) fn resolve_ended_bound(
    root: &Path,
    session: &str,
    target_exe: &Path,
    expected_game: Option<&Identity>,
    closed: impl Fn() -> Result<()>,
) -> Result<()> {
    resolve_ended_inner(root, session, Some((target_exe, expected_game)), closed)
}
fn resolve_ended_inner(
    root: &Path,
    session: &str,
    expected: Option<(&Path, Option<&Identity>)>,
    closed: impl Fn() -> Result<()>,
) -> Result<()> {
    closed()?;
    let _pins = vec![
        loader_files::Directories::pin(root)?,
        loader_files::Directories::pin(&root.join("Data"))?,
    ];
    let (path, journal_path) = paths(root, session)?;
    let journal = OpenOptions::new()
        .read(true)
        .write(true)
        .access_mode(0xc0010000)
        .share_mode(1)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(journal_path)?;
    let id = ordinary(&journal, 65536)?;
    let bytes = read(&journal, 65536)?;
    ensure!(
        !bytes.is_empty() && bytes.last() == Some(&b'\n'),
        "Incomplete native lease journal; retained"
    );
    let records = bytes
        .split(|b| *b == b'\n')
        .filter(|b| !b.is_empty())
        .map(serde_json::from_slice::<Record>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(
        (1..=4).contains(&records.len()),
        "Native lease journal event count differs"
    );
    let Record::Header {
        schema,
        session: owner_session,
        data,
        worker,
        parent,
        target,
    } = &records[0]
    else {
        anyhow::bail!("Native lease header absent")
    };
    if let Some((target_exe, _)) = expected {
        ensure!(
            crate::native_session::same_path(target, target_exe),
            "Native lease target differs from the retained launch journal"
        );
    }
    let data_file = OpenOptions::new()
        .read(true)
        .share_mode(3)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(root.join("Data"))?;
    ensure!(
        schema == SCHEMA && owner_session == session && *data == Stable::read(&data_file)?,
        "Native lease directory/session identity differs"
    );
    identity(worker)?;
    identity(parent)?;
    let mut desired = None;
    let mut owned = None;
    let mut ended = false;
    for record in records.iter().skip(1) {
        match record {
            Record::Creating { game, sha256 } if desired.is_none() && owned.is_none() && !ended => {
                if let Some((_, expected_game)) = expected {
                    ensure!(
                        expected_game == Some(game),
                        "Native lease game identity differs from the retained launch journal"
                    );
                }
                ensure!(
                    crate::native_session::same_path(&game.path, Path::new(target)),
                    "Native lease game target differs"
                );
                let body = encode(session, game, worker, parent)?;
                ensure!(
                    loader_files::digest(&body) == *sha256,
                    "Native lease creation hash differs"
                );
                desired = Some(body);
            }
            Record::Owned { file } if desired.is_some() && owned.is_none() && !ended => {
                owned = Some(file)
            }
            Record::Ended {} if !ended => ended = true,
            _ => anyhow::bail!("Native lease journal sequence invalid"),
        }
    }
    ensure!(
        ordinary(&journal, 65536)? == id && read(&journal, 65536)? == bytes,
        "Native lease journal changed during recovery"
    );
    if path.try_exists()? {
        let owner = owned.context("Unconfirmed native lease file retained")?;
        let body = desired
            .as_ref()
            .context("Native lease file has no creation intent")?;
        let file = OpenOptions::new()
            .read(true)
            .access_mode(0x80010000)
            .share_mode(1)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(path)?;
        ensure!(
            ordinary(&file, 1632)? == *owner && read(&file, 1632)? == *body,
            "Foreign/replaced native lease left untouched"
        );
        closed()?;
        delete(&file)?;
    }
    closed()?;
    ensure!(
        ordinary(&journal, 65536)? == id,
        "Native lease journal replaced before cleanup"
    );
    delete(&journal)?;
    Ok(())
}
#[cfg(test)]
#[path = "outfit_worker_v2_lease_cancel_review_tests.rs"]
mod cancellation_review_tests;
#[cfg(test)]
mod configuration_tests {
    #[test]
    fn lease_parent_and_worker_match_the_native_fixed_abi() {
        use crate::session_safety::Identity;
        let id = |pid, birth, file: &str| Identity {
            pid,
            started: format!("{birth}"),
            path: format!(r"C:\Fixture\App\{file}"),
        };
        let game = id(100, 1000u64, "NBA2K27.exe");
        let worker = id(200, 2000u64, "worker.exe");
        let parent = id(300, 3000u64, "launcher.exe");
        let b = super::encode(&"a".repeat(48), &game, &worker, &parent).unwrap();
        assert_eq!(u32::from_le_bytes(b[8..12].try_into().unwrap()), game.pid);
        assert_eq!(
            u32::from_le_bytes(b[12..16].try_into().unwrap()),
            worker.pid
        );
        assert_eq!(
            u32::from_le_bytes(b[16..20].try_into().unwrap()),
            parent.pid
        );
        assert_eq!(u64::from_le_bytes(b[40..48].try_into().unwrap()), 3000);
        let path = b[1120..1632]
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .take_while(|v| *v != 0)
            .collect::<Vec<_>>();
        assert_eq!(String::from_utf16(&path).unwrap(), parent.path);
        assert!(super::encode(&"a".repeat(48), &game, &worker, &worker).is_err());
    }
    #[test]
    fn v2_config_carries_freestyle_playlist_but_not_the_event_test() {
        let session = "c".repeat(48);
        let selection = format!("{}{}", "1".repeat(58), "0".repeat(13));
        let base =
            format!("PIPE=x\r\nFREESTYLE_PLAYLIST=992\r\nFREESTYLE_SELECTION={selection}\r\n");
        let config = super::configuration(base.as_bytes(), &session).unwrap();
        assert_eq!(
            String::from_utf8(config).unwrap(),
            format!("{}OUTFIT_V2_SESSION={session}\r\n", base)
        );
        assert!(super::configuration(b"FREESTYLE_EVENT_TEST=992\r\n", &session).is_err());
    }
}
