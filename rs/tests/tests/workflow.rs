// Workflows: steps run one after another for their time, a stopped run is continued, the bug list tools. Fake "claude".
use hearth_tests::*;
use serde_json::json;

const CLAUDE: &str = r#"#!/usr/bin/env python3
import sys, os, json, re, datetime
text = sys.stdin.read()
a = sys.argv
sid = a[a.index('--resume') + 1] if '--resume' in a else 'wf-%d' % os.getpid()
cwd = os.getcwd()
pd = os.path.join(os.environ['HOME'], '.claude', 'projects', re.sub('[^A-Za-z0-9]', '-', cwd)); os.makedirs(pd, exist_ok=True)
open(os.path.join(pd, sid + '.jsonl'), 'a').write(json.dumps({'type': 'user', 'cwd': cwd, 'sessionId': sid, 'timestamp': datetime.datetime.utcnow().isoformat() + 'Z', 'message': {'role': 'user', 'content': text.strip()}}) + '\n')
open(os.path.join(os.environ['HOME'], 'prompts.txt'), 'a').write(text.strip().split('\n')[0] + '\n')
open(os.path.join(os.environ['HOME'], 'args.txt'), 'a').write(' '.join(a) + '\n')
print(json.dumps({'type': 'system', 'subtype': 'init', 'session_id': sid})); print(json.dumps({'type': 'result', 'is_error': False, 'result': 'ok'}))
"#;

#[test]
fn workflow() {
    let env = Env::new("wf");
    std::fs::create_dir_all(env.proj.join("app")).unwrap();
    env.stub("claude", CLAUDE);
    let relay = Relay::start(&env, Opts::new().env("HEARTH_WF_MINUTE_MS", "2500").env("HEARTH_WF_TICK_MS", "400"));
    let a = relay.api(); let tok = relay.token.clone();
    let mut c = Checks::new();
    let prompts = || -> Vec<String> { let s = env.read("home/prompts.txt"); let s = s.trim(); if s.is_empty() { vec![] } else { s.split('\n').map(|x| x.to_string()).collect() } };
    let proj = env.proj.join("app").to_string_lossy().to_string();
    a.t("POST", "/sessions/new/send", &tok, Some(json!({"text": "hello", "cwd": proj}))); sleep(800);
    let sid = jstr(&a.t("GET", "/sessions", &tok, None).j[0]["id"]);
    let wf = format!("/sessions/{sid}/workflow");
    c.ok("1 no workflow at first", a.t("GET", &wf, &tok, None).j["state"] == "none");
    c.ok("2 empty workflow refused", a.t("POST", &wf, &tok, Some(json!({"steps": [{"name": "x", "prompt": "", "minutes": 1}]}))).code == 400);
    c.ok("3 a guest key cannot start one", a.t("POST", &wf, "share.abcd.abcdefghijklmnop", Some(json!({"steps": [{"name": "a", "prompt": "b", "minutes": 1}]}))).code == 401);
    let before = prompts().len();
    let st = a.t("POST", &wf, &tok, Some(json!({"steps": [{"name": "Bug test", "prompt": "Test everything and list bugs.", "minutes": 1}, {"name": "Bug fix", "prompt": "Fix all the bugs.", "minutes": 1}]})));
    c.ok_x("4 it starts", st.code == 200 && st.j["state"] == "running" && st.j["step"] == "Bug test", st.text.chars().take(100).collect::<String>());
    sleep(1800);
    let p1: Vec<String> = prompts().into_iter().skip(before).collect();
    c.ok_x("5 Claude is started again and again while the step lasts (a run that ends early is continued)", p1.len() >= 2 && p1.iter().all(|x| re("step 1 of 2", x)) && re("(?i)Carry on|about", &p1.join(" ")), p1.join(" | ").chars().take(200).collect::<String>());
    sleep(1900);
    let p2: Vec<String> = prompts().into_iter().skip(before).collect();
    c.ok("6 after the time it moves on to the next step", p2.iter().any(|x| re("step 2 of 2", x)) && re("Bug fix|Fix all", &env.read("home/prompts.txt")));
    sleep(3200);
    c.ok("7 after the last step it is done and stops starting runs", a.t("GET", &wf, &tok, None).j["state"] == "done");
    let n = prompts().len(); sleep(1000); c.ok("8 no runs after it is done", prompts().len() == n);
    a.t("POST", &wf, &tok, Some(json!({"steps": [{"name": "Long", "prompt": "Keep testing.", "minutes": 60}]}))); sleep(600);
    c.ok("9 pause stops new runs", a.t("POST", &wf, &tok, Some(json!({"action": "pause"}))).j["state"] == "paused");
    sleep(1500); let m = prompts().len(); sleep(1200);
    c.ok("10 nothing starts while paused", prompts().len() == m);
    c.ok("11 resume works", a.t("POST", &wf, &tok, Some(json!({"action": "resume"}))).j["state"] == "running"); sleep(900);
    c.ok("12 and runs start again", prompts().len() > m);
    c.ok("13 stop removes it", a.t("POST", &wf, &tok, Some(json!({"action": "stop"}))).j["state"] == "none");
    let args = env.read("home/args.txt");
    c.ok("14 chats get the bug list tools", re("mcp__meter__bug_list", &args) && re("mcp__meter__bug_fixed", &args));
    c.finish();
}
