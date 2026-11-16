// Always-allowed commands, Full auto while Claude works, and working outside the projects folder. Uses a fake "claude" (and a fake sudo).
use hearth_tests::*;
use serde_json::json;

const CLAUDE: &str = r#"#!/usr/bin/env python3
import sys, os, json, re, subprocess, random, datetime
a = sys.argv
sf = a[a.index('--settings') + 1]
hook = json.load(open(sf))['hooks']['PermissionRequest'][0]['hooks'][0]['command']
sid = 'fake-%d-%d' % (os.getpid(), random.randint(0, 999999))
text = sys.stdin.read()
cwd = os.getcwd()
pd = os.path.join(os.environ['HOME'], '.claude', 'projects', re.sub('[^A-Za-z0-9]', '-', cwd)); os.makedirs(pd, exist_ok=True)
open(os.path.join(pd, sid + '.jsonl'), 'w').write(json.dumps({'type': 'user', 'cwd': cwd, 'timestamp': datetime.datetime.utcnow().isoformat() + 'Z', 'message': {'content': 'fake chat ' + text.strip()}}) + '\n')
def out(o): sys.stdout.write(json.dumps(o) + '\n'); sys.stdout.flush()
out({'type': 'system', 'subtype': 'init', 'session_id': sid, 'model': 'fake'})
r = subprocess.run(['sh', '-c', hook], input=json.dumps({'hook_event_name': 'PermissionRequest', 'session_id': sid, 'tool_name': 'Bash', 'tool_input': {'command': text.strip()}, 'cwd': cwd}), capture_output=True, text=True)
j = ' '.join(a)
mode = a[a.index('--permission-mode') + 1] if '--permission-mode' in a else None
out({'type': 'stream_event', 'event': {'delta': {'type': 'text_delta', 'text': 'RESULT ' + ('allowed' if '"allow"' in r.stdout else 'not-allowed') + ' cwd=' + cwd + (' ROOTPROMPT' if 'running as root' in j else '') + (' DROPPED' if 'WITHOUT root rights' in j else '') + (' BYPASS' if mode == 'bypassPermissions' else '')}}})
out({'type': 'result', 'is_error': False, 'result': 'done'})
"#;

fn perm_count(events: &Stream) -> usize { sse_events(&events.text()).iter().filter(|e| e["kind"] == "permission").count() }
fn perms(events: &Stream) -> Vec<serde_json::Value> { sse_events(&events.text()).into_iter().filter(|e| e["kind"] == "permission").collect() }

#[test]
fn perms_flow() {
    let env = Env::new("perms");
    for d in ["proj/app", "elsewhere"] { std::fs::create_dir_all(env.t.join(d)).unwrap(); }
    env.stub("claude", CLAUDE);
    let sudolog = env.t.join("sudo.log");
    // stands in for passwordless sudo: logs what it was asked, answers "id -u" with 0, does not really chown or kill
    env.stub("sudo", &format!("#!/bin/sh\nprintf '%s\\n' \"$*\" >> {}\nif [ \"$1\" = \"-n\" ]; then shift; fi\ncase \"$1\" in chown|pkill) exit 0;; esac\nif [ \"$1\" = env ]; then for a in \"$@\"; do [ \"$a\" = id ] && {{ echo 0; exit 0; }}; done; fi\nexec \"$@\"\n", sudolog.display()));
    let relay = Relay::start(&env, Opts::new().args(&["--allow-bypass"]));
    let a = relay.api(); let tok = relay.token.clone();
    let mut c = Checks::new();
    let events = a.tstream("GET", "/events", &tok, None);
    let proj = env.proj.join("app");
    let proj_s = proj.to_string_lossy().to_string();
    let run = |text: &str, cwd: &str| a.tstream("POST", "/sessions/new/send", &tok, Some(json!({"text": text, "cwd": cwd})));
    let sudo = || std::fs::read_to_string(&sudolog).unwrap_or_default();

    // 1) the command is asked, "don't ask again" stores a rule, and the same kind of command is not asked again
    let r1 = run("npm test -- login", &proj_s); sleep(1500);
    let ev = perms(&events);
    c.ok_x("1 the first command is asked, with a suggested rule", ev.len() == 1 && ev[0]["suggest"]["label"] == "npm test …", ev.get(0).map(|e| e["suggest"].to_string()).unwrap_or_default());
    a.t("POST", "/decision", &tok, Some(json!({"req": ev[0]["req"], "behavior": "allow_always"}))); r1.wait();
    c.ok("2 it was allowed", re("RESULT allowed", &r1.text()));
    let mut o = a.t("GET", "/admin/options", &tok, None).j;
    c.ok("3 the rule is stored", o["rules"].as_array().map(|r| r.len()) == Some(1) && o["rules"][0]["prefix"] == "npm test");
    let before = perm_count(&events);
    let r2 = run("npm test -- other", &proj_s); r2.wait();
    c.ok("4 the same kind of command is allowed without asking", re("RESULT allowed", &r2.text()) && perm_count(&events) == before);
    let r3 = run("npm test; rm -rf ~", &proj_s); sleep(1500);
    c.ok("5 a compound command is asked again", perm_count(&events) == before + 1);
    // 2) Full auto while Claude is working: the waiting approval is allowed at once
    let pend = perms(&events).pop().unwrap();
    let au = a.t("POST", &format!("/sessions/{}/auto", jstr(&pend["session"])), &tok, Some(json!({"on": true})));
    c.ok("6 Full auto can be switched on during a run", au.code == 200 && au.j["auto"] == true);
    r3.wait(); c.ok("7 the waiting approval was allowed", re("RESULT allowed", &r3.text()));
    let n0 = perm_count(&events);
    let r4 = run("make deploy-everything", &proj_s); sleep(1500);
    let ev8 = perms(&events); c.ok("8 a new chat is not on Full auto: it asks", ev8.len() == n0 + 1);
    a.t("POST", "/decision", &tok, Some(json!({"req": ev8.last().unwrap()["req"], "behavior": "deny"}))); r4.wait();
    a.t("POST", "/admin/options", &tok, Some(json!({"removeRule": o["rules"][0]["id"]})));
    o = a.t("GET", "/admin/options", &tok, None).j;
    c.ok("9 a rule can be removed", o["rules"].as_array().map(|r| r.len()) == Some(0));
    c.ok("10 a dangerous rule cannot be added", a.t("POST", "/admin/options", &tok, Some(json!({"addPrefix": "rm -rf"}))).code == 400);
    // 3) work outside the projects folder
    let fsr = a.t("GET", &format!("/fs?path={}", env.t.display()), &tok, None).j;
    c.ok("11 the server can be browsed", fsr["dirs"].as_array().map(|d| d.iter().any(|x| x["name"] == "elsewhere")).unwrap_or(false));
    let mk = a.t("POST", "/fs/mkdir", &tok, Some(json!({"path": env.t.join("elsewhere").to_string_lossy(), "name": "newproject"})));
    c.ok("12 a folder can be made", mk.code == 200 && env.t.join("elsewhere/newproject").exists());
    c.ok("13 a folder name with a slash is refused", a.t("POST", "/fs/mkdir", &tok, Some(json!({"path": env.t.join("elsewhere").to_string_lossy(), "name": "../x"}))).code == 400);
    let out = env.t.join("elsewhere/newproject").to_string_lossy().to_string();
    let mut r5 = run("ls", &out); sleep(1500);
    { let pe = perms(&events).pop().unwrap(); a.t("POST", "/decision", &tok, Some(json!({"req": pe["req"], "behavior": "allow"}))); } r5.wait();
    let st = r5.text();
    c.ok_x("14 a chat can start outside the projects folder", st.contains(&format!("cwd={out}")), &st[st.len().saturating_sub(120)..]);
    let list = a.t("GET", "/sessions", &tok, None).j;
    let rp = std::fs::canonicalize(&out).unwrap().to_string_lossy().to_string();
    c.ok_x("15 that chat shows in the list", list.as_array().map(|l| l.iter().any(|x| x["cwd"] == json!(rp))).unwrap_or(false), list.as_array().map(|l| l.iter().map(|x| x["cwd"].to_string()).collect::<Vec<_>>().join(",")).unwrap_or_default());
    a.t("POST", "/admin/options", &tok, Some(json!({"outside": false})));
    r5 = run("ls", &out); r5.wait(); let st = r5.text();
    c.ok("16 it is refused again when the option is off", re("folder", &st) || st.is_empty() || !re("RESULT", &st));
    // 4) chats with root rights
    a.t("POST", "/admin/options", &tok, Some(json!({"outside": true})));
    let off = a.t("POST", "/sessions/new/send", &tok, Some(json!({"text": "ls", "cwd": proj_s, "root": true})));
    c.ok("17 root is refused while it is off", off.code == 403 && re("root rights are off", &off.text));
    let o2 = a.t("POST", "/admin/options", &tok, Some(json!({"root": true, "recheck": true}))).j;
    c.ok("18 root can be switched on and sudo is found by really running it", o2["root"] == true && o2["rootReady"]["ok"] == true && re("sudo works", &jstr(&o2["rootReady"]["how"])));
    c.ok_x("18b the check ran the same form of command as a root chat", re(r"-n env HOME=.* id -u", &sudo()), sudo().chars().take(300).collect::<String>());
    let a2 = run("ls", &proj_s);
    let rr = a.tstream("POST", "/sessions/new/send", &tok, Some(json!({"text": "make it so", "cwd": proj_s, "root": true, "mode": "bypassPermissions"}))); sleep(1500);
    for e in perms(&events) { a.t("POST", "/decision", &tok, Some(json!({"req": e["req"], "behavior": "allow"}))); }
    rr.wait(); a2.wait();
    let st = rr.text();
    c.ok_x("20 the root chat knows it has root", re("ROOTPROMPT", &st), &st[st.len().saturating_sub(160)..]);
    c.ok("21 Full auto is not passed to Claude as root (Hearth answers the approvals itself)", !re("BYPASS", &st) && re("RESULT allowed", &st));
    let ls2 = a.t("GET", "/sessions", &tok, None).j;
    c.ok("22 the chat is marked root in the list", ls2.as_array().map(|l| l.iter().any(|x| x["root"] == true)).unwrap_or(false));
    let rid = ls2.as_array().and_then(|l| l.iter().find(|x| x["root"] == true)).map(|x| jstr(&x["id"])).unwrap_or_default();
    sleep(500);
    c.ok("23 after a root run the Claude folder is given back to you", re(r"-n chown -R \d+:\d+ .*\.claude", &sudo()));
    c.ok("24 the root run went through sudo with the right folder and environment", re(r"-n env HOME=.* PATH=.* .*claude ", &sudo()));
    let root_of = |id: &str| a.t("GET", "/sessions", &tok, None).j.as_array().and_then(|l| l.iter().find(|x| x["id"] == json!(id)).map(|x| x["root"].clone())).unwrap_or(json!(null));
    let off1 = a.t("POST", &format!("/sessions/{rid}/root"), &tok, Some(json!({"on": false})));
    c.ok("25 an existing chat can lose its root rights", off1.code == 200 && root_of(&rid) == json!(false));
    let on1 = a.t("POST", &format!("/sessions/{rid}/root"), &tok, Some(json!({"on": true})));
    c.ok("26 and get them back", on1.code == 200 && root_of(&rid) == json!(true));
    // root is switched off while that chat has root rights: the chat is not lost, it goes on without root
    a.t("POST", "/admin/options", &tok, Some(json!({"root": false})));
    let cont = a.tstream("POST", &format!("/sessions/{rid}/send"), &tok, Some(json!({"text": "carry on", "mode": "acceptEdits"}))); sleep(1500);
    { let p2 = perms(&events); if let Some(l) = p2.last() { a.t("POST", "/decision", &tok, Some(json!({"req": l["req"], "behavior": "allow"}))); } } cont.wait();
    let st = cont.text();
    c.ok_x("27 an old root chat goes on without root, and Claude is told so, when root is switched off", re("RESULT", &st) && !re("ROOTPROMPT", &st) && re("DROPPED", &st), &st[st.len().saturating_sub(140)..]);
    c.ok("28 giving a chat root rights is refused while root is off", a.t("POST", &format!("/sessions/{rid}/root"), &tok, Some(json!({"on": true}))).code == 403);
    c.ok("29 a guest key cannot touch root", a.t("POST", &format!("/sessions/{rid}/root"), "share.abcd.abcdefghijklmnop", Some(json!({"on": true}))).code == 401);
    // a chat on Full auto stays on bypassPermissions whatever the app sends
    {
        let _n1 = run("echo full", &proj_s); sleep(1200);
        let sl = a.t("GET", "/sessions", &tok, None).j;
        let sid1 = sl.as_array().and_then(|l| l.get(0)).map(|x| jstr(&x["id"])).filter(|s| !s.is_empty());
        a.t("POST", "/admin/options", &tok, Some(json!({"bypass": true})));
        if let Some(sid1) = sid1 {
            a.t("POST", &format!("/sessions/{sid1}/auto"), &tok, Some(json!({"on": true}))); sleep(300);
            let f1 = a.tstream("POST", &format!("/sessions/{sid1}/send"), &tok, Some(json!({"text": "again", "mode": "auto"}))); sleep(1500);
            c.ok_x("30 a chat on Full auto is run on bypassPermissions even when the app sends \"auto\"", re("BYPASS", &f1.text()), f1.text().chars().take(120).collect::<String>());
        } else { c.ok_x("30 a chat on Full auto is run on bypassPermissions even when the app sends \"auto\"", false, "no chat"); }
    }
    c.finish();
}
