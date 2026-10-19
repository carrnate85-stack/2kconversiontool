#![allow(dead_code)]
mod blacktop_native {
    include!(concat!(env!("OUT_DIR"), "/native_types.rs"));
    pub fn boundary(c: &ContextState, b: u64) -> &'static str {
        crate::rules_v2::boundary(c, b)
    }
}
mod blacktop_arena {
    include!(concat!(env!("OUT_DIR"), "/inspect_types.rs"));
}
mod game_rules {
    include!(concat!(env!("OUT_DIR"), "/rule_types.rs"));
}
mod blacktop_ball {
    include!(concat!(env!("OUT_DIR"), "/ball_policy.rs"));
}
mod loader_files {
    include!(concat!(env!("OUT_DIR"), "/pins.rs"));
}
#[path = "../../src/ball_v2.rs"]
mod ball_v2;
#[path = "../../src/ball_v2_probe.rs"]
mod ball_v2_probe;
#[path = "../../src/rules_v2.rs"]
mod rules_v2;
pub fn run(portable: &std::path::Path, pid: u32) -> anyhow::Result<serde_json::Value> {
    ball_v2_probe::run(portable, pid)
}
