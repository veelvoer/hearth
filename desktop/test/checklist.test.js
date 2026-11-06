// The owner's checklist of reports. Run: node desktop/test/checklist.test.js
const { spawn } = require('child_process');
const fs = require('fs'), os = require('os'), path = require('path'), http = require('http');
if (process.platform === 'win32') { console.log('skipped on Windows'); process.exit(0); }
fs.mkdirSync(path.join(os.homedir(), '.cache'), { recursive: true }); const T = fs.mkdtempSync(path.join(os.homedir(), '.cache', 'hearth-tag-'));
const proj = path.join(T, 'proj', 'app'), P = path.join(T, 'home', '.claude', 'projects', proj.replace(/[^A-Za-z0-9]/g, '-')); fs.mkdirSync(proj, { recursive: true }); fs.mkdirSync(P, { recursive: true });
const mk = (id, user, reply) => fs.writeFileSync(path.join(P, id + '.jsonl'), [JSON.stringify({ type: 'user', cwd: proj, timestamp: '2026-10-20T10:00:00Z', message: { content: user } }), JSON.stringify({ type: 'assistant', cwd: proj, timestamp: '2026-10-20T10:00:05Z', message: { role: 'assistant', content: [{ type: 'text', text: reply }] } })].join('\n') + '\n');
mk('one', 'fix the login page', 'I changed the Pineapple handler in auth.js'); mk('two', 'write a poem', 'Roses are red');
const FAKE = path.join(T, 'fakebin'); fs.mkdirSync(FAKE, { recursive: true }); fs.writeFileSync(path.join(FAKE, 'claude'), '#!/bin/sh\nexit 1\n', { mode: 0o755 });
const relay = spawn('node', [path.join(__dirname, '..', 'relay', 'standalone.js'), '--dir', path.join(T, 'd'), '--port', '47792', '--host', '127.0.0.1', '--projects', path.join(T, 'proj')], { env: { ...process.env, PATH: FAKE + path.delimiter + process.env.PATH, HOME: path.join(T, 'home'), HEARTH_TEST: '1', HEARTH_MEM_CHILD: '1' }, stdio: 'ignore' });
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
  const inc = await req('POST', '/support/incoming', tok, { kind: 'new', id: 'HRT-4242', title: 'With a picture', category: 'bug', from: 'a@x.nl', text: 'See the screenshot', files: [{ name: 'shot one.jpg', type: 'image/jpeg', data: Buffer.from('JPEGDATA').toString('base64') }, { name: '../evil.sh', data: Buffer.from('x').toString('base64') }] });
  const fd = path.join(T, 'd', 'support-files', 'HRT-4242'), got = fs.existsSync(fd) ? fs.readdirSync(fd) : [];
  ok('9 pictures that come with a support mail are kept for Claude', inc.code === 200 && got.length === 2 && got.some((n) => /shot one\.jpg$/.test(n)) && !got.some((n) => /\.\./.test(n)), got.join());
  fs.writeFileSync(path.join(T, 'd', 'pending-runs.json'), JSON.stringify({ 'lost-run-1': { cwd: proj, text: 'make me a license plate counter web app', at: Date.now() - 60000 } }));
  const lst = (await req('GET', '/sessions', tok)).j, lostItem = lst.find((x) => x.id === 'lost-run-1');
  ok('10 a run that never reached the disk still shows in the list, marked as interrupted', !!lostItem && lostItem.interrupted === true && /license plate/.test(lostItem.title));
  const lm = (await req('GET', '/sessions/lost-run-1/messages', tok)).j;
  ok('11 opening it shows the prompt and says what happened', Array.isArray(lm) && lm[0].role === 'user' && /license plate/.test(lm[0].text) && /interrupted/.test(lm[1].text));
  ok('12 deleting it works and it does not come back', (await req('POST', '/sessions/lost-run-1/delete', tok, {})).code === 200 && !(await req('GET', '/sessions', tok)).j.some((x) => x.id === 'lost-run-1'));
  // a Claude that quits before it reads the prompt must not take the server down
  await req('POST', '/sessions/new/send', tok, { text: 'x'.repeat(400000), mode: 'acceptEdits', cwd: proj });
  await sleep(800);
  ok('13 the server is still up after a run that died at once', (await req('GET', '/health', tok)).code === 200 && (await req('GET', '/chat-tags', tok)).code !== 0);
  relay.kill(); await sleep(300); fs.rmSync(T, { recursive: true, force: true });
  console.log(`\n${pass} passed, ${fail} failed`); process.exit(fail ? 1 : 0);
})();
