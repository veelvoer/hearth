'use strict';
/* The small, unnecessary, fun extras of the beta page (switched on in Settings > Beta features). */
const toyOn = (id) => !!(S.settings && S.settings.betaUnlocked && (S.settings.betaOn || {})[id]);
const TOY_PEP = ['You are doing great. The tests are the ones that are wrong.', 'Small steps still compile.', 'Today’s forecast: 100% chance of green checkmarks.', 'Every bug you fix makes a future you smile.',
  'Take a sip of water. The code will wait.', 'Ship it. Or at least commit it.', 'You asked a computer to write code for you. That is already magic.', 'Even the best programmers search for the answer sometimes.',
  'Breathe. It is only a semicolon.', 'Your future self says thank you for those comments.', 'Plan, build, learn, repeat. You are on step two.', 'A short walk fixes at least one bug.'];
const TOY_HELLO = ['Hallo', 'Hola', 'Bonjour', 'Ciao', 'Olá', 'Hej', 'Salut', 'Moi', 'Ahoj', 'Namaste', 'Konnichiwa', 'Merhaba', 'Hei', 'Servus', 'Aloha', 'Shalom'];
const TOY_EMBER = ['Warm in here, isn’t it?', 'I only eat good ideas.', 'Poke me again. I dare you.', 'Fire safety tip: save your files.', 'I’m not hot, I’m just glowing.', 'Hearth, sweet hearth.', 'Did somebody say s’mores?', 'Crackle crackle.'];
const toyDaily = (list) => list[Math.floor(Date.now() / 864e5) % list.length];

let toyCtx = null;
function toySound(kind) {
  try {
    toyCtx = toyCtx || new (window.AudioContext || window.webkitAudioContext)();
    const o = toyCtx.createOscillator(), g = toyCtx.createGain(), t = toyCtx.currentTime, notes = kind === 'done' ? [660, 880] : [520];
    o.type = 'sine'; o.connect(g); g.connect(toyCtx.destination); g.gain.setValueAtTime(0.0001, t);
    notes.forEach((f, i) => { o.frequency.setValueAtTime(f, t + i * 0.11); g.gain.exponentialRampToValueAtTime(0.12, t + i * 0.11 + 0.02); g.gain.exponentialRampToValueAtTime(0.0001, t + i * 0.11 + 0.16); });
    o.start(t); o.stop(t + notes.length * 0.11 + 0.2);
  } catch { /* no audio */ }
}

function toyConfetti() {
  if (matchMedia('(prefers-reduced-motion: reduce)').matches) return;
  const cv = h('canvas', { style: 'position:fixed;inset:0;width:100%;height:100%;pointer-events:none;z-index:200', 'aria-hidden': 'true' });
  cv.width = innerWidth; cv.height = innerHeight; document.body.append(cv);
  const cx = cv.getContext('2d'), cols = ['#F0643C', '#D9A441', '#6A9BCC', '#4CAF7D', '#B04D2A'];
  const ps = Array.from({ length: 120 }, () => ({ x: Math.random() * cv.width, y: -20 - Math.random() * cv.height * .3, vx: (Math.random() - .5) * 4, vy: 2 + Math.random() * 4, a: Math.random() * 6.3, va: (Math.random() - .5) * .3, c: cols[Math.floor(Math.random() * cols.length)] }));
  const t0 = performance.now();
  const frame = (now) => {
    const t = (now - t0) / 2600; cx.clearRect(0, 0, cv.width, cv.height);
    for (const p of ps) { p.x += p.vx; p.y += p.vy; p.vy += 0.06; p.a += p.va; cx.save(); cx.globalAlpha = Math.max(0, 1 - t); cx.translate(p.x, p.y); cx.rotate(p.a); cx.fillStyle = p.c; cx.fillRect(-5, -3, 10, 6); cx.restore(); }
    if (t < 1) requestAnimationFrame(frame); else cv.remove();
  };
  requestAnimationFrame(frame);
}
/** Claude finished a chat you are looking at. */
function toyDone() { if (toyOn('confetti')) toyConfetti(); if (toyOn('sounds')) toySound('done'); }

/** Ember, the little flame of Hearth: poke it and it hops and says something. */
function toyEmber() {
  const say = h('div', {}, toyDaily(TOY_EMBER)), flame = h('button', { class: 'ember', 'aria-label': 'Poke Ember', html: sparkSVG(56) });
  flame.onclick = () => {
    flame.classList.remove('hop'); void flame.offsetWidth; flame.classList.add('hop'); say.textContent = TOY_EMBER[Math.floor(Math.random() * TOY_EMBER.length)];
    for (let i = 0; i < 8; i++) { const sp = h('i', { class: 'spark' }); sp.style.setProperty('--a', (i / 8 * 360 + Math.random() * 30) + 'deg'); flame.append(sp); setTimeout(() => sp.remove(), 700); }
  };
  return h('div', { class: 'card row', style: 'align-items:center;gap:16px' }, flame, h('div', { class: 'grow' }, h('div', { class: 'label' }, 'Ember'), say));
}

/* ── Hidden things ── */
/** A greeting for a special day, or null. */
function toyHoliday() {
  const d = new Date(), k = (d.getMonth() + 1) + '-' + d.getDate();
  return ({ '12-24': 'Merry Christmas!', '12-25': 'Merry Christmas!', '12-26': 'Merry Christmas!', '12-31': 'Happy New Year!', '1-1': 'Happy New Year!', '10-31': 'Boo! Happy Halloween', '11-5': 'Happy Bonfire Night',
    '2-14': 'Happy Valentine’s Day', '4-1': 'Nothing is broken today. Probably.', '5-4': 'May the fourth be with you' })[k] || null;
}
/** The first time you look on a special day, the confetti goes off once. */
function toyHolidayOnce() { const day = new Date().toDateString(); if (toyHoliday() && localStorage.getItem('cm.holidayShown') !== day) { localStorage.setItem('cm.holidayShown', day); setTimeout(toyConfetti, 400); } }
/** Things you can type in a chat instead of a message. true = it was one, and nothing is sent. */
function toyEgg(text) {
  const t = String(text || '').trim().toLowerCase(), say = {
    '/confetti': 'Party time!', '/party': 'Party time!', '/ember': TOY_EMBER[Math.floor(Math.random() * TOY_EMBER.length)], '/flame': TOY_EMBER[Math.floor(Math.random() * TOY_EMBER.length)],
    '/coffee': 'Brewing… this is not a real feature, but you deserve one.', '/hello': TOY_HELLO[Math.floor(Math.random() * TOY_HELLO.length)] + '!', '/42': 'The answer. Now, what was the question?', '/sudo': 'Nice try.' }[t];
  if (!say) return false;
  if (t === '/confetti' || t === '/party') toyConfetti();
  toast(say); return true;
}
// the old code: up up down down left right left right B A
(() => {
  const code = ['ArrowUp', 'ArrowUp', 'ArrowDown', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'ArrowLeft', 'ArrowRight', 'b', 'a']; let i = 0;
  document.addEventListener('keydown', (e) => { const k = e.key.length === 1 ? e.key.toLowerCase() : e.key; if (k === code[i]) { i++; if (i === code.length) { i = 0; toyConfetti(); toast('Ember approves.'); } } else i = k === code[0] ? 1 : 0; });
  let taps = 0, last = 0;   // the logo in the top bar, five quick clicks
  document.addEventListener('click', (e) => { if (!e.target.closest || !e.target.closest('#nav .brand')) return; const n = Date.now(); taps = n - last > 1500 ? 1 : taps + 1; last = n; if (taps >= 5) { taps = 0; toyConfetti(); toast(TOY_EMBER[Math.floor(Math.random() * TOY_EMBER.length)]); } });
})();

const TOY_VERBS = ['Pondering', 'Flibbertigibbeting', 'Percolating', 'Simmering', 'Noodling', 'Conjuring', 'Marinating', 'Reticulating', 'Cogitating', 'Tinkering', 'Wibbling', 'Spelunking', 'Moonwalking', 'Brewing', 'Doodling', 'Fermenting',
  'Herding bits', 'Jiving', 'Kneading', 'Musing', 'Puzzling', 'Ruminating', 'Shimmying', 'Sprouting', 'Stewing', 'Transmuting', 'Unfurling', 'Vibing', 'Whirring', 'Zigzagging', 'Warming the hearth', 'Poking the embers', 'Toasting marshmallows', 'Stoking the fire', 'Counting sparks', 'Sweeping the chimney'];
/** "Claude is working": a flickering flame and, with the beta words on, a playful word that changes every few seconds. */
function workingEl() {
  const playful = toyOn('verbs'), word = h('span', { class: 'wword' }, playful ? TOY_VERBS[Math.floor(Math.random() * TOY_VERBS.length)] + '…' : 'Claude is working…');
  const el = h('div', { class: 'working' }, h('span', { class: 'wflame', html: sparkSVG(20) }), word);
  if (playful) { const t = setInterval(() => { if (!el.isConnected) return clearInterval(t); word.classList.add('out'); setTimeout(() => { word.textContent = TOY_VERBS[Math.floor(Math.random() * TOY_VERBS.length)] + '…'; word.classList.remove('out'); }, 260); }, 2800); }
  return el;
}

/* ── Keyboard shortcuts: press ? (outside a text field) to see them ── */
function showShortcuts() {
  if (document.querySelector('.shortcuts')) return;
  const rows = [['Ctrl/⌘ K', 'Find a chat'], ['Ctrl/⌘ N', 'New chat'], ['Ctrl/⌘ 1 – 4', 'Chats, Home, History, Settings'], ['Enter', 'Send a message'], ['Shift + Enter', 'New line'], ['/', 'Commands in a message'], ['Esc', 'Close a window'], ['?', 'This list']];
  const over = h('div', { class: 'overlay shortcuts', onclick: (e) => { if (e.target === over) over.remove(); } },
    h('div', { class: 'card', role: 'dialog', 'aria-label': 'Keyboard shortcuts', style: 'width:min(420px,92vw);display:flex;flex-direction:column;gap:6px' },
      h('h3', { style: 'margin:0 0 6px' }, 'Keyboard shortcuts'), ...rows.map(([k, t]) => h('div', { class: 'row', style: 'min-height:34px' }, h('kbd', { class: 'kbd' }, k), h('div', { class: 'grow', style: 'margin-left:14px' }, t))),
      h('div', { style: 'text-align:right;margin-top:6px' }, h('button', { class: 'ghost', onclick: () => over.remove() }, 'Close'))));
  document.body.append(over);
  const esc = (e) => { if (e.key === 'Escape') { over.remove(); document.removeEventListener('keydown', esc); } }; document.addEventListener('keydown', esc);
}
document.addEventListener('keydown', (e) => { const t = e.target, typing = t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable); if (e.key === '?' && !typing && !e.ctrlKey && !e.metaKey) { e.preventDefault(); showShortcuts(); } });
