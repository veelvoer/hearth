'use strict';
/* Runs the relay in its own process for the desktop app, so heavy log reading can never freeze the window.
   Talks to the app over the fork() IPC channel. */
const { start } = require('./server');
const dir = process.env.CM_DIR;
let r = null, lastCode = '';
const say = (m) => { try { process.send(m); } catch { /* app gone */ } };
start({ dir, moveRoot: process.env.CM_MOVE_ROOT || '', onLog: (m) => say({ t: 'log', m }) }).then((x) => {
  r = x;
  if (!x.ok) return say({ t: 'ready', ok: false, inUse: x.inUse, error: x.error });
  say({ t: 'ready', ok: true, port: x.port, token: x.token, addresses: x.addresses(), code: x.pairCode() });
  setInterval(() => { const c = x.pairCode(); if (c !== lastCode) { lastCode = c; say({ t: 'code', code: c }); } }, 1500);
});
process.on('message', (m) => {
  if (!r || !r.ok || !m) return;
  if (m.t === 'newcode') say({ t: 'code', code: (lastCode = r.newPairCode()) });
  else if (m.t === 'away') r.setAway(!!m.on);
  else if (m.t === 'link') r.setLink(m.link);
  else if (m.t === 'addresses') say({ t: 'addresses', addresses: r.addresses() });
});
process.on('disconnect', () => { try { if (r && r.ok) r.stop(); } finally { process.exit(0); } });
