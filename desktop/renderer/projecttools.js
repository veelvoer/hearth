'use strict';
/* The "Project" panel: what changed (and save it), the project's notes, and connections. Opened from the chat header or the / menu. */
const PT = { tab: 'changes', el: null, s: null };
const tcall = (s, method, p, body) => cm.relay(s.mid, method, '/tools/' + p, body);

function openTools(s, tab) {
  if (!s || !s.mid) return;
  PT.s = s; PT.tab = tab || PT.tab;
  if (!PT.el && document.startViewTransition) { try { document.startViewTransition(() => openToolsNow(s, tab)); return; } catch { /* plain */ } }
  openToolsNow(s, tab);
}
function openToolsNow(s, tab) {
  PT.s = s; PT.tab = tab || PT.tab;
  if (!PT.el) { PT.el = h('div', { class: 'modal', onclick: (e) => { if (e.target === PT.el) closeTools(); } }); document.body.append(PT.el); }
  drawTools();
}
function closeTools() { if (PT.el) PT.el.remove(); PT.el = null; }

function drawTools() {
  const s = PT.s, card = h('div', { class: 'mcard' });
  const tabs = [['skills', 'Skills'], ['plugins', 'Plugins'], ['connections', 'Connections'], ['changes', 'Changes'], ['notes', 'Notes']];
  card.append(h('div', { class: 'row' }, h('div', { class: 'grow row wrap' }, tabs.map(([k, l]) => chip(l, PT.tab === k, () => { PT.tab = k; drawTools(); }))), ibtn('close', 'Close', closeTools, 'ghost sm')));
  const body = h('div', { class: 'mbody' }); card.append(body);
  PT.el.innerHTML = ''; PT.el.append(card);
  ({ changes: viewChanges, notes: viewNotes, connections: viewConnections, skills: viewSkills, plugins: viewPlugins })[PT.tab](body, s);
}

async function viewChanges(body, s) {
  body.append(h('p', { class: 'muted small' }, 'Loading…'));
  let st, stash = 0;
  try { st = await tcall(s, 'GET', 'git/status?cwd=' + encodeURIComponent(s.cwd || '')); } catch (e) { body.innerHTML = ''; body.append(h('p', { class: 'err small' }, clean(e))); return; }
  if (PT.tab !== 'changes') return;
  body.innerHTML = '';
  if (!st.repo) { body.append(h('p', { class: 'muted' }, 'This project isn’t tracked yet, so there is nothing to compare.'), h('p', { class: 'small muted' }, 'Ask Claude: “set up git for this project”.')); return; }
  try { stash = (await tcall(s, 'GET', 'git/stashes?cwd=' + encodeURIComponent(s.cwd))).count; } catch { /* ignore */ }
  const refresh = () => { if (PT.tab === 'changes') drawTools(); };
  const act = async (p, b, ok) => { try { await tcall(s, 'POST', p, { cwd: s.cwd, ...b }); if (ok) toast(ok); } catch (e) { toast(clean(e)); } refresh(); };
  body.append(h('div', { class: 'small muted' }, 'Branch ' + st.branch + (st.last ? ' · last save: ' + st.last : '')));
  if (!st.files.length) body.append(h('p', {}, 'Everything is saved. No changes since the last save.'));
  else {
    const list = h('div', { class: 'flist' });
    st.files.forEach((f) => {
      const row = h('details', { class: 'frow' }, h('summary', {}, h('span', { class: 'tag ' + f.state }, f.state), h('span', { class: 'ellip' }, f.path)));
      row.ontoggle = async () => { if (!row.open || row.dataset.loaded) return; row.dataset.loaded = 1; try { const d = await tcall(s, 'GET', 'git/diff?cwd=' + encodeURIComponent(s.cwd) + '&file=' + encodeURIComponent(f.path)); row.append(diffText(d.diff)); } catch (e) { row.append(h('p', { class: 'err small' }, clean(e))); } };
      list.append(row);
    });
    const msg = h('input', { placeholder: 'What did you change? (a few words)', class: 'grow' });
    body.append(list, h('div', { class: 'row', style: 'margin-top:10px' }, msg,
      h('button', { class: 'primary sm', onclick: () => msg.value.trim() ? act('git/commit', { message: msg.value }, 'Saved') : toast('Write a few words about the change first') }, 'Save changes')),
      h('div', { class: 'row', style: 'margin-top:8px' }, h('button', { class: 'ghost sm', onclick: () => { if (confirm('Set all these changes aside? Your files go back to the last save. You can bring them back afterwards.')) act('git/aside', {}, 'Changes set aside'); } }, 'Undo all changes'), h('span', { class: 'small muted' }, 'Safe: you can bring them back.')));
  }
  if (stash) body.append(h('div', { class: 'row', style: 'margin-top:8px' }, h('button', { class: 'ghost sm', onclick: () => act('git/bring-back', {}, 'Changes brought back') }, 'Bring back set-aside changes'), h('span', { class: 'small muted' }, stash + ' set aside')));
}
function diffText(t) {
  const d = h('div', { class: 'diff' });
  (t || '(no preview)').split('\n').filter((l) => !/^(diff --git|index |--- |\+\+\+ )/.test(l)).forEach((l) => d.append(h('div', { class: l[0] === '+' ? 'add' : l[0] === '-' ? 'del' : '' }, l)));
  return d;
}

async function viewNotes(body, s) {
  let scope = PT.scope || 'project';
  const load = async () => {
    body.innerHTML = '';
    body.append(h('div', { class: 'row' }, chip('This project', scope === 'project', () => { PT.scope = scope = 'project'; load(); }), chip('Everywhere', scope === 'global', () => { PT.scope = scope = 'global'; load(); })),
      h('p', { class: 'small muted' }, scope === 'project' ? 'Things Claude should always know about this project: how to run it, what style you like, what to avoid.' : 'Things Claude should know in every project, like your preferences.'));
    let r; try { r = await tcall(s, 'GET', 'notes?scope=' + scope + '&cwd=' + encodeURIComponent(s.cwd || '')); } catch (e) { body.append(h('p', { class: 'err small' }, clean(e))); return; }
    const ta = h('textarea', { rows: 12, class: 'notes', placeholder: 'Nothing here yet.' }); ta.value = r.text;
    body.append(ta, h('div', { class: 'row', style: 'margin-top:8px' }, h('button', { class: 'primary sm', onclick: async () => { try { await tcall(s, 'POST', 'notes/save', { scope, cwd: s.cwd, text: ta.value }); toast('Notes saved'); } catch (e) { toast(clean(e)); } } }, 'Save notes')));
  };
  load();
}

/** A search box on top and a list below that refills as you type. */
function searchList(body, placeholder, fetchRows, delay = 0) {
  const box = h('input', { class: 'search', placeholder, autofocus: true }), list = h('div', { class: 'flist' }), note = h('div', { class: 'small muted' });
  body.append(box, list, note);
  let seq = 0, timer = null;
  const go = async () => {
    const my = ++seq; list.style.opacity = .5;
    try { const r = await fetchRows(box.value.trim()); if (my !== seq) return; list.innerHTML = ''; r.rows.forEach((x) => list.append(x)); note.textContent = r.note || ''; if (!r.rows.length) list.append(h('p', { class: 'muted' }, 'Nothing found.')); }
    catch (e) { if (my === seq) { list.innerHTML = ''; list.append(h('p', { class: 'err small' }, clean(e))); } }
    list.style.opacity = 1;
  };
  box.oninput = () => { clearTimeout(timer); timer = setTimeout(go, delay); };
  go(); setTimeout(() => box.focus(), 0);
  return go;
}
const itemRow = (title, sub, ...actions) => h('div', { class: 'row crow' }, h('div', { class: 'grow', style: 'min-width:0' }, h('b', {}, title), sub ? h('div', { class: 'small muted clamp' }, sub) : null), ...actions);

function viewSkills(body, s) {
  searchList(body, 'Search skills and commands', async (q) => {
    SS.cmds = null;
    const all = await tcall(s, 'GET', 'commands?cwd=' + encodeURIComponent(s.cwd || ''));
    const hit = all.filter((x) => !q || (x.name + ' ' + (x.desc || '')).toLowerCase().includes(q.toLowerCase()));
    return { rows: hit.map((x) => itemRow('/' + x.name, (x.desc || '') + (x.src ? '  ·  ' + x.src : ''), h('button', { class: 'ghost sm', onclick: () => useCommand(x.name) }, 'Use'))), note: 'Skills come from your own folders, the project, and installed plugins. Find more under Plugins.' };
  });
}
function useCommand(name) {
  closeTools();
  const ta = document.getElementById('composer-text');
  if (ta) { ta.value = '/' + name + ' '; ta.focus(); ta.dispatchEvent(new Event('input')); } else toast('Open a chat first, then pick the skill');
}

function viewPlugins(body, s) {
  let again;
  const act = async (verb, id, ok) => { try { await tcall(s, 'POST', 'plugins/' + verb, { id }); toast(ok); SS.cmds = null; } catch (e) { toast(clean(e)); } again(); };
  again = searchList(body, 'Search plugins to add (skills, tools, helpers)', async (q) => {
    const r = await tcall(s, 'GET', 'plugins?q=' + encodeURIComponent(q));
    const rows = [];
    r.installed.filter((p) => !q || p.name.toLowerCase().includes(q.toLowerCase())).forEach((p) => rows.push(itemRow(p.name, 'Installed · ' + (p.enabled ? 'on' : 'off'),
      h('button', { class: 'ghost sm', onclick: () => act(p.enabled ? 'disable' : 'enable', p.id, p.enabled ? 'Turned off' : 'Turned on') }, p.enabled ? 'Turn off' : 'Turn on'),
      h('button', { class: 'ghost sm', onclick: () => confirm('Remove ' + p.name + '?') && act('uninstall', p.id, 'Removed') }, 'Remove'))));
    r.available.forEach((p) => rows.push(itemRow(p.name, p.desc, h('button', { class: 'primary sm', onclick: (e) => { e.target.textContent = 'Adding…'; e.target.disabled = true; act('install', p.id, 'Added ' + p.name); } }, 'Add'))));
    return { rows, note: r.total ? (r.available.length >= 40 ? 'Showing the top 40 of ' + r.total + '. Type to narrow it down.' : r.total + ' plugins available') : 'Plugin store not available here.' };
  }, 250);
}

function viewConnections(body, s) {
  let again;
  const del = async (name) => { if (!confirm('Remove ' + name + '?')) return; try { await tcall(s, 'POST', 'mcp/remove', { name }); } catch (e) { toast(clean(e)); } again(); };
  const add = h('div', { class: 'stack', style: 'margin-top:10px' });
  const name = h('input', { placeholder: 'Name, e.g. github' }), target = h('input', { class: 'grow', placeholder: 'Web address (https://…) or command' });
  add.append(h('div', { class: 'small muted' }, 'Add your own connection'), h('div', { class: 'row' }, name, target, h('button', { class: 'primary sm', onclick: async () => { try { await tcall(s, 'POST', 'mcp/add', { name: name.value, target: target.value }); toast('Added'); name.value = target.value = ''; } catch (e) { toast(clean(e)); } again(); } }, 'Add')));
  let cache = null;
  again = searchList(body, 'Search your connections', async (q) => {
    if (!cache || again.fresh) { cache = (await tcall(s, 'GET', 'mcp?cwd=' + encodeURIComponent(s.cwd || ''))).items; }
    const hit = cache.filter((x) => !q || (x.name + x.target).toLowerCase().includes(q.toLowerCase()));
    return { rows: hit.map((x) => itemRow(x.name, x.target, h('span', { class: 'small ' + (x.status === 'connected' ? 'okc' : 'bad') }, x.status), x.name.startsWith('claude.ai ') ? null : h('button', { class: 'ghost sm', onclick: () => { cache = null; del(x.name); } }, 'Remove'))), note: 'Connections that start with “claude.ai” are managed in your Claude account settings. Every active connection adds a little to each message.' };
  });
  const orig = again; again = () => { cache = null; return orig(); }; 
  body.append(add);
}
