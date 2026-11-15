// The desktop app starts the Rust reader next to its relay. Run: HEARTH_RUST_FRONT=/path/to/hearth-relay node desktop/test/rustcore.test.js
const http = require('http');
const rc = require('../rustcore');
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + x : '')); };
const post = (env, p, secret) => new Promise((res) => { const u = new URL(env.HEARTH_RUST_CORE); const r = http.request({ host: u.hostname, port: u.port, path: p, method: 'POST', headers: { 'x-hearth-core': secret, 'Content-Type': 'application/json', 'Content-Length': 2 } }, (rs) => { let b = ''; rs.on('data', (x) => { b += x; }); rs.on('end', () => res({ code: rs.statusCode, b })); }); r.on('error', () => res({ code: 0, b: '' })); r.end('{}'); });
(async () => {
  ok('1 a missing program means no fast part, without an error', (await rc.start({ bin: '/nonexistent/hearth-relay' })) === null);
  const bin = process.env.HEARTH_RUST_FRONT;
  if (!bin) { console.log('skipped the rest (no HEARTH_RUST_FRONT)'); console.log(`\n${pass} passed, ${fail} failed`); process.exit(fail ? 1 : 0); }
  const c = await rc.start({ bin });
  ok('2 it starts and gives the relay an address and a secret', !!c && /^http:\/\/127\.0\.0\.1:\d+$/.test(c.env.HEARTH_RUST_CORE) && c.env.HEARTH_RUST_CORE_SECRET.length >= 32);
  if (c) {
    const good = await post(c.env, '/summaries', c.env.HEARTH_RUST_CORE_SECRET), bad = await post(c.env, '/summaries', 'wrong');
    ok('3 it answers with the secret and refuses without', good.code === 200 && bad.code === 401, good.code + '/' + bad.code);
    c.stop(); await new Promise((r) => setTimeout(r, 300));
    ok('4 stop ends it', (await post(c.env, '/summaries', c.env.HEARTH_RUST_CORE_SECRET)).code === 0);
  }
  console.log(`\n${pass} passed, ${fail} failed`); process.exit(fail ? 1 : 0);
})();
