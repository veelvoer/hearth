//! Hearth for Windows and Linux, in Rust. The window shows the same pages as before (desktop/renderer); everything behind them
//! (this computer's relay, your server, notifications, the tray, updates) is here. The relay runs inside the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod net;
mod sys;
mod upd;

use base64::Engine;
use hearth_relay::relay::Relay;
use hearth_relay::util::{arr, home, hostname, now_ms, rand_hex, read_json, write_private, J};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

const SHIM: &str = include_str!("shim.js");
const ICON_PNG: &[u8] = include_bytes!("../icons/icon.png");
const TRAY_PNG: &[u8] = include_bytes!("../icons/tray.png");
const BG: tauri::window::Color = tauri::window::Color(0x1b, 0x1a, 0x19, 0xff);

#[derive(Default)]
pub struct RelayInfo { pub ok: bool, pub in_use: bool, pub error: Option<String>, pub port: u16, pub token: String, pub r: Option<Arc<Relay>> }

pub struct App {
    pub s: Mutex<Value>,
    pub dir: PathBuf,
    pub relay: Mutex<RelayInfo>,
    pub http: reqwest::Client,
    pub long: reqwest::Client,
    pub events_http: reqwest::Client,
    listeners: Mutex<Vec<tauri::async_runtime::JoinHandle<()>>>,
    calls: Mutex<HashMap<String, (String, Value, Value)>>,   // key -> (window label, computer, event)
    notified_req: Mutex<VecDeque<String>>,
    pend_seen: Mutex<HashSet<String>>, last_pend: Mutex<Value>,
    pair_seen: Mutex<HashSet<String>>, last_pair_req: Mutex<Value>,
    refreshing: AtomicBool, pub quitting: AtomicBool, voice_job: AtomicBool,
    save_gen: AtomicU64, popups: AtomicU64,
    last_guest_sig: Mutex<String>,
    pub icon_file: PathBuf,
    pub upd: upd::Upd,
    tray_ring: Mutex<Option<tauri::menu::CheckMenuItem<tauri::Wry>>>,
    tray: Mutex<Option<tauri::tray::TrayIcon>>,
}

static APP: OnceLock<Arc<App>> = OnceLock::new();
static HANDLE: OnceLock<AppHandle> = OnceLock::new();
pub fn app() -> &'static Arc<App> { APP.get().expect("app") }
pub fn handle() -> &'static AppHandle { HANDLE.get().expect("handle") }

/// Where the app keeps its settings: the same folder the Electron app used, so nothing is lost when moving over.
fn data_dir() -> PathBuf {
    if cfg!(windows) { PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("Hearth") }
    else if cfg!(target_os = "macos") { home().join("Library/Application Support/Hearth") }
    else { std::env::var("XDG_CONFIG_HOME").ok().filter(|s| !s.is_empty()).map(PathBuf::from).unwrap_or_else(|| home().join(".config")).join("Hearth") }
}

fn defaults() -> Value {
    json!({"settings": {"interval": 5, "thresholds": [75, 90, 100], "notifyReset": true, "callMe": false, "theme": "system", "palette": "claude", "background": true, "autostart": false},
           "machines": [], "deployed": {}, "history": [], "usage": null, "profile": null, "tok": null, "ln": {}, "error": null})
}

fn load_state(dir: &PathBuf) -> Value {
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

impl App {
    pub fn st(&self) -> Value { self.s.lock().unwrap().clone() }
    pub fn setting(&self, k: &str) -> Value { self.s.lock().unwrap().g("settings").g(k).clone() }
    pub fn save(&self) {
        let g = self.save_gen.fetch_add(1, Ordering::SeqCst) + 1;
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            let a = app();
            if a.save_gen.load(Ordering::SeqCst) != g { return; }
            let text = a.s.lock().unwrap().to_string();
            let _ = std::fs::create_dir_all(&a.dir);
            let _ = write_private(&a.dir.join("state.json"), text.as_bytes());
        });
    }
    pub fn save_now(&self) { let text = self.s.lock().unwrap().to_string(); let _ = write_private(&self.dir.join("state.json"), text.as_bytes()); }

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
        arr(self.s.lock().unwrap().g("guests")).into_iter().map(|g| {
            let mut p = guest_pub(&g); p["token"] = g.g("key").clone(); p
        }).collect()
    }
    pub fn machine(&self, id: &str) -> Option<Value> { self.machines().into_iter().chain(self.guest_machines()).find(|m| m.s("id") == id) }
    pub fn server(&self) -> Option<Value> { self.machines().into_iter().find(|m| m.t("secure")) }
    pub fn local(&self) -> Option<Value> { self.machines().into_iter().find(|m| m.t("local")) }

    pub async fn relay_json(&self, m: &Value, method: &str, p: &str, body: Option<Value>) -> Result<Value, String> { net::json_call(&self.http, m, method, p, body.as_ref()).await }

    fn keep_running(&self) -> bool { self.setting("callMe").as_bool().unwrap_or(false) || (self.relay.lock().unwrap().ok && self.setting("background") != json!(false)) }
    pub fn public_state(&self) -> Value {
        let s = self.s.lock().unwrap();
        json!({"usage": s.g("usage"), "profile": s.g("profile"), "signedIn": true, "history": s.g("history"), "settings": s.g("settings"), "error": s.g("error"), "now": now_ms() as i64})
    }
}

fn guest_pub(g: &Value) -> Value {
    let host = if g.s("host").is_empty() { "A friend".to_string() } else { g.s("host") };
    let title = if g.s("title").is_empty() { "shared chat".to_string() } else { g.s("title") };
    json!({"id": format!("g:{}", g.s("share")), "name": format!("{host}: {title}"), "host": g.g("hostname"), "port": g.g("port"), "local": false, "secure": g.t("secure"), "guest": true, "hostName": g.s("host"), "share": g.g("share"), "title": g.s("title")})
}
fn pub_m(m: &Value) -> Value { json!({"id": m.g("id"), "name": m.g("name"), "host": m.g("host"), "port": m.g("port"), "local": m.t("local"), "secure": m.t("secure")}) }
fn strip_token(m: &Value) -> Value { let mut m = m.clone(); if let Some(o) = m.as_object_mut() { o.remove("token"); } m }

pub fn emit(ch: &str, data: Value) { let _ = handle().emit(ch, data); }
fn main_win() -> Option<WebviewWindow> { handle().get_webview_window("main") }
fn main_in_front() -> bool { main_win().map(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false)).unwrap_or(false) }
pub fn main_hidden() -> bool { main_win().map(|w| !w.is_visible().unwrap_or(false)).unwrap_or(true) }

// ───────────────────────── notifications ─────────────────────────
/// A desktop notification that can never freeze the app: on Linux it is handed to notify-send in its own process.
pub fn notify(title: &str, body: &str) {
    let t: String = title.chars().take(120).collect(); let b: String = body.chars().take(300).collect();
    if cfg!(target_os = "linux") {
        let icon = app().icon_file.to_string_lossy().to_string();
        std::thread::spawn(move || { let _ = sys::cmd("notify-send").args(["-a", "Hearth", "-i", &icon, "-t", "8000", &t, &b]).status(); });
        return;
    }
    use tauri_plugin_notification::NotificationExt;
    let _ = handle().notification().builder().title(t).body(b).show();
}

fn dur(ms: f64) -> String {
    let m = (ms / 60000.0).floor().max(0.0) as i64;
    if m < 60 { format!("{m}m") } else if m < 1440 { format!("{}h {}m", m / 60, m % 60) } else { format!("{}d {}h", m / 1440, (m % 1440) / 60) }
}
fn now_pct(l: &Value, t: f64) -> Option<f64> { if l.is_null() { None } else if l.n("resetsAt") > 0.0 && l.n("resetsAt") < t { Some(0.0) } else { Some(l.n("pct")) } }

fn check_alerts(s: &mut Value, prev: &Value, u: &Value) {
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
        notify(&title, &if l.n("resetsAt") > 0.0 { format!("Resets in {}", dur(l.n("resetsAt") - u.n("at"))) } else { format!("Crossed {hit}%") });
    }
    let (ps, ns) = (prev.g("session"), u.g("session"));
    if s.g("settings").t("notifyReset") && !ps.is_null() && !ns.is_null() && ps.n("pct") >= 75.0 && ns.n("pct") < ps.n("pct") - 30.0 { notify("Session reset", "A fresh 5-hour window is ready."); }
}

/// Plan usage comes from your computers' relays, which read it from the real Claude Code. [ask]: read it again first.
async fn refresh_usage(ask: bool) {
    let a = app();
    if a.refreshing.swap(true, Ordering::SeqCst) { return; }
    let r: Result<(), &str> = async {
        let ms = a.machines();
        if ms.is_empty() { return Err("none"); }
        if ask { let first = ms.iter().find(|m| !m.t("secure")).unwrap_or(&ms[0]).clone(); let _ = a.relay_json(&first, "POST", "/usage/refresh", Some(json!({}))).await; }
        let mut best: Option<Value> = None;
        for m in &ms {
            if let Ok(o) = a.relay_json(m, "GET", "/usage", None).await { if o.n("at") > 0.0 && best.as_ref().map(|b| o.n("at") > b.n("at")).unwrap_or(true) { best = Some(o); } }
        }
        let best = best.ok_or("empty")?;
        let mut s = a.s.lock().unwrap();
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
            check_alerts(&mut s, &prev, &u);
        }
        drop(s); a.save();
        Ok(())
    }.await;
    if let Err(e) = r {
        a.s.lock().unwrap()["error"] = match e { "empty" => json!("No usage data yet. Start a Claude Code chat from here, or press Refresh."), "none" => Value::Null, _ => json!("Can't reach your computer right now.") };
    }
    a.refreshing.store(false, Ordering::SeqCst);
    emit("usage", a.public_state());
}

// ───────────────────────── events from the relays ─────────────────────────
fn stop_events() { for h in app().listeners.lock().unwrap().drain(..) { h.abort(); } }
fn start_events() {
    stop_events();
    let a = app();
    let all: Vec<Value> = a.machines().into_iter().chain(a.guest_machines()).collect();
    let mut hs = a.listeners.lock().unwrap();
    for m in all {
        hs.push(tauri::async_runtime::spawn(async move {
            let mut wait = 3000u64;
            loop {
                let a = app();
                let cur = a.machine(&m.s("id")).unwrap_or_else(|| m.clone());
                let away_ok = if a.setting("callMe").as_bool().unwrap_or(false) && !cur.t("guest") { a.relay_json(&cur, "POST", "/away", Some(json!({"on": true}))).await.is_ok() } else { true };
                if away_ok {
                    wait = 3000;
                    if let Ok(res) = net::request(&a.events_http, &cur, "GET", "/events", None, None).await {
                        if res.status().as_u16() == 200 {
                            let c2 = cur.clone();
                            net::read_records(res, "\n\n", move |block| {
                                if let Some(dl) = block.split('\n').find(|x| x.starts_with("data: ")) { if let Ok(ev) = serde_json::from_str::<Value>(&dl[6..]) { ring(&c2, ev); } }
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

/// A notification for what happens in a chat. Which kinds you get, and whether you get them while the app is open, is up to you.
fn notify_event(ev: &Value, force: bool) {
    let a = app(); let st = a.st()["settings"].clone();
    let not_false = |k: &str| st.g(k) != &json!(false);
    let on = match ev.s("kind").as_str() { "request" => Some(not_false("notifyRequest")), "permission" | "limit" => Some(not_false("notifyPerm")), "question" => Some(not_false("notifyQuestion")), "done" | "error" => Some(not_false("notifyDone")), "support" => Some(true), _ => None };
    if !force {
        if on != Some(true) { return; }
        if st.g("notifyFocused") != &json!(true) && main_in_front() { return; }   // you are looking at the app: it shows this itself
    }
    let cwd = ev.s("cwd"); let proj = cwd.trim_end_matches('/').rsplit(['/', '\\']).next().filter(|s| !s.is_empty()).unwrap_or("Claude").to_string();
    let title = match ev.s("kind").as_str() {
        "request" => format!("{} wants to use your Claude", if ev.s("from").is_empty() { "A friend".into() } else { ev.s("from") }),
        "support" => format!("Support mail: {}", if ev.s("title").is_empty() { "new message".into() } else { ev.s("title") }),
        "limit" => format!("{proj}: usage limit reached"), "permission" => format!("{proj}: needs your OK"), "question" => format!("{proj}: Claude has a question"),
        "error" => format!("{proj}: something went wrong"), _ => format!("{proj}: Claude is done"),
    };
    notify(&title, &if ev.s("text").is_empty() { ev.s("title") } else { ev.s("text") });
}

fn seen_req(req: &str) -> bool {
    let mut q = app().notified_req.lock().unwrap();
    if q.iter().any(|x| x == req) { return true; }
    q.push_back(req.to_string()); if q.len() > 500 { q.pop_front(); }
    false
}

/// A small window on top of everything: a friend's request, a support mail, or Claude "calling" you.
fn popup(m: &Value, ev: &Value, key: String, q: Vec<(&str, String)>, height: f64, close_after: u64) {
    let n = app().popups.fetch_add(1, Ordering::SeqCst);
    let label = format!("call-{n}");
    app().calls.lock().unwrap().insert(key.clone(), (label.clone(), m.clone(), ev.clone()));
    let mut all = vec![("key", key.clone())]; all.extend(q);
    let url = format!("call.html?{}", hearth_relay::util::qbuild(&all));
    let h = handle().clone();
    let _ = h.clone().run_on_main_thread(move || {
        let (mut x, y) = (900.0, 40.0);
        if let Ok(Some(mon)) = h.primary_monitor() { let sf = mon.scale_factor(); x = mon.size().width as f64 / sf - 400.0; }
        let w = WebviewWindowBuilder::new(&h, &label, WebviewUrl::App(url.into())).title("Claude is calling").inner_size(380.0, height).position(x, y)
            .decorations(false).always_on_top(true).resizable(false).skip_taskbar(true).background_color(BG).initialization_script(SHIM).disable_drag_drop_handler().build();
        if let Ok(w) = w {
            let k2 = key.clone();
            w.on_window_event(move |e| { if let tauri::WindowEvent::Destroyed = e { app().calls.lock().unwrap().remove(&k2); } });
            let label = label.clone();
            tauri::async_runtime::spawn(async move { tokio::time::sleep(Duration::from_millis(close_after)).await; if let Some(w) = handle().get_webview_window(&label) { let _ = w.close(); } });
        }
    });
}

fn ring(m: &Value, ev: Value) {
    let a = app();
    emit("event", json!({"machine": m.g("id"), "ev": ev}));
    let kind = ev.s("kind");
    if kind == "resolved" { return; }
    if kind == "request" {   // a friend asks to use your account: notification, and a small window when the app is not in front
        if !seen_req(&ev.s("req")) {
            notify_event(&ev, false);
            if a.setting("requestPopup") != json!(false) && !main_in_front() {
                let from = if ev.s("from").is_empty() { "A friend".into() } else { ev.s("from") };
                let text = regex::Regex::new(r"^[^:]*:\s*").unwrap().replace(&ev.s("text"), "").to_string();
                popup(m, &ev, format!("{}:req:{}", m.s("id"), ev.s("req")), vec![("kind", "request".into()), ("title", format!("{from} wants to use your Claude")), ("text", text), ("permission", "1".into()), ("own", if ev.t("forMember") { "1".into() } else { String::new() })], 330.0, 120000);
            }
        }
        return;
    }
    if kind == "support" {
        notify_event(&ev, false);
        if !ev.s("req").is_empty() && !seen_req(&ev.s("req")) {
            popup(m, &ev, format!("{}:support:{}", m.s("id"), ev.s("req")), vec![("kind", "support".into()), ("title", format!("Support mail: {}", ev.s("title"))), ("text", ev.s("text")), ("permission", "1".into())], 330.0, 600000);
        }
        return;
    }
    notify_event(&ev, false);
    if a.setting("popup") != json!(true) || ev.g("call") == &json!(false) || !a.setting("callMe").as_bool().unwrap_or(false) { return; }
    popup(m, &ev, format!("{}:{}", m.s("id"), ev.s("id")), vec![("kind", kind.clone()), ("title", if ev.s("title").is_empty() { "Claude".into() } else { ev.s("title") }), ("text", ev.s("text")), ("permission", if ev.s("req").is_empty() { String::new() } else { "1".into() })], 360.0, 45000);
}

// ───────────────────────── windows, tray, theme ─────────────────────────
pub fn show_main(tab: Option<&str>) {
    let tab = tab.map(String::from);
    let h = handle().clone();
    let _ = h.clone().run_on_main_thread(move || {
        let w = match h.get_webview_window("main") { Some(w) => w, None => match create_main(&h, false) { Some(w) => w, None => return } };
        let _ = w.unminimize(); let _ = w.show(); let _ = w.set_focus();
        if let Some(t) = tab { let _ = w.emit("goto", t); }
    });
}

fn create_main(h: &AppHandle, hidden: bool) -> Option<WebviewWindow> {
    let w = WebviewWindowBuilder::new(h, "main", WebviewUrl::App("index.html".into())).title("Hearth").inner_size(1120.0, 780.0).min_inner_size(820.0, 560.0)
        .visible(!hidden).background_color(BG).initialization_script(SHIM).disable_drag_drop_handler().build().ok()?;
    if let Ok(img) = tauri::image::Image::from_bytes(ICON_PNG) { let _ = w.set_icon(img); }
    let w2 = w.clone();
    w.on_window_event(move |e| match e {
        tauri::WindowEvent::CloseRequested { api, .. } => {
            if app().quitting.load(Ordering::SeqCst) { return; }
            if app().keep_running() { api.prevent_close(); let _ = w2.hide(); }
        }
        tauri::WindowEvent::Focused(true) => {
            tauri::async_runtime::spawn(async { app().upd.check(false).await; });
            let stale = { let s = app().s.lock().unwrap(); s.g("usage").is_null() || now_ms() - s.g("usage").n("at") > 60000.0 };
            if stale { tauri::async_runtime::spawn(refresh_usage(true)); }
        }
        _ => {}
    });
    apply_theme();
    Some(w)
}

/// Light, dark or following the desktop, also where the desktop does not tell the window.
fn apply_theme() {
    let theme = app().setting("theme").as_str().unwrap_or("system").to_string();
    let set = |t: Option<tauri::Theme>| { for (_, w) in handle().webview_windows() { let _ = w.set_theme(t); } };
    match theme.as_str() {
        "dark" => set(Some(tauri::Theme::Dark)),
        "light" => set(Some(tauri::Theme::Light)),
        _ => {
            tauri::async_runtime::spawn(async move {
                let d = sys::dark().await;
                let t = d.map(|d| if d { tauri::Theme::Dark } else { tauri::Theme::Light });
                let h = handle().clone();
                let _ = h.clone().run_on_main_thread(move || { for (_, w) in h.webview_windows() { let _ = w.set_theme(t); } });
            });
        }
    }
}

fn lib_present(names: &[&str]) -> bool {
    if !cfg!(target_os = "linux") { return true; }
    let dirs = ["/usr/lib64", "/usr/lib", "/usr/lib/x86_64-linux-gnu", "/usr/lib/aarch64-linux-gnu", "/lib/x86_64-linux-gnu", "/lib64"];
    let appdir = std::env::var("APPDIR").unwrap_or_default();
    names.iter().any(|n| dirs.iter().any(|d| std::path::Path::new(d).join(n).exists()) || (!appdir.is_empty() && std::path::Path::new(&appdir).join("usr/lib").join(n).exists()))
}

fn build_tray(h: &AppHandle) -> Result<(), String> {
    use tauri::menu::{CheckMenuItem, MenuBuilder, MenuItem, PredefinedMenuItem};
    use tauri::tray::TrayIconBuilder;
    // the tray needs the indicator library on Linux; without it there is simply no tray icon
    if !lib_present(&["libayatana-appindicator3.so.1", "libappindicator3.so.1"]) { return Err("no indicator library".into()); }
    let e = |x: tauri::Error| x.to_string();
    let open = MenuItem::with_id(h, "open", "Open Hearth", true, None::<&str>).map_err(e)?;
    let ring = CheckMenuItem::with_id(h, "ring", "Ring me on this computer", true, app().setting("callMe").as_bool().unwrap_or(false), None::<&str>).map_err(e)?;
    let quit = MenuItem::with_id(h, "quit", "Quit", true, None::<&str>).map_err(e)?;
    let menu = MenuBuilder::new(h).item(&open).item(&ring).item(&PredefinedMenuItem::separator(h).map_err(e)?).item(&quit).build().map_err(e)?;
    let img = tauri::image::Image::from_bytes(TRAY_PNG).map_err(e)?;
    let tray = TrayIconBuilder::with_id("main").icon(img).tooltip("Hearth").menu(&menu).show_menu_on_left_click(false)
        .on_menu_event(|h, ev| match ev.id().as_ref() {
            "open" => show_main(None),
            "ring" => { let on = !app().setting("callMe").as_bool().unwrap_or(false); set_call_me(on); }
            "quit" => { app().quitting.store(true, Ordering::SeqCst); h.exit(0); }
            _ => {}
        })
        .on_tray_icon_event(|_, ev| { if let tauri::tray::TrayIconEvent::Click { button: tauri::tray::MouseButton::Left, button_state: tauri::tray::MouseButtonState::Up, .. } = ev { show_main(None); } })
        .build(h).map_err(e)?;
    *app().tray_ring.lock().unwrap() = Some(ring);
    *app().tray.lock().unwrap() = Some(tray);
    Ok(())
}

fn set_call_me(on: bool) {
    let a = app();
    a.s.lock().unwrap()["settings"]["callMe"] = json!(on); a.save();
    if let Some(r) = a.tray_ring.lock().unwrap().as_ref() { let _ = r.set_checked(on); }
    start_events();   // messages keep arriving either way; this only controls ringing on this computer
    if !on { for m in a.machines() { tauri::async_runtime::spawn(async move { let _ = app().relay_json(&m, "POST", "/away", Some(json!({"on": false}))).await; }); } }
    emit("usage", a.public_state());
}

// ───────────────────────── periodic checks ─────────────────────────
/// Projects that appeared on the server while this computer was off: ask before copying them here.
async fn check_pending() {
    let a = app(); let Some(m) = a.local() else { return };
    let Ok(st) = a.relay_json(&m, "GET", "/sync/status", None).await else { return };
    let list: Vec<Value> = arr(st.g("pending")).into_iter().map(|mut p| { p["path"] = json!(home().join("projects").join(p.s("name")).to_string_lossy()); p }).collect();
    *a.last_pend.lock().unwrap() = Value::Array(list.clone());
    emit("pending-projects", Value::Array(list.clone()));
    let fresh: Vec<Value> = { let mut seen = a.pend_seen.lock().unwrap(); list.into_iter().filter(|p| seen.insert(p.s("name"))).collect() };
    if !fresh.is_empty() {
        let t = if fresh.len() == 1 { format!("New project: {}", fresh[0].s("name")) } else { format!("{} new projects", fresh.len()) };
        notify(&t, &format!("Made on your server while this computer was off. Open Hearth to install {}.", if fresh.len() == 1 { "it" } else { "them" }));
    }
}
/// A phone on your Wi-Fi asks to connect: tell the person, let them accept or decline.
async fn check_pair_requests() {
    let a = app(); let Some(m) = a.local() else { return };
    let Ok(list) = a.relay_json(&m, "GET", "/pair/requests", None).await else { return };
    *a.last_pair_req.lock().unwrap() = list.clone();
    emit("pair-requests", list.clone());
    for r in arr(&list) {
        if !a.pair_seen.lock().unwrap().insert(r.s("id")) { continue; }
        if a.setting("pairNotify") != json!(false) {
            notify(&format!("{} wants to connect", r.s("name")), "A phone on your Wi-Fi is asking to use Hearth with this computer. Open Hearth to accept or decline.");
            if main_hidden() { show_main(None); }
        }
    }
}

fn every(first: u64, ms: u64, f: fn() -> tauri::async_runtime::JoinHandle<()>) {
    tauri::async_runtime::spawn(async move { tokio::time::sleep(Duration::from_millis(first)).await; loop { let _ = f().await; tokio::time::sleep(Duration::from_millis(ms)).await; } });
}

// ───────────────────────── the calls of the pages ─────────────────────────
fn arg(a: &[Value], i: usize) -> Value { a.get(i).cloned().unwrap_or(Value::Null) }
fn sarg(a: &[Value], i: usize) -> String { hearth_relay::util::vstr(&arg(a, i)) }
fn need(id: &str) -> Result<Value, String> { app().machine(id).ok_or_else(|| "Unknown computer".to_string()) }

mod notes { include!(concat!(env!("OUT_DIR"), "/notes.rs")); }
fn ver_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let p = |v: &str| v.split('.').map(|x| x.parse::<i64>().unwrap_or(0)).collect::<Vec<_>>();
    let (x, y) = (p(a), p(b));
    for i in 0..3 { let c = x.get(i).unwrap_or(&0).cmp(y.get(i).unwrap_or(&0)); if c != std::cmp::Ordering::Equal { return c; } }
    std::cmp::Ordering::Equal
}
pub fn version() -> String { handle().package_info().version.to_string() }

const SUPPORT_URL_DEFAULT: &str = "https://cm.vpswb.store/support";
async fn support_call(method: &str, p: &str, body: Option<Value>, auth: bool) -> Result<Value, String> {
    let a = app();
    let url = std::env::var("HEARTH_SUPPORT_URL").unwrap_or_else(|_| SUPPORT_URL_DEFAULT.into()) + p;
    let mut rq = a.http.request(reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET), url).header("Content-Type", "application/json").timeout(Duration::from_secs(20));
    let tok = a.setting("supportToken").as_str().unwrap_or("").to_string();
    if auth && !tok.is_empty() { rq = rq.bearer_auth(tok); }
    if let Some(b) = body { rq = rq.body(b.to_string()); }
    let r = rq.send().await.map_err(|_| "Could not reach support. Check your internet connection and try again.".to_string())?;
    let st = r.status().as_u16();
    let j: Value = r.json().await.unwrap_or_else(|_| json!({}));
    if st == 401 && auth { let mut s = a.s.lock().unwrap(); s["settings"]["supportToken"] = json!(""); s["settings"]["supportEmail"] = json!(""); drop(s); a.save(); }
    if !(200..300).contains(&st) { return Err(if j.s("error").is_empty() { format!("Support said {st}") } else { j.s("error") }); }
    Ok(j)
}

async fn export_import(from: &Value, to: &Value, from_cwd: &str, to_cwd: &str, read_err: &str, copy_err: &str) -> Result<(), String> {
    let a = app(); let enc = hearth_relay::util::enc;
    let res = net::request(&a.long, from, "GET", &format!("/projects/export?cwd={}", enc(from_cwd)), None, Some(120000)).await?;
    if res.status().as_u16() != 200 { return Err(read_err.into()); }
    let buf = res.bytes().await.map_err(|e| net::err_text(&e))?.to_vec();
    let imp = net::request(&a.long, to, "POST", &format!("/projects/import?cwd={}&from={}", enc(to_cwd), enc(from_cwd)), Some((buf, "application/gzip")), Some(600000)).await?;
    let st = imp.status().as_u16(); let ib = imp.bytes().await.unwrap_or_default();
    if st != 200 { return Err(net::err_of(&ib).unwrap_or_else(|| copy_err.into())); }
    Ok(())
}

async fn newest_session(m: &Value, cwd: &str) -> Value {
    let list = app().relay_json(m, "GET", "/sessions", None).await.unwrap_or(json!([]));
    arr(&list).into_iter().filter(|s| s.s("cwd") == cwd).max_by(|a, b| a.n("mtime").partial_cmp(&b.n("mtime")).unwrap()).unwrap_or(Value::Null)
}

/// Asks a relay to run a message and passes every event of the run to the page; when the connection drops, joins the run again where it left off.
fn send(m: Value, session: String, text: Value, mode: Value, voice: bool, sid: String, extra: Value) {
    tauri::async_runtime::spawn(async move {
        let a = app();
        let out = |o: Value| { let mut o = o; o["sid"] = json!(sid.clone()); emit("stream", o); };
        let state = Arc::new(Mutex::new((-1.0f64, false, session.clone())));   // last event number, finished, chat id
        let take = { let state = state.clone(); let sid = sid.clone(); move |o: Value| {
            let mut s = state.lock().unwrap();
            if let Some(n) = o.get("n").and_then(|n| n.as_f64()) { if n <= s.0 { return; } s.0 = n; }
            if o.s("t") == "session" && !o.s("id").is_empty() { s.2 = o.s("id"); }
            if o.s("t") == "end" || o.s("t") == "fail" { s.1 = true; }
            drop(s);
            let mut o = o; o["sid"] = json!(sid.clone()); emit("stream", o);
        } };
        let mut body = json!({"text": text, "mode": mode, "brief": voice});
        if let Some(x) = extra.as_object() { for (k, v) in x { if !v.is_null() { body[k] = v.clone(); } } }
        let path = format!("/sessions/{}/send", if session.is_empty() { "new" } else { &session });
        let first = match net::request(&a.long, &m, "POST", &path, Some((body.to_string().into_bytes(), "application/json")), None).await {
            Ok(r) => r, Err(e) => return out(json!({"t": "fail", "text": e})),
        };
        if first.status().as_u16() != 200 {
            let st = first.status().as_u16(); let b = first.bytes().await.unwrap_or_default();
            return out(json!({"t": "fail", "text": net::err_of(&b).unwrap_or_else(|| format!("Relay error {st}"))}));
        }
        let t2 = take.clone();
        net::read_records(first, "\n", move |line| { if let Ok(o) = serde_json::from_str::<Value>(line.trim()) { t2(o); } }).await;
        // The connection dropped before the run said it was over: the run goes on at the computer, so join it again where we left off.
        let mut attempt = 1u64;
        while !state.lock().unwrap().1 {
            if attempt > 60 { return out(json!({"t": "fail", "text": format!("Lost the connection to {}. Claude may still be working; open the chat again in a moment.", m.s("name"))})); }
            tokio::time::sleep(Duration::from_millis((400 * attempt).min(4000))).await;
            let (last, _, chat) = state.lock().unwrap().clone();
            if chat.is_empty() { return out(json!({"t": "fail", "text": format!("Lost the connection to {}.", m.s("name"))})); }
            match net::request(&a.long, &m, "GET", &format!("/sessions/{chat}/stream?from={}", last as i64 + 1), None, None).await {
                Ok(r) if r.status().as_u16() == 404 => { take(json!({"t": "end"})); return; }   // no run any more: it finished while we were away
                Ok(r) if r.status().as_u16() == 200 => { attempt = 0; let t3 = take.clone(); net::read_records(r, "\n", move |line| { if let Ok(o) = serde_json::from_str::<Value>(line.trim()) { t3(o); } }).await; }
                _ => {}
            }
            attempt += 1;
        }
    });
}

async fn wait_sync<F, Fut>(deadline_min: u64, mut done: F, late: &str, mut say: impl FnMut(&Value)) -> Result<(), String>
where F: FnMut() -> Fut, Fut: std::future::Future<Output = (bool, Value)> {
    let end = now_ms() + deadline_min as f64 * 60000.0;
    loop {
        let (ok, info) = done().await;
        if ok { return Ok(()); }
        if now_ms() > end { return Err(late.into()); }
        say(&info);
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}
fn idle(x: &Value) -> bool { !x.t("available") || x.s("state") == "idle" }

async fn dispatch(name: &str, a_: Vec<Value>) -> Result<Value, String> {
    let a = app(); let args = &a_;
    match name {
        "state" => Ok(a.public_state()),
        "refresh" => { refresh_usage(true).await; Ok(a.public_state()) }
        "notify:test" => { notify("Hearth", "Notifications work. This is how a request for your OK looks."); Ok(json!(true)) }
        "settings:set" => {
            let patch = arg(args, 0);
            { let mut s = a.s.lock().unwrap(); if let Some(p) = patch.as_object() { for (k, v) in p { s["settings"][k] = v.clone(); } } }
            a.save();
            if patch.get("callMe").is_some() { set_call_me(patch.t("callMe")); }
            if patch.get("theme").is_some() { let h = handle().clone(); let _ = h.run_on_main_thread(apply_theme); }
            emit("usage", a.public_state());
            Ok(a.setting_all())
        }
        "notes:since" => {
            let (last, all) = (sarg(args, 0), arg(args, 1).as_bool().unwrap_or(false));
            let cur = version();
            let mut vs: Vec<&(&str, &str)> = notes::NOTES.iter().filter(|(v, _)| ver_cmp(v, &cur) != std::cmp::Ordering::Greater && if last.is_empty() { *v == cur } else { ver_cmp(v, &last) == std::cmp::Ordering::Greater }).collect();
            vs.sort_by(|x, y| ver_cmp(y.0, x.0)); vs.truncate(if all { 300 } else { 12 });
            Ok(Value::Array(vs.into_iter().map(|(v, t)| json!({"version": v, "text": t.chars().take(6000).collect::<String>()})).collect()))
        }
        "stats" => { let m = need(&sarg(args, 0))?; let days = arg(args, 1).as_f64().filter(|d| *d > 0.0).unwrap_or(30.0); a.relay_json(&m, "GET", &format!("/stats?days={days}"), None).await }
        "pair:info" => {
            let ri = a.relay.lock().unwrap();
            let addrs: Vec<String> = if ri.ok { Relay::addresses() } else { vec![] };
            let ts = regex::Regex::new(r"^100\.(6[4-9]|[7-9]\d|1[01]\d|12[0-7])\.").unwrap();
            let code = ri.r.as_ref().map(|r| r.pair.lock().unwrap().code.clone());
            Ok(json!({"embedded": ri.ok, "external": ri.in_use, "error": if !ri.ok && !ri.in_use { ri.error.clone() } else { None }, "code": code, "port": if ri.ok { ri.port } else { 47601 },
                "tailscale": addrs.iter().find(|x| ts.is_match(x)), "addresses": addrs, "allowRemote": a.setting("allowRemote").as_bool().unwrap_or(false), "name": hostname(),
                "claude": hearth_relay::relay::find_claude().is_some(), "hooks": sys::hooks_status(), "platform": sys::platform()}))
        }
        "pair:newcode" => Ok(a.relay.lock().unwrap().r.as_ref().map(|r| json!(r.new_pair_code())).unwrap_or(Value::Null)),
        "hooks:install" => { let ri = a.relay.lock().unwrap(); let port = if ri.ok { ri.port } else { 47601 }; drop(ri); sys::hooks_install(port).map(Value::Bool) }
        "hooks:remove" => sys::hooks_remove().map(Value::Bool),
        "system:accent" => Ok(json!(sys::accent().await)),
        "system:dark" => Ok(json!(sys::dark().await)),
        "setup:paths" => { let p = home().join("projects"); let _ = std::fs::create_dir_all(&p); Ok(json!({"home": home().to_string_lossy(), "projects": p.to_string_lossy(), "platform": sys::platform()})) }
        "voice:install" => voice_install().await,
        "app:icon" => {
            let url = sarg(args, 0); let Some(b64) = url.split_once("base64,").map(|x| x.1) else { return Ok(json!(false)) };
            let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) else { return Ok(json!(false)) };
            let h = handle().clone();
            let _ = h.clone().run_on_main_thread(move || {
                let Ok(img) = tauri::image::Image::from_bytes(&bytes) else { return };
                if let Some(w) = h.get_webview_window("main") { let _ = w.set_icon(img.clone()); }
                if let Some(t) = app().tray.lock().unwrap().as_ref() { let _ = t.set_icon(Some(img)); }
            });
            Ok(json!(true))
        }
        "autostart:set" => { let on = arg(args, 0).as_bool().unwrap_or(false); sys::set_autostart(on, &a.icon_file.to_string_lossy())?; a.s.lock().unwrap()["settings"]["autostart"] = json!(on); a.save(); Ok(json!(true)) }
        "machines" => Ok(Value::Array(a.machines().iter().map(pub_m).collect())),
        "machines:add" => add_machine(arg(args, 0)).await,
        "machines:remove" => {
            let id = sarg(args, 0);
            { let mut s = a.s.lock().unwrap(); let l: Vec<Value> = arr(s.g("machines")).into_iter().filter(|m| m.s("id") != id).collect(); s["machines"] = Value::Array(l); }
            a.save(); write_link(); start_events();
            Ok(Value::Array(a.machines().iter().map(pub_m).collect()))
        }
        "discover" => { let ms = a.machines(); Ok(Value::Array(net::discover().await.into_iter().filter(|f| !ms.iter().any(|m| m.s("host") == f.s("host"))).collect())) }
        "pair:requests" => { check_pair_requests().await; Ok(a.last_pair_req.lock().unwrap().clone()) }
        "pair:decide" => {
            let m = a.local().ok_or("This computer is not running Hearth yet.")?;
            a.relay_json(&m, "POST", "/pair/decision", Some(json!({"id": arg(args, 0), "accept": arg(args, 1).as_bool().unwrap_or(false)}))).await?;
            check_pair_requests().await; Ok(a.last_pair_req.lock().unwrap().clone())
        }
        "support:state" => { let s = a.setting_all(); Ok(json!({"email": if s.t("supportToken") { s.s("supportEmail") } else { String::new() }, "signedIn": s.t("supportToken"), "meta": {"app": "Desktop", "version": version(), "platform": sys::os_text()}})) }
        "support:start" => support_call("POST", "/auth/start", Some(json!({"email": arg(args, 0)})), false).await,
        "support:verify" => {
            let j = support_call("POST", "/auth/verify", Some(json!({"email": arg(args, 0), "code": arg(args, 1)})), false).await?;
            { let mut s = a.s.lock().unwrap(); s["settings"]["supportToken"] = j.g("token").clone(); s["settings"]["supportEmail"] = j.g("email").clone(); }
            a.save(); Ok(json!({"email": j.g("email")}))
        }
        "support:logout" => { let _ = support_call("POST", "/auth/logout", Some(json!({})), true).await; { let mut s = a.s.lock().unwrap(); s["settings"]["supportToken"] = json!(""); s["settings"]["supportEmail"] = json!(""); } a.save(); Ok(json!(true)) }
        "support:call" => { let b = arg(args, 2); support_call(&sarg(args, 0), &sarg(args, 1), if b.is_null() { None } else { Some(b) }, true).await }
        "app:version" => Ok(json!(version())),
        "app:relaunch" => { a.quitting.store(true, Ordering::SeqCst); a.save_now(); handle().restart(); }
        "projects:pending" => { check_pending().await; Ok(a.last_pend.lock().unwrap().clone()) }
        "projects:accept" | "projects:reject" => {
            let m = a.local().ok_or("This computer is not running Hearth yet.")?;
            let names = arg(args, 0); let body = if names == json!("all") { json!({"all": true}) } else { json!({"names": names}) };
            a.relay_json(&m, "POST", if name == "projects:accept" { "/sync/projects/accept" } else { "/sync/projects/reject" }, Some(body)).await?;
            check_pending().await; Ok(a.last_pend.lock().unwrap().clone())
        }
        "guests" => Ok(Value::Array(a.guest_machines().iter().map(strip_token).collect())),
        "guests:set" => {
            let re = regex::Regex::new(r"^[A-Za-z0-9_-]{4,16}$").unwrap();
            let list: Vec<Value> = arr(&arg(args, 0)).into_iter().filter(|g| re.is_match(&g.s("share")) && g.g("key").as_str().map(|k| k.starts_with("share.")).unwrap_or(false) && g.g("hostname").is_string() && g.n("port") > 0.0)
                .map(|g| json!({"share": g.s("share"), "key": g.g("key"), "hostname": g.g("hostname"), "port": g.n("port") as u16, "secure": g.t("secure"), "host": g.s("host").chars().take(60).collect::<String>(), "title": g.s("title").chars().take(120).collect::<String>()})).collect();
            a.s.lock().unwrap()["guests"] = Value::Array(list);
            let sig = json!(a.guest_machines().iter().map(|g| json!([g.g("id"), g.g("token"), g.g("host"), g.g("port")])).collect::<Vec<_>>()).to_string();
            a.save();
            let changed = { let mut l = a.last_guest_sig.lock().unwrap(); if *l != sig { *l = sig; true } else { false } };
            if changed { start_events(); }
            Ok(Value::Array(a.guest_machines().iter().map(strip_token).collect()))
        }
        "share:join" => {
            let u = url::Url::parse(&sarg(args, 0)).map_err(|_| "Not a server address")?;
            if !["http", "https"].contains(&u.scheme()) { return Err("Not a server address".into()); }
            let secure = u.scheme() == "https";
            let m = json!({"name": "the host", "host": u.host_str().unwrap_or(""), "port": u.port().unwrap_or(if secure { 443 } else { 80 }), "secure": secure, "token": ""});
            let r = a.relay_json(&m, "POST", "/shares/join", Some(json!({"invite": sarg(args, 1), "name": sarg(args, 2).chars().take(40).collect::<String>()}))).await?;
            Ok(json!({"share": r.g("share"), "key": r.g("key"), "session": r.g("session"), "title": r.g("title"), "host": r.g("host"), "member": r.g("member"), "hostname": m.g("host"), "port": m.g("port"), "secure": secure}))
        }
        "relay" => { let m = need(&sarg(args, 0))?; let b = arg(args, 3); a.relay_json(&m, &sarg(args, 1), &sarg(args, 2), if b.is_null() { None } else { Some(b) }).await }
        "project:move" => project_move(sarg(args, 0), sarg(args, 1), sarg(args, 2), sarg(args, 3)).await,
        "project:continue" => project_continue(sarg(args, 0), sarg(args, 1), sarg(args, 2)).await,
        "update:run" => { let want = arg(args, 0); tauri::async_runtime::spawn(async move { app().upd.run(if want.is_object() { want } else { json!({}) }).await; }); Ok(json!(true)) }
        "update:check" => { let det = a.upd.details().await; tauri::async_runtime::spawn(async { app().upd.check(true).await; }); Ok(det) }
        "update:later" => { a.upd.later().await; Ok(json!(true)) }
        "update:status" => Ok(a.upd.status().await),
        "hub:status" => {
            let (server, local) = (a.server(), a.local());
            async fn get(m: &Option<Value>, p: &str) -> Option<Value> { match m { Some(m) => app().relay_json(m, "GET", p, None).await.ok(), None => None } }
            let (ss, sync, lsync) = tokio::join!(get(&server, "/status"), get(&server, "/sync"), get(&local, "/sync"));
            Ok(json!({"server": server.as_ref().map(|s| json!({"name": s.g("name"), "host": s.g("host"), "online": ss.is_some(), "laptop": ss.as_ref().map(|x| x.g("laptop").clone()), "listeners": ss.as_ref().map(|x| x.n("listeners")).unwrap_or(0.0), "queued": ss.as_ref().map(|x| x.n("queued")).unwrap_or(0.0), "sync": sync})),
                "local": local.as_ref().map(|l| json!({"name": l.g("name"), "sync": lsync})), "me": hostname()}))
        }
        "hub:phonecode" => {
            let server = a.server().ok_or("Connect your server first.")?;
            let r = a.relay_json(&server, "POST", "/pair/new", Some(json!({}))).await?;
            Ok(json!({"code": r.g("code"), "address": server.g("host"), "expiresIn": r.g("expiresIn")}))
        }
        "upload" => {
            let m = need(&sarg(args, 0))?; let enc = hearth_relay::util::enc;
            let session = sarg(args, 1);
            let q = if !session.is_empty() { format!("session={}", enc(&session)) } else { format!("cwd={}", enc(&sarg(args, 2))) };
            let buf = base64::engine::general_purpose::STANDARD.decode(sarg(args, 4)).map_err(|e| e.to_string())?;
            let res = net::request(&a.long, &m, "POST", &format!("/upload?{q}&name={}", enc(&sarg(args, 3))), Some((buf, "application/octet-stream")), Some(120000)).await?;
            let st = res.status().as_u16(); let b = res.bytes().await.unwrap_or_default();
            if st != 200 { return Err(net::err_of(&b).unwrap_or_else(|| "Upload failed".into())); }
            Ok(serde_json::from_slice::<Value>(&b).map(|v| v.g("path").clone()).unwrap_or(Value::Null))
        }
        "openPath" => { let p = sarg(args, 0); if !p.is_empty() { sys::open(&p); } Ok(json!(true)) }
        "terminal:open" => Ok(sys::terminal(&sarg(args, 0), &sarg(args, 1))),
        "pickFolder" => {
            use tauri_plugin_dialog::DialogExt;
            let (tx, rx) = tokio::sync::oneshot::channel();
            let mut d = handle().dialog().file();
            if let Some(w) = main_win() { d = d.set_parent(&w); }
            d.pick_folder(move |p| { let _ = tx.send(p); });
            Ok(match rx.await.ok().flatten() { Some(p) => json!(p.into_path().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()), None => Value::Null })
        }
        "stop" => match a.machine(&sarg(args, 0)) { Some(m) => a.relay_json(&m, "POST", "/stop", Some(json!({"session": arg(args, 1)}))).await, None => Ok(json!(false)) },
        "openExternal" => { let u = sarg(args, 0); if u.starts_with("http://") || u.starts_with("https://") { sys::open(&u); } Ok(Value::Null) }
        "send" => {
            let m = need(&sarg(args, 0))?;
            let sid = Some(sarg(args, 5)).filter(|s| !s.is_empty()).unwrap_or_else(|| rand_hex(4));
            send(m, sarg(args, 1), arg(args, 2), arg(args, 3), hearth_relay::util::truthy(&arg(args, 4)), sid.clone(), arg(args, 6));
            Ok(json!(sid))
        }
        "voice:status" => match a.machine(&sarg(args, 0)) { Some(m) => a.relay_json(&m, "GET", "/voice/status", None).await, None => Ok(json!({"stt": false, "tts": false})) },
        "voice:stt" => {
            let m = need(&sarg(args, 0))?;
            let wav = base64::engine::general_purpose::STANDARD.decode(sarg(args, 1)).map_err(|e| e.to_string())?;
            let res = net::request(&a.long, &m, "POST", "/voice/stt", Some((wav, "audio/wav")), Some(90000)).await?;
            let st = res.status().as_u16(); let b = res.bytes().await.unwrap_or_default();
            if st != 200 { return Err(net::err_of(&b).unwrap_or_else(|| "Speech recognition failed".into())); }
            Ok(json!(serde_json::from_slice::<Value>(&b).map(|v| v.s("text")).unwrap_or_default()))
        }
        "call:action" => call_action(&sarg(args, 0), &sarg(args, 1)).await,
        "test:ring" if std::env::var("HEARTH_TEST_LOG").is_ok() => { let m = app().local().ok_or("no local")?; ring(&m, arg(args, 0)); Ok(json!(true)) }
        // tests only (HEARTH_TEST_LOG): the page reports what it saw
        "test:log" => match std::env::var("HEARTH_TEST_LOG") { Ok(f) => { use std::io::Write; if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(f) { let _ = writeln!(f, "{}", sarg(args, 0)); } Ok(json!(true)) } Err(_) => Err("not testing".into()) },
        _ => Err(format!("Unknown call {name}")),
    }
}

impl App { fn setting_all(&self) -> Value { self.s.lock().unwrap().g("settings").clone() } }

async fn add_machine(m: Value) -> Result<Value, String> {
    let a = app();
    let (mut name, mut host, mut token) = (m.s("name"), m.s("host").trim().to_string(), m.s("token").trim().to_string());
    let (mut port, mut secure) = (m.n("port") as u16, false);
    if host.starts_with("hearth://") {   // a pairing link
        if let Ok(u) = url::Url::parse(&host) {
            let q: HashMap<String, String> = u.query_pairs().map(|(k, v)| (k.to_string(), v.to_string())).collect();
            if let Some(t) = q.get("token").filter(|t| !t.is_empty()) { token = t.clone(); }
            if name.is_empty() { name = q.get("name").cloned().unwrap_or_default(); }
            host = q.get("url").cloned().unwrap_or_default();
        }
    }
    if host.starts_with("http://") || host.starts_with("https://") {
        let u = url::Url::parse(&host).map_err(|_| "Type the server address and the 6-digit code.")?;
        secure = u.scheme() == "https"; port = u.port().unwrap_or(if secure { 443 } else { 80 }); host = u.host_str().unwrap_or("").to_string();
    } else if host.contains(':') { let (h, p) = host.split_once(':').unwrap(); port = p.parse().unwrap_or(port); host = h.to_string(); }
    if host.is_empty() || token.is_empty() { return Err("Type the server address and the 6-digit code.".into()); }
    if port == 0 { port = 47601; }
    let code: String = token.chars().filter(|c| !c.is_whitespace()).collect();
    if code.len() == 6 && code.chars().all(|c| c.is_ascii_digit()) {   // the short code from the server's installer: trade it for the real key
        let base = format!("{}://{}:{}", if secure { "https" } else { "http" }, host, port);
        let r = a.http.post(format!("{base}/pair")).header("Content-Type", "application/json").body(json!({"code": code}).to_string()).timeout(Duration::from_secs(15)).send().await
            .map_err(|_| format!("I could not reach {host}. Check the address and your internet."))?;
        let st = r.status().as_u16(); let j: Value = r.json().await.unwrap_or_else(|_| json!({}));
        if !(200..300).contains(&st) || j.s("token").is_empty() { return Err(if st == 403 { "That code is wrong or ran out. Ask the server for a new one (hearth-server code).".into() } else if !j.s("error").is_empty() { j.s("error") } else { "The server did not accept the code.".into() }); }
        token = j.s("token"); if name.is_empty() { name = j.s("name"); }
    }
    {
        let mut s = a.s.lock().unwrap();
        let mut l: Vec<Value> = arr(s.g("machines")).into_iter().filter(|x| x.s("host") != host || x.n("port") as u16 != port).collect();
        l.push(json!({"id": rand_hex(4), "name": if name.is_empty() { host.clone() } else { name }, "host": host, "port": port, "token": token, "secure": secure}));
        s["machines"] = Value::Array(l);
    }
    a.save(); start_events(); write_link();
    Ok(Value::Array(a.machines().iter().map(pub_m).collect()))
}

/// Tells the relay which server (if any) to link to. The built-in relay reads it from this app's state; the always-on relay service reads link.json.
fn write_link() {
    let a = app();
    a.save_now();
    if !a.relay.lock().unwrap().in_use { return; }
    let srv = a.server();
    let text = srv.map(|s| { let p = s.n("port") as u16; json!({"url": format!("https://{}{}", s.s("host"), if p != 0 && p != 443 { format!(":{p}") } else { String::new() }), "name": s.g("name"), "token": s.g("token")}).to_string() }).unwrap_or_default();
    let _ = write_private(&home().join(".config/hearth-node/link.json"), text.as_bytes());
}

async fn project_move(local_id: String, server_id: String, cwd: String, name: String) -> Result<Value, String> {
    let a = app();
    let (local, server) = match (a.machine(&local_id), a.machine(&server_id)) { (Some(l), Some(s)) => (l, s), _ => return Err("Unknown computer".into()) };
    let say = |t: String| emit("move-progress", json!({"text": t}));
    say("Moving the project into your projects folder…".into());
    let mv = a.relay_json(&local, "POST", "/projects/move", Some(json!({"cwd": cwd, "name": name}))).await?;
    let new_path = mv.s("path");
    let info = a.relay_json(&server, "GET", "/projects", None).await?;
    if info.s("root").is_empty() { return Err("That server has no projects folder.".into()); }
    let server_path = format!("{}/{}", info.s("root").trim_end_matches('/'), hearth_relay::util::basename(&new_path));
    let (l2, s2, sp) = (local.clone(), server.clone(), server_path.clone());
    wait_sync(25, || { let (l, s, sp) = (l2.clone(), s2.clone(), sp.clone()); async move {
        let (ls, ss, pl) = tokio::join!(app().relay_json(&l, "GET", "/sync", None), app().relay_json(&s, "GET", "/sync", None), app().relay_json(&s, "GET", "/projects", None));
        let (ls, ss, pl) = (ls.unwrap_or(json!({})), ss.unwrap_or(json!({})), pl.unwrap_or(json!({"projects": []})));
        let there = arr(pl.g("projects")).iter().any(|p| p.s("path") == sp);
        let ok = there && idle(&ls) && (!ss.t("available") || (ss.s("state") == "idle" && !ss.t("needFiles")));
        (ok, json!({"left": ss.n("needFiles") + ls.n("needFiles")}))
    } }, "Syncing is taking long. The project is moved and will show up on the server when syncing finishes.", |i| { let left = i.n("left"); say(format!("Syncing the files to the server…{}", if left > 0.0 { format!(" {left} files left") } else { String::new() })); }).await?;
    say("Copying your chats to the server…".into());
    export_import(&local, &server, &new_path, &server_path, "Could not read the chats of this project.", "Could not copy the chats.").await?;
    let session = newest_session(&server, &server_path).await;
    Ok(json!({"cwd": server_path, "newPath": new_path, "session": session}))
}

async fn project_continue(server_id: String, local_id: String, server_cwd: String) -> Result<Value, String> {
    let a = app();
    let (local, server) = match (a.machine(&local_id), a.machine(&server_id)) { (Some(l), Some(s)) => (l, s), _ => return Err("Unknown computer".into()) };
    let say = |t: &str| emit("move-progress", json!({"text": t}));
    let st = a.relay_json(&local, "GET", "/status", None).await?;
    if st.s("moveRoot").is_empty() { return Err("This computer has no projects folder set up.".into()); }
    let local_cwd = format!("{}/{}", st.s("moveRoot").trim_end_matches('/'), hearth_relay::util::basename(&server_cwd));
    say("Checking that the files are on this computer…");
    let (l2, s2, lc) = (local.clone(), server.clone(), local_cwd.clone());
    wait_sync(20, || { let (l, s, lc) = (l2.clone(), s2.clone(), lc.clone()); async move {
        let q = format!("/projects/exists?cwd={}", hearth_relay::util::enc(&lc));
        let (ex, ss, ls) = tokio::join!(app().relay_json(&l, "GET", &q, None), app().relay_json(&s, "GET", "/sync", None), app().relay_json(&l, "GET", "/sync", None));
        (ex.map(|e| e.t("exists")).unwrap_or(false) && idle(&ss.unwrap_or(json!({}))) && idle(&ls.unwrap_or(json!({}))), Value::Null)
    } }, "The files are still syncing to this computer. Try again in a moment.", |_| say("Waiting for the project files to sync…")).await?;
    say("Copying the chat to this computer…");
    export_import(&server, &local, &server_cwd, &local_cwd, "Could not read the chats of this project on the server.", "Could not copy the chat.").await?;
    let session = newest_session(&local, &local_cwd).await;
    Ok(json!({"cwd": local_cwd, "session": session}))
}

async fn call_action(key: &str, action: &str) -> Result<Value, String> {
    let a = app();
    let Some((label, m, ev)) = a.calls.lock().unwrap().get(key).cloned() else { return Ok(json!(false)) };
    if ev.t("forMember") && action == "deny" && !ev.s("req").is_empty() {
        let _ = a.relay_json(&m, "POST", &format!("/sessions/{}/requests/{}", ev.s("session"), ev.s("req")), Some(json!({"decision": "deny"}))).await;
    } else if (action == "allow" || action == "deny" || (ev.s("kind") == "support" && ["claude", "list", "ignore"].contains(&action))) && !ev.s("req").is_empty() {
        let _ = a.relay_json(&m, "POST", "/decision", Some(json!({"req": ev.g("req"), "behavior": action}))).await;
    }
    if action == "talk" || action == "chat" {
        show_main(Some("sessions"));
        emit("open-session", json!({"machine": m.g("id"), "session": ev.g("session"), "title": ev.g("title"), "cwd": ev.g("cwd"), "voice": action == "talk"}));
    }
    if let Some(w) = handle().get_webview_window(&label) { let _ = w.close(); }
    Ok(json!(true))
}

/// The better local voices (Linux): runs the installer script and reports each line while it works.
async fn voice_install() -> Result<Value, String> {
    if !cfg!(target_os = "linux") { return Err("The extra voices are for Linux. On this system Hearth uses the voices of your computer.".into()); }
    let a = app();
    if a.voice_job.swap(true, Ordering::SeqCst) { return Ok(json!({"running": true})); }
    let send = |d: Value| emit("voice-install", d);
    let mut script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../relay/install_voice.sh");
    if !script.exists() {
        send(json!({"line": "Downloading the voice installer…"}));
        script = std::env::temp_dir().join("hearth-install-voice.sh");
        let r = a.http.get("https://raw.githubusercontent.com/veelvoer/hearth/main/relay/install_voice.sh").timeout(Duration::from_secs(30)).send().await;
        let text = match r { Ok(r) if r.status().is_success() => r.text().await.ok(), _ => None };
        match text { Some(t) => { let _ = std::fs::write(&script, t); } None => { a.voice_job.store(false, Ordering::SeqCst); return Err("Could not download the voice installer. Check your internet connection.".into()); } }
    }
    let child = tokio::process::Command::new("bash").arg(&script).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn();
    let mut child = match child { Ok(c) => c, Err(e) => { a.voice_job.store(false, Ordering::SeqCst); return Err(e.to_string()); } };
    use tokio::io::AsyncReadExt;
    fn feed(b: &[u8]) { for l in String::from_utf8_lossy(b).split(['\n', '\r']).map(str::trim).filter(|l| !l.is_empty()) { emit("voice-install", json!({"line": l.chars().take(160).collect::<String>()})); } }
    let (mut out, mut err) = (child.stdout.take().unwrap(), child.stderr.take().unwrap());
    tauri::async_runtime::spawn(async move { let mut b = [0u8; 4096]; while let Ok(n) = err.read(&mut b).await { if n == 0 { break; } feed(&b[..n]); } });
    tauri::async_runtime::spawn(async move {
        let mut b = [0u8; 4096]; while let Ok(n) = out.read(&mut b).await { if n == 0 { break; } feed(&b[..n]); }
        let ok = child.wait().await.map(|s| s.success()).unwrap_or(false);
        app().voice_job.store(false, Ordering::SeqCst);
        emit("voice-install", json!({"done": true, "ok": ok}));
    });
    Ok(json!({"running": true}))
}

#[tauri::command]
async fn cm(name: String, args: Vec<Value>) -> Result<Value, String> { dispatch(&name, args).await }

#[tauri::command]
async fn cm_bin(name: String, args: Vec<Value>) -> Result<tauri::ipc::Response, String> {
    if name != "voice:tts" { return Err(format!("Unknown call {name}")); }
    let a = app();
    let m = need(&sarg(&args, 0))?;
    let res = net::request(&a.long, &m, "POST", "/voice/tts", Some((json!({"text": arg(&args, 1), "lang": arg(&args, 2)}).to_string().into_bytes(), "application/json")), Some(60000)).await?;
    let st = res.status().as_u16(); let b = res.bytes().await.map_err(|e| net::err_text(&e))?;
    if st != 200 { return Err(net::err_of(&b).unwrap_or_else(|| "Speech synthesis failed".into())); }
    Ok(tauri::ipc::Response::new(b.to_vec()))
}

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    // the small MCP server that gives Claude the ask_user tool: Claude starts this program with "mcp-ask"
    if argv.get(1).map(|a| a == "mcp-ask").unwrap_or(false) { hearth_relay::mcpask::run(); return; }
    if argv.iter().any(|a| a == "--version") { println!("{}", env!("CARGO_PKG_VERSION")); return; }
    let hidden = argv.iter().any(|a| a == "--hidden");
    let dir = data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let icon_file = dir.join("icon.png");
    if std::fs::metadata(&icon_file).map(|m| m.len() as usize != ICON_PNG.len()).unwrap_or(true) { let _ = std::fs::write(&icon_file, ICON_PNG); }
    let a = Arc::new(App {
        s: Mutex::new(load_state(&dir)), dir: dir.clone(), relay: Mutex::new(RelayInfo::default()),
        http: net::client(None), long: net::client(Some(300)), events_http: net::client(Some(150)),
        listeners: Mutex::new(vec![]), calls: Mutex::new(HashMap::new()), notified_req: Mutex::new(VecDeque::new()),
        pend_seen: Mutex::new(HashSet::new()), last_pend: Mutex::new(json!([])), pair_seen: Mutex::new(HashSet::new()), last_pair_req: Mutex::new(json!([])),
        refreshing: AtomicBool::new(false), quitting: AtomicBool::new(false), voice_job: AtomicBool::new(false), save_gen: AtomicU64::new(0), popups: AtomicU64::new(0),
        last_guest_sig: Mutex::new(String::new()), icon_file, upd: upd::Upd::default(), tray_ring: Mutex::new(None), tray: Mutex::new(None),
    });
    let _ = APP.set(a);

    let ctx = tauri::generate_context!();
    let res = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_, _, _| show_main(None)))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![cm, cm_bin])
        .setup(move |tapp| {
            let h = tapp.handle().clone();
            let _ = HANDLE.set(h.clone());
            let a = app();
            // this computer's relay, inside the app (unless the always-on relay service already runs here)
            if std::env::var("CM_NO_RELAY").is_err() {
                let o = hearth_relay::relay::Opts {
                    dir: a.dir.to_string_lossy().to_string(), port: std::env::var("CM_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(47601), host: "0.0.0.0".into(),
                    name: Some(hostname()), public_mode: false, projects_root: None, allow_bypass: true, allow_remote: a.setting("allowRemote").as_bool().unwrap_or(false),
                    self_update: false, relay_dir: String::new(), move_root: home().join("projects").to_string_lossy().to_string(), seed_token: String::new(), voice: None,
                };
                let port = o.port;
                let ri = match tauri::async_runtime::block_on(hearth_relay::boot::embedded(o)) {
                    Ok(r) => RelayInfo { ok: true, in_use: false, error: None, port, token: r.token.clone(), r: Some(r) },
                    Err((in_use, e)) => RelayInfo { ok: false, in_use, error: Some(e), ..Default::default() },
                };
                *a.relay.lock().unwrap() = ri;
            } else { a.relay.lock().unwrap().in_use = true; }
            write_link();
            create_main(&h, hidden);
            if std::env::var("CM_NO_TRAY").is_err() { if let Err(e) = build_tray(&h) { eprintln!("no tray icon: {e}"); } }
            every(6000, 120000, || tauri::async_runtime::spawn(async { app().upd.check(false).await; }));
            every(2500, 2500, || tauri::async_runtime::spawn(check_pair_requests()));
            every(8000, 20000, || tauri::async_runtime::spawn(check_pending()));
            every(30000, 30000, || tauri::async_runtime::spawn(async {
                refresh_usage(false).await;
                // keep the numbers live while you look at them
                let stale = { let s = app().s.lock().unwrap(); s.g("usage").is_null() || now_ms() - s.g("usage").n("at") > 120000.0 };
                if stale && main_in_front() { refresh_usage(true).await; }
            }));
            every(30000, 30000, || tauri::async_runtime::spawn(async { let h = handle().clone(); let _ = h.run_on_main_thread(apply_theme); }));
            tauri::async_runtime::spawn(refresh_usage(false));
            start_events();
            // tests only: run a script in the window once it has loaded
            if let Ok(f) = std::env::var("HEARTH_EVAL") {
                tauri::async_runtime::spawn(async move { tokio::time::sleep(Duration::from_secs(4)).await; if let (Ok(js), Some(w)) = (std::fs::read_to_string(f), main_win()) { let _ = w.eval(&js); } });
            }
            Ok(())
        })
        .build(ctx);
    let tapp = match res { Ok(t) => t, Err(e) => { eprintln!("Hearth could not start: {e}"); std::process::exit(1); } };
    tapp.run(|_, ev| {
        if let tauri::RunEvent::Exit = ev {
            app().quitting.store(true, Ordering::SeqCst); app().save_now();
            // runs of this computer's relay end with the app, as they did when the relay was its own process
            if let Some(r) = app().relay.lock().unwrap().r.clone() { for j in r.jobs.lock().unwrap().values() { if !j.is_done() { hearth_relay::jobs::kill_tree(*j.pid.lock().unwrap(), false); } } }
        }
    });
}
