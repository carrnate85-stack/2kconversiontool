//! Actual native adapter API typecheck; process enumeration/identity/observer
//! calls are forbidden stubs. Runtime tests use sparse snapshots only.
#![allow(dead_code)]
mod native_session {
    pub(crate) fn same_path(a: &str, b: &std::path::Path) -> bool {
        a.trim_start_matches(r"\\?\")
            .eq_ignore_ascii_case(b.to_string_lossy().trim_start_matches(r"\\?\"))
    }
}
mod provider_protocol {
    pub(crate) const PIPE_PREFIX: &str = r"\\.\pipe\NBA2K27ModProvider-";
}
mod backend {
    use serde::Serialize;
    use std::{
        io::Write,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(1);
    pub fn unique_id() -> String {
        format!(
            "{:016x}{:016x}{:016x}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            NEXT.fetch_add(1, Ordering::Relaxed)
        )
    }
    pub fn atomic_json(p: &std::path::Path, v: &impl Serialize) -> anyhow::Result<()> {
        let mut f = std::fs::File::create(p)?;
        f.write_all(&serde_json::to_vec(v)?)?;
        f.sync_all()?;
        Ok(())
    }
    pub fn atomic_json_new(p: &std::path::Path, v: &impl Serialize) -> anyhow::Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(p)?;
        f.write_all(&serde_json::to_vec(v)?)?;
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
        anyhow::bail!("Process enumeration forbidden in offline presentation harness")
    }
    pub fn identity(_: u32) -> anyhow::Result<Identity> {
        anyhow::bail!("Process identity forbidden in offline presentation harness")
    }
    static GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    pub fn write_gate() -> anyhow::Result<std::sync::MutexGuard<'static, ()>> {
        Ok(GATE.lock().unwrap())
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
            anyhow::bail!("No game observer permitted")
        }
    }
    impl Memory for Reader {
        fn base(&self) -> u64 {
            0x140000000
        }
        fn read(&self, _: u64, _: usize) -> Result<Vec<u8>> {
            anyhow::bail!("No game memory capability")
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
            "Other owner present"
        );
        ensure!(
            !rows.iter().any(|r| {
                let n = r.name.to_ascii_lowercase();
                n.starts_with("easyanticheat")
                    || n == "eaclaunch.exe"
                    || n == "start_protected_game.exe"
            }),
            "Protected process present"
        );
        let games: Vec<_> = rows
            .iter()
            .filter(|r| r.name.eq_ignore_ascii_case("NBA2K27.exe"))
            .collect();
        ensure!(
            games.len() == 1 && games[0].identity.as_ref() == Some(expected),
            "Game identity changed"
        );
        Ok(())
    }
    pub fn validate_build(_: &impl Memory) -> Result<()> {
        anyhow::bail!("Legacy gate unavailable")
    }
    pub fn context(_: &impl Memory) -> Result<ContextState> {
        anyhow::bail!("Legacy context unavailable")
    }
    pub fn boundary(_: &ContextState, _: u64) -> &'static str {
        "legacy-unavailable"
    }
    pub fn rules_fixture() -> Result<crate::game_rules::Snapshot> {
        crate::rules_v2::capture(&crate::rules_v2::tests::fixture())
    }
}
#[path = "../src/file_identity.rs"]
mod file_identity;
#[path = "../src/game_rules.rs"]
mod game_rules;
#[path = "../src/loader_files.rs"]
mod loader_files;
#[path = "../src/presentation_v2.rs"]
mod presentation_v2;
#[path = "../src/presentation_v2_runtime.rs"]
mod presentation_v2_runtime;
#[path = "../src/rules_v2.rs"]
mod rules_v2;
