//! Hearth API keys: an app of yours calls Claude through your Hearth server with a key, on your plan's usage. Port of apikeys.js.
use crate::util::*;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Mutex;

pub const MODELS: [&str; 3] = ["haiku", "sonnet", "opus"];
pub const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];
pub fn presets() -> Value { json!({"low": {"models": ["haiku", "sonnet"], "efforts": ["low", "medium"]}, "high": {"models": ["sonnet", "opus"], "efforts": ["high", "xhigh", "max"]}, "general": {"models": MODELS, "efforts": EFFORTS}}) }
fn day_of(t: f64) -> String { iso(t)[..10].to_string() }

pub struct Keys { file: PathBuf, db: Mutex<Value> }
impl Keys {
    pub fn new(dir: &std::path::Path) -> Self {
        let file = dir.join("apikeys.json");
        let db = read_json(&file).filter(|d| d.g("keys").is_array()).unwrap_or_else(|| json!({"keys": []}));
        Keys { file, db: Mutex::new(db) }
    }
    fn save_v(&self, db: &Value) { let _ = write_atomic_private(&self.file, db.to_string().as_bytes()); }
    fn view(k: &Value) -> Value {
        json!({"id": k.g("id"), "name": k.g("name"), "kind": k.g("kind"), "ends": k.g("last4"), "models": k.g("models"), "efforts": k.g("efforts"), "perDay": k.g("perDay"), "weekStopPct": k.g("weekStopPct"), "sessionStopPct": num(k.n("sessionStopPct")), "session": if k.s("session").is_empty() { "none".into() } else { k.s("session") },
            "expiresAt": k.g("expiresAt"), "created": k.g("created"), "revoked": k.t("revoked"), "usedToday": if k.s("day") == day_of(now_ms()) { num(k.n("count")) } else { json!(0) }, "total": num(k.n("total")), "tokens": num(k.n("tokens")), "lastUsed": num(k.n("lastUsed"))})
    }
    fn pick_list(given: &Value, all: &[&str], fallback: &Value) -> Value {
        let l: Vec<String> = arr(given).iter().map(vstr).filter(|x| all.contains(&x.as_str())).collect();
        if l.is_empty() { fallback.clone() } else { json!(all.iter().filter(|x| l.contains(&x.to_string())).collect::<Vec<_>>()) }
    }
    pub fn create(&self, b: &Value) -> Value {
        let p = presets(); let kind = if p.get(b.s("kind")).is_some() { b.s("kind") } else { "general".into() }; let pre = p.g(&kind).clone();
        let nm = clip(b.s("name").trim(), 60);
        let name = if nm.is_empty() { format!("{}{} key", kind[..1].to_uppercase(), &kind[1..]) } else { nm };
        let key = format!("hk_{}", rand_b64url(24));
        let now = now_ms();
        let exp = if b.n("expiresAt") > now { b.n("expiresAt") } else if b.n("expiresInDays") > 0.0 { now + b.n("expiresInDays").min(3650.0) * 86400000.0 } else { 0.0 };
        let session = match b.s("session").as_str() { "key" => "key", "named" => "named", _ => "none" };
        let rec = json!({"id": rand_hex(4), "name": name, "kind": kind, "hash": sha256_hex(key.as_bytes()), "last4": &key[key.len() - 4..], "models": Self::pick_list(b.g("models"), &MODELS, pre.g("models")), "efforts": Self::pick_list(b.g("efforts"), &EFFORTS, pre.g("efforts")),
            "perDay": num(b.n("perDay").round().clamp(0.0, 100000.0)), "weekStopPct": num(b.n("weekStopPct").clamp(0.0, 100.0)), "sessionStopPct": num(b.n("sessionStopPct").clamp(0.0, 100.0)), "session": session, "sessions": {}, "expiresAt": num(exp), "created": now as i64, "revoked": false, "day": day_of(now), "count": 0, "total": 0, "tokens": 0});
        let mut db = self.db.lock().unwrap(); if let Some(a) = db.get_mut("keys").and_then(|x| x.as_array_mut()) { a.push(rec.clone()); } self.save_v(&db);
        merge(&json!({"key": key}), &Self::view(&rec))
    }
    pub fn list(&self) -> Vec<Value> { arr(self.db.lock().unwrap().g("keys")).iter().map(Self::view).collect() }
    fn with_key<R>(&self, id: &str, f: impl FnOnce(&mut Value) -> R) -> Option<R> {
        let mut db = self.db.lock().unwrap();
        let r = db.get_mut("keys").and_then(|x| x.as_array_mut()).and_then(|a| a.iter_mut().find(|k| k.s("id") == id)).map(f);
        if r.is_some() { self.save_v(&db); }
        r
    }
    pub fn revoke(&self, id: &str) -> bool { self.with_key(id, |k| set(k, "revoked", json!(true))).is_some() }
    pub fn remove(&self, id: &str) -> bool { let mut db = self.db.lock().unwrap(); let Some(a) = db.get_mut("keys").and_then(|x| x.as_array_mut()) else { return false }; let n = a.len(); a.retain(|k| k.s("id") != id); let ch = a.len() != n; if ch { self.save_v(&db); } ch }
    pub fn authenticate(&self, token: &str) -> Option<String> {
        if !regex::Regex::new(r"^hk_[A-Za-z0-9_-]{20,}$").unwrap().is_match(token) { return None; }
        let h = sha256_hex(token.as_bytes());
        arr(self.db.lock().unwrap().g("keys")).into_iter().filter(|k| k.s("hash") == h).last().map(|k| k.s("id"))
    }
    pub fn get(&self, id: &str) -> Value { arr(self.db.lock().unwrap().g("keys")).into_iter().find(|k| k.s("id") == id).unwrap_or(Value::Null) }
    /// Can this key be used right now?
    pub fn check(&self, id: &str, week: Option<f64>, sess: Option<f64>) -> Result<(), (u16, String)> {
        self.with_key(id, |k| {
            if k.t("revoked") { return Err((401, "This key was revoked.".to_string())); }
            if k.n("expiresAt") > 0.0 && k.n("expiresAt") <= now_ms() { return Err((401, "This key has expired.".into())); }
            if k.s("day") != day_of(now_ms()) { set(k, "day", json!(day_of(now_ms()))); set(k, "count", json!(0)); }
            if k.n("perDay") > 0.0 && k.n("count") >= k.n("perDay") { return Err((429, format!("This key may make {} requests a day and has used them. It resets at midnight UTC.", k.n("perDay")))); }
            if let Some(w) = week { if k.n("weekStopPct") > 0.0 && w >= k.n("weekStopPct") { return Err((429, format!("This key stops when {}% of the weekly usage is used (now {}%).", crate::jsu::js_string(&num(k.n("weekStopPct"))), w.round()))); } }
            if let Some(s) = sess { if k.n("sessionStopPct") > 0.0 && s >= k.n("sessionStopPct") { return Err((429, format!("This key stops when {}% of the 5-hour session is used (now {}%).", crate::jsu::js_string(&num(k.n("sessionStopPct"))), s.round()))); } }
            Ok(())
        }).unwrap_or(Err((401, "invalid API key".into())))
    }
    /// The remembered Claude session of a key (or of a name under that key): (id, fresh, name).
    pub fn session(&self, id: &str, name: &str, reset: bool) -> (String, bool, String) {
        self.with_key(id, |k| {
            let nm = if k.s("session") == "key" { "main".to_string() } else { name.to_string() };
            let mut ss = if k.g("sessions").is_object() { k.g("sessions").clone() } else { obj() };
            let cur = ss.g(&nm).clone();
            let s = if cur.is_null() || reset { let s = json!({"id": uuid4(), "started": false, "last": 0}); set(&mut ss, &nm, s.clone()); s } else { cur };
            set(k, "sessions", ss);
            (s.s("id"), !s.t("started"), nm)
        }).unwrap_or_default()
    }
    pub fn session_used(&self, id: &str, name: &str) { self.with_key(id, |k| { if let Some(s) = k.get_mut("sessions").and_then(|x| x.get_mut(name)) { set(s, "started", json!(true)); set(s, "last", json!(now_i())); } }); }
    pub fn session_forget(&self, id: &str, name: &str) { self.with_key(id, |k| { if let Some(m) = k.get_mut("sessions").and_then(|x| x.as_object_mut()) { m.remove(name); } }); }
    pub fn used(&self, id: &str, tokens: f64) { self.with_key(id, |k| { if k.s("day") != day_of(now_ms()) { set(k, "day", json!(day_of(now_ms()))); set(k, "count", json!(0)); } let c = k.n("count") + 1.0; set(k, "count", num(c)); set(k, "total", num(k.n("total") + 1.0)); set(k, "tokens", num(k.n("tokens") + tokens)); set(k, "lastUsed", json!(now_i())); }); }
    fn difficulty(text: &str, wanted: &str) -> usize {
        let l = text.chars().count();
        let mut d = if l > 12000 { 3 } else if l > 3000 { 2 } else if l > 400 { 1 } else { 0 };
        if regex::Regex::new(r"(?i)\b(prove|derive|architecture|refactor|debug|optimi[sz]e|analy[sz]e|step by step|algorithm|plan|design|security|migrate|review)\b").unwrap().is_match(text) { d = (d + 1).min(3); }
        if regex::Regex::new(r"```|\bfunction\b|\bclass\b|\bSELECT\b|\bimport\b").unwrap().is_match(text) { d = (d + 1).min(3); }
        if let Some(w) = EFFORTS.iter().position(|e| *e == wanted) { d = d.max(3.min(((w * 3) as f64 / (EFFORTS.len() - 1) as f64).round() as usize)); }
        d
    }
    fn spread(list: &[String], d: usize) -> String { list[(list.len() - 1).min(((d * (list.len() - 1)) as f64 / 3.0).round() as usize)].clone() }
    /// The model and effort for a request: (model, effort, auto) or an error.
    pub fn choose(&self, id: &str, model: &str, effort: &str, text: &str) -> Result<(String, String, bool), String> {
        let k = self.get(id);
        let models: Vec<String> = arr(k.g("models")).iter().map(vstr).collect(); let efforts: Vec<String> = arr(k.g("efforts")).iter().map(vstr).collect();
        let want = model.to_lowercase(); let named = MODELS.iter().find(|m| want.contains(*m)).map(|m| m.to_string());
        if let Some(n) = &named { if !models.contains(n) { return Err(format!("This key may use {}, not {}. Ask for \"auto\" to let Hearth pick.", models.join(", "), n)); } }
        let ef = effort.to_lowercase();
        if !ef.is_empty() && ef != "auto" && EFFORTS.contains(&ef.as_str()) && !efforts.contains(&ef) { return Err(format!("This key may use the effort levels {}, not {}.", efforts.join(", "), ef)); }
        let d = Self::difficulty(text, if !ef.is_empty() && ef != "auto" { &ef } else { "" });
        if models.is_empty() || efforts.is_empty() { return Err("This key allows no model.".into()); }
        Ok((named.clone().unwrap_or_else(|| Self::spread(&models, d)), if !ef.is_empty() && ef != "auto" && EFFORTS.contains(&ef.as_str()) { ef } else { Self::spread(&efforts, d) }, named.is_none()))
    }
}
