// A Claude that ends without an answer must say so, not leave the chat empty. Run: node desktop/test/silent-exit.test.js
const { spawn } = require('child_process');
const fs = require('fs'), os = require('os'), path = require('path'), http = require('http');
if (process.platform === 'win32') { console.log('skipped on Windows'); process.exit(0); }
fs.mkdirSync(path.join(os.homedir(), '.cache'), { recursive: true }); const T = fs.mkdtempSync(path.join(os.homedir(), '.cache', 'hearth-silent-'));
const proj = path.join(T, 'proj', 'app'); fs.mkdirSync(proj, { recursive: true });
const FAKE = path.join(T, 'bin'); fs.mkdirSync(FAKE);
fs.writeFileSync(path.join(FAKE, 'claude'), `#!/usr/bin/env node
process.stderr.write('boom: something broke\\n'); process.exit(1);
`, { mode: 0o755 });
const relay = spawn('node', [path.join(__dirname, '..', 'relay', 'standalone.js'), '--dir', path.join(T, 'd'), '--port', '47797', '--host', '127.0.0.1', '--projects', path.join(T, 'proj')], { env: { ...process.env, PATH: FAKE + path.delimiter + process.env.PATH, HOME: path.join(T, 'home'), HEARTH_TEST: '1', HEARTH_MEM_CHILD: '' }, stdio: 'ignore' });
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + x : '')); };
const post = (p, tok, body) => new Promise((res) => { const d = Buffer.from(JSON.stringify(body)); const r = http.request({ host: '127.0.0.1', port: 47797, path: p, method: 'POST', headers: { Authorization: 'Bearer ' + tok, 'Content-Type': 'application/json', 'Content-Length': d.length } }, (rs) => { let b = ''; rs.on('data', (x) => { b += x; }); rs.on('end', () => res({ code: rs.statusCode, raw: b })); }); r.on('error', () => res({ code: 0, raw: '' })); r.write(d); r.end(); });
(async () => {
  const tok = await require('./_wait')(path.join(T, 'd', 'relay.json'));
  const r = await post('/sessions/new/send', tok, { text: 'hello', mode: 'plan', cwd: proj });
  ok('1 a run that ends without an answer reports an error with the reason', /"t":"done"/.test(r.raw) && /"error":true/.test(r.raw) && /something broke/.test(r.raw), r.raw.slice(0, 200));
  console.log(`\n${pass} passed, ${fail} failed`); relay.kill(); try { fs.rmSync(T, { recursive: true, force: true }); } catch { /* */ } process.exit(fail ? 1 : 0);
})();
