use anyhow::{Result, ensure};
use serde_json::Value;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Staged,
    Catalog,
    Independent,
}

impl Kind {
    pub fn request(command: &str, payload: &Value) -> Self {
        match payload["_cmtPreviewKind"].as_str() {
            Some("staged") => Self::Staged,
            Some("catalog") => Self::Catalog,
            Some("independent") => Self::Independent,
            _ => match command {
                "native-preview-resources" | "preview-resources" => Self::Staged,
                "native-preview-hair" | "native-hair-preview" | "native-catalog-geometry" => {
                    Self::Catalog
                }
                _ => Self::Independent,
            },
        }
    }
}

#[derive(Clone, Copy)]
pub struct Ticket {
    kind: Kind,
    epoch: u64,
}
#[derive(Default)]
struct State {
    epochs: [u64; 3],
    active: Option<(Kind, Arc<AtomicBool>)>,
}
#[derive(Default)]
pub struct PreviewJobs(Mutex<State>);
pub struct Registration<'a> {
    jobs: &'a PreviewJobs,
    pub cancel: Arc<AtomicBool>,
}

impl PreviewJobs {
    pub fn ticket(&self, kind: Kind) -> Ticket {
        Ticket {
            kind,
            epoch: self.0.lock().unwrap().epochs[kind as usize],
        }
    }
    pub fn current(&self, ticket: Ticket) -> bool {
        self.0.lock().unwrap().epochs[ticket.kind as usize] == ticket.epoch
    }
    pub fn begin(&self, ticket: Ticket) -> Result<Registration<'_>> {
        let mut state = self.0.lock().unwrap();
        ensure!(
            state.epochs[ticket.kind as usize] == ticket.epoch,
            "Operation cancelled"
        );
        ensure!(
            state.active.is_none(),
            "A native preview operation is already registered"
        );
        let cancel = Arc::new(AtomicBool::new(false));
        state.active = Some((ticket.kind, cancel.clone()));
        Ok(Registration { jobs: self, cancel })
    }
    pub fn cancel(&self, kind: Option<Kind>) {
        let mut state = self.0.lock().unwrap();
        if let Some(kind) = kind {
            state.epochs[kind as usize] += 1;
        } else {
            for epoch in &mut state.epochs {
                *epoch += 1;
            }
        }
        if let Some((active_kind, token)) = &state.active {
            if kind.is_none_or(|kind| kind == *active_kind) {
                token.store(true, Ordering::SeqCst);
            }
        }
    }
    pub fn active(&self, kind: Kind) -> bool {
        self.0
            .lock()
            .unwrap()
            .active
            .as_ref()
            .is_some_and(|(active, _)| *active == kind)
    }
}
impl Drop for Registration<'_> {
    fn drop(&mut self) {
        let mut state = self.jobs.0.lock().unwrap();
        if state
            .active
            .as_ref()
            .is_some_and(|(_, token)| Arc::ptr_eq(token, &self.cancel))
        {
            state.active = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_is_scoped_and_rejects_only_its_queued_requests() {
        let jobs = PreviewJobs::default();
        let staged = jobs.ticket(Kind::Staged);
        let catalog = jobs.ticket(Kind::Catalog);
        let active = jobs.begin(catalog).unwrap();
        jobs.cancel(Some(Kind::Staged));
        assert!(!active.cancel.load(Ordering::SeqCst));
        assert!(jobs.current(catalog));
        assert!(!jobs.current(staged));
        assert!(jobs.active(Kind::Catalog));
        drop(active);
        assert!(jobs.begin(staged).is_err());
        let active = jobs.begin(jobs.ticket(Kind::Staged)).unwrap();
        jobs.cancel(Some(Kind::Catalog));
        assert!(!active.cancel.load(Ordering::SeqCst));
        jobs.cancel(Some(Kind::Staged));
        assert!(active.cancel.load(Ordering::SeqCst));
        drop(active);
        assert!(!jobs.active(Kind::Staged));
    }
    #[test]
    fn independent_reference_work_survives_both_preview_cancellations() {
        let jobs = PreviewJobs::default();
        let active = jobs.begin(jobs.ticket(Kind::Independent)).unwrap();
        jobs.cancel(Some(Kind::Staged));
        jobs.cancel(Some(Kind::Catalog));
        assert!(!active.cancel.load(Ordering::SeqCst));
        jobs.cancel(None);
        assert!(active.cancel.load(Ordering::SeqCst));
    }
    #[test]
    fn command_defaults_and_explicit_reference_scope_are_preserved() {
        assert_eq!(
            Kind::request("native-catalog-geometry", &Value::Null),
            Kind::Catalog
        );
        assert_eq!(
            Kind::request("preview-resources", &Value::Null),
            Kind::Staged
        );
        assert_eq!(
            Kind::request("read-archive", &Value::Null),
            Kind::Independent
        );
        assert_eq!(
            Kind::request(
                "preview-resources",
                &serde_json::json!({"_cmtPreviewKind":"independent"})
            ),
            Kind::Independent
        );
    }
}
