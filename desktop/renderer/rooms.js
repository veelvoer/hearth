'use strict';
/* Shared chats: work in one chat with someone else, each with their own Claude account, or ask the host to run a prompt on theirs. See relay/rooms.js. */
const RM = { list: [], sel: null, view: null, seq: 0, sig: '', timer: 0, busy: false, err: null, ownBusy: false, form: null };
/** Everything about the shared chats you joined (the invites you used, your folder, your Claude chat, your "always run" choices) lives in one list of
 *  items with a time. It is kept on your own server too, so all your phones and computers show the same chats. The newest write of an item wins. */
const RS = { items: (() => { try { return JSON.parse(localStorage.getItem('cm.rsync') || '{}'); } catch { return {}; } })(), timer: 0, last: 0, busy: false };
const rsVal = (k, d) => { const it = RS.items[k]; return it && it.v != null ? it.v : d; };
function rsSet(k, v) { RS.items[k] = { v: v === undefined ? null : v, t: Date.now() }; localStorage.setItem('cm.rsync', JSON.stringify(RS.items)); clearTimeout(RS.timer); RS.timer = setTimeout(rsSync, 800); }
async function rsSync() {
  if (RS.busy) return; RS.busy = true;
  try {
    const ms = await cm.machines(), m = ms.find((x) => x.secure); if (!m) return;
    const r = await cm.relay(m.id, 'POST', '/rooms-sync', { items: RS.items });
    let changed = false;
    for (const [k, it] of Object.entries(r.items || {})) { const cur = RS.items[k]; if (!cur || cur.t < it.t) { RS.items[k] = it; changed = true; } }
    localStorage.setItem('cm.rsync', JSON.stringify(RS.items)); RS.last = Date.now();
    if (changed && tab === 'shared') { await rmLoadList(); rmSide(); if (!RM.sel && !RM.form) rmDraw(); }
  } catch { /* no server reachable: the local copy is used and pushed later */ }
  finally { RS.busy = false; }
}
// old versions kept these in separate places: bring them in once
(function () {
  if (localStorage.getItem('cm.rsyncMigrated')) return; localStorage.setItem('cm.rsyncMigrated', '1');
  const t = Date.now() - 1; const get = (k, d) => { try { return JSON.parse(localStorage.getItem(k) || d); } catch { return JSON.parse(d); } };
  for (const j of get('cm.joined', '[]')) RS.items['j:' + j.id + ':' + j.member] = { v: j, t };
  for (const [k, v] of Object.entries(get('cm.roomCfg', '{}'))) RS.items['cfg:' + k] = { v, t };
  for (const [k, v] of Object.entries(get('cm.roomSess', '{}'))) RS.items['s:' + k] = { v, t };
  for (const [k, v] of Object.entries(get('cm.roomAuto', '{}'))) RS.items['a:' + k] = { v, t };
  localStorage.setItem('cm.rsync', JSON.stringify(RS.items));
})();
const rmJoined = () => Object.entries(RS.items).filter(([k, it]) => k.startsWith('j:') && it.v).map(([, it]) => it.v);
const rmAddJoined = (j) => rsSet('j:' + j.id + ':' + j.member, j);
const rmDropJoined = (id, member) => rsSet('j:' + id + ':' + member, null);
const rmSess = (key) => rsVal('s:' + key, {});
const rmSetSess = (key, v) => rsSet('s:' + key, v);
const rmCfg = (key) => rsVal('cfg:' + key, {});
const rmAutoMode = (key, mid) => rsVal('a:' + key + ':' + mid, null);
const rmSetAuto = (key, mid, mode) => rsSet('a:' + key + ':' + mid, mode);
const rmMyName = () => rsVal('name', localStorage.getItem('cm.myName') || '');
const rmMachineUrl = (m) => `${m.secure ? 'https' : 'http'}://${m.host}${m.port && m.port !== 443 && m.port !== 80 ? ':' + m.port : ''}`;
/** Shared chats always live on your server (so they keep going when your computer is off, and every device sees the same). */
async function rmHostMachine() { const ms = await cm.machines(); return ms.find((x) => x.secure) || null; }
const rmNeedServer = () => h('div', { class: 'warnnote' }, 'Shared chats run on your server, so they keep going when your computer is off and every device sees the same chat. Connect a server first (Settings → Computers and servers).');
const rmCall = (e, method, path, body) => e.kind === 'host' ? cm.relay(e.mid, method, '/rooms' + path, body) : cm.roomCall(e.conn, method, '/rooms' + path, body);

async function rmLoadList() {
  const out = [];
  try { const m = await rmHostMachine(); if (m) for (const r of (await cm.relay(m.id, 'GET', '/rooms'))) out.push({ key: 'h:' + r.id, kind: 'host', mid: m.id, id: r.id, title: r.title, pending: r.pending, members: r.members, last: r.last, url: rmMachineUrl(m), secure: !!m.secure, local: !!m.local }); } catch { /* no rooms here yet */ }
  for (const j of rmJoined()) out.push({ key: 'g:' + j.id + ':' + j.member, kind: 'guest', conn: { url: j.url, token: j.token }, id: j.id, title: j.title, host: j.host, member: j.member });
  RM.list = out;
  if (RM.sel && !out.some((x) => x.key === RM.sel)) { RM.sel = null; RM.view = null; }
}
const rmEntry = () => RM.list.find((x) => x.key === RM.sel) || null;
async function rmPoll(force) {
  const e = rmEntry(); if (!e || RM.busy) return;
  RM.busy = true;
  try {
    const v = await rmCall(e, 'GET', `/${e.id}?since=${force ? 0 : RM.seq}`);
    if (rmEntry() !== e) return;   // another chat was opened while this was on its way
    const prev = RM.view && RM.view.id === v.id && !force ? RM.view : null;
    const msgs = prev ? prev.messages.concat(v.messages.filter((m) => !prev.messages.some((x) => x.n === m.n))) : v.messages;
    RM.view = { ...v, messages: msgs }; RM.seq = v.seq; RM.err = null;
  } catch (er) { RM.err = clean(er); }
  finally { RM.busy = false; }
  if (tab === 'shared') rmSync();
}
function sharedView() {
  const root = h('div', { class: 'rmwrap' }, h('div', { class: 'rmside', id: 'rmside' }), h('div', { class: 'rmmain', id: 'rmmain' }));
  clearInterval(RM.timer); RM.dom = null; RM.sideSig = '';
  rsSync();
  setTimeout(async () => { await rmLoadList(); if (!RM.sel && !RM.form && RM.list[0]) RM.sel = RM.list[0].key; rmDraw(); if (RM.sel) rmPoll(true); }, 0);
  RM.timer = setInterval(async () => {
    if (tab !== 'shared') { clearInterval(RM.timer); return; }
    if (RM.sel) rmPoll(false);
    if (Date.now() - RS.last > 6000) rsSync();
    const before = RM.sideSig; await rmLoadList(); if (tab === 'shared') rmSide(); if (before !== RM.sideSig && !RM.sel && !RM.form) rmDraw();
  }, 2500);
  return root;
}
function rmSelect(key) { RM.sel = key; RM.view = null; RM.seq = 0; RM.form = null; RM.dom = null; rmPoll(true); rmDraw(); }

/** The list on the left: redrawn only when it really changed. */
function rmSide() {
  const side = $('#rmside'); if (!side) return;
  const sig = JSON.stringify([RM.sel, RM.list.map((r) => [r.key, r.title, r.pending, r.members])]);
  if (sig === RM.sideSig && side.firstChild) return;
  RM.sideSig = sig; side.innerHTML = '';
  side.append(h('div', { class: 'row' }, h('button', { class: 'primary sm grow', onclick: () => { RM.form = 'new'; RM.sel = null; RM.view = null; RM.dom = null; rmDraw(); } }, '+ Share a chat'), h('button', { class: 'ghost sm', onclick: () => { RM.form = 'join'; RM.sel = null; RM.view = null; RM.dom = null; rmDraw(); } }, 'Join')));
  for (const r of RM.list) side.append(h('div', { class: 'item' + (r.key === RM.sel ? ' on' : ''), onclick: () => rmSelect(r.key) },
    h('div', { class: 'ititle' }, r.pending ? h('span', { class: 'dot' }) : null, h('span', { class: 'ellip' }, r.title)),
    h('div', { class: 'small muted' }, r.kind === 'host' ? `You host · ${r.members} ${r.members === 1 ? 'person' : 'people'}` + (r.pending ? ` · ${r.pending} waiting` : '') : `With ${r.host || 'a friend'}`)));
  if (!RM.list.length) side.append(h('p', { class: 'small muted' }, 'No shared chats yet. Start one, or join with an invite from a friend.'));
}
/** Builds the main area from scratch. Done when you open something, never by the timer. */
function rmDraw() {
  const main = $('#rmmain'); if (!main) return;
  RM.sideSig = ''; rmSide();
  main.innerHTML = ''; RM.dom = null;
  if (RM.form === 'new') return main.append(rmNewForm());
  if (RM.form === 'join') return main.append(rmJoinForm());
  const e = rmEntry();
  if (!e) return main.append(h('div', { class: 'empty' }, h('h2', {}, 'Work together'), h('p', { class: 'muted' }, 'Share a chat with a friend. You each use your own Claude account, and you can ask each other to run something on the other’s account, with a question first.')));
  main.append(h('div', { class: 'muted small', style: 'padding:20px', id: 'rmload' }, RM.err || 'Loading…'));
  if (RM.view) rmSync();
}
/** Brings an open chat up to date part by part: the box you type in, the prompt you are editing and the scroll position stay exactly as they are. */
function rmSync() {
  const main = $('#rmmain'), e = rmEntry(), v = RM.view; if (!main || RM.form || !e) return;
  if (!v) { const l = $('#rmload'); if (l && RM.err) l.textContent = RM.err; return; }
  if (!RM.dom || RM.dom.key !== e.key) { main.innerHTML = ''; RM.dom = rmBuild(e); main.append(RM.dom.root); }
  const D = RM.dom, host = e.kind === 'host';
  const sig = (k, val) => { const s = JSON.stringify(val); if (D.sig[k] === s) return false; D.sig[k] = s; return true; };
  if (sig('head', [v.title, v.host, host, v.members.map((m) => m.name)])) D.head.replaceWith(D.head = rmHead(e, v));
  if (sig('msgs', [v.messages.length && v.messages[v.messages.length - 1].n, v.messages.length, v.working.map((w) => w.name)])) {
    const o = D.msgs, stick = o.scrollHeight - o.scrollTop - o.clientHeight < 140, top = o.scrollTop;
    D.col.replaceChildren(...rmMessages(e, v));
    o.scrollTop = stick ? o.scrollHeight : top;
  }
  rmRequests(e, v);
  if (sig('btns', [v.members.map((m) => m.id + m.name), v.host, v.grant && v.grant.auto])) rmButtons(e, v);
  if (host && sig('members', v.members.map((m) => [m.id, m.name, m.grant]))) D.members.replaceChildren(...rmMembersRows(e, v));
  D.members.style.display = host && v.members.length ? '' : 'none';
}
function rmBuild(e) {
  const D = { key: e.key, sig: {}, handled: new Set() };
  D.head = h('div'); D.msgs = h('div', { class: 'msgs', id: 'rmmsgs' }); D.col = h('div', { class: 'col' }); D.msgs.append(D.col);
  D.reqs = h('div', { class: 'rmreqs' }); D.btns = h('div', { class: 'row wrap rmbtns' }); D.note = h('div', { class: 'small muted' });
  D.ta = h('textarea', { id: 'rm-text', rows: 2, placeholder: e.kind === 'host' ? 'Ask your Claude, or just talk to the others…' : 'Write something…' });
  D.members = h('div', { class: 'rmmembers' });
  D.ta.onkeydown = (ev) => { if (ev.key === 'Enter' && !ev.shiftKey && !ev.isComposing) { ev.preventDefault(); (e.kind === 'host' ? D.askMine : D.sendOwn)(); } };
  const own = e.kind === 'guest' ? rmOwnSettings(e) : null;
  D.root = h('div', { class: 'rmroom' }, D.head, D.msgs, D.reqs, h('div', { class: 'composer rmcomposer' }, h('div', { class: 'cbox' }, D.ta, D.btns), D.note, own), D.members);
  return D;
}
function rmMessages(e, v) {
  const out = [];
  for (const m of v.messages) {
    if (m.kind === 'status') { out.push(h('div', { class: 'meta' }, m.text)); continue; }
    const mine = m.from === v.me;
    if (m.kind === 'assistant') out.push(h('div', { class: 'rmmsg claude' }, h('div', { class: 'small muted' }, m.name + (m.via === 'host' ? ' · on the host’s account' : m.via === 'own' ? ' · own account' : '')), (() => { const d = h('div', { class: 'md' }); d.append(md(m.text)); return d; })()));
    else out.push(h('div', { class: 'rmmsg' + (mine ? ' mine' : '') }, h('div', { class: 'small muted' }, m.name + (m.via === 'ask-host' ? ' → asks the host’s Claude' : m.via === 'ask' ? ` → asks ${m.to}’s Claude` : '')), h('div', { class: 'msg user', style: 'align-self:auto;max-width:100%' }, m.text)));
  }
  for (const w of v.working) out.push(h('div', { class: 'meta' }, w.name + ' is working…'));
  return out;
}
function rmHead(e, v) {
  const host = e.kind === 'host';
  return h('div', { class: 'chat-head' }, h('div', { class: 'grow', style: 'min-width:0' }, h('h3', { class: 'ellip' }, v.title),
    h('div', { class: 'small muted ellip' }, [host ? 'You host this chat' : 'Hosted by ' + v.host, v.members.map((m) => m.name).join(', ') || 'only you so far'].join(' · '))),
    host ? h('button', { class: 'ghost sm', onclick: () => rmInvite(e) }, 'Invite') : h('button', { class: 'ghost sm', onclick: async () => { if (!confirm('Leave this shared chat?')) return; try { await rmCall(e, 'POST', `/${e.id}/leave`, {}); } catch { /* gone anyway */ } rmDropJoined(e.id, e.member); RM.sel = null; RM.dom = null; await rmLoadList(); rmDraw(); } }, 'Leave'),
    host ? h('button', { class: 'ghost sm', onclick: async () => { if (!confirm('Delete this shared chat for everyone?')) return; try { await rmCall(e, 'DELETE', `/${e.id}`); } catch (er) { toast(clean(er)); } RM.sel = null; RM.dom = null; await rmLoadList(); rmDraw(); } }, 'Delete') : null);
}
/** Cards for prompts that wait for YOUR decision: on the host those asked of the host, for a friend those asked of them. Existing cards stay untouched (what you edited stays). */
function rmRequests(e, v) {
  const D = RM.dom, host = e.kind === 'host';
  const mineToDecide = v.requests.filter((q) => q.status === 'pending' && (host ? q.to === 'host' : q.to === v.me));
  const have = new Map([...D.reqs.children].map((c) => [c.dataset.id, c]));
  for (const [id, c] of have) if (!mineToDecide.some((q) => q.id === id)) c.remove();
  for (const q of mineToDecide) if (!have.has(q.id)) {
    const auto = !host && rmAutoMode(e.key, q.mid);
    if (auto) { if (!D.handled.has(q.id)) { D.handled.add(q.id); rmRunRequest(e, v, q, q.text, auto); } continue; }
    D.reqs.append(rmRequestCard(e, v, q));
  }
  const waiting = v.requests.filter((q) => q.status === 'pending' && q.mid === v.me);
  const sig = JSON.stringify(waiting.map((q) => q.id + q.toName));
  if (D.sig.wait !== sig) { D.sig.wait = sig; let w = D.reqs.querySelector('.rmwait'); if (w) w.remove(); if (waiting.length) D.reqs.prepend(h('div', { class: 'small muted rmwait', style: 'padding:0 26px' }, waiting.map((q) => `Waiting for ${q.toName} to approve: “${q.text.slice(0, 60)}”`).join(' · '))); }
}
function rmRequestCard(e, v, q) {
  const host = e.kind === 'host';
  const ta = h('textarea', { rows: 3, class: 'rmedit' }); ta.value = q.text;
  const mode = h('select', { class: 'sel' }, RM_MODES.map(([k, l]) => h('option', { value: k }, l)));
  const always = h('input', { type: 'checkbox' });
  const busy = { v: false };
  const decide = async (decision) => {
    if (busy.v) return; busy.v = true;
    try {
      if (host) { await rmCall(e, 'POST', `/${e.id}/requests/${q.id}`, { decision, text: ta.value, mode: mode.value, always: always.checked }); }
      else if (decision === 'deny') await rmCall(e, 'POST', `/${e.id}/requests/${q.id}`, { decision: 'deny' });
      else { if (always.checked) rmSetAuto(e.key, q.mid, mode.value); RM.dom && RM.dom.handled.add(q.id); await rmRunRequest(e, v, q, ta.value, mode.value); }
      rmPoll(true);
    } catch (er) { toast(clean(er)); } finally { busy.v = false; }
  };
  return h('div', { class: 'perm pc rmreq', 'data-id': q.id }, h('div', { class: 'label' }, `${q.name} wants to ask your Claude`),
    h('div', { class: 'small muted' }, host ? 'It runs on your account and your server. You can change the prompt first.' : 'It runs on your own account, on your own computer or server. You can change the prompt first.'), ta,
    h('div', { class: 'row wrap' }, mode, h('label', { class: 'row small' }, always, h('span', {}, `Always run ${q.name}'s prompts without asking (full permission)`))),
    h('div', { class: 'row' }, h('button', { class: 'primary sm', onclick: () => decide('approve') }, 'Run it'), h('button', { class: 'ghost sm', onclick: () => decide('deny') }, 'Deny')));
}
function rmMembersRows(e, v) {
  return [h('div', { class: 'label' }, 'People in this chat'), ...v.members.map((m) => {
    const g = m.grant || { auto: false, mode: 'plan' };
    const set = async (grant) => { try { await rmCall(e, 'POST', `/${e.id}/members/${m.id}`, { grant }); rmPoll(true); } catch (er) { toast(clean(er)); } };
    const sel = h('select', { class: 'sel', onchange: (ev) => set({ auto: g.auto, mode: ev.target.value }) }, RM_MODES.map(([k, l]) => h('option', { value: k, selected: g.mode === k }, l.split(':')[0])));
    return h('div', { class: 'row wrap' }, h('b', { class: 'grow' }, m.name), h('label', { class: 'row small' }, (() => { const c = h('input', { type: 'checkbox' }); c.checked = g.auto; c.onchange = () => set({ auto: c.checked, mode: g.mode }); return c; })(), h('span', {}, 'May use my account without asking')), sel,
      h('button', { class: 'ghost sm', onclick: async () => { if (!confirm('Remove ' + m.name + '?')) return; try { await rmCall(e, 'DELETE', `/${e.id}/members/${m.id}`); rmPoll(true); } catch (er) { toast(clean(er)); } } }, 'Remove'));
  })];
}
/** The buttons under the box: they depend on who is in the chat, so only they are rebuilt (never the box you type in). */
function rmButtons(e, v) {
  const D = RM.dom, host = e.kind === 'host', ta = D.ta;
  const take = () => { const t = ta.value.trim(); if (!t) return ''; ta.value = ''; return t; };
  const give = (t) => { ta.value = t; };
  const post = (t) => rmCall(e, 'POST', `/${e.id}/messages`, { text: t });
  D.askMine = async () => { const t = take(); if (!t) return; try { await rmCall(e, 'POST', `/${e.id}/ask`, { text: t }); rmPoll(false); } catch (er) { toast(clean(er)); give(t); } };
  D.sendOwn = async () => { const t = take(); if (!t) return; await rmRunOwn(e, RM.view || v, t); };
  const say = async () => { const t = take(); if (!t) return; try { await post(t); rmPoll(false); } catch (er) { toast(clean(er)); give(t); } };
  const targets = [...(host ? [] : [{ id: 'host', name: v.host }]), ...v.members.filter((m) => m.id !== v.me).map((m) => ({ id: m.id, name: m.name }))];
  const pick = h('select', { class: 'sel' }, targets.map((t) => h('option', { value: t.id }, t.name)));
  if (D.target && targets.some((t) => t.id === D.target)) pick.value = D.target; pick.onchange = () => { D.target = pick.value; };
  const askThem = async () => {
    const t = take(); if (!t) return; const to = pick.value, tn = (targets.find((x) => x.id === to) || {}).name || '';
    try { await rmCall(e, 'POST', `/${e.id}/requests`, { text: t, to }); rmPoll(false); toast(to === 'host' && v.grant && v.grant.auto ? `${tn} lets you run prompts at once.` : `Sent to ${tn}. They can change it before it runs.`); } catch (er) { toast(clean(er)); give(t); }
  };
  D.btns.replaceChildren(...[
    host ? h('button', { class: 'primary sm', onclick: D.askMine, title: 'Runs on your account and your server' }, 'Ask my Claude') : h('button', { class: 'primary sm', onclick: D.sendOwn, title: 'Runs on your own Claude account, on your own computer or server' }, 'Send with my account'),
    targets.length ? h('span', { class: 'row small' }, pick, h('button', { class: 'ghost sm', onclick: askThem, title: 'They see your prompt first and can change it' }, 'Ask their Claude')) : null,
    h('button', { class: 'ghost sm', onclick: say }, 'Just say it')].filter(Boolean));
  D.note.textContent = host ? 'Asking someone’s Claude uses their account: they decide, and can change your prompt first.' : `Asking someone’s Claude uses their account: they decide and can change your prompt first.` + (v.grant && v.grant.auto ? ` ${v.host} gave you full permission: prompts for them run at once.` : '');
}
function rmNewForm() {
  const title = h('input', { placeholder: 'Name, e.g. Landing page with Sam' }), folder = h('input', { placeholder: '/path/to/project on your server', style: 'font-family:ui-monospace,monospace' }), err = h('div', { class: 'small err' });
  (async () => { const m = await rmHostMachine(); if (!m) { err.replaceChildren(rmNeedServer()); return; } try { const info = await cm.relay(m.id, 'GET', '/projects'); folder.value = (SS.lastCwd && SS.lastCwd.startsWith('/') && SS.lastCwd) || ((info.projects || [])[0] || {}).path || ''; } catch { /* type it */ } })();
  const go = async () => {
    try { const m = await rmHostMachine(); if (!m) throw new Error('Shared chats run on your server. Connect a server first.'); const r = await cm.relay(m.id, 'POST', '/rooms', { title: title.value, cwd: folder.value.trim() }); await rmLoadList(); rmSelect('h:' + r.id); }
    catch (e) { err.textContent = clean(e); }
  };
  return h('div', { class: 'rmform stack' }, h('h2', {}, 'Share a chat'), h('p', { class: 'muted' }, 'Your Claude works in this folder on your server. Friends join with an invite and see everything said here. Nobody gets your login: they use their own account, or they ask you.'),
    title, folder, h('div', { class: 'row' }, h('button', { class: 'primary', onclick: go }, 'Create'), h('button', { class: 'ghost', onclick: () => { RM.form = null; rmDraw(); } }, 'Cancel')), err);
}
function rmJoinForm() {
  const link = h('input', { placeholder: 'Paste the invite link from your friend' }), name = h('input', { placeholder: 'Your name', value: rmMyName() }), err = h('div', { class: 'small err' });
  const go = async () => {
    try {
      const u = new URL(link.value.trim().replace(/^hearth:\/\/room/, 'https://x/room'));
      const url = u.searchParams.get('url'), invite = u.searchParams.get('invite'); if (!url || !invite) throw new Error('That is not an invite link.');
      const nm = name.value.trim() || 'Guest'; rsSet('name', nm);
      const r = await cm.roomCall({ url, token: '' }, 'POST', '/rooms/join', { invite, name: nm });
      rmAddJoined({ id: r.room, member: r.member, url, token: r.token, title: r.title, host: r.host });
      RM.form = null; await rmLoadList(); rmSelect('g:' + r.room + ':' + r.member);
    } catch (e) { err.textContent = /Invalid URL/.test(String(e.message)) ? 'That is not an invite link.' : clean(e); }
  };
  return h('div', { class: 'rmform stack' }, h('h2', {}, 'Join a shared chat'), link, name, h('div', { class: 'row' }, h('button', { class: 'primary', onclick: go }, 'Join'), h('button', { class: 'ghost', onclick: () => { RM.form = null; rmDraw(); } }, 'Cancel')), err);
}

const RM_MODES = [['plan', 'Plan only: can read, changes nothing'], ['acceptEdits', 'Edits: can change files'], ['manual', 'Ask me at every step'], ['bypassPermissions', 'Full auto: anything, without asking']];
async function rmInvite(e) {
  try {
    const r = await rmCall(e, 'POST', `/${e.id}/invite`, {});
    const link = `hearth://room?url=${encodeURIComponent(e.url)}&invite=${r.invite}`;
    const inp = h('input', { value: link, readonly: true, style: 'font-family:ui-monospace,monospace' });
    const over = h('div', { class: 'overlay' }, h('div', { class: 'modal browser' }, h('h3', {}, 'Invite a friend'),
      h('div', { class: 'small muted' }, 'Send this link. It works once and for 24 hours. They paste it in Hearth under Shared → Join.'), inp,
      e.local && !e.secure ? h('div', { class: 'warnnote' }, 'This address only works on your own network. For a friend elsewhere, host the shared chat on your server.') : null,
      h('div', { class: 'row' }, h('button', { class: 'primary sm', onclick: () => { inp.select(); try { navigator.clipboard.writeText(link); toast('Copied'); } catch { document.execCommand('copy'); } } }, 'Copy'), h('button', { class: 'ghost sm', onclick: () => over.remove() }, 'Close'))));
    over.onclick = (ev) => { if (ev.target === over) over.remove(); }; document.body.append(over); inp.select();
  } catch (er) { toast(clean(er)); }
}

function rmOwnSettings(e) {
  const key = e.key, cfg = rmCfg(key);
  const inp = h('input', { placeholder: 'Folder for your Claude on your server (empty = just talk, no files)', value: cfg.cwd || '', style: 'font-family:ui-monospace,monospace;font-size:12px', onchange: (ev) => { rsSet('cfg:' + key, { ...rmCfg(key), cwd: ev.target.value.trim() }); } });
  return h('div', { class: 'small muted' }, 'Your Claude works in: ', inp);
}
/** A prompt runs on YOUR own Claude account (your own computer or server), and the answer is posted into the shared chat.
 *  opts.by: the prompt comes from someone else's request (their message is already in the chat); opts.req: that request, reported done or failed when it ends. */
async function rmRunLocal(e, v, text, opts = {}) {
  if (RM.ownBusy) { toast('Your Claude is still working on the last one.'); throw new Error('busy'); }
  const cfg = rmCfg(e.key);
  SS.machines = await cm.machines();
  const machine = SS.machines.find((m) => m.secure);   // always your server, never a laptop: it keeps going when your computer is off
  if (!machine) { toast('Shared chats run on your server. Connect a server first (Settings).'); throw new Error('no server'); }
  const last = rmSess(e.key);
  const seen = last.seen || 0, others = v.messages.filter((m) => m.n > seen && m.from !== v.me && m.kind !== 'status').slice(-40);
  const people = [v.host, ...v.members.map((m) => m.name)].join(', ');
  const prompt = `You are in a shared chat called "${v.title}" with ${people}. ` + (opts.by ? `The next message comes from ${opts.by}, not from the owner of this account. Answer it directly.` : `Answer ${v.myName} directly.`)
    + (others.length ? '\n\nWhat was said in the chat since your last turn:\n' + others.map((m) => `${m.name}: ${m.text}`).join('\n') : '') + `\n\n${opts.by || v.myName}: ${text}`;
  RM.ownBusy = true;
  try { if (!opts.by) await rmCall(e, 'POST', `/${e.id}/messages`, { text }); await rmCall(e, 'POST', `/${e.id}/working`, { on: true }); } catch (er) { RM.ownBusy = false; toast(clean(er)); throw er; }
  rmPoll(false);
  return new Promise((resolve) => {
    const sid = rand(); let out = '', failed = '';
    const finish = async () => {
      delete chatHandlers[sid]; RM.ownBusy = false;
      rmSetSess(e.key, { id: rmSess(e.key).id || last.id, seen: v.seq });
      try { if (out.trim() || failed) await rmCall(e, 'POST', `/${e.id}/messages`, { text: out.trim() || `(${failed})`, kind: 'assistant' }); } catch { /* host gone */ }
      try { await rmCall(e, 'POST', `/${e.id}/working`, { on: false }); } catch { /* ignore */ }
      if (opts.req) { try { await rmCall(e, 'POST', `/${e.id}/requests/${opts.req}`, { decision: failed ? 'failed' : 'done', note: failed }); } catch { /* ignore */ } }
      rmPoll(false); resolve();
    };
    chatHandlers[sid] = (ev) => {
      if (ev.t === 'session' && ev.id) rmSetSess(e.key, { ...rmSess(e.key), id: ev.id });
      else if (ev.t === 'delta') out += ev.text;
      else if (ev.t === 'fail') { failed = ev.text || 'Your Claude failed'; finish(); }
      else if (ev.t === 'end') finish();
    };
    const talk = !cfg.cwd;
    cm.send(machine.id, last.id || null, prompt, opts.mode || 'acceptEdits', false, sid, { run: 'vps', ...(talk ? { kind: 'talk' } : { cwd: cfg.cwd }) }).catch((er) => { failed = clean(er); finish(); });
  });
}
const rmRunOwn = (e, v, text) => rmRunLocal(e, v, text).catch(() => {});
/** Someone asked for your Claude and you said yes: tell the chat, then run it here. */
async function rmRunRequest(e, v, q, text, mode) {
  await rmCall(e, 'POST', `/${e.id}/requests/${q.id}`, { decision: 'approve', text, mode });
  const fresh = RM.view || v;
  await rmRunLocal(e, fresh, text, { by: q.name, req: q.id, mode }).catch(() => {});
}
