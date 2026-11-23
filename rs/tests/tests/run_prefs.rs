// A chat's mode, model and effort live on the server: what a device sends (or picks) is kept, and a device that sends none
// of them (the watch, "continue") gets the chat's own. A voice call's quick model is not kept. Fake "claude".
use hearth_tests::*;
use serde_json::json;

const CLAUDE: &str = r#"#!/usr/bin/env python3
import sys, os, json, re, datetime
text = sys.stdin.read()
a = sys.argv
def opt(k): return a[a.index(k) + 1] if k in a else ''
log = os.path.join(os.environ['HOME'], 'runs.jsonl')
open(log, 'a').write(json.dumps({'mode': opt('--permission-mode'), 'model': opt('--model'), 'effort': opt('--effort'), 'resume': opt('--resume')}) + '\n')
sid = opt('--resume') or 'fake-prefs-1'
cwd = os.getcwd()
pd = os.path.join(os.environ['HOME'], '.claude', 'projects', re.sub('[^A-Za-z0-9]', '-', cwd)); os.makedirs(pd, exist_ok=True)
ts = datetime.datetime.utcnow().isoformat() + 'Z'
with open(os.path.join(pd, sid + '.jsonl'), 'a') as f:
    f.write(json.dumps({'type': 'user', 'cwd': cwd, 'timestamp': ts, 'message': {'content': text.strip()}}) + '\n')
    f.write(json.dumps({'type': 'assistant', 'cwd': cwd, 'timestamp': ts, 'message': {'content': [{'type': 'text', 'text': 'ok'}]}}) + '\n')
def out(o): sys.stdout.write(json.dumps(o) + '\n'); sys.stdout.flush()
out({'type': 'system', 'subtype': 'init', 'session_id': sid, 'model': 'fake'})
out({'type': 'result', 'is_error': False, 'result': 'ok'})
"#;

#[test]
fn run_prefs() {
    let env = Env::new("runprefs");
    std::fs::create_dir_all(env.proj.join("app")).unwrap();
    env.stub("claude", CLAUDE);
    let relay = Relay::start(&env, Opts::new());
    let a = relay.api(); let tok = relay.token.clone();
    let mut c = Checks::new();
    let runs = || env.read("home/runs.jsonl").lines().filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok()).collect::<Vec<_>>();
    let proj = env.proj.join("app").to_string_lossy().to_string();
    let send = |b: serde_json::Value, n: usize| { a.t("POST", if n == 1 { "/sessions/new/send" } else { "/sessions/fake-prefs-1/send" }, &tok, Some(b)); wait_for(8000, || runs().len() >= n); sleep(1200); runs()[n - 1].clone() };

    let r = send(json!({"text": "start", "cwd": proj, "mode": "plan", "model": "opus", "effort": "high"}), 1);
    c.ok_x("1 a new chat runs with what was picked", r["mode"] == "plan" && r["model"] == "opus" && r["effort"] == "high", r.to_string());
    let p = a.t("GET", "/sessions/fake-prefs-1/prefs", &tok, None);
    c.ok_x("2 and the chat keeps it on the server", p.j["mode"] == "plan" && p.j["model"] == "opus" && p.j["effort"] == "high", p.text.clone());
    let r = send(json!({"text": "continue"}), 2);
    c.ok_x("3 a device that sends nothing (the watch) gets the chat's settings", r["mode"] == "plan" && r["model"] == "opus" && r["effort"] == "high", r.to_string());
    a.t("POST", "/sessions/fake-prefs-1/prefs", &tok, Some(json!({"mode": "manual", "model": "", "effort": "low"})));
    let r = send(json!({"text": "continue"}), 3);
    c.ok_x("4 picked on another device (even back to the default model): the next run follows", r["mode"] == "manual" && r["model"] == "" && r["effort"] == "low", r.to_string());
    let r = send(json!({"text": "a voice turn", "model": "haiku", "oneOff": true}), 4);
    c.ok("5 a voice call can run on its own model", r["model"] == "haiku");
    let r = send(json!({"text": "continue"}), 5);
    c.ok_x("6 without changing the chat's model", r["model"] == "", r.to_string());
    let r = send(json!({"text": "go", "mode": "acceptEdits"}), 6);
    let p = a.t("GET", "/sessions/fake-prefs-1/prefs", &tok, None);
    c.ok_x("7 what a device sends becomes the chat's setting", r["mode"] == "acceptEdits" && p.j["mode"] == "acceptEdits" && p.j["effort"] == "low", p.text.clone());
    c.finish();
}
