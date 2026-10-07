'use strict';
/* Chat sync between your computers and the server, over the app's own HTTPS link. No Syncthing involved.

   The server is the one place that knows every chat. A computer (the "agent") sends it a manifest of its chats; the
   server answers with what to push and what to pull, newest copy wins. A chat that only grows is sent as just its new tail.

   Paths: a chat's transcript contains the absolute path of its project. In transit that path is replaced by a placeholder
   (MARK), so every machine writes the chat with its own projects folder. Chats from folders outside the projects folder are
   kept on the server as they are, tagged with the computer they live on ("elsewhere"), so they show up everywhere and can be
   read from the phone; replying to one is handed back to that computer. */
const fs = require('fs');
const os = require('os');
const path = require('path');
const crypto = require('crypto');
const zlib = require('zlib');
const core = require('./core');

const MARK = '/__hearth_projects__';   // stands in for each computer's own projects folder while a chat travels
const SLACK = 1500;          // mtimes this close count as equal
const QUIET_MS = 20000;      // never overwrite a local chat that was written to this recently
const MAX_AGE_MS = 120 * 86400e3;
const encodeCwd = (p) => p.replace(/[^A-Za-z0-9]/g, '-');
const sha1 = (b) => crypto.createHash('sha1').update(b).digest('hex');
const normalize = (text, root) => (root && root !== MARK ? text.split(root).join(MARK) : text);
const denormalize = (text, root) => (root && root !== MARK ? text.split(MARK).join(root) : text);
const TMP = ['/tmp', '/var/tmp', os.tmpdir()];

const inside = (p, root) => !!root && (p === root || p.startsWith(root + path.sep));

/** A chat that is worth sharing, described for the manifest. null for junk (temp folders, deleted folders, empty chats). */
function describeChat(f, root, { needDir = true } = {}) {
  const s = core.summarize(f) || {};
  const cwd = s.cwd;
  if (!cwd) return null;
  if (TMP.some((t) => inside(cwd, t))) return null;
  if (needDir) { try { if (!fs.statSync(cwd).isDirectory()) return null; } catch { return null; } }
  if (s.title === 'Untitled' && f.size < 4000) return null;
  return { id: f.id, rel: inside(cwd, root) ? path.relative(root, cwd) : null, cwd, mtime: Math.floor(f.mtime), size: f.size, title: s.title, file: f.file };
}

/** Normalized bytes of a chat file, plus the numbers needed to send only what the other side lacks. */
function readNorm(file, root) {
  const buf = Buffer.from(normalize(fs.readFileSync(file, 'utf8'), root), 'utf8');
  return buf;
}
function writeChat(dst, text, mtimeMs) {
  const tmp = dst + '.tmp-' + process.pid;
  fs.mkdirSync(path.dirname(dst), { recursive: true });
  fs.writeFileSync(tmp, text);
  fs.renameSync(tmp, dst);
  const t = new Date(mtimeMs); fs.utimesSync(dst, t, t);   // same mtime on both sides, so it is never sent back
}
const gz = (b) => zlib.gzipSync(b, { level: 6 });

// ───────────────────────── server side ─────────────────────────
function createHub({ dir, root, claudeDir, isBusy = () => false, onLog = () => {} }) {
  const store = path.join(dir, 'elsewhere');
  const agents = new Map();     // computer name -> { at, last (its latest sync report), running: [ids], runningAt }
  let ewCache = { at: 0, list: [] };

  const ownMap = (opts) => { const m = new Map(); for (const f of core.listFiles()) { const e = describeChat(f, root, opts); if (e && e.rel !== null) m.set(e.id, e); } return m; };

  /** Chats other computers keep outside the projects folder. */
  function elsewhere(force) {
    if (!force && Date.now() - ewCache.at < 2000) return ewCache.list;
    const out = [];
    let ms = []; try { ms = fs.readdirSync(store, { withFileTypes: true }).filter((d) => d.isDirectory()); } catch { /* none yet */ }
    for (const m of ms) {
      let machine = m.name; try { machine = fs.readFileSync(path.join(store, m.name, '_name'), 'utf8').trim() || machine; } catch { /* unnamed */ }
      let names = []; try { names = fs.readdirSync(path.join(store, m.name)).filter((n) => n.endsWith('.jsonl')); } catch { continue; }
      for (const n of names) {
        const file = path.join(store, m.name, n);
        let st; try { st = fs.statSync(file); } catch { continue; }
        const f = { id: n.slice(0, -6), file, mtime: st.mtimeMs, size: st.size };
        const s = core.summarize(f) || {};
        out.push({ ...f, machineId: m.name, machine, cwd: s.cwd || '', title: s.title || 'Untitled' });
      }
    }
    ewCache = { at: Date.now(), list: out };
    return out;
  }
  const findElsewhere = (id) => elsewhere().find((e) => e.id === id) || null;

  /** Which chats should travel? Compares an agent's manifest with what the server has. */
  function plan(agent, manifest) {
    const mine = ownMap({ needDir: false }), push = [], pull = [], seen = new Set();
    const ew = new Map(elsewhere(true).filter((e) => e.machineId === agent.machineId).map((e) => [e.id, e]));
    for (const e of manifest) {
      seen.add(e.id);
      if (e.rel === null) {                                           // lives outside the projects folder: the computer owns it, the server keeps a copy
        const x = ew.get(e.id);
        if (!x || e.mtime > x.mtime + SLACK) push.push({ id: e.id, have: x ? { ...describe(x.file, null) } : null });
        continue;
      }
      if (isBusy(e.id)) continue;
      const h = mine.get(e.id);
      if (!h) push.push({ id: e.id, have: null });
      else if (e.mtime > h.mtime + SLACK) push.push({ id: e.id, have: describe(h.file, root) });
      else if (h.mtime > e.mtime + SLACK) pull.push({ id: e.id, rel: h.rel, mtime: h.mtime });
    }
    for (const [id, h] of mine) {   // chats only the server has (made from the phone, or on another computer)
      if (!seen.has(id) && !isBusy(id) && Date.now() - h.mtime < MAX_AGE_MS && (h.size > 0)) pull.push({ id, rel: h.rel, mtime: h.mtime });
    }
    return { push, pull };
  }
  const describe = (file, r) => { try { const b = readNorm(file, r); return { bytes: b.length, sha: sha1(b) }; } catch { return null; } };

  /** A chat arrives from a computer. */
  function receive(q, body) {
    const { machineId, name, id, rel, mtime, offset = 0, sha } = q;
    if (!/^[0-9a-fA-F-]{36}$/.test(id)) return { error: 'bad id' };
    const inRoot = rel !== null && rel !== undefined;
    const dest = inRoot ? path.join(claudeDir(), encodeCwd(path.join(root, rel)), id + '.jsonl') : path.join(store, String(machineId).replace(/[^A-Za-z0-9]/g, ''), id + '.jsonl');
    const r = inRoot ? root : null;
    if (inRoot && isBusy(id)) return { ok: true, skipped: 'busy' };
    let cur = null; try { cur = fs.statSync(dest); } catch { /* new here */ }
    if (cur && cur.mtimeMs > mtime + SLACK) return { ok: true, skipped: 'newer here' };
    let norm = body;
    if (offset > 0) {
      let have; try { have = readNorm(dest, r); } catch { return { resync: true }; }
      if (have.length !== offset || sha1(have) !== sha) return { resync: true };
      norm = Buffer.concat([have, body]);
    }
    if (!inRoot) { fs.mkdirSync(path.dirname(dest), { recursive: true }); try { fs.writeFileSync(path.join(path.dirname(dest), '_name'), String(name || machineId)); } catch { /* cosmetic */ } }
    writeChat(dest, inRoot ? denormalize(norm.toString('utf8'), root) : norm.toString('utf8'), mtime);
    ewCache.at = 0;
    return { ok: true };
  }

  /** A computer asks for a chat the server has newer. */
  function serve(id, offset, sha) {
    const f = core.findSession(id); if (!f) return null;
    const e = describeChat(f, root, { needDir: false }); if (!e || e.rel === null) return null;
    const norm = readNorm(f.file, root);
    const delta = offset > 0 && offset <= norm.length && sha1(norm.subarray(0, offset)) === sha;
    return { body: delta ? norm.subarray(offset) : norm, delta, mtime: f.mtime, rel: e.rel };
  }

  const note = (name, patch) => { const a = agents.get(name) || { at: 0 }; agents.set(name, { ...a, ...patch, seen: Date.now() }); };
  const remoteRunning = () => { const s = new Set(); for (const a of agents.values()) if (a.runningAt && Date.now() - a.runningAt < 90000) for (const i of a.running || []) s.add(i); return s; };
  const AGENT_FRESH_MS = 45000;
  const status = () => ({
    role: 'server', chats: ownMap({ needDir: false }).size,
    elsewhere: [...elsewhere(true).reduce((m, e) => m.set(e.machine, (m.get(e.machine) || 0) + 1), new Map())].map(([machine, count]) => ({ machine, count })),
    computers: [...agents].map(([name, a]) => ({ name, online: Date.now() - (a.seen || 0) < AGENT_FRESH_MS, lastSeen: a.seen || 0, sync: a.last || null })),
  });
  return { plan, receive, serve, elsewhere, findElsewhere, note, remoteRunning, status, agents, ownMap };
}

// ───────────────────────── computer side ─────────────────────────
function createAgent({ root, claudeDir, machineId, name, isBusy = () => false, running = () => [], transport, onLog = () => {}, intervalMs = 10000 }) {
  let timer = null, inflight = null, dirty = false, last = null, lastPlan = null;

  const entries = () => core.listFiles().map((f) => describeChat(f, root)).filter(Boolean);

  async function pushOne(e, have, retried) {
    if (Date.now() - e.mtime < 3000) return false;                    // still being written: next round
    const full = readNorm(e.file, root);
    const delta = !retried && have && have.bytes > 0 && have.bytes <= full.length && sha1(full.subarray(0, have.bytes)) === have.sha;
    const body = gz(delta ? full.subarray(have.bytes) : full);
    const q = new URLSearchParams({ id: e.id, machineId, name, mtime: String(e.mtime), offset: delta ? String(have.bytes) : '0' });
    if (delta) q.set('sha', have.sha);
    if (e.rel !== null) q.set('rel', e.rel);
    const r = await transport.put('/agent/chats/put?' + q, body);
    if (r.resync && !retried) return pushOne(e, null, true);
    if (r.error) throw new Error(r.error);
    return true;
  }

  async function pullOne(p, byId) {
    const dir = path.join(root, p.rel), dst = path.join(claudeDir(), encodeCwd(dir), p.id + '.jsonl');
    let st = null; try { st = fs.statSync(dst); } catch { /* not here yet */ }
    if (st && st.mtimeMs >= p.mtime - SLACK) return true;                                   // already have it (even if it is too small to be listed)
    if (!st && p.rel && !fs.existsSync(dir)) return true;                                    // its project folder hasn't arrived here yet; later
    const known = byId.get(p.id);
    if (st && (isBusy(p.id) || Date.now() - st.mtimeMs < QUIET_MS || (known && st.mtimeMs > known.mtime + SLACK))) return false;
    let off = 0, sha = '';
    if (st) { const b = readNorm(dst, root); off = b.length; sha = sha1(b); }
    const r = await transport.get('/agent/chats/get?' + new URLSearchParams({ id: p.id, offset: String(off), sha }));
    if (!r) return false;
    const base = r.delta ? readNorm(dst, root) : Buffer.alloc(0);
    const text = Buffer.concat([base, r.body]).toString('utf8');
    if (st) { let again; try { again = fs.statSync(dst).mtimeMs; } catch { again = 0; } if (again > st.mtimeMs + SLACK) return false; }   // changed while we were downloading
    writeChat(dst, denormalize(text, root), r.mtime);
    return true;
  }

  async function syncOnce(reason) {
    if (inflight) { dirty = true; return inflight; }
    inflight = (async () => {
      const t0 = Date.now(), stat = { at: t0, reason, pushed: 0, pulled: 0, errors: [], waiting: 0 };
      try {
        const list = entries(), byId = new Map(list.map((e) => [e.id, e]));
        const plan = await transport.json('/agent/chats/sync', { machineId, name, running: running(), manifest: list.map(({ file, ...m }) => m) }, 40000);
        lastPlan = plan;
        for (const x of plan.push || []) {
          const e = byId.get(x.id); if (!e) continue;
          try { if (await pushOne(e, x.have)) stat.pushed++; else stat.waiting++; } catch (err) { stat.errors.push(`send ${e.title.slice(0, 30)}: ${err.message}`); }
        }
        for (const p of plan.pull || []) {
          try { if (await pullOne(p, byId)) stat.pulled++; else stat.waiting++; } catch (err) { stat.errors.push(`receive ${p.id.slice(0, 8)}: ${err.message}`); }
        }
        stat.localChats = list.length;
      } catch (err) { stat.errors.push(err.message); }
      stat.ok = stat.errors.length === 0; stat.ms = Date.now() - t0;
      last = stat;
      try { await transport.json('/agent/chats/report', { name, machineId, stat }, 10000); } catch { /* the next sync reports again */ }
      return stat;
    })().finally(() => { inflight = null; if (dirty) { dirty = false; setTimeout(() => syncOnce('again'), 500); } });
    return inflight;
  }

  const start = () => { stop(); timer = setInterval(() => syncOnce('timer').catch(() => {}), intervalMs); syncOnce('start').catch(() => {}); };
  const stop = () => { if (timer) clearInterval(timer); timer = null; };
  return { start, stop, kick: (why) => syncOnce(why || 'kick').catch(() => {}), syncOnce, status: () => ({ role: 'computer', last, localChats: last ? last.localChats : null, waiting: lastPlan ? (lastPlan.push || []).length + (lastPlan.pull || []).length : 0 }) };
}

module.exports = { MARK, createHub, createAgent, normalize, denormalize, encodeCwd, describeChat, gz };
