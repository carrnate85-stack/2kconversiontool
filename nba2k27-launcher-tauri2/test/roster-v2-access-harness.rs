//! Isolated actual Win32-adapter compilation and fake-backend tests. The stub
//! observer refuses every process open, so no test can inspect a game.
mod native_session {
    pub(crate) fn same_path(actual: &str, expected: &std::path::Path) -> bool {
        actual
            .replace('/', "\\")
            .eq_ignore_ascii_case(&expected.to_string_lossy().replace('/', "\\"))
    }
}
mod session_safety {
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(crate) struct Identity {
        pub pid: u32,
        pub started: String,
        pub path: String,
    }
    pub(crate) struct Process {
        pub name: String,
        pub pid: u32,
        pub parent: u32,
        pub identity: Option<Identity>,
    }
    pub(crate) fn launcher_process(p: &Process) -> bool {
        p.name.eq_ignore_ascii_case("2k Launcher.exe")
    }
    pub(crate) fn processes() -> anyhow::Result<Vec<Process>> {
        anyhow::bail!("Process enumeration is forbidden in this offline harness")
    }
    pub(crate) fn identity(_: u32) -> anyhow::Result<Identity> {
        anyhow::bail!("Native identity queries are forbidden in this offline harness")
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
            anyhow::bail!("A game cannot be opened by this harness")
        }
    }
    impl Memory for Reader {
        fn base(&self) -> u64 {
            0x140000000
        }
        fn read(&self, _: u64, _: usize) -> Result<Vec<u8>> {
            anyhow::bail!("No process read capability")
        }
    }
    pub(crate) fn guard(
        rows: &[crate::session_safety::Process],
        expected: &crate::session_safety::Identity,
    ) -> Result<()> {
        ensure!(
            !rows.iter().any(crate::session_safety::launcher_process),
            "Another launcher is active"
        );
        ensure!(
            !rows
                .iter()
                .any(|p| p.name.to_ascii_lowercase().contains("easyanticheat")),
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
}
#[path = "../src/roster_v2_access.rs"]
mod roster_v2_access;
#[path = "../src/roster_v2_profile.rs"]
mod roster_v2_profile;
#[path = "../src/roster_v2_readonly.rs"]
mod roster_v2_readonly;
