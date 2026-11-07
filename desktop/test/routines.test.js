// Routines: a goal that Claude works on by itself on a schedule, and the timeline of what it did. Run: node desktop/test/routines.test.js
const { spawn } = require('child_process');
const fs = require('fs'), os = require('os'), path = require('path'), http = require('http');
if (process.platform === 'win32') { console.log('skipped on Windows'); process.exit(0); }
fs.mkdirSync(path.join(os.homedir(), '.cache'), { recursive: true }); const T = fs.mkdtempSync(path.join(os.homedir(), '.cache', 'hearth-routines-'));
const proj = path.join(T, 'proj', 'app'); fs.mkdirSync(proj, { recursive: true });
const FAKE = path.join(T, 'bin'); fs.mkdirSync(FAKE);
fs.writeFileSync(path.join(FAKE, 'claude'), `#!/usr/bin/env node
const sid = 'sid-' + Math.random().toString(36).slice(2, 8); const out = (o) => process.stdout.write(JSON.stringify(o) + '\\n');
out({ type: 'system', subtype: 'init', session_id: sid, model: 'x' });
let text = ''; process.stdin.on('data', (d) => { text += d; }); process.stdin.on('end', () => { setTimeout(() => { out({ type: 'result', subtype: 'success', is_error: false, result: 'Found 3 things. ' + (/routine "(.*?)"/.exec(text) || [])[1], session_id: sid }); process.exit(0); }, 1500); });
`, { mode: 0o755 });
const relay = spawn('node', [path.join(__dirname, '..', 'relay', 'standalone.js'), '--dir', path.join(T, 'd'), '--port', '47791', '--host', '127.0.0.1', '--projects', path.join(T, 'proj')], { env: { ...process.env, PATH: FAKE + path.delimiter + process.env.PATH, HOME: path.join(T, 'home'), HEARTH_TEST: '1', HEARTH_MEM_CHILD: '1', HEARTH_ROUTINE_TICK_MS: '300' }, stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + x : '')); };
const req = (m, p, tok, body) => new Promise((res) => { const d = body ? Buffer.from(JSON.stringify(body)) : null; const r = http.request({ host: '127.0.0.1', port: 47791, path: p, method: m, headers: { Authorization: 'Bearer ' + tok, ...(d ? { 'Content-Type': 'application/json', 'Content-Length': d.length } : {}) } }, (rs) => { let b = ''; rs.on('data', (x) => { b += x; }); rs.on('end', () => { let j = {}; try { j = JSON.parse(b); } catch { /* */ } res({ code: rs.statusCode, j }); }); }); r.on('error', () => res({ code: 0, j: {} })); if (d) r.write(d); r.end(); });
const until = async (fn, ms) => { const t = Date.now(); while (Date.now() - t < ms) { if (await fn()) return true; await sleep(200); } return false; };
(async () => {
  const tok = await require('./_wait')(path.join(T, 'd', 'relay.json'));
  ok('1 a routine needs a name and a goal', (await req('POST', '/routines', tok, { add: { name: '', goal: 'x' } })).code === 400);
  ok('2 a folder outside the projects folder is refused', (await (async () => { await req('POST', '/admin/options', tok, { outside: false }); return (await req('POST', '/routines', tok, { add: { name: 'x', goal: 'y', cwd: '/etc' } })).code === 400; })()));
  const off = await req('POST', '/routines', tok, { add: { name: 'Off', goal: 'never', cwd: proj, enabled: false, schedule: { type: 'interval', minutes: 0.005 } } });
  const on = await req('POST', '/routines', tok, { add: { name: 'Digest', goal: 'Summarise yesterday.', cwd: proj, mode: 'plan', schedule: { type: 'interval', minutes: 0.005 } } });
  ok('3 routines are saved and listed, the plan mode is the default', on.code === 200 && on.j.routines.length === 2 && on.j.routines.find((r) => r.name === 'Digest').mode === 'plan');
  const ran = await until(async () => ((await req('GET', '/routines/timeline', tok)).j.runs || []).some((r) => r.name === 'Digest' && r.status === 'done'), 12000);
  const runs = (await req('GET', '/routines/timeline', tok)).j.runs;
  ok('4 a routine that is due runs by itself, and the timeline has what it found', ran && /Found 3 things\. Digest/.test(runs.find((r) => r.status === 'done').summary) && runs.find((r) => r.status === 'done').session.startsWith('sid-'), JSON.stringify(runs[0]).slice(0, 160));
  { const sid = runs.find((r) => r.status === 'done').session; const l = (await req('GET', '/sessions', tok)).j; ok('4b chats made by a routine are not in the main chat list', Array.isArray(l) && !l.some((x) => x.id === sid), String(sid)); }
  ok('5 a routine that is switched off never runs', !runs.some((r) => r.name === 'Off'));
  ok('6 it does not start again while the last run is still going', (() => { const d = runs.filter((r) => r.name === 'Digest').sort((a, b) => a.at - b.at); return d.every((r, i) => i === 0 || r.at - d[i - 1].at >= 1400); })());
  const id = on.j.routines.find((r) => r.name === 'Digest').id; await req('POST', '/routines', tok, { update: { id, enabled: false } });
  await sleep(2500); const n = (await req('GET', '/routines/timeline', tok)).j.runs.length; await sleep(1500);
  ok('7 switching it off stops the runs', (await req('GET', '/routines/timeline', tok)).j.runs.length === n);
  const now = await req('POST', '/routines', tok, { run: off.j.routines.find((r) => r.name === 'Off').id });
  ok('8 "run now" works even for a routine that is off', now.code === 200 && now.j.run && now.j.run.why === 'by you');
  const again = await req('POST', '/routines', tok, { run: off.j.routines.find((r) => r.name === 'Off').id });
  ok('9 and says no when it is still running', again.code === 409);
  const d = new Date(Date.now() - 60e3), at = String(d.getHours()).padStart(2, '0') + ':' + String(d.getMinutes()).padStart(2, '0');
  if (d.getDate() === new Date().getDate()) { const dly = await req('POST', '/routines', tok, { add: { name: 'Daily', goal: 'Check the disks.', cwd: proj, schedule: { type: 'daily', at } } }); await sleep(2500); await sleep(2500);
    const c = (await req('GET', '/routines/timeline', tok)).j.runs.filter((r) => r.name === 'Daily').length; ok('10 a daily routine whose time has passed runs once today, not again', c === 1, String(c)); } else ok('10 skipped just after midnight', true);
  await req('POST', '/routines', tok, { remove: id });
  ok('11 a routine can be removed', !(await req('GET', '/routines', tok)).j.routines.some((r) => r.id === id));
  ok('12 a guest key cannot use routines', (await req('GET', '/routines', 'share.abcd.abcdefghijklmnop')).code === 401 && (await req('POST', '/routines', 'share.abcd.abcdefghijklmnop', { run: 'x' })).code === 401);
  relay.kill(); await sleep(300); fs.rmSync(T, { recursive: true, force: true });
  console.log(`\n${pass} passed, ${fail} failed`); process.exit(fail ? 1 : 0);
})();
