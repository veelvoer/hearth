//! Your computers and the server: the server hands work to a laptop whenever it is online (hub side), and a computer links to a
//! server, reports there and does the work it hands over (laptop side). Port of the hub/link parts of server.js.
use crate::relay::*;
use crate::util::*;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Arc;

pub const AGENT_FRESH_MS: f64 = 45000.0;
const CLAIM_MS: f64 = 15.0 * 60000.0;

#[derive(Clone)]
pub struct Link { pub url: String, pub token: String, pub name: String }
#[derive(Default)]
pub struct LinkState {
    pub link: Option<Link>,
    pub gen: u64,
    pub agent_sync: Option<Arc<crate::chathub::ChatAgent>>,
    pub file_agent: Option<Arc<crate::filehub::FileAgent>>,
    pub pending_projects: Vec<Value>,
    pub last_epoch: i64,
    pub doing: HashSet<String>,
}

// ───────── hub side ─────────
pub fn laptop_online(r: &Relay) -> Option<String> {
    let h = r.hub.lock().unwrap();
    let mut best: Option<(String, f64)> = None;
    for (n, seen) in &h.agents { if now_ms() - seen < AGENT_FRESH_MS && best.as_ref().map(|b| *seen >= b.1).unwrap_or(true) { best = Some((n.clone(), *seen)); } }
    best.map(|b| b.0)
}
pub fn is_online(r: &Relay, name: &str) -> bool { r.hub.lock().unwrap().agents.get(name).map(|s| now_ms() - s < AGENT_FRESH_MS).unwrap_or(false) }
pub fn valid_run(v: &Value) -> bool {
    let s = vstr(v);
    ["auto", "laptop", "vps"].contains(&s.as_str()) || regex::Regex::new(r"^pc:[\w .()-]{1,60}$").unwrap().is_match(&s)
}
/// Which of your computers should run this? 'pc:Name' = that one, 'laptop'/'auto' = the one that is online. None = the server itself.
pub fn target_for(r: &Relay, run: &str) -> Option<String> {
    if let Some(n) = run.strip_prefix("pc:") { Some(n.to_string()) } else if run == "laptop" || run == "auto" { laptop_online(r) } else { None }
}
pub fn computers_list(r: &Relay) -> Vec<Value> {
    let mut l: Vec<Value> = r.hub.lock().unwrap().agents.iter().map(|(n, s)| json!({"id": format!("pc:{n}"), "name": n, "online": now_ms() - s < AGENT_FRESH_MS, "kind": "computer"})).collect();
    l.sort_by(|a, b| (b.t("online") as i32).cmp(&(a.t("online") as i32)).then(a.s("name").cmp(&b.s("name"))));
    let mut out = vec![json!({"id": "vps", "name": r.o.name.clone().unwrap_or_else(|| "Server".into()), "online": true, "kind": "server"})];
    out.extend(l); out
}
/// The work and answers waiting for one computer. Marks the work as claimed.
fn ready_for(r: &Relay, h: &mut Hub, name: &str) -> (Vec<Value>, Vec<Value>) {
    let mut out = Vec::new();
    for w in h.work.iter_mut() {
        let tgt = w.s("target");
        if !tgt.is_empty() && tgt != name { continue; }
        if w.n("claimedAt") > 0.0 && now_ms() - w.n("claimedAt") <= CLAIM_MS { continue; }
        let after = w.s("afterJob");
        if !after.is_empty() && r.job_busy(&after) { continue; }
        if !after.is_empty() { if let Some(f) = crate::core::find_session(&w.s("session")) { set(w, "chatMtime", num(f.mtime)); } if let Some(m) = w.as_object_mut() { m.remove("afterJob"); } }
        set(w, "claimedAt", json!(now_i())); set(w, "by", json!(name));
        out.push(w.clone());
    }
    (out, h.answers_for.get(name).cloned().unwrap_or_default())
}
pub fn wake(r: &Arc<Relay>) {
    let mut h = r.hub.lock().unwrap();
    let waiters = std::mem::take(&mut h.waiters);
    let mut keep = Vec::new();
    let mut changed = false;
    for w in waiters {
        if w.tx.is_closed() { continue; }
        let (work, answers) = ready_for(r, &mut h, &w.name);
        if !work.is_empty() || !answers.is_empty() || w.epoch != json!(h.chat_epoch) {
            h.answers_for.insert(w.name.clone(), vec![]); changed = true;
            let ep = h.chat_epoch;
            let _ = w.tx.send(json!({"work": work, "answers": answers, "epoch": ep}));
        } else { keep.push(w); }
    }
    h.waiters.extend(keep);
    if changed { let wk = h.work.clone(); drop(h); r.save_work(&wk); }
}
/// A linked laptop asks: any work for me? any answers? (long poll, 20 seconds)
pub async fn poll(r: &Arc<Relay>, b: &Value) -> Value {
    let name = clip(&{ let n = b.s("name"); if n.is_empty() { "laptop".into() } else { n } }, 60);
    let rx = {
        let mut h = r.hub.lock().unwrap();
        h.agents.insert(name.clone(), now_ms());
        let (work, answers) = ready_for(r, &mut h, &name);
        let ep = h.chat_epoch;
        if !work.is_empty() || !answers.is_empty() || (b.get("epoch").is_some() && *b.g("epoch") != json!(ep)) {
            h.answers_for.insert(name.clone(), vec![]);
            let wk = h.work.clone(); drop(h); r.save_work(&wk);
            return json!({"work": work, "answers": answers, "epoch": ep});
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        h.waiters.push(Waiter { name: name.clone(), epoch: b.g("epoch").clone(), tx });
        rx
    };
    match tokio::time::timeout(std::time::Duration::from_secs(20), rx).await {
        Ok(Ok(v)) => v,
        _ => { let mut h = r.hub.lock().unwrap(); h.waiters.retain(|w| !w.tx.is_closed()); h.agents.insert(name, now_ms()); json!({"work": [], "answers": [], "epoch": h.chat_epoch}) }
    }
}
/// The mcp tools of a chat on the server that hand work to a computer.
pub async fn hub_tool(r: &Arc<Relay>, q: &Value, job: Option<Arc<crate::jobs::Job>>) -> Value {
    let tool = q.s("tool");
    let on = laptop_online(r);
    if tool == "list_computers" {
        let l: Vec<Value> = computers_list(r).into_iter().filter(|c| c.s("kind") == "computer").collect();
        return json!({"text": if l.is_empty() { "No computer of the user is linked to this server.".to_string() } else { format!("The user's computers:\n{}", l.iter().map(|c| format!("- {}: {}", c.s("name"), if c.t("online") { "online" } else { "offline" })).collect::<Vec<_>>().join("\n")) }});
    }
    let mut chosen = on.clone();
    if tool == "run_on_computer" {
        let want = q.s("computer").trim().to_lowercase();
        match computers_list(r).into_iter().find(|c| c.s("kind") == "computer" && c.s("name").to_lowercase() == want) {
            Some(h) => chosen = Some(h.s("name")),
            None => return json!({"text": format!("There is no computer called \"{}\". Use list_computers to see the names.", q.s("computer"))}),
        }
    }
    if tool == "laptop_status" {
        let mine: Vec<String> = r.hub.lock().unwrap().work.iter().filter(|w| !w.t("done")).map(|w| format!("- {}: {}", { let x = w.s("rel"); if x.is_empty() { "projects".into() } else { x } }, clip(&w.s("text"), 80))).collect();
        return json!({"text": format!("{}{}", match &on { Some(n) => format!("The laptop ({n}) is online now."), None => "The laptop is offline; queued tasks run when it is next opened.".into() }, if mine.is_empty() { "\nNo tasks are waiting.".to_string() } else { format!("\nWaiting tasks:\n{}", mine.join("\n")) })});
    }
    let Some(job) = job.filter(|j| !j.sid().is_empty()) else { return json!({"text": "This only works inside a chat."}) };
    let sid = job.sid();
    let relp = r.root.as_ref().map(|root| relative(root, &job.cwd)).unwrap_or_default();
    let task = q.s("task");
    let text = clip(&match &r.root { Some(root) => task.split(root.as_str()).collect::<Vec<_>>().join(crate::chathub::MARK), None => task }, 8000);
    let mut w = json!({"id": rand_hex(6), "session": sid, "rel": relp, "text": text, "mode": "auto", "model": "", "call": r.get_prefs(&sid).s("call"), "created": now_i(), "afterJob": job.key, "fromServer": true});
    if tool == "run_on_computer" { if let Some(c) = &chosen { set(&mut w, "target", json!(c)); } }
    { let mut h = r.hub.lock().unwrap(); h.work.push(w); let wk = h.work.clone(); drop(h); r.save_work(&wk); }
    { let r2 = r.clone(); tokio::spawn(async move { tokio::time::sleep(std::time::Duration::from_millis(500)).await; wake(&r2); }); }
    let is_on = if tool == "run_on_computer" { chosen.as_deref().map(|c| is_online(r, c)).unwrap_or(false) } else { on.is_some() };
    let who = chosen.clone().unwrap_or_else(|| "your laptop".into());
    let b = r.base_for(&sid, &job.cwd);
    r.publish(merge(&b, &json!({"kind": "queued", "call": false, "text": if is_on { format!("Handed to {who}; it starts once this reply is finished.") } else { format!("Waiting for {who}: it starts when you open it.") }})));
    let cap = chosen.unwrap_or_else(|| "The laptop".into());
    json!({"text": if is_on { format!("Queued. {cap} is online and starts as soon as you finish this reply.") } else { format!("Queued. {cap} is offline; it starts automatically the next time the user opens it. They get a notification when it is done.") }})
}
pub fn queue_work(r: &Arc<Relay>, w: Value) { let mut h = r.hub.lock().unwrap(); h.work.push(w); let wk = h.work.clone(); drop(h); r.save_work(&wk); wake(r); }

// ───────── laptop side ─────────
pub fn link(r: &Relay) -> Option<Link> { r.link.lock().unwrap().link.clone() }
async fn agent_post(r: &Relay, l: &Link, p: &str, body: &Value, ms: u64) -> Result<Value, String> {
    let b = merge(&json!({"name": hostname()}), body);
    let resp = r.http.post(format!("{}{}", l.url.trim_end_matches('/'), p)).bearer_auth(&l.token).json(&b).timeout(std::time::Duration::from_millis(ms)).send().await.map_err(|e| e.to_string())?;
    let t = resp.text().await.unwrap_or_default();
    Ok(serde_json::from_str(if t.is_empty() { "{}" } else { &t }).unwrap_or_else(|_| obj()))
}
pub fn forward_event(r: &Arc<Relay>, e: &Value) {
    let Some(l) = link(r) else { return };
    if !["done", "error", "permission", "question", "resolved", "started"].contains(&e.s("kind").as_str()) { return; }
    let r2 = r.clone(); let e = e.clone();
    tokio::spawn(async move { let _ = agent_post(&r2, &l, "/agent/event", &json!({"event": e}), 10000).await; });
}
pub fn forward_usage(r: &Arc<Relay>, u: &Value) {
    let Some(l) = link(r) else { return };
    let r2 = r.clone(); let u = u.clone();
    tokio::spawn(async move { let _ = agent_post(&r2, &l, "/agent/usage", &json!({"usage": u}), 10000).await; });
}
pub fn kick(r: &Arc<Relay>, why: &str) { let a = r.link.lock().unwrap().agent_sync.clone(); if let Some(a) = a { a.kick(r.clone(), why); } }

/// The transport the chat and file agents use: the linked server, with the token.
#[derive(Clone)]
pub struct Transport { pub http: reqwest::Client, pub url: String, pub token: String }
impl Transport {
    pub async fn json(&self, p: &str, body: &Value, ms: u64) -> Result<Value, String> {
        let b = merge(&json!({"name": hostname()}), body);
        let r = self.http.post(format!("{}{}", self.url.trim_end_matches('/'), p)).bearer_auth(&self.token).json(&b).timeout(std::time::Duration::from_millis(ms)).send().await.map_err(|e| e.to_string())?;
        let t = r.text().await.unwrap_or_default();
        Ok(serde_json::from_str(if t.is_empty() { "{}" } else { &t }).unwrap_or_else(|_| obj()))
    }
    pub async fn put(&self, p: &str, gz: Vec<u8>, ms: u64) -> Result<Value, String> {
        let r = self.http.post(format!("{}{}", self.url.trim_end_matches('/'), p)).bearer_auth(&self.token).header("Content-Type", "application/octet-stream").header("X-Gzip", "1").body(gz).timeout(std::time::Duration::from_millis(ms)).send().await.map_err(|e| e.to_string())?;
        let st = r.status().as_u16(); let t = r.text().await.unwrap_or_default();
        let mut j: Value = serde_json::from_str(if t.is_empty() { "{}" } else { &t }).unwrap_or_else(|_| obj());
        if st >= 400 && j.s("error").is_empty() { set(&mut j, "error", json!(format!("server said {st}"))); }
        Ok(j)
    }
    /// (body, headers) or None for 404.
    pub async fn get(&self, p: &str, ms: u64) -> Result<Option<(Vec<u8>, reqwest::header::HeaderMap)>, String> {
        let r = self.http.get(format!("{}{}", self.url.trim_end_matches('/'), p)).bearer_auth(&self.token).timeout(std::time::Duration::from_millis(ms)).send().await.map_err(|e| e.to_string())?;
        let st = r.status().as_u16();
        if st == 404 { return Ok(None); }
        if st != 200 { return Err(format!("server said {st}")); }
        let h = r.headers().clone();
        let b = r.bytes().await.map_err(|e| e.to_string())?.to_vec();
        let body = if h.get("x-gzip").is_some() { gunzip(&b).ok_or("bad gzip")? } else { b };
        Ok(Some((body, h)))
    }
}

pub fn set_link(r: &Arc<Relay>, l: Option<Value>) {
    let mut st = r.link.lock().unwrap();
    st.link = l.filter(|l| !l.s("url").is_empty() && !l.s("token").is_empty()).map(|l| Link { url: l.s("url"), token: l.s("token"), name: clip(&l.s("name"), 60) });
    st.gen += 1;
    if let Some(a) = st.agent_sync.take() { a.stop(); }
    if let Some(f) = st.file_agent.take() { f.stop(); st.pending_projects.clear(); }
    let Some(link) = st.link.clone() else { return };
    let gen = st.gen;
    let tr = Transport { http: r.http.clone(), url: link.url.clone(), token: link.token.clone() };
    if r.hub_sync.is_none() {
        let root = r.root.clone().or_else(|| r.move_root.clone());
        let a = crate::chathub::ChatAgent::new(root.clone(), r.machine_id.clone(), hostname(), tr.clone());
        a.start(r.clone()); st.agent_sync = Some(a);
        if let Some(fr) = root { if !r.dir.join("no-filesync").exists() {
            let f = crate::filehub::FileAgent::new(fr, r.dir.join("filesync-state.json"), hostname(), tr.clone());
            f.start(r.clone()); st.file_agent = Some(f);
        } }
    }
    drop(st);
    let r2 = r.clone();
    tokio::spawn(async move { agent_loop(r2, gen, link).await; });
}
async fn agent_loop(r: Arc<Relay>, gen: u64, l: Link) {
    loop {
        { let st = r.link.lock().unwrap(); if st.gen != gen || st.link.is_none() { return; } }
        let ep = r.link.lock().unwrap().last_epoch;
        match agent_post(&r, &l, "/agent/poll", &json!({"epoch": ep}), 30000).await {
            Ok(res) => {
                if res.get("epoch").is_some() && res.n("epoch") as i64 != ep { r.link.lock().unwrap().last_epoch = res.n("epoch") as i64; kick(&r, "server-changed"); let fa = r.link.lock().unwrap().file_agent.clone(); if let Some(f) = fa { let (f, r3) = (f.clone(), r.clone()); tokio::spawn(async move { f.sync_once(&r3, "server-changed").await; }); } }
                for a in arr(res.g("answers")) {
                    if !a.s("stop").is_empty() { r.stop_job(&a.s("stop")); continue; }
                    r.resolve_pending(&a.s("req"), a.g("value").clone());
                }
                for w in arr(res.g("work")) {
                    let id = w.s("id");
                    { let mut st = r.link.lock().unwrap(); if st.doing.contains(&id) { continue; } st.doing.insert(id.clone()); }
                    let r2 = r.clone(); let l2 = l.clone();
                    tokio::spawn(async move {
                        let out = run_work(&r2, &w).await;
                        let ack = match out { Ok(()) => json!({"id": id, "ok": true, "session": w.s("session")}), Err(e) => json!({"id": id, "ok": false, "error": e, "session": w.s("session")}) };
                        let _ = agent_post(&r2, &l2, "/agent/ack", &ack, 15000).await;
                        r2.link.lock().unwrap().doing.remove(&id);
                    });
                }
            }
            Err(_) => tokio::time::sleep(std::time::Duration::from_secs(5)).await,
        }
    }
}
async fn wait_job(j: &Arc<crate::jobs::Job>) { while !j.is_done() { tokio::time::sleep(std::time::Duration::from_millis(1500)).await; } }
async fn run_work(r: &Arc<Relay>, w: &Value) -> Result<(), String> {
    let mr = r.move_root.clone().unwrap_or_else(|| home().to_string_lossy().to_string());
    let swap = |t: &str| if r.move_root.is_some() { t.split(crate::chathub::MARK).collect::<Vec<_>>().join(mr.trim_end_matches('/')) } else { t.to_string() };
    if w.t("newChat") {
        let dest = std::path::Path::new(&mr).join(w.s("rel")).to_string_lossy().to_string();
        std::fs::create_dir_all(&dest).map_err(|_| format!("Could not make the folder {dest}"))?;
        let b = r.base_for("", &dest);
        r.publish(merge(&b, &json!({"kind": "started", "call": false, "text": format!("Started on {}: {}", hostname(), clip(&w.s("text"), 140))})));
        let s = r.start_job(&json!({"text": swap(&w.s("text")), "mode": w.g("mode"), "model": w.g("model"), "effort": w.g("effort"), "cwd": dest, "kind": w.g("kind")}), "new").map_err(|e| e.1)?;
        wait_job(&s.job).await; return Ok(());
    }
    let direct = !w.s("cwd").is_empty();
    let local = if direct { w.s("cwd") } else { std::path::Path::new(&mr).join(w.s("rel")).to_string_lossy().to_string() };
    let deadline = now_ms() + 45.0 * 1000.0;
    let mut synced = direct;
    while !direct {
        if std::path::Path::new(&local).exists() { if let Some(f) = crate::core::find_session(&w.s("session")) { if f.mtime >= w.n("chatMtime") - 3000.0 { synced = true; break; } } }
        if now_ms() > deadline { break; }
        kick(r, "wait");
        tokio::time::sleep(std::time::Duration::from_secs(4)).await;
    }
    if !synced {
        // The project or the chat did not reach this computer (not shared with it, or still syncing). Never leave the task waiting
        // for nothing: run it now as a new chat in a work folder here, with what is needed to understand it.
        let dest = std::path::Path::new(&mr).join("Laptop tasks").to_string_lossy().to_string();
        std::fs::create_dir_all(&dest).map_err(|_| format!("Could not make the folder {dest}"))?;
        let b = r.base_for("", &dest);
        r.publish(merge(&b, &json!({"kind": "started", "call": false, "text": format!("Your computer started (the project is not on it, so in a work folder): {}", clip(&w.s("text"), 120))})));
        let note = "This task was handed to this computer from a chat on the server; the project folder is not on this computer, so you are in a work folder. Do exactly what the task says on this computer (adb, installs, hardware). If it needs project files, say so.\n\nTask:\n";
        let s = r.start_job(&json!({"text": format!("{note}{}", swap(&w.s("text"))), "mode": w.g("mode"), "model": w.g("model"), "effort": w.g("effort"), "cwd": dest}), "new").map_err(|e| e.1)?;
        wait_job(&s.job).await; return Ok(());
    }
    if direct && !std::path::Path::new(&local).exists() { return Err("That folder is not on this computer any more.".into()); }
    let sid = w.s("session");
    if !w.s("call").is_empty() && w.s("call") != "off" { r.set_prefs(&sid, json!({"call": w.s("call")})); }
    if w.s("kind") == "talk" { r.set_prefs(&sid, json!({"kind": "talk"})); }
    let b = r.base_for(&sid, &local);
    r.publish(merge(&b, &json!({"kind": "started", "call": false, "text": format!("Your laptop started: {}", clip(&w.s("text"), 140))})));
    let s = r.start_job(&json!({"text": swap(&w.s("text")), "mode": w.g("mode"), "model": w.g("model"), "effort": w.g("effort")}), &sid).map_err(|e| e.1)?;
    wait_job(&s.job).await;
    Ok(())
}

/// What the Sync screen shows.
pub async fn sync_overview(r: &Arc<Relay>) -> Value {
    let (asy, fag, linked, pend) = { let st = r.link.lock().unwrap(); (st.agent_sync.clone(), st.file_agent.clone(), st.link.is_some(), st.pending_projects.clone()) };
    let base = if let Some(h) = &r.hub_sync { h.status() } else if let Some(a) = &asy { merge(&a.status(), &json!({"linked": true})) } else { json!({"role": "single", "linked": linked}) };
    let files = if let Some(h) = &r.hub_files { json!({"count": h.count()}) } else if let Some(f) = &fag { f.status() } else { Value::Null };
    let st = sync_status(r, "claude-projects").await;
    merge(&base, &json!({"files": files, "pending": pend, "build": r.build, "version": crate::VERSION, "now": now_i(), "syncthing": st}))
}
/// Progress of the Syncthing folder that mirrors the projects folder, if Syncthing runs on this machine.
pub async fn sync_status(r: &Relay, folder: &str) -> Value {
    let h = home();
    let cands = [h.join(".local/state/syncthing/config.xml"), h.join(".config/syncthing/config.xml"), std::path::PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_default()).join("Syncthing/config.xml")];
    let (mut key, mut addr) = (String::new(), "127.0.0.1:8384".to_string());
    for f in cands { if let Ok(x) = std::fs::read_to_string(f) {
        key = regex::Regex::new(r"<apikey>([^<]+)<").unwrap().captures(&x).map(|c| c[1].to_string()).unwrap_or_default();
        if let Some(c) = regex::Regex::new(r"<gui[^>]*>[\s\S]*?<address>([^<]+)<").unwrap().captures(&x) { addr = c[1].to_string(); }
        if !key.is_empty() { break; }
    } }
    if key.is_empty() { return json!({"available": false}); }
    match r.http.get(format!("http://{addr}/rest/db/status?folder={}", enc(folder))).header("X-API-Key", key).timeout(std::time::Duration::from_secs(4)).send().await {
        Ok(rs) => match rs.json::<Value>().await { Ok(o) => json!({"available": true, "state": o.g("state"), "needFiles": o.g("needFiles"), "needBytes": o.g("needBytes"), "localFiles": o.g("localFiles"), "globalFiles": o.g("globalFiles")}), Err(_) => json!({"available": false}) },
        Err(_) => json!({"available": false}),
    }
}
