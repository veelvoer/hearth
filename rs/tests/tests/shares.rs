// Shared chats: a normal chat that friends can open. Their prompts are requests the host approves (and may edit), or runs at once with full permission. Fake "claude".
use hearth_tests::*;
use serde_json::{json, Value};

const CLAUDE: &str = r#"#!/usr/bin/env python3
import sys, os, json, re, time, datetime
a = sys.argv
sid = a[a.index('--resume') + 1] if '--resume' in a else 'fake-%d' % os.getpid()
text = sys.stdin.read()
cwd = os.getcwd()
pd = os.path.join(os.environ['HOME'], '.claude', 'projects', re.sub('[^A-Za-z0-9]', '-', cwd)); os.makedirs(pd, exist_ok=True)
def ts(): return datetime.datetime.utcnow().strftime('%Y-%m-%dT%H:%M:%S.%f')[:-3] + 'Z'
f = os.path.join(pd, sid + '.jsonl')
mode = a[a.index('--permission-mode') + 1] if '--permission-mode' in a else 'undefined'
reply = 'MODE=' + mode + ' | ' + text.strip()
def out(o): sys.stdout.write(json.dumps(o) + '\n'); sys.stdout.flush()
out({'type': 'system', 'subtype': 'init', 'session_id': sid, 'model': 'fake'})
open(f, 'a').write(json.dumps({'type': 'user', 'cwd': cwd, 'timestamp': ts(), 'message': {'content': text.strip()}}) + '\n')
time.sleep(1.8 if 'HOLD' in text else 0.05)
open(f, 'a').write(json.dumps({'type': 'assistant', 'cwd': cwd, 'timestamp': ts(), 'message': {'id': 'm%d' % int(time.time() * 1000), 'model': 'claude-fake', 'content': [{'type': 'text', 'text': reply}], 'usage': {'input_tokens': 1, 'output_tokens': 1}}}) + '\n')
out({'type': 'result', 'is_error': False, 'result': reply})
"#;

fn arr(v: &Value) -> Vec<Value> { v.as_array().cloned().unwrap_or_default() }
fn sl(s: &str, n: usize) -> String { s.chars().take(n).collect() }

#[test]
fn shares() {
    let env = Env::new("shares");
    std::fs::create_dir_all(env.proj.join("app")).unwrap();
    env.stub("claude", CLAUDE);
    let start = |port: u16| Relay::start(&env, Opts::new().port(port).args(&["--name", "Warre"]));
    let mut relay = start(0);
    let port = relay.port;
    let a = relay.api(); let tok = relay.token.clone();
    let mut c = Checks::new();
    let rq = |m: &str, p: &str, t: &str, b: Option<Value>| a.t(m, p, t, b);
    let events = a.tstream("GET", "/events", &tok, None);
    let cwd = env.proj.join("app").to_string_lossy().to_string();
    // a normal chat of the host
    let first = rq("POST", "/sessions/new/send", &tok, Some(json!({"text": "start the landing page", "cwd": cwd, "mode": "acceptEdits"})));
    let sid = regex::Regex::new(r#""id":"(fake-\d+)""#).unwrap().captures(&first.text).map(|m| m[1].to_string());
    c.ok_x("1 the host has a normal chat", sid.is_some(), sid.clone().unwrap_or_default());
    let sid = sid.unwrap_or_default();
    sleep(300);
    let sh = rq("POST", &format!("/sessions/{sid}/share"), &tok, Some(json!({})));
    let shid = jstr(&sh.j["id"]);
    c.ok_x("2 the host shares it", sh.code == 200 && sh.j["shared"] == true && arr(&sh.j["members"]).is_empty(), sl(&sh.text, 100));
    let again = rq("POST", &format!("/sessions/{sid}/share"), &tok, Some(json!({})));
    c.ok("3 sharing again keeps the same share", again.j["id"] == sh.j["id"]);
    let lst = rq("GET", "/sessions", &tok, None);
    c.ok("4 the list tells which chats are shared", arr(&lst.j).iter().any(|x| x["id"] == json!(sid) && x["shared"] == 0));
    let inv = rq("POST", &format!("/shares/{shid}/invite"), &tok, Some(json!({})));
    let join = rq("POST", "/shares/join", "", Some(json!({"invite": inv.j["invite"], "name": "Sam"})));
    c.ok_x("5 a friend joins with the invite", join.code == 200 && jstr(&join.j["key"]).starts_with("share.") && join.j["session"] == json!(sid), sl(&join.text, 80));
    let g = jstr(&join.j["key"]); let mid = jstr(&join.j["member"]);
    c.ok("6 an invite works only once", rq("POST", "/shares/join", "", Some(json!({"invite": inv.j["invite"], "name": "Eve"}))).code == 404);
    // what the guest may see and do
    let gl = rq("GET", "/sessions", &g, None);
    let t_s = env.t.to_string_lossy().to_string();
    c.ok("7 the guest sees only the shared chat, and not the folder", gl.code == 200 && arr(&gl.j).len() == 1 && gl.j[0]["id"] == json!(sid) && !jstr(&gl.j[0]["cwd"]).contains(&t_s));
    let gm = rq("GET", &format!("/sessions/{sid}/messages"), &g, None);
    c.ok("8 the guest reads the chat like a normal one", gm.code == 200 && arr(&gm.j).iter().any(|m| m["role"] == "user" && re("landing page", &jstr(&m["text"]))) && arr(&gm.j).iter().any(|m| m["role"] == "assistant"));
    let deny: Vec<(&str, String)> = vec![("GET", "/status".into()), ("GET", "/projects".into()), ("GET", "/admin/options".into()), ("GET", "/usage".into()), ("POST", "/admin/options".into()), ("GET", "/fs".into()),
        ("GET", format!("/shares/{shid}")), ("POST", format!("/shares/{shid}/invite")), ("GET", "/rooms-sync".into()), ("GET", "/nothing".into()), ("GET", "/sessions/other/messages".into())];
    for (m, p) in &deny {
        let r = rq(m, p, &g, if *m == "POST" { Some(json!({})) } else { None });
        c.ok_x(&format!("9 a guest key cannot {m} {p}"), r.code == 403 || r.code == 401, r.code.to_string());
    }
    let pr = rq("GET", &format!("/sessions/{sid}/prefs"), &g, None);
    c.ok("10 the guest sees their own status", pr.j["shared"]["myName"] == "Sam" && pr.j["shared"]["grant"]["auto"] == false);
    // a prompt from the guest is a request, nothing runs
    let msgs_before = arr(&rq("GET", &format!("/sessions/{sid}/messages"), &tok, None).j).len();
    let snd = rq("POST", &format!("/sessions/{sid}/send"), &g, Some(json!({"text": "please add a footer"})));
    c.ok("11 the guest sends a prompt: it waits for approval", re(r#""t":"queued""#, &snd.text) && re(r#""approval":true"#, &snd.text));
    sleep(500);
    let reqs = || arr(&rq("GET", "/pending", &tok, None).j).into_iter().filter(|e| e["kind"] == "request").collect::<Vec<_>>();
    let pend = reqs();
    c.ok_x("12 the host sees the request in the waiting list", pend.len() == 1 && pend[0]["session"] == json!(sid) && re("Sam: please add a footer", &jstr(&pend[0]["text"])), sl(&pend.get(0).map(|p| p.to_string()).unwrap_or_default(), 120));
    let live: Vec<Value> = sse_events(&events.text()).into_iter().filter(|e| e["kind"] == "request").collect();
    c.ok("13 and gets a live notification event", live.len() == 1 && live[0]["req"] == pend[0]["req"] && live[0]["from"] == "Sam");
    c.ok("14 nothing ran yet", arr(&rq("GET", &format!("/sessions/{sid}/messages"), &tok, None).j).len() == msgs_before);
    let mine = rq("GET", &format!("/sessions/{sid}/prefs"), &g, None);
    c.ok("15 the guest sees their request waiting", mine.j["shared"]["requests"][0]["status"] == "pending");
    // the host edits and approves
    let dec = rq("POST", "/decision", &tok, Some(json!({"req": pend[0]["req"], "behavior": "allow", "text": "please add a small footer", "mode": "plan"})));
    c.ok("16 the host approves with an edited prompt", dec.code == 200);
    sleep(1200);
    let after = arr(&rq("GET", &format!("/sessions/{sid}/messages"), &g, None).j);
    c.ok_x("17 it ran in the same chat, as that person, in plan mode", after.iter().any(|m| m["role"] == "user" && m["text"] == "Sam: please add a small footer") && after.iter().any(|m| m["role"] == "assistant" && re("MODE=plan", &jstr(&m["text"]))),
        sl(&json!(after[after.len().saturating_sub(2)..]).to_string(), 200));
    c.ok("18 the resolved event went out", sse_events(&events.text()).iter().any(|e| e["kind"] == "resolved" && e["req"] == pend[0]["req"]));
    let mine2 = rq("GET", &format!("/sessions/{sid}/prefs"), &g, None);
    let r0 = &mine2.j["shared"]["requests"][0];
    c.ok_x("19 the guest sees it was changed and ran", (r0["status"] == "running" || r0["status"] == "done") && r0["edited"] == true, jstr(&r0["status"]));
    // deny
    sleep(1500); rq("POST", &format!("/sessions/{sid}/send"), &g, Some(json!({"text": "delete everything"}))); sleep(300);
    let p2 = reqs(); rq("POST", "/decision", &tok, Some(json!({"req": p2[0]["req"], "behavior": "deny"}))); sleep(300);
    c.ok("20 a denied prompt never runs", !arr(&rq("GET", &format!("/sessions/{sid}/messages"), &tok, None).j).iter().any(|m| re("delete everything", &jstr(&m["text"])))
        && rq("GET", &format!("/sessions/{sid}/prefs"), &g, None).j["shared"]["requests"][0]["status"] == "denied");
    c.ok("21 a request cannot be decided twice", rq("POST", "/decision", &tok, Some(json!({"req": p2[0]["req"], "behavior": "allow"}))).code == 409);
    // full permission: runs at once
    sleep(1600);
    let gr = rq("POST", &format!("/shares/{shid}/members/{mid}"), &tok, Some(json!({"grant": {"auto": true, "mode": "acceptEdits"}})));
    c.ok("22 the host gives full permission", gr.code == 200 && gr.j["grant"]["auto"] == true);
    let s3 = rq("POST", &format!("/sessions/{sid}/send"), &g, Some(json!({"text": "third, no asking"})));
    c.ok("23 with full permission it is not a request", re(r#""auto":true"#, &s3.text));
    sleep(1200);
    c.ok("24 it ran at once, in the granted mode", arr(&rq("GET", &format!("/sessions/{sid}/messages"), &tok, None).j).iter().any(|m| m["role"] == "assistant" && re(r"MODE=acceptEdits \| .*Sam: third, no asking", &jstr(&m["text"]))));
    let fa = rq("POST", &format!("/shares/{shid}/members/{mid}"), &tok, Some(json!({"grant": {"auto": true, "mode": "bypassPermissions"}})));
    c.ok("25 Full auto for a friend needs the server to allow it", fa.j["grant"]["mode"] == "acceptEdits");
    // busy chat: the approved prompt waits its turn; the guest can stop only their own run
    sleep(1600);
    let host2 = a.tstream("POST", &format!("/sessions/{sid}/send"), &tok, Some(json!({"text": "host HOLD a while", "mode": "acceptEdits"}))); sleep(500);
    rq("POST", &format!("/sessions/{sid}/send"), &g, Some(json!({"text": "queued behind the host"})));
    let stop_other = rq("POST", "/stop", &g, Some(json!({"session": sid})));
    c.ok("26 a guest cannot stop the host's own run", stop_other.code == 403);
    host2.wait(); sleep(1500);
    c.ok("27 the prompt that had to wait ran after the host's", arr(&rq("GET", &format!("/sessions/{sid}/messages"), &tok, None).j).iter().any(|m| m["role"] == "assistant" && re("queued behind the host", &jstr(&m["text"]))));
    // the other direction: the host asks a friend to use THE FRIEND'S Claude
    rq("POST", &format!("/shares/{shid}/members/{mid}"), &tok, Some(json!({"grant": {"auto": false, "mode": "plan"}})));
    let gevents = a.tstream("GET", "/events", &g, None); sleep(400);
    sleep(1600);
    let ask = rq("POST", &format!("/shares/{shid}/ask"), &tok, Some(json!({"to": mid, "text": "host wants the friend's usage"})));
    c.ok("34 the host asks the friend's Claude", ask.code == 200 && ask.j["status"] == "pending");
    sleep(300);
    let askid = jstr(&ask.j["id"]);
    let gev: Vec<Value> = gevents.text().lines().filter(|l| l.starts_with("data: ")).filter_map(|l| serde_json::from_str::<Value>(&l[6..]).ok()).filter(|e| e["kind"] == "request").collect();
    c.ok_x("35 the friend gets a notification event", gev.len() == 1 && gev[0]["forMember"] == json!(mid) && re("host wants the friend", &jstr(&gev[0]["text"])), sl(&gev.get(0).map(|e| e.to_string()).unwrap_or("{}".into()), 100));
    c.ok("36 the host's own waiting list does not show it", !arr(&rq("GET", "/pending", &tok, None).j).iter().any(|e| e["req"] == json!(askid)));
    let gi = rq("GET", &format!("/sessions/{sid}/prefs"), &g, None).j["shared"].clone();
    c.ok("37 the friend sees it as incoming", arr(&gi["incoming"]).len() == 1 && gi["incoming"][0]["fromName"] == "Warre" && arr(&gi["people"]).iter().any(|p| p["id"] == "host"));
    let inv3 = rq("POST", &format!("/shares/{shid}/invite"), &tok, Some(json!({})));
    let j3 = rq("POST", "/shares/join", "", Some(json!({"invite": inv3.j["invite"], "name": "Kim"})));
    let wrong = rq("POST", &format!("/sessions/{sid}/requests/{askid}"), &jstr(&j3.j["key"]), Some(json!({"decision": "approve"})));
    c.ok("38 another friend cannot decide for them", wrong.code == 404);
    let fd2 = rq("POST", &format!("/sessions/{sid}/requests/{askid}"), &g, Some(json!({"decision": "approve", "text": "host wants the friend's usage, edited", "mode": "plan"})));
    c.ok("39 the friend approves and edits", fd2.code == 200 && re("edited", &jstr(&fd2.j["text"])) && fd2.j["from"] == "Warre");
    let pu = rq("POST", &format!("/sessions/{sid}/post"), &g, Some(json!({"role": "user", "text": "host wants the friend's usage, edited", "req": askid})));
    let pa = rq("POST", &format!("/sessions/{sid}/post"), &g, Some(json!({"role": "assistant", "text": "answer from the friend's own account", "req": askid})));
    c.ok("40 the friend's app posts the prompt and the answer", pu.code == 200 && pa.code == 200);
    rq("POST", &format!("/sessions/{sid}/requests/{askid}"), &g, Some(json!({"decision": "done"})));
    let mh = arr(&rq("GET", &format!("/sessions/{sid}/messages"), &tok, None).j); let mg = arr(&rq("GET", &format!("/sessions/{sid}/messages"), &g, None).j);
    c.ok("41 everyone sees them in the chat, with who they are", [&mh, &mg].iter().all(|l| l.iter().any(|m| m["role"] == "user" && m["name"] == "Warre" && re("edited", &jstr(&m["text"]))) && l.iter().any(|m| m["role"] == "assistant" && m["name"] == "Sam's Claude for Warre")));
    let own = rq("POST", &format!("/sessions/{sid}/post"), &g, Some(json!({"role": "assistant", "text": "my own question went to my own Claude"})));
    c.ok("42 a friend can post what their own Claude said", own.code == 200 && arr(&rq("GET", &format!("/sessions/{sid}/messages"), &tok, None).j).iter().any(|m| m["name"] == "Sam's Claude"));
    let fake = rq("POST", &format!("/sessions/{sid}/post"), &g, Some(json!({"role": "user", "text": "x", "req": "nope"})));
    c.ok("43 posting as somebody else is refused", fake.code == 403);
    rq("POST", &format!("/sessions/{sid}/working"), &g, Some(json!({"on": true})));
    c.ok("44 who is working shows", arr(&rq("GET", &format!("/sessions/{sid}/messages"), &tok, None).j).iter().any(|m| m["role"] == "meta" && re("Sam's Claude is working", &jstr(&m["text"]))));
    rq("POST", &format!("/sessions/{sid}/working"), &g, Some(json!({"on": false})));
    sleep(1600); rq("POST", &format!("/sessions/{sid}/send"), &tok, Some(json!({"text": "host asks directly again", "mode": "acceptEdits"}))); sleep(1500);
    c.ok("45 the host's own Claude hears what the others did with theirs (the fake Claude repeats what it was told)", arr(&rq("GET", &format!("/sessions/{sid}/messages"), &tok, None).j).iter().any(|m| m["role"] == "assistant" && re("others used their own Claude", &jstr(&m["text"])) && re("answer from the friend's own account", &jstr(&m["text"]))));
    let mine3 = rq("POST", &format!("/sessions/{sid}/ask"), &g, Some(json!({"to": "host", "text": "friend asks host via ask"})));
    c.ok("46 a friend can also ask the host through \"ask\"", mine3.code == 200);
    // everyone sees the same chat
    let inv4 = rq("POST", &format!("/shares/{shid}/invite"), &tok, Some(json!({})));
    let j4 = rq("POST", "/shares/join", "", Some(json!({"invite": inv4.j["invite"], "name": "Lee"})));
    let strip = |l: Vec<Value>| -> Vec<Value> { l.into_iter().filter(|m| m["role"] != "meta" || !re("working", &jstr(&m["text"]))).map(|m| json!([m["role"], m["text"], if m["name"].is_null() || m["name"] == "" { json!("") } else { m["name"].clone() }, if m["ts"].is_null() { json!(0) } else { m["ts"].clone() }])).collect() };
    let aa = strip(arr(&rq("GET", &format!("/sessions/{sid}/messages"), &tok, None).j));
    let bb = strip(arr(&rq("GET", &format!("/sessions/{sid}/messages"), &g, None).j));
    let cc = strip(arr(&rq("GET", &format!("/sessions/{sid}/messages"), &jstr(&j4.j["key"]), None).j));
    c.ok_x("47 host and two friends get exactly the same messages", aa == bb && bb == cc, format!("{} {} {}", json!(aa).to_string().len(), json!(bb).to_string().len(), json!(cc).to_string().len()));
    c.ok("48 the note for the host's Claude is not shown as a message of the host", !aa.iter().any(|m| m[0] == "user" && re("others used their own Claude", &jstr(&m[1]))));
    let order: Vec<f64> = aa.iter().map(|m| m[3].as_f64().unwrap_or(0.0)).collect();
    c.ok("49 messages are in time order", order.iter().enumerate().all(|(i, t)| i == 0 || *t >= order[i - 1] || order[i - 1] == 0.0 || *t == 0.0));
    let gtitle = arr(&rq("GET", "/sessions", &g, None).j).get(0).map(|s| s["title"].clone()).unwrap_or(json!(null));
    let htitle = arr(&rq("GET", "/sessions", &tok, None).j).into_iter().find(|x| x["id"] == json!(sid)).map(|s| s["title"].clone()).unwrap_or(json!(0));
    c.ok("50 the list entry of the chat is the same title for everyone", gtitle == htitle);
    rq("DELETE", &format!("/shares/{shid}/members/{}", jstr(&j4.j["member"])), &tok, None);
    // restart: waiting requests survive
    rq("POST", &format!("/shares/{shid}/members/{mid}"), &tok, Some(json!({"grant": {"auto": false, "mode": "plan"}}))); sleep(1600);
    rq("POST", &format!("/sessions/{sid}/send"), &g, Some(json!({"text": "survives a restart"}))); sleep(300);
    drop(events); drop(gevents);
    relay.kill(); sleep(600); relay = start(port); sleep(2000);
    let p3 = reqs();
    c.ok("28 a waiting request survives a restart", p3.iter().any(|e| re("survives a restart", &jstr(&e["text"]))));
    let gv = rq("GET", &format!("/sessions/{sid}/messages"), &g, None); c.ok("29 and the guest key still works", gv.code == 200);
    // removing a friend
    let rm = rq("DELETE", &format!("/shares/{shid}/members/{mid}"), &tok, None);
    c.ok("30 a removed friend is locked out", rm.code == 200 && rq("GET", &format!("/sessions/{sid}/messages"), &g, None).code == 401);
    c.ok("31 and their waiting prompts are gone", reqs().is_empty());
    let un = rq("DELETE", &format!("/shares/{shid}"), &tok, None);
    c.ok("32 the host can stop sharing", un.code == 200 && rq("GET", &format!("/sessions/{sid}/share"), &tok, None).j["shared"] == false);
    // the list of shared chats kept on the server for all devices
    let s1 = rq("POST", "/rooms-sync", &tok, Some(json!({"items": {"g:a:b": {"v": {"name": "x"}, "t": 100}}})));
    c.ok("33 the synced list works", s1.code == 200 && s1.j["items"]["g:a:b"]["v"]["name"] == "x");
    let _ = relay;
    c.finish();
}
