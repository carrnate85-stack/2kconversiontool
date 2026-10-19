//! Isolated developer-only read-only roster profile tests; no process API.
mod blacktop_native {
    pub(crate) trait Memory {
        fn base(&self) -> u64;
        fn read(&self, at: u64, len: usize) -> anyhow::Result<Vec<u8>>;
        fn checkpoint(&self) -> anyhow::Result<()> {
            Ok(())
        }
    }
}
#[path = "../src/roster_v2_profile.rs"]
mod roster_v2_profile;
