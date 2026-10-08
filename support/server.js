#!/usr/bin/env node
'use strict';
/* Runs the support desk. Settings come from the environment (see install.sh):
   SUPPORT_USER (the mailbox), SUPPORT_PASS (its app password), SUPPORT_OWNERS (extra addresses that may answer), SUPPORT_PORT, SUPPORT_DATA */
const http = require('http');
const path = require('path');
const nodemailer = require('nodemailer');
const { createApp } = require('./app');

const user = process.env.SUPPORT_USER, pass = process.env.SUPPORT_PASS;
if (!user || !pass) { console.error('SUPPORT_USER and SUPPORT_PASS are not set. Run install.sh first.'); process.exit(1); }
const transport = nodemailer.createTransport({ host: 'smtp.gmail.com', port: 465, secure: true, auth: { user, pass } });
const app = createApp({ dataDir: process.env.SUPPORT_DATA || path.join(__dirname, 'data'), transport, from: user, owners: (process.env.SUPPORT_OWNERS || '').split(',').map((s) => s.trim()).filter(Boolean) });
const port = Number(process.env.SUPPORT_PORT || 47610);
http.createServer((req, res) => app.handle(req, res)).listen(port, '127.0.0.1', () => console.log('Support desk on 127.0.0.1:' + port));

// answers come back as emails: look at the mailbox now and then.
// Messages are followed by number (UID), not by "unread": reading a mail in Gmail must never make us miss it.
const { ImapFlow } = require('imapflow');
const { simpleParser } = require('mailparser');
let busy = false;
const log = (...a) => console.log(new Date().toISOString().slice(11, 19), ...a);
async function poll() {
  if (busy) return; busy = true;
  const db = app.db(); db.mail = db.mail || { lastUid: 0, ids: [] };
  const c = new ImapFlow({ host: 'imap.gmail.com', port: 993, secure: true, auth: { user, pass }, logger: false, connectionTimeout: 20000, greetingTimeout: 20000, socketTimeout: 45000 });
  c.on('error', (e) => log('mailbox connection problem:', e.message));   // without this a timeout would stop the whole program
  try {
    await c.connect(); const lock = await c.getMailboxLock('INBOX');
    try {
      const top = c.mailbox.uidNext - 1;
      if (db.mail.lastUid === 0 && top > 0) db.mail.lastUid = Math.max(0, top - 200);   // the first time: look at the newest 200 messages
      if (top > db.mail.lastUid) {
        const found = [];
        for await (const msg of c.fetch(`${db.mail.lastUid + 1}:*`, { source: true, uid: true }, { uid: true })) if (msg.uid > db.mail.lastUid) found.push(msg);
        for (const msg of found.sort((x, y) => x.uid - y.uid)) {
          try {
            const m = await simpleParser(msg.source), mid = String(m.messageId || msg.uid);
            if (!db.mail.ids.includes(mid) && !m.headers.get('x-hearth-mail')) {
              const arH = m.headers.get('authentication-results'), auth = arH ? (typeof arH === 'string' ? arH : JSON.stringify(arH)) : '';
              // a mail that Gmail did not receive from outside (we answered from the mailbox itself) has no such header; anything from outside always has one, and must pass
              const r = await app.inbound({ from: (m.from && m.from.value[0] && m.from.value[0].address) || '', to: (m.to ? m.to.value : []).map((x) => x.address).concat(m.headers.get('delivered-to') ? [String(m.headers.get('delivered-to'))] : []), subject: m.subject || '', text: m.text || '', authOk: !arH || (/(dkim|spf)=pass/i.test(auth) && !/dkim=fail/i.test(auth)) });
              if (r.handled) log('handled a', r.kind, 'from the mailbox:', m.subject); else if (r.why) log('ignored a mail:', r.why, '-', m.subject);
              if (r.handled || !r.why) db.mail.ids = [...db.mail.ids, mid].slice(-500);
            }
          } catch (e) { log('could not read mail', msg.uid, e.message); }
          db.mail.lastUid = Math.max(db.mail.lastUid, msg.uid); app.save();
        }
      }
      db.mail.lastUid = Math.max(db.mail.lastUid, top); app.save();
    } finally { lock.release(); }
    await c.logout();
  } catch (e) { log('mailbox check failed:', e.message); try { c.close(); } catch { /* gone */ } }
  busy = false;
}
setInterval(poll, 30000); setTimeout(poll, 3000);
setInterval(() => app.flushOutbox().catch((e) => log('outbox:', e.message)), 60000);
process.on('uncaughtException', (e) => log('problem (kept running):', e.message));
