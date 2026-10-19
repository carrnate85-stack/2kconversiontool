//! Selection-bound ball service, owned by the existing provider process. A
//! catalog request queues an exact ID; never writes from a UI/pipe callback.
use crate::{
    arena_runtime::Status,
    ball_session::{Access, Engine, NativeAccess},
    blacktop_ball as ball,
    session_safety::Identity,
};
use anyhow::{Context, Result, ensure};
use std::{path::Path, sync::Mutex};
pub(crate) struct Queue(Mutex<Status>, Mutex<serde_json::Value>);
impl Queue {
    pub fn new() -> Self {
        Self(
            Mutex::new(Status {
                phase: "current".into(),
                requested_id: "current".into(),
                active_id: "current".into(),
                revision: 0,
                applied_revision: 0,
                message: "Currently installed ball; no change requested for this game.".into(),
            }),
            Mutex::new(serde_json::Value::Null),
        )
    }
    pub fn view(&self) -> Result<Status> {
        Ok(self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Ball request state unavailable"))?
            .clone())
    }
    pub fn json(&self) -> Result<serde_json::Value> {
        let s = self.view()?;
        let name = ball::choice(&s.requested_id)?.name.clone();
        let mut value = serde_json::to_value(&s)?;
        value["requestedName"] = serde_json::json!(name);
        let details = self
            .1
            .lock()
            .map_err(|_| anyhow::anyhow!("Ball details unavailable"))?
            .clone();
        value["gameplayModelConfirmed"] = serde_json::json!(
            s.phase == "model-confirmed"
                && details["revision"] == s.revision
                && details["status"]["active"] == s.requested_id
                && details["status"]["gameplayModelConfirmed"] == true
        );
        value["visualAppearanceProven"] = serde_json::json!(false);
        value["details"] = details;
        Ok(value)
    }
    pub fn request(&self, id: &str, check: impl FnOnce() -> Result<()>) -> Result<Status> {
        ball::choice(id)?;
        let mut s = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Ball request state unavailable"))?;
        check()?;
        ensure!(
            !["held", "ended"].contains(&s.phase.as_str()),
            "Ball session held/ended; close the game before retrying"
        );
        s.revision = s
            .revision
            .checked_add(1)
            .context("Ball request count exhausted")?;
        s.requested_id = id.into();
        *self
            .1
            .lock()
            .map_err(|_| anyhow::anyhow!("Ball details unavailable"))? = serde_json::Value::Null;
        s.phase = "queued".into();
        s.message =
            "Waiting for unloaded Blacktop player selection; wait for Ready before advancing."
                .into();
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
            .map_err(|_| anyhow::anyhow!("Ball request state unavailable"))?;
        if phase != "held" {
            s.active_id = active.into();
        }
        if applied {
            s.applied_revision = revision;
        }
        if ["held", "ended"].contains(&phase) || revision == s.revision {
            s.phase = phase.into();
            s.message = message.chars().take(1024).collect();
        }
        Ok(())
    }
    fn publish_gameplay(
        &self,
        revision: u64,
        requested: &str,
        raw: serde_json::Value,
    ) -> Result<()> {
        let (phase, message, active, acknowledged) = gameplay_status(requested, &raw)?;
        *self
            .1
            .lock()
            .map_err(|_| anyhow::anyhow!("Ball details unavailable"))? =
            serde_json::json!({"revision":revision,"status":raw});
        self.publish(revision, &phase, &message, &active, acknowledged)?;
        if raw["held"] == true && raw["ownsMutation"] == false {
            let mut s = self
                .0
                .lock()
                .map_err(|_| anyhow::anyhow!("Ball state unavailable"))?;
            if s.revision == revision {
                s.active_id = "current".into();
            }
        }
        Ok(())
    }
    fn reject_gameplay(&self, revision: u64, raw: serde_json::Value, message: &str) -> Result<()> {
        // A worker can become held between queuing and consuming a new
        // choice. Report its actual ownership without acknowledging that
        // rejected choice, and keep polling safe restoration.
        let selected = raw["selected"]
            .as_str()
            .context("Gameplay ball choice missing")?;
        let (_, _, active, _) = gameplay_status(selected, &raw)?;
        let mut s = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Ball state unavailable"))?;
        if s.revision == revision {
            s.phase = "held".into();
            s.message = message.chars().take(1024).collect();
            s.active_id = if raw["ownsMutation"] == true {
                active
            } else {
                "current".into()
            };
            *self
                .1
                .lock()
                .map_err(|_| anyhow::anyhow!("Ball details unavailable"))? =
                serde_json::json!({"revision":revision,"requestRejected":true,"status":raw});
        }
        Ok(())
    }
}
fn gameplay_status(
    requested: &str,
    raw: &serde_json::Value,
) -> Result<(String, String, String, bool)> {
    ball::choice(requested)?;
    ensure!(
        raw["version2"] == true && raw["selected"] == requested,
        "Gameplay ball result belongs to another choice"
    );
    let active = raw["active"].as_str().unwrap_or("current");
    ball::choice(active)?;
    let held = raw["held"] == true;
    let owned = raw["ownsMutation"] == true;
    let original_phase = raw["phase"]
        .as_str()
        .context("Gameplay ball phase missing")?;
    let message = raw["message"]
        .as_str()
        .context("Gameplay ball message missing")?;
    let phase = if held {
        "held"
    } else if owned && active != requested {
        "waiting-for-unload"
    } else if !owned && ball::current(requested) {
        "current"
    } else {
        match original_phase {
            "prepared" if owned && active == requested => "ready",
            "gameplay-model-confirmed"
                if owned && active == requested && raw["gameplayModelConfirmed"] == true =>
            {
                "model-confirmed"
            }
            "prepared-unverified" => "prepared-unverified",
            "waiting-for-release" => "waiting-for-unload",
            "restored" => "restored",
            _ => "waiting-for-selection",
        }
    };
    Ok((
        phase.into(),
        if phase == "waiting-for-unload" {
            "Waiting for the gameplay ball to release before restoring its exact owned setting."
                .into()
        } else {
            message.into()
        },
        active.into(),
        matches!(phase, "ready" | "model-confirmed" | "current"),
    ))
}
pub(crate) struct Runner {
    pub engine: Engine,
    used: bool,
    stable: u8,
    owner_closed: bool,
    rejected: Option<u64>,
}
impl Runner {
    pub fn new(root: &Path, session: &str, id: &Identity) -> Result<Self> {
        Ok(Self {
            engine: Engine::new(root, session, id)?,
            used: false,
            stable: 0,
            owner_closed: false,
            rejected: None,
        })
    }
    pub fn tick(&mut self, q: &Queue, a: &impl Access, owner_alive: bool) -> Result<()> {
        if !owner_alive && !self.owner_closed {
            self.owner_closed = true;
            if self.engine.halted().is_none() {
                q.request("current", || Ok(()))?;
            }
        }
        let r = q.view()?;
        if let Some(e) = self.engine.halted() {
            return q.publish(r.revision, "held", e, self.engine.selected(), false);
        }
        if self.engine.selected() == "current" && ball::current(&r.requested_id) {
            self.stable = 0;
            return q.publish(
                r.revision,
                "current",
                "Currently installed ball; no live substitution is active.",
                "current",
                true,
            );
        }
        // Torn/transition snapshots never authorize a write. Keep observing an
        // already-owned substitution; a failed read is not permission to drop it.
        let s = match a.capture() {
            Ok(s) => s,
            Err(e) => {
                self.stable = 0;
                return q.publish(
                    r.revision,
                    "waiting",
                    &format!(
                        "Waiting for a stable ball observation; no filename write attempted: {e:#}"
                    ),
                    self.engine.selected(),
                    false,
                );
            }
        };
        if self.engine.selected() != "current" {
            if let Err(e) = self.engine.observe(&s) {
                let _ = self.engine.hold::<()>(e);
                return q.publish(
                    r.revision,
                    "held",
                    self.engine.halted().unwrap(),
                    self.engine.selected(),
                    false,
                );
            }
            self.used |= ball::observed_load(s.references, &s.resource);
            let changed = r.requested_id != self.engine.selected();
            if ball::restore_now(
                self.used,
                changed,
                self.owner_closed,
                false,
                ball::main_menu(&s),
                s.can_restore(),
                s.references,
                &s.resource,
            ) {
                if let Err(e) = self.engine.restore(a) {
                    return q.publish(
                        r.revision,
                        if e.is::<crate::ball_session::Deferred>() {
                            "waiting-for-unload"
                        } else {
                            "held"
                        },
                        &format!("{e:#}"),
                        self.engine.selected(),
                        false,
                    );
                }
                self.used = false;
                self.stable = 0;
                return q.publish(r.revision,"restored","Original ball restored. The selected choice can prepare at the next Blacktop selection.","current",ball::current(&r.requested_id));
            }
            let mesh_ready = s
                .resource
                .get(16..24)
                .is_some_and(|bytes| bytes.iter().any(|b| *b != 0));
            let (phase, message) = if changed || self.owner_closed {
                (
                    "waiting-for-unload",
                    "Choice queued. Waiting for the ball to unload at selection, Main Menu or Quick Play before restoration.",
                )
            } else if !s.native_normal() {
                (
                    "incompatible",
                    "The game chose a different gameplay ball. This resource is not proof that the court ball changed. Waiting for a safe unload to restore; no live retargeting attempted.",
                )
            } else if self.used && mesh_ready {
                (
                    "resource-loaded",
                    "Ball resource loaded; court-ball appearance is not verified. Original restores safely after Blacktop unloads.",
                )
            } else if self.used {
                (
                    "loading",
                    "Ball resource loading; court-ball appearance is not verified. Original restores safely after Blacktop unloads.",
                )
            } else {
                (
                    "ready",
                    "Ready for Blacktop; filename read-back verified. Start the match when ready.",
                )
            };
            // Loaded Normal props (including spectator balls) are not evidence
            // that AI_BALL consumed this filename. Keep the owned restoration
            // worker running even when the gameplay choice differs.
            return q.publish(
                r.revision,
                phase,
                message,
                self.engine.selected(),
                !changed && !self.owner_closed && s.native_normal(),
            );
        }
        if self.rejected == Some(r.revision) {
            return Ok(());
        }
        if !s.can_prepare() {
            self.stable = 0;
            return q.publish(
                r.revision,
                if !s.native_normal() { "incompatible" } else { "waiting-for-selection" },
                if !s.native_normal() {
                    "The gameplay ball does not match this substitution target. No filename change was attempted."
                } else {
                    "Return to Blacktop player selection and wait for Ready before advancing."
                },
                "current",
                false,
            );
        }
        self.stable = self.stable.saturating_add(1);
        if self.stable < 2 {
            return Ok(());
        }
        match self.engine.apply(a, &r.requested_id) {
            Ok(()) => {
                self.used = false;
                q.publish(
                    r.revision,
                    "ready",
                    "Ready for Blacktop; filename read-back verified. Start the match when ready.",
                    self.engine.selected(),
                    true,
                )
            }
            Err(e) => {
                if e.is::<crate::ball_session::Deferred>() {
                    self.stable = 0;
                    return q.publish(
                        r.revision,
                        "waiting-for-selection",
                        &format!("{e:#}"),
                        self.engine.selected(),
                        false,
                    );
                }
                self.rejected = Some(r.revision);
                q.publish(
                    r.revision,
                    if self.engine.halted().is_some() {
                        "held"
                    } else {
                        "error"
                    },
                    &format!("{e:#}"),
                    self.engine.selected(),
                    false,
                )
            }
        }
    }
}
pub(crate) enum Native {
    Legacy {
        runner: Runner,
        access: NativeAccess,
    },
    VersionTwo {
        runner: crate::ball_gameplay_runtime::Runner,
        revision: u64,
    },
}
impl Native {
    pub fn open(
        root: &Path,
        game: &Path,
        session: &str,
        id: &Identity,
        owner: &Identity,
    ) -> Result<Self> {
        Ok(Self::Legacy {
            runner: Runner::new(root, session, id)?,
            access: NativeAccess::open(game, id, owner)?,
        })
    }
    pub fn open_v2(
        root: &Path,
        game: &Path,
        session: &str,
        id: &Identity,
        owner: &Identity,
    ) -> Result<Self> {
        let binding = crate::ball_gameplay_runtime::Binding::from_launched(
            id.clone(),
            owner.clone(),
            game.to_owned(),
            root.to_owned(),
            session.into(),
        )?;
        Ok(Self::VersionTwo {
            runner: crate::ball_gameplay_runtime::Runner::new(binding, "current")?,
            revision: 0,
        })
    }
    pub fn tick(&mut self, q: &Queue, owner_alive: bool) -> Result<()> {
        match self {
            Self::Legacy { runner, access } => runner.tick(q, access, owner_alive),
            Self::VersionTwo { runner, revision } => {
                let request = q.view()?;
                if *revision != request.revision {
                    if let Err(e) = runner.choose(&request.requested_id) {
                        // A rejected new choice cannot stop restoration of an
                        // already-owned gameplay setting after an uncertain write.
                        runner.tick(owner_alive)?;
                        return q.reject_gameplay(request.revision, runner.status(),
                            &format!("Ball choice not applied; exact-owned restoration is still monitored: {e:#}"));
                    }
                    *revision = request.revision;
                }
                runner.tick(owner_alive)?;
                q.publish_gameplay(*revision, &request.requested_id, runner.status())
            }
        }
    }
    pub fn ended(&mut self, q: &Queue, closed: impl Fn() -> Result<()>) -> Result<()> {
        match self {
            Self::Legacy { runner, .. } => runner.engine.game_ended(&closed)?,
            Self::VersionTwo { runner, .. } => {
                closed()?;
                runner.finish_ended(&crate::session_safety::processes()?)?;
            }
        }
        *q.1.lock()
            .map_err(|_| anyhow::anyhow!("Ball details unavailable"))? = serde_json::Value::Null;
        let r = q.view()?;
        q.publish(
            r.revision,
            "ended",
            "Game ended; memory-only ball substitution cleared.",
            "current",
            true,
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ball_session::{Access, test_support::*};
    fn runner(f: &Fixture) -> Runner {
        Runner::new(&f.root, &"a".repeat(48), &identity()).unwrap()
    }
    fn prepare(r: &mut Runner, q: &Queue, a: &impl Access) {
        q.request(&ball::test_support::id(), || Ok(())).unwrap();
        r.tick(q, a, true).unwrap();
        r.tick(q, a, true).unwrap();
        assert_eq!(q.view().unwrap().phase, "ready");
    }
    #[test]
    fn gameplay_queue_never_confuses_props_or_preparation_with_court_ball_confirmation() {
        let q = Queue::new();
        let id = ball::test_support::id();
        let rev = q.request(&id, || Ok(())).unwrap().revision;
        let mut raw = serde_json::json!({"version2":true,"selected":id,"active":id,
            "phase":"prepared","held":false,"ownsMutation":true,
            "gameplayModelConfirmed":false,"message":"Prepared; visually verify in game"});
        q.publish_gameplay(rev, &id, raw.clone()).unwrap();
        assert_eq!(q.view().unwrap().phase, "ready");
        assert_eq!(q.json().unwrap()["gameplayModelConfirmed"], false);
        assert_eq!(q.json().unwrap()["visualAppearanceProven"], false);
        raw["phase"] = serde_json::json!("gameplay-model-confirmed");
        // A phase string alone is not proof from a typed native court ball.
        q.publish_gameplay(rev, &id, raw.clone()).unwrap();
        assert_eq!(q.view().unwrap().phase, "waiting-for-selection");
        raw["gameplayModelConfirmed"] = serde_json::json!(true);
        q.publish_gameplay(rev, &id, raw.clone()).unwrap();
        assert_eq!(q.view().unwrap().phase, "model-confirmed");
        assert_eq!(q.json().unwrap()["gameplayModelConfirmed"], true);
        assert_eq!(q.json().unwrap()["visualAppearanceProven"], false);
        raw["phase"] = serde_json::json!("waiting-for-release");
        raw["gameplayModelConfirmed"] = serde_json::json!(false);
        q.publish_gameplay(rev, &id, raw.clone()).unwrap();
        assert_eq!(q.view().unwrap().phase, "waiting-for-unload");
        assert_eq!(q.json().unwrap()["gameplayModelConfirmed"], false);
        let newer = q.request("current", || Ok(())).unwrap().revision;
        assert!(newer > rev);
        assert_eq!(q.json().unwrap()["gameplayModelConfirmed"], false);
        // A new request clears the old acknowledgement even while the old
        // model is still owned and waiting for a safe unload.
        raw["selected"] = serde_json::json!("current");
        q.publish_gameplay(newer, "current", raw.clone()).unwrap();
        assert_eq!(q.view().unwrap().phase, "waiting-for-unload");
        assert_eq!(q.json().unwrap()["gameplayModelConfirmed"], false);
        assert!(q.view().unwrap().applied_revision < newer);
        raw["held"] = serde_json::json!(true);
        raw["ownsMutation"] = serde_json::json!(false);
        q.publish_gameplay(newer, "current", raw).unwrap();
        assert_eq!(q.view().unwrap().phase, "held");
        assert_eq!(q.view().unwrap().active_id, "current");
        assert_eq!(q.json().unwrap()["gameplayModelConfirmed"], false);
    }
    #[test]
    fn held_gameplay_worker_reports_completed_restore_without_acknowledging_rejected_choice() {
        let q = Queue::new();
        let id = ball::test_support::id();
        let old = q.request(&id, || Ok(())).unwrap().revision;
        q.publish(old, "ready", "prepared", &id, true).unwrap();
        let newer = q.request("current", || Ok(())).unwrap().revision;
        let mut raw = serde_json::json!({"version2":true,"selected":id,"active":id,
            "held":true,"ownsMutation":true,"phase":"held","message":"uncertain"});
        q.reject_gameplay(
            newer,
            raw.clone(),
            "New request rejected; restoring owned ball",
        )
        .unwrap();
        assert_eq!(q.view().unwrap().active_id, id);
        assert_eq!(q.view().unwrap().applied_revision, old);
        raw["active"] = serde_json::Value::Null;
        raw["ownsMutation"] = serde_json::json!(false);
        raw["phase"] = serde_json::json!("restored-held");
        q.reject_gameplay(
            newer,
            raw,
            "Owned original confirmed; restart before retrying",
        )
        .unwrap();
        let s = q.view().unwrap();
        assert_eq!(s.phase, "held");
        assert_eq!(s.requested_id, "current");
        assert_eq!(s.active_id, "current");
        assert_eq!(s.applied_revision, old);
        assert!(s.applied_revision < newer);
        assert_eq!(q.json().unwrap()["gameplayModelConfirmed"], false);
    }
    #[test]
    fn gameplay_queue_rejects_cross_choice_results_and_unowned_prepared_success() {
        let id = ball::test_support::id();
        let q = Queue::new();
        let raw = serde_json::json!({"version2":true,"selected":id,"active":"current",
            "phase":"prepared","held":false,"ownsMutation":false,"message":"Not prepared"});
        assert_eq!(
            gameplay_status(&id, &raw).unwrap().0,
            "waiting-for-selection"
        );
        assert!(q.publish_gameplay(0, "current", raw).is_err());
        assert_eq!(q.view().unwrap().phase, "current");
        assert_eq!(q.json().unwrap()["details"], serde_json::Value::Null);
    }
    #[test]
    fn fresh_session_is_current_only_and_queue_requires_exact_id_and_fresh_owner() {
        let q = Queue::new();
        assert_eq!(q.view().unwrap().requested_id, "current");
        assert_eq!(
            q.json().unwrap()["requestedName"],
            ball::choice("current").unwrap().name
        );
        assert!(q.request("CURRENT", || Ok(())).is_err());
        assert!(
            q.request(&ball::test_support::id(), || anyhow::bail!("Game ended"))
                .is_err()
        );
        assert_eq!(q.view().unwrap().revision, 0);
        q.publish(0, "held", "uncertain", "current", false).unwrap();
        assert!(q.request("current", || Ok(())).is_err());
    }
    #[test]
    fn load_match_then_quick_play_restore_and_reselection_rearm_preserve_the_choice() {
        let f = Fixture::new();
        let a = f.access();
        let q = Queue::new();
        let mut r = runner(&f);
        prepare(&mut r, &q, &a);
        a.loaded(true);
        a.mode(0xB4D37B22, 1);
        r.tick(&q, &a, true).unwrap();
        assert_eq!(q.view().unwrap().phase, "resource-loaded");
        assert_eq!(a.writes.get(), 1);
        a.mode(0x079964C0, 3);
        r.tick(&q, &a, true).unwrap();
        assert_eq!(a.writes.get(), 1);
        a.loaded(false);
        r.tick(&q, &a, true).unwrap();
        assert_eq!(q.view().unwrap().phase, "restored");
        assert_eq!(a.writes.get(), 2);
        a.mode(0x92DF06DE, 3);
        r.tick(&q, &a, true).unwrap();
        r.tick(&q, &a, true).unwrap();
        assert_eq!(q.view().unwrap().phase, "ready");
        assert_eq!(a.writes.get(), 3);
    }
    #[test]
    fn spectator_resource_load_is_not_gameplay_success_and_keeps_safe_restore_polling() {
        // The approved QA 0.34 observation had a loaded Moneyball Normal
        // resource, but GameData selected Version 2 DEFAULT (0x42A7A5EF).
        for gameplay in [0x42A7A5EFu32, 0, ball::DEFAULT, 0xDEADBEEF] {
            for change in [false, true] {
                let f = Fixture::new();
                let a = f.access();
                let q = Queue::new();
                let mut r = runner(&f);
                prepare(&mut r, &q, &a);
                a.loaded(true);
                {
                    let mut s = a.snapshot.borrow_mut();
                    s.context.root_tag = s.image + 0x0791E158;
                    s.context.current = 0x3EEF98B9;
                    s.context.alternate = s.context.current;
                    s.context.stage = 1;
                    s.context.active = 1;
                    s.context.kind = 1;
                    s.context.game_mode = 6;
                    s.native_ball = gameplay;
                }
                for _ in 0..3 {
                    r.tick(&q, &a, true).unwrap();
                    let status = q.view().unwrap();
                    assert_eq!(status.phase, "incompatible");
                    assert!(status.message.contains("not proof"));
                    assert!(r.engine.halted().is_none());
                    assert_eq!(a.writes.get(), 1);
                }
                if change {
                    q.request("current", || Ok(())).unwrap();
                }
                {
                    let mut s = a.snapshot.borrow_mut();
                    s.context.current = 0x079964C0;
                    s.context.alternate = s.context.current;
                    s.context.active = 0;
                    s.context.kind = 0;
                }
                // Menu visibility alone never permits changing a loaded prop.
                r.tick(&q, &a, true).unwrap();
                assert_eq!(a.writes.get(), 1);
                assert!(crate::ball_session::pending(&f.root).unwrap());
                a.loaded(false);
                r.tick(&q, &a, true).unwrap();
                assert_eq!(q.view().unwrap().phase, "restored");
                assert_eq!(a.writes.get(), 2);
                assert!(!crate::ball_session::pending(&f.root).unwrap());
            }
        }
    }
    #[test]
    fn unresolved_v2_gameplay_choice_does_not_authorize_a_normal_prop_write() {
        for gameplay in [0, ball::DEFAULT, 0x42A7A5EF] {
            let f = Fixture::new();
            let a = f.access();
            let q = Queue::new();
            let mut r = runner(&f);
            {
                let mut s = a.snapshot.borrow_mut();
                s.context.root_tag = s.image + 0x0791E158;
                s.native_ball = gameplay;
            }
            q.request(&ball::test_support::id(), || Ok(())).unwrap();
            for _ in 0..5 {
                r.tick(&q, &a, true).unwrap();
            }
            assert_eq!(q.view().unwrap().phase, "incompatible");
            assert_eq!(a.writes.get(), 0);
            assert!(!crate::ball_session::pending(&f.root).unwrap());
        }
    }
    #[test]
    fn change_or_ui_close_queues_restore_until_unloaded_without_erasing_latest_request() {
        for close in [false, true] {
            let f = Fixture::new();
            let a = f.access();
            let q = Queue::new();
            let mut r = runner(&f);
            prepare(&mut r, &q, &a);
            a.loaded(true);
            a.mode(0xB4D37B22, 1);
            if !close {
                q.request("current", || Ok(())).unwrap();
            }
            r.tick(&q, &a, !close).unwrap();
            assert_eq!(q.view().unwrap().phase, "waiting-for-unload");
            assert_eq!(a.writes.get(), 1);
            a.mode(0x6509837B, 3);
            a.loaded(false);
            r.tick(&q, &a, !close).unwrap();
            assert_eq!(a.writes.get(), 2);
            assert_eq!(q.view().unwrap().active_id, "current");
            r.tick(&q, &a, !close).unwrap();
            assert_eq!(q.view().unwrap().phase, "current");
        }
    }
    #[test]
    fn unavailable_choice_is_retryable_but_no_repeated_failed_writes_or_loaded_rearming() {
        let f = Fixture::new();
        let a = f.access();
        let q = Queue::new();
        let mut r = runner(&f);
        a.source.set(false);
        q.request(&ball::test_support::id(), || Ok(())).unwrap();
        for _ in 0..5 {
            r.tick(&q, &a, true).unwrap();
        }
        assert_eq!(q.view().unwrap().phase, "error");
        assert_eq!(a.writes.get(), 0);
        a.source.set(true);
        for _ in 0..5 {
            r.tick(&q, &a, true).unwrap();
        }
        assert_eq!(a.writes.get(), 0);
        q.request(&ball::test_support::id(), || Ok(())).unwrap();
        r.tick(&q, &a, true).unwrap();
        assert_eq!(a.writes.get(), 1);
        a.snapshot.borrow_mut().record[0] ^= 1;
        r.tick(&q, &a, true).unwrap();
        assert_eq!(q.view().unwrap().phase, "held");
        assert_eq!(a.writes.get(), 1);
    }
    #[test]
    fn out_of_order_status_never_erases_a_newer_catalog_request() {
        let q = Queue::new();
        q.request(&ball::test_support::id(), || Ok(())).unwrap();
        q.request("current", || Ok(())).unwrap();
        q.publish(1, "ready", "old result", &ball::test_support::id(), true)
            .unwrap();
        let s = q.view().unwrap();
        assert_eq!(s.requested_id, "current");
        assert_eq!(s.phase, "queued");
        assert_eq!(s.revision, 2);
        assert_eq!(s.applied_revision, 1);
    }
    #[test]
    fn safe_menu_transition_before_write_retries_without_losing_active_restoration_ownership() {
        let f = Fixture::new();
        let a = f.access();
        let q = Queue::new();
        let mut r = runner(&f);
        q.request(&ball::test_support::id(), || Ok(())).unwrap();
        a.deferred.set(true);
        r.tick(&q, &a, true).unwrap();
        r.tick(&q, &a, true).unwrap();
        assert_eq!(q.view().unwrap().phase, "waiting-for-selection");
        assert_eq!(a.writes.get(), 0);
        assert!(!crate::ball_session::pending(&f.root).unwrap());
        assert!(r.engine.halted().is_none());
        a.deferred.set(false);
        r.tick(&q, &a, true).unwrap();
        r.tick(&q, &a, true).unwrap();
        assert_eq!(a.writes.get(), 1);
        a.mode(0x079964C0, 3);
        a.deferred.set(true);
        r.tick(&q, &a, true).unwrap();
        assert_eq!(q.view().unwrap().phase, "waiting-for-unload");
        assert!(crate::ball_session::pending(&f.root).unwrap());
        assert!(r.engine.halted().is_none());
        a.deferred.set(false);
        r.tick(&q, &a, true).unwrap();
        assert_eq!(a.writes.get(), 2);
        assert!(!crate::ball_session::pending(&f.root).unwrap());
    }
}
