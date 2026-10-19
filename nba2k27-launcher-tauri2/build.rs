fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "launcher",
            "preview_resource",
            "finish_smoke",
            "startup_intro_event",
        ]),
    ))
    .expect("Tauri resource generation failed");
    println!("cargo:rerun-if-changed=assets");
}
