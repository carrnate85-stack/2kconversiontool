//! Actual Version 2 adapter Win32 type-check plus sparse-memory transaction
//! tests. Process scanning and observer creation are unconditional errors.
#![allow(dead_code)]
mod native_session {
    pub(crate) fn same_path(actual: &str, expected: &std::path::Path) -> bool {
        actual
            .replace('/', "\\")
            .eq_ignore_ascii_case(&expected.to_string_lossy().replace('/', "\\"))
    }
}
mod loader_files {
    pub(crate) struct Directories;
    impl Directories {
        pub fn pin(_: &std::path::Path) -> anyhow::Result<Self> {
            anyhow::bail!("Directory capabilities are not created by this offline harness")
        }
    }
}
mod backend {
    use serde::Serialize;
    use std::{
        io::Write,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(1);
    pub fn unique_id() -> String {
        format!("{:048x}", NEXT.fetch_add(1, Ordering::Relaxed))
    }
    // Test journal only. Production adapter calls the real sync_all +
    // MoveFileExW(REPLACE_EXISTING|WRITE_THROUGH) implementation unchanged.
    pub fn atomic_json(path: &std::path::Path, value: &impl Serialize) -> anyhow::Result<()> {
        let mut f = std::fs::File::create(path)?;
        f.write_all(&serde_json::to_vec(value)?)?;
        f.sync_all()?;
        Ok(())
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
        anyhow::bail!("Process enumeration is forbidden in this offline harness")
    }
    pub fn identity(_: u32) -> anyhow::Result<Identity> {
        anyhow::bail!("Process identity queries are forbidden in this offline harness")
    }
}
mod blacktop_native {
    use anyhow::{Result, ensure};
    pub(crate) trait Memory {
        fn base(&self) -> u64;
        fn read(&self, at: u64, len: usize) -> Result<Vec<u8>>;
        fn checkpoint(&self) -> Result<()> {
            Ok(())
        }
    }
    pub(crate) struct Reader;
    impl Reader {
        pub(crate) fn open_observer(
            _: &crate::session_safety::Identity,
            _: &std::path::Path,
        ) -> Result<Self> {
            anyhow::bail!("No game process can be opened by this harness")
        }
    }
    impl Memory for Reader {
        fn base(&self) -> u64 {
            0x140000000
        }
        fn read(&self, _: u64, _: usize) -> Result<Vec<u8>> {
            anyhow::bail!("No process memory capability")
        }
    }
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct ContextState {
        pub current_index: u32,
        pub alternate_index: u32,
        pub current: u32,
        pub alternate: u32,
        pub stage: u32,
        pub active: u8,
        pub owner: u64,
        pub kind: u8,
        pub game_mode: u32,
        pub root_tag: u64,
    }
    pub fn guard(
        rows: &[crate::session_safety::Process],
        expected: &crate::session_safety::Identity,
    ) -> Result<()> {
        ensure!(
            !rows.iter().any(crate::session_safety::launcher_process),
            "Another launcher is active"
        );
        ensure!(
            !rows.iter().any(|p| {
                let n = p.name.to_ascii_lowercase();
                n.starts_with("easyanticheat")
                    || n == "eaclaunch.exe"
                    || n == "start_protected_game.exe"
            }),
            "Protected process present"
        );
        let games: Vec<_> = rows
            .iter()
            .filter(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe"))
            .collect();
        ensure!(
            games.len() == 1 && games[0].identity.as_ref() == Some(expected),
            "No unique unchanged game"
        );
        Ok(())
    }
    pub fn validate_build(_: &impl Memory) -> Result<()> {
        anyhow::bail!("Legacy profile is deliberately unavailable")
    }
    pub fn context(_: &impl Memory) -> Result<ContextState> {
        anyhow::bail!("Legacy context is deliberately unavailable")
    }
    pub fn boundary(_: &ContextState, _: u64) -> &'static str {
        "legacy-disabled-in-harness"
    }
    pub fn rules_fixture() -> Result<crate::game_rules::Snapshot> {
        crate::rules_v2::capture(&crate::rules_v2::tests::fixture())
    }
}
#[path = "../src/game_rules.rs"]
mod game_rules;
#[path = "../src/rules_v2.rs"]
mod rules_v2;
#[path = "../src/rules_v2_access.rs"]
mod rules_v2_access;
