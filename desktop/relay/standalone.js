#!/usr/bin/env node
'use strict';
/* Runs the relay on a server (no desktop app). Meant to sit behind an HTTPS reverse proxy.
     node standalone.js --projects ~/projects --port 47601 --url https://cm.example.com
     node standalone.js --link          print the pairing link and exit */
const path = require('path');
const os = require('os');
const fs = require('fs');
const { start } = require('./server');

const args = process.argv.slice(2);
const get = (k, d) => { const i = args.indexOf('--' + k); return i >= 0 && args[i + 1] ? args[i + 1] : d; };
const dir = get('dir', path.join(os.homedir(), '.config', 'hearth'));
const url = get('url', '');
const name = get('name', os.hostname());

function link(token) {
  return 'hearth://pair?' + new URLSearchParams({ url, token, name }).toString();
}

if (args.includes('--link')) {
  try {
    const token = JSON.parse(fs.readFileSync(path.join(dir, 'relay.json'), 'utf8')).token;
    console.log(url ? link(token) : token);
  } catch { console.error('The relay has not been started yet.'); process.exit(1); }
  process.exit(0);
}

// Memory: start again with the heap limit the setting asks for (75% of the free memory unless you chose another share). Only when this process was not given a limit already.
if (!process.env.HEARTH_MEM_CHILD && !args.includes('--no-memory-limit')) {
  // This program only watches. The server itself runs in a second process: when that one stops answering (it hangs, or it is out of memory and crawling), it is stopped and started again,
  // so a stuck server comes back by itself in a minute or two, without anyone logging in to it.
  let cfg = {}; try { cfg = JSON.parse(fs.readFileSync(path.join(dir, 'relay.json'), 'utf8')); } catch { /* first start */ }
  const mem = require('./memory'), mb = process.execArgv.some((x) => x.startsWith('--max-old-space-size')) ? 0 : mem.limitMb(mem.pctOf(cfg));
  const http = require('http'), env = (k, d) => Number(process.env[k]) || d;
  const EVERY = env('HEARTH_WD_INTERVAL_MS', 20000), TIMEOUT = env('HEARTH_WD_TIMEOUT_MS', 10000), FAILS = env('HEARTH_WD_FAILS', 3), GRACE = env('HEARTH_WD_GRACE_MS', 90000);
  const host = (get('host', '127.0.0.1') === '0.0.0.0' ? '127.0.0.1' : get('host', '127.0.0.1')), port = Number(get('port', '47601'));
  let child = null, killedByUs = false, startedAt = 0, fails = 0, busy = false; const kills = [];
  const launch = () => {
    killedByUs = false; fails = 0; startedAt = Date.now();
    child = require('child_process').spawn(process.execPath, [...(mb ? [`--max-old-space-size=${mb}`] : []), ...process.execArgv, __filename, ...process.argv.slice(2)], { stdio: 'inherit', env: { ...process.env, HEARTH_MEM_CHILD: '1' } });
    child.on('exit', (code, sig) => { if (killedByUs) { console.error(new Date().toISOString(), 'watchdog: the server was stopped because it did not answer; starting it again'); launch(); } else process.exit(code == null ? (sig ? 1 : 0) : code); });
    child.on('error', () => process.exit(1));
  };
  for (const sig of ['SIGTERM', 'SIGINT', 'SIGHUP']) process.on(sig, () => { killedByUs = false; try { child.kill(sig); } catch { /* gone */ } });
  const ping = () => new Promise((resolve) => { const r = http.get({ host, port, path: '/health', timeout: TIMEOUT }, (res) => { res.resume(); res.on('end', () => resolve(res.statusCode === 200)); }); r.on('timeout', () => { r.destroy(); resolve(false); }); r.on('error', () => resolve(false)); });
  setInterval(async () => {
    if (busy || !child || killedByUs || Date.now() - startedAt < GRACE) return;
    busy = true; const ok = await ping(); busy = false;
    if (ok) { fails = 0; return; }
    if (++fails < FAILS) return;
    const now = Date.now(); while (kills.length && now - kills[0] > 10 * 60000) kills.shift(); kills.push(now);
    if (kills.length > 5) { console.error(new Date().toISOString(), 'watchdog: the server keeps getting stuck; giving up so the service manager can take over'); process.exit(1); }
    console.error(new Date().toISOString(), `watchdog: no answer from the server for ${FAILS} checks in a row; restarting it`);
    killedByUs = true; try { child.kill('SIGKILL'); } catch { launch(); }
  }, EVERY).unref();
  launch();
  return;   // this process only watches, the child is the server
}

start({
  dir, name,
  host: get('host', '127.0.0.1'),
  port: Number(get('port', '47601')),
  publicMode: !args.includes('--lan'),
  projectsRoot: args.includes('--lan') && !args.includes('--projects') ? null : get('projects', path.join(os.homedir(), 'projects')),
  allowBypass: args.includes('--allow-bypass'),
  selfUpdate: true, relayDir: __dirname,
  moveRoot: get('move-root', ''),
  seedToken: get('seed-token-file', '') ? (() => { try { return fs.readFileSync(get('seed-token-file', ''), 'utf8'); } catch { return ''; } })() : '',
  voice: get('voice-port', '') ? { port: Number(get('voice-port', '')), tokenFile: get('voice-token-file', '') } : null,
  onLog: (m) => console.error(new Date().toISOString(), m),
}).then((r) => {
  if (!r.ok) { console.error('Could not start:', r.error); process.exit(1); }
  console.log(`Hearth relay listening on ${get('host', '127.0.0.1')}:${r.port}`);
  console.log(`Projects folder: ${r.root}`);
  if (url && process.stdout.isTTY) console.log('Pairing link (keep it secret):', link(r.token));  // never into service logs
  // the desktop app drops the server link here when this service (not the app) owns the phone connection
  const linkFile = path.join(dir, 'link.json'); let lastLink = '';
  // zero setup: without a link file, use the server you connected in the desktop app
  const fromApp = () => {
    for (const p of [path.join(os.homedir(), '.config', 'Hearth', 'state.json'), path.join(process.env.APPDATA || '', 'Hearth', 'state.json')]) {
      try { const m = (JSON.parse(fs.readFileSync(p, 'utf8')).machines || []).find((x) => x.secure && x.token); if (m) return JSON.stringify({ url: `https://${m.host}${m.port && m.port !== 443 ? ':' + m.port : ''}`, name: m.name, token: m.token }); } catch { /* next */ }
    }
    return '';
  };
  const readLink = () => { let t = ''; try { t = fs.readFileSync(linkFile, 'utf8'); } catch { /* none */ } if (!t.trim() && args.includes('--lan')) t = fromApp(); if (t !== lastLink) { lastLink = t; try { r.setLink(t ? JSON.parse(t) : null); } catch { r.setLink(null); } } };
  readLink(); setInterval(readLink, 10000).unref();
  const stop = () => { r.stop(); process.exit(0); };
  process.on('SIGTERM', stop); process.on('SIGINT', stop);
});
