//! Exact-ID requests only. UI and pipe callbacks cannot inspect or mutate the
//! game; the provider's serialized worker owns the separately verified runner.
use crate::{arena_runtime::Status, scoreboard_profile as catalog, scoreboard_runtime};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::sync::Mutex;

struct State {
    status: Status,
    layouts: bool,
    layout_message: String,
    details: Value,
}
pub(crate) struct Queue(Mutex<State>);
impl Queue {
    pub(crate) fn new() -> Self {
        Self(Mutex::new(State {
            status: Status {
                phase: "current".into(),
                requested_id: "default".into(),
                active_id: "default".into(),
                revision: 0,
                applied_revision: 0,
                message: "Currently installed scoreboard; no change requested for this game."
                    .into(),
            },
            layouts: false,
            layout_message: "Text-adapted layouts require preparation before Play.".into(),
            details: Value::Null,
        }))
    }
    pub(crate) fn view(&self) -> Result<Status> {
        Ok(self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Scoreboard state unavailable"))?
            .status
            .clone())
    }
    pub(crate) fn json(&self) -> Result<Value> {
        let s = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Scoreboard state unavailable"))?;
        let mut v = serde_json::to_value(&s.status)?;
        v["requestedName"] = json!(catalog::choice(&s.status.requested_id)?.label);
        v["layoutsPrepared"] = json!(s.layouts);
        v["layoutMessage"] = json!(s.layout_message);
        v["details"] = s.details.clone();
        v["gameplayVerified"] = json!(false);
        Ok(v)
    }
    pub(crate) fn layouts_prepared(&self, ready: bool, message: &str) -> Result<()> {
        let mut s = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Scoreboard state unavailable"))?;
        ensure!(
            s.status.revision == 0,
            "Cannot change scoreboard assets after a request"
        );
        s.layouts = ready;
        s.layout_message = message.chars().take(1024).collect();
        Ok(())
    }
    pub(crate) fn ready_for(&self, id: &str) -> Result<bool> {
        let c = catalog::choice(id)?;
        Ok(!c.requires_layout_adapter
            || self
                .0
                .lock()
                .map_err(|_| anyhow::anyhow!("Scoreboard state unavailable"))?
                .layouts)
    }
    pub(crate) fn request(&self, id: &str, check: impl FnOnce() -> Result<()>) -> Result<Value> {
        let c = catalog::choice(id)?;
        let mut s = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Scoreboard state unavailable"))?;
        check()?;
        ensure!(
            !["held", "ended"].contains(&s.status.phase.as_str()),
            "Scoreboard session is held/ended; restart before retrying"
        );
        ensure!(
            !c.requires_layout_adapter || s.layouts,
            "This scoreboard layout was not prepared before Play: {}",
            s.layout_message
        );
        s.status.revision = s
            .status
            .revision
            .checked_add(1)
            .context("Scoreboard request count exhausted")?;
        s.status.requested_id = id.into();
        s.status.phase = "queued".into();
        s.status.message =
            "Waiting for stable Blacktop player selection; wait for Ready before advancing.".into();
        drop(s);
        self.json()
    }
    fn publish(&self, revision: u64, id: &str, raw: Value) -> Result<()> {
        catalog::choice(id)?;
        ensure!(
            raw["selected"] == id && raw["version2"] == true,
            "Scoreboard worker status belongs to another choice"
        );
        let phase = raw["phase"].as_str().context("Scoreboard phase missing")?;
        let held = raw["held"] == true;
        let message = raw["message"]
            .as_str()
            .context("Scoreboard message missing")?;
        let mut s = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Scoreboard state unavailable"))?;
        if phase == "ready" {
            s.status.active_id = id.into();
            s.status.applied_revision = revision;
        } else if matches!(phase, "restored" | "restored-held") {
            s.status.active_id = "default".into();
        }
        if revision == s.status.revision || held {
            s.status.phase = if held {
                "held"
            } else if phase == "held" {
                "waiting"
            } else {
                phase
            }
            .into();
            s.status.message = message.chars().take(1024).collect();
            s.details = raw;
        }
        Ok(())
    }
    pub(crate) fn failure(&self, message: &str) -> Result<()> {
        let mut s = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Scoreboard state unavailable"))?;
        s.status.phase = "held".into();
        s.status.message = message.chars().take(1024).collect();
        Ok(())
    }
    fn ended(&self) -> Result<()> {
        let mut s = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Scoreboard state unavailable"))?;
        s.status.phase = "ended".into();
        s.status.active_id = "default".into();
        s.status.message = "Game ended; memory-only scoreboard changes cleared.".into();
        Ok(())
    }
}

pub(crate) struct Native {
    runner: scoreboard_runtime::Runner,
    revision: u64,
    selected: String,
    rejected: Option<String>,
}
impl Native {
    pub(crate) fn new(binding: scoreboard_runtime::Binding, queue: &Queue) -> Result<Self> {
        let request = queue.view()?;
        Ok(Self {
            runner: scoreboard_runtime::Runner::new(
                binding,
                &request.requested_id,
                queue.ready_for(&request.requested_id)?,
            )?,
            revision: request.revision,
            selected: request.requested_id,
            rejected: None,
        })
    }
    pub(crate) fn tick(&mut self, queue: &Queue, parent_alive: bool) -> Result<()> {
        let request = queue.view()?;
        if request.revision != self.revision {
            match self.runner.choose(
                &request.requested_id,
                queue.ready_for(&request.requested_id)?,
            ) {
                Ok(()) => self.selected = request.requested_id,
                Err(e) => self.rejected = Some(format!("Scoreboard choice held: {e:#}")),
            }
            self.revision = request.revision;
        }
        // A held forward request never prevents the service from restoring an
        // exact-owned prefix when its verified safe menu returns.
        self.runner.tick(parent_alive && self.rejected.is_none())?;
        queue.publish(self.revision, &self.selected, self.runner.status())?;
        if let Some(e) = &self.rejected {
            queue.failure(e)?;
        }
        Ok(())
    }
    pub(crate) fn finish_ended(
        &mut self,
        queue: &Queue,
        rows: &[crate::session_safety::Process],
    ) -> Result<()> {
        self.runner.finish_ended(rows)?;
        queue.ended()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_exact_selectable_ids_and_prepared_layouts_queue_after_fresh_authority() {
        let q = Queue::new();
        for id in [
            "mycareer-story",
            "flashback-openrun",
            "Broadcast",
            "../broadcast",
            "mypark-simple",
        ] {
            assert!(q.request(id, || Ok(())).is_err());
        }
        assert!(
            q.request("broadcast", || anyhow::bail!("game exited"))
                .is_err()
        );
        assert_eq!(q.view().unwrap().revision, 0);
        q.layouts_prepared(true, "Fixed stock layouts frozen")
            .unwrap();
        assert_eq!(
            q.request("mypark-simple", || Ok(())).unwrap()["revision"],
            1
        );
        assert_eq!(q.json().unwrap()["activeId"], "default");
        assert!(q.layouts_prepared(false, "too late").is_err());
    }
    #[test]
    fn preflight_wait_does_not_fake_ready_or_block_retry_and_stale_result_keeps_new_request() {
        let q = Queue::new();
        q.request("broadcast", || Ok(())).unwrap();
        q.publish(1, "broadcast", json!({"version2":true,"selected":"broadcast","held":false,"phase":"held","message":"descriptor is not loaded"})).unwrap();
        assert_eq!(q.view().unwrap().phase, "waiting");
        q.request("default", || Ok(())).unwrap();
        q.publish(1, "broadcast", json!({"version2":true,"selected":"broadcast","held":false,"phase":"ready","message":"prepared"})).unwrap();
        let s = q.view().unwrap();
        assert_eq!(s.requested_id, "default");
        assert_eq!(s.phase, "queued");
        assert_eq!(s.applied_revision, 1);
        assert_eq!(s.active_id, "broadcast");
        assert_eq!(q.json().unwrap()["gameplayVerified"], false);
    }
    #[test]
    fn held_transaction_blocks_forward_requests_but_can_publish_owned_restoration() {
        let q = Queue::new();
        q.request("broadcast", || Ok(())).unwrap();
        q.publish(1, "broadcast", json!({"version2":true,"selected":"broadcast","held":true,"phase":"held","message":"partial transaction held"})).unwrap();
        assert!(q.request("default", || Ok(())).is_err());
        q.publish(1, "broadcast", json!({"version2":true,"selected":"broadcast","held":true,"phase":"restored-held","message":"exact-owned prefix restored"})).unwrap();
        assert_eq!(q.view().unwrap().active_id, "default");
        assert_eq!(q.view().unwrap().phase, "held");
    }
    #[test]
    fn waiting_with_latched_mutation_fault_remains_held_instead_of_reauthorizing_requests() {
        let q = Queue::new();
        q.request("broadcast", || Ok(())).unwrap();
        q.publish(1, "broadcast", json!({"version2":true,"selected":"broadcast","held":true,"ownsMutation":true,"phase":"waiting","message":"Waiting for safe restoration of a held transaction"})).unwrap();
        assert_eq!(q.view().unwrap().phase, "held");
        assert!(q.request("default", || Ok(())).is_err());
        assert_eq!(q.view().unwrap().applied_revision, 0);
    }
}
