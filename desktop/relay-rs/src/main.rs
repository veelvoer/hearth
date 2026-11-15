//! Hearth relay front. It listens on the public port and serves the routes that are already written in Rust;
//! everything else goes to the Node relay it starts on a private port. Routes move over one by one.
mod core;
mod jsu;
mod pricing;
mod service;

use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{header, HeaderMap, HeaderName, HeaderValue, Request, Response, StatusCode},
    Router,
};
use std::{net::SocketAddr, path::PathBuf, process::Stdio, sync::Arc, time::Duration};
use tokio::process::Command;

struct Front {
    client: reqwest::Client,
    node: String, // http://127.0.0.1:<private port>
    port: u16,
    ready: std::sync::atomic::AtomicBool,
}

/// Headers that belong to one connection and must not be copied to the other side.
fn hop(n: &HeaderName) -> bool {
    matches!(n.as_str(), "connection" | "keep-alive" | "proxy-connection" | "te" | "trailer" | "transfer-encoding" | "upgrade" | "host")
}

async fn forward(State(f): State<Arc<Front>>, ConnectInfo(peer): ConnectInfo<SocketAddr>, req: Request<Body>) -> Response<Body> {
    let (parts, body) = req.into_parts();
    // the Node relay takes a moment to start: until it listens, wait (a request with a body cannot be sent twice)
    if !f.ready.load(std::sync::atomic::Ordering::Relaxed) {
        for _ in 0..200 {
            if tokio::net::TcpStream::connect(("127.0.0.1", f.port)).await.is_ok() { f.ready.store(true, std::sync::atomic::Ordering::Relaxed); break; }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    let url = format!("{}{}", f.node, parts.uri.path_and_query().map(|x| x.as_str()).unwrap_or("/"));
    let mut h = HeaderMap::new();
    for (k, v) in parts.headers.iter() {
        if !hop(k) {
            h.append(k.clone(), v.clone());
        }
    }
    // The Node relay trusts X-Forwarded-For only from a loopback peer: a client from outside never gets to choose it.
    // A loopback peer (Caddy, the desktop app) is passed on as it is; everyone else is named by the address we saw.
    if !peer.ip().is_loopback() {
        if let Ok(v) = HeaderValue::from_str(&peer.ip().to_string()) {
            h.insert(HeaderName::from_static("x-forwarded-for"), v);
        }
    }
    let mut last = String::new();
    // The Node relay may still be starting: try for a few seconds before giving up.
    for attempt in 0..100 {
        let mut rb = f.client.request(parts.method.clone(), &url).headers(h.clone());
        // a body can be streamed only once, so retries are for requests without one
        let has_body = parts.headers.contains_key(header::CONTENT_LENGTH) && parts.headers.get(header::CONTENT_LENGTH).map(|v| v != "0").unwrap_or(false)
            || parts.headers.contains_key(header::TRANSFER_ENCODING);
        if has_body {
            rb = rb.body(reqwest::Body::wrap_stream(body.into_data_stream()));
            return match rb.send().await {
                Ok(r) => relay(r),
                Err(e) => bad_gateway(&e.to_string()),
            };
        }
        match rb.send().await {
            Ok(r) => return relay(r),
            Err(e) => {
                last = e.to_string();
                if !e.is_connect() || attempt >= 99 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
    bad_gateway(&last)
}

fn relay(r: reqwest::Response) -> Response<Body> {
    let mut out = Response::builder().status(r.status());
    for (k, v) in r.headers().iter() {
        if !hop(k) {
            out = out.header(k, v);
        }
    }
    out.body(Body::from_stream(r.bytes_stream())).unwrap_or_else(|_| bad_gateway("bad response"))
}

fn bad_gateway(why: &str) -> Response<Body> {
    let b = serde_json::json!({ "error": format!("The relay is not answering ({})", why) }).to_string();
    Response::builder().status(StatusCode::BAD_GATEWAY).header(header::CONTENT_TYPE, "application/json").body(Body::from(b)).unwrap()
}

#[cfg(unix)]
async fn terminated() {
    if let Ok(mut t) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) { t.recv().await; } else { std::future::pending::<()>().await }
}
#[cfg(not(unix))]
async fn terminated() { std::future::pending::<()>().await }

/// 32 random bytes from the system, as hex.
fn random_hex() -> String {
    let mut b = [0u8; 32];
    if getrandom::fill(&mut b).is_err() {
        eprintln!("No system randomness: the Rust reads stay off.");
        std::process::exit(1);
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn flag(args: &[String], k: &str) -> Option<String> {
    args.iter().position(|a| a == k).and_then(|i| args.get(i + 1).cloned())
}

/// Where the Node relay lives: next to this program (installed), or in ../relay (checkout).
fn standalone() -> PathBuf {
    if let Ok(p) = std::env::var("HEARTH_NODE_RELAY") {
        return PathBuf::from(p);
    }
    let exe = std::env::current_exe().unwrap_or_default();
    for up in [exe.parent().map(|x| x.join("relay")), exe.ancestors().nth(4).map(|x| x.join("relay")), Some(PathBuf::from("relay"))].into_iter().flatten() {
        if up.join("standalone.js").exists() {
            return up.join("standalone.js");
        }
    }
    PathBuf::from("standalone.js")
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let node_bin = std::env::var("HEARTH_NODE").unwrap_or_else(|_| "node".into());
    let script = standalone();
    // `--link` and friends only print something: the Node relay does it
    if args.iter().any(|a| a == "--link") {
        let s = Command::new(&node_bin).arg(&script).args(&args).status().await;
        std::process::exit(s.ok().and_then(|x| x.code()).unwrap_or(1));
    }
    // for tests: only the private reads, on a port and with a secret given on the command line
    if args.iter().any(|a| a == "--core-only") {
        let port: u16 = flag(&args, "--port").and_then(|p| p.parse().ok()).unwrap_or(0);
        let l = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}")).await.expect("port");
        println!("listening {}", l.local_addr().map(|a| a.port()).unwrap_or(0));   // the program that started us reads the port from this line
        let _ = std::io::Write::flush(&mut std::io::stdout());
        let _ = axum::serve(l, service::router(flag(&args, "--secret").unwrap_or_default())).await;
        return;
    }
    let host = flag(&args, "--host").unwrap_or_else(|| "127.0.0.1".into());
    let port: u16 = flag(&args, "--port").and_then(|p| p.parse().ok()).unwrap_or(47601);
    // a free private port for the Node relay
    let private = std::net::TcpListener::bind("127.0.0.1:0").and_then(|l| l.local_addr()).map(|a| a.port()).unwrap_or(47999);
    let mut node_args: Vec<String> = Vec::new();
    let mut it = args.iter().peekable();
    while let Some(a) = it.next() {
        if a == "--port" || a == "--host" {
            it.next();
        } else {
            node_args.push(a.clone());
        }
    }
    node_args.extend(["--port".into(), private.to_string(), "--host".into(), "127.0.0.1".into()]);

    let bind = if host.contains(':') && !host.starts_with('[') { format!("[{host}]:{port}") } else { format!("{host}:{port}") };
    let listener = match tokio::net::TcpListener::bind(bind).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Could not listen on {host}:{port}: {e}");
            std::process::exit(1);
        }
    };
    // the private side for the Node relay: heavy reads in Rust, reachable only from this machine and only with the secret
    let secret = random_hex();
    let core_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("private port");
    let core_port = core_listener.local_addr().map(|a| a.port()).unwrap_or(0);
    let secret_for_task = secret.clone();
    tokio::spawn(async move { let _ = axum::serve(core_listener, service::router(secret_for_task)).await; });
    let mut child = Command::new(&node_bin)
        .arg(&script)
        .args(&node_args)
        .env_remove("HEARTH_RUST_FRONT")
        .env("HEARTH_NO_RUST", "1")
        .env("HEARTH_RUST_CORE", format!("http://127.0.0.1:{core_port}"))
        .env("HEARTH_RUST_CORE_SECRET", &secret)
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap_or_else(|e| {
            eprintln!("Could not start the Node relay ({node_bin} {}): {e}", script.display());
            std::process::exit(1);
        });
    let front = Arc::new(Front { client: reqwest::Client::builder().pool_idle_timeout(Duration::from_secs(30)).build().unwrap(), node: format!("http://127.0.0.1:{private}"), port: private, ready: std::sync::atomic::AtomicBool::new(false) });
    let app = Router::new().fallback(forward).with_state(front);
    let serve = axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>());
    tokio::select! {
        r = serve => { if let Err(e) = r { eprintln!("server: {e}"); } }
        s = child.wait() => { std::process::exit(s.ok().and_then(|x| x.code()).unwrap_or(1)); }   // the Node relay ended: so do we, and the service manager starts us again
        _ = tokio::signal::ctrl_c() => {}
        _ = terminated() => {}
    }
    // stop the Node relay the polite way, so its own watchdog stops the real server too
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        unsafe { libc::kill(pid as i32, libc::SIGTERM); }
        let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
    }
    let _ = child.kill().await;
}
