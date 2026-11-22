//! The POST routes of the relay (owner only), the shared-chat routes, the guest routes and the API with Hearth keys.
use crate::relay::*;
use crate::routes::*;
use crate::util::*;
use axum::body::Body;
use axum::http::Response;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;

fn name_ok(n: &str) -> bool { regex::Regex::new(r"^[A-Za-z0-9][A-Za-z0-9._ -]{0,63}$").unwrap().is_match(n) && !n.contains("..") }
fn mv(from: &Path, to: &Path) -> std::io::Result<()> { if std::fs::rename(from, to).is_err() { copy_tree(from, to)?; std::fs::remove_dir_all(from)?; } Ok(()) }
fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)?.flatten() { let ft = e.file_type()?; let d = to.join(e.file_name()); if ft.is_dir() { copy_tree(&e.path(), &d)?; } else if ft.is_symlink() { if let Ok(t) = std::fs::read_link(e.path()) { #[cfg(unix)] { let _ = std::os::unix::fs::symlink(t, &d); } #[cfg(not(unix))] { let _ = t; } } } else { std::fs::copy(e.path(), &d)?; } }
    Ok(())
}
/// Replaces a project's old path with its new one inside the copied chat files.
fn rewrite_paths(d: &Path, from: &str, to: &str) {
    let Ok(rd) = std::fs::read_dir(d) else { return };
    for e in rd.flatten() {
        let p = e.path(); if p.is_dir() { rewrite_paths(&p, from, to); continue; }
        let n = e.file_name().to_string_lossy().to_string();
        if !(n.ends_with(".jsonl") || n.ends_with(".md") || n.ends_with(".json")) || e.metadata().map(|m| m.len() > 400_000_000).unwrap_or(true) { continue; }
        if let Ok(t) = std::fs::read_to_string(&p) { if t.contains(from) { let _ = std::fs::write(&p, t.split(from).collect::<Vec<_>>().join(to)); } }
    }
}
fn ndjson(lines: &[Value]) -> Resp { let mut s = String::new(); for l in lines { s.push_str(&l.to_string()); s.push('\n'); } Response::builder().status(200).header("Content-Type", "application/x-ndjson").body(Body::from(s)).unwrap() }

impl Relay {
    pub async fn post_route(self: &Arc<Self>, mut q: Req) -> Resp {
        let gz = q.gz; let j = |code: u16, v: Value| json_resp(code, &v, gz);
        let (p0, p1, p2) = (q.p(0).to_string(), q.p(1).to_string(), q.p(2).to_string()); let n = q.n();
        if p0 == "voice" { return self.proxy_voice_pub(q).await; }
        if p0 == "agent" && p1 == "files" && p2 == "put" {
            let Some(h) = &self.hub_files else { return j(404, json!({"error": "this is not the server"})) };
            let Ok(data) = q.bytes(260_000_000).await else { return j(413, json!({"error": "too large"})) };
            let body = if !q.h("x-gzip").is_empty() { match gunzip(&data) { Some(b) => b, None => return j(400, json!({"error": "bad body"})) } } else { data.to_vec() };
            let base = if q.q.iter().any(|(k, _)| k == "baseSize") { Some(Some((q.qs("baseSize").parse().unwrap_or(0.0), q.qs("baseMtime").parse().unwrap_or(0.0)))) } else if q.q.iter().any(|(k, _)| k == "baseNone") { Some(None) } else { None };
            let out = h.put(&q.qs("rel"), &body, q.qs("mtime").parse().unwrap_or_else(|_| now_ms()), q.qs("mode").parse().unwrap_or(0o644), q.qs("keepOld") == "1", &q.qs("who"), base);
            return j(if out.t("error") { 400 } else { 200 }, out);
        }
        if p0 == "agent" && p1 == "chats" && p2 == "put" {
            let Some(h) = &self.hub_sync else { return j(404, json!({"error": "this is not the server"})) };
            let Ok(data) = q.bytes(400_000_000).await else { return j(413, json!({"error": "too large"})) };
            let body = if !q.h("x-gzip").is_empty() { match gunzip(&data) { Some(b) => b, None => return j(400, json!({"error": "bad body"})) } } else { data.to_vec() };
            let meta = json!({"machineId": q.qs("machineId").chars().filter(|c| c.is_ascii_alphanumeric()).take(16).collect::<String>(), "name": clip(&q.qs("name"), 60), "id": q.qs("id"), "rel": if q.q.iter().any(|(k, _)| k == "rel") { json!(q.qs("rel")) } else { Value::Null }, "mtime": num(q.qs("mtime").parse().unwrap_or_else(|_| now_ms())), "offset": num(q.qs("offset").parse().unwrap_or(0.0)), "sha": q.qs("sha")});
            let me = self.clone(); let out = h.receive(&meta, &body, &move |id| me.is_busy(id));
            return j(if out.t("error") { 400 } else { 200 }, out);
        }
        if p0 == "projects" && p1 == "import" {
            let cwd = q.qs("cwd"); let from = q.qs("from");
            if cwd.is_empty() || from.is_empty() || self.root.as_ref().map(|r| !under(&cwd, r)).unwrap_or(false) { return j(400, json!({"error": "that folder is not inside the projects folder"})); }
            let Ok(data) = q.bytes(600_000_000).await else { return j(413, json!({"error": "too large"})) };
            let tmp = self.dir.join(format!("import-{}.tgz", rand_hex(4)));
            let _ = std::fs::create_dir_all(&self.dir); let _ = std::fs::write(&tmp, &data);
            let res = (|| {
                let list = crate::util::cmd("tar").args(["-tzf", &tmp.to_string_lossy()]).output().map_err(|_| (400, "invalid archive"))?;
                let t = String::from_utf8_lossy(&list.stdout).to_string();
                if !list.status.success() || t.lines().any(|e| e.starts_with('/') || e.split('/').any(|x| x == "..")) { return Err((400, "invalid archive")); }
                let dest = Self::claude_dir(&cwd); let _ = std::fs::create_dir_all(&dest);
                let st = crate::util::cmd("tar").args(["-xzf", &tmp.to_string_lossy(), "-C", &dest.to_string_lossy(), "--no-same-owner"]).status().map_err(|_| (500, "could not unpack"))?;
                if !st.success() { return Err((500, "could not unpack")); }
                rewrite_paths(&dest, &from, &cwd);
                Ok(std::fs::read_dir(&dest).map(|rd| rd.flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".jsonl")).count()).unwrap_or(0))
            })();
            let _ = std::fs::remove_file(&tmp);
            return match res { Ok(n) => j(200, json!({"ok": true, "sessions": n})), Err((c, e)) => j(c, json!({"error": e})) };
        }
        if p0 == "upload" {
            let mut cwd = q.qs("cwd"); let sid = q.qs("session");
            if !sid.is_empty() { cwd = crate::core::find_session(&sid).map(|f| crate::core::summary(&f).cwd).unwrap_or_default(); }
            if !is_dir(Path::new(&cwd)) { return j(400, json!({"error": "unknown project folder"})); }
            if !self.ok_dir(&cwd) { return j(403, json!({"error": "outside the projects folder"})); }
            let raw = basename(&{ let n = q.qs("name"); if n.is_empty() { "file".into() } else { n } });
            let safe = regex::Regex::new(r"[^A-Za-z0-9._-]+").unwrap().replace_all(&raw, "_").to_string();
            let tail: String = { let c = safe.chars().count(); safe.chars().skip(c.saturating_sub(80)).collect() };
            let name = if tail.is_empty() { "file".to_string() } else { tail };
            let Ok(data) = q.bytes(25_000_000).await else { return j(413, json!({"error": "file too large (25 MB max)"})) };
            let d = Path::new(&cwd).join(".uploads"); let _ = std::fs::create_dir_all(&d);
            let file = d.join(format!("{}-{}", now_i(), name)); let _ = std::fs::write(&file, &data);
            return j(200, json!({"path": file.to_string_lossy()}));
        }
        let b = q.json().await;
        match (p0.as_str(), p1.as_str()) {
            ("usage", "refresh") => return j(200, self.check_usage().await),
            ("chat-tags", "") if n == 1 => {
                let id = b.s("id"); if !regex::Regex::new(r"^[A-Za-z0-9_-]{1,80}$").unwrap().is_match(&id) { return j(400, json!({"error": "bad id"})); }
                let mut tags = if self.cfg("chatTags").is_object() { self.cfg("chatTags") } else { obj() };
                let mut cur = tags.g(&id).clone(); if !cur.is_object() { cur = obj(); }
                if b.get("pin").is_some() { if b.t("pin") { set(&mut cur, "pin", json!(true)); } else if let Some(m) = cur.as_object_mut() { m.remove("pin"); } }
                if b.get("name").is_some() { let nm = clip(vstr(b.g("name")).trim(), 120); if !nm.is_empty() { set(&mut cur, "name", json!(nm)); } else if let Some(m) = cur.as_object_mut() { m.remove("name"); } }
                if cur.t("pin") || !cur.s("name").is_empty() { set(&mut cur, "t", json!(now_i())); set(&mut tags, &id, cur); } else if let Some(m) = tags.as_object_mut() { m.remove(&id); }
                if tags.as_object().map(|m| m.len()).unwrap_or(0) > 2000 { return j(400, json!({"error": "too many"})); }
                self.write_cfg(json!({"chatTags": tags})); return j(200, json!({"tags": tags}));
            }
            ("routines", "") if n == 1 => {
                let mut list = self.read_routines(); let mut out = obj();
                if b.t("add") { match self.clean_routine(b.g("add"), &obj()) { Err(e) => return j(400, json!({"error": e})), Ok(r) => { set(&mut out, "added", r.g("id").clone()); list.insert(0, r); } } }
                if !b.g("update").s("id").is_empty() {
                    let id = b.g("update").s("id");
                    let Some(i) = list.iter().position(|x| x.s("id") == id) else { return j(404, json!({"error": "No such routine."})) };
                    match self.clean_routine(b.g("update"), &list[i].clone()) { Err(e) => return j(400, json!({"error": e})), Ok(r) => list[i] = r }
                }
                if b.t("remove") { let id = vstr(b.g("remove")); list.retain(|x| x.s("id") != id); }
                self.save_routines(&list);
                if b.t("run") {
                    let id = vstr(b.g("run")); let Some(r) = list.iter().find(|x| x.s("id") == id).cloned() else { return j(404, json!({"error": "No such routine."})) };
                    match self.run_routine(&r, "by you") { Err(e) => return j(409, json!({"error": e})), Ok(run) => set(&mut out, "run", run) }
                }
                return j(200, merge(&json!({"routines": self.read_routines()}), &out));
            }
            ("mcp", "install") => {
                let Some(t) = MCP_CATALOG.iter().find(|x| x.id == b.s("id")) else { return j(404, json!({"error": "Unknown tool."})) };
                let Some(cl) = find_claude() else { return j(500, json!({"error": "Claude Code was not found on this server."})) };
                let mut secret = String::new();
                if !t.needs.is_empty() { secret = b.s("secret").trim().to_string(); if !regex::Regex::new(r"^[\w.\-]{8,200}$").unwrap().is_match(&secret) { return j(400, json!({"error": format!("This tool needs {}.", t.needs)})); } }
                let h = self.root.clone().unwrap_or_else(|| home().to_string_lossy().to_string());
                let mut args = vec!["mcp".to_string(), "add".into(), "--scope".into(), "user".into()];
                if !t.needs.is_empty() { args.push("-e".into()); args.push(format!("{}={}", t.env, secret)); }
                args.extend([t.id.to_string(), "--".into(), "npx".into(), "-y".into(), t.pkg.into()]);
                if t.with_root { args.push(h.clone()); }
                let mut c = crate::jobs::claude_command(&cl, &args, &h);
                let r = tokio::time::timeout(std::time::Duration::from_secs(90), c.output()).await;
                return match r { Ok(Ok(o)) if o.status.success() => { self.log(&format!("tool added: {}", t.id)); j(200, json!({"ok": true, "text": format!("{} is added. New chats can use it.", t.name)})) }
                    Ok(Ok(o)) => j(500, json!({"error": clip(&format!("Could not add it: {}", { let e = String::from_utf8_lossy(&o.stderr).trim().to_string(); if e.is_empty() { String::from_utf8_lossy(&o.stdout).trim().to_string() } else { e } }), 300)})),
                    Ok(Err(e)) => j(500, json!({"error": clip(&format!("Could not add it: {e}"), 300)})), Err(_) => j(500, json!({"error": "Could not add it: timed out"})) };
            }
            ("memory", "") if n == 1 => {
                let mut list = self.mem_notes();
                if b.get("add").map(|x| !x.is_null()).unwrap_or(false) {
                    let text = clip(&vstr(b.g("add")).split_whitespace().collect::<Vec<_>>().join(" "), 300);
                    if text.is_empty() { return j(400, json!({"error": "Write something to remember."})); }
                    if list.len() >= 40 { return j(400, json!({"error": "That is 40 notes. Remove one first."})); }
                    if !list.iter().any(|x| x.s("text") == text) { list.push(json!({"id": rand_hex(4), "text": text, "at": now_i()})); }
                }
                if b.t("remove") { let id = vstr(b.g("remove")); list.retain(|x| x.s("id") != id); }
                self.write_cfg(json!({"memory": list})); return j(200, json!({"notes": list}));
            }
            ("templates", "") if n == 1 => {
                let mut list = arr(&self.cfg("templates"));
                if b.g("add").is_object() {
                    let name = clip(b.g("add").s("name").trim(), 40); let text = clip(b.g("add").s("text").trim(), 4000);
                    if name.is_empty() || text.is_empty() { return j(400, json!({"error": "A template needs a name and a text."})); }
                    list.retain(|x| x.s("name") != name); list.push(json!({"id": rand_hex(4), "name": name, "text": text}));
                    let k = list.len().saturating_sub(50); list = list.into_iter().skip(k).collect();
                }
                if b.t("remove") { let id = vstr(b.g("remove")); list.retain(|x| x.s("id") != id); }
                self.write_cfg(json!({"templates": list})); return j(200, json!({"templates": list}));
            }
            ("admin", "safety") => {
                for k in ["plain", "snapshots", "backups", "plans", "useApiKey"] { if b.get(k).is_some() { self.write_cfg(json!({ k: b.t(k) })); } }
                if let Some(Value::String(k)) = b.get("apiKey") {   // "" removes it
                    let k = k.trim();
                    if k.is_empty() { let _ = std::fs::remove_file(self.dir.join("anthropic-key")); self.write_cfg(json!({"useApiKey": false})); }
                    else if !k.starts_with("sk-ant-") || k.len() > 300 || k.chars().any(|c| c.is_whitespace()) { return j(400, json!({"error": "That is not an Anthropic API key (it starts with sk-ant-)."})); }
                    else { let _ = write_atomic_private(&self.dir.join("anthropic-key"), k.as_bytes()); if b.get("useApiKey").is_none() { self.write_cfg(json!({"useApiKey": true})); } }
                }
                if b.g("budget").is_object() { self.write_cfg(json!({"budget": {"perDay": num(b.g("budget").n("perDay").clamp(0.0, 100000.0)), "perChat": num(b.g("budget").n("perChat").clamp(0.0, 100000.0))}})); }
                let mut result = obj();
                if b.t("backupNow") { result = self.run_backup(); }
                if b.t("restoreBackup") { result = crate::safety::restore_backup(&self.dir, &crate::core::projects_dir(), &vstr(b.g("restoreBackup"))); }
                self.cost_cache.lock().unwrap().0 = 0.0;
                let bad = result.t("error");
                return j(if bad { 400 } else { 200 }, self.safety_view(result));
            }
            ("admin", "options") => {
                for (k, key) in [("outside", "outside"), ("bypass", "bypass"), ("root", "allowRoot"), ("autoContinue", "autoContinue")] { if b.get(k).is_some() { self.write_cfg(json!({ key: b.t(k) })); } }
                if b.get("googleTasks").is_some() { let g = b.g("googleTasks"); self.write_cfg(json!({"googleTasks": if !g.s("refreshToken").is_empty() { json!({"clientId": g.s("clientId"), "clientSecret": g.s("clientSecret"), "refreshToken": g.s("refreshToken")}) } else { Value::Null }})); }
                if b.t("recheck") { self.root_ready(true); }
                if b.t("removeRule") { let id = vstr(b.g("removeRule")); let l: Vec<Value> = self.allow_rules().into_iter().filter(|r| r.s("id") != id).collect(); self.write_cfg(json!({"allowRules": l})); }
                if b.t("removeDeny") { let id = vstr(b.g("removeDeny")); let l: Vec<Value> = self.deny_rules().into_iter().filter(|r| r.s("id") != id).collect(); self.write_cfg(json!({"denyRules": l})); }
                if b.t("addDeny") { self.add_deny(&vstr(b.g("addDeny"))); }
                if b.get("addPrefix").is_some() { match crate::perms::from_prefix(&vstr(b.g("addPrefix"))) { None => return j(400, json!({"error": "That cannot be allowed for good: use a plain command like \"npm test\" (no ; & | > or dangerous commands)."})), Some(r) => { self.add_rule(Some(r)); } } }
                return j(200, self.options_view(b.t("recheck"), false));
            }
            ("fs", "mkdir") => {
                let parent = b.s("path"); let name = b.s("name").trim().to_string();
                if name.is_empty() || name.chars().count() > 80 || name.contains(['\\', '/', '\0']) || name == "." || name == ".." { return j(400, json!({"error": "Give the folder a plain name (no slashes)."})); }
                let Some(p) = realpath(&parent).filter(|p| is_dir(Path::new(p))) else { return j(404, json!({"error": "no such folder"})) };
                if !self.ok_dir(&p) { return j(403, json!({"error": "outside the projects folder"})); }
                let target = Path::new(&p).join(&name);
                if target.exists() { return j(409, json!({"error": format!("there is already a folder called {name}")})); }
                return match std::fs::create_dir(&target) { Ok(()) => j(200, json!({"path": target.to_string_lossy()})), Err(e) => j(403, json!({"error": format!("cannot create it here: {}", if e.kind() == std::io::ErrorKind::PermissionDenied { "EACCES".into() } else { e.to_string() })})) };
            }
            ("admin", "memory") => {
                let p = b.n("pct"); let pv = vnum_opt(b.g("pct"));
                if !(pv == Some(0.0) || (pv.map(|x| x.is_finite()).unwrap_or(false) && (5.0..=95.0).contains(&p))) { return j(400, json!({"error": "pct must be 0 (no limit) or 5 to 95"})); }
                self.write_cfg(json!({"memoryPct": p.round() as i64}));
                let mut restarting = false;
                if b.t("restart") && self.o.self_update { if self.any_running() { return j(409, json!({"error": "Claude is working on this server right now. Try again when it has finished."})); } restarting = true; }
                if restarting { restart_soon(600); }
                return j(200, json!({"ok": true, "restarting": restarting}));
            }
            ("weekplan", _) => {
                let wd = b.n("day"); let wdv = vnum_opt(b.g("day"));
                if wdv.map(|x| x.fract() != 0.0 || !(0.0..=6.0).contains(&x)).unwrap_or(true) || b.g("day").is_null() { return j(400, json!({"error": "day must be 0 (Sunday) to 6 (Saturday)"})); }
                let wd = wd as i64; self.roll_week();
                if !b.g("pct").is_null() { let pl = self.week_plan(b.n("tz")); if arr(pl.g("days")).iter().any(|d| d.n("wd") as i64 == wd && !d.t("start") && d.s("state") == "past") { return j(400, json!({"error": "that day is already over"})); } }
                { let mut wt = self.week_targets.lock().unwrap();
                  if b.g("pct").is_null() { wt.1.remove(&wd.to_string()); }
                  else { let p = vnum_opt(b.g("pct")).unwrap_or(f64::NAN); if !p.is_finite() || !(0.0..=100.0).contains(&p) { return j(400, json!({"error": "pct must be 0 to 100"})); } wt.1.insert(wd.to_string(), num((p * 10.0).round() / 10.0)); }
                  write_json_private(&self.dir.join("weekplan.json"), &json!({"end": num(wt.0), "t": wt.1})); }
                return j(200, self.week_plan(b.n("tz")));
            }
            ("projects", "move") => {
                let Some(mr) = self.move_root.clone() else { return j(400, json!({"error": "this computer has no projects folder set up"})) };
                let src = resolve(&b.s("cwd")).to_string_lossy().to_string();
                let name = { let n = b.s("name"); if n.is_empty() { basename(&src) } else { n } }.trim().to_string();
                if !name_ok(&name) { return j(400, json!({"error": "use letters, numbers, spaces, dots, dashes and underscores"})); }
                if !is_dir(Path::new(&src)) { return j(400, json!({"error": "that folder doesn't exist"})); }
                if under(&src, &mr) { return j(200, json!({"path": src, "moved": false})); }
                let dest = Path::new(&mr).join(&name);
                if dest.exists() { return j(409, json!({"error": format!("there is already a project called {name} in the projects folder")})); }
                let _ = std::fs::create_dir_all(&mr);
                if let Err(e) = mv(Path::new(&src), &dest) { return j(500, json!({"error": e.to_string()})); }
                let old = Self::claude_dir(&src); let new = Self::claude_dir(&dest.to_string_lossy());
                if old.exists() {
                    let _ = copy_tree(&old, &new); rewrite_paths(&new, &src, &dest.to_string_lossy());
                    fn recent(d: &Path) -> bool { std::fs::read_dir(d).map(|rd| rd.flatten().any(|e| if e.path().is_dir() { recent(&e.path()) } else { e.metadata().map(|m| now_ms() - mtime_ms(&m) < 120000.0).unwrap_or(false) })).unwrap_or(false) }
                    if !recent(&old) { let _ = std::fs::remove_dir_all(&old); }   // a chat still running keeps its old copy
                }
                return j(200, json!({"path": dest.to_string_lossy(), "moved": true}));
            }
            ("away", _) => { self.write_cfg(json!({"away": b.t("on")})); return j(200, json!({"ok": true})); }
            _ => {}
        }
        // answers and decisions for questions that live on a linked laptop
        if (p0 == "answer" || p0 == "decision") && !self.pending.lock().unwrap().contains_key(&b.s("req")) {
            let rp = self.hub.lock().unwrap().remote_pending.remove(&b.s("req"));
            if let Some((name, session)) = rp {
                let v = if p0 == "answer" { json!(clip(&b.s("answer"), 2000)) } else { b.g("behavior").clone() };
                { let mut h = self.hub.lock().unwrap(); h.answers_for.entry(name).or_default().push(json!({"req": b.s("req"), "value": v})); }
                self.publish(json!({"kind": "resolved", "req": b.s("req"), "session": session})); crate::agent::wake(self);
                return j(200, json!({"ok": true}));
            }
        }
        if p0 == "answer" { let r = self.resolve_pending(&b.s("req"), json!(clip(&b.s("answer"), 2000))); if let Some(ev) = &r { self.publish(json!({"kind": "resolved", "req": b.s("req"), "session": ev.g("session")})); } return j(200, json!({"ok": r.is_some()})); }
        if p0 == "decision" && self.shares.has_request(&b.s("req")) { let (c, d) = self.shares.decide(self, &b.s("req"), &json!({"decision": if b.s("behavior") == "deny" { "deny" } else { "approve" }, "text": b.g("text"), "mode": b.g("mode"), "always": b.t("always"), "note": b.g("note")})); return j(c, d); }
        if p0 == "admin" && p1 == "apikeys" {
            if p2 == "revoke" { return j(if self.keys.revoke(&b.s("id")) { 200 } else { 404 }, json!({"ok": true})); }
            if p2 == "delete" { return j(if self.keys.remove(&b.s("id")) { 200 } else { 404 }, json!({"ok": true})); }
            return j(200, self.keys.create(&b));
        }
        if p0 == "mcp" && p1 == "build" {
            let target = clip(b.s("target").trim(), 200); if target.chars().count() < 2 { return j(400, json!({"error": "Tell me what it should connect to, like \"Inkscape\" or \"the Onshape API\"."})); }
            let slug = { let s = regex::Regex::new(r"[^a-z0-9]+").unwrap().replace_all(&target.to_lowercase(), "-").trim_matches('-').to_string(); let s = clip(&s, 40); if s.is_empty() { "server".to_string() } else { s } };
            let name = format!("mcp-{slug}");
            let dirp = Path::new(&self.root.clone().unwrap_or_else(|| home().join("projects").to_string_lossy().to_string())).join(&name);
            if let Err(e) = std::fs::create_dir_all(&dirp) { return j(500, json!({"error": format!("Could not make the folder: {e}")})); }
            for (f, text) in crate::mcpkit::FILES { let to = dirp.join(f); if !to.exists() { let _ = std::fs::write(&to, text); } }
            let d = dirp.to_string_lossy().to_string();
            return match self.start_job(&json!({"text": mcp_build_prompt(&target, &clip(&b.s("notes"), 1500), &name, &d), "cwd": d, "mode": "acceptEdits"}), "new") {
                Err((c, m)) => j(c, json!({"error": m})), Ok(_) => j(200, json!({"ok": true, "cwd": d, "name": name})) };
        }
        if p0 == "support" && p1 == "checklist" {
            let mut all = self.read_list();
            if let Some(sync) = b.g("sync").as_array() {
                for t in sync.iter().take(300) {
                    let id = clip(&t.s("id"), 20); if id.is_empty() { continue; }
                    if let Some(have) = all.iter_mut().find(|x| x.s("id") == id) {
                        if t.t("fixed") && !have.t("done") { set(have, "done", json!(true)); set(have, "doneAt", json!(now_i())); }
                        else if !t.t("fixed") && have.t("done") && have.get("reopen") != Some(&Value::Bool(false)) && t.n("updatedAt") > have.n("doneAt") + 1000.0 && t.s("from") != "support" { set(have, "done", json!(false)); if let Some(m) = have.as_object_mut() { m.remove("doneAt"); } }
                    } else if !t.t("fixed") {
                        let from = { let e = t.s("email"); if e.is_empty() { t.s("from") } else { e } }; let text = { let x = t.s("text"); if x.is_empty() { t.s("last") } else { x } };
                        all.insert(0, json!({"id": id, "title": clip(&t.s("title"), 120), "from": clip(&from, 120), "text": clip(&text, 5000), "at": if t.n("updatedAt") > 0.0 { num(t.n("updatedAt")) } else { json!(now_i()) }}));
                    }
                }
                all.truncate(300); self.save_list(&all);
            }
            let id = b.s("id");
            if !id.is_empty() && b.g("done").is_boolean() {
                if let Some(i) = all.iter().position(|x| x.s("id") == id) {
                    if b.t("done") {
                        set(&mut all[i], "done", json!(true)); set(&mut all[i], "doneAt", json!(now_i()));
                        if b.get("notify") == Some(&Value::Bool(true)) { self.support_notify(&id, "fixed", &clip(&b.s("note"), 500)); }
                        if let Some(t) = self.gt_open().await.into_iter().find(|t| t.s("title").contains(&format!("Support {id}"))) { self.gt_complete(&t.s("id")).await; }
                    } else { set(&mut all[i], "done", json!(false)); if let Some(m) = all[i].as_object_mut() { m.remove("doneAt"); } }
                    self.save_list(&all);
                }
            }
            if b.s("remove") == "done" { all.retain(|x| !x.t("done")); self.save_list(&all); }
            else if !b.s("remove").is_empty() { let r = b.s("remove"); all.retain(|x| x.s("id") != r); self.save_list(&all); }
            let mut open: Vec<Value> = all.iter().filter(|x| !x.t("done")).cloned().collect(); open.sort_by(|a, c| c.n("at").partial_cmp(&a.n("at")).unwrap_or(std::cmp::Ordering::Equal));
            let mut done: Vec<Value> = all.iter().filter(|x| x.t("done")).cloned().collect(); done.sort_by(|a, c| c.n("doneAt").partial_cmp(&a.n("doneAt")).unwrap_or(std::cmp::Ordering::Equal));
            return j(200, json!({"items": open.into_iter().chain(done).map(|x| json!({"id": x.g("id"), "title": x.g("title"), "from": x.g("from"), "text": x.g("text"), "at": x.g("at"), "done": x.t("done"), "doneAt": num(x.n("doneAt")), "note": x.s("note")})).collect::<Vec<_>>()}));
        }
        if p0 == "support" && p1 == "act" {
            let t = json!({"kind": "new", "id": clip(&b.s("id"), 20), "title": clip(&b.s("title"), 120), "category": clip(&{ let c = b.s("category"); if c.is_empty() { "other".into() } else { c } }, 20), "from": clip(&b.s("from"), 120), "text": clip(&b.s("text"), 5000)});
            if t.s("id").is_empty() || !["claude", "list", "ignore"].contains(&b.s("action").as_str()) { return j(400, json!({"error": "id and action (claude, list or ignore) are needed"})); }
            return j(200, self.support_act(&t, &b.s("action"), "").await);
        }
        if p0 == "support" && p1 == "incoming" { let (c, d) = self.support_incoming(&b); return j(c, d); }
        if p0 == "decision" { let r = self.resolve_pending(&b.s("req"), b.g("behavior").clone()); if let Some(ev) = &r { self.publish(json!({"kind": "resolved", "req": b.s("req"), "session": ev.g("session")})); } return j(200, json!({"ok": r.is_some()})); }
        if p0 == "stop" {
            let sid = b.s("session");
            if self.stop_job(&sid) { return j(200, json!({"ok": true})); }
            let mut sent = false;   // the run may be on one of your computers that the server handed the work to: pass the stop on
            if let Some(h) = &self.hub_sync { if !sid.is_empty() { let names: Vec<String> = h.agents.lock().unwrap().iter().filter(|(_, a)| arr(a.g("running")).iter().any(|x| vstr(x) == sid)).map(|(n, _)| n.clone()).collect(); for nme in names { self.hub.lock().unwrap().answers_for.entry(nme).or_default().push(json!({"stop": sid})); sent = true; } } }
            if sent { crate::agent::wake(self); }
            return j(200, json!({"ok": sent}));
        }
        if p0 == "projects" && n == 1 {
            let Some(root) = &self.root else { return j(400, json!({"error": "this computer does not manage a projects folder"})) };
            let name = b.s("name").trim().to_string();
            if !name_ok(&name) { return j(400, json!({"error": "use letters, numbers, spaces, dots, dashes and underscores"})); }
            let p = Path::new(root).join(&name); let _ = std::fs::create_dir_all(&p);
            return j(200, json!({"path": p.to_string_lossy(), "name": name}));
        }
        if p0 == "projects" && (p1 == "delete" || p1 == "restore") && n == 2 { return self.project_trash(&b, p1 == "restore" || b.t("restore"), gz); }
        if p0 == "pair" && p1 == "new" {
            if !self.o.public_mode { return j(400, json!({"error": "only a server pairs phones this way"})); }
            let mut hp = self.hub_pair.lock().unwrap(); hp.code = format!("{:06}", rand_below(1_000_000)); hp.expires = now_ms() + 10.0 * 60000.0; hp.fails = 0;
            return j(200, json!({"code": hp.code, "expiresIn": 600}));
        }
        if p0 == "pair" && p1 == "decision" {
            let mut g = self.pair_reqs.lock().unwrap();
            let Some(r) = g.get_mut(&b.s("id")).filter(|r| r.status == "pending") else { return j(404, json!({"error": "that request is gone"})) };
            r.status = if b.t("accept") { "accepted".into() } else { "declined".into() }; r.at = now_ms();
            if !b.t("accept") { self.asked_at.lock().unwrap().insert(r.ip.clone(), now_ms() + 57000.0); }   // a declined phone waits a minute before it can ask again
            return j(200, json!({"ok": true}));
        }
        if p0 == "agent" {
            match p1.as_str() {
                "usage" => {
                    let us = b.g("usage").clone(); let cur_at = self.usage.lock().unwrap().n("at");
                    if us.n("at") > 0.0 && (cur_at == 0.0 || us.n("at") > cur_at) { *self.usage.lock().unwrap() = us.clone(); self.note_week(&us); write_json_private(&self.dir.join("usage.json"), &us); }
                    return j(200, json!({"ok": true}));
                }
                "chats" if p2 == "sync" => {
                    let Some(h) = &self.hub_sync else { return j(404, json!({"error": "this is not the server"})) };
                    let name = clip(&{ let n = b.s("name"); if n.is_empty() { "computer".into() } else { n } }, 60);
                    let mid: String = b.s("machineId").chars().filter(|c| c.is_ascii_alphanumeric()).take(16).collect();
                    if mid.is_empty() || !b.g("manifest").is_array() { return j(400, json!({"error": "bad manifest"})); }
                    let uuid = |s: &str| s.len() == 36 && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
                    self.hub.lock().unwrap().agents.insert(name.clone(), now_ms());
                    h.note(&name, json!({"machineId": mid, "running": arr(b.g("running")).iter().map(vstr).filter(|x| uuid(x)).collect::<Vec<_>>(), "runningAt": now_i()}));
                    let manifest: Vec<Value> = arr(b.g("manifest")).into_iter().filter(|e| uuid(&e.s("id")) && e.g("mtime").is_number() && (e.g("rel").is_null() || e.g("rel").is_string())).take(5000).collect();
                    let me = self.clone();
                    return j(200, h.plan(&mid, &manifest, &move |id| me.is_busy(id)));
                }
                "chats" if p2 == "report" => { if let Some(h) = &self.hub_sync { h.note(&clip(&{ let n = b.s("name"); if n.is_empty() { "computer".into() } else { n } }, 60), json!({"last": merge(b.g("stat"), &json!({"received": now_i()}))})); } return j(200, json!({"ok": true})); }
                "files" if p2 == "delete" => { let Some(h) = &self.hub_files else { return j(404, json!({"error": "this is not the server"})) }; return j(200, h.remove(&b.s("rel"))); }
                "poll" => return j(200, crate::agent::poll(self, &b).await),
                "event" => {
                    let name = clip(&{ let n = b.s("name"); if n.is_empty() { "laptop".into() } else { n } }, 60);
                    let mut ev = if b.g("event").is_object() { b.g("event").clone() } else { obj() };
                    if ev.s("kind").is_empty() { return j(400, json!({"error": "no event"})); }
                    if let Some(m) = ev.as_object_mut() { m.remove("id"); m.remove("boot"); }
                    set(&mut ev, "remote", json!(true)); if ev.s("machine").is_empty() { set(&mut ev, "machine", json!(name)); }
                    { let mut h = self.hub.lock().unwrap(); h.agents.insert(name.clone(), now_ms());
                      if (ev.s("kind") == "permission" || ev.s("kind") == "question") && !ev.s("req").is_empty() { h.remote_pending.insert(ev.s("req"), (name.clone(), ev.s("session"))); }
                      if ev.s("kind") == "resolved" { h.remote_pending.remove(&ev.s("req")); } }
                    if ev.s("kind") == "done" && !ev.s("session").is_empty() && self.get_prefs(&ev.s("session")).s("call") == "done" { self.set_prefs(&ev.s("session"), json!({"call": "off"})); }
                    self.publish(ev); return j(200, json!({"ok": true}));
                }
                "ack" => {
                    { let mut h = self.hub.lock().unwrap(); let id = b.s("id"); h.work.retain(|w| w.s("id") != id); let wk = h.work.clone(); drop(h); self.save_work(&wk); }
                    if b.get("ok") == Some(&Value::Bool(false)) { let e = b.s("error"); let mach = { let n = b.s("name"); if n.is_empty() { "laptop".to_string() } else { n } }; let txt = clip(if e.is_empty() { "The task failed on your laptop." } else { &e }, 200); self.publish(json!({"kind": "error", "call": false, "session": b.s("session"), "title": "Your laptop", "cwd": "", "machine": mach, "ts": now_i(), "text": txt, "remote": true})); }
                    return j(200, json!({"ok": true}));
                }
                _ => {}
            }
        }
        if p0 == "admin" && p1 == "restart" {
            if !self.o.self_update { return j(400, json!({"error": "This one restarts together with its app."})); }
            if !b.t("force") && self.any_running() { return j(409, json!({"error": "Claude is working on this server right now.", "busy": true})); }
            restart_soon(600); return j(200, json!({"ok": true, "restarting": true}));
        }
        if p0 == "admin" && p1 == "update" {
            if !self.o.self_update { return j(400, json!({"error": "This one updates together with its app."})); }
            if self.any_running() { return j(409, json!({"error": "Claude is working on this server right now. Try again when it has finished."})); }
            let l = match crate::selfupdate::latest().await { Ok(l) => l, Err(e) => return j(500, json!({"error": e})) };
            if !crate::selfupdate::newer(&l.s("version"), crate::VERSION) { return j(200, json!({"ok": true, "from": crate::VERSION, "to": crate::VERSION, "restarting": false})); }
            return match crate::selfupdate::apply(&l.s("tag")).await { Ok(to) => { restart_soon(800); j(200, json!({"ok": true, "from": crate::VERSION, "to": to, "restarting": true})) } Err(e) => j(500, json!({"error": e})) };
        }
        if p0 == "sync" && p1 == "projects" && (p2 == "reject" || p2 == "allow" || p2 == "accept") {
            let fa = self.link.lock().unwrap().file_agent.clone();
            let Some(fa) = fa else { return j(400, json!({"error": "this computer is not linked to a server"})) };
            let names: Vec<String> = arr(b.g("names")).iter().map(vstr).collect();
            match p2.as_str() { "reject" => { fa.reject(&names, b.t("all")); self.link.lock().unwrap().pending_projects = fa.pending(); } "allow" => { fa.allow(self, &names).await; } _ => { fa.accept(self, &names, b.t("all")).await; } }
            return j(200, crate::agent::sync_overview(self).await);
        }
        if p0 == "sync" && p1 == "now" {
            let t0 = now_ms();
            if let Some(h) = &self.hub_sync { self.bump_chats(); for _ in 0..40 { tokio::time::sleep(std::time::Duration::from_millis(300)).await; let cs: Vec<Value> = h.agents.lock().unwrap().values().cloned().collect(); if cs.is_empty() || cs.iter().all(|a| a.g("last").n("received") >= t0 || now_ms() - a.n("seen") > 45000.0) { break; } } }
            else {
                let (asy, fag) = { let st = self.link.lock().unwrap(); (st.agent_sync.clone(), st.file_agent.clone()) };
                if let Some(a) = asy { a.sync_once(self, "manual").await; if let Some(f) = fag { f.sync_once(self, "manual").await; } }
            }
            return j(200, crate::agent::sync_overview(self).await);
        }
        if p0 == "sessions" && n >= 3 {
            let sid = p1.clone();
            match p2.as_str() {
                "btw" if n == 3 => {
                    let qx = clip(b.s("text").trim(), 2000); if qx.is_empty() { return j(400, json!({"error": "Write a question."})); }
                    let Some(f) = crate::core::find_session(&sid) else { return j(404, json!({"error": "no such session"})) };
                    let cwd = crate::core::summary(&f).cwd; if cwd.is_empty() || !self.ok_dir(&cwd) { return j(403, json!({"error": "that project is outside the projects folder"})); }
                    let Some(cl) = find_claude() else { return j(500, json!({"error": "Claude Code was not found on this server."})) };
                    let args: Vec<String> = ["-p", "--output-format", "json", "--resume", &sid, "--fork-session", "--no-session-persistence", "--model", "haiku", "--tools", "", "--strict-mcp-config"].iter().map(|s| s.to_string()).collect();
                    let r = crate::jobs::run_once(&cl, args, &cwd, format!("A side question, asked while you are busy with the task above. Answer it briefly from what you already know in this conversation. You cannot use tools or change anything, and you should not continue the task.\n\n{qx}"), 90000, "That took too long.").await;
                    return match r {
                        Ok(jv) if !jv.t("is_error") => j(200, json!({"text": jv.s("result").trim()})),
                        Ok(jv) => j(502, json!({"error": clip(&{ let r = jv.s("result"); if r.is_empty() { "No answer.".into() } else { r } }.split_whitespace().collect::<Vec<_>>().join(" "), 300)})),
                        Err((c, e)) => j(c, json!({"error": clip(&e.split_whitespace().collect::<Vec<_>>().join(" "), 300)})),
                    };
                }
                "undo" if n == 3 => {
                    let Some(f) = crate::core::find_session(&sid) else { return j(404, json!({"error": "no such session"})) };
                    let cwd = crate::core::summary(&f).cwd; if cwd.is_empty() || !self.ok_dir(&cwd) { return j(403, json!({"error": "that project is outside the projects folder"})); }
                    if self.job_busy(&sid) { return j(409, json!({"error": "Claude is still working on this chat. Stop it first."})); }
                    let snaps = crate::safety::list_snapshots(&self.dir, &cwd);
                    let pick = if b.t("snap") { snaps.iter().find(|x| x.s("id") == vstr(b.g("snap"))) } else { snaps.iter().find(|x| x.s("label") == "before a run") };
                    let Some(pick) = pick else { return j(404, json!({"error": "There is no snapshot of this project to go back to."})) };
                    let r = crate::safety::restore_snapshot(&self.dir, &cwd, &pick.s("id"));
                    self.log(&format!("undo of {cwd} to {}", iso(pick.n("at")))); self.bump_chats();
                    return j(if r.t("error") { 500 } else { 200 }, r);
                }
                "workflow" if n == 3 => { let (c, d) = self.wf_set(&sid, &b); return j(c, d); }
                "delete" | "restore" if n == 3 => return self.chat_trash(&sid, p2 == "restore", gz),
                "root" => {
                    let on = b.t("on");
                    if crate::core::find_session(&sid).is_none() { return j(404, json!({"error": "no such session"})); }
                    if on { if !self.root_allowed() { return j(403, json!({"error": "Chats with root rights are off on this server. Switch them on in Settings → Server permissions first."})); } let rr = self.root_ready(false); if !rr.t("ok") { return j(403, json!({"error": rr.s("why")})); } }
                    if self.job_busy(&sid) { return j(409, json!({"error": "Claude is working on this chat right now. Try again when it has finished."})); }
                    self.set_prefs(&sid, json!({"root": on})); return j(200, json!({"ok": true, "root": on}));
                }
                "auto" => {
                    let on = b.t("on");
                    if on && !self.bypass_ok() { return j(403, json!({"error": "Full auto is off on this server. Switch it on in Settings first."})); }
                    self.set_prefs(&sid, json!({"auto": on}));
                    if on { let reqs: Vec<String> = self.pending.lock().unwrap().iter().filter(|(_, p)| p.event.s("session") == sid && p.event.s("kind") == "permission").map(|(k, _)| k.clone()).collect(); for r in reqs { if self.resolve_pending(&r, json!("allow")).is_some() { self.publish(json!({"kind": "resolved", "req": r, "session": sid})); } } }
                    return j(200, json!({"ok": true, "auto": on}));
                }
                "prefs" => {
                    let mut patch = obj();
                    if ["off", "done", "attention"].contains(&b.s("call").as_str()) { set(&mut patch, "call", json!(b.s("call"))); }
                    if crate::agent::valid_run(b.g("run")) { set(&mut patch, "run", b.g("run").clone()); }
                    return j(200, self.set_prefs(&sid, patch));
                }
                "send" => return self.send(&sid, b, gz).await,
                _ => {}
            }
        }
        j(404, json!({"error": "not found"}))
    }

    pub async fn proxy_voice_pub(&self, q: Req) -> Resp { self.proxy_voice(q).await }
    fn run_backup(&self) -> Value { let r = crate::safety::create_backup(&self.dir, &crate::core::projects_dir()); if r.t("ok") { self.log(&format!("chat backup made: {}", r.s("name"))); } else { self.log(&format!("chat backup: {}", r.s("error"))); } r }
    pub fn nightly_backup(&self) {
        if !self.backups_on() { return; }
        let last = crate::safety::list_backups(&self.dir).into_iter().next();
        let h = { use chrono::Timelike; chrono::Local::now().hour() };
        if last.map(|l| now_ms() - l.n("at") > 23.0 * 3600e3).unwrap_or(true) && (2..=5).contains(&h) { self.run_backup(); }
    }

    fn project_trash(self: &Arc<Self>, b: &Value, restore: bool, gz: bool) -> Resp {
        let j = |code: u16, v: Value| json_resp(code, &v, gz);
        let Some(base) = self.root.clone().or_else(|| self.move_root.clone()) else { return j(400, json!({"error": "This computer has no projects folder set up."})) };
        let trash = Path::new(&base).join(".hearth-trash");
        if restore {
            let stem = b.s("stem"); if !regex::Regex::new(r"^[0-9]+__[A-Za-z0-9._ -]+$").unwrap().is_match(&stem) { return j(400, json!({"error": "unknown item"})); }
            let Some(meta) = read_json(&trash.join(&stem).join("meta.json")).filter(|m| !m.s("cwd").is_empty() && under(&m.s("cwd"), &base)) else { return j(404, json!({"error": "Nothing to restore: it was removed for good."})) };
            let cwd = meta.s("cwd");
            if Path::new(&cwd).exists() { return j(409, json!({"error": "A folder with that name exists again. Rename or remove it first."})); }
            let r = (|| -> std::io::Result<()> {
                std::fs::create_dir_all(dirname(&cwd))?; std::fs::rename(trash.join(&stem).join("files"), &cwd)?;
                if trash.join(&stem).join("chats").exists() { let cd = Self::claude_dir(&cwd); if !cd.exists() { std::fs::rename(trash.join(&stem).join("chats"), cd)?; } }
                std::fs::remove_dir_all(trash.join(&stem))
            })();
            if let Err(e) = r { return j(500, json!({"error": format!("Could not restore it: {e}")})); }
            self.bump_chats(); return j(200, json!({"ok": true, "cwd": cwd}));
        }
        let cwd = resolve(&b.s("cwd")).to_string_lossy().to_string();
        if b.s("cwd").is_empty() || !under(&cwd, &base) || cwd == resolve(&base).to_string_lossy() { return j(403, json!({"error": "Only a project inside the projects folder can be deleted."})); }
        let Ok(st) = std::fs::metadata(&cwd) else { return j(404, json!({"error": "That project is already gone."})) };
        if !st.is_dir() { return j(400, json!({"error": "That is not a folder."})); }
        if self.jobs.lock().unwrap().values().any(|jb| !jb.is_done() && !jb.cwd.is_empty() && under(&jb.cwd, &cwd)) { return j(409, json!({"error": "Claude is working in this project. Stop it first."})); }
        let cd = Self::claude_dir(&cwd);
        let ids: Vec<String> = std::fs::read_dir(&cd).map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().to_string()).filter(|f| f.ends_with(".jsonl")).map(|f| f[..f.len() - 6].to_string()).collect()).unwrap_or_default();
        if ids.iter().any(|id| self.shares.by_session(id).is_some()) { return j(409, json!({"error": "A chat of this project is shared. Stop sharing it first."})); }
        if ids.iter().any(|id| self.workflows.lock().unwrap().contains_key(id)) { return j(409, json!({"error": "A chat of this project has a workflow. Stop it first."})); }
        let stem = format!("{}__{}", now_i(), clip(&basename(&cwd).chars().map(|c| if c.is_ascii_alphanumeric() || "._ -".contains(c) { c } else { '_' }).collect::<String>(), 60));
        let dest = trash.join(&stem);
        let r = (|| -> std::io::Result<()> {
            std::fs::create_dir_all(&dest)?;
            if let Ok(rd) = std::fs::read_dir(&trash) { for e in rd.flatten() { if e.metadata().map(|m| now_ms() - mtime_ms(&m) > 30.0 * 86400000.0).unwrap_or(false) { let _ = std::fs::remove_dir_all(e.path()); } } }   // 30 days
            std::fs::write(dest.join("meta.json"), json!({"cwd": cwd, "name": basename(&cwd), "chats": ids.len(), "at": now_i()}).to_string())?;
            mv(Path::new(&cwd), &dest.join("files"))?; if !ids.is_empty() { mv(&cd, &dest.join("chats"))?; }
            Ok(())
        })();
        if let Err(e) = r { return j(500, json!({"error": format!("Could not delete it: {e}")})); }
        self.bump_chats(); j(200, json!({"ok": true, "stem": stem, "chats": ids.len()}))
    }
    fn chat_trash(self: &Arc<Self>, sid: &str, restore: bool, gz: bool) -> Resp {
        let j = |code: u16, v: Value| json_resp(code, &v, gz);
        if !restore { let mut st = self.read_stubs(); if st.remove(sid).is_some() { self.write_stubs(&st); if crate::core::find_session(sid).is_none() { return j(200, json!({"ok": true})); } } }   // an interrupted run that you delete must not come back
        let trash = crate::core::projects_dir().join(".hearth-trash");
        if restore {
            let hit = std::fs::read_dir(&trash).ok().and_then(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().to_string()).find(|n| n.ends_with(&format!("__{sid}.jsonl"))));
            let Some(hit) = hit else { return j(404, json!({"error": "Nothing to restore: it was removed for good."})) };
            let meta_f = trash.join(hit.replace(".jsonl", ".json"));
            let meta = read_json(&meta_f).unwrap_or_else(obj);
            let back = if !meta.s("dir").is_empty() && Path::new(&meta.s("dir")).exists() { meta.s("dir") } else if !meta.s("dir").is_empty() { dirname(&meta.s("dir")) } else { crate::core::projects_dir().join("restored").to_string_lossy().to_string() };
            let _ = std::fs::create_dir_all(&back); let _ = std::fs::rename(trash.join(&hit), Path::new(&back).join(format!("{sid}.jsonl"))); let _ = std::fs::remove_file(meta_f);
            let del: Vec<Value> = arr(&self.cfg("deleted")).into_iter().filter(|x| vstr(x) != sid).collect();
            self.write_cfg(json!({"deleted": del})); self.bump_chats(); return j(200, json!({"ok": true}));
        }
        let Some(f) = crate::core::find_session(sid) else { return j(404, json!({"error": "This chat is already gone."})) };
        if self.job_busy(sid) { return j(409, json!({"error": "Claude is working on this chat. Stop it first."})); }
        if self.shares.by_session(sid).is_some() { return j(409, json!({"error": "This chat is shared. Stop sharing it first."})); }
        if self.workflows.lock().unwrap().contains_key(sid) { return j(409, json!({"error": "This chat has a workflow. Stop it first."})); }
        let r = (|| -> std::io::Result<()> {
            std::fs::create_dir_all(&trash)?;
            if let Ok(rd) = std::fs::read_dir(&trash) { for e in rd.flatten() { if e.metadata().map(|m| now_ms() - mtime_ms(&m) > 30.0 * 86400000.0).unwrap_or(false) { let _ = std::fs::remove_file(e.path()); } } }   // the trash keeps a chat for 30 days
            let stem = format!("{}__{}", now_i(), sid);
            std::fs::write(trash.join(format!("{stem}.json")), json!({"dir": dirname(&f.file.to_string_lossy()), "title": crate::core::summary(&f).title}).to_string())?;
            if std::fs::rename(&f.file, trash.join(format!("{stem}.jsonl"))).is_err() { std::fs::copy(&f.file, trash.join(format!("{stem}.jsonl")))?; std::fs::remove_file(&f.file)?; }
            Ok(())
        })();
        if let Err(e) = r { return j(500, json!({"error": format!("Could not delete it: {e}")})); }
        let mut del: Vec<Value> = arr(&self.cfg("deleted")); if !del.iter().any(|x| vstr(x) == sid) { del.push(json!(sid)); }
        let k = del.len().saturating_sub(2000); let del: Vec<Value> = del.into_iter().skip(k).collect();
        self.write_cfg(json!({"deleted": del})); self.bump_chats();
        j(200, json!({"ok": true}))
    }

    /// A message for a chat (or the first one of a new chat).
    async fn send(self: &Arc<Self>, sidr: &str, mut b: Value, gz: bool) -> Resp {
        let j = |code: u16, v: Value| json_resp(code, &v, gz);
        let mode_of = |b: &Value| if MODES.contains(&b.s("mode").as_str()) { b.s("mode") } else { "auto".into() };
        if sidr != "new" { if let Some(away) = self.hub_sync.as_ref().and_then(|h| h.find_elsewhere_v(sidr)) {   // this chat's files only exist on one of your computers: hand the message to it
            let p = self.get_prefs(sidr);
            crate::agent::queue_work(self, json!({"id": rand_hex(6), "session": sidr, "cwd": away.s("cwd"), "target": away.s("machine"), "text": clip(&b.s("text"), 20000), "mode": mode_of(&b), "model": b.s("model"), "effort": b.s("effort"), "call": p.g("call"), "kind": p.g("kind"), "created": now_i()}));
            return ndjson(&[json!({"t": "queued", "to": away.s("machine"), "online": crate::agent::is_online(self, &away.s("machine"))}), json!({"t": "end"})]);
        } }
        let prefs0 = if sidr == "new" { json!({"run": "vps"}) } else { self.get_prefs(sidr) };
        let run = if crate::agent::valid_run(b.g("run")) { vstr(b.g("run")) } else { prefs0.s("run") };
        if self.o.public_mode && sidr == "new" && run.starts_with("pc:") {   // a new chat that should start on one of your computers
            let kind = if b.s("kind") == "talk" { "talk" } else { "code" }; let cwd = b.s("cwd");
            if kind != "talk" && (self.root.is_none() || !self.in_root(&cwd)) { return j(400, json!({"error": "A folder outside the projects folder only runs on the server. Pick “Server” to work there."})); }
            let target = crate::agent::target_for(self, &run);
            let root = self.root.clone().unwrap_or_default();
            let text = clip(&b.s("text").split(root.as_str()).collect::<Vec<_>>().join(crate::chathub::MARK), 20000);
            crate::agent::queue_work(self, json!({"id": rand_hex(6), "session": "", "newChat": true, "kind": kind, "target": target, "rel": if kind == "talk" { "Talks".into() } else { relative(&root, &cwd) }, "text": text, "mode": mode_of(&b), "model": b.s("model"), "effort": b.s("effort"), "call": "off", "created": now_i()}));
            return ndjson(&[json!({"t": "queued", "to": target, "online": target.as_deref().map(|t| crate::agent::is_online(self, t)).unwrap_or(false)}), json!({"t": "end"})]);
        }
        let elsewhere_here = sidr != "new" && self.root.as_ref().map(|root| crate::core::find_session(sidr).map(|f| { let c = crate::core::summary(&f).cwd; !c.is_empty() && !under(&c, root) }).unwrap_or(false)).unwrap_or(false);
        if self.o.public_mode && sidr != "new" && !elsewhere_here && (run == "laptop" || run.starts_with("pc:") || (run == "auto" && crate::agent::laptop_online(self).is_some())) {
            let Some(f) = crate::core::find_session(sidr) else { return j(404, json!({"error": "no such session"})) };
            let s0 = crate::core::summary(&f);
            if !self.visible_session(&s0.cwd, f.mtime) { return j(403, json!({"error": "that chat is outside the projects folder"})); }
            if self.job_busy(sidr) { return j(409, json!({"error": "Claude is still working on this chat"})); }
            let p = self.get_prefs(sidr); let target = crate::agent::target_for(self, &run);
            let mut w = json!({"id": rand_hex(6), "session": sidr, "rel": relative(self.root.as_deref().unwrap_or("/"), &s0.cwd), "text": b.s("text"), "mode": mode_of(&b), "model": b.s("model"), "effort": b.s("effort"), "call": p.g("call"), "chatMtime": num(f.mtime), "kind": p.g("kind"), "created": now_i()});
            if let Some(t) = &target { set(&mut w, "target", json!(t)); }
            crate::agent::queue_work(self, w);
            let online = target.as_deref().map(|t| crate::agent::is_online(self, t)).unwrap_or(false) || (!run.starts_with("pc:") && crate::agent::laptop_online(self).is_some());
            return ndjson(&[json!({"t": "queued", "to": target.unwrap_or_else(|| "your computer".into()), "online": online}), json!({"t": "end"})]);
        }
        if let Some(shx) = self.shares.by_session(sidr) { if b.t("text") { let t = format!("{}{}", self.shares.context_for(&shx.s("id")), b.s("text")); set(&mut b, "text", json!(t)); } }   // what others did with their own Claude since the last time
        match self.start_job(&b, sidr) { Err((c, m)) => j(c, json!({"error": m})), Ok(s) => self.attach(&s.job, 0) }
    }

    // ───────── shared chats (owner side) ─────────
    pub async fn shares_http(self: &Arc<Self>, mut q: Req) -> Resp {
        let gz = q.gz; let j = |code: u16, v: Value| json_resp(code, &v, gz);
        let (p0, p1, p2, p3) = (q.p(0).to_string(), q.p(1).to_string(), q.p(2).to_string(), q.p(3).to_string());
        if p0 == "rooms-sync" {   // owner only
            if let Some(r) = self.authed(&q) { return r; }
            if q.method == "POST" { let b = q.json().await; return j(200, json!({"items": self.merge_sync(b.g("items"))})); }
            return j(200, json!({"items": Value::Object(self.room_sync.lock().unwrap().clone())}));
        }
        if p0 == "shares" && p1 == "join" && q.method == "POST" {   // a friend uses an invite: no key yet
            if !self.o.public_mode && !self.reach(&q.ip) { return j(403, json!({"error": "local network only"})); }
            if let Some(f) = self.failures.lock().unwrap().get(&q.ip) { if f.1 > now_ms() { return j(429, json!({"error": "too many failed attempts, try again later"})); } }
            let b = q.json().await; let (c, d) = self.shares.join(&b);
            if c != 200 { let mut g = self.failures.lock().unwrap(); let n = g.get(&q.ip).map(|f| f.0).unwrap_or(0) + 1; g.insert(q.ip.clone(), (n, if n >= 8 { now_ms() + 15.0 * 60000.0 } else { 0.0 })); }
            return j(c, d);
        }
        if let Some(r) = self.authed(&q) { return r; }
        let m = q.method.clone(); let b = if m == "GET" { obj() } else { q.json().await };
        if p0 == "sessions" && p2 == "share" {
            let sid = p1.clone();
            if m == "GET" { return j(200, match self.shares.by_session(&sid) { Some(s) => merge(&json!({"shared": true}), &self.shares.view(&s)), None => json!({"shared": false}) }); }
            let Some(f) = crate::core::find_session(&sid) else { return j(404, json!({"error": "This chat is not on your server yet. Move it to your server first."})) };
            let sm = crate::core::summary(&f); if !self.visible_session(&sm.cwd, f.mtime) { return j(403, json!({"error": "This chat is outside the folders of this server."})); }
            let s = self.shares.ensure(&sid, &sm.cwd, &sm.title); return j(200, merge(&json!({"shared": true}), &self.shares.view(&s)));
        }
        let Some(sh) = (if p0 == "shares" { self.shares.get(&p1) } else { None }) else { return j(404, json!({"error": "No such shared chat."})) };
        let id = sh.s("id"); let host = json!({"id": "host", "name": self.name()});
        let (c, d) = match (p2.as_str(), m.as_str()) {
            ("invite", "POST") => (200, merge(&self.shares.invite(&id, &b.s("name")), &json!({"share": id}))),
            ("ask", "POST") => self.shares.ask(self, &id, &host, &vstr(b.g("to")), &b.s("text")),
            ("post", "POST") => self.shares.post(&id, &host, &b),
            ("working", "POST") => self.shares.working(&id, &host, b.t("on")),
            ("members", "POST") if !p3.is_empty() => self.shares.set_grant(self, &id, &p3, b.g("grant")),
            ("members", "DELETE") if !p3.is_empty() => self.shares.remove_member(self, &id, &p3),
            ("", "GET") => (200, merge(&json!({"shared": true}), &self.shares.view(&sh))),
            ("", "DELETE") => self.shares.unshare(self, &id),
            _ => (404, json!({"error": "not found"})),
        };
        j(c, d)
    }
    fn merge_sync(&self, inc: &Value) -> Value {
        let mut rs = self.room_sync.lock().unwrap(); let mut changed = false;
        let re = regex::Regex::new(r"^[A-Za-z0-9:._-]{1,120}$").unwrap();
        if let Some(m) = inc.as_object() { for (k, it) in m {
            if !re.is_match(k) || !it.is_object() || !it.g("t").is_number() { continue; }
            if let Some(cur) = rs.get(k) { if cur.n("t") >= it.n("t") { continue; } }
            let v = it.get("v").cloned().unwrap_or(Value::Null); if v.to_string().len() > 4000 { continue; }
            rs.insert(k.clone(), json!({"v": v, "t": it.g("t")})); changed = true;
        } }
        if changed {
            if rs.len() > 600 { let mut keys: Vec<(String, f64)> = rs.iter().map(|(k, v)| (k.clone(), v.n("t"))).collect(); keys.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap()); for (k, _) in keys.iter().take(rs.len() - 600) { rs.remove(k); } }
            write_json_private(&self.dir.join("roomsync.json"), &Value::Object(rs.clone()));
        }
        Value::Object(rs.clone())
    }

    /// What a guest key may do: read the one chat, and send prompts to it (which become requests).
    pub async fn guest_http(self: &Arc<Self>, mut q: Req, sh: Value, mem: Value) -> Resp {
        let gz = q.gz; let j = |code: u16, v: Value| json_resp(code, &v, gz);
        let sid = sh.s("sid"); let m = q.method.clone(); let (p0, p1, p2, p3) = (q.p(0).to_string(), q.p(1).to_string(), q.p(2).to_string(), q.p(3).to_string());
        let id = sh.s("id"); let me = json!({"id": mem.s("id"), "name": mem.s("name")});
        if m == "GET" && p0 == "sessions" && q.n() == 1 {
            let Some(f) = crate::core::find_session(&sid) else { return j(200, json!([])) };
            let sm = crate::core::summary(&f); let running = self.job_busy(&sid);
            return j(200, json!([{"id": sid, "title": if sm.title.is_empty() { sh.s("title") } else { sm.title }, "cwd": format!("/Shared by {}", self.name()), "mtime": f.mtime.floor() as i64, "live": false, "busy": running || (now_ms() - f.mtime) / 1000.0 < 20.0, "running": running, "kind": "code", "tokens": num(sm.tokens), "model": sm.model, "shared": 1}]));
        }
        if m == "GET" && p0 == "info" { return j(200, json!({"name": self.name(), "claude": true, "guest": true})); }
        if m == "GET" && p0 == "events" {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<bytes::Bytes>();
            let _ = tx.send(bytes::Bytes::from_static(b": hello\n\n"));
            for e in self.shares.incoming_events(&id, &mem.s("id")) { let b = self.base_for(&e.s("session"), &e.s("cwd")); let _ = tx.send(bytes::Bytes::from(format!("data: {}\n\n", merge(&merge(&b, &e), &json!({"replay": true}))))); }
            self.ev.lock().unwrap().guests.push((mem.s("id"), tx.clone()));
            tokio::spawn(async move { loop { tokio::time::sleep(std::time::Duration::from_secs(15)).await; if tx.send(bytes::Bytes::from_static(b": ping\n\n")).is_err() { break; } } });
            let s = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);
            use futures_util::StreamExt;
            return Response::builder().status(200).header("Content-Type", "text/event-stream").header("Cache-Control", "no-cache").header("X-Accel-Buffering", "no").body(Body::from_stream(s.map(Ok::<bytes::Bytes, std::convert::Infallible>))).unwrap();
        }
        if m == "GET" && (p0 == "pending" || p0 == "commands") { return j(200, json!([])); }
        if p0 == "stop" && m == "POST" {   // a guest may end only a run that was started for them
            let b = q.json().await; let s = b.s("session");
            let Some(jb) = self.job(&s).filter(|x| !x.is_done()) else { return j(200, json!({"ok": false})) };
            if s != sid || jb.st.lock().unwrap().by_member != mem.s("id") { return j(403, json!({"error": "You can only stop a prompt that you sent."})); }
            return j(200, json!({"ok": self.stop_job(&sid)}));
        }
        if p0 == "sessions" && p1 == sid {
            match (m.as_str(), p2.as_str()) {
                ("GET", "messages") => { if crate::core::find_session(&sid).is_none() { return j(404, json!({"error": "This chat is no longer on the server."})); } let s2 = sid.clone(); let l = tokio::task::spawn_blocking(move || crate::core::messages_sid(&s2, 400)).await.unwrap_or_default(); return j(200, self.shares.merge(&sh, arr(&l))); }
                ("GET", "state") => { let jb = self.job(&sid); let f = crate::core::find_session(&sid); let quiet = jb.as_ref().map(|x| x.is_done()).unwrap_or(false); let running = jb.as_ref().map(|x| !x.is_done()).unwrap_or(false); let busy = running || (!quiet && f.map(|f| (now_ms() - f.mtime) / 1000.0 < 20.0).unwrap_or(false)); return j(200, json!({"running": running, "busy": busy})); }
                ("GET", "prefs") => return j(200, json!({"call": "off", "run": "vps", "kind": "code", "shared": self.shares.guest_info(&sh, &mem)})),
                ("GET", "stream") => { let Some(jb) = self.job(&sid) else { return j(404, json!({"error": "not running"})) }; return self.attach(&jb, q.qs("from").parse::<f64>().unwrap_or(0.0).max(0.0) as usize); }
                ("POST", "post") => { let b = q.json().await; let (c, d) = self.shares.post(&id, &me, &b); return j(c, d); }
                ("POST", "working") => { let b = q.json().await; let (c, d) = self.shares.working(&id, &me, b.t("on")); return j(c, d); }
                ("POST", "ask") => { let b = q.json().await; let (c, d) = if vstr(b.g("to")) == "host" { self.shares.submit(self, &id, &mem, &b.s("text")) } else { self.shares.ask(self, &id, &me, &vstr(b.g("to")), &b.s("text")) }; return j(c, d); }
                ("POST", "requests") if !p3.is_empty() => { let b = q.json().await; let (c, d) = self.shares.member_decides(self, &id, &mem, &p3, &b); return j(c, d); }
                ("POST", "send") => {
                    let b = q.json().await; let (c, d) = self.shares.submit(self, &id, &mem, &b.s("text"));
                    if c != 200 { return j(c, d); }
                    return ndjson(&[json!({"t": "queued", "approval": !d.t("auto"), "auto": d.t("auto"), "to": self.name(), "online": true, "request": d.g("id")}), json!({"t": "end"})]);
                }
                _ => {}
            }
        }
        j(403, json!({"error": "This is not available in a shared chat."}))
    }

    /// An app of yours with a Hearth API key: POST /v1/messages and GET /v1/models, like the Anthropic API.
    pub async fn api_http(self: &Arc<Self>, mut q: Req) -> Resp {
        let err = |code: u16, msg: &str, t: Option<&str>| json_resp(code, &json!({"type": "error", "error": {"type": t.unwrap_or(match code { 401 => "authentication_error", 429 => "rate_limit_error", 400 => "invalid_request_error", _ => "api_error" }), "message": msg}}), false);
        if !self.o.public_mode && !self.reach(&q.ip) { return json_resp(403, &json!({"error": {"type": "forbidden", "message": "local network only"}}), false); }
        if let Some(f) = self.failures.lock().unwrap().get(&q.ip) { if f.1 > now_ms() { return err(429, "too many failed attempts, try again later", None); } }
        let given = { let k = q.h("x-api-key"); if !k.trim().is_empty() { k.trim().to_string() } else { q.h("authorization").trim_start_matches("Bearer ").trim().to_string() } };
        let Some(kid) = self.keys.authenticate(&given) else { self.note_fail(&q.ip); return err(401, "invalid API key", None) };
        self.failures.lock().unwrap().remove(&q.ip);
        let u = self.usage.lock().unwrap().clone();
        let week = u.g("week").get("pct").and_then(|x| x.as_f64()); let sess = u.g("session").get("pct").and_then(|x| x.as_f64());
        if let Err((c, why)) = self.keys.check(&kid, week, sess) { return err(c, &why, None); }
        let k = self.keys.get(&kid);
        let p1 = q.p(1).to_string();
        if p1 == "models" && q.method == "GET" {
            return json_resp(200, &json!({"data": arr(k.g("models")).iter().map(|id| json!({"id": id, "type": "model", "display_name": id})).collect::<Vec<_>>(), "efforts": k.g("efforts"), "per_day": if k.n("perDay") > 0.0 { k.g("perDay").clone() } else { Value::Null }, "week_stop_percent": if k.n("weekStopPct") > 0.0 { k.g("weekStopPct").clone() } else { Value::Null }, "session_stop_percent": if k.n("sessionStopPct") > 0.0 { k.g("sessionStopPct").clone() } else { Value::Null }, "memory": if k.s("session").is_empty() { "none".into() } else { k.s("session") }, "expires_at": if k.n("expiresAt") > 0.0 { json!(iso(k.n("expiresAt"))) } else { Value::Null }}), false);
        }
        if p1 == "messages" && q.method == "POST" {
            let b = q.json().await;
            if b.get("stream") == Some(&Value::Bool(true)) { return err(400, "streaming is not supported yet: send \"stream\": false", None); }
            let text_of = |c: &Value| -> String { match c { Value::String(s) => s.clone(), Value::Array(a) => a.iter().map(|x| if x.s("type") == "text" { x.s("text") } else if x.get("type").is_some() { "\u{0}".into() } else { String::new() }).collect(), _ => String::new() } };
            let msgs = arr(b.g("messages"));
            if msgs.is_empty() || msgs.len() > 200 { return err(400, "messages must be a list of 1 to 200 items like [{\"role\":\"user\",\"content\":\"Hello\"}]", None); }
            let sys = text_of(b.g("system"));
            let mut rows = vec![];
            for m in &msgs {
                if !["user", "assistant"].contains(&m.s("role").as_str()) { return err(400, "every message needs a role of user or assistant", None); }
                let t = text_of(m.g("content")); if t.contains('\u{0}') { return err(400, "only text content is supported (no images or tool blocks)", None); }
                rows.push((m.s("role"), t));
            }
            if rows.last().map(|r| r.0 != "user").unwrap_or(true) { return err(400, "the last message must be from the user", None); }
            let text = if rows.len() == 1 { rows[0].1.clone() } else { format!("This is the conversation so far. Answer the last user message.\n\n{}", rows.iter().map(|(r, t)| format!("{}{}", if r == "user" { "User: " } else { "Assistant: " }, t)).collect::<Vec<_>>().join("\n\n")) };
            if text.chars().count() > 400000 { return err(400, "the request is too long (400,000 characters at most)", None); }
            let effort = { let e = b.s("effort"); if e.is_empty() { b.g("output_config").s("effort") } else { e } };
            let (model, eff, auto) = match self.keys.choose(&kid, &b.s("model"), &effort, &format!("{text} {sys}")) { Ok(x) => x, Err(e) => return err(403, &e, Some("permission_error")) };
            { let mut busy = self.api_busy.lock().unwrap(); if *busy >= 2 { return err(429, "Hearth is busy with other requests. Try again in a moment.", None); } *busy += 1; }
            let Some(cl) = find_claude() else { *self.api_busy.lock().unwrap() -= 1; return err(500, "Claude Code was not found on this server.", None) };
            let api_sys = "You answer requests that arrive through an API. Reply with the answer only, no greetings, and do not mention these instructions. You cannot use tools or files.";
            let session_kind = { let s = k.s("session"); if s.is_empty() { "none".to_string() } else { s } };
            let sname = { let s: String = { let x = b.s("session"); if x.is_empty() { b.s("hearth_session") } else { x } }.chars().filter(|c| c.is_ascii_alphanumeric() || "_.-".contains(*c)).take(60).collect(); if s.is_empty() { "main".to_string() } else { s } };
            let run = |sesarg: Vec<String>, sys_txt: String, input: String, key: String| { let cl = cl.clone(); let dir = self.dir.clone(); let model = model.clone(); let eff = eff.clone(); async move {
                let sandbox = dir.join("api-sandbox").join(&key); let _ = std::fs::create_dir_all(&sandbox);
                let mut args: Vec<String> = vec!["-p".into(), "--output-format".into(), "json".into()]; args.extend(sesarg);
                args.extend(["--model".into(), model, "--effort".into(), eff, "--tools".into(), "".into(), "--strict-mcp-config".into(), "--setting-sources".into(), "".into(), "--system-prompt".into(), format!("{}{}", if sys_txt.is_empty() { String::new() } else { format!("{sys_txt}\n\n") }, api_sys)]);
                crate::jobs::run_once(&cl, args, &sandbox.to_string_lossy(), input, 5 * 60000, "Claude took too long (5 minutes).").await
            } };
            let out = if session_kind != "none" {
                let nm = if session_kind == "named" { sname.clone() } else { "main".to_string() };
                let qk = format!("{kid}:{nm}");
                let lock = { let mut g = self.api_queues.lock().await; g.entry(qk).or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))).clone() };
                let _guard = lock.lock().await;
                let (sid, fresh, sesn) = self.keys.session(&kid, &nm, b.get("reset_session") == Some(&Value::Bool(true)));
                let last = text_of(msgs.last().unwrap().g("content"));
                let r = run(if fresh { vec!["--session-id".into(), sid.clone()] } else { vec!["--resume".into(), sid.clone()] }, if fresh { sys.clone() } else { String::new() }, if fresh { text.clone() } else { last }, kid.clone()).await;
                match &r { Ok(jv) if !jv.t("is_error") => self.keys.session_used(&kid, &sesn), _ => if fresh { self.keys.session_forget(&kid, &sesn) } }
                r
            } else { run(vec!["--no-session-persistence".into()], sys.clone(), text.clone(), "_".into()).await };
            *self.api_busy.lock().unwrap() -= 1;
            let jv = match out { Ok(j) => j, Err((c, e)) => return err(c, &e, None) };
            if jv.t("is_error") { let lim = crate::jobs::limit_re().is_match(&jv.s("result")); return err(if lim { 429 } else { 502 }, &if lim { "The usage limit of the plan is reached. Try again when it resets.".to_string() } else { clip(&{ let r = jv.s("result"); if r.is_empty() { "Claude reported an error.".into() } else { r } }, 300) }, None); }
            let us = jv.g("usage"); let in_tok = us.n("input_tokens") + us.n("cache_read_input_tokens") + us.n("cache_creation_input_tokens"); let out_tok = us.n("output_tokens");
            self.keys.used(&kid, in_tok + out_tok);
            let ses_out = if session_kind == "none" { Value::Null } else if session_kind == "named" { { let x = b.s("session"); let x = if !x.is_empty() { x } else { let y = b.s("hearth_session"); if y.is_empty() { "main".to_string() } else { y } }; json!(x) } } else { json!("main") };
            return json_resp(200, &json!({"id": format!("msg_hk_{}", rand_hex(8)), "type": "message", "role": "assistant", "model": model, "content": [{"type": "text", "text": jv.s("result")}], "stop_reason": "end_turn", "stop_sequence": Value::Null, "usage": {"input_tokens": num(in_tok), "output_tokens": num(out_tok)}, "hearth": {"effort": eff, "picked_automatically": auto, "session": ses_out}}), false);
        }
        err(404, "unknown endpoint. Use POST /v1/messages or GET /v1/models.", None)
    }
}

/// The service manager (or the watchdog) starts the program again.
pub fn restart_soon(ms: u64) { std::thread::spawn(move || { std::thread::sleep(std::time::Duration::from_millis(ms)); std::process::exit(0); }); }
