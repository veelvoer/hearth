'use strict';
/* Shared chats: work in one chat with someone else, each with their own Claude account, or ask the host to run a prompt on theirs. See relay/rooms.js. */
const RM = { list: [], sel: null, view: null, seq: 0, sig: '', timer: 0, busy: false, err: null, ownBusy: false, form: null };
const rmJoined = () => { try { return JSON.parse(localStorage.getItem('cm.joined') || '[]'); } catch { return []; } };
const rmSaveJoined = (l) => localStorage.setItem('cm.joined', JSON.stringify(l));
const rmSess = () => { try { return JSON.parse(localStorage.getItem('cm.roomSess') || '{}'); } catch { return {}; } };
const rmCfg = () => { try { return JSON.parse(localStorage.getItem('cm.roomCfg') || '{}'); } catch { return {}; } };
const rmMachineUrl = (m) => `${m.secure ? 'https' : 'http'}://${m.host}${m.port && m.port !== 443 && m.port !== 80 ? ':' + m.port : ''}`;
/** Your own server (or this computer) hosts the rooms you start. */
async function rmHostMachine() { const ms = await cm.machines(); return ms.find((x) => x.secure) || ms.find((x) => x.local) || ms[0] || null; }
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
    const prev = RM.view && RM.view.id === v.id && !force ? RM.view : null;
    const msgs = prev ? prev.messages.concat(v.messages.filter((m) => !prev.messages.some((x) => x.n === m.n))) : v.messages;
    RM.view = { ...v, messages: msgs }; RM.seq = v.seq; RM.err = null;
  } catch (er) { RM.err = clean(er); if (/No such shared chat|bad token|locked|401/.test(String(er.message || er))) { /* removed from the room */ } }
  finally { RM.busy = false; }
  if (tab === 'shared') rmDraw();
}
function sharedView() {
  const root = h('div', { class: 'rmwrap' }, h('div', { class: 'rmside', id: 'rmside' }), h('div', { class: 'rmmain', id: 'rmmain' }));
  clearInterval(RM.timer);
  setTimeout(async () => { await rmLoadList(); if (!RM.sel && RM.list[0]) rmSelect(RM.list[0].key); rmDraw(); }, 0);
  RM.timer = setInterval(() => { if (tab !== 'shared') { clearInterval(RM.timer); return; } if (RM.sel) rmPoll(false); else rmLoadList().then(rmDraw); }, 2500);
  return root;
}
function rmSelect(key) { RM.sel = key; RM.view = null; RM.seq = 0; RM.sig = ''; RM.form = null; rmPoll(true); rmDraw(); }

function rmDraw() {
  const side = $('#rmside'), main = $('#rmmain'); if (!side || !main) return;
  side.innerHTML = '';
  side.append(h('div', { class: 'row' }, h('button', { class: 'primary sm grow', onclick: () => { RM.form = 'new'; RM.sel = null; RM.view = null; rmDraw(); } }, '+ Share a chat'), h('button', { class: 'ghost sm', onclick: () => { RM.form = 'join'; RM.sel = null; RM.view = null; rmDraw(); } }, 'Join')));
  for (const r of RM.list) side.append(h('div', { class: 'item' + (r.key === RM.sel ? ' on' : ''), onclick: () => rmSelect(r.key) },
    h('div', { class: 'ititle' }, r.pending ? h('span', { class: 'dot' }) : null, h('span', { class: 'ellip' }, r.title)),
    h('div', { class: 'small muted' }, r.kind === 'host' ? `You host · ${r.members} ${r.members === 1 ? 'person' : 'people'}` + (r.pending ? ` · ${r.pending} waiting` : '') : `With ${r.host || 'a friend'}`)));
  if (!RM.list.length) side.append(h('p', { class: 'small muted' }, 'No shared chats yet. Start one, or join with an invite from a friend.'));
  main.innerHTML = '';
  if (RM.form === 'new') return main.append(rmNewForm());
  if (RM.form === 'join') return main.append(rmJoinForm());
  const e = rmEntry(), v = RM.view;
  if (!e) return main.append(h('div', { class: 'empty' }, h('h2', {}, 'Work together'), h('p', { class: 'muted' }, 'Share a chat with a friend. You each use your own Claude account, and you can ask each other to run something on the other’s account, with a question first.')));
  if (!v) return main.append(h('div', { class: 'muted small', style: 'padding:20px' }, RM.err || 'Loading…'));
  main.append(rmRoom(e, v));
}

async function rmNewHost() { const m = await rmHostMachine(); return m; }
function rmNewForm() {
  const title = h('input', { placeholder: 'Name, e.g. Landing page with Sam' }), folder = h('input', { placeholder: '/path/to/project on your server', style: 'font-family:ui-monospace,monospace' }), err = h('div', { class: 'small err' });
  (async () => { const m = await rmHostMachine(); if (!m) return; try { const info = await cm.relay(m.id, 'GET', '/projects'); folder.value = (SS.lastCwd && SS.lastCwd.startsWith('/') && SS.lastCwd) || ((info.projects || [])[0] || {}).path || ''; } catch { /* type it */ } })();
  const go = async () => {
    try { const m = await rmHostMachine(); if (!m) throw new Error('Connect a server first.'); const r = await cm.relay(m.id, 'POST', '/rooms', { title: title.value, cwd: folder.value.trim() }); await rmLoadList(); rmSelect('h:' + r.id); }
    catch (e) { err.textContent = clean(e); }
  };
  return h('div', { class: 'rmform stack' }, h('h2', {}, 'Share a chat'), h('p', { class: 'muted' }, 'Your Claude works in this folder on your server. Friends join with an invite and see everything said here. Nobody gets your login: they use their own account, or they ask you.'),
    title, folder, h('div', { class: 'row' }, h('button', { class: 'primary', onclick: go }, 'Create'), h('button', { class: 'ghost', onclick: () => { RM.form = null; rmDraw(); } }, 'Cancel')), err);
}
function rmJoinForm() {
  const link = h('input', { placeholder: 'Paste the invite link from your friend' }), name = h('input', { placeholder: 'Your name', value: localStorage.getItem('cm.myName') || '' }), err = h('div', { class: 'small err' });
  const go = async () => {
    try {
      const u = new URL(link.value.trim().replace(/^hearth:\/\/room/, 'https://x/room'));
      const url = u.searchParams.get('url'), invite = u.searchParams.get('invite'); if (!url || !invite) throw new Error('That is not an invite link.');
      const nm = name.value.trim() || 'Guest'; localStorage.setItem('cm.myName', nm);
      const r = await cm.roomCall({ url, token: '' }, 'POST', '/rooms/join', { invite, name: nm });
      const l = rmJoined(); l.push({ id: r.room, member: r.member, url, token: r.token, title: r.title, host: r.host }); rmSaveJoined(l);
      RM.form = null; await rmLoadList(); rmSelect('g:' + r.room + ':' + r.member);
    } catch (e) { err.textContent = /Invalid URL/.test(String(e.message)) ? 'That is not an invite link.' : clean(e); }
  };
  return h('div', { class: 'rmform stack' }, h('h2', {}, 'Join a shared chat'), link, name, h('div', { class: 'row' }, h('button', { class: 'primary', onclick: go }, 'Join'), h('button', { class: 'ghost', onclick: () => { RM.form = null; rmDraw(); } }, 'Cancel')), err);
}

const RM_MODES = [['plan', 'Plan only: can read, changes nothing'], ['acceptEdits', 'Edits: can change files'], ['manual', 'Ask me at every step'], ['bypassPermissions', 'Full auto: anything, without asking']];
function rmRoom(e, v) {
  const host = e.kind === 'host', box = h('div', { class: 'rmroom' });
  box.append(h('div', { class: 'chat-head' }, h('div', { class: 'grow', style: 'min-width:0' }, h('h3', { class: 'ellip' }, v.title),
    h('div', { class: 'small muted ellip' }, [host ? 'You host this chat' : 'Hosted by ' + v.host, v.members.map((m) => m.name).join(', ') || 'only you so far'].join(' · '))),
    host ? h('button', { class: 'ghost sm', onclick: () => rmInvite(e) }, 'Invite') : h('button', { class: 'ghost sm', onclick: async () => { if (!confirm('Leave this shared chat?')) return; try { await rmCall(e, 'POST', `/${e.id}/leave`, {}); } catch { /* gone anyway */ } rmSaveJoined(rmJoined().filter((j) => !(j.id === e.id && j.member === e.member))); RM.sel = null; await rmLoadList(); rmDraw(); } }, 'Leave'),
    host ? h('button', { class: 'ghost sm', onclick: async () => { if (!confirm('Delete this shared chat for everyone?')) return; try { await rmCall(e, 'DELETE', `/${e.id}`); } catch (er) { toast(clean(er)); } RM.sel = null; await rmLoadList(); rmDraw(); } }, 'Delete') : null));
  const msgs = h('div', { class: 'msgs', id: 'rmmsgs' }), col = h('div', { class: 'col' });
  for (const m of v.messages) {
    if (m.kind === 'status') { col.append(h('div', { class: 'meta' }, m.text)); continue; }
    const mine = m.from === v.me;
    if (m.kind === 'assistant') col.append(h('div', { class: 'rmmsg claude' }, h('div', { class: 'small muted' }, m.name + (m.via === 'host' ? ' · on the host’s account' : m.via === 'own' ? ' · own account' : '')), (() => { const d = h('div', { class: 'md' }); d.append(md(m.text)); return d; })()));
    else col.append(h('div', { class: 'rmmsg' + (mine ? ' mine' : '') }, h('div', { class: 'small muted' }, m.name + (m.via === 'ask-host' ? ' → asks the host’s Claude' : '')), h('div', { class: 'msg user', style: 'align-self:auto;max-width:100%' }, m.text)));
  }
  for (const w of v.working) col.append(h('div', { class: 'meta' }, w.name + ' is working…'));
  msgs.append(col);
  const stick = (() => { const o = $('#rmmsgs'); return !o || o.scrollHeight - o.scrollTop - o.clientHeight < 140; })();
  box.append(msgs);
  if (host) for (const q of v.requests.filter((x) => x.status === 'pending')) box.append(rmRequestCard(e, v, q));
  const guestReqs = !host ? v.requests.filter((x) => x.status === 'pending') : [];
  if (guestReqs.length) box.append(h('div', { class: 'small muted', style: 'padding:0 26px' }, `${guestReqs.length} request${guestReqs.length > 1 ? 's' : ''} waiting for ${v.host} to approve…`));
  box.append(rmComposer(e, v));
  const keepTa = $('#rm-text'), draft = keepTa ? keepTa.value : '';
  queueMicrotask(() => { const o = $('#rmmsgs'); if (o && stick) o.scrollTop = o.scrollHeight; const ta = $('#rm-text'); if (ta && draft) ta.value = draft; });
  if (host) box.append(rmMembers(e, v));
  return box;
}
function rmRequestCard(e, v, q) {
  const ta = h('textarea', { rows: 3, class: 'rmedit' }); ta.value = q.text;
  const mode = h('select', { class: 'sel' }, RM_MODES.map(([k, l]) => h('option', { value: k }, l)));
  const always = h('input', { type: 'checkbox' });
  const decide = async (decision) => { try { await rmCall(e, 'POST', `/${e.id}/requests/${q.id}`, { decision, text: ta.value, mode: mode.value, always: always.checked }); rmPoll(true); } catch (er) { toast(clean(er)); } };
  return h('div', { class: 'perm pc rmreq' }, h('div', { class: 'label' }, `${q.name} wants to ask your Claude`), h('div', { class: 'small muted' }, 'It runs on your account and your server. You can change the prompt first.'), ta,
    h('div', { class: 'row wrap' }, mode, h('label', { class: 'row small' }, always, h('span', {}, `Always run ${q.name}'s prompts without asking (full permission)`))),
    h('div', { class: 'row' }, h('button', { class: 'primary sm', onclick: () => decide('approve') }, 'Run it'), h('button', { class: 'ghost sm', onclick: () => decide('deny') }, 'Deny')));
}
function rmMembers(e, v) {
  if (!v.members.length) return h('div');
  return h('div', { class: 'rmmembers' }, h('div', { class: 'label' }, 'People in this chat'), ...v.members.map((m) => {
    const g = m.grant || { auto: false, mode: 'plan' };
    const set = async (grant) => { try { await rmCall(e, 'POST', `/${e.id}/members/${m.id}`, { grant }); rmPoll(true); } catch (er) { toast(clean(er)); } };
    const sel = h('select', { class: 'sel', onchange: (ev) => set({ auto: g.auto, mode: ev.target.value }) }, RM_MODES.map(([k, l]) => h('option', { value: k, selected: g.mode === k }, l.split(':')[0])));
    return h('div', { class: 'row wrap' }, h('b', { class: 'grow' }, m.name), h('label', { class: 'row small' }, (() => { const c = h('input', { type: 'checkbox' }); c.checked = g.auto; c.onchange = () => set({ auto: c.checked, mode: g.mode }); return c; })(), h('span', {}, 'Runs without asking')), sel,
      h('button', { class: 'ghost sm', onclick: async () => { if (!confirm('Remove ' + m.name + '?')) return; try { await rmCall(e, 'DELETE', `/${e.id}/members/${m.id}`); rmPoll(true); } catch (er) { toast(clean(er)); } } }, 'Remove'));
  }));
}
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

function rmComposer(e, v) {
  const host = e.kind === 'host', ta = h('textarea', { id: 'rm-text', rows: 2, placeholder: host ? 'Ask your Claude, or just talk to the others…' : 'Write something…' });
  ta.onkeydown = (ev) => { if (ev.key === 'Enter' && !ev.shiftKey && !ev.isComposing) { ev.preventDefault(); (host ? askHost : sendOwn)(); } };
  const take = () => { const t = ta.value.trim(); if (!t) return ''; ta.value = ''; return t; };
  const post = (t, extra) => rmCall(e, 'POST', `/${e.id}/messages`, { text: t, ...(extra || {}) });
  const say = async () => { const t = take(); if (!t) return; try { await post(t); rmPoll(false); } catch (er) { toast(clean(er)); ta.value = t; } };
  const askHost = async () => { const t = take(); if (!t) return; try { await rmCall(e, 'POST', `/${e.id}/ask`, { text: t }); rmPoll(false); } catch (er) { toast(clean(er)); ta.value = t; } };
  const requestHost = async () => { const t = take(); if (!t) return; try { await rmCall(e, 'POST', `/${e.id}/requests`, { text: t }); rmPoll(false); toast(v.grant && v.grant.auto ? `${v.host} lets you run prompts at once.` : `Sent to ${v.host}. They can change it before it runs.`); } catch (er) { toast(clean(er)); ta.value = t; } };
  const sendOwn = async () => { const t = take(); if (!t) return; await rmRunOwn(e, v, t); };
  const row = h('div', { class: 'row wrap rmbtns' },
    host ? h('button', { class: 'primary sm', onclick: askHost, title: 'Runs on your account and your server' }, 'Ask my Claude') : h('button', { class: 'primary sm', onclick: sendOwn, title: 'Runs on your own Claude account, on your own computer or server' }, 'Send with my account'),
    !host ? h('button', { class: 'ghost sm', onclick: requestHost, title: `${v.host} sees your prompt first and can change it` }, `Ask ${v.host}’s Claude`) : null,
    h('button', { class: 'ghost sm', onclick: say }, 'Just say it'));
  const note = !host ? h('div', { class: 'small muted' }, 'Asking ' + v.host + '’s Claude uses their account. ' + (v.grant && v.grant.auto ? 'They gave you full permission: it runs at once.' : 'They decide, and can change your prompt first.')) : null;
  const own = !host ? rmOwnSettings(e) : null;
  return h('div', { class: 'composer rmcomposer' }, h('div', { class: 'cbox' }, ta, row), note, own);
}
function rmOwnSettings(e) {
  const key = e.key, cfg = rmCfg()[key] || {};
  const inp = h('input', { placeholder: 'Folder for your Claude, on your computer (empty = just talk, no files)', value: cfg.cwd || '', style: 'font-family:ui-monospace,monospace;font-size:12px', onchange: (ev) => { const c = rmCfg(); c[key] = { ...(c[key] || {}), cwd: ev.target.value.trim() }; localStorage.setItem('cm.roomCfg', JSON.stringify(c)); } });
  return h('div', { class: 'small muted' }, 'Your Claude works in: ', inp);
}
/** The guest's own account: the prompt runs on their own computer or server, and the answer is posted into the shared chat. */
async function rmRunOwn(e, v, text) {
  if (RM.ownBusy) { toast('Your Claude is still working on the last one.'); return; }
  const cfg = rmCfg()[e.key] || {};
  const mid = (typeof defaultMachineId === 'function' && defaultMachineId()) || null;
  if (!SS.machines.length) SS.machines = await cm.machines();
  const machine = (SS.machines.find((m) => m.id === mid) || SS.machines.find((m) => !m.secure) || SS.machines[0]);
  if (!machine) { toast('Connect your own computer or server first (Settings).'); return; }
  const sess = rmSess(), last = (sess[e.key] || {});
  const seen = last.seen || 0, others = v.messages.filter((m) => m.n > seen && m.from !== v.me && m.kind !== 'status').slice(-40);
  const people = [v.host, ...v.members.map((m) => m.name)].join(', ');
  const prompt = `You are in a shared chat called "${v.title}" with ${people}. Answer ${v.myName} directly.` + (others.length ? '\n\nWhat was said in the chat since your last turn:\n' + others.map((m) => `${m.name}: ${m.text}`).join('\n') : '') + `\n\n${v.myName}: ${text}`;
  RM.ownBusy = true;
  try { await rmCall(e, 'POST', `/${e.id}/messages`, { text }); await rmCall(e, 'POST', `/${e.id}/working`, { on: true }); } catch (er) { RM.ownBusy = false; toast(clean(er)); return; }
  rmPoll(false);
  const sid = rand(); let out = '', failed = '';
  const finish = async () => {
    delete chatHandlers[sid]; RM.ownBusy = false;
    const s = rmSess(); s[e.key] = { id: s[e.key] && s[e.key].id || last.id, seen: v.seq }; localStorage.setItem('cm.roomSess', JSON.stringify(s));
    try { if (out.trim() || failed) await rmCall(e, 'POST', `/${e.id}/messages`, { text: out.trim() || `(${failed})`, kind: 'assistant' }); } catch { /* host gone */ }
    try { await rmCall(e, 'POST', `/${e.id}/working`, { on: false }); } catch { /* ignore */ }
    rmPoll(false);
  };
  chatHandlers[sid] = (ev) => {
    if (ev.t === 'session' && ev.id) { const s = rmSess(); s[e.key] = { ...(s[e.key] || {}), id: ev.id }; localStorage.setItem('cm.roomSess', JSON.stringify(s)); }
    else if (ev.t === 'delta') out += ev.text;
    else if (ev.t === 'fail') { failed = ev.text || 'Your Claude failed'; finish(); }
    else if (ev.t === 'end') finish();
  };
  const talk = !cfg.cwd;
  cm.send(machine.id, last.id || null, prompt, 'acceptEdits', false, sid, talk ? { kind: 'talk' } : { cwd: cfg.cwd }).catch((er) => { failed = clean(er); finish(); });
}
