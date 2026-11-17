//! The watch core and the WatchApp callbacks headlessly against the in-process relay: servers pushed by the phone
//! (PhoneLink path), usage, chats, voice reply, stop.

use hearth_android::bridge::save_machines;
use hearth_android::wear::Request;
use hearth_client::api::RelayClient;
use hearth_client::model::Machine;
use hearth_ui::WatchApp;
use hearth_watch::binding::{apply, wire, Dictate, Runner};
use hearth_watch::core::{Core, Direct, Transport};
use serde_json::json;
use slint::platform::software_renderer::MinimalSoftwareWindow;
use slint::platform::{Platform as SlintPlatform, WindowAdapter};
use slint::Model;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

struct Headless;
impl SlintPlatform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(MinimalSoftwareWindow::new(Default::default()))
    }
}

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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn watch_flow() {
    let base: PathBuf = std::env::var_os("HOME").map(PathBuf::from).unwrap().join("hearth-tmp");
    std::fs::create_dir_all(&base).unwrap();
    let t = tempfile::Builder::new().prefix("watch-it-").tempdir_in(&base).unwrap();
    let root = t.path().to_path_buf();
    let (home, bin, proj, dir) = (root.join("home"), root.join("bin"), root.join("proj"), root.join("watch"));
    for d in [&home, &bin, &proj, &dir] { std::fs::create_dir_all(d).unwrap(); }
    let log = root.join("stub.log");
    let stub = bin.join("claude");
    std::fs::write(&stub, STUB).unwrap();
    { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap(); }
    let seed_dir = home.join(".claude/projects").join(proj.to_string_lossy().replace(|c: char| !c.is_ascii_alphanumeric(), "-"));
    std::fs::create_dir_all(&seed_dir).unwrap();
    let ts = "2026-11-16T10:00:00.000Z";
    let lines = [json!({"type":"user","cwd":proj,"sessionId":"seed1","timestamp":ts,"message":{"role":"user","content":"first question"}})];
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

    let dir_s = dir.display().to_string();
    let core = Arc::new(Core::new(&dir_s, Arc::new(Direct(dir_s.clone()))));
    core.refresh();
    assert!(core.state().status.contains("phone"), "{}", core.state().status);

    // the phone hands over its servers (what PhoneLink does with the data item)
    let probe = Machine { name: "".into(), host: "127.0.0.1".into(), port, token: "".into(), secure: false, guest: false, label: "".into(), share: "".into() };
    let paired = RelayClient::new(probe).unwrap().pair(&relay.new_pair_code()).await.unwrap();
    assert_eq!(save_machines(&dir_s, "not json"), 0);
    let pushed = json!([{"name": "Laptop", "base": paired.base(), "token": paired.token}, {"name": "bad", "base": "ftp://x", "token": "t"}]).to_string();
    assert_eq!(save_machines(&dir_s, &pushed), 1);
    assert_eq!(core.machine().unwrap().name, "Laptop");

    // callbacks, run inline (on a plain thread: the handlers block on their own runtime)
    let (core, dir_s2, log2, base2) = (core.clone(), dir_s.clone(), log.clone(), paired.base());
    std::thread::spawn(move || {
    slint::platform::set_platform(Box::new(Headless)).ok();
    let (dir_s, log, paired_base) = (dir_s2, log2, base2);
    let app = WatchApp::new().unwrap();
    let c2 = core.clone();
    let done: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
    let run: Runner = Arc::new(|f| f());
    let dictate: Dictate = Arc::new(|| Some("voice reply".to_string()));
    wire(&app, core.clone(), run, dictate, done);
    app.invoke_refresh();
    apply(&app, &c2.state());
    assert!(c2.state().has_server && !c2.state().via_phone);
    assert_eq!(app.get_chats().row_count(), 1);
    assert_eq!(app.get_chats().row_data(0).unwrap().title, "first question");
    assert!(app.get_week_line().is_empty() || app.get_week_line().starts_with("Week"));

    // a chat that is working is stopped by a tap; wait until it is idle, then a tap dictates and sends
    if c2.state().chats[0].working {
        app.invoke_open_chat(0);
        assert_eq!(c2.state().status, "Stopped");
    }
    let t1 = std::time::Instant::now();
    while c2.state().chats[0].working && t1.elapsed().as_secs() < 20 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        app.invoke_refresh();
    }
    assert!(!c2.state().chats[0].working, "chat stayed busy");
    app.invoke_open_chat(0);
    let t0 = std::time::Instant::now();
    while t0.elapsed().as_secs() < 10 && !std::fs::read_to_string(&log).map(|l| l.contains("PROMPT:voice reply")).unwrap_or(false) {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(std::fs::read_to_string(&log).map(|l| l.contains("PROMPT:voice reply")).unwrap_or(false), "reply reached the relay: status {:?}", c2.state().status);
    assert_eq!(c2.state().status, "Sent");

    // stop goes through the same handlers
    let (r, _) = Direct(dir_s.clone()).call(Request { op: "stop".into(), base: paired_base.clone(), id: "seed1".into(), ..Default::default() });
    assert!(r.error.as_deref() != Some("unknown"), "{r:?}");
    }).join().unwrap();
    drop(relay);
}
