// When Claude's login has expired, the chat says so in plain words (and how to fix it) instead of "Claude reported an error". Run: node desktop/test/auth-error.test.js
const { spawn } = require('child_process');
const fs = require('fs'), os = require('os'), path = require('path'), http = require('http');
if (process.platform === 'win32') { console.log('skipped on Windows'); process.exit(0); }
fs.mkdirSync(path.join(os.homedir(), '.cache'), { recursive: true }); const T = fs.mkdtempSync(path.join(os.homedir(), '.cache', 'hearth-auth-'));
for (const d of ['bin', 'home', 'proj', 'proj/app']) fs.mkdirSync(path.join(T, d), { recursive: true });
fs.writeFileSync(path.join(T, 'bin', 'claude'), `#!/usr/bin/env node
let t = ''; process.stdin.on('data', (d) => { t += d; }); process.stdin.on('end', () => {
  console.log(JSON.stringify({ type: 'system', subtype: 'init', session_id: 'auth-' + process.pid }));
  console.log(JSON.stringify({ type: 'result', is_error: true, result: 'Failed to authenticate: OAuth session expired and could not be refreshed' }));
});
`, { mode: 0o755 });
const relay = spawn('node', [path.join(__dirname, '..', 'relay', 'standalone.js'), '--dir', path.join(T, 'd'), '--port', '47790', '--host', '127.0.0.1', '--projects', path.join(T, 'proj'), '--name', 'My server'], { env: { ...process.env, HOME: path.join(T, 'home'), PATH: path.join(T, 'bin') + path.delimiter + process.env.PATH }, stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + x : '')); };
const req = (method, url, tok, body) => new Promise((res) => { const d = body ? Buffer.from(JSON.stringify(body)) : null; const r = http.request({ host: '127.0.0.1', port: 47790, path: url, method, headers: { Authorization: 'Bearer ' + tok, ...(d ? { 'Content-Type': 'application/json', 'Content-Length': d.length } : {}) } }, (rs) => { let b = ''; rs.on('data', (x) => { b += x; }); rs.on('end', () => res({ code: rs.statusCode, body: b })); }); r.on('error', () => res({ code: 0, body: '' })); if (d) r.write(d); r.end(); });
(async () => {
  await sleep(2000); const tok = JSON.parse(fs.readFileSync(path.join(T, 'd', 'relay.json'))).token;
  const r = await req('POST', '/sessions/new/send', tok, { text: 'hi', cwd: path.join(T, 'proj', 'app') });
  const done = r.body.split('\n').filter(Boolean).map((l) => { try { return JSON.parse(l); } catch { return {}; } }).find((e) => e.t === 'done');
  ok('1 the done event is an error with a clear message', done && done.error === true && /logged out on My server/.test(done.text) && /claude login/.test(done.text), JSON.stringify(done || {}).slice(0, 200));
  ok('2 it does not show the raw technical message', done && !/OAuth session expired/.test(done.text));
  relay.kill(); await sleep(300); fs.rmSync(T, { recursive: true, force: true });
  console.log(`\n${pass} passed, ${fail} failed`); process.exit(fail ? 1 : 0);
})();
