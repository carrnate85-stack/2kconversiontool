extern crate self as tauri;
pub use url::Url;
#[path = "../../../src/jukebox_cover_route.rs"]
mod jukebox_cover_route;
#[path = "../../../src/jukebox_prepared.rs"]
mod jukebox_prepared;
#[path = "../../../src/file_identity.rs"]
mod file_identity;
// This importer harness cannot prepare game loader files. The unrelated cover
// publication route fails closed if accidentally reached by an offline fixture.
mod loader_files {pub struct Directories;impl Directories {pub fn pin(_:&std::path::Path)->anyhow::Result<Self>{anyhow::bail!("Game-loader route unavailable in importer fixture")}}}
mod backend {pub fn unique_id()->String{format!("{:032x}{:016x}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),std::process::id())}}

/// Narrow production-API fixture, callable without compiling unrelated legacy test modules.
pub fn curated_import_fixture()->anyhow::Result<()> {
    use std::{path::PathBuf,sync::{Arc,atomic::AtomicBool},time::{Instant,Duration}};
    use jukebox_api::{Api,Host};use serde_json::json;
    struct NoPicker;
    impl Host for NoPicker {
        fn choose_audio(&self,_:&AtomicBool)->anyhow::Result<Option<Vec<PathBuf>>>{anyhow::bail!("No fixture picker")}
        fn choose_playlist(&self,_:&AtomicBool)->anyhow::Result<Option<PathBuf>>{anyhow::bail!("Downloaded import must bypass picker")}
        fn choose_export(&self,_:&str,_:&AtomicBool)->anyhow::Result<Option<PathBuf>>{anyhow::bail!("No fixture picker")}
    }
    let root=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../artifacts/qa131-download-library").canonicalize()?.join(format!("playlist-api-{}",backend::unique_id()));std::fs::create_dir(&root)?;
    let stock=jukebox_catalog::Music{tracks:vec![jukebox_catalog::Track{name:"Stock".into(),artist:"Artist".into(),cover:"001_helicopter".into(),collection:1,bank:1,unique:1,extra:serde_json::Map::new()}],extra:serde_json::Map::new()};
    let api=Api::open(&root,stock,None,Arc::new(NoPicker))?;
    let call=|channel:&str,mut args:serde_json::Value|->anyhow::Result<serde_json::Value>{args["expectedRevision"]=json!(api.view()?.revision);api.dispatch(channel,args)};
    call("jukebox:create-playlist",json!({"name":"Original"}))?;let id=api.view()?.state.playlists[0].id.clone();
    call("jukebox:set-order",json!({"playlistId":id,"trackKeys":["1:1:1"]}))?;call("jukebox:set-active",json!({"playlistId":id}))?;
    call("jukebox:refresh-bundle",json!({"playlistId":id}))?;
    let started=Instant::now();loop{let v=api.view()?;if v.job.as_ref().is_some_and(|j|j.phase=="completed"){break}anyhow::ensure!(started.elapsed()<Duration::from_secs(10),"Fixture bundle timed out");std::thread::sleep(Duration::from_millis(20));}
    std::thread::sleep(Duration::from_millis(30));
    let added=api.import_downloaded(&root.join("Library/Jukebox/Playlists/Original.2kplaylist"),&AtomicBool::new(false),|_|{})?;
    let v=api.view()?;anyhow::ensure!(v.state.active_playlist_id.as_deref()==Some(&id)&&v.state.playlists.len()==2&&added!=id,"Import changed active selection or failed to add playlist");
    std::thread::sleep(Duration::from_millis(30));let bad=root.join("invalid.2kplaylist");std::fs::write(&bad,b"not an archive")?;
    anyhow::ensure!(api.import_downloaded(&bad,&AtomicBool::new(false),|_|{}).is_err(),"Invalid payload was accepted");anyhow::ensure!(api.view()?.state.playlists.len()==2,"Invalid import changed library");
    std::fs::write(root.join("result.json"),serde_json::to_vec_pretty(&json!({"passed":true,"added":added,"selectionPreserved":id,"invalidRejected":true,"pickerUsed":false}))?)?;
    if std::env::args().any(|a|a=="--hosted-playlist") {
        std::thread::sleep(Duration::from_millis(50));
        let hosted=root.parent().unwrap().join("hosted-playlist.2kplaylist");
        let imported=api.import_downloaded(&hosted,&AtomicBool::new(false),|s|println!("{s}"))?;
        let v=api.view()?;anyhow::ensure!(v.state.active_playlist_id.as_deref()==Some(&id),"Hosted playlist changed active selection");
        std::fs::write(root.join("hosted-result.json"),serde_json::to_vec_pretty(&json!({"passed":true,"playlistId":imported,"playlists":v.state.playlists,"selectionPreserved":id}))?)?;
    }
    println!("Passed production playlist API download fixture: {}",root.display());Ok(())
}

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
