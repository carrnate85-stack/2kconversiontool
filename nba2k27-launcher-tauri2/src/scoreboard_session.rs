//! Functional scoreboard lifecycle through injected exact-retained IO. This is
//! deliberately unwired: no process is opened and no filesystem is accessed.
//! The host supplies a per-game identity-bound, 12-byte-only write capability
//! and a durable journal. All errors preserve the owned/uncertain transaction.
#![allow(dead_code)]
use crate::{
    blacktop_arena::Inspect,
    blacktop_native::ContextState,
    rules_v2,
    scoreboard_profile::{self as profile, Change, Plan},
    scoreboard_v2::{self as native, Snapshot},
};
use anyhow::{Result, ensure};
pub(crate) trait Executor: Inspect {
    /// Implement only on a retained, exact PID/birth/exe session capability. The
    /// native writer must independently check identity, private RW allocation,
    /// header allowlist and the supplied full guard before WriteProcessMemory.
    fn write_header(&mut self, permit: &Permit<'_>, durable: &dyn Fn() -> Result<()>)
    -> Result<()>;
}
pub(crate) trait Journal {
    fn intent(&mut self, snapshot: &Snapshot, plan: &Plan) -> Result<()>;
    fn restore_intent(
        &mut self,
        snapshot: &Snapshot,
        plan: &Plan,
        applied: usize,
        restored: usize,
    ) -> Result<()>;
    fn verify(&self, permit: &Permit<'_>) -> Result<()>;
    fn progress(&mut self, applied: usize, restored: usize) -> Result<()>;
    fn resolved(&mut self) -> Result<()>;
}
pub(crate) struct Permit<'a> {
    snapshot: &'a Snapshot,
    plan: &'a Plan,
    applied: usize,
    restored: usize,
    undo: bool,
}
impl Permit<'_> {
    pub(crate) fn snapshot(&self) -> &Snapshot {
        self.snapshot
    }
    pub(crate) fn plan(&self) -> &Plan {
        self.plan
    }
    pub(crate) fn undo(&self) -> bool {
        self.undo
    }
    pub(crate) fn progress(&self) -> (usize, usize) {
        (self.applied, self.restored)
    }
    pub(crate) fn change(&self) -> Result<Change> {
        ensure!(
            self.snapshot.plan()? == *self.plan && self.applied <= self.plan.changes.len(),
            "Scoreboard publication plan escaped its captured descriptor"
        );
        if self.undo {
            ensure!(
                self.restored < self.applied,
                "Scoreboard restore permit exceeds owned prefix"
            );
            let c = &self.plan.changes[self.applied - 1 - self.restored];
            let mut before = c.before.clone();
            before[..12].copy_from_slice(&c.after);
            Ok(Change {
                address: c.address,
                before,
                after: c.before[..12].to_vec(),
            })
        } else {
            ensure!(
                self.applied < self.plan.changes.len() && self.restored == 0,
                "Scoreboard apply permit exceeds plan"
            );
            Ok(self.plan.changes[self.applied].clone())
        }
    }
    pub(crate) fn verify_before(&self, m: &impl Inspect) -> Result<()> {
        self.change()?;
        if self.undo {
            native::guard_restore(m, self.snapshot, self.plan, self.applied, self.restored)
        } else {
            native::guard_progress(m, self.snapshot, self.plan, self.applied)
        }
    }
    pub(crate) fn verify_after(&self, m: &impl Inspect) -> Result<()> {
        self.change()?;
        if self.undo {
            native::guard_restore(m, self.snapshot, self.plan, self.applied, self.restored + 1)
        } else {
            native::guard_progress(m, self.snapshot, self.plan, self.applied + 1)
        }
    }
}
#[cfg(test)]
impl<'a> Permit<'a> {
    pub(crate) fn fixture(
        snapshot: &'a Snapshot,
        plan: &'a Plan,
        applied: usize,
        restored: usize,
        undo: bool,
    ) -> Self {
        Self {
            snapshot,
            plan,
            applied,
            restored,
            undo,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    Waiting,
    Ready,
    Restored,
    Held(String),
}
#[derive(Clone, Debug)]
struct Owned {
    snapshot: Snapshot,
    plan: Plan,
    applied: usize,
    restored: usize,
    failed: bool,
}
pub(crate) struct Service {
    desired: String,
    owned: Option<Owned>,
    stable: Option<ContextState>,
}
impl Service {
    pub(crate) fn new(id: &str) -> Result<Self> {
        profile::choice(id)?;
        Ok(Self {
            desired: id.into(),
            owned: None,
            stable: None,
        })
    }
    pub(crate) fn choose(&mut self, id: &str) -> Result<()> {
        profile::choice(id)?;
        self.desired = id.into();
        Ok(())
    }
    pub(crate) fn owns_mutation(&self) -> bool {
        self.owned.is_some()
    }
    /// Only call after the retained game handle confirms that exact process has
    /// exited. Memory dies with it; no old address is carried into the next game.
    pub(crate) fn game_ended(&mut self, journal: &mut impl Journal) -> Result<()> {
        journal.resolved()?;
        self.owned = None;
        self.stable = None;
        Ok(())
    }
    pub(crate) fn tick(
        &mut self,
        io: &mut impl Executor,
        journal: &mut impl Journal,
        layout_ready: bool,
    ) -> Status {
        self.tick_owned(io, journal, layout_ready, true)
    }
    pub(crate) fn tick_owned(
        &mut self,
        io: &mut impl Executor,
        journal: &mut impl Journal,
        layout_ready: bool,
        parent_alive: bool,
    ) -> Status {
        match self.try_tick(io, journal, layout_ready, parent_alive) {
            Ok(status) => status,
            Err(e) => {
                if let Some(o) = self.owned.as_mut() {
                    o.failed = true;
                }
                Status::Held(format!("{e:#}"))
            }
        }
    }
    fn try_tick(
        &mut self,
        io: &mut impl Executor,
        journal: &mut impl Journal,
        layout_ready: bool,
        parent_alive: bool,
    ) -> Result<Status> {
        native::verify_profile(io)?;
        let context = rules_v2::context(io)?;
        let boundary = rules_v2::boundary(&context, io.base());
        let stable = self.stable.as_ref() == Some(&context);
        self.stable = Some(context);
        if !stable || !["selection", "main-menu", "quick-play"].contains(&boundary) {
            return Ok(Status::Waiting);
        }
        if let Some(o) = self.owned.as_mut() {
            if boundary == "selection"
                && self.desired == o.snapshot.choice().id
                && !o.failed
                && o.restored == 0
            {
                native::guard_progress(io, &o.snapshot, &o.plan, o.applied)?;
                ensure!(
                    o.applied == o.plan.changes.len(),
                    "Scoreboard apply is incomplete"
                );
                return Ok(Status::Ready);
            }
            // Re-capture directory/list ownership even for a zero-step intent. A foreign
            // writer cannot be adopted merely because its first header looks familiar.
            if o.restored == 0 {
                native::restore_plan(io, &o.snapshot, &o.plan, o.applied)?;
            }
            // Prove the exact current prefix BEFORE journaling: a held restore is
            // retried every safe-menu tick, and a persistent guard failure must
            // not append one intent per tick until the journal can never resolve.
            native::guard_restore(io, &o.snapshot, &o.plan, o.applied, o.restored)?;
            journal.restore_intent(&o.snapshot, &o.plan, o.applied, o.restored)?;
            while o.restored < o.applied {
                native::guard_restore(io, &o.snapshot, &o.plan, o.applied, o.restored)?;
                let permit = Permit {
                    snapshot: &o.snapshot,
                    plan: &o.plan,
                    applied: o.applied,
                    restored: o.restored,
                    undo: true,
                };
                let result = io.write_header(&permit, &|| journal.verify(&permit));
                // A failed native call may still have completed. Observe the exact prefix
                // before recording ownership; a partial/foreign header is held, not guessed.
                let confirmed =
                    native::guard_restore(io, &o.snapshot, &o.plan, o.applied, o.restored + 1)
                        .is_ok();
                if confirmed {
                    o.restored += 1;
                    journal.progress(o.applied, o.restored)?;
                }
                result?;
                ensure!(
                    confirmed,
                    "Scoreboard restore write was not fully read-back verified"
                );
                native::guard_restore(io, &o.snapshot, &o.plan, o.applied, o.restored)?;
            }
            native::guard_restore(io, &o.snapshot, &o.plan, o.applied, o.restored)?;
            journal.resolved()?;
            self.owned = None;
            // Never restore+reapply in one poll: a fresh stable selection observation is
            // required after complete rollback (including a changed picker selection).
            self.stable = None;
            return Ok(Status::Restored);
        }
        if boundary != "selection" || !parent_alive {
            return Ok(Status::Waiting);
        }
        let snapshot = native::capture(io, &self.desired, layout_ready)?;
        let plan = snapshot.plan()?;
        if plan.changes.is_empty() {
            return Ok(Status::Ready);
        }
        self.owned = Some(Owned {
            snapshot,
            plan,
            applied: 0,
            restored: 0,
            failed: false,
        });
        let o = self.owned.as_mut().unwrap();
        // Durable ownership precedes EVERY possible first mutation. An intent error
        // leaves the lease retained; it must be resolved rather than silently lost.
        journal.intent(&o.snapshot, &o.plan)?;
        native::guard_progress(io, &o.snapshot, &o.plan, 0)?;
        while o.applied < o.plan.changes.len() {
            native::guard_progress(io, &o.snapshot, &o.plan, o.applied)?;
            let permit = Permit {
                snapshot: &o.snapshot,
                plan: &o.plan,
                applied: o.applied,
                restored: 0,
                undo: false,
            };
            let result = io.write_header(&permit, &|| journal.verify(&permit));
            let confirmed = native::guard_progress(io, &o.snapshot, &o.plan, o.applied + 1).is_ok();
            if confirmed {
                o.applied += 1;
                journal.progress(o.applied, 0)?;
            }
            result?;
            ensure!(
                confirmed,
                "Scoreboard apply write was not fully read-back verified"
            );
            native::guard_progress(io, &o.snapshot, &o.plan, o.applied)?;
        }
        Ok(Status::Ready)
    }
}
