(() => {
  const key = '2k-launcher-playlist-order';
  function ordered(items, ids) {
    const ranks = new Map(ids.map((id, i) => [id, i]));
    return [...items].sort((a, b) => (ranks.get(a.value) ?? Infinity) - (ranks.get(b.value) ?? Infinity));
  }
  if (typeof module !== 'undefined') module.exports = { ordered };
  if (typeof document === 'undefined') return;
  let ids = [];
  try { const saved = JSON.parse(localStorage.getItem(key)); if (Array.isArray(saved)) ids = saved.filter(id => typeof id === 'string'); } catch {}
  const selects = ['jukeboxPlaylistSelect', 'homeJukeboxPlaylist', 'jukeboxImportPlaylist'].map(id => document.getElementById(id));
  function sync() {
    for (const select of selects) {
      if (!select) continue;
      const before = [...select.options], after = ordered(before, ids);
      if (before.every((option, i) => option === after[i])) continue;
      const selected = select.value;
      select.replaceChildren(...after);
      select.value = selected;
    }
  }
  const observer = new MutationObserver(sync);
  selects.filter(Boolean).forEach(select => observer.observe(select, { childList: true }));
  sync();
  const modal = document.createElement('dialog');
  modal.className = 'playlist-order-dialog';
  const title = document.createElement('h2'); title.textContent = 'Playlist order';
  const help = document.createElement('p'); help.textContent = 'Drag to reorder, or use the arrows. Changes save automatically.';
  const list = document.createElement('div'); list.className = 'playlist-order-list';
  const close = document.createElement('button'); close.textContent = 'Done'; close.className = 'secondary'; close.onclick = () => modal.close();
  modal.append(title, help, list, close); document.body.append(modal);
  let rows = [], dragging = null;
  function move(from, to) {
    if (from < 0 || to < 0 || from >= rows.length || to >= rows.length || from === to) return;
    const next = [...rows]; next.splice(to, 0, next.splice(from, 1)[0]);
    try { localStorage.setItem(key, JSON.stringify(next.map(row => row.value))); }
    catch { help.textContent = 'Could not save the order. Please try again.'; return; }
    rows = next; ids = rows.map(row => row.value); sync(); paint();
  }
  function paint() {
    list.replaceChildren();
    rows.forEach((item, i) => {
      const row = document.createElement('div'); row.className = 'playlist-order-row'; row.draggable = true;
      const name = document.createElement('span'); name.textContent = `⠿  ${item.text}`;
      row.ondragstart = event => { dragging = i; event.dataTransfer.effectAllowed = 'move'; event.dataTransfer.setData('text/plain', item.value); };
      row.ondragend = () => { dragging = null; };
      row.ondragover = event => { event.preventDefault(); event.dataTransfer.dropEffect = 'move'; };
      row.ondrop = event => { event.preventDefault(); if (dragging !== null) move(dragging, i); dragging = null; };
      row.append(name);
      for (const [symbol, delta, label] of [['↑', -1, 'up'], ['↓', 1, 'down']]) {
        const button = document.createElement('button'); button.className = 'secondary small'; button.textContent = symbol;
        button.setAttribute('aria-label', `Move ${item.text} ${label}`); button.disabled = i + delta < 0 || i + delta >= rows.length;
        button.onclick = () => { move(i, i + delta); list.children[i + delta]?.querySelector('button:not(:disabled)')?.focus(); };
        row.append(button);
      }
      list.append(row);
    });
  }
  document.getElementById('orderJukeboxPlaylists').onclick = () => {
    rows = [...selects[0].options].map(option => ({ value: option.value, text: option.textContent }));
    paint(); modal.showModal();
  };
})();
