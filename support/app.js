'use strict';
/* The support desk: sign in with an emailed code, send requests, answer them by replying to an email. No third party, no account system. */
const crypto = require('crypto');
const fs = require('fs');
const path = require('path');
const dns = require('dns').promises;
const mail = require('./mail');

const sha = (s) => crypto.createHash('sha256').update(String(s)).digest('hex');
const EMAIL = /^[^\s@<>()[\],;:"]{1,64}@[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?)+$/;
const SESSION_MS = 90 * 86400e3, CODE_MS = 10 * 60e3;

function createApp({ dataDir, transport, from, owners = [], resolveMx = async (d) => { try { const r = await dns.resolveMx(d); if (r.length) return true; } catch { /* try A below */ } try { return (await dns.resolve4(d)).length > 0; } catch { return false; } }, now = () => Date.now() }) {
  fs.mkdirSync(dataDir, { recursive: true, mode: 0o700 });
  const file = path.join(dataDir, 'support.json');
  let db = { sessions: {}, codes: {}, tickets: {}, outbox: [] };
  try { db = { ...db, ...JSON.parse(fs.readFileSync(file, 'utf8')) }; } catch { /* first run */ }
  const save = () => { fs.writeFileSync(file + '.tmp', JSON.stringify(db), { mode: 0o600 }); fs.renameSync(file + '.tmp', file); };
  const ownerSet = new Set([from, ...owners].map((x) => String(x).toLowerCase()));
  const plus = (id) => from.replace('@', '+' + id + '@');

  // ── limits ──
  const hits = new Map();
  const limited = (key, max, ms) => { const t = now(), a = (hits.get(key) || []).filter((x) => t - x < ms); if (a.length >= max) { hits.set(key, a); return true; } a.push(t); hits.set(key, a); return false; };

  // ── mail ──
  async function send(m, to) {
    const msg = { from: `Hearth Support <${from}>`, to, subject: m.subject, html: m.html, text: m.text, replyTo: m.replyTo, headers: { 'X-Hearth-Mail': '1' } };
    try { await transport.sendMail(msg); return true; } catch (e) { db.outbox.push({ msg, at: now(), tries: 1, last: String(e.message).slice(0, 120) }); save(); return false; }
  }
  async function flushOutbox() {
    const left = [];
    for (const o of db.outbox) { try { await transport.sendMail(o.msg); } catch (e) { o.tries++; o.last = String(e.message).slice(0, 120); if (o.tries < 20 && now() - o.at < 2 * 86400e3) left.push(o); } }
    db.outbox = left; save();
  }

  // ── people ──
  async function authStart(email, ip) {
    email = String(email || '').trim().toLowerCase();
    if (!EMAIL.test(email) || email.length > 120) return { code: 400, error: 'That does not look like an email address.' };
    if (limited('ip:' + ip, 20, 3600e3) || limited('em:' + email, 5, 3600e3)) return { code: 429, error: 'Too many tries. Please wait a while and try again.' };
    if (!(await resolveMx(email.split('@')[1]))) return { code: 400, error: 'That email domain does not exist. Check the spelling.' };
    const code = String(crypto.randomInt(0, 1000000)).padStart(6, '0');
    db.codes[email] = { hash: sha(code), exp: now() + CODE_MS, tries: 0 }; save();
    const ok = await send(mail.loginCode(code), email);
    return ok ? { code: 200, body: { ok: true } } : { code: 502, error: 'We could not send the email right now. Please try again in a minute.' };
  }
  function authVerify(email, code) {
    email = String(email || '').trim().toLowerCase(); const c = db.codes[email];
    if (!c || now() > c.exp) return { code: 400, error: 'That code ran out. Ask for a new one.' };
    if (++c.tries > 5) { delete db.codes[email]; save(); return { code: 429, error: 'Too many wrong codes. Ask for a new one.' }; }
    if (c.hash !== sha(String(code || '').replace(/\s/g, ''))) { save(); return { code: 400, error: 'That code is not right.' }; }
    delete db.codes[email];
    const token = crypto.randomBytes(32).toString('hex'); db.sessions[sha(token)] = { email, exp: now() + SESSION_MS }; save();
    return { code: 200, body: { token, email } };
  }
  const who = (token) => { const s = db.sessions[sha(token || '')]; return s && now() < s.exp ? s.email : null; };

  // ── requests ──
  const brief = (t) => ({ id: t.id, title: t.title, category: t.category, status: t.status, createdAt: t.createdAt, updatedAt: t.updatedAt, last: t.messages[t.messages.length - 1].text.slice(0, 120), from: t.messages[t.messages.length - 1].from, count: t.messages.length });
  async function create(email, b) {
    const category = ['bug', 'question', 'idea', 'other'].includes(b.category) ? b.category : 'other';
    const title = String(b.title || '').trim().slice(0, 120), text = String(b.text || '').trim().slice(0, 5000);
    if (title.length < 3) return { code: 400, error: 'Please give your request a short title.' };
    if (text.length < 10) return { code: 400, error: 'Please explain a little more (at least a sentence).' };
    if (limited('new:' + email, 6, 86400e3)) return { code: 429, error: 'You sent a lot of requests today. Please wait until tomorrow, or reply to an earlier one.' };
    let id; do { id = 'HRT-' + crypto.randomInt(10000, 100000); } while (db.tickets[id]);
    const m = b.meta || {};
    const t = { id, email, category, title, status: 'open', createdAt: now(), updatedAt: now(), meta: { app: String(m.app || '').slice(0, 30), version: String(m.version || '').slice(0, 20), platform: String(m.platform || '').slice(0, 80) }, messages: [{ from: 'user', text, at: now() }] };
    db.tickets[id] = t; save();
    const sent = await Promise.all([send(mail.confirmation(t, plus(id)), email), send(mail.notifyOwner(t, plus(id)), from)]);
    return { code: 200, body: { id, mailed: sent.every(Boolean) } };
  }
  async function addUserMessage(t, text) {
    t.messages.push({ from: 'user', text, at: now() }); t.updatedAt = now(); t.status = 'open'; save();
    return send(mail.followUpOwner(t, text, plus(t.id)), from);
  }
  async function addSupportMessage(t, text) {
    t.messages.push({ from: 'support', text, at: now() }); t.updatedAt = now(); t.status = 'answered'; save();
    return send(mail.reply(t, text, plus(t.id)), t.email);
  }

  // ── mail that comes in (replies) ──
  const stripQuoted = (text) => {
    const out = []; for (const line of String(text || '').split(/\r?\n/)) {
      if (/^>/.test(line) || /^On .+ wrote:\s*$/i.test(line) || /^-{2,}\s*(Original|Forwarded)/i.test(line) || /^_{5,}/.test(line) || /^From:\s.+@/.test(line)) break; out.push(line);
    } return out.join('\n').trim().slice(0, 5000);
  };
  /** { from, to: [addresses], subject, text, authOk } */
  async function inbound(m) {
    const hay = [...(m.to || []), m.subject || ''].join(' '), id = (/HRT-\d{5}/.exec(hay) || [])[0], t = id && db.tickets[id];
    if (!t) return { handled: false };
    const text = stripQuoted(m.text); if (text.length < 1) return { handled: false };
    const sender = String(m.from || '').toLowerCase();
    if (ownerSet.has(sender)) { if (!m.authOk) return { handled: false, why: 'unverified sender' }; await addSupportMessage(t, text); return { handled: true, kind: 'reply' }; }
    if (sender === t.email) { await addUserMessage(t, text); return { handled: true, kind: 'follow-up' }; }
    return { handled: false, why: 'unknown sender' };
  }

  // ── HTTP ──
  const readJson = (req) => new Promise((res) => { let b = ''; req.on('data', (d) => { b += d; if (b.length > 40000) req.destroy(); }); req.on('end', () => { try { res(JSON.parse(b || '{}')); } catch { res({}); } }); });
  async function handle(req, res) {
    const send = (code, obj) => { const b = Buffer.from(JSON.stringify(obj)); res.writeHead(code, { 'Content-Type': 'application/json', 'Content-Length': b.length, 'Cache-Control': 'no-store' }); res.end(b); };
    const ip = String(req.headers['x-forwarded-for'] || req.socket.remoteAddress || '').split(',')[0].trim();
    const u = new URL(req.url, 'http://x'), parts = u.pathname.replace(/^\/|\/$/g, '').split('/');
    const out = (r) => (r.error ? send(r.code, { error: r.error }) : send(r.code, r.body));
    try {
      if (req.method === 'GET' && parts[0] === 'health') return send(200, { ok: true });
      if (req.method === 'POST' && parts[0] === 'auth' && parts[1] === 'start') return out(await authStart((await readJson(req)).email, ip));
      if (req.method === 'POST' && parts[0] === 'auth' && parts[1] === 'verify') { const b = await readJson(req); if (limited('v:' + ip, 30, 3600e3)) return send(429, { error: 'Too many tries.' }); return out(authVerify(b.email, b.code)); }
      const token = String(req.headers.authorization || '').replace(/^Bearer /, ''), email = who(token);
      if (!email) return send(401, { error: 'Please sign in again.' });
      if (req.method === 'GET' && parts[0] === 'me') return send(200, { email });
      if (req.method === 'POST' && parts[0] === 'auth' && parts[1] === 'logout') { delete db.sessions[sha(token)]; save(); return send(200, { ok: true }); }
      if (parts[0] === 'tickets') {
        if (req.method === 'GET' && !parts[1]) return send(200, Object.values(db.tickets).filter((t) => t.email === email).sort((a, b) => b.updatedAt - a.updatedAt).map(brief));
        if (req.method === 'POST' && !parts[1]) return out(await create(email, await readJson(req)));
        const t = db.tickets[parts[1]]; if (!t || t.email !== email) return send(404, { error: 'No such request.' });
        if (req.method === 'GET' && !parts[2]) return send(200, { ...brief(t), messages: t.messages });
        if (req.method === 'POST' && parts[2] === 'messages') {
          const text = String((await readJson(req)).text || '').trim().slice(0, 5000);
          if (text.length < 2) return send(400, { error: 'Write a message first.' });
          if (limited('msg:' + email, 30, 86400e3)) return send(429, { error: 'Too many messages today.' });
          await addUserMessage(t, text); return send(200, { ok: true });
        }
      }
      return send(404, { error: 'unknown' });
    } catch (e) { return send(500, { error: 'Something went wrong on our side.' }); }
  }
  return { handle, inbound, flushOutbox, db: () => db, plus, stripQuoted, addSupportMessage };
}
module.exports = { createApp };
