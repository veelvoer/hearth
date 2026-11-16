//! The everyday commands of an installed server: code, status, start/stop/restart, logs, update, uninstall.
use crate::detect::*;
use crate::install::{bin_of, find_instances};
use crate::sysx::*;
use crate::ui::*;
use crate::update::{self, Source};
use crate::VERSION;
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn s<'a>(v: &'a Value, k: &str) -> &'a str { v[k].as_str().unwrap_or("") }

/// Which Hearth on this server? The one named by --instance, else the one of this user, else the only one.
pub fn pick_instance(env: &Env, wanted: Option<&str>) -> Option<(String, Value)> {
    let all = find_instances(&env.opt());
    match wanted {
        Some(w) => all.into_iter().find(|(n, _)| n == w),
        None => { let me = env.user.clone(); let n = all.len(); let mine = all.iter().find(|(_, m)| s(m, "user") == me).cloned(); mine.or_else(|| if n == 1 { all.into_iter().next() } else { None }) }
    }
}

pub fn pairing_link(url: &str, token: &str, name: &str) -> String {
    let q: String = url::form_urlencoded::Serializer::new(String::new()).append_pair("url", url).append_pair("token", token).append_pair("name", name).finish();
    format!("hearth://pair?{q}")
}
pub fn pretty_code(code: &str) -> String {
    if code.len() == 6 && code.chars().all(|c| c.is_ascii_digit()) { format!("{} {}", &code[..3], &code[3..]) } else { code.to_string() }
}
fn pad(sx: &str, n: usize) -> String { let l = sx.chars().count(); format!("{sx}{}", " ".repeat(n.saturating_sub(l))) }

/// The finish line: one address and one short code.
pub fn show_code(env: &Env, meta: &Value, first: bool) {
    let port = meta["port"].as_u64().unwrap_or(47601) as u16;
    let bin = if s(meta, "bin").is_empty() { "hearth-server".to_string() } else { s(meta, "bin").to_string() };
    let cfg = Path::new(s(meta, "cfgDir")).join("relay.json");
    let token = serde_json::from_str::<Value>(&env.sudo_cat(&cfg)).ok().and_then(|v| v["token"].as_str().map(String::from)).unwrap_or_default();
    let mut code = String::new();
    if env.live() && !token.is_empty() { if let Ok(j) = local_json(port, "POST", "/pair/new", &token, Some(&json!({}))) { code = j["code"].as_str().unwrap_or("").to_string(); } }
    if !env.live() { code = "123456".into(); }
    if code.is_empty() { bad(&format!("I could not make a code. Is Hearth running?  Try:  {bin} status")); return; }
    let link = pairing_link(s(meta, "url"), &token, s(meta, "name"));
    gap();
    println!("{}", ember(&format!("  ┌{}┐", "─".repeat(54))));
    println!("{}{}{}", ember("  │"), bold("   Your server is ready! "), format!("{}{}", " ".repeat(29), ember("│")));
    println!("{}{}{}", ember("  │"), " ".repeat(54), ember("│"));
    println!("{}   Address:  {}{}", ember("  │"), bold(&pad(s(meta, "url"), 41)), ember("│"));
    println!("{}   Code:     {}{}", ember("  │"), bold(&ember(&pad(&pretty_code(&code), 41))), ember("│"));
    println!("{}", ember(&format!("  └{}┘", "─".repeat(54))));
    gap();
    say(&format!("{} Do this on each one:", bold("Now connect your devices.")));
    gap();
    item(1, "Install the Hearth app (from github.com/veelvoer/hearth/releases).");
    item(2, &format!("Open it. It asks for a server. Type the {} and the {} from the box above.", bold("address"), bold("code")));
    item(3, "That is all. Your laptop and phone now share the same chats and project files.");
    gap();
    say(&dim(&format!("The code works for 10 minutes. Type  {bin} code  on this server any time to see the address, a new code and the link again, for example to connect another computer.")));
    say(&dim("Or paste this one link on a device instead (keep it secret, it opens your server):"));
    println!("  {}", dim(&link));
    gap();
    if first { say("Have fun! 🔥"); }
}

pub struct Ctx<'a> { pub env: &'a Env, pub instance: String, pub meta: Value, pub install_dir: PathBuf, pub port: u16 }

pub fn status(c: &Ctx) {
    banner(VERSION);
    let up = tcp_up(c.port);
    if up { ok(&format!("Hearth is running at {}", s(&c.meta, "url"))); } else { bad("Hearth is not answering."); }
    say(&dim(&if s(&c.meta, "mode") == "docker" { format!("Docker folder: {}", c.install_dir.display()) } else { format!("Logs: sudo journalctl -u {} -n 30", c.instance) }));
}

pub fn service_cmd(c: &Ctx, cmd: &str) -> Result<()> {
    banner(VERSION);
    let label = format!("{}{} Hearth", cmd[..1].to_uppercase(), &cmd[1..]);
    let sudo = RunOpts { sudo: true, cwd: Some(c.install_dir.clone()), ..Default::default() };
    if s(&c.meta, "mode") == "docker" { c.env.must(&label, "docker", &["compose", cmd], &sudo)?; } else { c.env.must(&label, "systemctl", &[cmd, &c.instance], &RunOpts { sudo: true, ..Default::default() })?; }
    if cmd != "stop" {
        for _ in 0..20 { if tcp_up(c.port) { break; } std::thread::sleep(std::time::Duration::from_secs(1)); }
        if tcp_up(c.port) { ok(&format!("Hearth is running at {}", s(&c.meta, "url"))); } else { bad("Hearth did not answer yet. Look at the logs with: hearth-server logs"); }
    } else { ok("Hearth is stopped. Start it again with: hearth-server start"); }
    Ok(())
}

pub fn logs(c: &Ctx) {
    banner(VERSION);
    let sudo = RunOpts { sudo: true, cwd: Some(c.install_dir.clone()), ..Default::default() };
    let r = if s(&c.meta, "mode") == "docker" { c.env.run("Last lines of the log", "docker", &["compose", "logs", "--tail", "60"], &sudo) } else { c.env.run("Last lines of the log", "journalctl", &["-u", &c.instance, "-n", "60", "--no-pager"], &sudo) };
    println!("{}", r.out);
}

/// `hearth-server update`: newest hearth-relay + hearth-server from the release, checked and replaced, then restarted.
pub fn update(c: &Ctx, src: &Source, force: bool) -> Result<()> {
    banner(VERSION);
    say("Updating to the newest version…"); gap();
    match update::update_install(c.env, &c.install_dir, src, force) {
        Err(e) => { bad(&format!("The update did not finish: {e}")); bail!("update failed") }
        Ok(None) => { ok("You already have the newest version."); Ok(()) }
        Ok(Some(u)) => {
            ok(&format!("Hearth {} → {} (checked and replaced)", u.from, u.to));
            update::bump_meta(c.env, &c.install_dir, &u.to)?;
            if c.env.live() { update::restart_hint(s(&c.meta, "mode"), &c.instance, &c.install_dir, c.env)?; }
            Ok(())
        }
    }
}

pub fn uninstall(c: &Ctx, pr: &mut Prompt) -> Result<()> {
    banner(VERSION);
    say("This removes the Hearth server program. Your projects and chats stay where they are.");
    if !pr.confirm("Remove Hearth from this server?", false) { return Ok(()); }
    let env = c.env; let i = &c.instance;
    let sudo = RunOpts { sudo: true, cwd: Some(c.install_dir.clone()), ..Default::default() };
    if s(&c.meta, "mode") == "docker" { env.run("Stop Hearth", "docker", &["compose", "down"], &sudo); }
    else {
        if env.prefix.is_none() { env.run("Stop Hearth", "systemctl", &["disable", "--now", i], &RunOpts { sudo: true, ..Default::default() }); }
        env.remove(&[&env.sys(&format!("/etc/systemd/system/{i}.service")), &env.sys(&format!("/etc/caddy/{i}.caddy"))]);
        let main = env.sys("/etc/caddy/Caddyfile");
        if let Ok(t) = std::fs::read_to_string(&main) {
            let line = format!("import /etc/caddy/{i}.caddy");
            if t.lines().any(|l| l == line) { let kept: Vec<&str> = t.lines().filter(|l| *l != line).collect(); let _ = env.write(&main, &format!("{}\n", kept.join("\n")), 0o644); }
        }
        if env.prefix.is_none() { env.run("Reload services", "sh", &["-c", "systemctl daemon-reload; systemctl restart caddy 2>/dev/null; true"], &RunOpts { sudo: true, ..Default::default() }); }
    }
    let d = &c.install_dir;
    env.remove(&[&d.join("hearth-relay"), &d.join("hearth-server"), &d.join("hearth-relay.prev"), &d.join("hearth-server.prev"), &d.join("installer"), &d.join("install.json"), &env.sys(&format!("/usr/local/bin/{}", bin_of(i))), &env.sys(&format!("/etc/caddy/{i}.d"))]);
    ok("Hearth is removed.");
    Ok(())
}
