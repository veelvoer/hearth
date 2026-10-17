// Usage limit: Hearth asks whether to continue when usage is back, or continues by itself when "Continue by itself" is on. Fake "claude". Run: node desktop/test/limit.test.js
const { spawn } = require('child_process');
const fs = require('fs'), os = require('os'), path = require('path'), http = require('http');
if (process.platform === 'win32') { console.log('skipped on Windows'); process.exit(0); }
fs.mkdirSync(path.join(os.homedir(), '.cache'), { recursive: true }); const T = fs.mkdtempSync(path.join(os.homedir(), '.cache', 'hearth-limit-'));
for (const d of ['bin', 'home', 'proj', 'proj/app']) fs.mkdirSync(path.join(T, d), { recursive: true });
fs.writeFileSync(path.join(T, 'bin', 'claude'), `#!/usr/bin/env node
const fs = require('fs'), path = require('path');
let text = ''; process.stdin.on('data', (d) => { text += d; }); process.stdin.on('end', () => {
  const cnt = path.join(process.env.HOME, 'count'); let n = 0; try { n = Number(fs.readFileSync(cnt, 'utf8')); } catch {} fs.writeFileSync(cnt, String(n + 1));
  const a = process.argv, r = a.indexOf('--resume'), sid = r >= 0 ? a[r + 1] : 'fake-limit-' + process.pid;
  const pd = path.join(process.env.HOME, '.claude', 'projects', process.cwd().replace(/[^A-Za-z0-9]/g, '-')); fs.mkdirSync(pd, { recursive: true });
  fs.writeFileSync(path.join(pd, sid + '.jsonl'), JSON.stringify({ type: 'user', cwd: process.cwd(), timestamp: new Date().toISOString(), message: { content: 'x ' + text.trim() } }) + '\\n');
  console.log(JSON.stringify({ type: 'system', subtype: 'init', session_id: sid, model: 'fake' }));
  if (n % 2 === 0) {
    console.log(JSON.stringify({ type: 'rate_limit_event', rate_limit_info: { status: 'rejected', rateLimitType: 'five_hour', utilization: 1, resetsAt: Math.floor(Date.now() / 1000) + 1 } }));
    console.log(JSON.stringify({ type: 'result', is_error: true, result: "You've hit your limit \\u00b7 resets soon" }));
  } else {
    console.log(JSON.stringify({ type: 'stream_event', event: { delta: { type: 'text_delta', text: 'RESUMED ' + text.trim() } } }));
    console.log(JSON.stringify({ type: 'result', is_error: false, result: 'done' }));
  }
});
`, { mode: 0o755 });
const env = { ...process.env, HOME: path.join(T, 'home'), PATH: path.join(T, 'bin') + path.delimiter + process.env.PATH, HEARTH_LIMIT_BUFFER_MS: '300' };
const relay = spawn('node', [path.join(__dirname, '..', 'relay', 'standalone.js'), '--dir', path.join(T, 'd'), '--port', '47796', '--host', '127.0.0.1', '--projects', path.join(T, 'proj')], { env, stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + x : '')); };
const req = (method, url, tok, body, onData) => new Promise((res) => { const d = body ? Buffer.from(JSON.stringify(body)) : null; const r = http.request({ host: '127.0.0.1', port: 47796, path: url, method, headers: { Authorization: 'Bearer ' + tok, ...(d ? { 'Content-Type': 'application/json', 'Content-Length': d.length } : {}) } }, (rs) => { let b = ''; rs.on('data', (c) => { b += c; if (onData) onData(b); }); rs.on('end', () => res({ code: rs.statusCode, body: b })); }); r.on('error', () => res({ code: 0, body: '' })); if (d) r.write(d); r.end(); });
const J = (r) => { try { return JSON.parse(r.body); } catch { return {}; } };
const evs = (b) => b.split('\n\n').filter((x) => x.startsWith('id: ')).map((x) => x.split('\n').find((l) => l.startsWith('data: '))).filter(Boolean).map((l) => JSON.parse(l.slice(6)));
const count = () => { try { return Number(fs.readFileSync(path.join(T, 'home', 'count'), 'utf8')); } catch { return 0; } };
(async () => {
  await sleep(2000); const tok = JSON.parse(fs.readFileSync(path.join(T, 'd', 'relay.json'))).token;
  let events = ''; req('GET', '/events', tok, null, (b) => { events = b; });
  const proj = path.join(T, 'proj', 'app');
  const o = await req('POST', '/sessions/new/send', tok, { text: 'build it', cwd: proj });
  await sleep(800);
  const lim = evs(events).filter((e) => e.kind === 'limit');
  ok('1 a limit card with a request id is published', lim.length === 1 && !!lim[0].req && !lim[0].auto, JSON.stringify(lim[0] || {}).slice(0, 120));
  ok('2 no generic error notification for it', !evs(events).some((e) => e.kind === 'error'));
  await sleep(1500); ok('3 nothing resumes before you choose', count() === 1);
  await req('POST', '/decision', tok, { req: lim[0].req, behavior: 'skip' }); await sleep(1500);
  ok('4 "no" leaves it alone', count() === 1);
  // again, and say yes
  const sid = lim[0].session;
  await req('POST', `/sessions/${sid}/send`, tok, { text: 'again', cwd: proj }); await sleep(300);   // count 2 (odd -> resumes fine)
  ok('5 a normal send still works', count() === 2);
  await req('POST', `/sessions/${sid}/send`, tok, { text: 'limit again', cwd: proj }); await sleep(800);
  const lim2 = evs(events).filter((e) => e.kind === 'limit' && e.req !== lim[0].req);
  ok('6 hit again: a new card', lim2.length === 1);
  await req('POST', '/decision', tok, { req: lim2[0].req, behavior: 'continue' }); await sleep(2500);
  ok('7 yes: it continues by itself after the reset', count() === 4, 'count=' + count());
  // auto continue
  const opt = await req('POST', '/admin/options', tok, { autoContinue: true });
  ok('8 the setting is saved and shown', J(opt).autoContinue === true && J(await req('GET', '/admin/options', tok)).autoContinue === true);
  await req('POST', `/sessions/${sid}/send`, tok, { text: 'limit three', cwd: proj }); await sleep(3000);
  const auto = evs(events).filter((e) => e.kind === 'limit' && e.auto);
  ok('9 auto: a heads-up without a question, and it continued', auto.length === 1 && !auto[0].req && count() === 6, 'count=' + count());
  relay.kill(); await sleep(300); fs.rmSync(T, { recursive: true, force: true });
  console.log(`\n${pass} passed, ${fail} failed`); process.exit(fail ? 1 : 0);
})();
