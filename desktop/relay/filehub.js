'use strict';
/* File sync for the projects folder, over the same connection the apps already use (no extra program to install).
   The server keeps the shared copy. Each computer remembers what it last synced, so it can tell "changed here",
   "changed there", "deleted here" and "deleted there" apart. Rules, all aimed at never losing work:
   - changed on one side only: that side wins
   - changed on both: the newest wins, the other version is kept next to it as "name.conflict-<computer>-<time>"
   - deleted: the file moves into .hearth-trash (on every computer) instead of vanishing, and a safety brake stops
     a sync that would delete a lot of files at once (a missing drive must never wipe the other side)
   - a project folder that appears on the server while your computer was off waits for your OK ("pending projects") */
const fs = require('fs');
const path = require('path');
const zlib = require('zlib');

const SLACK = 2500;                       // clocks and file systems differ by a little
const MAX_FILE = 200 * 1024 * 1024;
const SKIP_DIRS = new Set(['node_modules', '.venv', 'venv', '__pycache__', '.gradle', 'build', 'dist', '.next', 'target', '.cache', '.hearth-trash', '.idea', '.vs', '.mypy_cache', '.pytest_cache', '.stfolder', '.stversions']);
const SKIP_FILES = /(^\.DS_Store$|^Thumbs\.db$|\.pyc$|\.swp$|~$|^\.hearth-tmp-|^\.syncthing\.|\.sync-conflict-|^index\.lock$|\.lock$)/;
const TOP_SKIP = new Set(['.hearth-trash', '.stfolder', '.stversions', '.claude-chats', '.claude-sync']);

/** Every syncable file below root as { 'rel/path': [size, mtimeMs, mode] }. Symlinks and odd files are left alone. */
function scan(root) {
  const out = {};
  const walk = (dir, rel, top) => {
    let ents; try { ents = fs.readdirSync(dir, { withFileTypes: true }); } catch { return; }
    for (const e of ents) {
      if (top && TOP_SKIP.has(e.name)) continue;
      const r = rel ? rel + '/' + e.name : e.name, p = path.join(dir, e.name);
      if (e.isDirectory()) { if (!SKIP_DIRS.has(e.name)) walk(p, r, false); }
      else if (e.isFile() && !SKIP_FILES.test(e.name)) {
        try { const s = fs.statSync(p); if (s.size <= MAX_FILE) out[r] = [s.size, Math.floor(s.mtimeMs), s.mode & 0o777]; } catch { /* vanished */ }
      }
    }
  };
  walk(root, '', true);
  return out;
}

const safeRel = (rel) => typeof rel === 'string' && rel && !rel.includes('\0') && !path.isAbsolute(rel) && !rel.split('/').some((p) => p === '..' || p === '' || p === '.') && !rel.includes('\\');
const abs = (root, rel) => path.join(root, ...rel.split('/'));
const stamp = () => new Date().toISOString().replace(/[-:]/g, '').replace(/\..*/, '').replace('T', '-');

function writeAtomic(file, buf, mtime, mode) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  const tmp = path.join(path.dirname(file), '.hearth-tmp-' + process.pid + '-' + Math.random().toString(36).slice(2, 8));
  fs.writeFileSync(tmp, buf);
  if (mode) { try { fs.chmodSync(tmp, mode & 0o777); } catch { /* not supported here */ } }
  const t = new Date(mtime); fs.utimesSync(tmp, t, t);
  fs.renameSync(tmp, file);
}
function toTrash(root, rel) {
  const from = abs(root, rel); if (!fs.existsSync(from)) return;
  const to = path.join(root, '.hearth-trash', stamp(), ...rel.split('/'));
  fs.mkdirSync(path.dirname(to), { recursive: true });
  try { fs.renameSync(from, to); } catch { fs.copyFileSync(from, to); fs.unlinkSync(from); }
}
function conflictName(rel, who) {
  const i = rel.lastIndexOf('.'), s = rel.lastIndexOf('/');
  const tag = `.conflict-${String(who || 'other').replace(/[^A-Za-z0-9]/g, '').slice(0, 12) || 'other'}-${stamp()}`;
  return i > s + 1 ? rel.slice(0, i) + tag + rel.slice(i) : rel + tag;
}

// ───────────────────────── server side ─────────────────────────
function createHub({ root, onLog = () => {} }) {
  let cache = null, cacheAt = 0;
  const manifest = () => { if (!cache || Date.now() - cacheAt > 1000) { cache = scan(root); cacheAt = Date.now(); } return cache; };
  const touch = () => { cache = null; };
  /** base: what the sending computer believes the server's copy looked like ([size, mtime] or null when it thinks there is none). If the real file is different, somebody changed it in between: that copy is kept as a conflict file, never overwritten silently. */
  function put(rel, buf, mtime, mode, keepOld, who, base) {
    if (!safeRel(rel)) return { error: 'bad path' };
    const f = abs(root, rel);
    let changedMeanwhile = false;
    try { const st = fs.statSync(f); changedMeanwhile = base === undefined ? false : (!base || st.size !== base[0] || Math.abs(Math.floor(st.mtimeMs) - base[1]) > SLACK) && !(st.size === buf.length && Math.abs(Math.floor(st.mtimeMs) - mtime) <= SLACK); } catch { /* nothing there yet */ }
    if ((keepOld || changedMeanwhile) && fs.existsSync(f)) { try { fs.copyFileSync(f, abs(root, conflictName(rel, who))); } catch { /* best effort */ } }
    writeAtomic(f, buf, mtime, mode); touch();
    return { ok: true };
  }
  function get(rel) {
    if (!safeRel(rel)) return null;
    const f = abs(root, rel); try { const s = fs.statSync(f); if (!s.isFile()) return null; return { body: fs.readFileSync(f), mtime: Math.floor(s.mtimeMs), mode: s.mode & 0o777 }; } catch { return null; }
  }
  function remove(rel) { if (!safeRel(rel)) return { error: 'bad path' }; toTrash(root, rel); touch(); return { ok: true }; }
  return { manifest, put, get, remove, count: () => Object.keys(manifest()).length };
}

// ───────────────────────── computer side ─────────────────────────
function createAgent({ root, stateFile, name, transport, onLog = () => {}, intervalMs = 12000, onPending = () => {} }) {
  let timer = null, inflight = null, dirty = false, last = null, pending = [], paused = null;
  let state = { files: {}, accepted: [], rejected: [] };
  try { state = { files: {}, accepted: [], rejected: [], ...JSON.parse(fs.readFileSync(stateFile, 'utf8')) }; } catch { /* first run */ }
  const save = () => { try { fs.mkdirSync(path.dirname(stateFile), { recursive: true }); fs.writeFileSync(stateFile + '.tmp', JSON.stringify(state)); fs.renameSync(stateFile + '.tmp', stateFile); } catch { /* next time */ } };
  const differs = (a, b) => !a || !b || a[0] !== b[0] || Math.abs(a[1] - b[1]) > SLACK;
  const topOf = (rel) => (rel.includes('/') ? rel.split('/')[0] : '');

  async function syncOnce(reason) {
    if (inflight) { dirty = true; return inflight; }
    inflight = (async () => {
      const t0 = Date.now(), stat = { at: t0, reason, pushed: 0, pulled: 0, deleted: 0, conflicts: 0, errors: [], waiting: 0 };
      try {
        if (!fs.existsSync(root)) throw new Error('The projects folder is missing: ' + root);
        const mr = await transport.get('/agent/files/manifest');
        if (!mr) throw new Error('The server does not do file sync yet. Update the server.');
        const hub = JSON.parse(mr.body.toString()).files || {};
        const local = scan(root), base = state.files;
        const plan = { push: [], pull: [], delLocal: [], delHub: [], conflict: [] };
        const pend = new Map();
        const rels = new Set([...Object.keys(local), ...Object.keys(hub), ...Object.keys(base)]);
        for (const rel of rels) {
          const L = local[rel], H = hub[rel], B = base[rel];
          if (L && H && !differs(L, H)) { state.files[rel] = { l: L, h: H }; continue; }
          const lc = L && (!B || differs(L, B.l)), hc = H && (!B || differs(H, B.h));
          if (L && H) {
            if (lc && !hc) plan.push.push(rel);
            else if (hc && !lc) plan.pull.push(rel);
            else if (L[1] >= H[1]) { plan.push.push(rel); plan.conflict.push([rel, 'hub']); }
            else { plan.pull.push(rel); plan.conflict.push([rel, 'local']); }
          } else if (L) {
            if (B && !lc) plan.delLocal.push(rel); else plan.push.push(rel);
          } else if (H) {
            if (B && !hc) plan.delHub.push(rel);
            else {
              const top = topOf(rel);
              if (top && state.rejected.includes(top) && !fs.existsSync(path.join(root, top))) continue;   // "No thanks": never offered again, never copied
              if (top && !B && !state.accepted.includes(top) && !fs.existsSync(path.join(root, top))) { const p = pend.get(top) || { name: top, files: 0, bytes: 0, newest: 0 }; p.files++; p.bytes += H[0]; p.newest = Math.max(p.newest, H[1]); pend.set(top, p); }
              else plan.pull.push(rel);
            }
          } else delete state.files[rel];
        }
        pending = [...pend.values()].sort((a, b) => b.newest - a.newest);
        onPending(pending);
        // safety brake: a sync that would delete a lot is never right (an unplugged drive, a wrong folder, a moved project)
        const known = Object.keys(base).length, dels = plan.delLocal.length + plan.delHub.length;
        paused = null;
        if (dels > Math.max(15, known * 0.25)) { paused = `${dels} files look deleted. Nothing was deleted. If that is right, delete them again later in smaller steps.`; plan.delLocal = []; plan.delHub = []; }
        for (const rel of plan.push) {
          try {
            const L = local[rel]; if (Date.now() - L[1] < 2000) { stat.waiting++; continue; }
            const buf = fs.readFileSync(abs(root, rel)), s2 = fs.statSync(abs(root, rel));
            if (s2.size !== L[0] || Math.floor(s2.mtimeMs) !== L[1]) { stat.waiting++; continue; }   // changed while we read it
            const keep = plan.conflict.some((c) => c[0] === rel && c[1] === 'hub');
            const bh = base[rel] && base[rel].h;   // what we believe the server has: it checks that against the real file
            const r = await transport.put('/agent/files/put?' + new URLSearchParams({ rel, mtime: String(L[1]), mode: String(L[2]), keepOld: keep ? '1' : '0', who: name || '', ...(bh ? { baseSize: String(bh[0]), baseMtime: String(bh[1]) } : { baseNone: '1' }) }), zlib.gzipSync(buf, { level: 3 }));
            if (r.error) throw new Error(r.error);
            state.files[rel] = { l: L, h: [L[0], L[1]] }; stat.pushed++; if (keep) stat.conflicts++;
          } catch (e) { stat.errors.push(`send ${rel}: ${e.message}`); }
        }
        for (const rel of plan.pull) {
          try {
            const cur = local[rel], keep = plan.conflict.some((c) => c[0] === rel && c[1] === 'local');
            const r = await transport.get('/agent/files/get?' + new URLSearchParams({ rel }));
            if (!r) { stat.waiting++; continue; }
            if (cur) { const now = fs.statSync(abs(root, rel)); if (now.size !== cur[0] || Math.floor(now.mtimeMs) !== cur[1]) { stat.waiting++; continue; } }   // edited while we downloaded
            if (keep && cur) { try { fs.copyFileSync(abs(root, rel), abs(root, conflictName(rel, name))); } catch { /* best effort */ } stat.conflicts++; }
            writeAtomic(abs(root, rel), r.body, r.mtime, r.mode);
            const s = fs.statSync(abs(root, rel)); state.files[rel] = { l: [s.size, Math.floor(s.mtimeMs), s.mode & 0o777], h: [s.size, r.mtime] }; stat.pulled++;
          } catch (e) { stat.errors.push(`receive ${rel}: ${e.message}`); }
        }
        for (const rel of plan.delLocal) { try { toTrash(root, rel); delete state.files[rel]; stat.deleted++; } catch (e) { stat.errors.push(`remove ${rel}: ${e.message}`); } }
        for (const rel of plan.delHub) { try { const r = await transport.json('/agent/files/delete', { rel }, 20000); if (r.error) throw new Error(r.error); delete state.files[rel]; stat.deleted++; } catch (e) { stat.errors.push(`remove on server ${rel}: ${e.message}`); } }
        save();
        stat.files = Object.keys(local).length;
      } catch (e) { stat.errors.push(e.message); }
      stat.ok = stat.errors.length === 0; stat.ms = Date.now() - t0; stat.paused = paused;
      last = stat;
      if (stat.pushed || stat.pulled || stat.deleted || stat.conflicts || stat.errors.length) onLog(`files: sent ${stat.pushed}, received ${stat.pulled}, removed ${stat.deleted}, conflicts ${stat.conflicts}${stat.errors.length ? ', errors: ' + stat.errors.slice(0, 2).join('; ') : ''}`);
      return stat;
    })().finally(() => { inflight = null; if (dirty) { dirty = false; setTimeout(() => syncOnce('again').catch(() => {}), 500); } });
    return inflight;
  }

  /** The person said Install: from now on this project folder syncs like all the others. */
  function accept(names) {
    const all = names === 'all';
    for (const p of pending) if (all || (names || []).includes(p.name)) { if (!state.accepted.includes(p.name)) state.accepted.push(p.name); }
    save(); pending = pending.filter((p) => !(all || (names || []).includes(p.name)));
    return syncOnce('accepted');
  }
  /** The person said "No thanks" to a project that appeared on the server: it is never offered again (until they allow it in Settings). */
  function reject(names) {
    const all = names === 'all';
    for (const p of pending) if (all || (names || []).includes(p.name)) { if (!state.rejected.includes(p.name)) state.rejected.push(p.name); }
    save(); pending = pending.filter((p) => !(all || (names || []).includes(p.name)));
  }
  function allow(names) { state.rejected = state.rejected.filter((n) => !(names || []).includes(n)); save(); return syncOnce('allowed'); }
  const start = () => { stop(); timer = setInterval(() => syncOnce('timer').catch(() => {}), intervalMs); syncOnce('start').catch(() => {}); };
  const stop = () => { if (timer) clearInterval(timer); timer = null; };
  return { start, stop, syncOnce, accept, reject, allow, kick: (why) => syncOnce(why || 'kick').catch(() => {}), status: () => ({ last, pending, paused, rejected: state.rejected, tracked: Object.keys(state.files).length }) };
}

module.exports = { scan, createHub, createAgent, safeRel, SLACK };
