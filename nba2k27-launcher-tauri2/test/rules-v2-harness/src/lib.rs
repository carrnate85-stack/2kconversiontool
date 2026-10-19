//! Offline planner harness without any process API dependency.
mod blacktop_native {
    use crate::session_safety::{self, Identity};
    use anyhow::{Context, Result, ensure};
    use std::{mem::size_of, path::Path};
    use windows::Win32::{
        Foundation::*,
        System::{Diagnostics::Debug::*, Memory::*, Threading::*},
    };
    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
    struct Reader {
        handle: Handle,
        identity: Identity,
        base: u64,
    }
    impl Reader {
        fn open_observer(_: &Identity, _: &Path) -> Result<Self> {
            anyhow::bail!("No process API is enabled in this offline harness")
        }
    }
    impl Memory for Reader {
        fn base(&self) -> u64 {
            self.base
        }
        fn read(&self, _: u64, _: usize) -> Result<Vec<u8>> {
            anyhow::bail!("No process API is enabled in this offline harness")
        }
    }
    impl crate::blacktop_arena::Inspect for Reader {
        fn region(&self, _: u64) -> Result<crate::blacktop_arena::Region> {
            anyhow::bail!("No process API is enabled in this offline harness")
        }
    }
    fn handle_identity(_: HANDLE, _: u32) -> Result<Identity> {
        anyhow::bail!("No process API is enabled in this offline harness")
    }
    fn arena_guard(_: &[session_safety::Process], _: &Identity, _: &Identity) -> Result<()> {
        anyhow::bail!("No process API is enabled in this offline harness")
    }
    #[path = "../../../../src/arena_ball_v2_access.rs"]
    pub(crate) mod arena_ball_v2_access;
    pub(crate) trait Memory {
        fn base(&self) -> u64;
        fn read(&self, at: u64, len: usize) -> anyhow::Result<Vec<u8>>;
        fn checkpoint(&self) -> anyhow::Result<()> {
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
    impl ContextState {
        pub fn mode_identity(&self) -> (u32, u32, u32, u8, u8) {
            (
                self.current,
                self.alternate,
                self.stage,
                self.active,
                self.kind,
            )
        }
    }
    pub(crate) fn validate_build(_: &impl Memory) -> anyhow::Result<()> {
        anyhow::bail!("Legacy live profile deliberately excluded from this offline harness")
    }
    pub(crate) fn context(_: &impl Memory) -> anyhow::Result<ContextState> {
        anyhow::bail!("Legacy live context deliberately excluded from this offline harness")
    }
    pub(crate) fn boundary(_: &ContextState, _: u64) -> &'static str {
        "unavailable-legacy-harness"
    }
    #[cfg(test)]
    pub(crate) fn selection_memory_fixture() -> std::collections::HashMap<u64, Vec<u8>> {
        std::collections::HashMap::new()
    }
}
mod game_rules {
    pub struct Field {
        pub id: &'static str,
        pub offset: u64,
        pub width: usize,
        max: u32,
    }
    impl Field {
        pub fn valid(&self, value: u32) -> bool {
            value <= self.max
        }
    }
    pub static FIELDS: &[Field] = &[
        Field {
            id: "ShotClock",
            offset: 2070,
            width: 1,
            max: 1,
        },
        Field {
            id: "OutOfBounds",
            offset: 2084,
            width: 1,
            max: 1,
        },
        Field {
            id: "BackcourtViolation",
            offset: 2085,
            width: 1,
            max: 1,
        },
        Field {
            id: "EightSecondsHalfCourt",
            offset: 2086,
            width: 1,
            max: 1,
        },
        Field {
            id: "FiveSecondsInbounding",
            offset: 2677,
            width: 1,
            max: 1,
        },
        Field {
            id: "OffensiveViolationInKey",
            offset: 2312,
            width: 4,
            max: 2,
        },
        Field {
            id: "DefensiveViolationInKey",
            offset: 2316,
            width: 4,
            max: 2,
        },
        Field {
            id: "FiveSecondsBackToBasket",
            offset: 2087,
            width: 1,
            max: 1,
        },
        Field {
            id: "CloselyGuarded",
            offset: 2638,
            width: 1,
            max: 1,
        },
        Field {
            id: "OverTheBackRule",
            offset: 2096,
            width: 1,
            max: 1,
        },
        Field {
            id: "GenericPersonalFoulRule",
            offset: 2097,
            width: 1,
            max: 1,
        },
        Field {
            id: "ChargingRule",
            offset: 2098,
            width: 1,
            max: 1,
        },
        Field {
            id: "OffensiveOffballFoulRule",
            offset: 2099,
            width: 1,
            max: 1,
        },
        Field {
            id: "DefensiveOffballFoulRule",
            offset: 2100,
            width: 1,
            max: 1,
        },
        Field {
            id: "BlockingRule",
            offset: 2101,
            width: 1,
            max: 1,
        },
        Field {
            id: "ReachingFoulRule",
            offset: 2102,
            width: 1,
            max: 1,
        },
        Field {
            id: "LooseBallRule",
            offset: 2104,
            width: 1,
            max: 1,
        },
        Field {
            id: "ShootingFoulRule",
            offset: 2103,
            width: 1,
            max: 1,
        },
        Field {
            id: "IntentionalFoul",
            offset: 2105,
            width: 1,
            max: 1,
        },
        Field {
            id: "ClearPathRule",
            offset: 2112,
            width: 1,
            max: 1,
        },
        Field {
            id: "TransitionTakeFoulRule",
            offset: 2113,
            width: 1,
            max: 1,
        },
        Field {
            id: "KickBall",
            offset: 2106,
            width: 1,
            max: 1,
        },
        Field {
            id: "Goaltending",
            offset: 2088,
            width: 1,
            max: 1,
        },
        Field {
            id: "IllegalPick",
            offset: 2091,
            width: 1,
            max: 1,
        },
        Field {
            id: "Traveling",
            offset: 2089,
            width: 1,
            max: 1,
        },
        Field {
            id: "DoubleDribble",
            offset: 2090,
            width: 1,
            max: 1,
        },
        Field {
            id: "FoulOut",
            offset: 2092,
            width: 4,
            max: 6,
        },
        Field {
            id: "ThreePointShotRestriction",
            offset: 2712,
            width: 4,
            max: 2,
        },
        Field {
            id: "ZoneDefenseAllowed",
            offset: 2260,
            width: 1,
            max: 1,
        },
        Field {
            id: "NoChargeZone",
            offset: 2640,
            width: 1,
            max: 1,
        },
        Field {
            id: "BuzzerBeaterReview",
            offset: 2121,
            width: 1,
            max: 1,
        },
        Field {
            id: "ClearBallRequired",
            offset: 2641,
            width: 1,
            max: 1,
        },
        Field {
            id: "Timeouts",
            offset: 2653,
            width: 1,
            max: 1,
        },
        Field {
            id: "ScreenOutOfBoundsViolation",
            offset: 2116,
            width: 1,
            max: 1,
        },
        Field {
            id: "PlayerOutOfBoundsViolationRule",
            offset: 2118,
            width: 1,
            max: 1,
        },
        Field {
            id: "BallEnteredBasketFromBelow",
            offset: 2676,
            width: 1,
            max: 1,
        },
        Field {
            id: "TechnicalFoulRule",
            offset: 2107,
            width: 1,
            max: 1,
        },
        Field {
            id: "FlagrantFoulRule",
            offset: 2108,
            width: 1,
            max: 1,
        },
        Field {
            id: "TechnicalFoulEjectionRule",
            offset: 2109,
            width: 1,
            max: 1,
        },
    ];
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Snapshot {
        pub context: crate::blacktop_native::ContextState,
        pub base: u64,
        pub values: Vec<u32>,
    }
}
#[path = "../../../src/arena_v2.rs"]
mod arena_v2;
mod session_safety {
    use serde::{Deserialize, Serialize};
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    pub(crate) struct Identity {
        pub pid: u32,
        pub started: String,
        pub path: String,
    }
    pub(crate) struct Process;
    pub(crate) fn processes() -> anyhow::Result<Vec<Process>> {
        anyhow::bail!("No process API enabled")
    }
    pub(crate) fn identity(_: u32) -> anyhow::Result<Identity> {
        anyhow::bail!("No process API enabled")
    }
    static GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    pub(crate) fn write_gate() -> anyhow::Result<std::sync::MutexGuard<'static, ()>> {
        GATE.lock()
            .map_err(|_| anyhow::anyhow!("Poisoned test gate"))
    }
}
mod loader_files {
    pub(crate) struct Directories;
    impl Directories {
        pub(crate) fn pin(_: &std::path::Path) -> anyhow::Result<Self> {
            Ok(Self)
        }
    }
}
mod arena_session {
    use crate::blacktop_arena::{Choice, Record, Snapshot};
    pub(crate) trait Access {
        fn capture(&self, c: &Choice, expected: &[u8; 20]) -> anyhow::Result<Snapshot>;
        fn verify(&self, r: &Record, expected: &[u8]) -> anyhow::Result<()>;
        fn template(&self, r: &Record, expected: &[u8], next: &[u8]) -> anyhow::Result<()>;
        fn mapping(&self, expected: &[u8; 20], next: &[u8; 20]) -> anyhow::Result<()>;
        fn assets(&self, c: &Choice) -> anyhow::Result<()>;
    }
}
mod ball_session {
    use crate::blacktop_ball::{Lease, Snapshot};
    #[derive(Debug)]
    pub(crate) struct Deferred(pub String);
    impl std::fmt::Display for Deferred {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }
    impl std::error::Error for Deferred {}
    pub(crate) trait Access {
        fn capture(&self) -> anyhow::Result<Snapshot>;
        fn resolve(&self, s: &Snapshot, id: &str) -> anyhow::Result<(u32, String)>;
        fn write(&self, lease: &Lease, replacement: &[u8], restore: bool) -> anyhow::Result<()>;
    }
}
#[path = "../../../src/arena_ball_v2_adapter.rs"]
mod arena_ball_v2_adapter;
#[path = "../../../src/ball_v2.rs"]
mod ball_v2;
#[path = "../../../src/blacktop_arena.rs"]
mod blacktop_arena;
#[path = "../../../src/blacktop_ball.rs"]
mod blacktop_ball;
#[path = "../../../src/presentation_v2.rs"]
mod presentation_v2;
#[path = "../../../src/roster_v2_profile.rs"]
mod roster_v2_profile;
#[path = "../../../src/rules_v2.rs"]
mod rules_v2;
