// One-tap tools for Claude: only the ready-made ones can be added, with the right command. Run: node desktop/test/mcp-catalog.test.js
const { spawn } = require('child_process');
const fs = require('fs'), os = require('os'), path = require('path'), http = require('http');
if (process.platform === 'win32') { console.log('skipped on Windows'); process.exit(0); }
fs.mkdirSync(path.join(os.homedir(), '.cache'), { recursive: true }); const T = fs.mkdtempSync(path.join(os.homedir(), '.cache', 'hearth-mcpcat-'));
fs.mkdirSync(path.join(T, 'proj', 'app'), { recursive: true });
const FAKE = path.join(T, 'bin'); fs.mkdirSync(FAKE), LOG = path.join(T, 'args.jsonl');
fs.writeFileSync(path.join(FAKE, 'claude'), `#!/usr/bin/env node\nrequire('fs').appendFileSync(${JSON.stringify(LOG)}, JSON.stringify(process.argv.slice(2)) + '\\n');\n`, { mode: 0o755 });
var LOG;
const relay = spawn('node', [path.join(__dirname, '..', 'relay', 'standalone.js'), '--dir', path.join(T, 'd'), '--port', '47790', '--host', '127.0.0.1', '--projects', path.join(T, 'proj')], { env: { ...process.env, PATH: FAKE + path.delimiter + process.env.PATH, HOME: path.join(T, 'home'), HEARTH_TEST: '1', HEARTH_MEM_CHILD: '1' }, stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let pass = 0, fail = 0; const ok = (n, c, x = '') => { c ? pass++ : fail++; console.log((c ? 'PASS ' : 'FAIL ') + n + (x ? '  ' + x : '')); };
const req = (m, p, tok, body) => new Promise((res) => { const d = body ? Buffer.from(JSON.stringify(body)) : null; const r = http.request({ host: '127.0.0.1', port: 47790, path: p, method: m, headers: { Authorization: 'Bearer ' + tok, ...(d ? { 'Content-Type': 'application/json', 'Content-Length': d.length } : {}) } }, (rs) => { let b = ''; rs.on('data', (x) => { b += x; }); rs.on('end', () => { let j = {}; try { j = JSON.parse(b); } catch { /* */ } res({ code: rs.statusCode, j }); }); }); r.on('error', () => res({ code: 0, j: {} })); if (d) r.write(d); r.end(); });
const calls = () => { try { return fs.readFileSync(LOG, 'utf8').trim().split('\n').map((l) => JSON.parse(l)); } catch { return []; } };
(async () => {
  const tok = await require('./_wait')(path.join(T, 'd', 'relay.json'));
  const cat = (await req('GET', '/mcp/catalog', tok)).j.tools || [];
  ok('1 the catalog lists the ready tools, and which need a secret', cat.length >= 4 && cat.find((x) => x.id === 'github').needs && !cat.find((x) => x.id === 'memory').needs);
  const m = await req('POST', '/mcp/install', tok, { id: 'memory' });
  ok('2 a tool is added with claude mcp add, for the user, with a fixed command', m.code === 200 && m.j.ok && JSON.stringify(calls()[0]) === JSON.stringify(['mcp', 'add', '--scope', 'user', 'memory', '--', 'npx', '-y', '@modelcontextprotocol/server-memory']), JSON.stringify(calls()[0]));
  ok('3 a tool that is not in the catalog cannot be added', (await req('POST', '/mcp/install', tok, { id: 'rm -rf /' })).code === 404 && calls().length === 1);
  ok('4 a tool that needs a secret says so when it is missing, or looks wrong', (await req('POST', '/mcp/install', tok, { id: 'github' })).code === 400 && (await req('POST', '/mcp/install', tok, { id: 'github', secret: 'a b; rm' })).code === 400);
  const g = await req('POST', '/mcp/install', tok, { id: 'github', secret: 'ghp_abcdefghijklmnop1234' });
  ok('5 the secret is handed over as the tool's own environment setting', g.code === 200 && calls()[1].includes('-e') && calls()[1].includes('GITHUB_PERSONAL_ACCESS_TOKEN=ghp_abcdefghijklmnop1234'));
  const f = await req('POST', '/mcp/install', tok, { id: 'filesystem' });
  ok('6 the files tool is limited to the projects folder', f.code === 200 && calls()[2].slice(-1)[0] === path.join(T, 'proj'), JSON.stringify(calls()[2]));
  ok('7 a guest key cannot add tools', (await req('POST', '/mcp/install', 'share.abcd.abcdefghijklmnop', { id: 'memory' })).code === 401);
  relay.kill(); await sleep(300); fs.rmSync(T, { recursive: true, force: true });
  console.log(`\n${pass} passed, ${fail} failed`); process.exit(fail ? 1 : 0);
})();
