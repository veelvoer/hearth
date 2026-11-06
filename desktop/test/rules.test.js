// "Always ask / never allow" rules and the log of what was decided. Run: node desktop/test/rules.test.js
const { spawn } = require('child_process');
const fs = require('fs'), os = require('os'), path = require('path'), http = require('http');
if (process.platform === 'win32') { console.log('skipped on Windows'); process.exit(0); }
fs.mkdirSync(path.join(os.homedir(), '.cache'), { recursive: true }); const T = fs.mkdtempSync(path.join(os.homedir(), '.cache', 'hearth-rules-'));
const proj = path.join(T, 'proj', 'app'); fs.mkdirSync(proj, { recursive: true });
const FAKE = path.join(T, 'bin'); fs.mkdirSync(FAKE);
fs.writeFileSync(path.join(FAKE, 'claude'), `#!/usr/bin/env node
const out = (o) => process.stdout.write(JSON.stringify(o) + '\\n'); out({ type: 'system', subtype: 'init', session_id: 'sid-rules', model: 'x' });
process.stdin.resume(); process.stdin.on('data', () => {}); setTimeout(() => process.exit(0), 6000);
`, { mode: 0o755 });
const relay = spawn('node', [path.join(__dirname, '..', 'relay', 'standalone.js'), '--dir', path.join(T, 'd'), '--port', '47793', '--host', '127.0.0.1', '--projects', path.join(T, 'proj')], { env: { ...process.env, PATH: FAKE + path.delimiter + process.env.PATH, HOME: path.join(T, 'home'), HEARTH_TEST: '1', HEARTH_MEM_CHILD: '1' }, stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + x : '')); };
const req = (m, p, tok, body, noWait) => new Promise((res) => { const d = body ? Buffer.from(JSON.stringify(body)) : null; const r = http.request({ host: '127.0.0.1', port: 47793, path: p, method: m, headers: { Authorization: 'Bearer ' + tok, ...(d ? { 'Content-Type': 'application/json', 'Content-Length': d.length } : {}) } }, (rs) => { let b = ''; rs.on('data', (x) => { b += x; }); rs.on('end', () => { let j = {}; try { j = JSON.parse(b); } catch { /* ndjson */ } res({ code: rs.statusCode, j }); }); }); r.on('error', () => res({ code: 0, j: {} })); if (d) r.write(d); r.end(); if (noWait) res({ code: 0, j: {} }); });
const hook = (tok, tool, input) => req('POST', '/hook', tok, { hook_event_name: 'PermissionRequest', session_id: 'sid-rules', tool_name: tool, tool_input: input, cwd: proj });
(async () => {
  const tok = await require('./_wait')(path.join(T, 'd', 'relay.json'));
  req('POST', '/sessions/new/send', tok, { text: 'go', mode: 'acceptEdits', cwd: proj }, true); await sleep(1200);
  const add = await req('POST', '/admin/options', tok, { addDeny: 'rm -rf' });
  ok('1 a rule that always says no is saved and listed', add.code === 200 && (await req('GET', '/admin/options', tok)).j.denyRules.some((r) => r.prefix === 'rm -rf'));
  const d1 = await hook(tok, 'Bash', { command: 'cd x && rm -rf build' });
  ok('2 a command with those words is refused at once, without asking', d1.j.hookSpecificOutput && d1.j.hookSpecificOutput.decision.behavior === 'deny' && /rule you made/.test(d1.j.hookSpecificOutput.decision.message));
  await req('POST', '/admin/options', tok, { addPrefix: 'npm test' });
  const d2 = await hook(tok, 'Bash', { command: 'npm test' });
  ok('3 an allow rule answers yes at once', d2.j.hookSpecificOutput && d2.j.hookSpecificOutput.decision.behavior === 'allow');
  const log = (await req('GET', '/admin/decisions', tok)).j.decisions || [];
  ok('4 both decisions are in the log, newest first, with what the rule was', log.length >= 2 && log[0].result === 'allowed by your rule' && log[1].result === 'denied by your rule' && log[1].rule === 'rm -rf', JSON.stringify(log.map((x) => x.result)));
  const id = (await req('GET', '/admin/options', tok)).j.denyRules[0].id; await req('POST', '/admin/options', tok, { removeDeny: id });
  ok('5 a rule can be removed again', (await req('GET', '/admin/options', tok)).j.denyRules.length === 0);
  ok('6 a guest key cannot read the log or add rules', (await req('GET', '/admin/decisions', 'share.abcd.abcdefghijklmnop')).code === 401 && (await req('POST', '/admin/options', 'share.abcd.abcdefghijklmnop', { addDeny: 'ls' })).code === 401);
  relay.kill(); await sleep(300); fs.rmSync(T, { recursive: true, force: true });
  console.log(`\n${pass} passed, ${fail} failed`); process.exit(fail ? 1 : 0);
})();
