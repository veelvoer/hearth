//! Chat sync between your computers and the server, over the app's own HTTPS link. Port of chathub.js.
//! The server knows every chat; a computer sends it a manifest, the server answers what to push and what to pull, newest copy wins.
//! In transit the project path inside a chat is replaced by MARK, so every machine writes the chat with its own projects folder.
use crate::agent::Transport;
use crate::core::{find_session, list_files, summary, FileInfo};
use crate::util::*;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const MARK: &str = "/__hearth_projects__";
const SLACK: f64 = 1500.0;
const QUIET_MS: f64 = 20000.0;
const MAX_AGE_MS: f64 = 120.0 * 86400e3;

fn normalize(text: &str, root: Option<&str>) -> String { match root { Some(r) if r != MARK && !r.is_empty() => text.split(r).collect::<Vec<_>>().join(MARK), _ => text.to_string() } }
fn denormalize(text: &str, root: Option<&str>) -> String { match root { Some(r) if r != MARK && !r.is_empty() => text.split(MARK).collect::<Vec<_>>().join(r), _ => text.to_string() } }
fn inside(p: &str, root: &str) -> bool { !root.is_empty() && (p == root || p.starts_with(&format!("{root}/"))) }
fn is_uuid(s: &str) -> bool { s.len() == 36 && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-') }

/// A chat that is worth sharing, described for the manifest. None for junk (temp folders, deleted folders, empty chats).
pub fn describe_chat(f: &FileInfo, root: Option<&str>, need_dir: bool) -> Option<Value> {
    let s = summary(f);
    let cwd = s.cwd.clone();
    if cwd.is_empty() { return None; }
    let tmp = std::env::temp_dir().to_string_lossy().to_string();
    if ["/tmp", "/var/tmp", tmp.as_str()].iter().any(|t| inside(&cwd, t)) { return None; }
    if need_dir && !is_dir(Path::new(&cwd)) { return None; }
    if s.title == "Untitled" && f.size < 4000 { return None; }
    let rel = match root { Some(r) if inside(&cwd, r) => json!(relative(r, &cwd)), _ => Value::Null };
    Some(json!({"id": f.id, "rel": rel, "cwd": cwd, "mtime": f.mtime.floor() as i64, "size": f.size, "title": s.title, "file": f.file.to_string_lossy()}))
}
fn read_norm(file: &Path, root: Option<&str>) -> Option<Vec<u8>> { std::fs::read(file).ok().map(|b| normalize(&String::from_utf8_lossy(&b), root).into_bytes()) }
fn write_chat(dst: &Path, text: &str, mtime: f64) {
    let tmp = PathBuf::from(format!("{}.tmp-{}", dst.display(), std::process::id()));
    if let Some(d) = dst.parent() { let _ = std::fs::create_dir_all(d); }
    if std::fs::write(&tmp, text).is_ok() && std::fs::rename(&tmp, dst).is_ok() { set_mtime(dst, mtime); }
}
pub fn encode_cwd(p: &str) -> String { crate::relay::Relay::encode_cwd(p) }

// ───────── server side ─────────
pub struct HubSync {
    store: PathBuf,
    root: String,
    pub agents: Mutex<HashMap<String, Value>>,
    ew: Mutex<(f64, Vec<Value>)>,
}
impl HubSync {
    pub fn new(dir: &Path, root: &str) -> Self { HubSync { store: dir.join("elsewhere"), root: root.to_string(), agents: Mutex::new(HashMap::new()), ew: Mutex::new((0.0, vec![])) } }
    pub fn own_map(&self, need_dir: bool) -> HashMap<String, Value> {
        let mut m = HashMap::new();
        for f in list_files() { if let Some(e) = describe_chat(&f, Some(&self.root), need_dir) { if !e.g("rel").is_null() { m.entry(f.id.clone()).or_insert(e); } } }
        m
    }
    /// Chats other computers keep outside the projects folder.
    pub fn elsewhere(&self, force: bool) -> Vec<Value> {
        { let g = self.ew.lock().unwrap(); if !force && now_ms() - g.0 < 2000.0 { return g.1.clone(); } }
        let mut out = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&self.store) { for m in rd.flatten() {
            if !m.path().is_dir() { continue; }
            let mid = m.file_name().to_string_lossy().to_string();
            let machine = std::fs::read_to_string(m.path().join("_name")).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| mid.clone());
            let Ok(names) = std::fs::read_dir(m.path()) else { continue };
            for n in names.flatten() {
                let nm = n.file_name().to_string_lossy().to_string(); if !nm.ends_with(".jsonl") { continue; }
                let Ok(md) = n.metadata() else { continue };
                let f = FileInfo { id: nm[..nm.len() - 6].to_string(), file: n.path(), mtime: mtime_ms(&md), size: md.len() };
                let s = summary(&f);
                out.push(json!({"id": f.id, "file": f.file.to_string_lossy(), "mtime": f.mtime, "size": f.size, "machineId": mid, "machine": machine, "cwd": s.cwd, "title": s.title}));
            }
        } }
        *self.ew.lock().unwrap() = (now_ms(), out.clone());
        out
    }
    pub fn find_elsewhere(&self, id: &str) -> Option<FileInfo> {
        self.elsewhere(false).into_iter().find(|e| e.s("id") == id).map(|e| FileInfo { id: e.s("id"), file: PathBuf::from(e.s("file")), mtime: e.n("mtime"), size: e.n("size") as u64 })
    }
    pub fn find_elsewhere_v(&self, id: &str) -> Option<Value> { self.elsewhere(false).into_iter().find(|e| e.s("id") == id) }
    fn describe(file: &Path, r: Option<&str>) -> Value { match read_norm(file, r) { Some(b) => json!({"bytes": b.len(), "sha": sha1_hex(&b)}), None => Value::Null } }
    /// Which chats should travel? Compares an agent's manifest with what the server has.
    pub fn plan(&self, machine_id: &str, manifest: &[Value], is_busy: &dyn Fn(&str) -> bool) -> Value {
        let mine = self.own_map(false);
        let (mut push, mut pull) = (Vec::new(), Vec::new());
        let mut seen = std::collections::HashSet::new();
        let ew: HashMap<String, Value> = self.elsewhere(true).into_iter().filter(|e| e.s("machineId") == machine_id).map(|e| (e.s("id"), e)).collect();
        for e in manifest {
            let id = e.s("id"); seen.insert(id.clone());
            if e.g("rel").is_null() {
                let x = ew.get(&id);
                if x.map(|x| e.n("mtime") > x.n("mtime") + SLACK).unwrap_or(true) { push.push(json!({"id": id, "have": x.map(|x| Self::describe(Path::new(&x.s("file")), None)).unwrap_or(Value::Null)})); }
                continue;
            }
            if is_busy(&id) { continue; }
            match mine.get(&id) {
                None => push.push(json!({"id": id, "have": Value::Null})),
                Some(h) if e.n("mtime") > h.n("mtime") + SLACK => push.push(json!({"id": id, "have": Self::describe(Path::new(&h.s("file")), Some(&self.root))})),
                Some(h) if h.n("mtime") > e.n("mtime") + SLACK => pull.push(json!({"id": id, "rel": h.g("rel"), "mtime": h.g("mtime")})),
                _ => {}
            }
        }
        for (id, h) in &mine { if !seen.contains(id) && !is_busy(id) && now_ms() - h.n("mtime") < MAX_AGE_MS && h.n("size") > 0.0 { pull.push(json!({"id": id, "rel": h.g("rel"), "mtime": h.g("mtime")})); } }
        json!({"push": push, "pull": pull})
    }
    /// A chat arrives from a computer.
    pub fn receive(&self, q: &Value, body: &[u8], is_busy: &dyn Fn(&str) -> bool) -> Value {
        let id = q.s("id"); if !is_uuid(&id) { return json!({"error": "bad id"}); }
        let in_root = !q.g("rel").is_null();
        let dest = if in_root { crate::core::projects_dir().join(encode_cwd(&Path::new(&self.root).join(q.s("rel")).to_string_lossy())).join(format!("{id}.jsonl")) }
            else { self.store.join(q.s("machineId").chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>()).join(format!("{id}.jsonl")) };
        let r = if in_root { Some(self.root.as_str()) } else { None };
        if in_root && is_busy(&id) { return json!({"ok": true, "skipped": "busy"}); }
        let mtime = q.n("mtime");
        if let Ok(md) = std::fs::metadata(&dest) { if mtime_ms(&md) > mtime + SLACK { return json!({"ok": true, "skipped": "newer here"}); } }
        let mut norm = body.to_vec();
        let off = q.n("offset") as usize;
        if off > 0 {
            let Some(have) = read_norm(&dest, r) else { return json!({"resync": true}) };
            if have.len() != off || sha1_hex(&have) != q.s("sha") { return json!({"resync": true}); }
            norm = [have, norm].concat();
        }
        if !in_root { if let Some(d) = dest.parent() { let _ = std::fs::create_dir_all(d); let nm = q.s("name"); let _ = std::fs::write(d.join("_name"), if nm.is_empty() { q.s("machineId") } else { nm }); } }
        let text = String::from_utf8_lossy(&norm).to_string();
        write_chat(&dest, &if in_root { denormalize(&text, Some(&self.root)) } else { text }, mtime);
        self.ew.lock().unwrap().0 = 0.0;
        json!({"ok": true})
    }
    /// A computer asks for a chat the server has newer: (body, delta, mtime, rel).
    pub fn serve(&self, id: &str, offset: usize, sha: &str) -> Option<(Vec<u8>, bool, f64, String)> {
        let f = find_session(id)?;
        let e = describe_chat(&f, Some(&self.root), false)?;
        if e.g("rel").is_null() { return None; }
        let norm = read_norm(&f.file, Some(&self.root))?;
        let delta = offset > 0 && offset <= norm.len() && sha1_hex(&norm[..offset]) == sha;
        Some((if delta { norm[offset..].to_vec() } else { norm }, delta, f.mtime, e.s("rel")))
    }
    pub fn note(&self, name: &str, patch: Value) {
        let mut g = self.agents.lock().unwrap();
        let cur = g.get(name).cloned().unwrap_or_else(|| json!({"at": 0}));
        g.insert(name.to_string(), merge(&merge(&cur, &patch), &json!({"seen": now_i()})));
    }
    pub fn remote_running(&self) -> std::collections::HashSet<String> {
        let mut s = std::collections::HashSet::new();
        for a in self.agents.lock().unwrap().values() { if a.n("runningAt") > 0.0 && now_ms() - a.n("runningAt") < 90000.0 { for i in arr(a.g("running")) { s.insert(vstr(&i)); } } }
        s
    }
    pub fn status(&self) -> Value {
        let mut per: Vec<(String, i64)> = Vec::new();
        for e in self.elsewhere(true) { let m = e.s("machine"); match per.iter_mut().find(|x| x.0 == m) { Some(x) => x.1 += 1, None => per.push((m, 1)) } }
        json!({"role": "server", "chats": self.own_map(false).len(), "elsewhere": per.into_iter().map(|(m, c)| json!({"machine": m, "count": c})).collect::<Vec<_>>(),
            "computers": self.agents.lock().unwrap().iter().map(|(n, a)| json!({"name": n, "online": now_ms() - a.n("seen") < crate::agent::AGENT_FRESH_MS, "lastSeen": num(a.n("seen")), "sync": a.get("last").cloned().unwrap_or(Value::Null)})).collect::<Vec<_>>()})
    }
}

// ───────── computer side ─────────
pub struct ChatAgent {
    root: Option<String>,
    machine_id: String,
    name: String,
    tr: Transport,
    state: Mutex<AgentState>,
}
#[derive(Default)]
struct AgentState { running: bool, dirty: bool, stopped: bool, last: Value, last_plan: Value, timer: Option<tokio::task::JoinHandle<()>> }
impl ChatAgent {
    pub fn new(root: Option<String>, machine_id: String, name: String, tr: Transport) -> Arc<Self> { Arc::new(ChatAgent { root, machine_id, name, tr, state: Mutex::new(AgentState { last: Value::Null, last_plan: Value::Null, ..Default::default() }) }) }
    pub fn start(self: &Arc<Self>, r: Arc<crate::relay::Relay>) {
        let me = self.clone();
        let h = tokio::spawn(async move {
            me.sync_once(&r, "start").await;
            loop { tokio::time::sleep(std::time::Duration::from_secs(10)).await; if me.state.lock().unwrap().stopped { return; } me.sync_once(&r, "timer").await; }
        });
        self.state.lock().unwrap().timer = Some(h);
    }
    pub fn stop(&self) { let mut s = self.state.lock().unwrap(); s.stopped = true; if let Some(t) = s.timer.take() { t.abort(); } }
    pub fn kick(self: &Arc<Self>, r: Arc<crate::relay::Relay>, why: &str) { let me = self.clone(); let why = why.to_string(); tokio::spawn(async move { me.sync_once(&r, &why).await; }); }
    pub fn status(&self) -> Value {
        let s = self.state.lock().unwrap();
        json!({"role": "computer", "last": s.last, "localChats": if s.last.is_null() { Value::Null } else { s.last.g("localChats").clone() }, "waiting": arr(s.last_plan.g("push")).len() + arr(s.last_plan.g("pull")).len()})
    }
    fn entries(&self) -> Vec<Value> { list_files().iter().filter_map(|f| describe_chat(f, self.root.as_deref(), true)).collect() }
    async fn push_one(&self, e: &Value, have: &Value) -> Result<bool, String> {
        if now_ms() - e.n("mtime") < 3000.0 { return Ok(false); }   // still being written: next round
        let full = read_norm(Path::new(&e.s("file")), self.root.as_deref()).ok_or("cannot read the chat")?;
        for retried in [false, true] {
            let hb = have.n("bytes") as usize;
            let delta = !retried && have.is_object() && hb > 0 && hb <= full.len() && sha1_hex(&full[..hb]) == have.s("sha");
            let body = gzip(if delta { &full[hb..] } else { &full }, 6);
            let mut q = vec![("id", e.s("id")), ("machineId", self.machine_id.clone()), ("name", self.name.clone()), ("mtime", e.n("mtime").to_string()), ("offset", if delta { hb.to_string() } else { "0".into() })];
            if delta { q.push(("sha", have.s("sha"))); }
            if !e.g("rel").is_null() { q.push(("rel", e.s("rel"))); }
            let r = self.tr.put(&format!("/agent/chats/put?{}", qbuild(&q)), body, 120000).await?;
            if r.t("resync") && !retried { continue; }
            if !r.s("error").is_empty() { return Err(r.s("error")); }
            return Ok(true);
        }
        Ok(true)
    }
    async fn pull_one(&self, r: &Arc<crate::relay::Relay>, p: &Value, by_id: &HashMap<String, Value>) -> Result<bool, String> {
        let root = self.root.clone().unwrap_or_default();
        let dir = Path::new(&root).join(p.s("rel")).to_string_lossy().to_string();
        let dst = crate::core::projects_dir().join(encode_cwd(&dir)).join(format!("{}.jsonl", p.s("id")));
        let st = std::fs::metadata(&dst).ok();
        if let Some(m) = &st { if mtime_ms(m) >= p.n("mtime") - SLACK { return Ok(true); } }
        if st.is_none() && !p.s("rel").is_empty() && !Path::new(&dir).exists() { return Ok(true); }
        let known = by_id.get(&p.s("id"));
        if let Some(m) = &st { let mt = mtime_ms(m); if r.is_busy(&p.s("id")) || now_ms() - mt < QUIET_MS || known.map(|k| mt > k.n("mtime") + SLACK).unwrap_or(false) { return Ok(false); } }
        let (mut off, mut sha) = (0usize, String::new());
        if st.is_some() { if let Some(b) = read_norm(&dst, Some(&root)) { off = b.len(); sha = sha1_hex(&b); } }
        let Some((body, h)) = self.tr.get(&format!("/agent/chats/get?{}", qbuild(&[("id", p.s("id")), ("offset", off.to_string()), ("sha", sha)])), 120000).await? else { return Ok(false) };
        let delta = h.get("x-delta").map(|v| v == "1").unwrap_or(false);
        let mtime = h.get("x-mtime").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<f64>().ok()).unwrap_or_else(now_ms);
        let base = if delta { read_norm(&dst, Some(&root)).unwrap_or_default() } else { vec![] };
        let text = String::from_utf8_lossy(&[base, body].concat()).to_string();
        if let Some(m) = &st { let again = std::fs::metadata(&dst).map(|x| mtime_ms(&x)).unwrap_or(0.0); if again > mtime_ms(m) + SLACK { return Ok(false); } }
        write_chat(&dst, &denormalize(&text, Some(&root)), mtime);
        Ok(true)
    }
    pub fn sync_once<'a>(self: &'a Arc<Self>, r: &'a Arc<crate::relay::Relay>, reason: &'a str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Value> + Send + 'a>> { Box::pin(async move {
        { let mut s = self.state.lock().unwrap(); if s.running { s.dirty = true; return s.last.clone(); } s.running = true; }
        let t0 = now_ms();
        let (mut pushed, mut pulled, mut waiting, mut errors) = (0, 0, 0, Vec::<String>::new());
        let list = self.entries();
        let by_id: HashMap<String, Value> = list.iter().map(|e| (e.s("id"), e.clone())).collect();
        let running: Vec<String> = r.jobs.lock().unwrap().values().filter(|j| !j.is_done()).map(|j| j.sid()).filter(|s| !s.is_empty()).collect();
        let manifest: Vec<Value> = list.iter().map(|e| { let mut m = e.clone(); if let Some(o) = m.as_object_mut() { o.remove("file"); } m }).collect();
        let mut local_chats = Value::Null;
        match self.tr.json("/agent/chats/sync", &json!({"machineId": self.machine_id, "name": self.name, "running": running, "manifest": manifest}), 40000).await {
            Ok(plan) => {
                self.state.lock().unwrap().last_plan = plan.clone();
                for x in arr(plan.g("push")) { let Some(e) = by_id.get(&x.s("id")) else { continue }; match self.push_one(e, x.g("have")).await { Ok(true) => pushed += 1, Ok(false) => waiting += 1, Err(err) => errors.push(format!("send {}: {}", clip(&e.s("title"), 30), err)) } }
                for p in arr(plan.g("pull")) { match self.pull_one(r, &p, &by_id).await { Ok(true) => pulled += 1, Ok(false) => waiting += 1, Err(err) => errors.push(format!("receive {}: {}", clip(&p.s("id"), 8), err)) } }
                local_chats = json!(list.len());
            }
            Err(e) => errors.push(e),
        }
        let stat = json!({"at": t0 as i64, "reason": reason, "pushed": pushed, "pulled": pulled, "errors": errors, "waiting": waiting, "localChats": local_chats, "ok": errors.is_empty(), "ms": (now_ms() - t0) as i64});
        let _ = self.tr.json("/agent/chats/report", &json!({"name": self.name, "machineId": self.machine_id, "stat": stat}), 10000).await;
        let again = { let mut s = self.state.lock().unwrap(); s.last = stat.clone(); s.running = false; std::mem::take(&mut s.dirty) };
        if again { let me = self.clone(); let r2 = r.clone(); tokio::spawn(async move { tokio::time::sleep(std::time::Duration::from_millis(500)).await; me.sync_once(&r2, "again").await; }); }
        stat
    }) }
}
