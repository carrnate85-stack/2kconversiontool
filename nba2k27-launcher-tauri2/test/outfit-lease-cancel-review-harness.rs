//! Lease-only review harness: actual lease, persistent IDs, ordinary-directory
//! pins and Win32 event/ACL APIs. No process scanning, VM or game launch APIs.
#![allow(dead_code)]
mod native_session {
    pub(crate) fn same_path(a: &str, b: &std::path::Path) -> bool {
        a.trim_start_matches(r"\\?\")
            .eq_ignore_ascii_case(b.to_string_lossy().trim_start_matches(r"\\?\"))
    }
}
mod session_safety {
    #[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    pub(crate) struct Identity {
        pub pid: u32,
        pub started: String,
        pub path: String,
    }
}
mod provider_protocol {
    pub(crate) const PIPE_PREFIX: &str = r"\\.\pipe\NBA2K27ModProvider-";
}
mod backend {
    use std::{
        io::Write,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(1);
    pub(crate) fn unique_id() -> String {
        format!(
            "{:016x}{:016x}{:016x}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            NEXT.fetch_add(1, Ordering::Relaxed)
        )
    }
    pub(crate) fn atomic_json(
        p: &std::path::Path,
        value: &impl serde::Serialize,
    ) -> anyhow::Result<()> {
        let mut f = std::fs::File::create(p)?;
        f.write_all(&serde_json::to_vec(value)?)?;
        f.sync_all()?;
        Ok(())
    }
    pub(crate) fn atomic_json_new(
        p: &std::path::Path,
        value: &impl serde::Serialize,
    ) -> anyhow::Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(p)?;
        f.write_all(&serde_json::to_vec(value)?)?;
        f.sync_all()?;
        Ok(())
    }
}
#[path = "../src/file_identity.rs"]
mod file_identity;
#[path = "../src/loader_files.rs"]
mod loader_files;
#[path = "../src/outfit_worker_v2_lease.rs"]
mod outfit_worker_v2_lease;
#[path = "outfit-worker-v2-acl-fixture.rs"]
mod provider_access;
