#!/usr/bin/env node
'use strict';
/* Adds this MCP server to Claude Code and checks that it connects.
   Run:  node register.js <name> [KEY=value ...]     (KEY=value are secrets for the server, like an API key: they go to Claude's settings, not into the code) */
const { spawnSync } = require('child_process');
const path = require('path');
const [name, ...envs] = process.argv.slice(2);
if (!name || !/^[a-z][a-z0-9_-]{1,40}$/.test(name)) { console.error('Usage: node register.js <name> [KEY=value ...]   (name: lowercase letters, digits, - and _)'); process.exit(1); }
const claude = process.env.CLAUDE_BIN || 'claude';
spawnSync(claude, ['mcp', 'remove', name, '--scope', 'user'], { stdio: 'ignore' });
const args = ['mcp', 'add', '--scope', 'user', ...envs.flatMap((e) => ['-e', e]), name, '--', process.execPath, path.join(__dirname, 'server.js')];
const add = spawnSync(claude, args, { encoding: 'utf8' });
if (add.status !== 0) { console.error('Could not add it: ' + (add.stderr || add.stdout || add.error)); process.exit(1); }
console.log('Added "' + name + '". Checking the connection…');
const ls = spawnSync(claude, ['mcp', 'list'], { encoding: 'utf8', timeout: 120000 });
const line = String(ls.stdout || '').split('\n').find((l) => l.startsWith(name + ':')) || '';
console.log(line || '(not listed)');
if (/Connected|✓/.test(line)) { console.log('It works. Start a NEW chat (or restart Claude) to use it.'); process.exit(0); }
console.error('It was added but does not connect. Run  node check.js  to see why.'); process.exit(2);
