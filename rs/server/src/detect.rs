//! Facts about this computer and about the running relay.
use crate::sysx::have;
use anyhow::{anyhow, Result};
use serde_json::Value;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

pub struct OsInfo { pub name: String, pub id: String, pub pm: Option<&'static str> }

pub fn parse_os_release(text: &str) -> (String, String) {
    let (mut pretty, mut id) = (String::new(), String::new());
    for l in text.lines() {
        if let Some((k, v)) = l.split_once('=') { let v = v.replace('"', ""); match k { "PRETTY_NAME" => pretty = v, "ID" => id = v, _ => {} } }
    }
    (pretty, id)
}
pub fn os_info() -> OsInfo {
    let (pretty, id) = parse_os_release(&std::fs::read_to_string("/etc/os-release").unwrap_or_default());
    let pm = if have("apt-get") { Some("apt") } else if have("dnf") { Some("dnf") } else if have("pacman") { Some("pacman") } else { None };
    OsInfo { name: if pretty.is_empty() { std::env::consts::OS.to_string() } else { pretty }, id, pm }
}

fn client(ms: u64) -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder().timeout(Duration::from_millis(ms)).user_agent("Hearth-server").build()?)
}
pub fn fetch_text(url: &str, ms: u64) -> String {
    client(ms).ok().and_then(|c| c.get(url).send().ok()).and_then(|r| r.text().ok()).map(|t| t.trim().to_string()).unwrap_or_default()
}
pub fn is_ipv4(s: &str) -> bool {
    let p: Vec<&str> = s.split('.').collect();
    p.len() == 4 && p.iter().all(|x| !x.is_empty() && x.len() <= 3 && x.chars().all(|c| c.is_ascii_digit()))
}
/// The address of this server on the internet, looked up over HTTP (first answer that looks like an IPv4 address).
pub fn public_ip_from(urls: &[&str]) -> String {
    for u in urls { let ip = fetch_text(u, 6000); if is_ipv4(&ip) { return ip; } }
    String::new()
}
pub fn public_ip() -> String { public_ip_from(&["https://api.ipify.org", "https://ifconfig.me/ip", "https://icanhazip.com"]) }

pub fn port_free(p: u16) -> bool { std::net::TcpListener::bind(("127.0.0.1", p)).is_ok() }

/// Health check: the port accepts a connection AND GET /health answers 200.
pub fn tcp_up(port: u16) -> bool {
    let addr = match ("127.0.0.1", port).to_socket_addrs().ok().and_then(|mut a| a.next()) { Some(a) => a, None => return false };
    let mut s = match TcpStream::connect_timeout(&addr, Duration::from_secs(2)) { Ok(s) => s, Err(_) => return false };
    let _ = s.set_read_timeout(Some(Duration::from_secs(2))); let _ = s.set_write_timeout(Some(Duration::from_secs(2)));
    if s.write_all(format!("GET /health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n").as_bytes()).is_err() { return false; }
    let mut head = [0u8; 64]; let mut got = 0;
    while got < 12 { match s.read(&mut head[got..]) { Ok(0) | Err(_) => break, Ok(n) => got += n } }
    String::from_utf8_lossy(&head[..got]).split_whitespace().nth(1) == Some("200")
}

pub fn local_json(port: u16, method: &str, path: &str, token: &str, body: Option<&Value>) -> Result<Value> {
    let c = client(8000)?;
    let url = format!("http://127.0.0.1:{port}{path}");
    let mut r = if method == "POST" { c.post(&url) } else { c.get(&url) }.bearer_auth(token);
    if let Some(b) = body { r = r.json(b); }
    let t = r.send().map_err(|e| anyhow!("{e}"))?.text()?;
    serde_json::from_str(if t.is_empty() { "{}" } else { &t }).map_err(|_| anyhow!("bad answer"))
}
