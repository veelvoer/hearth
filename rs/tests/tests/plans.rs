// A plan with every task: the system prompt asks for one (unless switched off), "/plan" asks for one anyway and is not sent on,
// /sessions/:id/plan reads the checklist back, and your own Anthropic API key reaches Claude when switched on. Fake "claude".
use hearth_tests::*;
use serde_json::json;

const CLAUDE: &str = r#"#!/usr/bin/env python3
import sys, os, json, re, datetime
if '--help' in sys.argv: print('  --tools <tools...>  Specify the list of available tools'); sys.exit(0)
text = sys.stdin.read()
a = sys.argv
log = os.path.join(os.environ['HOME'], 'runs.jsonl')
open(log, 'a').write(json.dumps({'text': text, 'sys': a[a.index('--append-system-prompt') + 1] if '--append-system-prompt' in a else '', 'key': os.environ.get('ANTHROPIC_API_KEY', ''), 'tools': os.environ.get('CLAUDE_CODE_ENABLE_TODO_TOOLS', '')}) + '\n')
n = sum(1 for _ in open(log))
sid = 'fake-plan-%d' % n
cwd = os.getcwd()
pd = os.path.join(os.environ['HOME'], '.claude', 'projects', re.sub('[^A-Za-z0-9]', '-', cwd)); os.makedirs(pd, exist_ok=True)
ts = datetime.datetime.utcnow().isoformat() + 'Z'
def tool(i, name, inp): return {'type': 'assistant', 'cwd': cwd, 'timestamp': ts, 'message': {'content': [{'type': 'tool_use', 'id': 't%d' % i, 'name': name, 'input': inp}]}}
def res(i, txt): return {'type': 'user', 'cwd': cwd, 'timestamp': ts, 'message': {'content': [{'type': 'tool_result', 'tool_use_id': 't%d' % i, 'content': txt}]}}
with open(os.path.join(pd, sid + '.jsonl'), 'a') as f:
    f.write(json.dumps({'type': 'user', 'cwd': cwd, 'timestamp': ts, 'message': {'content': text.strip()}}) + '\n')
    if 'todo' in text:
        f.write(json.dumps(tool(1, 'TodoWrite', {'todos': [{'content': 'Read the code', 'status': 'completed', 'activeForm': 'Reading the code'}, {'content': 'Write the parser', 'status': 'in_progress', 'activeForm': 'Writing the parser'}, {'content': 'Test it', 'status': 'pending', 'activeForm': 'Testing it'}]})) + '\n')
    if 'tasks' in text:
        f.write(json.dumps(tool(1, 'TaskCreate', {'subject': 'Plan the work', 'activeForm': 'Planning the work'})) + '\n'); f.write(json.dumps(res(1, 'Task #7 created successfully: Plan the work')) + '\n')
        f.write(json.dumps(tool(2, 'TaskCreate', {'subject': 'Do the work', 'activeForm': 'Doing the work'})) + '\n'); f.write(json.dumps(res(2, 'Task #8 created successfully: Do the work')) + '\n')
        f.write(json.dumps(tool(3, 'TaskUpdate', {'taskId': '7', 'status': 'completed'})) + '\n'); f.write(json.dumps(res(3, 'Updated task #7')) + '\n')
        f.write(json.dumps(tool(4, 'TaskUpdate', {'taskId': '8', 'status': 'in_progress'})) + '\n'); f.write(json.dumps(res(4, 'Updated task #8')) + '\n')
    f.write(json.dumps({'type': 'assistant', 'cwd': cwd, 'timestamp': ts, 'message': {'content': [{'type': 'text', 'text': 'ok'}]}}) + '\n')
def out(o): sys.stdout.write(json.dumps(o) + '\n'); sys.stdout.flush()
out({'type': 'system', 'subtype': 'init', 'session_id': sid, 'model': 'fake'})
out({'type': 'result', 'is_error': False, 'result': 'ok'})
"#;

#[test]
fn plans() {
    let env = Env::new("plans");
    std::fs::create_dir_all(env.proj.join("app")).unwrap();
    env.stub("claude", CLAUDE);
    let relay = Relay::start(&env, Opts::new());
    let a = relay.api(); let tok = relay.token.clone();
    let mut c = Checks::new();
    let runs = || env.read("home/runs.jsonl").lines().filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok()).collect::<Vec<_>>();
    let proj = env.proj.join("app").to_string_lossy().to_string();
    let send = |t: &str| { a.t("POST", "/sessions/new/send", &tok, Some(json!({"text": t, "cwd": proj}))); };

    send("make the todo parser");
    wait_for(8000, || runs().len() >= 1);
    let r = runs();
    c.ok_x("1 plans are on by default: the run is asked for a plan, with the task tools switched on", jstr(&r[0]["sys"]).contains("TaskCreate") && jstr(&r[0]["tools"]) == "1", r[0]["tools"].to_string());
    sleep(1500);
    let p = a.t("GET", "/sessions/fake-plan-1/plan", &tok, None);
    let items = p.j["items"].as_array().cloned().unwrap_or_default();
    c.ok_x("2 the checklist comes back: done, now (its -ing form) and still to come", items.len() == 3 && items[0]["status"] == "completed" && items[1]["status"] == "in_progress" && items[1]["text"] == "Writing the parser" && items[2]["status"] == "pending", p.text.chars().take(200).collect::<String>());

    send("tasks please");
    wait_for(8000, || runs().len() >= 2);
    sleep(1500);
    let p = a.t("GET", "/sessions/fake-plan-2/plan", &tok, None);
    let items = p.j["items"].as_array().cloned().unwrap_or_default();
    c.ok_x("3 TaskCreate and TaskUpdate make the same checklist (ids from the tool results)", items.len() == 2 && items[0]["status"] == "completed" && items[0]["text"] == "Plan the work" && items[1]["status"] == "in_progress" && items[1]["text"] == "Doing the work", p.text.chars().take(200).collect::<String>());

    a.t("POST", "/admin/safety", &tok, Some(json!({"plans": false})));
    c.ok("4 plans can be switched off", a.t("GET", "/admin/safety", &tok, None).j["plans"] == false);
    send("just a question");
    wait_for(8000, || runs().len() >= 3);
    c.ok("5 switched off: no plan asked", !jstr(&runs()[2]["sys"]).contains("TaskCreate"));
    send("/plan fix the bug");
    wait_for(8000, || runs().len() >= 4);
    let r = runs();
    c.ok_x("6 /plan asks for one anyway, and only the rest of the message reaches Claude", jstr(&r[3]["sys"]).contains("The user asked for a plan") && r[3]["text"] == "fix the bug", r[3]["text"].to_string());

    let bad = a.t("POST", "/admin/safety", &tok, Some(json!({"apiKey": "hello"})));
    c.ok("7 a key that is no Anthropic key is refused", bad.code == 400);
    a.t("POST", "/admin/safety", &tok, Some(json!({"apiKey": "sk-ant-test-1234567890abcd"})));
    let v = a.t("GET", "/admin/safety", &tok, None);
    c.ok_x("8 the key is kept, in use, and only its end is shown", v.j["apiKey"]["set"] == true && v.j["apiKey"]["use"] == true && v.j["apiKey"]["ends"] == "abcd" && !v.text.contains("sk-ant-test"), v.j["apiKey"].to_string());
    send("with my key");
    wait_for(8000, || runs().len() >= 5);
    c.ok("9 Claude gets the key", runs()[4]["key"] == "sk-ant-test-1234567890abcd");
    a.t("POST", "/admin/safety", &tok, Some(json!({"useApiKey": false})));
    send("on my account again");
    wait_for(8000, || runs().len() >= 6);
    c.ok("10 switched off: Claude runs on the account (no key)", runs()[5]["key"] == "");
    c.finish();
}
