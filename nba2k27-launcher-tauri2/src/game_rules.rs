//! Whitelisted GameRuleData mappings from the live-verified WPF implementation.
//! No arbitrary addresses, persisted opt-in, injection or automatic rule changes.
use crate::{
    backend,
    blacktop_native::{self, ContextState, Memory},
    session_safety::Identity,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{fs, path::Path, time::Instant};

pub struct Field {
    pub id: &'static str,
    pub label: &'static str,
    pub offset: u64,
    pub width: usize,
    max: u32,
}
impl Field {
    pub fn valid(&self, value: u32) -> bool {
        value <= self.max
    }
    fn choices(&self) -> Vec<Value> {
        (0..=self.max).map(|value| json!({"value":value,"label":match self.id {
            "ThreePointShotRestriction" => ["Enabled","Enabled after steal or block","Disabled"][value as usize].to_string(),
            "OffensiveViolationInKey" | "DefensiveViolationInKey" => ["Off","3 seconds","5 seconds"][value as usize].to_string(),
            "FoulOut" if value > 0 => value.to_string(),
            _ => if value == 0 {"Off"} else {"On"}.to_string(),
        }})).collect()
    }
}
macro_rules! fields { ($(($id:literal,$label:literal,$off:literal,$width:literal,$max:literal)),* $(,)?) => {pub static FIELDS: &[Field] = &[$(Field{id:$id,label:$label,offset:$off,width:$width,max:$max}),*];}; }
fields![
    ("ShotClock", "Shot Clock", 0x816, 1, 1),
    ("OutOfBounds", "Out Of Bounds", 0x824, 1, 1),
    ("BackcourtViolation", "Backcourt Violation", 0x825, 1, 1),
    ("EightSecondsHalfCourt", "8 Sec Half Court", 0x826, 1, 1),
    ("FiveSecondsInbounding", "5 Sec Inbound", 0xA75, 1, 1),
    ("OffensiveViolationInKey", "3 in the Key", 0x908, 4, 2),
    ("DefensiveViolationInKey", "Def 3 in the Key", 0x90C, 4, 2),
    (
        "FiveSecondsBackToBasket",
        "5 Sec Back To Basket",
        0x827,
        1,
        1
    ),
    ("CloselyGuarded", "5 Sec Closely Guarded", 0xA4E, 1, 1),
    ("OverTheBackRule", "Over the Back Foul", 0x830, 1, 1),
    (
        "GenericPersonalFoulRule",
        "Generic Personal Foul",
        0x831,
        1,
        1
    ),
    ("ChargingRule", "Charging Foul", 0x832, 1, 1),
    (
        "OffensiveOffballFoulRule",
        "Offensive Offball Foul",
        0x833,
        1,
        1
    ),
    (
        "DefensiveOffballFoulRule",
        "Defensive Offball Foul",
        0x834,
        1,
        1
    ),
    ("BlockingRule", "Blocking Foul", 0x835, 1, 1),
    ("ReachingFoulRule", "Reaching Foul", 0x836, 1, 1),
    ("LooseBallRule", "Loose Ball Foul", 0x838, 1, 1),
    ("ShootingFoulRule", "Shooting Foul", 0x837, 1, 1),
    ("IntentionalFoul", "Intentional Foul", 0x839, 1, 1),
    ("ClearPathRule", "Clear Path Foul", 0x840, 1, 1),
    (
        "TransitionTakeFoulRule",
        "Transition Take Foul",
        0x841,
        1,
        1
    ),
    ("KickBall", "Kick Ball", 0x83A, 1, 1),
    ("Goaltending", "Goaltending", 0x828, 1, 1),
    ("IllegalPick", "Illegal Pick", 0x82B, 1, 1),
    ("Traveling", "Traveling", 0x829, 1, 1),
    ("DoubleDribble", "Double Dribble", 0x82A, 1, 1),
    ("FoulOut", "Foul Out", 0x82C, 4, 6),
    ("ThreePointShotRestriction", "3Pt Shots", 0xA98, 4, 2),
    ("ZoneDefenseAllowed", "Zone Defense Allowed", 0x8D4, 1, 1),
    ("NoChargeZone", "No Charge Zone", 0xA50, 1, 1),
    ("BuzzerBeaterReview", "Buzzer Beater Review", 0x849, 1, 1),
    ("ClearBallRequired", "Clear Ball Required", 0xA51, 1, 1),
    ("Timeouts", "Timeouts", 0xA5D, 1, 1),
    (
        "ScreenOutOfBoundsViolation",
        "Screen Set Out of Bounds",
        0x844,
        1,
        1
    ),
    (
        "PlayerOutOfBoundsViolationRule",
        "Offensive Player Out of Bounds",
        0x846,
        1,
        1
    ),
    (
        "BallEnteredBasketFromBelow",
        "Ball Entered Basket From Below",
        0xA74,
        1,
        1
    ),
    ("TechnicalFoulRule", "Technical Foul", 0x83B, 1, 1),
    ("FlagrantFoulRule", "Flagrant Foul", 0x83C, 1, 1),
    (
        "TechnicalFoulEjectionRule",
        "Technical Foul Ejection",
        0x83D,
        1,
        1
    ),
];
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub context: ContextState,
    pub base: u64,
    pub values: Vec<u32>,
}
pub fn capture(m: &impl Memory) -> Result<Snapshot> {
    blacktop_native::validate_build(m)?;
    let c = blacktop_native::context(m)?;
    ensure!(
        matches!(
            blacktop_native::boundary(&c, m.base()),
            "selection" | "match"
        ),
        "Open Blacktop player selection or a Blacktop match before reading rules"
    );
    let values = FIELDS
        .iter()
        .map(|f| {
            let b = m.read(c.owner + f.offset, f.width)?;
            ensure!(b.len() == f.width, "Partial rule read");
            Ok(if f.width == 1 {
                b[0] as u32
            } else {
                u32::from_le_bytes(
                    b.try_into()
                        .map_err(|_| anyhow::anyhow!("Partial numeric rule"))?,
                )
            })
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        blacktop_native::context(m)? == c,
        "Mode or rules owner changed during reading"
    );
    // Repeat every field; reject a torn group rather than fabricating a write lease.
    for (f, v) in FIELDS.iter().zip(&values) {
        ensure!(
            m.read(c.owner + f.offset, f.width)? == v.to_le_bytes()[..f.width],
            "Rule group changed during reading"
        );
    }
    ensure!(
        blacktop_native::context(m)? == c,
        "Mode changed after reading"
    );
    Ok(Snapshot {
        context: c,
        base: m.base(),
        values,
    })
}
pub trait Access {
    fn capture(&self) -> Result<Snapshot>;
    fn write(&self, expected: &Snapshot, index: usize, value: u32) -> Result<()>;
}
pub struct Lease {
    pub token: String,
    pub identity: Identity,
    pub session: String,
    pub snapshot: Snapshot,
    pub created: Instant,
}
impl Lease {
    pub fn new(identity: Identity, session: String, snapshot: Snapshot) -> Self {
        Self {
            token: backend::unique_id(),
            identity,
            session,
            snapshot,
            created: Instant::now(),
        }
    }
    pub fn validate(&self, token: &str, identity: &Identity, session: &str) -> Result<()> {
        ensure!(
            self.token == token
                && self.identity == *identity
                && self.session == session
                && self.created.elapsed().as_secs() < 30,
            "Rules snapshot expired or belongs to another session; refresh current values"
        );
        Ok(())
    }
    pub fn view(&self) -> Value {
        json!({"token":self.token,"boundary":blacktop_native::boundary(&self.snapshot.context,self.snapshot.base),"expiresInSeconds":30,"rules":FIELDS.iter().zip(&self.snapshot.values).map(|(f,v)|json!({"id":f.id,"label":f.label,"value":v,"supported":f.valid(*v),"choices":f.choices()})).collect::<Vec<_>>()})
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Intent {
    version: u32,
    pub pending: bool,
    session: String,
    game: Identity,
    base: u64,
    owner: u64,
    field: String,
    before: u32,
    after: u32,
    pub result: String,
}
impl Intent {
    fn validate(&self) -> Result<()> {
        let f = FIELDS
            .iter()
            .find(|f| f.id == self.field)
            .context("Unknown journal field")?;
        ensure!(
            self.version == 1
                && self.session.len() == 48
                && self.session.bytes().all(|b| b.is_ascii_hexdigit())
                && self.game.pid > 0
                && self.game.started.parse::<u64>().is_ok()
                && Path::new(&self.game.path).is_absolute()
                && Path::new(&self.game.path)
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("NBA2K27.exe"))
                && (0x10000..=0x7fffffff0000).contains(&self.base)
                && (0x10000..=0x7fffffff0000).contains(&self.owner)
                && self.owner % 8 == 0
                && f.valid(self.before)
                && f.valid(self.after)
                && self.before != self.after
                && self.result.len() <= 256,
            "Unsupported rule transaction journal; left untouched"
        );
        Ok(())
    }
}
pub fn path(root: &Path) -> std::path::PathBuf {
    root.join("Data/game-rules-journal.json")
}
pub fn pending(root: &Path) -> Result<bool> {
    let p = path(root);
    if !p.try_exists()? {
        return Ok(false);
    }
    let i = read_intent(&p)?;
    Ok(i.pending)
}
fn read_intent(p: &Path) -> Result<Intent> {
    use std::io::Read;
    let mut data = vec![];
    fs::File::open(p)?.take(16385).read_to_end(&mut data)?;
    ensure!(data.len() <= 16384, "Rule journal exceeds limits");
    let i: Intent = serde_json::from_slice(&data)?;
    i.validate()?;
    Ok(i)
}
/// Memory is gone only after a fresh, known snapshot proves *all* NBA processes absent.
pub fn resolve_ended(root: &Path, rows: &[crate::session_safety::Process]) -> Result<()> {
    ensure!(
        !rows
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe")),
        "Close NBA 2K27 before resolving a rule transaction"
    );
    let p = path(root);
    if p.try_exists()? {
        let mut i = read_intent(&p)?;
        if i.pending {
            i.pending = false;
            i.result = "game-ended; no memory restoration needed".into();
            backend::atomic_json(&p, &i)?;
        }
    }
    Ok(())
}
/// The persistence callback is exercised by offline failure fixtures. Production uses atomic_json.
pub fn apply(
    m: &impl Access,
    lease: &Lease,
    field: &str,
    after: u32,
    mut save: impl FnMut(&Intent) -> Result<()>,
) -> Result<()> {
    let index = FIELDS
        .iter()
        .position(|f| f.id == field)
        .context("Unknown rule")?;
    let f = &FIELDS[index];
    let before = *lease
        .snapshot
        .values
        .get(index)
        .context("Incomplete rule snapshot")?;
    ensure!(
        f.valid(before) && f.valid(after) && before != after,
        "Choose a different supported value"
    );
    ensure!(
        m.capture()? == lease.snapshot,
        "Game rules changed; refresh current values"
    );
    let mut i = Intent {
        version: 1,
        pending: true,
        session: lease.session.clone(),
        game: lease.identity.clone(),
        base: lease.snapshot.base,
        owner: lease.snapshot.context.owner,
        field: field.into(),
        before,
        after,
        result: "intent saved before write".into(),
    };
    i.validate()?;
    save(&i)?;
    let mut expected = lease.snapshot.clone();
    expected.values[index] = after;
    let result = (|| {
        ensure!(
            m.capture()? == lease.snapshot,
            "Rules changed after journaling; no write attempted"
        );
        m.write(&lease.snapshot, index, after)?;
        ensure!(
            m.capture()? == expected,
            "Rule read-back or unchanged-neighbor verification failed"
        );
        i.pending = false;
        i.result = "applied and verified".into();
        save(&i)?;
        Ok(())
    })();
    if let Err(e) = result {
        // Never restore over a third value, a changed neighbor, a new owner or another mode.
        let rollback = (|| {
            let current = m.capture()?;
            if current == expected {
                m.write(&expected, index, before)?;
                ensure!(m.capture()? == lease.snapshot, "Rollback read-back failed");
            } else {
                ensure!(
                    current == lease.snapshot,
                    "Ownership/value changed; automatic rollback refused"
                );
            }
            i.pending = false;
            i.result = "failed transaction; original value verified".into();
            save(&i)?;
            Ok::<(), anyhow::Error>(())
        })();
        return match rollback {
            Ok(()) => Err(e.context("Rule not applied; original values verified")),
            Err(r) => Err(anyhow::anyhow!(
                "Rule transaction needs review; further writes blocked until game exit. Cause: {e:#}. Recovery: {r:#}"
            )),
        };
    }
    Ok(())
}

mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    struct Fake {
        s: RefCell<Snapshot>,
        writes: Cell<usize>,
        fail: Cell<u8>,
    }
    impl Access for Fake {
        fn capture(&self) -> Result<Snapshot> {
            Ok(self.s.borrow().clone())
        }
        fn write(&self, expected: &Snapshot, index: usize, value: u32) -> Result<()> {
            ensure!(*self.s.borrow() == *expected, "stale fixture");
            self.writes.set(self.writes.get() + 1);
            self.s.borrow_mut().values[index] = value;
            match self.fail.replace(0) {
                1 => anyhow::bail!("write reported failure after write"),
                2 => {
                    self.s.borrow_mut().values[index] = 99;
                    anyhow::bail!("foreign value")
                }
                3 => {
                    self.s.borrow_mut().values[(index + 1) % 39] = 99;
                }
                _ => {}
            }
            Ok(())
        }
    }
    fn fixture() -> (Fake, Lease) {
        let s = blacktop_native::rules_fixture().unwrap();
        let l = Lease::new(
            Identity {
                pid: 123,
                started: "100".into(),
                path: "C:\\Game\\NBA2K27.exe".into(),
            },
            "a".repeat(48),
            s.clone(),
        );
        (
            Fake {
                s: RefCell::new(s),
                writes: Cell::new(0),
                fail: Cell::new(0),
            },
            l,
        )
    }
    pub(super) fn smoke() -> Result<Value> {
        let (m, l) = fixture();
        let mut saved = vec![];
        apply(&m, &l, "ThreePointShotRestriction", 2, |i| {
            saved.push(i.clone());
            Ok(())
        })?;
        ensure!(
            m.writes.get() == 1 && saved.len() == 2 && saved[0].pending && !saved[1].pending,
            "Rules transaction fixture failed"
        );
        let (bad, original) = fixture();
        bad.fail.set(1);
        ensure!(
            apply(&bad, &original, "ShotClock", 1, |_| Ok(())).is_err()
                && *bad.s.borrow() == original.snapshot
                && bad.writes.get() == 2,
            "Owned rollback fixture failed"
        );
        let (foreign, original) = fixture();
        foreign.fail.set(2);
        let mut held = false;
        ensure!(
            apply(&foreign, &original, "ShotClock", 1, |i| {
                held = i.pending;
                Ok(())
            })
            .is_err()
                && foreign.writes.get() == 1
                && held,
            "Foreign value rollback was not held"
        );
        Ok(
            json!({"pass":true,"source":"isolated sparse-memory transaction fixtures; no live game access","mappedRules":FIELDS.len(),"verifiedWrite":true,"ownedRollback":true,"foreignValueHeld":true,"sample":l.view()}),
        )
    }
    #[test]
    fn mappings_unique_bounded_non_overlapping() {
        assert_eq!(FIELDS.len(), 39);
        for (n, a) in FIELDS.iter().enumerate() {
            assert!(a.offset + a.width as u64 <= 0xA9C);
            assert!([1, 4].contains(&a.width));
            for b in &FIELDS[n + 1..] {
                assert_ne!(a.id, b.id);
                assert!(
                    a.offset + a.width as u64 <= b.offset || b.offset + b.width as u64 <= a.offset
                );
            }
        }
        assert_eq!(FIELDS[27].choices()[0]["label"], "Enabled");
        assert_eq!(FIELDS[26].max, 6);
    }
    #[test]
    fn every_choice_transaction_verified() {
        for field in FIELDS {
            for before in 0..=field.max {
                for after in 0..=field.max {
                    if before == after {
                        continue;
                    }
                    let (m, mut l) = fixture();
                    let index = FIELDS.iter().position(|f| f.id == field.id).unwrap();
                    m.s.borrow_mut().values[index] = before;
                    l.snapshot.values[index] = before;
                    let mut saved = vec![];
                    apply(&m, &l, field.id, after, |i| {
                        saved.push(i.clone());
                        Ok(())
                    })
                    .unwrap();
                    assert_eq!(m.writes.get(), 1);
                    assert!(saved[0].pending);
                    assert!(!saved.last().unwrap().pending);
                }
            }
        }
    }
    #[test]
    fn invalid_unknown_noop_stale_and_failed_intent_never_write() {
        let (m, l) = fixture();
        for (f, v) in [
            ("Unknown", 1),
            ("ShotClock", 2),
            ("ShotClock", 0),
            ("FoulOut", 7),
        ] {
            assert!(apply(&m, &l, f, v, |_| Ok(())).is_err());
        }
        assert!(apply(&m, &l, "ShotClock", 1, |_| anyhow::bail!("disk full")).is_err());
        assert_eq!(m.writes.get(), 0);
        m.s.borrow_mut().values[1] = 1;
        assert!(apply(&m, &l, "ShotClock", 1, |_| Ok(())).is_err());
        assert_eq!(m.writes.get(), 0);
    }
    #[test]
    fn reported_write_failure_rolls_back_only_owned_value() {
        let (m, l) = fixture();
        m.fail.set(1);
        let mut pending = true;
        assert!(
            apply(&m, &l, "ShotClock", 1, |i| {
                pending = i.pending;
                Ok(())
            })
            .is_err()
        );
        assert_eq!(*m.s.borrow(), l.snapshot);
        assert_eq!(m.writes.get(), 2);
        assert!(!pending);
    }
    #[test]
    fn owner_changed_after_intent_blocks_write_and_holds_recovery() {
        let (m, l) = fixture();
        let mut pending = false;
        assert!(
            apply(&m, &l, "ShotClock", 1, |i| {
                pending = i.pending;
                if i.pending {
                    m.s.borrow_mut().context.owner += 8;
                }
                Ok(())
            })
            .is_err()
        );
        assert_eq!(m.writes.get(), 0);
        assert!(pending);
    }
    #[test]
    fn foreign_value_or_neighbor_never_overwritten() {
        for fail in [2, 3] {
            let (m, l) = fixture();
            m.fail.set(fail);
            let mut pending = false;
            assert!(
                apply(&m, &l, "ShotClock", 1, |i| {
                    pending = i.pending;
                    Ok(())
                })
                .is_err()
            );
            assert_eq!(m.writes.get(), 1);
            assert!(pending);
        }
    }
    #[test]
    fn failed_commit_rolls_back_and_journal_failure_holds() {
        let (m, l) = fixture();
        let mut n = 0;
        assert!(
            apply(&m, &l, "ShotClock", 1, |_| {
                n += 1;
                if n > 1 {
                    anyhow::bail!("disk full")
                }
                Ok(())
            })
            .is_err()
        );
        assert_eq!(*m.s.borrow(), l.snapshot);
        assert_eq!(m.writes.get(), 2);
    }
    #[test]
    fn snapshot_token_identity_session_expiry() {
        let (_, mut l) = fixture();
        assert!(l.validate(&l.token, &l.identity, &l.session).is_ok());
        assert!(l.validate("wrong", &l.identity, &l.session).is_err());
        assert!(l.validate(&l.token, &l.identity, "wrong").is_err());
        let mut id = l.identity.clone();
        id.started = "101".into();
        assert!(l.validate(&l.token, &id, &l.session).is_err());
        l.created = Instant::now() - std::time::Duration::from_secs(30);
        assert!(l.validate(&l.token, &l.identity, &l.session).is_err());
    }
    #[test]
    fn pending_journal_cannot_clear_with_live_game_and_malformed_left_intact() {
        let dir = std::env::temp_dir().join(format!("rule-qa-{}", backend::unique_id()));
        fs::create_dir_all(dir.join("Data")).unwrap();
        let (m, l) = fixture();
        apply(&m, &l, "ShotClock", 1, |i| {
            backend::atomic_json(&path(&dir), i)
        })
        .unwrap();
        assert!(!pending(&dir).unwrap());
        let mut i = read_intent(&path(&dir)).unwrap();
        i.pending = true;
        backend::atomic_json(&path(&dir), &i).unwrap();
        let row = crate::session_safety::Process {
            name: "NBA2K27.exe".into(),
            pid: 123,
            parent: 1,
            identity: Some(l.identity),
        };
        assert!(resolve_ended(&dir, &[row]).is_err());
        assert!(pending(&dir).unwrap());
        resolve_ended(&dir, &[]).unwrap();
        assert!(!pending(&dir).unwrap());
        fs::write(path(&dir), b"{}").unwrap();
        assert!(pending(&dir).is_err());
        assert!(resolve_ended(&dir, &[]).is_err());
        assert_eq!(fs::read(path(&dir)).unwrap(), b"{}");
        fs::remove_dir_all(&dir).unwrap();
    }
}
pub fn smoke() -> Result<Value> {
    tests::smoke()
}
