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

// answers come back as emails: look at the mailbox now and then
const { ImapFlow } = require('imapflow');
const { simpleParser } = require('mailparser');
let busy = false;
async function poll() {
  if (busy) return; busy = true;
  const c = new ImapFlow({ host: 'imap.gmail.com', port: 993, secure: true, auth: { user, pass }, logger: false });
  try {
    await c.connect(); const lock = await c.getMailboxLock('INBOX');
    try {
      for await (const msg of c.fetch({ seen: false }, { source: true, uid: true })) {
        const m = await simpleParser(msg.source);
        const auth = String(m.headers.get('authentication-results') || '');
        if (!m.headers.get('x-hearth-mail')) {
          const r = await app.inbound({ from: (m.from && m.from.value[0] && m.from.value[0].address) || '', to: (m.to ? m.to.value : []).map((x) => x.address).concat((m.headers.get('delivered-to') ? [String(m.headers.get('delivered-to'))] : [])), subject: m.subject || '', text: m.text || '', authOk: /(dkim|spf)=pass/i.test(auth) });
          if (!r.handled && r.why) console.error('ignored a mail:', r.why);
        }
        await c.messageFlagsAdd({ uid: msg.uid }, ['\\Seen'], { uid: true });
      }
    } finally { lock.release(); }
    await c.logout();
  } catch (e) { console.error('mailbox check failed:', e.message); try { await c.close(); } catch { /* gone */ } }
  busy = false;
}
setInterval(poll, 30000); setTimeout(poll, 3000);
setInterval(() => app.flushOutbox().catch(() => {}), 60000);
