// One odd chat file must never keep the other chats from loading. Run: node desktop/test/odd-chats.test.js
const { spawn } = require('child_process');
const fs = require('fs'), os = require('os'), path = require('path'), http = require('http');
if (process.platform === 'win32') { console.log('skipped on Windows'); process.exit(0); }
fs.mkdirSync(path.join(os.homedir(), '.cache'), { recursive: true }); const T = fs.mkdtempSync(path.join(os.homedir(), '.cache', 'hearth-odd-'));
const proj = path.join(T, 'proj', 'a'), P = path.join(T, 'home', '.claude', 'projects');
fs.mkdirSync(proj, { recursive: true }); for (const d of ['-a', '-b', '-c']) fs.mkdirSync(path.join(P, d), { recursive: true });
const line = (o) => JSON.stringify(o) + '\n';
fs.writeFileSync(path.join(P, '-a', 'good.jsonl'), line({ type: 'user', cwd: proj, timestamp: '2026-10-19T10:00:00Z', message: { content: 'a good chat' } }));
fs.writeFileSync(path.join(P, '-a', 'empty.jsonl'), '');
fs.writeFileSync(path.join(P, '-a', 'binary.jsonl'), require('crypto').randomBytes(3000));
fs.writeFileSync(path.join(P, '-b', 'numcwd.jsonl'), line({ type: 'user', cwd: 5, timestamp: 'nope', message: { content: null } }) + line({ type: 'user', cwd: proj, message: { content: 'chat with a number as folder' } }) + 'not json\n[]\nnull\n');
fs.mkdirSync(path.join(P, '-c', 'dir.jsonl')); fs.symlinkSync('/nonexistent', path.join(P, '-c', 'dead.jsonl'));
// a huge chat (bigger than one look reads) must not stall the list
{ const row = line({ type: 'assistant', cwd: proj, timestamp: '2026-10-19T10:00:01Z', message: { id: 'm1', model: 'claude-opus-4', role: 'assistant', usage: { input_tokens: 10, output_tokens: 20 }, content: [{ type: 'text', text: 'x'.repeat(600) }] } });
  const fd = fs.openSync(path.join(P, '-a', 'huge.jsonl'), 'w'); fs.writeSync(fd, line({ type: 'user', cwd: proj, timestamp: '2026-10-19T09:00:00Z', message: { content: 'a huge chat' } })); for (let b = 0; b < 20; b++) { let blk = ''; for (let i = 0; i < 10000; i++) blk += row.replace('"m1"', '"m' + b + '_' + i + '"'); fs.writeSync(fd, blk); } fs.closeSync(fd); }   // about 130 MB
const relay = spawn('node', [path.join(__dirname, '..', 'relay', 'standalone.js'), '--dir', path.join(T, 'd'), '--port', '47798', '--host', '127.0.0.1', '--projects', path.join(T, 'proj')], { env: { ...process.env, HOME: path.join(T, 'home') }, stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + x : '')); };
const get = (url, tok) => new Promise((res) => http.get({ host: '127.0.0.1', port: 47798, path: url, headers: { Authorization: 'Bearer ' + tok } }, (rs) => { let b = ''; rs.on('data', (c) => { b += c; }); rs.on('end', () => res({ code: rs.statusCode, body: b })); }).on('error', () => res({ code: 0, body: '' })));
(async () => {
  await sleep(2000); const tok = JSON.parse(fs.readFileSync(path.join(T, 'd', 'relay.json'))).token;
  const r = await get('/sessions', tok); let list = []; try { list = JSON.parse(r.body); } catch { /* */ }
  ok('1 the list loads', r.code === 200 && Array.isArray(list), 'code ' + r.code);
  ok('2 the good chat and the one with a number as folder are both there', list.some((x) => x.title === 'a good chat') && list.some((x) => x.title === 'chat with a number as folder'), list.map((x) => x.title).join(' | '));
  const m = await get('/sessions/good/messages', tok);
  ok('3 messages of the good chat load', m.code === 200 && /a good chat/.test(m.body));
  const t0 = Date.now(), r1 = await get('/sessions', tok), ms = Date.now() - t0; let l1 = []; try { l1 = JSON.parse(r1.body); } catch { /* */ }
  const big1 = (l1.find((x) => x.title === 'a huge chat') || {}).tokens || 0;
  ok('4 a 130 MB chat does not stall the list', r1.code === 200 && ms < 3000 && big1 >= 0, ms + ' ms');
  await get('/sessions', tok); const l2 = JSON.parse((await get('/sessions', tok)).body); const big2 = (l2.find((x) => x.title === 'a huge chat') || {}).tokens || 0;
  ok('5 its totals fill in over a few looks', big2 > big1);
  relay.kill(); await sleep(300); fs.rmSync(T, { recursive: true, force: true });
  console.log(`\n${pass} passed, ${fail} failed`); process.exit(fail ? 1 : 0);
})();
