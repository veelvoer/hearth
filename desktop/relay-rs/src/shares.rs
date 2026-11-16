//! Shared chats: a normal chat of the host that other people may open with a guest key. What they send is a request on the host's
//! account: the host approves (maybe changes it), denies, or gives that person full permission. Port of shares.js.
use crate::relay::Relay;
use crate::util::*;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const MAX_TEXT: usize = 20000;
const MAX_PENDING: usize = 5;
const INVITE_MS: f64 = 24.0 * 3600e3;
const MODES: [&str; 4] = ["plan", "acceptEdits", "manual", "bypassPermissions"];
const WORK_TTL: f64 = 8.0 * 60e3;

fn clean(s: &str, n: usize) -> String { clip(&s.chars().filter(|c| { let u = *c as u32; !(u <= 8 || u == 11 || u == 12 || (14..=31).contains(&u)) }).collect::<String>(), n) }
fn sha(s: &str) -> String { sha256_hex(s.as_bytes()) }
fn fail(code: u16, e: &str) -> (u16, Value) { (code, json!({"error": e})) }
fn ok(d: Value) -> (u16, Value) { (200, d) }
fn to_of(q: &Value) -> String { let t = q.s("to"); if t.is_empty() { "host".into() } else { t } }
fn grant_of(m: &Value) -> Value { let g = m.g("grant"); json!({"auto": g.t("auto"), "mode": if MODES.contains(&g.s("mode").as_str()) { g.s("mode") } else { "plan".into() }}) }

pub struct Shares { file: PathBuf, pub st: Mutex<Value>, queue: Mutex<Vec<(String, String)>>, host: String }

impl Shares {
    pub fn new(dir: &std::path::Path, host: String) -> Self {
        let file = dir.join("shares.json");
        let st = read_json(&file).filter(|j| j.g("shares").is_object()).unwrap_or_else(|| json!({"shares": {}}));
        let s = Shares { file, st: Mutex::new(st), queue: Mutex::new(Vec::new()), host };
        // on start: approved prompts go back in the line; ones that were running when the server stopped are marked
        { let mut st = s.st.lock().unwrap(); let mut q = s.queue.lock().unwrap();
          if let Some(shares) = st.get_mut("shares").and_then(|x| x.as_object_mut()) { for (sid, sh) in shares.iter_mut() {
            if let Some(reqs) = sh.get_mut("requests").and_then(|x| x.as_object_mut()) { for (rid, r) in reqs.iter_mut() {
                if r.s("status") == "approved" && to_of(r) == "host" { q.push((sid.clone(), rid.clone())); }
                else if r.s("status") == "running" { set(r, "status", json!("failed")); set(r, "note", json!("The server restarted while this ran.")); }
            } }
          } } }
        s
    }
    pub fn save(&self) {
        let st = self.st.lock().unwrap().clone();
        if let Some(d) = self.file.parent() { let _ = std::fs::create_dir_all(d); }
        let _ = write_atomic_private(&self.file, st.to_string().as_bytes());
    }
    pub fn get(&self, id: &str) -> Option<Value> { self.st.lock().unwrap().g("shares").get(id).cloned() }
    pub fn by_session(&self, sid: &str) -> Option<Value> { self.st.lock().unwrap().g("shares").as_object().and_then(|m| m.values().find(|s| s.s("sid") == sid).cloned()) }
    fn with<R>(&self, id: &str, f: impl FnOnce(&mut Value) -> R) -> Option<R> { let mut st = self.st.lock().unwrap(); st.get_mut("shares").and_then(|m| m.get_mut(id)).map(f) }
    pub fn live(s: &Value) -> Vec<Value> { s.g("members").as_object().map(|m| m.values().filter(|x| !x.t("revoked")).cloned().collect()).unwrap_or_default() }
    fn reqs(s: &Value) -> Vec<Value> { s.g("requests").as_object().map(|m| m.values().cloned().collect()).unwrap_or_default() }
    fn event(s: &Value, q: &Value) -> Value {
        let mut e = json!({"kind": "request", "session": s.s("sid"), "title": s.s("title"), "cwd": s.s("cwd"), "req": q.s("id"), "from": q.s("name"), "text": clip(&format!("{}: {}", q.s("name"), q.s("text")), 600), "call": false, "tool": "", "input": {"prompt": q.s("text")}});
        if !q.s("to").is_empty() && q.s("to") != "host" { set(&mut e, "forMember", json!(q.s("to"))); }
        e
    }
    /// Waiting requests as notification events.
    pub fn pending_events(&self) -> Vec<Value> {
        let st = self.st.lock().unwrap(); let mut out = vec![];
        if let Some(m) = st.g("shares").as_object() { for s in m.values() { for q in Self::reqs(s) {
            if q.s("status") == "pending" && to_of(&q) == "host" && !s.g("members").g(&q.s("mid")).t("revoked") { out.push(merge(&Self::event(s, &q), &json!({"ts": q.g("ts"), "id": 0}))); }
        } } }
        out
    }
    pub fn incoming_events(&self, sh: &str, mid: &str) -> Vec<Value> {
        let Some(s) = self.get(sh) else { return vec![] };
        Self::reqs(&s).into_iter().filter(|q| q.s("status") == "pending" && to_of(q) == mid).map(|q| merge(&Self::event(&s, &q), &json!({"ts": q.g("ts"), "id": 0}))).collect()
    }
    fn find_request(&self, rid: &str) -> Option<String> { self.st.lock().unwrap().g("shares").as_object().and_then(|m| m.iter().find(|(_, s)| s.g("requests").get(rid).is_some()).map(|(k, _)| k.clone())) }
    pub fn has_request(&self, rid: &str) -> bool { self.find_request(rid).is_some() }
    fn downgrade(r: &Relay, mode: &str) -> String { let m = if MODES.contains(&mode) { mode } else { "plan" }; if m == "bypassPermissions" && !r.bypass_ok() { "acceptEdits".into() } else { m.into() } }

    pub fn ensure(&self, sid: &str, cwd: &str, title: &str) -> Value {
        if let Some(s) = self.by_session(sid) { return s; }
        let t = clean(title, 120);
        let s = json!({"id": rand_b64url(6), "sid": sid, "cwd": cwd, "title": if t.is_empty() { "Shared chat".into() } else { t }, "created": now_i(), "members": {}, "invites": {}, "requests": {}});
        { let mut st = self.st.lock().unwrap(); if let Some(m) = st.get_mut("shares").and_then(|x| x.as_object_mut()) { m.insert(s.s("id"), s.clone()); } }
        self.save(); s
    }
    fn req_view(&self, q: &Value, s: &Value) -> Value {
        let to = to_of(q);
        json!({"id": q.s("id"), "text": clip(&q.s("text"), 600), "finalText": q.get("finalText").cloned().unwrap_or(Value::Null), "status": q.s("status"), "ts": q.g("ts"), "edited": q.t("edited"), "note": q.s("note"), "from": q.s("mid"), "fromName": q.s("name"), "to": to,
            "toName": if to == "host" { self.host.clone() } else { let n = s.g("members").g(&to).s("name"); if n.is_empty() { "?".into() } else { n } }, "mode": q.get("mode").cloned().unwrap_or(Value::Null)})
    }
    fn people(&self, s: &Value, except: &str) -> Vec<Value> {
        let mut p = vec![json!({"id": "host", "name": self.host})];
        p.extend(Self::live(s).iter().map(|m| json!({"id": m.s("id"), "name": m.s("name")})));
        p.into_iter().filter(|x| x.s("id") != except).collect()
    }
    fn sorted_reqs(s: &Value, f: impl Fn(&Value) -> bool, newest_first: bool) -> Vec<Value> {
        let mut l: Vec<Value> = Self::reqs(s).into_iter().filter(|q| f(q)).collect();
        l.sort_by(|a, b| { let o = a.n("ts").partial_cmp(&b.n("ts")).unwrap_or(std::cmp::Ordering::Equal); if newest_first { o.reverse() } else { o } }); l
    }
    pub fn owner_info(&self, s: &Value) -> Value {
        json!({"people": self.people(s, "host"), "requests": Self::sorted_reqs(s, |q| q.s("mid") == "host", true).iter().take(8).map(|q| self.req_view(q, s)).collect::<Vec<_>>()})
    }
    pub fn view(&self, s: &Value) -> Value {
        let pending = Self::reqs(s).iter().filter(|q| q.s("status") == "pending" && !s.g("members").g(&q.s("mid")).t("revoked")).count();
        merge(&self.owner_info(s), &json!({"id": s.s("id"), "session": s.s("sid"), "title": s.s("title"), "members": Self::live(s).iter().map(|m| json!({"id": m.s("id"), "name": m.s("name"), "joined": m.g("joined"), "grant": grant_of(m)})).collect::<Vec<_>>(), "pending": pending}))
    }
    pub fn guest_info(&self, s: &Value, mem: &Value) -> Value {
        let mid = mem.s("id");
        json!({"host": self.host, "role": "guest", "title": s.s("title"), "me": mid, "myName": mem.s("name"), "grant": grant_of(mem), "people": self.people(s, &mid),
            "requests": Self::sorted_reqs(s, |q| q.s("mid") == mid, true).iter().take(8).map(|q| self.req_view(q, s)).collect::<Vec<_>>(),
            "incoming": Self::sorted_reqs(s, |q| to_of(q) == mid && (q.s("status") == "pending" || q.s("status") == "approved"), false).iter().map(|q| self.req_view(q, s)).collect::<Vec<_>>()})
    }
    pub fn invite(&self, sh: &str, name: &str) -> Value {
        let code = rand_b64url(16);
        self.with(sh, |s| { let mut inv = s.g("invites").clone(); set(&mut inv, &sha(&code), json!({"exp": num(now_ms() + INVITE_MS), "name": clean(name, 40)})); set(s, "invites", inv); });
        self.save(); json!({"invite": code, "expires": num(now_ms() + INVITE_MS)})
    }
    pub fn set_grant(&self, r: &Relay, sh: &str, mid: &str, g: &Value) -> (u16, Value) {
        let out = self.with(sh, |s| {
            let m = s.g("members").g(mid).clone(); if m.is_null() || m.t("revoked") { return fail(404, "No such person."); }
            let mut mem = s.g("members").clone(); let mut mm = m.clone(); set(&mut mm, "grant", json!({"auto": g.t("auto"), "mode": Self::downgrade(r, &g.s("mode"))})); let gv = grant_of(&mm); set(&mut mem, mid, mm); set(s, "members", mem);
            ok(json!({"grant": gv}))
        }).unwrap_or_else(|| fail(404, "No such shared chat."));
        self.save(); out
    }
    pub fn remove_member(&self, r: &Arc<Relay>, sh: &str, mid: &str) -> (u16, Value) {
        let mut evs = vec![];
        let out = self.with(sh, |s| {
            let m = s.g("members").g(mid).clone(); if m.is_null() || m.t("revoked") { return fail(404, "No such person."); }
            let mut mem = s.g("members").clone(); let mut mm = m; set(&mut mm, "revoked", json!(true)); set(&mut mem, mid, mm); set(s, "members", mem);
            let sid = s.s("sid");
            if let Some(reqs) = s.get_mut("requests").and_then(|x| x.as_object_mut()) { for q in reqs.values_mut() { if q.s("mid") == mid && (q.s("status") == "pending" || q.s("status") == "approved") { set(q, "status", json!("denied")); set(q, "note", json!("Removed")); evs.push(json!({"kind": "resolved", "req": q.s("id"), "session": sid})); } } }
            ok(json!({"ok": true}))
        }).unwrap_or_else(|| fail(404, "No such shared chat."));
        self.save(); for e in evs { on_event(r, e); } out
    }
    pub fn unshare(&self, r: &Arc<Relay>, sh: &str) -> (u16, Value) {
        let s = self.get(sh);
        if let Some(s) = &s { for q in Self::reqs(s) { if q.s("status") == "pending" { on_event(r, json!({"kind": "resolved", "req": q.s("id"), "session": s.s("sid")})); } } }
        { let mut st = self.st.lock().unwrap(); if let Some(m) = st.get_mut("shares").and_then(|x| x.as_object_mut()) { m.remove(sh); } }
        self.save(); ok(json!({"ok": true}))
    }
    /// The host decides on a request. body: { decision: 'approve'|'deny', text, mode, always }
    pub fn decide(&self, r: &Arc<Relay>, rid: &str, body: &Value) -> (u16, Value) {
        let Some(sh) = self.find_request(rid) else { return fail(404, "No such request.") };
        let mut ev = None; let mut pump = false;
        let out = self.with(&sh, |s| {
            let q = s.g("requests").g(rid).clone(); let to = to_of(&q);
            if to != "host" { let n = s.g("members").g(&to).s("name"); return fail(403, &format!("Only {} can decide this.", if n.is_empty() { "that person".into() } else { n })); }
            if q.s("status") != "pending" { return fail(409, "Already decided."); }
            let m = s.g("members").g(&q.s("mid")).clone(); let sid = s.s("sid");
            let mut q2 = q.clone();
            let res = if m.is_null() || m.t("revoked") { set(&mut q2, "status", json!("denied")); set(&mut q2, "note", json!("Removed")); fail(409, "That person was removed.") }
            else if body.s("decision") == "deny" { set(&mut q2, "status", json!("denied")); set(&mut q2, "note", json!(clean(&body.s("note"), 200))); ev = Some(json!({"kind": "resolved", "req": rid, "session": sid})); ok(json!({"ok": true})) }
            else if body.s("decision") != "approve" { return fail(400, "decision must be approve or deny"); }
            else {
                let text = clean(if body.g("text").is_null() { q.s("text") } else { vstr(body.g("text")) }.as_str(), MAX_TEXT).trim().to_string();
                if text.is_empty() { return fail(400, "Empty prompt."); }
                let mode = Self::downgrade(r, &{ let m = body.s("mode"); if m.is_empty() { "plan".into() } else { m } });
                set(&mut q2, "status", json!("approved")); set(&mut q2, "finalText", json!(text)); set(&mut q2, "edited", json!(text != q.s("text"))); set(&mut q2, "mode", json!(mode));
                if body.t("always") { let mut mem = s.g("members").clone(); let mut mm = m.clone(); set(&mut mm, "grant", json!({"auto": true, "mode": mode})); set(&mut mem, &q.s("mid"), mm); set(s, "members", mem); }
                ev = Some(json!({"kind": "resolved", "req": rid, "session": sid})); pump = true;
                ok(json!({"ok": true, "status": "approved"}))
            };
            let mut reqs = s.g("requests").clone(); set(&mut reqs, rid, q2); set(s, "requests", reqs);
            res
        }).unwrap_or_else(|| fail(404, "No such request."));
        if pump { self.queue.lock().unwrap().push((sh.clone(), rid.to_string())); }
        self.save();
        if let Some(e) = ev { on_event(r, e); }
        if pump { self.pump(r); }
        out
    }
    pub fn join(&self, body: &Value) -> (u16, Value) {
        let h = sha(&body.s("invite"));
        let join_fail = || fail(404, "This invite does not work (used up, expired or wrong).");
        let Some(sh) = self.st.lock().unwrap().g("shares").as_object().and_then(|m| m.iter().find(|(_, s)| s.g("invites").get(&h).is_some()).map(|(k, _)| k.clone())) else { return join_fail() };
        let out = self.with(&sh, |s| {
            let inv = s.g("invites").g(&h).clone(); if inv.n("exp") < now_ms() { return None; }
            let mut invs = s.g("invites").clone(); if let Some(m) = invs.as_object_mut() { m.remove(&h); } set(s, "invites", invs);   // one person per invite
            let nm = clean(&{ let n = body.s("name"); if n.is_empty() { inv.s("name") } else { n } }, 40).trim().to_string();
            let name = if nm.is_empty() { "Guest".to_string() } else { nm };
            let (secret, mid) = (rand_b64url(24), rand_b64url(5));
            let key = format!("share.{}.{}", s.s("id"), secret);
            let mut mem = s.g("members").clone(); set(&mut mem, &mid, json!({"id": mid, "name": name, "tokenHash": sha(&key), "joined": now_i(), "grant": {"auto": false, "mode": "plan"}})); set(s, "members", mem);
            Some(json!({"share": s.s("id"), "session": s.s("sid"), "title": s.s("title"), "member": mid, "key": key}))
        }).flatten();
        match out { Some(mut d) => { set(&mut d, "host", json!(self.host)); self.save(); ok(d) } None => join_fail() }
    }
    /// A guest key ("share.<id>.<secret>") -> (share, member).
    pub fn guest_for(&self, token: &str) -> Option<(Value, Value)> {
        let re = regex::Regex::new(r"^share\.([A-Za-z0-9_-]{4,16})\.([A-Za-z0-9_-]{10,64})$").unwrap();
        let c = re.captures(token)?;
        let s = self.get(&c[1])?;
        let h = sha(token);
        let mem = s.g("members").as_object()?.values().find(|m| !m.t("revoked") && m.s("tokenHash") == h)?.clone();
        Some((s, mem))
    }
    fn add_request(&self, sh: &str, q: Value) { self.with(sh, |s| {
        let mut reqs = s.g("requests").clone(); set(&mut reqs, &q.s("id"), q);
        let mut all: Vec<Value> = reqs.as_object().map(|m| m.values().cloned().collect()).unwrap_or_default(); all.sort_by(|a, b| b.n("ts").partial_cmp(&a.n("ts")).unwrap_or(std::cmp::Ordering::Equal));
        if let Some(m) = reqs.as_object_mut() { for old in all.iter().skip(200) { m.remove(&old.s("id")); } }
        set(s, "requests", reqs);
    }); }
    /// A guest sends a prompt: it becomes a request (or runs at once when the host gave that person full permission).
    pub fn submit(&self, r: &Arc<Relay>, sh: &str, mem: &Value, text0: &str) -> (u16, Value) {
        let text = clean(text0, MAX_TEXT).trim().to_string(); if text.is_empty() { return fail(400, "Empty message."); }
        let Some(s) = self.get(sh) else { return fail(404, "No such shared chat.") };
        let mine: Vec<Value> = Self::reqs(&s).into_iter().filter(|q| q.s("mid") == mem.s("id")).collect();
        if mine.iter().filter(|q| q.s("status") == "pending").count() >= MAX_PENDING { return fail(429, "You have too many prompts waiting. Wait for the host."); }
        if mine.iter().any(|q| now_ms() - q.n("ts") < 1500.0) { return fail(429, "Slow down a little."); }
        let mut q = json!({"id": rand_b64url(5), "mid": mem.s("id"), "name": mem.s("name"), "text": text, "ts": now_i(), "status": "pending"});
        let g = grant_of(mem);
        if g.t("auto") {
            set(&mut q, "status", json!("approved")); set(&mut q, "finalText", json!(text)); set(&mut q, "mode", json!(Self::downgrade(r, &g.s("mode")))); set(&mut q, "note", json!("Runs at once: full permission"));
            let id = q.s("id"); self.add_request(sh, q); self.queue.lock().unwrap().push((sh.to_string(), id.clone())); self.save(); self.pump(r);
            return ok(json!({"id": id, "status": "approved", "auto": true}));
        }
        let id = q.s("id"); self.add_request(sh, q.clone()); self.save();
        let s = self.get(sh).unwrap_or(s); on_event(r, Self::event(&s, &q));
        ok(json!({"id": id, "status": "pending"}))
    }
    /// Asks a person for THEIR Claude (the host asks a friend, or a friend asks another friend).
    pub fn ask(&self, r: &Arc<Relay>, sh: &str, from: &Value, to_id: &str, text0: &str) -> (u16, Value) {
        let Some(s) = self.get(sh) else { return fail(404, "No such shared chat.") };
        let to = s.g("members").g(to_id); if to.is_null() || to.t("revoked") { return fail(404, "No such person in this chat."); }
        if to_id == from.s("id") { return fail(400, "That is your own Claude."); }
        let text = clean(text0, MAX_TEXT).trim().to_string(); if text.is_empty() { return fail(400, "Empty message."); }
        let mine: Vec<Value> = Self::reqs(&s).into_iter().filter(|q| q.s("mid") == from.s("id")).collect();
        if mine.iter().filter(|q| q.s("status") == "pending").count() >= MAX_PENDING { return fail(429, "You have too many prompts waiting."); }
        if mine.iter().any(|q| now_ms() - q.n("ts") < 1500.0) { return fail(429, "Slow down a little."); }
        let q = json!({"id": rand_b64url(5), "mid": from.s("id"), "name": from.s("name"), "to": to_id, "text": text, "ts": now_i(), "status": "pending"});
        self.add_request(sh, q.clone()); self.save(); on_event(r, Self::event(&s, &q));
        ok(json!({"id": q.s("id"), "status": "pending"}))
    }
    pub fn member_decides(&self, r: &Arc<Relay>, sh: &str, mem: &Value, rid: &str, body: &Value) -> (u16, Value) {
        let mid = mem.s("id"); let mut ev = None;
        let out = self.with(sh, |s| {
            let q = s.g("requests").g(rid).clone(); if q.is_null() || to_of(&q) != mid { return fail(404, "No such request."); }
            let mut q2 = q.clone(); let d = body.s("decision"); let sid = s.s("sid");
            let res = if d == "done" || d == "failed" { if q.s("status") != "approved" { return fail(409, "Not running."); } set(&mut q2, "status", json!(d)); set(&mut q2, "note", json!(clean(&body.s("note"), 200))); ok(json!({"ok": true})) }
            else if q.s("status") != "pending" { return fail(409, "Already decided."); }
            else if d == "deny" { set(&mut q2, "status", json!("denied")); set(&mut q2, "note", json!(clean(&body.s("note"), 200))); ev = Some(json!({"kind": "resolved", "req": q.s("id"), "session": sid, "forMember": mid})); ok(json!({"ok": true})) }
            else if d != "approve" { return fail(400, "decision must be approve, deny, done or failed"); }
            else {
                let text = clean(&if body.g("text").is_null() { q.s("text") } else { vstr(body.g("text")) }, MAX_TEXT).trim().to_string(); if text.is_empty() { return fail(400, "Empty prompt."); }
                let mode = if MODES.contains(&body.s("mode").as_str()) { body.s("mode") } else { "plan".into() };
                set(&mut q2, "status", json!("approved")); set(&mut q2, "finalText", json!(text)); set(&mut q2, "edited", json!(text != q.s("text"))); set(&mut q2, "mode", json!(mode));
                ev = Some(json!({"kind": "resolved", "req": q.s("id"), "session": sid, "forMember": mid}));
                ok(json!({"ok": true, "text": text, "mode": mode, "from": q.s("name")}))
            };
            let mut reqs = s.g("requests").clone(); set(&mut reqs, rid, q2); set(s, "requests", reqs); res
        }).unwrap_or_else(|| fail(404, "No such request."));
        self.save(); if let Some(e) = ev { on_event(r, e); } out
    }
    pub fn post(&self, sh: &str, who: &Value, body: &Value) -> (u16, Value) {
        let role = if body.s("role") == "assistant" { "assistant" } else { "user" };
        let text = clean(&body.s("text"), MAX_TEXT).trim().to_string(); if text.is_empty() { return fail(400, "Empty message."); }
        let out = self.with(sh, |s| {
            let mut name = who.s("name"); let mut via = String::new();
            if !body.s("req").is_empty() {
                let q = s.g("requests").g(&body.s("req")).clone();
                if q.is_null() || to_of(&q) != who.s("id") || q.s("status") != "approved" { return fail(403, "That request is not yours to run."); }
                if role == "user" { name = q.s("name"); } else { name = format!("{}'s Claude", who.s("name")); via = format!(" for {}", q.s("name")); }
            } else if role == "assistant" { name = format!("{}'s Claude", who.s("name")); }
            let n = s.n("logSeq") as i64 + 1; set(s, "logSeq", json!(n));
            let mut log = arr(s.g("log")); log.push(json!({"n": n, "ts": now_i(), "role": role, "name": format!("{name}{via}"), "text": text, "by": who.s("id")}));
            let k = log.len().saturating_sub(400); let log: Vec<Value> = log.into_iter().skip(k).collect(); set(s, "log", Value::Array(log));
            ok(json!({"n": n}))
        }).unwrap_or_else(|| fail(404, "No such shared chat."));
        self.save(); out
    }
    pub fn working(&self, sh: &str, who: &Value, on: bool) -> (u16, Value) {
        self.with(sh, |s| { let mut w = if s.g("working").is_object() { s.g("working").clone() } else { obj() }; if on { set(&mut w, &who.s("id"), json!({"name": format!("{}'s Claude", who.s("name")), "since": now_i()})); } else if let Some(m) = w.as_object_mut() { m.remove(&who.s("id")); } set(s, "working", w); });
        ok(json!({"ok": true}))
    }
    fn workers(&self, sh: &str) -> Vec<String> {
        self.with(sh, |s| { let mut w = s.g("working").as_object().cloned().unwrap_or_default(); w.retain(|_, v| now_ms() - v.n("since") <= WORK_TTL); let names = w.values().map(|v| v.s("name")).collect(); set(s, "working", Value::Object(w)); names }).unwrap_or_default()
    }
    /// The chat as everyone sees it: the real messages, with the others' own-Claude messages in their place by time.
    pub fn merge(&self, sh: &Value, list: Vec<Value>) -> Value {
        let ctx = regex::Regex::new(r"^\[In this shared chat, others used their own Claude since your last turn:[\s\S]*?\]\n\n").unwrap();
        let mut out = list.clone();
        let first_ts = list.first().map(|m| m.n("ts")).unwrap_or(0.0);
        let extra: Vec<Value> = arr(sh.g("log")).into_iter().filter(|e| list.is_empty() || e.n("ts") >= first_ts - 60000.0).collect();
        for e in extra {
            let mut at = out.len(); while at > 0 && out[at - 1].n("ts") > e.n("ts") { at -= 1; }
            out.insert(at, json!({"role": e.g("role"), "text": e.g("text"), "name": e.g("name"), "ts": e.g("ts"), "own": true}));
        }
        for w in self.workers(&sh.s("id")) { out.push(json!({"role": "meta", "text": format!("{w} is working…"), "ts": now_i()})); }
        Value::Array(out.into_iter().map(|m| { if m.s("role") == "user" && m.get("text").map(|t| t.is_string()).unwrap_or(false) && ctx.is_match(&m.s("text")) { let t = ctx.replace(&m.s("text"), "").to_string(); merge(&m, &json!({"text": t})) } else { m } }).collect())
    }
    /// What the host's own Claude has not heard yet: what others did with their own accounts. Marks it as heard.
    pub fn context_for(&self, sh: &str) -> String {
        let out = self.with(sh, |s| {
            let seen = s.n("hostSeen"); let log = arr(s.g("log"));
            let items: Vec<Value> = log.into_iter().filter(|e| e.n("n") > seen).collect();
            let items: Vec<Value> = items.iter().skip(items.len().saturating_sub(30)).cloned().collect();
            if items.is_empty() { return String::new(); }
            set(s, "hostSeen", items.last().unwrap().g("n").clone());
            format!("[In this shared chat, others used their own Claude since your last turn:\n{}]\n\n", items.iter().map(|e| format!("{}: {}", e.s("name"), e.s("text"))).collect::<Vec<_>>().join("\n"))
        }).unwrap_or_default();
        if !out.is_empty() { self.save(); }
        out
    }
    /// Starts the approved prompts whose chat is free.
    pub fn pump(&self, r: &Arc<Relay>) {
        let items = self.queue.lock().unwrap().clone();
        let mut done_idx = vec![];
        for (i, (sh, rid)) in items.iter().enumerate() {
            let Some(s) = self.get(sh) else { done_idx.push(i); continue };
            let q = s.g("requests").g(rid).clone();
            if q.is_null() || q.s("status") != "approved" { done_idx.push(i); continue; }
            if r.job_busy(&s.s("sid")) { continue; }
            let text = format!("{}{}: {}", self.context_for(sh), q.s("name"), q.s("finalText"));
            let res = r.start_job(&json!({"text": text, "mode": q.g("mode"), "noRoot": true}), &s.s("sid"));
            match res {
                Err((409, _)) => continue,
                Err((_, m)) => { done_idx.push(i); self.with(sh, |s| { let mut reqs = s.g("requests").clone(); if let Some(x) = reqs.get_mut(rid.as_str()) { set(x, "status", json!("failed")); set(x, "note", json!(clean(&m, 200))); } set(s, "requests", reqs); }); }
                Ok(st) => {
                    done_idx.push(i);
                    { let mut g = st.job.st.lock().unwrap(); g.by = q.s("name"); g.by_req = q.s("id"); g.by_member = q.s("mid"); }
                    self.with(sh, |s| { let mut reqs = s.g("requests").clone(); if let Some(x) = reqs.get_mut(rid.as_str()) { set(x, "status", json!("running")); } set(s, "requests", reqs); });
                }
            }
            self.save();
        }
        let mut qq = self.queue.lock().unwrap();
        let keep: Vec<(String, String)> = qq.iter().enumerate().filter(|(i, x)| !(done_idx.contains(i) && items.get(*i) == Some(x))).map(|(_, x)| x.clone()).collect();
        *qq = keep;
    }
}

fn on_event(r: &Arc<Relay>, ev: Value) { let b = r.base_for(&ev.s("session"), &ev.s("cwd")); r.publish(merge(&b, &ev)); }

/// The server tells the shares when a run of a chat has ended.
pub fn job_done(r: &Arc<Relay>, sid: &str) {
    if let Some(s) = r.shares.by_session(sid) {
        r.shares.with(&s.s("id"), |s| { let mut reqs = s.g("requests").clone(); if let Some(m) = reqs.as_object_mut() { for q in m.values_mut() { if q.s("status") == "running" { set(q, "status", json!("done")); } } } set(s, "requests", reqs); });
        r.shares.save();
    }
    r.shares.pump(r);
}
