// Included only inside Jukebox test modules. These are compile-time-known
// crate layouts, not production storage discovery or a broad folder search.
fn fixture_artifacts() -> std::path::PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let project = match env!("CARGO_PKG_NAME") {
        "nba2k27-launcher-tauri2" => manifest.to_path_buf(),
        "jukebox-offline-harness" | "jukebox-api-harness" => manifest
            .parent()
            .and_then(std::path::Path::parent)
            .expect("Known Jukebox harness must be project/test/<harness>")
            .to_path_buf(),
        other => panic!("No Jukebox fixture root is authorized for package {other}"),
    }
    .canonicalize()
    .expect("Known Jukebox project root must exist");
    assert_eq!(
        project.file_name().and_then(|s| s.to_str()),
        Some("nba2k27-launcher-tauri2")
    );
    assert!(project.join("src/jukebox_service.rs").is_file());
    let artifacts = project.join("artifacts");
    std::fs::create_dir_all(&artifacts).expect("Create project-only Jukebox fixture parent");
    artifacts
        .canonicalize()
        .expect("Project fixture directory must exist")
}
