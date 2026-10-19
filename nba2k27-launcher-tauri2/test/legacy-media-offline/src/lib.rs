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
#[path = "../../../src/jukebox_local.rs"]
mod jukebox_local;
#[path = "../../../src/jukebox_package.rs"]
mod jukebox_package;
#[path = "../../../src/jukebox_service.rs"]
mod jukebox_service;
#[path = "../../../src/jukebox_v2_resources.rs"]
mod jukebox_v2_resources;
#[path = "../../../src/legacy_saved_media.rs"]
mod legacy_saved_media;
#[path = "../../../src/video_library.rs"]
mod video_library;
#[path = "../../../src/startup_mp4.rs"]
mod startup_mp4;

/// Explicit disk-only stock verification + retained read-only source audit.
/// A destination, if supplied, must be a new disjoint workspace stage.
pub fn audit(
    source: &std::path::Path,
    profile: &str,
    game: &std::path::Path,
    destination: Option<&std::path::Path>,
) -> anyhow::Result<serde_json::Value> {
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let stock = jukebox_v2_resources::read_stock(game, || Ok(()))?;
    let mut plan = legacy_saved_media::inspect(source, profile, stock.music, &cancel)?;
    let report = serde_json::to_value(plan.report())?;
    if let Some(destination) = destination {
        let staged = plan.stage_new(destination, &cancel, |done, total| {
            eprintln!("Staged {done}/{total} media groups");
            Ok(())
        })?;
        Ok(
            serde_json::json!({"report":report,"staged":true,"playlists":staged.playlist_ids.len(),"tracks":staged.track_keys.len(),"videos":staged.videos.items.len(),"activePlaylistId":staged.active_playlist_id,"startupVideoEnabled":staged.startup_video_enabled}),
        )
    } else {
        Ok(serde_json::json!({"report":report,"staged":false}))
    }
}
