'use strict';
/* Small usage helpers shared by the screens. */
(function () {
  const SESSION_MS = 5 * 3600e3, WEEK_MS = 7 * 86400e3;

  const dur = (ms) => {
    const m = Math.max(0, Math.floor(ms / 60000));
    return m < 60 ? m + 'm' : m < 1440 ? Math.floor(m / 60) + 'h ' + (m % 60) + 'm' : Math.floor(m / 1440) + 'd ' + Math.floor((m % 1440) / 60) + 'h';
  };
  const usedAt = (l, t) => !l ? null : (l.resetsAt && l.resetsAt < t) ? 0 : l.pct;

  function pace(l, win, t) {
    if (!l || !l.resetsAt || l.resetsAt < t) return null;
    const start = l.resetsAt - win;
    const el = Math.min(win, Math.max(1, t - start));
    const frac = el / win;
    if (frac < 0.04 || l.pct <= 0) return { elapsed: frac, projected: l.pct, hitsAt: null };
    const proj = l.pct / frac;
    return { elapsed: frac, projected: proj, hitsAt: proj > 100 ? start + el * 100 / l.pct : null };
  }
  function paceWord(l, p) {
    if (!l || !p) return null;
    const d = l.pct - p.elapsed * 100;
    return d > 5 ? 'Ahead of pace' : d < -5 ? 'Under pace' : 'On pace';
  }

  window.CMW = { dur, usedAt, pace, paceWord, SESSION_MS, WEEK_MS };
})();
