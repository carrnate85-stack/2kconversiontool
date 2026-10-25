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
// Only measurements admitted by the selected player's native read are editable.
// Keep unavailable/shared appearance values out of the ordinary save payload.
(() => {
  if (typeof window.addEventListener !== 'function' || !window.document) return;
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
        input.title = player.measurementStatus || 'This measurement is unavailable for this player; it is not saved.';
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

// Read the selected player's native appearance before the editor creates its
// draft/baseline. Do not asynchronously overwrite fields after editing begins.
(() => {
  let visit = 0;
  window.addEventListener('player-editor:closed', () => { visit++; });
  window.addEventListener('keydown', event => { if (event.key === 'Escape') visit++; });
  window.addEventListener('DOMContentLoaded', () => {
    const original = window.openPlayerEditor;
    if (typeof original !== 'function') return;
    window.openPlayerEditor = async function(player) {
      const token = ++visit;
      if (player?.source !== 'live-roster' || !player.editCapabilities)
        return original.call(this, player);
      const identity = {recordIndex:player.recordIndex, firstName:player.firstName, lastName:player.lastName, cyberfaceId:player.cyberfaceId};
      let result;
      try {
        const detail = await window.modLauncher.playerMeasurements(identity);
        if (token !== visit) return;
        if (!detail || Object.entries(identity).some(([key,value]) => detail[key] !== value))
          throw Error('Player identity changed; refresh the roster.');
        result = {...player, editCapabilities:{...player.editCapabilities}, measurementStatus:detail.measurementStatus};
        for (const [key,max] of [['heightCm',260],['wingspanCm',320]]) {
          const available = detail.editCapabilities?.[key] === true && Number.isFinite(detail[key]) && detail[key] >= 120 && detail[key] <= max;
          result[key] = available ? detail[key] : null;
          result.editCapabilities[key] = available;
        }
      } catch (error) {
        result = {...player, heightCm:null, wingspanCm:null,
          editCapabilities:{...player.editCapabilities,heightCm:false,wingspanCm:false},
          measurementStatus:error.message || String(error)};
      }
      if (token === visit) return original.call(this, result);
    };
  });
})();
