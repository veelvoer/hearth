//! Claude runs that outlive the connection that started them: startJob, the stream of events, Stop, "done" notices,
//! the plan limit, routines and workflows. Port of the job part of server.js.
use crate::relay::*;
use crate::util::*;
use bytes::Bytes;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc::UnboundedSender;

pub type OnDone = Box<dyn FnOnce(&JobView) + Send>;
/// What a finished run looked like, for the ones waiting on it.
#[derive(Clone)]
pub struct JobView { pub sid: String, pub key: String, pub error: bool, pub result_text: String, pub limit_hit: bool }

#[derive(Default)]
pub struct JobState {
    pub sid: String,
    pub events: Vec<Value>,
    pub listeners: Vec<UnboundedSender<Bytes>>,
    pub done: bool,
    pub text: String,
    pub error: bool,
    pub result_text: String,
    pub limit_hit: bool,
    pub limit_reset: f64,
    pub auth_fail: bool,
    pub want_call: String,
    pub want_kind: String,
    pub want_root: bool,
    pub want_auto: bool,
    pub routine: String,
    pub by: String,
    pub by_req: String,
    pub by_member: String,
    pub on_done: Vec<OnDone>,
}
pub struct Job {
    pub key: String,
    pub cwd: String,
    pub started: f64,
    pub root_launch: bool,
    pub bugs: bool,
    pub resume_body: Value,
    pub pid: Mutex<Option<u32>>,
    pub st: Mutex<JobState>,
}
impl Job {
    pub fn is_done(&self) -> bool { self.st.lock().unwrap().done }
    pub fn sid(&self) -> String { self.st.lock().unwrap().sid.clone() }
    /// The chat id, or the temporary key until Claude said which chat it is.
    pub fn id(&self) -> String { let s = self.sid(); if s.is_empty() { self.key.clone() } else { s } }
    pub fn on_done(&self, f: OnDone) { self.st.lock().unwrap().on_done.push(f); }
    pub fn emit(&self, mut o: Value) {
        let mut g = self.st.lock().unwrap();
        set(&mut o, "n", json!(g.events.len()));
        let line = Bytes::from(format!("{}\n", o));
        if g.events.len() < 20000 { g.events.push(o); }
        g.listeners.retain(|l| l.send(line.clone()).is_ok());
    }
}

/// The Claude program started the right way on this system (inside Flatpak through the host; .cmd files through a shell on Windows).
pub fn claude_command(claude: &str, args: &[String], cwd: &str) -> tokio::process::Command {
    let mut c = if std::env::var("FLATPAK_ID").is_ok() {
        let mut c = tokio::process::Command::new("flatpak-spawn");
        c.arg("--host").arg(format!("--directory={cwd}")).args(["bash", "-lc", "exec claude \"$@\"", "claude"]).args(args);
        c
    } else if cfg!(windows) && (claude.to_lowercase().ends_with(".cmd") || claude.to_lowercase().ends_with(".bat")) {
        let mut c = tokio::process::Command::new("cmd");
        c.arg("/C").arg(claude).args(args);
        c
    } else { let mut c = tokio::process::Command::new(claude); c.args(args); c };
    c.current_dir(cwd);
    #[cfg(windows)] { c.creation_flags(0x08000000); }
    c.kill_on_drop(false);
    c
}

pub fn detect_call(text: &str) -> Option<&'static str> {
    let re = regex::Regex::new(r"(?i)\b(call|ring|phone) me\b").unwrap();
    if !re.is_match(text) { return None; }
    let att = regex::Regex::new(r"(?i)permission|need me|stuck|question|attention|input|approve|asks?\b").unwrap();
    Some(if att.is_match(text) { "attention" } else { "done" })
}
fn auth_re() -> regex::Regex { regex::Regex::new(r"(?i)OAuth (session|token)|Failed to authenticate|authentication_error|not logged in|please run /?login|invalid[_ ]api[_ ]key").unwrap() }
pub fn limit_re() -> regex::Regex { regex::Regex::new(r"(?i)(usage|rate)\s+limit\s+(reached|hit)|hit your (usage |session |weekly )?limit|reached your (usage |session |weekly )?limit|limit reached").unwrap() }

pub struct Started { pub job: Arc<Job> }
pub type StartResult = Result<Started, (u16, String)>;

impl Relay {
    /// Starts a turn: a new chat (sid "new") or a message in an existing chat. Port of startJob.
    pub fn start_job(self: &Arc<Self>, body: &Value, sid_param: &str) -> StartResult {
        if let Some((_, h)) = self.limit_waits.lock().unwrap().remove(sid_param) { h.abort(); }   // you wrote again yourself: no need to wait for the reset
        let Some(claude) = find_claude() else { return Err((500, "Claude Code was not found on this computer. Install it first: https://claude.com/claude-code".into())) };
        let bmode = body.s("mode");
        let mut mode = if MODES.contains(&bmode.as_str()) { bmode.clone() } else if self.o.public_mode { "auto".into() } else { "acceptEdits".into() };
        if sid_param != "new" && self.get_prefs(sid_param).get("auto") == Some(&Value::Bool(true)) && self.bypass_ok() { mode = "bypassPermissions".into(); }
        if mode == "bypassPermissions" && !self.bypass_ok() { mode = "auto".into(); }
        let settings_file = self.dir.join("run-settings.json");
        let wait_s = JOB_PERMISSION_WAIT_MS / 1000;
        let hook_cmd = format!("curl -s -m {wait_s} -X POST -H \"Content-Type: application/json\" --data-binary @- http://127.0.0.1:{}/hook", self.o.port);
        let _ = std::fs::create_dir_all(&self.dir);
        let _ = std::fs::write(&settings_file, json!({"hooks": {"PermissionRequest": [{"matcher": "", "hooks": [{"type": "command", "command": hook_cmd, "timeout": wait_s}]}]}}).to_string());
        let mut args: Vec<String> = ["-p", "--output-format", "stream-json", "--verbose", "--include-partial-messages", "--permission-mode", &mode, "--settings", &settings_file.to_string_lossy()].iter().map(|s| s.to_string()).collect();
        let kind = if body.s("kind") == "talk" || (sid_param != "new" && self.get_prefs(sid_param).s("kind") == "talk") { "talk" } else { "code" };
        let model = body.s("model");
        if kind == "talk" { args.extend(["--model".into(), "haiku".into(), "--disallowedTools".into(), TALK_DENY.into()]); }
        else if ["opus", "sonnet", "haiku", "fable"].contains(&model.as_str()) { args.extend(["--model".into(), if model == "fable" { "claude-fable-5-1".into() } else { model.clone() }]); }
        let effort = body.s("effort");
        if kind != "talk" && ["low", "medium", "high", "xhigh", "max"].contains(&effort.as_str()) { args.extend(["--effort".into(), effort]); }
        let (cwd, key);
        if sid_param == "new" {
            cwd = if kind == "talk" { std::path::Path::new(self.root.as_deref().or(self.move_root.as_deref()).map(String::from).unwrap_or_else(|| home().to_string_lossy().to_string()).as_str()).join("Talks").to_string_lossy().to_string() } else { body.s("cwd") };
            if kind == "talk" { let _ = std::fs::create_dir_all(&cwd); }
            if !is_dir(std::path::Path::new(&cwd)) { return Err((400, "that folder doesn't exist on this computer".into())); }
            if !self.ok_dir(&cwd) { return Err((403, if self.outside_ok() { "that folder doesn't exist on this server".into() } else { "new chats must be inside the projects folder (you can allow other folders in Settings)".into() })); }
            key = format!("new-{}", rand_hex(4));
        } else {
            let Some(f) = crate::core::find_session(sid_param) else { return Err((404, "no such session".into())) };
            let s = crate::core::summary(&f);
            if !self.visible_session(&s.cwd, f.mtime) { return Err((403, "that chat is outside the projects folder".into())); }
            if self.job_busy(sid_param) { return Err((409, "Claude is still working on this chat".into())); }
            cwd = if s.cwd.is_empty() { home().to_string_lossy().to_string() } else { s.cwd.clone() };
            if !std::path::Path::new(&cwd).exists() { return Err((409, "The project's files haven't reached this computer yet (still syncing). Try again in a moment.".into())); }
            if self.root.is_some() && !self.ok_dir(&cwd) { return Err((403, "that chat is outside the projects folder".into())); }
            args.extend(["--resume".into(), sid_param.into()]);
            key = sid_param.to_string();
        }
        let no_root = body.t("noRoot");
        let bugs_on = !no_root && kind != "talk";
        let mcp_file = self.dir.join(format!("ask-{key}.json"));
        let exe = std::env::current_exe().unwrap_or_default().to_string_lossy().to_string();
        let env = json!({"CM_RELAY_PORT": self.o.port.to_string(), "CM_JOB": key, "CM_HUB": if self.o.public_mode { "1" } else { "" }, "CM_BUGS": if bugs_on { "1" } else { "" }});
        let _ = std::fs::write(&mcp_file, json!({"mcpServers": {"meter": {"command": exe, "args": ["mcp-ask"], "env": env}}}).to_string());
        let allowed = format!("{}{}", if self.o.public_mode { "mcp__meter__ask_user mcp__meter__laptop_status mcp__meter__run_on_laptop" } else { "mcp__meter__ask_user" }, if bugs_on { " mcp__meter__bug_list mcp__meter__bug_fixed" } else { "" });
        let mut sys = format!("{}{}{}{}{}{}{}", ASK_PROMPT, self.plain_prompt(), if kind != "talk" { self.mem_prompt() } else { String::new() },
            if self.o.public_mode && kind != "talk" { format!(" {HUB_PROMPT}") } else { String::new() }, if bugs_on { format!(" {BUGS_PROMPT}") } else { String::new() },
            if kind == "talk" { format!(" {TALK_PROMPT}") } else { String::new() }, if body.t("brief") { format!(" {BRIEF}") } else { String::new() });
        args.extend(["--mcp-config".into(), mcp_file.to_string_lossy().to_string(), "--allowedTools".into(), allowed]);
        // a chat with root rights: only when the owner switched it on, and only through passwordless sudo (or when this program already is root)
        let was_root = sid_param != "new" && self.get_prefs(sid_param).get("root") == Some(&Value::Bool(true));
        let body_root = body.get("root") == Some(&Value::Bool(true));
        let mut as_root = !no_root && kind != "talk" && (body_root || was_root);
        let mut root_launch = false;
        if as_root && !body_root && (!self.root_allowed() || !self.root_ready(false).t("ok")) { as_root = false; sys.push_str(" This chat was started with root rights, but it now runs WITHOUT root rights. Say so if a command fails because of permissions."); }
        if as_root {
            if !self.root_allowed() { return Err((403, "Chats with root rights are off on this server. Switch them on in Settings → Server permissions first.".into())); }
            let rr = self.root_ready(false); if !rr.t("ok") { return Err((403, rr.s("why"))); }
            root_launch = !rr.t("uid0"); if root_launch { self.note_root_cwd(&cwd); }
            if mode == "bypassPermissions" { if let Some(i) = args.iter().position(|a| a == "--permission-mode") { args[i + 1] = "acceptEdits".into(); } }
            sys.push(' '); sys.push_str(ROOT_PROMPT);
        }
        args.extend(["--append-system-prompt".into(), sys]);
        if let Some(b) = self.budget_block(sid_param) { return Err(b); }
        if self.snapshots_on() && !cwd.is_empty() && self.ok_dir(&cwd) {
            let sn = crate::safety::take_snapshot(&self.dir, &cwd, "before a run", if sid_param == "new" { "" } else { sid_param }, &body.s("text"), 4000);
            if sn.t("skipped") { self.log(&format!("no snapshot: {}", sn.s("skipped"))); } else { self.log(&format!("snapshot {}", sn.s("id"))); }
        }
        let mut cmd = if root_launch {
            let mut c = tokio::process::Command::new("sudo");
            c.args(["-n", "env", &format!("HOME={}", home().display()), &format!("PATH={}", std::env::var("PATH").unwrap_or_default())]);
            for (k, v) in std::env::vars() { let ku = k.to_uppercase(); if ku.starts_with("ANTHROPIC_") || ku.starts_with("CLAUDE_") || ku.starts_with("HTTP_PROXY") || ku.starts_with("HTTPS_PROXY") || ku.starts_with("NO_PROXY") { c.arg(format!("{k}={v}")); } }
            c.arg(&claude).args(&args); c.current_dir(&cwd); c
        } else { claude_command(&claude, &args, &cwd) };
        cmd.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
        let mut child = match cmd.spawn() { Ok(c) => c, Err(e) => return Err((500, format!("Could not start Claude: {e}"))) };
        let job = Arc::new(Job { key: key.clone(), cwd: cwd.clone(), started: now_ms(), root_launch, bugs: bugs_on, resume_body: json!({"mode": body.g("mode"), "model": body.g("model")}), pid: Mutex::new(child.id()), st: Mutex::new(JobState {
            sid: if sid_param == "new" { String::new() } else { sid_param.into() }, want_kind: if kind == "talk" && sid_param == "new" { "talk".into() } else { String::new() },
            want_root: as_root, want_auto: as_root && bmode == "bypassPermissions" && self.bypass_ok(), ..Default::default() }) });
        self.jobs.lock().unwrap().insert(key.clone(), job.clone());
        crate::agent::kick(self, "job-start");
        // "call me when it's done" typed into the chat sets the call preference for this chat
        let text = body.s("text");
        if let Some(det) = detect_call(&text) {
            let sid = job.sid(); if !sid.is_empty() { self.set_prefs(&sid, json!({"call": det})); } else { job.st.lock().unwrap().want_call = det.into(); }
            job.emit(json!({"t": "prefs", "call": det}));
        }
        let mut stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let err_tail = Arc::new(Mutex::new(String::new()));
        if let Some(mut se) = stderr { let et = err_tail.clone(); tokio::spawn(async move { let mut b = [0u8; 4096]; while let Ok(n) = se.read(&mut b).await { if n == 0 { break; } let mut t = et.lock().unwrap(); t.push_str(&String::from_utf8_lossy(&b[..n])); let l = t.chars().count(); if l > 600 { *t = t.chars().skip(l - 600).collect(); } } }); }
        let me = self.clone(); let j = job.clone(); let text2 = text.clone(); let mcp2 = mcp_file.clone();
        tokio::spawn(async move {
            if let Some(mut si) = stdin.take() { let _ = si.write_all(text2.as_bytes()).await; let _ = si.shutdown().await; drop(si); }   // the prompt goes in on stdin, never on the command line
            if let Some(so) = stdout {
                let mut rd = tokio::io::BufReader::new(so);
                let mut line = Vec::new();
                loop {
                    line.clear();
                    match rd.read_until(b'\n', &mut line).await { Ok(0) | Err(_) => break, Ok(_) => {} }
                    let Ok(o) = serde_json::from_slice::<Value>(&line) else { continue };
                    me.on_stream_event(&j, &o, &text2);
                }
            }
            let code = child.wait().await.ok().and_then(|s| s.code());
            let has_done = j.st.lock().unwrap().events.iter().any(|x| x.s("t") == "done");
            if !has_done {   // Claude ended without an answer: say why instead of showing nothing
                let why = { let t = err_tail.lock().unwrap(); let w = t.split_whitespace().collect::<Vec<_>>().join(" "); let n = w.chars().count(); w.chars().skip(n.saturating_sub(300)).collect::<String>() };
                me.log(&format!("run ended without an answer (exit {}){}", code.map(|c| c.to_string()).unwrap_or("null".into()), if why.is_empty() { String::new() } else { format!(": {why}") }));
                me.finish_job(&j, Some(format!("Claude stopped without answering{}. Send it again.", if why.is_empty() { format!(" (exit {})", code.map(|c| c.to_string()).unwrap_or("null".into())) } else { format!(": {why}") })), &mcp2);
            } else { me.finish_job(&j, None, &mcp2); }
        });
        Ok(Started { job })
    }

    fn on_stream_event(self: &Arc<Self>, job: &Arc<Job>, o: &Value, text: &str) {
        let t = o.s("type");
        if t == "system" && o.s("subtype") == "init" {
            let sid = o.s("session_id");
            let need = !sid.is_empty() && job.sid().is_empty();
            if need {
                let (wc, wk, wr, wa) = { let mut g = job.st.lock().unwrap(); g.sid = sid.clone(); (g.want_call.clone(), g.want_kind.clone(), g.want_root, g.want_auto) };
                self.jobs.lock().unwrap().insert(sid.clone(), job.clone());
                self.note_run(&sid, &job.cwd, text);
                if !wc.is_empty() { self.set_prefs(&sid, json!({"call": wc})); }
                if !wk.is_empty() { self.set_prefs(&sid, json!({"kind": wk})); }
                if wr { self.set_prefs(&sid, json!({"root": true})); }
                if wa { self.set_prefs(&sid, json!({"auto": true})); }
            }
            job.emit(json!({"t": "session", "id": sid, "model": o.s("model")}));
        } else if t == "stream_event" {
            let dl = o.g("event").g("delta");
            if dl.s("type") == "text_delta" { let tx = dl.s("text"); job.st.lock().unwrap().text.push_str(&tx); job.emit(json!({"t": "delta", "text": tx})); }
        } else if t == "assistant" {
            for b in arr(o.g("message").g("content")) { if b.s("type") == "tool_use" { job.emit(json!({"t": "tool", "id": b.s("id"), "name": b.s("name"), "input": crate::core::tool_input(&b.s("name"), b.get("input")), "text": crate::core::tool_line(&b)})); } }
        } else if t == "user" {
            for b in arr(o.g("message").g("content")) { if b.s("type") == "tool_result" { job.emit(json!({"t": "result", "id": b.s("tool_use_id"), "text": crate::core::result_text(b.get("content")), "error": b.t("is_error")})); } }
        } else if t == "rate_limit_event" {
            self.note_rate_limit(o.g("rate_limit_info"));
            let ri = o.g("rate_limit_info");
            if ri.s("status") == "rejected" { let mut g = job.st.lock().unwrap(); g.limit_hit = true; if truthy(ri.g("resetsAt")) { g.limit_reset = vnum(ri.g("resetsAt")) * 1000.0; } }
        } else if t == "result" {
            let (err, txt) = {
                let mut g = job.st.lock().unwrap();
                g.error = o.t("is_error"); g.result_text = o.get("result").and_then(|r| r.as_str()).unwrap_or("").to_string();
                if g.error && auth_re().is_match(&g.result_text) {
                    g.auth_fail = true;
                    g.result_text = format!("Claude is logged out on {}: its login expired and could not be renewed. Open a terminal on that computer and run \"claude login\", then send your message again.", self.name());
                    let me = self.clone(); tokio::spawn(async move { tokio::time::sleep(std::time::Duration::from_secs(2)).await; me.keep_login(true).await; });
                }
                if g.error && limit_re().is_match(&g.result_text) {
                    g.limit_hit = true;
                    if let Some(c) = regex::Regex::new(r"\|(\d{9,13})\b").unwrap().captures(&g.result_text.clone()) { if g.limit_reset == 0.0 { let n: f64 = c[1].parse().unwrap_or(0.0); g.limit_reset = if n < 1e12 { n * 1000.0 } else { n }; } }
                }
                (g.error, g.result_text.clone())
            };
            let us = o.g("usage");
            let mut d = json!({"t": "done", "error": err, "ms": o.g("duration_ms"), "cost": o.g("total_cost_usd"), "usage": {"in": num(us.n("input_tokens")), "out": num(us.n("output_tokens")), "cr": num(us.n("cache_read_input_tokens")), "cw": num(us.n("cache_creation_input_tokens"))}});
            if err { set(&mut d, "text", json!(clip(&txt, 400))); }
            job.emit(d);
        }
    }

    pub fn finish_job(self: &Arc<Self>, job: &Arc<Job>, err: Option<String>, mcp_file: &std::path::Path) {
        if job.st.lock().unwrap().done { return; }
        if let Some(e) = &err { job.st.lock().unwrap().error = true; job.emit(json!({"t": "done", "error": true, "text": e})); }
        job.emit(json!({"t": "end"}));
        let (cbs, view) = {
            let mut g = job.st.lock().unwrap();
            g.done = true;
            g.listeners.clear();
            (std::mem::take(&mut g.on_done), JobView { sid: g.sid.clone(), key: job.key.clone(), error: g.error, result_text: g.result_text.clone(), limit_hit: g.limit_hit })
        };
        self.announce_done(job);
        for f in cbs { f(&view); }
        crate::shares::job_done(self, &job.id());
        if job.root_launch { self.owner_fix.lock().unwrap().1 = 0.0; self.fix_ownership("run ended"); }
        self.bump_chats(); crate::agent::kick(self, "job-done");
        let _ = std::fs::remove_file(mcp_file);
        let me = self.clone(); let j = job.clone();
        tokio::spawn(async move { tokio::time::sleep(std::time::Duration::from_secs(120)).await; me.jobs.lock().unwrap().retain(|_, v| !Arc::ptr_eq(v, &j)); });
    }

    /// The Stop button: end the run of this chat here, and withdraw an approval it was waiting for.
    pub fn stop_job(self: &Arc<Self>, sid: &str) -> bool {
        let Some(j) = (if sid.is_empty() { None } else { self.job(sid) }) else { return false };
        if j.is_done() { return false; }
        let jsid = j.sid();
        let reqs: Vec<(String, String)> = self.pending.lock().unwrap().iter().filter(|(_, p)| { let s = p.event.s("session"); s == sid || (!jsid.is_empty() && s == jsid) }).map(|(k, p)| (k.clone(), p.event.s("session"))).collect();
        for (req, s) in reqs { if self.resolve_pending(&req, Value::Null).is_some() { self.publish(json!({"kind": "resolved", "req": req, "session": s})); } }
        kill_tree(*j.pid.lock().unwrap(), j.root_launch);
        true
    }

    /// The milestone message: always sent; the phone rings only if you asked for a call.
    fn announce_done(self: &Arc<Self>, job: &Arc<Job>) {
        let (sid, limit, err, text, result) = { let g = job.st.lock().unwrap(); (g.sid.clone(), g.limit_hit, g.error, g.text.clone(), g.result_text.clone()) };
        if sid.is_empty() { return; }
        if limit { self.on_limit(job); return; }
        let prefs = self.get_prefs(&sid);
        let base = self.base_for(&sid, &job.cwd);
        let seconds = ((now_ms() - job.started) / 1000.0).round();
        let body = if text.trim().is_empty() { result } else { text.trim().to_string() };
        let tail: String = { let n = body.chars().count(); body.chars().skip(n.saturating_sub(220)).collect() };
        let txt = format!("{}{}", if err { "Something went wrong. " } else { "" }, tail);
        let call = prefs.s("call");
        self.publish(merge(&base, &json!({"kind": if err { "error" } else { "done" }, "call": if call != "off" && !err { true } else { call == "attention" }, "text": txt.trim(), "seconds": seconds})));
        if call == "done" { self.set_prefs(&sid, json!({"call": "off"})); }
    }

    fn limit_reset_at(&self, job: &Arc<Job>) -> f64 {
        let now = now_ms();
        let lr = job.st.lock().unwrap().limit_reset; if lr > 0.0 { return lr; }
        let u = self.usage.lock().unwrap().clone();
        let full: Vec<f64> = ["session", "week"].iter().map(|k| u.g(k).clone()).filter(|w| w.n("pct") >= 99.0 && w.n("resetsAt") > now).map(|w| w.n("resetsAt")).collect();
        if full.is_empty() { now + 10.0 * 60000.0 } else { full.into_iter().fold(0.0, f64::max) }
    }
    pub fn resume_after_limit(self: &Arc<Self>, sid: &str, body: Value, at: f64) {
        if let Some((_, h)) = self.limit_waits.lock().unwrap().remove(sid) { h.abort(); }
        let buf = std::env::var("HEARTH_LIMIT_BUFFER_MS").ok().and_then(|v| v.parse::<f64>().ok()).unwrap_or(20000.0);
        let wait = (if buf > 1000.0 { 5000.0f64 } else { 200.0 }).max(at - now_ms() + buf).min(2147000000.0);
        let me = self.clone(); let sid2 = sid.to_string();
        let h = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(wait as u64)).await;
            me.limit_waits.lock().unwrap().remove(&sid2);
            if me.job_busy(&sid2) { return; }
            let u = me.usage.lock().unwrap().clone();
            let w: Vec<f64> = [u.g("session").clone(), u.g("week").clone()].iter().filter(|x| x.n("pct") >= 99.0 && x.n("resetsAt") > now_ms()).map(|x| x.n("resetsAt")).collect();
            if !w.is_empty() { me.resume_after_limit(&sid2, body, w.into_iter().fold(0.0, f64::max)); return; }   // still full: wait for the next reset
            let b = merge(&body, &json!({"text": "Your usage limit has reset. Please continue where you left off."}));
            if let Err((_, m)) = me.start_job(&b, &sid2) { me.log(&format!("resume after limit: {m}")); }
        });
        self.limit_waits.lock().unwrap().insert(sid.to_string(), (at, h));
    }
    fn on_limit(self: &Arc<Self>, job: &Arc<Job>) {
        let sid = job.sid(); let base = self.base_for(&sid, &job.cwd); let at = self.limit_reset_at(job);
        let when = { use chrono::TimeZone; let d0 = chrono::Local.timestamp_millis_opt(at as i64).single().unwrap_or_else(chrono::Local::now); let today = chrono::Local::now().date_naive() == d0.date_naive(); format!("{}{}", d0.format("%H:%M"), if today { String::new() } else { format!(" {}", d0.format("%a")) }) };
        let body = job.resume_body.clone();
        {
            let mut wfs = self.workflows.lock().unwrap();
            if let Some(w) = wfs.get_mut(&sid) { if w.s("state") == "running" { set(w, "waitUntil", num(at + 20000.0)); drop(wfs); self.save_wf(); self.wf_event(&sid, &format!("Usage limit reached. The workflow carries on by itself when usage is back ({when}).")); return; } }
        }
        if self.read_cfg().get("autoContinue") == Some(&Value::Bool(true)) {
            self.resume_after_limit(&sid, body, at);
            self.publish(merge(&base, &json!({"kind": "limit", "auto": true, "resetsAt": num(at), "call": false, "text": format!("Hearth continues by itself when your usage is back ({when}).")})));
            return;
        }
        let req = rand_hex(4);
        let ev = merge(&base, &json!({"kind": "limit", "req": req, "resetsAt": num(at), "call": false, "text": format!("Continue when your usage is back ({when})?")}));
        let me = self.clone(); let sid2 = sid.clone();
        self.add_pending(&req, ev.clone(), Box::new(move |b: Value| { if b.as_str() == Some("continue") { me.resume_after_limit(&sid2, body, at); } }));
        self.publish(ev);
    }

    // ───────── Claude's login is renewed early, one run at a time ─────────
    pub async fn keep_login(self: &Arc<Self>, force: bool) {
        let cred = home().join(".claude").join(".credentials.json");
        let Some(o) = read_json(&cred) else { return };
        let exp = o.g("claudeAiOauth").n("expiresAt");
        { let mut k = self.keeping.lock().unwrap(); if *k { return; } if !force && exp - now_ms() > 40.0 * 60000.0 { return; } *k = true; }
        *self.last_check.lock().unwrap() = 0.0;
        let _ = self.check_usage().await;
        *self.keeping.lock().unwrap() = false;
    }

    // ───────── routines ─────────
    pub fn run_routine(self: &Arc<Self>, r: &Value, why: &str) -> Result<Value, String> {
        let rid = r.s("id");
        if self.jobs.lock().unwrap().values().any(|j| !j.is_done() && j.st.lock().unwrap().routine == rid) { return Err("It is still running.".into()); }
        let text = format!("{}\n\n(This is the routine \"{}\", started {}. Work on your own, do not ask questions, and finish with a short summary of what you found or did.)", r.s("goal"), r.s("name"), why);
        let mut b = json!({"text": text, "mode": r.g("mode"), "kind": "code"}); if !r.s("cwd").is_empty() { set(&mut b, "cwd", json!(r.s("cwd"))); }
        let res = self.start_job(&b, "new");
        let run_id = rand_hex(4);
        let run = json!({"id": run_id, "routine": rid, "name": r.s("name"), "at": now_i(), "why": why, "status": if res.is_err() { "failed" } else { "running" }, "summary": match &res { Err((_, m)) => if m.is_empty() { "Could not start".into() } else { m.clone() }, Ok(_) => String::new() }, "session": ""});
        let mut runs = self.read_runs(); runs.insert(0, run.clone()); self.save_runs(&runs);
        let mut all = self.read_routines(); if let Some(m) = all.iter_mut().find(|x| x.s("id") == rid) { set(m, "lastRun", json!(now_i())); self.save_routines(&all); }
        let started = res.map_err(|(_, m)| if m.is_empty() { "Could not start".to_string() } else { m })?;
        started.job.st.lock().unwrap().routine = rid;
        let me = self.clone(); let rid2 = run_id.clone();
        started.job.on_done(Box::new(move |j: &JobView| {
            let mut l = me.read_runs();
            if let Some(x) = l.iter_mut().find(|x| x.s("id") == rid2) {
                set(x, "status", json!(if j.error { "failed" } else { "done" })); set(x, "session", json!(j.sid));
                set(x, "summary", json!(clip(&j.result_text.split_whitespace().collect::<Vec<_>>().join(" "), 600))); set(x, "doneAt", json!(now_i()));
                me.save_runs(&l);
            }
        }));
        Ok(run)
    }
    pub fn clean_routine(&self, x: &Value, old: &Value) -> Result<Value, String> {
        let pick = |k: &str| if x.get(k).is_some() && !x.g(k).is_null() { vstr(x.g(k)) } else { old.s(k) };
        let name = clip(pick("name").trim(), 60); let goal = clip(pick("goal").trim(), 4000);
        if name.is_empty() || goal.is_empty() { return Err("A routine needs a name and a goal.".into()); }
        let sch = if x.g("schedule").is_object() { x.g("schedule").clone() } else if old.g("schedule").is_object() { old.g("schedule").clone() } else { json!({"type": "daily", "at": "09:00"}) };
        let min: f64 = if std::env::var("HEARTH_TEST").is_ok() { 0.005 } else { 15.0 };
        let schedule = if sch.s("type") == "interval" { let m = sch.n("minutes"); json!({"type": "interval", "minutes": num(min.max((7.0 * 1440.0f64).min(if m == 0.0 { 60.0 } else { m })))}) }
            else { let at = vstr(sch.g("at")); let ok = regex::Regex::new(r"^\d{1,2}:\d{2}$").unwrap().is_match(&at); json!({"type": "daily", "at": if ok { at } else { "09:00".into() }}) };
        let cwd = if x.get("cwd").is_some() { vstr(x.g("cwd")) } else if !old.s("cwd").is_empty() { old.s("cwd") } else { self.root.clone().unwrap_or_default() };
        if !cwd.is_empty() && !self.ok_dir(&cwd) { return Err("That folder is outside the projects folder.".into()); }
        let mode = if ["plan", "acceptEdits", "auto"].contains(&x.s("mode").as_str()) { x.s("mode") } else if !old.s("mode").is_empty() { old.s("mode") } else { "plan".into() };
        Ok(json!({"id": if old.s("id").is_empty() { rand_hex(4) } else { old.s("id") }, "name": name, "goal": goal, "cwd": cwd, "schedule": schedule, "mode": mode,
            "enabled": if x.get("enabled").is_some() { x.t("enabled") } else { old.get("enabled") != Some(&Value::Bool(false)) }, "lastRun": num(old.n("lastRun")), "createdAt": if old.n("createdAt") > 0.0 { num(old.n("createdAt")) } else { json!(now_i()) }}))
    }
    pub fn routine_due(r: &Value, now: f64) -> bool {
        if !r.t("enabled") { return false; }
        let s = r.g("schedule");
        if s.s("type") == "interval" { return now - r.n("lastRun") >= s.n("minutes") * 60000.0; }
        let at = s.s("at"); let mut hm = at.split(':').map(|x| x.parse::<u32>().unwrap_or(0));
        let (h, m) = (hm.next().unwrap_or(9), hm.next().unwrap_or(0));
        use chrono::TimeZone;
        let d = chrono::Local.timestamp_millis_opt(now as i64).single().unwrap_or_else(chrono::Local::now);
        let due = d.date_naive().and_hms_opt(h.min(23), m.min(59), 0).and_then(|x| chrono::Local.from_local_datetime(&x).earliest()).map(|x| x.timestamp_millis() as f64).unwrap_or(f64::MAX);
        now >= due && r.n("lastRun") < due
    }

    // ───────── workflows: a chat that keeps working through steps ─────────
    pub fn wf_min() -> f64 { std::env::var("HEARTH_WF_MINUTE_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(60000.0) }
    pub fn wf_view(&self, sid: &str) -> Value {
        let wfs = self.workflows.lock().unwrap();
        let Some(w) = wfs.get(sid) else { return json!({"state": "none", "steps": []}) };
        let idx = w.n("idx") as usize; let steps = arr(w.g("steps")); let step = steps.get(idx).cloned();
        let left = step.as_ref().filter(|s| s.n("minutes") > 0.0).map(|s| ((w.n("stepStart") + s.n("minutes") * Self::wf_min() - now_ms()) / Self::wf_min()).round().max(0.0));
        json!({"state": w.s("state"), "steps": steps, "idx": idx, "auto": w.t("auto"), "step": step.as_ref().map(|s| json!(s.s("name"))).unwrap_or(Value::Null),
            "minutesLeft": if w.s("state") == "running" { left.map(num).unwrap_or(Value::Null) } else { Value::Null }, "stepStart": w.g("stepStart"), "note": w.s("note"), "waitUntil": num(w.n("waitUntil"))})
    }
    pub fn wf_event(self: &Arc<Self>, sid: &str, text: &str) { let b = self.base_for(sid, ""); self.publish(merge(&b, &json!({"kind": "done", "call": false, "text": text, "seconds": 0}))); }
    fn wf_prompt(w: &Value, first: bool) -> String {
        let steps = arr(w.g("steps")); let idx = w.n("idx") as usize; let step = &steps[idx];
        let (n, total) = (idx + 1, steps.len());
        let left = if step.n("minutes") > 0.0 { ((w.n("stepStart") + step.n("minutes") * Self::wf_min() - now_ms()) / Self::wf_min()).round().max(1.0) } else { 0.0 };
        let head = format!("Workflow, step {n} of {total}: \"{}\".{}", step.s("name"), if first && n > 1 { format!(" The step before it (\"{}\") is done; build on what you found.", steps[n - 2].s("name")) } else { String::new() });
        let time = if step.n("minutes") > 0.0 { format!("{} Keep working the whole time: do not stop early, do not ask me questions unless you are really stuck, and when you run out of obvious things, look deeper.", if first { format!(" You have {} minutes for this step.", step.n("minutes")) } else { format!(" About {} minutes of this step are left.", left) }) } else { " Work until it is done.".into() };
        format!("{head}{time}\n\n{}{}", if first { "" } else { "Carry on where you left off. " }, step.s("prompt"))
    }
    fn wf_advance(self: &Arc<Self>, sid: &str) {
        let msg = {
            let mut wfs = self.workflows.lock().unwrap();
            let Some(w) = wfs.get_mut(sid) else { return };
            let n = arr(w.g("steps")).len();
            let idx = w.n("idx") as usize + 1;
            set(w, "stepStart", json!(now_i())); set(w, "ranOnce", json!(false)); set(w, "fails", json!(0));
            if idx >= n { set(w, "state", json!("done")); set(w, "idx", json!(n - 1)); format!("Workflow finished: all {n} step(s) done.") }
            else { set(w, "idx", json!(idx)); format!("Workflow: next step {} of {}, \"{}\".", idx + 1, n, arr(w.g("steps"))[idx].s("name")) }
        };
        self.wf_event(sid, &msg); self.save_wf();
    }
    pub fn wf_tick(self: &Arc<Self>, sid: &str) {
        let Some(w) = self.workflows.lock().unwrap().get(sid).cloned() else { return };
        if w.s("state") != "running" || self.job_busy(sid) { return; }
        if self.limit_waits.lock().unwrap().contains_key(sid) || now_ms() < w.n("waitUntil") { return; }
        let step = arr(w.g("steps"))[w.n("idx") as usize].clone();
        let over = if step.n("minutes") > 0.0 { now_ms() - w.n("stepStart") >= step.n("minutes") * Self::wf_min() } else { w.t("ranOnce") };
        if over { self.wf_advance(sid); if self.workflows.lock().unwrap().get(sid).map(|w| w.s("state") != "running").unwrap_or(true) { return; } }
        let w = self.workflows.lock().unwrap().get(sid).cloned().unwrap_or_default();
        let first = !w.t("ranOnce");
        let auto = if w.t("auto") && self.bypass_ok() { "bypassPermissions" } else { "acceptEdits" };
        let r = self.start_job(&json!({"text": Self::wf_prompt(&w, first), "mode": auto}), sid);
        match r {
            Err((_, m)) => {
                let paused = { let mut wfs = self.workflows.lock().unwrap(); let Some(w) = wfs.get_mut(sid) else { return }; let f = w.n("fails") + 1.0; set(w, "fails", num(f)); set(w, "note", json!(m)); if f >= 3.0 { set(w, "state", json!("paused")); true } else { false } };
                if paused { self.wf_event(sid, &format!("Workflow paused: {m}")); }
                self.save_wf();
            }
            Ok(s) => {
                { let mut wfs = self.workflows.lock().unwrap(); if let Some(w) = wfs.get_mut(sid) { set(w, "ranOnce", json!(true)); set(w, "note", json!("")); } }
                self.save_wf();
                let me = self.clone(); let sid2 = sid.to_string();
                s.job.on_done(Box::new(move |j: &JobView| {
                    let mut ev = None;
                    { let mut wfs = me.workflows.lock().unwrap(); if let Some(w) = wfs.get_mut(&sid2) {
                        if j.error && !j.limit_hit { let f = w.n("fails") + 1.0; set(w, "fails", num(f)); let note = clip(if j.result_text.is_empty() { "The run stopped with an error." } else { &j.result_text }, 200); set(w, "note", json!(note)); if f >= 3.0 { set(w, "state", json!("paused")); ev = Some(format!("Workflow paused after 3 failed runs: {note}")); } }
                        else if !j.error { set(w, "fails", json!(0)); }
                    } }
                    if let Some(e) = ev { me.wf_event(&sid2, &e); }
                    me.save_wf();
                    let me2 = me.clone(); tokio::spawn(async move { tokio::time::sleep(std::time::Duration::from_secs(3)).await; me2.wf_tick(&sid2); });
                }));
            }
        }
    }
    pub fn wf_set(self: &Arc<Self>, sid: &str, b: &Value) -> (u16, Value) {
        let action = b.s("action");
        if !action.is_empty() {
            let Some(w) = self.workflows.lock().unwrap().get(sid).cloned() else { return (404, json!({"error": "No workflow in this chat."})) };
            let state = w.s("state");
            let later = |me: &Arc<Self>, sid: &str| { let me = me.clone(); let s = sid.to_string(); tokio::spawn(async move { tokio::time::sleep(std::time::Duration::from_millis(200)).await; me.wf_tick(&s); }); };
            if action == "pause" && state == "running" { self.workflows.lock().unwrap().get_mut(sid).map(|w| set(w, "state", json!("paused"))); self.wf_event(sid, "Workflow paused."); }
            else if action == "resume" && state == "paused" { self.workflows.lock().unwrap().get_mut(sid).map(|w| { set(w, "state", json!("running")); set(w, "fails", json!(0)); set(w, "waitUntil", json!(0)); }); later(self, sid); }
            else if action == "stop" { self.workflows.lock().unwrap().remove(sid); if self.job_busy(sid) { self.stop_job(sid); } }
            else if action == "skip" && state != "done" { self.wf_advance(sid); if self.workflows.lock().unwrap().get(sid).map(|w| w.s("state") == "running").unwrap_or(false) { later(self, sid); } }
            else { return (409, json!({"error": format!("That does not fit the workflow state ({state}).")})); }
            self.save_wf(); return (200, self.wf_view(sid));
        }
        let steps: Vec<Value> = arr(b.g("steps")).iter().take(10).enumerate().map(|(i, x)| {
            let nm = clip(vstr(x.g("name")).trim(), 60);
            json!({"name": if nm.is_empty() { format!("Step {}", i + 1) } else { nm }, "prompt": clip(vstr(x.g("prompt")).trim(), 4000), "minutes": num(x.n("minutes").round().clamp(0.0, 1440.0))})
        }).filter(|x| !x.s("prompt").is_empty()).collect();
        if steps.is_empty() { return (400, json!({"error": "Add at least one step with something to do."})); }
        if crate::core::find_session(sid).is_none() { return (404, json!({"error": "Send a first message in the chat before starting a workflow."})); }
        if self.job_busy(sid) { return (409, json!({"error": "Claude is working on this chat right now. Start the workflow when it has finished, or stop it first."})); }
        let n = steps.len();
        self.workflows.lock().unwrap().insert(sid.into(), json!({"steps": steps, "idx": 0, "state": "running", "stepStart": now_i(), "ranOnce": false, "auto": b.get("auto") == Some(&Value::Bool(true)) && self.bypass_ok(), "fails": 0, "note": ""}));
        self.save_wf(); self.wf_event(sid, &format!("Workflow started: {n} step(s)."));
        let me = self.clone(); let s = sid.to_string(); tokio::spawn(async move { tokio::time::sleep(std::time::Duration::from_millis(200)).await; me.wf_tick(&s); });
        (200, merge(&self.wf_view(sid), &json!({"autoBlocked": b.get("auto") == Some(&Value::Bool(true)) && !self.bypass_ok()})))
    }
}

/// Ends a process (and force-ends it after 4 seconds if it does not go).
pub fn kill_tree(pid: Option<u32>, via_sudo: bool) {
    let Some(pid) = pid else { return };
    #[cfg(unix)]
    unsafe { libc::kill(pid as i32, libc::SIGTERM); }
    #[cfg(windows)]
    { let _ = std::process::Command::new("taskkill").args(["/pid", &pid.to_string(), "/T", "/F"]).output(); }
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(4));
        #[cfg(unix)]
        {
            if via_sudo { let _ = std::process::Command::new("sudo").args(["-n", "pkill", "-KILL", "-P", &pid.to_string()]).output(); }
            unsafe { if libc::kill(pid as i32, 0) == 0 { libc::kill(pid as i32, libc::SIGKILL); } }
        }
    });
}

/// Runs Claude once without a session or with a copy of one (side questions, API calls). Returns (text, inTok, outTok) or (code, error).
pub async fn run_once(claude: &str, args: Vec<String>, cwd: &str, stdin: String, timeout_ms: u64, too_long: &str) -> Result<Value, (u16, String)> {
    let mut cmd = claude_command(claude, &args, cwd);
    cmd.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).kill_on_drop(true);
    let mut child = cmd.spawn().map_err(|e| (500, format!("Could not start Claude: {e}")))?;
    if let Some(mut si) = child.stdin.take() { tokio::spawn(async move { let _ = si.write_all(stdin.as_bytes()).await; let _ = si.shutdown().await; }); }
    match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), child.wait_with_output()).await {
        Err(_) => Err((504, too_long.to_string())),
        Ok(Err(e)) => Err((500, format!("Could not start Claude: {e}"))),
        Ok(Ok(o)) => {
            let out = String::from_utf8_lossy(&o.stdout).to_string();
            match serde_json::from_str::<Value>(&out) {
                Ok(j) => Ok(j),
                Err(_) => Err((502, clip(&{ let e = String::from_utf8_lossy(&o.stderr).to_string(); if !e.is_empty() { e } else if !out.is_empty() { out } else { "Claude gave no answer.".into() } }, 300))),
            }
        }
    }
}
