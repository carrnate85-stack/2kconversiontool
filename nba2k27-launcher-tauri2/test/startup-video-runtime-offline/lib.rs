//! Native APIs are compiled, never called by fixture tests. No game/audio/window
//! or installed portable data access. Dedicated immutable MP4 fixture roots only.
#![allow(dead_code)]
#[path = "../../src/startup_audio_guardian.rs"]
mod startup_audio_guardian;
#[path = "../../src/startup_mp4.rs"]
mod startup_mp4;
#[path = "../../src/startup_video_audio.rs"]
mod startup_video_audio;
#[path = "../../src/startup_video_helper.rs"]
mod startup_video_helper;
#[path = "../../src/startup_video_media.rs"]
mod startup_video_media;
#[path = "../../src/startup_video_native.rs"]
mod startup_video_native;
#[path = "../../src/startup_video_policy.rs"]
mod startup_video_policy;
#[path = "../../src/startup_video_runtime.rs"]
mod startup_video_runtime;
#[cfg(feature = "tauri-adapter")]
#[path = "../../src/startup_video_tauri.rs"]
mod startup_video_tauri;
