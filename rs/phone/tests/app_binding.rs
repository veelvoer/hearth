//! The root callbacks of `hearth_ui::App` fired headlessly (no window system) against the in-process relay.

use hearth_phone::binding::{apply, wire, Applied};
use hearth_phone::controller::*;
use hearth_ui::{App, Palette};
use serde_json::json;
use slint::platform::software_renderer::MinimalSoftwareWindow;
use slint::platform::{Platform as SlintPlatform, WindowAdapter};
use slint::{ComponentHandle, Model};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

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

fn until(app: &App, ctl: &Controller, last: &mut Applied, what: &str, mut f: impl FnMut(&App) -> bool) {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(30) {
        apply(app, &ctl.state(), last);
        if f(app) { return; }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out waiting for {what}: {:?}", ctl.state().error);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn root_callbacks_drive_the_controller() {
    slint::platform::set_platform(Box::new(Headless)).ok();
    let base: PathBuf = std::env::var_os("HOME").map(PathBuf::from).unwrap().join("hearth-tmp");
    std::fs::create_dir_all(&base).unwrap();
    let t = tempfile::Builder::new().prefix("app-it-").tempdir_in(&base).unwrap();
    let root = t.path().to_path_buf();
    let (home, bin, proj, dir) = (root.join("home"), root.join("bin"), root.join("proj"), root.join("app"));
    for d in [&home, &bin, &proj, &dir] { std::fs::create_dir_all(d).unwrap(); }
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
    std::env::set_var("STUB_LOG", root.join("stub.log"));
    std::env::set_var("HEARTH_TEST", "1");
    let port = { let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap(); l.local_addr().unwrap().port() };
    let relay = hearth_relay::boot::embedded(hearth_relay::relay::Opts {
        dir: root.join("relay").to_string_lossy().to_string(), port, host: "127.0.0.1".into(), name: Some("it-relay".into()),
        public_mode: false, projects_root: None, allow_bypass: false, allow_remote: false, self_update: false,
        relay_dir: String::new(), move_root: String::new(), seed_token: String::new(), voice: None,
    }).await.unwrap_or_else(|e| panic!("relay did not start: {e:?}"));

    let ctl = Controller::new(&dir, Arc::new(NoPlatform), tokio::runtime::Handle::current()).unwrap();
    let app = App::new().unwrap();
    wire(&app, &ctl, true);
    let mut last = Applied::default();
    until(&app, &ctl, &mut last, "onboarding", |a| a.get_screen() == "onboarding");

    // pairing screen: "address code"
    app.invoke_pair_connect(format!("127.0.0.1:{port} {}", relay.new_pair_code()).into());
    until(&app, &ctl, &mut last, "chats", |a| a.get_screen() == "chats" && a.get_chats().row_count() >= 1);
    assert_eq!(app.get_devices().row_count(), 1);
    assert!(app.get_status().contains("online"), "{}", app.get_status());

    // open the chat, see its messages
    app.invoke_open_chat(0);
    until(&app, &ctl, &mut last, "chat messages", |a| a.get_screen() == "chat" && a.get_messages().row_count() == 2);
    assert_eq!(app.get_messages().row_data(1).unwrap().text, "first answer");

    // send, queue, watch the reply stream in
    app.invoke_send("hello stub".into());
    app.invoke_send("queued one".into());
    apply(&app, &ctl.state(), &mut last);
    assert!(app.get_chat_working());
    let kinds: Vec<String> = app.get_messages().iter().map(|m| m.kind.to_string()).collect();
    assert!(kinds.contains(&"queued".to_string()) && kinds.last().map(|k| k == "working").unwrap_or(false), "{kinds:?}");
    until(&app, &ctl, &mut last, "both replies", |a| !a.get_chat_working() && a.get_messages().iter().filter(|m| m.text == "Hello, from the stub.").count() >= 2);

    // settings and theme
    app.invoke_setting_toggled("notify-finished".into(), false);
    app.invoke_theme_picked("light".into());
    until(&app, &ctl, &mut last, "settings", |a| !a.get_settings().notify_finished);
    assert!(!app.global::<Palette>().get_dark());
    assert_eq!(app.get_settings().theme, "light");

    // pin
    app.invoke_pin_chat(0);
    until(&app, &ctl, &mut last, "pinned", |a| a.get_chats().row_data(0).map(|c| c.pinned).unwrap_or(false));

    // permission prompt from a relay event; answering closes it
    ctl.on_event(&json!({"kind": "permission", "session": "seed1", "req": "r1", "tool": "Edit", "text": "Edit Settings.tsx"}));
    until(&app, &ctl, &mut last, "prompt", |a| a.get_prompt());
    assert_eq!(app.get_prompt_tool(), "Edit");
    app.invoke_prompt_answered("deny".into());
    until(&app, &ctl, &mut last, "prompt closed", |a| !a.get_prompt());

    // call
    app.invoke_mic();
    until(&app, &ctl, &mut last, "call", |a| a.get_screen() == "call");
    app.invoke_call_hangup();
    until(&app, &ctl, &mut last, "back", |a| a.get_screen() == "chat");

    // tabs and devices
    app.invoke_tab_changed(3);
    until(&app, &ctl, &mut last, "settings screen", |a| a.get_screen() == "settings");
    app.invoke_device_remove(0);
    until(&app, &ctl, &mut last, "onboarding again", |a| a.get_screen() == "onboarding" && a.get_devices().row_count() == 0);
    drop(relay);
}
