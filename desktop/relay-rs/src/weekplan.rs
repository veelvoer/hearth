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

#[cfg(test)]
mod tests {
    // The weekly limit shared out over the days of the week (weekplan.test.js)
    use super::*;
    const D: f64 = 86400000.0; const TZ: f64 = 120.0;   // tz=120 means local = UTC+2
    fn t(y: i64, m: i64, d: i64, h: i64, mi: i64) -> f64 {   // epoch ms of a UTC time
        let (y2, m2) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
        let era = y2.div_euclid(400); let yoe = y2.rem_euclid(400); let doy = (153 * m2 + 2) / 5 + d - 1; let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        (((era * 146097 + doe - 719468) * 24 + h) * 60 + mi) as f64 * 60000.0
    }
    fn end() -> f64 { t(2026, 10, 15, 15, 0) }   // reset Thu 2026-10-15 15:00 UTC (17:00 local)
    fn smp(at: f64, w: f64) -> Value { json!({"t": at, "w": w, "r": end()}) }
    fn days(p: &Value) -> Vec<Value> { p["days"].as_array().cloned().unwrap_or_default() }
    fn sum(p: &Value) -> f64 { days(p).iter().map(|d| d.n("planned")).sum() }
    fn by(p: &Value, n: &str) -> f64 { days(p).iter().rev().find(|d| d.s("name") == n).map(|d| d.n("planned")).unwrap_or(-1.0) }   // the last one: the reset weekday also has a small day at the start
    fn eq(name: &str, a: f64, b: f64) { assert!((a - b).abs() < 0.6, "{name}: {a} expect {b}"); }
    fn samples() -> Vec<Value> { vec![smp(t(2026, 10, 9, 8, 0), 5.0), smp(t(2026, 10, 10, 8, 0), 12.0), smp(t(2026, 10, 10, 20, 0), 30.0), smp(t(2026, 10, 11, 8, 0), 35.0)] }
    fn usage(pct: f64) -> Value { json!({"week": {"pct": pct, "resetsAt": end()}}) }

    #[test] fn eight_bars_the_reset_weekday_is_split_in_a_start_and_an_end_day() {
        let now = t(2026, 10, 11, 10, 0);
        let p = plan(&usage(35.0), &samples(), &json!({}), now, TZ);
        eq("8 bars", days(&p).len() as f64, 8.0);
        eq("plan sums to 100 (no unknown)", sum(&p), 100.0);
    }
    #[test] fn pinned_days_keep_their_share() {
        let now = t(2026, 10, 11, 10, 0);
        let p = plan(&usage(35.0), &samples(), &json!({"6": 20, "0": 20}), now, TZ);
        eq("sat (already past) keeps what was used", by(&p, "Sat"), 25.0);
        eq("sun planned = max(20, used)", by(&p, "Sun"), 20.0);
        eq("sum 100", sum(&p), 100.0);
    }
    #[test] fn after_sunday_the_rest_is_recalculated() {
        let mut s2 = samples(); s2.push(smp(t(2026, 10, 11, 20, 0), 50.0));
        let p = plan(&usage(50.0), &s2, &json!({"6": 20, "0": 20}), t(2026, 10, 12, 7, 0), TZ);
        eq("sum 100", sum(&p), 100.0);
    }
    #[test] fn over_commit_is_scaled_down() {
        let p = plan(&usage(10.0), &[], &json!({"6": 60, "0": 60}), t(2026, 10, 9, 10, 0), TZ);
        eq("over flag", if p["over"] == json!(true) { 1.0 } else { 0.0 }, 1.0);
        eq("sat scaled to half of the 90 left", by(&p, "Sat"), 45.0);
        eq("autos get 0", by(&p, "Mon"), 0.0);
    }
    #[test] fn the_reset_weekday_merges_its_pieces() {
        let p = plan(&usage(0.0), &[], &json!({"4": 14}), t(2026, 10, 8, 16, 0), TZ);
        eq("thu planned 14", by(&p, "Thu"), 14.0); eq("sum", sum(&p), 100.0);
    }
    #[test] fn a_rolled_week_is_unavailable() {
        let p = plan(&json!({"week": {"pct": 10, "resetsAt": t(2026, 10, 1, 0, 0)}}), &[], &json!({}), t(2026, 10, 12, 7, 0), TZ);
        assert!(p["unavailable"].is_string());
    }
    #[test] fn record_dedupes_equal_readings() {
        let u = |pct: f64| usage(pct);
        let mut h: Vec<Value> = vec![];
        for (u1, now) in [(u(5.0), 1000.0), (u(5.0), 2000.0), (u(6.0), 3000.0)] { if let Some(n) = record(&h, &u1, now) { h = n; } }
        assert_eq!(h.len(), 2);
    }
    #[test] fn a_future_pinned_day_with_saturday_still_ahead() {
        let now = t(2026, 10, 9, 10, 0); let s = vec![smp(t(2026, 10, 9, 8, 0), 5.0)];
        let p = plan(&usage(5.0), &s, &json!({"6": 20, "0": 20}), now, TZ);
        eq("future sat 20", by(&p, "Sat"), 20.0); eq("future sun 20", by(&p, "Sun"), 20.0);
        eq("autos share the rest evenly", by(&p, "Mon"), by(&p, "Tue"));
        eq("sum", sum(&p), 100.0);
    }
    #[test] fn the_sliders_live_recalculation_equals_asking_again_with_that_pin() {
        let now = t(2026, 10, 9, 10, 0); let s = vec![smp(t(2026, 10, 9, 8, 0), 5.0)];
        let base = plan(&usage(5.0), &s, &json!({"0": 20}), now, TZ);
        let pinned = plan(&usage(5.0), &s, &json!({"0": 20, "6": 30}), now, TZ);
        let mut tg = base["targets"].clone(); tg["6"] = json!(30);
        let live = groups(base["segs"].as_array().unwrap(), &tg, base["R"].as_f64().unwrap());
        assert_eq!(live["days"], pinned["days"]);
    }
}
