const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const esbuild = require('../build-tools/node_modules/esbuild');
const source = esbuild.transformSync(fs.readFileSync(require.resolve('../web/ui/app.ts'), 'utf8'), { loader: 'ts', target: 'es2022' }).code;
const context = vm.createContext({});
vm.runInContext(source.slice(source.indexOf('function healthStatusLabel('), source.indexOf('function renderHealthReport(')), context);
for (const [status, label, expected] of [
  ['pass', '', 'Passed'], ['warning', '', 'Warning'], ['error', '', 'Error'], ['info', '', 'Optional'],
  ['info', 'Info', 'Info'], ['info', 'Unverified', 'Unverified'], ['info', 'Passed', 'Optional'],
  ['warning', 'Info', 'Warning'], ['error', 'Unverified', 'Error'], ['unknown', '', 'Info'],
]) assert.equal(context.healthStatusLabel(status, label), expected);
assert(source.includes('healthStatusLabel(check.status, check.statusLabel)'), 'Health renderer did not pass its diagnostic label');
console.log('Health status labels: ten prerequisite, unverified, optional and severity-preservation cases passed.');
