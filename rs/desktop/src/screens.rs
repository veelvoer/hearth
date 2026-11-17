//! Real data for the screens of `rs/ui` (`hearth_ui::App`): chats of every computer, usage tiles, computers. Loaded off the UI thread
//! into plain structs, then poured into the window's models on the UI thread.
use crate::app::App;
use hearth_client::{fmt, RelayClient};
use hearth_relay::util::{now_ms, J};
use hearth_ui::{ChatRow, Device, Tile};
use slint::{ModelRc, VecModel};

#[derive(Default, Clone, Debug, PartialEq)]
pub struct Row { pub header: String, pub title: String, pub sub: String, pub working: bool, pub id: String, pub machine: String }
#[derive(Default, Clone, Debug)]
pub struct ScreenData { pub chats: Vec<Row>, pub history: Vec<Row>, pub tiles: Vec<(String, String, String)>, pub devices: Vec<(String, String, bool)>, pub status: String }

/// "Today", "Yesterday", "Previous 7 days", "Older" for a chat's last change.
pub fn group_of(mtime: i64, now: i64) -> &'static str {
    let day = 86_400_000i64; let d = (now / day) - (mtime / day);
    if d <= 0 { "Today" } else if d == 1 { "Yesterday" } else if d < 8 { "Previous 7 days" } else { "Older" }
}
fn title_of(t: &str, cwd: &str) -> String { if !t.trim().is_empty() { t.trim().chars().take(90).collect() } else { hearth_relay::util::basename(cwd) } }

/// Rows sorted newest first with a header where the group changes; chats running now come first under "Working now".
pub fn rows(mut list: Vec<(String, hearth_client::Sess, String)>, now: i64) -> Vec<Row> {
    list.sort_by(|a, b| (b.1.running || b.1.busy).cmp(&(a.1.running || a.1.busy)).then(b.1.mtime.cmp(&a.1.mtime)));
    let mut out: Vec<Row> = vec![]; let mut last = "";
    for (machine, s, mname) in list {
        let working = s.running || s.busy;
        let g = if working { "Working now" } else { group_of(s.mtime, now) };
        let header = if g != last { last = g; g.to_string() } else { String::new() };
        let proj = hearth_relay::util::basename(&s.cwd);
        out.push(Row { header, title: title_of(&s.title, &s.cwd), sub: format!("{} · {}{}", proj, if working { "working now".into() } else { fmt::age(now - s.mtime) }, if mname.is_empty() { String::new() } else { format!(" · {mname}") }), working, id: s.id, machine });
    }
    out
}

pub async fn load(a: &std::sync::Arc<App>) -> ScreenData {
    let mut d = ScreenData::default();
    let now = now_ms() as i64;
    let mut all = vec![];
    for m in a.machines().into_iter().chain(a.guest_machines()) {
        let name = if m.t("local") { String::new() } else { m.s("name") };
        d.devices.push((m.s("name"), if m.t("local") { "This computer".into() } else if m.t("secure") { format!("Server · {}", m.s("host")) } else { format!("{}:{}", m.s("host"), m.n("port")) }, !m.t("local")));
        if let Ok(c) = RelayClient::new(crate::app::to_machine(&m)) { if let Ok(list) = c.sessions().await { for s in list { all.push((m.s("id"), s, name.clone())); } } }
    }
    d.history = rows(all.clone(), now);
    d.chats = d.history.iter().take(60).cloned().collect();
    let st = a.public_state();
    let u = st.g("usage");
    let pct = |k: &str| if u.g(k).is_null() { "-".to_string() } else { format!("{}%", u.g(k).n("pct").floor()) };
    d.tiles = vec![("Session (5h)".into(), pct("session"), "plan usage".into()), ("Weekly".into(), pct("week"), "plan usage".into()), ("Opus".into(), pct("opus"), String::new()), ("Sonnet".into(), pct("sonnet"), String::new()), ("Chats".into(), all.len().to_string(), format!("{} computers", d.devices.len()))];
    d.status = st.s("error");
    d
}

fn model(v: &[Row]) -> ModelRc<ChatRow> { ModelRc::new(VecModel::from(v.iter().map(|r| ChatRow { header: r.header.clone().into(), title: r.title.clone().into(), sub: r.sub.clone().into(), working: r.working, ..Default::default() }).collect::<Vec<_>>())) }

pub fn apply(w: &hearth_ui::App, d: &ScreenData) {
    w.set_chats(model(&d.chats)); w.set_history(model(&d.history));
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
        let r = rows(vec![s("old", 86_400_000, false), s("new", now - 5, false), s("run", now - 9000, true)], now);
        assert_eq!(r.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["run", "new", "old"]);
        assert_eq!(r[0].header, "Working now"); assert_eq!(r[1].header, "Today"); assert_eq!(r[2].header, "Older"); assert!(r[0].sub.contains("working now"));
    }
}
