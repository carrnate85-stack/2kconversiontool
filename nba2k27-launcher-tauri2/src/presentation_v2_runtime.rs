//! Provider-owned Blacktop presentation session. Frozen next-launch
//! choices; no game writes outside verified selection/inactive exit menus.
#![allow(dead_code)]
use crate::{
    blacktop_native::ContextState,
    presentation_v2 as policy, rules_v2,
    session_safety::{self, Identity},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

const SETTLING: &str = "Blacktop selection is still settling; no presentation write attempted";
pub(crate) const MARKER: &str = "blacktop-presentation-v2-session.jsonl";
const LIMIT: u64 = 4 * 1024 * 1024;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Options {
    pub remove_drone: bool,
    pub remove_dj: bool,
    pub remove_mc: bool,
    pub remove_crowd: bool,
}
impl Options {
    pub fn enabled(self) -> bool {
        self.mask() != 0 || self.remove_crowd
    }
    fn mask(self) -> u8 {
        self.remove_drone as u8 | ((self.remove_dj as u8) << 1) | ((self.remove_mc as u8) << 2)
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Binding {
    game: Identity,
    parent: Identity,
    game_root: PathBuf,
    portable: PathBuf,
    session: String,
}
impl Binding {
    pub fn from_launched(
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
                && session.bytes().all(|v| v.is_ascii_hexdigit()),
            "Invalid owned presentation binding"
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Boundary {
    pub base: u64,
    pub context: ContextState,
}
impl Boundary {
    fn name(&self) -> &'static str {
        rules_v2::boundary(&self.context, self.base)
    }
}
#[derive(Clone)]
struct Permit {
    before: policy::Snapshot,
    after: policy::Snapshot,
    change: policy::Change,
}
impl Permit {
    fn valid(&self) -> Result<()> {
        let s = &self.before;
        let c = &self.change;
        ensure!(
            c.before <= 1 && c.after <= 1 && c.before != c.after,
            "Invalid presentation boolean change"
        );
        let mut expected = s.clone();
        match c.kind {
            policy::Kind::Drone | policy::Kind::Dj | policy::Kind::Mc | policy::Kind::McCoupled => {
                let index = match c.kind {
                    policy::Kind::Drone => 6,
                    policy::Kind::Dj => 7,
                    policy::Kind::Mc => 8,
                    _ => 11,
                };
                ensure!(
                    rules_v2::boundary(&s.context, s.base) == "selection"
                        && c.after == 1
                        && c.at == s.owner + 0x2C48 + index as u64
                        && s.window.get(index) == Some(&c.before),
                    "Actor permit escaped its fixed selection field"
                );
                expected.window[index] = c.after;
            }
            policy::Kind::CrowdGlobal => {
                ensure!(
                    matches!(
                        rules_v2::boundary(&s.context, s.base),
                        "selection" | "main-menu" | "quick-play"
                    ) && c.at == s.owner + 0x2C49
                        && s.window.get(1) == Some(&c.before),
                    "Crowd permit escaped its fixed field"
                );
                expected.window[1] = c.after;
            }
            policy::Kind::CrowdStanding(index) => {
                ensure!(
                    (175..=179).contains(&index)
                        && matches!(
                            rules_v2::boundary(&s.context, s.base),
                            "selection" | "main-menu" | "quick-play"
                        ),
                    "Crowd preset escaped its range/boundary"
                );
                let row = expected
                    .sources
                    .get_mut((index - 175) as usize)
                    .context("Missing crowd preset")?;
                ensure!(
                    row.index == index
                        && row.address == s.preload_table + index as u64 * 0xC810 + 8
                        && c.at == row.address + 0x7430
                        && row.front.get(8) == Some(&c.before),
                    "Crowd permit escaped its exact preset byte"
                );
                row.front[8] = c.after;
            }
        }
        ensure!(
            self.after == expected,
            "Presentation permit changes unmapped bytes/context"
        );
        Ok(())
    }
}
trait Access {
    fn boundary(&self) -> Result<Boundary>;
    fn capture(&self, crowd: bool) -> Result<policy::Snapshot>;
    fn publish(
        &self,
        permit: &Permit,
        parent_required: bool,
        durable: &dyn Fn() -> Result<()>,
    ) -> Result<()>;
}
#[derive(Clone, Debug)]
struct Proof {
    offset: u64,
    bytes: Vec<u8>,
}
trait Log {
    fn intent(&mut self, operation: &str, transaction: &policy::Transaction) -> Result<Proof>;
    fn verify(&self, proof: &Proof) -> Result<()>;
    fn note(&mut self, event: &str, operation: &str, message: &str) -> Result<()>;
}
fn snapshot_value(s: &policy::Snapshot) -> Value {
    let c = &s.context;
    json!({"base":s.base,"owner":s.owner,"preloadTable":s.preload_table,"window":s.window,
        "context":{"currentIndex":c.current_index,"alternateIndex":c.alternate_index,"current":c.current,"alternate":c.alternate,
            "stage":c.stage,"active":c.active,"owner":c.owner,"kind":c.kind,"gameMode":c.game_mode,"rootTag":c.root_tag},
        "sources":s.sources.iter().map(|r|json!({"index":r.index,"address":r.address,"front":r.front,"ambient":r.ambient})).collect::<Vec<_>>()})
}
fn apply_to(s: &mut policy::Snapshot, c: &policy::Change) -> Result<()> {
    match c.kind {
        policy::Kind::Drone => s.window[6] = c.after,
        policy::Kind::Dj => s.window[7] = c.after,
        policy::Kind::Mc => s.window[8] = c.after,
        policy::Kind::McCoupled => s.window[11] = c.after,
        policy::Kind::CrowdGlobal => s.window[1] = c.after,
        policy::Kind::CrowdStanding(index) => {
            s.sources
                .get_mut((index - 175) as usize)
                .context("Missing crowd source")?
                .front[8] = c.after
        }
    }
    Ok(())
}
/// Ok(false): the snapshot moved before the durable intent, so nothing was
/// journaled or written. The caller waits for the next stable sample instead
/// of holding the whole game session (WPF re-read and retried the same way).
fn execute(
    access: &impl Access,
    log: &mut impl Log,
    name: &str,
    tx: &policy::Transaction,
    parent_required: bool,
) -> Result<bool> {
    ensure!(
        tx.changes.len() <= 10,
        "Presentation operation exceeds fixed field count"
    );
    if tx.changes.is_empty() {
        return Ok(true);
    }
    if !access
        .capture(!tx.before.sources.is_empty())
        .is_ok_and(|current| current == tx.before)
    {
        return Ok(false);
    }
    let proof = log.intent(name, tx)?;
    let mut expected = tx.before.clone();
    for change in &tx.changes {
        let mut after = expected.clone();
        apply_to(&mut after, change)?;
        let permit = Permit {
            before: expected.clone(),
            after: after.clone(),
            change: change.clone(),
        };
        permit.valid()?;
        // On uncertainty, leave the journal and already-applied owned fields
        // held. Only the audited inactive-menu partial-crowd restore may run
        // later; never manufacture a rollback in a loading/match scene.
        log.verify(&proof)?;
        access.publish(&permit, parent_required, &|| log.verify(&proof))?;
        ensure!(
            access.capture(!after.sources.is_empty())? == after,
            "Presentation read-back/unchanged-neighbor check failed"
        );
        expected = after;
    }
    ensure!(
        expected == tx.after,
        "Presentation plan does not match the exact field sequence"
    );
    log.note(
        "applied",
        name,
        "All fixed bytes read back; context and neighbors unchanged",
    )?;
    Ok(true)
}
struct Engine {
    options: Options,
    crowd: Option<policy::Transaction>,
    boundary: Option<Boundary>,
    samples: u8,
    actor_phase: String,
    crowd_phase: String,
    message: String,
    held: bool,
    visits: u32,
    prepared_selection: bool,
    actor_retries: u8,
    cleanup_allowed: bool,
    // An inactive-menu cleanup can race the game's immediate transition back
    // into Blacktop.  No restoration is replayed through that transition.
    // A later stable selection must prove the same owner, preset table and
    // every unowned byte before the normal per-visit preparation may resume.
    restore_rearm_pending: bool,
}
impl Engine {
    fn new(options: Options) -> Self {
        Self {
            options,
            crowd: None,
            boundary: None,
            samples: 0,
            actor_phase: if options.mask() != 0 {
                "waiting"
            } else {
                "disabled"
            }
            .into(),
            crowd_phase: if options.remove_crowd {
                "waiting"
            } else {
                "disabled"
            }
            .into(),
            message: "Waiting for stable Blacktop selection".into(),
            held: false,
            visits: 0,
            prepared_selection: false,
            actor_retries: 0,
            cleanup_allowed: false,
            restore_rearm_pending: false,
        }
    }
    fn status(&self) -> Value {
        json!({"version2":true,"actors":self.actor_phase,"crowd":self.crowd_phase,
        "message":self.message,"held":self.held,"preparedVisits":self.visits,"gameplayVerified":false})
    }
    fn tick(&mut self, access: &impl Access, log: &mut impl Log, parent_alive: bool) -> Result<()> {
        if !self.options.enabled() || (self.held && (!self.cleanup_allowed || self.crowd.is_none()))
        {
            return Ok(());
        }
        let b = access.boundary()?;
        let name = b.name();
        if self.held && name == "selection" {
            return Ok(());
        }
        if name != "selection" {
            self.prepared_selection = false;
        }
        if !matches!(name, "selection" | "main-menu" | "quick-play") {
            self.boundary = None;
            self.samples = 0;
            return Ok(());
        }
        if self.boundary.as_ref() == Some(&b) {
            self.samples = self.samples.saturating_add(1);
        } else {
            self.boundary = Some(b);
            self.samples = 1;
        }
        if self.samples < 2 {
            return Ok(());
        }
        let result = self.ready(access, log, parent_alive, name);
        if let Err(e) = &result {
            self.held = true;
            if name != "selection" {
                self.cleanup_allowed = false;
            }
            self.message = format!(
                "Presentation preparation held; only proven inactive-menu crowd cleanup is permitted: {e:#}"
            );
            let _ = log.note("held", "presentation", &self.message);
        }
        result
    }
    fn ready(
        &mut self,
        access: &impl Access,
        log: &mut impl Log,
        parent_alive: bool,
        name: &str,
    ) -> Result<()> {
        if name == "selection" {
            if self.prepared_selection {
                return Ok(());
            }
            if !parent_alive {
                self.message = "Launcher closed; no new presentation preparation".into();
                return Ok(());
            }
            if self.restore_rearm_pending {
                let current = match access.capture(true) {
                    Ok(s) => s,
                    Err(e) => {
                        self.message = format!(
                            "Waiting to prove presentation ownership after the menu transition: {e:#}"
                        );
                        return Ok(());
                    }
                };
                let owned = self
                    .crowd
                    .as_ref()
                    .context("Missing crowd ownership after interrupted restoration")?;
                ensure!(
                    same_crowd_owned(&current, owned),
                    "Presentation ownership changed after interrupted menu restoration"
                );
            }
            if self.options.mask() != 0 {
                let before = match access.capture(false) {
                    Ok(s) => s,
                    Err(e) => {
                        self.message = format!("Waiting for presentation settings: {e:#}");
                        return Ok(());
                    }
                };
                let tx = policy::presentation(&before, self.options.mask())?;
                match execute(access, log, "actors-prepare", &tx, true) {
                    Ok(true) => {}
                    Ok(false) => {
                        self.message = SETTLING.into();
                        return Ok(());
                    }
                    // Drone/DJ/MC are session-long booleans the game itself
                    // clears at match end (no restore exists). A write cut short
                    // after its intent is re-planned from the live window, but
                    // only when a fresh capture proves the owner, context and
                    // every unselected byte unchanged. Bounded per game.
                    Err(e) => {
                        let owned = access
                            .capture(false)
                            .is_ok_and(|now| actors_partial(&before, &now, self.options.mask()));
                        if !owned || self.actor_retries >= ACTOR_RETRIES {
                            return Err(e);
                        }
                        self.actor_retries += 1;
                        self.message = format!(
                            "Drone/DJ/MC write interrupted; owned bytes verified, retrying at selection: {e:#}"
                        );
                        let _ = log.note("retry", "actors-prepare", &self.message);
                        return Ok(());
                    }
                }
                self.actor_phase = "active".into();
            }
            if self.options.remove_crowd {
                let current = match access.capture(true) {
                    Ok(s) => s,
                    Err(e) => {
                        self.message = format!("Waiting for Blacktop crowd presets: {e:#}");
                        return Ok(());
                    }
                };
                if let Some(owned) = &self.crowd {
                    ensure!(
                        same_crowd_owned(&current, owned),
                        "Crowd owners or unselected bytes changed; old visit held"
                    );
                    // Reassert the same owned choice if the game reinitialized
                    // its selected booleans between matches without an exit.
                    let tx = policy::crowd(&current)?;
                    if !execute(access, log, "crowd-rearm", &tx, true)? {
                        self.message = SETTLING.into();
                        return Ok(());
                    }
                } else {
                    let tx = policy::crowd(&current)?;
                    // Retain original ownership BEFORE the first possible
                    // effect, so a partial transaction can be safely restored
                    // at an inactive exit menu using restore_crowd's proof.
                    self.crowd = Some(tx.clone());
                    self.cleanup_allowed = true;
                    if !execute(access, log, "crowd-prepare", &tx, true)? {
                        // Nothing journaled or written: no ownership to keep.
                        self.crowd = None;
                        self.cleanup_allowed = false;
                        self.message = SETTLING.into();
                        return Ok(());
                    }
                    self.visits = self.visits.saturating_add(1);
                }
                self.crowd_phase = "active".into();
            }
            self.message =
                "Presentation is prepared for Blacktop; all five crowd sizes are covered".into();
            self.prepared_selection = true;
            self.restore_rearm_pending = false;
        } else if let Some(owned) = self.crowd.as_ref() {
            let current = match access.capture(true) {
                Ok(s) => s,
                Err(e) => {
                    self.message = format!("Waiting for safe crowd restoration: {e:#}");
                    return Ok(());
                }
            };
            let restore = policy::restore_crowd(&current, owned)?;
            match execute(access, log, "crowd-restore", &restore, false) {
                Ok(true) => {}
                Ok(false) => {
                    self.message = "Waiting for a stable inactive menu to restore the crowd".into();
                    return Ok(());
                }
                Err(e) => {
                    // Ordinary journal/write/ownership failures at the same
                    // stable boundary remain hard holds.  This recovery is
                    // only for the observed case where the game departed the
                    // exact sampled inactive menu while the restore ran.
                    let departed = !access.boundary().is_ok_and(|current| {
                        current.name() == name && self.boundary.as_ref() == Some(&current)
                    });
                    if !departed {
                        return Err(e);
                    }
                    // The game can expose Quick Play only long enough for the
                    // durable intent and part of the owned restore, then enter
                    // the next Blacktop visit.  Do not replay the restore in a
                    // changing/loading context and do not strand the session.
                    // The next selection gets a complete fresh ownership
                    // proof above before any actor or crowd write is allowed.
                    self.restore_rearm_pending = true;
                    self.message = format!(
                        "Crowd restoration was interrupted by the menu transition; waiting for a stable Blacktop selection to verify and reapply: {e:#}"
                    );
                    let _ = log.note("rearm-pending", "crowd-restore", &self.message);
                    return Ok(());
                }
            }
            self.crowd = None;
            self.cleanup_allowed = false;
            self.restore_rearm_pending = false;
            self.crowd_phase = if self.held {
                "restored-held"
            } else {
                "waiting"
            }
            .into();
            self.message = if self.held {
                "Owned crowd restored; preparation remains held for this game"
            } else {
                "Crowd restored at inactive menu; ready for the next Blacktop visit"
            }
            .into();
        }
        Ok(())
    }
}
const ACTOR_RETRIES: u8 = 3;
/// Only the selected actor bytes may have moved, and only to 1.
fn actors_partial(before: &policy::Snapshot, now: &policy::Snapshot, mask: u8) -> bool {
    let selected = |i: usize| match i {
        6 => mask & 1 != 0,
        7 => mask & 2 != 0,
        8 | 11 => mask & 4 != 0,
        _ => false,
    };
    now.base == before.base
        && now.owner == before.owner
        && now.context == before.context
        && now.preload_table == before.preload_table
        && now.sources == before.sources
        && now.window.len() == 16
        && before.window.len() == 16
        && now
            .window
            .iter()
            .zip(&before.window)
            .enumerate()
            .all(|(i, (a, b))| a == b || (selected(i) && *a == 1))
}
fn same_crowd_owned(current: &policy::Snapshot, owned: &policy::Transaction) -> bool {
    let first = &owned.before;
    current.base == first.base
        && current.owner == first.owner
        && current.preload_table == first.preload_table
        && policy::crowd_window_neighbors(&first.window, &current.window)
        && (current.window[1] == 0 || current.window[1] == first.window[1])
        && current.sources.len() == 5
        && first.sources.len() == 5
        && current.sources.iter().zip(&first.sources).all(|(a, b)| {
            a.index == b.index
                && a.address == b.address
                && a.ambient == b.ambient
                && a.front.len() == 24
                && b.front.len() == 24
                && a.front
                    .iter()
                    .zip(&b.front)
                    .enumerate()
                    .all(|(i, (a, b))| if i == 8 { *a == 0 || a == b } else { a == b })
        })
}

struct Journal {
    file: File,
    size: u64,
    _pins: Vec<crate::loader_files::Directories>,
    identity: crate::file_identity::Stable,
}
impl Journal {
    fn create(binding: &Binding, options: Options) -> Result<Self> {
        let pins = vec![
            crate::loader_files::Directories::pin(&binding.portable)?,
            crate::loader_files::Directories::pin(&binding.portable.join("Data"))?,
        ];
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .share_mode(1)
            .custom_flags(0x00200000)
            .open(binding.portable.join("Data").join(MARKER))?;
        let identity = crate::file_identity::Stable::read(&file)?;
        let mut journal = Self {
            file,
            size: 0,
            _pins: pins,
            identity,
        };
        journal.append(json!({"event":"header","schema":"nba2k27-presentation-v2-session-v1","game":binding.game,
            "parent":binding.parent,"session":binding.session,"options":options}))?;
        Ok(journal)
    }
    fn append(&mut self, value: Value) -> Result<Proof> {
        let mut bytes = serde_json::to_vec(&value)?;
        bytes.push(b'\n');
        ensure!(
            bytes.len() <= 64 * 1024
                && self
                    .size
                    .checked_add(bytes.len() as u64)
                    .is_some_and(|v| v <= LIMIT),
            "Presentation journal limit reached; writes held"
        );
        ensure!(
            self.file.metadata()?.len() == self.size
                && crate::file_identity::Stable::read(&self.file)? == self.identity,
            "Presentation journal identity/length changed"
        );
        let proof = Proof {
            offset: self.size,
            bytes,
        };
        self.file.seek(SeekFrom::Start(self.size))?;
        self.file.write_all(&proof.bytes)?;
        self.file.sync_all()?;
        self.size += proof.bytes.len() as u64;
        self.verify(&proof)?;
        Ok(proof)
    }
}
impl Log for Journal {
    fn intent(&mut self, operation: &str, tx: &policy::Transaction) -> Result<Proof> {
        ensure!(
            [
                "actors-prepare",
                "crowd-prepare",
                "crowd-rearm",
                "crowd-restore"
            ]
            .contains(&operation),
            "Unknown presentation operation"
        );
        self.append(json!({"event":"intent","operation":operation,"before":snapshot_value(&tx.before),"after":snapshot_value(&tx.after),
            "changes":tx.changes.iter().map(|c|json!({"at":c.at,"before":c.before,"after":c.after,"kind":format!("{:?}",c.kind)})).collect::<Vec<_>>()}))
    }
    fn verify(&self, proof: &Proof) -> Result<()> {
        let meta = self.file.metadata()?;
        ensure!(
            meta.is_file()
                && meta.file_attributes() & 0x400 == 0
                && meta.len() == self.size
                && self.size <= LIMIT
                && crate::file_identity::Stable::read(&self.file)? == self.identity
                && proof
                    .offset
                    .checked_add(proof.bytes.len() as u64)
                    .is_some_and(|v| v <= self.size),
            "Presentation durable receipt ownership changed"
        );
        let mut reader = &self.file;
        reader.seek(SeekFrom::Start(proof.offset))?;
        let mut actual = vec![0; proof.bytes.len()];
        reader.read_exact(&mut actual)?;
        ensure!(
            actual == proof.bytes,
            "Presentation intent read-back differs"
        );
        Ok(())
    }
    fn note(&mut self, event: &str, operation: &str, message: &str) -> Result<()> {
        self.append(json!({"event":event,"operation":operation,"message":message.chars().take(1024).collect::<String>()}))?;
        Ok(())
    }
}

#[path = "presentation_v2_native.rs"]
mod native;
pub(crate) struct Runner {
    binding: Binding,
    options: Options,
    engine: Engine,
    native: Option<native::Native>,
    journal: Option<Journal>,
}
impl Runner {
    pub fn new(binding: Binding, options: Options) -> Self {
        Self {
            binding,
            options,
            engine: Engine::new(options),
            native: None,
            journal: None,
        }
    }
    pub fn status(&self) -> Value {
        self.engine.status()
    }
    /// Call from the already-owned provider's serialized feature worker only.
    /// It acquires the common native write gate before any apply/restore tick.
    pub fn tick(&mut self, parent_alive: bool) -> Result<()> {
        if !self.options.enabled()
            || (self.engine.held && (!self.engine.cleanup_allowed || self.engine.crowd.is_none()))
        {
            return Ok(());
        }
        let _gate = session_safety::write_gate()?;
        if self.native.is_none() {
            if !parent_alive {
                self.engine.message = "Launcher closed; no new presentation preparation".into();
                return Ok(());
            }
            self.native = match native::Native::open(&self.binding) {
                Ok(native) => Some(native),
                Err(e) => {
                    self.engine.message =
                        format!("Waiting for the exact owned Version 2 game: {e:#}");
                    return Ok(());
                }
            };
        }
        if self.journal.is_none() {
            match Journal::create(&self.binding, self.options) {
                Ok(journal) => self.journal = Some(journal),
                Err(e) => {
                    self.engine.held = true;
                    self.engine.message =
                        format!("Presentation journal unavailable; no writes: {e:#}");
                    return Ok(());
                }
            }
        }
        let result = self.engine.tick(
            self.native.as_ref().unwrap(),
            self.journal.as_mut().unwrap(),
            parent_alive,
        );
        match result {
            Err(e) if !self.engine.held => {
                self.engine.message = format!("Waiting for a stable presentation boundary: {e:#}");
                Ok(())
            }
            // A presentation failure must not abort the independent native
            // loader/mod route. Status records the hold; keep this Runner so
            // its already-owned partial crowd can reach safe menu cleanup.
            Err(_) => Ok(()),
            Ok(()) => Ok(()),
        }
    }
    /// Only after the provider proves this retained game exited and a fresh
    /// known process snapshot proves every NBA2K27.exe absent. Never replays.
    pub fn finish_ended(&mut self, rows: &[session_safety::Process]) -> Result<()> {
        ensure!(
            !rows
                .iter()
                .any(|r| r.name.eq_ignore_ascii_case("NBA2K27.exe")),
            "Game still present; presentation log held"
        );
        if let Some(native) = &self.native {
            native.ended()?;
        }
        if let Some(mut journal) = self.journal.take() {
            // Best effort: the journal is already taken, so failing here (full
            // budget, changed length) would strand the marker and block the
            // next launch. remove_ended re-proves ownership and game absence.
            let _ = journal.note(
                "game-ended",
                "presentation",
                "Memory changes no longer exist; no restoration replay",
            );
            drop(journal);
            remove_ended(
                &self.binding.portable,
                &self.binding.game,
                &self.binding.session,
                rows,
            )?;
        }
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
    ensure!(
        !rows
            .iter()
            .any(|r| r.name.eq_ignore_ascii_case("NBA2K27.exe")),
        "Close every NBA 2K27 before presentation log recovery"
    );
    let _pins = [
        crate::loader_files::Directories::pin(root)?,
        crate::loader_files::Directories::pin(&root.join("Data"))?,
    ];
    let path = root.join("Data").join(MARKER);
    let mut file = fs::OpenOptions::new()
        .read(true)
        .access_mode(0x80000000 | 0x10000)
        .share_mode(1)
        .custom_flags(0x00200000)
        .open(&path)?;
    let meta = file.metadata()?;
    ensure!(
        meta.is_file() && meta.file_attributes() & 0x400 == 0 && (1..=LIMIT).contains(&meta.len()),
        "Unsupported presentation recovery log"
    );
    let mut bytes = Vec::new();
    (&mut file).take(LIMIT + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 == meta.len(),
        "Presentation recovery log changed"
    );
    let first = bytes
        .split(|b| *b == b'\n')
        .next()
        .context("Missing presentation header")?;
    let header: Value = serde_json::from_slice(first)?;
    ensure!(
        header["schema"] == "nba2k27-presentation-v2-session-v1"
            && header["event"] == "header"
            && header["session"] == session
            && header["game"] == serde_json::to_value(game)?,
        "Presentation recovery ownership differs"
    );
    crate::loader_files::unlink(&file)
}

#[cfg(test)]
#[path = "presentation_v2_runtime_tests.rs"]
mod tests;
