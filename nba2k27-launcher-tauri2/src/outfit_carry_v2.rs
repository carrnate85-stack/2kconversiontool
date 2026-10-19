//! Independent V2 carry-over admission. Does not change legacy native gates or
//! the frozen selection-only service. Native readiness still requires the new
//! worker's single-use owned plan/report transport.
#![allow(dead_code)]
use crate::{
    backend,
    blacktop_native::{Memory, outfit_selection_v2_writer::Writer},
    blacktop_outfit_match::{Choice, MatchPlayer},
    blacktop_outfits::d,
    outfit_journal::runtime::{Boundary, Driver as MatchDriver, Readiness, Sample, Status},
    outfit_selection_session::{Driver, SelectionView, Session},
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Capabilities {
    pub selection_fitting: bool,
    pub match_carry_over: bool,
    pub automatic_randomization: bool,
    pub gameplay_verified: bool,
    pub minimum_carry_pieces: usize,
    pub maximum_carry_pieces: usize,
}
pub(crate) fn capabilities() -> Capabilities {
    Capabilities {
        selection_fitting: true,
        match_carry_over: true,
        automatic_randomization: false,
        gameplay_verified: false,
        minimum_carry_pieces: 3,
        maximum_carry_pieces: 7,
    }
}
pub(crate) fn lineup(m: &impl Memory) -> Result<Vec<u64>> {
    use crate::outfit_selection_v2::LINEUP;
    crate::outfit_selection_v2::verify_profile(m)?;
    let context = crate::rules_v2::context(m)?;
    ensure!(
        matches!(
            crate::rules_v2::boundary(&context, m.base()),
            "selection" | "match"
        ),
        "Open Blacktop selection or a loaded Blacktop match"
    );
    let required = d(m, m.base() + LINEUP + 4)?;
    let count = d(m, m.base() + LINEUP + 0xB8)?;
    ensure!(
        (2..=10).contains(&required)
            && required % 2 == 0
            && required == count
            && d(m, m.base() + LINEUP + 8)? == required,
        "V2 Blacktop lineup is incomplete"
    );
    let rows = crate::outfit_selection_v2::lineup_rows(m, count)?;
    ensure!(
        rows.iter()
            .all(|r| *r >= 65536 && *r < 0x7fff00000000 && r % 8 == 0)
            && rows.iter().enumerate().all(|(n, r)| !rows[..n].contains(r)),
        "V2 lineup contains duplicate or invalid roster identities"
    );
    ensure!(
        crate::rules_v2::context(m)? == context
            && d(m, m.base() + LINEUP + 4)? == required
            && d(m, m.base() + LINEUP + 8)? == required
            && d(m, m.base() + LINEUP + 0xB8)? == count,
        "V2 lineup changed while reading"
    );
    ensure!(
        crate::outfit_selection_v2::lineup_rows(m, count)? == rows,
        "V2 lineup identity changed while reading"
    );
    m.checkpoint()?;
    Ok(rows)
}
pub(crate) fn ready(m: &impl Memory, choices: &[Choice]) -> Result<Vec<MatchPlayer>> {
    ensure!(
        (1..=10).contains(&choices.len()),
        "No V2 match outfits chosen"
    );
    let selected = lineup(m)?;
    let out = ready_selected(m, choices, &selected)?;
    ensure!(
        lineup(m)? == selected,
        "V2 lineup changed before outfit handoff"
    );
    Ok(out)
}
// This selected-lineup value is scoped to the immediately surrounding fresh
// capture. It is never retained as a reusable ownership/admission proof.
fn ready_selected(
    m: &impl Memory,
    choices: &[Choice],
    selected: &[u64],
) -> Result<Vec<MatchPlayer>> {
    ensure!(
        (1..=10).contains(&choices.len()),
        "No V2 match outfits chosen"
    );
    let mut originals = Vec::new();
    for c in choices {
        c.validate()?;
        ensure!(
            selected.contains(&c.roster) && !originals.contains(&c.roster),
            "Chosen outfit does not uniquely belong to this V2 lineup"
        );
        originals.push(c.roster);
    }
    let players = crate::outfit_match_v2::capture(m, &originals)?;
    let mut out = Vec::new();
    for original in originals {
        let p = players
            .iter()
            .find(|p| p.original_roster == original)
            .context("V2 match roster has not loaded")?;
        // Existing stable-readiness format stores exactly the stock top/bottom
        // pair. Never truncate an expanded description to manufacture readiness.
        ensure!(
            p.description.count == 2 && p.description.names.len() == 2,
            "V2 carry-over currently requires an ordinary stock two-piece match source"
        );
        out.push(MatchPlayer {
            roster: p.original_roster,
            record: p.row,
            roster_copy: p.loaded_roster,
            manager: p.primary,
            player: p.actor,
            description: p.description.address,
            array: p.description.array,
            items: [
                p.description.names[0].clone(),
                p.description.names[1].clone(),
            ],
        });
    }
    Ok(out)
}
/// One bounded fresh transaction, repeated at each durable reserve/publication
/// check. Owner/anti-cheat/process and native profiles are re-proved before and
/// after the read; every memory read keeps its normal retained identity guard.
pub(crate) fn handoff(
    m: &impl Memory,
    choices: &[Choice],
    expected_lineup: &[u64],
    expected_players: &[MatchPlayer],
    owner: impl Fn() -> Result<()>,
) -> Result<bool> {
    owner()?;
    let observed = (|| -> Result<bool> {
        let context = crate::rules_v2::context(m)?;
        ensure!(
            crate::rules_v2::boundary(&context, m.base()) == "match",
            "V2 match changed before handoff"
        );
        let selected = lineup(m)?;
        if selected != expected_lineup {
            return Ok(false);
        }
        let players = ready_selected(m, choices, &selected)?;
        // Preserve both fresh lineup proofs around full native capture.
        let ending_lineup = lineup(m)?;
        let ending_context = crate::rules_v2::context(m)?;
        Ok(players == expected_players && ending_lineup == selected && ending_context == context)
    })();
    // Do not turn a late owner/cancellation failure into a retryable capture.
    owner()?;
    Ok(matches!(observed, Ok(true)))
}
// Shared classifier keeps the pre-existing selection/match/menu conditions.
fn observed_boundary(m: &impl Memory) -> (Boundary, Option<crate::blacktop_native::ContextState>) {
    let Ok(context) = crate::rules_v2::context(m) else {
        return (Boundary::Other, None);
    };
    let boundary = match crate::rules_v2::boundary(&context, m.base()) {
        "selection" => Boundary::Selection,
        "match" => Boundary::Match,
        "main-menu" | "quick-play" => Boundary::Menu,
        _ => Boundary::Other,
    };
    (boundary, Some(context))
}
/// Initial sample uses no persisted proof and never mints/reserves a choice.
/// The caller's choose callback reads the ledger only after fresh lineup proof.
pub(crate) fn sample(
    m: &impl Memory,
    allow_ready: bool,
    choose: impl FnOnce(&[u64]) -> Result<Vec<Choice>>,
    owner: impl Fn() -> Result<()>,
) -> Result<Sample> {
    let started = std::time::Instant::now();
    owner()?;
    let mut ready_started = None;
    let observed = (|| -> Result<Sample> {
        let (boundary, context) = observed_boundary(m);
        let mut sample = Sample {
            boundary,
            match_after_ms: (boundary == Boundary::Match)
                .then(|| started.elapsed().as_millis().min(u64::MAX as u128) as u64),
            readiness_ms: 0,
            readiness: None,
        };
        if !allow_ready || boundary != Boundary::Match {
            return Ok(sample);
        }
        let selected = match lineup(m) {
            Ok(selected) => selected,
            Err(_) => {
                sample.readiness = Some(Readiness::LineupWaiting);
                return Ok(sample);
            }
        };
        let choices = choose(&selected)?;
        if choices.is_empty() {
            sample.readiness = Some(Readiness::NoChoices);
            return Ok(sample);
        }
        ready_started = Some(std::time::Instant::now());
        let readiness = (|| -> Result<Vec<MatchPlayer>> {
            let players = ready_selected(m, &choices, &selected)?;
            ensure!(
                lineup(m)? == selected
                    && crate::rules_v2::context(m)?
                        == context.context("Missing V2 initial context")?,
                "V2 lineup or context changed during initial readiness"
            );
            Ok(players)
        })();
        sample.readiness = Some(match readiness {
            Ok(players) => Readiness::Ready {
                lineup: selected,
                choices,
                players,
            },
            Err(error) => Readiness::NotReady(format!("{error:#}")),
        });
        Ok(sample)
    })();
    // Always re-prove full process/parent/anti-cheat/profile ownership even when
    // the bounded read failed or the read-only ledger callback returned error.
    owner()?;
    let mut observed = observed?;
    if let Some(ready_started) = ready_started {
        observed.readiness_ms = ready_started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    }
    Ok(observed)
}
impl MatchDriver for Writer {
    fn check(&self) -> Result<()> {
        self.check_owner()
    }
    fn boundary(&self) -> Result<Boundary> {
        self.check_owner()?;
        Ok(observed_boundary(self).0)
    }
    fn sample(
        &self,
        allow_ready: bool,
        choose: impl FnOnce(&[u64]) -> Result<Vec<Choice>>,
    ) -> Result<Sample> {
        sample(self, allow_ready, choose, || self.check_owner())
    }

    fn lineup(&self) -> Result<Vec<u64>> {
        self.check_owner()?;
        lineup(self)
    }
    fn ready(&self, choices: &[Choice]) -> Result<Vec<MatchPlayer>> {
        self.check_owner()?;
        ready(self, choices)
    }
    fn handoff(&self, choices: &[Choice], lineup: &[u64], players: &[MatchPlayer]) -> Result<bool> {
        handoff(self, choices, lineup, players, || self.check_owner())
    }
}

pub(crate) struct Service<D: Driver + MatchDriver> {
    session: Session<D>,
}
impl<D: Driver + MatchDriver> Service<D> {
    pub fn create(root: &Path, session: &str, driver: D) -> Result<Self> {
        Ok(Self {
            session: Session::create(root, session, driver)?,
        })
    }
    pub fn list(&mut self, now: u64) -> Result<SelectionView> {
        self.session.list(now)
    }
    fn token(&mut self, binding: &str, now: u64) -> Result<String> {
        ensure!(
            binding.len() == 48 && binding.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid native player binding"
        );
        self.session
            .list(now)?
            .players
            .into_iter()
            .find(|p| p.binding == binding)
            .map(|p| p.token)
            .context("Selected player changed; reopen Edit outfit")
    }
    pub fn apply(&mut self, binding: &str, outfit: &backend::Outfit, now: u64) -> Result<()> {
        let token = self.token(binding, now)?;
        self.session.apply(&token, outfit, now)
    }
    pub fn restore(&mut self, binding: &str, now: u64) -> Result<()> {
        let token = self.token(binding, now)?;
        self.session.restore(&token, now)
    }
    /// Random all: one list mints every token, then one combined batch.
    pub fn apply_many(&mut self, items: &[(String, backend::Outfit)], now: u64) -> Result<()> {
        self.apply_many_with_origin(items, now, false).map(|_| ())
    }
    pub fn apply_many_with_origin(
        &mut self,
        items: &[(String, backend::Outfit)],
        now: u64,
        automatic: bool,
    ) -> Result<usize> {
        for (binding, _) in items {
            ensure!(
                binding.len() == 48 && binding.bytes().all(|b| b.is_ascii_hexdigit()),
                "Invalid native player binding"
            );
        }
        let view = self.session.list(now)?;
        let tokens = items
            .iter()
            .map(|(binding, outfit)| {
                view.players
                    .iter()
                    .find(|p| p.binding == *binding)
                    .map(|p| (p.token.clone(), outfit.clone()))
                    .context("Selected player changed; reopen Edit outfit")
            })
            .collect::<Result<Vec<_>>>()?;
        if automatic {
            self.session.apply_automatic_many(&tokens, now)
        } else {
            self.session.apply_many(&tokens, now)?;
            Ok(tokens.len())
        }
    }
    pub fn mark_batch_written(&mut self, now: u64) {
        self.session.mark_batch_written(now)
    }
    /// This is the ONLY selection-exit handoff. Do not call leave_selection /
    /// leave_blacktop here: those deliberately clear the verified choices.
    pub fn tick(&mut self, now: u64) -> Result<Status> {
        self.session.tick(now)
    }
    /// DEV diagnostic snapshot only. Does not reserve, consume or edit the ledger.
    #[cfg(debug_assertions)]
    pub(crate) fn shadow_choices(&self) -> Vec<(u64, String)> {
        let ledger = self.session.journal().ledger();
        ledger
            .choices
            .iter()
            .filter(|c| !ledger.consumed.contains(&c.choice_id))
            .map(|c| (c.roster, c.choice_id.clone()))
            .collect()
    }
    pub fn timing(&self) -> &crate::outfit_journal::runtime::Timing {
        self.session.carry_timing()
    }
    pub fn busy(&self) -> bool {
        self.session.busy()
    }
    pub fn held(&self) -> Option<&str> {
        self.session.held()
    }
    pub fn observed_boundary(&self) -> Boundary {
        self.session.observed_boundary()
    }
    pub fn finish_ended(&mut self, closed: impl Fn() -> Result<()>) -> Result<()> {
        self.session.finish(closed)
    }
}

#[cfg(test)]
mod live_probe {
    use super::*;
    /// Selection-side read-only probe: the exact gates the outfit List uses.
    #[test]
    #[ignore = "opt-in live READ-ONLY probe (QUERY_LIMITED_INFORMATION|VM_READ); never writes"]
    fn v2_selection_outfit_gates_live_read_only() {
        let game = std::path::PathBuf::from(
            std::env::var_os("C27_LIVE_GAME_DIR").expect("C27_LIVE_GAME_DIR = NBA 2K27 folder"),
        );
        let processes = crate::session_safety::processes().unwrap();
        let p = processes
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe"))
            .expect("NBA2K27.exe is not running");
        let id = p.identity.clone().expect("game identity");
        let m = crate::blacktop_native::Reader::open_observer(&id, &game).unwrap();
        match crate::rules_v2::context(&m) {
            Ok(c) => eprintln!(
                "context={c:x?} boundary={} owner+0xA04={:?}",
                crate::rules_v2::boundary(&c, m.base()),
                crate::blacktop_outfits::d(&m, c.owner + 0xA04)
            ),
            Err(e) => eprintln!("context error: {e:#}"),
        }
        for n in 0..3 {
            let t = std::time::Instant::now();
            let ok = crate::outfit_selection_v2::capture(&m, &id, &[]).is_ok();
            eprintln!("capture timing #{n}: {:?} ok={ok}", t.elapsed());
        }
        match crate::outfit_selection_v2::capture(&m, &id, &[]) {
            Ok(s) => eprintln!(
                "selection capture OK: required={} selected={} players={:?} unavailable={:?} reasons={:?}",
                s.required,
                s.selected,
                s.players
                    .iter()
                    .map(|p| (p.key.slot, p.name.clone(), p.pool.clone()))
                    .collect::<Vec<_>>(),
                s.unavailable,
                s.reasons
            ),
            Err(e) => eprintln!("selection capture error: {e:#}"),
        }
    }
    /// Prints, per lineup player, whether the running match is carry-ready and
    /// the exact reason when it is not. Read-only observer handle only.
    #[test]
    #[ignore = "opt-in live READ-ONLY probe (QUERY_LIMITED_INFORMATION|VM_READ); never writes"]
    fn v2_match_carry_readiness_live_read_only() {
        let game = std::path::PathBuf::from(
            std::env::var_os("C27_LIVE_GAME_DIR").expect("C27_LIVE_GAME_DIR = NBA 2K27 folder"),
        );
        let processes = crate::session_safety::processes().unwrap();
        let p = processes
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe"))
            .expect("NBA2K27.exe is not running");
        let id = p.identity.clone().expect("game identity");
        let m = crate::blacktop_native::Reader::open_observer(&id, &game).unwrap();
        match crate::rules_v2::context(&m) {
            Ok(c) => eprintln!(
                "boundary={} game_mode={}",
                crate::rules_v2::boundary(&c, m.base()),
                c.game_mode
            ),
            Err(e) => eprintln!("context error: {e:#}"),
        }
        let rows = match lineup(&m) {
            Ok(rows) => rows,
            Err(e) => return eprintln!("lineup error: {e:#}"),
        };
        eprintln!("lineup={} players {:x?}", rows.len(), rows);
        // Raw table rows the match capture walks (read-only dump).
        {
            use crate::outfit_match_v2::{MANAGER, TABLE, WRAPPER};
            let b = m.base();
            let r = |at: u64| crate::blacktop_outfits::q(&m, at).unwrap_or(u64::MAX);
            for n in 0..30u64 {
                let row = b + TABLE + n * 0x50;
                if r(row) != b + WRAPPER {
                    continue;
                }
                let primary = r(row + 0x10);
                let loaded = r(row + 0x28);
                if primary == 0 || loaded == 0 {
                    continue;
                }
                let original = r(loaded + 0x500);
                eprintln!(
                    "row {n}: primary={primary:x} vt={:x} (want {MANAGER:x}) loaded={loaded:x} original={original:x} in_lineup={} loaded+0x518={:x} loaded+0x510={:x} loaded+0x520={:x} row+0x18={:x}",
                    r(primary).wrapping_sub(b),
                    rows.contains(&original),
                    r(loaded + 0x518),
                    r(loaded + 0x510),
                    r(loaded + 0x520),
                    r(row + 0x18),
                );
                // Active-manager list membership for both candidates.
                {
                    let head = b + crate::outfit_match_v2::ACTIVE;
                    let (mut node, mut full_in, mut sec_in, mut n) = (r(head + 0x10), 0, 0, 0);
                    while node != head && n < 256 && node != u64::MAX {
                        full_in += (node == primary.wrapping_sub(0x78)) as u32;
                        sec_in += (node == primary) as u32;
                        node = r(node + 0x10);
                        n += 1;
                    }
                    eprintln!(
                        "  active list: head_vt={:x} nodes={n} full_in={full_in} secondary_in={sec_in}",
                        r(head).wrapping_sub(b)
                    );
                }
                // Hypothesis check: row+0x10 may address the secondary base.
                for adj in [0x78u64, 0] {
                    let full = primary.wrapping_sub(adj);
                    eprintln!(
                        "  full=primary-{adj:#x}: vt0={:x} vt78={:x} actor(+0x110)={:x} actor_vt={:x} +0x118={:x} +0x90={:x} sentinel(+0xF8)vt={:x}",
                        r(full).wrapping_sub(b),
                        r(full + 0x78).wrapping_sub(b),
                        r(full + 0x110),
                        r(r(full + 0x110)).wrapping_sub(b),
                        r(full + 0x118),
                        r(full + 0x90) & 0xffff_ffff,
                        r(full + 0xF8).wrapping_sub(b),
                    );
                }
            }
        }
        match crate::outfit_match_v2::capture(&m, &rows) {
            Err(e) => eprintln!("match capture error: {e:#}"),
            Ok(players) => {
                for (n, r) in rows.iter().enumerate() {
                    match players.iter().find(|p| p.original_roster == *r) {
                        None => eprintln!("slot {n}: match roster has not loaded"),
                        Some(p) => eprintln!(
                            "slot {n}: description count={} names={:?} -> {}",
                            p.description.count,
                            p.description.names,
                            if p.description.count == 2 && p.description.names.len() == 2 {
                                "carry-ready source"
                            } else {
                                "REFUSED: not an ordinary stock two-piece match source"
                            }
                        ),
                    }
                }
            }
        }
    }
}
