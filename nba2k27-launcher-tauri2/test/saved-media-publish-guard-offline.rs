#[path = "saved-media-publish-closed.rs"]
mod closed;
fn main() -> anyhow::Result<()> { closed::run_tests() }
