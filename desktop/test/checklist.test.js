// The owner's checklist of reports. Run: node desktop/test/checklist.test.js
const { spawn } = require('child_process');
const fs = require('fs'), os = require('os'), path = require('path'), http = require('http');
if (process.platform === 'win32') { console.log('skipped on Windows'); process.exit(0); }
fs.mkdirSync(path.join(os.homedir(), '.cache'), { recursive: true }); const T = fs.mkdtempSync(path.join(os.homedir(), '.cache', 'hearth-tag-'));
const proj = path.join(T, 'proj', 'app'), P = path.join(T, 'home', '.claude', 'projects', proj.replace(/[^A-Za-z0-9]/g, '-')); fs.mkdirSync(proj, { recursive: true }); fs.mkdirSync(P, { recursive: true });
const mk = (id, user, reply) => fs.writeFileSync(path.join(P, id + '.jsonl'), [JSON.stringify({ type: 'user', cwd: proj, timestamp: '2026-10-20T10:00:00Z', message: { content: user } }), JSON.stringify({ type: 'assistant', cwd: proj, timestamp: '2026-10-20T10:00:05Z', message: { role: 'assistant', content: [{ type: 'text', text: reply }] } })].join('\n') + '\n');
mk('one', 'fix the login page', 'I changed the Pineapple handler in auth.js'); mk('two', 'write a poem', 'Roses are red');
const relay = spawn('node', [path.join(__dirname, '..', 'relay', 'standalone.js'), '--dir', path.join(T, 'd'), '--port', '47792', '--host', '127.0.0.1', '--projects', path.join(T, 'proj')], { env: { ...process.env, HOME: path.join(T, 'home'), HEARTH_TEST: '1', HEARTH_MEM_CHILD: '1' }, stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + x : '')); };
const req = (method, url, tok, body) => new Promise((res) => { const d = body ? Buffer.from(JSON.stringify(body)) : null; const r = http.request({ host: '127.0.0.1', port: 47792, path: url, method, headers: { Authorization: 'Bearer ' + tok, ...(d ? { 'Content-Type': 'application/json', 'Content-Length': d.length } : {}) } }, (rs) => { let b = ''; rs.on('data', (x) => { b += x; }); rs.on('end', () => { let j = {}; try { j = JSON.parse(b); } catch { /* */ } res({ code: rs.statusCode, j }); }); }); r.on('error', () => res({ code: 0, j: {} })); if (d) r.write(d); r.end(); });
(async () => {
  let tok = ''; for (let i = 0; i < 40 && !tok; i++) { await sleep(400); try { tok = JSON.parse(fs.readFileSync(path.join(T, 'd', 'relay.json'))).token; } catch { /* not yet */ } }
  const ck = (b) => req('POST', '/support/checklist', tok, b);
  const now = Date.now();
  let r = await ck({ sync: [{ id: 'HRT-1', title: 'Crash', email: 'a@x.nl', last: 'It crashes', updatedAt: now, from: 'user' }, { id: 'HRT-2', title: 'Idea', email: 'b@x.nl', last: 'More icons', updatedAt: now - 5, from: 'user' }, { id: 'HRT-3', title: 'Old', email: 'c@x.nl', last: 'Good news', updatedAt: now - 9, from: 'support', fixed: true }] });
  ok('1 open reports become items, fixed ones are not added', r.j.items.map((x) => x.id).join() === 'HRT-1,HRT-2' && r.j.items.every((x) => !x.done), JSON.stringify(r.j).slice(0, 120));
  r = await ck({ id: 'HRT-1', done: true });
  ok('2 ticking moves an item to done, below the open ones', r.j.items.map((x) => x.id + (x.done ? '!' : '')).join() === 'HRT-2,HRT-1!');
  r = await ck({ id: 'HRT-1', done: false });
  ok('3 unticking brings it back', r.j.items.every((x) => !x.done));
  r = await ck({ sync: [{ id: 'HRT-2', title: 'Idea', email: 'b@x.nl', last: 'Good news: done', updatedAt: now + 10, from: 'support', fixed: true }] });
  ok('4 a report the desk says is fixed gets ticked', r.j.items.find((x) => x.id === 'HRT-2').done === true);
  r = await ck({ sync: [{ id: 'HRT-2', title: 'Idea', email: 'b@x.nl', last: 'Still wrong', updatedAt: now + 5000, from: 'user' }] });
  ok('5 a new message from the person after it was ticked opens it again', r.j.items.find((x) => x.id === 'HRT-2').done === false);
  await ck({ id: 'HRT-1', done: true });
  r = await ck({ remove: 'done' });
  ok('6 "remove done" removes only the done ones', r.j.items.map((x) => x.id).join() === 'HRT-2');
  r = await ck({ remove: 'HRT-2' });
  ok('7 one item can be removed', r.j.items.length === 0);
  ok('8 a guest key cannot touch the checklist', (await req('POST', '/support/checklist', 'share.abcd.abcdefghijklmnop', { remove: 'done' })).code === 401);
  relay.kill(); await sleep(300); fs.rmSync(T, { recursive: true, force: true });
  console.log(`\n${pass} passed, ${fail} failed`); process.exit(fail ? 1 : 0);
})();
