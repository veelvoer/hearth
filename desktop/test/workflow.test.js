// Workflows: steps run one after another for their time, a stopped run is continued, the bug list tools. Fake "claude". Run: node desktop/test/workflow.test.js
const { spawn } = require('child_process');
const fs = require('fs'), os = require('os'), path = require('path'), http = require('http');
if (process.platform === 'win32') { console.log('skipped on Windows'); process.exit(0); }
fs.mkdirSync(path.join(os.homedir(), '.cache'), { recursive: true }); const T = fs.mkdtempSync(path.join(os.homedir(), '.cache', 'hearth-wf-'));
for (const d of ['bin', 'home', 'proj', 'proj/app']) fs.mkdirSync(path.join(T, d), { recursive: true });
fs.writeFileSync(path.join(T, 'bin', 'claude'), `#!/usr/bin/env node
const fs = require('fs'), path = require('path');
let text = ''; process.stdin.on('data', (d) => { text += d; }); process.stdin.on('end', () => {
  const a = process.argv, r = a.indexOf('--resume'), sid = r >= 0 ? a[r + 1] : 'wf-' + process.pid;
  const pd = path.join(process.env.HOME, '.claude', 'projects', process.cwd().replace(/[^A-Za-z0-9]/g, '-')); fs.mkdirSync(pd, { recursive: true });
  fs.appendFileSync(path.join(pd, sid + '.jsonl'), JSON.stringify({ type: 'user', cwd: process.cwd(), sessionId: sid, timestamp: new Date().toISOString(), message: { role: 'user', content: text.trim() } }) + '\\n');
  fs.appendFileSync(path.join(process.env.HOME, 'prompts.txt'), text.trim().split('\\n')[0] + '\\n');
  fs.appendFileSync(path.join(process.env.HOME, 'args.txt'), a.join(' ') + '\\n');
  console.log(JSON.stringify({ type: 'system', subtype: 'init', session_id: sid })); console.log(JSON.stringify({ type: 'result', is_error: false, result: 'ok' }));
});
`, { mode: 0o755 });
const env = { ...process.env, HOME: path.join(T, 'home'), PATH: path.join(T, 'bin') + path.delimiter + process.env.PATH, HEARTH_WF_MINUTE_MS: '2500', HEARTH_WF_TICK_MS: '400' };
const relay = spawn('node', [path.join(__dirname, '..', 'relay', 'standalone.js'), '--dir', path.join(T, 'd'), '--port', '47795', '--host', '127.0.0.1', '--projects', path.join(T, 'proj')], { env, stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + x : '')); };
const req = (method, url, tok, body) => new Promise((res) => { const d = body ? Buffer.from(JSON.stringify(body)) : null; const r = http.request({ host: '127.0.0.1', port: 47795, path: url, method, headers: { Authorization: 'Bearer ' + tok, ...(d ? { 'Content-Type': 'application/json', 'Content-Length': d.length } : {}) } }, (rs) => { let b = ''; rs.on('data', (c) => { b += c; }); rs.on('end', () => res({ code: rs.statusCode, body: b })); }); r.on('error', () => res({ code: 0, body: '' })); if (d) r.write(d); r.end(); });
const J = (r) => { try { return JSON.parse(r.body); } catch { return {}; } };
const prompts = () => { try { return fs.readFileSync(path.join(T, 'home', 'prompts.txt'), 'utf8').trim().split('\n'); } catch { return []; } };
(async () => {
  await sleep(2000); const tok = JSON.parse(fs.readFileSync(path.join(T, 'd', 'relay.json'))).token;
  const proj = path.join(T, 'proj', 'app');
  await req('POST', '/sessions/new/send', tok, { text: 'hello', cwd: proj }); await sleep(800);
  const sid = J(await req('GET', '/sessions', tok))[0].id;
  ok('1 no workflow at first', J(await req('GET', `/sessions/${sid}/workflow`, tok)).state === 'none');
  ok('2 empty workflow refused', (await req('POST', `/sessions/${sid}/workflow`, tok, { steps: [{ name: 'x', prompt: '', minutes: 1 }] })).code === 400);
  ok('3 a guest key cannot start one', (await req('POST', `/sessions/${sid}/workflow`, 'share.abcd.abcdefghijklmnop', { steps: [{ name: 'a', prompt: 'b', minutes: 1 }] })).code === 401);
  const before = prompts().length;
  const st = await req('POST', `/sessions/${sid}/workflow`, tok, { steps: [{ name: 'Bug test', prompt: 'Test everything and list bugs.', minutes: 1 }, { name: 'Bug fix', prompt: 'Fix all the bugs.', minutes: 1 }] });
  ok('4 it starts', st.code === 200 && J(st).state === 'running' && J(st).step === 'Bug test', st.body.slice(0, 100));
  await sleep(1800); const p1 = prompts().slice(before);
  ok('5 Claude is started again and again while the step lasts (a run that ends early is continued)', p1.length >= 2 && p1.every((x) => /step 1 of 2/.test(x)) && /Carry on|about/i.test(p1.join(' ')), p1.join(' | ').slice(0, 200));
  await sleep(1900); const p2 = prompts().slice(before);
  ok('6 after the time it moves on to the next step', p2.some((x) => /step 2 of 2/.test(x)) && /Bug fix|Fix all/.test(fs.readFileSync(path.join(T, 'home', 'prompts.txt'), 'utf8')));
  await sleep(3200);
  ok('7 after the last step it is done and stops starting runs', J(await req('GET', `/sessions/${sid}/workflow`, tok)).state === 'done');
  const n = prompts().length; await sleep(1000); ok('8 no runs after it is done', prompts().length === n);
  await req('POST', `/sessions/${sid}/workflow`, tok, { steps: [{ name: 'Long', prompt: 'Keep testing.', minutes: 60 }] }); await sleep(600);
  ok('9 pause stops new runs', J(await req('POST', `/sessions/${sid}/workflow`, tok, { action: 'pause' })).state === 'paused'); const m = prompts().length; await sleep(1000); ok('10 nothing starts while paused', prompts().length === m);
  ok('11 resume works', J(await req('POST', `/sessions/${sid}/workflow`, tok, { action: 'resume' })).state === 'running'); await sleep(900); ok('12 and runs start again', prompts().length > m);
  ok('13 stop removes it', J(await req('POST', `/sessions/${sid}/workflow`, tok, { action: 'stop' })).state === 'none');
  const args = fs.readFileSync(path.join(T, 'home', 'args.txt'), 'utf8');
  ok('14 chats get the bug list tools', /mcp__meter__bug_list/.test(args) && /mcp__meter__bug_fixed/.test(args));
  relay.kill(); await sleep(300); fs.rmSync(T, { recursive: true, force: true });
  console.log(`\n${pass} passed, ${fail} failed`); process.exit(fail ? 1 : 0);
})();
