'use strict';
/* Shared chats. A shared chat is a normal chat of someone's server that other people may open. What a friend sends is a request on the host's account:
   the host gets a notification, can change the prompt and run it, deny it, or give that friend full permission. */
const SMODES = [['plan', 'Plan only: can read, changes nothing'], ['acceptEdits', 'Edits: can change files'], ['manual', 'Ask me at every step'], ['bypassPermissions', 'Full auto: anything, without asking']];

/** The chats you were invited to are a list of items with a time, kept on your own server too, so all your phones and computers show the same chats. The newest write wins. */
const RS = { items: (() => { try { return JSON.parse(localStorage.getItem('cm.shsync') || '{}'); } catch { return {}; } })(), timer: 0, last: 0, busy: false };
const rsVal = (k, d) => { const it = RS.items[k]; return it && it.v != null ? it.v : d; };
function rsSet(k, v) { RS.items[k] = { v: v === undefined ? null : v, t: Date.now() }; localStorage.setItem('cm.shsync', JSON.stringify(RS.items)); clearTimeout(RS.timer); RS.timer = setTimeout(rsSync, 600); shApply(); }
/** Gives the main process the guest keys, so the shared chats show up in the chat list. */
async function shApply() {
  const list = Object.entries(RS.items).filter(([k, it]) => k.startsWith('g:') && it.v).map(([, it]) => it.v);
  try { SS.guests = await cm.setGuests(list); } catch { /* keep the old list */ }
}
async function rsSync() {
  if (RS.busy) return; RS.busy = true;
  try {
    const m = (await cm.machines()).find((x) => x.secure);
    if (m) {
      const r = await cm.relay(m.id, 'POST', '/rooms-sync', { items: RS.items });
      let changed = false;
      for (const [k, it] of Object.entries(r.items || {})) { const cur = RS.items[k]; if (!cur || cur.t < it.t) { RS.items[k] = it; changed = true; } }
      localStorage.setItem('cm.shsync', JSON.stringify(RS.items));
      if (changed) { await shApply(); if (tab === 'sessions') { await pollLive(); drawSide(); } }
    }
    RS.last = Date.now();
  } catch { /* no server reachable: the local copy is used and pushed later */ }
  finally { RS.busy = false; }
}
const shMyName = () => rsVal('name', localStorage.getItem('cm.myName') || '');
const shMachineUrl = (m) => `${m.secure ? 'https' : 'http'}://${m.host}${m.port && m.port !== 443 && m.port !== 80 ? ':' + m.port : ''}`;
const isGuestMid = (mid) => (SS.guests || []).some((g) => g.id === mid);
shApply();
setInterval(() => { if (Date.now() - RS.last > 8000) rsSync(); }, 4000);
setTimeout(rsSync, 1500);

// ───────── a window for the small dialogs ─────────
function shModal(...kids) {
  const over = h('div', { class: 'overlay' }), box = h('div', { class: 'modal browser' }, ...kids);
  over.append(box); over.onclick = (e) => { if (e.target === over) over.remove(); }; document.body.append(over);
  return { over, close: () => over.remove() };
}

// ───────── a friend joins ─────────
function shJoinDialog() {
  const link = h('input', { placeholder: 'Paste the invite link from your friend' }), name = h('input', { placeholder: 'Your name', value: shMyName() }), err = h('div', { class: 'small err' });
  let w;
  const go = async () => {
    try {
      const u = new URL(link.value.trim().replace(/^hearth:\/\/share/, 'https://x/share'));
      const url = u.searchParams.get('url'), invite = u.searchParams.get('invite'); if (!url || !invite) throw new Error('That is not an invite link.');
      const nm = name.value.trim() || 'Guest'; rsSet('name', nm); localStorage.setItem('cm.myName', nm);
      const r = await cm.shareJoin(url, invite, nm);
      rsSet('g:' + r.share, { share: r.share, key: r.key, hostname: r.hostname, port: r.port, secure: r.secure, host: r.host, title: r.title, member: r.member, session: r.session });
      await shApply(); await pollLive(); w.close();
      if (tab !== 'sessions') setTab('sessions');
      const g = (SS.guests || []).find((x) => x.share === r.share), hit = g && allSessions().find((x) => x.mid === g.id);
      if (hit) openSession(hit); else drawSide();
      toast('Joined “' + r.title + '”. What you send goes to ' + r.host + ' to approve.');
    } catch (e) { err.textContent = /Invalid URL/.test(String(e.message)) ? 'That is not an invite link.' : clean(e); }
  };
  link.onkeydown = name.onkeydown = (e) => { if (e.key === 'Enter') go(); };
  w = shModal(h('h3', {}, 'Join a shared chat'), h('div', { class: 'small muted' }, 'You see the chat like any other. When you send something, it goes to the host to approve, and they can change it first.'), link, name,
    h('div', { class: 'row' }, h('button', { class: 'primary', onclick: go }, 'Join'), h('button', { class: 'ghost', onclick: () => w.close() }, 'Cancel')), err);
  link.focus();
}

// ───────── the host shares a chat ─────────
/** Opens the sharing window of a chat: who can open it, what each person may do, the invite link. */
async function shOpenShare(s) {
  if (!s || !s.id) { toast('Send a message first: a chat is shared once it exists.'); return; }
  const mach = SS.machines.find((m) => m.id === s.mid) || {};
  let info;
  try { info = await cm.relay(s.mid, 'POST', `/sessions/${s.id}/share`, {}); } catch (e) { toast(clean(e)); return; }
  const body = h('div', { class: 'stack' }), url = shMachineUrl(mach);
  const w = shModal(h('div', { class: 'row' }, h('h3', { class: 'grow' }, 'Share “' + (s.title || 'this chat') + '”'), h('button', { class: 'ghost sm', onclick: () => w.close() }, 'Close')),
    h('div', { class: 'small muted' }, 'People you invite open this chat like any other. What they send is not run at once: you get a notification, can change it, and run it on your account, or deny it. Or give someone full permission.'), body);
  const draw = () => {
    body.innerHTML = '';
    if (mach.local && !mach.secure) body.append(h('div', { class: 'warnnote' }, 'This chat is on your own computer, so friends elsewhere cannot reach it. Share chats that live on your server.'));
    body.append(h('div', { class: 'label' }, 'People'));
    if (!info.members.length) body.append(h('div', { class: 'small muted' }, 'Nobody yet. Make an invite link and send it.'));
    for (const m of info.members) {
      const g = m.grant || { auto: false, mode: 'plan' };
      const set = async (grant) => { try { const r = await cm.relay(s.mid, 'POST', `/shares/${info.id}/members/${m.id}`, { grant }); m.grant = r.grant; draw(); } catch (e) { toast(clean(e)); } };
      const chk = h('input', { type: 'checkbox' }); chk.checked = g.auto; chk.onchange = () => set({ auto: chk.checked, mode: g.mode });
      body.append(h('div', { class: 'row wrap' }, h('b', { class: 'grow' }, m.name), h('label', { class: 'row small' }, chk, h('span', {}, 'Full permission: runs without asking')),
        h('select', { class: 'sel', onchange: (e) => set({ auto: g.auto, mode: e.target.value }) }, SMODES.map(([k, l]) => h('option', { value: k, selected: g.mode === k }, l.split(':')[0]))),
        h('button', { class: 'ghost sm', onclick: async () => { if (!confirm('Remove ' + m.name + '?')) return; try { await cm.relay(s.mid, 'DELETE', `/shares/${info.id}/members/${m.id}`); info.members = info.members.filter((x) => x.id !== m.id); draw(); } catch (e) { toast(clean(e)); } } }, 'Remove')));
    }
    const out = h('input', { readonly: true, placeholder: 'The invite link shows here', style: 'font-family:ui-monospace,monospace' });
    body.append(h('div', { class: 'row' }, h('button', { class: 'primary sm', onclick: async () => {
      try { const r = await cm.relay(s.mid, 'POST', `/shares/${info.id}/invite`, {}); out.value = `hearth://share?url=${encodeURIComponent(url)}&invite=${r.invite}`; out.select(); try { await navigator.clipboard.writeText(out.value); toast('Invite link copied. It works once, for 24 hours.'); } catch { /* copy by hand */ } } catch (e) { toast(clean(e)); } } }, 'Make an invite link'),
      h('div', { class: 'grow' }, out)));
    body.append(h('div', { class: 'row' }, h('div', { class: 'grow' }), h('button', { class: 'ghost sm', onclick: async () => { if (!confirm('Stop sharing this chat? Everyone loses access.')) return; try { await cm.relay(s.mid, 'DELETE', `/shares/${info.id}`); w.close(); toast('This chat is not shared any more.'); pollLive(); } catch (e) { toast(clean(e)); } } }, 'Stop sharing')));
  };
  draw();
}

// ───────── a friend's request, for the host to decide (card in the chat) ─────────
function requestCard(p) {
  const ev = p.ev, ta = h('textarea', { rows: 3, class: 'rmedit' }); ta.value = String(ev.text || '').replace(/^[^:]*:\s*/, '');
  const mode = h('select', { class: 'sel' }, SMODES.map(([k, l]) => h('option', { value: k }, l)));
  const always = h('input', { type: 'checkbox' }); let busy = false;
  const decide = async (behavior) => {
    if (busy) return; busy = true;
    try { await cm.relay(p.machine, 'POST', '/decision', { req: ev.req, behavior, text: ta.value, mode: mode.value, always: always.checked }); SS.perms = SS.perms.filter((x) => x !== p); renderPerms(); }
    catch (e) { toast(clean(e)); busy = false; }
  };
  return h('div', { class: 'perm pc rmreq', 'data-req': ev.req }, h('div', { class: 'label' }, `${ev.from || 'A friend'} wants to use your Claude`),
    h('div', { class: 'small muted' }, 'It runs in this chat, on your account. You can change the prompt first.'), ta,
    h('div', { class: 'row wrap' }, mode, h('label', { class: 'row small' }, always, h('span', {}, `Always run ${ev.from || 'their'} prompts without asking (full permission)`))),
    h('div', { class: 'row' }, h('button', { class: 'primary sm', onclick: () => decide('allow') }, 'Run it'), h('button', { class: 'ghost sm', onclick: () => decide('deny') }, 'Deny')));
}
/** For a friend: what happened to the prompts they sent (waiting, denied, running...). Polled with the chat. */
async function shLoadGuestInfo() {
  const s = SS.sel; if (!s || !s.id || !isGuestMid(s.mid)) { SS.shared = null; return; }
  try { const p = await cm.relay(s.mid, 'GET', `/sessions/${s.id}/prefs`); if (SS.sel === s) { const was = JSON.stringify(SS.shared); SS.shared = p.shared || null; if (was !== JSON.stringify(SS.shared)) shDrawStatus(); } } catch { /* offline */ }
}
function shDrawStatus() {
  const box = $('#shstatus'); if (!box) return;
  box.innerHTML = '';
  const sh = SS.shared; if (!sh) return;
  const LABEL = { pending: 'Waiting for ' + sh.host + ' to approve', approved: 'Approved, starting', running: 'Running', done: 'Done', denied: 'Not run', failed: 'Failed' };
  for (const q of sh.requests.filter((x) => Date.now() - x.ts < 15 * 60e3 || x.status === 'pending').slice(0, 4).reverse()) {
    box.append(h('div', { class: 'meta shreq ' + q.status }, `${LABEL[q.status] || q.status}${q.edited ? ' (changed by ' + sh.host + ')' : ''}: “${q.text.length > 70 ? q.text.slice(0, 70) + '…' : q.text}”` + (q.note && q.status !== 'done' ? ' · ' + q.note : '')));
  }
}

/** A friend sends a prompt: it goes to the host as a request. Nothing is added to the chat until the host runs it. */
function shGuestSend(s, text) {
  const sid = rand(); let host = '';
  chatHandlers[sid] = (ev) => {
    if (ev.t === 'queued') toast(ev.auto ? 'It runs at once: ' + (ev.to || 'the host') + ' gave you full permission.' : `Sent to ${ev.to || 'the host'}. They have to approve it first, and may change it.`);
    else if (ev.t === 'fail') toast(ev.text || 'Could not send');
    if (ev.t === 'end' || ev.t === 'fail') { delete chatHandlers[sid]; shLoadGuestInfo(); if (typeof loadMsgs === 'function') loadMsgs(true); }
  };
  cm.send(s.mid, s.id, text, 'plan', false, sid, {}).catch((e) => { delete chatHandlers[sid]; toast(clean(e)); });
}
