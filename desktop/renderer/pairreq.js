'use strict';
/* "A phone wants to connect": the phone asked on your Wi-Fi, you say yes or no. No address or code to type. */
(function () {
  let list = [], card = null;
  const remove = () => { if (card) { card.remove(); card = null; } };
  async function decide(id, accept) {
    try { list = await cm.decidePair(id, accept); } catch (e) { toast(clean(e)); list = list.filter((r) => r.id !== id); }
    if (accept) toast('Phone connected');
    draw();
  }
  function draw() {
    remove();
    if (!list.length) return;
    const r = list[0];
    card = h('div', { class: 'updcard pairreq' }, h('div', { class: 'row' }, h('div', { html: sparkSVG(22) }), h('h3', { class: 'grow' }, r.name + ' wants to connect')),
      h('div', { class: 'small muted' }, 'A phone on your Wi-Fi (' + r.ip + ') is asking to use Hearth with this computer. Only accept if it is yours.'),
      h('div', { class: 'row', style: 'margin-top:12px' }, h('div', { class: 'grow' }), h('button', { class: 'ghost sm', onclick: () => decide(r.id, false) }, 'Decline'), h('button', { class: 'primary sm', onclick: () => decide(r.id, true) }, 'Accept')),
      list.length > 1 ? h('div', { class: 'small muted' }, (list.length - 1) + ' more waiting') : null);
    cardHost().append(card);
  }
  cm.on('pair-requests', (l) => { list = l || []; draw(); });
  window.addEventListener('load', () => { cm.pairRequests().then((l) => { list = l || []; draw(); }).catch(() => {}); });
})();
