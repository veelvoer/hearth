// The MCP server kit: packed copy is current, the kit passes its own check, and "build one" starts a chat in a fresh project folder. Run: node desktop/test/mcpkit.test.js
const { spawn, spawnSync } = require('child_process');
const fs = require('fs'), os = require('os'), path = require('path'), http = require('http');
if (process.platform === 'win32') { console.log('skipped on Windows'); process.exit(0); }
fs.mkdirSync(path.join(os.homedir(), '.cache'), { recursive: true }); const T = fs.mkdtempSync(path.join(os.homedir(), '.cache', 'hearth-mcpkit-'));
for (const d of ['bin', 'home', 'proj']) fs.mkdirSync(path.join(T, d), { recursive: true });
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + x : '')); };
const kitDir = path.join(__dirname, '..', 'relay', 'mcpkit'), packed = require('../relay/mcpkit-files.js');
ok('1 the packed kit equals the files (run node scripts/build-mcpkit.js when this fails)', fs.readdirSync(kitDir).every((f) => packed[f] === fs.readFileSync(path.join(kitDir, f), 'utf8')) && Object.keys(packed).length === fs.readdirSync(kitDir).length);
const k = path.join(T, 'kit'); fs.mkdirSync(k); for (const [f, t] of Object.entries(packed)) fs.writeFileSync(path.join(k, f), t);
const c = spawnSync('node', [path.join(k, 'check.js')], { encoding: 'utf8' });
ok('2 the starter kit passes its own check', c.status === 0 && /All good/.test(c.stdout), c.stdout.slice(-120));
fs.writeFileSync(path.join(k, 'tools.js'), "module.exports=[{name:'Bad Name',description:'x',inputSchema:{type:'object',properties:{a:{type:'string'}},required:['a']},run:async()=>'x',sample:{a:'1'}}];");
const c2 = spawnSync('node', [path.join(k, 'check.js')], { encoding: 'utf8' });
ok('3 the check catches a bad name, a short description and an undescribed argument', c2.status === 1 && /name must be lowercase/.test(c2.stdout) && /description is missing or too short/.test(c2.stdout) && /needs a description/.test(c2.stdout));
fs.writeFileSync(path.join(k, 'tools.js'), "module.exports=[{name:'boom',description:'Always throws an error on purpose.',inputSchema:{type:'object',properties:{}},run:async()=>{throw new Error('nope')},sample:{}}];");
ok('4 a tool that throws is reported, and a sample that should fail may fail', spawnSync('node', [path.join(k, 'check.js')], { encoding: 'utf8' }).status === 1);
fs.writeFileSync(path.join(T, 'bin', 'claude'), `#!/usr/bin/env node
let t = ''; process.stdin.on('data', (d) => { t += d; }); process.stdin.on('end', () => { require('fs').appendFileSync(process.env.HOME + '/prompts.txt', process.cwd() + '\\n' + t + '\\n----\\n'); console.log(JSON.stringify({ type: 'system', subtype: 'init', session_id: 'mcp-' + process.pid })); console.log(JSON.stringify({ type: 'result', is_error: false, result: 'ok' })); });
`, { mode: 0o755 });
const relay = spawn('node', [path.join(__dirname, '..', 'relay', 'standalone.js'), '--dir', path.join(T, 'd'), '--port', '47792', '--host', '127.0.0.1', '--projects', path.join(T, 'proj')], { env: { ...process.env, HOME: path.join(T, 'home'), PATH: path.join(T, 'bin') + path.delimiter + process.env.PATH }, stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const req = (method, url, tok, body) => new Promise((res) => { const d = body ? Buffer.from(JSON.stringify(body)) : null; const r = http.request({ host: '127.0.0.1', port: 47792, path: url, method, headers: { Authorization: 'Bearer ' + tok, ...(d ? { 'Content-Type': 'application/json', 'Content-Length': d.length } : {}) } }, (rs) => { let b = ''; rs.on('data', (x) => { b += x; }); rs.on('end', () => { let j = {}; try { j = JSON.parse(b); } catch { /* */ } res({ code: rs.statusCode, j }); }); }); r.on('error', () => res({ code: 0, j: {} })); if (d) r.write(d); r.end(); });
(async () => {
  await sleep(2000); const tok = JSON.parse(fs.readFileSync(path.join(T, 'd', 'relay.json'))).token;
  ok('5 it needs to be told what to connect to', (await req('POST', '/mcp/build', tok, { target: '' })).code === 400);
  ok('6 a guest key cannot start one', (await req('POST', '/mcp/build', 'share.abcd.abcdefghijklmnop', { target: 'Inkscape' })).code === 401);
  const r = await req('POST', '/mcp/build', tok, { target: 'Inkscape (vector graphics)', notes: 'export to PNG too' }); await sleep(1500);
  const dir = path.join(T, 'proj', 'mcp-inkscape-vector-graphics');
  ok('7 a project folder with the kit is made', r.code === 200 && ['server.js', 'tools.js', 'check.js', 'register.js'].every((f) => fs.existsSync(path.join(dir, f))), JSON.stringify(r.j));
  const pr = fs.existsSync(path.join(T, 'home', 'prompts.txt')) ? fs.readFileSync(path.join(T, 'home', 'prompts.txt'), 'utf8') : '';
  ok('8 a chat starts in that folder with clear steps, the target and the notes', pr.includes(dir) && /Inkscape \(vector graphics\)/.test(pr) && /export to PNG too/.test(pr) && /node check\.js/.test(pr) && /node register\.js mcp-inkscape-vector-graphics/.test(pr) && /never write a secret/i.test(pr));
  relay.kill(); await sleep(300); fs.rmSync(T, { recursive: true, force: true });
  console.log(`\n${pass} passed, ${fail} failed`); process.exit(fail ? 1 : 0);
})();
