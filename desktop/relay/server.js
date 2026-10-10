'use strict';
/* The Hearth relay. Lets the phone and desktop apps list Claude Code chats, start new ones in a
   project folder, message them, and get notified (or called) when Claude finishes or needs you.

   Two ways to run it:
   - built into the desktop app: local network only, pairing by 6-digit code
   - standalone on a server (relay/standalone.js): behind HTTPS, token only, confined to one projects folder

   Chats keep running on the server when the phone disconnects; the connection is only a window onto the run. */
const http = require('http');
const https = require('https');
const dgram = require('dgram');
const fs = require('fs');
const os = require('os');
const path = require('path');
const crypto = require('crypto');
const { spawn, spawnSync } = require('child_process');
const core = require('./core');
const chathub = require('./chathub');
const filehub = require('./filehub');
const selfupdate = require('./selfupdate');
const VERSION = require('./version');
const tools = require('./tools');
const zlib = require('zlib');
const BUILD = require('./buildsig').sigOfDir(__dirname);   // what code this relay is running

const DISCOVER_PORT = 47600;
const MODES = ['acceptEdits', 'auto', 'plan', 'manual', 'dontAsk', 'bypassPermissions'];
const ASK_PROMPT = 'When you need the user to choose between options or make a decision, call the mcp__meter__ask_user tool (2-4 short options) instead of writing the options in your reply, then continue with their answer.';
const HUB_PROMPT = "You are running on the user's always-on server. The user also has their own laptop, which syncs the same projects folder and your chats. When something has to happen on that laptop (install dependencies or tools there, run or test the app, anything needing their hardware or screen), use mcp__meter__run_on_laptop: it runs there automatically, continuing this conversation, as soon as the laptop is online. Use mcp__meter__laptop_status to see if it is online.";
const TALK_PROMPT = "This is a casual talking session, not a coding session: chat, brainstorm, explain and answer questions in plain, friendly language. Keep answers short unless asked for more. You cannot edit files or run commands here; if the user wants something built or changed, tell them to start a coding session for it.";
const TALK_DENY = 'Bash Edit Write MultiEdit NotebookEdit Agent';
const BRIEF = 'The user is listening to your reply read aloud. Keep it brief and conversational: short spoken sentences, no markdown, no lists, no code blocks. Say what you did and what you found.';
const MIN_DONE_SECONDS = 20;       // hook-based "done" events for terminal sessions
const PERMISSION_WAIT_MS = 25000;  // terminal sessions: wait this long for the phone, then fall back to the terminal prompt
const JOB_PERMISSION_WAIT_MS = 15 * 60000; // relay-run chats: Claude waits this long for you to approve

const rank = (f) => ({ off: 0, done: 1, attention: 2 }[f] || 0);

/** True for loopback, link-local and private (RFC 1918 / unique-local) addresses only. */
function lanOnly(addr) {
  const a = String(addr || '').replace(/^::ffff:/, '').split('%')[0];
  if (a === '::1' || /^127\./.test(a) || /^10\./.test(a) || /^192\.168\./.test(a) || /^169\.254\./.test(a)) return true;
  if (/^100\.(6[4-9]|[7-9]\d|1[01]\d|12[0-7])\./.test(a)) return true;   // Tailscale
  const m = /^172\.(\d+)\./.exec(a);
  if (m && Number(m[1]) >= 16 && Number(m[1]) <= 31) return true;
  return /^f[cd][0-9a-f]{2}:/i.test(a) || /^fe80:/i.test(a);
}
const isLoopback = (addr) => { const a = String(addr || '').replace(/^::ffff:/, ''); return a === '::1' || /^127\./.test(a); };
/** A request made directly on this machine, not forwarded by a reverse proxy. */
const isLocalDirect = (req) => isLoopback(req.socket.remoteAddress) && !req.headers['x-forwarded-for'];

function findClaude() {
  if (process.env.FLATPAK_ID) return 'claude'; // inside Flatpak: run the host's Claude Code through flatpak-spawn
  const names = process.platform === 'win32' ? ['claude.exe', 'claude.cmd'] : ['claude'];
  const dirs = (process.env.PATH || '').split(path.delimiter);
  dirs.push(path.join(os.homedir(), '.local', 'bin'), path.join(os.homedir(), '.claude', 'local'), path.join(os.homedir(), 'AppData', 'Roaming', 'npm'), path.join(os.homedir(), 'AppData', 'Local', 'Programs', 'claude'));
  for (const d of dirs) for (const n of names) { const p = path.join(d, n); try { if (fs.statSync(p).isFile()) return p; } catch { /* next */ } }
  return null;
}

function start(opts) {
  const dir = opts.dir, port = opts.port || Number(process.env.CM_PORT) || 47601, host = opts.host || '0.0.0.0';
  const reach = (a) => !!opts.allowRemote || lanOnly(a);   // normally only your own network; "reach from outside" is an opt-in for port forwarding
  const publicMode = !!opts.publicMode, allowBypass = opts.allowBypass !== false;
  const onLog = opts.onLog || (() => {});
  let root = null;
  if (opts.projectsRoot) { fs.mkdirSync(opts.projectsRoot, { recursive: true }); root = fs.realpathSync(opts.projectsRoot); }
  const moveRoot = opts.moveRoot ? path.resolve(opts.moveRoot) : null;   // where "move to projects" puts a project (your own computer)
  const encodeCwd = (p) => p.replace(/[^A-Za-z0-9]/g, '-');              // how Claude Code names a project's chat folder
  const claudeDir = (cwd) => path.join(core.projectsDir(), encodeCwd(cwd));
  const under = (p, base) => { const r = path.resolve(p); return r === base || r.startsWith(base + path.sep); };
  const inRoot = (p) => { if (!root) return true; try { const r = fs.realpathSync(p); return r === root || r.startsWith(root + path.sep); } catch { return false; } };

  const cfgFile = path.join(dir, 'relay.json');
  const readCfg = () => { try { return JSON.parse(fs.readFileSync(cfgFile, 'utf8')); } catch { return {}; } };
  const writeCfg = (patch) => { fs.mkdirSync(dir, { recursive: true }); fs.writeFileSync(cfgFile, JSON.stringify({ ...readCfg(), ...patch }), { mode: 0o600 }); };
  if (!readCfg().token) writeCfg({ token: (opts.seedToken || '').trim() || crypto.randomBytes(publicMode ? 32 : 16).toString('base64url') });
  const token = readCfg().token;
  const getPrefs = (sid) => ({ call: 'off', run: 'auto', kind: 'code', ...((readCfg().prefs || {})[sid] || {}) });
  const setPrefs = (sid, p) => { const all = readCfg().prefs || {}; all[sid] = { ...(all[sid] || { call: 'off' }), ...p }; writeCfg({ prefs: all }); return all[sid]; };

  // ── pairing code (desktop app only) ──
  const pair = { code: '', fails: 0, lockedUntil: 0 };
  const newCode = () => { pair.code = String(crypto.randomInt(0, 1000000)).padStart(6, '0'); pair.fails = 0; return pair.code; };
  newCode();
  const codeTimer = setInterval(newCode, 10 * 60000); codeTimer.unref();

  // ── plan usage: read from the real Claude Code's own output (rate_limit_event), never from account tokens ──
  const usageFile = path.join(dir, 'usage.json');
  let usage = (() => { try { return JSON.parse(fs.readFileSync(usageFile, 'utf8')); } catch { return {}; } })();
  const WINDOWS = { five_hour: 'session', seven_day: 'week', seven_day_opus: 'opus', seven_day_sonnet: 'sonnet' };
  function noteRateLimit(info) {
    if (!info) return;
    const wins = info.unifiedWindows || (info.rateLimitType && info.utilization != null ? { [info.rateLimitType]: { utilization: info.utilization, resetsAt: info.resetsAt } } : {});
    const next = { ...usage };
    let got = false;
    for (const [k, v] of Object.entries(wins)) {
      const name = WINDOWS[k];
      if (!name || !v || v.utilization == null) continue;
      next[name] = { pct: Math.max(0, Math.min(100, Number(v.utilization) * 100)), resetsAt: v.resetsAt ? Number(v.resetsAt) * 1000 : null };
      got = true;
    }
    if (!got) return;
    usage = { ...next, at: Date.now() };
    try { fs.writeFileSync(usageFile, JSON.stringify(usage), { mode: 0o600 }); } catch { /* best effort */ }
    if (typeof forwardUsage === 'function') forwardUsage(usage);
  }
  let lastCheck = 0;
  /** One tiny request through the real Claude Code, only to read the limits it reports. Only when asked (or on the opt-in heartbeat). */
  function checkUsage() {
    return new Promise((resolve) => {
      const claude = findClaude();
      if (!claude || Date.now() - lastCheck < 60000) return resolve(usage);
      lastCheck = Date.now();
      const args = ['-p', '--output-format', 'stream-json', '--verbose', '--no-session-persistence', '--model', 'haiku'];
      const useShell = process.platform === 'win32' && /\.(cmd|bat)$/i.test(claude);
      const child = process.env.FLATPAK_ID
        ? spawn('flatpak-spawn', ['--host', 'bash', '-lc', 'exec claude "$@"', 'claude', ...args], { stdio: ['pipe', 'pipe', 'ignore'] })
        : spawn(useShell ? `"${claude}"` : claude, args, { cwd: os.tmpdir(), shell: useShell, windowsHide: true, stdio: ['pipe', 'pipe', 'ignore'] });
      child.stdin.end('Reply with the word ok.');
      let buf = '';
      child.stdout.setEncoding('utf8');
      child.stdout.on('data', (d) => { buf += d; let i; while ((i = buf.indexOf('\n')) >= 0) { const l = buf.slice(0, i); buf = buf.slice(i + 1); try { const o = JSON.parse(l); if (o.type === 'rate_limit_event') noteRateLimit(o.rate_limit_info); } catch { /* skip */ } } });
      const t = setTimeout(() => child.kill(), 60000);
      child.on('close', () => { clearTimeout(t); resolve(usage); });
      child.on('error', () => { clearTimeout(t); resolve(usage); });
    });
  }
  const beat = Number(readCfg().heartbeatMinutes) || 0;   // opt-in: refresh the limits every N minutes even when idle
  if (beat > 0) setInterval(checkUsage, Math.max(5, beat) * 60000).unref();

  /** Replaces a project's old path with its new one inside the copied chat files. */
  function rewritePaths(d, from, to) {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, e.name);
      if (e.isDirectory()) { rewritePaths(p, from, to); continue; }
      if (!/\.(jsonl|md|json)$/.test(e.name) || fs.statSync(p).size > 400e6) continue;
      const txt = fs.readFileSync(p, 'utf8');
      if (txt.includes(from)) fs.writeFileSync(p, txt.split(from).join(to));
    }
  }
  /** Progress of the Syncthing folder that mirrors the projects folder, if Syncthing runs on this machine. */
  function syncStatus(folder) {
    return new Promise((resolve) => {
      const cands = [path.join(os.homedir(), '.local/state/syncthing/config.xml'), path.join(os.homedir(), '.config/syncthing/config.xml'), path.join(process.env.LOCALAPPDATA || '', 'Syncthing/config.xml')];
      let key = '', addr = '127.0.0.1:8384';
      for (const f of cands) { try { const x = fs.readFileSync(f, 'utf8'); key = (/<apikey>([^<]+)</.exec(x) || [])[1] || ''; addr = (/<gui[^>]*>[\s\S]*?<address>([^<]+)</.exec(x) || [])[1] || addr; if (key) break; } catch { /* next */ } }
      if (!key) return resolve({ available: false });
      const [h, p] = addr.split(':');
      const r = http.get({ host: h, port: Number(p), path: '/rest/db/status?folder=' + encodeURIComponent(folder), headers: { 'X-API-Key': key }, timeout: 4000 }, (rs) => {
        let b = ''; rs.on('data', (d) => { b += d; }); rs.on('end', () => { try { const o = JSON.parse(b); resolve({ available: true, state: o.state, needFiles: o.needFiles, needBytes: o.needBytes, localFiles: o.localFiles, globalFiles: o.globalFiles }); } catch { resolve({ available: false }); } });
      });
      r.on('error', () => resolve({ available: false })); r.on('timeout', () => { r.destroy(); resolve({ available: false }); });
    });
  }

  // ── events (messages and calls) ──
  const clients = new Set();
  let seq = 0;
  const events = [];
  const pending = new Map(); // permission request id -> {resolve, event}
  const BOOT = crypto.randomBytes(4).toString('hex'); // lets the phone notice a restarted relay (event numbers start over)
  let forward = null, forwardUsage = null;   // set when this computer is linked to a server: its events are passed on so your phone gets them anywhere
  const publish = (e) => {
    e.id = ++seq; e.boot = BOOT; events.push(e); if (events.length > 200) events.shift();
    for (const c of clients) c.write(`id: ${e.id}\ndata: ${JSON.stringify(e)}\n\n`);
    if (forward && !e.remote) forward(e);
    return e;
  };

  // ── hub: a server that hands work to your laptop whenever it is online ──
  const workFile = path.join(dir, 'work.json');
  let work = (() => { try { return JSON.parse(fs.readFileSync(workFile, 'utf8')); } catch { return []; } })();
  const saveWork = () => { try { fs.writeFileSync(workFile, JSON.stringify(work), { mode: 0o600 }); } catch { /* best effort */ } };
  const agents = new Map();          // laptop name -> { seen }
  const remotePending = new Map();   // req -> { name } for questions/permissions that live on a linked laptop
  const answersFor = new Map();      // laptop name -> answers waiting to be fetched
  const waiters = new Set();         // laptops waiting on a long poll
  const AGENT_FRESH_MS = 45000, CLAIM_MS = 15 * 60000;
  const laptopOnline = () => { let best = null; for (const [n, a] of agents) if (Date.now() - a.seen < AGENT_FRESH_MS) best = n; return best; };
  const jobBusy = (k) => { const j = k && jobs.get(k); return !!(j && !j.done); };
  let chatEpoch = 1;   // bumped whenever the server's chats change, so linked computers sync at once instead of on their next timer
  const readyFor = (name) => ({
    work: work.filter((w) => (!w.target || w.target === name) && (!w.claimedAt || Date.now() - w.claimedAt > CLAIM_MS) && !jobBusy(w.afterJob)).map((w) => {
      if (w.afterJob) { const f = core.findSession(w.session); if (f) w.chatMtime = f.mtime; delete w.afterJob; }
      w.claimedAt = Date.now(); w.by = name; return w; }),
    answers: answersFor.get(name) || [],
  });
  const wake = () => { for (const w of [...waiters]) { const r = readyFor(w.name); if (r.work.length || r.answers.length || w.epoch !== chatEpoch) { clearTimeout(w.timer); waiters.delete(w); answersFor.set(w.name, []); saveWork(); json(w.res, 200, { ...r, epoch: chatEpoch }); } } };
  const bumpChats = () => { chatEpoch++; wake(); };
  const isOnline = (name) => { const a = agents.get(name); return !!a && Date.now() - a.seen < AGENT_FRESH_MS; };
  const hubPair = { code: '', expires: 0, fails: 0, lockedUntil: 0 };
  const lastQuestion = new Map();
  let link = null, linkGen = 0;   // this computer's link to a server (laptop side)
  const pendingByKey = new Map();
  const away = () => !!readCfg().away;

  // ── jobs: Claude runs that outlive the connection that started them ──
  const jobs = new Map(); // session id (or temporary key) -> job
  const runningIds = () => new Set([...jobs.values()].filter((j) => !j.done).map((j) => j.sid || j.key));
  const baseFor = (sid, cwdHint) => {
    const f = core.findSession(sid), s = f ? core.summarize(f) : null;
    return { session: sid, title: s ? s.title : 'Claude Code', cwd: (s && s.cwd) || cwdHint || '', machine: opts.name || os.hostname(), ts: Date.now() };
  };
  function turnInfo(sid) {
    const f = core.findSession(sid);
    let started = 0, last = '';
    if (f) for (const o of core.readJsonl(f.file)) {
      if (o.isSidechain) continue;
      const c = (o.message || {}).content;
      if (o.type === 'user' && core.textOf(c).trim()) started = core.isoTs(o);
      else if (o.type === 'assistant' && Array.isArray(c) && core.textOf(c).trim()) last = core.textOf(c).trim();
    }
    return { seconds: started ? Math.floor(Date.now() / 1000 - started) : 0, last };
  }
  const describe = (tool, arg) => { const k = ['command', 'file_path', 'pattern', 'path', 'url'].find((x) => arg && x in arg); return `${tool}: ${k ? String(arg[k]).slice(0, 160) : ''}`.replace(/: $/, ''); };

  /** Claude Code hook calls. Returns the exact JSON the hook should print. */
  async function handleHook(h) {
    const name = h.hook_event_name, sid = h.session_id || '', tool = h.tool_name || '';
    const job = jobs.get(sid);
    const mine = !!job && !job.done;            // a chat this relay started: always notify, even when not "away"
    if (!mine && (!away() || clients.size === 0)) return {};
    const base = baseFor(sid, h.cwd);
    const prefs = getPrefs(sid);
    if (name === 'Stop' && !mine) {
      const { seconds, last } = turnInfo(sid);
      if (seconds >= MIN_DONE_SECONDS) publish({ ...base, kind: 'done', call: true, text: last.slice(0, 200), seconds });
    } else if (tool === 'AskUserQuestion' && (name === 'PreToolUse' || name === 'PermissionRequest')) {
      if (Date.now() - (lastQuestion.get(sid) || 0) > 10000) {
        lastQuestion.set(sid, Date.now());
        const qs = (h.tool_input && h.tool_input.questions) || [{}];
        publish({ ...base, kind: 'question', call: mine ? prefs.call === 'attention' : true, text: String(qs[0].question || 'Claude has a question').slice(0, 200) });
      }
    } else if (name === 'PermissionRequest') {
      // the same prompt can reach us twice (a hook in your Claude settings plus the one added for this run): share one question
      const key = sid + '|' + tool + '|' + JSON.stringify(h.tool_input || {});
      let shared = pendingByKey.get(key);
      if (!shared) {
        const req = crypto.randomBytes(4).toString('hex');
        const promise = new Promise((resolve) => {
          const t = setTimeout(() => { pending.delete(req); resolve(null); }, mine ? JOB_PERMISSION_WAIT_MS : PERMISSION_WAIT_MS);
          const ev = { ...base, kind: 'permission', call: mine ? prefs.call === 'attention' : true, req, tool, text: describe(tool, h.tool_input) };
          pending.set(req, { event: ev, resolve: (b) => { clearTimeout(t); pending.delete(req); resolve(b); } });
          publish(ev);
        });
        shared = { promise };
        pendingByKey.set(key, shared);
        promise.finally(() => pendingByKey.delete(key));
      }
      const decision = await shared.promise;
      if (decision === 'allow' || decision === 'deny') {
        const d = { behavior: decision };
        if (decision === 'deny') d.message = 'Denied from your phone';
        return { hookSpecificOutput: { hookEventName: 'PermissionRequest', decision: d } };
      }
    }
    return {};
  }

  /** Speech-to-text and text-to-speech come from an optional local engine (Parakeet + Kokoro) on another port. */
  function proxyVoice(req, res) {
    const vp = opts.voice;
    const none = () => (req.method === 'GET' ? json(res, 200, { stt: false, tts: false, stt_engine: '', tts_engine: '' }) : json(res, 503, { error: 'voice engine not running' }));
    if (!vp) return none();
    let tok = ''; try { tok = fs.readFileSync(vp.tokenFile, 'utf8').trim(); } catch { /* no engine */ }
    const headers = { Authorization: 'Bearer ' + tok };
    for (const h of ['content-type', 'content-length']) if (req.headers[h]) headers[h] = req.headers[h];
    const pr = http.request({ host: '127.0.0.1', port: vp.port, path: req.url, method: req.method, headers, timeout: 120000 }, (r) => {
      const h = { 'Content-Type': r.headers['content-type'] || 'application/json' };
      if (r.headers['content-length']) h['Content-Length'] = r.headers['content-length'];
      res.writeHead(r.statusCode, h); r.pipe(res);
    });
    pr.on('error', none); pr.on('timeout', () => pr.destroy());
    req.pipe(pr);
    return undefined;
  }

  // ── http plumbing ──
  const json = (res, code, obj) => { const b = Buffer.from(JSON.stringify(obj)); res.writeHead(code, { 'Content-Type': 'application/json', 'Content-Length': b.length }); res.end(b); };
  const readBody = (req, max = 10e6) => new Promise((resolve, reject) => {
    const chunks = []; let n = 0;
    req.on('data', (d) => { n += d.length; if (n > max) { reject(new Error('too large')); req.destroy(); } else chunks.push(d); });
    req.on('end', () => resolve(Buffer.concat(chunks)));
    req.on('error', reject);
  });
  const bodyJson = async (req) => { try { return JSON.parse((await readBody(req)).toString() || '{}'); } catch { return {}; } };
  const clientIp = (req) => (isLoopback(req.socket.remoteAddress) && req.headers['x-forwarded-for'] ? String(req.headers['x-forwarded-for']).split(',')[0].trim() : req.socket.remoteAddress);
  const failures = new Map(); // ip -> {n, until}
  const authed = (req, res) => {
    const ip = clientIp(req);
    if (!publicMode && !reach(ip)) { json(res, 403, { error: 'local network only' }); return false; }
    const f = failures.get(ip);
    if (f && f.until > Date.now()) { json(res, 429, { error: 'too many failed attempts, try again later' }); return false; }
    const got = Buffer.from(req.headers.authorization || ''), want = Buffer.from('Bearer ' + token);
    if (got.length === want.length && crypto.timingSafeEqual(got, want)) { failures.delete(ip); return true; }
    const n = ((f && f.n) || 0) + 1;
    failures.set(ip, { n, until: n >= 8 ? Date.now() + 15 * 60000 : 0 });
    if (n >= 8) onLog(`blocked ${ip} after ${n} bad tokens`);
    json(res, 401, { error: 'bad token' });
    return false;
  };

  // ── projects ──
  function listProjects() {
    if (!root) {
      const seen = new Map();
      for (const s of core.sessions(new Set())) if (s.cwd && !seen.has(s.cwd)) seen.set(s.cwd, s.mtime);
      return { root: null, projects: [...seen.entries()].map(([p, m]) => ({ name: path.basename(p), path: p, mtime: m })) };
    }
    const byCwd = {};
    for (const s of core.sessions(new Set())) byCwd[s.cwd] = Math.max(byCwd[s.cwd] || 0, s.mtime);
    const out = fs.readdirSync(root, { withFileTypes: true }).filter((d) => d.isDirectory() && !d.name.startsWith('.')).map((d) => {
      const p = path.join(root, d.name);
      let m = 0; try { m = fs.statSync(p).mtimeMs; } catch { /* ignore */ }
      return { name: d.name, path: p, mtime: Math.max(m, byCwd[p] || 0) };
    }).sort((a, b) => b.mtime - a.mtime);
    return { root, projects: out };
  }
  /** Shown in the list: inside the projects folder, and the folder is there (or is still arriving through the file sync). */
  const visibleSession = (s) => {
    if (!root) return true;
    if (!s.cwd || !under(s.cwd, root)) return false;
    try { return fs.statSync(s.cwd).isDirectory(); } catch { return Date.now() - (s.mtime || 0) < 15 * 60000; }
  };
  /** A chat worth listing on a computer without a projects root: its folder exists, it is not a temp folder, and it is not empty. */
  const worthListing = (f, s) => !!chathub.describeChat(f, moveRoot || root);

  // ── running a turn ──
  function startJob(body, sidParam) {
    const claude = findClaude();
    if (!claude) return { error: 500, message: 'Claude Code was not found on this computer. Install it first: https://claude.com/claude-code' };
    let mode = MODES.includes(body.mode) ? body.mode : (publicMode ? 'auto' : 'acceptEdits');
    if (mode === 'bypassPermissions' && !allowBypass) mode = 'auto';
    const settingsFile = path.join(dir, 'run-settings.json');
    const hookCmd = `curl -s -m ${Math.round(JOB_PERMISSION_WAIT_MS / 1000)} -X POST -H "Content-Type: application/json" --data-binary @- http://127.0.0.1:${port}/hook`;
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(settingsFile, JSON.stringify({ hooks: { PermissionRequest: [{ matcher: '', hooks: [{ type: 'command', command: hookCmd, timeout: Math.round(JOB_PERMISSION_WAIT_MS / 1000) }] }] } }));
    const args = ['-p', '--output-format', 'stream-json', '--verbose', '--include-partial-messages', '--permission-mode', mode, '--settings', settingsFile];
    const kind = body.kind === 'talk' || (sidParam !== 'new' && getPrefs(sidParam).kind === 'talk') ? 'talk' : 'code';
    if (kind === 'talk') args.push('--model', 'haiku', '--disallowedTools', TALK_DENY);   // talking sessions always use the cheap model and cannot touch files
    else if (['opus', 'sonnet', 'haiku'].includes(body.model)) args.push('--model', body.model);
    if (kind !== 'talk' && ['low', 'medium', 'high', 'xhigh', 'max'].includes(body.effort)) args.push('--effort', body.effort);
    let cwd, key;
    if (sidParam === 'new') {
      cwd = kind === 'talk' ? path.join(root || moveRoot || os.homedir(), 'Talks') : String(body.cwd || '');
      if (kind === 'talk') { try { fs.mkdirSync(cwd, { recursive: true }); } catch { /* checked below */ } }
      try { if (!fs.statSync(cwd).isDirectory()) throw new Error(); } catch { return { error: 400, message: "that folder doesn't exist on this computer" }; }
      if (!inRoot(cwd)) return { error: 403, message: 'new chats must be inside the projects folder' };
      key = 'new-' + crypto.randomBytes(4).toString('hex');
    } else {
      const f = core.findSession(sidParam);
      if (!f) return { error: 404, message: 'no such session' };
      const s = core.summarize(f);
      if (!visibleSession(s)) return { error: 403, message: 'that chat is outside the projects folder' };
      const old = jobs.get(sidParam);
      if (old && !old.done) return { error: 409, message: 'Claude is still working on this chat' };
      cwd = s.cwd || os.homedir();
      if (!fs.existsSync(cwd)) return { error: 409, message: "The project's files haven't reached this computer yet (still syncing). Try again in a moment." };
      if (root && !inRoot(cwd)) return { error: 403, message: 'that chat is outside the projects folder' };
      args.push('--resume', sidParam);
      key = sidParam;
    }
    const mcpFile = path.join(dir, `ask-${key}.json`);
    const asarFix = (p) => p;
    fs.writeFileSync(mcpFile, JSON.stringify({ mcpServers: { meter: process.versions.electron
      ? { command: process.execPath, args: [asarFix(path.join(__dirname, 'ask-mcp.js'))], env: { ELECTRON_RUN_AS_NODE: '1', CM_RELAY_PORT: String(port), CM_JOB: key, CM_HUB: publicMode ? '1' : '' } }
      : { command: process.execPath, args: [path.join(__dirname, 'ask-mcp.js')], env: { CM_RELAY_PORT: String(port), CM_JOB: key, CM_HUB: publicMode ? '1' : '' } } } }));
    args.push('--mcp-config', mcpFile, '--allowedTools', publicMode ? 'mcp__meter__ask_user mcp__meter__laptop_status mcp__meter__run_on_laptop' : 'mcp__meter__ask_user',
      '--append-system-prompt', ASK_PROMPT + (publicMode && kind !== 'talk' ? ' ' + HUB_PROMPT : '') + (kind === 'talk' ? ' ' + TALK_PROMPT : '') + (body.brief ? ' ' + BRIEF : ''));
    const useShell = process.platform === 'win32' && /\.(cmd|bat)$/i.test(claude);
    const child = process.env.FLATPAK_ID
      ? spawn('flatpak-spawn', ['--host', `--directory=${cwd}`, 'bash', '-lc', 'exec claude "$@"', 'claude', ...args], { stdio: ['pipe', 'pipe', 'ignore'] })
      : spawn(useShell ? `"${claude}"` : claude, useShell ? args.map((a) => (/[\s]/.test(a) ? `"${a}"` : a)) : args, { cwd, shell: useShell, windowsHide: true, stdio: ['pipe', 'pipe', 'ignore'] });
    child.stdin.end(String(body.text || ''));  // the prompt goes in on stdin, never on the command line
    const job = { key, sid: sidParam === 'new' ? '' : sidParam, child, events: [], listeners: new Set(), done: false, started: Date.now(), cwd, text: '', error: false, wantCall: null, wantKind: kind === 'talk' && sidParam === 'new' ? 'talk' : null };
    jobs.set(key, job);
    if (agentSync) agentSync.kick('job-start');
    // "call me when it's done" typed into the chat sets the call preference for this chat
    const det = detectCall(String(body.text || ''));
    if (det) { if (job.sid) setPrefs(job.sid, { call: det }); else job.wantCall = det; emit(job, { t: 'prefs', call: det }); }

    let buf = '';
    child.stdout.setEncoding('utf8');
    child.stdout.on('data', (d) => {
      buf += d;
      let i;
      while ((i = buf.indexOf('\n')) >= 0) {
        const line = buf.slice(0, i); buf = buf.slice(i + 1);
        let o; try { o = JSON.parse(line); } catch { continue; }
        const t = o.type;
        if (t === 'system' && o.subtype === 'init') {
          if (o.session_id && !job.sid) { job.sid = o.session_id; jobs.set(job.sid, job); if (job.wantCall) setPrefs(job.sid, { call: job.wantCall }); if (job.wantKind) setPrefs(job.sid, { kind: job.wantKind }); }
          emit(job, { t: 'session', id: o.session_id || '', model: o.model || '' });
        } else if (t === 'stream_event') {
          const dl = ((o.event || {}).delta) || {};
          if (dl.type === 'text_delta') { job.text += dl.text; emit(job, { t: 'delta', text: dl.text }); }
        } else if (t === 'assistant') {
          for (const b of ((o.message || {}).content) || []) if (b.type === 'tool_use') emit(job, { t: 'tool', id: b.id || '', name: b.name || '', input: core.toolInput(b.name, b.input), text: core.toolLine(b) });
        } else if (t === 'user') {
          const c = (o.message || {}).content;
          if (Array.isArray(c)) for (const b of c) if (b.type === 'tool_result') emit(job, { t: 'result', id: b.tool_use_id || '', text: core.resultText(b.content), error: !!b.is_error });
        } else if (t === 'rate_limit_event') {
          noteRateLimit(o.rate_limit_info);
        } else if (t === 'result') {
          job.error = !!o.is_error; job.resultText = typeof o.result === 'string' ? o.result : '';
          const us = o.usage || {};
          emit(job, { t: 'done', error: job.error, ms: o.duration_ms, cost: o.total_cost_usd, usage: { in: us.input_tokens || 0, out: us.output_tokens || 0, cr: us.cache_read_input_tokens || 0, cw: us.cache_creation_input_tokens || 0 } });
        }
      }
    });
    const finish = (err) => {
      if (job.done) return;
      job.done = true;
      if (err) { job.error = true; emit(job, { t: 'done', error: true, text: String(err.message || err) }); }
      emit(job, { t: 'end' });
      for (const r of job.listeners) { try { r.end(); } catch { /* closed */ } }
      job.listeners.clear();
      announceDone(job);
      bumpChats(); if (agentSync) agentSync.kick('job-done');
      try { fs.unlinkSync(mcpFile); } catch { /* gone */ }
      setTimeout(() => { for (const [k, v] of jobs) if (v === job) jobs.delete(k); }, 120000).unref();
    };
    child.on('close', () => finish());
    child.on('error', (e) => finish(e));
    return { job };
  }

  function emit(job, o) {
    o.n = job.events.length;
    if (job.events.length < 20000) job.events.push(o);
    for (const r of job.listeners) { try { r.write(JSON.stringify(o) + '\n'); } catch { /* client left */ } }
  }
  function attach(res, job, from) {
    res.writeHead(200, { 'Content-Type': 'application/x-ndjson', 'Cache-Control': 'no-cache', 'X-Accel-Buffering': 'no' });
    for (let i = from; i < job.events.length; i++) res.write(JSON.stringify(job.events[i]) + '\n');
    if (job.done) return res.end();
    job.listeners.add(res);
    res.on('close', () => job.listeners.delete(res));  // the run keeps going without us
  }

  /** The milestone message: always sent; the phone rings only if you asked for a call. */
  function announceDone(job) {
    const sid = job.sid;
    if (!sid) return;
    const prefs = getPrefs(sid);
    const base = baseFor(sid, job.cwd);
    const seconds = Math.round((Date.now() - job.started) / 1000);
    const text = (job.error ? 'Something went wrong. ' : '') + (job.text.trim() || job.resultText || '').slice(-220);
    publish({ ...base, kind: job.error ? 'error' : 'done', call: prefs.call !== 'off' && !job.error ? true : prefs.call === 'attention', text: text.trim(), seconds });
    if (prefs.call === 'done') setPrefs(sid, { call: 'off' });  // a one-time "call me when it's done"
  }
  function detectCall(text) {
    if (!/\b(call|ring|phone) me\b/i.test(text)) return null;
    return /permission|need me|stuck|question|attention|input|approve|asks?\b/i.test(text) ? 'attention' : 'done';
  }

  // ── "ask to connect": a phone on your Wi-Fi asks, the person at this computer says yes or no ──
  const pairReqs = new Map();   // id -> { name, ip, at, status: 'pending' | 'accepted' | 'declined', delivered }
  const askedAt = new Map();    // ip -> when it last asked or was declined
  const pairLive = () => { for (const [id, r] of pairReqs) if (Date.now() - r.at > 120000 || (r.delivered && Date.now() - r.at > 30000)) pairReqs.delete(id); };
  const pairPoll = (id) => {   // what the phone asks every second or two; the key is only handed out once
    pairLive(); const r = pairReqs.get(id); if (!r) return { status: 'expired' };
    if (r.status === 'accepted' && !r.delivered) { r.delivered = true; return { status: 'accepted', token, name: os.hostname(), server: link ? { url: link.url, token: link.token, name: link.name || 'Server' } : null }; }
    return { status: r.status === 'accepted' ? 'done' : r.status };
  };
  const pairPending = () => { pairLive(); return [...pairReqs].filter(([, r]) => r.status === 'pending').map(([id, r]) => ({ id, name: r.name, ip: r.ip, at: r.at })); };

  // ── routes ──
  const server = http.createServer(async (req, res) => {
    try {
      const u = new URL(req.url, 'http://x'), parts = u.pathname.replace(/^\/|\/$/g, '').split('/');
      const ip = clientIp(req);
      if (parts[0] === 'tools') {   // git changes, project notes, connections
        if (!authed(req, res)) return undefined;
        const r = await tools.route(parts.slice(1), u, req.method === 'GET' ? {} : await bodyJson(req), { inRoot, claude: findClaude() });
        return json(res, r.code || 200, r.data);
      }
      if (req.method === 'GET') {
        if (parts[0] === 'info') { if (publicMode ? !authed(req, res) : !reach(ip)) return undefined; return json(res, 200, { name: opts.name || os.hostname(), claude: !!findClaude(), root, public: publicMode }); }
        if (parts[0] === 'pair' && parts[1] === 'request' && parts[2]) {   // the phone checks on its request
          if (publicMode || !reach(ip)) return json(res, 403, { error: 'not available here' });
          return json(res, 200, pairPoll(String(parts[2])));
        }
        if (!authed(req, res)) return undefined;
        if (parts[0] === 'status') return json(res, 200, { away: away(), build: BUILD, version: VERSION, canSelfUpdate: !!opts.selfUpdate, listeners: clients.size, root, allowBypass, moveRoot, laptop: laptopOnline(), queued: work.length });
        if (parts[0] === 'voice' && parts[1] === 'status') return proxyVoice(req, res);
        if (parts[0] === 'stats') return json(res, 200, core.stats(Math.max(1, Math.min(90, Number(u.searchParams.get('days')) || 30))));
        if (parts[0] === 'commands') { const cwd = String(u.searchParams.get('cwd') || ''); return json(res, 200, await tools.commands(cwd && (!root || inRoot(cwd)) ? cwd : '', findClaude())); }
        if (parts[0] === 'projects' && parts.length === 1) return json(res, 200, listProjects());
        if (parts[0] === 'usage') return json(res, 200, usage);
        if (parts[0] === 'projects' && parts[1] === 'exists') { const c = String(u.searchParams.get('cwd') || ''); let ok = false; try { ok = fs.statSync(c).isDirectory() && (!root || inRoot(c)); } catch { /* no */ } return json(res, 200, { exists: ok }); }
        if (parts[0] === 'sync' && !parts[1]) return json(res, 200, await syncStatus(u.searchParams.get('folder') || 'claude-projects'));
        if (parts[0] === 'projects' && parts[1] === 'export') {
          const cwd = String(u.searchParams.get('cwd') || ''), dirp = claudeDir(cwd);
          if (!cwd || !fs.existsSync(dirp)) return json(res, 404, { error: 'no chats for that project' });
          res.writeHead(200, { 'Content-Type': 'application/gzip' });
          const tar = spawn('tar', ['-czf', '-', '-C', dirp, '.'], { stdio: ['ignore', 'pipe', 'ignore'] });
          tar.stdout.pipe(res);
          res.on('close', () => tar.kill());
          return undefined;
        }
        if (parts[0] === 'agent' && parts[1] === 'chats' && parts[2] === 'get') {   // a computer downloads a chat the server has newer
          const r = hubSync && hubSync.serve(String(u.searchParams.get('id') || ''), Number(u.searchParams.get('offset')) || 0, String(u.searchParams.get('sha') || ''));
          if (!r) return json(res, 404, { error: 'no such chat' });
          const body = chathub.gz(r.body);
          res.writeHead(200, { 'Content-Type': 'application/octet-stream', 'Content-Length': body.length, 'X-Gzip': '1', 'X-Delta': r.delta ? '1' : '0', 'X-Mtime': String(Math.floor(r.mtime)), 'X-Rel': encodeURIComponent(r.rel) });
          return res.end(body);
        }
        if (parts[0] === 'agent' && parts[1] === 'files' && parts[2] === 'manifest') {   // every file the server has
          if (!hubFiles) return json(res, 404, { error: 'this is not the server' });
          const body = zlib.gzipSync(Buffer.from(JSON.stringify({ files: hubFiles.manifest() })));
          res.writeHead(200, { 'Content-Type': 'application/octet-stream', 'Content-Length': body.length, 'X-Gzip': '1' });
          return res.end(body);
        }
        if (parts[0] === 'agent' && parts[1] === 'files' && parts[2] === 'get') {
          const f = hubFiles && hubFiles.get(String(u.searchParams.get('rel') || ''));
          if (!f) return json(res, 404, { error: 'no such file' });
          const body = zlib.gzipSync(f.body, { level: 3 });
          res.writeHead(200, { 'Content-Type': 'application/octet-stream', 'Content-Length': body.length, 'X-Gzip': '1', 'X-Mtime': String(f.mtime), 'X-Mode': String(f.mode) });
          return res.end(body);
        }
        if (parts[0] === 'admin' && parts[1] === 'latest') {   // is there a newer Hearth on GitHub than the one running here?
          try { const l = await selfupdate.latest(); return json(res, 200, { current: VERSION, latest: l.version, newer: selfupdate.newer(l.version, VERSION), notes: l.notes, canSelfUpdate: !!opts.selfUpdate }); }
          catch (e) { return json(res, 502, { error: 'Could not look on GitHub: ' + e.message }); }
        }
        if (parts[0] === 'pair' && parts[1] === 'requests') return json(res, 200, pairPending());
        if (parts[0] === 'sync' && parts[1] === 'status') return json(res, 200, await syncOverview());
        if (parts[0] === 'pending') return json(res, 200, [...pending.values()].map((p) => p.event));
        if (parts[0] === 'sessions' && parts.length === 1) {
          const run = runningIds(), far = hubSync ? hubSync.remoteRunning() : new Set();
          const own = core.sessions(run, (f, sm) => (root ? visibleSession({ ...sm }) : worthListing(f, sm))).map((x) => ({ ...x, running: run.has(x.id) || far.has(x.id), kind: getPrefs(x.id).kind }));
          const have = new Set(own.map((x) => x.id));
          const away = hubSync ? hubSync.elsewhere().filter((e) => !have.has(e.id) && e.cwd).map((e) => ({ id: e.id, title: e.title, cwd: e.cwd, mtime: Math.floor(e.mtime), live: false, busy: false, running: far.has(e.id), kind: getPrefs(e.id).kind, elsewhere: e.machine })) : [];
          return json(res, 200, [...own, ...away].sort((x, y) => y.mtime - x.mtime));
        }
        if (parts[0] === 'sessions' && parts[2] === 'messages') {
          const ew = hubSync && hubSync.findElsewhere(parts[1]);
          const tail = Math.max(1, Math.min(1000, Number(u.searchParams.get('tail')) || 80));
          // messages that were sent but wait for a laptop that is off: show them, so a sent prompt never seems to vanish
          const waiting = work.filter((w) => w.session === parts[1] && w.text).map((w) => ({ role: 'user', text: String(root ? w.text.split(chathub.MARK).join(root) : w.text), pending: true }));
          const withWaiting = (list) => { const n = (t) => String(t || '').replace(/\s+/g, ' ').trim(), seen = list.filter((m) => m.role === 'user').slice(-40).map((m) => n(m.text)); return [...list, ...waiting.filter((w) => !seen.includes(n(w.text)))]; };
          if (ew) return json(res, 200, withWaiting(core.messagesOf(ew, tail)));
          const f = core.findSession(parts[1]);
          if (f && !visibleSession(core.summarize(f))) return json(res, 403, { error: 'outside the projects folder' });
          return json(res, 200, withWaiting(core.messages(parts[1], tail)));
        }
        if (parts[0] === 'sessions' && parts[2] === 'prefs') return json(res, 200, getPrefs(parts[1]));
        if (parts[0] === 'sessions' && parts[2] === 'stream') {
          const job = jobs.get(parts[1]);
          if (!job) return json(res, 404, { error: 'not running' });
          return attach(res, job, Math.max(0, Number(u.searchParams.get('from')) || 0));
        }
        if (parts[0] === 'events') {
          res.writeHead(200, { 'Content-Type': 'text/event-stream', Connection: 'keep-alive', 'Cache-Control': 'no-cache', 'X-Accel-Buffering': 'no' });
          res.write(': hello\n\n');
          const since = Number(u.searchParams.get('since') || req.headers['last-event-id']);
          if (Number.isFinite(since) && since >= 0) for (const e of events) if (e.id > since) res.write(`id: ${e.id}\ndata: ${JSON.stringify(e)}\n\n`);
          for (const p of pending.values()) if (!(Number.isFinite(since) && p.event.id <= since)) res.write(`id: ${p.event.id}\ndata: ${JSON.stringify(p.event)}\n\n`);
          clients.add(res);
          const ping = setInterval(() => res.write(': ping\n\n'), 15000);
          res.on('close', () => { clearInterval(ping); clients.delete(res); });
          return undefined;
        }
        return json(res, 404, { error: 'not found' });
      }
      if (req.method === 'POST') {
        if (parts[0] === 'pair' && !parts[1] && publicMode) {  // a phone pairs with the server using the code your desktop app showed
          if (Date.now() < hubPair.lockedUntil) return json(res, 429, { error: 'too many tries, wait a minute' });
          const b = await bodyJson(req);
          if (hubPair.code && Date.now() < hubPair.expires && String(b.code || '') === hubPair.code) { hubPair.code = ''; return json(res, 200, { token, name: opts.name || os.hostname() }); }
          if (++hubPair.fails >= 5) { hubPair.fails = 0; hubPair.lockedUntil = Date.now() + 60000; }
          return json(res, 403, { error: 'wrong or expired code' });
        }
        if (parts[0] === 'pair' && parts[1] === 'request') {   // a phone on your network asks to connect (no code needed: the person here must accept)
          if (publicMode || !reach(ip)) return json(res, 403, { error: 'not available here' });
          const b = await bodyJson(req);
          if (Date.now() - (askedAt.get(ip) || 0) < 3000) return json(res, 429, { error: 'wait a moment' });
          askedAt.set(ip, Date.now()); pairLive();
          if (pairPending().length >= 4) return json(res, 429, { error: 'too many requests waiting' });
          const id = crypto.randomBytes(16).toString('hex');
          pairReqs.set(id, { name: String(b.name || 'A phone').replace(/[^\p{L}\p{N} ._()-]/gu, '').slice(0, 40) || 'A phone', ip: String(ip).replace(/^::ffff:/, ''), at: Date.now(), status: 'pending', delivered: false });
          return json(res, 200, { id });
        }
        if (parts[0] === 'pair' && !parts[1]) {  // desktop app only: the 6-digit code on screen is the proof
          if (publicMode || !reach(ip)) return json(res, 403, { error: 'not available here' });
          if (Date.now() < pair.lockedUntil) return json(res, 429, { error: 'too many tries, wait a minute' });
          const b = await bodyJson(req);
          if (String(b.code || '') === pair.code) { newCode(); return json(res, 200, { token, name: os.hostname() }); }
          if (++pair.fails >= 5) { newCode(); pair.lockedUntil = Date.now() + 60000; }
          return json(res, 403, { error: 'wrong code' });
        }
        if (parts[0] === 'mcp' && parts[1] === 'hub') {  // run_on_laptop / laptop_status from a chat on this server
          if (!isLocalDirect(req)) return json(res, 403, { error: 'local only' });
          const q = await bodyJson(req), job = jobs.get(String(q.job || ''));
          const on = laptopOnline();
          if (q.tool === 'laptop_status') {
            const mine = work.filter((w) => !w.done).map((w) => '- ' + (w.rel || 'projects') + ': ' + String(w.text).slice(0, 80));
            return json(res, 200, { text: (on ? `The laptop (${on}) is online now.` : 'The laptop is offline; queued tasks run when it is next opened.') + (mine.length ? '\nWaiting tasks:\n' + mine.join('\n') : '\nNo tasks are waiting.') });
          }
          if (!job || !job.sid) return json(res, 200, { text: 'This only works inside a chat.' });
          const relp = root ? path.relative(root, job.cwd) : '';
          work.push({ id: crypto.randomBytes(6).toString('hex'), session: job.sid, rel: relp, text: (root ? String(q.task || '').split(root).join(chathub.MARK) : String(q.task || '')).slice(0, 8000), mode: 'auto', model: '', call: getPrefs(job.sid).call, created: Date.now(), afterJob: job.key, fromServer: true });
          saveWork(); setTimeout(wake, 500);
          publish({ ...baseFor(job.sid, job.cwd), kind: 'queued', call: false, text: on ? 'Handed to your laptop; it starts once this reply is finished.' : 'Waiting for your laptop: it starts when you open it.' });
          return json(res, 200, { text: on ? `Queued. The laptop (${on}) is online and starts as soon as you finish this reply.` : 'Queued. The laptop is offline; it starts automatically the next time the user opens it. They get a notification when it is done.' });
        }
        if (parts[0] === 'question') {  // from the ask_user tool of a chat this relay started
          if (!isLocalDirect(req)) return json(res, 403, { error: 'local only' });
          const q = await bodyJson(req), job = jobs.get(String(q.job || ''));
          const sid = job ? job.sid : '', base = baseFor(sid, job && job.cwd), prefs = getPrefs(sid);
          if (typeof q.options === 'string') { try { q.options = JSON.parse(q.options); } catch { q.options = []; } }
          const options = (Array.isArray(q.options) ? q.options : []).slice(0, 6).map((o) => ({ label: String(o.label || '').slice(0, 120), description: String(o.description || '').slice(0, 240) })).filter((o) => o.label);
          const req2 = crypto.randomBytes(4).toString('hex');
          const answer = await new Promise((resolve) => {
            const t = setTimeout(() => { pending.delete(req2); resolve('(no answer in time)'); }, JOB_PERMISSION_WAIT_MS);
            const ev = { ...base, kind: 'question', call: prefs.call === 'attention', req: req2, question: String(q.question || '').slice(0, 400), options, multi: !!q.multiSelect, text: String(q.question || 'Claude has a question').slice(0, 200) };
            pending.set(req2, { event: ev, resolve: (a) => { clearTimeout(t); pending.delete(req2); resolve(a); } });
            publish(ev);
          });
          return json(res, 200, { answer });
        }
        if (parts[0] === 'hook' || parts[0] === 'call') {  // from scripts on this machine only, never through the proxy
          if (!isLocalDirect(req)) return json(res, 403, { error: 'local only' });
          const b = await bodyJson(req);
          if (parts[0] === 'hook') return json(res, 200, await handleHook(b));
          let sid = String(b.session || '');
          const cwd = String(b.cwd || '');
          if (!sid && cwd) { const m = core.sessions(new Set()).find((x) => x.cwd === cwd); sid = m ? m.id : ''; }
          publish({ ...baseFor(sid, cwd), kind: 'done', call: true, text: String(b.text || 'Claude is calling').slice(0, 200), seconds: 0 });
          return json(res, 200, { listeners: clients.size });
        }
        if (!authed(req, res)) return undefined;
        if (parts[0] === 'voice') return proxyVoice(req, res);
        if (parts[0] === 'agent' && parts[1] === 'files' && parts[2] === 'put') {   // a computer sends a file
          if (!hubFiles) return json(res, 404, { error: 'this is not the server' });
          let data; try { data = await readBody(req, 260e6); } catch { return json(res, 413, { error: 'too large' }); }
          let body; try { body = req.headers['x-gzip'] ? zlib.gunzipSync(data) : data; } catch { return json(res, 400, { error: 'bad body' }); }
          const q = u.searchParams, out = hubFiles.put(String(q.get('rel') || ''), body, Number(q.get('mtime')) || Date.now(), Number(q.get('mode')) || 0o644, q.get('keepOld') === '1', String(q.get('who') || ''), q.has('baseSize') ? [Number(q.get('baseSize')), Number(q.get('baseMtime'))] : q.has('baseNone') ? null : undefined);
          return json(res, out.error ? 400 : 200, out);
        }
        if (parts[0] === 'agent' && parts[1] === 'chats' && parts[2] === 'put') {   // a computer uploads a chat (or the new end of one)
          if (!hubSync) return json(res, 404, { error: 'this is not the server' });
          let data; try { data = await readBody(req, 400e6); } catch { return json(res, 413, { error: 'too large' }); }
          let body; try { body = req.headers['x-gzip'] ? zlib.gunzipSync(data) : data; } catch { return json(res, 400, { error: 'bad body' }); }
          const q = u.searchParams;
          const out = hubSync.receive({ machineId: String(q.get('machineId') || '').replace(/[^A-Za-z0-9]/g, '').slice(0, 16), name: String(q.get('name') || '').slice(0, 60), id: String(q.get('id') || ''),
            rel: q.has('rel') ? String(q.get('rel')) : null, mtime: Number(q.get('mtime')) || Date.now(), offset: Number(q.get('offset')) || 0, sha: String(q.get('sha') || '') }, body);
          return json(res, out.error ? 400 : 200, out);
        }
        if (parts[0] === 'projects' && parts[1] === 'import') {  // chats of a project that was moved here from another computer
          const cwd = String(u.searchParams.get('cwd') || ''), from = String(u.searchParams.get('from') || '');
          if (!cwd || !from || (root && !under(cwd, root))) return json(res, 400, { error: 'that folder is not inside the projects folder' });
          let data;
          try { data = await readBody(req, 600e6); } catch { return json(res, 413, { error: 'too large' }); }
          const tmp = path.join(dir, 'import-' + crypto.randomBytes(4).toString('hex') + '.tgz');
          fs.mkdirSync(dir, { recursive: true });
          fs.writeFileSync(tmp, data);
          try {
            const list = spawnSync('tar', ['-tzf', tmp], { encoding: 'utf8', maxBuffer: 64e6 });
            if (list.status !== 0 || list.stdout.split('\n').some((e) => e.startsWith('/') || e.split('/').includes('..'))) return json(res, 400, { error: 'invalid archive' });
            const dest = claudeDir(cwd);
            fs.mkdirSync(dest, { recursive: true });
            if (spawnSync('tar', ['-xzf', tmp, '-C', dest, '--no-same-owner'], { stdio: 'ignore' }).status !== 0) return json(res, 500, { error: 'could not unpack' });
            rewritePaths(dest, from, cwd);
            return json(res, 200, { ok: true, sessions: fs.readdirSync(dest).filter((f) => f.endsWith('.jsonl')).length });
          } finally { try { fs.unlinkSync(tmp); } catch { /* gone */ } }
        }
        if (parts[0] === 'upload') {  // a photo or file from the phone/desktop, saved inside the project so Claude can read it
          let cwd = u.searchParams.get('cwd') || '';
          const sid = u.searchParams.get('session');
          if (sid) { const f = core.findSession(sid); cwd = f ? core.summarize(f).cwd : ''; }
          try { if (!fs.statSync(cwd).isDirectory()) throw new Error(); } catch { return json(res, 400, { error: 'unknown project folder' }); }
          if (!inRoot(cwd)) return json(res, 403, { error: 'outside the projects folder' });
          const name = path.basename(String(u.searchParams.get('name') || 'file')).replace(/[^A-Za-z0-9._-]+/g, '_').slice(-80) || 'file';
          let data;
          try { data = await readBody(req, 25e6); } catch { return json(res, 413, { error: 'file too large (25 MB max)' }); }
          const dir = path.join(cwd, '.uploads');
          fs.mkdirSync(dir, { recursive: true });
          const file = path.join(dir, `${Date.now()}-${name}`);
          fs.writeFileSync(file, data);
          return json(res, 200, { path: file });
        }
        const b = await bodyJson(req);
        if (parts[0] === 'usage' && parts[1] === 'refresh') return json(res, 200, await checkUsage());
        if (parts[0] === 'projects' && parts[1] === 'move') {  // put a project into the projects folder, chats included
          if (!moveRoot) return json(res, 400, { error: 'this computer has no projects folder set up' });
          const src = path.resolve(String(b.cwd || '')), name = String(b.name || path.basename(src)).trim();
          if (!/^[A-Za-z0-9][A-Za-z0-9._ -]{0,63}$/.test(name) || name.includes('..')) return json(res, 400, { error: 'use letters, numbers, spaces, dots, dashes and underscores' });
          try { if (!fs.statSync(src).isDirectory()) throw new Error(); } catch { return json(res, 400, { error: "that folder doesn't exist" }); }
          if (under(src, moveRoot)) return json(res, 200, { path: src, moved: false });
          const dest = path.join(moveRoot, name);
          if (fs.existsSync(dest)) return json(res, 409, { error: `there is already a project called ${name} in the projects folder` });
          fs.mkdirSync(moveRoot, { recursive: true });
          try { fs.renameSync(src, dest); } catch { fs.cpSync(src, dest, { recursive: true }); fs.rmSync(src, { recursive: true, force: true }); }
          const oldData = claudeDir(src), newData = claudeDir(dest);
          if (fs.existsSync(oldData)) {
            fs.cpSync(oldData, newData, { recursive: true });
            rewritePaths(newData, src, dest);
            const recent = (function walk(d) { return fs.readdirSync(d, { withFileTypes: true }).some((e) => e.isDirectory() ? walk(path.join(d, e.name)) : Date.now() - fs.statSync(path.join(d, e.name)).mtimeMs < 120000); })(oldData);
            if (!recent) fs.rmSync(oldData, { recursive: true, force: true });  // a chat still running keeps its old copy
          }
          return json(res, 200, { path: dest, moved: true });
        }
        if (parts[0] === 'away') { writeCfg({ away: !!b.on }); return json(res, 200, { ok: true }); }
        if ((parts[0] === 'answer' || parts[0] === 'decision') && !pending.has(b.req) && remotePending.has(b.req)) {
          const r = remotePending.get(b.req); remotePending.delete(b.req);
          const list = answersFor.get(r.name) || []; list.push({ req: b.req, value: parts[0] === 'answer' ? String(b.answer || '').slice(0, 2000) : b.behavior }); answersFor.set(r.name, list);
          publish({ kind: 'resolved', req: b.req, session: r.session }); wake(); return json(res, 200, { ok: true });
        }
        if (parts[0] === 'answer') { const p = pending.get(b.req); if (p) { p.resolve(String(b.answer || '').slice(0, 2000)); publish({ kind: 'resolved', req: b.req, session: p.event.session }); } return json(res, 200, { ok: !!p }); }
        if (parts[0] === 'decision') { const p = pending.get(b.req); if (p) { p.resolve(b.behavior); publish({ kind: 'resolved', req: b.req, session: p.event.session }); } return json(res, 200, { ok: !!p }); }
        if (parts[0] === 'stop') { const j = jobs.get(String(b.session || '')); if (j && !j.done) j.child.kill(); return json(res, 200, { ok: !!j }); }
        if (parts[0] === 'projects') {
          if (!root) return json(res, 400, { error: 'this computer does not manage a projects folder' });
          const name = String(b.name || '').trim();
          if (!/^[A-Za-z0-9][A-Za-z0-9._ -]{0,63}$/.test(name) || name.includes('..')) return json(res, 400, { error: 'use letters, numbers, spaces, dots, dashes and underscores' });
          const p = path.join(root, name);
          fs.mkdirSync(p, { recursive: true });
          return json(res, 200, { path: p, name });
        }
        if (parts[0] === 'pair' && parts[1] === 'new') {   // your desktop app asks for a fresh phone-pairing code (needs the token)
          if (!publicMode) return json(res, 400, { error: 'only a server pairs phones this way' });
          hubPair.code = String(crypto.randomInt(0, 1000000)).padStart(6, '0'); hubPair.expires = Date.now() + 10 * 60000; hubPair.fails = 0;
          return json(res, 200, { code: hubPair.code, expiresIn: 600 });
        }
        if (parts[0] === 'pair' && parts[1] === 'decision') {   // the desktop app answers a request
          const r = pairReqs.get(String(b.id || ''));
          if (!r || r.status !== 'pending') return json(res, 404, { error: 'that request is gone' });
          r.status = b.accept ? 'accepted' : 'declined'; r.at = Date.now();
          if (!b.accept) askedAt.set(r.ip, Date.now() + 57000);   // a declined phone waits a minute before it can ask again
          return json(res, 200, { ok: true });
        }
        if (parts[0] === 'agent' && parts[1] === 'usage') {   // plan limits seen by a linked laptop
          const us = b.usage || {};
          if (us.at && (!usage.at || us.at > usage.at)) { usage = { ...us }; try { fs.writeFileSync(usageFile, JSON.stringify(usage), { mode: 0o600 }); } catch { /* best effort */ } }
          return json(res, 200, { ok: true });
        }
        if (parts[0] === 'agent' && parts[1] === 'chats' && parts[2] === 'sync') {   // a computer sends its list of chats; the answer says what to send and what to fetch
          if (!hubSync) return json(res, 404, { error: 'this is not the server' });
          const name = String(b.name || 'computer').slice(0, 60), machineId = String(b.machineId || '').replace(/[^A-Za-z0-9]/g, '').slice(0, 16);
          if (!machineId || !Array.isArray(b.manifest)) return json(res, 400, { error: 'bad manifest' });
          const ok = (e) => e && /^[0-9a-fA-F-]{36}$/.test(String(e.id)) && Number.isFinite(e.mtime) && (e.rel === null || typeof e.rel === 'string');
          agents.set(name, { seen: Date.now() });
          hubSync.note(name, { machineId, running: (Array.isArray(b.running) ? b.running : []).filter((x) => /^[0-9a-fA-F-]{36}$/.test(String(x))), runningAt: Date.now() });
          return json(res, 200, hubSync.plan({ machineId, name }, b.manifest.filter(ok).slice(0, 5000)));
        }
        if (parts[0] === 'agent' && parts[1] === 'files' && parts[2] === 'delete') {
          if (!hubFiles) return json(res, 404, { error: 'this is not the server' });
          return json(res, 200, hubFiles.remove(String(b.rel || '')));
        }
        if (parts[0] === 'admin' && parts[1] === 'update') {   // replace this server's program with the newest release, then restart (the service manager starts it again)
          if (!opts.selfUpdate) return json(res, 400, { error: 'This one updates together with its app.' });
          if ([...jobs.values()].some((j) => !j.done)) return json(res, 409, { error: 'Claude is working on this server right now. Try again when it has finished.' });
          try {
            const l = await selfupdate.latest();
            if (!selfupdate.newer(l.version, VERSION)) return json(res, 200, { ok: true, from: VERSION, to: VERSION, restarting: false });
            const to = await selfupdate.apply(l.tag, opts.relayDir || __dirname);
            json(res, 200, { ok: true, from: VERSION, to, restarting: true });
            setTimeout(() => process.exit(0), 800);
          } catch (e) { return json(res, 500, { error: e.message }); }
          return undefined;
        }
        if (parts[0] === 'sync' && parts[1] === 'projects' && parts[2] === 'accept') {   // "Install": this computer now keeps these project folders too
          if (!fileAgent) return json(res, 400, { error: 'this computer is not linked to a server' });
          await fileAgent.accept(b.all ? 'all' : (Array.isArray(b.names) ? b.names.map(String) : []));
          return json(res, 200, await syncOverview());
        }
        if (parts[0] === 'agent' && parts[1] === 'chats' && parts[2] === 'report') {
          if (hubSync) hubSync.note(String(b.name || 'computer').slice(0, 60), { last: { ...(b.stat || {}), received: Date.now() } });
          return json(res, 200, { ok: true });
        }
        if (parts[0] === 'sync' && parts[1] === 'now') {   // "Sync now": on a computer, sync with the server; on the server, ask the linked computers to
          const t0 = Date.now();
          if (hubSync) { bumpChats(); for (let i = 0; i < 40; i++) { await sleep(300); const cs = [...hubSync.agents.values()]; if (!cs.length || cs.every((a) => (a.last && a.last.received >= t0) || Date.now() - (a.seen || 0) > 45000)) break; } }
          else if (agentSync) { await agentSync.syncOnce('manual'); if (fileAgent) await fileAgent.syncOnce('manual'); }
          return json(res, 200, await syncOverview());
        }
        if (parts[0] === 'agent' && parts[1] === 'poll') {   // a linked laptop asks: any work for me? any answers?
          const name = String(b.name || 'laptop').slice(0, 60);
          agents.set(name, { seen: Date.now() });
          const r = readyFor(name);
          if (r.work.length || r.answers.length || (b.epoch !== undefined && b.epoch !== chatEpoch)) { answersFor.set(name, []); saveWork(); return json(res, 200, { ...r, epoch: chatEpoch }); }
          const w = { name, epoch: b.epoch, res, timer: setTimeout(() => { waiters.delete(w); agents.set(name, { seen: Date.now() }); json(res, 200, { work: [], answers: [], epoch: chatEpoch }); }, 20000) };
          waiters.add(w); res.on('close', () => { clearTimeout(w.timer); waiters.delete(w); });
          return undefined;
        }
        if (parts[0] === 'agent' && parts[1] === 'event') {  // an event from a linked laptop: show it to the phone
          const name = String(b.name || 'laptop').slice(0, 60), ev = { ...(b.event || {}) };
          if (!ev.kind) return json(res, 400, { error: 'no event' });
          delete ev.id; delete ev.boot; ev.remote = true; ev.machine = ev.machine || name;
          agents.set(name, { seen: Date.now() });
          if ((ev.kind === 'permission' || ev.kind === 'question') && ev.req) remotePending.set(ev.req, { name, session: ev.session });
          if (ev.kind === 'resolved') remotePending.delete(ev.req);
          if (ev.kind === 'done' && ev.session && getPrefs(ev.session).call === 'done') setPrefs(ev.session, { call: 'off' });
          publish(ev); return json(res, 200, { ok: true });
        }
        if (parts[0] === 'agent' && parts[1] === 'ack') {
          work = work.filter((w) => w.id !== b.id); saveWork();
          if (b.ok === false) publish({ kind: 'error', call: false, session: String(b.session || ''), title: 'Your laptop', cwd: '', machine: String(b.name || 'laptop'), ts: Date.now(), text: String(b.error || 'The task failed on your laptop.').slice(0, 200), remote: true });
          return json(res, 200, { ok: true });
        }
        if (parts[0] === 'sessions' && parts[2] === 'prefs') {
          const patch = {};
          if (['off', 'done', 'attention'].includes(b.call)) patch.call = b.call;
          if (['auto', 'laptop', 'vps'].includes(b.run)) patch.run = b.run;
          return json(res, 200, setPrefs(parts[1], patch));
        }
        if (parts[0] === 'sessions' && parts[2] === 'send') {
          const sidr = parts[1];
          const away = sidr !== 'new' && hubSync ? hubSync.findElsewhere(sidr) : null;
          if (away) {   // this chat's files only exist on one of your computers: hand the message to it
            work.push({ id: crypto.randomBytes(6).toString('hex'), session: sidr, cwd: away.cwd, target: away.machine, text: String(b.text || '').slice(0, 20000), mode: MODES.includes(b.mode) ? b.mode : 'auto', model: b.model || '', effort: b.effort || '', call: getPrefs(sidr).call, kind: getPrefs(sidr).kind, created: Date.now() });
            saveWork(); wake();
            res.writeHead(200, { 'Content-Type': 'application/x-ndjson' });
            res.end(JSON.stringify({ t: 'queued', to: away.machine, online: isOnline(away.machine) }) + '\n' + JSON.stringify({ t: 'end' }) + '\n');
            return undefined;
          }
          const prefs0 = sidr === 'new' ? { run: 'vps' } : getPrefs(sidr);
          const run = ['auto', 'laptop', 'vps'].includes(b.run) ? b.run : prefs0.run;   // a per-message choice wins over the chat's setting
          if (publicMode && sidr !== 'new' && (run === 'laptop' || (run === 'auto' && laptopOnline()))) {
            const f = core.findSession(sidr);
            if (!f) return json(res, 404, { error: 'no such session' });
            const s0 = core.summarize(f);
            if (!visibleSession(s0)) return json(res, 403, { error: 'that chat is outside the projects folder' });
            const old = jobs.get(sidr);
            if (old && !old.done) return json(res, 409, { error: 'Claude is still working on this chat' });
            const relp = path.relative(root, s0.cwd);
            work.push({ id: crypto.randomBytes(6).toString('hex'), session: sidr, rel: relp, text: String(b.text || ''), mode: MODES.includes(b.mode) ? b.mode : 'auto', model: b.model || '', effort: b.effort || '', call: getPrefs(sidr).call, chatMtime: f.mtime, kind: getPrefs(sidr).kind, created: Date.now() });
            saveWork(); wake();
            res.writeHead(200, { 'Content-Type': 'application/x-ndjson' });
            res.end(JSON.stringify({ t: 'queued', to: laptopOnline() || 'your laptop', online: !!laptopOnline() }) + '\n' + JSON.stringify({ t: 'end' }) + '\n');
            return undefined;
          }
          const r = startJob(b, parts[1]);
          if (r.error) return json(res, r.error, { error: r.message });
          return attach(res, r.job, 0);
        }
      }
      return json(res, 404, { error: 'not found' });
    } catch (e) { onLog(String(e && e.message)); if (!res.headersSent) json(res, 500, { error: 'relay error' }); }
  });
  server.keepAliveTimeout = 65000;

  // ── laptop side: link to a server, report there, and do the work it hands over ──
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  function agentPost(p, body, ms) {
    return new Promise((resolve, reject) => {
      const u = new URL(link.url), mod = u.protocol === 'https:' ? https : http, data = Buffer.from(JSON.stringify({ name: os.hostname(), ...body }));
      const rq = mod.request({ host: u.hostname, port: u.port || (u.protocol === 'https:' ? 443 : 80), path: p, method: 'POST', timeout: ms, headers: { Authorization: 'Bearer ' + link.token, 'Content-Type': 'application/json', 'Content-Length': data.length } }, (rs) => {
        let b = ''; rs.on('data', (d) => { b += d; }); rs.on('end', () => { try { resolve(JSON.parse(b || '{}')); } catch { resolve({}); } });
      });
      rq.on('timeout', () => rq.destroy(new Error('timeout'))); rq.on('error', reject); rq.end(data);
    });
  }
  const doing = new Set();
  let lastEpoch = 0;
  async function runWork(w) {
    const direct = !!w.cwd;   // a chat that lives only on this computer: no waiting for files to arrive
    const local = direct ? w.cwd : path.join(moveRoot || os.homedir(), w.rel || '');
    const deadline = Date.now() + 4 * 60000;
    for (; !direct;) {   // wait until the project folder and the chat have reached this computer
      if (fs.existsSync(local)) { const f = core.findSession(w.session); if (f && f.mtime >= (w.chatMtime || 0) - 3000) break; }
      if (Date.now() > deadline) throw new Error('The project or chat has not synced to this computer yet.');
      if (agentSync) agentSync.kick('wait');
      await sleep(4000);
    }
    if (direct && !fs.existsSync(local)) throw new Error('That folder is not on this computer any more.');
    if (w.call && w.call !== 'off') setPrefs(w.session, { call: w.call });
    if (w.kind === 'talk') setPrefs(w.session, { kind: 'talk' });
    publish({ ...baseFor(w.session, local), kind: 'started', call: false, text: 'Your laptop started: ' + String(w.text).slice(0, 140) });
    const text = moveRoot ? String(w.text).split(chathub.MARK).join(moveRoot.replace(/\/$/, '')) : w.text;   // paths in the task point at this computer's projects folder
    const r = startJob({ text, mode: w.mode, model: w.model, effort: w.effort }, w.session);
    if (r.error) throw new Error(r.message);
    while (!r.job.done) await sleep(1500);
  }
  async function agentLoop(gen) {
    while (link && gen === linkGen) {
      try {
        const r = await agentPost('/agent/poll', { epoch: lastEpoch }, 30000);
        if (r.epoch !== undefined && r.epoch !== lastEpoch) { lastEpoch = r.epoch; if (agentSync) agentSync.kick('server-changed'); }
        for (const a of r.answers || []) { const p = pending.get(a.req); if (p) p.resolve(a.value); }
        for (const w of r.work || []) {
          if (doing.has(w.id)) continue;
          doing.add(w.id);
          runWork(w).then(() => agentPost('/agent/ack', { id: w.id, ok: true, session: w.session }, 15000), (e) => agentPost('/agent/ack', { id: w.id, ok: false, error: e.message, session: w.session }, 15000)).catch(() => {}).finally(() => doing.delete(w.id));
        }
      } catch { await sleep(5000); }
    }
  }
  /** Any request to the linked server, with a raw body and raw answer (chats travel gzip-compressed). */
  function agentRaw(method, p, headers, bodyBuf, ms) {
    return new Promise((resolve, reject) => {
      const u = new URL(link.url), mod = u.protocol === 'https:' ? https : http;
      const rq = mod.request({ host: u.hostname, port: u.port || (u.protocol === 'https:' ? 443 : 80), path: p, method, timeout: ms, headers: { Authorization: 'Bearer ' + link.token, ...headers, ...(bodyBuf ? { 'Content-Length': bodyBuf.length } : {}) } }, (rs) => {
        const c = []; rs.on('data', (d) => c.push(d)); rs.on('end', () => resolve({ status: rs.statusCode, headers: rs.headers, body: Buffer.concat(c) }));
      });
      rq.on('timeout', () => rq.destroy(new Error('timed out'))); rq.on('error', reject); rq.end(bodyBuf || undefined);
    });
  }
  const chatTransport = {
    json: (p, body, ms) => agentPost(p, body, ms),
    put: async (p, gzBody) => { const r = await agentRaw('POST', p, { 'Content-Type': 'application/octet-stream', 'X-Gzip': '1' }, gzBody, 120000); let j = {}; try { j = JSON.parse(r.body.toString() || '{}'); } catch { /* not json */ } if (r.status >= 400 && !j.error) j.error = 'server said ' + r.status; return j; },
    get: async (p) => { const r = await agentRaw('GET', p, {}, null, 120000); if (r.status === 404) return null; if (r.status !== 200) throw new Error('server said ' + r.status); return { body: r.headers['x-gzip'] ? zlib.gunzipSync(r.body) : r.body, delta: r.headers['x-delta'] === '1', mtime: Number(r.headers['x-mtime']) || Date.now() }; },
  };
  const fileTransport = {
    json: (p, body, ms) => agentPost(p, body, ms),
    put: async (p, gzBody) => { const r = await agentRaw('POST', p, { 'Content-Type': 'application/octet-stream', 'X-Gzip': '1' }, gzBody, 300000); let j = {}; try { j = JSON.parse(r.body.toString() || '{}'); } catch { /* not json */ } if (r.status >= 400 && !j.error) j.error = 'server said ' + r.status; return j; },
    get: async (p) => { const r = await agentRaw('GET', p, {}, null, 300000); if (r.status === 404) return null; if (r.status !== 200) throw new Error('server said ' + r.status); return { body: r.headers['x-gzip'] ? zlib.gunzipSync(r.body) : r.body, mtime: Number(r.headers['x-mtime']) || Date.now(), mode: Number(r.headers['x-mode']) || 0o644 }; },
  };
  function setLink(l) {
    link = l && l.url && l.token ? { url: l.url, token: l.token, name: String(l.name || '').slice(0, 60) } : null;
    linkGen++;
    if (agentSync) { agentSync.stop(); agentSync = null; }
    if (fileAgent) { fileAgent.stop(); fileAgent = null; pendingProjects = []; }
    if (link && !hubSync) {
      agentSync = chathub.createAgent({ root: root || moveRoot, claudeDir: () => core.projectsDir(), machineId, name: os.hostname(), isBusy,
        running: () => [...jobs.values()].filter((j) => !j.done && j.sid).map((j) => j.sid), transport: chatTransport, onLog });
      agentSync.start();
      const fr = root || moveRoot;
      if (fr && fileSyncOn()) { fileAgent = filehub.createAgent({ root: fr, stateFile: path.join(dir, 'filesync-state.json'), name: os.hostname(), transport: fileTransport, onLog, onPending: (p) => { pendingProjects = p; } }); fileAgent.start(); }
    }
    forwardUsage = link ? (u) => { agentPost('/agent/usage', { usage: u }, 10000).catch(() => {}); } : null;
    forward = link ? (e) => { if (['done', 'error', 'permission', 'question', 'resolved', 'started'].includes(e.kind)) agentPost('/agent/event', { event: e }, 10000).catch(() => {}); } : null;
    if (link) agentLoop(linkGen);
  }

  // ── chats: the server knows every chat; computers sync with it over the link ──
  const machineId = (() => { let id = readCfg().machineId; if (!id) { id = crypto.randomBytes(5).toString('hex'); writeCfg({ machineId: id }); } return id; })();
  const isBusy = (id) => { for (const j of jobs.values()) if (!j.done && (j.sid === id || j.key === id)) return true; return false; };
  const hubSync = publicMode && root ? chathub.createHub({ dir, root, claudeDir: () => core.projectsDir(), isBusy, onLog }) : null;
  let agentSync = null, fileAgent = null, pendingProjects = [];
  const hubFiles = publicMode && root ? filehub.createHub({ root, onLog }) : null;
  /** Files sync through Hearth itself unless Syncthing is set up for the projects (then it keeps doing that). */
  const fileSyncOn = () => !fs.existsSync(path.join(dir, 'no-filesync'));   // a file with that name turns it off (for people who use Syncthing instead)
  async function syncOverview() {
    const base = hubSync ? hubSync.status() : agentSync ? { ...agentSync.status(), linked: true } : { role: 'single', linked: !!link };
    return { ...base, files: hubFiles ? { count: hubFiles.count() } : fileAgent ? fileAgent.status() : null, pending: pendingProjects, build: BUILD, version: VERSION, now: Date.now(), syncthing: await syncStatus('claude-projects').catch(() => ({ available: false })) };
  }

  // ── discovery (desktop app only) ──
  let udp = null;
  if (!publicMode) {
    try {
      udp = dgram.createSocket({ type: 'udp4', reuseAddr: true });
      udp.on('message', (msg, r) => { if (msg.toString().startsWith('CLAUDE_METER_DISCOVER') && lanOnly(r.address)) udp.send(JSON.stringify({ name: os.hostname(), port }), r.port, r.address); });
      udp.on('error', () => { try { udp.close(); } catch { /* ignore */ } udp = null; });
      udp.bind(DISCOVER_PORT);
    } catch { udp = null; }
  }

  return new Promise((resolve) => {
    server.once('error', (e) => resolve({ ok: false, inUse: e.code === 'EADDRINUSE', error: e.message }));
    server.listen(port, host, () => resolve({
      ok: true, port, token, root,
      pairCode: () => pair.code, newPairCode: newCode,
      setAway: (on) => writeCfg({ away: !!on }), setLink,
      addresses: () => Object.values(os.networkInterfaces()).flat().filter((i) => i && i.family === 'IPv4' && !i.internal).map((i) => i.address),
      stop: () => { link = null; linkGen++; if (agentSync) agentSync.stop(); clearInterval(codeTimer); for (const c of clients) c.end(); server.close(); if (udp) { try { udp.close(); } catch { /* ignore */ } } for (const j of jobs.values()) if (!j.done) j.child.kill(); },
    }));
  });
}

module.exports = { start, lanOnly, findClaude };
