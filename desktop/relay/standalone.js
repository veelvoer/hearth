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
      try { const m = (JSON.parse(fs.readFileSync(p, 'utf8')).machines || []).find((x) => x.secure && x.token); if (m) return JSON.stringify({ url: `https://${m.host}${m.port && m.port !== 443 ? ':' + m.port : ''}`, token: m.token }); } catch { /* next */ }
    }
    return '';
  };
  const readLink = () => { let t = ''; try { t = fs.readFileSync(linkFile, 'utf8'); } catch { /* none */ } if (!t.trim() && args.includes('--lan')) t = fromApp(); if (t !== lastLink) { lastLink = t; try { r.setLink(t ? JSON.parse(t) : null); } catch { r.setLink(null); } } };
  readLink(); setInterval(readLink, 10000).unref();
  const stop = () => { r.stop(); process.exit(0); };
  process.on('SIGTERM', stop); process.on('SIGINT', stop);
});
