//! Nonblocking selection-to-match carry-over. Readiness must be stable before
//! one frozen submission; rearming requires selection observations AND the
//! next native ARMED report. An uncertain rebuild is never automatically retried.
use super::*;
use crate::{
    blacktop_native,
    blacktop_outfit_match::{self as policy, MatchPlayer, Stable},
};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Boundary {
    Selection,
    Match,
    Menu,
    #[default]
    Other,
}
#[derive(Debug)]
struct Deferred;
impl std::fmt::Display for Deferred {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Blacktop match changed before outfit handoff")
    }
}
impl std::error::Error for Deferred {}
/// One fresh initial observation; it cannot reserve choices or publish files.
pub(crate) enum Readiness {
    LineupWaiting,
    NoChoices,
    NotReady(String),
    Ready {
        lineup: Vec<u64>,
        choices: Vec<Choice>,
        players: Vec<MatchPlayer>,
    },
}
pub(crate) struct Sample {
    pub boundary: Boundary,
    /// Measured immediately after observing Match, before loaded actor reads.
    pub match_after_ms: Option<u64>,
    pub readiness_ms: u64,
    pub readiness: Option<Readiness>,
}
fn elapsed_ms(started: std::time::Instant) -> u64 {
    started.elapsed().as_millis().min(u64::MAX as u128) as u64
}
pub(crate) trait Driver {
    fn check(&self) -> Result<()>;
    fn boundary(&self) -> Result<Boundary>;
    fn lineup(&self) -> Result<Vec<u64>>;
    fn ready(&self, choices: &[Choice]) -> Result<Vec<MatchPlayer>>;
    /// Default retains legacy check/boundary/lineup/ready order exactly.
    fn sample(
        &self,
        allow_ready: bool,
        choose: impl FnOnce(&[u64]) -> Result<Vec<Choice>>,
    ) -> Result<Sample> {
        let started = std::time::Instant::now();
        self.check()?;
        let boundary = self.boundary()?;
        let mut sample = Sample {
            boundary,
            match_after_ms: (boundary == Boundary::Match).then(|| elapsed_ms(started)),
            readiness_ms: 0,
            readiness: None,
        };
        if !allow_ready || boundary != Boundary::Match {
            return Ok(sample);
        }
        let lineup = match self.lineup() {
            Ok(value) => value,
            Err(_) => {
                sample.readiness = Some(Readiness::LineupWaiting);
                return Ok(sample);
            }
        };
        let choices = choose(&lineup)?;
        if choices.is_empty() {
            sample.readiness = Some(Readiness::NoChoices);
            return Ok(sample);
        }
        let ready_started = std::time::Instant::now();
        sample.readiness = Some(match self.ready(&choices) {
            Ok(players) => Readiness::Ready {
                lineup,
                choices,
                players,
            },
            Err(error) => Readiness::NotReady(format!("{error:#}")),
        });
        sample.readiness_ms = elapsed_ms(ready_started);
        Ok(sample)
    }
    /// Revalidate independently at every existing durable handoff boundary.
    /// Legacy drivers retain their original full proof sequence.
    fn handoff(&self, choices: &[Choice], lineup: &[u64], players: &[MatchPlayer]) -> Result<bool> {
        self.check()?;
        Ok(matches!(
            (|| -> Result<bool> {
                Ok(self.boundary()? == Boundary::Match
                    && self.lineup()? == lineup
                    && self.ready(choices)? == players)
            })(),
            Ok(true)
        ))
    }
}
impl Driver for crate::blacktop_native::outfit_writer::Writer {
    fn check(&self) -> Result<()> {
        self.check_owner()
    }
    fn boundary(&self) -> Result<Boundary> {
        self.check_owner()?;
        let r = self.reader();
        // Mode-owner pointers/indexes can be transient during ordinary menu
        // loading. Without a trusted context we wait; build/owner failures from
        // check_owner above still stop the feature before any write.
        let Ok(c) = blacktop_native::context(r) else {
            return Ok(Boundary::Other);
        };
        Ok(
            match blacktop_native::boundary(&c, blacktop_native::Memory::base(r)) {
                "selection" if outfits::d(r, c.owner + 0xA04)? == 6 => Boundary::Selection,
                "match" => Boundary::Match,
                "main-menu" | "quick-play" => Boundary::Menu,
                _ => Boundary::Other,
            },
        )
    }
    fn lineup(&self) -> Result<Vec<u64>> {
        self.check_owner()?;
        policy::lineup(self.reader())
    }
    fn ready(&self, c: &[Choice]) -> Result<Vec<MatchPlayer>> {
        self.check_owner()?;
        policy::capture(self.reader(), c)
    }
}
#[derive(Default)]
struct Rearm {
    samples: u8,
    last: Option<u64>,
}
impl Rearm {
    fn observe(&mut self, now: u64, selection: bool) -> bool {
        if !selection {
            self.samples = 0;
            self.last = None;
            return false;
        }
        if let Some(last) = self.last {
            if now < last {
                self.samples = 0;
                self.last = None;
            } else if now - last < 500 {
                return self.samples >= 4;
            }
        }
        self.last = Some(now);
        self.samples = self.samples.saturating_add(1);
        self.samples >= 4
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(
    tag = "phase",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub(crate) enum Status {
    Selecting,
    Menu,
    WaitingForMatch,
    /// Choices exist for this match but the loaded players are not a valid
    /// carry source yet; reads retry, writes never do. Carries the reason so
    /// the overlay is never silently stuck.
    NotReady {
        reason: String,
    },
    NoChoices,
    WaitingForWorker {
        number: u32,
    },
    Applying {
        number: u32,
        rebuilt: u32,
    },
    Applied {
        number: u32,
        players: u32,
    },
    Rearming {
        number: u32,
    },
    SelectionRefreshing,
    SelectionUpdated {
        slot: u8,
        name: String,
        carry_over: bool,
    },
    SelectionUpdatedMany {
        players: Vec<crate::outfit_selection_session::SlotUpdate>,
        timing: crate::outfit_selection_session::BatchTiming,
    },
}
struct Active {
    file: match_files::Pending,
    number: u32,
    submitted: u64,
}
/// In-memory measurements only. The owning worker can retain a snapshot when
/// status changes; polling never adds a journal frame or filesystem flush.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Timing {
    pub first_match_ms: Option<u64>,
    pub first_ready_ms: Option<u64>,
    pub stable_admission_ms: Option<u64>,
    pub freeze_begin_ms: Option<u64>,
    pub freeze_end_ms: Option<u64>,
    pub publish_begin_ms: Option<u64>,
    pub publish_end_ms: Option<u64>,
    pub submitted_unix_ms: Option<u64>,
    pub ready_samples: u64,
    pub readiness_failures: u64,
    pub nonmatch_samples: u64,
    pub deferred_publications: u64,
    pub longest_ready_read_ms: u64,
}
fn measured_ms(now: u64, started: std::time::Instant) -> u64 {
    now.saturating_add(started.elapsed().as_millis().min(u64::MAX as u128) as u64)
}
#[derive(Default)]
pub(crate) struct Coordinator {
    active: Option<Active>,
    stable: Stable,
    rearm: Rearm,
    needs_rearm: bool,
    waiting_arm: Option<u64>,
    last: Option<u64>,
    held: Option<String>,
    closed: bool,
    menu: bool,
    observed: Boundary,
    timing: Timing,
}
impl Coordinator {
    pub fn timing(&self) -> &Timing {
        &self.timing
    }
    pub fn held(&self) -> Option<&str> {
        self.held.as_deref()
    }
    pub fn busy(&self) -> bool {
        self.active.is_some()
    }
    pub fn at_menu(&self) -> bool {
        self.menu
    }
    pub fn observed(&self) -> Boundary {
        self.observed
    }
    pub fn close(&mut self) {
        self.active.take();
        self.stable.reset();
        self.closed = true;
    }
    pub fn tick(&mut self, driver: &impl Driver, store: &mut Store, now: u64) -> Result<Status> {
        ensure!(!self.closed, "Outfit match coordinator is closed");
        ensure!(
            self.held.is_none() && store.halted().is_none(),
            "Outfit carry-over is held; no automatic retry"
        );
        let result = self.tick_inner(driver, store, now);
        if let Err(e) = &result {
            self.active.take();
            self.stable.reset();
            self.held = Some(format!("Outfit carry-over stopped without retry: {e:#}"));
        }
        result
    }
    fn tick_inner(&mut self, driver: &impl Driver, store: &mut Store, now: u64) -> Result<Status> {
        let tick_started = std::time::Instant::now();
        ensure!(
            self.last.is_none_or(|last| now >= last),
            "Outfit match clock moved backwards"
        );
        self.last = Some(now);
        if let Some(active) = &mut self.active {
            driver.check()?;
            ensure!(
                now - active.submitted <= 650_000,
                "Native outfit rebuild confirmation timed out"
            );
            let report = store.poll_match(&mut active.file, || driver.check())?;
            if let Some(r) = report {
                if r.phase == match_files::Phase::Rebuilt {
                    let number = active.number;
                    let players = r.rebuilt;
                    self.active.take();
                    self.needs_rearm = true;
                    self.rearm = Rearm::default();
                    self.stable.reset();
                    return Ok(Status::Applied { number, players });
                }
                return Ok(Status::Applying {
                    number: active.number,
                    rebuilt: r.rebuilt,
                });
            }
            return Ok(Status::Applying {
                number: active.number,
                rebuilt: 0,
            });
        }
        // The callback borrows the ledger only for this read-only sample; it
        // cannot outlive sample(), and ends before reserve_match below.
        let sample = driver.sample(!self.needs_rearm, |lineup| store.ledger().fresh(lineup))?;
        let boundary = sample.boundary;
        self.observed = boundary;
        self.menu = boundary == Boundary::Menu;
        // A new selection/menu visit starts a new measurement. Transient
        // non-match observations remain visible in this match's counters.
        if matches!(boundary, Boundary::Selection | Boundary::Menu) {
            self.timing = Timing::default();
        }
        if boundary == Boundary::Menu {
            store.clear_pending()?;
        }
        if self.needs_rearm {
            let number = store.next_match()?;
            if !self.rearm.observe(now, boundary == Boundary::Selection) {
                return Ok(Status::Rearming { number });
            }
            match store.worker_report(number)? {
                Some(r) if r.phase == match_files::Phase::Armed => {
                    self.needs_rearm = false;
                    self.waiting_arm = None;
                }
                Some(r)
                    if matches!(
                        r.phase,
                        match_files::Phase::Failed | match_files::Phase::Inconclusive
                    ) =>
                {
                    anyhow::bail!("Native outfit rearm failed: {}", r.reason)
                }
                _ => return Ok(Status::Rearming { number }),
            }
        }
        if boundary != Boundary::Match {
            if self.timing.first_match_ms.is_some() {
                self.timing.nonmatch_samples = self.timing.nonmatch_samples.saturating_add(1);
            }
            self.stable.reset();
            self.waiting_arm = None;
            return Ok(match boundary {
                Boundary::Selection => Status::Selecting,
                Boundary::Menu => Status::Menu,
                _ => Status::WaitingForMatch,
            });
        }
        self.timing
            .first_match_ms
            .get_or_insert_with(|| now.saturating_add(sample.match_after_ms.unwrap_or(0)));
        self.timing.longest_ready_read_ms =
            self.timing.longest_ready_read_ms.max(sample.readiness_ms);
        let (lineup, choices, players) = match sample.readiness {
            Some(Readiness::Ready {
                lineup,
                choices,
                players,
            }) => (lineup, choices, players),
            Some(Readiness::NoChoices) => {
                self.stable.reset();
                self.waiting_arm = None;
                return Ok(Status::NoChoices);
            }
            Some(Readiness::NotReady(reason)) => {
                self.timing.readiness_failures = self.timing.readiness_failures.saturating_add(1);
                self.stable.reset();
                return Ok(Status::NotReady {
                    reason: reason.chars().take(300).collect(),
                });
            }
            Some(Readiness::LineupWaiting) | None => {
                self.stable.reset();
                return Ok(Status::WaitingForMatch);
            }
        };
        self.timing.ready_samples = self.timing.ready_samples.saturating_add(1);
        self.timing
            .first_ready_ms
            .get_or_insert_with(|| measured_ms(now, tick_started));
        if !self.stable.observe(now, players.clone(), choices.clone()) {
            return Ok(Status::WaitingForMatch);
        }
        self.timing
            .stable_admission_ms
            .get_or_insert_with(|| measured_ms(now, tick_started));
        let number = store.next_match()?;
        match store.worker_report(number)? {
            Some(r) if r.phase == match_files::Phase::Armed => {}
            Some(r)
                if matches!(
                    r.phase,
                    match_files::Phase::Failed | match_files::Phase::Inconclusive
                ) =>
            {
                anyhow::bail!("Native outfit worker failed: {}", r.reason)
            }
            _ => {
                let start = *self.waiting_arm.get_or_insert(now);
                // WPF waited 10 s for ARMED too, but its worker always ran.
                // Nothing is reserved, written or consumed before ARMED, so a
                // late or absent worker is a visible retryable wait, never a
                // hold for the rest of the game.
                if now - start > 10_000 {
                    return Ok(Status::NotReady {
                        reason: format!(
                            "native outfit worker {number} has not armed after {} s; nothing was written, and carry-over starts if it arms",
                            (now - start) / 1000
                        ),
                    });
                }
                return Ok(Status::WaitingForWorker { number });
            }
        }
        let check = || -> Result<()> {
            if !driver.handoff(&choices, &lineup, &players)? {
                return Err(Deferred.into());
            }
            Ok(())
        };
        self.timing.freeze_begin_ms = Some(measured_ms(now, tick_started));
        let reserved = store.reserve_match(&choices, &check);
        self.timing.freeze_end_ms = Some(measured_ms(now, tick_started));
        let frozen = match reserved {
            Ok(f) => f,
            Err(e)
                if e.downcast_ref::<Deferred>().is_some()
                    && choices
                        .iter()
                        .all(|c| !store.ledger().consumed.contains(&c.choice_id)) =>
            {
                self.timing.deferred_publications =
                    self.timing.deferred_publications.saturating_add(1);
                self.stable.reset();
                self.waiting_arm = None;
                return Ok(Status::WaitingForMatch);
            }
            Err(e) => return Err(e),
        };
        self.timing.publish_begin_ms = Some(measured_ms(now, tick_started));
        let publication = store.publish_match(frozen, &check);
        self.timing.publish_end_ms = Some(measured_ms(now, tick_started));
        let file = publication?;
        self.timing.submitted_unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_millis().min(u64::MAX as u128) as u64);
        self.active = Some(Active {
            file,
            number,
            submitted: now,
        });
        self.stable.reset();
        self.waiting_arm = None;
        Ok(Status::Applying { number, rebuilt: 0 })
    }
}
#[cfg(test)]
#[path = "outfit_match_runtime_tests.rs"]
mod tests;
