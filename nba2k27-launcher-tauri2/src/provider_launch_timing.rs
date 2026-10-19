//! Bounded diagnostic history. Caller supplies monotonic elapsed milliseconds;
//! this type performs no clock, file, process or game operation.
use serde::Serialize;
use std::collections::VecDeque;
const MAX_TRANSITIONS: usize = 64;
const MAX_STEPS: usize = 48;
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Transition {
    phase: String,
    started_ms: u64,
    elapsed_ms: u64,
    completed: bool,
}
/// One measured sub-step inside (or across) phases, e.g. players-activate or
/// a parallel prepare worker. Steps may overlap; they never drive the phase.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Step {
    name: String,
    started_ms: u64,
    elapsed_ms: u64,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Snapshot {
    elapsed_ms: u64,
    phase_elapsed_ms: u64,
    previous_phase: Option<String>,
    previous_phase_elapsed_ms: Option<u64>,
    dropped_transitions: u64,
    transitions: Vec<Transition>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    steps: Vec<Step>,
}
#[derive(Default)]
pub(super) struct History {
    last_ms: u64,
    dropped: u64,
    transitions: VecDeque<Transition>,
    steps: VecDeque<Step>,
}
impl History {
    /// Record a finished sub-step. Bounded; oldest steps are dropped first.
    /// Diagnostics only: never consulted by launch or cleanup decisions.
    pub(super) fn step(
        &mut self,
        name: &str,
        started_ms: u64,
        elapsed_ms: u64,
        ok: bool,
        detail: Option<&str>,
    ) {
        if self.steps.len() == MAX_STEPS {
            self.steps.pop_front();
        }
        self.steps.push_back(Step {
            name: name.chars().take(96).collect(),
            started_ms,
            elapsed_ms,
            ok,
            detail: detail.map(|d| d.chars().take(160).collect()),
        });
    }
    pub(super) fn observe(&mut self, phase: &str, elapsed_ms: u64) -> Snapshot {
        // Clock regressions or integer saturation must never create a negative
        // duration, and diagnostics must not prevent owned cleanup.
        let now = elapsed_ms.max(self.last_ms);
        self.last_ms = now;
        let phase: String = phase.chars().take(96).collect();
        if self
            .transitions
            .back()
            .is_none_or(|last| last.phase != phase)
        {
            if let Some(last) = self.transitions.back_mut() {
                last.elapsed_ms = now.saturating_sub(last.started_ms);
                last.completed = true;
            }
            if self.transitions.len() == MAX_TRANSITIONS {
                self.transitions.pop_front();
                self.dropped = self.dropped.saturating_add(1);
            }
            self.transitions.push_back(Transition {
                phase,
                started_ms: now,
                elapsed_ms: 0,
                completed: false,
            });
        }
        let phase_elapsed_ms = self
            .transitions
            .back()
            .map_or(0, |last| now.saturating_sub(last.started_ms));
        let mut transitions: Vec<_> = self.transitions.iter().cloned().collect();
        if let Some(last) = transitions.last_mut() {
            last.elapsed_ms = phase_elapsed_ms;
        }
        let previous = transitions.iter().rev().nth(1);
        Snapshot {
            elapsed_ms: now,
            phase_elapsed_ms,
            previous_phase: previous.map(|p| p.phase.clone()),
            previous_phase_elapsed_ms: previous.map(|p| p.elapsed_ms),
            dropped_transitions: self.dropped,
            transitions,
            steps: self.steps.iter().cloned().collect(),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prior_phase_duration_is_recorded_before_new_work_and_same_phase_does_not_append() {
        let mut history = History::default();
        let first = history.observe("preparing-cameras", 10);
        assert_eq!(first.elapsed_ms, 10);
        assert_eq!(first.phase_elapsed_ms, 0);
        assert!(first.previous_phase.is_none());
        let update = history.observe("preparing-cameras", 10_010);
        assert_eq!(update.transitions.len(), 1);
        assert_eq!(update.phase_elapsed_ms, 10_000);
        let next = history.observe("preparing-scoreboards", 20_010);
        assert_eq!(next.transitions.len(), 2);
        assert_eq!(next.previous_phase.as_deref(), Some("preparing-cameras"));
        assert_eq!(next.previous_phase_elapsed_ms, Some(20_000));
        assert!(next.transitions[0].completed);
        assert!(!next.transitions[1].completed);
        assert_eq!(next.phase_elapsed_ms, 0);
    }
    #[test]
    fn repeated_active_updates_never_grow_history_and_cleanup_always_remains() {
        let mut history = History::default();
        history.observe("loader-active", 1);
        for now in 2..10_000 {
            assert_eq!(history.observe("loader-active", now).transitions.len(), 1);
        }
        for now in 10_000..10_200 {
            history.observe(&format!("fixture-phase-{now}"), now);
        }
        history.observe("held-until-game-exit", 10_200);
        history.observe("waiting-for-safe-cleanup", 10_201);
        history.observe("restoring-files", 10_202);
        history.observe("restoring-session", 10_203);
        let end = history.observe("closed", 10_204);
        assert_eq!(end.transitions.len(), 64);
        assert_eq!(end.dropped_transitions, 142);
        assert_eq!(end.transitions.last().unwrap().phase, "closed");
        assert!(end.transitions.iter().any(|p| p.phase == "restoring-files"));
        assert!(
            end.transitions
                .iter()
                .any(|p| p.phase == "restoring-session")
        );
        assert_eq!(end.previous_phase.as_deref(), Some("restoring-session"));
    }
    #[test]
    fn milliseconds_are_monotonic_and_saturation_cannot_overflow() {
        let mut history = History::default();
        history.observe("one", 100);
        let a = history.observe("one", 50);
        assert_eq!(a.elapsed_ms, 100);
        assert_eq!(a.phase_elapsed_ms, 0);
        let b = history.observe("two", u64::MAX);
        assert_eq!(b.previous_phase_elapsed_ms, Some(u64::MAX - 100));
        let c = history.observe("closed", 0);
        assert_eq!(c.elapsed_ms, u64::MAX);
        assert_eq!(c.previous_phase_elapsed_ms, Some(0));
        assert_eq!(c.phase_elapsed_ms, 0);
    }
    #[test]
    fn history_serializes_stable_optional_elapsed_fields_without_wall_time() {
        let mut history = History::default();
        history.observe("preparing-scoreboards", 123);
        let json = serde_json::to_value(history.observe("waiting-for-game", 2_123)).unwrap();
        assert_eq!(json["elapsedMs"], 2123);
        assert_eq!(json["previousPhaseElapsedMs"], 2000);
        assert_eq!(json["transitions"][0]["startedMs"], 123);
        assert_eq!(json["transitions"][0]["elapsedMs"], 2000);
        assert_eq!(json["transitions"][0]["completed"], true);
        assert!(json.get("steps").is_none());
    }
    #[test]
    fn steps_are_bounded_overlapping_diagnostics_that_never_change_phases() {
        let mut history = History::default();
        history.observe("preparing-players", 100);
        history.step("players-create", 100, 50, true, None);
        history.step("jukebox-prepare", 100, 900, true, Some("parallel"));
        let json = serde_json::to_value(history.observe("preparing-players", 1_000)).unwrap();
        assert_eq!(json["transitions"].as_array().unwrap().len(), 1);
        assert_eq!(json["steps"][1]["name"], "jukebox-prepare");
        assert_eq!(json["steps"][1]["elapsedMs"], 900);
        assert_eq!(json["steps"][1]["detail"], "parallel");
        assert!(json["steps"][0].get("detail").is_none());
        for i in 0..200 {
            history.step(&format!("s{i}"), i, 1, i % 2 == 0, None);
        }
        let json = serde_json::to_value(history.observe("closed", 2_000)).unwrap();
        assert_eq!(json["steps"].as_array().unwrap().len(), MAX_STEPS);
        assert_eq!(json["steps"][MAX_STEPS - 1]["name"], "s199");
    }
}
