#![windows_subsystem = "windows"]
#![allow(dead_code)]
// Only this fixed audio role is reachable. No Tauri, Steam, loader, memory
// adapters, launcher commands, arbitrary paths or arbitrary child entrypoints.
#[path = "../../src/startup_audio_guardian.rs"]
mod startup_audio_guardian;
#[path = "../../src/startup_video_audio.rs"]
mod startup_video_audio;
#[path = "../../src/startup_video_helper.rs"]
mod startup_video_helper;
#[path = "../../src/startup_video_native.rs"]
mod startup_video_native;
#[path = "../../src/startup_video_policy.rs"]
mod startup_video_policy;
fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let result = if args.len() == 3 && args[1] == startup_video_audio::FLAG {
        startup_video_audio::run(&args[2])
    } else {
        Err(anyhow::anyhow!(
            "Private audio role requires its exact bootstrap"
        ))
    };
    if result.is_err() {
        std::process::exit(1);
    }
}
