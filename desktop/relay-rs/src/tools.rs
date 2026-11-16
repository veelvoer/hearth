//! Everyday Claude Code extras for the apps: git changes, saving and setting them aside, the project's notes (CLAUDE.md),
//! slash commands and skills, plugins, and the connections (MCP servers). Port of tools.js.
use crate::util::*;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

struct Out { ok: bool, out: String, err: String }
async fn run(cmd: &str, args: &[String], cwd: &str, timeout_ms: u64) -> Out {
    let mut c = crate::util::tcmd(cmd); c.args(args).current_dir(cwd).env("GIT_TERMINAL_PROMPT", "0").kill_on_drop(true).stdin(std::process::Stdio::null());
    match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), c.output()).await {
        Ok(Ok(o)) => Out { ok: o.status.success(), out: String::from_utf8_lossy(&o.stdout).to_string(), err: { let e = String::from_utf8_lossy(&o.stderr).to_string(); if e.is_empty() && !o.status.success() { format!("Command failed: {cmd}") } else { e } } },
        Ok(Err(e)) => Out { ok: false, out: String::new(), err: e.to_string() },
        Err(_) => Out { ok: false, out: String::new(), err: "timed out".into() },
    }
}
fn s(v: &[&str]) -> Vec<String> { v.iter().map(|x| x.to_string()).collect() }
static PL: Mutex<(f64, Option<Value>)> = Mutex::new((0.0, None));
async fn plugin_data(claude: &str, force: bool) -> Value {
    { let g = PL.lock().unwrap(); if !force { if let Some(d) = &g.1 { if now_ms() - g.0 < 5.0 * 60000.0 { return d.clone(); } } } }
    let r = run(claude, &s(&["plugin", "list", "--json", "--available"]), &home().to_string_lossy(), 90000).await;
    let d = serde_json::from_str::<Value>(&r.out).unwrap_or_else(|_| json!({"installed": [], "available": []}));
    *PL.lock().unwrap() = (now_ms(), Some(d.clone())); d
}
fn front(f: &Path) -> Option<String> { let t = std::fs::read_to_string(f).ok()?; let t = t.strip_prefix("---\n")?; let e = t.find("\n---")?; Some(t[..e].to_string()) }
fn fm(f: &Path) -> String {
    front(f).and_then(|m| m.lines().find_map(|l| l.strip_prefix("description:").map(|x| x.trim().to_string()))).map(|d| clip(d.trim_matches(|c| c == '"' || c == '\''), 160)).unwrap_or_default()
}
fn fm_name(f: &Path) -> String { front(f).and_then(|m| m.lines().find_map(|l| l.strip_prefix("name:").map(|x| x.trim().trim_matches(|c| c == '"' || c == '\'').to_string()))).unwrap_or_default() }
/// Skills sit in folders with a SKILL.md, sometimes a level or two deeper.
fn find_skills(dir: &Path, depth: i32, found: &mut Vec<(PathBuf, PathBuf, String)>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut es: Vec<_> = rd.flatten().collect(); es.sort_by_key(|e| e.file_name());
    for e in es {
        let n = e.file_name().to_string_lossy().to_string();
        if n.starts_with('.') || n == "node_modules" { continue; }
        let p = e.path(); if !p.is_dir() { continue; }
        if p.join("SKILL.md").exists() { found.push((p.clone(), p.join("SKILL.md"), n)); } else if depth > 0 { find_skills(&p, depth - 1, found); }
    }
}
fn scan(base: &Path, prefix: &str, out: &mut Vec<(String, Value)>, src: &str) {
    let mut put = |k: String, v: Value| { if let Some(x) = out.iter_mut().find(|x| x.0 == k) { x.1 = v; } else { out.push((k, v)); } };
    if let Ok(rd) = std::fs::read_dir(base.join("commands")) { let mut es: Vec<_> = rd.flatten().collect(); es.sort_by_key(|e| e.file_name()); for e in es { let n = e.file_name().to_string_lossy().to_string(); if let Some(stem) = n.strip_suffix(".md") { put(format!("{prefix}{stem}"), json!({"desc": fm(&e.path()), "src": src})); } } }
    let mut found = vec![]; find_skills(&base.join("skills"), 3, &mut found);
    for (dir, file, folder) in found {
        let nm = { let n = fm_name(&file); if n.is_empty() { folder.clone() } else { n } }.split_whitespace().collect::<Vec<_>>().join("-");
        put(format!("{prefix}{nm}"), json!({"desc": fm(&file), "src": src, "folder": folder, "own": dir.parent() == Some(&base.join("skills"))}));
    }
}
fn plugins_from_disk() -> Vec<Value> {
    let h = home();
    let Some(inst) = read_json(&h.join(".claude/plugins/installed_plugins.json")) else { return vec![] };
    let on = read_json(&h.join(".claude/settings.json")).map(|s| s.g("enabledPlugins").clone()).unwrap_or_else(obj);
    inst.g("plugins").as_object().map(|m| m.iter().filter_map(|(id, v)| { let ip = v.get(0).map(|x| x.s("installPath")).unwrap_or_default(); if ip.is_empty() { None } else { Some(json!({"id": id, "enabled": on.get(id) != Some(&Value::Bool(false)), "installPath": ip})) } }).collect()).unwrap_or_default()
}
/// Everything you can start with a slash.
pub async fn commands(cwd: &str, claude: Option<&str>) -> Value {
    let mut out = vec![];
    let h = home();
    scan(&h.join(".claude"), "", &mut out, "yours"); scan(&h.join(".agents"), "", &mut out, "yours");
    if !cwd.is_empty() { scan(&Path::new(cwd).join(".claude"), "", &mut out, "project"); }
    let mut inst = vec![]; if let Some(c) = claude { inst = arr(plugin_data(c, false).await.g("installed")); } if inst.is_empty() { inst = plugins_from_disk(); }
    for p in inst { if p.t("enabled") && !p.s("installPath").is_empty() { let id = p.s("id"); let base = id.split('@').next().unwrap_or("").to_string(); scan(Path::new(&p.s("installPath")), &format!("{base}:"), &mut out, &format!("plugin {base}")); } }
    if let Ok(rd) = std::fs::read_dir(h.join(".claude/plugins/synced")) { for d in rd.flatten() { scan(&d.path(), "", &mut out, "your Claude account"); } }
    let mut list: Vec<Value> = [("compact", "Shrink this chat so it stays fast and cheap"), ("init", "Let Claude write notes about this project (CLAUDE.md)"), ("review", "Review the latest changes"), ("security-review", "Check the changes for security problems")].iter().map(|(n, d)| json!({"name": n, "desc": d, "src": "built in", "builtin": true})).collect();
    for (name, v) in out { if !list.iter().any(|x| x.s("name") == name) { list.push(json!({"name": name, "desc": v.s("desc"), "src": v.s("src"), "own": v.t("own") && v.s("src") == "yours"})); } }
    Value::Array(list)
}
fn safe_name(n: &str) -> bool { regex::Regex::new(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$").unwrap().is_match(n) }
fn skills_dir() -> PathBuf { home().join(".claude").join("skills") }
fn dir_size(d: &Path, cap: u64) -> u64 { let mut t = 0; fn w(x: &Path, t: &mut u64, cap: u64) { let Ok(rd) = std::fs::read_dir(x) else { return }; for e in rd.flatten() { if *t > cap { return; } let Ok(ft) = e.file_type() else { continue }; if ft.is_symlink() { continue; } if ft.is_dir() { w(&e.path(), t, cap); } else { *t += e.metadata().map(|m| m.len()).unwrap_or(0); } } } w(d, &mut t, cap); t }
fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)?.flatten() { let n = e.file_name(); if n == ".git" { continue; } let ft = e.file_type()?; if ft.is_dir() { copy_dir(&e.path(), &to.join(&n))?; } else if ft.is_file() { std::fs::copy(e.path(), to.join(&n))?; } }
    Ok(())
}
fn put_skill(src: &Path, name: &str) -> Result<String, String> {
    if !safe_name(name) { return Err(format!("That skill has a name I cannot use: {name}")); }
    if dir_size(src, 6_000_000) > 5_000_000 { return Err(format!("\"{name}\" is bigger than 5 MB.")); }
    let dest = skills_dir().join(name); let _ = std::fs::create_dir_all(skills_dir()); let _ = std::fs::remove_dir_all(&dest);
    copy_dir(src, &dest).map_err(|e| e.to_string())?; Ok(name.to_string())
}
async fn install_skills(source: &str) -> Result<Vec<String>, String> {
    let src = source.trim(); if src.is_empty() { return Err("Paste a GitHub link or the text of a skill.".into()); }
    let tmp = std::env::temp_dir().join(format!("hearth-skill-{}", rand_hex(4))); let _ = std::fs::create_dir_all(&tmp);
    let res = async {
        if src.starts_with("---") && src[3..].trim_start_matches([' ', '\t']).starts_with('\n') {
            let head = src.split("\n---").next().unwrap_or("");
            let name = head.lines().find_map(|l| l.strip_prefix("name:")).map(|x| x.trim().trim_matches(|c| c == '"' || c == '\'').split_whitespace().collect::<Vec<_>>().join("-")).unwrap_or_default();
            if name.is_empty() { return Err("The text needs a \"name:\" line at the top.".to_string()); }
            let _ = std::fs::create_dir_all(tmp.join(&name)); let _ = std::fs::write(tmp.join(&name).join("SKILL.md"), src);
            return Ok(vec![put_skill(&tmp.join(&name), &name)?]);
        }
        let one = regex::Regex::new(r"(?i)^https?://(?:raw\.githubusercontent\.com|github\.com)/\S+/(?:raw/|blob/)?\S*SKILL\.md$").unwrap().is_match(src) || regex::Regex::new(r"(?i)^https://raw\.githubusercontent\.com/\S+\.md$").unwrap().is_match(src);
        if one {
            let url = src.replacen("github.com/", "raw.githubusercontent.com/", 1).replacen("/blob/", "/", 1).replacen("/raw/", "/", 1);
            let r = reqwest::Client::new().get(&url).timeout(std::time::Duration::from_secs(20)).send().await.map_err(|e| e.to_string())?;
            if !r.status().is_success() { return Err(format!("Could not download it ({}).", r.status().as_u16())); }
            let text = r.text().await.map_err(|e| e.to_string())?; if text.len() > 1_000_000 { return Err("That file is too big.".into()); }
            let parts: Vec<&str> = url.split('/').collect(); let folder = parts.get(parts.len().saturating_sub(2)).copied().unwrap_or("skill");
            let nm = text.split("\n---").next().unwrap_or("").lines().find_map(|l| l.strip_prefix("name:")).map(|x| x.trim().to_string()).unwrap_or_else(|| folder.to_string());
            let name = nm.trim_matches(|c| c == '"' || c == '\'').split_whitespace().collect::<Vec<_>>().join("-");
            let _ = std::fs::create_dir_all(tmp.join(&name)); let _ = std::fs::write(tmp.join(&name).join("SKILL.md"), &text);
            return Ok(vec![put_skill(&tmp.join(&name), &name)?]);
        }
        let re = regex::Regex::new(r"^(?:https?://github\.com/)?([A-Za-z0-9_.-]+)/([A-Za-z0-9_.-]+?)(?:\.git)?(?:/(?:tree/[^/]+/)?(.*))?$").unwrap();
        let src2 = src.trim_end_matches('/');
        let Some(m) = re.captures(src2).filter(|_| !src.contains(char::is_whitespace)) else { return Err("Use a GitHub link like github.com/owner/repo, or owner/repo/skill-folder.".into()) };
        let (owner, repo, sub) = (m[1].to_string(), m[2].to_string(), m.get(3).map(|x| x.as_str().to_string()).unwrap_or_default());
        let repo_dir = tmp.join("repo");
        let g = run("git", &s(&["clone", "--depth", "1", &format!("https://github.com/{owner}/{repo}.git"), &repo_dir.to_string_lossy()]), &tmp.to_string_lossy(), 90000).await;
        if !g.ok { return Err(format!("Could not download {owner}/{repo}. Is the link right and the repository public?")); }
        let base = if sub.is_empty() { repo_dir.clone() } else { resolve(&repo_dir.join(&sub).to_string_lossy()) };
        if base != repo_dir && !base.starts_with(&repo_dir) { return Err("That folder is not inside the repository.".into()); }
        let mut found = vec![];
        if base.join("SKILL.md").exists() { found.push((base.clone(), base.join("SKILL.md"), basename(&base.to_string_lossy()))); } else { find_skills(&base, 4, &mut found); }
        if found.is_empty() { return Err("No skill (a folder with a SKILL.md) found there.".into()); }
        if found.len() > 25 { return Err(format!("That has {} skills. Point to one folder instead.", found.len())); }
        let mut names = vec![];
        for (d, f, folder) in found { let nm = if safe_name(&folder) { folder } else { { let n = fm_name(&f); if n.is_empty() { "skill".into() } else { n } }.chars().map(|c| if c.is_ascii_alphanumeric() || "._-".contains(c) { c } else { '-' }).collect() }; names.push(put_skill(&d, &nm)?); }
        Ok(names)
    }.await;
    let _ = std::fs::remove_dir_all(&tmp);
    res
}
fn fail(code: u16, e: &str) -> (u16, Value) { (code, json!({"error": e})) }
fn last2(t: &str) -> String { let l: Vec<&str> = t.trim().lines().collect(); l[l.len().saturating_sub(2)..].join(" ") }

/// /tools/... routes. ok_dir checks a path against the projects folder.
pub async fn route(parts: &[String], q: &[(String, String)], body: &Value, ok_dir: &(dyn Fn(&str) -> bool + Send + Sync), claude: Option<String>) -> (u16, Value) {
    let get = |k: &str| qget(q, k).unwrap_or_else(|| vstr(body.g(k)));
    let cwd = get("cwd");
    let need_dir = || !cwd.is_empty() && Path::new(&cwd).is_absolute() && ok_dir(&cwd) && is_dir(Path::new(&cwd));
    let a = parts.first().map(|x| x.as_str()).unwrap_or(""); let b = parts.get(1).map(|x| x.as_str()).unwrap_or("");
    if a == "git" {
        if !need_dir() { return fail(400, "This project folder is not on this computer."); }
        let git = |args: &[&str]| { let a = s(args); let c = cwd.clone(); async move { run("git", &a, &c, 20000).await } };
        let top = git(&["rev-parse", "--show-toplevel"]).await;
        if !top.ok { return (200, json!({"repo": false})); }
        match b {
            "status" => {
                let st = git(&["status", "--porcelain=v1", "-b"]).await;
                let lines: Vec<&str> = st.out.lines().filter(|l| !l.is_empty()).collect();
                let branch = lines.first().map(|l| l.trim_start_matches("## ").split("...").next().unwrap_or("").to_string()).unwrap_or_default();
                let files: Vec<Value> = lines.iter().skip(1).map(|l| { let c = l.chars().nth(0).unwrap_or(' '); let c2 = l.chars().nth(1).unwrap_or(' '); let k = if c == ' ' { c2 } else { c }; json!({"state": match k { 'M' => "changed", 'A' => "new", 'D' => "deleted", 'R' => "renamed", '?' => "new", _ => "changed" }, "path": l.get(3..).unwrap_or("")}) }).collect();
                let last = git(&["log", "-1", "--format=%s"]).await.out.trim().to_string();
                return (200, json!({"repo": true, "branch": branch, "files": files, "last": last}));
            }
            "diff" => {
                let f = get("file");
                let d = if f.is_empty() { git(&["diff", "HEAD"]).await } else { git(&["diff", "HEAD", "--", &f]).await };
                let mut text = d.out;
                if text.is_empty() && !f.is_empty() { if let Ok(t) = std::fs::read_to_string(Path::new(top.out.trim()).join(&f)) { text = t.split('\n').map(|l| format!("+{l}")).collect::<Vec<_>>().join("\n"); } }
                return (200, json!({"diff": clip(&text, 60000)}));
            }
            "commit" => {
                let msg = get("message"); let msg = msg.trim(); if msg.is_empty() { return fail(400, "Write a short description of what you changed."); }
                git(&["add", "-A"]).await;
                let u = std::env::var("GIT_AUTHOR_NAME").unwrap_or_else(|_| username());
                let em = std::env::var("GIT_AUTHOR_EMAIL").unwrap_or_else(|_| format!("{}@{}", username(), hostname()));
                let r = git(&["-c", &format!("user.name={u}"), "-c", &format!("user.email={em}"), "commit", "-m", msg]).await;
                return if r.ok { (200, json!({"ok": true})) } else { fail(400, &last2(&format!("{}{}", r.out, r.err))) };
            }
            "aside" => { let r = git(&["stash", "push", "-u", "-m", &format!("set aside from Hearth {}", iso(now_ms()))]).await; return if r.ok { (200, json!({"ok": true, "text": r.out.trim()})) } else { fail(400, r.err.trim()) }; }
            "bring-back" => { let r = git(&["stash", "pop"]).await; return if r.ok { (200, json!({"ok": true})) } else { fail(400, &last2(if r.err.is_empty() { &r.out } else { &r.err })) }; }
            "stashes" => { let r = git(&["stash", "list"]).await; return (200, json!({"count": r.out.lines().filter(|l| !l.is_empty()).count()})); }
            _ => {}
        }
    }
    if a == "commands" { return (200, commands(if need_dir() { &cwd } else { "" }, claude.as_deref()).await); }
    if a == "plugins" { if let Some(cl) = &claude {
        if b.is_empty() {
            let d = plugin_data(cl, qget(q, "fresh").as_deref() == Some("1")).await; let ql = get("q").to_lowercase();
            let have: std::collections::HashSet<String> = arr(d.g("installed")).iter().map(|p| p.s("id")).collect();
            let installed: Vec<Value> = arr(d.g("installed")).iter().map(|p| json!({"id": p.s("id"), "name": p.s("id").split('@').next().unwrap_or(""), "version": p.g("version"), "enabled": p.t("enabled")})).collect();
            let mut av: Vec<Value> = arr(d.g("available")).into_iter().filter(|p| !have.contains(&p.s("pluginId")) && (ql.is_empty() || format!("{} {}", p.s("name"), p.s("description")).to_lowercase().contains(&ql))).collect();
            av.sort_by(|x, y| y.n("installCount").partial_cmp(&x.n("installCount")).unwrap_or(std::cmp::Ordering::Equal));
            let total = arr(d.g("available")).len();
            return (200, json!({"installed": installed, "available": av.iter().take(40).map(|p| json!({"id": p.s("pluginId"), "name": p.s("name"), "desc": clip(&p.s("description"), 200), "market": p.g("marketplaceName")})).collect::<Vec<_>>(), "total": total}));
        }
        let id = get("id");
        if !regex::Regex::new(r"^[\w.@-]+$").unwrap().is_match(&id) || !["install", "uninstall", "enable", "disable"].contains(&b) { return fail(400, "bad request"); }
        let r = run(cl, &s(&["plugin", b, &id, "--scope", "user"]), &home().to_string_lossy(), 120000).await;
        PL.lock().unwrap().0 = 0.0;
        return if r.ok { (200, json!({"ok": true})) } else { fail(400, &last2(if r.err.is_empty() { &r.out } else { &r.err })) };
    } }
    if a == "skills" && b == "install" { return match install_skills(&body.s("source")).await { Ok(n) => (200, json!({"added": n})), Err(e) => fail(400, &e) }; }
    if a == "skills" && b == "remove" {
        let name = body.s("name"); let p = skills_dir().join(&name);
        if !safe_name(&name) || !p.join("SKILL.md").exists() { return fail(404, "That skill is not one you added here."); }
        let _ = std::fs::remove_dir_all(&p); return (200, json!({"ok": true}));
    }
    if a == "notes" {
        let scope = if get("scope") == "global" { "global" } else { "project" };
        if scope == "project" && !need_dir() { return fail(400, "This project folder is not on this computer."); }
        let file = if scope == "global" { home().join(".claude").join("CLAUDE.md") } else { Path::new(&cwd).join("CLAUDE.md") };
        if b == "save" { if let Some(d) = file.parent() { let _ = std::fs::create_dir_all(d); } let _ = std::fs::write(&file, body.s("text")); return (200, json!({"ok": true})); }
        return (200, json!({"text": std::fs::read_to_string(&file).unwrap_or_default(), "scope": scope}));
    }
    if a == "mcp" { if let Some(cl) = &claude {
        let h = if need_dir() { cwd.clone() } else { home().to_string_lossy().to_string() };
        if b.is_empty() {
            let r = run(cl, &s(&["mcp", "list"]), &h, 45000).await;
            let re = regex::Regex::new(r"^([^\s:][^:]*):\s+(.+?)\s+-\s+(.+)$").unwrap();
            let items: Vec<Value> = r.out.lines().filter_map(|l| re.captures(l.trim()).map(|m| { let st = m[3].to_string(); json!({"name": &m[1], "target": &m[2], "status": if st.contains('✓') || st.to_lowercase().contains("connected") { "connected" } else if st.to_lowercase().contains("auth") { "needs sign-in" } else { "not working" }}) })).collect();
            return (200, json!({"items": items}));
        }
        if b == "add" {
            let name: String = get("name").chars().filter(|c| c.is_alphanumeric() || *c == '_' || *c == '.' || *c == '-').collect(); let target = get("target").trim().to_string();
            if name.is_empty() || target.is_empty() { return fail(400, "Give it a name and an address or command."); }
            let web = regex::Regex::new(r"(?i)^https?://").unwrap().is_match(&target);
            let mut args = s(&["mcp", "add", "--scope", "user"]);
            if web { args.extend(s(&["--transport", "http"])); args.push(name); args.push(target); } else { args.push(name); args.push("--".into()); args.extend(target.split_whitespace().map(String::from)); }
            let r = run(cl, &args, &h, 30000).await;
            return if r.ok { (200, json!({"ok": true})) } else { fail(400, &last2(if r.err.is_empty() { &r.out } else { &r.err })) };
        }
        if b == "remove" { let r = run(cl, &s(&["mcp", "remove", &get("name"), "--scope", "user"]), &h, 20000).await; return if r.ok { (200, json!({"ok": true})) } else { fail(400, &last2(if r.err.is_empty() { &r.out } else { &r.err })) }; }
    } }
    fail(404, "unknown")
}
