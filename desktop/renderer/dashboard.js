'use strict';
/* Dashboard: limits, token activity from your local Claude Code logs, projects, tools, and live sessions. */
const D = { stats: null, err: null, loading: false, machineId: null, live: [], voice: null, range: 14, loadedAt: 0 };
const FAM = [['opus', 'Opus', '--c-opus'], ['sonnet', 'Sonnet', '--c-sonnet'], ['haiku', 'Haiku', '--c-haiku'], ['other', 'Other', '--c-other']];

const fmtN = (n) => n >= 1e9 ? (n / 1e9).toFixed(1) + 'B' : n >= 1e6 ? (n / 1e6).toFixed(1) + 'M' : n >= 1e3 ? Math.round(n / 1e3) + 'k' : String(Math.round(n));
const work = (d) => d.in + d.out + d.cw;

async function loadDashboard() {
  if (D.loading) return;
  D.loading = true;
  try {
    const ms = await cm.machines();
    const m = ms.find((x) => x.local) || ms[0];
    if (!m) { D.err = 'norelay'; D.stats = null; return; }
    D.machineId = m.id;
    D.stats = await cm.stats(m.id, 30);
    D.live = (await cm.relay(m.id, 'GET', '/sessions').catch(() => [])).filter((s) => s.live).map((s) => ({ ...s, mid: m.id }));
    D.voice = await cm.relay(m.id, 'GET', '/voice/status').catch(() => null);
    D.err = null; D.loadedAt = Date.now();
  } catch (e) { D.err = clean(e); }
  finally { D.loading = false; D.loadedAt = Date.now(); if (tab === 'dashboard') softRender(); }
}
setInterval(() => { if (tab === 'dashboard' && S.signedIn) loadDashboard(); }, 45000);

function ringSVG(pct, size, stroke) {
  const r = size / 2 - stroke / 2 - 1, c = 2 * Math.PI * r, p = Math.max(0, Math.min(100, pct)) / 100;
  const hot = pct >= 90;
  return `<svg width="${size}" height="${size}" viewBox="0 0 ${size} ${size}"><circle cx="${size / 2}" cy="${size / 2}" r="${r}" fill="none" stroke="var(--line)" stroke-width="${stroke}"/>` +
    `<circle cx="${size / 2}" cy="${size / 2}" r="${r}" fill="none" stroke="${hot ? '#B3402A' : 'var(--clay)'}" stroke-width="${stroke}" stroke-linecap="round" stroke-dasharray="${c * p} ${c}" transform="rotate(-90 ${size / 2} ${size / 2})" style="transition:stroke-dasharray .6s"/></svg>`;
}

function limitPanel(name, sub, l, ms) {
  const now = Date.now();
  const card = h('div', { class: 'card ringcard' });
  const pct = l ? CMW.usedAt(l, now) : 0;
  card.append(h('div', { class: 'ringbox', html: ringSVG(pct, 150, 12) + `<div class="mid"><b>${l ? Math.floor(pct) : '–'}</b><span>%</span></div>` }));
  const side = h('div', { class: 'grow stack', style: 'gap:8px' }, h('div', { class: 'row' }, h('h3', { class: 'grow' }, name), h('span', { class: 'label' }, sub)));
  if (!l) { side.append(h('div', { class: 'muted small' }, 'No data yet')); card.append(side); return card; }
  const r = l.resetsAt, pc = CMW.pace(l, ms, now), w = CMW.paceWord(l, pc);
  side.append(h('div', { 'data-resets': r || '' }, r && r > now ? 'Resets in ' + CMW.dur(r - now) : 'Fresh window', h('span', { class: 'muted small' }, r && r > now ? '  ·  ' + at(r, ms === CMW.WEEK_MS) : '')));
  if (w) side.append(h('div', { class: 'row wrap' }, h('span', { class: 'pill ' + (w === 'Ahead of pace' ? 'warn' : 'ok') }, h('i'), w),
    h('span', { class: 'small muted' }, pc.hitsAt ? 'Limit in ' + CMW.dur(pc.hitsAt - now) : pc.projected > 0 ? '~' + Math.min(100, Math.floor(pc.projected)) + '% by reset' : '')));
  if (ms === CMW.WEEK_MS && S.usage) for (const [n, x] of [['Opus', S.usage.opus], ['Sonnet', S.usage.sonnet]]) if (x) {
    const v = CMW.usedAt(x, now);
    side.append(h('div', {}, h('div', { class: 'row small' }, h('span', { class: 'grow muted' }, n), h('span', {}, Math.floor(v) + '%')), h('div', { class: 'meter thin', style: 'margin:4px 0 0' }, h('i', { style: `width:${v}%` }))));
  }
  card.append(side);
  return card;
}

function kpi(label, value, sub, delta) {
  return h('div', { class: 'card kpi' }, h('div', { class: 'label' }, label), h('div', { class: 'v' }, value),
    h('div', { class: 'small muted' }, sub || '', delta != null && isFinite(delta) ? h('span', { class: 'delta ' + (delta > 0 ? 'up' : 'dn') }, `  ${delta > 0 ? '↑' : '↓'} ${Math.abs(Math.round(delta))}% vs yesterday`) : null));
}

function tooltip() {
  let t = $('.tip');
  if (!t) { t = h('div', { class: 'tip' }); document.body.append(t); }
  return t;
}

function drawTokens(cv, days) {
  const dpr = devicePixelRatio || 1, W = cv.clientWidth, H = cv.clientHeight;
  cv.width = W * dpr; cv.height = H * dpr;
  const c = cv.getContext('2d'); c.setTransform(dpr, 0, 0, dpr, 0, 0);
  const L = 40, T = 10, B = H - 22, R = W, line = css('--line'), muted = css('--muted');
  const max = Math.max(1, ...days.map((d) => FAM.reduce((a, [k]) => a + (d.fam[k] || 0), 0)));
  const nice = (() => { const p = Math.pow(10, Math.floor(Math.log10(max))); return Math.ceil(max / p * 2) / 2 * p; })();
  c.font = '10.5px system-ui'; c.fillStyle = muted; c.strokeStyle = line; c.lineWidth = 1; c.textAlign = 'right';
  for (let i = 0; i <= 4; i++) { const v = nice * i / 4, y = B - (B - T) * i / 4; c.beginPath(); c.moveTo(L, y); c.lineTo(R, y); c.stroke(); c.fillText(fmtN(v), L - 6, y + 3.5); }
  const bw = (R - L) / days.length, gap = Math.min(6, bw * 0.28), rects = [];
  days.forEach((d, i) => {
    let y = B;
    for (const [k, , v] of FAM) {
      const val = d.fam[k] || 0; if (!val) continue;
      const h2 = (B - T) * val / nice;
      c.fillStyle = css(v); c.beginPath(); c.roundRect(L + i * bw + gap / 2, y - h2, bw - gap, h2, 2); c.fill(); y -= h2;
    }
    rects.push(L + i * bw);
    if (days.length <= 16 || i % 3 === 0) { c.fillStyle = muted; c.textAlign = 'center'; c.fillText(new Date(d.date + 'T12:00').toLocaleDateString([], days.length <= 16 ? { weekday: 'short' } : { day: 'numeric', month: 'short' }), L + i * bw + bw / 2, H - 6); }
  });
  const tip = tooltip();
  cv.onmousemove = (e) => {
    const x = e.offsetX, i = Math.floor((x - L) / bw);
    if (i < 0 || i >= days.length) { tip.style.display = 'none'; return; }
    const d = days[i];
    tip.textContent = new Date(d.date + 'T12:00').toLocaleDateString([], { weekday: 'long', day: 'numeric', month: 'short' }) + '\n' +
      FAM.filter(([k]) => d.fam[k]).map(([k, n]) => n + '  ' + fmtN(d.fam[k])).join('\n') + (d.msgs ? `\n${d.msgs} messages` : '');
    tip.style.display = 'block'; tip.style.left = e.clientX + 14 + 'px'; tip.style.top = e.clientY + 14 + 'px';
  };
  cv.onmouseleave = () => { tip.style.display = 'none'; };
}

function drawSpark(cv) {
  const now = Date.now(), from = now - 864e5 * 2, data = S.history.filter((s) => s.t >= from);
  const dpr = devicePixelRatio || 1, W = cv.clientWidth, H = cv.clientHeight;
  cv.width = W * dpr; cv.height = H * dpr;
  const c = cv.getContext('2d'); c.setTransform(dpr, 0, 0, dpr, 0, 0);
  const x = (t) => 4 + (W - 8) * (t - from) / (now - from), y = (v) => H - 6 - (H - 14) * v / 100;
  c.strokeStyle = css('--line'); c.lineWidth = 1;
  for (const v of [0, 50, 100]) { c.beginPath(); c.moveTo(0, y(v)); c.lineTo(W, y(v)); c.stroke(); }
  const series = (sel, col, w, fill) => {
    if (data.length < 2) return;
    c.strokeStyle = col; c.lineWidth = w; c.lineJoin = 'round'; c.beginPath();
    let prev = null;
    for (const s of data) { const v = sel(s); if (!prev || v < sel(prev) - 5) c.moveTo(x(s.t), y(v)); else c.lineTo(x(s.t), y(v)); prev = s; }
    c.stroke();
  };
  series((s) => s.w, css('--ink'), 1.5); series((s) => s.s, css('--clay'), 2);
  if (data.length < 2) { c.fillStyle = css('--muted'); c.font = '12px system-ui'; c.fillText('Collecting samples…', 8, H / 2); }
}

function heatmap(heat) {
  const max = Math.max(1, ...heat.flat());
  const g = h('div', { class: 'heat' });
  const dn = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];
  g.append(h('div'));
  for (let i = 0; i < 24; i++) g.append(h('div', { class: 'l', style: 'text-align:center' }, i % 6 === 0 ? String(i) : ''));
  heat.forEach((row, d) => {
    g.append(h('div', { class: 'l' }, dn[d]));
    row.forEach((v, hr) => {
      const cell = h('div', { class: 'c', title: `${dn[d]} ${hr}:00 · ${v} message${v === 1 ? '' : 's'}` });
      if (v) cell.style.background = `color-mix(in srgb, var(--clay) ${Math.round(18 + 82 * v / max)}%, var(--line))`;
      g.append(cell);
    });
  });
  return g;
}

const hbars = (items, fmt = fmtN) => {
  const max = Math.max(1, ...items.map((i) => i[1]));
  const box = h('div', { class: 'hbar' });
  for (const [n, v, color] of items) box.append(h('div', { class: 'small' }, n), h('div', { class: 'small muted' }, fmt(v)), h('div', { class: 't' }, h('i', { style: `width:${v / max * 100}%` + (color ? `;background:var(${color})` : '') })));
  return box;
};

function greeting() { const hr = new Date().getHours(); return hr < 5 ? 'Late night' : hr < 12 ? 'Good morning' : hr < 18 ? 'Good afternoon' : 'Good evening'; }

function dashboardView() {
  if (!D.stats && !D.loading && D.err == null) loadDashboard();
  else if (Date.now() - D.loadedAt > 60000 && !D.loading && D.err !== 'norelay') loadDashboard();
  const u = S.usage, now = Date.now();
  const root = h('div', { class: 'dash' });

  const pills = h('div', { class: 'row wrap' },
    h('span', { class: 'pill ' + (D.stats ? 'ok' : D.err ? 'warn' : '') }, h('i'), D.stats ? 'Relay online' : D.err === 'norelay' ? 'No relay' : D.err ? 'Relay unreachable' : 'Connecting…'),
    h('span', { class: 'pill ' + (D.voice && D.voice.stt && D.voice.tts ? 'ok' : '') }, h('i'), D.voice && D.voice.stt && D.voice.tts ? 'Voice ready' : 'Voice not installed'),
    h('span', { class: 'pill ' + (S.settings.callMe ? 'ok' : '') }, h('i'), S.settings.callMe ? 'Calls on' : 'Calls off'),
    u ? h('span', { class: 'small muted', 'data-ago': u.at }, 'Usage updated ' + ago(u.at, now)) : null);
  root.append(h('div', { class: 'hero' }, h('div', { class: 'grow' }, h('h1', {}, greeting() + (S.profile && S.profile.email ? '' : '.')),
    h('div', { class: 'muted' }, new Date().toLocaleDateString([], { weekday: 'long', day: 'numeric', month: 'long' }) + (S.profile && S.profile.plan ? '  ·  Claude ' + S.profile.plan : ''))),
    h('div', { class: 'row' }, ibtn('refresh', 'Refresh the numbers now', async () => { S = await cm.refresh(); loadDashboard(); render(); }),
      D.live[0] ? ibtn('mic', 'Talk to Claude', () => openSession(D.live[0], true), 'primary') : null)));
  root.append(pills);
  if (S.error) root.append(h('div', { class: 'card err small' }, S.error));

  root.append(h('div', { class: 'g2' }, limitPanel('Session', '5-hour window', u && u.session, CMW.SESSION_MS), limitPanel('Week', '7-day window', u && u.week, CMW.WEEK_MS)));

  if (typeof usageSection === 'function' && D.machineId) root.append(usageSection());
  if (D.err === 'norelay') {
    root.append(h('div', { class: 'card stack' }, h('h3', {}, 'Turn on activity stats'),
      h('p', { class: 'muted', style: 'margin:0' }, 'Token activity, projects, tools and live sessions come from your Claude Code logs through the relay. Run this once on this computer:'),
      h('code', { style: 'background:var(--bg);padding:10px 12px;border-radius:10px' }, 'bash relay/install.sh')));
    return root;
  }
  if (D.err) root.append(h('div', { class: 'card err small' }, D.err));
  if (!D.stats) { root.append(h('div', { class: 'card muted' }, 'Reading your Claude Code activity…')); return root; }

  const days = D.stats.days, today = days[days.length - 1], yday = days[days.length - 2] || { in: 0, out: 0, cw: 0, cr: 0, msgs: 0 };
  const wk = days.slice(-7), cr = wk.reduce((a, d) => a + d.cr, 0), fresh = wk.reduce((a, d) => a + d.in + d.cw, 0);
  const tT = work(today), yT = work(yday);
  root.append(h('div', { class: 'g4' },
    kpi('Tokens today', fmtN(tT), `${fmtN(today.out)} written · ${fmtN(today.cr)} cached reads`, yT ? (tT - yT) / yT * 100 : null),
    kpi('Messages today', String(today.msgs), `${wk.reduce((a, d) => a + d.msgs, 0)} this week`, yday.msgs ? (today.msgs - yday.msgs) / yday.msgs * 100 : null),
    kpi('Sessions today', String(today.sessions), `${D.live.length} open right now`),
    kpi('Cache hit rate', cr + fresh ? Math.round(cr / (cr + fresh) * 100) + '%' : '–', 'Last 7 days, higher is cheaper')));

  const shown = days.slice(-D.range);
  const tok = h('canvas', { style: 'width:100%;height:240px;display:block' });
  const spark = h('canvas', { style: 'width:100%;height:150px;display:block;margin-top:8px' });
  root.append(h('div', { class: 'g32' },
    h('div', { class: 'card' }, h('div', { class: 'row' }, h('div', { class: 'label grow' }, 'Tokens per day'), [14, 30].map((n) => chip(n + 'd', D.range === n, () => { D.range = n; render(); }))),
      h('div', { class: 'legend', style: 'margin:10px 0' }, FAM.filter(([k]) => D.stats.models[k]).map(([k, n, v]) => h('span', {}, h('i', { style: `background:var(${v})` }), n))), tok),
    h('div', { class: 'card' }, h('div', { class: 'label' }, 'Limits, last 48 hours'), h('div', { class: 'legend', style: 'margin-top:8px' }, h('span', {}, h('i', { style: 'background:var(--clay)' }), 'Session'), h('span', {}, h('i', { style: 'background:var(--ink)' }), 'Week')), spark)));

  const proj = D.stats.projects.slice(0, 6).map((p) => [p.cwd.replace(/\/$/, '').split('/').slice(-2).join('/') || p.cwd, p.tokens]);
  const mods = FAM.filter(([k]) => D.stats.models[k]).map(([k, n, v]) => [n, D.stats.models[k], v]);
  const tools = Object.entries(D.stats.tools).slice(0, 7);
  root.append(h('div', { class: 'g32' },
    h('div', { class: 'card' }, h('div', { class: 'label', style: 'margin-bottom:10px' }, 'When you work with Claude · last 30 days'), heatmap(D.stats.heat)),
    h('div', { class: 'card' }, h('div', { class: 'label' }, 'Top projects · 30 days'), proj.length ? hbars(proj) : h('p', { class: 'muted small' }, 'No activity yet'))));
  root.append(h('div', { class: 'g2' },
    h('div', { class: 'card' }, h('div', { class: 'label' }, 'Models · 30 days'), mods.length ? hbars(mods) : h('p', { class: 'muted small' }, 'No activity yet')),
    h('div', { class: 'card' }, h('div', { class: 'label' }, 'Most used tools'), tools.length ? hbars(tools, (v) => v + '×') : h('p', { class: 'muted small' }, 'No activity yet'))));

  const live = h('div', { class: 'card stack' }, h('div', { class: 'label' }, 'Open Claude Code sessions'));
  if (!D.live.length) live.append(h('p', { class: 'muted small', style: 'margin:0' }, 'None right now. Start `claude` in a terminal and it shows up here.'));
  D.live.forEach((s) => live.append(h('div', { class: 'sesscard' }, h('span', { class: 'dot' }), h('div', { class: 'grow' }, h('div', {}, s.title), h('div', { class: 'small muted' }, short(s.cwd) + (s.busy ? ' · working' : ''))),
    h('button', { class: 'ghost', onclick: () => openSession(s, false) }, 'Open'), h('button', { class: 'primary', onclick: () => openSession(s, true) }, 'Talk'))));
  root.append(live);

  requestAnimationFrame(() => { drawTokens(tok, shown); drawSpark(spark); });
  return root;
}

/** Time labels tick by themselves, without redrawing the screen. */
setInterval(() => {
  const now = Date.now();
  document.querySelectorAll('[data-ago]').forEach((e) => { e.textContent = 'Usage updated ' + ago(Number(e.dataset.ago), now); });
  document.querySelectorAll('[data-resets]').forEach((e) => { const r = Number(e.dataset.resets); if (r && r > now && e.firstChild && e.firstChild.nodeType === 3) e.firstChild.nodeValue = 'Resets in ' + CMW.dur(r - now); });
}, 15000);
