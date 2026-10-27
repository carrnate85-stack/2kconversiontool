// Headless staged-player preview, mirroring loadStaged() in web/tauri-bridge.ts:
// the app's Rust backend (--serve-rpc) prepares native resources, the bundled
// preview worker decodes them, and the payload is written as JSON for the harness.
//
//   node decode-player.cjs --game <NBA 2K27 folder> <player.iff> [out.json]
//
// Options (or environment variables):
//   --game  NBA2K27_ROOT   NBA 2K27 install folder (required)
//   --exe   CMT_EXE        app binary (default: ../../target/release/character-mod-tool-tauri.exe)
//   --data  CMT_TEST_DATA  isolated app data folder (default: work/cmt-data)
//   --config               accessory configuration, e.g. flat_top (default: the player's default)
// The default output is harness/players/<player>[-<config>].json.
const { spawn } = require('child_process');
const fs = require('fs');
const path = require('path');
const vm = require('vm');
const readline = require('readline');

const tool = __dirname;
const app = path.resolve(tool, '../..');
const options = {}, positional = [];
for (let index = 2; index < process.argv.length; index++) {
  const arg = process.argv[index];
  if (arg.startsWith('--')) options[arg.slice(2)] = process.argv[++index];
  else positional.push(arg);
}
const game = options.game || process.env.NBA2K27_ROOT;
const exe = path.resolve(options.exe || process.env.CMT_EXE || path.join(app, 'target/release/character-mod-tool-tauri.exe'));
const data = path.resolve(options.data || process.env.CMT_TEST_DATA || path.join(tool, 'work/cmt-data'));
const [target, outArg] = positional;
if (!game || !target) {
  console.error('Usage: node decode-player.cjs --game <NBA 2K27 folder> <player.iff> [out.json]');
  process.exit(2);
}
if (!fs.existsSync(path.join(game, 'manifest'))) throw new Error(`No NBA 2K27 manifest in ${game}`);
if (!fs.existsSync(exe)) throw new Error(`App binary not found: ${exe} (cargo build --release first)`);
const stem = path.basename(target, path.extname(target)) + (options.config ? `-${options.config}` : '');
const outFile = path.resolve(outArg || path.join(tool, 'harness/players', stem + '.json'));
fs.mkdirSync(data, { recursive: true });
fs.mkdirSync(path.dirname(outFile), { recursive: true });
// --serve-rpc honours CMT_TEST_DATA, so the real app settings are never read or written.
fs.writeFileSync(path.join(data, 'settings.json'), JSON.stringify({ game_root_2k27: path.resolve(game) }));

const backend = spawn(exe, ['--serve-rpc'], { env: { ...process.env, CMT_TEST_DATA: data }, stdio: ['pipe', 'pipe', 'inherit'] });
let sequence = 0;
const pending = new Map();
readline.createInterface({ input: backend.stdout, crlfDelay: Infinity }).on('line', (line) => {
  const message = JSON.parse(line);
  if (message.kind === 'event') return;
  const task = pending.get(message.id);
  pending.delete(message.id);
  if (message.error) task.reject(new Error(message.error)); else task.resolve(message.value);
});
function call(command, payload) {
  return new Promise((resolve, reject) => {
    const id = ++sequence;
    pending.set(id, { resolve, reject });
    backend.stdin.write(JSON.stringify({ id, command, payload: { ...payload, _cmtPreviewKind: 'staged' } }) + '\n');
  });
}

// The worker bundle replaces globalThis.process and Buffer, so it runs in its own realm.
const listeners = [];
let finish;
const sandbox = {
  console, setTimeout, clearTimeout, setInterval, clearInterval, queueMicrotask, structuredClone,
  TextDecoder, TextEncoder, atob, btoa, performance, URL,
  addEventListener: (type, listener) => { if (type === 'message') listeners.push(listener); },
  postMessage: async (message) => {
    if (message.kind !== 'native') { finish(message); return; }
    let reply;
    try { reply = { kind: 'native-reply', id: message.id, value: await call(message.command, message.payload) }; }
    catch (error) { reply = { kind: 'native-reply', id: message.id, error: String(error) }; }
    listeners.forEach((listener) => listener({ data: reply }));
  },
};
sandbox.globalThis = sandbox;
sandbox.self = sandbox;
vm.createContext(sandbox);
vm.runInContext(fs.readFileSync(path.join(app, 'frontend/preview-worker.bundle.js'), 'utf8'), sandbox,
  { filename: 'preview-worker.bundle.js' });

// Typed arrays become tagged base64; harness/app.mjs rebuilds them.
function encode(value) {
  if (ArrayBuffer.isView(value)) {
    return { __typed: value.constructor.name, base64: Buffer.from(value.buffer, value.byteOffset, value.byteLength).toString('base64') };
  }
  if (Array.isArray(value)) return value.map(encode);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, encode(item)]));
  return value;
}

(async () => {
  const started = Date.now();
  const resources = await call('native-preview-resources', { target: path.resolve(target), config: options.config || '' });
  const result = await new Promise((resolve) => {
    finish = resolve;
    listeners.forEach((listener) => listener({ data: { id: 1, type: 'preview', payload: { filePath: resources.filePath, options: resources.options } } }));
  });
  if (result.error) throw new Error(result.error.message || String(result.error));
  const preview = result.result;
  Object.assign(preview, { player: resources.player, name: resources.name, activeItems: resources.activeItems,
    companionNames: resources.options.companionPaths.map((item) => item.name) });
  preview.skipped.push(...resources.warnings);
  fs.writeFileSync(outFile, JSON.stringify(encode(preview)));
  console.log(`${path.relative(process.cwd(), outFile)}: ${preview.meshes.length} meshes, ${preview.textures.length} textures, `
    + `${preview.stats.triangles} triangles in ${Date.now() - started} ms`);
  for (const note of preview.skipped) console.log(`  note: ${note}`);
  backend.stdin.end();
})().catch((error) => { console.error('Decode failed:', error.stack || error); backend.kill(); process.exitCode = 1; });
