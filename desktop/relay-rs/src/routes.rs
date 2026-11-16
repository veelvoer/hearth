//! Every HTTP route of the relay. Port of the request handler of server.js (plus sharesHttp and guestHttp).
use crate::relay::*;
use crate::util::*;
use axum::body::Body;
use axum::http::{HeaderMap, Response, StatusCode};
use bytes::Bytes;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;

pub type Resp = Response<Body>;
/// Set by the watchdog test: the whole server acts stuck.
pub static HUNG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub struct Req { pub method: String, pub parts: Vec<String>, pub q: Vec<(String, String)>, pub headers: HeaderMap, pub ip: String, pub local_direct: bool, pub body: std::sync::Mutex<Option<Body>>, pub gz: bool, pub url: String }
impl Req {
    pub fn p(&self, i: usize) -> &str { self.parts.get(i).map(|s| s.as_str()).unwrap_or("") }
    pub fn n(&self) -> usize { self.parts.len() }
    pub fn qs(&self, k: &str) -> String { qs(&self.q, k) }
    pub fn h(&self, k: &str) -> String { self.headers.get(k).and_then(|v| v.to_str().ok()).unwrap_or("").to_string() }
    pub async fn bytes(&mut self, max: usize) -> Result<Bytes, ()> {
        let b = self.body.lock().unwrap().take().unwrap_or_else(Body::empty);
        axum::body::to_bytes(b, max).await.map_err(|_| ())
    }
    pub async fn json(&mut self) -> Value { match self.bytes(10_000_000).await { Ok(b) if !b.is_empty() => serde_json::from_slice(&b).unwrap_or_else(|_| obj()), _ => obj() } }
}

pub fn json_resp(code: u16, v: &Value, gz: bool) -> Resp {
    let mut b = v.to_string().into_bytes();
    let mut rb = Response::builder().status(StatusCode::from_u16(code).unwrap_or(StatusCode::OK)).header("Content-Type", "application/json");
    if b.len() > 1024 && gz { b = gzip(&b, 6); rb = rb.header("Content-Encoding", "gzip").header("Vary", "Accept-Encoding"); }   // a long list of chats is mostly repeated words
    rb.header("Content-Length", b.len()).body(Body::from(b)).unwrap()
}
fn stream_resp(ct: &str, rx: tokio::sync::mpsc::UnboundedReceiver<Bytes>) -> Resp {
    let s = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);
    use futures_util::StreamExt;
    let s = s.map(Ok::<Bytes, std::convert::Infallible>);
    Response::builder().status(200).header("Content-Type", ct).header("Cache-Control", "no-cache").header("X-Accel-Buffering", "no").body(Body::from_stream(s)).unwrap()
}
fn bin_resp(body: Vec<u8>, headers: &[(&str, String)]) -> Resp {
    let mut rb = Response::builder().status(200).header("Content-Type", "application/octet-stream").header("Content-Length", body.len());
    for (k, v) in headers { rb = rb.header(*k, v); }
    rb.body(Body::from(body)).unwrap()
}
fn ndjson_end(lines: &[Value]) -> Resp {
    let mut s = String::new(); for l in lines { s.push_str(&l.to_string()); s.push('\n'); }
    Response::builder().status(200).header("Content-Type", "application/x-ndjson").body(Body::from(s)).unwrap()
}

impl Relay {
    pub fn reach(&self, ip: &str) -> bool { self.o.allow_remote || lan_only(ip) }
    pub fn fail_count(&self, ip: &str) -> Option<(u32, f64)> { self.failures.lock().unwrap().get(ip).copied() }
    pub fn note_fail(&self, ip: &str) -> u32 { let mut g = self.failures.lock().unwrap(); let n = g.get(ip).map(|f| f.0).unwrap_or(0) + 1; g.insert(ip.to_string(), (n, if n >= 8 { now_ms() + 15.0 * 60000.0 } else { 0.0 })); n }
    /// The owner's token. Returns the answer to send when it is not given or wrong.
    pub fn authed(&self, q: &Req) -> Option<Resp> {
        if !self.o.public_mode && !self.reach(&q.ip) { return Some(json_resp(403, &json!({"error": "local network only"}), false)); }
        let got = q.h("authorization"); let want = format!("Bearer {}", self.token);
        if got.len() == want.len() && got.bytes().zip(want.bytes()).fold(0u8, |a, (x, y)| a | (x ^ y)) == 0 { self.failures.lock().unwrap().remove(&q.ip); return None; }   // the right token always gets in
        if let Some(f) = self.fail_count(&q.ip) { if f.1 > now_ms() { return Some(json_resp(429, &json!({"error": "too many failed attempts, try again later"}), false)); } }
        let n = self.note_fail(&q.ip);
        if n >= 8 { self.log(&format!("blocked {} after {} bad tokens", q.ip, n)); }
        Some(json_resp(401, &json!({"error": "bad token"}), false))
    }
    /// A run's events as newline-separated JSON, from event [from] on; it ends when the run ends.
    pub fn attach(&self, job: &Arc<crate::jobs::Job>, from: usize) -> Resp {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Bytes>();
        {
            let mut g = job.st.lock().unwrap();
            for e in g.events.iter().skip(from) { let _ = tx.send(Bytes::from(format!("{e}\n"))); }
            if !g.done {
                let weak = tx.downgrade();
                g.listeners.push(tx);   // the stream ends when the run ends (its sender goes away)
                tokio::spawn(async move { loop { tokio::time::sleep(std::time::Duration::from_secs(15)).await; let Some(t) = weak.upgrade() else { break }; if t.send(Bytes::from_static(b"\n")).is_err() { break; } } });   // a quiet stretch must not look like a dead connection
            }
        }
        stream_resp("application/x-ndjson", rx)
    }
    fn sse(self: &Arc<Self>, since: Option<i64>) -> Resp {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Bytes>();
        let _ = tx.send(Bytes::from_static(b": hello\n\n"));
        {
            let g = self.ev.lock().unwrap();
            if let Some(s) = since { if s >= 0 { for e in g.list.iter() { if e.n("id") as i64 > s { let _ = tx.send(Bytes::from(format!("id: {}\ndata: {}\n\n", e.n("id") as i64, e))); } } } }
        }
        for e in self.pending_events() { if !(since.map(|s| s >= 0 && e.n("id") as i64 <= s).unwrap_or(false)) { let _ = tx.send(Bytes::from(format!("id: {}\ndata: {}\n\n", crate::jsu::js_string(e.g("id")), e))); } }
        for e in self.shares.pending_events() { let b = self.base_for(&e.s("session"), &e.s("cwd")); let _ = tx.send(Bytes::from(format!("data: {}\n\n", merge(&merge(&b, &e), &json!({"replay": true}))))); }
        self.ev.lock().unwrap().clients.push(tx.clone());
        tokio::spawn(async move { loop { tokio::time::sleep(std::time::Duration::from_secs(15)).await; if tx.send(Bytes::from_static(b": ping\n\n")).is_err() { break; } } });
        stream_resp("text/event-stream", rx)
    }
    fn usage_report_extra(&self) -> Vec<crate::core::FileInfo> {
        let mut extra = vec![];
        if self.hub_sync.is_some() { if let Ok(rd) = std::fs::read_dir(self.dir.join("elsewhere")) { for m in rd.flatten() { if let Ok(fs) = std::fs::read_dir(m.path()) { for f in fs.flatten() {
            let n = f.file_name().to_string_lossy().to_string(); if !n.ends_with(".jsonl") { continue; }
            if let Ok(md) = f.metadata() { extra.push(crate::core::FileInfo { id: n[..n.len() - 6].to_string(), file: f.path(), mtime: mtime_ms(&md), size: md.len() }); }
        } } } } }
        extra
    }
    fn list_projects(&self) -> Value {
        let ss = crate::core::sessions(&Default::default(), None, 300, &Default::default());
        let Some(root) = &self.root else {
            let mut seen: Vec<(String, f64)> = vec![];
            for s in &ss { let c = s.s("cwd"); if !c.is_empty() && !seen.iter().any(|x| x.0 == c) { seen.push((c, s.n("mtime"))); } }
            return json!({"root": Value::Null, "projects": seen.iter().map(|(p, m)| json!({"name": basename(p), "path": p, "mtime": num(*m)})).collect::<Vec<_>>()});
        };
        let mut by_cwd: std::collections::HashMap<String, f64> = Default::default();
        for s in &ss { let e = by_cwd.entry(s.s("cwd")).or_insert(0.0); *e = e.max(s.n("mtime")); }
        let mut out: Vec<Value> = std::fs::read_dir(root).map(|rd| rd.flatten().filter(|d| d.path().is_dir() && !d.file_name().to_string_lossy().starts_with('.')).map(|d| {
            let p = d.path().to_string_lossy().to_string(); let m = d.metadata().map(|m| mtime_ms(&m)).unwrap_or(0.0);
            json!({"name": d.file_name().to_string_lossy(), "path": p, "mtime": num(m.max(by_cwd.get(&p).copied().unwrap_or(0.0)))})
        }).collect()).unwrap_or_default();
        out.sort_by(|a, b| b.n("mtime").partial_cmp(&a.n("mtime")).unwrap_or(std::cmp::Ordering::Equal));
        json!({"root": root, "projects": out})
    }
    fn worth_listing(&self, f: &crate::core::FileInfo) -> bool { crate::chathub::describe_chat(f, self.move_root.as_deref().or(self.root.as_deref()), true).is_some() }

    /// The list of chats.
    fn sessions_list(self: &Arc<Self>) -> Value {
        let dn = crate::core::denied_count();
        { let mut sd = self.seen_denied.lock().unwrap(); if dn > *sd { *sd = dn; drop(sd); self.owner_fix.lock().unwrap().1 = 0.0; self.fix_ownership("a chat could not be read"); } }
        let run = self.running_ids(); let far = self.hub_sync.as_ref().map(|h| h.remote_running()).unwrap_or_default();
        let quiet: std::collections::HashSet<String> = self.jobs.lock().unwrap().values().filter(|j| j.is_done()).map(|j| j.id()).collect();
        let keep = |f: &crate::core::FileInfo, sm: &crate::core::Summary| -> bool {
            match &self.root { Some(root) => self.visible_session(&sm.cwd, f.mtime) && (sm.cwd.is_empty() || under(&sm.cwd, root) || crate::chathub::describe_chat(f, Some(root), true).is_some()), None => self.worth_listing(f) }
        };
        let cfg = self.read_cfg();
        let prefs_of = |id: &str| merge(&json!({"call": "off", "run": "auto", "kind": "code"}), cfg.g("prefs").g(id));
        let own: Vec<Value> = crate::core::sessions(&run, Some(&keep), 300, &quiet).into_iter().map(|x| {
            let id = x.s("id"); let p = prefs_of(&id);
            let shared = self.shares.by_session(&id).map(|s| crate::shares::Shares::live(&s).len() as i64).unwrap_or(-1);
            merge(&x, &json!({"running": run.contains(&id) || far.contains(&id), "kind": p.g("kind"), "root": p.get("root") == Some(&Value::Bool(true)), "shared": shared}))
        }).collect();
        let have: std::collections::HashSet<String> = own.iter().map(|x| x.s("id")).collect();
        let away: Vec<Value> = self.hub_sync.as_ref().map(|h| h.elsewhere(false).into_iter().filter(|e| !have.contains(&e.s("id")) && !e.s("cwd").is_empty()).map(|e| json!({"id": e.s("id"), "title": e.s("title"), "cwd": e.s("cwd"), "mtime": e.n("mtime").floor() as i64, "live": false, "busy": false, "running": far.contains(&e.s("id")), "kind": prefs_of(&e.s("id")).g("kind"), "elsewhere": e.s("machine")})).collect()).unwrap_or_default();
        let gone: std::collections::HashSet<String> = arr(cfg.g("deleted")).iter().map(vstr).collect();
        let stubs = self.read_stubs();
        let seen: std::collections::HashSet<String> = own.iter().chain(away.iter()).map(|x| x.s("id")).collect();
        let lost: Vec<Value> = stubs.iter().filter(|(id, v)| !seen.contains(*id) && crate::core::find_session(id).is_none() && self.job(id).is_none() && (self.root.as_ref().map(|r| v.s("cwd").is_empty() || under(&v.s("cwd"), r)).unwrap_or(true)))
            .map(|(id, v)| { let t = clip(&v.s("text").split_whitespace().collect::<Vec<_>>().join(" "), 70); json!({"id": id, "title": if t.is_empty() { "Interrupted chat".into() } else { t }, "cwd": v.s("cwd"), "mtime": v.n("at").floor() as i64, "live": false, "busy": false, "running": false, "kind": "code", "interrupted": true}) }).collect();
        let by_routine: std::collections::HashSet<String> = self.read_runs().iter().map(|x| x.s("session")).filter(|s| !s.is_empty()).collect();
        let mut all: Vec<Value> = own.into_iter().chain(away).chain(lost).filter(|x| !gone.contains(&x.s("id")) && !by_routine.contains(&x.s("id"))).collect();
        all.sort_by(|a, b| b.n("mtime").partial_cmp(&a.n("mtime")).unwrap_or(std::cmp::Ordering::Equal));
        Value::Array(all)
    }

    pub async fn handle(self: &Arc<Self>, mut q: Req) -> Resp {
        let gz = q.gz;
        let j = |code: u16, v: Value| json_resp(code, &v, gz);
        let (p0, p1, p2) = (q.p(0).to_string(), q.p(1).to_string(), q.p(2).to_string());
        let m = q.method.clone();
        if p0 == "health" && q.n() == 1 && m == "GET" { while HUNG.load(std::sync::atomic::Ordering::Relaxed) { tokio::time::sleep(std::time::Duration::from_millis(200)).await; } return j(200, json!({"ok": true, "boot": self.boot})); }
        if std::env::var("HEARTH_TEST").as_deref() == Ok("1") && p0 == "__hang" { HUNG.store(true, std::sync::atomic::Ordering::Relaxed); tokio::time::sleep(std::time::Duration::from_secs(60)).await; HUNG.store(false, std::sync::atomic::Ordering::Relaxed); return j(200, json!({"ok": true})); }   // stuck on purpose, for the watchdog test
        if p0 == "v1" { return self.api_http(q).await; }
        let bearer = q.h("authorization").trim_start_matches("Bearer ").to_string();
        if bearer.starts_with("share.") {   // a friend's key for one shared chat
            if !self.o.public_mode && !self.reach(&q.ip) { return j(403, json!({"error": "local network only"})); }
            if let Some(f) = self.fail_count(&q.ip) { if f.1 > now_ms() { return j(429, json!({"error": "too many failed attempts, try again later"})); } }
            return match self.shares.guest_for(&bearer) { Some((s, mem)) => self.guest_http(q, s, mem).await, None => self.authed(&q).unwrap_or_else(|| j(401, json!({"error": "bad token"}))) };
        }
        if p0 == "rooms-sync" || p0 == "shares" || (p0 == "sessions" && p2 == "share") { return self.shares_http(q).await; }
        if p0 == "tools" {
            if let Some(r) = self.authed(&q) { return r; }
            let b = if m == "GET" { obj() } else { q.json().await };
            let me = self.clone(); let okd = move |p: &str| me.ok_dir(p);
            let (code, d) = crate::tools::route(&q.parts[1..], &q.q, &b, &okd, find_claude()).await;
            return j(code, d);
        }
        if m == "GET" {
            if p0 == "info" {
                if self.o.public_mode { if let Some(r) = self.authed(&q) { return r; } } else if !self.reach(&q.ip) { return j(403, json!({"error": "local network only"})); }
                return j(200, json!({"name": self.name(), "claude": find_claude().is_some(), "root": self.root, "public": self.o.public_mode}));
            }
            if p0 == "pair" && p1 == "request" && !p2.is_empty() {
                if self.o.public_mode || !self.reach(&q.ip) { return j(403, json!({"error": "not available here"})); }
                return j(200, self.pair_poll(&p2));
            }
            if let Some(r) = self.authed(&q) { return r; }
            return self.get_route(q).await;
        }
        if m == "POST" {
            if p0 == "pair" && p1.is_empty() && self.o.public_mode {   // a phone pairs with the server using the code your desktop app showed
                if now_ms() < self.hub_pair.lock().unwrap().locked_until { return j(429, json!({"error": "too many tries, wait a minute"})); }
                let b = q.json().await; let mut hp = self.hub_pair.lock().unwrap();
                if !hp.code.is_empty() && now_ms() < hp.expires && b.s("code") == hp.code { hp.code.clear(); return j(200, json!({"token": self.token, "name": self.name()})); }
                hp.fails += 1; if hp.fails >= 5 { hp.fails = 0; hp.locked_until = now_ms() + 60000.0; }
                return j(403, json!({"error": "wrong or expired code"}));
            }
            if p0 == "pair" && p1 == "request" {   // a phone on your network asks to connect
                if self.o.public_mode || !self.reach(&q.ip) { return j(403, json!({"error": "not available here"})); }
                let b = q.json().await;
                { let mut aa = self.asked_at.lock().unwrap(); if now_ms() - aa.get(&q.ip).copied().unwrap_or(0.0) < 3000.0 { return j(429, json!({"error": "wait a moment"})); } aa.insert(q.ip.clone(), now_ms()); }
                self.pair_live();
                if self.pair_pending().len() >= 4 { return j(429, json!({"error": "too many requests waiting"})); }
                let id = rand_hex(16);
                let nm: String = b.s("name").chars().filter(|c| c.is_alphanumeric() || " ._()-".contains(*c)).take(40).collect();
                self.pair_reqs.lock().unwrap().insert(id.clone(), PairReq { name: if nm.is_empty() { "A phone".into() } else { nm }, ip: q.ip.trim_start_matches("::ffff:").to_string(), at: now_ms(), status: "pending".into(), delivered: false });
                return j(200, json!({"id": id}));
            }
            if p0 == "pair" && p1.is_empty() {   // desktop app only: the 6-digit code on screen is the proof
                if self.o.public_mode || !self.reach(&q.ip) { return j(403, json!({"error": "not available here"})); }
                if now_ms() < self.pair.lock().unwrap().locked_until { return j(429, json!({"error": "too many tries, wait a minute"})); }
                let b = q.json().await;
                if b.s("code") == self.pair.lock().unwrap().code { self.new_pair_code(); return j(200, json!({"token": self.token, "name": hostname()})); }
                let lock = { let mut p = self.pair.lock().unwrap(); p.fails += 1; p.fails >= 5 };
                if lock { self.new_pair_code(); self.pair.lock().unwrap().locked_until = now_ms() + 60000.0; }
                return j(403, json!({"error": "wrong code"}));
            }
            if p0 == "mcp" && p1 == "hub" { if !q.local_direct { return j(403, json!({"error": "local only"})); } let b = q.json().await; return j(200, self.mcp_hub(b).await); }
            if p0 == "question" { if !q.local_direct { return j(403, json!({"error": "local only"})); } let b = q.json().await; return j(200, self.question(b).await); }
            if p0 == "hook" || p0 == "call" {   // from scripts on this machine only, never through the proxy
                if !q.local_direct { return j(403, json!({"error": "local only"})); }
                let b = q.json().await;
                if p0 == "hook" { return j(200, self.handle_hook(b).await); }
                let mut sid = b.s("session"); let cwd = b.s("cwd");
                if sid.is_empty() && !cwd.is_empty() { sid = crate::core::sessions(&Default::default(), None, 300, &Default::default()).into_iter().find(|x| x.s("cwd") == cwd).map(|x| x.s("id")).unwrap_or_default(); }
                let base = self.base_for(&sid, &cwd);
                let t = b.s("text");
                self.publish(merge(&base, &json!({"kind": "done", "call": true, "text": clip(if t.is_empty() { "Claude is calling" } else { &t }, 200), "seconds": 0})));
                return j(200, json!({"listeners": self.clients()}));
            }
            if let Some(r) = self.authed(&q) { return r; }
            return self.post_route(q).await;
        }
        j(404, json!({"error": "not found"}))
    }

    fn pair_live(&self) { let mut g = self.pair_reqs.lock().unwrap(); g.retain(|_, r| !(now_ms() - r.at > 120000.0 || (r.delivered && now_ms() - r.at > 30000.0))); }
    fn pair_pending(&self) -> Vec<Value> { self.pair_live(); self.pair_reqs.lock().unwrap().iter().filter(|(_, r)| r.status == "pending").map(|(id, r)| json!({"id": id, "name": r.name, "ip": r.ip, "at": num(r.at)})).collect() }
    fn pair_poll(&self, id: &str) -> Value {
        self.pair_live();
        let mut g = self.pair_reqs.lock().unwrap();
        let Some(r) = g.get_mut(id) else { return json!({"status": "expired"}) };
        if r.status == "accepted" && !r.delivered {
            r.delivered = true;
            let server = crate::agent::link(self).map(|l| json!({"url": l.url, "token": l.token, "name": if l.name.is_empty() { "Server".into() } else { l.name }})).unwrap_or(Value::Null);
            return json!({"status": "accepted", "token": self.token, "name": hostname(), "server": server});
        }
        json!({"status": if r.status == "accepted" { "done".to_string() } else { r.status.clone() }})
    }
    fn proxy_voice_none(&self, get: bool) -> Resp { if get { json_resp(200, &json!({"stt": false, "tts": false, "stt_engine": "", "tts_engine": ""}), false) } else { json_resp(503, &json!({"error": "voice engine not running"}), false) } }
    /// Speech-to-text and text-to-speech come from an optional local engine on another port.
    pub async fn proxy_voice(&self, mut q: Req) -> Resp {
        let get = q.method == "GET";
        let Some((port, tf)) = self.o.voice.clone() else { return self.proxy_voice_none(get) };
        let tok = std::fs::read_to_string(&tf).map(|s| s.trim().to_string()).unwrap_or_default();
        let body = q.bytes(50_000_000).await.unwrap_or_default();
        let mut rb = self.http.request(q.method.parse().unwrap_or(reqwest::Method::GET), format!("http://127.0.0.1:{port}{}", q.url)).bearer_auth(tok).timeout(std::time::Duration::from_secs(120));
        if !q.h("content-type").is_empty() { rb = rb.header("content-type", q.h("content-type")); }
        if !body.is_empty() { rb = rb.body(body); }
        match rb.send().await {
            Ok(r) => { let st = r.status().as_u16(); let ct = r.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("application/json").to_string(); let b = r.bytes().await.unwrap_or_default(); Response::builder().status(st).header("Content-Type", ct).header("Content-Length", b.len()).body(Body::from(b)).unwrap() }
            Err(_) => self.proxy_voice_none(get),
        }
    }

    async fn get_route(self: &Arc<Self>, q: Req) -> Resp {
        let gz = q.gz; let j = |code: u16, v: Value| json_resp(code, &v, gz);
        let (p0, p1, p2) = (q.p(0).to_string(), q.p(1).to_string(), q.p(2).to_string()); let n = q.n();
        let qn = |k: &str| q.qs(k).parse::<f64>().ok().filter(|x| x.is_finite());
        match (p0.as_str(), p1.as_str()) {
            ("status", _) => return j(200, json!({"away": self.away(), "build": self.build, "version": crate::VERSION, "canSelfUpdate": self.o.self_update, "listeners": self.clients(), "root": self.root, "allowBypass": self.o.allow_bypass, "moveRoot": self.move_root, "laptop": crate::agent::laptop_online(self), "queued": self.hub.lock().unwrap().work.len()})),
            ("voice", "status") => return self.proxy_voice(q).await,
            ("usage-report", _) => { let d = qn("days"); let days = if d == Some(0.0) { 0 } else { d.unwrap_or(30.0).clamp(1.0, 365.0) as i64 }; let extra = self.usage_report_extra(); let r = tokio::task::spawn_blocking(move || crate::core::report(days, &extra)).await.unwrap_or_default(); return j(200, merge(&r, &json!({"scope": if self.hub_sync.is_some() { "account" } else { "device" }}))); }
            ("stats", _) => { let d = qn("days").filter(|x| *x != 0.0).unwrap_or(30.0).clamp(1.0, 90.0) as i64; return j(200, tokio::task::spawn_blocking(move || crate::core::stats(d)).await.unwrap_or_default()); }
            ("commands", _) => { let cwd = q.qs("cwd"); let c = if !cwd.is_empty() && (self.root.is_none() || self.ok_dir(&cwd)) { cwd } else { String::new() }; return j(200, crate::tools::commands(&c, find_claude().as_deref()).await); }
            ("projects", "") if n == 1 => return j(200, self.list_projects()),
            ("usage", _) => return j(200, self.usage.lock().unwrap().clone()),
            ("weekplan", _) => { self.roll_week(); return j(200, self.week_plan(qn("tz").unwrap_or(0.0))); }
            ("projects", "exists") => { let c = q.qs("cwd"); return j(200, json!({"exists": is_dir(Path::new(&c)) && (self.root.is_none() || self.ok_dir(&c))})); }
            ("sync", "") => { let f = q.qs("folder"); return j(200, crate::agent::sync_status(self, if f.is_empty() { "claude-projects" } else { &f }).await); }
            ("projects", "export") => {
                let cwd = q.qs("cwd"); let d = Self::claude_dir(&cwd);
                if cwd.is_empty() || !d.exists() { return j(404, json!({"error": "no chats for that project"})); }
                let out = crate::util::tcmd("tar").args(["-czf", "-", "-C", &d.to_string_lossy(), "."]).output().await.map(|o| o.stdout).unwrap_or_default();
                return Response::builder().status(200).header("Content-Type", "application/gzip").body(Body::from(out)).unwrap();
            }
            ("agent", "chats") if p2 == "get" => {
                let Some(h) = &self.hub_sync else { return j(404, json!({"error": "no such chat"})) };
                let Some((body, delta, mtime, rel)) = h.serve(&q.qs("id"), qn("offset").unwrap_or(0.0) as usize, &q.qs("sha")) else { return j(404, json!({"error": "no such chat"})) };
                return bin_resp(gzip(&body, 6), &[("X-Gzip", "1".into()), ("X-Delta", if delta { "1".into() } else { "0".into() }), ("X-Mtime", (mtime.floor() as i64).to_string()), ("X-Rel", percent_encoding::utf8_percent_encode(&rel, percent_encoding::NON_ALPHANUMERIC).to_string())]);
            }
            ("agent", "files") if p2 == "manifest" => { let Some(h) = &self.hub_files else { return j(404, json!({"error": "this is not the server"})) }; let m: serde_json::Map<String, Value> = h.manifest().into_iter().map(|(k, v)| (k, json!(v.map(num)))).collect(); return bin_resp(gzip(json!({"files": m}).to_string().as_bytes(), 6), &[("X-Gzip", "1".into())]); }
            ("agent", "files") if p2 == "get" => { let Some(h) = &self.hub_files else { return j(404, json!({"error": "no such file"})) }; let Some((b, mt, md)) = h.get(&q.qs("rel")) else { return j(404, json!({"error": "no such file"})) }; return bin_resp(gzip(&b, 3), &[("X-Gzip", "1".into()), ("X-Mtime", (mt as i64).to_string()), ("X-Mode", md.to_string())]); }
            ("search", "") if n == 1 => { let s = q.qs("q"); return j(200, json!({"results": tokio::task::spawn_blocking(move || crate::core::search(&s, 25)).await.unwrap_or_default()})); }
            ("chat-tags", "") if n == 1 => return j(200, json!({"tags": if self.cfg("chatTags").is_object() { self.cfg("chatTags") } else { obj() }})),
            ("admin", "doctor") => return j(200, self.doctor()),
            ("admin", "safety") => return j(200, self.safety_view(obj())),
            ("admin", "decisions") => { let t = qn("tail").filter(|x| *x != 0.0).unwrap_or(50.0).clamp(1.0, 200.0) as usize; let g = self.decisions.lock().unwrap(); let l: Vec<Value> = g.iter().skip(g.len().saturating_sub(t)).rev().cloned().collect(); return j(200, json!({"decisions": l})); }
            ("recap", _) => {
                let days = qn("days").filter(|x| *x != 0.0).unwrap_or(7.0).clamp(1.0, 31.0) as i64;
                let r = tokio::task::spawn_blocking(move || crate::core::report(days, &[])).await.unwrap_or_default();
                let t = r.g("totals").clone();
                let busiest = arr(r.g("days")).into_iter().fold(None::<Value>, |m, d| if d.n("tokens") > m.as_ref().map(|x| x.n("tokens")).unwrap_or(0.0) { Some(d) } else { m });
                return j(200, json!({"days": days, "activeDays": num(t.n("activeDays")), "prompts": num(t.n("prompts")), "sessions": num(t.n("sessions")), "tokens": num(t.n("tokens")), "cost": (t.n("cost") * 100.0).round() / 100.0, "streak": num(t.n("currentStreak")), "favorite": r.s("favorite"),
                    "busiest": busiest.filter(|b| b.n("tokens") > 0.0).map(|b| json!({"date": b.g("date"), "tokens": b.g("tokens")})).unwrap_or(Value::Null), "empty": !(t.n("prompts") > 0.0 || t.n("tokens") > 0.0)}));
            }
            ("mcp", "catalog") => return j(200, json!({"tools": MCP_CATALOG.iter().map(|t| json!({"id": t.id, "name": t.name, "about": t.about, "needs": t.needs})).collect::<Vec<_>>()})),
            ("memory", "") if n == 1 => return j(200, json!({"notes": self.mem_notes()})),
            ("templates", "") if n == 1 => return j(200, json!({"templates": if self.cfg("templates").is_array() { self.cfg("templates") } else { json!([]) }})),
            ("projects", "summary") => {
                let cwd = q.qs("cwd"); if cwd.is_empty() || !self.ok_dir(&cwd) { return j(403, json!({"error": "that project is outside the projects folder"})); }
                let c2 = cwd.clone(); let keep = move |_f: &crate::core::FileInfo, sm: &crate::core::Summary| sm.cwd == c2;
                let chats: Vec<Value> = crate::core::sessions(&Default::default(), Some(&keep), 300, &Default::default()).iter().map(|x| json!({"id": x.g("id"), "title": x.g("title"), "mtime": x.g("mtime")})).collect();
                let mut cost = 0.0; for c in chats.iter().take(60) { if let Some(f) = crate::core::find_session(&c.s("id")) { cost += crate::core::summary(&f).cost; } }
                let (mut changed, mut branch) = (Value::Null, String::new());
                if let Ok(o) = crate::util::cmd("git").args(["-C", &cwd, "status", "--porcelain", "-b"]).output() { if o.status.success() { let t = String::from_utf8_lossy(&o.stdout).to_string(); let lines: Vec<&str> = t.lines().filter(|l| !l.is_empty()).collect(); branch = lines.first().map(|l| l.trim_start_matches("## ").split("...").next().unwrap_or("").to_string()).unwrap_or_default(); changed = json!(lines.len().saturating_sub(1)); } }
                return j(200, json!({"name": basename(&cwd), "cwd": cwd, "chats": chats.len(), "lastAt": chats.first().map(|c| c.g("mtime").clone()).unwrap_or(json!(0)), "cost": (cost * 100.0).round() / 100.0, "changed": changed, "branch": branch, "recent": chats.iter().take(5).cloned().collect::<Vec<_>>(), "snapshots": crate::safety::list_snapshots(&self.dir, &cwd).len()}));
            }
            ("routines", "timeline") => { let t = qn("tail").filter(|x| *x != 0.0).unwrap_or(30.0).clamp(1.0, 100.0) as usize; return j(200, json!({"runs": self.read_runs().into_iter().take(t).collect::<Vec<_>>()})); }
            ("routines", "") if n == 1 => return j(200, json!({"routines": self.read_routines()})),
            ("admin", "logs") => { let t = qn("tail").filter(|x| *x != 0.0).unwrap_or(40.0).clamp(1.0, 300.0) as usize; return j(200, json!({"version": crate::VERSION, "lines": log_lines(t)})); }
            ("admin", "options") => return j(200, self.options_view(false, true)),
            ("fs", _) => {
                let want = { let w = q.qs("path"); if !w.is_empty() { w } else { self.root.clone().unwrap_or_else(|| home().to_string_lossy().to_string()) } };
                let Some(p) = realpath(&want).filter(|p| is_dir(Path::new(p))) else { return j(404, json!({"error": "no such folder"})) };
                if !self.ok_dir(&p) { return j(403, json!({"error": "outside the projects folder"})); }
                let hidden = q.qs("hidden") == "1";
                let Ok(rd) = std::fs::read_dir(&p) else { return j(403, json!({"error": "this folder cannot be read"})) };
                let mut names: Vec<String> = rd.flatten().filter(|d| d.path().is_dir()).map(|d| d.file_name().to_string_lossy().to_string()).filter(|n| hidden || !n.starts_with('.')).collect();
                names.sort_by_key(|a| a.to_lowercase());
                let dirs: Vec<Value> = names.iter().take(500).map(|n| { let pp = Path::new(&p).join(n); json!({"name": n, "path": pp.to_string_lossy(), "git": pp.join(".git").exists()}) }).collect();
                let up = dirname(&p);
                return j(200, json!({"path": p, "parent": if up != p && self.ok_dir(&up) { json!(up) } else { Value::Null }, "dirs": dirs, "home": home().to_string_lossy(), "root": self.root, "outside": self.outside_ok()}));
            }
            ("admin", "memory") => {
                let pct = { let v = self.cfg("memoryPct"); if v == json!(0) { 0 } else { let x = vnum(&v); if (5.0..=95.0).contains(&x) { x.round() as i64 } else { 75 } } };
                let (avail, total) = mem_info(); let mb = 1048576.0;
                let want = if pct == 0 { Value::Null } else { json!(((avail * pct as f64 / 100.0).min(total * 0.9) / mb).floor().max(256.0) as i64) };
                return j(200, json!({"pct": pct, "defaultPct": 75, "appliedMb": want, "wantedMb": want, "availableMb": (avail / mb).round() as i64, "totalMb": (total / mb).round() as i64, "usedMb": (rss_bytes() / mb).round() as i64, "canRestart": self.o.self_update}));
            }
            ("admin", "latest") => {
                return match crate::selfupdate::latest().await {
                    Ok(l) => j(200, json!({"current": crate::VERSION, "latest": l.s("version"), "newer": crate::selfupdate::newer(&l.s("version"), crate::VERSION), "notes": l.s("notes"), "canSelfUpdate": self.o.self_update})),
                    Err(e) => j(502, json!({"error": format!("Could not look on GitHub: {e}")})),
                };
            }
            ("pair", "requests") => return j(200, Value::Array(self.pair_pending())),
            ("computers", _) => return j(200, Value::Array(crate::agent::computers_list(self))),
            ("sync", "status") => return j(200, crate::agent::sync_overview(self).await),
            ("support", "list") => return j(200, Value::Array(self.read_list())),
            ("admin", "apikeys") => return j(200, json!({"keys": self.keys.list(), "presets": crate::apikeys::presets(), "models": crate::apikeys::MODELS, "efforts": crate::apikeys::EFFORTS})),
            ("projects", "trash") => {
                let tr = Path::new(self.root.as_deref().or(self.move_root.as_deref()).unwrap_or("")).join(".hearth-trash");
                let mut out: Vec<Value> = std::fs::read_dir(&tr).map(|rd| rd.flatten().filter_map(|e| { let n = e.file_name().to_string_lossy().to_string(); read_json(&tr.join(&n).join("meta.json")).map(|m| merge(&json!({"stem": n}), &m)) }).collect()).unwrap_or_default();
                out.sort_by(|a, b| b.n("at").partial_cmp(&a.n("at")).unwrap_or(std::cmp::Ordering::Equal));
                return j(200, Value::Array(out));
            }
            ("pending", _) => { let mut l = self.pending_events(); l.extend(self.shares.pending_events()); return j(200, Value::Array(l)); }
            ("sessions", "") if n == 1 => { let me = self.clone(); return j(200, tokio::task::spawn_blocking(move || me.sessions_list()).await.unwrap_or_default()); }
            ("events", _) => { let since = q.qs("since"); let since = if since.is_empty() { q.h("last-event-id") } else { since }; return self.sse(since.trim().parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| x as i64)); }
            _ => {}
        }
        if p0 == "sessions" && n >= 3 {
            let sid = p1.clone();
            match p2.as_str() {
                "resume" if n == 3 => {
                    let Some(f) = crate::core::find_session(&sid) else { return j(404, json!({"error": "This chat is not on this computer (yet)."})) };
                    let cwd = crate::core::summary(&f).cwd; let rel = match &self.root { Some(r) if !cwd.is_empty() && under(&cwd, r) => relative(r, &cwd), _ => String::new() };
                    return j(200, json!({"id": sid, "cwd": cwd, "rel": rel, "root": self.root.clone().unwrap_or_default(), "machine": self.name(), "command": format!("{}claude --resume {}", if cwd.is_empty() { String::new() } else { format!("cd {} && ", serde_json::to_string(&cwd).unwrap()) }, sid)}));
                }
                "changes" => { let cwd = crate::core::find_session(&sid).map(|f| crate::core::summary(&f).cwd).unwrap_or_default(); return j(200, if !cwd.is_empty() && self.ok_dir(&cwd) { crate::safety::git_changes(&cwd) } else { json!({"git": false, "files": [], "diff": ""}) }); }
                "snapshots" => { let cwd = crate::core::find_session(&sid).map(|f| crate::core::summary(&f).cwd).unwrap_or_default(); return j(200, if !cwd.is_empty() && self.ok_dir(&cwd) { Value::Array(crate::safety::list_snapshots(&self.dir, &cwd).iter().map(|x| json!({"id": x.g("id"), "at": x.g("at"), "label": x.g("label"), "text": x.g("text"), "files": x.g("files"), "bytes": x.g("bytes")})).collect()) } else { json!([]) }); }
                "workflow" if n == 3 => return j(200, self.wf_view(&sid)),
                "messages" => {
                    let ew = self.hub_sync.as_ref().and_then(|h| h.find_elsewhere(&sid));
                    let tail = if self.shares.by_session(&sid).is_some() { 400 } else { qn("tail").filter(|x| *x != 0.0).unwrap_or(80.0).clamp(1.0, 1000.0) as usize };
                    let waiting: Vec<Value> = self.hub.lock().unwrap().work.iter().filter(|w| w.s("session") == sid && !w.s("text").is_empty()).map(|w| json!({"role": "user", "text": match &self.root { Some(r) => w.s("text").split(crate::chathub::MARK).collect::<Vec<_>>().join(r), None => w.s("text") }, "pending": true})).collect();
                    let with_waiting = |list: Value| -> Vec<Value> {
                        let norm = |t: &str| t.split_whitespace().collect::<Vec<_>>().join(" ");
                        let mut l = arr(&list);
                        let users: Vec<String> = l.iter().filter(|m| m.s("role") == "user").map(|m| norm(&m.s("text"))).collect();
                        let seen: Vec<String> = users.iter().skip(users.len().saturating_sub(40)).cloned().collect();
                        l.extend(waiting.iter().filter(|w| !seen.contains(&norm(&w.s("text")))).cloned()); l
                    };
                    if let Some(f) = ew { let file = f.file.clone(); let m = tokio::task::spawn_blocking(move || crate::core::messages(&file, tail)).await.unwrap_or_default(); return j(200, Value::Array(with_waiting(m))); }
                    let f = crate::core::find_session(&sid);
                    if f.is_none() && self.job(&sid).is_none() { if let Some(st) = self.read_stubs().get(&sid) { return j(200, json!([{"role": "user", "text": st.s("text"), "ts": (st.n("at") / 1000.0).floor() as i64}, {"role": "meta", "text": "This run was interrupted before it could be saved (the server restarted or the connection broke). Send the message again to start it over."}])); } }
                    if let Some(f) = &f { let s = crate::core::summary(f); if !self.visible_session(&s.cwd, f.mtime) { return j(403, json!({"error": "outside the projects folder"})); } }
                    let shx = self.shares.by_session(&sid);
                    let s2 = sid.clone(); let raw = tokio::task::spawn_blocking(move || crate::core::messages_sid(&s2, tail)).await.unwrap_or_default();
                    let list = with_waiting(raw);
                    return j(200, match shx { Some(s) => self.shares.merge(&s, list), None => Value::Array(list) });
                }
                "state" => {
                    let run = self.running_ids(); let far = self.hub_sync.as_ref().map(|h| h.remote_running()).unwrap_or_default();
                    let f = crate::core::find_session(&sid);
                    let quiet = self.jobs.lock().unwrap().values().any(|j| j.is_done() && j.id() == sid);
                    let running = run.contains(&sid) || far.contains(&sid);
                    let busy = running || (!quiet && f.map(|f| (now_ms() - f.mtime) / 1000.0 < 20.0).unwrap_or(false));
                    return j(200, json!({"running": running, "busy": busy}));
                }
                "prefs" => return j(200, self.get_prefs(&sid)),
                "stream" => { let Some(job) = self.job(&sid) else { return j(404, json!({"error": "not running"})) }; return self.attach(&job, qn("from").unwrap_or(0.0).max(0.0) as usize); }
                _ => {}
            }
        }
        j(404, json!({"error": "not found"}))
    }

    pub fn options_view(&self, recheck: bool, with_deny: bool) -> Value {
        let mut v = json!({"outside": self.outside_ok(), "outsideAvailable": self.root.is_some(), "bypass": self.bypass_ok(), "bypassFixed": self.o.allow_bypass, "root": self.root_allowed(), "autoContinue": self.read_cfg().get("autoContinue") == Some(&Value::Bool(true)), "googleTasks": !self.cfg("googleTasks").s("refreshToken").is_empty(), "rootReady": self.root_ready(recheck),
            "rules": self.allow_rules().iter().map(|r| json!({"id": r.g("id"), "tool": r.g("tool"), "prefix": r.g("prefix"), "label": r.g("label")})).collect::<Vec<_>>()});
        if with_deny { set(&mut v, "denyRules", Value::Array(self.deny_rules().iter().map(|r| json!({"id": r.g("id"), "prefix": r.g("prefix"), "label": r.g("label")})).collect())); }
        else if let Some(m) = v.as_object_mut() { m.remove("outsideAvailable"); m.remove("bypassFixed"); }
        v
    }
    pub fn safety_view(&self, extra: Value) -> Value {
        let (pd, pc) = self.budget_cfg();
        merge(&extra, &json!({"plain": self.read_cfg().t("plain"), "snapshots": self.snapshots_on(), "backups": {"on": self.backups_on(), "list": crate::safety::list_backups(&self.dir)}, "budget": {"perDay": num(pd), "perChat": num(pc), "usedDay": (self.cost_last_day() * 100.0).round() / 100.0}}))
    }
    fn doctor(self: &Arc<Self>) -> Value {
        let mut checks = vec![];
        let mut add = |name: &str, ok: bool, text: String, fix: &str| { let mut c = json!({"name": name, "ok": ok, "text": text}); if !fix.is_empty() { set(&mut c, "fix", json!(fix)); } checks.push(c); };
        let cl = find_claude();
        add("Claude Code", cl.is_some(), cl.as_ref().map(|c| format!("Found at {c}")).unwrap_or_else(|| "Not found on this server".into()), if cl.is_some() { "" } else { "Install it: https://claude.com/claude-code" });
        if let Some(c) = &cl {
            let v = crate::util::cmd(c).arg("--version").output();
            let ok = v.as_ref().map(|o| o.status.success()).unwrap_or(false);
            add("Claude Code starts", ok, match &v { Ok(o) if ok => String::from_utf8_lossy(&o.stdout).trim().to_string(), Ok(o) => format!("It did not start ({})", clip(&String::from_utf8_lossy(&o.stderr), 120)), Err(e) => format!("It did not start ({})", clip(&e.to_string(), 120)) }, if ok { "" } else { "Run \"claude\" once on the server and sign in." });
        }
        let pr = self.root.clone().unwrap_or_else(|| home().to_string_lossy().to_string());
        let can_write = { let t = Path::new(&pr).join(format!(".hearth-w-{}", rand_hex(3))); let ok = std::fs::write(&t, b"").is_ok(); let _ = std::fs::remove_file(&t); ok };
        add("Projects folder", can_write, if can_write { pr.clone() } else { format!("{pr} is not writable by this server") }, if can_write { "" } else { "Give the user of this server write access to the folder." });
        if let Some(free) = free_bytes(&pr) { let gb = free / 1e9; add("Disk space", gb > 1.0, format!("{} GB free", crate::jsu::to_fixed(gb, 1)), if gb > 1.0 { "" } else { "Free some disk space: Claude cannot save chats without it." }); }
        crate::core::list_files(); let denied = crate::core::denied_count();
        add("Chats readable", denied == 0, if denied == 0 { "Every chat file can be read".into() } else { format!("{denied} chat file(s) cannot be read (made with root rights)") }, if denied == 0 { "" } else if self.root_allowed() { "They are given back automatically within seconds." } else { "Run: sudo chown -R you ~/.claude" });
        if self.root_allowed() { let rr = self.root_ready(true); add("Root chats", rr.t("ok"), if rr.t("ok") { rr.s("how") } else { clip(&{ let w = rr.s("why"); if w.is_empty() { "Not possible".into() } else { w } }, 200) }, &if rr.t("ok") { String::new() } else { rr.s("fix") }); }
        if let Some(root) = &self.root {
            let sc = crate::safety::scan_secrets(root, 30.0, 1500); let f = arr(sc.g("findings"));
            add("Secrets in projects", f.is_empty(), if f.is_empty() { format!("None found in {} recently changed files", sc.n("scanned")) } else { format!("{} possible secret(s): {}", f.len(), f.iter().take(3).map(|x| format!("{} in {}", x.s("kind"), x.s("file"))).collect::<Vec<_>>().join("; ")) }, if f.is_empty() { "" } else { "Move them out of the files (use an environment variable) and change the keys: Claude can read them." });
        }
        let probs = log_lines(300).iter().filter(|l| l.contains("problem") || l.contains("error") || l.contains("Error")).count();
        add("Recent problems", probs == 0, if probs == 0 { "None in the log".into() } else { format!("{probs} line(s) with a problem in the last log lines") }, if probs == 0 { "" } else { "Open the details of an error in a chat to read them." });
        add("Server", true, format!("Hearth {} · up {} min · {} MB", crate::VERSION, ((now_ms() - self.started) / 60000.0).round(), (rss_bytes() / 1048576.0).round()), "");
        json!({"checks": checks})
    }
    pub fn roll_week(&self) {
        let e = self.usage.lock().unwrap().g("week").n("resetsAt"); if e == 0.0 { return; }
        let mut wt = self.week_targets.lock().unwrap();
        if wt.0 == 0.0 || (e - wt.0).abs() > 6.0 * 3600e3 { wt.1.clear(); wt.0 = e; write_json_private(&self.dir.join("weekplan.json"), &json!({"end": num(wt.0), "t": wt.1})); }
    }
    pub fn week_plan(&self, tz: f64) -> Value {
        let u = self.usage.lock().unwrap().clone(); let h = self.week_hist.lock().unwrap().clone(); let t = Value::Object(self.week_targets.lock().unwrap().1.clone());
        crate::weekplan::plan(&u, &h, &t, now_ms(), tz)
    }
}

pub fn mem_info() -> (f64, f64) {
    let m = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let get = |k: &str| m.lines().find(|l| l.starts_with(k)).and_then(|l| l.split_whitespace().nth(1)).and_then(|x| x.parse::<f64>().ok()).map(|kb| kb * 1024.0);
    (get("MemAvailable:").unwrap_or(0.0), get("MemTotal:").unwrap_or(0.0))
}
pub fn rss_bytes() -> f64 { std::fs::read_to_string("/proc/self/status").ok().and_then(|s| s.lines().find(|l| l.starts_with("VmRSS:")).and_then(|l| l.split_whitespace().nth(1)).and_then(|x| x.parse::<f64>().ok())).map(|kb| kb * 1024.0).unwrap_or(0.0) }
pub fn free_bytes(p: &str) -> Option<f64> {
    #[cfg(unix)] {
        let c = std::ffi::CString::new(p).ok()?; let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(c.as_ptr(), &mut s) } == 0 { return Some(s.f_bavail as f64 * s.f_frsize as f64); }
        None
    }
    #[cfg(not(unix))] { let _ = p; None }
}
