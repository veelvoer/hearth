//! The app's state and what runs behind the window: this computer's relay (inside the app), usage polling, the event feeds of every
//! computer, notifications and the "popups" (a call, a permission, a question, a support mail, a friend's request).
//! Nothing here draws: it tells the UI layer what happened through an [`Event`], so the same code runs under the tests.
use crate::{net, sys, upd};
use hearth_relay::relay::Relay;
use hearth_relay::util::{arr, home, hostname, now_ms, read_json, write_private, J};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");
pub const TRAY_PNG: &[u8] = include_bytes!("../assets/tray.png");

/// A small window on top of everything. The UI layer opens it; [`App::call_action`] answers it.
#[derive(Clone, Debug)]
pub struct PopupSpec { pub key: String, pub kind: String, pub title: String, pub text: String, pub permission: bool, pub own: bool, pub height: f32, pub close_after_ms: u64 }

/// Everything the UI layer has to react to.
#[derive(Clone, Debug)]
pub enum Event {
    /// Usage, settings, history or error changed: read [`App::public_state`] again.
    State,
    /// An event of a relay (done, question, permission...), for the chats list.
    Relay { machine: String, ev: Value },
    /// One record of a run started with [`App::send`].
    Stream { sid: String, rec: Value },
    MoveProgress(String),
    UpdateAvailable(Value),
    UpdateProgress(Value),
    VoiceInstall(Value),
    PendingProjects(Vec<Value>),
    PairRequests(Value),
    OpenSession { machine: String, session: String, title: String, cwd: String, voice: bool },
    Goto(String),
    ShowMain,
    Popup(PopupSpec),
    ClosePopup(String),
    /// The system colors or dark/light changed (or the user's choice did): ask [`App::theme`].
    ThemeChanged,
    /// Replace the app and tray icon (PNG bytes).
    Icon(Vec<u8>),
    /// Update the tray's "Ring me" check mark.
    RingMe(bool),
    Quit,
    Relaunch,
}
pub type Sink = Arc<dyn Fn(Event) + Send + Sync>;
pub type Notifier = Arc<dyn Fn(&str, &str) + Send + Sync>;

#[derive(Default)]
pub struct RelayInfo { pub ok: bool, pub in_use: bool, pub error: Option<String>, pub port: u16, pub token: String, pub r: Option<Arc<Relay>> }

pub struct App {
    pub s: Mutex<Value>,
    pub dir: PathBuf,
    pub relay: Mutex<RelayInfo>,
    pub http: reqwest::Client,
    pub long: reqwest::Client,
    pub events_http: reqwest::Client,
    pub rt: tokio::runtime::Handle,
    pub sink: Sink,
    pub notifier: Mutex<Notifier>,
    pub icon_file: PathBuf,
    pub upd: upd::Upd,
    pub quitting: AtomicBool,
    /// The main window, as the UI layer reports it (for "you are looking at the app: it shows this itself").
    pub main_visible: AtomicBool,
    pub main_focused: AtomicBool,
    pub(crate) listeners: Mutex<Vec<tokio::task::JoinHandle<()>>>,
    pub(crate) calls: Mutex<HashMap<String, (Value, Value)>>,   // popup key -> (computer, event)
    notified_req: Mutex<VecDeque<String>>,
    pend_seen: Mutex<HashSet<String>>, pub(crate) last_pend: Mutex<Value>,
    pair_seen: Mutex<HashSet<String>>, pub(crate) last_pair_req: Mutex<Value>,
    refreshing: AtomicBool, pub(crate) voice_job: AtomicBool,
    save_gen: AtomicU64,
    pub(crate) last_guest_sig: Mutex<String>,
}

/// Where the app keeps its settings: the same folder the Electron/Tauri app used, so nothing is lost when moving over.
pub fn data_dir() -> PathBuf {
    if cfg!(windows) { PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("Hearth") }
    else if cfg!(target_os = "macos") { home().join("Library/Application Support/Hearth") }
    else { sys::config_home().join("Hearth") }
}

pub fn defaults() -> Value {
    json!({"settings": {"interval": 5, "thresholds": [75, 90, 100], "notifyReset": true, "callMe": false, "theme": "system", "palette": "claude", "background": true, "autostart": false},
           "machines": [], "deployed": {}, "history": [], "usage": null, "profile": null, "tok": null, "ln": {}, "error": null})
}

pub fn load_state(dir: &std::path::Path) -> Value {
    let f = dir.join("state.json");
    let existing = f.exists();
    let mut s = defaults();
    if let Some(Value::Object(m)) = read_json(&f) { for (k, v) in m { s[k] = v; } }
    let mut st = defaults()["settings"].clone();
    if let Some(m) = s.g("settings").as_object() { for (k, v) in m { st[k] = v.clone(); } }
    if existing && st.get("onboarded").is_none() { st["onboarded"] = json!(true); }   // someone who already uses the app doesn't need the first-run tutorial
    s["settings"] = st;
    if s.t("tok") || s.t("profile") { s.as_object_mut().unwrap().remove("tok"); s["profile"] = Value::Null; }
    s
}

pub fn guest_pub(g: &Value) -> Value {
    let host = if g.s("host").is_empty() { "A friend".to_string() } else { g.s("host") };
    let title = if g.s("title").is_empty() { "shared chat".to_string() } else { g.s("title") };
    json!({"id": format!("g:{}", g.s("share")), "name": format!("{host}: {title}"), "host": g.g("hostname"), "port": g.g("port"), "local": false, "secure": g.t("secure"), "guest": true, "hostName": g.s("host"), "share": g.g("share"), "title": g.s("title")})
}
pub fn pub_m(m: &Value) -> Value { json!({"id": m.g("id"), "name": m.g("name"), "host": m.g("host"), "port": m.g("port"), "local": m.t("local"), "secure": m.t("secure")}) }
pub fn strip_token(m: &Value) -> Value { let mut m = m.clone(); if let Some(o) = m.as_object_mut() { o.remove("token"); } m }

/// A relay machine (a JSON object with id/name/host/port/token/secure) as the client library's type.
pub fn to_machine(m: &Value) -> hearth_client::Machine {
    hearth_client::Machine { name: m.s("name"), host: m.s("host"), port: m.n("port") as u16, token: m.s("token"), secure: m.t("secure"), guest: m.t("guest"), label: String::new(), share: m.s("share") }
}

// ───────────────────────── pure helpers ─────────────────────────
pub fn dur(ms: f64) -> String {
    let m = (ms / 60000.0).floor().max(0.0) as i64;
    if m < 60 { format!("{m}m") } else if m < 1440 { format!("{}h {}m", m / 60, m % 60) } else { format!("{}d {}h", m / 1440, (m % 1440) / 60) }
}
pub fn now_pct(l: &Value, t: f64) -> Option<f64> { if l.is_null() { None } else if l.n("resetsAt") > 0.0 && l.n("resetsAt") < t { Some(0.0) } else { Some(l.n("pct")) } }

/// Which usage notifications are due: crossing a threshold once per window, and a fresh window after a reset. Updates `s["ln"]`.
pub fn alerts(s: &mut Value, prev: &Value, u: &Value) -> Vec<(String, String)> {
    let mut out = vec![];
    let mut thr: Vec<f64> = arr(s.g("settings").g("thresholds")).iter().map(hearth_relay::util::vnum).collect();
    thr.sort_by(|a, b| b.partial_cmp(a).unwrap());
    for (k, name) in [("session", "Session (5h)"), ("week", "Weekly")] {
        let l = u.g(k); if l.is_null() { continue; }
        let win = ((l.n("resetsAt")) / 3600000.0).floor().to_string();
        let cur = s.g("ln").s(k); let cur = if cur.is_empty() { ":0".into() } else { cur };
        let (w0, d0) = cur.split_once(':').unwrap_or(("", "0"));
        let done = if w0 == win { d0.parse::<f64>().unwrap_or(0.0) } else { 0.0 };
        let Some(hit) = thr.iter().copied().find(|t| l.n("pct") >= *t && *t > done) else { continue };
        if !s.g("ln").is_object() { s["ln"] = json!({}); }
        s["ln"][k] = json!(format!("{win}:{hit}"));
        let title = if hit >= 100.0 { format!("{name} limit reached") } else { format!("{name} at {}%", l.n("pct").floor()) };
        out.push((title, if l.n("resetsAt") > 0.0 { format!("Resets in {}", dur(l.n("resetsAt") - u.n("at"))) } else { format!("Crossed {hit}%") }));
    }
    let (ps, ns) = (prev.g("session"), u.g("session"));
    if s.g("settings").t("notifyReset") && !ps.is_null() && !ns.is_null() && ps.n("pct") >= 75.0 && ns.n("pct") < ps.n("pct") - 30.0 { out.push(("Session reset".into(), "A fresh 5-hour window is ready.".into())); }
    out
}

/// The notification for a relay event, or None when this kind is switched off. `force` skips the settings (the test button).
pub fn notification_for(ev: &Value, st: &Value, main_in_front: bool, force: bool) -> Option<(String, String)> {
    let not_false = |k: &str| st.g(k) != &json!(false);
    let on = match ev.s("kind").as_str() { "request" => Some(not_false("notifyRequest")), "permission" | "limit" => Some(not_false("notifyPerm")), "question" => Some(not_false("notifyQuestion")), "done" | "error" => Some(not_false("notifyDone")), "support" => Some(true), _ => None };
    if !force {
        if on != Some(true) { return None; }
        if st.g("notifyFocused") != &json!(true) && main_in_front { return None; }   // you are looking at the app: it shows this itself
    }
    let cwd = ev.s("cwd"); let proj = cwd.trim_end_matches('/').rsplit(['/', '\\']).next().filter(|s| !s.is_empty()).unwrap_or("Claude").to_string();
    let title = match ev.s("kind").as_str() {
        "request" => format!("{} wants to use your Claude", if ev.s("from").is_empty() { "A friend".into() } else { ev.s("from") }),
        "support" => format!("Support mail: {}", if ev.s("title").is_empty() { "new message".into() } else { ev.s("title") }),
        "limit" => format!("{proj}: usage limit reached"), "permission" => format!("{proj}: needs your OK"), "question" => format!("{proj}: Claude has a question"),
        "error" => format!("{proj}: something went wrong"), _ => format!("{proj}: Claude is done"),
    };
    Some((title, if ev.s("text").is_empty() { ev.s("title") } else { ev.s("text") }))
}

/// A desktop notification that can never freeze the app: on Linux it is handed to notify-send in its own process, on Windows to a toast.
pub fn platform_notify(icon: &str, title: &str, body: &str) {
    let t: String = title.chars().take(120).collect(); let b: String = body.chars().take(300).collect();
    #[cfg(target_os = "linux")]
    { let icon = icon.to_string(); std::thread::spawn(move || { let _ = sys::cmd("notify-send").args(["-a", "Hearth", "-i", &icon, "-t", "8000", &t, &b]).status(); }); }
    #[cfg(windows)]
    { let _ = icon; std::thread::spawn(move || { let _ = tauri_winrt_notification::Toast::new(tauri_winrt_notification::Toast::POWERSHELL_APP_ID).title(&t).text1(&b).show(); }); }
    #[cfg(not(any(target_os = "linux", windows)))]
    { let _ = (icon, t, b); }
}

impl App {
    pub fn new(dir: PathBuf, rt: tokio::runtime::Handle, sink: Sink) -> Arc<App> {
        let _ = std::fs::create_dir_all(&dir);
        let icon_file = dir.join("icon.png");
        if std::fs::metadata(&icon_file).map(|m| m.len() as usize != ICON_PNG.len()).unwrap_or(true) { let _ = std::fs::write(&icon_file, ICON_PNG); }
        let icon = icon_file.to_string_lossy().to_string();
        Arc::new(App {
            s: Mutex::new(load_state(&dir)), dir, relay: Mutex::new(RelayInfo::default()),
            http: net::client(None), long: net::client(Some(300)), events_http: net::client(Some(150)), rt, sink,
            notifier: Mutex::new(Arc::new(move |t, b| platform_notify(&icon, t, b))), icon_file, upd: upd::Upd::default(),
            quitting: AtomicBool::new(false), main_visible: AtomicBool::new(false), main_focused: AtomicBool::new(false),
            listeners: Mutex::new(vec![]), calls: Mutex::new(HashMap::new()), notified_req: Mutex::new(VecDeque::new()),
            pend_seen: Mutex::new(HashSet::new()), last_pend: Mutex::new(json!([])), pair_seen: Mutex::new(HashSet::new()), last_pair_req: Mutex::new(json!([])),
            refreshing: AtomicBool::new(false), voice_job: AtomicBool::new(false), save_gen: AtomicU64::new(0), last_guest_sig: Mutex::new(String::new()),
        })
    }

    pub fn emit(&self, e: Event) { (self.sink)(e); }
    pub fn notify(&self, title: &str, body: &str) { let n = self.notifier.lock().unwrap().clone(); n(title, body); }
    pub fn st(&self) -> Value { self.s.lock().unwrap().clone() }
    pub fn setting(&self, k: &str) -> Value { self.s.lock().unwrap().g("settings").g(k).clone() }
    pub fn setting_all(&self) -> Value { self.s.lock().unwrap().g("settings").clone() }
    pub fn call_me(&self) -> bool { self.setting("callMe").as_bool().unwrap_or(false) }

    /// Saves in 300 ms (many changes in a row make one write).
    pub fn save(self: &Arc<Self>) {
        let g = self.save_gen.fetch_add(1, Ordering::SeqCst) + 1;
        let a = self.clone();
        self.rt.spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            if a.save_gen.load(Ordering::SeqCst) != g { return; }
            a.save_now();
        });
    }
    pub fn save_now(&self) {
        let text = self.s.lock().unwrap().to_string();
        let _ = std::fs::create_dir_all(&self.dir);
        let _ = write_private(&self.dir.join("state.json"), text.as_bytes());
    }

    pub fn machines(&self) -> Vec<Value> {
        let mut list = arr(self.s.lock().unwrap().g("machines"));
        let ri = self.relay.lock().unwrap();
        if ri.ok { list.insert(0, json!({"id": "local", "name": format!("{} (this computer)", hostname()), "host": "127.0.0.1", "port": ri.port, "token": ri.token, "local": true})); }
        else if let Ok(t) = std::fs::read_to_string(home().join(".config/hearth-relay/token")) {
            list.insert(0, json!({"id": "local", "name": format!("{} (this computer)", hostname()), "host": "127.0.0.1", "port": 47601, "token": t.trim(), "local": true}));
        }
        list
    }
    pub fn guest_machines(&self) -> Vec<Value> {
        arr(self.s.lock().unwrap().g("guests")).into_iter().map(|g| { let mut p = guest_pub(&g); p["token"] = g.g("key").clone(); p }).collect()
    }
    pub fn machine(&self, id: &str) -> Option<Value> { self.machines().into_iter().chain(self.guest_machines()).find(|m| m.s("id") == id) }
    pub fn server(&self) -> Option<Value> { self.machines().into_iter().find(|m| m.t("secure")) }
    pub fn local(&self) -> Option<Value> { self.machines().into_iter().find(|m| m.t("local")) }
    pub fn need(&self, id: &str) -> Result<Value, String> { self.machine(id).ok_or_else(|| "Unknown computer".to_string()) }

    pub async fn relay_json(&self, m: &Value, method: &str, p: &str, body: Option<Value>) -> Result<Value, String> { net::json_call(&self.http, m, method, p, body.as_ref()).await }

    /// Closing the window hides it instead of quitting while something still has to run.
    pub fn keep_running(&self) -> bool { self.call_me() || (self.relay.lock().unwrap().ok && self.setting("background") != json!(false)) }
    pub fn public_state(&self) -> Value {
        let s = self.s.lock().unwrap();
        json!({"usage": s.g("usage"), "profile": s.g("profile"), "signedIn": true, "history": s.g("history"), "settings": s.g("settings"), "error": s.g("error"), "now": now_ms() as i64})
    }
    pub fn main_in_front(&self) -> bool { self.main_visible.load(Ordering::SeqCst) && self.main_focused.load(Ordering::SeqCst) }
    pub fn main_hidden(&self) -> bool { !self.main_visible.load(Ordering::SeqCst) }

    // ───────────────────────── this computer's relay, inside the app ─────────────────────────
    /// Starts the relay in this process (unless the always-on relay service already runs here, or `CM_NO_RELAY` is set).
    pub async fn start_relay(self: &Arc<Self>) {
        if std::env::var("CM_NO_RELAY").is_ok() { self.relay.lock().unwrap().in_use = true; return; }
        let o = hearth_relay::relay::Opts {
            dir: self.dir.to_string_lossy().to_string(), port: std::env::var("CM_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(47601), host: "0.0.0.0".into(),
            name: Some(hostname()), public_mode: false, projects_root: None, allow_bypass: true, allow_remote: self.setting("allowRemote").as_bool().unwrap_or(false),
            self_update: false, relay_dir: String::new(), move_root: home().join("projects").to_string_lossy().to_string(), seed_token: String::new(), voice: None,
        };
        let port = o.port;
        let ri = match hearth_relay::boot::embedded(o).await {
            Ok(r) => RelayInfo { ok: true, in_use: false, error: None, port, token: r.token.clone(), r: Some(r) },
            Err((in_use, e)) => RelayInfo { ok: false, in_use, error: Some(e), ..Default::default() },
        };
        *self.relay.lock().unwrap() = ri;
        self.write_link();
    }
    /// Tells the relay which server (if any) to link to. The built-in relay reads it from this app's state; the always-on relay service reads link.json.
    pub fn write_link(&self) {
        self.save_now();
        if !self.relay.lock().unwrap().in_use { return; }
        let text = self.server().map(|s| { let p = s.n("port") as u16; json!({"url": format!("https://{}{}", s.s("host"), if p != 0 && p != 443 { format!(":{p}") } else { String::new() }), "name": s.g("name"), "token": s.g("token")}).to_string() }).unwrap_or_default();
        let _ = write_private(&home().join(".config/hearth-node/link.json"), text.as_bytes());
    }
    /// Runs of this computer's relay end with the app, as they did when the relay was its own process.
    pub fn shutdown(&self) {
        self.quitting.store(true, Ordering::SeqCst); self.save_now();
        if let Some(r) = self.relay.lock().unwrap().r.clone() { for j in r.jobs.lock().unwrap().values() { if !j.is_done() { hearth_relay::jobs::kill_tree(*j.pid.lock().unwrap(), false); } } }
    }

    // ───────────────────────── usage ─────────────────────────
    /// Plan usage comes from your computers' relays, which read it from the real Claude Code. `ask`: read it again first.
    pub async fn refresh_usage(self: &Arc<Self>, ask: bool) {
        if self.refreshing.swap(true, Ordering::SeqCst) { return; }
        let r: Result<(), &str> = async {
            let ms = self.machines();
            if ms.is_empty() { return Err("none"); }
            if ask { let first = ms.iter().find(|m| !m.t("secure")).unwrap_or(&ms[0]).clone(); let _ = self.relay_json(&first, "POST", "/usage/refresh", Some(json!({}))).await; }
            let mut best: Option<Value> = None;
            for m in &ms {
                if let Ok(o) = self.relay_json(m, "GET", "/usage", None).await { if o.n("at") > 0.0 && best.as_ref().map(|b| o.n("at") > b.n("at")).unwrap_or(true) { best = Some(o); } }
            }
            let best = best.ok_or("empty")?;
            let mut notes = vec![];
            {
                let mut s = self.s.lock().unwrap();
                let prev = s.g("usage").clone();
                let u = json!({"session": best.g("session"), "week": best.g("week"), "opus": best.g("opus"), "sonnet": best.g("sonnet"), "extra": null, "at": best.g("at")});
                s["usage"] = u.clone(); s["error"] = Value::Null;
                if prev.is_null() || prev.n("at") != u.n("at") {
                    let at = u.n("at"); let cut = at - 30.0 * 864e5;
                    let mut h: Vec<Value> = arr(s.g("history")).into_iter().filter(|x| x.n("t") >= cut).collect();
                    let sp = now_pct(u.g("session"), at).unwrap_or(0.0); let wp = now_pct(u.g("week"), at).unwrap_or(0.0);
                    let push = match h.last() { None => true, Some(l) => at - l.n("t") > 240000.0 || l.n("s") != sp || l.n("w") != wp };
                    if push { h.push(json!({"t": u.g("at"), "s": hearth_relay::util::num(sp), "w": hearth_relay::util::num(wp)})); }
                    s["history"] = Value::Array(h);
                    notes = alerts(&mut s, &prev, &u);
                }
            }
            for (t, b) in notes { self.notify(&t, &b); }
            self.save();
            Ok(())
        }.await;
        if let Err(e) = r {
            self.s.lock().unwrap()["error"] = match e { "empty" => json!("No usage data yet. Start a Claude Code chat from here, or press Refresh."), "none" => Value::Null, _ => json!("Can't reach your computer right now.") };
        }
        self.refreshing.store(false, Ordering::SeqCst);
        self.emit(Event::State);
    }

    // ───────────────────────── events from the relays ─────────────────────────
    pub fn stop_events(&self) { for h in self.listeners.lock().unwrap().drain(..) { h.abort(); } }
    /// One listener per computer (yours, your server, friends' shared chats). Reconnects by itself: 3 s, doubling to 60 s.
    pub fn start_events(self: &Arc<Self>) {
        self.stop_events();
        let all: Vec<Value> = self.machines().into_iter().chain(self.guest_machines()).collect();
        let mut hs = self.listeners.lock().unwrap();
        for m in all {
            let a = self.clone();
            hs.push(self.rt.spawn(async move {
                let mut wait = 3000u64;
                loop {
                    let cur = a.machine(&m.s("id")).unwrap_or_else(|| m.clone());
                    let away_ok = if a.call_me() && !cur.t("guest") { a.relay_json(&cur, "POST", "/away", Some(json!({"on": true}))).await.is_ok() } else { true };
                    if away_ok {
                        wait = 3000;
                        if let Ok(res) = net::request(&a.events_http, &cur, "GET", "/events", None, None).await {
                            if res.status().as_u16() == 200 {
                                let (a2, c2) = (a.clone(), cur.clone());
                                net::read_records(res, "\n\n", move |block| {
                                    if let Some(dl) = block.split('\n').find(|x| x.starts_with("data: ")) { if let Ok(ev) = serde_json::from_str::<Value>(&dl[6..]) { a2.ring(&c2, ev); } }
                                }).await;
                            }
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(wait)).await;
                    wait = (wait * 2).min(60000);
                }
            }));
        }
    }

    fn notify_event(&self, ev: &Value, force: bool) {
        let st = self.setting_all();
        if let Some((t, b)) = notification_for(ev, &st, self.main_in_front(), force) { self.notify(&t, &b); }
    }
    fn seen_req(&self, req: &str) -> bool {
        let mut q = self.notified_req.lock().unwrap();
        if q.iter().any(|x| x == req) { return true; }
        q.push_back(req.to_string()); if q.len() > 500 { q.pop_front(); }
        false
    }

    /// A small window on top of everything: a friend's request, a support mail, or Claude "calling" you.
    fn popup(self: &Arc<Self>, m: &Value, ev: &Value, spec: PopupSpec) {
        let key = spec.key.clone(); let close_after = spec.close_after_ms;
        self.calls.lock().unwrap().insert(key.clone(), (m.clone(), ev.clone()));
        self.emit(Event::Popup(spec));
        let a = self.clone();
        self.rt.spawn(async move { tokio::time::sleep(Duration::from_millis(close_after)).await; a.close_popup(&key); });
    }
    pub fn close_popup(&self, key: &str) { if self.calls.lock().unwrap().remove(key).is_some() { self.emit(Event::ClosePopup(key.to_string())); } }
    /// The UI layer says the popup window is gone (closed by the user).
    pub fn popup_gone(&self, key: &str) { self.calls.lock().unwrap().remove(key); }

    /// Handles one relay event: tells the chats list, notifies, and opens a popup where it is due.
    pub fn ring(self: &Arc<Self>, m: &Value, ev: Value) {
        self.emit(Event::Relay { machine: m.s("id"), ev: ev.clone() });
        let kind = ev.s("kind");
        if kind == "resolved" { self.close_popups_of(m, &ev); return; }
        if kind == "request" {   // a friend asks to use your account: notification, and a small window when the app is not in front
            if !self.seen_req(&ev.s("req")) {
                self.notify_event(&ev, false);
                if self.setting("requestPopup") != json!(false) && !self.main_in_front() {
                    let from = if ev.s("from").is_empty() { "A friend".into() } else { ev.s("from") };
                    let text = regex::Regex::new(r"^[^:]*:\s*").unwrap().replace(&ev.s("text"), "").to_string();
                    self.popup(m, &ev, PopupSpec { key: format!("{}:req:{}", m.s("id"), ev.s("req")), kind: "request".into(), title: format!("{from} wants to use your Claude"), text, permission: true, own: ev.t("forMember"), height: 330.0, close_after_ms: 120000 });
                }
            }
            return;
        }
        if kind == "support" {
            self.notify_event(&ev, false);
            if !ev.s("req").is_empty() && !self.seen_req(&ev.s("req")) {
                self.popup(m, &ev, PopupSpec { key: format!("{}:support:{}", m.s("id"), ev.s("req")), kind: "support".into(), title: format!("Support mail: {}", ev.s("title")), text: ev.s("text"), permission: true, own: false, height: 330.0, close_after_ms: 600000 });
            }
            return;
        }
        self.notify_event(&ev, false);
        if self.setting("popup") != json!(true) || ev.g("call") == &json!(false) || !self.call_me() { return; }
        self.popup(m, &ev, PopupSpec { key: format!("{}:{}", m.s("id"), ev.s("id")), kind, title: if ev.s("title").is_empty() { "Claude".into() } else { ev.s("title") }, text: ev.s("text"), permission: !ev.s("req").is_empty(), own: false, height: 360.0, close_after_ms: 45000 });
    }
    /// Something was answered elsewhere (the phone, the chat): the popups that asked it have nothing left to ask.
    fn close_popups_of(&self, m: &Value, ev: &Value) {
        let req = ev.s("req"); if req.is_empty() { return; }
        let keys: Vec<String> = self.calls.lock().unwrap().iter().filter(|(_, (cm, cev))| cm.s("id") == m.s("id") && cev.s("req") == req).map(|(k, _)| k.clone()).collect();
        for k in keys { self.close_popup(&k); }
    }

    /// The user pressed a button of a popup (`allow`, `deny`, `talk`, `chat`, `claude`, `list`, `ignore`, or just closed it).
    pub async fn call_action(self: &Arc<Self>, key: &str, action: &str) -> bool {
        let Some((m, ev)) = self.calls.lock().unwrap().get(key).cloned() else { return false };
        if ev.t("forMember") && action == "deny" && !ev.s("req").is_empty() {
            let _ = self.relay_json(&m, "POST", &format!("/sessions/{}/requests/{}", ev.s("session"), ev.s("req")), Some(json!({"decision": "deny"}))).await;
        } else if (action == "allow" || action == "deny" || (ev.s("kind") == "support" && ["claude", "list", "ignore"].contains(&action))) && !ev.s("req").is_empty() {
            let _ = self.relay_json(&m, "POST", "/decision", Some(json!({"req": ev.g("req"), "behavior": action}))).await;
        }
        if action == "talk" || action == "chat" {
            self.emit(Event::ShowMain); self.emit(Event::Goto("sessions".into()));
            self.emit(Event::OpenSession { machine: m.s("id"), session: ev.s("session"), title: ev.s("title"), cwd: ev.s("cwd"), voice: action == "talk" });
        }
        self.close_popup(key);
        true
    }

    // ───────────────────────── periodic checks ─────────────────────────
    /// Projects that appeared on the server while this computer was off: ask before copying them here.
    pub async fn check_pending(self: &Arc<Self>) {
        let Some(m) = self.local() else { return };
        let Ok(st) = self.relay_json(&m, "GET", "/sync/status", None).await else { return };
        let list: Vec<Value> = arr(st.g("pending")).into_iter().map(|mut p| { p["path"] = json!(home().join("projects").join(p.s("name")).to_string_lossy()); p }).collect();
        *self.last_pend.lock().unwrap() = Value::Array(list.clone());
        self.emit(Event::PendingProjects(list.clone()));
        let fresh: Vec<Value> = { let mut seen = self.pend_seen.lock().unwrap(); list.into_iter().filter(|p| seen.insert(p.s("name"))).collect() };
        if !fresh.is_empty() {
            let t = if fresh.len() == 1 { format!("New project: {}", fresh[0].s("name")) } else { format!("{} new projects", fresh.len()) };
            self.notify(&t, &format!("Made on your server while this computer was off. Open Hearth to install {}.", if fresh.len() == 1 { "it" } else { "them" }));
        }
    }
    /// A phone on your Wi-Fi asks to connect: tell the person, let them accept or decline.
    pub async fn check_pair_requests(self: &Arc<Self>) {
        let Some(m) = self.local() else { return };
        let Ok(list) = self.relay_json(&m, "GET", "/pair/requests", None).await else { return };
        *self.last_pair_req.lock().unwrap() = list.clone();
        self.emit(Event::PairRequests(list.clone()));
        for r in arr(&list) {
            if !self.pair_seen.lock().unwrap().insert(r.s("id")) { continue; }
            if self.setting("pairNotify") != json!(false) {
                self.notify(&format!("{} wants to connect", r.s("name")), "A phone on your Wi-Fi is asking to use Hearth with this computer. Open Hearth to accept or decline.");
                if self.main_hidden() { self.emit(Event::ShowMain); }
            }
        }
    }

    /// Starts the timers: update check, pairing requests, pending projects, usage (every 30 s), the system theme (every 30 s).
    pub fn start_timers(self: &Arc<Self>) {
        fn every<F, Fut>(a: &Arc<App>, first: u64, ms: u64, f: F) where F: Fn(Arc<App>) -> Fut + Send + 'static, Fut: std::future::Future<Output = ()> + Send + 'static {
            let a = a.clone();
            a.clone().rt.spawn(async move { tokio::time::sleep(Duration::from_millis(first)).await; loop { f(a.clone()).await; tokio::time::sleep(Duration::from_millis(ms)).await; } });
        }
        every(self, 6000, 120000, |a| async move { let u = a.upd_check(false).await; let _ = u; });
        every(self, 2500, 2500, |a| async move { a.check_pair_requests().await });
        every(self, 8000, 20000, |a| async move { a.check_pending().await });
        every(self, 30000, 30000, |a| async move {
            a.refresh_usage(false).await;
            // keep the numbers live while you look at them
            let stale = { let s = a.s.lock().unwrap(); s.g("usage").is_null() || now_ms() - s.g("usage").n("at") > 120000.0 };
            if stale && a.main_in_front() { a.refresh_usage(true).await; }
        });
        every(self, 30000, 30000, |a| async move { a.emit(Event::ThemeChanged); });
        let a = self.clone(); self.rt.spawn(async move { a.refresh_usage(false).await; });
        self.start_events();
    }
    /// The window got focus: look for an update, and read usage again when it is stale.
    pub fn on_focus(self: &Arc<Self>) {
        let a = self.clone();
        self.rt.spawn(async move {
            a.upd_check(false).await;
            let stale = { let s = a.s.lock().unwrap(); s.g("usage").is_null() || now_ms() - s.g("usage").n("at") > 60000.0 };
            if stale { a.refresh_usage(true).await; }
        });
    }
    pub async fn upd_check(self: &Arc<Self>, force: bool) { upd::check(self, force).await }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn st() -> Value { let mut s = defaults(); s["settings"]["thresholds"] = json!([75, 90, 100]); s }
    #[test] fn dur_formats() { assert_eq!(dur(45.0 * 60000.0), "45m"); assert_eq!(dur(125.0 * 60000.0), "2h 5m"); assert_eq!(dur(3000.0 * 60000.0), "2d 2h"); assert_eq!(dur(-5.0), "0m"); }
    #[test] fn alerts_once_per_window() {
        let mut s = st();
        let u = json!({"session": {"pct": 80, "resetsAt": 36e5 * 100.0 + 5.0 * 60000.0}, "at": 36e5 * 100.0});
        let a = alerts(&mut s, &Value::Null, &u);
        assert_eq!(a.len(), 1); assert_eq!(a[0].0, "Session (5h) at 80%"); assert!(a[0].1.starts_with("Resets in 5m"));
        assert!(alerts(&mut s, &u, &u).is_empty(), "same window, same threshold: silent");
        let u2 = json!({"session": {"pct": 95, "resetsAt": 36e5 * 100.0 + 5.0 * 60000.0}, "at": 36e5 * 100.0 + 1.0});
        assert_eq!(alerts(&mut s, &u, &u2)[0].0, "Session (5h) at 95%");
        let u3 = json!({"session": {"pct": 100, "resetsAt": 36e5 * 100.0 + 5.0 * 60000.0}, "at": 36e5 * 100.0 + 2.0});
        assert_eq!(alerts(&mut s, &u2, &u3)[0].0, "Session (5h) limit reached");
        // a new window: crossing 75 again counts, and the drop is a "reset"
        let u4 = json!({"session": {"pct": 5, "resetsAt": 36e5 * 105.0}, "at": 36e5 * 101.0});
        assert_eq!(alerts(&mut s, &u3, &u4)[0].0, "Session reset");
    }
    #[test] fn alerts_reset_can_be_off() { let mut s = st(); s["settings"]["notifyReset"] = json!(false); let p = json!({"session": {"pct": 90}}); let u = json!({"session": {"pct": 1, "resetsAt": 0}, "at": 1}); assert!(alerts(&mut s, &p, &u).is_empty()); }
    #[test] fn now_pct_after_reset() { assert_eq!(now_pct(&json!({"pct": 80, "resetsAt": 10}), 20.0), Some(0.0)); assert_eq!(now_pct(&json!({"pct": 80, "resetsAt": 30}), 20.0), Some(80.0)); assert_eq!(now_pct(&Value::Null, 1.0), None); }
    #[test] fn notification_kinds() {
        let s = json!({"notifyDone": false});
        let done = json!({"kind": "done", "cwd": "/home/a/proj/", "text": "all set"});
        assert!(notification_for(&done, &s, false, false).is_none(), "switched off");
        assert_eq!(notification_for(&done, &s, false, true).unwrap(), ("proj: Claude is done".into(), "all set".into()));
        let perm = json!({"kind": "permission", "cwd": "C:\\work\\app", "title": "Run ls"});
        assert_eq!(notification_for(&perm, &json!({}), false, false).unwrap(), ("app: needs your OK".into(), "Run ls".into()));
        assert!(notification_for(&perm, &json!({}), true, false).is_none(), "you are looking at the app");
        assert!(notification_for(&perm, &json!({"notifyFocused": true}), true, false).is_some());
        assert!(notification_for(&json!({"kind": "resolved"}), &json!({}), false, false).is_none());
        assert_eq!(notification_for(&json!({"kind": "request", "from": "Sam"}), &json!({}), false, false).unwrap().0, "Sam wants to use your Claude");
    }
    #[test] fn state_loading_keeps_old_users() {
        let d = tempfile::tempdir().unwrap();
        assert!(load_state(d.path()).g("settings").get("onboarded").is_none());
        std::fs::write(d.path().join("state.json"), r#"{"settings":{"theme":"dark"},"machines":[{"id":"x"}],"tok":"old"}"#).unwrap();
        let s = load_state(d.path());
        assert_eq!(s.g("settings").s("theme"), "dark"); assert!(s.g("settings").t("onboarded")); assert_eq!(s.g("settings").n("interval"), 5.0); assert!(s.get("tok").is_none());
    }
    #[test] fn guests_become_machines() { let g = json!({"share": "ab12", "key": "share.k", "hostname": "h.example", "port": 443, "secure": true, "host": "Sam", "title": ""}); let p = guest_pub(&g); assert_eq!(p.s("id"), "g:ab12"); assert_eq!(p.s("name"), "Sam: shared chat"); assert!(strip_token(&json!({"token": "x", "id": 1})).get("token").is_none()); }
}
