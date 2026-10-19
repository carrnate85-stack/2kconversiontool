//! Saved-outfit native worker transport and stable match readiness. These pure
//! plans cannot themselves publish a trigger or write game memory. The owning
//! session transaction must durably consume choice IDs before submission.
#![allow(dead_code)]
use crate::{
    blacktop_native::{self, Memory},
    blacktop_outfits::{self as outfits, Player},
    session_safety::Identity,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub(crate) const TRIGGER: &[u8] = b"SAVED_OUTFITS_V1";
const STRIDE: usize = 16 + 7 * 512;
const LINEUP: u64 = 0x9BBF0A0;
const MATCH_TABLE: u64 = 0xAACB9B0;
pub(crate) const MAX_CONSUMED: usize = 10000;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Choice {
    pub choice_id: String,
    pub roster: u64,
    pub slot: u8,
    pub name: String,
    pub items: Vec<String>,
}
impl Choice {
    pub fn new(id: String, player: &Player, items: Vec<String>) -> Result<Self> {
        let s = Self {
            choice_id: id,
            roster: player.key.roster,
            slot: player.key.slot,
            name: player.name.clone(),
            items,
        };
        s.validate()?;
        Ok(s)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.choice_id.len() == 48
                && self.choice_id.bytes().all(|b| b.is_ascii_hexdigit())
                && self.slot < 10
                && self.roster >= 65536
                && self.roster < 0x8000_0000_0000
                && !self.name.is_empty()
                && self.name.chars().count() <= 80
                && !self.name.chars().any(char::is_control)
                && (3..=7).contains(&self.items.len())
                && self.items.iter().all(|n| outfits::valid_item(n)),
            "Invalid session-bound match outfit"
        );
        Ok(())
    }
}
pub(crate) fn encode(game: &Identity, choices: &[Choice]) -> Result<Vec<u8>> {
    ensure!(
        game.pid > 0 && (1..=10).contains(&choices.len()),
        "Choose one to ten match outfits"
    );
    let birth = game.started.parse::<u64>()?;
    ensure!(birth > 0, "Invalid match game birth time");
    let mut out = vec![0; 24 + choices.len() * STRIDE];
    out[..4].copy_from_slice(&0x33504F42u32.to_le_bytes());
    out[4..8].copy_from_slice(&game.pid.to_le_bytes());
    out[8..12].copy_from_slice(&(choices.len() as u32).to_le_bytes());
    out[16..24].copy_from_slice(&birth.to_le_bytes());
    let mut rosters = BTreeSet::new();
    let mut slots = BTreeSet::new();
    let mut ids = BTreeSet::new();
    for (i, c) in choices.iter().enumerate() {
        c.validate()?;
        ensure!(
            rosters.insert(c.roster) && slots.insert(c.slot) && ids.insert(&c.choice_id),
            "Duplicate match player, slot or choice ID"
        );
        let at = 24 + i * STRIDE;
        out[at..at + 8].copy_from_slice(&c.roster.to_le_bytes());
        out[at + 8..at + 12].copy_from_slice(&(c.items.len() as u32).to_le_bytes());
        for (piece, name) in c.items.iter().enumerate() {
            for (j, w) in name.encode_utf16().enumerate() {
                let pos = at + 16 + piece * 512 + j * 2;
                out[pos..pos + 2].copy_from_slice(&w.to_le_bytes());
            }
        }
    }
    Ok(out)
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Ledger {
    pub choices: Vec<Choice>,
    pub consumed: BTreeSet<String>,
}
impl Ledger {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.choices.len() <= 10 && self.consumed.len() <= MAX_CONSUMED,
            "Outfit ledger exceeds session bounds"
        );
        let mut roster = BTreeSet::new();
        let mut slots = BTreeSet::new();
        let mut ids = BTreeSet::new();
        for c in &self.choices {
            c.validate()?;
            ensure!(
                roster.insert(c.roster) && slots.insert(c.slot) && ids.insert(&c.choice_id),
                "Duplicate outfit ledger identity"
            );
        }
        ensure!(
            self.consumed
                .iter()
                .all(|id| id.len() == 48 && id.bytes().all(|b| b.is_ascii_hexdigit())),
            "Invalid consumed outfit ID"
        );
        Ok(())
    }
    pub fn record(&mut self, choice: Choice) -> Result<()> {
        self.validate()?;
        choice.validate()?;
        ensure!(
            !self.consumed.contains(&choice.choice_id)
                && !self.choices.iter().any(|c| c.choice_id == choice.choice_id),
            "Outfit choice ID cannot be reused"
        );
        let mut next = self.clone();
        next.choices
            .retain(|c| c.roster != choice.roster && c.slot != choice.slot);
        next.choices.push(choice);
        next.validate()?;
        *self = next;
        Ok(())
    }
    pub fn restore(&mut self, roster: u64, slot: u8) {
        self.choices
            .retain(|c| c.roster != roster && c.slot != slot);
    }
    pub fn leave_blacktop(&mut self) {
        self.choices.clear();
    } // Keep consumed IDs.
    pub fn fresh(&self, lineup: &[u64]) -> Result<Vec<Choice>> {
        self.validate()?;
        ensure!(
            lineup.len() <= 10
                && lineup
                    .iter()
                    .enumerate()
                    .all(|(i, r)| !lineup[..i].contains(r)),
            "Ambiguous active lineup"
        );
        Ok(self
            .choices
            .iter()
            .filter(|c| !self.consumed.contains(&c.choice_id) && lineup.contains(&c.roster))
            .cloned()
            .collect())
    }
    pub fn consume(
        &mut self,
        frozen: &[Choice],
        persist: impl FnOnce(&Self) -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        ensure!(
            !frozen.is_empty() && frozen.len() <= 10,
            "No frozen match outfits"
        );
        let mut next = self.clone();
        for c in frozen {
            ensure!(
                self.choices.contains(c)
                    && !self.consumed.contains(&c.choice_id)
                    && next.consumed.insert(c.choice_id.clone()),
                "Outfit choice changed or already consumed"
            );
        }
        next.validate()?;
        // The caller must commit this before creating the native trigger. If
        // submission later becomes uncertain, these IDs still cannot replay.
        persist(&next)?;
        *self = next;
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MatchPlayer {
    pub roster: u64,
    pub record: u64,
    pub roster_copy: u64,
    pub manager: u64,
    pub player: u64,
    pub description: u64,
    pub array: u64,
    pub items: [String; 2],
}
fn name(m: &impl Memory, at: u64) -> Result<String> {
    outfits::span(at, 512)?;
    let b = m.read(at, 512)?;
    ensure!(b.len() == 512, "Partial match clothing name");
    let words = b
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect::<Vec<_>>();
    let end = words
        .iter()
        .position(|c| *c == 0)
        .context("Unterminated match clothing name")?;
    ensure!(end > 0, "Empty match clothing name");
    Ok(String::from_utf16(&words[..end])?)
}
fn one(m: &impl Memory, roster: u64) -> Result<MatchPlayer> {
    use outfits::{d, q};
    let mut found = None;
    for slot in 0..30 {
        m.checkpoint()?;
        let record = m.base() + MATCH_TABLE + slot * 0x50;
        if q(m, record)? != m.base() + 0x75C0D58 {
            continue;
        }
        let copy = q(m, record + 0x30)?;
        if copy < 65536 {
            continue;
        }
        outfits::span(copy, 0x520)?;
        if q(m, copy + 0x500)? != roster {
            continue;
        }
        ensure!(
            found.is_none(),
            "Saved player is ambiguous in the match table"
        );
        let manager = q(m, record + 0x10)?;
        outfits::span(manager, 0xC0)?;
        ensure!(
            q(m, manager)? == m.base() + 0x6820088
                && q(m, copy + 0x518)? == manager
                && q(m, manager + 0xA0)? == 0
                && m.read(manager + 0xB8, 1)? == [0],
            "Match player manager is busy or changed"
        );
        let player = q(m, manager + 0x98)?;
        let description = player
            .checked_add(0xF20)
            .context("Match player address overflow")?;
        outfits::span(player, 0x12A0)?;
        ensure!(
            q(m, player)? == m.base() + 0x682C528
                && q(m, description)? == m.base() + 0x682DB58
                && q(m, description + 0xB8)? == description + 0x378
                && d(m, description + 0xD0)? == 2
                && m.read(description + 0x7D, 1)?[0] & 1 == 0,
            "Match outfit is not a stable ordinary description"
        );
        let array = q(m, description + 0xC8)?;
        outfits::span(array, 16)?;
        let items = [name(m, q(m, array)?)?, name(m, q(m, array + 8)?)?];
        found = Some(MatchPlayer {
            roster,
            record,
            roster_copy: copy,
            manager,
            player,
            description,
            array,
            items,
        });
    }
    found.context("Saved player has not loaded into the match")
}
pub(crate) fn lineup(m: &impl Memory) -> Result<Vec<u64>> {
    let count = outfits::d(m, m.base() + LINEUP + 0xB8)?;
    ensure!((1..=10).contains(&count), "Match lineup is not ready");
    let rows = (0..count)
        .map(|i| outfits::q(m, m.base() + LINEUP + 0xC0 + i as u64 * 8))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        rows.iter().all(|r| *r >= 65536 && *r < 0x8000_0000_0000)
            && rows.iter().enumerate().all(|(i, r)| !rows[..i].contains(r)),
        "Invalid or duplicate match lineup"
    );
    Ok(rows)
}
pub(crate) fn capture(m: &impl Memory, choices: &[Choice]) -> Result<Vec<MatchPlayer>> {
    blacktop_native::validate_build(m)?;
    let context = blacktop_native::context(m)?;
    ensure!(
        blacktop_native::boundary(&context, m.base()) == "match"
            && outfits::d(m, context.owner + 0xA04)? == 6,
        "Wait for the actual Blacktop match"
    );
    ensure!((1..=10).contains(&choices.len()), "No match choices");
    let before = lineup(m)?;
    for (i, c) in choices.iter().enumerate() {
        c.validate()?;
        ensure!(
            before.iter().filter(|r| **r == c.roster).count() == 1
                && !choices[..i].iter().any(|p| p.roster == c.roster),
            "Saved outfit player is not unique in this lineup"
        );
    }
    let players = choices
        .iter()
        .map(|c| one(m, c.roster))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        blacktop_native::context(m)? == context && lineup(m)? == before,
        "Blacktop match changed during readiness capture"
    );
    // Recheck every resolved manager/description, not only the last player.
    for p in &players {
        ensure!(
            one(m, p.roster)? == *p,
            "Match player changed during readiness capture"
        );
    }
    Ok(players)
}
#[derive(Default)]
pub(crate) struct Stable {
    previous: Option<(Vec<MatchPlayer>, Vec<Choice>)>,
    samples: u8,
    first: u64,
    last: u64,
}
impl Stable {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn observe(&mut self, now: u64, players: Vec<MatchPlayer>, choices: Vec<Choice>) -> bool {
        let current = (players, choices);
        if self.previous.as_ref() != Some(&current) || now < self.last {
            self.previous = Some(current);
            self.samples = 1;
            self.first = now;
            self.last = now;
            return false;
        }
        if now - self.last < 250 {
            return false;
        }
        self.samples = self.samples.saturating_add(1);
        self.last = now;
        self.samples >= 3 && now - self.first >= 500
    }
}
#[cfg(test)]
#[path = "blacktop_outfit_match_tests.rs"]
mod tests;
