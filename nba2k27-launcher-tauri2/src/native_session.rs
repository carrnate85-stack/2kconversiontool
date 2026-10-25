//! Native launch journal and observation. Explicit modded QA delegates the
//! proven native DLL and profile lifecycle to an independently owned provider.
//! This UI service never stops Steam/the game or installs over Main.
use crate::{
    backend, blacktop_native, game_rules,
    session_safety::{self, Identity, Journal, LaunchContext, Phase, Process},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Debug)]
struct TransientProviderStatusIo(std::io::Error);
impl std::fmt::Display for TransientProviderStatusIo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Provider status read unavailable: {}", self.0)
    }
}
impl std::error::Error for TransientProviderStatusIo {}
fn provider_status_observation(result: Result<Value>) -> Result<Option<Value>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.is::<TransientProviderStatusIo>() => Ok(None),
        Err(error) => Err(error),
    }
}
pub(crate) fn cleanup_step(
    errors: &mut Vec<String>,
    name: &str,
    action: impl FnOnce() -> Result<()>,
) {
    if let Err(error) = action() {
        errors.push(format!("{name}: {error:#}"));
    }
}
pub(crate) fn cleanup_result(errors: Vec<String>) -> Result<()> {
    ensure!(
        errors.is_empty(),
        "Owned recovery remains pending: {}",
        errors.join("; ")
    );
    Ok(())
}
fn recovery_root(
    portable: &Path,
    session: &str,
    profile: Option<PathBuf>,
    game: &Path,
    errors: &mut Vec<String>,
) -> Result<PathBuf> {
    let mut roots: Vec<PathBuf> = profile.into_iter().collect();
    for (name, result) in [
        (
            "selector root",
            crate::selector_files::Store::recorded_root(portable, session),
        ),
        (
            "camera root",
            crate::camera_name_files::Store::recorded_root(portable, session),
        ),
        (
            "player root",
            crate::player_folders::Store::recorded_root(portable, session),
        ),
        (
            "Jukebox root",
            crate::jukebox_files::Store::recorded_root(portable, session),
        ),
    ] {
        match result {
            Ok(Some(root)) => roots.push(root),
            Ok(None) => {}
            Err(error) => errors.push(format!("{name}: {error:#}")),
        }
    }
    root_consensus(&roots, game)
}
fn root_consensus(roots: &[PathBuf], game: &Path) -> Result<PathBuf> {
    let selected = roots.first().cloned().unwrap_or_else(|| game.join("mods"));
    ensure!(
        roots
            .iter()
            .all(|root| same_path(&root.to_string_lossy(), &selected)),
        "Owned recovery receipts disagree on their physical Mods root"
    );
    Ok(selected)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
pub(crate) fn same_path(a: &str, b: &Path) -> bool {
    fn normalized(s: &str) -> String {
        if let Some(s) = s.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{s}").to_ascii_lowercase();
        }
        s.strip_prefix(r"\\?\").unwrap_or(s).to_ascii_lowercase()
    }
    normalized(a) == normalized(&b.to_string_lossy())
}
fn quiet(snapshot: &[Process]) -> Result<()> {
    ensure!(
        !snapshot.iter().any(session_safety::launcher_process),
        "Another launcher/backend is present; Steam-only dispatch refused"
    );
    ensure!(
        !snapshot
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe")),
        "NBA 2K27 is running; no second launch"
    );
    ensure!(
        !snapshot
            .iter()
            .any(|p| session_safety::protected_process(&p.name)),
        "Anti-cheat process present; launch refused"
    );
    Ok(())
}
fn launch_allowed(stopped: &AtomicBool) -> Result<()> {
    ensure!(
        !stopped.load(Ordering::Acquire),
        "Launcher is closing; new game launches are stopped"
    );
    Ok(())
}
fn dispatch_if_allowed<T>(stopped: &AtomicBool, dispatch: impl FnOnce() -> Result<T>) -> Result<T> {
    launch_allowed(stopped)?;
    dispatch()
}
struct Controller {
    root: PathBuf,
    journal: Option<Journal>,
    message: String,
    error: Option<String>,
    blacktop_enabled: bool,
    blacktop: Value,
    rules: Option<game_rules::Lease>,
    rule_writes: u32,
    provider: Option<crate::provider_worker::Worker>,
}
fn modded(j: &Journal) -> bool {
    j.launch
        .as_ref()
        .is_some_and(|l| l.kind == "native-modded-qa")
}
fn arena_owner(j: &Journal, identity: &Identity, session: &str, worker: &Identity) -> Result<()> {
    ensure!(
        j.phase == Phase::Running
            && modded(j)
            && j.session == session
            && j.game.as_ref() == Some(identity),
        "This arena request does not belong to the current modded game"
    );
    ensure!(
        j.helpers.len() == 1
            && j.helpers[0].role == "provider"
            && j.helpers[0].session == session
            && j.helpers[0].identity == *worker,
        "Arena provider ownership changed"
    );
    Ok(())
}
fn mod_pending(root: &Path) -> Result<bool> {
    Ok(root
        .join("Data")
        .join(crate::feature_journal::MARKER)
        .try_exists()?
        || root.join("Data/loader-session.json").try_exists()?
        || crate::outfit_worker_v2_loader::pending(root)?
        || crate::outfit_worker_v2_lease::pending(root)?
        || root
            .join("Data")
            .join(crate::selector_files::MARKER)
            .try_exists()?
        || root
            .join("Data")
            .join(crate::camera_name_files::MARKER)
            .try_exists()?
        || root.join("Data/mod-profile-session.json").try_exists()?
        || root
            .join("Data")
            .join(crate::player_folders::MARKER)
            .try_exists()?
        || crate::arena_session::pending(root)?
        || crate::ball_session::pending(root)?
        || crate::ball_gameplay_runtime::pending(root)?
        || crate::outfit_journal::pending(root)?
        || crate::presentation_v2_runtime::pending(root)?
        || crate::scoreboard_runtime::pending(root)?
        || crate::jukebox_launch::pending(root)?)
}
fn owned_provider_record(j: &Journal) -> bool {
    modded(j)
        && j.files.is_empty()
        && j.helpers.len() <= 1
        && j.helpers.iter().all(|h| h.role == "provider")
}
fn intro_launch_phase(expected: &str, actual: &str, phase: &Phase, failed: bool) -> Result<bool> {
    ensure!(
        expected.len() == 48
            && expected.bytes().all(|b| b.is_ascii_hexdigit())
            && actual == expected
            && !failed,
        "Startup video launch owner changed or failed"
    );
    match phase {
        Phase::Waiting => Ok(false),
        Phase::Running => Ok(true),
        _ => anyhow::bail!("Startup video launch is no longer active"),
    }
}
impl Controller {
    fn load_reconciled(root: PathBuf, snapshot: impl FnOnce() -> Result<Vec<Process>>) -> Self {
        let mut controller = Self::load(root);
        if controller.error.is_none()
            && controller
                .journal
                .as_ref()
                .is_some_and(|j| j.phase != Phase::Closed)
        {
            // Resolve a finished worker's outer journal before exposing Play.
            // observe never restores game files: live/unknown owners and any
            // remaining physical receipts continue to hold launch admission.
            if let Err(error) = controller.observe(snapshot()) {
                controller.error = Some(format!("Session recovery needs review: {error:#}"));
            }
        }
        // WPF restored a crashed session's owned files at startup whenever the
        // game was closed (main.js jukeboxSession.restore() and siblings).
        // Run the same guarded Clear once: game closed, recorded provider and
        // helpers gone, exact owned receipts only. Any refusal leaves the
        // record pending for Finish cleanup, exactly as before.
        if controller.error.is_none()
            && controller
                .journal
                .as_ref()
                .is_some_and(|j| j.phase == Phase::CleanupPending)
        {
            if let Err(error) = controller.clear(session_safety::processes()) {
                controller.message =
                    format!("Previous session cleanup is still pending: {error:#}");
            }
        }
        controller
    }
    fn load(root: PathBuf) -> Self {
        let mut c = Self {
            root,
            journal: None,
            message: "No native launch session.".into(),
            error: None,
            blacktop_enabled: false,
            blacktop: blacktop_native::idle(),
            rules: None,
            rule_writes: 0,
            provider: None,
        };
        match c.path().try_exists() {
            Ok(false) => {}
            Ok(true) => match session_safety::read_journal(&c.path()) {
                Ok(j) => {
                    // Never adopt a journal containing modifications/helper leases from another engine.
                    if j.launch.is_none()
                        || (!owned_provider_record(&j)
                            && (!j.helpers.is_empty() || !j.files.is_empty()))
                    {
                        c.error = Some("Session recovery belongs to an unsupported engine; original journal left untouched.".into());
                    } else {
                        c.journal = Some(j);
                        c.message="Previous native launch record loaded; awaiting a verified process snapshot.".into();
                    }
                }
                Err(e) => {
                    c.error = Some(format!(
                        "Session journal needs review; left untouched: {e:#}"
                    ))
                }
            },
            Err(e) => c.error = Some(format!("Session journal presence unknown: {e}")),
        }
        c
    }
    fn path(&self) -> PathBuf {
        self.root.join("Data/session-journal.json")
    }
    fn commit(&mut self, journal: Journal) -> Result<()> {
        journal.validate()?;
        backend::atomic_json(&self.path(), &journal)?;
        self.journal = Some(journal);
        Ok(())
    }
    fn view(&self) -> Value {
        json!({"phase": if self.error.is_some(){"needs-review"}else{match self.journal.as_ref().map(|j| &j.phase){Some(Phase::Waiting)=>"waiting-for-game",Some(Phase::Running)=>"game-running",Some(Phase::Closed)=>"closed",Some(Phase::CleanupPending)=>"needs-review",None=>"idle"}},"message":self.message,"error":self.error,"session":self.journal.as_ref().map(|j| &j.session),"game":self.journal.as_ref().and_then(|j| j.game.as_ref()),"steamOnly":!self.journal.as_ref().is_some_and(modded),"nativeProvider":self.provider_view().unwrap_or_else(|e|json!({"phase":"unknown","error":format!("{e:#}")})),"automaticFeaturesEnabled":false,"verifiedRuleWrites":self.rule_writes,"gameStoppedByLauncher":false})
    }
    fn provider_view(&self) -> Result<Value> {
        self.provider_view_with_live_status(true)
    }
    fn provider_view_with_live_status(&self, live: bool) -> Result<Value> {
        let Some(j) = self.journal.as_ref().filter(|j| modded(j)) else {
            return Ok(Value::Null);
        };
        let validate = |v: &Value| -> Result<()> {
            ensure!(
                serde_json::to_vec(v)?.len() <= 65536,
                "Provider status exceeds bounds"
            );
            ensure!(
                v["schema"] == "nba2k27-native-provider-session-v1"
                    && v["session"] == j.session
                    && j.helpers
                        .first()
                        .is_none_or(|h| v["workerPid"].as_u64() == Some(h.identity.pid as u64)),
                "Provider status owner differs"
            );
            Ok(())
        };
        // Authenticated, read-only status sees the outfit queue immediately.
        // The durable snapshot may lag while the provider handles other work.
        // Retain the file fallback for cleanup/recovery or unavailable transport.
        if live
            && j.phase == Phase::Running
            && let Some(worker) = &self.provider
            && let Ok(status) = worker.control(json!({"operation":"modded-status"}))
        {
            validate(&status)?;
            return Ok(status);
        }
        let path = self
            .root
            .join("Logs/native-sessions")
            .join(&j.session)
            .join("provider-state.json");
        let mut file = match fs::File::open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(json!({"phase":"queued","message":"Waiting for the native provider"}));
            }
            Err(e) => return Err(TransientProviderStatusIo(e).into()),
        };
        use std::io::Read;
        use std::os::windows::fs::MetadataExt;
        let meta = file.metadata().map_err(TransientProviderStatusIo)?;
        ensure!(
            meta.is_file() && meta.len() <= 65536 && meta.file_attributes() & 0x400 == 0,
            "Invalid provider session status"
        );
        let mut bytes = Vec::new();
        (&mut file)
            .take(65537)
            .read_to_end(&mut bytes)
            .map_err(TransientProviderStatusIo)?;
        ensure!(bytes.len() <= 65536, "Provider status exceeds bounds");
        let v: Value = serde_json::from_slice(&bytes)?;
        validate(&v)?;
        Ok(v)
    }
    fn launch_modded(
        &mut self,
        game: &Path,
        mods: &Path,
        features: crate::feature_plan::Plan,
        stopped: &AtomicBool,
    ) -> Result<Value> {
        launch_allowed(stopped)?;
        features.validate()?;
        ensure!(
            self.error.is_none()
                && self
                    .journal
                    .as_ref()
                    .is_none_or(|j| j.phase == Phase::Closed),
            "Finish/recover the previous session first"
        );
        let ready = session_safety::preflight(game, &self.root);
        ensure!(
            ready["checksPassed"] == true,
            "Launch readiness failed; check the blockers"
        );
        quiet(&session_safety::processes()?)?;
        let _image = crate::provider_access::verify_build(&game.join("NBA2K27.exe"))?;
        // A re-derived game update admits mod loading only: every native
        // feature in the frozen plan needs a new mapping and is dropped.
        let resolved_update = _image.build["label"] == crate::feature_relocation::RESOLVED_LABEL;
        let features = if resolved_update {
            crate::feature_plan::Plan::default()
        } else {
            features
        };
        ensure!(
            mods.is_absolute() && mods.is_dir(),
            "Select an existing absolute mod profile"
        );
        let plan = session_safety::resolve(game)?;
        let session = backend::unique_id();
        self.commit(Journal {
            version: 1,
            session: session.clone(),
            phase: Phase::Waiting,
            game: None,
            helpers: vec![],
            files: vec![],
            launch: Some(LaunchContext {
                game_root: plan["gameRoot"]
                    .as_str()
                    .context("Game root missing")?
                    .into(),
                app_id: plan["appId"].as_str().context("App ID missing")?.into(),
                requested_ms: now_ms(),
                kind: "native-modded-qa".into(),
            }),
        })?;
        self.blacktop_enabled = false;
        self.rules = None;
        self.rule_writes = 0;
        self.blacktop = blacktop_native::idle();
        // Bootstrap/indexing may take seconds. Closing does not need this
        // controller mutex and must prevent a still-pending game dispatch.
        if let Err(error) = launch_allowed(stopped) {
            self.close_unstarted_provider()?;
            return Err(error);
        }
        let created = crate::provider_worker::Worker::start_owned(
            &game.join("NBA2K27.exe"),
            mods,
            crate::provider_launch::Owner {
                portable: self.root.clone(),
                session: session.clone(),
            },
            features,
        );
        let mut worker = match created {
            Ok(w) => w,
            Err(e) => {
                let mut j = self.journal.clone().unwrap();
                j.phase = Phase::Closed;
                self.commit(j)?;
                self.message =
                    format!("Native provider could not start; Steam was not dispatched: {e:#}");
                let _ = backend::atomic_json(
                    &self.root.join("Data/launch-last-error.json"),
                    &json!({
                        "recordedMs": now_ms(),
                        "stage": "provider-bootstrap",
                        "message": self.message,
                        "steamDispatched": false
                    }),
                );
                return Err(e);
            }
        };
        let cancelled_after_bootstrap = launch_allowed(stopped).err();
        let mut j = self.journal.clone().unwrap();
        j.helpers.push(session_safety::HelperLease {
            session,
            role: "provider".into(),
            identity: worker.identity.clone(),
            binary_sha256: crate::helper_owner::hash(&std::env::current_exe()?)?,
        });
        if let Err(error) = self.commit(j) {
            // Do not lose the exact unstarted child merely because publishing
            // its durable receipt failed. Its existing graceful stop/drop
            // path is still required; no PID/name kill is introduced.
            return self.stop_prepared_provider(worker, error);
        }
        if let Some(error) = cancelled_after_bootstrap {
            return self.stop_prepared_provider(worker, error);
        }
        // Record the exact helper before testing cancellation after indexing,
        // so even an uncertain graceful stop remains owned and clearable.
        let ready = (|| {
            launch_allowed(stopped)?;
            quiet(&session_safety::processes()?)?;
            Ok(())
        })();
        if let Err(error) = ready {
            return self.stop_prepared_provider(worker, error);
        }
        let mut attempted = false;
        let queued = dispatch_if_allowed(stopped, || {
            attempted = true;
            worker.launch()
        });
        if !attempted {
            return self.stop_prepared_provider(worker, queued.unwrap_err());
        }
        self.provider = Some(worker);
        match queued {
            Ok(_) => {
                self.message = if resolved_update {
                    crate::feature_relocation::resolved_update_message().into()
                } else {
                    "Native modded QA requested. Automatic Blacktop features remain off; explicit arena, ball and Game Rules QA are available after activation.".into()
                }
            }
            Err(e) => {
                self.message = format!(
                    "Native launch acknowledgement is uncertain; session retained, do not retry: {e:#}"
                );
                return Err(e);
            }
        }
        Ok(self.view())
    }
    fn close_unstarted_provider(&mut self) -> Result<()> {
        let mut journal = self
            .journal
            .clone()
            .context("Unstarted launch record missing")?;
        ensure!(
            modded(&journal) && journal.phase == Phase::Waiting && journal.game.is_none(),
            "Cannot close a provider launch after game dispatch"
        );
        journal.phase = Phase::Closed;
        journal.helpers.clear();
        self.commit(journal)
    }
    fn stop_prepared_provider(
        &mut self,
        mut worker: crate::provider_worker::Worker,
        cause: anyhow::Error,
    ) -> Result<Value> {
        match worker.stop() {
            Ok(()) => {
                // Drop uses the existing graceful unstarted-worker path, not
                // process termination, and is immediate for this exited child.
                drop(worker);
                self.close_unstarted_provider()
                    .context("Unstarted provider stopped but journal closure needs recovery")?;
                self.message = format!("Launch stopped before Steam dispatch: {cause:#}");
                Err(cause)
            }
            Err(cleanup) => {
                // Never mark an uncertain helper gone, discard its receipt or
                // skip owned file restoration. Observation will wait for it.
                self.provider = Some(worker);
                self.message = format!(
                    "Launch was not dispatched; owned provider cleanup is pending: {cause:#}; {cleanup:#}"
                );
                Err(cause.context(format!("Owned provider cleanup is pending: {cleanup:#}")))
            }
        }
    }
    fn launch<F>(
        &mut self,
        game: &Path,
        preflight: Value,
        mut snapshot: impl FnMut() -> Result<Vec<Process>>,
        send: F,
    ) -> Result<Value>
    where
        F: FnOnce(&Path, &str) -> Result<()>,
    {
        ensure!(self.error.is_none(), "Session recovery needs review");
        ensure!(
            !crate::player_transaction::pending(&self.root)?,
            "A player transaction needs review; close the game and clear finished launch before retrying"
        );
        ensure!(
            !crate::team_transaction::pending(&self.root)?,
            "A team transaction needs review; close the game and clear finished launch before retrying"
        );
        ensure!(
            !crate::uniform_transaction::pending(&self.root)?,
            "A uniform transaction needs review; close the game and clear finished launch before retrying"
        );
        ensure!(
            !crate::roster_v2_write::pending(&self.root)?,
            "A Version 2 roster transaction needs review; close the game and clear finished launch before retrying"
        );
        ensure!(
            !game_rules::pending(&self.root)?,
            "A rule transaction needs review; close the game and clear finished launch before retrying"
        );
        ensure!(
            !crate::arena_session::pending(&self.root)?,
            "An arena transaction needs review; close the game and clear finished launch before retrying"
        );
        ensure!(
            !crate::ball_session::pending(&self.root)?,
            "A ball transaction needs review; close the game and clear finished launch before retrying"
        );
        ensure!(
            !crate::ball_gameplay_runtime::pending(&self.root)?,
            "A gameplay-ball transaction needs review; close the game and clear finished launch before retrying"
        );
        ensure!(
            !crate::outfit_journal::pending(&self.root)?,
            "An outfit transaction needs review; close the game and clear finished launch before retrying"
        );
        ensure!(
            !crate::outfit_worker_v2_loader::pending(&self.root)?
                && !crate::outfit_worker_v2_lease::pending(&self.root)?,
            "A Version 2 outfit worker/loader needs owned recovery before a new launch"
        );
        ensure!(
            self.journal
                .as_ref()
                .is_none_or(|j| j.phase == Phase::Closed),
            "A native launch session is already active"
        );
        ensure!(
            preflight["checksPassed"] == true && preflight["readOnly"] == true,
            "Launch eligibility failed; check readiness"
        );
        let plan = session_safety::resolve(game)?;
        ensure!(
            plan == preflight["launchPlan"],
            "Steam launch plan changed; retry readiness"
        );
        let game_root = plan["gameRoot"].as_str().context("Missing game root")?;
        let steam = Path::new(
            plan["steamExe"]
                .as_str()
                .context("Missing Steam executable")?,
        );
        let url = plan["steamUrl"]
            .as_str()
            .context("Missing Steam launch option")?;
        quiet(&snapshot()?)?;
        let j = Journal {
            version: 1,
            session: backend::unique_id(),
            phase: Phase::Waiting,
            game: None,
            helpers: vec![],
            files: vec![],
            launch: Some(LaunchContext {
                game_root: game_root.into(),
                app_id: plan["appId"].as_str().context("Missing app ID")?.into(),
                requested_ms: now_ms(),
                kind: "steam-only-qa".into(),
            }),
        };
        self.commit(j)?; // Durable intent BEFORE dispatch; failed save must not launch.
        self.blacktop_enabled = false;
        self.rules = None;
        self.rule_writes = 0;
        self.blacktop = blacktop_native::idle();
        let result = (|| {
            // The per-root gate remains locked; repeat process presence after journaling.
            quiet(&snapshot()?)?;
            ensure!(
                !game.join("version.dll").try_exists()?,
                "A version.dll appeared; launch refused"
            );
            ensure!(
                session_safety::resolve(game)? == plan,
                "Steam launch plan changed before dispatch"
            );
            send(steam, url)
        })();
        if let Err(e) = result {
            let mut closed = self.journal.clone().unwrap();
            closed.phase = Phase::Closed;
            if let Err(save) = self.commit(closed) {
                self.error = Some(format!(
                    "Launch failed: {e:#}; journal closure failed: {save:#}"
                ));
                anyhow::bail!("{}", self.error.as_ref().unwrap());
            }
            self.message = format!("Steam dispatch failed; no game was adopted: {e:#}");
            return Err(e);
        }
        self.message =
            "Steam-only QA launch sent. Waiting for NBA 2K27; launcher features are not applied."
                .into();
        Ok(self.view())
    }
    fn observe(&mut self, snapshot: Result<Vec<Process>>) -> Result<()> {
        let Some(mut j) = self.journal.clone() else {
            return Ok(());
        };
        if j.phase == Phase::Closed {
            return Ok(());
        }
        let rows = match snapshot {
            Ok(v) => v,
            Err(e) => {
                self.message = format!("Process presence unknown; session record held: {e:#}");
                return Ok(());
            }
        };
        let games: Vec<_> = rows
            .iter()
            .filter(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe"))
            .collect();
        if games.is_empty() {
            if modded(&j) {
                let live = if let Some(worker) = &mut self.provider {
                    !worker.exited()?
                } else {
                    j.helpers.iter().any(|h| {
                        rows.iter().any(|p| {
                            p.pid == h.identity.pid
                                && (p.identity.is_none()
                                    || p.identity.as_ref() == Some(&h.identity))
                        })
                    })
                };
                if live {
                    self.message="The native provider still owns this session. Waiting for its exact game-exit and cleanup confirmation.".into();
                    return Ok(());
                }
                if mod_pending(&self.root)? {
                    j.phase = Phase::CleanupPending;
                    self.commit(j)?;
                    self.message="Game/provider ended with owned recovery pending. Close other launchers and use Clear finished launch.".into();
                    return Ok(());
                }
                j.phase = Phase::Closed;
                j.helpers.clear();
                self.commit(j)?;
                self.rules = None;
                self.provider.take();
                self.message="Native provider ended; loader/profile recovery is clear. Check native provider status for any launch error.".into();
                return Ok(());
            }
            if j.game.is_some() {
                j.phase = Phase::Closed;
                self.commit(j)?;
                self.rules = None;
                self.message="NBA 2K27 ended. Native launch record closed; game-memory rule values no longer exist. Pending rule recovery, if any, requires Clear finished launch.".into();
            } else {
                let age = now_ms().saturating_sub(
                    j.launch
                        .as_ref()
                        .context("Launch context missing")?
                        .requested_ms,
                );
                self.message=if age>120_000 {"No game observed after two minutes. Monitoring remains active; clear it only if Steam will not launch."}else{"Waiting for NBA 2K27. No launcher features are applied."}.into();
            }
            return Ok(());
        }
        if games.len() != 1 {
            self.message = "Multiple NBA 2K27 processes; session observation held.".into();
            return Ok(());
        }
        let Some(i) = games[0].identity.as_ref() else {
            self.message = "Game identity unavailable; session observation held.".into();
            return Ok(());
        };
        let launch = j.launch.as_ref().context("Launch context missing")?;
        let expected = Path::new(&launch.game_root).join("NBA2K27.exe");
        if !same_path(&i.path, &expected) {
            self.message = "A game from a different folder is running; not adopted.".into();
            return Ok(());
        }
        if let Some(original) = &j.game {
            if i != original {
                self.message = "Game identity changed; original session held for review.".into();
                return Ok(());
            }
        } else {
            let floor = launch
                .requested_ms
                .checked_mul(10_000)
                .and_then(|n| n.checked_add(116_444_736_000_000_000))
                .context("Invalid launch timestamp")?;
            if i.started.parse::<u64>()? < floor {
                self.message = "A game predating this launch is running; not adopted.".into();
                return Ok(());
            }
            j.game = Some(i.clone());
            j.phase = Phase::Running;
            self.commit(j)?;
        }
        self.message = if rows
            .iter()
            .any(|p| session_safety::protected_process(&p.name))
        {
            "NBA 2K27 observed with an anti-cheat process. All native game access is blocked."
                .into()
        } else if self.journal.as_ref().is_some_and(modded) {
            let Some(view) = provider_status_observation(self.provider_view())? else {
                self.message = "Provider status temporarily unreadable; waiting for a fresh validated observation.".into();
                return Ok(());
            };
            view["message"]
                .as_str()
                .unwrap_or("Native provider is preparing the modded session")
                .into()
        } else {
            "NBA 2K27 observed. No automatic features applied; explicit native Game Rules QA is available below.".into()
        };
        Ok(())
    }
    fn observer_binding(&self, rows: &[Process]) -> Result<(Identity, PathBuf)> {
        ensure!(self.error.is_none(), "Native session needs review");
        let j = self
            .journal
            .as_ref()
            .context("Launch through this prototype first")?;
        ensure!(
            j.phase == Phase::Running,
            "Wait for the launch session to show game-running"
        );
        let id = j.game.as_ref().context("No observed game identity")?;
        blacktop_native::guard(rows, id)?;
        Ok((
            id.clone(),
            PathBuf::from(
                &j.launch
                    .as_ref()
                    .context("Launch context missing")?
                    .game_root,
            ),
        ))
    }
    fn enable_observer(&mut self, rows: Result<Vec<Process>>) -> Result<Value> {
        self.observer_binding(&rows?)?;
        self.blacktop_enabled = true;
        self.blacktop = blacktop_native::status(
            "waiting",
            "Starting read-only Blacktop observation. No game writes.",
            None,
        );
        Ok(self.blacktop.clone())
    }
    fn observe_blacktop(&mut self, reader: &mut Option<blacktop_native::Reader>) {
        if !self.blacktop_enabled {
            reader.take();
            return;
        }
        if self
            .journal
            .as_ref()
            .is_none_or(|j| j.phase == Phase::Closed)
        {
            reader.take();
            self.blacktop_enabled = false;
            self.blacktop = blacktop_native::status(
                "ended",
                "Launch session ended; read-only handle released.",
                None,
            );
            return;
        }
        let result = (|| -> Result<blacktop_native::Snapshot> {
            let (id, game) = self.observer_binding(&session_safety::processes()?)?;
            if reader.as_ref().is_none_or(|r| r.identity != id) {
                reader.take();
                *reader = Some(blacktop_native::Reader::open_observer(&id, &game)?);
            }
            let value = reader
                .as_ref()
                .context("Observer handle missing")?
                .observer_snapshot()?;
            self.observer_binding(&session_safety::processes()?)?;
            Ok(value)
        })();
        let next = match result {
            Ok(s) => blacktop_native::status(
                "observing",
                "Native read-only sample. Features are not applied.",
                Some(&s),
            ),
            Err(e) => {
                reader.take();
                blacktop_native::status("held", &format!("No trusted sample: {e:#}"), None)
            }
        };
        if next != self.blacktop {
            let log = json!({"session":self.journal.as_ref().map(|j|&j.session),"observedAtMs":now_ms(),"observation":next});
            if let Err(e) =
                backend::atomic_json(&self.root.join("Logs/blacktop-observer.json"), &log)
            {
                reader.take();
                self.blacktop_enabled = false;
                self.blacktop = blacktop_native::status(
                    "held",
                    &format!("Observer diagnostics unavailable; reading stopped: {e:#}"),
                    None,
                );
            } else {
                self.blacktop = next;
            }
        }
    }
    fn clear(&mut self, rows: Result<Vec<Process>>) -> Result<Value> {
        self.clear_with_closed(rows, || quiet(&session_safety::processes()?))
    }
    fn clear_with_closed(
        &mut self,
        rows: Result<Vec<Process>>,
        closed: impl Fn() -> Result<()>,
    ) -> Result<Value> {
        let _write_gate = session_safety::write_gate()?;
        ensure!(
            self.error.is_none(),
            "Unsupported or damaged journal must be reviewed; not cleared automatically"
        );
        let rows = rows?;
        if let Some(worker) = &mut self.provider {
            ensure!(
                worker.exited()?,
                "Native provider is still completing this session; do not pull its resources"
            );
        }
        ensure!(
            !rows
                .iter()
                .any(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe")),
            "Close NBA 2K27 before clearing the launch record"
        );
        if let Some(j) = self.journal.as_ref().filter(|j| modded(j)) {
            ensure!(
                !j.helpers
                    .iter()
                    .any(|h| rows.iter().any(|p| p.pid == h.identity.pid
                        && (p.identity.is_none() || p.identity.as_ref() == Some(&h.identity)))),
                "The recorded native provider is still active; recovery is held"
            );
            let game = Path::new(&j.launch.as_ref().unwrap().game_root);
            // Two independent loader variants can never own the same physical
            // game-side names. Hold both receipts before mutating either one.
            let old_loader = match fs::symlink_metadata(self.root.join("Data/loader-session.json"))
            {
                Ok(_) => true,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
                Err(e) => return Err(e.into()),
            };
            ensure!(
                !(old_loader && crate::outfit_worker_v2_loader::pending(&self.root)?),
                "Original and Version 2 outfit loader receipts collide; neither was changed"
            );
            let mut physical_errors = Vec::new();
            cleanup_step(&mut physical_errors, "loader", || {
                if let Some(mut store) = crate::loader_files::Store::recover(&self.root, game)? {
                    ensure!(
                        store.session() == j.session,
                        "Loader recovery belongs to another session"
                    );
                    store.restore(&closed)?;
                }
                Ok(())
            });
            cleanup_step(&mut physical_errors, "V2 loader", || {
                if let Some(mut store) =
                    crate::outfit_worker_v2_loader::Store::recover(&self.root, game)?
                {
                    ensure!(
                        store.session() == j.session,
                        "Version 2 outfit loader belongs to another session"
                    );
                    store.restore(&closed)?;
                }
                Ok(())
            });
            let mut profile = None;
            cleanup_step(&mut physical_errors, "profile receipt", || {
                let recovered = crate::mod_profile::Store::recover(&self.root, game)?;
                if let Some(store) = &recovered {
                    ensure!(
                        store.session() == j.session,
                        "Profile recovery belongs to another session"
                    );
                }
                profile = recovered;
                Ok(())
            });
            let selected = recovery_root(
                &self.root,
                &j.session,
                profile.as_ref().map(|s| s.selected_root().to_path_buf()),
                game,
                &mut physical_errors,
            );
            if let Ok(selected) = &selected {
                cleanup_step(&mut physical_errors, "selectors", || {
                    if let Some(mut store) =
                        crate::selector_files::Store::recover(&self.root, selected, &j.session)?
                    {
                        store.restore(&closed)?;
                    }
                    Ok(())
                });
                cleanup_step(&mut physical_errors, "camera labels", || {
                    if let Some(mut store) =
                        crate::camera_name_files::Store::recover(&self.root, selected, &j.session)?
                    {
                        store.restore(&closed)?;
                    }
                    Ok(())
                });
                cleanup_step(&mut physical_errors, "player links", || {
                    if let Some(mut store) =
                        crate::player_folders::Store::recover(&self.root, selected, &j.session)?
                    {
                        store.restore(&closed)?;
                    }
                    Ok(())
                });
                cleanup_step(&mut physical_errors, "Jukebox", || {
                    crate::jukebox_launch::recover_ended(
                        &self.root, selected, &j.session, &closed,
                    )?;
                    Ok(())
                });
            } else if let Err(error) = selected {
                physical_errors.push(format!("physical root: {error:#}"));
            }
            cleanup_step(&mut physical_errors, "profile junction", || {
                if let Some(store) = &mut profile {
                    store.restore(&closed)?;
                }
                Ok(())
            });
            cleanup_result(physical_errors)?;
            // Ended memory receipts are independent of the exact-owned files.
            // Their validation can hold final recovery without stranding DLLs
            // or a mod-profile junction that can already be restored safely.
            closed()?;
            if crate::arena_session::pending(&self.root)? {
                crate::arena_session::resolve_ended(
                    &self.root,
                    &rows,
                    &j.session,
                    j.game
                        .as_ref()
                        .context("Arena receipt has no matching recorded game owner")?,
                )?;
            }
            if crate::ball_session::pending(&self.root)? {
                crate::ball_session::resolve_ended(
                    &self.root,
                    &rows,
                    &j.session,
                    j.game
                        .as_ref()
                        .context("Ball receipt has no matching recorded game owner")?,
                )?;
            }
            if crate::ball_gameplay_runtime::pending(&self.root)? {
                closed()?;
                crate::ball_gameplay_runtime::remove_ended(
                    &self.root,
                    j.game
                        .as_ref()
                        .context("Gameplay-ball log has no matching recorded game owner")?,
                    &j.session,
                    &rows,
                )?;
            }
            // Match the provider's outfit-last cleanup: a damaged memory-only
            // receipt must not strand verified loader/profile/camera files.
            if crate::outfit_journal::pending(&self.root)? {
                crate::outfit_journal::resolve_ended(
                    &self.root,
                    &j.session,
                    j.game
                        .as_ref()
                        .context("Outfit receipt has no matching recorded game owner")?,
                    &closed,
                )?;
            }
            // The provider has exited and exact physical files have already
            // been restored. Resolve only this launch's ended lease; its own
            // journal must match the saved target and exact game identity.
            // No native address is read/replayed during closed recovery.
            if crate::outfit_worker_v2_lease::pending(&self.root)? {
                crate::outfit_worker_v2_lease::resolve_ended_bound(
                    &self.root,
                    &j.session,
                    &game.join("NBA2K27.exe"),
                    j.game.as_ref(),
                    &closed,
                )?;
                ensure!(
                    !crate::outfit_worker_v2_lease::pending(&self.root)?,
                    "Another Version 2 outfit worker receipt is still pending; launch journal retained"
                );
            }
            // A corrupt memory-only intent must not strand an owned loader or
            // profile junction after the game and provider have exited.
            if self
                .root
                .join("Data")
                .join(crate::feature_journal::MARKER)
                .try_exists()?
            {
                crate::feature_journal::resolve_ended(
                    &self.root,
                    &j.session,
                    j.game
                        .as_ref()
                        .context("Feature intent has no matching game owner")?,
                    &closed,
                )?;
            }
            if crate::presentation_v2_runtime::pending(&self.root)? {
                closed()?;
                crate::presentation_v2_runtime::remove_ended(
                    &self.root,
                    j.game
                        .as_ref()
                        .context("Presentation log has no matching recorded game owner")?,
                    &j.session,
                    &rows,
                )?;
            }
            if crate::scoreboard_runtime::pending(&self.root)? {
                closed()?;
                crate::scoreboard_runtime::remove_ended(
                    &self.root,
                    j.game
                        .as_ref()
                        .context("Scoreboard log has no matching recorded game owner")?,
                    &j.session,
                    &rows,
                )?;
            }
        } else {
            ensure!(
                !mod_pending(&self.root)?,
                "Native mod recovery is not owned by this journal; left untouched"
            );
        }
        game_rules::resolve_ended(&self.root, &rows)?;
        crate::player_transaction::resolve_ended(&self.root, &rows)?;
        crate::team_transaction::resolve_ended(&self.root, &rows)?;
        crate::uniform_transaction::resolve_ended(&self.root, &rows)?;
        crate::roster_v2_write::resolve_ended(&self.root, &rows)?;
        self.rules = None;
        if let Some(mut j) = self.journal.clone() {
            j.phase = Phase::Closed;
            if modded(&j) {
                j.helpers.clear();
            }
            self.commit(j)?;
        }
        self.message =
            "Native launch monitoring cleared. Steam and the game were not stopped.".into();
        self.blacktop_enabled = false;
        self.blacktop = blacktop_native::idle();
        self.provider.take();
        Ok(self.view())
    }
}
struct Inner {
    controller: Mutex<Controller>,
    wake: Condvar,
    stopping: Mutex<bool>,
    launch_stopped: AtomicBool,
    launching: AtomicBool,
    owns_provider: AtomicBool,
}

struct LaunchProgress<'a>(&'a AtomicBool);
impl<'a> LaunchProgress<'a> {
    fn begin(flag: &'a AtomicBool) -> Result<Self> {
        ensure!(
            flag.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok(),
            "A game launch is already being prepared"
        );
        Ok(Self(flag))
    }
}
impl Drop for LaunchProgress<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
enum Catalog {
    Arena,
    Ball,
    Scoreboard,
}
impl Catalog {
    fn operation(&self) -> (&'static str, &'static str) {
        match self {
            Self::Arena => ("arena-select", "apply-one-arena-for-current-game"),
            Self::Ball => ("ball-select", "apply-one-ball-for-current-game"),
            Self::Scoreboard => ("scoreboard-select", "apply-one-scoreboard-for-current-game"),
        }
    }
}
pub struct Service {
    inner: Arc<Inner>,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
}
impl Service {
    pub fn outfit_command(
        &self,
        game: &Path,
        session: &str,
        command: crate::outfit_runtime::Command,
    ) -> Result<Value> {
        command.validate()?;
        let c = self
            .inner
            .controller
            .lock()
            .map_err(|_| anyhow::anyhow!("Native session unavailable"))?;
        let (identity, root) = c.observer_binding(&session_safety::processes()?)?;
        ensure!(
            same_path(&game.canonicalize()?.to_string_lossy(), &root),
            "Configured game folder changed"
        );
        let j = c.journal.as_ref().context("No launch session")?;
        let worker = c
            .provider
            .as_ref()
            .context("Cannot adopt an earlier provider; finish its game before retrying")?;
        arena_owner(j, &identity, session, &worker.identity)?;
        worker.control(json!({"operation":"outfit-command","acknowledgement":if command.writes(){"apply-one-outfit-for-current-game"}else{"read-blacktop-outfits"},"command":command}))
    }
    /// Retry cameras / Stop & restore for the exact launched game. Same owner
    /// proof as catalog selection; the worker's runner does the guarded work.
    pub fn camera_command(
        &self,
        game: &Path,
        session: &str,
        command: crate::feature_runtime::CameraCommand,
    ) -> Result<Value> {
        let c = self
            .inner
            .controller
            .lock()
            .map_err(|_| anyhow::anyhow!("Native session unavailable"))?;
        let (identity, root) = c.observer_binding(&session_safety::processes()?)?;
        ensure!(
            same_path(&game.canonicalize()?.to_string_lossy(), &root),
            "Configured game folder changed"
        );
        let j = c.journal.as_ref().context("No launch session")?;
        let worker = c
            .provider
            .as_ref()
            .context("Cannot adopt an earlier provider; finish its game before retrying")?;
        arena_owner(j, &identity, session, &worker.identity)?;
        worker.control(json!({"operation":"camera-command","acknowledgement":"control-cameras-for-current-game","command":command}))
    }
    pub fn select_arena(&self, game: &Path, session: &str, id: &str) -> Result<Value> {
        crate::blacktop_arena::choice(id)?;
        self.select_catalog(game, session, id, Catalog::Arena)
    }
    pub fn select_ball(&self, game: &Path, session: &str, id: &str) -> Result<Value> {
        crate::blacktop_ball::choice(id)?;
        self.select_catalog(game, session, id, Catalog::Ball)
    }
    pub fn select_scoreboard(&self, game: &Path, session: &str, id: &str) -> Result<Value> {
        crate::scoreboard_profile::choice(id)?;
        self.select_catalog(game, session, id, Catalog::Scoreboard)
    }
    fn select_catalog(
        &self,
        game: &Path,
        session: &str,
        id: &str,
        catalog: Catalog,
    ) -> Result<Value> {
        let c = self
            .inner
            .controller
            .lock()
            .map_err(|_| anyhow::anyhow!("Native session unavailable"))?;
        let (identity, root) = c.observer_binding(&session_safety::processes()?)?;
        ensure!(
            same_path(&game.canonicalize()?.to_string_lossy(), &root),
            "Configured game folder changed"
        );
        let j = c.journal.as_ref().context("No launch session")?;
        let worker = c
            .provider
            .as_ref()
            .context("Cannot adopt an earlier provider; finish its game before retrying")?;
        arena_owner(j, &identity, session, &worker.identity)?;
        let (operation, acknowledgement) = catalog.operation();
        worker
            .control(json!({"operation":operation,"assetId":id,"acknowledgement":acknowledgement}))
    }
    pub fn launch_modded(
        &self,
        game: &Path,
        mods: &Path,
        features: crate::feature_plan::Plan,
    ) -> Result<Value> {
        launch_allowed(&self.inner.launch_stopped)?;
        let _progress = LaunchProgress::begin(&self.inner.launching)?;
        // Lock order everywhere is controller, then the global write gate
        // (apply_rule, clear and the monitor). The reverse order here could
        // deadlock a Play click against a concurrent Rules apply or Clear.
        let mut controller = self
            .inner
            .controller
            .lock()
            .map_err(|_| anyhow::anyhow!("Native session unavailable"))?;
        let _gate = session_safety::write_gate()?;
        let result = controller.launch_modded(game, mods, features, &self.inner.launch_stopped);
        self.inner
            .owns_provider
            .store(controller.provider.is_some(), Ordering::Release);
        result
    }
    pub fn new(root: PathBuf) -> Result<Self> {
        let inner = Arc::new(Inner {
            controller: Mutex::new(Controller::load_reconciled(root, session_safety::processes)),
            wake: Condvar::new(),
            stopping: Mutex::new(false),
            launch_stopped: AtomicBool::new(false),
            launching: AtomicBool::new(false),
            owns_provider: AtomicBool::new(false),
        });
        let monitor = inner.clone();
        let worker = thread::Builder::new()
            .name("native-session-monitor".into())
            .spawn(move || {
                let mut reader = None;
                loop {
                    let stop = monitor.stopping.lock().unwrap_or_else(|e| e.into_inner());
                    let (stop, _) = monitor
                        .wake
                        .wait_timeout_while(stop, Duration::from_secs(1), |s| !*s)
                        .unwrap_or_else(|e| e.into_inner());
                    if *stop {
                        break;
                    }
                    drop(stop);
                    if let Ok(mut c) = monitor.controller.lock() {
                        if c.error.is_none()
                            && c.journal.as_ref().is_some_and(|j| j.phase != Phase::Closed)
                        {
                            if let Err(e) = c.observe(session_safety::processes()) {
                                c.error = Some(format!(
                                    "Native session persistence failed; held for review: {e:#}"
                                ));
                            }
                        }
                        c.observe_blacktop(&mut reader);
                        monitor
                            .owns_provider
                            .store(c.provider.is_some(), Ordering::Release);
                    }
                }
            })?;
        Ok(Self {
            inner,
            worker: Mutex::new(Some(worker)),
        })
    }
    /// Best-effort metadata snapshot for the exact launch's intro. Never wait
    /// behind startup work, contact the provider, or read game memory for a bar.
    pub(crate) fn intro_progress(&self, session: &str) -> Result<Option<Value>> {
        let Ok(c) = self.inner.controller.try_lock() else {
            return Ok(None);
        };
        let Some(j) = c.journal.as_ref().filter(|j| j.session == session) else {
            return Ok(None);
        };
        let phase = if c.error.is_some() {
            "needs-review"
        } else {
            match j.phase {
                Phase::Waiting => "waiting-for-game",
                Phase::Running => "game-running",
                Phase::Closed => "closed",
                Phase::CleanupPending => "needs-review",
            }
        };
        Ok(Some(
            json!({"session":session,"phase":phase,"error":c.error,"steamOnly":!modded(j),"nativeProvider":c.provider_view_with_live_status(false)?}),
        ))
    }
    pub fn status(&self) -> Result<Value> {
        if self.inner.launching.load(Ordering::Acquire) {
            // Bootstrap indexes the selected mod profile while holding the
            // controller. Status must remain responsive during that work.
            return Ok(
                json!({"phase":"waiting-for-game", "message":"Checking the game and indexing your selected mods. Keep the launcher open.",
                "error":null, "nativeProvider":{"phase":"preparing-mods", "message":"Checking the game and indexing your selected mods. Keep the launcher open."},
                "automaticFeaturesEnabled":false,"verifiedRuleWrites":0,"gameStoppedByLauncher":false}),
            );
        }
        Ok(self
            .inner
            .controller
            .lock()
            .map_err(|_| anyhow::anyhow!("Native session unavailable"))?
            .view())
    }
    pub fn launch(&self, game: &Path) -> Result<Value> {
        launch_allowed(&self.inner.launch_stopped)?;
        let _progress = LaunchProgress::begin(&self.inner.launching)?;
        let mut c = self
            .inner
            .controller
            .lock()
            .map_err(|_| anyhow::anyhow!("Native session unavailable"))?;
        launch_allowed(&self.inner.launch_stopped)?;
        let preflight = session_safety::preflight(game, &c.root);
        c.launch(game, preflight, session_safety::processes, |steam, url| {
            use std::os::windows::process::CommandExt;
            // Explicit executable and one validated argument. No shell/script, no job ownership of Steam.
            let mut command = std::process::Command::new(steam);
            command
                .arg(url)
                .current_dir(steam.parent().context("Steam parent missing")?)
                .creation_flags(0x08000000)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            let _steam_dispatch = dispatch_if_allowed(&self.inner.launch_stopped, || {
                command.spawn().context("Steam dispatch failed")
            })?;
            Ok(())
        })
    }
    pub fn clear(&self) -> Result<Value> {
        self.inner
            .controller
            .lock()
            .map_err(|_| anyhow::anyhow!("Native session unavailable"))?
            .clear(session_safety::processes())
    }
    pub fn blacktop_status(&self) -> Result<Value> {
        Ok(self
            .inner
            .controller
            .lock()
            .map_err(|_| anyhow::anyhow!("Native session unavailable"))?
            .blacktop
            .clone())
    }
    pub fn binding(&self, game: &Path) -> Result<(Identity, PathBuf)> {
        let c = self
            .inner
            .controller
            .lock()
            .map_err(|_| anyhow::anyhow!("Native session unavailable"))?;
        let (id, root) = c.observer_binding(&session_safety::processes()?)?;
        ensure!(
            same_path(&game.canonicalize()?.to_string_lossy(), &root),
            "Configured game folder changed"
        );
        Ok((id, root))
    }
    /// Owned launch binding for the Version 2 read-only roster backend. It
    /// comes only from this launcher's running journal, never browser input.
    pub(crate) fn roster_v2_binding(
        &self,
        game: &Path,
    ) -> Result<crate::roster_v2_access::Binding> {
        let c = self
            .inner
            .controller
            .lock()
            .map_err(|_| anyhow::anyhow!("Native session unavailable"))?;
        let (id, root) = c.observer_binding(&session_safety::processes()?)?;
        ensure!(
            same_path(&game.canonicalize()?.to_string_lossy(), &root),
            "Configured game folder changed"
        );
        let session = c
            .journal
            .as_ref()
            .context("Missing session")?
            .session
            .clone();
        crate::roster_v2_access::Binding::from_launched(
            id,
            session_safety::identity(std::process::id())?,
            root,
            session,
        )
    }
    /// Metadata-only binding for an intro frozen by this exact launch. A
    /// restart, old journal, later launch, failed owner or PID reuse cannot
    /// supply a game. No process memory is read or written here.
    pub(crate) fn intro_binding(&self, session: &str) -> Result<Option<(Identity, PathBuf)>> {
        let c = self
            .inner
            .controller
            .lock()
            .map_err(|_| anyhow::anyhow!("Native session unavailable"))?;
        let j = c.journal.as_ref().context("Startup video launch ended")?;
        if !intro_launch_phase(session, &j.session, &j.phase, c.error.is_some())? {
            return Ok(None);
        }
        let binding = c.observer_binding(&session_safety::processes()?)?;
        ensure!(
            j.game.as_ref() == Some(&binding.0),
            "Intro game identity changed"
        );
        Ok(Some(binding))
    }
    pub fn read_rules(&self, game: &Path) -> Result<Value> {
        use game_rules::Access;
        let mut c = self
            .inner
            .controller
            .lock()
            .map_err(|_| anyhow::anyhow!("Native session unavailable"))?;
        c.rules = None;
        ensure!(
            !game_rules::pending(&c.root)?,
            "Rule transaction needs review; close NBA 2K27 and clear finished launch"
        );
        let (id, root) = c.observer_binding(&session_safety::processes()?)?;
        ensure!(
            same_path(&game.canonicalize()?.to_string_lossy(), &root),
            "Configured game folder changed"
        );
        let session = c
            .journal
            .as_ref()
            .context("Missing session")?
            .session
            .clone();
        let version_two = crate::provider_access::verify_build(&root.join("NBA2K27.exe"))?.build["label"]
            == "Version 2";
        let snapshot = if version_two {
            let binding = crate::rules_v2_access::Binding::from_launched(
                id.clone(),
                session_safety::identity(std::process::id())?,
                root.clone(),
                c.root.clone(),
                session.clone(),
            )?;
            crate::rules_v2_access::RulesAccessV2::open(&binding)?.capture()?
        } else {
            blacktop_native::RulesAccess::open(&id, &root, false)?.capture()?
        };
        let lease = game_rules::Lease::new(id, session, snapshot);
        let view = if version_two {
            crate::rules_v2_access::lease_view(&lease)
        } else {
            lease.view()
        };
        c.rules = Some(lease);
        Ok(view)
    }
    pub fn apply_rule(&self, game: &Path, token: &str, field: &str, value: u32) -> Result<Value> {
        let mut c = self
            .inner
            .controller
            .lock()
            .map_err(|_| anyhow::anyhow!("Native session unavailable"))?;
        let _write_gate = session_safety::write_gate()?;
        let lease = c
            .rules
            .take()
            .context("Refresh current Game Rules before applying a change")?;
        ensure!(
            !crate::player_transaction::pending(&c.root)?,
            "Unresolved player transaction; further writes blocked"
        );
        ensure!(
            !crate::team_transaction::pending(&c.root)?,
            "Unresolved team transaction; further writes blocked"
        );
        ensure!(
            !crate::uniform_transaction::pending(&c.root)?,
            "Unresolved uniform transaction; further writes blocked"
        );
        ensure!(
            !game_rules::pending(&c.root)?,
            "Unresolved rule transaction; further writes blocked"
        );
        let (id, root) = c.observer_binding(&session_safety::processes()?)?;
        ensure!(
            same_path(&game.canonicalize()?.to_string_lossy(), &root),
            "Configured game folder changed"
        );
        let session = &c.journal.as_ref().context("Missing session")?.session;
        lease.validate(token, &id, session)?;
        if crate::provider_access::verify_build(&root.join("NBA2K27.exe"))?.build["label"]
            == "Version 2"
        {
            let binding = crate::rules_v2_access::Binding::from_launched(
                id.clone(),
                session_safety::identity(std::process::id())?,
                root.clone(),
                c.root.clone(),
                session.clone(),
            )?;
            crate::rules_v2_access::RulesAccessV2::open(&binding)?.apply_under_gate(
                &lease,
                token,
                field,
                value,
                &_write_gate,
            )?;
        } else {
            let access = blacktop_native::RulesAccess::open(&id, &root, true)?;
            game_rules::apply(&access, &lease, field, value, |i| {
                backend::atomic_json(&game_rules::path(&c.root), i)
            })?;
        }
        c.rule_writes = c.rule_writes.saturating_add(1);
        // The lease is one-use. User refreshes independently before another transaction.
        Ok(
            json!({"applied":true,"field":field,"value":value,"message":"Rule applied and read back; all neighboring rules unchanged. Refresh current values before another change."}),
        )
    }
    pub fn enable_blacktop(&self, game: &Path) -> Result<Value> {
        let mut c = self
            .inner
            .controller
            .lock()
            .map_err(|_| anyhow::anyhow!("Native session unavailable"))?;
        let root = &c
            .journal
            .as_ref()
            .and_then(|j| j.launch.as_ref())
            .context("No launch context")?
            .game_root;
        ensure!(
            same_path(root, &game.canonicalize()?),
            "Configured game folder differs from this launch session"
        );
        let result = c.enable_observer(session_safety::processes());
        self.inner.wake.notify_all();
        result
    }
    pub fn disable_blacktop(&self) -> Result<Value> {
        let mut c = self
            .inner
            .controller
            .lock()
            .map_err(|_| anyhow::anyhow!("Native session unavailable"))?;
        c.blacktop_enabled = false;
        c.blacktop = blacktop_native::idle();
        Ok(c.blacktop.clone())
    }
    /// Nonblocking: close requests must cancel launch admission even while a
    /// launch holds the controller mutex for validation or provider indexing.
    pub fn stop_new_launches(&self) {
        self.inner.launch_stopped.store(true, Ordering::Release);
    }
    pub fn should_keep_open(&self) -> bool {
        if self.inner.launching.load(Ordering::Acquire) {
            return true;
        }
        match self.inner.controller.try_lock() {
            Ok(mut controller) => controller
                .provider
                .as_mut()
                .is_some_and(|worker| !worker.exited().unwrap_or(false)),
            Err(_) => self.inner.owns_provider.load(Ordering::Acquire),
        }
    }
    pub fn shutdown(&self) {
        self.stop_new_launches();
        *self
            .inner
            .stopping
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = true;
        self.inner.wake.notify_all();
        if let Some(worker) = self.worker.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = worker.join();
        }
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.shutdown()
    }
}

pub fn smoke_lifecycle(root: &Path) -> Result<Value> {
    let fixture = root.join("Cache/NativeSessionQA");
    fs::create_dir_all(fixture.join("Data"))?;
    let report = session_safety::smoke_fixture(&fixture)?;
    let game = fixture.join("Cache/SessionQA/Steam/steamapps/common/NBA 2K27");
    let mut c = Controller::load(fixture.clone());
    let mut sent = 0;
    c.launch(
        &game,
        report,
        || Ok(vec![]),
        |_, url| {
            ensure!(url == "steam://launch/123456/option1", "Wrong option");
            sent += 1;
            Ok(())
        },
    )?;
    let waiting = session_safety::read_journal(&c.path())?;
    ensure!(waiting.phase == Phase::Waiting, "Intent not durable");
    let i = Identity {
        pid: 123,
        started: (waiting.launch.as_ref().unwrap().requested_ms * 10_000 + 116_444_736_000_010_000)
            .to_string(),
        path: game
            .join("NBA2K27.exe")
            .canonicalize()?
            .to_string_lossy()
            .into_owned(),
    };
    c.observe(Ok(vec![Process {
        name: "NBA2K27.exe".into(),
        pid: i.pid,
        parent: 1,
        identity: Some(i),
    }]))?;
    ensure!(
        session_safety::read_journal(&c.path())?.phase == Phase::Running,
        "Running phase not durable"
    );
    c.observe(Err(anyhow::anyhow!("QA unknown")))?;
    ensure!(
        session_safety::read_journal(&c.path())?.phase == Phase::Running,
        "Unknown cleared session"
    );
    c.observe(Ok(vec![]))?;
    ensure!(
        session_safety::read_journal(&c.path())?.phase == Phase::Closed,
        "Ended phase not durable"
    );
    let service = Service::new(fixture.clone())?;
    let start = std::time::Instant::now();
    service.shutdown();
    let shutdown_ms = start.elapsed().as_secs_f64() * 1000.;
    Ok(
        json!({"pass":true,"fixture":true,"steamDispatches":sent,"gameFeaturesEnabled":false,"realGameLaunched":false,"monitorShutdownMs":shutdown_ms}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stopped_service_rejects_both_launches_without_waiting_for_controller() {
        let root = fixture();
        let service = Service::new(root.clone()).unwrap();
        // CloseRequested must not wait behind a long provider bootstrap.
        let held = service.inner.controller.lock().unwrap();
        service.stop_new_launches();
        service.stop_new_launches();
        let absent = root.join("must-not-be-inspected/NBA 2K27");
        for result in [
            service.launch(&absent),
            service.launch_modded(&absent, &absent, Default::default()),
        ] {
            assert!(result.unwrap_err().to_string().contains("closing"));
        }
        assert!(!held.path().exists());
        drop(held);
        service.shutdown();
        assert!(service.inner.launch_stopped.load(Ordering::Acquire));
        assert!(
            service
                .launch(&absent)
                .unwrap_err()
                .to_string()
                .contains("closing")
        );
        drop(service);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn steam_cancellation_after_durable_intent_never_calls_inert_dispatch_and_closes_record() {
        let root = fixture();
        let mut controller = Controller::load(root.clone());
        let stopped = AtomicBool::new(false);
        let snapshots = std::cell::Cell::new(0);
        let dispatched = std::cell::Cell::new(0);
        let result = controller.launch(
            &game(&root),
            session_safety::smoke_fixture(&root).unwrap(),
            || {
                snapshots.set(snapshots.get() + 1);
                if snapshots.get() == 2 {
                    assert_eq!(
                        session_safety::read_journal(&root.join("Data/session-journal.json"))
                            .unwrap()
                            .phase,
                        Phase::Waiting
                    );
                    stopped.store(true, Ordering::Release);
                }
                Ok(vec![])
            },
            |_, _| {
                dispatch_if_allowed(&stopped, || {
                    dispatched.set(dispatched.get() + 1);
                    Ok(())
                })
            },
        );
        assert!(result.unwrap_err().to_string().contains("closing"));
        assert_eq!(snapshots.get(), 2);
        assert_eq!(dispatched.get(), 0);
        assert_eq!(
            session_safety::read_journal(&controller.path())
                .unwrap()
                .phase,
            Phase::Closed
        );
        assert!(controller.journal.as_ref().unwrap().helpers.is_empty());
        drop(controller);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn cancelled_prepared_dispatch_drops_inert_helper_and_leaves_clearable_owned_record() {
        struct InertPrepared<'a>(&'a std::cell::Cell<u32>);
        impl Drop for InertPrepared<'_> {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        let root = fixture();
        let mut controller = modded_fixture(&root);
        let original = controller.journal.clone().unwrap();
        let stopped = AtomicBool::new(false);
        let dropped = std::cell::Cell::new(0);
        let attempted = std::cell::Cell::new(0);
        let prepared = InertPrepared(&dropped);
        // Deterministic stand-in for a close during Worker::start_owned.
        stopped.store(true, Ordering::Release);
        assert!(
            dispatch_if_allowed(&stopped, || {
                attempted.set(attempted.get() + 1);
                Ok(())
            })
            .is_err()
        );
        assert_eq!(attempted.get(), 0);
        // A failed/unknown stop must not close or erase this exact receipt.
        assert_eq!(
            serde_json::to_value(session_safety::read_journal(&controller.path()).unwrap())
                .unwrap(),
            serde_json::to_value(&original).unwrap()
        );
        drop(prepared);
        assert_eq!(dropped.get(), 1);
        controller.close_unstarted_provider().unwrap();
        let closed = session_safety::read_journal(&controller.path()).unwrap();
        assert_eq!(closed.session, original.session);
        assert_eq!(closed.phase, Phase::Closed);
        assert!(closed.helpers.is_empty());
        assert_eq!(
            serde_json::to_value(closed.launch).unwrap(),
            serde_json::to_value(original.launch).unwrap()
        );
        drop(controller);
        let recovered = Controller::load(root.clone());
        assert!(recovered.error.is_none());
        assert_eq!(recovered.journal.as_ref().unwrap().phase, Phase::Closed);
        assert!(recovered.journal.as_ref().unwrap().helpers.is_empty());
        drop(recovered);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn cancelled_modded_admission_does_not_prepare_or_create_a_journal() {
        let root = fixture();
        let mut controller = Controller::load(root.clone());
        let stopped = AtomicBool::new(true);
        let absent = root.join("must-not-be-inspected");
        let result = controller.launch_modded(&absent, &absent, Default::default(), &stopped);
        assert!(result.unwrap_err().to_string().contains("closing"));
        assert!(!controller.path().exists());
        drop(controller);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn intro_binding_never_crosses_a_launch_or_failed_closed_session() {
        let session = "a".repeat(48);
        assert!(!intro_launch_phase(&session, &session, &Phase::Waiting, false).unwrap());
        assert!(intro_launch_phase(&session, &session, &Phase::Running, false).unwrap());
        for phase in [Phase::Closed, Phase::CleanupPending] {
            assert!(intro_launch_phase(&session, &session, &phase, false).is_err());
        }
        assert!(intro_launch_phase(&session, &"b".repeat(48), &Phase::Running, false).is_err());
        assert!(intro_launch_phase(&session, &session, &Phase::Running, true).is_err());
        assert!(intro_launch_phase("old-pid", "old-pid", &Phase::Running, false).is_err());
    }
    include!("native_cleanup_order_tests.rs");
    include!("native_outfit_v2_recovery_tests.rs");
    #[test]
    fn startup_reconciles_only_an_ended_modded_session_without_remaining_resources() {
        for case in 0..5 {
            let root = fixture();
            let c = modded_fixture(&root);
            let owner = c.journal.as_ref().unwrap().helpers[0].identity.clone();
            let original = fs::read(c.path()).unwrap();
            drop(c);
            if case == 3 {
                fs::write(root.join("Data/loader-session.json"), b"unknown receipt").unwrap();
            }
            let c = Controller::load_reconciled(root.clone(), || match case {
                1 => Err(anyhow::anyhow!("process scan unavailable")),
                2 => Ok(vec![Process {
                    name: "2k Launcher Rust Prototype.exe".into(),
                    pid: owner.pid,
                    parent: 1,
                    identity: Some(owner),
                }]),
                4 => Ok(vec![Process {
                    name: "NBA2K27.exe".into(),
                    pid: 9,
                    parent: 1,
                    identity: None,
                }]),
                _ => Ok(vec![]),
            });
            if case == 0 {
                assert_eq!(c.journal.as_ref().unwrap().phase, Phase::Closed);
                assert!(c.journal.as_ref().unwrap().helpers.is_empty());
                assert_eq!(
                    session_safety::read_journal(&c.path()).unwrap().phase,
                    Phase::Closed
                );
            } else if case == 3 {
                assert_eq!(c.journal.as_ref().unwrap().phase, Phase::CleanupPending);
                assert_eq!(
                    fs::read(root.join("Data/loader-session.json")).unwrap(),
                    b"unknown receipt"
                );
            } else {
                assert_eq!(fs::read(c.path()).unwrap(), original);
            }
            drop(c);
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn bootstrap_status_does_not_wait_for_the_controller_and_progress_clears_on_error() {
        let root = fixture();
        let service = Service::new(root.clone()).unwrap();
        {
            let _held = service.inner.controller.lock().unwrap();
            assert!(
                !service.should_keep_open(),
                "A reopened/read-only window is not an owner"
            );
        }
        let progress = LaunchProgress::begin(&service.inner.launching).unwrap();
        assert!(service.should_keep_open());
        assert!(LaunchProgress::begin(&service.inner.launching).is_err());
        let held = service.inner.controller.lock().unwrap();
        let value = service.status().unwrap();
        assert_eq!(value["nativeProvider"]["phase"], "preparing-mods");
        assert_eq!(value["gameStoppedByLauncher"], false);
        drop(held);
        drop(progress);
        assert!(!service.inner.launching.load(Ordering::Acquire));
        assert_eq!(service.status().unwrap()["phase"], "idle");
        drop(service);
        fs::remove_dir_all(root).unwrap();
    }
    fn fixture() -> PathBuf {
        let root = std::env::temp_dir().join(format!("native-session-{}", backend::unique_id()));
        fs::create_dir_all(root.join("Data")).unwrap();
        session_safety::smoke_fixture(&root).unwrap();
        root
    }
    fn game(root: &Path) -> PathBuf {
        root.join("Cache/SessionQA/Steam/steamapps/common/NBA 2K27")
    }
    #[test]
    fn durable_launch_running_unknown_and_exit() {
        let root = fixture();
        assert_eq!(smoke_lifecycle(&root).unwrap()["steamDispatches"], 1);
        fs::remove_dir_all(root).unwrap();
    }
    fn modded_fixture(root: &Path) -> Controller {
        let mut c = Controller::load(root.into());
        c.launch(
            &game(root),
            session_safety::smoke_fixture(root).unwrap(),
            || Ok(vec![]),
            |_, _| Ok(()),
        )
        .unwrap();
        let mut j = c.journal.clone().unwrap();
        j.launch.as_mut().unwrap().kind = "native-modded-qa".into();
        j.helpers = vec![session_safety::HelperLease {
            session: j.session.clone(),
            role: "provider".into(),
            identity: Identity {
                pid: 4242,
                started: "1234".into(),
                path: root.join("App/provider.exe").to_string_lossy().into(),
            },
            binary_sha256: "a".repeat(64),
        }];
        c.commit(j).unwrap();
        c
    }
    #[test]
    fn arena_requests_cannot_cross_session_game_or_worker_identity() {
        let root = fixture();
        let c = modded_fixture(&root);
        let mut j = c.journal.clone().unwrap();
        let id = Identity {
            pid: 123,
            started: "133000000000000000".into(),
            path: game(&root).join("NBA2K27.exe").to_string_lossy().into(),
        };
        j.phase = Phase::Running;
        j.game = Some(id.clone());
        let worker = j.helpers[0].identity.clone();
        arena_owner(&j, &id, &j.session, &worker).unwrap();
        assert!(arena_owner(&j, &id, &"b".repeat(48), &worker).is_err());
        let mut reused = id.clone();
        reused.started.push('1');
        assert!(arena_owner(&j, &reused, &j.session, &worker).is_err());
        let mut reused_worker = worker.clone();
        reused_worker.started.push('1');
        assert!(arena_owner(&j, &id, &j.session, &reused_worker).is_err());
        for phase in [Phase::Closed, Phase::CleanupPending] {
            let mut other = j.clone();
            other.phase = phase;
            assert!(arena_owner(&other, &id, &j.session, &worker).is_err());
        }
        let mut other = j.clone();
        other.helpers[0].role = "probe".into();
        assert!(arena_owner(&other, &id, &j.session, &worker).is_err());
        let mut other = j.clone();
        other.helpers[0].session = "b".repeat(48);
        assert!(arena_owner(&other, &id, &j.session, &worker).is_err());
        let mut other = j.clone();
        other.launch.as_mut().unwrap().kind = "steam-only-qa".into();
        assert!(arena_owner(&other, &id, &j.session, &worker).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn orphaned_arena_receipt_keeps_launch_and_clear_blocked_without_mutating_it() {
        let root = fixture();
        let path = root.join("Data/blacktop-arena-session.json");
        fs::write(&path, b"unknown arena owner").unwrap();
        assert!(mod_pending(&root).unwrap());
        let mut c = Controller::load(root.clone());
        assert!(c.clear(Ok(vec![])).is_err());
        let report = session_safety::smoke_fixture(&root).unwrap();
        assert_eq!(report["recoveryRequired"], true);
        assert_eq!(report["checksPassed"], false);
        assert_eq!(fs::read(&path).unwrap(), b"unknown arena owner");
        assert!(
            c.launch(
                &game(&root),
                report,
                || Ok(vec![]),
                |_, _| anyhow::bail!("Must not dispatch")
            )
            .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), b"unknown arena owner");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn orphaned_ball_receipt_blocks_launch_and_is_never_adopted_or_deleted() {
        let root = fixture();
        let path = root.join("Data/blacktop-ball-session.json");
        fs::write(&path, b"unknown ball owner").unwrap();
        assert!(mod_pending(&root).unwrap());
        let mut c = Controller::load(root.clone());
        assert!(c.clear(Ok(vec![])).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"unknown ball owner");
        drop(c);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn orphaned_outfit_receipt_blocks_launch_and_recovery_never_guesses_ownership() {
        let root = fixture();
        let path = root.join("Data").join(crate::outfit_journal::MARKER);
        fs::write(&path, b"unknown outfit owner").unwrap();
        assert!(mod_pending(&root).unwrap());
        let mut c = Controller::load(root.clone());
        assert!(c.clear(Ok(vec![])).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"unknown outfit owner");
        let report = session_safety::smoke_fixture(&root).unwrap();
        assert_eq!(report["recoveryRequired"], true);
        assert_eq!(report["checksPassed"], false);
        assert!(
            c.launch(
                &game(&root),
                report,
                || Ok(vec![]),
                |_, _| anyhow::bail!("Must not dispatch")
            )
            .is_err()
        );
        drop(c);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn modded_restart_retains_a_live_recorded_worker_and_never_closes_on_game_absence_alone() {
        let root = fixture();
        let c = modded_fixture(&root);
        let identity = c.journal.as_ref().unwrap().helpers[0].identity.clone();
        drop(c);
        let mut c = Controller::load(root.clone());
        assert!(c.error.is_none());
        c.observe(Ok(vec![Process {
            name: "2k Launcher Rust Prototype.exe".into(),
            pid: identity.pid,
            parent: 1,
            identity: Some(identity),
        }]))
        .unwrap();
        assert_eq!(c.journal.as_ref().unwrap().phase, Phase::Waiting);
        assert!(c.message.contains("still owns"));
        c.observe(Ok(vec![])).unwrap();
        assert_eq!(c.journal.as_ref().unwrap().phase, Phase::Closed);
        assert!(c.journal.as_ref().unwrap().helpers.is_empty());
        assert_eq!(c.view()["steamOnly"], false);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn malformed_modded_recovery_blocks_new_launch_and_cannot_be_cleared_blindly() {
        let root = fixture();
        let mut c = modded_fixture(&root);
        let path = root.join("Data/loader-session.json");
        fs::write(&path, b"foreign or damaged").unwrap();
        c.observe(Ok(vec![])).unwrap();
        assert_eq!(c.journal.as_ref().unwrap().phase, Phase::CleanupPending);
        assert!(c.clear(Ok(vec![])).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"foreign or damaged");
        let report = session_safety::smoke_fixture(&root).unwrap();
        assert_eq!(report["recoveryRequired"], true);
        assert_eq!(report["checksPassed"], false);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn orphaned_player_folder_receipt_blocks_launch_and_is_never_cleared_without_an_owner() {
        for marker in [
            crate::player_folders::MARKER,
            crate::selector_files::MARKER,
            crate::camera_name_files::MARKER,
            crate::feature_journal::MARKER,
            crate::presentation_v2_runtime::MARKER,
            crate::scoreboard_runtime::MARKER,
            crate::ball_gameplay_runtime::MARKER,
            crate::jukebox_files::MARKER,
        ] {
            let root = fixture();
            let mut c = Controller::load(root.clone());
            let path = root.join("Data").join(marker);
            fs::write(&path, b"foreign or damaged player receipt").unwrap();
            let report = session_safety::smoke_fixture(&root).unwrap();
            assert_eq!(report["recoveryRequired"], true);
            assert_eq!(report["checksPassed"], false);
            assert!(c.clear(Ok(vec![])).is_err());
            assert_eq!(
                fs::read(path).unwrap(),
                b"foreign or damaged player receipt"
            );
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn unknown_recorded_worker_identity_holds_clear_and_native_provider_status_is_session_bound() {
        let root = fixture();
        let mut c = modded_fixture(&root);
        let j = c.journal.as_ref().unwrap().clone();
        assert!(
            c.clear(Ok(vec![Process {
                name: "2k Launcher Rust Prototype.exe".into(),
                pid: 4242,
                parent: 1,
                identity: None
            }]))
            .is_err()
        );
        let path = root
            .join("Logs/native-sessions")
            .join(&j.session)
            .join("provider-state.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        backend::atomic_json(&path,&json!({"schema":"nba2k27-native-provider-session-v1","session":j.session,"workerPid":4242,"phase":"loader-active"})).unwrap();
        assert_eq!(c.provider_view().unwrap()["phase"], "loader-active");
        backend::atomic_json(&path,&json!({"schema":"nba2k27-native-provider-session-v1","session":j.session,"workerPid":5555,"phase":"loader-active"})).unwrap();
        assert!(c.provider_view().is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn no_launch_on_save_failure_or_unknown_and_retry_after_dispatch_failure() {
        let root = fixture();
        let g = game(&root);
        let report = session_safety::smoke_fixture(&root).unwrap();
        let mut c = Controller::load(root.clone());
        let sent = std::cell::Cell::new(0);
        assert!(
            c.launch(
                &g,
                report.clone(),
                || anyhow::bail!("unknown"),
                |_, _| {
                    sent.set(sent.get() + 1);
                    Ok(())
                }
            )
            .is_err()
        );
        fs::create_dir_all(c.path()).unwrap();
        assert!(
            c.launch(
                &g,
                report.clone(),
                || Ok(vec![]),
                |_, _| {
                    sent.set(sent.get() + 1);
                    Ok(())
                }
            )
            .is_err()
        );
        assert_eq!(sent.get(), 0);
        fs::remove_dir(c.path()).unwrap();
        assert!(
            c.launch(
                &g,
                report.clone(),
                || Ok(vec![]),
                |_, _| anyhow::bail!("spawn failed")
            )
            .is_err()
        );
        assert_eq!(
            session_safety::read_journal(&c.path()).unwrap().phase,
            Phase::Closed
        );
        c.launch(&g, report, || Ok(vec![]), |_, _| Ok(())).unwrap();
        assert!(
            c.launch(
                &g,
                session_safety::smoke_fixture(&root).unwrap(),
                || Ok(vec![]),
                |_, _| Ok(())
            )
            .is_err()
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn restarted_controller_recovery_and_identity_reuse() {
        let root = fixture();
        let g = game(&root);
        let mut c = Controller::load(root.clone());
        c.launch(
            &g,
            session_safety::smoke_fixture(&root).unwrap(),
            || Ok(vec![]),
            |_, _| Ok(()),
        )
        .unwrap();
        let mut c = Controller::load(root.clone());
        let request = c
            .journal
            .as_ref()
            .unwrap()
            .launch
            .as_ref()
            .unwrap()
            .requested_ms;
        let mut i = Identity {
            pid: 5,
            started: (request * 10_000 + 116_444_736_000_010_000).to_string(),
            path: g
                .join("NBA2K27.exe")
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        };
        i.path = r"C:\Other\NBA2K27.exe".into();
        c.observe(Ok(vec![Process {
            name: "NBA2K27.exe".into(),
            pid: 5,
            parent: 1,
            identity: Some(i.clone()),
        }]))
        .unwrap();
        assert!(c.journal.as_ref().unwrap().game.is_none());
        i.path = g
            .join("NBA2K27.exe")
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        c.observe(Ok(vec![Process {
            name: "NBA2K27.exe".into(),
            pid: 5,
            parent: 1,
            identity: Some(i.clone()),
        }]))
        .unwrap();
        assert_eq!(c.journal.as_ref().unwrap().phase, Phase::Running);
        i.started = (i.started.parse::<u64>().unwrap() + 1).to_string();
        c.observe(Ok(vec![Process {
            name: "NBA2K27.exe".into(),
            pid: 5,
            parent: 1,
            identity: Some(i.clone()),
        }]))
        .unwrap();
        assert!(c.message.contains("identity changed"));
        assert!(
            c.clear(Ok(vec![Process {
                name: "NBA2K27.exe".into(),
                pid: 5,
                parent: 1,
                identity: Some(i)
            }]))
            .is_err()
        );
        c.clear(Ok(vec![])).unwrap();
        assert_eq!(c.journal.as_ref().unwrap().phase, Phase::Closed);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn dispatch_rechecks_after_intent_and_closes_on_new_conflict() {
        let root = fixture();
        let g = game(&root);
        for name in [
            "EasyAntiCheat_EOS.exe",
            "start_protected_game.exe",
            "2k Launcher WPF.exe",
            "2k Launcher Rust Prototype.exe",
        ] {
            let mut c = Controller::load(root.clone());
            let checks = std::cell::Cell::new(0);
            let sent = std::cell::Cell::new(false);
            let result = c.launch(
                &g,
                session_safety::smoke_fixture(&root).unwrap(),
                || {
                    checks.set(checks.get() + 1);
                    Ok(if checks.get() == 1 {
                        vec![]
                    } else {
                        vec![Process {
                            name: name.into(),
                            pid: 99,
                            parent: 1,
                            identity: None,
                        }]
                    })
                },
                |_, _| {
                    sent.set(true);
                    Ok(())
                },
            );
            assert!(result.is_err());
            assert!(!sent.get());
            assert_eq!(
                session_safety::read_journal(&c.path()).unwrap().phase,
                Phase::Closed
            );
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn observer_requires_own_running_session_and_resets_on_restart_and_exit() {
        let root = fixture();
        let g = game(&root);
        let mut c = Controller::load(root.clone());
        assert!(c.enable_observer(Ok(vec![])).is_err());
        c.launch(
            &g,
            session_safety::smoke_fixture(&root).unwrap(),
            || Ok(vec![]),
            |_, _| Ok(()),
        )
        .unwrap();
        assert!(c.enable_observer(Ok(vec![])).is_err());
        let id = Identity {
            pid: 5,
            started: (c
                .journal
                .as_ref()
                .unwrap()
                .launch
                .as_ref()
                .unwrap()
                .requested_ms
                * 10_000
                + 116_444_736_000_010_000)
                .to_string(),
            path: g
                .join("NBA2K27.exe")
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .into(),
        };
        let rows = || {
            vec![Process {
                name: "NBA2K27.exe".into(),
                pid: 5,
                parent: 1,
                identity: Some(id.clone()),
            }]
        };
        c.observe(Ok(rows())).unwrap();
        assert!(c.enable_observer(Err(anyhow::anyhow!("Unknown"))).is_err());
        c.enable_observer(Ok(rows())).unwrap();
        assert!(c.blacktop_enabled);
        let restarted = Controller::load(root.clone());
        assert!(!restarted.blacktop_enabled);
        assert_eq!(restarted.blacktop["phase"], "disabled");
        c.observe(Ok(vec![])).unwrap();
        let mut reader = None;
        c.observe_blacktop(&mut reader);
        assert!(!c.blacktop_enabled);
        assert_eq!(c.blacktop["phase"], "ended");
        assert!(reader.is_none());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn unsupported_journal_preserved_and_monitor_shutdown_prompt() {
        let root = fixture();
        fs::write(root.join("Data/session-journal.json"), b"{broken").unwrap();
        let service = Service::new(root.clone()).unwrap();
        assert_eq!(service.status().unwrap()["phase"], "needs-review");
        assert!(service.clear().is_err());
        service.shutdown();
        assert!(service.worker.lock().unwrap().is_none());
        service.shutdown(); // Repeated shutdown is safe; timing measured in packaged smoke.
        assert_eq!(
            fs::read(root.join("Data/session-journal.json")).unwrap(),
            b"{broken"
        );
        drop(service);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn transient_status_reads_retry_but_corrupt_owner_and_persistence_errors_stay_terminal() {
        let transient =
            TransientProviderStatusIo(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
        assert!(
            provider_status_observation(Err(transient.into()))
                .unwrap()
                .is_none()
        );
        for error in [
            anyhow::anyhow!("Provider status owner differs"),
            anyhow::anyhow!("corrupt JSON"),
            anyhow::Error::from(std::io::Error::from(std::io::ErrorKind::PermissionDenied)),
        ] {
            assert!(provider_status_observation(Err(error)).is_err());
        }
        assert_eq!(
            provider_status_observation(Ok(json!({"phase":"ready"})))
                .unwrap()
                .unwrap()["phase"],
            "ready"
        );
    }
    #[test]
    fn independent_cleanup_preserves_all_errors_and_does_not_skip_later_owned_stores() {
        let mut errors = Vec::new();
        let calls = std::cell::RefCell::new(Vec::new());
        cleanup_step(&mut errors, "loader", || {
            calls.borrow_mut().push("loader");
            anyhow::bail!("held loader")
        });
        cleanup_step(&mut errors, "player", || {
            calls.borrow_mut().push("player");
            anyhow::bail!("missing source")
        });
        cleanup_step(&mut errors, "junction", || {
            calls.borrow_mut().push("junction");
            Ok(())
        });
        assert_eq!(*calls.borrow(), vec!["loader", "player", "junction"]);
        let error = format!("{:#}", cleanup_result(errors).unwrap_err());
        assert!(error.contains("held loader") && error.contains("missing source"));
    }

    #[test]
    fn physical_receipt_root_consensus_never_adopts_disagreeing_roots_after_profile_removal() {
        let game = PathBuf::from(r"C:\Game");
        assert_eq!(root_consensus(&[], &game).unwrap(), game.join("mods"));
        let selected = PathBuf::from(r"D:\SelectedProfile");
        assert_eq!(
            root_consensus(&[selected.clone(), selected.clone()], &game).unwrap(),
            selected
        );
        assert!(root_consensus(&[selected, PathBuf::from(r"D:\ForeignProfile")], &game).is_err());
    }
}
