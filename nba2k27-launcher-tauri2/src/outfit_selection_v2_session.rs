//! Selection-only facade: never exposes or starts the legacy match coordinator.
//! A confirmed model refresh means selection fitting, not match carry-over.
#![allow(dead_code)]
use crate::{
    backend,
    outfit_selection_session::{Driver, Poll, SelectionView, Session},
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::path::Path;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Capabilities {
    pub selection_fitting: bool,
    pub match_carry_over: bool,
    pub automatic_randomization: bool,
    pub gameplay_verified: bool,
}
pub(crate) fn capabilities() -> Capabilities {
    Capabilities {
        selection_fitting: true,
        match_carry_over: false,
        automatic_randomization: false,
        gameplay_verified: false,
    }
}
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(
    tag = "phase",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub(crate) enum Update {
    Idle,
    Waiting,
    SelectionUpdated {
        slot: u8,
        name: String,
        match_carry_over: bool,
    },
}
pub(crate) struct Service<D: Driver> {
    session: Session<D>,
}
impl<D: Driver> Service<D> {
    /// Root/session must come from the retained provider launch, never a UI path.
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
        // A fresh capture produces an internal single-use token. Browser input
        // cannot supply a native address, slot identity, page or journal proof.
        let view = self.session.list(now)?;
        view.players
            .iter()
            .find(|p| p.binding == binding)
            .map(|p| p.token.clone())
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
    pub fn poll(&mut self, now: u64) -> Result<Update> {
        Ok(match self.session.poll(now)? {
            Poll::Idle => Update::Idle,
            Poll::Waiting => Update::Waiting,
            Poll::Confirmed { slot, name, .. } => Update::SelectionUpdated {
                slot,
                name,
                match_carry_over: false,
            },
            // This facade never issues batches; report the last slot if one appears.
            Poll::ConfirmedMany { players, .. } => {
                let last = players.last().context("Empty outfit batch")?;
                Update::SelectionUpdated {
                    slot: last.slot,
                    name: last.name.clone(),
                    match_carry_over: false,
                }
            }
        })
    }
    pub fn busy(&self) -> bool {
        self.session.busy()
    }
    pub fn held(&self) -> Option<&str> {
        self.session.held()
    }
    /// Call once after a trusted selection exit. No out-of-mode writes, page
    /// freeing, saved-address replay, native trigger or rebuild is performed.
    pub fn leave_selection(&mut self) -> Result<()> {
        self.session.leave_blacktop()
    }
    /// Requires the provider's retained-handle ended + known-game-absence check.
    pub fn finish_ended(&mut self, closed: impl Fn() -> Result<()>) -> Result<()> {
        self.session.finish(closed)
    }
}
