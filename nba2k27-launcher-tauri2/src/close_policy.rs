//! Normal-window close presentation only. Ownership is supplied by the native
//! session service; this module never infers it from a PID/name or a journal.
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) fn should_prompt(smoke: bool, main_window: bool, owned_session_active: bool) -> bool {
    !smoke && main_window && owned_session_active
}
/// Win32 IDYES=6. Explicit No, a failed dialog or an unknown answer all retain
/// the pre-existing exit path; this courtesy prompt must never trap app exit.
pub(crate) fn minimize(answer: i32) -> bool {
    answer == 6
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_original_owned_non_smoke_main_sessions_request_a_prompt() {
        assert!(should_prompt(false, true, true));
        for input in [
            (true, true, true),
            (false, false, true),
            (false, true, false),
            (true, false, false),
        ] {
            assert!(!should_prompt(input.0, input.1, input.2));
        }
    }
    #[test]
    fn explicit_no_and_dialog_errors_do_not_trap_exit() {
        assert!(minimize(6));
        for code in [0, 2, 7, 99, -1] {
            assert!(!minimize(code));
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
}
