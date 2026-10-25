fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&["exporter_invoke", "smoke_result"]),
    ))
    .expect("Could not build Tauri resources");
    println!("cargo:rerun-if-changed=Web");
}
