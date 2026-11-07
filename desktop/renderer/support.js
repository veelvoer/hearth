'use strict';
/* Settings → Support: sign in with an emailed code, send a request, read the answers. */
const SP = { el: null, view: 'home', email: '', signedIn: false, meta: {}, tickets: [], open: null, timer: null };
const CATS = [['bug', 'Bug', 'Something does not work', 'What happened? What did you expect? What did you do just before it went wrong?'], ['question', 'Question', 'I need help using Hearth', 'What do you want to do? Where are you stuck?'], ['idea', 'Idea', 'I wish Hearth could…', 'What would you like Hearth to do, and why would it help you?'], ['other', 'Other', 'Anything else', 'Tell us what is on your mind.']];
const spAgo = (t) => { const s = Math.round((Date.now() - t) / 1000); return s < 60 ? 'just now' : s < 3600 ? Math.round(s / 60) + ' min ago' : s < 86400 ? Math.round(s / 3600) + ' h ago' : new Date(t).toLocaleDateString(); };

async function openSupport(view) {
  if (!SP.el) { SP.el = h('div', { class: 'modal', onclick: (e) => { if (e.target === SP.el) closeSupport(); } }); document.body.append(SP.el); }
  const st = await cm.supportState().catch(() => ({}));
  SP.signedIn = !!st.signedIn; SP.email = st.email || ''; SP.meta = st.meta || {};
  SP.view = view || (SP.signedIn ? 'home' : 'login');
  drawSupport();
}
function closeSupport() { clearInterval(SP.timer); if (SP.el) SP.el.remove(); SP.el = null; }

function drawSupport() {
  if (!SP.el) return;
  clearInterval(SP.timer);
  const card = h('div', { class: 'mcard sup' }), body = h('div', { class: 'mbody' });
  const head = h('div', { class: 'row' }, h('div', { class: 'grow row' }, h('div', { html: sparkSVG(22) }), h('h3', { style: 'margin:0' }, 'Support')), SP.signedIn ? h('button', { class: 'link', onclick: async () => { await cm.supportLogout(); SP.signedIn = false; SP.view = 'login'; drawSupport(); } }, 'Sign out') : null, ibtn('close', 'Close', closeSupport, 'ghost sm'));
  card.append(head, body); SP.el.innerHTML = ''; SP.el.append(card);
  ({ login: viewLogin, home: viewHome, new: viewNew, thread: viewThread, sent: viewSent })[SP.view](body);
}

function viewLogin(body) {
  let step = 'email', email = '';
  const draw = () => {
    body.innerHTML = '';
    body.append(SP.notice ? h('p', { class: 'err small' }, SP.notice) : null, h('p', { class: 'muted' }, 'Something not working, or an idea? Write to us here. We answer by email.'),
      h('div', { class: 'obnote' }, h('b', {}, step === 'email' ? 'First, tell us your email' : 'Check your email'),
        h('div', { class: 'small muted' }, step === 'email' ? 'We send you a 6-digit code, so we know the answers reach you. It works with any real email address, for example Gmail. No password.' : 'We sent a code to ' + email + '. It can take a minute. Look in spam if you do not see it.')));
    const msg = h('div', { class: 'small' });
    if (step === 'email') {
      const inp = h('input', { type: 'email', placeholder: 'you@example.com', autofocus: true, value: email });
      const go = async () => { msg.className = 'small'; msg.textContent = 'Sending…'; try { email = inp.value.trim(); await cm.supportStart(email); step = 'code'; draw(); } catch (e) { msg.className = 'small err'; msg.textContent = clean(e); } };
      inp.onkeydown = (e) => { if (e.key === 'Enter') go(); };
      body.append(inp, h('button', { class: 'primary', onclick: go }, 'Send me a code'), msg); setTimeout(() => inp.focus(), 0);
    } else {
      const inp = h('input', { placeholder: '6-digit code', inputmode: 'numeric', maxlength: 7, style: 'font:600 22px ui-monospace,monospace;letter-spacing:.2em;text-align:center' });
      const go = async () => { msg.className = 'small'; msg.textContent = 'Checking…'; try { const r = await cm.supportVerify(email, inp.value); SP.email = r.email; SP.signedIn = true; SP.view = SP.after || 'home'; SP.after = null; drawSupport(); } catch (e) { msg.className = 'small err'; msg.textContent = clean(e); } };
      inp.onkeydown = (e) => { if (e.key === 'Enter') go(); };
      body.append(inp, h('button', { class: 'primary', onclick: go }, 'Sign in'), msg, h('button', { class: 'link', onclick: () => { step = 'email'; draw(); } }, 'Use another email')); setTimeout(() => inp.focus(), 0);
    }
  };
  draw();
}

/** open / listed / answered / fixed: what the desk says, or for an older desk the wording of its last automatic mail. */
const spStatus = (t) => (t.status === 'fixed' || (t.from === 'support' && /^Good news: what you told us about/.test(t.last || '')) ? 'fixed'
  : t.status === 'listed' || (t.from === 'support' && /^Thank you for your message about.*added it to our list/.test(t.last || '')) ? 'listed' : t.status === 'answered' ? 'answered' : 'open');
const SP_LABEL = { open: 'Open', listed: 'On the list', answered: 'Answered', fixed: 'Fixed' };
/** A small file picker: up to three files of at most 1.5 MB, read as base64 for the desk. */
function spAttach() {
  const files = [], list = h('div', { class: 'stack', style: 'gap:4px' }), note = h('div', { class: 'small err' });
  const input = h('input', { type: 'file', multiple: true, style: 'display:none' });
  const draw = () => { list.innerHTML = ''; files.forEach((f) => list.append(h('div', { class: 'row small' }, h('span', { class: 'grow ellip' }, '📎 ' + f.name), h('button', { class: 'link', 'aria-label': 'Remove ' + f.name, onclick: () => { files.splice(files.indexOf(f), 1); draw(); } }, 'Remove')))); btn.style.display = files.length >= 3 ? 'none' : ''; };
  const btn = h('button', { class: 'ghost sm', onclick: () => input.click() }, 'Attach a file');
  input.onchange = async () => {
    note.textContent = '';
    for (const f of [...input.files]) {
      if (files.length >= 3) { note.textContent = 'You can attach up to 3 files.'; break; }
      if (f.size > 1.5 * 1024 * 1024) { note.textContent = '“' + f.name + '” is bigger than 1.5 MB. Send a smaller file, or a screenshot.'; continue; }
      const data = await new Promise((res) => { const r = new FileReader(); r.onload = () => res(String(r.result).split(',')[1] || ''); r.readAsDataURL(f); });
      files.push({ name: f.name, type: f.type || 'application/octet-stream', data });
    }
    input.value = ''; draw();
  };
  return { el: h('div', { class: 'stack', style: 'gap:6px' }, list, h('div', { class: 'row' }, btn, input), note), files };
}

/** The owner's checklist of what people reported. Tick it when it is done; done ones go below and can be removed. */
let SP_DONE_OPEN = false;
async function spChecklist(box, hub, body) {
  const draw = (items) => {
    box.innerHTML = '';
    const todo = items.filter((x) => !x.done), done = items.filter((x) => x.done);
    box.append(h('div', { class: 'small muted', style: 'margin:6px 4px 2px;text-transform:uppercase;letter-spacing:.06em' }, 'To do · ' + todo.length));
    if (!todo.length) box.append(h('div', { class: 'obnote' }, h('b', {}, 'Nothing left to do'), h('div', { class: 'small muted' }, 'Every report is done. New ones show up here by themselves.')));
    const row = (x) => h('div', { class: 'spwrap' }, h('div', { class: 'row', style: 'align-items:flex-start;gap:10px' },
      h('input', { type: 'checkbox', checked: x.done, 'aria-label': (x.done ? 'Mark not done: ' : 'Mark done: ') + x.title, style: 'margin-top:4px;width:18px;height:18px', onchange: (e) => act({ id: x.id, done: e.target.checked }) }),
      h('button', { class: 'spitem grow', style: 'text-align:left', onclick: () => { SP.open = x.id; SP.view = 'thread'; drawSupport(); } }, h('b', { class: 'ellip', style: x.done ? 'text-decoration:line-through;opacity:.6' : '' }, x.title), h('div', { class: 'small muted ellip' }, x.from + ' · ' + x.id + ' · ' + spAgo(x.at))),
      x.done ? null : h('button', { class: 'ghost sm', title: 'Let Claude look into this', onclick: async (e) => { e.currentTarget.disabled = true; try { const r = await cm.relay(hub.id, 'POST', '/inbox/act', { id: x.id, title: x.title, from: x.from, text: x.text, action: 'claude' }); toast(r.text || 'Claude is looking into it.'); if (r.ok && r.session) { closeSupport(); setTab('sessions'); openSession({ id: r.session, title: x.title, cwd: r.cwd || '', mid: hub.id, mtime: Date.now(), live: true }); } } catch (er) { toast(clean(er)); } } }, 'Fix with Claude')));
    todo.forEach((x) => box.append(row(x)));
    if (done.length) {
      box.append(h('div', { class: 'row', style: 'margin-top:8px' }, h('button', { class: 'link grow', style: 'text-align:left', onclick: () => { SP_DONE_OPEN = !SP_DONE_OPEN; draw(items); } }, (SP_DONE_OPEN ? '⌃ ' : '⌄ ') + 'Done · ' + done.length), SP_DONE_OPEN ? h('button', { class: 'ghost sm', onclick: () => act({ remove: 'done' }) }, 'Remove done') : null));
      if (SP_DONE_OPEN) done.forEach((x) => box.append(row(x)));
    }
  };
  const act = async (b) => {
    try { draw((await cm.relay(hub.id, 'POST', '/inbox/checklist', b)).items || []); }
    catch (e) {   // the server could not give the checklist: say why, and still show the open reports so nothing is hidden
      box.innerHTML = ''; const m = clean(e);
      box.append(h('div', { class: 'obnote' }, h('div', { class: 'small err' }, /unknown|404|not found/i.test(m) ? 'Your server does not know the checklist yet. Update it in Settings, Updates, and this fills in by itself.' : m)));
      (body.sync || []).filter((x) => !x.fixed).forEach((x) => box.append(h('div', { class: 'spwrap' }, h('button', { class: 'spitem', onclick: () => { SP.open = x.id; SP.view = 'thread'; drawSupport(); } }, h('b', { class: 'ellip' }, x.title), h('div', { class: 'small muted ellip' }, x.email + ' · ' + x.id + ' · ' + spAgo(x.updatedAt))))));
    }
  };
  await act(body);
}

async function viewHome(body) {
  body.append(h('div', { class: 'row' }, h('div', { class: 'grow small muted' }, 'Signed in as ' + SP.email), h('button', { class: 'primary sm', onclick: () => { SP.view = 'new'; drawSupport(); } }, 'New request')), h('div', { class: 'flist', id: 'sp-list' }, h('p', { class: 'muted small' }, 'Loading…')));
  const load = async () => {
    let list; try { list = await cm.support('GET', '/tickets'); } catch (e) { if (!SP.signedIn || !SP.el) return; if (/sign in again/i.test(clean(e))) { SP.signedIn = false; SP.view = 'login'; SP.notice = 'You were signed out. Sign in again with a code by email.'; drawSupport(); return; } const l = $('#sp-list'); if (l) { l.innerHTML = ''; l.append(h('p', { class: 'err small' }, clean(e))); } return; }
    SP.tickets = list; const box = $('#sp-list'); if (!box) return; box.innerHTML = '';
    if (!list.length) box.append(h('div', { class: 'obnote' }, h('b', {}, 'No requests yet'), h('div', { class: 'small muted' }, 'Press “New request” and tell us what is going on. We will answer by email, and you can read the answer here too.')));
    const owner = list.some((t) => t.theirs), hub = owner && typeof SS !== 'undefined' ? (SS.machines.find((m) => m.secure) || SS.machines[0]) : null;
    if (owner && hub) {
      const ck = h('div', { class: 'stack', id: 'sp-check' }); box.append(ck);
      spChecklist(ck, hub, { sync: list.filter((t) => t.theirs).map((t) => ({ id: t.id, title: t.title, email: t.email, last: t.last, text: t.thread || t.last, updatedAt: t.updatedAt, from: t.from, fixed: spStatus(t) === 'fixed' })) });
    }
    const mine = hub ? list.filter((t) => !t.theirs) : list;
    if (hub && mine.length) box.append(h('div', { class: 'small muted', style: 'margin:14px 4px 2px;text-transform:uppercase;letter-spacing:.06em' }, 'Your own requests'));
    const groups = {}; mine.forEach((t) => { (groups[spStatus(t)] = groups[spStatus(t)] || []).push(t); });
    const heads = Object.keys(groups).length > 1;
    ['open', 'listed', 'answered', 'fixed'].forEach((st) => {
      if (!groups[st]) return;
      if (heads) box.append(h('div', { class: 'small muted', style: 'margin:10px 4px 2px;text-transform:uppercase;letter-spacing:.06em' }, SP_LABEL[st] + ' · ' + groups[st].length));
      groups[st].forEach((t) => box.append(h('div', { class: 'spwrap' }, h('button', { class: 'spitem', onclick: () => { SP.open = t.id; SP.view = 'thread'; drawSupport(); } }, h('div', { class: 'row' }, h('b', { class: 'grow ellip' }, t.title), h('span', { class: 'pill ' + (st === 'open' || st === 'listed' ? 'warn' : 'ok'), style: 'padding:2px 9px' }, h('i'), (st === 'fixed' ? '✓ ' : '') + SP_LABEL[st])),
        h('div', { class: 'small muted ellip' }, (t.theirs ? t.email + ' · ' : '') + (t.from === 'support' ? 'Hearth Support: ' : t.theirs ? 'They: ' : 'You: ') + t.last), h('div', { class: 'small muted' }, t.id + ' · ' + spAgo(t.updatedAt))), t.theirs && st !== 'fixed' ? spActions(t, t.thread || t.last) : null)));
    });
  };
  load(); SP.timer = setInterval(load, 20000);
}

function viewNew(body) {
  let cat = 'bug';
  const title = h('input', { placeholder: 'A few words, e.g. “The app closes when I open it”', maxlength: 120 }), text = h('textarea', { rows: 7 }), info = h('input', { type: 'checkbox', checked: true }), msg = h('div', { class: 'small' });
  const chips = h('div', { class: 'row wrap' }), hint = h('div', { class: 'small muted' });
  const redraw = () => { chips.innerHTML = ''; CATS.forEach(([k, l]) => chips.append(chip(l, cat === k, () => { cat = k; redraw(); }))); const c = CATS.find((x) => x[0] === cat); text.placeholder = c[3]; hint.textContent = c[2]; };
  redraw();
  const send = async (btn) => {
    msg.className = 'small'; msg.textContent = 'Sending…'; btn.disabled = true;
    try { const r = await cm.support('POST', '/tickets', { category: cat, title: title.value, text: text.value, meta: info.checked ? SP.meta : {}, files: att.files }); SP.sent = r; SP.view = 'sent'; drawSupport(); }
    catch (e) { msg.className = 'small err'; msg.textContent = clean(e); btn.disabled = false; }
  };
  const att = spAttach();
  const go = h('button', { class: 'primary', onclick: () => send(go) }, 'Send request');
  body.append(h('button', { class: 'link', onclick: () => { SP.view = 'home'; drawSupport(); } }, '‹ Back'), h('div', { class: 'small muted' }, 'What is it about?'), chips, hint, title, text, att.el,
    h('label', { class: 'updrow' }, info, h('span', { class: 'small' }, 'Include app info (version and system, so we can find the problem faster)')),
    h('div', { class: 'small muted' }, 'We use your email only to answer you. Never put passwords or secret keys in your message.'), go, msg);
  setTimeout(() => title.focus(), 0);
}

function viewSent(body) {
  const r = SP.sent || {};
  body.append(h('div', { class: 'obnote good' }, h('b', {}, '✓ Request sent'), h('div', {}, 'Your number is ' + r.id + '.'), h('div', { class: 'small muted' }, 'We just emailed you a confirmation to ' + SP.email + '. When we answer, the answer arrives in your email, and you can read it here too.' + (r.mailed === false ? ' (The confirmation email is delayed; it will follow.)' : ''))),
    h('button', { class: 'primary', onclick: () => { SP.view = 'home'; drawSupport(); } }, 'Done'));
}

async function viewThread(body) {
  body.append(h('button', { class: 'link', onclick: () => { SP.view = 'home'; drawSupport(); } }, '‹ All requests'), h('div', { id: 'sp-thread', class: 'spthread' }, h('p', { class: 'muted small' }, 'Loading…')));
  const reply = h('textarea', { rows: 3, placeholder: 'Write a reply…' }), msg = h('div', { class: 'small' });
  const send = h('button', { class: 'primary sm', onclick: async () => { msg.textContent = ''; send.disabled = true; try { await cm.support('POST', '/tickets/' + SP.open + '/messages', { text: reply.value, files: att.files.splice(0) }); att.redraw && att.redraw(); reply.value = ''; await load(); } catch (e) { msg.className = 'small err'; msg.textContent = clean(e); } send.disabled = false; } }, 'Send');
  const att = spAttach(), replyBox = h('div', { class: 'stack' }, reply, att.el, h('div', { class: 'row' }, h('div', { class: 'grow' }), send), msg); body.append(replyBox);
  const load = async () => {
    let t; try { t = await cm.support('GET', '/tickets/' + SP.open); } catch (e) { const b = $('#sp-thread'); if (b) { b.innerHTML = ''; b.append(h('p', { class: 'err small' }, clean(e))); } return; }
    const box = $('#sp-thread'); if (!box) return; box.innerHTML = '';
    const theirs = !!t.theirs; replyBox.style.display = theirs ? 'none' : '';
    box.append(h('div', { class: 'row' }, h('h4', { class: 'grow', style: 'margin:0' }, t.title), h('span', { class: 'small muted' }, t.id)));
    if (theirs) box.append(spActions(t, t.messages.map((m) => (m.from === 'support' ? 'Support' : t.email) + ': ' + m.text).join('\n\n')));
    if (theirs) box.append(h('div', { class: 'small muted' }, 'From ' + t.email + '. You see everything here and cannot change it. Answer by replying to the email.'));
    t.messages.forEach((m) => box.append(h('div', { class: 'spmsg ' + m.from }, h('div', { class: 'small muted' }, (m.from === 'support' ? 'Hearth Support' : theirs ? t.email : 'You') + ' · ' + spAgo(m.at)), h('div', {}, m.text), ...(m.files || []).map((f) => h('div', { class: 'small muted' }, '📎 ' + f.name)))));
  };
  load(); SP.timer = setInterval(load, 15000);
}

/** For an owner: what to do with somebody's request. The same three choices as the notification. */
function spActions(t, text) {
  const msg = h('span', { class: 'small muted' });
  const go = async (action, label) => {
    msg.textContent = label + '…';
    try {
      const ms = await cm.machines(), m = ms.find((x) => x.secure) || ms[0]; if (!m) throw new Error('Connect your server first.');
      const r = await cm.relay(m.id, 'POST', '/inbox/act', { id: t.id, title: t.title, category: t.category, from: t.email, text: String(text || '').slice(0, 5000), action });
      msg.textContent = r.text || 'Done.';
      if (action === 'claude' && r.ok && r.session) { closeSupport(); setTab('sessions'); openSession({ id: r.session, title: t.title, cwd: r.cwd || '', mid: m.id, mtime: Date.now(), live: true }); }
    } catch (e) { msg.textContent = clean(e).includes('unknown') ? 'Update your server first.' : clean(e); }
  };
  return h('div', { class: 'row wrap', style: 'gap:8px;margin:6px 0' },
    h('button', { class: 'ghost sm', onclick: () => go('list', 'Adding') }, 'Add to tasks'),
    h('button', { class: 'ghost sm', onclick: () => go('claude', 'Starting Claude') }, 'Fix with Claude'),
    msg);
}
