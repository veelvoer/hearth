'use strict';
/* The weekly limit planned over the days of the week.
   You pin some weekdays to a share of the week (say Saturday 20%); the rest of the limit that is still left is shared out evenly over the other days
   that are still to come. What really got used moves the rest: use less on a pinned day and every other day gets a little more.
   Days are calendar days in the user's time zone. The reset moment cuts the first and last day short, so the weekday of the reset has two pieces
   (before the reset next week and after the reset this week); they count as one day with one target. */
const DAY = 86400000;

/** Percent of the week used at time t, from readings [{t, w}] (sorted). Before the first reading in the window nothing was used. */
function usedAt(samples, t, now) {
  let w = 0;
  for (const s of samples) { if (s.t <= Math.min(t, now)) w = s.w; else break; }
  return w;
}

/**
 * @param usage    {week:{pct,resetsAt}, at}
 * @param samples  [{t, w, r}]  readings of the week percentage, oldest first
 * @param targets  {0..6: percent of the week for that weekday}, 0 = Sunday
 * @param now      ms
 * @param tzMin    minutes east of UTC (Belgium in summer: 120)
 */
function plan(usage, samples, targets, now, tzMin) {
  const wk = usage && usage.week;
  if (!wk || wk.resetsAt == null) return { unavailable: 'No weekly reading yet. Use Claude once and it shows up here.' };
  const end = wk.resetsAt, start = end - 7 * DAY;
  if (end <= now) return { unavailable: 'A fresh week starts with your next message.' };
  const off = (Number(tzMin) || 0) * 60000;
  const dayStart = (t) => Math.floor((t + off) / DAY) * DAY - off;
  const wdOf = (t) => new Date(t + off).getUTCDay();
  const pctNow = Math.max(0, Math.min(100, wk.pct));
  const inWin = (samples || []).filter((s) => s.t >= start - 60000 && s.t <= now + 60000 && (s.r == null || Math.abs(s.r - end) < 120000)).sort((a, b) => a.t - b.t);

  // the pieces of days inside the window, oldest first
  const segs = [];
  for (let d = dayStart(start); d < end; d += DAY) {
    const a = Math.max(start, d), b = Math.min(end, d + DAY);
    if (b - a < 60000) continue;
    segs.push({ wd: wdOf(a + 1), start: a, end: b, f: (b - a) / DAY });
  }
  for (const s of segs) {
    s.state = s.end <= now ? 'past' : s.start <= now ? 'today' : 'future';
    s.used = s.state === 'future' ? 0 : Math.max(0, usedAt(inWin, s.end, now) - usedAt(inWin, s.start, now));
    if (s.state === 'today') s.used = Math.max(0, pctNow - usedAt(inWin, s.start, now));
  }
  // use before the first reading is unknown (Hearth was not watching yet): it is not put on any day
  const first = inWin[0];
  const unknown = first && !(samples || []).some((s) => s.t < first.t) && first.w > 3 ? first.w : 0;

  const R = Math.max(0, 100 - pctNow);
  for (const g of segs) g.w = g.state === 'today' ? (g.end - now) / DAY : g.f;   // the weight of a day without a pin: its time still ahead
  const out = groups(segs, targets, R);
  return {
    now, start, end, used: round(pctNow), left: round(R), ...out, unknown: round(unknown), R, targets: cleanTargets(targets),
    segs: segs.map((g) => ({ wd: g.wd, f: g.f, state: g.state, used: g.used, w: g.w })),
  };
}
const cleanTargets = (t) => { const o = {}; for (let wd = 0; wd < 7; wd++) { const v = t && t[wd]; if (v != null && Number.isFinite(Number(v))) o[wd] = Math.max(0, Math.min(100, Number(v))); } return o; };

/** Shares the part of the week that is left (R percent) over the days that are still to come. Pure: the phone and the desktop use it to follow a slider while it moves.
 *  segs: [{wd, f (share of a day), state: past|today|future, used, w (weight when not pinned)}], targets: {weekday: percent of the week} */
function groups(segs, targets, R) {
  const tgt = (wd) => { const v = targets && targets[wd]; return v == null || !Number.isFinite(Number(v)) ? null : Math.max(0, Math.min(100, Number(v))); };
  const rem = segs.filter((s) => s.state !== 'past');
  let fixedWant = 0, autoW = 0;
  const want = new Map(), alloc = new Map();
  for (const s of rem) {
    const t = tgt(s.wd);
    if (t != null) { const w = Math.max(0, t * s.f - (s.state === 'today' ? s.used : 0)); want.set(s, w); fixedWant += w; }
    else autoW += s.w;
  }
  const over = fixedWant > R + 1e-9;
  const k = over && fixedWant > 0 ? R / fixedWant : 1;
  const pool = Math.max(0, R - fixedWant);
  for (const s of rem) alloc.set(s, want.has(s) ? want.get(s) * k : (over || autoW <= 0 ? 0 : pool * s.w / autoW));
  const leftover = autoW <= 0 && !over ? Math.max(0, R - fixedWant) : 0;
  const order = [];
  for (const s of segs) if (!order.includes(s.wd)) order.push(s.wd);
  const NAMES = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
  const days = order.map((wd) => {
    const g = segs.filter((s) => s.wd === wd), t = tgt(wd);
    const state = g.some((s) => s.state === 'today') ? 'today' : g.every((s) => s.state === 'past') ? 'past' : 'future';
    const used = g.reduce((a, s) => a + s.used, 0);
    const planned = g.reduce((a, s) => a + s.used + (s.state === 'past' ? 0 : (alloc.get(s) || 0)), 0);
    return { wd, name: NAMES[wd], state, used: round(used), planned: round(planned), fixed: t != null, target: t };
  });
  const autoSegs = rem.filter((s) => !want.has(s));
  return { days, over, leftover: round(leftover), autoDays: new Set(autoSegs.map((s) => s.wd)).size, autoEach: autoW > 0 && !over ? round(pool / autoW) : 0 };
}
const round = (x) => Math.round(x * 10) / 10;

/** Keeps the readings of the week percentage: a new one only when it changed, and not more than 3 weeks back. */
function record(list, usage, now) {
  const wk = usage && usage.week;
  if (!wk || wk.resetsAt == null) return list;
  const last = list[list.length - 1];
  if (last && Math.abs(last.w - wk.pct) < 0.05 && last.r === wk.resetsAt) return list;
  const out = list.concat([{ t: now, w: Math.round(wk.pct * 100) / 100, r: wk.resetsAt }]);
  return out.filter((s) => s.t > now - 21 * DAY).slice(-3000);
}

const api = { plan, record, usedAt, groups };
if (typeof module !== 'undefined' && module.exports) module.exports = api; else window.WeekPlan = api;
