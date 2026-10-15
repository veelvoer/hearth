'use strict';
/* The "new version is ready" popup. Shown when the project files on this computer are newer than what runs or is installed. */
(function () {
  let avail = null, run = null, card = null, timer = null, idleSig = '';
  const LABEL = { github: 'Newest code', server: 'Server', phone: 'Phone app', desktop: 'This app' };
  const remove = () => { if (card) { card.remove(); card = null; } clearTimeout(timer); };

  function drawIdle() {
    const sig = JSON.stringify([avail.mode, avail.latest, avail.notes, !!avail.github, !!avail.server, !!avail.phone, !!avail.desktop]);
    if (card && card.isConnected && card.dataset.kind === 'idle' && sig === idleSig) return;   // the same news again: leave the card as it is (redrawing it replays its entrance and checkboxes reset)
    idleSig = sig;
    const again = !!card; remove();
    const want = { github: !!avail.github, server: !!avail.server, phone: !!avail.phone, desktop: !!avail.desktop };
    const rows = ['github', 'server', 'phone', 'desktop'].filter((k) => avail[k]).map((k) => {
      const box = h('input', { type: 'checkbox', checked: true, onchange: () => { want[k] = box.checked; } });
      return h('label', { class: 'updrow' }, box, h('span', {}, LABEL[k], h('span', { class: 'muted small' }, '  ' + (avail.mode === 'release' ? { github: '', server: 'your server gets the new version', phone: '', desktop: 'downloads and restarts' } : { github: 'downloads from GitHub first', server: 'your server gets the new version', phone: 'builds and installs over USB', desktop: 'restarts with the new version' })[k])));
    });
    const notes = avail.notes ? h('div', { class: 'updnotes' }, h('div', { class: 'label' }, 'What is new'), ...String(avail.notes).split('\n').map((l) => l.trim()).filter(Boolean).slice(0, 14).map((l) => h(/^#/.test(l) ? 'b' : 'div', { class: 'small' }, l.replace(/^#+\s*/, '').replace(/^[-*]\s+/, '• ').replace(/\*\*(.+?)\*\*/g, '$1').replace(/`([^`]+)`/g, '$1')))) : null;
    card = h('div', { class: 'updcard' + (again ? ' again' : ''), 'data-kind': 'idle' }, h('div', { class: 'row' }, h('div', { html: sparkSVG(22) }), h('h3', { class: 'grow' }, 'A new version is ready')),
      h('div', { class: 'small muted' }, avail.mode === 'release' ? 'Hearth ' + (avail.latest || '') + ' is available on GitHub.' : 'Your projects folder has newer code than what is installed.'), notes, h('div', { class: 'stack', style: 'gap:6px;margin:10px 0' }, rows),
      h('div', { class: 'row' }, h('div', { class: 'grow' }), h('button', { class: 'ghost sm', onclick: () => { cm.updateLater(); remove(); } }, 'Later'),
        h('button', { class: 'primary sm', onclick: () => { run = {}; cm.updateRun(want); drawRun(); } }, 'Update now')));
    cardHost().append(card);
  }

  function drawRun(done) {
    const again = !!card; remove(); idleSig = '';
    const lines = Object.entries(run).map(([k, v]) => h('div', { class: 'updline ' + v.state }, h('span', { class: 'updic' }, v.state === 'ok' ? '✓' : v.state === 'fail' ? '✗' : ''), h('div', {}, h('b', {}, LABEL[k] || k), h('div', { class: 'small ' + (v.state === 'fail' ? 'err' : 'muted') }, v.text || ''))));
    const failed = Object.values(run).some((v) => v.state === 'fail');
    card = h('div', { class: 'updcard' + (again ? ' again' : ''), 'data-kind': 'run' }, h('div', { class: 'row' }, h('div', { html: sparkSVG(22) }), h('h3', { class: 'grow' }, done ? (failed ? 'Not everything worked' : 'Everything is up to date') : 'Updating…')),
      h('div', { class: 'stack', style: 'gap:8px;margin:10px 0' }, lines.length ? lines : [h('div', { class: 'small muted' }, 'Starting…')]),
      done ? h('div', { class: 'row' }, h('div', { class: 'grow' }), failed ? h('button', { class: 'ghost sm', onclick: () => { cm.updateLater(); remove(); } }, 'Close') : null,
        failed ? h('button', { class: 'primary sm', onclick: async () => { avail = await cm.updateStatus(); if (avail) drawIdle(); else remove(); } }, 'Try again') : null) : null);
    cardHost().append(card);
    if (done && !failed) timer = setTimeout(remove, 4000);
  }

  cm.on('update-available', (a) => { if (run) return; avail = a; drawIdle(); });
  cm.on('update-progress', (p) => {
    if (!run) run = {};
    if (p.step === 'end') { drawRun(true); run = null; return; }
    run[p.step] = { text: p.text, state: p.state };
    drawRun(false);
  });
})();
