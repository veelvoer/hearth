//! A tiny MCP server (stdio) that gives Claude a tool to ask YOU a question with options, and on a server the tools that hand work
//! to your laptop and read the bug list. Claude starts it (see the mcp config of a run). Port of ask-mcp.js.
use serde_json::{json, Value};
use std::io::{BufRead, Read, Write};

fn post(port: u16, path: &str, body: &Value, timeout_ms: u64) -> Option<Value> {
    let b = body.to_string();
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).ok()?;
    let _ = s.set_read_timeout(Some(std::time::Duration::from_millis(timeout_ms)));
    s.write_all(format!("POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}", b.len()).as_bytes()).ok()?;
    let mut out = Vec::new(); s.read_to_end(&mut out).ok()?;
    let t = String::from_utf8_lossy(&out).to_string();
    let (head, body) = t.split_once("\r\n\r\n")?;
    let body = if head.to_lowercase().contains("transfer-encoding: chunked") { dechunk(body) } else { body.to_string() };
    serde_json::from_str(&body).ok()
}
fn dechunk(b: &str) -> String { let mut out = String::new(); let mut rest = b; while let Some((len, more)) = rest.split_once("\r\n") { let n = usize::from_str_radix(len.trim(), 16).unwrap_or(0); if n == 0 { break; } out.push_str(&more[..n.min(more.len())]); rest = more.get(n + 2..).unwrap_or(""); } out }

pub fn run() {
    let port: u16 = std::env::var("CM_RELAY_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(47601);
    let job = std::env::var("CM_JOB").unwrap_or_default();
    let hub = std::env::var("CM_HUB").as_deref() == Ok("1");
    let bugs = std::env::var("CM_BUGS").as_deref() == Ok("1");
    let tool = json!({"name": "ask_user", "description": "Ask the user a question and wait for their answer. Use this whenever you need the user to choose between options or decide something, instead of writing the options in your reply. Give 2-4 short options; the user can also type their own answer.",
        "inputSchema": {"type": "object", "properties": {"question": {"type": "string", "description": "The question, one sentence."}, "options": {"type": "array", "items": {"type": "object", "properties": {"label": {"type": "string"}, "description": {"type": "string"}}, "required": ["label"]}, "description": "The choices."}, "multiSelect": {"type": "boolean", "description": "True if more than one option may be chosen."}}, "required": ["question"]}});
    let hub_tools = json!([
        {"name": "laptop_status", "description": "Check whether the user's own laptop is online right now and which tasks are already waiting for it.", "inputSchema": {"type": "object", "properties": {}}},
        {"name": "run_on_laptop", "description": "Hand a task to the user's own laptop. It runs there in this same project, continuing this conversation, as soon as the laptop is online (right away if it is online now; otherwise when the user next opens it). Use it for anything that must happen on the laptop itself: installing dependencies or tools, running or testing the app, using hardware, a GUI, or files that only exist there. Write the task as complete instructions. After this call, tell the user it is queued and stop; do not try to do the laptop part yourself.", "inputSchema": {"type": "object", "properties": {"task": {"type": "string", "description": "Full instructions for Claude on the laptop."}}, "required": ["task"]}},
        {"name": "list_computers", "description": "List the user's own computers that are linked to this server (for example a laptop and a desktop PC) and whether each is online.", "inputSchema": {"type": "object", "properties": {}}},
        {"name": "run_on_computer", "description": "Hand a task to one of the user's own computers by name (see list_computers). It runs there in this same project, continuing this conversation, once this reply is finished (when that computer is online; otherwise as soon as the user opens it). Use it when the user asks you to set something up or run something on another PC.", "inputSchema": {"type": "object", "properties": {"computer": {"type": "string", "description": "The name of the computer, exactly as list_computers shows it."}, "task": {"type": "string", "description": "What to do there, in full sentences."}}, "required": ["computer", "task"]}}]);
    let bug_tools = json!([
        {"name": "bug_list", "description": "List everything that is still open on the user's list: the support mails they put on it and the other tasks on their Google Tasks list. Each has an id (like HRT-12345 or gt:abc), a title and the text.", "inputSchema": {"type": "object", "properties": {}}},
        {"name": "bug_fixed", "description": "Check an item off the list, after you really did it. For a support mail this also tells the person who wrote.", "inputSchema": {"type": "object", "properties": {"id": {"type": "string", "description": "The id from bug_list, like HRT-12345 or gt:abc."}, "note": {"type": "string", "description": "One short line about what you changed."}}, "required": ["id"]}}]);
    let out = std::sync::Arc::new(std::sync::Mutex::new(std::io::stdout()));
    let send = { let out = out.clone(); move |o: Value| { let mut g = out.lock().unwrap(); let _ = writeln!(g, "{o}"); let _ = g.flush(); } };
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let line = line.trim(); if line.is_empty() { continue; }
        let Ok(m) = serde_json::from_str::<Value>(line) else { continue };
        let method = m.get("method").and_then(|x| x.as_str()).unwrap_or("");
        let id = m.get("id").cloned();
        match method {
            "initialize" => send(json!({"jsonrpc": "2.0", "id": id, "result": {"protocolVersion": m.pointer("/params/protocolVersion").cloned().unwrap_or(json!("2024-11-05")), "capabilities": {"tools": {}}, "serverInfo": {"name": "meter", "version": "1.0.0"}}})),
            "tools/list" => { let mut t = vec![tool.clone()]; if hub { t.extend(hub_tools.as_array().unwrap().iter().cloned()); } if bugs { t.extend(bug_tools.as_array().unwrap().iter().cloned()); } send(json!({"jsonrpc": "2.0", "id": id, "result": {"tools": t}})); }
            "tools/call" => {
                let name = m.pointer("/params/name").and_then(|x| x.as_str()).unwrap_or("").to_string();
                let args = m.pointer("/params/arguments").cloned().unwrap_or(json!({}));
                let job = job.clone(); let send = send.clone();
                std::thread::spawn(move || {
                    let hubby = (hub && ["laptop_status", "run_on_laptop", "list_computers", "run_on_computer"].contains(&name.as_str())) || (bugs && ["bug_list", "bug_fixed"].contains(&name.as_str()));
                    let mut body = json!({"job": job});
                    if let (Some(b), Some(a)) = (body.as_object_mut(), args.as_object()) { for (k, v) in a { b.insert(k.clone(), v.clone()); } }
                    let text = if hubby {
                        body["tool"] = json!(name);
                        match post(port, "/mcp/hub", &body, 20000) { Some(v) => v.get("text").and_then(|t| t.as_str()).filter(|t| !t.is_empty()).unwrap_or("Done.").to_string(), None => "The server could not be reached.".into() }
                    } else {
                        let a = match post(port, "/question", &body, 16 * 60000) { Some(v) => v.get("answer").and_then(|t| t.as_str()).filter(|t| !t.is_empty()).unwrap_or("(no answer)").to_string(), None => "(the user could not be reached)".into() };
                        format!("The user answered: {a}")
                    };
                    send(json!({"jsonrpc": "2.0", "id": id, "result": {"content": [{"type": "text", "text": text}]}}));
                });
            }
            _ => { if id.is_some() && !method.is_empty() { send(json!({"jsonrpc": "2.0", "id": id, "result": {}})); } }
        }
    }
}
