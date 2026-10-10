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
    body.append(h('p', { class: 'muted' }, 'Something not working, or an idea? Write to us here. We answer by email.'),
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

async function viewHome(body) {
  body.append(h('div', { class: 'row' }, h('div', { class: 'grow small muted' }, 'Signed in as ' + SP.email), h('button', { class: 'primary sm', onclick: () => { SP.view = 'new'; drawSupport(); } }, 'New request')), h('div', { class: 'flist', id: 'sp-list' }, h('p', { class: 'muted small' }, 'Loading…')));
  const load = async () => {
    let list; try { list = await cm.support('GET', '/tickets'); } catch (e) { if (!SP.signedIn || !SP.el) return; const l = $('#sp-list'); if (l) { l.innerHTML = ''; l.append(h('p', { class: 'err small' }, clean(e))); } return; }
    SP.tickets = list; const box = $('#sp-list'); if (!box) return; box.innerHTML = '';
    if (!list.length) box.append(h('div', { class: 'obnote' }, h('b', {}, 'No requests yet'), h('div', { class: 'small muted' }, 'Press “New request” and tell us what is going on. We will answer by email, and you can read the answer here too.')));
    list.forEach((t) => box.append(h('button', { class: 'spitem', onclick: () => { SP.open = t.id; SP.view = 'thread'; drawSupport(); } }, h('div', { class: 'row' }, h('b', { class: 'grow ellip' }, t.title), h('span', { class: 'pill ' + (t.status === 'answered' ? 'ok' : 'warn'), style: 'padding:2px 9px' }, h('i'), t.status === 'answered' ? 'Answered' : 'Open')),
      h('div', { class: 'small muted ellip' }, (t.from === 'support' ? 'Hearth Support: ' : 'You: ') + t.last), h('div', { class: 'small muted' }, t.id + ' · ' + spAgo(t.updatedAt)))));
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
    try { const r = await cm.support('POST', '/tickets', { category: cat, title: title.value, text: text.value, meta: info.checked ? SP.meta : {} }); SP.sent = r; SP.view = 'sent'; drawSupport(); }
    catch (e) { msg.className = 'small err'; msg.textContent = clean(e); btn.disabled = false; }
  };
  const go = h('button', { class: 'primary', onclick: () => send(go) }, 'Send request');
  body.append(h('button', { class: 'link', onclick: () => { SP.view = 'home'; drawSupport(); } }, '‹ Back'), h('div', { class: 'small muted' }, 'What is it about?'), chips, hint, title, text,
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
  const send = h('button', { class: 'primary sm', onclick: async () => { msg.textContent = ''; send.disabled = true; try { await cm.support('POST', '/tickets/' + SP.open + '/messages', { text: reply.value }); reply.value = ''; await load(); } catch (e) { msg.className = 'small err'; msg.textContent = clean(e); } send.disabled = false; } }, 'Send');
  body.append(reply, h('div', { class: 'row' }, h('div', { class: 'grow' }), send), msg);
  const load = async () => {
    let t; try { t = await cm.support('GET', '/tickets/' + SP.open); } catch (e) { const b = $('#sp-thread'); if (b) { b.innerHTML = ''; b.append(h('p', { class: 'err small' }, clean(e))); } return; }
    const box = $('#sp-thread'); if (!box) return; box.innerHTML = '';
    box.append(h('div', { class: 'row' }, h('h4', { class: 'grow', style: 'margin:0' }, t.title), h('span', { class: 'small muted' }, t.id)));
    t.messages.forEach((m) => box.append(h('div', { class: 'spmsg ' + m.from }, h('div', { class: 'small muted' }, (m.from === 'support' ? 'Hearth Support' : 'You') + ' · ' + spAgo(m.at)), h('div', {}, m.text))));
  };
  load(); SP.timer = setInterval(load, 15000);
}
