//! Provider-owned gameplay-ball QA backend. One data DWORD only.
//! New requests must not run alongside the legacy Normal-filename service.
#![allow(dead_code)]
use crate::{
    ball_gameplay_v2 as policy, blacktop_ball as ball,
    blacktop_native::Memory,
    session_safety::{self, Identity},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::Instant,
};
pub(crate) const MARKER: &str = "blacktop-gameplay-ball-v2-session.jsonl";
const SCHEMA: &str = "nba2k27-gameplay-ball-v2-session-v1";
const LIMIT: u64 = 2 * 1024 * 1024;
#[derive(Clone, Debug)]
pub(crate) struct Binding {
    game: Identity,
    parent: Identity,
    game_root: PathBuf,
    portable: PathBuf,
    session: String,
}
impl Binding {
    pub(crate) fn from_launched(
        game: Identity,
        parent: Identity,
        game_root: PathBuf,
        portable: PathBuf,
        session: String,
    ) -> Result<Self> {
        ensure!(
            game.pid > 0
                && parent.pid > 0
                && game.pid != parent.pid
                && game.started.parse::<u64>().is_ok_and(|v| v > 0)
                && parent.started.parse::<u64>().is_ok_and(|v| v > 0)
                && Path::new(&game.path).is_absolute()
                && Path::new(&parent.path).is_absolute()
                && game_root.is_absolute()
                && portable.is_absolute()
                && crate::native_session::same_path(&game.path, &game_root.join("NBA2K27.exe"))
                && session.len() == 48
                && session.bytes().all(|c| c.is_ascii_hexdigit()),
            "Invalid exact-owned gameplay-ball launch binding"
        );
        Ok(Self {
            game,
            parent,
            game_root,
            portable,
            session,
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Header {
    schema: String,
    event: String,
    session: String,
    game: Identity,
    parent: Identity,
    choice: String,
    file_identity: crate::file_identity::Stable,
}
impl Header {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == SCHEMA
                && self.event == "header"
                && self.session.len() == 48
                && self.session.bytes().all(|c| c.is_ascii_hexdigit()),
            "Invalid gameplay-ball journal header"
        );
        ball::choice(&self.choice)?;
        for id in [&self.game, &self.parent] {
            ensure!(
                id.pid > 0
                    && id.started.parse::<u64>().is_ok_and(|v| v > 0)
                    && Path::new(&id.path).is_absolute(),
                "Invalid gameplay-ball journal identity"
            );
        }
        ensure!(
            Path::new(&self.game.path)
                .file_name()
                .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case("NBA2K27.exe")),
            "Invalid gameplay-ball journal game path"
        );
        Ok(())
    }
}
fn hash(v: &Value) -> Result<String> {
    Ok(format!("{:X}", Sha256::digest(serde_json::to_vec(v)?)))
}
struct Proof {
    offset: u64,
    bytes: Vec<u8>,
    hash: String,
}
struct Journal {
    file: File,
    size: u64,
    events: usize,
    identity: crate::file_identity::Stable,
    _pins: Vec<crate::loader_files::Directories>,
    proof: Option<Proof>,
}
impl Journal {
    fn create(b: &Binding, id: &str) -> Result<Self> {
        ball::choice(id)?;
        let pins = vec![
            crate::loader_files::Directories::pin(&b.portable)?,
            crate::loader_files::Directories::pin(&b.portable.join("Data"))?,
        ];
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .share_mode(1)
            .custom_flags(0x00200000)
            .open(b.portable.join("Data").join(MARKER))?;
        let meta = file.metadata()?;
        ensure!(
            meta.is_file() && meta.file_attributes() & 0x400 == 0,
            "Gameplay-ball journal is not an ordinary file"
        );
        let identity = crate::file_identity::Stable::read(&file)?;
        let mut j = Self {
            file,
            size: 0,
            events: 0,
            identity: identity.clone(),
            _pins: pins,
            proof: None,
        };
        let header = Header {
            schema: SCHEMA.into(),
            event: "header".into(),
            session: b.session.clone(),
            game: b.game.clone(),
            parent: b.parent.clone(),
            choice: id.into(),
            file_identity: identity,
        };
        header.validate()?;
        j.append(serde_json::to_value(header)?)?;
        Ok(j)
    }
    fn append(&mut self, value: Value) -> Result<(u64, Vec<u8>)> {
        let mut bytes = serde_json::to_vec(&value)?;
        bytes.push(b'\n');
        ensure!(
            bytes.len() <= 65536
                && self.events < 2048
                && self
                    .size
                    .checked_add(bytes.len() as u64)
                    .is_some_and(|v| v <= LIMIT),
            "Gameplay-ball durable journal bound reached"
        );
        ensure!(
            crate::file_identity::Stable::read(&self.file)? == self.identity
                && self.file.metadata()?.len() == self.size,
            "Gameplay-ball journal identity/length changed"
        );
        self.file.seek(SeekFrom::Start(self.size))?;
        let offset = self.size;
        self.file.write_all(&bytes)?;
        self.file.sync_all()?;
        self.size += bytes.len() as u64;
        self.events += 1;
        self.readback(offset, &bytes)?;
        Ok((offset, bytes))
    }
    fn readback(&self, offset: u64, bytes: &[u8]) -> Result<()> {
        ensure!(
            crate::file_identity::Stable::read(&self.file)? == self.identity
                && self.file.metadata()?.len() == self.size
                && offset
                    .checked_add(bytes.len() as u64)
                    .is_some_and(|e| e <= self.size),
            "Gameplay-ball durable proof identity/extent changed"
        );
        let mut f = &self.file;
        f.seek(SeekFrom::Start(offset))?;
        let mut actual = vec![0; bytes.len()];
        f.read_exact(&mut actual)?;
        ensure!(actual == bytes, "Gameplay-ball durable proof differs");
        Ok(())
    }
    fn intent(&mut self, p: &policy::Permit<'_>) -> Result<()> {
        let tx = p.receipt();
        let digest = hash(&tx)?;
        let (offset, bytes) =
            self.append(json!({"event":"intent","transaction":tx,"sha256":digest}))?;
        self.proof = Some(Proof {
            offset,
            bytes,
            hash: digest,
        });
        Ok(())
    }
    fn verify(&self, p: &policy::Permit<'_>) -> Result<()> {
        let proof = self
            .proof
            .as_ref()
            .context("Gameplay-ball publication has no durable intent")?;
        ensure!(
            proof.hash == hash(&p.receipt())?,
            "Gameplay-ball publication differs from durable intent"
        );
        self.readback(proof.offset, &proof.bytes)
    }
    fn note(&mut self, event: &str) -> Result<()> {
        ensure!(
            matches!(event, "prepared" | "restored"),
            "Unknown gameplay-ball journal event"
        );
        self.append(json!({"event":event}))?;
        self.proof = None;
        Ok(())
    }
}
#[path = "ball_gameplay_native.rs"]
mod native;
pub(crate) struct Runner {
    binding: Binding,
    desired: String,
    native: Option<native::Native>,
    journal: Option<Journal>,
    owned: Option<policy::Plan>,
    samples: policy::Samples,
    restore_samples: policy::RestoreSamples,
    /// Failed restore publications; each costs one durable intent.
    restore_failures: u8,
    clock: Instant,
    seen_match: bool,
    restore_requested: bool,
    held: bool,
    message: String,
    phase: &'static str,
    gameplay_model_confirmed: bool,
    /// Choice whose stock-manifest row was proven ahead of selection (once).
    source_warmed: Option<String>,
    requested_at: Instant,
    timings: Timings,
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Timings {
    tick_ms: u128,
    stability_ms: u128,
    publication_ms: u128,
}
fn restore_boundary(boundary: &str, seen_match: bool) -> bool {
    matches!(boundary, "main-menu" | "quick-play") || (seen_match && boundary == "selection")
}
fn observe_status_boundary(
    boundary: &str,
    restore_requested: bool,
    confirmed: &mut bool,
    phase: &mut &'static str,
    restore_samples: &mut policy::RestoreSamples,
) {
    if boundary != "match" {
        *confirmed = false;
        if restore_requested {
            *phase = "waiting-for-release";
        } else if boundary != "selection" {
            *phase = "prepared-unverified";
        }
    }
    if !matches!(boundary, "selection" | "main-menu" | "quick-play") {
        *restore_samples = policy::RestoreSamples::new();
    }
}
fn context_unavailable_status(
    error: &anyhow::Error,
    confirmed: &mut bool,
    phase: &mut &'static str,
    restore_samples: &mut policy::RestoreSamples,
    message: &mut String,
) {
    *confirmed = false;
    *phase = "waiting-for-context";
    *restore_samples = policy::RestoreSamples::new();
    *message = format!("Owned gameplay ball held; context unavailable: {error:#}");
}
fn prepared_selection_status(
    fresh_check: Result<()>,
    confirmed: &mut bool,
    phase: &mut &'static str,
    message: &mut String,
) {
    *confirmed = false;
    match fresh_check {
        Ok(()) => {
            *phase = "prepared";
            *message = "Native gameplay ball prepared for the next Blacktop match; visual appearance still needs gameplay verification".into();
        }
        Err(error) => {
            *phase = "prepared-unverified";
            *message = format!(
                "Owned gameplay ball awaiting fresh prepared-selection verification: {error:#}"
            );
        }
    }
}
/// Bounded restore publications per owned edit (each journals an intent).
const RESTORE_FAILURE_LIMIT: u8 = 8;
/// Samples per stability burst: four spaced observations plus slack for one
/// sub-100 ms early sample.
const BURST_ATTEMPTS: usize = 6;
const BURST_SPACING: std::time::Duration = std::time::Duration::from_millis(100);
/// Sleeps one sample spacing without holding the global native write gate.
fn pause(gate: &mut Option<crate::session_safety::WriteGuard>) {
    drop(gate.take());
    std::thread::sleep(BURST_SPACING);
}
/// Retakes the write gate before any journal intent or publication.
fn regain(gate: &mut Option<crate::session_safety::WriteGuard>) -> Result<()> {
    if gate.is_none() {
        *gate = Some(session_safety::write_gate()?);
    }
    Ok(())
}
pub(crate) enum Burst {
    Capture(anyhow::Error),
    Unsafe(anyhow::Error),
}
/// Forward stability burst. Returns None when no ready sample started a
/// sequence (the next poll retries); any failure or mismatch resets samples.
pub(crate) fn sample_burst(
    samples: &mut policy::Samples,
    mut capture: impl FnMut() -> Result<policy::Snapshot>,
    elapsed: impl Fn() -> u64,
    pause: &mut dyn FnMut(),
) -> std::result::Result<Option<policy::Plan>, Burst> {
    for attempt in 0..BURST_ATTEMPTS {
        let snapshot = match capture() {
            Ok(s) => s,
            Err(e) => {
                *samples = policy::Samples::new();
                return Err(Burst::Capture(e));
            }
        };
        match samples.push(snapshot, elapsed()) {
            Ok(Some(plan)) => return Ok(Some(plan)),
            Ok(None) if samples.started() && attempt + 1 < BURST_ATTEMPTS => pause(),
            Ok(None) => return Ok(None),
            Err(e) => {
                *samples = policy::Samples::new();
                return Err(Burst::Unsafe(e));
            }
        }
    }
    Ok(None)
}
impl Runner {
    fn acknowledge_restored(&mut self) {
        self.owned = None;
        self.seen_match = false;
        self.restore_requested = false;
        self.gameplay_model_confirmed = false;
        self.samples = policy::Samples::new();
        self.restore_samples = policy::RestoreSamples::new();
        self.phase = if self.held {
            "restored-held"
        } else {
            "restored"
        };
        self.message = if self.held {
            "Original gameplay ball confirmed after uncertain publication; no forward retries"
        } else {
            "Original gameplay ball restored; next Blacktop selection will rearm"
        }
        .into();
    }
    pub(crate) fn new(binding: Binding, id: &str) -> Result<Self> {
        ball::choice(id)?;
        Ok(Self {
            binding,
            desired: id.into(),
            native: None,
            journal: None,
            owned: None,
            samples: policy::Samples::new(),
            restore_samples: policy::RestoreSamples::new(),
            restore_failures: 0,
            clock: Instant::now(),
            seen_match: false,
            restore_requested: false,
            held: false,
            message: "Waiting for stable empty Blacktop selection".into(),
            phase: "waiting",
            gameplay_model_confirmed: false,
            source_warmed: None,
            requested_at: Instant::now(),
            timings: Timings::default(),
        })
    }
    pub(crate) fn choose(&mut self, id: &str) -> Result<()> {
        ball::choice(id)?;
        ensure!(
            !self.held,
            "Gameplay ball is held until the exact game ends"
        );
        if id != self.desired {
            self.requested_at = Instant::now();
            self.desired = id.into();
            self.restore_requested = self.owned.is_some();
            self.samples = policy::Samples::new();
        }
        Ok(())
    }
    pub(crate) fn status(&self) -> Value {
        json!({"version2":true,"selected":self.desired,"active":self.owned.as_ref().map(|p|p.selected()),"phase":self.phase,"held":self.held,"message":self.message,"ownsMutation":self.owned.is_some(),"gameplayModelConfirmed":self.gameplay_model_confirmed,"visualAppearanceProven":false,"qa":true,"timings":self.timings,"choiceAgeMs":self.requested_at.elapsed().as_millis()})
    }
    pub(crate) fn tick(&mut self, parent_alive: bool) -> Result<()> {
        let started = Instant::now();
        self.timings = Timings::default();
        let result = self.tick_inner(parent_alive);
        self.timings.tick_ms = started.elapsed().as_millis();
        result
    }
    fn tick_inner(&mut self, parent_alive: bool) -> Result<()> {
        // Released only while a stability burst sleeps between samples.
        let mut gate = Some(session_safety::write_gate()?);
        if self.held && self.owned.is_none() {
            return Ok(());
        }
        if self.native.is_none() {
            if !parent_alive || ball::current(&self.desired) {
                return Ok(());
            }
            match native::Native::open(&self.binding) {
                Ok(n) => self.native = Some(n),
                Err(e) => {
                    self.message =
                        format!("Waiting for exact Version 2 gameplay-ball owner: {e:#}");
                    return Ok(());
                }
            }
        }
        let n = self.native.as_mut().unwrap();
        if let Some(plan) = &self.owned {
            let context = match crate::rules_v2::context(n) {
                Ok(c) => c,
                Err(e) => {
                    context_unavailable_status(
                        &e,
                        &mut self.gameplay_model_confirmed,
                        &mut self.phase,
                        &mut self.restore_samples,
                        &mut self.message,
                    );
                    return Ok(());
                }
            };
            let boundary = crate::rules_v2::boundary(&context, n.base());
            if boundary == "match" {
                self.seen_match = true;
                self.phase = "prepared-unverified";
                self.message =
                    "Court ball asset prepared; appearance needs in-game verification".into();
                match policy::observe_gameplay(n).and_then(|(_, models)| {
                    Ok(models.contains(&plan.court()) && plan.court_shows_choice(n)?)
                }) {
                    Ok(shown) => {
                        self.gameplay_model_confirmed = shown;
                        if shown {
                            self.phase = "gameplay-model-confirmed";
                            self.message="Court ball loaded from the selected asset; visual appearance still needs gameplay verification".into();
                        } else {
                            self.message="Court ball has not loaded the selected asset yet; no active-match writes attempted".into();
                        }
                    }
                    Err(e) => {
                        self.gameplay_model_confirmed = false;
                        self.message = format!("Gameplay ball observation unavailable: {e:#}");
                    }
                }
            }
            self.restore_requested |=
                !parent_alive || self.held || restore_boundary(boundary, self.seen_match);
            observe_status_boundary(
                boundary,
                self.restore_requested,
                &mut self.gameplay_model_confirmed,
                &mut self.phase,
                &mut self.restore_samples,
            );
            if !self.restore_requested {
                if boundary == "selection" && !self.seen_match {
                    prepared_selection_status(
                        plan.prepared(n),
                        &mut self.gameplay_model_confirmed,
                        &mut self.phase,
                        &mut self.message,
                    );
                }
                return Ok(());
            }
            if plan.restored(n).is_ok() {
                if let Some(j) = self.journal.as_mut() {
                    if let Err(e) = j.note("restored") {
                        self.held = true;
                        self.message = format!(
                            "Original gameplay ball confirmed; completion note held: {e:#}"
                        );
                        return Ok(());
                    }
                }
                self.acknowledge_restored();
                return Ok(());
            }
            if self.restore_failures >= RESTORE_FAILURE_LIMIT {
                // Every attempt journals an intent: stop before the journal
                // fills. The swapped filename ends with the game process.
                return Ok(());
            }
            let clock = self.clock;
            let mut attempts = 0;
            let permit = match loop {
                let elapsed = u64::try_from(clock.elapsed().as_millis()).unwrap_or(u64::MAX);
                let result = plan.restore_stable(&*n, &mut self.restore_samples, elapsed);
                attempts += 1;
                // Same WPF AssetStable burst for the restore direction.
                if matches!(result, Ok(None))
                    && self.restore_samples.started()
                    && attempts < BURST_ATTEMPTS
                {
                    pause(&mut gate);
                    continue;
                }
                break result;
            } {
                Ok(Some(p)) => p,
                Ok(None) => {
                    self.message =
                        "Confirming four stable unloaded gameplay-ball restore samples".into();
                    return Ok(());
                }
                Err(e) => {
                    self.message =
                        format!("Waiting for exact-owned unloaded gameplay-ball restore: {e:#}");
                    return Ok(());
                }
            };
            regain(&mut gate)?;
            let j = self
                .journal
                .as_mut()
                .context("Owned gameplay ball lost its durable journal")?;
            if let Err(e) = j
                .intent(&permit)
                .and_then(|_| n.publish(&permit, &|| j.verify(&permit)))
            {
                self.held = true;
                self.restore_failures = self.restore_failures.saturating_add(1);
                // Each retry needs fresh stable samples, not one per tick.
                self.restore_samples = policy::RestoreSamples::new();
                self.message = if self.restore_failures >= RESTORE_FAILURE_LIMIT {
                    format!(
                        "Gameplay-ball restore held after {RESTORE_FAILURE_LIMIT} attempts; the original returns when the game exits: {e:#}"
                    )
                } else {
                    format!("Gameplay-ball restore held: {e:#}")
                };
                return Ok(());
            }
            // Retain in-memory ownership if the durable completion note fails. The next
            // poll then sees the restored value and does not publish a new forward edit.
            if let Err(e) = j.note("restored") {
                self.held = true;
                self.message = format!("Gameplay ball restored, journal completion held: {e:#}");
                return Ok(());
            }
            self.owned = None;
            self.seen_match = false;
            self.restore_requested = false;
            self.gameplay_model_confirmed = false;
            self.samples = policy::Samples::new();
            self.restore_samples = policy::RestoreSamples::new();
            self.phase = "restored";
            self.message = "Original court ball asset restored".into();
            return Ok(());
        }
        if !parent_alive || self.held || ball::current(&self.desired) {
            return Ok(());
        }
        // A full capture resolves the whole live ball registry; outside
        // selection it can never be ready, so only the cheap context is read.
        match crate::rules_v2::context(n) {
            Ok(c) if crate::rules_v2::boundary(&c, n.base()) == "selection" => {}
            Ok(_) => {
                self.samples = policy::Samples::new();
                self.message = "Waiting for stable empty Blacktop selection".into();
                // WPF read the manifest before selection; prove the row once
                // here so selection pays only the identity check. A failure
                // is reported (and refused) by forward_checked at selection.
                if self.source_warmed.as_deref() != Some(self.desired.as_str()) {
                    self.source_warmed = Some(self.desired.clone());
                    let _ = n.source_available(&self.desired);
                }
                return Ok(());
            }
            Err(e) => {
                self.samples = policy::Samples::new();
                self.message = format!("Waiting for safe gameplay-ball selection: {e:#}");
                return Ok(());
            }
        }
        // WPF AssetStable: once the first ready sample is seen, take the other
        // three 100 ms apart now instead of one per provider poll.
        let clock = self.clock;
        let elapsed = move || u64::try_from(clock.elapsed().as_millis()).unwrap_or(u64::MAX);
        let stability_started = Instant::now();
        let burst = sample_burst(
            &mut self.samples,
            || policy::capture(&*n, &self.desired),
            elapsed,
            &mut || pause(&mut gate),
        );
        self.timings.stability_ms = stability_started.elapsed().as_millis();
        let plan = match burst {
            Ok(Some(p)) => p,
            Ok(None) => return Ok(()),
            Err(Burst::Capture(e)) => {
                self.message = format!("Waiting for safe gameplay-ball selection: {e:#}");
                return Ok(());
            }
            Err(Burst::Unsafe(e)) => {
                self.message = format!("Gameplay ball not yet safe: {e:#}");
                return Ok(());
            }
        };
        regain(&mut gate)?;
        let n = self.native.as_mut().unwrap();
        let permit = match plan.forward_checked(&*n, &|selected| n.source_available(selected)) {
            Ok(p) => p,
            Err(e) => {
                self.samples = policy::Samples::new();
                self.message =
                    format!("Gameplay-ball source or selection changed; no write: {e:#}");
                return Ok(());
            }
        };
        if self.journal.is_none() {
            match Journal::create(&self.binding, &self.desired) {
                Ok(j) => self.journal = Some(j),
                Err(e) => {
                    self.held = true;
                    self.phase = "held";
                    self.message =
                        format!("Gameplay-ball durable intent unavailable; no write: {e:#}");
                    return Ok(());
                }
            }
        }
        let j = self.journal.as_mut().unwrap();
        if let Err(e) = j.intent(&permit) {
            self.held = true;
            self.message = format!("Gameplay-ball intent failed; no write: {e:#}");
            return Ok(());
        }
        // Retain before attempting WriteProcessMemory, including a partial/unknown
        // native result. No automatic forward retry after any publication attempt.
        let publication_started = Instant::now();
        let result = n.publish(&permit, &|| j.verify(&permit));
        self.timings.publication_ms = publication_started.elapsed().as_millis();
        drop(permit);
        self.owned = Some(plan);
        self.samples = policy::Samples::new();
        if let Err(e) = result {
            self.held = true;
            self.restore_requested = true;
            self.phase = "held";
            self.message =
                format!("Gameplay-ball publication uncertain; safe restore retained: {e:#}");
            return Ok(());
        }
        if let Err(e) = j.note("prepared") {
            self.held = true;
            self.restore_requested = true;
            self.message = format!("Gameplay-ball prepared; durable completion held: {e:#}");
            return Ok(());
        }
        self.phase = "prepared";
        self.message =
            "Gameplay ball prepared for next Blacktop match; visual verification pending".into();
        Ok(())
    }
    pub(crate) fn finish_ended(&mut self, rows: &[session_safety::Process]) -> Result<()> {
        ensure!(
            !rows
                .iter()
                .any(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe")),
            "Game present; gameplay-ball recovery held"
        );
        if let Some(n) = &self.native {
            n.ended()?;
        }
        drop(self.journal.take());
        remove_ended(
            &self.binding.portable,
            &self.binding.game,
            &self.binding.session,
            rows,
        )?;
        self.owned = None;
        self.native = None;
        Ok(())
    }
}
pub(crate) fn pending(root: &Path) -> Result<bool> {
    match fs::symlink_metadata(root.join("Data").join(MARKER)) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}
pub(crate) fn remove_ended(
    root: &Path,
    game: &Identity,
    session: &str,
    rows: &[session_safety::Process],
) -> Result<()> {
    if !pending(root)? {
        return Ok(());
    }
    ensure!(
        !rows
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe")),
        "Close every NBA2K27 before gameplay-ball receipt recovery"
    );
    let _pins = [
        crate::loader_files::Directories::pin(root)?,
        crate::loader_files::Directories::pin(&root.join("Data"))?,
    ];
    let mut file = fs::OpenOptions::new()
        .read(true)
        .access_mode(0x80000000 | 0x10000)
        .share_mode(1)
        .custom_flags(0x00200000)
        .open(root.join("Data").join(MARKER))?;
    let meta = file.metadata()?;
    ensure!(
        meta.is_file() && meta.file_attributes() & 0x400 == 0 && (1..=LIMIT).contains(&meta.len()),
        "Unsupported gameplay-ball receipt retained"
    );
    let mut bytes = Vec::new();
    (&mut file).take(LIMIT + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 == meta.len() && bytes.last() == Some(&b'\n'),
        "Incomplete gameplay-ball receipt retained"
    );
    let mut lines = bytes[..bytes.len() - 1].split(|b| *b == b'\n');
    let h: Header = serde_json::from_slice(lines.next().context("Missing gameplay-ball header")?)?;
    h.validate()?;
    ensure!(
        h.game == *game
            && h.session == session
            && h.file_identity == crate::file_identity::Stable::read(&file)?,
        "Gameplay-ball receipt belongs to another session/file"
    );
    for (i, line) in lines.enumerate() {
        ensure!(i < 2048, "Gameplay-ball receipt event limit");
        let v: Value = serde_json::from_slice(line)?;
        ensure!(
            matches!(
                v["event"].as_str(),
                Some("intent" | "prepared" | "restored")
            ),
            "Unknown gameplay-ball receipt event retained"
        );
        if v["event"] == "intent" {
            ensure!(
                v["sha256"].as_str() == Some(hash(&v["transaction"])?.as_str()),
                "Gameplay-ball receipt transaction digest differs"
            );
        }
    }
    crate::loader_files::unlink(&file)
}
#[cfg(test)]
#[path = "ball_gameplay_runtime_tests.rs"]
mod tests;
