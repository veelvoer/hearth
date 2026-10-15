'use strict';
/* Shared chats ("rooms"). One room lives on the host's server. People take part with a guest key that only opens that room.
   Messages of the room are a shared log. Each person's prompts run on their OWN Claude login and server (their app does that and posts the reply here),
   or a guest can ask the host to run a prompt on the HOST's account: the host sees the prompt, may edit it, and approves it, or gives that person full permission.
   Nothing here starts Claude by itself: the server passes a function that does (see server.js). */
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

const MAX_MSG = 20000, MAX_LOG = 2000, MAX_PENDING = 10, INVITE_MS = 24 * 3600e3, WORK_TTL = 6 * 60e3;
const sha = (s) => crypto.createHash('sha256').update(String(s)).digest('hex');
const rid = (n = 8) => crypto.randomBytes(n).toString('base64url');
const clean = (s, n) => String(s == null ? '' : s).replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f]/g, '').slice(0, n);
const MODES = ['plan', 'acceptEdits', 'manual', 'bypassPermissions'];

function createRooms({ dir, hostName, now = () => Date.now(), run, bypassOk = () => false, allowCwd = () => true, log = () => {} }) {
  const file = path.join(dir, 'rooms.json');
  let st = { rooms: {} };
  try { const j = JSON.parse(fs.readFileSync(file, 'utf8')); if (j && j.rooms) st = j; } catch { /* none yet */ }
  const save = () => { try { fs.mkdirSync(dir, { recursive: true }); const t = file + '.tmp'; fs.writeFileSync(t, JSON.stringify(st), { mode: 0o600 }); fs.renameSync(t, file); } catch (e) { log('rooms: could not save: ' + e.message); } };
  const busy = new Map();   // room id -> true while the host's Claude is running a prompt for it
  const fail = (code, error) => ({ code, data: { error } });
  const ok = (data) => ({ code: 200, data });

  const get = (id) => st.rooms[id] || null;
  function push(r, m) {
    r.seq = (r.seq || 0) + 1;
    const msg = { n: r.seq, ts: now(), kind: 'user', via: '', ...m, text: clean(m.text, MAX_MSG) };
    r.messages.push(msg); if (r.messages.length > MAX_LOG) r.messages.splice(0, r.messages.length - MAX_LOG);
    return msg;
  }
  const memberOf = (r, token) => { const h = sha(token); return Object.values(r.members).find((m) => !m.revoked && m.tokenHash === h) || null; };
  const grantOf = (m) => ({ auto: !!(m.grant && m.grant.auto), mode: MODES.includes(m.grant && m.grant.mode) ? m.grant.mode : 'plan' });
  const live = (r) => { for (const [k, w] of Object.entries(r.working || {})) if (now() - w.since > WORK_TTL) delete r.working[k]; return Object.entries(r.working || {}).map(([k, w]) => ({ key: k, name: w.name, lane: w.lane, since: w.since })); };

  /** The view of a room for the owner or for one guest. Messages after `since` only. */
  function view(r, who, since) {
    const msgs = r.messages.filter((m) => m.n > since);
    const members = Object.values(r.members).filter((m) => !m.revoked).map((m) => ({ id: m.id, name: m.name, joined: m.joined, ...(who.owner ? { grant: grantOf(m) } : {}) }));
    const reqs = Object.values(r.requests).filter((q) => who.owner || q.mid === who.member.id).sort((a, b) => a.ts - b.ts).slice(-40)
      .map((q) => ({ id: q.id, mid: q.mid, name: q.name, text: q.text, status: q.status, ts: q.ts, mode: q.mode || null, ...(q.edited ? { edited: true, finalText: who.owner || q.mid === who.member.id ? q.finalText : undefined } : {}), note: q.note || '' }));
    return { id: r.id, title: r.title, cwd: who.owner ? r.cwd : undefined, host: hostName, role: who.owner ? 'host' : 'guest', me: who.owner ? 'host' : who.member.id, myName: who.owner ? hostName : who.member.name,
      grant: who.owner ? undefined : grantOf(who.member), seq: r.seq || 0, messages: msgs, members, requests: reqs, working: live(r), hostBusy: !!busy.get(r.id) };
  }

  // ───────── host's Claude: runs prompts that were approved ─────────
  function promptFor(r, text, by) {
    const since = r.hostSeen || 0;
    const others = r.messages.filter((m) => m.n > since && m.from !== 'host-claude' && m.kind !== 'status').slice(-40);
    const head = `You are in a shared chat called "${r.title}" with ${Object.values(r.members).filter((m) => !m.revoked).map((m) => m.name).concat([hostName]).join(', ')}. `
      + (by ? `The next message comes from ${by}, not from the owner of this account. ` : '') + 'Answer it directly.';
    const ctx = others.length ? '\n\nWhat was said in the chat since your last turn:\n' + others.map((m) => `${m.name}: ${m.text}`).join('\n') : '';
    return `${head}${ctx}\n\n${by || hostName}: ${text}`;
  }
  const queue = new Map();   // room id -> [{ text, by, mode }]
  function pump(r) {
    if (busy.get(r.id)) return;
    const next = (queue.get(r.id) || []).shift(); if (!next) return;
    busy.set(r.id, true);
    r.working = r.working || {}; r.working.host = { name: `${hostName}'s Claude`, lane: 'host', since: now() };
    const upto = r.seq || 0;
    let started;
    try { started = run({ room: r, text: promptFor(r, next.text, next.by), mode: next.mode, session: r.hostSession || '' }, (res) => {
      busy.set(r.id, false); delete r.working.host;
      if (res.session) r.hostSession = res.session;
      push(r, { from: 'host-claude', name: `Claude (${hostName}'s account)`, kind: 'assistant', via: 'host', text: res.error ? `(${res.error})` : (res.text || '(no answer)') });
      r.hostSeen = Math.max(r.hostSeen || 0, upto);   // what was said while Claude worked is passed on with the next prompt
      save(); pump(r);
    }); } catch (e) { started = { error: e.message }; }
    if (started && started.error) { busy.set(r.id, false); delete r.working.host; push(r, { from: 'host-claude', name: 'Hearth', kind: 'status', text: 'Could not start Claude: ' + started.error }); save(); pump(r); }
    save();
  }
  const enqueue = (r, item) => { const q = queue.get(r.id) || []; q.push(item); queue.set(r.id, q); pump(r); };

  function downgrade(mode) { const m = MODES.includes(mode) ? mode : 'plan'; return m === 'bypassPermissions' && !bypassOk() ? 'acceptEdits' : m; }

  /** Routes. `who` = { owner: true } or { member }. Returns { code, data }. */
  function route(method, parts, body, who, ip) {
    // parts: after "rooms"
    if (!parts.length) {
      if (!who.owner) return fail(403, 'not allowed');
      if (method === 'GET') return ok(Object.values(st.rooms).map((r) => ({ id: r.id, title: r.title, cwd: r.cwd, members: Object.values(r.members).filter((m) => !m.revoked).length, pending: Object.values(r.requests).filter((q) => q.status === 'pending').length, last: (r.messages[r.messages.length - 1] || {}).ts || r.created })));
      if (method === 'POST') {
        const title = clean(body.title, 80).trim(), cwd = String(body.cwd || '');
        if (!title) return fail(400, 'Give the shared chat a name.');
        if (!cwd || !allowCwd(cwd)) return fail(400, 'Pick a folder on this server for your Claude to work in.');
        const r = { id: rid(6), title, cwd, created: now(), messages: [], seq: 0, members: {}, invites: {}, requests: {}, working: {}, hostSession: '', hostSeen: 0 };
        st.rooms[r.id] = r; push(r, { from: 'system', name: 'Hearth', kind: 'status', text: `${hostName} started this shared chat.` }); save();
        return ok({ id: r.id });
      }
      return fail(405, 'no');
    }
    const id = parts[0];
    if (id === 'join' && method === 'POST') return join(body);
    const r = get(id); if (!r) return fail(404, 'No such shared chat.');
    if (who.member && who.member.roomId !== r.id) return fail(403, 'not allowed');
    const sub = parts[1];
    if (!sub) {
      if (method === 'GET') return ok(view(r, who, Number(body.since) || 0));
      if (method === 'DELETE' && who.owner) { delete st.rooms[r.id]; save(); return ok({ ok: true }); }
      return fail(405, 'no');
    }
    if (sub === 'messages' && method === 'POST') {
      const text = clean(body.text, MAX_MSG).trim(); if (!text) return fail(400, 'Empty message.');
      const kind = body.kind === 'assistant' ? 'assistant' : 'user';
      if (who.owner) { push(r, { from: 'host', name: hostName, kind, via: kind === 'assistant' ? 'own' : '', text }); }
      else { const m = who.member; push(r, { from: m.id, name: m.name, kind, via: kind === 'assistant' ? 'own' : '', text }); }
      save(); return ok({ seq: r.seq });
    }
    if (sub === 'working' && method === 'POST') {
      const key = who.owner ? 'host-own' : who.member.id, name = who.owner ? hostName : who.member.name;
      r.working = r.working || {};
      if (body.on) r.working[key] = { name: `${name}'s Claude`, lane: 'own', since: now() }; else delete r.working[key];
      return ok({ ok: true });
    }
    if (sub === 'ask' && method === 'POST') {   // the host runs a prompt on their own Claude, at once
      if (!who.owner) return fail(403, 'Only the host can do this. Send a request instead.');
      const text = clean(body.text, MAX_MSG).trim(); if (!text) return fail(400, 'Empty message.');
      push(r, { from: 'host', name: hostName, kind: 'user', text });
      enqueue(r, { text, by: '', mode: downgrade(body.mode || 'acceptEdits') }); save();
      return ok({ ok: true, seq: r.seq });
    }
    if (sub === 'requests' && !parts[2] && method === 'POST') {   // a guest asks the host to run a prompt on the host's account
      if (who.owner) return fail(400, 'You are the host: use "ask".');
      const m = who.member, text = clean(body.text, MAX_MSG).trim(); if (!text) return fail(400, 'Empty message.');
      const mine = Object.values(r.requests).filter((q) => q.mid === m.id);
      if (mine.filter((q) => q.status === 'pending').length >= MAX_PENDING) return fail(429, 'You have too many requests waiting. Wait for the host.');
      if (mine.some((q) => now() - q.ts < 2000)) return fail(429, 'Slow down a little.');
      const q = { id: rid(5), mid: m.id, name: m.name, text, ts: now(), status: 'pending' };
      r.requests[q.id] = q;
      push(r, { from: m.id, name: m.name, kind: 'user', via: 'ask-host', text });
      const g = grantOf(m);
      if (g.auto) { q.status = 'approved'; q.mode = downgrade(g.mode); q.finalText = text; q.note = 'Run automatically (full permission)'; enqueue(r, { text, by: m.name, mode: q.mode }); }
      save(); return ok({ id: q.id, status: q.status });
    }
    if (sub === 'requests' && parts[2] && method === 'POST') {   // the host decides
      if (!who.owner) return fail(403, 'Only the host can decide.');
      const q = r.requests[parts[2]]; if (!q) return fail(404, 'No such request.');
      if (q.status !== 'pending') return fail(409, 'Already decided.');
      if (body.decision === 'deny') { q.status = 'denied'; q.note = clean(body.note, 200); push(r, { from: 'system', name: 'Hearth', kind: 'status', text: `${hostName} did not run ${q.name}'s request.` }); save(); return ok({ ok: true }); }
      if (body.decision !== 'approve') return fail(400, 'decision must be approve or deny');
      const text = clean(body.text == null ? q.text : body.text, MAX_MSG).trim(); if (!text) return fail(400, 'Empty prompt.');
      q.status = 'approved'; q.mode = downgrade(body.mode || 'plan'); q.finalText = text; q.edited = text !== q.text;
      if (q.edited) push(r, { from: 'system', name: 'Hearth', kind: 'status', text: `${hostName} changed the prompt of ${q.name} before running it.` });
      if (body.always) { const m = r.members[q.mid]; if (m) m.grant = { auto: true, mode: q.mode }; }
      enqueue(r, { text, by: q.name, mode: q.mode }); save();
      return ok({ ok: true });
    }
    if (sub === 'invite' && method === 'POST') {
      if (!who.owner) return fail(403, 'Only the host can invite.');
      const code = rid(16); r.invites[sha(code)] = { exp: now() + INVITE_MS, name: clean(body.name, 40) }; save();
      return ok({ invite: code, room: r.id, expires: now() + INVITE_MS });
    }
    if (sub === 'members' && parts[2]) {
      if (!who.owner) return fail(403, 'Only the host can do this.');
      const m = r.members[parts[2]]; if (!m || m.revoked) return fail(404, 'No such person.');
      if (method === 'DELETE') { m.revoked = true; for (const q of Object.values(r.requests)) if (q.mid === m.id && q.status === 'pending') q.status = 'denied'; push(r, { from: 'system', name: 'Hearth', kind: 'status', text: `${m.name} was removed.` }); save(); return ok({ ok: true }); }
      if (method === 'POST') {
        const g = body.grant || {}; m.grant = { auto: !!g.auto, mode: downgrade(g.mode || 'plan') };
        if (!bypassOk() && g.mode === 'bypassPermissions') m.grant.mode = 'acceptEdits';
        save(); return ok({ grant: grantOf(m) });
      }
    }
    if (sub === 'leave' && method === 'POST' && !who.owner) { const m = who.member; m.revoked = true; push(r, { from: 'system', name: 'Hearth', kind: 'status', text: `${m.name} left.` }); save(); return ok({ ok: true }); }
    return fail(404, 'not found');
  }

  const joinFails = new Map();
  function join(body) {
    const code = String(body.invite || ''), h = sha(code);
    const r = Object.values(st.rooms).find((x) => x.invites[h]);
    const inv = r && r.invites[h];
    if (!inv || inv.exp < now()) { return fail(404, 'This invite does not work (used up, expired or wrong).'); }
    delete r.invites[h];   // one person per invite
    const name = clean(body.name || inv.name, 40).trim() || 'Guest';
    const secret = rid(24), mid = rid(5);
    r.members[mid] = { id: mid, roomId: r.id, name, tokenHash: sha(`room.${r.id}.${secret}`), joined: now(), grant: { auto: false, mode: 'plan' } };
    push(r, { from: 'system', name: 'Hearth', kind: 'status', text: `${name} joined.` }); save();
    return ok({ room: r.id, title: r.title, token: `room.${r.id}.${secret}`, member: mid, host: hostName });
  }

  /** Who is calling? A guest key ("room.<id>.<secret>") or null. */
  function guestFor(token) {
    const m = /^room\.([A-Za-z0-9_-]{4,16})\.([A-Za-z0-9_-]{10,64})$/.exec(String(token || '')); if (!m) return null;
    const r = get(m[1]); if (!r) return null;
    const mem = memberOf(r, token); return mem ? { member: { ...mem, roomId: r.id } } : null;
  }
  return { route, guestFor, state: () => st, save };
}
module.exports = { createRooms, MODES };
