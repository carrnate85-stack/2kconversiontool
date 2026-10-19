//! Backend channels for the WPF Settings/Developer tools (`launcher-tools-*`).
//! WPF IPC equivalents: launcher:health, launcher:copy-health,
//! launcher:performance, resolver:status/rescan/copy, provider:diagnose,
//! bridge:audit, launcher:build-info, launcher:update-choose/-rollback/-cancel.
//! Channels needing a native dialog or app exit live in main.rs.
use anyhow::{Result, bail, ensure};
use serde_json::{Value, json};
use std::path::Path;

use crate::backend::Backend;

pub(crate) fn dispatch(b: &Backend, channel: &str, args: Value) -> Result<Value> {
    let settings = b.current_settings()?;
    let game = Path::new(&settings.game_root);
    match channel {
        "launcher-tools-health" => {
            let session = b.dispatch("native-session-status", Value::Null).ok();
            Ok(crate::launcher_health::report(&b.root, game, session.as_ref()))
        }
        "launcher-tools-health-copy" => {
            let session = b.dispatch("native-session-status", Value::Null).ok();
            let report = crate::launcher_health::report(&b.root, game, session.as_ref());
            let known = crate::launcher_health::known_paths(&settings, &b.root);
            Ok(json!({"text": crate::launcher_health::copy_text(&report, &known)?}))
        }
        "launcher-tools-performance" => {
            // main.js:177 accepts only catalog timings from the renderer.
            let name = args["name"].as_str().unwrap_or("");
            ensure!(name.starts_with("catalog."), "Unsupported timing event.");
            let ms = args["ms"].as_f64().unwrap_or(-1.);
            crate::launcher_health::record(&b.root, name, ms, false)?;
            Ok(json!(true))
        }
        "launcher-tools-resolver" => {
            let force = args["force"].as_bool().unwrap_or(false);
            crate::build_resolver::status(&b.root, game, force)
        }
        "launcher-tools-resolver-copy" => {
            let state = crate::build_resolver::status(&b.root, game, false)?;
            let known = crate::launcher_health::known_paths(&settings, &b.root);
            Ok(json!({"text": crate::launcher_health::copy_text(&state["report"], &known)?}))
        }
        "launcher-tools-provider-test" => {
            let path = crate::launcher_health::require_text(&args, "path", "png10000.iff")?;
            Ok(crate::launcher_health::provider_test(&b.root, Path::new(&settings.mods_root), &path))
        }
        "launcher-tools-bridge-audit" => {
            let path = crate::launcher_health::require_text(&args, "path", "png10000.iff")?;
            crate::launcher_health::bridge_audit(&b.root, game, Path::new(&settings.mods_root), &path)
        }
        "launcher-tools-migration-scan" => crate::wpf_migration::scan(b),
        "launcher-tools-migration-apply" => crate::wpf_migration::apply(b, &args),
        "launcher-tools-compatibility" => {
            let force = args["force"].as_bool().unwrap_or(false);
            crate::feature_relocation::report(&b.root, game, force)
        }
        "launcher-tools-hotkey" => Ok(crate::game_capture::hotkey_status()),
        "launcher-tools-build-info" => Ok(crate::launcher_update::build_info(&b.root)),
        "launcher-tools-update-rollback" => crate::launcher_update::prepare(&b.root, None, &|_| {}),
        "launcher-tools-update-cancel" => crate::launcher_update::cancel(),
        _ => bail!("Unsupported launcher tool: {channel}"),
    }
}
