//! Streaming from the relay: a run (ndjson with resume) and the event feed (SSE with reconnect).
//!
//! Ported from `send` in desktop/tauri/src/main.rs and `read_records` in net.rs.

use crate::model::Machine;
use futures_util::{Stream, StreamExt};
use reqwest::Client;
use serde_json::{json, Value};
use std::pin::Pin;
use std::time::Duration;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

/// How many times a dropped run is joined again before giving up.
pub const MAX_ATTEMPTS: u64 = 60;

/// One record of a run. `raw` is the whole JSON record; the other fields are the common ones.
#[derive(Debug, Clone, PartialEq)]
pub struct RunEvent {
    /// The record type `t`: "session", "delta", "tool", "end", "fail", ...
    pub event_type: String,
    /// `text` (or `id` for "session", `call` for "prefs").
    pub payload: String,
    pub error: bool,
    /// The running number `n` (-1 when the record has none).
    pub sequence: i64,
    pub raw: Value,
}

impl RunEvent {
    pub fn from_value(o: Value) -> RunEvent {
        let event_type = o.get("t").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let key = match event_type.as_str() {
            "session" => "id",
            "prefs" => "call",
            _ => "text",
        };
        RunEvent {
            payload: o.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string(),
            error: o.get("error").and_then(|v| v.as_bool()).unwrap_or(false),
            sequence: o.get("n").and_then(|v| v.as_f64()).map(|n| n as i64).unwrap_or(-1),
            event_type,
            raw: o,
        }
    }
    /// The run is over with this record.
    pub fn is_final(&self) -> bool {
        self.event_type == "end" || self.event_type == "fail"
    }
}

pub type RunEvents = Pin<Box<dyn Stream<Item = RunEvent> + Send>>;
pub type EventFeed = Pin<Box<dyn Stream<Item = Value> + Send>>;

fn client() -> Client {
    Client::builder()
        .connect_timeout(Duration::from_secs(15))
        // the relay sends a heartbeat every 15 s, so a silent minute means the connection is dead
        .read_timeout(Duration::from_secs(60))
        .build()
        .unwrap_or_else(|_| Client::new())
}

/// Reads an SSE / ndjson body piece by piece and hands every complete record (split at `sep`) to `each`.
/// Returns early when `each` says false.
pub async fn read_records(res: reqwest::Response, sep: &str, mut each: impl FnMut(&str) -> bool) {
    let mut s = res.bytes_stream();
    let mut buf = String::new();
    let mut raw: Vec<u8> = Vec::new();
    while let Some(Ok(chunk)) = s.next().await {
        raw.extend_from_slice(&chunk);
        // keep an incomplete UTF-8 sequence for the next piece
        let ok = match std::str::from_utf8(&raw) {
            Ok(t) => t.len(),
            Err(e) => e.valid_up_to(),
        };
        buf.push_str(std::str::from_utf8(&raw[..ok]).unwrap_or(""));
        raw.drain(..ok);
        while let Some(i) = buf.find(sep) {
            let rec: String = buf[..i].to_string();
            buf.drain(..i + sep.len());
            if !each(&rec) {
                return;
            }
        }
    }
}

/// The JSON of the first `data: ` line of an SSE block.
pub fn parse_sse_block(block: &str) -> Option<Value> {
    let line = block.split('\n').find(|x| x.starts_with("data: "))?;
    serde_json::from_str(line[6..].trim()).ok()
}

fn into_stream<T: Send + 'static>(rx: tokio::sync::mpsc::UnboundedReceiver<T>) -> Pin<Box<dyn Stream<Item = T> + Send>> {
    Box::pin(futures_util::stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|v| (v, rx)) }))
}

fn error_text(body: &[u8]) -> Option<String> {
    serde_json::from_slice::<Value>(body).ok()?.get("error")?.as_str().map(|s| s.to_string())
}

/// What the run loop knows: last event number, whether the run said it is over, the chat id.
struct RunState {
    last: i64,
    finished: bool,
    chat: String,
}

/// Passes one record on, once (records with a number at or below the last one are repeats).
fn take(st: &mut RunState, tx: &UnboundedSender<RunEvent>, o: Value) -> bool {
    if let Some(n) = o.get("n").and_then(|n| n.as_f64()) {
        let n = n as i64;
        if n <= st.last {
            return true;
        }
        st.last = n;
    }
    let ev = RunEvent::from_value(o);
    if ev.event_type == "session" && !ev.payload.is_empty() {
        st.chat = ev.payload.clone();
    }
    if ev.is_final() {
        st.finished = true;
    }
    tx.send(ev).is_ok()
}

/// Asks the relay to run a message in a chat (`session` empty = a new chat) and streams every record of the run.
/// `body` holds text, mode, brief and extras (cwd, model, kind, ...); null values are left out.
/// When the connection drops before an `end`/`fail` record, the run is joined again with
/// `GET /sessions/:id/stream?from=<last n + 1>` (backoff, at most 60 attempts; 404 means the run ended).
/// Dropping the stream stops the task.
pub fn run(machine: Machine, session: String, body: Value) -> RunEvents {
    let (tx, rx) = unbounded_channel::<RunEvent>();
    tokio::spawn(async move {
        let name = machine.name.clone();
        let fail = |text: String| {
            let _ = tx.send(RunEvent::from_value(json!({"t": "fail", "text": text})));
        };
        let http = client();
        let auth = |b: reqwest::RequestBuilder| b.bearer_auth(&machine.token);
        let mut st = RunState { last: -1, finished: false, chat: session.clone() };

        let mut b = serde_json::Map::new();
        if let Some(x) = body.as_object() {
            for (k, v) in x {
                if !v.is_null() {
                    b.insert(k.clone(), v.clone());
                }
            }
        }
        let path = format!("/sessions/{}/send", if session.is_empty() { "new" } else { &session });
        let first = match auth(http.post(format!("{}{}", machine.base(), path))).json(&Value::Object(b)).send().await {
            Ok(r) => r,
            Err(e) => return fail(e.to_string()),
        };
        let code = first.status().as_u16();
        if code == 401 {
            return fail(format!("Wrong token for {name}"));
        }
        if code != 200 {
            let body = first.bytes().await.unwrap_or_default();
            return fail(error_text(&body).unwrap_or_else(|| format!("Relay error {code}")));
        }
        read_records(first, "\n", |line| {
            match serde_json::from_str::<Value>(line.trim()) {
                Ok(o) => take(&mut st, &tx, o),
                Err(_) => true,
            }
        })
        .await;

        // The connection dropped before the run said it was over: the run goes on at the computer, so join it again where we left off.
        let mut attempt = 1u64;
        while !st.finished {
            if tx.is_closed() {
                return;
            }
            if attempt > MAX_ATTEMPTS {
                return fail(format!("Lost the connection to {name}. Claude may still be working; open the chat again in a moment."));
            }
            tokio::time::sleep(Duration::from_millis((400 * attempt).min(4000))).await;
            if st.chat.is_empty() {
                return fail(format!("Lost the connection to {name}."));
            }
            let url = format!("{}/sessions/{}/stream?from={}", machine.base(), st.chat, st.last + 1);
            match auth(http.get(url)).send().await {
                Ok(r) if r.status().as_u16() == 404 => {
                    // no run any more: it finished while we were away
                    take(&mut st, &tx, json!({"t": "end"}));
                    return;
                }
                Ok(r) if r.status().as_u16() == 200 => {
                    attempt = 0;
                    read_records(r, "\n", |line| match serde_json::from_str::<Value>(line.trim()) {
                        Ok(o) => take(&mut st, &tx, o),
                        Err(_) => true,
                    })
                    .await;
                }
                _ => {}
            }
            attempt += 1;
        }
    });
    into_stream(rx)
}

/// The relay's event feed (`GET /events`, server-sent events): every `data: {json}` block as a Value
/// (`resolved`, `request`, `question`, `done`, ... with a `kind`). Reconnects by itself: 3 s, doubling to 60 s
/// while the relay is unreachable. Dropping the stream stops the task.
pub fn events(machine: Machine) -> EventFeed {
    let (tx, rx) = unbounded_channel::<Value>();
    tokio::spawn(async move {
        let http = Client::builder().connect_timeout(Duration::from_secs(15)).build().unwrap_or_else(|_| Client::new());
        let mut wait = 3000u64;
        loop {
            let req = http.get(format!("{}/events", machine.base())).bearer_auth(&machine.token).send();
            let res = tokio::select! { r = req => r, _ = tx.closed() => return };
            if let Ok(res) = res {
                if res.status().as_u16() == 200 {
                    wait = 3000;
                    let read = read_records(res, "\n\n", |block| match parse_sse_block(block) {
                        Some(ev) => tx.send(ev).is_ok(),
                        None => true,
                    });
                    tokio::select! { _ = read => {}, _ = tx.closed() => return }
                }
            }
            tokio::select! { _ = tokio::time::sleep(Duration::from_millis(wait)) => {}, _ = tx.closed() => return }
            wait = (wait * 2).min(60000);
        }
    });
    into_stream(rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_event_fields() {
        let ev = RunEvent::from_value(json!({"t": "delta", "text": "hi", "n": 4}));
        assert_eq!((ev.event_type.as_str(), ev.payload.as_str(), ev.sequence), ("delta", "hi", 4));
        assert!(!ev.is_final());
        let s = RunEvent::from_value(json!({"t": "session", "id": "abc"}));
        assert_eq!((s.payload.as_str(), s.sequence), ("abc", -1));
        assert!(RunEvent::from_value(json!({"t": "end"})).is_final());
    }

    #[test]
    fn take_dedupes_by_n() {
        let (tx, mut rx) = unbounded_channel();
        let mut st = RunState { last: -1, finished: false, chat: String::new() };
        for n in [0, 1, 1, 0, 2] {
            take(&mut st, &tx, json!({"t": "delta", "text": "x", "n": n}));
        }
        take(&mut st, &tx, json!({"t": "session", "id": "c1", "n": 3}));
        take(&mut st, &tx, json!({"t": "end", "n": 4}));
        drop(tx);
        let mut ns = vec![];
        while let Ok(e) = rx.try_recv() {
            ns.push(e.sequence);
        }
        assert_eq!(ns, vec![0, 1, 2, 3, 4]);
        assert!(st.finished);
        assert_eq!(st.chat, "c1");
    }

    #[test]
    fn sse_block() {
        assert_eq!(parse_sse_block("id: 4\ndata: {\"kind\":\"done\"}").unwrap()["kind"], "done");
        assert!(parse_sse_block(": ping").is_none());
        assert!(parse_sse_block("data: nope").is_none());
    }
}
