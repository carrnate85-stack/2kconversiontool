//! A pinned append-only intent log. It is never replayed into memory after a
//! restart. Uncertain memory/allocation ownership lasts until exact game exit.
use crate::{feature_plan::Plan, loader_files, session_safety::Identity};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};
pub(crate) const MARKER: &str = "native-v2-features-session.jsonl";
const SCHEMA: &str = "nba2k27-native-v2-feature-intent-v1";
const LIMIT: usize = 2 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Header {
    schema: String,
    session: String,
    game: Identity,
    parent: Identity,
    plan: Plan,
}
impl Header {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == SCHEMA
                && self.session.len() == 48
                && self.session.bytes().all(|b| b.is_ascii_hexdigit()),
            "Feature intent ownership is invalid"
        );
        for id in [&self.game, &self.parent] {
            ensure!(
                id.pid > 0
                    && id.started.parse::<u64>().is_ok_and(|v| v > 0)
                    && Path::new(&id.path).is_absolute(),
                "Feature intent process identity is invalid"
            );
        }
        ensure!(
            Path::new(&self.game.path)
                .file_name()
                .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case("NBA2K27.exe")),
            "Feature intent game name is invalid"
        );
        self.plan.validate()
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Change {
    pub at: u64,
    pub before: Vec<u8>,
    pub after: Vec<u8>,
}
pub(crate) struct Intent {
    feature: String,
    changes: Vec<Change>,
}
impl Intent {
    pub fn permits(&self, feature: &str, at: u64, before: &[u8], after: &[u8]) -> bool {
        self.feature == feature
            && self
                .changes
                .iter()
                .any(|c| c.at == at && c.before == before && c.after == after)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) enum Event {
    AllocationIntent {
        bytes: usize,
    },
    Allocated {
        at: u64,
        bytes: usize,
    },
    WriteIntent {
        feature: String,
        changes: Vec<Change>,
    },
    ProtectionIntent {
        at: u64,
        bytes: usize,
        original: u32,
    },
    Applied {
        feature: String,
    },
    Held {
        feature: String,
        message: String,
    },
}
fn encode(value: &impl Serialize) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}
fn open(path: &Path, create: bool) -> Result<File> {
    let file = fs::OpenOptions::new()
        .read(true)
        .write(create)
        .create_new(create)
        .access_mode(0x80000000 | 0x10000 | if create { 0x40000000 } else { 0 })
        .share_mode(1)
        .custom_flags(0x00200000)
        .open(path)?;
    let meta = file.metadata()?;
    ensure!(
        meta.is_file() && meta.file_attributes() & 0x400 == 0 && meta.len() <= LIMIT as u64,
        "Feature intent is not a bounded ordinary file"
    );
    Ok(file)
}
pub(crate) struct Journal {
    file: File,
    header: Header,
    size: usize,
    events: usize,
    _pins: Vec<loader_files::Directories>,
}
impl Journal {
    pub fn create(
        root: &Path,
        session: &str,
        game: &Identity,
        parent: &Identity,
        plan: &Plan,
    ) -> Result<Self> {
        let pins = vec![
            loader_files::Directories::pin(root)?,
            loader_files::Directories::pin(&root.join("Data"))?,
        ];
        let header = Header {
            schema: SCHEMA.into(),
            session: session.into(),
            game: game.clone(),
            parent: parent.clone(),
            plan: plan.clone(),
        };
        header.validate()?;
        let data = encode(&header)?;
        let mut file = open(&root.join("Data").join(MARKER), true)?;
        file.write_all(&data)?;
        file.sync_all()?;
        Ok(Self {
            file,
            header,
            size: data.len(),
            events: 0,
            _pins: pins,
        })
    }
    /// Events still available before the budget refuses every further write.
    /// Callers reserve a whole transaction (and its restore) up front so a
    /// half-journaled transaction can never be stranded without its undo.
    pub fn remaining(&self) -> usize {
        1024usize.saturating_sub(self.events)
    }
    pub fn append(&mut self, event: Event) -> Result<()> {
        let bytes = encode(&event)?;
        ensure!(
            self.events < 1024,
            "Feature intent event budget exhausted; no further writes permitted"
        );
        ensure!(
            self.size
                .checked_add(bytes.len())
                .is_some_and(|n| n <= LIMIT),
            "Feature intent exceeds bounds"
        );
        ensure!(
            self.file.metadata()?.len() == self.size as u64,
            "Feature intent changed outside its owner"
        );
        self.file.write_all(&bytes)?;
        self.file.sync_all()?;
        self.size += bytes.len();
        self.events += 1;
        Ok(())
    }
    pub fn write_intent(&mut self, feature: &str, changes: Vec<Change>) -> Result<Intent> {
        ensure!(
            matches!(
                feature,
                "cameras" | "scaler" | "wnba-freestyle" | "wnba-freestyle-loader"
            ) && !changes.is_empty()
                && changes.len() <= 128
                && changes.iter().all(
                    |c| c.before.len() == c.after.len() && (1..=4096).contains(&c.before.len())
                ),
            "Invalid bounded feature intent"
        );
        self.append(Event::WriteIntent {
            feature: feature.into(),
            changes: changes.clone(),
        })?;
        Ok(Intent {
            feature: feature.into(),
            changes,
        })
    }
    pub fn finish(&mut self, closed: impl Fn() -> Result<()>) -> Result<()> {
        closed()?;
        let mut bytes = Vec::new();
        self.file.seek(SeekFrom::Start(0))?;
        Read::by_ref(&mut self.file)
            .take((LIMIT + 1) as u64)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() == self.size && bytes.starts_with(&encode(&self.header)?),
            "Owned feature intent changed"
        );
        validate_lines(&bytes)?;
        closed()?;
        loader_files::unlink(&self.file)
    }
}
fn validate_lines(bytes: &[u8]) -> Result<Header> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= LIMIT && bytes.last() == Some(&b'\n'),
        "Incomplete feature intent retained for review"
    );
    let mut rows = bytes[..bytes.len() - 1].split(|v| *v == b'\n');
    let header: Header =
        serde_json::from_slice(rows.next().context("Feature intent header missing")?)?;
    header.validate()?;
    for (i, row) in rows.enumerate() {
        ensure!(i < 1024, "Feature intent event limit exceeded");
        let _: Event = serde_json::from_slice(row)?;
    }
    Ok(header)
}
/// Called only after the existing session coordinator proves the game and its
/// native provider are closed. Addresses in this receipt are NEVER followed.
pub(crate) fn resolve_ended(
    root: &Path,
    session: &str,
    game: &Identity,
    closed: impl Fn() -> Result<()>,
) -> Result<()> {
    closed()?;
    let path = root.join("Data").join(MARKER);
    if !path.try_exists()? {
        return Ok(());
    }
    let _pins = [
        loader_files::Directories::pin(root)?,
        loader_files::Directories::pin(&root.join("Data"))?,
    ];
    let mut file = open(&path, false)?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)?;
    let header = validate_lines(&bytes)?;
    ensure!(
        header.session == session && header.game == *game,
        "Feature recovery belongs to a different game session; left untouched"
    );
    closed()?;
    loader_files::unlink(&file)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    fn fixture() -> (PathBuf, Header) {
        let root =
            std::env::temp_dir().join(format!("v2-feature-intent-{}", crate::backend::unique_id()));
        fs::create_dir_all(root.join("Data")).unwrap();
        let header = Header {
            schema: SCHEMA.into(),
            session: crate::backend::unique_id(),
            game: Identity {
                pid: 111,
                started: "222".into(),
                path: r"C:\Fixture\NBA2K27.exe".into(),
            },
            parent: Identity {
                pid: 222,
                started: "333".into(),
                path: r"C:\Fixture\launcher.exe".into(),
            },
            plan: Plan::default(),
        };
        (root, header)
    }
    #[test]
    fn pinned_intent_precedes_effect_and_can_only_end_after_closed_check() {
        let (root, h) = fixture();
        let mut journal = Journal::create(&root, &h.session, &h.game, &h.parent, &h.plan).unwrap();
        assert!(root.join("Data").join(MARKER).is_file());
        journal
            .append(Event::AllocationIntent { bytes: 4096 })
            .unwrap();
        assert!(fs::write(root.join("Data").join(MARKER), b"replacement").is_err());
        assert!(journal.finish(|| anyhow::bail!("game alive")).is_err());
        journal.finish(|| Ok(())).unwrap();
        drop(journal);
        assert!(!root.join("Data").join(MARKER).exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn stale_receipt_never_replays_addresses_or_clears_malformed_data() {
        let (root, h) = fixture();
        let path = root.join("Data").join(MARKER);
        let mut data = encode(&h).unwrap();
        data.extend(
            encode(&Event::Allocated {
                at: 0x12345678,
                bytes: 4096,
            })
            .unwrap(),
        );
        fs::write(&path, &data).unwrap();
        assert!(
            resolve_ended(&root, &h.session, &h.game, || anyhow::bail!(
                "provider alive"
            ))
            .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), data);
        assert!(resolve_ended(&root, &crate::backend::unique_id(), &h.game, || Ok(())).is_err());
        resolve_ended(&root, &h.session, &h.game, || Ok(())).unwrap();
        fs::write(&path, b"{broken\n").unwrap();
        assert!(resolve_ended(&root, &h.session, &h.game, || Ok(())).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"{broken\n");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn intent_tokens_are_exact_and_event_budget_stays_recoverable() {
        let (root, h) = fixture();
        let mut journal = Journal::create(&root, &h.session, &h.game, &h.parent, &h.plan).unwrap();
        let intent = journal
            .write_intent(
                "scaler",
                vec![Change {
                    at: 0x10000,
                    before: vec![0; 4],
                    after: vec![1; 4],
                }],
            )
            .unwrap();
        assert!(intent.permits("scaler", 0x10000, &[0; 4], &[1; 4]));
        assert!(!intent.permits("cameras", 0x10000, &[0; 4], &[1; 4]));
        assert!(!intent.permits("scaler", 0x10001, &[0; 4], &[1; 4]));
        for _ in 1..1024 {
            journal
                .append(Event::Applied {
                    feature: "scaler".into(),
                })
                .unwrap();
        }
        assert!(
            journal
                .append(Event::Applied {
                    feature: "scaler".into()
                })
                .is_err()
        );
        journal.finish(|| Ok(())).unwrap();
        drop(journal);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn stock_wnba_loader_pointer_has_a_distinct_exact_intent_token() {
        let (root, h) = fixture();
        let mut journal = Journal::create(&root, &h.session, &h.game, &h.parent, &h.plan).unwrap();
        let before = 0x180000000u64.to_le_bytes();
        let after = 0x180000100u64.to_le_bytes();
        let intent = journal
            .write_intent(
                "wnba-freestyle-loader",
                vec![Change {
                    at: 0x20000,
                    before: before.to_vec(),
                    after: after.to_vec(),
                }],
            )
            .unwrap();
        assert!(intent.permits("wnba-freestyle-loader", 0x20000, &before, &after));
        assert!(!intent.permits("wnba-freestyle", 0x20000, &before, &after));
        journal.finish(|| Ok(())).unwrap();
        drop(journal);
        fs::remove_dir_all(root).unwrap();
    }
}
