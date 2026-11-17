//! The watch's logic without any UI: usage, chats, voice reply and stop, through the phone (Wear data layer) or
//! straight to the server. Host-testable: the transport is a trait.

use hearth_android::bridge;
use hearth_android::wear::{Reply, Request};
use hearth_client::model::Machine;
use hearth_client::store::Store;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// One request, answered by the phone when one is in reach (`true`) or by the server directly (`false`).
pub trait Transport: Send + Sync {
    fn call(&self, q: Request) -> (Reply, bool);
}

/// Straight to the server (what the watch does when no phone is around). Same handlers the phone runs.
pub struct Direct(pub String);
impl Transport for Direct {
    fn call(&self, q: Request) -> (Reply, bool) {
        (bridge::answer_blocking(&self.0, q), false)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WChat {
    pub id: String,
    pub title: String,
    pub sub: String,
    pub working: bool,
}

#[derive(Clone, Debug, Default)]
pub struct WState {
    pub session_pct: f32,
    pub session_reset: String,
    pub week_line: String,
    pub chats: Vec<WChat>,
    pub status: String,
    pub via_phone: bool,
    pub has_server: bool,
}

pub struct Core {
    dir: String,
    transport: Arc<dyn Transport>,
    state: Mutex<WState>,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

impl Core {
    pub fn new(dir: &str, transport: Arc<dyn Transport>) -> Core {
        Core { dir: dir.to_string(), transport, state: Mutex::new(WState::default()) }
    }

    pub fn state(&self) -> WState {
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
    }

    fn set(&self, f: impl FnOnce(&mut WState)) {
        if let Ok(mut s) = self.state.lock() {
            f(&mut s);
        }
    }

    /// The first server the phone handed over (the file is read again every time: the PhoneLink service writes it).
    pub fn machine(&self) -> Option<Machine> {
        Store::open(Path::new(&self.dir).join("hearth.json")).ok()?.machines().into_iter().find(|m| !m.guest)
    }

    /// Reload usage and chats (blocking).
    pub fn refresh(&self) {
        let Some(m) = self.machine() else {
            self.set(|s| {
                s.has_server = false;
                s.status = "Open Hearth on your phone to share your servers".into();
            });
            return;
        };
        let base = m.base();
        let (chats, via) = self.transport.call(Request { op: "chats".into(), base: base.clone(), ..Default::default() });
        let (usage, _) = self.transport.call(Request { op: "usage".into(), base, ..Default::default() });
        let now = now_ms();
        self.set(|s| {
            s.has_server = true;
            s.via_phone = via;
            if let Some(e) = chats.error_text(&m.name) {
                s.status = e;
            } else if s.status.contains("phone") {
                s.status.clear();
            }
            if let Some(list) = chats.chats {
                s.chats = list
                    .into_iter()
                    .map(|c| WChat { sub: if c.busy { "working now".into() } else { hearth_client::fmt::age(now - c.mtime) }, title: if c.title.is_empty() { "Chat".into() } else { c.title }, id: c.id, working: c.busy })
                    .collect();
            }
            if let Some(u) = &usage.usage {
                let lim = |k: &str| u.get(k).filter(|l| !l.is_null());
                let pct = |k: &str| lim(k).and_then(|l| l.get("pct")).and_then(|v| v.as_f64());
                s.session_pct = pct("session").unwrap_or(0.0) as f32;
                s.session_reset = lim("session").and_then(|l| l.get("resetsAt")).and_then(|v| v.as_i64()).map(|r| format!("Resets in {}", hearth_client::fmt::dur((r - now).max(0)))).unwrap_or_default();
                s.week_line = pct("week").map(|w| format!("Week {w:.0}%")).unwrap_or_default();
            }
        });
    }

    /// Tap on chat `i`: a working chat is stopped; otherwise the user dictates a reply and it is sent.
    /// `dictate` returns the recognised sentence (None = cancelled). Returns a short line for the status.
    pub fn open_chat(&self, i: usize, dictate: &dyn Fn() -> Option<String>) -> String {
        let (Some(m), Some(c)) = (self.machine(), self.state().chats.get(i).cloned()) else { return String::new() };
        let base = m.base();
        let line = if c.working {
            let (r, _) = self.transport.call(Request { op: "stop".into(), base, id: c.id.clone(), ..Default::default() });
            r.error_text(&m.name).unwrap_or_else(|| "Stopped".into())
        } else if let Some(text) = dictate().filter(|t| !t.trim().is_empty()) {
            let (r, _) = self.transport.call(Request { op: "send".into(), base, id: c.id.clone(), text, ..Default::default() });
            match (r.code, r.error_text(&m.name)) {
                (Some(409), _) => "Claude is still busy with this chat".to_string(),
                (_, Some(e)) => e,
                _ => "Sent".to_string(),
            }
        } else {
            String::new()
        };
        self.set(|s| s.status = line.clone());
        line
    }
}
