//! Looks for a newer Hearth on GitHub and, on a server, replaces its own program with the newer one. Only talks to github.com/veelvoer/hearth.
use crate::util::*;
use serde_json::{json, Value};

const REPO: &str = "veelvoer/hearth";
fn parse(v: &str) -> Vec<i64> { v.trim_start_matches('v').split('.').map(|n| n.parse().unwrap_or(0)).collect() }
pub fn newer(a: &str, b: &str) -> bool { let (x, y) = (parse(a), parse(b)); for i in 0..3 { let (p, q) = (*x.get(i).unwrap_or(&0), *y.get(i).unwrap_or(&0)); if p != q { return p > q; } } false }
fn client() -> reqwest::Client { reqwest::Client::builder().user_agent("Hearth-updater").build().unwrap_or_default() }

/// The newest release: { tag, version, notes, assets }
pub async fn latest() -> Result<Value, String> {
    let r = client().get(format!("https://api.github.com/repos/{REPO}/releases/latest")).header("Accept", "application/vnd.github+json").timeout(std::time::Duration::from_secs(20)).send().await.map_err(|e| e.to_string())?;
    if !r.status().is_success() { return Err(format!("GitHub said {}", r.status().as_u16())); }
    let j: Value = r.json().await.map_err(|e| e.to_string())?;
    Ok(json!({"tag": j.g("tag_name"), "version": j.s("tag_name").trim_start_matches('v'), "notes": clip(&j.s("body"), 1500), "assets": arr(j.g("assets")).iter().map(|a| json!({"name": a.g("name"), "url": a.g("browser_download_url"), "size": a.g("size"), "digest": a.g("digest")})).collect::<Vec<_>>()}))
}
pub fn asset_name() -> &'static str {
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) { "hearth-relay-linux-x64" } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) { "hearth-relay-linux-arm64" } else { "" }
}
/// Downloads the program of release `tag`, checks it, and puts it in place of the running one. The caller restarts.
pub async fn apply(tag: &str) -> Result<String, String> {
    if !regex::Regex::new(r"^v\d+\.\d+\.\d+$").unwrap().is_match(tag) { return Err("bad version".into()); }
    let name = asset_name(); if name.is_empty() { return Err("There is no update for this kind of computer.".into()); }
    let want = tag.trim_start_matches('v').to_string();
    let base = format!("https://github.com/{REPO}/releases/download/{tag}/");
    let c = client();
    let sums = c.get(format!("{base}hearth-relay.sha256")).timeout(std::time::Duration::from_secs(60)).send().await.map_err(|e| e.to_string())?.text().await.map_err(|e| e.to_string())?;
    let line = sums.lines().find(|l| l.trim().ends_with(&format!(" {name}")) || l.trim().ends_with(&format!("*{name}"))).ok_or(format!("this release has no {name}"))?;
    let bin = c.get(format!("{base}{name}")).timeout(std::time::Duration::from_secs(300)).send().await.map_err(|e| e.to_string())?.bytes().await.map_err(|e| e.to_string())?;
    if sha256_hex(&bin) != line.split_whitespace().next().unwrap_or("") { return Err("the download did not match its checksum".into()); }
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let next = me.with_extension("new");
    std::fs::write(&next, &bin).map_err(|e| format!("This installation cannot update itself ({e}). On the server run: hearth-server update"))?;
    #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; let _ = std::fs::set_permissions(&next, std::fs::Permissions::from_mode(0o755)); }
    let v = std::process::Command::new(&next).arg("--version").output().map_err(|e| e.to_string())?;
    if String::from_utf8_lossy(&v.stdout).trim() != want { let _ = std::fs::remove_file(&next); return Err("the update says it is another version".into()); }
    let prev = me.with_extension("prev"); let _ = std::fs::copy(&me, &prev);
    std::fs::rename(&next, &me).map_err(|e| e.to_string())?;
    Ok(want)
}
