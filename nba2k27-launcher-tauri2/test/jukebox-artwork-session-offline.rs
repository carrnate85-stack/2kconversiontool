#![allow(dead_code)]
#[path = "../src/file_identity.rs"]
mod file_identity;
#[path = "../src/jukebox_artwork_session.rs"]
mod jukebox_artwork_session;
#[path = "../src/jukebox_catalog.rs"]
mod jukebox_catalog;
mod loader_files {
    pub struct Directories;
    impl Directories {
        pub fn pin(_: &std::path::Path) -> anyhow::Result<Self> {
            anyhow::bail!("This pure planner harness never reads an installed game")
        }
    }
}
mod backend {
    pub fn unique_id() -> String {
        format!("artwork-session-{}", std::process::id())
    }
}
