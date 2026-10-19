#![allow(dead_code)]
#[path = "../src/file_identity.rs"]
mod file_identity;
#[path = "../src/outfit_worker_v2_loader.rs"]
mod outfit_worker_v2_loader;
#[path = "../src/provider_build_profile.rs"]
mod provider_build_profile;
mod native_session {
    pub fn same_path(a: &str, b: &std::path::Path) -> bool {
        a.trim_start_matches(r"\\?\")
            .eq_ignore_ascii_case(b.to_string_lossy().trim_start_matches(r"\\?\"))
    }
}
mod backend {
    pub fn unique_id() -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
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
}
mod provider_access {
    pub struct Verified {
        pub build: serde_json::Value,
    }
    pub fn verify_build(_: &std::path::Path) -> anyhow::Result<Verified> {
        anyhow::bail!("This isolated fixture never runs production process discovery")
    }
}
