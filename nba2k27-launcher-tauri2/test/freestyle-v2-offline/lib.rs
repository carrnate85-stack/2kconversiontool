//! Isolated disk/fixture harness. No game process APIs or memory write executor.
#![allow(dead_code)]
mod blacktop_native {
    pub(crate) trait Memory {
        fn base(&self) -> u64;
        fn read(&self, at: u64, len: usize) -> anyhow::Result<Vec<u8>>;
        fn checkpoint(&self) -> anyhow::Result<()> {
            Ok(())
        }
    }
}
#[path = "../../src/blacktop_unlock_v2.rs"]
mod blacktop_unlock_v2;
#[path = "../../src/camera_name_files.rs"]
mod camera_name_files;
#[path = "../../src/freestyle_v2.rs"]
mod freestyle_v2;
mod loader_files {
    include!(concat!(env!("OUT_DIR"), "/actual_loader_primitives.rs"));
}
#[path = "../../src/media_import.rs"]
mod media_import;
#[cfg(test)]
mod mod_profile {
    include!(concat!(env!("OUT_DIR"), "/actual_profile_junction.rs"));
}
#[path = "../../src/startup_mp4.rs"]
mod startup_mp4;
#[path = "../../src/startup_video_policy.rs"]
mod startup_video_policy;
mod backend {
    include!(concat!(env!("OUT_DIR"), "/actual_backend_atomic.rs"));
}
mod native_session {
    include!(concat!(env!("OUT_DIR"), "/actual_same_path.rs"));
}
#[cfg(test)]
mod provider_access {
    use anyhow::{Result, ensure};
    use sha2::{Digest, Sha256};
    use std::{fs::File, io::Read, path::Path};
    pub(crate) struct Verified {
        pub build: serde_json::Value,
    }
    pub(crate) fn verify_build(path: &Path) -> Result<Verified> {
        ensure!(
            path.file_name()
                .is_some_and(|n| n.eq_ignore_ascii_case("NBA2K27.exe")),
            "Wrong disk image"
        );
        let mut f = File::open(path)?;
        ensure!(
            f.metadata()?.len() == 870548760,
            "Version 2 disk size differs"
        );
        let mut hash = Sha256::new();
        let mut block = [0; 1048576];
        loop {
            let n = f.read(&mut block)?;
            if n == 0 {
                break;
            }
            hash.update(&block[..n]);
        }
        ensure!(
            format!("{:X}", hash.finalize())
                == "29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1",
            "Version 2 full disk hash differs"
        );
        Ok(Verified {
            build: serde_json::json!({"label":"Version 2"}),
        })
    }
}
