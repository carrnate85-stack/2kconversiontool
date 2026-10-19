include!("jukebox_test_paths.rs");
use super::*;
use std::time::Instant;

/// Explicit offline benchmark: read-only installed Version 2 game files, a
/// project-local copy of the saved legacy library, and a scratch Mods root.
/// No process access, no game-folder writes. Run with `--release --ignored`.
#[test]
#[ignore = "Offline launch-preparation benchmark against the installed game (read-only)"]
fn offline_prepare_cold_vs_warm_benchmark() {
    let game = Path::new("C:/Program Files (x86)/Steam/steamapps/common/NBA 2K27");
    let artifacts = fixture_artifacts();
    let source = artifacts.join("legacy-media-staging-20261016-native");
    let root = artifacts.join("qa047-prepared-bench");
    if !root.join("Library/Jukebox/Audio").is_dir() {
        copy_tree(&source.join("Data/Jukebox"), &root.join("Data/Jukebox"));
        copy_tree(
            &source.join("Library/Jukebox"),
            &root.join("Library/Jukebox"),
        );
    }
    let mods = root.join("mods");
    fs::create_dir_all(&mods).unwrap();
    let _ = fs::remove_dir_all(root.join("Cache"));
    let summary = crate::jukebox_launch::selection_summary(&root).unwrap();
    let target = std::env::var("JUKEBOX_BENCH_PLAYLIST").unwrap_or_else(|_| "NBA 2k20".into());
    let choice = summary
        .playlists
        .iter()
        .find(|p| p.name == target)
        .expect("benchmark playlist");
    crate::jukebox_launch::select_saved(
        &root,
        Some(&choice.id),
        summary.revision.as_deref().unwrap(),
    )
    .unwrap();
    let plan = crate::jukebox_launch::freeze_saved(&root).unwrap();
    let stock_start = Instant::now();
    crate::jukebox_v2_resources::read_stock(game, || Ok(())).unwrap();
    println!(
        "{}",
        serde_json::json!({"phase":"uncachedReadStock","ms":stock_start.elapsed().as_millis()})
    );
    let mut rows = vec![];
    for (n, label) in ["cold", "warm", "warm"].into_iter().enumerate() {
        let session = format!("{:048x}", 0xa47 + n);
        let start = Instant::now();
        let prepared = crate::jukebox_launch::prepare(
            &root,
            game,
            &mods,
            &session,
            &plan,
            crate::jukebox_launch::Checks {
                prepare: &|| Ok(()),
                poll: &|| Ok(()),
                cleanup: &|| Ok(()),
            },
        )
        .unwrap()
        .unwrap();
        let prepare_ms = start.elapsed().as_millis();
        prepared.verify_before_launch(|| Ok(())).unwrap();
        let publication = prepared.publication().unwrap();
        let total_ms = start.elapsed().as_millis();
        prepared.restore(|| Ok(())).unwrap();
        rows.push(serde_json::json!({"run":label,"prepareMs":prepare_ms,"throughPublicationMs":total_ms,"files":publication.len(),"cache":prepared.cache,"bytes":publication.iter().map(|p|p.size).sum::<usize>()}));
        println!("{}", rows.last().unwrap());
    }
    assert_eq!(
        fs::read_dir(&mods)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .count(),
        0
    );
    println!(
        "{}",
        serde_json::json!({"schema":"nba2k27-jukebox-prepared-bench-v1","playlist":target,"songs":choice.song_count,"runs":rows,"gameProcessAccess":false,"gameFilesChanged":false})
    );
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&path, &to.join(entry.file_name()));
        } else {
            fs::copy(&path, to.join(entry.file_name())).unwrap();
        }
    }
}
