// Routines: a goal that Claude works on by itself on a schedule, and the timeline of what it did.
use hearth_tests::*;
use serde_json::{json, Value};

const CLAUDE: &str = r#"#!/usr/bin/env python3
import sys, os, json, re, time, random, string
sid = 'sid-' + ''.join(random.choice(string.ascii_lowercase + string.digits) for _ in range(6))
def out(o): sys.stdout.write(json.dumps(o) + '\n'); sys.stdout.flush()
out({'type': 'system', 'subtype': 'init', 'session_id': sid, 'model': 'x'})
text = sys.stdin.read()
time.sleep(1.5)
m = re.search(r'routine "(.*?)"', text)
out({'type': 'result', 'subtype': 'success', 'is_error': False, 'result': 'Found 3 things. ' + (m.group(1) if m else 'undefined'), 'session_id': sid})
"#;

fn arr(v: &Value) -> Vec<Value> { v.as_array().cloned().unwrap_or_default() }

#[test]
fn routines() {
    let env = Env::new("routines");
    let proj = env.proj.join("app"); std::fs::create_dir_all(&proj).unwrap();
    let proj = proj.to_string_lossy().to_string();
    env.stub("claude", CLAUDE);
    let relay = Relay::start(&env, Opts::new().env("HEARTH_MEM_CHILD", "1").env("HEARTH_ROUTINE_TICK_MS", "300"));
    let a = relay.api(); let tok = relay.token.clone();
    let mut c = Checks::new();
    let runs = || arr(&a.t("GET", "/routines/timeline", &tok, None).j["runs"]);
    c.ok("1 a routine needs a name and a goal", a.t("POST", "/routines", &tok, Some(json!({"add": {"name": "", "goal": "x"}}))).code == 400);
    a.t("POST", "/admin/options", &tok, Some(json!({"outside": false})));
    c.ok("2 a folder outside the projects folder is refused", a.t("POST", "/routines", &tok, Some(json!({"add": {"name": "x", "goal": "y", "cwd": "/etc"}}))).code == 400);
    let off = a.t("POST", "/routines", &tok, Some(json!({"add": {"name": "Off", "goal": "never", "cwd": proj, "enabled": false, "schedule": {"type": "interval", "minutes": 0.005}}})));
    let on = a.t("POST", "/routines", &tok, Some(json!({"add": {"name": "Digest", "goal": "Summarise yesterday.", "cwd": proj, "mode": "plan", "schedule": {"type": "interval", "minutes": 0.005}}})));
    let find = |j: &Value, n: &str| arr(&j["routines"]).into_iter().find(|r| r["name"] == n).unwrap_or(json!({}));
    c.ok("3 routines are saved and listed, the plan mode is the default", on.code == 200 && arr(&on.j["routines"]).len() == 2 && find(&on.j, "Digest")["mode"] == "plan");
    let ran = wait_for(12000, || runs().iter().any(|r| r["name"] == "Digest" && r["status"] == "done"));
    let rs = runs();
    let done = rs.iter().find(|r| r["status"] == "done").cloned().unwrap_or(json!({}));
    c.ok_x("4 a routine that is due runs by itself, and the timeline has what it found", ran && re(r"Found 3 things\. Digest", &jstr(&done["summary"])) && jstr(&done["session"]).starts_with("sid-"), rs.get(0).map(|r| r.to_string().chars().take(160).collect::<String>()).unwrap_or_default());
    { let sid = jstr(&done["session"]); let l = a.t("GET", "/sessions", &tok, None).j; c.ok_x("4b chats made by a routine are not in the main chat list", l.is_array() && !arr(&l).iter().any(|x| x["id"] == json!(sid)), sid); }
    c.ok("5 a routine that is switched off never runs", !rs.iter().any(|r| r["name"] == "Off"));
    let mut d: Vec<f64> = rs.iter().filter(|r| r["name"] == "Digest").map(|r| r["at"].as_f64().unwrap_or(0.0)).collect(); d.sort_by(|x, y| x.partial_cmp(y).unwrap());
    c.ok("6 it does not start again while the last run is still going", d.iter().enumerate().all(|(i, t)| i == 0 || t - d[i - 1] >= 1400.0));
    let id = jstr(&find(&on.j, "Digest")["id"]);
    a.t("POST", "/routines", &tok, Some(json!({"update": {"id": id, "enabled": false}})));
    sleep(2500); let n = runs().len(); sleep(1500);
    c.ok("7 switching it off stops the runs", runs().len() == n);
    let off_id = jstr(&find(&off.j, "Off")["id"]);
    let now = a.t("POST", "/routines", &tok, Some(json!({"run": off_id})));
    c.ok("8 \"run now\" works even for a routine that is off", now.code == 200 && now.j["run"]["why"] == "by you");
    let again = a.t("POST", "/routines", &tok, Some(json!({"run": off_id})));
    c.ok("9 and says no when it is still running", again.code == 409);
    let t = now_ms() - 60e3; let (day, at) = local_day_hm(t); let (today, _) = local_day_hm(now_ms());
    if day == today {
        a.t("POST", "/routines", &tok, Some(json!({"add": {"name": "Daily", "goal": "Check the disks.", "cwd": proj, "schedule": {"type": "daily", "at": at}}}))); sleep(2500); sleep(2500);
        let cnt = runs().iter().filter(|r| r["name"] == "Daily").count();
        c.ok_x("10 a daily routine whose time has passed runs once today, not again", cnt == 1, cnt.to_string());
    } else { c.ok("10 skipped just after midnight", true); }
    {
        let r = a.t("POST", "/memory", &tok, Some(json!({"add": "  I use   metric units "}))); a.t("POST", "/memory", &tok, Some(json!({"add": "I use metric units"})));
        let l = arr(&a.t("GET", "/memory", &tok, None).j["notes"]);
        c.ok("10b a memory note is saved once, tidied", r.code == 200 && l.len() == 1 && l[0]["text"] == "I use metric units");
        c.ok("10c an empty note is refused", a.t("POST", "/memory", &tok, Some(json!({"add": "   "}))).code == 400);
        c.ok("10d a memory note can be removed", arr(&a.t("POST", "/memory", &tok, Some(json!({"remove": l[0]["id"]}))).j["notes"]).is_empty());
    }
    a.t("POST", "/routines", &tok, Some(json!({"remove": id})));
    c.ok("11 a routine can be removed", !arr(&a.t("GET", "/routines", &tok, None).j["routines"]).iter().any(|r| r["id"] == json!(id)));
    c.ok("12 a guest key cannot use routines", a.t("GET", "/routines", "share.abcd.abcdefghijklmnop", None).code == 401 && a.t("POST", "/routines", "share.abcd.abcdefghijklmnop", Some(json!({"run": "x"}))).code == 401);
    c.finish();
}
