// UI-only locale collation: keep the original WPF renderer files unchanged.
// The native backend owns persistence; no game operation is added here.
(() => {
  const original = window.modLauncher?.editorPresets;
  if (typeof original !== 'function') return;
  Object.defineProperty(window.modLauncher, 'editorPresets', {
    value: async request => {
      const result = await original(request);
      return request?.action === 'list' && Array.isArray(result)
        ? result.sort((a, b) => a.name.localeCompare(b.name))
        : result;
    },
  });
})();
// Version 2 live players leave height and wingspan unmapped (null); older
// documents may also leave weight null. The unchanged WPF form cannot submit
// blank measurements, so each unmapped one is shown as not saved: blank,
// disabled, and read back as null. The native backend refuses real values.
(() => {
  if (typeof window.addEventListener !== 'function' || !window.document) return;
  // Each measurement is unmapped on its own: Version 2 proves weight but not
  // the appearance pointer that holds height and wingspan.
  const fields = { playerEditHeight: 'heightCm', playerEditHeightInches: 'heightCm', playerEditWingspan: 'wingspanCm', playerEditWeight: 'weightLbs' };
  const unmapped = input => input?.dataset?.rustUnmapped === '1';
  let wrapped = false;
  const wrap = () => {
    const measurements = window.playerMeasurements;
    if (wrapped || !measurements) return;
    wrapped = true;
    const { readHeight, readWingspan } = measurements;
    measurements.readHeight = (feet, inches) => (unmapped(feet) ? null : readHeight(feet, inches));
    measurements.readWingspan = input => (unmapped(input) ? null : readWingspan(input));
  };
  window.addEventListener('player-editor:open', event => {
    wrap();
    const player = event.detail || {};
    for (const [id, key] of Object.entries(fields)) {
      const input = window.document.getElementById(id);
      if (!input) continue;
      if (player.source === 'live-roster' && player[key] == null) {
        input.value = '';
        input.disabled = true;
        input.dataset.rustUnmapped = '1';
        input.placeholder = 'Not mapped';
        input.title = 'Not mapped for this game version; this value is not saved.';
      } else if (unmapped(input)) {
        delete input.dataset.rustUnmapped;
        input.placeholder = '';
        input.title = '';
      }
    }
  });
})();

// Optional scalar capability is native-row-specific (unknown negative audio
// sentinels are display-only and must not invite an unsupported overwrite).
(() => {
  const update = player => {
    const input = document.getElementById('playerAudioFullName');
    if (!input || !player?.editCapabilities) return;
    input.disabled = player.editCapabilities.fullNameId !== true;
    input.title = input.disabled ? 'This player has an unmapped audio value. It is preserved unchanged.' : '';
  };
  window.addEventListener('player-editor:open', event => update(event.detail));
  window.addEventListener('player-editor:committed', event => update(event.detail?.player));
})();
