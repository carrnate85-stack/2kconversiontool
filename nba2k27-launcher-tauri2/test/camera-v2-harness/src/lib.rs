//! Offline-only harness: no Windows/process APIs are linked.
mod blacktop_native {
    pub(crate) trait Memory {
        fn base(&self) -> u64;
        fn read(&self, at: u64, len: usize) -> anyhow::Result<Vec<u8>>;
        fn checkpoint(&self) -> anyhow::Result<()> {
            Ok(())
        }
    }
}
mod camera_names {
    pub(crate) fn key(id: u32) -> anyhow::Result<u64> {
        Ok(match id {
            51 => 0x070A02F4676F7A26,
            53 => 0x724E93A6FB8B2389,
            54 => 0xB800A4C12E23B6BD,
            52 => 0x81C8B0C640438EFB,
            57 => 0x2AFCCD8CA86B576C,
            59 => 0x3B94D5AAE1F235DE,
            55 => 0x7DE7AF563ED79DC4,
            _ => anyhow::bail!("Unknown camera key"),
        })
    }
}
#[path = "../../../src/camera_policy.rs"]
mod camera_policy;
#[path = "../../../src/camera_v2.rs"]
mod camera_v2;
#[path = "../../../src/scaler_policy.rs"]
mod scaler_policy;
#[path = "../../../src/scaler_v2.rs"]
mod scaler_v2;
