//! Authenticated requests are queued, never executed on the pipe/UI thread.
//! The outfit worker has its own retained native session and monotonic clock;
//! catalog scans and refresh waits cannot block asset, arena or ball service.
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
        Arc, Mutex,
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
}
impl Command {
    pub fn writes(&self) -> bool {
        !matches!(self, Self::List { .. })
    }
    pub fn validate(&self) -> Result<()> {
        let binding = match self {
            Self::List {} => return Ok(()),
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
    pub boundary: String,
    pub selection: Option<SelectionView>,
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
pub(crate) struct Queue(Mutex<State>);
impl Queue {
    pub fn new() -> Self {
        Self(Mutex::new(State {
            status: Status {
                phase: "idle".into(),
                message: "Open Edit outfit to read Blacktop players.".into(),
                revision: 0,
                completed_revision: 0,
                update: 0,
                busy: false,
                boundary: "unknown".into(),
                selection: None,
            },
            pending: None,
        }))
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
        s.status.busy = true;
        s.status.phase = "queued".into();
        s.status.message = "Outfit request queued for the native worker.".into();
        s.pending = Some(Envelope {
            revision: s.status.revision,
            command,
            created: Instant::now(),
        });
        Ok(s.status.clone())
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
            thread::sleep(Duration::from_millis(100));
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
        thread::sleep(Duration::from_millis(100));
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

#[path = "../src/outfit_runtime_v2.rs"]
pub(crate) mod v2;
pub(crate) fn queue_fixture(outfit: backend::Outfit) -> Result<()> {
    let q = Queue::new();
    ensure!(
        q.request(Command::List {}, || anyhow::bail!("lost owner"))
            .is_err()
            && q.view()?.revision == 0,
        "Unauthenticated queue mutated"
    );
    ensure!(
        q.request(Command::List {}, || Ok(()))?.revision == 1,
        "First list revision"
    );
    ensure!(
        q.request(Command::List {}, || Ok(()))?.revision == 1,
        "Duplicate list did not coalesce"
    );
    ensure!(
        q.request(
            Command::Apply {
                player_binding: "a".repeat(48),
                outfit: outfit.clone()
            },
            || Ok(())
        )
        .is_err(),
        "Busy queue accepted a write"
    );
    let e = q.take()?.context("List envelope missing")?;
    ensure!(
        matches!(e.command, Command::List {}) && e.revision == 1,
        "Queued command changed"
    );
    q.publish(1, "ready", "fixture", false, "selection", None, true)?;
    ensure!(
        q.request(
            Command::Restore {
                player_binding: "invalid".into()
            },
            || Ok(())
        )
        .is_err(),
        "Invalid restore binding admitted"
    );
    let state = q.request(
        Command::Apply {
            player_binding: "b".repeat(48),
            outfit,
        },
        || Ok(()),
    )?;
    ensure!(
        state.revision == 2 && q.take()?.is_some(),
        "Existing Apply command was not reused"
    );
    q.publish(2, "ready", "fixture", false, "selection", None, true)?;
    q.fail("fixture terminal")?;
    ensure!(
        q.request(Command::List {}, || Ok(())).is_err(),
        "Held queue accepted a new request"
    );
    println!(
        "PASS existing authenticated Queue/Commands: expiry envelope, coalescing, single-flight writes, binding validation and terminal hold"
    );
    Ok(())
}
