// The prompt improver: /improve rewrites a prompt (Haiku), the mode (off, button, auto) is a server setting, and in auto
// every real prompt is improved before it runs. Fake "claude".
use hearth_tests::*;
use serde_json::json;

const CLAUDE: &str = r#"#!/usr/bin/env python3
import sys, os, json, re, datetime
a = sys.argv
text = sys.stdin.read()
def opt(k): return a[a.index(k) + 1] if k in a else ''
if '--system-prompt' in a:
    inner = re.sub(r'</?prompt>', '', text).strip()
    sys.stdout.write(json.dumps({'type': 'result', 'is_error': False, 'result': 'Goal: ' + inner + '\n- keep it small'})); sys.exit(0)
log = os.path.join(os.environ['HOME'], 'runs.jsonl')
open(log, 'a').write(json.dumps({'text': text}) + '\n')
sid = 'fake-imp-%d' % sum(1 for _ in open(log))
cwd = os.getcwd()
pd = os.path.join(os.environ['HOME'], '.claude', 'projects', re.sub('[^A-Za-z0-9]', '-', cwd)); os.makedirs(pd, exist_ok=True)
ts = datetime.datetime.utcnow().isoformat() + 'Z'
with open(os.path.join(pd, sid + '.jsonl'), 'a') as f:
    f.write(json.dumps({'type': 'user', 'cwd': cwd, 'timestamp': ts, 'message': {'content': text.strip()}}) + '\n')
def out(o): sys.stdout.write(json.dumps(o) + '\n'); sys.stdout.flush()
out({'type': 'system', 'subtype': 'init', 'session_id': sid, 'model': 'fake'})
out({'type': 'result', 'is_error': False, 'result': 'ok'})
"#;

#[test]
fn improve() {
    let env = Env::new("improve");
    std::fs::create_dir_all(env.proj.join("app")).unwrap();
    env.stub("claude", CLAUDE);
    let relay = Relay::start(&env, Opts::new());
    let a = relay.api(); let tok = relay.token.clone();
    let mut c = Checks::new();
    let runs = || env.read("home/runs.jsonl").lines().filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok()).collect::<Vec<_>>();
    let proj = env.proj.join("app").to_string_lossy().to_string();

    c.ok("1 the default is the button", a.t("GET", "/admin/safety", &tok, None).j["improve"] == "button");
    let r = a.t("POST", "/improve", &tok, Some(json!({"text": "make the login faster"})));
    c.ok_x("2 the button rewrites a prompt", r.code == 200 && jstr(&r.j["text"]).starts_with("Goal: make the login faster"), r.text.clone());
    c.ok("3 an empty prompt is refused", a.t("POST", "/improve", &tok, Some(json!({"text": "  "}))).code == 400);
    a.t("POST", "/sessions/new/send", &tok, Some(json!({"text": "make the login page much faster please", "cwd": proj})));
    wait_for(8000, || runs().len() >= 1);
    c.ok("4 in button mode a sent prompt is left as written", runs()[0]["text"] == "make the login page much faster please");

    a.t("POST", "/admin/safety", &tok, Some(json!({"improve": "auto"})));
    c.ok("5 the mode can be set to always", a.t("GET", "/admin/safety", &tok, None).j["improve"] == "auto");
    a.t("POST", "/sessions/new/send", &tok, Some(json!({"text": "make the signup page much faster please", "cwd": proj})));
    wait_for(8000, || runs().len() >= 2);
    c.ok_x("6 in always mode the prompt is improved before it runs", jstr(&runs()[1]["text"]).starts_with("Goal: make the signup page"), runs()[1]["text"].to_string());
    a.t("POST", "/sessions/new/send", &tok, Some(json!({"text": "/plan ok then", "cwd": proj})));
    wait_for(8000, || runs().len() >= 3);
    c.ok("7 a slash command is never improved", !jstr(&runs()[2]["text"]).contains("Goal:"));

    a.t("POST", "/admin/safety", &tok, Some(json!({"improve": "off"})));
    c.ok("8 off: no improving at all", a.t("POST", "/improve", &tok, Some(json!({"text": "hello there friend"}))).code == 403);
    c.finish();
}
