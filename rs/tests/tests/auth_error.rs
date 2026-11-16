// When Claude's login has expired, the chat says so in plain words (and how to fix it) instead of "Claude reported an error".
use hearth_tests::*;
use serde_json::{json, Value};

const CLAUDE: &str = r#"#!/usr/bin/env python3
import sys, os, json
t = sys.stdin.read()
print(json.dumps({'type': 'system', 'subtype': 'init', 'session_id': 'auth-%d' % os.getpid()}))
print(json.dumps({'type': 'result', 'is_error': True, 'result': 'Failed to authenticate: OAuth session expired and could not be refreshed'}))
"#;

#[test]
fn auth_error() {
    let env = Env::new("auth");
    std::fs::create_dir_all(env.proj.join("app")).unwrap();
    env.stub("claude", CLAUDE);
    let relay = Relay::start(&env, Opts::new().args(&["--name", "My server"]));
    let a = relay.api(); let tok = relay.token.clone();
    let mut c = Checks::new();
    let r = a.t("POST", "/sessions/new/send", &tok, Some(json!({"text": "hi", "cwd": env.proj.join("app").to_string_lossy()})));
    let done = r.text.lines().filter(|l| !l.is_empty()).filter_map(|l| serde_json::from_str::<Value>(l).ok()).find(|e| e["t"] == "done");
    c.ok_x("1 the done event is an error with a clear message", done.as_ref().map(|d| d["error"] == true && re("logged out on My server", &jstr(&d["text"])) && re("claude login", &jstr(&d["text"]))).unwrap_or(false), done.clone().unwrap_or(json!({})).to_string().chars().take(200).collect::<String>());
    c.ok("2 it does not show the raw technical message", done.as_ref().map(|d| !re("OAuth session expired", &jstr(&d["text"]))).unwrap_or(false));
    c.finish();
}
