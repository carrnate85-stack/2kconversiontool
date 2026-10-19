'use strict';

function waitForExit(child, timeoutMs) {
  if (!child || child.exitCode != null || child.signalCode) return Promise.resolve(true);
  if(typeof child.once!=='function'||typeof child.removeListener!=='function')return Promise.resolve(true);
  return new Promise(resolve => {
    let settled = false;
    const finish = value => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      child.removeListener('exit', onExit);
      child.removeListener('error', onError);
      resolve(value);
    };
    const onExit = () => finish(true);
    // A pipe/process error after spawn is not proof the worker has exited.
    const onError = () => { if (!child.pid || child.exitCode != null || child.signalCode) finish(true); };
    const timer = setTimeout(() => finish(false), timeoutMs);
    timer.unref?.();
    child.once('exit', onExit);
    child.once('error', onError);
  });
}

async function stopChild(child, { timeoutMs = 1500, graceful = null, forceKill = true } = {}) {
  if (!child || child.exitCode != null || child.signalCode) return true;
  try {
    if (graceful) graceful(child);
    else child.stdin?.end();
  } catch {}
  if(typeof child.kill!=='function')return true;
  if (await waitForExit(child, timeoutMs)) return true;
  if (!forceKill) return false;
  try { child.kill(); } catch {}
  return waitForExit(child, Math.min(timeoutMs, 1000));
}

module.exports = { stopChild, waitForExit };
