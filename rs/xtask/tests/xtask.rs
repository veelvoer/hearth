//! xtask against the Node scripts it replaces: the same inputs must give the same bytes. The fixture is a minimal copy
//! of the version files (never the whole repo) under ~/hearth-tmp.
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use xtask::*;

fn repo() -> PathBuf { PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap() }
fn tmp() -> tempfile::TempDir {
    let base = PathBuf::from(std::env::var("HOME").unwrap()).join("hearth-tmp");
    std::fs::create_dir_all(&base).unwrap();
    tempfile::Builder::new().prefix("xt-").tempdir_in(base).unwrap()
}
fn have_node() -> bool { Command::new("node").arg("-v").output().map(|o| o.status.success()).unwrap_or(false) }
const VERSION_FILES: [&str; 7] = ["desktop/package.json", "desktop/relay/version.js", "desktop/relay-rs/Cargo.toml", "desktop/tauri/Cargo.toml", "app/build.gradle.kts", "wear/build.gradle.kts", "rs/Cargo.toml"];
fn copy_rel(from: &Path, to: &Path, rel: &str) { let t = to.join(rel); std::fs::create_dir_all(t.parent().unwrap()).unwrap(); std::fs::copy(from.join(rel), t).unwrap(); }
fn fixture(with_script: &str) -> tempfile::TempDir {
    let t = tmp();
    for f in VERSION_FILES { copy_rel(&repo(), t.path(), f); }
    copy_rel(&repo(), t.path(), with_script);
    t
}
fn read(root: &Path, rel: &str) -> Vec<u8> { std::fs::read(root.join(rel)).unwrap_or_else(|_| b"<missing>".to_vec()) }
fn xtask(cwd: &Path, args: &[&str]) -> std::process::Output { Command::new(env!("CARGO_BIN_EXE_xtask")).args(args).current_dir(cwd).output().unwrap() }
fn node(cwd: &Path, args: &[&str]) -> std::process::Output { Command::new("node").args(args).current_dir(cwd).output().unwrap() }

#[test]
fn set_version_matches_node_byte_for_byte() {
    if !have_node() { eprintln!("SKIPPED: node is not installed"); return; }
    let (a, b) = (fixture("scripts/set-version.js"), fixture("scripts/set-version.js"));
    // a sequence on the same fixtures, so each step starts from what the previous one wrote
    for spec in ["feature", "fix", "major", "7.8.9", "fix", "0.0.1"] {
        let ra = node(a.path(), &["scripts/set-version.js", spec]);
        let rb = xtask(b.path(), &["set-version", spec]);
        assert!(ra.status.success() && rb.status.success(), "{spec}: node {} xtask {}", String::from_utf8_lossy(&ra.stderr), String::from_utf8_lossy(&rb.stderr));
        assert_eq!(String::from_utf8_lossy(&ra.stdout), String::from_utf8_lossy(&rb.stdout), "stdout for {spec}");
        for f in &VERSION_FILES[..6] { assert_eq!(read(a.path(), f), read(b.path(), f), "{f} after {spec}"); }
        let v = String::from_utf8_lossy(&rb.stdout).lines().last().unwrap().trim_start_matches("Version is now ").to_string();
        assert_eq!(read(a.path(), &format!("release-notes/{v}.md")), read(b.path(), &format!("release-notes/{v}.md")));
        assert!(String::from_utf8_lossy(&read(b.path(), "rs/Cargo.toml")).contains(&format!("version = \"{v}\"")), "the workspace version follows (the node script leaves it stale)");
    }
    let gradle = String::from_utf8(read(b.path(), "app/build.gradle.kts")).unwrap();
    assert!(gradle.contains("versionCode = 1") && gradle.contains("versionName = \"0.0.1\""));
    // an existing note is never overwritten
    std::fs::write(b.path().join("release-notes/0.0.1.md"), "mine").unwrap();
    xtask(b.path(), &["set-version", "0.0.1"]);
    assert_eq!(read(b.path(), "release-notes/0.0.1.md"), b"mine");
    // bad input: both refuse and change nothing
    let before: Vec<Vec<u8>> = VERSION_FILES.iter().map(|f| read(b.path(), f)).collect();
    for bad in ["1.2", "banana", "1.2.x", "v1.2.3"] {
        assert!(!node(a.path(), &["scripts/set-version.js", bad]).status.success());
        assert!(!xtask(b.path(), &["set-version", bad]).status.success(), "{bad}");
    }
    assert_eq!(before, VERSION_FILES.iter().map(|f| read(b.path(), f)).collect::<Vec<_>>());
}

#[test]
fn version_arithmetic_and_codes() {
    assert_eq!(version::next_version("1.40.0", "feature").as_deref(), Some("1.41.0"));
    assert_eq!(version::next_version("1.40.7", "fix").as_deref(), Some("1.40.8"));
    assert_eq!(version::next_version("1.40.7", "major").as_deref(), Some("2.0.0"));
    assert_eq!(version::next_version("1.40.7", "3.2.1").as_deref(), Some("3.2.1"));
    assert_eq!(version::next_version("1.40.7", "3.2"), None);
    assert_eq!(version::version_code("1.40.0"), 14000);
    assert_eq!(version::version_code("2.3.4"), 20304);
}

#[test]
fn check_release_passes_only_when_everything_agrees() {
    let t = fixture("scripts/set-version.js"); let r = t.path();
    let v = version::current_version(r).unwrap();
    std::fs::create_dir_all(r.join("release-notes")).unwrap();
    // no notes yet
    let e = version::check_release(r, Some(&format!("v{v}"))).unwrap();
    assert_eq!(e, vec![format!("release-notes/{v}.md is missing: every release needs notes")]);
    // a fresh set-version leaves a TODO note
    xtask(r, &["set-version", "fix"]);
    let v2 = version::current_version(r).unwrap();
    assert_ne!(v, v2);
    assert_eq!(version::check_release(r, Some(&v2)).unwrap(), vec![format!("release-notes/{v2}.md still says TODO")]);
    std::fs::write(r.join(format!("release-notes/{v2}.md")), "## What is new\n\n- real notes\n").unwrap();
    assert!(version::check_release(r, Some(&format!("v{v2}"))).unwrap().is_empty());
    let out = xtask(r, &["check-release", "--tag", &format!("v{v2}")]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    // a wrong tag, and each file out of step, are all named
    assert!(version::check_release(r, Some("v9.9.9")).unwrap().len() >= 8);
    for (file, from, to, msg) in [("desktop/tauri/Cargo.toml", format!("version = \"{v2}\""), "version = \"0.0.1\"".to_string(), "desktop/tauri/Cargo.toml"), ("wear/build.gradle.kts", format!("versionName = \"{v2}\""), "versionName = \"0.0.1\"".to_string(), "wear/build.gradle.kts"), ("desktop/package.json", format!("\"version\": \"{v2}\""), "\"version\": \"0.0.1\"".to_string(), "desktop/package.json"), ("rs/Cargo.toml", format!("version = \"{v2}\""), "version = \"0.0.1\"".to_string(), "rs/Cargo.toml"), ("desktop/relay/version.js", v2.clone(), "0.0.1".to_string(), "desktop/relay/version.js")] {
        let orig = std::fs::read_to_string(r.join(file)).unwrap();
        std::fs::write(r.join(file), orig.replacen(&from, &to, 1)).unwrap();
        let e = version::check_release(r, Some(&v2)).unwrap();
        assert!(e.iter().any(|m| m.starts_with(msg)), "{file}: {e:?}");
        let out = xtask(r, &["check-release", "--tag", &v2]);
        assert_eq!(out.status.code(), Some(1));
        std::fs::write(r.join(file), orig).unwrap();
    }
    // versionCode out of step with versionName
    let g = std::fs::read_to_string(r.join("app/build.gradle.kts")).unwrap();
    std::fs::write(r.join("app/build.gradle.kts"), g.replacen(&format!("versionCode = {}", version::version_code(&v2)), "versionCode = 1", 1)).unwrap();
    assert!(version::check_release(r, Some(&v2)).unwrap()[0].contains("versionCode"));
}

#[test]
fn build_mcpkit_matches_node_byte_for_byte() {
    if !have_node() { eprintln!("SKIPPED: node is not installed"); return; }
    let mk = || {
        let t = tmp();
        copy_rel(&repo(), t.path(), "scripts/build-mcpkit.js");
        let d = t.path().join("desktop/relay/mcpkit"); std::fs::create_dir_all(&d).unwrap();
        for e in std::fs::read_dir(repo().join("desktop/relay/mcpkit")).unwrap().flatten() { std::fs::copy(e.path(), d.join(e.file_name())).unwrap(); }
        std::fs::write(d.join("weird.txt"), "tab\t ctl\u{1} \u{8}\u{c} quote\" back\\ slash/ é 😀 line\u{2028}sep \r\n end").unwrap();
        std::fs::write(d.join("Z-upper.js"), "").unwrap();
        t
    };
    let (a, b) = (mk(), mk());
    let ra = node(a.path(), &["scripts/build-mcpkit.js"]);
    let rb = xtask_in(b.path(), &["build-mcpkit"]);
    assert!(ra.status.success() && rb.status.success(), "{}", String::from_utf8_lossy(&rb.stderr));
    assert_eq!(String::from_utf8_lossy(&ra.stdout), String::from_utf8_lossy(&rb.stdout));
    assert_eq!(read(a.path(), "desktop/relay/mcpkit-files.js"), read(b.path(), "desktop/relay/mcpkit-files.js"));
    assert!(mcpkit::is_current(b.path()).unwrap());
    assert_eq!(xtask_in(b.path(), &["build-mcpkit", "--check"]).status.code(), Some(0));
    std::fs::write(b.path().join("desktop/relay/mcpkit/new.js"), "x").unwrap();
    assert_eq!(xtask_in(b.path(), &["build-mcpkit", "--check"]).status.code(), Some(1), "out of date is noticed");
    // and the real repo's packed file is current right now
    assert!(mcpkit::is_current(&repo()).unwrap(), "desktop/relay/mcpkit-files.js is out of date in the repo");
}
/// xtask needs rs/Cargo.toml to find its root.
fn xtask_in(root: &Path, args: &[&str]) -> std::process::Output {
    std::fs::create_dir_all(root.join("rs")).unwrap();
    if !root.join("rs/Cargo.toml").exists() { std::fs::write(root.join("rs/Cargo.toml"), "[workspace]\n").unwrap(); }
    xtask(root, args)
}

#[test]
fn lucide_gen_matches_node_byte_for_byte() {
    if !have_node() { eprintln!("SKIPPED: node is not installed"); return; }
    let t = tmp(); let icons = t.path().join("icons"); std::fs::create_dir_all(&icons).unwrap();
    copy_rel(&repo(), t.path(), "scripts/lucide-gen.js");
    let svgs: [(&str, &str); 7] = [
        ("pin", "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\">\n  <path d=\"M12 17v5\" />\n  <path d=\"M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z\" />\n</svg>\n"),
        ("sun", "<svg>\n  <circle cx=\"12\" cy=\"12\" r=\"4\" />\n  <path d=\"M12 2v2\" />\n  <path d=\"m4.93 4.93 1.41 1.41\" />\n</svg>"),
        ("card", "<svg>\n  <rect width=\"20\" height=\"14\" x=\"2\" y=\"5\" rx=\"2\" />\n  <line x1=\"2\" x2=\"22\" y1=\"10\" y2=\"10\" />\n</svg>"),
        ("plain-rect", "<svg><rect x=\"3\" y=\"3\" width=\"18\" height=\"18\" /></svg>"),
        ("shapes", "<svg><polyline points=\"22 12 18 12 15 21 9 3 6 12 2 12\" /><polygon points=\"12 2 2 7 12 12 22 7 12 2\" /><ellipse cx=\"12\" cy=\"5\" rx=\"9\" ry=\"3\" /></svg>"),
        ("decimals", "<svg><circle cx=\"12.5\" cy=\"7.25\" r=\"0.5\" /><rect x=\"1.5\" y=\"2\" width=\"7\" height=\"5\" rx=\"9\" ry=\"1.5\" /></svg>"),
        ("multi\u{20}space", "<svg><path   d=\"M1 1\"\n      stroke-width=\"3\"   /></svg>"),
    ];
    for (n, s) in svgs { std::fs::write(icons.join(format!("{n}.svg")), s).unwrap(); }
    let map = r#"{"pin":"pin","sun":"sun","card":"card","box":"plain-rect","shapes":"shapes","dec":"decimals","sp":"multi space"}"#;
    let ra = node(t.path(), &["scripts/lucide-gen.js", "icons", map]);
    let rb = xtask_in(t.path(), &["lucide-gen", "icons", map]);
    assert!(ra.status.success() && rb.status.success(), "{} {}", String::from_utf8_lossy(&ra.stderr), String::from_utf8_lossy(&rb.stderr));
    assert_eq!(String::from_utf8_lossy(&ra.stdout), String::from_utf8_lossy(&rb.stdout));
    let j: serde_json::Value = serde_json::from_slice(&rb.stdout).unwrap();
    assert!(j["phone"]["pin"].as_str().unwrap().starts_with("M12 17v5 M9 10.76"));
    // the real map file works too (--key picks "phone")
    let m = lucide::load_map(repo().join("scripts/lucide-map.json").to_str().unwrap(), "phone").unwrap();
    assert!(m.len() > 10 && m["pin"] == "pin");
}

#[test]
fn lint_renderer_pieces() {
    assert_eq!(lint::script_list("<script src=\"a.js\"></script>\n<link>\n<script src=\"b/c.js\" defer></script><script>inline()</script>"), vec!["a.js".to_string(), "b/c.js".to_string()]);
    let real = std::fs::read_to_string(repo().join("desktop/renderer/index.html")).unwrap();
    assert!(!lint::script_list(&real).is_empty());
    let c = lint::renderer_config();
    assert!(c.starts_with("module.exports = [{ files: ['all.js'], languageOptions: { ecmaVersion: 2023, sourceType: 'script', globals: {\"document\":\"readonly\"") && c.contains("\"WeekPlan\":\"readonly\"") && c.contains("'no-undef': 'error'"));
    assert!(lint::node_config().contains("sourceType: 'commonjs'") && lint::node_config().contains("\"__dirname\":\"readonly\""));
}

// ───────────── google-tasks-token against mock Google and a mock Hearth server ─────────────
fn mock(handler: impl Fn(&str, &str) -> (u16, String) + Send + 'static) -> (u16, std::sync::mpsc::Receiver<String>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap(); let port = l.local_addr().unwrap().port();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || for s in l.incoming() {
        let Ok(mut s) = s else { continue };
        let mut buf = vec![0u8; 8192]; let mut n = 0;
        loop { let k = s.read(&mut buf[n..]).unwrap_or(0); n += k; let t = String::from_utf8_lossy(&buf[..n]).to_string(); if let Some(i) = t.find("\r\n\r\n") { let cl = t.to_lowercase().split("content-length:").nth(1).and_then(|x| x.split("\r\n").next()).and_then(|x| x.trim().parse::<usize>().ok()).unwrap_or(0); if n >= i + 4 + cl { break; } } if k == 0 { break; } }
        let req = String::from_utf8_lossy(&buf[..n]).to_string();
        let head = req.lines().next().unwrap_or("").to_string();
        let body = req.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
        let auth = req.lines().find(|l| l.to_lowercase().starts_with("authorization:")).unwrap_or("").to_string();
        let _ = tx.send(format!("{head}\n{auth}\n{body}"));
        let (st, out) = handler(&head, &body);
        let _ = s.write_all(format!("HTTP/1.1 {st} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{out}", out.len()).as_bytes());
    });
    (port, rx)
}
fn browser(url: &str, suffix: &str) -> String {
    let u = url::Url::parse(url).unwrap();
    let redirect = u.query_pairs().find(|(k, _)| k == "redirect_uri").unwrap().1.to_string();
    reqwest::blocking::get(format!("{redirect}/{suffix}")).unwrap().text().unwrap()
}
#[test]
fn google_tasks_flow_hands_the_refresh_token_to_the_server() {
    let (gport, grx) = mock(|_, _| (200, r#"{"refresh_token":"R-TOKEN","access_token":"x"}"#.into()));
    let (hport, hrx) = mock(|_, _| (200, "{}".into()));
    let a = google::Args { client_id: "cid".into(), client_secret: "sec".into(), server: Some(format!("http://127.0.0.1:{hport}/")), token: Some("srv-token".into()) };
    let ep = google::Endpoints { auth: "https://accounts.example/auth".into(), token: format!("http://127.0.0.1:{gport}/token") };
    let mut seen = String::new();
    let page = std::cell::RefCell::new(String::new());
    let code = google::run(&a, &ep, &mut |url| {
        seen = url.to_string();
        let u = url.to_string();
        // the "browser" first hits the page without a code, then comes back with one
        let h = std::thread::spawn(move || { let w = browser(&u, ""); let d = browser(&u, "?code=AUTH%20CODE&scope=x"); (w, d) });
        std::mem::forget(h);   // joined through the mock channels below
    }).unwrap();
    let _ = &page;
    assert_eq!(code, 0);
    assert!(seen.starts_with("https://accounts.example/auth?client_id=cid&redirect_uri=http%3A%2F%2F127.0.0.1%3A") && seen.contains("&response_type=code&scope=https%3A%2F%2Fwww.googleapis.com%2Fauth%2Ftasks&access_type=offline&prompt=consent"), "{seen}");
    let g = grx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
    assert!(g.starts_with("POST /token ") && g.contains("code=AUTH+CODE") && g.contains("client_id=cid") && g.contains("client_secret=sec") && g.contains("grant_type=authorization_code") && g.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A"), "{g}");
    let h = hrx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
    assert!(h.starts_with("POST /admin/options "), "{h}");
    assert!(h.to_lowercase().contains("authorization: bearer srv-token"));
    let body: serde_json::Value = serde_json::from_str(h.split('\n').nth(2).unwrap()).unwrap();
    assert_eq!(body, serde_json::json!({"googleTasks": {"clientId": "cid", "clientSecret": "sec", "refreshToken": "R-TOKEN"}}));
}
#[test]
fn google_tasks_flow_reports_failures() {
    let (gport, _g) = mock(|_, _| (200, r#"{"error":"invalid_grant","error_description":"Bad code"}"#.into()));
    let ep = google::Endpoints { auth: "https://a/auth".into(), token: format!("http://127.0.0.1:{gport}/token") };
    let a = google::Args { client_id: "c".into(), client_secret: "s".into(), server: None, token: None };
    let code = google::run(&a, &ep, &mut |url| { let u = url.to_string(); std::mem::forget(std::thread::spawn(move || browser(&u, "?code=x"))); }).unwrap();
    assert_eq!(code, 1);
    assert!(google::settings_from("c", "s", &serde_json::json!({})).unwrap_err().to_string().contains("no refresh token"));
    assert!(google::settings_from("c", "s", &serde_json::json!({"error": "access_denied"})).unwrap_err().to_string().contains("access_denied"));
    assert!(google::run(&google::Args { client_id: "".into(), client_secret: "".into(), server: None, token: None }, &ep, &mut |_| {}).is_err());
}
