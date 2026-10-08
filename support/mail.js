'use strict';
/* Every email Hearth Support sends: same warm look as the app (ember orange, soft cream, serif headings). Plain-text versions too. */
const LOGO = 'https://raw.githubusercontent.com/veelvoer/hearth/main/branding/hearth-mail.png';
const esc = (s) => String(s == null ? '' : s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const nl2br = (s) => esc(s).replace(/\r?\n/g, '<br>');
const CAT = { bug: 'Bug', question: 'Question', idea: 'Idea', other: 'Other' };

function layout({ title, preheader = '', body, ticket }) {
  return `<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="color-scheme" content="light dark"><title>${esc(title)}</title></head>
<body style="margin:0;padding:0;background:#F4F1EC;font-family:-apple-system,'Segoe UI',Roboto,Helvetica,Arial,sans-serif;color:#2B2A28;">
<span style="display:none;max-height:0;overflow:hidden;opacity:0;">${esc(preheader)}</span>
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background:#F4F1EC;"><tr><td align="center" style="padding:28px 12px;">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="max-width:560px;">
  <tr><td style="background:#F0643C;border-radius:18px 18px 0 0;padding:22px 26px;">
    <table role="presentation" width="100%" cellpadding="0" cellspacing="0"><tr>
      <td width="52" style="padding-right:12px;"><img src="${LOGO}" width="40" height="40" alt="Hearth" style="display:block;border-radius:10px;border:0;"></td>
      <td style="font-family:Georgia,'Times New Roman',serif;font-size:24px;color:#ffffff;">Hearth <span style="opacity:.85;font-size:16px;">Support</span></td>
      ${ticket ? `<td align="right" style="font-size:13px;color:#ffffff;white-space:nowrap;"><span style="background:rgba(255,255,255,.22);border-radius:99px;padding:5px 12px;">${esc(ticket)}</span></td>` : ''}
    </tr></table>
  </td></tr>
  <tr><td style="background:#FFFFFF;border:1px solid #E6E1D8;border-top:0;border-radius:0 0 18px 18px;padding:28px 26px;font-size:15px;line-height:1.6;">
    <h1 style="margin:0 0 14px;font-family:Georgia,'Times New Roman',serif;font-weight:normal;font-size:24px;color:#2B2A28;">${esc(title)}</h1>
    ${body}
  </td></tr>
  <tr><td style="padding:18px 8px;text-align:center;font-size:12px;color:#8A857B;line-height:1.5;">
    Hearth is an independent app, not made by or affiliated with Anthropic.<br>
    <a href="https://github.com/veelvoer/hearth" style="color:#C4491E;text-decoration:none;">github.com/veelvoer/hearth</a>
  </td></tr>
</table></td></tr></table></body></html>`;
}
const quote = (text) => `<div style="margin:14px 0;padding:12px 16px;background:#FAF8F4;border-left:4px solid #F0643C;border-radius:8px;color:#4A4741;">${nl2br(text)}</div>`;
const row = (k, v) => `<tr><td style="padding:4px 14px 4px 0;color:#8A857B;white-space:nowrap;vertical-align:top;">${esc(k)}</td><td style="padding:4px 0;color:#2B2A28;">${esc(v)}</td></tr>`;
const table = (rows) => `<table role="presentation" cellpadding="0" cellspacing="0" style="font-size:14px;margin:8px 0 4px;">${rows.join('')}</table>`;

/** The six-digit sign-in code. */
function loginCode(code) {
  const pretty = code.replace(/(\d{3})(\d{3})/, '$1 $2');
  return {
    subject: `Your Hearth support code: ${pretty}`,
    html: layout({ title: 'Your sign-in code', preheader: `Your code is ${pretty}`, body: `<p style="margin:0 0 12px;">Type this code in Hearth to sign in to support:</p>
      <div style="text-align:center;margin:18px 0;"><span style="display:inline-block;font:600 34px 'SFMono-Regular',Consolas,monospace;letter-spacing:.2em;background:#FAF8F4;border:1px solid #E6E1D8;border-radius:14px;padding:14px 22px;color:#C4491E;">${esc(pretty)}</span></div>
      <p style="margin:0;color:#8A857B;font-size:13px;">The code works for 10 minutes. If you did not ask for it, you can ignore this email. Nobody can use your account without the code.</p>` }),
    text: `Your Hearth support code: ${pretty}\n\nIt works for 10 minutes. If you did not ask for it, ignore this email.\n`,
  };
}

/** To the person who wrote in: we got it. */
function confirmation(t, replyTo) {
  const first = t.messages[0];
  return {
    subject: `We got your request ${t.id}: ${t.title}`, replyTo,
    html: layout({ title: 'We got your request', ticket: t.id, preheader: `Thank you. Your request ${t.id} is with us.`, body: `
      <p style="margin:0 0 12px;">Thank you for writing to us. Your request is saved and a real person will read it.</p>
      ${table([row('Request', t.id), row('Type', CAT[t.category] || 'Other'), row('Subject', t.title)])}
      <p style="margin:16px 0 4px;color:#8A857B;font-size:13px;">What you wrote</p>${quote(first.text)}
      <h2 style="margin:22px 0 8px;font-family:Georgia,'Times New Roman',serif;font-weight:normal;font-size:18px;">What happens next</h2>
      <ol style="margin:0;padding-left:20px;"><li>We read your request and reply by email.</li><li>You can answer that email, or write in the Hearth app (Settings → Support), to add more details.</li><li>Keep this email: the number <b>${esc(t.id)}</b> is how we find your request.</li></ol>
      <p style="margin:18px 0 0;color:#8A857B;font-size:13px;">Tip: the quickest answers come when you tell us what you did, what you expected and what happened instead.</p>` }),
    text: `We got your request ${t.id}\n\nType: ${CAT[t.category] || 'Other'}\nSubject: ${t.title}\n\nWhat you wrote:\n${first.text}\n\nWhat happens next:\n1. We read it and reply by email.\n2. You can reply to this email, or write in the Hearth app (Settings > Support), to add details.\n3. Keep the number ${t.id}.\n`,
  };
}

/** To the owner of the support mailbox: a new request. Reply to this email to answer. */
function notifyOwner(t, replyTo) {
  const first = t.messages[0], m = t.meta || {};
  return {
    subject: `[${t.id}] ${CAT[t.category] || 'Other'}: ${t.title}`, replyTo,
    html: layout({ title: t.title, ticket: t.id, preheader: `New ${CAT[t.category] || 'request'} from ${t.email}`, body: `
      ${table([row('From', t.email), row('Type', CAT[t.category] || 'Other'), row('App', [m.app, m.version].filter(Boolean).join(' ') || '–'), row('Device', m.platform || '–')])}
      <p style="margin:16px 0 4px;color:#8A857B;font-size:13px;">Message</p>${quote(first.text)}
      <p style="margin:18px 0 0;padding:12px 16px;background:#FFF3EC;border-radius:10px;color:#8A3A18;font-size:14px;"><b>Just reply to this email.</b> Your answer is sent to ${esc(t.email)} with Hearth's layout. Start your reply above the quoted text.</p>` }),
    text: `${t.id} from ${t.email}\nType: ${CAT[t.category] || 'Other'}\nApp: ${[m.app, m.version].filter(Boolean).join(' ')}\nDevice: ${m.platform || ''}\n\n${first.text}\n\nReply to this email to answer.`,
  };
}

/** To the owner: the person added something to an existing request. */
function followUpOwner(t, text, replyTo) {
  return {
    subject: `[${t.id}] ${t.title} (new message)`, replyTo,
    html: layout({ title: 'New message on ' + t.id, ticket: t.id, preheader: text.slice(0, 80), body: `${table([row('From', t.email), row('Subject', t.title)])}${quote(text)}<p style="margin:18px 0 0;color:#8A857B;font-size:13px;">Reply to this email to answer.</p>` }),
    text: `New message on ${t.id} from ${t.email}:\n\n${text}\n\nReply to this email to answer.`,
  };
}

/** To the person: the answer. */
function reply(t, text, replyTo) {
  return {
    subject: `Re: [${t.id}] ${t.title}`, replyTo,
    html: layout({ title: 'We answered your request', ticket: t.id, preheader: text.slice(0, 90), body: `
      <p style="margin:0 0 6px;color:#8A857B;font-size:13px;">Hearth Support wrote:</p>
      <div style="margin:0 0 16px;padding:14px 18px;background:#FFF3EC;border-left:4px solid #F0643C;border-radius:8px;color:#2B2A28;">${nl2br(text)}</div>
      <p style="margin:0;color:#8A857B;font-size:13px;">Not solved yet? Just reply to this email, or write in the Hearth app (Settings → Support). Your request number is <b>${esc(t.id)}</b>.</p>` }),
    text: `Hearth Support wrote:\n\n${text}\n\nNot solved yet? Reply to this email. Your request number is ${t.id}.\n`,
  };
}
module.exports = { loginCode, confirmation, notifyOwner, followUpOwner, reply, esc, CAT };
