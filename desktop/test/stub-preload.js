'use strict';
// Test-only: fakes the `cm` API with sample data so every screen can be rendered and screenshotted.
const { contextBridge } = require('electron');
const now = Date.now();
const rnd = (a, b) => Math.floor(a + Math.random() * (b - a));
const days = Array.from({ length: 30 }, (_, i) => {
  const d = new Date(now - (29 - i) * 864e5), busy = [0, 6].includes(d.getDay()) ? 0.3 : 1;
  const o = Math.floor(rnd(0, 900e3) * busy), s = Math.floor(rnd(100e3, 2.4e6) * busy), hk = Math.floor(rnd(0, 150e3) * busy);
  return { date: d.toISOString().slice(0, 10), in: rnd(1e3, 9e3), out: rnd(40e3, 400e3) * busy | 0, cr: rnd(5e6, 80e6) * busy | 0, cw: rnd(2e5, 2e6) * busy | 0, msgs: rnd(5, 90) * busy | 0, sessions: rnd(1, 7), fam: { opus: o, sonnet: s, haiku: hk } };
});
const heat = Array.from({ length: 7 }, (_, d) => Array.from({ length: 24 }, (_, h) => (h >= 9 && h <= 22 && d < 5 ? rnd(0, 30) : rnd(0, 4)) * (h === 3 ? 0 : 1)));
const stats = { generated: now, days, models: { opus: 9e6, sonnet: 38e6, haiku: 2e6 }, tools: { Bash: 525, Read: 312, Edit: 201, Grep: 140, Write: 77, Agent: 30, WebFetch: 12 }, heat,
  projects: [{ cwd: '/home/you/photo-app', tokens: 21e6 }, { cwd: '/home/you/code/api-server', tokens: 14e6 }, { cwd: '/home/you/code/website', tokens: 8e6 }, { cwd: '/home/you/dotfiles', tokens: 2e6 }].map((p) => ({ ...p, sessions: 3, last: now })) };
const history = Array.from({ length: 160 }, (_, i) => ({ t: now - (160 - i) * 15 * 60e3, s: Math.min(95, (i * 7) % 100 * 0.9), w: 10 + i * 0.28 }));
const state = { signedIn: true, error: null, now, history,
  settings: { interval: 5, thresholds: [75, 90], notifyReset: true, callMe: true, theme: 'system', palette: process.env.CM_PALETTE || 'claude', background: true },
  profile: { email: 'you@example.com', plan: 'Max 5x' },
  usage: { session: { pct: 62, resetsAt: now + 2 * 36e5 + 14 * 6e4 }, week: { pct: 34, resetsAt: now + 3 * 864e5 }, opus: { pct: 12, resetsAt: now + 3 * 864e5 }, sonnet: { pct: 31, resetsAt: now + 3 * 864e5 }, extra: { used: 12.4, limit: 50, pct: 24.8 }, at: now - 120e3 } };
const sessions = [
  { id: 'a1', title: 'Fix the login handler tests', cwd: '/home/you/code/api-server', mtime: now - 4e3, live: true, busy: true },
  { id: 'a2', title: 'Add a dark mode to the gallery', cwd: '/home/you/photo-app', mtime: now - 60e3, live: true, busy: false },
  { id: 'a3', title: 'Refactor the settings page', cwd: '/home/you/code/website', mtime: now - 3 * 36e5, live: false, busy: false }];
const msgs = [
  { role: 'user', text: 'Can you fix the failing login tests?' },
  { role: 'tool', name: 'Bash', text: 'Bash npm test', input: { command: 'npm test -- login', description: 'Run the login tests' }, result: 'FAIL  login.test.js\n  ✕ rejects an expired session (4 ms)\n\nTests: 2 failed, 46 passed', error: true },
  { role: 'tool', name: 'Read', text: 'Read src/auth/session.js', input: { file_path: '/home/you/code/api-server/src/auth/session.js' }, result: 'export function readSession(req) {...}' },
  { role: 'assistant', text: 'Two tests failed because the **session cookie was renamed** from `sid` to `session_id`. I\'ll update the helper.' },
  { role: 'tool', name: 'Edit', text: 'Edit session.js', input: { file_path: '/home/you/code/api-server/src/auth/session.js', old_string: "const id = req.cookies.sid;", new_string: "const id = req.cookies.session_id;" }, result: 'The file has been updated.' },
  { role: 'tool', name: 'Bash', text: 'Bash npm test', input: { command: 'npm test', description: 'Run all tests' }, result: 'Tests: 48 passed, 48 total' },
  { role: 'assistant', text: "All 48 tests pass now. Here's what changed:\n\n## Fix\n- Renamed the cookie lookup in `readSession`\n- No other callers needed changes\n\n```js\nexport function readSession(req) {\n  const id = req.cookies.session_id;\n  return store.get(id);\n}\n```\n\nWant me to open a PR?" },
];
const noop = () => () => {};
contextBridge.exposeInMainWorld('cm', {
  state: async () => state, refresh: async () => state, authBegin: async () => true, authFinish: async () => state, signOut: async () => state, setSettings: async (p) => ({ ...state.settings, ...p }),
  stats: async () => stats, pairInfo: async () => ({ embedded: true, external: false, code: '482915', port: 47601, addresses: ['192.168.1.20'], name: 'laptop', claude: true, hooks: false, platform: 'win32' }), newPairCode: async () => '123456', installHooks: async () => true, removeHooks: async () => true, accent: async () => '#e5844b', setAutostart: async () => true, machines: async () => [{ id: 'local', name: 'my-laptop (this computer)', host: '127.0.0.1', port: 47601, local: true }, { id: 'vps', name: 'VPS', host: 'cm.example.com', port: 443, secure: true }], addMachine: async () => [], removeMachine: async () => [], discover: async () => [],
  pendingProjects: async () => process.env.HEARTH_SHOTS ? [] : [{ name: 'photo-app', files: 42, bytes: 2.5e6, path: '/home/u/projects/photo-app' }, { name: 'a-project-with-a-really-long-name-that-keeps-going', files: 1, bytes: 900, path: '/x' }], acceptProjects: async () => [],
  hubStatus: async () => ({ server: { name: 'My server', online: true, queued: 0, laptop: 'laptop', listeners: 2, sync: { available: true, state: 'idle' } }, me: 'laptop', local: { sync: { available: true, state: 'idle' } } }),
  phoneCode: async () => ({ code: '123456', address: 'https://relay.example.com' }),
  updateStatus: async () => null, updateCheck: async () => ({}), updateLater: async () => true, updateRun: async () => true,
  relay: async (id, m, p) => p === '/sync/status' ? { role: 'computer', linked: true, files: { tracked: 134, last: { ok: true, at: Date.now() - 60000, errors: [] } }, last: { ok: true, at: Date.now() - 30000, localChats: 15, errors: [] }, pending: [] } : p.startsWith('/tools/git/status') ? { repo: true, branch: 'main', last: 'first version', files: [{ state: 'changed', path: 'src/index.js' }, { state: 'new', path: 'a-very-long-folder-name/another-long-folder/some_really_long_file_name_here.test.ts' }] } : p.startsWith('/tools/notes') ? { text: '# Notes', scope: 'project' } : p.startsWith('/tools/plugins') ? { installed: [{ id: 'x@y', name: 'caveman', enabled: false }], available: [{ id: 'a@b', name: 'github', desc: 'Work with GitHub issues, pull requests and repositories directly from your chats without leaving the app.', market: 'm' }], total: 3549 } : p.startsWith('/tools/mcp') ? { items: [{ name: 'claude.ai Gmail', target: 'https://gmailmcp.googleapis.com/mcp/v1', status: 'connected' }] } : p.startsWith('/commands') || p.startsWith('/tools/commands') ? [{ name: 'compact', desc: 'Shrink this chat', builtin: true }, { name: 'frontend-design', desc: 'Create distinctive, production-grade interfaces', src: 'plugin' }] : p === '/status' ? { moveRoot: '/home/you/projects', away: false } : p.includes('messages') ? msgs : p.includes('voice') ? { stt: true, tts: true } : p.startsWith('/projects') ? { root: id === 'vps' ? '/home/server/projects' : null, projects: [{ name: 'budget-app', path: '/home/server/projects/budget-app', mtime: 1 }, { name: 'blog', path: '/home/server/projects/blog', mtime: 1 }] } : p.includes('/prefs') ? { call: 'off' } : id === 'vps' ? sessions.slice(2).map((x) => ({ ...x, cwd: '/home/server/projects/blog', title: 'Landing page for the blog' })) : sessions,
  send: async () => 'x', moveProject: async () => ({}), pickFolder: async () => '/home/you/code', stop: async () => true, openExternal: async () => {}, filePath: () => '', voiceStatus: async () => ({ stt: true, tts: true }), stt: async () => '', tts: async () => new ArrayBuffer(0), callAction: async () => true, on: noop,
});
