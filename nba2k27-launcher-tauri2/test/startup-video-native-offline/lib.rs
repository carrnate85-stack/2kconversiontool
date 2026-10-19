//! No native APIs are called by tests. Real Win32 adapter bodies are typechecked
//! and linked; injected fake OS samples exercise the actual orchestration.
#![allow(dead_code)]
#[path = "../../src/startup_audio_guardian.rs"]
mod startup_audio_guardian;
#[path = "../../src/startup_video_native.rs"]
mod startup_video_native;
#[path = "../../src/startup_video_policy.rs"]
mod startup_video_policy;
