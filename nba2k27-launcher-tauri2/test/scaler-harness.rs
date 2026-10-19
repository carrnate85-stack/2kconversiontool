//! Developer-only offline compilation harness. No application entry point or
//! process opening capability is included in this test binary.
mod blacktop_native {
    pub(crate) trait Memory {
        fn base(&self) -> u64;
        fn read(&self, at: u64, len: usize) -> anyhow::Result<Vec<u8>>;
        fn checkpoint(&self) -> anyhow::Result<()> {
            Ok(())
        }
    }
}
#[path = "../src/scaler_policy.rs"]
mod scaler_policy;
#[path = "../src/scaler_v2.rs"]
mod scaler_v2;
