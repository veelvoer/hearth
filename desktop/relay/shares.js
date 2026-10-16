'use strict';
/* Shared chats. A shared chat is a NORMAL chat of the host (a Claude Code session on the host's server); sharing it gives other people a guest key that opens only that chat.
   They read it like any chat. When one of them sends a prompt it is NOT run at once: it is a request on the host's account. The host gets a notification, can change the
   prompt, picks how much Claude may do, and runs it, or denies it, or gives that person full permission. The prompt then runs in the same chat as "<name>: <prompt>". */
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

const MAX_TEXT = 20000, MAX_PENDING = 5, INVITE_MS = 24 * 3600e3, MODES = ['plan', 'acceptEdits', 'manual', 'bypassPermissions'];
const sha = (s) => crypto.createHash('sha256').update(String(s)).digest('hex');
const rid = (n = 8) => crypto.randomBytes(n).toString('base64url');
const clean = (s, n) => String(s == null ? '' : s).replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f]/g, '').slice(0, n);

function createShares({ dir, hostName, now = () => Date.now(), bypassOk = () => false, start, busy = () => false, onEvent = () => {}, log = () => {} }) {
  const file = path.join(dir, 'shares.json');
  let st = { shares: {} };
  try { const j = JSON.parse(fs.readFileSync(file, 'utf8')); if (j && j.shares) st = j; } catch { /* none yet */ }
  const save = () => { try { fs.mkdirSync(dir, { recursive: true }); const t = file + '.tmp'; fs.writeFileSync(t, JSON.stringify(st), { mode: 0o600 }); fs.renameSync(t, file); } catch (e) { log('shares: could not save: ' + e.message); } };
  const fail = (code, error) => ({ code, data: { error } });
  const ok = (data) => ({ code: 200, data });
  const queue = [];   // approved prompts waiting for their chat to be free: { shareId, reqId }

  const get = (id) => st.shares[id] || null;
  const bySession = (sid) => Object.values(st.shares).find((s) => s.sid === sid) || null;
  const grantOf = (m) => ({ auto: !!(m.grant && m.grant.auto), mode: MODES.includes(m.grant && m.grant.mode) ? m.grant.mode : 'plan' });
  const downgrade = (mode) => { const m = MODES.includes(mode) ? mode : 'plan'; return m === 'bypassPermissions' && !bypassOk() ? 'acceptEdits' : m; };
  const live = (s) => Object.values(s.members).filter((m) => !m.revoked);

  // on start: prompts that were approved but not started go back in the line; ones that were running when the server stopped are marked
  for (const s of Object.values(st.shares)) for (const q of Object.values(s.requests)) {
    if (q.status === 'approved' && toOf(q) === 'host') queue.push({ shareId: s.id, reqId: q.id });
    else if (q.status === 'running') { q.status = 'failed'; q.note = 'The server restarted while this ran.'; }
  }

  const event = (s, q) => ({ kind: 'request', session: s.sid, title: s.title, cwd: s.cwd, req: q.id, from: q.name, text: `${q.name}: ${q.text}`.slice(0, 600), call: false, tool: '', input: { prompt: q.text }, ...(q.to && q.to !== 'host' ? { forMember: q.to } : {}) });
  const toOf = (q) => q.to || 'host';
  /** Waiting requests as notification events (so a client that missed the live event still sees them). */
  const pendingEvents = () => { const out = []; for (const s of Object.values(st.shares)) for (const q of Object.values(s.requests)) if (q.status === 'pending' && toOf(q) === 'host' && !(s.members[q.mid] || {}).revoked) out.push({ ...event(s, q), ts: q.ts, id: 0 }); return out; };
  /** Requests that wait for a friend's decision (the host asked to use their Claude). */
  const incomingEvents = (s, mid) => Object.values(s.requests).filter((q) => q.status === 'pending' && toOf(q) === mid).map((q) => ({ ...event(s, q), ts: q.ts, id: 0 }));
  const findRequest = (reqId) => { for (const s of Object.values(st.shares)) if (s.requests[reqId]) return { s, q: s.requests[reqId] }; return null; };
  const hasRequest = (reqId) => !!findRequest(reqId);

  function pump() {
    for (let i = 0; i < queue.length;) {
      const it = queue[i], s = get(it.shareId), q = s && s.requests[it.reqId];
      if (!q || q.status !== 'approved') { queue.splice(i, 1); continue; }
      if (busy(s.sid)) { i++; continue; }
      const r = start({ sid: s.sid, cwd: s.cwd, text: contextFor(s) + `${q.name}: ${q.finalText}`, mode: q.mode, by: q.name, reqId: q.id, mid: q.mid });
      if (r && r.busy) { i++; continue; }
      queue.splice(i, 1);
      if (r && r.error) { q.status = 'failed'; q.note = clean(r.error, 200); } else q.status = 'running';
      save();
    }
  }
  /** The server tells us when a run of a chat has ended. */
  function jobDone(sid) { const s = bySession(sid); if (s) for (const q of Object.values(s.requests)) if (q.status === 'running') { q.status = 'done'; } save(); pump(); }

  // ───────── owner side ─────────
  function ensure(sid, cwd, title) {
    let s = bySession(sid);
    if (!s) { s = { id: rid(6), sid, cwd, title: clean(title, 120) || 'Shared chat', created: now(), members: {}, invites: {}, requests: {} }; st.shares[s.id] = s; save(); }
    return s;
  }
  const view = (s) => ({ ...ownerInfo(s), id: s.id, session: s.sid, title: s.title, members: live(s).map((m) => ({ id: m.id, name: m.name, joined: m.joined, grant: grantOf(m) })), pending: Object.values(s.requests).filter((q) => q.status === 'pending' && !(s.members[q.mid] || {}).revoked).length });
  function invite(s, name) { const code = rid(16); s.invites[sha(code)] = { exp: now() + INVITE_MS, name: clean(name, 40) }; save(); return { invite: code, expires: now() + INVITE_MS }; }
  function setGrant(s, mid, g) { const m = s.members[mid]; if (!m || m.revoked) return fail(404, 'No such person.'); m.grant = { auto: !!(g && g.auto), mode: downgrade(g && g.mode) }; save(); return ok({ grant: grantOf(m) }); }
  function removeMember(s, mid) {
    const m = s.members[mid]; if (!m || m.revoked) return fail(404, 'No such person.');
    m.revoked = true;
    for (const q of Object.values(s.requests)) if (q.mid === mid && (q.status === 'pending' || q.status === 'approved')) { q.status = 'denied'; q.note = 'Removed'; onEvent({ kind: 'resolved', req: q.id, session: s.sid }); }
    save(); return ok({ ok: true });
  }
  function unshare(s) { for (const q of Object.values(s.requests)) if (q.status === 'pending') onEvent({ kind: 'resolved', req: q.id, session: s.sid }); delete st.shares[s.id]; save(); return ok({ ok: true }); }

  /** The host decides on a request. body: { decision: 'approve'|'deny', text, mode, always } */
  function decide(reqId, body) {
    const f = findRequest(reqId); if (!f) return fail(404, 'No such request.');
    const { s, q } = f;
    if (toOf(q) !== 'host') return fail(403, 'Only ' + ((s.members[toOf(q)] || {}).name || 'that person') + ' can decide this.');
    if (q.status !== 'pending') return fail(409, 'Already decided.');
    const m = s.members[q.mid];
    if (!m || m.revoked) { q.status = 'denied'; q.note = 'Removed'; save(); return fail(409, 'That person was removed.'); }
    if (body.decision === 'deny') { q.status = 'denied'; q.note = clean(body.note, 200); save(); onEvent({ kind: 'resolved', req: q.id, session: s.sid }); return ok({ ok: true }); }
    if (body.decision !== 'approve') return fail(400, 'decision must be approve or deny');
    const text = clean(body.text == null ? q.text : body.text, MAX_TEXT).trim(); if (!text) return fail(400, 'Empty prompt.');
    q.status = 'approved'; q.finalText = text; q.edited = text !== q.text; q.mode = downgrade(body.mode || 'plan');
    if (body.always) m.grant = { auto: true, mode: q.mode };
    queue.push({ shareId: s.id, reqId: q.id }); save();
    onEvent({ kind: 'resolved', req: q.id, session: s.sid });
    pump();
    return ok({ ok: true, status: q.status });
  }

  // ───────── guest side ─────────
  const joinFail = () => fail(404, 'This invite does not work (used up, expired or wrong).');
  function join(body) {
    const h = sha(String(body.invite || ''));
    const s = Object.values(st.shares).find((x) => x.invites[h]); const inv = s && s.invites[h];
    if (!inv || inv.exp < now()) return joinFail();
    delete s.invites[h];   // one person per invite
    const name = clean(body.name || inv.name, 40).trim() || 'Guest', secret = rid(24), mid = rid(5);
    s.members[mid] = { id: mid, name, tokenHash: sha(`share.${s.id}.${secret}`), joined: now(), grant: { auto: false, mode: 'plan' } };
    save();
    return ok({ share: s.id, session: s.sid, title: s.title, host: hostName, member: mid, key: `share.${s.id}.${secret}` });
  }
  /** A guest key ("share.<id>.<secret>") -> { share, member } or null. */
  function guestFor(token) {
    const m = /^share\.([A-Za-z0-9_-]{4,16})\.([A-Za-z0-9_-]{10,64})$/.exec(String(token || '')); if (!m) return null;
    const s = get(m[1]); if (!s) return null;
    const h = sha(token), mem = Object.values(s.members).find((x) => !x.revoked && x.tokenHash === h);
    return mem ? { share: s, member: mem } : null;
  }
  /** A guest sends a prompt: it becomes a request (or runs at once when the host gave that person full permission). */
  function submit(s, mem, text0) {
    const text = clean(text0, MAX_TEXT).trim(); if (!text) return fail(400, 'Empty message.');
    const mine = Object.values(s.requests).filter((q) => q.mid === mem.id);
    if (mine.filter((q) => q.status === 'pending').length >= MAX_PENDING) return fail(429, 'You have too many prompts waiting. Wait for the host.');
    if (mine.some((q) => now() - q.ts < 1500)) return fail(429, 'Slow down a little.');
    const q = { id: rid(5), mid: mem.id, name: mem.name, text, ts: now(), status: 'pending' };
    s.requests[q.id] = q;
    const keep = Object.values(s.requests).sort((a, b) => b.ts - a.ts); for (const old of keep.slice(200)) delete s.requests[old.id];
    const g = grantOf(mem);
    if (g.auto) { q.status = 'approved'; q.finalText = text; q.mode = downgrade(g.mode); q.note = 'Runs at once: full permission'; queue.push({ shareId: s.id, reqId: q.id }); save(); pump(); return ok({ id: q.id, status: q.status, auto: true }); }
    save(); onEvent(event(s, q));
    return ok({ id: q.id, status: 'pending' });
  }
  const reqView = (q, s) => ({ id: q.id, text: q.text.slice(0, 600), finalText: q.finalText, status: q.status, ts: q.ts, edited: !!q.edited, note: q.note || '', from: q.mid, fromName: q.name, to: toOf(q), toName: toOf(q) === 'host' ? hostName : (s.members[toOf(q)] || {}).name || '?', mode: q.mode || null });
  const people = (s, exceptId) => [{ id: 'host', name: hostName }, ...live(s).map((m) => ({ id: m.id, name: m.name }))].filter((p) => p.id !== exceptId);
  const guestInfo = (s, mem) => ({ host: hostName, role: 'guest', title: s.title, me: mem.id, myName: mem.name, grant: grantOf(mem), people: people(s, mem.id),
    requests: Object.values(s.requests).filter((q) => q.mid === mem.id).sort((a, b) => b.ts - a.ts).slice(0, 8).map((q) => reqView(q, s)),
    incoming: Object.values(s.requests).filter((q) => toOf(q) === mem.id && (q.status === 'pending' || q.status === 'approved')).sort((a, b) => a.ts - b.ts).map((q) => reqView(q, s)) });
  const ownerInfo = (s) => ({ people: people(s, 'host'), requests: Object.values(s.requests).filter((q) => q.mid === 'host').sort((a, b) => b.ts - a.ts).slice(0, 8).map((q) => reqView(q, s)) });

  /** Asks a person for THEIR Claude: the host asks a friend, or a friend asks another friend. (A friend asking the host goes through submit.) */
  function ask(s, from, toId, text0) {
    const to = s.members[toId]; if (!to || to.revoked) return fail(404, 'No such person in this chat.');
    if (toId === from.id) return fail(400, 'That is your own Claude.');
    const text = clean(text0, MAX_TEXT).trim(); if (!text) return fail(400, 'Empty message.');
    const mine = Object.values(s.requests).filter((q) => q.mid === from.id);
    if (mine.filter((q) => q.status === 'pending').length >= MAX_PENDING) return fail(429, 'You have too many prompts waiting.');
    if (mine.some((q) => now() - q.ts < 1500)) return fail(429, 'Slow down a little.');
    const q = { id: rid(5), mid: from.id, name: from.name, to: toId, text, ts: now(), status: 'pending' };
    s.requests[q.id] = q; save(); onEvent(event(s, q));
    return ok({ id: q.id, status: 'pending' });
  }
  /** The person who was asked decides: approve (maybe with a changed prompt), deny, or report that their own Claude finished (done / failed). */
  function memberDecides(s, mem, reqId, body) {
    const q = s.requests[reqId]; if (!q || toOf(q) !== mem.id) return fail(404, 'No such request.');
    if (body.decision === 'done' || body.decision === 'failed') { if (q.status !== 'approved') return fail(409, 'Not running.'); q.status = body.decision; q.note = clean(body.note, 200); save(); return ok({ ok: true }); }
    if (q.status !== 'pending') return fail(409, 'Already decided.');
    if (body.decision === 'deny') { q.status = 'denied'; q.note = clean(body.note, 200); save(); onEvent({ kind: 'resolved', req: q.id, session: s.sid, forMember: mem.id }); return ok({ ok: true }); }
    if (body.decision !== 'approve') return fail(400, 'decision must be approve, deny, done or failed');
    const text = clean(body.text == null ? q.text : body.text, MAX_TEXT).trim(); if (!text) return fail(400, 'Empty prompt.');
    q.status = 'approved'; q.finalText = text; q.edited = text !== q.text; q.mode = MODES.includes(body.mode) ? body.mode : 'plan';
    save(); onEvent({ kind: 'resolved', req: q.id, session: s.sid, forMember: mem.id }); return ok({ ok: true, text, mode: q.mode, from: q.name });
  }

  // ───────── what people's OWN Claude says in the chat ─────────
  // Their prompts and answers run on their own server and account; here they are kept as extra messages that every view of the chat shows between the real ones.
  const WORK_TTL = 8 * 60e3;
  function post(s, who, body) {   // who: { id, name } (the host is id 'host')
    const role = body.role === 'assistant' ? 'assistant' : 'user', text = clean(body.text, MAX_TEXT).trim(); if (!text) return fail(400, 'Empty message.');
    let name = who.name, via = '';
    if (body.req) {   // answering a request: the prompt is the asker's, the answer comes from this person's Claude
      const q = s.requests[body.req];
      if (!q || toOf(q) !== who.id || q.status !== 'approved') return fail(403, 'That request is not yours to run.');
      if (role === 'user') name = q.name; else { name = `${who.name}'s Claude`; via = ' for ' + q.name; }
    } else if (role === 'assistant') name = `${who.name}'s Claude`;
    s.log = s.log || []; s.logSeq = (s.logSeq || 0) + 1;
    s.log.push({ n: s.logSeq, ts: now(), role, name: name + via, text, by: who.id }); if (s.log.length > 400) s.log.splice(0, s.log.length - 400);
    save(); return ok({ n: s.logSeq });
  }
  function working(s, who, on) {
    s.working = s.working || {};
    if (on) s.working[who.id] = { name: `${who.name}'s Claude`, since: now() }; else delete s.working[who.id];
    return ok({ ok: true });
  }
  const workers = (s) => { for (const [k, w] of Object.entries(s.working || {})) if (now() - w.since > WORK_TTL) delete s.working[k]; return Object.values(s.working || {}).map((w) => w.name); };
  // the note that tells the host's Claude what others did with their own Claude is for Claude, not for the people reading the chat
  const CTX = /^\[In this shared chat, others used their own Claude since your last turn:[\s\S]*?\]\n\n/;
  const clearContext = (m) => (m && m.role === 'user' && typeof m.text === 'string' && CTX.test(m.text) ? { ...m, text: m.text.replace(CTX, '') } : m);
  /** The chat as everyone sees it: the real messages, with the others' own-Claude messages in their place by time, and who is working now. */
  function merge(s, list) {
    const out = list.slice(), extra = (s.log || []).filter((e) => !list.length || e.ts >= (list[0].ts || 0) - 60000);
    for (const e of extra) {
      let at = out.length; while (at > 0 && (out[at - 1].ts || 0) > e.ts) at--;
      out.splice(at, 0, { role: e.role, text: e.text, name: e.name, ts: e.ts, own: true });
    }
    for (const w of workers(s)) out.push({ role: 'meta', text: `${w} is working…`, ts: now() });
    return out.map(clearContext);
  }
  /** What the host's own Claude has not heard yet: what others did with their own accounts. Marks it as heard. */
  function contextFor(s) {
    const seen = s.hostSeen || 0, items = (s.log || []).filter((e) => e.n > seen).slice(-30);
    if (!items.length) return '';
    s.hostSeen = items[items.length - 1].n; save();
    return '[In this shared chat, others used their own Claude since your last turn:\n' + items.map((e) => `${e.name}: ${e.text}`).join('\n') + ']\n\n';
  }
  /** Who started the run that is going on in a chat (for the Stop button of a guest): the name given when it was started. */
  return { get, bySession, ensure, view, invite, setGrant, removeMember, unshare, decide, join, guestFor, submit, guestInfo, pendingEvents, incomingEvents, hasRequest, jobDone, pump, ask, memberDecides, post, working, merge, contextFor, ownerInfo, state: () => st, save, live };
}
module.exports = { createShares, MODES };
