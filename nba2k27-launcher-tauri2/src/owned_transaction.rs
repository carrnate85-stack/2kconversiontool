//! Isolated multi-field transactions. Only an authenticated child dispatcher may
//! execute while the same kernel freeze object is retained by two processes.
//! No PID-based resume, NtSuspendProcess fallback, or journal replay exists here.
use crate::session_safety::Identity;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

#[path = "owned_transaction_win.rs"]
mod platform;
pub(crate) use platform::{Context, ReadContext};

pub(crate) const MAX_REQUEST: usize = 196_608;
pub(crate) const MAX_OPERATION: usize = 65_536;
pub(crate) const MAX_RESPONSE: usize = 196_608;
pub(crate) const FORWARD_MS: u64 = 5_000;
pub(crate) const REVERSE_MS: u64 = 8_000;
pub(crate) const WATCHDOG_MS: u64 = 10_000;
pub(crate) const BOOTSTRAP_MS: u64 = 30_000;
pub(crate) const GAME_SHA: &str =
    "29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "spec", deny_unknown_fields)]
pub(crate) enum Operation {
    Season(Value),
    ArenaClone(Value),
    ArenaProfile(Value),
}
impl Operation {
    fn validate(&self) -> Result<()> {
        ensure!(
            serde_json::to_vec(self)?.len() <= MAX_OPERATION,
            "Transaction request is too large"
        );
        let spec = match self {
            Self::Season(v) | Self::ArenaClone(v) | Self::ArenaProfile(v) => v,
        };
        ensure!(spec.is_object(), "Transaction selector must be an object");
        // This is a transport bound. Each native dispatcher MUST additionally
        // deserialize its exact deny_unknown_fields selector and reject addresses.
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    root: String,
    game: Identity,
    parent: Identity,
    session: String,
    operation: Operation,
    emergency_close_consent: bool,
    game_handle: usize,
    parent_handle: usize,
    prior_verified: Vec<String>,
}
impl Request {
    fn validate(&self) -> Result<()> {
        self.operation.validate()?;
        ensure!(
            self.prior_verified.len() <= 512
                && self
                    .prior_verified
                    .iter()
                    .all(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())),
            "Invalid delegated completion witnesses"
        );
        ensure!(
            self.emergency_close_consent,
            "Save requires emergency-close consent before a structural transaction"
        );
        ensure!(
            Path::new(&self.root).is_absolute(),
            "Transaction root must be absolute"
        );
        ensure!(
            (16..=128).contains(&self.session.len())
                && self.session.bytes().all(|c| c.is_ascii_hexdigit()),
            "Invalid transaction session"
        );
        ensure!(
            self.game.pid != 0 && self.parent.pid != 0 && self.game != self.parent,
            "Invalid transaction identities"
        );
        ensure!(
            self.game_handle > 0
                && self.parent_handle > 0
                && self.game_handle != self.parent_handle,
            "Missing retained transaction handles"
        );
        Ok(())
    }
}

/// All effects must occur inside execute_inside. verify_inside runs in the
/// independent watchdog AFTER the exact helper has exited, while still frozen.
/// It must validate an entire native consistent BEFORE or AFTER state from its
/// own fresh reads; an intent, prefix, status string or result hash is not enough.
pub(crate) type ExecuteInside = fn(&mut Context, &Operation) -> Result<Value>;
pub(crate) type VerifyInside = fn(&mut ReadContext, &Operation, &Value) -> Result<()>;

pub(crate) fn verified_intent(game: &Identity, session: &str, sha256: &str) -> bool {
    platform::verified_intent(game, session, sha256)
}

pub(crate) fn available() -> Result<()> {
    platform::available()
}

pub(crate) fn execute(
    root: &Path,
    game: &Identity,
    parent: &Identity,
    session: &str,
    operation: Operation,
    emergency_close_consent: bool,
) -> Result<Value> {
    let gate = crate::session_safety::write_gate()?;
    execute_guarded(
        &gate,
        root,
        game,
        parent,
        session,
        operation,
        emergency_close_consent,
    )
}
/// The actual existing global write guard remains borrowed for the entire
/// child/watchdog transaction. No reentry exemption or temporary unlock exists.
pub(crate) fn execute_guarded(
    _gate: &crate::session_safety::WriteGuard,
    root: &Path,
    game: &Identity,
    parent: &Identity,
    session: &str,
    operation: Operation,
    emergency_close_consent: bool,
) -> Result<Value> {
    platform::execute(
        root,
        game,
        parent,
        session,
        operation,
        emergency_close_consent,
    )
}

/// Call before starting Tauri or creating any windows/services. None means the
/// process is an ordinary launcher. Numeric arguments are never native targets:
/// only private pipe messages may supply handles duplicated into this child.
pub(crate) fn maybe_run_helper(
    execute_inside: ExecuteInside,
    verify_inside: VerifyInside,
) -> Option<i32> {
    platform::maybe_run_helper(execute_inside, verify_inside)
}

/// The effect window starts at the authenticated freeze acknowledgement, never
/// parent bootstrap. A separate finite bootstrap window applies until then.
#[derive(Debug)]
pub(crate) struct WatchdogDeadline {
    bootstrap: u64,
    effect: Option<u64>,
    last: u64,
    held: bool,
}
impl WatchdogDeadline {
    pub(crate) fn new(bootstrap: u64) -> Self {
        Self {
            bootstrap,
            effect: None,
            last: bootstrap,
            held: false,
        }
    }
    pub(crate) fn check(&mut self, now: u64) -> Result<()> {
        if self.held || now < self.last || now < self.bootstrap {
            self.held = true;
            anyhow::bail!("Watchdog clock is held");
        }
        self.last = now;
        let (start, budget) = self
            .effect
            .map(|s| (s, WATCHDOG_MS))
            .unwrap_or((self.bootstrap, BOOTSTRAP_MS));
        if now - start >= budget {
            self.held = true;
            anyhow::bail!("Watchdog phase deadline expired");
        }
        Ok(())
    }
    pub(crate) fn freeze_ack(&mut self, now: u64) -> Result<()> {
        self.check(now)?;
        if self.effect.is_some() {
            self.held = true;
            anyhow::bail!("Freeze acknowledgement cannotrepeat");
        }
        self.effect = Some(now);
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    BeforeFreeze,
    FrozenClean,
    EffectsPossible,
    Consistent,
    Released,
    Held,
}
#[derive(Debug)]
pub(crate) struct Lifecycle {
    phase: Phase,
    start: Option<u64>,
    last: u64,
    attempted: bool,
}
impl Lifecycle {
    pub(crate) fn new() -> Self {
        Self {
            phase: Phase::BeforeFreeze,
            start: None,
            last: 0,
            attempted: false,
        }
    }
    pub(crate) fn freeze(&mut self, now: u64) -> Result<()> {
        ensure!(
            self.phase == Phase::BeforeFreeze && self.start.is_none(),
            "Freeze cannot be repeated"
        );
        self.start = Some(now);
        self.last = now;
        self.phase = Phase::FrozenClean;
        Ok(())
    }
    pub(crate) fn check(&mut self, now: u64, reverse: bool, companion_live: bool) -> Result<()> {
        let start = self
            .start
            .ok_or_else(|| anyhow::anyhow!("Transaction is not frozen"))?;
        if now < self.last || now < start {
            self.phase = Phase::Held;
            anyhow::bail!("Transaction clock moved backwards");
        }
        self.last = now;
        ensure!(
            matches!(
                self.phase,
                Phase::FrozenClean | Phase::EffectsPossible | Phase::Consistent
            ),
            "Transaction is held or released"
        );
        ensure!(
            now - start < if reverse { REVERSE_MS } else { FORWARD_MS },
            "Frozen transaction deadline expired"
        );
        ensure!(
            reverse || companion_live,
            "Independent freeze holder ended; forward writes refused"
        );
        Ok(())
    }
    pub(crate) fn before_effect(&mut self) -> Result<()> {
        ensure!(
            matches!(self.phase, Phase::FrozenClean | Phase::EffectsPossible),
            "No effect permission"
        );
        // Set BEFORE invoking WriteProcessMemory, including an error/partial write.
        self.attempted = true;
        self.phase = Phase::EffectsPossible;
        Ok(())
    }
    pub(crate) fn consistent(&mut self) -> Result<()> {
        ensure!(
            matches!(self.phase, Phase::FrozenClean | Phase::EffectsPossible),
            "No completion permission"
        );
        self.phase = Phase::Consistent;
        Ok(())
    }
    pub(crate) fn may_release(&self) -> bool {
        matches!(
            self.phase,
            Phase::BeforeFreeze | Phase::FrozenClean | Phase::Consistent
        )
    }
}

#[cfg(test)]
#[path = "owned_transaction_tests.rs"]
mod tests;

/// Filesystem-only archival: game ended OR current private verified completion.
pub(crate) fn archive_intent(file: &std::fs::File, target: &Path) -> Result<()> {
    platform::archive_intent(file, target)
}

pub(crate) fn forget_verified(game: &Identity, session: &str, sha: &str) -> Result<()> {
    platform::forget_verified(game, session, sha)
}
