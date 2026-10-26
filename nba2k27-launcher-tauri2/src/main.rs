#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod animation_codec;
mod animation_preview;
mod arena_ball_v2_adapter;
mod arena_runtime;
mod arena_session;
mod arena_v2;
mod backend;
mod ball_gameplay_runtime;
mod ball_gameplay_v2;
mod ball_runtime;
mod ball_session;
mod ball_thumbnails;
mod ball_v2;
mod bc7;
mod blacktop_arena;
mod blacktop_ball;
mod blacktop_native;
mod blacktop_outfit_match;
mod blacktop_outfits;
mod blacktop_unlock_v2;
mod browser_files;
mod browser_workspace;
mod build_resolver;
mod camera_localization;
mod camera_name_files;
mod camera_names;
mod camera_policy;
mod camera_v2;
mod close_policy;
mod custom_arena_profiles;
mod editor_presets;
mod feature_journal;
mod feature_plan;
mod feature_relocation;
mod feature_runtime;
mod file_identity;
mod fit_assets;
mod freestyle_v2;
#[cfg(test)]
mod freestyle_v2_live_probe;
mod game_capture;
mod game_rules;
mod helper_owner;
mod instance;
mod jukebox_api;
mod jukebox_artwork;
mod jukebox_artwork_session;
mod jukebox_audio;
mod jukebox_catalog;
mod jukebox_cover_route;
mod jukebox_decode_worker;
mod jukebox_decoder;
mod jukebox_files;
mod jukebox_http;
mod jukebox_launch;
mod jukebox_local;
mod jukebox_package;
mod jukebox_prepared;
mod jukebox_runtime;
mod jukebox_service;
mod jukebox_v2_resources;
mod launcher_health;
mod launcher_tools;
mod launcher_update;
mod legacy_saved_media;
mod library;
mod loader_files;
mod mod_install;
mod mod_profile;
mod native_overlay;
mod native_session;
mod online_ratings;
mod outfit_carry_v2;
mod outfit_journal;
mod outfit_match_v2;
mod outfit_runtime;
mod outfit_selection_session;
mod outfit_selection_v2;
mod outfit_selection_v2_session;
mod outfit_worker_v2_lease;
mod outfit_worker_v2_loader;
mod outfits;
mod owned_transaction;
mod player_audio_preview;
mod player_catalog;
mod player_catalog_store;
mod player_changes;
mod player_dna;
mod player_folders;
mod player_header_labels;
mod player_transaction;
mod presentation_v2;
mod presentation_v2_runtime;
mod preview_service;
mod previews;
mod provider_access;
mod provider_assets;
mod provider_build_profile;
mod provider_launch;
mod provider_pipe;
mod provider_protocol;
mod provider_worker;
mod rdat_roster;
mod reference_stats;
mod roster_artwork;
mod roster_artwork_import;
mod roster_native;
mod roster_snapshot;
mod roster_v2_access;
mod roster_v2_appearance;
mod roster_v2_arena;
mod roster_v2_arena_clone;
mod roster_v2_name_audio;
mod roster_v2_origins;
mod roster_v2_profile;
mod roster_v2_readonly;
mod roster_v2_services;
mod roster_v2_write;
mod rules_v2;
mod rules_v2_access;
mod scaler_npc_v2;
mod scaler_policy;
mod scaler_v2;
mod scoreboard_assets;
mod scoreboard_layout;
mod scoreboard_profile;
mod scoreboard_queue;
mod scoreboard_runtime;
mod scoreboard_session;
mod scoreboard_v2;
mod season_import_review;
mod season_math;
mod season_structure_v2;
mod selector_cache;
mod selector_files;
mod selector_generation;
mod selector_runtime;
mod selector_tables;
mod session_safety;
mod settings_compat;
mod sigconfig;
mod startup_mp4;
mod startup_video_audio;
mod startup_video_helper;
mod startup_video_import;
mod startup_video_launch;
mod startup_video_media;
mod startup_video_native;
mod startup_video_policy;
mod startup_video_runtime;
mod startup_video_tauri;
mod steam_libraries;
mod stock_selectors;
mod support_bundle;
mod team_changes;
mod team_dna;
mod team_transaction;
#[cfg(all(test, windows, target_arch = "x86_64"))]
mod test_crash_capture;
mod trusted_oodle;
mod uniform_changes;
mod uniform_transaction;
mod vcblowfish;
mod video_library;
mod wnba_loader_v2;
mod wnba_priority_categories;
mod wnba_priority_launch;
mod wnba_roster_v2;
mod wpf_migration;
use anyhow::{Context, Result, ensure};
use backend::Backend;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Instant,
};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use windows::{
    Win32::{
        Foundation::*,
        System::Com::*,
        UI::{Shell::*, WindowsAndMessaging::*},
    },
    core::*,
};

struct State {
    backend: Arc<Backend>,
    overlay: Arc<native_overlay::Runtime>,
    browser: Arc<browser_workspace::Runtime>,
    files: Arc<browser_files::Service>,
    startup: Arc<startup_video_launch::Coordinator>,
    started: Instant,
    smoke: bool,
}
static CLOSE_PROMPT_ACTIVE: AtomicBool = AtomicBool::new(false);
fn minimize_owned_session_on_close(window: &tauri::Window) -> bool {
    let Ok(owner) = window.hwnd() else {
        return false;
    };
    let answer = unsafe {
        MessageBoxW(
            Some(HWND(owner.0)),
            w!(
                "NBA 2K27 is starting, running, or finishing session cleanup.\n\nMinimize the launcher and keep it open?\n\nYes (recommended): minimize without interrupting the current session.\n\nNo: close anyway. This can interrupt startup or disable live controls. Close NBA 2K27 before reopening the launcher or completing recovery."
            ),
            w!("Keep 2k Launcher open?"),
            MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON1 | MB_SETFOREGROUND,
        )
    };
    close_policy::minimize(answer.0)
}
/// WPF showed an owned warning box before shared or inconsistent season saves.
fn editor_confirm(message: &str) -> bool {
    let text: Vec<u16> = format!(
        "{message}

Save anyway?"
    )
    .encode_utf16()
    .chain([0])
    .collect();
    let answer = unsafe {
        MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            w!("Save season history?"),
            MB_OKCANCEL | MB_ICONWARNING | MB_DEFBUTTON2 | MB_TOPMOST | MB_SETFOREGROUND,
        )
    };
    answer == IDOK
}
fn local(url: &tauri::Url) -> bool {
    // Vite is a development-only origin. Production binaries must never grant
    // command authority to a localhost web server.
    #[cfg(all(debug_assertions, not(feature = "custom-protocol")))]
    if url.scheme() == "http"
        && url.host_str() == Some("localhost")
        && url.port() == Some(5173)
        && url.username().is_empty()
        && url.password().is_none()
        && matches!(url.path(), "/" | "/index.html")
    {
        return true;
    }
    url.scheme() == "https"
        && url.host_str() == Some("tauri.localhost")
        && matches!(url.path(), "/" | "/index.html")
}
fn executable_portable_root(exe: &Path) -> Result<PathBuf> {
    let parent = exe.parent().context("Executable folder missing")?;
    if parent
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("App"))
    {
        Ok(parent
            .parent()
            .context("Portable root missing")?
            .to_path_buf())
    } else {
        Ok(parent.to_path_buf())
    }
}
fn trusted(w: &WebviewWindow) -> Result<()> {
    ensure!(
        matches!(w.label(), "main" | "overlay" | "live-overlay") && local(&w.url()?),
        "Only bundled prototype windows can invoke commands"
    );
    Ok(())
}
/// Explicit window authority before any dispatcher fallthrough. Overlays can
/// operate their catalogs, never settings paths, installers or update services.
fn channel_allowed(label: &str, channel: &str, args: &Value, smoke: bool) -> bool {
    if label == "main" {
        return true;
    }
    if !matches!(label, "overlay" | "live-overlay") {
        return false;
    }
    if channel == "save-settings" {
        return args.as_object().is_some_and(|p| {
            p.keys().all(|k| {
                matches!(
                    k.as_str(),
                    "arena" | "arenaId" | "ball" | "scoreboard" | "scoreboardId"
                )
            })
        }) || (smoke && label == "overlay");
    }
    if matches!(
        channel,
        "bootstrap"
            | "media"
            | "library"
            | "outfit"
            | "choices"
            | "wardrobe"
            | "outfit-pieces"
            | "outfit-for-pool"
            | "random-outfit"
            | "select-outfit-piece"
            | "save-outfit"
            | "arena-favorites"
            | "arena-favorite"
    ) {
        return true;
    }
    if label == "overlay" {
        return channel == "overlay-close";
    }
    matches!(
        channel,
        "native-session-status"
            | "native-overlay-status"
            | "native-overlay-heartbeat"
            | "native-overlay-layout"
            | "native-overlay-hide"
            | "native-rules-read"
            | "native-rules-apply"
            | "native-arena-select"
            | "native-ball-select"
            | "native-scoreboard-select"
            | "native-outfit-command"
            | "previews:ball-thumbnail"
    ) || (smoke
        && matches!(
            channel,
            "smoke-native-overlay-metrics" | "smoke-native-overlay-result"
        ))
}
fn preview_window_allowed(label: &str, channel: &str) -> bool {
    (label == "main" && channel.starts_with("previews:"))
        || (label == "live-overlay" && channel == "previews:ball-thumbnail")
}
fn production_launch_window(smoke: bool, label: &str) -> Result<()> {
    ensure!(
        !smoke && label == "main",
        "Steam launch is disabled in smoke mode and overlay windows"
    );
    Ok(())
}
fn build_live_overlay(
    handle: &tauri::AppHandle,
    owner: HWND,
    profile: PathBuf,
    smoke: bool,
) -> Result<WebviewWindow> {
    let builder = WebviewWindowBuilder::new(
        handle,
        "live-overlay",
        WebviewUrl::App("index.html?overlay=1&live=1".into()),
    )
    .title("Blacktop native F8 · QA")
    .owner_raw(owner)
    .inner_size(1200., 112.)
    .decorations(false)
    .resizable(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    .focused(false)
    .focusable(false)
    .data_directory(profile)
    .use_https_scheme(true)
    .disable_drag_drop_handler()
    .devtools(false)
    .on_navigation(local)
    .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
    .on_permission_request(|_, _| tauri::webview::PermissionResponse::Deny);
    let builder = if smoke {
        builder.initialization_script("window.__nativeOverlayProbe=true;")
    } else {
        builder
    };
    Ok(builder.build()?)
}
/// Shared by the explicit Enable command and the overlay worker's
/// "Automatically show at player selection" attach.
fn enable_live_overlay(
    handle: &tauri::AppHandle,
    state: &State,
) -> std::result::Result<Value, String> {
    let frame = state.overlay.enable().map_err(|e| format!("{e:#}"))?;
    // A previous game can own/destroy its overlay. Never reuse a window
    // with the previous game's owner HWND, even across identical PIDs.
    if let Some(old) = handle.get_webview_window("live-overlay") {
        let _ = state.overlay.disable();
        let _ = old.close();
        return Err("Previous F8 window is closing; enable again once closed".into());
    }
    let built = build_live_overlay(
        handle,
        HWND(frame.hwnd as _),
        state.backend.root.join("Cache/WebView2"),
        false,
    );
    match built {
        Ok(w) => {
            let bounds = frame.bounds(false, 112).map_err(|e| e.to_string())?;
            w.set_position(tauri::PhysicalPosition::new(bounds.x, bounds.y))
                .map_err(|e| e.to_string())?;
            w.set_size(tauri::PhysicalSize::new(bounds.width, bounds.height))
                .map_err(|e| e.to_string())?;
            Ok(state.overlay.status())
        }
        Err(e) => {
            let _ = state.overlay.disable();
            Err(e.to_string())
        }
    }
}
fn pick(hwnd: HWND, folders: bool) -> Result<Value> {
    unsafe {
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
        let mut opts = FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST | FOS_NOCHANGEDIR;
        if folders {
            opts |= FOS_PICKFOLDERS
        } else {
            opts |= FOS_FILEMUSTEXIST
        }
        dialog.SetOptions(opts)?;
        if let Err(e) = dialog.Show(Some(hwnd)) {
            if e.code() == HRESULT(0x800704c7u32 as i32) {
                return Ok(json!(null));
            }
            return Err(e.into());
        }
        let p = dialog.GetResult()?.GetDisplayName(SIGDN_FILESYSPATH)?;
        let value = p.to_string()?;
        CoTaskMemFree(Some(p.0 as _));
        Ok(json!(value))
    }
}
/// WPF `mods:install` chooser: several files/archives, or one folder.
fn pick_mods(hwnd: HWND, folder: bool) -> Result<Value> {
    unsafe {
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
        let mut opts = FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST | FOS_NOCHANGEDIR;
        if folder {
            opts |= FOS_PICKFOLDERS;
            dialog.SetTitle(w!("Choose a mod folder"))?;
        } else {
            opts |= FOS_FILEMUSTEXIST | FOS_ALLOWMULTISELECT;
            dialog.SetTitle(w!("Choose mod files, ZIPs, or RARs"))?;
            use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
            dialog.SetFileTypes(&[
                COMDLG_FILTERSPEC {
                    pszName: w!("Mod packages"),
                    pszSpec: w!("*.zip;*.rar;*.iff;*.bin;*.dds"),
                },
                COMDLG_FILTERSPEC {
                    pszName: w!("All files"),
                    pszSpec: w!("*.*"),
                },
            ])?;
        }
        dialog.SetOptions(opts)?;
        if let Err(e) = dialog.Show(Some(hwnd)) {
            if e.code() == HRESULT(0x800704c7u32 as i32) {
                return Ok(json!([]));
            }
            return Err(e.into());
        }
        let items = dialog.GetResults()?;
        let mut paths = Vec::new();
        for index in 0..items.GetCount()? {
            let p = items.GetItemAt(index)?.GetDisplayName(SIGDN_FILESYSPATH)?;
            let value = p.to_string();
            CoTaskMemFree(Some(p.0 as _));
            paths.push(value?);
        }
        Ok(json!(paths))
    }
}
fn require_game_closed() -> std::result::Result<(), String> {
    let running = session_safety::processes()
        .map_err(|e| format!("{e:#}"))?
        .iter()
        .any(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe"));
    if running {
        return Err("Close NBA 2K27 before installing mods.".into());
    }
    Ok(())
}
async fn ui(
    window: WebviewWindow,
    f: impl FnOnce(HWND) -> Result<Value> + Send + 'static,
) -> std::result::Result<Value, String> {
    let hwnd = window.hwnd().map_err(|e| e.to_string())?.0 as usize;
    let (tx, rx) = mpsc::channel();
    window
        .run_on_main_thread(move || {
            let _ = tx.send(f(HWND(hwnd as _)).map_err(|e| format!("{e:#}")));
        })
        .map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || rx.recv().map_err(|e| e.to_string())?)
        .await
        .map_err(|e| e.to_string())?
}
/// Resolves a mod-library open request to a canonical path inside the active
/// mods folder. Returns the path and whether Explorer should select it.
fn mod_open_target(mods_root: &str, args: Option<&Value>) -> Result<(String, bool)> {
    let root = std::fs::canonicalize(mods_root)
        .ok()
        .filter(|p| p.is_dir())
        .context("The selected mods folder does not exist")?;
    let (path, select) = match args {
        None => (root, false),
        Some(args) => {
            let rel = args["path"].as_str().context("Mod path required")?;
            let rel = std::path::Path::new(rel);
            ensure!(
                rel.as_os_str().len() <= 1024
                    && rel.components().count() > 0
                    && rel
                        .components()
                        .all(|c| matches!(c, std::path::Component::Normal(_))),
                "Invalid mod path"
            );
            let file = std::fs::canonicalize(root.join(rel))
                .context("The mod file no longer exists; scan again")?;
            ensure!(
                file.starts_with(&root) && file.is_file(),
                "The mod file is outside the selected mods folder"
            );
            match args["action"].as_str() {
                Some("folder") => (file, true),
                Some("file") => {
                    // Open data files only; never execute programs or scripts
                    // that happen to live inside a mod folder.
                    let ext = file
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    const DATA: [&str; 22] = [
                        "iff", "bin", "dds", "png", "jpg", "jpeg", "bmp", "tga", "txt", "json",
                        "ini", "xml", "csv", "log", "md", "wav", "mp3", "ogg", "flac", "mp4",
                        "webm", "bk2",
                    ];
                    ensure!(
                        DATA.contains(&ext.as_str()),
                        "This file type cannot be opened from the launcher; use Folder instead"
                    );
                    (file, false)
                }
                _ => anyhow::bail!("Unknown mod-library open action"),
            }
        }
    };
    let text = path.to_string_lossy().into_owned();
    let text = if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        let stripped = text.strip_prefix(r"\\?\").map(str::to_owned);
        stripped.unwrap_or(text)
    };
    Ok((text, select))
}
/// Decoded clothing preview resource for the mannequin worker (WPF
/// Nba2kFastFitDecoder output), returned as raw bytes.
#[tauri::command]
async fn preview_resource(
    window: WebviewWindow,
    state: tauri::State<'_, State>,
    name: String,
) -> std::result::Result<tauri::ipc::Response, String> {
    trusted(&window).map_err(|e| e.to_string())?;
    if window.label() != "main" || name.len() > 240 {
        return Err("Previews require the main launcher window".into());
    }
    let backend = state.backend.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let game = backend.current_settings()?.game_root;
        preview_service::service(&backend.root).resource(std::path::Path::new(&game), &name)
    })
    .await
    .map_err(|e| e.to_string())?
    .map(tauri::ipc::Response::new)
    .map_err(|e| format!("{e:#}"))
}
#[tauri::command]
async fn launcher(
    window: WebviewWindow,
    state: tauri::State<'_, State>,
    channel: String,
    args: Value,
) -> std::result::Result<Value, String> {
    trusted(&window).map_err(|e| e.to_string())?;
    if !channel_allowed(window.label(), &channel, &args, state.smoke) {
        return Err("This command requires the main launcher window".into());
    }
    if channel.len() > 60
        || serde_json::to_vec(&args).map_err(|e| e.to_string())?.len() > 2 * 1024 * 1024
    {
        return Err("Request exceeds limits".into());
    }
    match channel.as_str() {
        "native-session-status" => {
            let backend = state.backend.clone();
            let mut status = tauri::async_runtime::spawn_blocking(move || {
                backend.session.status().map_err(|e| format!("{e:#}"))
            })
            .await
            .map_err(|e| e.to_string())??;
            status["startupIntro"] = state.startup.status();
            return Ok(status);
        }
        "startup-video-status" => {
            if window.label() != "main" {
                return Err("Startup status requires main window".into());
            }
            return Ok(state.startup.status());
        }
        "native-launch-modded" | "native-launch-steam-only" => {
            production_launch_window(state.smoke, window.label()).map_err(|e| format!("{e:#}"))?;
            state
                .startup
                .ensure_available()
                .map_err(|e| format!("{e:#}"))?;
            let backend = state.backend.clone();
            let command = channel.clone();
            let (mut result, intro) = tauri::async_runtime::spawn_blocking(move || {
                backend
                    .launch_request(&command, args)
                    .map_err(|e| format!("{e:#}"))
            })
            .await
            .map_err(|e| e.to_string())??;
            if let Some(intro) = intro {
                let session = result["session"].as_str().unwrap_or("").to_owned();
                // The game has already been dispatched. A separate intro error
                // must never make the UI imply that launch itself failed.
                result["startupIntro"] =
                    match state
                        .startup
                        .arm(Arc::downgrade(&state.backend), session, intro)
                    {
                        Ok(()) => json!({"phase":"waiting-for-launched-game"}),
                        Err(error) => json!({"phase":"not-played","message":format!("{error:#}")}),
                    };
            }
            return Ok(result);
        }
        channel if channel.starts_with("previews:") => {
            // WPF Blacktop previews: thumbnails, Prepare all, mannequin index.
            if !preview_window_allowed(window.label(), channel) {
                return Err("Previews require the main launcher; the live overlay may only request ball thumbnails".into());
            }
            let backend = state.backend.clone();
            let handle = window.app_handle().clone();
            let command = channel.to_owned();
            return tauri::async_runtime::spawn_blocking(move || {
                let game = backend
                    .current_settings()
                    .map_err(|e| format!("{e:#}"))?
                    .game_root;
                let emit: preview_service::Emit = Arc::new(move |event, payload| {
                    let _ = handle.emit(event, payload);
                });
                preview_service::service(&backend.root)
                    .dispatch(&command, &args, std::path::Path::new(&game), emit)
                    .map_err(|e| format!("{e:#}"))
            })
            .await
            .map_err(|e| e.to_string())?;
        }
        channel if channel.starts_with("jukebox:") => {
            if window.label() != "main" {
                return Err("Jukebox requires the main launcher window".into());
            }
            let backend = state.backend.clone();
            let app = window.app_handle().clone();
            let smoke = state.smoke;
            let command = channel.to_owned();
            return tauri::async_runtime::spawn_blocking(move || {
                if command == "jukebox:selection-summary" {
                    return backend
                        .jukebox
                        .selection_summary()
                        .map_err(|e| format!("{e:#}"));
                }
                if command == "jukebox:play-set-active" {
                    return backend
                        .jukebox
                        .select_for_play(args)
                        .map_err(|e| format!("{e:#}"));
                }
                if smoke {
                    return backend
                        .jukebox
                        .dispatch_smoke(&command, args)
                        .map_err(|e| format!("{e:#}"));
                }
                let game = backend
                    .current_settings()
                    .map_err(|e| format!("{e:#}"))?
                    .game_root;
                let native = backend.session.status().unwrap_or_else(
                    |error| json!({"phase":"needs-review","error":format!("{error:#}")}),
                );
                backend
                    .jukebox
                    .dispatch(
                        &command,
                        args,
                        std::path::Path::new(&game),
                        app,
                        smoke,
                        &native,
                    )
                    .map_err(|e| format!("{e:#}"))
            })
            .await
            .map_err(|e| e.to_string())?;
        }
        "reference-stats-load" => {
            if state.smoke || window.label() != "main" {
                return Err("Network stats import requires the main production window; network disabled in smoke mode".into());
            }
        }
        "reference-stats-stage" => {
            if window.label() != "main" {
                return Err("Stats staging requires the main editor window".into());
            }
        }
        "smoke-reference-load" => {
            if !state.smoke || window.label() != "main" {
                return Err("Stats fixture requires main smoke window".into());
            }
            if crate::reference_stats::player_url(args["url"].as_str().unwrap_or(""))
                .map_err(|e| e.to_string())?
                != "https://www.basketball-reference.com/players/m/maxeyty01.html"
            {
                return Err("Only the offline fixture URL is supported in smoke mode".into());
            }
            return state
                .backend
                .reference
                .fixture()
                .map_err(|e| format!("{e:#}"));
        }
        "native-rules-read"
        | "native-rules-apply"
        | "native-arena-select"
        | "native-ball-select"
        | "native-scoreboard-select"
        | "native-outfit-command" => {
            if state.smoke || !matches!(window.label(), "main" | "live-overlay") {
                return Err(
                    "Live rule/arena access is disabled in smoke mode and offline previews".into(),
                );
            }
            if window.label() == "live-overlay" {
                state.overlay.authorize().map_err(|e| format!("{e:#}"))?;
            }
        }
        "native-camera-command" => {
            if state.smoke || window.label() != "main" {
                return Err(
                    "Live camera control requires the main window and is disabled in smoke mode"
                        .into(),
                );
            }
        }
        "native-overlay-status" => {
            if state.smoke && window.label() == "live-overlay" {
                return Ok(
                    json!({"enabled":true,"phase":"fixture","boundary":"selection","visible":true,"message":"Isolated owned-window fixture; no game access","playerBinding":false,"gameWrites":false}),
                );
            }
            return Ok(state.overlay.status());
        }
        "native-overlay-disable" | "native-overlay-toggle" => {
            if state.smoke || window.label() != "main" {
                return Err("Native F8 control requires the main production window".into());
            }
            return (if channel == "native-overlay-disable" {
                state.overlay.disable()
            } else {
                state.overlay.toggle()
            })
            .map_err(|e| format!("{e:#}"));
        }
        "native-overlay-hide" => {
            if state.smoke || window.label() != "live-overlay" {
                return Err("Not a live F8 window".into());
            }
            return state.overlay.hide().map_err(|e| format!("{e:#}"));
        }
        "native-overlay-heartbeat" => {
            if state.smoke || window.label() != "live-overlay" {
                return Err("Heartbeat requires live F8 window".into());
            }
            return state.overlay.heartbeat().map_err(|e| format!("{e:#}"));
        }
        "native-overlay-layout" => {
            #[derive(serde::Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Layout {
                catalog: bool,
                bar_height: u32,
                #[serde(default)]
                dropdown: bool,
            }
            if window.label() != "live-overlay" {
                return Err("Layout requires the live F8 window".into());
            }
            let layout: Layout = serde_json::from_value(args).map_err(|e| e.to_string())?;
            if !(70..=600).contains(&layout.bar_height) {
                return Err("Unsupported bar height".into());
            }
            if state.smoke {
                window
                    .set_focusable(layout.catalog)
                    .map_err(|e| e.to_string())?;
                let main = window
                    .app_handle()
                    .get_webview_window("main")
                    .ok_or("Missing fixture owner")?;
                let expected = main.hwnd().map_err(|e| e.to_string())?;
                if unsafe { GetWindow(HWND(window.hwnd().map_err(|e| e.to_string())?.0), GW_OWNER) }
                    .ok()
                    != Some(HWND(expected.0))
                {
                    return Err("Fixture window owner mismatch".into());
                }
                let bounds = native_overlay::fixture_frame(HWND(expected.0))
                    .and_then(|f| f.bounds(layout.catalog && !layout.dropdown, layout.bar_height))
                    .map_err(|e| e.to_string())?;
                window
                    .set_position(tauri::PhysicalPosition::new(bounds.x, bounds.y))
                    .map_err(|e| e.to_string())?;
                window
                    .set_size(tauri::PhysicalSize::new(bounds.width, bounds.height))
                    .map_err(|e| e.to_string())?;
                return Ok(json!({"fixture":true}));
            }
            return state
                .overlay
                .layout(layout.catalog, layout.bar_height, layout.dropdown)
                .map_err(|e| format!("{e:#}"));
        }
        "smoke-native-overlay-open" => {
            if !state.smoke || window.label() != "main" {
                return Err("Fixture requires main smoke window".into());
            }
            build_live_overlay(
                window.app_handle(),
                HWND(window.hwnd().map_err(|e| e.to_string())?.0),
                state.backend.root.join("Cache/WebView2"),
                true,
            )
            .map_err(|e| e.to_string())?;
            return Ok(json!(true));
        }
        "smoke-native-overlay-stop" => {
            if !state.smoke || window.label() != "main" {
                return Err("Stop probe requires main smoke window".into());
            }
            let start = Instant::now();
            state.overlay.shutdown();
            let result = json!({"fixture":true,"milliseconds":start.elapsed().as_secs_f64()*1000.,"status":state.overlay.status()});
            backend::atomic_json(
                &state.backend.root.join("Logs/native-overlay-stop.json"),
                &result,
            )
            .map_err(|e| e.to_string())?;
            return Ok(result);
        }
        "smoke-native-overlay-metrics" => {
            if !state.smoke || window.label() != "live-overlay" {
                return Err("Fixture requires owned smoke overlay".into());
            }
            let main = window
                .app_handle()
                .get_webview_window("main")
                .ok_or("Missing fixture owner")?;
            let owned =
                unsafe { GetWindow(HWND(window.hwnd().map_err(|e| e.to_string())?.0), GW_OWNER) }
                    .ok()
                    == Some(HWND(main.hwnd().map_err(|e| e.to_string())?.0));
            return Ok(
                json!({"owned":owned,"hidden":!window.is_visible().map_err(|e|e.to_string())?,"focusable":unsafe{GetWindowLongPtrW(HWND(window.hwnd().map_err(|e|e.to_string())?.0),GWL_EXSTYLE)} as u32&WS_EX_NOACTIVATE.0==0,"size":window.inner_size().map_err(|e|e.to_string())?,"position":window.outer_position().map_err(|e|e.to_string())?}),
            );
        }
        "smoke-native-overlay-result" => {
            if !state.smoke {
                return Err("Fixture requires smoke mode".into());
            }
            let path = state.backend.root.join("Logs/native-overlay-qa.json");
            if window.label() == "live-overlay" {
                backend::atomic_json(&path, &args).map_err(|e| e.to_string())?;
                window.close().map_err(|e| e.to_string())?;
                return Ok(json!(true));
            }
            if window.label() != "main" {
                return Err("Not the fixture owner".into());
            }
            return Ok(if path.is_file() {
                serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?
            } else {
                Value::Null
            });
        }
        "native-overlay-enable" => {
            #[derive(serde::Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Enable {
                acknowledgement: String,
                game_root: String,
            }
            if state.smoke || window.label() != "main" {
                return Err("Native F8 is disabled in smoke mode and overlay windows".into());
            }
            let enable: Enable = serde_json::from_value(args).map_err(|e| e.to_string())?;
            if enable.acknowledgement != "native-overlay-explicit-qa"
                || enable.game_root
                    != state
                        .backend
                        .current_settings()
                        .map_err(|e| e.to_string())?
                        .game_root
            {
                return Err("Confirm the current game root and native F8 limitations".into());
            }
            return enable_live_overlay(window.app_handle(), &state);
        }
        "smoke-game-rules" => {
            if !state.smoke {
                return Err("Rules fixture requires smoke mode".into());
            }
            let report = game_rules::smoke().map_err(|e| format!("{e:#}"))?;
            backend::atomic_json(&state.backend.root.join("Logs/game-rules-qa.json"), &report)
                .map_err(|e| e.to_string())?;
            return Ok(report);
        }
        "native-blacktop-enable" => {
            if state.smoke || window.label() != "main" {
                return Err("Live observer is disabled in smoke mode and overlay windows".into());
            }
        }
        "smoke-blacktop-observer" => {
            if !state.smoke {
                return Err("Observer fixture requires smoke mode".into());
            }
            let report = blacktop_native::smoke().map_err(|e| format!("{e:#}"))?;
            backend::atomic_json(
                &state.backend.root.join("Logs/blacktop-observer-qa.json"),
                &report,
            )
            .map_err(|e| e.to_string())?;
            return Ok(report);
        }
        "startup-decode-fixture" => {
            if !state.smoke
                || window.label() != "main"
                || !std::env::args().any(|a| a == "--startup-video-check")
            {
                return Err(
                    "MP4 decode fixture requires its isolated main-window test mode".into(),
                );
            }
            let backend = state.backend.clone();
            return tauri::async_runtime::spawn_blocking(move || {
                backend.decode_fixture().map_err(|e| format!("{e:#}"))
            })
            .await
            .map_err(|e| e.to_string())?;
        }
        "smoke-native-session" => {
            if !state.smoke {
                return Err("Native session fixtures require isolated smoke mode".into());
            }
            let root = state.backend.root.clone();
            return tauri::async_runtime::spawn_blocking(move || -> Result<Value> {
                let result = native_session::smoke_lifecycle(&root)?;
                backend::atomic_json(&root.join("Logs/native-session-qa.json"), &result)?;
                Ok(result)
            })
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("{e:#}"));
        }
        "smoke-owned-helper" => {
            if !state.smoke {
                return Err("Helper probes require isolated smoke mode".into());
            }
            let root = state.backend.root.clone();
            return tauri::async_runtime::spawn_blocking(move || -> Result<Value> {
                let result = helper_owner::smoke(&root)?;
                backend::atomic_json(&root.join("Logs/helper-ownership-qa.json"), &result)?;
                Ok(result)
            })
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("{e:#}"));
        }
        "launch-preflight" if state.smoke => {
            return session_safety::smoke_fixture(&state.backend.root)
                .map_err(|e| format!("{e:#}"));
        }
        "smoke-team-import-file" | "smoke-player-import-file" => {
            if !state.smoke || window.label() != "main" {
                return Err("Import fixtures require isolated smoke mode".into());
            }
            let is_team = channel == "smoke-team-import-file";
            let document = if is_team {
                team_dna::validate(&args)
            } else {
                player_dna::validate(&args)
            }
            .map_err(|e| e.to_string())?;
            let path = state.backend.root.join(if is_team {
                "Cache/Imports/qa-team.2kteam"
            } else {
                "Cache/Imports/qa-player.2kplayer"
            });
            backend::atomic_json(&path, &document).map_err(|e| e.to_string())?;
            return Ok(json!(true));
        }
        "smoke-resize" => {
            if !state.smoke || window.label() != "main" {
                return Err("Resize tests require the isolated main smoke window".into());
            }
            let width = args["width"].as_f64().unwrap_or(1440.);
            let height = args["height"].as_f64().unwrap_or(900.);
            if !(1060. ..=1920.).contains(&width) || !(700. ..=1080.).contains(&height) {
                return Err("Smoke size outside supported range".into());
            }
            window
                .set_size(tauri::LogicalSize::new(width, height))
                .map_err(|e| e.to_string())?;
            return Ok(json!(true));
        }
        "smoke-single-instance" => {
            if !state.smoke {
                return Err("Single-instance tests require isolated smoke mode".into());
            }
            let root = state.backend.root.clone();
            return tauri::async_runtime::spawn_blocking(move || {
                json!(instance::acquire(&root).is_err())
            })
            .await
            .map_err(|e| e.to_string());
        }
        "smoke-fixture" => {
            if !state.smoke {
                return Err("Fixture creation is restricted to isolated smoke mode".into());
            }
            let b = state.backend.clone();
            return tauri::async_runtime::spawn_blocking(move || -> Result<Value> {
                use std::{fs, io::Write};
                let root = b.root.join("Library/SmokeMods");
                for i in 0..60 {
                    let folder = root.join(format!("char/group-{i:02}"));
                    fs::create_dir_all(&folder)?;
                    fs::write(
                        folder.join(format!("player-{i}.iff")),
                        b"isolated read-only fixture",
                    )?;
                }
                // One pngNNNN base + dependency bundle for the mod-library
                // expand check (60 single entries + 1 bundle = 61 rows).
                for name in ["png1234.iff", "png1234_hair.iff"] {
                    fs::write(
                        root.join("char/group-12").join(name),
                        b"isolated read-only fixture",
                    )?;
                }
                let mut wav = Vec::new();
                wav.extend_from_slice(b"RIFF");
                wav.extend_from_slice(&38u32.to_le_bytes());
                wav.extend_from_slice(b"WAVEfmt ");
                wav.extend_from_slice(&16u32.to_le_bytes());
                wav.extend_from_slice(&1u16.to_le_bytes());
                wav.extend_from_slice(&1u16.to_le_bytes());
                wav.extend_from_slice(&44100u32.to_le_bytes());
                wav.extend_from_slice(&88200u32.to_le_bytes());
                wav.extend_from_slice(&2u16.to_le_bytes());
                wav.extend_from_slice(&16u16.to_le_bytes());
                wav.extend_from_slice(b"data");
                wav.extend_from_slice(&2u32.to_le_bytes());
                wav.extend_from_slice(&0u16.to_le_bytes());
                fs::write(b.root.join("Library/Music/Smoke.wav"), wav)?;
                let garment = b.dispatch(
                    "wardrobe",
                    json!({"slot":"top","pool":"male","offset":1,"limit":1}),
                )?["items"][0]
                    .clone();
                let zip_path = b.root.join("Cache/Imports/smoke-previews.zip");
                let mut image = Vec::new();
                {
                    let mut encoder = png::Encoder::new(&mut image, 256, 256);
                    encoder.set_color(png::ColorType::Rgba);
                    let pixels: [u8; 4] = [50, 120, 75, 255];
                    encoder
                        .write_header()?
                        .write_image_data(&pixels.repeat(256 * 256))?;
                }
                let mut writer = zip::ZipWriter::new(fs::File::create(&zip_path)?);
                writer.start_file(
                    garment["previewKey"]
                        .as_str()
                        .context("Preview key missing")?,
                    zip::write::SimpleFileOptions::default(),
                )?;
                writer.write_all(&image)?;
                writer.finish()?;
                b.previews.import(&zip_path, |_, _, _| {})?;
                Ok(json!({"modsRoot":root,"asset":garment["asset"]}))
            })
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("{e:#}"));
        }
        "capture" => {
            if !state.smoke {
                return Err("Screenshot capture is available only in isolated smoke mode".into());
            }
            let name = args["name"].as_str().ok_or("Capture name required")?;
            if name.is_empty()
                || name.len() > 40
                || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            {
                return Err("Invalid capture name".into());
            }
            let path = state.backend.root.join("Logs").join(format!("{name}.png"));
            let (tx, rx) = mpsc::channel();
            window.with_webview(move|webview|{let tx_error=tx.clone();let result=(||->windows::core::Result<()>{unsafe{
                let p:Vec<u16>=path.to_string_lossy().encode_utf16().chain(Some(0)).collect();
                let stream=SHCreateStreamOnFileEx(PCWSTR(p.as_ptr()),(STGM_CREATE|STGM_WRITE|STGM_SHARE_EXCLUSIVE).0,0,true,None)?;
                let view=webview.controller().CoreWebView2()?;
                let handler=webview2_com::CapturePreviewCompletedHandler::create(Box::new(move|status|{let _=tx.send(status.map(|_|true).map_err(|e|e.to_string()));Ok(())}));
                view.CapturePreview(webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG,&stream,&handler)?;
            }Ok(())})();if let Err(e)=result{let _=tx_error.send(Err(e.to_string()));}}).map_err(|e|e.to_string())?;
            return tauri::async_runtime::spawn_blocking(move || {
                rx.recv_timeout(std::time::Duration::from_secs(10))
                    .map_err(|e| e.to_string())?
                    .map(|v| json!(v))
            })
            .await
            .map_err(|e| e.to_string())?;
        }
        "pick-folder" if state.smoke => {
            // The packaged smoke cannot drive a native chooser; the WPF
            // New profile flow picks an owned, empty fixture folder instead.
            let path = state.backend.root.join("Library").join("SmokeProfile");
            std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            return Ok(json!(path));
        }
        "pick-folder" => return ui(window, |hwnd| pick(hwnd, true)).await,
        "launcher-tools-update-choose" => {
            if window.label() != "main" || state.smoke {
                return Err("This action is only available in the launcher.".into());
            }
            let chosen = ui(window.clone(), |hwnd| pick(hwnd, true)).await?;
            let Some(path) = chosen.as_str().map(PathBuf::from) else {
                return Ok(json!({"canceled":true,"message":"Update selection canceled."}));
            };
            let root = state.backend.root.clone();
            let handle = window.app_handle().clone();
            return tauri::async_runtime::spawn_blocking(move || {
                launcher_update::prepare(&root, Some(&path), &|message| {
                    let _ = handle.emit("launcher-update-progress", json!({"message":message}));
                })
                .map_err(|e| format!("{e:#}"))
            })
            .await
            .map_err(|e| e.to_string())?;
        }
        "launcher-tools-update-restart" => {
            if window.label() != "main" || state.smoke {
                return Err("This action is only available in the launcher.".into());
            }
            let root = state.backend.root.clone();
            tauri::async_runtime::spawn_blocking(move || launcher_update::start_runner(&root))
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| format!("{e:#}"))?;
            // The runner waits for this process to exit, then swaps App.
            let handle = window.app_handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(100));
                handle.exit(0);
            });
            return Ok(
                json!({"message":"Finishing session cleanup, then installing and restarting…"}),
            );
        }
        "import-previews" => {
            let chosen = ui(window.clone(), |hwnd| pick(hwnd, false)).await?;
            if chosen.is_null() {
                return Ok(chosen);
            }
            let path = PathBuf::from(chosen.as_str().ok_or("Invalid filename")?);
            let b = state.backend.clone();
            let handle = window.app_handle().clone();
            return tauri::async_runtime::spawn_blocking(move || {
                b.previews
                    .import(&path, |done, total, phase| {
                        let _ = handle.emit(
                            "preview-progress",
                            json!({"done":done,"total":total,"phase":phase}),
                        );
                    })
                    .map_err(|e| format!("{e:#}"))
            })
            .await
            .map_err(|e| e.to_string())?;
        }
        "import-media" => {
            let kind = args["kind"].as_str().ok_or("Library required")?.to_string();
            if !matches!(kind.as_str(), "Music" | "Videos") {
                return Err("Unsupported library".into());
            }
            let video_operation_id = if kind == "Videos" {
                let operation_id = args["operationId"]
                    .as_str()
                    .ok_or("Startup-video import ID required")?;
                startup_video_import::validate_frontend_id(operation_id)
                    .map_err(|e| format!("{e:#}"))?;
                Some(operation_id.to_owned())
            } else {
                None
            };
            let chosen = ui(window.clone(), |hwnd| pick(hwnd, false)).await?;
            if chosen.is_null() {
                return Ok(chosen);
            }
            let path = PathBuf::from(chosen.as_str().ok_or("Invalid filename")?);
            let b = state.backend.clone();
            if kind == "Videos" {
                let handle = window.app_handle().clone();
                let operation_id = video_operation_id.ok_or("Startup-video import ID required")?;
                return tauri::async_runtime::spawn_blocking(move || {
                    b.import_video_with_progress(&path, &operation_id, |id, update| {
                        handle
                            .emit(
                                "startup-video-import-progress",
                                json!({
                                    "operationId": id,
                                    "phase": update.phase,
                                    "bytes": update.bytes,
                                    "total": update.total,
                                }),
                            )
                            .map_err(Into::into)
                    })
                    .map_err(|e| format!("{e:#}"))
                })
                .await
                .map_err(|e| e.to_string())?;
            }
            return tauri::async_runtime::spawn_blocking(move || {
                b.import_media(&path, &kind).map_err(|e| format!("{e:#}"))
            })
            .await
            .map_err(|e| e.to_string())?;
        }
        "cancel-video-import" => {
            let operation_id = args["operationId"]
                .as_str()
                .ok_or("Startup-video import ID required")?;
            return state
                .backend
                .cancel_video_import(operation_id)
                .map(|cancelled| json!({"cancelled":cancelled}))
                .map_err(|e| format!("{e:#}"));
        }
        "import-outfit" => {
            let chosen = ui(window, |hwnd| pick(hwnd, false)).await?;
            if chosen.is_null() {
                return Ok(chosen);
            }
            let path = PathBuf::from(chosen.as_str().ok_or("Invalid filename")?);
            return tauri::async_runtime::spawn_blocking(
                move || -> std::result::Result<Value, String> {
                    (|| -> Result<Value> {
                        ensure!(
                            path.extension()
                                .and_then(|s| s.to_str())
                                .is_some_and(|s| s.eq_ignore_ascii_case("2koutfit")),
                            "Select a .2koutfit file"
                        );
                        use std::io::Read;
                        let file = std::fs::File::open(&path)?;
                        ensure!(
                            file.metadata()?.is_file() && file.metadata()?.len() <= 16384,
                            "Outfit file exceeds 16 KB"
                        );
                        let mut bytes = Vec::new();
                        file.take(16385).read_to_end(&mut bytes)?;
                        Ok(json!(outfits::catalog()?.imported_outfit(&bytes)?))
                    })()
                    .map_err(|e| format!("{e:#}"))
                },
            )
            .await
            .map_err(|e| e.to_string())?;
        }
        "import-player-dna" | "import-team-dna" => {
            let is_team = channel == "import-team-dna";
            let extension = if is_team { "2kteam" } else { "2kplayer" };
            let limit = if is_team { 512 * 1024 } else { 256 * 1024 };
            // The packaged QA suite uses an owned fixture file, never an arbitrary
            // external path. Ordinary windows always use the native file chooser.
            let chosen = if state.smoke {
                json!(state.backend.root.join(if is_team {
                    "Cache/Imports/qa-team.2kteam"
                } else {
                    "Cache/Imports/qa-player.2kplayer"
                }))
            } else {
                ui(window, |hwnd| pick(hwnd, false)).await?
            };
            if chosen.is_null() {
                return Ok(chosen);
            }
            let path = PathBuf::from(chosen.as_str().ok_or("Invalid filename")?);
            return tauri::async_runtime::spawn_blocking(
                move || -> std::result::Result<Value, String> {
                    (|| -> Result<Value> {
                        if is_team {
                            return team_dna::read_file(&path);
                        }
                        ensure!(
                            path.extension()
                                .and_then(|s| s.to_str())
                                .is_some_and(|s| s.eq_ignore_ascii_case(extension)),
                            "Select a .{extension} file"
                        );
                        use std::io::Read;
                        let file = std::fs::File::open(&path)?;
                        ensure!(
                            file.metadata()?.is_file() && file.metadata()?.len() <= limit,
                            "DNA file exceeds its size limit"
                        );
                        let mut bytes = Vec::new();
                        file.take(limit + 1).read_to_end(&mut bytes)?;
                        ensure!(
                            bytes.len() as u64 <= limit,
                            "DNA file exceeds its size limit"
                        );
                        let document = serde_json::from_slice(&bytes)?;
                        player_dna::validate(&document)
                    })()
                    .map_err(|e| format!("{e:#}"))
                },
            )
            .await
            .map_err(|e| e.to_string())?;
        }
        "open-browser-editor" => {
            if window.label() != "main" || state.smoke {
                return Err("Open the browser editor from the main prototype window".into());
            }
            let section = args["section"].as_str().unwrap_or("").to_owned();
            let preview = args["preview"].as_bool().unwrap_or(false);
            let browser = state.browser.clone();
            let url = tauri::async_runtime::spawn_blocking(move || browser.url(&section, preview))
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| format!("{e:#}"))?;
            return ui(window, move |hwnd| {
                // URL and fragment are generated solely by the owned loopback server.
                let value: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
                let result = unsafe {
                    ShellExecuteW(
                        Some(hwnd),
                        w!("open"),
                        PCWSTR(value.as_ptr()),
                        PCWSTR::null(),
                        PCWSTR::null(),
                        SW_SHOWNORMAL,
                    )
                };
                ensure!(
                    result.0 as isize > 32,
                    "Windows could not open your default browser"
                );
                Ok(json!(true))
            })
            .await;
        }
        "open-library" => {
            let kind = args["kind"].as_str().unwrap_or("");
            if !matches!(
                kind,
                "" | "Music" | "Videos" | "Previews" | "Outfits" | "Players" | "Teams"
            ) {
                return Err("Unsupported folder".into());
            }
            let path = state.backend.root.join("Library").join(kind);
            std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            return ui(window, move |hwnd| {
                let p: Vec<u16> = path
                    .to_string_lossy()
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                let result = unsafe {
                    ShellExecuteW(
                        Some(hwnd),
                        w!("open"),
                        PCWSTR(p.as_ptr()),
                        PCWSTR::null(),
                        PCWSTR::null(),
                        SW_SHOWNORMAL,
                    )
                };
                ensure!(result.0 as isize > 32, "Windows could not open library");
                Ok(json!(true))
            })
            .await;
        }
        "open-mods-folder" | "open-mod-item" => {
            if window.label() != "main" || state.smoke {
                return Err("Open mod files from the main launcher window".into());
            }
            let root = state
                .backend
                .current_settings()
                .map_err(|e| format!("{e:#}"))?
                .mods_root;
            let item = (channel == "open-mod-item").then_some(&args);
            let (path, select) = mod_open_target(&root, item).map_err(|e| format!("{e:#}"))?;
            return ui(window, move |hwnd| {
                // Explorer receives one quoted, canonical path inside the mods
                // folder; Windows paths cannot contain quote characters.
                let (file, params) = if select {
                    ("explorer.exe".to_owned(), format!("/select,\"{path}\""))
                } else {
                    (path, String::new())
                };
                let file: Vec<u16> = file.encode_utf16().chain(Some(0)).collect();
                let params: Vec<u16> = params.encode_utf16().chain(Some(0)).collect();
                let result = unsafe {
                    ShellExecuteW(
                        Some(hwnd),
                        w!("open"),
                        PCWSTR(file.as_ptr()),
                        PCWSTR(params.as_ptr()),
                        PCWSTR::null(),
                        SW_SHOWNORMAL,
                    )
                };
                ensure!(
                    result.0 as isize > 32,
                    "Windows could not open this mod item"
                );
                Ok(json!(true))
            })
            .await;
        }
        "mod-install-plan" | "mod-install-commit" => {
            if window.label() != "main" || state.smoke {
                return Err("Install mods from the main launcher window".into());
            }
            require_game_closed()?;
            let settings = state
                .backend
                .current_settings()
                .map_err(|e| format!("{e:#}"))?;
            let b = state.backend.clone();
            if channel == "mod-install-commit" {
                return tauri::async_runtime::spawn_blocking(move || {
                    b.mod_install
                        .commit(std::path::Path::new(&settings.mods_root), &args)
                        .map_err(|e| format!("{e:#}"))
                })
                .await
                .map_err(|e| e.to_string())?;
            }
            let mode = match args["mode"].as_str() {
                Some("folder") => mod_install::Mode::Folder,
                Some("files") => mod_install::Mode::Files,
                _ => return Err("Unknown mod install mode".into()),
            };
            let chosen = ui(window.clone(), move |hwnd| {
                pick_mods(hwnd, mode == mod_install::Mode::Folder)
            })
            .await?;
            let paths: Vec<PathBuf> = chosen
                .as_array()
                .ok_or("Invalid selection")?
                .iter()
                .filter_map(|v| v.as_str().map(PathBuf::from))
                .collect();
            if paths.is_empty() {
                return Ok(json!({"cancelled":true}));
            }
            return tauri::async_runtime::spawn_blocking(move || {
                b.mod_install
                    .plan(
                        std::path::Path::new(&settings.game_root),
                        std::path::Path::new(&settings.mods_root),
                        &paths,
                        mode,
                        &mod_install::tar_executable(),
                        &mod_install::Limits::default(),
                    )
                    .map_err(|e| format!("{e:#}"))
            })
            .await
            .map_err(|e| e.to_string())?;
        }
        "mod-install-ready" => {
            // WPF checks for a running game before showing any install dialog.
            if window.label() != "main" || state.smoke {
                return Err("Install mods from the main launcher window".into());
            }
            require_game_closed()?;
            return Ok(json!(true));
        }
        "mod-install-cancel" => {
            let b = state.backend.clone();
            return tauri::async_runtime::spawn_blocking(move || {
                b.mod_install
                    .cancel()
                    .map(|_| json!(true))
                    .map_err(|e| format!("{e:#}"))
            })
            .await
            .map_err(|e| e.to_string())?;
        }
        "open-support-folder" => {
            if window.label() != "main" || state.smoke {
                return Err("Open support diagnostics from the main launcher window".into());
            }
            let path = state.backend.root.join("Logs/Support");
            std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            return ui(window, move |hwnd| {
                let p: Vec<u16> = path
                    .to_string_lossy()
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                let result = unsafe {
                    ShellExecuteW(
                        Some(hwnd),
                        w!("open"),
                        PCWSTR(p.as_ptr()),
                        PCWSTR::null(),
                        PCWSTR::null(),
                        SW_SHOWNORMAL,
                    )
                };
                ensure!(
                    result.0 as isize > 32,
                    "Windows could not open support folder"
                );
                Ok(json!(true))
            })
            .await;
        }
        "open-launcher-storage" | "open-game-captures" => {
            if window.label() != "main" || state.smoke {
                return Err("Open launcher folders from the main launcher window".into());
            }
            let path = if channel == "open-launcher-storage" {
                state.backend.root.clone()
            } else {
                game_capture::captures_root(&state.backend.root)
            };
            std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            return ui(window, move |hwnd| {
                game_capture::shell_open(hwnd, &path)?;
                Ok(json!(true))
            })
            .await;
        }
        "game-hang-capture" => {
            if window.label() != "main" || state.smoke {
                return Err("Capture a game hang from the main launcher window".into());
            }
            let backend = state.backend.clone();
            return tauri::async_runtime::spawn_blocking(move || {
                let game = backend.current_settings()?.game_root;
                game_capture::capture(&backend.root, std::path::Path::new(&game), backend::VERSION)
            })
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("{e:#}"));
        }
        "steam-find-game" => {
            return tauri::async_runtime::spawn_blocking(|| {
                let games = steam_libraries::candidate_game_roots(&steam_libraries::steam_roots())
                    .into_iter()
                    .filter_map(|game| session_safety::resolve(&game).ok())
                    .filter_map(|plan| plan["gameRoot"].as_str().map(str::to_owned))
                    .map(|game| game.strip_prefix(r"\?\").map(str::to_owned).unwrap_or(game))
                    .collect::<std::collections::BTreeSet<_>>();
                json!({"games": games})
            })
            .await
            .map_err(|e| e.to_string());
        }
        "smoke-offline-preview-state" => {
            if !state.smoke || window.label() != "main" {
                return Err("Preview inspection requires main smoke window".into());
            }
            let handle = window.app_handle();
            let Some(preview) = handle.get_webview_window("overlay") else {
                return Ok(json!({"exists":false}));
            };
            let scale = preview.scale_factor().map_err(|e| e.to_string())?;
            let position = preview
                .outer_position()
                .map_err(|e| e.to_string())?
                .to_logical::<f64>(scale);
            let size = preview
                .inner_size()
                .map_err(|e| e.to_string())?
                .to_logical::<f64>(scale);
            let monitor = window
                .current_monitor()
                .map_err(|e| e.to_string())?
                .ok_or("Missing fixture monitor")?;
            let monitor_scale = monitor.scale_factor();
            return Ok(
                json!({"exists":true,"hidden":!preview.is_visible().map_err(|e|e.to_string())?,"position":position,"size":size,
                "expected":{"x":monitor.position().x as f64/monitor_scale+10.,"y":monitor.position().y as f64/monitor_scale+10.,"width":monitor.size().width as f64/monitor_scale-20.},"gameAccess":false}),
            );
        }
        "overlay-open" => {
            let handle = window.app_handle();
            if let Some(w) = handle.get_webview_window("overlay") {
                if !state.smoke {
                    w.show().map_err(|e| e.to_string())?;
                    w.set_focus().map_err(|e| e.to_string())?;
                }
                return Ok(json!(true));
            }
            let monitor = window.current_monitor().map_err(|e| e.to_string())?;
            let (width, x, y) = monitor
                .map(|m| {
                    let scale = m.scale_factor();
                    (
                        m.size().width as f64 / scale - 20.,
                        m.position().x as f64 / scale + 10.,
                        m.position().y as f64 / scale + 10.,
                    )
                })
                .unwrap_or((1400., 10., 10.));
            let builder = WebviewWindowBuilder::new(
                handle,
                "overlay",
                WebviewUrl::App("index.html?overlay=1".into()),
            )
            .title("Blacktop overlay · offline preview")
            .inner_size(width, 640.)
            .position(x, y)
            .decorations(false)
            .resizable(false)
            .always_on_top(true)
            .visible(!state.smoke)
            .focused(!state.smoke)
            .data_directory(state.backend.root.join("Cache/WebView2"))
            .use_https_scheme(true)
            .disable_drag_drop_handler()
            .devtools(false)
            .on_navigation(local)
            .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
            .on_permission_request(|_, _| tauri::webview::PermissionResponse::Deny);
            let builder = if state.smoke {
                builder.initialization_script("window.__overlayProbe=true;")
            } else {
                builder
            };
            builder.build().map_err(|e| e.to_string())?;
            return Ok(json!(true));
        }
        "overlay-close" => {
            if window.label() != "overlay" {
                return Err("Not an overlay window".into());
            }
            window.close().map_err(|e| e.to_string())?;
            return Ok(json!(true));
        }
        _ => {}
    }
    let signal = match channel.as_str() {
        "save-settings" => Some(("settings-changed", args.clone())),
        "save-outfit" => Some(("outfit-changed", args.clone())),
        _ => None,
    };
    let b = state.backend.clone();
    let bootstrap = channel == "bootstrap";
    let started = state.started;
    let mut result = tauri::async_runtime::spawn_blocking(move || {
        b.dispatch(&channel, args).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())??;
    if let Some((name, payload)) = signal {
        let payload = if name == "settings-changed" {
            result.clone()
        } else {
            payload
        };
        for (label, _) in window.app_handle().webview_windows() {
            if label != window.label() {
                let _ = window.app_handle().emit_to(label, name, &payload);
            }
        }
    }
    if bootstrap {
        let ms = started.elapsed().as_secs_f64() * 1000.;
        result["startupMs"] = json!(ms);
        // WPF launcher-health `startup` metric: first bootstrap of this run.
        static FIRST: std::sync::Once = std::sync::Once::new();
        let root = state.backend.root.clone();
        let smoke = state.smoke;
        FIRST.call_once(|| {
            let _ = launcher_health::record(&root, "startup", ms, smoke);
        });
    }
    Ok(result)
}
#[tauri::command]
fn startup_intro_event(
    window: WebviewWindow,
    state: tauri::State<'_, Arc<startup_video_launch::Coordinator>>,
    request: startup_video_tauri::Request,
) -> std::result::Result<Value, String> {
    state
        .request(&window, request)
        .map_err(|e| format!("{e:#}"))
}
#[tauri::command]
fn finish_smoke(
    window: WebviewWindow,
    state: tauri::State<'_, State>,
    result: Value,
) -> std::result::Result<(), String> {
    trusted(&window).map_err(|e| e.to_string())?;
    if !state.smoke {
        return Err("Smoke mode is disabled".into());
    }
    backend::atomic_json(
        &state.backend.root.join("Logs/ui-smoke.json"),
        &json!({"result":result,"elapsedMs":state.started.elapsed().as_secs_f64()*1000.}),
    )
    .map_err(|e| e.to_string())?;
    window.app_handle().exit(0);
    Ok(())
}
fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    // Owned audio helpers must dispatch before Tauri, storage or UI mutexes.
    if args.get(1).is_some_and(|a| a == "--jukebox-decode-worker") {
        ensure!(
            args.len() == 3,
            "Private audio helper requires its exact nonce"
        );
        return jukebox_decode_worker::run(&args[2]);
    }
    // This is the same native binary, not a second runtime. It has no UI,
    // browser capabilities or game-memory access, and must precede the UI mutex.
    if args.get(1).is_some_and(|a| a == "--native-mod-provider") {
        ensure!(
            args.len() == 2,
            "Provider bootstrap must use private standard input"
        );
        return provider_worker::run();
    }
    // Launcher update runner: a copy of the previous exe in
    // Recovery/Updates/Runner swaps App after the launcher exits (WPF
    // app-update-runner.cjs). No UI, no game access, before the UI mutex.
    if args
        .get(1)
        .is_some_and(|a| a == launcher_update::RUNNER_FLAG)
    {
        std::process::exit(launcher_update::runner_main(&args[2..]));
    }
    let smoke = args.iter().any(|a| a == "--smoke");
    let exe = std::env::current_exe()?;
    let parent = exe.parent().context("Executable folder missing")?;
    let smoke_root = args
        .windows(2)
        .find(|pair| pair[0] == "--smoke-root")
        .map(|pair| PathBuf::from(&pair[1]));
    let root = if smoke {
        smoke_root.unwrap_or_else(|| parent.join("Smoke"))
    } else {
        executable_portable_root(&exe)?
    };
    std::fs::create_dir_all(&root)?;
    ensure!(
        !root.join("App/2k Launcher WPF.exe").is_file()
            && !root.join("App/2k Launcher.exe").is_file(),
        "Close and back up the older WPF/Electron App before promoting Rust into this portable folder. Do not keep two launchers in the active App folder."
    );
    let _instance = instance::acquire(&root)?;
    if args.iter().any(|a| a == "--provider-worker-check") {
        ensure!(smoke, "Provider fixture requires isolated smoke mode");
        return provider_worker::self_check(&root);
    }
    if args.iter().any(|a| a == "--browser-workspace-check") {
        ensure!(
            smoke,
            "Browser workspace check requires isolated smoke mode"
        );
        let backend = Arc::new(Backend::new(root.clone())?);
        let browser = browser_workspace::Runtime::with_services(
            Arc::new(move |args| backend.browser_catalog(args)),
            Arc::new(|| Ok(())),
        );
        let url = browser.url("players", true)?;
        backend::atomic_json(
            &root.join("Logs/browser-workspace-session.json"),
            &json!({"url":url,"gameAccess":false,"expiresSeconds":180}),
        )?;
        let started = Instant::now();
        while started.elapsed() < std::time::Duration::from_secs(180)
            && !root.join("Logs/browser-check-stop").exists()
        {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        browser.shutdown();
        return Ok(());
    }
    if let Some(pair) = args
        .windows(2)
        .find(|pair| pair[0] == "--native-helper-probe")
    {
        ensure!(smoke, "Inert helper probe requires isolated smoke mode");
        return helper_owner::probe(&root, &pair[1]);
    }
    if let Some(pair) = args
        .windows(2)
        .find(|pair| pair[0] == "--check-launch-readiness")
    {
        ensure!(
            smoke,
            "Launch verification is restricted to isolated smoke mode"
        );
        std::fs::create_dir_all(root.join("Logs"))?;
        let report = session_safety::preflight(std::path::Path::new(&pair[1]), &root);
        backend::atomic_json(&root.join("Logs/launch-readiness.json"), &report)?;
        return Ok(());
    }
    if let Some(pair) = args
        .windows(2)
        .find(|pair| pair[0] == "--check-preview-zip")
    {
        ensure!(
            smoke,
            "Preview ZIP verification is restricted to isolated smoke mode"
        );
        std::fs::create_dir_all(root.join("Logs"))?;
        let start = Instant::now();
        let result = previews::validate(std::path::Path::new(&pair[1]), |_, _, _| {});
        let success = result.is_ok();
        let result = match result {
            Ok(v) => json!({"pass":true,"validation":v,"elapsedMs":start.elapsed().as_millis()}),
            Err(e) => json!({"pass":false,"error":format!("{e:#}")}),
        };
        backend::atomic_json(&root.join("Logs/preview-check.json"), &result)?;
        ensure!(
            success,
            "Preview ZIP check failed; see the isolated preview-check.json"
        );
        return Ok(());
    }
    let backend = Arc::new(Backend::new(root)?);
    if !smoke {
        custom_arena_profiles::Service::start(&backend)?;
    }
    let started = Instant::now();
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            launcher,
            preview_resource,
            finish_smoke,
            startup_intro_event
        ])
        .setup(move |app| {
            app.asset_protocol_scope()
                .allow_directory(backend.root.join("Library"), true)?;
            app.asset_protocol_scope()
                .allow_directory(backend.root.join("Cache/PreviewImages"), true)?;
            let profile = backend.root.join("Cache/WebView2");
            let files = Arc::new(browser_files::Service::new());
            let online_ratings = Arc::new(online_ratings::Service::new());
            if !smoke {
                let backend = backend.clone();
                let handle = app.handle().clone();
                game_capture::start_hotkey(backend.root.clone(), move || {
                    let backend = backend.clone();
                    let handle = handle.clone();
                    std::thread::spawn(move || {
                        let _ = handle.emit("game-capture-status", json!({"phase":"capturing"}));
                        let result = backend.current_settings().and_then(|s| {
                            game_capture::capture(
                                &backend.root,
                                std::path::Path::new(&s.game_root),
                                backend::VERSION,
                            )
                        });
                        let payload = match result {
                            Ok(value) => json!({"phase":"captured","result":value}),
                            Err(error) => json!({"phase":"error","message":format!("{error:#}")}),
                        };
                        let _ = handle.emit("game-capture-status", payload);
                    });
                });
            }
            backend.jukebox.set_decode_admission({
                let backend = Arc::downgrade(&backend);
                Arc::new(move || {
                    use crate::jukebox_api::admission::{Admission, session_quiet};
                    let backend = backend
                        .upgrade()
                        .context("Launcher closed before audio decode")?;
                    if !session_quiet(&backend.session.status()?) {
                        return Ok(Admission::NativeOnly);
                    }
                    let rows = session_safety::processes()?;
                    if rows.iter().any(|p| {
                        p.name.eq_ignore_ascii_case("NBA2K27.exe")
                            || session_safety::protected_process(&p.name)
                            || session_safety::launcher_process(p)
                    }) {
                        return Ok(Admission::NativeOnly);
                    }
                    Ok(if session_quiet(&backend.session.status()?) {
                        Admission::DecoderAllowed
                    } else {
                        Admission::NativeOnly
                    })
                })
            })?;
            let startup = Arc::new(startup_video_launch::Coordinator::new(
                app.handle().clone(),
                backend.root.clone(),
            ));
            app.manage(startup.clone());
            app.manage(State {
                backend: backend.clone(),
                files: files.clone(),
                startup,
                browser: Arc::new(
                    browser_workspace::Runtime::with_services(
                        {
                            let backend = backend.clone();
                            let files = files.clone();
                            let app_handle = app.handle().clone();
                            Arc::new(move |args| {
                                ensure!(!smoke, "Catalog file services are refused in smoke mode");
                                files.catalog(&backend.root, args, |choice| files.ask(&app_handle, choice))
                            })
                        },
                        {
                            let handle = app.handle().clone();
                            Arc::new(move || {
                                let main = handle
                                    .get_webview_window("main")
                                    .context("Launcher is closed")?;
                                main.show()?;
                                main.set_focus()?;
                                Ok(())
                            })
                        },
                    )
                    .with_roster({
                        let backend = backend.clone();
                        Arc::new(move |method, tab| {
                            ensure!(!smoke, "Native roster reads are refused in smoke mode");
                            backend.roster.request_tab_routed(
                                method,
                                std::path::Path::new(&backend.current_settings()?.game_root),
                                tab,
                                &backend.session,
                            )
                        })
                    })
                    .with_player_save({
                        let backend = backend.clone();
                        Arc::new(move |tab, input| {
                            ensure!(!smoke, "Native player saves are refused in smoke mode");
                            backend.roster.save_player(
                                &backend.root,
                                std::path::Path::new(&backend.current_settings()?.game_root),
                                tab,
                                input,
                            )
                        })
                    })
                    .with_team_save({
                        let backend = backend.clone();
                        Arc::new(move |tab, input| {
                            ensure!(!smoke, "Native team saves are refused in smoke mode");
                            backend.roster.save_team(
                                &backend.root,
                                std::path::Path::new(&backend.current_settings()?.game_root),
                                tab,
                                input,
                            )
                        })
                    })
                    .with_uniform_save({
                        let backend = backend.clone();
                        Arc::new(move |tab, input| {
                            ensure!(!smoke, "Native uniform saves are refused in smoke mode");
                            backend.roster.save_uniform(
                                &backend.root,
                                std::path::Path::new(&backend.current_settings()?.game_root),
                                tab,
                                input,
                            )
                        })
                    })
                    .with_player_dna({
                        let backend = backend.clone();
                        let files = files.clone();
                        let app_handle = app.handle().clone();
                        Arc::new(move |tab, request| {
                            ensure!(
                                !smoke,
                                "Native player snapshot copying is refused in smoke mode"
                            );
                            if matches!(request["action"].as_str(), Some("import" | "export")) {
                                files.transfer(request, |choice| files.ask(&app_handle, choice))
                            } else {
                                backend.roster.player_dna(tab, request)
                            }
                        })
                    })
                    .with_team_dna({
                        let files = files.clone();
                        let app_handle = app.handle().clone();
                        Arc::new(move |_tab, request| {
                            ensure!(!smoke, "Team DNA file transfer is refused in smoke mode");
                            files.transfer_team(request, |choice| files.ask(&app_handle, choice))
                        })
                    })
                    .with_presets({
                        let backend = backend.clone();
                        Arc::new(move |request| backend.presets.request(request))
                    })
                    .with_online_ratings({
                        let service=online_ratings.clone();
                        let backend=backend.clone();
                        let app_handle=app.handle().clone();
                        Arc::new(move |tab,input| {
                            ensure!(!smoke,"Online capture is refused in smoke mode");
                            if input["action"]!="cancel" {
                                let destination=&input["destination"];
                                let index=destination["recordIndex"].as_u64().context("Destination player missing")?;
                                let key=serde_json::to_string(&json!([destination["firstName"],destination["lastName"],destination["cyberfaceId"]]))?;
                                ensure!(backend.roster.live_player(tab,index,&key).is_some(),"The destination player changed. Reopen the player before importing.");
                            }
                            let owner=app_handle.get_webview_window("main").context("Launcher is closed")?.hwnd()?.0 as usize;
                            service.request(&backend.root,owner,tab,input)
                        })
                    },{
                        let service=online_ratings.clone();Arc::new(move ||service.cancel())
                    })
                    .with_animation({
                        // WPF animation:* (player-animation-preview.js); reads the
                        // selected game folder only, cache under <root>/Cache.
                        let settings = backend.clone();
                        animation_preview::Service::new(
                            &backend.root,
                            Arc::new(move || {
                                ensure!(!smoke, "Animation preview game reads are refused in smoke mode");
                                Ok(std::path::PathBuf::from(&settings.current_settings()?.game_root))
                            }),
                        )
                    })
                    .with_editor({
                        let backend = backend.clone();
                        let files = files.clone();
                        let app_handle = app.handle().clone();
                        let artwork = Arc::new(roster_artwork::Service::new(&backend.root));
                        Arc::new(move |method, tab, input| {
                            ensure!(!smoke, "Native editor services are refused in smoke mode");
                            if method == "previewPlayerAudio" {
                                // WPF player-research:preview-audio (stock files only).
                                let game = std::path::PathBuf::from(&backend.current_settings()?.game_root);
                                ensure!(!game.as_os_str().is_empty(), "Select the NBA 2K27 folder first.");
                                return player_audio_preview::preview(&game, input);
                            }
                            if method == "playerEditorBadgeIcons" {
                                // WPF player-research:badge-icons: {} when unavailable.
                                let labels: Vec<String> = input["labels"]
                                    .as_array()
                                    .filter(|l| l.len() <= 64)
                                    .context("Invalid badge icon request.")?
                                    .iter()
                                    .filter_map(|l| l.as_str().filter(|s| !s.is_empty() && s.len() <= 60).map(str::to_owned))
                                    .collect::<std::collections::BTreeSet<_>>()
                                    .into_iter()
                                    .collect();
                                let game = std::path::PathBuf::from(&backend.current_settings()?.game_root);
                                return Ok(Value::Object(artwork.badges(&game, &labels).unwrap_or_default()));
                            }
                            if method == "playerEditorArtwork" {
                                // WPF player-research:artwork (game art + manual library).
                                let kind = input["kind"].as_str().filter(|k| matches!(*k, "portrait" | "logo")).context("Invalid artwork request.")?;
                                let key = input["key"].as_str().filter(|k| !k.is_empty() && k.len() <= 256).context("Invalid artwork request.")?;
                                if input["choose"] == true || input["action"].is_string() {
                                    if input["action"] != "cancel" {
                                        let destination = &input["destination"];
                                        let index = destination["recordIndex"].as_u64().context("Missing artwork destination")?;
                                        let player_key = serde_json::to_string(&json!([destination["firstName"], destination["lastName"], destination["cyberfaceId"]]))?;
                                        let player = backend.roster.live_player(tab, index, &player_key).context("Artwork player changed. Reopen the editor.")?;
                                        let authorized_key = if kind == "portrait" { player_key } else {
                                            player["teamName"].as_str().context("Player team is unavailable")?.to_owned()
                                        };
                                        ensure!(key == authorized_key, "Artwork destination changed. Reopen the editor.");
                                    }
                                    return files.artwork(&backend.root, tab, input, |choice| files.ask(&app_handle, choice));
                                }
                                if let Some(manual) = artwork.manual(kind, key)? {
                                    return Ok(json!(manual));
                                }
                                let game = std::path::PathBuf::from(&backend.current_settings()?.game_root);
                                if input["preview"] == true {
                                    let asset = if kind == "portrait" { json!(4090) } else { json!("logo000") };
                                    let k = if kind == "logo" { "background" } else { kind };
                                    return Ok(json!(artwork.artwork(&game, k, &asset)?));
                                }
                                let (Some(index), Some(player_key)) = (input["recordIndex"].as_u64(), input["playerKey"].as_str()) else {
                                    return Ok(Value::Null);
                                };
                                let Some(player) = backend.roster.live_player(tab, index, player_key) else {
                                    return Ok(Value::Null);
                                };
                                let asset = if kind == "portrait" {
                                    match roster_artwork::photo_id(&player) {
                                        Some(id) => json!(id),
                                        None => return Ok(Value::Null),
                                    }
                                } else {
                                    let view = backend.roster.tab_view(tab)?;
                                    let team = view["teams"].as_array().and_then(|t| {
                                        t.iter().find(|t| t["recordIndex"] == player["teamIndex"] && !player["teamIndex"].is_null())
                                    });
                                    match team.and_then(roster_artwork::logo_asset) {
                                        Some(a) => json!(a),
                                        None => return Ok(Value::Null),
                                    }
                                };
                                let k = if kind == "logo" { "background" } else { kind };
                                return Ok(json!(artwork.artwork(&game, k, &asset)?));
                            }
                            if method == "basketballReferenceStats" {
                                // WPF player:reference-stats: the user typed the URL and pressed Load.
                                return backend.dispatch(
                                    "reference-stats-load",
                                    json!({"url":input.as_str().context("Enter a Basketball-Reference player URL")?,"acknowledgement":"fetch-public-player-page"}),
                                );
                            }
                            if method == "playerEditorHeaderLabels" {
                                // WPF player-research:header-labels: null on any mismatch or failure.
                                let (Some(index), Some(key)) =
                                    (input["recordIndex"].as_u64(), input["playerKey"].as_str())
                                else {
                                    return Ok(Value::Null);
                                };
                                if key.len() > 256 {
                                    return Ok(Value::Null);
                                }
                                let Some(player) = backend.roster.live_player(tab, index, key) else {
                                    return Ok(Value::Null);
                                };
                                let Some(archetype) = player["archetypeKey"].as_str() else {
                                    return Ok(Value::Null);
                                };
                                let game = std::path::PathBuf::from(&backend.current_settings()?.game_root);
                                return Ok(match player_header_labels::label(&game, archetype) {
                                    Ok(Some(name)) => json!({"archetypeName":name}),
                                    _ => Value::Null,
                                });
                            }
                            if method == "exportPlayerResearch" {
                                let catalog = backend.roster.catalog(tab)?;
                                return files.export_catalog(&catalog, |choice| files.ask(&app_handle, choice));
                            }
                            if method == "linkTeamUniform" {
                                return backend.roster.link_uniform(&backend.root, tab, input);
                            }
                            if method == "cloneArena" {
                                let edit = roster_v2_arena_clone::Edit::parse(input)?;
                                let settings = backend.current_settings()?;
                                let mut result = backend.roster.editor(method, &backend.root, tab,
                                    &json!({"edit":edit,"modsRoot":settings.mods_root}), &editor_confirm)?;
                                if result["report"]["readBackVerified"] == true && result["report"]["ok"] == true {
                                    match backend.arena_profiles.record_verified(&backend.root, &edit) {
                                        Ok(profile) => result["savedProfile"] = serde_json::to_value(profile)?,
                                        Err(error) => result["profileSaveError"] = json!(format!("Arena was applied; saving its startup profile failed: {error:#}")),
                                    }
                                }
                                return Ok(result);
                            }
                            if method == "customArenaProfiles" {
                                return backend.arena_profiles.request(&backend.root, input, &editor_confirm);
                            }
                            backend.roster.editor(method, &backend.root, tab, input, &editor_confirm)
                        })
                    }),
                ),
                overlay: Arc::new(native_overlay::Runtime::new(
                    app.handle().clone(),
                    backend.clone(),
                    smoke,
                )?),
                started,
                smoke,
            });
            let decode_fixture = smoke && std::env::args().any(|a| a == "--startup-video-check");
            let builder =
                WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                    .title("2K Launcher")
                    .inner_size(1440., 900.)
                    .min_inner_size(1060., 700.)
                    .center()
                    .visible(!smoke || decode_fixture)
                    .focused(!smoke)
                    .data_directory(profile)
                    .use_https_scheme(true)
                    .devtools(false)
                    .disable_drag_drop_handler()
                    .on_navigation(local)
                    .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
                    .on_permission_request(|_, _| tauri::webview::PermissionResponse::Deny);
            let builder = if smoke {
                if decode_fixture {
                    // Actual frame callbacks require a rendered, unoccluded
                    // WebView. This small silent test window does not activate
                    // or use the fullscreen/game/audio-guardian routes.
                    builder
                        .inner_size(640., 480.)
                        .min_inner_size(640., 480.)
                        .always_on_top(true)
                        .initialization_script("window.__startupVideoDecodeProbe=true;")
                } else {
                    builder.initialization_script("window.__launcherSmoke=true;")
                }
            } else {
                builder
            };
            let main = builder.build()?;
            if !smoke {
                let handle = app.handle().clone();
                app.state::<State>()
                    .overlay
                    .set_auto_enable(Arc::new(move || {
                        let state = handle
                            .try_state::<State>()
                            .context("Launcher state unavailable")?;
                        enable_live_overlay(&handle, &state).map_err(anyhow::Error::msg)?;
                        Ok(())
                    }));
            }
            if decode_fixture {
                main.show()?;
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "live-overlay" && matches!(event, tauri::WindowEvent::Destroyed) {
                if let Some(state) = window.app_handle().try_state::<State>() {
                    let _ = state.overlay.disable();
                }
            }
            if window.label() == "main"
                && matches!(event, tauri::WindowEvent::CloseRequested { .. })
            {
                if let Some(state) = window.app_handle().try_state::<State>() {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        // A modal Win32 prompt pumps window messages. A second
                        // close must not destroy its owner or create another
                        // prompt while the first user decision is outstanding.
                        if CLOSE_PROMPT_ACTIVE.load(Ordering::Acquire) {
                            api.prevent_close();
                            return;
                        }
                        let keep_open = !state.smoke && state.backend.session.should_keep_open();
                        if close_policy::should_prompt(state.smoke, true, keep_open) {
                            let Some(_prompt) = close_policy::Prompt::acquire(&CLOSE_PROMPT_ACTIVE)
                            else {
                                api.prevent_close();
                                return;
                            };
                            if minimize_owned_session_on_close(window) {
                                api.prevent_close();
                                // Do not call begin_shutdown/cancel here:
                                // the exact original parent stays alive.
                                let _ = window.minimize();
                                return;
                            }
                        }
                    }
                    state.backend.begin_shutdown();
                    state.files.cancel();
                    state.backend.presets.cancel();
                    state.backend.jukebox.cancel();
                    state.startup.shutdown();
                }
            }
            if window.label() == "main" && matches!(event, tauri::WindowEvent::Destroyed) {
                if let Some(state) = window.app_handle().try_state::<State>() {
                    state.backend.begin_shutdown();
                    state.backend.roster.cancel();
                    state.files.cancel();
                    state.backend.presets.cancel();
                    state.backend.jukebox.shutdown();
                    state.startup.shutdown();
                    state.overlay.shutdown();
                    state.browser.shutdown();
                    state.backend.session.shutdown();
                }
                window.app_handle().exit(0)
            }
        })
        .build(tauri::generate_context!())
        .context("Could not start Tauri launcher")?
        .run(|app, event| {
            if matches!(
                event,
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
            ) {
                if let Some(state) = app.try_state::<State>() {
                    let cleanup = std::time::Instant::now();
                    state.backend.begin_shutdown();
                    state.backend.roster.cancel();
                    state.files.cancel();
                    state.backend.presets.cancel();
                    state.backend.jukebox.shutdown();
                    state.startup.shutdown();
                    state.overlay.shutdown();
                    state.browser.shutdown();
                    state.backend.session.shutdown();
                    // WPF launcher-health `shutdown` metric (prepare -> finalize),
                    // from the first cleanup pass only.
                    static SHUTDOWN: std::sync::Once = std::sync::Once::new();
                    let elapsed = cleanup.elapsed().as_secs_f64() * 1000.;
                    SHUTDOWN.call_once(|| {
                        let _ = launcher_health::record(
                            &state.backend.root,
                            "shutdown",
                            elapsed,
                            state.smoke,
                        );
                    });
                }
            }
        });
    Ok(())
}
fn main() {
    if let Some(code) = owned_transaction::maybe_run_helper(
        roster_v2_access::execute_owned_transaction,
        roster_v2_access::verify_owned_transaction,
    ) {
        std::process::exit(code);
    }
    if let Err(error) = run() {
        if std::env::args().any(|a| {
            a == "--smoke" || a == "--native-mod-provider" || a == "--jukebox-decode-worker"
        }) {
            eprintln!("{error:#}");
            std::process::exit(1);
        }
        let text: Vec<u16> = format!("{error:#}").encode_utf16().chain(Some(0)).collect();
        unsafe {
            MessageBoxW(
                None,
                PCWSTR(text.as_ptr()),
                w!("2k Launcher Rust Prototype"),
                MB_OK | MB_ICONERROR,
            );
        }
        std::process::exit(1)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlay_authority_excludes_installers_paths_and_unrelated_writes() {
        for label in ["overlay", "live-overlay"] {
            for channel in [
                "mod-install-commit",
                "launcher-update-apply",
                "save-library",
                "open-external",
                "native-launch-modded",
                "unknown",
            ] {
                assert!(!channel_allowed(label, channel, &json!({}), false));
            }
            assert!(!channel_allowed(
                label,
                "save-settings",
                &json!({"gameRoot":"C:/foreign"}),
                false
            ));
            assert!(channel_allowed(
                label,
                "save-settings",
                &json!({"ball":"Current installed"}),
                false
            ));
            assert!(channel_allowed(label, "bootstrap", &Value::Null, false));
        }
        assert!(!channel_allowed(
            "overlay",
            "native-outfit-command",
            &Value::Null,
            false
        ));
        assert!(channel_allowed(
            "live-overlay",
            "native-outfit-command",
            &Value::Null,
            false
        ));
        assert!(!channel_allowed(
            "foreign",
            "bootstrap",
            &Value::Null,
            false
        ));
    }
    #[test]
    fn overlay_preview_permission_is_only_read_only_ball_thumbnail() {
        assert!(preview_window_allowed("main", "previews:thumbnail"));
        assert!(preview_window_allowed("main", "previews:cache-start"));
        assert!(preview_window_allowed(
            "live-overlay",
            "previews:ball-thumbnail"
        ));
        for channel in [
            "previews:thumbnail",
            "previews:cache-start",
            "previews:fit-index",
            "previews:fit-cancel",
            "previews:ball-thumbnail-extra",
            "native-ball-select",
        ] {
            assert!(!preview_window_allowed("live-overlay", channel));
        }
        for label in ["overlay", "startup-intro", "foreign"] {
            assert!(!preview_window_allowed(label, "previews:ball-thumbnail"));
        }
    }
    #[test]
    fn neither_fixture_nor_overlay_can_reach_a_real_launch() {
        assert!(production_launch_window(false, "main").is_ok());
        for (smoke, label) in [
            (true, "main"),
            (true, "overlay"),
            (false, "overlay"),
            (false, "live-overlay"),
            (false, "startup-intro"),
        ] {
            assert!(
                production_launch_window(smoke, label)
                    .unwrap_err()
                    .to_string()
                    .contains("disabled in smoke mode")
            );
        }
    }
    #[test]
    fn app_directory_casing_never_creates_a_nested_portable_root() {
        for dir in ["App", "app", "APP", "aPp"] {
            assert_eq!(
                executable_portable_root(Path::new(&format!(
                    r"C:\Fixture\Portable\{dir}\launcher.exe"
                )))
                .unwrap(),
                PathBuf::from(r"C:\Fixture\Portable")
            );
        }
        assert_eq!(
            executable_portable_root(Path::new(r"C:\Fixture\launcher.exe")).unwrap(),
            PathBuf::from(r"C:\Fixture")
        );
        assert_eq!(
            executable_portable_root(Path::new(r"C:\Fixture\Application\launcher.exe")).unwrap(),
            PathBuf::from(r"C:\Fixture\Application")
        );
    }
    #[test]
    fn navigation_is_local_only() {
        for url in [
            "https://example.com",
            "file:///C:/test.html",
            "http://tauri.localhost",
            "https://tauri.localhost/other.html",
        ] {
            assert!(!local(&tauri::Url::parse(url).unwrap()))
        }
        assert!(local(
            &tauri::Url::parse("https://tauri.localhost/index.html?overlay=1").unwrap()
        ));
        for url in [
            "http://localhost:5174/",
            "http://localhost:5173/other.html",
            "http://user@localhost:5173/",
            "http://localhost.example.com:5173/",
            "https://localhost:5173/",
        ] {
            assert!(!local(&tauri::Url::parse(url).unwrap()));
        }
        assert_eq!(
            local(&tauri::Url::parse("http://localhost:5173/index.html?overlay=1").unwrap()),
            cfg!(all(debug_assertions, not(feature = "custom-protocol")))
        );
    }
}
