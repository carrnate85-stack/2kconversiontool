#![allow(dead_code)]
#![allow(dead_code)]
mod blacktop_native {
    include!(concat!(env!("OUT_DIR"), "/native_types.rs"));
    pub fn boundary(c: &ContextState, b: u64) -> &'static str {
        crate::rules_v2::boundary(c, b)
    }
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

mod loader_files {
    include!(concat!(env!("OUT_DIR"), "/pins.rs"));
}
mod blacktop_ball {
    include!(concat!(env!("OUT_DIR"), "/ball_policy.rs"));
}
#[path = "../../src/ball_gameplay_runtime.rs"]
mod ball_gameplay_runtime;
#[path = "../../src/ball_gameplay_v2.rs"]
mod ball_gameplay_v2;
#[path = "../../src/ball_v2.rs"]
mod ball_v2;
#[path = "../../src/rules_v2.rs"]
mod rules_v2;
