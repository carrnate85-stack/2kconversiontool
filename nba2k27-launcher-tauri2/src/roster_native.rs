//! Owned roster snapshots and explicit, verified player saves for the original
//! browser editor. No bundled helper runtime, hooks or automatic game writes.
use crate::{
    blacktop_native::{self, Memory, Reader},
    roster_snapshot::{self, Allocation, VerifiedLayout},
    session_safety,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const SCAN_BUDGET: Duration = Duration::from_secs(12);
#[path = "roster_uniform_access.rs"]
mod uniform_access;
#[derive(Clone)]
struct NativeSnapshot {
    game: session_safety::Identity,
    image: u64,
    root: u64,
    owner: u64,
    allocation: Allocation,
    table: u64,
    count: usize,
    players: Vec<roster_snapshot::PlayerIdentity>,
    teams: Option<roster_snapshot::TeamDirectory>,
}
impl NativeSnapshot {
    fn owner_for(&self, index: usize) -> Result<crate::player_transaction::Owner> {
        let row = self
            .players
            .iter()
            .find(|p| p.index == index)
            .context("Player is not in this tab's native snapshot")?;
        Ok(crate::player_transaction::Owner {
            game: self.game.clone(),
            image: self.image,
            root: self.root,
            owner: self.owner,
            allocation_base: self.allocation.base,
            allocation_len: self.allocation.len,
            table: self.table,
            count: self.count,
            index,
            player_id: row.id,
            appearance: row.appearance,
            serializer: roster_snapshot::serializer_fingerprint().into(),
            audio_verified: false,
            origin_stamp: None,
        })
    }
}
struct PlayerAccess<'a> {
    writer: blacktop_native::PlayerWriter,
    expected: crate::player_transaction::Owner,
    record: u64,
    stop: &'a AtomicBool,
    started: Instant,
}
impl<'a> PlayerAccess<'a> {
    fn open(
        native: &NativeSnapshot,
        game: &Path,
        index: usize,
        stop: &'a AtomicBool,
    ) -> Result<Self> {
        ensure!(!stop.load(Ordering::Acquire), "Roster service is closing");
        let owner = native.owner_for(index)?;
        ensure!(
            gate(&session_safety::processes()?, game)? == owner.game,
            "The game differs from this editor snapshot"
        );
        let record = owner
            .table
            .checked_add(
                (index as u64)
                    .checked_mul(roster_snapshot::PLAYER_STRIDE as u64)
                    .context("Player index overflow")?,
            )
            .context("Player table overflow")?;
        let writer =
            blacktop_native::PlayerWriter::open(&owner.game, game, record, owner.appearance)?;
        VerifiedLayout::verify(writer.reader(), owner.table)?;
        let result = Self {
            writer,
            expected: owner,
            record,
            stop,
            started: Instant::now(),
        };
        use crate::player_transaction::Access;
        ensure!(
            result.owner()? == result.expected,
            "The active database differs from this editor snapshot"
        );
        Ok(result)
    }
}
impl crate::player_transaction::Access for PlayerAccess<'_> {
    fn owner(&self) -> Result<crate::player_transaction::Owner> {
        let m = self.writer.reader();
        let root = pointer(m, m.base() + 0x9A47EB8)?;
        let owner = pointer(m, root + 0xA8)?;
        let header = m.read(owner + 0x10, 16)?;
        let count = u32::from_le_bytes(header[..4].try_into()?) as usize;
        let table = u64::from_le_bytes(header[8..].try_into()?);
        let (base, len) = m.roster_region(table)?;
        let raw = m.read(self.record, roster_snapshot::PLAYER_STRIDE)?;
        let mut result = self.expected.clone();
        result.game = m.identity.clone();
        result.image = m.base();
        result.root = root;
        result.owner = owner;
        result.allocation_base = base;
        result.allocation_len = len;
        result.table = table;
        result.count = count;
        result.player_id = u16::from_le_bytes(raw[0x128..0x12A].try_into()?);
        result.appearance = u64::from_le_bytes(raw[0x78..0x80].try_into()?);
        ensure!(
            root == pointer(m, m.base() + 0x9A47EB8)?
                && owner == pointer(m, root + 0xA8)?
                && header == m.read(owner + 0x10, 16)?,
            "Player database is changing"
        );
        Ok(result)
    }
    fn capture(&self) -> Result<crate::player_changes::RawPlayer> {
        let m = self.writer.reader();
        let record = m.read(self.record, roster_snapshot::PLAYER_STRIDE)?;
        let appearance = m.read(self.expected.appearance, 4)?;
        ensure!(
            record == m.read(self.record, roster_snapshot::PLAYER_STRIDE)?
                && appearance == m.read(self.expected.appearance, 4)?,
            "Player values changed during inspection"
        );
        Ok(crate::player_changes::RawPlayer { record, appearance })
    }
    fn write(&self, area: crate::player_changes::Area, offset: usize, bytes: &[u8]) -> Result<()> {
        self.writer.write(area, offset, bytes)
    }
    fn checkpoint(&self) -> Result<()> {
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Player save cancelled because the launcher is closing"
        );
        ensure!(
            self.started.elapsed() < Duration::from_secs(12),
            "Player save exceeded its time budget"
        );
        Ok(())
    }
}
fn read_large(m: &impl Memory, at: u64, len: usize) -> Result<Vec<u8>> {
    ensure!(len > 0 && len <= 0xA00000, "Unsupported roster read size");
    let mut bytes = Vec::with_capacity(len);
    for offset in (0..len).step_by(4096) {
        bytes.extend(
            m.read(
                at.checked_add(offset as u64)
                    .context("Roster read overflow")?,
                (len - offset).min(4096),
            )?,
        );
    }
    ensure!(bytes.len() == len, "Incomplete roster data read");
    Ok(bytes)
}
fn team_current(
    m: &impl Memory,
    d: &crate::team_changes::Directory,
    index: usize,
) -> Result<crate::team_changes::Current> {
    use crate::team_changes::{ARENA_POINTER, ARENA_STRIDE, TEAM_STRIDE};
    d.validate()?;
    ensure!(index < d.team_count, "Team index out of range");
    let team = read_large(m, d.team_table + (index * TEAM_STRIDE) as u64, TEAM_STRIDE)?;
    let arenas = read_large(m, d.arena_table, d.arena_count * ARENA_STRIDE)?
        .chunks_exact(ARENA_STRIDE)
        .map(|r| r.to_vec())
        .collect();
    let references = (0..d.team_count)
        .map(|i| {
            Ok(u64::from_le_bytes(
                m.read(d.team_table + (i * TEAM_STRIDE + ARENA_POINTER) as u64, 8)?
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Incomplete team reference"))?,
            ))
        })
        .collect::<Result<_>>()?;
    Ok(crate::team_changes::Current {
        team,
        arenas,
        references,
    })
}
struct TeamAccess<'a> {
    writer: blacktop_native::TeamWriter,
    expected: crate::team_transaction::Owner,
    stop: &'a AtomicBool,
    started: Instant,
}
impl<'a> TeamAccess<'a> {
    fn prepare(
        native: &NativeSnapshot,
        game: &Path,
        edit: &crate::team_changes::Edit,
        cached: &Value,
        stop: &'a AtomicBool,
    ) -> Result<(
        Self,
        crate::team_transaction::Owner,
        crate::team_changes::Plan,
    )> {
        use crate::{team_changes as t, team_transaction as tx};
        use tx::Access;
        ensure!(!stop.load(Ordering::Acquire), "Roster service is closing");
        let meta = native
            .teams
            .as_ref()
            .context("This page has no native team snapshot; refresh teams")?;
        let d = &meta.directory;
        d.validate()?;
        let row = meta
            .teams
            .get(edit.record_index)
            .context("Team is absent from this native snapshot")?;
        ensure!(
            row.id == edit.team_id,
            "Team identity differs from this native snapshot"
        );
        ensure!(
            gate(&session_safety::processes()?, game)? == native.game,
            "Game differs from this editor snapshot"
        );
        let reader = Reader::open_roster(&native.game, game)?;
        let m = Bounded {
            reader: &reader,
            stop,
            started: Instant::now(),
        };
        VerifiedLayout::verify(&m, native.table)?;
        let current = team_current(&m, d, edit.record_index)?;
        ensure!(
            current == team_current(&m, d, edit.record_index)?,
            "The team or arena directory changed during inspection"
        );
        let plan = t::plan(edit, cached, d, &current, row.arena)?;
        ensure!(
            meta.arena_hashes.get(plan.donor) == Some(&t::hash(&current.arenas[plan.donor])),
            "The selected arena changed since this tab opened; refresh teams"
        );
        let expected = tx::Owner {
            game: native.game.clone(),
            image: native.image,
            root: native.root,
            owner: native.owner,
            allocation_base: native.allocation.base,
            allocation_len: native.allocation.len,
            player_table: native.table,
            player_count: native.count,
            team_table: d.team_table,
            team_count: d.team_count,
            arena_table: d.arena_table,
            arena_count: d.arena_count,
            uniform_header: meta.uniform_header.clone(),
            index: edit.record_index,
            team_id: row.id,
            donor: plan.donor,
            donor_hash: t::hash(&current.arenas[plan.donor]),
            arena_ids_hash: t::hash(
                &current
                    .arenas
                    .iter()
                    .flat_map(|r| r[t::ARENA_ID..t::ARENA_ID + 2].iter().copied())
                    .collect::<Vec<_>>(),
            ),
            other_references_hash: tx::references_hash(&current.references, edit.record_index),
            clone_index: plan.clone_index,
            clone_id: plan.clone_index.map(|_| plan.arena_id),
            serializer: roster_snapshot::serializer_fingerprint().into(),
        };
        expected.validate()?;
        let writer = blacktop_native::TeamWriter::open(
            &native.game,
            game,
            d.team_table + (edit.record_index * t::TEAM_STRIDE) as u64,
            plan.clone_index.map(|i| d.arena_address(i)).transpose()?,
        )?;
        VerifiedLayout::verify(writer.reader(), native.table)?;
        let result = Self {
            writer,
            expected: expected.clone(),
            stop,
            started: Instant::now(),
        };
        ensure!(
            result.owner()? == expected && result.capture()? == plan.before,
            "The team owner or values changed before Save"
        );
        Ok((result, expected, plan))
    }
}
impl crate::team_transaction::Access for TeamAccess<'_> {
    fn owner(&self) -> Result<crate::team_transaction::Owner> {
        use crate::team_changes as t;
        let m = self.writer.reader();
        let e = &self.expected;
        let root = pointer(m, m.base() + 0x9A47EB8)?;
        let owner = pointer(m, root + 0xA8)?;
        let ph = m.read(owner + 0x10, 16)?;
        let th = m.read(e.allocation_base + 0xD0, 16)?;
        let ah = m.read(e.allocation_base + 0xB0, 16)?;
        let uh = m.read(e.allocation_base + 0x290, 16)?;
        ensure!(
            uh == e.uniform_header,
            "Uniform directory changed; team save refused"
        );
        let mut result = e.clone();
        result.game = m.identity.clone();
        result.image = m.base();
        result.root = root;
        result.owner = owner;
        result.player_table = t::uint(&ph, 8, 8)?;
        result.player_count = t::uint(&ph, 0, 4)? as usize;
        let (base, len) = m.roster_region(result.player_table)?;
        result.allocation_base = base;
        result.allocation_len = len;
        result.team_table = t::uint(&th, 8, 8)?;
        result.team_count = usize::try_from(t::uint(&th, 0, 8)?)?;
        result.arena_table = t::uint(&ah, 8, 8)?;
        result.arena_count = usize::try_from(t::uint(&ah, 0, 8)?)?;
        ensure!(
            result.game == e.game
                && result.image == e.image
                && result.root == e.root
                && result.owner == e.owner
                && result.allocation_base == e.allocation_base
                && result.allocation_len == e.allocation_len
                && result.player_table == e.player_table
                && result.player_count == e.player_count
                && result.team_table == e.team_table
                && result.team_count == e.team_count
                && result.arena_table == e.arena_table
                && result.arena_count == e.arena_count,
            "Team directory owner changed"
        );
        result.team_id = t::uint(
            &m.read(
                e.team_table + (e.index * t::TEAM_STRIDE + t::TEAM_ID) as u64,
                2,
            )?,
            0,
            2,
        )? as u16;
        result.donor_hash = t::hash(&read_large(
            m,
            e.arena_table + (e.donor * t::ARENA_STRIDE) as u64,
            t::ARENA_STRIDE,
        )?);
        let mut ids = Vec::with_capacity(e.arena_count * 2);
        for i in 0..e.arena_count {
            ids.extend(m.read(
                e.arena_table + (i * t::ARENA_STRIDE + t::ARENA_ID) as u64,
                2,
            )?);
        }
        result.arena_ids_hash = t::hash(&ids);
        let refs = (0..e.team_count)
            .map(|i| {
                Ok(t::uint(
                    &m.read(
                        e.team_table + (i * t::TEAM_STRIDE + t::ARENA_POINTER) as u64,
                        8,
                    )?,
                    0,
                    8,
                )?)
            })
            .collect::<Result<Vec<_>>>()?;
        if let Some(i) = e.clone_index {
            let at = e.arena_table + (i * t::ARENA_STRIDE) as u64;
            ensure!(
                !refs
                    .iter()
                    .enumerate()
                    .any(|(j, p)| j != e.index && *p == at),
                "Another team claimed the arena clone"
            );
        }
        result.other_references_hash = crate::team_transaction::references_hash(&refs, e.index);
        ensure!(
            ph == m.read(owner + 0x10, 16)?
                && uh == m.read(e.allocation_base + 0x290, 16)?
                && th == m.read(e.allocation_base + 0xD0, 16)?
                && ah == m.read(e.allocation_base + 0xB0, 16)?
                && root == pointer(m, m.base() + 0x9A47EB8)?
                && owner == pointer(m, root + 0xA8)?,
            "Team directories changed during inspection"
        );
        Ok(result)
    }
    fn capture(&self) -> Result<crate::team_changes::Raw> {
        use crate::team_changes as t;
        let m = self.writer.reader();
        let e = &self.expected;
        let capture = || -> Result<t::Raw> {
            Ok(t::Raw {
                team: read_large(
                    m,
                    e.team_table + (e.index * t::TEAM_STRIDE) as u64,
                    t::TEAM_STRIDE,
                )?,
                clone: e
                    .clone_index
                    .map(|i| {
                        read_large(
                            m,
                            e.arena_table + (i * t::ARENA_STRIDE) as u64,
                            t::ARENA_STRIDE,
                        )
                    })
                    .transpose()?
                    .unwrap_or_default(),
            })
        };
        let raw = capture()?;
        ensure!(
            raw == capture()?,
            "Team or reserved arena changed during read"
        );
        Ok(raw)
    }
    fn write(&self, area: crate::team_changes::Area, offset: usize, bytes: &[u8]) -> Result<()> {
        self.writer.write(area, offset, bytes)
    }
    fn checkpoint(&self) -> Result<()> {
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Team save cancelled because the launcher is closing"
        );
        ensure!(
            self.started.elapsed() < Duration::from_secs(12),
            "Team save exceeded its time budget"
        );
        Ok(())
    }
}
struct Snapshot {
    view: Value,
    native: Option<NativeSnapshot>,
}
impl From<Value> for Snapshot {
    fn from(view: Value) -> Self {
        Self { view, native: None }
    }
}
impl std::ops::Deref for Snapshot {
    type Target = Value;
    fn deref(&self) -> &Value {
        &self.view
    }
}
/// The existing unchanged browser bridge already assigns a fresh UUID per tab.
/// Bound the cache and share immutable snapshots rather than copying a complete
/// roster for every tab. Status polling never upgrades an existing draft.
#[derive(Default)]
struct Tabs {
    revision: u64,
    rows: BTreeMap<String, (u64, Arc<Snapshot>)>,
}
pub(crate) fn valid_tab(tab: &str) -> bool {
    tab.len() == 36
        && tab.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}
impl Tabs {
    fn bind(&mut self, tab: &str, snapshot: Arc<Snapshot>, replace: bool) -> Result<Arc<Snapshot>> {
        ensure!(
            valid_tab(tab),
            "Reopen the roster editor from the launcher; tab identity is invalid"
        );
        self.revision = self.revision.wrapping_add(1);
        if let Some((used, old)) = self.rows.get_mut(tab) {
            *used = self.revision;
            if replace {
                *old = snapshot;
            }
            return Ok(old.clone());
        }
        if self.rows.len() >= 16 {
            let key = self
                .rows
                .iter()
                .min_by_key(|(_, (n, _))| *n)
                .unwrap()
                .0
                .clone();
            self.rows.remove(&key);
        }
        self.rows
            .insert(tab.to_owned(), (self.revision, snapshot.clone()));
        Ok(snapshot)
    }
    fn get(&self, tab: &str) -> Result<Arc<Snapshot>> {
        ensure!(valid_tab(tab), "Invalid editor tab identity");
        self.rows
            .get(tab)
            .map(|(_, s)| s.clone())
            .context("This editor snapshot expired. Refresh the roster before saving.")
    }
}
struct Bounded<'a> {
    reader: &'a Reader,
    stop: &'a AtomicBool,
    started: Instant,
}
impl Memory for Bounded<'_> {
    fn base(&self) -> u64 {
        self.reader.base()
    }
    fn read(&self, address: u64, len: usize) -> Result<Vec<u8>> {
        self.checkpoint()?;
        self.reader.read(address, len)
    }
    fn checkpoint(&self) -> Result<()> {
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Roster read cancelled because the launcher is closing"
        );
        ensure!(
            self.started.elapsed() < SCAN_BUDGET,
            "Roster snapshot exceeded its time budget; wait for loading to finish and refresh"
        );
        Ok(())
    }
}
fn pointer(m: &impl Memory, at: u64) -> Result<u64> {
    roster_snapshot::checked_pointer(u64::from_le_bytes(
        m.read(at, 8)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("Incomplete roster pointer"))?,
    ))
}
fn gate(rows: &[session_safety::Process], game: &Path) -> Result<session_safety::Identity> {
    let games: Vec<_> = rows
        .iter()
        .filter(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe"))
        .collect();
    ensure!(
        games.len() == 1,
        "NBA 2K27 is not running, or more than one game is open."
    );
    let id = games[0]
        .identity
        .clone()
        .context("Game process identity is unavailable")?;
    ensure!(
        crate::native_session::same_path(&id.path, &game.join("NBA2K27.exe")),
        "The running game differs from the configured NBA 2K27 folder"
    );
    blacktop_native::guard(rows, &id)?;
    Ok(id)
}
fn snapshot(game: &Path, stop: &AtomicBool) -> Result<Snapshot> {
    ensure!(!stop.load(Ordering::Acquire), "Roster service is closing");
    let id = gate(&session_safety::processes()?, game)?;
    let reader = Reader::open_roster(&id, game)?;
    let m = Bounded {
        reader: &reader,
        stop,
        started: Instant::now(),
    };
    // The fingerprinted game's active owner chooses the database. Unlike a
    // broad address-space scan, this cannot select an unrelated named buffer.
    let root = pointer(&m, m.base() + 0x9A47EB8)?;
    let owner = pointer(&m, root + 0xA8)?;
    let owner_header = m.read(owner + 0x10, 16)?;
    let count = u32::from_le_bytes(owner_header[..4].try_into()?) as u64;
    let table = u64::from_le_bytes(owner_header[8..].try_into()?);
    ensure!(
        (100..=8192).contains(&count),
        "The active roster is still loading"
    );
    let (base, len) = reader.roster_region(table)?;
    let allocation = Allocation { base, len };
    ensure!(
        allocation.valid() && base.checked_add(0x680) == Some(table),
        "The active roster allocation does not match the verified table layout"
    );
    let verified = VerifiedLayout::verify(&m, table)?;
    let (mut result, players, teams) =
        roster_snapshot::decode_owned(&m, allocation, Some(&verified))?;
    ensure!(
        m.read(owner + 0x10, 16)? == owner_header
            && result["declaredPlayerCount"].as_u64() == Some(count),
        "The active roster changed during inspection"
    );
    ensure!(
        result["decodedPlayerCount"]
            .as_u64()
            .is_some_and(|n| n >= 100),
        "The player-name validation threshold was not met"
    );
    blacktop_native::guard(&session_safety::processes()?, &id)?;
    ensure!(!stop.load(Ordering::Acquire), "Roster service is closing");
    for player in result["players"]
        .as_array_mut()
        .context("Roster players missing")?
    {
        player["source"] = json!("live-roster");
        player["nameStatus"] = json!("live-exact");
    }
    for team in result["teams"]
        .as_array_mut()
        .context("Roster teams missing")?
    {
        team["source"] = json!("live-team");
    }
    result["pid"] = json!(id.pid);
    result["processIdentity"] = serde_json::to_value(&id)?;
    result["readOnly"] = json!(false);
    result["writeAvailable"] = json!(true);
    result["status"] = json!("ready");
    result["totalCatalogEntries"] = result["decodedPlayerCount"].clone();
    result["message"] = json!(format!(
        "{} current players and {} named teams read without game writes. Explicit player, basic team and 52-control uniform Save & verify are available for QA, including separate team arena/floor; live acceptance is pending. Audio, From, staff, rotations, seasons and remaining services are not ported yet.",
        result["decodedPlayerCount"], result["namedTeamCount"]
    ));
    Ok(Snapshot {
        view: result,
        native: Some(NativeSnapshot {
            game: id,
            image: m.base(),
            root,
            owner,
            allocation,
            table,
            count: count as usize,
            players,
            teams: Some(teams),
        }),
    })
}
fn idle(message: &str) -> Value {
    json!({"status":"idle","readOnly":true,"writes":0,"writeAvailable":false,"message":message,"players":[],"teams":[],"arenas":[],"origins":[]})
}
type Engine = Arc<dyn Fn(&Path, &AtomicBool) -> Result<Snapshot> + Send + Sync>;
#[derive(Default)]
struct AutoState {
    game: PathBuf,
    retry_after: Option<Instant>,
}
pub struct Service {
    operation: Mutex<()>,
    last: Mutex<Arc<Snapshot>>,
    tabs: Mutex<Tabs>,
    stop: AtomicBool,
    engine: Engine,
    auto: Mutex<AutoState>,
    auto_pending: AtomicBool,
    finished: Condvar,
    /// Separate owned Version 2 backend: verified reads plus its own guarded
    /// Save & verify (roster_v2_write), never the legacy layout gates.
    v2: crate::roster_v2_access::Service,
    #[cfg(test)]
    waiters: std::sync::atomic::AtomicUsize,
}
struct Completion<'a>(&'a Service);
impl Drop for Completion<'_> {
    fn drop(&mut self) {
        self.0.auto_pending.store(false, Ordering::Release);
        self.0.finished.notify_all();
    }
}
#[cfg(test)]
struct Waiter<'a>(&'a std::sync::atomic::AtomicUsize);
#[cfg(test)]
impl Drop for Waiter<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
impl Service {
    pub fn new() -> Self {
        Self {
            operation: Mutex::new(()),
            last: Mutex::new(Arc::new(idle(
                "Open NBA 2K27 offline without anti-cheat, then refresh to read players and teams. Explicit player saves require a verified native snapshot; team saves and remaining services are not ported yet.",
            ).into())),
            tabs: Mutex::new(Tabs::default()),
            stop: AtomicBool::new(false),
            engine: Arc::new(snapshot),
            auto: Mutex::new(AutoState::default()),
            auto_pending: AtomicBool::new(false),
            finished: Condvar::new(),
            v2: crate::roster_v2_access::Service::new(),
            #[cfg(test)]
            waiters: std::sync::atomic::AtomicUsize::new(0),
        }
    }
    #[cfg(test)]
    pub fn view(&self) -> Result<Value> {
        Ok(self.current()?.view.clone())
    }
    fn current(&self) -> Result<Arc<Snapshot>> {
        Ok(self
            .last
            .lock()
            .map_err(|_| anyhow::anyhow!("Roster state lock failed"))?
            .clone())
    }
    #[cfg(test)]
    pub fn request(&self, method: &str, game: &Path) -> Result<Value> {
        Ok(self.request_snapshot(method, game)?.view.clone())
    }
    pub fn request_tab(&self, method: &str, game: &Path, tab: &str) -> Result<Value> {
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Roster service is closing"
        );
        ensure!(
            valid_tab(tab),
            "Reopen the roster editor from the launcher; tab identity is invalid"
        );
        if matches!(method, "status" | "playerResearchStatus") {
            if let Ok(snapshot) = self
                .tabs
                .lock()
                .map_err(|_| anyhow::anyhow!("Editor tab lock failed"))?
                .get(tab)
            {
                return Ok(snapshot.view.clone());
            }
            // A status poll must not grant a fresh baseline to an expired old
            // page whose unsaved fields still describe the previous snapshot.
            return Ok(idle(
                "This editor snapshot expired. Refresh live players before saving.",
            ));
        }
        let snapshot = self.request_snapshot(method, game)?;
        if snapshot["status"] != "ready" {
            return Ok(snapshot.view.clone());
        }
        let _guard = self
            .operation
            .lock()
            .map_err(|_| anyhow::anyhow!("Roster operation lock failed"))?;
        ensure!(
            Arc::ptr_eq(&snapshot, &self.current()?),
            "A newer roster operation completed. Refresh this editor before continuing."
        );
        let snapshot = self
            .tabs
            .lock()
            .map_err(|_| anyhow::anyhow!("Editor tab lock failed"))?
            .bind(tab, snapshot, method == "scanPlayerResearch")?;
        Ok(snapshot.view.clone())
    }
    /// Browser live-read entry point. A Version 2 game is read only through the
    /// owned read-only backend; the September build keeps the legacy path.
    pub fn request_tab_routed(
        &self,
        method: &str,
        game: &Path,
        tab: &str,
        session: &crate::native_session::Service,
    ) -> Result<Value> {
        self.route(
            method,
            game,
            tab,
            || {
                Ok(
                    crate::provider_access::verify_build(&game.join("NBA2K27.exe"))?.build["label"]
                        == "Version 2",
                )
            },
            || session.roster_v2_binding(game),
        )
    }
    fn route(
        &self,
        method: &str,
        game: &Path,
        tab: &str,
        version_two: impl FnOnce() -> Result<bool>,
        binding: impl FnOnce() -> Result<crate::roster_v2_access::Binding>,
    ) -> Result<Value> {
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Roster service is closing"
        );
        ensure!(
            valid_tab(tab),
            "Reopen the roster editor from the launcher; tab identity is invalid"
        );
        if matches!(method, "bootstrap" | "status" | "playerResearchStatus") {
            return if self.v2.owns_tab(tab) {
                self.v2.status(tab)
            } else {
                self.request_tab(method, game, tab)
            };
        }
        ensure!(
            matches!(method, "rosterAutoAttach" | "scanPlayerResearch"),
            "Unsupported read-only roster operation"
        );
        // An unsupported or unreadable executable keeps the legacy path and its
        // exact-build refusal; only a verified Version 2 image is rerouted.
        if !version_two().unwrap_or(false) {
            self.v2.release(tab);
            return self.request_tab(method, game, tab);
        }
        // Never leave a September-build baseline saveable in a Version 2 tab.
        self.tabs
            .lock()
            .map_err(|_| anyhow::anyhow!("Editor tab lock failed"))?
            .rows
            .remove(tab);
        match binding() {
            Ok(binding) => self.v2.request(method, tab, &binding),
            Err(error) => {
                let mut view = idle(&format!(
                    "Version 2 players and teams need a game launched by this launcher: {error:#}"
                ));
                view["status"] = json!(if method == "rosterAutoAttach" {
                    "waiting"
                } else {
                    "error"
                });
                Ok(view)
            }
        }
    }
    fn refuse_v2_write(&self, tab: &str) -> Result<()> {
        if self.v2.owns_tab(tab) {
            self.v2.reject_write()?;
        }
        Ok(())
    }
    /// Browser editor services beyond Save & verify (team staff, seasons).
    /// Version 2 tabs use the owned V2 backend; the September build keeps
    /// WPF's helper-only services unported.
    pub fn editor(
        &self,
        method: &str,
        root: &Path,
        tab: &str,
        input: &Value,
        confirm: &dyn Fn(&str) -> bool,
    ) -> Result<Value> {
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Roster service is closing"
        );
        ensure!(
            self.v2.owns_tab(tab),
            "Team staff and season services need a Version 2 roster refreshed in this editor. No game read or write was attempted."
        );
        self.v2.editor(method, tab, root, input, confirm)
    }
    /// The tab's own live player (WPF state.players lookup by recordIndex + key).
    pub fn live_player(&self, tab: &str, index: u64, key: &str) -> Option<Value> {
        let player = if self.v2.owns_tab(tab) {
            self.v2.view_player(tab, index)?
        } else {
            let snapshot = self.tabs.lock().ok()?.get(tab).ok()?;
            snapshot["players"]
                .as_array()?
                .iter()
                .find(|p| p["source"] == "live-roster" && p["recordIndex"].as_u64() == Some(index))
                .cloned()?
        };
        let identity = serde_json::to_string(&json!([
            player["firstName"].as_str().unwrap_or(""),
            player["lastName"].as_str().unwrap_or(""),
            player["cyberfaceId"]
        ]))
        .ok()?;
        (identity == key).then_some(player)
    }
    /// The tab's own current live view (V2 capture or legacy baseline).
    pub fn tab_view(&self, tab: &str) -> Result<Value> {
        Ok(if self.v2.owns_tab(tab) {
            self.v2.status(tab)?
        } else {
            self.tabs
                .lock()
                .map_err(|_| anyhow::anyhow!("Editor tab lock failed"))?
                .get(tab)?
                .view
                .clone()
        })
    }
    /// WPF exportCatalog() document for this tab's current live view.
    pub fn catalog(&self, tab: &str) -> Result<Value> {
        let view = self.tab_view(tab)?;
        ensure!(
            view["status"] == "ready" && view["players"].is_array(),
            "Refresh the live player directory before exporting a catalog."
        );
        let players: Vec<Value> = view["players"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                let id = p["cyberfaceId"].as_u64();
                json!({"source":p["source"],"rosterGroup":p.get("rosterGroup").cloned().unwrap_or(Value::Null),"nameStatus":p["nameStatus"],
                    "manifestListed":p["manifestListed"],"modPresent":p["modPresent"],"recordIndex":p["recordIndex"],"firstName":p["firstName"],
                    "lastName":p["lastName"],"name":p["name"],"cyberfaceId":p["cyberfaceId"],"cyberfaceIdDisplay":id.map_or(String::new(),|i|format!("{i:05}")),
                    "teamName":p["teamName"].as_str().unwrap_or(""),"position":p["position"].as_str().unwrap_or(""),"secondaryPosition":p["secondaryPosition"].as_str().unwrap_or(""),
                    "jerseyNumber":p.get("jerseyNumber").cloned().unwrap_or(json!("")),"heightCm":p["heightCm"],"wingspanCm":p["wingspanCm"],"weightLbs":p["weightLbs"],
                    "potential":p["potential"],"potentialRaw":p["potentialRaw"],"overall":p["overall"],"cachedOverallRaw":p["cachedOverallRaw"],"overallStatus":p["overallStatus"]})
            })
            .collect();
        let teams = view["teams"].as_array().cloned().unwrap_or_default();
        Ok(
            json!({"schema":"nba2k27-player-asset-catalog-v2","generatedAt":crate::roster_v2_services::utc_now(),
            "source":{"pid":view["pid"],"access":view["access"],"writes":view["writes"],"playerStride":view["playerStride"],"cyberfaceIdOffset":view["cyberfaceIdOffset"],
                "cyberfaceIdWidthBytes":view["cyberfaceIdWidthBytes"],"cyberfaceAnchorValidation":view["cyberfaceAnchorValidation"],"vitalsOffsets":view["vitalsOffsets"],
                "verifiedFields":view["verifiedFields"],"candidateFields":view["candidateFields"]},
            "count":players.len(),"teamCount":teams.len(),"teams":teams,"players":players}),
        )
    }
    /// WPF team-uniform:link. Version 2 only (the legacy helper link is unported).
    pub fn link_uniform(&self, root: &Path, tab: &str, input: &Value) -> Result<Value> {
        ensure!(
            self.v2.owns_tab(tab),
            "Adding a uniform needs a Version 2 roster refreshed in this editor. No game write was attempted."
        );
        self.v2.link_uniform(tab, root, input)
    }
    pub fn save_player(&self, root: &Path, game: &Path, tab: &str, input: &Value) -> Result<Value> {
        if self.v2.owns_tab(tab) {
            return self
                .v2
                .save(crate::roster_v2_write::Kind::Player, tab, root, input);
        }
        self.save_using(root, tab, input, |native, index| {
            PlayerAccess::open(native, game, index, &self.stop)
        })
    }
    /// Copying DNA reads only this tab's immutable snapshot. It never refreshes
    /// game memory, advances a tab lease, or stages/saves the destination.
    pub fn player_dna(&self, tab: &str, request: &Value) -> Result<Value> {
        if self.v2.owns_tab(tab) {
            return self.v2.player_dna(tab, request);
        }
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Roster service is closing"
        );
        let snapshot = self
            .tabs
            .lock()
            .map_err(|_| anyhow::anyhow!("Editor tab lock failed"))?
            .get(tab)?;
        ensure!(
            snapshot.native.is_some(),
            "Refresh live players before copying Player DNA"
        );
        let players = snapshot["players"]
            .as_array()
            .context("Player list missing")?;
        crate::player_dna::snapshot_request(players, request)
    }
    fn save_using<M: crate::player_transaction::Access>(
        &self,
        root: &Path,
        tab: &str,
        input: &Value,
        open: impl FnOnce(&NativeSnapshot, usize) -> Result<M>,
    ) -> Result<Value> {
        self.refuse_v2_write(tab)?;
        use crate::{player_changes, player_transaction};
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Roster service is closing"
        );
        let edit = player_changes::validate(input)?;
        let _operation = self.operation.try_lock().map_err(|_| {
            anyhow::anyhow!("Another roster operation is in progress. Wait before saving.")
        })?;
        let _write_gate = session_safety::write_gate()?;
        ensure!(
            !crate::roster_v2_write::any_pending(root)?,
            "A native roster transaction needs review; close NBA 2K27 before another legacy write"
        );
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Roster service is closing"
        );
        ensure!(
            !player_transaction::pending(root)?,
            "A player transaction needs review. Close NBA 2K27 and clear finished launch before retrying."
        );
        ensure!(
            !crate::game_rules::pending(root)?,
            "A Game Rules transaction needs review; further player writes blocked."
        );
        ensure!(
            !crate::team_transaction::pending(root)?,
            "A team transaction needs review; further player writes blocked."
        );
        ensure!(
            !crate::uniform_transaction::pending(root)?,
            "A uniform transaction needs review; further player writes blocked."
        );
        ensure!(
            !crate::roster_v2_write::pending(root)?,
            "A Version 2 roster transaction needs review; further player writes blocked."
        );
        let baseline = self
            .tabs
            .lock()
            .map_err(|_| anyhow::anyhow!("Editor tab lock failed"))?
            .get(tab)?;
        let native = baseline.native.as_ref().context(
            "This page has no native player snapshot. Refresh live players before saving.",
        )?;
        let owner = native.owner_for(edit.record_index)?;
        let cached = baseline["players"]
            .as_array()
            .context("Player list missing")?
            .iter()
            .find(|p| p["recordIndex"].as_u64() == Some(edit.record_index as u64))
            .context("Player is absent from this tab's snapshot")?;
        let m = open(native, edit.record_index)?;
        ensure!(
            m.owner()? == owner,
            "The player database or immutable player identity changed. Refresh live players before saving."
        );
        let raw = m.capture()?;
        let plan = player_changes::plan(&edit, cached, &raw, player_changes::Extras::default())?;
        ensure!(
            !plan
                .patches
                .iter()
                .any(|p| p.area == player_changes::Area::Appearance)
                || native
                    .players
                    .iter()
                    .filter(|p| p.appearance == owner.appearance)
                    .count()
                    == 1,
            "This player's measurements share another player's data. No save attempted; refresh the roster before editing height or wingspan."
        );
        let commit = player_transaction::apply(&m, &owner, &plan, |intent| {
            player_transaction::save(root, intent)
        })?;
        let projected = match roster_snapshot::project_player(
            cached,
            &commit.read_back.record,
            &commit.read_back.appearance,
        ) {
            Ok(p) => p,
            Err(e) => {
                self.tabs
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Editor tab lock failed"))?
                    .rows
                    .remove(tab);
                anyhow::bail!(
                    "The player was saved and verified, but the editor could not reload it ({e:#}). Refresh live players before another edit."
                );
            }
        };
        // Preserve committed edits from other tabs/players in the shared list.
        // Only this tab advances; older tabs must still pass their old values.
        let current = self.current()?;
        let basis = if current
            .native
            .as_ref()
            .and_then(|n| n.owner_for(edit.record_index).ok())
            .as_ref()
            == Some(&owner)
        {
            current
        } else {
            baseline.clone()
        };
        let mut view = basis.view.clone();
        let player = view["players"]
            .as_array_mut()
            .context("Committed player list missing")?
            .iter_mut()
            .find(|p| p["recordIndex"].as_u64() == Some(edit.record_index as u64))
            .context("Committed player missing")?;
        *player = projected;
        let next = Arc::new(Snapshot {
            view,
            native: basis.native.clone(),
        });
        *self
            .last
            .lock()
            .map_err(|_| anyhow::anyhow!("Roster state lock failed"))? = next.clone();
        self.tabs
            .lock()
            .map_err(|_| anyhow::anyhow!("Editor tab lock failed"))?
            .bind(tab, next.clone(), true)?;
        Ok(json!({"report":commit.report,"playerResearch":next.view}))
    }
    pub fn save_team(&self, root: &Path, game: &Path, tab: &str, input: &Value) -> Result<Value> {
        if self.v2.owns_tab(tab) {
            return self
                .v2
                .save(crate::roster_v2_write::Kind::Team, tab, root, input);
        }
        self.save_team_using(
            root,
            tab,
            input,
            |native, edit, cached| TeamAccess::prepare(native, game, edit, cached, &self.stop),
            || (self.engine)(game, &self.stop),
        )
    }
    fn save_team_using<M: crate::team_transaction::Access>(
        &self,
        root: &Path,
        tab: &str,
        input: &Value,
        prepare: impl FnOnce(
            &NativeSnapshot,
            &crate::team_changes::Edit,
            &Value,
        )
            -> Result<(M, crate::team_transaction::Owner, crate::team_changes::Plan)>,
        refresh: impl FnOnce() -> Result<Snapshot>,
    ) -> Result<Value> {
        self.refuse_v2_write(tab)?;
        use crate::{team_changes, team_transaction};
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Roster service is closing"
        );
        let edit = team_changes::validate(input)?;
        let _operation = self.operation.try_lock().map_err(|_| {
            anyhow::anyhow!("Another roster operation is in progress. Wait before saving.")
        })?;
        let _write_gate = session_safety::write_gate()?;
        ensure!(
            !crate::roster_v2_write::any_pending(root)?,
            "A native roster transaction needs review; close NBA 2K27 before another legacy write"
        );
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Roster service is closing"
        );
        ensure!(
            !team_transaction::pending(root)?
                && !crate::uniform_transaction::pending(root)?
                && !crate::player_transaction::pending(root)?
                && !crate::game_rules::pending(root)?
                && !crate::roster_v2_write::pending(root)?,
            "A native transaction needs review. Close NBA 2K27 and clear finished launch before retrying."
        );
        let baseline = self
            .tabs
            .lock()
            .map_err(|_| anyhow::anyhow!("Editor tab lock failed"))?
            .get(tab)?;
        let native = baseline
            .native
            .as_ref()
            .context("This page has no native team snapshot. Refresh teams before saving.")?;
        let cached = baseline["teams"]
            .as_array()
            .context("Team list missing")?
            .iter()
            .find(|p| p["recordIndex"].as_u64() == Some(edit.record_index as u64))
            .context("Team is absent from this tab's snapshot")?;
        let (m, owner, plan) = prepare(native, &edit, cached)?;
        ensure!(
            owner.game == native.game
                && owner.root == native.root
                && owner.owner == native.owner
                && owner.player_table == native.table
                && owner.index == edit.record_index
                && owner.team_id == edit.team_id,
            "The team lease differs from this tab's native snapshot"
        );
        let commit =
            team_transaction::apply(&m, &owner, &plan, |i| team_transaction::save(root, i))?;
        drop(m);
        // Like WPF, rebuild the teams/arenas and linked player names from actual
        // read-back, not optimistic form values. Never silently undo a committed
        // save just because the subsequent read-only refresh fails.
        let reloaded = (|| -> Result<Snapshot> {
            let next = refresh()?;
            let n = next
                .native
                .as_ref()
                .context("Native team refresh unavailable")?;
            ensure!(
                n.game == native.game
                    && n.root == native.root
                    && n.owner == native.owner
                    && n.table == native.table
                    && n.allocation.base == native.allocation.base
                    && n.allocation.len == native.allocation.len,
                "The active roster changed after the team was saved"
            );
            let actual = next["teams"]
                .as_array()
                .context("Refreshed team list missing")?
                .iter()
                .find(|p| p["recordIndex"].as_u64() == Some(owner.index as u64))
                .context("Saved team is absent from refreshed list")?;
            let fields = team_changes::fields(&commit.read_back.team)?;
            for (key, value) in fields.as_object().unwrap() {
                ensure!(
                    actual[key] == *value,
                    "Saved team values changed during refresh"
                );
            }
            let d = n
                .teams
                .as_ref()
                .context("Refreshed native team directory missing")?;
            ensure!(
                n.count == native.count
                    && d.directory.team_table == owner.team_table
                    && d.directory.team_count == owner.team_count
                    && d.directory.arena_table == owner.arena_table
                    && d.directory.arena_count == owner.arena_count,
                "The team directories changed after Save"
            );
            if let Some(i) = owner.clone_index {
                ensure!(
                    d.arena_hashes.get(i) == Some(&team_changes::hash(&commit.read_back.clone)),
                    "The arena/floor clone changed during refresh"
                );
            }
            let row = d
                .teams
                .get(owner.index)
                .context("Refreshed native team identity missing")?;
            ensure!(
                row.id == owner.team_id
                    && row.arena
                        == team_changes::uint(
                            &commit.read_back.team,
                            team_changes::ARENA_POINTER,
                            8
                        )?,
                "The saved team's arena changed during refresh"
            );
            Ok(next)
        })();
        let next = match reloaded {
            Ok(s) => Arc::new(s),
            Err(e) => {
                self.tabs
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Editor tab lock failed"))?
                    .rows
                    .remove(tab);
                anyhow::bail!(
                    "The team was saved and verified, but the editor could not reload it ({e:#}). Refresh live teams before another edit."
                );
            }
        };
        *self
            .last
            .lock()
            .map_err(|_| anyhow::anyhow!("Roster state lock failed"))? = next.clone();
        self.tabs
            .lock()
            .map_err(|_| anyhow::anyhow!("Editor tab lock failed"))?
            .bind(tab, next.clone(), true)?;
        Ok(json!({"report":commit.report,"playerResearch":next.view}))
    }
    pub fn save_uniform(
        &self,
        root: &Path,
        game: &Path,
        tab: &str,
        input: &Value,
    ) -> Result<Value> {
        if self.v2.owns_tab(tab) {
            return self
                .v2
                .save(crate::roster_v2_write::Kind::Uniform, tab, root, input);
        }
        self.save_uniform_using(
            root,
            tab,
            input,
            |native, edit, team, uniform| {
                uniform_access::UniformAccess::prepare(
                    native, game, edit, team, uniform, &self.stop,
                )
            },
            || (self.engine)(game, &self.stop),
        )
    }
    fn save_uniform_using<M: crate::uniform_transaction::Access>(
        &self,
        root: &Path,
        tab: &str,
        input: &Value,
        prepare: impl FnOnce(
            &NativeSnapshot,
            &crate::uniform_changes::Edit,
            &Value,
            &Value,
        ) -> Result<(
            M,
            crate::uniform_transaction::Owner,
            crate::uniform_changes::Plan,
        )>,
        refresh: impl FnOnce() -> Result<Snapshot>,
    ) -> Result<Value> {
        self.refuse_v2_write(tab)?;
        use crate::{uniform_changes as u, uniform_transaction as tx};
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Roster service is closing"
        );
        let edit = u::validate(input)?;
        let _operation = self.operation.try_lock().map_err(|_| {
            anyhow::anyhow!("Another roster operation is in progress. Wait before saving.")
        })?;
        let _write_gate = session_safety::write_gate()?;
        ensure!(
            !crate::roster_v2_write::any_pending(root)?,
            "A native roster transaction needs review; close NBA 2K27 before another legacy write"
        );
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Roster service is closing"
        );
        ensure!(
            !tx::pending(root)?
                && !crate::team_transaction::pending(root)?
                && !crate::player_transaction::pending(root)?
                && !crate::game_rules::pending(root)?
                && !crate::roster_v2_write::pending(root)?,
            "A native transaction needs review. Close NBA 2K27 and clear finished launch before retrying."
        );
        let baseline = self
            .tabs
            .lock()
            .map_err(|_| anyhow::anyhow!("Editor tab lock failed"))?
            .get(tab)?;
        let native = baseline
            .native
            .as_ref()
            .context("This page has no native uniform snapshot; refresh teams")?;
        let team = baseline["teams"]
            .as_array()
            .context("Team list missing")?
            .iter()
            .find(|t| {
                t["recordIndex"].as_u64() == Some(edit.record_index as u64)
                    && t["teamId"] == edit.team_id
            })
            .context("Team is absent from this tab's snapshot")?;
        let uniform = team["uniforms"]
            .as_array()
            .context("Uniform list missing")?
            .iter()
            .find(|r| {
                r["slot"].as_u64() == Some(edit.slot as u64)
                    && r["uniformId"] == edit.uniform_id
                    && r["baseFilename"] == edit.expected_base_filename
            })
            .context("Uniform is absent from this tab's team slot")?;
        let meta = native
            .teams
            .as_ref()
            .context("This tab has no private uniform directory; refresh teams")?;
        let (m, owner, plan) = prepare(native, &edit, team, uniform)?;
        ensure!(
            owner.game == native.game
                && owner.image == native.image
                && owner.root == native.root
                && owner.owner == native.owner
                && owner.allocation_base == native.allocation.base
                && owner.allocation_len == native.allocation.len
                && owner.player_table == native.table
                && owner.player_count == native.count
                && owner.index == edit.record_index
                && owner.team_id == edit.team_id
                && owner.slot == edit.slot,
            "Uniform lease differs from this tab's native snapshot"
        );
        let identity = meta
            .uniforms
            .get(owner.uniform_index)
            .context("Uniform lease has no private table identity")?;
        let selected = meta
            .teams
            .get(owner.index)
            .context("Uniform lease has no private team identity")?;
        ensure!(
            owner.team_table == meta.directory.team_table
                && owner.team_count == meta.directory.team_count
                && owner.arena_table == meta.directory.arena_table
                && owner.arena_count == meta.directory.arena_count
                && owner.uniform_table == crate::team_changes::uint(&meta.uniform_header, 8, 8)?
                && owner.uniform_count == meta.uniforms.len()
                && owner.uniform_count as u64
                    == crate::team_changes::uint(&meta.uniform_header, 0, 8)?
                && identity.id == owner.uniform_id
                && identity.team_id == owner.team_id
                && identity.filename == owner.filename
                && selected.id == owner.team_id
                && selected.uniform_slots.get(owner.slot) == Some(&owner.address()),
            "Uniform lease does not match this tab's native table or slot"
        );
        let commit = tx::apply(&m, &owner, &edit, &plan, |i| tx::save(root, i))?;
        drop(m);
        let reloaded = (|| -> Result<Snapshot> {
            let next = refresh()?;
            let n = next
                .native
                .as_ref()
                .context("Native uniform refresh unavailable")?;
            ensure!(
                n.game == native.game
                    && n.image == native.image
                    && n.root == native.root
                    && n.owner == native.owner
                    && n.table == native.table
                    && n.count == native.count
                    && n.allocation.base == native.allocation.base
                    && n.allocation.len == native.allocation.len,
                "The active roster changed after the uniform was saved"
            );
            let meta = n
                .teams
                .as_ref()
                .context("Refreshed native uniform metadata missing")?;
            ensure!(
                meta.directory.team_table == owner.team_table
                    && meta.directory.team_count == owner.team_count
                    && meta.directory.arena_table == owner.arena_table
                    && meta.directory.arena_count == owner.arena_count
                    && crate::team_changes::uint(&meta.uniform_header, 8, 8)?
                        == owner.uniform_table
                    && crate::team_changes::uint(&meta.uniform_header, 0, 8)?
                        == owner.uniform_count as u64,
                "Uniform directories changed after Save"
            );
            let row = meta
                .uniforms
                .get(owner.uniform_index)
                .context("Saved uniform is absent from refreshed directory")?;
            ensure!(
                row.id == owner.uniform_id
                    && row.team_id == owner.team_id
                    && row.filename == owner.filename
                    && row.hash == crate::team_changes::hash(&commit.read_back),
                "The saved uniform changed during refresh"
            );
            let t = meta
                .teams
                .get(owner.index)
                .context("Saved team identity is absent")?;
            let slots = t
                .uniform_slots
                .iter()
                .flat_map(|p| p.to_le_bytes())
                .collect::<Vec<_>>();
            let references = meta
                .teams
                .iter()
                .map(|t| t.uniform_slots.clone())
                .collect::<Vec<_>>();
            ensure!(
                t.id == owner.team_id
                    && t.uniform_slots.get(owner.slot) == Some(&owner.address())
                    && crate::team_changes::hash(&slots) == owner.slots_hash
                    && tx::reference_hash(&references, owner.address()) == owner.references_hash,
                "The saved uniform's team slots changed during refresh"
            );
            let current = next["teams"]
                .as_array()
                .context("Refreshed team list missing")?
                .iter()
                .find(|t| {
                    t["recordIndex"].as_u64() == Some(owner.index as u64)
                        && t["teamId"] == owner.team_id
                })
                .context("Saved team is absent from refreshed list")?;
            ensure!(
                current["name"] == owner.team_name
                    && current["uniforms"]
                        .as_array()
                        .is_some_and(|rows| rows
                            .iter()
                            .any(|r| r["slot"].as_u64() == Some(owner.slot as u64)
                                && r["uniformId"] == owner.uniform_id
                                && r["baseFilename"] == owner.filename)),
                "Saved uniform is absent from refreshed team slot"
            );
            Ok(next)
        })();
        let next = match reloaded {
            Ok(s) => Arc::new(s),
            Err(e) => {
                self.tabs
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Editor tab lock failed"))?
                    .rows
                    .remove(tab);
                anyhow::bail!(
                    "The uniform was saved and verified, but the editor could not reload it ({e:#}). Refresh live teams before another edit."
                );
            }
        };
        *self
            .last
            .lock()
            .map_err(|_| anyhow::anyhow!("Roster state lock failed"))? = next.clone();
        self.tabs
            .lock()
            .map_err(|_| anyhow::anyhow!("Editor tab lock failed"))?
            .bind(tab, next.clone(), true)?;
        Ok(serde_json::json!({"report":commit.report,"playerResearch":next.view}))
    }
    fn request_snapshot(&self, method: &str, game: &Path) -> Result<Arc<Snapshot>> {
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Roster service is closing"
        );
        match method {
            "bootstrap" | "status" | "playerResearchStatus" => return self.current(),
            "rosterAutoAttach" | "scanPlayerResearch" => {}
            _ => anyhow::bail!("Unsupported read-only roster operation"),
        }
        let _guard = match self.operation.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                ensure!(
                    method == "rosterAutoAttach",
                    "A roster read is already in progress; wait for it to finish"
                );
                #[cfg(test)]
                let _waiting = {
                    self.waiters.fetch_add(1, Ordering::SeqCst);
                    Waiter(&self.waiters)
                };
                let mut state = self
                    .last
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Roster state lock failed"))?;
                let start = Instant::now();
                while self.auto_pending.load(Ordering::Acquire)
                    && !self.stop.load(Ordering::Acquire)
                    && start.elapsed() < SCAN_BUDGET + Duration::from_secs(1)
                {
                    state = self
                        .finished
                        .wait_timeout(state, Duration::from_millis(50))
                        .map_err(|_| anyhow::anyhow!("Roster wait failed"))?
                        .0;
                }
                ensure!(
                    !self.stop.load(Ordering::Acquire),
                    "Roster service is closing"
                );
                if state["status"] == "ready" {
                    return Ok(state.clone());
                }
                let mut waiting = idle("Waiting for the game roster to finish loading…");
                waiting["status"] = json!("waiting");
                return Ok(Arc::new(waiting.into()));
            }
        };
        // Auto-attach is a one-time browser operation, as in WPF. Explicit
        // Refresh always gets a new snapshot; it never silently reuses a draft.
        if method == "rosterAutoAttach" {
            let current = self.current()?;
            if current["status"] == "ready" {
                if let Ok(id) = serde_json::from_value::<session_safety::Identity>(
                    current["processIdentity"].clone(),
                ) {
                    if session_safety::identity(id.pid).ok().as_ref() == Some(&id)
                        && crate::native_session::same_path(&id.path, &game.join("NBA2K27.exe"))
                    {
                        return Ok(current);
                    }
                }
            }
            let mut auto = self
                .auto
                .lock()
                .map_err(|_| anyhow::anyhow!("Roster retry lock failed"))?;
            if auto.game != game {
                auto.game = game.to_owned();
                auto.retry_after = None;
            }
            if auto.retry_after.is_some_and(|next| Instant::now() < next) {
                let mut waiting = idle("Waiting for the game roster to finish loading…");
                waiting["status"] = json!("waiting");
                return Ok(Arc::new(waiting.into()));
            }
            auto.retry_after = Some(Instant::now() + Duration::from_secs(15));
            self.auto_pending.store(true, Ordering::Release);
        }
        let _completion = Completion(self);
        *self
            .last
            .lock()
            .map_err(|_| anyhow::anyhow!("Roster state lock failed"))? = Arc::new(json!({"status":"scanning","readOnly":true,"writes":0,"players":[],"teams":[],"message":"Reading native player and team directories…"}).into());
        let result = (self.engine)(game, &self.stop).map(Arc::new);
        if method == "rosterAutoAttach"
            && result
                .as_ref()
                .err()
                .is_some_and(|e| e.to_string().starts_with("NBA 2K27 is not running"))
        {
            self.auto
                .lock()
                .map_err(|_| anyhow::anyhow!("Roster retry lock failed"))?
                .retry_after = None;
        }
        let next = match &result {
            Ok(v) => v.clone(),
            Err(e) => {
                let mut v = idle(&format!("{e:#}"));
                v["status"] = json!(if method == "rosterAutoAttach" {
                    "waiting"
                } else {
                    "error"
                });
                Arc::new(v.into())
            }
        };
        *self
            .last
            .lock()
            .map_err(|_| anyhow::anyhow!("Roster state lock failed"))? = next.clone();
        if method == "rosterAutoAttach" {
            Ok(next)
        } else {
            result
        }
    }
    // Set before the HTTP server joins its worker threads. Chunked memory reads
    // observe this flag, so closing never waits for the full scan budget.
    pub fn cancel(&self) {
        self.stop.store(true, Ordering::Release);
        self.v2.cancel();
        self.finished.notify_all();
    }
    #[cfg(test)]
    fn finished_waiters(&self) -> usize {
        self.waiters.load(Ordering::SeqCst)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, sync::Arc, thread};
    const TAB_A: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    const TAB_B: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    #[test]
    fn tab_baselines_are_immutable_bounded_and_shared_until_explicit_refresh() {
        let one = Arc::new(Snapshot::from(json!({"status":"ready","revision":1})));
        let two = Arc::new(Snapshot::from(json!({"status":"ready","revision":2})));
        let mut tabs = Tabs::default();
        assert!(tabs.bind("", one.clone(), true).is_err());
        assert!(tabs.bind("../bad", one.clone(), true).is_err());
        tabs.bind(TAB_A, one.clone(), false).unwrap();
        tabs.bind(TAB_B, one.clone(), false).unwrap();
        assert!(Arc::ptr_eq(
            &tabs.get(TAB_A).unwrap(),
            &tabs.get(TAB_B).unwrap()
        ));
        tabs.bind(TAB_A, two.clone(), false).unwrap();
        assert!(Arc::ptr_eq(&tabs.get(TAB_A).unwrap(), &one));
        tabs.bind(TAB_B, two.clone(), true).unwrap();
        assert!(Arc::ptr_eq(&tabs.get(TAB_B).unwrap(), &two));
        for i in 0..20 {
            tabs.bind(
                &format!("{i:08x}-0000-4000-8000-000000000000"),
                two.clone(),
                false,
            )
            .unwrap();
        }
        assert_eq!(tabs.rows.len(), 16);
        assert!(tabs.get(TAB_A).is_err());
    }
    #[derive(Clone)]
    struct FakePlayer {
        owner: crate::player_transaction::Owner,
        raw: Arc<Mutex<crate::player_changes::RawPlayer>>,
        writes: Arc<std::sync::atomic::AtomicUsize>,
    }
    impl crate::player_transaction::Access for FakePlayer {
        fn owner(&self) -> Result<crate::player_transaction::Owner> {
            Ok(self.owner.clone())
        }
        fn capture(&self) -> Result<crate::player_changes::RawPlayer> {
            Ok(self.raw.lock().unwrap().clone())
        }
        fn checkpoint(&self) -> Result<()> {
            Ok(())
        }
        fn write(
            &self,
            area: crate::player_changes::Area,
            offset: usize,
            bytes: &[u8],
        ) -> Result<()> {
            self.writes.fetch_add(1, Ordering::SeqCst);
            self.raw.lock().unwrap().bytes_mut(area)[offset..offset + bytes.len()]
                .copy_from_slice(bytes);
            Ok(())
        }
    }
    fn save_fixture() -> (Service, FakePlayer, Value, std::path::PathBuf) {
        let f = crate::player_changes::tests::fixture();
        let case = &f["basic"][1];
        let raw = crate::player_changes::tests::raw(case);
        let appearance = u64::from_le_bytes(raw.record[0x78..0x80].try_into().unwrap());
        let base = appearance - 0x30000;
        let native = NativeSnapshot {
            game: session_safety::Identity {
                pid: 7,
                started: "42".into(),
                path: r"C:\QA\NBA2K27.exe".into(),
            },
            image: 0x140000000,
            root: base + 0x31000,
            owner: base + 0x32000,
            allocation: Allocation {
                base,
                len: 0x800000,
            },
            table: base + 0x680,
            count: 100,
            players: vec![roster_snapshot::PlayerIdentity {
                index: 0,
                id: 42,
                appearance,
            }],
            teams: None,
        };
        let fake = FakePlayer {
            owner: native.owner_for(0).unwrap(),
            raw: Arc::new(Mutex::new(raw)),
            writes: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        };
        let snapshot = Arc::new(Snapshot {
            view: json!({"status":"ready","players":[case["cached"].clone()],"teams":[]}),
            native: Some(native),
        });
        let service = Service::new();
        *service.last.lock().unwrap() = snapshot.clone();
        service
            .tabs
            .lock()
            .unwrap()
            .bind(TAB_A, snapshot.clone(), true)
            .unwrap();
        service
            .tabs
            .lock()
            .unwrap()
            .bind(TAB_B, snapshot, true)
            .unwrap();
        let root = std::env::temp_dir().join(format!(
            "rust-roster-service-{}",
            crate::backend::unique_id()
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join("Data")).unwrap();
        (service, fake, case["input"].clone(), root)
    }
    fn clean_save_fixture(root: &Path) {
        let journal = crate::player_transaction::path(root);
        if journal.exists() {
            std::fs::remove_file(journal).unwrap();
        }
        std::fs::remove_dir(root.join("Data")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn explicit_save_projects_verified_values_without_upgrading_other_tabs() {
        let (s, m, input, root) = save_fixture();
        let result = s
            .save_using(&root, TAB_A, &input, |_, _| Ok(m.clone()))
            .unwrap();
        assert_eq!(result["report"]["readBackVerified"], true);
        assert_eq!(
            result["playerResearch"]["players"][0]["firstName"],
            input["firstName"]
        );
        assert_eq!(
            result["playerResearch"]["players"][0]["jerseyNumberRaw"],
            input["jerseyNumberRaw"]
        );
        assert!(m.writes.load(Ordering::SeqCst) > 0);
        let before = m.writes.load(Ordering::SeqCst);
        // Polling the second tab must keep its old baseline, not the saved row.
        let old = s
            .request_tab("playerResearchStatus", Path::new("synthetic-only"), TAB_B)
            .unwrap();
        assert_ne!(old["players"][0]["firstName"], input["firstName"]);
        assert!(
            s.save_using(&root, TAB_B, &input, |_, _| Ok(m.clone()))
                .is_err()
        );
        assert_eq!(m.writes.load(Ordering::SeqCst), before);
        assert!(!crate::player_transaction::pending(&root).unwrap());
        clean_save_fixture(&root);
    }
    #[test]
    fn recycled_player_preview_page_invalid_tab_and_recovery_never_write() {
        for case in 0..5 {
            let (s, mut m, input, root) = save_fixture();
            match case {
                0 => m.owner.player_id += 1,
                1 => m.owner.appearance += 8,
                2 => {
                    let old = s.current().unwrap();
                    let preview = Arc::new(Snapshot::from(old.view.clone()));
                    s.tabs.lock().unwrap().bind(TAB_A, preview, true).unwrap();
                }
                3 => {
                    std::fs::write(crate::player_transaction::path(&root), b"{bad").unwrap();
                }
                _ => s.cancel(),
            }
            assert!(
                s.save_using(&root, TAB_A, &input, |_, _| Ok(m.clone()))
                    .is_err()
            );
            assert_eq!(m.writes.load(Ordering::SeqCst), 0, "Case {case}");
            assert!(
                s.save_using(&root, "invalid-tab", &input, |_, _| Ok(m.clone()))
                    .is_err()
            );
            clean_save_fixture(&root);
        }
    }
    #[test]
    fn expired_tab_status_never_grants_new_save_ownership() {
        let (s, m, input, root) = save_fixture();
        s.tabs.lock().unwrap().rows.remove(TAB_A);
        let status = s
            .request_tab("playerResearchStatus", Path::new("synthetic-only"), TAB_A)
            .unwrap();
        assert_eq!(status["status"], "idle");
        assert!(s.tabs.lock().unwrap().get(TAB_A).is_err());
        assert!(
            s.save_using(&root, TAB_A, &input, |_, _| Ok(m.clone()))
                .is_err()
        );
        assert_eq!(m.writes.load(Ordering::SeqCst), 0);
        // Bootstrap is separate from status and supplies a complete fresh list.
        assert_eq!(
            s.request_tab("bootstrap", Path::new("synthetic-only"), TAB_A)
                .unwrap()["status"],
            "ready"
        );
        assert!(s.tabs.lock().unwrap().get(TAB_A).is_ok());
        clean_save_fixture(&root);
    }
    #[test]
    fn shared_measurements_are_rejected_before_intent_or_write() {
        let (s, m, input, root) = save_fixture();
        let old = s.current().unwrap();
        let mut native = old.native.clone().unwrap();
        native.players.push(roster_snapshot::PlayerIdentity {
            index: 1,
            id: 43,
            appearance: m.owner.appearance,
        });
        let next = Arc::new(Snapshot {
            view: old.view.clone(),
            native: Some(native),
        });
        s.tabs.lock().unwrap().bind(TAB_A, next, true).unwrap();
        assert!(
            s.save_using(&root, TAB_A, &input, |_, _| Ok(m.clone()))
                .unwrap_err()
                .to_string()
                .contains("share another")
        );
        assert_eq!(m.writes.load(Ordering::SeqCst), 0);
        assert!(!crate::player_transaction::path(&root).exists());
        clean_save_fixture(&root);
    }
    #[test]
    fn loaded_player_copy_is_tab_scoped_portable_and_never_a_game_write() {
        let (s, m, _, root) = save_fixture();
        let rows = s.player_dna(TAB_A, &json!({"action":"players"})).unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 1);
        let request = json!({"action":"copy","recordIndex":0,"key":rows[0]["key"],"sections":["info","attributes"]});
        let before = s.current().unwrap();
        let copied = s.player_dna(TAB_A, &request).unwrap();
        assert_eq!(copied["format"], "2kplayer");
        assert_eq!(copied["version"], 1);
        assert_eq!(
            copied["info"]["firstName"],
            before["players"][0]["firstName"]
        );
        assert!(copied.get("recordAddress").is_none());
        assert!(copied.get("teamAddress").is_none());
        assert!(Arc::ptr_eq(&before, &s.current().unwrap()));
        assert_eq!(m.writes.load(Ordering::SeqCst), 0);
        assert!(!crate::player_transaction::path(&root).exists());
        for (key, value) in [
            ("key", json!("stale")),
            ("recordIndex", json!(0.5)),
            ("sections", json!(["seasons"])),
            ("sections", json!([])),
        ] {
            let mut bad = request.clone();
            bad[key] = value;
            assert!(s.player_dna(TAB_A, &bad).is_err());
        }
        assert!(s.player_dna("invalid", &request).is_err());
        s.tabs.lock().unwrap().rows.remove(TAB_B);
        assert!(s.player_dna(TAB_B, &request).is_err());
        let sample = Arc::new(Snapshot::from(before.view.clone()));
        s.tabs.lock().unwrap().bind(TAB_A, sample, true).unwrap();
        assert!(s.player_dna(TAB_A, &request).is_err());
        clean_save_fixture(&root);
    }
    fn synthetic(_: &Path, stop: &AtomicBool) -> Result<Snapshot> {
        while !stop.load(Ordering::Acquire) {
            thread::sleep(Duration::from_millis(1));
        }
        anyhow::bail!("Synthetic read cancelled")
    }
    #[test]
    fn concurrent_read_refused_and_shutdown_cancels_owned_work() {
        let service = Arc::new(Service {
            engine: Arc::new(synthetic),
            ..Service::new()
        });
        let worker = service.clone();
        let handle = thread::spawn(move || {
            worker.request("scanPlayerResearch", Path::new("synthetic-only"))
        });
        let started = Instant::now();
        while service.view().unwrap()["status"] != "scanning" {
            assert!(started.elapsed() < Duration::from_secs(2));
            thread::yield_now();
        }
        assert!(
            service
                .request("scanPlayerResearch", Path::new("synthetic-only"))
                .unwrap_err()
                .to_string()
                .contains("already in progress")
        );
        let start = Instant::now();
        service.cancel();
        assert!(handle.join().unwrap().is_err());
        assert!(start.elapsed() < Duration::from_millis(500));
        assert!(
            service
                .request("rosterAutoAttach", Path::new("synthetic-only"))
                .is_err()
        );
    }
    #[test]
    fn no_game_foreign_path_and_protected_owners_never_pass_gate() {
        let game = Path::new(r"C:\QA\NBA 2K27");
        assert!(gate(&[], game).is_err());
        let id = session_safety::Identity {
            pid: 7,
            started: "42".into(),
            path: r"C:\Other\NBA2K27.exe".into(),
        };
        let mut rows = vec![session_safety::Process {
            name: "NBA2K27.exe".into(),
            pid: 7,
            parent: 0,
            identity: Some(id.clone()),
        }];
        assert!(gate(&rows, game).is_err());
        rows[0].identity.as_mut().unwrap().path = game.join("NBA2K27.exe").to_string_lossy().into();
        assert!(gate(&rows, game).is_ok());
        rows.push(session_safety::Process {
            name: "EasyAntiCheat_EOS.exe".into(),
            pid: 8,
            parent: 0,
            identity: None,
        });
        assert!(gate(&rows, game).is_err());
        assert!(Service::new().request("savePlayer", game).is_err());
    }
    #[test]
    fn automatic_retry_has_wpf_cooldown_but_manual_refresh_is_not_throttled() {
        use std::sync::atomic::AtomicUsize;
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let service = Service {
            engine: Arc::new(move |_, _| {
                observed.fetch_add(1, Ordering::SeqCst);
                anyhow::bail!("Roster is still loading")
            }),
            ..Service::new()
        };
        let game = Path::new("synthetic-only");
        assert_eq!(
            service.request("rosterAutoAttach", game).unwrap()["status"],
            "waiting"
        );
        assert_eq!(
            service.request("rosterAutoAttach", game).unwrap()["status"],
            "waiting"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(service.request("scanPlayerResearch", game).is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        service.auto.lock().unwrap().retry_after = Some(Instant::now() - Duration::from_secs(1));
        service.request("rosterAutoAttach", game).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }
    #[test]
    fn simultaneous_automatic_tabs_share_one_owned_read() {
        use std::sync::atomic::AtomicUsize;
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let release = Arc::new(AtomicBool::new(false));
        let wait = release.clone();
        let service = Arc::new(Service {
            engine: Arc::new(move |_, stop| {
                observed.fetch_add(1, Ordering::SeqCst);
                while !wait.load(Ordering::Acquire) {
                    ensure!(!stop.load(Ordering::Acquire), "Cancelled");
                    thread::sleep(Duration::from_millis(1));
                }
                Ok(
                    json!({"status":"ready","players":[],"teams":[],"readOnly":true,"writes":0})
                        .into(),
                )
            }),
            ..Service::new()
        });
        let first = service.clone();
        let one =
            thread::spawn(move || first.request("rosterAutoAttach", Path::new("synthetic-only")));
        let start = Instant::now();
        while calls.load(Ordering::SeqCst) == 0 {
            assert!(start.elapsed() < Duration::from_secs(2));
            thread::yield_now();
        }
        let second = service.clone();
        let two =
            thread::spawn(move || second.request("rosterAutoAttach", Path::new("synthetic-only")));
        // Wait until the second worker is in the condition-variable wait rather
        // than relying on a scheduling sleep before releasing the first scan.
        let start = Instant::now();
        while service.finished_waiters() == 0 {
            assert!(start.elapsed() < Duration::from_secs(2));
            thread::yield_now();
        }
        release.store(true, Ordering::Release);
        assert_eq!(one.join().unwrap().unwrap(), two.join().unwrap().unwrap());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
    #[derive(Clone)]
    struct FakeTeam {
        owner: crate::team_transaction::Owner,
        raw: Arc<Mutex<crate::team_changes::Raw>>,
    }
    impl crate::team_transaction::Access for FakeTeam {
        fn owner(&self) -> Result<crate::team_transaction::Owner> {
            Ok(self.owner.clone())
        }
        fn capture(&self) -> Result<crate::team_changes::Raw> {
            Ok(self.raw.lock().unwrap().clone())
        }
        fn write(&self, a: crate::team_changes::Area, o: usize, b: &[u8]) -> Result<()> {
            self.raw.lock().unwrap().bytes_mut(a)[o..o + b.len()].copy_from_slice(b);
            Ok(())
        }
        fn checkpoint(&self) -> Result<()> {
            Ok(())
        }
    }
    fn team_fixture() -> (
        Service,
        FakeTeam,
        Value,
        crate::team_changes::Plan,
        Snapshot,
        PathBuf,
    ) {
        let (p, o) = crate::team_transaction::tests::fixture();
        let (_, d, r) = crate::team_changes::tests::clone_plan();
        let mut cached = crate::team_changes::fields(&r.team).unwrap();
        cached["recordIndex"] = json!(0);
        cached["source"] = json!("live-team");
        let native = NativeSnapshot {
            game: o.game.clone(),
            image: o.image,
            root: o.root,
            owner: o.owner,
            allocation: Allocation {
                base: o.allocation_base,
                len: o.allocation_len,
            },
            table: o.player_table,
            count: o.player_count,
            players: vec![],
            teams: Some(roster_snapshot::TeamDirectory {
                directory: d,
                teams: r
                    .references
                    .iter()
                    .enumerate()
                    .map(|(i, p)| roster_snapshot::TeamIdentity {
                        id: 42 + i as u16,
                        arena: *p,
                        uniform_slots: vec![0; 32],
                    })
                    .collect(),
                arena_hashes: r
                    .arenas
                    .iter()
                    .map(|r| crate::team_changes::hash(r))
                    .collect(),
                uniform_header: vec![0; 16],
                uniforms: vec![],
            }),
        };
        let baseline = Arc::new(Snapshot {
            view: json!({"status":"ready","players":[],"teams":[cached]}),
            native: Some(native.clone()),
        });
        let service = Service::new();
        *service.last.lock().unwrap() = baseline.clone();
        for tab in [TAB_A, TAB_B] {
            service
                .tabs
                .lock()
                .unwrap()
                .bind(tab, baseline.clone(), true)
                .unwrap();
        }
        let mut after = crate::team_changes::fields(&p.after.team).unwrap();
        after["recordIndex"] = json!(0);
        after["source"] = json!("live-team");
        let mut after_native = native;
        after_native.teams.as_mut().unwrap().teams[0].arena =
            crate::team_changes::uint(&p.after.team, crate::team_changes::ARENA_POINTER, 8)
                .unwrap();
        if let Some(i) = p.clone_index {
            after_native.teams.as_mut().unwrap().arena_hashes[i] =
                crate::team_changes::hash(&p.after.clone);
        }
        let refreshed = Snapshot {
            view: json!({"status":"ready","players":[],"teams":[after]}),
            native: Some(after_native),
        };
        let root =
            std::env::temp_dir().join(format!("rust-team-service-{}", crate::backend::unique_id()));
        std::fs::create_dir_all(root.join("Data")).unwrap();
        let input = crate::team_changes::tests::fixture()[2]["input"].clone();
        let m = FakeTeam {
            owner: o,
            raw: Arc::new(Mutex::new(p.before.clone())),
        };
        (service, m, input, p, refreshed, root)
    }
    #[test]
    fn team_save_uses_original_tab_and_verified_reload_not_form_values() {
        let (s, m, input, p, next, root) = team_fixture();
        let report = s
            .save_team_using(
                &root,
                TAB_A,
                &input,
                |_, _, _| Ok((m.clone(), m.owner.clone(), p.clone())),
                || Ok(next),
            )
            .unwrap();
        assert_eq!(report["report"]["teamId"], 42);
        assert_eq!(*m.raw.lock().unwrap(), p.after);
        let tabs = s.tabs.lock().unwrap();
        assert_eq!(tabs.get(TAB_A).unwrap()["teams"][0]["arenaId"], Value::Null); // The fake refresh omits this derived label; it is not optimistically inserted.
        assert_eq!(
            tabs.get(TAB_B)
                .unwrap()
                .native
                .as_ref()
                .unwrap()
                .teams
                .as_ref()
                .unwrap()
                .teams[0]
                .arena,
            crate::team_changes::uint(&p.before.team, crate::team_changes::ARENA_POINTER, 8)
                .unwrap()
        );
        drop(tabs);
        let result = s.save_team_using(
            &root,
            TAB_B,
            &input,
            |native, edit, cached| {
                let meta = native.teams.as_ref().unwrap();
                let (_, mut raw, _) =
                    crate::team_changes::tests::data(&crate::team_changes::tests::fixture()[2]);
                raw.team = m.raw.lock().unwrap().team.clone();
                raw.references[0] =
                    crate::team_changes::uint(&raw.team, crate::team_changes::ARENA_POINTER, 8)?;
                let plan = crate::team_changes::plan(
                    edit,
                    cached,
                    &meta.directory,
                    &raw,
                    meta.teams[0].arena,
                )?;
                Ok((m.clone(), m.owner.clone(), plan))
            },
            || anyhow::bail!("Must not refresh"),
        );
        assert!(result.is_err());
        assert_eq!(*m.raw.lock().unwrap(), p.after);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn team_committed_refresh_failure_invalidates_only_saving_tab_and_never_replays() {
        let (s, m, input, p, _, root) = team_fixture();
        let error = s
            .save_team_using(
                &root,
                TAB_A,
                &input,
                |_, _, _| Ok((m.clone(), m.owner.clone(), p.clone())),
                || anyhow::bail!("Synthetic game closed"),
            )
            .unwrap_err();
        assert!(error.to_string().contains("saved and verified"));
        assert_eq!(*m.raw.lock().unwrap(), p.after);
        assert!(!crate::team_transaction::pending(&root).unwrap());
        let tabs = s.tabs.lock().unwrap();
        assert!(tabs.get(TAB_A).is_err());
        assert!(tabs.get(TAB_B).is_ok());
        drop(tabs);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn team_stop_invalid_tab_and_pending_journal_never_open_a_write_lease() {
        let (s, _, input, _, _, root) = team_fixture();
        let attempted = Cell::new(0);
        let call = |tab| {
            s.save_team_using::<FakeTeam>(
                &root,
                tab,
                &input,
                |_, _, _| {
                    attempted.set(attempted.get() + 1);
                    anyhow::bail!("Unexpected lease")
                },
                || anyhow::bail!("Unexpected refresh"),
            )
        };
        assert!(call("invalid").is_err());
        std::fs::write(crate::team_transaction::path(&root), b"unknown").unwrap();
        assert!(call(TAB_A).is_err());
        s.stop.store(true, Ordering::Release);
        assert!(call(TAB_A).is_err());
        assert_eq!(attempted.get(), 0);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[derive(Clone)]
    struct FakeUniform {
        owner: crate::uniform_transaction::Owner,
        raw: Arc<Mutex<Vec<u8>>>,
        writes: Arc<std::sync::atomic::AtomicUsize>,
    }
    impl crate::uniform_transaction::Access for FakeUniform {
        fn owner(&self) -> Result<crate::uniform_transaction::Owner> {
            Ok(self.owner.clone())
        }
        fn capture(&self) -> Result<Vec<u8>> {
            Ok(self.raw.lock().unwrap().clone())
        }
        fn write(&self, at: usize, b: &[u8]) -> Result<()> {
            self.writes.fetch_add(1, Ordering::SeqCst);
            self.raw.lock().unwrap()[at..at + b.len()].copy_from_slice(b);
            Ok(())
        }
        fn checkpoint(&self) -> Result<()> {
            Ok(())
        }
    }
    fn uniform_fixture() -> (
        Service,
        FakeUniform,
        Value,
        crate::uniform_changes::Plan,
        Snapshot,
        PathBuf,
    ) {
        use crate::{uniform_changes as u, uniform_transaction as tx};
        let (_, p, o, c) = tx::tests::fixture();
        let mut team = c["cached"].clone();
        team["source"] = json!("live-team");
        let raw_team = u::tests::bytes(&c["teamBefore"]);
        let slot0 = raw_team[0xDE0..0xEE0]
            .chunks_exact(8)
            .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
            .collect();
        let mut slot1 = vec![0; 32];
        slot1[0] = o.uniform_table + u::STRIDE as u64;
        let uniform_rows = u::tests::bytes(&c["allUniformsBefore"]);
        let uniform_header = [
            (o.uniform_count as u64).to_le_bytes(),
            o.uniform_table.to_le_bytes(),
        ]
        .concat();
        let directory = roster_snapshot::TeamDirectory {
            directory: crate::team_changes::Directory {
                team_table: o.team_table,
                team_count: o.team_count,
                arena_table: o.arena_table,
                arena_count: o.arena_count,
            },
            teams: vec![
                roster_snapshot::TeamIdentity {
                    id: 42,
                    arena: o.arena_table,
                    uniform_slots: slot0,
                },
                roster_snapshot::TeamIdentity {
                    id: 43,
                    arena: o.arena_table + crate::team_changes::ARENA_STRIDE as u64,
                    uniform_slots: slot1,
                },
                roster_snapshot::TeamIdentity {
                    id: 44,
                    arena: o.arena_table + 2 * crate::team_changes::ARENA_STRIDE as u64,
                    uniform_slots: vec![0; 32],
                },
            ],
            arena_hashes: vec!["0".repeat(64); 3],
            uniform_header,
            uniforms: uniform_rows
                .chunks_exact(u::STRIDE)
                .map(|r| roster_snapshot::UniformIdentity {
                    id: crate::team_changes::uint(r, 0x50, 2).unwrap() as u16,
                    team_id: crate::team_changes::uint(r, 0x52, 2).unwrap() as u16,
                    filename: u::ascii(r, 8, 64).unwrap(),
                    hash: crate::team_changes::hash(r),
                })
                .collect(),
        };
        let native = NativeSnapshot {
            game: o.game.clone(),
            image: o.image,
            root: o.root,
            owner: o.owner,
            allocation: Allocation {
                base: o.allocation_base,
                len: o.allocation_len,
            },
            table: o.player_table,
            count: o.player_count,
            players: vec![],
            teams: Some(directory),
        };
        let baseline = Arc::new(Snapshot {
            view: json!({"status":"ready","players":[],"teams":[team]}),
            native: Some(native.clone()),
        });
        let s = Service::new();
        *s.last.lock().unwrap() = baseline.clone();
        for tab in [TAB_A, TAB_B] {
            s.tabs
                .lock()
                .unwrap()
                .bind(tab, baseline.clone(), true)
                .unwrap();
        }
        let mut next_native = native;
        next_native.teams.as_mut().unwrap().uniforms[0].hash = crate::team_changes::hash(&p.after);
        let mut next_team = c["refreshed"].clone();
        next_team["source"] = json!("live-team");
        let next = Snapshot {
            view: json!({"status":"ready","players":[],"teams":[next_team]}),
            native: Some(next_native),
        };
        let root = std::env::temp_dir().join(format!(
            "rust-uniform-service-{}",
            crate::backend::unique_id()
        ));
        std::fs::create_dir_all(root.join("Data")).unwrap();
        let m = FakeUniform {
            owner: o,
            raw: Arc::new(Mutex::new(p.before.clone())),
            writes: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        };
        (s, m, c["input"].clone(), p, next, root)
    }
    #[test]
    fn uniform_save_refreshes_all_shared_slots_and_preserves_other_tab_baseline() {
        let (s, m, input, p, next, root) = uniform_fixture();
        let response = s
            .save_uniform_using(
                &root,
                TAB_A,
                &input,
                |_, _, _, _| Ok((m.clone(), m.owner.clone(), p.clone())),
                || Ok(next),
            )
            .unwrap();
        assert_eq!(response["report"]["writes"], 52);
        assert_eq!(*m.raw.lock().unwrap(), p.after);
        let uniforms = response["playerResearch"]["teams"][0]["uniforms"]
            .as_array()
            .unwrap();
        assert_eq!(uniforms.len(), 2);
        for r in uniforms {
            assert_eq!(r["primaryColor"], input["primaryColor"]);
            assert_eq!(r["typeId"], input["typeId"]);
        }
        let before = m.writes.load(Ordering::SeqCst);
        let result = s.save_uniform_using(
            &root,
            TAB_B,
            &input,
            |native, e, team, uniform| {
                let raw_team = crate::uniform_changes::tests::bytes(
                    &crate::uniform_changes::tests::fixture()[1]["teamBefore"],
                );
                let meta = native.teams.as_ref().unwrap();
                let address = meta.teams[e.record_index].uniform_slots[e.slot];
                let p = crate::uniform_changes::plan(
                    e,
                    team,
                    uniform,
                    &raw_team,
                    &m.raw.lock().unwrap(),
                    address,
                )?;
                Ok((m.clone(), m.owner.clone(), p))
            },
            || anyhow::bail!("Must not refresh"),
        );
        assert!(result.is_err());
        assert_eq!(m.writes.load(Ordering::SeqCst), before);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn uniform_committed_refresh_failure_never_replays_or_silently_rolls_back() {
        let (s, m, input, p, mut next, root) = uniform_fixture();
        next.native
            .as_mut()
            .unwrap()
            .teams
            .as_mut()
            .unwrap()
            .uniforms[0]
            .hash = "f".repeat(64);
        let error = s
            .save_uniform_using(
                &root,
                TAB_A,
                &input,
                |_, _, _, _| Ok((m.clone(), m.owner.clone(), p.clone())),
                || Ok(next),
            )
            .unwrap_err();
        assert!(error.to_string().contains("saved and verified"));
        assert_eq!(*m.raw.lock().unwrap(), p.after);
        assert!(!crate::uniform_transaction::pending(&root).unwrap());
        let tabs = s.tabs.lock().unwrap();
        assert!(tabs.get(TAB_A).is_err());
        assert!(tabs.get(TAB_B).is_ok());
        drop(tabs);
        std::fs::remove_dir_all(root).unwrap();
    }
    struct FakeV2(Arc<std::sync::atomic::AtomicUsize>);
    /// Records which Version 2 saves were routed; never touches the legacy path.
    static V2_SAVES: std::sync::Mutex<Vec<crate::roster_v2_write::Kind>> =
        std::sync::Mutex::new(Vec::new());
    struct FakeV2Capture(Value);
    impl crate::roster_v2_access::Captured for FakeV2Capture {
        fn document(&self) -> &Value {
            &self.0
        }
        fn detail(&mut self, _: usize) -> Result<Value> {
            anyhow::bail!("Unused")
        }
        fn save(&mut self, request: &crate::roster_v2_access::Save<'_>) -> Result<Value> {
            V2_SAVES.lock().unwrap().push(request.kind);
            self.0["players"][0]["name"] = json!("V2 Saved");
            Ok(json!({"ok":true,"readBackVerified":true,"kind":request.kind}))
        }
    }
    impl crate::roster_v2_access::Engine for FakeV2 {
        fn capture(
            &self,
            _: &crate::roster_v2_access::Binding,
            stop: Arc<AtomicBool>,
        ) -> Result<Box<dyn crate::roster_v2_access::Captured>> {
            ensure!(!stop.load(Ordering::Acquire), "Cancelled");
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(FakeV2Capture(
                json!({"status":"ready","readOnly":true,"writes":0,"writeAvailable":false,
                "players":[{"source":"live-roster","recordIndex":0,"name":"V2 Player","readOnly":true,"writeAvailable":false}],
                "teams":[{"source":"live-team","recordIndex":0,"name":"V2 Team","readOnly":true,"writeAvailable":false}]}),
            )))
        }
    }
    fn v2_binding() -> crate::roster_v2_access::Binding {
        crate::roster_v2_access::Binding::from_launched(
            session_safety::Identity {
                pid: 11,
                started: "100".into(),
                path: r"C:\Fixture\NBA2K27.exe".into(),
            },
            session_safety::Identity {
                pid: 22,
                started: "200".into(),
                path: r"C:\Portable\2k Launcher.exe".into(),
            },
            PathBuf::from(r"C:\Fixture"),
            "a".repeat(48),
        )
        .unwrap()
    }
    #[test]
    fn version_two_reads_and_saves_route_only_to_the_owned_v2_backend() {
        use std::sync::atomic::AtomicUsize;
        let (mut s, m, input, root) = save_fixture();
        let legacy = Arc::new(AtomicUsize::new(0));
        let v2 = Arc::new(AtomicUsize::new(0));
        let observed = legacy.clone();
        s.engine = Arc::new(move |_, _| {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(json!({"status":"ready","players":[],"teams":[],"readOnly":true,"writes":0}).into())
        });
        s.v2 = crate::roster_v2_access::Service::with_engine(Arc::new(FakeV2(v2.clone())));
        let game = Path::new(r"C:\Fixture");
        let view = s
            .route(
                "rosterAutoAttach",
                game,
                TAB_A,
                || Ok(true),
                || Ok(v2_binding()),
            )
            .unwrap();
        assert_eq!(view["status"], "ready");
        assert_eq!(view["players"][0]["name"], "V2 Player");
        assert_eq!(view["teams"][0]["name"], "V2 Team");
        assert_eq!(
            (v2.load(Ordering::SeqCst), legacy.load(Ordering::SeqCst)),
            (1, 0)
        );
        // Status polls need neither a build check nor a launch binding.
        let status = s
            .route(
                "playerResearchStatus",
                game,
                TAB_A,
                || -> Result<bool> { panic!("status must not inspect the build") },
                || -> Result<crate::roster_v2_access::Binding> {
                    panic!("status needs no binding")
                },
            )
            .unwrap();
        assert_eq!(status["players"][0]["name"], "V2 Player");
        // The tab's former September-build baseline can never be saved now.
        for message in [s
            .save_using(&root, TAB_A, &input, |_, _| Ok(m.clone()))
            .unwrap_err()
            .to_string()]
        {
            assert!(message.contains("Version 2"), "{message}");
            assert!(message.contains("no game write was attempted"), "{message}");
        }
        let copied_rows = s.player_dna(TAB_A, &json!({"action":"players"})).unwrap();
        assert_eq!(copied_rows[0]["name"], ""); // fixture has no name fields
        assert_eq!(copied_rows[0]["recordIndex"], 0);
        assert_eq!(v2.load(Ordering::SeqCst), 1, "DNA listing cannot recapture");
        assert_eq!(m.writes.load(Ordering::SeqCst), 0);
        assert!(s.tabs.lock().unwrap().get(TAB_A).is_err());
        // Public saves for a Version 2 tab go to the V2 transaction only.
        V2_SAVES.lock().unwrap().clear();
        let saved = s.save_player(&root, game, TAB_A, &input).unwrap();
        assert_eq!(saved["report"]["readBackVerified"], true);
        assert_eq!(saved["playerResearch"]["players"][0]["name"], "V2 Saved");
        s.save_team(&root, game, TAB_A, &json!({})).unwrap();
        s.save_uniform(&root, game, TAB_A, &json!({})).unwrap();
        {
            use crate::roster_v2_write::Kind;
            assert_eq!(
                *V2_SAVES.lock().unwrap(),
                [Kind::Player, Kind::Team, Kind::Uniform]
            );
        }
        assert_eq!(m.writes.load(Ordering::SeqCst), 0);
        assert_eq!(
            (v2.load(Ordering::SeqCst), legacy.load(Ordering::SeqCst)),
            (1, 0)
        );
        // Without an owned launch binding nothing is read, and TAB_B loses its
        // stale legacy baseline instead of keeping a save path open.
        let waiting = s
            .route(
                "rosterAutoAttach",
                game,
                TAB_B,
                || Ok(true),
                || anyhow::bail!("Launch through this prototype first"),
            )
            .unwrap();
        assert_eq!(waiting["status"], "waiting");
        assert_eq!(waiting["writeAvailable"], false);
        assert!(
            waiting["message"]
                .as_str()
                .unwrap()
                .contains("launched by this launcher")
        );
        let error = s
            .route(
                "scanPlayerResearch",
                game,
                TAB_B,
                || Ok(true),
                || anyhow::bail!("Launch through this prototype first"),
            )
            .unwrap();
        assert_eq!(error["status"], "error");
        assert!(s.tabs.lock().unwrap().get(TAB_B).is_err());
        assert!(
            s.save_using(&root, TAB_B, &input, |_, _| Ok(m.clone()))
                .is_err()
        );
        assert_eq!(m.writes.load(Ordering::SeqCst), 0);
        assert_eq!(
            (v2.load(Ordering::SeqCst), legacy.load(Ordering::SeqCst)),
            (1, 0)
        );
        assert!(
            s.route("savePlayer", game, TAB_A, || Ok(true), || Ok(v2_binding()))
                .is_err()
        );
        assert!(
            s.route(
                "rosterAutoAttach",
                game,
                "invalid",
                || Ok(true),
                || Ok(v2_binding())
            )
            .is_err()
        );
        clean_save_fixture(&root);
    }
    #[test]
    fn september_and_unverified_builds_keep_the_legacy_path() {
        use std::sync::atomic::AtomicUsize;
        let legacy = Arc::new(AtomicUsize::new(0));
        let v2 = Arc::new(AtomicUsize::new(0));
        let observed = legacy.clone();
        let mut s =
            Service {
                engine: Arc::new(move |_, _| {
                    observed.fetch_add(1, Ordering::SeqCst);
                    Ok(json!({"status":"ready","players":[],"teams":[],"readOnly":true,"writes":0})
                    .into())
                }),
                ..Service::new()
            };
        s.v2 = crate::roster_v2_access::Service::with_engine(Arc::new(FakeV2(v2.clone())));
        let game = Path::new("synthetic-only");
        let unused =
            || -> Result<crate::roster_v2_access::Binding> { panic!("legacy needs no binding") };
        s.route(
            "rosterAutoAttach",
            game,
            TAB_A,
            || Ok(true),
            || Ok(v2_binding()),
        )
        .unwrap();
        assert!(s.v2.owns_tab(TAB_A));
        // Same tab after the game changed back to the September build.
        let view = s
            .route("scanPlayerResearch", game, TAB_A, || Ok(false), unused)
            .unwrap();
        assert_eq!(view["status"], "ready");
        assert!(!s.v2.owns_tab(TAB_A));
        assert!(s.tabs.lock().unwrap().get(TAB_A).is_ok());
        // An unreadable or unsupported executable never selects Version 2.
        s.route(
            "scanPlayerResearch",
            game,
            TAB_B,
            || anyhow::bail!("Unsupported game build"),
            unused,
        )
        .unwrap();
        assert_eq!(
            (v2.load(Ordering::SeqCst), legacy.load(Ordering::SeqCst)),
            (1, 2)
        );
        s.cancel();
        assert!(s.v2.status(TAB_A).is_err());
    }
    #[test]
    fn uniform_invalid_tab_closing_and_recovery_refuse_the_write_lease() {
        let (s, _, input, _, _, root) = uniform_fixture();
        let calls = Cell::new(0);
        let call = |tab| {
            s.save_uniform_using::<FakeUniform>(
                &root,
                tab,
                &input,
                |_, _, _, _| {
                    calls.set(calls.get() + 1);
                    anyhow::bail!("Unexpected lease")
                },
                || anyhow::bail!("Unexpected refresh"),
            )
        };
        assert!(call("invalid").is_err());
        std::fs::write(crate::uniform_transaction::path(&root), b"unknown").unwrap();
        assert!(call(TAB_A).is_err());
        s.stop.store(true, Ordering::Release);
        assert!(call(TAB_A).is_err());
        assert_eq!(calls.get(), 0);
        std::fs::remove_dir_all(root).unwrap();
    }
}
