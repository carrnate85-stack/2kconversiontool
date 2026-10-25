//! A single provider-owned Version 2 feature session. Frozen next-launch
//! choices, feature-specific gates, durable intent and exact data ownership.
use crate::{
    blacktop_native::{
        Memory,
        feature_writer::{Access, Feature},
    },
    feature_journal::{Change, Event, Journal},
    feature_plan::Plan,
    session_safety::Identity,
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// Informational timing only: these facts can never authorize an effect.
/// Keep first evidence through mode exits and ended status without extra reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum ScalerMoment {
    Attempt,
    ReadinessRefused,
    PlanReady,
    GlobalApplied,
    PlayerCacheObserved,
    NpcWriteVerified,
    NpcCacheVerified,
    ResetObserved,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScalerFact {
    phase: ScalerMoment,
    elapsed_ms: u64,
    attempt: u32,
    mode: Option<u32>,
    loaded_players: Option<usize>,
    note: Option<String>,
}
// Exact diagnostic allowlist only. Unknown lower-level transport errors may
// include arbitrary OS text, paths or addresses; keep those out of timing facts.
// Matching here NEVER participates in admission, retries, holds or writes.
fn scaler_refusal_note(error: &anyhow::Error) -> &'static str {
    const KNOWN: &[&str] = &[
        "Waiting for the main menu, Freestyle or Blacktop",
        "Waiting for a stable player wrapper table",
        "Waiting for verified player managers",
        "Waiting for verified player objects",
        "Player roster-manager ownership differs",
        "Player has not initialized its native shared-size opt-in",
        "Waiting for stable native player size data",
        "Player changed during scaler snapshot",
        "The game is changing players or modes; no scaler write planned",
        "Waiting for the original player size input",
        "Player size was changed outside this session",
        "Scaler context changed after preparation",
        "Invalid scaler address",
        "Invalid scaler object span",
        "Partial scaler snapshot; no write planned",
        "Invalid scaler executable header",
        "Invalid scaler PE header",
        "This game build has no verified scaler profile",
        "Scaler code anchor differs",
        "Scaler anchor has a different target",
        "Scaler object-field signature differs",
        "Scaler class-table signature differs",
        "Scaler virtual method differs",
        "Duplicate scaler player manager",
        "Duplicate scaler player actor",
    ];
    for cause in error.chain() {
        let text = cause.to_string();
        if let Some(label) = KNOWN.iter().find(|label| **label == text) {
            return label;
        }
    }
    "Unclassified readiness refusal"
}
#[derive(Default, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScalerTiming {
    #[serde(skip)]
    attempts: u32,
    facts: Vec<ScalerFact>,
}
impl ScalerTiming {
    fn record(
        &mut self,
        phase: ScalerMoment,
        elapsed_ms: u64,
        mode: Option<u32>,
        loaded_players: Option<usize>,
        note: Option<&str>,
    ) {
        if self.facts.len() >= 16 || self.facts.iter().any(|fact| fact.phase == phase) {
            return;
        }
        self.facts.push(ScalerFact {
            phase,
            elapsed_ms,
            attempt: self.attempts,
            mode,
            loaded_players,
            note: note.map(|text| text.chars().take(256).collect()),
        });
    }
}
/// Diagnostics only. An entered writer may refuse before any effect; a
/// successful return verifies its write, not the later transaction commit.
#[derive(Clone, Default, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NpcWriterCounts {
    entered: u32,
    verified_flag_returns: u32,
    verified_cache_returns: u32,
    uncertain_returns: u32,
    first_entered_ms: Option<u64>,
    last_entered_ms: Option<u64>,
}
impl NpcWriterCounts {
    fn enter(&mut self, elapsed_ms: u64) {
        self.entered = self.entered.saturating_add(1);
        self.first_entered_ms.get_or_insert(elapsed_ms);
        self.last_entered_ms = Some(elapsed_ms);
    }
    fn returned(&mut self, bytes: usize, ok: bool) {
        match (ok, bytes) {
            (true, 1) => self.verified_flag_returns = self.verified_flag_returns.saturating_add(1),
            (true, 8) => {
                self.verified_cache_returns = self.verified_cache_returns.saturating_add(1)
            }
            _ => self.uncertain_returns = self.uncertain_returns.saturating_add(1),
        }
    }
}
#[derive(Clone, Default, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScalerNpcEffects {
    total: NpcWriterCounts,
    coach: NpcWriterCounts,
    cheerleader: NpcWriterCounts,
    arena: NpcWriterCounts,
    celebrity: NpcWriterCounts,
    unclassified: NpcWriterCounts,
}
impl ScalerNpcEffects {
    fn group(&mut self, group: Option<crate::scaler_npc_v2::Group>) -> &mut NpcWriterCounts {
        use crate::scaler_npc_v2::Group;
        match group {
            Some(Group::Coach) => &mut self.coach,
            Some(Group::Cheerleader) => &mut self.cheerleader,
            Some(Group::Arena) => &mut self.arena,
            Some(Group::Celebrity) => &mut self.celebrity,
            None => &mut self.unclassified,
        }
    }
    fn enter(&mut self, group: Option<crate::scaler_npc_v2::Group>, elapsed_ms: u64) {
        self.total.enter(elapsed_ms);
        self.group(group).enter(elapsed_ms);
    }
    fn returned(&mut self, group: Option<crate::scaler_npc_v2::Group>, bytes: usize, ok: bool) {
        self.total.returned(bytes, ok);
        self.group(group).returned(bytes, ok);
    }
}
struct Backoff<T> {
    last: Option<T>,
}
impl<T> Default for Backoff<T> {
    fn default() -> Self {
        Self { last: None }
    }
}
impl<T: PartialEq> Backoff<T> {
    fn same(&self, value: &T) -> bool {
        self.last.as_ref() == Some(value)
    }
    fn remember(&mut self, value: T) {
        self.last = Some(value);
    }
    fn clear(&mut self) {
        self.last = None;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScalerPublication {
    Active,
    StockReset,
    Conflict,
}

fn scaler_publication(current: u32, target: u32) -> ScalerPublication {
    if current == target {
        ScalerPublication::Active
    } else if current == crate::scaler_policy::ORIGINAL {
        ScalerPublication::StockReset
    } else {
        ScalerPublication::Conflict
    }
}

// Ticks run every 500 ms from loader-armed. WPF treated "camera data could
// not be opened" as transient for 20 seconds; match that window.
const ACCESS_OPEN_FAILURE_LIMIT: u8 = 40;

/// A publication attempt that failed AND was verified fully restored. Typed,
/// never inferred from text: an unrestorable failure still holds.
#[derive(Debug)]
struct CameraPublicationDeferred(String);
impl std::fmt::Display for CameraPublicationDeferred {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Camera publication was interrupted and fully restored: {}",
            self.0
        )
    }
}
impl std::error::Error for CameraPublicationDeferred {}
const CAMERA_RESTORED_LIMIT: u8 = 10;
/// Retries for one scaler character whose data changes under its write.
const NPC_MOVED_LIMIT: u8 = 3;
/// Bounded write count of one guarded transaction.
const MAX_TRANSACTION: usize = 128;
/// Worst-case journal events of one transaction: its write intent, one
/// protection intent per read-only page write, and its applied receipt.
fn transaction_events(changes: usize) -> usize {
    changes.saturating_add(2)
}
/// A camera publication plus the restore of every edit it could have made.
fn camera_publication_events(edits: usize) -> usize {
    transaction_events(edits).saturating_mul(2)
}
/// Opportunistic character-group writes stop before they could consume the
/// budget a camera publication needs to journal itself and its full restore.
const CAMERA_RESTORE_RESERVE: usize = 2 * (MAX_TRANSACTION + 2);
/// Every one of CAMERA_RESTORED_LIMIT attempts was verified fully restored.
/// Typed so only this (and an access that never opened) can be retried.
#[derive(Debug)]
struct CameraRestoredLimit(String);
impl std::fmt::Display for CameraRestoredLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Camera publication failed {CAMERA_RESTORED_LIMIT} times; every attempt was fully restored. Last: {}",
            self.0
        )
    }
}
impl std::error::Error for CameraRestoredLimit {}

/// WPF camera:start (Retry cameras) and camera:stop (Stop & restore).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CameraCommand {
    Retry,
    Stop,
}

/// Stop & restore: reverse every public camera edit through the plan's own
/// ownership rules. A foreign value refuses the whole restore before writing.
fn stop_and_restore<A: DataAccess>(
    access: &mut A,
    journal: &mut Journal,
    plan: &crate::camera_v2::Plan,
) -> Result<()> {
    let edits = plan.restore(access)?;
    if edits.is_empty() {
        return Ok(());
    }
    apply(access, journal, Feature::Camera, changes_of(&edits))
}

fn changes_of(edits: &[crate::camera_v2::Edit]) -> Vec<Change> {
    edits
        .iter()
        .map(|e| Change {
            at: e.at,
            before: e.before.clone(),
            after: e.after.clone(),
        })
        .collect()
}

/// All-or-nothing publication, matching the WPF helper's RestoreAll. A
/// failure after any write is reversed using the plan's tolerant ownership
/// rules (game-owned settings/runtime bytes adopted, foreign values refused).
fn publish_or_restore<A: DataAccess>(
    access: &mut A,
    journal: &mut Journal,
    plan: &crate::camera_v2::Plan,
) -> Result<()> {
    let error = match apply(access, journal, Feature::Camera, changes_of(&plan.edits)) {
        Ok(()) => return Ok(()),
        Err(error) => error,
    };
    match plan
        .restore(access)
        .and_then(|edits| apply(access, journal, Feature::Camera, changes_of(&edits)))
    {
        Ok(()) => Err(CameraPublicationDeferred(format!("{error:#}")).into()),
        Err(restore) => {
            anyhow::bail!("Camera publication failed: {error:#}; restore also failed: {restore:#}")
        }
    }
}

/// Only emitted after this transaction's entire changed prefix is restored.
/// The real caller additionally proves the complete category tables and source.
#[derive(Debug)]
struct FreestylePublicationRestored(String);
impl std::fmt::Display for FreestylePublicationRestored {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Freestyle publication was interrupted and restored: {}",
            self.0
        )
    }
}
impl std::error::Error for FreestylePublicationRestored {}
/// Only constructed before journal intent/protection/write entry. Never inferred
/// from zero observed after-values: an uncertain attempted effect stays terminal.
#[derive(Debug)]
struct FreestylePreEffectDeferred(String);
impl std::fmt::Display for FreestylePreEffectDeferred {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Freestyle readiness changed before any publication intent or effect: {}",
            self.0
        )
    }
}
impl std::error::Error for FreestylePreEffectDeferred {}

/// All-or-nothing Freestyle category publication. The four DWORDs form two
/// navigation permutations: a prefix left behind by a failed guarded write
/// (for example the picker moving between writes) would make a category
/// unreachable for the session. Exactly the owned prefix is reversed; any
/// other state (a foreign value) is refused before writing and held.
fn publish_freestyle_or_restore<A: DataAccess>(
    access: &mut A,
    journal: &mut Journal,
    changes: Vec<Change>,
    mut check: impl FnMut(&A, usize) -> Result<()>,
) -> Result<()> {
    ensure!(
        !changes.is_empty() && changes.len() <= MAX_TRANSACTION,
        "Freestyle transaction bounds differ"
    );
    ensure!(
        journal.remaining() >= transaction_events(changes.len()).saturating_mul(2),
        "Feature intent budget cannot cover a Freestyle publication and its restore; nothing written"
    );
    // Every entry must start at its stock value, so any later prefix found at
    // its after-value was written by this transaction and is ours to reverse.
    for change in &changes {
        ensure!(
            access.read(change.at, change.before.len())? == change.before,
            "Feature changed before durable write intent"
        );
    }
    // Explicit pre-effect phase: no journal intent/protection/write API has
    // been entered. Only typed readiness differences may wait without a hold.
    if let Err(error) = check(access, 0) {
        if error.is::<crate::freestyle_v2::ReadinessChanged>() {
            return Err(FreestylePreEffectDeferred(format!("{error:#}")).into());
        }
        return Err(error);
    }
    for change in &changes {
        ensure!(
            access.read(change.at, change.before.len())? == change.before,
            "Freestyle changed before durable publication intent"
        );
    }
    // Entering intent is the uncertainty boundary. Any error from here is
    // terminal after exact conditional cleanup, even if no word appears changed.
    let publication = (|| -> Result<()> {
        let intent = journal.write_intent(Feature::Freestyle.name(), changes.clone())?;
        for (index, change) in changes.iter().enumerate() {
            check(access, index)?;
            access.write_intended(Feature::Freestyle, change, &intent, journal)?;
        }
        check(access, changes.len())?;
        journal.append(Event::Applied {
            feature: Feature::Freestyle.name().into(),
        })
    })();
    let error = match publication {
        Ok(()) => return Ok(()),
        Err(error) => error,
    };
    let restored = (|| -> Result<bool> {
        let mut applied = 0;
        for change in &changes {
            if access.read(change.at, change.after.len())? != change.after {
                break;
            }
            applied += 1;
        }
        for change in &changes[applied..] {
            ensure!(
                access.read(change.at, change.before.len())? == change.before,
                "Freestyle restore conflict; foreign category values left untouched"
            );
        }
        if applied == 0 {
            return Ok(false);
        }
        let reverse = changes[..applied]
            .iter()
            .rev()
            .map(|c| Change {
                at: c.at,
                before: c.after.clone(),
                after: c.before.clone(),
            })
            .collect();
        apply(access, journal, Feature::Freestyle, reverse)?;
        Ok(true)
    })();
    match restored {
        Ok(_) => Err(FreestylePublicationRestored(format!("{error:#}")).into()),
        Err(restore) => anyhow::bail!(
            "Freestyle publication failed: {error:#}; restore also failed: {restore:#}"
        ),
    }
}

/// The WPF helper takes complete picker/roster proofs immediately before its
/// tight four-DWORD publication and immediately after it.  Keep the same
/// narrow mutation window here: each DWORD still has the durable intent,
/// expected-prefix and read-back guards in `publish_freestyle_or_restore`, but
/// the expensive complete picker snapshot is not repeated between adjacent
/// owned words.  Repeating it there made normal picker animation wide enough
/// to move the selected-category window after a prefix write, forcing an exact
/// restore and a permanent session hold (QA094 live component 0x200).
fn freestyle_full_guard(index: usize, changes: usize) -> bool {
    index == 0 || index == changes
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FeatureLane {
    Camera,
    Scaler,
}
fn camera_scaler_order(initial_scaler: bool, camera_cleanup: bool) -> [FeatureLane; 2] {
    if initial_scaler && !camera_cleanup {
        [FeatureLane::Scaler, FeatureLane::Camera]
    } else {
        [FeatureLane::Camera, FeatureLane::Scaler]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AccessOpenDisposition {
    Waiting { attempt: u8, retries_left: u8 },
    Held,
}

#[derive(Default)]
struct AccessOpenRetry {
    failures: u8,
}
impl AccessOpenRetry {
    fn failed(&mut self) -> AccessOpenDisposition {
        self.failures = self.failures.saturating_add(1);
        if self.failures >= ACCESS_OPEN_FAILURE_LIMIT {
            AccessOpenDisposition::Held
        } else {
            AccessOpenDisposition::Waiting {
                attempt: self.failures,
                retries_left: ACCESS_OPEN_FAILURE_LIMIT - self.failures,
            }
        }
    }
    fn succeeded(&mut self) {
        self.failures = 0;
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    phase: String,
    message: String,
    /// Cameras only: held for a reason Retry may safely re-arm.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    retryable: bool,
}
/// Experimental mixed NBA+WNBA Learn2K roster route (wnba_roster_v2.rs). The
/// provider publishes `ready`; categories are applied only after publication
/// and a strict deep-verified mixed roster in the live Freestyle picker.
#[derive(Default)]
struct MixedRoster {
    published: bool,
    build: Option<std::thread::JoinHandle<Result<crate::wnba_roster_v2::Prepared>>>,
    ready: Option<crate::wnba_roster_v2::Prepared>,
    failed: bool,
}
pub(crate) struct Runner {
    mixed: MixedRoster,
    root: PathBuf,
    game: PathBuf,
    session: String,
    identity: Identity,
    parent: Identity,
    plan: Plan,
    access: Option<Access>,
    access_open_retry: AccessOpenRetry,
    gate_retry: AccessOpenRetry,
    journal: Option<Journal>,
    camera: Option<crate::camera_v2::Plan>,
    scaler_applied: bool,
    scaler_reset: bool,
    scaler_clock: Instant,
    scaler_timing: ScalerTiming,
    scaler_npc_effects: ScalerNpcEffects,
    freestyle_applied: bool,
    freestyle_loader_armed: bool,
    freestyle_loader_deadline: Option<Instant>,
    freestyle_loader_loaded: bool,
    freestyle_missing: Backoff<crate::freestyle_v2::RosterFingerprint>,
    states: BTreeMap<&'static str, Status>,
    camera_restored_attempts: u8,
    camera_stop_pending: bool,
    scaler_stable_menu: Option<String>,
    scaler_menu_samples: u8,
    npc_owned: BTreeMap<u64, crate::scaler_npc_v2::Owned>,
    npc_profile: bool,
    npc_held: Option<String>,
    npc_moved: BTreeMap<u64, u8>,
    debug_applied: bool,
    debug_snapshot: Option<crate::debug_menu_v2::Snapshot>,
    debug_started: Instant,
}
impl Runner {
    pub fn wnba_loader_enabled(&self) -> bool {
        self.plan.wnba_freestyle && crate::wnba_loader_v2::RUNTIME_ENABLED
    }
    pub fn new(
        root: &Path,
        game: &Path,
        session: &str,
        identity: &Identity,
        parent: &Identity,
        plan: &Plan,
    ) -> Result<Self> {
        plan.validate()?;
        let mut runner = Self {
            mixed: MixedRoster::default(),
            root: root.into(),
            game: game.into(),
            session: session.into(),
            identity: identity.clone(),
            parent: parent.clone(),
            plan: plan.clone(),
            access: None,
            access_open_retry: AccessOpenRetry::default(),
            gate_retry: AccessOpenRetry::default(),
            journal: None,
            camera: None,
            scaler_applied: false,
            scaler_reset: false,
            scaler_clock: Instant::now(),
            scaler_timing: ScalerTiming::default(),
            scaler_npc_effects: ScalerNpcEffects::default(),
            freestyle_applied: false,
            freestyle_loader_armed: false,
            freestyle_loader_deadline: None,
            freestyle_loader_loaded: false,
            freestyle_missing: Backoff::default(),
            states: BTreeMap::new(),
            camera_restored_attempts: 0,
            camera_stop_pending: false,
            scaler_stable_menu: None,
            scaler_menu_samples: 0,
            npc_owned: BTreeMap::new(),
            npc_profile: false,
            npc_held: None,
            npc_moved: BTreeMap::new(),
            debug_applied: false,
            debug_snapshot: None,
            debug_started: Instant::now(),
        };
        for (name, enabled) in [
            ("cameras", !plan.cameras.is_empty()),
            ("scaler", plan.scaler_percent.is_some()),
            ("debug-menus", plan.debug_menus),
        ] {
            runner.set(
                name,
                if enabled { "waiting" } else { "disabled" },
                if enabled {
                    "Waiting for this owned game's verified Version 2 context."
                } else {
                    "Not requested for this launch."
                },
            );
        }
        if let Some(variant) = plan.wnba_mixed {
            runner.set(
                "wnba-freestyle",
                "preparing",
                &format!(
                    "Experimental mixed NBA + WNBA roster ({}): waiting for the game's roster key to build it from your installed files. Stay out of Freestyle until it is ready.",
                    variant.name()
                ),
            );
            return Ok(runner);
        }
        if plan.wnba_priority_categories {
            runner.set(
                "wnba-freestyle",
                "waiting-source",
                "Preparing WNBA teams for Freestyle in this owned game session.",
            );
            return Ok(runner);
        }
        runner.set(
            "wnba-freestyle",
            if plan.wnba_freestyle {
                "held"
            } else {
                "disabled"
            },
            if plan.wnba_freestyle {
                crate::wnba_loader_v2::RUNTIME_BLOCKER
            } else {
                "Not requested for this launch."
            },
        );
        Ok(runner)
    }
    /// The provider published a cached/pre-launch build before Steam dispatch.
    pub fn mixed_roster_prepublished(&mut self, summary: &crate::wnba_roster_v2::Summary) {
        self.mixed_roster_published(summary);
    }
    /// A finished in-session build waiting for provider publication.
    pub fn take_mixed_roster(&mut self) -> Option<crate::wnba_roster_v2::Prepared> {
        self.mixed.ready.take()
    }
    pub fn mixed_roster_published(&mut self, summary: &crate::wnba_roster_v2::Summary) {
        self.mixed.published = true;
        let message = if summary.variant.exposes_categories() {
            format!(
                "Mixed roster ready ({} teams: {} NBA, {} WNBA). Enter Freestyle from the main menu; WNBA categories are added once the picker shows this roster.",
                summary.teams, summary.nba, summary.wnba
            )
        } else {
            format!(
                "Clubs-only roster test ready ({} teams). Freestyle categories stay stock: WPF requires both national teams.",
                summary.teams
            )
        };
        self.set("wnba-freestyle", "roster-ready", &message);
    }
    pub fn mixed_roster_publication_failed(&mut self, error: &anyhow::Error) {
        self.mixed.failed = true;
        self.record_failure("wnba-freestyle", error);
    }
    /// Read-only key capture, then one background build from installed files.
    fn mixed_roster_tick(&mut self, access: &Access, variant: crate::wnba_roster_v2::Variant) {
        if self.mixed.published || self.mixed.ready.is_some() || self.mixed.failed {
            return;
        }
        if let Some(handle) = &self.mixed.build {
            if !handle.is_finished() {
                return;
            }
            let handle = self.mixed.build.take().unwrap();
            match handle.join() {
                Ok(Ok(prepared)) => {
                    self.set(
                        "wnba-freestyle",
                        "publishing",
                        "Mixed roster built and verified; publishing it to the provider.",
                    );
                    self.mixed.ready = Some(prepared);
                }
                Ok(Err(error)) => {
                    self.mixed.failed = true;
                    self.record_failure("wnba-freestyle", &error);
                }
                Err(_) => {
                    self.mixed.failed = true;
                    self.record_failure(
                        "wnba-freestyle",
                        &anyhow::anyhow!(
                            "Mixed roster build stopped unexpectedly; stock roster kept"
                        ),
                    );
                }
            }
            return;
        }
        let key = match crate::wnba_roster_v2::capture_key(access) {
            Ok(key) => key,
            Err(error) => {
                self.set(
                    "wnba-freestyle",
                    "preparing",
                    &format!("Waiting for the game's roster key (read-only): {error:#}"),
                );
                return;
            }
        };
        let (root, game) = (self.root.clone(), self.game.clone());
        self.mixed.build = Some(std::thread::spawn(move || {
            crate::wnba_roster_v2::build(&root, &game, variant, &key, || Ok(()))
        }));
        self.set(
            "wnba-freestyle",
            "building",
            "Roster key read; building the mixed roster from your installed game files.",
        );
    }
    fn set(&mut self, name: &'static str, phase: &str, message: &str) {
        self.states.insert(
            name,
            Status {
                phase: phase.into(),
                message: message.chars().take(1024).collect(),
                retryable: false,
            },
        );
    }
    fn held(&self, name: &str) -> bool {
        self.states.get(name).is_some_and(|s| s.phase == "held")
    }
    /// Held, disabled, or stopped by the user: no ticks and no status reuse.
    fn idle(&self, name: &str) -> bool {
        self.states
            .get(name)
            .is_some_and(|s| matches!(s.phase.as_str(), "held" | "disabled" | "stopped"))
    }
    fn hold_camera_retryable(&mut self, error: &anyhow::Error) {
        self.record_failure("cameras", error);
        if let Some(status) = self.states.get_mut("cameras") {
            status.retryable = true;
            status.message.push_str(" Retry cameras is available.");
        }
    }
    /// Queues a user camera command for the next tick, which performs it
    /// through the same verified access, profile and ownership proofs as
    /// automatic publication. Retry only re-arms a restorable hold or a stop.
    pub fn camera_command(&mut self, command: CameraCommand) -> Result<&'static str> {
        ensure!(
            !self.plan.cameras.is_empty(),
            "Cameras were not requested for this launch"
        );
        let status = self
            .states
            .get("cameras")
            .context("Camera status unavailable")?;
        match command {
            CameraCommand::Retry => {
                ensure!(
                    status.phase == "stopped" || (status.phase == "held" && status.retryable),
                    "Cameras can be retried only after Stop or a fully restored failure; this hold keeps its no-further-writes rule"
                );
                self.camera_restored_attempts = 0;
                self.camera_stop_pending = false;
                self.access_open_retry.succeeded();
                self.set(
                    "cameras",
                    "waiting",
                    "Retry requested; republishing with the same ownership checks.",
                );
                Ok("Camera retry queued.")
            }
            CameraCommand::Stop => {
                ensure!(
                    matches!(
                        status.phase.as_str(),
                        "waiting" | "active" | "reload" | "reset"
                    ),
                    "Cameras are not running"
                );
                self.camera_stop_pending = true;
                Ok("Camera stop and restore queued.")
            }
        }
    }
    fn camera_stop(&mut self, access: &mut Access) -> Result<()> {
        self.camera_stop_pending = false;
        if let Some(plan) = self.camera.clone() {
            crate::camera_v2::verify_profile(access)?;
            access.begin_batch(Feature::Camera)?;
            let result = match self.journal() {
                Ok(journal) => stop_and_restore(access, journal, &plan),
                Err(error) => Err(error),
            };
            access.end_batch();
            result?;
        }
        self.set(
            "cameras",
            "stopped",
            "Cameras stopped; the game's camera data was restored. Retry cameras republishes them.",
        );
        Ok(())
    }
    pub fn status(&self) -> Value {
        json!({"version2":true,"features":self.states,"scalerTiming":self.plan.scaler_percent.map(|percent| json!({"percent":percent,"origin":"owned-runner-created","informationalOnly":true,"facts":self.scaler_timing.facts,"npcEffects":self.scaler_npc_effects})),"gameProcessStopped":false,"gameplayVerified":false})
    }
    pub(crate) fn observe_blacktop_status(&mut self) {
        if !self.plan.wnba_blacktop_categories {
            return;
        }
        match crate::wnba_priority_launch::blacktop_state(&self.root, &self.session, &self.identity)
        {
            Ok(Some((phase, message))) => self.set("wnba-blacktop", &phase, &message),
            Ok(None) => {}
            Err(_error) => self.set(
                "wnba-blacktop",
                "unverified",
                "WNBA status could not be verified. Close the game and start a new Play session.",
            ),
        }
    }
    fn journal(&mut self) -> Result<&mut Journal> {
        if self.journal.is_none() {
            self.journal = Some(Journal::create(
                &self.root,
                &self.session,
                &self.identity,
                &self.parent,
                &self.plan,
            )?);
        }
        Ok(self.journal.as_mut().unwrap())
    }
    fn record_freestyle_failure(&mut self, error: &anyhow::Error) {
        if error.is::<FreestylePreEffectDeferred>() {
            self.set("wnba-freestyle", "waiting", &format!("Waiting for stable Freestyle readiness; no publication intent or effect attempted: {error:#}"));
            return;
        }
        // WPF does not replay an interrupted publication. A fully restored
        // attempted transaction is still terminal for this owned game session.
        self.record_failure("wnba-freestyle", error);
    }
    fn record_failure(&mut self, name: &'static str, error: &anyhow::Error) {
        let message = format!("{error:#}");
        self.set(
            name,
            "held",
            &format!("No further writes this session: {message}"),
        );
        if let Some(journal) = &mut self.journal {
            let _ = journal.append(Event::Held {
                feature: name.into(),
                message: message.chars().take(1024).collect(),
            });
        }
    }
    fn record_camera_failure(&mut self, error: &anyhow::Error) {
        if error.is::<CameraPublicationDeferred>() {
            self.set(
                "cameras",
                "waiting",
                &format!("{error:#}. Retrying automatically with the same prepared data."),
            );
        } else if error.is::<CameraRestoredLimit>() {
            self.hold_camera_retryable(error);
        } else {
            self.record_failure("cameras", error);
        }
    }
    fn record_access_open_failure(&mut self, error: &anyhow::Error) {
        match self.access_open_retry.failed() {
            AccessOpenDisposition::Waiting {
                attempt,
                retries_left,
            } => {
                for name in ["cameras", "scaler", "wnba-freestyle", "debug-menus"] {
                    if !self.idle(name) {
                        self.set(
                            name,
                            "waiting",
                            &format!(
                                "Native game access is still settling (attempt {attempt}; {retries_left} retries left): {error:#}"
                            ),
                        );
                    }
                }
            }
            AccessOpenDisposition::Held => {
                for name in ["cameras", "scaler", "wnba-freestyle", "debug-menus"] {
                    if !self.idle(name) {
                        if name == "cameras" {
                            // Nothing was written: access never opened.
                            self.hold_camera_retryable(error);
                        } else {
                            self.record_failure(name, error);
                        }
                    }
                }
            }
        }
    }
    /// The provider's per-tick safety gate (exact game identity, protected
    /// process scan, owner liveness) could not be proven. Nothing is written
    /// this tick. Match transitions can briefly deny process metadata, so this
    /// waits like an unopened access; only after the same 20 s window does each
    /// active feature hold. Mod loading and the other runners are unaffected.
    pub fn gate_unavailable(&mut self, error: &anyhow::Error) {
        if !self.plan.enabled() {
            return;
        }
        let names = ["cameras", "scaler", "wnba-freestyle", "debug-menus"];
        match self.gate_retry.failed() {
            AccessOpenDisposition::Waiting {
                attempt,
                retries_left,
            } => {
                for name in names {
                    if !self.idle(name) {
                        self.set(
                            name,
                            "waiting",
                            &format!(
                                "Game safety check is settling; no writes this tick (attempt {attempt}; {retries_left} retries left): {error:#}"
                            ),
                        );
                    }
                }
            }
            AccessOpenDisposition::Held => {
                for name in names {
                    if !self.idle(name) {
                        self.record_failure(name, error);
                    }
                }
            }
        }
    }
    /// A tick returned an error it did not attribute to one feature. Every
    /// active feature holds (no further writes); the session keeps running.
    pub fn tick_failed(&mut self, error: &anyhow::Error) {
        for name in ["cameras", "scaler", "wnba-freestyle", "debug-menus"] {
            if !self.idle(name) {
                self.record_failure(name, error);
            }
        }
    }
    fn rearm_freestyle_roster_loader(&mut self) {
        // The four category DWORDs are session-scoped and intentionally stay
        // applied. Only the stock roster generation is consumed per entry.
        self.freestyle_loader_loaded = false;
        self.freestyle_missing.clear();
    }
    pub fn tick(&mut self, parent_alive: bool) -> Result<()> {
        if !self.plan.enabled() {
            return Ok(());
        }
        self.gate_retry.succeeded();
        if !parent_alive {
            for name in ["cameras", "scaler", "wnba-freestyle", "debug-menus"] {
                if !self.idle(name) {
                    self.set(name,"owner-closed","Launcher closed; no new memory writes. Owned allocations remain until game exit.");
                }
            }
            return Ok(());
        }
        // The saved Freestyle choice is a metadata watcher until the owned
        // launcher publishes its exact-session token. A Play with no other features must
        // not open Access, consume access retry budget, or create an intent.
        let priority_categories_ready = if self.plan.wnba_priority_categories
            && !self.freestyle_applied
            && !self.held("wnba-freestyle")
        {
            match crate::wnba_priority_categories::approval_requested(
                &self.root,
                &self.session,
                &self.identity,
            ) {
                Ok(true) => match crate::wnba_priority_launch::source_state(
                    &self.root,
                    &self.session,
                    &self.identity,
                ) {
                    Ok(crate::wnba_priority_launch::SourceState::Refused(reason)) => {
                        self.record_failure("wnba-freestyle", &anyhow::anyhow!(reason));
                        false
                    }
                    Ok(crate::wnba_priority_launch::SourceState::Registered) => true,
                    Ok(crate::wnba_priority_launch::SourceState::Pending) => false,
                    Err(error) => {
                        self.record_failure("wnba-freestyle", &error);
                        false
                    }
                },
                Ok(false) => false,
                Err(error) => {
                    self.record_failure("wnba-freestyle", &error);
                    false
                }
            }
        } else {
            false
        };
        if self.plan.wnba_priority_categories
            && !self.freestyle_applied
            && !priority_categories_ready
        {
            if !self.held("wnba-freestyle") {
                self.set(
                    "wnba-freestyle",
                    "waiting-source",
                    "Waiting for WNBA roster initialization. Stay at Home or Quick Play until it is ready.",
                );
            }
            let other_features = (!self.plan.cameras.is_empty() && !self.idle("cameras"))
                || (self.plan.scaler_percent.is_some() && !self.idle("scaler"))
                || (self.plan.debug_menus && !self.idle("debug-menus"))
                || self.camera_stop_pending;
            if !other_features {
                return Ok(());
            }
        }
        if !self.camera_stop_pending
            && self
                .states
                .iter()
                // Native Blacktop progress is observation only; it must not
                // cause this Rust runner to acquire game-memory capabilities.
                .filter(|(name, _)| **name != "wnba-blacktop")
                .map(|(_, state)| state)
                .all(|s| matches!(s.phase.as_str(), "held" | "disabled" | "stopped"))
        {
            return Ok(());
        }
        let mut access = match self.access.take() {
            Some(access) => access,
            None => match Access::open(&self.identity, &self.game, &self.parent) {
                Ok(access) => {
                    self.access_open_retry.succeeded();
                    access
                }
                Err(error) => {
                    self.record_access_open_failure(&error);
                    return Ok(());
                }
            },
        };
        // The shared size input should be armed before native player loading,
        // as in WPF's independent scaler helper. Only the first publication gets
        // priority over camera setup; cleanup, reset and observations retain the
        // normal order. Each lane runs at most once with its unchanged guards.
        let initial_scaler = self.plan.scaler_percent.is_some()
            && !self.scaler_applied
            && !self.scaler_reset
            && !self.held("scaler");
        for lane in camera_scaler_order(initial_scaler, self.camera_stop_pending) {
            match lane {
                FeatureLane::Camera => {
                    if self.camera_stop_pending {
                        if let Err(error) = self.camera_stop(&mut access) {
                            self.record_failure("cameras", &error);
                        }
                    } else if !self.plan.cameras.is_empty() && !self.idle("cameras") {
                        if let Err(error) = self.camera_tick(&mut access) {
                            self.record_camera_failure(&error);
                        }
                    }
                }
                FeatureLane::Scaler => {
                    if let Some(percent) = self.plan.scaler_percent.filter(|_| !self.held("scaler"))
                    {
                        if let Err(error) = self.scaler_tick(&mut access, percent as u8) {
                            self.record_failure("scaler", &error);
                        }
                    }
                }
            }
        }
        if self.plan.debug_menus && !self.held("debug-menus") {
            if let Err(error) = self.debug_tick(&mut access) {
                self.record_failure("debug-menus", &error);
            }
        }
        if let Some(variant) = self.plan.wnba_mixed {
            self.mixed_roster_tick(&access, variant);
        }
        // WPF category route. The withdrawn sibling-loader swap stays gated by
        // wnba_loader_v2::RUNTIME_ENABLED; the mixed-roster route instead waits
        // for its published roster and a strict mixed deep check.
        let mixed_categories =
            self.plan.wnba_mixed.is_some_and(|v| v.exposes_categories()) && self.mixed.published;
        if (priority_categories_ready
            || (self.plan.wnba_priority_categories && self.freestyle_applied)
            || mixed_categories
            || (self.plan.wnba_freestyle && crate::wnba_loader_v2::RUNTIME_ENABLED))
            && !self.held("wnba-freestyle")
        {
            if let Err(error) = self.freestyle_tick(&mut access) {
                self.record_freestyle_failure(&error);
            }
        }
        self.access = Some(access);
        Ok(())
    }
    /// The stock loader pointer has to be armed before a fast menu transition,
    /// independently of the one-second camera/scaler/deep-roster cadence.
    pub fn tick_wnba_loader(&mut self, parent_alive: bool) -> Result<()> {
        if !self.plan.wnba_freestyle
            || !crate::wnba_loader_v2::RUNTIME_ENABLED
            || self.held("wnba-freestyle")
        {
            return Ok(());
        }
        if !parent_alive {
            self.set(
                "wnba-freestyle",
                "owner-closed",
                "Launcher closed; no new WNBA loader writes. Any stock pointer remains valid until game exit.",
            );
            return Ok(());
        }
        let mut access = match self.access.take() {
            Some(access) => access,
            None => match Access::open(&self.identity, &self.game, &self.parent) {
                Ok(access) => access,
                Err(error) => {
                    self.record_failure("wnba-freestyle", &error);
                    return Ok(());
                }
            },
        };
        if let Err(error) = self.freestyle_loader_tick(&mut access) {
            self.record_failure("wnba-freestyle", &error);
        }
        self.access = Some(access);
        Ok(())
    }
    fn freestyle_loader_tick(&mut self, access: &mut Access) -> Result<()> {
        crate::wnba_loader_v2::validate_profile(access)?;
        let current = access.wnba_loader_current()?;
        if self.freestyle_loader_armed {
            ensure!(
                current == crate::wnba_loader_v2::WNBA_LOADER,
                "Owned WNBA roster-loader pointer changed"
            );
            if crate::freestyle_v2::fingerprint(access, access).is_ok() {
                access.swap_wnba_loader(
                    crate::wnba_loader_v2::WNBA_LOADER,
                    crate::wnba_loader_v2::GENERAL_LOADER,
                    self.journal()?,
                )?;
                self.freestyle_loader_armed = false;
                self.freestyle_loader_deadline = None;
                self.freestyle_loader_loaded = true;
                self.freestyle_missing.clear();
                self.set(
                    "wnba-freestyle",
                    "validating-roster",
                    "Stock WNBA Freestyle roster loaded; validating its teams before category publication.",
                );
            } else if self
                .freestyle_loader_deadline
                .is_some_and(|deadline| Instant::now() >= deadline)
            {
                access.swap_wnba_loader(
                    crate::wnba_loader_v2::WNBA_LOADER,
                    crate::wnba_loader_v2::GENERAL_LOADER,
                    self.journal()?,
                )?;
                self.freestyle_loader_armed = false;
                self.freestyle_loader_deadline = None;
                self.freestyle_loader_loaded = false;
                self.freestyle_missing.clear();
                self.set(
                    "wnba-freestyle",
                    "waiting",
                    "Freestyle was not entered within 60 seconds; the stock NBA loader was restored.",
                );
            }
            return Ok(());
        }
        ensure!(
            current == crate::wnba_loader_v2::GENERAL_LOADER,
            "Freestyle roster-loader pointer is not stock"
        );
        if self.freestyle_loader_loaded {
            if crate::freestyle_v2::fingerprint(access, access).is_ok() {
                return Ok(());
            }
            if access.inactive_launch_menu()?.is_none() {
                return Ok(());
            }
            self.rearm_freestyle_roster_loader();
        }
        let Some(menu) = access.inactive_launch_menu()? else {
            self.set(
                "wnba-freestyle",
                "waiting",
                "Return to the main or Quick Play menu to prepare the stock WNBA Freestyle roster.",
            );
            return Ok(());
        };
        access.swap_wnba_loader(
            crate::wnba_loader_v2::GENERAL_LOADER,
            crate::wnba_loader_v2::WNBA_LOADER,
            self.journal()?,
        )?;
        self.freestyle_loader_armed = true;
        self.freestyle_loader_deadline = Some(Instant::now() + Duration::from_secs(60));
        self.freestyle_missing.clear();
        self.set(
            "wnba-freestyle",
            "loader-armed",
            &format!("Stock WNBA roster is prepared for the next Freestyle entry from {menu}."),
        );
        Ok(())
    }
    fn camera_selection(&self) -> Result<Vec<crate::camera_v2::Selection>> {
        self.plan
            .cameras
            .iter()
            .map(|c| {
                Ok(crate::camera_v2::Selection {
                    id: c.id,
                    steps: crate::camera_policy::Steps::new(c.height, c.zoom)?,
                })
            })
            .collect()
    }
    /// WPF parity: enable the three registered Boolean feature entries once
    /// for this exact game process. All three are captured and journaled as a
    /// single logical transaction before the first byte changes. A partial
    /// failure restores the verified prefix and is terminal for this session.
    fn debug_tick(&mut self, access: &mut Access) -> Result<()> {
        if self.debug_applied {
            let current = crate::debug_menu_v2::capture(access)?;
            ensure!(
                self.debug_snapshot.as_ref() == Some(&current)
                    && current.entries.iter().all(|entry| entry.value == 1),
                "Owned debug-menu feature values changed after publication"
            );
            self.set(
                "debug-menus",
                "active",
                "Built-in debug menus are enabled for this game session. Steam Offline Mode is required.",
            );
            return Ok(());
        }

        let first = match crate::debug_menu_v2::capture(access) {
            Ok(value) => value,
            Err(error)
                if self.debug_started.elapsed() < Duration::from_secs(120)
                    && error.chain().any(|cause| {
                        matches!(
                            cause.to_string().as_str(),
                            "Debug-menu feature manager has not initialized"
                                | "Debug-menu feature table is outside fixed bounds"
                                | "Allowlisted debug-menu feature is not registered"
                        )
                    }) =>
            {
                self.set(
                    "debug-menus",
                    "waiting",
                    &format!("Waiting for the game's debug-feature registry: {error:#}"),
                );
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        let second = crate::debug_menu_v2::capture(access)?;
        if first != second {
            self.set(
                "debug-menus",
                "waiting",
                "The game's debug-feature registry is still changing; no write was attempted.",
            );
            return Ok(());
        }
        let changes = crate::debug_menu_v2::enable_plan(&first)?;
        if changes.is_empty() {
            self.debug_applied = true;
            self.debug_snapshot = Some(first);
            self.set(
                "debug-menus",
                "active",
                "All three built-in debug-menu flags were already enabled; their exact values are being monitored.",
            );
            return Ok(());
        }
        let apply_rows = changes
            .iter()
            .map(|change| Change {
                at: change.address,
                before: vec![change.before],
                after: vec![change.after],
            })
            .collect::<Vec<_>>();
        let restore_rows = changes
            .iter()
            .rev()
            .map(|change| Change {
                at: change.address,
                before: vec![change.after],
                after: vec![change.before],
            })
            .collect::<Vec<_>>();

        access.begin_batch(Feature::Debug)?;
        let result = (|| -> Result<crate::debug_menu_v2::Snapshot> {
            let journal = self.journal()?;
            ensure!(
                journal.remaining() >= changes.len() * 2 + 3,
                "The session intent budget cannot cover debug-menu publication and restore"
            );
            let apply_intent = journal.write_intent("debug-menus", apply_rows)?;
            let restore_intent = journal.write_intent("debug-menus", restore_rows)?;
            let mut current = first.clone();
            let mut applied = Vec::new();
            for change in &changes {
                let before = current.clone();
                let mut after = before.clone();
                crate::debug_menu_v2::apply(&mut after, change)?;
                match access.write_debug(change, &before, &after, &apply_intent, journal) {
                    Ok(()) => {
                        current = after;
                        applied.push(change.clone());
                    }
                    Err(write_error) => {
                        let observed = crate::debug_menu_v2::capture(access)?;
                        if observed == after {
                            current = after;
                            applied.push(change.clone());
                        } else {
                            ensure!(
                                observed == before,
                                "Debug-menu publication failed and the exact resulting state is unknown: {write_error:#}"
                            );
                            current = before;
                        }
                        for original in applied.iter().rev() {
                            let reverse = crate::debug_menu_v2::Change {
                                definition: original.definition,
                                address: original.address,
                                before: original.after,
                                after: original.before,
                            };
                            let restore_before = current.clone();
                            let mut restore_after = restore_before.clone();
                            crate::debug_menu_v2::apply(&mut restore_after, &reverse)?;
                            access
                                .write_debug(
                                    &reverse,
                                    &restore_before,
                                    &restore_after,
                                    &restore_intent,
                                    journal,
                                )
                                .with_context(|| {
                                    format!(
                                        "Debug-menu publication failed and its verified prefix could not be restored: {write_error:#}"
                                    )
                                })?;
                            current = restore_after;
                        }
                        ensure!(
                            current == first && crate::debug_menu_v2::capture(access)? == first,
                            "Debug-menu rollback did not return to the complete original snapshot"
                        );
                        return Err(write_error.context(
                            "Debug-menu publication failed; every changed flag was restored",
                        ));
                    }
                }
            }
            journal.append(Event::Applied {
                feature: "debug-menus".into(),
            })?;
            Ok(current)
        })();
        access.end_batch();
        let active = result?;
        self.debug_applied = true;
        self.debug_snapshot = Some(active);
        self.set(
            "debug-menus",
            "active",
            "Built-in debug menus are enabled for this game session. Steam Offline Mode is required.",
        );
        Ok(())
    }
    /// WPF parity: no menu gate. The helper installed once per game process as
    /// soon as the static tables validated, as one all-or-nothing transaction.
    /// Stock-capture failures simply wait; ownership proof stays byte-exact.
    fn camera_tick(&mut self, access: &mut Access) -> Result<()> {
        crate::camera_v2::verify_profile(access)?;
        if let Some(retained) = self.camera.clone() {
            match retained.publication(access)? {
                crate::camera_v2::Publication::Active => {
                    self.camera_restored_attempts = 0;
                    let edits = retained.maintenance(access, self.plan.lock_camera)?;
                    if !edits.is_empty() {
                        apply(access, self.journal()?, Feature::Camera, changes_of(&edits))?;
                    }
                    self.set("cameras","active","Camera order/tracking is active. Native height/zoom are synchronized; visual gameplay verification is pending.");
                    return Ok(());
                }
                crate::camera_v2::Publication::Transitional => {
                    self.set(
                        "cameras",
                        "reload",
                        "The game is changing camera publication data; no write was attempted.",
                    );
                    return Ok(());
                }
                crate::camera_v2::Publication::Conflict => {
                    anyhow::bail!(
                        "Camera publication contains a foreign value; no republish attempted"
                    )
                }
                crate::camera_v2::Publication::FullyReset => {
                    // Re-plan from a fresh stock capture into the SAME retained
                    // allocation so game-owned bytes captured earlier (runtime
                    // flags, native settings blocks) are not stale guards.
                    let capture = match crate::camera_v2::capture(access) {
                        Ok(capture) => capture,
                        Err(e) => {
                            self.set("cameras", "reset", &format!("Prepared camera data is retained; waiting for stock camera data before republishing: {e:#}"));
                            return Ok(());
                        }
                    };
                    let fresh = capture.plan(&self.camera_selection()?, retained.private_base)?;
                    ensure!(
                        retained.same_private_layout(&fresh),
                        "Camera source data changed since preparation; retained allocation not republished"
                    );
                    self.publish_camera(access, &fresh)?;
                    self.camera = Some(fresh);
                    self.set("cameras","active","Camera order/tracking was republished from verified retained data; visual gameplay verification is pending.");
                    return Ok(());
                }
            }
        }
        let capture = match crate::camera_v2::capture(access) {
            Ok(capture) => capture,
            Err(e) => {
                self.set(
                    "cameras",
                    "waiting",
                    &format!("Waiting for the game's stock camera data: {e:#}"),
                );
                return Ok(());
            }
        };
        let selected = self.camera_selection()?;
        let bytes = crate::camera_v2::required_bytes(&selected)?;
        access.begin_batch(Feature::Camera)?;
        let prepared = (|| -> Result<crate::camera_v2::Plan> {
            let address = access.allocate_camera(bytes, self.journal()?)?;
            let plan = capture.plan(&selected, address)?;
            for init in &plan.initialization {
                access.initialize(init.at, &init.bytes, self.journal()?)?;
            }
            Ok(plan)
        })();
        access.end_batch();
        let plan = prepared?;
        // Retain the initialized ownership plan before publication so a
        // restored attempt reuses it instead of allocating again.
        self.camera = Some(plan.clone());
        self.publish_camera(access, &plan)?;
        self.set("cameras","active","Camera order, tracking, labels and initial height/zoom are active; live visual verification pending.");
        Ok(())
    }
    /// One fast transaction (single ownership/profile proof). Any failure
    /// restores every public edit already made, like WPF RestoreAll, then a
    /// later tick retries the same allocation. Only an unrestorable state or
    /// repeated restored attempts hold the feature.
    fn publish_camera(&mut self, access: &mut Access, plan: &crate::camera_v2::Plan) -> Result<()> {
        plan.compare_private(access)?;
        plan.compare_before(access)?;
        // A publication that fails part-way must still be able to journal its
        // restore. Refuse before the first write instead of stranding a
        // partial publication behind an exhausted budget.
        let remaining = self.journal()?.remaining();
        ensure!(
            remaining >= camera_publication_events(plan.edits.len()),
            "The session's write-intent budget can no longer cover a camera publication and its full restore; nothing written"
        );
        access.begin_batch(Feature::Camera)?;
        let journal = match self.journal() {
            Ok(journal) => journal,
            Err(error) => {
                access.end_batch();
                return Err(error);
            }
        };
        let result = publish_or_restore(access, journal, plan);
        access.end_batch();
        match result {
            Ok(()) => {
                self.camera_restored_attempts = 0;
                Ok(())
            }
            Err(error) if error.is::<CameraPublicationDeferred>() => {
                self.camera_restored_attempts = self.camera_restored_attempts.saturating_add(1);
                if self.camera_restored_attempts >= CAMERA_RESTORED_LIMIT {
                    return Err(CameraRestoredLimit(format!("{error:#}")).into());
                }
                Err(error)
            }
            Err(error) => Err(error),
        }
    }
    fn scaler_fact(
        &mut self,
        phase: ScalerMoment,
        mode: Option<u32>,
        loaded: Option<usize>,
        note: Option<&str>,
    ) {
        let elapsed = self
            .scaler_clock
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;
        self.scaler_timing
            .record(phase, elapsed, mode, loaded, note);
    }
    fn scaler_tick(&mut self, access: &mut Access, percent: u8) -> Result<()> {
        if !self.scaler_applied {
            self.scaler_timing.attempts = self.scaler_timing.attempts.saturating_add(1);
            self.scaler_fact(ScalerMoment::Attempt, None, None, None);
        }
        if let Err(error) = crate::scaler_v2::verify_profile(access) {
            self.scaler_fact(
                ScalerMoment::ReadinessRefused,
                None,
                None,
                Some(scaler_refusal_note(&error)),
            );
            return Err(error);
        }
        if self.scaler_applied {
            let current = u32::from_le_bytes(
                access
                    .read(access.base() + crate::scaler_v2::INPUT, 4)?
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Partial shared player-size input"))?,
            );
            match scaler_publication(current, crate::scaler_policy::target_bits(percent)?) {
                ScalerPublication::Active => {}
                ScalerPublication::StockReset => {
                    self.scaler_fact(ScalerMoment::ResetObserved, None, None, None);
                    self.scaler_applied = false;
                    self.scaler_reset = true;
                    self.scaler_stable_menu = None;
                    self.scaler_menu_samples = 0;
                    self.set(
                        "scaler",
                        "reset",
                        "The game restored stock player size. Return to Main or Quick Play to rearm the requested scale safely.",
                    );
                    return Ok(());
                }
                ScalerPublication::Conflict => anyhow::bail!(
                    "The shared player-size input contains a foreign value; it was not overwritten"
                ),
            }
            let groups = self.scaler_groups_tick(access);
            match crate::scaler_v2::observe(access, percent) {
                Ok(value) if value.players > 0 => {
                    self.scaler_fact(ScalerMoment::PlayerCacheObserved, None, Some(value.players), None);
                    self.set("scaler", "active", &format!(
                        "Shared input {percent}% verified; {} loaded player size caches verified (native range {:.4} to {:.4}). {groups} Visual size still needs gameplay confirmation.",
                        value.players, value.minimum_cache.unwrap(), value.maximum_cache.unwrap()));
                }
                Ok(_) => self.set("scaler", "active", &format!("Shared input {percent}% verified; waiting for loaded players to verify native size caches. {groups}")),
                Err(error) => self.set("scaler", "unverified", &format!("Shared input {percent}% is applied, but loaded-player verification is pending: {error:#}. {groups}")),
            }
            return Ok(());
        }
        if self.scaler_reset {
            let Some(menu) = access.camera_ready_boundary()? else {
                self.scaler_stable_menu = None;
                self.scaler_menu_samples = 0;
                self.set(
                    "scaler",
                    "reset",
                    "Stock player size remains active; return to Main or Quick Play to rearm the requested scale.",
                );
                return Ok(());
            };
            if self.scaler_stable_menu.as_ref() == Some(&menu) {
                self.scaler_menu_samples = self.scaler_menu_samples.saturating_add(1);
            } else {
                self.scaler_stable_menu = Some(menu);
                self.scaler_menu_samples = 1;
            }
            if self.scaler_menu_samples < 2 {
                self.set(
                    "scaler",
                    "reset",
                    "Stock player size detected; confirming a stable unloaded menu before rearming.",
                );
                return Ok(());
            }
        }
        let plan = match crate::scaler_v2::plan_shared_input(access, percent, None) {
            Ok(Some(plan)) => plan,
            Ok(None) => {
                self.set(
                    "scaler",
                    "unchanged",
                    "Shared player-size input already matches the requested stock baseline.",
                );
                return Ok(());
            }
            Err(e) => {
                // Diagnostic only, never inspected by permission/retry policy.
                self.scaler_fact(
                    ScalerMoment::ReadinessRefused,
                    None,
                    None,
                    Some(scaler_refusal_note(&e)),
                );
                self.set(
                    "scaler",
                    "waiting",
                    &format!("Waiting for supported native player-size data: {e:#}"),
                );
                return Ok(());
            }
        };
        // Reuse the proven Readiness summary; telemetry performs no reads.
        self.scaler_fact(
            ScalerMoment::PlanReady,
            Some(plan.readiness.mode()),
            Some(plan.readiness.loaded_players()),
            None,
        );
        if let Err(error) = crate::scaler_v2::revalidate(access, &plan.readiness) {
            self.scaler_fact(
                ScalerMoment::ReadinessRefused,
                None,
                None,
                Some(scaler_refusal_note(&error)),
            );
            return Err(error);
        }
        if self.scaler_reset {
            ensure!(
                !plan.readiness.loaded(),
                "Player models loaded before scaler rearm; no write attempted"
            );
        }
        let reload = plan.reload_required;
        apply_guarded(
            access,
            self.journal()?,
            Feature::Scaler,
            vec![Change {
                at: plan.address,
                before: plan.before.to_vec(),
                after: plan.after.to_vec(),
            }],
            |a, _| crate::scaler_v2::revalidate(a, &plan.readiness),
        )?;
        self.scaler_fact(
            ScalerMoment::GlobalApplied,
            Some(plan.readiness.mode()),
            Some(plan.readiness.loaded_players()),
            None,
        );
        self.scaler_applied = true;
        self.scaler_reset = false;
        self.scaler_stable_menu = None;
        self.scaler_menu_samples = 0;
        self.set("scaler","active",if reload {"Shared input verified. Players were already loaded: reload the court to evaluate the requested scale. No per-frame player writes."}
            else {"Shared input verified before players loaded. Selected character groups join it as they load. Live visual verification pending."});
        Ok(())
    }
    /// WPF parity for coaches, cheerleaders and arena personnel. Never holds
    /// the player scaler: list/loading changes wait, and only ownership
    /// conflicts or failed writes hold this part for the rest of the session.
    fn scaler_groups_tick(&mut self, access: &mut Access) -> String {
        let groups = self.plan.scaler_groups;
        if !groups.any() {
            return "Coaches, cheerleaders and arena personnel stay at original size (not selected).".into();
        }
        if let Some(reason) = &self.npc_held {
            return format!("Character groups held (no further writes): {reason}");
        }
        match self.scaler_groups_apply(access, groups) {
            Ok(note) => note,
            Err(error) => {
                let reason: String = format!("{error:#}").chars().take(400).collect();
                if let Some(journal) = &mut self.journal {
                    let _ = journal.append(Event::Held {
                        feature: "scaler".into(),
                        message: format!("Character groups: {reason}"),
                    });
                }
                self.npc_held = Some(reason.clone());
                format!("Character groups held (no further writes): {reason}")
            }
        }
    }
    fn scaler_groups_apply(
        &mut self,
        access: &mut Access,
        groups: crate::scaler_npc_v2::Groups,
    ) -> Result<String> {
        if !self.npc_profile {
            crate::scaler_npc_v2::verify_profile(access)?;
            self.npc_profile = true;
        }
        let mut found = match crate::scaler_npc_v2::scan(access) {
            Ok(found) => found,
            Err(error) => return Ok(format!("Character groups waiting: {error:#}.")),
        };
        // A character whose data kept moving under its write is left alone for
        // this session, so it cannot spend the journal's write budget.
        found.retain(|c| self.npc_moved.get(&c.node).copied().unwrap_or(0) < NPC_MOVED_LIMIT);
        let input = u32::from_le_bytes(
            access
                .read(access.base() + crate::scaler_v2::INPUT, 4)?
                .try_into()
                .map_err(|_| anyhow::anyhow!("Partial shared player-size input"))?,
        );
        let plan = crate::scaler_npc_v2::plan(&found, groups, input, &mut self.npc_owned);
        ensure!(
            plan.conflicts == 0,
            "{} owned character size flags changed outside the launcher; left untouched",
            plan.conflicts
        );
        for (number, chunk) in plan.writes.chunks(64).enumerate() {
            let remaining = self.journal()?.remaining();
            ensure!(
                remaining >= transaction_events(chunk.len()) + CAMERA_RESTORE_RESERVE,
                "Character groups stopped to keep the session's write-intent budget for camera restores"
            );
            let changes = chunk
                .iter()
                .map(|w| Change {
                    at: w.at,
                    before: w.before.clone(),
                    after: w.after.clone(),
                })
                .collect::<Vec<_>>();
            // Capture only existing local receipt classes. Unknown classes
            // still increment total/unclassified; diagnostics never gate work.
            let effect_groups = chunk
                .iter()
                .map(|w| self.npc_owned.get(&w.node).map(|o| o.group))
                .collect::<Vec<_>>();
            let effect_clock = self.scaler_clock;
            let mut effect_counts = self.scaler_npc_effects.clone();
            access.begin_batch(Feature::Scaler)?;
            let journal = match self.journal() {
                Ok(journal) => journal,
                Err(error) => {
                    access.end_batch();
                    return Err(error);
                }
            };
            let result = (|| -> Result<Option<usize>> {
                let intent = journal.write_intent("scaler", changes)?;
                for (index, w) in chunk.iter().enumerate() {
                    let (node, actor) = (w.node, w.actor);
                    let still = crate::scaler_npc_v2::identity(&*access, node);
                    // A character unloading between scan and write is a
                    // normal transition: stop before touching it.
                    if !matches!(&still, Ok(Some(c)) if c.actor == actor) {
                        return Ok(Some(index));
                    }
                    let elapsed_ms =
                        effect_clock.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
                    effect_counts.enter(effect_groups[index], elapsed_ms);
                    let write_result =
                        access.write_object(w.at, &w.before, &w.after, &intent, journal, |a| {
                            let c = crate::scaler_npc_v2::identity(a, node)?
                                .context("Character unloaded before write")?;
                            ensure!(c.actor == actor, "Character changed before write");
                            Ok(())
                        });
                    effect_counts.returned(
                        effect_groups[index],
                        w.after.len(),
                        write_result.is_ok(),
                    );
                    write_result?;
                }
                journal.append(Event::Applied {
                    feature: "scaler".into(),
                })?;
                Ok(None)
            })();
            access.end_batch();
            // Keep entries and uncertain returns even if this chunk failed.
            // An Applied journal event remains the transaction authority.
            self.scaler_npc_effects = effect_counts;
            let result = match result {
                Err(error)
                    if error.is::<crate::blacktop_native::feature_writer::CharacterMoved>() =>
                {
                    // Find the write that refused: the first one still at its
                    // before-bytes. Everything before it applied and is owned.
                    let stopped = chunk
                        .iter()
                        .position(|w| {
                            access.read(w.at, w.before.len()).ok().as_deref() != Some(&w.after[..])
                        })
                        .unwrap_or(0);
                    let node = chunk[stopped].node;
                    *self.npc_moved.entry(node).or_insert(0) += 1;
                    Ok(Some(stopped))
                }
                other => other,
            };
            let completed = result?;
            if completed.is_none() && !chunk.is_empty() {
                self.scaler_fact(ScalerMoment::NpcWriteVerified, None, None, None);
                if chunk.iter().any(|write| write.after.len() == 8) {
                    self.scaler_fact(ScalerMoment::NpcCacheVerified, None, None, None);
                }
            }
            if let Some(stopped) = completed {
                // Every unwritten write, including later chunks, is re-planned
                // on the next tick (never written blindly).
                forget_unfinished(&mut self.npc_owned, &plan.writes[number * 64 + stopped..]);
                return Ok("Character groups waiting: characters are loading.".into());
            }
        }
        let counts = crate::scaler_npc_v2::summary(&self.npc_owned);
        let n = |g| counts.get(&g).copied().unwrap_or(0);
        use crate::scaler_npc_v2::Group;
        Ok(format!(
            "Shared size also active for {} coaches, {} cheerleaders, {} arena personnel and {} courtside celebrities.",
            n(Group::Coach),
            n(Group::Cheerleader),
            n(Group::Arena),
            n(Group::Celebrity)
        ))
    }
    fn wait_for_freestyle_capture(&mut self, error: &anyhow::Error, missing: bool) {
        // Automatic source publication is asynchronous. A token can precede
        // its REGISTERED receipt; this is a retryable readiness state, never a
        // held writer or permission to bypass the next full source proof.
        self.set(
            "wnba-freestyle",
            if missing { "unavailable" } else { "waiting" },
            &if missing {
                format!("{error:#}")
            } else {
                format!("Waiting for Freestyle team selection and its mixed roster: {error:#}")
            },
        );
    }
    fn freestyle_tick(&mut self, access: &mut Access) -> Result<()> {
        crate::freestyle_v2::validate_profile(access)?;
        let tables = crate::freestyle_v2::Tables::read(access)?;
        if self.freestyle_applied {
            return crate::freestyle_v2::observe(tables);
        }
        // Mixed route: the published roster must pass the strict (mixed-only)
        // deep check; the loader route accepts its proven WNBA-only load.
        let priority = self.plan.wnba_priority_categories;
        let mixed = priority || self.plan.wnba_mixed.is_some();
        if !priority && !self.freestyle_loader_loaded && !(mixed && self.mixed.published) {
            return Ok(());
        }
        let proof_root = self.root.clone();
        let proof_session = self.session.clone();
        let proof_identity = self.identity.clone();
        let capture = |a: &Access| -> Result<crate::freestyle_v2::Snapshot> {
            if priority {
                crate::wnba_priority_categories::verify(
                    &proof_root,
                    &proof_session,
                    &proof_identity,
                    a,
                )?;
            }
            let snapshot = if mixed {
                crate::freestyle_v2::capture(a, a)?
            } else {
                crate::freestyle_v2::capture_after_wnba_loader(a, a)?
            };
            if priority {
                crate::wnba_priority_categories::verify(
                    &proof_root,
                    &proof_session,
                    &proof_identity,
                    a,
                )?;
            }
            Ok(snapshot)
        };
        match crate::freestyle_v2::fingerprint(access, access) {
            Ok(current) if self.freestyle_missing.same(&current) => {
                self.set(
                    "wnba-freestyle",
                    "unavailable",
                    if mixed {
                        "This Freestyle roster was loaded before the mixed roster was ready (or is a custom roster). Return to the main menu and enter Freestyle again."
                    } else {
                        "The unchanged stock roster is still missing required WNBA teams; waiting for a new Freestyle roster generation."
                    },
                );
                return Ok(());
            }
            Ok(_) => {}
            Err(error) => {
                self.freestyle_missing.clear();
                self.set(
                    "wnba-freestyle",
                    "waiting",
                    &format!("Waiting for initialized Freestyle selection: {error:#}"),
                );
                return Ok(());
            }
        }
        let first = match capture(access) {
            Ok(snapshot) => snapshot,
            Err(e) => {
                let missing = e
                    .downcast_ref::<crate::freestyle_v2::MissingRosterTeams>()
                    .is_some();
                if missing {
                    if let Ok(current) = crate::freestyle_v2::fingerprint(access, access) {
                        self.freestyle_missing.remember(current);
                    }
                } else {
                    self.freestyle_missing.clear();
                }
                self.wait_for_freestyle_capture(&e, missing);
                return Ok(());
            }
        };
        let second = match capture(access) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.set(
                    "wnba-freestyle",
                    "waiting",
                    &format!("Freestyle selection is changing; nothing written: {error:#}"),
                );
                return Ok(());
            }
        };
        let plan = match crate::freestyle_v2::plan(tables, &first, &second) {
            Ok(plan) => plan,
            Err(error) if error.is::<crate::freestyle_v2::ReadinessChanged>() => {
                self.set(
                    "wnba-freestyle",
                    "waiting",
                    &format!("Freestyle selection is changing; nothing attempted: {error:#}"),
                );
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        self.freestyle_missing.clear();
        let changes = plan
            .changes
            .into_iter()
            .map(|c| Change {
                at: access.base() + c.rva,
                before: c.before.to_le_bytes().to_vec(),
                after: c.after.to_le_bytes().to_vec(),
            })
            .collect();
        let publication =
            publish_freestyle_or_restore(access, self.journal()?, changes, |a, index| {
                let expected = freestyle_progress(&plan, index)?;
                ensure!(
                    crate::freestyle_v2::Tables::read(a)? == expected,
                    "Freestyle category ownership changed before write"
                );
                let full_guard = freestyle_full_guard(index, plan.changes.len());
                if priority && full_guard {
                    crate::wnba_priority_categories::verify(
                        &proof_root,
                        &proof_session,
                        &proof_identity,
                        a,
                    )?;
                }
                if full_guard {
                    crate::freestyle_v2::verify_compact(a, a, &first)?;
                }
                if index == plan.changes.len() {
                    // WPF temporal shape: deep pair before plan, deep post after
                    // four adjacent guarded DWORDs. In-place row changes can be
                    // detected later than per-word deep scans; exact rollback is
                    // mandatory and this tradeoff is explicit (QA078 report).
                    let fresh = capture(a)?;
                    crate::freestyle_v2::verify_deep_equal(&first, &fresh)?;
                    crate::freestyle_v2::verify_compact(a, a, &first)?;
                }
                ensure!(
                    crate::freestyle_v2::Tables::read(a)? == expected,
                    "Freestyle navigation changed during readiness check"
                );
                if priority && full_guard {
                    // Last source/token check before this scalar's durable
                    // publication boundary; capture() also brackets the full
                    // roster. Middle DWORDs stay in the tight owned prefix.
                    crate::wnba_priority_categories::verify(
                        &proof_root,
                        &proof_session,
                        &proof_identity,
                        a,
                    )?;
                }
                Ok(())
            });
        if let Err(error) = publication {
            if error.is::<FreestylePublicationRestored>()
                || error.is::<FreestylePreEffectDeferred>()
            {
                crate::freestyle_v2::validate_profile(access)?;
                ensure!(
                    crate::freestyle_v2::Tables::read(access)? == plan.before,
                    "Freestyle full-table restoration differs; no retry allowed"
                );
                if priority {
                    crate::wnba_priority_categories::verify(
                        &proof_root,
                        &proof_session,
                        &proof_identity,
                        access,
                    )?;
                }
            }
            return Err(error);
        }
        crate::freestyle_v2::observe(crate::freestyle_v2::Tables::read(access)?)?;
        self.freestyle_applied = true;
        self.set("wnba-freestyle","active",&format!("Freestyle category navigation includes the verified {:?} stock roster for this game session. Live team selection remains to be confirmed.", first.profile));
        Ok(())
    }
    pub fn ended(&mut self, closed: impl Fn() -> Result<()>) -> Result<()> {
        closed()?;
        if let Some(journal) = &mut self.journal {
            journal.finish(&closed)?;
        }
        self.journal = None;
        self.access = None;
        for name in ["cameras", "scaler", "wnba-freestyle", "debug-menus"] {
            if self.states[name].phase != "disabled" {
                self.set(
                    name,
                    "ended",
                    "Game ended; process-owned memory changes and allocations are gone.",
                );
            }
        }
        Ok(())
    }
}
trait DataAccess: Memory {
    fn write_intended(
        &mut self,
        feature: Feature,
        change: &Change,
        intent: &crate::feature_journal::Intent,
        journal: &mut Journal,
    ) -> Result<()>;
}
impl DataAccess for Access {
    fn write_intended(
        &mut self,
        feature: Feature,
        change: &Change,
        intent: &crate::feature_journal::Intent,
        journal: &mut Journal,
    ) -> Result<()> {
        self.write(
            feature,
            change.at,
            &change.before,
            &change.after,
            intent,
            journal,
        )
    }
}
fn apply(
    access: &mut impl DataAccess,
    journal: &mut Journal,
    feature: Feature,
    changes: Vec<Change>,
) -> Result<()> {
    apply_guarded(access, journal, feature, changes, |_, _| Ok(()))
}
fn apply_guarded<A: DataAccess>(
    access: &mut A,
    journal: &mut Journal,
    feature: Feature,
    changes: Vec<Change>,
    mut check: impl FnMut(&A, usize) -> Result<()>,
) -> Result<()> {
    if changes.is_empty() {
        return Ok(());
    }
    ensure!(
        changes.len() <= MAX_TRANSACTION,
        "Feature transaction exceeds write-count bounds"
    );
    ensure!(
        journal.remaining() >= transaction_events(changes.len()),
        "Feature intent budget cannot journal this whole transaction; nothing written"
    );
    check(access, 0)?;
    for change in &changes {
        ensure!(
            access.read(change.at, change.before.len())? == change.before,
            "Feature changed before durable write intent"
        );
    }
    let intent = journal.write_intent(feature.name(), changes.clone())?;
    // Intent flushes and protection operations are not assumed instantaneous.
    // Revalidate context and ALL category words, including untouched entries.
    for (index, change) in changes.iter().enumerate() {
        check(access, index)?;
        access.write_intended(feature, change, &intent, journal)?;
    }
    check(access, changes.len())?;
    journal.append(Event::Applied {
        feature: feature.name().into(),
    })
}
/// Receipts for writes that never happened. An unwritten opt-in flag forgets
/// the character, so it is re-proved from scratch. An unwritten cache seed
/// whose flag already applied keeps the receipt and re-arms only the seed;
/// forgetting it would leave our own opt-in bit unowned and the cache stale.
fn forget_unfinished(
    owned: &mut BTreeMap<u64, crate::scaler_npc_v2::Owned>,
    unwritten: &[crate::scaler_npc_v2::Write],
) {
    for w in unwritten {
        if w.after.len() == 1 {
            owned.remove(&w.node);
        } else if let Some(receipt) = owned.get_mut(&w.node) {
            receipt.seeded = false;
        }
    }
}
fn freestyle_progress(
    plan: &crate::freestyle_v2::ApplyPlan,
    count: usize,
) -> Result<crate::freestyle_v2::Tables> {
    plan.validate()?;
    ensure!(count <= 4, "Invalid Freestyle publication progress");
    let mut result = plan.before;
    for change in plan.changes.iter().take(count) {
        let (table, start) = if change.rva >= crate::freestyle_v2::NEXT {
            (1, crate::freestyle_v2::NEXT)
        } else {
            (0, crate::freestyle_v2::PREVIOUS)
        };
        let index = usize::try_from((change.rva - start) / 4)?;
        ensure!(index < 6, "Freestyle category entry overflow");
        result.0[table][index] = change.after;
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_a_typed_restored_publication_waits_and_real_errors_remain_held() {
        let mut runner = fixture(Plan::default());
        runner.record_camera_failure(&CameraPublicationDeferred("write failed".into()).into());
        assert_eq!(runner.status()["features"]["cameras"]["phase"], "waiting");
        assert!(runner.journal.is_none());
        // An identical string is not a typed, verified restore.
        runner.record_camera_failure(&anyhow::anyhow!(
            CameraPublicationDeferred("write failed".into()).to_string()
        ));
        assert!(runner.held("cameras"));
    }
    fn camera_plan() -> Plan {
        Plan {
            cameras: vec![crate::feature_plan::Camera {
                id: crate::camera_policy::CURATED[0].0,
                name: crate::camera_policy::CURATED[0].1.into(),
                height: 0,
                zoom: 0,
            }],
            ..Plan::default()
        }
    }
    #[test]
    fn camera_retry_rearms_only_restorable_holds_and_stop() {
        let mut runner = fixture(camera_plan());
        // Nothing to retry while waiting; nothing to stop when not requested.
        assert!(runner.camera_command(CameraCommand::Retry).is_err());
        assert!(
            fixture(Plan::default())
                .camera_command(CameraCommand::Stop)
                .is_err()
        );
        // Every attempt fully restored: held, but Retry may re-arm.
        runner.record_camera_failure(&CameraRestoredLimit("write failed".into()).into());
        let status = runner.status();
        assert_eq!(status["features"]["cameras"]["phase"], "held");
        assert_eq!(status["features"]["cameras"]["retryable"], true);
        assert!(runner.camera_command(CameraCommand::Stop).is_err());
        runner.camera_restored_attempts = CAMERA_RESTORED_LIMIT;
        runner.camera_command(CameraCommand::Retry).unwrap();
        let status = runner.status();
        assert_eq!(status["features"]["cameras"]["phase"], "waiting");
        assert!(status["features"]["cameras"].get("retryable").is_none());
        assert_eq!(runner.camera_restored_attempts, 0);
        // A foreign value or failed restore keeps its no-further-writes hold.
        for error in [
            anyhow::anyhow!("Camera publication contains a foreign value; no republish attempted"),
            anyhow::anyhow!("Camera publication failed: x; restore also failed: y"),
            anyhow::anyhow!(CameraRestoredLimit("text only".into()).to_string()),
        ] {
            let mut runner = fixture(camera_plan());
            runner.record_camera_failure(&error);
            assert!(runner.held("cameras"));
            assert!(
                runner.status()["features"]["cameras"]
                    .get("retryable")
                    .is_none()
            );
            assert!(runner.camera_command(CameraCommand::Retry).is_err());
        }
    }
    #[test]
    fn camera_stop_is_queued_for_the_guarded_tick_and_stopped_can_retry() {
        let mut runner = fixture(camera_plan());
        runner.camera_command(CameraCommand::Stop).unwrap();
        assert!(runner.camera_stop_pending);
        // Without a prepared plan there is nothing to restore; no access or
        // receipt is needed to mark the stop.
        runner.camera = None;
        runner.camera_stop_pending = false;
        runner.set("cameras", "stopped", "stopped");
        assert!(runner.idle("cameras"));
        assert!(runner.camera_command(CameraCommand::Stop).is_err());
        // A stopped feature is not reset to waiting by access retries.
        runner.record_access_open_failure(&anyhow::anyhow!("not yet"));
        assert_eq!(runner.status()["features"]["cameras"]["phase"], "stopped");
        runner.camera_command(CameraCommand::Retry).unwrap();
        assert_eq!(runner.status()["features"]["cameras"]["phase"], "waiting");
        assert!(!runner.camera_stop_pending);
    }
    #[test]
    fn unproven_game_gate_waits_then_holds_each_feature_without_ending_the_session() {
        let mut runner = fixture(Plan {
            scaler_percent: Some(97),
            ..camera_plan()
        });
        let denied = anyhow::anyhow!("Access is denied. (0x80070005)");
        for _ in 1..ACCESS_OPEN_FAILURE_LIMIT {
            runner.gate_unavailable(&denied);
        }
        let status = runner.status();
        for name in ["cameras", "scaler"] {
            assert_eq!(status["features"][name]["phase"], "waiting");
            assert!(
                status["features"][name]["message"]
                    .as_str()
                    .unwrap()
                    .contains("1 retries left")
            );
        }
        // Disabled features are never revived by gate retries.
        assert_eq!(status["features"]["wnba-freestyle"]["phase"], "disabled");
        // A proven gate (owner closed here, so no OS access) resets the window.
        runner.tick(false).unwrap();
        assert_eq!(runner.gate_retry.failures, 0);
        runner.set("cameras", "waiting", "re-armed");
        runner.set("scaler", "waiting", "re-armed");
        for _ in 0..ACCESS_OPEN_FAILURE_LIMIT {
            runner.gate_unavailable(&denied);
        }
        let status = runner.status();
        assert_eq!(status["features"]["cameras"]["phase"], "held");
        assert_eq!(status["features"]["scaler"]["phase"], "held");
        assert!(status["features"]["cameras"].get("retryable").is_none());
        assert!(runner.access.is_none() && runner.journal.is_none());
    }
    #[test]
    fn unattributed_tick_error_holds_active_features_only() {
        let mut runner = fixture(Plan {
            scaler_percent: Some(97),
            ..camera_plan()
        });
        runner.set("cameras", "stopped", "stopped by user");
        runner.tick_failed(&anyhow::anyhow!("runner defect"));
        let status = runner.status();
        assert_eq!(status["features"]["cameras"]["phase"], "stopped");
        assert_eq!(status["features"]["scaler"]["phase"], "held");
        assert_eq!(status["features"]["wnba-freestyle"]["phase"], "disabled");
    }
    #[test]
    fn camera_access_that_never_opened_is_retryable_but_scaler_is_not() {
        let mut runner = fixture(Plan {
            scaler_percent: Some(97),
            ..camera_plan()
        });
        for _ in 0..ACCESS_OPEN_FAILURE_LIMIT {
            runner.record_access_open_failure(&anyhow::anyhow!("access denied"));
        }
        let status = runner.status();
        assert_eq!(status["features"]["cameras"]["retryable"], true);
        assert_eq!(status["features"]["scaler"]["phase"], "held");
        assert!(status["features"]["scaler"].get("retryable").is_none());
        runner.camera_command(CameraCommand::Retry).unwrap();
        assert_eq!(runner.access_open_retry.failures, 0);
        assert!(runner.held("scaler"));
    }
    #[test]
    fn initial_access_open_gets_bounded_retries_then_holds_and_can_reset() {
        let mut retry = AccessOpenRetry::default();
        for attempt in 1..ACCESS_OPEN_FAILURE_LIMIT {
            assert_eq!(
                retry.failed(),
                AccessOpenDisposition::Waiting {
                    attempt,
                    retries_left: ACCESS_OPEN_FAILURE_LIMIT - attempt
                }
            );
        }
        assert_eq!(retry.failed(), AccessOpenDisposition::Held);
        assert_eq!(retry.failed(), AccessOpenDisposition::Held);
        retry.succeeded();
        assert_eq!(
            retry.failed(),
            AccessOpenDisposition::Waiting {
                attempt: 1,
                retries_left: ACCESS_OPEN_FAILURE_LIMIT - 1
            }
        );
    }
    #[test]
    fn access_open_failures_wait_without_receipts_then_surface_a_terminal_hold() {
        let mut runner = fixture(Plan {
            scaler_percent: Some(95),
            ..Plan::default()
        });
        let error = anyhow::anyhow!("fixture process is still starting");
        for attempt in 1..ACCESS_OPEN_FAILURE_LIMIT {
            runner.record_access_open_failure(&error);
            assert_eq!(runner.status()["features"]["scaler"]["phase"], "waiting");
            assert!(
                runner.status()["features"]["scaler"]["message"]
                    .as_str()
                    .unwrap()
                    .contains(&format!("attempt {attempt}"))
            );
            assert!(runner.access.is_none() && runner.journal.is_none());
        }
        runner.record_access_open_failure(&error);
        assert_eq!(runner.status()["features"]["scaler"]["phase"], "held");
        assert!(
            runner.status()["features"]["scaler"]["message"]
                .as_str()
                .unwrap()
                .contains("No further writes this session")
        );
        assert!(runner.access.is_none() && runner.journal.is_none());
    }
    #[test]
    fn scaler_timing_diagnostics_never_retain_unknown_transport_addresses_or_paths() {
        let error =
            anyhow::anyhow!("ReadProcessMemory failed at 0x123456789 in C:\\private\\game.exe");
        assert_eq!(
            scaler_refusal_note(&error),
            "Unclassified readiness refusal"
        );
        let error = anyhow::anyhow!("Player has not initialized its native shared-size opt-in");
        assert_eq!(
            scaler_refusal_note(&error),
            "Player has not initialized its native shared-size opt-in"
        );
    }

    #[test]
    fn npc_writer_diagnostics_keep_uncertain_entries_and_survive_session_end() {
        use crate::scaler_npc_v2::Group;
        let mut runner = fixture(Plan {
            scaler_percent: Some(97),
            ..Plan::default()
        });
        let audit = &mut runner.scaler_npc_effects;
        audit.enter(Some(Group::Arena), 10);
        audit.returned(Some(Group::Arena), 8, true);
        // Entering a guarded writer does not prove it wrote. A refused call
        // remains uncertain rather than being called a verified flag effect.
        audit.enter(Some(Group::Arena), 20);
        audit.returned(Some(Group::Arena), 1, false);
        audit.enter(None, 30);
        audit.returned(None, 1, true);
        let before = runner.status()["scalerTiming"].clone();
        let counts = &before["npcEffects"];
        assert_eq!(counts["total"]["entered"], 3);
        assert_eq!(counts["total"]["verifiedFlagReturns"], 1);
        assert_eq!(counts["total"]["verifiedCacheReturns"], 1);
        assert_eq!(counts["total"]["uncertainReturns"], 1);
        assert_eq!(counts["total"]["firstEnteredMs"], 10);
        assert_eq!(counts["total"]["lastEnteredMs"], 30);
        assert_eq!(counts["arena"]["entered"], 2);
        assert_eq!(counts["unclassified"]["entered"], 1);
        for name in ["coach", "cheerleader", "celebrity"] {
            assert_eq!(counts[name]["entered"], 0);
        }
        for _ in 0..100 {
            assert_eq!(runner.status()["scalerTiming"], before);
        }
        runner.ended(|| Ok(())).unwrap();
        assert_eq!(runner.status()["scalerTiming"], before);
        assert!(!runner.held("scaler"));
        assert!(runner.access.is_none() && runner.journal.is_none());
    }

    #[test]
    fn scaler_timing_retains_first_refusal_and_never_grows_or_authorizes() {
        let mut timing = ScalerTiming::default();
        timing.record(ScalerMoment::Attempt, 0, None, None, None);
        timing.record(
            ScalerMoment::ReadinessRefused,
            1,
            None,
            None,
            Some("Player has not initialized its native shared-size opt-in"),
        );
        for n in 2..2000 {
            timing.record(
                ScalerMoment::ReadinessRefused,
                n,
                None,
                None,
                Some("another reason"),
            );
        }
        assert_eq!(timing.facts.len(), 2);
        assert_eq!(timing.facts[1].elapsed_ms, 1);
        assert_eq!(
            timing.facts[1].note.as_deref(),
            Some("Player has not initialized its native shared-size opt-in")
        );
        let long = "x".repeat(1000);
        timing.record(ScalerMoment::PlanReady, 3, Some(0), Some(0), Some(&long));
        assert_eq!(timing.facts[2].note.as_ref().unwrap().len(), 256);
        let mut runner = fixture(Plan {
            scaler_percent: Some(97),
            ..Plan::default()
        });
        runner.scaler_timing = timing;
        let before = runner.status()["scalerTiming"].clone();
        runner.scaler_timing.attempts = runner.scaler_timing.attempts.saturating_add(100);
        assert_eq!(
            runner.status()["scalerTiming"],
            before,
            "private counter must not cause recurring status-file writes"
        );
        runner.ended(|| Ok(())).unwrap();
        assert_eq!(
            runner.status()["scalerTiming"],
            before,
            "closed provider snapshot must retain first refusal"
        );
        assert!(
            !runner.held("scaler"),
            "timing does not grant/deny permission or hold features"
        );
        assert!(before["informationalOnly"].as_bool().unwrap());
    }

    #[test]
    fn initial_scaler_effect_precedes_camera_once_and_cleanup_keeps_priority() {
        for (initial, cleanup, expected) in [
            (true, false, [FeatureLane::Scaler, FeatureLane::Camera]),
            (true, true, [FeatureLane::Camera, FeatureLane::Scaler]),
            (false, false, [FeatureLane::Camera, FeatureLane::Scaler]),
            (false, true, [FeatureLane::Camera, FeatureLane::Scaler]),
        ] {
            for failed in [None, Some(FeatureLane::Camera), Some(FeatureLane::Scaler)] {
                let mut runner = fixture(Plan::default());
                let mut effects = Vec::new();
                for lane in camera_scaler_order(initial, cleanup) {
                    effects.push(lane); // Record effect entry, including failed attempts.
                    if failed == Some(lane) {
                        let name = if lane == FeatureLane::Camera {
                            "cameras"
                        } else {
                            "scaler"
                        };
                        runner.record_failure(name, &anyhow::anyhow!("injected lane failure"));
                    }
                }
                assert_eq!(effects.as_slice(), &expected);
                assert_eq!(
                    effects
                        .iter()
                        .filter(|v| **v == FeatureLane::Scaler)
                        .count(),
                    1
                );
                assert_eq!(
                    effects
                        .iter()
                        .filter(|v| **v == FeatureLane::Camera)
                        .count(),
                    1
                );
                if let Some(lane) = failed {
                    let name = if lane == FeatureLane::Camera {
                        "cameras"
                    } else {
                        "scaler"
                    };
                    let other = if lane == FeatureLane::Camera {
                        "scaler"
                    } else {
                        "cameras"
                    };
                    assert!(runner.held(name));
                    assert!(
                        !runner.held(other),
                        "one lane's failure must not prevent the other lane"
                    );
                }
            }
        }
    }

    #[test]
    fn scaler_publication_distinguishes_owned_stock_reset_and_foreign_values() {
        let target = crate::scaler_policy::target_bits(95).unwrap();
        assert_eq!(
            scaler_publication(target, target),
            ScalerPublication::Active
        );
        assert_eq!(
            scaler_publication(crate::scaler_policy::ORIGINAL, target),
            ScalerPublication::StockReset
        );
        assert_eq!(
            scaler_publication(crate::scaler_policy::target_bits(96).unwrap(), target),
            ScalerPublication::Conflict
        );
        assert_eq!(scaler_publication(0, target), ScalerPublication::Conflict);
    }
    fn fixture(plan: Plan) -> Runner {
        Runner::new(
            Path::new(r"C:\Fixture\Portable"),
            Path::new(r"C:\Fixture"),
            &crate::backend::unique_id(),
            &Identity {
                pid: 123,
                started: "456".into(),
                path: r"C:\Fixture\NBA2K27.exe".into(),
            },
            &Identity {
                pid: 456,
                started: "789".into(),
                path: r"C:\Fixture\launcher.exe".into(),
            },
            &plan,
        )
        .unwrap()
    }
    #[test]
    fn observed_native_blacktop_status_does_not_open_rust_memory_access() {
        let mut runner = fixture(Plan {
            wnba_blacktop_categories: true,
            ..Plan::default()
        });
        runner.set(
            "wnba-blacktop",
            "waiting",
            "Waiting for native category status",
        );
        runner.tick(true).unwrap();
        assert!(runner.access.is_none());
        assert!(runner.journal.is_none());
        assert_eq!(
            runner.status()["features"]["wnba-blacktop"]["phase"],
            "waiting"
        );
    }
    #[test]
    fn mixed_wnba_roster_route_waits_for_publication_before_categories() {
        use crate::wnba_roster_v2::{Summary, Variant};
        let summary = |variant| Summary {
            variant,
            teams: 403,
            inserted_at: 161,
            teams_added: 69,
            nba: 30,
            wnba: 15,
            national: vec![69, 361],
            players: 5563,
            players_appended: 53,
            sha256: String::new(),
        };
        let mut runner = fixture(Plan {
            wnba_mixed: Some(Variant::WpfEra),
            ..Plan::default()
        });
        let phase = |r: &Runner| r.status()["features"]["wnba-freestyle"]["phase"].clone();
        assert_eq!(phase(&runner), "preparing");
        assert!(!runner.wnba_loader_enabled(), "withdrawn loader stays off");
        assert!(runner.take_mixed_roster().is_none());
        // Closed owner: no process access, no journal, no publication.
        runner.tick(false).unwrap();
        assert!(runner.access.is_none() && runner.journal.is_none());
        runner.mixed_roster_published(&summary(Variant::WpfEra));
        assert_eq!(phase(&runner), "roster-ready");
        assert!(runner.mixed.published);
        let mut clubs = fixture(Plan {
            wnba_mixed: Some(Variant::Clubs),
            ..Plan::default()
        });
        clubs.mixed_roster_published(&summary(Variant::Clubs));
        assert!(
            clubs.status()["features"]["wnba-freestyle"]["message"]
                .as_str()
                .unwrap()
                .contains("categories stay stock")
        );
        let mut failed = fixture(Plan {
            wnba_mixed: Some(Variant::National),
            ..Plan::default()
        });
        failed.mixed_roster_publication_failed(&anyhow::anyhow!("user roster mod"));
        assert_eq!(phase(&failed), "held");
        assert!(failed.mixed.failed);
    }
    #[cfg(debug_assertions)]
    #[test]
    fn priority_categories_without_separate_approval_never_open_access_or_start_roster_work() {
        let mut runner = fixture(Plan {
            wnba_priority_categories: true,
            ..Plan::default()
        });
        assert_eq!(
            runner.status()["features"]["wnba-freestyle"]["phase"],
            "waiting-source"
        );
        for _ in 0..3 {
            runner.tick(true).unwrap();
            assert!(runner.access.is_none() && runner.journal.is_none());
            assert!(!runner.wnba_loader_enabled());
            assert!(runner.mixed.build.is_none() && runner.take_mixed_roster().is_none());
            assert!(!runner.mixed.published && !runner.freestyle_loader_armed);
            assert!(!runner.freestyle_loader_loaded && !runner.freestyle_applied);
            assert_eq!(
                runner.status()["features"]["wnba-freestyle"]["phase"],
                "waiting-source"
            );
        }
        runner.tick(false).unwrap();
        assert!(runner.access.is_none() && runner.journal.is_none());
        assert_eq!(
            runner.status()["features"]["wnba-freestyle"]["phase"],
            "owner-closed"
        );
    }
    #[test]
    fn freestyle_toggle_off_opens_no_access_and_creates_no_receipt() {
        let settings = crate::backend::Settings::default();
        assert!(!settings.unlock_wnba_freestyle);
        let plan = Plan::freeze(&settings, &crate::library::Library::default()).unwrap();
        assert!(!plan.wnba_priority_categories);
        let mut runner = fixture(plan);
        for _ in 0..3 {
            runner.tick(true).unwrap();
            assert!(runner.access.is_none() && runner.journal.is_none());
            assert!(runner.mixed.build.is_none() && runner.take_mixed_roster().is_none());
        }
        assert_eq!(
            runner.status()["features"]["wnba-freestyle"]["phase"],
            "disabled"
        );
    }
    #[cfg(debug_assertions)]
    #[test]
    fn pending_native_registration_receipt_keeps_freestyle_retryable() {
        let mut runner = fixture(Plan {
            wnba_priority_categories: true,
            ..Plan::default()
        });
        let pending: anyhow::Error = std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Native REGISTERED receipt is not published yet",
        )
        .into();
        for _ in 0..3 {
            runner.wait_for_freestyle_capture(&pending, false);
            assert_eq!(
                runner.status()["features"]["wnba-freestyle"]["phase"],
                "waiting"
            );
            assert!(!runner.held("wnba-freestyle") && !runner.freestyle_applied);
            assert!(runner.access.is_none() && runner.journal.is_none());
        }
    }
    #[cfg(debug_assertions)]
    #[test]
    fn terminal_native_source_refusal_holds_categories_before_any_game_access() {
        let mut runner = fixture(Plan {
            wnba_priority_categories: true,
            ..Plan::default()
        });
        runner.root = std::env::temp_dir().join(format!(
            "wnba-source-refused-{}",
            crate::backend::unique_id()
        ));
        let data = runner.root.join("Data/wnba-priority-v2");
        let logs = runner.root.join("Logs/game-session");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&logs).unwrap();
        let mut token = b"C27WNPC1".to_vec();
        token.extend(runner.identity.pid.to_le_bytes());
        token.extend([0; 4]);
        token.extend(
            runner
                .identity
                .started
                .parse::<u64>()
                .unwrap()
                .to_le_bytes(),
        );
        token.extend(runner.session.bytes());
        std::fs::write(
            data.join(format!("categories-{}.bin", runner.session)),
            token,
        )
        .unwrap();
        std::fs::write(logs.join(format!("wnba-source-test-{}.log", runner.identity.pid)),
            format!("SCHEMA=wnba-native-source-admission-test-v1\nSTATE=REFUSED\nPID={}\nSESSION={}\nREASON=No stable inactive Home/Quick Play boundary within 120 seconds\nCATEGORY_WRITES=0\nGAME_CODE_HOOKS=0\nWNBA_GAMEPLAY_VERIFIED=0\n", runner.identity.pid, runner.session)).unwrap();
        for _ in 0..3 {
            runner.tick(true).unwrap();
            assert!(runner.held("wnba-freestyle"));
            assert!(runner.access.is_none() && runner.journal.is_none());
            assert!(
                runner.status()["features"]["wnba-freestyle"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("Home/Quick Play")
            );
        }
    }
    #[test]
    fn disabled_and_closed_owner_never_open_processes_or_create_receipts() {
        let mut disabled = fixture(Plan::default());
        disabled.tick(true).unwrap();
        assert!(disabled.access.is_none() && disabled.journal.is_none());
        let mut enabled = fixture(Plan {
            wnba_freestyle: true,
            ..Plan::default()
        });
        enabled.tick(false).unwrap();
        assert!(enabled.access.is_none() && enabled.journal.is_none());
        assert_eq!(enabled.status()["features"]["cameras"]["phase"], "disabled");
        assert_eq!(
            enabled.status()["features"]["wnba-freestyle"]["phase"],
            "held"
        );
        assert!(
            enabled.status()["features"]["wnba-freestyle"]["message"]
                .as_str()
                .unwrap()
                .contains("No WNBA memory write")
        );
    }
    #[test]
    fn held_features_do_not_repeatedly_reopen_game_processes() {
        let mut runner = fixture(Plan {
            wnba_freestyle: true,
            ..Plan::default()
        });
        runner.set("wnba-freestyle", "held", "Unverified profile");
        runner.tick(true).unwrap();
        assert!(runner.access.is_none() && runner.journal.is_none());
    }
    #[test]
    fn withdrawn_wnba_loader_fails_closed_before_process_access_or_intent() {
        let mut runner = fixture(Plan {
            wnba_freestyle: true,
            ..Plan::default()
        });
        assert!(!crate::wnba_loader_v2::RUNTIME_ENABLED);
        assert!(!runner.wnba_loader_enabled());
        assert_eq!(
            runner.status()["features"]["wnba-freestyle"]["phase"],
            "held"
        );
        assert!(
            runner.status()["features"]["wnba-freestyle"]["message"]
                .as_str()
                .unwrap()
                .contains("No WNBA memory write")
        );
        // Defense in depth: even an accidental status transition cannot bypass
        // the withdrawn runtime gate.
        runner.set("wnba-freestyle", "waiting", "simulated stale status");
        runner.tick_wnba_loader(true).unwrap();
        runner.tick(true).unwrap();
        assert!(runner.access.is_none());
        assert!(runner.journal.is_none());
    }
    #[test]
    fn returning_to_quick_play_rearms_only_the_roster_loader() {
        let mut runner = fixture(Plan {
            wnba_freestyle: true,
            ..Plan::default()
        });
        runner.freestyle_applied = true;
        runner.freestyle_loader_loaded = true;
        runner.rearm_freestyle_roster_loader();
        assert!(!runner.freestyle_loader_loaded);
        assert!(runner.freestyle_applied);
        assert!(runner.freestyle_missing.last.is_none());
    }
    struct Fake {
        bytes: std::collections::BTreeMap<u64, u8>,
        writes: usize,
        fail_write: Option<usize>,
        partial_write: Option<usize>,
    }
    impl Memory for Fake {
        fn base(&self) -> u64 {
            0x180000000
        }
        fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
            (0..len)
                .map(|offset| {
                    self.bytes
                        .get(&(at + offset as u64))
                        .copied()
                        .ok_or_else(|| anyhow::anyhow!("Missing fake data"))
                })
                .collect()
        }
    }
    impl DataAccess for Fake {
        fn write_intended(
            &mut self,
            feature: Feature,
            change: &Change,
            intent: &crate::feature_journal::Intent,
            _: &mut Journal,
        ) -> Result<()> {
            ensure!(
                intent.permits(feature.name(), change.at, &change.before, &change.after),
                "Missing durable intent"
            );
            ensure!(
                self.read(change.at, change.before.len())? == change.before,
                "Fake precondition changed"
            );
            self.writes += 1;
            if self.fail_write == Some(self.writes) {
                anyhow::bail!("Injected attempted transaction failure");
            }
            if self.partial_write == Some(self.writes) {
                self.bytes.insert(change.at, change.after[0]);
                anyhow::bail!("Injected uncertain partial DWORD write");
            }
            for (offset, value) in change.after.iter().enumerate() {
                self.bytes.insert(change.at + offset as u64, *value);
            }
            Ok(())
        }
    }
    fn intent_fixture() -> (PathBuf, Journal, Fake, Vec<Change>) {
        let root = std::env::temp_dir().join(format!("v2-apply-{}", crate::backend::unique_id()));
        std::fs::create_dir_all(root.join("Data")).unwrap();
        let runner = fixture(Plan::default());
        let journal = Journal::create(
            &root,
            &runner.session,
            &runner.identity,
            &runner.parent,
            &runner.plan,
        )
        .unwrap();
        let changes = vec![
            Change {
                at: 0x10000,
                before: vec![0; 4],
                after: vec![1; 4],
            },
            Change {
                at: 0x20000,
                before: vec![0; 4],
                after: vec![2; 4],
            },
        ];
        let bytes = changes
            .iter()
            .flat_map(|change| (0..4).map(move |n| (change.at + n, 0)))
            .collect();
        (
            root,
            journal,
            Fake {
                bytes,
                writes: 0,
                fail_write: None,
                partial_write: None,
            },
            changes,
        )
    }
    #[test]
    fn context_changed_during_intent_flush_stops_before_the_first_write() {
        let (root, mut journal, mut fake, changes) = intent_fixture();
        let mut checks = 0;
        let result = apply_guarded(&mut fake, &mut journal, Feature::Scaler, changes, |_, _| {
            checks += 1;
            ensure!(checks < 2, "Context changed while intent flushed");
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(fake.writes, 0);
        let text = std::fs::read_to_string(root.join("Data").join(crate::feature_journal::MARKER))
            .unwrap();
        assert!(text.contains("write-intent") && !text.contains("\"event\":\"applied\""));
        journal.finish(|| Ok(())).unwrap();
        drop(journal);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn camera_foreign_bytes_and_write_failures_are_not_transitions() {
        for fail_write in [None, Some(1), Some(2)] {
            let (root, mut journal, mut fake, changes) = intent_fixture();
            fake.fail_write = fail_write;
            if fail_write.is_none() {
                fake.bytes.insert(0x20000, 9);
            }
            let error = apply(&mut fake, &mut journal, Feature::Camera, changes).unwrap_err();
            assert!(!error.is::<CameraPublicationDeferred>());
            let mut runner = fixture(Plan::default());
            runner.record_camera_failure(&error);
            assert!(runner.held("cameras"));
            journal.finish(|| Ok(())).unwrap();
            drop(journal);
            std::fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn all_preconditions_are_checked_before_any_write_and_partial_failure_is_journaled() {
        let (root, mut journal, mut fake, changes) = intent_fixture();
        fake.bytes.insert(0x20000, 9);
        assert!(apply(&mut fake, &mut journal, Feature::Camera, changes.clone()).is_err());
        assert_eq!(fake.writes, 0);
        fake.bytes.insert(0x20000, 0);
        fake.fail_write = Some(2);
        assert!(apply(&mut fake, &mut journal, Feature::Camera, changes).is_err());
        assert_eq!(fake.writes, 2);
        assert_eq!(fake.read(0x10000, 4).unwrap(), [1; 4]);
        let text = std::fs::read_to_string(root.join("Data").join(crate::feature_journal::MARKER))
            .unwrap();
        assert!(text.contains("write-intent") && !text.contains("\"event\":\"applied\""));
        journal.finish(|| Ok(())).unwrap();
        drop(journal);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn freestyle_prefix_guard_includes_untouched_category_words() {
        let plan = crate::freestyle_v2::ApplyPlan {
            changes: crate::freestyle_v2::CHANGES,
            before: crate::freestyle_v2::Tables::original(),
            after: crate::freestyle_v2::Tables::enabled(),
        };
        assert_eq!(freestyle_progress(&plan, 0).unwrap(), plan.before);
        assert_eq!(freestyle_progress(&plan, 4).unwrap(), plan.after);
        for count in 0..=4 {
            let expected = freestyle_progress(&plan, count).unwrap();
            let mut foreign = expected;
            foreign.0[0][0] ^= 1;
            assert_ne!(
                expected, foreign,
                "Unchanged category DWORD is still ownership-checked"
            );
        }
        assert!(freestyle_progress(&plan, 5).is_err());
    }
    #[test]
    fn freestyle_full_picker_guard_brackets_the_tight_four_word_publication() {
        let phases = (0..=4)
            .map(|index| freestyle_full_guard(index, 4))
            .collect::<Vec<_>>();
        assert_eq!(phases, vec![true, false, false, false, true]);
        assert!(freestyle_full_guard(0, 1));
        assert!(freestyle_full_guard(1, 1));
        assert!(!freestyle_full_guard(2, 1));
    }
    #[test]
    fn exhausted_journal_budget_cannot_authorize_a_new_image_write() {
        let (root, mut journal, mut fake, changes) = intent_fixture();
        for _ in 0..1024 {
            journal
                .append(Event::Applied {
                    feature: "scaler".into(),
                })
                .unwrap();
        }
        assert!(apply(&mut fake, &mut journal, Feature::Scaler, changes).is_err());
        assert_eq!(fake.writes, 0);
        journal.finish(|| Ok(())).unwrap();
        drop(journal);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn freestyle_partial_publication_restores_exactly_its_owned_prefix() {
        // Second write fails: the first is reversed, nothing stays half-applied.
        let (root, mut journal, mut fake, changes) = intent_fixture();
        fake.fail_write = Some(2);
        let error =
            publish_freestyle_or_restore(&mut fake, &mut journal, changes.clone(), |_, _| Ok(()))
                .unwrap_err();
        assert!(format!("{error:#}").contains("restored"));
        for c in &changes {
            assert_eq!(fake.read(c.at, 4).unwrap(), c.before);
        }
        assert_eq!(fake.writes, 3);
        assert!(
            error
                .downcast_ref::<FreestylePublicationRestored>()
                .is_some()
        );
        // Real runner holds after this attempted transaction; it is never replayed.
        let mut runner = fixture(Plan::default());
        runner.record_freestyle_failure(&error);
        assert!(runner.held("wnba-freestyle"));
        journal.finish(|| Ok(())).unwrap();
        drop(journal);
        std::fs::remove_dir_all(root).unwrap();
        // The picker moves after the first write: same restore.
        let (root, mut journal, mut fake, changes) = intent_fixture();
        let error =
            publish_freestyle_or_restore(&mut fake, &mut journal, changes.clone(), |_, i| {
                ensure!(i < 2, "Freestyle picker/roster changed before write");
                Ok(())
            })
            .unwrap_err();
        assert!(format!("{error:#}").contains("restored"));
        for c in &changes {
            assert_eq!(fake.read(c.at, 4).unwrap(), c.before);
        }
        journal.finish(|| Ok(())).unwrap();
        drop(journal);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn pre_effect_waiting_is_typed_and_attempted_publication_is_terminal() {
        let mut runner = fixture(Plan::default());
        for _ in 0..32 {
            runner.record_freestyle_failure(
                &FreestylePreEffectDeferred("picker moved".into()).into(),
            );
            assert!(!runner.held("wnba-freestyle"));
        }
        runner
            .record_freestyle_failure(&FreestylePublicationRestored("picker moved".into()).into());
        assert!(runner.held("wnba-freestyle"));
        let mut foreign = fixture(Plan::default());
        foreign.record_freestyle_failure(&anyhow::anyhow!("readiness changed before any effect"));
        assert!(
            foreign.held("wnba-freestyle"),
            "Text cannot authorize waiting"
        );
    }

    fn freestyle_four_fixture() -> (PathBuf, Journal, Fake, Vec<Change>) {
        let (root, journal, mut fake, mut changes) = intent_fixture();
        for i in 3..=4u64 {
            let at = i * 0x10000;
            for offset in 0..4 {
                fake.bytes.insert(at + offset, 0);
            }
            changes.push(Change {
                at,
                before: vec![0; 4],
                after: vec![i as u8; 4],
            });
        }
        (root, journal, fake, changes)
    }
    #[test]
    fn freestyle_pre_intent_refusal_never_spends_journal_or_enters_writer() {
        let (root, mut journal, mut fake, changes) = freestyle_four_fixture();
        let remaining = journal.remaining();
        for _ in 0..32 {
            let error =
                publish_freestyle_or_restore(&mut fake, &mut journal, changes.clone(), |_, _| {
                    Err(crate::freestyle_v2::ReadinessChanged { components: 512 }.into())
                })
                .unwrap_err();
            assert!(error.is::<FreestylePreEffectDeferred>());
            assert_eq!(fake.writes, 0);
            assert_eq!(journal.remaining(), remaining);
        }
        journal.finish(|| Ok(())).unwrap();
        drop(journal);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn freestyle_each_guard_phase_restores_or_defers_by_explicit_effect_phase() {
        // Calls: preintent0, preword0, preword1, preword2, preword3, deep-post4.
        for fail_call in 1..=6 {
            let (root, mut journal, mut fake, changes) = freestyle_four_fixture();
            let mut calls = 0;
            let mut phases = Vec::new();
            let error = publish_freestyle_or_restore(
                &mut fake,
                &mut journal,
                changes.clone(),
                |_, prefix| {
                    calls += 1;
                    phases.push(prefix);
                    if calls == fail_call {
                        return Err(
                            crate::freestyle_v2::ReadinessChanged { components: 0x1000 }.into()
                        );
                    }
                    Ok(())
                },
            )
            .unwrap_err();
            assert_eq!(error.is::<FreestylePreEffectDeferred>(), fail_call == 1);
            assert_eq!(error.is::<FreestylePublicationRestored>(), fail_call != 1);
            for c in &changes {
                assert_eq!(fake.read(c.at, 4).unwrap(), c.before);
            }
            assert_eq!(phases.as_slice(), &[0, 0, 1, 2, 3, 4][..fail_call]);
            let mut runner = fixture(Plan::default());
            runner.record_freestyle_failure(&error);
            assert_eq!(runner.held("wnba-freestyle"), fail_call != 1);
            if fail_call == 2 {
                assert_eq!(
                    fake.writes, 0,
                    "intent entered but no observed word is still terminal"
                );
            }
            if fail_call == 6 {
                assert_eq!(
                    fake.writes, 8,
                    "deep post rejection restores all four words"
                );
            }
            journal.finish(|| Ok(())).unwrap();
            drop(journal);
            std::fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn freestyle_unknown_pre_effect_error_and_attempted_writer_errors_never_defer() {
        for fail in 0..=4 {
            let (root, mut journal, mut fake, changes) = freestyle_four_fixture();
            if fail > 0 {
                fake.fail_write = Some(fail);
            }
            let error =
                publish_freestyle_or_restore(&mut fake, &mut journal, changes.clone(), |_, _| {
                    if fail == 0 {
                        anyhow::bail!("unreadable or authority uncertain");
                    }
                    Ok(())
                })
                .unwrap_err();
            assert!(!error.is::<FreestylePreEffectDeferred>());
            let mut runner = fixture(Plan::default());
            runner.record_freestyle_failure(&error);
            assert!(runner.held("wnba-freestyle"));
            for c in &changes {
                assert_eq!(fake.read(c.at, 4).unwrap(), c.before);
            }
            journal.finish(|| Ok(())).unwrap();
            drop(journal);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn freestyle_uncertain_partial_word_holds_without_forcing_or_replaying() {
        for fail in 1..=4 {
            let (root, mut journal, mut fake, changes) = freestyle_four_fixture();
            fake.partial_write = Some(fail);
            let error =
                publish_freestyle_or_restore(&mut fake, &mut journal, changes.clone(), |_, _| {
                    Ok(())
                })
                .unwrap_err();
            assert!(!error.is::<FreestylePreEffectDeferred>());
            assert!(!error.is::<FreestylePublicationRestored>());
            assert_eq!(
                fake.writes, fail,
                "uncertain word must not authorize any reverse overwrite"
            );
            let c = &changes[fail - 1];
            assert_ne!(fake.read(c.at, 4).unwrap(), c.before);
            assert_ne!(fake.read(c.at, 4).unwrap(), c.after);
            let mut runner = fixture(Plan::default());
            runner.record_freestyle_failure(&error);
            assert!(runner.held("wnba-freestyle"));
            // Exact game-closed cleanup only; no new live publication is attempted.
            journal.finish(|| Ok(())).unwrap();
            drop(journal);
            std::fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn freestyle_reserved_cleanup_budget_is_checked_before_any_effect_or_intent() {
        let (root, mut journal, mut fake, changes) = freestyle_four_fixture();
        while journal.remaining() >= transaction_events(changes.len()).saturating_mul(2) {
            journal
                .append(Event::Applied {
                    feature: "fixture".into(),
                })
                .unwrap();
        }
        let before = journal.remaining();
        let mut checked = false;
        let error = publish_freestyle_or_restore(&mut fake, &mut journal, changes, |_, _| {
            checked = true;
            Ok(())
        })
        .unwrap_err();
        assert!(!error.is::<FreestylePreEffectDeferred>());
        assert!(!checked);
        assert_eq!(fake.writes, 0);
        assert_eq!(journal.remaining(), before);
        journal.finish(|| Ok(())).unwrap();
        drop(journal);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn freestyle_restore_never_reverses_values_it_did_not_write() {
        // Success: all four entries stay published.
        let (root, mut journal, mut fake, changes) = intent_fixture();
        publish_freestyle_or_restore(&mut fake, &mut journal, changes.clone(), |_, _| Ok(()))
            .unwrap();
        for c in &changes {
            assert_eq!(fake.read(c.at, 4).unwrap(), c.after);
        }
        journal.finish(|| Ok(())).unwrap();
        drop(journal);
        std::fs::remove_dir_all(root).unwrap();
        // An entry already at its after-value before we wrote: refused with
        // nothing written and nothing "restored".
        let (root, mut journal, mut fake, changes) = intent_fixture();
        for n in 0..4 {
            fake.bytes.insert(0x10000 + n, 1);
        }
        assert!(
            publish_freestyle_or_restore(&mut fake, &mut journal, changes, |_, _| Ok(())).is_err()
        );
        assert_eq!(fake.writes, 0);
        assert_eq!(fake.read(0x10000, 4).unwrap(), vec![1; 4]);
        journal.finish(|| Ok(())).unwrap();
        drop(journal);
        std::fs::remove_dir_all(root).unwrap();
    }
}
