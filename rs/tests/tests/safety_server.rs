// The safety features through the server: a snapshot before a run and undo, the budget, backups, and the secrets check.
use hearth_tests::*;
use serde_json::{json, Value};
use std::fs;

const CLAUDE: &str = r#"#!/usr/bin/env python3
import sys, os, json, re, time, random, string, datetime
cwd = os.getcwd(); sid = 'sid-' + ''.join(random.choice(string.ascii_lowercase + string.digits) for _ in range(6))
d = os.path.join(os.environ['HOME'], '.claude', 'projects', re.sub('[^A-Za-z0-9]', '-', cwd)); os.makedirs(d, exist_ok=True)
open(os.path.join(cwd, 'a.txt'), 'w').write('CHANGED'); open(os.path.join(cwd, 'b.txt'), 'w').write('new')
try: os.unlink(os.path.join(cwd, 'c.txt'))
except Exception: pass
open(os.path.join(d, sid + '.jsonl'), 'w').write(json.dumps({'type': 'user', 'cwd': cwd, 'sessionId': sid, 'timestamp': datetime.datetime.utcnow().isoformat() + 'Z', 'message': {'role': 'user', 'content': 'do it'}}) + '\n')
def out(o): sys.stdout.write(json.dumps(o) + '\n'); sys.stdout.flush()
out({'type': 'system', 'subtype': 'init', 'session_id': sid, 'model': 'x'}); out({'type': 'result', 'subtype': 'success', 'is_error': False, 'result': 'ok', 'session_id': sid})
sys.stdin.read()
time.sleep(0.2)
"#;

fn arr(v: &Value) -> Vec<Value> { v.as_array().cloned().unwrap_or_default() }

#[test]
fn safety_server() {
    let env = Env::new("safe");
    let proj = env.proj.join("app"); fs::create_dir_all(&proj).unwrap();
    fs::write(proj.join("a.txt"), "original a").unwrap(); fs::write(proj.join("c.txt"), "original c").unwrap(); fs::write(proj.join("key.env"), "K=AKIAABCDEFGHIJKLMNOP").unwrap();
    let ps = proj.to_string_lossy().to_string();
    env.stub("claude", CLAUDE);
    let relay = Relay::start(&env, Opts::new().env("HEARTH_MEM_CHILD", "1"));
    let a = relay.api(); let tok = relay.token.clone();
    let mut c = Checks::new();
    let run = a.t("POST", "/sessions/new/send", &tok, Some(json!({"text": "change things", "mode": "acceptEdits", "cwd": ps}))); sleep(900);
    let sid = regex::Regex::new(r#""id":"(sid-[a-z0-9]+)""#).unwrap().captures(&run.text).map(|m| m[1].to_string()).unwrap_or_default();
    c.ok_x("1 the run changed the project", fs::read_to_string(proj.join("a.txt")).unwrap() == "CHANGED" && proj.join("b.txt").exists() && !proj.join("c.txt").exists(), sid.clone());
    let snaps = a.t("GET", &format!("/sessions/{sid}/snapshots"), &tok, None).j;
    c.ok_x("2 a snapshot was taken before the run", snaps.is_array() && !arr(&snaps).is_empty() && snaps[0]["label"] == "before a run" && snaps[0]["text"] == "change things", snaps.to_string().chars().take(120).collect::<String>());
    let ch = a.t("GET", &format!("/sessions/{sid}/changes"), &tok, None).j;
    c.ok("2b a folder without git says so", ch["git"] == false && ch["files"].is_array());
    {
        let g = env.t.join("gitproj"); fs::create_dir_all(&g).unwrap();
        let sh = |args: &[&str]| { let _ = std::process::Command::new("git").arg("-C").arg(&g).args(["-c", "user.name=t", "-c", "user.email=t@t"]).args(args).output(); };
        sh(&["init", "-q"]); fs::write(g.join("x.txt"), "one\n").unwrap(); sh(&["add", "."]); sh(&["commit", "-qm", "i"]);
        fs::write(g.join("x.txt"), "two\n").unwrap(); fs::write(g.join("new.txt"), "n").unwrap();
        let r = hearth_relay::safety::git_changes(&g.to_string_lossy());
        let files = arr(&r["files"]);
        c.ok_x("2c changes lists changed and new files with a diff", r["git"] == true && files.iter().any(|f| f["path"] == "x.txt" && f["status"] == "changed") && files.iter().any(|f| f["path"] == "new.txt" && f["status"] == "new") && re("-one", &jstr(&r["diff"])) && re(r"\+two", &jstr(&r["diff"])), r["files"].to_string());
    }
    {
        let x = a.t("POST", "/admin/safety", &tok, Some(json!({"plain": true}))); let y = a.t("GET", "/admin/safety", &tok, None).j;
        c.ok("2d plain language is switched on and remembered", x.j["plain"] == true && y["plain"] == true); a.t("POST", "/admin/safety", &tok, Some(json!({"plain": false})));
    }
    let un = a.t("POST", &format!("/sessions/{sid}/undo"), &tok, Some(json!({})));
    c.ok_x("3 undo puts everything back", un.code == 200 && fs::read_to_string(proj.join("a.txt")).unwrap() == "original a" && proj.join("c.txt").exists() && !proj.join("b.txt").exists(), un.j.to_string().chars().take(100).collect::<String>());
    c.ok("4 a guest key cannot undo or read snapshots", a.t("POST", &format!("/sessions/{sid}/undo"), "share.abcd.abcdefghijklmnop", Some(json!({}))).code == 401);
    c.ok("5 undo without any snapshot says so", a.t("POST", "/sessions/nope/undo", &tok, Some(json!({}))).code == 404);
    // snapshots can be switched off
    a.t("POST", "/admin/safety", &tok, Some(json!({"snapshots": false}))); let before = arr(&a.t("GET", &format!("/sessions/{sid}/snapshots"), &tok, None).j).len();
    a.t("POST", "/sessions/new/send", &tok, Some(json!({"text": "again", "mode": "acceptEdits", "cwd": ps}))); sleep(900);
    c.ok("6 with snapshots off no new snapshot is made", arr(&a.t("GET", &format!("/sessions/{sid}/snapshots"), &tok, None).j).len() == before);
    a.t("POST", "/admin/safety", &tok, Some(json!({"snapshots": true})));
    // budget: a chat that already cost a lot today
    let big = env.home.join(".claude/projects/-big"); fs::create_dir_all(&big).unwrap();
    let now_iso = { let secs = (now_ms() / 1000.0) as i64; let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400)); let z = days + 719468; let era = z.div_euclid(146097); let doe = z.rem_euclid(146097); let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; let y = yoe + era * 400; let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); let mp = (5 * doy + 2) / 153; let d = doy - (153 * mp + 2) / 5 + 1; let m = if mp < 10 { mp + 3 } else { mp - 9 }; format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.000Z", if m <= 2 { y + 1 } else { y }, m, d, rem / 3600, rem % 3600 / 60, rem % 60) };
    fs::write(big.join("big.jsonl"), json!({"type":"assistant","cwd":"/big","sessionId":"big","timestamp":now_iso,"message":{"id":"m1","role":"assistant","model":"claude-opus-4-1","usage":{"input_tokens":1000,"output_tokens":2000000},"content":[{"type":"text","text":"x"}]}}).to_string() + "\n").unwrap();
    let set = a.t("POST", "/admin/safety", &tok, Some(json!({"budget": {"perDay": 1, "perChat": 0}})));
    c.ok_x("7 the budget is saved, with what was used today", set.code == 200 && set.j["budget"]["perDay"] == 1 && set.j["budget"]["usedDay"].as_f64().unwrap_or(0.0) > 1.0, set.j["budget"].to_string());
    let blocked = a.t("POST", "/sessions/new/send", &tok, Some(json!({"text": "more", "mode": "acceptEdits", "cwd": ps})));
    c.ok_x("8 a run is refused once the daily budget is used up, with a clear message", blocked.code == 429 && re("daily budget", &jstr(&blocked.j["error"])), blocked.text.chars().take(120).collect::<String>());
    a.t("POST", "/admin/safety", &tok, Some(json!({"budget": {"perDay": 0, "perChat": 0}})));
    c.ok("9 with the budget off runs are allowed again", a.t("POST", "/sessions/new/send", &tok, Some(json!({"text": "fine", "mode": "acceptEdits", "cwd": ps}))).code == 200); sleep(700);
    // backups
    let bk = a.t("POST", "/admin/safety", &tok, Some(json!({"backupNow": true})));
    c.ok_x("10 a backup of the chats can be made on request", bk.code == 200 && bk.j["ok"] == true && arr(&bk.j["backups"]["list"]).len() == 1, bk.j.to_string().chars().take(100).collect::<String>());
    let gone = big.join("big.jsonl"); fs::remove_file(&gone).unwrap();
    let rs = a.t("POST", "/admin/safety", &tok, Some(json!({"restoreBackup": bk.j["name"]})));
    c.ok("11 and restored", rs.code == 200 && gone.exists());
    c.ok("12 a strange backup name is refused", a.t("POST", "/admin/safety", &tok, Some(json!({"restoreBackup": "../../x"}))).code == 400);
    let dr = a.t("GET", "/admin/doctor", &tok, None);
    c.ok("13 the health check warns about a secret in a project, without showing it", arr(&dr.j["checks"]).iter().any(|c| c["name"] == "Secrets in projects" && c["ok"] == false && re(r"key\.env", &jstr(&c["text"])) && !re("AKIAABC", &c.to_string())));
    c.finish();
}
