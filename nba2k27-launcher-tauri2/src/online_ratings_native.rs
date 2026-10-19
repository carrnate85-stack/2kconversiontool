//! Raw WebView2 STA host: no Tauri initialization, IPC, host objects, or web messages.
use super::{Address, LIFETIME, MAX_TEXT, Profile};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use webview2_com::{Microsoft::Web::WebView2::Win32::*, *};
use windows::{
    Win32::{
        Foundation::{HWND, RECT},
        System::{
            Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize},
            Threading::GetCurrentProcessId,
        },
        UI::WindowsAndMessaging::*,
    },
    core::{HRESULT, Interface, PCWSTR, PWSTR, implement, w},
};
const SCRIPT: &str = r#"(()=>{let out='',size=0;for(const s of document.scripts){if(s.src)continue;const text=s.textContent||'';size+=new TextEncoder().encode(text).length+1;if(size>4000000)return {oversize:true};out+=text+'\n';}return {text:out};})()"#;
struct Finished(Arc<AtomicBool>);
impl Drop for Finished {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
type Reply = mpsc::SyncSender<std::result::Result<Value, String>>;
enum Command {
    Sample(Reply),
}
pub(super) struct Worker {
    tx: mpsc::SyncSender<Command>,
    stop: Arc<AtomicBool>,
    ended: Arc<AtomicBool>,
    fatal: Arc<Mutex<Option<String>>>,
}
impl Worker {
    pub(super) fn spawn(
        owner: usize,
        address: Address,
        profile: Profile,
        global: Arc<AtomicBool>,
    ) -> Result<Self> {
        let (tx, rx) = mpsc::sync_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let ended = Arc::new(AtomicBool::new(false));
        let fatal = Arc::new(Mutex::new(None));
        let (s, e, f) = (stop.clone(), ended.clone(), fatal.clone());
        std::thread::Builder::new()
            .name("isolated-ratings-sta".into())
            .spawn(move || {
                let _finished = Finished(e.clone());
                let result = run(owner, address, profile, global, s.clone(), f.clone(), rx);
                if let Err(error) = result {
                    if let Ok(mut out) = f.lock() {
                        *out = Some(format!("Isolated ratings capture: {error:#}"));
                    }
                }
                e.store(true, Ordering::Release);
            })?;
        Ok(Self {
            tx,
            stop,
            ended,
            fatal,
        })
    }
    pub(super) fn cancel(&self) {
        self.stop.store(true, Ordering::Release);
    }
    pub(super) fn close(&self) -> bool {
        self.cancel();
        let until = Instant::now() + Duration::from_secs(2);
        while !self.ended() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
        }
        self.ended()
    }
    pub(super) fn ended(&self) -> bool {
        self.ended.load(Ordering::Acquire)
    }
    pub(super) fn sample(&self) -> Result<Value> {
        if let Some(error) = self
            .fatal
            .lock()
            .map_err(|_| anyhow::anyhow!("Ratings status unavailable"))?
            .clone()
        {
            anyhow::bail!(error)
        }
        ensure!(!self.ended(), "Ratings window closed or timed out");
        let (tx, rx) = mpsc::sync_channel(1);
        self.tx
            .try_send(Command::Sample(tx))
            .context("Ratings sample is already pending")?;
        match rx.recv_timeout(Duration::from_secs(6)) {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(e)) => anyhow::bail!(e),
            Err(_) => {
                self.cancel();
                anyhow::bail!("Ratings extraction timed out; no values were staged")
            }
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel();
    }
}
fn guard(
    fatal: &Arc<Mutex<Option<String>>>,
    stop: &Arc<AtomicBool>,
    result: windows::core::Result<()>,
) -> windows::core::Result<()> {
    if let Err(e) = &result {
        if let Ok(mut f) = fatal.lock() {
            *f = Some(format!("Isolated browser guard failed: {e}"));
        }
        stop.store(true, Ordering::Release);
    }
    result
}
fn denied() -> windows::core::Error {
    windows::core::Error::from_hresult(HRESULT(0x80070005u32 as i32))
}
unsafe fn bounded(ptr: PCWSTR, max: usize) -> windows::core::Result<String> {
    if ptr.is_null() {
        return Err(denied());
    }
    for n in 0..=max {
        if unsafe { *ptr.0.add(n) } == 0 {
            return String::from_utf16(unsafe { std::slice::from_raw_parts(ptr.0, n) })
                .map_err(|_| denied());
        }
    }
    Err(denied())
}
unsafe fn owned_text(
    mut get: impl FnMut(*mut PWSTR) -> windows::core::Result<()>,
    max: usize,
) -> windows::core::Result<String> {
    let mut p = PWSTR::null();
    let r = get(&mut p).and_then(|_| unsafe { bounded(PCWSTR(p.0), max) });
    if !p.is_null() {
        unsafe { CoTaskMemFree(Some(p.0.cast())) };
    }
    r
}
pub(super) fn valid_owner(raw: usize) -> bool {
    owner_alive(HWND(raw as _))
}
fn owner_alive(owner: HWND) -> bool {
    unsafe {
        let mut pid = 0;
        IsWindow(Some(owner)).as_bool()
            && GetWindowThreadProcessId(owner, Some(&mut pid)) != 0
            && pid == GetCurrentProcessId()
    }
}
#[implement(ICoreWebView2ExecuteScriptCompletedHandler)]
struct Extracted {
    result: Rc<RefCell<Option<std::result::Result<String, String>>>>,
}
impl ICoreWebView2ExecuteScriptCompletedHandler_Impl for Extracted_Impl {
    fn Invoke(&self, status: HRESULT, text: &PCWSTR) -> windows::core::Result<()> {
        let out = status
            .ok()
            .and_then(|_| unsafe { bounded(*text, 8_000_000) })
            .map_err(|_| "Ratings extraction failed or exceeded its native bound".to_owned());
        *self.result.borrow_mut() = Some(out);
        Ok(())
    }
}
struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}
struct Window(HWND);
impl Drop for Window {
    fn drop(&mut self) {
        unsafe {
            if IsWindow(Some(self.0)).as_bool() {
                let _ = DestroyWindow(self.0);
            }
        }
    }
}
struct Controller(ICoreWebView2Controller);
impl Drop for Controller {
    fn drop(&mut self) {
        unsafe {
            let _ = self.0.Close();
        }
    }
}
fn secure(
    env: &ICoreWebView2Environment,
    core: &ICoreWebView2,
    address: Address,
    stop: Arc<AtomicBool>,
    fatal: Arc<Mutex<Option<String>>>,
    navigation: Rc<Cell<u64>>,
) -> windows::core::Result<()> {
    unsafe {
        let settings = core.Settings()?;
        settings.SetIsWebMessageEnabled(false)?;
        settings.SetAreHostObjectsAllowed(false)?;
        settings.SetAreDefaultScriptDialogsEnabled(false)?;
        settings.SetAreDevToolsEnabled(false)?;
        settings.SetAreDefaultContextMenusEnabled(false)?;
        settings.SetIsStatusBarEnabled(false)?;
        let settings4: ICoreWebView2Settings4 = settings.cast()?;
        settings4.SetIsPasswordAutosaveEnabled(false)?;
        settings4.SetIsGeneralAutofillEnabled(false)?;
        let muted: ICoreWebView2_8 = core.cast()?;
        muted.SetIsMuted(true)?;
        let wide: ICoreWebView2_22 = core.cast()?;
        let blocked=env.CreateWebResourceResponse(None::<&windows::Win32::System::Com::IStream>,403,w!("Blocked"),w!("Content-Type: text/plain\r\nCache-Control: no-store\r\nContent-Security-Policy: default-src 'none'"))?;
        let mut token = 0i64;
        let (a, s, f) = (address.clone(), stop.clone(), fatal.clone());
        core.add_WebResourceRequested(
            &WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
                let result = (|| {
                    let args = args.ok_or_else(denied)?;
                    unsafe {
                        args.SetResponse(&blocked)?;
                        let request = args.Request()?;
                        let uri = owned_text(|p| request.Uri(p), 8192)?;
                        if !s.load(Ordering::Acquire) && a.resource(&uri) {
                            args.SetResponse(None::<&ICoreWebView2WebResourceResponse>)?;
                        }
                    }
                    Ok(())
                })();
                guard(&f, &s, result)
            })),
            &mut token,
        )?;
        // Includes document/iframe, shared worker and service worker requests. A runtime
        // lacking this API is rejected before the first remote navigation.
        wide.AddWebResourceRequestedFilterWithRequestSourceKinds(
            w!("*"),
            COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
            COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
        )?;
        let (a, s, f, n) = (
            address.clone(),
            stop.clone(),
            fatal.clone(),
            navigation.clone(),
        );
        core.add_NavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                let result = (|| {
                    let args = args.ok_or_else(denied)?;
                    unsafe {
                        args.SetCancel(true)?;
                        let uri = owned_text(|p| args.Uri(p), 4096)?;
                        n.set(n.get().checked_add(1).ok_or_else(denied)?);
                        if !s.load(Ordering::Acquire) && a.navigation(&uri) {
                            args.SetCancel(false)?;
                        }
                    }
                    Ok(())
                })();
                guard(&f, &s, result)
            })),
            &mut token,
        )?;
        let (a, s, f) = (address.clone(), stop.clone(), fatal.clone());
        core.add_FrameNavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                let result = (|| {
                    let args = args.ok_or_else(denied)?;
                    unsafe {
                        args.SetCancel(true)?;
                        let uri = owned_text(|p| args.Uri(p), 8192)?;
                        if !s.load(Ordering::Acquire) && (uri == "about:blank" || a.resource(&uri))
                        {
                            args.SetCancel(false)?;
                        }
                    }
                    Ok(())
                })();
                guard(&f, &s, result)
            })),
            &mut token,
        )?;
        let (s, f) = (stop.clone(), fatal.clone());
        core.add_NewWindowRequested(
            &NewWindowRequestedEventHandler::create(Box::new(move |_, args| {
                guard(
                    &f,
                    &s,
                    (|| {
                        unsafe { args.ok_or_else(denied)?.SetHandled(true) }?;
                        Ok(())
                    })(),
                )
            })),
            &mut token,
        )?;
        let (s, f) = (stop.clone(), fatal.clone());
        core.add_PermissionRequested(
            &PermissionRequestedEventHandler::create(Box::new(move |_, args| {
                guard(
                    &f,
                    &s,
                    (|| {
                        unsafe {
                            args.ok_or_else(denied)?
                                .SetState(COREWEBVIEW2_PERMISSION_STATE_DENY)
                        }?;
                        Ok(())
                    })(),
                )
            })),
            &mut token,
        )?;
        let downloads: ICoreWebView2_4 = core.cast()?;
        let (s, f) = (stop.clone(), fatal.clone());
        downloads.add_DownloadStarting(
            &DownloadStartingEventHandler::create(Box::new(move |_, args| {
                guard(
                    &f,
                    &s,
                    (|| {
                        let args = args.ok_or_else(denied)?;
                        unsafe {
                            args.SetCancel(true)?;
                            args.SetHandled(true)?;
                        }
                        Ok(())
                    })(),
                )
            })),
            &mut token,
        )?;
        let auth: ICoreWebView2_10 = core.cast()?;
        let (s, f) = (stop.clone(), fatal.clone());
        auth.add_BasicAuthenticationRequested(
            &BasicAuthenticationRequestedEventHandler::create(Box::new(move |_, args| {
                guard(
                    &f,
                    &s,
                    (|| {
                        unsafe { args.ok_or_else(denied)?.SetCancel(true) }?;
                        Ok(())
                    })(),
                )
            })),
            &mut token,
        )?;
        let certificates: ICoreWebView2_5 = core.cast()?;
        let (s, f) = (stop.clone(), fatal.clone());
        certificates.add_ClientCertificateRequested(
            &ClientCertificateRequestedEventHandler::create(Box::new(move |_, args| {
                guard(
                    &f,
                    &s,
                    (|| {
                        let args = args.ok_or_else(denied)?;
                        unsafe {
                            args.SetCancel(true)?;
                            args.SetHandled(true)?;
                        }
                        Ok(())
                    })(),
                )
            })),
            &mut token,
        )?;
        let certificate_errors: ICoreWebView2_14 = core.cast()?;
        let (s, f) = (stop.clone(), fatal.clone());
        certificate_errors.add_ServerCertificateErrorDetected(
            &ServerCertificateErrorDetectedEventHandler::create(Box::new(move |_, args| {
                guard(
                    &f,
                    &s,
                    (|| {
                        unsafe {
                            args.ok_or_else(denied)?
                                .SetAction(COREWEBVIEW2_SERVER_CERTIFICATE_ERROR_ACTION_CANCEL)
                        }?;
                        Ok(())
                    })(),
                )
            })),
            &mut token,
        )?;
        let s = stop.clone();
        core.add_ProcessFailed(
            &ProcessFailedEventHandler::create(Box::new(move |_, _| {
                s.store(true, Ordering::Release);
                Ok(())
            })),
            &mut token,
        )?;
        let s = stop.clone();
        core.add_WindowCloseRequested(
            &WindowCloseRequestedEventHandler::create(Box::new(move |_, _| {
                s.store(true, Ordering::Release);
                Ok(())
            })),
            &mut token,
        )?;
        Ok(())
    }
}
fn run(
    owner: usize,
    address: Address,
    profile: Profile,
    global: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    fatal: Arc<Mutex<Option<String>>>,
    commands: mpsc::Receiver<Command>,
) -> Result<()> {
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        let _apartment = Apartment;
        let owner = HWND(owner as _);
        ensure!(
            owner_alive(owner),
            "Capture owner is not this launcher window"
        );
        let window = Window(CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("STATIC"),
            w!("Ratings page — isolated read-only capture"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1100,
            800,
            Some(owner),
            None,
            None,
            None,
        )?);
        let env_result: Rc<RefCell<Option<std::result::Result<ICoreWebView2Environment, String>>>> =
            Rc::new(RefCell::new(None));
        let e = env_result.clone();
        let handler =
            CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(move |status, env| {
                *e.borrow_mut() = Some(
                    status
                        .map_err(|e| e.to_string())
                        .and_then(|_| env.ok_or_else(|| "WebView2 environment missing".into())),
                );
                Ok(())
            }));
        let options = CoreWebView2EnvironmentOptions::default();
        options.set_allow_single_sign_on_using_os_primary_account(false);
        options.set_are_browser_extensions_enabled(false);
        let options: ICoreWebView2EnvironmentOptions = options.into();
        let path = profile
            .path
            .to_string_lossy()
            .encode_utf16()
            .chain(Some(0))
            .collect::<Vec<_>>();
        CreateCoreWebView2EnvironmentWithOptions(
            PCWSTR::null(),
            PCWSTR(path.as_ptr()),
            &options,
            &handler,
        )?;
        let controllers: Rc<RefCell<Option<std::result::Result<ICoreWebView2Controller, String>>>> =
            Rc::new(RefCell::new(None));
        let mut controller_started = false;
        let mut environment = None;
        let mut controller = None;
        let mut view = None;
        let navigation = Rc::new(Cell::new(0u64));
        let started = Instant::now();
        let mut last_bounds = None;
        let mut pending: Option<(
            Instant,
            Reply,
            Rc<RefCell<Option<std::result::Result<String, String>>>>,
            u64,
        )> = None;
        while started.elapsed() < LIFETIME
            && !stop.load(Ordering::Acquire)
            && !global.load(Ordering::Acquire)
            && owner_alive(owner)
            && IsWindow(Some(window.0)).as_bool()
        {
            let mut message = MSG::default();
            for _ in 0..64 {
                if !PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                    break;
                }
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            if !controller_started {
                if let Some(result) = env_result.borrow_mut().take() {
                    let env = result.map_err(|e| anyhow::anyhow!(e))?;
                    let e10: ICoreWebView2Environment10 = env.cast()?;
                    let opt = e10.CreateCoreWebView2ControllerOptions()?;
                    opt.SetIsInPrivateModeEnabled(true)?;
                    opt.SetProfileName(w!("ratings-isolated"))?;
                    let c = controllers.clone();
                    let callback = CreateCoreWebView2ControllerCompletedHandler::create(Box::new(
                        move |status, value| {
                            *c.borrow_mut() =
                                Some(status.map_err(|e| e.to_string()).and_then(|_| {
                                    value.ok_or_else(|| "WebView2 controller missing".into())
                                }));
                            Ok(())
                        },
                    ));
                    e10.CreateCoreWebView2ControllerWithOptions(window.0, &opt, &callback)?;
                    controller_started = true;
                    environment = Some(env);
                }
            }
            if view.is_none() {
                if let Some(result) = controllers.borrow_mut().take() {
                    let c = Controller(result.map_err(|e| anyhow::anyhow!(e))?);
                    let core = c.0.CoreWebView2()?;
                    secure(
                        environment
                            .as_ref()
                            .context("Capture environment missing")?,
                        &core,
                        address.clone(),
                        stop.clone(),
                        fatal.clone(),
                        navigation.clone(),
                    )?;
                    let mut bounds = RECT::default();
                    GetClientRect(window.0, &mut bounds)?;
                    c.0.SetBounds(bounds)?;
                    c.0.SetIsVisible(true)?;
                    let url = address
                        .url
                        .encode_utf16()
                        .chain(Some(0))
                        .collect::<Vec<_>>();
                    ensure!(
                        !stop.load(Ordering::Acquire) && !global.load(Ordering::Acquire),
                        "Capture canceled before navigation"
                    );
                    core.Navigate(PCWSTR(url.as_ptr()))?;
                    controller = Some(c);
                    view = Some(core);
                }
            }
            if let Some(c) = &controller {
                let mut bounds = RECT::default();
                GetClientRect(window.0, &mut bounds)?;
                let shape = (bounds.left, bounds.top, bounds.right, bounds.bottom);
                if last_bounds != Some(shape) {
                    c.0.SetBounds(bounds)?;
                    last_bounds = Some(shape);
                }
            }
            if let Ok(Command::Sample(reply)) = commands.try_recv() {
                if pending.is_some() {
                    let _ = reply.send(Err("An extraction is already pending".into()));
                } else if let Some(core) = &view {
                    let source = owned_text(|p| core.Source(p), 4096)?;
                    if !address.is_source(&source) {
                        let _ = reply.send(Ok(json!({"ready":false})));
                    } else {
                        let output = Rc::new(RefCell::new(None));
                        let callback: ICoreWebView2ExecuteScriptCompletedHandler = Extracted {
                            result: output.clone(),
                        }
                        .into();
                        let script = SCRIPT.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
                        core.ExecuteScript(PCWSTR(script.as_ptr()), &callback)?;
                        pending = Some((Instant::now(), reply, output, navigation.get()));
                    }
                } else {
                    let _ = reply.send(Ok(json!({"ready":false})));
                }
            }
            if let Some((when, _, output, _)) = &pending {
                if when.elapsed() > Duration::from_secs(5) {
                    stop.store(true, Ordering::Release);
                } else if output.borrow().is_some() {
                    let (_, reply, output, revision) = pending.take().unwrap();
                    let text = output
                        .borrow_mut()
                        .take()
                        .unwrap()
                        .map_err(|e| anyhow::anyhow!(e))?;
                    let value: Value = serde_json::from_str(&text)
                        .context("Invalid isolated extraction envelope")?;
                    let source = owned_text(|p| view.as_ref().unwrap().Source(p), 4096)?;
                    let result = if revision != navigation.get() || !address.is_source(&source) {
                        json!({"ready":false})
                    } else {
                        ensure!(value["oversize"] != true, "Ratings page data exceeds4MB");
                        let text = value["text"]
                            .as_str()
                            .context("Ratings script data unavailable")?;
                        ensure!(text.len() <= MAX_TEXT, "Ratings page data exceeds4MB");
                        json!({"ready":true,"text":text})
                    };
                    let _ = reply.send(Ok(result));
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        if let Some((_, reply, _, _)) = pending.take() {
            let _ = reply.send(Err("Ratings capture closed, canceled or timed out".into()));
        }
        if let Some(core) = view.take() {
            let _ = core.Stop();
        }
        drop(controller);
        drop(environment);
        drop(profile);
        Ok(())
    }
}
