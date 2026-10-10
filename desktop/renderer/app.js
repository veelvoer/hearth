'use strict';
const $ = (s, e = document) => e.querySelector(s);
function h(tag, props, ...kids) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(props || {})) {
    if (k === 'class') e.className = v;
    else if (k === 'style') e.style.cssText = v;
    else if (k === 'html') e.innerHTML = v;
    else if (k.startsWith('on')) e[k] = v;
    else if (v !== false && v != null) e.setAttribute(k, v === true ? '' : v);
  }
  for (const k of kids.flat(Infinity)) { if (k == null || k === false) continue; e.append(k.nodeType ? k : document.createTextNode(k)); }
  return e;
}
/** Popup cards (updates, new projects, phones asking to connect) stack in one corner instead of covering each other. */
function cardHost() { let c = document.getElementById('cardhost'); if (!c) { c = h('div', { id: 'cardhost' }); document.body.append(c); } return c; }
const clean = (e) => String((e && e.message) || e).replace(/^Error invoking remote method '[^']+': (Error: )?/, '');
const css = (v) => getComputedStyle(document.documentElement).getPropertyValue(v).trim();
/** Icon-only button with a hover label. */
const ibtn = (name, title, fn, cls = 'ghost') => h('button', { class: cls + ' ib', title, 'aria-label': title, onclick: fn }, icon(name));
/** The little (i): explains a setting in a popover instead of a paragraph of text. */
function info(text) {
  const b = h('button', { class: 'infob', title: 'More info', 'aria-label': 'More info', onclick: (e) => {
    e.stopPropagation(); const open = document.querySelector('.infopop'); if (open) { const same = open.dataset.for === text; open.remove(); if (same) return; }
    const r = b.getBoundingClientRect(), pop = h('div', { class: 'infopop', 'data-for': text }, text); document.body.append(pop);
    const w = Math.min(300, innerWidth - 24); pop.style.width = w + 'px'; pop.style.left = Math.max(12, Math.min(innerWidth - w - 12, r.left - w / 2 + 9)) + 'px';
    const below = r.bottom + pop.offsetHeight + 12 < innerHeight; pop.style.top = (below ? r.bottom + 8 : r.top - pop.offsetHeight - 8) + 'px';
    setTimeout(() => document.addEventListener('click', function close() { pop.remove(); document.removeEventListener('click', close); }, { once: true }), 0);
  } }, icon('info'));
  return b;
}
const chip = (label, on, fn) => h('button', { class: 'chip' + (on ? ' on' : ''), onclick: fn }, label);
const toggle = (on, fn) => h('button', { class: 'toggle' + (on ? ' on' : ''), role: 'switch', 'aria-checked': String(!!on), onclick: fn });

let S = { signedIn: false, settings: {}, history: [] };
let tab = 'sessions';
const content = $('#content');
const ICONS = {
  dashboard: '<path d="M3 3h7v9H3zM14 3h7v5h-7zM14 12h7v9h-7zM3 16h7v5H3z"/>',
  now: '<path d="M12 21a9 9 0 1 1 9-9"/><path d="M12 12l4-4"/>',
  sessions: '<path d="M4 5h16v11H9l-5 4z"/>',
  history: '<path d="M4 20V10M10 20V4M16 20v-8M22 20H2"/>',
  settings: '<circle cx="12" cy="12" r="3"/><path d="M12 2v3M12 19v3M2 12h3M19 12h3M5 5l2 2M17 17l2 2M19 5l-2 2M7 17l-2 2"/>',
  refresh: '<path d="M20 12a8 8 0 1 1-2.6-5.9"/><path d="M20 4v5h-5"/>', mic: '<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5.5 11.5a6.5 6.5 0 0 0 13 0M12 18v3"/>',
  addons: '<rect x="3.5" y="3.5" width="7" height="7" rx="1.8"/><rect x="13.5" y="3.5" width="7" height="7" rx="1.8"/><rect x="3.5" y="13.5" width="7" height="7" rx="1.8"/><path d="M17 13.5v7M13.5 17h7"/>', sliders: '<path d="M4 7h9M17 7h3M4 17h3M11 17h9"/><circle cx="15" cy="7" r="2"/><circle cx="9" cy="17" r="2"/>',
  close: '<path d="M6.5 6.5l11 11M17.5 6.5l-11 11"/>', back: '<path d="M14.5 5.5L8 12l6.5 6.5"/>', plus: '<path d="M12 5v14M5 12h14"/>', check: '<path d="M5 12.5l4.5 4.5L19 7.5"/>', info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v5.5"/><circle cx="12" cy="7.8" r=".6" fill="currentColor"/>',
  devices: '<rect x="3" y="5" width="13" height="9" rx="1.8"/><path d="M1.5 18.5h16"/><rect x="18" y="8.5" width="3.5" height="8" rx="1"/>', sync: '<path d="M4.5 10A8 8 0 0 1 18 6.5L20 8.5"/><path d="M20 4v4.5h-4.5"/><path d="M19.5 14A8 8 0 0 1 6 17.5L4 15.5"/><path d="M4 20v-4.5h4.5"/>',
  support: '<circle cx="12" cy="12" r="9"/><path d="M9.6 9.6a2.6 2.6 0 1 1 3.7 2.4c-.8.4-1.3 1-1.3 2"/><circle cx="12" cy="16.8" r=".6" fill="currentColor"/>', paint: '<path d="M12 3.5a8.5 8.5 0 1 0 0 17c1.4 0 2-.9 1.6-1.9-.4-1 .2-2.1 1.4-2.1h2a3.5 3.5 0 0 0 3.5-3.5C20.5 7.5 16.7 3.5 12 3.5z"/><circle cx="7.8" cy="11" r=".9" fill="currentColor"/><circle cx="11" cy="7.4" r=".9" fill="currentColor"/><circle cx="15.4" cy="8.6" r=".9" fill="currentColor"/>',
  bell: '<path d="M6 16.5V11a6 6 0 0 1 12 0v5.5l1.5 1.5h-15z"/><path d="M10 20.5h4"/>', phone: '<rect x="7" y="2.5" width="10" height="19" rx="2.4"/><path d="M10.5 18.5h3"/>', server: '<rect x="3.5" y="4" width="17" height="6.5" rx="1.8"/><rect x="3.5" y="13.5" width="17" height="6.5" rx="1.8"/><circle cx="7.5" cy="7.2" r=".7" fill="currentColor"/><circle cx="7.5" cy="16.8" r=".7" fill="currentColor"/>',
  folder: '<path d="M3.5 7a1.5 1.5 0 0 1 1.5-1.5h4l2 2.2h8A1.5 1.5 0 0 1 20.5 9.2V18a1.5 1.5 0 0 1-1.5 1.5H5A1.5 1.5 0 0 1 3.5 18z"/>', send: '<path d="M4 12l16-8-5.5 16-3-6.5z"/><path d="M11.5 13.5L20 4"/>', stop: '<rect x="6.5" y="6.5" width="11" height="11" rx="2.2"/>', git: '<circle cx="6.5" cy="6" r="2"/><circle cx="6.5" cy="18" r="2"/><circle cx="17.5" cy="9" r="2"/><path d="M6.5 8v8M8.4 6.6c5 0 9 .5 9.1 0"/>',
  tour: '<circle cx="12" cy="12" r="9"/><path d="M15.8 8.2l-2.1 5.5-5.5 2.1 2.1-5.5z"/>', tool: '<path d="M14.5 6.5a4 4 0 0 0 4.5 4.5l-8.5 8.5a2.1 2.1 0 0 1-3-3l8.5-8.5a4 4 0 0 0-1.5-1.5z"/>',
};
const icon = (n) => h('span', { class: 'ic', html: `<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">${ICONS[n]}</svg>` });
const TABS = [['sessions', 'Chats'], ['dashboard', 'Home'], ['history', 'History'], ['settings', 'Settings']];

function renderNav() {
  const nav = $('#nav');
  const collapsed = localStorage.getItem('cm.navCollapsed') === '1';
  nav.innerHTML = '';
  nav.className = collapsed ? 'collapsed' : '';
  nav.append(h('div', { class: 'brand-row' }, h('div', { class: 'brand', html: sparkSVG(24) }, h('span', { class: 'lbl' }, ' Hearth')),
    h('button', { class: 'navtoggle', title: collapsed ? 'Show menu names' : 'Shrink menu to icons', 'aria-label': collapsed ? 'Expand menu' : 'Collapse menu', onclick: () => { localStorage.setItem('cm.navCollapsed', collapsed ? '0' : '1'); renderNav(); } }, collapsed ? '›' : '‹')));
  if (!S.signedIn) return;
  for (const [id, label] of TABS) nav.append(h('button', { class: tab === id ? 'on' : '', title: label, onclick: () => setTab(id) }, icon(id), h('span', { class: 'lbl' }, label)));
}
/** Runs a change of screen as one morphing transition (shared elements glide, the rest cross-fades). */
function morph(fn) {
  if (!document.startViewTransition || matchMedia('(prefers-reduced-motion: reduce)').matches) return fn();
  try { return document.startViewTransition(() => { fn(); }); } catch { return fn(); }
}
function setTab(t) { if (t === tab) return; morph(() => { tab = t; render(); }); }
/** The first-run setup. Also tried again once every script has loaded: the state can arrive before onboarding.js has run. */
function maybeTutorial() { if (S && S.settings && !S.settings.onboarded && typeof startTutorial === 'function') startTutorial(); }
window.addEventListener('load', () => { if (S && S.settings) render(); });
let lastTab = null;
function render() {
  lastSig = sigOf();
  renderNav();
  content.className = (tab === 'sessions' && S.signedIn ? 'flush' : '') + (lastTab !== tab ? ' entering' : ''); lastTab = tab;   // only a new screen glides in, not every refresh
  content.innerHTML = '';
  maybeTutorial();
  if (!S.signedIn) return content.append(loginView());
  const v = { dashboard: dashboardView, now: nowView, sessions: sessionsView, history: historyView, settings: settingsView }[tab]();
  content.append(v);
  if (tab === 'history') requestAnimationFrame(drawHistory);
}
const page = (...k) => h('div', { class: 'page' }, ...k);
const head = (title, sub, action) => h('div', { class: 'head' }, h('div', { class: 'grow' }, h('h1', {}, title), sub ? h('div', { class: 'muted small' }, sub) : null), action);

// ───────────────────────── Now ─────────────────────────
const at = (ms, week) => { const d = new Date(ms); const t = d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }); return week ? d.toLocaleDateString([], { weekday: 'short' }) + ' ' + t : t; };
const ago = (t, now) => (now - t) < 60000 ? 'just now' : CMW.dur(now - t) + ' ago';

function limitCard(name, window, l, ms) {
  const now = Date.now();
  const c = h('div', { class: 'card' }, h('div', { class: 'row' }, h('div', { class: 'label grow' }, name), h('div', { class: 'label' }, window)));
  if (!l) { c.append(h('p', { class: 'muted' }, 'No data')); return c; }
  const p = CMW.usedAt(l, now), pc = CMW.pace(l, ms, now), r = l.resetsAt;
  c.append(h('div', { class: 'big', style: 'margin-top:6px' }, String(Math.floor(p)), h('small', {}, '%')),
    h('div', { class: 'meter' }, h('i', { style: `width:${Math.min(100, p)}%` }), pc ? h('b', { style: `left:${pc.elapsed * 100}%` }) : null));
  c.append(r && r > now ? h('div', { class: 'row' }, h('span', { class: 'grow' }, 'Resets in ' + CMW.dur(r - now)), h('span', { class: 'muted small' }, at(r, ms === CMW.WEEK_MS)))
    : h('div', { class: 'muted small' }, 'Fresh window. Starts on your next message.'));
  const w = CMW.paceWord(l, pc);
  if (w) {
    const detail = pc.hitsAt ? `At this rate you hit the limit in ${CMW.dur(pc.hitsAt - now)}.` : pc.projected > 0 ? `Heading for about ${Math.min(100, Math.floor(pc.projected))}% by reset.` : '';
    c.append(h('div', { class: 'small muted', style: 'margin-top:10px' }, `${w}. ${detail}`));
  }
  return c;
}
function nowView() {
  const u = S.usage, now = Date.now();
  const refresh = h('button', { class: 'link', onclick: async (e) => { e.target.textContent = 'Updating…'; S = await cm.refresh(); render(); } }, 'Refresh');
  const p = page(head('Now', S.profile ? [S.profile.plan, S.profile.email].filter(Boolean).join(' · ') : '', refresh));
  if (S.error) p.append(h('div', { class: 'card err small' }, S.error));
  p.append(limitCard('Session', '5-hour window', u && u.session, CMW.SESSION_MS), limitCard('Week', '7-day window', u && u.week, CMW.WEEK_MS));
  if (u && (u.opus || u.sonnet)) {
    const c = h('div', { class: 'card' }, h('div', { class: 'label' }, 'Weekly by model'));
    for (const [n, l] of [['Opus', u.opus], ['Sonnet', u.sonnet]]) if (l) {
      const v = CMW.usedAt(l, now);
      c.append(h('div', { class: 'row', style: 'margin-top:12px' }, h('span', { class: 'grow' }, n), h('span', { class: 'muted' }, Math.floor(v) + '%')), h('div', { class: 'meter thin' }, h('i', { style: `width:${v}%` })));
    }
    p.append(c);
  }
  if (u && u.extra) p.append(h('div', { class: 'card' }, h('div', { class: 'label' }, 'Extra usage'), h('h2', { style: 'margin:8px 0' }, '$' + u.extra.used.toFixed(2), h('span', { class: 'muted small' }, '  of $' + u.extra.limit.toFixed(2))), h('div', { class: 'meter thin' }, h('i', { style: `width:${u.extra.pct}%` }))));
  if (u) p.append(h('div', { class: 'small muted' }, `Updated ${ago(u.at, now)}. Same limits as /usage in Claude Code.`));
  return p;
}

// ───────────────────────── History ─────────────────────────
let histRange = 0;
const SPANS = [864e5, 7 * 864e5, 30 * 864e5];
function historyView() {
  const now = Date.now(), from = now - SPANS[histRange];
  const data = S.history.filter((s) => s.t >= from);
  const p = page(head('History', 'Sampled every time usage refreshes'));
  p.append(h('div', { class: 'row' }, ['24h', '7d', '30d'].map((l, i) => chip(l, histRange === i, () => { histRange = i; render(); }))));
  p.append(h('div', { class: 'card' }, h('div', { class: 'row small muted', style: 'gap:16px;margin-bottom:8px' },
    h('span', {}, h('span', { class: 'dot' }), 'Session'), h('span', {}, h('span', { class: 'dot', style: 'background:var(--ink)' }), 'Week')),
    data.length < 2 ? h('p', { class: 'muted small', style: 'padding:36px 0' }, 'Not enough samples yet. Come back after a few refreshes.') : h('canvas', { id: 'chart', style: 'width:100%;height:210px;display:block' })));
  if (data.length) {
    let hits = 0;
    for (let i = 1; i < data.length; i++) if (data[i - 1].s < 90 && data[i].s >= 90) hits++;
    p.append(h('div', { class: 'row' }, [['Peak session', Math.floor(Math.max(...data.map((d) => d.s))) + '%'], ['Peak week', Math.floor(Math.max(...data.map((d) => d.w))) + '%'], ['Hit 90%+', hits + '×']]
      .map(([k, v]) => h('div', { class: 'card grow' }, h('div', { class: 'label' }, k), h('h2', { style: 'margin-top:6px' }, v)))));
    const days = histRange === 2 ? 14 : 7, byDay = {};
    for (const s of S.history) { const k = new Date(s.t).toDateString(); byDay[k] = Math.max(byDay[k] || 0, s.s); }
    const bars = h('div', { class: 'bars' });
    for (let i = days - 1; i >= 0; i--) {
      const d = new Date(now - i * 864e5), pk = byDay[d.toDateString()] || 0;
      bars.append(h('div', {}, h('i', { class: pk ? '' : 'zero', style: `height:${Math.max(2, pk)}%` }), h('span', { class: 'label', style: 'margin-top:4px' }, d.toLocaleDateString([], { weekday: 'narrow' }))));
    }
    p.append(h('div', { class: 'card' }, h('div', { class: 'label', style: 'margin-bottom:12px' }, 'Daily session peak'), bars));
  }
  return p;
}
function drawHistory() {
  const cv = $('#chart');
  if (!cv) return;
  const now = Date.now(), from = now - SPANS[histRange], data = S.history.filter((s) => s.t >= from);
  const dpr = devicePixelRatio || 1, W = cv.clientWidth, H = cv.clientHeight;
  cv.width = W * dpr; cv.height = H * dpr;
  const c = cv.getContext('2d'); c.setTransform(dpr, 0, 0, dpr, 0, 0);
  const L = 30, T = 8, B = H - 18, R = W, line = css('--line'), muted = css('--muted');
  const x = (t) => L + (R - L) * (t - from) / (now - from), y = (v) => B - (B - T) * v / 100;
  c.font = '10px system-ui'; c.fillStyle = muted; c.strokeStyle = line; c.lineWidth = 1;
  for (const v of [0, 50, 100]) { c.beginPath(); c.moveTo(L, y(v)); c.lineTo(R, y(v)); c.stroke(); c.textAlign = 'left'; c.fillText(String(v), 0, y(v) + 4); }
  c.textAlign = 'left'; c.fillText(SPANS[histRange] <= 864e5 ? '24h ago' : Math.round(SPANS[histRange] / 864e5) + 'd ago', L, H - 3);
  c.textAlign = 'right'; c.fillText('now', R, H - 3);
  const series = (sel, col, w) => {
    c.strokeStyle = col; c.lineWidth = w; c.lineJoin = 'round'; c.lineCap = 'round'; c.beginPath();
    let prev = null;
    for (const s of data) { const v = sel(s); if (!prev || v < sel(prev) - 5) c.moveTo(x(s.t), y(v)); else c.lineTo(x(s.t), y(v)); prev = s; }
    c.stroke();
    const l = data[data.length - 1];
    if (l) { c.fillStyle = col; c.beginPath(); c.arc(x(l.t), y(sel(l)), 3.5, 0, Math.PI * 2); c.fill(); }
  };
  series((s) => s.w, css('--ink'), 1.5);
  series((s) => s.s, css('--clay'), 2);
}

// ───────────────────────── Settings ─────────────────────────
const ch = (ic, title, tip, ...right) => h('div', { class: 'row chead' }, icon(ic), h('div', { class: 'label grow' }, title), tip ? info(tip) : null, ...right);
function settingsView() {
  const st = S.settings;
  const set = async (patch) => { S.settings = await cm.setSettings(patch); render(); };
  const p = page(head('Settings'));
  p.append(updatesCard());
  p.append(syncCard());
  p.append(h('div', { class: 'card' }, ch('tour', 'Help', 'The tour lights up each button and explains it. The setup walks you through server, projects, voice, colors and phone again.',
    ibtn('tour', 'Take the tour', () => startTour()), h('button', { class: 'ghost sm', onclick: () => { OB.step = 0; startTutorial(); } }, 'Setup'))));
  p.append(h('div', { class: 'card stack' }, ch('paint', 'Appearance', 'Auto follows your desktop and switches between light and dark by itself. “My desktop” uses your desktop’s accent color (found automatically on Windows, macOS, GNOME, KDE and Hyprland). The app icon follows the color you choose.'),
    h('div', { class: 'row' }, [['light', 'Light'], ['dark', 'Dark'], ['system', 'Auto']].map(([v, l]) => chip(l, (st.theme || 'system') === v, () => set({ theme: v })))),
    h('div', { class: 'row wrap' }, [['claude', 'Hearth'], ['system', 'My desktop'], ['custom', 'Pick a color']].map(([v, l]) => chip(l, (st.palette || 'claude') === v, () => set({ palette: v }))),
      st.palette === 'custom' ? h('input', { type: 'color', value: st.customAccent || '#F0643C', style: 'width:52px;height:36px;padding:2px', onchange: (e) => set({ customAccent: e.target.value }) }) : null)));
  p.append(h('div', { class: 'card stack' }, ch('bell', 'Alerts', 'You get a notification when your usage crosses these levels. “Session reset” tells you when a heavy 5-hour session rolls over and you have a fresh one.'),
    h('div', { class: 'row' }, [50, 75, 90, 100].map((v) => chip(v + '%', st.thresholds.includes(v), () => set({ thresholds: st.thresholds.includes(v) ? st.thresholds.filter((x) => x !== v) : [...st.thresholds, v] })))),
    h('div', { class: 'row' }, h('div', { class: 'grow' }, 'Session reset'), toggle(st.notifyReset, () => set({ notifyReset: !st.notifyReset })))));
  p.append(h('div', { class: 'card stack' }, ch('phone', 'Call me', 'A call window and sound when Claude finishes a task, asks a question or needs permission. It follows the relay’s “away” switch, shared with your phone: turn it on while you are away from the keyboard. Needs relay/install_hooks.py run once.'),
    h('div', { class: 'row' }, h('div', { class: 'grow' }, 'Ring on this computer'), toggle(st.callMe, () => set({ callMe: !st.callMe })))));
  p.append(devicesCard(), computersCard(), hooksCard());
  p.append(h('div', { class: 'card stack' }, ch('settings', 'Startup', 'Closing the window keeps your phone connected. Quit from the tray icon.'),
    h('div', { class: 'row' }, h('div', { class: 'grow' }, 'Keep running in the background'), toggle(st.background !== false, () => set({ background: st.background === false }))),
    h('div', { class: 'row' }, h('div', { class: 'grow' }, 'Start when I log in'), toggle(!!st.autostart, async () => { await cm.setAutostart(!st.autostart); S = await cm.state(); render(); }))));
  const v = h('div', { class: 'card stack' }, ch('mic', 'Voice', 'Talk to Claude and hear it answer. Local voices (Whisper and Piper) run on your own computer; run relay/install_voice.sh to add them.'), h('div', { class: 'row' }, h('span', { class: 'pill' }, h('i'), h('span', { id: 'vstat' }, 'Checking…'))));
  cm.machines().then((ms) => ms[0] ? cm.voiceStatus(ms[0].id).then((s) => { const e = $('#vstat'); if (e) { e.textContent = s.stt && s.tts ? 'Ready' : 'Not installed'; e.parentElement.className = 'pill ' + (s.stt && s.tts ? 'ok' : ''); } }).catch(() => { const e = $('#vstat'); if (e) e.textContent = 'Not reachable'; }) : null);
  p.append(v);
  p.append(h('div', { class: 'card' }, ch('support', 'Support', 'Found a bug, have a question or an idea? Write to us. We answer by email.', h('button', { class: 'primary sm', onclick: () => openSupport() }, 'Contact'))));
  return p;
}

/** Which computers and servers this app talks to. Chats pick from these; the first own computer is the default. */
/** One place that shows how the server, this laptop, the synced projects folder and your phone fit together. */
function devicesCard() {
  const card = h('div', { class: 'card stack' }, ch('devices', 'Your devices', 'How the server, this computer, the synced projects folder and your phone fit together.'), h('div', { class: 'small muted' }, 'Checking…'));
  const dot = (ok) => h('span', { class: 'pill ' + (ok ? 'ok' : 'warn'), style: 'padding:2px 9px' }, h('i'), ok ? 'OK' : 'Check');
  const row = (name, ok, text) => h('div', { class: 'row' }, h('div', { class: 'grow' }, h('div', {}, name), h('div', { class: 'small muted' }, text)), dot(ok));
  let showing = false;
  const draw = async () => {
    let st; try { st = await cm.hubStatus(); } catch { st = null; }
    card.innerHTML = '';
    card.append(ch('devices', 'Your devices', 'How the server, this computer, the synced projects folder and your phone fit together.'));
    if (!st || !st.server) {
      card.append(h('div', { class: 'small muted' }, 'Connect your server below (paste the pairing link in “Computers and servers”). Then your laptop, the server and your phone work as one: chats and projects stay in sync, and work moves to your laptop whenever it is online.'));
      return;
    }
    const sv = st.server, sync = (x) => (!x || !x.available ? 'sync not running' : x.state === 'idle' && !x.needFiles ? 'up to date' : `syncing${x.needFiles ? ', ' + x.needFiles + ' files left' : ''}`);
    card.append(row('Server · ' + sv.name, sv.online, sv.online ? 'Online. ' + (sv.queued ? sv.queued + ' task(s) waiting for your laptop.' : 'Nothing waiting.') : 'Cannot be reached right now. It reconnects on its own.'));
    const linked = sv.online && sv.laptop && sv.laptop.toLowerCase() === String(st.me).toLowerCase();
    card.append(row('This laptop · ' + st.me, !!linked, linked ? 'Linked: the server hands work to it while it is online.' : sv.online ? 'Not linked yet (a moment after startup, or restart the app).' : 'Waiting for the server.'));
    const lsOk = st.local && st.local.sync && st.local.sync.available && st.local.sync.state === 'idle' && !st.local.sync.needFiles;
    card.append(row('Projects folder sync', !!(lsOk && sv.sync && sv.sync.available), `Laptop: ${sync(st.local && st.local.sync)} · Server: ${sync(sv.sync)}`));
    card.append(row('Phone', sv.listeners > 1, sv.listeners > 1 ? 'Connected devices are listening for updates.' : 'No phone is connected right now.'));
    const out = h('div', { class: 'stack', style: 'gap:6px' });
    out.append(h('button', { class: 'ghost sm', onclick: async () => {
      out.innerHTML = '';
      try {
        const r = await cm.phoneCode(); showing = true; setTimeout(() => { showing = false; }, 600000);
        out.append(h('div', { class: 'small muted' }, 'On your phone: Settings → Computers and servers → Add. Enter the address and this code (valid 10 minutes):'),
          h('div', { class: 'row' }, h('div', { style: 'font:600 30px ui-monospace,monospace;letter-spacing:.16em' }, r.code.replace(/(\d{3})(\d{3})/, '$1 $2')), h('div', { class: 'small muted' }, r.address)));
      } catch (e) { out.append(h('div', { class: 'small err' }, clean(e))); }
    } }, 'Add a phone'), );
    card.append(out);
  };
  draw();
  const t = setInterval(() => { if (!card.isConnected) return clearInterval(t); if (!showing) draw(); }, 15000);
  return card;
}

/** Is everything in sync? Files and chats, in plain words, with a button to sync right now. */
function syncCard() {
  const card = h('div', { class: 'card stack' }, ch('sync', 'Sync', 'Your project files and chats are kept the same on this computer, your server and your phone. “Sync now” checks right away.'), h('div', { class: 'small muted' }, 'Checking…'));
  const draw = async () => {
    let st = null; try { st = await cm.relay('local', 'GET', '/sync/status'); } catch { /* none */ }
    card.innerHTML = ''; card.append(ch('sync', 'Sync', 'Your project files and chats are kept the same on this computer, your server and your phone. “Sync now” checks right away.'));
    if (!st || st.role === 'single' || !st.linked) { card.append(h('div', { class: 'small muted' }, 'Not connected to a server yet. Connect one under “Computers and servers” and your files and chats will stay the same everywhere.')); return; }
    const f = st.files && st.files.last, c = st.last;
    const ago = (t) => (t ? ago2(t) : 'not yet');
    const ago2 = (t) => { const s = Math.round((Date.now() - t) / 1000); return s < 60 ? 'just now' : s < 3600 ? Math.round(s / 60) + ' min ago' : Math.round(s / 3600) + ' h ago'; };
    const row = (name, okk, text) => h('div', { class: 'row' }, h('div', { class: 'grow' }, h('div', {}, name), h('div', { class: 'small muted' }, text)), h('span', { class: 'pill ' + (okk ? 'ok' : 'warn'), style: 'padding:2px 9px' }, h('i'), okk ? 'OK' : 'Check'));
    card.append(row('Project files', !!(f && f.ok), f ? `${st.files.tracked || 0} files kept the same · last check ${ago(f.at)}` + (f.paused ? ' · ' + f.paused : '') + (f.errors && f.errors.length ? ' · ' + f.errors[0] : '') : 'Waiting for the first check'));
    card.append(row('Chats', !!(c && c.ok), c ? `${c.localChats || 0} chats · last check ${ago(c.at)}` + (c.errors && c.errors.length ? ' · ' + c.errors[0] : '') : 'Waiting for the first check'));
    if ((st.pending || []).length) card.append(h('div', { class: 'small' }, st.pending.length + ' project(s) from your server are waiting to be installed. Look for the card in the corner.'));
    const rej = (st.files && st.files.rejected) || [];
    if (rej.length) card.append(h('div', { class: 'stack', style: 'gap:6px' }, h('div', { class: 'small muted' }, 'Projects you said no to (they are not copied to this computer):'), rej.map((n) => h('div', { class: 'row' }, h('div', { class: 'grow' }, n), h('button', { class: 'ghost sm', onclick: async () => { try { await cm.relay('local', 'POST', '/sync/projects/allow', { names: [n] }); } catch { /* shown on next refresh */ } draw(); } }, 'Offer again')))));
    const b = h('button', { class: 'ghost sm', onclick: async () => { b.textContent = 'Syncing…'; try { await cm.relay('local', 'POST', '/sync/now', {}); } catch { /* shown below */ } draw(); } }, 'Sync now');
    card.append(b);
  };
  draw();
  const t = setInterval(() => { if (!card.isConnected) return clearInterval(t); draw(); }, 20000);
  return card;
}

function computersCard() {
  const card = h('div', { class: 'card stack' });
  let found = [], pairing = null;
  const draw = async () => {
    const ms = await cm.machines();
    SS.machines = ms;
    card.innerHTML = '';
    card.append(ch('server', 'Computers and servers', 'The computers and servers this app talks to. Chats run on whichever you pick. Add a server with its address and 6-digit code, or scan your Wi-Fi.', ibtn('refresh', 'Scan the Wi-Fi', async () => { found = await cm.discover(); draw(); }, 'ghost sm'), ibtn('plus', 'Add a computer or server', () => { pairing = pairing ? null : {}; draw(); }, 'ghost sm')));
    for (const m of ms) card.append(h('div', { class: 'row' }, h('div', { class: 'grow' }, h('div', {}, m.name), h('div', { class: 'small muted' }, (m.secure ? 'Server · ' : m.local ? 'This computer' : 'Computer · ') + (m.local ? '' : m.host))),
      m.local ? null : h('button', { class: 'link', onclick: async () => { SS.machines = await cm.removeMachine(m.id); if (SS.mid === m.id) SS.mid = null; draw(); } }, 'Remove')));
    for (const f of found.filter((x) => !ms.some((m) => m.host === x.host))) card.append(h('div', { class: 'row small' }, h('div', { class: 'grow' }, f.name + ' · found on your network'), h('button', { class: 'link', onclick: () => { pairing = f; draw(); } }, 'Pair')));
    if (pairing) {
      const f = pairing, name = h('input', { placeholder: 'Name (optional)', value: f.name || '' }), host = h('input', { placeholder: 'Pairing link, https://… or 192.168.1.20', value: f.host ? (f.port === 47601 ? f.host : f.host + ':' + f.port) : '' });
      const tok = h('input', { placeholder: 'Token (not needed with a link)', style: 'font-family:ui-monospace,monospace' }), err = h('div', { class: 'small err' });
      card.append(name, host, tok, err, h('button', { class: 'primary sm', onclick: async () => {
        try { await cm.addMachine({ name: name.value, host: host.value, token: tok.value }); pairing = null; found = []; draw(); } catch (e) { err.textContent = clean(e); }
      } }, 'Connect'));
    }
  };
  draw();
  return card;
}

/** Updates for this computer, your phone and the server, with a button to look for them right now. */
function updatesCard() {
  const rows = h('div', { class: 'stack', style: 'gap:10px' });
  const checkBtn = h('button', { class: 'ghost sm' }, 'Check'), goBtn = h('button', { class: 'primary sm', style: 'display:none' }, 'Update now');
  const note = h('div', { class: 'small muted' });
  const ver = h('span', { class: 'small muted' }); cm.version().then((v) => { ver.textContent = 'Hearth ' + v; }).catch(() => {});
  const card = h('div', { class: 'card stack' }, ch('sync', 'Updates', 'Hearth looks on GitHub for a newer version. The app, your server and (on the phone) the phone app update themselves; press Update when it shows.', ver), rows, h('div', { class: 'row' }, checkBtn, goBtn, h('div', { class: 'grow' }), note));
  const NAMES = [['desktop', 'This computer', 'the app and its background service'], ['phone', 'Phone app', 'built here and installed over USB'], ['server', 'Server', 'your server']];
  let det = null;
  const draw = () => {
    rows.innerHTML = '';
    if (!det) { rows.append(h('div', { class: 'small muted' }, 'Checking…')); return; }
    if (det.unavailable) { rows.append(h('div', { class: 'small muted' }, det.unavailable)); return; }
    const names = det.mode === 'release' ? [['desktop', 'This app', 'downloads the new version from GitHub'], ['server', 'Server', 'your server'], ['phone', 'Phone app', 'updates itself']] : (det.github ? [['github', 'Code on GitHub', 'the newest version of Hearth'], ...NAMES] : NAMES);
    for (const [k, name, what] of names) {
      const r = det[k] || {};
      rows.append(h('div', { class: 'row' }, h('span', { class: 'pill ' + (r.need ? 'warn' : r.na && k !== 'phone' ? '' : 'ok') }, h('i')), icon(k === 'server' ? 'server' : k === 'phone' ? 'phone' : k === 'github' ? 'git' : 'devices'), h('div', { class: 'grow' }, h('div', {}, name), h('div', { class: 'small ' + (r.need ? '' : 'muted') }, r.text || '')), info(what)));
    }
    const any = ['github', ...NAMES.map((x) => x[0])].some((k) => det[k] && det[k].need);
    goBtn.style.display = any ? '' : 'none';
    note.textContent = det.checkedAt ? 'Checked ' + new Date(det.checkedAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) : '';
  };
  const check = async () => {
    checkBtn.disabled = true; checkBtn.textContent = 'Checking…';
    try { det = await cm.updateCheck(); } catch (e) { det = { unavailable: clean(e) }; }
    checkBtn.disabled = false; checkBtn.textContent = 'Check'; draw();
  };
  checkBtn.onclick = check;
  goBtn.onclick = () => { const want = {}; for (const k of ['github', ...NAMES.map((x) => x[0])]) want[k] = !!(det && det[k] && det[k].need); cm.updateRun(want); };
  draw(); check();
  return card;
}

function pairCard() {
  const card = h('div', { class: 'card stack' }, ch('phone', 'Connect your phone'), h('div', { class: 'small muted' }, 'Checking…'));
  cm.pairInfo().then(async (i) => {
    card.innerHTML = '';
    card.append(ch('phone', 'Connect your phone'));
    if (i.error) { card.append(h('div', { class: 'small err' }, 'The phone connection could not start: ' + i.error)); return; }
    if (!i.claude) card.append(h('div', { class: 'small err' }, 'Claude Code was not found on this computer. Install it from claude.com/claude-code, then restart this app.'));
    card.append(h('ol', { class: 'oblist small' }, h('li', {}, 'Put your phone on the same Wi-Fi as this computer.'), h('li', {}, 'Open Hearth on the phone and tap “Find my computer”.'), h('li', {}, 'A card appears here. Press Accept. Done!')));
    card.append(h('div', { class: 'row' }, h('div', { class: 'grow' }, h('div', {}, 'Tell me when a phone wants to connect'), h('div', { class: 'small muted' }, 'A notification and a card. If you turn it off, requests still wait here for two minutes.')),
      toggle(S.settings.pairNotify !== false, async () => { S.settings = await cm.setSettings({ pairNotify: S.settings.pairNotify === false }); render(); })));
    const waiting = await cm.pairRequests().catch(() => []);
    waiting.forEach((r) => card.append(h('div', { class: 'row crow' }, h('div', { class: 'grow' }, h('b', {}, r.name), h('div', { class: 'small muted' }, 'wants to connect · ' + r.ip)),
      h('button', { class: 'ghost sm', onclick: async () => { await cm.decidePair(r.id, false); render(); } }, 'Decline'), h('button', { class: 'primary sm', onclick: async () => { await cm.decidePair(r.id, true); render(); } }, 'Accept'))));
    if (i.external) card.append(h('div', { class: 'small muted' }, 'A background service on this computer handles your phone. Pair with the address and code below.'));
    card.append(h('details', { class: 'small muted' }, h('summary', {}, 'Other ways to connect (address and code)'),
      h('div', { class: 'stack', style: 'gap:6px;margin-top:8px' },
        h('div', {}, 'If your phone cannot find this computer, type this in the Hearth app under “Enter the address myself”:'),
        h('div', { class: 'row' }, h('div', { style: 'font:600 30px ui-monospace,monospace;letter-spacing:.18em' }, (i.code || '').replace(/(\d{3})(\d{3})/, '$1 $2')), h('button', { class: 'link', onclick: async () => { await cm.newPairCode(); render(); } }, 'New code')),
        h('div', {}, 'Address: ' + (i.addresses.length ? i.addresses.map((x) => x + (i.port === 47601 ? '' : ':' + i.port)).join(' or ') : 'check your network')),
        i.tailscale ? h('div', {}, 'Away from home with Tailscale: ' + i.tailscale + (i.port === 47601 ? '' : ':' + i.port)) : h('div', {}, 'Away from home? Install the free app Tailscale on this computer and your phone. Hearth then shows a second address here.'),
        i.platform === 'win32' ? h('div', {}, 'If Windows asks, allow Hearth through the firewall on private networks.') : null,
        h('div', { class: 'row' }, h('div', { class: 'grow' }, h('div', {}, 'Allow connections from the internet'), h('div', {}, 'Advanced. Only if you opened a port on your router. It is not encrypted, so Tailscale is the safer way.')),
          toggle(!!i.allowRemote, async () => { S.settings = await cm.setSettings({ allowRemote: !i.allowRemote }); toast('Restart Hearth to apply this', () => cm.relaunch()); render(); })))));
  }).catch(() => { card.append(h('div', { class: 'small err' }, 'Could not read the connection status.')); });
  return card;
}

function hooksCard() {
  const card = h('div', { class: 'card stack' }, h('div', { class: 'label' }, 'Calls from Claude Code'), h('div', { class: 'small muted' }, 'Checking…'));
  cm.pairInfo().then((i) => {
    card.innerHTML = '';
    card.append(h('div', { class: 'label' }, 'Calls from Claude Code'),
      h('div', { class: 'small muted' }, 'Lets Claude Code tell this app when it finishes a task, asks you a question, or needs permission, so your phone can ring. This adds three small hooks to your Claude Code settings (a backup is made first). Turn on “Call me” in the phone app to receive them.'),
      h('div', { class: 'row' }, h('span', { class: 'pill ' + (i.hooks ? 'ok' : '') }, h('i'), i.hooks ? 'Enabled' : 'Not enabled'), h('div', { class: 'grow' }),
        i.hooks ? h('button', { class: 'ghost sm', onclick: async () => { await cm.removeHooks(); render(); } }, 'Remove')
          : h('button', { class: 'primary sm', onclick: async () => { await cm.installHooks(); render(); } }, 'Enable')));
  });
  return card;
}

// system colors: follow the desktop's accent color
const FLAME_OUT = 'M256 96c10 52 78 86 78 170a78 78 0 0 1-156 0c0-34 18-58 34-76 2 24 14 38 28 44-8-50 4-98 16-138z';
const FLAME_IN = 'M256 262c6 26 38 40 38 72a38 38 0 0 1-76 0c0-20 10-30 20-42 2 12 8 18 14 20-4-24 0-38 4-50z';
const shade = (hex, f) => { const n = parseInt(hex.slice(1), 16); const c = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((x) => Math.max(0, Math.min(255, Math.round(f >= 0 ? x + (255 - x) * f : x * (1 + f))))); return '#' + c.map((x) => x.toString(16).padStart(2, '0')).join(''); };
/** The app's icon (window corner, taskbar, tray) in the colors you picked. */
function updateAppIcon(accent) {
  try {
    const c = document.createElement('canvas'); c.width = c.height = 256; const g = c.getContext('2d'), a = accent || '#F0643C';
    const grad = g.createLinearGradient(0, 0, 256, 256); grad.addColorStop(0, accent ? shade(a, .14) : '#FF8A4C'); grad.addColorStop(1, accent ? shade(a, -.18) : '#E5484D');
    g.fillStyle = grad; g.beginPath(); g.roundRect(0, 0, 256, 256, 56); g.fill();
    g.save(); g.scale(.5, .5); g.fillStyle = '#fff'; g.fill(new Path2D(FLAME_OUT)); g.fillStyle = '#FFD3A8'; g.fill(new Path2D(FLAME_IN)); g.strokeStyle = '#fff'; g.lineWidth = 24; g.lineCap = 'round'; g.beginPath(); g.moveTo(150, 392); g.lineTo(362, 392); g.stroke(); g.restore();
    cm.setIcon(c.toDataURL('image/png'));
  } catch { /* keep the default icon */ }
}
/** Colors: Hearth's own, the desktop's accent color, or one you pick. */
async function applyPalette() {
  const root = document.documentElement, pal = (S.settings && S.settings.palette) || 'claude';
  let a = null;
  if (pal === 'system') a = await cm.accent(); else if (pal === 'custom') a = (S.settings && S.settings.customAccent) || '#F0643C';
  if (!a) { root.removeAttribute('data-palette'); ['--accent', '--onclay'].forEach((v) => root.style.removeProperty(v)); updateAppIcon(null); return; }
  const n = parseInt(a.slice(1), 16), lum = (0.299 * ((n >> 16) & 255) + 0.587 * ((n >> 8) & 255) + 0.114 * (n & 255)) / 255;
  root.dataset.palette = 'system';
  root.style.setProperty('--accent', a);
  root.style.setProperty('--onclay', lum > 0.62 ? '#111111' : '#ffffff');
  updateAppIcon(a);
}
setInterval(() => { if (S.settings && S.settings.palette === 'system') applyPalette(); }, 20000);

// ───────────────────────── wiring ─────────────────────────
/** Redraws a screen only when what it shows has changed: no flicker on every refresh. */
let lastSig = '';
const sigOf = () => JSON.stringify([tab, S.signedIn, S.usage, S.error, S.settings, S.history && S.history.length, typeof D !== 'undefined' ? [D.err, D.stats && { ...D.stats, generated: 0 }, D.live && D.live.length, D.voice] : 0, typeof UR !== 'undefined' ? UR.data && { ...UR.data, generated: 0 } : 0]);
function softRender() { const sig = sigOf(); if (sig === lastSig) return; lastSig = sig; render(); }
cm.on('usage', (s) => { const pal = S.settings && S.settings.palette; S = s; if (pal !== (S.settings || {}).palette) applyPalette(); if (tab === 'dashboard' || tab === 'now' || tab === 'history' || !S.signedIn) softRender(); else renderNav(); });
cm.on('goto', (t) => setTab(t));
cm.on('open-session', async (o) => { SS.machines = await cm.machines(); SS.mid = o.machine; openSession({ id: o.session, title: o.title || 'Session', cwd: o.cwd, mid: o.machine, live: true }, o.voice); });
setInterval(() => { if (tab === 'now' || tab === 'history') render(); }, 30000);
matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => render());
cm.state().then(async (s) => { S = s; await applyPalette(); render(); });
