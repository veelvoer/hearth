// The Rust reads must give the same answers as relay/core.js. Run: HEARTH_RUST_FRONT=/path/to/hearth-relay node desktop/test/rust-core.test.js
// Uses synthetic chats with odd cases, and (when HEARTH_REAL=1) the chats of this computer too.
const { spawn } = require('child_process');
const fs = require('fs'), os = require('os'), path = require('path'), http = require('http'), assert = require('assert');
const bin = process.env.HEARTH_RUST_FRONT;
if (!bin || process.platform === 'win32') { console.log('skipped (no HEARTH_RUST_FRONT)'); process.exit(0); }
fs.mkdirSync(path.join(os.homedir(), '.cache'), { recursive: true });
const T = fs.mkdtempSync(path.join(os.homedir(), '.cache', 'hearth-rcore-'));
const REAL = process.env.HEARTH_REAL === '1';
const HOME = REAL ? os.homedir() : path.join(T, 'home');
process.env.HOME = HOME;   // core.js reads os.homedir()
const core = require('../relay/core');
const dir = path.join(HOME, '.claude', 'projects', '-synth'); fs.mkdirSync(dir, { recursive: true });
const ts = (n) => new Date(Date.UTC(2026, 10, 1, 10, 0, n, 123)).toISOString();
const L = (o) => JSON.stringify(o) + '\n';
const synth = {
  'aaaa-1': [
    { type: 'user', cwd: '/work/app', timestamp: ts(1), message: { role: 'user', content: '  <system>x</system>' } },
    { type: 'user', cwd: '/work/app', timestamp: ts(2), message: { role: 'user', content: '  Fix the “login” bug 🎉 please   ' } },
    { type: 'assistant', timestamp: ts(3), message: { id: 'm1', model: 'claude-sonnet-5-1', usage: { input_tokens: 100, output_tokens: 10, cache_read_input_tokens: 5000, cache_creation_input_tokens: 200 }, content: [{ type: 'text', text: 'Looking.' }, { type: 'tool_use', id: 't1', name: 'Bash', input: { command: 'ls -la', description: 'list' } }] } },
    { type: 'assistant', timestamp: ts(4), message: { id: 'm1', model: 'claude-sonnet-5-1', usage: { input_tokens: 100, output_tokens: 60, cache_read_input_tokens: 5000, cache_creation_input_tokens: 200 }, content: [{ type: 'tool_use', id: 't2', name: 'Edit', input: { file_path: '/a/b.js', old_string: 'x'.repeat(2000), new_string: 'é' } }, { type: 'tool_use', id: 't3', name: 'MultiEdit', input: { file_path: '/c', edits: [{ old_string: 'a', new_string: 'b' }, { old_string: 'c', new_string: 'd' }] } }, { type: 'tool_use', id: 't4', name: 'Grep', input: { pattern: 'foo', path: '/x', extra: { a: 1, b: [1, 2] }, n: 5 } }] } },
    { type: 'user', timestamp: ts(5), message: { role: 'user', content: [{ type: 'tool_result', tool_use_id: 't1', content: [{ type: 'text', text: 'total 0' }, { type: 'text', text: 'more' }] }, { type: 'tool_result', tool_use_id: 't2', content: 'ok', is_error: true }] } },
    { type: 'assistant', isSidechain: true, timestamp: ts(6), message: { id: 'sc', model: 'claude-haiku-4-5', usage: { input_tokens: 9, output_tokens: 9 }, content: [{ type: 'text', text: 'side' }] } },
    { type: 'assistant', timestamp: ts(130), message: { id: 'm2', model: 'claude-opus-5-5', usage: { input_tokens: 1, output_tokens: 1234567 }, content: [{ type: 'text', text: '  Done. Searching for NEEDLE-word here ' }] } },
    { type: 'ai-title', aiTitle: 'Login fixes' },
    { type: 'assistant', timestamp: ts(131), message: { id: 'm3', model: '<synthetic>', usage: { input_tokens: 1, output_tokens: 1 }, content: [{ type: 'text', text: 'ignored model' }] } },
  ],
  'bbbb-2': [
    { type: 'user', cwd: '/other', timestamp: ts(10), message: { role: 'user', content: [{ type: 'text', text: 'Naïve café ΑΣ question' }, { type: 'image', source: {} }] } },
    { type: 'assistant', timestamp: ts(11), message: { id: 'z', model: 'claude-fable-5-1', usage: { input_tokens: 2000000, output_tokens: 25000, cache_read_input_tokens: 10 }, content: [{ type: 'text', text: 'Ξ NEEDLE-word again, ΑΣ naïve' }] } },
  ],
};
for (const [id, lines] of Object.entries(synth)) fs.writeFileSync(path.join(dir, id + '.jsonl'), lines.map(L).join('') + '{"type":"user","message":{"content":"broken line'.slice(0, 18) + '\n' + '{"type":"user","timestamp":"' + ts(200) + '","message":{"content":"unfinished tail"}}');
const secret = 'test-secret';
const port = 47852;
const rs = spawn(bin, ['--core-only', '--port', String(port), '--secret', secret], { env: { ...process.env, HOME }, stdio: 'ignore' });
const post = (p, body) => new Promise((res, rej) => { const d = Buffer.from(JSON.stringify(body || {})); const r = http.request({ host: '127.0.0.1', port, path: p, method: 'POST', headers: { 'x-hearth-core': secret, 'Content-Type': 'application/json', 'Content-Length': d.length } }, (rs2) => { const c = []; rs2.on('data', (x) => c.push(x)); rs2.on('end', () => { try { res({ code: rs2.statusCode, j: JSON.parse(Buffer.concat(c).toString()) }); } catch (e) { rej(e); } }); }); r.on('error', rej); r.write(d); r.end(); });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + String(x).slice(0, 400) : '')); };
const same = (n, a, b) => { try { assert.deepStrictEqual(JSON.parse(JSON.stringify(a)), JSON.parse(JSON.stringify(b))); ok(n, true); } catch (e) { ok(n, false, 'js=' + JSON.stringify(a).slice(0, 300) + ' rust=' + JSON.stringify(b).slice(0, 300)); } };
(async () => {
  for (let i = 0; i < 50; i++) { try { await post('/search', { q: 'zz' }); break; } catch { await sleep(100); } }
  ok('0 a call without the secret is refused', await new Promise((res) => { http.request({ host: '127.0.0.1', port, path: '/summaries', method: 'POST' }, (r) => { r.resume(); res(r.statusCode === 401); }).on('error', () => res(false)).end(); }));
  const files = core.listFiles().filter((f) => REAL || f.file.includes('-synth'));
  const sums = (await post('/summaries')).j;
  let bad = 0, shown = 0;
  for (const f of files) {
    let js = core.summarize(f, null); for (let k = 0; !js.complete && k < 100; k++) js = core.summarize(f, null);   // core.js reads a huge chat in steps of 16 MB per look
    const r = sums.find((x) => x.file === f.file);
    if (!r) { bad++; if (shown++ < 3) console.log('  missing', f.file); continue; }
    const a = { title: js.title, cwd: js.cwd, tokens: js.tokens, cost: js.cost, model: js.model }, b = { title: r.title, cwd: r.cwd, tokens: r.tokens, cost: r.cost, model: r.model };
    const close = (x, y) => Math.abs(x - y) <= 1e-9 * Math.max(1, Math.abs(x));
    if (a.title !== b.title || a.cwd !== b.cwd || a.model !== b.model || !close(a.tokens, b.tokens) || !close(a.cost, b.cost)) { bad++; if (shown++ < 3) console.log('  differs', f.file, JSON.stringify(a).slice(0, 200), JSON.stringify(b).slice(0, 200)); }
  }
  ok(`1 titles, folders, tokens, cost and model of ${files.length} chats match`, bad === 0, bad + ' differ');
  let mbad = 0; shown = 0;
  for (const [f, tail] of files.slice(0, REAL ? 25 : 99).flatMap((f) => [80, 5, 400].map((t) => [f, t]))) {
    const js = core.messagesOf(f, tail), r = (await post('/messages', { file: f.file, tail })).j;
    try { assert.deepStrictEqual(JSON.parse(JSON.stringify(js)), r); } catch { mbad++; if (shown++ < 2) { const i = js.findIndex((m, k) => JSON.stringify(m) !== JSON.stringify(r[k])); console.log('  messages differ', f.file, 'at', i, JSON.stringify(js[i]).slice(0, 300), '| rust', JSON.stringify(r[i]).slice(0, 300)); } }
  }
  ok('2 the last 5, 80 and 400 messages match', mbad === 0, mbad + ' differ');
  for (const q of ['needle-word', 'NAÏVE', 'login', 'ασ', 'zzzz-not-there', 'é']) {
    const js = core.search(q, 25).filter((x) => REAL || x.cwd === '/work/app' || x.cwd === '/other' || x.id.includes('-')), r = (await post('/search', { q, limit: 25 })).j;
    const pick = (l) => l.filter((x) => REAL || /^(aaaa|bbbb)/.test(x.id)).map((x) => ({ id: x.id, snippet: x.snippet, count: x.count, title: x.title, cwd: x.cwd }));
    { const a = pick(js), b = pick(r); let i = a.findIndex((x, k) => JSON.stringify(x) !== JSON.stringify(b[k])); if (i < 0 && b.length < a.length) i = b.length;   // core.js stops after 4 seconds, so Rust may know more results, never fewer
     if (i < 0) ok('3 search "' + q + '"', true); else ok('3 search "' + q + '"', false, 'at ' + i + ' of ' + a.length + '/' + b.length + ' js=' + JSON.stringify(a[i]) + ' rust=' + JSON.stringify(b[i])); }
  }
  let ubad = 0; shown = 0;
  const batch = files.slice(0, REAL ? 30 : 99), got = (await post('/usage', { files: batch.map((f) => f.file) })).j;
  batch.forEach((f, i) => { const js = core.parseUsage(f.file); if (!js) return; try { assert.deepStrictEqual(JSON.parse(JSON.stringify(js)), got[i]); } catch { ubad++; if (shown++ < 2) console.log('  usage differs', f.file); } });
  ok('4 usage of the chats matches', ubad === 0, ubad + ' differ');
  console.log(`\n${pass} passed, ${fail} failed`); rs.kill(); try { fs.rmSync(T, { recursive: true, force: true }); } catch { /* */ } process.exit(fail ? 1 : 0);
})().catch((e) => { console.error(e); rs.kill(); process.exit(1); });
