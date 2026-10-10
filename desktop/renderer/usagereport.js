'use strict';
/* "Usage and cost": everything a /usage-style report shows, for 7 days, 30 days or all time. Cost is the API-equivalent value of the tokens. */
const UR = { range: 30, data: null, key: '', loading: false, loadedAt: 0, err: null };
const money = (n) => (n >= 100 ? '$' + Math.round(n).toLocaleString() : n >= 1 ? '$' + n.toFixed(2) : '$' + n.toFixed(n < 0.01 && n > 0 ? 3 : 2));
const urDur = (ms) => { const d = Math.floor(ms / 864e5), h = Math.floor(ms % 864e5 / 36e5), m = Math.floor(ms % 36e5 / 6e4); return d ? d + 'd ' + h + 'h' : h ? h + 'h ' + m + 'm' : m + ' min'; };

const CUR = { '€': 1.08, '$': 1, '£': 1.27 };
/** Your whole account: the server holds the chats of all your computers. Without a server, only this computer. */
const urMachine = () => { const ms = SS.machines || []; return ms.find((m) => m.secure) || ms.find((m) => m.local) || ms[0] || null; };
async function loadUsageReport() {
  const um = urMachine();
  if (UR.loading || !um) return;
  const key = um.id + ':' + UR.range;
  if (UR.key === key && Date.now() - UR.loadedAt < 60000) return;
  UR.loading = true;
  try { UR.data = await cm.relay(um.id, 'GET', '/usage-report?days=' + UR.range); UR.key = key; UR.err = null; }
  catch (e) { UR.err = clean(e); }
  UR.loading = false; UR.loadedAt = Date.now();
  if (tab === 'dashboard') softRender();
}

function planPriceDialog() {
  const amt = h('input', { type: 'number', min: '0', step: '0.5', placeholder: '25', value: S.settings.planPrice || '', style: 'width:110px' });
  let cur = S.settings.planCur || '€'; const curRow = h('div', { class: 'row' });
  const draw = () => { curRow.innerHTML = ''; ['€', '$', '£'].forEach((c) => curRow.append(chip(c, cur === c, () => { cur = c; draw(); }))); }; draw();
  const modal = h('div', { class: 'modal', onclick: (e) => { if (e.target === modal) modal.remove(); } }, h('div', { class: 'mcard' }, h('h3', { style: 'margin:0' }, 'What do you pay for Claude?'), h('div', { class: 'small muted' }, 'Per month. It is only used on this screen to compare with the API value. Stays on this computer.'),
    h('div', { class: 'row' }, curRow, amt, h('span', { class: 'small muted' }, 'per month')),
    h('div', { class: 'row' }, h('div', { class: 'grow' }), h('button', { class: 'ghost sm', onclick: () => modal.remove() }, 'Cancel'), h('button', { class: 'primary sm', onclick: async () => { S.settings = await cm.setSettings({ planPrice: Number(amt.value) || 0, planCur: cur }); modal.remove(); render(); } }, 'Save'))));
  document.body.append(modal);
}
function usageSection() {
  loadUsageReport();
  const r = UR.data;
  const card = h('div', { class: 'card stack ur' });
  card.append(h('div', { class: 'row wrap' }, h('div', { class: 'label grow' }, 'Usage and cost'),
    [[7, '7 days'], [30, '30 days'], [0, 'All time']].map(([n, l]) => chip(l, UR.range === n, () => { UR.range = n; UR.key = ''; UR.data = UR.data && UR.data.range === n ? UR.data : UR.data; loadUsageReport(); render(); }))));
  if (UR.err) { card.append(h('div', { class: 'small err' }, UR.err)); return card; }
  if (!r || r.range !== UR.range) { card.append(h('div', { class: 'small muted' }, 'Counting your tokens…')); return card; }
  const t = r.totals, fmt = (n) => (typeof fmtN === 'function' ? fmtN(n) : String(n));
  const price = Number(S.settings.planPrice) || 0, cur = S.settings.planCur || '€', rate = CUR[cur] || 1;
  const months = r.range ? r.range / 30 : Math.max(1, Math.ceil(((t.last || Date.now()) - (t.first || Date.now())) / (30 * 864e5)));
  const paid = price * months, worth = paid ? t.cost / (paid * rate) : 0;
  card.append(h('div', { class: 'row small muted' }, icon('devices'), h('span', {}, r.scope === 'account' ? 'All your devices' : 'This computer only'), info('Your server keeps the chats of every computer you linked, so the numbers cover your whole account. Without a server this shows only this computer.')));
  const tile = (label, value, sub) => h('div', { class: 'urtile' }, h('div', { class: 'small muted' }, label), h('div', { class: 'urval' }, value), sub ? h('div', { class: 'small muted' }, sub) : null);
  card.append(h('div', { class: 'urgrid' },
    price ? tile('You pay', cur + (paid >= 100 ? Math.round(paid) : paid.toFixed(2)), cur + price + ' per month · ' + (worth >= 1.1 ? 'worth ' + worth.toFixed(1) + '× that' : 'about what it is worth')) : h('button', { class: 'urtile urset', onclick: () => planPriceDialog() }, h('div', { class: 'small muted' }, 'What do you pay?'), h('div', { class: 'urval' }, 'Set my price'), h('div', { class: 'small muted' }, 'to compare with the value')),
    tile('API value', money(t.cost), 'what it would cost per token'), tile('Tokens used', fmt(t.tokens), fmt(t.out) + ' written · ' + fmt(t.in + t.cw) + ' read fresh'),
    tile('Cached reads', fmt(t.cr), 'saved about ' + money(t.cacheSaved)), tile('Sessions', String(t.sessions), t.activeDays + ' active days'),
    tile('Messages', fmt(t.messages), t.prompts + ' prompts from you'), tile('Longest streak', t.longestStreak + (t.longestStreak === 1 ? ' day' : ' days'), t.currentStreak ? 'now ' + t.currentStreak : 'not today'),
    tile('Favorite model', r.favorite || '–', r.models[0] ? Math.round(r.models[0].cost / Math.max(0.0001, t.cost) * 100) + '% of the cost' : ''), tile('Longest session', urDur(t.longestSessionMs), t.first ? 'since ' + new Date(t.first).toLocaleDateString([], { day: 'numeric', month: 'short', year: 'numeric' }) : '')));
  // cost per day (or per week when the range is long)
  let bars = r.days; if (bars.length > 100) { const w = []; for (let i = 0; i < bars.length; i += 7) { const c = bars.slice(i, i + 7); w.push({ date: c[0].date, cost: c.reduce((a, x) => a + x.cost, 0), tokens: c.reduce((a, x) => a + x.tokens, 0), week: true }); } bars = w; }
  const max = Math.max(0.0001, ...bars.map((d) => d.cost));
  card.append(h('div', { class: 'small muted' }, 'Cost per ' + (bars[0] && bars[0].week ? 'week' : 'day')),
    h('div', { class: 'urbars' }, bars.map((d) => h('div', { class: 'urbar', title: d.date + (d.week ? ' (week)' : '') + ': ' + money(d.cost) + ' · ' + fmt(d.tokens) + ' tokens' }, h('i', { style: 'height:' + Math.max(d.cost ? 3 : 0, Math.round(d.cost / max * 100)) + '%' })))));
  // by model
  const share = (v, tot) => Math.max(2, Math.round(v / Math.max(0.0001, tot) * 100));
  card.append(h('div', { class: 'small muted', style: 'margin-top:6px' }, 'By model'),
    h('div', { class: 'urtable' }, r.models.map((m) => h('div', { class: 'urrow' }, h('div', { class: 'urname' }, m.label), h('div', { class: 'urbarwrap' }, h('i', { style: 'width:' + share(m.cost, t.cost) + '%' })),
      h('div', { class: 'small muted urnum' }, fmt(m.in + m.out + m.cw) + ' tok · ' + fmt(m.cr) + ' cached'), h('b', { class: 'urmoney' }, money(m.cost))))));
  if (r.projects.length) card.append(h('div', { class: 'small muted', style: 'margin-top:6px' }, 'By project'),
    h('div', { class: 'urtable' }, r.projects.slice(0, 6).map((p) => h('div', { class: 'urrow' }, h('div', { class: 'urname ellip', title: p.cwd }, p.cwd.replace(/\/$/, '').split('/').slice(-2).join('/') || p.cwd), h('div', { class: 'urbarwrap' }, h('i', { style: 'width:' + share(p.cost, t.cost) + '%' })),
      h('div', { class: 'small muted urnum' }, fmt(p.tokens) + ' tok · ' + p.sessions + ' chats'), h('b', { class: 'urmoney' }, money(p.cost))))));
  card.append(h('div', { class: 'row' }, h('div', { class: 'grow' }), price ? h('button', { class: 'link', onclick: () => planPriceDialog() }, 'Change my price') : null, info(r.note)));
  return card;
}
