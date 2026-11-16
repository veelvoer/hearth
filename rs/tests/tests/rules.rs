// "Always ask / never allow" rules and the log of what was decided.
use hearth_tests::*;
use serde_json::{json, Value};

const CLAUDE: &str = r#"#!/usr/bin/env python3
import sys, json, time
sys.stdout.write(json.dumps({'type': 'system', 'subtype': 'init', 'session_id': 'sid-rules', 'model': 'x'}) + '\n'); sys.stdout.flush()
time.sleep(6)
"#;

fn arr(v: &Value) -> Vec<Value> { v.as_array().cloned().unwrap_or_default() }

#[test]
fn rules() {
    let env = Env::new("rules");
    let proj = env.proj.join("app"); std::fs::create_dir_all(&proj).unwrap();
    let proj = proj.to_string_lossy().to_string();
    env.stub("claude", CLAUDE);
    let relay = Relay::start(&env, Opts::new().env("HEARTH_MEM_CHILD", "1"));
    let a = relay.api(); let tok = relay.token.clone();
    let mut c = Checks::new();
    let hook = |tool: &str, input: Value| a.t("POST", "/hook", &tok, Some(json!({"hook_event_name": "PermissionRequest", "session_id": "sid-rules", "tool_name": tool, "tool_input": input, "cwd": proj})));
    let _run = a.tstream("POST", "/sessions/new/send", &tok, Some(json!({"text": "go", "mode": "acceptEdits", "cwd": proj}))); sleep(1200);
    let add = a.t("POST", "/admin/options", &tok, Some(json!({"addDeny": "rm -rf"})));
    c.ok("1 a rule that always says no is saved and listed", add.code == 200 && arr(&a.t("GET", "/admin/options", &tok, None).j["denyRules"]).iter().any(|r| r["prefix"] == "rm -rf"));
    let d1 = hook("Bash", json!({"command": "cd x && rm -rf build"}));
    c.ok("2 a command with those words is refused at once, without asking", d1.j["hookSpecificOutput"]["decision"]["behavior"] == "deny" && re("rule you made", &jstr(&d1.j["hookSpecificOutput"]["decision"]["message"])));
    a.t("POST", "/admin/options", &tok, Some(json!({"addPrefix": "npm test"})));
    let d2 = hook("Bash", json!({"command": "npm test"}));
    c.ok("3 an allow rule answers yes at once", d2.j["hookSpecificOutput"]["decision"]["behavior"] == "allow");
    let log = arr(&a.t("GET", "/admin/decisions", &tok, None).j["decisions"]);
    c.ok_x("4 both decisions are in the log, newest first, with what the rule was", log.len() >= 2 && log[0]["result"] == "allowed by your rule" && log[1]["result"] == "denied by your rule" && log[1]["rule"] == "rm -rf", json!(log.iter().map(|x| x["result"].clone()).collect::<Vec<_>>()).to_string());
    let id = a.t("GET", "/admin/options", &tok, None).j["denyRules"][0]["id"].clone();
    a.t("POST", "/admin/options", &tok, Some(json!({"removeDeny": id})));
    c.ok("5 a rule can be removed again", arr(&a.t("GET", "/admin/options", &tok, None).j["denyRules"]).is_empty());
    let guest = "share.abcd.abcdefghijklmnop";
    c.ok("6 a guest key cannot read the log or add rules", a.t("GET", "/admin/decisions", guest, None).code == 401 && a.t("POST", "/admin/options", guest, Some(json!({"addDeny": "ls"}))).code == 401);
    c.finish();
}
