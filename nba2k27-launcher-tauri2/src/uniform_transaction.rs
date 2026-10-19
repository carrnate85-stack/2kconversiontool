//! Retained-lease uniform transactions. Journals block retries, never replay.
use crate::{
    backend, roster_snapshot,
    session_safety::{self, Identity},
    team_changes,
    uniform_changes::{self, Edit, Patch, Plan, STRIDE},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
};
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
    pub uniform_table: u64,
    pub uniform_count: usize,
    pub index: usize,
    pub team_id: u16,
    pub team_name: String,
    pub slot: usize,
    pub uniform_index: usize,
    pub uniform_id: u16,
    pub filename: String,
    pub slots_hash: String,
    pub references_hash: String,
    pub serializer: String,
}
impl Owner {
    pub fn address(&self) -> u64 {
        self.uniform_table + (self.uniform_index * STRIDE) as u64
    }
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.game.pid > 0
                && self.game.started.parse::<u64>().is_ok_and(|n| n > 0)
                && Path::new(&self.game.path).is_absolute()
                && Path::new(&self.game.path)
                    .file_name()
                    .is_some_and(|p| p.to_string_lossy().eq_ignore_ascii_case("NBA2K27.exe")),
            "Unsupported uniform process identity"
        );
        for p in [
            self.image,
            self.root,
            self.owner,
            self.allocation_base,
            self.player_table,
            self.team_table,
            self.arena_table,
            self.uniform_table,
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
                && (1..=8192).contains(&self.uniform_count)
                && self.index < self.team_count
                && self.slot < 32
                && self.uniform_index < self.uniform_count,
            "Unsupported uniform directories"
        );
        let mut spans = vec![];
        for (p, n, s) in [
            (
                self.player_table,
                self.player_count,
                roster_snapshot::PLAYER_STRIDE,
            ),
            (self.team_table, self.team_count, team_changes::TEAM_STRIDE),
            (
                self.arena_table,
                self.arena_count,
                team_changes::ARENA_STRIDE,
            ),
            (self.uniform_table, self.uniform_count, STRIDE),
        ] {
            let end = p
                .checked_add((n * s) as u64)
                .context("Uniform table overflow")?;
            ensure!(
                p >= a.base + 0x680 && end <= a.base + a.len,
                "Uniform directory exceeds allocation"
            );
            spans.push((p, end));
        }
        spans.sort_unstable();
        ensure!(
            spans.windows(2).all(|w| w[0].1 <= w[1].0),
            "Uniform directories overlap"
        );
        ensure!(
            !self.team_name.is_empty()
                && self.team_name.encode_utf16().count() <= 23
                && !self.team_name.chars().any(|c| c <= '\u{1F}')
                && !self.filename.is_empty()
                && self.filename.encode_utf16().count() <= 63
                && !self.filename.chars().any(|c| c <= '\u{1F}')
                && self.serializer == roster_snapshot::serializer_fingerprint(),
            "Unsupported uniform identity or serializer"
        );
        for h in [&self.slots_hash, &self.references_hash] {
            ensure!(
                h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()),
                "Invalid uniform ownership digest"
            );
        }
        Ok(())
    }
}
/// Preserve existing WPF shared-slot behavior; do not clone or silently reject
/// a uniform merely because two valid slots reference the same native record.
pub(crate) fn reference_hash(rows: &[Vec<u64>], address: u64) -> String {
    let mut bytes = vec![];
    for (i, slots) in rows.iter().enumerate() {
        for (j, p) in slots.iter().enumerate() {
            if *p == address {
                bytes.extend((i as u32).to_le_bytes());
                bytes.extend((j as u32).to_le_bytes());
            }
        }
    }
    team_changes::hash(&bytes)
}
pub(crate) trait Access {
    fn owner(&self) -> Result<Owner>;
    fn capture(&self) -> Result<Vec<u8>>;
    fn write(&self, offset: usize, bytes: &[u8]) -> Result<()>;
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
    ensure!(patches.len() <= 25, "Too many uniform writes");
    let masks = uniform_changes::masks();
    let mut seen = HashSet::new();
    for p in patches {
        let mask = masks
            .get(&p.offset)
            .context("Unsupported uniform write offset")?
            .to_le_bytes();
        ensure!(
            seen.insert(p.offset)
                && p.before.len() == 4
                && p.after.len() == 4
                && p.mask.len() == 4
                && p.before != p.after,
            "Invalid uniform patch shape"
        );
        for i in 0..4 {
            ensure!(
                p.mask[i] & !mask[i] == 0 && (p.before[i] ^ p.after[i]) & !p.mask[i] == 0,
                "Uniform patch changes unowned bits"
            );
        }
    }
    Ok(())
}
impl Intent {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == "nba2k27-uniform-write-intent-v1"
                && self.id.len() == 48
                && self.id.bytes().all(|b| b.is_ascii_hexdigit())
                && self.result.len() <= 512,
            "Unsupported uniform journal; left untouched"
        );
        validate_patches(&self.owner, &self.patches)
    }
}
pub(crate) fn path(root: &Path) -> PathBuf {
    root.join("Data/uniform-write-journal.json")
}
fn read_intent(path: &Path) -> Result<Intent> {
    let mut b = vec![];
    fs::File::open(path)?.take(65537).read_to_end(&mut b)?;
    ensure!(
        b.len() <= 65536,
        "Uniform journal exceeds limits; left untouched"
    );
    let i: Intent = serde_json::from_slice(&b)?;
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
        serde_json::to_vec_pretty(i)?.len() <= 65536,
        "Uniform journal exceeds limits before write"
    );
    backend::atomic_json(&path(root), i)
}
pub(crate) fn resolve_ended(root: &Path, rows: &[session_safety::Process]) -> Result<()> {
    ensure!(
        !rows
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe")),
        "Close NBA 2K27 before resolving a uniform transaction"
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
fn check(m: &impl Access, owner: &Owner, expected: &[u8]) -> Result<Vec<u8>> {
    ensure!(
        m.owner()? == *owner,
        "Uniform process, directory or slot ownership changed"
    );
    let raw = m.capture()?;
    ensure!(
        raw.len() == STRIDE && raw == expected,
        "Uniform or unchanged neighboring values changed"
    );
    Ok(raw)
}
pub(crate) struct Commit {
    pub report: Value,
    pub read_back: Vec<u8>,
}
pub(crate) fn apply(
    m: &impl Access,
    owner: &Owner,
    edit: &Edit,
    plan: &Plan,
    mut persist: impl FnMut(&Intent) -> Result<()>,
) -> Result<Commit> {
    validate_patches(owner, &plan.patches)?;
    ensure!(
        owner.index == edit.record_index
            && owner.team_id == edit.team_id
            && owner.team_name == edit.expected_team_name
            && owner.slot == edit.slot
            && owner.uniform_id == edit.uniform_id
            && owner.filename == edit.expected_base_filename
            && plan.before.len() == STRIDE
            && plan.after.len() == STRIDE
            && team_changes::uint(&plan.before, 0x50, 2)? == owner.uniform_id as u64
            && team_changes::uint(&plan.before, 0x52, 2)? == owner.team_id as u64
            && uniform_changes::ascii(&plan.before, 8, 64)? == owner.filename
            && plan.expected(plan.patches.len())? == plan.after,
        "Uniform plan belongs to another record or is incomplete"
    );
    m.checkpoint()?;
    let original = check(m, owner, &plan.before)?;
    if plan.patches.is_empty() {
        return Ok(Commit {
            report: uniform_changes::report(edit, plan, owner.game.pid, owner.address()),
            read_back: original,
        });
    }
    let mut i = Intent {
        schema: "nba2k27-uniform-write-intent-v1".into(),
        id: backend::unique_id(),
        owner: owner.clone(),
        patches: plan.patches.clone(),
        pending: true,
        result: "intent saved before write".into(),
    };
    i.validate()?;
    persist(&i)?;
    let mut attempted = 0;
    let outcome = (|| -> Result<Vec<u8>> {
        for (n, p) in plan.patches.iter().enumerate() {
            m.checkpoint()?;
            check(m, owner, &plan.expected(n)?)?;
            m.checkpoint()?;
            attempted = n + 1;
            m.write(p.offset, &p.after)?;
            check(m, owner, &plan.expected(n + 1)?)?;
        }
        let raw = check(m, owner, &plan.after)?;
        i.pending = false;
        i.result = "applied and read back verified".into();
        persist(&i)?;
        Ok(raw)
    })();
    let raw = match outcome {
        Ok(raw) => raw,
        Err(error) => {
            let rollback = (|| -> Result<()> {
                for n in (0..attempted).rev() {
                    ensure!(m.owner()? == *owner, "Uniform rollback owner changed");
                    let raw = m.capture()?;
                    let before = plan.expected(n)?;
                    let after = plan.expected(n + 1)?;
                    if raw == before {
                        continue;
                    }
                    ensure!(
                        raw == after,
                        "Partial or foreign uniform changes prevent safe rollback"
                    );
                    let p = &plan.patches[n];
                    m.write(p.offset, &p.before)?;
                    check(m, owner, &before)?;
                }
                Ok(())
            })();
            i.pending = rollback.is_err();
            i.result = if rollback.is_ok() {
                "failed; original owned values verified"
            } else {
                "failed; recovery held for changed owner or foreign/partial values"
            }
            .into();
            let recorded = persist(&i);
            anyhow::bail!(
                "{error:#}. {}{}",
                if rollback.is_ok() {
                    "All attempted uniform changes are verified rolled back."
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
        report: uniform_changes::report(edit, plan, owner.game.pid, owner.address()),
        read_back: raw,
    })
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    pub fn fixture() -> (Edit, Plan, Owner, Value) {
        let (e, p, c) = uniform_changes::tests::largest_plan();
        let u = |key| c[key].as_str().unwrap().parse::<u64>().unwrap();
        let base = u("regionBase");
        let team = uniform_changes::tests::bytes(&c["teamBefore"]);
        let slots: Vec<_> = team[0xDE0..0xEE0]
            .chunks_exact(8)
            .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
            .collect();
        let o = Owner {
            game: Identity {
                pid: 7,
                started: "42".into(),
                path: r"C:\QA\NBA2K27.exe".into(),
            },
            image: 0x140000000,
            root: base + 0x41000,
            owner: base + 0x42000,
            allocation_base: base,
            allocation_len: 0x800000,
            player_table: base + 0x680,
            player_count: 100,
            team_table: u("teamTable"),
            team_count: 3,
            arena_table: u("arenaTable"),
            arena_count: 3,
            uniform_table: u("uniformTable"),
            uniform_count: 4,
            index: 0,
            team_id: 42,
            team_name: "QA Team".into(),
            slot: 0,
            uniform_index: 0,
            uniform_id: 77,
            filename: "uniform_qa".into(),
            slots_hash: team_changes::hash(&team[0xDE0..0xEE0]),
            references_hash: reference_hash(
                &[slots, vec![u("uniformTable") + STRIDE as u64], vec![]],
                u("uniformTable"),
            ),
            serializer: roster_snapshot::serializer_fingerprint().into(),
        };
        (e, p, o, c)
    }
    struct Fake {
        owner: RefCell<Owner>,
        raw: RefCell<Vec<u8>>,
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
        fn capture(&self) -> Result<Vec<u8>> {
            let n = self.reads.get() + 1;
            self.reads.set(n);
            ensure!(self.fail_read != Some(n), "Synthetic uniform read failure");
            Ok(self.raw.borrow().clone())
        }
        fn checkpoint(&self) -> Result<()> {
            ensure!(
                self.cancel.is_none_or(|n| self.writes.get() < n)
                    && self.cancel_read.is_none_or(|n| self.reads.get() < n),
                "Synthetic uniform close cancellation"
            );
            Ok(())
        }
        fn write(&self, at: usize, b: &[u8]) -> Result<()> {
            let n = self.writes.get() + 1;
            self.writes.set(n);
            let fail = self.fail == Some(n);
            if fail && !self.after {
                anyhow::bail!("Synthetic uniform write failure");
            }
            let mut raw = self.raw.borrow_mut();
            raw[at..at + b.len()].copy_from_slice(b);
            if fail && self.partial {
                raw[at] ^= 0x80;
            }
            if fail && self.foreign {
                raw[0x50] ^= 1;
            }
            if fail && self.changed_owner {
                self.owner.borrow_mut().slots_hash = "f".repeat(64);
            }
            ensure!(!fail, "Synthetic uniform reported failure after write");
            Ok(())
        }
    }
    #[test]
    fn unchanged_uniform_verifies_without_writes_or_journal() {
        let rows = uniform_changes::tests::fixture();
        let c = &rows[0];
        let (e, team, raw, address) = uniform_changes::tests::case_data(c);
        let p = uniform_changes::plan(
            &e,
            &c["cached"],
            &c["cached"]["uniforms"][0],
            &team,
            &raw,
            address,
        )
        .unwrap();
        let (_, _, o, _) = fixture();
        let m = Fake::new(&p, &o);
        let result = apply(&m, &o, &e, &p, |_| {
            anyhow::bail!("No-op must not publish a journal")
        })
        .unwrap();
        assert!(p.patches.is_empty());
        assert_eq!(m.writes.get(), 0);
        assert_eq!(result.read_back, raw);
        assert_eq!(result.report["noChanges"], true);
        assert_eq!(result.report["physicalWrites"], 0);
    }
    #[test]
    fn every_write_readback_and_close_prefix_rolls_back() {
        let (e, p, o, _) = fixture();
        for n in 1..=p.patches.len() {
            for after in [false, true] {
                let mut m = Fake::new(&p, &o);
                m.fail = Some(n);
                m.after = after;
                let mut records = vec![];
                assert!(
                    apply(&m, &o, &e, &p, |i| {
                        records.push(i.clone());
                        Ok(())
                    })
                    .is_err()
                );
                assert_eq!(*m.raw.borrow(), p.before);
                assert!(!records.last().unwrap().pending);
            }
            let mut m = Fake::new(&p, &o);
            m.cancel = Some(n - 1);
            assert!(apply(&m, &o, &e, &p, |_| Ok(())).is_err());
            assert_eq!(*m.raw.borrow(), p.before);
        }
        for n in 1..=2 * p.patches.len() + 2 {
            let mut m = Fake::new(&p, &o);
            m.fail_read = Some(n);
            assert!(apply(&m, &o, &e, &p, |_| Ok(())).is_err());
            assert_eq!(*m.raw.borrow(), p.before);
        }
        let mut m = Fake::new(&p, &o);
        m.cancel_read = Some(2);
        assert!(apply(&m, &o, &e, &p, |_| Ok(())).is_err());
        assert_eq!(m.writes.get(), 0);
    }
    #[test]
    fn durable_intent_commit_failure_and_foreign_ownership_are_safe() {
        let (e, p, o, _) = fixture();
        let m = Fake::new(&p, &o);
        let mut calls = 0;
        let c = apply(&m, &o, &e, &p, |i| {
            calls += 1;
            if calls == 1 {
                assert_eq!(m.writes.get(), 0);
                assert!(i.pending);
            } else {
                assert!(!i.pending);
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(c.read_back, p.after);
        assert_eq!(c.report["writes"], 52);
        assert_eq!(c.report["physicalWrites"], p.patches.len());
        let m = Fake::new(&p, &o);
        assert!(apply(&m, &o, &e, &p, |_| anyhow::bail!("Full disk")).is_err());
        assert_eq!(m.writes.get(), 0);
        let m = Fake::new(&p, &o);
        let mut calls = 0;
        assert!(
            apply(&m, &o, &e, &p, |_| {
                calls += 1;
                ensure!(calls != 2, "Completion disk failure");
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
                apply(&m, &o, &e, &p, |i| {
                    pending = i.pending;
                    Ok(())
                })
                .is_err()
            );
            assert!(pending);
            assert_eq!(m.writes.get(), 2);
        }
    }
    #[test]
    fn unsupported_masks_aliases_and_recycled_identity_never_write() {
        let (e, p, o, _) = fixture();
        for variant in 0..6 {
            let mut p = p.clone();
            let mut o = o.clone();
            match variant {
                0 => p.patches[0].offset = 0x50,
                1 => {
                    p.patches[0].offset = 0x164;
                    p.patches[0].mask = vec![255; 4];
                }
                2 => o.uniform_table = o.team_table,
                3 => o.uniform_id += 1,
                4 => o.slot = 32,
                _ => p.patches.push(p.patches[0].clone()),
            }
            let m = Fake::new(&p, &o);
            assert!(apply(&m, &o, &e, &p, |_| Ok(())).is_err());
            assert_eq!(m.writes.get(), 0);
        }
    }
    #[test]
    fn journal_requires_game_exit_and_preserves_malformed_data() {
        let (e, p, o, _) = fixture();
        let _ = e;
        let root = std::env::temp_dir().join(format!("uniform-journal-{}", backend::unique_id()));
        fs::create_dir_all(root.join("Data")).unwrap();
        let i = Intent {
            schema: "nba2k27-uniform-write-intent-v1".into(),
            id: "a".repeat(48),
            owner: o,
            patches: p.patches,
            pending: true,
            result: "test".into(),
        };
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
        fs::write(path(&root), b"bad").unwrap();
        assert!(pending(&root).is_err());
        assert!(resolve_ended(&root, &[]).is_err());
        assert_eq!(fs::read(path(&root)).unwrap(), b"bad");
        fs::remove_dir_all(root).unwrap();
    }
}
