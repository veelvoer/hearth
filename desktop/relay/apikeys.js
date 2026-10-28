'use strict';
/* Hearth API keys: an app of yours can call Claude through your Hearth server with a key, and it uses your plan's usage instead of credits.
   A key can be limited to some models and effort levels (low / high / general), to some requests a day, to a point of the week's usage, and it can expire.
   Keys are stored only as a hash; the key itself is shown once when it is made. */
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

const MODELS = ['haiku', 'sonnet', 'opus'];                       // cheapest first
const EFFORTS = ['low', 'medium', 'high', 'xhigh', 'max'];       // lightest first
const PRESETS = {
  low: { models: ['haiku', 'sonnet'], efforts: ['low', 'medium'] },
  high: { models: ['sonnet', 'opus'], efforts: ['high', 'xhigh', 'max'] },
  general: { models: MODELS.slice(), efforts: EFFORTS.slice() },
};
const sha = (s) => crypto.createHash('sha256').update(String(s)).digest('hex');
const dayOf = (t) => new Date(t).toISOString().slice(0, 10);

function createKeys({ dir, now = () => Date.now() }) {
  const file = path.join(dir, 'apikeys.json');
  let db = { keys: [] };
  try { const d = JSON.parse(fs.readFileSync(file, 'utf8')); if (d && Array.isArray(d.keys)) db = d; } catch { /* first run */ }
  const save = () => { try { fs.mkdirSync(dir, { recursive: true }); fs.writeFileSync(file + '.tmp', JSON.stringify(db), { mode: 0o600 }); fs.renameSync(file + '.tmp', file); } catch { /* best effort */ } };
  const view = (k) => ({ id: k.id, name: k.name, kind: k.kind, ends: k.last4, models: k.models, efforts: k.efforts, perDay: k.perDay, weekStopPct: k.weekStopPct, sessionStopPct: k.sessionStopPct || 0, session: k.session || 'none', expiresAt: k.expiresAt, created: k.created, revoked: !!k.revoked,
    usedToday: k.day === dayOf(now()) ? k.count : 0, total: k.total || 0, tokens: k.tokens || 0, lastUsed: k.lastUsed || 0 });
  const pickList = (given, all, fallback) => { const l = (Array.isArray(given) ? given : []).map(String).filter((x) => all.includes(x)); return l.length ? all.filter((x) => l.includes(x)) : fallback; };

  function create(b) {
    const kind = PRESETS[b.kind] ? b.kind : 'general', pre = PRESETS[kind];
    const name = String(b.name || '').trim().slice(0, 60) || (kind[0].toUpperCase() + kind.slice(1) + ' key');
    const key = 'hk_' + crypto.randomBytes(24).toString('base64url');
    const exp = Number(b.expiresAt) > now() ? Number(b.expiresAt) : (Number(b.expiresInDays) > 0 ? now() + Math.min(3650, Number(b.expiresInDays)) * 86400000 : 0);
    const rec = { id: crypto.randomBytes(4).toString('hex'), name, kind, hash: sha(key), last4: key.slice(-4), models: pickList(b.models, MODELS, pre.models), efforts: pickList(b.efforts, EFFORTS, pre.efforts),
      perDay: Math.max(0, Math.min(100000, Math.round(Number(b.perDay) || 0))), weekStopPct: Math.max(0, Math.min(100, Number(b.weekStopPct) || 0)), sessionStopPct: Math.max(0, Math.min(100, Number(b.sessionStopPct) || 0)), session: ['none', 'key', 'named'].includes(b.session) ? (b.session === 'key' ? 'key' : b.session) : 'none', sessions: {}, expiresAt: exp, created: now(), revoked: false, day: dayOf(now()), count: 0, total: 0, tokens: 0 };
    db.keys.push(rec); save();
    return { key, ...view(rec) };
  }
  const list = () => db.keys.map(view);
  function revoke(id) { const k = db.keys.find((x) => x.id === id); if (!k) return false; k.revoked = true; save(); return true; }
  function remove(id) { const n = db.keys.length; db.keys = db.keys.filter((x) => x.id !== id); if (db.keys.length !== n) { save(); return true; } return false; }
  function authenticate(token) {
    if (!/^hk_[A-Za-z0-9_-]{20,}$/.test(String(token || ''))) return null;
    const h = Buffer.from(sha(token)); let hit = null;
    for (const k of db.keys) { const w = Buffer.from(k.hash); if (w.length === h.length && crypto.timingSafeEqual(w, h)) hit = k; }
    return hit;
  }
  /** Can this key be used right now? [weekPct] is how much of the plan's week is used. */
  function check(k, weekPct, sessionPct) {
    if (k.revoked) return { ok: false, code: 401, why: 'This key was revoked.' };
    if (k.expiresAt && k.expiresAt <= now()) return { ok: false, code: 401, why: 'This key has expired.' };
    if (k.day !== dayOf(now())) { k.day = dayOf(now()); k.count = 0; }
    if (k.perDay && k.count >= k.perDay) return { ok: false, code: 429, why: `This key may make ${k.perDay} requests a day and has used them. It resets at midnight UTC.` };
    if (k.weekStopPct && Number.isFinite(weekPct) && weekPct >= k.weekStopPct) return { ok: false, code: 429, why: `This key stops when ${k.weekStopPct}% of the weekly usage is used (now ${Math.round(weekPct)}%).` };
    if (k.sessionStopPct && Number.isFinite(sessionPct) && sessionPct >= k.sessionStopPct) return { ok: false, code: 429, why: `This key stops when ${k.sessionStopPct}% of the 5-hour session is used (now ${Math.round(sessionPct)}%).` };
    return { ok: true };
  }
  /** The remembered Claude session of a key (or of a name under that key). Made on first use; [reset] starts a new one. */
  function session(k, name, reset) {
    k.sessions = k.sessions || {}; const nm = k.session === 'key' ? 'main' : name;
    let s = k.sessions[nm];
    if (!s || reset) { s = k.sessions[nm] = { id: crypto.randomUUID(), started: false, last: 0 }; }
    return { id: s.id, fresh: !s.started, name: nm, key: k.id };
  }
  function sessionUsed(k, name) { const s = (k.sessions || {})[k.session === 'key' ? 'main' : name]; if (s) { s.started = true; s.last = now(); save(); } }
  function sessionForget(k, name) { const nm = k.session === 'key' ? 'main' : name; if (k.sessions) delete k.sessions[nm]; save(); }
  function used(k, tokens) { if (k.day !== dayOf(now())) { k.day = dayOf(now()); k.count = 0; } k.count++; k.total = (k.total || 0) + 1; k.tokens = (k.tokens || 0) + (tokens || 0); k.lastUsed = now(); save(); }

  /** How demanding a request looks, 0 (a one-liner) to 3 (long, or asks for real work). */
  function difficulty(text, wanted) {
    const t = String(text || ''); let d = t.length > 12000 ? 3 : t.length > 3000 ? 2 : t.length > 400 ? 1 : 0;
    if (/\b(prove|derive|architecture|refactor|debug|optimi[sz]e|analy[sz]e|step by step|algorithm|plan|design|security|migrate|review)\b/i.test(t)) d = Math.min(3, d + 1);
    if (/```|\bfunction\b|\bclass\b|\bSELECT\b|\bimport\b/.test(t)) d = Math.min(3, d + 1);
    const w = EFFORTS.indexOf(String(wanted || '')); if (w >= 0) d = Math.max(d, Math.min(3, Math.round(w * 3 / (EFFORTS.length - 1))));
    return d;
  }
  const spread = (list, d) => list[Math.min(list.length - 1, Math.round(d * (list.length - 1) / 3))];
  /** The model and effort for a request: what the caller asked for if the key allows it, else the best fit among what the key allows. */
  function choose(k, { model, effort, text }) {
    const want = String(model || '').toLowerCase(); const named = MODELS.find((m) => want.includes(m));
    if (named && !k.models.includes(named)) return { error: `This key may use ${k.models.join(', ')}, not ${named}. Ask for "auto" to let Hearth pick.` };
    const ef = String(effort || '').toLowerCase();
    if (ef && ef !== 'auto' && EFFORTS.includes(ef) && !k.efforts.includes(ef)) return { error: `This key may use the effort levels ${k.efforts.join(', ')}, not ${ef}.` };
    const d = difficulty(text, ef && ef !== 'auto' ? ef : '');
    return { model: named || spread(k.models, d), effort: ef && ef !== 'auto' && EFFORTS.includes(ef) ? ef : spread(k.efforts, d), auto: !named };
  }
  return { create, list, revoke, remove, authenticate, check, used, session, sessionUsed, sessionForget, choose, difficulty, PRESETS, MODELS, EFFORTS };
}
module.exports = { createKeys, MODELS, EFFORTS, PRESETS };
