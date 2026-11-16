// A support mail becomes a notification event; "list" saves it (and tries Google Tasks), "claude" starts a chat, "ignore" does nothing.
use hearth_tests::*;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

const CLAUDE: &str = r#"#!/usr/bin/env python3
import sys, os, json, urllib.request
t = sys.stdin.read()
home = os.environ['HOME']
open(home + '/prompts.txt', 'a').write(t + '\n----\n')
a = sys.argv
if 'USE LIST TOOLS' in t:
    cfg = json.load(open(a[a.index('--mcp-config') + 1]))['mcpServers']['meter']['env']
    def post(b):
        b = dict(b); b['job'] = cfg['CM_JOB']
        r = urllib.request.Request('http://127.0.0.1:' + cfg['CM_RELAY_PORT'] + '/mcp/hub', data=json.dumps(b).encode(), headers={'Content-Type': 'application/json'}, method='POST')
        return json.loads(urllib.request.urlopen(r, timeout=20).read())
    for b in ({'tool': 'bug_list'}, {'tool': 'bug_fixed', 'id': 'gt:TASK-OTHER', 'note': 'done'}, {'tool': 'bug_fixed', 'id': 'HRT-20001', 'note': 'Fixed it.'}):
        open(home + '/tools.txt', 'a').write(json.dumps(post(b)) + '\n')
print(json.dumps({'type': 'system', 'subtype': 'init', 'session_id': 'sup-%d' % os.getpid()})); print(json.dumps({'type': 'result', 'is_error': False, 'result': 'ok'}))
"#;

fn arr(v: &Value) -> Vec<Value> { v.as_array().cloned().unwrap_or_default() }

#[test]
fn support_mail() {
    let env = Env::new("supmail");
    env.stub("claude", CLAUDE);
    let support = Mock::start(0, |_| "{}".into());
    let gt_items = Arc::new(Mutex::new(vec![
        json!({"id":"TASK-OTHER","title":"mcp server","notes":"make it easy","status":"needsAction"}),
        json!({"id":"TASK-SUP","title":"Support HRT-20001: Crash","notes":"x","status":"needsAction"})]));
    let gi = gt_items.clone();
    let google = Mock::start(0, move |q| {
        if q.url.starts_with("/token") { return r#"{"access_token":"tok"}"#.into(); }
        if q.method == "GET" { return json!({"items": gi.lock().unwrap().iter().filter(|x| x["status"] != "completed").cloned().collect::<Vec<_>>()}).to_string(); }
        if q.method == "PATCH" { let id = q.url.rsplit('/').next().unwrap_or("").to_string(); for t in gi.lock().unwrap().iter_mut() { if t["id"] == json!(id) { t["status"] = json!("completed"); } } return "{}".into(); }
        r#"{"id":"1"}"#.into()
    });
    let relay = Relay::start(&env, Opts::new()
        .env("HEARTH_SUPPORT_URL", &format!("http://127.0.0.1:{}", support.port))
        .env("HEARTH_GOOGLE_TOKEN_URL", &format!("http://127.0.0.1:{}/token", google.port))
        .env("HEARTH_GOOGLE_TASKS_URL", &format!("http://127.0.0.1:{}/tasks", google.port)));
    let a = relay.api(); let tok = relay.token.clone();
    let mut c = Checks::new();
    let events = a.tstream("GET", "/events", &tok, None);
    let evs = || -> Vec<Value> { events.text().split("\n\n").filter(|x| x.contains("data: ")).filter_map(|x| x.lines().find(|l| l.starts_with("data: "))).map(|l| serde_json::from_str(&l[6..]).unwrap_or(json!({}))).collect() };
    let mail = |id: &str| json!({"kind":"new","id":id,"title":"App crashes","category":"bug","from":"friend@example.com","text":"It crashes when I open settings."});
    let prompts = || env.read("home/prompts.txt");
    let supports = |t: &str| evs().into_iter().filter(|e| e["kind"] == "support" && e["ticket"] == t).collect::<Vec<_>>();
    c.ok("1 only the owner can hand in a support mail", a.t("POST", "/support/incoming", "wrong", Some(mail("HRT-1"))).code == 401);
    c.ok("2 a support mail is accepted", a.t("POST", "/support/incoming", &tok, Some(mail("HRT-10001"))).code == 200);
    sleep(500);
    let mut s: Vec<Value> = evs().into_iter().filter(|e| e["kind"] == "support").collect();
    c.ok_x("3 it is published with a request id, ticket and short text", s.len() == 1 && !s[0]["req"].is_null() && s[0]["ticket"] == "HRT-10001" && re("(?i)new bug report from friend@example.com", &jstr(&s[0]["text"])), s.get(0).map(|e| e.to_string().chars().take(140).collect::<String>()).unwrap_or_default());
    a.t("POST", "/decision", &tok, Some(json!({"req": s[0]["req"], "behavior": "ignore"}))); sleep(500);
    c.ok("4 ignore does nothing: no chat, nothing on the list", !env.home.join("prompts.txt").exists() && a.t("GET", "/support/list", &tok, None).text == "[]");
    a.t("POST", "/support/incoming", &tok, Some(mail("HRT-10002"))); sleep(400); s = supports("HRT-10002");
    a.t("POST", "/decision", &tok, Some(json!({"req": s[0]["req"], "behavior": "list"}))); sleep(800);
    let list = arr(&a.t("GET", "/support/list", &tok, None).j);
    c.ok("5 add to list without Google: saved locally, and tells why Google was skipped", list.len() == 1 && list[0]["id"] == "HRT-10002" && list[0]["google"] == false && evs().iter().any(|e| e["kind"] == "support" && re("Google Tasks: .*not connected", &jstr(&e["text"]))));
    c.ok("6 saving Google Tasks settings works and is shown", a.t("POST", "/admin/options", &tok, Some(json!({"googleTasks": {"clientId":"cid","clientSecret":"sec","refreshToken":"ref"}}))).text.contains(r#""googleTasks":true"#));
    a.t("POST", "/support/incoming", &tok, Some(mail("HRT-10003"))); sleep(400); s = supports("HRT-10003");
    a.t("POST", "/decision", &tok, Some(json!({"req": s[0]["req"], "behavior": "list"}))); sleep(1000);
    let calls: Vec<String> = google.seen().iter().map(|q| format!("{} {} {}", q.method, q.url, q.body.chars().take(200).collect::<String>())).collect();
    c.ok_x("7 with Google connected a task is created", calls.iter().any(|c| c.contains("/token") && re("refresh_token=ref", c)) && calls.iter().any(|c| c.starts_with("POST /tasks") && c.contains("HRT-10003")), calls.join(" | ").chars().take(200).collect::<String>());
    c.ok("8 and the user is told", evs().iter().any(|e| e["kind"] == "support" && re("Added to your Google Tasks", &jstr(&e["text"]))));
    a.t("POST", "/support/incoming", &tok, Some(mail("HRT-10004"))); sleep(400); s = supports("HRT-10004");
    a.t("POST", "/decision", &tok, Some(json!({"req": s[0]["req"], "behavior": "claude"}))); sleep(1500);
    let pr = prompts();
    c.ok("9 give to Claude starts a chat with the mail in it", pr.contains("HRT-10004") && pr.contains("It crashes when I open settings") && pr.contains("Do not email anyone"));
    c.ok("10 a handled mail is no longer pending", !a.t("GET", "/pending", &tok, None).text.contains("HRT-10004"));
    a.t("POST", "/support/incoming", &tok, Some(mail("HRT-10005"))); sleep(300);
    c.ok("11 an unhandled one is replayed to apps that connect later", a.t("GET", "/pending", &tok, None).text.contains("HRT-10005"));
    c.ok("12 from the support screen: bad input refused", a.t("POST", "/support/act", &tok, Some(json!({"id":"HRT-1","action":"delete"}))).code == 400);
    let before2 = arr(&a.t("GET", "/support/list", &tok, None).j).len();
    let act1 = a.t("POST", "/support/act", &tok, Some(json!({"id":"HRT-20001","title":"Crash","category":"bug","from":"a@b.c","text":"It crashes.","action":"list"}))).j;
    let after2 = arr(&a.t("GET", "/support/list", &tok, None).j);
    c.ok_x("13 \"add to tasks\" from the screen saves it and reports", act1["ok"] == true && after2.len() == before2 + 1 && re("Google Tasks", &jstr(&act1["text"])), jstr(&act1["text"]));
    c.ok("14 doing it twice does not add it twice", a.t("POST", "/support/act", &tok, Some(json!({"id":"HRT-20001","title":"Crash","from":"a@b.c","text":"It crashes.","action":"list"}))).j["text"] == "Already on your list.");
    a.t("POST", "/support/act", &tok, Some(json!({"id":"HRT-20002","title":"Slow","category":"bug","from":"a@b.c","text":"It is slow on start.","action":"claude"}))); sleep(1500);
    c.ok("15 \"fix with Claude\" from the screen starts a chat", prompts().contains("It is slow on start."));
    c.ok("16 a guest key cannot use it", a.t("POST", "/support/act", "share.abcd.abcdefghijklmnop", Some(json!({"id":"x","action":"list"}))).code == 401);
    // the person who wrote is told by mail: when it is put on the list, and when it is fixed
    let sc = support.seen();
    let listed: Vec<_> = sc.iter().filter(|c| c.body.contains(r#""event":"listed""#)).collect();
    c.ok_x("17 putting a request on the list tells the support desk to mail the person", !listed.is_empty() && listed[0].url == "/owner/notify" && listed[0].auth == format!("Bearer {tok}") && re("HRT-20001|HRT-10003|HRT-10002", &listed[0].body), listed.get(0).map(|x| format!("{:?}", x).chars().take(120).collect::<String>()).unwrap_or_default());
    c.ok("18 ignore and \"fix with Claude\" do not mail anyone", !sc.iter().any(|c| re("HRT-10001|HRT-20002", &c.body)));
    // the list is the Google Tasks list: Claude sees everything on it and checks things off
    a.t("POST", "/support/act", &tok, Some(json!({"id":"HRT-20001","title":"Crash","category":"bug","from":"a@b.c","text":"It crashes.","action":"list"})));
    let _ = std::fs::remove_file(env.home.join("tools.txt"));
    a.t("POST", "/sessions/new/send", &tok, Some(json!({"text":"USE LIST TOOLS","cwd": env.proj.to_string_lossy()}))); sleep(2500);
    let tl: Vec<String> = env.read("home/tools.txt").trim().lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).map(|j| jstr(&j["text"])).collect();
    let t0 = tl.get(0).cloned().unwrap_or_default();
    c.ok_x("19 the list shows support mails and the other tasks of Google Tasks, each once", re(r"\[gt:TASK-OTHER\] mcp server", &t0) && re(r"\[HRT-20001\]", &t0) && !re(r"\[gt:TASK-SUP\]", &t0), t0.chars().take(160).collect::<String>());
    let status = |id: &str| gt_items.lock().unwrap().iter().find(|x| x["id"] == json!(id)).map(|x| jstr(&x["status"])).unwrap_or_default();
    c.ok("20 checking off a Google task completes it", re("Checked off in Google Tasks", tl.get(1).map(|s| s.as_str()).unwrap_or("")) && status("TASK-OTHER") == "completed");
    c.ok("21 checking off a support mail also completes its task and tells the person", re("Marked HRT-20001 as fixed and checked off", tl.get(2).map(|s| s.as_str()).unwrap_or("")) && status("TASK-SUP") == "completed" && support.seen().iter().any(|c| c.body.contains(r#""event":"fixed""#) && c.body.contains("HRT-20001")));
    c.finish();
}
