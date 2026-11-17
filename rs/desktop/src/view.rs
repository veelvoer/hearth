//! Pure mappings between the app's state and the properties of rs/ui's `App`: settings switches, usage numbers, list filters.
use hearth_relay::util::J;
use serde_json::{json, Value};

/// The switches of rs/ui's `SettingsState`.
#[derive(Clone, Debug, PartialEq)]
pub struct UiSettings { pub theme: String, pub motion: bool, pub frosted: bool, pub expressive: bool, pub notify_finished: bool, pub notify_usage: bool, pub auto_update: bool, pub install_on_quit: bool, pub run_on_server: bool, pub keep_copy: bool, pub ask_edit: bool, pub ask_cmd: bool }

pub fn settings_from(s: &Value) -> UiSettings {
    let not_false = |k: &str| s.g(k) != &json!(false); let is_true = |k: &str| s.g(k) == &json!(true);
    UiSettings { theme: if s.s("theme").is_empty() { "system".into() } else { s.s("theme") }, motion: not_false("motion"), frosted: is_true("glass"), expressive: s.s("style") == "material",
        notify_finished: not_false("notifyDone"), notify_usage: not_false("notifyReset"), auto_update: not_false("autoUpdate"), install_on_quit: is_true("installOnQuit"), run_on_server: is_true("runOnServer"), keep_copy: is_true("keepCopy"), ask_edit: not_false("askEdit"), ask_cmd: not_false("askCmd") }
}

/// The settings patch for a switch id of `SettingsState` (kebab-case field name).
pub fn setting_patch(id: &str, on: bool) -> Option<Value> {
    Some(match id {
        "motion" => json!({"motion": on}), "frosted" => json!({"glass": on}), "expressive" => json!({"style": if on { "material" } else { "classic" }}),
        "notify-finished" => json!({"notifyDone": on, "notifyPerm": on, "notifyQuestion": on}), "notify-usage" => json!({"notifyReset": on}),
        "auto-update" => json!({"autoUpdate": on}), "install-on-quit" => json!({"installOnQuit": on}), "run-on-server" => json!({"runOnServer": on}), "keep-copy" => json!({"keepCopy": on}),
        "ask-edit" => json!({"askEdit": on}), "ask-cmd" => json!({"askCmd": on}), _ => return None,
    })
}

/// The next value when a composer pill is pressed: mode, model, effort go round their choices.
pub fn cycle(option: &str, cur: &str) -> String {
    let list: &[&str] = match option { "edits" => &["acceptEdits", "plan", "manual", "auto"], "model" => &["", "opus", "sonnet", "haiku"], "effort" => &["", "low", "medium", "high"], _ => return cur.to_string() };
    let i = list.iter().position(|x| *x == cur).map(|i| i + 1).unwrap_or(0);
    list[i % list.len()].to_string()
}
pub fn option_label(option: &str, v: &str) -> String {
    match (option, v) { ("edits", "acceptEdits") => "Edits".into(), ("edits", "plan") => "Plan only".into(), ("edits", "manual") => "Ask me".into(), ("edits", "auto") => "Auto".into(), ("model", "") => "Default model".into(), ("effort", "") => "Default effort".into(), (_, v) => { let mut c = v.chars(); c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default() } }
}

pub struct UsageView { pub session_pct: f32, pub session_reset: String, pub session_pace: String, pub week_pct: f32, pub week_reset: String, pub week_pace: String, pub opus_pct: f32, pub sonnet_pct: f32, pub updated: String }
fn pace_word(l: &Value, window: f64, now: f64) -> String {
    let r = l.n("resetsAt"); if r <= now { return String::new(); }
    let elapsed = ((now - (r - window)) / window).clamp(0.0, 1.0); let delta = l.n("pct") - elapsed * 100.0;
    if elapsed < 0.04 { String::new() } else if delta > 5.0 { "Ahead of pace".into() } else if delta < -5.0 { "Under pace".into() } else { "On pace".into() }
}
pub fn usage_view(u: &Value, now: f64) -> UsageView {
    let reset = |l: &Value| if l.n("resetsAt") > now { format!("Resets in {}", crate::app::dur(l.n("resetsAt") - now)) } else { String::new() };
    let pct = |k: &str| crate::app::now_pct(u.g(k), now).unwrap_or(0.0) as f32;
    UsageView { session_pct: pct("session"), session_reset: reset(u.g("session")), session_pace: pace_word(u.g("session"), 5.0 * 3600e3, now), week_pct: pct("week"), week_reset: reset(u.g("week")), week_pace: pace_word(u.g("week"), 7.0 * 86400e3, now), opus_pct: pct("opus"), sonnet_pct: pct("sonnet"),
        updated: if u.is_null() { "No usage yet".into() } else { format!("Usage updated {} ago", crate::app::dur(now - u.n("at"))) } }
}

/// Keeps the rows whose title or subtitle contains `q` (case-insensitive) and, when set, whose project is `project`; the first kept row of each group gets its header back.
pub fn filter_rows(rows: &[crate::screens::Row], q: &str, project: &str) -> Vec<crate::screens::Row> {
    let q = q.trim().to_lowercase();
    let mut out: Vec<crate::screens::Row> = rows.iter().filter(|r| (q.is_empty() || r.title.to_lowercase().contains(&q) || r.sub.to_lowercase().contains(&q)) && (project.is_empty() || r.project == project)).cloned().collect();
    // headers: rebuild so each group starts with one
    let mut last = String::new();
    for r in out.iter_mut() { let g = if r.group.is_empty() { r.header.clone() } else { r.group.clone() }; r.header = if g != last { last = g; last.clone() } else { String::new() }; }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn settings_roundtrip() {
        let s = settings_from(&json!({"theme": "dark", "glass": true, "style": "material", "notifyDone": false}));
        assert_eq!(s.theme, "dark"); assert!(s.motion && s.frosted && s.expressive && !s.notify_finished && s.auto_update && !s.install_on_quit && s.ask_edit);
        assert_eq!(setting_patch("expressive", false).unwrap(), json!({"style": "classic"})); assert_eq!(setting_patch("notify-finished", true).unwrap().s("notifyPerm"), "true"); assert!(setting_patch("nope", true).is_none());
    }
    #[test] fn cycles() { assert_eq!(cycle("edits", "acceptEdits"), "plan"); assert_eq!(cycle("edits", "auto"), "acceptEdits"); assert_eq!(cycle("model", ""), "opus"); assert_eq!(cycle("effort", "high"), ""); assert_eq!(option_label("edits", "plan"), "Plan only"); assert_eq!(option_label("model", "opus"), "Opus"); }
    #[test] fn usage_numbers() {
        let now = 1e12; let u = json!({"session": {"pct": 60, "resetsAt": now + 2.0 * 3600e3}, "week": {"pct": 10, "resetsAt": now + 6.0 * 86400e3}, "opus": null, "at": now - 120000.0});
        let v = usage_view(&u, now); assert_eq!(v.session_pct, 60.0); assert_eq!(v.session_reset, "Resets in 2h 0m"); assert_eq!(v.session_pace, "On pace"); assert_eq!(v.opus_pct, 0.0); assert_eq!(v.updated, "Usage updated 2m ago");
    }
}
