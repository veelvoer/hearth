//! A snapshot of a project before every run (with undo), nightly backups of the chats, and a scan for secrets. Port of safety.js.
//! Uses find and tar like the JavaScript did (every Linux and macOS has them).
use crate::util::*;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const HEAVY: [&str; 14] = ["node_modules", ".git", ".venv", "venv", "__pycache__", ".cache", "dist", "build", ".next", "target", ".uploads", ".gradle", ".idea", ".hearth-trash"];
const MAX_FILES: usize = 20000;
const KEEP_SNAPSHOTS: usize = 8;
fn key(cwd: &str) -> String { sha1_hex(resolve(cwd).to_string_lossy().as_bytes())[..12].to_string() }

/// Every ordinary file of a folder (relative paths), skipping heavy folders. None when there are too many.
pub fn files_of(cwd: &str, max: usize) -> Option<Vec<String>> {
    let mut args: Vec<String> = vec![cwd.into(), "-xdev".into(), "(".into()];
    for (i, n) in HEAVY.iter().enumerate() { if i > 0 { args.push("-o".into()); } args.push("-name".into()); args.push(n.to_string()); }
    args.extend([")", "-prune", "-o", "-type", "f", "-size", "-5M", "-printf", "%P\\0"].iter().map(|s| s.to_string()));
    let o = crate::util::cmd("find").args(&args).output().ok()?;
    if !o.status.success() { return None; }
    let list: Vec<String> = String::from_utf8_lossy(&o.stdout).split('\0').filter(|s| !s.is_empty()).map(String::from).collect();
    if list.len() > max { None } else { Some(list) }
}
fn unsafe_folder(cwd: &str) -> bool { let r = resolve(cwd); let s = r.to_string_lossy().to_string(); s == "/" || r == home() || s.split('/').filter(|x| !x.is_empty()).count() < 2 }
fn tar_status(args: &[&str]) -> Option<i32> { crate::util::cmd("tar").args(args).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().ok().and_then(|s| s.code()) }

pub fn take_snapshot(dir: &Path, cwd: &str, label: &str, sid: &str, text: &str, max_files: usize) -> Value {
    if cwd.is_empty() || unsafe_folder(cwd) || !Path::new(cwd).exists() { return json!({"skipped": "not a project folder"}); }
    let Some(files) = files_of(cwd, max_files) else { return json!({"skipped": "too many files"}) };
    let id = now_i().to_string(); let where_ = dir.join("snapshots").join(key(cwd));
    let _ = std::fs::create_dir_all(&where_);
    let list = where_.join(format!("{id}.list")); let out = where_.join(format!("{id}.tgz"));
    let _ = write_private(&list, files.join("\0").as_bytes());
    let st = tar_status(&["-czf", &out.to_string_lossy(), "-C", cwd, "--null", "-T", &list.to_string_lossy()]);
    let _ = std::fs::remove_file(&list);
    if st != Some(0) && st != Some(1) { let _ = std::fs::remove_file(&out); return json!({"skipped": "tar failed"}); }
    let bytes = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
    let meta = json!({"id": id, "cwd": resolve(cwd).to_string_lossy(), "sid": sid, "label": label, "text": clip(text, 200), "at": now_i(), "files": files.len(), "bytes": bytes});
    write_json_private(&where_.join(format!("{id}.json")), &meta);
    for old in list_snapshots(dir, cwd).iter().skip(KEEP_SNAPSHOTS) { for ext in [".tgz", ".json"] { let _ = std::fs::remove_file(where_.join(format!("{}{}", old.s("id"), ext))); } }
    meta
}
pub fn list_snapshots(dir: &Path, cwd: &str) -> Vec<Value> {
    let where_ = dir.join("snapshots").join(key(cwd));
    let Ok(rd) = std::fs::read_dir(&where_) else { return vec![] };
    let mut l: Vec<Value> = rd.flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".json")).filter_map(|e| read_json(&e.path())).collect();
    l.sort_by(|a, b| b.n("at").partial_cmp(&a.n("at")).unwrap_or(std::cmp::Ordering::Equal)); l
}
/// Puts cwd back the way it was in the snapshot (a snapshot of the present is taken first, so the undo can be undone).
pub fn restore_snapshot(dir: &Path, cwd: &str, id: &str) -> Value {
    let Some(snap) = list_snapshots(dir, cwd).into_iter().find(|s| s.s("id") == id) else { return json!({"error": "That snapshot is gone."}) };
    let file = dir.join("snapshots").join(key(cwd)).join(format!("{}.tgz", snap.s("id")));
    if !file.exists() { return json!({"error": "That snapshot is gone."}); }
    let when = { use chrono::TimeZone; chrono::Local.timestamp_millis_opt(snap.n("at") as i64).single().map(|d| d.format("%-d/%-m/%Y, %H:%M:%S").to_string()).unwrap_or_default() };
    let before = take_snapshot(dir, cwd, "before an undo", "", &format!("before going back to {when}"), MAX_FILES);
    let Ok(lt) = crate::util::cmd("tar").args(["-tzf", &file.to_string_lossy()]).output() else { return json!({"error": "The snapshot could not be read."}) };
    if !lt.status.success() { return json!({"error": "The snapshot could not be read."}); }
    let keep: std::collections::HashSet<String> = String::from_utf8_lossy(&lt.stdout).lines().filter(|l| !l.is_empty()).map(|p| p.trim_start_matches("./").to_string()).collect();
    let now = files_of(cwd, usize::MAX).unwrap_or_default(); let mut removed = 0;
    for f in &now { if !keep.contains(f) && std::fs::remove_file(Path::new(cwd).join(f)).is_ok() { removed += 1; } }
    let mut dirs: Vec<String> = now.iter().filter_map(|f| Path::new(f).parent().map(|p| p.to_string_lossy().to_string())).filter(|d| !d.is_empty() && d != ".").collect::<std::collections::HashSet<_>>().into_iter().collect();
    dirs.sort_by_key(|d| std::cmp::Reverse(d.len()));
    for d in dirs { let _ = std::fs::remove_dir(Path::new(cwd).join(d)); }
    if tar_status(&["-xzf", &file.to_string_lossy(), "-C", cwd]) != Some(0) { return json!({"error": "Putting the files back failed. A snapshot of how it was just now is kept."}); }
    json!({"ok": true, "restored": keep.len(), "removed": removed, "undoId": before.get("id").cloned().unwrap_or(Value::Null)})
}

// ───────── backups of the chats ─────────
fn stamp() -> String { chrono::Utc::now().format("%Y%m%d%H%M%S").to_string() }
pub fn list_backups(dir: &Path) -> Vec<Value> {
    let Ok(rd) = std::fs::read_dir(dir.join("backups")) else { return vec![] };
    let re = regex::Regex::new(r"^chats-\d{14}\.tgz$").unwrap();
    let mut names: Vec<String> = rd.flatten().map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| re.is_match(n)).collect();
    names.sort(); names.reverse();
    names.into_iter().filter_map(|n| { let m = std::fs::metadata(dir.join("backups").join(&n)).ok()?; Some(json!({"name": n, "bytes": m.len(), "at": mtime_ms(&m)})) }).collect()
}
pub fn create_backup(dir: &Path, chats: &Path) -> Value {
    if !chats.exists() { return json!({"error": "There are no chats to back up yet."}); }
    if let Ok(o) = crate::util::cmd("du").args(["-sk", &chats.to_string_lossy()]).output() { if o.status.success() { let kb: f64 = String::from_utf8_lossy(&o.stdout).split_whitespace().next().and_then(|x| x.parse().ok()).unwrap_or(0.0); if kb > 2.0 * 1024.0 * 1024.0 { return json!({"error": "The chats are bigger than 2 GB: too big for a backup here."}); } } }
    let where_ = dir.join("backups"); let _ = std::fs::create_dir_all(&where_);
    let name = format!("chats-{}.tgz", stamp()); let out = where_.join(&name);
    let st = tar_status(&["-czf", &out.to_string_lossy(), "-C", &chats.to_string_lossy(), "--exclude=.hearth-trash", "."]);
    if st != Some(0) && st != Some(1) { let _ = std::fs::remove_file(&out); return json!({"error": "The backup failed."}); }
    #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; let _ = std::fs::set_permissions(&out, std::fs::Permissions::from_mode(0o600)); }
    for old in list_backups(dir).iter().skip(7) { let _ = std::fs::remove_file(where_.join(old.s("name"))); }
    json!({"ok": true, "name": name, "bytes": std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0)})
}
pub fn restore_backup(dir: &Path, chats: &Path, name: &str) -> Value {
    if !regex::Regex::new(r"^chats-\d{14}\.tgz$").unwrap().is_match(name) { return json!({"error": "Unknown backup."}); }
    let file = dir.join("backups").join(name); if !file.exists() { return json!({"error": "That backup is gone."}); }
    let _ = std::fs::create_dir_all(chats);
    let st = tar_status(&["-xzf", &file.to_string_lossy(), "-C", &chats.to_string_lossy(), "--keep-newer-files"]);
    if st == Some(0) || st == Some(2) { json!({"ok": true}) } else { json!({"error": "Restoring failed."}) }
}

// ───────── secrets left in files ─────────
pub fn scan_secrets(root: &str, days: f64, max_files: usize) -> Value {
    let pats: Vec<(&str, regex::Regex)> = [("an AWS access key", r"AKIA[0-9A-Z]{16}"), ("a private key", r"-----BEGIN (?:RSA |EC |OPENSSH |DSA |PGP )?PRIVATE KEY-----"), ("a GitHub token", r"\bgh[pousr]_[A-Za-z0-9]{36,}\b"), ("an Anthropic key", r"\bsk-ant-[A-Za-z0-9_-]{20,}"),
        ("a Slack token", r"\bxox[baprs]-[A-Za-z0-9-]{10,}"), ("a Google API key", r"\bAIza[0-9A-Za-z_-]{35}\b"), ("an OpenAI key", r"\bsk-[A-Za-z0-9]{32,}\b")].iter().map(|(k, r)| (*k, regex::Regex::new(r).unwrap())).collect();
    let cutoff = now_ms() - days * 864e5;
    let mut out = vec![]; let mut seen = 0usize;
    fn walk(d: &Path, depth: usize, root: &str, cutoff: f64, max: usize, seen: &mut usize, out: &mut Vec<Value>, pats: &[(&str, regex::Regex)]) {
        if depth > 8 || *seen >= max { return; }
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            if *seen >= max { return; }
            let p = e.path(); let n = e.file_name().to_string_lossy().to_string();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() { if !HEAVY.contains(&n.as_str()) { walk(&p, depth + 1, root, cutoff, max, seen, out, pats); } continue; }
            if !ft.is_file() { continue; }
            let Ok(m) = e.metadata() else { continue };
            if m.len() > 262144 || mtime_ms(&m) < cutoff { continue; }
            *seen += 1;
            let Ok(b) = std::fs::read(&p) else { continue };
            if b.contains(&0) { continue; }
            let text = String::from_utf8_lossy(&b);
            for (kind, re) in pats { if re.is_match(&text) { out.push(json!({"file": relative(root, &p.to_string_lossy()), "kind": kind})); break; } }
        }
    }
    walk(Path::new(root), 0, root, cutoff, max_files, &mut seen, &mut out, &pats);
    json!({"findings": out, "scanned": seen})
}
/// What is different in a project right now, from git.
pub fn git_changes(cwd: &str) -> Value {
    let run = |a: &[&str]| crate::util::cmd("git").args(["-C", cwd, "-c", "core.quotepath=off"]).args(a).output().ok();
    match run(&["rev-parse", "--is-inside-work-tree"]) { Some(o) if o.status.success() => {} _ => return json!({"git": false, "files": [], "diff": ""}) }
    let st = run(&["status", "--porcelain"]).map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
    let files: Vec<Value> = st.lines().filter(|l| !l.is_empty()).take(200).map(|l| { let x = &l[..2.min(l.len())]; json!({"status": if x.trim() == "??" { "new" } else if x.contains('D') { "deleted" } else { "changed" }, "path": l.get(3..).unwrap_or("").trim_matches('"')}) }).collect();
    let has_head = run(&["rev-parse", "--verify", "-q", "HEAD"]).map(|o| o.status.success()).unwrap_or(false);
    let mut diff = run(if has_head { &["diff", "HEAD", "--no-color", "--stat=60", "-p"] } else { &["diff", "--no-color", "-p"] }).map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
    let cut = diff.chars().count() > 60000; if cut { diff = format!("{}\n… (cut)", clip(&diff, 60000)); }
    json!({"git": true, "files": files, "diff": diff, "cut": cut})
}
pub fn _unused(_: PathBuf) {}

#[cfg(test)]
mod tests {
    // Snapshots with undo, backups of chats, and the scan for secrets (safety.test.js)
    use super::*;
    use std::fs;
    fn tree(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let t = scratch(name); let dir = t.join("data"); let proj = t.join("projects/app");
        fs::create_dir_all(proj.join("src")).unwrap(); fs::create_dir_all(proj.join("node_modules/x")).unwrap();
        fs::write(proj.join("a.txt"), "original a").unwrap(); fs::write(proj.join("c.txt"), "original c").unwrap();
        fs::write(proj.join("src/deep file.js"), "deep").unwrap(); fs::write(proj.join("node_modules/x/big.js"), "heavy").unwrap();
        (t, dir, proj)
    }
    fn snap(dir: &Path, proj: &Path, text: &str) -> Value { take_snapshot(dir, &proj.to_string_lossy(), "before a run", "", text, MAX_FILES) }
    fn rd(p: PathBuf) -> String { fs::read_to_string(p).unwrap_or_default() }
    #[test] fn snapshot_undo_and_friends() {
        let (t, dir, proj) = tree("snap"); let ps = proj.to_string_lossy().to_string();
        let s = snap(&dir, &proj, "make an app");
        assert!(!s["id"].is_null() && s["files"] == 3, "1 a snapshot is taken of the project, without the heavy folders: {s}");
        fs::write(proj.join("a.txt"), "CHANGED").unwrap(); fs::remove_file(proj.join("c.txt")).unwrap(); fs::write(proj.join("new.txt"), "made by Claude").unwrap();
        fs::create_dir(proj.join("newdir")).unwrap(); fs::write(proj.join("newdir/n.txt"), "x").unwrap(); fs::write(proj.join("src/deep file.js"), "edited").unwrap();
        let r = restore_snapshot(&dir, &ps, &s.s("id"));
        assert!(r["ok"] == true && rd(proj.join("a.txt")) == "original a" && rd(proj.join("c.txt")) == "original c" && rd(proj.join("src/deep file.js")) == "deep", "2 undo brings back changed and deleted files: {r}");
        assert!(!proj.join("new.txt").exists() && !proj.join("newdir").exists() && r["removed"] == 2, "3 undo removes what was made since, with its empty folders: {r}");
        assert_eq!(rd(proj.join("node_modules/x/big.js")), "heavy", "4 the heavy folders are left alone");
        assert!(!r["undoId"].is_null() && list_snapshots(&dir, &ps).iter().any(|x| x["id"] == r["undoId"]), "5 the undo itself can be undone");
        assert!(!snap(&dir, &home(), "")["skipped"].is_null() && !take_snapshot(&dir, "/", "", "", "", MAX_FILES)["skipped"].is_null(), "6 a home folder or the root is never snapshotted");
        for _ in 0..10 { snap(&dir, &proj, ""); std::thread::sleep(std::time::Duration::from_millis(5)); }
        assert_eq!(list_snapshots(&dir, &ps).len(), 8, "7 only the last 8 snapshots are kept");
        let _ = fs::remove_dir_all(t);
    }
    #[test] fn backups() {
        let t = scratch("backup"); let dir = t.join("data"); let chats = t.join("claude/projects");
        fs::create_dir_all(chats.join("-p-a")).unwrap(); fs::create_dir_all(chats.join(".hearth-trash")).unwrap();
        fs::write(chats.join("-p-a/one.jsonl"), "{\"a\":1}\n").unwrap(); fs::write(chats.join(".hearth-trash/gone.jsonl"), "trash").unwrap();
        let b = create_backup(&dir, &chats); let name = b.s("name");
        assert!(b["ok"] == true && list_backups(&dir).len() == 1 && name.starts_with("chats-") && name.ends_with(".tgz") && name.len() == "chats-20260101000000.tgz".len(), "8 a backup of the chats is made: {b}");
        fs::remove_file(chats.join("-p-a/one.jsonl")).unwrap();
        let rb = restore_backup(&dir, &chats, &name);
        let listing = String::from_utf8_lossy(&std::process::Command::new("tar").arg("-tzf").arg(dir.join("backups").join(&name)).output().unwrap().stdout).to_string();
        assert!(rb["ok"] == true && chats.join("-p-a/one.jsonl").exists() && !listing.contains("gone.jsonl"), "9 restoring brings a deleted chat back, and the trash was not part of the backup");
        fs::write(chats.join("-p-a/one.jsonl"), "NEWER CONTENT").unwrap();
        restore_backup(&dir, &chats, &name);
        assert_eq!(fs::read_to_string(chats.join("-p-a/one.jsonl")).unwrap(), "NEWER CONTENT", "10 a chat that is newer than the backup is not overwritten");
        assert!(!restore_backup(&dir, &chats, "../../etc/passwd")["error"].is_null(), "11 a strange backup name is refused");
        let _ = fs::remove_dir_all(t);
    }
    #[test] fn secrets() {
        let t = scratch("secrets"); let sp = t.join("projects/leaky");
        fs::create_dir_all(sp.join("node_modules")).unwrap();
        fs::write(sp.join(".env"), "KEY=AKIAABCDEFGHIJKLMNOP\n").unwrap(); fs::write(sp.join("fine.js"), "const a = 1;").unwrap();
        fs::write(sp.join("node_modules/k.js"), "-----BEGIN PRIVATE KEY-----").unwrap(); fs::write(sp.join("tok.txt"), format!("ghp_{}", "a".repeat(36))).unwrap();
        let sc = scan_secrets(&t.join("projects").to_string_lossy(), 30.0, 5000);
        let f = sc["findings"].as_array().cloned().unwrap_or_default();
        assert!(f.len() == 2 && f.iter().any(|x| x.s("file") == "leaky/.env" && x.s("kind").contains("AWS")) && f.iter().any(|x| x.s("kind").contains("GitHub")) && !sc.to_string().contains("AKIAABC"), "12 secrets are found with their place and kind, never their value: {sc}");
        assert!(!f.iter().any(|x| x.s("file").contains("node_modules")), "13 heavy folders are not searched");
        let _ = fs::remove_dir_all(t);
    }
}
