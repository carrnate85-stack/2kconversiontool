//! Synthetic publisher tests only. Never invoke the fixed Main entrypoints.
#![allow(dead_code)]
#[path = "../src/saved_media_publish.rs"]
mod saved_media_publish;
fn main() -> anyhow::Result<()> {
    saved_media_publish::tests::run()
}
