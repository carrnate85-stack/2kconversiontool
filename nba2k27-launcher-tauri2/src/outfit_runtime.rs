//! Authenticated requests are queued, never executed on the pipe/UI thread.
//! The outfit worker has its own retained native session and monotonic clock;
//! catalog scans and refresh waits cannot block asset, arena or ball service.
#[path = "outfit_runtime_v2.rs"]
pub(crate) mod v2;
use crate::{
    backend,
    blacktop_native::outfit_writer::Writer,
    outfit_journal,
    outfit_selection_session::{SelectionView, Session},
    session_safety::Identity,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Deserialize, Serialize)]
#[serde(
    tag = "action",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum Command {
    List {},
    Apply {
        player_binding: String,
        outfit: backend::Outfit,
    },
    Restore {
        player_binding: String,
    },
    /// Random all: several players written back to back, one combined wait.
    ApplyMany {
        items: Vec<BatchItem>,
        #[serde(default)]
        automatic: bool,
    },
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BatchItem {
    pub player_binding: String,
    pub outfit: backend::Outfit,
}
impl Command {
    pub fn writes(&self) -> bool {
        !matches!(self, Self::List { .. })
    }
    pub fn validate(&self) -> Result<()> {
        let binding = match self {
            Self::List {} => return Ok(()),
            Self::ApplyMany { items, .. } => {
                ensure!((1..=10).contains(&items.len()), "Choose 1 to 10 players");
                let c = crate::outfits::catalog()?;
                for (i, item) in items.iter().enumerate() {
                    backend::validate_outfit(&item.outfit)?;
                    c.compose(&c.selection(&item.outfit)?)?;
                    ensure!(
                        item.player_binding.len() == 48
                            && item.player_binding.bytes().all(|b| b.is_ascii_hexdigit())
                            && !items[..i]
                                .iter()
                                .any(|o| o.player_binding == item.player_binding),
                        "Invalid or repeated native player binding"
                    );
                }
                return Ok(());
            }
            Self::Apply {
                player_binding,
                outfit,
            } => {
                backend::validate_outfit(outfit)?;
                let c = crate::outfits::catalog()?;
                c.compose(&c.selection(outfit)?)?;
                player_binding
            }
            Self::Restore { player_binding } => player_binding,
        };
        ensure!(
            binding.len() == 48 && binding.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid native player binding"
        );
        Ok(())
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Status {
    pub phase: String,
    pub message: String,
    pub revision: u64,
    pub completed_revision: u64,
    pub update: u64,
    pub busy: bool,
    pub automatic_choice_protection: bool,
    pub request_action: String,
    pub request_origin: String,
    pub last_mutation_revision: u64,
    pub last_mutation_action: String,
    pub last_mutation_origin: String,
    pub boundary: String,
    pub selection: Option<SelectionView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub carry_timing: Option<outfit_journal::runtime::Timing>,
}
struct Envelope {
    revision: u64,
    command: Command,
    created: Instant,
}
struct State {
    status: Status,
    pending: Option<Envelope>,
}
pub(crate) struct Queue(Mutex<State>, Condvar);
impl Queue {
    pub fn new() -> Self {
        Self(
            Mutex::new(State {
                status: Status {
                    phase: "idle".into(),
                    message: "Open Edit outfit to read Blacktop players.".into(),
                    revision: 0,
                    completed_revision: 0,
                    update: 0,
                    busy: false,
                    automatic_choice_protection: false,
                    request_action: "none".into(),
                    request_origin: "none".into(),
                    last_mutation_revision: 0,
                    last_mutation_action: "none".into(),
                    last_mutation_origin: "none".into(),
                    boundary: "unknown".into(),
                    selection: None,
                    carry_timing: None,
                },
                pending: None,
            }),
            Condvar::new(),
        )
    }
    fn enable_automatic_choice_protection(&self) -> Result<()> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Outfit queue unavailable"))?;
        state.status.automatic_choice_protection = true;
        Ok(())
    }
    pub fn view(&self) -> Result<Status> {
        Ok(self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Outfit queue unavailable"))?
            .status
            .clone())
    }
    pub fn fail(&self, message: &str) -> Result<()> {
        let s = self.view()?;
        self.publish(s.revision, "held", message, false, "unknown", None, true)
    }
    fn record_carry_timing(&self, timing: &outfit_journal::runtime::Timing) -> Result<()> {
        self.0
            .lock()
            .map_err(|_| anyhow::anyhow!("Outfit queue unavailable"))?
            .status
            .carry_timing = Some(timing.clone());
        Ok(())
    }
    pub fn request(&self, command: Command, check: impl FnOnce() -> Result<()>) -> Result<Status> {
        command.validate()?;
        let mut s = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Outfit queue unavailable"))?;
        check()?;
        ensure!(
            !matches!(s.status.phase.as_str(), "held" | "ended"),
            "Outfit session is held/ended; close the game before retrying"
        );
        if s.status.busy && matches!(command, Command::List { .. }) {
            return Ok(s.status.clone());
        }
        ensure!(
            !s.status.busy && s.pending.is_none(),
            "An outfit operation is still running"
        );
        s.status.revision = s
            .status
            .revision
            .checked_add(1)
            .context("Outfit request count exhausted")?;
        s.status.update += 1;
        let (action, origin) = match &command {
            Command::List {} => ("list", "read"),
            Command::Apply { .. } => ("apply", "manual"),
            Command::Restore { .. } => ("restore", "manual"),
            Command::ApplyMany {
                automatic: true, ..
            } => ("apply-many", "automatic"),
            Command::ApplyMany { .. } => ("apply-many", "manual"),
        };
        s.status.request_action = action.into();
        s.status.request_origin = origin.into();
        if command.writes() {
            s.status.last_mutation_revision = s.status.revision;
            s.status.last_mutation_action = action.into();
            s.status.last_mutation_origin = origin.into();
        }
        s.status.busy = true;
        s.status.phase = "queued".into();
        s.status.message = "Outfit request queued for the native worker.".into();
        s.pending = Some(Envelope {
            revision: s.status.revision,
            command,
            created: Instant::now(),
        });
        self.1.notify_one();
        Ok(s.status.clone())
    }
    /// New requests interrupt the idle cadence. Checking pending under the
    /// same lock as request() prevents notifications being lost before wait.
    /// Held sessions keep their bounded idle cadence even with a pending item.
    fn wait_for_work(&self, stop: &AtomicBool, timeout: Duration) -> Result<()> {
        let state = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Outfit queue unavailable"))?;
        let _guard = self
            .1
            .wait_timeout_while(state, timeout, |s| {
                !stop.load(Ordering::Acquire)
                    && (s.pending.is_none() || matches!(s.status.phase.as_str(), "held" | "ended"))
            })
            .map_err(|_| anyhow::anyhow!("Outfit queue unavailable"))?;
        Ok(())
    }
    fn wake_worker(&self) {
        // Pair stop notifications with the wait lock to avoid a stop arriving
        // between the predicate check and the condition-variable wait.
        let _guard = self.0.lock();
        self.1.notify_all();
    }
    fn take(&self) -> Result<Option<Envelope>> {
        Ok(self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Outfit queue unavailable"))?
            .pending
            .take())
    }
    fn publish(
        &self,
        revision: u64,
        phase: &str,
        message: &str,
        busy: bool,
        boundary: &str,
        selection: Option<SelectionView>,
        completed: bool,
    ) -> Result<()> {
        let mut s = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Outfit queue unavailable"))?;
        if revision != s.status.revision && !matches!(phase, "held" | "ended") {
            return Ok(());
        }
        s.status.phase = phase.into();
        s.status.message = message.chars().take(1024).collect();
        s.status.busy = busy;
        s.status.boundary = boundary.into();
        s.status.selection = selection;
        if completed {
            s.status.completed_revision = revision;
        }
        s.status.update += 1;
        Ok(())
    }
}
#[derive(Clone)]
pub(crate) struct Owner {
    pub root: PathBuf,
    pub game: PathBuf,
    pub session: String,
    pub identity: Identity,
    pub parent: Identity,
}
pub(crate) struct Native {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<()>>>,
    queue: Arc<Queue>,
    terminal_error: Option<String>,
}
impl Native {
    pub fn start(owner: Owner, queue: Arc<Queue>) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let ended = stop.clone();
        let q = queue.clone();
        let worker = thread::Builder::new()
            .name("native-outfit-session".into())
            .spawn(move || run(owner, q, ended))?;
        Ok(Self {
            stop,
            worker: Some(worker),
            queue,
            terminal_error: None,
        })
    }
    pub fn ended(&mut self, closed: impl Fn() -> Result<()>) -> Result<()> {
        closed()?;
        self.stop.store(true, Ordering::Release);
        self.queue.wake_worker();
        if let Some(e) = &self.terminal_error {
            anyhow::bail!("{e}")
        }
        if let Some(worker) = self.worker.take() {
            let result = worker
                .join()
                .map_err(|_| {
                    anyhow::anyhow!("Native outfit worker failed; retain its ownership receipt")
                })
                .and_then(|r| r);
            if let Err(e) = result {
                self.terminal_error = Some(format!("{e:#}"));
                return Err(e);
            }
        }
        let s = self.queue.view()?;
        self.queue.publish(
            s.revision,
            "ended",
            "Game ended; outfit session released.",
            false,
            "ended",
            None,
            true,
        )
    }
}
fn run(owner: Owner, queue: Arc<Queue>, stop: Arc<AtomicBool>) -> Result<()> {
    let clock = Instant::now();
    let mut session: Option<Session<Writer>> = None;
    let mut active_revision = 0u64;
    let mut last_state = String::new();
    while !stop.load(Ordering::Acquire) {
        if queue.view()?.phase == "held" {
            queue.wait_for_work(&stop, Duration::from_millis(100))?;
            continue;
        }
        if let Some(request) = queue.take()? {
            active_revision = request.revision;
            last_state.clear();
            let result = (|| -> Result<()> {
                ensure!(
                    request.created.elapsed() <= Duration::from_secs(3),
                    "Outfit request expired before native execution; refresh players"
                );
                if session.is_none() {
                    let reader =
                        Writer::open_readonly(&owner.identity, &owner.game, &owner.parent)?;
                    session = Some(Session::create(&owner.root, &owner.session, reader)?);
                }
                let s = session.as_mut().unwrap();
                let now = clock.elapsed().as_millis() as u64;
                // A stable opaque binding identifies the player's native key.
                // The single-use source token is freshly minted/revalidated on
                // this same worker, not taken from an old UI editing session.
                let view = s.list(now)?;
                match request.command {
                    Command::List {} => queue.publish(
                        active_revision,
                        "ready",
                        "Native Blacktop players refreshed.",
                        false,
                        "selection",
                        Some(view),
                        true,
                    )?,
                    Command::Apply {
                        player_binding,
                        outfit,
                    } => {
                        let token = view
                            .players
                            .iter()
                            .find(|p| p.binding == player_binding)
                            .context("Selected player changed; select the player again")?
                            .token
                            .clone();
                        s.allow_writes(&owner.game)?;
                        s.apply(&token, &outfit, clock.elapsed().as_millis() as u64)?;
                        queue.publish(
                            active_revision,
                            "refreshing",
                            "Waiting for the selected player's clothing refresh.",
                            true,
                            "selection",
                            None,
                            false,
                        )?;
                    }
                    Command::Restore { player_binding } => {
                        let token = view
                            .players
                            .iter()
                            .find(|p| p.binding == player_binding)
                            .context("Selected player changed; select the player again")?
                            .token
                            .clone();
                        s.allow_writes(&owner.game)?;
                        s.restore(&token, clock.elapsed().as_millis() as u64)?;
                        queue.publish(
                            active_revision,
                            "refreshing",
                            "Restoring the selected player's original outfit.",
                            true,
                            "selection",
                            None,
                            false,
                        )?;
                    }
                    Command::ApplyMany { .. } => anyhow::bail!(
                        "Random all batches are only available on Version 2; apply players one at a time"
                    ),
                }
                Ok(())
            })();
            if let Err(e) = result {
                let held = session.as_ref().is_some_and(|s| s.held().is_some())
                    || outfit_journal::pending(&owner.root)? && session.is_none();
                queue.publish(
                    active_revision,
                    if held { "held" } else { "error" },
                    &format!("{e:#}"),
                    false,
                    "unknown",
                    None,
                    true,
                )?;
            }
        }
        if let Some(s) = &mut session {
            if queue.view()?.phase != "held" {
                let result = s.tick(clock.elapsed().as_millis() as u64);
                match result {
                    Ok(value) => {
                        use outfit_journal::runtime::Status as P;
                        let key = serde_json::to_string(&value)?;
                        if key != last_state {
                            last_state = key;
                            let current = queue.view()?;
                            match value{
                                P::SelectionUpdated{slot,name,carry_over}=>{
                                    let view=s.list(clock.elapsed().as_millis()as u64).ok();
                                    queue.publish(active_revision,"ready",&format!("{name} · slot {} · {}",slot+1,if carry_over{"outfit verified; queued for this Blacktop match"}else{"selection verified; no match carry-over queued (restore or two-piece outfit)"}),false,"selection",view,true)?;
                                },
                                P::SelectionUpdatedMany{players,timing}=>{
                                    let view=s.list(clock.elapsed().as_millis()as u64).ok();
                                    let queued=players.iter().filter(|p|p.carry_over).count();
                                    let names=players.iter().map(|p|format!("{}. {}",p.slot+1,p.name)).collect::<Vec<_>>().join(", ");
                                    queue.publish(active_revision,"ready",&format!("{} outfits verified ({queued} queued for this Blacktop match): {names} · write {} ms, game refresh {} ms",players.len(),timing.write_ms,timing.refresh_ms),false,"selection",view,true)?;
                                },
                                P::SelectionRefreshing=>{},
                                P::Applying{number,rebuilt}=>queue.publish(active_revision,"applying",&format!("Rebuilding Blacktop match {number} outfits · {rebuilt} confirmed"),true,"match",None,false)?,
                                P::Applied{number,players}=>queue.publish(active_revision,"ready",&format!("Blacktop match {number} · {players} outfit(s) rebuilt; gameplay verification remains required."),false,"match",None,true)?,
                                P::Menu=>queue.publish(active_revision,"waiting-for-selection","Return to Blacktop player selection to edit outfits.",false,"menu",None,true)?,
                                P::Rearming{..}=>{
                                    let selection=s.observed_boundary()==outfit_journal::runtime::Boundary::Selection;
                                    queue.publish(active_revision,"rearming","Waiting for stable Blacktop selection and the native outfit worker.",false,if selection{"selection"}else{"unknown"},if selection{current.selection}else{None},true)?;
                                },
                                P::Selecting if current.boundary!="selection"=>queue.publish(active_revision,"ready","Blacktop selection ready; refresh the player list.",false,"selection",None,true)?,
                                P::NoChoices=>queue.publish(active_revision,"waiting-for-selection","No new outfit choices for this match. Edit outfits at Blacktop player selection.",false,"match",None,true)?,
                                P::WaitingForMatch if current.boundary=="selection"=>queue.publish(active_revision,"waiting-for-match","Blacktop is transitioning; player editing waits for selection.",false,"unknown",None,true)?,
                                P::WaitingForWorker{number}=>queue.publish(active_revision,"waiting-for-worker",&format!("Waiting for native outfit worker {number}."),false,"match",None,false)?,
                                P::NotReady{reason}=>queue.publish(active_revision,"waiting-for-match",&format!("Blacktop match outfits not carried yet: {reason}"),false,"match",None,true)?,
                                P::WaitingForMatch|P::Selecting=>{},
                            }
                        }
                    }
                    Err(e) => queue.publish(
                        active_revision,
                        "held",
                        &format!("Outfit session stopped without retry: {e:#}"),
                        false,
                        "unknown",
                        None,
                        true,
                    )?,
                }
            }
        }
        queue.wait_for_work(&stop, Duration::from_millis(100))?;
    }
    let closed = || crate::provider_launch::closed(&owner.parent);
    if let Some(s) = &mut session {
        s.finish(&closed)?;
    } else if outfit_journal::pending(&owner.root)? {
        outfit_journal::resolve_ended(&owner.root, &owner.session, &owner.identity, &closed)?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queued_request_before_wait_does_not_wait_for_idle_timeout() {
        let q = Arc::new(Queue::new());
        q.request(Command::List {}, || Ok(())).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = thread::spawn(move || {
            q.wait_for_work(&AtomicBool::new(false), Duration::from_secs(5))
                .unwrap();
            tx.send(q.take().unwrap().unwrap().revision).unwrap();
        });
        assert_eq!(rx.recv_timeout(Duration::from_secs(1)).unwrap(), 1);
        worker.join().unwrap();
    }
    #[test]
    fn concurrent_request_wakes_idle_wait_and_preserves_single_flight() {
        let q = Arc::new(Queue::new());
        let worker_queue = q.clone();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let worker = thread::spawn(move || {
            started_tx.send(()).unwrap();
            worker_queue
                .wait_for_work(&AtomicBool::new(false), Duration::from_secs(5))
                .unwrap();
            done_tx
                .send(worker_queue.take().unwrap().unwrap().revision)
                .unwrap();
        });
        started_rx.recv().unwrap();
        // A request on either side of entering wait must be observed.
        q.request(Command::List {}, || Ok(())).unwrap();
        assert_eq!(done_rx.recv_timeout(Duration::from_secs(1)).unwrap(), 1);
        worker.join().unwrap();
        assert_eq!(q.request(Command::List {}, || Ok(())).unwrap().revision, 1);
        assert!(q.take().unwrap().is_none());
        let start = Instant::now();
        q.wait_for_work(&AtomicBool::new(false), Duration::from_millis(20))
            .unwrap();
        assert!(start.elapsed() >= Duration::from_millis(20));
        assert!(start.elapsed() < Duration::from_secs(1));
    }
    #[test]
    fn stop_wakes_idle_wait_without_queuing_a_request() {
        let q = Arc::new(Queue::new());
        let stop = Arc::new(AtomicBool::new(false));
        let worker_queue = q.clone();
        let worker_stop = stop.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = thread::spawn(move || {
            worker_queue
                .wait_for_work(&worker_stop, Duration::from_secs(5))
                .unwrap();
            tx.send(()).unwrap();
        });
        stop.store(true, Ordering::Release);
        q.wake_worker();
        rx.recv_timeout(Duration::from_secs(1)).unwrap();
        worker.join().unwrap();
        assert!(q.take().unwrap().is_none());
    }
    #[test]
    fn held_session_with_pending_request_preserves_idle_timeout() {
        let q = Queue::new();
        q.request(Command::List {}, || Ok(())).unwrap();
        q.fail("Admission ended").unwrap();
        let start = Instant::now();
        q.wait_for_work(&AtomicBool::new(false), Duration::from_millis(20))
            .unwrap();
        assert!(start.elapsed() >= Duration::from_millis(20));
        assert!(q.request(Command::List {}, || Ok(())).is_err());
        assert_eq!(q.view().unwrap().phase, "held");
    }
    #[test]
    fn requests_are_bounded_single_flight_authenticated_and_list_scans_coalesce() {
        let q = Queue::new();
        assert!(
            q.request(Command::List {}, || anyhow::bail!("expired owner"))
                .is_err()
        );
        assert_eq!(q.view().unwrap().revision, 0);
        let first = q.request(Command::List {}, || Ok(())).unwrap();
        assert_eq!(first.revision, 1);
        assert_eq!(q.request(Command::List {}, || Ok(())).unwrap().revision, 1);
        assert!(
            q.request(
                Command::Restore {
                    player_binding: "a".repeat(48)
                },
                || Ok(())
            )
            .is_err()
        );
        let request = q.take().unwrap().unwrap();
        assert_eq!(request.revision, 1);
        q.publish(
            1,
            "ready",
            "Players refreshed",
            false,
            "selection",
            None,
            true,
        )
        .unwrap();
        assert!(
            q.request(
                Command::Restore {
                    player_binding: "arbitrary-address".into()
                },
                || Ok(())
            )
            .is_err()
        );
        q.request(
            Command::Restore {
                player_binding: "a".repeat(48),
            },
            || Ok(()),
        )
        .unwrap();
        q.publish(1, "ready", "Old completion", false, "selection", None, true)
            .unwrap();
        assert!(q.view().unwrap().busy);
        q.publish(2, "held", "Unconfirmed write", false, "unknown", None, true)
            .unwrap();
        assert!(q.request(Command::List {}, || Ok(())).is_err());
    }
    #[test]
    fn automatic_origin_defaults_manual_and_last_mutation_survives_lists() {
        let catalog = crate::outfits::catalog().unwrap();
        let outfit = catalog
            .original_outfit(
                &catalog
                    .presets
                    .iter()
                    .find(|p| p.separable && !p.manual_only && p.pool.as_deref() == Some("male"))
                    .unwrap()
                    .id,
                "Origin fixture",
            )
            .unwrap();
        let value = serde_json::json!({"action":"apply-many","items":[{"playerBinding":"a".repeat(48),"outfit":outfit}]});
        let manual: Command = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(
            manual,
            Command::ApplyMany {
                automatic: false,
                ..
            }
        ));
        let mut automatic = value;
        automatic["automatic"] = serde_json::json!(true);
        let command: Command = serde_json::from_value(automatic).unwrap();
        let q = Queue::new();
        assert!(!q.view().unwrap().automatic_choice_protection);
        q.enable_automatic_choice_protection().unwrap();
        assert!(q.view().unwrap().automatic_choice_protection);
        let queued = q.request(command, || Ok(())).unwrap();
        assert_eq!(queued.request_origin, "automatic");
        assert_eq!(queued.last_mutation_revision, queued.revision);
        q.take().unwrap();
        q.publish(
            queued.revision,
            "ready",
            "Automatic outfits verified",
            false,
            "selection",
            None,
            true,
        )
        .unwrap();
        let read = q.request(Command::List {}, || Ok(())).unwrap();
        assert_eq!(read.request_origin, "read");
        assert_eq!(read.last_mutation_origin, "automatic");
        assert_eq!(read.last_mutation_revision, queued.revision);
        q.fail("Unconfirmed request").unwrap();
        assert_eq!(q.view().unwrap().last_mutation_origin, "automatic");
    }
    #[test]
    fn commands_reject_extra_fields_addresses_and_renderer_timestamps() {
        for value in [
            serde_json::json!({"action":"list","timestamp":123}),
            serde_json::json!({"action":"restore","playerBinding":"a".repeat(48),"description":12345}),
            serde_json::json!({"action":"unknown"}),
        ] {
            assert!(serde_json::from_value::<Command>(value).is_err());
        }
        let command: Command =
            serde_json::from_value(serde_json::json!({"action":"list"})).unwrap();
        assert!(!command.writes());
    }
    #[test]
    fn idle_worker_never_opens_a_game_and_ends_promptly_and_idempotently() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("artifacts")
            .join(format!("idle-outfit-worker-{}", backend::unique_id()));
        std::fs::create_dir_all(root.join("Data")).unwrap();
        let q = Arc::new(Queue::new());
        let id = Identity {
            pid: 0x7fff_fffe,
            started: "1".into(),
            path: root.join("NBA2K27.exe").to_string_lossy().into_owned(),
        };
        let mut worker = Native::start(
            Owner {
                root: root.clone(),
                game: root.clone(),
                session: backend::unique_id(),
                identity: id.clone(),
                parent: id,
            },
            q.clone(),
        )
        .unwrap();
        thread::sleep(Duration::from_millis(20));
        assert_eq!(q.view().unwrap().phase, "idle");
        let start = Instant::now();
        worker.ended(|| Ok(())).unwrap();
        assert!(start.elapsed() < Duration::from_secs(2));
        worker.ended(|| Ok(())).unwrap();
        assert_eq!(q.view().unwrap().phase, "ended");
        assert!(!outfit_journal::pending(&root).unwrap());
    }
    #[test]
    fn repeated_end_never_hides_a_failed_owned_worker_as_successful_cleanup() {
        let q = Arc::new(Queue::new());
        let handle =
            thread::spawn(|| -> Result<()> { anyhow::bail!("fixture cleanup was not confirmed") });
        let mut worker = Native {
            stop: Arc::new(AtomicBool::new(false)),
            worker: Some(handle),
            queue: q.clone(),
            terminal_error: None,
        };
        assert!(worker.ended(|| Ok(())).is_err());
        assert!(worker.ended(|| Ok(())).is_err());
        assert_ne!(q.view().unwrap().phase, "ended");
    }
}
