//! Provider-owned Version2 scoreboard runner. Exact launch identities, real
//! retained native adapter, pinned append-only intent and closed-only recovery.
//! Not wired automatically. No journal address is ever replayed after restart.
#![allow(dead_code)]
use crate::{
    scoreboard_profile::{self, Plan},
    scoreboard_session::{Journal as JournalApi, Permit, Service, Status},
    scoreboard_v2::Snapshot,
    session_safety::{self, Identity},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
pub(crate) const MARKER: &str = "blacktop-scoreboard-v2-session.jsonl";
const LIMIT: u64 = 2 * 1024 * 1024;
const SCHEMA: &str = "nba2k27-scoreboard-v2-session-v1";
#[derive(Clone, Debug)]
pub(crate) struct Binding {
    game: Identity,
    parent: Identity,
    game_root: PathBuf,
    portable: PathBuf,
    session: String,
}
impl Binding {
    pub(crate) fn from_launched(
        game: Identity,
        parent: Identity,
        game_root: PathBuf,
        portable: PathBuf,
        session: String,
    ) -> Result<Self> {
        ensure!(
            game.pid > 0
                && parent.pid > 0
                && game.pid != parent.pid
                && game.started.parse::<u64>().is_ok_and(|v| v > 0)
                && parent.started.parse::<u64>().is_ok_and(|v| v > 0)
                && Path::new(&game.path).is_absolute()
                && Path::new(&parent.path).is_absolute()
                && game_root.is_absolute()
                && portable.is_absolute()
                && crate::native_session::same_path(&game.path, &game_root.join("NBA2K27.exe"))
                && session.len() == 48
                && session.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid exact-owned scoreboard launch binding"
        );
        Ok(Self {
            game,
            parent,
            game_root,
            portable,
            session,
        })
    }
}
fn transaction(s: &Snapshot, p: &Plan) -> Result<Value> {
    ensure!(
        s.plan()? == *p && p.changes.len() <= 3,
        "Scoreboard journal plan differs from snapshot"
    );
    let c = s.context();
    Ok(
        json!({"base":s.base(),"choice":s.choice().id,"context":{"currentIndex":c.current_index,"alternateIndex":c.alternate_index,"current":c.current,"alternate":c.alternate,"stage":c.stage,"active":c.active,"owner":c.owner,"kind":c.kind,"gameMode":c.game_mode,"rootTag":c.root_tag},"fields":s.fields().iter().map(|(a,b)|Ok(json!({"at":a,"allocation":s.allocation(*a)?,"bytes":b}))).collect::<Result<Vec<_>>>()?,"current":p.current,"wanted":p.wanted,"changes":p.changes.iter().map(|c|json!({"at":c.address,"before":c.before,"after":c.after})).collect::<Vec<_>>()}),
    )
}
fn fingerprint(v: &Value) -> Result<String> {
    Ok(format!("{:X}", Sha256::digest(serde_json::to_vec(v)?)))
}
struct Proof {
    offset: u64,
    bytes: Vec<u8>,
    fingerprint: String,
    undo: bool,
    applied: usize,
    restored: usize,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Header {
    event: String,
    schema: String,
    session: String,
    game: Identity,
    parent: Identity,
    choice: String,
    file_identity: crate::file_identity::Stable,
}
impl Header {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.event == "header"
                && self.schema == SCHEMA
                && self.session.len() == 48
                && self.session.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid scoreboard journal header"
        );
        scoreboard_profile::choice(&self.choice)?;
        for id in [&self.game, &self.parent] {
            ensure!(
                id.pid > 0
                    && id.started.parse::<u64>().is_ok_and(|v| v > 0)
                    && Path::new(&id.path).is_absolute(),
                "Invalid scoreboard journal identity"
            );
        }
        ensure!(
            Path::new(&self.game.path)
                .file_name()
                .is_some_and(|p| p.to_string_lossy().eq_ignore_ascii_case("NBA2K27.exe")),
            "Invalid scoreboard journal game name"
        );
        Ok(())
    }
}
struct Journal {
    file: File,
    size: u64,
    events: usize,
    identity: crate::file_identity::Stable,
    _pins: Vec<crate::loader_files::Directories>,
    proof: Option<Proof>,
}
impl Journal {
    fn create(b: &Binding, id: &str) -> Result<Self> {
        scoreboard_profile::choice(id)?;
        let pins = vec![
            crate::loader_files::Directories::pin(&b.portable)?,
            crate::loader_files::Directories::pin(&b.portable.join("Data"))?,
        ];
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .share_mode(1)
            .custom_flags(0x00200000)
            .open(b.portable.join("Data").join(MARKER))?;
        let meta = file.metadata()?;
        ensure!(
            meta.is_file() && meta.file_attributes() & 0x400 == 0,
            "Scoreboard intent is not an ordinary file"
        );
        let identity = crate::file_identity::Stable::read(&file)?;
        let mut j = Self {
            file,
            size: 0,
            events: 0,
            identity,
            _pins: pins,
            proof: None,
        };
        let header = Header {
            event: "header".into(),
            schema: SCHEMA.into(),
            session: b.session.clone(),
            game: b.game.clone(),
            parent: b.parent.clone(),
            choice: id.into(),
            file_identity: j.identity.clone(),
        };
        header.validate()?;
        j.append(serde_json::to_value(header)?)?;
        Ok(j)
    }
    fn append(&mut self, value: Value) -> Result<(u64, Vec<u8>)> {
        let mut bytes = serde_json::to_vec(&value)?;
        bytes.push(b'\n');
        ensure!(
            bytes.len() <= 64 * 1024
                && self.events < 2048
                && self
                    .size
                    .checked_add(bytes.len() as u64)
                    .is_some_and(|v| v <= LIMIT),
            "Scoreboard durable log budget exhausted"
        );
        self.validate()?;
        let offset = self.size;
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write_all(&bytes)?;
        self.file.sync_all()?;
        self.size += bytes.len() as u64;
        self.events += 1;
        self.readback(offset, &bytes)?;
        Ok((offset, bytes))
    }
    fn validate(&self) -> Result<()> {
        let meta = self.file.metadata()?;
        ensure!(
            meta.is_file()
                && meta.file_attributes() & 0x400 == 0
                && meta.len() == self.size
                && self.size <= LIMIT
                && crate::file_identity::Stable::read(&self.file)? == self.identity,
            "Scoreboard durable receipt identity or size changed"
        );
        Ok(())
    }
    fn readback(&self, offset: u64, expected: &[u8]) -> Result<()> {
        self.validate()?;
        ensure!(
            offset
                .checked_add(expected.len() as u64)
                .is_some_and(|v| v <= self.size),
            "Scoreboard proof escapes log"
        );
        let mut file = &self.file;
        file.seek(SeekFrom::Start(offset))?;
        let mut actual = vec![0; expected.len()];
        file.read_exact(&mut actual)?;
        ensure!(actual == expected, "Scoreboard durable proof bytes differ");
        Ok(())
    }
    fn save_intent(
        &mut self,
        s: &Snapshot,
        p: &Plan,
        undo: bool,
        applied: usize,
        restored: usize,
    ) -> Result<()> {
        ensure!(
            applied <= p.changes.len() && restored <= applied,
            "Invalid scoreboard intent prefix"
        );
        let tx = transaction(s, p)?;
        let hash = fingerprint(&tx)?;
        let(offset,bytes)=self.append(json!({"event":"intent","undo":undo,"applied":applied,"restored":restored,"transaction":tx,"sha256":hash}))?;
        self.proof = Some(Proof {
            offset,
            bytes,
            fingerprint: hash,
            undo,
            applied,
            restored,
        });
        Ok(())
    }
}
impl JournalApi for Journal {
    fn intent(&mut self, s: &Snapshot, p: &Plan) -> Result<()> {
        self.save_intent(s, p, false, 0, 0)
    }
    fn restore_intent(&mut self, s: &Snapshot, p: &Plan, a: usize, r: usize) -> Result<()> {
        self.save_intent(s, p, true, a, r)
    }
    fn verify(&self, p: &Permit<'_>) -> Result<()> {
        let proof = self
            .proof
            .as_ref()
            .context("Scoreboard publication has no durable intent")?;
        let (a, r) = p.progress();
        ensure!(
            proof.undo == p.undo()
                && proof.fingerprint == fingerprint(&transaction(p.snapshot(), p.plan())?)?
                && if p.undo() {
                    a == proof.applied && r >= proof.restored && r < a
                } else {
                    proof.applied == 0
                        && proof.restored == 0
                        && r == 0
                        && a < p.plan().changes.len()
                },
            "Scoreboard publication differs from exact durable transaction"
        );
        p.change()?;
        self.readback(proof.offset, &proof.bytes)
    }
    fn progress(&mut self, a: usize, r: usize) -> Result<()> {
        ensure!(a <= 3 && r <= a, "Invalid scoreboard progress note");
        self.append(json!({"event":"progress","applied":a,"restored":r}))?;
        Ok(())
    }
    fn resolved(&mut self) -> Result<()> {
        self.append(json!({"event":"resolved","message":"Owned prefix restored or exact game ended; no memory replay"}))?;
        self.proof = None;
        Ok(())
    }
}
#[path = "scoreboard_native.rs"]
mod native;
pub(crate) struct Runner {
    binding: Binding,
    desired: String,
    ready: bool,
    service: Service,
    native: Option<native::Native>,
    journal: Option<Journal>,
    status: Status,
    held: bool,
    failure_reason: Option<String>,
}
impl Runner {
    pub(crate) fn new(binding: Binding, id: &str, layout_ready: bool) -> Result<Self> {
        let c = scoreboard_profile::choice(id)?;
        ensure!(
            !c.requires_layout_adapter || layout_ready,
            "Selected scoreboard layout was not prepared before Steam"
        );
        Ok(Self {
            binding,
            desired: id.into(),
            ready: layout_ready,
            service: Service::new(id)?,
            native: None,
            journal: None,
            status: Status::Waiting,
            held: false,
            failure_reason: None,
        })
    }
    pub(crate) fn choose(&mut self, id: &str, layout_ready: bool) -> Result<()> {
        let c = scoreboard_profile::choice(id)?;
        ensure!(
            !c.requires_layout_adapter || layout_ready,
            "Selected scoreboard needs the next Play launch to prepare its exact layout"
        );
        ensure!(
            !self.held,
            "Scoreboard is held for this game; restart before another selection"
        );
        self.service.choose(id)?;
        self.desired = id.into();
        self.ready = layout_ready;
        Ok(())
    }
    pub(crate) fn status(&self) -> Value {
        json!({"version2":true,"selected":self.desired,"held":self.held,"phase":match &self.status{Status::Waiting=>"waiting",Status::Ready=>"ready",Status::Restored=>if self.held{"restored-held"}else{"restored"},Status::Held(_)=>"held"},"message":match &self.status{Status::Waiting=>"Waiting for stable Blacktop selection".to_owned(),Status::Ready=>"Scoreboard prepared for Blacktop".to_owned(),Status::Restored=>if self.held{"Original scoreboard restored; preparation remains held until restart"}else{"Original scoreboard restored; next selection will rearm"}.to_owned(),Status::Held(s)=>s.clone()},"failureReason":self.failure_reason.as_ref().or_else(|| if self.held { if let Status::Held(reason)=&self.status {Some(reason)} else {None} } else {None}),"ownsMutation":self.service.owns_mutation(),"gameplayVerified":false})
    }
    /// One serialized provider feature poll. A held feature never aborts the
    /// independent loader route; keep Runner so safe-menu restoration can retry.
    pub(crate) fn tick(&mut self, parent_alive: bool) -> Result<()> {
        let _gate = session_safety::write_gate()?;
        if self.held && !self.service.owns_mutation() {
            return Ok(());
        }
        if self.native.is_none() {
            if !parent_alive {
                return Ok(());
            }
            match native::Native::open(&self.binding) {
                Ok(n) => self.native = Some(n),
                Err(e) => {
                    self.status = Status::Held(format!(
                        "Waiting for exact Version2 scoreboard owner: {e:#}"
                    ));
                    return Ok(());
                }
            }
        }
        if self.journal.is_none() {
            match Journal::create(&self.binding, &self.desired) {
                Ok(j) => self.journal = Some(j),
                Err(e) => {
                    self.status =
                        Status::Held(format!("Scoreboard intent unavailable; no writes: {e:#}"));
                    self.held = true;
                    return Ok(());
                }
            }
        }
        let status = self.service.tick_owned(
            self.native.as_mut().unwrap(),
            self.journal.as_mut().unwrap(),
            self.ready,
            parent_alive && !self.held,
        );
        self.observe_status(status);
        Ok(())
    }
    fn observe_status(&mut self, status: Status) {
        // Preserve the first transaction fault even after a successful owned
        // restore. Preflight waits without any owned transaction remain soft.
        if self.held && self.failure_reason.is_none() {
            if let Status::Held(reason) = &self.status {
                self.failure_reason = Some(reason.clone());
            }
        }
        if self.held && matches!(status, Status::Waiting) {
            return;
        }
        if let Status::Held(reason) = &status {
            if self.held || self.service.owns_mutation() {
                self.failure_reason.get_or_insert_with(|| reason.clone());
                self.held = true;
            }
        }
        self.status = status;
    }
    pub(crate) fn finish_ended(&mut self, rows: &[session_safety::Process]) -> Result<()> {
        ensure!(
            !rows
                .iter()
                .any(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe")),
            "Game still present; scoreboard log held"
        );
        if let Some(n) = &self.native {
            n.ended()?;
        }
        // Exact process exit + fresh global absence is the cleanup authority;
        // appending one more diagnostic must not strand a valid full journal.
        // remove_ended still checks complete lines and the kernel FileID, so a
        // genuinely torn/foreign receipt remains held rather than discarded.
        drop(self.journal.take());
        remove_ended(
            &self.binding.portable,
            &self.binding.game,
            &self.binding.session,
            rows,
        )?;
        self.native = None;
        Ok(())
    }
}
pub(crate) fn pending(root: &Path) -> Result<bool> {
    match fs::symlink_metadata(root.join("Data").join(MARKER)) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}
pub(crate) fn remove_ended(
    root: &Path,
    game: &Identity,
    session: &str,
    rows: &[session_safety::Process],
) -> Result<()> {
    if !pending(root)? {
        return Ok(());
    }
    ensure!(
        !rows
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe")),
        "Close every NBA2K27 before scoreboard receipt recovery"
    );
    let _pins = [
        crate::loader_files::Directories::pin(root)?,
        crate::loader_files::Directories::pin(&root.join("Data"))?,
    ];
    let mut file = fs::OpenOptions::new()
        .read(true)
        .access_mode(0x80000000 | 0x10000)
        .share_mode(1)
        .custom_flags(0x00200000)
        .open(root.join("Data").join(MARKER))?;
    let meta = file.metadata()?;
    ensure!(
        meta.is_file() && meta.file_attributes() & 0x400 == 0 && (1..=LIMIT).contains(&meta.len()),
        "Unsupported scoreboard recovery log"
    );
    let mut bytes = Vec::new();
    (&mut file).take(LIMIT + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 == meta.len() && bytes.last() == Some(&b'\n'),
        "Incomplete scoreboard recovery log retained"
    );
    let mut lines = bytes[..bytes.len() - 1].split(|b| *b == b'\n');
    let h: Header =
        serde_json::from_slice(lines.next().context("Missing scoreboard recovery header")?)?;
    h.validate()?;
    ensure!(
        h.session == session
            && h.game == *game
            && h.file_identity == crate::file_identity::Stable::read(&file)?,
        "Scoreboard recovery belongs to another session"
    );
    for (i, line) in lines.enumerate() {
        ensure!(i < 2048, "Scoreboard recovery event bound exceeded");
        let v: Value = serde_json::from_slice(line)?;
        ensure!(
            matches!(
                v["event"].as_str(),
                Some("intent" | "progress" | "resolved")
            ),
            "Unknown scoreboard recovery event retained"
        );
    }
    crate::loader_files::unlink(&file)
}
#[cfg(test)]
#[path = "scoreboard_runtime_tests.rs"]
mod tests;
