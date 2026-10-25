use crate::backend::Backend;
use anyhow::{Context, Result, ensure};
use base64::Engine;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
};
use webview2_com::{Microsoft::Web::WebView2::Win32::*, *};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        System::{Com::*, LibraryLoader::GetModuleHandleW},
        UI::{HiDpi::*, Shell::*, WindowsAndMessaging::*},
    },
    core::{HSTRING, PCWSTR, PWSTR, w},
};

const WAKE: u32 = WM_APP + 1;
struct WindowState {
    controller: Option<ICoreWebView2Controller>,
}
#[derive(Clone)]
struct Sender {
    tx: mpsc::Sender<Value>,
    window: usize,
}
impl Sender {
    fn send(&self, value: Value) {
        if self.tx.send(value).is_ok() {
            unsafe {
                let _ = PostMessageW(
                    Some(HWND(self.window as *mut _)),
                    WAKE,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        }
    }
}
#[derive(Deserialize)]
struct Request {
    id: u64,
    command: String,
    #[serde(default)]
    payload: Value,
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match message {
            WM_SIZE => {
                let pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut WindowState;
                if let Some(state) = pointer.as_ref() {
                    if let Some(controller) = &state.controller {
                        let mut bounds = RECT::default();
                        let _ = GetClientRect(window, &mut bounds);
                        let _ = controller.SetBounds(bounds);
                    }
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = DestroyWindow(window);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(window, message, wparam, lparam),
        }
    }
}

fn wide(value: &str) -> HSTRING {
    HSTRING::from(value)
}

fn trace(path: &Option<PathBuf>, message: &str) {
    use std::io::Write;
    if let Some(path) = path {
        if let Ok(mut file) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path.with_extension("trace"))
        {
            let _ = writeln!(file, "{message}");
        }
    }
}

pub fn run(backend: Arc<Backend>, args: &[String]) -> Result<()> {
    let started = std::time::Instant::now();
    let smoke = if args.first().map(String::as_str) == Some("--smoke") {
        Some(PathBuf::from(
            args.get(1).context("--smoke needs an output prefix")?,
        ))
    } else {
        None
    };
    trace(&smoke, "Creating native window");
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        let _ = SetProcessDpiAwareness(PROCESS_PER_MONITOR_DPI_AWARE);
    }
    let module = unsafe { GetModuleHandleW(None)? };
    let class = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: HINSTANCE(module.0),
        lpszClassName: w!("CharacterModToolRustPrototype"),
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW)? },
        hIcon: unsafe { LoadIconW(Some(HINSTANCE(module.0)), PCWSTR(1_usize as *const u16))? },
        ..Default::default()
    };
    let mut window_state = Box::new(WindowState { controller: None });
    let window = unsafe {
        RegisterClassW(&class);
        let window = CreateWindowExW(
            Default::default(),
            class.lpszClassName,
            w!("Character Mod Tool - Rust Prototype"),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1380,
            900,
            None,
            None,
            Some(HINSTANCE(module.0)),
            None,
        )?;
        SetWindowLongPtrW(window, GWLP_USERDATA, &mut *window_state as *mut _ as isize);
        let _ = ShowWindow(window, SW_SHOW);
        window
    };
    let profile = wide(&backend.data.join("WebView2").to_string_lossy());
    let (env_tx, env_rx) = mpsc::channel();
    CreateCoreWebView2EnvironmentCompletedHandler::wait_for_async_operation(
        Box::new(move |handler| unsafe {
            CreateCoreWebView2EnvironmentWithOptions(None, &profile, None, &handler)
                .map_err(webview2_com::Error::WindowsError)
        }),
        Box::new(move |error, environment| {
            error?;
            let _ = env_tx.send(environment);
            Ok(())
        }),
    )?;
    let environment = env_rx
        .recv()?
        .context("WebView2 could not initialize; install Microsoft Edge WebView2 Runtime")?;
    trace(&smoke, "WebView2 environment ready");
    let (controller_tx, controller_rx) = mpsc::channel();
    CreateCoreWebView2ControllerCompletedHandler::wait_for_async_operation(
        Box::new(move |handler| unsafe {
            environment
                .CreateCoreWebView2Controller(window, &handler)
                .map_err(webview2_com::Error::WindowsError)
        }),
        Box::new(move |error, controller| {
            error?;
            let _ = controller_tx.send(controller);
            Ok(())
        }),
    )?;
    let controller = controller_rx
        .recv()?
        .context("WebView2 did not create its controller")?;
    let view = unsafe { controller.CoreWebView2()? };
    trace(&smoke, "WebView2 controller ready");
    unsafe {
        let mut bounds = RECT::default();
        GetClientRect(window, &mut bounds)?;
        controller.SetBounds(bounds)?;
        controller.SetIsVisible(true)?;
        let settings = view.Settings()?;
        settings.SetAreDevToolsEnabled(cfg!(debug_assertions))?;
        settings.SetAreDefaultContextMenusEnabled(false)?;
        settings.SetIsStatusBarEnabled(false)?;
    }
    window_state.controller = Some(controller.clone());
    let (tx, rx) = mpsc::channel();
    let sender = Sender {
        tx,
        window: window.0 as usize,
    };
    let inflight = Arc::new(AtomicUsize::new(0));
    let smoke_enabled = smoke.is_some();
    let service = backend.clone();
    let results = sender.clone();
    let navigation_trace = smoke.clone();
    let message_trace = smoke.clone();
    let html = include_str!("../web/index.html")
        .replace("/*__ICONS__*/", include_str!("../web/lucide.min.js"))
        .replace("/*__APP__*/", include_str!("../web/app.js"))
        .replace("/*__STYLE__*/", include_str!("../web/style.css"))
        .replace(
            "/*__SMOKE__*/false",
            if smoke_enabled { "true" } else { "false" },
        );
    let trusted_uri = format!(
        "data:text/html;charset=utf-8;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(html.as_bytes())
    );
    let navigation_uri = trusted_uri.clone();
    let mut token = 0;
    unsafe {
        view.add_NavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let mut uri = PWSTR::null();
                    args.Uri(&mut uri)?;
                    let uri = CoTaskMemPWSTR::from(uri).to_string();
                    trace(
                        &navigation_trace,
                        &format!("Navigation: {}", uri.chars().take(100).collect::<String>()),
                    );
                    if uri != "about:blank" && uri != navigation_uri {
                        args.SetCancel(true)?;
                    }
                }
                Ok(())
            })),
            &mut token,
        )?;
        view.add_NewWindowRequested(
            &NewWindowRequestedEventHandler::create(Box::new(|_, args| {
                if let Some(args) = args {
                    args.SetHandled(true)?;
                }
                Ok(())
            })),
            &mut token,
        )?;
        view.add_PermissionRequested(
            &PermissionRequestedEventHandler::create(Box::new(|_, args| {
                if let Some(args) = args {
                    args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY)?;
                }
                Ok(())
            })),
            &mut token,
        )?;
        view.add_WebMessageReceived(&WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()); };
            let mut source = PWSTR::null(); args.Source(&mut source)?;
            let source = CoTaskMemPWSTR::from(source).to_string();
            trace(&message_trace, &format!("Message source: {}", source.chars().take(100).collect::<String>()));
            if source != "about:blank" && source != trusted_uri { return Ok(()); }
            let mut raw = PWSTR::null(); args.WebMessageAsJson(&mut raw)?;
            let raw = CoTaskMemPWSTR::from(raw).to_string();
            if raw.len() > 2 * 1024 * 1024 { return Ok(()); }
            let Ok(request) = serde_json::from_str::<Request>(&raw) else { return Ok(()); };
            if request.command == "smoke-ready" && smoke_enabled {
                results.send(json!({"kind":"smoke","value":request.payload})); return Ok(());
            }
            if inflight.fetch_add(1, Ordering::SeqCst) >= 4 {
                inflight.fetch_sub(1, Ordering::SeqCst);
                results.send(json!({"kind":"reply","id":request.id,"error":"Too many pending operations"})); return Ok(());
            }
            let results = results.clone(); let service = service.clone(); let inflight = inflight.clone();
            std::thread::spawn(move || {
                let result = dispatch(&service, &request, |event| results.send(event));
                match result {
                    Ok(value) => results.send(json!({"kind":"reply","id":request.id,"value":value})),
                    Err(error) => results.send(json!({"kind":"reply","id":request.id,"error":error.to_string()})),
                }
                inflight.fetch_sub(1, Ordering::SeqCst);
            });
            Ok(())
        })), &mut token)?;
        view.NavigateToString(&wide(&html))?;
        trace(&smoke, "Embedded UI requested");
    }
    let mut message = MSG::default();
    unsafe {
        while GetMessageW(&mut message, None, 0, 0).0 > 0 {
            if message.message == WAKE {
                for row in rx.try_iter() {
                    if row["kind"] == "smoke" {
                        if let Some(path) = &smoke {
                            let mut value = row["value"].clone();
                            value["startupMilliseconds"] = json!(started.elapsed().as_millis());
                            value["host"] = json!("Rust/WebView2");
                            value["nodeBackend"] = json!(false);
                            value["dotnetHost"] = json!(false);
                            fs::write(
                                path.with_extension("json"),
                                serde_json::to_vec_pretty(&value)?,
                            )?;
                            let _ = PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0));
                        }
                    } else {
                        view.PostWebMessageAsJson(&wide(&row.to_string()))?;
                    }
                }
            } else {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        backend.shutdown();
        controller.Close()?;
        SetWindowLongPtrW(window, GWLP_USERDATA, 0);
        CoUninitialize();
    }
    Ok(())
}

fn dispatch(backend: &Backend, request: &Request, event: impl FnMut(Value)) -> Result<Value> {
    match request.command.as_str() {
        "choose-file" | "choose-folder" => choose(&request.command, &request.payload),
        "open-folder" => {
            let path = Path::new(request.payload["path"].as_str().context("Missing folder")?);
            ensure!(path.is_dir(), "This folder does not exist");
            unsafe {
                let result = ShellExecuteW(
                    None,
                    w!("open"),
                    &wide(&path.to_string_lossy()),
                    None,
                    None,
                    SW_SHOW,
                );
                ensure!(result.0 as isize > 32, "Windows could not open this folder");
            }
            Ok(json!(true))
        }
        "runtime-info" => Ok(
            json!({"data":backend.data,"session":backend.session,"host":"Rust/WebView2","nodeBackend":false,"dotnetHost":false}),
        ),
        _ => backend.invoke(&request.command, request.payload.clone(), event),
    }
}

fn choose(command: &str, payload: &Value) -> Result<Value> {
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        let result = (|| -> Result<Value> {
            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
            let mut options =
                dialog.GetOptions()? | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST | FOS_NOCHANGEDIR;
            if command == "choose-folder" {
                options |= FOS_PICKFOLDERS;
            } else {
                options |= FOS_FILEMUSTEXIST;
            }
            dialog.SetOptions(options)?;
            dialog.SetTitle(&wide(
                payload["title"].as_str().unwrap_or("Character Mod Tool"),
            ))?;
            if command == "choose-file" {
                let spec = if payload["exe"] == true {
                    w!("*.exe")
                } else {
                    w!("*.iff;*.zip;*.rar")
                };
                dialog.SetFileTypes(&[COMDLG_FILTERSPEC {
                    pszName: w!("Supported files"),
                    pszSpec: spec,
                }])?;
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
                        &wide(&directory.to_string_lossy()),
                        None,
                    ) {
                        let _ = dialog.SetFolder(&item);
                    }
                }
            }
            match dialog.Show(None) {
                Ok(()) => {
                    let item = dialog.GetResult()?;
                    let path = item.GetDisplayName(SIGDN_FILESYSPATH)?;
                    Ok(json!(CoTaskMemPWSTR::from(path).to_string()))
                }
                Err(error) if error.code().0 as u32 == 0x800704C7 => Ok(Value::Null),
                Err(error) => Err(error.into()),
            }
        })();
        CoUninitialize();
        result
    }
}
