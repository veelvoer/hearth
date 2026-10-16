// Shared chats: a normal chat that friends can open. Their prompts are requests the host approves (and may edit), or runs at once with full permission. Fake "claude". Run: node desktop/test/shares.test.js
const { spawn } = require('child_process');
const fs = require('fs'), os = require('os'), path = require('path'), http = require('http');
if (process.platform === 'win32') { console.log('skipped on Windows'); process.exit(0); }
fs.mkdirSync(path.join(os.homedir(), '.cache'), { recursive: true });
const T = fs.mkdtempSync(path.join(os.homedir(), '.cache', 'hearth-shares-'));
for (const d of ['bin', 'home', 'proj', 'proj/app']) fs.mkdirSync(path.join(T, d), { recursive: true });
fs.writeFileSync(path.join(T, 'bin', 'claude'), `#!/usr/bin/env node
const fs = require('fs'); const a = process.argv;
const sid = a.includes('--resume') ? a[a.indexOf('--resume') + 1] : 'fake-' + process.pid;
let text = ''; process.stdin.on('data', (d) => { text += d; }); process.stdin.on('end', () => {
  const pd = require('path').join(process.env.HOME, '.claude', 'projects', process.cwd().replace(/[^A-Za-z0-9]/g, '-')); fs.mkdirSync(pd, { recursive: true });
  const ts = () => new Date().toISOString(); const f = require('path').join(pd, sid + '.jsonl');
  const reply = 'MODE=' + a[a.indexOf('--permission-mode') + 1] + ' | ' + text.trim();
  console.log(JSON.stringify({ type: 'system', subtype: 'init', session_id: sid, model: 'fake' }));
  fs.appendFileSync(f, JSON.stringify({ type: 'user', cwd: process.cwd(), timestamp: ts(), message: { content: text.trim() } }) + '\\n');
  const hold = /HOLD/.test(text) ? 1800 : 50;
  setTimeout(() => {
    fs.appendFileSync(f, JSON.stringify({ type: 'assistant', cwd: process.cwd(), timestamp: ts(), message: { id: 'm' + Date.now(), model: 'claude-fake', content: [{ type: 'text', text: reply }], usage: { input_tokens: 1, output_tokens: 1 } } }) + '\\n');
    console.log(JSON.stringify({ type: 'result', is_error: false, result: reply }));
  }, hold);
});
`, { mode: 0o755 });
const env = { ...process.env, HOME: path.join(T, 'home'), PATH: path.join(T, 'bin') + path.delimiter + process.env.PATH };
const startRelay = () => spawn('node', [path.join(__dirname, '..', 'relay', 'standalone.js'), '--dir', path.join(T, 'd'), '--port', '47792', '--host', '127.0.0.1', '--projects', path.join(T, 'proj'), '--name', 'Warre', '--no-memory-limit'], { env, stdio: 'ignore' });
let relay = startRelay();
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + x : '')); };
const req = (method, url, tok, body, onData) => new Promise((res) => { const d = body ? Buffer.from(JSON.stringify(body)) : null; const r = http.request({ host: '127.0.0.1', port: 47792, path: url, method, headers: { ...(tok ? { Authorization: 'Bearer ' + tok } : {}), ...(d ? { 'Content-Type': 'application/json', 'Content-Length': d.length } : {}) } }, (x) => { let b = ''; x.on('data', (c) => { b += c; if (onData) onData(b); }); x.on('end', () => { let j = {}; try { j = JSON.parse(b); } catch { /* ndjson */ } res({ code: x.statusCode, j, b }); }); }); r.on('error', () => res({ code: 0, j: {}, b: '' })); if (d) r.write(d); r.end(); });
const evs = (b) => b.split('\n\n').filter((x) => x.startsWith('id: ')).map((x) => x.split('\n').find((l) => l.startsWith('data: '))).filter(Boolean).map((l) => JSON.parse(l.slice(6)));
(async () => {
  await sleep(2000); const tok = JSON.parse(fs.readFileSync(path.join(T, 'd', 'relay.json'))).token;
  let events = ''; req('GET', '/events', tok, null, (b) => { events = b; });
  const cwd = path.join(T, 'proj', 'app');
  // a normal chat of the host
  const first = await req('POST', '/sessions/new/send', tok, { text: 'start the landing page', cwd, mode: 'acceptEdits' });
  const sid = (/"id":"(fake-\d+)"/.exec(first.b) || [])[1]; ok('1 the host has a normal chat', !!sid, sid);
  await sleep(300);
  const sh = await req('POST', `/sessions/${sid}/share`, tok, {}); ok('2 the host shares it', sh.code === 200 && sh.j.shared === true && sh.j.members.length === 0, sh.b.slice(0, 100));
  const again = await req('POST', `/sessions/${sid}/share`, tok, {}); ok('3 sharing again keeps the same share', again.j.id === sh.j.id);
  const lst = await req('GET', '/sessions', tok); ok('4 the list tells which chats are shared', lst.j.some((x) => x.id === sid && x.shared === 0));
  const inv = await req('POST', `/shares/${sh.j.id}/invite`, tok, {}); const join = await req('POST', '/shares/join', null, { invite: inv.j.invite, name: 'Sam' });
  ok('5 a friend joins with the invite', join.code === 200 && /^share\./.test(join.j.key) && join.j.session === sid, join.b.slice(0, 80)); const g = join.j.key, mid = join.j.member;
  ok('6 an invite works only once', (await req('POST', '/shares/join', null, { invite: inv.j.invite, name: 'Eve' })).code === 404);
  // what the guest may see and do
  const gl = await req('GET', '/sessions', g); ok('7 the guest sees only the shared chat, and not the folder', gl.code === 200 && gl.j.length === 1 && gl.j[0].id === sid && !gl.j[0].cwd.includes(T));
  const gm = await req('GET', `/sessions/${sid}/messages`, g); ok('8 the guest reads the chat like a normal one', gm.code === 200 && gm.j.some((m) => m.role === 'user' && /landing page/.test(m.text)) && gm.j.some((m) => m.role === 'assistant'));
  for (const [m, p] of [['GET', '/status'], ['GET', '/projects'], ['GET', '/admin/options'], ['GET', '/usage'], ['POST', '/admin/options'], ['GET', '/fs'], ['GET', '/shares/' + sh.j.id], ['POST', `/shares/${sh.j.id}/invite`], ['GET', '/rooms-sync'], ['GET', '/nothing'], ['GET', '/sessions/other/messages']]) {
    const r = await req(m, p, g, m === 'POST' ? {} : null); ok(`9 a guest key cannot ${m} ${p}`, r.code === 403 || r.code === 401, String(r.code));
  }
  const pr = await req('GET', `/sessions/${sid}/prefs`, g); ok('10 the guest sees their own status', pr.j.shared && pr.j.shared.myName === 'Sam' && pr.j.shared.grant.auto === false);
  // a prompt from the guest is a request, nothing runs
  const msgsBefore = (await req('GET', `/sessions/${sid}/messages`, tok)).j.length;
  const snd = await req('POST', `/sessions/${sid}/send`, g, { text: 'please add a footer' }); ok('11 the guest sends a prompt: it waits for approval', /"t":"queued"/.test(snd.b) && /"approval":true/.test(snd.b));
  await sleep(500);
  const pend = (await req('GET', '/pending', tok)).j.filter((e) => e.kind === 'request'); ok('12 the host sees the request in the waiting list', pend.length === 1 && pend[0].session === sid && /Sam: please add a footer/.test(pend[0].text), JSON.stringify(pend[0]).slice(0, 120));
  const live = evs(events).filter((e) => e.kind === 'request'); ok('13 and gets a live notification event', live.length === 1 && live[0].req === pend[0].req && live[0].from === 'Sam');
  ok('14 nothing ran yet', (await req('GET', `/sessions/${sid}/messages`, tok)).j.length === msgsBefore);
  const mine = await req('GET', `/sessions/${sid}/prefs`, g); ok('15 the guest sees their request waiting', mine.j.shared.requests[0].status === 'pending');
  // the host edits and approves
  const dec = await req('POST', '/decision', tok, { req: pend[0].req, behavior: 'allow', text: 'please add a small footer', mode: 'plan' }); ok('16 the host approves with an edited prompt', dec.code === 200);
  await sleep(1200);
  const after = (await req('GET', `/sessions/${sid}/messages`, g)).j;
  ok('17 it ran in the same chat, as that person, in plan mode', after.some((m) => m.role === 'user' && m.text === 'Sam: please add a small footer') && after.some((m) => m.role === 'assistant' && /MODE=plan/.test(m.text)), JSON.stringify(after.slice(-2)).slice(0, 200));
  ok('18 the resolved event went out', evs(events).some((e) => e.kind === 'resolved' && e.req === pend[0].req));
  const mine2 = await req('GET', `/sessions/${sid}/prefs`, g); ok('19 the guest sees it was changed and ran', ['running', 'done'].includes(mine2.j.shared.requests[0].status) && mine2.j.shared.requests[0].edited === true, mine2.j.shared.requests[0].status);
  // deny
  await sleep(1500); await req('POST', `/sessions/${sid}/send`, g, { text: 'delete everything' }); await sleep(300);
  const p2 = (await req('GET', '/pending', tok)).j.filter((e) => e.kind === 'request'); await req('POST', '/decision', tok, { req: p2[0].req, behavior: 'deny' }); await sleep(300);
  ok('20 a denied prompt never runs', !(await req('GET', `/sessions/${sid}/messages`, tok)).j.some((m) => /delete everything/.test(m.text)) && (await req('GET', `/sessions/${sid}/prefs`, g)).j.shared.requests[0].status === 'denied');
  ok('21 a request cannot be decided twice', (await req('POST', '/decision', tok, { req: p2[0].req, behavior: 'allow' })).code === 409);
  // full permission: runs at once
  await sleep(1600); const gr = await req('POST', `/shares/${sh.j.id}/members/${mid}`, tok, { grant: { auto: true, mode: 'acceptEdits' } }); ok('22 the host gives full permission', gr.code === 200 && gr.j.grant.auto === true);
  const s3 = await req('POST', `/sessions/${sid}/send`, g, { text: 'third, no asking' }); ok('23 with full permission it is not a request', /"auto":true/.test(s3.b));
  await sleep(1200);
  ok('24 it ran at once, in the granted mode', (await req('GET', `/sessions/${sid}/messages`, tok)).j.some((m) => m.role === 'assistant' && /MODE=acceptEdits \| .*Sam: third, no asking/.test(m.text)));
  const fa = await req('POST', `/shares/${sh.j.id}/members/${mid}`, tok, { grant: { auto: true, mode: 'bypassPermissions' } }); ok('25 Full auto for a friend needs the server to allow it', fa.j.grant.mode === 'acceptEdits');
  // busy chat: the approved prompt waits its turn; the guest can stop only their own run
  await sleep(1600); const host2 = req('POST', `/sessions/${sid}/send`, tok, { text: 'host HOLD a while', mode: 'acceptEdits' }); await sleep(500);
  await req('POST', `/sessions/${sid}/send`, g, { text: 'queued behind the host' });
  const stopOther = await req('POST', '/stop', g, { session: sid }); ok('26 a guest cannot stop the host\'s own run', stopOther.code === 403);
  await host2; await sleep(1500);
  ok('27 the prompt that had to wait ran after the host\'s', (await req('GET', `/sessions/${sid}/messages`, tok)).j.some((m) => m.role === 'assistant' && /queued behind the host/.test(m.text)));
  // the other direction: the host asks a friend to use THE FRIEND'S Claude; the friend's own Claude runs it and the result shows in the chat
  await req('POST', `/shares/${sh.j.id}/members/${mid}`, tok, { grant: { auto: false, mode: 'plan' } });
  let gevents = ''; req('GET', '/events', g, null, (b) => { gevents = b; }); await sleep(400);
  await sleep(1600); const ask = await req('POST', `/shares/${sh.j.id}/ask`, tok, { to: mid, text: 'host wants the friend\'s usage' }); ok('34 the host asks the friend\'s Claude', ask.code === 200 && ask.j.status === 'pending');
  await sleep(300);
  const gev = (gevents.match(/data: (.*)/g) || []).map((l) => JSON.parse(l.slice(6))).filter((e) => e.kind === 'request'); ok('35 the friend gets a notification event', gev.length === 1 && gev[0].forMember === mid && /host wants the friend/.test(gev[0].text), JSON.stringify(gev[0] || {}).slice(0, 100));
  ok('36 the host\'s own waiting list does not show it', !(await req('GET', '/pending', tok)).j.some((e) => e.req === ask.j.id));
  const gi = (await req('GET', `/sessions/${sid}/prefs`, g)).j.shared; ok('37 the friend sees it as incoming', gi.incoming.length === 1 && gi.incoming[0].fromName === 'Warre' && gi.people.some((p) => p.id === 'host'));
  const inv3 = await req('POST', `/shares/${sh.j.id}/invite`, tok, {}); const j3 = await req('POST', '/shares/join', null, { invite: inv3.j.invite, name: 'Kim' }); const wrong = await req('POST', `/sessions/${sid}/requests/${ask.j.id}`, j3.j.key, { decision: 'approve' }); ok('38 another friend cannot decide for them', wrong.code === 404);
  const fd2 = await req('POST', `/sessions/${sid}/requests/${ask.j.id}`, g, { decision: 'approve', text: 'host wants the friend\'s usage, edited', mode: 'plan' }); ok('39 the friend approves and edits', fd2.code === 200 && /edited/.test(fd2.j.text) && fd2.j.from === 'Warre');
  const pu = await req('POST', `/sessions/${sid}/post`, g, { role: 'user', text: 'host wants the friend\'s usage, edited', req: ask.j.id }); const pa = await req('POST', `/sessions/${sid}/post`, g, { role: 'assistant', text: 'answer from the friend\'s own account', req: ask.j.id });
  ok('40 the friend\'s app posts the prompt and the answer', pu.code === 200 && pa.code === 200);
  await req('POST', `/sessions/${sid}/requests/${ask.j.id}`, g, { decision: 'done' });
  const mh = (await req('GET', `/sessions/${sid}/messages`, tok)).j; const mg = (await req('GET', `/sessions/${sid}/messages`, g)).j;
  ok('41 everyone sees them in the chat, with who they are', [mh, mg].every((l) => l.some((m) => m.role === 'user' && m.name === 'Warre' && /edited/.test(m.text)) && l.some((m) => m.role === 'assistant' && m.name === "Sam's Claude for Warre")));
  const own = await req('POST', `/sessions/${sid}/post`, g, { role: 'assistant', text: 'my own question went to my own Claude' }); ok('42 a friend can post what their own Claude said', own.code === 200 && (await req('GET', `/sessions/${sid}/messages`, tok)).j.some((m) => m.name === "Sam's Claude"));
  const fake = await req('POST', `/sessions/${sid}/post`, g, { role: 'user', text: 'x', req: 'nope' }); ok('43 posting as somebody else is refused', fake.code === 403);
  await req('POST', `/sessions/${sid}/working`, g, { on: true }); ok('44 who is working shows', (await req('GET', `/sessions/${sid}/messages`, tok)).j.some((m) => m.role === 'meta' && /Sam's Claude is working/.test(m.text))); await req('POST', `/sessions/${sid}/working`, g, { on: false });
  await sleep(1600); await req('POST', `/sessions/${sid}/send`, tok, { text: 'host asks directly again', mode: 'acceptEdits' }); await sleep(1500);
  ok('45 the host\'s own Claude hears what the others did with theirs (the fake Claude repeats what it was told)', (await req('GET', `/sessions/${sid}/messages`, tok)).j.some((m) => m.role === 'assistant' && /others used their own Claude/.test(m.text) && /answer from the friend's own account/.test(m.text)));
  const mine3 = await req('POST', `/sessions/${sid}/ask`, g, { to: 'host', text: 'friend asks host via ask' }); ok('46 a friend can also ask the host through "ask"', mine3.code === 200);
  // everyone sees the same chat: host, two friends, the same list in the same order
  const inv4 = await req('POST', `/shares/${sh.j.id}/invite`, tok, {}); const j4 = await req('POST', '/shares/join', null, { invite: inv4.j.invite, name: 'Lee' });
  const strip = (l) => JSON.stringify(l.filter((m) => m.role !== 'meta' || !/working/.test(m.text)).map((m) => [m.role, m.text, m.name || '', m.ts || 0]));
  const A = strip((await req('GET', `/sessions/${sid}/messages`, tok)).j), B = strip((await req('GET', `/sessions/${sid}/messages`, g)).j), C = strip((await req('GET', `/sessions/${j4.j.key ? sid : sid}/messages`, j4.j.key)).j);
  ok('47 host and two friends get exactly the same messages', A === B && B === C, `${A.length} ${B.length} ${C.length}`);
  ok('48 the note for the host\'s Claude is not shown as a message of the host', !JSON.parse(A).some((m) => m[0] === 'user' && /others used their own Claude/.test(m[1])));
  const order = JSON.parse(A).map((m) => m[3]); ok('49 messages are in time order', order.every((t, i) => i === 0 || t >= order[i - 1] || order[i - 1] === 0 || t === 0));
  const slist = (x) => JSON.stringify((x.j || []).map((s) => [s.id, s.title]));
  ok('50 the list entry of the chat is the same title for everyone', JSON.parse(slist(await req('GET', '/sessions', g)))[0][1] === JSON.parse(slist(await req('GET', '/sessions', tok))).find((x) => x[0] === sid)[1]);
  await req('DELETE', `/shares/${sh.j.id}/members/${j4.j.member}`, tok);
  // restart: waiting requests survive
  await req('POST', `/shares/${sh.j.id}/members/${mid}`, tok, { grant: { auto: false, mode: 'plan' } }); await sleep(1600);
  await req('POST', `/sessions/${sid}/send`, g, { text: 'survives a restart' }); await sleep(300);
  relay.kill(); await sleep(600); relay = startRelay(); await sleep(2000);
  const p3 = (await req('GET', '/pending', tok)).j.filter((e) => e.kind === 'request'); ok('28 a waiting request survives a restart', p3.some((e) => /survives a restart/.test(e.text)));
  const gv = await req('GET', `/sessions/${sid}/messages`, g); ok('29 and the guest key still works', gv.code === 200);
  // removing a friend
  const rm = await req('DELETE', `/shares/${sh.j.id}/members/${mid}`, tok); ok('30 a removed friend is locked out', rm.code === 200 && (await req('GET', `/sessions/${sid}/messages`, g)).code === 401);
  ok('31 and their waiting prompts are gone', (await req('GET', '/pending', tok)).j.filter((e) => e.kind === 'request').length === 0);
  const un = await req('DELETE', `/shares/${sh.j.id}`, tok); ok('32 the host can stop sharing', un.code === 200 && (await req('GET', `/sessions/${sid}/share`, tok)).j.shared === false);
  // the list of shared chats kept on the server for all devices
  const s1 = await req('POST', '/rooms-sync', tok, { items: { 'g:a:b': { v: { name: 'x' }, t: 100 } } }); ok('33 the synced list works', s1.code === 200 && s1.j.items['g:a:b'].v.name === 'x');
  relay.kill(); await sleep(300); fs.rmSync(T, { recursive: true, force: true });
  console.log(`\n${pass} passed, ${fail} failed`); process.exit(fail ? 1 : 0);
})();
