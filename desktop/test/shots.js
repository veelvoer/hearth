// Takes pictures of the desktop screens in a headless browser, with a pretend relay. A tool for looking at the look, not a test.
//   node desktop/test/shots.js --out /tmp/shots [--style m3|calm] [--dark] [--accent #3A86E8] [--screens sessions,home,settings:connection,...]
// Needs playwright-core and a Chromium (PLAYWRIGHT_CORE=/path/to/node_modules/playwright-core).
const fs = require('fs'), path = require('path');
const arg = (k, d) => { const i = process.argv.indexOf('--' + k); return i >= 0 ? (process.argv[i + 1] && !process.argv[i + 1].startsWith('--') ? process.argv[i + 1] : true) : d; };
const OUT = arg('out', '/tmp/hearth-shots'), STYLE = arg('style', 'calm'), DARK = !!arg('dark', false), ACCENT = arg('accent', ''), SCREENS = String(arg('screens', 'sessions,chat,home,settings:connection,settings:look')).split(',');
const core = process.env.PLAYWRIGHT_CORE || '/home/voer/mtbuild/e2e/node_modules/playwright-core';
const { chromium } = require(core);
const R = path.join(__dirname, '..', 'renderer');
fs.mkdirSync(OUT, { recursive: true });
const html = fs.readFileSync(path.join(R, 'index.html'), 'utf8').replace(/<meta http-equiv="Content-Security-Policy"[^>]*>/, '');
const tmp = path.join(R, '.shot.html'); fs.writeFileSync(tmp, html);
const now = Date.now();
const stub = `
const NOW = ${now}, MIN = 60000;
const sessions = [
  { id: 's1', title: 'Fix the login bug on the settings page', cwd: '/home/me/projects/webshop', mtime: Math.floor(NOW - 4 * MIN), live: true, busy: true, running: true, tokens: 182000, cost: 2.4, model: 'claude-sonnet-5-5' },
  { id: 's2', title: 'Plan the trip to Lisbon', cwd: '', mtime: Math.floor(NOW - 3600e3), live: false, busy: false, tokens: 9000, cost: 0.2, model: 'claude-haiku-4-5', kind: 'talk' },
  { id: 's3', title: 'Write release notes for 1.4', cwd: '/home/me/projects/hearth', mtime: Math.floor(NOW - 26 * 3600e3), live: false, busy: false, tokens: 54000, cost: 0.8, model: 'claude-opus-5-5' },
  { id: 's4', title: 'Why is the build slow?', cwd: '/home/me/projects/api', mtime: Math.floor(NOW - 3 * 86400e3), live: false, busy: false, tokens: 31000, cost: 0.5, model: 'claude-sonnet-5-5' },
];
const msgs = [
  { role: 'user', text: 'The login form on the settings page loses what I typed when I switch tabs. Can you fix it?', ts: NOW - 9 * MIN },
  { role: 'assistant', text: "I'll look at how the form keeps its state.\\n\\nThe tabs unmount the form, so the draft is lost. I'll keep the draft in the parent and restore it when the tab opens again.", ts: NOW - 8 * MIN },
  { role: 'tool', text: 'Read src/settings/LoginForm.tsx', name: 'Read', input: { file_path: 'src/settings/LoginForm.tsx' }, result: 'export function LoginForm() {...}', ts: NOW - 8 * MIN },
  { role: 'tool', text: 'Edit src/settings/Settings.tsx', name: 'Edit', input: { file_path: 'src/settings/Settings.tsx', old_string: 'const [tab, setTab] = useState(0)', new_string: 'const [tab, setTab] = useState(0)\\n  const [draft, setDraft] = useState({})' }, result: 'ok', ts: NOW - 7 * MIN },
  { role: 'assistant', text: 'Done. The form now keeps its draft when you switch tabs.\\n\\n- **Settings.tsx** holds the draft\\n- **LoginForm.tsx** reads it on mount\\n\\nWant me to add a test?', ts: NOW - 6 * MIN },
  { role: 'meta', text: '1.2k output · 18k input · 41s', ts: NOW - 6 * MIN },
];
const usage = { session: { pct: 42, resetsAt: NOW + 2 * 3600e3 }, week: { pct: 63, resetsAt: NOW + 3 * 86400e3 }, opus: { pct: 21, resetsAt: NOW + 3 * 86400e3 }, sonnet: { pct: 48, resetsAt: NOW + 3 * 86400e3 }, at: NOW };
const history = Array.from({ length: 48 }, (_, i) => ({ at: NOW - (48 - i) * 1800e3, session: 20 + (i * 7) % 60, week: 30 + i * 0.6 }));
let S = { usage, profile: { email: 'me@example.com', plan: 'Max' }, signedIn: true, history, settings: { interval: 5, theme: 'system', palette: 'claude', style: ${JSON.stringify(STYLE === 'm3' ? 'material' : 'calm')}, background: true, onboarded: true, tourSeen: true, lastSeenVersion: '9.9.9', thresholds: [75, 90, 100] }, error: null, now: NOW };
const machines = [{ id: 'local', name: 'laptop (this computer)', host: '127.0.0.1', port: 47601, local: true, secure: false }, { id: 'vps', name: 'VPS', host: 'hearth.example.com', port: 443, secure: true }];
const report = { range: 30, generated: NOW, totals: { in: 1200000, out: 640000, cr: 9000000, cw: 300000, cost: 38.2, tokens: 2140000, allTokens: 11140000, messages: 912, prompts: 188, sessions: 41, activeDays: 19, longestStreak: 9, currentStreak: 4, first: NOW - 30 * 86400e3, last: NOW, longestSessionMs: 7200e3, avgPerActiveDay: 112000, cacheSaved: 61 }, models: [{ model: 'claude-sonnet-5-5', label: 'Sonnet 5.5', in: 800000, out: 400000, cr: 5000000, cw: 200000, msgs: 500, cost: 20 }, { model: 'claude-opus-5-5', label: 'Opus 5.5', in: 300000, out: 200000, cr: 3000000, cw: 80000, msgs: 200, cost: 15 }], days: Array.from({ length: 30 }, (_, i) => ({ date: new Date(NOW - (29 - i) * 86400e3).toISOString().slice(0, 10), tokens: (i * 37 % 11) * 20000, cost: (i * 13 % 7) })), favorite: 'Sonnet 5.5', projects: [{ cwd: '/home/me/projects/webshop', tokens: 900000, cost: 14, sessions: 12 }], tools: { Edit: 120, Bash: 90, Read: 200 } };
const relay = (mid, method, p0) => { const p = String(p0).split('?')[0];
  if (/^\\/sessions$/.test(p)) return sessions;
  if (/\\/messages/.test(p)) return msgs;
  if (/\\/state/.test(p)) return { running: true, busy: true };
  if (/^\\/usage-report|^\\/report/.test(p)) return report;
  if (/^\\/projects/.test(p)) return [];
  if (/^\\/computers/.test(p)) return [{ id: 'vps', name: 'VPS', online: true, kind: 'server' }];
  if (/^\\/templates/.test(p) || /^\\/memory/.test(p)) return { templates: [], notes: [] };
  if (/^\\/routines/.test(p)) return { routines: [], runs: [] };
  if (/^\\/admin\\/safety/.test(p)) return { snapshots: true, backups: { on: true, list: [] }, budget: { perDay: 10, perChat: 0, usedDay: 1.2 } };
  if (/^\\/admin\\/doctor/.test(p)) return { checks: [] };
  if (/^\\/mcp\\/catalog/.test(p)) return { tools: [] };
  if (/^\\/chat-tags/.test(p)) return {};
  if (/^\\/recap/.test(p)) return null;
  return {};
};
const base = { state: S, machines, relay, version: '1.36.0', accent: null, systemDark: ${DARK}, notesSince: [], stats: { generated: NOW, days: Array.from({ length: 30 }, (_, i) => ({ date: new Date(NOW - (29 - i) * 86400e3).toISOString().slice(0, 10), in: 4000 + (i * 977) % 9000, out: 20000 + (i * 7919) % 90000, cr: 400000 + (i * 31337) % 900000, cw: 20000 + (i * 1237) % 30000, msgs: (i * 3) % 17, sessions: 1 + (i % 4), fam: { sonnet: 60000 + (i * 911) % 40000, opus: 20000 + (i * 313) % 30000 } })), models: { sonnet: 2100000, opus: 800000, haiku: 90000 }, tools: { Edit: 120, Bash: 90, Read: 200, Grep: 60 }, heat: Array.from({ length: 7 }, (_, d) => Array.from({ length: 24 }, (_, h) => (h > 8 && h < 22 ? (d * h) % 6 : 0))), projects: [{ cwd: '/home/me/projects/webshop', tokens: 900000, sessions: 12, last: NOW - 3600e3 }, { cwd: '/home/me/projects/hearth', tokens: 600000, sessions: 8, last: NOW - 86400e3 }] }, pairInfo: { code: '123456', addresses: ['192.168.1.20'] }, setSettings: (patch) => { S = { ...S, settings: { ...S.settings, ...patch } }; return S.settings; }, discover: [], guests: [], setGuests: [], voiceStatus: { stt: false, tts: false }, supportState: { signedIn: false }, pairRequests: [], pendingProjects: [], setupPaths: {}, installHooks: {}, filePath: () => '' };
window.cm = new Proxy({}, { get: (t, k) => {
  if (k === 'on') return () => () => {};
  if (k === 'send') return async () => {};
  if (k === 'setSettings') return async (patch) => base.setSettings(patch);
  if (k === 'state' || k === 'refresh') return async () => S;
  if (k === 'relay') return async (...a) => relay(...a);
  return async (...a) => { const v = base[k]; return typeof v === 'function' ? v(...a) : v === undefined ? {} : v; };
} });
`;
(async () => {
  const browser = await chromium.launch(process.env.CHROME_PATH ? { executablePath: process.env.CHROME_PATH } : {});
  const ctx = await browser.newContext({ viewport: { width: 1180, height: 760 }, colorScheme: DARK ? 'dark' : 'light', deviceScaleFactor: 1 });
  const page = await ctx.newPage();
  const errors = []; page.on('pageerror', (e) => errors.push(String(e.stack || e.message || e).split('\n').slice(0, 3).join(' « ').slice(0, 330)));
  page.on('console', (m) => { if (arg('debug', false) && ['error', 'warning'].includes(m.type())) console.log('console', m.type(), m.text().slice(0, 200)); });
  await page.addInitScript(stub);
  if (ACCENT) await page.addInitScript(`window.addEventListener('DOMContentLoaded', () => {});`);
  await page.goto('file://' + tmp); await page.waitForTimeout(900);
  const click = async (text) => { const b = page.locator('#nav button', { hasText: text }).first(); if (await b.count()) { await b.click(); await page.waitForTimeout(500); } };
  for (const s of SCREENS) {
    const [name, sub] = s.split(':');
    if (name === 'sessions') { await click('Chats'); }
    else if (name === 'chat') { await click('Chats'); const it = page.locator('.side .item').first(); if (await it.count()) { await it.click(); await page.waitForTimeout(700); } }
    else if (name === 'home') { await click('Home'); }
    else if (name === 'history') { await click('History'); }
    else if (name === 'newchat') { await click('Chats'); const b = page.locator('#nav button.newchat').first(); if (await b.count()) { await b.click(); await page.waitForTimeout(600); } }
    else if (name === 'motion') {   // the expressive motion: frames in the middle of each move, and nothing running while idle
      const shot = async (n) => page.screenshot({ path: path.join(OUT, `${STYLE}${DARK ? '-dark' : ''}-motion-${n}.png`) });
      await click('Chats'); await page.waitForTimeout(700);
      const it = page.locator('.side .item').nth(1); const bb = await it.boundingBox();
      if (bb) { await page.mouse.move(bb.x + bb.width * 0.85, bb.y + bb.height * 0.3); await page.waitForTimeout(350); await shot('1-hover');
        await page.mouse.down(); await page.waitForTimeout(160); await shot('2-press');
        await page.mouse.up(); await page.waitForTimeout(70); await shot('3-release'); }
      const nb = page.locator('#nav button.newchat').first(); const nbb = await nb.boundingBox();
      if (nbb) { await page.mouse.move(nbb.x + nbb.width * 0.8, nbb.y + nbb.height * 0.8); await page.mouse.down(); await page.waitForTimeout(150); await shot('4-fab-press'); await page.mouse.move(5, 700); await page.mouse.up(); }
      const home = page.locator('#nav button', { hasText: 'Home' }).first();
      await home.click(); await page.waitForTimeout(90); await shot('5-page-90ms'); await page.waitForTimeout(160); await shot('6-page-250ms'); await page.waitForTimeout(900); await shot('7-page-rest');
      const tb = await page.locator('#nav .tabs button.on .ic').first().boundingBox();
      if (tb) { await page.mouse.move(tb.x + tb.width / 2, tb.y + tb.height / 2); await page.mouse.down(); await page.mouse.move(tb.x + tb.width / 2, tb.y + tb.height / 2 + 50, { steps: 6 }); await page.waitForTimeout(120); await shot('8-drag');
        await page.mouse.move(tb.x + tb.width / 2, tb.y + tb.height / 2 + 75, { steps: 4 }); await page.mouse.up(); await page.waitForTimeout(900); await shot('9-drag-dropped'); }
      await page.mouse.move(600, 740); await page.waitForTimeout(1500);
      const frames = await page.evaluate(() => new Promise((res) => { let n = 0; const o = window.requestAnimationFrame; window.requestAnimationFrame = (f) => { n++; return o(f); }; setTimeout(() => { window.requestAnimationFrame = o; res(n); }, 1500); }));
      console.log('motion: rAF calls while idle for 1.5 s:', frames, '· tab now:', await page.evaluate(() => tab), '· light layers left:', await page.evaluate(() => document.querySelectorAll('.m3light').length));
    }
    else if (name === 'settings') { await click('Settings'); if (sub) { const t = page.locator('.snavitem', { hasText: new RegExp(sub, 'i') }).first(); if (await t.count()) { await t.click(); await page.waitForTimeout(400); } } }
    await page.waitForTimeout(500);
    await page.screenshot({ path: path.join(OUT, `${STYLE}${DARK ? '-dark' : ''}-${name}${sub ? '-' + sub : ''}.png`) });
  }
  if (arg('debug', false)) console.log('cm.machines()', await page.evaluate(async () => JSON.stringify(await cm.machines()).slice(0, 80)), 'cm.guests()', await page.evaluate(async () => JSON.stringify(await cm.guests())));
  if (arg('debug', false)) console.log(await page.evaluate(() => JSON.stringify({ live: Object.fromEntries(Object.entries(SS.live).map(([k, v]) => [k, Array.isArray(v) ? 'array' + v.length : typeof v])), list: Array.isArray(SS.list) ? 'array' : typeof SS.list, machines: Array.isArray(SS.machines) ? SS.machines.length : typeof SS.machines, guests: Array.isArray(SS.guests) ? 'array' : typeof SS.guests })));
  if (errors.length) console.log('page errors:', [...new Set(errors)].slice(0, 6).join(' | '));
  await browser.close(); fs.rmSync(tmp, { force: true });
})().catch((e) => { try { fs.rmSync(tmp, { force: true }); } catch { /* */ } console.error(e); process.exit(1); });
