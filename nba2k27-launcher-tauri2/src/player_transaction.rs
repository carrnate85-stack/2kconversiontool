//! Player Save & verify policy. Native access is provided separately; this
//! module cannot discover a process, resolve an address or open a handle.
use crate::{
    backend,
    player_changes::{Area, Patch, Plan, RawPlayer},
    roster_snapshot,
    session_safety::{self, Identity},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
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
    pub table: u64,
    pub count: usize,
    pub index: usize,
    pub player_id: u16,
    pub appearance: u64,
    pub serializer: String,
    pub audio_verified: bool,
    pub origin_stamp: Option<String>,
}
impl Owner {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.game.pid > 0
                && self.game.started.parse::<u64>().is_ok_and(|n| n > 0)
                && Path::new(&self.game.path).is_absolute()
                && Path::new(&self.game.path)
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("NBA2K27.exe")),
            "Unsupported player transaction process identity"
        );
        for pointer in [
            self.image,
            self.root,
            self.owner,
            self.allocation_base,
            self.table,
        ] {
            roster_snapshot::checked_pointer(pointer)?;
        }
        let a = roster_snapshot::Allocation {
            base: self.allocation_base,
            len: self.allocation_len,
        };
        ensure!(
            a.valid()
                && self.table == a.base + 0x680
                && (100..=8192).contains(&self.count)
                && self.index < self.count
                && (self.count as u64)
                    .checked_mul(roster_snapshot::PLAYER_STRIDE as u64)
                    .is_some_and(|n| self
                        .table
                        .checked_add(n)
                        .is_some_and(|end| end <= a.base + a.len)),
            "Unsupported player transaction directory"
        );
        ensure!(
            self.appearance >= 0x10000
                && self
                    .appearance
                    .checked_add(4)
                    .is_some_and(|n| n <= 0x7fff_ffff_ffff)
                && self.serializer == roster_snapshot::serializer_fingerprint(),
            "Unsupported player transaction layout"
        );
        let table_end = self.table + self.count as u64 * roster_snapshot::PLAYER_STRIDE as u64;
        ensure!(
            self.appearance + 4 <= self.table || self.appearance >= table_end,
            "Player measurements alias the player record table; no write is safe"
        );
        ensure!(
            self.origin_stamp
                .as_ref()
                .is_none_or(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())),
            "Invalid player origin stamp"
        );
        Ok(())
    }
}
pub(crate) trait Access {
    fn owner(&self) -> Result<Owner>;
    fn capture(&self) -> Result<RawPlayer>;
    fn write(&self, area: Area, offset: usize, bytes: &[u8]) -> Result<()>;
    /// Cancellation stops new work here. The other methods must stay usable
    /// for bounded guarded rollback after cancellation, on the retained lease.
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
fn allowed(owner: &Owner) -> RawPlayer {
    let mut masks = RawPlayer {
        record: vec![0; roster_snapshot::PLAYER_STRIDE],
        appearance: vec![255; 4],
    };
    for (at, len) in [(0, 80), (0x108, 4), (0x110, 2), (0x45D, 1)] {
        masks.record[at..at + len].fill(255);
    }
    masks.record[0x115] |= 0xF8;
    masks.record[0x116] |= 3;
    masks.record[0x12D] |= 0x3F;
    masks.record[0x112] |= 255;
    masks.record[0x113] |= 15;
    masks.record[0x114] |= 0x3E;
    masks.record[0x36B] |= 0xF0;
    masks.record[0x3E2] |= 0x3E;
    for field in roster_snapshot::rating_fields()
        .iter()
        .filter(|f| f.maximum >= 0)
    {
        let mask = ((((1u64 << field.bits) - 1) << field.shift) as u32).to_le_bytes();
        for (i, b) in mask.into_iter().enumerate() {
            masks.record[field.offset + i] |= b;
        }
    }
    if owner.audio_verified {
        masks.record[0x2E8..0x2EA].fill(255);
    }
    if owner.origin_stamp.is_some() {
        masks.record[0xB0..0xB8].fill(255);
    }
    masks
}
fn validate_patches(owner: &Owner, patches: &[Patch]) -> Result<()> {
    owner.validate()?;
    ensure!(patches.len() <= 1280, "Too many player patches");
    let masks = allowed(owner);
    let mut seen = HashSet::new();
    for p in patches {
        ensure!(
            !p.before.is_empty()
                && p.before.len() <= 40
                && p.before.len() == p.after.len()
                && p.mask.len() == p.after.len()
                && p.before != p.after
                && p.offset
                    .checked_add(p.after.len())
                    .is_some_and(|end| end <= masks.bytes(p.area).len()),
            "Invalid player patch range"
        );
        for i in 0..p.after.len() {
            ensure!(
                seen.insert((p.area, p.offset + i))
                    && p.mask[i] & !masks.bytes(p.area)[p.offset + i] == 0
                    && (p.before[i] ^ p.after[i]) & !p.mask[i] == 0,
                "Unowned, overlapping or out-of-mask player patch"
            );
        }
    }
    Ok(())
}
impl Intent {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == "nba2k27-roster-write-intent-v1"
                && self.id.len() == 48
                && self.id.bytes().all(|b| b.is_ascii_hexdigit())
                && self.result.len() <= 512,
            "Unsupported roster transaction journal; left untouched"
        );
        validate_patches(&self.owner, &self.patches)
    }
}
pub(crate) fn path(root: &Path) -> PathBuf {
    root.join("Data/roster-write-journal.json")
}
fn read_intent(path: &Path) -> Result<Intent> {
    let mut bytes = Vec::new();
    fs::File::open(path)?.take(131073).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 131072,
        "Roster journal exceeds limits; left untouched"
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
pub(crate) fn save(root: &Path, intent: &Intent) -> Result<()> {
    intent.validate()?;
    backend::atomic_json(&path(root), intent)
}
pub(crate) fn resolve_ended(root: &Path, rows: &[session_safety::Process]) -> Result<()> {
    ensure!(
        !rows
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe")),
        "Close NBA 2K27 before resolving a roster transaction"
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
fn check(m: &impl Access, owner: &Owner, plan: &Plan, expected: &RawPlayer) -> Result<RawPlayer> {
    ensure!(
        m.owner()? == *owner,
        "Player database or process ownership changed"
    );
    let raw = m.capture()?;
    ensure!(
        plan.matches(&raw, expected)?,
        "Player values or unchanged neighboring bytes changed"
    );
    Ok(raw)
}
pub(crate) struct Commit {
    pub report: Value,
    pub read_back: RawPlayer,
}
impl std::ops::Deref for Commit {
    type Target = Value;
    fn deref(&self) -> &Value {
        &self.report
    }
}
/// Persist before writing, verify every step, and reverse only exact owned
/// states. Partial or foreign values are held, never overwritten or retried.
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
        owner.index == plan.record_index
            && u16::from_le_bytes(plan.before.record[0x128..0x12A].try_into()?) == owner.player_id
            && u64::from_le_bytes(plan.before.record[0x78..0x80].try_into()?) == owner.appearance
            && plan.expected(plan.patches.len())? == plan.after,
        "Player plan belongs to another record or is incomplete"
    );
    m.checkpoint()?;
    let original = check(m, owner, plan, &plan.before)?;
    if plan.patches.is_empty() {
        return Ok(Commit {
            report: json!({"ok":true,"writes":0,"readBackVerified":true,"noChanges":true}),
            read_back: original,
        });
    }
    let mut intent = Intent {
        schema: "nba2k27-roster-write-intent-v1".into(),
        id: backend::unique_id(),
        owner: owner.clone(),
        patches: plan.patches.clone(),
        pending: true,
        result: "intent saved before write".into(),
    };
    intent.validate()?;
    persist(&intent)?;
    let mut attempted = 0;
    let outcome = (|| -> Result<RawPlayer> {
        for (index, p) in plan.patches.iter().enumerate() {
            m.checkpoint()?;
            check(m, owner, plan, &plan.expected(index)?)?;
            attempted = index + 1; // A reported failure may nevertheless have changed bytes.
            m.write(p.area, p.offset, &p.after)?;
            check(m, owner, plan, &plan.expected(index + 1)?)?;
        }
        let read_back = check(m, owner, plan, &plan.after)?;
        intent.pending = false;
        intent.result = "applied and read back verified".into();
        persist(&intent)?;
        Ok(read_back)
    })();
    let read_back = match outcome {
        Ok(raw) => raw,
        Err(error) => {
            let rollback = (|| -> Result<()> {
                for index in (0..attempted).rev() {
                    ensure!(m.owner()? == *owner, "Rollback owner changed");
                    let current = m.capture()?;
                    let before = plan.expected(index)?;
                    let after = plan.expected(index + 1)?;
                    if plan.matches(&current, &before)? {
                        continue;
                    }
                    ensure!(
                        plan.matches(&current, &after)?,
                        "Partial or foreign player values prevent safe rollback"
                    );
                    let p = &plan.patches[index];
                    m.write(p.area, p.offset, &p.before)?;
                    check(m, owner, plan, &before)?;
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
            return Err(anyhow::anyhow!(
                "{error:#}. {}{}",
                if rollback.is_ok() {
                    "All attempted player changes are verified rolled back."
                } else {
                    "Some player changes may remain. Do not retry; close NBA 2K27 before clearing the recovery record."
                },
                if recorded.is_err() {
                    " Recovery record could not be completed; further writes must remain held."
                } else {
                    ""
                }
            ));
        }
    };
    Ok(Commit {
        report: json!({"schema":"nba2k27-player-edit-v1","ok":true,"gameCodeHooks":0,"writes":plan.patches.len(),"readBackVerified":true,"rollbackOnFailure":true,"plan":plan.view()}),
        read_back,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player_changes::{self, Extras};
    use std::{
        cell::{Cell, RefCell},
        sync::atomic::{AtomicU64, Ordering},
    };
    fn owner(raw: &RawPlayer) -> Owner {
        let appearance = u64::from_le_bytes(raw.record[0x78..0x80].try_into().unwrap());
        let base = appearance - 0x30000;
        Owner {
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
            table: base + 0x680,
            count: 100,
            index: 0,
            player_id: 42,
            appearance,
            serializer: roster_snapshot::serializer_fingerprint().into(),
            audio_verified: false,
            origin_stamp: None,
        }
    }
    fn fixture() -> (Plan, Owner) {
        let f = player_changes::tests::fixture();
        let case = &f["basic"][1];
        let raw = player_changes::tests::raw(case);
        let p = player_changes::plan(
            &player_changes::validate(&case["input"]).unwrap(),
            &case["cached"],
            &raw,
            Extras::default(),
        )
        .unwrap();
        let o = owner(&raw);
        (p, o)
    }
    struct Fake {
        owner: RefCell<Owner>,
        raw: RefCell<RawPlayer>,
        writes: Cell<usize>,
        reads: Cell<usize>,
        fail_read_at: Option<usize>,
        fail_at: Option<usize>,
        fail_after_write: bool,
        partial: bool,
        foreign: bool,
        change_owner: bool,
        cancel_at: Option<usize>,
    }
    impl Fake {
        fn new(p: &Plan, o: &Owner) -> Self {
            Self {
                owner: RefCell::new(o.clone()),
                raw: RefCell::new(p.before.clone()),
                writes: Cell::new(0),
                reads: Cell::new(0),
                fail_read_at: None,
                fail_at: None,
                fail_after_write: false,
                partial: false,
                foreign: false,
                change_owner: false,
                cancel_at: None,
            }
        }
    }
    impl Access for Fake {
        fn owner(&self) -> Result<Owner> {
            Ok(self.owner.borrow().clone())
        }
        fn capture(&self) -> Result<RawPlayer> {
            let n = self.reads.get() + 1;
            self.reads.set(n);
            ensure!(self.fail_read_at != Some(n), "Synthetic read-back failure");
            Ok(self.raw.borrow().clone())
        }
        fn checkpoint(&self) -> Result<()> {
            ensure!(
                self.cancel_at.is_none_or(|n| self.writes.get() < n),
                "Synthetic close cancellation"
            );
            Ok(())
        }
        fn write(&self, area: Area, offset: usize, bytes: &[u8]) -> Result<()> {
            let n = self.writes.get() + 1;
            self.writes.set(n);
            let fail = self.fail_at == Some(n);
            if fail && !self.fail_after_write {
                anyhow::bail!("Synthetic write failure");
            }
            let mut raw = self.raw.borrow_mut();
            let destination = &mut raw.bytes_mut(area)[offset..offset + bytes.len()];
            destination.copy_from_slice(bytes);
            if fail && self.partial {
                destination[0] ^= 0x80;
            }
            if fail && self.foreign {
                raw.record[0x128] ^= 1;
            }
            if fail && self.change_owner {
                self.owner.borrow_mut().owner += 8;
            }
            ensure!(!fail, "Synthetic reported failure after write");
            Ok(())
        }
    }
    #[test]
    fn successful_commit_noop_and_durable_intent_precede_writes() {
        let (p, o) = fixture();
        let m = Fake::new(&p, &o);
        let records = RefCell::new(Vec::<Intent>::new());
        let result = apply(&m, &o, &p, |i| {
            if records.borrow().is_empty() {
                assert_eq!(m.writes.get(), 0);
            }
            records.borrow_mut().push(i.clone());
            Ok(())
        })
        .unwrap();
        assert_eq!(m.raw.borrow().clone(), p.after);
        assert_eq!(result["readBackVerified"], true);
        assert_eq!(result.read_back, p.after);
        assert!(records.borrow()[0].pending);
        assert!(!records.borrow().last().unwrap().pending);
        let f = player_changes::tests::fixture();
        let p = player_changes::plan(
            &player_changes::validate(&f["input"]).unwrap(),
            &f["cached"],
            &player_changes::tests::raw(&f),
            Extras::default(),
        )
        .unwrap();
        assert!(p.patches.is_empty());
        let m = Fake::new(&p, &o);
        let result = apply(&m, &o, &p, |_| {
            anyhow::bail!("No-op must not create an intent")
        })
        .unwrap();
        assert_eq!(result["noChanges"], true);
        assert_eq!(m.writes.get(), 0);
    }
    #[test]
    fn each_read_back_failure_rolls_back_without_a_success_response() {
        let (p, o) = fixture();
        let good = Fake::new(&p, &o);
        apply(&good, &o, &p, |_| Ok(())).unwrap();
        for at in 1..=good.reads.get() {
            let mut m = Fake::new(&p, &o);
            m.fail_read_at = Some(at);
            let last = RefCell::new(None);
            assert!(
                apply(&m, &o, &p, |i| {
                    *last.borrow_mut() = Some(i.clone());
                    Ok(())
                })
                .is_err(),
                "Read {at}"
            );
            assert_eq!(*m.raw.borrow(), p.before, "Read {at}");
            assert!(last.borrow().as_ref().is_none_or(|i| !i.pending));
        }
    }
    #[test]
    fn every_reported_failure_and_close_cancellation_rolls_back_owned_prefix() {
        let (p, o) = fixture();
        for at in 1..=p.patches.len() {
            for after in [false, true] {
                let mut m = Fake::new(&p, &o);
                m.fail_at = Some(at);
                m.fail_after_write = after;
                let last = RefCell::new(None);
                assert!(
                    apply(&m, &o, &p, |i| {
                        *last.borrow_mut() = Some(i.clone());
                        Ok(())
                    })
                    .is_err()
                );
                assert_eq!(
                    m.raw.borrow().clone(),
                    p.before,
                    "Failure {at}, after={after}"
                );
                assert!(!last.borrow().as_ref().unwrap().pending);
            }
        }
        for at in 1..p.patches.len() {
            let mut m = Fake::new(&p, &o);
            m.cancel_at = Some(at);
            assert!(apply(&m, &o, &p, |_| Ok(())).is_err());
            assert_eq!(m.raw.borrow().clone(), p.before);
        }
    }
    #[test]
    fn partial_foreign_or_new_owner_is_held_and_never_blindly_restored() {
        let (p, o) = fixture();
        for kind in 0..3 {
            let mut m = Fake::new(&p, &o);
            m.fail_at = Some(2);
            m.fail_after_write = true;
            m.partial = kind == 0;
            m.foreign = kind == 1;
            m.change_owner = kind == 2;
            let last = RefCell::new(None);
            assert!(
                apply(&m, &o, &p, |i| {
                    *last.borrow_mut() = Some(i.clone());
                    Ok(())
                })
                .is_err()
            );
            assert!(last.borrow().as_ref().unwrap().pending);
            assert_eq!(m.writes.get(), 2);
        }
    }
    #[test]
    fn failed_intent_or_commit_record_never_silently_succeeds() {
        let (p, o) = fixture();
        let m = Fake::new(&p, &o);
        assert!(apply(&m, &o, &p, |_| anyhow::bail!("Disk full before intent")).is_err());
        assert_eq!(m.writes.get(), 0);
        let m = Fake::new(&p, &o);
        let calls = Cell::new(0);
        assert!(
            apply(&m, &o, &p, |_| {
                calls.set(calls.get() + 1);
                ensure!(calls.get() != 2, "Commit record failed");
                Ok(())
            })
            .is_err()
        );
        assert_eq!(m.raw.borrow().clone(), p.before);
    }
    #[test]
    fn stale_player_scope_and_arbitrary_or_readonly_masks_never_write() {
        let (mut p, o) = fixture();
        let m = Fake::new(&p, &o);
        let mut wrong = o.clone();
        wrong.index = 1;
        assert!(apply(&m, &wrong, &p, |_| Ok(())).is_err());
        assert_eq!(m.writes.get(), 0);
        let mut alias = o.clone();
        alias.appearance = o.table + 0x108;
        assert!(alias.validate().is_err());
        alias.appearance = o.table - 2;
        assert!(alias.validate().is_err());
        p.patches[0].offset = 0x60;
        assert!(apply(&m, &o, &p, |_| Ok(())).is_err());
        assert_eq!(m.writes.get(), 0);
        let mut before = Fake::new(&fixture().0, &o);
        before.raw.get_mut().record[0x128] ^= 1;
        assert!(apply(&before, &o, &fixture().0, |_| Ok(())).is_err());
        assert_eq!(before.writes.get(), 0);
    }
    #[test]
    fn pending_journal_requires_known_game_exit_and_malformed_stays_intact() {
        static N: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "roster-intent-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("Data")).unwrap();
        let (p, o) = fixture();
        let i = Intent {
            schema: "nba2k27-roster-write-intent-v1".into(),
            id: backend::unique_id(),
            owner: o.clone(),
            patches: p.patches,
            pending: true,
            result: "fixture".into(),
        };
        save(&root, &i).unwrap();
        assert!(pending(&root).unwrap());
        let rows = [session_safety::Process {
            name: "NBA2K27.exe".into(),
            pid: o.game.pid,
            parent: 0,
            identity: Some(o.game.clone()),
        }];
        assert!(resolve_ended(&root, &rows).is_err());
        resolve_ended(&root, &[]).unwrap();
        assert!(!pending(&root).unwrap());
        fs::write(path(&root), b"{broken").unwrap();
        assert!(pending(&root).is_err());
        assert_eq!(fs::read(path(&root)).unwrap(), b"{broken");
        fs::remove_file(path(&root)).unwrap();
        fs::remove_dir(root.join("Data")).unwrap();
        fs::remove_dir(root).unwrap();
    }
}
