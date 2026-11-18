// Auto-compact thrashing: when Claude Code gives up compacting, Hearth starts a fresh chat in the same folder with a hand-over,
// once (a fresh chat that thrashes again is left alone). Every run gets the output caps that prevent it. Fake "claude".
use hearth_tests::*;
use serde_json::json;

const CLAUDE: &str = r#"#!/usr/bin/env python3
import sys, os, json, re, datetime
text = sys.stdin.read()
log = os.path.join(os.environ['HOME'], 'runs.jsonl')
a = sys.argv
open(log, 'a').write(json.dumps({'resume': a[a.index('--resume') + 1] if '--resume' in a else '', 'text': text, 'bash_max': os.environ.get('BASH_MAX_OUTPUT_LENGTH', ''), 'sys': a[a.index('--append-system-prompt') + 1] if '--append-system-prompt' in a else ''}) + '\n')
n = sum(1 for _ in open(log))
sid = a[a.index('--resume') + 1] if '--resume' in a else 'fake-thrash-%d' % n
cwd = os.getcwd()
pd = os.path.join(os.environ['HOME'], '.claude', 'projects', re.sub('[^A-Za-z0-9]', '-', cwd)); os.makedirs(pd, exist_ok=True)
ts = datetime.datetime.utcnow().isoformat() + 'Z'
with open(os.path.join(pd, sid + '.jsonl'), 'a') as f:
    f.write(json.dumps({'type': 'user', 'cwd': cwd, 'timestamp': ts, 'message': {'content': text.strip()}}) + '\n')
    f.write(json.dumps({'type': 'assistant', 'cwd': cwd, 'timestamp': ts, 'message': {'content': [{'type': 'text', 'text': 'Half done: the parser is written.'}]}}) + '\n')
def out(o): sys.stdout.write(json.dumps(o) + '\n'); sys.stdout.flush()
out({'type': 'system', 'subtype': 'init', 'session_id': sid, 'model': 'fake'})
if 'THRASH' in text or 'Continue the work of an earlier chat' in text:
    out({'type': 'result', 'is_error': True, 'result': 'Autocompact is thrashing: the context refilled to the limit within 3 turns of the previous compact, 3 times in a row. A file being read or a tool output is likely too large for the context window. Try reading in smaller chunks, or use /clear to start fresh.'})
else:
    out({'type': 'result', 'is_error': False, 'result': 'ok'})
"#;

#[test]
fn thrash() {
    let env = Env::new("thrash");
    std::fs::create_dir_all(env.proj.join("app")).unwrap();
    env.stub("claude", CLAUDE);
    let relay = Relay::start(&env, Opts::new());
    let a = relay.api(); let tok = relay.token.clone();
    let mut c = Checks::new();
    let runs = || env.read("home/runs.jsonl").lines().filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok()).collect::<Vec<_>>();
    let proj = env.proj.join("app").to_string_lossy().to_string();

    a.t("POST", "/sessions/new/send", &tok, Some(json!({"text": "write the parser", "cwd": proj})));
    wait_for(8000, || runs().len() >= 1);
    let r = runs();
    c.ok_x("1 every run gets the output cap and the keep-the-context-small instruction", !r[0]["bash_max"].as_str().unwrap_or("").is_empty() && r[0]["sys"].as_str().unwrap_or("").contains("read big files in parts"), r[0]["bash_max"].to_string());
    sleep(1500);
    c.ok("2 a normal run starts nothing extra", runs().len() == 1);

    a.t("POST", "/sessions/new/send", &tok, Some(json!({"text": "THRASH: refactor everything", "cwd": proj})));
    wait_for(10000, || runs().len() >= 3);
    sleep(1500);
    let r = runs();
    let fresh = r.get(2).cloned().unwrap_or_default();
    c.ok_x("3 after thrashing a fresh chat starts by itself (not a resume)", r.len() >= 3 && fresh["resume"] == "", format!("runs={}", r.len()));
    let t = fresh["text"].as_str().unwrap_or("");
    c.ok_x("4 it gets the hand-over: what was asked and where it got to", t.contains("THRASH: refactor everything") && t.contains("Half done: the parser is written.") && t.contains("offset/limit"), t.chars().take(160).collect::<String>());
    c.ok_x("5 a fresh chat that thrashes again is left alone (no loop)", runs().len() == 3, format!("runs={}", runs().len()));
    c.finish();
}
