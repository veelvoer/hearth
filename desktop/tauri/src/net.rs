//! Talking to relays (this computer's, your server, a friend's shared chat): JSON calls, uploads and the event stream.
use hearth_relay::util::J;
use serde_json::{json, Value};
use std::time::Duration;

pub fn base(m: &Value) -> String {
    let host = m.s("host");
    let host = if host.contains(':') && !host.starts_with('[') { format!("[{host}]") } else { host };
    format!("{}://{}:{}", if m.t("secure") { "https" } else { "http" }, host, m.n("port") as u16)
}

pub fn client(read_timeout: Option<u64>) -> reqwest::Client {
    let mut b = reqwest::Client::builder().user_agent(concat!("Hearth/", env!("CARGO_PKG_VERSION"))).connect_timeout(Duration::from_secs(10));
    if let Some(s) = read_timeout { b = b.read_timeout(Duration::from_secs(s)); }
    b.build().unwrap_or_default()
}

pub fn err_text(e: &reqwest::Error) -> String {
    if e.is_timeout() { return "Timed out".into(); }
    if e.is_connect() { return "Could not connect".into(); }
    let s = e.to_string();
    s.split(": ").last().unwrap_or(&s).to_string()
}

/// One request to a relay. [total] is the most the whole request may take (None: only a stalled connection ends it).
pub async fn request(c: &reqwest::Client, m: &Value, method: &str, p: &str, body: Option<(Vec<u8>, &str)>, total: Option<u64>) -> Result<reqwest::Response, String> {
    let method = reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET);
    let mut rq = c.request(method, format!("{}{}", base(m), p)).header("Authorization", format!("Bearer {}", m.s("token")));
    if let Some((b, ty)) = body { rq = rq.header("Content-Type", ty).body(b); }
    if let Some(t) = total { rq = rq.timeout(Duration::from_millis(t)); }
    rq.send().await.map_err(|e| err_text(&e))
}

pub fn err_of(b: &[u8]) -> Option<String> { serde_json::from_slice::<Value>(b).ok().map(|v| v.s("error")).filter(|s| !s.is_empty()) }

pub async fn json_call(c: &reqwest::Client, m: &Value, method: &str, p: &str, body: Option<&Value>) -> Result<Value, String> {
    let res = request(c, m, method, p, body.map(|b| (b.to_string().into_bytes(), "application/json")), Some(10000)).await?;
    let st = res.status().as_u16();
    let buf = res.bytes().await.map_err(|e| err_text(&e))?;
    if st == 401 { return Err(format!("Wrong token for {}", m.s("name"))); }
    if st != 200 { return Err(err_of(&buf).unwrap_or_else(|| format!("Relay error {st}"))); }
    if buf.iter().all(|b| b.is_ascii_whitespace()) { return Ok(json!({})); }
    serde_json::from_slice(&buf).map_err(|e| e.to_string())
}

/// Reads an SSE / ndjson body piece by piece and hands every complete record to [each].
pub async fn read_records(res: reqwest::Response, sep: &str, mut each: impl FnMut(&str)) {
    use futures_util::StreamExt;
    let mut s = res.bytes_stream();
    let mut buf = String::new();
    let mut raw: Vec<u8> = Vec::new();
    while let Some(Ok(chunk)) = s.next().await {
        raw.extend_from_slice(&chunk);
        // keep an incomplete UTF-8 sequence for the next piece
        let ok = match std::str::from_utf8(&raw) { Ok(t) => t.len(), Err(e) => e.valid_up_to() };
        buf.push_str(std::str::from_utf8(&raw[..ok]).unwrap_or(""));
        raw.drain(..ok);
        while let Some(i) = buf.find(sep) {
            let rec: String = buf[..i].to_string();
            buf.drain(..i + sep.len());
            each(&rec);
        }
    }
}

/// Finds Hearth computers on this network (they answer a UDP broadcast).
pub async fn discover() -> Vec<Value> {
    tokio::task::spawn_blocking(|| {
        let mut found: Vec<Value> = Vec::new();
        let Ok(s) = std::net::UdpSocket::bind("0.0.0.0:0") else { return found };
        let _ = s.set_broadcast(true);
        let _ = s.set_read_timeout(Some(Duration::from_millis(200)));
        let _ = s.send_to(b"CLAUDE_METER_DISCOVER", "255.255.255.255:47600");
        let end = std::time::Instant::now() + Duration::from_millis(1300);
        let mut buf = [0u8; 1024];
        while std::time::Instant::now() < end {
            if let Ok((n, from)) = s.recv_from(&mut buf) {
                if let Ok(o) = serde_json::from_slice::<Value>(&buf[..n]) {
                    let host = from.ip().to_string();
                    found.retain(|f| f.s("host") != host);
                    let port = if o.n("port") > 0.0 { o.n("port") } else { 47601.0 };
                    found.push(json!({"name": if o.s("name").is_empty() { host.clone() } else { o.s("name") }, "host": host, "port": port as u16}));
                }
            }
        }
        found
    }).await.unwrap_or_default()
}
