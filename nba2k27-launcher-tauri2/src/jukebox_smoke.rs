//! Isolated UI-smoke host for the real Jukebox API. No game path, process,
//! external network, native dialog, decoder child or game-session capability.
use crate::{
    jukebox_api::{Api, Host},
    jukebox_audio, jukebox_catalog,
};
use anyhow::{Result, ensure};
use serde_json::Value;
use std::{
    fs,
    io::Write,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU16, Ordering},
    },
    time::Duration,
};

struct FixtureHost {
    audio: PathBuf,
    package: PathBuf,
    artwork: PathBuf,
    port: Arc<AtomicU16>,
}
impl Host for FixtureHost {
    fn choose_artwork(&self, cancel: &AtomicBool) -> Result<Option<PathBuf>> {
        ensure!(!cancel.load(Ordering::Acquire), "Smoke artwork cancelled");
        Ok(Some(self.artwork.clone()))
    }
    fn artwork_lookup_enabled(&self) -> bool {
        false
    }
    fn lookup_artwork(
        &self,
        _: &str,
        _: &str,
        _: &AtomicBool,
    ) -> Result<Vec<crate::jukebox_artwork::Candidate>> {
        anyhow::bail!("External artwork lookup is forbidden in smoke")
    }
    fn preview_enabled(&self) -> bool {
        self.port.load(Ordering::Acquire) != 0
    }
    fn preview_url(&self, token: &str) -> Option<String> {
        let port = self.port.load(Ordering::Acquire);
        (port != 0).then(|| format!("http://127.0.0.1:{port}/api/jukebox/preview/{token}"))
    }
    fn open_library_folder(&self, _: &std::path::Path) -> Result<()> {
        anyhow::bail!("Opening an external folder is forbidden in smoke")
    }
    fn choose_audio(&self, cancel: &AtomicBool) -> Result<Option<Vec<PathBuf>>> {
        ensure!(!cancel.load(Ordering::Acquire), "Smoke import cancelled");
        Ok(Some(vec![self.audio.clone()]))
    }
    fn choose_playlist(&self, cancel: &AtomicBool) -> Result<Option<PathBuf>> {
        ensure!(!cancel.load(Ordering::Acquire), "Smoke import cancelled");
        Ok(Some(self.package.clone()))
    }
    fn choose_export(&self, _: &str, cancel: &AtomicBool) -> Result<Option<PathBuf>> {
        ensure!(!cancel.load(Ordering::Acquire), "Smoke export cancelled");
        Ok(Some(self.package.clone()))
    }
    fn decode(
        &self,
        _: &[u8],
        _: &str,
        _: Arc<AtomicBool>,
        _: &mut dyn FnMut(u32) -> Result<()>,
    ) -> Result<Vec<u8>> {
        anyhow::bail!("Smoke uses a complete PCM WAV; no decoder process is permitted")
    }
}
pub(crate) struct Runtime {
    root: PathBuf,
    api: Mutex<Option<Arc<Api>>>,
    server: Mutex<Option<crate::jukebox_http::Server>>,
}
impl Runtime {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            api: Mutex::new(None),
            server: Mutex::new(None),
        }
    }
    fn api(&self) -> Result<Arc<Api>> {
        let mut slot = self
            .api
            .lock()
            .map_err(|_| anyhow::anyhow!("Smoke Jukebox unavailable"))?;
        if let Some(api) = slot.as_ref() {
            return Ok(api.clone());
        }
        let input = self.root.join("Cache/JukeboxSmokeInputs");
        fs::create_dir_all(&input)?;
        let path = input.join("Smoke native.wav");
        let wav = jukebox_audio::pcm_wave(&vec![0; 1920])?;
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                file.write_all(&wav)?;
                file.sync_all()?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                ensure!(
                    fs::read(&path)? == wav,
                    "Smoke audio fixture changed; left untouched"
                );
            }
            Err(error) => return Err(error.into()),
        }
        let music = jukebox_catalog::Music {
            tracks: [1, 100]
                .into_iter()
                .map(|bank| jukebox_catalog::Track {
                    name: format!("Smoke stock {bank}"),
                    artist: "Inert fixture".into(),
                    cover: String::new(),
                    collection: 1,
                    bank,
                    unique: bank,
                    extra: Default::default(),
                })
                .collect(),
            extra: Default::default(),
        };
        // One own 1x1 BMP; the actual WIC/native codec must normalize it to
        // the exact 256x256 BC1 TLD + matching decoded-pixel PNG.
        let art = input.join("Smoke cover.bmp");
        let mut bmp = vec![0u8; 58];
        bmp[..2].copy_from_slice(b"BM");
        bmp[2..6].copy_from_slice(&58u32.to_le_bytes());
        bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
        bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
        bmp[18..22].copy_from_slice(&1i32.to_le_bytes());
        bmp[22..26].copy_from_slice(&1i32.to_le_bytes());
        bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
        bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
        bmp[34..38].copy_from_slice(&4u32.to_le_bytes());
        bmp[54..57].copy_from_slice(&[45, 110, 180]);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&art)
        {
            Ok(mut f) => {
                f.write_all(&bmp)?;
                f.sync_all()?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                ensure!(fs::read(&art)? == bmp, "Smoke artwork fixture changed")
            }
            Err(e) => return Err(e.into()),
        }
        let port = Arc::new(AtomicU16::new(0));
        let api = Arc::new(Api::open(
            &self.root,
            music,
            None,
            Arc::new(FixtureHost {
                audio: path,
                package: input.join("Smoke native.2kplaylist"),
                artwork: art,
                port: port.clone(),
            }),
        )?);
        let server = crate::jukebox_http::Server::start(api.clone())?;
        port.store(server.port, Ordering::Release);
        api.set_preview_available()?;
        *self
            .server
            .lock()
            .map_err(|_| anyhow::anyhow!("Smoke image server unavailable"))? = Some(server);
        api.add_warning("Isolated smoke fixture: local library edits only; game playback, stock reads, native dialogs and decoder processes are disabled.")?;
        *slot = Some(api.clone());
        Ok(api)
    }
    pub fn dispatch(&self, channel: &str, args: Value) -> Result<Value> {
        self.api()?.dispatch(channel, args)
    }
    pub fn cancel(&self) {
        if let Ok(slot) = self.server.lock() {
            if let Some(server) = slot.as_ref() {
                server.stop();
            }
        }
        if let Ok(slot) = self.api.lock() {
            if let Some(api) = slot.as_ref() {
                api.cancel_pending();
            }
        }
    }
    pub fn shutdown(&self) {
        if let Ok(slot) = self.server.lock() {
            if let Some(server) = slot.as_ref() {
                server.shutdown();
            }
        }
        if let Ok(slot) = self.api.lock() {
            if let Some(api) = slot.as_ref() {
                let _ = api.shutdown(Duration::from_secs(1));
            }
        }
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        self.cancel();
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    include!("jukebox_test_paths.rs");
    use super::*;
    use serde_json::json;
    #[test]
    fn actual_smoke_service_persists_inert_stock_playlist_and_pcm_import_without_game_reads() {
        let root = fixture_artifacts().join(format!("jukebox-smoke-test-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        {
            let runtime = Runtime::new(root.clone());
            let first = runtime.dispatch("jukebox:get", Value::Null).unwrap();
            assert_eq!(first["stockKeys"], json!(["1:1:1", "1:100:100"]));
            assert_eq!(first["capabilities"]["nativePlayback"], false);
            assert_eq!(first["capabilities"]["stockPreview"], false);
            let created = runtime
                .dispatch(
                    "jukebox:create-playlist",
                    json!({"expectedRevision":first["revision"],"name":"Smoke"}),
                )
                .unwrap();
            let id = created["view"]["state"]["playlists"][0]["id"].clone();
            runtime
                .dispatch(
                    "jukebox:import-audio",
                    json!({"expectedRevision":created["view"]["revision"],"playlistId":id}),
                )
                .unwrap();
            let start = std::time::Instant::now();
            loop {
                let view = runtime.dispatch("jukebox:get", Value::Null).unwrap();
                if view["job"]["phase"] == "completed" {
                    assert_eq!(
                        view["state"]["playlists"][0]["trackKeys"]
                            .as_array()
                            .unwrap()
                            .len(),
                        1
                    );
                    assert_eq!(view["state"]["audio"].as_object().unwrap().len(), 1);
                    assert_eq!(view["playback"]["phase"], "default");
                    break;
                }
                assert!(start.elapsed() < Duration::from_secs(2), "{view}");
                std::thread::sleep(Duration::from_millis(1));
            }
            runtime.shutdown();
        }
        fs::remove_dir_all(root).unwrap();
    }
}
