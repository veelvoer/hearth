//! The interactive installer (server/hearth-server.js `install`): looks around, asks, installs, signs in, shows the pairing code.
use crate::detect::*;
use crate::sysx::*;
use crate::texts::*;
use crate::ui::*;
use crate::update::{self, Source};
use crate::{manage, VERSION};
use anyhow::{bail, Result};
use regex::Regex;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Default, Clone)]
pub struct Opts {
    pub mode: Option<String>,
    pub address: Option<String>,
    pub name: Option<String>,
    pub instance: Option<String>,
    /// Skip the lookup on the internet (tests, or a server that cannot reach the lookup sites).
    pub public_ip: Option<String>,
    /// A hearth-relay binary to install instead of downloading the release.
    pub relay_bin: Option<PathBuf>,
    pub source: Option<Source>,
}

pub fn bin_of(instance: &str) -> String { if instance == "hearth" { "hearth-server".into() } else { format!("{instance}-server") } }

pub fn find_instances(opt: &Path) -> Vec<(String, Value)> {
    let re = Regex::new(r"^hearth(-[a-z0-9]+)?$").unwrap();
    let mut out: Vec<(String, Value)> = vec![];
    if let Ok(rd) = std::fs::read_dir(opt) {
        for d in rd.flatten() {
            let n = d.file_name().to_string_lossy().to_string();
            if !re.is_match(&n) { continue; }
            if let Some(m) = std::fs::read_to_string(d.path().join("install.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) { out.push((n, m)); }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}
fn s<'a>(v: &'a Value, k: &str) -> &'a str { v[k].as_str().unwrap_or("") }

pub fn valid_domain(d: &str) -> bool { Regex::new(r"(?i)^[a-z0-9.-]+\.[a-z]{2,}$").unwrap().is_match(d) }
pub fn instance_for(user: &str) -> String {
    let clean: String = user.to_lowercase().chars().filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit()).take(20).collect();
    format!("hearth-{clean}")
}
pub fn free_port(used: &[u64], check_free: bool) -> u16 {
    for p in 47601u16..47700 { if !used.contains(&(p as u64)) && (!check_free || port_free(p)) { return p; } }
    47601
}

pub fn install(env: &Env, pr: &mut Prompt, o: &Opts) -> Result<Value> {
    banner(VERSION);
    say(&format!("{} I will set up your Hearth server. It takes about five minutes.", bold("Hello!")));
    gap();
    say(&format!("A {} is a computer that is always on, far away from you, like a robot that never sleeps. Hearth lives there, so your phone can talk to Claude any time, even when your laptop is off.", bold("server")));
    gap();
    say(&format!("You only need to answer a few easy questions. If you are not sure, just press {}: I pick the easy choice for you.", bold("Enter")));
    gap(); pr.pause("Press Enter to continue");

    // 0 ── is Hearth already here? Never overwrite someone else's
    let who = if env.is_root && env.prefix.is_none() { "hearth".to_string() } else { env.user.clone() };
    let here = find_instances(&env.opt());
    let mine = here.iter().find(|(_, m)| s(m, "user") == who).cloned();
    let instance = if let Some(i) = &o.instance { i.clone() } else if let Some((n, _)) = &mine { n.clone() } else if !here.is_empty() { instance_for(&who) } else { "hearth".into() };
    let existing = here.iter().find(|(n, _)| *n == instance).cloned();
    if !here.is_empty() && mine.is_none() && o.instance.is_none() {
        gap();
        let list: Vec<String> = here.iter().map(|(_, m)| format!("{} ({})", s(m, "user"), s(m, "url"))).collect();
        warn(&format!("Hearth is already installed on this server for {}.", bold(&list.join(", "))));
        say(&format!("I will not touch it. I set up a separate copy for {} next to it, with its own address, its own service and its own key.", bold(&who)));
        gap();
    }
    if let Some((_, m)) = &existing { if mine.is_none() && o.instance.is_some() { bail!("That Hearth belongs to {}, not to you. Choose another --instance name.", s(m, "user")); } }
    let install_dir = env.opt().join(&instance);
    let port: u16 = match &existing { Some((_, m)) => m["port"].as_u64().unwrap_or(47601) as u16, None => { let used: Vec<u64> = here.iter().filter_map(|(_, m)| m["port"].as_u64()).collect(); free_port(&used, env.prefix.is_none()) } };

    // 1 ── look around
    step(1, 6, "Looking around");
    let info = os_info();
    ok(&format!("This computer runs {}", info.name));
    if env.prefix.is_none() && std::env::consts::OS != "linux" { bail!("This installer is for Linux servers. On a Mac or Windows computer use the Hearth app itself."); }
    let docker_ok = have("docker") && std::process::Command::new("docker").args(["compose", "version"]).output().map(|r| r.status.success()).unwrap_or(false);
    if docker_ok { ok("Docker is installed"); } else { say(&dim("Docker is not installed (that is fine).")); }
    if !env.is_root && env.prefix.is_none() {
        if have("sudo") {
            say("I need your computer password once, so I can install things. Nothing is shown while you type it.");
            if !env.dry && !env.yes { let _ = std::process::Command::new("sudo").arg("-v").status(); }
            if !env.dry && !std::process::Command::new("sudo").args(["-n", "true"]).status().map(|s| s.success()).unwrap_or(false) { bail!("I could not get administrator rights. Try again, or run this as the root user."); }
            ok("I have the rights I need");
        } else { bail!("This needs administrator rights, but \"sudo\" is missing. Log in as root and run it again."); }
    }
    let ip = match (&o.public_ip, env.dry && env.prefix.is_none()) {
        (Some(ip), _) => ip.clone(),
        (None, true) => "203.0.113.7".to_string(),
        _ => std::env::var("HEARTH_PUBLIC_IP").ok().filter(|v| !v.is_empty()).unwrap_or_else(public_ip),
    };
    if !ip.is_empty() { ok(&format!("Your server can be found on the internet at {ip}")); } else { warn("I could not find your internet address. That is OK if you have your own domain name."); }
    let claude_already = have("claude");
    gap();

    // 2 ── how
    step(2, 6, "How should I install it?");
    let docker = match &o.mode {
        Some(m) => m == "docker",
        None => pr.choose("Where should Hearth live?", &[
            ("Directly on this server", "Simple and light. Hearth runs as a normal program that starts by itself."),
            ("Inside Docker", if docker_ok { "Neat and tidy: everything stays inside one box that is easy to remove." } else { "Docker is not installed on this server, so this will not work yet." }),
        ], 0) == 1,
    };
    if docker && !docker_ok && env.prefix.is_none() { bail!("Docker is not installed. Install it (see docs.docker.com/engine/install), or choose the first option."); }
    gap();

    // 3 ── address
    step(3, 6, "Choose an address");
    say(&format!("Your phone and laptop need an address to find the server. It has to be {} (a padlock, called HTTPS) so nobody can listen in.", bold("safe")));
    gap();
    let default_free = if ip.is_empty() { String::new() } else { format!("{}.sslip.io", ip.replace('.', "-")) };
    let addr_choice = match &o.address {
        Some(a) => a.clone(),
        None => {
            let free = format!("Get a free address for me{}", if default_free.is_empty() { String::new() } else { format!(" ({default_free})") });
            match pr.choose("Which address do you want?", &[
                (free.as_str(), "Works right away. No account or shopping needed. The server must be reachable from the internet."),
                ("I have my own domain name", "For example hearth.mywebsite.com. It must already point to this server."),
                ("No padlock, only my home network", "Only for testing. Others on the network could listen in. Not for the open internet."),
            ], if ip.is_empty() { 1 } else { 0 }) { 0 => "auto", 1 => "domain", _ => "none" }.to_string()
        }
    };
    let domain = match addr_choice.as_str() {
        "auto" => { if default_free.is_empty() { bail!("I could not find your internet address, so I cannot make a free one. Use your own domain instead."); } default_free.clone() }
        "none" => String::new(),
        other => {
            let d = if other == "domain" { pr.ask("Type your domain name", "") } else { other.to_string() };
            let d = Regex::new(r"^https?://").unwrap().replace(&d, "").to_string();
            let d = d.split('/').next().unwrap_or("").to_string();
            if !valid_domain(&d) { bail!("That does not look like a domain name."); }
            d
        }
    };
    if !domain.is_empty() { if let Some((_, m)) = find_instances(&env.opt()).iter().find(|(n, m)| *n != instance && s(m, "domain") == domain) { bail!("The address {domain} already belongs to the Hearth of {}. Choose another address.", s(m, "user")); } }
    let url = if domain.is_empty() { format!("http://{}:{port}", if ip.is_empty() { hostname() } else { ip.clone() }) } else { format!("https://{domain}") };
    ok(&format!("Address: {}", bold(&url)));
    if !domain.is_empty() { say(&dim("Ports 80 and 443 must be open to the internet (many hosting companies call this the \"firewall\"). I will open them on this server too.")); }
    let name = match &o.name { Some(n) => n.clone(), None => pr.ask("What should your server be called?", "My server") };
    gap();

    // 4 ── install
    step(4, 6, "Installing");
    let user = who.clone();
    let home = if env.prefix.is_some() { env.sys("/home").join(&user) } else if env.is_root { PathBuf::from("/home/hearth") } else { PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| format!("/home/{user}"))) };
    let cfg_dir = if docker { PathBuf::from("/data/config") } else { home.join(".config").join("hearth") };
    let projects = home.join("projects");
    let meta = json!({"instance": instance, "bin": bin_of(&instance), "mode": if docker {"docker"} else {"direct"}, "url": url, "domain": domain, "name": name, "user": user, "home": home.to_string_lossy(), "projects": projects.to_string_lossy(),
        "cfgDir": if docker { install_dir.join("data/config").to_string_lossy().to_string() } else { cfg_dir.to_string_lossy().to_string() }, "version": VERSION, "port": port});
    let res: Result<()> = (|| {
        let relay_dest = install_dir.join("hearth-relay");
        if !docker {
            if env.is_root && env.prefix.is_none() && !env.dry && !std::process::Command::new("id").arg("hearth").output().map(|r| r.status.success()).unwrap_or(false) {
                env.must("Create a user called \"hearth\"", "useradd", &["-m", "-s", "/bin/bash", "hearth"], &RunOpts::default())?;
            }
            env.mkdirs(&[&install_dir])?;
            acquire_relay(env, o, &relay_dest)?; ok(&format!("Copied Hearth to {}", install_dir.display()));
            env.copy_file(&env.self_exe, &install_dir.join("hearth-server"), 0o755)?;
            env.mkdirs(&[&projects, &cfg_dir])?;
            env.chown(&user, &[&projects, &cfg_dir]);
            env.chown(&user, &[&install_dir]);
            if !claude_already && env.prefix.is_none() { install_claude(env)?; } else { ok("Claude Code is already installed"); }
            env.write(&env.sys(&format!("/etc/systemd/system/{instance}.service")), &unit_text(&UnitArgs { user: &user, home: &home.to_string_lossy(), relay_bin: &relay_dest.to_string_lossy(), work_dir: &install_dir.to_string_lossy(), cfg_dir: &cfg_dir.to_string_lossy(), projects: &projects.to_string_lossy(), url: &url, name: &name, host: if domain.is_empty() { "0.0.0.0" } else { "127.0.0.1" }, port }), 0o644)?;
            if env.prefix.is_none() { env.must("Start Hearth, now and every time the server starts", "sh", &["-c", &format!("systemctl daemon-reload && systemctl enable --now {instance} && systemctl restart {instance}")], &RunOpts { sudo: true, ..Default::default() })?; }
            if !domain.is_empty() { setup_caddy(env, &info, &instance, &domain, &format!("127.0.0.1:{port}"))?; }
        } else {
            let data = install_dir.join("data");
            let dirs: Vec<PathBuf> = ["projects", "config", "claude", "caddy"].iter().map(|d| data.join(d)).collect();
            let mut all: Vec<&Path> = dirs.iter().map(|d| d.as_path()).collect(); all.push(&install_dir);
            env.mkdirs(&all)?;
            acquire_relay(env, o, &relay_dest)?; ok(&format!("Copied Hearth to {}", install_dir.display()));
            env.copy_file(&env.self_exe, &install_dir.join("hearth-server"), 0o755)?;
            let cj = data.join("claude.json");
            if !env.dry && std::fs::metadata(&cj).map(|m| m.len() == 0).unwrap_or(true) { env.write(&cj, "{}\n", 0o644)?; }
            env.chown("1000:1000", &[&data]);
            env.write(&install_dir.join("Dockerfile"), &dockerfile_text(port), 0o644)?;
            env.write(&install_dir.join("docker-compose.yml"), &compose_text(&url, &name, !domain.is_empty(), port), 0o644)?;
            if !domain.is_empty() { env.write(&install_dir.join("Caddyfile"), &caddy_text(&domain, &format!("hearth:{port}"), None), 0o644)?; }
            if !domain.is_empty() { open_firewall(env)?; }
            if env.prefix.is_none() { env.must("Build and start Hearth (the first time takes a few minutes)", "docker", &["compose", "up", "-d", "--build"], &RunOpts { sudo: true, cwd: Some(install_dir.clone()), ..Default::default() })?; }
        }
        env.write(&install_dir.join("install.json"), &serde_json::to_string_pretty(&meta)?, 0o644)?;
        env.write(&env.sys(&format!("/usr/local/bin/{}", bin_of(&instance))), &wrapper_text(&install_dir.to_string_lossy(), &instance), 0o755)?;
        Ok(())
    })();
    if let Err(e) = res {
        gap(); bad(&format!("Something went wrong: {e}"));
        say("Nothing is broken that cannot be fixed. Read the message above, fix that one thing, and run the installer again. It is safe to run it twice.");
        return Err(anyhow::anyhow!("{e}"));
    }

    // wait until it answers (TCP + /health)
    if env.live() {
        let mut up = false;
        for _ in 0..40 { if tcp_up(port) { up = true; break; } std::thread::sleep(std::time::Duration::from_millis(1500)); }
        if !up { bail!("Hearth did not start. Look at what it says with:  {}", if docker { format!("cd {} && sudo docker compose logs", install_dir.display()) } else { format!("sudo journalctl -u {instance} -n 30") }); }
        ok("Hearth is running");
    }

    // 5 ── Claude sign in
    step(5, 6, "Sign in to Claude");
    say("Claude Code needs to know who you are, so it can use your Claude account. You only do this once.");
    if env.live() && !signed_in(env, &meta, &install_dir) && !pr.yes {
        say(&format!("I will open Claude now. It shows a web link. Open the link on any device, sign in, and paste the code it gives you back here. When Claude is ready for a message, type  {}  and press Enter.", bold("/exit")));
        gap(); pr.pause("Press Enter to open Claude");
        if docker { let _ = std::process::Command::new("sudo").args(["docker", "compose", "run", "--rm", "-it", "hearth", "claude"]).current_dir(&install_dir).status(); }
        else if env.is_root { let _ = std::process::Command::new("su").args(["-", &user, "-c", "claude"]).status(); }
        else { let _ = std::process::Command::new("sudo").args(["-u", &user, "-H", "claude"]).status(); }
        if signed_in(env, &meta, &install_dir) { ok("You are signed in"); } else { warn(&format!("I could not see a sign-in yet. You can do it later with:  {}", if docker { format!("cd {} && sudo docker compose run --rm -it hearth claude", install_dir.display()) } else { format!("sudo -u {user} -H claude") })); }
    } else if env.live() && pr.yes && !signed_in(env, &meta, &install_dir) { warn(&format!("Not signed in yet. Run  hearth-server login  later (or: sudo -u {user} -H claude).")); }
    else { ok(if env.live() { "You are already signed in" } else { "Sign-in skipped (test run)" }); }

    // 6 ── connect devices
    step(6, 6, "Connect your laptop and phone");
    manage::show_code(env, &meta, true);
    Ok(meta)
}

pub fn hostname() -> String {
    std::fs::read_to_string("/etc/hostname").map(|s| s.trim().to_string()).ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "localhost".into())
}

pub fn signed_in(env: &Env, meta: &Value, install_dir: &Path) -> bool {
    let home = if s(meta, "mode") == "docker" { install_dir.join("data/claude") } else { PathBuf::from(s(meta, "home")).join(".claude") };
    env.sudo_test_file(&home.join(".credentials.json"))
}

/// Puts hearth-relay in place: given file, a matching one next to this program, or the release of this version.
pub fn acquire_relay(env: &Env, o: &Opts, dest: &Path) -> Result<()> {
    if let Some(b) = &o.relay_bin { return env.copy_file(b, dest, 0o755); }
    if let Some(sib) = env.self_exe.parent().map(|d| d.join("hearth-relay")) {
        if sib.is_file() && update::program_version(&sib).as_deref() == Some(VERSION) { return env.copy_file(&sib, dest, 0o755); }
    }
    if env.prefix.is_some() && !env.dry { warn("Test run: no hearth-relay was copied (give --relay-bin)."); return Ok(()); }
    update::install_relay(env, &o.source.clone().unwrap_or_else(Source::github), &format!("v{VERSION}"), dest)
}

fn install_claude(env: &Env) -> Result<()> {
    if have("npm") { env.must("Install Claude Code (this is the part that does the thinking)", "npm", &["install", "-g", "@anthropic-ai/claude-code"], &RunOpts { sudo: true, ..Default::default() })?; }
    else if env.dry { env.run("Install Claude Code", "npm", &["install", "-g", "@anthropic-ai/claude-code"], &RunOpts::default()); }
    else { warn("Claude Code is not installed and npm is missing. Install it (see docs.claude.com/claude-code), then run: hearth-server login"); }
    Ok(())
}

pub fn setup_caddy(env: &Env, info: &OsInfo, instance: &str, domain: &str, upstream: &str) -> Result<()> {
    let sudo = RunOpts { sudo: true, ..Default::default() };
    if !have("caddy") && env.prefix.is_none() {
        match info.pm {
            Some("apt") => { env.must("Install Caddy (it makes the safe padlock)", "apt-get", &["install", "-y", "caddy"], &sudo)?; }
            Some("dnf") => { env.must("Install Caddy (it makes the safe padlock)", "dnf", &["install", "-y", "caddy"], &sudo)?; }
            _ => bail!("I do not know how to install Caddy on this system. Install it from caddyserver.com, or choose the Docker option"),
        }
    } else { ok("Caddy is already installed"); }
    let dir = env.sys(&format!("/etc/caddy/{instance}.d"));
    env.write(&dir.join("00-hearth.caddy"), "# extra routes for this Hearth (the support desk adds one here)\n", 0o644)?;
    env.write(&env.sys(&format!("/etc/caddy/{instance}.caddy")), &caddy_text(domain, upstream, Some(&format!("/etc/caddy/{instance}.d"))), 0o644)?;
    let main = env.sys("/etc/caddy/Caddyfile");
    let line = format!("import /etc/caddy/{instance}.caddy");
    let has = std::fs::read_to_string(&main).unwrap_or_default();
    if !has.lines().any(|l| l.trim() == line) {
        let base = if has.is_empty() { String::new() } else { format!("{}\n\n", has.trim_end()) };
        env.write(&main, &format!("{base}{line}\n"), 0o644)?;
    }
    if env.live() {   // Caddy sometimes stops by itself (older versions); make the system start it again within seconds
        env.write(&env.sys("/etc/systemd/system/caddy.service.d/hearth.conf"), "[Service]\nRestart=always\nRestartSec=3\n", 0o644)?;
        let out = std::process::Command::new("caddy").arg("version").output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
        if let Some(c) = Regex::new(r"v?(\d+)\.(\d+)").unwrap().captures(&out) {
            let (a, b): (u32, u32) = (c[1].parse().unwrap_or(9), c[2].parse().unwrap_or(9));
            if a < 2 || (a == 2 && b < 7) { warn(&format!("Your Caddy is old ({}). It can crash now and then; Hearth restarts it automatically. Updating Caddy (caddyserver.com/docs/install) is recommended.", &c[0])); }
        }
    }
    open_firewall(env)?;
    if env.prefix.is_none() { env.must("Turn on the padlock", "sh", &["-c", "systemctl enable caddy && systemctl restart caddy"], &sudo)?; }
    Ok(())
}

pub fn open_firewall(env: &Env) -> Result<()> {
    if !env.live() { return Ok(()); }
    let sudo = RunOpts { sudo: true, ..Default::default() };
    let label = "Open the doors for the internet (ports 80 and 443)";
    let ufw_active = have("ufw") && { let mut full = env.sudo_prefix(); full.extend(["ufw".into(), "status".into()]); std::process::Command::new(&full[0]).args(&full[1..]).output().map(|o| String::from_utf8_lossy(&o.stdout).contains("Status: active")).unwrap_or(false) };
    if ufw_active { env.must(label, "sh", &["-c", "ufw allow 80/tcp && ufw allow 443/tcp"], &sudo)?; }
    else if have("firewall-cmd") { env.run(label, "sh", &["-c", "firewall-cmd --permanent --add-service=http --add-service=https && firewall-cmd --reload"], &sudo); }
    Ok(())
}
