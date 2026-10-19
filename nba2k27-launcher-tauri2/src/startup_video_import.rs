//! One-owner startup-video import lifecycle.
//!
//! The catalog importer already validates, copies, hashes and publishes safely.
//! This coordinator supplies the missing UI/runtime ownership around that work:
//! one active generation, exact-ID cancellation and prompt shutdown cancellation.
use anyhow::{Result, ensure};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone)]
struct Active {
    id: String,
    cancel: Arc<AtomicBool>,
    cancellable: Arc<AtomicBool>,
}

#[derive(Default)]
pub(crate) struct Coordinator {
    active: Mutex<Option<Active>>,
}

pub(crate) struct Lease {
    pub cancel: Arc<AtomicBool>,
    cancellable: Arc<AtomicBool>,
}

pub(crate) fn validate_frontend_id(id: &str) -> Result<()> {
    let uuid = id
        .strip_prefix("video-import-")
        .ok_or_else(|| anyhow::anyhow!("Invalid startup-video import ID"))?;
    ensure!(uuid.len() == 36, "Invalid startup-video import ID");
    for (index, byte) in uuid.bytes().enumerate() {
        let separator = matches!(index, 8 | 13 | 18 | 23);
        ensure!(
            if separator {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            },
            "Invalid startup-video import ID"
        );
    }
    Ok(())
}

impl Lease {
    pub(crate) fn commit_started(&self) {
        self.cancellable.store(false, Ordering::Release);
    }
}

impl Coordinator {
    pub(crate) fn begin(&self, id: &str) -> Result<Lease> {
        ensure!(
            !id.is_empty() && id.len() <= 128 && id.is_ascii(),
            "Invalid startup-video import ID"
        );
        let mut active = self
            .active
            .lock()
            .map_err(|_| anyhow::anyhow!("Startup-video import state unavailable"))?;
        ensure!(
            active.is_none(),
            "Another startup-video import is already in progress"
        );
        let cancel = Arc::new(AtomicBool::new(false));
        let cancellable = Arc::new(AtomicBool::new(true));
        *active = Some(Active {
            id: id.into(),
            cancel: cancel.clone(),
            cancellable: cancellable.clone(),
        });
        Ok(Lease {
            cancel,
            cancellable,
        })
    }

    pub(crate) fn finish(&self, id: &str) {
        if let Ok(mut active) = self.active.lock() {
            if active.as_ref().is_some_and(|current| current.id == id) {
                *active = None;
            }
        }
    }

    pub(crate) fn cancel(&self, id: &str) -> Result<bool> {
        let active = self
            .active
            .lock()
            .map_err(|_| anyhow::anyhow!("Startup-video import state unavailable"))?;
        let Some(active) = active.as_ref() else {
            return Ok(false);
        };
        ensure!(active.id == id, "Startup-video import ID is stale");
        if !active.cancellable.load(Ordering::Acquire) {
            return Ok(false);
        }
        active.cancel.store(true, Ordering::Release);
        Ok(true)
    }

    pub(crate) fn cancel_current(&self) {
        if let Ok(active) = self.active.lock() {
            if let Some(active) = active.as_ref() {
                if active.cancellable.load(Ordering::Acquire) {
                    active.cancel.store(true, Ordering::Release);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_generation_owns_cancel_and_stale_ids_cannot_touch_it() {
        let coordinator = Coordinator::default();
        let first = coordinator.begin("first").unwrap();
        assert!(coordinator.begin("second").is_err());
        assert!(coordinator.cancel("stale").is_err());
        assert!(!first.cancel.load(Ordering::Acquire));
        assert!(coordinator.cancel("first").unwrap());
        assert!(first.cancel.load(Ordering::Acquire));
        coordinator.finish("stale");
        assert!(coordinator.begin("second").is_err());
        coordinator.finish("first");
        assert!(coordinator.begin("second").is_ok());
    }

    #[test]
    fn commit_boundary_rejects_late_cancel_and_shutdown_cancels_copy() {
        let coordinator = Coordinator::default();
        let copying = coordinator.begin("copying").unwrap();
        coordinator.cancel_current();
        assert!(copying.cancel.load(Ordering::Acquire));
        coordinator.finish("copying");

        let committing = coordinator.begin("committing").unwrap();
        committing.commit_started();
        coordinator.cancel_current();
        assert!(!committing.cancel.load(Ordering::Acquire));
        assert!(!coordinator.cancel("committing").unwrap());
        coordinator.finish("committing");
        assert!(!coordinator.cancel("committing").unwrap());
    }

    #[test]
    fn frontend_identity_has_one_fixed_bounded_shape() {
        validate_frontend_id("video-import-00112233-4455-6677-8899-aabbccddeeff").unwrap();
        for invalid in [
            "",
            "cancel-generation",
            "video-import-00112233445566778899aabbccddeeff",
            "video-import-../../00112233-4455-6677-8899-aabbccddeeff",
            "video-import-00112233-4455-6677-8899-aabbccddeefg",
        ] {
            assert!(validate_frontend_id(invalid).is_err(), "{invalid}");
        }
    }
}
