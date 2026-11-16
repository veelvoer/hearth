//! Test harness: starts the real relay (the `hearth-relay-bin` binary) as its own process per test, with its own HOME, a fake `claude`
//! in PATH, and plain HTTP calls. Each test file is the port of one desktop/test/*.test.js file.
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub use regex;
pub use serde_json;

/// The relay binary: cargo builds it next to the test executables (target/<profile>/hearth-relay-bin).
pub fn relay_bin() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    for d in exe.ancestors().skip(1) { let b = d.join("hearth-relay-bin"); if b.is_file() { return b; } }
    PathBuf::from("hearth-relay-bin")
}

pub fn sleep(ms: u64) { std::thread::sleep(Duration::from_millis(ms)); }
pub fn now_ms() -> f64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as f64 }

/// A scratch folder under ~/hearth-tmp (never /tmp), removed when dropped. Holds bin/ home/ proj/ and d/ (the relay's folder).
pub struct Env { pub t: PathBuf, pub bin: PathBuf, pub home: PathBuf, pub proj: PathBuf, pub dir: PathBuf }
impl Env {
    pub fn new(prefix: &str) -> Env {
        let base = PathBuf::from(std::env::var("HOME").unwrap()).join("hearth-tmp");
        std::fs::create_dir_all(&base).unwrap();
        let t = base.join(format!("{prefix}-{}-{}", std::process::id(), now_ms() as u64 % 100000000));
        let _ = std::fs::remove_dir_all(&t);
        let e = Env { bin: t.join("bin"), home: t.join("home"), proj: t.join("proj"), dir: t.join("d"), t };
        for d in [&e.bin, &e.home, &e.proj] { std::fs::create_dir_all(d).unwrap(); }
        e
    }
    /// Write an executable script into the fake PATH folder.
    pub fn stub(&self, name: &str, script: &str) -> PathBuf {
        let p = self.bin.join(name);
        std::fs::write(&p, script).unwrap();
        #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap(); }
        p
    }
    pub fn path_var(&self) -> String { format!("{}:{}", self.bin.display(), std::env::var("PATH").unwrap_or_default()) }
    pub fn read(&self, rel: &str) -> String { std::fs::read_to_string(self.t.join(rel)).unwrap_or_default() }
    pub fn read_json(&self, rel: &str) -> Value { serde_json::from_str(&self.read(rel)).unwrap_or(Value::Null) }
    pub fn write(&self, rel: &str, s: &str) { let p = self.t.join(rel); if let Some(d) = p.parent() { std::fs::create_dir_all(d).unwrap(); } std::fs::write(p, s).unwrap(); }
    /// Lines of home/calls.txt (written by the standard claude stub) parsed as JSON.
    pub fn calls(&self) -> Vec<Value> {
        self.read("home/calls.txt").lines().filter(|l| !l.trim().is_empty()).filter_map(|l| serde_json::from_str(l).ok()).collect()
    }
}
impl Drop for Env { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.t); } }

pub fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

/// A running relay process. `kill()` stops it; dropping it also stops it.
pub struct Relay { pub port: u16, pub child: Option<Child>, pub token: String, pub dir: PathBuf, pub wd: bool }
pub struct Opts { pub port: u16, pub args: Vec<String>, pub env: Vec<(String, String)>, pub dir: Option<PathBuf>, pub home: Option<PathBuf>, pub watchdog: bool, pub projects: bool, pub log: Option<PathBuf> }
impl Opts {
    pub fn new() -> Opts { Opts { port: 0, args: vec![], env: vec![], dir: None, home: None, watchdog: false, projects: true, log: None } }
    pub fn args(mut self, a: &[&str]) -> Opts { self.args.extend(a.iter().map(|s| s.to_string())); self }
    pub fn env(mut self, k: &str, v: &str) -> Opts { self.env.push((k.into(), v.into())); self }
    pub fn port(mut self, p: u16) -> Opts { self.port = p; self }
    pub fn dir(mut self, p: &Path) -> Opts { self.dir = Some(p.to_path_buf()); self }
    pub fn home(mut self, p: &Path) -> Opts { self.home = Some(p.to_path_buf()); self }
    pub fn watchdog(mut self) -> Opts { self.watchdog = true; self }
    pub fn log(mut self, p: &Path) -> Opts { self.log = Some(p.to_path_buf()); self }
    pub fn no_projects(mut self) -> Opts { self.projects = false; self }
}

impl Relay {
    /// Start the relay in `env` (like `standalone.js --dir d --port P --host 127.0.0.1 --projects proj`) and wait until it listens.
    pub fn start(env: &Env, o: Opts) -> Relay { Relay::try_start(env, o).expect("the relay did not start") }
    pub fn try_start(env: &Env, o: Opts) -> Option<Relay> {
        let port = if o.port == 0 { free_port() } else { o.port };
        let dir = o.dir.clone().unwrap_or(env.dir.clone());
        let home = o.home.clone().unwrap_or(env.home.clone());
        std::fs::create_dir_all(&home).ok();
        let mut c = Command::new(relay_bin());
        c.arg("--dir").arg(&dir).args(["--port", &port.to_string(), "--host", "127.0.0.1"]);
        if o.projects { c.arg("--projects").arg(&env.proj); }
        if !o.watchdog { c.arg("--no-watchdog"); }
        c.args(&o.args);
        c.env("HOME", &home).env("PATH", env.path_var()).env("HEARTH_TEST", "1");
        for (k, v) in &o.env { c.env(k, v); }
        c.stdin(Stdio::null());
        if let Some(l) = &o.log { let f = std::fs::File::create(l).unwrap(); c.stdout(f.try_clone().unwrap()).stderr(f); } else { c.stdout(Stdio::null()).stderr(Stdio::null()); }
        let child = match c.spawn() { Ok(c) => c, Err(e) => { eprintln!("spawn {:?} failed: {e}", relay_bin()); return None; } };
        let mut r = Relay { port, child: Some(child), token: String::new(), dir, wd: o.watchdog };
        r.token = r.wait_token(Duration::from_secs(40))?;
        Some(r)
    }
    /// Waits until the relay has written its token file and answers on its port.
    fn wait_token(&self, limit: Duration) -> Option<String> {
        let t0 = Instant::now();
        let f = self.dir.join("relay.json");
        while t0.elapsed() < limit {
            if let Some(t) = std::fs::read_to_string(&f).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()).and_then(|j| j["token"].as_str().map(|s| s.to_string())).filter(|t| !t.is_empty()) {
                if std::net::TcpStream::connect_timeout(&format!("127.0.0.1:{}", self.port).parse().unwrap(), Duration::from_millis(200)).is_ok() { sleep(150); return Some(t); }
            }
            sleep(100);
        }
        None
    }
    pub fn own(&self) -> Vec<(String, String)> { vec![("Authorization".into(), format!("Bearer {}", self.token))] }
    pub fn api(&self) -> Api { Api { port: self.port } }
    pub fn pid(&self) -> u32 { self.child.as_ref().map(|c| c.id()).unwrap_or(0) }
    pub fn kill(&mut self) { if let Some(mut c) = self.child.take() { let _ = c.kill(); let _ = c.wait(); } }
    /// Ask politely (SIGTERM) and wait for it to exit.
    /// Returns true when it exited within 8 s.
    pub fn terminate(&mut self) -> bool {
        if let Some(mut c) = self.child.take() { unsafe { libc::kill(c.id() as i32, libc::SIGTERM); } for _ in 0..80 { if c.try_wait().ok().flatten().is_some() { return true; } sleep(100); } let _ = c.kill(); let _ = c.wait(); return false; }
        true
    }
    pub fn alive(&mut self) -> bool { self.child.as_mut().map(|c| c.try_wait().ok().flatten().is_none()).unwrap_or(false) }
}
impl Drop for Relay { fn drop(&mut self) { if self.wd { self.terminate(); } else { self.kill(); } } }

/// One answer from the relay: status code (0 when nothing answered), body parsed as JSON ({} when it is not), raw text.
pub struct R { pub code: u16, pub j: Value, pub text: String }
#[derive(Clone, Copy)]
pub struct Api { pub port: u16 }
pub type H<'a> = &'a [(&'a str, &'a str)];

impl Api {
    pub fn req(&self, method: &str, path: &str, hdr: H, body: Option<Value>) -> R {
        let c = reqwest::blocking::Client::builder().timeout(Duration::from_secs(60)).no_proxy().build().unwrap();
        let url = format!("http://127.0.0.1:{}{}", self.port, path);
        let m = reqwest::Method::from_bytes(method.as_bytes()).unwrap();
        let mut r = c.request(m, url);
        for (k, v) in hdr { r = r.header(*k, *v); }
        if let Some(b) = body { r = r.header("Content-Type", "application/json").body(serde_json::to_vec(&b).unwrap()); }
        match r.send() {
            Ok(rs) => { let code = rs.status().as_u16(); let text = rs.text().unwrap_or_default(); let j = serde_json::from_str(&text).unwrap_or(json!({})); R { code, j, text } }
            Err(_) => R { code: 0, j: json!({}), text: String::new() },
        }
    }
    pub fn get(&self, path: &str, hdr: H) -> R { self.req("GET", path, hdr, None) }
    pub fn post(&self, path: &str, hdr: H, body: Value) -> R { self.req("POST", path, hdr, Some(body)) }
    /// Raw bytes (for gzip and files): returns code, headers lowercased, body bytes.
    pub fn raw(&self, method: &str, path: &str, hdr: H, body: Option<Vec<u8>>) -> (u16, Vec<(String, String)>, Vec<u8>) {
        let c = reqwest::blocking::Client::builder().timeout(Duration::from_secs(60)).no_proxy().build().unwrap();
        let mut r = c.request(reqwest::Method::from_bytes(method.as_bytes()).unwrap(), format!("http://127.0.0.1:{}{}", self.port, path));
        for (k, v) in hdr { r = r.header(*k, *v); }
        if let Some(b) = body { r = r.body(b); }
        match r.send() {
            Ok(rs) => { let code = rs.status().as_u16(); let h = rs.headers().iter().map(|(k, v)| (k.as_str().to_lowercase(), v.to_str().unwrap_or("").to_string())).collect(); (code, h, rs.bytes().map(|b| b.to_vec()).unwrap_or_default()) }
            Err(_) => (0, vec![], vec![]),
        }
    }
}

/// A request whose answer is read while it arrives (event streams, runs). Stops when the answer ends or the relay is gone.
pub struct Stream { buf: std::sync::Arc<std::sync::Mutex<Vec<u8>>>, code: std::sync::Arc<std::sync::atomic::AtomicU16>, done: std::sync::Arc<std::sync::atomic::AtomicBool> }
impl Stream {
    pub fn text(&self) -> String { String::from_utf8_lossy(&self.buf.lock().unwrap()).to_string() }
    pub fn code(&self) -> u16 { self.code.load(std::sync::atomic::Ordering::SeqCst) }
    pub fn finished(&self) -> bool { self.done.load(std::sync::atomic::Ordering::SeqCst) }
    /// Wait for the end of the answer (up to 60 s); returns the text.
    pub fn wait(&self) -> String { wait_for(60000, || self.finished()); self.text() }
}
impl Api {
    pub fn stream(&self, method: &str, path: &str, hdr: &[(&str, &str)], body: Option<Value>) -> Stream {
        use std::io::Read; use std::sync::{atomic::Ordering, Arc, Mutex};
        let st = Stream { buf: Arc::new(Mutex::new(vec![])), code: Arc::new(Default::default()), done: Arc::new(Default::default()) };
        let (buf, code, done) = (st.buf.clone(), st.code.clone(), st.done.clone());
        let url = format!("http://127.0.0.1:{}{}", self.port, path);
        let (method, hdr): (String, Vec<(String, String)>) = (method.to_string(), hdr.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect());
        std::thread::spawn(move || {
            let c = reqwest::blocking::Client::builder().no_proxy().timeout(None::<Duration>).build().unwrap();
            let mut r = c.request(reqwest::Method::from_bytes(method.as_bytes()).unwrap(), url);
            for (k, v) in &hdr { r = r.header(k.as_str(), v.as_str()); }
            if let Some(b) = body { r = r.header("Content-Type", "application/json").body(serde_json::to_vec(&b).unwrap()); }
            if let Ok(mut rs) = r.send() {
                code.store(rs.status().as_u16(), Ordering::SeqCst);
                let mut chunk = [0u8; 4096];
                while let Ok(n) = rs.read(&mut chunk) { if n == 0 { break; } buf.lock().unwrap().extend_from_slice(&chunk[..n]); }
            }
            done.store(true, Ordering::SeqCst);
        });
        st
    }
}
/// The events of an /events or run stream (blocks that start with "id: ", their "data: " line as JSON).
pub fn sse_events(b: &str) -> Vec<Value> {
    b.split("\n\n").filter(|x| x.starts_with("id: ")).filter_map(|x| x.lines().find(|l| l.starts_with("data: "))).filter_map(|l| serde_json::from_str(&l[6..]).ok()).collect()
}

impl Api {
    /// Like the JS `req(method, url, tok, body)`: `tok` empty means no Authorization header.
    pub fn t(&self, method: &str, path: &str, tok: &str, body: Option<Value>) -> R {
        if tok.is_empty() { self.req(method, path, &[], body) } else { let b = format!("Bearer {tok}"); self.req(method, path, &[("Authorization", b.as_str())], body) }
    }
    pub fn tstream(&self, method: &str, path: &str, tok: &str, body: Option<Value>) -> Stream {
        let b = format!("Bearer {tok}"); self.stream(method, path, &[("Authorization", b.as_str())], body)
    }
}
/// Does the regex match? (the JS `/re/.test(s)`)
pub fn re(p: &str, s: &str) -> bool { regex::Regex::new(p).unwrap().is_match(s) }
pub fn jstr(v: &Value) -> String { v.as_str().unwrap_or("").to_string() }

/// Bearer header for the owner token.
pub fn bearer(tok: &str) -> Vec<(String, String)> { vec![("Authorization".into(), format!("Bearer {tok}"))] }

/// Collects PASS/FAIL lines like the JS `ok(name, cond, extra)`; `finish()` prints "N passed, M failed" and fails the test when any failed.
#[derive(Default)]
pub struct Checks { pub pass: usize, pub fail: usize, pub skipped: usize, pub failed: Vec<String> }
impl Checks {
    pub fn new() -> Checks { Checks::default() }
    pub fn ok(&mut self, name: &str, c: bool) { self.ok_x(name, c, "") }
    pub fn ok_x(&mut self, name: &str, c: bool, extra: impl AsRef<str>) {
        let x = extra.as_ref();
        if c { self.pass += 1 } else { self.fail += 1; self.failed.push(name.to_string()) }
        println!("{} {}{}", if c { "PASS" } else { "FAIL" }, name, if x.is_empty() { String::new() } else { format!("  {x}") });
    }
    pub fn finish(self) {
        println!("\n{} passed, {} failed", self.pass, self.fail);
        assert!(self.fail == 0, "failed: {:?}", self.failed);
    }
}

/// Poll `f` until it is true or `ms` pass.
pub fn wait_for(ms: u64, mut f: impl FnMut() -> bool) -> bool {
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_millis(ms) { if f() { return true; } sleep(100); }
    f()
}

/// The standard fake `claude` of the JS tests (apikeys, workflow...): records every call to home/calls.txt and answers with a result.
/// Python 3 stands in for node. `body` is Python code run after the call has been recorded with `t` (stdin text) in scope.
pub fn py_stub(body: &str) -> String {
    format!("#!/usr/bin/env python3\nimport sys, os, json, time\nt = sys.stdin.read()\nargs = sys.argv[1:]\ndef rec(extra=None):\n    d = {{'args': args, 'cwd': os.getcwd(), 'in': t}}\n    if extra: d.update(extra)\n    open(os.environ['HOME'] + '/calls.txt', 'a').write(json.dumps(d) + '\\n')\ndef out(o):\n    sys.stdout.write(json.dumps(o) + '\\n'); sys.stdout.flush()\n{body}\n")
}
pub fn arg_after(args: &Value, flag: &str) -> Option<String> {
    let a = args.as_array()?; let i = a.iter().position(|x| x == flag)?; a.get(i + 1).and_then(|v| v.as_str()).map(|s| s.to_string())
}
pub fn has_arg(args: &Value, flag: &str) -> bool { args.as_array().map(|a| a.iter().any(|x| x == flag)).unwrap_or(false) }

/// Local day-of-month and "HH:MM" for a time in ms (for the routines "daily at" test).
pub fn local_day_hm(ms: f64) -> (i32, String) {
    unsafe {
        let t = (ms / 1000.0) as libc::time_t; let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        (tm.tm_mday, format!("{:02}:{:02}", tm.tm_hour, tm.tm_min))
    }
}

/// One request seen by a [`Mock`] server.
#[derive(Clone, Debug)]
pub struct Seen { pub method: String, pub url: String, pub auth: String, pub body: String }

/// A tiny HTTP server for tests (a fake Google, a fake support desk). `handler` answers with a JSON text; every request is recorded.
pub struct Mock { pub port: u16, pub seen: std::sync::Arc<std::sync::Mutex<Vec<Seen>>>, stop: std::sync::Arc<std::sync::atomic::AtomicBool> }
impl Mock {
    pub fn start(port: u16, handler: impl Fn(&Seen) -> String + Send + Sync + 'static) -> Mock {
        use std::io::{Read, Write};
        let l = std::net::TcpListener::bind(("127.0.0.1", port)).expect("mock port");
        let port = l.local_addr().unwrap().port();
        l.set_nonblocking(true).unwrap();
        let seen: std::sync::Arc<std::sync::Mutex<Vec<Seen>>> = Default::default();
        let stop: std::sync::Arc<std::sync::atomic::AtomicBool> = Default::default();
        let (s2, st2, h) = (seen.clone(), stop.clone(), std::sync::Arc::new(handler));
        std::thread::spawn(move || {
            while !st2.load(std::sync::atomic::Ordering::SeqCst) {
                match l.accept() {
                    Ok((mut c, _)) => {
                        let (s3, h) = (s2.clone(), h.clone());
                        std::thread::spawn(move || {
                            c.set_nonblocking(false).ok(); c.set_read_timeout(Some(Duration::from_secs(5))).ok();
                            let mut buf = Vec::new(); let mut chunk = [0u8; 4096];
                            let (head_end, clen) = loop {
                                let n = match c.read(&mut chunk) { Ok(n) if n > 0 => n, _ => return };
                                buf.extend_from_slice(&chunk[..n]);
                                if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                                    let head = String::from_utf8_lossy(&buf[..i]).to_lowercase();
                                    let cl = head.lines().find_map(|l| l.strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0))).unwrap_or(0);
                                    break (i + 4, cl);
                                }
                            };
                            while buf.len() < head_end + clen { let n = match c.read(&mut chunk) { Ok(n) if n > 0 => n, _ => break }; buf.extend_from_slice(&chunk[..n]); }
                            let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
                            let mut first = head.lines().next().unwrap_or("").split(' ');
                            let (method, url) = (first.next().unwrap_or("").to_string(), first.next().unwrap_or("").to_string());
                            let auth = head.lines().find_map(|l| { let (k, v) = l.split_once(':')?; if k.eq_ignore_ascii_case("authorization") { Some(v.trim().to_string()) } else { None } }).unwrap_or_default();
                            let body = String::from_utf8_lossy(&buf[head_end..(head_end + clen).min(buf.len())]).to_string();
                            let seen1 = Seen { method, url, auth, body };
                            s3.lock().unwrap().push(seen1.clone());
                            let out = h(&seen1);
                            let _ = c.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", out.len(), out).as_bytes());
                        });
                    }
                    Err(_) => sleep(20),
                }
            }
        });
        Mock { port, seen, stop }
    }
    pub fn seen(&self) -> Vec<Seen> { self.seen.lock().unwrap().clone() }
}
impl Drop for Mock { fn drop(&mut self) { self.stop.store(true, std::sync::atomic::Ordering::SeqCst); } }

/// JavaScript truthiness of a JSON value.
pub fn truthy(v: &Value) -> bool {
    match v { Value::Null => false, Value::Bool(b) => *b, Value::Number(n) => n.as_f64().map(|x| x != 0.0).unwrap_or(true), Value::String(s) => !s.is_empty(), _ => true }
}

/// RFC 3339 UTC time with milliseconds for an epoch time in ms (like JS `new Date(ms).toISOString()`).
pub fn iso_ms(ms: f64) -> String {
    let secs = (ms / 1000.0).floor() as i64; let milli = (ms - secs as f64 * 1000.0) as i64;
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    let z = days + 719468; let era = z.div_euclid(146097); let doe = z.rem_euclid(146097); let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400; let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); let mp = (5 * doy + 2) / 153; let d = doy - (153 * mp + 2) / 5 + 1; let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z", if m <= 2 { y + 1 } else { y }, m, d, rem / 3600, rem % 3600 / 60, rem % 60, milli)
}
/// Set a file's modified time to `age_sec` seconds ago.
pub fn age_file(f: &Path, age_sec: f64) {
    let t = std::time::SystemTime::now() - Duration::from_millis((age_sec * 1000.0) as u64);
    std::fs::OpenOptions::new().write(true).open(f).and_then(|h| h.set_modified(t)).unwrap();
}
/// Claude's folder name for a working folder: every character that is not a letter or digit becomes a dash.
pub fn enc_path(p: &str) -> String { p.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect() }
