use super::*;
include!("jukebox_test_paths.rs");
use serde_json::Map;
use std::collections::BTreeSet;
use std::sync::atomic::AtomicUsize;
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let p = fixture_artifacts().join(format!(
            "2k-jukebox-edit-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(
            self.0
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("2k-jukebox-edit-")
        );
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn stock() -> catalog::Music {
    catalog::Music {
        tracks: vec![catalog::Track {
            name: "Stock song".into(),
            artist: "Stock artist".into(),
            cover: "001_helicopter".into(),
            collection: 1,
            bank: 1,
            unique: 0,
            extra: Map::new(),
        }],
        extra: Map::new(),
    }
}
fn bmp() -> Vec<u8> {
    let mut b = vec![0u8; 58];
    b[..2].copy_from_slice(b"BM");
    b[2..6].copy_from_slice(&58u32.to_le_bytes());
    b[10..14].copy_from_slice(&54u32.to_le_bytes());
    b[14..18].copy_from_slice(&40u32.to_le_bytes());
    b[18..22].copy_from_slice(&1u32.to_le_bytes());
    b[22..26].copy_from_slice(&1u32.to_le_bytes());
    b[26..28].copy_from_slice(&1u16.to_le_bytes());
    b[28..30].copy_from_slice(&24u16.to_le_bytes());
    b[34..38].copy_from_slice(&4u32.to_le_bytes());
    b[54..57].copy_from_slice(&[30, 140, 220]);
    b
}
struct ArtHost {
    path: PathBuf,
    searches: AtomicUsize,
}
impl Host for ArtHost {
    fn choose_audio(&self, _: &AtomicBool) -> Result<Option<Vec<PathBuf>>> {
        Ok(None)
    }
    fn choose_playlist(&self, _: &AtomicBool) -> Result<Option<PathBuf>> {
        Ok(None)
    }
    fn choose_export(&self, _: &str, _: &AtomicBool) -> Result<Option<PathBuf>> {
        Ok(None)
    }
    fn choose_artwork(&self, c: &AtomicBool) -> Result<Option<PathBuf>> {
        cancelled(c)?;
        Ok(Some(self.path.clone()))
    }
    fn preview_enabled(&self) -> bool {
        true
    }
    fn preview_url(&self, token: &str) -> Option<String> {
        Some(format!("http://127.0.0.1:12345/owned/{token}"))
    }
    fn lookup_artwork(
        &self,
        artist: &str,
        title: &str,
        c: &AtomicBool,
    ) -> Result<Vec<crate::jukebox_artwork::Candidate>> {
        self.searches.fetch_add(1, Ordering::SeqCst);
        (0..6)
            .map(|i| {
                Ok(crate::jukebox_artwork::Candidate {
                    label: format!("Fixture {i}"),
                    artwork: crate::jukebox_artwork::prepare(&bmp(), c)?,
                    matched_title: title.into(),
                    matched_artist: artist.into(),
                    source: "Offline fixture".into(),
                })
            })
            .collect()
    }
}
fn open(f: &Fixture) -> (Arc<Api>, Arc<ArtHost>) {
    let path = f.0.join("owned.bmp");
    fs::write(&path, bmp()).unwrap();
    let h = Arc::new(ArtHost {
        path,
        searches: AtomicUsize::new(0),
    });
    (
        Arc::new(Api::open(&f.0, stock(), None, h.clone()).unwrap()),
        h,
    )
}
fn call(a: &Api, c: &str, mut v: Value) -> Result<Value> {
    v["expectedRevision"] = json!(a.view()?.revision);
    a.dispatch(c, v)
}
fn choose(a: &Api, key: &str) -> String {
    call(a, "jukebox:choose-artwork", json!({"trackKey":key})).unwrap()["artwork"]["token"]
        .as_str()
        .unwrap()
        .into()
}
fn edit(a: &Api, key: &str, title: &str, artist: &str, token: &str) -> Result<Value> {
    call(
        a,
        "jukebox:edit-track",
        json!({"trackKey":key,"title":title,"artist":artist,"artworkToken":token}),
    )
}

#[test]
fn actual_stock_custom_artwork_and_metadata_roundtrip_without_audio_duplication() {
    let f = Fixture::new();
    let (a, h) = open(&f);
    call(
        &a,
        "jukebox:create-playlist",
        json!({"name":"Edited playlist","addDefaultSongs":true}),
    )
    .unwrap();
    let id = a.view().unwrap().state.playlists[0].id.clone();
    let custom = {
        let mut s = lock(&a.shared.service).unwrap();
        let k = s
            .import_audio(
                &id,
                "Custom song",
                "Custom artist",
                &crate::jukebox_audio::pcm_wave(&[0; 16]).unwrap(),
                &nonce().unwrap(),
            )
            .unwrap();
        a.shared.refresh(&s).unwrap();
        k
    };
    let before = a.view().unwrap().state;
    for (key, title) in [
        ("1:1:0", "Stock edited"),
        (custom.as_str(), "Custom edited"),
    ] {
        let token = choose(&a, key);
        assert!(edit(&a, key, title, "", &token).is_err());
        // Invalid metadata must not consume an otherwise valid prepared lease.
        assert!(
            lock(&a.shared.status)
                .unwrap()
                .prepared_art
                .contains_key(&token)
        );
        edit(&a, key, title, "Edited artist", &token).unwrap();
        assert!(
            a.consume_artwork(key, &a.view().unwrap().revision, Some(&token))
                .is_err()
        );
        a.release_artwork(&token).unwrap();
    }
    let after = a.view().unwrap().state;
    assert_eq!(
        after.playlists[0].track_keys,
        before.playlists[0].track_keys
    );
    assert_eq!(after.audio.len(), 1);
    assert!(!after.audio.contains_key("1:1:0"));
    assert_eq!(
        after.audio[&custom].native.sha256,
        before.audio[&custom].native.sha256
    );
    assert_eq!(
        after
            .music
            .tracks
            .iter()
            .map(|t| t.key())
            .collect::<Vec<_>>(),
        before
            .music
            .tracks
            .iter()
            .map(|t| t.key())
            .collect::<Vec<_>>()
    );
    let bytes = lock(&a.shared.service)
        .unwrap()
        .export_package(&id)
        .unwrap();
    let entries =
        crate::jukebox_package::unpack(&bytes, crate::jukebox_package::MAX_SEGMENT, 512).unwrap();
    assert_eq!(
        entries.keys().filter(|s| s.starts_with("audio/")).count(),
        1
    );
    let m: Value = serde_json::from_slice(&entries["playlist.json"]).unwrap();
    assert_eq!(m["schema"], "nba2k27-2kplaylist-v4");
    assert_eq!(m["tracks"][0]["kind"], "stock-override");
    assert!(m["tracks"][0]["audioFile"].is_null());
    let f2 = Fixture::new();
    let mut s = Service::open(&f2.0, stock(), &nonce().unwrap()).unwrap();
    let imported = s
        .import_package(&bytes, &nonce().unwrap(), |_, _| {
            anyhow::bail!("No decoder expected")
        })
        .unwrap();
    let keys = s
        .view()
        .playlists
        .iter()
        .find(|p| p.id == imported)
        .unwrap()
        .track_keys
        .clone();
    assert_eq!(keys.len(), 2);
    assert_eq!(keys[0], "1:1:0");
    assert_eq!(s.view().music.tracks[0].name, "Stock edited");
    for key in &keys {
        assert!(s.artwork_preview(key).unwrap().is_some());
        assert_eq!(s.artwork_native(key).unwrap().unwrap().len(), 32800);
    }
    assert_eq!(s.view().audio.len(), 1);
    let (_, legacy) =
        crate::jukebox_package::export_segment("Old", &stock(), &stock(), &BTreeMap::new(), None)
            .unwrap();
    s.import_package(&legacy, &nonce().unwrap(), |_, _| {
        anyhow::bail!("No decode")
    })
    .unwrap();
    assert_eq!(s.view().music.tracks[0].name, "Stock edited");
    assert_eq!(h.searches.load(Ordering::SeqCst), 0);
}

#[test]
fn independent_parallel_thumbnail_leases_and_bounded_first_lookup_result() {
    let f = Fixture::new();
    let (a, h) = open(&f);
    let token = choose(&a, "1:1:0");
    let rev = a.view().unwrap().revision;
    assert!(a.consume_artwork("1:9:0", &rev, Some(&token)).is_err());
    assert!(
        a.consume_artwork("1:1:0", &"f".repeat(64), Some(&token))
            .is_err()
    );
    edit(&a, "1:1:0", "Edited", "Artist", &token).unwrap();
    a.release_artwork(&token).unwrap();
    let workers = (0..20)
        .map(|_| {
            let a = a.clone();
            thread::spawn(move || {
                call(&a, "jukebox:get-artwork", json!({"trackKey":"1:1:0"})).unwrap()["artwork"]
                        ["token"]
                        .as_str()
                        .unwrap()
                        .to_owned()
            })
        })
        .collect::<Vec<_>>();
    let tokens = workers
        .into_iter()
        .map(|w| w.join().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(tokens.len(), 20);
    let v = tokens.iter().collect::<Vec<_>>();
    a.release_artwork(v[0]).unwrap();
    assert!(a.read_preview(v[0], 0, 33).is_err());
    assert_eq!(a.read_preview(v[1], 0, 33).unwrap().mime_type, "image/png");
    for t in tokens {
        a.release_artwork(&t).unwrap();
    }
    let before = lock(&a.shared.status).unwrap().prepared_art.len();
    let result = call(
        &a,
        "jukebox:lookup-artwork",
        json!({"trackKey":"1:1:0","title":"Explicit title","artist":"Explicit artist"}),
    )
    .unwrap();
    assert_eq!(h.searches.load(Ordering::SeqCst), 1);
    assert_eq!(
        lock(&a.shared.status).unwrap().prepared_art.len(),
        before + 1
    );
    a.release_artwork(result["artwork"]["token"].as_str().unwrap())
        .unwrap();
    assert_eq!(lock(&a.shared.status).unwrap().prepared_art.len(), before);
    assert!(
        call(
            &a,
            "jukebox:choose-artwork",
            json!({"trackKey":"1:1:0","path":"C:/renderer/path.png"})
        )
        .is_err()
    );
}

#[test]
fn playlist_reordering_defaults_and_single_metadata_are_atomic() {
    let f = Fixture::new();
    let (a, _) = open(&f);
    call(
        &a,
        "jukebox:create-playlist",
        json!({"name":"One","addDefaultSongs":true}),
    )
    .unwrap();
    call(&a, "jukebox:create-playlist", json!({"name":"Two"})).unwrap();
    let state = a.view().unwrap().state;
    assert_eq!(state.playlists[0].track_keys, vec!["1:1:0"]);
    let ids = state
        .playlists
        .iter()
        .map(|p| p.id.clone())
        .collect::<Vec<_>>();
    let rev = a.view().unwrap().revision;
    assert!(
        call(
            &a,
            "jukebox:reorder-playlists",
            json!({"playlistIds":[ids[0],ids[0]]})
        )
        .is_err()
    );
    assert_eq!(a.view().unwrap().revision, rev);
    call(
        &a,
        "jukebox:reorder-playlists",
        json!({"playlistIds":[ids[1],ids[0]]}),
    )
    .unwrap();
    assert_eq!(
        a.view().unwrap().state.playlists[1].track_keys,
        state.playlists[0].track_keys
    );
    assert!(AudioOptions::new(false, Some("Title".into()), None).is_err());
    assert!(AudioOptions::new(true, Some(" ".into()), None).is_err());
    assert!(AudioOptions::new(true, Some("Title".into()), Some("Artist".into())).is_ok());
    let bytes = serde_json::to_value(&state).unwrap();
    assert!(bytes.get("artworkOverrides").is_none());
    assert!(serde_json::from_value::<crate::jukebox_service::State>(bytes).is_ok());
}
