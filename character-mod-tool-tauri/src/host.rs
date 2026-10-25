use crate::backend::Backend;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tauri::{Emitter, Manager};
use windows::{
    Win32::System::Com::*,
    Win32::UI::Shell::{Common::COMDLG_FILTERSPEC, *},
    Win32::UI::WindowsAndMessaging::SW_SHOW,
    core::{HSTRING, w},
};

struct AppState {
    backend: Arc<Backend>,
    smoke: Option<PathBuf>,
    started: std::time::Instant,
}

#[tauri::command]
async fn preview_binary(
    state: tauri::State<'_, AppState>,
    command: String,
    payload: Value,
) -> std::result::Result<tauri::ipc::Response, String> {
    let backend = state.backend.clone();
    tauri::async_runtime::spawn_blocking(move || backend.preview_binary(&command, &payload))
        .await
        .map_err(|error| error.to_string())?
        .map(tauri::ipc::Response::new)
        .map_err(|error| format!("{error:#}"))
}

#[tauri::command]
async fn engine_call(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    command: String,
    payload: Value,
) -> std::result::Result<Value, String> {
    if command == "close-app" {
        app.exit(0);
        return Ok(json!(true));
    }
    if command == "smoke-ready" {
        if let Some(path) = &state.smoke {
            let mut value = payload;
            value["startupMilliseconds"] = json!(state.started.elapsed().as_millis());
            value["host"] = json!("Rust/Tauri 2");
            value["nodeBackend"] = json!(false);
            value["dotnetHost"] = json!(false);
            std::fs::write(
                path.with_extension("json"),
                serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            app.exit(0);
        }
        return Ok(json!(true));
    }
    if command == "runtime-info" {
        return Ok(
            json!({"data":state.backend.data,"session":state.backend.session,"host":"Rust/Tauri 2","smoke":state.smoke.is_some(),"previewBinary":true,"previewGeometryBinary":true}),
        );
    }
    let backend = state.backend.clone();
    let owner = app
        .get_webview_window("main")
        .and_then(|window| window.hwnd().ok())
        .map(|handle| handle.0 as isize);
    tauri::async_runtime::spawn_blocking(move || {
        dispatch(&backend, &command, payload, owner, |event| {
            let _ = app.emit("engine-event", event);
        })
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))
}

pub fn run(backend: Arc<Backend>, args: &[String]) -> Result<()> {
    use windows::Win32::{
        Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE},
        System::Threading::CreateMutexW,
    };
    struct Instance(HANDLE);
    impl Drop for Instance {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
    let ui_test = args.first().map(String::as_str) == Some("--ui-test");
    let instance_name = if ui_test {
        HSTRING::from(format!(
            "Local\\CharacterModToolTauri.Test.{}",
            std::process::id()
        ))
    } else {
        HSTRING::from("Local\\CharacterModToolTauri")
    };
    let instance = unsafe { Instance(CreateMutexW(None, false, &instance_name)?) };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        return Ok(());
    }
    let _instance = instance;
    let smoke = if args.first().map(String::as_str) == Some("--smoke") {
        Some(PathBuf::from(args.get(1).context("Missing smoke output")?))
    } else {
        None
    };
    let app = tauri::Builder::default()
        .manage(AppState {
            backend: backend.clone(),
            smoke,
            started: std::time::Instant::now(),
        })
        .invoke_handler(tauri::generate_handler![engine_call, preview_binary])
        .setup(move |app| {
            let state = app.state::<AppState>();
            let mut window = tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title("Character Mod Tool - Tauri Preview")
            .inner_size(1380.0, 900.0)
            .min_inner_size(900.0, 620.0)
            .data_directory(if ui_test {
                state.backend.session.join("WebView2")
            } else {
                state.backend.data.join("WebView2")
            })
            .devtools(cfg!(debug_assertions))
            .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
            .on_download(|_, _| false)
            .on_permission_request(|_, _| tauri::webview::PermissionResponse::Deny)
            .on_navigation(|url| {
                url.host_str() == Some("tauri.localhost")
                    || url.as_str() == "tauri://localhost/"
                    || url.scheme() == "tauri" && url.host_str() == Some("localhost")
            });
            if ui_test {
                window = window.additional_browser_args("--remote-debugging-port=9764");
            }
            let window = window.build()?;
            monitor_webview(&window, state.backend.clone(), ui_test)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                window.state::<AppState>().backend.cancel();
            }
        })
        .build(tauri::generate_context!())?;
    app.run(move |_, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            backend.shutdown();
        }
    });
    Ok(())
}

fn fatal_webview_kind(kind: i32) -> bool {
    use webview2_com::Microsoft::Web::WebView2::Win32::*;
    kind == COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED.0
        || kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED.0
}

fn report_webview_failure(
    app: &tauri::AppHandle,
    backend: &Backend,
    ui_test: bool,
    kind: i32,
    fatal: bool,
    message: &str,
) {
    let logs = backend.data.join("Logs");
    if std::fs::create_dir_all(&logs).is_ok() {
        let _ = std::fs::write(logs.join("webview-failure.json"), json!({
            "kind": kind, "fatal": fatal, "message": message,
            "timestamp": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|time| time.as_secs()).unwrap_or_default()
        }).to_string());
    }
    if !fatal {
        return;
    }
    backend.cancel();
    if !ui_test {
        unsafe {
            let owner = app
                .get_webview_window("main")
                .and_then(|window| window.hwnd().ok());
            windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
                owner,
                &HSTRING::from(message),
                w!("Character Mod Tool"),
                windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
            );
        }
    }
    app.exit(1);
}

fn monitor_webview(
    window: &tauri::WebviewWindow,
    backend: Arc<Backend>,
    ui_test: bool,
) -> Result<()> {
    use webview2_com::{
        Microsoft::Web::WebView2::Win32::COREWEBVIEW2_PROCESS_FAILED_KIND,
        ProcessFailedEventHandler,
    };
    let app = window.app_handle().clone();
    window.with_webview(move |webview| {
        let callback_app = app.clone();
        let callback_backend = backend.clone();
        let fatal_reported = std::sync::atomic::AtomicBool::new(false);
        let installed = (|| -> windows::core::Result<()> {
            let core = unsafe { webview.controller().CoreWebView2()? };
            unsafe {
                let settings = core.Settings()?;
                settings.SetAreDefaultContextMenusEnabled(false)?;
                settings.SetAreDevToolsEnabled(cfg!(debug_assertions))?;
                settings.SetAreHostObjectsAllowed(false)?;
                settings.SetIsStatusBarEnabled(false)?;
                settings.SetIsZoomControlEnabled(false)?;
                if ui_test {
                    let mut menus = windows::core::BOOL::default();
                    let mut devtools = windows::core::BOOL::default();
                    let mut host_objects = windows::core::BOOL::default();
                    let mut status_bar = windows::core::BOOL::default();
                    let mut zoom = windows::core::BOOL::default();
                    let mut messaging = windows::core::BOOL::default();
                    settings.AreDefaultContextMenusEnabled(&mut menus)?;
                    settings.AreDevToolsEnabled(&mut devtools)?;
                    settings.AreHostObjectsAllowed(&mut host_objects)?;
                    settings.IsStatusBarEnabled(&mut status_bar)?;
                    settings.IsZoomControlEnabled(&mut zoom)?;
                    settings.IsWebMessageEnabled(&mut messaging)?;
                    let logs = backend.data.join("Logs");
                    if std::fs::create_dir_all(&logs).is_ok() {
                        let _ = std::fs::write(logs.join("webview-settings.json"), json!({
                            "contextMenus": menus.as_bool(), "devtools": devtools.as_bool(),
                            "hostObjects": host_objects.as_bool(), "statusBar": status_bar.as_bool(),
                            "zoomHotkeys": zoom.as_bool(), "webMessaging": messaging.as_bool()
                        }).to_string());
                    }
                }
            }
            let callback = ProcessFailedEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
                    unsafe { args.ProcessFailedKind(&mut kind)?; }
                    let fatal = fatal_webview_kind(kind.0);
                    if fatal && fatal_reported.swap(true, std::sync::atomic::Ordering::SeqCst) { return Ok(()); }
                    report_webview_failure(&callback_app, &callback_backend, ui_test, kind.0, fatal,
                        "The WebView2 renderer stopped unexpectedly. The app will close so conversion processes can be cleaned up safely.");
                }
                Ok(())
            }));
            let mut token = 0;
            unsafe { core.add_ProcessFailed(&callback, &mut token)?; }
            Ok(())
        })();
        if let Err(error) = installed {
            report_webview_failure(&app, &backend, ui_test, -1, true, &format!("Could not monitor the application renderer: {error}"));
        }
    })?;
    Ok(())
}

fn dispatch(
    backend: &Backend,
    command: &str,
    payload: Value,
    owner: Option<isize>,
    event: impl FnMut(Value),
) -> Result<Value> {
    match command {
        "choose-file" | "choose-folder" => choose(command, &payload, owner),
        "open-path" => {
            let value = payload["path"].as_str().unwrap_or_default();
            if value.is_empty() {
                return Ok(json!(false));
            }
            let path = Path::new(value);
            ensure!(path.exists(), "This path does not exist");
            unsafe {
                let result = ShellExecuteW(
                    None,
                    w!("open"),
                    &HSTRING::from(path.to_string_lossy().as_ref()),
                    None,
                    None,
                    SW_SHOW,
                );
                ensure!(result.0 as isize > 32, "Windows could not open this path");
            }
            Ok(json!(true))
        }
        "show-item" => {
            let value = payload["path"].as_str().unwrap_or_default();
            if value.is_empty() || !Path::new(value).exists() {
                return Ok(json!(false));
            }
            let path = Path::new(value).canonicalize()?;
            let explorer =
                PathBuf::from(std::env::var_os("WINDIR").context("Windows directory unavailable")?)
                    .join("explorer.exe");
            std::process::Command::new(explorer)
                .arg(format!("/select,{}", path.display()))
                .spawn()?;
            Ok(json!(true))
        }
        _ => backend.invoke(command, payload, event),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_browser_and_main_renderer_failure_are_fatal() {
        assert!(fatal_webview_kind(0));
        assert!(fatal_webview_kind(1));
        for kind in 2..=9 {
            assert!(!fatal_webview_kind(kind));
        }
    }
}

fn choose(command: &str, payload: &Value, owner: Option<isize>) -> Result<Value> {
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        let result = (|| -> Result<Value> {
            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
            let mut options =
                dialog.GetOptions()? | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST | FOS_NOCHANGEDIR;
            options |= if command == "choose-folder" {
                FOS_PICKFOLDERS
            } else {
                FOS_FILEMUSTEXIST
            };
            dialog.SetOptions(options)?;
            dialog.SetTitle(&HSTRING::from(
                payload["title"].as_str().unwrap_or("Character Mod Tool"),
            ))?;
            if command == "choose-file" {
                let filter = COMDLG_FILTERSPEC {
                    pszName: w!("Supported files"),
                    pszSpec: if payload["exe"] == true {
                        w!("*.exe")
                    } else if payload["iffOnly"] == true {
                        w!("*.iff")
                    } else {
                        w!("*.iff;*.zip;*.rar")
                    },
                };
                if payload["exe"] == true {
                    dialog.SetFileTypes(&[filter])?;
                } else {
                    dialog.SetFileTypes(&[
                        filter,
                        COMDLG_FILTERSPEC {
                            pszName: w!("All files"),
                            pszSpec: w!("*.*"),
                        },
                    ])?;
                }
            }
            if let Some(current) = payload["current"].as_str() {
                let path = Path::new(current);
                let directory = if path.is_file() {
                    path.parent()
                } else {
                    Some(path)
                };
                if let Some(directory) = directory.filter(|path| path.is_dir()) {
                    if let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(
                        &HSTRING::from(directory.to_string_lossy().as_ref()),
                        None,
                    ) {
                        let _ = dialog.SetFolder(&item);
                    }
                }
            }
            match dialog.Show(
                owner.map(|value| windows::Win32::Foundation::HWND(value as *mut std::ffi::c_void)),
            ) {
                Ok(()) => {
                    let item = dialog.GetResult()?;
                    let path = item.GetDisplayName(SIGDN_FILESYSPATH)?;
                    let value = path.to_string()?;
                    CoTaskMemFree(Some(path.0.cast()));
                    Ok(json!(value))
                }
                Err(error) if error.code().0 as u32 == 0x800704C7 => Ok(Value::Null),
                Err(error) => Err(error.into()),
            }
        })();
        CoUninitialize();
        result
    }
}
