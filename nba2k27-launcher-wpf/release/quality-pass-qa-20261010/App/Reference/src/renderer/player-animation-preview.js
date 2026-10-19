(() => {
 const modal = document.getElementById('playerEditorModal'), schema = window.playerRatingSchema || [];
 const fields = window.playerAnimationPreviewFields || [], shots = ['ShotJumper','ShotRelease','ShotRelease2','ShotTiming','ShotBlendingRatio'];
 const buttons = []; let epoch = 0, player = null, active = null, applying = false, lastClosedMetrics = null;
 const valid = context => context === active && context.epoch === epoch && !modal.classList.contains('hidden') && player?.source === 'live-roster';
 const cleanError = error => String(error?.message || error || 'Preview unavailable.').replace(/^Error invoking remote method '[^']+': (?:Error: )?/, '');
 const read = () => window.playerAnimationValues([...new Set([...fields,...shots])]);
 async function dispose() {
  const previous = active; active = null;
  if (!previous) return;
  previous.unsubscribe?.(); previous.loading?.remove();
  if (previous.popup) await previous.popup.close();
  if (previous.session) await window.modLauncher.closeAnimationPreview(previous.session).catch(() => {});
 }
 function invalidate() { if (applying) return; epoch++; void dispose(); }
 async function open(fieldKey) {
  const requestedEpoch = ++epoch;
  await dispose(); if (requestedEpoch !== epoch || modal.classList.contains('hidden') || player?.source !== 'live-roster') return;
  const context = { epoch, key: fieldKey }; active = context;
  const loading = document.createElement('dialog'); loading.className = 'animation-popup animation-loading';
  const title = document.createElement('h2'); title.textContent = 'Animation preview';
  const status = document.createElement('p'); status.textContent = 'Preparing animation preview…'; status.setAttribute('role','status');
  const close = document.createElement('button'); close.type = 'button'; close.textContent = 'Close';
  const retry = document.createElement('button'); retry.type = 'button'; retry.textContent = 'Retry preview'; retry.hidden = true;
  loading.append(title,status,retry,close); document.body.append(loading); context.loading = loading; loading.showModal(); close.focus();
  close.addEventListener('click',()=>void dispose()); loading.addEventListener('cancel',event=>{event.preventDefault();void dispose();});
  retry.addEventListener('click',()=>void open(fieldKey));
  try {
   const { session } = await window.modLauncher.openAnimationPreview(); context.session = session;
   if (!valid(context)) { await window.modLauncher.closeAnimationPreview(session); return; }
   context.unsubscribe = window.modLauncher.onAnimationPreviewProgress(row => {
    if (valid(context) && row.session === session) status.textContent = row.phase === 'ready' ? 'Opening preview…' : `${row.phase==='indexing'?'Indexing animations':row.phase==='model'?'Preparing player model':'Preparing animation cache'}${row.asset ? ': '+row.asset : ''} · ${row.percent ?? 0}%`;
   });
   const catalog = await window.modLauncher.animationPreviewCatalog(session);
   const { openAnimationPopup } = await import('./animation-preview/animation_popup.js');
   if (!valid(context)) return;
   const keys = shots.includes(fieldKey) ? shots : [fieldKey];
   const request = async (op, args) => {
    if (!valid(context)) throw Error('Preview closed.');
    const result = await window.modLauncher.animationPreviewRequest(op, { ...args, session });
    if (!valid(context)) throw Error('Preview expired.');
    if (!result.url) return result;
    if (result.session !== session || result.request !== args.request) throw Error('Preview reply is stale.');
    const response = await (window.modLauncher.animationPreviewFetch || fetch)(result.url, { cache: 'no-store' });
    if (!response.ok) throw Error('Preview data expired. Retry preview.');
    const json = await response.json(); if (!valid(context)) throw Error('Preview expired.'); return json;
   };
   context.unsubscribe?.(); context.unsubscribe = null; loading.close(); loading.remove(); context.loading = null;
   context.popup = openAnimationPopup({ catalog, fieldKey, values: read(), autoplay: true, onRetry:()=>void open(fieldKey),
    transport: { resolve:args=>request('resolve',args), load:args=>request('load',args), shotPart:args=>request('shot-part',args), character:args=>request('character',args), close:()=>window.modLauncher.closeAnimationPreview(session) },
    onUse: choices => {
     if (!valid(context) || window.isPlayerDnaBusy?.() || window.isSavingPlayerSeasons?.() || document.getElementById('savePlayerEdit').textContent === 'Saving…') return;
     const current = read();
     const changed = Object.fromEntries(keys.filter(key=>Object.hasOwn(choices,key)&&choices[key]!==current[key]).map(key=>[key,choices[key]]));
     if (Object.keys(choices).some(key=>!keys.includes(key))) throw Error('Unexpected preview staging field.');
     applying = true; try { window.stagePlayerRatingValues(changed); window.dispatchEvent(new Event('player-editor:staged')); } finally { applying = false; }
    },
    onClose: metrics => { lastClosedMetrics = metrics; if (active === context) active = null; },
   });
  } catch (error) {
   if (valid(context)) { status.textContent = cleanError(error); retry.hidden = false; }
  }
 }
 for (const key of [...new Set([...fields,...shots])]) {
  const index = schema.findIndex(field=>field.key===key), input = document.getElementById('studioField'+index);
  if (!input) continue;
  const control = document.createElement('span'); control.className = 'player-animation-control'; input.before(control); control.append(input);
  const button = document.createElement('button'); button.type = 'button'; button.className = 'secondary player-animation-play'; button.textContent = '▶'; button.title = button.ariaLabel = 'Preview '+schema[index].label; button.dataset.animationField = key;
  button.disabled = true; control.append(button); buttons.push({button,input}); button.addEventListener('click',event=>{event.preventDefault();void open(key);});
  input.addEventListener('change',()=>{if(active?.popup&&valid(active))active.popup.setValues(read());});
  input.addEventListener('input',()=>{if(active?.popup&&valid(active))active.popup.setValues(read());});
 }
 const warm=enabled=>{const fn=window.modLauncher.warmAnimationPreview;if(fn)void fn(enabled).catch(()=>{});};
 window.addEventListener('player-editor:open',event=>{invalidate();player=event.detail;warm(player?.source==='live-roster');for(const {button,input} of buttons)button.disabled=player?.source!=='live-roster'||input.disabled;});
 window.addEventListener('player-editor:closed',()=>warm(false));
 for (const event of ['player-editor:closed','player-editor:preview-invalidated','player-editor:committed','player-editor:import-verified']) window.addEventListener(event,invalidate);
 window.addEventListener('beforeunload',()=>{invalidate();warm(false);});
 window.playerAnimationPreview = { close:dispose, inspect:()=>({ epoch, field:active?.key, loading:!!active?.loading, popup:active?.popup?.inspect(), buttons:buttons.length, lastClosedMetrics }) };
})();
