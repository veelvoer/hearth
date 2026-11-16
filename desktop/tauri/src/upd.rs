//! "A new version is ready": notices when GitHub has a newer Hearth than this app or your server runs, and updates them in one go.
use crate::{app, emit, net, notify, sys, version};
use hearth_relay::selfupdate::{latest as gh_latest, newer};
use hearth_relay::util::{arr, home, now_ms, J};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

const MANUAL: &str = " You can also download it yourself: github.com/veelvoer/hearth/releases/latest";

#[derive(Default)]
pub struct Upd { running: AtomicBool, dismissed_at: Mutex<f64>, last: Mutex<String>, gh: Mutex<Option<(f64, Option<Value>)>> }

/// What a relay says about itself: none connected, unreachable, or its /status.
enum Sv { None, Unreachable, Ok(Value) }
async fn relay_status(server: bool) -> Sv {
    let a = app();
    let m = if server { a.server() } else { a.local() };
    match m { None => Sv::None, Some(m) => match a.relay_json(&m, "GET", "/status", None).await { Ok(v) => Sv::Ok(v), Err(_) => Sv::Unreachable } }
}
async fn busy(server: bool) -> bool {
    let a = app();
    let Some(m) = (if server { a.server() } else { a.local() }) else { return false };
    a.relay_json(&m, "GET", "/sessions", None).await.map(|l| arr(&l).iter().any(|s| s.t("running"))).unwrap_or(false)
}

impl Upd {
    async fn latest(&self) -> Option<Value> {
        { let g = self.gh.lock().unwrap(); if let Some((at, r)) = g.as_ref() { if now_ms() - at < 60000.0 { return r.clone(); } } }
        let r = gh_latest().await.ok();
        *self.gh.lock().unwrap() = Some((now_ms(), r.clone()));
        r
    }
    async fn compute(&self) -> (Value, Sv, Option<Value>) {
        let l = self.latest().await; let cur = version(); let sv = relay_status(true).await;
        let lv = l.as_ref().map(|l| l.s("version"));
        let server = match (&lv, &sv) { (Some(v), Sv::Ok(s)) => !s.s("version").is_empty() && s.t("canSelfUpdate") && newer(v, &s.s("version")), _ => false };
        let desktop = !cfg!(debug_assertions) && lv.as_ref().map(|v| newer(v, &cur)).unwrap_or(false);   // a build from source is not replaced by a download
        let svv = if let Sv::Ok(s) = &sv { s.g("version").clone() } else { Value::Null };
        let n = json!({"mode": "release", "latest": lv, "desktop": desktop, "phone": false, "server": server, "cur": cur, "sigKey": json!([lv, cur, svv]).to_string()});
        (n, sv, l)
    }
    fn summary(n: &Value, l: &Option<Value>) -> Option<Value> {
        if !(n.t("desktop") || n.t("server")) { return None; }
        Some(json!({"desktop": n.t("desktop"), "phone": false, "server": n.t("server"), "github": false, "behind": null, "canPhone": false, "canServer": true, "mode": "release", "latest": n.g("latest"), "notes": l.as_ref().map(|l| l.s("notes")).unwrap_or_default()}))
    }

    pub async fn check(&self, force: bool) {
        if self.running.load(Ordering::SeqCst) { return; }
        let (full, _, l) = self.compute().await;
        let Some(n) = Self::summary(&full, &l) else { *self.last.lock().unwrap() = String::new(); return };
        let st = app().st();
        let key = full.s("sigKey");
        if st.s("appliedSig") == key || (st.s("laterSig") == key && now_ms() - st.n("laterAt") < 12.0 * 3600e3) { return; }   // the same state you already updated to (or put off)
        let mut dis = self.dismissed_at.lock().unwrap();
        if *dis > 0.0 && now_ms() - *dis >= 30.0 * 60000.0 { *dis = 0.0; *self.last.lock().unwrap() = String::new(); }
        let k = n.to_string();
        { let last = self.last.lock().unwrap(); if !force && (*last == k || (*dis > 0.0 && now_ms() - *dis < 30.0 * 60000.0)) { return; } }
        drop(dis);
        *self.last.lock().unwrap() = k;
        emit("update-available", n);
        if crate::main_hidden() { notify("An update is ready", "Open Hearth to update the server, your phone and this computer."); }
    }

    /// For the Settings screen: the state of each part, in words.
    pub async fn details(&self) -> Value {
        let (n, sv, l) = self.compute().await;
        let Some(_) = l else { return json!({"unavailable": "Could not look for updates right now. Check your internet connection.", "checkedAt": now_ms() as i64}) };
        let latest = n.s("latest"); let cur = n.s("cur");
        let srv = match sv {
            Sv::None => json!({"na": true, "text": "No server connected"}),
            Sv::Unreachable => json!({"na": true, "text": "The server can't be reached right now"}),
            Sv::Ok(s) if s.s("version").is_empty() => json!({"need": false, "na": true, "text": "Older server: run the installer line again to update it"}),
            Sv::Ok(s) => if n.t("server") { json!({"need": true, "text": format!("Version {} · update to {latest}", s.s("version"))}) }
                else if newer(&latest, &s.s("version")) { json!({"na": true, "text": format!("Version {} · update it on the server with: hearth-server update", s.s("version"))}) }
                else { json!({"text": format!("Version {} · up to date", s.s("version"))}) },
        };
        let desk = if n.t("desktop") { format!("Version {cur} · update to {latest}") } else { format!("Version {cur} · up to date") };
        json!({"mode": "release", "checkedAt": now_ms() as i64, "desktop": {"need": n.t("desktop"), "text": desk}, "phone": {"na": true, "text": "Update it inside the phone app (Settings → Updates)"}, "server": srv})
    }

    pub async fn status(&self) -> Value { let (n, _, l) = self.compute().await; Self::summary(&n, &l).unwrap_or(Value::Null) }

    pub async fn later(&self) {
        *self.dismissed_at.lock().unwrap() = now_ms();
        let (n, _, _) = self.compute().await;
        let a = app();
        { let mut s = a.s.lock().unwrap(); s["laterSig"] = n.g("sigKey").clone(); s["laterAt"] = json!(now_ms() as i64); }
        a.save();
    }

    pub async fn run(&self, want: Value) {
        if self.running.swap(true, Ordering::SeqCst) { return; }
        let say = |step: &str, text: &str, state: &str| emit("update-progress", json!({"step": step, "text": text, "state": state}));
        let l = self.latest().await;
        let mut failed = false; let mut relaunch: Option<Inst> = None;
        if want.t("server") {
            say("server", "Updating the server…", "run");
            let r: Result<(), String> = async {
                if busy(true).await { return Err("Claude is working on the server right now. Try again when it has finished.".into()); }
                let m = app().server().ok_or("No server connected")?;
                let r = app().relay_json(&m, "POST", "/admin/update", Some(json!({}))).await?;
                if r.t("restarting") {
                    for _ in 0..30 { tokio::time::sleep(Duration::from_millis(1500)).await; if let Sv::Ok(s) = relay_status(true).await { if s.s("version") == r.s("to") { break; } } }
                }
                Ok(())
            }.await;
            match r { Ok(()) => say("server", "Server updated", "ok"), Err(e) => { failed = true; say("server", &e, "fail"); } }
        }
        if want.t("desktop") {
            if let Some(l) = &l {
                let r: Result<Inst, String> = async {
                    if busy(false).await { return Err("Claude is working on this computer right now. Try again when it has finished.".into()); }
                    say("desktop", "Downloading the new version…", "run");
                    install_desktop(l).await
                }.await;
                match r {
                    Ok(i) if i.manual.is_some() => say("desktop", i.manual.as_deref().unwrap(), "ok"),
                    Ok(i) => { say("desktop", &format!("{}Restarting…", i.note.as_ref().map(|n| format!("{n} ")).unwrap_or_default()), "ok"); relaunch = Some(i); }
                    Err(e) => { failed = true; say("desktop", &if e.contains("download it yourself") { e } else { format!("{e}{MANUAL}") }, "fail"); }
                }
            }
        }
        self.running.store(false, Ordering::SeqCst);
        if let Some(i) = relaunch { tokio::time::sleep(Duration::from_millis(1200)).await; restart_into(i); return; }
        emit("update-progress", json!({"step": "end", "state": if failed { "fail" } else { "ok" }}));
        if !failed { *self.last.lock().unwrap() = String::new(); }
    }
}

#[derive(Default)]
#[cfg_attr(not(windows), allow(dead_code))]
struct Inst { relaunch: Option<String>, manual: Option<String>, note: Option<String>, run_installer: Option<PathBuf>, swap_bat: Option<PathBuf> }

fn pick<'a>(l: &'a Value, re: &str) -> Option<&'a Value> {
    let re = regex::Regex::new(re).unwrap();
    l.g("assets").as_array().and_then(|a| a.iter().find(|x| re.is_match(&x.s("name"))))
}

/// Downloads a release file next to where it goes, checks its size (and sha256 when GitHub gives it), and only then calls it done.
async fn download(asset: &Value, file: &Path) -> Result<(), String> {
    use futures_util::StreamExt;
    use std::io::Write;
    let res = app().long.get(asset.s("url")).send().await.map_err(|e| net::err_text(&e))?;
    if !res.status().is_success() { return Err(format!("The download failed ({}).", res.status().as_u16())); }
    let total = asset.n("size").max(res.content_length().unwrap_or(0) as f64);
    if let Some(p) = file.parent() { let _ = std::fs::create_dir_all(p); }
    let part = file.with_extension("part");
    let mut f = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut s = res.bytes_stream(); let mut got = 0f64; let mut shown = -1i64;
    let mut h = <sha2::Sha256 as sha2::Digest>::new();
    while let Some(c) = s.next().await {
        let c = c.map_err(|e| format!("The download stopped ({}).", net::err_text(&e)))?;
        f.write_all(&c).map_err(|e| e.to_string())?; sha2::Digest::update(&mut h, &c);
        got += c.len() as f64;
        let pct = if total > 0.0 { (got / total * 100.0).floor() as i64 } else { 0 };
        if pct != shown && pct % 5 == 0 { shown = pct; emit("update-progress", json!({"step": "desktop", "text": format!("Downloading the new version… {pct}%"), "state": "run"})); }
    }
    drop(f);
    if asset.n("size") > 0.0 && got != asset.n("size") { let _ = std::fs::remove_file(&part); return Err(format!("The download is incomplete ({got} of {} bytes).", asset.n("size"))); }
    let digest = asset.s("digest");
    if let Some(want) = digest.strip_prefix("sha256:") {
        if hex::encode(sha2::Digest::finalize(h)) != want.to_lowercase() { let _ = std::fs::remove_file(&part); return Err("The download did not match its checksum.".into()); }
    }
    #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; let _ = std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o755)); }
    std::fs::rename(&part, file).map_err(|e| e.to_string())
}

fn writable(dir: &Path) -> bool { let t = dir.join(format!(".hearth-write-{}", std::process::id())); let ok = std::fs::write(&t, b"").is_ok(); let _ = std::fs::remove_file(&t); ok }

/// An AppImage in your own folder, with a menu entry: an install that can always update itself.
fn move_to_user_appimage(file: &Path) -> Result<String, String> {
    let dir = home().join("Applications"); let _ = std::fs::create_dir_all(&dir);
    let target = dir.join("Hearth.AppImage");
    std::fs::rename(file, &target).or_else(|_| std::fs::copy(file, &target).map(|_| ())).map_err(|e| e.to_string())?;
    let apps = home().join(".local/share/applications"); let _ = std::fs::create_dir_all(&apps);
    let _ = std::fs::write(apps.join("hearth.desktop"), format!("[Desktop Entry]\nType=Application\nName=Hearth\nComment=Claude Code on your phone and computer\nExec=\"{}\" %U\nTerminal=false\nCategories=Development;Utility;\nStartupWMClass=Hearth\n", target.display()));
    Ok(target.to_string_lossy().to_string())
}

async fn install_desktop(l: &Value) -> Result<Inst, String> {
    let tmp = std::env::temp_dir().join("hearth-update"); let _ = std::fs::create_dir_all(&tmp);
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    if cfg!(target_os = "linux") {
        let ai = pick(l, r"AppImage$");
        if let Ok(target) = std::env::var("APPIMAGE") {   // one AppImage file: the new file replaces it in one step
            let a = ai.ok_or(format!("There is no AppImage in the newest release.{MANUAL}"))?;
            let target = PathBuf::from(target);
            if writable(target.parent().unwrap_or(Path::new("/"))) {
                let new = target.with_extension("new");
                download(a, &new).await?;
                std::fs::rename(&new, &target).map_err(|e| e.to_string())?;
                return Ok(Inst { relaunch: Some(target.to_string_lossy().to_string()), ..Default::default() });
            }
            let moved = home().join("Downloads").join(a.s("name"));
            download(a, &moved).await?;
            return Ok(Inst { manual: Some(format!("The folder of the app is read-only, so the new version was saved as {}. Start it from there.", moved.display())), ..Default::default() });
        }
        // a package (.deb) or a copy under /usr: it cannot be replaced without a password
        let system = exe.starts_with("/usr") || exe.starts_with("/opt");
        let has_dpkg = Path::new("/usr/bin/dpkg").exists() || Path::new("/bin/dpkg").exists();
        if let (true, true, Some(deb)) = (system, has_dpkg, pick(l, r"\.deb$")) {
            let file = home().join("Downloads").join(deb.s("name"));
            download(deb, &file).await?;
            sys::open(&file.to_string_lossy());
            return Ok(Inst { manual: Some("The installer is open. Press Install, then start Hearth again.".into()), ..Default::default() });
        }
        let a = ai.ok_or(format!("There is no AppImage in the newest release.{MANUAL}"))?;
        let file = tmp.join(a.s("name")); download(a, &file).await?;
        let moved = move_to_user_appimage(&file)?;
        return Ok(Inst { note: Some(format!("Hearth now runs from {moved} and updates itself from there. You can remove the old installed copy.")), relaunch: Some(moved), ..Default::default() });
    }
    if cfg!(windows) {
        let dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
        if dir.join("uninstall.exe").exists() {   // installed: the installer runs quietly and starts the app again
            let a = pick(l, r"win-x64\.exe$").ok_or(format!("There is no installer in the newest release.{MANUAL}"))?;
            let file = tmp.join(a.s("name")); download(a, &file).await?;
            return Ok(Inst { run_installer: Some(file), ..Default::default() });
        }
        // the portable file: the new one is saved next to it, and a small script puts it in place after this app has closed
        let a = pick(l, r"portable\.exe$").ok_or(format!("There is no portable file in the newest release.{MANUAL}"))?;
        let fresh = exe.with_extension("new"); download(a, &fresh).await?;
        let bat = tmp.join("hearth-swap.bat");
        let (f, t) = (fresh.display(), exe.display());
        std::fs::write(&bat, format!("@echo off\r\nfor /l %%i in (1,1,30) do (move /y \"{f}\" \"{t}\" >nul 2>&1 && goto done)\r\ntimeout /t 1 >nul\r\n:done\r\nstart \"\" \"{t}\"\r\n")).map_err(|e| e.to_string())?;
        return Ok(Inst { swap_bat: Some(bat), ..Default::default() });
    }
    sys::open("https://github.com/veelvoer/hearth/releases/latest");
    Ok(Inst { manual: Some("The download page is open in your browser.".into()), ..Default::default() })
}

/// Ends this app and starts the new one once this one has really exited (or its single-instance check finds this one still there).
fn restart_into(i: Inst) {
    let a = app(); a.quitting.store(true, Ordering::SeqCst); a.save_now();
    let hidden: Vec<String> = std::env::args().filter(|x| x == "--hidden").collect();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let exe = std::env::current_exe().unwrap_or_default();
        if let Some(f) = &i.run_installer { let _ = std::process::Command::new("cmd").raw_arg(format!("/c start /wait \"\" \"{}\" /S && start \"\" \"{}\"", f.display(), exe.display())).creation_flags(0x0800_0000).spawn(); }
        if let Some(b) = &i.swap_bat { let _ = std::process::Command::new("cmd").args(["/c", &b.to_string_lossy()]).creation_flags(0x0800_0000).spawn(); }
    }
    #[cfg(unix)]
    if let Some(path) = &i.relaunch {
        let mut c = hearth_relay::util::cmd("/bin/sh");
        c.args(["-c", "n=0; while kill -0 \"$0\" 2>/dev/null && [ $n -lt 100 ]; do sleep 0.3; n=$((n+1)); done; sleep 0.5; exec \"$@\"", &std::process::id().to_string(), path]).args(&hidden);
        let _ = c.spawn();
    }
    let _ = (&i, &hidden);
    crate::handle().exit(0);
}
