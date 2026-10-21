#!/usr/bin/env node
'use strict';
/* Connects Hearth's support mail list to your Google Tasks. Run once on a computer with a browser:
     node scripts/google-tasks-token.js <client-id> <client-secret> [server-address server-token]
   1. In Google Cloud Console make a project, turn on "Google Tasks API", and create an OAuth client of type "Desktop app".
   2. Run this script, open the address it prints, allow access.
   3. With a server address and token it hands the result to your Hearth server; without, it prints the settings to paste into Hearth. */
const http = require('http');
const [clientId, clientSecret, server, token] = process.argv.slice(2);
if (!clientId || !clientSecret) { console.error('Usage: node scripts/google-tasks-token.js <client-id> <client-secret> [server-address server-token]'); process.exit(1); }
const srv = http.createServer(async (req, res) => {
  const u = new URL(req.url, 'http://127.0.0.1'); const code = u.searchParams.get('code');
  if (!code) { res.end('Waiting for Google…'); return; }
  try {
    const redirect = 'http://127.0.0.1:' + srv.address().port;
    const r = await fetch('https://oauth2.googleapis.com/token', { method: 'POST', headers: { 'Content-Type': 'application/x-www-form-urlencoded' }, body: new URLSearchParams({ code, client_id: clientId, client_secret: clientSecret, redirect_uri: redirect, grant_type: 'authorization_code' }) });
    const j = await r.json(); if (!j.refresh_token) throw new Error(j.error_description || j.error || 'Google gave no refresh token');
    const body = { googleTasks: { clientId, clientSecret, refreshToken: j.refresh_token } };
    if (server && token) {
      const p = await fetch(server.replace(/\/$/, '') + '/admin/options', { method: 'POST', headers: { 'Content-Type': 'application/json', Authorization: 'Bearer ' + token }, body: JSON.stringify(body) });
      if (!p.ok) throw new Error('Your Hearth server said ' + p.status);
      console.log('Done: your Hearth server can now add support mails to Google Tasks.');
    } else console.log('Send this to your Hearth server (POST /admin/options):\n' + JSON.stringify(body));
    res.end('Done. You can close this tab.');
  } catch (e) { console.error('Failed:', e.message); res.end('Failed: ' + e.message); }
  srv.close();
});
srv.listen(0, '127.0.0.1', () => {
  const redirect = 'http://127.0.0.1:' + srv.address().port;
  console.log('Open this address in your browser and allow access:\n' + 'https://accounts.google.com/o/oauth2/v2/auth?' + new URLSearchParams({ client_id: clientId, redirect_uri: redirect, response_type: 'code', scope: 'https://www.googleapis.com/auth/tasks', access_type: 'offline', prompt: 'consent' }));
});
