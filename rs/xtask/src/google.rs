//! scripts/google-tasks-token.js: connects Hearth's support mail list to Google Tasks (one-time OAuth on a computer with a browser).
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpListener;

pub struct Args { pub client_id: String, pub client_secret: String, pub server: Option<String>, pub token: Option<String> }
pub struct Endpoints { pub auth: String, pub token: String }
impl Endpoints {
    pub fn real() -> Endpoints { Endpoints { auth: "https://accounts.google.com/o/oauth2/v2/auth".into(), token: "https://oauth2.googleapis.com/token".into() } }
}

pub fn auth_url(auth: &str, client_id: &str, redirect: &str) -> String {
    let q = url::form_urlencoded::Serializer::new(String::new()).append_pair("client_id", client_id).append_pair("redirect_uri", redirect).append_pair("response_type", "code").append_pair("scope", "https://www.googleapis.com/auth/tasks").append_pair("access_type", "offline").append_pair("prompt", "consent").finish();
    format!("{auth}?{q}")
}
/// What Google answers with -> the settings Hearth stores.
pub fn settings_from(client_id: &str, client_secret: &str, j: &Value) -> Result<Value> {
    let rt = j["refresh_token"].as_str().filter(|s| !s.is_empty()).ok_or_else(|| anyhow!("{}", j["error_description"].as_str().or(j["error"].as_str()).unwrap_or("Google gave no refresh token")))?;
    Ok(json!({"googleTasks": {"clientId": client_id, "clientSecret": client_secret, "refreshToken": rt}}))
}

/// Starts the local page Google sends the browser back to (calls `announce` with the address to open), waits for the code, and finishes.
/// Returns the exit code.
pub fn run(a: &Args, ep: &Endpoints, announce: &mut dyn FnMut(&str)) -> Result<i32> {
    if a.client_id.is_empty() || a.client_secret.is_empty() { bail!("Usage: cargo xtask google-tasks-token <client-id> <client-secret> [server-address server-token]"); }
    let l = TcpListener::bind("127.0.0.1:0")?;
    let redirect = format!("http://127.0.0.1:{}", l.local_addr()?.port());
    announce(&auth_url(&ep.auth, &a.client_id, &redirect));
    for s in l.incoming() {
        let mut s = s?;
        let mut buf = [0u8; 4096]; let n = s.read(&mut buf)?;
        let req = String::from_utf8_lossy(&buf[..n]).to_string();
        let target = req.split_whitespace().nth(1).unwrap_or("/");
        let code = url::Url::parse(&format!("http://127.0.0.1{target}")).ok().and_then(|u| u.query_pairs().find(|(k, _)| k == "code").map(|(_, v)| v.to_string()));
        let Some(code) = code else { let _ = respond(&mut s, "Waiting for Google…"); continue; };
        let result: Result<()> = (|| {
            let http = reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(30)).build()?;
            let j: Value = http.post(&ep.token).form(&[("code", code.as_str()), ("client_id", &a.client_id), ("client_secret", &a.client_secret), ("redirect_uri", &redirect), ("grant_type", "authorization_code")]).send()?.json()?;
            let body = settings_from(&a.client_id, &a.client_secret, &j)?;
            if let (Some(server), Some(token)) = (&a.server, &a.token) {
                let p = http.post(format!("{}/admin/options", server.trim_end_matches('/'))).bearer_auth(token).json(&body).send()?;
                if !p.status().is_success() { bail!("Your Hearth server said {}", p.status().as_u16()); }
                println!("Done: your Hearth server can now add support mails to Google Tasks.");
            } else { println!("Send this to your Hearth server (POST /admin/options):\n{body}"); }
            Ok(())
        })();
        return match result {
            Ok(()) => { let _ = respond(&mut s, "Done. You can close this tab."); Ok(0) }
            Err(e) => { eprintln!("Failed: {e}"); let _ = respond(&mut s, &format!("Failed: {e}")); Ok(1) }
        };
    }
    Ok(1)
}
fn respond(s: &mut std::net::TcpStream, text: &str) -> std::io::Result<()> {
    s.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len()).as_bytes())
}
