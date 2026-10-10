'use strict';
/* The guided tour: dims the app, lights up one thing at a time and explains it. Smooth moves between steps.
   Start it with startTour(). Esc or "Skip" ends it. */
const TOUR = { el: null, spot: null, tip: null, i: 0, steps: [], timer: null };

const byLabel = (text) => [...document.querySelectorAll('.card')].find((c) => { const l = c.querySelector('.label'); return l && l.textContent.trim().toLowerCase() === text.toLowerCase(); });
const q = (sel) => () => document.querySelector(sel);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function tourSteps() {
  const chatTab = async () => { if (tab !== 'sessions') setTab('sessions'); await sleep(250); };
  const settingsTab = async () => { if (tab !== 'settings') setTab('settings'); await sleep(300); };
  return [
    { title: 'This is Hearth', text: 'A quick look around. I will point at things and say what they do. Press Next, or the right arrow key.', target: null },
    { title: 'The menu', text: 'Four places: Chats (talk to Claude), Dashboard (how much of your limit is left), History, and Settings.', target: q('#nav'), place: 'right' },
    { title: 'Your usage', text: 'These rings show how much of your Claude limit you used in the last 5 hours and this week, so you are never surprised.', prep: async () => { if (tab !== 'dashboard') setTab('dashboard'); await sleep(350); }, target: () => document.querySelector('.page .card') },
    { title: 'Start a chat', text: 'Press this to start a new chat. Pick a project folder, say what you want, and Claude gets to work.', prep: chatTab, target: q('.side > button.primary'), place: 'right', hint: 'click' },
    { title: 'All your chats, in one place', text: 'Chats are grouped by project. The same list shows on your laptop, your server and your phone.', target: q('.side'), place: 'right' },
    { title: 'Write your message', text: 'Type what you want here. Press Enter to send, Shift+Enter for a new line. You can also drop in files or paste a picture.', prep: async () => { await chatTab(); if (typeof newChat === 'function') newChat(); await sleep(350); }, target: q('.cbox'), place: 'top', hint: 'type' },
    { title: 'Commands with a slash', text: 'Type “/” to see commands, skills and shortcuts: change the model, see what changed, add plugins and more.', target: q('#composer-text'), place: 'top', hint: '/' },
    { title: 'How Claude works', text: 'These choose how careful Claude is (“Ask me” asks before every change), which model thinks, and how much effort it puts in.', target: q('.cfoot'), place: 'top' },
    { title: 'Add-ons, voice and options', text: 'Inside a chat you also get Talk (speak with Claude), Add-ons (skills, plugins, connections, saving changes) and Options (phone calls, where it runs).', target: q('.chat-head') || null, place: 'bottom' },
    { title: 'Everything is kept in sync', text: 'This card shows that your files and chats match on every device. “Sync now” checks right away.', prep: settingsTab, target: () => byLabel('Sync'), place: 'left' },
    { title: 'Updates', text: 'Hearth looks on GitHub for new versions and updates itself, your server and (on the phone) the phone app.', target: () => byLabel('Updates'), place: 'left' },
    { title: 'Connect a phone', text: 'To add another phone, open this card. The phone asks, you press Accept.', target: () => byLabel('Connect your phone') || byLabel('Your devices'), place: 'left' },
    { title: 'Look and feel', text: 'Light or dark, and your own colors. The icon of the app changes with your colors too.', target: () => byLabel('Appearance'), place: 'left' },
    { title: 'Need help?', text: 'Found a bug or have an idea? Press Contact support. You can also replay this tour or the setup from the Help card.', target: () => byLabel('Support') || byLabel('Help'), place: 'left' },
    { title: 'That is the tour', text: 'You know the important parts. Have fun building! Replay this any time from Settings → Help.', target: null },
  ];
}

async function startTour() {
  if (TOUR.el) return;
  TOUR.steps = tourSteps(); TOUR.i = 0;
  TOUR.el = h('div', { class: 'tour' });
  TOUR.spot = h('div', { class: 'tspot' }); TOUR.tip = h('div', { class: 'ttip' });
  TOUR.el.append(h('div', { class: 'tdim' }), TOUR.spot, TOUR.tip);
  document.body.append(TOUR.el);
  document.addEventListener('keydown', tourKey, true);
  window.addEventListener('resize', tourPlace);
  await showStep(0);
}
function endTour() {
  document.removeEventListener('keydown', tourKey, true); window.removeEventListener('resize', tourPlace); clearInterval(TOUR.timer);
  if (!TOUR.el) return; const el = TOUR.el; TOUR.el = null; el.classList.add('out'); setTimeout(() => el.remove(), 260);
  cm.setSettings({ tourDone: true }).catch(() => {});
}
function tourKey(e) {
  if (!TOUR.el) return;
  if (e.key === 'Escape') { e.stopPropagation(); endTour(); }
  else if (e.key === 'ArrowRight' || e.key === 'Enter') { e.preventDefault(); e.stopPropagation(); tourGo(1); }
  else if (e.key === 'ArrowLeft') { e.preventDefault(); e.stopPropagation(); tourGo(-1); }
}
async function tourGo(d) { const n = TOUR.i + d; if (n < 0) return; if (n >= TOUR.steps.length) return endTour(); await showStep(n); }

async function showStep(n) {
  TOUR.i = n; const st = TOUR.steps[n];
  TOUR.tip.classList.remove('in');
  if (st.prep) { try { await st.prep(); } catch { /* the step still explains */ } }
  let el = null;
  for (let k = 0; k < 8 && st.target; k++) { el = st.target(); if (el && el.getBoundingClientRect().width) break; el = null; await sleep(120); }
  st.el = el;
  if (el) el.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  await sleep(el ? 180 : 0);
  drawTip(st); tourPlace();
  clearInterval(TOUR.timer); TOUR.timer = setInterval(tourPlace, 400);   // the page may move while we point at it
}
function drawTip(st) {
  const last = TOUR.i === TOUR.steps.length - 1;
  TOUR.tip.innerHTML = '';
  TOUR.tip.append(h('div', { class: 'tcount' }, (TOUR.i + 1) + ' / ' + TOUR.steps.length), h('h3', {}, st.title), h('p', {}, st.text),
    h('div', { class: 'trow' }, h('button', { class: 'link', onclick: endTour }, last ? '' : 'Skip tour'), h('div', { class: 'grow' }),
      TOUR.i > 0 ? h('button', { class: 'ghost sm', onclick: () => tourGo(-1) }, 'Back') : null, h('button', { class: 'primary sm', onclick: () => tourGo(1) }, last ? 'Finish' : 'Next')));
  requestAnimationFrame(() => TOUR.tip.classList.add('in'));
}
/** Moves the light and the explanation next to the thing we are pointing at. */
function tourPlace() {
  if (!TOUR.el) return;
  const st = TOUR.steps[TOUR.i], el = st && st.el, W = innerWidth, H = innerHeight, pad = 8;
  const tip = TOUR.tip, tw = Math.min(340, W - 24), th = tip.offsetHeight || 170;
  tip.style.width = tw + 'px';
  if (!el || !el.isConnected) {   // nothing to point at: the light goes out and the card sits in the middle
    TOUR.spot.style.cssText = `left:${W / 2}px;top:${H / 2}px;width:0;height:0;opacity:0`;
    tip.style.left = (W - tw) / 2 + 'px'; tip.style.top = (H - th) / 2 + 'px'; return;
  }
  const r = el.getBoundingClientRect();
  const x = Math.max(4, r.left - pad), y = Math.max(4, r.top - pad), w = Math.min(W - 8, r.width + pad * 2), hh = Math.min(H - 8, r.height + pad * 2);
  TOUR.spot.style.cssText = `left:${x}px;top:${y}px;width:${w}px;height:${hh}px;opacity:1`;
  TOUR.spot.dataset.hint = st.hint || '';
  const room = { right: W - (x + w) - 16, left: x - 16, bottom: H - (y + hh) - 16, top: y - 16 };
  let place = st.place && room[st.place] >= (st.place === 'left' || st.place === 'right' ? tw : th) ? st.place : null;
  if (!place) place = room.right >= tw ? 'right' : room.left >= tw ? 'left' : room.bottom >= th ? 'bottom' : room.top >= th ? 'top' : 'bottom';
  let tx, ty;
  if (place === 'right') { tx = x + w + 16; ty = y; } else if (place === 'left') { tx = x - tw - 16; ty = y; } else if (place === 'bottom') { tx = x; ty = y + hh + 16; } else { tx = x; ty = y - th - 16; }
  tip.style.left = Math.max(12, Math.min(W - tw - 12, tx)) + 'px'; tip.style.top = Math.max(12, Math.min(H - th - 12, ty)) + 'px';
}
