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
pub fn list_files() -> Vec<FileInfo> {
    let mut out = Vec::new();
    let Ok(dirs) = std::fs::read_dir(projects_dir()) else { return out };
    for d in dirs.flatten() {
        let name = d.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || !d.path().is_dir() { continue; }
        let Ok(files) = std::fs::read_dir(d.path()) else { continue };
        for f in files.flatten() {
            let n = f.file_name().to_string_lossy().to_string();
            if !n.ends_with(".jsonl") { continue; }
            let Ok(md) = f.metadata() else { continue };
            if File::open(f.path()).is_err() { continue; }
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
fn read_jsonl(file: &Path) -> Vec<Value> {
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
struct R { title: String, cwd: String, tok: f64, cost: f64, model: String, last_id: String, last_w: f64, last_c: f64 }

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
fn tool_input(name: &str, arg: Option<&Value>) -> Value {
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

fn result_text(c: Option<&Value>) -> String {
    match c {
        Some(Value::Array(a)) => slice16(&a.iter().map(|x| match x.get("text") { Some(Value::String(s)) => s.clone(), _ => String::new() }).collect::<Vec<_>>().join("\n"), 0, 2000),
        other => clip(other, 2000),
    }
}
fn tool_line(b: &Value) -> String {
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

fn summarize_cached(f: &FileInfo) -> Option<R> {
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
