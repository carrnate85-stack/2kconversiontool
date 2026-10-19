//! Append-only, flushed outfit ownership journal. Every mutable-game capability
//! is minted only after its exact intent frame is durable. Held handles deny
//! foreign writes/deletion. Recovery NEVER replays a saved memory address.
#![allow(dead_code)]
use crate::{
    backend,
    blacktop_outfit_match::{Choice, Ledger},
    blacktop_outfits::{self as outfits, Composed, Page, Player, PlayerKey, Source},
    loader_files::{self, Directories},
    session_safety::Identity,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Seek, SeekFrom, Write},
    os::windows::{
        fs::{FileExt, MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};
use windows::Win32::{Foundation::HANDLE, Storage::FileSystem::*};
pub(crate) const MARKER: &str = "blacktop-outfits-session.bin";
const MAGIC: &[u8; 8] = b"2KOUTF01";
const MAX_FRAME: usize = 65536;
const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_EVENTS: usize = 16384;
#[path = "outfit_match_files.rs"]
pub(crate) mod match_files;
#[path = "outfit_match_runtime.rs"]
pub(crate) mod runtime;
fn nonce(s: &str) -> bool {
    s.len() == 48 && s.bytes().all(|b| b.is_ascii_hexdigit())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FileId {
    volume: u32,
    index: u64,
    birth: u64,
}
fn file_id(f: &File) -> Result<FileId> {
    let mut i = BY_HANDLE_FILE_INFORMATION::default();
    unsafe {
        GetFileInformationByHandle(HANDLE(f.as_raw_handle()), &mut i)?;
    }
    Ok(FileId {
        volume: i.dwVolumeSerialNumber,
        index: (i.nFileIndexHigh as u64) << 32 | i.nFileIndexLow as u64,
        birth: (i.ftCreationTime.dwHighDateTime as u64) << 32
            | i.ftCreationTime.dwLowDateTime as u64,
    })
}
fn data_id(root: &Path) -> Result<FileId> {
    let f = fs::OpenOptions::new()
        .read(true)
        .share_mode(3)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(root.join("Data"))?;
    ensure!(
        f.metadata()?.is_dir() && f.metadata()?.file_attributes() & 0x400 == 0,
        "Outfit data folder is not an ordinary directory"
    );
    file_id(&f)
}
fn read_at(f: &File, at: u64, len: usize) -> Result<Vec<u8>> {
    ensure!(len <= MAX_BYTES, "Outfit journal read bound");
    let mut b = vec![0; len];
    let mut read = 0;
    while read < len {
        let end = len.min(read + 1024 * 1024);
        let n = f.seek_read(&mut b[read..end], at + read as u64)?;
        ensure!(n > 0, "Incomplete outfit journal");
        read += n;
    }
    Ok(b)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Header {
    schema: String,
    session: String,
    game: Identity,
    file: FileId,
    data: FileId,
}
impl Header {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == "nba2k27-native-outfits-v1",
            "Unsupported outfit journal schema"
        );
        owner(&self.session, &self.game)
    }
}
fn owner(session: &str, game: &Identity) -> Result<()> {
    ensure!(
        nonce(session)
            && game.pid > 0
            && game.started.parse::<u64>().is_ok_and(|n| n > 0)
            && Path::new(&game.path).is_absolute()
            && Path::new(&game.path)
                .file_name()
                .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("NBA2K27.exe")),
        "Invalid outfit journal owner"
    );
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Kind {
    Compose,
    Existing,
    Restore,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Intent {
    pub id: String,
    pub kind: Kind,
    pub key: PlayerKey,
    pub before: Source,
    pub original_pointer: u64,
    pub name: String,
    pub pool: Option<String>,
    pub selection: Option<crate::outfits::Selection>,
    pub street_index: Option<usize>,
    pub roster_shoes: bool,
    pub items: Vec<String>,
}
impl Intent {
    fn validate(&self) -> Result<()> {
        ensure!(nonce(&self.id), "Invalid outfit operation ID");
        self.key.validate()?;
        outfits::span(self.before.pointer, 16)?;
        outfits::span(self.original_pointer, 16)?;
        if self.before.uniform != 0 {
            outfits::span(self.before.uniform, 8)?;
        }
        ensure!(
            (2..=7).contains(&self.before.count)
                && !self.name.is_empty()
                && self.name.chars().count() <= 80
                && !self.name.chars().any(char::is_control),
            "Invalid outfit source"
        );
        match self.kind {
            Kind::Restore => ensure!(
                self.selection.is_none()
                    && self.street_index.is_none()
                    && !self.roster_shoes
                    && self.items.len() == 2
                    && self.items.iter().all(|n| !n.is_empty()
                        && n.encode_utf16().count() < 256
                        && !n.contains('\0')),
                "Invalid stock outfit restore"
            ),
            _ => {
                let c = crate::outfits::catalog()?;
                let s = self
                    .selection
                    .as_ref()
                    .context("Outfit definition missing")?;
                let index = self.street_index.context("Native top index missing")?;
                let p = c.presets.get(index).context("Native top index changed")?;
                ensure!(
                    p.id == s.street_id
                        && self.pool.is_some()
                        && p.pool == self.pool
                        && self.roster_shoes == (s.shoe.as_deref() == Some("roster"))
                        && c.compose(s)? == self.items
                        && (2..=7).contains(&self.items.len())
                        && self.items.iter().all(|n| outfits::valid_item(n)),
                    "Outfit journal differs from the native catalog"
                );
            }
        }
        Ok(())
    }
    pub fn matches(&self, p: &Player, c: &Composed) -> bool {
        self.key == p.key
            && self.before == p.source
            && self.original_pointer == p.original_pointer
            && self.name == p.name
            && self.pool == p.pool
            && self.selection.as_ref() == Some(&c.selection)
            && self.street_index == Some(c.street_index)
            && self.roster_shoes == c.roster_shoes
            && self.items == c.items
    }
    pub fn target(&self, page: Option<&Page>) -> Result<Source> {
        let pointer = match self.kind {
            Kind::Compose => page
                .context("Prepared outfit page missing")?
                .allocation
                .checked_add(128)
                .context("Outfit page overflow")?,
            Kind::Existing => self.before.pointer,
            Kind::Restore => self.original_pointer,
        };
        Ok(Source {
            pointer,
            count: self.items.len() as u32,
            uniform: self.before.uniform,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "data",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
enum Event {
    Header(Header),
    Reserve(Intent),
    Prepared { id: String, page: Page },
    Attempt { id: String },
    Verified { id: String, choice_id: String },
    Consume { ids: Vec<String> },
    ClearPending,
    Match(match_files::Event),
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Operation {
    intent: Intent,
    page: Option<Page>,
    attempted: bool,
    verified: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct State {
    ops: BTreeMap<String, Operation>,
    pages: Vec<Page>,
    ledger: Ledger,
    reservations: usize,
    events: usize,
    matches: match_files::State,
}
impl State {
    fn apply(&mut self, event: &Event, game: &Identity) -> Result<()> {
        ensure!(
            self.events < MAX_EVENTS,
            "Outfit journal event budget reached; restart the game"
        );
        match event {
            Event::Header(_) => anyhow::bail!("Duplicate outfit journal header"),
            Event::Reserve(i) => {
                i.validate()?;
                ensure!(
                    self.ops.len() < 4096 && !self.ops.contains_key(&i.id),
                    "Duplicate or excessive outfit operation"
                );
                if i.before.pointer != i.original_pointer {
                    ensure!(
                        self.pages.iter().any(|p| p.owner == i.key
                            && p.original_pointer == i.original_pointer
                            && p.allocation.checked_add(128) == Some(i.before.pointer)
                            && p.items.len() == i.before.count as usize
                            && (i.kind != Kind::Existing || p.items == i.items)),
                        "Existing outfit is not owned by this session"
                    );
                } else {
                    ensure!(
                        i.before.count == 2 && (i.kind != Kind::Existing || i.items.len() == 2),
                        "Existing stock outfit shape differs"
                    );
                }
                if i.kind == Kind::Compose {
                    ensure!(
                        self.reservations < outfits::MAX_PAGES,
                        "Custom-outfit page budget reached; restart the game"
                    );
                    self.reservations += 1;
                }
                self.ops.insert(
                    i.id.clone(),
                    Operation {
                        intent: i.clone(),
                        page: None,
                        attempted: false,
                        verified: false,
                    },
                );
            }
            Event::Prepared { id, page } => {
                let op = self.ops.get_mut(id).context("Outfit page has no intent")?;
                let i = &op.intent;
                ensure!(
                    i.kind == Kind::Compose
                        && op.page.is_none()
                        && !op.attempted
                        && page.owner == i.key
                        && page.original_pointer == i.original_pointer
                        && page.street_index == i.street_index.context("Missing top index")?
                        && page.items == i.items
                        && page.roster_shoes == i.roster_shoes
                        && !self.pages.iter().any(|p| p.allocation == page.allocation),
                    "Prepared outfit page differs from its reserved intent"
                );
                let bytes = outfits::page_bytes(
                    game,
                    &page.owner,
                    page.original_pointer,
                    page.allocation,
                    page.street_index,
                    page.roster_shoes,
                    &page.items,
                )?;
                ensure!(
                    loader_files::digest(&bytes) == page.sha256,
                    "Prepared outfit page hash differs"
                );
                op.page = Some(page.clone());
                self.pages.push(page.clone());
            }
            Event::Attempt { id } => {
                let op = self
                    .ops
                    .get_mut(id)
                    .context("Outfit attempt has no intent")?;
                ensure!(
                    !op.attempted
                        && !op.verified
                        && op.intent.kind != Kind::Existing
                        && (op.intent.kind != Kind::Compose || op.page.is_some()),
                    "Outfit source attempt cannot be repeated"
                );
                op.attempted = true;
            }
            Event::Verified { id, choice_id } => {
                ensure!(nonce(choice_id), "Invalid verified outfit choice ID");
                let op = self
                    .ops
                    .get_mut(id)
                    .context("Outfit confirmation has no intent")?;
                ensure!(
                    !op.verified && (op.attempted || op.intent.kind == Kind::Existing),
                    "Outfit was not submitted or was already confirmed"
                );
                let i = &op.intent;
                if i.kind == Kind::Restore || i.items.len() < 3 {
                    self.ledger.restore(i.key.roster, i.key.slot);
                } else {
                    self.ledger.record(Choice {
                        choice_id: choice_id.clone(),
                        roster: i.key.roster,
                        slot: i.key.slot,
                        name: i.name.clone(),
                        items: i.items.clone(),
                    })?;
                }
                op.verified = true;
            }
            Event::Consume { ids } => {
                ensure!(
                    !ids.is_empty() && ids.len() <= 10,
                    "Invalid frozen outfit plan count"
                );
                let frozen = ids
                    .iter()
                    .map(|id| {
                        self.ledger
                            .choices
                            .iter()
                            .find(|c| &c.choice_id == id)
                            .cloned()
                            .context("Consumed outfit choice is stale")
                    })
                    .collect::<Result<Vec<_>>>()?;
                self.ledger.consume(&frozen, |_| Ok(()))?;
            }
            Event::ClearPending => self.ledger.leave_blacktop(),
            Event::Match(event) => self.matches.apply(event, &mut self.ledger, game)?,
        }
        self.ledger.validate()?;
        self.events += 1;
        Ok(())
    }
}
fn frame(e: &Event) -> Result<Vec<u8>> {
    let b = serde_json::to_vec(e)?;
    ensure!(
        !b.is_empty() && b.len() <= MAX_FRAME,
        "Outfit frame exceeds bounds"
    );
    Ok([
        (b.len() as u32).to_le_bytes().as_slice(),
        crc32fast::hash(&b).to_le_bytes().as_slice(),
        &b,
    ]
    .concat())
}
fn parse(bytes: &[u8]) -> Result<(Header, State)> {
    ensure!(
        bytes.len() >= 16 && bytes.len() <= MAX_BYTES && bytes.get(..8) == Some(MAGIC),
        "Unsupported or oversized outfit journal"
    );
    let mut at = 8;
    let mut header: Option<Header> = None;
    let mut state = State::default();
    while at < bytes.len() {
        ensure!(at + 8 <= bytes.len(), "Torn outfit journal frame header");
        let n = u32::from_le_bytes(bytes[at..at + 4].try_into()?) as usize;
        let crc = u32::from_le_bytes(bytes[at + 4..at + 8].try_into()?);
        at += 8;
        ensure!(
            n > 0 && n <= MAX_FRAME && n <= bytes.len() - at,
            "Torn or excessive outfit journal frame"
        );
        let body = &bytes[at..at + n];
        ensure!(
            crc32fast::hash(body) == crc,
            "Outfit journal frame CRC differs"
        );
        let e: Event = serde_json::from_slice(body)?;
        at += n;
        if let Some(h) = &header {
            state.apply(&e, &h.game)?;
        } else {
            let Event::Header(h) = e else {
                anyhow::bail!("Outfit journal has no header")
            };
            h.validate()?;
            header = Some(h);
        }
    }
    Ok((header.context("Outfit journal header is missing")?, state))
}
struct Shared {
    file: Mutex<File>,
    header: Header,
    path: PathBuf,
    _pins: Vec<Directories>,
}
impl Shared {
    fn read(&self) -> Result<(Header, State)> {
        let f = self
            .file
            .lock()
            .map_err(|_| anyhow::anyhow!("Outfit journal lock failed"))?;
        self.verify_file(&f)?;
        parse(&read_at(&f, 0, f.metadata()?.len() as usize)?)
    }
    fn verify_file(&self, f: &File) -> Result<()> {
        ensure!(
            file_id(f)? == self.header.file
                && f.metadata()?.is_file()
                && f.metadata()?.file_attributes() & 0x400 == 0,
            "Outfit journal file ownership changed"
        );
        Ok(())
    }
}
/// Non-cloneable proof of a committed frame. Fields are never renderer input.
pub(crate) struct Witness {
    shared: Arc<Shared>,
    offset: u64,
    bytes: Vec<u8>,
}
pub(crate) struct Pin<'a> {
    _guard: MutexGuard<'a, File>,
}
impl Witness {
    pub fn game(&self) -> &Identity {
        &self.shared.header.game
    }
    pub fn pin(&self) -> Result<Pin<'_>> {
        let f = self
            .shared
            .file
            .lock()
            .map_err(|_| anyhow::anyhow!("Outfit journal lock failed"))?;
        self.shared.verify_file(&f)?;
        ensure!(
            read_at(&f, self.offset, self.bytes.len())? == self.bytes,
            "Durable outfit capability frame changed"
        );
        Ok(Pin { _guard: f })
    }
}
pub(crate) struct Reservation {
    witness: Witness,
    intent: Intent,
}
impl Reservation {
    pub fn intent(&self) -> &Intent {
        &self.intent
    }
    pub fn witness(&self) -> &Witness {
        &self.witness
    }
    pub fn prepared(self, page: Page) -> Result<Prepared> {
        ensure!(
            self.intent.kind == Kind::Compose,
            "Only composed outfits allocate a page"
        );
        Ok(Prepared {
            reservation: self,
            page,
        })
    }
}
pub(crate) struct Prepared {
    reservation: Reservation,
    page: Page,
}
pub(crate) struct Ready {
    witness: Witness,
    intent: Intent,
    page: Option<Page>,
}
impl Ready {
    pub fn intent(&self) -> &Intent {
        &self.intent
    }
    pub fn page(&self) -> Option<&Page> {
        self.page.as_ref()
    }
    pub fn witness(&self) -> &Witness {
        &self.witness
    }
}
/// Single-use source-publication capability; the Attempt frame is already
/// durable. Failed/uncertain writes cannot reconstruct it by replaying the file.
pub(crate) struct Submission {
    witness: Witness,
    intent: Intent,
    page: Option<Page>,
}
impl Submission {
    pub fn intent(&self) -> &Intent {
        &self.intent
    }
    pub fn page(&self) -> Option<&Page> {
        self.page.as_ref()
    }
    pub fn witness(&self) -> &Witness {
        &self.witness
    }
}
pub(crate) struct Store {
    shared: Option<Arc<Shared>>,
    state: State,
    end: u64,
    halted: Option<String>,
    // At most one uncertain trigger. Its Drop keeps the exclusive handle alive.
    held_trigger: Option<match_files::LockedTrigger>,
    #[cfg(test)]
    fail_append_after: Option<usize>,
    #[cfg(test)]
    fail_append_skip: usize,
}
impl Store {
    pub fn create(root: &Path, session: &str, game: &Identity) -> Result<Self> {
        owner(session, game)?;
        let pins = vec![
            Directories::pin(root)?,
            Directories::pin(&root.join("Data"))?,
        ];
        let path = root.join("Data").join(MARKER);
        let mut file=fs::OpenOptions::new().read(true).write(true).access_mode(0xc0010000).share_mode(1).create_new(true).custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0).open(&path).context("An outfit journal is already pending or unavailable; never adopt it into a live game")?;
        let header = Header {
            schema: "nba2k27-native-outfits-v1".into(),
            session: session.into(),
            game: game.clone(),
            file: file_id(&file)?,
            data: data_id(root)?,
        };
        header.validate()?;
        let bytes = [MAGIC.as_slice(), &frame(&Event::Header(header.clone()))?].concat();
        let result = (|| -> Result<()> {
            file.write_all(&bytes)?;
            file.sync_all()?;
            ensure!(
                read_at(&file, 0, bytes.len())? == bytes,
                "Outfit header readback differs"
            );
            Ok(())
        })();
        if result.is_err() {
            let _ = loader_files::unlink(&file);
        }
        result?;
        Ok(Self {
            shared: Some(Arc::new(Shared {
                file: Mutex::new(file),
                header,
                path,
                _pins: pins,
            })),
            state: State::default(),
            end: bytes.len() as u64,
            halted: None,
            held_trigger: None,
            #[cfg(test)]
            fail_append_after: None,
            #[cfg(test)]
            fail_append_skip: 0,
        })
    }
    pub fn pages(&self) -> &[Page] {
        &self.state.pages
    }
    pub fn ledger(&self) -> &Ledger {
        &self.state.ledger
    }
    pub fn halted(&self) -> Option<&str> {
        self.halted.as_deref()
    }
    fn append(&mut self, event: Event) -> Result<Witness> {
        ensure!(
            self.halted.is_none(),
            "Outfit journal is held after an uncertain commit"
        );
        let shared = self.shared.as_ref().context("Outfit journal ended")?;
        let mut next = self.state.clone();
        next.apply(&event, &shared.header.game)?;
        let bytes = frame(&event)?;
        ensure!(
            self.end + (bytes.len() as u64) <= MAX_BYTES as u64,
            "Outfit journal byte budget reached; restart the game"
        );
        let result = (|| -> Result<()> {
            let mut f = shared
                .file
                .lock()
                .map_err(|_| anyhow::anyhow!("Outfit journal lock failed"))?;
            shared.verify_file(&f)?;
            ensure!(
                f.metadata()?.len() == self.end,
                "Outfit journal length changed"
            );
            f.seek(SeekFrom::End(0))?;
            #[cfg(test)]
            {
                if self.fail_append_skip > 0 {
                    self.fail_append_skip -= 1;
                } else if let Some(n) = self.fail_append_after.take() {
                    f.write_all(&bytes[..n.min(bytes.len())])?;
                    f.sync_all()?;
                    anyhow::bail!("Injected interrupted outfit commit");
                }
            }
            f.write_all(&bytes)?;
            f.sync_all()?;
            ensure!(
                read_at(&f, self.end, bytes.len())? == bytes,
                "Outfit commit readback differs"
            );
            Ok(())
        })();
        if let Err(e) = result {
            self.halted = Some(format!("{e:#}"));
            return Err(e);
        }
        let witness = Witness {
            shared: shared.clone(),
            offset: self.end,
            bytes,
        };
        self.end += witness.bytes.len() as u64;
        self.state = next;
        Ok(witness)
    }
    pub fn reserve(&mut self, p: &Player, c: &Composed) -> Result<Reservation> {
        let kind = if p.items == c.items {
            Kind::Existing
        } else {
            Kind::Compose
        };
        let intent = Intent {
            id: backend::unique_id(),
            kind,
            key: p.key.clone(),
            before: p.source.clone(),
            original_pointer: p.original_pointer,
            name: p.name.clone(),
            pool: p.pool.clone(),
            selection: Some(c.selection.clone()),
            street_index: Some(c.street_index),
            roster_shoes: c.roster_shoes,
            items: c.items.clone(),
        };
        let witness = self.append(Event::Reserve(intent.clone()))?;
        Ok(Reservation { witness, intent })
    }
    pub fn reserve_restore(&mut self, p: &Player) -> Result<Ready> {
        let stock = p
            .stock
            .iter()
            .find(|s| s.pointer == p.original_pointer)
            .context("Original stock outfit is not verified")?;
        let intent = Intent {
            id: backend::unique_id(),
            kind: Kind::Restore,
            key: p.key.clone(),
            before: p.source.clone(),
            original_pointer: p.original_pointer,
            name: p.name.clone(),
            pool: p.pool.clone(),
            selection: None,
            street_index: None,
            roster_shoes: false,
            items: stock.names.to_vec(),
        };
        let witness = self.append(Event::Reserve(intent.clone()))?;
        Ok(Ready {
            witness,
            intent,
            page: None,
        })
    }
    pub fn prepared(&mut self, p: Prepared) -> Result<Ready> {
        ensure!(
            Arc::ptr_eq(
                self.shared.as_ref().context("Outfit journal ended")?,
                &p.reservation.witness.shared
            ),
            "Outfit reservation belongs to another journal"
        );
        let intent = p.reservation.intent.clone();
        let page = p.page;
        let witness = self.append(Event::Prepared {
            id: intent.id.clone(),
            page: page.clone(),
        })?;
        Ok(Ready {
            witness,
            intent,
            page: Some(page),
        })
    }
    pub fn existing(&self, r: Reservation) -> Result<Ready> {
        ensure!(
            r.intent.kind == Kind::Existing
                && Arc::ptr_eq(
                    self.shared.as_ref().context("Outfit journal ended")?,
                    &r.witness.shared
                ),
            "Invalid unchanged outfit capability"
        );
        Ok(Ready {
            witness: r.witness,
            intent: r.intent,
            page: None,
        })
    }
    pub fn attempting(&mut self, r: Ready) -> Result<Submission> {
        ensure!(
            Arc::ptr_eq(
                self.shared.as_ref().context("Outfit journal ended")?,
                &r.witness.shared
            ),
            "Outfit source capability belongs to another journal"
        );
        let witness = self.append(Event::Attempt {
            id: r.intent.id.clone(),
        })?;
        Ok(Submission {
            witness,
            intent: r.intent,
            page: r.page,
        })
    }
    pub fn verified(&mut self, id: &str) -> Result<()> {
        self.append(Event::Verified {
            id: id.into(),
            choice_id: backend::unique_id(),
        })?;
        Ok(())
    }
    pub fn consume(&mut self, frozen: &[Choice]) -> Result<()> {
        ensure!(
            frozen.iter().all(|c| self.state.ledger.choices.contains(c)),
            "Frozen match outfits differ from the verified choices"
        );
        self.append(Event::Consume {
            ids: frozen.iter().map(|c| c.choice_id.clone()).collect(),
        })?;
        Ok(())
    }
    pub fn clear_pending(&mut self) -> Result<()> {
        if !self.state.ledger.choices.is_empty() {
            self.append(Event::ClearPending)?;
        }
        Ok(())
    }
    pub fn finish(&mut self, closed: impl Fn() -> Result<()>) -> Result<()> {
        closed()?;
        // Only a positive game-closed proof may release a quarantined lock.
        // Early Store Drop instead retains its kernel handle until process exit.
        if let Some(trigger) = self.held_trigger.take() {
            trigger.release_after_game_closed();
        }
        let shared = self
            .shared
            .as_ref()
            .context("Outfit journal already ended")?;
        ensure!(
            Arc::strong_count(shared) == 1,
            "Outfit operation capabilities are still active; retain the receipt"
        );
        let (h, state) = shared.read()?;
        ensure!(
            h == shared.header && state == self.state,
            "Outfit journal content changed; left untouched"
        );
        closed()?;
        match_files::cleanup(&shared.path, &shared.header, &self.state.matches, &closed)?;
        closed()?;
        let shared = Arc::try_unwrap(self.shared.take().unwrap())
            .map_err(|_| anyhow::anyhow!("Outfit journal still referenced"))?;
        let file = shared
            .file
            .into_inner()
            .map_err(|_| anyhow::anyhow!("Outfit journal lock failed"))?;
        loader_files::unlink(&file)?;
        drop(file);
        ensure!(
            !shared.path.try_exists()?,
            "Outfit receipt deletion is still pending"
        );
        Ok(())
    }
}
pub(crate) fn pending(root: &Path) -> Result<bool> {
    Ok(fs::symlink_metadata(root.join("Data").join(MARKER))
        .map(|_| true)
        .or_else(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Ok(false)
            } else {
                Err(e)
            }
        })?)
}
pub(crate) fn resolve_ended(
    root: &Path,
    session: &str,
    game: &Identity,
    closed: impl Fn() -> Result<()>,
) -> Result<()> {
    closed()?;
    let _pins = [
        Directories::pin(root)?,
        Directories::pin(&root.join("Data"))?,
    ];
    let path = root.join("Data").join(MARKER);
    if !pending(root)? {
        return Ok(());
    }
    let f = fs::OpenOptions::new()
        .read(true)
        .access_mode(0x80010000)
        .share_mode(1)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(&path)?;
    ensure!(
        f.metadata()?.is_file()
            && f.metadata()?.file_attributes() & 0x400 == 0
            && f.metadata()?.len() <= MAX_BYTES as u64,
        "Outfit recovery file is unsupported"
    );
    let (h, state) = parse(&read_at(&f, 0, f.metadata()?.len() as usize)?)?;
    ensure!(
        h.session == session
            && h.game == *game
            && h.file == file_id(&f)?
            && h.data == data_id(root)?,
        "Outfit recovery is not owned by this exact session/file/folder; left untouched"
    );
    closed()?;
    match_files::cleanup(&path, &h, &state.matches, &closed)?;
    closed()?;
    loader_files::unlink(&f)?;
    drop(f);
    ensure!(!path.try_exists()?, "Outfit recovery deletion is pending");
    Ok(())
}
#[cfg(test)]
#[path = "outfit_journal_tests.rs"]
mod tests;
