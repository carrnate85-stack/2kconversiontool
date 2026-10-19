//! Immutable composite evidence; never replays memory and never clears pending on Drop.
use super::*;
use crate::{backend, file_identity::Stable, loader_files};
use serde_json::json;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use windows::Win32::{Foundation::HANDLE, Storage::FileSystem::*};
pub(crate) const MARKER: &str = "roster-v2-appearance-pending.bin";
const LIMIT: usize = 65536;
fn hex(b: &[u8]) -> String {
    b.iter().map(|v| format!("{v:02x}")).collect()
}
pub(crate) fn fields(s: &Snapshot, p: &Plan) -> Result<([u8; 32], [u8; 32], [u8; 32])> {
    ensure!(p.origin == *s, "Appearance journal plan origin differs");
    let mut h = Sha256::new();
    h.update((s.pointers.len() as u32).to_le_bytes());
    for v in &s.pointers {
        h.update(v.to_le_bytes());
    }
    h.update((s.other_pointers.len() as u32).to_le_bytes());
    for v in &s.other_pointers {
        h.update(v.to_le_bytes());
    }
    Ok((
        Sha256::digest(&s.row).into(),
        h.finalize().into(),
        s.native_metadata
            .context("Native appearance metadata absent")?,
    ))
}
fn ordinary(file: &File) -> Result<()> {
    let mut i = BY_HANDLE_FILE_INFORMATION::default();
    unsafe {
        GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut i)?;
    }
    ensure!(
        i.dwFileAttributes & 0x410 == 0 && i.nNumberOfLinks == 1,
        "Appearance evidence is not an ordinary single-link file"
    );
    Ok(())
}
struct Pin {
    file: File,
    id: Stable,
    bytes: Vec<u8>,
}
impl Pin {
    fn verify(&self) -> Result<()> {
        ordinary(&self.file)?;
        ensure!(
            Stable::read(&self.file)? == self.id
                && self.file.metadata()?.len() == self.bytes.len() as u64,
            "Appearance evidence identity changed"
        );
        let mut f = &self.file;
        f.seek(SeekFrom::Start(0))?;
        let mut b = vec![0; self.bytes.len()];
        f.read_exact(&mut b)?;
        ensure!(b == self.bytes, "Appearance evidence bytes changed");
        Ok(())
    }
}
fn create(path: &Path, b: Vec<u8>) -> Result<Pin> {
    ensure!(b.len() <= 262144, "Appearance evidence bound");
    let mut f = OpenOptions::new()
        .read(true)
        .write(true)
        .access_mode(0xc0010000)
        .share_mode(0)
        .create_new(true)
        .custom_flags(0x200000 | 0x80000000)
        .open(path)?;
    ordinary(&f)?;
    let id = Stable::read(&f)?;
    f.write_all(&b)?;
    f.sync_all()?;
    f.seek(SeekFrom::Start(0))?;
    let mut check = vec![0; b.len()];
    f.read_exact(&mut check)?;
    ensure!(
        check == b && Stable::read(&f)? == id,
        "Appearance evidence durability failed"
    );
    drop(f);
    let f = OpenOptions::new()
        .read(true)
        .access_mode(0x80010000)
        .share_mode(1)
        .custom_flags(0x200000)
        .open(path)?;
    ensure!(
        Stable::read(&f)? == id,
        "Appearance evidence replaced during pin"
    );
    let pin = Pin {
        file: f,
        id,
        bytes: b,
    };
    pin.verify()?;
    Ok(pin)
}
#[derive(Clone)]
pub(crate) struct Seal {
    pin: Arc<Pin>,
    id: String,
    session: String,
    operation: u32,
    index: usize,
    before: [u8; 4],
    after: [u8; 4],
    mask: [u8; 4],
    row: [u8; 32],
    pointers: [u8; 32],
    metadata: [u8; 32],
}
impl Seal {
    pub(crate) fn digest_for(
        &self,
        id: &str,
        s: &Snapshot,
        p: &Plan,
        operation: u32,
    ) -> Result<[u8; 32]> {
        let (row, pointers, metadata) = fields(s, p)?;
        ensure!(
            self.id == id
                && self.session == s.generation.owner.session
                && self.operation == operation
                && self.index == s.generation.owner.index
                && self.before == s.words
                && self.after == p.after
                && self.mask == p.mask
                && self.row == row
                && self.pointers == pointers
                && self.metadata == metadata,
            "Appearance phase seal differs"
        );
        self.pin.verify()?;
        Ok(Sha256::digest(&self.pin.bytes).into())
    }
}
struct State {
    root: PathBuf,
    id: String,
    owner: Owner,
    marker: Pin,
    _dirs: loader_files::Directories,
    current: Option<Seal>,
    audit: u32,
    finished: bool,
    poisoned: bool,
    pins: Vec<Arc<Pin>>,
}
#[derive(Clone)]
pub(crate) struct Durable(Arc<Mutex<State>>);
impl Durable {
    pub(crate) fn begin(
        root: &Path,
        owner: &Owner,
        record: &writer::Plan,
        s: &Snapshot,
        p: &Plan,
    ) -> Result<Self> {
        owner.validate()?;
        ensure!(
            s.generation.owner == *owner
                && p.origin == *s
                && record.kind == Kind::Player
                && record.index == owner.index
                && record.before == s.row
                && record.after.len() == Kind::Player.stride()
                && record.expected(record.patches.len())? == record.after,
            "Appearance composite preparation differs"
        );
        fields(s, p)?;
        let dirs = loader_files::Directories::pin(&root.join("Data"))?;
        let id = backend::unique_id();
        let body = serde_json::to_vec(
            &json!({"schema":"appearance-composite-v1","id":id,"owner":owner,"recordBefore":hex(&record.before),"recordAfter":hex(&record.after),"changed":record.changed,"appearanceBefore":s.words,"appearanceAfter":p.after,"appearanceMask":p.mask,"appearanceProof":format!("{:x}",Sha256::digest(include_bytes!("../assets/appearance-v2-production-proof.json")))}),
        )?;
        ensure!(body.len() <= LIMIT, "Appearance prepared evidence bound");
        let marker = create(&root.join("Data").join(MARKER), body.clone())?;
        let prepared = Arc::new(create(
            &root.join("Data").join(format!("ap2-{id}.prepared")),
            body,
        )?);
        Ok(Self(Arc::new(Mutex::new(State {
            root: root.to_owned(),
            id,
            owner: owner.clone(),
            marker,
            _dirs: dirs,
            current: None,
            audit: 0,
            finished: false,
            poisoned: false,
            pins: vec![prepared],
        }))))
    }
    pub(crate) fn id(&self) -> Result<String> {
        Ok(self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Appearance journal lock poisoned"))?
            .id
            .clone())
    }
    pub(crate) fn current_seal(&self) -> Result<Seal> {
        let st = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Appearance journal lock poisoned"))?;
        ensure!(
            !st.finished && !st.poisoned,
            "Appearance journal unavailable"
        );
        st.current
            .clone()
            .context("Appearance durable phase absent")
    }
    fn phase(&self, s: &Snapshot, p: &Plan, op: u32) -> Result<Seal> {
        let (row, pointers, metadata) = fields(s, p)?;
        let mut st = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Appearance journal lock poisoned"))?;
        ensure!(
            !st.finished && !st.poisoned && st.owner == s.generation.owner,
            "Appearance journal binding changed"
        );
        st.marker.verify()?;
        let mut b = Vec::new();
        b.extend_from_slice(b"C27APJ01");
        b.extend_from_slice(st.id.as_bytes());
        b.extend_from_slice(st.owner.session.as_bytes());
        b.extend_from_slice(&st.owner.game.pid.to_le_bytes());
        b.extend_from_slice(&st.owner.game.started.parse::<u64>()?.to_le_bytes());
        b.extend_from_slice(&(st.owner.index as u32).to_le_bytes());
        b.extend_from_slice(&op.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&s.words);
        b.extend_from_slice(&p.after);
        b.extend_from_slice(&p.mask);
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&row);
        b.extend_from_slice(&pointers);
        b.extend_from_slice(&metadata);
        ensure!(b.len() == 240, "Appearance phase layout");
        b.extend_from_slice(&serde_json::to_vec(&json!({"owner":st.owner,"appearance":s.appearance,"allocation":s.allocation,"row":hex(&s.row),"phase":op}))?);
        ensure!(b.len() <= LIMIT, "Appearance phase evidence bound");
        let suffix = match op {
            1 => "intent",
            2 => "undo",
            3 => "verified",
            _ => anyhow::bail!("Unknown appearance phase"),
        };
        let path = st
            .root
            .join("Data")
            .join(format!("ap2-{}.{}", st.id, suffix));
        let pin = match create(&path, b) {
            Ok(p) => Arc::new(p),
            Err(e) => {
                st.poisoned = true;
                return Err(e);
            }
        };
        let seal = Seal {
            pin: pin.clone(),
            id: st.id.clone(),
            session: st.owner.session.clone(),
            operation: op,
            index: st.owner.index,
            before: s.words,
            after: p.after,
            mask: p.mask,
            row,
            pointers,
            metadata,
        };
        st.pins.push(pin);
        st.current = Some(seal.clone());
        Ok(seal)
    }
    pub(crate) fn verified(&self, s: &Snapshot, p: &Plan) -> Result<Seal> {
        self.phase(s, p, 3)
    }
    fn audit(&self, kind: &str, value: Value) -> Result<()> {
        let mut st = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Appearance journal lock poisoned"))?;
        ensure!(
            !st.finished && !st.poisoned && st.audit < 64,
            "Appearance journal audit bound/hold"
        );
        st.marker.verify()?;
        let path = st
            .root
            .join("Data")
            .join(format!("ap2-{}.receipt-{:02}", st.id, st.audit));
        let body =
            serde_json::to_vec(&json!({"id":st.id,"owner":st.owner,"kind":kind,"value":value}))?;
        let pin = match create(&path, body) {
            Ok(p) => Arc::new(p),
            Err(e) => {
                st.poisoned = true;
                return Err(e);
            }
        };
        st.audit += 1;
        st.pins.push(pin);
        Ok(())
    }
    pub(crate) fn ordinary(&self, intent: &writer::Intent) -> Result<()> {
        intent.owner.validate()?;
        ensure!(
            intent.schema == "nba2k27-roster-v2-write-intent-v1"
                && intent.id.len() == 48
                && intent.id.bytes().all(|b| b.is_ascii_hexdigit())
                && intent.result.len() <= 512,
            "Appearance ordinary evidence schema differs"
        );
        let owner = {
            self.0
                .lock()
                .map_err(|_| anyhow::anyhow!("Appearance journal lock poisoned"))?
                .owner
                .clone()
        };
        ensure!(
            intent.owner == owner,
            "Appearance ordinary receipt owner differs"
        );
        self.audit("ordinary-record", serde_json::to_value(intent)?)
    }
    pub(crate) fn held(&self) -> Result<()> {
        self.audit("held", json!({"noReplay":true}))
    }
    fn finish(&self, label: &str) -> Result<()> {
        self.audit(label, json!({"memoryProofByCaller":true}))?;
        let mut st = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Appearance journal lock poisoned"))?;
        ensure!(!st.finished && !st.poisoned, "Appearance completion held");
        st.marker.verify()?;
        loader_files::unlink(&st.marker.file)?;
        st.finished = true;
        Ok(())
    }
    pub(crate) fn complete(&self) -> Result<()> {
        self.finish("committed")
    }
    pub(crate) fn restored(&self) -> Result<()> {
        self.finish("restored")
    }
}
impl super::Journal for Durable {
    fn persist(&mut self, s: &Snapshot, p: &Plan, phase: Phase) -> Result<()> {
        match phase{Phase::Intent=>{self.phase(s,p,1)?;},Phase::Held=>{self.phase(s,p,2)?;},_=>self.audit("appearance-phase",json!({"phase":format!("{phase:?}"),"row":format!("{:x}",Sha256::digest(&s.row)),"before":s.words,"after":p.after}))?,}
        Ok(())
    }
    fn verify(&self, s: &Snapshot, p: &Plan, phase: Phase) -> Result<()> {
        let op = match phase {
            Phase::Intent => 1,
            Phase::Held => 2,
            _ => anyhow::bail!("No native appearance phase"),
        };
        self.current_seal()?.digest_for(&self.id()?, s, p, op)?;
        Ok(())
    }
}
pub(crate) fn pending(root: &Path) -> Result<bool> {
    Ok(root.join("Data").join(MARKER).try_exists()?)
}
/// Closed-game recovery only archives exact immutable marker; never replays RAM.
pub(crate) fn resolve_ended(root: &Path, rows: &[crate::session_safety::Process]) -> Result<()> {
    ensure!(
        !rows
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe")),
        "Close NBA 2K27 before resolving appearance"
    );
    let path = root.join("Data").join(MARKER);
    if !path.try_exists()? {
        return Ok(());
    }
    let _dirs = loader_files::Directories::pin(&root.join("Data"))?;
    let mut f = OpenOptions::new()
        .read(true)
        .access_mode(0x80010000)
        .share_mode(1)
        .custom_flags(0x200000)
        .open(&path)?;
    ordinary(&f)?;
    let id = Stable::read(&f)?;
    ensure!(
        f.metadata()?.len() <= LIMIT as u64,
        "Appearance marker exceeds recovery bound"
    );
    let mut b = Vec::new();
    f.read_to_end(&mut b)?;
    let value: Value = serde_json::from_slice(&b)?;
    ensure!(
        value["schema"] == "appearance-composite-v1",
        "Unknown appearance marker; left untouched"
    );
    let owner: Owner = serde_json::from_value(value["owner"].clone())?;
    owner.validate()?;
    let nonce = value["id"]
        .as_str()
        .context("Appearance recovery ID absent")?;
    ensure!(
        nonce.len() == 48 && nonce.bytes().all(|b| b.is_ascii_hexdigit()),
        "Appearance recovery ID invalid"
    );
    let receipt_path = root.join("Data").join(format!("ap2-{nonce}.game-ended"));
    let receipt =
        serde_json::to_vec(&json!({"owner":owner,"id":nonce,"reason":"game ended; never replay"}))?;
    if let Err(error) = create(&receipt_path, receipt.clone()) {
        ensure!(
            error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::AlreadyExists),
            "Appearance recovery receipt creation failed: {error}"
        );
        // This is closed-game evidence inspection, never live authority/adoption.
        // A prior crash may have flushed this exact terminal receipt before
        // unlinking its still-retained marker. Unknown bytes remain held.
        let mut terminal = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .custom_flags(0x200000)
            .open(&receipt_path)?;
        ordinary(&terminal)?;
        let terminal_id = Stable::read(&terminal)?;
        ensure!(
            terminal.metadata()?.len() == receipt.len() as u64,
            "Foreign appearance recovery receipt remains held"
        );
        let mut actual = vec![0; receipt.len()];
        terminal.read_exact(&mut actual)?;
        ensure!(
            actual == receipt && Stable::read(&terminal)? == terminal_id,
            "Foreign/changed appearance recovery receipt remains held"
        );
    }
    ensure!(
        Stable::read(&f)? == id,
        "Appearance recovery marker replaced"
    );
    loader_files::unlink(&f)
}
