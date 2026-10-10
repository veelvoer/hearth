'use strict';
/* Reads Claude Code's session logs: chats, messages, tool calls and usage statistics. No network here. */
const fs = require('fs');
const path = require('path');
const os = require('os');

const projectsDir = () => path.join(os.homedir(), '.claude', 'projects');

function readJsonl(file) {
  try {
    return fs.readFileSync(file, 'utf8').split('\n').filter(Boolean).map((l) => { try { return JSON.parse(l); } catch { return null; } }).filter(Boolean);
  } catch { return []; }
}
const textOf = (c) => typeof c === 'string' ? c : Array.isArray(c) ? c.filter((b) => b && b.type === 'text').map((b) => b.text || '').join('\n') : '';
const isoTs = (o) => { const t = Date.parse(o && o.timestamp); return Number.isFinite(t) ? t / 1000 : 0; };
const clip = (v, n) => String(v == null ? '' : v).slice(0, n);

function listFiles() {
  const out = [];
  let dirs = [];
  try { dirs = fs.readdirSync(projectsDir(), { withFileTypes: true }).filter((d) => d.isDirectory()); } catch { return out; }
  for (const d of dirs) {
    const dir = path.join(projectsDir(), d.name);
    let names = [];
    try { names = fs.readdirSync(dir); } catch { continue; }
    for (const f of names) {
      if (!f.endsWith('.jsonl')) continue;
      const file = path.join(dir, f);
      try { const st = fs.statSync(file); out.push({ id: f.slice(0, -6), file, mtime: st.mtimeMs, size: st.size }); } catch { /* gone */ }
    }
  }
  return out;
}
function findSession(sid) { return /^[A-Za-z0-9-]+$/.test(sid || '') ? listFiles().find((f) => f.id === sid) || null : null; }

const sumCache = new Map();
function summarize(f) {
  const hit = sumCache.get(f.file);
  if (hit && hit.mtime === f.mtime && hit.size === f.size) return hit;
  let title = '', cwd = '';
  for (const o of readJsonl(f.file)) {
    if (o.type === 'ai-title') title = o.aiTitle || title;
    cwd = cwd || o.cwd || '';
    if (!title && o.type === 'user' && !o.isSidechain) {
      const t = textOf((o.message || {}).content).trim();
      if (t && !t.startsWith('<')) title = t.slice(0, 80);
    }
  }
  const r = { mtime: f.mtime, size: f.size, title: title || 'Untitled', cwd };
  sumCache.set(f.file, r);
  return r;
}

/** Claude Code processes currently open, by working directory (Linux only; elsewhere we rely on file activity). */
function openClaudes() {
  const counts = {};
  if (process.platform !== 'linux' || process.env.FLATPAK_ID) return counts;
  let pids = [];
  try { pids = fs.readdirSync('/proc').filter((p) => /^\d+$/.test(p)); } catch { return counts; }
  for (const p of pids) {
    try {
      if (fs.readFileSync(`/proc/${p}/comm`, 'utf8').trim() !== 'claude') continue;
      if (fs.readFileSync(`/proc/${p}/cmdline`, 'utf8').includes('stream-json')) continue;
      const cwd = fs.readlinkSync(`/proc/${p}/cwd`);
      counts[cwd] = (counts[cwd] || 0) + 1;
    } catch { /* process ended */ }
  }
  return counts;
}

/** The chats, newest first. [keep] drops chats before the list is cut to [limit], so junk can never push real chats out. */
function sessions(active, keep, limit = 300) {
  const files = listFiles().sort((a, b) => b.mtime - a.mtime);
  const now = Date.now(), open = openClaudes(), hasProc = process.platform === 'linux' && !process.env.FLATPAK_ID;
  const out = [];
  for (const f of files) {
    const s = summarize(f);
    if (keep && !keep(f, s)) continue;
    const age = (now - f.mtime) / 1000;
    let live = active.has(f.id);
    if (!live) {
      if (hasProc) { if ((open[s.cwd] || 0) > 0) { live = true; open[s.cwd]--; } }
      else live = age < 120;
    }
    out.push({ id: f.id, title: s.title, cwd: s.cwd, mtime: Math.floor(f.mtime), live, busy: age < 20 || active.has(f.id) });
    if (out.length >= limit) break;
  }
  return out;
}

function toolInput(name, arg) {
  arg = arg || {};
  if (name === 'Edit' || name === 'NotebookEdit') return { file_path: arg.file_path || '', old_string: clip(arg.old_string, 1500), new_string: clip(arg.new_string, 1500) };
  if (name === 'MultiEdit') return { file_path: arg.file_path || '', edits: (arg.edits || []).slice(0, 6).map((e) => ({ old_string: clip(e.old_string, 800), new_string: clip(e.new_string, 800) })) };
  if (name === 'Write') return { file_path: arg.file_path || '', content: clip(arg.content, 1500) };
  if (name === 'Bash') return { command: clip(arg.command, 1500), description: clip(arg.description, 200) };
  const o = {};
  Object.entries(arg).slice(0, 6).forEach(([k, v]) => { o[k] = clip(typeof v === 'string' ? v : JSON.stringify(v), 300); });
  return o;
}
function resultText(c) {
  if (Array.isArray(c)) c = c.map((x) => (x && x.text) || '').join('\n');
  return clip(c, 2000);
}
function toolLine(b) {
  const a = b.input || {};
  const k = ['command', 'file_path', 'pattern', 'path', 'url'].find((x) => x in a);
  return `${b.name || ''} ${k ? clip(a[k], 60) : ''}`.trim();
}

const fmtTok = (n) => (n >= 1e6 ? (n / 1e6).toFixed(1) + 'M' : n >= 1e3 ? (n / 1e3).toFixed(n >= 1e4 ? 0 : 1) + 'k' : String(Math.round(n)));
const fmtDur = (ms) => { const s = Math.round(ms / 1000); return s < 60 ? s + 's' : Math.floor(s / 60) + 'm ' + (s % 60) + 's'; };
function turnLine(u, ms) {
  const parts = [`${fmtTok(u.out)} output`, `${fmtTok(u.in + u.cw)} input`];
  if (u.cr) parts.push(`${fmtTok(u.cr)} cached`);
  if (ms > 0) parts.push(fmtDur(ms));
  return parts.join(' · ');
}

function messages(sid, tail = 80) { return messagesOf(findSession(sid), tail); }

/** Same, for a chat file that lives somewhere else (for example a copy stored for another computer). */
function messagesOf(f, tail = 80) {
  if (!f) return [];
  const entries = readJsonl(f.file), results = {}, out = [];
  for (const o of entries) {
    const c = (o.message || {}).content;
    if (o.type === 'user' && Array.isArray(c)) for (const b of c) if (b.type === 'tool_result') results[b.tool_use_id || ''] = { result: resultText(b.content), error: !!b.is_error };
  }
  let turn = null;
  const flush = () => {
    if (turn && turn.ids.size) {
      const u = { in: 0, out: 0, cr: 0, cw: 0 };
      for (const m of turn.ids.values()) { u.in += m.in; u.out += m.out; u.cr += m.cr; u.cw += m.cw; }
      const ms = Math.max(0, turn.end - turn.start);
      out.push({ role: 'meta', text: turnLine(u, ms), tokens: u, ms, ts: turn.end });
    }
    turn = null;
  };
  for (const o of entries) {
    if (o.isSidechain || (o.type !== 'user' && o.type !== 'assistant')) continue;
    const c = (o.message || {}).content, ts = Math.floor(isoTs(o) * 1000);
    if (o.type === 'assistant' && turn) {
      const m = o.message || {}, us = m.usage;
      turn.end = Math.max(turn.end, ts);
      if (us && m.id && !String(m.model || '').startsWith('<')) {
        const r = { in: us.input_tokens || 0, out: us.output_tokens || 0, cr: us.cache_read_input_tokens || 0, cw: us.cache_creation_input_tokens || 0 };
        const old = turn.ids.get(m.id);
        if (!old || r.out >= old.out) turn.ids.set(m.id, r);
      }
    }
    if (o.type === 'user') {
      const t = textOf(c).trim();
      if (t && !t.startsWith('<')) { flush(); turn = { start: ts, end: ts, ids: new Map() }; out.push({ role: 'user', text: t, ts }); }
    } else if (Array.isArray(c)) {
      for (const b of c) {
        if (b.type === 'text' && (b.text || '').trim()) out.push({ role: 'assistant', text: b.text.trim(), ts });
        else if (b.type === 'tool_use') out.push({ role: 'tool', text: toolLine(b), name: b.name, input: toolInput(b.name, b.input), ts, ...(results[b.id || ''] || {}) });
      }
    }
  }
  flush();
  return out.slice(-tail);
}

// ───────── usage statistics ─────────
const statCache = new Map();
function parseUsage(file) {
  const msgs = new Map(), users = [], toolIds = new Map();
  let cwd = '';
  for (const o of readJsonl(file)) {
    cwd = cwd || o.cwd || '';
    if (o.type === 'assistant') {
      const m = o.message || {}, u = m.usage, model = m.model || '';
      if (!u || model.startsWith('<')) continue;
      const rec = { ts: isoTs(o), model, in: u.input_tokens || 0, out: u.output_tokens || 0, cr: u.cache_read_input_tokens || 0, cw: u.cache_creation_input_tokens || 0 };
      const key = m.id || o.uuid || String(msgs.size);
      if (!msgs.has(key) || rec.out >= msgs.get(key).out) msgs.set(key, rec);
      if (Array.isArray(m.content)) for (const b of m.content) if (b.type === 'tool_use') toolIds.set(b.id || String(toolIds.size), b.name || '');
    } else if (o.type === 'user' && !o.isSidechain && textOf((o.message || {}).content).trim()) users.push(isoTs(o));
  }
  return { cwd, msgs: [...msgs.values()], users, tools: [...toolIds.values()] };
}
const family = (m) => ['opus', 'sonnet', 'haiku', 'fable', 'mythos'].find((f) => m.includes(f)) || 'other';
const dayKey = (ms) => { const d = new Date(ms); return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`; };

function stats(days) {
  const now = Date.now(), cutoff = now - days * 864e5;
  const perDay = new Map(), projects = new Map(), models = {}, tools = {}, heat = Array.from({ length: 7 }, () => Array(24).fill(0));
  const row = (k) => { if (!perDay.has(k)) perDay.set(k, { in: 0, out: 0, cr: 0, cw: 0, msgs: 0, fam: {}, sess: new Set() }); return perDay.get(k); };
  for (const f of listFiles()) {
    if (f.mtime < cutoff) continue;
    let hit = statCache.get(f.file);
    if (!hit || hit.mtime !== f.mtime || hit.size !== f.size) { hit = { mtime: f.mtime, size: f.size, d: parseUsage(f.file) }; statCache.set(f.file, hit); }
    const d = hit.d, pk = d.cwd || 'unknown';
    if (!projects.has(pk)) projects.set(pk, { tokens: 0, sessions: new Set(), last: 0 });
    const proj = projects.get(pk);
    for (const m of d.msgs) {
      if (m.ts * 1000 < cutoff) continue;
      const r = row(dayKey(m.ts * 1000)), work = m.in + m.out + m.cw, fam = family(m.model);
      r.in += m.in; r.out += m.out; r.cr += m.cr; r.cw += m.cw; r.fam[fam] = (r.fam[fam] || 0) + work; r.sess.add(f.id);
      proj.tokens += work; proj.sessions.add(f.id); proj.last = Math.max(proj.last, m.ts); models[fam] = (models[fam] || 0) + work;
    }
    for (const ts of d.users) if (ts * 1000 >= cutoff) { row(dayKey(ts * 1000)).msgs++; const dt = new Date(ts * 1000); heat[(dt.getDay() + 6) % 7][dt.getHours()]++; }
    if (d.msgs.length && d.msgs[d.msgs.length - 1].ts * 1000 >= cutoff) for (const t of d.tools) tools[t] = (tools[t] || 0) + 1;
  }
  const outDays = [];
  for (let i = days - 1; i >= 0; i--) {
    const k = dayKey(now - i * 864e5), r = perDay.get(k);
    outDays.push({ date: k, in: r ? r.in : 0, out: r ? r.out : 0, cr: r ? r.cr : 0, cw: r ? r.cw : 0, msgs: r ? r.msgs : 0, sessions: r ? r.sess.size : 0, fam: r ? r.fam : {} });
  }
  const top = [...projects.entries()].sort((a, b) => b[1].tokens - a[1].tokens).slice(0, 10).filter(([, v]) => v.tokens)
    .map(([c, v]) => ({ cwd: c, tokens: v.tokens, sessions: v.sessions.size, last: Math.floor(v.last * 1000) }));
  const topTools = Object.fromEntries(Object.entries(tools).sort((a, b) => b[1] - a[1]).slice(0, 10));
  return { generated: now, days: outDays, models, tools: topTools, heat, projects: top };
}


/** Everything /usage-style: totals, cost at API prices, per model, per day, per project. days = 0 means all time. */
function report(days) {
  const pricing = require('./pricing');
  const now = Date.now(), cutoff = days ? now - days * 864e5 : 0;
  const byModel = new Map(), perDay = new Map(), proj = new Map(), sessions = new Set(), active = new Set(), tools = {};
  let first = 0, last = 0, prompts = 0, longest = 0, assistant = 0;
  const tot = { in: 0, out: 0, cr: 0, cw: 0, cost: 0 };
  for (const f of listFiles()) {
    if (f.mtime < cutoff) continue;
    let hit = statCache.get(f.file);
    if (!hit || hit.mtime !== f.mtime || hit.size !== f.size) { hit = { mtime: f.mtime, size: f.size, d: parseUsage(f.file) }; statCache.set(f.file, hit); }
    const d = hit.d, pk = d.cwd || 'unknown';
    let lo = 0, hi = 0;
    for (const m of d.msgs) {
      const t = m.ts * 1000; if (t < cutoff) continue;
      const cost = pricing.costOf(m.model, m), k = m.model || 'unknown';
      const b = byModel.get(k) || { model: k, label: pricing.labelOf(k), in: 0, out: 0, cr: 0, cw: 0, msgs: 0, cost: 0 };
      b.in += m.in; b.out += m.out; b.cr += m.cr; b.cw += m.cw; b.msgs++; b.cost += cost; byModel.set(k, b);
      tot.in += m.in; tot.out += m.out; tot.cr += m.cr; tot.cw += m.cw; tot.cost += cost; assistant++;
      const dk = dayKey(t), r = perDay.get(dk) || { date: dk, tokens: 0, cost: 0 }; r.tokens += m.in + m.out + m.cw; r.cost += cost; perDay.set(dk, r); active.add(dk);
      const p = proj.get(pk) || { cwd: pk, tokens: 0, cost: 0, sessions: new Set() }; p.tokens += m.in + m.out + m.cw; p.cost += cost; p.sessions.add(f.id); proj.set(pk, p);
      sessions.add(f.id); first = !first || t < first ? t : first; last = Math.max(last, t); lo = !lo || t < lo ? t : lo; hi = Math.max(hi, t);
    }
    longest = Math.max(longest, hi - lo);
    for (const ts of d.users) if (ts * 1000 >= cutoff) prompts++;
    if (d.msgs.length) for (const t of d.tools) tools[t] = (tools[t] || 0) + 1;
  }
  // streaks of days with activity
  const keys = [...active].sort(); let bestStreak = 0, run = 0, prev = null;
  for (const k of keys) { const t = Date.parse(k + 'T12:00:00'); run = prev && Math.round((t - prev) / 864e5) === 1 ? run + 1 : 1; bestStreak = Math.max(bestStreak, run); prev = t; }
  let current = 0; for (let t = Date.parse(dayKey(now) + 'T12:00:00'); active.has(dayKey(t)); t -= 864e5) current++;
  const span = Math.min(days || 365, 365), daysOut = [];
  for (let i = span - 1; i >= 0; i--) { const k = dayKey(now - i * 864e5), r = perDay.get(k); daysOut.push({ date: k, tokens: r ? r.tokens : 0, cost: r ? r.cost : 0 }); }
  const models = [...byModel.values()].sort((a, b) => b.cost - a.cost);
  const cacheSaved = models.reduce((s, m) => { const p = pricing.priceOf(m.model); return s + m.cr * (p.in - p.cr) / 1e6; }, 0);
  const work = tot.in + tot.out + tot.cw;
  return {
    range: days, generated: now,
    totals: { ...tot, tokens: work, allTokens: work + tot.cr, messages: assistant, prompts, sessions: sessions.size, activeDays: active.size, longestStreak: bestStreak, currentStreak: current, first, last, longestSessionMs: longest, avgPerActiveDay: active.size ? Math.round(work / active.size) : 0, cacheSaved },
    models, days: daysOut, favorite: models[0] ? models[0].label : '',
    projects: [...proj.values()].sort((a, b) => b.cost - a.cost).slice(0, 12).map((p) => ({ cwd: p.cwd, tokens: p.tokens, cost: p.cost, sessions: p.sessions.size })),
    tools: Object.fromEntries(Object.entries(tools).sort((a, b) => b[1] - a[1]).slice(0, 8)),
    note: 'Cost is what these tokens would cost at Anthropic API prices. A Pro or Max subscription is not billed per token.',
  };
}

module.exports = { report, messagesOf, listFiles, turnLine, findSession, sessions, messages, stats, summarize, toolInput, resultText, toolLine, textOf, isoTs, readJsonl, projectsDir };
