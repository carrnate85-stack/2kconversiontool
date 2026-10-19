// Optional V2 inspection workspace guard. No styling/layout changes. The Rust
// service remains the authority: this UI guard never grants a save capability.
(() => {
  'use strict';
  if (new URLSearchParams(location.search).get('readonly') !== 'v2') return;
  const message = 'Version 2 read-only inspection · Save/import is not enabled. No game writes are available.';
  const original = window.modLauncher;
  if (original) window.modLauncher = new Proxy(original, {
    get(target, name, receiver) {
      if (/^(save|apply|add|remove|delete|import)/i.test(String(name)) || name === 'playerDna' || name === 'teamDna') {
        return async () => { throw new Error(message); };
      }
      return Reflect.get(target, name, receiver);
    },
  });
  function guard(modal) {
    if (!modal) return;
    for (const control of modal.querySelectorAll('input, textarea, select')) {
      // Keep catalog filtering and the existing player switcher usable.
      if (control.id === 'playerQuickSwitch' || control.type === 'search' || /search|filter/i.test(control.id || '') || /search|filter/i.test(control.placeholder || '')) continue;
      if (!control.disabled) control.disabled = true;
    }
    for (const button of modal.querySelectorAll('button')) {
      if (/^(save|createCustomArena)/i.test(button.id || '') || /\b(save|import|stage|apply|random|add|remove|export)\b/i.test(button.textContent || '')) button.disabled = true;
    }
  }
  const modals = ['playerEditorModal', 'teamEditorModal'].map(id => document.getElementById(id)).filter(Boolean);
  function enforce() {
    for (const modal of modals) guard(modal);
    for (const id of ['playerEditBoundary', 'teamEditBoundary']) {
      const node = document.getElementById(id);
      if (node && node.textContent !== message) node.textContent = message;
    }
  }
  let queued = false;
  function schedule() { if (!queued) { queued = true; queueMicrotask(() => { queued = false; enforce(); }); } }
  // Registered after the embedded WPF scripts: override their source-based
  // editable state after every player open and dynamic panel insertion.
  window.addEventListener('player-editor:open', enforce);
  for (const modal of modals) new MutationObserver(schedule).observe(modal, { childList: true, subtree: true, attributes: true, attributeFilter: ['class'] });
  enforce();
})();
