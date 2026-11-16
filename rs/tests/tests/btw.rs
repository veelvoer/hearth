// A side question (like /btw) is answered by a copy of the chat with no tools.
use hearth_tests::*;
use serde_json::json;
use std::fs;

#[test]
fn btw() {
    let env = Env::new("btw");
    let proj = env.proj.join("app"); fs::create_dir_all(&proj).unwrap();
    let ps = proj.to_string_lossy().to_string();
    let enc: String = ps.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let dir = env.home.join(".claude/projects").join(enc); fs::create_dir_all(&dir).unwrap();
    let sid = "sid-btw1";
    fs::write(dir.join(format!("{sid}.jsonl")), json!({"type":"user","cwd":ps,"sessionId":sid,"timestamp":"2026-10-20T10:00:00.000Z","message":{"role":"user","content":"do it"}}).to_string() + "\n").unwrap();
    let log = env.t.join("args.txt");
    env.stub("claude", &format!("#!/usr/bin/env python3\nimport sys, json\nt = sys.stdin.read()\nopen({:?}, 'w').write(' '.join(sys.argv[1:]) + '\\n' + t)\nsys.stdout.write(json.dumps({{'type': 'result', 'is_error': False, 'result': 'It is in a.txt.', 'usage': {{}}}}))\n", log.to_string_lossy()));
    let relay = Relay::start(&env, Opts::new().env("HEARTH_MEM_CHILD", ""));
    let a = relay.api(); let tok = relay.token.clone();
    let mut c = Checks::new();
    let r = a.t("POST", &format!("/sessions/{sid}/btw"), &tok, Some(json!({"text": "where is the config?"})));
    let rec = fs::read_to_string(&log).unwrap_or_default();
    c.ok_x("1 the answer comes back", r.code == 200 && r.j["text"] == "It is in a.txt.", r.j.to_string());
    c.ok_x("2 it uses a copy of the chat, no tools, and keeps nothing", re("--resume sid-btw1", &rec) && re("--fork-session", &rec) && re("--no-session-persistence", &rec) && re(r"--tools\s", &rec) && re(r"where is the config\?", &rec), rec.chars().take(160).collect::<String>());
    c.ok("3 an empty question is refused", a.t("POST", &format!("/sessions/{sid}/btw"), &tok, Some(json!({"text": " "}))).code == 400);
    c.ok("4 an unknown chat is refused", a.t("POST", "/sessions/nope/btw", &tok, Some(json!({"text": "hi"}))).code == 404);
    c.ok("5 a guest key cannot ask", a.t("POST", &format!("/sessions/{sid}/btw"), "share.abcd.abcdefghijklmnop", Some(json!({"text": "hi"}))).code == 401);
    let rs = a.t("GET", &format!("/sessions/{sid}/resume"), &tok, None);
    c.ok_x("6 a chat says how to carry on in Claude Code: its folder and claude --resume", rs.code == 200 && rs.j["cwd"] == json!(ps) && rs.j["rel"] == "app" && rs.j["command"] == json!(format!("cd {} && claude --resume {sid}", json!(ps))), rs.j.to_string().chars().take(200).collect::<String>());
    c.finish();
}
