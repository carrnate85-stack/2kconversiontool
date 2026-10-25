import "./renderer/model-preview.js";

const hint = document.getElementById("hint");
const viewer = window.modelPreview;
let generation = 0;

function status(text, state = "ready") {
  hint.textContent = text;
  hint.dataset.state = state;
}
function reply(type, detail = {}) {
  parent.postMessage({ type, ...detail }, location.origin);
}

addEventListener("model-preview:pick", (event) => {
  reply("model-preview:pick", { detail: event.detail ?? null });
});

addEventListener("message", async (event) => {
  if (event.source !== parent || event.origin !== location.origin) return;
  const message = event.data;
  if (!message || typeof message !== "object") return;
  if (message.type === "model-preview:clear") {
    generation++;
    viewer.cancelLoad();
    viewer.clearPreview();
    status("Drag to orbit · wheel to zoom · right-drag to pan");
    return;
  }
  if (message.type === "model-preview:arena-textures") {
    const token = generation;
    try {
      const result = await viewer.loadArenaTextures(message.textures || [], (progress) => {
        if (token === generation) reply("model-preview:arena-progress", { requestId: message.requestId, completed: progress.completed, total: progress.total });
      }, { append: message.append !== false });
      if (token === generation) reply("model-preview:arena-textures-ready", { requestId: message.requestId, result });
    } catch (error) {
      if (token === generation) reply("model-preview:error", { requestId: message.requestId, message: String(error?.message ?? error) });
    }
    return;
  }
  if (message.type === "model-preview:command") {
    const commands = {
      fitCamera: () => viewer.fitCamera(),
      setArenaCourtView: () => viewer.setArenaCourtView(),
      setEyelashesVisible: () => viewer.setEyelashesVisible(Boolean(message.value)),
      setTexture: () => viewer.setTexture(message.value ?? ""),
      setWireframe: () => viewer.setWireframe(Boolean(message.value)),
    };
    commands[message.command]?.();
    return;
  }
  if (message.type !== "model-preview:load") return;
  const token = ++generation;
  const cameraState = message.preserveCamera ? viewer.getCameraState() : undefined;
  viewer.cancelLoad();
  viewer.clearPreview();
  status("Building preview…", "loading");
  try {
    const result = await viewer.load(message.payload, (progress) => {
      if (token === generation) {
        const phase = progress.phase === "textures" ? "Uploading textures" : "Building geometry";
        status(`${phase} ${progress.completed}/${progress.total}…`, "loading");
        reply("model-preview:progress", { requestId: message.requestId, phase, completed: progress.completed, total: progress.total });
      }
    }, cameraState ? { cameraState } : undefined);
    if (token !== generation) return;
    status("Drag to orbit · wheel to zoom · right-drag to pan");
    reply("model-preview:displayed", { requestId: message.requestId, result, stats: viewer.stats() });
  } catch (error) {
    if (token !== generation) return;
    status(String(error?.message ?? error), "error");
    reply("model-preview:error", { requestId: message.requestId, message: String(error?.message ?? error) });
  }
});

addEventListener("resize", () => viewer.resize());
reply("model-preview:ready");
