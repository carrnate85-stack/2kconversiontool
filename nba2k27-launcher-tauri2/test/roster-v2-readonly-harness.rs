//! Isolated disk/fake-memory checks only. Never opens a process or writes.
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
#[path = "../src/roster_v2_readonly.rs"]
mod roster_v2_readonly;
