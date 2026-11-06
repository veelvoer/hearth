// The safety features through the server: a snapshot before a run and undo, the budget, backups, and the secrets check. Run: node desktop/test/safety-server.test.js
const { spawn } = require('child_process');
const fs = require('fs'), os = require('os'), path = require('path'), http = require('http');
if (process.platform === 'win32') { console.log('skipped on Windows'); process.exit(0); }
fs.mkdirSync(path.join(os.homedir(), '.cache'), { recursive: true }); const T = fs.mkdtempSync(path.join(os.homedir(), '.cache', 'hearth-safe-'));
const proj = path.join(T, 'proj', 'app'); fs.mkdirSync(proj, { recursive: true });
fs.writeFileSync(path.join(proj, 'a.txt'), 'original a'); fs.writeFileSync(path.join(proj, 'c.txt'), 'original c'); fs.writeFileSync(path.join(proj, 'key.env'), 'K=AKIAABCDEFGHIJKLMNOP');
const FAKE = path.join(T, 'bin'); fs.mkdirSync(FAKE);
// a Claude that changes the project: edits a file, makes one, deletes one, and writes its chat
fs.writeFileSync(path.join(FAKE, 'claude'), `#!/usr/bin/env node
const fs = require('fs'), path = require('path'); const cwd = process.cwd(), sid = 'sid-' + Math.random().toString(36).slice(2, 8);
const dir = path.join(process.env.HOME, '.claude', 'projects', cwd.replace(/[^A-Za-z0-9]/g, '-')); fs.mkdirSync(dir, { recursive: true });
fs.writeFileSync(path.join(cwd, 'a.txt'), 'CHANGED'); fs.writeFileSync(path.join(cwd, 'b.txt'), 'new'); try { fs.unlinkSync(path.join(cwd, 'c.txt')); } catch {}
fs.writeFileSync(path.join(dir, sid + '.jsonl'), JSON.stringify({ type: 'user', cwd, sessionId: sid, timestamp: new Date().toISOString(), message: { role: 'user', content: 'do it' } }) + '\\n');
const out = (o) => process.stdout.write(JSON.stringify(o) + '\\n'); out({ type: 'system', subtype: 'init', session_id: sid, model: 'x' }); out({ type: 'result', subtype: 'success', is_error: false, result: 'ok', session_id: sid });
process.stdin.resume(); process.stdin.on('data', () => {}); setTimeout(() => process.exit(0), 200);
`, { mode: 0o755 });
const relay = spawn('node', [path.join(__dirname, '..', 'relay', 'standalone.js'), '--dir', path.join(T, 'd'), '--port', '47794', '--host', '127.0.0.1', '--projects', path.join(T, 'proj')], { env: { ...process.env, PATH: FAKE + path.delimiter + process.env.PATH, HOME: path.join(T, 'home'), HEARTH_TEST: '1', HEARTH_MEM_CHILD: '1' }, stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + x : '')); };
const req = (m, p, tok, body) => new Promise((res) => { const d = body ? Buffer.from(JSON.stringify(body)) : null; const r = http.request({ host: '127.0.0.1', port: 47794, path: p, method: m, headers: { Authorization: 'Bearer ' + tok, ...(d ? { 'Content-Type': 'application/json', 'Content-Length': d.length } : {}) } }, (rs) => { let b = ''; rs.on('data', (x) => { b += x; }); rs.on('end', () => { let j = {}; try { j = JSON.parse(b); } catch { /* ndjson */ } res({ code: rs.statusCode, j, raw: b }); }); }); r.on('error', () => res({ code: 0, j: {}, raw: '' })); if (d) r.write(d); r.end(); });
(async () => {
  const tok = await require('./_wait')(path.join(T, 'd', 'relay.json'));
  const run = await req('POST', '/sessions/new/send', tok, { text: 'change things', mode: 'acceptEdits', cwd: proj }); await sleep(900);
  const sid = (/"id":"(sid-[a-z0-9]+)"/.exec(run.raw) || [])[1];
  ok('1 the run changed the project', fs.readFileSync(path.join(proj, 'a.txt'), 'utf8') === 'CHANGED' && fs.existsSync(path.join(proj, 'b.txt')) && !fs.existsSync(path.join(proj, 'c.txt')), String(sid));
  const snaps = (await req('GET', `/sessions/${sid}/snapshots`, tok)).j;
  ok('2 a snapshot was taken before the run', Array.isArray(snaps) && snaps.length >= 1 && snaps[0].label === 'before a run' && snaps[0].text === 'change things', JSON.stringify(snaps).slice(0, 120));
  const un = await req('POST', `/sessions/${sid}/undo`, tok, {});
  ok('3 undo puts everything back', un.code === 200 && fs.readFileSync(path.join(proj, 'a.txt'), 'utf8') === 'original a' && fs.existsSync(path.join(proj, 'c.txt')) && !fs.existsSync(path.join(proj, 'b.txt')), JSON.stringify(un.j).slice(0, 100));
  ok('4 a guest key cannot undo or read snapshots', (await req('POST', `/sessions/${sid}/undo`, 'share.abcd.abcdefghijklmnop', {})).code === 401);
  ok('5 undo without any snapshot says so', (await req('POST', '/sessions/nope/undo', tok, {})).code === 404);
  // snapshots can be switched off
  await req('POST', '/admin/safety', tok, { snapshots: false }); const before = (await req('GET', `/sessions/${sid}/snapshots`, tok)).j.length;
  await req('POST', '/sessions/new/send', tok, { text: 'again', mode: 'acceptEdits', cwd: proj }); await sleep(900);
  ok('6 with snapshots off no new snapshot is made', (await req('GET', `/sessions/${sid}/snapshots`, tok)).j.length === before);
  await req('POST', '/admin/safety', tok, { snapshots: true });
  // budget: a chat that already cost a lot today
  const home = path.join(T, 'home', '.claude', 'projects', '-big'); fs.mkdirSync(home, { recursive: true });
  fs.writeFileSync(path.join(home, 'big.jsonl'), JSON.stringify({ type: 'assistant', cwd: '/big', sessionId: 'big', timestamp: new Date().toISOString(), message: { id: 'm1', role: 'assistant', model: 'claude-opus-4-1', usage: { input_tokens: 1000, output_tokens: 2000000 }, content: [{ type: 'text', text: 'x' }] } }) + '\n');
  const set = await req('POST', '/admin/safety', tok, { budget: { perDay: 1, perChat: 0 } });
  ok('7 the budget is saved, with what was used today', set.code === 200 && set.j.budget.perDay === 1 && set.j.budget.usedDay > 1, JSON.stringify(set.j.budget));
  const blocked = await req('POST', '/sessions/new/send', tok, { text: 'more', mode: 'acceptEdits', cwd: proj });
  ok('8 a run is refused once the daily budget is used up, with a clear message', blocked.code === 429 && /daily budget/.test(blocked.j.error || ''), blocked.raw.slice(0, 120));
  await req('POST', '/admin/safety', tok, { budget: { perDay: 0, perChat: 0 } });
  ok('9 with the budget off runs are allowed again', (await req('POST', '/sessions/new/send', tok, { text: 'fine', mode: 'acceptEdits', cwd: proj })).code === 200); await sleep(700);
  // backups
  const bk = await req('POST', '/admin/safety', tok, { backupNow: true });
  ok('10 a backup of the chats can be made on request', bk.code === 200 && bk.j.ok && bk.j.backups.list.length === 1, JSON.stringify(bk.j).slice(0, 100));
  const gone = path.join(T, 'home', '.claude', 'projects', '-big', 'big.jsonl'); fs.unlinkSync(gone);
  const rs = await req('POST', '/admin/safety', tok, { restoreBackup: bk.j.name });
  ok('11 and restored', rs.code === 200 && fs.existsSync(gone));
  ok('12 a strange backup name is refused', (await req('POST', '/admin/safety', tok, { restoreBackup: '../../x' })).code === 400);
  const dr = await req('GET', '/admin/doctor', tok);
  ok('13 the health check warns about a secret in a project, without showing it', dr.j.checks.some((c) => c.name === 'Secrets in projects' && c.ok === false && /key\.env/.test(c.text) && !/AKIAABC/.test(JSON.stringify(c))));
  relay.kill(); await sleep(300); fs.rmSync(T, { recursive: true, force: true });
  console.log(`\n${pass} passed, ${fail} failed`); process.exit(fail ? 1 : 0);
})();
