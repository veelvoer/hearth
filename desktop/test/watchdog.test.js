// A server that hangs is stopped and started again by the program that watches it, without anyone doing anything. Run: node desktop/test/watchdog.test.js
const { spawn } = require('child_process');
const fs = require('fs'), os = require('os'), path = require('path'), http = require('http');
if (process.platform === 'win32') { console.log('skipped on Windows'); process.exit(0); }
fs.mkdirSync(path.join(os.homedir(), '.cache'), { recursive: true }); const T = fs.mkdtempSync(path.join(os.homedir(), '.cache', 'hearth-wd-'));
for (const d of ['home', 'proj']) fs.mkdirSync(path.join(T, d), { recursive: true });
const env = { ...process.env, HOME: path.join(T, 'home'), HEARTH_TEST: '1', HEARTH_WD_INTERVAL_MS: '400', HEARTH_WD_TIMEOUT_MS: '700', HEARTH_WD_FAILS: '2', HEARTH_WD_GRACE_MS: '1500' };
const log = path.join(T, 'log.txt'), fd = fs.openSync(log, 'w');
const main = spawn('node', [path.join(__dirname, '..', 'relay', 'standalone.js'), '--dir', path.join(T, 'd'), '--port', '47789', '--host', '127.0.0.1', '--projects', path.join(T, 'proj')], { env, stdio: ['ignore', fd, fd] });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + x : '')); };
const get = (p, ms = 3000) => new Promise((res) => { const r = http.get({ host: '127.0.0.1', port: 47789, path: p, timeout: ms }, (rs) => { let b = ''; rs.on('data', (c) => { b += c; }); rs.on('end', () => { let j = {}; try { j = JSON.parse(b); } catch { /* */ } res({ code: rs.statusCode, j }); }); }); r.on('timeout', () => { r.destroy(); res({ code: 0, j: {} }); }); r.on('error', () => res({ code: 0, j: {} })); });
const until = async (fn, ms) => { const t = Date.now(); while (Date.now() - t < ms) { if (await fn()) return true; await sleep(150); } return false; };
(async () => {
  ok('1 the server answers /health, and /health shows nothing private', await until(async () => (await get('/health')).code === 200, 10000) && JSON.stringify((await get('/health')).j).length < 60);
  const boot1 = (await get('/health')).j.boot;
  get('/__hang', 60000);   // the server is now stuck for a minute
  await sleep(500);
  ok('2 while stuck it does not answer', (await get('/health', 600)).code === 0);
  const back = await until(async () => { const h = await get('/health', 800); return h.code === 200 && h.j.boot && h.j.boot !== boot1; }, 20000);
  ok('3 the watchdog stops it and starts a new one, which answers again', back);
  ok('4 it says so in the log', /watchdog: no answer from the server/.test(fs.readFileSync(log, 'utf8')) && /starting it again/.test(fs.readFileSync(log, 'utf8')));
  const boot2 = (await get('/health')).j.boot; await sleep(3000);
  ok('5 a healthy server is left alone', (await get('/health')).j.boot === boot2);
  const t0 = Date.now(); main.kill('SIGTERM'); const gone = await new Promise((r) => { main.on('exit', () => r(true)); setTimeout(() => r(false), 8000); });
  ok('6 stopping the program stops the server too', gone && (await get('/health', 800)).code === 0, (Date.now() - t0) + ' ms');
  fs.rmSync(T, { recursive: true, force: true });
  console.log(`\n${pass} passed, ${fail} failed`); process.exit(fail ? 1 : 0);
})();
