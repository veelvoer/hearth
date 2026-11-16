// Usage limit: Hearth asks whether to continue when usage is back, or continues by itself when "Continue by itself" is on. Fake "claude".
use hearth_tests::*;
use serde_json::json;

const CLAUDE: &str = r#"#!/usr/bin/env python3
import sys, os, json, re, time, datetime
text = sys.stdin.read()
cnt = os.path.join(os.environ['HOME'], 'count')
try: n = int(open(cnt).read())
except Exception: n = 0
open(cnt, 'w').write(str(n + 1))
a = sys.argv
sid = a[a.index('--resume') + 1] if '--resume' in a else 'fake-limit-%d' % os.getpid()
cwd = os.getcwd()
pd = os.path.join(os.environ['HOME'], '.claude', 'projects', re.sub('[^A-Za-z0-9]', '-', cwd)); os.makedirs(pd, exist_ok=True)
open(os.path.join(pd, sid + '.jsonl'), 'w').write(json.dumps({'type': 'user', 'cwd': cwd, 'timestamp': datetime.datetime.utcnow().isoformat() + 'Z', 'message': {'content': 'x ' + text.strip()}}) + '\n')
def out(o): sys.stdout.write(json.dumps(o) + '\n'); sys.stdout.flush()
out({'type': 'system', 'subtype': 'init', 'session_id': sid, 'model': 'fake'})
if n % 2 == 0:
    out({'type': 'rate_limit_event', 'rate_limit_info': {'status': 'rejected', 'rateLimitType': 'five_hour', 'utilization': 1, 'resetsAt': int(time.time()) + 1}})
    out({'type': 'result', 'is_error': True, 'result': "You've hit your limit · resets soon"})
else:
    out({'type': 'stream_event', 'event': {'delta': {'type': 'text_delta', 'text': 'RESUMED ' + text.strip()}}})
    out({'type': 'result', 'is_error': False, 'result': 'done'})
"#;

#[test]
fn limit() {
    let env = Env::new("limit");
    std::fs::create_dir_all(env.proj.join("app")).unwrap();
    env.stub("claude", CLAUDE);
    let relay = Relay::start(&env, Opts::new().env("HEARTH_LIMIT_BUFFER_MS", "300"));
    let a = relay.api(); let tok = relay.token.clone();
    let mut c = Checks::new();
    let events = a.tstream("GET", "/events", &tok, None);
    let evs = || sse_events(&events.text());
    let count = || env.read("home/count").trim().parse::<u32>().unwrap_or(0);
    let proj = env.proj.join("app").to_string_lossy().to_string();
    a.t("POST", "/sessions/new/send", &tok, Some(json!({"text": "build it", "cwd": proj})));
    wait_for(8000, || evs().iter().any(|e| e["kind"] == "limit"));
    let lim: Vec<_> = evs().into_iter().filter(|e| e["kind"] == "limit").collect();
    c.ok_x("1 a limit card with a request id is published", lim.len() == 1 && !lim[0]["req"].is_null() && !lim[0]["req"].as_str().unwrap_or("").is_empty() && lim[0]["auto"].as_bool() != Some(true), lim.get(0).map(|e| e.to_string().chars().take(120).collect::<String>()).unwrap_or_default());
    c.ok("2 no generic error notification for it", !evs().iter().any(|e| e["kind"] == "error"));
    sleep(1500); c.ok("3 nothing resumes before you choose", count() == 1);
    a.t("POST", "/decision", &tok, Some(json!({"req": lim[0]["req"], "behavior": "skip"}))); sleep(1500);
    c.ok("4 \"no\" leaves it alone", count() == 1);
    // again, and say yes
    let sid = jstr(&lim[0]["session"]);
    a.t("POST", &format!("/sessions/{sid}/send"), &tok, Some(json!({"text": "again", "cwd": proj}))); wait_for(8000, || count() >= 2);
    c.ok("5 a normal send still works", count() == 2);
    a.t("POST", &format!("/sessions/{sid}/send"), &tok, Some(json!({"text": "limit again", "cwd": proj}))); wait_for(8000, || evs().iter().filter(|e| e["kind"] == "limit").count() >= 2);
    let lim2: Vec<_> = evs().into_iter().filter(|e| e["kind"] == "limit" && e["req"] != lim[0]["req"]).collect();
    c.ok("6 hit again: a new card", lim2.len() == 1);
    a.t("POST", "/decision", &tok, Some(json!({"req": lim2[0]["req"], "behavior": "continue"}))); wait_for(10000, || count() >= 4);
    c.ok_x("7 yes: it continues by itself after the reset", count() == 4, format!("count={}", count()));
    // auto continue
    let opt = a.t("POST", "/admin/options", &tok, Some(json!({"autoContinue": true})));
    c.ok("8 the setting is saved and shown", opt.j["autoContinue"] == true && a.t("GET", "/admin/options", &tok, None).j["autoContinue"] == true);
    a.t("POST", &format!("/sessions/{sid}/send"), &tok, Some(json!({"text": "limit three", "cwd": proj}))); wait_for(10000, || count() >= 6);
    let auto: Vec<_> = evs().into_iter().filter(|e| e["kind"] == "limit" && e["auto"] == true).collect();
    c.ok_x("9 auto: a heads-up without a question, and it continued", auto.len() == 1 && auto[0]["req"].is_null() && count() == 6, format!("count={}", count()));
    c.finish();
}
