(() => {
  const layout = document.querySelector(".player-preview-layout");
  const inspector = layout?.querySelector(".player-preview-inspector");
  if (inspector) {
    let syncInspector2 = function() {
      const open = explicitOpen ?? !compact.matches;
      inspector.hidden = !open;
      layout.dataset.inspector = open ? "open" : "closed";
      toggle.setAttribute("aria-expanded", String(open));
      toggle.title = open ? "Hide render summary" : "Show render summary";
      toggle.setAttribute("aria-label", toggle.title);
    };
    var syncInspector = syncInspector2;
    const compact = matchMedia("(max-width: 1100px)");
    let explicitOpen = null;
    inspector.id = "previewRenderSummary";
    inspector.setAttribute("aria-label", "Render summary");
    const toggle = document.createElement("button");
    toggle.type = "button";
    toggle.className = "icon-button";
    toggle.dataset.previewInspectorToggle = "";
    toggle.setAttribute("aria-controls", inspector.id);
    const icon = document.createElement("i");
    icon.dataset.lucide = "panel-right";
    toggle.append(icon);
    layout.querySelector(".player-preview-tools").prepend(toggle);
    const close = document.createElement("button");
    close.type = "button";
    close.className = "icon-button";
    close.title = "Close render summary";
    close.setAttribute("aria-label", close.title);
    const closeIcon = document.createElement("i");
    closeIcon.dataset.lucide = "x";
    close.append(closeIcon);
    inspector.querySelector(".preview-inspector-heading").append(close);
    toggle.addEventListener("click", () => {
      explicitOpen = inspector.hidden;
      syncInspector2();
    });
    close.addEventListener("click", () => {
      explicitOpen = false;
      syncInspector2();
      toggle.focus();
    });
    compact.addEventListener("change", syncInspector2);
    syncInspector2();
  }
  const refresh = document.getElementById("refreshPreview");
  if (refresh) {
    refresh.className = "button icon-button";
    refresh.title = "Refresh render";
    refresh.setAttribute("aria-label", refresh.title);
    const icon = document.createElement("i");
    icon.dataset.lucide = "refresh-cw";
    refresh.replaceChildren(icon);
  }
  window.lucide.createIcons({ attrs: { "aria-hidden": "true", focusable: "false", "stroke-width": 1.7 } });
  const groups = [...document.querySelectorAll("[data-viewer-tools]")];
  const viewers = (kind) => kind === "hair" ? [window.hairBasePreview, window.hairAssetPreview].filter(Boolean) : [window.modelPreview].filter(Boolean);
  const primary = (kind) => viewers(kind)[0];
  function sync() {
    for (const group of groups) {
      const ready = Boolean(primary(group.dataset.viewerTools)?.stats().payload);
      for (const control of group.querySelectorAll("button, select, input")) control.disabled = !ready;
    }
  }
  for (const group of groups) {
    const kind = group.dataset.viewerTools;
    for (const button of group.querySelectorAll("[data-viewer-action]")) {
      button.type = "button";
      button.addEventListener("click", async () => {
        const action = button.dataset.viewerAction;
        if (action !== "fullscreen") {
          if (kind === "hair" && action === "head") {
            primary(kind)?.fitCamera(window.hairAssetPreview?.bounds());
            primary(kind)?.setView("front");
          } else primary(kind)?.setView(action);
          if (kind === "hair") window.hairAssetPreview?.setCameraState(window.hairBasePreview.cameraState());
          return;
        }
        const viewport = group.closest(".native-hair-preview-shell, .player-preview-viewport");
        try {
          if (document.fullscreenElement) await document.exitFullscreen();
          else await viewport.requestFullscreen();
        } catch {
          viewport.classList.toggle("viewer-expanded");
        }
        for (const viewer of viewers(kind)) viewer.resize();
      });
    }
    for (const control of group.querySelectorAll("[data-viewer-setting]")) {
      control.addEventListener("change", () => {
        const setting = control.dataset.viewerSetting;
        for (const viewer of viewers(kind)) {
          if (setting === "lighting") viewer.setStudioLighting(control.value === "studio");
          if (setting === "surface") viewer.setClayMode(control.value === "clay");
          if (setting === "grid" && control instanceof HTMLInputElement) viewer.setGridVisible(control.checked);
        }
        if (setting === "head" && control instanceof HTMLInputElement) {
          window.hairBasePreview?.setVisible(control.checked);
          window.hairAssetPreview?.setHeadOcclusionVisible(control.checked);
        }
      });
    }
  }
  window.addEventListener("model-preview:pick", ({ detail }) => {
    const kind = detail?.viewer && detail.viewer !== "modelPreview" ? "hair" : "player";
    const container = document.querySelector(`[data-viewer-details="${kind}"]`);
    container.replaceChildren();
    container.hidden = !detail?.material;
    if (!detail?.material) return;
    const values = [
      ["Primitive", detail.primitive],
      ["Material", detail.material.name],
      ["Shader", detail.material.shader],
      ["Lightmap", detail.material.lightmap],
      ...(detail.material.textures || []).map((texture) => [
        texture.channel === "map" ? "Color" : texture.channel === "normalMap" ? "Normal" : "Tangent",
        texture.name
      ])
    ];
    for (const [label, value] of values.filter((item) => item[1])) {
      const row = document.createElement("div");
      const title = document.createElement("span");
      title.textContent = label;
      const text = document.createElement("code");
      text.textContent = value;
      row.append(title, text);
      container.append(row);
    }
  });
  for (const name of ["model-preview:loaded", "model-preview:cleared"]) window.addEventListener(name, sync);
  document.addEventListener("fullscreenchange", () => {
    for (const group of groups) for (const viewer of viewers(group.dataset.viewerTools)) viewer.resize();
  });
  window.addEventListener("keydown", (event) => {
    if (event.key !== "Escape") return;
    if (document.fullscreenElement) document.exitFullscreen().catch(() => {
    });
    document.querySelectorAll(".viewer-expanded").forEach((viewport) => viewport.classList.remove("viewer-expanded"));
    for (const group of groups) for (const viewer of viewers(group.dataset.viewerTools)) viewer.resize();
  });
  sync();
})();
