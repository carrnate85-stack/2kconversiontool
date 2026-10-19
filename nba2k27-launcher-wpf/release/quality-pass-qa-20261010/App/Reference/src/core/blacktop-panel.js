'use strict';
const path = require('node:path');
class BlacktopPanel {
  constructor({ BrowserWindow, globalShortcut, screen, sourceDir, onPresent = () => {}, onHide = async () => {}, onError = () => {}, onTrace = () => {} }) {
    Object.assign(this, { BrowserWindow, globalShortcut, screen, sourceDir, onPresent, onHide, onError, onTrace });
    this.window = null; this.wanted = false; this.ready = false; this.disposed = false; this.focusSuspended = false; this.focusHandoff = false; this.focusHandoffTimer = null;
    this.accelerator = 'F8'; this.registered = false; this.hotkeyPresses = 0;
    this.lastError = ''; this.diagnosticError = ''; this.observationTimers = new Set(); this.playerMenuRows = 0;
    this.displayChanged=()=>{if(!this.disposed&&this.window&&!this.window.isDestroyed())this.window.setBounds(this.panelBounds(this.displayArea(),this.expanded));};
    this.screen.on?.('display-metrics-changed',this.displayChanged);
  }
  trace(event, details = {}) {
    const read = callback => { try { return callback() ?? null; } catch (error) { return { error: error.message }; } };
    const win = this.window && !this.window.isDestroyed() ? this.window : null;
    const record = { event, ...details, wanted: this.wanted, ready: this.ready, disposed: this.disposed,
      hotkeyPresses: this.hotkeyPresses, listener: read(() => this.globalShortcut.status?.()),
      window: win ? { id: win.id, visible: read(() => win.isVisible()), focused: read(() => win.isFocused()),
        minimized: read(() => win.isMinimized()), topmost: read(() => win.isAlwaysOnTop?.()),
        bounds: read(() => win.getBounds?.()), loading: read(() => win.webContents.isLoading?.()),
        hwnd: read(() => win.getNativeWindowHandle?.().toString('hex')) } : null,
      launcherFocusedWindowId: read(() => this.BrowserWindow.getFocusedWindow?.()?.id),
      displays: read(() => this.screen.getAllDisplays?.().map(d => ({ id: d.id, bounds: d.bounds, workArea: d.workArea, scaleFactor: d.scaleFactor }))),
      focusSuspended: this.focusSuspended, lastError: this.lastError };
    try { this.onTrace(record); } catch (error) { this.diagnosticError = String(error.message).slice(0, 512); }
  }
  report(error, stage) {
    this.lastError = `${stage}: ${error.message || error}`.slice(0, 1024);
    this.trace('error', { stage, error: this.lastError });
    this.onError(error);
  }
  observeAfterPresent() {
    for (const timer of this.observationTimers) clearTimeout(timer);
    this.observationTimers.clear();
    for (const delay of [100, 750]) {
      const timer = setTimeout(() => {
        this.observationTimers.delete(timer);
        if (!this.disposed) this.trace('present-observation', { afterMs: delay });
      }, delay);
      timer.unref?.(); this.observationTimers.add(timer);
    }
  }
  register() {
    if (this.globalShortcut.status?.().state === 'failed') this.registered = false;
    if (this.disposed || this.registered) return this.status();
    try { this.registered = this.globalShortcut.register(this.accelerator, () => { try { this.hotkeyPresses++; this.trace('hotkey'); this.toggle(); } catch (error) { this.report(error, 'hotkey'); } }); }
    catch (error) { this.report(error, 'register'); }
    this.trace('register');
    return this.status();
  }
  status() { return { hotkey: this.accelerator, registered: this.registered, listener: this.globalShortcut.status?.(), visible: Boolean(this.window?.isVisible()), focused: Boolean(this.window?.isFocused()), focusSuspended: this.focusSuspended, hotkeyPresses: this.hotkeyPresses, ready: this.ready, lastError: this.lastError, diagnosticError: this.diagnosticError }; }
  // Load local UI before a hotkey arrives. No show event, game scan, or focus change.
  prepare() { if (!this.disposed) { this.trace('prepare'); try { this.ensureWindow(); } catch (error) { this.report(error, 'prepare'); } } }
  compactWidth(area) { return Math.max(1, area.width - Math.round(24*(area.uiScale||1))); }
  displayArea() { const display=this.screen.getDisplayNearestPoint(this.screen.getCursorScreenPoint()); return {...(display.bounds||display.workArea),uiScale:this.screen.coordinateSpace==='physical'?display.scaleFactor||1:1}; }
  compactHeight(area) { const scale=area.uiScale||1;return Math.round((area.width/scale - 24 >= 1380 ? 78 : 120)*scale); }
  compactPanelHeight(area) { const scale=area.uiScale||1;return Math.min(this.compactHeight(area) + Math.round((this.playerMenuRows ? this.playerMenuRows * 42 + 16 : 0)*scale), area.height - Math.round(24*scale)); }
  panelBounds(area,expanded){const scale=area.uiScale||1;
    const width=this.compactWidth(area);
    const height=expanded?Math.min(area.height-Math.round(24*scale),Math.max(Math.round(360*scale),Math.round(area.height*.75))):this.compactPanelHeight(area);
    return {x:area.x+Math.round((area.width-width)/2),y:area.y+Math.round(12*scale),width:Math.max(1,width),height:Math.max(1,height)};
  }
  ensureWindow() {
    if (this.window && !this.window.isDestroyed()) return;
    const area = this.displayArea();
    const width = this.compactWidth(area), height = Math.min(this.compactHeight(area), area.height);
    const win = new this.BrowserWindow({
      x: area.x + Math.round((area.width-width)/2), y: area.y + Math.round((area.height-height)/2),
      width, height, minWidth: Math.min(420, width), minHeight: height, resizable: false,
      title: 'Blacktop outfits', frame: false, show: false, alwaysOnTop: true,
      skipTaskbar: true, focusable: false, autoHideMenuBar: true, backgroundColor: '#10151e',
      webPreferences: { preload: path.join(this.sourceDir, 'blacktop-panel-preload.js'),
        contextIsolation: true, nodeIntegration: false, sandbox: true },
    });
    this.window = win; this.ready = false;
    this.trace('window-created');
    for (const event of ['ready-to-show', 'show', 'hide', 'focus', 'blur', 'minimize', 'restore', 'unresponsive', 'responsive']) {
      win.on(event, () => this.trace(`window-${event}`));
    }
    win.webContents.on('dom-ready', () => this.trace('dom-ready'));
    win.webContents.on('did-fail-load', (_event, code, description, _url, isMainFrame) => {
      if (isMainFrame !== false) this.report(new Error(`${code}: ${description}`), 'did-fail-load');
    });
    win.webContents.on('preload-error', (_event, _file, error) => this.report(error, 'preload'));
    win.webContents.on('render-process-gone', (_event, details) => {
      if(this.window!==win)return;
      this.ready = false; this.wanted=false;this.expanded=false;this.playerMenuRows=0;
      this.report(new Error(`${details.reason}, exit ${details.exitCode}`), 'renderer-gone');
      win.destroy(); // Next F8 creates a clean UI; no native helper is restarted.
    });
    win.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
    win.webContents.on('will-navigate', event => event.preventDefault());
    win.webContents.on('before-input-event', (event, input) => {
      if (input.type === 'keyDown' && input.key === 'Escape') {
        event.preventDefault();
        if(input.isAutoRepeat)return;
        if(this.expanded) {
          this.setExpanded(false);
          win.webContents.send('blacktop:panel-collapsed');
        } else this.hide();
      }
    });
    win.on('close', event => { if (!this.disposed) { event.preventDefault(); this.hide(); } });
    win.on('closed', () => { if (this.window === win) { this.window = null; this.ready = false; this.trace('window-closed'); } });
    win.webContents.once('did-finish-load', () => { this.ready = true; this.trace('did-finish-load'); if (this.wanted) this.present(); });
    win.loadFile(path.join(this.sourceDir, 'renderer', 'blacktop-panel.html')).catch(error => {
      this.wanted = false; this.report(error, 'loadFile'); win.destroy();
    });
  }
  present() {
    if (!this.ready || this.disposed || !this.wanted) { this.trace('present-skipped'); return; }
    const win = this.window;
    // Do all foreground work synchronously in the global-shortcut callback.
    // Windows retains the final say; this does not bypass exclusive fullscreen.
    let stage = 'restore';
    try {
      this.trace('present-begin');
      if (win.isMinimized()) win.restore();
      win.setBounds(this.panelBounds(this.displayArea(),this.expanded));
      stage = 'topmost'; this.applyTopmost(); this.trace('present-topmost');
      stage = 'show';
      win.setFocusable(Boolean(this.expanded));
      if(this.expanded)win.show();else win.showInactive();
      this.trace('present-show');
      if(this.expanded){stage = 'present-hook'; this.onPresent(); this.trace('present-hook');}
      stage = 'move-top'; win.moveTop(); this.trace('present-move-top');
      if(this.expanded){stage = 'window-focus'; win.focus(); this.trace('present-focus');}
      stage = 'notify-renderer'; win.webContents.send('blacktop:panel-shown'); this.trace('present-notified');
    } catch (error) { this.report(error, stage); }
    finally { this.observeAfterPresent(); }
  }
  applyTopmost() {
    // Electron's Windows 'floating' level moves behind the taskbar, which can
    // clear topmost when fullscreen has lowered the taskbar. Avoid that path.
    this.window.setAlwaysOnTop(true, 'screen-saver');
    if (!this.window.isAlwaysOnTop()) throw new Error('Windows did not retain the panel topmost flag.');
  }
  setExpanded(expanded) {
    if (!this.window || this.window.isDestroyed()) return;
    const wasExpanded=this.expanded;
    this.expanded=expanded === true;
    if(this.expanded)this.playerMenuRows=0;
    this.window.setFocusable(this.expanded);
    const area = this.displayArea();
    this.window.setBounds(this.panelBounds(area,this.expanded));
    if(this.expanded){this.focusHandoff=false;clearTimeout(this.focusHandoffTimer);this.onPresent();this.window.focus();}
    else if(wasExpanded){
      this.focusHandoff=true;clearTimeout(this.focusHandoffTimer);
      Promise.resolve().then(()=>{if(!this.disposed&&!this.expanded)return this.onHide();})
       .catch(error=>this.report(error,'return-game-focus')).finally(()=>{
        const win=this.window;if(this.disposed||this.expanded||!this.wanted||!win||win.isDestroyed())return;
        try{this.applyTopmost();win.showInactive();win.moveTop();}catch(error){this.report(error,'compact-bar-restore');}
        this.focusHandoffTimer=setTimeout(()=>{this.focusHandoff=false;},900);this.focusHandoffTimer.unref?.();
       });
    }
  }
  setPlayerMenuRows(rows) {
    const count=Number.isInteger(rows)?Math.max(0,Math.min(7,rows)):0;
    this.playerMenuRows=count;
    if(this.expanded || !this.window || this.window.isDestroyed())return;
    this.window.setBounds(this.panelBounds(this.displayArea(),false));
  }
  setGameForeground(foreground) {
    if (typeof foreground !== 'boolean' || this.disposed) return;
    const win=this.window;
    if (!foreground) {
      if(this.focusHandoff){this.trace('game-background-ignored-during-focus-handoff');return;}
      // The expanded editor owns focus while it is being used. Any other
      // foreground app should never have to sit beneath this topmost window.
      if (this.wanted && win && !win.isDestroyed() && !(this.expanded && win.isFocused())) {
        this.focusSuspended=true;
        if(win.isVisible())win.hide();
        this.trace('game-background-hide');
      }
      return;
    }
    this.focusHandoff=false;clearTimeout(this.focusHandoffTimer);
    if (this.focusSuspended) {
      this.focusSuspended=false;
      this.trace('game-foreground-restore');
      if(this.wanted)this.present();
    }
  }
  show() { if (this.disposed) return; this.wanted = true; this.focusSuspended=false; this.trace('show-request'); this.ensureWindow(); this.present(); }
  hide() {
    this.wanted = false; this.focusSuspended=false;this.focusHandoff=false;clearTimeout(this.focusHandoffTimer);
    this.trace('hide-request');
    const win = this.window;
    if (!win || win.isDestroyed()) return;
    // Ask Windows to focus the verified game while the panel still has focus.
    // No keyboard injection, input-thread attachment, or game-memory writes.
    Promise.resolve().then(() => this.onHide()).catch(error => this.report(error, 'return-game-focus')).finally(() => {
      if (this.disposed || win.isDestroyed()) return;
      if (!this.wanted) win.hide(); else this.present();
    });
  }
  toggle() {
    // An open but unfocused panel may be behind the game: summon it, don't hide it.
    const hide = this.wanted && (!this.expanded || !this.ready || this.window?.isFocused());
    this.trace('toggle-decision', { action: hide ? 'hide' : 'show' });
    if (hide) this.hide(); else this.show();
  }
  dispose() {
    this.screen.removeListener?.('display-metrics-changed',this.displayChanged);
    for (const timer of this.observationTimers) clearTimeout(timer);
    this.observationTimers.clear();
    this.disposed = true; this.wanted = false; this.focusSuspended=false;this.focusHandoff=false;clearTimeout(this.focusHandoffTimer);
    this.trace('dispose');
    if (this.registered) this.globalShortcut.unregister(this.accelerator);
    this.registered = false;
    if (this.window && !this.window.isDestroyed()) this.window.destroy();
  }
  async waitForShortcutStop(){return this.globalShortcut.waitForStop?.()??true;}
}
module.exports = { BlacktopPanel };
