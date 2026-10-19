//! Admission for the same-EXE decoder, not an exception to native process
//! ownership. The production caller supplies trusted in-memory session state
//! plus a FRESH central process scan. Renderer JSON is never authority here.
use anyhow::{Result, ensure};
use serde_json::Value;
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Admission {
    DecoderAllowed,
    NativeOnly,
}
impl Admission {
    pub fn require_decoder(self) -> Result<()> {
        ensure!(
            self == Self::DecoderAllowed,
            "Close NBA 2K27 and finish launch-session cleanup before importing audio that needs conversion. Native PCM/Opus imports, previews and playlist edits remain available."
        );
        Ok(())
    }
}
pub(crate) type Check = Arc<dyn Fn() -> Result<Admission> + Send + Sync + 'static>;
pub(crate) type Guard = MutexGuard<'static, ()>;
static LAUNCH_OR_DECODE: Mutex<()> = Mutex::new(());
/// Root retains this guard from launch admission through Steam dispatch. The
/// decoder retains the SAME gate through exact child termination/reaping.
/// Contention refuses work promptly; it never blocks Exit behind a codec.
pub(crate) fn hold() -> Result<Guard> {
    try_hold(&LAUNCH_OR_DECODE)
}
fn try_hold(gate: &Mutex<()>) -> Result<MutexGuard<'_, ()>> {
    match gate.try_lock() {
        Ok(guard) => Ok(guard),
        Err(TryLockError::WouldBlock) => anyhow::bail!(
            "Wait for the current game launch/audio conversion to finish before starting the other operation"
        ),
        Err(TryLockError::Poisoned(_)) => anyhow::bail!(
            "Audio/launch admission needs review; restart the launcher before another conversion or Play"
        ),
    }
}
/// This checks only the authoritative controller view. The injected callback
/// MUST additionally prove fresh process absence and then call this again.
/// Inactive/unknown/held provider state is not treated as an ended session.
pub(crate) fn session_quiet(native: &Value) -> bool {
    if !matches!(native["phase"].as_str(), Some("idle" | "closed"))
        || native.get("error") != Some(&Value::Null)
    {
        return false;
    }
    match native.get("nativeProvider") {
        Some(Value::Null) => true,
        Some(provider) => {
            provider["phase"] == "closed" && provider.get("error").is_none_or(Value::is_null)
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn only_exact_inactive_authoritative_controller_states_admit_decode() {
        for phase in ["idle", "closed"] {
            assert!(session_quiet(
                &json!({"phase":phase,"error":null,"nativeProvider":null})
            ));
            assert!(session_quiet(
                &json!({"phase":phase,"error":null,"nativeProvider":{"phase":"closed"}})
            ));
        }
        for phase in [
            "waiting-for-game",
            "game-running",
            "needs-review",
            "unknown",
            "",
        ] {
            assert!(!session_quiet(
                &json!({"phase":phase,"error":null,"nativeProvider":null})
            ));
        }
        for value in [
            json!({"phase":"idle"}),
            json!({"phase":"closed","error":"held","nativeProvider":null}),
            json!({"phase":"idle","error":null,"nativeProvider":{"phase":"queued"}}),
            json!({"phase":"closed","error":null,"nativeProvider":{"phase":"closed","error":"restore uncertain"}}),
            Value::Null,
        ] {
            assert!(!session_quiet(&value), "{value}");
        }
        assert!(Admission::NativeOnly.require_decoder().is_err());
        Admission::DecoderAllowed.require_decoder().unwrap();
    }
    #[test]
    fn launch_and_decoder_gate_is_prompt_exclusive_and_release_scoped() {
        let gate = Mutex::new(());
        let lease = try_hold(&gate).unwrap();
        assert!(try_hold(&gate).is_err());
        drop(lease);
        drop(try_hold(&gate).unwrap());
    }
}
