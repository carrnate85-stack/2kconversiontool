import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { call } from "./api";
import { FitPreviewHost, type FitPayload } from "./fit-preview-host";

vi.mock("./api", () => ({ call: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
class FakeWorker {
  static instances: FakeWorker[] = [];
  sent: any[] = [];
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onerror: ((event: { message: string }) => void) | null = null;
  constructor() { FakeWorker.instances.push(this); }
  postMessage(value: unknown) { this.sent.push(value); }
  terminate() {}
  reply(value: unknown) { this.onmessage?.({ data: value }); }
}
const request = { items: ["tanktop_solid_gry"], knownMeshes: [], knownTextures: [], showMannequinEarly: true };
const payload = { parts: [], warnings: [], stats: { ms: 1, cache: { memory: 1, disk: 0, decoded: 0 } } } satisfies FitPayload;
const flush = async () => { for (let i = 0; i < 8; i++) await Promise.resolve(); };
beforeEach(() => { vi.clearAllMocks(); FakeWorker.instances = []; vi.stubGlobal("Worker", FakeWorker); });
afterEach(() => vi.unstubAllGlobals());

test("first open waits for initialization and renders without changing clothes", async () => {
  const originals = deferred<string[]>();
  vi.mocked(call).mockReturnValue(originals.promise);
  const host = new FitPreviewHost();
  const warming = host.warm().catch(error => error.message);
  const result = host.render(request);
  const worker = FakeWorker.instances[0];
  expect(worker.sent).toEqual([]);
  originals.resolve(["tanktop_solid_gry"]);
  await flush();
  expect(worker.sent).toHaveLength(2);
  expect(worker.sent[0]).toEqual({ init: { originals: ["tanktop_solid_gry"] } });
  expect(worker.sent[1].request.items).toEqual(request.items);
  expect(await warming).toBe("Preview superseded.");
  worker.reply({ id: worker.sent[1].id, payload });
  expect(await result).toBe(payload);
});

test("close before initialization discards queued work; reopening renders current choices", async () => {
  const originals = deferred<string[]>();
  vi.mocked(call).mockReturnValue(originals.promise);
  const host = new FitPreviewHost();
  host.prefetch("old_top");
  const cancelled = host.render(request).catch(error => error.message);
  host.cancel();
  const current = host.render({ ...request, items: ["new_top"] });
  originals.resolve([]);
  await flush();
  expect(await cancelled).toBe("Preview cancelled.");
  const worker = FakeWorker.instances[0];
  expect(worker.sent.filter(x => x.prefetch)).toHaveLength(0);
  const renders = worker.sent.filter(x => x.request);
  expect(renders).toHaveLength(1);
  expect(renders[0].request.items).toEqual(["new_top"]);
  worker.reply({ id: renders[0].id, payload });
  await current;
});

test("failed initialization rejects the render and can retry instead of hanging", async () => {
  vi.mocked(call).mockRejectedValueOnce(new Error("Catalog unavailable")).mockResolvedValue([]);
  const host = new FitPreviewHost();
  await expect(host.render(request)).rejects.toThrow("Catalog unavailable");
  const retry = host.render(request);
  await flush();
  const worker = FakeWorker.instances[1];
  expect(worker.sent[0]).toHaveProperty("init");
  worker.reply({ id: worker.sent[1].id, payload });
  expect(await retry).toBe(payload);
});
