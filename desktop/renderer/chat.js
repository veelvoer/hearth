'use strict';
/* Chats: Claude Code sessions as a chat app. Projects on the left, conversation on the right. */
const SS = { machines: [], mid: null, live: {}, list: [], err: null, sel: null, msgs: [], parts: [], sending: false, mode: 'acceptEdits', model: '',
  search: '', closed: {}, found: [], pairing: false, voice: null, timers: [], chatErr: null, open: new Set(), lastCwd: '', projects: {}, effort: '', call: 'off', perms: [], queue: [], lastRun: null, opts: false, files: [], status: {} };
const chatHandlers = {};
cm.on('stream', (ev) => { const f = chatHandlers[ev.sid] || voiceHandlers[ev.sid]; if (f) f(ev); });

const short = (p) => (p || '').replace(/\/$/, '').split('/').slice(-2).join('/');
const folderName = (p) => (p || '').replace(/\/$/, '').split('/').pop() || p || 'Unknown';
const relTime = (ms) => { const d = Date.now() - ms; return d < 90e3 ? 'now' : CMW.dur(d); };
const rand = () => Math.random().toString(36).slice(2);

// ───────── sidebar ─────────
function projects() {
  const seen = new Map();
  for (const s of SS.list) if (s.cwd && !seen.has(s.cwd)) seen.set(s.cwd, s.mtime);
  return [...seen.entries()].sort((a, b) => b[1] - a[1]).map((e) => e[0]);
}

/** Sessions from every connected computer in one list, newest project first. */
/** One entry per chat. A chat that exists on several computers (synced) is shown once, on the one that is working on it,
 *  else the one with the newest copy, and on a tie your own computer over the server. */
function allSessions() {
  // The server holds every chat (synced), so when it answers its list is the list, identical on every device.
  // A computer's own list only fills in what the server doesn't have (while it is unreachable or not yet synced).
  const hub = SS.machines.find((m) => m.secure && Array.isArray(SS.live[m.id]));
  const out = new Map();
  if (hub) for (const x of SS.live[hub.id]) out.set(x.id, { ...x, mid: hub.id, secure: true });
  for (const m of SS.machines) {
    if (hub && m.id === hub.id) continue;
    for (const x of (SS.live[m.id] || (m.id === SS.mid ? SS.list : []) || [])) {
      const cur = out.get(x.id);
      if (!cur || (!cur.running && x.running) || (!hub && x.mtime - cur.mtime > 2500)) out.set(x.id, { ...x, mid: m.id, secure: !!m.secure });
    }
  }
  for (const g of SS.guests || []) for (const x of SS.live[g.id] || []) out.set('guest:' + g.share, { ...x, mid: g.id, secure: !!g.secure, guest: true, guestHost: g.hostName || '' });   // shared chats of friends
  return [...out.values()];
}
/** Which computer should answer in this chat right now? Re-checked on every send, so it follows you when your laptop comes online. */
function machineFor(id, fallback) {
  const hit = allSessions().find((x) => x.id === id);
  return hit ? hit.mid : fallback;
}
const machineName = (mid) => (SS.machines.find((m) => m.id === mid) || {}).name || '';
const multi = () => SS.machines.length > 1;

function chatItem(s) {
  const on = SS.sel && SS.sel.id === s.id;
  return h('div', { class: 'item' + (on ? ' on' : ''), onclick: () => openSession(s) },
    h('div', { class: 'ititle' }, (s.running || s.busy) ? h('span', { class: 'dot' }) : null, h('span', { class: 'ellip' }, s.title)),
    h('div', { class: 'small muted' }, ((s.running || s.busy) ? 'working now' : relTime(s.mtime)) + (s.guest ? ' · shared by ' + (s.guestHost || 'a friend') : s.shared >= 0 ? ' · shared' + (s.shared ? ' with ' + s.shared : '') : '') + (s.elsewhere ? ' · only on ' + s.elsewhere : '')));
}

const groupKey = (s) => (s.guest || s.shared >= 0 ? 'Shared' : s.talk ? 'Talks' : (s.cwd || 'Other'));   // shared chats are kept apart from the others
/** The projects as tabs along the top, and the chats of the open project in the column beside the chat. Searching looks through every project. */
function drawSide() {
  const side = SS.side;
  if (!side) return;
  const keep = side.scrollTop, hadFocus = document.activeElement && document.activeElement.id === 'chat-search';
  const q = SS.search.toLowerCase();
  const all = allSessions();
  const groups = new Map();
  for (const s of all) { const k = groupKey(s); if (!groups.has(k)) groups.set(k, []); groups.get(k).push(s); }
  const ordered = [...groups.entries()].sort((a, b) => Math.max(...b[1].map((x) => x.mtime)) - Math.max(...a[1].map((x) => x.mtime)));
  if (SS.sel && !SS.sel.isNew && SS.sel.cwd && groups.has(SS.sel.cwd) && SS.projFor !== SS.sel) { SS.projKey = SS.sel.cwd; SS.projFor = SS.sel; }   // opening a chat shows its project
  if (!groups.has(SS.projKey)) SS.projKey = ordered[0] ? ordered[0][0] : null;
  if (SS.tabsEl) {
    SS.tabsEl.innerHTML = '';
    for (const [key, items] of ordered) SS.tabsEl.append(h('button', { class: 'ptab' + (key === SS.projKey ? ' on' : ''), title: key, onclick: () => { SS.projKey = key; SS.search = ''; drawSide(); } },
      folderName(key), items.some((x) => x.running || x.busy) ? h('i', { class: 'pdot' }) : null));
    SS.tabsEl.append(h('button', { class: 'ptab add', title: 'Start a chat in another project', onclick: newChat }, '+ project'), h('button', { class: 'ptab add', title: 'Open a chat a friend shared with you', onclick: shJoinDialog }, 'Join a shared chat'));
  }
  side.innerHTML = '';
  side.append(h('input', { id: 'chat-search', placeholder: 'Search chats', value: SS.search, oninput: (e) => { SS.search = e.target.value; drawSide(); } }));
  const list = (q ? all.filter((s) => s.title.toLowerCase().includes(q) || (s.cwd || '').toLowerCase().includes(q)) : (groups.get(SS.projKey) || [])).sort((a, b) => b.mtime - a.mtime);
  list.forEach((s) => side.append(chatItem(s)));
  if (!list.length && !SS.err) side.append(h('p', { class: 'small muted' }, q ? 'No chat matches.' : SS.machines.length ? 'No chats yet. Start one with “New chat”.' : 'No computer connected yet. Add one in Settings.'));
  if (SS.err) side.append(h('div', { class: 'small err' }, SS.err));
  side.scrollTop = keep;
  if (hadFocus) { const i = $('#chat-search'); if (i) { i.focus(); i.setSelectionRange(i.value.length, i.value.length); } }
}

// ───────── data ─────────
async function loadList() {
  if (!SS.mid) return;
  try { SS.list = await cm.relay(SS.mid, 'GET', '/sessions'); SS.err = null; } catch (e) { SS.err = clean(e); }
  drawSide();
}
async function pollLive() {
  try { SS.guests = await cm.guests(); } catch { SS.guests = SS.guests || []; }
  for (const m of [...SS.machines, ...(SS.guests || [])]) {
    try { SS.live[m.id] = await cm.relay(m.id, 'GET', '/sessions'); if (m.id === SS.mid) { SS.list = SS.live[m.id]; SS.err = null; } }
    catch (e) { SS.live[m.id] = null; if (m.id === SS.mid) SS.err = clean(e); }
  }
  if (tab === 'sessions') drawSide();
}
/** Messages you sent that the computer's copy of the chat does not show yet (still travelling, or queued for a laptop that is off). They stay visible until they show up. */
function withUnsent(list, sel) {
  const norm = (t) => String(t || '').replace(/\s+/g, ' ').trim();
  const seen = list.filter((x) => x.role === 'user').slice(-40).map((x) => norm(x.text));
  SS.unsent = (SS.unsent || []).filter((u) => (u.sel === sel || (u.id && u.id === sel.id)) && Date.now() - u.at < 30 * 60e3 && !seen.some((t) => t === norm(u.text) || (u.text.length > 24 && (t.includes(norm(u.text)) || norm(u.text).includes(t) && t.length > 24))));
  return SS.unsent.length ? [...list, ...SS.unsent.map((u) => ({ role: 'user', text: u.text, pending: true }))] : list;
}
async function loadMsgs(quiet) {
  const s = SS.sel;
  if (!s || !s.id) return;
  try {
    const raw = await cm.relay(s.mid, 'GET', `/sessions/${s.id}/messages?tail=400`);
    if (SS.sel !== s) return;
    const m = withUnsent(raw, s);
    const last = (a) => a.length ? a[a.length - 1] : {};
    if (!quiet || m.length !== SS.msgs.length || last(m).text !== last(SS.msgs).text || last(m).result !== last(SS.msgs).result) { SS.msgs = m; renderMsgs(); }
    SS.chatErr = null;
  } catch (e) { SS.chatErr = clean(e); }
}

/** Your own computers come first; a server is only preselected if nothing else is connected. */
function defaultMachineId() {
  const own = SS.machines.filter((m) => !m.secure);
  const last = localStorage.getItem('cm.lastMachine');
  return (own.find((m) => m.id === last) || own.find((m) => m.local) || own[0] || SS.machines[0] || {}).id || null;
}

function sessionsView() {
  SS.timers.forEach(clearInterval); SS.timers = [];
  const root = h('div', { class: 'chatwrap' }), split = h('div', { class: 'split' });
  SS.tabsEl = h('div', { class: 'ptabs' }); SS.side = h('div', { class: 'side' }); SS.pane = h('div', { class: 'pane' });
  split.append(SS.side, SS.pane); root.append(SS.tabsEl, split);
  drawSide(); drawPane();
  (async () => {
    SS.machines = await cm.machines(); try { SS.guests = await cm.guests(); } catch { SS.guests = []; }
    if (!SS.mid || !SS.machines.some((m) => m.id === SS.mid)) SS.mid = defaultMachineId();
    drawSide(); await loadList(); await pollLive();
  })();
  setTimeout(() => { if (SS.sel) { renderMsgs(); renderPerms(); renderFiles(); } }, 0);  // the pane is in the document now
  SS.timers.push(setInterval(shLoadGuestInfo, 3000), setInterval(pollLive, 8000), setInterval(() => { if (SS.sel && SS.sel.id && !SS.sending && !SS.voice) loadMsgs(true); }, 3000), setInterval(refreshBusy, 4000));
  return root;
}

function swapPane() { if (!SS.pane) return; SS.pane.classList.remove('swap'); void SS.pane.offsetWidth; SS.pane.classList.add('swap'); }
function openSession(s, voice) {
  if (document.startViewTransition && !matchMedia('(prefers-reduced-motion: reduce)').matches && SS.pane) { try { const t = document.startViewTransition(() => openSessionNow(s, voice)); [t.ready, t.finished, t.updateCallbackDone].forEach((x) => x.catch(() => {})); return; } catch { /* plain */ } }
  openSessionNow(s, voice);
}
/** Runs that are going on, by chat. Switching to another chat never stops or mixes them up. */
const RUNS = new Map(), QUEUES = new Map();
const qOf = (s) => { const k = (s && s.id) || 'new'; let q = QUEUES.get(k); if (!q) { q = []; QUEUES.set(k, q); } return q; };
const sameChat = (a, b) => !!a && !!b && (a === b || (!!a.id && a.id === b.id));
/** Claude is working on the chat I am looking at, started somewhere else (the phone, the terminal, another computer). */
function remoteWorking() {
  const s = SS.sel;
  if (!s || !s.id || SS.sending) return false;
  const x = allSessions().find((y) => y.id === s.id);
  return !!(x && (x.running || x.busy));
}
const remoteRunning = () => { const s = SS.sel, x = s && s.id && allSessions().find((y) => y.id === s.id); return !!(x && x.running); };
/** Keeps "working" honest while a chat is open: asks the computer, and starts a queued message once Claude is free. */
async function refreshBusy() {
  const s = SS.sel;
  if (!s || !s.id || SS.sending || tab !== 'sessions') return;
  const before = remoteWorking();
  try { SS.live[s.mid] = await cm.relay(s.mid, 'GET', '/sessions'); } catch { return; }
  if (!sameChat(SS.sel, s)) return;
  const now = remoteWorking();
  if (before !== now) {
    const had = document.activeElement && document.activeElement.id === 'composer-text';
    renderLive(); drawSide(); drawPane();
    if (had) { const t = $('#composer-text'); if (t) { t.focus(); t.setSelectionRange(t.value.length, t.value.length); } }
    if (!now) loadMsgs(false);
  }
  if (!now && SS.queue.length) sendMsg(SS.queue.shift());
}
function openSessionNow(s, voice) {
  SS.call = 'off';
  SS.sel = { id: s.id, title: s.title, cwd: s.cwd, mid: s.mid || SS.mid, live: s.live }; SS.mid = SS.sel.mid; SS.msgs = []; SS.shared = null; SS.shareInfo = null; SS.sharedPeople = [];
  const run = SS.sel.id && RUNS.get(SS.sel.id); SS.parts = run ? run.parts : []; SS.sending = !!run; SS.queue = qOf(SS.sel);   // a run that is still going shows up again
  if (tab !== 'sessions') setTab('sessions');
  drawSide(); drawPane(); swapPane();
  loadMsgs(false).then(() => { if (voice) startVoice(); });
  loadCall();
}
async function loadCall() {
  const s = SS.sel;
  if (!s || !s.id) return;
  try { const pr = await cm.relay(s.mid, 'GET', `/sessions/${s.id}/prefs`); SS.call = pr.call || 'off'; SS.run = pr.run || 'auto'; } catch { SS.call = null; }  // null: this relay has no call preferences
  if (SS.sel === s) drawPane();
}
async function setRun(v) {
  const s = SS.sel;
  if (!s || !s.id) return;
  SS.run = v; drawPane();
  try { await cm.relay(s.mid, 'POST', `/sessions/${s.id}/prefs`, { run: v }); } catch (e) { SS.chatErr = clean(e); }
}
async function setCall(v) {
  const s = SS.sel;
  if (!s || !s.id) return;
  const nv = SS.call === v ? 'off' : v;
  SS.call = nv; drawPane();
  try { await cm.relay(s.mid, 'POST', `/sessions/${s.id}/prefs`, { call: nv }); } catch (e) { SS.chatErr = clean(e); }
}
function newChat() {
  if (SS.sel && !SS.sel.isNew) SS.prevSel = SS.sel;   // so the back button can return to it
  const mid = defaultMachineId() || SS.mid;
  SS.sel = { id: null, isNew: true, title: 'New chat', cwd: '', mid }; SS.runOn = 'vps';
  SS.mid = mid; SS.perms = [];
  const own = (SS.live[mid] || []).filter((x) => x.cwd).sort((a, b) => b.mtime - a.mtime)[0];
  SS.sel.cwd = (SS.lastCwdBy && SS.lastCwdBy[mid]) || (own && own.cwd) || '';
  SS.msgs = []; SS.parts = []; SS.sending = false; SS.chatErr = null; SS.queue = qOf(SS.sel);
  if (tab !== 'sessions') setTab('sessions');
  drawSide(); drawPane(); swapPane();
  const t = $('#composer-text'); if (t) t.focus();
}
document.addEventListener('keydown', (e) => { if (tab === 'sessions' && (e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'n') { e.preventDefault(); newChat(); } });

// ───────── pane ─────────
SS.rightOff = localStorage.getItem('cm.rightOff') === '1';
const SUGGEST = ['Explain how this project is organized', 'Find and fix bugs in the recent changes', 'Write tests for the most important code', 'Review my uncommitted changes'];

function drawPane() {
  const pane = SS.pane;
  if (!pane) return;
  pane.innerHTML = '';
  const s = SS.sel;
  if (!s) {
    pane.append(h('div', { class: 'empty' }, h('div', { html: sparkSVG(54) }), h('h2', {}, 'Chat with Claude Code'),
      h('p', { class: 'muted' }, 'Pick a chat on the left, or start a new one in any project.'), h('button', { class: 'primary', onclick: newChat }, 'New chat')));
    return;
  }
  const guest = isGuestMid(s.mid), mach = SS.machines.find((x) => x.id === s.mid) || (guest ? { guest: true, ...((SS.guests || []).find((g) => g.id === s.mid) || {}) } : {});
  const me = allSessions().find((x) => x.id === s.id) || {};
  const working = SS.sending || remoteWorking() || !!(me.running || me.busy);
  const main = h('div', { class: 'chatmain' });
  if (s.isNew) main.append(h('button', { class: 'ghost sm backbtn', onclick: backToChats }, '← All chats'));
  main.append(h('div', { class: 'chat-head' }, h('div', { class: 'grow', style: 'min-width:0' }, h('h3', { class: 'ellip' }, s.title),
    h('div', { class: 'small muted ellip', title: s.cwd }, [s.cwd ? folderName(s.cwd) : '', modelLabel(me.model), me.root || s.root ? 'root' : '', multi() ? mach.name : '', SS.call && SS.call !== 'off' ? 'will call you' : '', working ? 'working now' : ''].filter(Boolean).join(' · '))),
    working ? h('i', { class: 'pdot big', title: 'Claude is working' }) : null,
    s.id && (guest || me.shared >= 0) ? h('button', { class: 'ghost sm', title: 'Whose usage do I use in this chat?', onclick: () => shOptions(s) }, 'Options') : null,
    s.id && !guest ? ibtn('shared', s.id && me.shared >= 0 ? 'Shared: who can open this chat' : 'Share this chat with a friend', () => shOpenShare(s), 'ghost sm') : null,
    s.id && !guest ? ibtn('mic', 'Talk with your voice', startVoice, 'ghost sm') : null,
    s.id && !guest ? ibtn('addons', 'Add-ons: skills, plugins, connections, changes', () => openTools(s, 'skills'), 'ghost sm') : null,
    s.id ? ibtn(SS.rightOff ? 'sliders' : 'check', SS.rightOff ? 'Show the side panel: phone calls, where it runs, folder' : 'Hide the side panel', () => { SS.rightOff = !SS.rightOff; localStorage.setItem('cm.rightOff', SS.rightOff ? '1' : ''); drawPane(); }, 'ghost sm') : null));
  main.append(h('div', { class: 'msgs', id: 'msgs' }, h('div', { class: 'col', id: 'col' })));
  if (guest || me.shared >= 0) main.append(h('div', { id: 'shin', class: 'shin' }), h('div', { id: 'shstatus', class: 'shstatus' }));
  main.append(h('div', { id: 'perms', class: 'perms' }));
  main.append(composer());
  pane.append(main);
  if (s.id && !SS.rightOff) {
    const rc = (title, ...kids) => { const k = kids.filter(Boolean); return k.length ? h('div', { class: 'rcard' }, h('div', { class: 'label' }, title), ...k) : null; };
    const runRow = SS.call !== null && (SS.machines.find((m) => m.id === s.mid) || {}).secure ? h('div', { class: 'row wrap' }, ...runChips((SS.run || 'auto'), setRun),
      h('span', { class: 'small muted' }, 'Pick the computer that works on this chat. Auto: whichever of your computers is online, otherwise the server. You can also just ask Claude to do something on another PC.')) : null;
    const callRow = SS.call !== null ? h('div', { class: 'stack', style: 'gap:8px' }, h('div', { class: 'row wrap' }, chip("When it's done", SS.call === 'done', () => setCall('done')), chip('If it needs me', SS.call === 'attention', () => setCall('attention'))),
      h('span', { class: 'small muted' }, 'You always get a message. A call only happens if you pick one here.')) : null;
    const tools = SS.msgs.filter((m) => m.role === 'tool').length;
    if (guest) pane.append(h('div', { class: 'rightcol' }, rc('Shared chat', h('div', { class: 'small' }, `This chat belongs to ${mach.hostName || 'a friend'}. You can read everything.`), h('div', { class: 'small muted' }, `What you send is not run at once: ${mach.hostName || 'they'} gets a notification, can change it and runs it on their account, or says no.`),
      SS.shared && SS.shared.grant && SS.shared.grant.auto ? h('div', { class: 'small', style: 'color:var(--clay)' }, 'You have full permission: what you send runs at once.') : null,
      h('div', {}, h('button', { class: 'ghost sm', onclick: () => { if (!confirm('Leave this shared chat?')) return; rsSet('g:' + mach.share, null); SS.sel = null; shApply().then(() => { pollLive(); drawSide(); drawPane(); }); } }, 'Leave'))))); else
    pane.append(h('div', { class: 'rightcol' },
      rc('Phone call', callRow), rc('Run on', runRow),
      rc('Folder', mach.local && s.cwd ? h('div', { class: 'stack', style: 'gap:8px' }, h('div', { class: 'mono small ellip', title: s.cwd }, s.cwd), h('div', {}, h('button', { class: 'ghost sm', onclick: () => cm.openPath(s.cwd) }, 'Open folder'))) : (s.cwd ? h('div', { class: 'mono small', style: 'overflow-wrap:anywhere' }, s.cwd) : null)),
      rc('Move to server', moveBox(s, mach)), rc('Continue here', continueBox(s, mach)),
      s.id && !guest && (me.root || s.root || (SS.sopts && SS.sopts[s.mid] && SS.sopts[s.mid].root)) ? rc('Root rights', h('div', { class: 'small' }, me.root ? 'This chat runs as root. Claude knows it has full administrator rights.' : 'This chat runs without root rights.'),
        h('div', {}, h('button', { class: 'ghost sm', onclick: async () => { try { await cm.relay(s.mid, 'POST', `/sessions/${s.id}/root`, { on: !me.root }); await pollLive(); drawPane(); toast(me.root ? 'Root rights removed.' : 'This chat runs as root now.'); } catch (e) { toast(clean(e)); } } }, me.root ? 'Take root rights away' : 'Run this chat as root'))) : null,
      me.shared >= 0 ? rc('Sharing', h('div', { class: 'small' }, me.shared ? `Shared with ${me.shared} ${me.shared === 1 ? 'person' : 'people'}. They send, you decide.` : 'Shared, nobody has joined yet.'), h('div', {}, h('button', { class: 'ghost sm', onclick: () => shOpenShare(s) }, 'Manage'))) : null,
      rc('This chat', h('div', { class: 'kv' }, h('span', {}, 'Tokens'), h('b', {}, me.tokens ? fmtN(me.tokens) : '–')), h('div', { class: 'kv' }, h('span', {}, 'Tool calls'), h('b', {}, String(tools))),
        h('div', { class: 'kv' }, h('span', {}, 'Cost at API prices'), h('b', {}, me.cost ? '$' + me.cost.toFixed(2) : '–')))));
  }
  pane.classList.toggle('isnew', !!(s.isNew && !SS.msgs.length));
  renderPerms();
  renderMsgs();
  if (SS.voice && SS.voice.el) pane.append(SS.voice.el);
  if (guest || me.shared >= 0) { shDrawStatus(); shLoadGuestInfo(); }
}

/** Leaves the new-chat screen: back to the chat you came from, or to the list of chats. */
function backToChats() {
  const prev = SS.prevSel; SS.prevSel = null;
  if (prev && prev.id) { openSession(prev); return; }
  SS.sel = null; SS.msgs = []; SS.parts = [];
  drawSide(); drawPane(); swapPane();
}
/** What you typed in a chat stays when you leave it (and when the app closes). */
const draftKey = (s) => s.id || ('new|' + (s.mid || ''));
const drafts = () => { try { return JSON.parse(localStorage.getItem('cm.drafts') || '{}'); } catch { return {}; } };
function saveDraft(s, text) { const d = drafts(); if (text) d[draftKey(s)] = text; else delete d[draftKey(s)]; try { localStorage.setItem('cm.drafts', JSON.stringify(d)); } catch { /* full */ } }
/** Full auto switched on or off while Claude is working: the server allows every approval of this chat (and the ones waiting) at once. A chat that does not exist yet starts in the chosen mode. */
async function setChatAuto(on) {
  const s = SS.sel; if (!s || !s.id) return;
  try { await cm.relay(s.mid, 'POST', `/sessions/${s.id}/auto`, { on }); if (on) { toast('Full auto is on for this chat. Claude asks for nothing now.'); SS.perms = SS.perms.filter((p) => p.ev.session !== s.id); renderPerms(); } }
  catch (e) { toast(clean(e)); }
}
/** The Stop button: ends the run (also one that was started from another device) and says so when there is nothing to stop. */
async function stopClaude() {
  const s = SS.sel; if (!s) return;
  SS.queue.length = 0;
  if (!s.id) { toast('Claude is still starting. Press Stop again in a second.'); return; }
  try {
    const r = await cm.stop(s.mid, s.id);
    if (r && r.ok === false) toast('Nothing to stop here: this chat is not running on ' + (machineName(s.mid) || 'this computer') + '.');
  } catch (e) { toast(clean(e)); }
  setTimeout(() => { if (typeof refreshBusy === 'function') refreshBusy(); }, 700);
}
/** /clear, as in Claude Code: a fresh start with no memory of this chat, in the same project, on the same computer. The old chat stays in the list. */
function clearChat() {
  const s = SS.sel; if (!s) return;
  const keep = { cwd: s.cwd, mid: s.mid, kind: s.kind };
  newChat();
  if (SS.sel) { SS.sel.cwd = keep.cwd; SS.sel.mid = keep.mid; if (keep.kind) SS.sel.kind = keep.kind; SS.mid = keep.mid; drawSide(); drawPane(); }
  toast('Fresh start. The earlier chat is still in your list.');
}
function composer() {
  const s = SS.sel;
  const ta = h('textarea', { id: 'composer-text', rows: 1, placeholder: isGuestMid(s.mid) ? 'Ask the host’s Claude… they approve it first' : SS.sending || remoteWorking() ? 'Claude is working. Type to queue a follow-up…' : s.isNew ? 'Describe what you want to do…' : 'Message Claude' });
  ta.value = drafts()[draftKey(s)] || '';
  const grow = () => { ta.style.height = 'auto'; ta.style.height = Math.min(220, ta.scrollHeight) + 'px'; };
  const menu = h('div', { class: 'slashmenu', hidden: true });
  const APP = [
    ['clear', 'Start fresh in this project, like /clear (this chat stays in the list)', () => clearChat()], ['new', 'Start a new chat', () => newChat()], ['changes', 'See and save what changed', () => openTools(s, 'changes')], ['notes', 'Edit the project notes (CLAUDE.md)', () => openTools(s, 'notes')],
    ['skills', 'Find and use skills', () => openTools(s, 'skills')], ['plugins', 'Add plugins', () => openTools(s, 'plugins')], ['connections', 'Connect Claude to other tools', () => openTools(s, 'connections')], ['plan', 'Plan only: Claude suggests, changes nothing', () => { SS.mode = 'plan'; drawPane(); }],
    ['opus', 'Use the strongest model', () => { SS.model = 'opus'; toast('Model: Opus'); }], ['sonnet', 'Use the balanced model', () => { SS.model = 'sonnet'; toast('Model: Sonnet'); }], ['haiku', 'Use the fastest, cheapest model', () => { SS.model = 'haiku'; toast('Model: Haiku'); }],
    ['talk', 'Talk with your voice', () => startVoice()]].filter(([n]) => s.id || n === 'skills' || n === 'plugins' || n === 'connections' || n === 'plan' || n === 'opus' || n === 'sonnet' || n === 'haiku' || n === 'new');
  const pick = (n, fn) => { if (fn) { ta.value = ''; menu.hidden = true; grow(); fn(); return; } ta.value = '/' + n + ' '; menu.hidden = true; ta.focus(); grow(); };
  const showMenu = async () => {
    const m = /^\/(\S*)$/.exec(ta.value);
    if (!m) { menu.hidden = true; return; }
    if (!SS.cmds || SS.cmds.key !== s.mid + '|' + s.cwd) { SS.cmds = { key: s.mid + '|' + s.cwd, list: [] }; try { SS.cmds.list = await cm.relay(s.mid, 'GET', '/commands?cwd=' + encodeURIComponent(s.cwd || '')); } catch { /* none */ } }
    const q = m[1].toLowerCase(), hits = [...APP.map(([name, desc, fn]) => ({ name, desc, fn })), ...SS.cmds.list.filter((x) => !APP.some((a) => a[0] === x.name))].filter((x) => x.name.toLowerCase().includes(q)).slice(0, 9);
    menu.innerHTML = ''; hits.forEach((x) => menu.append(h('div', { class: 'sitem', onmousedown: (e) => { e.preventDefault(); pick(x.name, x.fn); } }, h('b', {}, '/' + x.name), h('span', { class: 'small muted' }, ' ' + (x.desc || '')))));
    menu.hidden = !hits.length;
  };
  ta.oninput = () => { grow(); showMenu(); saveDraft(s, ta.value); };
  ta.onkeydown = (e) => { if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) { e.preventDefault(); submit(); } };
  const submit = () => {
    const refs = SS.files.map((f) => '\n@' + f.path).join('');
    const t = (ta.value.trim() + refs).trim();
    if (!t || SS.uploading) return;
    if (shIsShared(s) && shChoice(s) !== 'mine' || isGuestMid(s.mid)) { ta.value = ''; saveDraft(s, ''); grow(); if (shRoute(s, t)) return; ta.value = t; return; }
    if (/^\/clear\s*$/i.test(t)) { ta.value = ''; saveDraft(s, ''); grow(); clearChat(); return; }
    ta.value = ''; saveDraft(s, ''); SS.files = []; renderFiles(); grow();
    if (SS.sending || remoteWorking()) { SS.queue.push(t); renderLive(); if (!SS.sending) refreshBusy(); } else sendMsg(t);
  };
  const sel = (opts, cur, fn) => h('select', { class: 'sel', onchange: (e) => fn(e.target.value) }, opts.map(([v, l]) => h('option', { value: v, selected: v === cur }, l)));
  const box = h('div', { class: 'composer' }, h('div', { class: 'cbox' }, menu, h('div', { id: 'files', class: 'files' }), ta,
    h('div', { class: 'row cfoot' },
      ...(isGuestMid(s.mid) ? [] : [sel([['auto', 'Auto'], ['acceptEdits', 'Edits'], ['plan', 'Plan only'], ['manual', 'Ask me'], ['bypassPermissions', 'Full auto']], SS.mode, (v) => { SS.mode = v; drawPane(); setChatAuto(v === 'bypassPermissions'); }),
      sel([['', 'Default model'], ['opus', 'Opus'], ['sonnet', 'Sonnet'], ['haiku', 'Haiku']], SS.model, (v) => { SS.model = v; }),
      sel([['', 'Default effort'], ['low', 'Low effort'], ['medium', 'Medium effort'], ['high', 'High effort'], ['xhigh', 'Extra high'], ['max', 'Max effort']], SS.effort, (v) => { SS.effort = v; })]),
      h('div', { class: 'grow' }),
      SS.sending || remoteRunning() ? ibtn('stop', 'Stop Claude', stopClaude, 'ghost sm') : null,
      (() => { const b = ibtn('send', SS.sending || remoteWorking() ? 'Queue this message' : 'Send (Enter)', submit, 'primary sm'); b.id = 'send'; return b; })())));
  if (SS.mode === 'bypassPermissions') box.append(h('div', { class: 'small err', style: 'margin-top:6px' }, 'Full auto lets Claude run any command on this computer without asking.'));
  box.append(h('div', { class: 'small muted', style: 'margin-top:6px;text-align:center' }, 'Enter to send · Shift+Enter for a new line · drop or paste files and images'));
  box.ondragover = (e) => e.preventDefault();
  box.ondrop = (e) => { e.preventDefault(); for (const f of e.dataTransfer.files) addFile(f); ta.focus(); };
  ta.onpaste = (e) => { const imgs = [...(e.clipboardData ? e.clipboardData.files : [])].filter((f) => f.type.startsWith('image/')); if (imgs.length) { e.preventDefault(); imgs.forEach(addFile); } };
  setTimeout(renderFiles, 0);
  return box;
}

async function loadProjects(mid) {
  try { SS.projects[mid] = await cm.relay(mid, 'GET', '/projects'); } catch { SS.projects[mid] = { root: null, projects: [] }; }
  if (SS.sel && SS.sel.isNew && SS.sel.mid === mid) renderMsgs();
}
/** Where should this chat run? Your own computers first; a server only when you pick it. */
function computerPicker(s) {
  if (SS.machines.length < 2) return null;
  const pick = (m) => { s.mid = m.id; SS.mid = m.id; s.cwd = ''; if (!m.secure) localStorage.setItem('cm.lastMachine', m.id); SS.projects[m.id] = SS.projects[m.id] || null; const own = (SS.live[m.id] || []).filter((x) => x.cwd).sort((a, b) => b.mtime - a.mtime)[0]; if (!m.secure && own) s.cwd = own.cwd; renderMsgs(); };
  return h('div', { class: 'stack', style: 'align-items:center;gap:8px' }, h('div', { class: 'muted small' }, 'Where should it run?'),
    h('div', { class: 'row wrap', style: 'justify-content:center' }, [...SS.machines].sort((a, b) => Number(!!a.secure) - Number(!!b.secure)).map((m) => chip(m.name, s.mid === m.id, () => pick(m)))),
    (SS.machines.find((m) => m.id === s.mid) || {}).secure ? h('div', { class: 'small muted' }, 'Runs on the server, so it keeps going when your computer is off.') : null);
}
const TALK_IDEAS = ['Help me think through an idea', 'Explain something to me simply', 'What should I learn next?', 'Brainstorm names for a project'];
/** New chat: pick "Coding" (a project folder, full model) or "Talk" (no project, cheap Haiku model, cannot edit files). */
function newChatIntro() {
  const s = SS.sel;
  const kindRow = h('div', { class: 'row', style: 'justify-content:center' },
    chip('Coding', s.kind !== 'talk', () => { s.kind = 'code'; renderMsgs(); }), chip('Talk', s.kind === 'talk', () => { s.kind = 'talk'; renderMsgs(); }));
  if (s.kind === 'talk') {
    return h('div', { class: 'intro' }, h('div', { html: sparkSVG(46) }), h('h2', {}, "What's on your mind?"), kindRow, computerPicker(s),
      h('div', { class: 'muted small', style: 'max-width:460px' }, 'Just a conversation. It uses the cheaper Haiku model and cannot change files, so it costs far fewer tokens.'),
      h('div', { class: 'row wrap', style: 'justify-content:center;max-width:640px;margin-top:10px' }, TALK_IDEAS.map((t) => h('button', { class: 'chip', onclick: () => { const ta = $('#composer-text'); ta.value = t; ta.focus(); } }, t))));
  }
  const body = codingIntro();
  body.insertBefore(kindRow, body.children[2] || null);
  return body;
}
function codingIntro() {
  const s = SS.sel;
  const info = SS.projects[s.mid];
  if (!info) loadProjects(s.mid);
  if (info && info.root) return serverIntro(s, info);
  const picker = computerPicker(s);
  const path = h('input', { value: s.cwd, placeholder: '/path/to/project', oninput: (e) => { s.cwd = e.target.value.trim(); SS.lastCwd = s.cwd; (SS.lastCwdBy = SS.lastCwdBy || {})[s.mid] = s.cwd; }, style: 'font-family:ui-monospace,monospace' });
  const m = SS.machines.find((x) => x.id === s.mid);
  const row = h('div', { class: 'row', style: 'width:100%;max-width:560px' }, h('div', { class: 'grow' }, path),
    m && m.local ? h('button', { class: 'ghost', onclick: async () => { const p = await cm.pickFolder(); if (p) { s.cwd = p; SS.lastCwd = p; path.value = p; } } }, 'Browse…') : null);
  return h('div', { class: 'intro' }, h('div', { html: sparkSVG(46) }), h('h2', {}, 'What should we work on?'), picker,
    h('div', { class: 'muted small' }, 'Project folder'), row,
    h('div', { class: 'row wrap', style: 'justify-content:center;max-width:560px' }, projects().slice(0, 4).map((p) => chip(folderName(p), s.cwd === p, () => { s.cwd = p; SS.lastCwd = p; path.value = p; }))),
    h('div', { class: 'row wrap', style: 'justify-content:center;max-width:640px;margin-top:10px' }, SUGGEST.map((t) => h('button', { class: 'chip', onclick: () => { const ta = $('#composer-text'); ta.value = t; ta.focus(); } }, t))));
}

/** A server that manages one projects folder: pick a project in it, or create a new one. */
/** The computers linked to the server, as buttons: Auto, the Server, and each of your own computers (with a dot when it is online). */
function loadComputers(mid) { if (SS.computersFor === mid && Date.now() - (SS.computersAt || 0) < 15000) return; SS.computersFor = mid; SS.computersAt = Date.now(); cm.relay(mid, 'GET', '/computers').then((l) => { SS.computers = l || []; if (SS.sel && SS.sel.mid === mid && (!SS.rightOff || (SS.sel && SS.sel.isNew))) drawPane(); }).catch(() => { SS.computers = []; }); }
function runChips(cur, pick, withAuto = true) {
  if (SS.sel) loadComputers(SS.sel.mid);
  const pcs = (SS.computers || []).filter((c) => c.kind === 'computer');
  return [withAuto ? chip('Auto', cur === 'auto', () => pick('auto')) : null, chip('Server', cur === 'vps', () => pick('vps')), ...pcs.map((c) => chip((c.online ? '● ' : '○ ') + c.name, cur === c.id || (cur === 'laptop' && false), () => pick(c.id)))].filter(Boolean);
}
function runOnPicker(s) {
  const mm = SS.machines.find((x) => x.id === s.mid); if (!mm || !mm.secure) return null;
  loadComputers(s.mid);
  if (!(SS.computers || []).some((c) => c.kind === 'computer')) return null;
  return h('div', { class: 'stack', style: 'align-items:center;gap:6px' }, h('div', { class: 'muted small' }, 'Which computer should work on it?'), h('div', { class: 'row wrap', style: 'justify-content:center' }, runChips(SS.runOn || 'vps', (v) => { SS.runOn = v; renderMsgs(); }, false)));
}
/** Server options (root chats, working outside the projects folder), asked once per server. */
function serverOpts(mid) {
  SS.sopts = SS.sopts || {};
  if (SS.sopts[mid] === undefined) { SS.sopts[mid] = null; cm.relay(mid, 'GET', '/admin/options').then((o) => { SS.sopts[mid] = o; if (SS.sel && SS.sel.isNew && SS.sel.mid === mid) renderMsgs(); }).catch(() => { SS.sopts[mid] = false; }); }
  return SS.sopts[mid] || null;
}
/** Browse the folders of the server, pick one to work in, or make a new one. */
function openServerBrowser(s) {
  const mid = s.mid, over = h('div', { class: 'overlay' }), box = h('div', { class: 'modal browser' });
  let cur = null, hidden = false;
  const close = () => over.remove();
  const err = h('div', { class: 'small err' }), list = h('div', { class: 'blist' }), where = h('div', { class: 'mono small ellip' }), nameIn = h('input', { placeholder: 'New folder name' });
  const load = async (p) => {
    try {
      const r = await cm.relay(mid, 'GET', '/fs?path=' + encodeURIComponent(p || '') + (hidden ? '&hidden=1' : '')); cur = r; err.textContent = '';
      where.textContent = r.path; where.title = r.path; list.innerHTML = '';
      if (r.parent) list.append(h('button', { class: 'bitem up', onclick: () => load(r.parent) }, '↑ ..'));
      for (const d of r.dirs) list.append(h('button', { class: 'bitem', onclick: () => load(d.path) }, h('span', {}, '📁 ' + d.name), d.git ? h('span', { class: 'small muted' }, 'git') : null));
      if (!r.dirs.length) list.append(h('div', { class: 'small muted', style: 'padding:10px' }, 'No folders in here.'));
    } catch (e) { err.textContent = clean(e); }
  };
  const mk = async () => {
    const n = nameIn.value.trim(); if (!n || !cur) return;
    try { const r = await cm.relay(mid, 'POST', '/fs/mkdir', { path: cur.path, name: n }); nameIn.value = ''; await load(r.path); } catch (e) { err.textContent = clean(e); }
  };
  nameIn.onkeydown = (e) => { if (e.key === 'Enter') mk(); };
  box.append(h('div', { class: 'row' }, h('h3', { class: 'grow' }, 'Pick a folder on the server'), h('button', { class: 'ghost sm', onclick: close }, 'Close')), where, list,
    h('div', { class: 'row' }, h('div', { class: 'grow' }, nameIn), h('button', { class: 'ghost sm', onclick: mk }, 'Make folder')),
    h('div', { class: 'row wrap' }, chip('Show hidden folders', hidden, () => { hidden = !hidden; load(cur && cur.path); }), h('div', { class: 'grow' }),
      h('button', { class: 'primary', onclick: () => { if (!cur) return; s.cwd = cur.path; SS.lastCwd = cur.path; close(); renderMsgs(); } }, 'Work in this folder')), err);
  over.append(box); over.onclick = (e) => { if (e.target === over) close(); }; document.body.append(over);
  load(s.cwd || '');
}
function serverIntro(s, info) {
  const so = serverOpts(s.mid);
  const nameIn = h('input', { placeholder: 'New project name, e.g. budget-app' }), err = h('div', { class: 'small err' });
  const go = async () => {
    try { const r = await cm.relay(s.mid, 'POST', '/projects', { name: nameIn.value.trim() }); s.cwd = r.path; SS.lastCwd = r.path; SS.projects[s.mid] = null; err.textContent = ''; const t = $('#composer-text'); if (t) t.focus(); renderMsgs(); }
    catch (e) { err.textContent = clean(e); }
  };
  nameIn.onkeydown = (e) => { if (e.key === 'Enter') go(); };
  return h('div', { class: 'intro' }, h('div', { html: sparkSVG(46) }), h('h2', {}, 'What should we build?'), computerPicker(s), runOnPicker(s),
    h('div', { class: 'muted small' }, 'Projects folder on ' + ((SS.machines.find((m) => m.id === s.mid) || {}).name || 'the server')),
    h('div', { class: 'row', style: 'width:100%;max-width:560px' }, h('div', { class: 'grow' }, nameIn), h('button', { class: 'primary', onclick: go }, 'Create')), err,
    h('div', { class: 'row wrap', style: 'justify-content:center;max-width:560px' }, info.projects.slice(0, 8).map((p) => chip(p.name, s.cwd === p.path, () => { s.cwd = p.path; SS.lastCwd = p.path; renderMsgs(); }))),
    h('div', { class: 'row wrap', style: 'justify-content:center' }, so && so.outside !== false ? h('button', { class: 'ghost sm', onclick: () => openServerBrowser(s) }, 'Browse the server…') : null,
      so && so.root && so.rootReady && so.rootReady.ok ? chip('Run as root', !!s.root, () => { s.root = !s.root; renderMsgs(); }) : null),
    so && so.root && so.rootReady && !so.rootReady.ok ? h('div', { class: 'small err', style: 'max-width:520px;text-align:center' }, 'Root chats are switched on, but not possible yet: ' + so.rootReady.why) : null,
    s.root ? h('div', { class: 'small err' }, 'Root chat: Claude gets full administrator rights on this server and knows it. Be sure of what you ask.') : null,
    s.cwd ? h('div', { class: 'small muted' }, 'Starting in ' + s.cwd) : h('div', { class: 'small muted' }, 'Create a project or pick one, then describe what you want.'));
}

// ───────── messages ─────────
const base = (p) => String(p || '').split('/').pop();
function toolSummary(t) {
  const i = t.input || {};
  if (t.name === 'Bash') return i.description || String(i.command || '').split('\n')[0];
  if (i.file_path) return base(i.file_path);
  return String(t.text || '').replace(new RegExp('^' + t.name + '\\s*'), '');
}
const diffEl = (oldS, newS) => {
  const d = h('div', { class: 'diff' });
  if (oldS) oldS.split('\n').forEach((l) => d.append(h('div', { class: 'del' }, '- ' + l)));
  if (newS) newS.split('\n').forEach((l) => d.append(h('div', { class: 'add' }, '+ ' + l)));
  return d;
};
function toolBody(t) {
  const i = t.input || {}, b = h('div', { class: 'tbody' });
  if (t.name === 'TodoWrite') (i.todos || []).forEach((x) => b.append(h('div', { class: 'todo' + (x.status === 'completed' ? ' done' : '') }, (x.status === 'completed' ? '☑ ' : x.status === 'in_progress' ? '▶ ' : '☐ ') + (x.content || ''))));
  else if (t.name === 'Bash') b.append(h('pre', { class: 'cmd' }, '$ ' + (i.command || '')));
  else if (t.name === 'Edit') b.append(h('div', { class: 'small muted' }, i.file_path), diffEl(i.old_string, i.new_string));
  else if (t.name === 'MultiEdit') { b.append(h('div', { class: 'small muted' }, i.file_path)); (i.edits || []).forEach((e) => b.append(diffEl(e.old_string, e.new_string))); }
  else if (t.name === 'Write') b.append(h('div', { class: 'small muted' }, i.file_path), h('pre', { class: 'out' }, i.content || ''));
  else b.append(h('pre', { class: 'out' }, Object.entries(i).map(([k, v]) => k + ': ' + v).join('\n')));
  if (t.result) b.append(h('pre', { class: 'out' + (t.error ? ' bad' : '') }, t.result));
  return b;
}
function toolCard(t, key) {
  const d = h('details', { class: 'tool' + (t.error ? ' bad' : '') });
  d.open = SS.open.has(key);
  d.ontoggle = () => { if (d.open) SS.open.add(key); else SS.open.delete(key); };
  const status = t.result === undefined ? h('span', { class: 'spin', title: 'Running' }) : h('span', { class: t.error ? 'bad' : 'okc' }, t.error ? 'failed' : /^(Edit|MultiEdit|Write|NotebookEdit)$/.test(t.name || '') ? 'applied' : 'done');
  d.append(h('summary', {}, h('span', { class: 'tname' }, t.name || 'Tool'), h('span', { class: 'tsum ellip' }, toolSummary(t)), status), toolBody(t));
  return d;
}
const assistantEl = (text) => { const d = h('div', { class: 'md' }); d.append(md(text)); return d; };

function renderMsgs() {
  const col = $('#col');
  if (!col) return;
  const box = $('#msgs'), stick = box.scrollHeight - box.scrollTop - box.clientHeight < 120;
  col.innerHTML = '';
  if (SS.sel && SS.sel.isNew && !SS.msgs.length && !SS.sending) col.append(newChatIntro());
  let tools = null;
  const key = SS.sel ? (SS.sel.id || 'new') : '', before = SS.shown && SS.shown.key === key ? SS.shown.n : -1;   // messages that are new since the last drawing glide in
  SS.msgs.forEach((m, i) => {
    const fresh = before >= 0 && i >= before ? ' fresh' : '';
    if (m.role === 'tool') { if (!tools) { tools = h('div', { class: 'tools' + fresh }); col.append(tools); } tools.append(toolCard(m, 'm' + i)); return; }
    tools = null;
    if (m.role === 'meta') { col.append(h('div', { class: 'meta' }, m.text)); return; }
    if (m.role === 'user') col.append(m.own ? h('div', { class: 'rmmsg' }, h('div', { class: 'small muted' }, m.name), h('div', { class: 'msg user' + fresh, style: 'align-self:auto;max-width:100%' }, m.text)) : h('div', { class: 'msg user' + (m.pending ? ' pending' : '') + fresh }, m.text));
    else { const el = assistantEl(m.text); if (fresh) el.classList.add('fresh'); if (m.own) { const w = h('div', { class: 'rmmsg claude' }, h('div', { class: 'small muted' }, m.name + ' · own account'), el); col.append(w); } else col.append(el); }
  });
  SS.shown = { key, n: SS.msgs.length };
  col.append(h('div', { id: 'live' }));
  if (false) col.append(h('div', { class: 'small muted' }, (SS.lastRun.error ? 'Stopped' : 'Done') + (SS.lastRun.ms ? ' in ' + (SS.lastRun.ms >= 60000 ? Math.floor(SS.lastRun.ms / 60000) + 'm ' + Math.round(SS.lastRun.ms % 60000 / 1000) + 's' : Math.round(SS.lastRun.ms / 1000) + 's') : '')));
  if (SS.chatErr) col.append(h('div', { class: 'small err' }, SS.chatErr));
  renderLive();
  if (stick || SS.sending) box.scrollTop = box.scrollHeight;
}
let liveQueued = false;
function scheduleLive() { if (!liveQueued) { liveQueued = true; requestAnimationFrame(() => { liveQueued = false; renderLive(); }); } }
function renderLive() {
  const live = $('#live');
  if (!live) return;
  const box = $('#msgs'), stick = box.scrollHeight - box.scrollTop - box.clientHeight < 140;
  live.innerHTML = '';
  let tools = null;
  SS.parts.forEach((p, i) => {
    if (p.type === 'tool') { if (!tools) { tools = h('div', { class: 'tools' }); live.append(tools); } tools.append(toolCard(p, 'l' + (p.id || i))); }
    else { tools = null; live.append(assistantEl(p.text)); }
  });
  if ((SS.sending && !SS.parts.length) || remoteWorking()) live.append(h('div', { class: 'typing' }, h('i'), h('i'), h('i')));
  for (const q of SS.queue) live.append(h('div', { class: 'msg user queued' }, q, h('div', { class: 'small muted' }, 'queued')));
  if (SS.chatErr) live.append(h('div', { class: 'small err' }, SS.chatErr));
  if (stick) box.scrollTop = box.scrollHeight;
}

function sendMsg(text) {
  const s = SS.sel, sid = rand();
  if (s.id) { const mid = machineFor(s.id, s.mid); if (mid !== s.mid) { s.mid = mid; SS.mid = mid; } }
  if (s.isNew && s.kind !== 'talk' && !s.cwd) { SS.chatErr = 'Choose a project folder first.'; renderMsgs(); return; }
  const mm = SS.machines.find((x) => x.id === s.mid);
  if (mm && !mm.secure) localStorage.setItem('cm.lastMachine', mm.id);
  const run = { parts: [] };
  if (s.id) RUNS.set(s.id, run);
  SS.sending = true; SS.parts = run.parts; SS.chatErr = null; SS.lastRun = null; SS.msgs.push({ role: 'user', text, pending: true });
  const mine = { text, sel: s, id: s.id || null, at: Date.now() };
  (SS.unsent = SS.unsent || []).push(mine);
  drawPane();
  const viewing = () => sameChat(SS.sel, s) && SS.sel.id === s.id;
  const unqueue = () => { SS.unsent = (SS.unsent || []).filter((u) => u !== mine); SS.msgs = SS.msgs.filter((m) => !(m.pending && m.text === text)); };
  const finish = async (busy) => {
    if (!chatHandlers[sid]) return;
    delete chatHandlers[sid];
    if (s.id && RUNS.get(s.id) === run) RUNS.delete(s.id);
    if (busy) { qOf(s).unshift(text); unqueue(); }   // Claude is working on this chat from somewhere else: wait for it, then send
    if (viewing()) {
      SS.sending = false; SS.parts = [];
      await loadMsgs(false);
      SS.perms = SS.perms.filter((p) => p.ev.session !== s.id);
      drawPane(); renderMsgs();
    }
    loadList();
    if (busy) { refreshBusy(); return; }
    const q = qOf(s);
    if (q.length && viewing()) sendMsg(q.shift());
  };
  chatHandlers[sid] = (ev) => {
    const live = viewing();
    if (ev.t === 'queued') { toast(ev.online ? `Sent to ${ev.to}. It runs there now.` : 'Queued. It runs on your laptop when it is online.'); return; }
    if (ev.t === 'session') { if (!s.id) { s.id = ev.id; s.isNew = false; s.title = text.slice(0, 70); RUNS.set(ev.id, run); { const nq = QUEUES.get('new') || []; QUEUES.set(ev.id, nq); if (QUEUES.get('new') === nq) QUEUES.delete('new'); } (SS.unsent || []).forEach((u) => { if (u.sel === s) u.id = ev.id; }); drawSide(); } return; }
    if (ev.t === 'delta') { const last = run.parts[run.parts.length - 1]; if (last && last.type === 'text') last.text += ev.text; else run.parts.push({ type: 'text', text: ev.text }); }
    else if (ev.t === 'tool') run.parts.push({ type: 'tool', id: ev.id, name: ev.name, input: ev.input, text: ev.text });
    else if (ev.t === 'result') { const t = run.parts.find((p) => p.type === 'tool' && p.id === ev.id); if (t) { t.result = ev.text; t.error = ev.error; } }
    else if (ev.t === 'done') { if (live) SS.lastRun = { sid: s.id, ms: ev.ms, error: ev.error }; }
    else if (ev.t === 'fail') { if (/still working on this chat/i.test(ev.text || '')) { finish(true); return; } if (live) SS.chatErr = ev.text; finish(); return; }
    else if (ev.t === 'end') { finish(); return; }
    if (live) scheduleLive();
  };
  cm.send(s.mid, s.id || null, text, SS.mode, false, sid, { run: s.isNew && SS.runOn && SS.runOn !== 'vps' && (SS.machines.find((x) => x.id === s.mid) || {}).secure ? SS.runOn : undefined, cwd: s.isNew && s.kind !== 'talk' ? s.cwd : undefined, kind: s.isNew && s.kind === 'talk' ? 'talk' : undefined, root: s.isNew && s.root ? true : undefined, model: SS.model || undefined, effort: SS.effort || undefined })
    .catch((e) => { if (viewing()) SS.chatErr = clean(e); finish(); });
}

function startVoice() {
  if (SS.voice || !SS.sel || !SS.sel.id) return;
  SS.voice = new Voice(SS.pane, SS.sel.mid, SS.sel, SS.mode, { onClose: () => { SS.voice = null; loadMsgs(false); } });
  SS.voice.open();
}

// ───────── approvals and heads-ups ─────────
function renderPerms() {
  const box = $('#perms');
  if (!box) return;
  box.innerHTML = '';
  const s = SS.sel;
  for (const p of SS.perms.filter((x) => s && x.ev.session === s.id)) {
    if (p.ev.kind === 'question') { box.append(questionCard(p)); continue; }
    if (p.ev.kind === 'request') { box.append(requestCard(p)); continue; }
    box.append(permCard(p));
  }
}
const diffLines = (a, b) => [...String(a || '').split('\n').map((l) => h('div', { class: 'dl del' }, '- ' + l)), ...String(b || '').split('\n').map((l) => h('div', { class: 'dl add' }, '+ ' + l))];
/** What Claude wants to do, shown like in a terminal: the command, or the lines that change. */
function permPreview(ev) {
  const i = ev.input || {}, t = ev.tool || '';
  if (t === 'Bash' && i.command) return h('pre', { class: 'pcode' }, '$ ' + i.command);
  if ((t === 'Edit' || t === 'NotebookEdit') && (i.old_string != null || i.new_string != null)) return h('div', { class: 'pcode' }, h('div', { class: 'dl file' }, i.file_path || ''), ...diffLines(i.old_string, i.new_string));
  if (t === 'MultiEdit' && i.edits) return h('div', { class: 'pcode' }, h('div', { class: 'dl file' }, i.file_path || ''), ...i.edits.flatMap((e) => diffLines(e.old_string, e.new_string)));
  if (t === 'Write') return h('div', { class: 'pcode' }, h('div', { class: 'dl file' }, i.file_path || ''), ...String(i.content || '').split('\n').slice(0, 14).map((l) => h('div', { class: 'dl add' }, '+ ' + l)));
  return h('pre', { class: 'pcode' }, ev.text || '');
}
/** The approval prompt, as in Claude Code: 1 Yes · 2 Yes, don't ask again · 3 No. Answer with the keys 1, 2, 3 (or Y / N, Esc) or by clicking. */
function permCard(p) {
  const ev = p.ev, tool = ev.tool || 'a tool';
  const again = ev.tool === 'Bash' ? (ev.suggest ? `Yes, and don’t ask again for “${ev.suggest.label}”` : null) : (ev.tool ? `Yes, and don’t ask again for ${tool} in this chat` : null);
  const opts = [['1', 'Yes', 'allow'], ...(again ? [['2', again, 'allow_always']] : []), [String(again ? 3 : 2), 'No', 'deny']];
  return h('div', { class: 'perm pc', tabindex: '0', 'data-req': ev.req },
    h('div', { class: 'label' }, ev.tool ? `Claude wants to use ${tool}` : 'Claude needs your OK'), permPreview(ev),
    h('div', { class: 'popts' }, opts.map(([k, l, b]) => h('button', { class: 'popt' + (b === 'deny' ? ' no' : ''), onclick: () => decide(p, b) }, h('kbd', {}, k), l))));
}
document.addEventListener('keydown', (e) => {
  if (tab !== 'sessions' || e.ctrlKey || e.metaKey || e.altKey) return;
  const t = e.target, typing = t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable);
  const s = SS.sel, mine = s && SS.perms.filter((x) => x.ev.session === s.id && x.ev.kind === 'permission');
  if (!mine || !mine.length || (typing && !(t.id === 'composer-text' && !t.value))) return;   // never steal keys while you are typing
  const p = mine[0], card = $(`.pc[data-req="${p.ev.req}"]`); if (!card) return;
  const keys = [...card.querySelectorAll('.popt')], k = e.key.toLowerCase();
  const pick = k === 'y' ? keys[0] : k === 'n' || k === 'escape' ? keys[keys.length - 1] : /^[1-9]$/.test(k) ? keys[Number(k) - 1] : null;
  if (pick) { e.preventDefault(); pick.click(); }
});
function questionCard(p) {
  const ev = p.ev, picked = new Set();
  const input = h('input', { placeholder: 'Or type your own answer…' });
  const answer = async (text) => {
    if (!text) return;
    try { await cm.relay(p.machine, 'POST', '/answer', { req: ev.req, answer: text }); } catch (e) { SS.chatErr = clean(e); }
    SS.perms = SS.perms.filter((x) => x !== p); renderPerms();
  };
  input.onkeydown = (e) => { if (e.key === 'Enter') answer(input.value.trim()); };
  const opts = h('div', { class: 'qopts' }, (ev.options || []).map((o) => {
    const b = h('button', { class: 'qopt', onclick: () => { if (ev.multi) { picked.has(o.label) ? picked.delete(o.label) : picked.add(o.label); b.classList.toggle('on', picked.has(o.label)); } else answer(o.label); } },
      h('div', { class: 'qlabel' }, o.label), o.description ? h('div', { class: 'small muted' }, o.description) : null);
    return b;
  }));
  return h('div', { class: 'perm q' }, h('div', { class: 'grow stack', style: 'gap:8px' }, h('div', { class: 'label' }, 'Claude asks'), h('div', { class: 'qtext' }, ev.question || ev.text), opts,
    h('div', { class: 'row' }, h('div', { class: 'grow' }, input), ev.multi ? h('button', { class: 'ghost sm', onclick: () => answer([...picked, input.value.trim()].filter(Boolean).join(', ')) }, 'Send choices') : h('button', { class: 'primary sm', onclick: () => answer(input.value.trim()) }, 'Send'))));
}
async function decide(p, behavior) {
  try { await cm.relay(p.machine, 'POST', '/decision', { req: p.ev.req, behavior }); } catch (e) { SS.chatErr = clean(e); }
  SS.perms = SS.perms.filter((x) => x !== p);
  renderPerms();
}
function toast(text, onclick) {
  const t = h('div', { class: 'toast', onclick: () => { t.remove(); if (onclick) onclick(); } }, text);
  document.body.append(t);
  setTimeout(() => t.remove(), 9000);
}
cm.on('event', ({ machine, ev }) => {
  const viewing = tab === 'sessions' && SS.sel && SS.sel.id === ev.session;
  const proj = folderName(ev.cwd);
  if (ev.forMember) {   // meant for you as a friend in someone else's shared chat: you decide, your own Claude runs it
    if (ev.kind === 'request' && !viewing) toast(`${ev.from || 'Someone'} wants to use your Claude`, () => { const hit = allSessions().find((x) => x.id === ev.session && x.mid === machine); openSession(hit || { id: ev.session, title: ev.title, cwd: ev.cwd, mid: machine, live: true }); });
    if (viewing) shLoadGuestInfo();
    return;
  }
  if (ev.kind === 'resolved') { SS.perms = SS.perms.filter((p) => p.ev.req !== ev.req); renderPerms(); return; }
  if ((ev.kind === 'permission' || ev.kind === 'question' || ev.kind === 'request') && ev.req) {
    if (!SS.perms.some((p) => p.ev.req === ev.req)) SS.perms.push({ machine, ev });
    renderPerms();
    if (viewing) { const card = $(`.pc[data-req="${ev.req}"]`); if (card) { card.scrollIntoView({ block: 'nearest' }); const ta = $('#composer-text'); if (!ta || !ta.value) card.focus({ preventScroll: true }); } }
    if (!viewing) toast(ev.kind === 'request' ? `${ev.from || 'A friend'} wants to use your Claude` : `${proj}: ${ev.kind === 'question' ? 'Claude has a question' : 'Claude needs your OK'}`, () => openSession({ id: ev.session, title: ev.title, cwd: ev.cwd, mid: machine, live: true }));
  } else if ((ev.kind === 'done' || ev.kind === 'error') && !viewing) {
    toast(`${proj}: ${ev.kind === 'error' ? 'something went wrong' : 'Claude is done'}`, () => openSession({ id: ev.session, title: ev.title, cwd: ev.cwd, mid: machine, live: false }));
  }
});

// ───────── attachments ─────────
function renderFiles() {
  const box = $('#files');
  if (!box) return;
  box.innerHTML = '';
  SS.files.forEach((f) => box.append(h('span', { class: 'fchip' }, f.name, h('button', { class: 'x', title: 'Remove', onclick: () => { SS.files = SS.files.filter((x) => x !== f); renderFiles(); } }, '×'))));
  if (SS.uploading) box.append(h('span', { class: 'small muted' }, 'Uploading…'));
}
/** Files on this computer are referenced in place; anything else (or any file for another computer) is uploaded into the project. */
async function addFile(f) {
  const s = SS.sel;
  if (!s) return;
  const mach = SS.machines.find((m) => m.id === s.mid) || {};
  const local = mach.local && cm.filePath(f);
  if (local) { SS.files.push({ name: f.name, path: cm.filePath(f) }); return renderFiles(); }
  if (!s.id && !s.cwd) { SS.chatErr = 'Choose the project folder first, then attach files.'; return renderMsgs(); }
  if (f.size > 25e6) { SS.chatErr = 'That file is over 25 MB.'; return renderMsgs(); }
  SS.uploading = true; renderFiles();
  try {
    const path = await cm.upload(s.mid, s.id || null, s.cwd, f.name && f.name !== 'image.png' ? f.name : 'pasted-' + Date.now() + '.png', await f.arrayBuffer());
    SS.files.push({ name: f.name || 'image.png', path });
  } catch (e) { SS.chatErr = clean(e); renderMsgs(); }
  SS.uploading = false; renderFiles();
}

// ───────── work on the go: move a project to the projects folder and put it on the server ─────────
async function loadStatus(mid) {
  try { SS.status[mid] = await cm.relay(mid, 'GET', '/status'); } catch { SS.status[mid] = {}; }
  if (SS.sel && SS.sel.mid === mid) drawPane();
}
function continueBox(s, mach) {
  const local = SS.machines.find((m) => m.local);
  if (!mach.secure || !local || !s.id || !s.cwd) return null;
  const busy = (SS.list.concat(...Object.values(SS.live).filter(Boolean)).find((x) => x.id === s.id) || {}).running;
  return h('div', { class: 'row wrap' }, h('div', { class: 'grow small muted' }, `Carry on with this chat on ${local.name}. The project files are already synced; the chat is copied over, and Claude then works on your computer with your tools.`),
    h('button', { class: 'primary sm', disabled: SS.sending || busy, onclick: () => continueHere(s, mach, local) }, 'Continue on this computer'));
}
async function continueHere(s, server, local) {
  if (SS.sending) return;
  const box = h('div', { class: 'moving' }, h('div', { class: 'card stack', style: 'max-width:440px;text-align:center' }, h('div', { html: sparkSVG(40) }), h('h3', {}, 'Moving to ' + local.name), h('div', { class: 'muted', id: 'move-text' }, 'Starting…')));
  document.body.append(box);
  const off = cm.on('move-progress', (p) => { const t = $('#move-text'); if (t) t.textContent = p.text; });
  try {
    const r = await cm.continueHere(server.id, local.id, s.cwd);
    box.remove(); off();
    if (r.session) openSession({ id: r.session.id, title: r.session.title, cwd: r.cwd, mid: local.id, live: false });
    else toast('Copied. The chat will appear on ' + local.name + ' shortly.');
    loadList(); pollLive();
  } catch (e) { box.remove(); off(); SS.chatErr = clean(e); renderMsgs(); }
}
function moveBox(s, mach) {
  const server = SS.machines.find((m) => m.secure);
  if (mach.local && s.id && s.cwd && !server) return h('div', { class: 'row wrap' }, h('div', { class: 'grow small muted' }, 'Want to carry on from your phone without this computer? Connect your server first.'), h('button', { class: 'ghost sm', onclick: () => setTab('settings') }, 'Add a server'));
  if (!mach.local || !server || !s.id || !s.cwd) return null;
  const st = SS.status[mach.id];
  if (!st) { loadStatus(mach.id); return null; }
  if (!st.moveRoot) return null;
  const inside = s.cwd === st.moveRoot || s.cwd.startsWith(st.moveRoot.replace(/\/$/, '') + '/');
  const name = folderName(s.cwd);
  const btn = h('button', { class: 'primary sm', disabled: SS.sending, onclick: () => moveProject(s, mach, server, st, inside, name) }, inside ? `Copy this chat to ${server.name}` : `Move to projects and ${server.name}`);
  return h('div', { class: 'row wrap' }, h('div', { class: 'grow small muted' }, inside
    ? `This project is already in your projects folder. Copy the chat to ${server.name} and carry on from your phone, without your computer.`
    : `Moves ${name} into ${st.moveRoot}, syncs it to ${server.name} and copies this chat there. Then you can work on it from anywhere, without your computer or a terminal.`), btn);
}
async function moveProject(s, mach, server, st, inside, name) {
  if (SS.sending) return;
  const ok = confirm(inside
    ? `Copy this chat to ${server.name}?\n\nThe project is already in ${st.moveRoot}, so it is on the server.`
    : `Move "${name}" to ${st.moveRoot} and put it on ${server.name}?\n\nThe folder changes from ${s.cwd} to ${st.moveRoot}/${name}. Any terminal that is open in the old folder will lose it.\nYour chats for this project move with it.`);
  if (!ok) return;
  const box = h('div', { class: 'moving' }, h('div', { class: 'card stack', style: 'max-width:440px;text-align:center' }, h('div', { html: sparkSVG(40) }), h('h3', {}, 'Moving to ' + server.name), h('div', { class: 'muted', id: 'move-text' }, 'Starting…')));
  document.body.append(box);
  const off = cm.on('move-progress', (p) => { const t = $('#move-text'); if (t) t.textContent = p.text; });
  try {
    const r = await cm.moveProject(mach.id, server.id, s.cwd, name);
    box.remove(); off();
    if (r.session) openSession({ id: r.session.id, title: r.session.title, cwd: r.cwd, mid: server.id, live: false });
    else toast('Moved. The chat will appear on ' + server.name + ' shortly.');
    loadList(); pollLive();
  } catch (e) { box.remove(); off(); SS.chatErr = clean(e); renderMsgs(); }
}
