//! The controller: every call the old pages made (`cm.*` in desktop/tauri `dispatch`) as a plain function of [`App`].
use crate::app::{pub_m, strip_token, to_machine, App, Event};
use crate::{net, sys};
use base64::Engine;
use hearth_relay::relay::Relay;
use hearth_relay::util::{arr, basename, enc, home, hostname, rand_hex, J};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

mod notes { include!(concat!(env!("OUT_DIR"), "/notes.rs")); }
pub fn version() -> String { env!("CARGO_PKG_VERSION").to_string() }
pub fn ver_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let p = |v: &str| v.split('.').map(|x| x.parse::<i64>().unwrap_or(0)).collect::<Vec<_>>();
    let (x, y) = (p(a), p(b));
    for i in 0..3 { let c = x.get(i).unwrap_or(&0).cmp(y.get(i).unwrap_or(&0)); if c != std::cmp::Ordering::Equal { return c; } }
    std::cmp::Ordering::Equal
}
/// Patch notes between `last` (the version the user last saw; empty = only the current one) and the running version, newest first.
pub fn notes_since(list: &[(&str, &str)], cur: &str, last: &str, all: bool) -> Vec<Value> {
    let mut vs: Vec<&(&str, &str)> = list.iter().filter(|(v, _)| ver_cmp(v, cur) != std::cmp::Ordering::Greater && if last.is_empty() { *v == cur } else { ver_cmp(v, last) == std::cmp::Ordering::Greater }).collect();
    vs.sort_by(|x, y| ver_cmp(y.0, x.0)); vs.truncate(if all { 300 } else { 12 });
    vs.into_iter().map(|(v, t)| json!({"version": v, "text": t.chars().take(6000).collect::<String>()})).collect()
}

pub const SUPPORT_URL_DEFAULT: &str = "https://cm.vpswb.store/support";

/// A pairing request's machine out of what the user typed: a server address (host, host:port, https://host, a hearth:// link) and a token or 6-digit code.
pub struct Target { pub name: String, pub host: String, pub port: u16, pub secure: bool, pub token: String }
pub fn parse_target(m: &Value) -> Result<Target, String> {
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
    } else if let Some((h, p)) = host.split_once(':') { port = p.parse().unwrap_or(port); host = h.to_string(); }
    if host.is_empty() || token.is_empty() { return Err("Type the server address and the 6-digit code.".into()); }
    if port == 0 { port = 47601; }
    Ok(Target { name, host, port, secure, token })
}
pub fn is_pair_code(t: &str) -> Option<String> { let code: String = t.chars().filter(|c| !c.is_whitespace()).collect(); (code.len() == 6 && code.chars().all(|c| c.is_ascii_digit())).then_some(code) }

fn idle(x: &Value) -> bool { !x.t("available") || x.s("state") == "idle" }
fn body_err(b: &[u8], fallback: &str) -> String { net::err_of(b).unwrap_or_else(|| fallback.to_string()) }

impl App {
    // ── state, settings ──
    pub async fn refresh(self: &Arc<Self>) -> Value { self.refresh_usage(true).await; self.public_state() }
    pub fn notify_test(&self) { self.notify("Hearth", "Notifications work. This is how a request for your OK looks."); }

    /// Merges `patch` into the settings, saves, and applies the ones with an effect of their own (ringing, theme).
    pub fn settings_set(self: &Arc<Self>, patch: Value) -> Value {
        { let mut s = self.s.lock().unwrap(); if let Some(p) = patch.as_object() { for (k, v) in p { s["settings"][k] = v.clone(); } } }
        self.save();
        if patch.get("callMe").is_some() { self.set_call_me(patch.t("callMe")); }
        if patch.get("theme").is_some() || patch.get("palette").is_some() || patch.get("customAccent").is_some() { self.emit(Event::ThemeChanged); }
        self.emit(Event::State);
        self.setting_all()
    }
    pub fn set_call_me(self: &Arc<Self>, on: bool) {
        self.s.lock().unwrap()["settings"]["callMe"] = json!(on); self.save();
        self.emit(Event::RingMe(on));
        self.start_events();   // messages keep arriving either way; this only controls ringing on this computer
        if !on { for m in self.machines() { let a = self.clone(); self.rt.spawn(async move { let _ = a.relay_json(&m, "POST", "/away", Some(json!({"on": false}))).await; }); } }
        self.emit(Event::State);
    }
    pub fn notes_since(&self, last: &str, all: bool) -> Vec<Value> { notes_since(notes::NOTES, &version(), last, all) }
    pub fn app_version(&self) -> String { version() }
    pub fn set_autostart(self: &Arc<Self>, on: bool) -> Result<(), String> { sys::set_autostart(on, &self.icon_file.to_string_lossy())?; self.s.lock().unwrap()["settings"]["autostart"] = json!(on); self.save(); Ok(()) }
    pub fn set_icon(&self, png: Vec<u8>) { self.emit(Event::Icon(png)); }
    pub fn setup_paths(&self) -> Value { let p = home().join("projects"); let _ = std::fs::create_dir_all(&p); json!({"home": home().to_string_lossy(), "projects": p.to_string_lossy(), "platform": sys::platform()}) }
    pub fn quit(&self) { self.quitting.store(true, Ordering::SeqCst); self.emit(Event::Quit); }
    pub fn relaunch(&self) { self.quitting.store(true, Ordering::SeqCst); self.save_now(); self.emit(Event::Relaunch); }

    // ── this computer: pairing, hooks ──
    pub fn pair_info(&self) -> Value {
        let ri = self.relay.lock().unwrap();
        let addrs: Vec<String> = if ri.ok { Relay::addresses() } else { vec![] };
        let ts = regex::Regex::new(r"^100\.(6[4-9]|[7-9]\d|1[01]\d|12[0-7])\.").unwrap();
        let code = ri.r.as_ref().map(|r| r.pair.lock().unwrap().code.clone());
        json!({"embedded": ri.ok, "external": ri.in_use, "error": if !ri.ok && !ri.in_use { ri.error.clone() } else { None }, "code": code, "port": if ri.ok { ri.port } else { 47601 },
            "tailscale": addrs.iter().find(|x| ts.is_match(x)), "addresses": addrs, "allowRemote": self.setting("allowRemote").as_bool().unwrap_or(false), "name": hostname(),
            "claude": hearth_relay::relay::find_claude().is_some(), "hooks": sys::hooks_status(), "platform": sys::platform()})
    }
    pub fn pair_new_code(&self) -> Option<String> { self.relay.lock().unwrap().r.as_ref().map(|r| r.new_pair_code()) }
    pub fn hooks_install(&self) -> Result<bool, String> { let ri = self.relay.lock().unwrap(); let port = if ri.ok { ri.port } else { 47601 }; drop(ri); sys::hooks_install(port) }
    pub fn hooks_remove(&self) -> Result<bool, String> { sys::hooks_remove() }
    pub async fn pair_requests(self: &Arc<Self>) -> Value { self.check_pair_requests().await; self.last_pair_req.lock().unwrap().clone() }
    pub async fn pair_decide(self: &Arc<Self>, id: &Value, accept: bool) -> Result<Value, String> {
        let m = self.local().ok_or("This computer is not running Hearth yet.")?;
        self.relay_json(&m, "POST", "/pair/decision", Some(json!({"id": id, "accept": accept}))).await?;
        self.check_pair_requests().await; Ok(self.last_pair_req.lock().unwrap().clone())
    }

    // ── computers ──
    pub fn machines_public(&self) -> Vec<Value> { self.machines().iter().map(pub_m).collect() }
    pub async fn add_machine(self: &Arc<Self>, m: Value) -> Result<Vec<Value>, String> {
        let mut t = parse_target(&m)?;
        if let Some(code) = is_pair_code(&t.token) {   // the short code from the server's installer: trade it for the real key
            let base = format!("{}://{}:{}", if t.secure { "https" } else { "http" }, t.host, t.port);
            let r = self.http.post(format!("{base}/pair")).header("Content-Type", "application/json").body(json!({"code": code}).to_string()).timeout(Duration::from_secs(15)).send().await
                .map_err(|_| format!("I could not reach {}. Check the address and your internet.", t.host))?;
            let st = r.status().as_u16(); let j: Value = r.json().await.unwrap_or_else(|_| json!({}));
            if !(200..300).contains(&st) || j.s("token").is_empty() { return Err(if st == 403 { "That code is wrong or ran out. Ask the server for a new one (hearth-server code).".into() } else if !j.s("error").is_empty() { j.s("error") } else { "The server did not accept the code.".into() }); }
            t.token = j.s("token"); if t.name.is_empty() { t.name = j.s("name"); }
        }
        {
            let mut s = self.s.lock().unwrap();
            let mut l: Vec<Value> = arr(s.g("machines")).into_iter().filter(|x| x.s("host") != t.host || x.n("port") as u16 != t.port).collect();
            l.push(json!({"id": rand_hex(4), "name": if t.name.is_empty() { t.host.clone() } else { t.name.clone() }, "host": t.host, "port": t.port, "token": t.token, "secure": t.secure}));
            s["machines"] = Value::Array(l);
        }
        self.save(); self.start_events(); self.write_link();
        Ok(self.machines_public())
    }
    pub fn remove_machine(self: &Arc<Self>, id: &str) -> Vec<Value> {
        { let mut s = self.s.lock().unwrap(); let l: Vec<Value> = arr(s.g("machines")).into_iter().filter(|m| m.s("id") != id).collect(); s["machines"] = Value::Array(l); }
        self.save(); self.write_link(); self.start_events();
        self.machines_public()
    }
    pub async fn discover(&self) -> Vec<Value> { let ms = self.machines(); net::discover().await.into_iter().filter(|f| !ms.iter().any(|m| m.s("host") == f.s("host"))).collect() }
    pub fn guests(&self) -> Vec<Value> { self.guest_machines().iter().map(strip_token).collect() }
    pub fn guests_set(self: &Arc<Self>, input: &Value) -> Vec<Value> {
        let re = regex::Regex::new(r"^[A-Za-z0-9_-]{4,16}$").unwrap();
        let list: Vec<Value> = arr(input).into_iter().filter(|g| re.is_match(&g.s("share")) && g.g("key").as_str().map(|k| k.starts_with("share.")).unwrap_or(false) && g.g("hostname").is_string() && g.n("port") > 0.0)
            .map(|g| json!({"share": g.s("share"), "key": g.g("key"), "hostname": g.g("hostname"), "port": g.n("port") as u16, "secure": g.t("secure"), "host": g.s("host").chars().take(60).collect::<String>(), "title": g.s("title").chars().take(120).collect::<String>()})).collect();
        self.s.lock().unwrap()["guests"] = Value::Array(list);
        let sig = json!(self.guest_machines().iter().map(|g| json!([g.g("id"), g.g("token"), g.g("host"), g.g("port")])).collect::<Vec<_>>()).to_string();
        self.save();
        let changed = { let mut l = self.last_guest_sig.lock().unwrap(); if *l != sig { *l = sig; true } else { false } };
        if changed { self.start_events(); }
        self.guests()
    }
    /// Joins a friend's shared chat: `server` is the host's address (http(s)://...), `invite` the code from the link.
    pub async fn share_join(&self, server: &str, invite: &str, name: &str) -> Result<Value, String> {
        let u = url::Url::parse(server).map_err(|_| "Not a server address")?;
        if !["http", "https"].contains(&u.scheme()) { return Err("Not a server address".into()); }
        let secure = u.scheme() == "https";
        let m = json!({"name": "the host", "host": u.host_str().unwrap_or(""), "port": u.port().unwrap_or(if secure { 443 } else { 80 }), "secure": secure, "token": ""});
        let r = self.relay_json(&m, "POST", "/shares/join", Some(json!({"invite": invite, "name": name.chars().take(40).collect::<String>()}))).await?;
        Ok(json!({"share": r.g("share"), "key": r.g("key"), "session": r.g("session"), "title": r.g("title"), "host": r.g("host"), "member": r.g("member"), "hostname": m.g("host"), "port": m.g("port"), "secure": secure}))
    }
    /// Any call to a computer's relay (the pages' generic `cm.relay`).
    pub async fn relay(&self, machine: &str, method: &str, path: &str, body: Option<Value>) -> Result<Value, String> { let m = self.need(machine)?; self.relay_json(&m, method, path, body).await }
    pub async fn stats(&self, machine: &str, days: f64) -> Result<Value, String> { let m = self.need(machine)?; self.relay_json(&m, "GET", &format!("/stats?days={}", if days > 0.0 { days } else { 30.0 }), None).await }
    pub async fn stop(&self, machine: &str, session: &Value) -> Result<Value, String> { match self.machine(machine) { Some(m) => self.relay_json(&m, "POST", "/stop", Some(json!({"session": session}))).await, None => Ok(json!(false)) } }

    // ── hub ──
    pub async fn hub_status(&self) -> Value {
        let (server, local) = (self.server(), self.local());
        async fn get(a: &App, m: &Option<Value>, p: &str) -> Option<Value> { match m { Some(m) => a.relay_json(m, "GET", p, None).await.ok(), None => None } }
        let (ss, sync, lsync) = tokio::join!(get(self, &server, "/status"), get(self, &server, "/sync"), get(self, &local, "/sync"));
        json!({"server": server.as_ref().map(|s| json!({"name": s.g("name"), "host": s.g("host"), "online": ss.is_some(), "laptop": ss.as_ref().map(|x| x.g("laptop").clone()), "listeners": ss.as_ref().map(|x| x.n("listeners")).unwrap_or(0.0), "queued": ss.as_ref().map(|x| x.n("queued")).unwrap_or(0.0), "sync": sync})),
            "local": local.as_ref().map(|l| json!({"name": l.g("name"), "sync": lsync})), "me": hostname()})
    }
    pub async fn hub_phone_code(&self) -> Result<Value, String> {
        let server = self.server().ok_or("Connect your server first.")?;
        let r = self.relay_json(&server, "POST", "/pair/new", Some(json!({}))).await?;
        Ok(json!({"code": r.g("code"), "address": server.g("host"), "expiresIn": r.g("expiresIn")}))
    }

    // ── pending projects ──
    pub async fn projects_pending(self: &Arc<Self>) -> Value { self.check_pending().await; self.last_pend.lock().unwrap().clone() }
    /// `names`: a list of project names, or the string "all".
    pub async fn projects_decide(self: &Arc<Self>, accept: bool, names: &Value) -> Result<Value, String> {
        let m = self.local().ok_or("This computer is not running Hearth yet.")?;
        let body = if names == &json!("all") { json!({"all": true}) } else { json!({"names": names}) };
        self.relay_json(&m, "POST", if accept { "/sync/projects/accept" } else { "/sync/projects/reject" }, Some(body)).await?;
        self.check_pending().await; Ok(self.last_pend.lock().unwrap().clone())
    }

    // ── support ──
    pub async fn support_call(&self, method: &str, p: &str, body: Option<Value>, auth: bool) -> Result<Value, String> {
        let url = std::env::var("HEARTH_SUPPORT_URL").unwrap_or_else(|_| SUPPORT_URL_DEFAULT.into()) + p;
        let mut rq = self.http.request(reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET), url).header("Content-Type", "application/json").timeout(Duration::from_secs(20));
        let tok = self.setting("supportToken").as_str().unwrap_or("").to_string();
        if auth && !tok.is_empty() { rq = rq.bearer_auth(tok); }
        if let Some(b) = body { rq = rq.body(b.to_string()); }
        let r = rq.send().await.map_err(|_| "Could not reach support. Check your internet connection and try again.".to_string())?;
        let st = r.status().as_u16();
        let j: Value = r.json().await.unwrap_or_else(|_| json!({}));
        if st == 401 && auth { self.clear_support_login(); }
        if !(200..300).contains(&st) { return Err(if j.s("error").is_empty() { format!("Support said {st}") } else { j.s("error") }); }
        Ok(j)
    }
    fn clear_support_login(&self) { let mut s = self.s.lock().unwrap(); s["settings"]["supportToken"] = json!(""); s["settings"]["supportEmail"] = json!(""); }
    pub fn support_state(&self) -> Value { let s = self.setting_all(); json!({"email": if s.t("supportToken") { s.s("supportEmail") } else { String::new() }, "signedIn": s.t("supportToken"), "meta": {"app": "Desktop", "version": version(), "platform": sys::os_text()}}) }
    pub async fn support_start(&self, email: &str) -> Result<Value, String> { self.support_call("POST", "/auth/start", Some(json!({"email": email})), false).await }
    pub async fn support_verify(self: &Arc<Self>, email: &str, code: &str) -> Result<Value, String> {
        let j = self.support_call("POST", "/auth/verify", Some(json!({"email": email, "code": code})), false).await?;
        { let mut s = self.s.lock().unwrap(); s["settings"]["supportToken"] = j.g("token").clone(); s["settings"]["supportEmail"] = j.g("email").clone(); }
        self.save(); Ok(json!({"email": j.g("email")}))
    }
    pub async fn support_logout(self: &Arc<Self>) { let _ = self.support_call("POST", "/auth/logout", Some(json!({})), true).await; self.clear_support_login(); self.save(); }

    // ── moving a project between this computer and the server ──
    async fn export_import(&self, from: &Value, to: &Value, from_cwd: &str, to_cwd: &str, read_err: &str, copy_err: &str) -> Result<(), String> {
        let res = net::request(&self.long, from, "GET", &format!("/projects/export?cwd={}", enc(from_cwd)), None, Some(120000)).await?;
        if res.status().as_u16() != 200 { return Err(read_err.into()); }
        let buf = res.bytes().await.map_err(|e| net::err_text(&e))?.to_vec();
        let imp = net::request(&self.long, to, "POST", &format!("/projects/import?cwd={}&from={}", enc(to_cwd), enc(from_cwd)), Some((buf, "application/gzip")), Some(600000)).await?;
        let st = imp.status().as_u16(); let ib = imp.bytes().await.unwrap_or_default();
        if st != 200 { return Err(body_err(&ib, copy_err)); }
        Ok(())
    }
    async fn newest_session(&self, m: &Value, cwd: &str) -> Value {
        let list = self.relay_json(m, "GET", "/sessions", None).await.unwrap_or(json!([]));
        arr(&list).into_iter().filter(|s| s.s("cwd") == cwd).max_by(|a, b| a.n("mtime").partial_cmp(&b.n("mtime")).unwrap()).unwrap_or(Value::Null)
    }
    async fn wait_sync<F, Fut>(&self, deadline_min: u64, mut done: F, late: &str, mut say: impl FnMut(&Value)) -> Result<(), String> where F: FnMut() -> Fut, Fut: std::future::Future<Output = (bool, Value)> {
        let end = hearth_relay::util::now_ms() + deadline_min as f64 * 60000.0;
        loop {
            let (ok, info) = done().await;
            if ok { return Ok(()); }
            if hearth_relay::util::now_ms() > end { return Err(late.into()); }
            say(&info);
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    }
    /// Moves a project into this computer's projects folder, waits for it to sync to the server, and copies its chats over.
    pub async fn project_move(self: &Arc<Self>, local_id: &str, server_id: &str, cwd: &str, name: &str) -> Result<Value, String> {
        let (local, server) = match (self.machine(local_id), self.machine(server_id)) { (Some(l), Some(s)) => (l, s), _ => return Err("Unknown computer".into()) };
        let say = |t: String| self.emit(Event::MoveProgress(t));
        say("Moving the project into your projects folder…".into());
        let mv = self.relay_json(&local, "POST", "/projects/move", Some(json!({"cwd": cwd, "name": name}))).await?;
        let new_path = mv.s("path");
        let info = self.relay_json(&server, "GET", "/projects", None).await?;
        if info.s("root").is_empty() { return Err("That server has no projects folder.".into()); }
        let server_path = format!("{}/{}", info.s("root").trim_end_matches('/'), basename(&new_path));
        let (a, l2, s2, sp) = (self.clone(), local.clone(), server.clone(), server_path.clone());
        self.wait_sync(25, || { let (a, l, s, sp) = (a.clone(), l2.clone(), s2.clone(), sp.clone()); async move {
            let (ls, ss, pl) = tokio::join!(a.relay_json(&l, "GET", "/sync", None), a.relay_json(&s, "GET", "/sync", None), a.relay_json(&s, "GET", "/projects", None));
            let (ls, ss, pl) = (ls.unwrap_or(json!({})), ss.unwrap_or(json!({})), pl.unwrap_or(json!({"projects": []})));
            let there = arr(pl.g("projects")).iter().any(|p| p.s("path") == sp);
            let ok = there && idle(&ls) && (!ss.t("available") || (ss.s("state") == "idle" && !ss.t("needFiles")));
            (ok, json!({"left": ss.n("needFiles") + ls.n("needFiles")}))
        } }, "Syncing is taking long. The project is moved and will show up on the server when syncing finishes.", |i| { let left = i.n("left"); say(format!("Syncing the files to the server…{}", if left > 0.0 { format!(" {left} files left") } else { String::new() })); }).await?;
        say("Copying your chats to the server…".into());
        self.export_import(&local, &server, &new_path, &server_path, "Could not read the chats of this project.", "Could not copy the chats.").await?;
        let session = self.newest_session(&server, &server_path).await;
        Ok(json!({"cwd": server_path, "newPath": new_path, "session": session}))
    }
    /// Brings a project of the server to this computer: waits for its files, copies the chats.
    pub async fn project_continue(self: &Arc<Self>, server_id: &str, local_id: &str, server_cwd: &str) -> Result<Value, String> {
        let (local, server) = match (self.machine(local_id), self.machine(server_id)) { (Some(l), Some(s)) => (l, s), _ => return Err("Unknown computer".into()) };
        let say = |t: &str| self.emit(Event::MoveProgress(t.to_string()));
        let st = self.relay_json(&local, "GET", "/status", None).await?;
        if st.s("moveRoot").is_empty() { return Err("This computer has no projects folder set up.".into()); }
        let local_cwd = format!("{}/{}", st.s("moveRoot").trim_end_matches('/'), basename(server_cwd));
        say("Checking that the files are on this computer…");
        let (a, l2, s2, lc) = (self.clone(), local.clone(), server.clone(), local_cwd.clone());
        self.wait_sync(20, || { let (a, l, s, lc) = (a.clone(), l2.clone(), s2.clone(), lc.clone()); async move {
            let q = format!("/projects/exists?cwd={}", enc(&lc));
            let (ex, ss, ls) = tokio::join!(a.relay_json(&l, "GET", &q, None), a.relay_json(&s, "GET", "/sync", None), a.relay_json(&l, "GET", "/sync", None));
            (ex.map(|e| e.t("exists")).unwrap_or(false) && idle(&ss.unwrap_or(json!({}))) && idle(&ls.unwrap_or(json!({}))), Value::Null)
        } }, "The files are still syncing to this computer. Try again in a moment.", |_| say("Waiting for the project files to sync…")).await?;
        say("Copying the chat to this computer…");
        self.export_import(&server, &local, server_cwd, &local_cwd, "Could not read the chats of this project on the server.", "Could not copy the chat.").await?;
        let session = self.newest_session(&local, &local_cwd).await;
        Ok(json!({"cwd": local_cwd, "session": session}))
    }

    // ── chats: run a message, files, voice ──
    /// Asks a relay to run a message and passes every record of the run on as [`Event::Stream`]; a dropped connection joins the run again (hearth-client).
    /// Returns the stream id (`sid`) the records carry.
    pub fn send(self: &Arc<Self>, machine: &str, session: &str, text: Value, mode: Value, voice: bool, sid: Option<String>, extra: Value) -> Result<String, String> {
        use futures_util::StreamExt;
        let m = self.need(machine)?;
        let sid = sid.filter(|s| !s.is_empty()).unwrap_or_else(|| rand_hex(4));
        let mut body = json!({"text": text, "mode": mode, "brief": voice});
        if let Some(x) = extra.as_object() { for (k, v) in x { if !v.is_null() { body[k] = v.clone(); } } }
        let (a, sid2, session) = (self.clone(), sid.clone(), session.to_string());
        self.rt.spawn(async move {
            let mut ev = hearth_client::stream::run(to_machine(&m), session, body);
            while let Some(r) = ev.next().await {
                let mut o = r.raw; o["sid"] = json!(sid2.clone());
                a.emit(Event::Stream { sid: sid2.clone(), rec: o });
            }
        });
        Ok(sid)
    }
    /// Uploads bytes into a chat's folder (`session` given) or a project folder (`cwd`); returns the path on that computer.
    pub async fn upload(&self, machine: &str, session: &str, cwd: &str, name: &str, bytes: Vec<u8>) -> Result<Value, String> {
        let m = self.need(machine)?;
        let q = if !session.is_empty() { format!("session={}", enc(session)) } else { format!("cwd={}", enc(cwd)) };
        let res = net::request(&self.long, &m, "POST", &format!("/upload?{q}&name={}", enc(name)), Some((bytes, "application/octet-stream")), Some(120000)).await?;
        let st = res.status().as_u16(); let b = res.bytes().await.unwrap_or_default();
        if st != 200 { return Err(body_err(&b, "Upload failed")); }
        Ok(serde_json::from_slice::<Value>(&b).map(|v| v.g("path").clone()).unwrap_or(Value::Null))
    }
    /// A file from this computer (picked or dropped on the window) uploaded the same way.
    pub async fn upload_path(&self, machine: &str, session: &str, cwd: &str, file: &Path) -> Result<Value, String> {
        let name = file.file_name().map(|n| n.to_string_lossy().to_string()).ok_or("Not a file")?;
        let bytes = tokio::fs::read(file).await.map_err(|e| e.to_string())?;
        self.upload(machine, session, cwd, &name, bytes).await
    }
    pub async fn voice_status(&self, machine: &str) -> Result<Value, String> { match self.machine(machine) { Some(m) => self.relay_json(&m, "GET", "/voice/status", None).await, None => Ok(json!({"stt": false, "tts": false})) } }
    /// Speech to text: a WAV recording in, the words out.
    pub async fn voice_stt(&self, machine: &str, wav: Vec<u8>) -> Result<String, String> {
        let m = self.need(machine)?;
        let res = net::request(&self.long, &m, "POST", "/voice/stt", Some((wav, "audio/wav")), Some(90000)).await?;
        let st = res.status().as_u16(); let b = res.bytes().await.unwrap_or_default();
        if st != 200 { return Err(body_err(&b, "Speech recognition failed")); }
        Ok(serde_json::from_slice::<Value>(&b).map(|v| v.s("text")).unwrap_or_default())
    }
    /// Text to speech: audio bytes (WAV) out.
    pub async fn voice_tts(&self, machine: &str, text: &str, lang: &str) -> Result<Vec<u8>, String> {
        let m = self.need(machine)?;
        let res = net::request(&self.long, &m, "POST", "/voice/tts", Some((json!({"text": text, "lang": lang}).to_string().into_bytes(), "application/json")), Some(60000)).await?;
        let st = res.status().as_u16(); let b = res.bytes().await.map_err(|e| net::err_text(&e))?;
        if st != 200 { return Err(body_err(&b, "Speech synthesis failed")); }
        Ok(b.to_vec())
    }
    /// The better local voices (Linux): runs the installer script and reports each line as [`Event::VoiceInstall`] while it works.
    pub async fn voice_install(self: &Arc<Self>) -> Result<Value, String> {
        if !cfg!(target_os = "linux") { return Err("The extra voices are for Linux. On this system Hearth uses the voices of your computer.".into()); }
        if self.voice_job.swap(true, Ordering::SeqCst) { return Ok(json!({"running": true})); }
        let a = self.clone(); let send = move |d: Value| a.emit(Event::VoiceInstall(d));
        let mut script: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../relay/install_voice.sh");
        if !script.exists() {
            send(json!({"line": "Downloading the voice installer…"}));
            script = self.dir.join("install-voice.sh");
            let r = self.http.get("https://raw.githubusercontent.com/veelvoer/hearth/main/relay/install_voice.sh").timeout(Duration::from_secs(30)).send().await;
            let text = match r { Ok(r) if r.status().is_success() => r.text().await.ok(), _ => None };
            match text { Some(t) => { let _ = std::fs::write(&script, t); } None => { self.voice_job.store(false, Ordering::SeqCst); return Err("Could not download the voice installer. Check your internet connection.".into()); } }
        }
        let child = hearth_relay::util::tcmd("bash").arg(&script).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn();
        let mut child = match child { Ok(c) => c, Err(e) => { self.voice_job.store(false, Ordering::SeqCst); return Err(e.to_string()); } };
        use tokio::io::AsyncReadExt;
        fn feed(send: &dyn Fn(Value), b: &[u8]) { for l in String::from_utf8_lossy(b).split(['\n', '\r']).map(str::trim).filter(|l| !l.is_empty()) { send(json!({"line": l.chars().take(160).collect::<String>()})); } }
        let (mut out, mut err) = (child.stdout.take().unwrap(), child.stderr.take().unwrap());
        let s2 = send.clone();
        self.rt.spawn(async move { let mut b = [0u8; 4096]; while let Ok(n) = err.read(&mut b).await { if n == 0 { break; } feed(&s2, &b[..n]); } });
        let a = self.clone();
        self.rt.spawn(async move {
            let mut b = [0u8; 4096]; while let Ok(n) = out.read(&mut b).await { if n == 0 { break; } feed(&send, &b[..n]); }
            let ok = child.wait().await.map(|s| s.success()).unwrap_or(false);
            a.voice_job.store(false, Ordering::SeqCst);
            send(json!({"done": true, "ok": ok}));
        });
        Ok(json!({"running": true}))
    }

    // ── system ──
    pub fn open_path(&self, p: &str) { if !p.is_empty() { sys::open(p); } }
    pub fn open_external(&self, u: &str) { if u.starts_with("http://") || u.starts_with("https://") { sys::open(u); } }
    pub fn terminal_open(&self, cwd: &str, sid: &str) -> Result<(), String> { sys::terminal(cwd, sid) }
    /// The folder dialog (the desktop's own through the portal on Linux). None = cancelled.
    pub async fn pick_folder(&self) -> Option<String> { rfd::AsyncFileDialog::new().set_title("Choose a folder").pick_folder().await.map(|f| f.path().to_string_lossy().to_string()) }
    /// The file dialog for attaching files. Empty = cancelled.
    pub async fn pick_files(&self) -> Vec<PathBuf> { rfd::AsyncFileDialog::new().set_title("Attach files").pick_files().await.map(|v| v.into_iter().map(|f| f.path().to_path_buf()).collect()).unwrap_or_default() }
}

/// A PNG data URL (what a canvas gives) as bytes.
#[allow(dead_code)]
pub fn data_url_bytes(url: &str) -> Option<Vec<u8>> { base64::engine::general_purpose::STANDARD.decode(url.split_once("base64,")?.1).ok() }

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn versions() {
        assert_eq!(ver_cmp("1.10.0", "1.9.9"), std::cmp::Ordering::Greater); assert_eq!(ver_cmp("1.2", "1.2.0"), std::cmp::Ordering::Equal); assert_eq!(ver_cmp("1.0.1", "1.1.0"), std::cmp::Ordering::Less);
    }
    #[test] fn notes_filtering() {
        let l = [("1.38.0", "a"), ("1.39.0", "b"), ("1.40.0", "c"), ("1.41.0", "future")];
        assert_eq!(notes_since(&l, "1.40.0", "", false).len(), 1);   // first run after an update: only this version
        let v = notes_since(&l, "1.40.0", "1.38.0", false); assert_eq!(v.iter().map(|x| x.s("version")).collect::<Vec<_>>(), ["1.40.0", "1.39.0"]);
        assert_eq!(notes_since(&l, "1.40.0", "1.40.0", false).len(), 0);
    }
    #[test] fn targets() {
        let t = parse_target(&json!({"host": "https://srv.example.com", "token": "123 456"})).unwrap(); assert_eq!((t.host.as_str(), t.port, t.secure), ("srv.example.com", 443, true));
        assert_eq!(is_pair_code(&t.token).as_deref(), Some("123456"));
        let t = parse_target(&json!({"host": "192.168.1.5:4000", "token": "abc"})).unwrap(); assert_eq!((t.host.as_str(), t.port, t.secure), ("192.168.1.5", 4000, false));
        let t = parse_target(&json!({"host": "hearth://pair?url=https%3A%2F%2Fa.b%3A8443&token=KEY&name=Home"})).unwrap(); assert_eq!((t.host.as_str(), t.port, t.secure, t.token.as_str(), t.name.as_str()), ("a.b", 8443, true, "KEY", "Home"));
        assert!(parse_target(&json!({"host": "x", "token": ""})).is_err()); assert!(parse_target(&json!({"host": "", "token": "1"})).is_err());
        assert_eq!(parse_target(&json!({"host": "h", "token": "t"})).unwrap().port, 47601);
        assert!(is_pair_code("12345a").is_none());
    }
    #[test] fn data_urls() { assert_eq!(data_url_bytes("data:image/png;base64,aGk="), Some(b"hi".to_vec())); assert_eq!(data_url_bytes("nope"), None); }
}

// ── updates (the updater is in upd.rs) ──
impl App {
    pub fn update_run(self: &Arc<Self>, want: Value) { let a = self.clone(); self.rt.spawn(async move { a.upd.run(&a, if want.is_object() { want } else { json!({}) }).await; }); }
    pub async fn update_check(self: &Arc<Self>) -> Value { let det = self.upd.details(self).await; let a = self.clone(); self.rt.spawn(async move { a.upd.check(&a, true).await; }); det }
    pub async fn update_later(self: &Arc<Self>) { self.upd.later(self).await; }
    pub async fn update_status(&self) -> Value { self.upd.status(self).await }
}
