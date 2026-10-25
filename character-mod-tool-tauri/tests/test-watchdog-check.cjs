const assert = require('node:assert/strict');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const helper = JSON.stringify(path.join(__dirname, 'test-watchdog.cjs'));
for (const completed of [false, true]) {
  const code = `const done = require(${helper})('incomplete fixture', 50); ${completed ? 'done();' : 'void new Promise(() => {});'}`;
  const result = spawnSync(process.execPath, ['-e', code], { encoding: 'utf8', timeout: 3000, windowsHide: true });
  assert.ifError(result.error); assert.equal(result.signal, null);
  assert.equal(result.status, completed ? 0 : 1);
  if (completed) assert.equal(result.stderr, ''); else assert.match(result.stderr, /Test did not finish: incomplete fixture/);
}
console.log('Test watchdog: unfinished promise fails; completed test clears its timer and exits successfully.');
