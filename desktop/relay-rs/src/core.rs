//! Claude Code's chat logs, read fast: the Rust twin of the heavy parts of relay/core.js (titles and totals, messages, search, usage).
use crate::jsu::*;
use crate::pricing::{cost_of, Tokens};
use serde::Deserialize;
use serde_json::{json, value::RawValue, Map, Value};
use std::{
    borrow::Cow,
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Instant, UNIX_EPOCH},
};

const JSONL_TAIL: u64 = 48 * 1024 * 1024;

#[derive(Clone)]
pub struct FileInfo { pub id: String, pub file: PathBuf, pub mtime: f64, pub size: u64 }

/// The home folder, like Node's `os.homedir()`: HOME, or USERPROFILE on Windows.
fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").ok().filter(|h| !h.is_empty()).or_else(|| std::env::var("USERPROFILE").ok()).unwrap_or_default())
}

pub fn projects_dir() -> PathBuf {
    home().join(".claude").join("projects")
}

/// Every readable chat file, like `listFiles` of core.js.
/// Chat files or folders that exist but cannot be read (made by a run with root rights).
pub static DENIED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub fn denied_count() -> u64 { DENIED.load(std::sync::atomic::Ordering::Relaxed) }

pub fn list_files() -> Vec<FileInfo> {
    let mut out = Vec::new();
    let Ok(dirs) = std::fs::read_dir(projects_dir()) else { return out };
    for d in dirs.flatten() {
        let name = d.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || !d.path().is_dir() { continue; }
        let files = match std::fs::read_dir(d.path()) { Ok(f) => f, Err(e) => { if e.kind() == std::io::ErrorKind::PermissionDenied { DENIED.fetch_add(1, std::sync::atomic::Ordering::Relaxed); } continue } };
        for f in files.flatten() {
            let n = f.file_name().to_string_lossy().to_string();
            if !n.ends_with(".jsonl") { continue; }
            let Ok(md) = f.metadata() else { continue };
            if let Err(e) = File::open(f.path()) { if e.kind() == std::io::ErrorKind::PermissionDenied { DENIED.fetch_add(1, std::sync::atomic::Ordering::Relaxed); } continue; }
            let mtime = md.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs_f64() * 1000.0).unwrap_or(0.0);
            out.push(FileInfo { id: n[..n.len() - 6].to_string(), file: f.path(), mtime, size: md.len() });
        }
    }
    out
}

fn parse_val(line: &[u8]) -> Option<Value> {
    if line.is_empty() { return None; }
    match serde_json::from_slice::<Value>(line) {
        Ok(v) => Some(v),
        Err(_) => serde_json::from_str::<Value>(&String::from_utf8_lossy(line)).ok(),
    }
}

/// Every entry of a chat file like `readJsonl`: a file over 48 MB is read from its end (its cut first line is skipped).
pub fn read_jsonl(file: &Path) -> Vec<Value> {
    let Ok(mut f) = File::open(file) else { return vec![] };
    let size = f.metadata().map(|m| m.len()).unwrap_or(0);
    let mut buf = Vec::new();
    if size > JSONL_TAIL {
        if f.seek(SeekFrom::Start(size - JSONL_TAIL)).is_err() { return vec![]; }
        if f.take(JSONL_TAIL).read_to_end(&mut buf).is_err() { return vec![]; }
        let nl = memchr::memchr(b'\n', &buf);
        buf.drain(..nl.map(|i| i + 1).unwrap_or(0));
    } else if f.read_to_end(&mut buf).is_err() { return vec![]; }
    buf.split(|b| *b == b'\n').filter(|l| !l.is_empty()).filter_map(parse_val).collect()
}

// ───────── titles and totals ─────────
#[derive(Clone, Default)]
pub struct R { pub title: String, pub cwd: String, pub tok: f64, pub cost: f64, pub model: String, last_id: String, last_w: f64, last_c: f64 }

#[derive(Deserialize)]
struct LineS<'a> {
    #[serde(rename = "type", default)] kind: Option<Value>,
    #[serde(rename = "isSidechain", default)] side: Option<Value>,
    #[serde(default)] cwd: Option<Value>,
    #[serde(rename = "aiTitle", default)] ai_title: Option<Value>,
    #[serde(default, borrow)] message: Option<&'a RawValue>,
}
#[derive(Deserialize)]
struct MsgS<'a> {
    #[serde(default)] id: Option<Value>,
    #[serde(default)] model: Option<Value>,
    #[serde(default)] usage: Option<Value>,
    #[serde(default, borrow)] content: Option<&'a RawValue>,
}
fn truthy(v: &Option<Value>) -> bool {
    match v { None | Some(Value::Null) => false, Some(Value::Bool(b)) => *b, Some(Value::Number(n)) => n.as_f64().map(|x| x != 0.0).unwrap_or(true), Some(Value::String(s)) => !s.is_empty(), _ => true }
}

fn apply_line(line: &[u8], r: &mut R) {
    let owned;
    let parsed = match serde_json::from_slice::<LineS>(line) {
        Ok(p) => p,
        Err(_) => { owned = String::from_utf8_lossy(line).to_string(); match serde_json::from_str::<LineS>(&owned) { Ok(p) => p, Err(_) => return } }
    };
    let kind = parsed.kind.as_ref().and_then(|k| k.as_str()).unwrap_or("");
    let side = truthy(&parsed.side);
    let msg: Option<MsgS> = parsed.message.and_then(|m| serde_json::from_str(m.get()).ok());
    if kind == "assistant" && !side {
        if let Some(m) = &msg {
            if let Some(u @ Value::Object(_)) = &m.usage {
                let model = m.model.as_ref().and_then(|x| x.as_str()).unwrap_or("");
                if !model.starts_with('<') {
                    let (i, o, cc, cw) = (num(u.get("input_tokens")), num(u.get("output_tokens")), num(u.get("cache_read_input_tokens")), num(u.get("cache_creation_input_tokens")));
                    let w = i + o + cw;
                    let cost = cost_of(model, &Tokens { input: i, out: o, cr: cc, cw });
                    let id = m.id.as_ref().and_then(|x| x.as_str()).unwrap_or("");
                    if !id.is_empty() && id == r.last_id { r.tok += (w - r.last_w).max(0.0); r.cost += (cost - r.last_c).max(0.0); } else { r.tok += w; r.cost += cost; }
                    r.last_id = id.to_string(); r.last_w = w; r.last_c = cost;
                    if !model.is_empty() { r.model = model.to_string(); }
                }
            }
        }
    }
    if kind == "ai-title" { if let Some(Value::String(t)) = &parsed.ai_title { if !t.is_empty() { r.title = t.clone(); } } }
    if r.cwd.is_empty() { if let Some(Value::String(c)) = &parsed.cwd { r.cwd = c.clone(); } }
    if r.title.is_empty() && kind == "user" && !side {
        let content: Option<Value> = msg.as_ref().and_then(|m| m.content).and_then(|c| serde_json::from_str(c.get()).ok());
        let t = text_of(content.as_ref());
        let t = trim(&t);
        if !t.is_empty() && !t.starts_with('<') { r.title = slice16(t, 0, 80); }
    }
}

#[derive(Clone)]
struct SumEntry { mtime: f64, size: u64, read: u64, pos: u64, head: Vec<u8>, base: R, out: R }

static SUMS: Mutex<Option<HashMap<PathBuf, SumEntry>>> = Mutex::new(None);

fn head_of(file: &Path, n: u64) -> Vec<u8> {
    let mut b = Vec::new();
    if let Ok(f) = File::open(file) { let _ = f.take(n).read_to_end(&mut b); }
    b
}
fn same_start(file: &Path, head: &[u8]) -> bool { head_of(file, head.len() as u64) == head }

fn summarize(f: &FileInfo, hit: Option<SumEntry>) -> Option<SumEntry> {
    if let Some(h) = &hit { if h.mtime == f.mtime && h.size == f.size { return hit; } }
    let grew = hit.as_ref().map(|h| f.size >= h.read && same_start(&f.file, &h.head)).unwrap_or(false);
    let from = if grew { hit.as_ref().unwrap().pos } else { 0 };
    let mut r = if grew { hit.as_ref().unwrap().base.clone() } else { R::default() };
    let mut file = File::open(&f.file).ok()?;
    file.seek(SeekFrom::Start(from)).ok()?;
    let mut rd = BufReader::with_capacity(1 << 20, file.take(f.size.saturating_sub(from)));
    let mut line = Vec::new();
    let mut pos = from;
    let mut tail: Option<Vec<u8>> = None;
    loop {
        line.clear();
        let n = rd.read_until(b'\n', &mut line).ok()?;
        if n == 0 { break; }
        if line.last() == Some(&b'\n') { pos += n as u64; line.pop(); if !line.is_empty() { apply_line(&line, &mut r); } }
        else { tail = Some(line.clone()); }
    }
    let base = r.clone();
    if let Some(t) = tail { if parse_val(&t).is_some() { apply_line(&t, &mut r); } }
    let head = if grew { hit.unwrap().head } else { head_of(&f.file, 256.min(f.size)) };
    Some(SumEntry { mtime: f.mtime, size: f.size, read: f.size, pos, head, base, out: r })
}

fn r_json(r: &R) -> Value { json!({"title": r.title, "cwd": r.cwd, "tok": r.tok, "cost": r.cost, "model": r.model, "lastId": r.last_id, "lastW": r.last_w, "lastC": r.last_c}) }

/// The title, folder, tokens, cost and model of every chat, newest first. Chats that did not change are not read again.
pub fn summaries() -> Value {
    let mut files = list_files();
    files.sort_by(|a, b| b.mtime.partial_cmp(&a.mtime).unwrap_or(std::cmp::Ordering::Equal));
    let mut guard = SUMS.lock().unwrap();
    let cache = guard.get_or_insert_with(HashMap::new);
    let todo: Vec<(FileInfo, Option<SumEntry>)> = files.iter().map(|f| (f.clone(), cache.get(&f.file).cloned())).collect();
    use rayon::prelude::*;
    let done: Vec<(FileInfo, Option<SumEntry>)> = todo.into_par_iter().map(|(f, h)| { let e = summarize(&f, h); (f, e) }).collect();
    let mut out = Vec::new();
    for (f, e) in done {
        let Some(e) = e else { continue };
        cache.insert(f.file.clone(), e.clone());
        let head: String = e.head.iter().map(|b| *b as char).collect();
        out.push(json!({"id": f.id, "file": f.file.to_string_lossy(), "mtime": f.mtime, "size": f.size, "title": if e.out.title.is_empty() { "Untitled".to_string() } else { e.out.title.clone() },
            "cwd": e.out.cwd, "tokens": e.out.tok, "cost": e.out.cost, "model": e.out.model, "base": r_json(&e.base), "pos": e.pos, "read": e.read, "head": head}));
    }
    Value::Array(out)
}

// ───────── messages ─────────
pub fn tool_input(name: &str, arg: Option<&Value>) -> Value {
    let empty = Value::Object(Map::new());
    let arg = match arg { Some(a) if !a.is_null() => a, _ => &empty };
    let g = |k: &str| arg.get(k);
    let mut o = Map::new();
    match name {
        "Edit" | "NotebookEdit" => { o.insert("file_path".into(), json!(or_empty(g("file_path")))); o.insert("old_string".into(), json!(clip(g("old_string"), 1500))); o.insert("new_string".into(), json!(clip(g("new_string"), 1500))); }
        "MultiEdit" => {
            o.insert("file_path".into(), json!(or_empty(g("file_path"))));
            let edits: Vec<Value> = g("edits").and_then(|e| e.as_array()).map(|a| a.iter().take(6).map(|e| json!({"old_string": clip(e.get("old_string"), 800), "new_string": clip(e.get("new_string"), 800)})).collect()).unwrap_or_default();
            o.insert("edits".into(), Value::Array(edits));
        }
        "Write" => { o.insert("file_path".into(), json!(or_empty(g("file_path")))); o.insert("content".into(), json!(clip(g("content"), 1500))); }
        "Bash" => { o.insert("command".into(), json!(clip(g("command"), 1500))); o.insert("description".into(), json!(clip(g("description"), 200))); }
        "TodoWrite" => {   // the plan: every step and where it stands (the watch and the apps draw it as a checklist)
            let todos: Vec<Value> = g("todos").and_then(|t| t.as_array()).map(|a| a.iter().take(30).map(|t| json!({"content": clip(t.get("content"), 160), "status": or_empty(t.get("status")), "activeForm": clip(t.get("activeForm"), 160)})).collect()).unwrap_or_default();
            o.insert("todos".into(), Value::Array(todos));
        }
        _ => {
            if let Value::Object(m) = arg {
                for (k, v) in m.iter().take(6) {
                    let s = if let Value::String(s) = v { s.clone() } else { serde_json::to_string(v).unwrap_or_default() };
                    o.insert(k.clone(), json!(slice16(&s, 0, 300)));
                }
            }
        }
    }
    Value::Object(o)
}
fn or_empty(v: Option<&Value>) -> String { match v { Some(Value::String(s)) if !s.is_empty() => s.clone(), Some(x) if truthy(&Some(x.clone())) => js_string(x), _ => String::new() } }

pub fn result_text(c: Option<&Value>) -> String {
    match c {
        Some(Value::Array(a)) => slice16(&a.iter().map(|x| match x.get("text") { Some(Value::String(s)) => s.clone(), _ => String::new() }).collect::<Vec<_>>().join("\n"), 0, 2000),
        other => clip(other, 2000),
    }
}
pub fn tool_line(b: &Value) -> String {
    let a = b.get("input").cloned().unwrap_or(Value::Object(Map::new()));
    let k = ["command", "file_path", "pattern", "path", "url"].into_iter().find(|k| a.get(*k).is_some());
    let name = b.get("name").map(js_string).unwrap_or_default();
    trim(&format!("{} {}", name, k.map(|k| clip(a.get(k), 60)).unwrap_or_default())).to_string()
}
fn fmt_tok(n: f64) -> String {
    if n >= 1e6 { format!("{}M", to_fixed(n / 1e6, 1)) } else if n >= 1e3 { format!("{}k", to_fixed(n / 1e3, if n >= 1e4 { 0 } else { 1 })) } else { format!("{}", n.round() as i64) }
}
fn fmt_dur(ms: f64) -> String { let s = (ms / 1000.0).round() as i64; if s < 60 { format!("{s}s") } else { format!("{}m {}s", s / 60, s % 60) } }
fn turn_line(u: [f64; 4], ms: f64) -> String {
    let mut parts = vec![format!("{} output", fmt_tok(u[1])), format!("{} input", fmt_tok(u[0] + u[3]))];
    if u[2] != 0.0 { parts.push(format!("{} cached", fmt_tok(u[2]))); }
    if ms > 0.0 { parts.push(fmt_dur(ms)); }
    parts.join(" · ")
}

struct Turn { start: i64, end: i64, ids: Vec<(String, [f64; 4])> }

/// The last [tail] messages of a chat, like `messagesOf` of core.js.
pub fn messages(file: &Path, tail: usize) -> Value {
    let size = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
    // A long chat is read from its end, in growing pieces, until the piece holds enough messages and begins with one of your prompts
    // (a turn that starts inside the piece is complete). A chat under 48 MB, or the last 48 MB of a bigger one, is what the Node relay reads too.
    let mut window: u64 = 2 * 1024 * 1024;
    loop {
        if window >= size || window >= JSONL_TAIL { return compute_messages(&read_jsonl(file), tail); }
        let Ok(mut f) = File::open(file) else { return json!([]) };
        let mut buf = Vec::new();
        if f.seek(SeekFrom::Start(size - window)).is_err() || f.take(window).read_to_end(&mut buf).is_err() { return json!([]); }
        let nl = memchr::memchr(b'\n', &buf);
        buf.drain(..nl.map(|i| i + 1).unwrap_or(0));
        let entries: Vec<Value> = buf.split(|b| *b == b'\n').filter(|l| !l.is_empty()).filter_map(parse_val).collect();
        let first = entries.iter().position(|o| {
            o.get("type").and_then(|t| t.as_str()) == Some("user") && !truthy(&o.get("isSidechain").cloned())
                && { let t = text_of(o.get("message").and_then(|m| m.get("content"))); let t = trim(&t); !t.is_empty() && !t.starts_with('<') }
        });
        if let Some(i) = first {
            let out = compute_messages(&entries[i..], tail);
            if out.as_array().map(|a| a.len() >= tail).unwrap_or(false) { return out; }
        }
        window = window.saturating_mul(4);
    }
}

fn compute_messages(entries: &[Value], tail: usize) -> Value {
    let entries = entries;
    let mut results: HashMap<String, Value> = HashMap::new();
    for o in entries {
        let c = o.get("message").and_then(|m| m.get("content"));
        if o.get("type").and_then(|t| t.as_str()) == Some("user") {
            if let Some(Value::Array(a)) = c { for b in a { if b.get("type").and_then(|t| t.as_str()) == Some("tool_result") {
                let id = b.get("tool_use_id").and_then(|x| x.as_str()).unwrap_or("").to_string();
                results.insert(id, json!({"result": result_text(b.get("content")), "error": truthy(&b.get("is_error").cloned())}));
            } } }
        }
    }
    let mut out: Vec<Value> = Vec::new();
    let mut turn: Option<Turn> = None;
    let flush = |turn: &mut Option<Turn>, out: &mut Vec<Value>| {
        if let Some(t) = turn.take() {
            if !t.ids.is_empty() {
                let mut u = [0.0; 4];
                for (_, m) in &t.ids { for i in 0..4 { u[i] += m[i]; } }
                let ms = (t.end - t.start).max(0) as f64;
                out.push(json!({"role": "meta", "text": turn_line(u, ms), "tokens": {"in": u[0], "out": u[1], "cr": u[2], "cw": u[3]}, "ms": ms, "ts": t.end}));
            }
        }
    };
    for o in entries {
        let kind = o.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if truthy(&o.get("isSidechain").cloned()) || (kind != "user" && kind != "assistant") { continue; }
        let c = o.get("message").and_then(|m| m.get("content"));
        let ts = (iso_ts(o.get("timestamp")) * 1000.0).floor() as i64;
        if kind == "assistant" {
            if let Some(t) = turn.as_mut() {
                let m = o.get("message");
                t.end = t.end.max(ts);
                let us = m.and_then(|m| m.get("usage"));
                let mid = m.and_then(|m| m.get("id")).and_then(|x| x.as_str()).unwrap_or("");
                let model = m.and_then(|m| m.get("model")).and_then(|x| x.as_str()).unwrap_or("");
                if let (Some(us), true) = (us.filter(|u| truthy(&Some((*u).clone()))), !mid.is_empty() && !model.starts_with('<')) {
                    let r = [num(us.get("input_tokens")), num(us.get("output_tokens")), num(us.get("cache_read_input_tokens")), num(us.get("cache_creation_input_tokens"))];
                    match t.ids.iter_mut().find(|(k, _)| k == mid) { Some(old) => { if r[1] >= old.1[1] { old.1 = r; } } None => t.ids.push((mid.to_string(), r)) }
                }
            }
        }
        if kind == "user" {
            let t = text_of(c); let t = trim(&t);
            if !t.is_empty() && !t.starts_with('<') { flush(&mut turn, &mut out); turn = Some(Turn { start: ts, end: ts, ids: vec![] }); out.push(json!({"role": "user", "text": t, "ts": ts})); }
        } else if let Some(Value::Array(a)) = c {
            for b in a {
                let bt = b.get("type").and_then(|x| x.as_str()).unwrap_or("");
                if bt == "text" && b.get("text").map(|x| !trim(&js_string(x)).is_empty()).unwrap_or(false) {
                    out.push(json!({"role": "assistant", "text": trim(&js_string(b.get("text").unwrap())), "ts": ts}));
                } else if bt == "tool_use" {
                    let name = b.get("name").and_then(|x| x.as_str());
                    let mut m = Map::new();
                    m.insert("role".into(), json!("tool")); m.insert("text".into(), json!(tool_line(b)));
                    if let Some(n) = name { m.insert("name".into(), json!(n)); }
                    m.insert("input".into(), tool_input(name.unwrap_or(""), b.get("input")));
                    m.insert("ts".into(), json!(ts));
                    if let Some(Value::Object(r)) = results.get(b.get("id").and_then(|x| x.as_str()).unwrap_or("")) { for (k, v) in r { m.insert(k.clone(), v.clone()); } }
                    out.push(Value::Object(m));
                }
            }
        }
    }
    flush(&mut turn, &mut out);
    let n = out.len();
    Value::Array(out.split_off(n.saturating_sub(tail)))
}

// ───────── search ─────────
const SEARCH_CAP: u64 = 8 * 1024 * 1024;

pub fn search(q: &str, limit: usize) -> Value {
    let needle = trim(q).to_lowercase();
    if needle.encode_utf16().count() < 2 { return json!([]); }
    let t0 = Instant::now();
    let mut files = list_files();
    files.sort_by(|a, b| b.mtime.partial_cmp(&a.mtime).unwrap_or(std::cmp::Ordering::Equal));
    let ascii = needle.is_ascii();
    let nb = needle.as_bytes();
    let finder = memchr::memmem::Finder::new(nb).into_owned();
    let mut out = Vec::new();
    for f in files {
        if t0.elapsed().as_millis() > 4000 || out.len() >= limit { break; }
        let mut data = Vec::new();
        let Ok(mut fh) = File::open(&f.file) else { continue };
        if f.size > SEARCH_CAP { if fh.seek(SeekFrom::Start(f.size - SEARCH_CAP)).is_err() { continue; } }
        if fh.read_to_end(&mut data).is_err() { continue; }
        let lowered: Vec<u8> = if ascii { data.to_ascii_lowercase() } else { String::from_utf8_lossy(&data).to_lowercase().into_bytes() };
        if finder.find(&lowered).is_none() { continue; }
        // work line by line on the lowered copy; the lines of the original are needed for parsing
        let text_orig: Cow<str> = String::from_utf8_lossy(&data);
        let (mut hit, mut n) = (None::<String>, 0u32);
        if ascii {
            let mut start = 0usize;
            for (i, ch) in data.iter().enumerate().chain(std::iter::once((data.len(), &b'\n'))) {
                if *ch != b'\n' { continue; }
                let (lo, ln) = (&lowered[start..i], &data[start..i]);
                start = i + 1;
                if lo.is_empty() || finder.find(lo).is_none() { continue; }
                if let Some((h, _)) = hit_in(ln, &needle) { n += 1; if hit.is_none() { hit = Some(h); } }
            }
        } else {
            for ln in text_orig.split('\n') {
                if ln.is_empty() || !ln.to_lowercase().contains(&needle) { continue; }
                if let Some((h, _)) = hit_in(ln.as_bytes(), &needle) { n += 1; if hit.is_none() { hit = Some(h); } }
            }
        }
        let Some(hit) = hit else { continue };
        let s = summarize_cached(&f);
        out.push(json!({"id": f.id, "title": s.as_ref().map(|r| if r.title.is_empty() { "Untitled".to_string() } else { r.title.clone() }).unwrap_or_else(|| f.id.clone()), "cwd": s.map(|r| r.cwd).unwrap_or_default(), "mtime": f.mtime.floor(), "snippet": hit, "count": n}));
    }
    Value::Array(out)
}

pub fn summarize_cached(f: &FileInfo) -> Option<R> {
    let mut g = SUMS.lock().unwrap();
    let cache = g.get_or_insert_with(HashMap::new);
    let e = summarize(f, cache.get(&f.file).cloned())?;
    cache.insert(f.file.clone(), e.clone());
    Some(e.out)
}

/// The words around the first match of [needle] in one chat line, when it is a user or assistant message that really contains it.
fn hit_in(line: &[u8], needle: &str) -> Option<(String, ())> {
    let o = parse_val(line)?;
    let kind = o.get("type").and_then(|t| t.as_str())?;
    if kind != "user" && kind != "assistant" { return None; }
    let body = text_of(o.get("message").and_then(|m| m.get("content")));
    let lowered = body.to_lowercase();
    let i = utf16_find(&lowered, needle)?;
    let b16: Vec<u16> = body.encode_utf16().collect();
    let from = i.saturating_sub(40);
    let to = (i + needle.encode_utf16().count() + 60).min(b16.len());
    let piece = if from < to { String::from_utf16_lossy(&b16[from..to]) } else { String::new() };
    let mut collapsed = String::new();
    let mut ws = false;
    for c in piece.chars() { if is_js_ws(c) { if !ws { collapsed.push(' '); } ws = true; } else { collapsed.push(c); ws = false; } }
    Some((format!("{}{}", if i > 40 { "…" } else { "" }, collapsed), ()))
}
fn utf16_find(hay: &str, needle: &str) -> Option<usize> {
    let byte = hay.find(needle)?;
    Some(hay[..byte].encode_utf16().count())
}

// ───────── usage ─────────
/// One content block, with only what the totals need: everything else (the big tool inputs and results) is skipped without being built.
#[derive(Deserialize, Default)]
struct Block { #[serde(rename = "type", default)] kind: Option<Value>, #[serde(default)] text: Option<Value>, #[serde(default)] id: Option<Value>, #[serde(default)] name: Option<Value> }
enum Content { Text(String), Blocks(Vec<Block>), Other }
impl<'de> Deserialize<'de> for Content {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = Content;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { f.write_str("content") }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Content, E> { Ok(Content::Text(v.to_string())) }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Content, E> { Ok(Content::Text(v)) }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut a: A) -> Result<Content, A::Error> {
                let mut v = Vec::new();
                while let Some(b) = a.next_element::<Option<Block>>()? { v.push(b.unwrap_or_default()); }
                Ok(Content::Blocks(v))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut a: A) -> Result<Content, A::Error> { while a.next_entry::<serde::de::IgnoredAny, serde::de::IgnoredAny>()?.is_some() {} Ok(Content::Other) }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Content, E> { Ok(Content::Other) }
            fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<Content, E> { Ok(Content::Other) }
            fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<Content, E> { Ok(Content::Other) }
            fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<Content, E> { Ok(Content::Other) }
            fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<Content, E> { Ok(Content::Other) }
        }
        d.deserialize_any(V)
    }
}
impl Content {
    fn text(&self) -> String {
        match self {
            Content::Text(s) => s.clone(),
            Content::Blocks(b) => b.iter().filter(|b| b.kind.as_ref().and_then(|k| k.as_str()) == Some("text")).map(|b| match &b.text { Some(Value::String(s)) => s.clone(), Some(v) if truthy(&Some(v.clone())) => js_string(v), _ => String::new() }).collect::<Vec<_>>().join("\n"),
            Content::Other => String::new(),
        }
    }
}
#[derive(Deserialize)]
struct UMsg { #[serde(default)] id: Option<Value>, #[serde(default)] model: Option<Value>, #[serde(default)] usage: Option<Value>, #[serde(default)] content: Option<Content> }
#[derive(Deserialize)]
struct ULine {
    #[serde(rename = "type", default)] kind: Option<Value>, #[serde(rename = "isSidechain", default)] side: Option<Value>, #[serde(default)] cwd: Option<Value>,
    #[serde(default)] timestamp: Option<Value>, #[serde(default)] uuid: Option<Value>, #[serde(default)] message: Option<UMsg>,
}

fn jsonl_lines(file: &Path) -> Vec<u8> {
    let Ok(mut f) = File::open(file) else { return vec![] };
    let size = f.metadata().map(|m| m.len()).unwrap_or(0);
    let mut buf = Vec::new();
    if size > JSONL_TAIL {
        if f.seek(SeekFrom::Start(size - JSONL_TAIL)).is_err() { return vec![]; }
        if f.take(JSONL_TAIL).read_to_end(&mut buf).is_err() { return vec![]; }
        let nl = memchr::memchr(b'\n', &buf);
        buf.drain(..nl.map(|i| i + 1).unwrap_or(0));
    } else if f.read_to_end(&mut buf).is_err() { return vec![]; }
    buf
}

/// Per chat: the answers (once per message id) with their tokens, the times of your prompts, and the tools used. Like `parseUsage` of core.js.
pub fn usage(file: &Path) -> Value {
    let buf = jsonl_lines(file);
    let mut msgs: Vec<(Value, f64)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut users: Vec<f64> = Vec::new();
    let mut tools: Vec<String> = Vec::new();
    let mut tool_index: HashMap<String, usize> = HashMap::new();
    let mut cwd = String::new();
    for line in buf.split(|b| *b == b'\n').filter(|l| !l.is_empty()) {
        let owned;
        let o: ULine = match serde_json::from_slice(line) { Ok(o) => o, Err(_) => { owned = String::from_utf8_lossy(line).to_string(); match serde_json::from_str(&owned) { Ok(o) => o, Err(_) => continue } } };
        if cwd.is_empty() { if let Some(Value::String(c)) = &o.cwd { cwd = c.clone(); } }
        let kind = o.kind.as_ref().and_then(|t| t.as_str()).unwrap_or("");
        let ts = iso_ts(o.timestamp.as_ref());
        if kind == "assistant" {
            let Some(m) = &o.message else { continue };
            let model = m.model.as_ref().and_then(|x| x.as_str()).unwrap_or("");
            let Some(u) = m.usage.as_ref().filter(|u| truthy(&Some((*u).clone()))) else { continue };
            if model.starts_with('<') { continue; }
            let (i, out, cr, cw) = (num(u.get("input_tokens")), num(u.get("output_tokens")), num(u.get("cache_read_input_tokens")), num(u.get("cache_creation_input_tokens")));
            let rec = json!({"ts": ts, "model": model, "in": i, "out": out, "cr": cr, "cw": cw});
            let key = match m.id.as_ref().and_then(|x| x.as_str()).filter(|s| !s.is_empty()).or_else(|| o.uuid.as_ref().and_then(|x| x.as_str()).filter(|s| !s.is_empty())) { Some(k) => k.to_string(), None => msgs.len().to_string() };
            match index.get(&key) { Some(&p) => { if out >= msgs[p].1 { msgs[p] = (rec, out); } } None => { index.insert(key, msgs.len()); msgs.push((rec, out)); } }
            if let Some(Content::Blocks(a)) = &m.content {
                for b in a { if b.kind.as_ref().and_then(|t| t.as_str()) == Some("tool_use") {
                    let id = match b.id.as_ref().and_then(|x| x.as_str()).filter(|s| !s.is_empty()) { Some(i) => i.to_string(), None => tool_index.len().to_string() };
                    let name = b.name.as_ref().and_then(|x| x.as_str()).unwrap_or("").to_string();
                    match tool_index.get(&id) { Some(&p) => tools[p] = name, None => { tool_index.insert(id, tools.len()); tools.push(name); } }
                } }
            }
        } else if kind == "user" && !truthy(&o.side) && !trim(&o.message.as_ref().and_then(|m| m.content.as_ref()).map(|c| c.text()).unwrap_or_default()).is_empty() {
            users.push(ts);
        }
    }
    json!({"cwd": cwd, "msgs": msgs.into_iter().map(|(r, _)| r).collect::<Vec<_>>(), "users": users, "tools": tools})
}

// ───────── the rest of core.js: finding a chat, the list, statistics ─────────
static INDEX: Mutex<Option<HashMap<String, PathBuf>>> = Mutex::new(None);

fn info_of(id: &str, file: &Path) -> Option<FileInfo> {
    let md = std::fs::metadata(file).ok()?;
    Some(FileInfo { id: id.to_string(), file: file.to_path_buf(), mtime: crate::util::mtime_ms(&md), size: md.len() })
}
/// One chat by id. Remembers where chats are, so it does not list every chat file each time.
pub fn find_session(sid: &str) -> Option<FileInfo> {
    if sid.is_empty() || !sid.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') { return None; }
    {
        let mut g = INDEX.lock().unwrap();
        if let Some(f) = g.as_ref().and_then(|m| m.get(sid).cloned()) {
            if let Some(i) = info_of(sid, &f) { return Some(i); }
            if let Some(m) = g.as_mut() { m.remove(sid); }
        }
    }
    let mut next = HashMap::new();
    for f in list_files() { next.entry(f.id.clone()).or_insert(f.file.clone()); }
    let hit = next.get(sid).cloned();
    *INDEX.lock().unwrap() = Some(next);
    hit.and_then(|f| info_of(sid, &f))
}

/// What the list shows about a chat.
#[derive(Clone, Default)]
pub struct Summary { pub title: String, pub cwd: String, pub tokens: f64, pub cost: f64, pub model: String }
pub fn summary(f: &FileInfo) -> Summary {
    match summarize_cached(f) {
        Some(r) => Summary { title: if r.title.is_empty() { "Untitled".into() } else { r.title }, cwd: r.cwd, tokens: r.tok, cost: r.cost, model: r.model },
        None => Summary { title: "Untitled".into(), ..Default::default() },
    }
}
impl Summary {
    pub fn json(&self) -> Value { json!({"title": self.title, "cwd": self.cwd, "tokens": crate::util::num(self.tokens), "cost": self.cost, "model": self.model}) }
}

/// Claude Code programs open now, by working folder (Linux only).
fn open_claudes() -> HashMap<String, i32> {
    let mut counts = HashMap::new();
    if !cfg!(target_os = "linux") || std::env::var("FLATPAK_ID").is_ok() { return counts; }
    let Ok(rd) = std::fs::read_dir("/proc") else { return counts };
    for e in rd.flatten() {
        let n = e.file_name().to_string_lossy().to_string();
        if !n.chars().all(|c| c.is_ascii_digit()) { continue; }
        let p = e.path();
        if std::fs::read_to_string(p.join("comm")).map(|c| c.trim() != "claude").unwrap_or(true) { continue; }
        if std::fs::read(p.join("cmdline")).map(|c| memchr::memmem::find(&c, b"stream-json").is_some()).unwrap_or(true) { continue; }
        if let Ok(cwd) = std::fs::read_link(p.join("cwd")) { *counts.entry(cwd.to_string_lossy().to_string()).or_insert(0) += 1; }
    }
    counts
}

/// The chats, newest first. [keep] drops chats before the list is cut to [limit].
pub fn sessions(active: &std::collections::HashSet<String>, keep: Option<&dyn Fn(&FileInfo, &Summary) -> bool>, limit: usize, quiet: &std::collections::HashSet<String>) -> Vec<Value> {
    let mut files = list_files();
    files.sort_by(|a, b| b.mtime.partial_cmp(&a.mtime).unwrap_or(std::cmp::Ordering::Equal));
    let now = crate::util::now_ms();
    let mut open = open_claudes();
    let has_proc = cfg!(target_os = "linux") && std::env::var("FLATPAK_ID").is_err();
    let mut out = Vec::new();
    for f in files {
        let s = summary(&f);
        if let Some(k) = keep { if !k(&f, &s) { continue; } }
        let age = (now - f.mtime) / 1000.0;
        let mut live = active.contains(&f.id);
        if !live {
            if has_proc { if let Some(c) = open.get_mut(&s.cwd) { if *c > 0 { live = true; *c -= 1; } } } else { live = age < 120.0; }
        }
        out.push(json!({"id": f.id, "title": s.title, "cwd": s.cwd, "mtime": f.mtime.floor() as i64, "live": live,
            "busy": active.contains(&f.id) || (!quiet.contains(&f.id) && age < 20.0), "tokens": crate::util::num(s.tokens), "cost": s.cost, "model": s.model}));
        if out.len() >= limit { break; }
    }
    out
}

/// The plan of the newest prompt, from Claude's todo list (TodoWrite, or TaskCreate/TaskUpdate): [{text, status}] with status
/// pending, in_progress or completed. Empty when the newest prompt has no plan (a question, or plans are off).
pub fn plan(msgs: &Value) -> Value {
    let list = msgs.as_array().cloned().unwrap_or_default();
    let start = list.iter().rposition(|m| m.get("role").and_then(|r| r.as_str()) == Some("user")).map(|i| i + 1).unwrap_or(0);
    let mut items: Vec<(String, String, String)> = Vec::new();   // (id, text, status)
    let mut n = 0;
    for m in &list[start..] {
        if m.get("role").and_then(|r| r.as_str()) != Some("tool") { continue; }
        let input = m.get("input").cloned().unwrap_or(Value::Null);
        let s = |k: &str| input.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
        match m.get("name").and_then(|x| x.as_str()).unwrap_or("") {
            "TodoWrite" => {
                items = input.get("todos").and_then(|t| t.as_array()).map(|a| a.iter().enumerate().map(|(i, t)| {
                    let st = t.get("status").and_then(|x| x.as_str()).unwrap_or("pending").to_string();
                    let txt = if st == "in_progress" { t.get("activeForm").and_then(|x| x.as_str()).filter(|x| !x.is_empty()).or(t.get("content").and_then(|x| x.as_str())) } else { t.get("content").and_then(|x| x.as_str()) };
                    ((i + 1).to_string(), txt.unwrap_or("").to_string(), st)
                }).collect()).unwrap_or_default();
            }
            "TaskCreate" => {
                n += 1;
                let res = m.get("result").and_then(|x| x.as_str()).unwrap_or("");
                let id = res.split('#').nth(1).map(|t| t.chars().take_while(|c| c.is_ascii_digit()).collect::<String>()).filter(|t| !t.is_empty()).unwrap_or_else(|| n.to_string());
                items.push((id, s("subject"), "pending".into()));
            }
            "TaskUpdate" => {
                let id = s("taskId").trim_start_matches('#').to_string();
                if let Some(it) = items.iter_mut().find(|x| x.0 == id) {
                    let st = s("status"); if !st.is_empty() { it.2 = st; }
                    let sub = s("subject"); if !sub.is_empty() { it.1 = sub; }
                }
                items.retain(|x| x.2 != "deleted");
            }
            _ => {}
        }
    }
    json!({"items": items.into_iter().filter(|x| !x.1.trim().is_empty()).map(|(_, t, st)| json!({"text": t, "status": st})).collect::<Vec<_>>()})
}

pub fn messages_sid(sid: &str, tail: usize) -> Value { match find_session(sid) { Some(f) => messages(&f.file, tail), None => json!([]) } }

/// When the user's last prompt in a chat was, and Claude's last words (for "done" notices of terminal sessions).
pub fn turn_info(sid: &str) -> (f64, String) {
    let Some(f) = find_session(sid) else { return (0.0, String::new()) };
    let (mut started, mut last) = (0.0, String::new());
    for o in read_jsonl(&f.file) {
        if crate::util::truthy(o.get("isSidechain").unwrap_or(&Value::Null)) { continue; }
        let c = o.get("message").and_then(|m| m.get("content"));
        let k = o.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if k == "user" && !trim(&text_of(c)).is_empty() { started = iso_ts(o.get("timestamp")); }
        else if k == "assistant" && c.map(|x| x.is_array()).unwrap_or(false) && !trim(&text_of(c)).is_empty() { last = trim(&text_of(c)).to_string(); }
    }
    let secs = if started > 0.0 { (crate::util::now_ms() / 1000.0 - started).floor() } else { 0.0 };
    (secs, last)
}

// usage of every chat, kept until the chat changes
static STATS: Mutex<Option<HashMap<PathBuf, (f64, u64, Value)>>> = Mutex::new(None);
fn usage_cached(f: &FileInfo) -> Value {
    {
        let g = STATS.lock().unwrap();
        if let Some((m, s, v)) = g.as_ref().and_then(|m| m.get(&f.file)) { if *m == f.mtime && *s == f.size { return v.clone(); } }
    }
    let v = usage(&f.file);
    STATS.lock().unwrap().get_or_insert_with(HashMap::new).insert(f.file.clone(), (f.mtime, f.size, v.clone()));
    v
}
fn family(m: &str) -> &'static str { for f in ["opus", "sonnet", "haiku", "fable", "mythos"] { if m.contains(f) { return f; } } "other" }
fn local(ms: f64) -> chrono::DateTime<chrono::Local> { use chrono::TimeZone; chrono::Local.timestamp_millis_opt(ms as i64).single().unwrap_or_else(chrono::Local::now) }
fn day_key(ms: f64) -> String { local(ms).format("%Y-%m-%d").to_string() }

/// Per day, per model family and per project, for the dashboard. Like `stats` of core.js.
pub fn stats(days: i64) -> Value {
    use chrono::{Datelike, Timelike};
    let now = crate::util::now_ms();
    let cutoff = now - days as f64 * 864e5;
    struct Row { i: f64, o: f64, cr: f64, cw: f64, msgs: f64, fam: serde_json::Map<String, Value>, sess: std::collections::HashSet<String> }
    let mut per_day: HashMap<String, Row> = HashMap::new();
    let mut projects: Vec<(String, f64, std::collections::HashSet<String>, f64)> = Vec::new();
    let mut models = serde_json::Map::new();
    let mut tools: Vec<(String, f64)> = Vec::new();
    let mut heat = vec![vec![0i64; 24]; 7];
    let addf = |m: &mut serde_json::Map<String, Value>, k: &str, x: f64| { let v = m.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0) + x; m.insert(k.to_string(), crate::util::num(v)); };
    for f in list_files() {
        if f.mtime < cutoff { continue; }
        let d = usage_cached(&f);
        let pk = { let c = d.get("cwd").and_then(|x| x.as_str()).unwrap_or(""); if c.is_empty() { "unknown".to_string() } else { c.to_string() } };
        let pi = match projects.iter().position(|p| p.0 == pk) { Some(i) => i, None => { projects.push((pk.clone(), 0.0, Default::default(), 0.0)); projects.len() - 1 } };
        let msgs = d.get("msgs").and_then(|x| x.as_array()).cloned().unwrap_or_default();
        for m in &msgs {
            let ts = m.get("ts").and_then(|x| x.as_f64()).unwrap_or(0.0);
            if ts * 1000.0 < cutoff { continue; }
            let g = |k: &str| m.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0);
            let work = g("in") + g("out") + g("cw");
            let fam = family(m.get("model").and_then(|x| x.as_str()).unwrap_or(""));
            let r = per_day.entry(day_key(ts * 1000.0)).or_insert_with(|| Row { i: 0.0, o: 0.0, cr: 0.0, cw: 0.0, msgs: 0.0, fam: Default::default(), sess: Default::default() });
            r.i += g("in"); r.o += g("out"); r.cr += g("cr"); r.cw += g("cw"); addf(&mut r.fam, fam, work); r.sess.insert(f.id.clone());
            let p = &mut projects[pi]; p.1 += work; p.2.insert(f.id.clone()); p.3 = p.3.max(ts);
            addf(&mut models, fam, work);
        }
        for u in d.get("users").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
            let ts = u.as_f64().unwrap_or(0.0);
            if ts * 1000.0 >= cutoff {
                per_day.entry(day_key(ts * 1000.0)).or_insert_with(|| Row { i: 0.0, o: 0.0, cr: 0.0, cw: 0.0, msgs: 0.0, fam: Default::default(), sess: Default::default() }).msgs += 1.0;
                let dt = local(ts * 1000.0);
                heat[(dt.weekday().num_days_from_sunday() as usize + 6) % 7][dt.hour() as usize] += 1;
            }
        }
        if msgs.last().and_then(|m| m.get("ts")).and_then(|x| x.as_f64()).map(|t| t * 1000.0 >= cutoff).unwrap_or(false) {
            for t in d.get("tools").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
                let n = t.as_str().unwrap_or("").to_string();
                match tools.iter_mut().find(|x| x.0 == n) { Some(x) => x.1 += 1.0, None => tools.push((n, 1.0)) }
            }
        }
    }
    let mut out_days = Vec::new();
    for i in (0..days).rev() {
        let k = day_key(now - i as f64 * 864e5);
        let r = per_day.get(&k);
        out_days.push(match r {
            Some(r) => json!({"date": k, "in": crate::util::num(r.i), "out": crate::util::num(r.o), "cr": crate::util::num(r.cr), "cw": crate::util::num(r.cw), "msgs": crate::util::num(r.msgs), "sessions": r.sess.len(), "fam": r.fam}),
            None => json!({"date": k, "in": 0, "out": 0, "cr": 0, "cw": 0, "msgs": 0, "sessions": 0, "fam": {}}),
        });
    }
    projects.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let top: Vec<Value> = projects.iter().take(10).filter(|p| p.1 != 0.0).map(|p| json!({"cwd": p.0, "tokens": crate::util::num(p.1), "sessions": p.2.len(), "last": (p.3 * 1000.0).floor() as i64})).collect();
    tools.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let top_tools: serde_json::Map<String, Value> = tools.into_iter().take(10).map(|(k, v)| (k, crate::util::num(v))).collect();
    json!({"generated": now as i64, "days": out_days, "models": models, "tools": top_tools, "heat": heat, "projects": top})
}

fn price_of(model: &str) -> (f64, f64, f64, f64) { crate::pricing::price_of(model) }

/// Everything /usage-style: totals, cost at API prices, per model, per day, per project. days = 0 means all time. Like `report` of core.js.
pub fn report(days: i64, extra: &[FileInfo]) -> Value {
    use crate::util::num;
    let now = crate::util::now_ms();
    let cutoff = if days > 0 { now - days as f64 * 864e5 } else { 0.0 };
    struct M { model: String, label: String, i: f64, o: f64, cr: f64, cw: f64, msgs: f64, cost: f64 }
    let mut by_model: Vec<M> = Vec::new();
    let mut per_day: HashMap<String, (f64, f64)> = HashMap::new();
    let mut proj: Vec<(String, f64, f64, std::collections::HashSet<String>)> = Vec::new();
    let mut sessions = std::collections::HashSet::new();
    let mut active = std::collections::BTreeSet::new();
    let mut tools: Vec<(String, f64)> = Vec::new();
    let (mut first, mut last, mut prompts, mut longest, mut assistant) = (0.0f64, 0.0f64, 0.0, 0.0f64, 0.0);
    let (mut ti, mut to, mut tcr, mut tcw, mut tcost) = (0.0, 0.0, 0.0, 0.0, 0.0);
    let mut seen = std::collections::HashSet::new();
    let mut all = list_files(); all.extend(extra.iter().cloned());
    for f in all {
        if f.mtime < cutoff || seen.contains(&f.id) { continue; }
        seen.insert(f.id.clone());
        let d = usage_cached(&f);
        let pk = { let c = d.get("cwd").and_then(|x| x.as_str()).unwrap_or(""); if c.is_empty() { "unknown".to_string() } else { c.to_string() } };
        let (mut lo, mut hi) = (0.0f64, 0.0f64);
        let msgs = d.get("msgs").and_then(|x| x.as_array()).cloned().unwrap_or_default();
        for m in &msgs {
            let g = |k: &str| m.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0);
            let t = g("ts") * 1000.0; if t < cutoff { continue; }
            let model = m.get("model").and_then(|x| x.as_str()).unwrap_or("");
            let cost = crate::pricing::cost_of(model, &crate::pricing::Tokens { input: g("in"), out: g("out"), cr: g("cr"), cw: g("cw") });
            let k = if model.is_empty() { "unknown".to_string() } else { model.to_string() };
            let b = match by_model.iter_mut().position(|x| x.model == k) { Some(i) => &mut by_model[i], None => { by_model.push(M { label: crate::pricing::label_of(&k), model: k.clone(), i: 0.0, o: 0.0, cr: 0.0, cw: 0.0, msgs: 0.0, cost: 0.0 }); by_model.last_mut().unwrap() } };
            b.i += g("in"); b.o += g("out"); b.cr += g("cr"); b.cw += g("cw"); b.msgs += 1.0; b.cost += cost;
            ti += g("in"); to += g("out"); tcr += g("cr"); tcw += g("cw"); tcost += cost; assistant += 1.0;
            let dk = day_key(t); let r = per_day.entry(dk.clone()).or_insert((0.0, 0.0)); r.0 += g("in") + g("out") + g("cw"); r.1 += cost; active.insert(dk);
            let p = match proj.iter_mut().position(|x| x.0 == pk) { Some(i) => &mut proj[i], None => { proj.push((pk.clone(), 0.0, 0.0, Default::default())); proj.last_mut().unwrap() } };
            p.1 += g("in") + g("out") + g("cw"); p.2 += cost; p.3.insert(f.id.clone());
            sessions.insert(f.id.clone());
            first = if first == 0.0 || t < first { t } else { first }; last = last.max(t);
            lo = if lo == 0.0 || t < lo { t } else { lo }; hi = hi.max(t);
        }
        longest = longest.max(hi - lo);
        for u in d.get("users").and_then(|x| x.as_array()).cloned().unwrap_or_default() { if u.as_f64().unwrap_or(0.0) * 1000.0 >= cutoff { prompts += 1.0; } }
        if !msgs.is_empty() { for t in d.get("tools").and_then(|x| x.as_array()).cloned().unwrap_or_default() { let n = t.as_str().unwrap_or("").to_string(); match tools.iter_mut().find(|x| x.0 == n) { Some(x) => x.1 += 1.0, None => tools.push((n, 1.0)) } } }
    }
    // streaks of days with activity (noon of each day, so summer time never breaks a streak)
    let noon = |k: &str| -> f64 { chrono::NaiveDate::parse_from_str(k, "%Y-%m-%d").ok().and_then(|d| d.and_hms_opt(12, 0, 0)).and_then(|d| { use chrono::TimeZone; chrono::Local.from_local_datetime(&d).single() }).map(|d| d.timestamp_millis() as f64).unwrap_or(0.0) };
    let (mut best, mut run, mut prev) = (0, 0, None::<f64>);
    for k in &active { let t = noon(k); run = if prev.map(|p| ((t - p) / 864e5).round() == 1.0).unwrap_or(false) { run + 1 } else { 1 }; best = best.max(run); prev = Some(t); }
    let mut current = 0; let mut t = noon(&day_key(now)); while active.contains(&day_key(t)) { current += 1; t -= 864e5; }
    let span = if days > 0 { days.min(365) } else { 365 };
    let mut days_out = Vec::new();
    for i in (0..span).rev() { let k = day_key(now - i as f64 * 864e5); let r = per_day.get(&k).copied().unwrap_or((0.0, 0.0)); days_out.push(json!({"date": k, "tokens": num(r.0), "cost": r.1})); }
    by_model.sort_by(|a, b| b.cost.partial_cmp(&a.cost).unwrap_or(std::cmp::Ordering::Equal));
    let cache_saved: f64 = by_model.iter().map(|m| { let p = price_of(&m.model); m.cr * (p.0 - p.2) / 1e6 }).sum();
    let work = ti + to + tcw;
    let models: Vec<Value> = by_model.iter().map(|m| json!({"model": m.model, "label": m.label, "in": num(m.i), "out": num(m.o), "cr": num(m.cr), "cw": num(m.cw), "msgs": num(m.msgs), "cost": m.cost})).collect();
    proj.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    tools.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let favorite = by_model.first().map(|m| m.label.clone()).unwrap_or_default();
    json!({
        "range": days, "generated": now as i64,
        "totals": {"in": num(ti), "out": num(to), "cr": num(tcr), "cw": num(tcw), "cost": tcost, "tokens": num(work), "allTokens": num(work + tcr), "messages": num(assistant), "prompts": num(prompts), "sessions": sessions.len(), "activeDays": active.len(), "longestStreak": best, "currentStreak": current, "first": num(first), "last": num(last), "longestSessionMs": num(longest), "avgPerActiveDay": if active.is_empty() { 0.0 } else { (work / active.len() as f64).round() }, "cacheSaved": cache_saved},
        "models": models, "days": days_out, "favorite": favorite,
        "projects": proj.iter().take(12).map(|p| json!({"cwd": p.0, "tokens": num(p.1), "cost": p.2, "sessions": p.3.len()})).collect::<Vec<_>>(),
        "tools": tools.into_iter().take(8).map(|(k, v)| (k, num(v))).collect::<serde_json::Map<String, Value>>(),
        "note": "Cost is what these tokens would cost at Anthropic API prices. A Pro or Max subscription is not billed per token.",
    })
}
