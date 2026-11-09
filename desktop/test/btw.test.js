// A side question (like /btw) is answered by a copy of the chat with no tools. Run: node desktop/test/btw.test.js
const { spawn } = require('child_process');
const fs = require('fs'), os = require('os'), path = require('path'), http = require('http');
if (process.platform === 'win32') { console.log('skipped on Windows'); process.exit(0); }
fs.mkdirSync(path.join(os.homedir(), '.cache'), { recursive: true }); const T = fs.mkdtempSync(path.join(os.homedir(), '.cache', 'hearth-btw-'));
const proj = path.join(T, 'proj', 'app'); fs.mkdirSync(proj, { recursive: true });
const HOME = path.join(T, 'home'), dir = path.join(HOME, '.claude', 'projects', proj.replace(/[^A-Za-z0-9]/g, '-')); fs.mkdirSync(dir, { recursive: true });
const sid = 'sid-btw1'; fs.writeFileSync(path.join(dir, sid + '.jsonl'), JSON.stringify({ type: 'user', cwd: proj, sessionId: sid, timestamp: new Date().toISOString(), message: { role: 'user', content: 'do it' } }) + '\n');
const FAKE = path.join(T, 'bin'); fs.mkdirSync(FAKE); const LOG = path.join(T, 'args.txt');
fs.writeFileSync(path.join(FAKE, 'claude'), `#!/usr/bin/env node
let t = ''; process.stdin.on('data', (d) => { t += d; }); process.stdin.on('end', () => { require('fs').writeFileSync(${JSON.stringify(LOG)}, process.argv.slice(2).join(' ') + '\\n' + t);
  process.stdout.write(JSON.stringify({ type: 'result', is_error: false, result: 'It is in a.txt.', usage: {} })); });
`, { mode: 0o755 });
const relay = spawn('node', [path.join(__dirname, '..', 'relay', 'standalone.js'), '--dir', path.join(T, 'd'), '--port', '47798', '--host', '127.0.0.1', '--projects', path.join(T, 'proj')], { env: { ...process.env, PATH: FAKE + path.delimiter + process.env.PATH, HOME, HEARTH_TEST: '1', HEARTH_MEM_CHILD: '' }, stdio: 'ignore' });
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + x : '')); };
const post = (p, tok, body) => new Promise((res) => { const d = Buffer.from(JSON.stringify(body)); const r = http.request({ host: '127.0.0.1', port: 47798, path: p, method: 'POST', headers: { Authorization: 'Bearer ' + tok, 'Content-Type': 'application/json', 'Content-Length': d.length } }, (rs) => { let b = ''; rs.on('data', (x) => { b += x; }); rs.on('end', () => { let j = {}; try { j = JSON.parse(b); } catch { /* */ } res({ code: rs.statusCode, j }); }); }); r.on('error', () => res({ code: 0, j: {} })); r.write(d); r.end(); });
(async () => {
  const tok = await require('./_wait')(path.join(T, 'd', 'relay.json'));
  const r = await post(`/sessions/${sid}/btw`, tok, { text: 'where is the config?' });
  const rec = fs.existsSync(LOG) ? fs.readFileSync(LOG, 'utf8') : '';
  ok('1 the answer comes back', r.code === 200 && r.j.text === 'It is in a.txt.', JSON.stringify(r.j));
  ok('2 it uses a copy of the chat, no tools, and keeps nothing', /--resume sid-btw1/.test(rec) && /--fork-session/.test(rec) && /--no-session-persistence/.test(rec) && /--tools\s/.test(rec) && /where is the config\?/.test(rec), rec.slice(0, 160));
  ok('3 an empty question is refused', (await post(`/sessions/${sid}/btw`, tok, { text: ' ' })).code === 400);
  ok('4 an unknown chat is refused', (await post('/sessions/nope/btw', tok, { text: 'hi' })).code === 404);
  ok('5 a guest key cannot ask', (await post(`/sessions/${sid}/btw`, 'share.abcd.abcdefghijklmnop', { text: 'hi' })).code === 401);
  console.log(`\n${pass} passed, ${fail} failed`); relay.kill(); try { fs.rmSync(T, { recursive: true, force: true }); } catch { /* */ } process.exit(fail ? 1 : 0);
})();
