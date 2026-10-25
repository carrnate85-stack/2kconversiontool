use crate::backend::Emit;
use anyhow::{Context, Result, ensure};
use base64::Engine;
use include_dir::{Dir, include_dir};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Instant,
};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use windows::{
    Win32::{
        Foundation::*,
        System::{Com::*, DataExchange::*, Memory::*},
        UI::{Shell::*, WindowsAndMessaging::*},
    },
    core::*,
};

static LICENSES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/Web/licenses");
pub fn license_text() -> String {
    fn append(dir: &Dir<'_>, out: &mut String) {
        for f in dir.files() {
            if let Some(s) = f.contents_utf8() {
                out.push_str(&format!("\n\n===== {} =====\n\n{s}", f.path().display()));
            }
        }
        for d in dir.dirs() {
            append(d, out);
        }
    }
    let mut out="NBA 2K27 Exporter - third-party notices. No game assets or proprietary Oodle DLLs are redistributed.\n".to_string();
    append(&LICENSES, &mut out);
    out
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn folder(hwnd: HWND) -> Result<Option<PathBuf>> {
    unsafe {
        let d: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
        d.SetOptions(FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST | FOS_NOCHANGEDIR)?;
        if let Err(e) = d.Show(Some(hwnd)) {
            if e.code() == HRESULT(0x800704c7u32 as i32) {
                return Ok(None);
            }
            return Err(e.into());
        }
        let p = d.GetResult()?.GetDisplayName(SIGDN_FILESYSPATH)?;
        let value = p.to_string()?;
        CoTaskMemFree(Some(p.0 as _));
        Ok(Some(PathBuf::from(value)))
    }
}
fn save(hwnd: HWND, name: &str) -> Result<Option<PathBuf>> {
    unsafe {
        let d: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER)?;
        d.SetOptions(
            FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST | FOS_NOCHANGEDIR | FOS_OVERWRITEPROMPT,
        )?;
        let name = wide(name);
        d.SetFileName(PCWSTR(name.as_ptr()))?;
        if let Err(e) = d.Show(Some(hwnd)) {
            if e.code() == HRESULT(0x800704c7u32 as i32) {
                return Ok(None);
            }
            return Err(e.into());
        }
        let p = d.GetResult()?.GetDisplayName(SIGDN_FILESYSPATH)?;
        let value = p.to_string()?;
        CoTaskMemFree(Some(p.0 as _));
        Ok(Some(PathBuf::from(value)))
    }
}
fn clipboard(hwnd: HWND, text: &str) -> Result<Value> {
    ensure!(
        text.len() <= 4 * 1024 * 1024,
        "Clipboard text exceeds limit"
    );
    let bytes = wide(text);
    unsafe {
        OpenClipboard(Some(hwnd))?;
        let result = (|| -> Result<()> {
            EmptyClipboard()?;
            let mem = GlobalAlloc(GMEM_MOVEABLE, bytes.len() * 2)?;
            let p = GlobalLock(mem);
            if p.is_null() {
                let _ = GlobalFree(Some(mem));
                anyhow::bail!("Could not allocate clipboard");
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), p as *mut u16, bytes.len());
            let _ = GlobalUnlock(mem);
            if let Err(e) = SetClipboardData(13, Some(HANDLE(mem.0))) {
                let _ = GlobalFree(Some(mem));
                return Err(e.into());
            }
            Ok(())
        })();
        let _ = CloseClipboard();
        result?;
    }
    Ok(json!(true))
}
fn local_url(url: &tauri::Url) -> bool {
    let bundled = url.scheme() == "https"
        && url.host_str() == Some("tauri.localhost")
        && matches!(url.path(), "/" | "/index.html");
    let live = cfg!(all(
        feature = "development",
        not(feature = "custom-protocol")
    )) && url.scheme() == "http"
        && url.host_str() == Some("127.0.0.1")
        && url.port() == Some(1420)
        && matches!(url.path(), "/" | "/index.html");
    bundled || live
}
fn trusted(window: &WebviewWindow) -> Result<()> {
    ensure!(
        window.label() == "main" && local_url(&window.url()?),
        "Only the bundled main window can invoke exporter commands"
    );
    Ok(())
}

struct Job {
    channel: String,
    args: Vec<Value>,
    reply: mpsc::Sender<std::result::Result<Value, String>>,
}
struct State {
    jobs: Mutex<Option<mpsc::SyncSender<Job>>>,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
    cancel: Arc<AtomicBool>,
    closing: Arc<AtomicBool>,
    last: Arc<Mutex<Option<PathBuf>>>,
    started: Instant,
    smoke: Option<PathBuf>,
}
impl State {
    fn stop(&self) {
        self.closing.store(true, Ordering::Relaxed);
        self.cancel.store(true, Ordering::Relaxed);
        self.jobs.lock().unwrap().take();
        if let Some(worker) = self.worker.lock().unwrap().take() {
            let _ = worker.join();
        }
    }
}
// Native dialogs and clipboard are run on the window thread, not a Tokio pool
// thread with an unknown COM apartment. Awaiting never blocks the renderer.
async fn on_ui(
    window: WebviewWindow,
    f: impl FnOnce(HWND) -> Result<Value> + Send + 'static,
) -> std::result::Result<Value, String> {
    let handle = window.hwnd().map_err(|e| e.to_string())?.0 as usize;
    let (tx, rx) = mpsc::channel();
    window
        .run_on_main_thread(move || {
            let _ = tx.send(f(HWND(handle as _)).map_err(|e| format!("{e:#}")));
        })
        .map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || rx.recv().map_err(|e| e.to_string())?)
        .await
        .map_err(|e| e.to_string())?
}
async fn request(
    state: &State,
    channel: String,
    args: Vec<Value>,
) -> std::result::Result<Value, String> {
    let (tx, rx) = mpsc::channel();
    state
        .jobs
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .ok_or("Exporter is shutting down")?
        .try_send(Job {
            channel,
            args,
            reply: tx,
        })
        .map_err(|e| format!("Exporter queue unavailable: {e}"))?;
    tauri::async_runtime::spawn_blocking(move || rx.recv().map_err(|e| e.to_string())?)
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn exporter_invoke(
    window: WebviewWindow,
    state: tauri::State<'_, State>,
    channel: String,
    args: Vec<Value>,
) -> std::result::Result<Value, String> {
    trusted(&window).map_err(|e| e.to_string())?;
    if state.closing.load(Ordering::Relaxed) {
        return Err("Exporter is shutting down".into());
    }
    if channel.len() > 64
        || args.len() > 8
        || serde_json::to_vec(&args).map_err(|e| e.to_string())?.len()
            > crate::worker::MAX_REQUEST_ARGS
    {
        return Err("Request exceeds limits".into());
    }
    match channel.as_str() {
        "cancel" => {
            state.cancel.store(true, Ordering::Relaxed);
            return Ok(json!(true));
        }
        "game:choose-root" => return on_ui(window, |hwnd| Ok(json!(folder(hwnd)?))).await,
        "export:choose-root" => {
            let chosen = on_ui(window, |hwnd| Ok(json!(folder(hwnd)?))).await?;
            if chosen.is_null() {
                return Ok(chosen);
            }
            return request(&state, "export:set-root".into(), vec![chosen]).await;
        }
        "clipboard:copy-text" => {
            let text = args
                .first()
                .and_then(Value::as_str)
                .ok_or("Missing clipboard text")?
                .to_string();
            return on_ui(window, move |hwnd| clipboard(hwnd, &text)).await;
        }
        "diagnostics:copy" => {
            let text = serde_json::to_string_pretty(&crate::diagnostics::sanitized(
                args.first().unwrap_or(&Value::Null),
            ))
            .map_err(|e| e.to_string())?;
            return on_ui(window, move |hwnd| clipboard(hwnd, &text)).await;
        }
        "shell:show-export" => {
            let path = state
                .last
                .lock()
                .map_err(|e| e.to_string())?
                .clone()
                .ok_or("No finished export folder")?;
            return on_ui(window, move |hwnd| {
                ensure!(path.is_dir(), "Export folder no longer exists");
                let path = wide(&path.to_string_lossy());
                let r = unsafe {
                    ShellExecuteW(
                        Some(hwnd),
                        w!("open"),
                        PCWSTR(path.as_ptr()),
                        PCWSTR::null(),
                        PCWSTR::null(),
                        SW_SHOWNORMAL,
                    )
                };
                ensure!(r.0 as isize > 32, "Windows could not open export folder");
                Ok(json!(true))
            })
            .await;
        }
        _ if renderer_worker_channel(&channel) => {}
        _ => return Err(format!("Unsupported exporter command: {channel}")),
    }
    let bootstrap = channel == "app:bootstrap";
    let mut result = request(&state, channel, args).await?;
    if bootstrap {
        result["runtime"] = json!("Native desktop + shared WebView2; no .NET / Node / Electron");
        result["nativeStartupMs"] = json!(state.started.elapsed().as_secs_f64() * 1000.);
        result["host"] = json!("tauri2");
        result["buildChannel"] = json!(crate::build_channel::NAME);
        result["windowTitle"] = json!(crate::build_channel::TITLE);
        result["settingsProfile"] = json!(crate::build_channel::profile(
            window
                .app_handle()
                .path()
                .app_local_data_dir()
                .map_err(|e| e.to_string())?
        ));
        if crate::build_channel::DEVELOPMENT {
            result["defaultOutputRoot"] = json!(
                window
                    .app_handle()
                    .path()
                    .app_local_data_dir()
                    .map_err(|e| e.to_string())?
                    .join("Development/Exports")
            );
        }
    }
    if result["nativeSave"] == true {
        if result["nativeMember"] == true || result["nativeTexture"] == true {
            let publish_channel = if result["nativeTexture"] == true {
                "preview:publish-texture"
            } else {
                "file:publish-member"
            };
            let name = result["name"]
                .as_str()
                .context("Save name missing")
                .map_err(|e| e.to_string())?
                .rsplit('/')
                .next()
                .unwrap_or("member.bin")
                .to_string();
            let chosen = on_ui(window, move |hwnd| Ok(json!(save(hwnd, &name)?))).await?;
            if chosen.is_null() {
                return Ok(json!({"cancelled":true}));
            }
            result["target"] = chosen;
            return request(&state, publish_channel.into(), vec![result]).await;
        }
        return Err("Unsupported Save metadata; reopen the source member or texture".into());
    }
    Ok(result)
}
#[tauri::command]
fn smoke_result(
    window: WebviewWindow,
    state: tauri::State<'_, State>,
    result: Value,
) -> std::result::Result<(), String> {
    trusted(&window).map_err(|e| e.to_string())?;
    let path = state
        .smoke
        .as_ref()
        .ok_or("Diagnostics are disabled in normal launches")?;
    let mut result = result;
    if let Some(captures) = result["captures"].as_array() {
        for capture in captures {
            if let (Some(name), Some(data)) = (
                capture["name"].as_str(),
                capture["png"]
                    .as_str()
                    .and_then(|s| s.strip_prefix("data:image/png;base64,")),
            ) {
                if !name.is_empty()
                    && name.len() <= 80
                    && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
                    && data.len() <= 16 * 1024 * 1024
                {
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(data)
                        .map_err(|e| e.to_string())?;
                    std::fs::write(
                        path.parent()
                            .unwrap_or(Path::new("."))
                            .join(format!("{name}.png")),
                        bytes,
                    )
                    .map_err(|e| e.to_string())?;
                }
            }
        }
    }
    if let Some(map) = result.as_object_mut() {
        map.remove("captures");
    }
    let title = window.title().map_err(|e| e.to_string())?;
    if title != crate::build_channel::TITLE {
        result["pass"] = json!(false);
        result["error"] = json!("Unexpected application title");
    }
    let report = json!({"host":"Tauri 2","windowTitle":title,"workflowMs":state.started.elapsed().as_secs_f64()*1000.,"result":result});
    std::fs::write(
        path,
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let capture_path = path.parent().unwrap_or(Path::new(".")).join("explorer.png");
    let close_window = window.clone();
    window.with_webview(move|webview|{
        let result=(||->windows::core::Result<()>{unsafe{
            let stream=SHCreateMemStream(None).ok_or_else(||windows::core::Error::from(E_FAIL))?;
            let captured=stream.clone();let done_window=close_window.clone();
            let callback=webview2_com::CapturePreviewCompletedHandler::create(Box::new(move|code|{
                let result=(||->windows::core::Result<()>{code?;
                    let mut stat=STATSTG::default();captured.Stat(&mut stat,STATFLAG_NONAME)?;
                    if stat.cbSize>32*1024*1024{return Err(windows::core::Error::from(E_FAIL));}
                    captured.Seek(0,STREAM_SEEK_SET,None)?;let mut bytes=vec![0;stat.cbSize as usize];let mut count=0;
                    captured.Read(bytes.as_mut_ptr() as _,bytes.len() as u32,Some(&mut count)).ok()?;
                    if count!=bytes.len() as u32{return Err(windows::core::Error::from(E_FAIL));}
                    std::fs::write(&capture_path,bytes).map_err(|e|windows::core::Error::new(E_FAIL,e.to_string()))?;Ok(())})();
                let _=done_window.close();result
            }));
            webview.controller().CoreWebView2()?.CapturePreview(webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG,&stream,&callback)?;
            Ok(())
        }})();
        if result.is_err(){let _=close_window.close();}
    }).map_err(|e|e.to_string())?;
    Ok(())
}
pub fn run(smoke: Option<PathBuf>) -> Result<()> {
    let started = Instant::now();
    let app = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![exporter_invoke, smoke_result])
        .setup(move |app| {
            let profile = if let Some(path) = smoke.as_ref() {
                path.parent()
                    .context("Smoke report parent missing")?
                    .join("webview-profile")
            } else {
                crate::build_channel::profile(app.path().app_local_data_dir()?)
            };
            std::fs::create_dir_all(&profile)?;
            let cancel = Arc::new(AtomicBool::new(false));
            let closing = Arc::new(AtomicBool::new(false));
            let last = Arc::new(Mutex::new(None));
            let (tx, rx) = mpsc::sync_channel::<Job>(32);
            let app_handle = app.handle().clone();
            let emit: Emit = Arc::new(move |event| {
                let _ = app_handle.emit_to("main", "exporter-progress", &event["payload"]);
            });
            let worker_cancel = cancel.clone();
            let worker_closing = closing.clone();
            let worker_last = last.clone();
            let worker = std::thread::spawn(move || {
                let mut backend =
                    crate::worker::Session::new(emit, worker_cancel, worker_closing.clone());
                while let Ok(job) = rx.recv() {
                    if worker_closing.load(Ordering::Relaxed) {
                        let _ = job.reply.send(Err("Exporter is shutting down".into()));
                        continue;
                    }
                    let result = backend
                        .dispatch(&job.channel, &job.args)
                        .map_err(|e| format!("{e:#}"));
                    if let Ok(value) = result.as_ref() {
                        if let Some(folder) = successful_export_folder(value) {
                            *worker_last.lock().unwrap() = Some(PathBuf::from(folder));
                        }
                    }
                    let _ = job.reply.send(result);
                }
                backend.stop();
            });
            app.manage(State {
                jobs: Mutex::new(Some(tx)),
                worker: Mutex::new(Some(worker)),
                cancel,
                closing,
                last,
                started,
                smoke: smoke.clone(),
            });
            let builder =
                WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                    .title(crate::build_channel::TITLE)
                    .inner_size(
                        if smoke.is_some() && std::env::args().any(|a| a == "--compact-smoke") {
                            1060.
                        } else {
                            1440.
                        },
                        if smoke.is_some() && std::env::args().any(|a| a == "--compact-smoke") {
                            760.
                        } else {
                            920.
                        },
                    )
                    .min_inner_size(960., 640.)
                    .visible(smoke.is_none())
                    .focused(smoke.is_none())
                    .data_directory(profile)
                    .use_https_scheme(true)
                    .devtools(cfg!(all(
                        feature = "development",
                        not(feature = "custom-protocol")
                    )))
                    .disable_drag_drop_handler()
                    .on_navigation(local_url)
                    .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
                    .on_permission_request(|_, _| tauri::webview::PermissionResponse::Deny);
            let builder = if smoke.is_some() {
                builder.initialization_script(if std::env::args().any(|a| a == "--compact-smoke") {
                    "window.__rustSmoke=true;window.__rustCompactSmoke=true;"
                } else if std::env::args().any(|a| a == "--extended-families") {
                    "window.__rustSmoke=true;window.__rustExtendedFamilies=true;"
                } else {
                    "window.__rustSmoke=true;"
                })
            } else {
                builder
            };
            builder.build()?;
            Ok(())
        })
        .build(tauri::generate_context!())?;
    app.run(|handle, event| match event {
        tauri::RunEvent::ExitRequested { .. } => {
            let state = handle.state::<State>();
            state.closing.store(true, Ordering::Relaxed);
            state.cancel.store(true, Ordering::Relaxed);
        }
        tauri::RunEvent::Exit => handle.state::<State>().stop(),
        _ => {}
    });
    Ok(())
}
fn renderer_worker_channel(channel: &str) -> bool {
    matches!(
        channel,
        "app:bootstrap"
            | "app:licenses"
            | "cache:status"
            | "cache:clear"
            | "diagnostics:prepare"
            | "game:load"
            | "catalog:search"
            | "catalog:folders"
            | "catalog:inspect"
            | "catalog:resolve-resource"
            | "file:inspect"
            | "file:preview-member"
            | "file:save-member"
            | "preview:load"
            | "preview:load-loose"
            | "preview:textures"
            | "preview:material-textures"
            | "preview:save-texture"
            | "preview:change-design"
            | "export:set-root"
            | "export:start"
            | "export:textures"
            | "file:close"
    )
}
fn successful_export_folder(value: &Value) -> Option<&str> {
    value["results"]
        .as_array()
        .filter(|results| !results.is_empty())?;
    value["folder"].as_str()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn publish_commands_are_host_only_but_save_requests_remain_available() {
        assert!(!renderer_worker_channel("file:publish-member"));
        assert!(!renderer_worker_channel("preview:publish-texture"));
        assert!(renderer_worker_channel("file:save-member"));
        assert!(renderer_worker_channel("preview:save-texture"));
        // Retained for existing preference restoration. Removing it requires
        // a native settings migration, not silently dropping saved folders.
        assert!(renderer_worker_channel("export:set-root"));
    }
    #[test]
    fn failed_or_empty_job_does_not_replace_last_export_folder() {
        assert_eq!(
            successful_export_folder(&serde_json::json!({"folder":"empty","results":[]})),
            None
        );
        assert_eq!(
            successful_export_folder(
                &serde_json::json!({"folder":"saved","results":[{"target":"saved/model.iff"}]})
            ),
            Some("saved")
        );
    }
    #[test]
    fn only_bundled_navigation_allowed() {
        for url in [
            "https://example.org",
            "http://tauri.localhost/index.html",
            "file:///C:/test.html",
            "https://tauri.localhost/other.html",
        ] {
            assert!(!local_url(&tauri::Url::parse(url).unwrap()));
        }
        assert!(local_url(
            &tauri::Url::parse("https://tauri.localhost/index.html").unwrap()
        ));
        assert_eq!(
            local_url(&tauri::Url::parse("http://127.0.0.1:1420/").unwrap()),
            cfg!(all(
                feature = "development",
                not(feature = "custom-protocol")
            ))
        );
        for url in [
            "http://127.0.0.1:1421/",
            "http://localhost:1420/",
            "http://127.0.0.1:1420/other",
            "https://127.0.0.1:1420/",
        ] {
            assert!(!local_url(&tauri::Url::parse(url).unwrap()));
        }
    }
}
