// Two of your computers linked to one server: choose where a chat runs. Run: node test/computers.test.js
const { spawn } = require('child_process');
const fs = require('fs'), os = require('os'), path = require('path'), http = require('http');
const T = path.join(os.homedir(), '.cache', 'hearth-computers-test'); fs.rmSync(T, { recursive: true, force: true }); fs.mkdirSync(T + '/projects/app', { recursive: true });
const p = spawn('node', [path.join(__dirname, '..', 'relay', 'standalone.js'), '--dir', T + '/cfg', '--projects', T + '/projects', '--port', '47797', '--host', '127.0.0.1'], { stdio: 'ignore' });
const call = (method, url, tok, body, raw) => new Promise((res) => { const d = body ? Buffer.from(JSON.stringify(body)) : null; const r = http.request({ host: '127.0.0.1', port: 47797, path: url, method, headers: { Authorization: 'Bearer ' + tok, ...(d ? { 'Content-Type': 'application/json', 'Content-Length': d.length } : {}) } }, (rs) => { let b = ''; rs.on('data', (x) => { b += x; }); rs.on('end', () => { if (raw) return res(b); try { res(JSON.parse(b)); } catch { res({}); } }); }); r.on('error', () => res({})); r.end(d || undefined); });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + x : '')); };
(async () => {
  await sleep(1500); const tok = JSON.parse(fs.readFileSync(T + '/cfg/relay.json')).token;
  call('POST', '/agent/poll', tok, { name: 'Laptop', epoch: 0 }); call('POST', '/agent/poll', tok, { name: 'Desktop', epoch: 0 }); await sleep(700);
  const list = await call('GET', '/computers', tok);
  ok('1 the server lists both computers and itself', Array.isArray(list) && list[0].kind === 'server' && list.some((c) => c.id === 'pc:Laptop' && c.online) && list.some((c) => c.id === 'pc:Desktop' && c.online), JSON.stringify(list.map((c) => c.id)));
  const out = await call('POST', '/sessions/new/send', tok, { text: 'make a todo app', cwd: T + '/projects/app', run: 'pc:Desktop', mode: 'auto' }, true);
  ok('2 a new chat can be started on one computer (queued for it)', out.includes('"t":"queued"') && out.includes('"to":"Desktop"'), out.slice(0, 120));
  const a = await call('POST', '/agent/poll', tok, { name: 'Laptop', epoch: 0 });
  ok('3 the other computer gets nothing', !(a.work || []).length);
  const b = await call('POST', '/agent/poll', tok, { name: 'Desktop', epoch: 0 });
  ok('4 the chosen computer gets the work, as a new chat in the right folder', (b.work || []).length === 1 && b.work[0].newChat === true && b.work[0].rel === 'app' && b.work[0].target === 'Desktop', JSON.stringify((b.work || []).map((w) => [w.rel, w.target])));
  const bad = await call('POST', '/sessions/new/send', tok, { text: 'x', cwd: '/etc', run: 'pc:Desktop' }, true);
  ok('5 a folder outside the projects folder is refused', bad.includes('error'));
  p.kill(); fs.rmSync(T, { recursive: true, force: true });
  console.log(`\n${pass} passed, ${fail} failed`); process.exit(fail ? 1 : 0);
})();
