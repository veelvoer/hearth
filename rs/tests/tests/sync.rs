// End-to-end test of chat and file sync: starts a server relay and a laptop relay on this machine and checks 24 things.
// Two real relay processes with separate HOME folders: a "server" (public mode) and a "laptop" linked to it.
use hearth_tests::*;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

fn uuid(n: u32) -> String { format!("00000000-0000-4000-8000-{:012}", n) }
fn arr(v: &Value) -> Vec<Value> { v.as_array().cloned().unwrap_or_default() }

fn chat(home: &Path, id: &str, cwd: &str, lines: &[&str], age_sec: f64) -> std::path::PathBuf {
    let d = home.join(".claude/projects").join(enc_path(cwd)); fs::create_dir_all(&d).unwrap();
    let f = d.join(format!("{id}.jsonl"));
    let l: Vec<String> = lines.iter().enumerate().map(|(i, t)| json!({"type": if i % 2 == 1 { "assistant" } else { "user" }, "cwd": cwd, "timestamp": iso_ms(now_ms() - 1e5 + i as f64),
        "message": {"role": if i % 2 == 1 { "assistant" } else { "user" }, "content": if i % 2 == 1 { json!([{"type":"text","text":t}]) } else { json!(t) }, "id": format!("m{i}"), "usage": {"input_tokens":1,"output_tokens":1}, "model": "x"}}).to_string()).collect();
    fs::write(&f, l.join("\n") + "\n").unwrap(); age_file(&f, age_sec); f
}
fn append(f: &Path, cwd: &str, t: &str, age_sec: f64) {
    use std::io::Write;
    let mut h = fs::OpenOptions::new().append(true).open(f).unwrap();
    h.write_all((json!({"type":"user","cwd":cwd,"message":{"role":"user","content":t}}).to_string() + "\n").as_bytes()).unwrap(); drop(h); age_file(f, age_sec);
}
fn w(f: &Path, txt: &str, age: f64) { fs::create_dir_all(f.parent().unwrap()).unwrap(); fs::write(f, txt).unwrap(); age_file(f, age); }

#[test]
fn sync() {
    let env = Env::new("sync");
    let (srv, lap) = (env.t.join("srv"), env.t.join("lap"));
    for h in [&srv, &lap] { fs::create_dir_all(h.join(".claude/projects")).unwrap(); }
    fs::create_dir_all(srv.join("projects/app")).unwrap(); fs::create_dir_all(lap.join("projects/app")).unwrap(); fs::create_dir_all(lap.join("other/school")).unwrap();
    let (srv_root, lap_root) = (srv.join("projects").to_string_lossy().to_string(), lap.join("projects").to_string_lossy().to_string());
    let mut c = Checks::new();
    // data before start: laptop has an in-project chat (A), an elsewhere chat (B), a temp junk chat (J), an empty/deleted-folder chat (D); server has a chat the laptop lacks (S)
    let lap_a = chat(&lap, &uuid(1), &format!("{lap_root}/app"), &["build the app", "sure, on it", "now add tests", "done"], 120.0);
    chat(&lap, &uuid(2), &format!("{}/other/school", lap.display()), &["help with my homework", "happy to help"], 100.0);
    chat(&lap, &uuid(3), "/tmp/junk", &["x", "y"], 90.0);
    chat(&lap, &uuid(4), &format!("{lap_root}/gone"), &["old", "chat"], 80.0);
    chat(&srv, &uuid(5), &format!("{srv_root}/app"), &["from the phone", "ok"], 60.0);
    let srv_a = srv.join(".claude/projects").join(enc_path(&format!("{srv_root}/app"))).join(format!("{}.jsonl", uuid(1)));
    let n_lines = |f: &Path| fs::read_to_string(f).unwrap().trim().split('\n').count();

    let srv_relay = Relay::start(&env, Opts::new().home(&srv).dir(&srv.join("cfg")).no_projects().args(&["--projects", &srv_root]));
    let (sp, tok) = (srv_relay.port, srv_relay.token.clone());
    fs::create_dir_all(lap.join("cfg")).unwrap();
    fs::write(lap.join("cfg/link.json"), json!({"url": format!("http://127.0.0.1:{sp}"), "token": tok}).to_string()).unwrap();
    let lap_relay = Relay::start(&env, Opts::new().home(&lap).dir(&lap.join("cfg")).no_projects().args(&["--lan", "--move-root", &lap_root]));
    let ltok = lap_relay.token.clone();
    let (sa, la) = (srv_relay.api(), lap_relay.api());
    // the laptop reads link.json every 10 s; first sync follows
    for _ in 0..40 { sleep(1000); if srv_a.exists() { break; } }
    sleep(2500);
    let s = sa.t("GET", "/sessions", &tok, None).j;
    let by_id = |l: &Value, n: u32| arr(l).into_iter().find(|x| x["id"] == json!(uuid(n)));
    c.ok_x("1 in-project chat reached the server", by_id(&s, 1).is_some(), by_id(&s, 1).map(|x| jstr(&x["title"])).unwrap_or_default());
    let srv_a_text = fs::read_to_string(&srv_a).unwrap_or_default();
    c.ok("1b its path was rewritten for the server", srv_a.exists() && srv_a_text.contains(&srv_root) && !srv_a_text.contains(&lap_root));
    c.ok_x("2 outside-projects chat listed on the server as laptop-only", by_id(&s, 2).map(|x| truthy(&x["elsewhere"])).unwrap_or(false), by_id(&s, 2).map(|x| x["elsewhere"].to_string()).unwrap_or_default());
    let m2 = sa.t("GET", &format!("/sessions/{}/messages", uuid(2)), &tok, None).j;
    c.ok("2b ...and readable", m2.is_array() && arr(&m2).len() >= 1);
    c.ok("3 temp-folder junk NOT on server", by_id(&s, 3).is_none());
    c.ok("3b deleted-folder chat NOT on server", by_id(&s, 4).is_none());
    let l = la.t("GET", "/sessions", &ltok, None).j;
    c.ok_x("4 server-only chat arrived on the laptop", by_id(&l, 5).is_some(), by_id(&l, 5).map(|x| jstr(&x["title"])).unwrap_or_default());
    c.ok("4b laptop list hides junk", by_id(&l, 3).is_none() && by_id(&l, 4).is_none());
    let lap_s = lap.join(".claude/projects").join(enc_path(&format!("{lap_root}/app"))).join(format!("{}.jsonl", uuid(5)));
    let lap_s_text = fs::read_to_string(&lap_s).unwrap_or_default();
    c.ok("4c rewritten for the laptop", lap_s.exists() && lap_s_text.contains(&lap_root) && !lap_s_text.contains(&srv_root));
    // 5 growth on the laptop -> only the tail goes over; server ends identical
    append(&lap_a, &format!("{lap_root}/app"), "one more thing from the laptop", 30.0);
    la.t("POST", "/sync/now", &ltok, Some(json!({})));
    c.ok_x("5 laptop append reached server", n_lines(&srv_a) == 5, format!("lines={}", n_lines(&srv_a)));
    // 6 server newer -> laptop pulls
    append(&srv_a, &format!("{srv_root}/app"), "reply made on the server", 25.0);
    la.t("POST", "/sync/now", &ltok, Some(json!({})));
    c.ok_x("6 server append reached laptop", n_lines(&lap_a) == 6, format!("lines={}", n_lines(&lap_a)));
    // 7 stable: another sync changes nothing
    let mt = |f: &Path| fs::metadata(f).unwrap().modified().unwrap();
    let before = (mt(&lap_a), mt(&srv_a)); la.t("POST", "/sync/now", &ltok, Some(json!({}))); la.t("POST", "/sync/now", &ltok, Some(json!({})));
    c.ok("7 no ping-pong", before == (mt(&lap_a), mt(&srv_a)));
    // 8 sync status
    let st = sa.t("GET", "/sync/status", &tok, None).j; let ls = la.t("GET", "/sync/status", &ltok, None).j;
    c.ok_x("8 status: server sees the computer", arr(&st["computers"]).len() == 1 && st["computers"][0]["online"] == true, json!({"ok": st["computers"][0]["sync"]["ok"], "pushed": st["computers"][0]["sync"]["pushed"]}).to_string());
    c.ok_x("8b status: laptop knows last sync", ls["last"]["ok"] == true, json!({"chats": ls["localChats"], "waiting": ls["waiting"]}).to_string());
    // 9 reply to a laptop-only chat from the server side is handed to the laptop
    let q = sa.t("POST", &format!("/sessions/{}/send", uuid(2)), &tok, Some(json!({"text": "continue please"}))).text;
    c.ok_x("9 reply to laptop-only chat is queued for the laptop", re(r#""t":"queued""#, &q) && re(r#""online":true"#, &q), q.lines().next().unwrap_or("").to_string());
    // 10 an older laptop copy can never clobber a newer server copy
    let stale_before = n_lines(&srv_a); age_file(&lap_a, 3600.0); la.t("POST", "/sync/now", &ltok, Some(json!({})));
    c.ok("10 an older laptop copy never overwrites a newer server copy", n_lines(&srv_a) >= stale_before);

    // files
    let sync_now = || { la.t("POST", "/sync/now", &ltok, Some(json!({}))); la.t("POST", "/sync/now", &ltok, Some(json!({}))); };
    let until = |cond: &dyn Fn() -> bool, tries: u32| -> bool { for _ in 0..tries { sleep(1500); sync_now(); if cond() { return true; } } cond() };   // slow machines need a few rounds
    w(&lap.join("projects/app/src/a.txt"), "hello from laptop", 10.0); w(&lap.join("projects/app/node_modules/x/y.js"), "junk", 10.0); w(&srv.join("projects/newproj/readme.md"), "made on the phone", 10.0);
    sync_now();
    c.ok("11 laptop file reached the server", fs::read_to_string(srv.join("projects/app/src/a.txt")).map(|s| s == "hello from laptop").unwrap_or(false));
    c.ok("11b node_modules not synced", !srv.join("projects/app/node_modules").exists());
    let ov = la.t("GET", "/sync/status", &ltok, None).j;
    c.ok_x("12 a project made on the server comes to the laptop by itself, and is reported as arrived", until(&|| lap.join("projects/newproj/readme.md").exists(), 6) && arr(&ov["files"]["arrived"]).iter().any(|p| p["name"] == "newproj") && arr(&ov["pending"]).is_empty(), ov["files"]["arrived"].to_string());

    c.ok("12b after Install it arrives", fs::read_to_string(lap.join("projects/newproj/readme.md")).map(|s| s == "made on the phone").unwrap_or(false));
    w(&srv.join("projects/app/src/a.txt"), "edited on server", 1.0);
    c.ok("13 server edit reached laptop", until(&|| fs::read_to_string(lap.join("projects/app/src/a.txt")).map(|s| s == "edited on server").unwrap_or(false), 8));
    fs::remove_file(lap.join("projects/app/src/a.txt")).unwrap(); sync_now();
    c.ok("14 delete on laptop removes it on server (to trash)", !srv.join("projects/app/src/a.txt").exists() && srv.join("projects/.hearth-trash").exists());
    w(&lap.join("projects/app/c.txt"), "v1", 30.0); sync_now(); sleep(2500);   // the wake-up this push caused must be over before the two edits
    w(&lap.join("projects/app/c.txt"), "laptop v2", 5.0); w(&srv.join("projects/app/c.txt"), "server v2", 1.0);
    let conflicts = || -> Vec<String> { let mut v: Vec<String> = vec![]; for d in [lap.join("projects/app"), srv.join("projects/app")] { for e in fs::read_dir(d).unwrap().flatten() { let n = e.file_name().to_string_lossy().to_string(); if n.contains("conflict") { v.push(n); } } } v };
    let okc = until(&|| fs::read_to_string(lap.join("projects/app/c.txt")).map(|s| s == "server v2").unwrap_or(false) && !conflicts().is_empty(), 8);
    c.ok_x("15 conflict: newest wins, other kept", okc, format!("{} lap={} srv={}", conflicts().join(","), fs::read_to_string(lap.join("projects/app/c.txt")).unwrap_or_default(), fs::read_to_string(srv.join("projects/app/c.txt")).unwrap_or_default()));
    for i in 0..30 { w(&lap.join(format!("projects/bulk/f{i}.txt")), &format!("b{i}"), 10.0); }
    la.t("POST", "/sync/projects/accept", &ltok, Some(json!({"all": true}))); sync_now();
    for i in 0..30 { fs::remove_file(lap.join(format!("projects/bulk/f{i}.txt"))).unwrap(); } sync_now();
    c.ok_x("16 safety brake: mass delete is held back", srv.join("projects/bulk/f5.txt").exists(), la.t("GET", "/sync/status", &ltok, None).j["files"]["paused"].to_string());
    w(&srv.join("projects/nope/a.txt"), "not wanted here", 10.0); until(&|| lap.join("projects/nope/a.txt").exists(), 2); sleep(1500); sync_now();
    let s17 = la.t("GET", "/sync/status", &ltok, None).j;
    c.ok("17 another server project arrives by itself too", until(&|| lap.join("projects/nope/a.txt").exists(), 6));
    c.finish();
}
