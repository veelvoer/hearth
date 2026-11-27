//! File sync for the projects folder, over the connection the apps already use. Port of filehub.js.
//! The server keeps the shared copy; each computer remembers what it last synced, so it can tell "changed here", "changed there",
//! "deleted here" and "deleted there" apart. Both changed: the newest wins and the other is kept as "name.conflict-<computer>-<time>".
//! Deleted files go to .hearth-trash, and a sync that would delete many files at once is stopped.
use crate::agent::Transport;
use crate::util::*;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const SLACK: f64 = 2500.0;
const MAX_FILE: u64 = 200 * 1024 * 1024;
const SKIP_DIRS: [&str; 17] = ["node_modules", ".venv", "venv", "__pycache__", ".gradle", "build", "dist", ".next", "target", ".cache", ".hearth-trash", ".idea", ".vs", ".mypy_cache", ".pytest_cache", ".stfolder", ".stversions"];
const TOP_SKIP: [&str; 5] = [".hearth-trash", ".stfolder", ".stversions", ".claude-chats", ".claude-sync"];
fn skip_file(n: &str) -> bool {
    n == ".DS_Store" || n == "Thumbs.db" || n.ends_with(".pyc") || n.ends_with(".swp") || n.ends_with('~') || n.starts_with(".hearth-tmp-") || n.starts_with(".syncthing.") || n.contains(".sync-conflict-") || n == "index.lock" || n.ends_with(".lock")
}

/// Inside a git folder (of any project)?
fn in_git(rel: &str) -> bool { rel.starts_with(".git/") || rel.contains("/.git/") }
/// A git object or pack: named after its own content, so two copies of one name are the same file, whatever their times say.
fn git_immutable(rel: &str) -> bool { in_git(rel) && (rel.contains("/objects/") || rel.contains(".git/objects/")) }

/// Every syncable file below root as { 'rel/path': [size, mtimeMs, mode] }.
pub fn scan(root: &str) -> BTreeMap<String, [f64; 3]> {
    let mut out = BTreeMap::new();
    fn walk(dir: &Path, rel: &str, top: bool, out: &mut BTreeMap<String, [f64; 3]>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if top && TOP_SKIP.contains(&n.as_str()) { continue; }
            let r = if rel.is_empty() { n.clone() } else { format!("{rel}/{n}") };
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() { if !SKIP_DIRS.contains(&n.as_str()) { walk(&e.path(), &r, false, out); } }
            else if ft.is_file() && !skip_file(&n) && !(n.contains(".conflict-") && in_git(&r)) {   // copies a past conflict left inside a git folder are junk
                if let Ok(m) = e.metadata() { if m.len() <= MAX_FILE { out.insert(r, [m.len() as f64, mtime_ms(&m).floor(), mode_of(&m) as f64]); } }
            }
        }
    }
    walk(Path::new(root), "", true, &mut out);
    out
}
fn mode_of(m: &std::fs::Metadata) -> u32 {
    #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; m.permissions().mode() & 0o777 }
    #[cfg(not(unix))] { let _ = m; 0o644 }
}
pub fn safe_rel(rel: &str) -> bool { !rel.is_empty() && !rel.contains('\0') && !rel.starts_with('/') && !rel.contains('\\') && !rel.split('/').any(|p| p == ".." || p.is_empty() || p == ".") }
fn abs(root: &str, rel: &str) -> PathBuf { let mut p = PathBuf::from(root); for x in rel.split('/') { p.push(x); } p }
fn stamp() -> String { chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string() }
fn write_atomic(file: &Path, buf: &[u8], mtime: f64, mode: u32) -> std::io::Result<()> {
    let d = file.parent().unwrap_or(Path::new("/")); std::fs::create_dir_all(d)?;
    let tmp = d.join(format!(".hearth-tmp-{}-{}", std::process::id(), rand_hex(3)));
    std::fs::write(&tmp, buf)?;
    #[cfg(unix)] if mode != 0 { use std::os::unix::fs::PermissionsExt; let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(mode & 0o777)); }
    #[cfg(not(unix))] let _ = mode;
    set_mtime(&tmp, mtime);
    std::fs::rename(&tmp, file)
}
fn to_trash(root: &str, rel: &str) -> std::io::Result<()> {
    let from = abs(root, rel); if !from.exists() { return Ok(()); }
    let mut to = PathBuf::from(root).join(".hearth-trash").join(stamp()); for x in rel.split('/') { to.push(x); }
    if let Some(d) = to.parent() { std::fs::create_dir_all(d)?; }
    if std::fs::rename(&from, &to).is_err() { std::fs::copy(&from, &to)?; std::fs::remove_file(&from)?; }
    Ok(())
}
fn conflict_name(rel: &str, who: &str) -> String {
    let w: String = who.chars().filter(|c| c.is_ascii_alphanumeric()).take(12).collect();
    let tag = format!(".conflict-{}-{}", if w.is_empty() { "other".into() } else { w }, stamp());
    let i = rel.rfind('.'); let s = rel.rfind('/');
    match i { Some(i) if s.map(|s| i > s + 1).unwrap_or(i > 0) => format!("{}{}{}", &rel[..i], tag, &rel[i..]), _ => format!("{rel}{tag}") }
}

// ───────── server side ─────────
pub struct HubFiles { root: String, cache: Mutex<(f64, Option<BTreeMap<String, [f64; 3]>>)> }
impl HubFiles {
    pub fn new(root: &str) -> Self { HubFiles { root: root.into(), cache: Mutex::new((0.0, None)) } }
    pub fn manifest(&self) -> BTreeMap<String, [f64; 3]> {
        let mut c = self.cache.lock().unwrap();
        if c.1.is_none() || now_ms() - c.0 > 1000.0 { *c = (now_ms(), Some(scan(&self.root))); }
        c.1.clone().unwrap_or_default()
    }
    fn touch(&self) { self.cache.lock().unwrap().1 = None; }
    pub fn count(&self) -> usize { self.manifest().len() }
    /// base: what the sending computer believes the server's copy looked like (None = it thinks there is none, Some(None) = not given).
    pub fn put(&self, rel: &str, buf: &[u8], mtime: f64, mode: u32, keep_old: bool, who: &str, base: Option<Option<(f64, f64)>>) -> Value {
        if !safe_rel(rel) { return json!({"error": "bad path"}); }
        let f = abs(&self.root, rel);
        let mut changed_meanwhile = false;
        if let Ok(st) = std::fs::metadata(&f) {
            if let Some(b) = base {
                let mt = mtime_ms(&st).floor();
                let differs = match b { None => true, Some((sz, bm)) => st.len() as f64 != sz || (mt - bm).abs() > SLACK };
                changed_meanwhile = differs && !(st.len() as usize == buf.len() && (mt - mtime).abs() <= SLACK);
            }
        }
        if (keep_old || changed_meanwhile) && f.exists() { let _ = std::fs::copy(&f, abs(&self.root, &conflict_name(rel, who))); }
        match write_atomic(&f, buf, mtime, mode) { Ok(()) => { self.touch(); json!({"ok": true}) } Err(e) => json!({"error": e.to_string()}) }
    }
    pub fn get(&self, rel: &str) -> Option<(Vec<u8>, f64, u32)> {
        if !safe_rel(rel) { return None; }
        let f = abs(&self.root, rel); let m = std::fs::metadata(&f).ok()?; if !m.is_file() { return None; }
        Some((std::fs::read(&f).ok()?, mtime_ms(&m).floor(), mode_of(&m)))
    }
    pub fn remove(&self, rel: &str) -> Value { if !safe_rel(rel) { return json!({"error": "bad path"}); } let _ = to_trash(&self.root, rel); self.touch(); json!({"ok": true}) }
}

// ───────── computer side ─────────
#[derive(Default)]
struct FState { arrived: Vec<Value>, running: bool, dirty: bool, stopped: bool, last: Value, pending: Vec<Value>, paused: Value, timer: Option<tokio::task::JoinHandle<()>>, files: serde_json::Map<String, Value>, accepted: Vec<String>, rejected: Vec<String> }
pub struct FileAgent { root: String, state_file: PathBuf, name: String, tr: Transport, st: Mutex<FState> }
fn arr3(v: &Value) -> Option<[f64; 3]> { let a = v.as_array()?; Some([vnum(a.first()?), vnum(a.get(1)?), a.get(2).map(vnum).unwrap_or(0.0)]) }
fn differs(a: Option<[f64; 3]>, b: Option<[f64; 3]>) -> bool { match (a, b) { (Some(a), Some(b)) => a[0] != b[0] || (a[1] - b[1]).abs() > SLACK, _ => true } }
impl FileAgent {
    pub fn new(root: String, state_file: PathBuf, name: String, tr: Transport) -> Arc<Self> {
        let mut st = FState { last: Value::Null, paused: Value::Null, ..Default::default() };
        if let Some(s) = read_json(&state_file) {
            st.files = s.g("files").as_object().cloned().unwrap_or_default();
            st.accepted = arr(s.g("accepted")).iter().map(vstr).collect();
            st.rejected = arr(s.g("rejected")).iter().map(vstr).collect();
        }
        Arc::new(FileAgent { root, state_file, name, tr, st: Mutex::new(st) })
    }
    fn save(&self) {
        let s = self.st.lock().unwrap();
        let v = json!({"files": s.files, "accepted": s.accepted, "rejected": s.rejected});
        if let Some(d) = self.state_file.parent() { let _ = std::fs::create_dir_all(d); }
        let tmp = PathBuf::from(format!("{}.tmp", self.state_file.display()));
        if std::fs::write(&tmp, v.to_string()).is_ok() { let _ = std::fs::rename(&tmp, &self.state_file); }
    }
    pub fn start(self: &Arc<Self>, r: Arc<crate::relay::Relay>) {
        let me = self.clone();
        let h = tokio::spawn(async move {
            me.sync_once(&r, "start").await;
            loop { tokio::time::sleep(std::time::Duration::from_secs(if std::env::var_os("HEARTH_TEST").is_some() { 12 } else { 6 })).await; if me.st.lock().unwrap().stopped { return; } me.sync_once(&r, "timer").await; }
        });
        self.st.lock().unwrap().timer = Some(h);
    }
    pub fn stop(&self) { let mut s = self.st.lock().unwrap(); s.stopped = true; if let Some(t) = s.timer.take() { t.abort(); } }
    pub fn status(&self) -> Value { let s = self.st.lock().unwrap(); json!({"arrived": s.arrived, "last": s.last, "pending": s.pending, "paused": s.paused, "rejected": s.rejected, "tracked": s.files.len()}) }
    pub fn pending(&self) -> Vec<Value> { self.st.lock().unwrap().pending.clone() }

    pub fn sync_once<'a>(self: &'a Arc<Self>, r: &'a Arc<crate::relay::Relay>, reason: &'a str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Value> + Send + 'a>> {
        Box::pin(async move {
        { let mut s = self.st.lock().unwrap(); if s.running { s.dirty = true; return s.last.clone(); } s.running = true; }
        let t0 = now_ms();
        let (mut pushed, mut pulled, mut deleted, mut conflicts, mut waiting) = (0, 0, 0, 0, 0);
        let mut errors: Vec<String> = Vec::new();
        let mut paused = Value::Null;
        let mut nfiles = Value::Null;
        let res: Result<(), String> = async {
            if !Path::new(&self.root).exists() { return Err(format!("The projects folder is missing: {}", self.root)); }
            let Some((mb, _)) = self.tr.get("/agent/files/manifest", 300000).await? else { return Err("The server does not do file sync yet. Update the server.".into()) };
            let hubv: Value = serde_json::from_slice(&mb).unwrap_or_else(|_| obj());
            let hub = hubv.g("files").as_object().cloned().unwrap_or_default();
            let local = scan(&self.root);
            let base = self.st.lock().unwrap().files.clone();
            let (accepted, rejected) = { let s = self.st.lock().unwrap(); (s.accepted.clone(), s.rejected.clone()) };
            let (mut push, mut pull, mut del_local, mut del_hub, mut conflict) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::<(String, &str)>::new());
            let mut pend: BTreeMap<String, (f64, f64, f64)> = BTreeMap::new();
            let mut auto_new: HashSet<String> = HashSet::new();
            let mut rels: Vec<String> = local.keys().cloned().chain(hub.keys().cloned()).chain(base.keys().cloned()).collect::<HashSet<_>>().into_iter().collect();
            rels.sort();
            let mut files_next = base.clone();
            for rel in rels {
                let l = local.get(&rel).copied(); let h = hub.get(&rel).and_then(arr3); let b = base.get(&rel).cloned();
                let bl = b.as_ref().and_then(|b| arr3(b.g("l"))); let bh = b.as_ref().and_then(|b| arr3(b.g("h")));
                if l.is_some() && h.is_some() && !differs(l, h) { files_next.insert(rel.clone(), json!({"l": l.unwrap().map(num), "h": h.unwrap().map(num)})); continue; }
                // git objects keep their name for their content: both sides having one is agreement (no conflict copies of 400,000 objects)
                if l.is_some() && h.is_some() && git_immutable(&rel) { files_next.insert(rel.clone(), json!({"l": l.unwrap().map(num), "h": h.unwrap().map(num)})); continue; }
                let lc = l.is_some() && (b.is_none() || differs(l, bl)); let hc = h.is_some() && (b.is_none() || differs(h, bh));
                if l.is_some() && h.is_some() {
                    if lc && !hc { push.push(rel); } else if hc && !lc { pull.push(rel); }
                    else if in_git(&rel) { if l.unwrap()[1] >= h.unwrap()[1] { push.push(rel); } else { pull.push(rel); } }   // git's own state files (index, HEAD, refs): the newest wins, no copies
                    else if l.unwrap()[1] >= h.unwrap()[1] { push.push(rel.clone()); conflict.push((rel, "hub")); }
                    else { pull.push(rel.clone()); conflict.push((rel, "local")); }
                } else if l.is_some() { if b.is_some() && !lc { del_local.push(rel); } else { push.push(rel); } }
                else if let Some(hv) = h {
                    if b.is_some() && !hc { del_hub.push(rel); }
                    else {
                        let top = if rel.contains('/') { rel.split('/').next().unwrap_or("").to_string() } else { String::new() };
                        if !top.is_empty() && rejected.contains(&top) && !Path::new(&self.root).join(&top).exists() { continue; }
                        if !top.is_empty() && b.is_none() && !accepted.contains(&top) && !Path::new(&self.root).join(&top).exists() {
                            // a project made on another device: it comes here by itself (a project you rejected stays away); the app says so
                            let p = pend.entry(top.clone()).or_insert((0.0, 0.0, 0.0)); p.0 += 1.0; p.1 += hv[0]; p.2 = p.2.max(hv[1]);
                            auto_new.insert(top);
                        }
                        pull.push(rel);
                    }
                } else { files_next.remove(&rel); }
            }
            {   // accepted for good, and remembered as "arrived" so the app can tell you
                let mut st = self.st.lock().unwrap();
                for (n, p) in &pend { if auto_new.contains(n) { if !st.accepted.contains(n) { st.accepted.push(n.clone()); } st.arrived.push(json!({"name": n, "files": num(p.0), "bytes": num(p.1), "at": now_ms() as i64})); } }
                let cut = now_ms() as i64 - 6 * 3600_000; st.arrived.retain(|a| a.n("at") as i64 > cut);
            }
            pend.clear();
            let mut pending: Vec<Value> = pend.into_iter().map(|(n, p)| json!({"name": n, "files": num(p.0), "bytes": num(p.1), "newest": num(p.2)})).collect();
            pending.sort_by(|a, b| b.n("newest").partial_cmp(&a.n("newest")).unwrap_or(std::cmp::Ordering::Equal));
            r.link.lock().unwrap().pending_projects = pending.clone();
            self.st.lock().unwrap().pending = pending;
            // safety brake: a sync that would delete a lot is never right
            let known = base.len() as f64; let dels = (del_local.len() + del_hub.len()) as f64;
            if dels > 15f64.max(known * 0.25) { paused = json!(format!("{} files look deleted. Nothing was deleted. If that is right, delete them again later in smaller steps.", dels)); del_local.clear(); del_hub.clear(); }
            for rel in push {
                let l = local[&rel];
                if now_ms() - l[1] < 2000.0 { waiting += 1; continue; }
                let p = abs(&self.root, &rel);
                let (Ok(buf), Ok(s2)) = (std::fs::read(&p), std::fs::metadata(&p)) else { errors.push(format!("send {rel}: cannot read it")); continue };
                if s2.len() as f64 != l[0] || mtime_ms(&s2).floor() != l[1] { waiting += 1; continue; }
                let keep = conflict.iter().any(|c| c.0 == rel && c.1 == "hub");
                let bh = base.get(&rel).and_then(|b| arr3(b.g("h")));
                let mut q = vec![("rel", rel.clone()), ("mtime", (l[1] as i64).to_string()), ("mode", (l[2] as i64).to_string()), ("keepOld", if keep { "1".into() } else { "0".into() }), ("who", self.name.clone())];
                match bh { Some(b) => { q.push(("baseSize", (b[0] as i64).to_string())); q.push(("baseMtime", (b[1] as i64).to_string())); } None => q.push(("baseNone", "1".into())) }
                match self.tr.put(&format!("/agent/files/put?{}", qbuild(&q)), gzip(&buf, 3), 300000).await {
                    Ok(r) if r.s("error").is_empty() => { files_next.insert(rel.clone(), json!({"l": l.map(num), "h": [num(l[0]), num(l[1])]})); pushed += 1; if keep { conflicts += 1; } }
                    Ok(r) => errors.push(format!("send {rel}: {}", r.s("error"))),
                    Err(e) => errors.push(format!("send {rel}: {e}")),
                }
            }
            for rel in pull {
                let cur = local.get(&rel).copied(); let keep = conflict.iter().any(|c| c.0 == rel && c.1 == "local");
                match self.tr.get(&format!("/agent/files/get?{}", qbuild(&[("rel", rel.clone())])), 300000).await {
                    Ok(None) => { waiting += 1; continue; }
                    Err(e) => { errors.push(format!("receive {rel}: {e}")); continue; }
                    Ok(Some((body, h))) => {
                        if let Some(c) = cur { if let Ok(now) = std::fs::metadata(abs(&self.root, &rel)) { if now.len() as f64 != c[0] || mtime_ms(&now).floor() != c[1] { waiting += 1; continue; } } }
                        if keep && cur.is_some() { let _ = std::fs::copy(abs(&self.root, &rel), abs(&self.root, &conflict_name(&rel, &self.name))); conflicts += 1; }
                        let mt = h.get("x-mtime").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<f64>().ok()).unwrap_or_else(now_ms);
                        let md = h.get("x-mode").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u32>().ok()).unwrap_or(0o644);
                        if let Err(e) = write_atomic(&abs(&self.root, &rel), &body, mt, md) { errors.push(format!("receive {rel}: {e}")); continue; }
                        if let Ok(s) = std::fs::metadata(abs(&self.root, &rel)) { files_next.insert(rel.clone(), json!({"l": [num(s.len() as f64), num(mtime_ms(&s).floor()), num(mode_of(&s) as f64)], "h": [num(s.len() as f64), num(mt)]})); }
                        pulled += 1;
                    }
                }
            }
            for rel in del_local { match to_trash(&self.root, &rel) { Ok(()) => { files_next.remove(&rel); deleted += 1; } Err(e) => errors.push(format!("remove {rel}: {e}")) } }
            for rel in del_hub { match self.tr.json("/agent/files/delete", &json!({"rel": rel}), 20000).await { Ok(r) if r.s("error").is_empty() => { files_next.remove(&rel); deleted += 1; } Ok(r) => errors.push(format!("remove on server {rel}: {}", r.s("error"))), Err(e) => errors.push(format!("remove on server {rel}: {e}")) } }
            self.st.lock().unwrap().files = files_next;
            self.save();
            nfiles = json!(local.len());
            Ok(())
        }.await;
        if let Err(e) = res { errors.push(e); }
        let stat = json!({"at": t0 as i64, "reason": reason, "pushed": pushed, "pulled": pulled, "deleted": deleted, "conflicts": conflicts, "errors": errors, "waiting": waiting, "files": nfiles, "ok": errors.is_empty(), "ms": (now_ms() - t0) as i64, "paused": paused});
        if pushed + pulled + deleted + conflicts > 0 || !errors.is_empty() { r.log(&format!("files: sent {pushed}, received {pulled}, removed {deleted}, conflicts {conflicts}{}", if errors.is_empty() { String::new() } else { format!(", errors: {}", errors.iter().take(2).cloned().collect::<Vec<_>>().join("; ")) })); }
        let again = { let mut s = self.st.lock().unwrap(); s.last = stat.clone(); s.paused = paused; s.running = false; std::mem::take(&mut s.dirty) };
        if again { let me = self.clone(); let r2 = r.clone(); tokio::spawn(async move { tokio::time::sleep(std::time::Duration::from_millis(500)).await; me.sync_once(&r2, "again").await; }); }
        stat
        })
    }
    pub async fn accept(self: &Arc<Self>, r: &Arc<crate::relay::Relay>, names: &[String], all: bool) -> Value {
        { let mut s = self.st.lock().unwrap(); let pend = s.pending.clone();
          for p in &pend { let n = p.s("name"); if all || names.contains(&n) { if !s.accepted.contains(&n) { s.accepted.push(n); } } }
          s.pending.retain(|p| !(all || names.contains(&p.s("name")))); }
        self.save(); self.sync_once(r, "accepted").await
    }
    pub fn reject(&self, names: &[String], all: bool) {
        { let mut s = self.st.lock().unwrap(); let pend = s.pending.clone();
          for p in &pend { let n = p.s("name"); if all || names.contains(&n) { if !s.rejected.contains(&n) { s.rejected.push(n); } } }
          s.pending.retain(|p| !(all || names.contains(&p.s("name")))); }
        self.save();
    }
    pub async fn allow(self: &Arc<Self>, r: &Arc<crate::relay::Relay>, names: &[String]) -> Value { { let mut s = self.st.lock().unwrap(); s.rejected.retain(|n| !names.contains(n)); } self.save(); self.sync_once(r, "allowed").await }
}

#[cfg(test)]
mod git_tests {
    use super::*;
    #[test]
    fn git_rules() {
        assert!(git_immutable(".git/objects/0a/9d538f") && git_immutable("oxide/.git/objects/pack/pack-1.pack"));
        assert!(!git_immutable("oxide/.git/index") && !git_immutable("src/objects/a.rs"));
        assert!(in_git("oxide/.git/HEAD") && !in_git("src/gitx/a"));
    }
}
