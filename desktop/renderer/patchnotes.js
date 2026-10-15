'use strict';
/* The patch notes popup: after an update and a restart, one window with what changed in every version since the last one you saw. */
(function () {
  const lines = (md) => String(md).split('\n').map((l) => l.trim()).filter(Boolean);
  const inline = (t) => { const f = document.createDocumentFragment(); String(t).split(/(\*\*[^*]+\*\*|`[^`]+`)/).forEach((p) => { if (/^\*\*[^*]+\*\*$/.test(p)) f.append(h('b', {}, p.slice(2, -2))); else if (/^`[^`]+`$/.test(p)) f.append(h('code', {}, p.slice(1, -1))); else if (p) f.append(p); }); return f; };
  function block(n) {
    const box = h('div', { class: 'pnver' }, h('h3', {}, 'Hearth ' + n.version));
    for (const l of lines(n.text)) {
      if (/^#+\s/.test(l)) box.append(h('div', { class: 'pnhead' }, l.replace(/^#+\s*/, '')));
      else if (/^[-*]\s+/.test(l)) box.append(h('div', { class: 'pnitem' }, h('span', { class: 'pndot' }, '•'), h('div', {}, inline(l.replace(/^[-*]\s+/, '')))));
      else box.append(h('div', { class: 'pntext' }, inline(l)));
    }
    return box;
  }
  function show(list, cur) {
    const over = h('div', { class: 'overlay pnover' });
    const close = () => { over.remove(); };
    over.append(h('div', { class: 'modal pnmodal' }, h('div', { class: 'row' }, h('div', { html: sparkSVG(26) }), h('div', { class: 'grow' }, h('h2', {}, 'Updated to ' + cur), h('div', { class: 'small muted' }, list.length > 1 ? 'Everything that changed since the version you had.' : 'What changed in this version.'))),
      h('div', { class: 'pnbody' }, ...list.map(block)), h('div', { class: 'row' }, h('div', { class: 'grow' }), h('button', { class: 'primary', onclick: close }, 'Got it'))));
    over.onclick = (e) => { if (e.target === over) close(); };
    document.addEventListener('keydown', function esc(e) { if (e.key === 'Escape' || e.key === 'Enter') { close(); document.removeEventListener('keydown', esc); } });
    document.body.append(over);
  }
  async function check() {
    try {
      const cur = await cm.version(); if (!cur) return;
      const last = localStorage.getItem('cm.notesSeen');
      localStorage.setItem('cm.notesSeen', cur);
      if (last === cur) return;
      if (!last && !(S.settings && S.settings.onboarded)) return;   // a fresh install: nothing was updated
      const list = await cm.notesSince(last || null);
      if (list && list.length) show(list, cur);
    } catch { /* no notes: no popup */ }
  }
  window.addEventListener('load', () => setTimeout(check, 2500));
})();
