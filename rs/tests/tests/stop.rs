// The Stop button, and a permission request that is waiting when Stop is pressed. Uses a fake "claude".
use hearth_tests::*;
use serde_json::json;

const CLAUDE: &str = r#"#!/usr/bin/env python3
import sys, os, json, subprocess, time
a = sys.argv
sf = a[a.index('--settings') + 1]
hook = json.load(open(sf))['hooks']['PermissionRequest'][0]['hooks'][0]['command']
sid = 'fake-%d' % os.getpid()
def out(o): sys.stdout.write(json.dumps(o) + '\n'); sys.stdout.flush()
out({'type': 'system', 'subtype': 'init', 'session_id': sid, 'model': 'fake'})
if os.environ.get('ASK') == '1':
    subprocess.run(['sh', '-c', hook], input=json.dumps({'hook_event_name': 'PermissionRequest', 'session_id': sid, 'tool_name': 'Edit', 'tool_input': {'file_path': 'a.js', 'old_string': 'x', 'new_string': 'y'}, 'cwd': os.getcwd()}), capture_output=True, text=True)
while True:
    out({'type': 'stream_event', 'event': {'delta': {'type': 'text_delta', 'text': '.'}}})
    time.sleep(0.2)
"#;

fn alive(pid: i32) -> bool { unsafe { libc::kill(pid, 0) == 0 } }

#[test]
fn stop() {
    let env = Env::new("stop");
    env.stub("claude", CLAUDE);
    let relay = Relay::start(&env, Opts::new().no_projects().args(&["--lan"]).env("ASK", "1"));
    let a = relay.api(); let tok = relay.token.clone();
    let mut c = Checks::new();
    let events = a.tstream("GET", "/events", &tok, None);
    // a chat that waits for your OK is stopped: the approval is withdrawn and the process ends
    let run = a.tstream("POST", "/sessions/new/send", &tok, Some(json!({"text": "hi", "cwd": env.home.to_string_lossy()})));
    sleep(1500);
    let st = run.text();
    let sid = regex::Regex::new(r#""id":"(fake-\d+)""#).unwrap().captures(&st).map(|m| m[1].to_string());
    c.ok_x("1 the run reports its chat id", sid.is_some(), sid.clone().unwrap_or_default());
    let sid = sid.unwrap_or_default();
    let pid: i32 = sid.get(5..).and_then(|s| s.parse().ok()).unwrap_or(0);
    let r = a.t("POST", "/stop", &tok, Some(json!({"session": sid})));
    c.ok("2 stop answers ok", r.code == 200 && r.j["ok"] == true);
    run.wait(); sleep(300);
    c.ok("3 the process is gone", !alive(pid));
    c.ok("4 the stream ended", re(r#""t":"end""#, &run.text()));
    let r2 = a.t("POST", "/stop", &tok, Some(json!({"session": "nothing-here"})));
    c.ok("5 stopping something that does not run says so", r2.j["ok"] == false);
    // the events are "id: N" then "data: {...}" blocks (the desktop app reads them with this rule)
    let datas = sse_events(&events.text());
    c.ok("6 a permission request came as an id + data block", datas.iter().any(|e| e["kind"] == "permission" && e["tool"] == "Edit" && e["input"]["new_string"] == "y"));
    c.ok("7 stopping withdrew the waiting approval", datas.iter().any(|e| e["kind"] == "resolved"));
    c.finish();
}
