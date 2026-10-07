'use strict';
/* Everyday Claude Code extras for the apps, without a terminal: what changed in a project (git), saving it, setting it aside,
   the project's notes (CLAUDE.md), and the connections (MCP servers). Every path is checked against the projects folder by the caller. */
const fs = require('fs');
const os = require('os');
const path = require('path');
const { execFile } = require('child_process');

const run = (cmd, args, cwd, timeout = 20000) => new Promise((resolve) => {
  execFile(cmd, args, { cwd, timeout, maxBuffer: 4e6, env: { ...process.env, GIT_TERMINAL_PROMPT: '0' } }, (err, out, errOut) => resolve({ ok: !err, out: String(out || ''), err: String(errOut || (err && err.message) || '') }));
});

let plCache = { at: 0, data: null };
async function pluginData(claude, force) {
  if (!force && plCache.data && Date.now() - plCache.at < 5 * 60000) return plCache.data;
  const r = await run(claude, ['plugin', 'list', '--json', '--available'], os.homedir(), 90000);
  let d = { installed: [], available: [] }; try { d = JSON.parse(r.out); } catch { /* keep empty */ }
  plCache = { at: Date.now(), data: d }; return d;
}
const fm = (f) => { try { const t = fs.readFileSync(f, 'utf8'); const m = /^---\n([\s\S]*?)\n---/.exec(t); const d = m && /^description:\s*(.+)$/m.exec(m[1]); return d ? d[1].replace(/^["']|["']$/g, '').slice(0, 160) : ''; } catch { return ''; } };
const scan = (base, prefix, out, src) => {
  try { for (const f of fs.readdirSync(path.join(base, 'commands'))) if (f.endsWith('.md')) out.set(prefix + f.slice(0, -3), { desc: fm(path.join(base, 'commands', f)), src }); } catch { /* none */ }
  try { for (const d of fs.readdirSync(path.join(base, 'skills'))) { const p = path.join(base, 'skills', d, 'SKILL.md'); if (fs.existsSync(p)) out.set(prefix + d, { desc: fm(p), src }); } } catch { /* none */ }
};
/** Everything you can start with a slash: built-ins, your own commands and skills, the project's, and those of enabled plugins. */
async function commands(cwd, claude) {
  const out = new Map();
  scan(path.join(os.homedir(), '.claude'), '', out, 'yours');
  if (cwd) scan(path.join(cwd, '.claude'), '', out, 'project');
  if (claude) { const d = await pluginData(claude); for (const p of d.installed) if (p.enabled && p.installPath) scan(p.installPath, p.id.split('@')[0] + ':', out, 'plugin ' + p.id.split('@')[0]); }
  const list = [['compact', 'Shrink this chat so it stays fast and cheap'], ['init', 'Let Claude write notes about this project (CLAUDE.md)'], ['review', 'Review the latest changes'], ['security-review', 'Check the changes for security problems']].map(([name, desc]) => ({ name, desc, src: 'built in', builtin: true }));
  for (const [name, v] of out) if (!list.some((x) => x.name === name)) list.push({ name, desc: v.desc, src: v.src });
  return list;
}
const STATE = { M: 'changed', A: 'new', D: 'deleted', R: 'renamed', '?': 'new' };
const fail = (code, error) => ({ code, data: { error } });

async function git(cwd, args) { return run('git', args, cwd); }

async function route(parts, u, body, ctx) {
  const q = (k) => String(u.searchParams.get(k) || body[k] || '');
  const cwd = q('cwd');
  const needDir = () => { if (!cwd || !path.isAbsolute(cwd) || !ctx.inRoot(cwd)) return false; try { return fs.statSync(cwd).isDirectory(); } catch { return false; } };
  const [a, b] = parts;

  if (a === 'git') {
    if (!needDir()) return fail(400, 'This project folder is not on this computer.');
    const top = await git(cwd, ['rev-parse', '--show-toplevel']);
    if (!top.ok) return { data: { repo: false } };
    if (b === 'status') {
      const st = await git(cwd, ['status', '--porcelain=v1', '-b']);
      const lines = st.out.split('\n').filter(Boolean);
      const branch = (lines[0] || '').replace(/^## /, '').split('...')[0];
      const files = lines.slice(1).map((l) => ({ state: STATE[l[0] === ' ' ? l[1] : l[0]] || 'changed', path: l.slice(3) }));
      const last = (await git(cwd, ['log', '-1', '--format=%s'])).out.trim();
      return { data: { repo: true, branch, files, last } };
    }
    if (b === 'diff') {
      const f = q('file');
      const d = await git(cwd, f ? ['diff', 'HEAD', '--', f] : ['diff', 'HEAD']);
      let text = d.out;
      if (!text && f) { try { text = fs.readFileSync(path.join(top.out.trim(), f), 'utf8').split('\n').map((l) => '+' + l).join('\n'); } catch { /* binary or gone */ } }
      return { data: { diff: text.slice(0, 60000) } };
    }
    if (b === 'commit') {
      const msg = q('message').trim();
      if (!msg) return fail(400, 'Write a short description of what you changed.');
      await git(cwd, ['add', '-A']);
      const r = await git(cwd, ['-c', 'user.name=' + (process.env.GIT_AUTHOR_NAME || os.userInfo().username), '-c', 'user.email=' + (process.env.GIT_AUTHOR_EMAIL || os.userInfo().username + '@' + os.hostname()), 'commit', '-m', msg]);
      return r.ok ? { data: { ok: true } } : fail(400, (r.out + r.err).trim().split('\n').slice(-2).join(' '));
    }
    if (b === 'aside') {   // undo that can itself be undone: the changes are kept in git's stash
      const r = await git(cwd, ['stash', 'push', '-u', '-m', 'set aside from Hearth ' + new Date().toISOString()]);
      return r.ok ? { data: { ok: true, text: r.out.trim() } } : fail(400, r.err.trim());
    }
    if (b === 'bring-back') { const r = await git(cwd, ['stash', 'pop']); return r.ok ? { data: { ok: true } } : fail(400, (r.err || r.out).trim().split('\n').slice(-2).join(' ')); }
    if (b === 'stashes') { const r = await git(cwd, ['stash', 'list']); return { data: { count: r.out.split('\n').filter(Boolean).length } }; }
  }

  if (a === 'commands') return { data: await commands(needDir() ? cwd : '', ctx.claude) };
  if (a === 'plugins' && ctx.claude) {
    if (!b) {
      const d = await pluginData(ctx.claude, u.searchParams.get('fresh') === '1'), ql = q('q').toLowerCase();
      const have = new Set(d.installed.map((p) => p.id));
      const installed = d.installed.map((p) => ({ id: p.id, name: p.id.split('@')[0], version: p.version, enabled: !!p.enabled }));
      const available = d.available.filter((p) => !have.has(p.pluginId) && (!ql || (p.name + ' ' + (p.description || '')).toLowerCase().includes(ql))).sort((x, y) => (y.installCount || 0) - (x.installCount || 0)).slice(0, 40).map((p) => ({ id: p.pluginId, name: p.name, desc: String(p.description || '').slice(0, 200), market: p.marketplaceName }));
      return { data: { installed, available, total: d.available.length } };
    }
    const id = q('id');
    if (!/^[\w.@-]+$/.test(id) || !['install', 'uninstall', 'enable', 'disable'].includes(b)) return fail(400, 'bad request');
    const r = await run(ctx.claude, ['plugin', b, id, '--scope', 'user'], os.homedir(), 120000);
    plCache.at = 0;
    return r.ok ? { data: { ok: true } } : fail(400, (r.err || r.out).trim().split('\n').slice(-2).join(' '));
  }
  if (a === 'notes') {   // CLAUDE.md: what Claude should always know about this project (or about you)
    const scope = q('scope') === 'global' ? 'global' : 'project';
    if (scope === 'project' && !needDir()) return fail(400, 'This project folder is not on this computer.');
    const file = scope === 'global' ? path.join(os.homedir(), '.claude', 'CLAUDE.md') : path.join(cwd, 'CLAUDE.md');
    if (b === 'save') { fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, String(body.text || '')); return { data: { ok: true } }; }
    let text = ''; try { text = fs.readFileSync(file, 'utf8'); } catch { /* none yet */ }
    return { data: { text, scope } };
  }

  if (a === 'mcp' && ctx.claude) {   // connections (MCP servers) via Claude Code's own settings
    const home = needDir() ? cwd : os.homedir();
    if (!b) {
      const r = await run(ctx.claude, ['mcp', 'list'], home, 45000);
      const items = r.out.split('\n').map((l) => /^([^\s:][^:]*):\s+(.+?)\s+-\s+(.+)$/.exec(l.trim())).filter(Boolean).map((m) => ({ name: m[1], target: m[2], status: /✓|connected/i.test(m[3]) ? 'connected' : /auth/i.test(m[3]) ? 'needs sign-in' : 'not working' }));
      return { data: { items } };
    }
    if (b === 'add') {
      const name = q('name').replace(/[^\w.-]/g, ''), target = q('target').trim();
      if (!name || !target) return fail(400, 'Give it a name and an address or command.');
      const web = /^https?:\/\//i.test(target);
      const args = web ? ['mcp', 'add', '--scope', 'user', '--transport', 'http', name, target] : ['mcp', 'add', '--scope', 'user', name, '--', ...target.split(/\s+/)];
      const r = await run(ctx.claude, args, home, 30000);
      return r.ok ? { data: { ok: true } } : fail(400, (r.err || r.out).trim().split('\n').slice(-2).join(' '));
    }
    if (b === 'remove') { const r = await run(ctx.claude, ['mcp', 'remove', q('name'), '--scope', 'user'], home, 20000); return r.ok ? { data: { ok: true } } : fail(400, (r.err || r.out).trim().split('\n').slice(-2).join(' ')); }
  }
  return fail(404, 'unknown');
}
module.exports = { route, commands };
