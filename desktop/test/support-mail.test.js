// A support mail becomes a notification event; "list" saves it (and tries Google Tasks), "claude" starts a chat, "ignore" does nothing. Run: node desktop/test/support-mail.test.js
const { spawn } = require('child_process');
const fs = require('fs'), os = require('os'), path = require('path'), http = require('http');
if (process.platform === 'win32') { console.log('skipped on Windows'); process.exit(0); }
fs.mkdirSync(path.join(os.homedir(), '.cache'), { recursive: true }); const T = fs.mkdtempSync(path.join(os.homedir(), '.cache', 'hearth-supmail-'));
for (const d of ['bin', 'home', 'proj']) fs.mkdirSync(path.join(T, d), { recursive: true });
fs.writeFileSync(path.join(T, 'bin', 'claude'), `#!/usr/bin/env node
let t = ''; process.stdin.on('data', (d) => { t += d; }); process.stdin.on('end', async () => { require('fs').appendFileSync(process.env.HOME + '/prompts.txt', t + '\\n----\\n');
  if (/USE LIST TOOLS/.test(t)) { const a = process.argv, cfg = JSON.parse(require('fs').readFileSync(a[a.indexOf('--mcp-config') + 1], 'utf8')).mcpServers.meter.env, post = (b) => fetch('http://127.0.0.1:' + cfg.CM_RELAY_PORT + '/mcp/hub', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ job: cfg.CM_JOB, ...b }) }).then((r) => r.json()); const l = await post({ tool: 'bug_list' }); require('fs').appendFileSync(process.env.HOME + '/tools.txt', JSON.stringify(l) + '\\n'); const f1 = await post({ tool: 'bug_fixed', id: 'gt:TASK-OTHER', note: 'done' }); require('fs').appendFileSync(process.env.HOME + '/tools.txt', JSON.stringify(f1) + '\\n'); const f2 = await post({ tool: 'bug_fixed', id: 'HRT-20001', note: 'Fixed it.' }); require('fs').appendFileSync(process.env.HOME + '/tools.txt', JSON.stringify(f2) + '\\n'); }
  console.log(JSON.stringify({ type: 'system', subtype: 'init', session_id: 'sup-' + process.pid })); console.log(JSON.stringify({ type: 'result', is_error: false, result: 'ok' })); });
`, { mode: 0o755 });
const calls = [];
const supportCalls = [];
const fakeSupport = http.createServer((q, r) => { let b = ''; q.on('data', (c) => { b += c; }); q.on('end', () => { supportCalls.push({ url: q.url, auth: q.headers.authorization, body: b }); r.end('{}'); }); }).listen(47796, '127.0.0.1');
const gtItems = [{ id: 'TASK-OTHER', title: 'mcp server', notes: 'make it easy', status: 'needsAction' }, { id: 'TASK-SUP', title: 'Support HRT-20001: Crash', notes: 'x', status: 'needsAction' }];
const google = http.createServer((req, res) => { let b = ''; req.on('data', (c) => { b += c; }); req.on('end', () => { calls.push(req.method + ' ' + req.url + ' ' + b.slice(0, 200)); res.setHeader('Content-Type', 'application/json');
  if (req.url.startsWith('/token')) return res.end('{"access_token":"tok"}');
  if (req.method === 'GET') return res.end(JSON.stringify({ items: gtItems.filter((x) => x.status !== 'completed') }));
  if (req.method === 'PATCH') { const id = decodeURIComponent(req.url.split('/').pop()), t = gtItems.find((x) => x.id === id); if (t) t.status = 'completed'; return res.end('{}'); }
  res.end('{"id":"1"}'); }); }).listen(47797, '127.0.0.1');
const env = { ...process.env, HOME: path.join(T, 'home'), PATH: path.join(T, 'bin') + path.delimiter + process.env.PATH, HEARTH_SUPPORT_URL: 'http://127.0.0.1:47796', HEARTH_GOOGLE_TOKEN_URL: 'http://127.0.0.1:47797/token', HEARTH_GOOGLE_TASKS_URL: 'http://127.0.0.1:47797/tasks' };
const relay = spawn('node', [path.join(__dirname, '..', 'relay', 'standalone.js'), '--dir', path.join(T, 'd'), '--port', '47799', '--host', '127.0.0.1', '--projects', path.join(T, 'proj')], { env, stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + x : '')); };
const req = (method, url, tok, body, onData) => new Promise((res) => { const d = body ? Buffer.from(JSON.stringify(body)) : null; const r = http.request({ host: '127.0.0.1', port: 47799, path: url, method, headers: { Authorization: 'Bearer ' + tok, ...(d ? { 'Content-Type': 'application/json', 'Content-Length': d.length } : {}) } }, (rs) => { let b = ''; rs.on('data', (c) => { b += c; if (onData) onData(b); }); rs.on('end', () => res({ code: rs.statusCode, body: b })); }); r.on('error', () => res({ code: 0, body: '' })); if (d) r.write(d); r.end(); });
const J = (r) => { try { return JSON.parse(r.body); } catch { return {}; } };
const evs = (b) => b.split('\n\n').filter((x) => x.includes('data: ')).map((x) => x.split('\n').find((l) => l.startsWith('data: '))).filter(Boolean).map((l) => { try { return JSON.parse(l.slice(6)); } catch { return {}; } });
(async () => {
  await sleep(2000); const tok = JSON.parse(fs.readFileSync(path.join(T, 'd', 'relay.json'))).token;
  let events = ''; req('GET', '/events', tok, null, (b) => { events = b; });
  const mail = (id, extra) => ({ kind: 'new', id, title: 'App crashes', category: 'bug', from: 'friend@example.com', text: 'It crashes when I open settings.', ...extra });
  ok('1 only the owner can hand in a support mail', (await req('POST', '/support/incoming', 'wrong', mail('HRT-1'))).code === 401);
  ok('2 a support mail is accepted', (await req('POST', '/support/incoming', tok, mail('HRT-10001'))).code === 200);
  await sleep(500); let s = evs(events).filter((e) => e.kind === 'support');
  ok('3 it is published with a request id, ticket and short text', s.length === 1 && !!s[0].req && s[0].ticket === 'HRT-10001' && /new bug report from friend@example.com/i.test(s[0].text), JSON.stringify(s[0] || {}).slice(0, 140));
  await req('POST', '/decision', tok, { req: s[0].req, behavior: 'ignore' }); await sleep(500);
  ok('4 ignore does nothing: no chat, nothing on the list', !fs.existsSync(path.join(T, 'home', 'prompts.txt')) && (await req('GET', '/support/list', tok)).body === '[]');
  await req('POST', '/support/incoming', tok, mail('HRT-10002')); await sleep(400); s = evs(events).filter((e) => e.kind === 'support' && e.ticket === 'HRT-10002');
  await req('POST', '/decision', tok, { req: s[0].req, behavior: 'list' }); await sleep(800);
  let list = JSON.parse((await req('GET', '/support/list', tok)).body);
  ok('5 add to list without Google: saved locally, and tells why Google was skipped', list.length === 1 && list[0].id === 'HRT-10002' && list[0].google === false && evs(events).some((e) => e.kind === 'support' && /Google Tasks: .*not connected/.test(e.text || '')));
  ok('6 saving Google Tasks settings works and is shown', (await req('POST', '/admin/options', tok, { googleTasks: { clientId: 'cid', clientSecret: 'sec', refreshToken: 'ref' } })).body.includes('"googleTasks":true'));
  await req('POST', '/support/incoming', tok, mail('HRT-10003')); await sleep(400); s = evs(events).filter((e) => e.kind === 'support' && e.ticket === 'HRT-10003');
  await req('POST', '/decision', tok, { req: s[0].req, behavior: 'list' }); await sleep(1000);
  ok('7 with Google connected a task is created', calls.some((c) => c.includes('/token') && /refresh_token=ref/.test(c)) && calls.some((c) => c.startsWith('POST /tasks') && /HRT-10003/.test(c)), calls.join(' | ').slice(0, 200));
  ok('8 and the user is told', evs(events).some((e) => e.kind === 'support' && /Added to your Google Tasks/.test(e.text || '')));
  await req('POST', '/support/incoming', tok, mail('HRT-10004')); await sleep(400); s = evs(events).filter((e) => e.kind === 'support' && e.ticket === 'HRT-10004');
  await req('POST', '/decision', tok, { req: s[0].req, behavior: 'claude' }); await sleep(1500);
  const pr = fs.existsSync(path.join(T, 'home', 'prompts.txt')) ? fs.readFileSync(path.join(T, 'home', 'prompts.txt'), 'utf8') : '';
  ok('9 give to Claude starts a chat with the mail in it', /HRT-10004/.test(pr) && /It crashes when I open settings/.test(pr) && /Do not email anyone/.test(pr));
  const ev1 = await req('GET', '/pending', tok);
  ok('10 a handled mail is no longer pending', !ev1.body.includes('HRT-10004'));
  await req('POST', '/support/incoming', tok, mail('HRT-10005')); await sleep(300);
  ok('11 an unhandled one is replayed to apps that connect later', (await req('GET', '/pending', tok)).body.includes('HRT-10005'));
  ok('12 from the support screen: bad input refused', (await req('POST', '/support/act', tok, { id: 'HRT-1', action: 'delete' })).code === 400);
  const before2 = JSON.parse((await req('GET', '/support/list', tok)).body).length;
  const act1 = J(await req('POST', '/support/act', tok, { id: 'HRT-20001', title: 'Crash', category: 'bug', from: 'a@b.c', text: 'It crashes.', action: 'list' }));
  const after2 = JSON.parse((await req('GET', '/support/list', tok)).body);
  ok('13 "add to tasks" from the screen saves it and reports', act1.ok === true && after2.length === before2 + 1 && /Google Tasks/.test(act1.text), act1.text);
  ok('14 doing it twice does not add it twice', J(await req('POST', '/support/act', tok, { id: 'HRT-20001', title: 'Crash', from: 'a@b.c', text: 'It crashes.', action: 'list' })).text === 'Already on your list.');
  const n0 = fs.readFileSync(path.join(T, 'home', 'prompts.txt'), 'utf8').split('----').length;
  await req('POST', '/support/act', tok, { id: 'HRT-20002', title: 'Slow', category: 'bug', from: 'a@b.c', text: 'It is slow on start.', action: 'claude' }); await sleep(1500);
  ok('15 "fix with Claude" from the screen starts a chat', fs.readFileSync(path.join(T, 'home', 'prompts.txt'), 'utf8').includes('It is slow on start.'));
  ok('16 a guest key cannot use it', (await req('POST', '/support/act', 'share.abcd.abcdefghijklmnop', { id: 'x', action: 'list' })).code === 401);
  // the person who wrote is told by mail: when it is put on the list, and when it is fixed
  const listedCalls = supportCalls.filter((c) => /"event":"listed"/.test(c.body));
  ok('17 putting a request on the list tells the support desk to mail the person', listedCalls.length >= 1 && listedCalls[0].url === '/owner/notify' && listedCalls[0].auth === 'Bearer ' + tok && /HRT-20001|HRT-10003|HRT-10002/.test(listedCalls[0].body), JSON.stringify(listedCalls[0] || {}).slice(0, 120));
  ok('18 ignore and "fix with Claude" do not mail anyone', !supportCalls.some((c) => /HRT-10001|HRT-20002/.test(c.body)));
  const lst = JSON.parse((await req('GET', '/support/list', tok)).body).find((x) => /^HRT-1000/.test(x.id));
  // the list is the Google Tasks list: Claude sees everything on it and checks things off
  await req('POST', '/support/act', tok, { id: 'HRT-20001', title: 'Crash', category: 'bug', from: 'a@b.c', text: 'It crashes.', action: 'list' });
  fs.rmSync(path.join(T, 'home', 'tools.txt'), { force: true });
  await req('POST', '/sessions/new/send', tok, { text: 'USE LIST TOOLS', cwd: path.join(T, 'proj') }); await sleep(2500);
  const tl = fs.existsSync(path.join(T, 'home', 'tools.txt')) ? fs.readFileSync(path.join(T, 'home', 'tools.txt'), 'utf8').trim().split('\n').map((l) => JSON.parse(l).text) : [];
  ok('19 the list shows support mails and the other tasks of Google Tasks, each once', tl[0] && /\[gt:TASK-OTHER\] mcp server/.test(tl[0]) && /\[HRT-20001\]/.test(tl[0]) && !/\[gt:TASK-SUP\]/.test(tl[0]), (tl[0] || '').slice(0, 160));
  ok('20 checking off a Google task completes it', /Checked off in Google Tasks/.test(tl[1] || '') && gtItems.find((x) => x.id === 'TASK-OTHER').status === 'completed');
  ok('21 checking off a support mail also completes its task and tells the person', /Marked HRT-20001 as fixed and checked off/.test(tl[2] || '') && gtItems.find((x) => x.id === 'TASK-SUP').status === 'completed' && supportCalls.some((c) => /"event":"fixed"/.test(c.body) && /HRT-20001/.test(c.body)));
  relay.kill(); google.close(); fakeSupport.close(); await sleep(300); fs.rmSync(T, { recursive: true, force: true });
  console.log(`\n${pass} passed, ${fail} failed`); process.exit(fail ? 1 : 0);
})();
