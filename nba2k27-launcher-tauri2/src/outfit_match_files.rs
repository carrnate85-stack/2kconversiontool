//! Owned transport for the unchanged native saved-outfit worker. Consumed
//! choices and creation intents are durable before files can be acted upon.
//! Native code is allowed to delete only its trigger. Reports remain diagnostics.
use super::*;
use crate::blacktop_outfit_match as policy;
const MAX_MATCHES: u32 = 10000;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    number: u32,
    choices: Vec<Choice>,
    directory: FileId,
    sha256: String,
    plan: Option<FileId>,
    trigger: Option<FileId>,
    submitted: bool,
    complete: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "action",
    content = "value",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub(super) enum Event {
    Reserve {
        number: u32,
        choices: Vec<Choice>,
        directory: FileId,
        sha256: String,
    },
    Owned {
        number: u32,
        trigger: bool,
        file: FileId,
    },
    Submitted {
        number: u32,
    },
    Complete {
        number: u32,
        rebuilt: u32,
    },
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct State {
    records: Vec<Record>,
}
impl State {
    pub fn apply(&mut self, e: &Event, ledger: &mut Ledger, game: &Identity) -> Result<()> {
        let valid_file = |f: &FileId| f.index > 0 && f.birth > 0;
        match e {
            Event::Reserve {
                number,
                choices,
                directory,
                sha256,
            } => {
                ensure!(
                    (1..=MAX_MATCHES).contains(number) && valid_file(directory),
                    "Invalid outfit match number/directory"
                );
                ensure!(
                    self.records.last().is_none_or(|r| r.complete
                        && r.number + 1 == *number
                        && r.directory == *directory)
                        && (!self.records.is_empty() || *number == 1),
                    "Previous outfit match is not complete/rearmed"
                );
                let bytes = policy::encode(game, choices)?;
                ensure!(
                    loader_files::digest(&bytes) == *sha256,
                    "Frozen outfit plan hash differs"
                );
                ledger.consume(choices, |_| Ok(()))?;
                self.records.push(Record {
                    number: *number,
                    choices: choices.clone(),
                    directory: directory.clone(),
                    sha256: sha256.clone(),
                    plan: None,
                    trigger: None,
                    submitted: false,
                    complete: false,
                });
            }
            Event::Owned {
                number,
                trigger,
                file,
            } => {
                ensure!(valid_file(file), "Invalid owned outfit file identity");
                let r = self
                    .records
                    .last_mut()
                    .context("Outfit file has no creation intent")?;
                ensure!(
                    r.number == *number && !r.submitted && !r.complete,
                    "Outfit file event is stale"
                );
                if *trigger {
                    ensure!(
                        r.plan.is_some() && r.trigger.is_none(),
                        "Trigger requires an owned plan"
                    );
                    r.trigger = Some(file.clone());
                } else {
                    ensure!(
                        r.plan.is_none() && r.trigger.is_none(),
                        "Plan file cannot be adopted twice"
                    );
                    r.plan = Some(file.clone());
                }
            }
            Event::Submitted { number } => {
                let r = self
                    .records
                    .last_mut()
                    .context("Outfit submission has no intent")?;
                ensure!(
                    r.number == *number
                        && r.plan.is_some()
                        && r.trigger.is_some()
                        && !r.submitted
                        && !r.complete,
                    "Outfit trigger cannot be submitted twice"
                );
                r.submitted = true;
            }
            Event::Complete { number, rebuilt } => {
                let r = self
                    .records
                    .last_mut()
                    .context("Outfit result has no submission")?;
                ensure!(
                    r.number == *number
                        && r.submitted
                        && !r.complete
                        && *rebuilt == r.choices.len() as u32,
                    "Outfit result is incomplete/stale"
                );
                r.complete = true;
            }
        }
        Ok(())
    }
}
fn root(path: &Path) -> Result<&Path> {
    path.parent()
        .and_then(Path::parent)
        .context("Outfit portable root missing")
}
fn directory(path: &Path) -> PathBuf {
    path.join("Logs/game-session")
}
fn prefix(game: &Identity, number: u32) -> Result<String> {
    ensure!(
        game.pid > 0 && (1..=MAX_MATCHES).contains(&number),
        "Invalid outfit file prefix"
    );
    Ok(format!(
        "nba2k27_saved_outfits_{}{}",
        game.pid,
        if number > 1 {
            format!("_match{number}")
        } else {
            String::new()
        }
    ))
}
fn path(dir: &Path, game: &Identity, number: u32, extension: &str) -> Result<PathBuf> {
    ensure!(
        ["plan", "trigger", "log"].contains(&extension),
        "Unknown outfit file extension"
    );
    Ok(dir.join(format!("{}.{}", prefix(game, number)?, extension)))
}
fn exists(path: &Path) -> Result<bool> {
    Ok(fs::symlink_metadata(path).map(|_| true).or_else(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Ok(false)
        } else {
            Err(e)
        }
    })?)
}
fn directory_id(path: &Path) -> Result<FileId> {
    let f = fs::OpenOptions::new()
        .read(true)
        .share_mode(3)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)?;
    ensure!(
        f.metadata()?.is_dir() && f.metadata()?.file_attributes() & 0x400 == 0,
        "Outfit report folder is not an ordinary directory"
    );
    file_id(&f)
}
fn ordinary(file: &File, limit: usize) -> Result<usize> {
    let m = file.metadata()?;
    ensure!(
        m.is_file() && m.file_attributes() & 0x400 == 0 && m.len() <= limit as u64,
        "Unsupported outfit transport file"
    );
    Ok(m.len() as usize)
}
fn verify(file: &File, id: &FileId, expected: &[u8]) -> Result<()> {
    ensure!(
        file_id(file)? == *id
            && ordinary(file, expected.len())? == expected.len()
            && read_at(file, 0, expected.len())? == expected,
        "Owned outfit file identity/content changed"
    );
    Ok(())
}
fn create(path: &Path, bytes: &[u8]) -> Result<File> {
    let mut f = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .access_mode(0xc0010000)
        .share_mode(1)
        .create_new(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)
        .context(
            "Outfit plan/trigger already exists or cannot be created; never overwrite/adopt it",
        )?;
    let result = (|| {
        f.write_all(bytes)?;
        f.sync_all()?;
        verify(&f, &file_id(&f)?, bytes)
    })();
    if let Err(e) = result {
        let _ = loader_files::unlink(&f);
        return Err(e);
    }
    Ok(f)
}
// Non-cloneable exclusive transport lock, constructed before any marker bytes.
// No native authority is stored here. Unknown cleanup must not unlock a trigger.
pub(super) struct LockedTrigger {
    file: Option<File>,
    id: Option<FileId>,
    number: u32,
    pins: Option<Vec<Directories>>,
}
impl LockedTrigger {
    fn file(&self) -> Result<&File> {
        self.file.as_ref().context("Trigger lock already released")
    }
    fn verify(&self) -> Result<()> {
        verify(
            self.file()?,
            self.id
                .as_ref()
                .context("Unknown created trigger identity")?,
            policy::TRIGGER,
        )
    }
    fn unlink_exact(&self) -> Result<()> {
        let file = self.file()?;
        ensure!(
            file_id(file)?
                == *self
                    .id
                    .as_ref()
                    .context("Unknown trigger identity; retain exclusive lock")?
                && file.metadata()?.is_file()
                && file.metadata()?.file_attributes() & 0x400 == 0,
            "Trigger identity changed; retain exclusive lock"
        );
        #[cfg(test)]
        submission_fault(8)?;
        loader_files::unlink(file)
    }
    fn close(mut self) -> Vec<Directories> {
        drop(self.file.take());
        self.pins.take().unwrap_or_default()
    }
    pub(super) fn release_after_game_closed(self) {
        drop(self.close());
    }
}
impl Drop for LockedTrigger {
    fn drop(&mut self) {
        // Bounded quarantine: one pending journal/trigger per owned session;
        // sticky-held Store and create_new MARKER prohibit live reopening.
        // The OS releases these handles at provider process exit. Native fresh
        // worker/parent checks then reject effects (not atomic cancellation of
        // a call which already entered before process exit).
        if let Some(file) = self.file.take() {
            std::mem::forget(file);
            if let Some(pins) = self.pins.take() {
                std::mem::forget(pins);
            }
        }
    }
}
#[cfg(test)]
thread_local! {
    static SUBMISSION_FAULT: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
    static SUBMISSION_TRACE: std::cell::RefCell<Vec<&'static str>> = const { std::cell::RefCell::new(Vec::new()) };
}
#[cfg(test)]
fn submission_fault(bit: u8) -> Result<()> {
    SUBMISSION_FAULT.with(|fault| {
        let mask = fault.get();
        if mask & bit != 0 {
            fault.set(mask & !bit);
            anyhow::bail!("Injected submission stage {bit}");
        }
        Ok(())
    })
}
impl Store {
    fn abort_locked_trigger(&mut self, trigger: LockedTrigger) -> Result<()> {
        match trigger.unlink_exact() {
            Ok(()) => {
                drop(trigger.close());
                Ok(())
            }
            Err(error) => {
                ensure!(
                    self.held_trigger.is_none(),
                    "Only one pending trigger lock is permitted"
                );
                self.held_trigger = Some(trigger);
                Err(error).context(
                    "Exact trigger cleanup failed; exclusive lock quarantined until game closes",
                )
            }
        }
    }
    fn create_locked_trigger(
        &mut self,
        target: &Path,
        number: u32,
        pins: Vec<Directories>,
    ) -> Result<LockedTrigger> {
        ensure!(
            self.halted.is_none() && self.held_trigger.is_none(),
            "Held outfit transport cannot create another trigger"
        );
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .access_mode(0xc0010000)
            .share_mode(1)
            .create_new(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(target)?;
        let mut locked = LockedTrigger {
            file: Some(file),
            id: None,
            number,
            pins: Some(pins),
        };
        let created = (|| -> Result<()> {
            #[cfg(test)]
            submission_fault(64)?;
            locked.id = Some(file_id(locked.file()?)?);
            let mut file = locked.file()?;
            file.write_all(policy::TRIGGER)?;
            #[cfg(test)]
            submission_fault(16)?;
            file.sync_all()?;
            locked.verify()
        })();
        if let Err(error) = created {
            let cleanup = self.abort_locked_trigger(locked);
            return match cleanup {
                Ok(()) => Err(error),
                Err(cleanup) => Err(error).context(format!("{cleanup:#}")),
            };
        }
        Ok(locked)
    }
    // Deliberately not a generic batching API. Only this exact existing event
    // pair is legal, while the caller still owns an unreadable trigger handle.
    fn commit_locked_submission(&mut self, trigger: &LockedTrigger) -> Result<Witness> {
        ensure!(
            self.halted.is_none() && self.held_trigger.is_none(),
            "Outfit submission is held"
        );
        trigger.verify()?;
        let shared = self
            .shared
            .as_ref()
            .context("Outfit journal ended")?
            .clone();
        let owned = super::Event::Match(Event::Owned {
            number: trigger.number,
            trigger: true,
            file: trigger
                .id
                .as_ref()
                .context("Missing trigger identity")?
                .clone(),
        });
        let submitted = super::Event::Match(Event::Submitted {
            number: trigger.number,
        });
        // Validate BOTH transitions and every physical bound before writing.
        let mut next = self.state.clone();
        next.apply(&owned, &shared.header.game)?;
        next.apply(&submitted, &shared.header.game)?;
        let first = super::frame(&owned)?;
        let second = super::frame(&submitted)?;
        let bytes = [first.as_slice(), second.as_slice()].concat();
        let end = self
            .end
            .checked_add(bytes.len() as u64)
            .context("Outfit journal byte overflow")?;
        ensure!(
            end <= MAX_BYTES as u64,
            "Outfit journal byte budget reached"
        );
        let result = (|| -> Result<()> {
            let mut file = shared
                .file
                .lock()
                .map_err(|_| anyhow::anyhow!("Outfit journal lock failed"))?;
            shared.verify_file(&file)?;
            #[cfg(test)]
            submission_fault(1)?;
            ensure!(
                file.metadata()?.len() == self.end,
                "Outfit journal length changed"
            );
            file.seek(SeekFrom::End(0))?;
            #[cfg(test)]
            {
                // Preserve existing logical-event fault-injection semantics.
                if self.fail_append_skip >= 2 {
                    self.fail_append_skip -= 2;
                } else if let Some(cut) = self.fail_append_after.take() {
                    let offset = if self.fail_append_skip == 1 {
                        first.len()
                    } else {
                        0
                    };
                    self.fail_append_skip = 0;
                    file.write_all(&bytes[..offset.saturating_add(cut).min(bytes.len())])?;
                    file.sync_all()?;
                    anyhow::bail!("Injected interrupted submission pair");
                }
                SUBMISSION_TRACE.with(|trace| trace.borrow_mut().push("pair-write"));
            }
            file.write_all(&bytes)?;
            #[cfg(test)]
            submission_fault(2)?;
            file.sync_all()?;
            #[cfg(test)]
            SUBMISSION_TRACE.with(|trace| trace.borrow_mut().push("pair-sync"));
            #[cfg(test)]
            submission_fault(4)?;
            ensure!(
                read_at(&file, self.end, bytes.len())? == bytes,
                "Outfit submission pair readback differs"
            );
            shared.verify_file(&file)?;
            #[cfg(test)]
            SUBMISSION_TRACE.with(|trace| trace.borrow_mut().push("pair-readback"));
            Ok(())
        })();
        if let Err(error) = result {
            self.halted = Some(format!("Uncertain outfit submission pair: {error:#}"));
            return Err(error);
        }
        let witness = Witness {
            shared,
            offset: self.end,
            bytes,
        };
        self.end = end;
        self.state = next;
        Ok(witness)
    }
}

/// Only Store::reserve_match can mint this. The consumed-ID frame is already
/// flushed, and its exact frozen choices/plan hash cannot be changed by callers.
pub(crate) struct Frozen {
    proof: Witness,
    record: Record,
}
pub(crate) struct Pending {
    proof: Witness,
    record: Record,
    plan: File,
    _pins: Vec<Directories>,
    complete: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Armed,
    Running,
    Rebuilt,
    Failed,
    Inconclusive,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Report {
    pub phase: Phase,
    pub rebuilt: u32,
    pub reason: String,
}
fn parse_report(bytes: &[u8], game: &Identity, expected: Option<usize>) -> Result<Report> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= 2048 && bytes.is_ascii(),
        "Unsupported outfit worker report"
    );
    let text = std::str::from_utf8(bytes)?;
    let mut fields = BTreeMap::new();
    for raw in text.lines() {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        ensure!(
            !line.chars().any(char::is_control),
            "Control byte in outfit report"
        );
        let (key, value) = line
            .split_once('=')
            .context("Incomplete outfit worker report")?;
        ensure!(
            [
                "SCHEMA",
                "PID",
                "STATE",
                "REASON",
                "PLAYERS_REBUILT",
                "GAME_CODE_HOOKS",
                "OBJECT_HOOKS"
            ]
            .contains(&key)
                && fields.insert(key, value).is_none(),
            "Unknown/duplicate outfit report field"
        );
    }
    ensure!(
        fields.len() == 7
            && fields.get("SCHEMA") == Some(&"blacktop-saved-outfits-v1")
            && fields.get("PID") == Some(&game.pid.to_string().as_str())
            && fields.get("GAME_CODE_HOOKS") == Some(&"0")
            && fields.get("OBJECT_HOOKS") == Some(&"0"),
        "Outfit worker report owner/schema differs"
    );
    let phase = match fields["STATE"] {
        "ARMED" => Phase::Armed,
        "RUNNING" => Phase::Running,
        "REBUILT" => Phase::Rebuilt,
        "FAILED" => Phase::Failed,
        "INCONCLUSIVE" => Phase::Inconclusive,
        _ => anyhow::bail!("Unsupported outfit worker state"),
    };
    let rebuilt = fields["PLAYERS_REBUILT"].parse::<u32>()?;
    ensure!(
        rebuilt <= 10
            && fields["PLAYERS_REBUILT"] == rebuilt.to_string()
            && !fields["REASON"].is_empty()
            && fields["REASON"].len() <= 256,
        "Invalid outfit worker count/reason"
    );
    if phase == Phase::Armed {
        ensure!(rebuilt == 0, "Armed worker already reports rebuilt players");
    }
    if let Some(n) = expected {
        ensure!(
            rebuilt as usize <= n,
            "Outfit worker rebuilt an unexpected player count"
        );
        if phase == Phase::Rebuilt {
            ensure!(
                rebuilt as usize == n,
                "Outfit worker did not rebuild every requested player"
            );
        }
    }
    Ok(Report {
        phase,
        rebuilt,
        reason: fields["REASON"].into(),
    })
}
fn report(
    dir: &Path,
    game: &Identity,
    number: u32,
    expected: Option<usize>,
) -> Result<Option<Report>> {
    let path = path(dir, game, number, "log")?;
    if !exists(&path)? {
        return Ok(None);
    }
    let f = fs::OpenOptions::new()
        .read(true)
        .share_mode(7)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)?;
    let len = ordinary(&f, 2048)?;
    if len == 0 {
        return Ok(None);
    }
    ensure!(
        f.metadata()?.last_write_time() >= game.started.parse::<u64>()?,
        "Stale outfit worker report predates this game"
    );
    let before = file_id(&f)?;
    let first = read_at(&f, 0, len)?;
    if ordinary(&f, 2048)? != len || read_at(&f, 0, len)? != first || file_id(&f)? != before {
        return Ok(None);
    }
    match parse_report(&first, game, expected) {
        Ok(report) => Ok(Some(report)),
        Err(_) if torn_rewrite(&first, game, expected) => Ok(None),
        Err(error) => Err(error),
    }
}
/// The worker rewrites in place (seek 0, write, then truncate), so a shorter
/// state briefly reads as a complete valid report followed by the tail of the
/// previous one. That exact shape is "not yet stable", never a hold; any other
/// malformed report still fails.
fn torn_rewrite(bytes: &[u8], game: &Identity, expected: Option<usize>) -> bool {
    const LAST: &[u8] = b"OBJECT_HOOKS=0";
    const ENDING: &[u8] = b"\nGAME_CODE_HOOKS=0\nOBJECT_HOOKS=0\n";
    let Some(field) = bytes.windows(LAST.len()).position(|w| w == LAST) else {
        return false;
    };
    let Some(newline) = bytes[field..].iter().position(|b| *b == b'\n') else {
        return false;
    };
    let (report, tail) = bytes.split_at(field + newline + 1);
    !tail.is_empty()
        && tail.len() < report.len()
        && (ENDING.ends_with(tail) || tail.ends_with(ENDING))
        && parse_report(report, game, expected).is_ok()
}
impl Store {
    pub fn next_match(&self) -> Result<u32> {
        let n = match self.state.matches.records.last() {
            None => 1,
            Some(r) if r.complete => r.number + 1,
            Some(_) => anyhow::bail!("An outfit match submission is already pending"),
        };
        ensure!(
            n <= MAX_MATCHES,
            "Outfit match budget reached; restart the game"
        );
        Ok(n)
    }
    pub fn worker_report(&self, number: u32) -> Result<Option<Report>> {
        let shared = self.shared.as_ref().context("Outfit journal ended")?;
        let dir = directory(root(&shared.path)?);
        let _pin = Directories::pin(&dir)?;
        report(&dir, &shared.header.game, number, None)
    }
    pub fn reserve_match(
        &mut self,
        choices: &[Choice],
        check: impl Fn() -> Result<()>,
    ) -> Result<Frozen> {
        check()?;
        let number = self.next_match()?;
        let shared = self.shared.as_ref().context("Outfit journal ended")?;
        let dir = directory(root(&shared.path)?);
        let _pin = Directories::pin(&dir)?;
        let r = report(&dir, &shared.header.game, number, None)?
            .context("Outfit worker has not armed this match")?;
        ensure!(
            r.phase == Phase::Armed,
            "Outfit worker is not armed for a new submission"
        );
        for ext in ["plan", "trigger"] {
            ensure!(
                !exists(&path(&dir, &shared.header.game, number, ext)?)?,
                "Previous outfit transport file exists; never adopt it"
            );
        }
        let record = Record {
            number,
            choices: choices.to_vec(),
            directory: directory_id(&dir)?,
            sha256: loader_files::digest(&policy::encode(&shared.header.game, choices)?),
            plan: None,
            trigger: None,
            submitted: false,
            complete: false,
        };
        check()?;
        let proof = self.append(super::Event::Match(Event::Reserve {
            number,
            choices: choices.to_vec(),
            directory: record.directory.clone(),
            sha256: record.sha256.clone(),
        }))?;
        Ok(Frozen { proof, record })
    }
    pub fn publish_match(&mut self, f: Frozen, check: impl Fn() -> Result<()>) -> Result<Pending> {
        let result = self.publish_inner(f, &check);
        if let Err(e) = &result {
            self.halted = Some(format!("Outfit submission held; no retry: {e:#}"));
        }
        result
    }
    fn publish_inner(&mut self, f: Frozen, check: &impl Fn() -> Result<()>) -> Result<Pending> {
        ensure!(
            self.halted.is_none() && self.held_trigger.is_none(),
            "Outfit submission is held; no replay"
        );
        let shared = self
            .shared
            .as_ref()
            .context("Outfit journal ended")?
            .clone();
        ensure!(
            Arc::ptr_eq(&shared, &f.proof.shared),
            "Frozen outfit plan belongs to another journal"
        );
        let dir = directory(root(&shared.path)?);
        let pins = vec![Directories::pin(&dir)?];
        ensure!(
            directory_id(&dir)? == f.record.directory,
            "Outfit report directory changed"
        );
        let game = &shared.header.game;
        let number = f.record.number;
        let bytes = policy::encode(game, &f.record.choices)?;
        ensure!(
            loader_files::digest(&bytes) == f.record.sha256,
            "Frozen plan changed"
        );
        let plan_path = path(&dir, game, number, "plan")?;
        let plan = {
            let _pin = f.proof.pin()?;
            check()?;
            create(&plan_path, &bytes)?
        };
        let plan_id = file_id(&plan)?;
        let plan_proof = match self.append(super::Event::Match(Event::Owned {
            number,
            trigger: false,
            file: plan_id.clone(),
        })) {
            Ok(w) => w,
            Err(e) => {
                let _ = loader_files::unlink(&plan);
                return Err(e);
            }
        };
        // A WRITE/DELETE handle would conflict with the native reader's
        // FILE_SHARE_READ. Retain only a verified read-only handle from here.
        drop(plan);
        let plan = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(&plan_path)?;
        verify(&plan, &plan_id, &bytes)?;
        let trigger = {
            let _pin = plan_proof.pin()?;
            check()?;
            let armed =
                report(&dir, game, number, None)?.context("Outfit worker report disappeared")?;
            ensure!(
                armed.phase == Phase::Armed,
                "Outfit worker changed before submission"
            );
            self.create_locked_trigger(&path(&dir, game, number, "trigger")?, number, pins)?
        };
        let submitted = (|| -> Result<Witness> {
            let proof = self.commit_locked_submission(&trigger)?;
            // Both ordinary existing frames are durable before this last live
            // check, and native open/delete is STILL blocked by LockedTrigger.
            {
                let _pin = proof.pin()?;
                #[cfg(test)]
                SUBMISSION_TRACE.with(|trace| trace.borrow_mut().push("pair-pin"));
                check()?;
                verify(&plan, &plan_id, &bytes)?;
                #[cfg(test)]
                submission_fault(32)?;
                trigger.verify()?;
                #[cfg(test)]
                SUBMISSION_TRACE.with(|trace| trace.borrow_mut().push("final-check"));
            }
            Ok(proof)
        })();
        let proof = match submitted {
            Ok(proof) => proof,
            Err(error) => {
                let cleanup = self.abort_locked_trigger(trigger);
                return match cleanup {
                    Ok(()) => Err(error),
                    Err(cleanup) => Err(error).context(format!("{cleanup:#}")),
                };
            }
        };
        let record = self
            .state
            .matches
            .records
            .last()
            .context("Missing submitted outfit record")?
            .clone();
        #[cfg(test)]
        SUBMISSION_TRACE.with(|trace| trace.borrow_mut().push("trigger-unlock"));
        let pins = trigger.close();
        drop(plan_proof);
        drop(f);
        Ok(Pending {
            proof,
            record,
            plan,
            _pins: pins,
            complete: false,
        })
    }
    pub fn poll_match(
        &mut self,
        p: &mut Pending,
        check: impl Fn() -> Result<()>,
    ) -> Result<Option<Report>> {
        let result = self.poll_inner(p, &check);
        if let Err(e) = &result {
            self.halted = Some(format!("Outfit rebuild unconfirmed; no retry: {e:#}"));
        }
        result
    }
    fn poll_inner(
        &mut self,
        p: &mut Pending,
        check: &impl Fn() -> Result<()>,
    ) -> Result<Option<Report>> {
        let shared = self
            .shared
            .as_ref()
            .context("Outfit journal ended")?
            .clone();
        ensure!(
            Arc::ptr_eq(&shared, &p.proof.shared),
            "Native outfit result belongs to another journal"
        );
        let dir = directory(root(&shared.path)?);
        let game = &shared.header.game;
        let outcome = {
            let _pin = p.proof.pin()?;
            check()?;
            ensure!(
                directory_id(&dir)? == p.record.directory,
                "Outfit report directory changed"
            );
            verify(
                &p.plan,
                p.record.plan.as_ref().context("Owned plan missing")?,
                &policy::encode(game, &p.record.choices)?,
            )?;
            report(&dir, game, p.record.number, Some(p.record.choices.len()))?
        };
        if let Some(r) = &outcome {
            ensure!(
                !p.complete || r.phase == Phase::Rebuilt,
                "Completed native outfit report changed; do not rearm from it"
            );
            ensure!(
                !matches!(r.phase, Phase::Failed | Phase::Inconclusive),
                "Native outfit worker stopped: {}",
                r.reason
            );
            if r.phase == Phase::Rebuilt && !p.complete {
                ensure!(
                    !exists(&path(&dir, game, p.record.number, "trigger")?)?,
                    "Native outfit worker did not consume its trigger"
                );
                check()?;
                self.append(super::Event::Match(Event::Complete {
                    number: p.record.number,
                    rebuilt: r.rebuilt,
                }))?;
                p.complete = true;
            }
        }
        Ok(outcome)
    }
}
pub(super) fn cleanup(
    journal: &Path,
    header: &Header,
    state: &State,
    closed: &impl Fn() -> Result<()>,
) -> Result<()> {
    if state.records.is_empty() {
        return Ok(());
    }
    closed()?;
    let dir = directory(root(journal)?);
    if !exists(&dir)? {
        return Ok(());
    }
    let _pin = Directories::pin(&dir)?;
    let identity = directory_id(&dir)?;
    for r in &state.records {
        ensure!(
            identity == r.directory,
            "Outfit recovery report directory identity changed; left untouched"
        );
        for (ext, owned, bytes) in [
            ("plan", &r.plan, policy::encode(&header.game, &r.choices)?),
            ("trigger", &r.trigger, policy::TRIGGER.to_vec()),
        ] {
            let target = path(&dir, &header.game, r.number, ext)?;
            if !exists(&target)? {
                continue;
            }
            let id=owned.as_ref().context("Outfit creation was interrupted before file ownership was committed; retain for review")?;
            let f = fs::OpenOptions::new()
                .read(true)
                .access_mode(0x80010000)
                .share_mode(1)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
                .open(&target)?;
            verify(&f, id, &bytes)?;
            closed()?;
            loader_files::unlink(&f)?;
            drop(f);
            ensure!(!exists(&target)?, "Owned outfit file deletion is pending");
        }
    }
    closed()?;
    Ok(())
}
#[cfg(test)]
#[path = "outfit_match_files_tests.rs"]
mod tests;
