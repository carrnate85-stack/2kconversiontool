//! Pure scoreboard/profile/session fixtures only. No game/process/native API.
#![allow(dead_code)]
#[path = "../../src/scoreboard_profile.rs"]
mod scoreboard_profile;
mod blacktop_native {
    include!(concat!(env!("OUT_DIR"), "/native_types.rs"));
    pub(crate) struct Reader;
    impl Reader {
        pub(crate) fn open_observer(
            _: &crate::session_safety::Identity,
            _: &std::path::Path,
        ) -> anyhow::Result<Self> {
            anyhow::bail!("Process observer forbidden in offline scoreboard harness")
        }
    }
    impl Memory for Reader {
        fn base(&self) -> u64 {
            0x140000000
        }
        fn read(&self, _: u64, _: usize) -> anyhow::Result<Vec<u8>> {
            anyhow::bail!("Game memory access forbidden in offline scoreboard harness")
        }
    }
}
mod native_session {
    pub(crate) fn same_path(a: &str, b: &std::path::Path) -> bool {
        a.trim_start_matches(r"\\?\")
            .eq_ignore_ascii_case(b.to_string_lossy().trim_start_matches(r"\\?\"))
    }
}
mod session_safety {
    use serde::{Deserialize, Serialize};
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    pub struct Identity {
        pub pid: u32,
        pub started: String,
        pub path: String,
    }
    pub struct Process {
        pub name: String,
        pub pid: u32,
        pub parent: u32,
        pub identity: Option<Identity>,
    }
    pub fn launcher_process(p: &Process) -> bool {
        p.name.eq_ignore_ascii_case("2k Launcher.exe") || p.name.eq_ignore_ascii_case("node.exe")
    }
    pub fn processes() -> anyhow::Result<Vec<Process>> {
        anyhow::bail!("Process enumeration forbidden in offline scoreboard harness")
    }
    pub fn identity(_: u32) -> anyhow::Result<Identity> {
        anyhow::bail!("Process identity forbidden in offline scoreboard harness")
    }
    static GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    pub fn write_gate() -> anyhow::Result<std::sync::MutexGuard<'static, ()>> {
        Ok(GATE.lock().unwrap())
    }
}
mod backend {
    pub fn unique_id() -> String {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        format!(
            "{:016x}{:016x}{:016x}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        )
    }
}
#[path = "../../src/file_identity.rs"]
mod file_identity;
mod blacktop_arena {
    include!(concat!(env!("OUT_DIR"), "/inspect_types.rs"));
}
mod game_rules {
    include!(concat!(env!("OUT_DIR"), "/rule_types.rs"));
}
mod sigconfig {
    pub const MAX_BYTES: usize = 16 * 1024 * 1024;
}
mod selector_tables {
    pub const PATHS: [&str; 3] = [
        "char/signatureconfigs_all.iff",
        "char/signatureconfigs_editplayer.iff",
        "char/fpplayer_global.iff",
    ];
    include!(concat!(env!("OUT_DIR"), "/archive_codec.rs"));
}
mod loader_files {
    include!(concat!(env!("OUT_DIR"), "/directory_pins.rs"));
}
mod stock_selectors {
    include!(concat!(env!("OUT_DIR"), "/stock_decoder.rs"));
}
#[path = "../../src/rules_v2.rs"]
mod rules_v2;
#[path = "../../src/scoreboard_assets.rs"]
mod scoreboard_assets;
#[path = "../../src/scoreboard_layout.rs"]
mod scoreboard_layout;
#[path = "../../src/scoreboard_runtime.rs"]
mod scoreboard_runtime;
#[path = "../../src/scoreboard_session.rs"]
mod scoreboard_session;
#[path = "../../src/scoreboard_v2.rs"]
mod scoreboard_v2;
