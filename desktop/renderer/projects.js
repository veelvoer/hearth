'use strict';
/* "A project was made on your server": a card asking whether to install it on this computer. Later brings it back next time. */
(function () {
  let list = [], card = null, later = false, done = null;
  const size = (b) => b > 1e6 ? (b / 1e6).toFixed(1) + ' MB' : Math.max(1, Math.round(b / 1e3)) + ' KB';
  const remove = () => { if (card) { card.remove(); card = null; } };

  function draw() {
    remove();
    if (done) return drawDone();
    if (!list.length || later) return;
    const rows = list.map((p) => h('div', { class: 'row crow' }, h('div', { class: 'grow', style: 'min-width:0' }, h('b', { class: 'ellip', style: 'display:block' }, p.name), h('div', { class: 'small muted' }, p.files + (p.files === 1 ? ' file' : ' files') + ' · ' + size(p.bytes))),
      h('button', { class: 'primary sm', onclick: () => install([p.name]) }, 'Install')));
    card = h('div', { class: 'updcard' }, h('div', { class: 'row' }, h('div', { html: sparkSVG(22) }), h('h3', { class: 'grow' }, list.length === 1 ? 'A new project is waiting' : list.length + ' new projects are waiting')),
      h('div', { class: 'small muted' }, 'Made on your server while this computer was off. Install copies it here so you can work on it.'), h('div', { class: 'stack', style: 'gap:2px;margin:8px 0;max-height:220px;overflow-y:auto;overflow-x:hidden' }, rows),
      h('div', { class: 'row' }, h('div', { class: 'grow' }), h('button', { class: 'ghost sm', onclick: () => { later = true; remove(); } }, 'Later'), list.length > 1 ? h('button', { class: 'primary sm', onclick: () => install('all') }, 'Install all') : null));
    document.body.append(card);
  }
  async function install(what) {
    const names = what === 'all' ? list.map((p) => p.name) : what, paths = list.filter((p) => names.includes(p.name));
    remove(); card = h('div', { class: 'updcard' }, h('div', { class: 'row' }, h('div', { html: sparkSVG(22) }), h('h3', { class: 'grow' }, 'Installing…')), h('div', { class: 'small muted' }, 'Copying the project to this computer.')); document.body.append(card);
    try { list = await cm.acceptProjects(what === 'all' ? 'all' : names); done = paths; } catch (e) { remove(); toast(clean(e)); return; }
    draw();
  }
  function drawDone() {
    remove(); const p = done[0]; const many = done.length > 1;
    card = h('div', { class: 'updcard' }, h('div', { class: 'row' }, h('div', { html: sparkSVG(22) }), h('h3', { class: 'grow' }, many ? 'Projects installed' : p.name + ' is installed')),
      h('div', { class: 'small muted' }, 'The files are on this computer now. Claude can also set it up for you (install what it needs and tell you how to start it).'),
      h('div', { class: 'row', style: 'margin-top:10px' }, h('div', { class: 'grow' }), h('button', { class: 'ghost sm', onclick: () => { done = null; remove(); draw(); } }, 'Done'),
        many ? null : h('button', { class: 'primary sm', onclick: () => { const d = done[0]; done = null; remove(); setupWithClaude(d.path); } }, 'Set it up with Claude')));
    document.body.append(card);
  }
  function setupWithClaude(dir) {
    newChat(); SS.sel.cwd = dir; SS.sel.title = 'Set up ' + dir.split(/[\\/]/).pop();
    sendMsg('Set this project up on this computer so it runs: install the tools and dependencies it needs, and tell me in simple words how to start it. Do not change the project\'s code.');
  }
  cm.on('pending-projects', (l) => { list = l || []; if (!list.length) { later = false; } draw(); });
  window.addEventListener('load', () => { later = false; cm.pendingProjects().then((l) => { list = l || []; draw(); }).catch(() => {}); });
})();
