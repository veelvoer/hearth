// Deleting a chat: it goes to a trash folder, disappears from the list, can be restored; nothing else is touched. Run: node desktop/test/delete-chats.test.js
const { spawn } = require('child_process');
const fs = require('fs'), os = require('os'), path = require('path'), http = require('http');
if (process.platform === 'win32') { console.log('skipped on Windows'); process.exit(0); }
fs.mkdirSync(path.join(os.homedir(), '.cache'), { recursive: true }); const T = fs.mkdtempSync(path.join(os.homedir(), '.cache', 'hearth-del-'));
const proj = path.join(T, 'proj', 'app'), P = path.join(T, 'home', '.claude', 'projects', path.join(T, 'proj', 'app').replace(/[^A-Za-z0-9]/g, '-')); fs.mkdirSync(proj, { recursive: true }); fs.mkdirSync(P, { recursive: true });
const mk = (id, text) => fs.writeFileSync(path.join(P, id + '.jsonl'), JSON.stringify({ type: 'user', cwd: proj, timestamp: '2026-10-20T10:00:00Z', message: { content: text } }) + '\n');
mk('keep', 'keep me'); mk('gone', 'delete me'); mk('gone2', 'delete me too');
const relay = spawn('node', [path.join(__dirname, '..', 'relay', 'standalone.js'), '--dir', path.join(T, 'd'), '--port', '47791', '--host', '127.0.0.1', '--projects', path.join(T, 'proj')], { env: { ...process.env, HOME: path.join(T, 'home') }, stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + x : '')); };
const req = (method, url, tok, body) => new Promise((res) => { const d = body ? Buffer.from(JSON.stringify(body)) : null; const r = http.request({ host: '127.0.0.1', port: 47791, path: url, method, headers: { Authorization: 'Bearer ' + tok, ...(d ? { 'Content-Type': 'application/json', 'Content-Length': d.length } : {}) } }, (rs) => { let b = ''; rs.on('data', (x) => { b += x; }); rs.on('end', () => { let j = {}; try { j = JSON.parse(b); } catch { /* */ } res({ code: rs.statusCode, j }); }); }); r.on('error', () => res({ code: 0, j: {} })); if (d) r.write(d); r.end(); });
(async () => {
  await sleep(2000); const tok = JSON.parse(fs.readFileSync(path.join(T, 'd', 'relay.json'))).token;
  const ids = async () => (await req('GET', '/sessions', tok)).j.map((x) => x.id).sort().join();
  ok('1 three chats to begin with', await ids() === 'gone,gone2,keep');
  ok('2 a guest key cannot delete', (await req('POST', '/sessions/gone/delete', 'share.abcd.abcdefghijklmnop', {})).code === 401);
  ok('3 deleting works', (await req('POST', '/sessions/gone/delete', tok, {})).code === 200 && await ids() === 'gone2,keep');
  ok('4 the file is in the trash, not erased', fs.readdirSync(path.join(T, 'home', '.claude', 'projects', '.hearth-trash')).some((n) => n.endsWith('__gone.jsonl')) && !fs.existsSync(path.join(P, 'gone.jsonl')));
  ok('5 the trash folder is not shown as a project or chat', !(await req('GET', '/sessions', tok)).j.some((x) => x.id === 'gone'));
  ok('6 a second delete says it is gone', (await req('POST', '/sessions/gone/delete', tok, {})).code === 404);
  ok('7 undo brings it back where it was', (await req('POST', '/sessions/gone/restore', tok, {})).code === 200 && await ids() === 'gone,gone2,keep' && fs.existsSync(path.join(P, 'gone.jsonl')));
  ok('8 restoring something that is not in the trash says so', (await req('POST', '/sessions/nothing/restore', tok, {})).code === 404);
  ok('9 a strange id cannot reach other files', (await req('POST', '/sessions/..%2F..%2Fx/delete', tok, {})).code === 404);
  // whole projects
  fs.writeFileSync(path.join(proj, 'main.js'), 'console.log(1)'); mk('keep', 'keep me');
  ok('10 a guest key cannot delete a project', (await req('POST', '/projects/delete', 'share.abcd.abcdefghijklmnop', { cwd: proj })).code === 401);
  ok('11 the projects folder itself and outside folders cannot be deleted', (await req('POST', '/projects/delete', tok, { cwd: path.join(T, 'proj') })).code === 403 && (await req('POST', '/projects/delete', tok, { cwd: '/tmp' })).code === 403);
  const dp = await req('POST', '/projects/delete', tok, { cwd: proj });
  ok('12 deleting a project moves its folder and its chats to the trash', dp.code === 200 && dp.j.chats === 3 && !fs.existsSync(proj) && !fs.existsSync(P) && (await ids()) === '');
  const tr = (await req('GET', '/projects/trash', tok)).j;
  ok('13 the trash lists it, and the trash is not a project', tr.length === 1 && tr[0].name === 'app' && !(await req('GET', '/projects', tok)).j.projects.some((x) => x.name.startsWith('.')));
  ok('14 restoring brings back files and chats', (await req('POST', '/projects/restore', tok, { stem: tr[0].stem })).code === 200 && fs.existsSync(path.join(proj, 'main.js')) && await ids() === 'gone,gone2,keep');
  ok('15 a restore of something unknown is refused', (await req('POST', '/projects/restore', tok, { stem: '1__nothing' })).code === 404 && (await req('POST', '/projects/restore', tok, { stem: '../../etc' })).code === 400);
  relay.kill(); await sleep(300); fs.rmSync(T, { recursive: true, force: true });
  console.log(`\n${pass} passed, ${fail} failed`); process.exit(fail ? 1 : 0);
})();
