const fs = require('node:fs');
const fsp = require('node:fs/promises');
const path = require('node:path');
const crypto = require('node:crypto');
const { execFile } = require('node:child_process');
const { promisify } = require('node:util');

const execFileAsync = promisify(execFile);
const HOST_FILENAME = 'NBA2K27CrossModuleHost.exe';
const FIXTURE_FILENAME = 'NBA2K27CrossModuleFixture.dll';
const EXPECTED_HOST_SHA256 = '96a6e0635a2f1f4d098ed1cd93a6b24fa28df8ce855c01d085fbfe1cb1a39721';
const EXPECTED_FIXTURE_SHA256 = 'aba6d7cc250b1eb63c281c6b36b0e2d2eca4877a0fa068c37b540dec63e58402';

function developmentHarnessDirectory() {
  return path.join(__dirname, '..', '..', 'cross-module-harness', 'dist');
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
  if (report?.schema !== 'nba2k27-cross-module-lifetime-v1') {
    throw new Error('Cross-module harness returned an unsupported report.');
  }
  if (report.mode !== 'standalone-offline' || report.cycles !== 2) {
    throw new Error('Cross-module harness did not run the required offline cycles.');
  }
  const required = [
    'moduleLoaded',
    'invalidVersionRejected',
    'abiNegotiated',
    'objectCreated',
    'registration',
    'prematureDestroyRejected',
    'crossModuleDispatch',
    'modPrecedence',
    'stockFallback',
    'faultIsolation',
    'callbacksQuiesced',
    'handlesClosed',
    'managerRollback',
    'objectDestroyed',
    'forcedFailureRollback',
    'moduleUnloaded',
    'repeatedLoadUnload',
  ];
  if (!report.passed || required.some((name) => report.checks?.[name] !== true)) {
    throw new Error('Cross-module harness did not pass every required lifetime check.');
  }
  if (report.safety?.gameOpened || report.safety?.processAttached
    || report.safety?.hooksInstalled || report.safety?.gameFilesModified) {
    throw new Error('Cross-module harness reported an unsafe runtime action.');
  }
  return report;
}

async function runCrossModuleHarness(directory = developmentHarnessDirectory()) {
  const resolved = path.resolve(directory);
  const hostPath = path.join(resolved, HOST_FILENAME);
  const fixturePath = path.join(resolved, FIXTURE_FILENAME);
  const stats = await Promise.all([hostPath, fixturePath].map((filePath) => (
    fsp.stat(filePath).catch((error) => {
      if (error.code === 'ENOENT') return null;
      throw error;
    })
  )));
  if (stats.some((stat) => !stat?.isFile())) {
    return {
      available: false,
      passed: false,
      directory: resolved,
      reason: 'Cross-module host or fixture DLL is missing.',
    };
  }
  const [hostSha256, fixtureSha256] = await Promise.all([
    sha256File(hostPath),
    sha256File(fixturePath),
  ]);
  if (hostSha256 !== EXPECTED_HOST_SHA256 || fixtureSha256 !== EXPECTED_FIXTURE_SHA256) {
    return {
      available: true,
      passed: false,
      directory: resolved,
      hostSha256,
      fixtureSha256,
      reason: 'Cross-module harness hashes do not match this launcher build.',
    };
  }
  const { stdout, stderr } = await execFileAsync(hostPath, [], {
    cwd: resolved,
    windowsHide: true,
    timeout: 5000,
    maxBuffer: 1024 * 1024,
  });
  if (stderr.trim()) throw new Error(`Cross-module harness wrote unexpected error output: ${stderr.trim()}`);
  const report = validateReport(JSON.parse(stdout));
  return {
    available: true,
    directory: resolved,
    hostSha256,
    fixtureSha256,
    ...report,
  };
}

module.exports = {
  EXPECTED_FIXTURE_SHA256,
  EXPECTED_HOST_SHA256,
  developmentHarnessDirectory,
  runCrossModuleHarness,
  validateReport,
};

