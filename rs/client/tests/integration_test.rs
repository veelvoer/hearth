//! Integration tests: the client against the real relay (in-process, random port, temp dir under ~/hearth-tmp,
//! a stubbed `claude` first in PATH), and against a tiny fake server for dropped connections.

use futures_util::StreamExt;
use hearth_client::model::Machine;
use hearth_client::{RelayClient, RunEvent};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn machine(port: u16, token: &str) -> Machine {
    Machine {
        name: "test".into(),
        host: "127.0.0.1".into(),
        port,
        token: token.into(),
        secure: false,
        guest: false,
        label: String::new(),
        share: String::new(),
    }
}

async fn collect(mut s: hearth_client::RunEvents) -> Vec<RunEvent> {
    let mut v = vec![];
    let end = tokio::time::timeout(Duration::from_secs(30), async {
        while let Some(e) = s.next().await {
            v.push(e);
        }
    })
    .await;
    assert!(end.is_ok(), "stream did not end: {v:?}");
    v
}

fn text_of(ev: &[RunEvent]) -> String {
    ev.iter().filter(|e| e.event_type == "delta").map(|e| e.payload.as_str()).collect()
}

// ---------- the real relay ----------

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
async fn real_relay_pair_list_send_resume() {
    let base: PathBuf = std::env::var_os("HOME").map(PathBuf::from).unwrap().join("hearth-tmp");
    std::fs::create_dir_all(&base).unwrap();
    let t = tempfile::Builder::new().prefix("client-it-").tempdir_in(&base).unwrap();
    let root = t.path().to_path_buf();
    let (home, bin, proj) = (root.join("home"), root.join("bin"), root.join("proj"));
    for d in [&home, &bin, &proj] {
        std::fs::create_dir_all(d).unwrap();
    }
    let log = root.join("stub.log");
    let stub = bin.join("claude");
    std::fs::write(&stub, STUB).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    // a chat that exists already, with three messages
    let seed_dir = home.join(".claude/projects").join(proj.to_string_lossy().replace(|c: char| !c.is_ascii_alphanumeric(), "-"));
    std::fs::create_dir_all(&seed_dir).unwrap();
    let ts = "2026-11-16T10:00:00.000Z";
    let lines = [
        json!({"type":"user","cwd":proj,"sessionId":"seed1","timestamp":ts,"message":{"role":"user","content":"first question"}}),
        json!({"type":"assistant","cwd":proj,"sessionId":"seed1","timestamp":ts,"message":{"role":"assistant","content":[{"type":"text","text":"first answer"}]}}),
        json!({"type":"user","cwd":proj,"sessionId":"seed1","timestamp":ts,"message":{"role":"user","content":"second question"}}),
    ];
    std::fs::write(seed_dir.join("seed1.jsonl"), lines.iter().map(|l| l.to_string() + "\n").collect::<String>()).unwrap();

    // The relay finds `claude` through PATH and the chats through HOME; this is the only test that touches them.
    std::env::set_var("HOME", &home);
    std::env::set_var("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()));
    std::env::set_var("STUB_LOG", &log);
    std::env::set_var("HEARTH_TEST", "1");

    let port = { let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap(); l.local_addr().unwrap().port() };
    let relay = hearth_relay::boot::embedded(hearth_relay::relay::Opts {
        dir: root.join("relay").to_string_lossy().to_string(),
        port,
        host: "127.0.0.1".into(),
        name: Some("it-relay".into()),
        public_mode: false,
        projects_root: None,
        allow_bypass: false,
        allow_remote: false,
        self_update: false,
        relay_dir: String::new(),
        move_root: String::new(),
        seed_token: String::new(),
        voice: None,
    })
    .await
    .unwrap_or_else(|e| panic!("relay did not start: {e:?}"));

    // pair with the 6-digit code
    let code = relay.new_pair_code();
    let wrong = RelayClient::new(machine(port, "")).unwrap().pair("000000").await;
    assert!(wrong.is_err(), "a wrong code must be refused");
    let paired = RelayClient::new(machine(port, "")).unwrap().pair(&code).await.expect("pair with the code");
    assert!(!paired.token.is_empty());
    let c = RelayClient::new(paired.clone()).unwrap();

    // list sessions, usage, messages tail
    let sessions = c.sessions().await.expect("sessions");
    let seed = sessions.iter().find(|s| s.id == "seed1").unwrap_or_else(|| panic!("seed chat not listed: {sessions:?}"));
    assert_eq!(seed.cwd, proj.to_string_lossy());
    c.usage().await.expect("usage");
    let tail = c.messages("seed1", Some(2)).await.expect("messages");
    assert_eq!(tail.len(), 2, "{tail:?}");
    assert_eq!(tail[1].text, "second question");
    assert!(RelayClient::new(machine(port, "bad")).unwrap().sessions().await.is_err());

    // the event feed
    let mut feed = c.events();

    // a new chat through the stubbed claude
    let ev = collect(c.send("", "hello stub", "auto", false, json!({"cwd": proj, "model": null}))).await;
    let types: Vec<&str> = ev.iter().map(|e| e.event_type.as_str()).collect();
    assert_eq!(types.last(), Some(&"end"), "{types:?}");
    assert!(!types.contains(&"fail"), "{ev:?}");
    let session = ev.iter().find(|e| e.event_type == "session").unwrap_or_else(|| panic!("no session record: {ev:?}"));
    assert_eq!(session.payload, "stub-chat-1");
    assert_eq!(text_of(&ev), "Hello, from the stub.", "{ev:?}");
    let ns: Vec<i64> = ev.iter().map(|e| e.sequence).filter(|n| *n >= 0).collect();
    assert!(ns.windows(2).all(|w| w[1] > w[0]), "numbers must rise: {ns:?}");
    let log1 = std::fs::read_to_string(&log).unwrap();
    assert!(log1.contains("-p --output-format stream-json") && !log1.contains("--resume"), "{log1}");
    assert!(log1.contains("PROMPT:hello stub"));

    // the chat is listed and has the messages
    let msgs = c.messages("stub-chat-1", None).await.expect("messages of the new chat");
    assert!(msgs.iter().any(|m| m.text.contains("hello stub")) && msgs.iter().any(|m| m.text.contains("from the stub")), "{msgs:?}");

    // resume: sending into that chat passes --resume <id>
    let ev2 = collect(c.send("stub-chat-1", "and again", "auto", false, json!({}))).await;
    assert_eq!(ev2.last().map(|e| e.event_type.as_str()), Some("end"), "{ev2:?}");
    assert_eq!(text_of(&ev2), "Hello, from the stub.");
    let log2 = std::fs::read_to_string(&log).unwrap();
    assert!(log2.contains("--resume stub-chat-1") && log2.contains("PROMPT:and again"), "{log2}");

    // sending into a chat that does not exist fails cleanly
    let bad = collect(c.send("no-such-chat", "x", "auto", false, json!({}))).await;
    assert_eq!(bad.last().map(|e| e.event_type.as_str()), Some("fail"), "{bad:?}");

    // the event feed saw the runs finish (a "done" event) or at least delivered something
    let mut got: Vec<Value> = vec![];
    while let Ok(Some(e)) = tokio::time::timeout(Duration::from_secs(3), feed.next()).await {
        got.push(e);
        if got.iter().filter(|e| e["kind"] == "done").count() >= 2 {
            break;
        }
    }
    assert!(got.iter().any(|e| e["kind"] == "done"), "no done event in the feed: {got:?}");

    drop(feed);
    drop(relay);
    // t (and the stub chat files in it) is removed on drop
}

// ---------- dropped connections against a fake server ----------

/// Answers each connection with the next canned response, then closes it. Returns the request lines it saw.
async fn fake(responses: Vec<(u16, &'static str, String)>) -> (u16, tokio::task::JoinHandle<Vec<String>>) {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    let h = tokio::spawn(async move {
        let mut seen = vec![];
        for (code, ctype, body) in responses {
            let (mut s, _) = l.accept().await.unwrap();
            let mut head = Vec::new();
            let mut b = [0u8; 1024];
            while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = s.read(&mut b).await.unwrap();
                if n == 0 {
                    break;
                }
                head.extend_from_slice(&b[..n]);
            }
            let text = String::from_utf8_lossy(&head).to_string();
            seen.push(text.lines().next().unwrap_or("").to_string());
            // read the request body, if there is one
            let want: usize = text.lines().find_map(|x| x.to_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse().unwrap_or(0))).unwrap_or(0);
            let have = head.len() - head.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4).unwrap_or(head.len());
            let mut rest = want.saturating_sub(have);
            while rest > 0 {
                let n = s.read(&mut b).await.unwrap_or(0);
                if n == 0 {
                    break;
                }
                rest -= n.min(rest);
            }
            let _ = s.write_all(format!("HTTP/1.1 {code} X\r\nContent-Type: {ctype}\r\nConnection: close\r\n\r\n{body}").as_bytes()).await;
            let _ = s.shutdown().await;
        }
        seen
    });
    (port, h)
}

fn nd(v: &[Value]) -> String {
    v.iter().map(|x| x.to_string() + "\n").collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn dropped_run_is_joined_again_and_deduped() {
    let first = nd(&[json!({"t":"session","id":"c1","n":0}), json!({"t":"delta","text":"a","n":1}), json!({"t":"delta","text":"b","n":2})]);
    // the relay replays from the number asked for; this fake replays more, to prove the client skips repeats
    let second = format!("\n{}", nd(&[json!({"t":"delta","text":"b","n":2}), json!({"t":"delta","text":"c","n":3}), json!({"t":"end","n":4})]));
    let (port, seen) = fake(vec![(200, "application/x-ndjson", first), (200, "application/x-ndjson", second)]).await;
    let c = RelayClient::new(machine(port, "tok")).unwrap();
    let ev = collect(c.send("", "hi", "auto", false, json!({}))).await;
    assert_eq!(text_of(&ev), "abc", "{ev:?}");
    assert_eq!(ev.last().unwrap().event_type, "end");
    assert_eq!(ev.iter().filter(|e| e.event_type == "end").count(), 1);
    let seen = seen.await.unwrap();
    assert_eq!(seen[0], "POST /sessions/new/send HTTP/1.1");
    assert_eq!(seen[1], "GET /sessions/c1/stream?from=3 HTTP/1.1");
}

#[tokio::test(flavor = "multi_thread")]
async fn rejoin_404_means_the_run_ended() {
    let first = nd(&[json!({"t":"session","id":"c9","n":0}), json!({"t":"delta","text":"x","n":1})]);
    let (port, _seen) = fake(vec![(200, "application/x-ndjson", first), (404, "application/json", r#"{"error":"not running"}"#.into())]).await;
    let c = RelayClient::new(machine(port, "tok")).unwrap();
    let ev = collect(c.send("", "hi", "auto", false, json!({}))).await;
    assert_eq!(ev.last().unwrap().event_type, "end", "{ev:?}");
    assert_eq!(text_of(&ev), "x");
}

#[tokio::test(flavor = "multi_thread")]
async fn send_refused_gives_fail_with_the_relays_words() {
    let (port, _s) = fake(vec![(400, "application/json", r#"{"error":"that folder doesn't exist"}"#.into())]).await;
    let c = RelayClient::new(machine(port, "tok")).unwrap();
    let ev = collect(c.send("", "hi", "auto", false, json!({}))).await;
    assert_eq!(ev.len(), 1);
    assert_eq!((ev[0].event_type.as_str(), ev[0].payload.as_str()), ("fail", "that folder doesn't exist"));
}

#[tokio::test(flavor = "multi_thread")]
async fn event_feed_reconnects() {
    let a = "id: 1\ndata: {\"kind\":\"request\",\"id\":\"r1\"}\n\n: ping\n\ndata: {\"kind\":\"resolved\",\"id\":\"r1\"}\n\n";
    let b = ": hello\n\ndata: {\"kind\":\"done\",\"id\":\"d1\"}\n\n";
    let (port, seen) = fake(vec![(200, "text/event-stream", a.into()), (200, "text/event-stream", b.into())]).await;
    let c = RelayClient::new(machine(port, "tok")).unwrap();
    let mut feed = c.events();
    let mut kinds = vec![];
    let r = tokio::time::timeout(Duration::from_secs(15), async {
        while let Some(e) = feed.next().await {
            kinds.push(e["kind"].as_str().unwrap().to_string());
            if kinds.len() == 3 {
                break;
            }
        }
    })
    .await;
    assert!(r.is_ok(), "{kinds:?}");
    assert_eq!(kinds, ["request", "resolved", "done"]);
    drop(feed);
    assert_eq!(seen.await.unwrap(), ["GET /events HTTP/1.1", "GET /events HTTP/1.1"]);
}
