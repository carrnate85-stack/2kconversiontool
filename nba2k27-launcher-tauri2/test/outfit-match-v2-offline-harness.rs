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
    #[derive(Clone, serde::Serialize, serde::Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    pub struct Outfit {
        pub name: String,
        pub top: String,
        pub bottom: String,
        pub shoes: String,
        pub socks: String,
        pub pool: String,
        pub accessories: Vec<String>,
        pub street_id: String,
        pub layer: String,
    }
    use anyhow::{Result, ensure};
    pub fn validate_outfit(o: &Outfit) -> Result<()> {
        ensure!(
            !o.name.trim().is_empty()
                && o.name.encode_utf16().count() <= 80
                && !o.name.chars().any(|c| c <= '\u{1f}' || c == '\u{7f}'),
            "Outfit name required (maximum 80 characters)"
        );
        ensure!(
            matches!(o.pool.as_str(), "male" | "female"),
            "Invalid clothing fit"
        );
        ensure!(o.accessories.len() <= 3, "Maximum 3 accessories");
        ensure!(
            [
                &o.top,
                &o.bottom,
                &o.shoes,
                &o.socks,
                &o.street_id,
                &o.layer
            ]
            .into_iter()
            .chain(o.accessories.iter())
            .all(|s| s.len() <= 240),
            "Asset ID exceeds limit"
        );
        let mut seen = std::collections::HashSet::new();
        ensure!(
            o.accessories.iter().all(|a| seen.insert(a)),
            "Duplicate accessories"
        );
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
    use crate::session_safety::{self, Identity, Process};
    use anyhow::{Context, Result, ensure};
    use std::{mem::size_of, path::Path};
    use windows::{
        Win32::{
            Foundation::*,
            System::{
                Diagnostics::Debug::{ReadProcessMemory, WriteProcessMemory},
                Memory::*,
                Threading::*,
            },
        },
        core::PWSTR,
    };
    pub(crate) trait Memory {
        fn base(&self) -> u64;
        fn read(&self, at: u64, len: usize) -> Result<Vec<u8>>;
        fn checkpoint(&self) -> Result<()> {
            Ok(())
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
    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
    pub struct Reader {
        handle: Handle,
        pub identity: Identity,
        base: u64,
    }
    impl Reader {
        pub fn open_observer(_: &Identity, _: &Path) -> Result<Self> {
            anyhow::bail!("No process discovery/game observer permitted in offline outfit harness")
        }
        fn check(&self) -> Result<()> {
            anyhow::bail!("No real game reads")
        }
    }
    impl Memory for Reader {
        fn base(&self) -> u64 {
            self.base
        }
        fn read(&self, _: u64, _: usize) -> Result<Vec<u8>> {
            anyhow::bail!("No real game memory")
        }
    }
    impl crate::blacktop_arena::Inspect for Reader {
        fn region(&self, _: u64) -> Result<crate::blacktop_arena::Region> {
            anyhow::bail!("No real game region")
        }
    }
    fn address(at: u64, len: usize) -> Result<()> {
        ensure!(
            at >= 65536
                && len > 0
                && len <= 4096
                && at
                    .checked_add(len as u64)
                    .is_some_and(|v| v <= 0x7fffffffffff),
            "bounds"
        );
        Ok(())
    }
    fn handle_identity(_: HANDLE, _: u32) -> Result<Identity> {
        anyhow::bail!("No real process identity in offline harness")
    }
    pub fn guard(rows: &[Process], game: &Identity) -> Result<()> {
        ensure!(
            !rows.iter().any(session_safety::launcher_process),
            "Other launcher"
        );
        ensure!(
            !rows
                .iter()
                .any(|p| p.name.to_ascii_lowercase().contains("easyanticheat")),
            "Protected"
        );
        let games: Vec<_> = rows
            .iter()
            .filter(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe"))
            .collect();
        ensure!(
            games.len() == 1 && games[0].identity.as_ref() == Some(game),
            "Game identity"
        );
        Ok(())
    }
    fn arena_guard(rows: &[Process], game: &Identity, owner: &Identity) -> Result<()> {
        let others = rows
            .iter()
            .filter(|p| {
                !(session_safety::launcher_process(p) && p.identity.as_ref() == Some(owner))
            })
            .map(|p| Process {
                name: p.name.clone(),
                pid: p.pid,
                parent: p.parent,
                identity: p.identity.clone(),
            })
            .collect::<Vec<_>>();
        guard(&others, game)
    }
    pub fn validate_build(_: &impl Memory) -> Result<()> {
        anyhow::bail!("Legacy profile forbidden")
    }
    pub fn context(_: &impl Memory) -> Result<ContextState> {
        anyhow::bail!("Legacy context forbidden")
    }
    pub fn boundary(_: &ContextState, _: u64) -> &'static str {
        "legacy-forbidden"
    }
    pub fn rules_fixture() -> Result<crate::game_rules::Snapshot> {
        anyhow::bail!("No legacy rules fixture")
    }
    #[path = "C:/Users/carrn/CodexProjects/2kconversiontool/nba2k27-launcher-tauri2/src/outfit_selection_v2_writer.rs"]
    pub mod outfit_selection_v2_writer;
    pub mod outfit_writer {
        use super::*;
        use crate::{
            blacktop_outfits::{self as o, Composed, Page, Player},
            outfit_journal::{Prepared, Ready, Reservation, Submission},
        };
        pub struct Writer;
        pub struct Receipt;
        impl Writer {
            pub fn reader(&self) -> &Reader {
                panic!("No legacy writer")
            }
            pub fn check_owner(&self) -> Result<()> {
                anyhow::bail!("Legacy forbidden")
            }
            pub fn allow_writes(&mut self, _: &Path) -> Result<()> {
                anyhow::bail!("Legacy forbidden")
            }
            pub fn prepare(
                &self,
                _: Reservation,
                _: &Player,
                _: &Composed,
                _: &[Page],
            ) -> Result<Prepared> {
                anyhow::bail!("Legacy forbidden")
            }
            pub fn unchanged(
                &self,
                _: Ready,
                _: &Player,
                _: &Composed,
                _: &[Page],
            ) -> Result<Receipt> {
                anyhow::bail!("Legacy forbidden")
            }
            pub fn publish(&self, _: Submission, _: &Player, _: &[Page]) -> Result<Receipt> {
                anyhow::bail!("Legacy forbidden")
            }
            pub fn consumed(&self, _: &Receipt, _: &[Page]) -> Result<bool> {
                anyhow::bail!("Legacy forbidden")
            }
        }
    }
}
#[path = "../src/blacktop_arena.rs"]
mod blacktop_arena;
#[path = "../src/blacktop_outfit_match.rs"]
mod blacktop_outfit_match;
#[path = "../src/blacktop_outfits.rs"]
mod blacktop_outfits;
#[path = "../src/game_rules.rs"]
mod game_rules;
#[path = "../src/loader_files.rs"]
mod loader_files;
#[path = "../src/outfit_journal.rs"]
mod outfit_journal;
#[path = "../src/outfit_match_v2.rs"]
mod outfit_match_v2;
#[path = "../src/outfit_selection_session.rs"]
mod outfit_selection_session;
#[path = "../src/outfit_selection_v2.rs"]
mod outfit_selection_v2;
#[path = "../src/outfit_selection_v2_session.rs"]
mod outfit_selection_v2_session;
#[path = "../src/outfits.rs"]
mod outfits;
#[path = "../src/rules_v2.rs"]
mod rules_v2;
#[path = "../src/outfit_match_v2_tests.rs"]
mod suite;
fn main() -> anyhow::Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() == 3 && args[1] == "--disk-profile" {
        suite::disk_profile(std::path::Path::new(&args[2]))
    } else {
        suite::run()?;
        Ok(())
    }
}
