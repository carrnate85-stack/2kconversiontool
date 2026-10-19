//! Explicit team Save & verify with durable intent and guarded rollback.
//! Access is a retained, native-owned lease; journals never replay memory.
use crate::{
    backend, roster_snapshot,
    session_safety::{self, Identity},
    team_changes::{
        self, ARENA_ID, ARENA_POINTER, ARENA_STRIDE, Area, Patch, Plan, Raw, TEAM_ID, TEAM_STRIDE,
    },
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
};
const JOURNAL_LIMIT: u64 = 524288;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Owner {
    pub game: Identity,
    pub image: u64,
    pub root: u64,
    pub owner: u64,
    pub allocation_base: u64,
    pub allocation_len: u64,
    pub player_table: u64,
    pub player_count: usize,
    pub team_table: u64,
    pub team_count: usize,
    pub arena_table: u64,
    pub arena_count: usize,
    pub uniform_header: Vec<u8>,
    pub index: usize,
    pub team_id: u16,
    pub donor: usize,
    pub donor_hash: String,
    pub arena_ids_hash: String,
    pub other_references_hash: String,
    pub clone_index: Option<usize>,
    pub clone_id: Option<u16>,
    pub serializer: String,
}
impl Owner {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.game.pid > 0
                && self.game.started.parse::<u64>().is_ok_and(|n| n > 0)
                && Path::new(&self.game.path).is_absolute()
                && Path::new(&self.game.path)
                    .file_name()
                    .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case("NBA2K27.exe")),
            "Unsupported team transaction process identity"
        );
        for p in [
            self.image,
            self.root,
            self.owner,
            self.allocation_base,
            self.player_table,
            self.team_table,
            self.arena_table,
        ] {
            roster_snapshot::checked_pointer(p)?;
        }
        let a = roster_snapshot::Allocation {
            base: self.allocation_base,
            len: self.allocation_len,
        };
        ensure!(
            a.valid()
                && self.player_table == a.base + 0x680
                && (100..=8192).contains(&self.player_count)
                && (1..=1024).contains(&self.team_count)
                && (1..=2048).contains(&self.arena_count)
                && self.index < self.team_count
                && self.donor < self.arena_count,
            "Unsupported team transaction directories"
        );
        let mut spans = Vec::new();
        for (p, count, stride) in [
            (
                self.player_table,
                self.player_count,
                roster_snapshot::PLAYER_STRIDE,
            ),
            (self.team_table, self.team_count, TEAM_STRIDE),
            (self.arena_table, self.arena_count, ARENA_STRIDE),
        ] {
            let end = p
                .checked_add((count * stride) as u64)
                .context("Team span overflow")?;
            ensure!(
                p >= a.base + 0x680 && end <= a.base + a.len,
                "Team directory is outside the owned allocation"
            );
            spans.push((p, end));
        }
        ensure!(
            self.uniform_header.len() == 16,
            "Invalid uniform directory ownership"
        );
        let uniform_count = team_changes::uint(&self.uniform_header, 0, 8)?;
        let uniform_table = team_changes::uint(&self.uniform_header, 8, 8)?;
        if uniform_count != 0 {
            ensure!(
                uniform_count <= 8192
                    && uniform_table >= a.base + 0x680
                    && uniform_table.is_multiple_of(8),
                "Unsupported uniform directory"
            );
            let end = uniform_table
                .checked_add(uniform_count * 0x170)
                .context("Uniform directory overflow")?;
            ensure!(
                end <= a.base + a.len,
                "Uniform directory exceeds allocation"
            );
            spans.push((uniform_table, end));
        }
        spans.sort_unstable();
        ensure!(
            spans.windows(2).all(|w| w[0].1 <= w[1].0),
            "Team directories overlap"
        );
        for s in [
            &self.donor_hash,
            &self.arena_ids_hash,
            &self.other_references_hash,
        ] {
            ensure!(
                s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()),
                "Invalid team ownership digest"
            );
        }
        ensure!(
            self.serializer == roster_snapshot::serializer_fingerprint(),
            "Unsupported team serializer"
        );
        match (self.clone_index, self.clone_id) {
            (None, None) => {}
            (Some(i), Some(id)) => ensure!(
                i < self.arena_count && i != self.donor && (996..=1045).contains(&id),
                "Invalid reserved clone slot"
            ),
            _ => anyhow::bail!("Incomplete clone ownership"),
        }
        Ok(())
    }
}
pub(crate) fn references_hash(references: &[u64], index: usize) -> String {
    team_changes::hash(
        &references
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != index)
            .flat_map(|(_, p)| p.to_le_bytes())
            .collect::<Vec<_>>(),
    )
}
pub(crate) trait Access {
    fn owner(&self) -> Result<Owner>;
    fn capture(&self) -> Result<Raw>;
    fn write(&self, area: Area, offset: usize, bytes: &[u8]) -> Result<()>;
    /// Rollback methods above remain usable after cancellation on this lease.
    fn checkpoint(&self) -> Result<()>;
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Intent {
    pub schema: String,
    pub id: String,
    pub owner: Owner,
    pub patches: Vec<Patch>,
    pub pending: bool,
    pub result: String,
}
fn validate_patches(owner: &Owner, patches: &[Patch]) -> Result<()> {
    owner.validate()?;
    ensure!(patches.len() <= 48, "Too many team patches");
    let mut masks = vec![0; TEAM_STRIDE];
    for (at, len) in [
        (0x2FA, 48),
        (0x32C, 36),
        (0x350, 14),
        (0x35E, 14),
        (0x1AE, 72),
        (ARENA_POINTER, 8),
    ] {
        masks[at..at + len].fill(255);
    }
    masks[0x123F] = 0x7C;
    masks[0x154A] = 0xF8;
    masks[0x154B] = 3;
    let mut seen = HashSet::new();
    let mut team_started = false;
    let mut pointer_seen = false;
    for p in patches {
        let limit = if p.area == Area::Team {
            TEAM_STRIDE
        } else {
            ARENA_STRIDE
        };
        ensure!(
            !p.before.is_empty()
                && p.before.len() <= 128
                && p.before.len() == p.after.len()
                && p.after.len() == p.mask.len()
                && p.before != p.after
                && p.offset
                    .checked_add(p.after.len())
                    .is_some_and(|end| end <= limit),
            "Invalid team patch span"
        );
        ensure!(!pointer_seen, "Team arena link must be the final patch");
        if p.area == Area::Clone {
            ensure!(
                !team_started && owner.clone_index.is_some(),
                "Arena clone must precede team edits"
            );
        } else {
            team_started = true;
        }
        for i in 0..p.after.len() {
            let at = p.offset + i;
            let allowed = if p.area == Area::Team {
                masks[at]
            } else if (ARENA_ID..ARENA_ID + 2).contains(&at) {
                0
            } else {
                255
            };
            ensure!(
                seen.insert((p.area, at))
                    && p.mask[i] & !allowed == 0
                    && (p.before[i] ^ p.after[i]) & !p.mask[i] == 0,
                "Unowned, overlapping or out-of-mask team patch"
            );
            if p.area == Area::Team && (ARENA_POINTER..ARENA_POINTER + 8).contains(&at) {
                ensure!(
                    p.offset == ARENA_POINTER && p.after.len() == 8,
                    "Partial arena links are not supported"
                );
                pointer_seen = true;
            }
        }
    }
    Ok(())
}
impl Intent {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == "nba2k27-team-write-intent-v1"
                && self.id.len() == 48
                && self.id.bytes().all(|b| b.is_ascii_hexdigit())
                && self.result.len() <= 512,
            "Unsupported team journal; left untouched"
        );
        validate_patches(&self.owner, &self.patches)
    }
}
pub(crate) fn path(root: &Path) -> PathBuf {
    root.join("Data/team-write-journal.json")
}
fn read_intent(p: &Path) -> Result<Intent> {
    let mut bytes = Vec::new();
    fs::File::open(p)?
        .take(JOURNAL_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= JOURNAL_LIMIT,
        "Team journal exceeds limits; left untouched"
    );
    let i: Intent = serde_json::from_slice(&bytes)?;
    i.validate()?;
    Ok(i)
}
pub(crate) fn pending(root: &Path) -> Result<bool> {
    let p = path(root);
    if !p.try_exists()? {
        return Ok(false);
    }
    Ok(read_intent(&p)?.pending)
}
pub(crate) fn save(root: &Path, i: &Intent) -> Result<()> {
    i.validate()?;
    ensure!(
        serde_json::to_vec_pretty(i)?.len() as u64 <= JOURNAL_LIMIT,
        "Team journal exceeds limits before write"
    );
    backend::atomic_json(&path(root), i)
}
pub(crate) fn resolve_ended(root: &Path, rows: &[session_safety::Process]) -> Result<()> {
    ensure!(
        !rows
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe")),
        "Close NBA 2K27 before resolving a team transaction"
    );
    let p = path(root);
    if p.try_exists()? {
        let mut i = read_intent(&p)?;
        if i.pending {
            i.pending = false;
            i.result = "game-ended; memory changes cannot be replayed".into();
            save(root, &i)?;
        }
    }
    Ok(())
}
fn check(m: &impl Access, owner: &Owner, expected: &Raw) -> Result<Raw> {
    ensure!(
        m.owner()? == *owner,
        "Team database, donor or reference ownership changed"
    );
    let raw = m.capture()?;
    raw.validate()?;
    ensure!(raw == *expected, "Team or reserved arena values changed");
    Ok(raw)
}
pub(crate) struct Commit {
    pub report: Value,
    pub read_back: Raw,
}
pub(crate) fn apply(
    m: &impl Access,
    owner: &Owner,
    plan: &Plan,
    mut persist: impl FnMut(&Intent) -> Result<()>,
) -> Result<Commit> {
    validate_patches(owner, &plan.patches)?;
    plan.before.validate()?;
    plan.after.validate()?;
    ensure!(
        owner.index == plan.index
            && owner.team_id == plan.team_id
            && owner.donor == plan.donor
            && owner.clone_index == plan.clone_index
            && team_changes::uint(&plan.before.team, TEAM_ID, 2)? == owner.team_id as u64
            && plan.expected(plan.patches.len())? == plan.after,
        "Team plan belongs to another record or is incomplete"
    );
    ensure!(
        plan.before.clone.is_empty() == owner.clone_index.is_none()
            && plan.after.clone.len() == plan.before.clone.len(),
        "Team clone shape differs"
    );
    if let Some(id) = owner.clone_id {
        ensure!(
            team_changes::uint(&plan.before.clone, ARENA_ID, 2)? == id as u64
                && team_changes::uint(&plan.after.clone, ARENA_ID, 2)? == id as u64,
            "Reserved arena ID changed"
        );
    }
    let target = owner.clone_index.unwrap_or(owner.donor);
    let expected_link = owner.arena_table + (target * ARENA_STRIDE) as u64;
    ensure!(
        team_changes::uint(&plan.after.team, ARENA_POINTER, 8)? == expected_link,
        "Unowned team arena target"
    );
    m.checkpoint()?;
    let original = check(m, owner, &plan.before)?;
    if plan.patches.is_empty() {
        return Ok(Commit {
            report: json!({"schema":"nba2k27-team-edit-v1","ok":true,"gameCodeHooks":0,"writes":0,"readBackVerified":true,"rollbackOnFailure":true,"noChanges":true,"recordIndex":owner.index,"teamId":owner.team_id,"arenaId":plan.arena_id}),
            read_back: original,
        });
    }
    let mut intent = Intent {
        schema: "nba2k27-team-write-intent-v1".into(),
        id: backend::unique_id(),
        owner: owner.clone(),
        patches: plan.patches.clone(),
        pending: true,
        result: "intent saved before write".into(),
    };
    intent.validate()?;
    persist(&intent)?;
    let mut attempted = 0;
    let outcome = (|| -> Result<Raw> {
        for (i, p) in plan.patches.iter().enumerate() {
            m.checkpoint()?;
            check(m, owner, &plan.expected(i)?)?;
            // Owner inspection can involve a large directory. Recheck closing
            // and deadline after that work, immediately before any new write.
            m.checkpoint()?;
            attempted = i + 1;
            m.write(p.area, p.offset, &p.after)?;
            check(m, owner, &plan.expected(i + 1)?)?;
        }
        let raw = check(m, owner, &plan.after)?;
        intent.pending = false;
        intent.result = "applied and read back verified".into();
        persist(&intent)?;
        Ok(raw)
    })();
    let read_back = match outcome {
        Ok(raw) => raw,
        Err(error) => {
            let rollback = (|| -> Result<()> {
                for i in (0..attempted).rev() {
                    ensure!(m.owner()? == *owner, "Team rollback owner changed");
                    let raw = m.capture()?;
                    let before = plan.expected(i)?;
                    let after = plan.expected(i + 1)?;
                    if raw == before {
                        continue;
                    }
                    ensure!(
                        raw == after,
                        "Foreign or partial team values prevent safe rollback"
                    );
                    let p = &plan.patches[i];
                    m.write(p.area, p.offset, &p.before)?;
                    check(m, owner, &before)?;
                }
                Ok(())
            })();
            intent.pending = rollback.is_err();
            intent.result = if rollback.is_ok() {
                "failed; original owned values verified"
            } else {
                "failed; recovery held for changed owner or foreign/partial values"
            }
            .into();
            let recorded = persist(&intent);
            anyhow::bail!(
                "{error:#}. {}{}",
                if rollback.is_ok() {
                    "All attempted team changes are verified rolled back."
                } else {
                    "Some changes may remain. Do not retry; close NBA 2K27 before clearing the recovery record."
                },
                if recorded.is_err() {
                    " Recovery record could not be completed; further writes must remain held."
                } else {
                    ""
                }
            );
        }
    };
    Ok(Commit {
        // WPF's success toast calls `writes` fields. Preserve its eight team
        // operations plus one optional clone, not the clone's low-level chunks.
        report: json!({"schema":"nba2k27-team-edit-v1","ok":true,"pid":owner.game.pid,"gameCodeHooks":0,"writes":8+usize::from(plan.clone_index.is_some()),"patchWrites":plan.patches.len(),"recordIndex":owner.index,"recordAddress":format!("0x{:X}",owner.team_table+(owner.index*TEAM_STRIDE)as u64),"teamId":owner.team_id,"arenaId":plan.arena_id,"readBackVerified":true,"rollbackOnFailure":true}),
        read_back,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::{
        cell::{Cell, RefCell},
        sync::atomic::{AtomicU64, Ordering},
    };
    pub fn fixture() -> (Plan, Owner) {
        let (p, d, r) = team_changes::tests::clone_plan();
        let base = d.team_table - 0x21000;
        let o = Owner {
            game: Identity {
                pid: 7,
                started: "42".into(),
                path: r"C:\QA\NBA2K27.exe".into(),
            },
            image: 0x140000000,
            root: base + 0x31000,
            owner: base + 0x32000,
            allocation_base: base,
            allocation_len: 0x800000,
            player_table: base + 0x680,
            player_count: 100,
            team_table: d.team_table,
            team_count: d.team_count,
            arena_table: d.arena_table,
            arena_count: d.arena_count,
            uniform_header: vec![0; 16],
            index: p.index,
            team_id: p.team_id,
            donor: p.donor,
            donor_hash: team_changes::hash(&r.arenas[p.donor]),
            arena_ids_hash: team_changes::hash(
                &r.arenas
                    .iter()
                    .flat_map(|r| r[ARENA_ID..ARENA_ID + 2].iter().copied())
                    .collect::<Vec<_>>(),
            ),
            other_references_hash: references_hash(&r.references, p.index),
            clone_index: p.clone_index,
            clone_id: Some(p.arena_id),
            serializer: roster_snapshot::serializer_fingerprint().into(),
        };
        (p, o)
    }
    struct Fake {
        owner: RefCell<Owner>,
        raw: RefCell<Raw>,
        writes: Cell<usize>,
        reads: Cell<usize>,
        fail: Option<usize>,
        fail_read: Option<usize>,
        after: bool,
        partial: bool,
        foreign: bool,
        changed_owner: bool,
        cancel: Option<usize>,
        cancel_read: Option<usize>,
    }
    impl Fake {
        fn new(p: &Plan, o: &Owner) -> Self {
            Self {
                owner: RefCell::new(o.clone()),
                raw: RefCell::new(p.before.clone()),
                writes: Cell::new(0),
                reads: Cell::new(0),
                fail: None,
                fail_read: None,
                after: false,
                partial: false,
                foreign: false,
                changed_owner: false,
                cancel: None,
                cancel_read: None,
            }
        }
    }
    impl Access for Fake {
        fn owner(&self) -> Result<Owner> {
            Ok(self.owner.borrow().clone())
        }
        fn capture(&self) -> Result<Raw> {
            let n = self.reads.get() + 1;
            self.reads.set(n);
            ensure!(self.fail_read != Some(n), "Synthetic read failure");
            Ok(self.raw.borrow().clone())
        }
        fn checkpoint(&self) -> Result<()> {
            ensure!(
                self.cancel.is_none_or(|n| self.writes.get() < n),
                "Synthetic cancellation"
            );
            ensure!(
                self.cancel_read.is_none_or(|n| self.reads.get() < n),
                "Synthetic close during owner inspection"
            );
            Ok(())
        }
        fn write(&self, a: Area, at: usize, bytes: &[u8]) -> Result<()> {
            let n = self.writes.get() + 1;
            self.writes.set(n);
            let fail = self.fail == Some(n);
            if fail && !self.after {
                anyhow::bail!("Synthetic write failure");
            }
            let mut raw = self.raw.borrow_mut();
            raw.bytes_mut(a)[at..at + bytes.len()].copy_from_slice(bytes);
            if fail && self.partial {
                raw.bytes_mut(a)[at] ^= 0x80;
            }
            if fail && self.foreign {
                raw.team[TEAM_ID] ^= 1;
            }
            if fail && self.changed_owner {
                self.owner.borrow_mut().other_references_hash = "f".repeat(64);
            }
            ensure!(!fail, "Synthetic reported failure after write");
            Ok(())
        }
    }
    #[test]
    fn every_write_failure_and_cancelled_prefix_rolls_back() {
        let (p, o) = fixture();
        for i in 1..=p.patches.len() {
            for after in [false, true] {
                let mut m = Fake::new(&p, &o);
                m.fail = Some(i);
                m.after = after;
                let mut records = vec![];
                assert!(
                    apply(&m, &o, &p, |j| {
                        records.push(j.clone());
                        Ok(())
                    })
                    .is_err()
                );
                assert_eq!(*m.raw.borrow(), p.before, "step {i} after {after}");
                assert!(!records.last().unwrap().pending);
            }
            let mut m = Fake::new(&p, &o);
            m.cancel = Some(i - 1);
            assert!(apply(&m, &o, &p, |_| Ok(())).is_err());
            assert_eq!(*m.raw.borrow(), p.before);
        }
    }
    #[test]
    fn readback_failures_persistence_and_foreign_states_are_safe() {
        let (p, o) = fixture();
        for i in 1..=2 * p.patches.len() + 2 {
            let mut m = Fake::new(&p, &o);
            m.fail_read = Some(i);
            assert!(apply(&m, &o, &p, |_| Ok(())).is_err());
            assert_eq!(*m.raw.borrow(), p.before, "read {i}");
        }
        let m = Fake::new(&p, &o);
        assert!(apply(&m, &o, &p, |_| anyhow::bail!("Disk full")).is_err());
        assert_eq!(m.writes.get(), 0);
        let m = Fake::new(&p, &o);
        let mut calls = 0;
        assert!(
            apply(&m, &o, &p, |_| {
                calls += 1;
                ensure!(calls != 2, "Completion disk error");
                Ok(())
            })
            .is_err()
        );
        assert_eq!(*m.raw.borrow(), p.before);
        for variant in 0..3 {
            let mut m = Fake::new(&p, &o);
            m.fail = Some(2);
            m.after = true;
            m.partial = variant == 0;
            m.foreign = variant == 1;
            m.changed_owner = variant == 2;
            let mut pending = false;
            assert!(
                apply(&m, &o, &p, |j| {
                    pending = j.pending;
                    Ok(())
                })
                .is_err()
            );
            assert!(pending);
            assert_eq!(m.writes.get(), 2);
        }
    }
    #[test]
    fn commit_intent_precedes_writes_and_tampered_plans_are_rejected() {
        let (p, o) = fixture();
        let m = Fake::new(&p, &o);
        let mut calls = 0;
        let c = apply(&m, &o, &p, |j| {
            calls += 1;
            if calls == 1 {
                assert!(j.pending);
                assert_eq!(m.writes.get(), 0);
            } else {
                assert!(!j.pending);
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(c.read_back, p.after);
        assert_eq!(c.report["teamId"], 42);
        assert_eq!(c.report["writes"], 9);
        assert_eq!(c.report["patchWrites"], p.patches.len());
        for variant in 0..5 {
            let mut p = p.clone();
            let mut o = o.clone();
            match variant {
                0 => {
                    p.patches[0].offset = ARENA_ID;
                }
                1 => {
                    p.patches[0].area = Area::Team;
                    p.patches[0].offset = TEAM_ID;
                }
                2 => o.arena_table = o.player_table,
                3 => p.patches.reverse(),
                _ => o.clone_index = Some(o.donor),
            }
            let m = Fake::new(&p, &o);
            assert!(apply(&m, &o, &p, |_| Ok(())).is_err());
            assert_eq!(m.writes.get(), 0);
        }
    }
    #[test]
    fn journal_bounds_malformed_and_known_game_exit() {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "team-transaction-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let (p, o) = fixture();
        let mut i = Intent {
            schema: "nba2k27-team-write-intent-v1".into(),
            id: "a".repeat(48),
            owner: o,
            patches: p.patches,
            pending: true,
            result: "test".into(),
        };
        fs::create_dir_all(root.join("Data")).unwrap();
        save(&root, &i).unwrap();
        assert!(pending(&root).unwrap());
        let rows = vec![session_safety::Process {
            name: "NBA2K27.exe".into(),
            pid: 7,
            parent: 0,
            identity: None,
        }];
        assert!(resolve_ended(&root, &rows).is_err());
        assert!(pending(&root).unwrap());
        resolve_ended(&root, &[]).unwrap();
        assert!(!pending(&root).unwrap());
        i.owner.clone_id = Some(1);
        assert!(save(&root, &i).is_err());
        fs::write(path(&root), b"invalid").unwrap();
        assert!(pending(&root).is_err());
        assert!(resolve_ended(&root, &[]).is_err());
        assert_eq!(fs::read(path(&root)).unwrap(), b"invalid");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn no_op_and_nonclone_saves_keep_team_id_and_reserved_bytes() {
        let cases = team_changes::tests::fixture();
        for i in [0, 1, 9, 11] {
            let c = &cases[i];
            let (d, r, cached) = team_changes::tests::data(c);
            let p = team_changes::plan(
                &team_changes::validate(&c["input"]).unwrap(),
                &cached,
                &d,
                &r,
                r.references[0],
            )
            .unwrap();
            let (_, mut o) = fixture();
            let base = d.team_table - 0x21000;
            o.root = base + 0x31000;
            o.owner = base + 0x32000;
            o.allocation_base = base;
            o.player_table = base + 0x680;
            o.team_table = d.team_table;
            o.arena_table = d.arena_table;
            o.donor = p.donor;
            o.donor_hash = team_changes::hash(&r.arenas[p.donor]);
            o.clone_index = None;
            o.clone_id = None;
            let m = Fake::new(&p, &o);
            let mut calls = 0;
            let result = apply(&m, &o, &p, |_| {
                calls += 1;
                Ok(())
            })
            .unwrap();
            assert_eq!(result.read_back, p.after);
            assert_eq!(result.report["teamId"], 42);
            if i == 0 {
                assert_eq!(calls, 0);
                assert_eq!(m.writes.get(), 0);
            }
        }
    }
    #[test]
    fn closing_during_prewrite_inspection_never_starts_a_new_write() {
        let (p, o) = fixture();
        let mut m = Fake::new(&p, &o);
        m.cancel_read = Some(2);
        let mut records = vec![];
        assert!(
            apply(&m, &o, &p, |i| {
                records.push(i.clone());
                Ok(())
            })
            .is_err()
        );
        assert_eq!(m.writes.get(), 0);
        assert_eq!(*m.raw.borrow(), p.before);
        assert!(!records.last().unwrap().pending);
    }
}
