// The recap, saved chat templates and the one-project summary. Run: node desktop/test/extras.test.js
const { spawn, spawnSync } = require('child_process');
const fs = require('fs'), os = require('os'), path = require('path'), http = require('http');
if (process.platform === 'win32') { console.log('skipped on Windows'); process.exit(0); }
fs.mkdirSync(path.join(os.homedir(), '.cache'), { recursive: true }); const T = fs.mkdtempSync(path.join(os.homedir(), '.cache', 'hearth-extras-'));
const proj = path.join(T, 'proj', 'app'); fs.mkdirSync(proj, { recursive: true }); spawnSync('git', ['init', '-q', proj]); fs.writeFileSync(path.join(proj, 'a.txt'), 'x');
const home = path.join(T, 'home', '.claude', 'projects', proj.replace(/[^A-Za-z0-9]/g, '-')); fs.mkdirSync(home, { recursive: true });
const line = (id, ts, extra) => JSON.stringify({ type: 'assistant', cwd: proj, sessionId: id, timestamp: ts, message: { id: 'm' + id, role: 'assistant', model: 'claude-sonnet-4', usage: { input_tokens: 100, output_tokens: 50000 }, content: [{ type: 'text', text: 'hi' }] }, ...extra }) + '\n';
const u = (id, ts) => JSON.stringify({ type: 'user', cwd: proj, sessionId: id, timestamp: ts, message: { role: 'user', content: 'do ' + id } }) + '\n';
fs.writeFileSync(path.join(home, 'c1.jsonl'), u('c1', new Date().toISOString()) + line('c1', new Date().toISOString())); fs.writeFileSync(path.join(home, 'c2.jsonl'), u('c2', new Date(Date.now() - 864e5).toISOString()) + line('c2', new Date(Date.now() - 864e5).toISOString()));
const relay = spawn('node', [path.join(__dirname, '..', 'relay', 'standalone.js'), '--dir', path.join(T, 'd'), '--port', '47792', '--host', '127.0.0.1', '--projects', path.join(T, 'proj')], { env: { ...process.env, HOME: path.join(T, 'home'), HEARTH_TEST: '1', HEARTH_MEM_CHILD: '1' }, stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + x : '')); };
const req = (m, p, tok, body) => new Promise((res) => { const d = body ? Buffer.from(JSON.stringify(body)) : null; const r = http.request({ host: '127.0.0.1', port: 47792, path: p, method: m, headers: { Authorization: 'Bearer ' + tok, ...(d ? { 'Content-Type': 'application/json', 'Content-Length': d.length } : {}) } }, (rs) => { let b = ''; rs.on('data', (x) => { b += x; }); rs.on('end', () => { let j = {}; try { j = JSON.parse(b); } catch { /* */ } res({ code: rs.statusCode, j }); }); }); r.on('error', () => res({ code: 0, j: {} })); if (d) r.write(d); r.end(); });
(async () => {
  const tok = await require('./_wait')(path.join(T, 'd', 'relay.json'));
  const rc = (await req('GET', '/recap?days=7', tok)).j;
  ok('1 the recap counts the days, prompts and cost', rc.days === 7 && rc.activeDays === 2 && rc.prompts === 2 && rc.cost > 0 && rc.busiest && rc.empty === false, JSON.stringify(rc).slice(0, 160));
  ok('2 a quiet period says it is empty', (await req('GET', '/recap?days=1', tok)).j.activeDays <= 1);
  const t1 = await req('POST', '/templates', tok, { add: { name: 'Review', text: 'Review my last change and tell me what is risky.' } });
  ok('3 a template is saved and listed', t1.code === 200 && (await req('GET', '/templates', tok)).j.templates[0].name === 'Review');
  ok('4 a template needs a name and a text', (await req('POST', '/templates', tok, { add: { name: '', text: 'x' } })).code === 400);
  await req('POST', '/templates', tok, { add: { name: 'Review', text: 'Newer text' } });
  ok('5 the same name replaces the old one', (await req('GET', '/templates', tok)).j.templates.length === 1 && (await req('GET', '/templates', tok)).j.templates[0].text === 'Newer text');
  const id = (await req('GET', '/templates', tok)).j.templates[0].id; await req('POST', '/templates', tok, { remove: id });
  ok('6 and removed', (await req('GET', '/templates', tok)).j.templates.length === 0);
  const ps = (await req('GET', '/projects/summary?cwd=' + encodeURIComponent(proj), tok)).j;
  ok('7 a project summary: chats, cost, and the changes of its git repository', ps.name === 'app' && ps.chats === 2 && ps.cost > 0 && ps.changed === 1, JSON.stringify(ps).slice(0, 160));
  await req('POST', '/admin/options', tok, { outside: false });
  ok('8 a folder outside the projects folder is refused', (await req('GET', '/projects/summary?cwd=/etc', tok)).code === 403);
  ok('9 guests cannot read any of it', (await req('GET', '/recap', 'share.abcd.abcdefghijklmnop')).code === 401 && (await req('GET', '/templates', 'share.abcd.abcdefghijklmnop')).code === 401);
  relay.kill(); await sleep(300); fs.rmSync(T, { recursive: true, force: true });
  console.log(`\n${pass} passed, ${fail} failed`); process.exit(fail ? 1 : 0);
})();
