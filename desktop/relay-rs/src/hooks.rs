//! Claude Code hooks (approvals), questions from the ask_user tool, the bug list tools, and support mail. Port of those parts of server.js.
use crate::relay::*;
use crate::util::*;
use serde_json::{json, Value};
use std::sync::Arc;

fn describe(tool: &str, arg: &Value) -> String {
    let k = ["command", "file_path", "pattern", "path", "url"].into_iter().find(|k| arg.get(*k).is_some());
    let s = format!("{}: {}", tool, k.map(|k| clip(&crate::jsu::js_string(arg.g(k)), 160)).unwrap_or_default());
    s.strip_suffix(": ").map(String::from).unwrap_or(s)
}
fn decision(behavior: &str, extra: Option<Value>) -> Value {
    let mut d = json!({"behavior": behavior}); if let Some(Value::Object(m)) = extra { for (k, v) in m { set(&mut d, &k, v); } }
    json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": d}})
}

impl Relay {
    /// A Claude Code hook call. Returns the exact JSON the hook should print.
    pub async fn handle_hook(self: &Arc<Self>, h: Value) -> Value {
        let name = h.s("hook_event_name"); let sid = h.s("session_id"); let tool = h.s("tool_name");
        let mine = self.job(&sid).map(|j| !j.is_done()).unwrap_or(false);   // a chat this relay started: always notify, even when not "away"
        if !mine && (!self.away() || self.clients() == 0) { return json!({}); }
        let base = self.base_for(&sid, &h.s("cwd"));
        let prefs = self.get_prefs(&sid);
        let input = h.g("tool_input").clone();
        if name == "Stop" && !mine {
            let (seconds, last) = crate::core::turn_info(&sid);
            if seconds >= MIN_DONE_SECONDS { self.publish(merge(&base, &json!({"kind": "done", "call": true, "text": clip(&last, 200), "seconds": seconds}))); }
        } else if tool == "AskUserQuestion" && (name == "PreToolUse" || name == "PermissionRequest") {
            let mut lq = self.last_question.lock().unwrap();
            if now_ms() - lq.get(&sid).copied().unwrap_or(0.0) > 10000.0 {
                lq.insert(sid.clone(), now_ms()); drop(lq);
                let q0 = input.g("questions").as_array().and_then(|a| a.first().cloned()).unwrap_or_else(obj);
                let t = if q0.s("question").is_empty() { "Claude has a question".into() } else { q0.s("question") };
                self.publish(merge(&base, &json!({"kind": "question", "call": if mine { prefs.s("call") == "attention" } else { true }, "text": clip(&t, 200)})));
            }
        } else if name == "PermissionRequest" {
            let cmd = input.s("command");
            if tool == "Bash" {
                if let Some(dr) = self.deny_rules().into_iter().find(|r| cmd.contains(&r.s("prefix"))) {
                    self.log_decision(json!({"tool": tool, "text": clip(&cmd, 120), "result": "denied by your rule", "rule": dr.s("label"), "chat": sid}));
                    return decision("deny", Some(json!({"message": format!("Blocked by a rule you made in Hearth: \"{}\"", dr.s("label"))})));
                }
                if crate::perms::any_match(&self.allow_rules(), &tool, &input) {
                    self.log_decision(json!({"tool": tool, "text": clip(&cmd, 120), "result": "allowed by your rule", "chat": sid}));
                    return decision("allow", None);
                }
            }
            if self.get_prefs(&sid).get("auto") == Some(&Value::Bool(true)) && self.bypass_ok() {
                self.log_decision(json!({"tool": tool, "text": describe(&tool, &input), "result": "allowed (Full auto)", "chat": sid}));
                return decision("allow", None);
            }
            // the same prompt can reach us twice (a hook in your Claude settings plus the one added for this run): share one question
            let key = format!("{sid}|{tool}|{input}");
            let mut rx = {
                let mut g = self.pending_by_key.lock().unwrap();
                match g.get(&key) {
                    Some(tx) => tx.subscribe(),
                    None => {
                        let (tx, rx) = tokio::sync::broadcast::channel(1);
                        g.insert(key.clone(), tx.clone());
                        let req = rand_hex(4);
                        let sug = crate::perms::suggest(&tool, &input).map(|r| json!({"label": r.s("label")})).unwrap_or(Value::Null);
                        let ev = merge(&base, &json!({"kind": "permission", "call": if mine { prefs.s("call") == "attention" } else { true }, "req": req, "tool": tool, "text": describe(&tool, &input), "input": crate::core::tool_input(&tool, Some(&input)), "suggest": sug}));
                        let me = self.clone(); let k2 = key.clone(); let tx2 = tx.clone();
                        self.add_pending(&req, ev.clone(), Box::new(move |b: Value| { me.pending_by_key.lock().unwrap().remove(&k2); let _ = tx2.send(b); }));
                        let me = self.clone(); let k3 = key.clone(); let wait = if mine { JOB_PERMISSION_WAIT_MS } else { PERMISSION_WAIT_MS };
                        let req2 = req.clone();
                        tokio::spawn(async move { tokio::time::sleep(std::time::Duration::from_millis(wait)).await; if me.pending.lock().unwrap().remove(&req2).is_some() { me.pending_by_key.lock().unwrap().remove(&k3); let _ = tx.send(Value::Null); } });
                        self.publish(ev);
                        rx
                    }
                }
            };
            let d = rx.recv().await.unwrap_or(Value::Null);
            let ds = d.as_str().unwrap_or("").to_string();
            self.log_decision(json!({"tool": tool, "text": describe(&tool, &input), "result": if ds == "allow" || ds == "allow_always" { "allowed by you" } else if ds == "deny" { "denied by you" } else { "no answer in time" }, "chat": sid}));
            if ds == "allow" || ds == "deny" || ds == "allow_always" {
                let mut extra = obj();
                if ds == "deny" { set(&mut extra, "message", json!("Denied in Hearth")); }
                // "don't ask again": a command (its first words) is allowed for good; any other tool is allowed for the rest of this chat's run
                if ds == "allow_always" && tool == "Bash" { self.add_rule(crate::perms::suggest(&tool, &input)); }
                if ds == "allow_always" && !tool.is_empty() && tool != "Bash" { set(&mut extra, "updatedPermissions", json!([{"type": "addRules", "rules": [{"toolName": tool}], "behavior": "allow", "destination": "session"}])); }
                return decision(if ds == "deny" { "deny" } else { "allow" }, Some(extra));
            }
        }
        json!({})
    }

    /// From the ask_user tool of a chat this relay started: shows the question and waits for the answer.
    pub async fn question(self: &Arc<Self>, mut q: Value) -> Value {
        let job = self.job(&q.s("job"));
        let sid = job.as_ref().map(|j| j.sid()).unwrap_or_default();
        let base = self.base_for(&sid, &job.as_ref().map(|j| j.cwd.clone()).unwrap_or_default());
        let prefs = self.get_prefs(&sid);
        if let Some(s) = q.get("options").and_then(|o| o.as_str()).map(String::from) { set(&mut q, "options", serde_json::from_str(&s).unwrap_or(json!([]))); }
        let options: Vec<Value> = arr(q.g("options")).iter().take(6).map(|o| json!({"label": clip(&o.s("label"), 120), "description": clip(&o.s("description"), 240)})).filter(|o| !o.s("label").is_empty()).collect();
        let req = rand_hex(4);
        let (tx, rx) = tokio::sync::oneshot::channel::<Value>();
        let qs = q.s("question");
        let ev = merge(&base, &json!({"kind": "question", "call": prefs.s("call") == "attention", "req": req, "question": clip(&qs, 400), "options": options, "multi": q.t("multiSelect"), "text": clip(if qs.is_empty() { "Claude has a question" } else { &qs }, 200)}));
        self.add_pending(&req, ev.clone(), Box::new(move |a: Value| { let _ = tx.send(a); }));
        self.publish(ev);
        let answer = match tokio::time::timeout(std::time::Duration::from_millis(JOB_PERMISSION_WAIT_MS), rx).await {
            Ok(Ok(a)) => if a.is_null() { "(no answer in time)".to_string() } else { vstr(&a) },
            _ => { self.pending.lock().unwrap().remove(&req); "(no answer in time)".into() }
        };
        json!({"answer": answer})
    }

    // ───────── support mail ─────────
    pub fn support_text(m: &Value) -> String { format!("Support {} {} ({}) from {}\nTitle: {}\n\n{}", if m.s("kind") == "follow-up" { "follow-up" } else { "request" }, m.s("id"), m.s("category"), m.s("from"), m.s("title"), m.s("text")) }
    pub async fn google_token(self: &Arc<Self>) -> Result<String, String> {
        let g = self.cfg("googleTasks"); if g.s("refreshToken").is_empty() { return Err("Google Tasks is not connected on this server".into()); }
        let url = std::env::var("HEARTH_GOOGLE_TOKEN_URL").unwrap_or_else(|_| "https://oauth2.googleapis.com/token".into());
        let form = qbuild(&[("client_id", g.s("clientId")), ("client_secret", g.s("clientSecret")), ("refresh_token", g.s("refreshToken")), ("grant_type", "refresh_token".into())]);
        let r = self.http.post(url).header("Content-Type", "application/x-www-form-urlencoded").body(form).timeout(std::time::Duration::from_secs(15)).send().await.map_err(|e| e.to_string())?;
        let st = r.status().as_u16();
        let tj: Value = r.json().await.unwrap_or_else(|_| obj());
        if tj.s("access_token").is_empty() { return Err(format!("Google said no: {}", if !tj.s("error_description").is_empty() { tj.s("error_description") } else if !tj.s("error").is_empty() { tj.s("error") } else { st.to_string() })); }
        Ok(tj.s("access_token"))
    }
    fn gt_base() -> String { std::env::var("HEARTH_GOOGLE_TASKS_URL").unwrap_or_else(|_| "https://tasks.googleapis.com/tasks/v1/lists/@default/tasks".into()) }
    pub async fn google_task(self: &Arc<Self>, title: &str, notes: &str) -> (bool, String) {
        let t = match self.google_token().await { Ok(t) => t, Err(e) => return (false, e) };
        match self.http.post(Self::gt_base()).bearer_auth(t).json(&json!({"title": clip(title, 1000), "notes": clip(notes, 8000)})).timeout(std::time::Duration::from_secs(15)).send().await {
            Ok(r) if r.status().is_success() => (true, String::new()),
            Ok(r) => (false, format!("Google Tasks answered {}", r.status().as_u16())),
            Err(e) => (false, e.to_string()),
        }
    }
    async fn gt_access(self: &Arc<Self>) -> String {
        if self.cfg("googleTasks").s("refreshToken").is_empty() { return String::new(); }
        { let g = self.gt_token.lock().unwrap(); if !g.1.is_empty() && now_ms() - g.0 < 45.0 * 60000.0 { return g.1.clone(); } }
        if let Ok(t) = self.google_token().await { *self.gt_token.lock().unwrap() = (now_ms(), t); }
        self.gt_token.lock().unwrap().1.clone()
    }
    pub async fn gt_open(self: &Arc<Self>) -> Vec<Value> {
        let t = self.gt_access().await; if t.is_empty() { return vec![]; }
        let Ok(r) = self.http.get(format!("{}?showCompleted=false&showHidden=false&maxResults=100", Self::gt_base())).bearer_auth(t).timeout(std::time::Duration::from_secs(15)).send().await else { return vec![] };
        let j: Value = r.json().await.unwrap_or_else(|_| obj());
        arr(j.g("items")).into_iter().filter(|x| x.s("status") != "completed" && !x.s("title").trim().is_empty()).collect()
    }
    pub async fn gt_complete(self: &Arc<Self>, id: &str) -> bool {
        let t = self.gt_access().await; if t.is_empty() { return false; }
        self.http.patch(format!("{}/{}", Self::gt_base(), enc(id).replace('+', "%20"))).bearer_auth(t).json(&json!({"status": "completed"})).timeout(std::time::Duration::from_secs(15)).send().await.map(|r| r.status().is_success()).unwrap_or(false)
    }
    /// Tells the person who wrote a support request that it is on the list / done (when this server has a support desk).
    pub fn support_notify(self: &Arc<Self>, id: &str, event: &str, note: &str) {
        let c = self.read_cfg();
        if c.get("autoAnswer") == Some(&Value::Bool(false)) || !regex::Regex::new(r"^HRT-\d{4,6}$").unwrap().is_match(id) || c.s("token").is_empty() { return; }
        let url = format!("{}/owner/notify", std::env::var("HEARTH_SUPPORT_URL").ok().or_else(|| Some(c.s("supportUrl")).filter(|s| !s.is_empty())).unwrap_or_else(|| "http://127.0.0.1:47610".into()).trim_end_matches('/'));
        let body = json!({"id": id, "event": event, "note": note}); let tok = c.s("token"); let http = self.http.clone();
        tokio::spawn(async move { let _ = http.post(url).bearer_auth(tok).json(&body).timeout(std::time::Duration::from_secs(10)).send().await; });
    }
    /// What to do with a support mail: "claude" starts a chat with it, "list" saves it, anything else does nothing.
    pub async fn support_act(self: &Arc<Self>, clean: &Value, b: &str, machine: &str) -> Value {
        if b == "claude" {
            let base = std::env::current_exe().ok().and_then(|p| p.ancestors().nth(4).map(|x| x.to_string_lossy().to_string())).unwrap_or_default();
            let want = self.cfg("supportCwd").as_str().unwrap_or("").trim().to_string();
            let is_hearth = |c: &str| !c.is_empty() && if !want.is_empty() { c == want } else { basename(c) == "claude_app" || std::path::Path::new(c).join("desktop").join("relay").join("server.js").exists() };
            let target = crate::core::sessions(&Default::default(), None, 60, &Default::default()).into_iter().filter(|x| is_hearth(&x.s("cwd")) && self.visible_session(&x.s("cwd"), x.n("mtime"))).max_by(|a, b| a.n("mtime").partial_cmp(&b.n("mtime")).unwrap());
            let cwd = match &target { Some(t) => t.s("cwd"), None => if is_hearth(&base) && self.ok_dir(&base) { base.clone() } else { self.root.clone().unwrap_or_default() } };
            let mut files = Vec::new();
            let fd = self.dir.join("support-files").join(clean.s("id"));
            if regex::Regex::new(r"^HRT-\d{3,8}$").unwrap().is_match(&clean.s("id")) && !cwd.is_empty() {
                if let Ok(rd) = std::fs::read_dir(&fd) {
                    let up = std::path::Path::new(&cwd).join(".uploads"); let _ = std::fs::create_dir_all(&up);
                    let strip = regex::Regex::new(r"^\d+-\d+-").unwrap();
                    for e in rd.flatten() { let n = e.file_name().to_string_lossy().to_string(); let to = up.join(format!("{}-{}", now_i(), strip.replace(&n, ""))); if std::fs::copy(e.path(), &to).is_ok() { files.push(to.to_string_lossy().to_string()); } }
                }
            }
            let text = format!("A support report came in ({}). Look into it here in this project, fix it, and tell me what you changed. Do not email anyone.\n\n{}{}", clean.s("id"), Self::support_text(clean), if files.is_empty() { String::new() } else { format!("\n\nThe person sent these pictures/files:\n{}", files.iter().map(|f| format!("@{f}")).collect::<Vec<_>>().join("\n")) });
            let mut body = json!({"text": text, "mode": "acceptEdits"}); if !cwd.is_empty() { set(&mut body, "cwd", json!(cwd)); }
            let tid = target.as_ref().map(|t| t.s("id")).unwrap_or_default();
            match self.start_job(&body, if tid.is_empty() { "new" } else { &tid }) {
                Err((code, m)) => { self.log(&format!("support to Claude: {m}")); json!({"ok": false, "text": if code == 409 { "Claude is still working in your Hearth chat. Try again when it is done.".to_string() } else { m }, "session": tid, "cwd": cwd}) }
                Ok(s) => {
                    let mut sid = tid.clone();
                    for _ in 0..40 { if !sid.is_empty() { break; } sid = s.job.sid(); if sid.is_empty() { tokio::time::sleep(std::time::Duration::from_millis(150)).await; } }
                    json!({"ok": true, "text": if target.is_some() { "Sent to your Hearth chat. Opening it…" } else { "Started a Hearth chat. Opening it…" }, "session": sid, "cwd": cwd})
                }
            }
        } else if b == "list" {
            if self.read_list().iter().any(|x| x.s("id") == clean.s("id") && !x.t("done")) { return json!({"ok": true, "text": "Already on your list."}); }
            let (gok, gwhy) = self.google_task(&format!("Support {}: {}", clean.s("id"), clean.s("title")), &Self::support_text(clean)).await;
            let item = json!({"id": clean.s("id"), "title": clean.s("title"), "from": clean.s("from"), "text": clean.s("text"), "at": now_i(), "google": gok});
            let mut l = vec![item]; l.extend(self.read_list()); l.truncate(300); self.save_list(&l);
            self.support_notify(&clean.s("id"), "listed", "");
            let text = if gok { "Added to your Google Tasks.".to_string() } else { format!("Saved in your Hearth list. Google Tasks: {gwhy}") };
            self.publish(json!({"kind": "support", "session": "", "cwd": "", "title": clean.s("title"), "machine": if machine.is_empty() { self.name() } else { machine.to_string() }, "ts": now_i(), "call": false, "text": text}));
            json!({"ok": true, "text": text})
        } else { json!({"ok": true, "text": "Ignored."}) }
    }
    pub fn support_incoming(self: &Arc<Self>, m: &Value) -> (u16, Value) {
        let clean = json!({"kind": if m.s("kind") == "follow-up" { "follow-up" } else { "new" }, "id": clip(&m.s("id"), 20), "title": clip(&m.s("title"), 120), "category": clip(&{ let c = m.s("category"); if c.is_empty() { "other".to_string() } else { c } }, 20), "from": clip(&m.s("from"), 120), "text": clip(&m.s("text"), 5000)});
        if self.pending.lock().unwrap().values().filter(|p| p.event.s("kind") == "support").count() >= 50 { return (429, json!({"error": "too many support mails waiting"})); }
        if let Some(files) = m.get("files").and_then(|f| f.as_array()) {
            if regex::Regex::new(r"^HRT-\d{3,8}$").unwrap().is_match(&clean.s("id")) {
                let d = self.dir.join("support-files").join(clean.s("id")); let _ = std::fs::create_dir_all(&d);
                for (i, f) in files.iter().take(3).enumerate() {
                    use base64::Engine;
                    let Ok(buf) = base64::engine::general_purpose::STANDARD.decode(f.s("data")) else { continue };
                    if buf.is_empty() || buf.len() > 2_000_000 { continue; }
                    let name0 = { let n = f.s("name"); if n.is_empty() { "file".into() } else { n } };
                    let safe: String = name0.chars().map(|c| if c.is_alphanumeric() || "_.- ()".contains(c) { c } else { '_' }).collect();
                    let safe = regex::Regex::new(r"\.{2,}").unwrap().replace_all(&safe, ".").to_string();
                    let tail: String = { let n = safe.chars().count(); safe.chars().skip(n.saturating_sub(80)).collect() };
                    let _ = write_private(&d.join(format!("{}-{}-{}", now_i(), i, tail)), &buf);
                }
            }
        }
        let req = rand_hex(4);
        let cat = match clean.s("category").as_str() { "bug" => "bug report", "question" => "question", "idea" => "idea", _ => "support mail" };
        let ev = json!({"kind": "support", "req": req, "session": "", "title": clean.s("title"), "cwd": "", "machine": self.name(), "ts": now_i(), "call": false, "ticket": clean.s("id"),
            "text": format!("{} {} from {}: {}", if clean.s("kind") == "follow-up" { "Follow-up on a" } else { "New" }, cat, clean.s("from"), clip(&clean.s("text").split_whitespace().collect::<Vec<_>>().join(" "), 160))});
        let me = self.clone(); let mc = self.name();
        self.add_pending(&req, ev.clone(), Box::new(move |b: Value| { let me2 = me.clone(); let b = vstr(&b); tokio::spawn(async move { me2.support_act(&clean, &b, &mc).await; }); }));
        self.publish(ev);
        (200, json!({"ok": true}))
    }

    /// run_on_laptop / laptop_status / bug_list / bug_fixed from a chat on this server (the mcp-ask helper calls this).
    pub async fn mcp_hub(self: &Arc<Self>, q: Value) -> Value {
        let job = self.job(&q.s("job"));
        let tool = q.s("tool");
        if tool == "bug_list" || tool == "bug_fixed" {
            if !job.as_ref().map(|j| j.bugs).unwrap_or(false) { return json!({"text": "The list is not available in this chat."}); }
            let mut all = self.read_list();
            if tool == "bug_list" {
                let open: Vec<Value> = all.iter().filter(|x| !x.t("done")).cloned().collect();
                let re = regex::Regex::new(r"Support (HRT-\d+)").unwrap();
                let tasks: Vec<Value> = self.gt_open().await.into_iter().filter(|t| !re.captures(&t.s("title")).map(|m| open.iter().any(|x| x.s("id") == m[1])).unwrap_or(false)).collect();
                let mut rows: Vec<String> = open.iter().map(|x| format!("[{}] {} (support mail from {})\n{}", x.s("id"), x.s("title"), x.s("from"), x.s("text"))).collect();
                rows.extend(tasks.iter().map(|t| format!("[gt:{}] {} (Google Tasks)\n{}", t.s("id"), t.s("title"), t.s("notes")).trim().to_string()));
                return json!({"text": if rows.is_empty() { "The list is empty: nothing to do.".to_string() } else { format!("{} open item(s):\n\n{}", rows.len(), rows.join("\n\n---\n")) }});
            }
            let id = q.s("id"); let note = clip(&q.s("note"), 500);
            if let Some(g) = id.strip_prefix("gt:") { let ok = self.gt_complete(g).await; return json!({"text": if ok { "Checked off in Google Tasks." } else { "Could not check it off in Google Tasks (is it connected?)." }}); }
            let Some(it) = all.iter_mut().find(|x| x.s("id") == id) else { return json!({"text": format!("No item with id {id}. Use bug_list to see the ids.")}) };
            set(it, "done", json!(true)); set(it, "doneAt", json!(now_i())); set(it, "note", json!(note));
            let itid = it.s("id");
            self.save_list(&all);
            self.support_notify(&itid, "fixed", &note);
            let task = self.gt_open().await.into_iter().find(|t| t.s("title").contains(&format!("Support {itid}")));
            if let Some(t) = &task { self.gt_complete(&t.s("id")).await; }
            return json!({"text": format!("Marked {itid} as fixed{}", if task.is_some() { " and checked off in Google Tasks." } else { "." })});
        }
        crate::agent::hub_tool(self, &q, job).await
    }
}
