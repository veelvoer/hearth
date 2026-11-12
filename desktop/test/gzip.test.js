// Big answers are compressed for clients that ask (phones on mobile data) and plain for the rest. Run: node desktop/test/gzip.test.js
const { spawn } = require('child_process');
const fs = require('fs'), os = require('os'), path = require('path'), http = require('http'), zlib = require('zlib');
if (process.platform === 'win32') { console.log('skipped on Windows'); process.exit(0); }
fs.mkdirSync(path.join(os.homedir(), '.cache'), { recursive: true }); const T = fs.mkdtempSync(path.join(os.homedir(), '.cache', 'hearth-gzip-'));
fs.mkdirSync(path.join(T, 'proj'), { recursive: true });
const relay = spawn('node', [path.join(__dirname, '..', 'relay', 'standalone.js'), '--dir', path.join(T, 'd'), '--port', '47799', '--host', '127.0.0.1', '--projects', path.join(T, 'proj')], { env: { ...process.env, HOME: path.join(T, 'home'), HEARTH_TEST: '1', HEARTH_MEM_CHILD: '' }, stdio: 'ignore' });
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + x : '')); };
const get = (p, tok, enc) => new Promise((res) => { http.get({ host: '127.0.0.1', port: 47799, path: p, headers: { Authorization: 'Bearer ' + tok, ...(enc ? { 'Accept-Encoding': 'gzip' } : {}) } }, (rs) => { const c = []; rs.on('data', (x) => c.push(x)); rs.on('end', () => res({ code: rs.statusCode, enc: rs.headers['content-encoding'], buf: Buffer.concat(c) })); }).on('error', () => res({ code: 0, buf: Buffer.alloc(0) })); });
(async () => {
  const tok = await require('./_wait')(path.join(T, 'd', 'relay.json'));
  const post = (b) => new Promise((res) => { const d = Buffer.from(JSON.stringify(b)); const r = http.request({ host: '127.0.0.1', port: 47799, path: '/templates', method: 'POST', headers: { Authorization: 'Bearer ' + tok, 'Content-Type': 'application/json', 'Content-Length': d.length } }, (rs) => { rs.resume(); rs.on('end', res); }); r.write(d); r.end(); });
  for (let i = 0; i < 12; i++) await post({ add: { name: 'Template ' + i, text: 'Write a friendly, short email to a customer about order number ' + i + '. '.repeat(1) + 'Keep it polite. '.repeat(20) } });
  const plain = await get('/templates', tok, false), gz = await get('/templates', tok, true);
  ok('1 without a request for it the answer is plain', plain.code === 200 && !plain.enc && plain.buf.length > 1024, String(plain.buf.length));
  ok('2 with Accept-Encoding gzip it is compressed, smaller, and the same', gz.enc === 'gzip' && gz.buf.length < plain.buf.length && zlib.gunzipSync(gz.buf).toString() === plain.buf.toString(), gz.buf.length + ' vs ' + plain.buf.length);
  console.log(`\n${pass} passed, ${fail} failed`); relay.kill(); try { fs.rmSync(T, { recursive: true, force: true }); } catch { /* */ } process.exit(fail ? 1 : 0);
})();
