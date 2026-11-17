//! Real data for the screens of `rs/ui` (`hearth_ui::App`): chats of every computer, usage tiles, computers. Loaded off the UI thread
//! into plain structs, then poured into the window's models on the UI thread.
use crate::app::App;
use hearth_client::{fmt, RelayClient};
use hearth_relay::util::{now_ms, J};
use hearth_ui::{ChatRow, Device, Tile};
use slint::{ModelRc, VecModel};

#[derive(Default, Clone, Debug, PartialEq)]
pub struct Row { pub header: String, pub title: String, pub sub: String, pub working: bool, pub id: String, pub machine: String, pub project: String, pub group: String, pub pinned: bool, pub cwd: String }
#[derive(Default, Clone, Debug)]
pub struct ScreenData { pub chats: Vec<Row>, pub history: Vec<Row>, pub tiles: Vec<(String, String, String)>, pub devices: Vec<(String, String, bool)>, pub device_ids: Vec<String>, pub status: String }

/// "Today", "Yesterday", "Previous 7 days", "Older" for a chat's last change.
pub fn group_of(mtime: i64, now: i64) -> &'static str {
    let day = 86_400_000i64; let d = (now / day) - (mtime / day);
    if d <= 0 { "Today" } else if d == 1 { "Yesterday" } else if d < 8 { "Previous 7 days" } else { "Older" }
}
fn title_of(t: &str, cwd: &str) -> String { if !t.trim().is_empty() { t.trim().chars().take(90).collect() } else { hearth_relay::util::basename(cwd) } }

/// Chat tags of a computer: id -> (pinned, name).
pub type Tags = std::collections::HashMap<String, (bool, String)>;

/// Rows sorted (pinned first, then running, then newest) with a header where the group changes.
pub fn rows(mut list: Vec<(String, hearth_client::Sess, String)>, now: i64, tags: &Tags) -> Vec<Row> {
    let pin = |id: &str| tags.get(id).map(|t| t.0).unwrap_or(false);
    list.sort_by(|a, b| pin(&b.1.id).cmp(&pin(&a.1.id)).then((b.1.running || b.1.busy).cmp(&(a.1.running || a.1.busy))).then(b.1.mtime.cmp(&a.1.mtime)));
    let mut out: Vec<Row> = vec![]; let mut last = "";
    for (machine, s, mname) in list {
        let working = s.running || s.busy; let pinned = pin(&s.id);
        let g = if pinned { "Pinned" } else if working { "Working now" } else { group_of(s.mtime, now) };
        let header = if g != last { last = g; g.to_string() } else { String::new() };
        let proj = hearth_relay::util::basename(&s.cwd);
        let name = tags.get(&s.id).map(|t| t.1.clone()).filter(|n| !n.is_empty());
        out.push(Row { header, group: g.to_string(), title: name.unwrap_or_else(|| title_of(&s.title, &s.cwd)), sub: format!("{} · {}{}", proj, if working { "working now".into() } else { fmt::age(now - s.mtime) }, if mname.is_empty() { String::new() } else { format!(" · {mname}") }), working, pinned, id: s.id, machine, project: proj, cwd: s.cwd });
    }
    out
}

pub fn tags_of(v: &serde_json::Value) -> Tags { v.g("tags").as_object().map(|m| m.iter().map(|(k, t)| (k.clone(), (t.t("pin"), t.s("name")))).collect()).unwrap_or_default() }

pub async fn load(a: &std::sync::Arc<App>, days: u32) -> ScreenData {
    let mut d = ScreenData::default();
    let mut tags = Tags::new();
    let now = now_ms() as i64;
    let mut all = vec![];
    for m in a.machines().into_iter().chain(a.guest_machines()) {
        let name = if m.t("local") { String::new() } else { m.s("name") };
        d.device_ids.push(m.s("id"));
        if let Ok(t) = a.relay_json(&m, "GET", "/chat-tags", None).await { tags.extend(tags_of(&t)); }
        d.devices.push((m.s("name"), if m.t("local") { "This computer".into() } else if m.t("secure") { format!("Server · {}", m.s("host")) } else { format!("{}:{}", m.s("host"), m.n("port")) }, !m.t("local")));
        if let Ok(c) = RelayClient::new(crate::app::to_machine(&m)) { if let Ok(list) = c.sessions().await { for s in list { all.push((m.s("id"), s, name.clone())); } } }
    }
    d.history = rows(all.clone(), now, &tags);
    d.chats = d.history.iter().take(60).cloned().collect();
    let st = a.public_state();
    let u = st.g("usage");
    let rep = match a.local() { Some(m) => a.relay_json(&m, "GET", &format!("/usage-report?days={days}"), None).await.unwrap_or_default(), None => serde_json::Value::Null };
    let t = rep.g("totals");
    d.tiles = vec![("API value".into(), format!("${:.2}", t.n("cost")), "what it would cost per token".into()), ("Tokens used".into(), human(t.n("tokens")), String::new()), ("Sessions".into(), (t.n("sessions") as i64).to_string(), format!("{} active days", t.n("activeDays") as i64)), ("Messages".into(), (t.n("prompts") as i64).to_string(), String::new())];
    d.status = st.s("error");
    d
}

pub fn human(n: f64) -> String { if n >= 1e6 { format!("{:.1}M", n / 1e6) } else if n >= 1e3 { format!("{:.0}k", n / 1e3) } else { format!("{}", n as i64) } }

fn model_sel(v: &[Row], selected: &str) -> ModelRc<ChatRow> { ModelRc::new(VecModel::from(v.iter().map(|r| ChatRow { header: r.header.clone().into(), title: r.title.clone().into(), sub: r.sub.clone().into(), working: r.working, pinned: r.pinned, selected: !selected.is_empty() && r.id == selected, ..Default::default() }).collect::<Vec<_>>())) }

pub fn apply(w: &hearth_ui::App, d: &ScreenData, selected: &str) {
    w.set_chats(model_sel(&d.chats, selected)); w.set_history(model_sel(&d.history, selected));
    w.set_tiles(ModelRc::new(VecModel::from(d.tiles.iter().map(|(l, v, s)| Tile { label: l.into(), value: v.into(), sub: s.into() }).collect::<Vec<_>>())));
    w.set_devices(ModelRc::new(VecModel::from(d.devices.iter().map(|(n, s, r)| Device { name: n.into(), sub: s.into(), removable: *r }).collect::<Vec<_>>())));
    if !d.status.is_empty() { w.set_status(d.status.clone().into()); }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn s(id: &str, mtime: i64, running: bool) -> (String, hearth_client::Sess, String) { (format!("m"), serde_json::from_value(serde_json::json!({"id": id, "title": format!("t{id}"), "cwd": "/a/proj", "mtime": mtime, "live": false, "running": running})).unwrap(), String::new()) }
    #[test] fn groups() { let now = 10 * 86_400_000 + 5000; assert_eq!(group_of(now - 10, now), "Today"); assert_eq!(group_of(now - 86_400_000, now), "Yesterday"); assert_eq!(group_of(now - 4 * 86_400_000, now), "Previous 7 days"); assert_eq!(group_of(0, now), "Older"); }
    #[test] fn rows_group_and_order() {
        let now = 20 * 86_400_000 + 1000;
        let r = rows(vec![s("old", 86_400_000, false), s("new", now - 5, false), s("run", now - 9000, true)], now, &Tags::new());
        assert_eq!(r.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["run", "new", "old"]);
        assert_eq!(r[0].header, "Working now"); assert_eq!(r[1].header, "Today"); assert_eq!(r[2].header, "Older"); assert!(r[0].sub.contains("working now"));
    }
}
