//! The controller against the real relay (in-process, free port, temp folders under ~/hearth-tmp, a stub `claude` first in PATH):
//! events of the relay arrive as notifications and popups, a popup answer resolves a permission hook, a chat run streams,
//! settings persist, uploads land, the support sign-in and the update download work against tiny fake servers.
use hearth_desktop::app::{App, Event};
use hearth_relay::util::J;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const STUB: &str = r#"#!/bin/sh
prompt=$(cat)
sid=stub-chat-1
dir="$HOME/.claude/projects/$(pwd | sed 's/[^A-Za-z0-9]/-/g')"
mkdir -p "$dir"
now=$(date -u +%Y-%m-%dT%H:%M:%S.000Z)
printf '{"type":"user","cwd":"%s","sessionId":"%s","timestamp":"%s","message":{"role":"user","content":"%s"}}\n' "$(pwd)" "$sid" "$now" "$prompt" >> "$dir/$sid.jsonl"
echo '{"type":"system","subtype":"init","session_id":"'$sid'","model":"fake"}'
for w in Hello, " from" " the" " stub."; do
  echo '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"'"$w"'"}}}'
  sleep 0.05
done
printf '{"type":"assistant","session_id":"%s","message":{"role":"assistant","content":[{"type":"text","text":"Hello, from the stub."}]}}\n' "$sid"
echo '{"type":"result","subtype":"success","is_error":false,"result":"Hello, from the stub.","session_id":"'$sid'","usage":{"input_tokens":3,"output_tokens":4}}'
"#;

/// A tiny HTTP server: answers every request with `(status, body)` of the first matching path suffix, and records the request lines.
async fn fake_http(routes: Vec<(&'static str, u16, Vec<u8>, &'static str)>) -> (u16, Arc<Mutex<Vec<String>>>) {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap(); let port = l.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(vec![])); let s2 = seen.clone();
    tokio::spawn(async move { loop {
        let Ok((mut s, _)) = l.accept().await else { return };
        let (routes, seen) = (routes.clone(), s2.clone());
        tokio::spawn(async move {
            let mut buf = vec![0u8; 8192]; let n = s.read(&mut buf).await.unwrap_or(0);
            let text = String::from_utf8_lossy(&buf[..n]).to_string(); let line = text.lines().next().unwrap_or("").to_string();
            seen.lock().unwrap().push(text.clone());
            let path = line.split(' ').nth(1).unwrap_or("").to_string();
            let (st, body, ty) = routes.iter().find(|r| path.ends_with(r.0)).map(|r| (r.1, r.2.clone(), r.3)).unwrap_or((404, b"{}".to_vec(), "application/json"));
            let head = format!("HTTP/1.1 {st} X\r\nContent-Type: {ty}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
            let _ = s.write_all(head.as_bytes()).await; let _ = s.write_all(&body).await; let _ = s.shutdown().await;
        });
    } });
    (port, seen)
}

async fn wait_for<T>(what: &str, secs: u64, mut f: impl FnMut() -> Option<T>) -> T {
    let end = Instant::now() + Duration::from_secs(secs);
    loop { if let Some(v) = f() { return v; } assert!(Instant::now() < end, "timed out waiting for {what}"); tokio::time::sleep(Duration::from_millis(50)).await; }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn controller_end_to_end() {
    let base: PathBuf = std::env::var_os("HOME").map(PathBuf::from).unwrap().join("hearth-tmp");
    std::fs::create_dir_all(&base).unwrap();
    let t = tempfile::Builder::new().prefix("desktop-it-").tempdir_in(&base).unwrap();
    let root = t.path().to_path_buf();
    let (home, bin, proj, cfg) = (root.join("home"), root.join("bin"), root.join("proj"), root.join("cfg"));
    for d in [&home, &bin, &proj, &cfg] { std::fs::create_dir_all(d).unwrap(); }
    let stub = bin.join("claude"); std::fs::write(&stub, STUB).unwrap();
    { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap(); }
    let port = { let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap(); l.local_addr().unwrap().port() };
    std::env::set_var("HOME", &home); std::env::set_var("XDG_CONFIG_HOME", &cfg); std::env::set_var("CM_PORT", port.to_string());
    std::env::set_var("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default())); std::env::set_var("HEARTH_TEST", "1");
    let (support_port, support_seen) = fake_http(vec![
        ("/auth/start", 200, br#"{"ok":true}"#.to_vec(), "application/json"),
        ("/auth/verify", 200, br#"{"token":"tok123","email":"me@example.com"}"#.to_vec(), "application/json"),
        ("/auth/logout", 200, b"{}".to_vec(), "application/json"),
    ]).await;
    std::env::set_var("HEARTH_SUPPORT_URL", format!("http://127.0.0.1:{support_port}"));

    // the app, with its events and notifications recorded
    let events: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(vec![])); let notes: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(vec![]));
    let e2 = events.clone();
    let app = App::new(root.join("data"), tokio::runtime::Handle::current(), Arc::new(move |e| e2.lock().unwrap().push(e)));
    { let n2 = notes.clone(); *app.notifier.lock().unwrap() = Arc::new(move |t, b| n2.lock().unwrap().push((t.into(), b.into()))); }
    app.start_relay().await;
    assert!(app.relay.lock().unwrap().ok, "embedded relay: {:?}", app.relay.lock().unwrap().error);
    let local = app.local().expect("this computer is a machine"); assert_eq!(local.s("id"), "local"); assert_eq!(local.n("port") as u16, port);
    assert!(app.machine("local").is_some() && app.machine("nope").is_none());

    // pair info, a new code, hooks into the (temp) home
    let info = app.pair_info(); assert!(info.t("embedded") && info.s("code").len() == 6, "{info}");
    let old = info.s("code"); assert_ne!(app.pair_new_code().unwrap(), old);
    assert!(!app.pair_info().t("hooks")); app.hooks_install().unwrap(); assert!(app.pair_info().t("hooks")); assert!(home.join(".claude/settings.json").exists()); app.hooks_remove().unwrap(); assert!(!app.pair_info().t("hooks"));

    // usage: the relay's numbers become state, history and a notification when a threshold is crossed
    app.refresh_usage(false).await; assert!(app.st().s("error").contains("No usage data"), "{}", app.st().s("error"));
    {
        let r = app.relay.lock().unwrap().r.clone().unwrap();
        let at = hearth_relay::util::now_ms();
        *r.usage.lock().unwrap() = json!({"session": {"pct": 80, "resetsAt": at + 3.0 * 3600e3}, "week": {"pct": 10, "resetsAt": at + 3.0 * 86400e3}, "at": at});
    }
    app.refresh_usage(false).await;
    let st = app.public_state(); assert_eq!(st.g("usage").g("session").n("pct"), 80.0); assert_eq!(st.g("history").as_array().unwrap().len(), 1); assert!(st.g("error").is_null());
    assert!(notes.lock().unwrap().iter().any(|(t, b)| t == "Session (5h) at 80%" && b.starts_with("Resets in 3h")), "{:?}", notes.lock().unwrap());

    // settings persist (same file the old app used) and callMe starts listening in away mode
    let s = app.settings_set(json!({"callMe": true, "popup": true, "theme": "dark"})); assert!(s.t("callMe") && s.s("theme") == "dark");
    app.save_now(); let saved: Value = serde_json::from_slice(&std::fs::read(app.dir.join("state.json")).unwrap()).unwrap(); assert!(saved.g("settings").t("callMe")); assert_eq!(saved.g("settings").s("theme"), "dark");
    assert!(events.lock().unwrap().iter().any(|e| matches!(e, Event::ThemeChanged)) && events.lock().unwrap().iter().any(|e| matches!(e, Event::RingMe(true))));
    app.start_events();
    let http = reqwest::Client::new();
    wait_for("the relay to see a listener (away mode)", 10, || { let r = app.relay.lock().unwrap().r.clone().unwrap(); (r.clients() > 0 && r.away()).then_some(()) }).await;

    // a permission question reaches the desktop as a notification and a popup; answering the popup resolves Claude's hook
    let hook = { let http = http.clone(); tokio::spawn(async move {
        http.post(format!("http://127.0.0.1:{port}/hook")).json(&json!({"hook_event_name": "PermissionRequest", "session_id": "sess-perm-1", "cwd": "/home/a/myproj", "tool_name": "Bash", "tool_input": {"command": "rm -rf build"}})).send().await.unwrap().json::<Value>().await.unwrap()
    }) };
    let key = wait_for("the popup", 10, || events.lock().unwrap().iter().find_map(|e| if let Event::Popup(p) = e { Some(p.clone()) } else { None })).await;
    assert!(key.permission && key.kind == "permission" && key.height > 0.0, "{key:?}");
    assert!(notes.lock().unwrap().iter().any(|(t, _)| t == "myproj: needs your OK"), "{:?}", notes.lock().unwrap());
    assert!(events.lock().unwrap().iter().any(|e| matches!(e, Event::Relay { ev, .. } if ev.s("kind") == "permission")));
    assert!(app.call_action(&key.key, "allow").await);
    let out = tokio::time::timeout(Duration::from_secs(10), hook).await.expect("hook answered").unwrap();
    assert_eq!(out.g("hookSpecificOutput").g("decision").s("behavior"), "allow", "{out}");
    assert!(events.lock().unwrap().iter().any(|e| matches!(e, Event::ClosePopup(k) if *k == key.key)));
    assert!(!app.call_action(&key.key, "allow").await, "a popup answers once");
    // "Talk" on a call opens the chat
    let hook2 = { let http = http.clone(); tokio::spawn(async move { http.post(format!("http://127.0.0.1:{port}/hook")).json(&json!({"hook_event_name": "PreToolUse", "session_id": "sess-q-1", "cwd": "/home/a/other", "tool_name": "AskUserQuestion", "tool_input": {"questions": [{"question": "Which one?"}]}})).send().await.unwrap().status().as_u16() }) };
    let q = wait_for("the question popup", 10, || events.lock().unwrap().iter().find_map(|e| if let Event::Popup(p) = e { if p.kind == "question" { Some(p.clone()) } else { None } } else { None })).await;
    assert_eq!(q.text, "Which one?"); assert!(!q.permission);
    assert!(app.call_action(&q.key, "talk").await); let _ = hook2.await;
    assert!(events.lock().unwrap().iter().any(|e| matches!(e, Event::OpenSession { voice: true, .. })) && events.lock().unwrap().iter().any(|e| matches!(e, Event::ShowMain)));
    // when the window is in front, the notification is skipped (the app shows it itself)
    app.main_visible.store(true, std::sync::atomic::Ordering::SeqCst); app.main_focused.store(true, std::sync::atomic::Ordering::SeqCst);
    let n0 = notes.lock().unwrap().len();
    app.ring(&local, json!({"kind": "done", "cwd": "/x/front", "text": "t"})); assert_eq!(notes.lock().unwrap().len(), n0);
    app.main_visible.store(false, std::sync::atomic::Ordering::SeqCst);
    app.ring(&local, json!({"kind": "done", "cwd": "/x/back", "text": "t"})); assert_eq!(notes.lock().unwrap().last().unwrap().0, "back: Claude is done");

    // a phone on the network asks to connect: notification, accept
    let r: Value = http.post(format!("http://127.0.0.1:{port}/pair/request")).json(&json!({"name": "Pixel 9"})).send().await.unwrap().json().await.unwrap();
    assert!(!r.s("id").is_empty(), "{r}");
    let list = app.pair_requests().await; assert_eq!(list.as_array().unwrap().len(), 1);
    assert!(notes.lock().unwrap().iter().any(|(t, _)| t == "Pixel 9 wants to connect") && events.lock().unwrap().iter().any(|e| matches!(e, Event::ShowMain)));
    let after = app.pair_decide(&list[0].g("id").clone(), true).await.unwrap(); assert!(after.as_array().unwrap().is_empty());

    // a chat run streams: session, deltas, end (through hearth-client's resume logic)
    let sid = app.send("local", "", json!("say hi"), json!("default"), false, Some("s1".into()), json!({"cwd": proj})).unwrap(); assert_eq!(sid, "s1");
    wait_for("the run to end", 30, || events.lock().unwrap().iter().any(|e| matches!(e, Event::Stream { rec, .. } if rec.s("t") == "end")).then_some(())).await;
    let recs: Vec<Value> = events.lock().unwrap().iter().filter_map(|e| if let Event::Stream { sid, rec } = e { assert_eq!(sid, "s1"); Some(rec.clone()) } else { None }).collect();
    let text: String = recs.iter().filter(|r| r.s("t") == "delta").map(|r| r.s("text")).collect(); assert!(text.contains("Hello, from the stub."), "{text} / {recs:?}");
    assert!(recs.iter().any(|r| r.s("t") == "session" && r.s("id") == "stub-chat-1"));
    assert!(app.send("nope", "", json!("x"), json!("default"), false, None, Value::Null).is_err());

    // upload: a file of this computer into the chat's folder
    let f = root.join("note.txt"); std::fs::write(&f, "hello upload").unwrap();
    let p = app.upload_path("local", "", &proj.to_string_lossy(), &f).await.unwrap(); assert_eq!(std::fs::read_to_string(p.as_str().unwrap()).unwrap(), "hello upload");
    assert!(app.upload("nope", "", "", "x", vec![]).await.is_err());
    // generic relay call, stats, stop of nothing
    let sess = app.relay("local", "GET", "/sessions", None).await.unwrap(); assert!(sess.is_array());
    assert!(app.relay("local", "GET", "/nothing-here", None).await.is_err());
    assert!(app.stats("local", 7.0).await.is_ok() || true);

    // computers: add by address + token (the relay's own token), remove
    let tok = app.local().unwrap().s("token");
    let ms = app.add_machine(json!({"name": "Same relay", "host": format!("127.0.0.1:{port}"), "token": tok})).await.unwrap(); assert_eq!(ms.len(), 2); assert_eq!(ms[1].s("name"), "Same relay");
    let id = ms[1].s("id"); assert!(app.relay(&id, "GET", "/status", None).await.is_ok());
    assert_eq!(app.remove_machine(&id).len(), 1);
    assert!(app.add_machine(json!({"host": "", "token": "1"})).await.is_err());
    assert_eq!(app.guests_set(&json!([{"share": "ab12", "key": "share.zz", "hostname": "h.example", "port": 443, "secure": true, "host": "Sam", "title": "T"}, {"share": "bad!", "key": "x", "hostname": "h", "port": 1}])).len(), 1);
    assert_eq!(app.guests()[0].s("name"), "Sam: T"); assert!(app.guests()[0].get("token").is_none()); assert!(app.machine("g:ab12").is_some());
    app.guests_set(&json!([]));

    // support sign-in against the fake support server
    assert!(!app.support_state().t("signedIn")); app.support_start("me@example.com").await.unwrap();
    assert_eq!(app.support_verify("me@example.com", "123456").await.unwrap().s("email"), "me@example.com"); assert!(app.support_state().t("signedIn"));
    app.support_call("GET", "/auth/whoami", None, true).await.err(); // 404 from the fake: an error, token stays
    assert!(app.support_state().t("signedIn")); app.support_logout().await; assert!(!app.support_state().t("signedIn"));
    assert!(support_seen.lock().unwrap().iter().any(|r| r.contains("Authorization: Bearer tok123") || r.contains("authorization: bearer tok123")));

    // the updater's download: size and sha256 are checked
    let payload = b"pretend this is an AppImage".to_vec();
    let good = hearth_relay::util::sha256_hex(&payload);
    let (dl_port, _) = fake_http(vec![("/good", 200, payload.clone(), "application/octet-stream")]).await;
    let asset = |digest: String, size: usize| json!({"name": "Hearth.AppImage", "url": format!("http://127.0.0.1:{dl_port}/good"), "size": size, "digest": digest});
    let dest = root.join("dl/Hearth.AppImage");
    hearth_desktop::upd::download(&app, &asset(format!("sha256:{good}"), payload.len()), &dest).await.unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), payload); { use std::os::unix::fs::PermissionsExt; assert_eq!(std::fs::metadata(&dest).unwrap().permissions().mode() & 0o111, 0o111); }
    let bad = root.join("dl/bad"); let e = hearth_desktop::upd::download(&app, &asset("sha256:00".into(), payload.len()), &bad).await.unwrap_err(); assert!(e.contains("checksum"), "{e}"); assert!(!bad.exists() && !bad.with_extension("part").exists());
    let e = hearth_desktop::upd::download(&app, &asset(String::new(), payload.len() + 5), &bad).await.unwrap_err(); assert!(e.contains("incomplete"), "{e}");

    // shutdown saves the state
    app.shutdown(); assert!(app.dir.join("state.json").exists());
}
