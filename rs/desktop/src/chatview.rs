//! What the chat screen shows: the relay's messages and the records of a live run as the `Msg` rows of rs/ui. Pure, no window needed.
use hearth_client::markdown::{self, Block};
use hearth_client::fmt;
use hearth_relay::util::J;
use serde_json::Value;

/// One row of the chat (the fields of rs/ui's `Msg`, plus the id of a tool call for matching its result).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct UiMsg { pub kind: String, pub lead: String, pub text: String, pub time: String, pub status: String, pub key: String }
impl UiMsg {
    pub fn new(kind: &str, text: &str) -> UiMsg { UiMsg { kind: kind.into(), text: text.into(), ..Default::default() } }
}

/// An assistant's markdown as rows: headings, paragraphs, bullets, quotes, code.
pub fn markdown_rows(text: &str) -> Vec<UiMsg> {
    let mut out = vec![];
    for b in markdown::parse(text) {
        match b {
            Block::Heading { text, .. } => out.push(UiMsg::new("heading", &text)),
            Block::Paragraph { text } => out.push(UiMsg::new("text", &text)),
            Block::Code { lang, text } => out.push(UiMsg { kind: "code".into(), lead: lang.to_uppercase(), text, ..Default::default() }),
            Block::List { items, .. } => for it in items { let (lead, rest) = match it.split_once(": ") { Some((a, b)) if a.chars().count() <= 40 => (a.to_string(), b.to_string()), _ => (String::new(), it) }; out.push(UiMsg { kind: "bullet".into(), lead, text: rest, ..Default::default() }) },
            Block::Quote { text } => out.push(UiMsg::new("quote", &text)),
            Block::Table { headers, rows } => { let mut t = headers.join(" | "); for r in rows { t.push('\n'); t.push_str(&r.join(" | ")); } out.push(UiMsg::new("text", &t)) }
            _ => {}
        }
    }
    out
}

/// The relay's `/sessions/:id/messages` list as rows. `more`: older messages exist (adds "Show earlier messages").
pub fn from_relay(list: &[Value], more: bool) -> Vec<UiMsg> {
    let mut out = vec![];
    if more { out.push(UiMsg::new("earlier", "")); }
    for m in list {
        let ts = m.n("ts") as i64;
        match m.s("role").as_str() {
            "user" => out.push(UiMsg::new("user", &m.s("text"))),
            "assistant" => { out.extend(markdown_rows(&m.s("text"))); if ts > 0 { out.push(UiMsg::new("time", &fmt::clock(ts))); } }
            "tool" => {
                let text = m.s("text"); let name = m.s("name");
                let (lead, rest) = if !name.is_empty() { (name, if m.s("detail").is_empty() { text } else { m.s("detail") }) } else { match text.split_once(' ') { Some((a, b)) => (a.trim_end_matches(':').to_string(), b.to_string()), None => (text, String::new()) } };
                out.push(UiMsg { kind: "tool".into(), lead, text: rest, time: if ts > 0 { fmt::clock(ts) } else { String::new() }, status: if m.t("err") { "failed".into() } else if !m.g("result").is_null() { "done".into() } else { String::new() }, ..Default::default() });
            }
            "meta" => out.push(UiMsg::new("meta", &m.s("text"))),
            _ => {}
        }
    }
    out
}

/// Applies one record of a live run (`delta`, `tool`, `result`, `done`) to the rows of the answer being written.
pub fn apply_record(live: &mut Vec<UiMsg>, rec: &Value) {
    match rec.s("t").as_str() {
        "delta" => { if let Some(l) = live.last_mut().filter(|l| l.kind == "text") { l.text.push_str(&rec.s("text")); } else { live.push(UiMsg::new("text", &rec.s("text"))); } }
        "tool" => live.push(UiMsg { kind: "tool".into(), lead: rec.s("name"), text: rec.s("text"), status: "running".into(), key: rec.s("id"), ..Default::default() }),
        "result" => { if let Some(t) = live.iter_mut().rev().find(|m| m.kind == "tool" && m.key == rec.s("id")) { t.status = if rec.t("error") { "failed".into() } else { "done".into() }; } }
        "done" => { if rec.t("error") && !rec.s("text").is_empty() { live.push(UiMsg::new("text", &rec.s("text"))); } else if rec.n("ms") > 0.0 { live.push(UiMsg::new("meta", &format!("{}s", (rec.n("ms") / 1000.0).round()))); } }
        _ => {}
    }
}

/// History + the live answer + "working" + queued follow-ups: the whole list the screen shows.
pub fn compose(base: &[UiMsg], live: &[UiMsg], working: bool, queued: &[String]) -> Vec<UiMsg> {
    let mut v: Vec<UiMsg> = base.to_vec(); v.extend_from_slice(live);
    if working { v.push(UiMsg::new("working", "")); }
    for q in queued { v.push(UiMsg::new("queued", q)); }
    v
}

#[cfg(test)]
mod tests {
    use super::*; use serde_json::json;
    #[test] fn history_rows() {
        let l = vec![json!({"role": "user", "text": "hi"}), json!({"role": "assistant", "text": "# Title\n\nHello\n\n- a: b\n- c\n\n> q\n\n```rust\nfn x(){}\n```", "ts": 1_000_000}), json!({"role": "tool", "text": "Read: file.rs", "ts": 5, "result": "ok"}), json!({"role": "meta", "text": "1k out"})];
        let r = from_relay(&l, true);
        let kinds: Vec<&str> = r.iter().map(|m| m.kind.as_str()).collect();
        assert_eq!(kinds, ["earlier", "user", "heading", "text", "bullet", "bullet", "quote", "code", "time", "tool", "meta"]);
        assert_eq!((r[4].lead.as_str(), r[4].text.as_str()), ("a", "b")); assert_eq!(r[7].lead, "RUST");
        assert_eq!((r[9].lead.as_str(), r[9].text.as_str(), r[9].status.as_str()), ("Read", "file.rs", "done"));
    }
    #[test] fn live_run() {
        let mut live = vec![];
        for rec in [json!({"t": "delta", "text": "Hel"}), json!({"t": "delta", "text": "lo"}), json!({"t": "tool", "id": "a1", "name": "Bash", "text": "ls"}), json!({"t": "result", "id": "a1", "text": "x"}), json!({"t": "delta", "text": "Done"}), json!({"t": "done", "ms": 4000})] { apply_record(&mut live, &rec); }
        assert_eq!(live.iter().map(|m| (m.kind.as_str(), m.text.as_str(), m.status.as_str())).collect::<Vec<_>>(), [("text", "Hello", ""), ("tool", "ls", "done"), ("text", "Done", ""), ("meta", "4s", "")]);
        let c = compose(&[UiMsg::new("user", "q")], &live, true, &["next".into()]);
        assert_eq!(c.first().unwrap().kind, "user"); assert_eq!(c[c.len() - 2].kind, "working"); assert_eq!(c.last().unwrap().text, "next");
    }
}
