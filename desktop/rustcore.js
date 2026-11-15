'use strict';
/* The desktop app's fast part: hearth-relay in "core only" mode, a small Rust program that reads the chat logs (titles, messages, search, usage) much faster than Node.
   The relay process gets its address and secret from the environment (see relay/core.js). When the program is missing or does not start, the app works as before. */
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const { spawn } = require('child_process');

const EXE = process.platform === 'win32' ? 'hearth-relay.exe' : 'hearth-relay';

/** Where the program is: next to the app (installed), or in the Rust build folder (checkout). */
function findBin(extra = []) {
  const res = process.resourcesPath || '';
  for (const p of [...extra, res && path.join(res, 'bin', EXE), path.join(__dirname, 'bin', EXE), path.join(__dirname, 'relay-rs', 'target', 'release', EXE)].filter(Boolean)) {
    try { if (fs.statSync(p).isFile()) return p; } catch { /* not there */ }
  }
  return '';
}

/** Starts it. Resolves with { env, stop } or null. Never throws. */
function start({ bin = findBin(), timeoutMs = 5000 } = {}) {
  return new Promise((resolve) => {
    if (!bin || process.env.HEARTH_NO_RUST) return resolve(null);
    const secret = crypto.randomBytes(24).toString('hex');
    let child;
    try { child = spawn(bin, ['--core-only', '--port', '0', '--secret', secret], { stdio: ['ignore', 'pipe', 'ignore'], windowsHide: true }); } catch { return resolve(null); }
    let buf = '', done = false;
    const finish = (v) => { if (done) return; done = true; clearTimeout(timer); resolve(v); };
    const stop = () => { try { child.kill(); } catch { /* gone */ } };
    const timer = setTimeout(() => { stop(); finish(null); }, timeoutMs);
    child.stdout.on('data', (d) => {
      buf += d;
      const m = /listening (\d+)/.exec(buf);
      if (m) finish({ env: { HEARTH_RUST_CORE: `http://127.0.0.1:${m[1]}`, HEARTH_RUST_CORE_SECRET: secret }, stop, child });
    });
    child.on('error', () => finish(null));
    child.on('exit', () => finish(null));
  });
}
module.exports = { start, findBin };
