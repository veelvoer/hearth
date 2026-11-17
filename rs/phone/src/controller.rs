//! The phone's view-model: plain Rust state plus async tasks on `hearth-client`. No UI types.
//! The UI binds to it by calling the action methods and rendering every `State` it is handed through
//! [`Controller::subscribe`]; effects that need Android (notifications, share, camera, ...) go through the
//! [`Platform`] trait so this runs, and is tested, on a desktop host against the in-process relay.

use futures_util::StreamExt;
use hearth_client::api::RelayClient;
use hearth_client::model::{Machine, Usage};
use hearth_client::pair::parse_machine;
use hearth_client::store::Store;
use hearth_client::RunEvent;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::runtime::Handle;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Screen {
    #[default]
    Chats,
    Chat,
    Home,
    History,
    Settings,
    Onboarding,
    Pairing,
    Call,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChatVm {
    pub id: String,
    pub title: String,
    pub sub: String,
    pub working: bool,
    pub pinned: bool,
    pub mtime: i64,
}

/// kind: user | text | tool | queued | working | error
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MsgVm {
    pub kind: String,
    pub text: String,
    pub name: String,
    pub detail: String,
    pub call_id: String,
    pub failed: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tile {
    pub label: String,
    pub value: String,
    pub sub: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Attachment {
    pub name: String,
    pub path: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CallVm {
    pub active: bool,
    pub muted: bool,
    pub since_ms: i64,
}

/// A permission request Claude is waiting on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PromptVm {
    pub req: String,
    pub tool: String,
    pub text: String,
}

#[derive(Clone, Debug, Default)]
pub struct State {
    pub prompt: Option<PromptVm>,
    pub screen: Screen,
    pub paired: bool,
    pub server: String,
    pub devices: Vec<(String, String)>,
    pub status: String,
    pub error: String,
    pub busy: bool,
    pub chats: Vec<ChatVm>,
    pub history: Vec<ChatVm>,
    pub current: String,
    pub current_title: String,
    pub messages: Vec<MsgVm>,
    pub working: bool,
    pub queued: Vec<String>,
    pub draft: String,
    pub attachments: Vec<Attachment>,
    pub uploading: bool,
    pub usage: Option<Usage>,
    pub tiles: Vec<Tile>,
    pub settings: BTreeMap<String, Value>,
    pub call: CallVm,
    /// project folder for a new chat
    pub cwd: String,
    pub projects: Vec<(String, String)>,
}

/// A picked or photographed file, already read.
pub struct Picked {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// Everything the controller needs from the operating system.
pub trait Platform: Send + Sync {
    fn notify(&self, _tag: i32, _title: &str, _text: &str) {}
    fn start_call(&self, _title: &str, _text: &str) {}
    fn stop_call(&self) {}
    fn share_text(&self, _title: &str, _text: &str) {}
    fn open_url(&self, _url: &str) {}
    fn pick_file(&self, cb: Box<dyn FnOnce(Option<Picked>) + Send>) {
        cb(None)
    }
    fn take_photo(&self, cb: Box<dyn FnOnce(Option<Picked>) + Send>) {
        cb(None)
    }
    fn dictate(&self, _prompt: &str, cb: Box<dyn FnOnce(Option<String>) + Send>) {
        cb(None)
    }
    fn copy(&self, _text: &str) {}
    /// Hand the server list to the watch (JSON `[{name, base, token}]`).
    fn push_machines(&self, _json: &str) {}
}

pub struct NoPlatform;
impl Platform for NoPlatform {}

type Sub = Box<dyn Fn(&State) + Send + Sync>;

#[derive(Default)]
struct RunCtl {
    /// text to start as soon as the current run has ended (send now)
    next: Option<String>,
    run: Option<tokio::task::JoinHandle<()>>,
    feed: Option<tokio::task::JoinHandle<()>>,
}

struct Inner {
    state: Mutex<State>,
    subs: Mutex<Vec<Sub>>,
    store: Mutex<Store>,
    platform: Arc<dyn Platform>,
    rt: Handle,
    ctl: Mutex<RunCtl>,
}

#[derive(Clone)]
pub struct Controller(Arc<Inner>);

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn tag_of(s: &str) -> i32 {
    s.bytes().fold(7i32, |h, b| h.wrapping_mul(31).wrapping_add(b as i32)).abs() % 1_000_000 + 100
}

fn chat_vm(s: &hearth_client::model::Sess, store: &Store, now: i64) -> ChatVm {
    let name = store.chat_name(&s.id);
    let title = if !name.is_empty() { name } else if s.title.trim().is_empty() { "Chat".into() } else { s.title.clone() };
    ChatVm { id: s.id.clone(), title, sub: { let a = hearth_client::fmt::age(now - s.mtime); if a == "now" { a } else { format!("{a} ago") } }, working: s.running || s.busy, pinned: store.pinned(&s.id), mtime: s.mtime }
}

impl Controller {
    /// `dir`: where `hearth.json` lives. `rt`: the runtime the tasks run on.
    pub fn new(dir: &Path, platform: Arc<dyn Platform>, rt: Handle) -> Result<Controller, String> {
        let path: PathBuf = dir.join("hearth.json");
        let store = Store::open(&path).map_err(|e| e.to_string())?;
        let mut st = State::default();
        for key in ["mode", "motion", "notifications", "call"] {
            if let Some(v) = store.get_setting(key) {
                st.settings.insert(key.into(), v);
            }
        }
        let machines = store.machines();
        st.paired = machines.iter().any(|m| !m.guest);
        st.server = machines.iter().find(|m| !m.guest).map(|m| m.name.clone()).unwrap_or_default();
        st.devices = machines.iter().map(|m| (m.name.clone(), m.base())).collect();
        st.screen = if st.paired { Screen::Chats } else { Screen::Onboarding };
        Ok(Controller(Arc::new(Inner { state: Mutex::new(st), subs: Mutex::new(vec![]), store: Mutex::new(store), platform, rt, ctl: Mutex::new(RunCtl::default()) })))
    }

    pub fn state(&self) -> State {
        self.0.state.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Be handed every new state (called on whatever thread changed it).
    pub fn subscribe(&self, f: impl Fn(&State) + Send + Sync + 'static) {
        if let Ok(mut s) = self.0.subs.lock() {
            s.push(Box::new(f));
        }
    }

    fn update(&self, f: impl FnOnce(&mut State)) {
        let snap = {
            let Ok(mut s) = self.0.state.lock() else { return };
            f(&mut s);
            s.clone()
        };
        if let Ok(subs) = self.0.subs.lock() {
            for f in subs.iter() {
                f(&snap);
            }
        }
    }

    fn machine(&self) -> Option<Machine> {
        self.0.store.lock().ok().and_then(|s| s.machines().into_iter().find(|m| !m.guest))
    }

    fn client(&self) -> Option<RelayClient> {
        RelayClient::new(self.machine()?).ok()
    }

    fn spawn<F: std::future::Future<Output = ()> + Send + 'static>(&self, f: F) -> tokio::task::JoinHandle<()> {
        self.0.rt.spawn(f)
    }

    pub fn go(&self, screen: Screen) {
        self.update(|s| s.screen = screen);
    }

    // ---------- pairing and devices ----------

    /// A `hearth://` link (carries the token) or an address plus the 6-digit code.
    pub fn pair(&self, input: &str, code: &str) {
        self.update(|s| {
            s.busy = true;
            s.error.clear();
            s.status = "Pairing...".into();
        });
        let (me, input, code) = (self.clone(), input.to_string(), code.trim().to_string());
        self.spawn(async move {
            let r: Result<Machine, String> = async {
                let m = parse_machine(&input, "", "").ok_or("That is not an address or link")?;
                let m = if input.trim().starts_with("hearth://") { parse_machine(&input, "", "").ok_or("Bad link")? } else { m };
                if !m.token.is_empty() {
                    return Ok(m);
                }
                if code.len() != 6 {
                    return Err("Enter the 6-digit code shown on the computer".to_string());
                }
                RelayClient::new(m).map_err(|e| e.to_string())?.pair(&code).await.map_err(|e| e.to_string())
            }
            .await;
            match r {
                Ok(m) => match me.add_machine(m) {
                    Ok(()) => {
                        me.update(|s| {
                            s.busy = false;
                            s.status.clear();
                            s.screen = Screen::Chats;
                        });
                        me.refresh();
                        me.start_feed();
                    }
                    Err(e) => me.update(|s| {
                        s.busy = false;
                        s.error = e;
                    }),
                },
                Err(e) => me.update(|s| {
                    s.busy = false;
                    s.error = e;
                }),
            }
        });
    }

    fn add_machine(&self, m: Machine) -> Result<(), String> {
        let all = {
            let mut st = self.0.store.lock().map_err(|e| e.to_string())?;
            let mut all: Vec<Machine> = st.machines().into_iter().filter(|x| !(x.host == m.host && x.port == m.port)).collect();
            all.insert(0, m);
            st.set_machines(all.clone()).map_err(|e| e.to_string())?;
            all
        };
        self.after_machines(all);
        Ok(())
    }

    /// Hand the current servers to the watch again (at app start).
    pub fn sync_watch(&self) {
        if let Ok(st) = self.0.store.lock() {
            let all = st.machines();
            drop(st);
            self.after_machines(all);
        }
    }

    fn after_machines(&self, all: Vec<Machine>) {
        let list: Vec<Value> = all.iter().filter(|m| !m.guest).map(|m| json!({"name": m.name, "base": m.base(), "token": m.token})).collect();
        self.0.platform.push_machines(&Value::Array(list).to_string());
        self.update(|s| {
            s.paired = all.iter().any(|m| !m.guest);
            s.server = all.iter().find(|m| !m.guest).map(|m| m.name.clone()).unwrap_or_default();
            s.devices = all.iter().map(|m| (m.name.clone(), m.base())).collect();
        });
    }

    /// Remove a paired computer (index into `State::devices`).
    pub fn remove_device(&self, ix: usize) {
        let all = {
            let Ok(mut st) = self.0.store.lock() else { return };
            let mut all = st.machines();
            if ix < all.len() {
                all.remove(ix);
            }
            let _ = st.set_machines(all.clone());
            all
        };
        let empty = all.is_empty();
        self.after_machines(all);
        if empty {
            self.update(|s| {
                s.screen = Screen::Onboarding;
                s.chats.clear();
                s.messages.clear();
            });
        }
    }

    // ---------- lists, usage, history ----------

    /// Reload chats and usage.
    pub fn refresh(&self) {
        let Some(c) = self.client() else { return };
        self.update(|s| s.busy = true);
        let me = self.clone();
        self.spawn(async move {
            let (chats, usage) = tokio::join!(c.sessions(), c.usage());
            let now = now_ms();
            me.update(|s| {
                s.busy = false;
                match &chats {
                    Ok(list) => {
                        let st = me.0.store.lock();
                        if let Ok(st) = st {
                            let mut v: Vec<ChatVm> = list.iter().filter(|x| x.kind != "talk" || !x.title.trim().is_empty()).map(|x| chat_vm(x, &st, now)).collect();
                            v.sort_by(|a, b| b.pinned.cmp(&a.pinned).then(b.mtime.cmp(&a.mtime)));
                            s.history = v.iter().take(500).cloned().collect();
                            s.chats = v.into_iter().take(100).collect();
                        }
                        s.status = format!("{} chats", s.chats.len());
                        s.error.clear();
                    }
                    Err(e) => s.error = e.to_string(),
                }
                if let Ok(Some(u)) = &usage {
                    s.tiles = tiles(u, now);
                    s.usage = Some(u.clone());
                }
            });
        });
    }

    /// Search chats by text (fills `history`); an empty query shows all.
    pub fn search(&self, q: &str) {
        let Some(c) = self.client() else { return };
        let (me, q) = (self.clone(), q.trim().to_string());
        if q.is_empty() {
            self.refresh();
            return;
        }
        self.spawn(async move {
            if let Ok(hits) = c.search(&q).await {
                let now = now_ms();
                me.update(|s| {
                    if let Ok(st) = me.0.store.lock() {
                        s.history = hits.iter().map(|(x, _)| chat_vm(x, &st, now)).collect();
                    }
                });
            }
        });
    }

    pub fn set_pinned(&self, id: &str, pinned: bool) {
        if let Ok(mut st) = self.0.store.lock() {
            let _ = st.set_pinned(id, pinned);
        }
        self.refresh();
    }

    pub fn rename_chat(&self, id: &str, name: &str) {
        if let Ok(mut st) = self.0.store.lock() {
            let _ = st.set_chat_name(id, name);
        }
        self.refresh();
    }

    // ---------- settings ----------

    pub fn set_setting(&self, key: &str, value: Value) {
        if let Ok(mut st) = self.0.store.lock() {
            let _ = st.set_setting(key, value.clone());
        }
        self.update(|s| {
            s.settings.insert(key.into(), value);
        });
    }

    fn setting_bool(&self, key: &str, default: bool) -> bool {
        self.0.state.lock().ok().and_then(|s| s.settings.get(key).and_then(|v| v.as_bool())).unwrap_or(default)
    }

    // ---------- chat ----------

    /// Open a chat (empty id = a new one).
    pub fn open_chat(&self, id: &str) {
        let title = self.0.state.lock().ok().and_then(|s| s.chats.iter().chain(s.history.iter()).find(|c| c.id == id).map(|c| c.title.clone())).unwrap_or_else(|| "New chat".into());
        self.update(|s| {
            s.screen = Screen::Chat;
            s.current = id.to_string();
            s.current_title = title;
            s.messages.clear();
            s.queued.clear();
            s.attachments.clear();
            s.error.clear();
        });
        if id.is_empty() {
            return;
        }
        let Some(c) = self.client() else { return };
        let (me, id) = (self.clone(), id.to_string());
        self.spawn(async move {
            match c.messages(&id, Some(80)).await {
                Ok(m) => {
                    let v: Vec<MsgVm> = m
                        .into_iter()
                        .map(|m| {
                            if !m.name.is_empty() {
                                MsgVm { kind: "tool".into(), text: m.text, name: m.name, detail: m.result.unwrap_or(m.detail), failed: m.err, ..Default::default() }
                            } else {
                                MsgVm { kind: if m.role == "user" { "user" } else { "text" }.into(), text: m.text, ..Default::default() }
                            }
                        })
                        .collect();
                    me.update(|s| {
                        if s.current == id {
                            s.messages = v;
                        }
                    });
                }
                Err(e) => me.update(|s| s.error = e.to_string()),
            }
        });
    }

    /// Project folders a new chat can start in (fills `projects`; the first becomes `cwd` when none is set).
    pub fn load_projects(&self) {
        let Some(c) = self.client() else { return };
        let me = self.clone();
        self.spawn(async move {
            if let Ok((_, list)) = c.projects().await {
                me.update(|s| {
                    s.projects = list.into_iter().map(|p| (p.name, p.path)).collect();
                    if s.cwd.is_empty() {
                        if let Some(p) = s.projects.first() {
                            s.cwd = p.1.clone();
                        }
                    }
                });
            }
        });
    }

    pub fn set_cwd(&self, cwd: &str) {
        self.update(|s| s.cwd = cwd.to_string());
    }

    pub fn back(&self) {
        self.update(|s| s.screen = Screen::Chats);
    }

    pub fn set_draft(&self, text: &str) {
        self.update(|s| s.draft = text.to_string());
    }

    /// Send the text (and attachments). While Claude is working the message is queued.
    pub fn send(&self, text: &str) {
        let text = text.trim().to_string();
        let (working, atts) = {
            let s = self.state();
            (s.working, s.attachments.clone())
        };
        let full = (text.clone() + &atts.iter().map(|a| format!("\n@{}", a.path)).collect::<String>()).trim().to_string();
        if full.is_empty() {
            return;
        }
        self.update(|s| {
            s.draft.clear();
            s.attachments.clear();
        });
        if working {
            self.update(|s| s.queued.push(full));
        } else {
            self.start_run(full);
        }
    }

    /// Stop the run; a queued message is kept.
    pub fn stop(&self) {
        let (Some(c), id) = (self.client(), self.state().current) else { return };
        if id.is_empty() {
            return;
        }
        self.spawn(async move {
            let _ = c.stop(&id).await;
        });
    }

    /// Queued message `ix` goes first: stop the run, then start that message.
    pub fn send_now(&self, ix: usize) {
        let text = {
            let mut taken = None;
            self.update(|s| {
                if ix < s.queued.len() {
                    taken = Some(s.queued.remove(ix));
                }
            });
            taken
        };
        let Some(text) = text else { return };
        if !self.state().working {
            self.start_run(text);
            return;
        }
        if let Ok(mut c) = self.0.ctl.lock() {
            c.next = Some(text);
        }
        self.stop();
    }

    pub fn remove_queued(&self, ix: usize) {
        self.update(|s| {
            if ix < s.queued.len() {
                s.queued.remove(ix);
            }
        });
    }

    fn start_run(&self, text: String) {
        let Some(c) = self.client() else {
            self.update(|s| s.error = "No server".into());
            return;
        };
        let mode = self.state().settings.get("mode").and_then(|v| v.as_str().map(String::from)).unwrap_or_else(|| "auto".into());
        let (id, cwd) = {
            let s = self.state();
            (s.current.clone(), if s.current.is_empty() { s.cwd.clone() } else { String::new() })
        };
        self.update(|s| {
            s.working = true;
            s.error.clear();
            s.messages.push(MsgVm { kind: "user".into(), text: text.clone(), ..Default::default() });
            s.messages.push(MsgVm { kind: "working".into(), ..Default::default() });
        });
        let me = self.clone();
        let h = self.spawn(async move {
            let extra = if cwd.is_empty() { json!({}) } else { json!({"cwd": cwd}) };
            let mut ev = c.send(&id, &text, &mode, false, extra);
            while let Some(e) = ev.next().await {
                let fin = e.is_final();
                me.apply(e);
                if fin {
                    break;
                }
            }
            me.finish_run();
        });
        if let Ok(mut ctl) = self.0.ctl.lock() {
            ctl.run = Some(h);
        }
    }

    fn apply(&self, e: RunEvent) {
        self.update(|s| {
            let working_last = matches!(s.messages.last(), Some(m) if m.kind == "working");
            let ins = |s: &mut State, m: MsgVm| {
                let at = if working_last { s.messages.len() - 1 } else { s.messages.len() };
                s.messages.insert(at, m);
            };
            match e.event_type.as_str() {
                "session" => {
                    if !e.payload.is_empty() {
                        s.current = e.payload.clone();
                    }
                }
                "delta" => {
                    let i = if working_last { s.messages.len().saturating_sub(2) } else { s.messages.len().saturating_sub(1) };
                    match s.messages.get_mut(i) {
                        Some(m) if m.kind == "text" => m.text.push_str(&e.payload),
                        _ => ins(s, MsgVm { kind: "text".into(), text: e.payload.clone(), ..Default::default() }),
                    }
                }
                "tool" => ins(
                    s,
                    MsgVm {
                        kind: "tool".into(),
                        text: e.payload.clone(),
                        name: e.raw.get("name").and_then(|v| v.as_str()).unwrap_or("").into(),
                        call_id: e.raw.get("id").and_then(|v| v.as_str()).unwrap_or("").into(),
                        ..Default::default()
                    },
                ),
                "result" => {
                    let id = e.raw.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    if let Some(m) = s.messages.iter_mut().rev().find(|m| m.kind == "tool" && m.call_id == id) {
                        m.detail = e.payload.clone();
                        m.failed = e.error;
                    }
                }
                "queued" => s.status = "Handed to another computer".into(),
                "fail" => s.error = e.payload.clone(),
                _ => {}
            }
        });
    }

    fn finish_run(&self) {
        self.update(|s| {
            s.working = false;
            s.messages.retain(|m| m.kind != "working");
        });
        self.refresh();
        let next = {
            let mut c = match self.0.ctl.lock() {
                Ok(c) => c,
                Err(_) => return,
            };
            c.run = None;
            c.next.take()
        };
        let next = next.or_else(|| {
            let mut t = None;
            self.update(|s| {
                if !s.queued.is_empty() {
                    t = Some(s.queued.remove(0));
                }
            });
            t
        });
        if let Some(t) = next {
            self.start_run(t);
        }
    }

    // ---------- relay events -> notifications ----------

    /// Listen to the relay's event feed and post notifications (done, question, permission, error).
    pub fn start_feed(&self) {
        let Some(c) = self.client() else { return };
        let me = self.clone();
        let h = self.spawn(async move {
            let mut feed = c.events();
            while let Some(ev) = feed.next().await {
                me.on_event(&ev);
            }
        });
        if let Ok(mut ctl) = self.0.ctl.lock() {
            if let Some(old) = ctl.feed.replace(h) {
                old.abort();
            }
        }
    }

    pub fn on_event(&self, ev: &Value) {
        let kind = ev.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let notify_ok = matches!(kind, "done" | "question" | "permission" | "error" | "support") && self.setting_bool("notifications", true);
        if kind == "resolved" {
            let req = ev.get("req").and_then(|v| v.as_str()).unwrap_or("").to_string();
            self.update(|s| {
                if s.prompt.as_ref().map(|p| p.req == req).unwrap_or(false) {
                    s.prompt = None;
                }
            });
            return;
        }
        if kind == "permission" {
            let g = |k: &str| ev.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let p = PromptVm { req: g("req"), tool: g("tool"), text: g("text") };
            if !p.req.is_empty() {
                self.update(|s| s.prompt = Some(p));
            }
        }
        let session = ev.get("session").and_then(|v| v.as_str()).unwrap_or("");
        // you are looking at it
        if !notify_ok || (!session.is_empty() && self.state().current == session && self.state().screen == Screen::Chat) {
            return;
        }
        let title = ev.get("title").and_then(|v| v.as_str()).filter(|t| !t.is_empty()).unwrap_or(match kind {
            "done" => "Claude finished",
            "question" => "Claude has a question",
            "permission" => "Claude asks permission",
            "error" => "Claude hit a problem",
            _ => "Hearth",
        });
        let text = ev.get("text").and_then(|v| v.as_str()).unwrap_or("");
        self.0.platform.notify(tag_of(&format!("{session}{kind}")), title, text);
        if ev.get("call").and_then(|v| v.as_bool()).unwrap_or(false) && matches!(kind, "done" | "question") {
            self.update(|s| s.status = format!("{title}: {text}"));
        }
    }

    /// "allow-once" | "allow-always" | "deny": answers the open permission prompt.
    pub fn answer_prompt(&self, choice: &str) {
        let Some(p) = self.state().prompt else { return };
        self.update(|s| s.prompt = None);
        let Some(c) = self.client() else { return };
        let a = match choice {
            "allow-once" => "allow",
            "allow-always" => "allow_always",
            _ => "deny",
        };
        self.spawn(async move {
            let _ = c.answer(&p.req, a).await;
        });
    }

    // ---------- call, share, attach, speech ----------

    pub fn start_call(&self) {
        self.0.platform.start_call("Hearth call", "Talking with Claude");
        self.update(|s| {
            s.call = CallVm { active: true, muted: false, since_ms: now_ms() };
            s.screen = Screen::Call;
        });
    }

    pub fn hang_up(&self) {
        self.0.platform.stop_call();
        self.update(|s| {
            s.call = CallVm::default();
            s.screen = Screen::Chat;
        });
    }

    pub fn set_muted(&self, m: bool) {
        self.update(|s| s.call.muted = m);
    }

    /// Share the last answer of the open chat.
    pub fn share_last_answer(&self) {
        let s = self.state();
        if let Some(m) = s.messages.iter().rev().find(|m| m.kind == "text") {
            self.0.platform.share_text(&s.current_title, &m.text);
        }
    }

    pub fn copy(&self, text: &str) {
        self.0.platform.copy(text);
    }

    pub fn attach_file(&self) {
        let me = self.clone();
        self.0.platform.pick_file(Box::new(move |p| me.upload(p)));
    }

    pub fn attach_photo(&self) {
        let me = self.clone();
        self.0.platform.take_photo(Box::new(move |p| me.upload(p)));
    }

    fn upload(&self, p: Option<Picked>) {
        let Some(p) = p else { return };
        if p.bytes.len() > 25_000_000 {
            self.update(|s| s.error = "That file is over 25 MB".into());
            return;
        }
        let Some(m) = self.machine() else { return };
        let (id, cwd) = {
            let s = self.state();
            let cwd = if s.current.is_empty() { s.cwd.clone() } else { String::new() };
            (s.current, cwd)
        };
        self.update(|s| {
            s.uploading = true;
            s.error.clear();
        });
        let me = self.clone();
        self.spawn(async move {
            let r = upload(&m, &id, &cwd, &p.name, p.bytes).await;
            me.update(|s| {
                s.uploading = false;
                match r {
                    Ok(path) => s.attachments.push(Attachment { name: p.name.clone(), path }),
                    Err(e) => s.error = e,
                }
            });
        });
    }

    pub fn remove_attachment(&self, ix: usize) {
        self.update(|s| {
            if ix < s.attachments.len() {
                s.attachments.remove(ix);
            }
        });
    }

    /// Speech to text into the draft.
    pub fn dictate(&self) {
        let me = self.clone();
        self.0.platform.dictate("Speak to Claude", Box::new(move |t| {
            if let Some(t) = t {
                me.update(|s| {
                    if !s.draft.is_empty() {
                        s.draft.push(' ');
                    }
                    s.draft.push_str(&t);
                });
            }
        }));
    }
}

fn tiles(u: &Usage, now: i64) -> Vec<Tile> {
    let one = |label: &str, l: &Option<hearth_client::model::Limit>| Tile {
        label: label.into(),
        value: l.as_ref().map(|l| format!("{:.0}%", l.pct)).unwrap_or_else(|| "-".into()),
        sub: l.as_ref().and_then(|l| l.resets_at).map(|r| format!("resets in {}", hearth_client::fmt::dur((r - now).max(0)))).unwrap_or_default(),
    };
    vec![one("Session", &u.session), one("Week", &u.week), one("Opus", &u.opus), one("Sonnet", &u.sonnet)]
}

async fn upload(m: &Machine, session: &str, cwd: &str, name: &str, bytes: Vec<u8>) -> Result<String, String> {
    let enc = |s: &str| s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect::<String>();
    let q = if !session.is_empty() { format!("session={}", enc(session)) } else { format!("cwd={}", enc(cwd)) };
    let url = format!("{}/upload?{q}&name={}", m.base(), enc(name));
    let r = reqwest::Client::new().post(url).bearer_auth(&m.token).header("Content-Type", "application/octet-stream").body(bytes).send().await.map_err(|e| e.to_string())?;
    let code = r.status().as_u16();
    let o: Value = r.json().await.unwrap_or(Value::Null);
    if code != 200 {
        return Err(o.get("error").and_then(|e| e.as_str()).map(String::from).unwrap_or_else(|| "Upload failed".into()));
    }
    o.get("path").and_then(|p| p.as_str()).map(String::from).ok_or_else(|| "Upload failed".into())
}
