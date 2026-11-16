//! The weekly limit planned over the days of the week. Port of weekplan.js.
use crate::util::*;
use serde_json::{json, Value};

const DAY: f64 = 86400000.0;
fn round1(x: f64) -> Value { num((x * 10.0).round() / 10.0) }
fn used_at(samples: &[Value], t: f64, now: f64) -> f64 { let mut w = 0.0; for s in samples { if s.n("t") <= t.min(now) { w = s.n("w"); } else { break; } } w }

/// Keeps the readings of the week percentage: a new one only when it changed, and not more than 3 weeks back. None = unchanged.
pub fn record(list: &[Value], usage: &Value, now: f64) -> Option<Vec<Value>> {
    let wk = usage.g("week"); if !wk.is_object() || wk.g("resetsAt").is_null() { return None; }
    if let Some(last) = list.last() { if (last.n("w") - wk.n("pct")).abs() < 0.05 && last.g("r") == wk.g("resetsAt") { return None; } }
    let mut out = list.to_vec(); out.push(json!({"t": num(now), "w": num((wk.n("pct") * 100.0).round() / 100.0), "r": wk.g("resetsAt")}));
    let out: Vec<Value> = out.into_iter().filter(|s| s.n("t") > now - 21.0 * DAY).collect();
    Some(out.iter().skip(out.len().saturating_sub(3000)).cloned().collect())
}
fn tgt(targets: &Value, wd: i64) -> Option<f64> { let v = targets.g(&wd.to_string()); vnum_opt(v).filter(|_| !v.is_null()).map(|x| x.clamp(0.0, 100.0)) }

pub fn plan(usage: &Value, samples: &[Value], targets: &Value, now: f64, tz_min: f64) -> Value {
    let wk = usage.g("week");
    if !wk.is_object() || wk.g("resetsAt").is_null() { return json!({"unavailable": "No weekly reading yet. Use Claude once and it shows up here."}); }
    let end = wk.n("resetsAt"); let start = end - 7.0 * DAY;
    if end <= now { return json!({"unavailable": "A fresh week starts with your next message."}); }
    let off = tz_min * 60000.0;
    let day_start = |t: f64| ((t + off) / DAY).floor() * DAY - off;
    let wd_of = |t: f64| -> i64 { let d = ((t + off) / DAY).floor() as i64; (d + 4).rem_euclid(7) };   // 1970-01-01 was a Thursday
    let pct_now = wk.n("pct").clamp(0.0, 100.0);
    let mut in_win: Vec<Value> = samples.iter().filter(|s| s.n("t") >= start - 60000.0 && s.n("t") <= now + 60000.0 && (s.g("r").is_null() || (s.n("r") - end).abs() < 120000.0)).cloned().collect();
    in_win.sort_by(|a, b| a.n("t").partial_cmp(&b.n("t")).unwrap_or(std::cmp::Ordering::Equal));
    struct Seg { wd: i64, start: f64, end: f64, f: f64, state: &'static str, used: f64, w: f64 }
    let mut segs = Vec::new();
    let mut d = day_start(start);
    while d < end {
        let a = start.max(d); let b = end.min(d + DAY);
        if b - a >= 60000.0 { segs.push(Seg { wd: wd_of(a + 1.0), start: a, end: b, f: (b - a) / DAY, state: "", used: 0.0, w: 0.0 }); }
        d += DAY;
    }
    for s in segs.iter_mut() {
        s.state = if s.end <= now { "past" } else if s.start <= now { "today" } else { "future" };
        s.used = if s.state == "future" { 0.0 } else { (used_at(&in_win, s.end, now) - used_at(&in_win, s.start, now)).max(0.0) };
        if s.state == "today" { s.used = (pct_now - used_at(&in_win, s.start, now)).max(0.0); }
    }
    let unknown = match in_win.first() { Some(f) if !samples.iter().any(|s| s.n("t") < f.n("t")) && f.n("w") > 3.0 => f.n("w"), _ => 0.0 };
    let r = (100.0 - pct_now).max(0.0);
    for g in segs.iter_mut() { g.w = if g.state == "today" { (g.end - now) / DAY } else { g.f }; }
    let seg_vals: Vec<Value> = segs.iter().map(|g| json!({"wd": g.wd, "f": g.f, "state": g.state, "used": g.used, "w": g.w})).collect();
    let out = groups(&seg_vals, targets, r);
    let mut clean_t = serde_json::Map::new(); for wd in 0..7 { if let Some(v) = tgt(targets, wd) { clean_t.insert(wd.to_string(), num(v)); } }
    merge(&json!({"now": num(now), "start": num(start), "end": num(end), "used": round1(pct_now), "left": round1(r)}), &merge(&out, &json!({"unknown": round1(unknown), "R": r, "targets": clean_t, "segs": seg_vals})))
}
/// Shares the part of the week that is left (R percent) over the days still to come.
pub fn groups(segs0: &[Value], targets: &Value, r: f64) -> Value {
    let n = segs0.len();
    let split = n > 1 && segs0[0].n("wd") == segs0[n - 1].n("wd");
    let segs: Vec<(Value, bool)> = segs0.iter().enumerate().map(|(i, s)| (s.clone(), split && i == 0)).collect();
    let rem: Vec<usize> = (0..n).filter(|i| segs[*i].0.s("state") != "past").collect();
    let (mut fixed_want, mut auto_w) = (0.0, 0.0);
    let mut want: std::collections::HashMap<usize, f64> = Default::default();
    for &i in &rem { let (s, st) = &segs[i]; let t = if *st { None } else { tgt(targets, s.n("wd") as i64) };
        match t { Some(t) => { let w = (t - if s.s("state") == "today" { s.n("used") } else { 0.0 }).max(0.0); want.insert(i, w); fixed_want += w; } None => auto_w += s.n("w") } }
    let over = fixed_want > r + 1e-9;
    let k = if over && fixed_want > 0.0 { r / fixed_want } else { 1.0 };
    let pool = (r - fixed_want).max(0.0);
    let mut alloc: std::collections::HashMap<usize, f64> = Default::default();
    for &i in &rem { alloc.insert(i, match want.get(&i) { Some(w) => w * k, None => if over || auto_w <= 0.0 { 0.0 } else { pool * segs[i].0.n("w") / auto_w } }); }
    let leftover = if auto_w <= 0.0 && !over { (r - fixed_want).max(0.0) } else { 0.0 };
    let key_of = |i: usize| if segs[i].1 { format!("{}s", segs[i].0.n("wd") as i64) } else { (segs[i].0.n("wd") as i64).to_string() };
    let mut order: Vec<String> = vec![]; for i in 0..n { let k = key_of(i); if !order.contains(&k) { order.push(k); } }
    const NAMES: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let days: Vec<Value> = order.iter().map(|key| {
        let g: Vec<usize> = (0..n).filter(|i| key_of(*i) == *key).collect();
        let wd = segs[g[0]].0.n("wd") as i64; let st = segs[g[0]].1; let t = if st { None } else { tgt(targets, wd) };
        let state = if g.iter().any(|i| segs[*i].0.s("state") == "today") { "today" } else if segs[g[0]].0.s("state") == "past" { "past" } else { "future" };
        let used: f64 = g.iter().map(|i| segs[*i].0.n("used")).sum();
        let planned: f64 = g.iter().map(|i| segs[*i].0.n("used") + if segs[*i].0.s("state") == "past" { 0.0 } else { alloc.get(i).copied().unwrap_or(0.0) }).sum();
        json!({"wd": wd, "name": NAMES[wd as usize], "state": state, "used": round1(used), "planned": round1(planned), "fixed": t.is_some(), "target": t.map(num).unwrap_or(Value::Null), "start": st})
    }).collect();
    let auto_days: std::collections::HashSet<String> = rem.iter().filter(|i| !want.contains_key(i)).map(|i| key_of(*i)).collect();
    json!({"days": days, "over": over, "leftover": round1(leftover), "autoDays": auto_days.len(), "autoEach": if auto_w > 0.0 && !over { round1(pool / auto_w) } else { json!(0) }})
}
