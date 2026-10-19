//! Nonblocking native selection lifecycle. Tokens expose no pointers, a token
//! is consumed once, and only an observed refresh can create a match choice.
//! The provider transport/automatic-fitting policy is a separate integration.
#![allow(dead_code)]
use crate::{
    backend,
    blacktop_outfits::{self as outfits, Composed, Page, Player, Selection},
    outfit_journal::{self, Kind, Prepared, Ready, Reservation, Submission},
    session_safety::Identity,
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::path::Path;

const TOKEN_MS: u64 = 3000;
const REFRESH_MS: u64 = 3000;
pub(crate) trait Driver {
    type Receipt;
    fn identity(&self) -> &Identity;
    fn capture(&self, pages: &[Page]) -> Result<Selection>;
    fn prepare(&self, r: Reservation, p: &Player, c: &Composed, pages: &[Page])
    -> Result<Prepared>;
    fn unchanged(
        &self,
        r: Ready,
        p: &Player,
        c: &Composed,
        pages: &[Page],
    ) -> Result<Self::Receipt>;
    fn publish(&self, s: Submission, p: &Player, pages: &[Page]) -> Result<Self::Receipt>;
    fn consumed(&self, r: &Self::Receipt, pages: &[Page]) -> Result<bool>;
}
type NativeWriter = crate::blacktop_native::outfit_writer::Writer;
impl Driver for NativeWriter {
    type Receipt = crate::blacktop_native::outfit_writer::Receipt;
    fn identity(&self) -> &Identity {
        &self.reader().identity
    }
    fn capture(&self, pages: &[Page]) -> Result<Selection> {
        outfits::capture(self.reader(), self.identity(), pages)
    }
    fn prepare(
        &self,
        r: Reservation,
        p: &Player,
        c: &Composed,
        pages: &[Page],
    ) -> Result<Prepared> {
        NativeWriter::prepare(self, r, p, c, pages)
    }
    fn unchanged(
        &self,
        r: Ready,
        p: &Player,
        c: &Composed,
        pages: &[Page],
    ) -> Result<Self::Receipt> {
        NativeWriter::unchanged(self, r, p, c, pages)
    }
    fn publish(&self, s: Submission, p: &Player, pages: &[Page]) -> Result<Self::Receipt> {
        NativeWriter::publish(self, s, p, pages)
    }
    fn consumed(&self, r: &Self::Receipt, pages: &[Page]) -> Result<bool> {
        NativeWriter::consumed(self, r, pages)
    }
}
struct Lease {
    token: String,
    player: Player,
    created: u64,
}
struct Pending<R> {
    id: String,
    receipt: R,
    started: u64,
    slot: u8,
    name: String,
    carry_over: bool,
}
/// Random all: every slot written back to back, then one combined wait.
/// Each slot keeps its own journal intent, receipt and readback proof.
struct Batch<R> {
    items: Vec<Pending<R>>,
    done: Vec<SlotUpdate>,
    started: u64,
    written: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SlotUpdate {
    pub slot: u8,
    pub name: String,
    pub carry_over: bool,
}
/// Timing of one combined write, for the overlay status (ms).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BatchTiming {
    pub write_ms: u64,
    pub refresh_ms: u64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlayerView {
    pub binding: String,
    pub token: String,
    pub slot: u8,
    pub name: String,
    pub cyberface: u16,
    pub pool: Option<String>,
    pub current_items: Vec<String>,
    pub current_outfit: Option<backend::Outfit>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SelectionView {
    pub required: u32,
    pub selected: u32,
    pub players: Vec<PlayerView>,
    pub unavailable: Vec<u8>,
    pub unavailable_reasons: Vec<UnavailableView>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UnavailableView {
    pub slot: u8,
    pub reason: String,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Poll {
    Idle,
    Waiting,
    Confirmed {
        slot: u8,
        name: String,
        carry_over: bool,
    },
    ConfirmedMany {
        players: Vec<SlotUpdate>,
        timing: BatchTiming,
    },
}
pub(crate) struct Session<D: Driver> {
    driver: D,
    store: outfit_journal::Store,
    leases: Vec<Lease>,
    pending: Option<Pending<D::Receipt>>,
    batch: Option<Batch<D::Receipt>>,
    held: Option<String>,
    ended: bool,
    matching: outfit_journal::runtime::Coordinator,
    bindings: Vec<(outfits::PlayerKey, String)>,
}
impl<D: Driver> Session<D> {
    pub fn create(root: &Path, session: &str, driver: D) -> Result<Self> {
        let store = outfit_journal::Store::create(root, session, driver.identity())?;
        Ok(Self {
            driver,
            store,
            leases: Vec::new(),
            pending: None,
            batch: None,
            held: None,
            ended: false,
            matching: Default::default(),
            bindings: Vec::new(),
        })
    }
    fn available(&self) -> Result<()> {
        ensure!(!self.ended, "Outfit session has ended");
        ensure!(
            self.held.is_none(),
            "Outfit session needs review: {}",
            self.held.as_deref().unwrap_or_default()
        );
        ensure!(
            self.store.halted().is_none(),
            "Outfit journal is held after an uncertain save"
        );
        ensure!(
            self.matching.held().is_none(),
            "Outfit match worker needs review; no automatic retry"
        );
        Ok(())
    }
    pub fn held(&self) -> Option<&str> {
        self.held
            .as_deref()
            .or(self.store.halted())
            .or(self.matching.held())
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some() || self.batch.is_some() || self.matching.busy()
    }
    pub fn journal(&self) -> &outfit_journal::Store {
        &self.store
    }
    pub fn observed_boundary(&self) -> outfit_journal::runtime::Boundary {
        self.matching.observed()
    }
    pub fn list(&mut self, now: u64) -> Result<SelectionView> {
        self.available()?;
        ensure!(
            !self.busy(),
            "Wait for the selected player to finish refreshing"
        );
        let snapshot = self.driver.capture(self.store.pages())?;
        self.bindings
            .retain(|(key, _)| snapshot.players.iter().any(|p| p.key == *key));
        self.leases.retain(|l| {
            now.checked_sub(l.created).is_some_and(|age| age < TOKEN_MS)
                && snapshot.players.contains(&l.player)
        });
        let mut players = Vec::new();
        for p in snapshot.players {
            let binding = if let Some((_, id)) = self.bindings.iter().find(|(key, _)| *key == p.key)
            {
                id.clone()
            } else {
                let id = backend::unique_id();
                self.bindings.push((p.key.clone(), id.clone()));
                id
            };
            let token = if let Some(l) = self.leases.iter().find(|l| l.player == p) {
                l.token.clone()
            } else {
                let token = backend::unique_id();
                self.leases.push(Lease {
                    token: token.clone(),
                    player: p.clone(),
                    created: now,
                });
                token
            };
            players.push(PlayerView {
                current_outfit: p
                    .pool
                    .as_deref()
                    .and_then(|pool| crate::outfits::catalog().ok()?.worn_outfit(&p.items, pool)),
                current_items: p.items.clone(),
                binding,
                token,
                slot: p.key.slot,
                name: p.name,
                cyberface: p.cyberface,
                pool: p.pool,
            });
        }
        ensure!(
            self.leases.len() <= 10,
            "Native outfit token limit exceeded"
        );
        Ok(SelectionView {
            required: snapshot.required,
            selected: snapshot.selected,
            players,
            unavailable: snapshot.unavailable,
            unavailable_reasons: snapshot
                .reasons
                .into_iter()
                .map(|(slot, reason)| UnavailableView { slot, reason })
                .collect(),
        })
    }
    fn take_player(&mut self, token: &str, now: u64) -> Result<Player> {
        self.available()?;
        ensure!(!self.busy(), "An outfit request is still refreshing");
        ensure!(
            token.len() == 48 && token.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid player token"
        );
        let at = self
            .leases
            .iter()
            .position(|l| l.token == token)
            .context("Refresh the native player list before applying")?;
        let lease = self.leases.remove(at);
        ensure!(
            now.checked_sub(lease.created)
                .is_some_and(|age| age < TOKEN_MS),
            "Player selection expired; refresh the player list"
        );
        // Full fresh capture is required even though the single-use token also
        // binds the old snapshot. A reused slot must never target a new player.
        let fresh = self.driver.capture(self.store.pages())?;
        ensure!(
            fresh.players.contains(&lease.player),
            "Selected player/outfit changed; refresh the player list"
        );
        self.leases.clear();
        Ok(lease.player)
    }
    fn uncertain<T>(&mut self, result: Result<T>) -> Result<T> {
        if let Err(e) = &result {
            self.held = Some(format!(
                "Outfit operation was not confirmed; no automatic retry: {e:#}"
            ));
            self.leases.clear();
        }
        result
    }
    pub fn apply(&mut self, token: &str, outfit: &backend::Outfit, now: u64) -> Result<()> {
        backend::validate_outfit(outfit)?;
        let p = self.take_player(token, now)?;
        let c = outfits::compose(&p, outfit)?;
        let r = self.store.reserve(&p, &c)?;
        let id = r.intent().id.clone();
        let result = (|| {
            if r.intent().kind == Kind::Existing {
                let ready = self.store.existing(r)?;
                self.driver.unchanged(ready, &p, &c, self.store.pages())
            } else {
                let prepared = self.driver.prepare(r, &p, &c, self.store.pages())?;
                let ready = self.store.prepared(prepared)?;
                let submission = self.store.attempting(ready)?;
                self.driver.publish(submission, &p, self.store.pages())
            }
        })();
        let receipt = self.uncertain(result)?;
        self.pending = Some(Pending {
            id,
            receipt,
            started: now,
            slot: p.key.slot,
            name: p.name,
            carry_over: c.items.len() >= 3,
        });
        Ok(())
    }
    /// One validated pass for several lineup players: one fresh capture, every
    /// outfit composed before any write, then each slot reserved, prepared,
    /// attempted and published back to back. The combined refresh is confirmed
    /// slot by slot in poll(); an unconfirmed slot holds the session.
    pub fn apply_many(&mut self, items: &[(String, backend::Outfit)], now: u64) -> Result<()> {
        self.apply_many_origin(items, now, false).map(|_| ())
    }
    pub fn apply_automatic_many(
        &mut self,
        items: &[(String, backend::Outfit)],
        now: u64,
    ) -> Result<usize> {
        self.apply_many_origin(items, now, true)
    }
    // Called only after the existing fresh full capture matched the token's
    // entire Player. No alias, elapsed-time or remembered browser value owns it.
    fn owns_queued_choice(&self, p: &Player) -> Result<bool> {
        let Some(choice) = self.store.ledger().choices.iter().find(|choice| {
            !self.store.ledger().consumed.contains(&choice.choice_id)
                && choice.roster == p.key.roster
                && choice.slot == p.key.slot
        }) else {
            return Ok(false);
        };
        let page_owned = self.store.pages().iter().any(|page| {
            page.owner == p.key
                && page.original_pointer == p.original_pointer
                && page.allocation.checked_add(128) == Some(p.source.pointer)
                && p.source.count as usize == page.items.len()
                && page.items == p.items
        });
        ensure!(
            page_owned && choice.name == p.name && choice.items == p.items,
            "Previously queued outfit ownership changed; automatic outfits were not applied"
        );
        Ok(true)
    }
    fn apply_many_origin(
        &mut self,
        items: &[(String, backend::Outfit)],
        now: u64,
        automatic: bool,
    ) -> Result<usize> {
        self.available()?;
        ensure!(!self.busy(), "An outfit request is still refreshing");
        ensure!((1..=10).contains(&items.len()), "Choose 1 to 10 players");
        let mut players = Vec::new();
        for (token, outfit) in items {
            backend::validate_outfit(outfit)?;
            ensure!(
                token.len() == 48 && token.bytes().all(|b| b.is_ascii_hexdigit()),
                "Invalid player token"
            );
            let lease = self
                .leases
                .iter()
                .find(|l| l.token == *token)
                .context("Refresh the native player list before applying")?;
            ensure!(
                now.checked_sub(lease.created)
                    .is_some_and(|age| age < TOKEN_MS),
                "Player selection expired; refresh the player list"
            );
            ensure!(
                !players
                    .iter()
                    .any(|p: &Player| p.key.slot == lease.player.key.slot),
                "A player appears twice in one batch"
            );
            players.push(lease.player.clone());
        }
        // Same single fresh capture as take_player, once for the whole batch.
        let fresh = self.driver.capture(self.store.pages())?;
        ensure!(
            players.iter().all(|p| fresh.players.contains(p)),
            "Selected player/outfit changed; refresh the player list"
        );
        let mut requested = Vec::new();
        for (player, item) in players.into_iter().zip(items) {
            if !automatic || !self.owns_queued_choice(&player)? {
                requested.push((player, item));
            }
        }
        self.leases.clear();
        if requested.is_empty() {
            return Ok(0);
        }
        // Compose everything before the first journal write or game write.
        let composed = requested
            .iter()
            .map(|(player, (_, outfit))| Ok((player.clone(), outfits::compose(player, outfit)?)))
            .collect::<Result<Vec<_>>>()?;
        let applied = composed.len();
        let mut batch = Batch {
            items: Vec::new(),
            done: Vec::new(),
            started: now,
            written: now,
        };
        for (p, c) in composed {
            let r = self.store.reserve(&p, &c)?;
            let id = r.intent().id.clone();
            let result = (|| {
                if r.intent().kind == Kind::Existing {
                    let ready = self.store.existing(r)?;
                    self.driver.unchanged(ready, &p, &c, self.store.pages())
                } else {
                    let prepared = self.driver.prepare(r, &p, &c, self.store.pages())?;
                    let ready = self.store.prepared(prepared)?;
                    let submission = self.store.attempting(ready)?;
                    self.driver.publish(submission, &p, self.store.pages())
                }
            })();
            let receipt = self.uncertain(result)?;
            batch.items.push(Pending {
                id,
                receipt,
                started: now,
                slot: p.key.slot,
                name: p.name,
                carry_over: c.items.len() >= 3,
            });
        }
        self.batch = Some(batch);
        Ok(applied)
    }
    /// Records when the last slot of the batch was published (timing only).
    pub fn mark_batch_written(&mut self, now: u64) {
        if let Some(b) = &mut self.batch {
            b.written = now;
        }
    }
    pub fn restore(&mut self, token: &str, now: u64) -> Result<()> {
        let p = self.take_player(token, now)?;
        let ready = self.store.reserve_restore(&p)?;
        let id = ready.intent().id.clone();
        let result = (|| {
            let submission = self.store.attempting(ready)?;
            self.driver.publish(submission, &p, self.store.pages())
        })();
        let receipt = self.uncertain(result)?;
        self.pending = Some(Pending {
            id,
            receipt,
            started: now,
            slot: p.key.slot,
            name: p.name,
            carry_over: false,
        });
        Ok(())
    }
    fn poll_batch(&mut self, now: u64) -> Result<Poll> {
        let Some(mut b) = self.batch.take() else {
            return Ok(Poll::Idle);
        };
        let n = b.items.len() + b.done.len();
        let result = (|| -> Result<()> {
            let age = now
                .checked_sub(b.started)
                .context("Outfit refresh clock changed")?;
            // The game refreshes each slot independently; allow the same 3 s
            // per slot that sequential applies had, never an open-ended wait.
            ensure!(
                age <= REFRESH_MS * n as u64,
                "Player refresh was not confirmed within {} seconds",
                3 * n
            );
            let mut waiting = Vec::new();
            for p in std::mem::take(&mut b.items) {
                if self.driver.consumed(&p.receipt, self.store.pages())? {
                    // Each slot's readback was proven by consumed(); record it.
                    self.store.verified(&p.id)?;
                    b.done.push(SlotUpdate {
                        slot: p.slot,
                        name: p.name,
                        carry_over: p.carry_over,
                    });
                } else {
                    waiting.push(p);
                }
            }
            b.items = waiting;
            Ok(())
        })();
        self.uncertain(result)?;
        if !b.items.is_empty() {
            self.batch = Some(b);
            return Ok(Poll::Waiting);
        }
        Ok(Poll::ConfirmedMany {
            timing: BatchTiming {
                write_ms: b.written.saturating_sub(b.started),
                refresh_ms: now.saturating_sub(b.written),
            },
            players: b.done,
        })
    }
    pub fn poll(&mut self, now: u64) -> Result<Poll> {
        self.available()?;
        if self.batch.is_some() {
            return self.poll_batch(now);
        }
        let Some(p) = self.pending.take() else {
            return Ok(Poll::Idle);
        };
        let result = (|| {
            let age = now
                .checked_sub(p.started)
                .context("Outfit refresh clock changed")?;
            ensure!(
                age <= REFRESH_MS,
                "Player refresh was not confirmed within three seconds"
            );
            self.driver.consumed(&p.receipt, self.store.pages())
        })();
        match self.uncertain(result)? {
            false => {
                self.pending = Some(p);
                Ok(Poll::Waiting)
            }
            true => {
                // Keep the ownership capability alive through confirmation.
                let result = self.store.verified(&p.id);
                self.uncertain(result)?;
                Ok(Poll::Confirmed {
                    slot: p.slot,
                    name: p.name,
                    carry_over: p.carry_over,
                })
            }
        }
    }
    /// No out-of-mode restoration writes. Private allocations remain reserved
    /// for this game lifetime, while pending match choices are cleared.
    pub fn leave_blacktop(&mut self) -> Result<()> {
        self.available()?;
        self.leases.clear();
        self.bindings.clear();
        if self.pending.take().is_some() | self.batch.take().is_some() {
            self.held =
                Some("Blacktop ended before the selection refresh was confirmed; no replay".into());
            anyhow::bail!("Blacktop selection refresh is unconfirmed");
        }
        let result = self.store.clear_pending();
        self.uncertain(result)
    }
    pub fn finish(&mut self, closed: impl Fn() -> Result<()>) -> Result<()> {
        closed()?;
        if self.ended {
            return Ok(());
        }
        self.pending.take();
        self.batch.take();
        self.matching.close();
        self.leases.clear();
        self.held = Some("Outfit session is closing; no further game requests are accepted".into());
        self.bindings.clear();
        self.store.finish(&closed)?;
        self.ended = true;
        Ok(())
    }
}
impl<D: Driver + outfit_journal::runtime::Driver> Session<D> {
    pub fn carry_timing(&self) -> &outfit_journal::runtime::Timing {
        self.matching.timing()
    }
    pub fn tick(&mut self, now: u64) -> Result<outfit_journal::runtime::Status> {
        self.available()?;
        match self.poll(now)? {
            Poll::Waiting => return Ok(outfit_journal::runtime::Status::SelectionRefreshing),
            Poll::Confirmed {
                slot,
                name,
                carry_over,
            } => {
                return Ok(outfit_journal::runtime::Status::SelectionUpdated {
                    slot,
                    name,
                    carry_over,
                });
            }
            Poll::ConfirmedMany { players, timing } => {
                return Ok(outfit_journal::runtime::Status::SelectionUpdatedMany {
                    players,
                    timing,
                });
            }
            Poll::Idle => {}
        }
        let status = self.matching.tick(&self.driver, &mut self.store, now)?;
        if self.matching.at_menu() {
            self.leases.clear();
            self.bindings.clear();
        }
        Ok(status)
    }
}
impl Session<NativeWriter> {
    pub fn allow_writes(&mut self, folder: &Path) -> Result<()> {
        self.available()?;
        self.driver.allow_writes(folder)
    }
}
#[cfg(test)]
#[path = "outfit_selection_session_tests.rs"]
mod tests;
