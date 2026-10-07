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
    h('div', { class: 'small muted' }, ((s.running || s.busy) ? 'working' : relTime(s.mtime)) + (s.elsewhere ? ' · only on ' + s.elsewhere : '')));
}

function drawSide() {
  const side = SS.side;
  if (!side) return;
  const keep = side.scrollTop, hadFocus = document.activeElement && document.activeElement.id === 'chat-search';
  side.innerHTML = '';
  side.append(h('button', { class: 'primary', onclick: newChat }, '+ New chat'));
  side.append(h('input', { id: 'chat-search', placeholder: 'Search', value: SS.search, oninput: (e) => { SS.search = e.target.value; drawSide(); } }));
  const q = SS.search.toLowerCase();
  const list = allSessions().filter((s) => !q || s.title.toLowerCase().includes(q) || (s.cwd || '').toLowerCase().includes(q));
  const groups = new Map();
  for (const s of list) { const k = '|' + (s.talk ? 'Talks' : (s.cwd || 'Other')); if (!groups.has(k)) groups.set(k, []); groups.get(k).push(s); }
  const ordered = [...groups.entries()].sort((a, b) => Math.max(...b[1].map((x) => x.mtime)) - Math.max(...a[1].map((x) => x.mtime)));
  for (const [key, items] of ordered) {
    const [mid, cwd] = [key.split('|')[0], key.slice(key.indexOf('|') + 1)];
    const closed = SS.closed[key] && !q;
    side.append(h('div', { class: 'ghead', onclick: () => { SS.closed[key] = !SS.closed[key]; drawSide(); } },
      h('span', { class: 'chev' }, closed ? '›' : '⌄'), h('span', { class: 'grow ellip', title: cwd }, folderName(cwd)),
      null));
    if (!closed) items.sort((a, b) => b.mtime - a.mtime).forEach((s) => side.append(chatItem(s)));
  }
  if (!list.length && !SS.err) side.append(h('p', { class: 'small muted' }, SS.machines.length ? 'No chats yet. Start one with “New chat”.' : 'No computer connected yet. Add one in Settings.'));
  if (SS.err) side.append(h('div', { class: 'small err' }, SS.err));
  side.scrollTop = keep;
  if (hadFocus) { const i = $('#chat-search'); i.focus(); i.setSelectionRange(i.value.length, i.value.length); }
}

// ───────── data ─────────
async function loadList() {
  if (!SS.mid) return;
  try { SS.list = await cm.relay(SS.mid, 'GET', '/sessions'); SS.err = null; } catch (e) { SS.err = clean(e); }
  drawSide();
}
async function pollLive() {
  for (const m of SS.machines) {
    try { SS.live[m.id] = await cm.relay(m.id, 'GET', '/sessions'); if (m.id === SS.mid) { SS.list = SS.live[m.id]; SS.err = null; } }
    catch (e) { SS.live[m.id] = null; if (m.id === SS.mid) SS.err = clean(e); }
  }
  if (tab === 'sessions') drawSide();
}
async function loadMsgs(quiet) {
  const s = SS.sel;
  if (!s || !s.id) return;
  try {
    const m = await cm.relay(s.mid, 'GET', `/sessions/${s.id}/messages?tail=400`);
    const last = (a) => a.length ? a[a.length - 1] : {};
    if (SS.sel !== s) return;
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
  const root = h('div', { class: 'split' });
  SS.side = h('div', { class: 'side' }); SS.pane = h('div', { class: 'pane' });
  const width = (w) => Math.max(220, Math.min(560, w));
  SS.side.style.width = width(Number(localStorage.getItem('cm.sideW')) || 310) + 'px';
  const rz = h('div', { class: 'resizer', title: 'Drag to resize · double-click to reset' });
  rz.ondblclick = () => { SS.side.style.width = '310px'; localStorage.setItem('cm.sideW', '310'); };
  rz.onmousedown = (e) => {
    e.preventDefault();
    const x0 = e.clientX, w0 = SS.side.offsetWidth;
    rz.classList.add('drag'); document.body.style.userSelect = 'none'; document.body.style.cursor = 'col-resize';
    const move = (ev) => { SS.side.style.width = width(w0 + ev.clientX - x0) + 'px'; };
    const up = () => { rz.classList.remove('drag'); document.body.style.userSelect = ''; document.body.style.cursor = ''; localStorage.setItem('cm.sideW', String(SS.side.offsetWidth)); removeEventListener('mousemove', move); removeEventListener('mouseup', up); };
    addEventListener('mousemove', move); addEventListener('mouseup', up);
  };
  root.append(SS.side, rz, SS.pane);
  drawSide(); drawPane();
  (async () => {
    SS.machines = await cm.machines();
    if (!SS.mid || !SS.machines.some((m) => m.id === SS.mid)) SS.mid = defaultMachineId();
    drawSide(); await loadList(); await pollLive();
  })();
  setTimeout(() => { if (SS.sel) { renderMsgs(); renderPerms(); renderFiles(); } }, 0);  // the pane is in the document now
  SS.timers.push(setInterval(pollLive, 8000), setInterval(() => { if (SS.sel && SS.sel.id && !SS.sending && !SS.voice) loadMsgs(true); }, 3000));
  return root;
}

function openSession(s, voice) {
  SS.call = 'off';
  SS.sel = { id: s.id, title: s.title, cwd: s.cwd, mid: s.mid || SS.mid, live: s.live }; SS.mid = SS.sel.mid; SS.msgs = []; SS.parts = []; SS.sending = false;
  if (tab !== 'sessions') setTab('sessions');
  drawSide(); drawPane();
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
  const mid = defaultMachineId() || SS.mid;
  SS.sel = { id: null, isNew: true, title: 'New chat', cwd: '', mid };
  SS.mid = mid; SS.queue = []; SS.perms = [];
  const own = (SS.live[mid] || []).filter((x) => x.cwd).sort((a, b) => b.mtime - a.mtime)[0];
  SS.sel.cwd = (SS.lastCwdBy && SS.lastCwdBy[mid]) || (own && own.cwd) || '';
  SS.msgs = []; SS.parts = []; SS.sending = false; SS.chatErr = null;
  if (tab !== 'sessions') setTab('sessions');
  drawSide(); drawPane();
  const t = $('#composer-text'); if (t) t.focus();
}
document.addEventListener('keydown', (e) => { if (tab === 'sessions' && (e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'n') { e.preventDefault(); newChat(); } });

// ───────── pane ─────────
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
  const mach = SS.machines.find((x) => x.id === s.mid) || {};
  pane.append(h('div', { class: 'chat-head' }, h('div', { class: 'grow', style: 'min-width:0' }, h('h3', { class: 'ellip' }, s.title),
    h('div', { class: 'small muted ellip', title: s.cwd }, [s.cwd ? folderName(s.cwd) : '', multi() ? mach.name : '', SS.call && SS.call !== 'off' ? 'will call you' : ''].filter(Boolean).join(' · '))),
    s.id ? h('button', { class: 'ghost sm', onclick: startVoice }, 'Talk') : null,
    s.id ? h('button', { class: 'ghost sm', onclick: () => openTools(s, 'skills') }, 'Add-ons') : null,
    s.id ? h('button', { class: 'ghost sm', onclick: () => { SS.opts = !SS.opts; drawPane(); } }, SS.opts ? 'Done' : 'Options') : null));
  if (SS.opts && s.id) pane.append(h('div', { class: 'opts' },
    SS.call !== null && (SS.machines.find((m) => m.id === s.mid) || {}).secure ? h('div', { class: 'row wrap' }, h('span', { class: 'muted small' }, 'Run on'),
      chip('Auto', (SS.run || 'auto') === 'auto', () => setRun('auto')), chip('Laptop', SS.run === 'laptop', () => setRun('laptop')), chip('Server only', SS.run === 'vps', () => setRun('vps')),
      h('span', { class: 'small muted' }, 'Auto: your laptop when it is online, otherwise the server. Laptop: wait until it is online, e.g. for testing.')) : null,
    SS.call !== null ? h('div', { class: 'row wrap' }, h('span', { class: 'muted small' }, 'Phone call'), chip("When it's done", SS.call === 'done', () => setCall('done')), chip('Also if it needs me', SS.call === 'attention', () => setCall('attention')),
      h('span', { class: 'small muted' }, 'You always get a message. A call only happens if you pick one here.')) : null,
    mach.local && s.cwd ? h('div', { class: 'row' }, h('span', { class: 'muted small grow ellip' }, s.cwd), h('button', { class: 'ghost sm', onclick: () => cm.openPath(s.cwd) }, 'Open folder')) : h('div', { class: 'small muted ellip' }, s.cwd),
    moveBox(s, mach), continueBox(s, mach)));
  pane.append(h('div', { class: 'msgs', id: 'msgs' }, h('div', { class: 'col', id: 'col' })));
  pane.append(h('div', { id: 'perms', class: 'perms' }));
  pane.append(composer());
  renderPerms();
  renderMsgs();
  if (SS.voice && SS.voice.el) pane.append(SS.voice.el);
}

function composer() {
  const s = SS.sel;
  const ta = h('textarea', { id: 'composer-text', rows: 1, placeholder: SS.sending ? 'Claude is working. Type to queue a follow-up…' : s.isNew ? 'Describe what you want to do…' : 'Message Claude' });
  const grow = () => { ta.style.height = 'auto'; ta.style.height = Math.min(220, ta.scrollHeight) + 'px'; };
  const menu = h('div', { class: 'slashmenu', hidden: true });
  const APP = [
    ['new', 'Start a new chat', () => newChat()], ['changes', 'See and save what changed', () => openTools(s, 'changes')], ['notes', 'Edit the project notes (CLAUDE.md)', () => openTools(s, 'notes')],
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
  ta.oninput = () => { grow(); showMenu(); };
  ta.onkeydown = (e) => { if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) { e.preventDefault(); submit(); } };
  const submit = () => {
    const refs = SS.files.map((f) => '\n@' + f.path).join('');
    const t = (ta.value.trim() + refs).trim();
    if (!t || SS.uploading) return;
    ta.value = ''; SS.files = []; renderFiles(); grow();
    if (SS.sending) { SS.queue.push(t); renderLive(); } else sendMsg(t);
  };
  const sel = (opts, cur, fn) => h('select', { class: 'sel', onchange: (e) => fn(e.target.value) }, opts.map(([v, l]) => h('option', { value: v, selected: v === cur }, l)));
  const box = h('div', { class: 'composer' }, h('div', { class: 'cbox' }, menu, h('div', { id: 'files', class: 'files' }), ta,
    h('div', { class: 'row cfoot' },
      sel([['auto', 'Auto'], ['acceptEdits', 'Edits'], ['plan', 'Plan only'], ['manual', 'Ask me'], ['bypassPermissions', 'Full auto']], SS.mode, (v) => { SS.mode = v; drawPane(); }),
      sel([['', 'Default model'], ['opus', 'Opus'], ['sonnet', 'Sonnet'], ['haiku', 'Haiku']], SS.model, (v) => { SS.model = v; }),
      sel([['', 'Default effort'], ['low', 'Low effort'], ['medium', 'Medium effort'], ['high', 'High effort'], ['xhigh', 'Extra high'], ['max', 'Max effort']], SS.effort, (v) => { SS.effort = v; }),
      h('div', { class: 'grow' }),
      SS.sending ? h('button', { class: 'ghost sm', id: 'stop', onclick: () => { SS.queue = []; if (s.id) cm.stop(s.mid, s.id); } }, 'Stop') : null,
      h('button', { class: 'primary sm', id: 'send', onclick: submit }, SS.sending ? 'Queue' : 'Send'))));
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
function serverIntro(s, info) {
  const nameIn = h('input', { placeholder: 'New project name, e.g. budget-app' }), err = h('div', { class: 'small err' });
  const go = async () => {
    try { const r = await cm.relay(s.mid, 'POST', '/projects', { name: nameIn.value.trim() }); s.cwd = r.path; SS.lastCwd = r.path; SS.projects[s.mid] = null; err.textContent = ''; const t = $('#composer-text'); if (t) t.focus(); renderMsgs(); }
    catch (e) { err.textContent = clean(e); }
  };
  nameIn.onkeydown = (e) => { if (e.key === 'Enter') go(); };
  return h('div', { class: 'intro' }, h('div', { html: sparkSVG(46) }), h('h2', {}, 'What should we build?'), computerPicker(s),
    h('div', { class: 'muted small' }, 'Projects folder on ' + ((SS.machines.find((m) => m.id === s.mid) || {}).name || 'the server')),
    h('div', { class: 'row', style: 'width:100%;max-width:560px' }, h('div', { class: 'grow' }, nameIn), h('button', { class: 'primary', onclick: go }, 'Create')), err,
    h('div', { class: 'row wrap', style: 'justify-content:center;max-width:560px' }, info.projects.slice(0, 8).map((p) => chip(p.name, s.cwd === p.path, () => { s.cwd = p.path; SS.lastCwd = p.path; renderMsgs(); }))),
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
  const status = t.result === undefined ? h('span', { class: 'spin', title: 'Running' }) : h('span', { class: t.error ? 'bad' : 'okc' }, t.error ? '✗' : '✓');
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
  SS.msgs.forEach((m, i) => {
    if (m.role === 'tool') { if (!tools) { tools = h('div', { class: 'tools' }); col.append(tools); } tools.append(toolCard(m, 'm' + i)); return; }
    tools = null;
    if (m.role === 'meta') { col.append(h('div', { class: 'meta' }, m.text)); return; }
    if (m.role === 'user') col.append(h('div', { class: 'msg user' }, m.text));
    else col.append(assistantEl(m.text));
  });
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
  if (SS.sending && !SS.parts.length) live.append(h('div', { class: 'typing' }, h('i'), h('i'), h('i')));
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
  SS.sending = true; SS.parts = []; SS.chatErr = null; SS.lastRun = null; SS.msgs.push({ role: 'user', text });
  drawPane();
  const finish = async () => {
    delete chatHandlers[sid];
    SS.sending = false; SS.parts = [];
    await loadMsgs(false);
    SS.perms = SS.perms.filter((p) => p.ev.session !== s.id);
    if (SS.sel === s) drawPane();
    loadList();
    if (SS.queue.length && SS.sel === s) sendMsg(SS.queue.shift());
  };
  chatHandlers[sid] = (ev) => {
    if (ev.t === 'queued') { toast(ev.online ? `Sent to ${ev.to}. It runs there now.` : 'Queued. It runs on your laptop when it is online.'); return; }
    if (ev.t === 'session') { if (!s.id) { s.id = ev.id; s.isNew = false; s.title = text.slice(0, 70); drawSide(); } return; }
    if (ev.t === 'delta') { const last = SS.parts[SS.parts.length - 1]; if (last && last.type === 'text') last.text += ev.text; else SS.parts.push({ type: 'text', text: ev.text }); }
    else if (ev.t === 'tool') SS.parts.push({ type: 'tool', id: ev.id, name: ev.name, input: ev.input, text: ev.text });
    else if (ev.t === 'result') { const t = SS.parts.find((p) => p.type === 'tool' && p.id === ev.id); if (t) { t.result = ev.text; t.error = ev.error; } }
    else if (ev.t === 'done') SS.lastRun = { sid: s.id, ms: ev.ms, error: ev.error };
    else if (ev.t === 'fail') { SS.chatErr = ev.text; finish(); return; }
    else if (ev.t === 'end') { finish(); return; }
    scheduleLive();
  };
  cm.send(s.mid, s.id || null, text, SS.mode, false, sid, { cwd: s.isNew && s.kind !== 'talk' ? s.cwd : undefined, kind: s.isNew && s.kind === 'talk' ? 'talk' : undefined, model: SS.model || undefined, effort: SS.effort || undefined })
    .catch((e) => { SS.chatErr = clean(e); finish(); });
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
    box.append(h('div', { class: 'perm' }, h('div', { class: 'grow' }, h('div', { class: 'label' }, 'Claude needs your OK'), h('div', { class: 'perm-t' }, p.ev.text)),
      h('button', { class: 'ghost sm', onclick: () => decide(p, 'deny') }, 'Deny'), h('button', { class: 'primary sm', onclick: () => decide(p, 'allow') }, 'Allow')));
  }
}
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
  if (ev.kind === 'resolved') { SS.perms = SS.perms.filter((p) => p.ev.req !== ev.req); renderPerms(); return; }
  if ((ev.kind === 'permission' || ev.kind === 'question') && ev.req) {
    if (!SS.perms.some((p) => p.ev.req === ev.req)) SS.perms.push({ machine, ev });
    renderPerms();
    if (!viewing) toast(`${proj}: ${ev.kind === 'question' ? 'Claude has a question' : 'Claude needs your OK'}`, () => openSession({ id: ev.session, title: ev.title, cwd: ev.cwd, mid: machine, live: true }));
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
