//! Offline, owned-byte season graph model. No process access, native calls, or writer.
//!
//! Plans and exact paused-prefix classification are diagnostic data, never permission
//! to execute, replay, undo, or infer that a native process is quiescent. A future
//! adapter/executor still needs separately proved identity, ownership and lifecycle.

use serde_json::Value;

pub(crate) const PLAYER_BYTES: usize = 0x4f8;
pub(crate) const STAT_BYTES: usize = 64;
pub(crate) const MANAGER_BYTES: usize = 56;
pub(crate) const ALL_SLOTS: usize = 33;
pub(crate) const REGULAR_SLOTS: usize = 25;
const EMPTY: u16 = u16::MAX;
const REF_MASK: u32 = 0x001f_e000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PoolKind {
    Player,
    Staff,
    Team,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TeamDirectory {
    pub base: u64,
    pub count: usize,
}
impl TeamDirectory {
    fn valid(self) -> bool {
        self.base >= 0x10000
            && self.base % 8 == 0
            && (1..=1024).contains(&self.count)
            && self
                .base
                .checked_add(self.count as u64 * 0x1550)
                .is_some_and(|end| end <= 0x0000_7fff_ffff_ffff)
    }
    fn contains(self, pointer: u64) -> bool {
        pointer >= self.base
            && (pointer - self.base) % 0x1550 == 0
            && (pointer - self.base) / 0x1550 < self.count as u64
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Snapshot {
    pub players: Vec<[u8; PLAYER_BYTES]>,
    pub rows: Vec<[u8; STAT_BYTES]>,
    pub manager: [u8; MANAGER_BYTES],
    pub pool: PoolKind,
    /// Native singleton directory COUNT, not a pool-kind tag.
    pub manager_count: usize,
    pub teams: TeamDirectory,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    Shape,
    Pool,
    TeamDirectory,
    PrivateHistory(usize),
    Reference { player: usize, slot: usize },
    CountOverflow(usize),
    CountMismatch(usize),
    FreeChain,
    NoncanonicalFree(usize),
    IncompleteFreeChain,
    Player,
    Slot,
    Calendar,
    EmptyHistory,
    FullHistory,
    YearFloor,
    NoFreeRow,
    TeamPointer,
    SharedRemoval,
    NotShared,
    Changes,
    UnprovedCopyBit(usize),
    UnknownPrefix,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Occurrence {
    pub player: usize,
    pub slot: usize,
}

#[derive(Debug)]
pub(crate) struct Validated {
    references: Vec<Vec<Occurrence>>,
    free: Vec<usize>,
}
impl Validated {
    pub fn references(&self, row: usize) -> Option<&[Occurrence]> {
        self.references.get(row).map(Vec::as_slice)
    }
    pub fn free_chain(&self) -> &[usize] {
        &self.free
    }
}

fn word(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
}
fn dword(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}
fn pointer(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}
fn put_word(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
fn put_dword(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn refcount(row: &[u8; STAT_BYTES]) -> usize {
    ((dword(row, 60) & REF_MASK) >> 13) as usize
}
fn set_refcount(row: &mut [u8; STAT_BYTES], count: usize) {
    let value = (dword(row, 60) & !REF_MASK) | ((count as u32) << 13);
    put_dword(row, 60, value);
}
fn slot(player: &[u8; PLAYER_BYTES], index: usize) -> u16 {
    word(player, 0x130 + index * 2)
}
fn set_slot(player: &mut [u8; PLAYER_BYTES], index: usize, row: u16) {
    put_word(player, 0x130 + index * 2, row);
}

/// Exact known native clear transform; it preserves destination-local high bits.
fn cleared(row: &[u8; STAT_BYTES], next: u16) -> [u8; STAT_BYTES] {
    let mut out = *row;
    out[..48].fill(0);
    put_dword(&mut out, 48, dword(row, 48) & 0xf000_0000);
    put_dword(&mut out, 52, 0);
    put_dword(&mut out, 56, dword(row, 56) & 0xf800_0000);
    put_dword(&mut out, 60, dword(row, 60) & 0xf000_0000);
    put_dword(&mut out, 16, (next as u32) << 16);
    out
}

impl Snapshot {
    /// Exact native extents; deliberately rejects the historical 64-byte manager
    /// capture, whose last eight bytes may actually be the first season row.
    pub fn from_bytes(
        players: &[u8],
        rows: &[u8],
        manager: &[u8],
        pool: PoolKind,
        manager_count: usize,
        teams: TeamDirectory,
    ) -> Result<Self, Refusal> {
        if players.is_empty()
            || players.len() % PLAYER_BYTES != 0
            || players.len() / PLAYER_BYTES > 8192
            || rows.is_empty()
            || rows.len() % STAT_BYTES != 0
            || rows.len() / STAT_BYTES >= 32768
            || manager.len() != MANAGER_BYTES
        {
            return Err(Refusal::Shape);
        }
        let out = Self {
            players: players
                .chunks_exact(PLAYER_BYTES)
                .map(|p| p.try_into().unwrap())
                .collect(),
            rows: rows
                .chunks_exact(STAT_BYTES)
                .map(|p| p.try_into().unwrap())
                .collect(),
            manager: manager.try_into().unwrap(),
            pool,
            manager_count,
            teams,
        };
        out.validate()?;
        Ok(out)
    }

    pub fn validate(&self) -> Result<Validated, Refusal> {
        if !(1..=8192).contains(&self.players.len()) || !(1..32768).contains(&self.rows.len()) {
            return Err(Refusal::Shape);
        }
        if self.pool != PoolKind::Player || self.manager_count != 1 {
            return Err(Refusal::Pool);
        }
        if !self.teams.valid() {
            return Err(Refusal::TeamDirectory);
        }
        let mut references = vec![Vec::new(); self.rows.len()];
        for (p, player) in self.players.iter().enumerate() {
            if pointer(player, 0xe0) != 0 {
                return Err(Refusal::PrivateHistory(p));
            }
            for s in 0..ALL_SLOTS {
                let r = slot(player, s);
                if r == EMPTY {
                    continue;
                }
                if r as usize >= self.rows.len() {
                    return Err(Refusal::Reference { player: p, slot: s });
                }
                let links = &mut references[r as usize];
                links.push(Occurrence { player: p, slot: s });
                if links.len() > 255 {
                    return Err(Refusal::CountOverflow(r as usize));
                }
            }
        }
        for (r, links) in references.iter().enumerate() {
            if refcount(&self.rows[r]) != links.len() {
                return Err(Refusal::CountMismatch(r));
            }
        }
        let mut seen = vec![false; self.rows.len()];
        let mut free = Vec::new();
        let mut r = word(&self.manager, 0);
        while r != EMPTY {
            let index = r as usize;
            if index >= self.rows.len() || seen[index] || !references[index].is_empty() {
                return Err(Refusal::FreeChain);
            }
            seen[index] = true;
            let next = word(&self.rows[index], 18);
            if self.rows[index] != cleared(&self.rows[index], next) {
                return Err(Refusal::NoncanonicalFree(index));
            }
            free.push(index);
            r = next;
        }
        if references
            .iter()
            .enumerate()
            .any(|(r, links)| links.is_empty() && !seen[r])
        {
            return Err(Refusal::IncompleteFreeChain);
        }
        Ok(Validated { references, free })
    }

    fn oldest(&self, player: usize) -> Result<(usize, usize), Refusal> {
        let p = self.players.get(player).ok_or(Refusal::Player)?;
        let mut origin = None;
        let mut last = None;
        for s in 0..REGULAR_SLOTS {
            let r = slot(p, s);
            if r == EMPTY {
                continue;
            }
            let year = 1900usize + self.rows[r as usize][17] as usize;
            if origin.is_some_and(|y| y != year + s) {
                return Err(Refusal::Calendar);
            }
            origin = Some(year + s);
            last = Some((s, r as usize));
        }
        last.ok_or(Refusal::EmptyHistory)
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Operation {
    Import {
        player: usize,
        choices: Vec<bulk::Choice>,
    },
    AddOlder {
        player: usize,
    },
    RemoveOldest {
        player: usize,
    },
    NarrowSharedCopy {
        player: usize,
        slot: usize,
        changes: Value,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    ReserveHead,
    InitializeRow,
    UpdateRow,
    PublishDirectory,
    PublishSlot,
    DecrementSourceCount,
    UnlinkSlot,
    ClearRow,
    ReleaseHead,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    Manager,
    Player(usize),
    Row(usize),
}
#[derive(Clone, Debug)]
pub(crate) struct Step {
    pub(crate) phase: Phase,
    pub(crate) target: Target,
    pub(crate) offset: usize,
    pub(crate) before: Vec<u8>,
    pub(crate) after: Vec<u8>,
}

#[derive(Debug)]
pub(crate) enum Outcome {
    Planned(Plan),
    Unchanged,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModelState {
    Before,
    After,
    KnownPaused { completed_steps: usize },
    Unknown,
}
#[derive(Debug)]
pub(crate) struct Plan {
    before: Snapshot,
    after: Snapshot,
    steps: Vec<Step>,
}

fn target(snapshot: &Snapshot, where_: Target) -> &[u8] {
    match where_ {
        Target::Manager => &snapshot.manager,
        Target::Player(p) => &snapshot.players[p],
        Target::Row(r) => &snapshot.rows[r],
    }
}
fn target_mut(snapshot: &mut Snapshot, where_: Target) -> &mut [u8] {
    match where_ {
        Target::Manager => &mut snapshot.manager,
        Target::Player(p) => &mut snapshot.players[p],
        Target::Row(r) => &mut snapshot.rows[r],
    }
}
impl Plan {
    fn new(before: &Snapshot) -> Self {
        Self {
            before: before.clone(),
            after: before.clone(),
            steps: Vec::new(),
        }
    }
    fn push(&mut self, phase: Phase, where_: Target, offset: usize, value: &[u8]) {
        let before = target(&self.after, where_)[offset..offset + value.len()].to_vec();
        self.steps.push(Step {
            phase,
            target: where_,
            offset,
            before,
            after: value.to_vec(),
        });
        target_mut(&mut self.after, where_)[offset..offset + value.len()].copy_from_slice(value);
    }
    pub fn before(&self) -> &Snapshot {
        &self.before
    }
    pub fn after(&self) -> &Snapshot {
        &self.after
    }
    /// Immutable logical spans; native adapter still needs its own retained freeze.
    pub(crate) fn steps(&self) -> &[Step] {
        &self.steps
    }
    pub fn phases(&self) -> Vec<Phase> {
        self.steps.iter().map(|s| s.phase).collect()
    }
    /// Synthesizes owned model bytes only; not a paused native state witness.
    pub fn expected_prefix(&self, completed: usize) -> Option<Snapshot> {
        if completed > self.steps.len() {
            return None;
        }
        let mut snapshot = self.before.clone();
        for s in &self.steps[..completed] {
            let bytes =
                &mut target_mut(&mut snapshot, s.target)[s.offset..s.offset + s.before.len()];
            if bytes != s.before.as_slice() {
                return None;
            }
            bytes.copy_from_slice(&s.after);
        }
        Some(snapshot)
    }
    /// Byte equivalence does not prove which effects ran or authorize replay.
    pub fn inspect(&self, observed: &Snapshot) -> ModelState {
        if observed == &self.before {
            return ModelState::Before;
        }
        if observed == &self.after {
            return ModelState::After;
        }
        let mut snapshot = self.before.clone();
        for (i, s) in self.steps.iter().enumerate() {
            target_mut(&mut snapshot, s.target)[s.offset..s.offset + s.after.len()]
                .copy_from_slice(&s.after);
            if observed == &snapshot {
                return ModelState::KnownPaused {
                    completed_steps: i + 1,
                };
            }
        }
        ModelState::Unknown
    }
    /// Dependency-order explanation ONLY. No mutation or native cleanup authority.
    pub fn reverse_preview(&self, observed: &Snapshot) -> Result<Vec<Phase>, Refusal> {
        let count = match self.inspect(observed) {
            ModelState::Before => 0,
            ModelState::After => self.steps.len(),
            ModelState::KnownPaused { completed_steps } => completed_steps,
            ModelState::Unknown => return Err(Refusal::UnknownPrefix),
        };
        Ok(self.steps[..count].iter().rev().map(|s| s.phase).collect())
    }
}

pub(crate) fn plan(snapshot: &Snapshot, operation: Operation) -> Result<Outcome, Refusal> {
    if let Operation::Import { player, choices } = &operation {
        return bulk::plan(snapshot, *player, choices);
    }
    let valid = snapshot.validate()?;
    let mut out = Plan::new(snapshot);
    match operation {
        Operation::Import { .. } => unreachable!("bulk handled before simple planning"),
        Operation::AddOlder { player } => {
            let (s, anchor) = snapshot.oldest(player)?;
            if s + 1 >= REGULAR_SLOTS {
                return Err(Refusal::FullHistory);
            }
            if snapshot.rows[anchor][17] == 0 {
                return Err(Refusal::YearFloor);
            }
            let team = pointer(&snapshot.rows[anchor], 0);
            if !snapshot.teams.contains(team) {
                return Err(Refusal::TeamPointer);
            }
            let &new = valid.free.first().ok_or(Refusal::NoFreeRow)?;
            let next = word(&snapshot.rows[new], 18);
            let mut row = cleared(&snapshot.rows[new], EMPTY);
            row[..8].copy_from_slice(&team.to_le_bytes());
            row[17] = snapshot.rows[anchor][17] - 1;
            set_refcount(&mut row, 1);
            out.push(Phase::ReserveHead, Target::Manager, 0, &next.to_le_bytes());
            out.push(Phase::InitializeRow, Target::Row(new), 0, &row);
            out.push(
                Phase::PublishSlot,
                Target::Player(player),
                0x130 + (s + 1) * 2,
                &(new as u16).to_le_bytes(),
            );
        }
        Operation::RemoveOldest { player } => {
            let (s, old) = snapshot.oldest(player)?;
            if valid.references[old].len() != 1 {
                return Err(Refusal::SharedRemoval);
            }
            let row = cleared(&snapshot.rows[old], word(&snapshot.manager, 0));
            out.push(
                Phase::UnlinkSlot,
                Target::Player(player),
                0x130 + s * 2,
                &EMPTY.to_le_bytes(),
            );
            out.push(Phase::ClearRow, Target::Row(old), 0, &row);
            out.push(
                Phase::ReleaseHead,
                Target::Manager,
                0,
                &(old as u16).to_le_bytes(),
            );
        }
        Operation::NarrowSharedCopy {
            player,
            slot: s,
            changes,
        } => {
            snapshot.oldest(player)?;
            if s >= REGULAR_SLOTS {
                return Err(Refusal::Slot);
            }
            let old = slot(&snapshot.players[player], s);
            if old == EMPTY {
                return Err(Refusal::Slot);
            }
            let old = old as usize;
            if valid.references[old].len() < 2 {
                return Err(Refusal::NotShared);
            }
            let changes = changes.as_object().ok_or(Refusal::Changes)?;
            if changes.len() > 31
                || changes.keys().any(|k| match k.strip_prefix("award:") {
                    Some(a) => !crate::roster_v2_services::AWARDS.iter().any(|f| f.0 == a),
                    None => !crate::roster_v2_services::SEASON_FIELDS
                        .iter()
                        .any(|f| f.0 == k),
                })
            {
                return Err(Refusal::Changes);
            }
            let source = &snapshot.rows[old];
            let (bytes, _) = crate::roster_v2_services::plan_season_record(
                source,
                &Value::Object(changes.clone()),
            )
            .map_err(|_| Refusal::Changes)?;
            crate::roster_v2_services::validate_season_patch(source, &bytes)
                .map_err(|_| Refusal::Changes)?;
            if bytes == source.as_slice() {
                return Ok(Outcome::Unchanged);
            }
            if !snapshot.teams.contains(pointer(source, 0))
                || (pointer(source, 8) != 0 && !snapshot.teams.contains(pointer(source, 8)))
            {
                return Err(Refusal::TeamPointer);
            }
            let &new = valid.free.first().ok_or(Refusal::NoFreeRow)?;
            let mut row: [u8; STAT_BYTES] = bytes.try_into().map_err(|_| Refusal::Shape)?;
            put_word(&mut row, 18, EMPTY);
            set_refcount(&mut row, 1);
            let mut masks = crate::roster_v2_services::season_masks();
            masks[..16].fill(255);
            masks[17..20].fill(255);
            let count_mask = REF_MASK.to_le_bytes();
            for i in 0..4 {
                masks[60 + i] |= count_mask[i];
            }
            for i in 0..STAT_BYTES {
                if (row[i] ^ snapshot.rows[new][i]) & !masks[i] != 0 {
                    return Err(Refusal::UnprovedCopyBit(i));
                }
            }
            let next = word(&snapshot.rows[new], 18);
            let mut old_after = *source;
            set_refcount(&mut old_after, valid.references[old].len() - 1);
            out.push(Phase::ReserveHead, Target::Manager, 0, &next.to_le_bytes());
            out.push(Phase::InitializeRow, Target::Row(new), 0, &row);
            out.push(
                Phase::PublishSlot,
                Target::Player(player),
                0x130 + s * 2,
                &(new as u16).to_le_bytes(),
            );
            out.push(
                Phase::DecrementSourceCount,
                Target::Row(old),
                60,
                &old_after[60..64],
            );
        }
    }
    out.after.validate()?;
    Ok(Outcome::Planned(out))
}

#[cfg(test)]
#[path = "season_structure_v2_tests.rs"]
mod tests;

#[path = "season_structure_v2_bulk.rs"]
pub(crate) mod bulk;
