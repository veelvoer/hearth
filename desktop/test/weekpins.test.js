// Pins belong to one week: after the reset the days are shared evenly again, and the weekday of the reset is split in two. Run: node desktop/test/weekpins.test.js
const { spawn } = require('child_process');
const fs = require('fs'), os = require('os'), path = require('path'), http = require('http');
if (process.platform === 'win32') { console.log('skipped on Windows'); process.exit(0); }
fs.mkdirSync(path.join(os.homedir(), '.cache'), { recursive: true }); const T = fs.mkdtempSync(path.join(os.homedir(), '.cache', 'hearth-pins-'));
for (const d of ['home', 'proj', 'd']) fs.mkdirSync(path.join(T, d), { recursive: true });
const DAY = 86400000, now = Date.now();
// a week that reset 2 hours ago (so the reset weekday has a small first piece and a long last piece)
const end = now - 2 * 3600000 + 7 * DAY;
fs.writeFileSync(path.join(T, 'd', 'usage.json'), JSON.stringify({ week: { pct: 1, resetsAt: end }, session: { pct: 0, resetsAt: now + 4 * 3600000 }, at: now }));
fs.writeFileSync(path.join(T, 'd', 'weekhist.json'), JSON.stringify([{ t: now - 3600000, w: 1, r: end }]));
fs.writeFileSync(path.join(T, 'd', 'weekplan.json'), JSON.stringify({ 5: 94, 6: 0, 1: 0 }));   // pins in the old format (unknown week)
const relay = spawn('node', [path.join(__dirname, '..', 'relay', 'standalone.js'), '--dir', path.join(T, 'd'), '--port', '47794', '--host', '127.0.0.1', '--projects', path.join(T, 'proj')], { env: { ...process.env, HOME: path.join(T, 'home') }, stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + x : '')); };
const req = (method, url, tok, body) => new Promise((res) => { const d = body ? Buffer.from(JSON.stringify(body)) : null; const r = http.request({ host: '127.0.0.1', port: 47794, path: url, method, headers: { Authorization: 'Bearer ' + tok, ...(d ? { 'Content-Type': 'application/json', 'Content-Length': d.length } : {}) } }, (rs) => { let b = ''; rs.on('data', (c) => { b += c; }); rs.on('end', () => { let j = {}; try { j = JSON.parse(b); } catch { /* */ } res({ code: rs.statusCode, j }); }); }); r.on('error', () => res({ code: 0, j: {} })); if (d) r.write(d); r.end(); });
(async () => {
  await sleep(2000); const tok = JSON.parse(fs.readFileSync(path.join(T, 'd', 'relay.json'))).token;
  const a = await req('GET', '/weekplan?tz=0', tok);
  ok('1 pins of an unknown (old) week are dropped', a.code === 200 && Object.keys(a.j.targets).length === 0, JSON.stringify(a.j.targets));
  const days = a.j.days || [], sum = days.reduce((x, d) => x + d.planned, 0);
  ok('2 the reset weekday is two days: a small one at the start and one at the end', days.length === 8 && days[0].start === true && days[0].wd === days[7].wd && !days[7].start, days.map((d) => d.name + (d.start ? '(start)' : '')).join(' '));
  ok('3 together the days add up to 100%', Math.abs(sum - 100) < 0.6, 'sum ' + sum.toFixed(1));
  ok('4 the end of the week is not a huge block: every open day is about the same', days.slice(2).every((d) => d.planned < 30), days.map((d) => d.planned).join(' '));
  const lastWd = days[7].wd, p = await req('POST', '/weekplan', tok, { day: lastWd, pct: 30, tz: 0 });
  ok('5 pinning the last piece works and gets its full share', p.code === 200 && p.j.days[7].fixed === true && Math.abs(p.j.days[7].planned - 30) < 0.6 && p.j.days[0].fixed === false, p.code + ' ' + (p.j.days ? p.j.days[7].planned : ''));
  ok('6 the pin is kept for this week', (await req('GET', '/weekplan?tz=0', tok)).j.targets[lastWd] === 30);
  relay.kill(); await sleep(400);
  // the week resets: a new end → the pin is gone
  const u = JSON.parse(fs.readFileSync(path.join(T, 'd', 'usage.json'))); u.week.resetsAt += 7 * DAY; fs.writeFileSync(path.join(T, 'd', 'usage.json'), JSON.stringify(u));
  const relay2 = spawn('node', [path.join(__dirname, '..', 'relay', 'standalone.js'), '--dir', path.join(T, 'd'), '--port', '47794', '--host', '127.0.0.1', '--projects', path.join(T, 'proj')], { env: { ...process.env, HOME: path.join(T, 'home') }, stdio: 'ignore' }); await sleep(2000);
  ok('7 next week starts without the pins', Object.keys((await req('GET', '/weekplan?tz=0', tok)).j.targets || { x: 1 }).length === 0);
  relay2.kill(); await sleep(300); fs.rmSync(T, { recursive: true, force: true });
  console.log(`\n${pass} passed, ${fail} failed`); process.exit(fail ? 1 : 0);
})();
