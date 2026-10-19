//! Actual-settings compatibility dry-run, confined to a copied workspace fixture.
//! Uses the production Store plus exact copied Backend Settings/validator.
#![allow(dead_code)]
mod backend {
    use anyhow::{Result, ensure};
    use serde::{Deserialize, Serialize};
    #[derive(Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", default, deny_unknown_fields)]
    pub struct Settings {
        pub game_root: String,
        pub mods_root: String,
        pub profile: String,
        pub auto_show_overlay: bool,
        pub auto_hide_overlay: bool,
        pub remove_drone: bool,
        pub remove_dj: bool,
        pub remove_mc: bool,
        pub remove_crowd: bool,
        pub lock_camera: bool,
        pub cameras_enabled: bool,
        pub startup_video: bool,
        pub camera: String,
        pub arena: String,
        pub ball: String,
        pub scoreboard: String,
        pub theme: String,
        pub video: String,
        pub music: String,
        pub unlock_wnba: bool,
        pub unlock_wnba_freestyle: bool,
        pub scaler: f64,
    }
    impl Default for Settings {
        fn default() -> Self {
            let game = r"C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27".to_string();
            Self {
                mods_root: format!("{game}\\mods"),
                game_root: game,
                profile: "Default mods".into(),
                auto_show_overlay: true,
                auto_hide_overlay: true,
                remove_drone: false,
                remove_dj: false,
                remove_mc: false,
                remove_crowd: false,
                lock_camera: false,
                cameras_enabled: false,
                startup_video: false,
                camera: "Broadcast Low 2K18".into(),
                arena: "Default · Day / Night".into(),
                ball: "Currently installed".into(),
                scoreboard: "Default".into(),
                theme: "dark".into(),
                video: String::new(),
                music: String::new(),
                unlock_wnba: false,
                unlock_wnba_freestyle: false,
                scaler: 1.,
            }
        }
    }

    pub fn validate_settings(s: &Settings) -> Result<()> {
        ensure!(
            s.game_root.len() <= 1000 && s.mods_root.len() <= 1000 && s.profile.len() <= 100,
            "Settings exceed limits"
        );
        ensure!(
            matches!(s.theme.as_str(), "dark" | "light"),
            "Unsupported theme"
        );
        ensure!(
            s.scaler.is_finite() && (0.8..=1.2).contains(&s.scaler),
            "Scaler must be between 0.8 and 1.2"
        );
        Ok(())
    }

    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    pub fn unique_id() -> String {
        format!(
            "{:016x}{:016x}{:016x}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        )
    }
}
#[path = "../src/settings_compat.rs"]
mod settings_compat;
fn main() -> anyhow::Result<()> {
    use anyhow::ensure;
    let args=std::env::args().collect::<Vec<_>>();
    ensure!(args.len()==2,"Exactly one copied workspace fixture is required");
    let base=std::env::current_dir()?.join("artifacts").canonicalize()?;
    let root=std::path::PathBuf::from(&args[1]).canonicalize()?;
    ensure!(root.starts_with(&base) && root!=base,"Dry-run must remain below workspace artifacts");
    let legacy=root.join("Data/settings.json");
    let before=std::fs::read(&legacy)?;
    let keys=serde_json::from_slice::<serde_json::Value>(&before)?.as_object().ok_or_else(||anyhow::anyhow!("Settings are not an object"))?.len();
    let (store,settings,report)=settings_compat::Store::open(&root,backend::validate_settings)?;
    ensure!(report.source=="wpf-compatible-preferences" && report.imported.len()==3,
        "Actual WPF settings did not match the expected conservative compatibility route");
    ensure!(!settings.remove_drone && !settings.remove_dj && !settings.remove_mc && !settings.remove_crowd
        && !settings.unlock_wnba && !settings.unlock_wnba_freestyle && !settings.lock_camera
        && !settings.cameras_enabled && !settings.startup_video && settings.scaler==1.0,
        "A native feature was inherited from WPF");
    ensure!(std::fs::read(&legacy)?==before,"Copied original changed during compatibility import");
    drop(store);
    let (store,_,repeat)=settings_compat::Store::open(&root,backend::validate_settings)?;
    ensure!(repeat.source=="rust-current","Repeated startup reimported WPF");
    drop(store);
    ensure!(std::fs::read(&legacy)?==before,"Copied original changed during repeated startup");
    println!("{}",serde_json::to_string_pretty(&serde_json::json!({
        "schema":"actual-main-settings-compatibility-dry-run-v1",
        "copiedOnlySettings":true,"legacyKeyCount":keys,"compatibility":report,
        "nativeOptionsInherited":false,"repeatSource":repeat.source,
        "copiedOriginalUnchanged":true,"mainWritten":false,
        "privatePathsOrOptionValuesReported":false,"result":"passed"
    }))?);
    Ok(())
}


