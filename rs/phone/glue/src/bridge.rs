//! What answers a watch RPC: the phone does it for the watch (over Bluetooth), and the watch runs the very same
//! code itself when no phone is in reach. Mirrors `WatchLink.answer` in Kotlin.

use crate::wear::{valid_id, ChatRow, Reply, Request};
use futures_util::StreamExt;
use hearth_client::api::{ApiError, RelayClient};
use hearth_client::model::Machine;
use hearth_client::store::Store;
use serde_json::json;
use std::time::Duration;

/// The saved server (not a guest one) whose address is `base`.
pub fn find_machine(files_dir: &str, base: &str) -> Option<Machine> {
    let store = Store::open(std::path::Path::new(files_dir).join("hearth.json")).ok()?;
    let want = base.trim_end_matches('/');
    store.machines().into_iter().find(|m| !m.guest && m.base().trim_end_matches('/') == want)
}

fn fail(e: ApiError) -> Reply {
    match e {
        ApiError::WrongToken { .. } => Reply { code: Some(401), error: Some("The phone needs to connect again".into()), ..Default::default() },
        ApiError::Http { code, .. } => Reply { code: Some(code as i64), error: Some(format!("said {code}")), ..Default::default() },
        e => Reply::error(e.to_string()),
    }
}

fn cut(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// Answer one request for `machine` (async; call from any tokio runtime).
pub async fn answer(machine: Machine, q: Request) -> Reply {
    if !valid_id(&q.id) {
        return Reply::error("bad chat");
    }
    let c = match RelayClient::new(machine) {
        Ok(c) => c,
        Err(e) => return fail(e),
    };
    match q.op.as_str() {
        "usage" => match c.usage().await {
            Ok(u) => {
                let lim = |l: &Option<hearth_client::model::Limit>| l.as_ref().map(|l| json!({"pct": l.pct, "resetsAt": l.resets_at}));
                let u = u.unwrap_or_else(|| hearth_client::model::Usage::from_api(&json!({}), 0));
                Reply { usage: Some(json!({"session": lim(&u.session), "week": lim(&u.week)})), ..Default::default() }
            }
            Err(e) => fail(e),
        },
        "chats" => match c.sessions().await {
            Ok(mut s) => {
                s.retain(|x| x.kind != "talk" || !x.title.trim().is_empty());
                s.sort_by_key(|x| std::cmp::Reverse(x.mtime));
                let chats = s.into_iter().take(20).map(|x| ChatRow { busy: x.running || x.busy, title: cut(&x.title, 120), id: x.id, mtime: x.mtime }).collect();
                Reply { chats: Some(chats), ..Default::default() }
            }
            Err(e) => fail(e),
        },
        "turn" => match c.messages(&q.id, Some(30)).await {
            Ok(m) => {
                let (mut asked, mut answer) = (String::new(), String::new());
                for x in m.iter().rev() {
                    match x.role.as_str() {
                        "assistant" if answer.is_empty() && asked.is_empty() => answer = x.text.clone(),
                        "user" if asked.is_empty() => asked = x.text.clone(),
                        _ => {}
                    }
                    if !asked.is_empty() {
                        break;
                    }
                }
                Reply { asked: Some(cut(&asked, 600)), answer: Some(cut(&answer, 4000)), ..Default::default() }
            }
            Err(e) => fail(e),
        },
        "send" => {
            // the run is started once the first record arrives; the relay keeps going without us
            let mut run = c.send(&q.id, &cut(&q.text, 4000), "acceptEdits", false, json!({}));
            match tokio::time::timeout(Duration::from_secs(20), run.next()).await {
                Ok(Some(ev)) if ev.event_type == "fail" => {
                    let code = if ev.payload.contains("409") || ev.payload.to_lowercase().contains("busy") { 409 } else { 500 };
                    Reply { code: Some(code), error: Some(ev.payload), ..Default::default() }
                }
                Ok(Some(_)) => Reply::code(200),
                _ => Reply { code: Some(504), error: Some("no answer".into()), ..Default::default() },
            }
        }
        "stop" => match c.stop(&q.id).await {
            Ok(()) => Reply::code(200),
            Err(e) => fail(e),
        },
        _ => Reply::error("unknown"),
    }
}

/// Blocking version for the JNI entry point and for the watch's direct fallback: one short-lived runtime.
pub fn answer_blocking(files_dir: &str, q: Request) -> Reply {
    let Some(m) = find_machine(files_dir, &q.base) else {
        return Reply::error("This phone does not know that server any more");
    };
    match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt.block_on(answer(m, q)),
        Err(e) => Reply::error(e.to_string()),
    }
}

/// `fn(files_dir, bytes) -> bytes` for [`crate::export_natives!`].
pub fn watch_handler(files_dir: &str, request: &[u8]) -> Vec<u8> {
    crate::wear::serve(request, |q| answer_blocking(files_dir, q))
}
