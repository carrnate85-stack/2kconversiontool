//! Normal-window close policy only. Ownership is supplied by the native
//! session service; this module never infers it from a PID/name or a journal.
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub(crate) const EXIT_GRACE: Duration = Duration::from_secs(3);

/// Never hide or minimize the main window while an owned helper can outlive it.
/// The user closes the game/session first, then a later close exits every
/// launcher process together.
pub(crate) fn should_block_close(
    smoke: bool,
    main_window: bool,
    owned_session_active: bool,
) -> bool {
    !smoke && main_window && owned_session_active
}
pub(crate) struct Prompt<'a>(&'a AtomicBool);
impl<'a> Prompt<'a> {
    pub fn acquire(active: &'a AtomicBool) -> Option<Self> {
        active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self(active))
    }
}
impl Drop for Prompt<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

pub(crate) fn claim_exit_watchdog(armed: &AtomicBool) -> bool {
    armed
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
}

/// Normal close gets a bounded graceful-shutdown window. If a native dialog,
/// WebView service or worker stalls after the main window disappears, the
/// launcher must not survive invisibly and keep its single-instance mutex.
pub(crate) fn arm_exit_watchdog(armed: &'static AtomicBool) {
    if !claim_exit_watchdog(armed) {
        return;
    }
    std::thread::spawn(|| {
        std::thread::sleep(EXIT_GRACE);
        std::process::exit(0);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_original_owned_non_smoke_main_sessions_block_full_exit() {
        assert!(should_block_close(false, true, true));
        for input in [
            (true, true, true),
            (false, false, true),
            (false, true, false),
            (true, false, false),
        ] {
            assert!(!should_block_close(input.0, input.1, input.2));
        }
    }
    #[test]
    fn nested_window_close_cannot_open_multiple_prompts_and_guard_releases() {
        let active = AtomicBool::new(false);
        let first = Prompt::acquire(&active).unwrap();
        assert!(active.load(Ordering::Acquire));
        assert!(Prompt::acquire(&active).is_none());
        drop(first);
        assert!(!active.load(Ordering::Acquire));
        assert!(Prompt::acquire(&active).is_some());
    }
    #[test]
    fn full_exit_watchdog_can_only_be_armed_once() {
        let armed = AtomicBool::new(false);
        assert!(claim_exit_watchdog(&armed));
        assert!(!claim_exit_watchdog(&armed));
    }
}
