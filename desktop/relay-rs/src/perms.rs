//! "Don't ask again" for commands: rules that allow a Bash command prefix ("npm test"). Port of perms.js.
use crate::util::*;
use serde_json::{json, Value};

fn shell_meta(s: &str) -> bool { s.contains(|c| ";&|`<>\n\r".contains(c)) || s.contains("$(") || s.contains("${") }
const NEVER: [&str; 39] = ["rm", "sudo", "su", "doas", "dd", "mkfs", "chmod", "chown", "chgrp", "mv", "shutdown", "reboot", "halt", "poweroff", "kill", "killall", "pkill", "eval", "exec", "curl", "wget", "ssh", "scp", "sh", "bash", "zsh", "fish", "xargs", "find", "env", "nohup", "crontab", "passwd", "iptables", "ufw", "mount", "umount", "truncate", "shred"];
const TWO: [&str; 31] = ["git", "npm", "pnpm", "yarn", "npx", "bun", "docker", "podman", "cargo", "go", "pip", "pip3", "python", "python3", "node", "systemctl", "journalctl", "kubectl", "make", "gradle", "dotnet", "gh", "apt", "brew", "flutter", "rustup", "composer", "php", "ruby", "bundle", "rake"];
const RUNNERS: [&str; 4] = ["npm", "pnpm", "yarn", "bun"];
const RISKY: [&str; 25] = ["push", "reset", "clean", "rebase", "checkout", "rm", "publish", "remove", "uninstall", "install", "add", "exec", "stop", "restart", "disable", "delete", "prune", "kill", "rmi", "system", "force", "config", "login", "logout", "unpublish"];
fn base_of(t: &str) -> &str { t.rsplit('/').next().unwrap_or(t) }

/// The rule that would be offered for this tool call, or None when none is safe to offer.
pub fn suggest(tool: &str, input: &Value) -> Option<Value> {
    if tool != "Bash" { return None; }
    let cmd = input.s("command"); let cmd = cmd.trim();
    if cmd.is_empty() || shell_meta(cmd) { return None; }
    let t: Vec<&str> = cmd.split_whitespace().collect(); let base = base_of(t[0]);
    if NEVER.contains(&base) || t[0].starts_with('/') || t[0].starts_with('~') || t[0].starts_with("..") { return None; }
    let risky = |s: &str| RISKY.contains(&s);
    let mut prefix = t[0].to_string();
    if RUNNERS.contains(&base) {
        if t.get(1) == Some(&"run") && t.get(2).map(|x| !x.starts_with('-')).unwrap_or(false) { prefix = t[..3].join(" "); }
        else if let Some(s) = t.get(1).filter(|s| **s != "run" && !s.starts_with('-') && !risky(s)) { prefix = format!("{} {}", t[0], s); }
        else { return None; }
    } else if TWO.contains(&base) {
        let s = t.get(1)?; if s.starts_with('-') || risky(s) { return None; }
        prefix = format!("{} {}", t[0], s);
    }
    Some(json!({"tool": "Bash", "prefix": prefix, "label": format!("{prefix} …")}))
}
pub fn matches(rule: &Value, tool: &str, input: &Value) -> bool {
    if rule.s("tool") != tool || tool != "Bash" || rule.s("prefix").is_empty() { return false; }
    let cmd = input.s("command"); let cmd = cmd.trim();
    if cmd.is_empty() || shell_meta(cmd) { return false; }
    let p = rule.s("prefix"); cmd == p || cmd.starts_with(&format!("{p} "))
}
pub fn any_match(rules: &[Value], tool: &str, input: &Value) -> bool { rules.iter().any(|r| matches(r, tool, input)) }
/// A rule typed by hand: only plain command prefixes.
pub fn from_prefix(text: &str) -> Option<Value> {
    let p = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if p.is_empty() || p.chars().count() > 80 || shell_meta(&p) || NEVER.contains(&base_of(p.split(' ').next().unwrap_or(""))) { return None; }
    Some(json!({"tool": "Bash", "prefix": p, "label": format!("{p} …")}))
}
