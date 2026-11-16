//! Claude Code hooks in ~/.claude/settings.json (same logic as desktop/tauri/src/sys.rs hooks_*), plus the small
//! programs that used to be relay/cm_hook.py (`hearth-server hook`) and relay/call_me.py (`hearth-server call`).
use anyhow::{anyhow, bail, Result};
use regex::Regex;
use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// (event, matcher, timeout seconds)
pub const HOOK_DEFS: [(&str, &str, u32); 3] = [("Stop", "", 10), ("PreToolUse", "AskUserQuestion", 10), ("PermissionRequest", "", 45)];

pub fn settings_path(home: &Path) -> PathBuf { home.join(".claude").join("settings.json") }

/// A hook that Hearth wrote: the curl to the relay, the old Python cm_hook.py, or `hearth-server hook`.
pub fn is_our_hook(h: &Value) -> bool {
    let c = h["command"].as_str().unwrap_or("");
    Regex::new(r"127\.0\.0\.1:\d+/hook").unwrap().is_match(c) || c.contains("cm_hook.py") || Regex::new(r#"hearth-server\S*"?\s+hook\b"#).unwrap().is_match(c)
}
fn has_ours(groups: &Value) -> bool {
    groups.as_array().map(|g| g.iter().any(|g| g["hooks"].as_array().map(|h| h.iter().any(is_our_hook)).unwrap_or(false))).unwrap_or(false)
}
/// Reads the settings. A missing file is an empty object; a file that is not valid JSON is an error (we never overwrite what we cannot read).
fn read_settings(home: &Path) -> Result<Value> {
    let f = settings_path(home);
    match std::fs::read_to_string(&f) {
        Err(_) => Ok(json!({})),
        Ok(t) if t.trim().is_empty() => Ok(json!({})),
        Ok(t) => { let v: Value = serde_json::from_str(&t).map_err(|e| anyhow!("{} is not valid JSON ({e}); fix it first, nothing was changed", f.display()))?; if v.is_object() { Ok(v) } else { bail!("{} is not a JSON object; nothing was changed", f.display()) } }
    }
}
fn write_settings(home: &Path, v: &Value) -> Result<()> {
    let f = settings_path(home);
    if let Some(d) = f.parent() { std::fs::create_dir_all(d)?; }
    std::fs::write(&f, serde_json::to_string_pretty(v)? + "\n")?;
    Ok(())
}

pub fn status(home: &Path) -> bool {
    read_settings(home).map(|s| HOOK_DEFS.iter().all(|(ev, _, _)| has_ours(&s["hooks"][*ev]))).unwrap_or(false)
}

/// What the hook runs.
pub enum HookCommand { /// curl straight to the relay (what the desktop app writes)
    Curl, /// `<this program> hook` (the port of cm_hook.py)
    Program(PathBuf) }

pub fn command_line(port: u16, how: &HookCommand) -> String {
    match how {
        HookCommand::Curl => format!("curl -s -m 40 -X POST -H \"Content-Type: application/json\" --data-binary @- http://127.0.0.1:{port}/hook"),
        HookCommand::Program(p) => format!("\"{}\" hook --port {port}", p.display()),
    }
}

/// Adds the three hooks (backs settings.json up first, safe to re-run). Returns true if something was added.
pub fn install(home: &Path, port: u16, how: &HookCommand) -> Result<bool> {
    let mut data = read_settings(home)?;
    let f = settings_path(home);
    if f.exists() { let _ = std::fs::copy(&f, f.with_extension("json.bak-hearth")); }
    let cmdline = command_line(port, how);
    if !data["hooks"].is_object() { data["hooks"] = json!({}); }
    let mut changed = false;
    for (ev, matcher, timeout) in HOOK_DEFS {
        if !data["hooks"][ev].is_array() { data["hooks"][ev] = json!([]); }
        if !has_ours(&data["hooks"][ev]) {
            data["hooks"][ev].as_array_mut().unwrap().push(json!({"matcher": matcher, "hooks": [{"type": "command", "command": cmdline, "timeout": timeout}]}));
            changed = true;
        }
    }
    write_settings(home, &data)?;
    Ok(changed)
}

/// Takes our hooks out again and leaves everybody else's alone.
pub fn remove(home: &Path) -> Result<bool> {
    let mut data = read_settings(home)?;
    let f = settings_path(home);
    if !f.exists() { return Ok(false); }
    let _ = std::fs::copy(&f, f.with_extension("json.bak-hearth"));
    let mut changed = false;
    if let Some(hooks) = data.get_mut("hooks").and_then(|h| h.as_object_mut()) {
        let keys: Vec<String> = hooks.keys().cloned().collect();
        for ev in keys {
            let groups: Vec<Value> = hooks[&ev].as_array().cloned().unwrap_or_default().into_iter().map(|mut g| {
                let all = g["hooks"].as_array().cloned().unwrap_or_default();
                let kept: Vec<Value> = all.iter().filter(|x| !is_our_hook(x)).cloned().collect();
                if kept.len() != all.len() { changed = true; }
                g["hooks"] = Value::Array(kept); g
            }).filter(|g| g["hooks"].as_array().map(|a| !a.is_empty()).unwrap_or(false)).collect();
            if groups.is_empty() { hooks.remove(&ev); } else { hooks.insert(ev, Value::Array(groups)); }
        }
        if hooks.is_empty() { data.as_object_mut().unwrap().remove("hooks"); }
    }
    write_settings(home, &data)?;
    Ok(changed)
}

// ───────────── relay/cm_hook.py ─────────────
/// What the hook prints, given what the hook was asked and what the relay answered (None: print nothing).
pub fn hook_output(raw: &str, answer: &Value) -> Option<Value> {
    if answer.get("hookSpecificOutput").is_some() { return Some(answer.clone()); }   // the relay answers with the finished hook output
    let name = serde_json::from_str::<Value>(raw).ok().and_then(|v| v["hook_event_name"].as_str().map(String::from))?;
    let decision = answer["decision"].as_str()?;
    if name == "PermissionRequest" && (decision == "allow" || decision == "deny") {
        let mut d = json!({"behavior": decision});
        if decision == "deny" { d["message"] = json!("Denied from your phone"); }
        return Some(json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": d}}));
    }
    None
}
/// `hearth-server hook`: reads the hook JSON on stdin, asks the relay, prints the answer. The relay not running: stay out of the way.
pub fn hook_main(port: u16) -> i32 {
    let mut raw = String::new();
    let _ = std::io::stdin().read_to_string(&mut raw);
    let answer: Option<Value> = reqwest::blocking::Client::builder().timeout(Duration::from_secs(40)).build().ok()
        .and_then(|c| c.post(format!("http://127.0.0.1:{port}/hook")).header("Content-Type", "application/json").body(raw.clone()).send().ok())
        .and_then(|r| r.json().ok());
    if let Some(out) = answer.and_then(|a| hook_output(&raw, &a)) { println!("{out}"); }
    0
}

// ───────────── relay/call_me.py ─────────────
pub fn call_message(listeners: u64) -> (i32, String) {
    if listeners == 0 { return (2, "Relay is up but no phone or desktop is listening. Turn on Call me in the app.".into()); }
    (0, format!("Calling ({listeners} device{} listening).", if listeners != 1 { "s" } else { "" }))
}
/// `hearth-server call "text"`: rings the phone (and the desktop, if enabled) through the relay.
pub fn call_me(text: &[String], port: u16) -> i32 {
    let text = if text.is_empty() { "Claude is calling".to_string() } else { text.join(" ") };
    let cwd = std::env::current_dir().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
    let r = reqwest::blocking::Client::builder().timeout(Duration::from_secs(5)).build().map_err(|e| e.to_string())
        .and_then(|c| c.post(format!("http://127.0.0.1:{port}/call")).json(&json!({"text": text, "cwd": cwd})).send().map_err(|e| e.to_string()))
        .and_then(|r| r.json::<Value>().map_err(|e| e.to_string()));
    match r {
        Err(e) => { println!("Couldn't reach the relay ({e}). Is it running? Try: systemctl --user restart hearth-node"); 1 }
        Ok(j) => { let (code, msg) = call_message(j["listeners"].as_u64().unwrap_or(0)); println!("{msg}"); code }
    }
}
