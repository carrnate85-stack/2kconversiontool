//! Foreground-scoped F8 and exact game-window ownership. Never writes game memory.
use crate::{
    backend::Backend,
    blacktop_native,
    session_safety::{self, Identity},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::ClientToScreen,
        UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::*,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Frame {
    pub hwnd: u64,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}
impl Frame {
    pub fn bounds(&self, catalog: bool, bar_height: u32) -> Result<Frame> {
        ensure!(
            (320..=16384).contains(&self.width) && (240..=16384).contains(&self.height),
            "Game client dimensions are unsupported"
        );
        // WPF BlacktopPanel: 12 px inset, top-anchored, full width; the
        // expanded catalog takes 75% of the height (at least 360 px).
        let mut f = self.clone();
        f.x = f.x.checked_add(12).context("Position overflow")?;
        f.y = f.y.checked_add(12).context("Position overflow")?;
        f.width -= 24;
        let available = f.height - 24;
        f.height = if catalog {
            available.min((f.height * 3 / 4).max(360))
        } else {
            // Compact bar, or the bar plus a WPF-style dropdown below it.
            bar_height.clamp(70, 600).min(available)
        };
        Ok(f)
    }
    pub fn check(&self, id: &Identity) -> Result<()> {
        ensure!(
            session_safety::identity(id.pid)? == *id,
            "Game-window process identity changed"
        );
        ensure!(
            window_frame(HWND(self.hwnd as _), id.pid, true)? == *self,
            "Game window changed or minimized"
        );
        Ok(())
    }
}
fn window_frame(hwnd: HWND, pid: u32, visible: bool) -> Result<Frame> {
    let mut actual = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut actual));
    }
    ensure!(
        actual == pid
            && unsafe { IsWindow(Some(hwnd)) }.as_bool()
            && (!visible
                || (unsafe { IsWindowVisible(hwnd) }.as_bool()
                    && !unsafe { IsIconic(hwnd) }.as_bool())),
        "No visible owned game window"
    );
    let mut r = RECT::default();
    let mut origin = POINT::default();
    unsafe {
        GetClientRect(hwnd, &mut r)?;
        ensure!(
            ClientToScreen(hwnd, &mut origin).as_bool(),
            "Cannot locate game client"
        );
    }
    let width = r
        .right
        .checked_sub(r.left)
        .context("Invalid window width")?;
    let height = r
        .bottom
        .checked_sub(r.top)
        .context("Invalid window height")?;
    ensure!(
        (320..=16384).contains(&width) && (240..=16384).contains(&height),
        "Game window is a splash/dialog or unsupported viewport"
    );
    Ok(Frame {
        hwnd: hwnd.0 as u64,
        x: origin.x,
        y: origin.y,
        width: width as u32,
        height: height as u32,
    })
}
pub fn fixture_frame(hwnd: HWND) -> Result<Frame> {
    window_frame(hwnd, std::process::id(), false)
}
pub fn find(id: &Identity) -> Result<Frame> {
    struct Search {
        pid: u32,
        frames: Vec<Frame>,
    }
    unsafe extern "system" fn visit(hwnd: HWND, param: LPARAM) -> BOOL {
        let search = unsafe { &mut *(param.0 as *mut Search) };
        // GetWindow translates a null owner into Err; null is normal for an
        // unowned top-level window, not a failed game-window lookup.
        if unsafe { GetWindow(hwnd, GW_OWNER) }.is_err() {
            if let Ok(f) = window_frame(hwnd, search.pid, true) {
                search.frames.push(f);
            }
        }
        BOOL(1)
    }
    ensure!(
        session_safety::identity(id.pid)? == *id,
        "Game identity changed before window lookup"
    );
    let mut search = Search {
        pid: id.pid,
        frames: vec![],
    };
    unsafe {
        EnumWindows(Some(visit), LPARAM(&mut search as *mut _ as isize))?;
    }
    // Ambiguity is held rather than attaching to a different dialog/top-level window.
    ensure!(
        search.frames.len() == 1,
        "No unique visible game client window"
    );
    let f = search.frames.pop().unwrap();
    f.check(id)?;
    Ok(f)
}
/// An F8 press or toggle command survives a failed sample (the reads at player
/// selection often fail while models load) and is used by the next good one,
/// unless it is older than a moment.
#[derive(Default)]
struct UserIntent {
    toggle: Option<Instant>,
}
const INTENT_LIFETIME: Duration = Duration::from_millis(2000);
impl UserIntent {
    fn note(&mut self, toggle: bool, now: Instant) {
        if toggle {
            self.toggle = Some(now);
        }
    }
    fn take(&mut self, now: Instant) -> bool {
        self.toggle
            .take()
            .is_some_and(|at| now.saturating_duration_since(at) < INTENT_LIFETIME)
    }
    fn take_when_allowed(&mut self, allowed: bool, now: Instant) -> bool {
        if allowed {
            self.take(now)
        } else {
            // A transient successful-but-unknown boundary is no stronger than
            // a failed sample. Keep a foreground press for the next good read.
            if self
                .toggle
                .is_some_and(|at| now.saturating_duration_since(at) >= INTENT_LIFETIME)
            {
                self.toggle = None;
            }
            false
        }
    }
}
/// One read-only observation of the Blacktop screen (WPF `detect`).
pub struct Observation<'a> {
    pub boundary: &'a str,
    pub lineup_ready: bool,
    pub required: u32,
    pub selected: u32,
}
/// WPF `BlacktopAutoOpen.observe` (core/blacktop-auto-open.js:10-40) plus the
/// F8 toggle. `visible` is WPF's `wanted`; `entered` makes auto-open fire once
/// per selection visit, so a user hide during a complete lineup is respected.
#[derive(Default)]
pub struct Policy {
    visible: bool,
    entered: bool,
    inside: u32,
    outside: u32,
    in_match: bool,
}
impl Policy {
    fn match_start(&self, boundary: &str) -> bool {
        boundary == "match" && !self.in_match
    }
    /// "Hide overlay" / Escape: sticky until F8 or the next selection visit.
    fn hide(&mut self) {
        self.visible = false;
        self.entered = true;
    }
    fn update(
        &mut self,
        o: &Observation,
        foreground: bool,
        auto_show: bool,
        auto_hide: bool,
        toggle: bool,
    ) -> bool {
        let selection = o.boundary == "selection";
        let allowed = selection || o.boundary == "match";
        let match_start = self.match_start(o.boundary);
        if selection
            || (!allowed && !matches!(o.boundary, "transition" | "transition-or-other-mode"))
        {
            self.in_match = false;
        } else if match_start {
            self.in_match = true;
            // A confirmed match is its own boundary. Loading may already have
            // ended the selection visit; still close exactly once here.
            if auto_hide {
                self.visible = false;
            }
        }
        if !selection {
            self.inside = 0;
            self.outside = self.outside.saturating_add(1);
            // Two consecutive reads off selection end the visit (match start,
            // menus): the bar goes away and the next selection re-arms it.
            // Only on that edge, so an F8-opened bar in a match stays put.
            if self.outside == 2 {
                self.entered = false;
                if auto_hide && !self.in_match {
                    self.visible = false;
                }
            }
            // Rust keeps F8 to Blacktop screens (WPF showed it anywhere).
            if !allowed && self.outside >= 2 {
                self.visible = false;
            }
        } else {
            self.outside = 0;
            if self.visible {
                self.entered = true;
            }
            if !o.lineup_ready {
                self.inside = 0;
                // Re-arm only on an explicit incomplete lineup; loading or
                // unknown reads neither open nor re-arm.
                let explicit = (2..=10).contains(&o.required)
                    && o.required % 2 == 0
                    && o.selected < o.required;
                if auto_show && explicit {
                    self.entered = false;
                    self.visible = false;
                }
            } else {
                self.inside = self.inside.saturating_add(1);
                if auto_show && self.inside >= 2 && !self.entered && foreground {
                    self.entered = true;
                    self.visible = true;
                }
            }
        }
        if toggle && foreground && allowed {
            self.visible = !self.visible;
            if self.visible && selection {
                self.entered = true;
            }
        }
        self.visible && foreground
    }
}
struct Key {
    down: bool,
}
impl Key {
    /// WPF F8Watcher semantics: only a fresh physical press in our foreground
    /// scope counts. Track releases even outside that scope; Alt-Tab while
    /// holding F8 must not synthesize a press when returning to the game.
    fn sample(&mut self, scoped: bool, down: bool) -> bool {
        let pressed = scoped && down && !self.down;
        self.down = down;
        pressed
    }
}
#[derive(Clone)]
struct Control {
    enabled: bool,
    epoch: u64,
    revision: u64,
    ready: bool,
    catalog: bool,
    // Last foreground classification (game/overlay/other/none), logged only.
    foreground: &'static str,
    foreground_pid: u32,
    foreground_hwnd: u64,
    foreground_exe: String,
    // A focusable dropdown (arena) sized to bar + menu instead of 75%.
    dropdown: bool,
    bar_height: u32,
    visible: bool,
    frame: Option<Frame>,
    identity: Option<Identity>,
    boundary: String,
    // Raw fresh observation also gates actions; presentation grace never does.
    observation: Option<blacktop_native::Snapshot>,
    observation_error: Option<String>,
    message: String,
    phase: String,
    manual_toggle: bool,
    hide_requested: bool,
    enabled_since: Instant,
    last_ui: Option<Instant>,
}
impl Default for Control {
    fn default() -> Self {
        Self {
            enabled: false,
            epoch: 0,
            revision: 0,
            ready: false,
            catalog: false,
            foreground: "none",
            foreground_pid: 0,
            foreground_hwnd: 0,
            foreground_exe: String::new(),
            dropdown: false,
            bar_height: 112,
            visible: false,
            frame: None,
            identity: None,
            boundary: "unknown".into(),
            observation: None,
            observation_error: None,
            message: "Native F8 is off; offline preview remains available.".into(),
            phase: "disabled".into(),
            manual_toggle: false,
            hide_requested: false,
            enabled_since: Instant::now(),
            last_ui: None,
        }
    }
}
type AutoEnable = Arc<dyn Fn() -> Result<()> + Send + Sync>;
struct Inner {
    control: Mutex<Control>,
    stop: Mutex<bool>,
    wake: Condvar,
    diagnostics: Mutex<(PathBuf, Option<Value>, std::collections::VecDeque<Value>)>,
    auto_enable: Mutex<Option<AutoEnable>>,
    focus_hide_pending: AtomicBool,
}
/// WPF's BlacktopAutoOpen watched every Play session without a manual step.
/// "Automatically show at player selection" is that opt-in here: attach the
/// owned F8 window once per exact game process. A later Stop, UI timeout or
/// failure in the same process is respected; a new launch attaches again.
fn should_auto_enable(
    enabled: bool,
    auto_show: bool,
    bound: &Identity,
    attached: Option<&Identity>,
) -> bool {
    !enabled && auto_show && attached != Some(bound)
}
pub struct Runtime {
    inner: Arc<Inner>,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
    input_worker: Mutex<Option<thread::JoinHandle<()>>>,
    app: AppHandle,
    backend: Arc<Backend>,
    smoke: bool,
}
impl Runtime {
    pub fn new(app: AppHandle, backend: Arc<Backend>, smoke: bool) -> Result<Self> {
        let inner = Arc::new(Inner {
            control: Mutex::new(Control::default()),
            stop: Mutex::new(false),
            wake: Condvar::new(),
            diagnostics: Mutex::new((
                backend.root.join("Logs/native-overlay-state.json"),
                None,
                Default::default(),
            )),
            auto_enable: Mutex::new(None),
            focus_hide_pending: AtomicBool::new(false),
        });
        let shared = inner.clone();
        let owner = app.clone();
        let engine = backend.clone();
        let worker=thread::Builder::new().name("native-f8-window-monitor".into()).spawn(move||{
            let mut reader:Option<blacktop_native::Reader>=None;let mut policy=Policy::default();let mut last=Instant::now()-Duration::from_secs(1);let mut previous_epoch=0;let mut previous_focus=None;let mut attached:Option<Identity>=None;let mut next_auto=Instant::now();let mut focus=FocusFilter::default();let mut held=FocusFilter::with(READ_ERROR_GRACE);let mut parents=ProcessCache::new();let mut intent=UserIntent::default();let mut boundary_display=BoundaryDisplay::default();
            let mut last_publish=Instant::now()-Duration::from_secs(10);
            loop {
                let stopping=shared.stop.lock().unwrap_or_else(|e|e.into_inner());
                let (stopping,_)=shared.wake.wait_timeout(stopping,Duration::from_millis(40)).unwrap_or_else(|e|e.into_inner());
                if *stopping {break;}drop(stopping);
                let c=shared.control.lock().unwrap_or_else(|e|e.into_inner()).clone();
                if !c.enabled||smoke {
                    reader.take();policy=Policy::default();previous_focus=None;boundary_display=BoundaryDisplay::default();parents.clear();
                    let auto=shared.auto_enable.lock().unwrap_or_else(|e|e.into_inner()).clone();
                    if let Some(auto)=auto.filter(|_|!smoke&&Instant::now()>=next_auto) {
                        next_auto=Instant::now()+Duration::from_secs(2);
                        let bound=engine.current_settings().ok().filter(|s|s.auto_show_overlay)
                            .and_then(|s|engine.session.binding(Path::new(&s.game_root)).ok()).map(|(id,_)|id);
                        if let Some(id)=bound.filter(|id|should_auto_enable(c.enabled,true,id,attached.as_ref())) {
                            // The game window may not exist yet; retry until it attaches.
                            if auto().is_ok() {attached=Some(id);}
                        }
                    }
                    continue;
                }
                if ui_stalled(&c,Instant::now()) || (c.phase=="ui-unresponsive"&&ui_expired(&c,Instant::now())) {
                    // A stalled WebView must hide and lose write authorization,
                    // not permanently disable F8 for the entire game session.
                    if c.phase!="ui-unresponsive" {
                        let mut next=c.clone();next.visible=false;next.phase="ui-unresponsive".into();next.message="Native F8 UI paused; hidden until its heartbeat resumes. Game left running.".into();publish(&owner,&shared,next);
                    }
                    continue;
                }
                if c.epoch!=previous_epoch {reader.take();policy=Policy::default();last=Instant::now()-Duration::from_secs(1);previous_epoch=c.epoch;previous_focus=None;focus=FocusFilter::default();held=FocusFilter::with(READ_ERROR_GRACE);intent=UserIntent::default();boundary_display=BoundaryDisplay::default();parents.clear();}
                // A Hide click is applied at once, even if this sample fails (WPF:
                // hide() cleared `wanted` immediately).
                if c.hide_requested {policy.hide();}
                let own=owner.get_webview_window("live-overlay").and_then(|w|w.hwnd().ok()).map(|h|h.0 as u64);
                let fg=read_foreground(c.frame.as_ref().map(|f|f.hwnd),c.identity.as_ref().map(|i|i.pid),own,&mut parents);
                // A process-name lookup can outlive an Alt-Tab. Do not publish
                // logical focus loss from a foreground sample that already changed.
                if !foreground_sample_current(&fg) {continue;}
                let kind=fg.kind;
                let focused=focus.update(kind.ours(),Instant::now());
                let focus_changed=previous_focus!=Some(focused);previous_focus=Some(focused);
                if !focused {
                    if c.visible&&!c.manual_toggle {
                        // One guarded UI-thread hide; stale posted Win32 hides
                        // must not land after a returning game has shown the bar.
                        schedule_focus_hide(&owner,&shared,&c,own,&fg);
                        let mut hidden=c.clone();hidden.visible=false;hidden.foreground=kind.label();hidden.foreground_pid=fg.pid;hidden.foreground_hwnd=fg.hwnd;hidden.foreground_exe=fg.exe.clone();hidden.phase="waiting-for-focus".into();hidden.message="Native F8 hidden while another app is foreground.".into();
                        publish(&owner,&shared,hidden);
                        continue;
                    }
                }
                let pressed=c.manual_toggle;
                intent.note(pressed||c.manual_toggle,Instant::now());
                if !pressed&&!focus_changed&&!c.manual_toggle&&!c.hide_requested&&last.elapsed()<Duration::from_millis(250) {continue;}
                last=Instant::now();
                let sample=(||->Result<(Frame,Identity,blacktop_native::Snapshot,ForegroundSample,crate::backend::Settings)>{
                    let settings=engine.current_settings()?;
                    let (id,game)=engine.session.binding(Path::new(&settings.game_root))?;
                    if reader.as_ref().is_none_or(|r|r.identity!=id) {reader.take();reader=Some(blacktop_native::Reader::open_observer(&id,&game)?);}
                    let snap=reader.as_ref().context("Missing reader")?.observer_snapshot()?;
                    blacktop_native::guard(&session_safety::processes()?,&id)?;
                    let frame=find(&id)?;
                    let own=owner.get_webview_window("live-overlay").and_then(|w|w.hwnd().ok()).map(|h|h.0 as u64);
                    let fg=read_foreground(Some(frame.hwnd),Some(id.pid),own,&mut parents);
                    Ok((frame,id,snap,fg,settings))
                })();
                let mut next=c.clone();next.manual_toggle=false;next.hide_requested=false;
                match sample {
                    Ok((frame,id,snap,fg,settings))=>{
                        if !foreground_sample_current(&fg) {continue;}
                        next.observation=Some(snap.clone());next.observation_error=None;
                        let boundary=boundary_display.observe(&id,&snap,Instant::now()).to_owned();
                        held.update(true,Instant::now());
                        let foreground=focus.update(fg.kind.ours(),Instant::now());next.foreground=fg.kind.label();next.foreground_pid=fg.pid;next.foreground_hwnd=fg.hwnd;next.foreground_exe=fg.exe;
                        let allowed=matches!(boundary.as_str(),"selection"|"match");
                        let match_start=settings.auto_hide_overlay&&policy.match_start(&boundary);
                        next.visible=policy.update(&Observation{boundary:&boundary,lineup_ready:snap.lineup_ready,required:snap.required_players,selected:snap.selected_players},foreground,settings.auto_show_overlay,settings.auto_hide_overlay,intent.take_when_allowed(allowed,Instant::now()));
                        if c.hide_requested&&!c.manual_toggle {policy.hide();next.visible=false;}
                        if !keep_panel(allowed,next.visible,foreground,pressed||c.manual_toggle||c.hide_requested,&boundary,match_start) {next.catalog=false;next.dropdown=false;}
                        // Re-shown after Alt-Tab: give the throttled hidden UI a fresh heartbeat window.
                        if next.visible&&!c.visible {next.last_ui=Some(Instant::now());}
                        next.frame=Some(frame);next.identity=Some(id);next.boundary=boundary;next.phase=if allowed&&foreground {"bound"}else if allowed {"waiting-for-focus"}else{"waiting-for-blacktop"}.into();
                        next.message=if allowed&&foreground {"Native F8 is bound to this game. Native write availability depends on the verified game profile and explicit acknowledgement."}else if allowed {"Native F8 is hidden while another app is foreground. Return to the game to use it."}else{"Waiting for Blacktop selection or match; overlay hidden and F8 released."}.into();
                    }
                    Err(e)=>{reader.take();
                        // Retain only presentation for a proven short context
                        // race. Raw action permission is revoked immediately.
                        observation_failed(&mut next,&e,held.update(false,Instant::now()));
                        if engine.session.status().is_ok_and(|s|matches!(s["phase"].as_str(),Some("closed"|"idle"))) {policy=Policy::default();next.enabled=false;next.phase="ended".into();next.ready=false;}
                    }
                }
                // WPF only re-presented the panel on a decision. A sample that
                // changes nothing visible is not re-published (no state event,
                // re-render, topmost or focus calls) unless the window's real
                // visibility disagrees (a deferred show) or 2 s have passed.
                let shown_matches=own.map(|h|unsafe{IsWindowVisible(HWND(h as _))}.as_bool())==Some(wanted_visible(&next));
                if !pressed&&!focus_changed&&shown_matches&&last_publish.elapsed()<Duration::from_secs(2)&&presentation_unchanged(&c,&next) {
                    // No UI work is needed, but raw observations still revoke
                    // action authorization immediately during display grace.
                    let mut current=shared.control.lock().unwrap_or_else(|e|e.into_inner());
                    let stored=commit(&mut current,&mut next);drop(current);
                    if stored {let _=record_diagnostic(&shared,&next,None,None);}
                    continue;
                }
                last_publish=Instant::now();
                publish(&owner,&shared,next);
            }
            reader.take();
        })?;
        // Input sampling is independent of the potentially slow model/process
        // observer. A quick F8 tap cannot disappear during a native read.
        let shared = inner.clone();
        let owner = app.clone();
        let input_worker = thread::Builder::new()
            .name("native-f8-input".into())
            .spawn(move || {
                let mut key = Key { down: false };
                let mut parents = ProcessCache::new();
                let mut cache_epoch = None;
                loop {
                    if *shared.stop.lock().unwrap_or_else(|e| e.into_inner()) {
                        break;
                    }
                    let c = shared
                        .control
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .clone();
                    // PID classifications never cross a new enable/disable session.
                    if cache_epoch != Some(c.epoch) {
                        parents.clear();
                        cache_epoch = Some(c.epoch);
                    }
                    let own = owner
                        .get_webview_window("live-overlay")
                        .and_then(|w| w.hwnd().ok())
                        .map(|h| h.0 as u64);
                    let fg = read_foreground(
                        c.frame.as_ref().map(|f| f.hwnd),
                        c.identity.as_ref().map(|i| i.pid),
                        own,
                        &mut parents,
                    );
                    // Physical focus hiding must not wait behind the observer's
                    // game/model reads, UI heartbeat or stale logical visibility.
                    // Keep user intent so Alt-Tab back can present it again.
                    if !fg.kind.ours() {
                        schedule_focus_hide(&owner, &shared, &c, own, &fg);
                    }
                    let scoped = !smoke
                        && c.enabled
                        && c.ready
                        && fg.kind.ours()
                        && foreground_sample_current(&fg);
                    if key.sample(scoped, unsafe { GetAsyncKeyState(VK_F8.0 as i32) } < 0) {
                        let mut current = shared.control.lock().unwrap_or_else(|e| e.into_inner());
                        if current.epoch == c.epoch && current.enabled {
                            let hidden = input_toggle(&mut current);
                            let next = current.clone();
                            drop(current);
                            if hidden {
                                // Queue directly from the 20 ms input loop. The window
                                // owner renders the latest intent, so a delayed close
                                // cannot undo a newer F8 reopen.
                                publish(&owner, &shared, next);
                            }
                            shared.wake.notify_all();
                        }
                    }
                    thread::sleep(Duration::from_millis(20));
                }
            });
        let input_worker = match input_worker {
            Ok(input) => input,
            Err(error) => {
                *inner.stop.lock().unwrap_or_else(|e| e.into_inner()) = true;
                inner.wake.notify_all();
                let _ = worker.join();
                return Err(error.into());
            }
        };
        Ok(Self {
            inner,
            worker: Mutex::new(Some(worker)),
            input_worker: Mutex::new(Some(input_worker)),
            app,
            backend,
            smoke,
        })
    }
    /// Installed once by the app after its state exists; builds the owned
    /// window through the same path as the manual Enable command.
    pub fn set_auto_enable(&self, f: AutoEnable) {
        *self
            .inner
            .auto_enable
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(f);
        self.inner.wake.notify_all();
    }
    pub fn status(&self) -> Value {
        view(&self.inner.control.lock().unwrap_or_else(|e| e.into_inner()))
    }
    pub fn enable(&self) -> Result<Frame> {
        ensure!(!self.smoke, "Native F8 cannot be enabled in smoke mode");
        let settings = self.backend.current_settings()?;
        let (id, _) = self
            .backend
            .session
            .binding(Path::new(&settings.game_root))?;
        let frame = find(&id)?;
        let mut c = self
            .inner
            .control
            .lock()
            .map_err(|_| anyhow::anyhow!("Overlay state unavailable"))?;
        ensure!(!c.enabled, "Native F8 already enabled");
        c.epoch += 1;
        c.revision += 1;
        c.enabled = true;
        c.ready = false;
        c.enabled_since = Instant::now();
        c.last_ui = None;
        c.catalog = false;
        c.dropdown = false;
        c.hide_requested = false;
        c.manual_toggle = false;
        c.frame = Some(frame.clone());
        c.identity = Some(id);
        c.phase = "waiting".into();
        c.message = "Preparing owned F8 window; game writes remain opt-in.".into();
        drop(c);
        self.inner.wake.notify_all();
        Ok(frame)
    }
    pub fn disable(&self) -> Result<Value> {
        let mut c = self
            .inner
            .control
            .lock()
            .map_err(|_| anyhow::anyhow!("Overlay state unavailable"))?;
        c.epoch += 1;
        c.revision += 1;
        c.enabled = false;
        c.ready = false;
        c.visible = false;
        let frame = c.frame.take();
        let id = c.identity.take();
        c.phase = "disabled".into();
        c.message = "Native F8 stopped; game and Steam left running.".into();
        let result = view(&c);
        drop(c);
        self.inner.wake.notify_all();
        if let Some(w) = self.app.get_webview_window("live-overlay") {
            return_focus(&w, frame.as_ref(), id.as_ref());
            w.hide()?;
        }
        Ok(result)
    }
    pub fn layout(&self, catalog: bool, bar_height: u32, dropdown: bool) -> Result<Value> {
        let mut c = self
            .inner
            .control
            .lock()
            .map_err(|_| anyhow::anyhow!("Overlay state unavailable"))?;
        ensure!(c.enabled, "Native F8 is disabled");
        c.revision += 1;
        c.catalog = catalog;
        c.dropdown = catalog && dropdown;
        c.bar_height = bar_height.clamp(70, 600);
        c.ready = true;
        c.last_ui = Some(Instant::now());
        let next = c.clone();
        drop(c);
        publish(&self.app, &self.inner, next);
        self.inner.wake.notify_all();
        Ok(self.status())
    }
    pub fn toggle(&self) -> Result<Value> {
        let mut c = self
            .inner
            .control
            .lock()
            .map_err(|_| anyhow::anyhow!("Overlay state unavailable"))?;
        ensure!(c.enabled, "Enable native F8 first");
        if c.visible {
            drop(c);
            return self.hide();
        }
        let frame = c.frame.clone().context("Wait for the bound game window")?;
        let identity = c
            .identity
            .clone()
            .context("Wait for the bound game identity")?;
        frame.check(&identity)?;
        // This explicit main-window button brings its exact game forward;
        // it never bypasses the foreground rule to draw over another app.
        if unsafe { GetForegroundWindow() }.0 as u64 != frame.hwnd {
            ensure!(
                unsafe { SetForegroundWindow(HWND(frame.hwnd as _)) }.as_bool(),
                "Return to NBA 2K27 and press F8; Windows refused the focus request"
            );
        }
        c.revision += 1;
        c.manual_toggle = true;
        drop(c);
        self.inner.wake.notify_all();
        Ok(self.status())
    }
    pub fn heartbeat(&self) -> Result<Value> {
        let mut c = self
            .inner
            .control
            .lock()
            .map_err(|_| anyhow::anyhow!("Overlay state unavailable"))?;
        ensure!(c.enabled && c.ready, "Native F8 is not ready");
        c.revision += 1;
        c.last_ui = Some(Instant::now());
        Ok(json!(true))
    }
    pub fn hide(&self) -> Result<Value> {
        let mut c = self
            .inner
            .control
            .lock()
            .map_err(|_| anyhow::anyhow!("Overlay state unavailable"))?;
        c.revision += 1;
        c.visible = false;
        c.catalog = false;
        c.dropdown = false;
        c.hide_requested = true;
        let next = c.clone();
        drop(c);
        publish(&self.app, &self.inner, next);
        self.inner.wake.notify_all();
        // Hide is an explicit user action, not merely a queued render hint.
        if let Some(w) = self.app.get_webview_window("live-overlay") {
            let current = self
                .inner
                .control
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            return_focus(&w, current.frame.as_ref(), current.identity.as_ref());
            w.hide()?;
            let _ = record_diagnostic(&self.inner, &current, Some(false), None);
        }
        Ok(self.status())
    }
    pub fn authorize(&self) -> Result<()> {
        let c = self
            .inner
            .control
            .lock()
            .map_err(|_| anyhow::anyhow!("Overlay state unavailable"))?
            .clone();
        ensure!(
            c.enabled && c.ready && c.visible && raw_action_boundary(c.observation.as_ref()),
            "No active bound F8 window"
        );
        ensure!(
            !ui_expired(&c, Instant::now()),
            "Native F8 UI lease expired"
        );
        let settings = self.backend.current_settings()?;
        let (id, _) = self
            .backend
            .session
            .binding(Path::new(&settings.game_root))?;
        ensure!(
            c.identity.as_ref() == Some(&id),
            "Overlay game session changed"
        );
        c.frame
            .as_ref()
            .context("No owned game viewport")?
            .check(&id)?;
        let foreground = unsafe { GetForegroundWindow() }.0;
        let game = c.frame.as_ref().map(|f| f.hwnd);
        let owned = self
            .app
            .get_webview_window("live-overlay")
            .and_then(|w| w.hwnd().ok())
            .map(|h| h.0 as u64);
        ensure!(
            Some(foreground as u64) == game || Some(foreground as u64) == owned,
            "Game or its owned F8 window must be foreground"
        );
        Ok(())
    }
    pub fn shutdown(&self) {
        *self.inner.stop.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.inner.wake.notify_all();
        if let Some(w) = self
            .input_worker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            let _ = w.join();
        }
        if let Some(w) = self.worker.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = w.join();
        }
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        self.shutdown();
    }
}
fn input_toggle(c: &mut Control) -> bool {
    c.revision += 1;
    if c.visible {
        c.visible = false;
        c.catalog = false;
        c.dropdown = false;
        c.hide_requested = true;
        c.manual_toggle = false;
        true
    } else {
        c.manual_toggle = true;
        false
    }
}
fn view(c: &Control) -> Value {
    json!({"enabled":c.enabled,"phase":c.phase,"boundary":c.boundary,"visible":c.visible&&c.ready,"catalog":c.catalog,"message":c.message,"gameWrites":false,"playerBinding":false})
}
fn return_focus(window: &tauri::WebviewWindow, frame: Option<&Frame>, id: Option<&Identity>) {
    if let (Some(f), Some(id), Ok(owned)) = (frame, id, window.hwnd()) {
        if unsafe { GetForegroundWindow() }.0 == owned.0 && f.check(id).is_ok() {
            let _ = unsafe { SetForegroundWindow(HWND(f.hwnd as _)) };
        }
    }
}
// Called only from the window owner thread after latest-state projection.
// Compact presentation uses native ShowWindow, so physical visibility and the
// actual hide use Win32 too, without a second queued framework operation.
fn owned_overlay_hwnd(window: &tauri::WebviewWindow) -> Result<HWND> {
    let hwnd = HWND(window.hwnd()?.0);
    let mut pid = 0;
    let thread = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    ensure!(
        pid == std::process::id()
            && thread == unsafe { windows::Win32::System::Threading::GetCurrentThreadId() },
        "Overlay window ownership changed"
    );
    Ok(hwnd)
}
fn hide_owned_overlay_now(window: &tauri::WebviewWindow) -> Result<()> {
    let hwnd = owned_overlay_hwnd(window)?;
    ensure_physical_hidden(
        || unsafe { IsWindowVisible(hwnd) }.as_bool(),
        || {
            let _ = unsafe { ShowWindow(hwnd, SW_HIDE) };
        },
    )
}
fn ensure_physical_hidden(mut visible: impl FnMut() -> bool, hide: impl FnOnce()) -> Result<()> {
    if visible() {
        hide();
    }
    ensure!(!visible(), "Overlay window remained visible after hide");
    Ok(())
}
fn physical_visibility(window: &tauri::WebviewWindow) -> Result<bool> {
    Ok(unsafe { IsWindowVisible(owned_overlay_hwnd(window)?) }.as_bool())
}
fn publish(app: &AppHandle, inner: &Arc<Inner>, mut next: Control) {
    {
        let mut current = inner.control.lock().unwrap_or_else(|e| e.into_inner());
        if !commit(&mut current, &mut next) {
            return;
        }
    }
    let _ = record_diagnostic(inner, &next, None, None);
    let handle = app.clone();
    let shared = inner.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(current) = render_state(
            &shared.control.lock().unwrap_or_else(|e| e.into_inner()),
            next.epoch,
            next.revision,
        ) else {
            return;
        };
        let _ = handle.emit("native-overlay-state", view(&current));
        let Some(window) = handle.get_webview_window("live-overlay") else {
            return;
        };
        let result = (|| -> Result<()> {
            let own = owned_overlay_hwnd(&window)?.0 as u64;
            // Never leave an already-visible topmost panel over another app.
            // Logical intent survives focus loss; physical visibility does not.
            if !may_show(&current, render_foreground(&current, own)) {
                return_focus(&window, current.frame.as_ref(), current.identity.as_ref());
                hide_owned_overlay_now(&window)?;
                return Ok(());
            }
            let frame = current.frame.as_ref().context("No game frame")?;
            frame.check(current.identity.as_ref().context("No game identity")?)?;
            let bounds = frame.bounds(current.catalog && !current.dropdown, current.bar_height)?;
            let hwnd = owned_overlay_hwnd(&window)?;
            let was_focusable =
                unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32 & WS_EX_NOACTIVATE.0 == 0;
            if was_focusable != current.catalog {
                window.set_focusable(current.catalog)?;
            }
            if !current.catalog {
                return_focus(&window, current.frame.as_ref(), current.identity.as_ref());
            }
            let position = PhysicalPosition::new(bounds.x, bounds.y);
            let size = PhysicalSize::new(bounds.width, bounds.height);
            if window.outer_position()? != position {
                window.set_position(position)?;
            }
            if window.inner_size()? != size {
                window.set_size(size)?;
            }
            let latest = shared
                .control
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            if latest.epoch != current.epoch
                || latest.catalog != current.catalog
                || !wanted_visible(&latest)
            {
                hide_owned_overlay_now(&window)?;
                return Ok(());
            }
            if !physical_visibility(&window)? {
                // Never pop up over another app; the worker re-publishes when
                // the game or overlay is in front again.
                if !may_show(&current, render_foreground(&current, own)) {
                    return Ok(());
                }
                if current.catalog {
                    // Keep framework focus bookkeeping in sync, but complete
                    // the physical show on this owner thread as for the bar.
                    window.show()?;
                    let _ = unsafe { ShowWindow(owned_overlay_hwnd(&window)?, SW_SHOW) };
                    window.set_focus()?;
                } else {
                    let _ = unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
                }
            } else if current.catalog && !was_focusable {
                window.set_focus()?;
            }
            if !current.catalog {
                unsafe {
                    SetWindowPos(
                        hwnd,
                        Some(HWND_TOPMOST),
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    )?
                };
            }
            Ok(())
        })();
        let window_error = result.as_ref().err().map(|e| format!("{e:#}"));
        if let Err(e) = result {
            let _ = hide_owned_overlay_now(&window);
            let mut c = shared.control.lock().unwrap_or_else(|p| p.into_inner());
            if c.epoch == current.epoch && c.revision == current.revision {
                c.revision += 1;
                c.visible = false;
                c.phase = "held".into();
                c.message = format!("Native overlay window held: {e:#}");
            }
        }
        let observed = shared
            .control
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let _ = record_diagnostic(
            &shared,
            &observed,
            physical_visibility(&window).ok(),
            window_error.as_deref(),
        );
    });
}
fn render_state(current: &Control, epoch: u64, _revision: u64) -> Option<Control> {
    // Revisions protect commits, not presentation. Render the latest state:
    // skipping a superseded callback can discard the only actionable hide.
    (current.epoch == epoch).then(|| current.clone())
}
#[derive(Clone)]
struct FocusHide {
    epoch: u64,
    identity: Option<Identity>,
    foreground_hwnd: u64,
    foreground_pid: u32,
}
fn foreground_now() -> (u64, u32, u64) {
    let hwnd = unsafe { GetForegroundWindow() };
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    let root = unsafe { GetAncestor(hwnd, GA_ROOTOWNER) }.0 as u64;
    (hwnd.0 as u64, pid, root)
}
fn foreground_sample_current(sample: &ForegroundSample) -> bool {
    let (hwnd, pid, _) = foreground_now();
    sample.hwnd == hwnd && sample.pid == pid
}
fn focus_hide_current(
    c: &Control,
    ticket: &FocusHide,
    own: u64,
    hwnd: u64,
    pid: u32,
    root: u64,
) -> bool {
    c.enabled
        && c.epoch == ticket.epoch
        && c.identity == ticket.identity
        && hwnd == ticket.foreground_hwnd
        && pid == ticket.foreground_pid
        && !classify_foreground(
            hwnd,
            pid,
            root,
            false,
            c.frame.as_ref().map(|f| f.hwnd),
            c.identity.as_ref().map(|id| id.pid),
            Some(own),
            std::process::id(),
        )
        .ours()
}
fn owned_overlay_target(own: u64, current: Option<u64>, pid: u32, owner_pid: u32) -> bool {
    own != 0 && current == Some(own) && pid == owner_pid
}
fn claim_focus_hide(pending: &AtomicBool) -> bool {
    pending
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
}
fn schedule_focus_hide(
    app: &AppHandle,
    shared: &Arc<Inner>,
    c: &Control,
    own: Option<u64>,
    sample: &ForegroundSample,
) {
    let Some(own) = own else {
        return;
    };
    if sample.kind.ours()
        || !unsafe { IsWindowVisible(HWND(own as _)) }.as_bool()
        || !claim_focus_hide(&shared.focus_hide_pending)
    {
        return;
    }
    let ticket = FocusHide {
        epoch: c.epoch,
        identity: c.identity.clone(),
        foreground_hwnd: sample.hwnd,
        foreground_pid: sample.pid,
    };
    let state = shared.clone();
    let handle = app.clone();
    let queued = app.run_on_main_thread(move || {
        let current = state
            .control
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let (hwnd, pid, root) = foreground_now();
        let current_overlay = handle
            .get_webview_window("live-overlay")
            .and_then(|window| window.hwnd().ok())
            .map(|hwnd| hwnd.0 as u64);
        let mut target_pid = 0;
        unsafe { GetWindowThreadProcessId(HWND(own as _), Some(&mut target_pid)) };
        if owned_overlay_target(own, current_overlay, target_pid, std::process::id())
            && focus_hide_current(&current, &ticket, own, hwnd, pid, root)
        {
            // Executed on the actual window owner thread: no delayed async
            // SW_HIDE can survive this fresh foreground proof and run later.
            let _ = unsafe { ShowWindow(HWND(own as _), SW_HIDE) };
        }
        state.focus_hide_pending.store(false, Ordering::Release);
    });
    if queued.is_err() {
        shared.focus_hide_pending.store(false, Ordering::Release);
    }
}
// The render step classifies like the worker without a process scan; a
// WebView2 child the worker already recognised keeps counting as the overlay.
fn render_foreground(c: &Control, own: u64) -> Foreground {
    let fg = unsafe { GetForegroundWindow() };
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(fg, Some(&mut pid)) };
    let root = unsafe { GetAncestor(fg, GA_ROOTOWNER) }.0 as u64;
    let kind = classify_foreground(
        fg.0 as u64,
        pid,
        root,
        false,
        c.frame.as_ref().map(|f| f.hwnd),
        c.identity.as_ref().map(|i| i.pid),
        Some(own),
        std::process::id(),
    );
    retained_render_foreground(c, kind, pid, fg.0 as u64)
}
fn retained_render_foreground(
    c: &Control,
    kind: Foreground,
    pid: u32,
    foreground_hwnd: u64,
) -> Foreground {
    // The worker's child classification applies only to the SAME foreground
    // process AND the same foreground HWND. Another window in the same
    // browser process cannot inherit a popup classification.
    if kind == Foreground::Other
        && pid != 0
        && pid == c.foreground_pid
        && c.foreground == "overlay"
        && foreground_hwnd != 0
        && foreground_hwnd == c.foreground_hwnd
    {
        Foreground::Overlay
    } else {
        kind
    }
}
fn may_show(c: &Control, foreground: Foreground) -> bool {
    wanted_visible(c) && foreground.ours()
}
fn wanted_visible(c: &Control) -> bool {
    c.enabled
        && c.ready
        && c.visible
        && matches!(c.boundary.as_str(), "selection" | "match")
        && c.frame.is_some()
}
fn observation_failed(c: &mut Control, error: &anyhow::Error, within_grace: bool) {
    c.observation = None;
    c.observation_error = Some(format!("{error:#}").chars().take(1024).collect());
    // The observed QA070 flips were this exact read-only capture race. A
    // different failure is never classified from a substring or hidden.
    let mode_race = error
        .chain()
        .any(|cause| cause.to_string() == "Mode changed during observation; sample discarded");
    let keep_status = within_grace
        && mode_race
        && c.enabled
        && c.ready
        && c.phase == "bound"
        && c.identity.is_some()
        && c.frame.is_some();
    if !keep_status {
        c.phase = "held".into();
        c.message = format!("Native F8 held: {error:#}");
    }
    if !within_grace {
        c.visible = false;
        c.frame = None;
        c.identity = None;
    }
}
fn record_diagnostic(
    inner: &Inner,
    c: &Control,
    visible: Option<bool>,
    error: Option<&str>,
) -> Result<bool> {
    let mut saved = inner
        .diagnostics
        .lock()
        .map_err(|_| anyhow::anyhow!("Overlay diagnostics unavailable"))?;
    let physical = visible
        .map(Value::Bool)
        .or_else(|| saved.1.as_ref().map(|s| s["windowVisible"].clone()))
        .unwrap_or(Value::Null);
    let value = json!({"schema":"nba2k27-native-overlay-state-v1","state":view(c),"uiReady":c.ready,"epoch":c.epoch,
        "gamePid":c.identity.as_ref().map(|i|i.pid),"foreground":c.foreground,"fgPid":c.foreground_pid,"fgHwnd":c.foreground_hwnd,"fgExe":c.foreground_exe,"windowVisible":physical,"windowError":error.map(|s|s.chars().take(1024).collect::<String>()),"observation":c.observation,"observationError":c.observation_error});
    if saved.1.as_ref() == Some(&value) {
        return Ok(false);
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    // Bounded local history retains the reason and sampled context, so a
    // later healthy observation cannot erase why a held/waiting flip occurred.
    let entry = json!({"atMs":now_ms,"visible":value["state"]["visible"],"phase":value["state"]["phase"],
        "message":c.message.chars().take(512).collect::<String>(),"observation":c.observation,"observationError":c.observation_error,
        "boundary":value["state"]["boundary"],"catalog":value["state"]["catalog"],"foreground":c.foreground,
        "fgPid":c.foreground_pid,"fgHwnd":c.foreground_hwnd,"fgExe":c.foreground_exe,"windowVisible":physical});
    if saved.2.back().is_none_or(|last| {
        [
            "visible",
            "phase",
            "message",
            "observation",
            "observationError",
            "boundary",
            "catalog",
            "foreground",
            "fgPid",
            "fgHwnd",
            "windowVisible",
        ]
        .iter()
        .any(|k| last[*k] != entry[*k])
    }) {
        if saved.2.len() >= 60 {
            saved.2.pop_front();
        }
        saved.2.push_back(entry);
    }
    let path = &saved.0;
    let logs = path.parent().context("Overlay log folder missing")?;
    let _root =
        crate::loader_files::Directories::pin(logs.parent().context("Overlay root missing")?)?;
    let _logs = crate::loader_files::Directories::pin(logs)?;
    let mut stored = value.clone();
    stored["recordedAtMs"] = json!(now_ms);
    stored["transitions"] = json!(saved.2.iter().collect::<Vec<_>>());
    crate::backend::atomic_json(path, &stored)?;
    saved.1 = Some(value);
    Ok(true)
}
/// Everything the render step or the UI reads; foreground pid/exe and the UI
/// heartbeat are diagnostics only.
fn presentation_unchanged(current: &Control, next: &Control) -> bool {
    current.epoch == next.epoch
        && current.enabled == next.enabled
        && current.ready == next.ready
        && current.catalog == next.catalog
        && current.dropdown == next.dropdown
        && current.bar_height == next.bar_height
        && current.visible == next.visible
        && current.frame == next.frame
        && current.identity == next.identity
        && current.boundary == next.boundary
        && current.message == next.message
        && current.phase == next.phase
        && current.foreground == next.foreground
        && current.foreground_hwnd == next.foreground_hwnd
        && current.manual_toggle == next.manual_toggle
        && current.hide_requested == next.hide_requested
}
fn commit(current: &mut Control, next: &mut Control) -> bool {
    if current.epoch != next.epoch || current.revision != next.revision {
        return false;
    }
    next.revision += 1;
    *current = next.clone();
    true
}
// WPF (BlacktopTool.cs) compared the foreground window's process with the
// game's. The overlay's own window, windows it owns (WebView2 <select> lists)
// and WebView2 child processes of this launcher are part of the game for this
// test, so using the overlay never hides it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Foreground {
    Game,
    Overlay,
    Other,
    None,
}
impl Foreground {
    fn label(self) -> &'static str {
        match self {
            Self::Game => "game",
            Self::Overlay => "overlay",
            Self::Other => "other",
            Self::None => "none",
        }
    }
    fn ours(self) -> bool {
        matches!(self, Self::Game | Self::Overlay)
    }
}
#[allow(clippy::too_many_arguments)]
fn classify_foreground(
    hwnd: u64,
    pid: u32,
    root_owner: u64,
    webview_child: bool,
    game_hwnd: Option<u64>,
    game_pid: Option<u32>,
    own_hwnd: Option<u64>,
    own_pid: u32,
) -> Foreground {
    if hwnd == 0 {
        return Foreground::None;
    }
    if Some(hwnd) == game_hwnd || (pid != 0 && Some(pid) == game_pid) {
        return Foreground::Game;
    }
    if own_hwnd.is_some_and(|own| own == hwnd || own == root_owner)
        || (pid != own_pid && webview_child)
    {
        return Foreground::Overlay;
    }
    Foreground::Other
}
// Display grace must never authorize actions from a raw transition or an
// unsuccessful observer read. Feature executors also validate their own context.
fn raw_action_boundary(sample: Option<&blacktop_native::Snapshot>) -> bool {
    sample.is_some_and(|sample| matches!(sample.boundary.as_str(), "selection" | "match"))
}
// The game briefly reports a different current mode while its alternate mode
// remains the same match. Preserve presentation for that narrow transition,
// never the raw Snapshot or any game-write authorization. Real menus and
// selection take effect immediately; unknown/long transitions expire.
const MATCH_TRANSITION_GRACE: Duration = Duration::from_millis(1500);
#[derive(Default)]
struct BoundaryDisplay {
    last_match: Option<(Identity, String, Instant)>,
}
impl BoundaryDisplay {
    fn observe<'a>(
        &mut self,
        identity: &Identity,
        sample: &'a blacktop_native::Snapshot,
        now: Instant,
    ) -> &'a str {
        if self
            .last_match
            .as_ref()
            .is_some_and(|(id, _, _)| id != identity)
        {
            self.last_match = None;
        }
        if sample.boundary == "match" {
            self.last_match = Some((identity.clone(), sample.current_hash.clone(), now));
        } else if sample.boundary == "transition" && sample.stage == 1 {
            if self.last_match.as_ref().is_some_and(|(_, hash, observed)| {
                sample.alternate_hash == *hash
                    && sample.current_hash != sample.alternate_hash
                    && now.saturating_duration_since(*observed) < MATCH_TRANSITION_GRACE
            }) {
                return "match";
            }
            self.last_match = None;
        } else {
            self.last_match = None;
        }
        &sample.boundary
    }
}
// A brief foreground change (popup, IME, focus hand-off between the game and
// the overlay) must not hide and re-show the bar. Loss counts after a grace.
const FOCUS_LOSS_GRACE: Duration = Duration::from_millis(350);
// WPF BlacktopAutoOpen: "Unknown/transient state must not re-arm a dismissed
// panel" -- and a transient read error must not hide a shown one either.
const READ_ERROR_GRACE: Duration = Duration::from_millis(1500);
struct FocusFilter {
    lost_since: Option<Instant>,
    limit: Duration,
}
impl Default for FocusFilter {
    fn default() -> Self {
        Self::with(FOCUS_LOSS_GRACE)
    }
}
impl FocusFilter {
    fn with(limit: Duration) -> Self {
        Self {
            lost_since: None,
            limit,
        }
    }
    /// True while `ours` holds, or has failed for less than the grace.
    fn update(&mut self, ours: bool, now: Instant) -> bool {
        if ours {
            self.lost_since = None;
            return true;
        }
        let since = *self.lost_since.get_or_insert(now);
        now.saturating_duration_since(since) < self.limit
    }
}
struct ForegroundSample {
    hwnd: u64,
    kind: Foreground,
    pid: u32,
    exe: String,
}
/// Per foreground pid: (owned WebView2 descendant, exe file name).
#[derive(Default)]
struct ProcessCache {
    window: u64,
    entries: std::collections::HashMap<u32, (bool, String, Option<Identity>)>,
}
impl ProcessCache {
    fn new() -> Self {
        Self::default()
    }
    fn clear(&mut self) {
        self.entries.clear();
        self.window = 0;
    }
    fn observe_window(&mut self, hwnd: u64) {
        if self.window != hwnd {
            self.clear();
            self.window = hwnd;
        }
    }
}
fn same_webview_identity(cached: Option<&Identity>, current: Option<&Identity>) -> bool {
    cached.zip(current).is_some_and(|(cached, current)| {
        cached == current
            && Path::new(&current.path).file_name().is_some_and(|name| {
                name.to_string_lossy()
                    .eq_ignore_ascii_case("msedgewebview2.exe")
            })
    })
}
fn launcher_descendant(
    pid: u32,
    own_pid: u32,
    parents: &std::collections::HashMap<u32, u32>,
) -> bool {
    let mut at = pid;
    for _ in 0..8 {
        match parents.get(&at) {
            Some(&parent) if parent == own_pid => return true,
            Some(&parent) if parent != 0 && parent != at => at = parent,
            _ => return false,
        }
    }
    false
}
// External programs (Steam, browsers, helpers) may also descend from this
// launcher. Only the actual WebView2 host qualifies for the popup exception.
fn owned_webview_child(
    pid: u32,
    own_pid: u32,
    parents: &std::collections::HashMap<u32, u32>,
    exe: &str,
) -> bool {
    exe.eq_ignore_ascii_case("msedgewebview2.exe") && launcher_descendant(pid, own_pid, parents)
}
fn read_foreground(
    game_hwnd: Option<u64>,
    game_pid: Option<u32>,
    own_hwnd: Option<u64>,
    cache: &mut ProcessCache,
) -> ForegroundSample {
    let fg = unsafe { GetForegroundWindow() };
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(fg, Some(&mut pid)) };
    let root = unsafe { GetAncestor(fg, GA_ROOTOWNER) }.0 as u64;
    let own_pid = std::process::id();
    cache.observe_window(fg.0 as u64);
    if cache.entries.len() > 64 {
        cache.clear();
    }
    // One process snapshot per new foreground pid: exe name for the log and
    // whether it is in this launcher's tree (WebView2 browser/renderer/GPU).
    let (child, exe, birth) = if pid == 0 {
        (false, String::new(), None)
    } else {
        cache
            .entries
            .entry(pid)
            .or_insert_with(|| {
                let list = session_safety::processes().unwrap_or_default();
                let parents = list.iter().map(|p| (p.pid, p.parent)).collect();
                let name: String = list
                    .iter()
                    .find(|p| p.pid == pid)
                    .map(|p| p.name.chars().take(64).collect())
                    .unwrap_or_default();
                let child = owned_webview_child(pid, own_pid, &parents, &name);
                let birth = child.then(|| session_safety::identity(pid).ok()).flatten();
                (child, name, birth)
            })
            .clone()
    };
    let kind = classify_foreground(
        fg.0 as u64,
        pid,
        root,
        child && same_webview_identity(birth.as_ref(), session_safety::identity(pid).ok().as_ref()),
        game_hwnd,
        game_pid,
        own_hwnd,
        own_pid,
    );
    ForegroundSample {
        hwnd: fg.0 as u64,
        kind,
        pid,
        exe,
    }
}
// WPF BlacktopPanel kept `wanted` while the game was in the background
// (focusSuspended) and presented again on return. A minimized fullscreen game
// or a transient read error is not a user hide, so the policy survives it;
// only an ended session resets it (handled in the worker).
// A panel stays open unless the boundary left Blacktop, the user hid/toggled,
// or a match start auto-hid it while the game was in front.
fn keep_panel(
    allowed: bool,
    visible: bool,
    foreground: bool,
    user_hide: bool,
    boundary: &str,
    match_start: bool,
) -> bool {
    !match_start && allowed && (visible || !(user_hide || (boundary == "match" && foreground)))
}
// A hidden WebView2 throttles its timers, so a missing heartbeat only counts
// while the UI is loading or on screen.
fn ui_stalled(c: &Control, now: Instant) -> bool {
    (!c.ready || c.visible) && ui_expired(c, now)
}
fn ui_expired(c: &Control, now: Instant) -> bool {
    now.saturating_duration_since(c.last_ui.unwrap_or(c.enabled_since)) >= Duration::from_secs(15)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unchanged_samples_are_not_republished_but_visible_changes_are() {
        let mut c = Control::default();
        c.enabled = true;
        c.ready = true;
        c.visible = true;
        c.boundary = "selection".into();
        c.phase = "bound".into();
        let mut next = c.clone();
        // Diagnostics-only fields and the heartbeat revision do not count.
        next.revision += 3;
        next.foreground_pid = 42;
        next.foreground_exe = "nba2k27.exe".into();
        next.last_ui = Some(Instant::now());
        assert!(presentation_unchanged(&c, &next));
        for change in [
            |n: &mut Control| n.visible = false,
            |n: &mut Control| n.catalog = true,
            |n: &mut Control| n.bar_height += 1,
            |n: &mut Control| n.boundary = "match".into(),
            |n: &mut Control| n.message = "held".into(),
            |n: &mut Control| n.foreground = "other",
            |n: &mut Control| n.foreground_hwnd = 99,
            |n: &mut Control| n.hide_requested = true,
            |n: &mut Control| {
                n.frame = Some(Frame {
                    hwnd: 1,
                    x: 0,
                    y: 0,
                    width: 10,
                    height: 10,
                })
            },
        ] {
            let mut n = c.clone();
            change(&mut n);
            assert!(!presentation_unchanged(&c, &n));
        }
    }
    #[test]
    fn hidden_overlay_projection_survives_a_later_heartbeat_revision() {
        let mut current = Control::default();
        current.enabled = true;
        current.ready = true;
        current.visible = false;
        let queued_epoch = current.epoch;
        let queued_revision = current.revision;
        // A heartbeat/layout can arrive after Hide or focus loss, before the
        // main thread drains its render callback. The hidden state must render.
        current.revision += 1;
        current.last_ui = Some(Instant::now());
        let rendered = render_state(&current, queued_epoch, queued_revision)
            .expect("Latest hide was discarded by a heartbeat");
        assert!(!rendered.visible);
    }
    #[test]
    fn overlay_projection_uses_latest_hide_end_or_layout_but_never_crosses_enable_epochs() {
        let mut c = Control::default();
        c.enabled = true;
        c.ready = true;
        c.visible = true;
        let old = (c.epoch, c.revision);
        c.revision += 1;
        c.visible = false;
        c.hide_requested = true;
        assert!(!render_state(&c, old.0, old.1).unwrap().visible);
        c.revision += 1;
        c.enabled = false;
        c.phase = "ended".into();
        let ended = render_state(&c, old.0, old.1).unwrap();
        assert!(!ended.enabled && !ended.visible);
        c.epoch += 1;
        c.enabled = true;
        c.visible = true;
        assert!(render_state(&c, old.0, old.1).is_none());
        c.catalog = true;
        c.revision += 1;
        assert!(render_state(&c, c.epoch, 0).unwrap().catalog);
    }
    #[test]
    fn overlay_projection_refuses_other_foreground_even_with_a_pending_manual_toggle() {
        let mut c = Control::default();
        c.enabled = true;
        c.ready = true;
        c.visible = true;
        c.manual_toggle = true;
        c.boundary = "selection".into();
        c.frame = Some(Frame {
            hwnd: 10,
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        });
        assert!(may_show(&c, Foreground::Game));
        assert!(may_show(&c, Foreground::Overlay));
        for foreground in [Foreground::None, Foreground::Other] {
            assert!(!may_show(&c, foreground));
        }
        c.boundary = "quick-play".into();
        assert!(!may_show(&c, Foreground::Game));
        c.boundary = "selection".into();
        c.enabled = false;
        assert!(!may_show(&c, Foreground::Game));
        c.enabled = true;
        c.ready = false;
        assert!(!may_show(&c, Foreground::Game));
        c.ready = true;
        c.visible = false;
        assert!(!may_show(&c, Foreground::Game));
    }
    #[test]
    fn overlay_diagnostics_are_local_deduplicated_and_record_desired_vs_physical_visibility() {
        let root =
            std::env::temp_dir().join(format!("overlay-log-{}", crate::backend::unique_id()));
        std::fs::create_dir_all(root.join("Logs")).unwrap();
        let path = root.join("Logs/native-overlay-state.json");
        let inner = Inner {
            control: Mutex::new(Control::default()),
            stop: Mutex::new(false),
            wake: Condvar::new(),
            diagnostics: Mutex::new((path.clone(), None, Default::default())),
            auto_enable: Mutex::new(None),
            focus_hide_pending: AtomicBool::new(false),
        };
        let mut c = Control::default();
        c.enabled = true;
        c.ready = true;
        c.visible = true;
        c.boundary = "selection".into();
        assert!(record_diagnostic(&inner, &c, Some(true), None).unwrap());
        c.revision += 1;
        assert!(!record_diagnostic(&inner, &c, None, None).unwrap());
        c.visible = false;
        c.phase = "waiting-for-focus".into();
        assert!(record_diagnostic(&inner, &c, Some(false), None).unwrap());
        let j: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(j["windowVisible"], false);
        assert_eq!(j["state"]["visible"], false);
        assert_eq!(j["state"]["phase"], "waiting-for-focus");
        assert!(j.get("foregroundWindowTitle").is_none());
        // Transitions are timestamped and bounded, with a foreground class only.
        let t = j["transitions"].as_array().unwrap();
        assert_eq!(t.len(), 2);
        assert!(t[1]["atMs"].as_u64().unwrap() >= t[0]["atMs"].as_u64().unwrap());
        assert_eq!(t[1]["phase"], "waiting-for-focus");
        assert_eq!(t[1]["foreground"], "none");
        assert_eq!(t[1]["fgPid"], 0);
        assert_eq!(t[1]["fgExe"], "");
        c.observation = Some(blacktop_native::Snapshot {
            boundary: "selection".into(),
            required_players: 2,
            selected_players: 2,
            loaded_players: 2,
            lineup_ready: true,
            players_ready: true,
            current_hash: "92DF06DE".into(),
            alternate_hash: "92DF06DE".into(),
            stage: 3,
            writes: 0,
        });
        c.message = "x".repeat(600);
        assert!(record_diagnostic(&inner, &c, None, None).unwrap());
        c.phase = "held".into();
        c.message = "Native F8 held: Mode changed during observation; sample discarded".into();
        c.observation = None; // Failed samples never appear as fresh contexts.
        assert!(record_diagnostic(&inner, &c, None, None).unwrap());
        let j: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let t = j["transitions"].as_array().unwrap();
        assert_eq!(t.len(), 4);
        assert_eq!(t[2]["message"].as_str().unwrap().len(), 512);
        assert_eq!(t[2]["observation"]["currentHash"], "92DF06DE");
        assert_eq!(t[2]["observation"]["selectedPlayers"], 2);
        assert_eq!(t[3]["message"], c.message);
        assert!(t[3]["observation"].is_null());
        for n in 0..100 {
            c.visible = n % 2 == 0;
            c.revision += 1;
            record_diagnostic(&inner, &c, Some(c.visible), None).unwrap();
        }
        let j: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(j["transitions"].as_array().unwrap().len(), 60);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn foreground_counts_game_process_overlay_and_its_popups_as_ours() {
        let own_pid = 500;
        let c = |hwnd, pid, root, child| {
            classify_foreground(
                hwnd,
                pid,
                root,
                child,
                Some(10),
                Some(42),
                Some(20),
                own_pid,
            )
        };
        assert_eq!(c(10, 42, 10, false), Foreground::Game);
        // Another window of the game process (WPF compared process ids).
        assert_eq!(c(11, 42, 11, false), Foreground::Game);
        assert_eq!(c(20, own_pid, 20, false), Foreground::Overlay);
        // A WebView2 <select> list owned by the overlay window.
        assert_eq!(c(30, 900, 20, false), Foreground::Overlay);
        // A validated WebView2 host in this launcher's tree.
        assert_eq!(c(31, 901, 10, true), Foreground::Overlay);
        assert_eq!(c(31, 901, 31, true), Foreground::Overlay);
        // The launcher's own main window and other apps are not the game.
        assert_eq!(c(40, own_pid, 40, false), Foreground::Other);
        assert_eq!(c(50, 7, 50, false), Foreground::Other);
        assert_eq!(c(0, 0, 0, false), Foreground::None);
        // Process tree: renderer -> browser -> launcher, at any depth; an
        // unrelated app (e.g. claude.exe under explorer) is not ours.
        let parents: std::collections::HashMap<u32, u32> =
            [(901, 900), (900, own_pid), (7, 3), (3, 1), (1, 0), (66, 66)]
                .into_iter()
                .collect();
        assert!(launcher_descendant(900, own_pid, &parents));
        assert!(launcher_descendant(901, own_pid, &parents));
        assert!(!launcher_descendant(7, own_pid, &parents));
        assert!(!launcher_descendant(66, own_pid, &parents));
        assert!(!launcher_descendant(12345, own_pid, &parents));
    }
    #[test]
    fn match_presentation_tolerates_only_bounded_same_session_transition() {
        let id = Identity {
            pid: 42,
            started: "birth1".into(),
            path: "game.exe".into(),
        };
        let mut sample = blacktop_native::Snapshot {
            boundary: "match".into(),
            required_players: 0,
            selected_players: 0,
            loaded_players: 0,
            lineup_ready: false,
            players_ready: false,
            current_hash: "7C602DF8".into(),
            alternate_hash: "7C602DF8".into(),
            stage: 1,
            writes: 0,
        };
        let mut display = BoundaryDisplay::default();
        let t = Instant::now();
        assert_eq!(display.observe(&id, &sample, t), "match");
        assert!(raw_action_boundary(Some(&sample)));
        assert!(!raw_action_boundary(None));
        sample.boundary = "transition".into();
        sample.current_hash = "B2AF8438".into();
        assert_eq!(
            display.observe(&id, &sample, t + Duration::from_millis(540)),
            "match"
        );
        // Raw observations and command gates retain the unmodified transition.
        assert_eq!(sample.boundary, "transition");
        assert!(!raw_action_boundary(Some(&sample)));
        assert_eq!(sample.writes, 0);
        assert_eq!(
            display.observe(&id, &sample, t + MATCH_TRANSITION_GRACE),
            "transition"
        );
        // An expired transition cannot extend its own grace.
        assert_eq!(
            display.observe(&id, &sample, t + Duration::from_secs(3)),
            "transition"
        );
        sample.boundary = "match".into();
        sample.current_hash = "7C602DF8".into();
        display.observe(&id, &sample, t);
        sample.boundary = "transition".into();
        sample.current_hash = "B2AF8438".into();
        let mut restarted = id.clone();
        restarted.started = "birth2".into();
        assert_eq!(
            display.observe(&restarted, &sample, t + Duration::from_millis(20)),
            "transition"
        );
        sample.boundary = "match".into();
        sample.current_hash = "7C602DF8".into();
        display.observe(&id, &sample, t);
        sample.boundary = "main-menu".into();
        assert_eq!(
            display.observe(&id, &sample, t + Duration::from_millis(1)),
            "main-menu"
        );
        sample.boundary = "transition".into();
        sample.current_hash = "B2AF8438".into();
        assert_eq!(
            display.observe(&id, &sample, t + Duration::from_millis(2)),
            "transition"
        );
        sample.boundary = "match".into();
        sample.current_hash = "7C602DF8".into();
        display.observe(&id, &sample, t);
        sample.boundary = "selection".into();
        assert_eq!(
            display.observe(&id, &sample, t + Duration::from_millis(1)),
            "selection"
        );
        sample.boundary = "transition".into();
        sample.current_hash = "B2AF8438".into();
        assert_eq!(
            display.observe(&id, &sample, t + Duration::from_millis(2)),
            "transition"
        );
        sample.boundary = "match".into();
        sample.current_hash = "7C602DF8".into();
        display.observe(&id, &sample, t);
        sample.boundary = "transition".into();
        sample.current_hash = "B2AF8438".into();
        sample.alternate_hash = "079964C0".into();
        assert_eq!(
            display.observe(&id, &sample, t + Duration::from_millis(2)),
            "transition"
        );
    }
    #[test]
    fn unchanged_presentation_still_commits_fresh_raw_action_gate() {
        let mut current = Control::default();
        current.boundary = "match".into();
        current.observation = Some(blacktop_native::Snapshot {
            boundary: "match".into(),
            required_players: 0,
            selected_players: 0,
            loaded_players: 0,
            lineup_ready: false,
            players_ready: false,
            current_hash: "7C602DF8".into(),
            alternate_hash: "7C602DF8".into(),
            stage: 1,
            writes: 0,
        });
        let mut next = current.clone();
        next.observation.as_mut().unwrap().boundary = "transition".into();
        assert!(presentation_unchanged(&current, &next));
        assert!(raw_action_boundary(current.observation.as_ref()));
        assert!(commit(&mut current, &mut next));
        assert_eq!(current.boundary, "match");
        assert!(!raw_action_boundary(current.observation.as_ref()));
    }
    #[test]
    fn unrelated_launcher_children_never_count_as_overlay_foreground() {
        let parents = [(901, 900), (900, 500), (700, 1)].into_iter().collect();
        for exe in [
            "steam.exe",
            "msedge.exe",
            "helper.exe",
            "msedgewebview2.exe.other",
            "",
        ] {
            let child = owned_webview_child(901, 500, &parents, exe);
            assert!(!child, "{exe} must hide the overlay on Alt-Tab");
            assert_eq!(
                classify_foreground(31, 901, 31, child, Some(10), Some(42), Some(20), 500),
                Foreground::Other
            );
        }
        let old = Identity {
            pid: 901,
            started: "birth1".into(),
            path: "C:\\WebView\\msedgewebview2.exe".into(),
        };
        assert!(same_webview_identity(Some(&old), Some(&old)));
        let mut recycled = old.clone();
        recycled.started = "birth2".into();
        assert!(!same_webview_identity(Some(&old), Some(&recycled)));
        recycled.path = "C:\\Apps\\external.exe".into();
        assert!(!same_webview_identity(Some(&old), Some(&recycled)));
        assert!(!same_webview_identity(Some(&old), None));
        let mut cache = ProcessCache::new();
        cache.observe_window(31);
        cache
            .entries
            .insert(901, (true, "msedgewebview2.exe".into(), Some(old)));
        cache.observe_window(31);
        assert_eq!(cache.entries.len(), 1);
        cache.observe_window(40);
        assert!(cache.entries.is_empty());
        cache
            .entries
            .insert(901, (false, "external.exe".into(), None));
        cache.clear();
        assert!(cache.entries.is_empty());
        assert!(owned_webview_child(
            901,
            500,
            &parents,
            "msedgewebview2.exe"
        ));
        assert!(owned_webview_child(
            901,
            500,
            &parents,
            "MSEDGEWEBVIEW2.EXE"
        ));
        assert!(!owned_webview_child(
            700,
            500,
            &parents,
            "msedgewebview2.exe"
        ));
        // Owned overlay root-owner popups continue to work independent of exe.
        assert_eq!(
            classify_foreground(31, 901, 20, false, Some(10), Some(42), Some(20), 500),
            Foreground::Overlay
        );
    }
    #[test]
    fn game_overlay_focus_alternation_never_toggles_and_real_loss_waits_for_grace() {
        let mut f = FocusFilter::default();
        let t = Instant::now();
        let ms = |n| t + Duration::from_millis(n);
        // Game <-> overlay hand-offs: always ours, no toggling.
        for (i, ours) in [true, true, true, true].into_iter().enumerate() {
            assert!(f.update(ours, ms(i as u64 * 40)));
        }
        // A brief foreign foreground (IME, popup) inside the grace stays ours.
        assert!(f.update(false, ms(200)));
        assert!(f.update(false, ms(400)));
        assert!(f.update(true, ms(440)));
        // Sustained loss hides after the grace and returns immediately.
        assert!(f.update(false, ms(1000)));
        assert!(f.update(false, ms(1300)));
        assert!(!f.update(false, ms(1350)));
        assert!(!f.update(false, ms(5000)));
        assert!(f.update(true, ms(5040)));
        // Flapping foreign foreground never accumulates into a hide.
        for n in 0..20u64 {
            assert!(f.update(n % 2 == 0, ms(6000 + n * 40)));
        }
    }
    #[test]
    fn transient_read_errors_keep_the_bar_and_only_persistent_ones_hide_it() {
        let mut held = FocusFilter::with(READ_ERROR_GRACE);
        let t = Instant::now();
        let ms = |n| t + Duration::from_millis(n);
        // The 0.50 log: single "held" samples between good reads.
        assert!(held.update(false, ms(0)));
        assert!(held.update(true, ms(250)));
        assert!(held.update(false, ms(500)));
        assert!(held.update(false, ms(1900)));
        assert!(held.update(true, ms(1950)));
        // Errors for longer than the grace hide it.
        assert!(held.update(false, ms(3000)));
        assert!(!held.update(false, ms(4500)));
    }
    #[test]
    fn render_hides_over_other_apps_while_retaining_user_intent() {
        let mut c = Control::default();
        c.enabled = true;
        c.ready = true;
        c.visible = true;
        c.boundary = "selection".into();
        c.frame = Some(Frame {
            hwnd: 10,
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        });
        // The user's intent survives Alt-Tab, but an already visible window
        // must hide physically when another application becomes foreground.
        assert!(wanted_visible(&c));
        assert!(!may_show(&c, Foreground::Other));
        assert!(may_show(&c, Foreground::Game));
        c.visible = false;
        assert!(!wanted_visible(&c));
    }
    #[test]
    fn stale_overlay_foreground_cannot_authorize_another_application() {
        let mut c = Control::default();
        c.foreground = "overlay";
        c.foreground_pid = 300;
        c.foreground_hwnd = 10;
        c.frame = Some(Frame {
            hwnd: 10,
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        });
        assert_eq!(
            retained_render_foreground(&c, Foreground::Other, 300, 10),
            Foreground::Overlay
        );
        assert_eq!(
            retained_render_foreground(&c, Foreground::Other, 400, 10),
            Foreground::Other
        );
        assert_eq!(
            retained_render_foreground(&c, Foreground::Other, 0, 10),
            Foreground::Other
        );
        assert_eq!(
            retained_render_foreground(&c, Foreground::None, 0, 10),
            Foreground::None
        );
        assert_eq!(
            retained_render_foreground(&c, Foreground::Game, 400, 10),
            Foreground::Game
        );
        // Another window in the same WebView host (including a main-launcher
        // dropdown) cannot inherit this popup's retained classification.
        assert_eq!(
            retained_render_foreground(&c, Foreground::Other, 300, 99),
            Foreground::Other
        );
        assert_eq!(
            retained_render_foreground(&c, Foreground::Other, 300, 40),
            Foreground::Other
        );
    }
    #[test]
    fn transient_mode_race_keeps_only_presentation_and_always_revokes_actions() {
        let mut c = Control::default();
        c.enabled = true;
        c.ready = true;
        c.visible = true;
        c.phase = "bound".into();
        c.message = "Ready".into();
        c.boundary = "selection".into();
        c.frame = Some(Frame {
            hwnd: 10,
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        });
        c.identity = Some(Identity {
            pid: 42,
            started: "100".into(),
            path: r"C:\Game\NBA2K27.exe".into(),
        });
        let good = c.clone();
        let error = anyhow::anyhow!("Mode changed during observation; sample discarded")
            .context("fresh read");
        observation_failed(&mut c, &error, true);
        assert_eq!(c.phase, "bound");
        assert_eq!(c.message, "Ready");
        assert!(c.visible);
        assert!(!raw_action_boundary(c.observation.as_ref()));
        assert!(c.observation_error.as_ref().unwrap().contains("fresh read"));
        observation_failed(&mut c, &error, false);
        assert_eq!(c.phase, "held");
        assert!(!c.visible);
        assert!(c.frame.is_none() && c.identity.is_none());
        let mut identity_failure = good.clone();
        observation_failed(
            &mut identity_failure,
            &anyhow::anyhow!("Game identity changed"),
            true,
        );
        assert_eq!(identity_failure.phase, "held");
        let mut unfocused = good;
        unfocused.phase = "waiting-for-focus".into();
        observation_failed(&mut unfocused, &error, true);
        assert_eq!(unfocused.phase, "held");
    }
    #[test]
    fn queued_focus_hide_after_alt_tab_return_cannot_undo_a_fresh_show() {
        let mut c = Control::default();
        c.enabled = true;
        c.ready = true;
        c.visible = true;
        c.catalog = true;
        c.boundary = "selection".into();
        c.epoch = 4;
        c.identity = Some(Identity {
            pid: 42,
            started: "birth".into(),
            path: "game.exe".into(),
        });
        c.frame = Some(Frame {
            hwnd: 10,
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        });
        // Input observes another app and queues exactly one hide. Before the
        // window owner thread executes it, Alt-Tab restores game and catalog.
        let ticket = FocusHide {
            epoch: c.epoch,
            identity: c.identity.clone(),
            foreground_hwnd: 30,
            foreground_pid: 7,
        };
        let mut physical_visible = may_show(&c, Foreground::Game);
        assert!(physical_visible);
        if focus_hide_current(&c, &ticket, 20, 10, 42, 10) {
            physical_visible = false;
        }
        assert!(
            physical_visible,
            "stale foreign-app hide must not hide returning catalog"
        );
        // The same foreground application still hides immediately at execution.
        assert!(focus_hide_current(&c, &ticket, 20, 30, 7, 30));
        assert!(!focus_hide_current(
            &c,
            &ticket,
            20,
            20,
            std::process::id(),
            10
        ));
        assert!(
            !focus_hide_current(&c, &ticket, 20, 31, 8, 31),
            "another HWND waits for its own fresh classification"
        );
        c.epoch += 1;
        assert!(!focus_hide_current(&c, &ticket, 20, 30, 7, 30));
        c.epoch = ticket.epoch;
        c.identity.as_mut().unwrap().started = "replacement".into();
        assert!(!focus_hide_current(&c, &ticket, 20, 30, 7, 30));
        c.identity = ticket.identity.clone();
        c.enabled = false;
        assert!(!focus_hide_current(&c, &ticket, 20, 30, 7, 30));
    }
    #[test]
    fn queued_focus_hide_rejects_recycled_or_replaced_overlay_targets() {
        assert!(owned_overlay_target(20, Some(20), 700, 700));
        assert!(!owned_overlay_target(20, Some(21), 700, 700));
        assert!(!owned_overlay_target(20, None, 700, 700));
        assert!(!owned_overlay_target(20, Some(20), 701, 700));
        assert!(!owned_overlay_target(20, Some(20), 0, 700));
        assert!(!owned_overlay_target(0, Some(0), 700, 700));
    }
    #[test]
    fn queued_f8_close_renders_a_newer_reopen_instead_of_hiding() {
        let mut c = Control::default();
        c.enabled = true;
        c.ready = true;
        c.visible = true;
        c.boundary = "selection".into();
        c.frame = Some(Frame {
            hwnd: 10,
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        });
        assert!(input_toggle(&mut c));
        let close = (c.epoch, c.revision);
        assert!(!may_show(
            &render_state(&c, close.0, close.1).unwrap(),
            Foreground::Game
        ));
        assert!(!input_toggle(&mut c));
        // The observer has admitted the explicit reopen before the old close
        // callback drains. This is the same latest-state projection as publish.
        c.visible = true;
        c.hide_requested = false;
        c.manual_toggle = false;
        c.revision += 1;
        assert!(may_show(
            &render_state(&c, close.0, close.1).unwrap(),
            Foreground::Game
        ));
        c.epoch += 1;
        assert!(render_state(&c, close.0, close.1).is_none());
    }
    #[test]
    fn queued_ui_stall_hide_renders_a_newer_heartbeat_recovery() {
        let mut c = Control::default();
        c.enabled = true;
        c.ready = true;
        c.visible = true;
        c.boundary = "selection".into();
        c.frame = Some(Frame {
            hwnd: 10,
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        });
        let now = c.enabled_since;
        c.last_ui = Some(now);
        assert!(ui_stalled(&c, now + Duration::from_secs(16)));
        c.visible = false;
        c.phase = "ui-unresponsive".into();
        c.revision += 1;
        let paused = (c.epoch, c.revision);
        assert!(!may_show(
            &render_state(&c, paused.0, paused.1).unwrap(),
            Foreground::Game
        ));
        c.last_ui = Some(now + Duration::from_secs(17));
        c.visible = true;
        c.phase = "ready".into();
        c.revision += 1;
        assert!(!ui_stalled(&c, now + Duration::from_secs(18)));
        assert!(may_show(
            &render_state(&c, paused.0, paused.1).unwrap(),
            Foreground::Game
        ));
        c.enabled = false;
        assert!(!may_show(
            &render_state(&c, paused.0, paused.1).unwrap(),
            Foreground::Game
        ));
    }
    #[test]
    fn focus_hide_queue_coalesces_and_owned_popup_recheck_never_hides() {
        let pending = AtomicBool::new(false);
        assert!(claim_focus_hide(&pending));
        for _ in 0..100 {
            assert!(!claim_focus_hide(&pending));
        }
        pending.store(false, Ordering::Release);
        assert!(claim_focus_hide(&pending));
        let mut c = Control::default();
        c.enabled = true;
        let ticket = FocusHide {
            epoch: c.epoch,
            identity: None,
            foreground_hwnd: 31,
            foreground_pid: 900,
        };
        assert!(focus_hide_current(&c, &ticket, 20, 31, 900, 31));
        // Root ownership changed before execution; do not hide the owned popup.
        assert!(!focus_hide_current(&c, &ticket, 20, 31, 900, 20));
        assert!(!focus_hide_current(&c, &ticket, 20, 31, 901, 31));
    }
    #[test]
    fn hidden_overlay_ui_is_not_expired_by_throttled_heartbeats() {
        let mut c = Control::default();
        let now = c.enabled_since;
        c.ready = true;
        c.last_ui = Some(now);
        c.visible = false;
        assert!(!ui_stalled(&c, now + Duration::from_secs(600)));
        c.visible = true;
        assert!(ui_stalled(&c, now + Duration::from_secs(15)));
        c.ready = false;
        c.visible = false;
        assert!(ui_stalled(&c, now + Duration::from_secs(15)));
    }
    #[test]
    fn missing_or_stalled_ui_readiness_expires_without_game_actions() {
        let mut c = Control::default();
        let now = c.enabled_since;
        assert!(!ui_expired(&c, now + Duration::from_secs(14)));
        assert!(ui_expired(&c, now + Duration::from_secs(15)));
        c.ready = true;
        c.last_ui = Some(now + Duration::from_secs(10));
        assert!(!ui_expired(&c, now + Duration::from_secs(24)));
        assert!(ui_expired(&c, now + Duration::from_secs(25)));
    }
    #[test]
    fn stale_monitor_result_cannot_undo_layout_hide_or_disable() {
        let mut current = Control::default();
        let mut stale = current.clone();
        current.revision += 1;
        current.catalog = true;
        current.hide_requested = true;
        assert!(!commit(&mut current, &mut stale));
        assert!(current.catalog && current.hide_requested);
        let mut stale = current.clone();
        current.epoch += 1;
        assert!(!commit(&mut current, &mut stale));
        let mut fresh = current.clone();
        fresh.ready = true;
        assert!(commit(&mut current, &mut fresh));
        assert!(current.ready);
        assert_eq!(current.revision, 2);
    }
    #[test]
    fn owned_fixture_window_geometry_and_foreign_pid_are_checked() {
        std::thread::spawn(|| unsafe {
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                w!("Owned overlay QA fixture"),
                WS_POPUP,
                30,
                40,
                1280,
                720,
                None,
                None,
                None,
                None,
            )
            .unwrap();
            let frame = fixture_frame(hwnd).unwrap();
            assert_eq!((frame.width, frame.height), (1280, 720));
            assert!(window_frame(hwnd, std::process::id() + 1, false).is_err());
            assert!(window_frame(hwnd, std::process::id(), true).is_err());
            let child = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                w!("Owned child"),
                WS_POPUP,
                30,
                40,
                1200,
                100,
                Some(hwnd),
                None,
                None,
                None,
            )
            .unwrap();
            assert_eq!(GetWindow(child, GW_OWNER).unwrap(), hwnd);
            assert!(GetWindow(hwnd, GW_OWNER).is_err());
            DestroyWindow(child).unwrap();
            DestroyWindow(hwnd).unwrap();
            assert!(fixture_frame(hwnd).is_err());
        })
        .join()
        .unwrap();
    }
    #[test]
    fn bounds_preserve_top_width_negative_monitor_and_catalog_cap() {
        let f = Frame {
            hwnd: 1,
            x: -1920,
            y: 40,
            width: 1920,
            height: 1080,
        };
        let bar = f.bounds(false, 90).unwrap();
        let catalog = f.bounds(true, 90).unwrap();
        assert_eq!(
            (bar.x, bar.y, bar.width),
            (catalog.x, catalog.y, catalog.width)
        );
        assert_eq!((bar.x, bar.y, bar.width, bar.height), (-1908, 52, 1896, 90));
        assert_eq!(catalog.height, 810);
        // A dropdown (arena) keeps the window at bar + menu height, up to 600.
        assert_eq!(f.bounds(false, 388).unwrap().height, 388);
        assert_eq!(f.bounds(false, 900).unwrap().height, 600);
        let small = Frame { height: 400, ..f };
        assert_eq!(small.bounds(true, 90).unwrap().height, 360);
        assert_eq!(
            Frame { height: 240, ..f }.bounds(true, 90).unwrap().height,
            216
        );
        assert!(Frame { width: 20, ..f }.bounds(false, 90).is_err());
    }
    #[test]
    fn overlay_auto_enable_attaches_once_per_exact_game_and_respects_opt_out() {
        let game = |pid: u32, started: &str| Identity {
            pid,
            started: started.into(),
            path: "C:/Games/NBA 2K27/NBA2K27.exe".into(),
        };
        let first = game(40, "100");
        assert!(should_auto_enable(false, true, &first, None));
        assert!(!should_auto_enable(false, false, &first, None));
        assert!(!should_auto_enable(true, true, &first, None));
        // Stopped or timed out in the same process: stay off.
        assert!(!should_auto_enable(false, true, &first, Some(&first)));
        // New Play session, including a reused PID with a new start time.
        assert!(should_auto_enable(
            false,
            true,
            &game(41, "200"),
            Some(&first)
        ));
        assert!(should_auto_enable(
            false,
            true,
            &game(40, "300"),
            Some(&first)
        ));
    }
    const READY: Observation = Observation {
        boundary: "selection",
        lineup_ready: true,
        required: 6,
        selected: 6,
    };
    const LOADING: Observation = Observation {
        boundary: "selection",
        lineup_ready: false,
        required: 0,
        selected: 0,
    };
    const PICKING: Observation = Observation {
        boundary: "selection",
        lineup_ready: false,
        required: 6,
        selected: 4,
    };
    const MATCH: Observation = Observation {
        boundary: "match",
        lineup_ready: false,
        required: 0,
        selected: 0,
    };
    const LOADING_MATCH: Observation = Observation {
        boundary: "transition-or-other-mode",
        lineup_ready: false,
        required: 0,
        selected: 0,
    };
    #[test]
    fn auto_open_waits_for_two_complete_lineup_reads_in_front_once_per_visit() {
        let mut p = Policy::default();
        // Loading models / unknown reads neither open nor re-arm.
        assert!(!p.update(&LOADING, true, true, true, false));
        assert!(!p.update(&READY, true, true, true, false));
        assert!(p.update(&READY, true, true, true, false));
        // A user hide during a complete lineup is respected for the visit.
        assert!(p.update(&READY, true, true, true, true) == false);
        for _ in 0..5 {
            assert!(!p.update(&READY, true, true, true, false));
        }
        // Deselecting a player hides and re-arms; completing it opens again.
        assert!(!p.update(&PICKING, true, true, true, false));
        assert!(!p.update(&READY, true, true, true, false));
        assert!(p.update(&READY, true, true, true, false));
        // Game not in front: the second ready read does not open it...
        let mut q = Policy::default();
        assert!(!q.update(&READY, false, true, true, false));
        assert!(!q.update(&READY, false, true, true, false));
        // ...it opens once the game is in front again.
        assert!(q.update(&READY, true, true, true, false));
        // Setting off: nothing opens automatically, F8 still does.
        let mut r = Policy::default();
        for _ in 0..4 {
            assert!(!r.update(&READY, true, false, false, false));
        }
        assert!(r.update(&READY, true, false, false, true));
    }
    #[test]
    fn confirmed_match_closes_once_even_after_loading_already_ended_the_visit() {
        let mut p = Policy::default();
        p.update(&READY, true, true, true, false);
        assert!(p.update(&READY, true, true, true, false));
        assert!(p.update(&LOADING_MATCH, true, true, true, false));
        assert!(!p.update(&LOADING_MATCH, true, true, true, false));
        assert!(p.match_start("match"));
        assert!(!p.update(&MATCH, true, true, true, false));
        assert!(!p.match_start("match"));
        assert!(p.update(&MATCH, true, true, true, true));
        for _ in 0..5 {
            assert!(p.update(&MATCH, true, true, true, false));
        }
        // Same-match presentation grace delivers the retained match boundary,
        // so it never rearms the entry close.
        assert!(p.update(&MATCH, true, true, true, false));
        p.hide();
        assert!(!p.update(&MATCH, true, true, true, false));
        p.update(&READY, true, true, true, false);
        p.update(&READY, true, true, true, false);
        assert!(p.match_start("match"));
        assert!(!p.update(&MATCH, true, true, true, false));
    }
    #[test]
    fn direct_selection_to_match_closes_on_first_read_and_allows_explicit_f8() {
        let mut p = Policy::default();
        p.update(&READY, true, true, true, false);
        assert!(p.update(&READY, true, true, true, false));
        assert!(!p.update(&MATCH, false, true, true, false));
        assert!(!p.update(&MATCH, true, true, true, false));
        assert!(p.update(&MATCH, true, true, true, true));
        assert!(p.update(&MATCH, true, true, true, false));
    }
    #[test]
    fn explicit_f8_on_first_match_read_is_not_closed_by_second_read() {
        let mut p = Policy::default();
        p.update(&READY, true, true, true, false);
        assert!(p.update(&READY, true, true, true, false));
        // Entry closes first, then this deliberate F8 reopens in the match.
        assert!(p.update(&MATCH, true, true, true, true));
        assert!(p.update(&MATCH, true, true, true, false));
        assert!(p.update(&MATCH, true, true, true, false));
    }
    #[test]
    fn match_entry_clears_selection_catalog_even_when_backgrounded_or_f8_reopens_bar() {
        for foreground in [false, true] {
            for visible in [false, true] {
                assert!(!keep_panel(true, visible, foreground, false, "match", true));
            }
        }
        assert!(keep_panel(true, true, true, false, "match", false));
        assert!(keep_panel(true, false, false, false, "selection", false));
    }
    #[test]
    fn physical_hide_ignores_cached_visibility_and_requires_readback() {
        use std::cell::Cell;
        let physical = Cell::new(true);
        let cached_visible = false;
        let calls = Cell::new(0);
        ensure_physical_hidden(
            || physical.get(),
            || {
                calls.set(calls.get() + 1);
                physical.set(false);
            },
        )
        .unwrap();
        assert!(!cached_visible && !physical.get());
        assert_eq!(calls.get(), 1);
        ensure_physical_hidden(|| physical.get(), || calls.set(calls.get() + 1)).unwrap();
        assert_eq!(calls.get(), 1);
        assert!(ensure_physical_hidden(|| true, || {}).is_err());
    }
    #[test]
    fn match_start_hides_the_bar_and_the_next_selection_brings_it_back() {
        let mut p = Policy::default();
        assert!(!p.update(&READY, true, true, true, false));
        assert!(p.update(&READY, true, true, true, false));
        // One read off selection is not enough (WPF needs two).
        assert!(p.update(&LOADING_MATCH, true, true, true, false));
        assert!(!p.update(&LOADING_MATCH, true, true, true, false));
        assert!(!p.update(&MATCH, true, true, true, false));
        // Alt-Tab away and back in the match: it stays away.
        assert!(!p.update(&MATCH, false, true, true, false));
        assert!(!p.update(&MATCH, true, true, true, false));
        // F8 in a match still opens it, and it stays (the hide is an edge).
        assert!(p.update(&MATCH, true, true, true, true));
        assert!(p.update(&MATCH, true, true, true, false));
        assert!(!p.update(&MATCH, true, true, true, true));
        // Next player selection: auto-open again after two ready reads.
        assert!(!p.update(&LOADING, true, true, true, false));
        assert!(!p.update(&READY, true, true, true, false));
        assert!(p.update(&READY, true, true, true, false));
        // "Automatically hide" off: the bar stays into the match.
        let mut k = Policy::default();
        k.update(&READY, true, true, false, false);
        assert!(k.update(&READY, true, true, false, false));
        assert!(k.update(&MATCH, true, true, false, false));
        assert!(k.update(&MATCH, true, true, false, false));
    }
    #[test]
    fn leaving_blacktop_hides_without_flashing_and_only_selection_brings_it_back() {
        let menu = Observation {
            boundary: "main-menu",
            lineup_ready: false,
            required: 0,
            selected: 0,
        };
        let quick = Observation {
            boundary: "quick-play",
            lineup_ready: false,
            required: 0,
            selected: 0,
        };
        for auto_hide in [true, false] {
            let mut p = Policy::default();
            p.update(&READY, true, true, auto_hide, false);
            assert!(p.update(&READY, true, true, auto_hide, false));
            // Backing out: one transient "transition" read does not flash it
            // off, the second read off Blacktop hides it (even with
            // "Automatically hide" off: menus never show the bar).
            assert!(p.update(&LOADING_MATCH, true, true, auto_hide, false));
            assert!(!p.update(&menu, true, true, auto_hide, false));
            // Alt-Tab away and back on the menu, F8 on the menu, Quick Play:
            // it stays away.
            assert!(!p.update(&menu, false, true, auto_hide, false));
            assert!(!p.update(&menu, true, true, auto_hide, false));
            assert!(!p.update(&menu, true, true, auto_hide, true));
            assert!(!p.update(&quick, true, true, auto_hide, false));
            // Back into Blacktop: a transition read while it is hidden does
            // not flash it on; the selection screen opens it again.
            assert!(!p.update(&LOADING_MATCH, true, true, auto_hide, false));
            assert!(!p.update(&LOADING, true, true, auto_hide, false));
            assert!(!p.update(&READY, true, true, auto_hide, false));
            assert!(p.update(&READY, true, true, auto_hide, false));
            // A single glitch read between selection reads never toggles it.
            for _ in 0..5 {
                assert!(p.update(&LOADING_MATCH, true, true, auto_hide, false));
                assert!(p.update(&READY, true, true, auto_hide, false));
            }
        }
    }
    #[test]
    fn hide_is_sticky_until_f8_or_the_next_selection_visit() {
        let mut p = Policy::default();
        p.update(&READY, true, true, true, false);
        assert!(p.update(&READY, true, true, true, false));
        // "Hide overlay": stays hidden through more complete-lineup reads,
        // loading reads and Alt-Tab.
        p.hide();
        for o in [&READY, &LOADING, &READY, &READY] {
            assert!(!p.update(o, true, true, true, false));
        }
        assert!(!p.update(&READY, false, true, true, false));
        assert!(!p.update(&READY, true, true, true, false));
        // F8 brings it back, and F8 hides it again.
        assert!(p.update(&READY, true, true, true, true));
        assert!(!p.update(&READY, true, true, true, true));
        // Hidden before the auto-open fired: still sticky for this visit.
        let mut q = Policy::default();
        q.update(&READY, true, true, true, false);
        q.hide();
        for _ in 0..4 {
            assert!(!q.update(&READY, true, true, true, false));
        }
        // The next selection visit opens it again.
        let mut r = Policy::default();
        r.update(&READY, true, true, true, false);
        assert!(r.update(&READY, true, true, true, false));
        r.hide();
        assert!(!r.update(&MATCH, true, true, true, false));
        assert!(!r.update(&MATCH, true, true, true, false));
        assert!(!r.update(&LOADING, true, true, true, false));
        assert!(!r.update(&READY, true, true, true, false));
        assert!(r.update(&READY, true, true, true, false));
    }
    #[test]
    fn f8_and_hide_survive_failed_samples() {
        let t = Instant::now();
        let ms = |n| t + Duration::from_millis(n);
        let mut i = UserIntent::default();
        // F8 noticed on a sample that failed; the next good one uses it once.
        i.note(true, ms(0));
        i.note(false, ms(100));
        assert!(i.take(ms(300)));
        assert!(!i.take(ms(340)));
        // A press older than the lifetime is dropped, not replayed later.
        i.note(true, ms(1000));
        assert!(!i.take(ms(3500)));
        // A hide applied before a failed sample is not undone by the next
        // good read (the 0.50 "Hide does nothing" bug).
        let mut p = Policy::default();
        p.update(&READY, true, true, true, false);
        assert!(p.update(&READY, true, true, true, false));
        p.hide();
        assert!(!p.update(&READY, true, true, true, false));
    }
    #[test]
    fn alt_tab_at_selection_restores_the_overlay_and_its_panel() {
        let mut p = Policy::default();
        p.update(&READY, true, true, true, false);
        assert!(p.update(&READY, true, true, true, false));
        // Game in the background (possibly minimized): hidden, still wanted.
        assert!(!p.update(&READY, false, true, true, false));
        assert!(keep_panel(true, false, false, false, "selection", false));
        assert!(p.update(&READY, true, true, true, false));
        // A user hide, a match auto-hide in front, or leaving Blacktop closes it.
        assert!(!keep_panel(true, false, true, true, "selection", false));
        assert!(!keep_panel(true, false, true, false, "match", false));
        assert!(!keep_panel(false, false, true, false, "main-menu", false));
        // Menus and other modes never show it, even with F8.
        let mut q = Policy::default();
        q.update(&LOADING_MATCH, true, true, true, false);
        assert!(!q.update(&LOADING_MATCH, true, true, true, true));
    }
    #[test]
    fn polled_f8_is_edge_triggered_and_never_replays_an_out_of_scope_press() {
        let mut key = Key { down: false };
        assert!(key.sample(true, true));
        assert!(!key.sample(true, true));
        assert!(!key.sample(true, false));
        assert!(key.sample(true, true));
        assert!(!key.sample(false, false));
        assert!(!key.sample(false, true));
        // Returning to the game with F8 still held is not a new toggle.
        assert!(!key.sample(true, true));
        assert!(!key.sample(true, false));
        assert!(key.sample(true, true));
    }
    #[test]
    fn foreground_f8_open_survives_transition_but_expires_without_a_valid_boundary() {
        let now = Instant::now();
        let mut key = Key { down: false };
        let mut intent = UserIntent::default();
        // Foreground input is captured independently of a transient boundary.
        intent.note(key.sample(true, true), now);
        assert!(!intent.take_when_allowed(false, now));
        assert!(intent.take_when_allowed(true, now + Duration::from_millis(250)));
        assert!(!intent.take_when_allowed(true, now + Duration::from_millis(300)));
        intent.note(true, now);
        assert!(!intent.take_when_allowed(false, now + INTENT_LIFETIME));
        assert!(!intent.take_when_allowed(true, now + INTENT_LIFETIME));
    }
    #[test]
    fn f8_hide_is_immediate_and_rejects_a_slow_stale_observation() {
        let mut c = Control::default();
        c.enabled = true;
        c.visible = true;
        c.catalog = true;
        let mut stale = c.clone();
        assert!(input_toggle(&mut c));
        assert!(!c.visible && !c.catalog && c.hide_requested && !c.manual_toggle);
        assert!(!commit(&mut c, &mut stale));
        // A second tap before that read finishes is an open after the queued
        // Hide, not another hide of the stale visible Policy.
        assert!(!input_toggle(&mut c));
        assert!(c.manual_toggle && c.hide_requested);
        let mut p = Policy::default();
        p.update(&READY, true, true, true, false);
        assert!(p.update(&READY, true, true, true, false));
        p.hide();
        assert!(p.update(&READY, true, true, true, c.manual_toggle));
    }
    #[test]
    fn ui_pause_is_recoverable_without_rearming_or_losing_user_hide() {
        let now = Instant::now();
        let mut c = Control::default();
        c.enabled = true;
        c.ready = true;
        c.visible = true;
        c.last_ui = Some(now);
        assert!(ui_stalled(&c, now + Duration::from_secs(16)));
        c.visible = false;
        c.phase = "ui-unresponsive".into();
        assert!(c.enabled && c.ready);
        assert!(ui_expired(&c, now + Duration::from_secs(16)));
        // heartbeat() is still allowed because readiness was not destroyed.
        c.last_ui = Some(now + Duration::from_secs(17));
        assert!(!ui_expired(&c, now + Duration::from_secs(18)));
        let mut p = Policy::default();
        p.update(&READY, true, true, true, false);
        assert!(p.update(&READY, true, true, true, false));
        p.hide();
        // The pause never resets Policy, so a previous explicit Hide sticks.
        assert!(!p.update(&READY, true, true, true, false));
    }
}
