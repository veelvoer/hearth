// The Stop button, and a permission request that is waiting when Stop is pressed. Uses a fake "claude". Run: node desktop/test/stop.test.js
const { spawn } = require('child_process');
const fs = require('fs'), os = require('os'), path = require('path'), http = require('http');
if (process.platform === 'win32') { console.log('skipped on Windows'); process.exit(0); }
const T = fs.mkdtempSync(path.join(os.tmpdir(), 'hearth-stop-')); fs.mkdirSync(path.join(T, 'bin')); fs.mkdirSync(path.join(T, 'home'));
fs.writeFileSync(path.join(T, 'bin', 'claude'), `#!/usr/bin/env node
const fs = require('fs'), cp = require('child_process');
const a = process.argv, sf = a[a.indexOf('--settings') + 1];
const hook = JSON.parse(fs.readFileSync(sf, 'utf8')).hooks.PermissionRequest[0].hooks[0].command;
const sid = 'fake-' + process.pid;
console.log(JSON.stringify({ type: 'system', subtype: 'init', session_id: sid, model: 'fake' }));
if (process.env.ASK === '1') cp.spawnSync('sh', ['-c', hook], { input: JSON.stringify({ hook_event_name: 'PermissionRequest', session_id: sid, tool_name: 'Edit', tool_input: { file_path: 'a.js', old_string: 'x', new_string: 'y' }, cwd: process.cwd() }), encoding: 'utf8' });
setInterval(() => console.log(JSON.stringify({ type: 'stream_event', event: { delta: { type: 'text_delta', text: '.' } } })), 200);
`, { mode: 0o755 });
const env = { ...process.env, ASK: '1', HOME: path.join(T, 'home'), PATH: path.join(T, 'bin') + path.delimiter + process.env.PATH };
const relay = spawn('node', [path.join(__dirname, '..', 'relay', 'standalone.js'), '--lan', '--dir', path.join(T, 'd'), '--port', '47795', '--host', '127.0.0.1', '--no-memory-limit'], { env, stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + x : '')); };
const req = (method, url, tok, body, onData) => new Promise((res) => { const d = body ? Buffer.from(JSON.stringify(body)) : null; const r = http.request({ host: '127.0.0.1', port: 47795, path: url, method, headers: { Authorization: 'Bearer ' + tok, ...(d ? { 'Content-Type': 'application/json', 'Content-Length': d.length } : {}) } }, (x) => { let b = ''; x.on('data', (c) => { b += c; if (onData) onData(b); }); x.on('end', () => res({ code: x.statusCode, body: b })); }); r.on('error', () => res({ code: 0, body: '' })); if (d) r.write(d); r.end(); });
const alive = (pid) => { try { process.kill(pid, 0); return true; } catch { return false; } };
(async () => {
  await sleep(1800); const tok = JSON.parse(fs.readFileSync(path.join(T, 'd', 'relay.json'))).token;
  let events = ''; req('GET', '/events', tok, null, (b) => { events = b; });
  // a chat that waits for your OK is stopped: the approval is withdrawn and the process ends
  let stream = ''; const run = req('POST', '/sessions/new/send', tok, { text: 'hi', cwd: path.join(T, 'home') }, (b) => { stream = b; });
  await sleep(1500);
  const sid = (/"id":"(fake-\d+)"/.exec(stream) || [])[1]; ok('1 the run reports its chat id', !!sid, sid);
  const pid = Number((sid || '').slice(5));
  const r = await req('POST', '/stop', tok, { session: sid }); ok('2 stop answers ok', r.code === 200 && JSON.parse(r.body).ok === true);
  await run; await sleep(300);
  ok('3 the process is gone', !alive(pid));
  ok('4 the stream ended', /"t":"end"/.test(stream));
  const r2 = await req('POST', '/stop', tok, { session: 'nothing-here' }); ok('5 stopping something that does not run says so', JSON.parse(r2.body).ok === false);
  // the events are "id: N" then "data: {...}" blocks (the desktop app reads them with this rule)
  const datas = events.split('\n\n').filter(Boolean).filter((b) => b.startsWith('id: ')).map((b) => b.split('\n').find((l) => l.startsWith('data: '))).filter(Boolean).map((l) => JSON.parse(l.slice(6)));
  ok('6 a permission request came as an id + data block', datas.some((e) => e.kind === 'permission' && e.tool === 'Edit' && e.input && e.input.new_string === 'y'));
  ok('7 stopping withdrew the waiting approval', datas.some((e) => e.kind === 'resolved'));
  relay.kill(); await sleep(300);
  console.log(`\n${pass} passed, ${fail} failed`); process.exit(fail ? 1 : 0);
})();
