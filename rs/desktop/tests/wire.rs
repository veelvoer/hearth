//! The real rs/ui window wired to the controller: root callbacks are fired headlessly (software renderer, no display) and the effect is
//! checked on the controller / the real relay (in-process, stub `claude`): open a chat, send, queue, stop, pin, rename, delete, a setting toggle,
//! theme, a permission prompt answer, pairing, onboarding, and the Oxide palette in `Palette`.
use hearth_desktop::app::{App, Event};
use hearth_desktop::color::Rgb;
use hearth_desktop::theme::ResolvedTheme;
use hearth_desktop::wire::Wire;
use hearth_relay::util::J;
use serde_json::{json, Value};
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter};
use slint::{ComponentHandle, Model};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct P(Rc<MinimalSoftwareWindow>);
impl Platform for P { fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> { Ok(self.0.clone()) } }

const STUB: &str = r#"#!/bin/sh
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
case "$prompt" in *slow*) n=40;; *) n=1;; esac
i=0
while [ $i -lt $n ]; do
for w in Hello, " from" " the" " stub."; do
  echo '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"'"$w"'"}}}'
  sleep 0.05
done
i=$((i+1))
done
printf '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hello, from the stub."}]},"timestamp":"%s","sessionId":"%s","cwd":"%s"}\n' "$now" "$sid" "$(pwd)" >> "$dir/$sid.jsonl"
echo '{"type":"result","subtype":"success","is_error":false,"result":"Hello, from the stub.","session_id":"'$sid'","usage":{"input_tokens":3,"output_tokens":4}}'
"#;

type Events = Arc<Mutex<Vec<Event>>>;
/// Feeds the controller's events to the window (what the UI thread does) until `cond` holds.
async fn pump(w: &Wire, ev: &Events, what: &str, secs: u64, mut cond: impl FnMut() -> bool) -> Vec<Event> {
    let end = Instant::now() + Duration::from_secs(secs); let mut seen = vec![];
    loop {
        let batch: Vec<Event> = std::mem::take(&mut *ev.lock().unwrap());
        for e in &batch { w.handle(e); }
        seen.extend(batch);
        if cond() { return seen; }
        assert!(Instant::now() < end, "timed out: {what}");
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
}
fn msgs(w: &Wire) -> Vec<(String, String)> { w.w.get_messages().iter().map(|m| (m.kind.to_string(), m.text.to_string())).collect() }

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn root_callbacks_drive_the_controller() {
    let base: PathBuf = std::env::var_os("HOME").map(PathBuf::from).unwrap().join("hearth-tmp");
    std::fs::create_dir_all(&base).unwrap();
    let t = tempfile::Builder::new().prefix("wire-it-").tempdir_in(&base).unwrap();
    let root = t.path().to_path_buf();
    let (home, bin, proj, cfg) = (root.join("home"), root.join("bin"), root.join("proj"), root.join("cfg"));
    for d in [&home, &bin, &proj, &cfg] { std::fs::create_dir_all(d).unwrap(); }
    let stub = bin.join("claude"); std::fs::write(&stub, STUB).unwrap();
    { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap(); }
    let log = root.join("stub.log");
    let seed_dir = home.join(".claude/projects").join(proj.to_string_lossy().replace(|c: char| !c.is_ascii_alphanumeric(), "-"));
    std::fs::create_dir_all(&seed_dir).unwrap();
    let ts = "2026-11-16T10:00:00.000Z";
    let lines = [json!({"type":"user","cwd":proj,"sessionId":"seed1","timestamp":ts,"message":{"role":"user","content":"first question"}}), json!({"type":"assistant","cwd":proj,"sessionId":"seed1","timestamp":ts,"message":{"role":"assistant","content":[{"type":"text","text":"first answer"}]}})];
    std::fs::write(seed_dir.join("seed1.jsonl"), lines.iter().map(|l| l.to_string() + "\n").collect::<String>()).unwrap();
    let port = { let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap(); l.local_addr().unwrap().port() };
    std::env::set_var("HOME", &home); std::env::set_var("XDG_CONFIG_HOME", &cfg); std::env::set_var("CM_PORT", port.to_string()); std::env::set_var("STUB_LOG", &log); std::env::set_var("HEARTH_TEST", "1");
    std::env::set_var("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()));

    let events: Events = Arc::new(Mutex::new(vec![])); let e2 = events.clone();
    let app = App::new(root.join("data"), tokio::runtime::Handle::current(), Arc::new(move |e| e2.lock().unwrap().push(e)));
    *app.notifier.lock().unwrap() = Arc::new(|_, _| {});
    app.start_relay().await; assert!(app.relay.lock().unwrap().ok);

    let win = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(P(win.clone()))).unwrap();
    let w = Wire::new(app.clone(), hearth_ui::App::new().unwrap());
    w.w.show().unwrap();
    // a first run: the onboarding shows, finishing it is remembered
    assert_eq!(w.w.get_screen(), "onboarding"); w.w.invoke_onboarding_finish(); assert_eq!(w.w.get_screen(), "chats"); assert!(app.setting("onboarded").as_bool().unwrap());

    // the chats list comes from the relay
    w.refresh();
    pump(&w, &events, "chat rows", 15, || w.w.get_chats().row_count() > 0).await;
    assert_eq!(w.w.get_chats().row_data(0).unwrap().title, "first question");
    // opening a chat shows its messages
    w.w.invoke_open_chat(0); assert_eq!(w.w.get_screen(), "chat");
    pump(&w, &events, "history", 15, || msgs(&w).iter().any(|m| m.1 == "first answer")).await;
    assert!(msgs(&w).iter().any(|m| m.0 == "user" && m.1 == "first question"));
    // sending resumes that chat on the relay (the stub logs `--resume seed1`) and streams the answer into the window
    w.w.invoke_send("second one".into());
    assert!(w.w.get_chat_working() && msgs(&w).iter().any(|m| m.0 == "user" && m.1 == "second one") && msgs(&w).last().unwrap().0 == "working");
    pump(&w, &events, "the run to end", 30, || !w.w.get_chat_working()).await;
    let l = std::fs::read_to_string(&log).unwrap(); assert!(l.contains("--resume seed1") && l.contains("PROMPT:second one"), "{l}");
    pump(&w, &events, "the answer in the list", 15, || msgs(&w).iter().filter(|m| m.1 == "Hello, from the stub.").count() >= 1).await;
    // a slow run: a follow-up is queued, shown as queued; Stop ends the run, and the queued one runs afterwards
    w.w.invoke_send("slow please".into()); pump(&w, &events, "slow run started", 10, || msgs(&w).iter().any(|m| m.1 == "Hello,")).await;
    w.w.invoke_send("and then this".into()); assert!(msgs(&w).iter().any(|m| m.0 == "queued" && m.1 == "and then this"));
    w.w.invoke_stop();
    pump(&w, &events, "stop + queued run", 40, || { let l = std::fs::read_to_string(&log).unwrap_or_default(); l.contains("PROMPT:and then this") && !w.w.get_chat_working() }).await;
    assert!(!msgs(&w).iter().any(|m| m.0 == "queued"));
    // remove a queued follow-up before it runs
    w.w.invoke_send("slow again".into()); pump(&w, &events, "second slow run", 10, || msgs(&w).iter().any(|m| m.0 == "working")).await;
    w.w.invoke_send("never sent".into()); let qi = msgs(&w).iter().position(|m| m.0 == "queued").unwrap();
    w.w.invoke_remove_queued(qi as i32); assert!(!msgs(&w).iter().any(|m| m.0 == "queued"));
    w.w.invoke_stop(); pump(&w, &events, "stop", 30, || !w.w.get_chat_working()).await;
    assert!(!std::fs::read_to_string(&log).unwrap().contains("never sent"));

    // pin: the relay stores the tag
    w.w.invoke_tab_changed(0); pump(&w, &events, "rows again", 10, || w.w.get_chats().row_count() > 0).await;
    let i = (0..w.w.get_chats().row_count()).find(|i| w.w.get_chats().row_data(*i).unwrap().title == "first question").unwrap_or(0);
    w.w.invoke_pin_chat(i as i32);
    pump(&w, &events, "pin on the relay", 10, || false || { let t = futures_block(&app); t }).await;
    // rename needs a name typed in the search box
    w.w.set_chat_search("Renamed chat".into()); w.w.invoke_rename_chat(i as i32);
    pump(&w, &events, "rename on the relay", 10, || { let t = tags(&app); t.g("seed1").s("name") == "Renamed chat" }).await;
    // the chat filter and search
    w.w.invoke_chat_search_edited("zzzz-no-match".into()); assert_eq!(w.w.get_chats().row_count(), 0); w.w.invoke_chat_search_edited("".into()); assert!(w.w.get_chats().row_count() > 0);
    // settings and theme
    w.w.invoke_setting_toggled("notify-usage".into(), false); assert_eq!(app.setting("notifyReset"), json!(false));
    w.w.invoke_setting_toggled("expressive".into(), true); assert_eq!(app.setting("style"), json!("material"));
    w.w.invoke_theme_picked("light".into()); assert_eq!(app.setting("theme"), json!("light"));
    pump(&w, &events, "theme event", 5, || true).await; assert!(events.lock().unwrap().is_empty());
    // new chat in a folder, send creates it in that folder
    w.handle(&Event::NewChat { cwd: proj.to_string_lossy().to_string() }); assert_eq!(w.w.get_screen(), "chat"); assert_eq!(w.w.get_chat_title(), "proj");
    w.w.invoke_composer_option("edits".into()); assert_eq!(w.chat().mode, "plan");
    w.w.invoke_send("brand new".into());
    pump(&w, &events, "new chat run", 30, || !w.w.get_chat_working() && w.chat().id == "stub-chat-1").await;
    assert!(std::fs::read_to_string(&log).unwrap().contains("--permission-mode plan"), "mode reaches the relay");
    // delete the chat on the relay
    pump(&w, &events, "rows with the new chat", 10, || w.w.get_chats().row_count() > 1).await;
    let k = (0..w.w.get_chats().row_count()).find(|i| w.rows().0[*i].id == "stub-chat-1").unwrap();
    w.w.invoke_delete_chat(k as i32);
    pump(&w, &events, "delete on the relay", 10, || { let s = futures_sessions(&app); !s.iter().any(|x| x.s("id") == "stub-chat-1") }).await;

    // a permission question while the window is in front: the sheet, and the answer resolves Claude's hook
    app.settings_set(json!({"callMe": true, "popup": true})); app.start_events();
    app.main_visible.store(true, std::sync::atomic::Ordering::SeqCst); app.main_focused.store(true, std::sync::atomic::Ordering::SeqCst);
    { let r = app.relay.lock().unwrap().r.clone().unwrap(); let t0 = Instant::now(); while !(r.clients() > 0 && r.away()) { assert!(t0.elapsed().as_secs() < 10, "listener"); tokio::time::sleep(Duration::from_millis(50)).await; } }
    let http = reqwest::Client::new();
    let hook = tokio::spawn(async move { http.post(format!("http://127.0.0.1:{port}/hook")).json(&json!({"hook_event_name": "PermissionRequest", "session_id": "sess-p", "cwd": "/a/myproj", "tool_name": "Bash", "tool_input": {"command": "rm -rf build"}})).send().await.unwrap().json::<Value>().await.unwrap() });
    pump(&w, &events, "the prompt sheet", 10, || w.w.get_prompt()).await;
    assert_eq!(w.w.get_prompt_tool(), "Bash"); assert!(w.w.get_prompt_target().contains("rm -rf build"));
    w.w.invoke_prompt_answered("deny".into()); w.w.set_prompt(false);
    let out = tokio::time::timeout(Duration::from_secs(10), hook).await.expect("hook answered").unwrap();
    assert_eq!(out.g("hookSpecificOutput").g("decision").s("behavior"), "deny", "{out}");

    // pairing a computer by address and token
    let tok = app.local().unwrap().s("token");
    w.w.invoke_pair_connect(format!("127.0.0.1:{port} {tok}").into());
    pump(&w, &events, "pairing", 15, || app.machines().len() == 2).await; assert_eq!(w.w.get_screen(), "settings");
    pump(&w, &events, "devices", 10, || w.w.get_devices().row_count() == 2).await;
    w.w.invoke_device_remove(1); assert_eq!(app.machines().len(), 1);

    // the Oxide palette lands in rs/ui's Palette; going back to the own one restores it
    let own_bg = w.w.global::<hearth_ui::Palette>().get_bg();
    let oxide = hearth_desktop::color::Palette::from_json(&json!({"bg": "#101820", "text": "#e0e8f0", "accent": "#33aaff", "ink": "#001122", "light": false, "surface": "#182430", "raised": "#203040", "line": "#304050", "bad": "#ff4455"})).unwrap();
    w.apply_theme(&ResolvedTheme { dark: true, surfaces: Some(oxide.tokens()), accent: Some((oxide.accent, Rgb(0, 0x11, 0x22))) });
    let p = w.w.global::<hearth_ui::Palette>();
    assert_eq!((p.get_bg().red(), p.get_bg().green(), p.get_bg().blue()), (0x10, 0x18, 0x20)); assert_eq!((p.get_accent().red(), p.get_accent().blue()), (0x33, 0xff)); assert_eq!(p.get_card().green(), 0x24); assert_eq!(p.get_error().red(), 0xff);
    w.apply_theme(&ResolvedTheme { dark: true, surfaces: None, accent: None }); assert_eq!(w.w.global::<hearth_ui::Palette>().get_bg(), own_bg);
    app.shutdown();
}

fn tags(app: &Arc<App>) -> Value { futures_block_on(app.relay("local", "GET", "/chat-tags", None)).unwrap_or_default().g("tags").clone() }
fn futures_block(app: &Arc<App>) -> bool { tags(app).g("seed1").t("pin") }
fn futures_sessions(app: &Arc<App>) -> Vec<Value> { futures_block_on(app.relay("local", "GET", "/sessions", None)).ok().and_then(|v| v.as_array().cloned()).unwrap_or_default() }
/// The relay answers over HTTP on other threads, so waiting for it from inside the test's own thread is fine.
fn futures_block_on<T>(f: impl std::future::Future<Output = T>) -> T { tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(f)) }
