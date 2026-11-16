//! Small things every part of the relay uses: reading JSON values the forgiving JavaScript way, ids, time, files, hashes.
use serde_json::{Map, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub fn now_ms() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64() * 1000.0).unwrap_or(0.0)
}
pub fn now_i() -> i64 { now_ms() as i64 }

pub fn rand_bytes(n: usize) -> Vec<u8> {
    let mut b = vec![0u8; n];
    let _ = getrandom::fill(&mut b);
    b
}
/// `crypto.randomBytes(n).toString('hex')`
pub fn rand_hex(n: usize) -> String { hex::encode(rand_bytes(n)) }
/// `crypto.randomBytes(n).toString('base64url')`
pub fn rand_b64url(n: usize) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(rand_bytes(n))
}
/// `crypto.randomInt(0, max)`
pub fn rand_below(max: u64) -> u64 {
    let b = rand_bytes(8);
    let v = u64::from_le_bytes(b.try_into().unwrap_or([0; 8]));
    v % max
}
pub fn uuid4() -> String { uuid::Uuid::new_v4().to_string() }

pub fn sha256_hex(s: &[u8]) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(s))
}
pub fn sha1_hex(s: &[u8]) -> String {
    use sha1::Digest;
    hex::encode(sha1::Sha1::digest(s))
}

pub fn gzip(b: &[u8], level: u32) -> Vec<u8> {
    let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::new(level));
    let _ = e.write_all(b);
    e.finish().unwrap_or_default()
}
pub fn gunzip(b: &[u8]) -> Option<Vec<u8>> {
    let mut d = flate2::read::GzDecoder::new(b);
    let mut out = Vec::new();
    d.read_to_end(&mut out).ok()?;
    Some(out)
}

pub fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").ok().filter(|h| !h.is_empty()).or_else(|| std::env::var("USERPROFILE").ok()).unwrap_or_default())
}
pub fn hostname() -> String {
    if let Ok(h) = std::fs::read_to_string("/proc/sys/kernel/hostname") { let h = h.trim(); if !h.is_empty() { return h.to_string(); } }
    if let Ok(h) = std::env::var("COMPUTERNAME") { return h; }
    if let Ok(o) = std::process::Command::new("hostname").output() { let h = String::from_utf8_lossy(&o.stdout).trim().to_string(); if !h.is_empty() { return h; } }
    "computer".into()
}
pub fn username() -> String {
    std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_else(|_| "user".into())
}

pub fn read_json(p: &Path) -> Option<Value> { std::fs::read(p).ok().and_then(|b| serde_json::from_slice(&b).ok()) }
/// Writes a file readable only by this user (mode 600), like `fs.writeFileSync(..., { mode: 0o600 })`.
pub fn write_private(p: &Path, data: &[u8]) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(p)?;
        f.write_all(data)
    }
    #[cfg(not(unix))]
    { std::fs::write(p, data) }
}
pub fn write_json_private(p: &Path, v: &Value) { let _ = write_private(p, v.to_string().as_bytes()); }
/// Write to a temporary file next to it, then rename: a reader never sees half a file.
pub fn write_atomic_private(p: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(d) = p.parent() { let _ = std::fs::create_dir_all(d); }
    let tmp = PathBuf::from(format!("{}.tmp", p.display()));
    write_private(&tmp, data)?;
    std::fs::rename(&tmp, p)
}
pub fn is_dir(p: &Path) -> bool { std::fs::metadata(p).map(|m| m.is_dir()).unwrap_or(false) }
pub fn mtime_ms(m: &std::fs::Metadata) -> f64 {
    m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs_f64() * 1000.0).unwrap_or(0.0)
}
pub fn set_mtime(p: &Path, ms: f64) {
    let t = std::time::UNIX_EPOCH + std::time::Duration::from_millis(ms.max(0.0) as u64);
    if let Ok(f) = std::fs::File::options().write(true).open(p) { let _ = f.set_times(std::fs::FileTimes::new().set_accessed(t).set_modified(t)); }
}
/// `path.resolve(p)` (no file system access, removes `.` and `..`).
pub fn resolve(p: &str) -> PathBuf {
    let base = if Path::new(p).is_absolute() { PathBuf::new() } else { std::env::current_dir().unwrap_or_default() };
    let mut out = PathBuf::new();
    for c in base.join(p).components() {
        match c {
            std::path::Component::ParentDir => { out.pop(); }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}
/// `under(p, base)` of server.js: p (resolved) is base or inside it.
pub fn under(p: &str, base: &str) -> bool {
    let r = resolve(p).to_string_lossy().to_string();
    let b = base.trim_end_matches('/');
    r == b || r.starts_with(&format!("{}{}", b, std::path::MAIN_SEPARATOR))
}
pub fn realpath(p: &str) -> Option<String> { std::fs::canonicalize(p).ok().map(|x| x.to_string_lossy().to_string()) }
pub fn basename(p: &str) -> String { Path::new(p).file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default() }
pub fn dirname(p: &str) -> String { Path::new(p).parent().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| "/".into()) }
/// `path.relative(from, to)` for two absolute paths.
pub fn relative(from: &str, to: &str) -> String {
    let f: Vec<_> = resolve(from).components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
    let t: Vec<_> = resolve(to).components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
    let mut i = 0;
    while i < f.len() && i < t.len() && f[i] == t[i] { i += 1; }
    let mut parts: Vec<String> = std::iter::repeat("..".to_string()).take(f.len() - i).collect();
    parts.extend(t[i..].iter().cloned());
    parts.join("/")
}

// ───────── reading JSON values like JavaScript does ─────────
pub trait J {
    fn g(&self, k: &str) -> &Value;
    /// `String(v[k] || '')` for strings and numbers; "" for the rest.
    fn s(&self, k: &str) -> String;
    fn n(&self, k: &str) -> f64;
    /// truthiness of v[k]
    fn t(&self, k: &str) -> bool;
}
static NULL: Value = Value::Null;
impl J for Value {
    fn g(&self, k: &str) -> &Value { self.get(k).unwrap_or(&NULL) }
    fn s(&self, k: &str) -> String { vstr(self.g(k)) }
    fn n(&self, k: &str) -> f64 { vnum(self.g(k)) }
    fn t(&self, k: &str) -> bool { truthy(self.g(k)) }
}
pub fn vstr(v: &Value) -> String {
    match v { Value::String(s) => s.clone(), Value::Number(_) | Value::Bool(true) => crate::jsu::js_string(v), _ => String::new() }
}
/// `Number(v) || 0`
pub fn vnum(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => s.trim().parse::<f64>().ok().filter(|x| x.is_finite()).unwrap_or(0.0),
        Value::Bool(b) => if *b { 1.0 } else { 0.0 },
        _ => 0.0,
    }
}
/// `Number(v)` with NaN as None
pub fn vnum_opt(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => { let t = s.trim(); if t.is_empty() { Some(0.0) } else { t.parse::<f64>().ok() } }
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        Value::Null => Some(0.0),
        _ => None,
    }
}
pub fn truthy(v: &Value) -> bool {
    match v { Value::Null => false, Value::Bool(b) => *b, Value::Number(n) => n.as_f64().map(|x| x != 0.0 && !x.is_nan()).unwrap_or(true), Value::String(s) => !s.is_empty(), _ => true }
}
pub fn obj() -> Value { Value::Object(Map::new()) }
/// `{ ...a, ...b }`
pub fn merge(a: &Value, b: &Value) -> Value {
    let mut m = a.as_object().cloned().unwrap_or_default();
    if let Some(o) = b.as_object() { for (k, v) in o { m.insert(k.clone(), v.clone()); } }
    Value::Object(m)
}
pub fn set(v: &mut Value, k: &str, x: Value) { if !v.is_object() { *v = obj(); } if let Some(m) = v.as_object_mut() { m.insert(k.to_string(), x); } }
/// A number for JSON: whole numbers without ".0".
pub fn num(x: f64) -> Value {
    if x.fract() == 0.0 && x.abs() < 9.0e15 { Value::from(x as i64) } else { serde_json::Number::from_f64(x).map(Value::Number).unwrap_or(Value::Null) }
}
/// String slice by characters (JS `.slice(0, n)` for text that is mostly not astral).
pub fn clip(s: &str, n: usize) -> String { crate::jsu::slice16(s, 0, n) }
pub fn arr(v: &Value) -> Vec<Value> { v.as_array().cloned().unwrap_or_default() }

/// A tiny query-string reader.
pub fn query(q: &str) -> Vec<(String, String)> {
    url::form_urlencoded::parse(q.as_bytes()).map(|(a, b)| (a.to_string(), b.to_string())).collect()
}
pub fn qget(q: &[(String, String)], k: &str) -> Option<String> { q.iter().find(|(a, _)| a == k).map(|(_, b)| b.clone()) }
pub fn qs(q: &[(String, String)], k: &str) -> String { qget(q, k).unwrap_or_default() }
pub fn enc(s: &str) -> String { url::form_urlencoded::byte_serialize(s.as_bytes()).collect() }
pub fn qbuild(pairs: &[(&str, String)]) -> String {
    let mut s = url::form_urlencoded::Serializer::new(String::new());
    for (k, v) in pairs { s.append_pair(k, v); }
    s.finish()
}

/// `toLocaleTimeString([], {hour, minute})`-ish: 24-hour local time.
pub fn local_hm(ms: f64) -> String {
    use chrono::TimeZone;
    chrono::Local.timestamp_millis_opt(ms as i64).single().map(|d| d.format("%H:%M").to_string()).unwrap_or_default()
}
pub fn local_ymd(ms: f64) -> String {
    use chrono::TimeZone;
    chrono::Local.timestamp_millis_opt(ms as i64).single().map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default()
}
pub fn iso(ms: f64) -> String {
    use chrono::TimeZone;
    chrono::Utc.timestamp_millis_opt(ms as i64).single().map(|d| d.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()).unwrap_or_default()
}
