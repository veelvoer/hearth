'use strict';
const { app, BrowserWindow, Tray, Menu, ipcMain, Notification, shell, safeStorage, nativeImage, screen, dialog, nativeTheme } = require('electron');
const path = require('path');
const fs = require('fs');
const os = require('os');
const crypto = require('crypto');
const http = require('http');
const https = require('https');
const dgram = require('dgram');
const { execFile, fork, spawn } = require('child_process');
const relayMod = require('./relay/server');
const updater = require('./updater');

app.setName('Hearth');
if (process.platform === 'win32') app.setAppUserModelId('dev.voer.hearth');   // so the taskbar shows Hearth's own icon, not Electron's
{ // carry over the settings of the app under its former name ("Hearth"), also when Electron already made the new folder
  const here = app.getPath('userData'), oldDir = path.join(path.dirname(here), 'Hearth');
  const read = (f) => { try { return JSON.parse(fs.readFileSync(f, 'utf8')); } catch { return null; } };
  try {
    const oldState = read(path.join(oldDir, 'state.json')), newState = read(path.join(here, 'state.json'));
    if (oldState && (!newState || (!(newState.machines || []).length && (oldState.machines || []).length))) {
      fs.mkdirSync(here, { recursive: true });
      fs.writeFileSync(path.join(here, 'state.json'), JSON.stringify({ ...(newState || {}), ...oldState, settings: { ...oldState.settings, ...(newState && newState.settings) }, deployed: { ...oldState.deployed, ...(newState && newState.deployed) } }), { mode: 0o600 });
      fs.renameSync(path.join(oldDir, 'state.json'), path.join(oldDir, 'state.json.migrated'));
    }
  } catch { /* start fresh */ }
}
if (!app.requestSingleInstanceLock()) { app.quit(); process.exit(0); }

// ───────────────────────── state ─────────────────────────
const stateFile = path.join(app.getPath('userData'), 'state.json');
const DEFAULTS = {
  settings: { interval: 5, thresholds: [75, 90, 100], notifyReset: true, callMe: false, theme: 'system', palette: 'claude', background: true, autostart: false },
  machines: [], deployed: {}, history: [], usage: null, profile: null, tok: null, ln: {}, error: null,
};
let S = (() => {
  try { return { ...DEFAULTS, ...JSON.parse(fs.readFileSync(stateFile, 'utf8')) }; } catch { return structuredClone(DEFAULTS); }
})();
const existing = fs.existsSync(stateFile);   // someone who already uses the app doesn't need the first-run tutorial
S.settings = { ...DEFAULTS.settings, ...S.settings };
if (existing && S.settings.onboarded === undefined) S.settings.onboarded = true;
if (S.tok || S.profile) { delete S.tok; S.profile = null; }  // the app no longer handles any Claude login; forget what older versions stored
let saveTimer = null;
function save() {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    fs.mkdirSync(path.dirname(stateFile), { recursive: true });
    fs.writeFileSync(stateFile, JSON.stringify(S), { mode: 0o600 });
  }, 300);
}

const nowPct = (l, t) => !l ? null : (l.resetsAt && l.resetsAt < t) ? 0 : l.pct;
const dur = (ms) => { const m = Math.max(0, Math.floor(ms / 60000)); return m < 60 ? m + 'm' : m < 1440 ? Math.floor(m / 60) + 'h ' + (m % 60) + 'm' : Math.floor(m / 1440) + 'd ' + Math.floor((m % 1440) / 60) + 'h'; };

/** Desktop notification that can never freeze the app. On Linux, Electron's own notifications wait on D-Bus on the main thread
 *  (up to 25 s each when no notification service answers), so there we hand it to notify-send in a separate process instead. */
function notify(title, body, onClick) {
  if (process.platform === 'linux') {
    try { execFile('notify-send', ['-a', 'Hearth', '-i', iconPath, '-t', '8000', String(title).slice(0, 120), String(body || '').slice(0, 300)], { timeout: 4000 }, () => {}); } catch { /* none installed */ }
    return;
  }
  if (!Notification.isSupported()) return;
  const n = new Notification({ title, body, icon: iconPath });
  if (onClick) n.on('click', onClick);
  n.show();
}

function checkAlerts(prev, u) {
  const thr = [...S.settings.thresholds].sort((a, b) => b - a);
  for (const [k, l, name] of [['session', u.session, 'Session (5h)'], ['week', u.week, 'Weekly']]) {
    if (!l) continue;
    const win = String(Math.floor((l.resetsAt || 0) / 3600000));
    const [w0, d0] = (S.ln[k] || ':0').split(':');
    const done = w0 === win ? Number(d0) : 0;
    const hit = thr.find((t) => l.pct >= t && t > done);
    if (!hit) continue;
    S.ln[k] = win + ':' + hit;
    notify(hit >= 100 ? name + ' limit reached' : name + ' at ' + Math.floor(l.pct) + '%', l.resetsAt ? 'Resets in ' + dur(l.resetsAt - u.at) : 'Crossed ' + hit + '%');
  }
  const ps = prev && prev.session, ns = u.session;
  if (S.settings.notifyReset && ps && ns && ps.pct >= 75 && ns.pct < ps.pct - 30) notify('Session reset', 'A fresh 5-hour window is ready.');
}

let refreshing = false;
/** Plan usage comes from your computers' relays, which read it from the real Claude Code. [ask]: read it again first (one tiny request). */
async function refreshUsage(ask = false) {
  if (refreshing) return S.usage;
  refreshing = true;
  try {
    const ms = machines();
    if (!ms.length) throw new Error('none');
    if (ask) { const first = ms.find((m) => !m.secure) || ms[0]; await relayJson(first, 'POST', '/usage/refresh', {}).catch(() => {}); }
    let best = null;
    for (const m of ms) {
      const o = await relayJson(m, 'GET', '/usage').catch(() => null);
      if (o && o.at && (!best || o.at > best.at)) best = o;
    }
    if (!best) throw new Error('empty');
    const prev = S.usage;
    const u = { session: best.session || null, week: best.week || null, opus: best.opus || null, sonnet: best.sonnet || null, extra: null, at: best.at };
    S.usage = u; S.error = null;
    if (!prev || prev.at !== u.at) {
      const cut = u.at - 30 * 864e5;
      S.history = S.history.filter((s) => s.t >= cut);
      const s = nowPct(u.session, u.at) || 0, w = nowPct(u.week, u.at) || 0, last = S.history[S.history.length - 1];
      if (!last || u.at - last.t > 240000 || last.s !== s || last.w !== w) S.history.push({ t: u.at, s, w });
      checkAlerts(prev, u);
    }
    save();
  } catch (e) {
    S.error = e.message === 'empty' ? 'No usage data yet. Start a Claude Code chat from here, or press Refresh.' : e.message === 'none' ? null : 'Can\'t reach your computer right now.';
  } finally { refreshing = false; }
  broadcast('usage', publicState());
  return S.usage;
}
let pollTimer = null;
function startPolling() {
  clearInterval(pollTimer);
  pollTimer = setInterval(() => refreshUsage(), 60000);  // cheap: reads what the relay already knows
  refreshUsage();
}

// ───────────────────────── relay client ─────────────────────────
const LOCAL_TOKEN = path.join(os.homedir(), '.config', 'hearth-relay', 'token');
let relayInfo = null; // the relay built into this app, when it owns the port
let relayProc = null;
/** Runs the relay in a separate process; resolves when it reports ready. The window never shares a thread with log reading. */
/** Tells the relay which server (if any) to link to, so the server can hand work to this computer and relay its messages to your phone. */
function sendLink() {
  const srv = S.machines.find((m) => m.secure);
  if (relayInfo && relayInfo.inUse) {   // the always-on relay service owns the port: leave the link where it reads it
    try { fs.writeFileSync(path.join(os.homedir(), '.config', 'hearth-node', 'link.json'), srv ? JSON.stringify({ url: `https://${srv.host}${srv.port && srv.port !== 443 ? ':' + srv.port : ''}`, name: srv.name, token: srv.token }) : '', { mode: 0o600 }); } catch { /* service not installed */ }
    return;
  }
  if (!relayProc || !relayProc.connected) return;
  try { relayProc.send({ t: 'link', link: srv ? { url: `https://${srv.host}${srv.port && srv.port !== 443 ? ':' + srv.port : ''}`, name: srv.name, token: srv.token } : null }); } catch { /* relay gone */ }
}
function startRelay() {
  return new Promise((resolve) => {
    let done = false;
    const finish = (v) => { if (!done) { done = true; resolve(v); } };
    const child = fork(path.join(__dirname, 'relay', 'embedded.js'), [], {
      execPath: process.execPath, env: { ...process.env, ELECTRON_RUN_AS_NODE: '1', CM_DIR: app.getPath('userData'), CM_MOVE_ROOT: path.join(os.homedir(), 'projects'), CM_ALLOW_REMOTE: S.settings.allowRemote ? '1' : '0' }, stdio: ['ignore', 'ignore', 'ignore', 'ipc'],
    });
    relayProc = child;
    const info = { ok: false, pairCode: () => info.code || '', addresses: () => info.addrs || [], newPairCode: () => { child.send({ t: 'newcode' }); return info.code || ''; }, stop: () => { try { child.kill(); } catch { /* gone */ } } };
    child.on('message', (m) => {
      if (m.t === 'ready') { Object.assign(info, m.ok ? { ok: true, port: m.port, token: m.token, code: m.code, addrs: m.addresses } : { ok: false, inUse: m.inUse, error: m.error }); if (m.ok) sendLink(); finish(info); }
      else if (m.t === 'code') info.code = m.code;
      else if (m.t === 'addresses') info.addrs = m.addresses;
    });
    child.on('exit', () => { info.ok = false; info.error = info.error || 'relay stopped'; finish(info); });
    setTimeout(() => finish(info), 8000);
  });
}
function machines() {
  const list = S.machines.map((m) => ({ ...m }));
  if (relayInfo && relayInfo.ok) list.unshift({ id: 'local', name: os.hostname() + ' (this computer)', host: '127.0.0.1', port: relayInfo.port, token: relayInfo.token, local: true });
  else try {
    const t = fs.readFileSync(LOCAL_TOKEN, 'utf8').trim();
    list.unshift({ id: 'local', name: os.hostname() + ' (this computer)', host: '127.0.0.1', port: 47601, token: t, local: true });
  } catch { /* relay not installed here */ }
  return list;
}
const machineById = (id) => machines().find((m) => m.id === id);
const pub = (m) => ({ id: m.id, name: m.name, host: m.host, port: m.port, local: !!m.local, secure: !!m.secure });
const lib = (m) => (m.secure ? https : http);

function relayRequest(m, method, p, body, extra = {}) {
  return new Promise((resolve, reject) => {
    const payload = body == null ? null : Buffer.isBuffer(body) ? body : Buffer.from(JSON.stringify(body));
    const req = lib(m).request({ host: m.host, port: m.port, path: p, method, timeout: extra.timeout || 10000, headers: {
      Authorization: 'Bearer ' + m.token, ...(payload ? { 'Content-Type': extra.type || 'application/json', 'Content-Length': payload.length } : {}) } }, resolve);
    req.on('timeout', () => req.destroy(new Error('Timed out')));
    req.on('error', reject);
    if (payload) req.write(payload);
    req.end();
  });
}
async function readAll(res) { const c = []; for await (const d of res) c.push(d); return Buffer.concat(c); }
async function relayJson(m, method, p, body) {
  const res = await relayRequest(m, method, p, body);
  const buf = await readAll(res);
  if (res.statusCode === 401) throw new Error('Wrong token for ' + m.name);
  if (res.statusCode !== 200) throw new Error((safeJson(buf) || {}).error || 'Relay error ' + res.statusCode);
  return JSON.parse(buf.toString() || '{}');
}
const safeJson = (b) => { try { return JSON.parse(b.toString()); } catch { return null; } };

function discover() {
  return new Promise((resolve) => {
    const found = new Map();
    const s = dgram.createSocket('udp4');
    s.on('error', () => { try { s.close(); } catch {} resolve([]); });
    s.on('message', (msg, rinfo) => {
      const o = safeJson(msg);
      if (o) found.set(rinfo.address, { name: o.name || rinfo.address, host: rinfo.address, port: o.port || 47601 });
    });
    s.bind(0, () => {
      s.setBroadcast(true);
      s.send(Buffer.from('CLAUDE_METER_DISCOVER'), 47600, '255.255.255.255');
      setTimeout(() => { try { s.close(); } catch {} resolve([...found.values()]); }, 1300);
    });
  });
}

// ───────────────────────── calls (relay event stream) ─────────────────────────
const listeners = new Map();
function stopEvents() { for (const l of listeners.values()) { l.stopped = true; l.req && l.req.destroy(); } listeners.clear(); }
function startEvents() {
  stopEvents();
  for (const m of machines()) {
    const l = { stopped: false, req: null };
    listeners.set(m.id, l);
    (async function loop() {
      let wait = 3000;
      while (!l.stopped) {
        try {
          const cur = machineById(m.id) || m;
          if (S.settings.callMe) await relayJson(cur, 'POST', '/away', { on: true });
          wait = 3000;
          await new Promise((resolve) => {
            const req = lib(cur).request({ host: cur.host, port: cur.port, path: '/events', headers: { Authorization: 'Bearer ' + cur.token } }, (res) => {
              if (res.statusCode !== 200) return resolve();
              let buf = '';
              res.setEncoding('utf8');
              res.on('data', (d) => {
                buf += d;
                let i;
                while ((i = buf.indexOf('\n\n')) >= 0) {
                  const block = buf.slice(0, i); buf = buf.slice(i + 2);
                  if (block.startsWith('data: ')) { const ev = safeJson(block.slice(6)); if (ev) ring(cur, ev); }
                }
              });
              res.on('end', resolve); res.on('error', resolve);
            });
            l.req = req;
            req.on('error', resolve);
            req.end();
          });
        } catch { /* offline: retry */ }
        if (l.stopped) break;
        await new Promise((r) => setTimeout(r, wait));
        wait = Math.min(wait * 2, 60000);
      }
    })();
  }
}
function notifyEvent(m, ev) {
  if (main && !main.isDestroyed() && main.isVisible() && main.isFocused()) return;  // you're looking at the app: it shows this itself
  const proj = String(ev.cwd || '').replace(/\/$/, '').split('/').pop() || 'Claude';
  const title = ev.kind === 'permission' ? `${proj}: needs your OK` : ev.kind === 'question' ? `${proj}: Claude has a question` : ev.kind === 'error' ? `${proj}: something went wrong` : `${proj}: Claude is done`;
  notify(title, ev.text || ev.title || '', () => { showMain('sessions'); main.webContents.send('open-session', { machine: m.id, session: ev.session, title: ev.title, cwd: ev.cwd, voice: false }); });
}
let callWins = new Map();
function ring(m, ev) {
  if (ev.kind === 'resolved') { if (main && !main.isDestroyed()) main.webContents.send('event', { machine: m.id, ev }); return; }
  if (main && !main.isDestroyed()) main.webContents.send('event', { machine: m.id, ev });
  if (ev.call === false || !S.settings.callMe) { notifyEvent(m, ev); return; }
  const what = ev.kind === 'permission' ? 'needs permission' : ev.kind === 'question' ? 'has a question' : 'is done';
  notify('Claude ' + what, ev.text || ev.title);
  const { width } = screen.getPrimaryDisplay().workAreaSize;
  const w = new BrowserWindow({ width: 380, height: 360, x: width - 400, y: 40, frame: false, alwaysOnTop: true, resizable: false, skipTaskbar: true,
    backgroundColor: '#1F1E1D', icon: iconPath,
    webPreferences: { preload: path.join(__dirname, 'preload.js'), contextIsolation: true, autoplayPolicy: 'no-user-gesture-required' } });
  w.setAlwaysOnTop(true, 'screen-saver');
  const key = m.id + ':' + ev.id;
  callWins.set(key, { w, m, ev });
  w.loadFile(path.join(__dirname, 'renderer', 'call.html'), { query: { key, kind: ev.kind, title: ev.title || 'Claude', text: ev.text || '', permission: ev.req ? '1' : '' } });
  w.on('closed', () => callWins.delete(key));
  setTimeout(() => { if (!w.isDestroyed()) w.close(); }, 45000);
}

// ───────────────────────── windows ─────────────────────────
const iconPath = path.join(__dirname, 'assets', process.platform === 'win32' ? 'icon.ico' : 'icon.png');
let main = null, tray = null, quitting = false;

function showMain(tab) {
  if (!main || main.isDestroyed()) createMain();
  main.show(); main.focus();
  if (tab) main.webContents.send('goto', tab);
}
/** Diagnostics only (CM_PROBE=<file>): drives the real UI with real data and records which step makes the page stop answering. */
function runProbe(win, file) {
  const log = (m) => fs.appendFileSync(file, `${new Date().toISOString().slice(11, 23)} ${m}\n`);
  const run = (code, ms = 12000) => Promise.race([win.webContents.executeJavaScript(code), new Promise((_, rej) => setTimeout(() => rej(new Error('PAGE DID NOT ANSWER in ' + ms / 1000 + 's')), ms))]);
  win.webContents.on('console-message', (e) => { const m = e.message || ''; if (m.startsWith('[probe]')) log(m); });
  win.webContents.on('render-process-gone', (e, d) => log('RENDERER GONE: ' + d.reason));
  win.webContents.on('unresponsive', () => log('Electron says: window unresponsive'));
  win.webContents.once('did-finish-load', async () => {
    await new Promise((r) => setTimeout(r, 5000));
    log('page loaded; starting steps');
    const step = async (name, code, wait = 2500) => {
      const t = Date.now();
      try { await run("window.__gap = 0; window.__t = performance.now(); if (!window.__iv) window.__iv = setInterval(() => { const n = performance.now(); window.__gap = Math.max(window.__gap, n - window.__t - 100); window.__t = n; }, 100); 1"); await run(code); await new Promise((r) => setTimeout(r, wait));
        log(`${name}: ok, page was blocked up to ${Math.round(await run('window.__gap'))} ms`); }
      catch (e) { log(`${name}: ${e.message}  <<< THIS STEP FREEZES THE WINDOW`); return false; }
      return true;
    };
    if (!(await step('Chats tab', "setTab('sessions')", 5000))) return app.exit(3);
    let list = [];
    try { list = await run("allSessions().slice(0, 12).map((s) => ({ id: s.id, title: s.title.slice(0, 40), mid: s.mid }))"); } catch (e) { log('listing sessions: ' + e.message); return app.exit(3); }
    log('sessions in the list: ' + list.length);
    for (let i = 0; i < list.length; i++) {
      if (!(await step(`open chat ${i} "${list[i].title}"`, `openSession(allSessions()[${i}])`, 3500))) return app.exit(3);
      try { log('   markdown time for this chat: ' + JSON.stringify(await run(`(() => { let worst = 0, len = 0, n = 0; for (const m of SS.msgs) { const t = performance.now(); try { md(m.text || ''); } catch (e) {} const d = performance.now() - t; n++; if (d > worst) { worst = d; len = (m.text || '').length; } } return { messages: n, slowest_ms: Math.round(worst), slowest_len: len }; })()`))); } catch (e) { log('   markdown check: ' + e.message); }
    }
    for (const t of ['dashboard', 'history', 'settings']) if (!(await step(t + ' tab', `setTab('${t}')`, 4000))) return app.exit(3);
    log('ALL STEPS OK: the page never froze in this run');
    app.exit(0);
  });
}

function createMain() {
  main = new BrowserWindow({ show: !process.argv.includes('--hidden'), width: 1120, height: 780, minWidth: 820, minHeight: 560, title: 'Hearth', icon: iconPath, backgroundColor: '#1F1E1D',
    autoHideMenuBar: true,
    webPreferences: { preload: path.join(__dirname, 'preload.js'), contextIsolation: true, autoplayPolicy: 'no-user-gesture-required' } });
  main.setMenuBarVisibility(false);
  main.loadFile(path.join(__dirname, 'renderer', 'index.html'));
  if (process.env.CM_PROBE) runProbe(main, process.env.CM_PROBE);
  main.webContents.on('did-finish-load', () => { if (upd) setTimeout(() => upd.check(true), 2500); });
  main.on('focus', () => { if (upd) upd.check(); });
  main.on('show', () => { if (upd) upd.check(); });
  main.on('close', (e) => {
    if (quitting) return;
    if (keepRunning()) { e.preventDefault(); main.hide(); }
  });
}

function buildTray() {
  try {
    tray = new Tray(nativeImage.createFromPath(path.join(__dirname, 'assets', 'tray.png')));
    tray.setToolTip('Hearth');
    const menu = () => Menu.buildFromTemplate([
      { label: 'Open Hearth', click: () => showMain() },
      { label: 'Ring me on this computer', type: 'checkbox', checked: S.settings.callMe, click: (i) => setCallMe(i.checked) },
      { type: 'separator' },
      { label: 'Quit', click: () => { quitting = true; app.quit(); } },
    ]);
    tray.setContextMenu(menu());
    tray.on('click', () => showMain());
  } catch { tray = null; }
}
function setCallMe(on) {
  S.settings.callMe = !!on; save();
  startEvents();  // messages keep arriving either way; this only controls ringing on this computer
  if (!on) for (const m of machines()) relayJson(m, 'POST', '/away', { on: false }).catch(() => {});
  broadcast('usage', publicState());
}

function publicState() {
  return { usage: S.usage, profile: S.profile, signedIn: true, history: S.history, settings: S.settings, error: S.error, now: Date.now() };
}
function broadcast(ch, data) { for (const w of BrowserWindow.getAllWindows()) if (!w.isDestroyed()) w.webContents.send(ch, data); }

// ───────────────────────── IPC ─────────────────────────
const h = (name, fn) => ipcMain.handle(name, async (e, ...a) => fn(e, ...a));
h('state', () => publicState());
h('refresh', async () => { await refreshUsage(true); return publicState(); });
h('settings:set', (e, patch) => {
  S.settings = { ...S.settings, ...patch }; save();
  if ('callMe' in patch) setCallMe(patch.callMe);
  if ('theme' in patch) { nativeTheme.themeSource = ['light', 'dark'].includes(patch.theme) ? patch.theme : 'system'; setTimeout(() => syncAutoTheme().catch(() => {}), 300); }
  broadcast('usage', publicState());
  return S.settings;
});
h('stats', async (e, id, days) => { const m = machineById(id); if (!m) throw new Error('Unknown computer'); return relayJson(m, 'GET', '/stats?days=' + (days || 30)); });
// ───────── phone pairing, call hooks, system colors ─────────
const keepRunning = () => S.settings.callMe || !!(relayInfo && relayInfo.ok && S.settings.background !== false);
const CLAUDE_SETTINGS = path.join(os.homedir(), '.claude', 'settings.json');
const hookCmd = () => `curl -s -m 40 -X POST -H "Content-Type: application/json" --data-binary @- http://127.0.0.1:${(relayInfo && relayInfo.port) || 47601}/hook`;
const HOOK_DEFS = [['Stop', '', 10], ['PreToolUse', 'AskUserQuestion', 10], ['PermissionRequest', '', 45]];
const isOurHook = (h) => /127\.0\.0\.1:\d+\/hook/.test(h.command || '') || /cm_hook\.py/.test(h.command || '');
function readClaudeSettings() { try { return JSON.parse(fs.readFileSync(CLAUDE_SETTINGS, 'utf8')); } catch { return {}; } }
function hooksStatus() {
  const hooks = readClaudeSettings().hooks || {};
  return HOOK_DEFS.every(([ev]) => (hooks[ev] || []).some((g) => (g.hooks || []).some(isOurHook)));
}
function hooksInstall() {
  const data = readClaudeSettings();
  try { if (fs.existsSync(CLAUDE_SETTINGS)) fs.copyFileSync(CLAUDE_SETTINGS, CLAUDE_SETTINGS + '.bak-hearth'); } catch { /* best effort */ }
  data.hooks = data.hooks || {};
  for (const [ev, matcher, timeout] of HOOK_DEFS) {
    const groups = (data.hooks[ev] = data.hooks[ev] || []);
    if (!groups.some((g) => (g.hooks || []).some(isOurHook))) groups.push({ matcher, hooks: [{ type: 'command', command: hookCmd(), timeout }] });
  }
  fs.mkdirSync(path.dirname(CLAUDE_SETTINGS), { recursive: true });
  fs.writeFileSync(CLAUDE_SETTINGS, JSON.stringify(data, null, 2) + '\n');
  return true;
}
function hooksRemove() {
  const data = readClaudeSettings();
  for (const ev of Object.keys(data.hooks || {})) {
    data.hooks[ev] = data.hooks[ev].map((g) => ({ ...g, hooks: (g.hooks || []).filter((x) => !isOurHook(x)) })).filter((g) => g.hooks.length);
    if (!data.hooks[ev].length) delete data.hooks[ev];
  }
  if (data.hooks && !Object.keys(data.hooks).length) delete data.hooks;
  fs.writeFileSync(CLAUDE_SETTINGS, JSON.stringify(data, null, 2) + '\n');
  return true;
}
const GNOME_ACCENT = { blue: '#3584e4', teal: '#2190a4', green: '#3a944a', yellow: '#c88800', orange: '#ed5b00', red: '#e62d42', pink: '#d56199', purple: '#9141ac', slate: '#6f8396' };
const run = (cmd, args) => new Promise((resolve) => execFile(cmd, args, { timeout: 1500 }, (err, out) => resolve(err ? '' : String(out))));
let accentCache = { at: 0, v: null };
const hex2 = (n) => Math.max(0, Math.min(255, Math.round(n))).toString(16).padStart(2, '0');
const GNOME_DESKTOP = () => /gnome|unity|budgie|cinnamon|pantheon|ubuntu/i.test(process.env.XDG_CURRENT_DESKTOP || '');
/** The accent color of this desktop. Windows and macOS say it directly; on Linux every desktop does it differently, so we ask the places in turn. */
async function systemAccent() {
  if (Date.now() - accentCache.at < 15000) return accentCache.v;
  let v = null;
  if (process.platform === 'win32' || process.platform === 'darwin') {
    try { const c = require('electron').systemPreferences.getAccentColor(); if (c) v = '#' + c.slice(0, 6); } catch { /* none */ }
  } else {
    // 1. the standard desktop portal (GNOME 47+, KDE 6, most others)
    const o = await run('gdbus', ['call', '--session', '--dest', 'org.freedesktop.portal.Desktop', '--object-path', '/org/freedesktop/portal/desktop', '--method', 'org.freedesktop.portal.Settings.Read', 'org.freedesktop.appearance', 'accent-color']);
    const pm = /\(\s*(-?[\d.e-]+),\s*(-?[\d.e-]+),\s*(-?[\d.e-]+)\s*\)/.exec(o);
    if (pm && Number(pm[1]) >= 0) v = '#' + [pm[1], pm[2], pm[3]].map((x) => hex2(Math.min(1, Number(x)) * 255)).join('');
    // 2. KDE Plasma
    if (!v) { try { const t = fs.readFileSync(path.join(os.homedir(), '.config', 'kdeglobals'), 'utf8'), km = /^AccentColor=(\d+),(\d+),(\d+)/m.exec(t); if (km) v = '#' + [km[1], km[2], km[3]].map((x) => hex2(Number(x))).join(''); } catch { /* not KDE */ } }
    // 3. Hyprland: the color of the focused window border is the closest thing it has
    if (!v && process.env.HYPRLAND_INSTANCE_SIGNATURE) { const hj = await run('hyprctl', ['getoption', 'general:col.active_border', '-j']); const hm = /"gradient":\s*"([0-9a-f]{8})/i.exec(hj); if (hm) v = '#' + hm[1].slice(2); }
    // 4. GNOME's own setting (only on GNOME-like desktops: elsewhere it always says "blue")
    if (!v && GNOME_DESKTOP()) { const g = (await run('gsettings', ['get', 'org.gnome.desktop.interface', 'accent-color'])).replace(/['\s]/g, ''); v = GNOME_ACCENT[g] || null; }
    // 5. the GTK theme's own selection / accent color
    if (!v) {
      for (const f of ['gtk-4.0/gtk.css', 'gtk-3.0/colors.css', 'gtk-3.0/gtk.css', 'gtk-4.0/colors.css']) {
        try { const t = fs.readFileSync(path.join(os.homedir(), '.config', f), 'utf8'); const gm = /@define-color\s+(?:accent_bg_color|accent_color|theme_selected_bg_color)\s+(#[0-9a-fA-F]{6})/.exec(t); if (gm) { v = gm[1].toLowerCase(); break; } } catch { /* next */ }
      }
    }
  }
  accentCache = { at: Date.now(), v };
  return v;
}
/** Dark or light, for desktops that do not tell Electron (a window manager without a settings daemon). null = unknown. */
async function systemDark() {
  if (process.platform !== 'linux') return null;
  const o = await run('gdbus', ['call', '--session', '--dest', 'org.freedesktop.portal.Desktop', '--object-path', '/org/freedesktop/portal/desktop', '--method', 'org.freedesktop.portal.Settings.Read', 'org.freedesktop.appearance', 'color-scheme']);
  const pm = /uint32 (\d)/.exec(o); if (pm) return pm[1] === '1' ? true : pm[1] === '2' ? false : null;
  const cs = (await run('gsettings', ['get', 'org.gnome.desktop.interface', 'color-scheme'])).replace(/['\s]/g, '');
  if (cs === 'prefer-dark') return true; if (cs === 'prefer-light') return false;
  const th = (await run('gsettings', ['get', 'org.gnome.desktop.interface', 'gtk-theme'])).toLowerCase();
  if (th) { if (/dark|black|night/.test(th)) return true; if (/light|white/.test(th)) return false; }
  try { const t = fs.readFileSync(path.join(os.homedir(), '.config', 'gtk-3.0', 'settings.ini'), 'utf8'); const dm = /gtk-application-prefer-dark-theme\s*=\s*(\w+)/i.exec(t); if (dm) return /1|true/i.test(dm[1]); const tm = /gtk-theme-name\s*=\s*(.+)/i.exec(t); if (tm) return /dark|black|night/i.test(tm[1]); } catch { /* none */ }
  return null;
}
/** "Auto" should follow the desktop even where the desktop does not tell Electron. */
async function syncAutoTheme() {
  if (S.settings.theme !== 'system' && S.settings.theme !== undefined) return;
  const dark = await systemDark(); if (dark === null) return;
  const want = dark ? 'dark' : 'light';
  if (nativeTheme.themeSource !== want) nativeTheme.themeSource = want;
}
function setAutostart(on) {
  if (process.platform === 'linux') {
    const f = path.join(os.homedir(), '.config', 'autostart', 'hearth.desktop');
    if (!on) { try { fs.unlinkSync(f); } catch { /* none */ } return; }
    fs.mkdirSync(path.dirname(f), { recursive: true });
    fs.writeFileSync(f, `[Desktop Entry]\nType=Application\nName=Hearth\nExec=${process.env.APPIMAGE || process.execPath} --hidden\nIcon=${iconPath}\nTerminal=false\n`);
  } else app.setLoginItemSettings({ openAtLogin: !!on, path: process.env.PORTABLE_EXECUTABLE_FILE || process.execPath, args: ['--hidden'] });
}
h('pair:info', () => ({
  embedded: !!(relayInfo && relayInfo.ok), external: !!(relayInfo && relayInfo.inUse), error: relayInfo && !relayInfo.ok && !relayInfo.inUse ? relayInfo.error : null,
  code: relayInfo && relayInfo.ok ? relayInfo.pairCode() : null, port: relayInfo && relayInfo.ok ? relayInfo.port : 47601,
  addresses: relayInfo && relayInfo.ok ? relayInfo.addresses() : [], tailscale: ((relayInfo && relayInfo.ok ? relayInfo.addresses() : []).find((a) => /^100\.(6[4-9]|[7-9]\d|1[01]\d|12[0-7])\./.test(a))) || null, allowRemote: !!S.settings.allowRemote, name: os.hostname(), claude: !!relayMod.findClaude(), hooks: hooksStatus(), platform: process.platform,
}));
h('pair:newcode', () => (relayInfo && relayInfo.ok ? relayInfo.newPairCode() : null));
h('hooks:install', () => hooksInstall());
h('hooks:remove', () => hooksRemove());
h('system:accent', () => systemAccent());
h('system:dark', () => systemDark());
h('setup:paths', () => { const projects = path.join(os.homedir(), 'projects'); try { fs.mkdirSync(projects, { recursive: true }); } catch { /* shown by the setup */ } return { home: os.homedir(), projects, platform: process.platform }; });
// the better local voices (Linux): runs the installer script and reports each line while it works
let voiceJob = null;
h('voice:install', async (e) => {
  if (process.platform !== 'linux') throw new Error('The extra voices are for Linux. On this system Hearth uses the voices of your computer.');
  if (voiceJob) return { running: true };
  const send = (d) => { if (!e.sender.isDestroyed()) e.sender.send('voice-install', d); };
  let script = path.join(__dirname, '..', 'relay', 'install_voice.sh');
  if (!fs.existsSync(script)) {
    send({ line: 'Downloading the voice installer…' });
    script = path.join(os.tmpdir(), 'hearth-install-voice.sh');
    try { const r = await fetch('https://raw.githubusercontent.com/veelvoer/hearth/main/relay/install_voice.sh', { signal: AbortSignal.timeout(30000) }); if (!r.ok) throw new Error(); fs.writeFileSync(script, await r.text()); } catch { throw new Error('Could not download the voice installer. Check your internet connection.'); }
  }
  voiceJob = spawn('bash', [script], { stdio: ['ignore', 'pipe', 'pipe'] });
  const feed = (b) => String(b).split(/\r?\n|\r/).map((l) => l.trim()).filter(Boolean).forEach((l) => send({ line: l.slice(0, 160) }));
  voiceJob.stdout.on('data', feed); voiceJob.stderr.on('data', feed);
  voiceJob.on('close', (code) => { voiceJob = null; send({ done: true, ok: code === 0 }); });
  return { running: true };
});
h('app:icon', (e, dataUrl) => { try { const img = nativeImage.createFromDataURL(String(dataUrl)); if (img.isEmpty()) return false; if (main && !main.isDestroyed()) main.setIcon(img); if (tray) tray.setImage(img.resize({ width: 32, height: 32 })); return true; } catch { return false; } });
h('autostart:set', (e, on) => { setAutostart(on); S.settings.autostart = !!on; save(); return true; });
h('machines', () => machines().map(pub));
h('machines:add', async (e, m) => {
  let name = m.name, host = String(m.host || '').trim(), token = String(m.token || '').trim(), port = Number(m.port) || 0, secure = false;
  if (/^(hearth|hearth):\/\//.test(host)) {  // pairing link
    const u = new URL(host);
    token = u.searchParams.get('token') || token; name = name || u.searchParams.get('name'); host = u.searchParams.get('url') || '';
  }
  if (/^https?:\/\//.test(host)) { const u = new URL(host); secure = u.protocol === 'https:'; port = u.port ? Number(u.port) : (secure ? 443 : 80); host = u.hostname; }
  else if (host.includes(':')) { port = Number(host.split(':')[1]) || port; host = host.split(':')[0]; }
  if (!host || !token) throw new Error('Type the server address and the 6-digit code.');
  if (!port) port = 47601;
  if (/^\d{6}$/.test(token.replace(/\s/g, ''))) {   // the short code from the server's installer: trade it for the real key
    const code = token.replace(/\s/g, ''), base = `${secure ? 'https' : 'http'}://${host}:${port}`;
    let r; try { r = await fetch(base + '/pair', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ code }), signal: AbortSignal.timeout(15000) }); } catch { throw new Error('I could not reach ' + host + '. Check the address and your internet.'); }
    const j = await r.json().catch(() => ({}));
    if (!r.ok || !j.token) throw new Error(r.status === 403 ? 'That code is wrong or ran out. Ask the server for a new one (hearth-server code).' : (j.error || 'The server did not accept the code.'));
    token = j.token; name = name || j.name || '';
  }
  S.machines = S.machines.filter((x) => x.host !== host || x.port !== port);
  S.machines.push({ id: crypto.randomBytes(4).toString('hex'), name: name || host, host, port, token, secure });
  save(); startEvents(); sendLink();
  return machines().map(pub);
});
h('machines:remove', (e, id) => { S.machines = S.machines.filter((m) => m.id !== id); save(); sendLink(); return machines().map(pub); });
h('discover', async () => (await discover()).filter((f) => !machines().some((m) => m.host === f.host)));
// projects that appeared on the server while this computer was off: ask before copying them here
let pendSeen = new Set(), lastPend = [];
async function checkPending() {
  const m = machines().find((x) => x.local); if (!m) return;
  const st = await relayJson(m, 'GET', '/sync/status').catch(() => null); if (!st) return;
  lastPend = (st.pending || []).map((p) => ({ ...p, path: path.join(os.homedir(), 'projects', p.name) }));
  if (main && !main.isDestroyed()) main.webContents.send('pending-projects', lastPend);
  const fresh = lastPend.filter((p) => !pendSeen.has(p.name)); fresh.forEach((p) => pendSeen.add(p.name));
  if (fresh.length) notify(fresh.length === 1 ? 'New project: ' + fresh[0].name : fresh.length + ' new projects', 'Made on your server while this computer was off. Open Hearth to install ' + (fresh.length === 1 ? 'it' : 'them') + '.', () => showMain());
}
// a phone on your Wi-Fi asks to connect: tell the person, let them accept or decline
let pairSeen = new Set(), lastPairReq = [];
async function checkPairRequests() {
  const m = machines().find((x) => x.local); if (!m) return;
  const list = await relayJson(m, 'GET', '/pair/requests').catch(() => null); if (!list) return;
  lastPairReq = list;
  if (main && !main.isDestroyed()) main.webContents.send('pair-requests', list);
  for (const r of list.filter((x) => !pairSeen.has(x.id))) {
    pairSeen.add(r.id);
    if (S.settings.pairNotify !== false) { notify(r.name + ' wants to connect', 'A phone on your Wi-Fi is asking to use Hearth with this computer. Open Hearth to accept or decline.', () => showMain()); if (main && !main.isDestroyed() && !main.isVisible()) showMain(); }
  }
}
h('pair:requests', async () => { await checkPairRequests().catch(() => {}); return lastPairReq; });
h('pair:decide', async (e, id, accept) => {
  const m = machines().find((x) => x.local); if (!m) throw new Error('This computer is not running Hearth yet.');
  await relayJson(m, 'POST', '/pair/decision', { id, accept: !!accept }); await checkPairRequests().catch(() => {}); return lastPairReq;
});
// ── support: sign in with an emailed code, send requests, read answers (the desk lives on the project's server) ──
const SUPPORT_URL = process.env.HEARTH_SUPPORT_URL || 'https://cm.vpswb.store/support';
async function supportCall(method, p, body, auth = true) {
  const headers = { 'Content-Type': 'application/json' }; if (auth && S.settings.supportToken) headers.Authorization = 'Bearer ' + S.settings.supportToken;
  let r; try { r = await fetch(SUPPORT_URL + p, { method, headers, body: body ? JSON.stringify(body) : undefined, signal: AbortSignal.timeout(20000) }); } catch { throw new Error('Could not reach support. Check your internet connection and try again.'); }
  const j = await r.json().catch(() => ({}));
  if (r.status === 401 && auth) { S.settings.supportToken = ''; S.settings.supportEmail = ''; save(); }
  if (!r.ok) throw new Error(j.error || 'Support said ' + r.status);
  return j;
}
h('support:state', () => ({ email: S.settings.supportToken ? S.settings.supportEmail || '' : '', signedIn: !!S.settings.supportToken, meta: { app: 'Desktop', version: app.getVersion(), platform: `${os.type()} ${os.release()}` } }));
h('support:start', (e, email) => supportCall('POST', '/auth/start', { email }, false));
h('support:verify', async (e, email, code) => { const j = await supportCall('POST', '/auth/verify', { email, code }, false); S.settings.supportToken = j.token; S.settings.supportEmail = j.email; save(); return { email: j.email }; });
h('support:logout', async () => { try { await supportCall('POST', '/auth/logout', {}); } catch { /* signed out anyway */ } S.settings.supportToken = ''; S.settings.supportEmail = ''; save(); return true; });
h('support:call', (e, method, p, body) => supportCall(method, p, body));
h('app:relaunch', () => { quitting = true; app.relaunch(); app.quit(); return true; });
h('projects:pending', async () => { await checkPending().catch(() => {}); return lastPend; });
h('projects:accept', async (e, names) => {
  const m = machines().find((x) => x.local); if (!m) throw new Error('This computer is not running Hearth yet.');
  await relayJson(m, 'POST', '/sync/projects/accept', names === 'all' ? { all: true } : { names });
  await checkPending().catch(() => {}); return lastPend;
});
h('relay', async (e, id, method, p, body) => {
  const m = machineById(id);
  if (!m) throw new Error('Unknown computer');
  return relayJson(m, method, p, body);
});
h('project:move', async (e, localId, serverId, cwd, name) => {
  const local = machineById(localId), server = machineById(serverId);
  if (!local || !server) throw new Error('Unknown computer');
  const say = (text) => { if (!e.sender.isDestroyed()) e.sender.send('move-progress', { text }); };
  const enc = encodeURIComponent, sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  say('Moving the project into your projects folder…');
  const mv = await relayJson(local, 'POST', '/projects/move', { cwd, name });
  const newPath = mv.path;
  const info = await relayJson(server, 'GET', '/projects');
  if (!info.root) throw new Error('That server has no projects folder.');
  const serverPath = info.root.replace(/\/$/, '') + '/' + path.basename(newPath);
  const deadline = Date.now() + 25 * 60000;
  for (;;) {  // wait until the files are on the server
    const [ls, ss, pl] = await Promise.all([relayJson(local, 'GET', '/sync').catch(() => ({})), relayJson(server, 'GET', '/sync').catch(() => ({})), relayJson(server, 'GET', '/projects').catch(() => ({ projects: [] }))]);
    const there = (pl.projects || []).some((p) => p.path === serverPath);
    const idle = (!ls.available || ls.state === 'idle') && (!ss.available || (ss.state === 'idle' && !ss.needFiles));
    if (there && idle) break;
    if (Date.now() > deadline) throw new Error('Syncing is taking long. The project is moved and will show up on the server when syncing finishes.');
    const left = (ss.needFiles || 0) + (ls.needFiles || 0);
    say('Syncing the files to the server…' + (left ? ` ${left} files left` : ''));
    await sleep(3000);
  }
  say('Copying your chats to the server…');
  const res = await relayRequest(local, 'GET', '/projects/export?cwd=' + enc(newPath), null, { timeout: 120000 });
  const buf = await readAll(res);
  if (res.statusCode !== 200) throw new Error('Could not read the chats of this project.');
  const imp = await relayRequest(server, 'POST', `/projects/import?cwd=${enc(serverPath)}&from=${enc(newPath)}`, buf, { type: 'application/gzip', timeout: 10 * 60000 });
  const ib = await readAll(imp);
  if (imp.statusCode !== 200) throw new Error((safeJson(ib) || {}).error || 'Could not copy the chats.');
  const list = await relayJson(server, 'GET', '/sessions');
  const mine = list.filter((s) => s.cwd === serverPath).sort((a, b) => b.mtime - a.mtime);
  return { cwd: serverPath, newPath, session: mine[0] || null };
});
h('project:continue', async (e, serverId, localId, serverCwd) => {
  const local = machineById(localId), server = machineById(serverId);
  if (!local || !server) throw new Error('Unknown computer');
  const say = (text) => { if (!e.sender.isDestroyed()) e.sender.send('move-progress', { text }); };
  const enc = encodeURIComponent, sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const st = await relayJson(local, 'GET', '/status');
  if (!st.moveRoot) throw new Error('This computer has no projects folder set up.');
  const localCwd = st.moveRoot.replace(/\/$/, '') + '/' + path.basename(serverCwd);
  say('Checking that the files are on this computer…');
  const deadline = Date.now() + 20 * 60000;
  for (;;) {
    const [ex, ss, ls] = await Promise.all([relayJson(local, 'GET', '/projects/exists?cwd=' + enc(localCwd)), relayJson(server, 'GET', '/sync').catch(() => ({})), relayJson(local, 'GET', '/sync').catch(() => ({}))]);
    if (ex.exists && (!ss.available || ss.state === 'idle') && (!ls.available || ls.state === 'idle')) break;
    if (Date.now() > deadline) throw new Error('The files are still syncing to this computer. Try again in a moment.');
    say('Waiting for the project files to sync…'); await sleep(3000);
  }
  say('Copying the chat to this computer…');
  const res = await relayRequest(server, 'GET', '/projects/export?cwd=' + enc(serverCwd), null, { timeout: 120000 });
  const buf = await readAll(res);
  if (res.statusCode !== 200) throw new Error('Could not read the chats of this project on the server.');
  const imp = await relayRequest(local, 'POST', `/projects/import?cwd=${enc(localCwd)}&from=${enc(serverCwd)}`, buf, { type: 'application/gzip', timeout: 10 * 60000 });
  const ib = await readAll(imp);
  if (imp.statusCode !== 200) throw new Error((safeJson(ib) || {}).error || 'Could not copy the chat.');
  const mine = (await relayJson(local, 'GET', '/sessions')).filter((s) => s.cwd === localCwd).sort((a, b) => b.mtime - a.mtime);
  return { cwd: localCwd, session: mine[0] || null };
});
h('update:run', (e, want) => { if (upd) upd.run(want || {}); return true; });
h('update:check', async () => { if (!upd) return null; const det = await upd.details(); upd.check(true); return det; });
h('update:later', () => { if (upd) upd.later(); return true; });
h('update:status', () => (upd ? upd.status() : null));
h('hub:status', async () => {
  const ms = machines(), server = ms.find((m) => m.secure), local = ms.find((m) => m.local);
  const get = (m, p) => (m ? relayJson(m, 'GET', p).catch(() => null) : Promise.resolve(null));
  const [ss, sync, lsync] = await Promise.all([get(server, '/status'), get(server, '/sync'), get(local, '/sync')]);
  return { server: server ? { name: server.name, host: server.host, online: !!ss, laptop: ss ? ss.laptop : null, listeners: ss ? ss.listeners : 0, queued: ss ? ss.queued : 0, sync } : null, local: local ? { name: local.name, sync: lsync } : null, me: os.hostname() };
});
h('hub:phonecode', async () => {
  const server = machines().find((m) => m.secure);
  if (!server) throw new Error('Connect your server first.');
  const r = await relayJson(server, 'POST', '/pair/new', {});
  return { code: r.code, address: server.host, expiresIn: r.expiresIn };
});
h('upload', async (e, id, session, cwd, name, buf) => {
  const m = machineById(id);
  if (!m) throw new Error('Unknown computer');
  const q = session ? 'session=' + encodeURIComponent(session) : 'cwd=' + encodeURIComponent(cwd);
  const res = await relayRequest(m, 'POST', `/upload?${q}&name=${encodeURIComponent(name)}`, Buffer.from(buf), { type: 'application/octet-stream', timeout: 120000 });
  const b = await readAll(res);
  if (res.statusCode !== 200) throw new Error((safeJson(b) || {}).error || 'Upload failed');
  return (safeJson(b) || {}).path;
});
h('openPath', (e, p) => { if (p) shell.openPath(String(p)); return true; });
h('pickFolder', async () => { const r = await dialog.showOpenDialog(main, { properties: ['openDirectory'] }); return r.canceled ? null : r.filePaths[0]; });
h('stop', async (e, id, session) => { const m = machineById(id); return m ? relayJson(m, 'POST', '/stop', { session }) : false; });
h('openExternal', (e, url) => { if (/^https?:\/\//.test(String(url))) shell.openExternal(url); });
h('send', (e, id, session, text, mode, voice, sid0, extra) => {
  const m = machineById(id);
  if (!m) throw new Error('Unknown computer');
  const sid = sid0 || crypto.randomBytes(4).toString('hex');
  const out = (o) => { if (!e.sender.isDestroyed()) e.sender.send('stream', { sid, ...o }); };
  relayRequest(m, 'POST', `/sessions/${session || 'new'}/send`, { text, mode, brief: !!voice, ...(extra || {}) }, { timeout: 60 * 60000 }).then((res) => {
    if (res.statusCode !== 200) { readAll(res).then((b) => out({ t: 'fail', text: (safeJson(b) || {}).error || 'Relay error ' + res.statusCode })); return; }
    let buf = '';
    res.setEncoding('utf8');
    res.on('data', (d) => {
      buf += d;
      let i;
      while ((i = buf.indexOf('\n')) >= 0) { const line = buf.slice(0, i); buf = buf.slice(i + 1); const o = safeJson(line); if (o) out(o); }
    });
    res.on('end', () => out({ t: 'end' }));
    res.on('error', (err) => out({ t: 'fail', text: err.message }));
  }).catch((err) => out({ t: 'fail', text: err.message }));
  return sid;
});
h('voice:status', async (e, id) => { const m = machineById(id); return m ? relayJson(m, 'GET', '/voice/status') : { stt: false, tts: false }; });
h('voice:stt', async (e, id, wav) => {
  const m = machineById(id);
  const res = await relayRequest(m, 'POST', '/voice/stt', Buffer.from(wav), { type: 'audio/wav', timeout: 90000 });
  const b = await readAll(res);
  if (res.statusCode !== 200) throw new Error((safeJson(b) || {}).error || 'Speech recognition failed');
  return (safeJson(b) || {}).text || '';
});
h('voice:tts', async (e, id, text, lang) => {
  const m = machineById(id);
  const res = await relayRequest(m, 'POST', '/voice/tts', { text, lang }, { timeout: 60000 });
  const b = await readAll(res);
  if (res.statusCode !== 200) throw new Error((safeJson(b) || {}).error || 'Speech synthesis failed');
  return b;
});
h('call:action', async (e, key, action) => {
  const c = callWins.get(key);
  if (!c) return false;
  const { w, m, ev } = c;
  if ((action === 'allow' || action === 'deny') && ev.req) await relayJson(m, 'POST', '/decision', { req: ev.req, behavior: action }).catch(() => {});
  if (action === 'talk' || action === 'chat') {
    showMain('sessions');
    main.webContents.send('open-session', { machine: m.id, session: ev.session, title: ev.title, cwd: ev.cwd, voice: action === 'talk' });
  }
  if (!w.isDestroyed()) w.close();
  return true;
});

// ───────────────────────── lifecycle ─────────────────────────
app.on('second-instance', () => showMain());
let upd = null;
const anyRunning = async (m) => { if (!m) return false; try { return (await relayJson(m, 'GET', '/sessions')).some((s) => s.running); } catch { return false; } };
app.whenReady().then(async () => {
  if (process.env.CM_HEARTBEAT) {  // diagnostics: how long is the main thread ever blocked?
    let last = Date.now(), worst = 0;
    setInterval(() => { const n = Date.now(); worst = Math.max(worst, n - last - 200); last = n; fs.writeFileSync(process.env.CM_HEARTBEAT, `alive ${n} worst_block_ms ${worst}\n`); }, 200);
  }
  if (!process.env.CM_NO_THEME) { nativeTheme.themeSource = ['light', 'dark'].includes(S.settings.theme) ? S.settings.theme : 'system'; syncAutoTheme().catch(() => {}); setInterval(() => syncAutoTheme().catch(() => {}), 30000); }
  relayInfo = process.env.CM_NO_RELAY ? { ok: false, inUse: true } : await startRelay();
  sendLink();
  upd = updater.create({
    root: path.join(__dirname, '..'), desktopDir: __dirname, getState: () => S, save, send: (ch, data) => broadcast(ch, data),
    serverBusy: () => anyRunning(machines().find((m) => m.secure)), localBusy: () => anyRunning(machines().find((m) => m.local)),
    relaunch: (execPath) => { quitting = true; if (execPath) app.relaunch({ execPath, args: process.argv.slice(1).filter((a) => a === '--hidden') }); else app.relaunch(); app.quit(); },
    relayBuild: async (which) => { const m = which === 'server' ? machines().find((x) => x.secure) : machines().find((x) => x.local); if (!m) return undefined; try { const st = await relayJson(m, 'GET', '/status'); return typeof st.build === 'string' ? st.build : ''; } catch { return null; } },
    version: () => app.getVersion(),
    relayStatus: async (which) => { const m = which === 'server' ? machines().find((x) => x.secure) : machines().find((x) => x.local); if (!m) return undefined; try { return await relayJson(m, 'GET', '/status'); } catch { return null; } },
    relayPost: async (which, p, body) => { const m = which === 'server' ? machines().find((x) => x.secure) : machines().find((x) => x.local); if (!m) throw new Error('No server connected'); return relayJson(m, 'POST', p, body); },
    openPath: (f) => shell.openPath(f), openExternal: (u) => shell.openExternal(u),
    notify: (t, b) => notify(t, b), windowHidden: () => !main || main.isDestroyed() || !main.isVisible(),
  });
  setTimeout(() => upd.check(), 15000); setInterval(() => upd.check(), 120000);
  setInterval(() => checkPairRequests().catch(() => {}), 2500);
  setTimeout(() => checkPending().catch(() => {}), 8000); setInterval(() => checkPending().catch(() => {}), 20000);
  createMain();
  if (!process.env.CM_NO_TRAY) buildTray();
  startPolling();
  startEvents();
});
app.on('before-quit', () => { quitting = true; if (relayInfo && relayInfo.ok) relayInfo.stop(); });
app.on('window-all-closed', () => { if (quitting || !keepRunning()) app.quit(); });
