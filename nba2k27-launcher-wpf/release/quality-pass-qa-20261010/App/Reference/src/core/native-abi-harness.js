const fs = require('node:fs');
const fsp = require('node:fs/promises');
const path = require('node:path');
const crypto = require('node:crypto');
const { execFile } = require('node:child_process');
const { promisify } = require('node:util');

const execFileAsync = promisify(execFile);
const EXPECTED_HARNESS_SHA256 = 'd7f938a65b9e010d094d37e98a6a9f94e2cb68014c0867b74cda3ba08a0e47ea';

function developmentHarnessPath() {
  return path.join(__dirname, '..', '..', 'native-abi-harness', 'dist', 'NBA2K27NativeAbiHarness.exe');
}

async function sha256File(filePath) {
  const hash = crypto.createHash('sha256');
  await new Promise((resolve, reject) => {
    const stream = fs.createReadStream(filePath);
    stream.on('data', (chunk) => hash.update(chunk));
    stream.once('error', reject);
    stream.once('end', resolve);
  });
  return hash.digest('hex');
}

function validateReport(report) {
  if (report?.schema !== 'nba2k27-native-abi-harness-v2') throw new Error('Native ABI harness returned an unsupported report.');
  if (report.mode !== 'standalone-offline') throw new Error('Native ABI harness did not run in standalone-offline mode.');
  const checks = report.checks || {};
  const required = [
    'layout',
    'embeddedOwnerLayout',
    'stockVtableClone',
    'overrideIsolation',
    'allocatorProvenance',
    'destructorOwnership',
    'activation',
    'modPrecedence',
    'stockFallback',
    'faultIsolation',
    'rollback',
    'forcedFailureRollback',
    'duplicateIdRejected',
    'finalCleanup',
  ];
  if (!report.passed || required.some((name) => checks[name] !== true)) {
    throw new Error('Native ABI harness did not pass every required lifecycle check.');
  }
  if (report.safety?.gameOpened || report.safety?.processAttached
    || report.safety?.hooksInstalled || report.safety?.gameFilesModified) {
    throw new Error('Native ABI harness reported an unsafe runtime action.');
  }
  return report;
}

async function runNativeAbiHarness(filePath = developmentHarnessPath()) {
  const resolved = path.resolve(filePath);
  const stat = await fsp.stat(resolved).catch((error) => {
    if (error.code === 'ENOENT') return null;
    throw error;
  });
  if (!stat?.isFile()) {
    return { available: false, passed: false, path: resolved, reason: 'Native ABI harness executable is missing.' };
  }
  const sha256 = await sha256File(resolved);
  if (sha256 !== EXPECTED_HARNESS_SHA256) {
    return { available: true, passed: false, path: resolved, sha256, reason: 'Native ABI harness hash does not match this launcher build.' };
  }
  const { stdout, stderr } = await execFileAsync(resolved, [], {
    windowsHide: true,
    timeout: 5000,
    maxBuffer: 1024 * 1024,
  });
  if (stderr.trim()) throw new Error(`Native ABI harness wrote unexpected error output: ${stderr.trim()}`);
  const report = validateReport(JSON.parse(stdout));
  return { available: true, path: resolved, sha256, ...report };
}

module.exports = {
  EXPECTED_HARNESS_SHA256,
  developmentHarnessPath,
  runNativeAbiHarness,
  validateReport,
};
