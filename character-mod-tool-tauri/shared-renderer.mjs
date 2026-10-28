// Shared renderer sync for tools outside the Exporter (Character Mod Tool,
// Roster Editor). Canonical source: 2k Exporter/shared/model-preview. This
// script is itself part of the canonical package and is copied next to each
// tool's vendored renderer on every sync; do not edit a tool's copy.
//
//   node shared-renderer.mjs <destination> [--verify] [--source <model-preview dir>]
//
// With the canonical folder available (--source, NBA2K_SHARED_RENDERER, or the
// default path) the destination is replaced by the canonical renderer and
// shared-renderer.lock.json is rewritten. --verify never copies: it fails if
// the destination differs from the canonical renderer. Without the canonical
// folder (another machine, CI) the destination is checked against its own lock,
// so a committed copy still builds and hand edits still fail.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { fileURLToPath } from 'node:url';

const LOCK = 'shared-renderer.lock.json';
const DEFAULT_SOURCE = 'D:/Projects/2k Exporter/shared/model-preview';
const self = fileURLToPath(import.meta.url);

const args = process.argv.slice(2);
const verifyOnly = args.includes('--verify');
const sourceFlag = args.indexOf('--source');
const destination = args.find((value, index) => !value.startsWith('--') && args[index - 1] !== '--source');
if (!destination) throw new Error('Usage: node shared-renderer.mjs <destination> [--verify] [--source <dir>]');
const sourceRoot = path.resolve(sourceFlag >= 0 ? args[sourceFlag + 1] : process.env.NBA2K_SHARED_RENDERER || DEFAULT_SOURCE);
const target = path.resolve(destination);
const canonical = path.join(sourceRoot, 'public', 'renderer');

const sha256 = (file) => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex').toUpperCase();
function listFiles(root, base = root) {
  if (!fs.existsSync(root)) return [];
  return fs.readdirSync(root, { withFileTypes: true }).flatMap((entry) => {
    const full = path.join(root, entry.name);
    return entry.isDirectory() ? listFiles(full, base) : [path.relative(base, full).split(path.sep).join('/')];
  }).sort();
}
// Same digest as Sync-ModelPreview.ps1, over the renderer files only.
function revisionOf(files) {
  const text = Object.keys(files).sort().map((key) => `renderer/${key}:${files[key]}`).join('\n');
  return crypto.createHash('sha256').update(text, 'utf8').digest('hex').toUpperCase();
}
function hashes(root) {
  return Object.fromEntries(listFiles(root).filter((file) => file !== LOCK).map((file) => [file, sha256(path.join(root, file))]));
}
function differences(expected, actual) {
  const keys = new Set([...Object.keys(expected), ...Object.keys(actual)]);
  return [...keys].sort().flatMap((key) => !(key in actual) ? [`${key}: missing`]
    : !(key in expected) ? [`${key}: extra file`] : expected[key] !== actual[key] ? [`${key}: changed`] : []);
}
function fail(title, problems) {
  throw new Error(`${title}\n  ${problems.slice(0, 20).join('\n  ')}${problems.length > 20 ? `\n  ... ${problems.length - 20} more` : ''}`);
}

if (fs.existsSync(canonical)) {
  const wanted = hashes(canonical);
  if (!verifyOnly) {
    // Only a previous copy (it has a lock) is cleaned; never an arbitrary folder.
    const existing = listFiles(target);
    if (existing.length && !existing.includes(LOCK)) throw new Error(`${target} is not empty and has no ${LOCK}; refusing to replace it.`);
    for (const file of existing) if (file !== LOCK && !(file in wanted)) fs.rmSync(path.join(target, file));
    for (const file of Object.keys(wanted)) {
      fs.mkdirSync(path.dirname(path.join(target, file)), { recursive: true });
      fs.copyFileSync(path.join(canonical, file), path.join(target, file));
    }
    const lock = { schema: 1, source: 'carrnate85-stack 2k Exporter shared/model-preview', revision: revisionOf(wanted), files: wanted };
    fs.writeFileSync(path.join(target, LOCK), JSON.stringify(lock, null, 2) + '\n');
    // Keep the tool's copy of this script current (it sits beside the lock's folder).
    const script = path.join(sourceRoot, 'consumer', 'shared-renderer.mjs');
    if (fs.existsSync(script) && path.resolve(script) !== self) fs.copyFileSync(script, self);
  }
  const problems = differences(wanted, hashes(target));
  if (problems.length) fail(`Shared renderer copy at ${target} differs from ${canonical}:`, problems);
  console.log(`Shared renderer ${revisionOf(wanted).slice(0, 12)} ${verifyOnly ? 'verified at' : 'synced into'} ${target}`);
} else {
  const lockFile = path.join(target, LOCK);
  if (!fs.existsSync(lockFile)) throw new Error(`No canonical renderer at ${canonical} and no ${LOCK} in ${target}.`);
  const lock = JSON.parse(fs.readFileSync(lockFile, 'utf8'));
  const problems = differences(lock.files, hashes(target));
  if (problems.length) fail(`Vendored shared renderer at ${target} does not match its lock (edit the canonical renderer and resync instead):`, problems);
  if (revisionOf(lock.files) !== lock.revision) throw new Error(`${LOCK} revision does not match its file list.`);
  console.log(`Shared renderer ${lock.revision.slice(0, 12)} verified against ${LOCK} (canonical folder not present).`);
}
