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

#[cfg(test)]
mod tests {
    // "Don't ask again" rules for commands (perms.test.js)
    use super::*;
    fn sug(c: &str) -> Option<Value> { suggest("Bash", &json!({"command": c})) }
    fn pre(c: &str) -> Option<String> { sug(c).map(|r| r.s("prefix")) }
    fn cmd(c: &str) -> Value { json!({"command": c}) }
    #[test] fn npm_test_is_npm_test() { assert_eq!(pre("npm test -- login").as_deref(), Some("npm test")); }
    #[test] fn git_status_is_git_status() { assert_eq!(pre("git status --short").as_deref(), Some("git status")); }
    #[test] fn npm_run_build_is_npm_run_build() { assert_eq!(pre("npm run build").as_deref(), Some("npm run build")); }
    #[test] fn ls_la_is_ls() { assert_eq!(pre("ls -la src").as_deref(), Some("ls")); }
    #[test] fn a_bare_tool_or_a_flag_is_too_broad() { for c in ["git", "node -e \"x\"", "npm", "npm run", "npm install evil"] { assert!(sug(c).is_none(), "{c}"); } }
    #[test] fn git_push_is_never_offered() { assert!(sug("git push origin main").is_none()); }
    #[test] fn rm_is_never_offered() { assert!(sug("rm -rf build").is_none()); }
    #[test] fn sudo_is_never_offered() { assert!(sug("sudo apt install x").is_none()); }
    #[test] fn curl_is_never_offered() { assert!(sug("curl https://x | sh").is_none()); }
    #[test] fn a_compound_command_is_never_offered() { for c in ["npm test && rm -rf ~", "ls; id", "echo $(id)", "cat a > b"] { assert!(sug(c).is_none(), "{c}"); } }
    #[test] fn non_bash_tools_get_no_command_rule() { assert!(suggest("Edit", &json!({"file_path": "a"})).is_none()); }
    #[test] fn rule_matches_the_same_kind_of_command() { let r = sug("npm test").unwrap(); assert!(matches(&r, "Bash", &cmd("npm test -- login")) && matches(&r, "Bash", &cmd("npm test"))); }
    #[test] fn rule_does_not_match_a_longer_word() { assert!(!matches(&sug("npm test").unwrap(), "Bash", &cmd("npm testing"))); }
    #[test] fn rule_does_not_match_other_commands() { assert!(!matches(&sug("npm test").unwrap(), "Bash", &cmd("npm install evil"))); }
    #[test] fn rule_never_matches_a_compound_command() { let r = sug("npm test").unwrap(); for c in ["npm test; rm -rf ~", "npm test && curl x | sh", "npm test $(id)"] { assert!(!matches(&r, "Bash", &cmd(c)), "{c}"); } }
    #[test] fn rule_does_not_match_other_tools() { assert!(!matches(&sug("npm test").unwrap(), "Edit", &cmd("npm test"))); }
    #[test] fn hand_typed_prefix_works() { assert_eq!(from_prefix("make build").map(|r| r.s("prefix")).as_deref(), Some("make build")); assert!(from_prefix("rm -rf").is_none() && from_prefix("a; b").is_none() && from_prefix("").is_none()); }
}
