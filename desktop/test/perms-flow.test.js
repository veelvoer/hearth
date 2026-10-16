// Always-allowed commands, Full auto while Claude works, and working outside the projects folder. Uses a fake "claude". Run: node desktop/test/perms-flow.test.js
const { spawn } = require('child_process');
const fs = require('fs'), os = require('os'), path = require('path'), http = require('http');
if (process.platform === 'win32') { console.log('skipped on Windows'); process.exit(0); }
fs.mkdirSync(path.join(os.homedir(), '.cache'), { recursive: true }); const T = fs.mkdtempSync(path.join(os.homedir(), '.cache', 'hearth-perms-'));
for (const d of ['bin', 'home', 'proj', 'proj/app', 'elsewhere']) fs.mkdirSync(path.join(T, d), { recursive: true });
fs.writeFileSync(path.join(T, 'bin', 'claude'), `#!/usr/bin/env node
const fs = require('fs'), cp = require('child_process');
const a = process.argv, sf = a[a.indexOf('--settings') + 1];
const hook = JSON.parse(fs.readFileSync(sf, 'utf8')).hooks.PermissionRequest[0].hooks[0].command;
const sid = 'fake-' + process.pid + '-' + Math.floor(Math.random() * 1e6);
let text = ''; process.stdin.on('data', (d) => { text += d; }); process.stdin.on('end', () => {
  const pd = require('path').join(process.env.HOME, '.claude', 'projects', process.cwd().replace(/[^A-Za-z0-9]/g, '-')); fs.mkdirSync(pd, { recursive: true });
  fs.writeFileSync(require('path').join(pd, sid + '.jsonl'), JSON.stringify({ type: 'user', cwd: process.cwd(), timestamp: new Date().toISOString(), message: { content: 'fake chat ' + text.trim() } }) + '\\n');
  console.log(JSON.stringify({ type: 'system', subtype: 'init', session_id: sid, model: 'fake' }));
  const r = cp.spawnSync('sh', ['-c', hook], { input: JSON.stringify({ hook_event_name: 'PermissionRequest', session_id: sid, tool_name: 'Bash', tool_input: { command: text.trim() }, cwd: process.cwd() }), encoding: 'utf8' });
  console.log(JSON.stringify({ type: 'stream_event', event: { delta: { type: 'text_delta', text: 'RESULT ' + (r.stdout.includes('"allow"') ? 'allowed' : 'not-allowed') + ' cwd=' + process.cwd() + (a.join(' ').includes('running as root') ? ' ROOTPROMPT' : '') + (a.join(' ').includes('WITHOUT root rights') ? ' DROPPED' : '') + (a[a.indexOf('--permission-mode') + 1] === 'bypassPermissions' ? ' BYPASS' : '') } } }));
  console.log(JSON.stringify({ type: 'result', is_error: false, result: 'done' }));
});
`, { mode: 0o755 });
const SUDOLOG = path.join(T, 'sudo.log');
fs.writeFileSync(path.join(T, 'bin', 'sudo'), `#!/bin/sh
printf '%s\\n' "$*" >> ${SUDOLOG}
if [ "$1" = "-n" ]; then shift; fi
case "$1" in chown|pkill) exit 0;; esac
if [ "$1" = env ]; then for a in "$@"; do [ "$a" = id ] && { echo 0; exit 0; }; done; fi
exec "$@"
`, { mode: 0o755 });   // stands in for passwordless sudo: logs what it was asked, answers "id -u" with 0, does not really chown or kill
const env = { ...process.env, HOME: path.join(T, 'home'), PATH: path.join(T, 'bin') + path.delimiter + process.env.PATH };
const relay = spawn('node', [path.join(__dirname, '..', 'relay', 'standalone.js'), '--dir', path.join(T, 'd'), '--port', '47794', '--host', '127.0.0.1', '--projects', path.join(T, 'proj'), '--allow-bypass', '--no-memory-limit'], { env, stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + x : '')); };
const req = (method, url, tok, body, onData) => new Promise((res) => { const d = body ? Buffer.from(JSON.stringify(body)) : null; const r = http.request({ host: '127.0.0.1', port: 47794, path: url, method, headers: { Authorization: 'Bearer ' + tok, ...(d ? { 'Content-Type': 'application/json', 'Content-Length': d.length } : {}) } }, (x) => { let b = ''; x.on('data', (c) => { b += c; if (onData) onData(b); }); x.on('end', () => res({ code: x.statusCode, body: b })); }); r.on('error', () => res({ code: 0, body: '' })); if (d) r.write(d); r.end(); });
const J = (r) => { try { return JSON.parse(r.body); } catch { return {}; } };
const evs = (b) => b.split('\n\n').filter((x) => x.startsWith('id: ')).map((x) => x.split('\n').find((l) => l.startsWith('data: '))).filter(Boolean).map((l) => JSON.parse(l.slice(6)));
(async () => {
  await sleep(2000); const tok = JSON.parse(fs.readFileSync(path.join(T, 'd', 'relay.json'))).token;
  let events = ''; req('GET', '/events', tok, null, (b) => { events = b; });
  const proj = path.join(T, 'proj', 'app');
  const run = (text, cwd) => { const o = { stream: '' }; o.done = req('POST', '/sessions/new/send', tok, { text, cwd: cwd || proj }, (b) => { o.stream = b; }); return o; };
  // 1) the command is asked, "don't ask again" stores a rule, and the same kind of command is not asked again
  let a = run('npm test -- login'); await sleep(1500);
  let ev = evs(events).filter((e) => e.kind === 'permission');
  ok('1 the first command is asked, with a suggested rule', ev.length === 1 && ev[0].suggest && ev[0].suggest.label === 'npm test …', JSON.stringify(ev[0] && ev[0].suggest));
  await req('POST', '/decision', tok, { req: ev[0].req, behavior: 'allow_always' }); await a.done;
  ok('2 it was allowed', /RESULT allowed/.test(a.stream));
  let o = J(await req('GET', '/admin/options', tok)); ok('3 the rule is stored', o.rules.length === 1 && o.rules[0].prefix === 'npm test');
  const before = evs(events).filter((e) => e.kind === 'permission').length;
  a = run('npm test -- other'); await a.done;
  ok('4 the same kind of command is allowed without asking', /RESULT allowed/.test(a.stream) && evs(events).filter((e) => e.kind === 'permission').length === before);
  a = run('npm test; rm -rf ~'); await sleep(1500);
  ok('5 a compound command is asked again', evs(events).filter((e) => e.kind === 'permission').length === before + 1);
  // 2) Full auto while Claude is working: the waiting approval is allowed at once
  const pend = evs(events).filter((e) => e.kind === 'permission').pop();
  const au = await req('POST', `/sessions/${pend.session}/auto`, tok, { on: true }); ok('6 Full auto can be switched on during a run', au.code === 200 && J(au).auto === true);
  await a.done; ok('7 the waiting approval was allowed', /RESULT allowed/.test(a.stream));
  const n0 = evs(events).filter((e) => e.kind === 'permission').length;
  a = run('make deploy-everything'); await sleep(1500);
  const ev8 = evs(events).filter((e) => e.kind === 'permission'); ok('8 a new chat is not on Full auto: it asks', ev8.length === n0 + 1);
  await req('POST', '/decision', tok, { req: ev8[ev8.length - 1].req, behavior: 'deny' }); await a.done;
  await req('POST', '/admin/options', tok, { removeRule: o.rules[0].id }); o = J(await req('GET', '/admin/options', tok)); ok('9 a rule can be removed', o.rules.length === 0);
  const bad = await req('POST', '/admin/options', tok, { addPrefix: 'rm -rf' }); ok('10 a dangerous rule cannot be added', bad.code === 400);
  // 3) work outside the projects folder
  const fsr = J(await req('GET', `/fs?path=${encodeURIComponent(path.join(T))}`, tok)); ok('11 the server can be browsed', fsr.dirs && fsr.dirs.some((d) => d.name === 'elsewhere'));
  const mk = await req('POST', '/fs/mkdir', tok, { path: path.join(T, 'elsewhere'), name: 'newproject' }); ok('12 a folder can be made', mk.code === 200 && fs.existsSync(path.join(T, 'elsewhere', 'newproject')));
  const bn = await req('POST', '/fs/mkdir', tok, { path: path.join(T, 'elsewhere'), name: '../x' }); ok('13 a folder name with a slash is refused', bn.code === 400);
  const out = path.join(T, 'elsewhere', 'newproject'); a = run('ls', out); await sleep(1500);
  { const pe = evs(events).filter((e) => e.kind === 'permission').pop(); await req('POST', '/decision', tok, { req: pe.req, behavior: 'allow' }); } await a.done;
  ok('14 a chat can start outside the projects folder', new RegExp('cwd=' + out.replace(/[/.]/g, '.')).test(a.stream), a.stream.slice(-120));
  const list = J(await req('GET', '/sessions', tok)); ok('15 that chat shows in the list', Array.isArray(list) && list.some((x) => x.cwd === fs.realpathSync(out)), JSON.stringify(list.map((x) => x.cwd)));
  await req('POST', '/admin/options', tok, { outside: false });
  a = run('ls', out); await a.done; ok('16 it is refused again when the option is off', /folder/.test(a.stream) || a.stream === '' || !/RESULT/.test(a.stream));
  // 4) chats with root rights
  await req('POST', '/admin/options', tok, { outside: true });
  const off = await req('POST', '/sessions/new/send', tok, { text: 'ls', cwd: proj, root: true }); ok('17 root is refused while it is off', off.code === 403 && /root rights are off/.test(off.body));
  const o2 = J(await req('POST', '/admin/options', tok, { root: true, recheck: true })); ok('18 root can be switched on and sudo is found by really running it', o2.root === true && o2.rootReady && o2.rootReady.ok === true && /sudo works/.test(o2.rootReady.how));
  ok('18b the check ran the same form of command as a root chat', /-n env HOME=.* id -u/.test(fs.readFileSync(SUDOLOG, 'utf8')), JSON.stringify(fs.readFileSync(SUDOLOG, 'utf8')).slice(0, 300));
  a = run('ls', proj); a.stream = ''; const rr = { stream: '' }; rr.done = req('POST', '/sessions/new/send', tok, { text: 'make it so', cwd: proj, root: true, mode: 'bypassPermissions' }, (b) => { rr.stream = b; }); await sleep(1500);
  const pe = evs(events).filter((e) => e.kind === 'permission'); for (const e of pe) await req('POST', '/decision', tok, { req: e.req, behavior: 'allow' }); await rr.done; await a.done;
  ok('20 the root chat knows it has root', /ROOTPROMPT/.test(rr.stream), rr.stream.slice(-160));
  ok('21 Full auto is not passed to Claude as root (Hearth answers the approvals itself)', !/BYPASS/.test(rr.stream) && /RESULT allowed/.test(rr.stream));
  const ls2 = J(await req('GET', '/sessions', tok)); ok('22 the chat is marked root in the list', Array.isArray(ls2) && ls2.some((x) => x.root === true));
  const rid = (ls2.find((x) => x.root === true) || {}).id;
  await sleep(500); ok('23 after a root run the Claude folder is given back to you', /-n chown -R \d+:\d+ .*\.claude/.test(fs.readFileSync(SUDOLOG, 'utf8')));
  ok('24 the root run went through sudo with the right folder and environment', /-n env HOME=.* PATH=.* .*claude /.test(fs.readFileSync(SUDOLOG, 'utf8')));
  const off1 = await req('POST', `/sessions/${rid}/root`, tok, { on: false }); ok('25 an existing chat can lose its root rights', off1.code === 200 && J(await req('GET', '/sessions', tok)).find((x) => x.id === rid).root === false);
  const on1 = await req('POST', `/sessions/${rid}/root`, tok, { on: true }); ok('26 and get them back', on1.code === 200 && J(await req('GET', '/sessions', tok)).find((x) => x.id === rid).root === true);
  // root is switched off while that chat has root rights: the chat is not lost, it goes on without root
  await req('POST', '/admin/options', tok, { root: false });
  const cont = { stream: '' }; cont.done = req('POST', `/sessions/${rid}/send`, tok, { text: 'carry on', mode: 'acceptEdits' }, (b) => { cont.stream = b; }); await sleep(1500);
  { const pe2 = evs(events).filter((e) => e.kind === 'permission'); const last2 = pe2[pe2.length - 1]; if (last2) await req('POST', '/decision', tok, { req: last2.req, behavior: 'allow' }); } await cont.done;
  ok('27 an old root chat goes on without root, and Claude is told so, when root is switched off', /RESULT/.test(cont.stream) && !/ROOTPROMPT/.test(cont.stream) && /DROPPED/.test(cont.stream), cont.stream.slice(-140));
  ok('28 giving a chat root rights is refused while root is off', (await req('POST', `/sessions/${rid}/root`, tok, { on: true })).code === 403);
  ok('29 a guest key cannot touch root', (await req('POST', `/sessions/${rid}/root`, 'share.abcd.abcdefghijklmnop', { on: true })).code === 401);
  relay.kill(); await sleep(300); fs.rmSync(T, { recursive: true, force: true });
  console.log(`\n${pass} passed, ${fail} failed`); process.exit(fail ? 1 : 0);
})();
