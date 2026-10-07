// End-to-end test of chat and file sync: starts a server relay and a laptop relay on this machine and checks 24 things.
// Run: node test/sync.test.js   (takes about 90 seconds)
// Two real relay processes with separate HOME folders: a "server" (public mode) and a "laptop" linked to it.
const { spawn } = require('child_process');
const fs = require('fs'), path = require('path'), http = require('http');
const T = require('os').homedir() + '/.cache/hearth-sync-test';   // not under /tmp: temp folders count as junk fs.rmSync(T, { recursive: true, force: true });
const R = path.join(__dirname, '..', 'relay', 'standalone.js');
const H = { srv: T + '/srv', lap: T + '/lap' };
for (const h of Object.values(H)) fs.mkdirSync(h + '/.claude/projects', { recursive: true });
fs.mkdirSync(H.srv + '/projects/app', { recursive: true }); fs.mkdirSync(H.lap + '/projects/app', { recursive: true }); fs.mkdirSync(H.lap + '/other/school', { recursive: true });
const MARK = '/__hearth_projects__';
const enc = (p) => p.replace(/[^A-Za-z0-9]/g, '-');
const uuid = (n) => '00000000-0000-4000-8000-' + String(n).padStart(12, '0');
function chat(home, id, cwd, lines, ageSec) {
  const d = `${home}/.claude/projects/${enc(cwd)}`; fs.mkdirSync(d, { recursive: true });
  const f = `${d}/${id}.jsonl`;
  const L = lines.map((t, i) => JSON.stringify({ type: i % 2 ? 'assistant' : 'user', cwd, timestamp: new Date(Date.now() - 1e5 + i).toISOString(), message: { role: i % 2 ? 'assistant' : 'user', content: i % 2 ? [{ type: 'text', text: t }] : t, id: 'm' + i, usage: { input_tokens: 1, output_tokens: 1 }, model: 'x' } }));
  fs.writeFileSync(f, L.join('\n') + '\n'); const t = new Date(Date.now() - ageSec * 1000); fs.utimesSync(f, t, t); return f;
}
const append = (f, cwd, t, ageSec) => { fs.appendFileSync(f, JSON.stringify({ type: 'user', cwd, message: { role: 'user', content: t } }) + '\n'); const x = new Date(Date.now() - ageSec * 1000); fs.utimesSync(f, x, x); };
const procs = [];
const run = (name, home, args) => { const p = spawn('node', [R, ...args], { env: { ...process.env, HOME: home }, stdio: ['ignore', 'ignore', fs.openSync(T + '-' + name + '.log', 'w')] }); procs.push(p); return p; };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const call = (port, tok, method, p, body) => new Promise((res) => { const d = body ? Buffer.from(JSON.stringify(body)) : null; const r = http.request({ host: '127.0.0.1', port, path: p, method, headers: { Authorization: 'Bearer ' + tok, ...(d ? { 'Content-Length': d.length } : {}) } }, (rs) => { let b = ''; rs.on('data', (x) => b += x); rs.on('end', () => { try { res(JSON.parse(b)); } catch { res(b); } }); }); if (d) r.write(d); r.end(); });
let pass = 0, fail = 0; const ok = (name, cond, extra = '') => { (cond ? pass++ : fail++); console.log((cond ? 'PASS ' : 'FAIL ') + name + (extra ? '  ' + extra : '')); };

(async () => {
  // data before start: laptop has an in-project chat (A), an elsewhere chat (B), a temp junk chat (J), an empty/deleted-folder chat (D); server has a chat the laptop lacks (S)
  const A = chat(H.lap, uuid(1), MARK + '/app', ['build the app', 'sure, on it', 'now add tests', 'done'], 120);
  chat(H.lap, uuid(2), H.lap + '/other/school', ['help with my homework', 'happy to help'], 100);
  chat(H.lap, uuid(3), '/tmp/junk', ['x', 'y'], 90);
  chat(H.lap, uuid(4), H.lap + '/projects/gone', ['old', 'chat'], 80);
  chat(H.srv, uuid(5), MARK + '/app', ['from the phone', 'ok'], 60);
  // the server's projects root must read as MARK for path rewriting; use real dirs under srv but root = MARK-like
  fs.mkdirSync(H.srv + '/projects/app', { recursive: true });
  const srvRoot = H.srv + '/projects';
  // rewrite server chat to use its own root
  { const f = `${H.srv}/.claude/projects/${enc(MARK + '/app')}/${uuid(5)}.jsonl`; const t = fs.readFileSync(f, 'utf8').split(MARK).join(srvRoot); fs.rmSync(path.dirname(f), { recursive: true }); const nd = `${H.srv}/.claude/projects/${enc(srvRoot + '/app')}`; fs.mkdirSync(nd, { recursive: true }); fs.writeFileSync(nd + '/' + uuid(5) + '.jsonl', t); const x = new Date(Date.now() - 60000); fs.utimesSync(nd + '/' + uuid(5) + '.jsonl', x, x); }
  const lapRoot = H.lap + '/projects';
  // laptop chat A written with the laptop's own root
  { const t = fs.readFileSync(A, 'utf8').split(MARK).join(lapRoot); fs.rmSync(path.dirname(A), { recursive: true }); const nd = `${H.lap}/.claude/projects/${enc(lapRoot + '/app')}`; fs.mkdirSync(nd, { recursive: true }); fs.writeFileSync(nd + '/' + uuid(1) + '.jsonl', t); const x = new Date(Date.now() - 120000); fs.utimesSync(nd + '/' + uuid(1) + '.jsonl', x, x); }
  const lapA = `${H.lap}/.claude/projects/${enc(lapRoot + '/app')}/${uuid(1)}.jsonl`, srvA = `${H.srv}/.claude/projects/${enc(srvRoot + '/app')}/${uuid(1)}.jsonl`;

  run('srv', H.srv, ['--dir', H.srv + '/cfg', '--projects', srvRoot, '--port', '47791', '--host', '127.0.0.1']);
  await sleep(1500);
  const tok = JSON.parse(fs.readFileSync(H.srv + '/cfg/relay.json')).token;
  fs.mkdirSync(H.lap + '/cfg', { recursive: true });
  fs.writeFileSync(H.lap + '/cfg/link.json', JSON.stringify({ url: 'http://127.0.0.1:47791', token: tok }));
  run('lap', H.lap, ['--lan', '--dir', H.lap + '/cfg', '--port', '47792', '--host', '127.0.0.1', '--move-root', lapRoot]);
  await sleep(1500);
  const ltok = JSON.parse(fs.readFileSync(H.lap + '/cfg/relay.json')).token;
  // the laptop reads link.json every 10 s; first sync follows
  for (let i = 0; i < 40; i++) { await sleep(1000); if (fs.existsSync(srvA)) break; }
  await sleep(2500);
  let S = await call(47791, tok, 'GET', '/sessions');
  const byId = (l, n) => l.find((x) => x.id === uuid(n));
  ok('1 in-project chat reached the server', !!byId(S, 1), byId(S, 1) ? byId(S, 1).title : '');
  ok('1b its path was rewritten for the server', fs.existsSync(srvA) && fs.readFileSync(srvA, 'utf8').includes(srvRoot) && !fs.readFileSync(srvA, 'utf8').includes(lapRoot));
  ok('2 outside-projects chat listed on the server as laptop-only', !!byId(S, 2) && !!byId(S, 2).elsewhere, byId(S, 2) && byId(S, 2).elsewhere);
  ok('2b ...and readable', Array.isArray(await call(47791, tok, 'GET', '/sessions/' + uuid(2) + '/messages')) && (await call(47791, tok, 'GET', '/sessions/' + uuid(2) + '/messages')).length >= 1);
  ok('3 temp-folder junk NOT on server', !byId(S, 3));
  ok('3b deleted-folder chat NOT on server', !byId(S, 4));
  let L = await call(47792, ltok, 'GET', '/sessions');
  ok('4 server-only chat arrived on the laptop', !!byId(L, 5), byId(L, 5) && byId(L, 5).title);
  ok('4b laptop list hides junk', !byId(L, 3) && !byId(L, 4));
  const lapS = `${H.lap}/.claude/projects/${enc(lapRoot + '/app')}/${uuid(5)}.jsonl`;
  ok('4c rewritten for the laptop', fs.existsSync(lapS) && fs.readFileSync(lapS, 'utf8').includes(lapRoot) && !fs.readFileSync(lapS, 'utf8').includes(srvRoot));
  // 5 growth on the laptop -> only the tail goes over; server ends identical
  append(lapA, lapRoot + '/app', 'one more thing from the laptop', 30);
  await call(47792, ltok, 'POST', '/sync/now', {});
  const n = (f) => fs.readFileSync(f, 'utf8').trim().split('\n').length;
  ok('5 laptop append reached server', n(srvA) === 5, 'lines=' + n(srvA));
  // 6 server newer -> laptop pulls
  append(srvA, srvRoot + '/app', 'reply made on the server', 25);
  await call(47792, ltok, 'POST', '/sync/now', {});
  ok('6 server append reached laptop', n(lapA) === 6, 'lines=' + n(lapA));
  // 7 stable: another sync changes nothing
  const before = fs.statSync(lapA).mtimeMs + ':' + fs.statSync(srvA).mtimeMs; await call(47792, ltok, 'POST', '/sync/now', {}); await call(47792, ltok, 'POST', '/sync/now', {});
  ok('7 no ping-pong', before === fs.statSync(lapA).mtimeMs + ':' + fs.statSync(srvA).mtimeMs);
  // 8 sync status
  const st = await call(47791, tok, 'GET', '/sync/status'); const ls = await call(47792, ltok, 'GET', '/sync/status');
  ok('8 status: server sees the computer', st.computers && st.computers.length === 1 && st.computers[0].online, JSON.stringify(st.computers && st.computers[0] && st.computers[0].sync && { ok: st.computers[0].sync.ok, pushed: st.computers[0].sync.pushed }));
  ok('8b status: laptop knows last sync', ls.last && ls.last.ok === true, JSON.stringify({ chats: ls.localChats, waiting: ls.waiting }));
  // 9 reply to a laptop-only chat from the server side is handed to the laptop
  const q = await new Promise((res) => { const d = Buffer.from(JSON.stringify({ text: 'continue please' })); const r = http.request({ host: '127.0.0.1', port: 47791, path: '/sessions/' + uuid(2) + '/send', method: 'POST', headers: { Authorization: 'Bearer ' + tok, 'Content-Length': d.length } }, (rs) => { let b = ''; rs.on('data', (x) => b += x); rs.on('end', () => res(b)); }); r.end(d); });
  ok('9 reply to laptop-only chat is queued for the laptop', /"t":"queued"/.test(q) && /"online":true/.test(q), q.split('\n')[0]);
  // 10 server busy chat is never overwritten: simulate by stale push (laptop older than server) handled in 6; check old copy cannot clobber newer
  const staleBefore = n(srvA); const old = new Date(Date.now() - 3600e3); fs.utimesSync(lapA, old, old); await call(47792, ltok, 'POST', '/sync/now', {});
  ok('10 an older laptop copy never overwrites a newer server copy', n(srvA) >= staleBefore);

  // ── files ──
  const W = (f, txt, ageSec = 10) => { fs.mkdirSync(path.dirname(f), { recursive: true }); fs.writeFileSync(f, txt); const x = new Date(Date.now() - ageSec * 1000); fs.utimesSync(f, x, x); };
  const syncNow = async () => { await call(47792, ltok, 'POST', '/sync/now', {}); await call(47792, ltok, 'POST', '/sync/now', {}); };
  W(lapRoot + '/app/src/a.txt', 'hello from laptop'); W(lapRoot + '/app/node_modules/x/y.js', 'junk'); W(srvRoot + '/newproj/readme.md', 'made on the phone');
  await syncNow();
  ok('11 laptop file reached the server', fs.existsSync(srvRoot + '/app/src/a.txt') && fs.readFileSync(srvRoot + '/app/src/a.txt', 'utf8') === 'hello from laptop');
  ok('11b node_modules not synced', !fs.existsSync(srvRoot + '/app/node_modules'));
  let ov = await call(47792, ltok, 'GET', '/sync/status');
  ok('12 new server project waits for approval', Array.isArray(ov.pending) && ov.pending.some((p) => p.name === 'newproj') && !fs.existsSync(lapRoot + '/newproj'), JSON.stringify(ov.pending));
  await call(47792, ltok, 'POST', '/sync/projects/accept', { names: ['newproj'] });
  ok('12b after Install it arrives', fs.existsSync(lapRoot + '/newproj/readme.md') && fs.readFileSync(lapRoot + '/newproj/readme.md', 'utf8') === 'made on the phone');
  W(srvRoot + '/app/src/a.txt', 'edited on server', 1); await new Promise((r) => setTimeout(r, 3500)); await syncNow();
  ok('13 server edit reached laptop', fs.readFileSync(lapRoot + '/app/src/a.txt', 'utf8') === 'edited on server');
  fs.unlinkSync(lapRoot + '/app/src/a.txt'); await syncNow();
  ok('14 delete on laptop removes it on server (to trash)', !fs.existsSync(srvRoot + '/app/src/a.txt') && fs.existsSync(srvRoot + '/.hearth-trash'));
  W(lapRoot + '/app/c.txt', 'v1', 30); await syncNow();
  W(lapRoot + '/app/c.txt', 'laptop v2', 5); W(srvRoot + '/app/c.txt', 'server v2', 1); await new Promise((r) => setTimeout(r, 3500)); await syncNow();
  const cf = fs.readdirSync(lapRoot + '/app').concat(fs.readdirSync(srvRoot + '/app'));
  ok('15 conflict: newest wins, other kept', fs.readFileSync(lapRoot + '/app/c.txt', 'utf8') === 'server v2' && cf.some((n) => n.includes('conflict')), cf.join(','));
  for (let i = 0; i < 30; i++) W(lapRoot + '/bulk/f' + i + '.txt', 'b' + i);
  await call(47792, ltok, 'POST', '/sync/projects/accept', { all: true }); await syncNow();
  for (let i = 0; i < 30; i++) fs.unlinkSync(lapRoot + '/bulk/f' + i + '.txt'); await syncNow();
  ok('16 safety brake: mass delete is held back', fs.existsSync(srvRoot + '/bulk/f5.txt'), JSON.stringify((await call(47792, ltok, 'GET', '/sync/status')).files && (await call(47792, ltok, 'GET', '/sync/status')).files.paused));
  for (const p of procs) p.kill();
  console.log(`\n${pass} passed, ${fail} failed`);
  fs.rmSync(T, { recursive: true, force: true }); process.exit(fail ? 1 : 0);
})();
