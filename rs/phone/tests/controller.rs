//! The phone controller against the in-process relay (stubbed `claude`), like rs/client/tests.

use hearth_phone::controller::*;
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const STUB: &str = r#"#!/bin/sh
# A fake `claude -p --output-format stream-json`: answers with a few deltas, writes the chat file, logs its arguments.
prompt=$(cat)
echo "$*" >> "$STUB_LOG"
echo "PROMPT:$prompt" >> "$STUB_LOG"
sid=stub-chat-1
while [ $# -gt 0 ]; do [ "$1" = "--resume" ] && sid="$2"; shift; done
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
printf '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hello, from the stub."}]},"timestamp":"%s","sessionId":"%s","cwd":"%s"}\n' "$now" "$sid" "$(pwd)" >> "$dir/$sid.jsonl"
echo '{"type":"result","subtype":"success","is_error":false,"result":"Hello, from the stub.","session_id":"'$sid'","usage":{"input_tokens":3,"output_tokens":4}}'
"#;

#[derive(Default)]
struct Rec {
    notes: Mutex<Vec<(String, String)>>,
    pushed: Mutex<Vec<String>>,
    calls: Mutex<Vec<String>>,
    shared: Mutex<Vec<String>>,
}
impl Platform for Rec {
    fn notify(&self, _t: i32, title: &str, text: &str) { self.notes.lock().unwrap().push((title.into(), text.into())); }
    fn push_machines(&self, j: &str) { self.pushed.lock().unwrap().push(j.into()); }
    fn start_call(&self, t: &str, _x: &str) { self.calls.lock().unwrap().push(format!("start {t}")); }
    fn stop_call(&self) { self.calls.lock().unwrap().push("stop".into()); }
    fn share_text(&self, _t: &str, x: &str) { self.shared.lock().unwrap().push(x.into()); }
    fn pick_file(&self, cb: Box<dyn FnOnce(Option<Picked>) + Send>) { cb(Some(Picked { name: "a note.txt".into(), bytes: b"hello file".to_vec() })); }
    fn dictate(&self, _p: &str, cb: Box<dyn FnOnce(Option<String>) + Send>) { cb(Some("spoken words".into())); }
}

fn wait(what: &str, mut f: impl FnMut() -> bool) {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(30) {
        if f() { return; }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out waiting for {what}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn controller_flow() {
    let base: PathBuf = std::env::var_os("HOME").map(PathBuf::from).unwrap().join("hearth-tmp");
    std::fs::create_dir_all(&base).unwrap();
    let t = tempfile::Builder::new().prefix("phone-it-").tempdir_in(&base).unwrap();
    let root = t.path().to_path_buf();
    let (home, bin, proj, app) = (root.join("home"), root.join("bin"), root.join("proj"), root.join("app"));
    for d in [&home, &bin, &proj, &app] { std::fs::create_dir_all(d).unwrap(); }
    let log = root.join("stub.log");
    let stub = bin.join("claude");
    std::fs::write(&stub, STUB).unwrap();
    { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap(); }
    let seed_dir = home.join(".claude/projects").join(proj.to_string_lossy().replace(|c: char| !c.is_ascii_alphanumeric(), "-"));
    std::fs::create_dir_all(&seed_dir).unwrap();
    let ts = "2026-11-16T10:00:00.000Z";
    let lines = [
        json!({"type":"user","cwd":proj,"sessionId":"seed1","timestamp":ts,"message":{"role":"user","content":"first question"}}),
        json!({"type":"assistant","cwd":proj,"sessionId":"seed1","timestamp":ts,"message":{"role":"assistant","content":[{"type":"text","text":"first answer"}]}}),
    ];
    std::fs::write(seed_dir.join("seed1.jsonl"), lines.iter().map(|l| l.to_string() + "\n").collect::<String>()).unwrap();
    std::env::set_var("HOME", &home);
    std::env::set_var("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()));
    std::env::set_var("STUB_LOG", &log);
    std::env::set_var("HEARTH_TEST", "1");

    let port = { let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap(); l.local_addr().unwrap().port() };
    let relay = hearth_relay::boot::embedded(hearth_relay::relay::Opts {
        dir: root.join("relay").to_string_lossy().to_string(), port, host: "127.0.0.1".into(), name: Some("it-relay".into()),
        public_mode: false, projects_root: None, allow_bypass: false, allow_remote: false, self_update: false,
        relay_dir: String::new(), move_root: String::new(), seed_token: String::new(), voice: None,
    }).await.unwrap_or_else(|e| panic!("relay did not start: {e:?}"));

    let rec = Arc::new(Rec::default());
    let c = Controller::new(&app, rec.clone(), tokio::runtime::Handle::current()).unwrap();
    assert!(!c.state().paired);
    assert_eq!(c.state().screen, Screen::Onboarding);

    // wrong code is refused with a message, the right one pairs and loads chats
    c.pair(&format!("127.0.0.1:{port}"), "000000");
    wait("pair error", || !c.state().error.is_empty());
    assert!(!c.state().paired);
    c.pair("127.0.0.1", "12");
    wait("code hint", || c.state().error.contains("6-digit"));
    c.pair(&format!("127.0.0.1:{port}"), &relay.new_pair_code());
    wait("paired", || c.state().paired);
    wait("chats", || c.state().chats.iter().any(|x| x.id == "seed1"));
    assert_eq!(c.state().screen, Screen::Chats);
    assert!(rec.pushed.lock().unwrap().last().unwrap().contains("\"token\""), "watch got the servers");
    assert!(c.state().usage.is_some() || c.state().tiles.is_empty());

    // settings persist across a restart of the controller
    c.set_setting("notifications", json!(true));
    c.set_setting("mode", json!("auto"));

    // open the existing chat
    c.open_chat("seed1");
    wait("messages", || c.state().messages.len() == 2);
    assert_eq!(c.state().messages[1].text, "first answer");

    // streaming send with a queued second message: the second starts by itself after the first
    c.send("hello stub");
    c.send("queued one");
    assert!(c.state().working);
    assert_eq!(c.state().queued, vec!["queued one".to_string()]);
    wait("both runs", || { let s = c.state(); !s.working && s.queued.is_empty() && s.messages.iter().filter(|m| m.kind == "text" && m.text == "Hello, from the stub.").count() >= 2 });
    let s = c.state();
    assert!(s.messages.iter().all(|m| m.kind != "working"));
    assert!(std::fs::read_to_string(&log).unwrap().contains("PROMPT:queued one"));

    // a brand new chat gets its id from the stream
    c.open_chat("");
    c.set_cwd(&proj.to_string_lossy());
    c.send("new chat please");
    wait("new chat id", || c.state().current == "stub-chat-1" && !c.state().working);
    wait("new chat listed", || c.state().chats.iter().any(|x| x.id == "stub-chat-1"));

    // queue management
    c.open_chat("stub-chat-1");
    c.send("one");
    c.send("two");
    c.send("three");
    assert_eq!(c.state().queued.len(), 2);
    c.remove_queued(0);
    assert_eq!(c.state().queued, vec!["three".to_string()]);
    c.send_now(0);
    wait("idle after send now", || { let s = c.state(); !s.working && s.queued.is_empty() });
    assert!(std::fs::read_to_string(&log).unwrap().contains("PROMPT:three"));

    // dictation fills the draft, attach uploads, share and call hooks
    c.dictate();
    assert_eq!(c.state().draft, "spoken words");
    c.attach_file();
    wait("upload", || !c.state().attachments.is_empty() || !c.state().error.is_empty());
    let st = c.state();
    assert!(st.error.is_empty(), "upload failed: {}", st.error);
    assert!(st.attachments[0].path.contains("a_note"), "{:?}", st.attachments);
    c.send("with a file");
    wait("sent with file", || !c.state().working && c.state().attachments.is_empty());
    assert!(std::fs::read_to_string(&log).unwrap().contains("@"), "file path went into the prompt");
    c.share_last_answer();
    assert_eq!(rec.shared.lock().unwrap().last().unwrap(), "Hello, from the stub.");
    c.start_call();
    assert!(c.state().call.active);
    c.hang_up();
    assert_eq!(*rec.calls.lock().unwrap(), vec!["start Hearth call".to_string(), "stop".to_string()]);

    // relay events become notifications (not for the chat that is open)
    c.go(Screen::Chats);
    c.on_event(&json!({"kind": "done", "session": "elsewhere", "title": "Build", "text": "All green", "call": false}));
    assert_eq!(rec.notes.lock().unwrap().last().unwrap(), &("Build".to_string(), "All green".to_string()));
    let n = rec.notes.lock().unwrap().len();
    c.set_setting("notifications", json!(false));
    c.on_event(&json!({"kind": "done", "session": "x", "text": "quiet"}));
    assert_eq!(rec.notes.lock().unwrap().len(), n);

    // pins and names persist in a second controller on the same directory
    c.set_pinned("seed1", true);
    wait("pinned first", || c.state().chats.first().map(|x| x.id == "seed1").unwrap_or(false));
    let c2 = Controller::new(&app, Arc::new(NoPlatform), tokio::runtime::Handle::current()).unwrap();
    assert!(c2.state().paired);
    assert_eq!(c2.state().settings.get("notifications"), Some(&json!(false)));
    c2.remove_device(0);
    assert!(!c2.state().paired);
    assert_eq!(c2.state().screen, Screen::Onboarding);
    drop(relay);
}
