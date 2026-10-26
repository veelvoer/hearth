#!/usr/bin/env node
'use strict';
/* Tests the MCP server the way Claude uses it: start it, say hello, list the tools, call every tool that has a sample.
   Run:  node check.js      Exit code 0 only when everything is fine. */
const { spawn } = require('child_process');
const path = require('path');
const tools = require(path.join(__dirname, 'tools.js'));
let bad = 0; const fail = (m) => { bad++; console.log('FAIL ' + m); }; const pass = (m) => console.log('PASS ' + m);
const child = spawn(process.execPath, [path.join(__dirname, 'server.js')], { stdio: ['pipe', 'pipe', 'inherit'] });
let buf = '', next = 1; const waiting = new Map();
child.stdout.on('data', (d) => { buf += d; let i; while ((i = buf.indexOf('\n')) >= 0) { const l = buf.slice(0, i); buf = buf.slice(i + 1); let m; try { m = JSON.parse(l); } catch { fail('the server printed something that is not JSON on stdout: ' + l.slice(0, 80)); continue; } if (waiting.has(m.id)) { waiting.get(m.id)(m); waiting.delete(m.id); } } });
const rpc = (method, params) => new Promise((resolve) => { const id = next++; const t = setTimeout(() => { waiting.delete(id); resolve({ error: { message: 'no answer in 130 s' } }); }, 130000); waiting.set(id, (m) => { clearTimeout(t); resolve(m); }); child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id, method, params }) + '\n'); });
(async () => {
  const init = await rpc('initialize', { protocolVersion: '2024-11-05', capabilities: {}, clientInfo: { name: 'check', version: '1' } });
  init.result && init.result.serverInfo ? pass('initialize works') : fail('initialize did not answer');
  const l = await rpc('tools/list', {}); const got = (l.result && l.result.tools) || [];
  got.length === tools.length && got.length > 0 ? pass(got.length + ' tool(s) listed') : fail('tools/list shows ' + got.length + ' tools, tools.js has ' + tools.length);
  const names = new Set();
  for (const t of got) {
    if (!/^[a-z][a-z0-9_]{1,63}$/.test(t.name)) fail(`${t.name}: the name must be lowercase letters, digits and underscores`);
    if (names.has(t.name)) fail(`${t.name}: the name is used twice`); names.add(t.name);
    if (!t.description || t.description.length < 20) fail(`${t.name}: the description is missing or too short (say what it does and when to use it)`);
    if (!t.inputSchema || t.inputSchema.type !== 'object') fail(`${t.name}: inputSchema must be an object schema`);
    else for (const [k, v] of Object.entries(t.inputSchema.properties || {})) if (!v.description) fail(`${t.name}.${k}: every argument needs a description`);
    for (const r of (t.inputSchema && t.inputSchema.required) || []) if (!((t.inputSchema.properties || {})[r])) fail(`${t.name}: "${r}" is required but not described in properties`);
  }
  for (const t of tools) {
    const noArgs = await rpc('tools/call', { name: t.name, arguments: {} });
    if (((t.inputSchema && t.inputSchema.required) || []).length) (noArgs.result && noArgs.result.isError) ? pass(`${t.name}: missing arguments give a clear error`) : fail(`${t.name}: a call without the required arguments should be an error`);
    if (!t.sample) { console.log(`SKIP ${t.name}: no sample (fine for tools that change things, but test them by hand)`); continue; }
    const r = await rpc('tools/call', { name: t.name, arguments: t.sample });
    const isErr = !!(r.result && r.result.isError), text = r.result && r.result.content && r.result.content[0] && r.result.content[0].text;
    if (r.error) fail(`${t.name}: protocol error ${r.error.message}`);
    else if (t.sampleError ? !isErr : isErr) fail(`${t.name}: sample ${t.sampleError ? 'should have given an error' : 'gave an error'}: ${String(text).slice(0, 160)}`);
    else pass(`${t.name}: sample works → ${String(text).replace(/\s+/g, ' ').slice(0, 80)}`);
  }
  const unknown = await rpc('tools/call', { name: 'does_not_exist', arguments: {} });
  unknown.result && unknown.result.isError ? pass('an unknown tool is an error, not a crash') : fail('an unknown tool should be an error');
  child.stdin.end(); setTimeout(() => process.exit(bad ? 1 : 0), 200);
  console.log(bad ? `\n${bad} problem(s)` : '\nAll good.');
})();
