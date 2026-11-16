//! Command line: the same words as server/hearth-server.js, plus hooks, voice, local, migrate, hook, call.
use crate::install::{self, Opts};
use crate::manage::{self, Ctx};
use crate::sysx::{current_user, is_root, Env};
use crate::ui::*;
use crate::update::Source;
use crate::{hooks, local, voice, DEFAULT_PORT, VERSION};
use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "hearth-server", about = "Hearth server installer and manager", disable_version_flag = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Cmd>,
    /// Print the version and exit
    #[arg(long, short = 'V', global = true)]
    pub version: bool,
    /// Write into this folder instead of the real system (for tests)
    #[arg(long, global = true)]
    pub prefix: Option<PathBuf>,
    /// Show what would be done without doing it
    #[arg(long, global = true)]
    pub dry_run: bool,
    /// Do not ask questions, take the easy choice
    #[arg(long, global = true)]
    pub yes: bool,
    /// Which Hearth on this server (one server can hold one per person)
    #[arg(long, global = true)]
    pub instance: Option<String>,
    /// direct | docker
    #[arg(long, global = true)]
    pub mode: Option<String>,
    /// auto | none | <domain>
    #[arg(long, global = true)]
    pub address: Option<String>,
    /// What the server is called
    #[arg(long, global = true)]
    pub name: Option<String>,
    /// Use this internet address instead of looking it up
    #[arg(long, global = true)]
    pub public_ip: Option<String>,
    /// Install this hearth-relay file instead of downloading the release
    #[arg(long, global = true)]
    pub relay_bin: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// Set up a server (asks simple questions)
    Install,
    /// Show the address, a fresh code and the link for a new device
    Code,
    /// Same as code
    Link,
    Status,
    Start,
    Stop,
    Restart,
    Logs,
    /// Get the newest hearth-relay and hearth-server from the release (sha256 checked). --local: this computer's own services
    Update {
        #[arg(long)]
        local: bool,
        /// Also when the version is not newer
        #[arg(long)]
        force: bool,
        /// A checkout of the project (for --local: use its build of hearth-relay)
        #[arg(long)]
        repo: Option<PathBuf>,
    },
    Uninstall,
    /// Sign in to Claude as the Hearth user
    Login,
    /// Claude Code hooks in ~/.claude/settings.json
    Hooks {
        #[command(subcommand)]
        action: HooksCmd,
    },
    /// (called by Claude Code) forwards a hook to the relay
    Hook {
        #[arg(long)]
        port: Option<u16>,
    },
    /// Ring your phone through the relay
    Call {
        text: Vec<String>,
    },
    /// The optional local voice engine (Python, third-party models)
    Voice {
        #[command(subcommand)]
        action: VoiceCmd,
    },
    /// This computer's relay as a user service (the laptop install)
    Local {
        #[command(subcommand)]
        action: LocalCmd,
    },
    /// Move an early install (claude-meter-*) to the Hearth names
    Migrate,
}
#[derive(Subcommand)]
pub enum HooksCmd {
    Install {
        #[arg(long)]
        port: Option<u16>,
        /// Write `hearth-server hook` instead of a curl command
        #[arg(long)]
        program: bool,
    },
    Remove,
    Status,
}
#[derive(Subcommand)]
pub enum VoiceCmd {
    /// Service + engines (venv, packages, models)
    Install {
        /// Only the service, no packages or models
        #[arg(long)]
        service_only: bool,
        /// Also the Dutch XTTS-v2 voice (non-commercial licence)
        #[arg(long)]
        dutch: bool,
    },
}
#[derive(Subcommand)]
pub enum LocalCmd {
    Install {
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Do not talk to systemd (write files only)
        #[arg(long)]
        no_systemctl: bool,
    },
}

fn home() -> PathBuf { std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/root")) }
fn port_of(p: Option<u16>) -> u16 { p.or_else(|| std::env::var("CM_PORT").ok().and_then(|v| v.parse().ok())).unwrap_or(DEFAULT_PORT) }

pub fn run(cli: Cli) -> Result<i32> {
    if cli.version { println!("{VERSION}"); return Ok(0); }
    let mut env = Env::real();
    env.prefix = cli.prefix.clone(); env.dry = cli.dry_run; env.yes = cli.yes; env.is_root = is_root(); env.user = current_user();
    let mut pr = Prompt::stdin(cli.yes);
    let cmd = cli.command.unwrap_or(Cmd::Install);
    let opts = Opts { mode: cli.mode.clone(), address: cli.address.clone(), name: cli.name.clone(), instance: cli.instance.clone(), public_ip: cli.public_ip.clone(), relay_bin: cli.relay_bin.clone(), source: None };
    match cmd {
        Cmd::Install => { install::install(&env, &mut pr, &opts)?; Ok(0) }
        Cmd::Hook { port } => Ok(hooks::hook_main(port_of(port))),
        Cmd::Call { text } => Ok(hooks::call_me(&text, port_of(None))),
        Cmd::Hooks { action } => {
            let h = home();
            match action {
                HooksCmd::Install { port, program } => {
                    let how = if program { hooks::HookCommand::Program(std::env::current_exe()?) } else { hooks::HookCommand::Curl };
                    let added = hooks::install(&h, port_of(port), &how)?;
                    ok(&format!("{} in {} (backup: settings.json.bak-hearth). The relay ignores them unless \"Call me\" is switched on in the app.", if added { "Hooks installed" } else { "Hooks were already installed" }, hooks::settings_path(&h).display()));
                }
                HooksCmd::Remove => { let r = hooks::remove(&h)?; ok(if r { "Hearth hooks removed." } else { "There were no Hearth hooks." }); }
                HooksCmd::Status => { let s = hooks::status(&h); println!("{}", if s { "installed" } else { "not installed" }); return Ok(if s { 0 } else { 1 }); }
            }
            Ok(0)
        }
        Cmd::Voice { action: VoiceCmd::Install { service_only, dutch } } => { voice::voice_install(&env, &home(), &mut pr, dutch, !service_only)?; println!("Next: hearth-server hooks install  (lets Claude Code tell the relay when it finishes or needs you)"); Ok(0) }
        Cmd::Local { action: LocalCmd::Install { repo, no_systemctl } } => {
            let o = local::LocalOpts { relay_bin: cli.relay_bin.clone(), repo, systemctl: !no_systemctl && !env.dry, source: None };
            local::install(&home(), &o, env.dry)?; ok("Hearth relay installed as a user service (hearth-node)."); Ok(0)
        }
        Cmd::Migrate => {
            let m = local::migrate(&home(), !env.dry)?;
            ok(&format!("Config copied: {} · old service retired: {} · links made: {} · old scripts removed: {}", m.copied_config, m.retired_service, m.linked.len(), m.removed_scripts.len())); Ok(0)
        }
        Cmd::Update { local: true, repo, .. } => {
            let o = local::LocalOpts { relay_bin: cli.relay_bin.clone(), repo, systemctl: !env.dry, source: None };
            println!("{}", local::update_local(&home(), &o)?); Ok(0)
        }
        other => {
            let Some((instance, meta)) = manage::pick_instance(&env, cli.instance.as_deref()) else { bad("Hearth is not installed here yet. Run the installer first."); return Ok(1) };
            let install_dir = env.opt().join(&instance);
            let port = meta["port"].as_u64().map(|p| p as u16).unwrap_or(DEFAULT_PORT);
            let c = Ctx { env: &env, instance, meta, install_dir, port };
            match other {
                Cmd::Code | Cmd::Link => { banner(VERSION); manage::show_code(&env, &c.meta, false); }
                Cmd::Status => manage::status(&c),
                Cmd::Start => manage::service_cmd(&c, "start")?,
                Cmd::Stop => manage::service_cmd(&c, "stop")?,
                Cmd::Restart => manage::service_cmd(&c, "restart")?,
                Cmd::Logs => manage::logs(&c),
                Cmd::Update { force, .. } => manage::update(&c, &Source::github(), force)?,
                Cmd::Uninstall => manage::uninstall(&c, &mut pr)?,
                Cmd::Login => {
                    let user = c.meta["user"].as_str().unwrap_or("hearth").to_string();
                    let st = if c.meta["mode"] == "docker" { std::process::Command::new("sudo").args(["docker", "compose", "run", "--rm", "-it", "hearth", "claude"]).current_dir(&c.install_dir).status() }
                        else if env.is_root { std::process::Command::new("su").args(["-", &user, "-c", "claude"]).status() } else { std::process::Command::new("sudo").args(["-u", &user, "-H", "claude"]).status() };
                    if !st.map(|s| s.success()).unwrap_or(false) { bail!("Claude did not finish"); }
                }
                _ => {}
            }
            Ok(0)
        }
    }
}
