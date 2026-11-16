'use strict';
/* Material 3 Expressive motion for the "Material 3" style (the desktop side of the phone's Expressive.kt).
   Everything that moves here moves on real springs: it can be interrupted at any moment and carries its speed into
   the next target. Movement may overshoot; light and opacity never do. Only active with data-style="m3", with
   animations on and without "reduce motion"; it only ever sets transform and opacity, and stops when nothing moves. */
(() => {
  const root = document.documentElement;
  const reduce = matchMedia('(prefers-reduced-motion: reduce)');
  const on = () => root.dataset.style === 'm3' && !root.classList.contains('noanim') && !reduce.matches;

  // ── the spring engine: one frame loop for every spring, idle when nothing moves ──
  const live = new Set(), dirty = new Set();
  let inFrame = false;
  const mark = (f) => { if (inFrame) dirty.add(f); else f(); };
  let raf = 0, last = 0;
  function frame(t) {
    const dt = Math.min(0.064, last ? (t - last) / 1000 : 1 / 60); last = t;
    inFrame = true;
    for (const s of live) s.step(dt);
    inFrame = false;
    for (const f of dirty) f(); dirty.clear();   // one style write per element per frame, however many of its springs moved
    for (const s of [...live]) if (s.rest) { live.delete(s); s.done && s.done(); }
    raf = live.size ? requestAnimationFrame(frame) : (last = 0, 0);
  }
  class Spring {
    constructor(v, onChange) { this.v = v; this.t = v; this.vel = 0; this.rest = true; this.onChange = onChange; this.k = 380; this.z = 0.8; }
    /** Move toward [t] with damping ratio [z] and stiffness [k]; the current speed is kept. */
    to(t, z = this.z, k = this.k, done) {
      this.t = t; this.z = z; this.k = k; this.done = done;
      if (!on()) { this.snap(t); done && done(); return this; }
      if (Math.abs(this.v - t) < 1e-4 && Math.abs(this.vel) < 1e-3) { this.snap(t); done && done(); return this; }
      this.rest = false; live.add(this); if (!raf) raf = requestAnimationFrame(frame);
      return this;
    }
    snap(t) { this.v = this.t = t; this.vel = 0; this.rest = true; live.delete(this); this.onChange(this.v); }
    kick(vel) { this.vel += vel; if (on()) { this.rest = false; live.add(this); if (!raf) raf = requestAnimationFrame(frame); } }
    step(dt) {
      const c = 2 * this.z * Math.sqrt(this.k);
      for (let n = Math.ceil(dt / (1 / 240)), h = dt / n; n > 0; n--) {   // small steps: stiff springs stay stable at any frame rate
        this.vel += (-this.k * (this.v - this.t) - c * this.vel) * h;
        this.v += this.vel * h;
      }
      if (Math.abs(this.v - this.t) < 5e-4 && Math.abs(this.vel) < 5e-3) { this.v = this.t; this.vel = 0; this.rest = true; }
      this.onChange(this.v);
    }
  }
  // the Material 3 Expressive springs
  const FAST = [0.6, 800], MOVE = [0.8, 380], SLOW = [0.8, 200], FADE = [1, 1600], FOLLOW = [0.82, 550], RELEASE = [0.52, 350], SETTLE = [0.55, 420];

  // ── the light layer: a glow under the pointer and a ring from the press, drawn above an element without touching it ──
  function makeLight(el) {
    const d = document.createElement('div');
    d.className = 'm3light';
    d.innerHTML = '<i class="glow"></i><i class="ring"></i>';
    document.body.append(d);
    const fit = () => {
      const r = el.getBoundingClientRect(), cs = getComputedStyle(el);
      Object.assign(d.style, { left: r.left + 'px', top: r.top + 'px', width: r.width + 'px', height: r.height + 'px', borderRadius: cs.borderRadius });
      return r;
    };
    return { d, glow: d.firstChild, ring: d.lastChild, fit };
  }

  // ── the elastic press: leans toward the pointer, stretches a little that way, springs back past rest on release ──
  const SEL = 'button:not(.toggle):not(:disabled), .item, .chip, .hitem, .rcard.click, .card.click, .sitem, [data-elastic]';
  const state = new WeakMap();
  function st(el) {
    let s = state.get(el); if (s) return s;
    s = { dx: 0, dy: 0, sc: 1, glow: 0, light: null, w: 1, h: 1, ox: 0, oy: 0 };
    const apply = () => {
      const big = Math.max(s.w, s.h), travel = big > 160 ? 1.5 : 2.5, stretch = big > 160 ? 0.025 : 0.06;
      const sx = s.sc * (1 + stretch * Math.abs(s.dx) - 0.03 * Math.abs(s.dy)), sy = s.sc * (1 + stretch * 1.06 * Math.abs(s.dy) - 0.03 * Math.abs(s.dx));
      const resting = Math.abs(s.dx) < 1e-3 && Math.abs(s.dy) < 1e-3 && Math.abs(s.sc - 1) < 1e-4;
      if (resting) { el.style.transform = ''; el.style.transformOrigin = ''; el.style.transitionProperty = ''; }
      else {
        el.style.transitionProperty = 'background-color, color, box-shadow, border-color, opacity';   // the spring drives transform, not a CSS transition
        el.style.transformOrigin = `${(50 - 30 * s.dx).toFixed(1)}% ${(50 - 30 * s.dy).toFixed(1)}%`;
        el.style.transform = `translate(${(s.dx * travel).toFixed(2)}px, ${(s.dy * travel).toFixed(2)}px) scale(${sx.toFixed(4)}, ${sy.toFixed(4)})`;
      }
      if (s.light) {
        s.light.d.style.transform = el.style.transform; s.light.d.style.transformOrigin = el.style.transformOrigin;
        s.light.glow.style.opacity = s.glow.toFixed(3);
      }
    };
    s.apply = apply;
    s.X = new Spring(0, (v) => { s.dx = v; mark(apply); });
    s.Y = new Spring(0, (v) => { s.dy = v; mark(apply); });
    s.S = new Spring(1, (v) => { s.sc = v; mark(apply); });
    s.G = new Spring(0, (v) => { s.glow = v; mark(apply); });
    state.set(el, s);
    return s;
  }
  function ensureLight(el, s) {
    if (!s.light) s.light = makeLight(el);
    const r = s.light.fit(); s.w = r.width; s.h = r.height; s.r = r; return r;   // measured once per press, not on every move
  }
  function dropLight(s) { if (s.light && s.G.rest && s.G.v < 0.01 && !s.ringing) { s.light.d.remove(); s.light = null; } }
  function aim(el, s, e, r, pull) {
    const cx = r.left + r.width / 2, cy = r.top + r.height / 2;
    const dx = Math.max(-1, Math.min(1, (e.clientX - cx) / (r.width / 2 || 1))), dy = Math.max(-1, Math.min(1, (e.clientY - cy) / (r.height / 2 || 1)));
    s.X.to(dx * pull, ...FOLLOW); s.Y.to(dy * pull, ...FOLLOW);
    const big = Math.max(r.width, r.height);
    s.light.glow.style.setProperty('--x', (e.clientX - r.left) + 'px'); s.light.glow.style.setProperty('--y', (e.clientY - r.top) + 'px');
    s.light.glow.style.setProperty('--r', (big * 0.9) + 'px');
  }
  let pressed = null, hovered = null;
  function release(el) {
    const s = state.get(el); if (!s) return;
    s.X.to(0, ...RELEASE); s.Y.to(0, ...RELEASE); s.S.to(1, ...SETTLE);
    s.G.to(0, ...FADE, () => dropLight(s));
  }
  addEventListener('pointerdown', (e) => {
    if (!on() || e.button !== 0) return;
    const el = e.target.closest && e.target.closest(SEL); if (!el || el.closest('.m3off')) return;
    const s = st(el), r = ensureLight(el, s);
    pressed = el;
    s.S.to(Math.max(0.94, Math.min(0.985, 1 - 10 / Math.max(r.width, r.height, 1))), 1, 1400);
    s.G.to(1, ...FADE);
    aim(el, s, e, r, 1);
    // the ring spreads from where you pressed
    const ring = s.light.ring, big = Math.max(r.width, r.height) * 2.6;
    Object.assign(ring.style, { left: (e.clientX - r.left) + 'px', top: (e.clientY - r.top) + 'px', width: big + 'px', height: big + 'px' });
    s.ringing = true;
    const a = ring.animate([{ transform: 'translate(-50%,-50%) scale(0)', opacity: 1 }, { transform: 'translate(-50%,-50%) scale(1)', opacity: 0 }], { duration: 520, easing: 'cubic-bezier(.2,0,0,1)' });
    a.onfinish = () => { s.ringing = false; dropLight(s); };
  }, true);
  addEventListener('pointermove', (e) => {
    if (!on()) return;
    if (!pressed) return;   // hovering costs nothing here: the hover color is plain CSS
    const s = state.get(pressed); if (s && s.light && s.r) aim(pressed, s, e, s.r, 1);
  }, { passive: true });
  const up = () => { if (pressed) { const el = pressed; pressed = null; if (el !== hovered) release(el); else { const s = state.get(el); s.S.to(1, ...SETTLE); s.X.to(0, ...RELEASE); s.Y.to(0, ...RELEASE); s.G.to(0.38, ...FADE); } } };
  addEventListener('pointerup', up, true);
  addEventListener('pointercancel', up, true);
  addEventListener('blur', () => { up(); if (hovered) { release(hovered); hovered = null; } });
  document.addEventListener('pointerleave', () => { if (hovered) { release(hovered); hovered = null; } });
  addEventListener('scroll', () => { if (hovered) { release(hovered); hovered = null; } }, { capture: true, passive: true });

  // ── the navigation: one marker that glides to the chosen place, stretches with its speed and can be dragged ──
  const nav = document.getElementById('nav');
  let pill = null, pillY = null, pillS = null, pillFor = null, drag = null;
  function navOn() { return root.dataset.style === 'm3' && nav && nav.querySelector('.tabs button[data-tab]'); }
  function pillTarget(btn) {
    const tabs = btn.parentElement, ic = btn.querySelector('.ic') || btn;
    const a = ic.getBoundingClientRect(), b = tabs.getBoundingClientRect();
    return { x: a.left - b.left, y: a.top - b.top + tabs.scrollTop, w: a.width, h: a.height };
  }
  const horiz = (tabs) => getComputedStyle(tabs).flexDirection.startsWith('row');   // the floating dock runs sideways, the rail runs down
  const along = (t, h) => (h ? t.x : t.y);
  function syncPill(animate) {
    if (!navOn()) { if (pill) { pill.remove(); pill = null; } root.classList.remove('m3pill'); return; }
    const tabs = nav.querySelector('.tabs'), btn = tabs.querySelector('button.on');
    const h = horiz(tabs);
    if (!pill || !tabs.contains(pill) || pill._h !== h) {
      if (pill) pill.remove();
      pill = document.createElement('span'); pill.className = 'm3marker'; pill._h = h; tabs.prepend(pill);
      const draw = () => { const p = pillY.v, s = pillS ? pillS.v : 1;
        pill.style.transform = h ? `translate(${p}px, ${pill._o || 0}px) scale(${s.toFixed(3)}, ${(2 - s).toFixed(3)})` : `translate(${pill._o || 0}px, ${p}px) scale(${(2 - s).toFixed(3)}, ${s.toFixed(3)})`; };
      pillY = new Spring(0, draw); pillS = new Spring(1, draw);
      pillFor = null; animate = false;
    }
    root.classList.add('m3pill');
    if (!btn) { pill.style.opacity = '0'; return; }
    pill.style.opacity = '1';
    if (drag) return;
    const t = pillTarget(btn);
    Object.assign(pill.style, { width: t.w + 'px', height: t.h + 'px' }); pill._o = h ? t.y : t.x;
    if (pillFor === btn.dataset.tab && animate !== 'force') { pillY.onChange(pillY.v); return; }
    const moving = pillFor !== null && animate !== false;
    pillFor = btn.dataset.tab;
    const to = along(t, h);
    if (moving) {
      const dist = Math.abs(to - pillY.v);
      pillS.to(1, ...FAST); pillS.kick(dist > 4 ? Math.min(5, dist / 25) : 0);   // stretches along its way and settles round
      pillY.to(to, ...MOVE);
    } else pillY.snap(to);
  }
  if (nav) {
    new MutationObserver(() => requestAnimationFrame(() => syncPill(true))).observe(nav, { subtree: true, childList: true, attributes: true, attributeFilter: ['class'] });
    addEventListener('resize', () => syncPill(false));
    // drag the marker along the places; letting go picks the nearest one
    nav.addEventListener('pointerdown', (e) => {
      if (!navOn() || !on() || e.button !== 0) return;
      const tabs = nav.querySelector('.tabs'); if (!tabs.contains(e.target)) return;
      const btns = [...tabs.querySelectorAll('button[data-tab]')], h = horiz(tabs);
      const p0 = h ? e.clientX : e.clientY, start = pillY ? pillY.v : 0;
      let moved = false;
      const mv = (ev) => {
        const d = (h ? ev.clientX : ev.clientY) - p0;
        if (!moved && Math.abs(d) < 6) return;
        moved = true; drag = true;
        const first = along(pillTarget(btns[0]), h), last = along(pillTarget(btns[btns.length - 1]), h);
        let p = start + d;
        if (p < first) p = first - Math.sqrt(first - p) * 3; else if (p > last) p = last + Math.sqrt(p - last) * 3;   // past the ends it gets heavy
        pillY.to(p, 1, 1600); pillS.to(1.14, ...FAST);
      };
      const end = (ev) => {
        removeEventListener('pointermove', mv); removeEventListener('pointerup', end); removeEventListener('pointercancel', end);
        if (!moved) return;
        drag = null; pillS.to(1, ...FAST);
        const p = pillY.v; let best = btns[0], bd = Infinity;
        for (const b of btns) { const dd = Math.abs(along(pillTarget(b), h) - p); if (dd < bd) { bd = dd; best = b; } }
        // pick the place first, then swallow the click the browser sends after this pointerup (it would land on the place you started from)
        if (best.classList.contains('on')) syncPill('force'); else if (typeof setTab === 'function') setTab(best.dataset.tab); else best.click();
        const swallow = (c) => { c.stopPropagation(); c.preventDefault(); }; addEventListener('click', swallow, { capture: true, once: true }); setTimeout(() => removeEventListener('click', swallow, true), 60);
        if (ev) ev.preventDefault();
      };
      addEventListener('pointermove', mv); addEventListener('pointerup', end); addEventListener('pointercancel', end);
    });
    // every icon has its own little move when you pick its place
    const TAPS = {
      sessions: [{ transform: 'rotate(0)' }, { transform: 'rotate(-12deg)', offset: .2 }, { transform: 'rotate(10deg)', offset: .45 }, { transform: 'rotate(-5deg)', offset: .7 }, { transform: 'rotate(0)' }],
      dashboard: [{ transform: 'scale(1)' }, { transform: 'scale(.72)', offset: .18 }, { transform: 'scale(1.16)', offset: .48 }, { transform: 'scale(.95)', offset: .72 }, { transform: 'scale(1)' }],
      history: [{ transform: 'rotate(0)' }, { transform: 'rotate(-380deg)', offset: .7 }, { transform: 'rotate(-355deg)', offset: .86 }, { transform: 'rotate(-360deg)' }],
      settings: [{ transform: 'rotate(0)' }, { transform: 'rotate(140deg)', offset: .6 }, { transform: 'rotate(112deg)', offset: .82 }, { transform: 'rotate(120deg)' }],
    };
    nav.addEventListener('click', (e) => {
      if (!navOn() || !on()) return;
      const b = e.target.closest('button[data-tab]'); if (!b) return;
      const svg = b.querySelector('.ic svg'), k = TAPS[b.dataset.tab]; if (!svg || !k) return;
      svg.animate(k, { duration: b.dataset.tab === 'history' ? 760 : b.dataset.tab === 'sessions' ? 460 : 560, easing: 'cubic-bezier(.2,0,0,1)' });
    });
  }

  // ── where the dock sits: on Chats at the foot of the chat list (out of the chat's way), elsewhere centered over the page ──
  function placeDock() {
    const side = root.dataset.style === 'm3' && document.querySelector('#content.flush .split > .side');
    const r = side && side.getBoundingClientRect();
    const fits = r && r.width >= 236;
    root.classList.toggle('m3side', !!fits);
    if (fits) root.style.setProperty('--dockx', Math.round(r.left + r.width / 2) + 'px'); else root.style.removeProperty('--dockx');
    requestAnimationFrame(() => syncPill(false));
  }
  if (document.getElementById('content')) {
    new MutationObserver(placeDock).observe(document.getElementById('content'), { attributes: true, attributeFilter: ['class'], childList: true });
    addEventListener('resize', placeDock);
    new MutationObserver(placeDock).observe(root, { attributes: true, attributeFilter: ['data-style'] });
  }

  // ── pages: the new screen travels in on a spring and its cards arrive one after another ──
  const content = document.getElementById('content');
  // a damped spring (damping 0.72, stiffness 320) sampled into a CSS linear() curve: it runs on the compositor, not in JS
  const SPRING_EASE = (() => {
    const pts = [], z = 0.72, w = Math.sqrt(320), wd = w * Math.sqrt(1 - z * z), T = 0.9;
    for (let i = 0; i <= 40; i++) { const tt = (i / 40) * T; pts.push((1 - Math.exp(-z * w * tt) * (Math.cos(wd * tt) + (z * w / wd) * Math.sin(wd * tt))).toFixed(4)); }
    const css = 'linear(' + pts.join(', ') + ')';
    return (window.CSS && CSS.supports && CSS.supports('transition-timing-function', css)) ? css : 'cubic-bezier(.34, 1.4, .64, 1)';
  })();
  function springIn(el, delay, from) {
    el.animate([{ opacity: 0 }, { opacity: 1 }], { duration: 220, delay, easing: 'ease-out', fill: 'backwards' });
    el.animate([{ transform: `translate(${from.x}px, ${from.y}px) scale(${1 - from.s})` }, { transform: 'none' }], { duration: 900, delay, easing: SPRING_EASE, fill: 'backwards' });
  }

  if (content) {
    new MutationObserver(() => {
      if (!on() || !/\bentering\b/.test(content.className)) return;
      const back = /\bback\b/.test(content.className);
      const page = content.firstElementChild; if (!page) return;
      springIn(page, 0, { x: back ? -40 : 40, y: 0, s: 0.02 });
      const items = [...page.querySelectorAll('.card, .rcard, .tablecard, .hitem, .side .item')].slice(0, 10);   // the first few only: no measuring, cheap on slow machines
      items.forEach((x, i) => springIn(x, 40 + i * 38, { x: 0, y: 26, s: 0.05 }));
    }).observe(content, { attributes: true, attributeFilter: ['class'] });
  }

  // ── a message you send rises out of the box you typed it in ──
  let users = 0, box = null;
  const userMsgs = document.getElementsByClassName('msg user');   // live list: reading its length is free
  new MutationObserver(() => {
    if (!on() || userMsgs.length === users) { users = userMsgs.length; return; }
    const all = userMsgs;
    const b = all.length ? all[all.length - 1].parentElement : null;
    if (b !== box) { box = b; users = all.length; return; }   // another chat (or a rebuilt list): nothing new was sent
    if (all.length > users) { const m = all[all.length - 1]; springIn(m, 0, { x: 0, y: 40, s: 0.08 }); }
    users = all.length;
  }).observe(document.getElementById('content') || document.body, { childList: true, subtree: true });
})();
