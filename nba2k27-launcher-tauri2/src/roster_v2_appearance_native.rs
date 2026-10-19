//! Authenticated file-object transport to the already-loaded version proxy.
//! No remote process writes, game calls, code injection, or caller addresses.
use super::journal::Seal;
pub(crate) use super::source::{generation, metadata, person_pointers, verify_pool_profile};
use super::*;
use crate::{file_identity::Stable, loader_files, provider_access::Acl};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
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
const REQUEST_BYTES: usize = 312;
const REPLY_BYTES: usize = 144;
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
    fn create(session: &str, id: &str, suffix: &str) -> Result<Self> {
        let acl = Acl::current_user()?;
        let name = format!("Local\\NBA2K27AppearanceV2-{session}-{id}-{suffix}")
            .encode_utf16()
            .chain([0])
            .collect::<Vec<_>>();
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
        let mut d = Descriptor(PSECURITY_DESCRIPTOR::default());
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(text.as_ptr()),
                SDDL_REVISION_1,
                &mut d.0,
                None,
            )?;
        }
        let attrs = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: d.0.0,
            bInheritHandle: false.into(),
        };
        let event =
            Self(unsafe { CreateEventW(Some(&attrs), true, false, PCWSTR(name.as_ptr()))? });
        ensure!(
            unsafe { GetLastError() } != ERROR_ALREADY_EXISTS,
            "Appearance event already exists; never adopt"
        );
        Ok(event)
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
fn ordinary(f: &File) -> Result<()> {
    let mut i = BY_HANDLE_FILE_INFORMATION::default();
    unsafe {
        GetFileInformationByHandle(HANDLE(f.as_raw_handle()), &mut i)?;
    }
    ensure!(
        i.dwFileAttributes & 0x410 == 0 && i.nNumberOfLinks == 1,
        "Appearance transport file must be ordinary/single-link"
    );
    Ok(())
}
fn file_id(f: &File) -> Result<(u64, [u8; 16])> {
    ordinary(f)?;
    let mut i = FILE_ID_INFO::default();
    unsafe {
        GetFileInformationByHandleEx(
            HANDLE(f.as_raw_handle()),
            FileIdInfo,
            &mut i as *mut FILE_ID_INFO as _,
            std::mem::size_of::<FILE_ID_INFO>() as u32,
        )?;
    }
    ensure!(
        i.VolumeSerialNumber != 0 && i.FileId.Identifier.iter().any(|b| *b != 0),
        "Appearance reply persistent identity unavailable"
    );
    Ok((i.VolumeSerialNumber, i.FileId.Identifier))
}
// Cancellation is ordered BEFORE either transport file can unlock on error or
// unwind. The monitor's outer Result handler is too late: locals would already
// have dropped while an admitted native command could still be pending.
struct Flight<'a> {
    cancel: &'a Event,
    reply: Option<File>,
    request: Option<File>,
    completed: bool,
}
impl Drop for Flight<'_> {
    fn drop(&mut self) {
        if !self.completed {
            let _ = self.cancel.set();
        }
    }
}
struct State {
    forward_attempted: bool,
    owned: bool,
    reverse_attempted: bool,
    closed: bool,
    poisoned: bool,
}
pub(crate) struct Operation {
    root: PathBuf,
    owner: Owner,
    id: String,
    cancel: Event,
    ready: Event,
    _dirs: loader_files::Directories,
    state: Mutex<State>,
}
impl Drop for Operation {
    fn drop(&mut self) {
        let _ = self.cancel.set();
    }
}
impl Operation {
    pub(crate) fn new(root: &Path, owner: &Owner, id: &str) -> Result<Self> {
        owner.validate()?;
        ensure!(
            owner.kind == Kind::Player
                && id.len() == 48
                && id.bytes().all(|b| b.is_ascii_hexdigit()),
            "Appearance operation binding invalid"
        );
        let dirs = loader_files::Directories::pin(&root.join("Data"))?;
        let cancel = Event::create(&owner.session, id, "Cancel")?;
        let ready = Event::create(&owner.session, id, "Ready")?;
        Ok(Self {
            root: root.to_owned(),
            owner: owner.clone(),
            id: id.into(),
            cancel,
            ready,
            _dirs: dirs,
            state: Mutex::new(State {
                forward_attempted: false,
                owned: false,
                reverse_attempted: false,
                closed: false,
                poisoned: false,
            }),
        })
    }
    fn invoke(
        &self,
        s: &Snapshot,
        p: &Plan,
        op: u32,
        seal: Option<&Seal>,
        check: &impl Fn() -> Result<()>,
    ) -> Result<u32> {
        ensure!(
            s.generation.owner == self.owner && p.origin == *s,
            "Appearance native operation source differs"
        );
        check()?;
        let (row, pointers, metadata) = super::journal::fields(s, p)?;
        let digest = if op == 4 {
            [0; 32]
        } else {
            seal.context("Appearance durable seal missing")?
                .digest_for(&self.id, s, p, op)?
        };
        self.ready.reset()?;
        // Create the reply FIRST and keep its exclusive file object alive throughout.
        // Native duplicates this exact object from the retained authenticated parent;
        // no path-created response or matching bytes can mint atomic ownership.
        let reply_path = self
            .root
            .join("Data")
            .join(format!("ap2-{}.reply", self.owner.session));
        let mut flight = Flight {
            cancel: &self.cancel,
            reply: None,
            request: None,
            completed: false,
        };
        flight.reply = Some(
            OpenOptions::new()
                .read(true)
                .write(true)
                .access_mode(0xc0010000)
                .share_mode(0)
                .create_new(true)
                .custom_flags(0x200000 | 0x80000000)
                .open(&reply_path)?,
        );
        let reply = flight
            .reply
            .as_mut()
            .context("Appearance reply handle absent")?;
        let (reply_volume, reply_id) = file_id(&reply)?;
        let stable_reply = Stable::read(&reply)?;
        let mut packet = Vec::with_capacity(REQUEST_BYTES);
        packet.extend_from_slice(b"C27APR01");
        for v in [1u32, op, self.owner.game.pid, self.owner.index as u32] {
            packet.extend_from_slice(&v.to_le_bytes());
        }
        packet.extend_from_slice(&self.owner.game.started.parse::<u64>()?.to_le_bytes());
        packet.extend_from_slice(self.owner.session.as_bytes());
        packet.extend_from_slice(self.id.as_bytes());
        packet.extend_from_slice(&s.words);
        packet.extend_from_slice(&p.after);
        packet.extend_from_slice(&p.mask);
        packet.extend_from_slice(&0u32.to_le_bytes());
        for sha in [row, pointers, metadata, digest] {
            packet.extend_from_slice(&sha);
        }
        packet.extend_from_slice(&std::process::id().to_le_bytes());
        packet.extend_from_slice(&0u32.to_le_bytes());
        packet.extend_from_slice(&(reply.as_raw_handle() as usize as u64).to_le_bytes());
        packet.extend_from_slice(&reply_volume.to_le_bytes());
        packet.extend_from_slice(&reply_id);
        ensure!(
            packet.len() == REQUEST_BYTES,
            "Appearance request layout differs"
        );
        let request_hash: [u8; 32] = Sha256::digest(&packet).into();
        let request_path = self
            .root
            .join("Data")
            .join(format!("ap2-{}.request", self.owner.session));
        flight.request = Some(
            OpenOptions::new()
                .read(true)
                .write(true)
                .access_mode(0xc0010000)
                .share_mode(0)
                .create_new(true)
                .custom_flags(0x200000 | 0x80000000)
                .open(&request_path)?,
        );
        let request = flight
            .request
            .as_mut()
            .context("Appearance request writer absent")?;
        ordinary(&request)?;
        let stable_request = Stable::read(&request)?;
        request.write_all(&packet)?;
        request.sync_all()?;
        request.seek(SeekFrom::Start(0))?;
        let mut readback = vec![0; REQUEST_BYTES];
        request.read_exact(&mut readback)?;
        ensure!(
            readback == packet && Stable::read(&request)? == stable_request,
            "Appearance request durability failed"
        );
        flight.request.take();
        // Reopen read+DELETE denying all writers/replacers. Native reader explicitly
        // shares DELETE; this keeps the exact request immutable until its ack.
        flight.request = Some(
            OpenOptions::new()
                .read(true)
                .access_mode(0x80010000)
                .share_mode(1)
                .custom_flags(0x200000)
                .open(&request_path)?,
        );
        let request = flight
            .request
            .as_ref()
            .context("Appearance retained request absent")?;
        ensure!(
            Stable::read(&request)? == stable_request,
            "Appearance request replaced before native observation"
        );
        let started = Instant::now();
        loop {
            check()?;
            let wait = unsafe { WaitForSingleObject(self.ready.0, 20) };
            if wait == WAIT_OBJECT_0 {
                break;
            }
            ensure!(
                wait == WAIT_TIMEOUT && started.elapsed() < Duration::from_secs(8),
                "Appearance native acknowledgement absent/uncertain"
            );
        }
        ensure!(
            Stable::read(&reply)? == stable_reply
                && file_id(&reply)? == (reply_volume, reply_id)
                && reply.metadata()?.len() == REPLY_BYTES as u64,
            "Appearance reply object changed"
        );
        reply.seek(SeekFrom::Start(0))?;
        let mut result = vec![0; REPLY_BYTES];
        reply.read_exact(&mut result)?;
        let outcome = parse_reply(&result, &self.owner.session, &self.id, &request_hash)?;
        // Native Ready is signalled AFTER both of its duplicated file handles close.
        ensure!(
            Stable::read(&request)? == stable_request,
            "Appearance request object changed before cleanup"
        );
        loader_files::unlink(&request)?;
        loader_files::unlink(&reply)?;
        flight.completed = true;
        Ok(outcome)
    }
    pub(crate) fn probe(
        &self,
        s: &Snapshot,
        p: &Plan,
        check: &impl Fn() -> Result<()>,
    ) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Appearance operation poisoned"))?;
        ensure!(
            !state.poisoned && !state.forward_attempted && !state.closed,
            "Appearance probe cannot adopt previous work"
        );
        match self.invoke(s, p, 4, None, check) {
            Ok(7) => Ok(()),
            Ok(_) => anyhow::bail!("Native appearance source is not ready"),
            Err(e) => {
                state.poisoned = true;
                let _ = self.cancel.set();
                Err(e)
            }
        }
    }
    pub(crate) fn compare(
        &self,
        permit: &Permit<'_>,
        seal: &Seal,
        check: &impl Fn() -> Result<()>,
    ) -> Result<CasOutcome> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Appearance operation poisoned"))?;
        ensure!(
            !state.poisoned && !state.closed,
            "Appearance native operation held"
        );
        let op = if permit.undo {
            ensure!(
                state.owned && !state.reverse_attempted,
                "Appearance undo requires native owned receipt"
            );
            state.reverse_attempted = true;
            state.owned = false;
            2
        } else {
            ensure!(
                !state.forward_attempted,
                "Appearance native effect never retries"
            );
            state.forward_attempted = true;
            1
        };
        let outcome = match self.invoke(permit.snapshot, permit.plan, op, Some(seal), check) {
            Ok(v) => v,
            Err(_) => {
                state.poisoned = true;
                let _ = self.cancel.set();
                return Ok(CasOutcome::Uncertain);
            }
        };
        Ok(match (op, outcome) {
            (1, 1) => {
                state.owned = true;
                CasOutcome::Applied(permit.applied_receipt())
            }
            (1, 2) => {
                state.owned = true;
                CasOutcome::FailedAfterApplied(
                    permit.applied_receipt(),
                    anyhow::anyhow!("Native appearance applied but post-check held"),
                )
            }
            (1, 0 | 6) => {
                state.closed = true;
                CasOutcome::NoEffect
            }
            (2, 3) => {
                state.closed = true;
                CasOutcome::Applied(permit.applied_receipt())
            }
            _ => {
                state.poisoned = true;
                CasOutcome::Uncertain
            }
        })
    }
    pub(crate) fn commit(
        &self,
        s: &Snapshot,
        p: &Plan,
        seal: &Seal,
        check: &impl Fn() -> Result<()>,
    ) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Appearance operation poisoned"))?;
        ensure!(
            !state.poisoned && !state.closed && state.owned && !state.reverse_attempted,
            "Appearance finalization lacks actual native receipt"
        );
        state.owned = false;
        state.closed = true;
        match self.invoke(s, p, 3, Some(seal), check) {
            Ok(4) => Ok(()),
            _ => {
                state.poisoned = true;
                let _ = self.cancel.set();
                anyhow::bail!("Appearance native commit uncertain; no replay or inverse permitted")
            }
        }
    }
}
fn parse_reply(b: &[u8], session: &str, id: &str, request: &[u8; 32]) -> Result<u32> {
    ensure!(
        b.len() == REPLY_BYTES
            && &b[..8] == b"C27APS01"
            && b[8..56] == *session.as_bytes()
            && b[56..104] == *id.as_bytes()
            && b[104..136] == request[..]
            && b[140..144] == [0; 4],
        "Appearance authenticated response differs"
    );
    let result = u32::from_le_bytes(b[136..140].try_into()?);
    ensure!(result <= 7, "Unknown appearance native result");
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn response_binds_all_fields_and_rejects_torn_or_foreign() {
        let s = "a".repeat(48);
        let id = "b".repeat(48);
        let hash = [9; 32];
        let mut b = Vec::new();
        b.extend_from_slice(b"C27APS01");
        b.extend_from_slice(s.as_bytes());
        b.extend_from_slice(id.as_bytes());
        b.extend_from_slice(&hash);
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(parse_reply(&b, &s, &id, &hash).unwrap(), 1);
        for n in 0..144 {
            assert!(parse_reply(&b[..n], &s, &id, &hash).is_err());
        }
        for at in [0, 8, 55, 56, 103, 104, 135, 140, 143] {
            let mut changed = b.clone();
            changed[at] ^= 1;
            assert!(parse_reply(&changed, &s, &id, &hash).is_err());
        }
        b[136..140].copy_from_slice(&8u32.to_le_bytes());
        assert!(parse_reply(&b, &s, &id, &hash).is_err());
    }
}

#[cfg(test)]
mod flight_tests {
    use super::*;
    #[test]
    fn failed_flight_signals_private_cancel_and_success_does_not() {
        let session = crate::backend::unique_id();
        let id = crate::backend::unique_id();
        let cancel = Event::create(&session, &id, "Cancel").unwrap();
        assert_eq!(unsafe { WaitForSingleObject(cancel.0, 0) }, WAIT_TIMEOUT);
        {
            let _flight = Flight {
                cancel: &cancel,
                reply: None,
                request: None,
                completed: false,
            };
        }
        assert_eq!(unsafe { WaitForSingleObject(cancel.0, 0) }, WAIT_OBJECT_0);
        cancel.reset().unwrap();
        {
            let _flight = Flight {
                cancel: &cancel,
                reply: None,
                request: None,
                completed: true,
            };
        }
        assert_eq!(unsafe { WaitForSingleObject(cancel.0, 0) }, WAIT_TIMEOUT);
    }
}
