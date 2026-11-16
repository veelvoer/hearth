//! Updates: download the newest hearth-relay and hearth-server from the GitHub release, check their sha256
//! (like desktop/relay-rs/src/selfupdate.rs), and replace the installed files atomically.
use crate::sysx::{Env, RunOpts};
use crate::ui::*;
use crate::REPO;
use anyhow::{anyhow, bail, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// Where releases come from (GitHub; tests point it at a local server).
#[derive(Clone)]
pub struct Source { pub api: String, pub download: String }
impl Source {
    pub fn github() -> Source {
        Source { api: std::env::var("HEARTH_RELEASE_API").unwrap_or_else(|_| format!("https://api.github.com/repos/{REPO}/releases/latest")), download: std::env::var("HEARTH_RELEASE_DOWNLOAD").unwrap_or_else(|_| format!("https://github.com/{REPO}/releases/download")) }
    }
}

pub fn arch_suffix() -> Option<&'static str> {
    match std::env::consts::ARCH { "x86_64" => Some("x64"), "aarch64" => Some("arm64"), _ => None }
}
pub fn relay_asset() -> Option<String> { arch_suffix().map(|a| format!("hearth-relay-linux-{a}")) }
pub fn server_asset() -> Option<String> { arch_suffix().map(|a| format!("hearth-server-linux-{a}")) }
pub const RELAY_SUMS: &str = "hearth-relay.sha256";
pub const SERVER_SUMS: &str = "hearth-server.sha256";

fn parse(v: &str) -> Vec<i64> { v.trim_start_matches('v').split('.').map(|n| n.parse().unwrap_or(0)).collect() }
/// Is version `a` newer than `b`?
pub fn newer(a: &str, b: &str) -> bool {
    let (x, y) = (parse(a), parse(b));
    for i in 0..3 { let (p, q) = (*x.get(i).unwrap_or(&0), *y.get(i).unwrap_or(&0)); if p != q { return p > q; } }
    false
}
pub fn valid_tag(t: &str) -> bool { let p: Vec<&str> = t.strip_prefix('v').unwrap_or("x").split('.').collect(); p.len() == 3 && p.iter().all(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())) }
pub fn sha256_hex(b: &[u8]) -> String { hex::encode(Sha256::digest(b)) }
/// The checksum of `name` in a `sha256sum` style file ("<hash>  name" or "<hash> *name").
pub fn checksum_for(sums: &str, name: &str) -> Option<String> {
    sums.lines().find(|l| { let l = l.trim(); l.ends_with(&format!(" {name}")) || l.ends_with(&format!("*{name}")) }).and_then(|l| l.split_whitespace().next()).map(|s| s.to_lowercase())
}

fn http(secs: u64) -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder().timeout(Duration::from_secs(secs)).user_agent("Hearth-updater").build()?)
}
pub fn latest_tag(src: &Source) -> Result<String> {
    let r = http(20)?.get(&src.api).header("Accept", "application/vnd.github+json").send()?;
    if !r.status().is_success() { bail!("GitHub said {}", r.status().as_u16()); }
    let j: Value = r.json()?;
    let tag = j["tag_name"].as_str().unwrap_or("").to_string();
    if !valid_tag(&tag) { bail!("the newest release has a strange name ({tag})"); }
    Ok(tag)
}
fn get_bytes(url: &str, secs: u64) -> Result<Vec<u8>> {
    let r = http(secs)?.get(url).send()?;
    if !r.status().is_success() { bail!("{} said {}", url, r.status().as_u16()); }
    Ok(r.bytes()?.to_vec())
}
/// Downloads `asset` of release `tag` and checks it against the release's checksum file.
pub fn fetch_verified(src: &Source, tag: &str, asset: &str, sums_name: &str) -> Result<Vec<u8>> {
    if !valid_tag(tag) { bail!("bad version"); }
    let base = format!("{}/{tag}", src.download.trim_end_matches('/'));
    let sums = String::from_utf8_lossy(&get_bytes(&format!("{base}/{sums_name}"), 60)?).to_string();
    let want = checksum_for(&sums, asset).ok_or_else(|| anyhow!("this release has no {asset}"))?;
    let bin = get_bytes(&format!("{base}/{asset}"), 300)?;
    if sha256_hex(&bin) != want { bail!("the download of {asset} did not match its checksum"); }
    Ok(bin)
}

/// What `prog --version` prints.
pub fn program_version(p: &Path) -> Option<String> {
    for _ in 0..5 {   // a file that was written a moment ago can still be "busy" for an instant
        match Command::new(p).arg("--version").output() {
            Ok(o) => return if o.status.success() { Some(String::from_utf8_lossy(&o.stdout).trim().to_string()) } else { None },
            Err(e) if e.raw_os_error() == Some(26) => std::thread::sleep(Duration::from_millis(60)),
            Err(_) => return None,
        }
    }
    None
}

/// Puts `bytes` in place of `target`: staged and checked first (it must report `want`), the old one kept as `.prev`, then renamed over.
pub fn atomic_replace(env: &Env, bytes: &[u8], target: &Path, want: &str) -> Result<()> {
    let scratch = std::env::temp_dir().join(format!("hearth-upd-{}-{}", std::process::id(), target.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
    std::fs::write(&scratch, bytes)?;
    crate::sysx::set_mode(&scratch, 0o755)?;
    let got = program_version(&scratch);
    if got.as_deref() != Some(want) { let _ = std::fs::remove_file(&scratch); bail!("the update says it is another version ({})", got.unwrap_or_else(|| "it does not run".into())); }
    if target.exists() && !env.dry {
        let prev = target.with_file_name(format!("{}.prev", target.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
        let _ = env.copy_file(target, &prev, 0o755);
    }
    let r = env.copy_file(&scratch, target, 0o755);
    let _ = std::fs::remove_file(&scratch);
    r
}

/// Gets hearth-relay of version `tag` into `dest` (used by install and update).
pub fn install_relay(env: &Env, src: &Source, tag: &str, dest: &Path) -> Result<()> {
    let asset = relay_asset().ok_or_else(|| anyhow!("There is no Hearth relay for this kind of computer."))?;
    if env.dry { println!("  {}", dim(&format!("[dry run] download {asset} {tag} and check its sha256"))); return Ok(()); }
    let bin = fetch_verified(src, tag, &asset, RELAY_SUMS)?;
    atomic_replace(env, &bin, dest, tag.trim_start_matches('v'))
}

#[derive(Debug)]
pub struct Updated { pub from: String, pub to: String, pub relay: bool, pub server: bool }

/// `hearth-server update` for a server install: both programs, checked and replaced; the caller restarts the service.
pub fn update_install(env: &Env, install_dir: &Path, src: &Source, force: bool) -> Result<Option<Updated>> {
    let relay_path = install_dir.join("hearth-relay");
    let server_path = install_dir.join("hearth-server");
    let current = program_version(&relay_path).unwrap_or_else(|| "0.0.0".into());
    let tag = latest_tag(src)?;
    let to = tag.trim_start_matches('v').to_string();
    if !force && !newer(&to, &current) { return Ok(None); }
    let mut up = Updated { from: current, to: to.clone(), relay: false, server: false };
    let relay_asset = relay_asset().ok_or_else(|| anyhow!("There is no update for this kind of computer."))?;
    let relay = fetch_verified(src, &tag, &relay_asset, RELAY_SUMS)?;
    // the installer itself: some releases may not carry it yet, then only the relay is updated
    let server = match server_asset() { Some(a) => match fetch_verified(src, &tag, &a, SERVER_SUMS) { Ok(b) => Some(b), Err(e) => { warn(&format!("Not updating the installer program: {e}")); None } }, None => None };
    atomic_replace(env, &relay, &relay_path, &to)?; up.relay = true;
    if let Some(b) = server { atomic_replace(env, &b, &server_path, &to)?; up.server = true; }
    Ok(Some(up))
}

/// Rewrites the "version" of install.json.
pub fn bump_meta(env: &Env, install_dir: &Path, version: &str) -> Result<()> {
    let f = install_dir.join("install.json");
    let mut m: Value = serde_json::from_str(&std::fs::read_to_string(&f)?)?;
    m["version"] = Value::String(version.to_string());
    env.write(&f, &serde_json::to_string_pretty(&m)?, 0o644)
}

pub fn restart_hint(meta_mode: &str, instance: &str, install_dir: &Path, env: &Env) -> Result<()> {
    if meta_mode == "docker" { env.must("Rebuild and restart Hearth", "docker", &["compose", "up", "-d", "--build"], &RunOpts { sudo: true, cwd: Some(install_dir.to_path_buf()), ..Default::default() })?; }
    else { env.must("Restart Hearth", "systemctl", &["restart", instance], &RunOpts { sudo: true, ..Default::default() })?; }
    Ok(())
}

pub fn repo_binary(repo: &Path) -> PathBuf { repo.join("desktop/relay-rs/target/release/hearth-relay") }
