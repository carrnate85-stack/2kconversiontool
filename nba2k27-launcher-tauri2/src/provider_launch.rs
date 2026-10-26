//! Provider-owned modded QA launch coordinator. The ready/authorized-start
//! handshake precedes ALL game mutations.
#![allow(dead_code)]
use crate::{
    arena_runtime, backend, ball_runtime, loader_files, mod_profile,
    provider_access::Gate,
    provider_assets::Service,
    provider_pipe::Server,
    session_safety::{self, Identity, Process},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    io::Read,
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
#[path = "provider_launch_parallel.rs"]
mod parallel;
#[path = "startup_readiness_v2.rs"]
mod readiness;
#[path = "provider_launch_timing.rs"]
mod timing;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Owner {
    pub portable: PathBuf,
    pub session: String,
}
#[derive(Clone)]
pub(crate) struct ContextData {
    pub owner: Owner,
    pub parent: Identity,
    pub game_exe: PathBuf,
    pub mods_root: PathBuf,
    pub features: crate::feature_plan::Plan,
}
/// A V2 payload is admitted separately. Never broaden the original DLL hash,
/// loader schema or legacy native-write gate to install the new worker.
enum LaunchLoader {
    Original(loader_files::Store),
    OutfitV2(crate::outfit_worker_v2_loader::Store),
}
impl LaunchLoader {
    fn create(
        v2: bool,
        root: &Path,
        game: &Path,
        session: &str,
        config: &[u8],
        check: impl Fn() -> Result<()>,
    ) -> Result<Self> {
        if v2 {
            Ok(Self::OutfitV2(
                crate::outfit_worker_v2_loader::Store::create(root, game, session, config, check)?,
            ))
        } else {
            Ok(Self::Original(loader_files::Store::create(
                root, game, session, config, check,
            )?))
        }
    }
    fn install_loader(&mut self, config: &[u8], check: impl Fn() -> Result<()>) -> Result<()> {
        match self {
            Self::Original(s) => s.install_loader(config, check),
            Self::OutfitV2(s) => s.install_loader(config, check),
        }
    }
    fn trigger(&mut self, check: impl Fn() -> Result<()>) -> Result<()> {
        match self {
            Self::Original(s) => s.trigger(check),
            Self::OutfitV2(s) => s.trigger(check),
        }
    }
    fn restore(&mut self, closed: impl Fn() -> Result<()>) -> Result<()> {
        match self {
            Self::Original(s) => s.restore(closed),
            Self::OutfitV2(s) => s.restore(closed),
        }
    }
}
fn command_name(command: crate::feature_runtime::CameraCommand) -> &'static str {
    match command {
        crate::feature_runtime::CameraCommand::Retry => "retry",
        crate::feature_runtime::CameraCommand::Stop => "stop",
    }
}
pub(crate) fn camera_command_allowed(
    command: crate::feature_runtime::CameraCommand,
    phase: &str,
    retryable: bool,
) -> bool {
    match command {
        crate::feature_runtime::CameraCommand::Retry => {
            phase == "stopped" || (phase == "held" && retryable)
        }
        crate::feature_runtime::CameraCommand::Stop => {
            matches!(phase, "waiting" | "active" | "reload" | "reset")
        }
    }
}
/// Hand a finished in-session mixed WNBA roster build to the provider. Its
/// forced OVERRIDE reads stock until this publication succeeds.
fn publish_mixed_roster(assets: &Service, runner: &mut crate::feature_runtime::Runner) {
    if let Some(prepared) = runner.take_mixed_roster() {
        match assets.publish_wnba_roster(&prepared) {
            Ok(()) => runner.mixed_roster_published(&prepared.summary),
            Err(error) => runner.mixed_roster_publication_failed(&error),
        }
    }
}
fn version_two_outfits(build: &Value) -> bool {
    build["label"] == "Version 2"
        && build["modProviderMapped"] == true
        && build["version2OutfitCarryMapped"] == true
}
impl ContextData {
    fn game(&self) -> Result<&Path> {
        self.game_exe.parent().context("Game parent missing")
    }
    fn status_path(&self) -> PathBuf {
        self.owner
            .portable
            .join("Logs/native-sessions")
            .join(&self.owner.session)
            .join("provider-state.json")
    }
    fn validate(&self) -> Result<()> {
        self.features.validate()?;
        ensure!(
            self.owner.portable.is_absolute()
                && self.game_exe.is_absolute()
                && self.mods_root.is_absolute()
                && self.owner.session.len() == 48
                && self.owner.session.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid immutable provider launch owner"
        );
        ensure!(
            !self
                .owner
                .portable
                .join("App/2k Launcher WPF.exe")
                .is_file()
                && !self.owner.portable.join("App/2k Launcher.exe").is_file(),
            "Keep native QA Data/Library isolated from Main"
        );
        Ok(())
    }
}
pub(crate) struct Control {
    context: Option<ContextData>,
    started: AtomicBool,
    queued: AtomicBool,
    pending: AtomicBool,
    cancelled: AtomicBool,
    dispatched: AtomicBool,
    state: Mutex<Value>,
    build: Mutex<Value>,
    arena: arena_runtime::Queue,
    ball: ball_runtime::Queue,
    scoreboard: crate::scoreboard_queue::Queue,
    outfits: std::sync::Arc<crate::outfit_runtime::Queue>,
    selectors: Mutex<crate::selector_tables::Summary>,
    player_folder_files: AtomicUsize,
    v2_features: Mutex<Value>,
    camera_command: Mutex<Option<crate::feature_runtime::CameraCommand>>,
    v2_presentation: Mutex<Value>,
    jukebox: Mutex<Value>,
    timing: Mutex<timing::History>,
    clock: Instant,
    /// The last status file write failed; the unchanged-state gate must not
    /// suppress the next attempt.
    status_unwritten: AtomicBool,
}
impl Control {
    pub fn new(context: Option<ContextData>) -> Result<Self> {
        Self::new_with_timing(context, 0, 0)
    }
    pub(crate) fn new_with_timing(
        context: Option<ContextData>,
        bootstrap_ms: u64,
        index_ms: u64,
    ) -> Result<Self> {
        if let Some(c) = &context {
            c.validate()?;
        }
        ensure!(
            bootstrap_ms <= 40_000 && index_ms <= 40_000,
            "Provider bootstrap timing exceeds its deadline"
        );
        let elapsed_ms = bootstrap_ms.saturating_add(index_ms);
        let mut timing = timing::History::default();
        if elapsed_ms > 0 {
            timing.observe("provider-bootstrap", 0);
            timing.observe("provider-indexing", bootstrap_ms);
            timing.observe("provider-ready", elapsed_ms);
        }
        let now = Instant::now();
        let clock = now
            .checked_sub(Duration::from_millis(elapsed_ms))
            .unwrap_or(now);
        Ok(Self {
            context,
            started: AtomicBool::new(false),
            queued: AtomicBool::new(false),
            pending: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            dispatched: AtomicBool::new(false),
            state: Mutex::new(
                json!({"phase":"ready","gameFilesChanged":false,"automaticBlacktopFeatures":false}),
            ),
            build: Mutex::new(Value::Null),
            arena: arena_runtime::Queue::new(),
            ball: ball_runtime::Queue::new(),
            scoreboard: crate::scoreboard_queue::Queue::new(),
            outfits: std::sync::Arc::new(crate::outfit_runtime::Queue::new()),
            selectors: Mutex::new(crate::selector_tables::Summary::absent()),
            player_folder_files: AtomicUsize::new(0),
            v2_features: Mutex::new(Value::Null),
            camera_command: Mutex::new(None),
            v2_presentation: Mutex::new(Value::Null),
            jukebox: Mutex::new(Value::Null),
            timing: Mutex::new(timing),
            clock,
            status_unwritten: AtomicBool::new(false),
        })
    }
    pub fn pending(&self) -> bool {
        self.pending.load(Ordering::Acquire)
    }
    #[cfg(test)]
    pub fn fixture_active_arena(context: ContextData) -> Result<Self> {
        let c = Self::new(Some(context))?;
        c.pending.store(true, Ordering::Release);
        *c.state.lock().unwrap() = json!({"phase":"loader-active"});
        *c.build.lock().unwrap() = json!({"nativeWritesMapped":true});
        // Independent scoreboard authority is test-only, as are all fixture
        // bindings. Production derives it from the pinned Version 2 profile.
        c.build.lock().unwrap()["label"] = json!("Version 2");
        c.build.lock().unwrap()["modProviderMapped"] = json!(true);
        c.build.lock().unwrap()["version2ScoreboardMapped"] = json!(true);
        Ok(c)
    }
    pub fn claim(&self) -> bool {
        self.queued.swap(false, Ordering::AcqRel)
    }
    pub fn request(&self, operation: &str, check: impl FnOnce() -> Result<()>) -> Result<Value> {
        match operation {
            "modded-status" => {
                check()?;
                let mut status = self
                    .state
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Provider launch state unavailable"))?
                    .clone();
                // The outfit reader runs independently of arena/ball work.
                // Return its latest reply without waiting for that main loop
                // to publish the next diagnostic file. Status grants no writes.
                status["outfits"] = serde_json::to_value(self.outfits.view()?)?;
                status["scoreboard"] = self.scoreboard.json()?;
                Ok(status)
            }
            "modded-start" => {
                ensure!(self.context.is_some(), "Provider has no launch ownership");
                let mut state = self
                    .state
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Provider launch state unavailable"))?;
                check()?;
                ensure!(
                    !self.cancelled.load(Ordering::Acquire),
                    "Provider launch was cancelled"
                );
                ensure!(
                    !self.started.swap(true, Ordering::AcqRel),
                    "This launch request was already accepted"
                );
                self.pending.store(true, Ordering::Release);
                *state = json!({"phase":"queued","gameFilesChanged":false,"automaticBlacktopFeatures":false});
                self.queued.store(true, Ordering::Release);
                Ok(state.clone())
            }
            _ => anyhow::bail!("Unknown native launch control"),
        }
    }
    pub fn cancel_before_dispatch(&self) -> Result<()> {
        ensure!(
            !self.dispatched.load(Ordering::Acquire),
            "Steam dispatch is pending/active; keep the provider until game exit"
        );
        self.cancelled.store(true, Ordering::Release);
        Ok(())
    }
    pub fn select_arena(&self, id: &str, check: impl FnOnce() -> Result<()>) -> Result<Value> {
        self.require_arena_ball()?;
        let state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Provider state unavailable"))?;
        ensure!(
            self.context.is_some() && self.pending() && state["phase"] == "loader-active",
            "Wait for this provider's loader-active state before changing an arena"
        );
        Ok(serde_json::to_value(self.arena.request(id, check)?)?)
    }
    pub fn select_ball(&self, id: &str, check: impl FnOnce() -> Result<()>) -> Result<Value> {
        let build = self
            .build
            .lock()
            .map_err(|_| anyhow::anyhow!("Game build status unavailable"))?;
        ensure!(
            build["nativeWritesMapped"] == true
                || (build["label"] == "Version 2"
                    && build["modProviderMapped"] == true
                    && build["version2GameplayBallMapped"] == true),
            "Ball controls require the independently verified gameplay-ball profile"
        );
        drop(build);
        let state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Provider state unavailable"))?;
        ensure!(
            self.context.is_some() && self.pending() && state["phase"] == "loader-active",
            "Wait for this provider's loader-active state before changing a ball"
        );
        self.ball.request(id, check)?;
        self.ball.json()
    }
    /// WPF Retry cameras / Stop & restore. Only queues the command: the
    /// feature runner performs it on its next guarded tick, revalidating the
    /// same phase/retryable state this pre-check reads from its last status.
    pub fn camera_command(
        &self,
        command: crate::feature_runtime::CameraCommand,
        check: impl FnOnce() -> Result<()>,
    ) -> Result<Value> {
        ensure!(
            self.context.is_some() && self.pending(),
            "Cameras are controlled only by this provider's launched game"
        );
        check()?;
        let status = self
            .v2_features
            .lock()
            .map_err(|_| anyhow::anyhow!("Feature status unavailable"))?["features"]["cameras"]
            .clone();
        let phase = status["phase"].as_str().unwrap_or("");
        ensure!(
            camera_command_allowed(command, phase, status["retryable"] == true),
            "Cameras are {} for this game; that action is unavailable",
            if phase.is_empty() {
                "not running"
            } else {
                phase
            }
        );
        *self
            .camera_command
            .lock()
            .map_err(|_| anyhow::anyhow!("Camera command unavailable"))? = Some(command);
        Ok(json!({"operation":"camera-command","command":command_name(command),"queued":true}))
    }
    fn apply_camera_command(&self, runner: &mut crate::feature_runtime::Runner) {
        let command = self.camera_command.lock().ok().and_then(|mut c| c.take());
        if let Some(command) = command {
            // A rejected command leaves the runner's status as the answer.
            let _ = runner.camera_command(command);
        }
    }
    pub fn select_scoreboard(&self, id: &str, check: impl FnOnce() -> Result<()>) -> Result<Value> {
        let build = self
            .build
            .lock()
            .map_err(|_| anyhow::anyhow!("Game build status unavailable"))?;
        ensure!(
            build["label"] == "Version 2"
                && build["modProviderMapped"] == true
                && build["version2ScoreboardMapped"] == true,
            "Scoreboard controls require the independently verified Version 2 profile"
        );
        drop(build);
        let state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Provider state unavailable"))?;
        ensure!(
            self.context.is_some() && self.pending() && state["phase"] == "loader-active",
            "Wait for this provider's loader-active state before changing a scoreboard"
        );
        self.scoreboard.request(id, check)
    }
    pub fn outfit_command(
        &self,
        command: crate::outfit_runtime::Command,
        check: impl FnOnce() -> Result<()>,
    ) -> Result<Value> {
        self.require_outfits()?;
        let state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Provider state unavailable"))?;
        ensure!(
            self.context.is_some() && self.pending() && state["phase"] == "loader-active",
            "Wait for this provider's loader-active state before reading/editing outfits"
        );
        Ok(serde_json::to_value(self.outfits.request(command, check)?)?)
    }
    /// WPF applied the saved arena/scoreboard without F8. Queue the frozen
    /// plan's choices through the same guarded requests F8 uses, once, at
    /// loader-active; the runners still wait for stable player selection. An
    /// explicit F8 request made first wins. Unmapped builds queue nothing.
    fn queue_saved_choices(&self, authorize: impl Fn() -> Result<()>) -> Result<()> {
        let plan = &self
            .context
            .as_ref()
            .context("Provider has no launch")?
            .features;
        if let Some(id) = &plan.arena
            && self.arena.view()?.revision == 0
        {
            let _ = self.select_arena(id, &authorize);
        }
        if let Some(id) = &plan.scoreboard
            && self.scoreboard.view()?.revision == 0
        {
            // A layout that was not prepared before Play refuses the request,
            // exactly as an F8 choice would; the stock scoreboard stays.
            let _ = self.select_scoreboard(id, &authorize);
        }
        Ok(())
    }
    fn require_outfits(&self) -> Result<()> {
        let build = self
            .build
            .lock()
            .map_err(|_| anyhow::anyhow!("Game build status unavailable"))?;
        ensure!(
            build["nativeWritesMapped"] == true || version_two_outfits(&build),
            "Outfit editing requires the independently verified manual outfit carry-over profile; no request queued."
        );
        Ok(())
    }
    fn require_arena_ball(&self) -> Result<()> {
        let build = self
            .build
            .lock()
            .map_err(|_| anyhow::anyhow!("Game build status unavailable"))?;
        ensure!(
            build["nativeWritesMapped"] == true
                || (build["version2ArenaBallMapped"] == true
                    && build["label"] == "Version 2"
                    && build["modProviderMapped"] == true),
            "Arena and ball controls require a separately verified build profile; no request queued"
        );
        Ok(())
    }
    fn emit(&self, phase: &str, message: &str, error: Option<&str>) -> Result<()> {
        let context = self
            .context
            .as_ref()
            .context("Provider launch has no owner")?;
        let arena = serde_json::to_value(self.arena.view()?)?;
        let ball = self.ball.json()?;
        let scoreboard = self.scoreboard.json()?;
        let jukebox = self
            .jukebox
            .lock()
            .map_err(|_| anyhow::anyhow!("Jukebox status unavailable"))?
            .clone();
        let outfits = serde_json::to_value(self.outfits.view()?)?;
        let build = self
            .build
            .lock()
            .map_err(|_| anyhow::anyhow!("Game build status unavailable"))?
            .clone();
        let player_folder_files = self.player_folder_files.load(Ordering::Acquire);
        let v2_features = self
            .v2_features
            .lock()
            .map_err(|_| anyhow::anyhow!("Feature status unavailable"))?
            .clone();
        let presentation = self
            .v2_presentation
            .lock()
            .map_err(|_| anyhow::anyhow!("Presentation status unavailable"))?
            .clone();
        let v2_features = merge_presentation_status(v2_features, &presentation);
        let selectors = self
            .selectors
            .lock()
            .map_err(|_| anyhow::anyhow!("Player selector status unavailable"))?
            .clone();
        {
            let old = self
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("Provider launch state unavailable"))?;
            if old["phase"] == phase
                && old["message"] == message
                && old["error"] == json!(error)
                && old["arena"] == arena
                && old["ball"] == ball
                && old["scoreboard"] == scoreboard
                && old["jukebox"] == jukebox
                && old["outfits"] == outfits
                && old["selectors"] == serde_json::to_value(&selectors)?
                && old["playerFolderFiles"] == json!(player_folder_files)
                && old["build"] == build
                && old["v2Features"] == v2_features
                && !self.status_unwritten.load(Ordering::Acquire)
            {
                return Ok(());
            }
        }
        // Advance diagnostics ONLY after the existing unchanged-state gate.
        // Repeated polls do not write files or grow the phase history.
        let timing = self
            .timing
            .lock()
            .map_err(|_| anyhow::anyhow!("Provider launch timing unavailable"))?
            .observe(
                phase,
                self.clock.elapsed().as_millis().min(u64::MAX as u128) as u64,
            );
        let state = json!({"schema":"nba2k27-native-provider-session-v1","session":context.owner.session,"workerPid":std::process::id(),"phase":phase,"message":message,"error":error,"steamDispatched":self.dispatched.load(Ordering::Acquire),"automaticBlacktopFeatures":false,"arena":arena,"ball":ball,"scoreboard":scoreboard,"jukebox":jukebox,"outfits":outfits,"selectors":selectors,"build":build,"playerFolderFiles":player_folder_files,"v2Features":v2_features,"timing":timing,"updatedMs":now_ms()});
        *self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Provider launch state unavailable"))? = state.clone();
        let written = fs::create_dir_all(context.status_path().parent().unwrap())
            .map_err(anyhow::Error::from)
            .and_then(|()| backend::atomic_json(&context.status_path(), &state));
        self.status_unwritten
            .store(written.is_err(), Ordering::Release);
        written
    }
    /// Active-session status is diagnostics. A failed write (for example the
    /// file briefly held by a reader at a match transition) never ends the
    /// launch; the in-memory state stays current and the next emit retries.
    fn emit_active(&self, message: &str) {
        let _ = self.emit("loader-active", message, None);
    }
    /// Diagnostics only (timing.steps); never fails the launch or cleanup.
    fn step(&self, name: &str, started: Instant, ok: bool, detail: Option<&str>) {
        let started_ms = started
            .saturating_duration_since(self.clock)
            .as_millis()
            .min(u64::MAX as u128) as u64;
        let elapsed_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        if let Ok(mut timing) = self.timing.lock() {
            timing.step(name, started_ms, elapsed_ms, ok, detail);
        }
    }
    fn authorized(&self, gate: &Gate) -> Result<()> {
        ensure!(
            !self.cancelled.load(Ordering::Acquire) && gate.parent_alive()?,
            "Launch owner exited or cancelled before dispatch"
        );
        Ok(())
    }
    pub fn execute(
        &self,
        assets: &Service,
        gate: &Gate,
        endpoint: &str,
        server: &mut Server,
    ) -> Result<()> {
        let context = self
            .context
            .as_ref()
            .context("Provider launch has no owner")?;
        context.validate()?;
        ensure!(
            self.started.load(Ordering::Acquire) && self.pending(),
            "No accepted native launch request"
        );
        let _pins = loader_files::Directories::pin(&context.owner.portable)?;
        let _data = loader_files::Directories::pin(&context.owner.portable.join("Data"))?;
        let _report_pins = loader_files::prepare_reports(&context.owner.portable)
            .context("Preparing native diagnostics before either DLL worker starts")?;
        let game = context.game()?;
        let mut profile = None;
        let mut players = None;
        let mut selector_files = None;
        let mut camera_names = None;
        let mut jukebox: Option<crate::jukebox_launch::Session> = None;
        let mut loader: Option<LaunchLoader> = None;
        let mut arena: Option<arena_runtime::Native> = None;
        let mut ball: Option<ball_runtime::Native> = None;
        let mut scoreboard: Option<crate::scoreboard_queue::Native> = None;
        let mut outfits: Option<crate::outfit_runtime::Native> = None;
        let mut outfits_v2: Option<crate::outfit_runtime::v2::Native> = None;
        let mut outfit_lease: Option<crate::outfit_worker_v2_lease::Lease> = None;
        let mut features: Option<crate::feature_runtime::Runner> = None;
        let mut presentation: Option<crate::presentation_v2_runtime::Runner> = None;
        let start = SystemTime::now();
        let requested = now_ms();
        let result = (|| -> Result<()> {
            self.emit(
                "verifying-game",
                "Verifying the game build and launch ownership before preparation",
                None,
            )?;
            let _image = crate::provider_access::verify_build(&context.game_exe)
                .with_context(|| format!("Verifying game image {}", context.game_exe.display()))?;
            *self
                .build
                .lock()
                .map_err(|_| anyhow::anyhow!("Game build status unavailable"))? =
                _image.build.clone();
            let native_writes = _image.build["nativeWritesMapped"] == true;
            let version_two_assets = _image.build["version2ArenaBallMapped"] == true;
            let version_two_ball = _image.build["version2GameplayBallMapped"] == true;
            let version_two_outfits = version_two_outfits(&_image.build);
            let mut wnba_priority = if context.features.wnba_priority_categories
                || context.features.wnba_blacktop_categories
            {
                ensure!(
                    version_two_outfits,
                    "WNBA team access requires the verified Version 2 native source"
                );
                Some(crate::wnba_priority_launch::Prepared::open(
                    &context.owner.portable,
                    context.features.wnba_priority_categories,
                    context.features.wnba_blacktop_categories,
                )?)
            } else {
                None
            };
            ensure!(
                !context.features.enabled() || _image.build["label"] == "Version 2",
                "Requested camera/scaler/Freestyle/presentation profiles require the verified Version 2 game build"
            );
            let check = || -> Result<()> {
                self.authorized(gate)?;
                closed(&context.parent)
            };
            check()?;
            self.emit(
                "preparing",
                "Preparing the owned native loader and selected mod profile",
                None,
            )
            .context("Saving provider preparation status")?;
            ensure!(
                !crate::outfit_journal::pending(&context.owner.portable)?,
                "Previous native outfit recovery needs review before another launch"
            );
            ensure!(
                !crate::outfit_worker_v2_lease::pending(&context.owner.portable)?,
                "Previous V2 native outfit admission needs exact owned recovery before another launch"
            );
            ensure!(
                !crate::presentation_v2_runtime::pending(&context.owner.portable)?,
                "Previous presentation recovery needs review before another launch"
            );
            ensure!(
                !crate::scoreboard_runtime::pending(&context.owner.portable)?,
                "Previous scoreboard recovery needs review before another launch"
            );
            ensure!(
                !crate::ball_gameplay_runtime::pending(&context.owner.portable)?,
                "Previous gameplay-ball recovery needs review before another launch"
            );
            ensure!(
                !crate::jukebox_launch::pending(&context.owner.portable)?,
                "Previous Jukebox session needs exact owned recovery before another launch"
            );
            for name in [
                "loader-session.json",
                "loader-outfit-v2-session.json",
                "mod-profile-session.json",
                crate::player_folders::MARKER,
                crate::selector_files::MARKER,
                crate::camera_name_files::MARKER,
                crate::feature_journal::MARKER,
                "blacktop-arena-session.json",
                "blacktop-ball-session.json",
            ] {
                ensure!(
                    !context
                        .owner
                        .portable
                        .join("Data")
                        .join(name)
                        .try_exists()?,
                    "Previous native mod recovery needs review before another launch"
                );
            }
            let plan = session_safety::resolve(game)
                .context("Resolving the verified Steam launch plan")?;
            // The durable profile intent binds recovery to this selected root
            // even if the worker stops before the game-folder junction exists.
            profile = mod_profile::Store::create(
                &context.owner.portable,
                game,
                &context.mods_root,
                &context.owner.session,
                check,
            )
            .with_context(|| {
                format!(
                    "Preparing mod profile {} for {}",
                    context.mods_root.display(),
                    game.display()
                )
            })?;
            // QA 0.53: independent pre-Steam preparation runs on scoped lanes.
            //  - main lane: player links (create -> activate -> index);
            //  - second: the Jukebox owned session (its own journal/receipts);
            //  - third: read-only stock decodes (camera English, scoreboards).
            // Each lane keeps its own fresh check() before every mutation and
            // also stops at its next checkpoint once a sibling failed. All
            // lanes are joined here; each owned Store/Session is assigned for
            // the unchanged exact-owned cleanup even if another lane failed.
            // Provider publication order is unchanged and stays on this
            // thread: players index, probe, camera names, Jukebox, scoreboards
            // (add_probe requires that no virtual asset was published yet).
            let cameras_enabled = !context.features.cameras.is_empty();
            let scoreboards_mapped = _image.build["version2ScoreboardMapped"] == true;
            let playlist = context
                .features
                .jukebox
                .selection
                .as_ref()
                .map(|s| s.playlist_id.clone());
            if context.features.jukebox.enabled() {
                assets.preflight_jukebox()?;
                *self
                    .jukebox
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Jukebox status unavailable"))? = json!({"phase":"preparing","message":"Preparing the sealed playlist for this mod-profile launch","playlistId":playlist,"gameplayVerified":false});
            }
            self.emit(
                "preparing-players",
                "Preparing shared-data player-folder links without copying mod files (camera labels, playlist and scoreboard layouts prepare alongside)",
                None,
            )?;
            let stop = AtomicBool::new(false);
            let step = |name: &str, started: Instant, ok: bool| self.step(name, started, ok, None);
            let lanes = parallel::run(
                &stop,
                |sibling| {
                    prepare_player_links(
                        assets,
                        &context.owner.portable,
                        &context.mods_root,
                        &context.owner.session,
                        &mut players,
                        &|| {
                            sibling.check()?;
                            check()
                        },
                        &step,
                    )
                },
                context.features.jukebox.enabled().then_some(
                    |sibling: &parallel::Sibling| -> Result<Option<crate::jukebox_launch::Session>> {
                        let started = Instant::now();
                        let prepare = || -> Result<()> {
                            sibling.check()?;
                            check()
                        };
                        let poll = || -> Result<()> {
                            sibling.check()?;
                            self.authorized(gate)
                        };
                        let session = crate::jukebox_launch::prepare(
                            &context.owner.portable,
                            game,
                            &context.mods_root,
                            &context.owner.session,
                            &context.features.jukebox,
                            crate::jukebox_launch::Checks {
                                prepare: &prepare,
                                poll: &poll,
                                cleanup: &|| closed(&context.parent),
                            },
                        );
                        step("jukebox-prepare", started, session.is_ok());
                        session
                    },
                ),
                (cameras_enabled || scoreboards_mapped).then_some(
                    |sibling: &parallel::Sibling| -> Result<StockSources> {
                        let check = || -> Result<()> {
                            sibling.check()?;
                            check()
                        };
                        // Both decoders use the game's Oodle DLL; keep them on
                        // one lane so this change adds at most one concurrent
                        // decode (the Jukebox lane's) to the process.
                        let camera = cameras_enabled.then(|| {
                            let started = Instant::now();
                            let source = crate::camera_localization::stock_source(game, &check);
                            step("cameras-decode", started, source.is_ok());
                            source
                        });
                        let scoreboards = scoreboards_mapped.then(|| {
                            let started = Instant::now();
                            let layouts = crate::scoreboard_assets::prepare(game, &check);
                            step("scoreboards-prepare", started, layouts.is_ok());
                            layouts
                        });
                        Ok(StockSources {
                            camera,
                            scoreboards,
                        })
                    },
                ),
            );
            let (_, prepared_jukebox, stock, failed) = lanes.into_parts();
            // Own the session before reporting any lane failure.
            jukebox = prepared_jukebox.flatten();
            if let Some(e) = failed {
                return Err(e);
            }
            let StockSources {
                camera: camera_source,
                scoreboards: scoreboard_layouts,
            } = stock.unwrap_or_default();
            check()?;
            if let Some(store) = &players {
                self.player_folder_files
                    .store(store.linked(), Ordering::Release);
            }
            let (path, size, sha) = assets
                .add_probe(&context.owner.session)
                .context("Adding the memory-only provider probe")?;
            if cameras_enabled {
                self.emit(
                    "preparing-cameras",
                    "Preparing camera names from the stock English labels before game launch",
                    None,
                )?;
            }
            match camera_source {
                Some(source) => crate::camera_localization::prepare_from_source(
                    assets,
                    &context.owner.portable,
                    &context.mods_root,
                    &context.owner.session,
                    &context.features,
                    &mut camera_names,
                    check,
                    source,
                ),
                None => crate::camera_localization::prepare(
                    assets,
                    &context.owner.portable,
                    game,
                    &context.mods_root,
                    &context.owner.session,
                    &context.features,
                    &mut camera_names,
                    check,
                ),
            }
            .context("Preparing owned camera labels before game launch")?;
            if context.features.jukebox.enabled() {
                self.emit(
                    "preparing-jukebox",
                    "Preparing exact owned playlist resources",
                    None,
                )?;
                let session = jukebox
                    .as_ref()
                    .context("Selected Jukebox playlist was not prepared")?;
                check()?;
                assets.publish_jukebox(session)?;
                *self
                    .jukebox
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Jukebox status unavailable"))? = json!({"phase":"prepared","message":"Playlist resources verified and pinned before Steam dispatch","playlistId":playlist,"gameplayVerified":false,"warnings":session.warnings});
            }
            if let Some(layouts) = scoreboard_layouts {
                self.emit(
                    "preparing-scoreboards",
                    "Preparing verified scoreboard text layouts from stock game assets",
                    None,
                )?;
                // The pinned DLL's fixed SCOREBOARD_TEXT_TEST=1 routing covers
                // these exact five full paths/leaves. General OVERRIDE remains
                // unchanged; no physical mod files are installed for layouts.
                let prepared = (|| -> Result<()> {
                    let layouts = layouts?;
                    check()?;
                    assets.publish_scoreboard_layouts(&layouts)?;
                    self.scoreboard.layouts_prepared(
                        true,
                        "Five verified stock text layouts frozen for this launch",
                    )
                })();
                check()?;
                if let Err(e) = prepared {
                    self.scoreboard.layouts_prepared(
                        false,
                        &format!("Text layouts unavailable; existing mods were preserved: {e:#}"),
                    )?;
                }
            }
            // Experimental WNBA Freestyle: publish a verified local build before
            // Steam when the key is already known; otherwise the feature
            // runner reads the key in-session and publishes later (the forced
            // OVERRIDE reads stock until then).
            let mut mixed_prelaunch: Option<Result<crate::wnba_roster_v2::Summary>> = None;
            if let Some(variant) = context.features.wnba_mixed {
                self.emit(
                    "preparing-wnba-roster",
                    "Checking the locally built mixed NBA + WNBA Freestyle roster",
                    None,
                )?;
                let portable = &context.owner.portable;
                let prepared = (|| -> Result<Option<crate::wnba_roster_v2::Prepared>> {
                    if let Some(p) = crate::wnba_roster_v2::cached(portable, game, variant)? {
                        return Ok(Some(p));
                    }
                    match crate::wnba_roster_v2::load_key(portable) {
                        Some(key) => Ok(Some(crate::wnba_roster_v2::build(
                            portable, game, variant, &key, check,
                        )?)),
                        None => Ok(None),
                    }
                })();
                check()?;
                // A failed pre-launch build retries in-session with a fresh
                // key; a refused publication (user roster mod) does not.
                if let Ok(Some(prepared)) = prepared {
                    mixed_prelaunch = Some(
                        assets
                            .publish_wnba_roster(&prepared)
                            .map(|()| prepared.summary),
                    );
                }
            }
            self.emit(
                "preparing-selectors",
                "Validating the selected mod folder's player configuration tables",
                None,
            )?;
            let selectors = crate::selector_runtime::prepare(
                assets,
                &context.owner.portable,
                game,
                &context.mods_root,
                &context.owner.session,
                &mut selector_files,
                check,
            )
            .context("Preparing immutable player selectors before game launch")?;
            *self
                .selectors
                .lock()
                .map_err(|_| anyhow::anyhow!("Player selector status unavailable"))? =
                selectors.clone();
            check()?;
            self.emit(
                "preparing-loader",
                "Preparing the owned native loader and its immutable launch configuration",
                None,
            )?;
            // The cover route's container is a stock file: one more forced
            // OVERRIDE (camera names + music.iff + container + mixed WNBA
            // roster <= the loader's four slots).
            let mut stock_overrides = context.features.stock_overrides();
            if let Some(session) = &jukebox {
                stock_overrides.extend(session.cover_overrides());
            }
            let config = loader_files::configuration(
                endpoint,
                &path,
                size,
                &sha,
                &context.owner.portable,
                &stock_overrides,
                selectors.enabled,
            )
            .context("Preparing the native loader configuration")?;
            let config = crate::jukebox_launch::append_loader_config(
                &config,
                jukebox.as_ref().map(|s| &s.policy),
            )?;
            // WPF "Use playlist in Freestyle": only the V2 outfit proxy carries
            // the Version 2 FreestyleBuildProfile; the stock proxy would ignore it.
            let config = match &jukebox {
                Some(session) if version_two_outfits && context.features.freestyle_playlist => {
                    crate::jukebox_launch::append_freestyle_config(&config, &session.policy)?
                }
                _ => config,
            };
            let config = if version_two_outfits {
                let worker = session_safety::identity(std::process::id())?;
                outfit_lease = Some(
                    crate::outfit_worker_v2_lease::Lease::prepare(
                        &context.owner.portable,
                        &context.owner.session,
                        &worker,
                        &context.parent,
                        &context.game_exe,
                        check,
                    )
                    .context("Preparing private V2 outfit launch admission before Steam")?,
                );
                crate::outfit_worker_v2_lease::configuration(&config, &context.owner.session)?
            } else {
                config
            };
            if let Some(p) = &mut profile {
                p.activate(check)
                    .context("Activating the no-copy mod-profile junction")?;
            }
            loader = Some(
                LaunchLoader::create(
                    version_two_outfits,
                    &context.owner.portable,
                    game,
                    &context.owner.session,
                    &config,
                    check,
                )
                .context("Recording native loader ownership")?,
            );
            loader
                .as_mut()
                .unwrap()
                .install_loader(&config, check)
                .context("Publishing the owned native loader and configuration")?;
            check()?;
            ensure!(
                session_safety::resolve(game)? == plan,
                "Steam launch plan changed during preparation"
            );
            if let Some(labels) = &camera_names {
                labels
                    .verify_current()
                    .context("Camera labels changed before Steam dispatch; launch cancelled")?;
                check()?;
            }
            if let Some(session) = &jukebox {
                session.verify_before_launch(check)?;
            }
            // Publish intent before sending the asynchronous Steam URL. Once it
            // may have been delivered, closing the UI cannot cancel resources.
            self.dispatched.store(true, Ordering::Release);
            self.emit(
                "waiting-for-game",
                "Steam launch pending; native provider owns the prepared resources",
                None,
            )?;
            let steam = Path::new(
                plan["steamExe"]
                    .as_str()
                    .context("Steam executable missing")?,
            );
            let url = plan["steamUrl"]
                .as_str()
                .context("Steam launch URL missing")?;
            let sent = Command::new(steam)
                .arg(url)
                .current_dir(steam.parent().context("Steam folder missing")?)
                .creation_flags(0x08000000)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
            if let Err(e) = sent {
                self.dispatched.store(false, Ordering::Release);
                return Err(e.into());
            }
            let wait = Instant::now();
            let identity = loop {
                if let Ok(rows) = session_safety::processes() {
                    other_owners(&rows, &context.parent)?;
                    if let Some(id) = candidate(&rows, &context.game_exe, requested)? {
                        gate.bind(&id, || Ok(()))?;
                        break id;
                    }
                }
                if wait.elapsed() > Duration::from_secs(120) {
                    self.emit("waiting-for-game","Steam has not produced a verified game yet. Resources remain owned; do not start another launcher",None)?;
                }
                thread::sleep(Duration::from_millis(200));
            };
            // The lease is created before Steam but cannot authorize an
            // in-process consumer until this exact retained game is bound.
            if let Some(lease) = &mut outfit_lease {
                lease.bind(&identity, || self.outfit_live(gate, &identity))?;
                lease.allow(|| self.outfit_live(gate, &identity))?;
            }
            if let Some(prepared) = &mut wnba_priority {
                prepared.bind(&context.owner.session, &identity, || {
                    self.outfit_live(gate, &identity)
                })?;
            }
            self.emit(
                "waiting-for-loader",
                "Game bound by exact retained identity; waiting for the native loader",
                None,
            )?;
            let logs = context.owner.portable.join("Logs/game-session");
            let armed = logs.join("nba2k27_version_proxy_armed.log");
            let report = logs.join("nba2k27_version_proxy.log");
            let ready = Instant::now();
            loop {
                live(gate, &identity)?;
                if let Some(failed) = report_for(&report, &identity, start)? {
                    // The proven loader reports a rejected build/configuration
                    // before arming too. Do not hide that behind a 90s timeout.
                    if failed.lines().any(|l| l == "LIVE_SESSION_ACTIVE=NO") {
                        let reason = readiness_failure(&failed);
                        if version_two_outfits {
                            // WPF audit D2: never leave outfits silently idle.
                            let _ = self.outfits.fail(&format!(
                                "Outfit match carry disabled: native loader inactive (LIVE_SESSION_ACTIVE=NO): {reason}"
                            ));
                        }
                        anyhow::bail!("Native loader rejected readiness: {reason}");
                    }
                }
                if report_for(&armed, &identity, start)?
                    .is_some_and(|r| r.lines().any(|l| l == "AUDIT_STATE=ARMED"))
                {
                    break;
                }
                ensure!(
                    ready.elapsed() < Duration::from_secs(90),
                    "Native loader did not arm within 90 seconds"
                );
                thread::sleep(Duration::from_millis(100));
            }
            self.emit(
                "loader-armed",
                "Native loader armed; waiting for verified stock asset readiness (up to 20 seconds)",
                None,
            )?;
            // WPF parity: its camera helper installed as soon as the game's
            // static camera tables validated, never waiting for a menu. The
            // armed native loader proves the image is initialized; the 20s
            // settle gate below is the mod loader's, not the camera's.
            if context.features.camera_scaler_freestyle_enabled() {
                let mut runner = crate::feature_runtime::Runner::new(
                    &context.owner.portable,
                    game,
                    &context.owner.session,
                    &identity,
                    &context.parent,
                    &context.features,
                )?;
                match &mixed_prelaunch {
                    Some(Ok(summary)) => runner.mixed_roster_prepublished(summary),
                    Some(Err(error)) => runner.mixed_roster_publication_failed(error),
                    None => {}
                }
                features = Some(runner);
            }
            let mut next_early_features = Instant::now();
            // Scaler-enabled Play gets its existing owned feature tick without
            // the incidental first 500ms sleep. No work occurs before ARMED;
            // combined enabled lanes keep the same guards/order within tick.
            let mut first_early_features = context.features.scaler_percent.is_some();
            let settle = Instant::now();
            // Exact pinned V2 only, after the owned ARMED report: read-only
            // stock-manager proof may end this gate early. Any doubt keeps
            // the unchanged 20 s maximum and native activation gates.
            self.authorized(gate)?;
            live(gate, &identity)?;
            let mut readiness = SettleProbe::open(
                _image.build["label"] == "Version 2",
                &context.game_exe,
                &identity,
                game,
            );
            let mut settle_end = "timeout";
            while settle.elapsed() < Duration::from_secs(20) {
                self.authorized(gate)?;
                live(gate, &identity)?;
                // Do not let an early readiness result skip the once-only
                // initial scaler opportunity. Authorization/liveness above are
                // the same existing owned gates, repeated again before tick.
                if early_feature_due(&mut first_early_features, next_early_features.elapsed()) {
                    next_early_features = Instant::now();
                    if let Some(runner) = &mut features {
                        self.authorized(gate)?;
                        live(gate, &identity)?;
                        self.apply_camera_command(runner);
                        runner.tick(gate.parent_alive()?)?;
                        publish_mixed_roster(assets, runner);
                        *self
                            .v2_features
                            .lock()
                            .map_err(|_| anyhow::anyhow!("Feature status unavailable"))? =
                            runner.status();
                    }
                }
                if readiness.ready(settle.elapsed()) {
                    settle_end = "readiness";
                    break;
                }
                thread::sleep(Duration::from_millis(100));
            }
            self.step(
                "settle-gate",
                settle,
                true,
                Some(&format!("{settle_end}; {}", readiness.detail())),
            );
            loader.as_mut().unwrap().trigger(|| {
                self.authorized(gate)?;
                live(gate, &identity)
            })?;
            self.emit(
                "activating-loader",
                "Activating the verified stock-first native asset route",
                None,
            )?;
            let activation = Instant::now();
            loop {
                live(gate, &identity)?;
                if early_feature_due(&mut first_early_features, next_early_features.elapsed()) {
                    next_early_features = Instant::now();
                    if let Some(runner) = &mut features {
                        self.authorized(gate)?;
                        live(gate, &identity)?;
                        self.apply_camera_command(runner);
                        runner.tick(gate.parent_alive()?)?;
                        publish_mixed_roster(assets, runner);
                        *self
                            .v2_features
                            .lock()
                            .map_err(|_| anyhow::anyhow!("Feature status unavailable"))? =
                            runner.status();
                    }
                }
                if let Some(text) = report_for(&report, &identity, start)? {
                    if version_two_outfits && text.lines().any(|l| l == "LIVE_SESSION_ACTIVE=NO") {
                        let _ = self.outfits.fail(
                            "Outfit match carry disabled: native loader inactive (LIVE_SESSION_ACTIVE=NO); no outfit worker this game",
                        );
                    }
                    ensure!(
                        !text.lines().any(|l| l == "LIVE_SESSION_ACTIVE=NO"),
                        "Native loader rejected activation; resources retained until game exit"
                    );
                    if text.lines().any(|l| l == "LIVE_SESSION_ACTIVE=YES") {
                        selectors.verify_loader_report(&text)?;
                        break;
                    }
                }
                ensure!(
                    activation.elapsed() < Duration::from_secs(180),
                    "Native loader activation report timed out"
                );
                thread::sleep(Duration::from_millis(100));
            }
            let active_message = if version_two_outfits {
                "Selected mod profile and native asset route are active. Manual V2 outfits include selection fitting and match carry-over QA; automatic outfits remain off. Arena, ball, scoreboard and other enabled features keep their separate checks."
            } else if native_writes {
                "Selected mod profile and native asset route are active. Manual outfit, arena and ball QA is available; automatic Blacktop features remain off"
            } else {
                "Selected mod profile and native asset route are active. Version 2 arena, gameplay-ball and scoreboard QA use separate selection checks; Game Rules uses Refresh/Apply. Cameras, scaler, Freestyle, presentation and Jukebox have separate status below. Version 2 outfit carry-over stays off unless its disk proof passes for this game image."
            };
            if jukebox.is_some() {
                let playlist = context
                    .features
                    .jukebox
                    .selection
                    .as_ref()
                    .map(|s| s.playlist_id.clone());
                *self
                    .jukebox
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Jukebox status unavailable"))? = json!({"phase":"active","message":"Playlist resources published to this mod-profile session; verify audio in game","playlistId":playlist,"gameplayVerified":false});
            }
            self.emit_active(active_message);
            self.queue_saved_choices(|| {
                self.authorized(gate)?;
                live(gate, &identity)
            })?;
            if context.features.presentation.enabled() {
                presentation = Some(crate::presentation_v2_runtime::Runner::new(
                    crate::presentation_v2_runtime::Binding::from_launched(
                        identity.clone(),
                        context.parent.clone(),
                        game.to_path_buf(),
                        context.owner.portable.clone(),
                        context.owner.session.clone(),
                    )?,
                    context.features.presentation,
                ));
                *self
                    .v2_presentation
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Presentation status unavailable"))? =
                    presentation.as_ref().unwrap().status();
            }
            if version_two_outfits {
                let started = (|| -> Result<_> {
                    let lease = outfit_lease.as_ref().context("V2 outfit lease is absent")?;
                    lease.allow(|| self.outfit_live(gate, &identity))?;
                    crate::outfit_runtime::v2::Native::start(
                        crate::outfit_runtime::Owner {
                            root: context.owner.portable.clone(),
                            game: game.to_path_buf(),
                            session: context.owner.session.clone(),
                            identity: identity.clone(),
                            parent: context.parent.clone(),
                        },
                        self.outfits.clone(),
                        lease.admission()?,
                    )
                })();
                match started {
                    Ok(worker) => outfits_v2 = Some(worker),
                    Err(e) => {
                        if let Some(lease) = &outfit_lease {
                            let _ = lease.cancel();
                        }
                        self.outfits
                            .fail(&format!("V2 outfit worker could not start: {e:#}"))?;
                    }
                }
            } else if native_writes {
                match crate::outfit_runtime::Native::start(
                    crate::outfit_runtime::Owner {
                        root: context.owner.portable.clone(),
                        game: game.to_path_buf(),
                        session: context.owner.session.clone(),
                        identity: identity.clone(),
                        parent: context.parent.clone(),
                    },
                    self.outfits.clone(),
                ) {
                    Ok(worker) => outfits = Some(worker),
                    Err(e) => self
                        .outfits
                        .fail(&format!("Outfit worker could not start: {e:#}"))?,
                }
            } else {
                self.outfits.fail("Outfit editing is disabled for this game build; updated memory mappings are pending. Mod loading is available independently.")?;
            }
            let mut next_arena = Instant::now();
            let mut next_features = Instant::now();
            let mut next_wnba_loader = Instant::now();
            while gate.game_state()? == Some(true) {
                if let Some(lease) = &outfit_lease {
                    if self.outfits.view()?.phase == "held" {
                        lease.cancel()?;
                    } else if let Err(e) = lease.allow(|| self.outfit_live(gate, &identity)) {
                        self.outfits.fail(&format!(
                            "V2 outfit admission cancelled; no retry this game: {e:#}"
                        ))?;
                    }
                }
                if next_wnba_loader.elapsed() >= Duration::from_millis(100) {
                    next_wnba_loader = Instant::now();
                    if let Some(runner) = &mut features
                        && runner.wnba_loader_enabled()
                    {
                        // An unproven gate writes nothing this tick; the
                        // 500 ms feature tick owns its bounded retry window.
                        if let Ok(alive) = feature_gate(gate, &identity)
                            && let Err(e) = runner.tick_wnba_loader(alive)
                        {
                            runner.tick_failed(&e);
                        }
                        *self
                            .v2_features
                            .lock()
                            .map_err(|_| anyhow::anyhow!("Feature status unavailable"))? =
                            runner.status();
                    }
                }
                if next_features.elapsed() >= Duration::from_millis(500) {
                    next_features = Instant::now();
                    if let Some(runner) = &mut features {
                        runner.observe_blacktop_status();
                        // Feature failures are per-feature holds; only the
                        // game-exit check above ends this session.
                        match feature_gate(gate, &identity) {
                            Ok(alive) => {
                                self.apply_camera_command(runner);
                                if let Err(e) = runner.tick(alive) {
                                    runner.tick_failed(&e);
                                }
                                publish_mixed_roster(assets, runner);
                            }
                            Err(e) => runner.gate_unavailable(&e),
                        }
                        *self
                            .v2_features
                            .lock()
                            .map_err(|_| anyhow::anyhow!("Feature status unavailable"))? =
                            runner.status();
                    }
                }
                if next_arena.elapsed() >= Duration::from_millis(250) {
                    next_arena = Instant::now();
                    if let Some(runner) = &mut presentation {
                        let status = match feature_gate(gate, &identity)
                            .and_then(|alive| runner.tick(alive))
                        {
                            Ok(()) => runner.status(),
                            Err(e) => json!({"held":true,"actors":"held","crowd":"held",
                                "message":format!("Presentation held; mod loading remains active: {e:#}")}),
                        };
                        *self
                            .v2_presentation
                            .lock()
                            .map_err(|_| anyhow::anyhow!("Presentation status unavailable"))? =
                            status;
                    }
                    let request = self.arena.view()?;
                    if request.revision > 0 && (arena.is_some() || request.phase != "held") {
                        let applied = (|| -> Result<()> {
                            live(gate, &identity)?;
                            if arena.is_none() {
                                let open = if version_two_assets {
                                    arena_runtime::Native::open_v2
                                } else {
                                    arena_runtime::Native::open
                                };
                                arena = Some(open(
                                    &context.owner.portable,
                                    game,
                                    &context.owner.session,
                                    &identity,
                                    &context.parent,
                                )?);
                            }
                            arena
                                .as_mut()
                                .unwrap()
                                .tick(&self.arena, gate.parent_alive()?)
                        })();
                        if let Err(e) = applied {
                            self.arena.publish(
                                request.revision,
                                "held",
                                &format!("Arena held; close the game before retrying: {e:#}"),
                                &request.active_id,
                                false,
                            )?;
                        }
                    }
                    self.emit_active(active_message);
                    let request = self.ball.view()?;
                    if request.revision > 0 && (ball.is_some() || request.phase != "held") {
                        if ball.is_none() && crate::blacktop_ball::current(&request.requested_id) {
                            self.ball.publish(
                                request.revision,
                                "current",
                                "Currently installed ball; no substitution active.",
                                "current",
                                true,
                            )?;
                        } else {
                            let applied = (|| -> Result<()> {
                                live(gate, &identity)?;
                                if ball.is_none() {
                                    let open = if version_two_ball {
                                        ball_runtime::Native::open_v2
                                    } else {
                                        ball_runtime::Native::open
                                    };
                                    ball = Some(open(
                                        &context.owner.portable,
                                        game,
                                        &context.owner.session,
                                        &identity,
                                        &context.parent,
                                    )?);
                                }
                                ball.as_mut()
                                    .unwrap()
                                    .tick(&self.ball, gate.parent_alive()?)
                            })();
                            if let Err(e) = applied {
                                self.ball.publish(
                                    request.revision,
                                    "held",
                                    &format!("Ball held; close the game before retrying: {e:#}"),
                                    &request.active_id,
                                    false,
                                )?;
                            }
                        }
                        self.emit_active(active_message);
                    }
                    let request = self.scoreboard.view()?;
                    if request.revision > 0 && (scoreboard.is_some() || request.phase != "held") {
                        let result = (|| -> Result<()> {
                            live(gate, &identity)?;
                            if scoreboard.is_none() {
                                let binding = crate::scoreboard_runtime::Binding::from_launched(
                                    identity.clone(),
                                    context.parent.clone(),
                                    game.to_owned(),
                                    context.owner.portable.clone(),
                                    context.owner.session.clone(),
                                )?;
                                scoreboard = Some(crate::scoreboard_queue::Native::new(
                                    binding,
                                    &self.scoreboard,
                                )?);
                            }
                            scoreboard
                                .as_mut()
                                .unwrap()
                                .tick(&self.scoreboard, gate.parent_alive()?)
                        })();
                        if let Err(e) = result {
                            self.scoreboard.failure(&format!(
                                "Scoreboard held; owned restoration remains monitored: {e:#}"
                            ))?;
                        }
                        self.emit_active(active_message);
                    }
                }
                thread::sleep(Duration::from_millis(100));
            }
            Ok(())
        })();
        // Revocation precedes waiting and every physical restore. It is
        // terminal even when the game or another owner prevents cleanup.
        let outfit_cancel = outfit_lease
            .as_ref()
            .map(|lease| lease.cancel())
            .transpose();
        let error = match (result, outfit_cancel) {
            (Ok(()), Ok(_)) => None,
            (Err(e), Ok(_)) => Some(format!("{e:#}")),
            (Ok(()), Err(e)) => Some(format!("V2 outfit cancellation requires review: {e:#}")),
            (Err(e), Err(cancel)) => Some(format!("{e:#}; V2 outfit cancellation: {cancel:#}")),
        };
        if let Some(e) = &error {
            let _ = self.emit(
                "held-until-game-exit",
                "Launch failed; keeping owned resources until game exit is verified",
                Some(e),
            );
        }
        // This global check also catches a foreign/new game. An old retained
        // handle exiting alone never authorizes restoration into a new one.
        loop {
            match closed(&context.parent) {
                Ok(()) => break,
                Err(e) => {
                    let _ = self.emit(
                        "waiting-for-safe-cleanup",
                        "Close NBA 2K27 and other launchers before recovery",
                        Some(&cleanup_failure(error.as_deref(), &format!("{e:#}"))),
                    );
                    thread::sleep(Duration::from_millis(250));
                }
            }
        }
        // Status failure must not strand exact-owned physical or session files.
        let _ = self.emit(
            "restoring-files",
            "Restoring exact-owned loader, labels, player links and playlist files",
            error.as_deref(),
        );
        let cleanup = physical_before_memory(
            || -> Result<()> {
                let mut failures = Vec::new();
                crate::native_session::cleanup_step(&mut failures, "provider server", || {
                    server.shutdown()
                });
                crate::native_session::cleanup_step(&mut failures, "loader", || {
                    if let Some(s) = &mut loader {
                        s.restore(|| closed(&context.parent))?;
                    }
                    Ok(())
                });
                crate::native_session::cleanup_step(&mut failures, "selectors", || {
                    if let Some(s) = &mut selector_files {
                        s.restore(|| closed(&context.parent))?;
                    } else if let Some(mut interrupted) = crate::selector_files::Store::recover(
                        &context.owner.portable,
                        &context.mods_root,
                        &context.owner.session,
                    )? {
                        interrupted.restore(|| closed(&context.parent))?;
                    }
                    Ok(())
                });
                crate::native_session::cleanup_step(&mut failures, "camera labels", || {
                    if let Some(s) = &mut camera_names {
                        s.restore(|| closed(&context.parent))?;
                    } else if let Some(mut interrupted) = crate::camera_name_files::Store::recover(
                        &context.owner.portable,
                        &context.mods_root,
                        &context.owner.session,
                    )? {
                        interrupted.restore(|| closed(&context.parent))?;
                    }
                    Ok(())
                });
                crate::native_session::cleanup_step(&mut failures, "player links", || {
                    if let Some(s) = &mut players {
                        s.restore(|| closed(&context.parent))?;
                    } else if let Some(mut interrupted) = crate::player_folders::Store::recover(
                        &context.owner.portable,
                        &context.mods_root,
                        &context.owner.session,
                    )? {
                        interrupted.restore(|| closed(&context.parent))?;
                    }
                    Ok(())
                });
                crate::native_session::cleanup_step(&mut failures, "Jukebox", || {
                    // shutdown() above drains readers and releases every retained
                    // audio handle before the Store needs DELETE access for restore.
                    if let Some(session) = &jukebox {
                        *self
                            .jukebox
                            .lock()
                            .map_err(|_| anyhow::anyhow!("Jukebox status unavailable"))? = json!({"phase":"restoring","message":"Restoring exact-owned playlist files after game exit","gameplayVerified":false});
                        session.restore(|| closed(&context.parent))?;
                    } else if crate::jukebox_launch::pending(&context.owner.portable)? {
                        crate::jukebox_launch::recover_ended(
                            &context.owner.portable,
                            &context.mods_root,
                            &context.owner.session,
                            || closed(&context.parent),
                        )?;
                    }
                    *self
                        .jukebox
                        .lock()
                        .map_err(|_| anyhow::anyhow!("Jukebox status unavailable"))? = json!({"phase":"default","message":"Owned playlist files restored; saved next-Play selection is unchanged","gameplayVerified":false});
                    Ok(())
                });
                crate::native_session::cleanup_step(&mut failures, "profile junction", || {
                    if let Some(s) = &mut profile {
                        s.restore(|| closed(&context.parent))?;
                    }
                    Ok(())
                });
                crate::native_session::cleanup_result(failures)?;
                Ok(())
            },
            || -> Result<()> {
                let _ = self.emit(
                    "restoring-session",
                    "Releasing ended game-session helpers and memory-only receipts",
                    error.as_deref(),
                );
                // Memory-only receipt trouble must not strand the loader or mod
                // links after the game has ended. Never replay saved addresses.
                if let Some(b) = &mut ball {
                    b.ended(&self.ball, || closed(&context.parent))?;
                }
                if let Some(a) = &mut arena {
                    a.ended(&self.arena, || closed(&context.parent))?;
                }
                if let Some(runner) = &mut scoreboard {
                    runner.finish_ended(&self.scoreboard, &session_safety::processes()?)?;
                }
                if let Some(runner) = &mut presentation {
                    runner.finish_ended(&session_safety::processes()?)?;
                    *self
                        .v2_presentation
                        .lock()
                        .map_err(|_| anyhow::anyhow!("Presentation status unavailable"))? = json!({"actors":"ended","crowd":"ended","held":false,"message":"Game session ended; presentation memory changes no longer exist."});
                }
                if let Some(runner) = &mut features {
                    runner.ended(|| closed(&context.parent))?;
                    *self
                        .v2_features
                        .lock()
                        .map_err(|_| anyhow::anyhow!("Feature status unavailable"))? =
                        runner.status();
                }
                // A corrupt outfit-only receipt must not leave the loader installed
                // after its game has ended. Its portable cleanup runs last.
                if let Some(worker) = &mut outfits {
                    worker.ended(|| closed(&context.parent))?;
                }
                if let Some(worker) = &mut outfits_v2 {
                    worker.ended(|| closed(&context.parent))?;
                }
                if let Some(lease) = &mut outfit_lease {
                    lease.finish_ended(|| closed(&context.parent))?;
                }
                Ok(())
            },
        );
        self.pending.store(false, Ordering::Release);
        match cleanup {
            Ok(()) => {
                self.emit(
                    "closed",
                    closed_message(self.dispatched.load(Ordering::Acquire)),
                    error.as_deref(),
                )?;
                Ok(())
            }
            Err(e) => {
                let _ = self.emit(
                    "needs-review",
                    "Recovery held; original/foreign files were not overwritten",
                    Some(&format!("{e:#}")),
                );
                Err(e)
            }
        }
    }
    fn outfit_live(&self, gate: &Gate, identity: &Identity) -> Result<()> {
        self.authorized(gate)?;
        live(gate, identity)?;
        let parent = &self
            .context
            .as_ref()
            .context("Outfit provider owner absent")?
            .parent;
        other_owners(&session_safety::processes()?, parent)
    }
}
/// Settle-gate observer. Disabled, unmapped or failing = never ready (the
/// caller keeps its fixed 20 s). It owns only a QUERY_INFORMATION|VM_READ handle
/// bound to the exact retained game identity.
struct SettleProbe {
    reader: Option<crate::blacktop_native::Reader>,
    stable: readiness::Stable,
    detail: String,
}
impl SettleProbe {
    fn open(enabled: bool, exe: &Path, identity: &Identity, game: &Path) -> Self {
        let mut probe = Self {
            reader: None,
            stable: readiness::Stable::default(),
            detail: "readiness fast path unavailable for this build".into(),
        };
        if !enabled {
            return probe;
        }
        let opened = (|| -> Result<crate::blacktop_native::Reader> {
            readiness::disk_proof(exe)?;
            let reader = crate::blacktop_native::Reader::open_readiness(identity, game)?;
            readiness::verify_live(&reader)?;
            Ok(reader)
        })();
        match opened {
            Ok(reader) => {
                probe.reader = Some(reader);
                probe.detail = "readiness probe armed".into();
            }
            Err(e) => probe.detail = format!("readiness probe unavailable: {e:#}"),
        }
        probe
    }
    fn ready(&mut self, now: Duration) -> bool {
        let Some(reader) = &self.reader else {
            return false;
        };
        match readiness::sample(reader) {
            Ok((manager, stock)) => {
                let ready = self.stable.update(manager, stock, now, readiness::STABLE);
                if ready {
                    self.detail = format!("stock file manager stable at {} ms", now.as_millis());
                }
                ready
            }
            Err(e) => {
                // Like WPF: a failed read restarts stability; never fails launch.
                self.stable = readiness::Stable::default();
                self.detail = format!("last sample unreadable: {e:#}");
                false
            }
        }
    }
    fn detail(&self) -> &str {
        &self.detail
    }
}
/// Read-only stock decodes from the parallel stock lane (None = not needed).
#[derive(Default)]
struct StockSources {
    camera: Option<Result<Vec<u8>>>,
    scoreboards: Option<Result<crate::scoreboard_layout::Prepared>>,
}
/// Player links: create -> activate -> index, each sub-step timed. The Store is
/// owned by `players` as soon as its receipt exists, so any later error (or a
/// sibling lane's failure) is restored by the launch's exact-owned cleanup.
fn prepare_player_links(
    assets: &Service,
    portable: &Path,
    mods: &Path,
    session: &str,
    players: &mut Option<crate::player_folders::Store>,
    check: &dyn Fn() -> Result<()>,
    step: &dyn Fn(&str, Instant, bool),
) -> Result<()> {
    let started = Instant::now();
    let created = crate::player_folders::Store::create(portable, mods, session, check);
    step("players-create", started, created.is_ok());
    *players = created?;
    let Some(store) = players.as_mut() else {
        return Ok(());
    };
    let started = Instant::now();
    let activated = store.activate(check);
    step("players-activate", started, activated.is_ok());
    activated.context("Publishing the selected player-folder links")?;
    let started = Instant::now();
    let indexed = assets.index_player_links(store);
    step("players-index", started, indexed.is_ok());
    indexed.context("Indexing canonical player-folder routes")
}
fn physical_before_memory(
    physical: impl FnOnce() -> Result<()>,
    memory: impl FnOnce() -> Result<()>,
) -> Result<()> {
    physical()?;
    memory()
}
fn merge_presentation_status(mut features: Value, presentation: &Value) -> Value {
    if presentation.is_null() {
        return features;
    }
    if !features.is_object() {
        features = json!({"version2":true,"features":{},"gameplayVerified":false});
    }
    if !features["features"].is_object() {
        features["features"] = json!({});
    }
    let actor = presentation["actors"].as_str().unwrap_or("waiting");
    let crowd = presentation["crowd"].as_str().unwrap_or("waiting");
    let phase = if presentation["held"] == true {
        "held"
    } else if actor == "waiting" || crowd == "waiting" {
        "waiting"
    } else if actor == "active" || crowd == "active" {
        "active"
    } else if actor == "ended" && crowd == "ended" {
        "ended"
    } else if crowd == "restored" {
        "restored"
    } else {
        "disabled"
    };
    features["features"]["presentation"] = json!({"phase":phase,
        "message":presentation["message"].as_str().unwrap_or("Waiting for presentation status."),
        "details":presentation});
    features
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn cleanup_failure(original: Option<&str>, blocker: &str) -> String {
    match original {
        Some(original) => format!("{original}. Cleanup waiting: {blocker}"),
        None => blocker.into(),
    }
}
fn closed_message(dispatched: bool) -> &'static str {
    if dispatched {
        "Game ended; exact owned loader files and original mod profile restored"
    } else {
        "Launch stopped before Steam dispatch; owned preparation is clear. See the launch error."
    }
}
pub(crate) fn closed(parent: &Identity) -> Result<()> {
    let scan = session_safety::process_scan()?;
    if let Err(error) = closed_rows(&scan.rows, parent) {
        let details = scan
            .issues
            .iter()
            .take(4)
            .map(|issue| format!("{} PID {}: {}", issue.name, issue.pid, issue.error))
            .collect::<Vec<_>>()
            .join("; ");
        if details.is_empty() {
            return Err(error);
        }
        anyhow::bail!("{error}; metadata still unreadable after a fresh scan: {details}");
    }
    Ok(())
}
fn closed_rows(rows: &[Process], parent: &Identity) -> Result<()> {
    ensure!(
        !rows
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe")
                || session_safety::protected_process(&p.name)),
        "Game/protected process is present"
    );
    other_owners(rows, parent)
}
fn other_owners(rows: &[Process], parent: &Identity) -> Result<()> {
    let conflicts: Vec<_> = rows
        .iter()
        .filter(|p| session_safety::launcher_process(p) && p.identity.as_ref() != Some(parent))
        .collect();
    ensure!(
        conflicts.is_empty(),
        "Another launcher owns or may start a game: {}{}",
        conflicts
            .iter()
            .take(4)
            .map(|p| format!(
                "{} (PID {}; {})",
                p.name,
                p.pid,
                if p.identity.is_none() {
                    "identity unavailable"
                } else if p.pid == parent.pid {
                    "parent identity changed"
                } else {
                    "different owner"
                }
            ))
            .collect::<Vec<_>>()
            .join(", "),
        if conflicts.len() > 4 {
            format!("; {} more", conflicts.len() - 4)
        } else {
            String::new()
        }
    );
    Ok(())
}
fn candidate(rows: &[Process], game: &Path, requested: u64) -> Result<Option<Identity>> {
    ensure!(
        !rows
            .iter()
            .any(|p| session_safety::protected_process(&p.name)),
        "Protected process present; do not bind or activate"
    );
    let games = rows
        .iter()
        .filter(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe"))
        .collect::<Vec<_>>();
    if games.is_empty() {
        return Ok(None);
    }
    ensure!(games.len() == 1, "Multiple games; provider binding held");
    let id = games[0]
        .identity
        .as_ref()
        .context("Game birth/path is unknown")?;
    let floor = requested
        .checked_mul(10_000)
        .and_then(|n| n.checked_add(116_444_736_000_000_000))
        .context("Invalid launch timestamp")?;
    ensure!(
        crate::native_session::same_path(&id.path, game) && id.started.parse::<u64>()? >= floor,
        "Game predates or differs from this launch request"
    );
    Ok(Some(id.clone()))
}
/// Per-tick gate for the in-session feature runners: the exact game is still
/// bound (live) and the launcher owner's liveness is known. A failure writes
/// nothing; callers hold or retry that feature instead of ending the session.
fn feature_gate(gate: &Gate, id: &Identity) -> Result<bool> {
    live(gate, id)?;
    gate.parent_alive()
}
fn live(gate: &Gate, id: &Identity) -> Result<()> {
    ensure!(
        gate.game_state()? == Some(true),
        "The exact game exited before native activation"
    );
    let rows = session_safety::processes()?;
    let games = rows
        .iter()
        .filter(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe"))
        .collect::<Vec<_>>();
    ensure!(
        !rows
            .iter()
            .any(|p| session_safety::protected_process(&p.name))
            && games.len() == 1
            && games[0].identity.as_ref() == Some(id),
        "Game changed/protected process present; activation held"
    );
    Ok(())
}
fn report_for(path: &Path, id: &Identity, start: SystemTime) -> Result<Option<String>> {
    current_report(
        path,
        id,
        start,
        "nba2k27-version-proxy-session-missing-asset-route-",
    )
}
fn current_report(
    path: &Path,
    id: &Identity,
    start: SystemTime,
    schema: &str,
) -> Result<Option<String>> {
    let file = match fs::OpenOptions::new()
        .read(true)
        .share_mode(7)
        .custom_flags(0x00200000)
        .open(path)
    {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let before = file.metadata()?;
    ensure!(
        before.is_file() && before.file_attributes() & 0x400 == 0 && before.len() <= 65536,
        "Invalid native loader report"
    );
    if before.modified()? < start {
        return Ok(None);
    }
    let mut b = Vec::new();
    file.take(65537).read_to_end(&mut b)?;
    ensure!(b.len() <= 65536, "Native report exceeds bounds");
    let text = String::from_utf8(b)?;
    let text = text.replace("\r\n", "\n");
    let pids = text
        .lines()
        .filter_map(|l| l.strip_prefix("PID="))
        .collect::<Vec<_>>();
    if pids != [id.pid.to_string().as_str()] {
        return Ok(None);
    }
    ensure!(
        text.lines().any(|l| l
            .strip_prefix("SCHEMA=")
            .is_some_and(|s| s.starts_with(schema))),
        "Unknown native loader report schema"
    );
    Ok(Some(text))
}
fn readiness_failure(text: &str) -> String {
    for key in ["RESOLVER_FAILURE=", "AUDIT_FAILURE=", "LIVE_FAILURE="] {
        if let Some(reason) = text.lines().find_map(|l| l.strip_prefix(key)) {
            if !reason.is_empty() && reason != "none" {
                return reason.chars().take(256).collect();
            }
        }
    }
    "see the current loader report in Logs/game-session".into()
}

#[cfg(test)]
#[path = "provider_launch_speed_tests.rs"]
mod speed_tests;
/// A schedule fact, never an authority: actual tick requires the unchanged
/// owned ARMED/live/parent/Access gates. Consume the first opportunity once.
fn early_feature_due(first: &mut bool, elapsed: Duration) -> bool {
    if *first {
        *first = false;
        return true;
    }
    elapsed >= Duration::from_millis(500)
}

#[cfg(test)]
mod tests {
    #[test]
    fn scaler_initial_feature_tick_is_due_once_without_advancing_disabled_lanes() {
        let mut first = true;
        assert!(super::early_feature_due(
            &mut first,
            std::time::Duration::ZERO
        ));
        assert!(!first);
        assert!(!super::early_feature_due(
            &mut first,
            std::time::Duration::ZERO
        ));
        assert!(!super::early_feature_due(
            &mut first,
            std::time::Duration::from_millis(499)
        ));
        assert!(super::early_feature_due(
            &mut first,
            std::time::Duration::from_millis(500)
        ));
        let mut disabled = false;
        assert!(!super::early_feature_due(
            &mut disabled,
            std::time::Duration::ZERO
        ));
        assert!(super::early_feature_due(
            &mut disabled,
            std::time::Duration::from_millis(500)
        ));
    }

    use super::*;
    #[test]
    fn camera_commands_need_an_owned_provider_and_a_matching_phase() {
        use crate::feature_runtime::CameraCommand::{Retry, Stop};
        assert!(camera_command_allowed(Retry, "stopped", false));
        assert!(camera_command_allowed(Retry, "held", true));
        assert!(!camera_command_allowed(Retry, "held", false));
        assert!(!camera_command_allowed(Retry, "active", false));
        for phase in ["waiting", "active", "reload", "reset"] {
            assert!(camera_command_allowed(Stop, phase, false));
        }
        for phase in ["held", "stopped", "disabled", "ended", "owner-closed", ""] {
            assert!(!camera_command_allowed(Stop, phase, true));
        }
        let unowned = Control::new(None).unwrap();
        assert!(unowned.camera_command(Stop, || Ok(())).is_err());
        assert!(unowned.camera_command.lock().unwrap().is_none());
        assert_eq!(
            serde_json::to_value(Retry).unwrap(),
            serde_json::json!("retry")
        );
    }
    #[test]
    fn provider_bootstrap_and_indexing_are_included_before_launch_phases() {
        let c = Control::new_with_timing(None, 120, 340).unwrap();
        let elapsed = c.clock.elapsed().as_millis() as u64;
        assert!(elapsed >= 460);
        let snapshot =
            serde_json::to_value(c.timing.lock().unwrap().observe("fixture-launch", elapsed))
                .unwrap();
        let phases = snapshot["transitions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["phase"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            phases,
            [
                "provider-bootstrap",
                "provider-indexing",
                "provider-ready",
                "fixture-launch"
            ]
        );
        assert_eq!(snapshot["transitions"][0]["elapsedMs"], 120);
        assert_eq!(snapshot["transitions"][1]["elapsedMs"], 340);
        assert!(Control::new_with_timing(None, 40_001, 0).is_err());
    }
    #[test]
    fn owned_physical_resources_restore_before_any_memory_only_receipt_error() {
        use std::cell::RefCell;
        let order = RefCell::new(Vec::new());
        let result = physical_before_memory(
            || {
                order.borrow_mut().extend([
                    "drain provider/audio pins",
                    "loader",
                    "selectors",
                    "camera labels",
                    "player links",
                    "jukebox originals",
                    "profile",
                ]);
                Ok(())
            },
            || {
                order.borrow_mut().push("memory receipt");
                anyhow::bail!("damaged memory receipt held")
            },
        );
        assert!(result.is_err());
        assert_eq!(
            order.borrow().as_slice(),
            [
                "drain provider/audio pins",
                "loader",
                "selectors",
                "camera labels",
                "player links",
                "jukebox originals",
                "profile",
                "memory receipt"
            ]
        );
        assert!(
            physical_before_memory(
                || anyhow::bail!("physical recovery still owned"),
                || panic!("memory cleanup must not bypass physical ownership")
            )
            .is_err()
        );
    }
    #[test]
    fn cleanup_wait_preserves_the_activation_failure_instead_of_hiding_it() {
        assert_eq!(
            cleanup_failure(
                Some("Native loader did not arm within 90 seconds"),
                "Game/protected process is present"
            ),
            "Native loader did not arm within 90 seconds. Cleanup waiting: Game/protected process is present"
        );
        assert_eq!(
            cleanup_failure(None, "Unknown game presence"),
            "Unknown game presence"
        );
    }
    #[test]
    fn readiness_rejections_preserve_the_specific_original_failure() {
        assert_eq!(
            readiness_failure(
                "RESOLVER_FAILURE=none\nAUDIT_FAILURE=local provider audit configuration is invalid\n"
            ),
            "local provider audit configuration is invalid"
        );
        assert_eq!(
            readiness_failure(
                "RESOLVER_FAILURE=manager cluster is ambiguous or missing\nAUDIT_FAILURE=structural resolver incomplete\n"
            ),
            "manager cluster is ambiguous or missing"
        );
    }
    #[test]
    fn saved_arena_and_scoreboard_queue_once_through_the_guarded_f8_requests() {
        let saved = || {
            let mut context = owner();
            context.features.arena = Some("arena_000_int".into());
            context.features.scoreboard = Some("broadcast".into());
            context
        };
        // Old plans and defaults: nothing is requested.
        let c = Control::fixture_active_arena(owner()).unwrap();
        c.queue_saved_choices(|| panic!("no saved choice")).unwrap();
        assert_eq!(c.arena.view().unwrap().revision, 0);
        assert_eq!(c.scoreboard.view().unwrap().revision, 0);
        // Saved choices queue at loader-active and wait for selection.
        let c = Control::fixture_active_arena(saved()).unwrap();
        c.queue_saved_choices(|| Ok(())).unwrap();
        let arena = c.arena.view().unwrap();
        assert_eq!(
            (arena.revision, arena.requested_id.as_str()),
            (1, "arena_000_int")
        );
        assert_eq!(
            (arena.phase.as_str(), arena.active_id.as_str()),
            ("queued", "default")
        );
        let board = c.scoreboard.view().unwrap();
        assert_eq!(
            (board.revision, board.requested_id.as_str()),
            (1, "broadcast")
        );
        // Once only: a later F8 request is never overwritten.
        c.select_arena("arena_brickleyfacility_int", || Ok(()))
            .unwrap();
        c.queue_saved_choices(|| Ok(())).unwrap();
        let arena = c.arena.view().unwrap();
        assert_eq!(
            (arena.revision, arena.requested_id.as_str()),
            (2, "arena_brickleyfacility_int")
        );
        assert_eq!(c.scoreboard.view().unwrap().revision, 1);
        // The same gates as F8: game-exited checks, unmapped builds and a
        // provider that is not loader-active queue nothing and never fail.
        let c = Control::fixture_active_arena(saved()).unwrap();
        c.queue_saved_choices(|| anyhow::bail!("game exited"))
            .unwrap();
        assert_eq!(c.arena.view().unwrap().revision, 0);
        assert_eq!(c.scoreboard.view().unwrap().revision, 0);
        *c.build.lock().unwrap() = json!({"label":"Unknown","modProviderMapped":true});
        c.queue_saved_choices(|| panic!("unmapped build reached the game gate"))
            .unwrap();
        assert_eq!(c.arena.view().unwrap().revision, 0);
        assert_eq!(c.scoreboard.view().unwrap().revision, 0);
        let c = Control::fixture_active_arena(saved()).unwrap();
        *c.state.lock().unwrap() = json!({"phase":"preparing"});
        c.queue_saved_choices(|| Ok(())).unwrap();
        assert_eq!(c.arena.view().unwrap().revision, 0);
        assert_eq!(c.scoreboard.view().unwrap().revision, 0);
    }
    #[test]
    fn explicit_arena_queue_needs_an_active_provider_and_fresh_request_gate() {
        let c = Control::new(Some(owner())).unwrap();
        assert!(c.select_arena("arena_000_int", || Ok(())).is_err());
        c.pending.store(true, Ordering::Release);
        *c.state.lock().unwrap() = json!({"phase":"loader-active"});
        *c.build.lock().unwrap() = json!({"nativeWritesMapped":true});
        assert!(
            c.select_arena("arena_000_int", || anyhow::bail!("Game exited"))
                .is_err()
        );
        assert_eq!(c.arena.view().unwrap().revision, 0);
        assert!(
            c.select_arena("1976–77 76ers / The Spectrum", || Ok(()))
                .is_err()
        );
        assert_eq!(
            c.select_arena("arena_000_int", || Ok(())).unwrap()["phase"],
            "queued"
        );
        assert_eq!(c.arena.view().unwrap().revision, 1);
        // Queuing is not an application and cannot create a memory receipt.
        assert_eq!(c.arena.view().unwrap().active_id, "default");
    }
    #[test]
    fn status_observes_outfit_replies_before_the_next_main_loop_emit() {
        let c = Control::fixture_active_arena(owner()).unwrap();
        let queued = c
            .outfit_command(crate::outfit_runtime::Command::List {}, || Ok(()))
            .unwrap();
        let revision = queued["revision"].as_u64().unwrap();
        // The independent outfit queue can advance while the main provider
        // loop still has an older diagnostic snapshot (e.g. doing ball work).
        let old = c.state.lock().unwrap().clone();
        let queued_status = c.request("modded-status", || Ok(())).unwrap();
        assert_eq!(queued_status["outfits"]["revision"], revision);
        assert_eq!(queued_status["outfits"]["phase"], "queued");
        let scoreboard = c.select_scoreboard("broadcast", || Ok(())).unwrap();
        let fresh = c.request("modded-status", || Ok(())).unwrap();
        assert_eq!(fresh["scoreboard"]["revision"], scoreboard["revision"]);
        assert_eq!(fresh["scoreboard"]["requestedId"], "broadcast");
        c.outfits.fail("Independent reader stopped").unwrap();
        let status = c.request("modded-status", || Ok(())).unwrap();
        assert_eq!(status["outfits"]["completedRevision"], revision);
        assert_eq!(status["outfits"]["phase"], "held");
        assert_eq!(status["outfits"]["message"], "Independent reader stopped");
        assert_eq!(*c.state.lock().unwrap(), old);
        assert_eq!(c.outfits.view().unwrap().revision, revision);
        assert!(
            c.request("modded-status", || anyhow::bail!("foreign client"))
                .is_err()
        );
    }
    #[test]
    fn unmapped_build_blocks_all_native_commands_without_queuing_or_calling_game_gate() {
        let c = Control::fixture_active_arena(owner()).unwrap();
        for build in [
            Value::Null,
            json!({"label":"Version 2","modProviderMapped":true,"nativeWritesMapped":false}),
            json!({"label":"Unknown","modProviderMapped":true,"nativeWritesMapped":false,"version2ArenaBallMapped":true}),
        ] {
            *c.build.lock().unwrap() = build;
            assert!(
                c.select_arena("arena_000_int", || panic!("unmapped game gate called"))
                    .is_err()
            );
            assert!(
                c.select_ball("current", || panic!("unmapped game gate called"))
                    .is_err()
            );
            assert!(
                c.outfit_command(crate::outfit_runtime::Command::List {}, || panic!(
                    "unmapped game gate called"
                ))
                .is_err()
            );
            assert_eq!(c.arena.view().unwrap().revision, 0);
            assert_eq!(c.ball.view().unwrap().revision, 0);
            assert_eq!(c.outfits.view().unwrap().revision, 0);
        }
    }
    #[test]
    fn version_two_asset_selection_gate_is_separate_from_outfit_and_legacy_writes() {
        let c = Control::fixture_active_arena(owner()).unwrap();
        *c.build.lock().unwrap() = json!({"label":"Version 2", "modProviderMapped":true,
            "nativeWritesMapped":false,"version2ArenaBallMapped":true,"version2GameplayBallMapped":true});
        assert!(
            c.outfit_command(crate::outfit_runtime::Command::List {}, || panic!(
                "legacy gate called"
            ))
            .is_err()
        );
        assert_eq!(
            c.select_arena("arena_000_int", || Ok(())).unwrap()["phase"],
            "queued"
        );
        assert_eq!(
            c.select_ball("current", || Ok(())).unwrap()["phase"],
            "queued"
        );
        assert_eq!(c.outfits.view().unwrap().revision, 0);
        assert!(
            c.select_ball("current", || anyhow::bail!("owner exited"))
                .is_err()
        );
        assert_eq!(c.ball.view().unwrap().revision, 1);
    }
    #[test]
    fn version_two_manual_outfit_capability_requires_exact_profile_and_active_provider() {
        let c = Control::fixture_active_arena(owner()).unwrap();
        for build in [
            json!({"label":"Unknown","modProviderMapped":true,"version2OutfitCarryMapped":true}),
            json!({"label":"Version 2","version2OutfitCarryMapped":true}),
            json!({"label":"Version 2","modProviderMapped":true}),
            json!({"label":"Version 2","modProviderMapped":true,"version2OutfitCarryMapped":false}),
        ] {
            *c.build.lock().unwrap() = build;
            assert!(
                c.outfit_command(crate::outfit_runtime::Command::List {}, || panic!(
                    "Unmapped outfit request reached game guard"
                ))
                .is_err()
            );
            assert_eq!(c.outfits.view().unwrap().revision, 0);
        }
        *c.build.lock().unwrap() = json!({"label":"Version 2","modProviderMapped":true,"version2OutfitCarryMapped":true,"nativeWritesMapped":false});
        assert!(
            c.outfit_command(crate::outfit_runtime::Command::List {}, || anyhow::bail!(
                "Exact owner ended"
            ))
            .is_err()
        );
        assert_eq!(c.outfits.view().unwrap().revision, 0);
        c.pending.store(false, Ordering::Release);
        assert!(
            c.outfit_command(crate::outfit_runtime::Command::List {}, || panic!(
                "Inactive provider reached game guard"
            ))
            .is_err()
        );
        c.pending.store(true, Ordering::Release);
        assert_eq!(
            c.outfit_command(crate::outfit_runtime::Command::List {}, || Ok(()))
                .unwrap()["phase"],
            "queued"
        );
        assert_eq!(c.outfits.view().unwrap().revision, 1);
        assert_eq!(c.build.lock().unwrap()["nativeWritesMapped"], false);
        assert!(
            c.select_arena("arena_000_int", || panic!(
                "Outfit capability widened arena gate"
            ))
            .is_err()
        );
        assert!(
            c.select_ball("current", || panic!("Outfit capability widened ball gate"))
                .is_err()
        );
    }
    #[test]
    fn gameplay_ball_capability_never_inherits_arena_or_scoreboard_authority() {
        let c = Control::fixture_active_arena(owner()).unwrap();
        for build in [
            json!({"label":"Version 2","modProviderMapped":true,"version2ArenaBallMapped":true}),
            json!({"label":"Version 2","modProviderMapped":true,"version2ScoreboardMapped":true}),
            json!({"label":"Unknown","modProviderMapped":true,"version2GameplayBallMapped":true}),
            json!({"label":"Version 2","version2GameplayBallMapped":true}),
        ] {
            *c.build.lock().unwrap() = build;
            assert!(
                c.select_ball("current", || panic!("Unmapped ball reached game gate"))
                    .is_err()
            );
            assert_eq!(c.ball.view().unwrap().revision, 0);
        }
        *c.build.lock().unwrap() = json!({"label":"Version 2","modProviderMapped":true,
            "nativeWritesMapped":false,"version2GameplayBallMapped":true});
        assert!(
            c.select_arena("arena_000_int", || panic!("Ball authority leaked to arena"))
                .is_err()
        );
        assert!(
            c.select_ball("current", || anyhow::bail!("Game ended"))
                .is_err()
        );
        assert_eq!(c.ball.view().unwrap().revision, 0);
        assert_eq!(
            c.select_ball("current", || Ok(())).unwrap()["phase"],
            "queued"
        );
        assert_eq!(c.arena.view().unwrap().revision, 0);
        assert_eq!(c.outfits.view().unwrap().revision, 0);
    }
    #[test]
    fn scoreboard_capability_never_inherits_legacy_or_other_v2_write_authority() {
        let c = Control::fixture_active_arena(owner()).unwrap();
        for build in [
            json!({"nativeWritesMapped":true}),
            json!({"label":"Version 2","modProviderMapped":true,"version2ArenaBallMapped":true}),
            json!({"label":"Unknown","modProviderMapped":true,"version2ScoreboardMapped":true}),
            json!({"label":"Version 2","version2ScoreboardMapped":true}),
        ] {
            *c.build.lock().unwrap() = build;
            assert!(
                c.select_scoreboard("broadcast", || panic!(
                    "Unmapped scoreboard reached game gate"
                ))
                .is_err()
            );
            assert_eq!(c.scoreboard.view().unwrap().revision, 0);
        }
        *c.build.lock().unwrap() = json!({"label":"Version 2","modProviderMapped":true,"nativeWritesMapped":false,"version2ScoreboardMapped":true});
        assert!(
            c.select_scoreboard("broadcast", || anyhow::bail!("game exited"))
                .is_err()
        );
        assert_eq!(
            c.select_scoreboard("broadcast", || Ok(())).unwrap()["phase"],
            "queued"
        );
        assert!(c.select_scoreboard("mycareer-story", || Ok(())).is_err());
        assert!(c.select_scoreboard("mypark-simple", || Ok(())).is_err());
        assert_eq!(c.scoreboard.view().unwrap().revision, 1);
        assert_eq!(c.arena.view().unwrap().revision, 0);
        assert_eq!(c.ball.view().unwrap().revision, 0);
        assert_eq!(c.outfits.view().unwrap().revision, 0);
    }
    #[test]
    fn presentation_status_never_erases_other_features_or_claims_pending_crowd_is_active() {
        let existing =
            json!({"version2":true,"features":{"cameras":{"phase":"active","message":"camera"}}});
        assert_eq!(
            merge_presentation_status(existing.clone(), &Value::Null),
            existing
        );
        let raw = json!({"actors":"active","crowd":"waiting","held":false,"message":"Waiting for preload"});
        let merged = merge_presentation_status(existing.clone(), &raw);
        assert_eq!(
            merged["features"]["cameras"],
            existing["features"]["cameras"]
        );
        assert_eq!(merged["features"]["presentation"]["phase"], "waiting");
        assert_eq!(merged["features"]["presentation"]["details"], raw);
        for (actor, crowd, held, expected) in [
            ("active", "active", false, "active"),
            ("active", "restored", true, "held"),
            ("disabled", "restored", false, "restored"),
            ("disabled", "disabled", false, "disabled"),
            ("ended", "ended", false, "ended"),
        ] {
            assert_eq!(
                merge_presentation_status(
                    Value::Null,
                    &json!({"actors":actor,"crowd":crowd,"held":held,"message":"state"})
                )["features"]["presentation"]["phase"],
                expected
            );
        }
    }
    fn id(pid: u32) -> Identity {
        Identity {
            pid,
            started: "116444736010000000".into(),
            path: r"C:\Fixture\NBA2K27.exe".into(),
        }
    }
    fn owner() -> ContextData {
        ContextData {
            owner: Owner {
                portable: r"C:\Fixture\Portable".into(),
                session: backend::unique_id(),
            },
            parent: Identity {
                pid: 99,
                started: "1".into(),
                path: r"C:\Fixture\launcher.exe".into(),
            },
            game_exe: r"C:\Fixture\NBA2K27.exe".into(),
            mods_root: r"C:\Fixture\Mods".into(),
            features: Default::default(),
        }
    }
    fn process(name: &str, id: Option<Identity>) -> Process {
        Process {
            name: name.into(),
            pid: id.as_ref().map_or(7, |i| i.pid),
            parent: 99,
            identity: id,
        }
    }
    #[test]
    fn queue_requires_owner_is_one_use_and_expired_request_does_not_publish() {
        let c = Control::new(None).unwrap();
        assert!(c.request("modded-start", || Ok(())).is_err());
        assert!(!c.pending());
        let c = Control::new(Some(owner())).unwrap();
        assert!(
            c.request("modded-start", || anyhow::bail!("expired"))
                .is_err()
        );
        assert!(!c.claim());
        assert!(!c.pending());
        assert_eq!(
            c.request("modded-start", || Ok(())).unwrap()["phase"],
            "queued"
        );
        assert!(c.claim());
        assert!(!c.claim());
        assert!(c.pending());
        assert!(c.request("modded-start", || Ok(())).is_err());
    }
    #[test]
    fn cancellation_before_dispatch_is_safe_but_dispatched_resources_cannot_be_pulled() {
        let c = Control::new(Some(owner())).unwrap();
        c.cancel_before_dispatch().unwrap();
        assert!(c.request("modded-start", || Ok(())).is_err());
        let c = Control::new(Some(owner())).unwrap();
        c.request("modded-start", || Ok(())).unwrap();
        c.dispatched.store(true, Ordering::Release);
        assert!(c.cancel_before_dispatch().is_err());
        assert!(!c.cancelled.load(Ordering::Acquire));
        assert!(c.pending());
    }
    #[test]
    fn game_candidate_needs_exact_path_birth_unique_identity_and_no_protected_process() {
        let game = Path::new(r"C:\Fixture\NBA2K27.exe");
        let i = id(42);
        assert_eq!(candidate(&[], game, 1000).unwrap(), None);
        assert_eq!(
            candidate(&[process("NBA2K27.exe", Some(i.clone()))], game, 1000).unwrap(),
            Some(i.clone())
        );
        assert!(candidate(&[process("NBA2K27.exe", Some(i.clone()))], game, 1001).is_err());
        assert!(candidate(&[process("NBA2K27.exe", None)], game, 1000).is_err());
        assert!(
            candidate(
                &[
                    process("NBA2K27.exe", Some(i.clone())),
                    process("NBA2K27.exe", Some(id(43)))
                ],
                game,
                1000
            )
            .is_err()
        );
        assert!(
            candidate(
                &[
                    process("NBA2K27.exe", Some(i)),
                    process("EasyAntiCheat_EOS.exe", None)
                ],
                game,
                1000
            )
            .is_err()
        );
    }
    #[test]
    fn cleanup_allows_only_the_exact_parent_not_a_reused_pid_or_another_launcher() {
        let c = owner();
        assert!(
            closed_rows(
                &[process(
                    "2k Launcher Rust Prototype.exe",
                    Some(c.parent.clone())
                )],
                &c.parent
            )
            .is_ok()
        );
        let mut reused = c.parent.clone();
        reused.started = "2".into();
        assert!(
            closed_rows(
                &[process("2k Launcher Rust Prototype.exe", Some(reused))],
                &c.parent
            )
            .is_err()
        );
        for p in [
            process("NBA2K27.exe", Some(id(42))),
            process("2k Launcher WPF.exe", None),
            process("EasyAntiCheat.exe", None),
        ] {
            assert!(closed_rows(&[p], &c.parent).is_err());
        }
    }
    #[test]
    fn owner_conflicts_identify_exact_blockers_and_never_grant_unresolved_parent_access() {
        let c = owner();
        let rows = [
            process("node.exe", None),
            process("2k Launcher WPF.exe", Some(id(42))),
        ];
        let message = other_owners(&rows, &c.parent).unwrap_err().to_string();
        assert!(message.contains("node.exe (PID 7; identity unavailable)"));
        assert!(message.contains("2k Launcher WPF.exe (PID 42; different owner)"));
        assert!(!message.contains(&c.parent.path));
        let unknown_parent = Process {
            name: "2k Launcher Rust Prototype.exe".into(),
            pid: c.parent.pid,
            parent: 1,
            identity: None,
        };
        assert!(other_owners(&[unknown_parent], &c.parent).is_err());
        assert!(closed_message(false).contains("before Steam dispatch"));
        assert!(!closed_message(false).contains("Game ended"));
        assert!(closed_message(true).contains("Game ended"));
    }
    #[test]
    #[ignore = "explicit metadata-only owner scan; no game memory or launch access"]
    fn actual_owner_scan_is_read_only() {
        let pid = std::env::var("C27_OWNER_AUDIT_PARENT_PID")
            .expect("explicit parent PID required")
            .parse()
            .unwrap();
        let parent = session_safety::identity(pid).unwrap();
        let retained = crate::provider_access::Process::open(&parent).unwrap();
        let scan = session_safety::process_scan().unwrap();
        let rows = scan.rows;
        let owners: Vec<_>=rows.iter().filter(|p| session_safety::launcher_process(p)).map(|p|
            json!({"name":p.name,"pid":p.pid,"identityKnown":p.identity.is_some(),"parentMatches":p.identity.as_ref()==Some(&parent)})).collect();
        let result = closed_rows(&rows, &parent);
        assert!(retained.alive().unwrap());
        let report = json!({"schema":"nba2k27-owner-metadata-audit-v2","parentPid":pid,"owners":owners,
            "refreshed":scan.refreshed,"initialIdentityIssues":scan.prior_issues,"identityIssues":scan.issues,
            "gatePassed":result.is_ok(),"error":result.err().map(|e|e.to_string()),"gameMemoryAccess":false,"steamDispatched":false,"filesChanged":false});
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("artifacts")
            .join(format!("owner-audit-{}", backend::unique_id()));
        fs::create_dir(&folder).unwrap();
        fs::write(
            folder.join("report.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        println!("{}", report);
    }
    #[test]
    fn loader_report_is_current_bounded_and_pid_specific_without_following_links() {
        let root =
            std::env::temp_dir().join(format!("rust-native-report-{}", backend::unique_id()));
        fs::create_dir(&root).unwrap();
        let path = root.join("report.log");
        let i = id(42);
        assert!(report_for(&path, &i, UNIX_EPOCH).unwrap().is_none());
        fs::write(&path,b"SCHEMA=nba2k27-version-proxy-session-missing-asset-route-v11.8\r\nPID=42\r\nLIVE_SESSION_ACTIVE=YES\r\n").unwrap();
        assert!(report_for(&path, &i, UNIX_EPOCH).unwrap().is_some());
        assert!(report_for(&path, &id(43), UNIX_EPOCH).unwrap().is_none());
        assert!(
            report_for(&path, &i, SystemTime::now() + Duration::from_secs(5))
                .unwrap()
                .is_none()
        );
        fs::write(&path, b"SCHEMA=wrong\nPID=42\n").unwrap();
        assert!(report_for(&path, &i, UNIX_EPOCH).is_err());
        fs::write(&path, vec![0; 65537]).unwrap();
        assert!(report_for(&path, &i, UNIX_EPOCH).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn complete_file_preparation_handles_deep_portable_and_canonical_game_paths() {
        let root =
            std::env::temp_dir().join(format!("native-launch-path-{}", backend::unique_id()));
        let portable = root
            .join("CodexProjects/2kconversiontool/nba2k27-launcher-tauri2/Builds/0.21.0/Portable");
        let game = root.join("Program Files (x86)/Steam/steamapps/common/NBA 2K27");
        for p in [
            portable.join("App"),
            portable.join("Data"),
            game.join("mods"),
        ] {
            fs::create_dir_all(p).unwrap();
        }
        backend::atomic_json_new(&portable.join("Data/settings.json"), &json!({})).unwrap();
        let mut context = owner();
        context.owner.portable = portable.clone();
        let c = Control::new(Some(context.clone())).unwrap();
        c.emit("preparing", "fixture preparation", None).unwrap();
        let endpoint = format!(
            "{}{}",
            crate::provider_protocol::PIPE_PREFIX,
            "11".repeat(32)
        );
        let config = loader_files::configuration(
            &endpoint,
            "__2k_launcher__/probe.bin",
            7,
            &loader_files::digest(b"fixture"),
            &portable,
            &[],
            false,
        )
        .unwrap();
        let _reports = loader_files::prepare_reports(&portable).unwrap();
        assert!(portable.join("Logs/game-session").is_dir());
        assert!(
            mod_profile::Store::create(
                &portable,
                &game,
                &game.join("mods"),
                &context.owner.session,
                || Ok(())
            )
            .unwrap()
            .is_none()
        );
        let mut store =
            loader_files::Store::create(&portable, &game, &context.owner.session, &config, || {
                Ok(())
            })
            .unwrap();
        store.install_loader(&config, || Ok(())).unwrap();
        store.restore(|| Ok(())).unwrap();
        c.emit("closed", "fixture restored", None).unwrap();
        assert!(!game.join("version.dll").exists());
        drop(store);
        drop(_reports);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn held_status_file_never_ends_the_session_and_the_next_emit_rewrites_it() {
        let root =
            std::env::temp_dir().join(format!("provider-status-held-{}", backend::unique_id()));
        let mut context = owner();
        context.owner.portable = root.join("Portable");
        let c = Control::new(Some(context.clone())).unwrap();
        c.emit("loader-active", "first", None).unwrap();
        let path = context.status_path();
        let read = || -> Value { serde_json::from_slice(&fs::read(&path).unwrap()).unwrap() };
        assert_eq!(read()["message"], "first");
        // A reader without FILE_SHARE_DELETE (the 0.52 match-load failure):
        // the replacing rename is denied for the whole bounded retry.
        let reader = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&path)
            .unwrap();
        let error = c.emit("loader-active", "match loading", None).unwrap_err();
        assert!(format!("{error:#}").contains("provider-state.json"));
        c.emit_active("match loading");
        assert_eq!(c.state.lock().unwrap()["message"], "match loading");
        drop(reader);
        // Identical state is NOT suppressed after a failed write.
        c.emit("loader-active", "match loading", None).unwrap();
        assert_eq!(read()["message"], "match loading");
        assert!(!c.status_unwritten.load(Ordering::Acquire));
        fs::remove_dir_all(root).unwrap();
    }
}
