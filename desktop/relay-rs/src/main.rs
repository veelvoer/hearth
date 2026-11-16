//! The Hearth relay. Lists Claude Code chats, runs them, and tells your phone and computer when Claude finishes or needs you.
//!   hearth-relay --projects ~/projects --port 47601 --url https://hearth.example.com      (a server: HTTPS proxy in front, token only)
//!   hearth-relay --lan --host 0.0.0.0 ...                                                  (a computer on your network: pairing by code)
//!   hearth-relay --link          print the pairing link and exit
//!   hearth-relay mcp-ask         the small MCP server that gives Claude the ask_user tool (started by Claude itself)
mod agent;
mod apikeys;
mod boot;
mod chathub;
mod core;
mod filehub;
mod hooks;
mod jobs;
mod jsu;
mod mcpkit;
mod perms;
mod post;
mod pricing;
mod relay;
mod routes;
mod safety;
mod selfupdate;
mod service;
mod shares;
mod tools;
mod util;
mod weekplan;
mod mcpask;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

fn flag(args: &[String], k: &str) -> Option<String> { args.iter().position(|a| a == k).and_then(|i| args.get(i + 1).cloned()).filter(|v| !v.starts_with("--")) }

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    #[cfg(unix)]
    unsafe {   // the watchdog blocks stop signals for itself; its child must get them again
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set); libc::sigaddset(&mut set, libc::SIGTERM); libc::sigaddset(&mut set, libc::SIGINT); libc::sigaddset(&mut set, libc::SIGHUP);
        libc::pthread_sigmask(libc::SIG_UNBLOCK, &set, std::ptr::null_mut());
    }
    if args.iter().any(|a| a == "--version") { println!("{VERSION}"); return; }
    if args.first().map(|a| a == "mcp-ask").unwrap_or(false) { mcpask::run(); return; }
    // This program watches; the server itself runs in a second process. When that one stops answering, it is stopped and started again.
    let plain = args.iter().any(|a| a == "--link" || a == "--core-only" || a == "--no-memory-limit" || a == "--no-watchdog");
    if !plain && std::env::var("HEARTH_MEM_CHILD").map(|v| v.is_empty()).unwrap_or(true) {
        let host = flag(&args, "--host").unwrap_or_else(|| "127.0.0.1".into());
        let port: u16 = flag(&args, "--port").and_then(|p| p.parse().ok()).unwrap_or(47601);
        watchdog(&host, port); return;
    }
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("runtime");
    rt.block_on(run(args));
}

fn default_dir() -> String { util::home().join(".config").join("hearth").to_string_lossy().to_string() }

async fn run(args: Vec<String>) {
    let get = |k: &str, d: &str| flag(&args, &format!("--{k}")).unwrap_or_else(|| d.to_string());
    let dir = get("dir", &default_dir());
    let url = get("url", "");
    let name = flag(&args, "--name").unwrap_or_else(util::hostname);
    if args.iter().any(|a| a == "--link") {
        match util::read_json(&std::path::Path::new(&dir).join("relay.json")).map(|c| util::J::s(&c, "token")).filter(|t| !t.is_empty()) {
            Some(token) => { if url.is_empty() { println!("{token}"); } else { println!("hearth://pair?{}", util::qbuild(&[("url", url.clone()), ("token", token), ("name", name.clone())])); } std::process::exit(0); }
            None => { eprintln!("The relay has not been started yet."); std::process::exit(1); }
        }
    }
    // for tests: only the private reads, on a port and with a secret given on the command line
    if args.iter().any(|a| a == "--core-only") {
        let port: u16 = get("port", "0").parse().unwrap_or(0);
        let l = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}")).await.expect("port");
        println!("listening {}", l.local_addr().map(|a| a.port()).unwrap_or(0));
        let _ = std::io::Write::flush(&mut std::io::stdout());
        let _ = axum::serve(l, service::router(get("secret", ""))).await;
        return;
    }
    let host = get("host", "127.0.0.1");
    let port: u16 = get("port", "47601").parse().unwrap_or(47601);
    let lan = args.iter().any(|a| a == "--lan");
    let o = relay::Opts {
        dir: dir.clone(), port, host: host.clone(), name: Some(name.clone()), public_mode: !lan,
        projects_root: if lan && !args.iter().any(|a| a == "--projects") { None } else { Some(get("projects", &util::home().join("projects").to_string_lossy())) },
        allow_bypass: args.iter().any(|a| a == "--allow-bypass"), allow_remote: false, self_update: true, relay_dir: String::new(), move_root: get("move-root", ""),
        seed_token: flag(&args, "--seed-token-file").and_then(|f| std::fs::read_to_string(f).ok()).unwrap_or_default(),
        voice: flag(&args, "--voice-port").and_then(|p| p.parse().ok()).map(|p| (p, get("voice-token-file", ""))),
    };
    let r = match boot::make(o) { Ok(r) => r, Err(e) => { eprintln!("Could not start: {e}"); std::process::exit(1); } };
    let bind = if host.contains(':') && !host.starts_with('[') { format!("[{host}]:{port}") } else { format!("{host}:{port}") };
    let listener = match tokio::net::TcpListener::bind(&bind).await { Ok(l) => l, Err(e) => { eprintln!("Could not start: {e}"); std::process::exit(1); } };
    println!("Hearth relay listening on {host}:{port}");
    println!("Projects folder: {}", r.root.clone().unwrap_or_else(|| "null".into()));
    if !url.is_empty() && is_tty() { println!("Pairing link (keep it secret): hearth://pair?{}", util::qbuild(&[("url", url), ("token", r.token.clone()), ("name", name)])); }   // never into service logs
    boot::timers(&r);
    if lan { boot::discovery(port); }
    boot::watch_link(r.clone(), lan);
    tokio::select! {
        _ = boot::serve(r.clone(), listener) => {}
        _ = tokio::signal::ctrl_c() => {}
        _ = terminated() => {}
    }
    for j in r.jobs.lock().unwrap().values() { if !j.is_done() { jobs::kill_tree(*j.pid.lock().unwrap(), false); } }
}

fn is_tty() -> bool {
    #[cfg(unix)] { unsafe { libc::isatty(1) == 1 } }
    #[cfg(not(unix))] { false }
}
#[cfg(unix)]
async fn terminated() { if let Ok(mut t) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) { t.recv().await; } else { std::future::pending::<()>().await } }
#[cfg(not(unix))]
async fn terminated() { std::future::pending::<()>().await }

static STOPPING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The watchdog: starts the real server as a child and starts it again when it stops answering /health.
fn watchdog(host: &str, port: u16) {
    let env_n = |k: &str, d: u64| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
    let (every, timeout, fails_max, grace) = (env_n("HEARTH_WD_INTERVAL_MS", 20000), env_n("HEARTH_WD_TIMEOUT_MS", 10000), env_n("HEARTH_WD_FAILS", 3), env_n("HEARTH_WD_GRACE_MS", 90000));
    let h = if host == "0.0.0.0" || host == "::" { "127.0.0.1".to_string() } else { host.to_string() };
    let exe = std::env::current_exe().unwrap_or_default();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let child: std::sync::Arc<std::sync::Mutex<Option<std::process::Child>>> = Default::default();
    let launch = { let child = child.clone(); let exe = exe.clone(); let args = args.clone(); move || { let c = std::process::Command::new(&exe).args(&args).env("HEARTH_MEM_CHILD", "1").spawn(); match c { Ok(c) => { *child.lock().unwrap() = Some(c); } Err(_) => std::process::exit(1) } } };
    #[cfg(unix)]
    {   // pass a stop on to the server, then end when it has ended (the main loop sees it)
        let mut set: libc::sigset_t = unsafe { std::mem::zeroed() };
        unsafe { libc::sigemptyset(&mut set); libc::sigaddset(&mut set, libc::SIGTERM); libc::sigaddset(&mut set, libc::SIGINT); libc::sigaddset(&mut set, libc::SIGHUP); libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut()); }
        let child2 = child.clone();
        std::thread::spawn(move || loop {
            let mut sig: libc::c_int = 0;
            unsafe { libc::sigwait(&set, &mut sig); }
            STOPPING.store(true, std::sync::atomic::Ordering::Relaxed);
            if let Some(c) = child2.lock().unwrap().as_ref() { unsafe { libc::kill(c.id() as i32, sig); } }
        });
    }
    launch();
    let ping = |h: &str| -> bool {
        use std::io::{Read, Write};
        let Ok(mut s) = std::net::TcpStream::connect_timeout(&format!("{h}:{port}").parse().unwrap_or_else(|_| std::net::SocketAddr::from(([127, 0, 0, 1], port))), std::time::Duration::from_millis(timeout)) else { return false };
        let _ = s.set_read_timeout(Some(std::time::Duration::from_millis(timeout)));
        if s.write_all(format!("GET /health HTTP/1.1\r\nHost: {h}\r\nConnection: close\r\n\r\n").as_bytes()).is_err() { return false; }
        let mut b = [0u8; 32]; matches!(s.read(&mut b), Ok(n) if n >= 12 && &b[9..12] == b"200")
    };
    let mut next = std::time::Instant::now() + std::time::Duration::from_millis(grace.max(every));
    let mut fails = 0u64; let mut kills: Vec<std::time::Instant> = vec![];
    loop {
        std::thread::sleep(std::time::Duration::from_millis(200));
        // the server ended by itself: so do we (a restart it asked for comes back through the service manager)
        let exited = { let mut g = child.lock().unwrap(); g.as_mut().and_then(|c| c.try_wait().ok().flatten()) };
        if let Some(st) = exited { std::process::exit(st.code().unwrap_or(1)); }
        if std::time::Instant::now() < next { continue; }
        next = std::time::Instant::now() + std::time::Duration::from_millis(every);
        if ping(&h) { fails = 0; continue; }
        fails += 1; if fails < fails_max { continue; }
        kills.retain(|k| k.elapsed().as_secs() < 600); kills.push(std::time::Instant::now());
        if kills.len() > 5 { eprintln!("watchdog: the server keeps getting stuck; giving up so the service manager can take over"); std::process::exit(1); }
        eprintln!("watchdog: no answer from the server for {fails_max} checks in a row; restarting it");
        { let mut g = child.lock().unwrap(); if let Some(c) = g.as_mut() { let _ = c.kill(); let _ = c.wait(); } }
        eprintln!("watchdog: the server was stopped because it did not answer; starting it again");
        launch(); fails = 0; next = std::time::Instant::now() + std::time::Duration::from_millis(grace.max(every));
    }
}
