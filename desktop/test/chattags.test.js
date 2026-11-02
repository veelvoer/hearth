// Pins and names of chats are kept on the server, so the phone and the computer show the same. Run: node desktop/test/chattags.test.js
const { spawn } = require('child_process');
const fs = require('fs'), os = require('os'), path = require('path'), http = require('http');
if (process.platform === 'win32') { console.log('skipped on Windows'); process.exit(0); }
fs.mkdirSync(path.join(os.homedir(), '.cache'), { recursive: true }); const T = fs.mkdtempSync(path.join(os.homedir(), '.cache', 'hearth-tag-'));
const proj = path.join(T, 'proj', 'app'), P = path.join(T, 'home', '.claude', 'projects', proj.replace(/[^A-Za-z0-9]/g, '-')); fs.mkdirSync(proj, { recursive: true }); fs.mkdirSync(P, { recursive: true });
const mk = (id, user, reply) => fs.writeFileSync(path.join(P, id + '.jsonl'), [JSON.stringify({ type: 'user', cwd: proj, timestamp: '2026-10-20T10:00:00Z', message: { content: user } }), JSON.stringify({ type: 'assistant', cwd: proj, timestamp: '2026-10-20T10:00:05Z', message: { role: 'assistant', content: [{ type: 'text', text: reply }] } })].join('\n') + '\n');
mk('one', 'fix the login page', 'I changed the Pineapple handler in auth.js'); mk('two', 'write a poem', 'Roses are red');
const relay = spawn('node', [path.join(__dirname, '..', 'relay', 'standalone.js'), '--dir', path.join(T, 'd'), '--port', '47792', '--host', '127.0.0.1', '--projects', path.join(T, 'proj')], { env: { ...process.env, HOME: path.join(T, 'home'), HEARTH_TEST: '1', HEARTH_MEM_CHILD: '1' }, stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + x : '')); };
const req = (method, url, tok, body) => new Promise((res) => { const d = body ? Buffer.from(JSON.stringify(body)) : null; const r = http.request({ host: '127.0.0.1', port: 47792, path: url, method, headers: { Authorization: 'Bearer ' + tok, ...(d ? { 'Content-Type': 'application/json', 'Content-Length': d.length } : {}) } }, (rs) => { let b = ''; rs.on('data', (x) => { b += x; }); rs.on('end', () => { let j = {}; try { j = JSON.parse(b); } catch { /* */ } res({ code: rs.statusCode, j }); }); }); r.on('error', () => res({ code: 0, j: {} })); if (d) r.write(d); r.end(); });
(async () => {
  let tok = ''; for (let i = 0; i < 40 && !tok; i++) { await sleep(400); try { tok = JSON.parse(fs.readFileSync(path.join(T, 'd', 'relay.json'))).token; } catch { /* not yet */ } }
  ok('1 starts with no tags', (await req('GET', '/chat-tags', tok)).j.tags && Object.keys((await req('GET', '/chat-tags', tok)).j.tags).length === 0);
  ok('2 a pin is kept', (await req('POST', '/chat-tags', tok, { id: 'abc', pin: true })).j.tags.abc.pin === true);
  ok('3 a name is kept next to the pin', (await req('POST', '/chat-tags', tok, { id: 'abc', name: '  Groceries ' })).j.tags.abc.name === 'Groceries' && (await req('GET', '/chat-tags', tok)).j.tags.abc.pin === true);
  ok('4 unpinning keeps the name', (await req('POST', '/chat-tags', tok, { id: 'abc', pin: false })).j.tags.abc.name === 'Groceries');
  ok('5 an empty name and no pin removes the entry', !('abc' in (await req('POST', '/chat-tags', tok, { id: 'abc', name: '' })).j.tags));
  ok('6 a strange id is refused', (await req('POST', '/chat-tags', tok, { id: '../x', pin: true })).code === 400);
  ok('7 a guest key cannot read or change tags', (await req('GET', '/chat-tags', 'share.abcd.abcdefghijklmnop')).code === 401 && (await req('POST', '/chat-tags', 'share.abcd.abcdefghijklmnop', { id: 'q', pin: true })).code === 401);
  ok('8 no key is refused', (await req('GET', '/chat-tags', 'nope')).code === 401);
  const f1 = (await req('GET', '/search?q=pineapple', tok)).j.results || [];
  ok('9 search finds words inside a chat, in any case', f1.length === 1 && f1[0].id === 'one' && /Pineapple/.test(f1[0].snippet) && f1[0].count === 1, JSON.stringify(f1).slice(0, 160));
  ok('10 search says nothing for words that are not there, or too short', ((await req('GET', '/search?q=zzzzqq', tok)).j.results || []).length === 0 && ((await req('GET', '/search?q=p', tok)).j.results || []).length === 0);
  ok('11 search matches what you wrote too', ((await req('GET', '/search?q=poem', tok)).j.results || []).map((x) => x.id).join() === 'two');
  ok('12 a guest key cannot search', (await req('GET', '/search?q=poem', 'share.abcd.abcdefghijklmnop')).code === 401);
  relay.kill(); await sleep(300); fs.rmSync(T, { recursive: true, force: true });
  console.log(`\n${pass} passed, ${fail} failed`); process.exit(fail ? 1 : 0);
})();
