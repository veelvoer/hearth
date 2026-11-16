//! A laptop or desktop: the relay as a systemd *user* service (relay/install-node.sh), the migration from the early
//! "claude-meter" names, and `update --local` (relay/update-local.sh).
use crate::sysx::{copy_dir_all, Env};
use crate::ui::*;
use crate::update::{self, Source};
use crate::{texts, voice, VERSION};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Paths { pub home: PathBuf, pub dest: PathBuf, pub unit: PathBuf, pub cfg: PathBuf }
pub fn paths(home: &Path) -> Paths {
    Paths { home: home.into(), dest: home.join(".local/share/hearth-node"), unit: home.join(".config/systemd/user/hearth-node.service"), cfg: home.join(".config/hearth-node") }
}

#[derive(Default, Clone)]
pub struct LocalOpts {
    /// A hearth-relay to install (else: the build in --repo if it has this version, else the release).
    pub relay_bin: Option<PathBuf>,
    pub repo: Option<PathBuf>,
    /// Talk to systemd (off in tests and in --dry-run).
    pub systemctl: bool,
    pub source: Option<Source>,
}

#[derive(Debug, Default, PartialEq)]
pub struct Migration { pub copied_config: bool, pub retired_service: bool, pub linked: Vec<String>, pub removed_scripts: Vec<String> }

fn user_systemctl(on: bool, args: &[&str]) -> bool {
    on && Command::new("systemctl").arg("--user").args(args).output().map(|o| o.status.success()).unwrap_or(false)
}

/// Early testers had the old names: keep their pairing, retire the old service, keep the optional voice engine reachable,
/// and drop the Node relay's program files (the Rust hearth-relay replaces them).
pub fn migrate(home: &Path, systemctl: bool) -> Result<Migration> {
    let p = paths(home);
    let mut m = Migration::default();
    let old_cfg = home.join(".config/claude-meter-node");
    if old_cfg.is_dir() && !p.cfg.join("relay.json").exists() { std::fs::create_dir_all(&p.cfg)?; copy_dir_all(&old_cfg, &p.cfg)?; m.copied_config = true; }
    let old_unit = home.join(".config/systemd/user/claude-meter-node.service");
    if old_unit.is_file() {
        user_systemctl(systemctl, &["disable", "--now", "claude-meter-node.service"]);
        let _ = std::fs::remove_file(&old_unit);
        let _ = std::fs::remove_dir_all(home.join(".local/share/claude-meter-node"));
        m.retired_service = true;
    }
    for (old, new) in [(".local/share/claude-meter-relay", ".local/share/hearth-relay"), (".config/claude-meter-relay", ".config/hearth-relay")] {
        let (o, n) = (home.join(old), home.join(new));
        if o.exists() && std::fs::symlink_metadata(&n).is_err() { std::os::unix::fs::symlink(&o, &n)?; m.linked.push(new.to_string()); }
    }
    if let Ok(rd) = std::fs::read_dir(&p.dest) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if n.ends_with(".js") { let _ = std::fs::remove_file(e.path()); m.removed_scripts.push(n); }
        }
    }
    m.removed_scripts.sort();
    Ok(m)
}

fn pick_relay(o: &LocalOpts) -> Option<PathBuf> {
    if let Some(b) = &o.relay_bin { return Some(b.clone()); }
    o.repo.as_ref().map(|r| update::repo_binary(r)).filter(|b| b.is_file() && update::program_version(b).as_deref() == Some(VERSION))
}

/// The arguments for the optional voice engine, when its token exists.
pub fn seed_voice_args(home: &Path) -> Vec<String> {
    let t = home.join(".config/hearth-relay/token");
    if t.is_file() { let t = t.to_string_lossy().to_string(); vec!["--seed-token-file".into(), t.clone(), "--voice-port".into(), "47602".into(), "--voice-token-file".into(), t] } else { vec![] }
}

/// install-node.sh: the relay as a user service on this computer. Returns the unit file text.
pub fn install(home: &Path, o: &LocalOpts, dry: bool) -> Result<String> {
    let p = paths(home);
    let env = Env::user_level(dry);
    if !dry { std::fs::create_dir_all(&p.dest)?; std::fs::create_dir_all(&p.cfg)?; if let Some(d) = p.unit.parent() { std::fs::create_dir_all(d)?; } }
    if !dry { migrate(home, o.systemctl)?; }
    let relay = p.dest.join("hearth-relay");
    match pick_relay(o) {
        Some(b) => env.copy_file(&b, &relay, 0o755)?,
        None => update::install_relay(&env, &o.source.clone().unwrap_or_else(Source::github), &format!("v{VERSION}"), &relay)?,
    }
    let unit = texts::local_unit_text(&relay.to_string_lossy(), &home.to_string_lossy(), &p.cfg.to_string_lossy(), &seed_voice_args(home));
    env.write(&p.unit, &unit, 0o644)?;
    if o.systemctl && !dry {
        user_systemctl(true, &["daemon-reload"]);
        user_systemctl(true, &["enable", "hearth-node.service"]);
        user_systemctl(true, &["restart", "hearth-node.service"]);
        let _ = Command::new("loginctl").args(["enable-linger", &env.user]).output();
        std::thread::sleep(std::time::Duration::from_millis(1500));
        if user_systemctl(true, &["is-active", "hearth-node.service"]) { ok("Hearth is running (hearth-node)"); } else { warn("hearth-node did not report active. Look at: journalctl --user -u hearth-node -n 30"); }
    }
    Ok(unit)
}

/// update-local.sh: bring this computer's services up to date with the installed/project files.
pub fn update_local(home: &Path, o: &LocalOpts) -> Result<String> {
    let p = paths(home);
    let mut done: Vec<&str> = vec![];
    if p.unit.is_file() { install(home, o, false)?; done.push("relay"); }
    let py = home.join(".local/share/hearth-relay");
    if py.is_dir() {   // the optional voice engine
        voice::refresh_engine_files(&py)?;
        user_systemctl(o.systemctl, &["try-restart", "hearth-relay.service", "hearth-nl-tts.service"]);
        done.push("voice engine");
    }
    Ok(format!("ok ({})", if done.is_empty() { "nothing installed here".to_string() } else { done.join(", ") }))
}
