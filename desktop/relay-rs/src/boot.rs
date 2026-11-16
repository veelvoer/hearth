//! Making the relay: its folder, token and saved state, the timers, the HTTP server and the Wi-Fi discovery. Port of the start of server.js `start()`.
use crate::relay::*;
use crate::util::*;
use axum::body::Body;
use axum::extract::{ConnectInfo, State};
use axum::http::Request;
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub const DISCOVER_PORT: u16 = 47600;

pub fn build_sig() -> String {
    // what program this is: its version, size and time (reading the whole program would be slow)
    let m = std::env::current_exe().ok().and_then(|p| std::fs::metadata(p).ok());
    let id = format!("{}|{}|{}", crate::VERSION, m.as_ref().map(|m| m.len()).unwrap_or(0), m.as_ref().map(mtime_ms).unwrap_or(0.0) as i64);
    sha1_hex(id.as_bytes())[..12].to_string()
}

pub fn make(o: Opts) -> Result<Arc<Relay>, String> {
    let dir = PathBuf::from(&o.dir);
    let _ = std::fs::create_dir_all(&dir);
    let root = match &o.projects_root { Some(p) if !p.is_empty() => { let _ = std::fs::create_dir_all(p); Some(realpath(p).ok_or_else(|| format!("The projects folder {p} cannot be used"))?) } _ => None };
    let move_root = if o.move_root.is_empty() { None } else { Some(resolve(&o.move_root).to_string_lossy().to_string()) };
    let cfg_file = dir.join("relay.json");
    let mut cfg = read_json(&cfg_file).filter(|v| v.is_object()).unwrap_or_else(obj);
    if cfg.s("token").is_empty() {
        let seed = o.seed_token.trim().to_string();
        set(&mut cfg, "token", json!(if seed.is_empty() { rand_b64url(if o.public_mode { 32 } else { 16 }) } else { seed }));
        write_json_private(&cfg_file, &cfg);
    }
    if cfg.s("machineId").is_empty() { set(&mut cfg, "machineId", json!(rand_hex(5))); write_json_private(&cfg_file, &cfg); }
    let token = cfg.s("token");
    let usage = read_json(&dir.join("usage.json")).filter(|v| v.is_object()).unwrap_or_else(obj);
    let week_hist = read_json(&dir.join("weekhist.json")).and_then(|v| v.as_array().cloned()).unwrap_or_default();
    let targets = read_json(&dir.join("weekplan.json")).filter(|d| d.g("t").is_object()).map(|d| (d.n("end"), d.g("t").as_object().cloned().unwrap_or_default())).unwrap_or((0.0, Default::default()));
    let workflows = read_json(&dir.join("workflows.json")).and_then(|v| v.as_object().cloned()).unwrap_or_default();
    let room_sync = read_json(&dir.join("roomsync.json")).and_then(|v| v.as_object().cloned()).unwrap_or_default();
    let work = read_json(&dir.join("work.json")).and_then(|v| v.as_array().cloned()).unwrap_or_default();
    let name = o.name.clone().unwrap_or_else(hostname);
    let hub_sync = if o.public_mode { root.as_ref().map(|r| crate::chathub::HubSync::new(&dir, r)) } else { None };
    let hub_files = if o.public_mode { root.as_ref().map(|r| crate::filehub::HubFiles::new(r)) } else { None };
    let r = Arc::new(Relay {
        dir: dir.clone(), root, move_root, token, boot: rand_hex(4), build: build_sig(), cfg_lock: Mutex::new(()),
        jobs: Mutex::new(HashMap::new()), pending: Mutex::new(HashMap::new()), pending_by_key: Mutex::new(HashMap::new()),
        ev: Mutex::new(Events { seq: 0, list: VecDeque::new(), clients: vec![], guests: vec![] }),
        hub: Mutex::new(Hub { work, agents: HashMap::new(), remote_pending: HashMap::new(), answers_for: HashMap::new(), waiters: vec![], chat_epoch: 1 }),
        failures: Mutex::new(HashMap::new()), decisions: Mutex::new(VecDeque::new()), usage: Mutex::new(usage.clone()), week_hist: Mutex::new(week_hist), week_targets: Mutex::new(targets),
        pair: Mutex::new(PairState { code: String::new(), fails: 0, locked_until: 0.0 }), hub_pair: Mutex::new(HubPair { code: String::new(), expires: 0.0, fails: 0, locked_until: 0.0 }),
        pair_reqs: Mutex::new(HashMap::new()), asked_at: Mutex::new(HashMap::new()), last_question: Mutex::new(HashMap::new()), limit_waits: Mutex::new(HashMap::new()),
        workflows: Mutex::new(workflows), sudo_cache: Mutex::new((0.0, false, String::new(), String::new(), None)), owner_fix: Mutex::new((false, 0.0)), seen_denied: Mutex::new(0),
        cost_cache: Mutex::new((0.0, 0.0)), last_check: Mutex::new(0.0), keeping: Mutex::new(false), gt_token: Mutex::new((0.0, String::new())), room_sync: Mutex::new(room_sync),
        api_busy: Mutex::new(0), api_queues: tokio::sync::Mutex::new(HashMap::new()), shares: crate::shares::Shares::new(&dir, name), keys: crate::apikeys::Keys::new(&dir),
        hub_sync, hub_files, link: Mutex::new(Default::default()), machine_id: cfg.s("machineId"), started: now_ms(),
        http: reqwest::Client::builder().build().map_err(|e| e.to_string())?, o,
    });
    r.new_pair_code();
    r.note_week(&usage);
    Ok(r)
}

fn every(secs: f64, f: impl Fn() + Send + 'static) { tokio::spawn(async move { loop { tokio::time::sleep(std::time::Duration::from_secs_f64(secs)).await; f(); } }); }
fn later(secs: f64, f: impl FnOnce() + Send + 'static) { tokio::spawn(async move { tokio::time::sleep(std::time::Duration::from_secs_f64(secs)).await; f(); }); }

/// The timers of a running relay.
pub fn timers(r: &Arc<Relay>) {
    { let r = r.clone(); every(3600.0, move || r.nightly_backup()); }
    { let r = r.clone(); every(10.0 * 60.0, move || { r.new_pair_code(); }); }
    let tick = std::env::var("HEARTH_ROUTINE_TICK_MS").ok().and_then(|v| v.parse::<f64>().ok()).unwrap_or(30000.0) / 1000.0;
    { let r = r.clone(); every(tick, move || { let now = now_ms(); for rt in r.read_routines() { if Relay::routine_due(&rt, now) { let why = if rt.g("schedule").s("type") == "daily" { "on schedule" } else { "on its interval" }; if let Err(e) = r.run_routine(&rt, why) { r.log(&format!("routine {}: {}", rt.s("name"), e)); } } } }); }
    let wft = std::env::var("HEARTH_WF_TICK_MS").ok().and_then(|v| v.parse::<f64>().ok()).unwrap_or(15000.0) / 1000.0;
    { let r = r.clone(); every(wft, move || { let ids: Vec<String> = r.workflows.lock().unwrap().keys().cloned().collect(); for s in ids { r.wf_tick(&s); } }); }
    { let r = r.clone(); later(4.0, move || { let ids: Vec<String> = r.workflows.lock().unwrap().keys().cloned().collect(); for s in ids { r.wf_tick(&s); } }); }
    { let r = r.clone(); every(10.0 * 60.0, move || { let r = r.clone(); tokio::spawn(async move { r.keep_login(false).await; }); }); }
    { let r = r.clone(); later(20.0, move || { tokio::spawn(async move { r.keep_login(false).await; }); }); }
    let beat = r.read_cfg().n("heartbeatMinutes");
    if beat > 0.0 { let r = r.clone(); every(beat.max(5.0) * 60.0, move || { let r = r.clone(); tokio::spawn(async move { r.check_usage().await; }); }); }
    { let r = r.clone(); later(5.0, move || r.fix_ownership("start")); }
    { let r = r.clone(); every(10.0 * 60.0, move || {
        let dn = crate::core::denied_count(); let mut sd = r.seen_denied.lock().unwrap();
        if dn > *sd { *sd = dn; drop(sd); crate::core::list_files(); r.owner_fix.lock().unwrap().1 = 0.0; r.fix_ownership("looking again"); } else { drop(sd); crate::core::list_files(); }
    }); }
    // read the chat titles in the background so the first chat list is quick
    std::thread::spawn(|| { std::thread::sleep(std::time::Duration::from_millis(500)); let _ = crate::core::summaries(); });
}

/// The HTTP server. Resolves when it stops.
pub async fn serve(r: Arc<Relay>, listener: tokio::net::TcpListener) {
    async fn handler(State(r): State<Arc<Relay>>, ConnectInfo(peer): ConnectInfo<SocketAddr>, req: Request<Body>) -> axum::response::Response {
        let (parts, body) = req.into_parts();
        let path = parts.uri.path().to_string();
        let mut segs: Vec<String> = path.trim_start_matches('/').trim_end_matches('/').split('/').map(|s| percent_encoding::percent_decode_str(s).decode_utf8_lossy().to_string()).collect();
        if segs.first().map(|s| s == "inbox").unwrap_or(false) { segs[0] = "support".into(); }   // the proxy sends /support/* to the support desk, so the apps say /inbox/*
        let q = query(parts.uri.query().unwrap_or(""));
        let peer_ip = { let ip = peer.ip(); match ip { std::net::IpAddr::V6(v6) => v6.to_ipv4_mapped().map(|v4| v4.to_string()).unwrap_or_else(|| v6.to_string()), _ => ip.to_string() } };
        let xff = parts.headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()).map(|s| s.split(',').next().unwrap_or("").trim().to_string()).filter(|s| !s.is_empty());
        let ip = if is_loopback(&peer_ip) { xff.clone().unwrap_or_else(|| peer_ip.clone()) } else { peer_ip.clone() };
        let local_direct = is_loopback(&peer_ip) && xff.is_none();
        let gz = parts.headers.get("accept-encoding").and_then(|v| v.to_str().ok()).map(|v| v.split(',').any(|x| x.trim().starts_with("gzip"))).unwrap_or(false);
        let url = parts.uri.path_and_query().map(|p| p.as_str().to_string()).unwrap_or_default();
        let method = parts.method.as_str().to_string();
        let req = crate::routes::Req { method: method.clone(), parts: segs, q, headers: parts.headers, ip, local_direct, body: std::sync::Mutex::new(Some(body)), gz, url: url.clone() };
        let r2 = r.clone();
        match tokio::spawn(async move { r2.handle(req).await }).await {
            Ok(resp) => resp,
            Err(e) => {
                r.log(&format!("request {} {}: {}", method, url.split('?').next().unwrap_or(""), e));
                crate::routes::json_resp(500, &json!({"error": format!("The server hit a problem with that request ({})", clip(&e.to_string(), 120))}), false)
            }
        }
    }
    let app = axum::Router::new().fallback(handler).with_state(r.clone());
    let _ = axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await;
}

/// The desktop app is found on the Wi-Fi: it answers a UDP "who is there".
pub fn discovery(port: u16) {
    std::thread::spawn(move || {
        let Ok(s) = std::net::UdpSocket::bind(("0.0.0.0", DISCOVER_PORT)) else { return };
        let mut buf = [0u8; 512];
        while let Ok((n, from)) = s.recv_from(&mut buf) {
            if String::from_utf8_lossy(&buf[..n]).starts_with("CLAUDE_METER_DISCOVER") && lan_only(&from.ip().to_string()) {
                let _ = s.send_to(json!({"name": hostname(), "port": port}).to_string().as_bytes(), from);
            }
        }
    });
}

/// The link to your server: dir/link.json, or (on a computer) the server you connected in the desktop app.
pub fn watch_link(r: Arc<Relay>, lan: bool) {
    tokio::spawn(async move {
        let mut last = String::new();
        loop {
            let mut t = std::fs::read_to_string(r.dir.join("link.json")).unwrap_or_default();
            if t.trim().is_empty() && lan {
                let h = home();
                for p in [h.join(".config/Hearth/state.json"), PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("Hearth/state.json")] {
                    if let Some(m) = read_json(&p).and_then(|s| arr(s.g("machines")).into_iter().find(|x| x.t("secure") && !x.s("token").is_empty())) {
                        let port = m.n("port"); t = json!({"url": format!("https://{}{}", m.s("host"), if port > 0.0 && port != 443.0 { format!(":{}", port) } else { String::new() }), "name": m.s("name"), "token": m.s("token")}).to_string(); break;
                    }
                }
            }
            if t != last { last = t.clone(); crate::agent::set_link(&r, serde_json::from_str::<Value>(&t).ok()); }
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        }
    });
}

/// The relay inside the desktop app: on this computer's network (pairing by code), in the app's own folder.
/// Err((true, _)) = the port is taken (the always-on relay service runs here already).
pub async fn embedded(o: Opts) -> Result<Arc<Relay>, (bool, String)> {
    let (host, port) = (o.host.clone(), o.port);
    let r = make(o).map_err(|e| (false, e))?;
    let listener = tokio::net::TcpListener::bind(format!("{host}:{port}")).await.map_err(|e| (e.kind() == std::io::ErrorKind::AddrInUse, e.to_string()))?;
    timers(&r);
    discovery(port);
    watch_link(r.clone(), true);
    tokio::spawn(serve(r.clone(), listener));
    Ok(r)
}
