//! One provider owns the complete ball transaction. Immutable durable intent
//! precedes the filename write. A restart never replays a saved memory address.
use crate::{
    blacktop_ball as ball, loader_files,
    session_safety::{self, Identity, Process},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{Read, Write},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
const NAME: &str = "blacktop-ball-session.json";
const SCHEMA: &str = "nba2k27-native-ball-transaction-v1";
const LIMIT: u64 = 16384;
/// Only a pre-write safe-menu change may use this marker. Once publication
/// begins, every error is uncertain and retains ownership until game exit.
#[derive(Debug)]
pub(crate) struct Deferred(pub String);
impl std::fmt::Display for Deferred {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for Deferred {}
pub(crate) trait Access {
    fn capture(&self) -> Result<ball::Snapshot>;
    fn resolve(&self, s: &ball::Snapshot, id: &str) -> Result<(u32, String)>;
    fn write(&self, lease: &ball::Lease, replacement: &[u8], restore: bool) -> Result<()>;
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Journal {
    schema: String,
    session: String,
    game: Identity,
    image: u64,
    selected: String,
    source: u32,
    lease: ball::Lease,
    replacement: Vec<u8>,
}
impl Journal {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == SCHEMA
                && self.session.len() == 48
                && self.session.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid ball receipt schema/session"
        );
        ensure!(
            self.game.pid > 0
                && self.game.started.parse::<u64>().is_ok_and(|v| v > 0)
                && Path::new(&self.game.path).is_absolute()
                && Path::new(&self.game.path)
                    .file_name()
                    .is_some_and(|p| p.to_string_lossy().eq_ignore_ascii_case("NBA2K27.exe"))
                && (65536..0x7fff_0000_0000).contains(&self.image),
            "Invalid ball game identity"
        );
        ball::choice(&self.selected)?;
        ensure!(
            !ball::current(&self.selected)
                && self.source != 0
                && self.source != ball::NORMAL
                && self
                    .selected
                    .strip_prefix("balls/")
                    .is_some_and(|s| ball::filename(&self.replacement)
                        .is_ok_and(|n| n.eq_ignore_ascii_case(s))),
            "Ball receipt differs from its catalog choice"
        );
        ball::plan(&self.lease, &self.replacement)
    }
}
struct Receipt {
    file: File,
    journal: Journal,
}
impl Receipt {
    fn create(root: &Path, journal: Journal) -> Result<Self> {
        journal.validate()?;
        let bytes = serde_json::to_vec_pretty(&journal)?;
        ensure!(bytes.len() as u64 <= LIMIT, "Ball receipt exceeds bounds");
        let mut file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .access_mode(0xc0010000)
            .create_new(true)
            .share_mode(1)
            .custom_flags(0x00200000)
            .open(root.join("Data").join(NAME))?;
        if let Err(e) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
            let _ = loader_files::unlink(&file);
            return Err(e.into());
        }
        Ok(Self { file, journal })
    }
    fn remove(&self) -> Result<()> {
        loader_files::unlink(&self.file)
    }
}
pub(crate) struct Engine {
    root: PathBuf,
    session: String,
    game: Identity,
    owned: Option<Receipt>,
    halted: Option<String>,
    _pins: Vec<loader_files::Directories>,
}
impl Engine {
    pub fn new(root: &Path, session: &str, game: &Identity) -> Result<Self> {
        ensure!(
            !pending(root)?,
            "Ball receipt pending; never adopt it into a running game"
        );
        ensure!(
            session.len() == 48 && session.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid ball session"
        );
        Ok(Self {
            root: root.into(),
            session: session.into(),
            game: game.clone(),
            owned: None,
            halted: None,
            _pins: vec![
                loader_files::Directories::pin(root)?,
                loader_files::Directories::pin(&root.join("Data"))?,
            ],
        })
    }
    pub fn selected(&self) -> &str {
        self.owned
            .as_ref()
            .map_or("current", |r| r.journal.selected.as_str())
    }
    pub fn halted(&self) -> Option<&str> {
        self.halted.as_deref()
    }
    pub fn hold<T>(&mut self, e: anyhow::Error) -> Result<T> {
        let text = format!("Ball ownership held; close the game before retrying: {e:#}");
        self.halted = Some(text.clone());
        Err(anyhow::anyhow!(text))
    }
    pub fn observe(&self, s: &ball::Snapshot) -> Result<()> {
        ensure!(self.halted.is_none(), "Ball session held");
        if let Some(o) = &self.owned {
            ensure!(s.image == o.journal.image, "Ball image changed");
            s.owns(&o.journal.lease, &o.journal.replacement)?;
        }
        Ok(())
    }
    pub fn apply(&mut self, a: &impl Access, id: &str) -> Result<()> {
        ensure!(
            self.halted.is_none() && self.owned.is_none(),
            "Restore the owned ball before another preparation"
        );
        ball::choice(id)?;
        if ball::current(id) {
            return Ok(());
        }
        let s = a.capture()?;
        if !s.can_prepare() {
            return Err(Deferred("Wait for unloaded Blacktop player selection".into()).into());
        }
        let (source, name) = a.resolve(&s, id)?;
        let replacement = ball::replacement(&s.record, &name)?;
        let fresh = a.capture()?;
        fresh.owns(&s.lease, &s.record)?;
        if fresh.context != s.context || !fresh.can_prepare() {
            return Err(Deferred("Ball menu changed before durable intent".into()).into());
        }
        let receipt = Receipt::create(
            &self.root,
            Journal {
                schema: SCHEMA.into(),
                session: self.session.clone(),
                game: self.game.clone(),
                image: s.image,
                selected: id.into(),
                source,
                lease: s.lease,
                replacement,
            },
        )?;
        self.owned = Some(receipt);
        let result = (|| {
            let j = &self.owned.as_ref().unwrap().journal;
            a.write(&j.lease, &j.replacement, false)?;
            self.observe(&a.capture()?)
        })();
        match result {
            Ok(()) => Ok(()),
            Err(e) if e.is::<Deferred>() => {
                // No write was entered. Release only our fresh immutable intent;
                // this does not restore or touch any game memory.
                if let Err(remove) = self.owned.as_ref().unwrap().remove() {
                    return self.hold(remove);
                }
                self.owned.take();
                Err(e)
            }
            Err(e) => self.hold(e),
        }
    }
    pub fn restore(&mut self, a: &impl Access) -> Result<()> {
        ensure!(self.halted.is_none(), "Ball session held until game exit");
        let Some(o) = &self.owned else { return Ok(()) };
        let result = (|| {
            let j = &o.journal;
            let s = a.capture()?;
            self.observe(&s)?;
            if !s.can_restore() {
                return Err(Deferred("Wait for the ball to unload at a safe menu".into()).into());
            }
            a.write(&j.lease, &j.replacement, true)?;
            a.capture()?.owns(&j.lease, &j.lease.original)?;
            o.remove()
        })();
        match result {
            Ok(()) => {
                self.owned.take();
                Ok(())
            }
            Err(e) if e.is::<Deferred>() => Err(e),
            Err(e) => self.hold(e),
        }
    }
    pub fn game_ended(&mut self, closed: impl Fn() -> Result<()>) -> Result<()> {
        closed()?;
        if let Some(o) = &self.owned {
            closed()?;
            o.remove()?;
        }
        self.owned.take();
        self.halted = None;
        Ok(())
    }
}
pub fn pending(root: &Path) -> Result<bool> {
    Ok(root.join("Data").join(NAME).try_exists()?)
}
fn resolve_with(
    root: &Path,
    session: &str,
    game: &Identity,
    closed: impl Fn() -> Result<()>,
) -> Result<()> {
    let _pins = [
        loader_files::Directories::pin(root)?,
        loader_files::Directories::pin(&root.join("Data"))?,
    ];
    closed()?;
    let path = root.join("Data").join(NAME);
    if !path.try_exists()? {
        return Ok(());
    }
    let mut file = fs::OpenOptions::new()
        .read(true)
        .access_mode(0x80010000)
        .share_mode(1)
        .custom_flags(0x00200000)
        .open(&path)?;
    let m = file.metadata()?;
    ensure!(
        m.is_file() && m.len() <= LIMIT && m.file_attributes() & 0x400 == 0,
        "Invalid ball receipt; left untouched"
    );
    let mut bytes = Vec::new();
    (&mut file).take(LIMIT + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= LIMIT, "Ball receipt exceeds bounds");
    let j: Journal =
        serde_json::from_slice(&bytes).context("Damaged ball receipt; memory is never replayed")?;
    j.validate()?;
    ensure!(
        j.session == session && j.game == *game,
        "Ball receipt belongs to another session/game; left untouched"
    );
    closed()?;
    loader_files::unlink(&file)
}
pub fn resolve_ended(root: &Path, rows: &[Process], session: &str, game: &Identity) -> Result<()> {
    let quiet = |p: &[Process]| -> Result<()> {
        ensure!(
            !p.iter().any(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe")
                || session_safety::protected_process(&p.name)),
            "Close game/protected processes before clearing ball receipt"
        );
        Ok(())
    };
    quiet(rows)?;
    resolve_with(root, session, game, || quiet(&session_safety::processes()?))
}
pub(crate) struct NativeAccess {
    writer: crate::blacktop_native::BallWriter,
    game: PathBuf,
}
impl NativeAccess {
    pub fn open(game: &Path, id: &Identity, owner: &Identity) -> Result<Self> {
        Ok(Self {
            writer: crate::blacktop_native::BallWriter::open(id, game, owner)?,
            game: game.into(),
        })
    }
}
impl Access for NativeAccess {
    fn capture(&self) -> Result<ball::Snapshot> {
        ball::capture(self.writer.reader())
    }
    fn resolve(&self, s: &ball::Snapshot, id: &str) -> Result<(u32, String)> {
        let found = ball::resolve(self.writer.reader(), s, id)?;
        let file = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .custom_flags(0x00200000)
            .open(self.game.join("manifest"))?;
        let before = file.metadata()?;
        ensure!(
            before.is_file()
                && (1..=64 * 1024 * 1024).contains(&before.len())
                && before.file_attributes() & 0x400 == 0,
            "Stock ball manifest is unavailable or linked"
        );
        let mut bytes = Vec::new();
        (&file).take(64 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 == before.len()
                && file.metadata()?.last_write_time() == before.last_write_time(),
            "Stock manifest changed"
        );
        let wanted = format!("balls/{}", found.1);
        ensure!(
            std::str::from_utf8(&bytes)?.lines().any(|l| l
                .split(',')
                .next()
                .is_some_and(|s| s.replace('\\', "/").eq_ignore_ascii_case(&wanted))),
            "Selected ball is absent from the stock manifest"
        );
        Ok(found)
    }
    fn write(&self, l: &ball::Lease, r: &[u8], restore: bool) -> Result<()> {
        self.writer.write(l, r, restore)
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use std::cell::{Cell, RefCell};
    pub struct Fixture {
        pub root: PathBuf,
    }
    impl Fixture {
        pub fn new() -> Self {
            let root = std::env::temp_dir()
                .join(format!("rust-ball-session-{}", crate::backend::unique_id()));
            fs::create_dir_all(root.join("Data")).unwrap();
            Self { root }
        }
        pub fn engine(&self) -> Engine {
            Engine::new(&self.root, &"a".repeat(48), &identity()).unwrap()
        }
        pub fn access(&self) -> Fake {
            Fake {
                root: self.root.clone(),
                snapshot: RefCell::new(ball::capture(&ball::test_support::fixture()).unwrap()),
                writes: Cell::new(0),
                fail_at: Cell::new(0),
                partial: Cell::new(false),
                source: Cell::new(true),
                deferred: Cell::new(false),
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    pub fn identity() -> Identity {
        Identity {
            pid: 123,
            started: "133000000000000000".into(),
            path: r"C:\Fixture\NBA2K27.exe".into(),
        }
    }
    pub struct Fake {
        pub root: PathBuf,
        pub snapshot: RefCell<ball::Snapshot>,
        pub writes: Cell<usize>,
        pub fail_at: Cell<usize>,
        pub partial: Cell<bool>,
        pub source: Cell<bool>,
        pub deferred: Cell<bool>,
    }
    impl Fake {
        pub fn mode(&self, hash: u32, stage: u32) {
            let mut f = ball::test_support::fixture();
            f.mode(hash, stage);
            self.snapshot.borrow_mut().context = ball::capture(&f).unwrap().context;
        }
        pub fn loaded(&self, on: bool) {
            let mut s = self.snapshot.borrow_mut();
            s.references = if on { 1 } else { 0 };
            s.resource.fill(0);
            if on {
                s.resource[16] = 1;
            }
        }
    }
    impl Access for Fake {
        fn capture(&self) -> Result<ball::Snapshot> {
            let mut s = self.snapshot.borrow().clone();
            s.lease.original = s.record.clone();
            Ok(s)
        }
        fn resolve(&self, _: &ball::Snapshot, id: &str) -> Result<(u32, String)> {
            ensure!(self.source.get(), "Selected ball unavailable");
            Ok((
                0x11223344,
                id.strip_prefix("balls/")
                    .context("Missing ball path")?
                    .into(),
            ))
        }
        fn write(&self, lease: &ball::Lease, replacement: &[u8], restore: bool) -> Result<()> {
            if self.deferred.get() {
                return Err(Deferred("Synthetic menu transition before write".into()).into());
            }
            ensure!(pending(&self.root)?, "Intent must precede every write");
            ball::plan(lease, replacement)?;
            let s = self.capture()?;
            let (expected, next) = if restore {
                (replacement, lease.original.as_slice())
            } else {
                (lease.original.as_slice(), replacement)
            };
            s.owns(lease, expected)?;
            ensure!(
                if restore {
                    s.can_restore()
                } else {
                    s.can_prepare()
                },
                "Unsafe fixture write refused"
            );
            self.writes.set(self.writes.get() + 1);
            if self.fail_at.get() == self.writes.get() {
                if self.partial.get() {
                    self.snapshot.borrow_mut().record[ball::FIELD] ^= 1;
                }
                anyhow::bail!("Synthetic filename write/readback failure");
            }
            let len = ball::field_len(&lease.original, replacement)?;
            self.snapshot.borrow_mut().record[ball::FIELD..ball::FIELD + len]
                .copy_from_slice(&next[ball::FIELD..ball::FIELD + len]);
            Ok(())
        }
    }
}
#[cfg(test)]
mod tests {
    use super::{test_support::*, *};
    #[test]
    fn immutable_intent_precedes_only_filename_write_and_verified_restore() {
        let f = Fixture::new();
        let a = f.access();
        let original = a.capture().unwrap().record;
        let mut e = f.engine();
        e.apply(&a, &ball::test_support::id()).unwrap();
        assert!(pending(&f.root).unwrap());
        assert_eq!(a.writes.get(), 1);
        let next = a.capture().unwrap().record;
        assert_eq!(&next[..ball::FIELD], &original[..ball::FIELD]);
        assert_eq!(next[ball::STRIDE - 4], 98);
        e.restore(&a).unwrap();
        assert_eq!(a.capture().unwrap().record, original);
        assert!(!pending(&f.root).unwrap());
        assert_eq!(a.writes.get(), 2);
    }
    #[test]
    fn bad_choice_source_loaded_state_and_foreign_receipt_never_write() {
        let f = Fixture::new();
        let a = f.access();
        let mut e = f.engine();
        assert!(e.apply(&a, "balls/ball_unknown.iff").is_err());
        a.source.set(false);
        assert!(e.apply(&a, &ball::test_support::id()).is_err());
        a.source.set(true);
        a.loaded(true);
        assert!(e.apply(&a, &ball::test_support::id()).is_err());
        a.loaded(false);
        fs::write(f.root.join("Data").join(NAME), b"foreign").unwrap();
        assert!(e.apply(&a, &ball::test_support::id()).is_err());
        assert_eq!(a.writes.get(), 0);
        assert!(e.halted().is_none());
        assert_eq!(
            fs::read(f.root.join("Data").join(NAME)).unwrap(),
            b"foreign"
        );
    }
    #[test]
    fn uncertain_apply_and_restore_failures_hold_without_blind_retries() {
        for restoring in [false, true] {
            for partial in [false, true] {
                let f = Fixture::new();
                let a = f.access();
                let mut e = f.engine();
                if restoring {
                    e.apply(&a, &ball::test_support::id()).unwrap();
                }
                a.fail_at.set(if restoring { 2 } else { 1 });
                a.partial.set(partial);
                assert!(
                    if restoring {
                        e.restore(&a)
                    } else {
                        e.apply(&a, &ball::test_support::id())
                    }
                    .is_err()
                );
                assert!(e.halted().is_some());
                let writes = a.writes.get();
                assert!(e.restore(&a).is_err());
                assert!(e.apply(&a, &ball::test_support::id()).is_err());
                assert_eq!(a.writes.get(), writes);
                assert!(pending(&f.root).unwrap());
                assert!(e.game_ended(|| anyhow::bail!("Game alive")).is_err());
                e.game_ended(|| Ok(())).unwrap();
                assert!(!pending(&f.root).unwrap());
                assert_eq!(a.writes.get(), writes);
            }
        }
    }
    #[test]
    fn changed_owner_or_metadata_never_restores_a_foreign_record() {
        for field in [0, 1, 2] {
            let f = Fixture::new();
            let a = f.access();
            let mut e = f.engine();
            e.apply(&a, &ball::test_support::id()).unwrap();
            {
                let mut s = a.snapshot.borrow_mut();
                match field {
                    0 => s.lease.allocation += 4096,
                    1 => s.lease.registry += 8,
                    _ => s.record[12] ^= 1,
                }
            }
            assert!(e.restore(&a).is_err());
            assert_eq!(a.writes.get(), 1);
            assert!(pending(&f.root).unwrap());
        }
    }
    #[test]
    fn restart_never_replays_memory_and_only_exact_ended_owner_clears_receipt() {
        let f = Fixture::new();
        let a = f.access();
        let mut e = f.engine();
        e.apply(&a, &ball::test_support::id()).unwrap();
        drop(e);
        assert!(Engine::new(&f.root, &"a".repeat(48), &identity()).is_err());
        assert!(resolve_with(&f.root, &"b".repeat(48), &identity(), || Ok(())).is_err());
        assert!(
            resolve_with(&f.root, &"a".repeat(48), &identity(), || anyhow::bail!(
                "unknown game presence"
            ))
            .is_err()
        );
        assert_eq!(a.writes.get(), 1);
        resolve_with(&f.root, &"a".repeat(48), &identity(), || Ok(())).unwrap();
        assert!(!pending(&f.root).unwrap());
        assert_eq!(a.writes.get(), 1);
        fs::write(f.root.join("Data").join(NAME), b"damaged").unwrap();
        assert!(resolve_with(&f.root, &"a".repeat(48), &identity(), || Ok(())).is_err());
        assert_eq!(
            fs::read(f.root.join("Data").join(NAME)).unwrap(),
            b"damaged"
        );
    }
}
