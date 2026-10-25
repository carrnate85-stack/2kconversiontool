(() => {
  let playerPath = '';
  let selected = '';
  let data = null;
  let revision = 0;
  let busy = false;
  let loading = false;
  let renderedItems = '[]';
  let pendingRefresh: { target: string; promise: Promise<void> } | null = null;
  let pendingChange: { target: string; promise: Promise<boolean> } | null = null;
  let dialogGeneration = 0;
  const types = [['hair', 'configHair'], ['facialhair', 'configFacialhair'], ['headband', 'configHeadband']] as const;
  const category = (item) => item.type?.toLowerCase().startsWith('headband') ? 'headband' : item.type?.toLowerCase();
  const status = (message) => { $('configsStatus').textContent = message; };
  const current = () => data?.configs.find((config) => config.name === selected);
  function active() { return playerPath === state.workingPlayer?.path ? selected : ''; }
  function renderPreviewSelector() {
    const picker = $('previewConfigSelect');
    const configs = playerPath === state.workingPlayer?.path ? data?.configs || [] : [];
    picker.replaceChildren(...configs.map((config) => new Option(config.name, config.name)));
    if (!configs.length) picker.add(new Option(state.workingPlayer ? 'No configurations loaded' : 'No staged player', ''));
    picker.value = active();
    picker.disabled = state.resetting || busy || loading || state.preview.loading || state.preview.blenderBusy || !configs.length;
  }
  function reset() {
    revision += 1;
    playerPath = ''; selected = ''; data = null; busy = false; loading = false;
    pendingRefresh = null; pendingChange = null; dialogGeneration++; dialog.close();
    render();
  }
  function render() {
    if (typeof updateStagePlayerControls === 'function') queueMicrotask(updateStagePlayerControls);
    renderPreviewSelector();
    $('configsPlayer').textContent = state.workingPlayer?.name || 'No staged player';
    $('configsList').replaceChildren();
    $('newConfig').disabled = busy || loading || !data?.configs.length;
    const renderedTarget = playerPath;
    for (const config of data?.configs || []) {
      const row = document.createElement('div');
      row.className = `config-row${config.name === selected ? ' active' : ''}`;
      const text = document.createElement('div');
      const title = document.createElement('strong');
      title.textContent = config.name;
      const detail = document.createElement('small');
      detail.textContent = [config.name === data.defaultConfig ? 'Default' : '', `${config.items.length} accessories`, !config.texturePresent ? 'Face package missing' : ''].filter(Boolean).join(' | ');
      text.append(title, detail);
      const edit = document.createElement('button');
      edit.className = 'button secondary'; edit.textContent = 'Edit Config'; edit.disabled = busy || loading;
      edit.addEventListener('click', () => select(config.name, renderedTarget));
      row.append(text, edit); $('configsList').append(row);
    }
    const config = current();
    $('configEditor').hidden = !config;
    if (!config) { renderedItems = '[]'; status('No configurations loaded.'); return; }
    $('configEditorName').textContent = config.name;
    $('configDefault').disabled = busy || loading || data.defaultConfig === config.name;
    for (const [kind, id] of types) {
      const select = $(id); select.replaceChildren(new Option('None', ''));
      select.disabled = busy || loading;
      for (const item of data.items.filter((i) => category(i) === kind)) select.add(new Option(item.name, item.name));
      select.value = config.items.find((name) => data.items.some((i) => i.name === name && category(i) === kind)) || '';
    }
    $('configOther').replaceChildren();
    const legend = document.createElement('legend'); legend.textContent = 'Other Accessories'; $('configOther').append(legend);
    for (const item of data.items.filter((i) => !types.some(([kind]) => kind === category(i)))) {
      const label = document.createElement('label'); const input = document.createElement('input');
      input.type = 'checkbox'; input.value = item.name; input.checked = config.items.includes(item.name);
      input.disabled = busy || loading;
      label.append(input, document.createTextNode(item.name)); $('configOther').append(label);
    }
    $('configOther').hidden = $('configOther').children.length === 1;
    renderedItems = selectedItemsSignature();
    for (const id of ['renameConfig', 'saveConfig', 'configShowPreview', 'configChooseHair', 'configChooseBeard', 'configAddHeadband'] as const) $(id).disabled = busy || loading;
  }
  function select(name, target = playerPath) {
    if (state.resetting || busy || loading || target !== playerPath || target !== state.workingPlayer?.path
      || !data?.configs.some((config) => config.name === name)) return;
    selected = name;
    invalidatePlayerRender(target);
    invalidateNativeHairTarget();
    state.preview.dirty = true;
    render(); status(`Editing ${name}`);
  }
  function refresh(): Promise<void> {
    if (!state.workingPlayer) { reset(); return Promise.resolve(); }
    const target = state.workingPlayer.path;
    if (pendingChange?.target === target) {
      const id = revision;
      return pendingChange.promise.then(() => {
        if (target !== state.workingPlayer?.path || target !== playerPath) return;
        if (pendingRefresh?.target === target) return pendingRefresh.promise;
        if (id === revision) return refresh();
      });
    }
    if (pendingRefresh?.target === target) return pendingRefresh.promise;
    const id = ++revision;
    if (target !== playerPath) {
      selected = ''; data = null; playerPath = target; busy = false;
      dialogGeneration++; dialog.close();
    }
    const request = { target, promise: Promise.resolve() };
    pendingRefresh = request; loading = true;
    const isCurrent = () => id === revision && target === playerPath && target === state.workingPlayer?.path;
    render(); status('Loading configurations...');
    request.promise = (async () => {
      let message = '';
      try {
        const result = await window.characterTool.listConfigs({ target });
        if (!isCurrent()) return;
        data = result;
        const previous = selected;
        if (!data.configs.some((config) => config.name === selected)) selected = data.defaultConfig || data.configs[0]?.name || '';
        if (previous !== selected) invalidateNativeHairTarget();
        message = selected ? `Editing ${selected}` : 'No configurations found.';
      } catch (error) { if (isCurrent()) { data = null; message = error.message; } }
      finally {
        if (isCurrent()) { loading = false; busy = false; render(); status(message); }
        if (pendingRefresh === request) pendingRefresh = null;
      }
    })();
    return request.promise;
  }
  function change(action, extra = {}): Promise<boolean> {
    if (state.resetting || busy || loading || !active() || !current()) return Promise.resolve(false);
    const target = playerPath;
    const config = selected, id = ++revision;
    const request = { target, promise: Promise.resolve(false) };
    pendingChange = request; busy = true; render(); status('Saving configuration...');
    const isCurrent = () => id === revision && target === playerPath && target === state.workingPlayer?.path;
    request.promise = (async () => {
      let message = '';
      try {
        const result = await window.characterTool.changeConfig({ target, config, action, ...extra });
        invalidatePlayerRender(target);
        if (!isCurrent()) return false;
        data = result; selected = result.activeConfig; state.preview.dirty = true;
        invalidateNativeHairTarget(); message = `Saved ${selected}`; return true;
      } catch (error) { if (isCurrent()) message = error.message; return false; }
      finally {
        if (isCurrent()) { busy = false; render(); status(message); }
        if (pendingChange === request) pendingChange = null;
      }
    })();
    return request.promise;
  }
  function save() {
    const items = types.map(([, id]) => $(id).value).filter(Boolean);
    $('configOther').querySelectorAll<HTMLInputElement>('input:checked').forEach((input) => items.push(input.value));
    return change('save', { items });
  }
  function stagingBlockReason() {
    if (busy || pendingChange || loading) return 'Wait for the configuration operation to finish.';
    const config = current();
    if (!config || playerPath !== state.workingPlayer?.path) return '';
    return selectedItemsSignature() === renderedItems
      ? '' : 'Save configuration changes before staging another player.';
  }
  function selectedItemsSignature() {
    const items = types.map(([, id]) => $(id).value).filter(Boolean);
    $('configOther').querySelectorAll<HTMLInputElement>('input:checked').forEach(input => items.push(input.value));
    return JSON.stringify([...new Set(items)].sort());
  }
  let dialogAction = 'create';
  const dialog = document.createElement('dialog'); dialog.className = 'config-create';
  dialog.innerHTML = '<form><h2>New Config</h2><label>Name<input name="name" maxlength="48" pattern="[A-Za-z][A-Za-z0-9_]*" required autocomplete="off"></label><label>Copy From<select name="base"></select></label><p role="status"></p><footer><button class="button secondary" type="button">Cancel</button><button class="button primary" type="submit">Create Config</button></footer></form>';
  document.body.append(dialog);
  dialog.addEventListener('cancel', (event) => { if (busy) event.preventDefault(); });
  dialog.querySelector<HTMLButtonElement>('button[type="button"]').onclick = () => dialog.close();
  dialog.querySelector('form').onsubmit = async (event) => {
    event.preventDefault();
    const generation = dialogGeneration, target = playerPath;
    if (dialogAction === 'create') selected = dialog.querySelector('select').value;
    const button = dialog.querySelector<HTMLButtonElement>('button[type="submit"]'); button.disabled = true;
    const cancel = dialog.querySelector<HTMLButtonElement>('button[type="button"]'); cancel.disabled = true;
    const ok = await change(dialogAction, { name: dialog.querySelector('input').value });
    if (generation !== dialogGeneration || target !== playerPath || target !== state.workingPlayer?.path) return;
    button.disabled = false;
    cancel.disabled = false;
    if (ok) dialog.close(); else dialog.querySelector('p').textContent = $('configsStatus').textContent;
  };
  function openNameDialog(action) {
    if (state.resetting || busy || loading || !active()) return;
    dialogGeneration++;
    dialogAction = action;
    const rename = action === 'rename';
    dialog.querySelector('h2').textContent = rename ? 'Rename Config' : 'New Config';
    dialog.querySelector('button[type="submit"]').textContent = rename ? 'Rename Config' : 'Create Config';
    dialog.querySelector<HTMLButtonElement>('button[type="submit"]').disabled = false;
    dialog.querySelector<HTMLButtonElement>('button[type="button"]').disabled = false;
    dialog.querySelector('select').closest('label').hidden = rename;
    dialog.querySelector('input').value = rename ? selected : ''; dialog.querySelector('p').textContent = '';
    dialog.querySelector('select').replaceChildren(...data.configs.map((c) => new Option(c.name, c.name)));
    dialog.querySelector('select').value = selected;
    dialog.showModal();
    dialog.querySelector('input').select();
  }
  $('newConfig').onclick = () => openNameDialog('create');
  $('renameConfig').onclick = () => openNameDialog('rename');
  $('configsTab').onclick = () => setActiveView('configs');
  $('previewConfigSelect').onchange = async (event) => {
    const name = (event.currentTarget as HTMLSelectElement).value;
    if (state.resetting || busy || loading || state.preview.loading || !data?.configs.some((config) => config.name === name)) return;
    select(name);
    await loadStagedPreview(true);
  };
  $('saveConfig').onclick = save;
  $('configDefault').onclick = () => change('default');
  async function saveThen(action: () => unknown) {
    const target = playerPath, id = revision;
    if (await save() && revision === id + 1 && target === playerPath && target === state.workingPlayer?.path) await action();
  }
  $('configShowPreview').onclick = () => saveThen(() => setActiveView('preview'));
  $('configChooseHair').onclick = () => saveThen(async () => { setActiveView('hair'); await setNativeHairType('hair'); });
  $('configChooseBeard').onclick = () => saveThen(async () => { setActiveView('hair'); await setNativeHairType('facialhair'); });
  $('configAddHeadband').onclick = () => saveThen(() => setActiveView('headband'));
  window.configsEditor = { refresh, reset, active, renderPreviewSelector, stagingBlockReason };
  $('configsView').addEventListener('change', () => {
    if (typeof updateStagePlayerControls === 'function') updateStagePlayerControls();
  });
  render();
})();
