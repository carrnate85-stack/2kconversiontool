extern crate self as tauri;
pub use url::Url;

#[path = "../../../src/jukebox_api.rs"]
mod jukebox_api;
#[path = "../../../src/jukebox_artwork.rs"]
mod jukebox_artwork;
#[path = "../../../src/jukebox_audio.rs"]
mod jukebox_audio;
#[path = "../../../src/jukebox_catalog.rs"]
mod jukebox_catalog;
#[path = "../../../src/jukebox_decode_worker.rs"]
mod jukebox_decode_worker;
#[path = "../../../src/jukebox_decoder.rs"]
mod jukebox_decoder;
#[path = "../../../src/jukebox_files.rs"]
mod jukebox_files;
#[path = "../../../src/jukebox_http.rs"]
mod jukebox_http;
#[path = "../../../src/jukebox_launch.rs"]
mod jukebox_launch;
#[path = "../../../src/jukebox_local.rs"]
mod jukebox_local;
#[path = "../../../src/jukebox_package.rs"]
mod jukebox_package;
#[path = "../../../src/jukebox_service.rs"]
mod jukebox_service;
#[path = "../../../src/jukebox_smoke.rs"]
mod jukebox_smoke;
#[path = "../../../src/jukebox_v2_resources.rs"]
mod jukebox_v2_resources;
