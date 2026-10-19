//! Offline Blacktop-WNBA policy port. This is NOT a native write capability.
//!
//! Blacktop's production WPF path differs from Freestyle: it swaps a stock
//! roster-loader callback at an inactive Home/Quick Play boundary, then exposes
//! category 3 using a process-lifetime executable bridge. The Version 2 callback,
//! loader, dispatch and continuation have not all been independently established.
//! Keep runtime disabled; never substitute Freestyle's four data-only DWORDs.
use anyhow::{Result, ensure};
use std::collections::BTreeSet;

pub(crate) const RUNTIME_MAPPED: bool = false;
pub(crate) const BLOCKER: &str = "Blacktop WNBA Version 2 callback and stock loader are mapped, but category dispatch/continuation and the executable bridge still require independent proof";
pub(crate) const LABEL_RECORD: u64 = 0xACFC4D0;
pub(crate) const DELEGATE_VTABLE_CANDIDATE: u64 = 0x74CD158;
pub(crate) const DELEGATE_OBJECT_CANDIDATE: u64 = 0x9B31BB0;
pub(crate) const CALLBACK_SLOT_CANDIDATE: u64 = 0x74CDE68;
pub(crate) const STOCK_CALLBACK_CANDIDATE: u64 = 0x23531A0;
pub(crate) const STOCK_MIXED_LOADER_CANDIDATE: u64 = 0x236FB30;
pub(crate) const EURO_LABEL: u64 = 0x6FC058B0442652CD;
pub(crate) const WOMEN_LABEL: u64 = 0x0F39A44C3426C962;
const HOME: u32 = 0x6509837B;
const QUICK_PLAY: u32 = 0x079964C0;
const SELECTION: u32 = 0x92DF06DE;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Boundary {
    pub current: u32,
    pub alternate: u32,
    pub stage: u32,
    pub active: u8,
    pub kind: u8,
}
impl Boundary {
    pub(crate) fn inactive_menu(self) -> bool {
        self.current == self.alternate
            && self.active == 0
            && self.kind == 0
            && ((self.current == HOME && self.stage <= 1)
                || (self.current == QUICK_PLAY && self.stage == 1))
    }
    pub(crate) fn entry_complete(self) -> bool {
        self.current == SELECTION && self.alternate == SELECTION && self.stage == 3
    }
    pub(crate) fn safe_selection(self) -> bool {
        self.entry_complete() && self.active == 0 && self.kind == 0
    }
}
pub(crate) fn verify_women_ids(ids: &[u16]) -> Result<()> {
    ensure!(
        (2..=32).contains(&ids.len())
            && ids.iter().all(|id| *id <= 1023)
            && ids.iter().copied().collect::<BTreeSet<_>>().len() == ids.len(),
        "Blacktop roster must expose 2–32 distinct initialized WNBA teams"
    );
    Ok(())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    RestoreCategory,
    RestoreRosterCallback,
    ArmRosterCallback,
    ExposeWomenCategory,
}
/// Policy state is committed only after a future independently reviewed
/// executor verifies its exact owned mutation. No addresses/bytes are emitted.
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SessionPolicy {
    pub armed: bool,
    pub category_owned: bool,
}
impl SessionPolicy {
    pub(crate) fn propose(
        self,
        boundary: Boundary,
        expired: bool,
        roster_ready: bool,
        women: &[u16],
        bridge_ready: bool,
    ) -> Result<Vec<Action>> {
        let mut next = self;
        let mut actions = Vec::new();
        if next.category_owned && !boundary.safe_selection() {
            actions.push(Action::RestoreCategory);
            next.category_owned = false;
        }
        if next.armed && (boundary.entry_complete() || expired) {
            actions.push(Action::RestoreRosterCallback);
            next.armed = false;
        }
        if boundary.inactive_menu() && !next.armed {
            if next.category_owned {
                actions.push(Action::RestoreCategory);
            }
            actions.push(Action::ArmRosterCallback);
            return Ok(actions);
        }
        if boundary.safe_selection() && !next.armed && !next.category_owned && roster_ready {
            verify_women_ids(women)?;
            if bridge_ready {
                actions.push(Action::ExposeWomenCategory);
            }
        }
        Ok(actions)
    }
    pub(crate) fn commit(&mut self, action: Action) {
        match action {
            Action::ArmRosterCallback => self.armed = true,
            Action::RestoreRosterCallback => self.armed = false,
            Action::ExposeWomenCategory => self.category_owned = true,
            Action::RestoreCategory => self.category_owned = false,
        }
    }
}
/// Hard stop for any future runtime caller. Pure policy coverage and matching
/// label initializer are not a complete native profile or gameplay validation.
pub(crate) fn require_runtime_mapping() -> Result<()> {
    ensure!(RUNTIME_MAPPED, "{BLOCKER}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn b(hash: u32, stage: u32) -> Boundary {
        Boundary {
            current: hash,
            alternate: hash,
            stage,
            active: 0,
            kind: 0,
        }
    }
    #[test]
    fn home_and_quick_play_rearm_without_releasing_during_loading() {
        let mut s = SessionPolicy::default();
        for (i, menu) in [HOME, QUICK_PLAY, QUICK_PLAY, HOME].into_iter().enumerate() {
            let actions = s
                .propose(
                    b(menu, if i == 0 { 0 } else { 1 }),
                    false,
                    false,
                    &[],
                    false,
                )
                .unwrap();
            assert!(actions.contains(&Action::ArmRosterCallback));
            for a in actions {
                s.commit(a);
            }
            assert!(s.armed);
            for stage in [1, 2] {
                assert!(
                    s.propose(b(SELECTION, stage), false, false, &[], false)
                        .unwrap()
                        .is_empty()
                );
            }
            let actions = s
                .propose(b(SELECTION, 3), false, true, &[300, 301], true)
                .unwrap();
            assert_eq!(
                actions,
                vec![Action::RestoreRosterCallback, Action::ExposeWomenCategory]
            );
            for a in actions {
                s.commit(a);
            }
            assert!(!s.armed && s.category_owned);
        }
    }
    #[test]
    fn active_foreign_mixed_and_unverified_menu_stages_never_arm() {
        for hash in [HOME, QUICK_PLAY, SELECTION, 0, 0xB4D37B22, 0xDEADBEEF] {
            for stage in [0, 1, 2, 3, 4, 99] {
                let mut boundary = b(hash, stage);
                for kind in 0..2 {
                    for active in 0..2 {
                        boundary.kind = kind;
                        boundary.active = active;
                        let actions = SessionPolicy::default()
                            .propose(boundary, false, false, &[], false)
                            .unwrap();
                        assert_eq!(
                            actions.contains(&Action::ArmRosterCallback),
                            boundary.inactive_menu()
                        );
                    }
                }
                boundary.alternate = hash ^ 1;
                assert!(!boundary.inactive_menu());
            }
        }
    }
    #[test]
    fn category_restores_on_exit_and_deadline_never_claims_unowned_callback() {
        let state = SessionPolicy {
            armed: false,
            category_owned: true,
        };
        assert_eq!(
            state
                .propose(b(0xB4D37B22, 4), false, false, &[], false)
                .unwrap(),
            vec![Action::RestoreCategory]
        );
        assert!(
            SessionPolicy::default()
                .propose(b(SELECTION, 1), true, false, &[], false)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            SessionPolicy {
                armed: true,
                category_owned: false
            }
            .propose(b(SELECTION, 2), true, false, &[], false)
            .unwrap(),
            vec![Action::RestoreRosterCallback]
        );
    }
    #[test]
    fn women_roster_minimum_maximum_duplicates_and_ranges_match_production() {
        for ids in [
            vec![],
            vec![300],
            vec![300, 300],
            vec![300, 1024],
            (0..33).collect(),
        ] {
            assert!(verify_women_ids(&ids).is_err());
        }
        for ids in [vec![300, 301], (0..32).collect()] {
            verify_women_ids(&ids).unwrap();
        }
    }
    #[test]
    fn no_category_before_stable_roster_or_executable_bridge_and_no_runtime_mapping() {
        let state = SessionPolicy::default();
        assert!(
            state
                .propose(b(SELECTION, 3), false, false, &[300, 301], true)
                .unwrap()
                .is_empty()
        );
        assert!(
            state
                .propose(b(SELECTION, 3), false, true, &[300, 301], false)
                .unwrap()
                .is_empty()
        );
        assert!(require_runtime_mapping().is_err());
    }
}
