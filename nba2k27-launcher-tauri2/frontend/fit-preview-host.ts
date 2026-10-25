// Page side of the mannequin preview: WPF core/fast-clothing-preview.js
// (FastClothingPreview) driving the reused WPF worker bundled in
// public/fit-preview/fit-runtime.js. The worker asks this page for the
// manifest index, cache keys and decoded game resources (Rust fit_assets.rs).
import { invoke } from "@tauri-apps/api/core";
import { call } from "./api";

type Pending = {
  resolve: (value: FitPayload) => void;
  reject: (error: Error) => void;
  onProgress?: (payload: FitPayload) => void;
};
export type FitPayload = {
  parts: { asset: string; accessorySlot?: string }[];
  warnings: string[];
  accessories?: { asset: string; slot: string; state: string; reason?: string }[];
  stats: { ms: number; cache: { memory: number; disk: number; decoded: number } };
  stage?: string;
  progress?: boolean;
  [key: string]: unknown;
};
export type FitRequest = {
  items: string[];
  knownMeshes: string[];
  knownTextures: string[];
  showMannequinEarly: boolean;
};

export class FitPreviewHost {
  private worker: Worker | null = null;
  private pending = new Map<number, Pending>();
  private sequence = 0;
  private originals: Promise<string[]> | null = null;
  private ready: Promise<void> | null = null;
  private epoch = 0;

  private ensure() {
    if (this.worker) return this.worker;
    const worker = new Worker("./fit-preview/fit-runtime.js");
    this.worker = worker;
    this.originals ??= call<string[]>("previews:fit-originals");
    this.ready = this.originals.then((originals) => {
      if (this.worker === worker) worker.postMessage({ init: { originals } });
    });
    void this.ready.catch((error) => {
      if (this.worker === worker) {
        this.originals = null;
        this.fail(new Error(String(error)));
      }
    });
    worker.onmessage = (event) => void this.receive(worker, event.data);
    worker.onerror = (event) => {
      if (this.worker === worker)
        this.fail(new Error(`Preview worker stopped (${event.message || "error"}).`));
    };
    return worker;
  }
  private send(worker: Worker, message: { id?: number; [key: string]: unknown }) {
    const epoch = this.epoch;
    void this.ready!.then(() => {
      // Initialization must be posted first. Ignore cancelled or superseded work
      // that was waiting for the catalog, including background prefetch requests.
      if (this.worker === worker && epoch === this.epoch &&
          (message.id === undefined || this.pending.has(message.id))) worker.postMessage(message);
    }).catch((error) => {
      if (this.worker === worker) this.fail(new Error(String(error)));
    });
  }
  private async receive(worker: Worker, message: {
    host?: { id: number; op: string; args: { name?: string; stamp?: string } };
    id?: number;
    progress?: boolean;
    payload?: FitPayload;
    error?: string;
  }) {
    if (this.worker !== worker) return;
    if (message.host) {
      const { id, op, args } = message.host;
      try {
        if (op === "resource") {
          const value = await invoke<ArrayBuffer>("preview_resource", { name: args.name });
          if (this.worker === worker) worker.postMessage({ hostReply: { id, value } }, [value]);
          return;
        }
        const channel = op === "index" ? "previews:fit-index" : op === "key" ? "previews:fit-key" : "";
        if (!channel) throw new Error("Unknown preview request");
        const value = await call(channel, args);
        if (this.worker === worker) worker.postMessage({ hostReply: { id, value } });
      } catch (error) {
        if (this.worker === worker) worker.postMessage({ hostReply: { id, error: String(error) } });
      }
      return;
    }
    const item = message.id === undefined ? undefined : this.pending.get(message.id);
    if (!item || message.id === undefined) return;
    if (message.progress) {
      if (message.payload) item.onProgress?.(message.payload);
      return;
    }
    this.pending.delete(message.id);
    if (message.error) item.reject(new Error(message.error));
    else item.resolve(message.payload as FitPayload);
  }
  private fail(error: Error) {
    this.worker?.terminate();
    this.worker = null;
    this.ready = null;
    for (const item of this.pending.values()) item.reject(error);
    this.pending.clear();
  }
  warm() {
    const worker = this.ensure();
    if (this.pending.size) return Promise.resolve({ busy: true });
    const id = ++this.sequence;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (v: FitPayload) => void, reject });
      this.send(worker, { id, request: { warm: true } });
    });
  }
  prefetch(asset: string) {
    if (!/^[a-zA-Z0-9_]{1,200}$/.test(asset)) return false;
    this.send(this.ensure(), { prefetch: asset });
    return true;
  }
  render(request: FitRequest, onProgress?: (payload: FitPayload) => void) {
    const worker = this.ensure();
    const known = (list: string[]) => list.filter((x) => /^[a-f0-9]{64}$/.test(x)).slice(0, 256);
    const id = ++this.sequence;
    for (const [old, item] of this.pending) {
      item.reject(new Error("Preview superseded."));
      this.pending.delete(old);
    }
    return new Promise<FitPayload>((resolve, reject) => {
      this.pending.set(id, { resolve, reject, onProgress });
      this.send(worker, {
        id,
        request: {
          items: request.items,
          knownMeshes: known(request.knownMeshes),
          knownTextures: known(request.knownTextures),
          showMannequinEarly: request.showMannequinEarly === true,
        },
      });
    });
  }
  cancel() {
    this.epoch++;
    this.worker?.postMessage({ cancel: true });
    void call("previews:fit-cancel").catch(() => {});
    for (const item of this.pending.values()) item.reject(new Error("Preview cancelled."));
    this.pending.clear();
  }
}
export const fitPreview = new FitPreviewHost();
