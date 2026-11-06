#!/usr/bin/env node
'use strict';
/* Finds names that are used but never defined in the desktop screens (a typo or a forgotten function shows up as a ReferenceError only when somebody clicks).
   The screens are plain scripts that share one global scope, so they are joined in the order of index.html and checked as one file. Needs network once for eslint. Run: node scripts/lint-renderer.js */
const fs = require('fs'), os = require('os'), path = require('path'), { spawnSync } = require('child_process');
const dir = path.join(__dirname, '..', 'desktop', 'renderer'), html = fs.readFileSync(path.join(dir, 'index.html'), 'utf8');
const srcs = [...html.matchAll(/<script src="([^"]+)"/g)].map((m) => m[1]);
const work = fs.mkdtempSync(path.join(os.tmpdir(), 'hearth-lint-'));
fs.writeFileSync(path.join(work, 'all.js'), srcs.map((s) => `// ---- ${s}\n` + fs.readFileSync(path.join(dir, s), 'utf8')).join('\n'));
const names = 'document window navigator localStorage console setTimeout clearTimeout setInterval clearInterval requestAnimationFrame cancelAnimationFrame matchMedia getComputedStyle innerWidth innerHeight devicePixelRatio performance Audio AudioContext AudioWorkletNode webkitAudioContext FileReader Blob URL fetch Promise HTMLElement Node Event KeyboardEvent MouseEvent Image ResizeObserver IntersectionObserver MutationObserver AbortController AbortSignal crypto prompt confirm alert location history Notification speechSynthesis SpeechSynthesisUtterance MediaRecorder getSelection DOMParser Intl TextDecoder TextEncoder btoa atob Uint8Array Float32Array Map Set WeakMap requestIdleCallback structuredClone queueMicrotask CSS DOMException Response Headers FormData screen scrollTo open close addEventListener removeEventListener dispatchEvent CustomEvent DocumentFragment SVGElement Element DataTransfer ClipboardEvent DragEvent File Path2D cm CMW md WeekPlan module'.split(' ');
fs.writeFileSync(path.join(work, 'eslint.config.js'), `module.exports = [{ files: ['all.js'], languageOptions: { ecmaVersion: 2023, sourceType: 'script', globals: ${JSON.stringify(Object.fromEntries(names.map((n) => [n, 'readonly'])))} }, rules: { 'no-undef': 'error' } }];`);
const r = spawnSync('npx', ['--yes', 'eslint@9', '-c', 'eslint.config.js', 'all.js'], { cwd: work, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], env: { ...process.env, npm_config_cache: path.join(os.tmpdir(), 'hearth-npm-cache') } });
process.stdout.write(r.stdout || ''); process.stderr.write((r.stderr || '').split('\n').filter((l) => !/npm notice/.test(l)).join('\n'));
// the server side: each file on its own, with the names Node gives every program
const node = 'require module exports process __dirname __filename Buffer console setTimeout clearTimeout setInterval clearInterval setImmediate clearImmediate fetch AbortSignal AbortController URL URLSearchParams TextDecoder TextEncoder structuredClone queueMicrotask Promise Intl WebAssembly performance crypto Blob atob btoa global globalThis'.split(' ');
fs.writeFileSync(path.join(work, 'eslint.node.js'), `module.exports = [{ languageOptions: { ecmaVersion: 2023, sourceType: 'commonjs', globals: ${JSON.stringify(Object.fromEntries(node.map((n) => [n, 'readonly'])))} }, rules: { 'no-undef': 'error' } }];`);
const root = path.join(__dirname, '..'), files = [];
for (const d of ['desktop', 'desktop/relay', 'server', 'support', 'scripts']) for (const f of fs.readdirSync(path.join(root, d))) if (f.endsWith('.js') && !/^(sw|preload-stub)/.test(f) && f !== 'mcpkit-files.js' && f !== 'weekplan.js') files.push(path.join(root, d, f));
const r2 = spawnSync('npx', ['--yes', 'eslint@9', '-c', path.join(work, 'eslint.node.js'), ...files], { cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], env: { ...process.env, npm_config_cache: path.join(os.tmpdir(), 'hearth-npm-cache') } });
process.stdout.write(r2.stdout || '');
fs.rmSync(work, { recursive: true, force: true });
process.exit(r.status === 0 && r2.status === 0 ? 0 : 1);
