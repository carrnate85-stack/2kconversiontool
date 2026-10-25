//! Explicit session-bound arena transactions. Immutable durable intent precedes
//! memory writes; restarts NEVER replay a memory address from this receipt.
#![allow(dead_code)]
use crate::{
    arena_ball_v2_adapter::{NotPublished, not_published},
    blacktop_arena::{self as arena, Choice, Record, STOCK, Snapshot},
    loader_files,
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
const NAME: &str = "blacktop-arena-session.json";
const SCHEMA: &str = "nba2k27-native-arena-transaction-v1";
const LIMIT: u64 = 16384;
pub(crate) trait Access {
    fn capture(&self, c: &Choice, expected: &[u8; 20]) -> Result<Snapshot>;
    fn verify(&self, r: &Record, expected: &[u8]) -> Result<()>;
    fn template(&self, r: &Record, expected: &[u8], next: &[u8]) -> Result<()>;
    fn mapping(&self, expected: &[u8; 20], next: &[u8; 20]) -> Result<()>;
    fn assets(&self, c: &Choice) -> Result<()>;
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Journal {
    schema: String,
    session: String,
    game: Identity,
    image: u64,
    selected: String,
    mapping: [u8; 20],
    record: Option<Record>,
    replacement: Option<Vec<u8>>,
}
impl Journal {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == SCHEMA
                && self.session.len() == 48
                && self.session.bytes().all(|b| b.is_ascii_hexdigit()),
            "Unsupported arena receipt schema/session"
        );
        ensure!(
            self.game.pid > 0
                && self.game.started.parse::<u64>().is_ok_and(|n| n > 0)
                && Path::new(&self.game.path).is_absolute()
                && Path::new(&self.game.path)
                    .file_name()
                    .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case("NBA2K27.exe"))
                && self.image >= 65536
                && self.image < 0x7fff_0000_0000,
            "Invalid arena game owner"
        );
        let c = arena::choice(&self.selected)?;
        ensure!(
            c.id != "default" && self.mapping == arena::mapping(c.arena_id)?,
            "Arena receipt mapping differs from its exact catalog choice"
        );
        let r = self
            .record
            .as_ref()
            .context("Arena receipt has no verified roster record")?;
        ensure!(
            Some(r.id as u32) == c.arena_id
                && r.fields.len() == 78
                && r.header.len() == 16
                && r.address >= 65536
                && r.address < 0x7fff_0000_0000
                && r.owner.checked_sub(r.allocation) == Some(0x50)
                && r.header_address.checked_sub(r.owner) == Some(0x60),
            "Arena roster record ownership is invalid"
        );
        let count = u64::from_le_bytes(r.header[..8].try_into()?);
        let table = u64::from_le_bytes(r.header[8..].try_into()?);
        ensure!(
            (1..=2048).contains(&count)
                && r.address
                    .checked_sub(table)
                    .is_some_and(|off| off % 0x1278 == 0 && off / 0x1278 < count),
            "Arena receipt record is outside its directory"
        );
        if c.template_arena_id.is_some() {
            let target = arena::choice_fields(&c)?;
            ensure!(
                r.id == 700 && self.replacement.as_ref() == Some(&target),
                "Arena template receipt differs from the allowlisted fields"
            );
        } else {
            ensure!(
                self.replacement.is_none(),
                "Non-template arena receipt attempts a roster write"
            );
        }
        Ok(())
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
        ensure!(bytes.len() as u64 <= LIMIT, "Arena receipt exceeds bounds");
        let path = root.join("Data").join(NAME);
        let mut file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .access_mode(0xc0010000)
            .create_new(true)
            .share_mode(1)
            .custom_flags(0x00200000)
            .open(&path)
            .with_context(|| format!("Creating immutable arena receipt {}", path.display()))?;
        if let Err(e) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
            // Only our just-created, still-open file. No game write has begun.
            let _ = loader_files::unlink(&file);
            return Err(e.into());
        }
        Ok(Self { file, journal })
    }
    fn remove(&self) -> Result<()> {
        // This handle was created by us and denies other writers/deletion for
        // the entire lease. A later user path cannot redirect this operation.
        loader_files::unlink(&self.file)
    }
}
pub(crate) struct Engine {
    root: PathBuf,
    session: String,
    game: Identity,
    owned: Option<Receipt>,
    halted: Option<String>,
    retryable: bool,
    _pins: Vec<loader_files::Directories>,
}
impl Engine {
    pub fn new(root: &Path, session: &str, game: &Identity) -> Result<Self> {
        ensure!(
            !pending(root)?,
            "An arena receipt is pending; never adopt or replay it into a live game"
        );
        ensure!(
            session.len() == 48 && session.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid arena owner session"
        );
        let pins = vec![
            loader_files::Directories::pin(root)?,
            loader_files::Directories::pin(&root.join("Data"))?,
        ];
        Ok(Self {
            root: root.into(),
            session: session.into(),
            game: game.clone(),
            owned: None,
            halted: None,
            retryable: false,
            _pins: pins,
        })
    }
    pub fn selected(&self) -> &str {
        self.owned
            .as_ref()
            .map_or("default", |o| o.journal.selected.as_str())
    }
    pub fn halted(&self) -> Option<&str> {
        self.halted.as_deref()
    }
    /// The last apply failed before any public write and released its receipt.
    pub fn retryable(&self) -> bool {
        self.retryable
    }
    fn hold<T>(&mut self, error: anyhow::Error) -> Result<T> {
        let text = format!("Arena ownership held; close the game before retrying: {error:#}");
        self.halted = Some(text.clone());
        Err(anyhow::anyhow!(text))
    }
    pub fn apply(&mut self, a: &impl Access, id: &str) -> Result<()> {
        ensure!(
            self.halted.is_none(),
            "{}",
            self.halted.as_deref().unwrap_or("Arena session is held")
        );
        self.retryable = false;
        let c = arena::choice(id)?;
        if self.selected() == id {
            let expected = self.owned.as_ref().map_or(&STOCK, |o| &o.journal.mapping);
            a.capture(&arena::choice("default")?, expected)?;
            let mut reset = false;
            if let Some(o) = &self.owned {
                let r = o.journal.record.as_ref().unwrap();
                if let Err(error) = a.verify(r, o.journal.replacement.as_deref().unwrap_or(&r.fields)) {
                    // The game can restore the exact original names when it
                    // refreshes this roster. Never adopt other bytes or owners.
                    if !crate::custom_blacktop::enabled() || !id.starts_with("custom-") || o.journal.replacement.is_none() || a.verify(r, &r.fields).is_err() { return Err(error); }
                    reset = true;
                }
            }
            if !reset { return Ok(()); }
        }
        self.restore(a)?;
        if id == "default" {
            return Ok(());
        }
        a.assets(&c)?;
        let s = a.capture(&c, &STOCK)?;
        let replacement = if c.template_arena_id.is_some() {
            Some(arena::choice_fields(&c)?)
        } else {
            None
        };
        let journal = Journal {
            schema: SCHEMA.into(),
            session: self.session.clone(),
            game: self.game.clone(),
            image: s.image,
            selected: id.into(),
            mapping: arena::mapping(c.arena_id)?,
            record: s.record,
            replacement,
        };
        let receipt = Receipt::create(&self.root, journal)?;
        self.owned = Some(receipt); // Ownership is durable BEFORE any memory write.
        let result = (|| -> Result<()> {
            let j = &self.owned.as_ref().unwrap().journal;
            let r = j.record.as_ref().unwrap();
            a.capture(&arena::choice("default")?, &STOCK)
                .and_then(|_| a.verify(r, &r.fields))
                .map_err(|e| e.context(NotPublished))?;
            if let Some(next) = &j.replacement {
                a.template(r, &r.fields, next)?;
            }
            a.mapping(&STOCK, &j.mapping)?;
            a.capture(&arena::choice("default")?, &j.mapping)?;
            a.verify(r, j.replacement.as_deref().unwrap_or(&r.fields))?;
            Ok(())
        })();
        match result {
            Ok(()) => Ok(()),
            Err(e) => {
                // A context/neighbor change detected before publication leaves
                // the stock mapping and exact original record in place. Prove
                // that with a fresh capture, release the receipt and let the
                // runner retry. Anything else (a public write, foreign data,
                // an unverifiable state) stays held until game exit.
                let untouched = not_published(&e) && {
                    let j = &self.owned.as_ref().unwrap().journal;
                    let r = j.record.as_ref().unwrap();
                    a.capture(&arena::choice("default")?, &STOCK).is_ok()
                        && a.verify(r, &r.fields).is_ok()
                };
                if untouched && self.owned.as_ref().unwrap().remove().is_ok() {
                    self.owned = None;
                    self.retryable = true;
                    return Err(e.context("Arena not published; original verified, retrying"));
                }
                self.hold(e)
            }
        }
    }
    pub fn restore(&mut self, a: &impl Access) -> Result<()> {
        ensure!(
            self.halted.is_none(),
            "Arena session is held until game exit"
        );
        let Some(o) = self.owned.as_ref() else {
            return Ok(());
        };
        let result = (|| -> Result<()> {
            let j = &o.journal;
            let r = j.record.as_ref().unwrap();
            a.capture(&arena::choice("default")?, &j.mapping)?;
            let already_original = crate::custom_blacktop::enabled() && j.selected.starts_with("custom-") && j.replacement.is_some() && a.verify(r, &r.fields).is_ok();
            if !already_original { a.verify(r, j.replacement.as_deref().unwrap_or(&r.fields))?; }
            a.mapping(&j.mapping, &STOCK)?;
            if let Some(replacement) = &j.replacement && !already_original {
                a.template(r, replacement, &r.fields)?;
            }
            a.capture(&arena::choice("default")?, &STOCK)?;
            a.verify(r, &r.fields)?;
            o.remove()?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.owned.take();
                Ok(())
            }
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
        self.retryable = false;
        Ok(())
    }
}
pub fn pending(root: &Path) -> Result<bool> {
    Ok(root.join("Data").join(NAME).try_exists()?)
}
pub fn resolve_ended(root: &Path, rows: &[Process], session: &str, game: &Identity) -> Result<()> {
    ensure!(
        !rows
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe")
                || session_safety::protected_process(&p.name)),
        "Close the game/protected process before clearing an ended arena receipt"
    );
    resolve_with(root, session, game, || {
        ensure!(
            !session_safety::processes()?
                .iter()
                .any(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe")
                    || session_safety::protected_process(&p.name)),
            "Game presence changed; arena receipt retained"
        );
        Ok(())
    })
}
fn resolve_with(
    root: &Path,
    session: &str,
    game: &Identity,
    closed: impl Fn() -> Result<()>,
) -> Result<()> {
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
        "Invalid arena receipt; left untouched"
    );
    let mut bytes = Vec::new();
    (&mut file).take(LIMIT + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= LIMIT, "Arena receipt exceeds bounds");
    let j: Journal = serde_json::from_slice(&bytes)
        .context("Damaged arena receipt; memory is never replayed")?;
    j.validate()?;
    ensure!(
        j.session == session && j.game == *game,
        "Arena receipt belongs to another session/game; left untouched"
    );
    // Fresh global snapshot immediately before deletion; never replay memory.
    closed()?;
    loader_files::unlink(&file)
}
pub(crate) struct NativeAccess {
    writer: crate::blacktop_native::ArenaWriter,
    game: PathBuf,
}
impl NativeAccess {
    pub fn boundary(&self) -> Result<String> {
        Ok(self.writer.reader().snapshot()?.boundary)
    }
    pub fn open(game: &Path, id: &Identity, owner: &Identity) -> Result<Self> {
        Ok(Self {
            writer: crate::blacktop_native::ArenaWriter::open(id, game, owner)?,
            game: game.into(),
        })
    }
}
impl Access for NativeAccess {
    fn capture(&self, c: &Choice, expected: &[u8; 20]) -> Result<Snapshot> {
        arena::capture(self.writer.reader(), c, expected)
    }
    fn verify(&self, r: &Record, expected: &[u8]) -> Result<()> {
        arena::revalidate(self.writer.reader(), r, expected)
    }
    fn template(&self, r: &Record, expected: &[u8], next: &[u8]) -> Result<()> {
        self.writer.write_template(r, expected, next)
    }
    fn mapping(&self, expected: &[u8; 20], next: &[u8; 20]) -> Result<()> {
        self.writer.write_mapping(expected, next)
    }
    fn assets(&self, c: &Choice) -> Result<()> {
        if c.id.starts_with("custom-") { return crate::custom_blacktop::verify_assets(c); }
        if c.template_arena_id.is_none() {
            return Ok(());
        }
        let file = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .custom_flags(0x00200000)
            .open(self.game.join("manifest"))?;
        let before = file.metadata()?;
        ensure!(
            before.is_file()
                && before.len() > 0
                && before.len() <= 64 * 1024 * 1024
                && before.file_attributes() & 0x400 == 0,
            "Stock arena manifest is missing, linked or too large"
        );
        let mut bytes = Vec::new();
        (&file).take(64 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 == before.len()
                && file.metadata()?.last_write_time() == before.last_write_time(),
            "Arena manifest changed during verification"
        );
        let text = std::str::from_utf8(&bytes)?;
        let wanted = [
            format!("levels/{}.iff", c.file.as_deref().unwrap()),
            format!("levels/{}.iff", c.floor_file.as_deref().unwrap()),
        ];
        for asset in wanted {
            ensure!(
                text.lines().any(|l| l
                    .split(',')
                    .next()
                    .is_some_and(|name| name.replace('\\', "/").eq_ignore_ascii_case(&asset))),
                "Arena template asset is absent from the stock manifest: {asset}"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend;
    use std::cell::{Cell, RefCell};
    struct Fixture {
        root: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("rust-arena-session-{}", backend::unique_id()));
            fs::create_dir_all(root.join("Data")).unwrap();
            Self { root }
        }
        fn engine(&self) -> Engine {
            Engine::new(&self.root, &"a".repeat(48), &identity()).unwrap()
        }
        fn access(&self) -> Fake {
            Fake {
                root: self.root.clone(),
                map: RefCell::new(STOCK),
                fields: RefCell::new(
                    arena::template_fields("arena_700_int", "arena_700_int_floor").unwrap(),
                ),
                writes: Cell::new(0),
                fail_at: Cell::new(0),
                selection: Cell::new(true),
                assets: Cell::new(true),
                blips: Cell::new(0),
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    fn identity() -> Identity {
        Identity {
            pid: 123,
            started: "133000000000000000".into(),
            path: r"C:\Fixture\NBA2K27.exe".into(),
        }
    }
    struct Fake {
        root: PathBuf,
        map: RefCell<[u8; 20]>,
        fields: RefCell<Vec<u8>>,
        writes: Cell<usize>,
        fail_at: Cell<usize>,
        selection: Cell<bool>,
        assets: Cell<bool>,
        /// Mapping calls that fail at the pre-publication mode check.
        blips: Cell<usize>,
    }
    impl Fake {
        fn record(&self, c: &Choice) -> Record {
            Record {
                address: 0x300001000,
                header_address: 0x3000000b0,
                root: 0x280000000,
                owner: 0x300000050,
                allocation: 0x300000000,
                header: [1u64.to_le_bytes(), 0x300001000u64.to_le_bytes()].concat(),
                fields: if c.template_arena_id.is_some() {
                    arena::template_fields("arena_700_int", "arena_700_int_floor").unwrap()
                } else {
                    arena::template_fields("arena_000_int", "arena_000_int_floor").unwrap()
                },
                id: c.arena_id.unwrap() as u16,
            }
        }
        fn attempted(&self) -> Result<()> {
            let bytes = fs::read(self.root.join("Data").join(NAME))
                .context("Missing durable intent before write")?;
            let receipt: Journal = serde_json::from_slice(&bytes)?;
            receipt.validate()?;
            self.writes.set(self.writes.get() + 1);
            ensure!(
                self.fail_at.get() != self.writes.get(),
                "Synthetic write failure"
            );
            Ok(())
        }
    }
    impl Access for Fake {
        fn capture(&self, c: &Choice, expected: &[u8; 20]) -> Result<Snapshot> {
            ensure!(
                self.selection.get() && *self.map.borrow() == *expected,
                "Synthetic selection/mapping changed"
            );
            Ok(Snapshot {
                image: 0x140000000,
                context: crate::blacktop_native::rules_fixture()?.context,
                mapping: *expected,
                record: if c.id == "default" {
                    None
                } else {
                    Some(self.record(c))
                },
            })
        }
        fn verify(&self, r: &Record, expected: &[u8]) -> Result<()> {
            if r.id == 700 {
                ensure!(
                    *self.fields.borrow() == expected,
                    "Synthetic template changed"
                );
            }
            Ok(())
        }
        fn template(&self, r: &Record, expected: &[u8], next: &[u8]) -> Result<()> {
            self.verify(r, expected)?;
            self.attempted()?;
            *self.fields.borrow_mut() = next.to_vec();
            Ok(())
        }
        fn mapping(&self, expected: &[u8; 20], next: &[u8; 20]) -> Result<()> {
            ensure!(*self.map.borrow() == *expected, "Synthetic mapping changed");
            if self.blips.get() > 0 {
                self.blips.set(self.blips.get() - 1);
                return Err(
                    anyhow::anyhow!("Arena mapping/context changed before publication")
                        .context(NotPublished),
                );
            }
            self.attempted()?;
            *self.map.borrow_mut() = *next;
            Ok(())
        }
        fn assets(&self, _: &Choice) -> Result<()> {
            ensure!(self.assets.get(), "Synthetic missing asset");
            Ok(())
        }
    }
    #[test]
    #[cfg(feature = "dev-model-preview")]
    fn custom_arena_refresh_recovers_only_exact_original_names_at_selection() {
        let f = Fixture::new(); let a = f.access(); let mut e = f.engine();
        fs::create_dir(f.root.join("levels")).unwrap();
        fs::write(f.root.join("levels/test.iff"), b"fixture").unwrap();
        let saved = crate::custom_blacktop::request(&f.root, &f.root, &serde_json::json!({"action":"add","name":"Refresh fixture","baseId":"blacktop-stock","arenaSource":"levels/test.iff","floorSource":null})).unwrap();
        let id = saved["id"].as_str().unwrap();
        let original = a.fields.borrow().clone();
        e.apply(&a, id).unwrap(); let applied = a.fields.borrow().clone();
        *a.fields.borrow_mut() = original.clone(); // the game refreshes its roster
        a.selection.set(false); let writes = a.writes.get();
        assert!(e.apply(&a, id).is_err()); assert_eq!(a.writes.get(), writes);
        a.selection.set(true); e.apply(&a, id).unwrap();
        assert_eq!(*a.fields.borrow(), applied);
        let writes = a.writes.get(); e.apply(&a, id).unwrap(); assert_eq!(a.writes.get(), writes);
        a.fields.borrow_mut()[0] = b'x';
        assert!(e.apply(&a, id).is_err()); assert_eq!(a.writes.get(), writes);
        *a.fields.borrow_mut() = original;
        e.apply(&a, "default").unwrap();
        assert_eq!(*a.map.borrow(), STOCK); assert!(!pending(&f.root).unwrap());
    }
    #[test]
    fn immutable_intent_precedes_writes_id_zero_noop_change_and_restore() {
        let f = Fixture::new();
        let a = f.access();
        let mut e = f.engine();
        e.apply(&a, "arena_000_int").unwrap();
        assert_eq!(a.writes.get(), 1);
        assert!(pending(&f.root).unwrap());
        let before = fs::read(f.root.join("Data").join(NAME)).unwrap();
        assert!(fs::write(f.root.join("Data").join(NAME), b"foreign").is_err());
        e.apply(&a, "arena_000_int").unwrap();
        assert_eq!(a.writes.get(), 1);
        assert_eq!(fs::read(f.root.join("Data").join(NAME)).unwrap(), before);
        e.apply(&a, "arena_brickleyfacility_int").unwrap();
        assert_eq!(a.writes.get(), 4);
        assert_eq!(e.selected(), "arena_brickleyfacility_int");
        e.apply(&a, "default").unwrap();
        assert_eq!(a.writes.get(), 6);
        assert_eq!(*a.map.borrow(), STOCK);
        assert!(!pending(&f.root).unwrap());
    }
    #[test]
    fn failed_intent_or_preflight_never_writes_and_foreign_receipt_is_preserved() {
        let f = Fixture::new();
        let a = f.access();
        let mut e = f.engine();
        a.assets.set(false);
        assert!(e.apply(&a, "arena_brickleyfacility_int").is_err());
        assert_eq!(a.writes.get(), 0);
        assert!(!pending(&f.root).unwrap());
        a.assets.set(true);
        fs::write(f.root.join("Data").join(NAME), b"foreign receipt").unwrap();
        assert!(e.apply(&a, "arena_000_int").is_err());
        assert_eq!(a.writes.get(), 0);
        assert_eq!(
            fs::read(f.root.join("Data").join(NAME)).unwrap(),
            b"foreign receipt"
        );
    }
    #[test]
    fn each_apply_restore_failure_holds_without_blind_memory_replay() {
        for fail in 1..=4 {
            let f = Fixture::new();
            let a = f.access();
            let mut e = f.engine();
            a.fail_at.set(fail);
            let prepared = e.apply(&a, "arena_brickleyfacility_int");
            if prepared.is_ok() {
                assert!(e.apply(&a, "default").is_err());
            } else {
                assert!(fail <= 2);
            }
            assert!(e.halted().is_some());
            assert!(pending(&f.root).unwrap());
            let writes = a.writes.get();
            assert!(e.apply(&a, "arena_000_int").is_err());
            assert_eq!(a.writes.get(), writes);
            assert!(e.game_ended(|| anyhow::bail!("Game still live")).is_err());
            assert!(pending(&f.root).unwrap());
            e.game_ended(|| Ok(())).unwrap();
            assert!(!pending(&f.root).unwrap());
            assert_eq!(a.writes.get(), writes);
        }
    }
    #[test]
    fn mode_or_foreign_values_refuse_restore_and_hold_exact_receipt_until_exit() {
        let f = Fixture::new();
        let a = f.access();
        let mut e = f.engine();
        e.apply(&a, "arena_000_int").unwrap();
        a.selection.set(false);
        assert!(e.restore(&a).is_err());
        assert_eq!(a.writes.get(), 1);
        assert!(e.halted().is_some());
        e.game_ended(|| Ok(())).unwrap();
        let mut e = f.engine();
        a.selection.set(true);
        *a.map.borrow_mut() = STOCK;
        e.apply(&a, "arena_brickleyfacility_int").unwrap();
        a.fields.borrow_mut()[0] = b'z';
        let writes = a.writes.get();
        assert!(e.restore(&a).is_err());
        assert_eq!(a.writes.get(), writes);
        e.game_ended(|| Ok(())).unwrap();
    }
    #[test]
    fn restart_only_clears_valid_receipt_after_known_exit_never_replays_memory() {
        let f = Fixture::new();
        let a = f.access();
        let mut e = f.engine();
        e.apply(&a, "arena_000_int").unwrap();
        drop(e);
        assert!(Engine::new(&f.root, &"b".repeat(48), &identity()).is_err());
        assert!(
            resolve_with(&f.root, &"a".repeat(48), &identity(), || anyhow::bail!(
                "Unknown game presence"
            ))
            .is_err()
        );
        assert!(pending(&f.root).unwrap());
        assert!(resolve_with(&f.root, &"b".repeat(48), &identity(), || Ok(())).is_err());
        let mut reused = identity();
        reused.started = "133000000000000001".into();
        assert!(resolve_with(&f.root, &"a".repeat(48), &reused, || Ok(())).is_err());
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
    #[test]
    fn runtime_waits_for_stable_selection_and_rechecks_without_repeat_writes_between_matches() {
        use crate::arena_runtime::{Queue, Runner};
        let f = Fixture::new();
        let a = f.access();
        let q = Queue::new();
        let mut r = Runner::new(&f.root, &"a".repeat(48), &identity()).unwrap();
        q.request("arena_000_int", || Ok(())).unwrap();
        for boundary in [
            Some("match"),
            Some("main-menu"),
            None,
            Some("quick-play-menu"),
        ] {
            a.selection.set(false);
            r.tick(&q, &a, boundary, true).unwrap();
            assert_eq!(a.writes.get(), 0);
            assert_eq!(q.view().unwrap().phase, "waiting-for-selection");
        }
        // First stable selection tick prepares, as WPF's helper did.
        a.selection.set(true);
        r.tick(&q, &a, Some("selection"), true).unwrap();
        assert_eq!(a.writes.get(), 1);
        assert_eq!(q.view().unwrap().phase, "ready");
        for _ in 0..5 {
            r.tick(&q, &a, Some("selection"), true).unwrap();
        }
        a.selection.set(false);
        r.tick(&q, &a, Some("quick-play-menu"), true).unwrap();
        a.selection.set(true);
        for _ in 0..2 {
            r.tick(&q, &a, Some("selection"), true).unwrap();
        }
        assert_eq!(a.writes.get(), 1);
        q.request("default", || Ok(())).unwrap();
        a.selection.set(false);
        r.tick(&q, &a, Some("match"), true).unwrap();
        assert_eq!(a.writes.get(), 1);
        a.selection.set(true);
        for _ in 0..2 {
            r.tick(&q, &a, Some("selection"), true).unwrap();
        }
        assert_eq!(a.writes.get(), 2);
        assert!(!pending(&f.root).unwrap());
        assert_eq!(q.view().unwrap().active_id, "default");
    }
    #[test]
    fn runtime_latest_queued_choice_wins_and_ui_close_restores_at_selection_only() {
        use crate::arena_runtime::{Queue, Runner};
        let f = Fixture::new();
        let a = f.access();
        let q = Queue::new();
        let mut r = Runner::new(&f.root, &"a".repeat(48), &identity()).unwrap();
        q.request("arena_000_int", || Ok(())).unwrap();
        q.request("default", || Ok(())).unwrap();
        for _ in 0..2 {
            r.tick(&q, &a, Some("selection"), true).unwrap();
        }
        assert_eq!(a.writes.get(), 0);
        q.request("arena_brickleyfacility_int", || Ok(())).unwrap();
        r.tick(&q, &a, Some("selection"), true).unwrap();
        assert_eq!(a.writes.get(), 2);
        a.selection.set(false);
        r.tick(&q, &a, Some("match"), false).unwrap();
        assert_eq!(a.writes.get(), 2);
        assert_eq!(q.view().unwrap().requested_id, "default");
        let revision = q.view().unwrap().revision;
        for _ in 0..3 {
            r.tick(&q, &a, None, false).unwrap();
        }
        assert_eq!(q.view().unwrap().revision, revision);
        a.selection.set(true);
        for _ in 0..2 {
            r.tick(&q, &a, Some("selection"), false).unwrap();
        }
        assert_eq!(a.writes.get(), 4);
        assert!(!pending(&f.root).unwrap());
        assert!(q.view().unwrap().message.contains("launcher closed"));
    }
    #[test]
    fn runtime_failed_preflight_is_retryable_but_uncertain_writes_never_repeat() {
        use crate::arena_runtime::{Queue, Runner};
        let f = Fixture::new();
        let a = f.access();
        let q = Queue::new();
        let mut r = Runner::new(&f.root, &"a".repeat(48), &identity()).unwrap();
        a.assets.set(false);
        q.request("arena_brickleyfacility_int", || Ok(())).unwrap();
        for _ in 0..5 {
            r.tick(&q, &a, Some("selection"), true).unwrap();
        }
        assert_eq!(q.view().unwrap().phase, "error");
        assert_eq!(a.writes.get(), 0);
        assert!(!pending(&f.root).unwrap());
        a.assets.set(true);
        a.fail_at.set(1);
        q.request("arena_brickleyfacility_int", || Ok(())).unwrap();
        r.tick(&q, &a, Some("selection"), true).unwrap();
        assert_eq!(q.view().unwrap().phase, "held");
        assert!(pending(&f.root).unwrap());
        assert!(q.request("default", || Ok(())).is_err());
        for _ in 0..5 {
            r.tick(&q, &a, Some("selection"), false).unwrap();
        }
        assert_eq!(a.writes.get(), 1);
        r.engine.game_ended(|| Ok(())).unwrap();
        assert!(!pending(&f.root).unwrap());
        assert_eq!(a.writes.get(), 1);
    }
    #[test]
    fn pre_publication_mode_blip_releases_receipt_and_retries_without_holding() {
        use crate::arena_runtime::{Queue, Runner};
        let f = Fixture::new();
        let a = f.access();
        let mut e = f.engine();
        a.blips.set(1);
        assert!(e.apply(&a, "arena_000_int").is_err());
        assert!(e.halted().is_none() && e.retryable());
        assert!(!pending(&f.root).unwrap());
        assert_eq!(a.writes.get(), 0);
        e.apply(&a, "arena_000_int").unwrap();
        assert_eq!(a.writes.get(), 1);
        assert_eq!(e.selected(), "arena_000_int");
        // Runner: retries on the next stable selection samples, then ready.
        let f = Fixture::new();
        let a = f.access();
        let q = Queue::new();
        let mut r = Runner::new(&f.root, &"a".repeat(48), &identity()).unwrap();
        a.blips.set(2);
        q.request("arena_000_int", || Ok(())).unwrap();
        for _ in 0..12 {
            r.tick(&q, &a, Some("selection"), true).unwrap();
        }
        assert_eq!(q.view().unwrap().phase, "ready");
        assert_eq!(a.writes.get(), 1);
        // A template already written means a public change: still held.
        let f = Fixture::new();
        let a = f.access();
        let mut e = f.engine();
        a.blips.set(1);
        assert!(e.apply(&a, "arena_brickleyfacility_int").is_err());
        assert!(e.halted().is_some() && !e.retryable());
        assert!(pending(&f.root).unwrap());
    }
    #[test]
    fn first_selection_tick_prepares_and_reports_selection_to_ready_latency() {
        use crate::arena_runtime::{Queue, Runner};
        let f = Fixture::new();
        let a = f.access();
        let q = Queue::new();
        let mut r = Runner::new(&f.root, &"a".repeat(48), &identity()).unwrap();
        q.request("arena_brickleyfacility_int", || Ok(())).unwrap();
        r.tick(&q, &a, Some("match"), true).unwrap();
        assert_eq!(q.view().unwrap().phase, "waiting-for-selection");
        r.tick(&q, &a, Some("selection"), true).unwrap();
        let v = q.view().unwrap();
        assert_eq!(v.phase, "ready");
        assert!(
            v.message.contains("s after reaching selection"),
            "{}",
            v.message
        );
        // Later visits re-verify silently, without a fresh latency claim.
        r.tick(&q, &a, Some("match"), true).unwrap();
        r.tick(&q, &a, Some("selection"), true).unwrap();
        assert!(!q.view().unwrap().message.contains("after reaching"));
    }
}
