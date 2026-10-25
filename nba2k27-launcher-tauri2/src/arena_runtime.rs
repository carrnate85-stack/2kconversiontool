//! Provider-owned arena request state. The pipe only queues exact catalog IDs;
//! one session worker performs selection-bound transactions, never the UI.
use crate::{
    arena_session::{Access, Engine, NativeAccess},
    blacktop_arena,
    session_safety::Identity,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Mutex};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Status {
    pub phase: String,
    pub requested_id: String,
    pub active_id: String,
    pub revision: u64,
    pub applied_revision: u64,
    pub message: String,
}
pub(crate) struct Queue(Mutex<Status>);
impl Queue {
    pub fn new() -> Self {
        Self(Mutex::new(Status {
            phase: "idle".into(),
            requested_id: "default".into(),
            active_id: "default".into(),
            revision: 0,
            applied_revision: 0,
            message: "No arena change requested for this game.".into(),
        }))
    }
    pub fn view(&self) -> Result<Status> {
        Ok(self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Arena request state unavailable"))?
            .clone())
    }
    pub fn request(&self, id: &str, check: impl FnOnce() -> Result<()>) -> Result<Status> {
        blacktop_arena::choice(id)?;
        let mut s = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Arena request state unavailable"))?;
        check()?;
        ensure!(
            s.phase != "held" && s.phase != "ended",
            "Arena session is held/ended; close the game before retrying"
        );
        s.revision = s
            .revision
            .checked_add(1)
            .context("Arena request count exhausted")?;
        s.requested_id = id.into();
        s.phase = "queued".into();
        s.message = "Waiting for stable Blacktop player selection to prepare this arena.".into();
        Ok(s.clone())
    }
    pub fn publish(
        &self,
        revision: u64,
        phase: &str,
        message: &str,
        active: &str,
        applied: bool,
    ) -> Result<()> {
        let mut s = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Arena request state unavailable"))?;
        if phase != "held" {
            s.active_id = active.into();
        }
        if applied {
            s.applied_revision = revision;
        }
        if phase == "held" || phase == "ended" || revision == s.revision {
            s.phase = phase.into();
            s.message = message.chars().take(1024).collect();
        }
        Ok(())
    }
}
pub(crate) struct Runner {
    pub engine: Engine,
    stable: u8,
    revision: u64,
    checked_selection: bool,
    owner_closed: bool,
    retries: u8,
    /// When the current selection visit was first observed; reported in the
    /// Ready message so live runs measure selection-to-Ready latency.
    selection_since: Option<std::time::Instant>,
}
/// Automatic retries per selection visit for failures proven to be before
/// any publication (a mode/neighbor change while preparing).
const RETRIES: u8 = 8;
const STABLE_TICKS: u8 = 1;
impl Runner {
    pub fn new(root: &Path, session: &str, id: &Identity) -> Result<Self> {
        Ok(Self {
            engine: Engine::new(root, session, id)?,
            stable: 0,
            revision: 0,
            checked_selection: false,
            owner_closed: false,
            retries: 0,
            selection_since: None,
        })
    }
    pub fn tick(
        &mut self,
        q: &Queue,
        a: &impl Access,
        boundary: Option<&str>,
        owner_alive: bool,
    ) -> Result<()> {
        if !owner_alive && !self.owner_closed {
            self.owner_closed = true;
            // A closing UI cancels queued choices and restores only at the
            // verified selection boundary, never into a currently loaded court.
            if self.engine.halted().is_none() {
                q.request("default", || Ok(()))?;
            }
        }
        let request = q.view()?;
        if let Some(error) = self.engine.halted() {
            q.publish(
                request.revision,
                "held",
                error,
                self.engine.selected(),
                false,
            )?;
            return Ok(());
        }
        if boundary != Some("selection") {
            self.stable = 0;
            self.checked_selection = false;
            self.retries = 0;
            self.selection_since = None;
            if request.revision != self.revision {
                q.publish(
                    request.revision,
                    "waiting-for-selection",
                    "Return to Blacktop player selection to prepare the selected arena.",
                    self.engine.selected(),
                    false,
                )?;
            }
            return Ok(());
        }
        self.stable = self.stable.saturating_add(1);
        let since = *self
            .selection_since
            .get_or_insert_with(std::time::Instant::now);
        // WPF's arena helper wrote on its first passing Selection() check;
        // the boundary read is already debounced within the tick. Two ticks
        // on the shared provider loop could outlast a short selection visit
        // (live 0.51: saved arena missed the first visit, applied the next).
        if self.stable < STABLE_TICKS
            || (request.revision == self.revision && self.checked_selection
                && !request.requested_id.starts_with("custom-"))
        {
            return Ok(());
        }
        self.checked_selection = true;
        let fresh = request.revision != self.revision;
        if fresh {
            // Visible while the transaction runs; the overlay keeps its badge
            // on "Preparing" until this request reports Ready.
            q.publish(
                request.revision,
                "preparing",
                "Preparing the arena at player selection; wait for Ready before starting.",
                self.engine.selected(),
                false,
            )?;
        }
        match self.engine.apply(a, &request.requested_id) {
            Ok(()) => {
                self.revision = request.revision;
                self.retries = 0;
                let message = if self.owner_closed {
                    "Original arena restored after the launcher closed.".to_owned()
                } else if fresh {
                    format!(
                        "Ready for the next Blacktop match; live mapping verified ({:.1} s after reaching selection).",
                        since.elapsed().as_secs_f32()
                    )
                } else {
                    "Ready for the next Blacktop match; live mapping verified.".to_owned()
                };
                q.publish(
                    request.revision,
                    "ready",
                    &message,
                    self.engine.selected(),
                    true,
                )?;
            }
            Err(e) if self.engine.retryable() && self.retries < RETRIES => {
                // Nothing was published and the original was re-verified: try
                // again on the next stable selection sample (bounded).
                self.retries += 1;
                self.checked_selection = false;
                self.stable = 0;
                q.publish(
                    request.revision,
                    "waiting-for-selection",
                    &format!("{e:#}"),
                    self.engine.selected(),
                    false,
                )?;
            }
            Err(e) => {
                // Do not repeat a failed transaction every tick. A new explicit
                // request may fix a preflight failure; uncertain writes remain
                // held by Engine until exact game exit.
                self.revision = request.revision;
                q.publish(
                    request.revision,
                    if self.engine.halted().is_some() {
                        "held"
                    } else {
                        "error"
                    },
                    &format!("{e:#}"),
                    self.engine.selected(),
                    false,
                )?;
            }
        }
        Ok(())
    }
}
pub(crate) struct Native {
    runner: Runner,
    access: Backend,
}
enum Backend {
    Legacy(NativeAccess),
    VersionTwo(crate::blacktop_native::arena_ball_v2_access::ArenaAccess),
}
impl Native {
    pub fn open(
        root: &Path,
        game: &Path,
        session: &str,
        id: &Identity,
        owner: &Identity,
    ) -> Result<Self> {
        Ok(Self {
            runner: Runner::new(root, session, id)?,
            access: Backend::Legacy(NativeAccess::open(game, id, owner)?),
        })
    }
    pub fn open_v2(
        root: &Path,
        game: &Path,
        session: &str,
        id: &Identity,
        owner: &Identity,
    ) -> Result<Self> {
        Ok(Self {
            runner: Runner::new(root, session, id)?,
            access: Backend::VersionTwo(crate::blacktop_native::arena_ball_v2_access::open_arena(
                root, game, session, id, owner,
            )?),
        })
    }
    pub fn tick(&mut self, q: &Queue, owner_alive: bool) -> Result<()> {
        match &self.access {
            Backend::Legacy(access) => {
                let boundary = access.boundary();
                self.runner
                    .tick(q, access, boundary.as_deref().ok(), owner_alive)
            }
            Backend::VersionTwo(access) => {
                let boundary = access.boundary();
                self.runner
                    .tick(q, access, boundary.as_deref().ok(), owner_alive)
            }
        }
    }
    pub fn ended(&mut self, q: &Queue, closed: impl Fn() -> Result<()>) -> Result<()> {
        self.runner.engine.game_ended(closed)?;
        let request = q.view()?;
        q.publish(
            request.revision,
            "ended",
            "Game ended; arena memory and owned receipt cleared.",
            "default",
            true,
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queue_exact_ids_expiry_and_held_owners_are_checked_before_acceptance() {
        let q = Queue::new();
        for id in [
            "1976–77 76ers / The Spectrum",
            "arena_reccenter_lobbyint",
            "../arena_000_int",
            "unknown",
        ] {
            assert!(q.request(id, || Ok(())).is_err());
        }
        assert!(
            q.request("arena_000_int", || anyhow::bail!("Expired owner"))
                .is_err()
        );
        assert_eq!(q.view().unwrap().revision, 0);
        q.request("arena_000_int", || Ok(())).unwrap();
        q.publish(1, "held", "Unknown ownership", "default", false)
            .unwrap();
        assert!(q.request("default", || Ok(())).is_err());
        assert_eq!(q.view().unwrap().revision, 1);
    }
    #[test]
    fn newer_request_survives_an_older_completion_and_only_verified_status_is_ready() {
        let q = Queue::new();
        q.request("arena_000_int", || Ok(())).unwrap();
        q.request("arena_brickleyfacility_int", || Ok(())).unwrap();
        q.publish(1, "ready", "First verified", "arena_000_int", true)
            .unwrap();
        let s = q.view().unwrap();
        assert_eq!(s.phase, "queued");
        assert_eq!(s.requested_id, "arena_brickleyfacility_int");
        assert_eq!(s.active_id, "arena_000_int");
        assert_eq!(s.applied_revision, 1);
        q.publish(
            2,
            "ready",
            "Second verified",
            "arena_brickleyfacility_int",
            true,
        )
        .unwrap();
        assert_eq!(q.view().unwrap().phase, "ready");
        q.publish(2, "ended", "Game ended", "default", true)
            .unwrap();
        assert!(q.request("default", || Ok(())).is_err());
    }
}
