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
const M3_LOAD = '<svg viewBox="0 0 48 48" width="28" height="28"><path fill="currentColor" d="M24.00 6.00 L25.25 4.93 L26.59 4.32 L27.89 4.42 L29.02 5.25 L29.91 6.59 L30.60 8.08 L31.23 9.35 L32.00 10.14 L33.08 10.42 L34.49 10.33 L36.13 10.17 L37.73 10.27 L39.01 10.84 L39.75 11.92 L39.89 13.38 L39.59 15.00 L39.15 16.53 L38.92 17.82 L39.19 18.84 L40.02 19.71 L41.27 20.56 L42.60 21.55 L43.62 22.71 L44.00 24.00 L43.62 25.29 L42.60 26.45 L41.27 27.44 L40.02 28.29 L39.19 29.16 L38.92 30.18 L39.15 31.47 L39.59 33.00 L39.89 34.62 L39.75 36.08 L39.01 37.16 L37.73 37.73 L36.13 37.83 L34.49 37.67 L33.08 37.58 L32.00 37.86 L31.23 38.65 L30.60 39.92 L29.91 41.41 L29.02 42.75 L27.89 43.58 L26.59 43.68 L25.25 43.07 L24.00 42.00 L22.90 40.85 L21.89 40.01 L20.87 39.73 L19.71 40.02 L18.34 40.68 L16.82 41.34 L15.30 41.64 L14.00 41.32 L13.08 40.35 L12.58 38.89 L12.39 37.24 L12.27 35.73 L11.94 34.57 L11.19 33.83 L9.96 33.38 L8.41 33.00 L6.86 32.45 L5.66 31.60 L5.10 30.42 L5.25 29.02 L5.96 27.59 L6.91 26.25 L7.70 25.07 L8.00 24.00 L7.70 22.93 L6.91 21.75 L5.96 20.41 L5.25 18.98 L5.10 17.58 L5.66 16.40 L6.86 15.55 L8.41 15.00 L9.96 14.62 L11.19 14.17 L11.94 13.43 L12.27 12.27 L12.39 10.76 L12.58 9.11 L13.08 7.65 L14.00 6.68 L15.30 6.36 L16.82 6.66 L18.34 7.32 L19.71 7.98 L20.87 8.27 L21.89 7.99 L22.90 7.15Z"><animate attributeName="d" dur="4.8s" repeatCount="indefinite" calcMode="spline" keyTimes="0;.2;.4;.6;.8;1" keySplines=".4 0 .2 1;.4 0 .2 1;.4 0 .2 1;.4 0 .2 1;.4 0 .2 1" values="M24.00 6.00 L25.25 4.93 L26.59 4.32 L27.89 4.42 L29.02 5.25 L29.91 6.59 L30.60 8.08 L31.23 9.35 L32.00 10.14 L33.08 10.42 L34.49 10.33 L36.13 10.17 L37.73 10.27 L39.01 10.84 L39.75 11.92 L39.89 13.38 L39.59 15.00 L39.15 16.53 L38.92 17.82 L39.19 18.84 L40.02 19.71 L41.27 20.56 L42.60 21.55 L43.62 22.71 L44.00 24.00 L43.62 25.29 L42.60 26.45 L41.27 27.44 L40.02 28.29 L39.19 29.16 L38.92 30.18 L39.15 31.47 L39.59 33.00 L39.89 34.62 L39.75 36.08 L39.01 37.16 L37.73 37.73 L36.13 37.83 L34.49 37.67 L33.08 37.58 L32.00 37.86 L31.23 38.65 L30.60 39.92 L29.91 41.41 L29.02 42.75 L27.89 43.58 L26.59 43.68 L25.25 43.07 L24.00 42.00 L22.90 40.85 L21.89 40.01 L20.87 39.73 L19.71 40.02 L18.34 40.68 L16.82 41.34 L15.30 41.64 L14.00 41.32 L13.08 40.35 L12.58 38.89 L12.39 37.24 L12.27 35.73 L11.94 34.57 L11.19 33.83 L9.96 33.38 L8.41 33.00 L6.86 32.45 L5.66 31.60 L5.10 30.42 L5.25 29.02 L5.96 27.59 L6.91 26.25 L7.70 25.07 L8.00 24.00 L7.70 22.93 L6.91 21.75 L5.96 20.41 L5.25 18.98 L5.10 17.58 L5.66 16.40 L6.86 15.55 L8.41 15.00 L9.96 14.62 L11.19 14.17 L11.94 13.43 L12.27 12.27 L12.39 10.76 L12.58 9.11 L13.08 7.65 L14.00 6.68 L15.30 6.36 L16.82 6.66 L18.34 7.32 L19.71 7.98 L20.87 8.27 L21.89 7.99 L22.90 7.15Z;M24.00 7.60 L25.14 6.66 L26.36 6.09 L27.64 5.71 L28.95 5.52 L30.27 5.53 L31.57 5.73 L32.82 6.11 L34.00 6.68 L35.08 7.42 L36.04 8.31 L36.86 9.33 L37.53 10.47 L38.02 11.70 L38.34 13.00 L38.45 14.35 L38.20 15.80 L39.58 16.32 L40.69 17.09 L41.66 18.01 L42.48 19.05 L43.13 20.19 L43.61 21.42 L43.90 22.70 L44.00 24.00 L43.90 25.30 L43.61 26.58 L43.13 27.81 L42.48 28.95 L41.66 29.99 L40.69 30.91 L39.58 31.68 L38.20 32.20 L38.45 33.65 L38.34 35.00 L38.02 36.30 L37.53 37.53 L36.86 38.67 L36.04 39.69 L35.08 40.58 L34.00 41.32 L32.82 41.89 L31.57 42.27 L30.27 42.47 L28.95 42.48 L27.64 42.29 L26.36 41.91 L25.14 41.34 L24.00 40.40 L22.86 41.34 L21.64 41.91 L20.36 42.29 L19.05 42.48 L17.73 42.47 L16.43 42.27 L15.18 41.89 L14.00 41.32 L12.92 40.58 L11.96 39.69 L11.14 38.67 L10.47 37.53 L9.98 36.30 L9.66 35.00 L9.55 33.65 L9.80 32.20 L8.42 31.68 L7.31 30.91 L6.34 29.99 L5.52 28.95 L4.87 27.81 L4.39 26.58 L4.10 25.30 L4.00 24.00 L4.10 22.70 L4.39 21.42 L4.87 20.19 L5.52 19.05 L6.34 18.01 L7.31 17.09 L8.42 16.32 L9.80 15.80 L9.55 14.35 L9.66 13.00 L9.98 11.70 L10.47 10.47 L11.14 9.33 L11.96 8.31 L12.92 7.42 L14.00 6.68 L15.18 6.11 L16.43 5.73 L17.73 5.53 L19.05 5.52 L20.36 5.71 L21.64 6.09 L22.86 6.66Z;M24.00 4.00 L25.30 4.14 L26.56 4.54 L27.74 5.19 L28.81 6.03 L29.76 7.03 L30.58 8.11 L31.29 9.22 L31.90 10.32 L32.46 11.34 L32.99 12.28 L33.56 13.10 L34.18 13.82 L34.90 14.44 L35.72 15.01 L36.66 15.54 L37.68 16.10 L38.78 16.71 L39.89 17.42 L40.97 18.24 L41.97 19.19 L42.81 20.26 L43.46 21.44 L43.86 22.70 L44.00 24.00 L43.86 25.30 L43.46 26.56 L42.81 27.74 L41.97 28.81 L40.97 29.76 L39.89 30.58 L38.78 31.29 L37.68 31.90 L36.66 32.46 L35.72 32.99 L34.90 33.56 L34.18 34.18 L33.56 34.90 L32.99 35.72 L32.46 36.66 L31.90 37.68 L31.29 38.78 L30.58 39.89 L29.76 40.97 L28.81 41.97 L27.74 42.81 L26.56 43.46 L25.30 43.86 L24.00 44.00 L22.70 43.86 L21.44 43.46 L20.26 42.81 L19.19 41.97 L18.24 40.97 L17.42 39.89 L16.71 38.78 L16.10 37.68 L15.54 36.66 L15.01 35.72 L14.44 34.90 L13.82 34.18 L13.10 33.56 L12.28 32.99 L11.34 32.46 L10.32 31.90 L9.22 31.29 L8.11 30.58 L7.03 29.76 L6.03 28.81 L5.19 27.74 L4.54 26.56 L4.14 25.30 L4.00 24.00 L4.14 22.70 L4.54 21.44 L5.19 20.26 L6.03 19.19 L7.03 18.24 L8.11 17.42 L9.22 16.71 L10.32 16.10 L11.34 15.54 L12.28 15.01 L13.10 14.44 L13.82 13.82 L14.44 13.10 L15.01 12.28 L15.54 11.34 L16.10 10.32 L16.71 9.22 L17.42 8.11 L18.24 7.03 L19.19 6.03 L20.26 5.19 L21.44 4.54 L22.70 4.14Z;M24.00 4.00 L25.25 4.98 L26.19 7.34 L26.84 9.74 L27.52 10.86 L28.67 10.23 L30.43 8.48 L32.43 6.90 L34.00 6.68 L34.59 8.15 L34.23 10.67 L33.59 13.07 L33.62 14.38 L34.93 14.41 L37.33 13.77 L39.85 13.41 L41.32 14.00 L41.10 15.57 L39.52 17.57 L37.77 19.33 L37.14 20.48 L38.26 21.16 L40.66 21.81 L43.02 22.75 L44.00 24.00 L43.02 25.25 L40.66 26.19 L38.26 26.84 L37.14 27.52 L37.77 28.67 L39.52 30.43 L41.10 32.43 L41.32 34.00 L39.85 34.59 L37.33 34.23 L34.93 33.59 L33.62 33.62 L33.59 34.93 L34.23 37.33 L34.59 39.85 L34.00 41.32 L32.43 41.10 L30.43 39.52 L28.67 37.77 L27.52 37.14 L26.84 38.26 L26.19 40.66 L25.25 43.02 L24.00 44.00 L22.75 43.02 L21.81 40.66 L21.16 38.26 L20.48 37.14 L19.33 37.77 L17.57 39.52 L15.57 41.10 L14.00 41.32 L13.41 39.85 L13.77 37.33 L14.41 34.93 L14.38 33.62 L13.07 33.59 L10.67 34.23 L8.15 34.59 L6.68 34.00 L6.90 32.43 L8.48 30.43 L10.23 28.67 L10.86 27.52 L9.74 26.84 L7.34 26.19 L4.98 25.25 L4.00 24.00 L4.98 22.75 L7.34 21.81 L9.74 21.16 L10.86 20.48 L10.23 19.33 L8.48 17.57 L6.90 15.57 L6.68 14.00 L8.15 13.41 L10.67 13.77 L13.07 14.41 L14.38 14.38 L14.41 13.07 L13.77 10.67 L13.41 8.15 L14.00 6.68 L15.57 6.90 L17.57 8.48 L19.33 10.23 L20.48 10.86 L21.16 9.74 L21.81 7.34 L22.75 4.98Z;M24.00 8.00 L25.05 8.00 L26.11 8.00 L27.18 8.01 L28.28 8.02 L29.41 8.05 L30.58 8.12 L31.78 8.23 L33.00 8.42 L34.22 8.71 L35.40 9.15 L36.49 9.75 L37.45 10.55 L38.25 11.51 L38.85 12.60 L39.29 13.78 L39.58 15.00 L39.77 16.22 L39.88 17.42 L39.95 18.59 L39.98 19.72 L39.99 20.82 L40.00 21.89 L40.00 22.95 L40.00 24.00 L40.00 25.05 L40.00 26.11 L39.99 27.18 L39.98 28.28 L39.95 29.41 L39.88 30.58 L39.77 31.78 L39.58 33.00 L39.29 34.22 L38.85 35.40 L38.25 36.49 L37.45 37.45 L36.49 38.25 L35.40 38.85 L34.22 39.29 L33.00 39.58 L31.78 39.77 L30.58 39.88 L29.41 39.95 L28.28 39.98 L27.18 39.99 L26.11 40.00 L25.05 40.00 L24.00 40.00 L22.95 40.00 L21.89 40.00 L20.82 39.99 L19.72 39.98 L18.59 39.95 L17.42 39.88 L16.22 39.77 L15.00 39.58 L13.78 39.29 L12.60 38.85 L11.51 38.25 L10.55 37.45 L9.75 36.49 L9.15 35.40 L8.71 34.22 L8.42 33.00 L8.23 31.78 L8.12 30.58 L8.05 29.41 L8.02 28.28 L8.01 27.18 L8.00 26.11 L8.00 25.05 L8.00 24.00 L8.00 22.95 L8.00 21.89 L8.01 20.82 L8.02 19.72 L8.05 18.59 L8.12 17.42 L8.23 16.22 L8.42 15.00 L8.71 13.78 L9.15 12.60 L9.75 11.51 L10.55 10.55 L11.51 9.75 L12.60 9.15 L13.78 8.71 L15.00 8.42 L16.22 8.23 L17.42 8.12 L18.59 8.05 L19.72 8.02 L20.82 8.01 L21.89 8.00 L22.95 8.00Z;M24.00 6.00 L25.25 4.93 L26.59 4.32 L27.89 4.42 L29.02 5.25 L29.91 6.59 L30.60 8.08 L31.23 9.35 L32.00 10.14 L33.08 10.42 L34.49 10.33 L36.13 10.17 L37.73 10.27 L39.01 10.84 L39.75 11.92 L39.89 13.38 L39.59 15.00 L39.15 16.53 L38.92 17.82 L39.19 18.84 L40.02 19.71 L41.27 20.56 L42.60 21.55 L43.62 22.71 L44.00 24.00 L43.62 25.29 L42.60 26.45 L41.27 27.44 L40.02 28.29 L39.19 29.16 L38.92 30.18 L39.15 31.47 L39.59 33.00 L39.89 34.62 L39.75 36.08 L39.01 37.16 L37.73 37.73 L36.13 37.83 L34.49 37.67 L33.08 37.58 L32.00 37.86 L31.23 38.65 L30.60 39.92 L29.91 41.41 L29.02 42.75 L27.89 43.58 L26.59 43.68 L25.25 43.07 L24.00 42.00 L22.90 40.85 L21.89 40.01 L20.87 39.73 L19.71 40.02 L18.34 40.68 L16.82 41.34 L15.30 41.64 L14.00 41.32 L13.08 40.35 L12.58 38.89 L12.39 37.24 L12.27 35.73 L11.94 34.57 L11.19 33.83 L9.96 33.38 L8.41 33.00 L6.86 32.45 L5.66 31.60 L5.10 30.42 L5.25 29.02 L5.96 27.59 L6.91 26.25 L7.70 25.07 L8.00 24.00 L7.70 22.93 L6.91 21.75 L5.96 20.41 L5.25 18.98 L5.10 17.58 L5.66 16.40 L6.86 15.55 L8.41 15.00 L9.96 14.62 L11.19 14.17 L11.94 13.43 L12.27 12.27 L12.39 10.76 L12.58 9.11 L13.08 7.65 L14.00 6.68 L15.30 6.36 L16.82 6.66 L18.34 7.32 L19.71 7.98 L20.87 8.27 L21.89 7.99 L22.90 7.15Z"/></path></svg>';
/** "Claude is working": a flickering flame and, with the beta words on, a playful word that changes every few seconds. */
function workingEl() {
  const playful = toyOn('verbs'), word = h('span', { class: 'wword' }, playful ? TOY_VERBS[Math.floor(Math.random() * TOY_VERBS.length)] + '…' : 'Claude is working…');
  // Material 3: the expressive loading indicator, one shape that morphs (cookie, flower, burst, pill) while it turns
  const el = h('div', { class: 'working' }, h('span', { class: 'wflame', html: sparkSVG(20) }), h('span', { class: 'm3load', html: M3_LOAD }), word);
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
