'use strict';
const q = new URLSearchParams(location.search);
const key = q.get('key'), kind = q.get('kind'), permission = !!q.get('permission');
document.getElementById('spark').innerHTML = sparkSVG(72);
document.getElementById('kind').textContent = kind === 'permission' ? 'Needs permission' : kind === 'question' ? 'Has a question' : 'Finished';
document.getElementById('title').textContent = q.get('title');
document.getElementById('text').textContent = q.get('text');

const act = (a) => cm.callAction(key, a);
const btn = (label, a, cls = 'ghost') => {
  const b = document.createElement('button');
  b.className = cls; b.textContent = label; b.onclick = () => act(a);
  document.getElementById('actions').append(b);
};
if (permission) { btn('Allow', 'allow', 'primary'); btn('Deny', 'deny'); btn('Open chat', 'chat'); }
else { btn('Talk', 'talk', 'primary'); btn('Open chat', 'chat'); btn('Dismiss', 'dismiss'); }

// ring tone: two short tones, repeated
try {
  const ctx = new AudioContext();
  const beep = (t, f) => {
    const o = ctx.createOscillator(), g = ctx.createGain();
    o.frequency.value = f; o.connect(g); g.connect(ctx.destination);
    g.gain.setValueAtTime(0, t); g.gain.linearRampToValueAtTime(0.18, t + 0.02); g.gain.linearRampToValueAtTime(0, t + 0.3);
    o.start(t); o.stop(t + 0.32);
  };
  for (let i = 0; i < 14; i++) { const t = ctx.currentTime + i * 2.4; beep(t, 880); beep(t + 0.4, 660); beep(t + 1.0, 880); beep(t + 1.4, 660); }
} catch { /* silent */ }
