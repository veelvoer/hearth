//! Real assertions: generated text exactly, the installer on a fake root, migration on a fake HOME, hooks, updates against a local mock of GitHub.
use hearth_server::detect::*;
use hearth_server::install::{self, Opts};
use hearth_server::sysx::Env;
use hearth_server::texts::*;
use hearth_server::ui::*;
use hearth_server::update::{self, Source};
use hearth_server::{hooks, local, manage, voice};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};

fn tmp() -> tempfile::TempDir {
    let base = PathBuf::from(std::env::var("HOME").unwrap()).join("hearth-tmp");
    std::fs::create_dir_all(&base).unwrap();
    tempfile::Builder::new().prefix("srv-").tempdir_in(base).unwrap()
}
fn script(dir: &Path, name: &str, version: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let f = dir.join(name);
    std::fs::write(&f, format!("#!/bin/sh\n[ \"$1\" = --version ] && echo {version}\n")).unwrap();
    std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
    f
}
fn fake_env(prefix: &Path, user: &str) -> Env {
    Env { prefix: Some(prefix.to_path_buf()), dry: false, yes: false, is_root: false, user: user.into(), self_exe: script(prefix, "self-hearth-server", hearth_server::VERSION) }
}
/// A tiny HTTP server: path -> (status, body). Runs until the test process ends.
fn serve(routes: HashMap<String, (u16, Vec<u8>)>) -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(mut s) = s else { continue };
            let routes = routes.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 2048]; let n = s.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                let (st, body) = routes.get(&path).cloned().unwrap_or((404, b"nope".to_vec()));
                let _ = s.write_all(format!("HTTP/1.1 {st} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).as_bytes());
                let _ = s.write_all(&body);
            });
        }
    });
    port
}

// ───────────── generated text ─────────────
#[test]
fn unit_text_is_exact_and_runs_the_rust_relay() {
    let t = unit_text(&UnitArgs { user: "alice", home: "/home/alice", relay_bin: "/opt/hearth/hearth-relay", work_dir: "/opt/hearth", cfg_dir: "/home/alice/.config/hearth", projects: "/home/alice/projects", url: "https://1-2-3-4.sslip.io", name: "My server", host: "127.0.0.1", port: 47601 });
    let want = "[Unit]
Description=Hearth server (Claude Code chats for your phone and computers)
After=network-online.target
Wants=network-online.target

[Service]
User=alice
WorkingDirectory=/opt/hearth
ExecStart=/opt/hearth/hearth-relay --host 127.0.0.1 --port 47601 --projects /home/alice/projects --dir /home/alice/.config/hearth --url https://1-2-3-4.sslip.io --name \"My server\"
Environment=HOME=/home/alice
Environment=PATH=/usr/local/bin:/usr/bin:/bin:/home/alice/.local/bin
Restart=always
RestartSec=3
NoNewPrivileges=yes
PrivateTmp=yes
ProtectSystem=full
ProtectKernelTunables=yes
ProtectControlGroups=yes

[Install]
WantedBy=multi-user.target
";
    assert_eq!(t, want);
    assert!(!t.contains("node"));
}
#[test]
fn systemd_words_are_quoted_and_percent_is_doubled() {
    assert_eq!(sd_word("plain-1.2/x"), "plain-1.2/x");
    assert_eq!(sd_word("My server"), "\"My server\"");
    assert_eq!(sd_word("50% \"off\" $HOME"), "\"50%% \\\"off\\\" $$HOME\"");
    let t = unit_text(&UnitArgs { user: "u", home: "/h", relay_bin: "/r", work_dir: "/w", cfg_dir: "/c", projects: "/p", url: "http://x", name: "a b", host: "0.0.0.0", port: 1 });
    assert!(t.contains("--name \"a b\"\n"));
}
#[test]
fn caddyfile_is_exact_with_and_without_import() {
    let with = caddy_text("hearth.example.com", "127.0.0.1:47601", Some("/etc/caddy/hearth.d"));
    assert_eq!(with, "# Written by the Hearth installer\nhearth.example.com {\n\theader {\n\t\tStrict-Transport-Security \"max-age=31536000\"\n\t\tX-Content-Type-Options \"nosniff\"\n\t\t-Server\n\t}\n\timport /etc/caddy/hearth.d/*.caddy\n\treverse_proxy 127.0.0.1:47601 {\n\t\tflush_interval -1\n\t}\n}\n");
    let without = caddy_text("hearth.example.com", "hearth:47601", None);
    assert!(!without.contains("import"));
    assert!(without.contains("\treverse_proxy hearth:47601 {\n"));
}
#[test]
fn docker_files_use_the_binary_and_quote_values() {
    let d = dockerfile_text(47601);
    assert!(d.contains("COPY --chmod=755 hearth-relay /usr/local/bin/hearth-relay"));
    assert!(d.contains("CMD [\"hearth-relay\", \"--host\", \"0.0.0.0\", \"--port\", \"47601\", \"--projects\", \"/data/projects\", \"--dir\", \"/data/config\"]"));
    let c = compose_text("https://x.example.com", "Bob's \"box\"", true, 47602);
    assert!(c.contains("\"--url\", \"https://x.example.com\", \"--name\", \"Bob's \\\"box\\\"\"]"));
    assert!(c.contains("127.0.0.1:47602:47602") && c.contains("image: caddy:2") && c.contains("- \"443:443\""));
    let c2 = compose_text("http://1.2.3.4:47601", "n", false, 47601);
    assert!(c2.contains("      - \"47601:47601\"\n") && !c2.contains("caddy"));
}
#[test]
fn local_unit_matches_install_node_sh() {
    let t = local_unit_text("/h/.local/share/hearth-node/hearth-relay", "/h", "/h/.config/hearth-node", &["--seed-token-file".into(), "/h/t".into(), "--voice-port".into(), "47602".into(), "--voice-token-file".into(), "/h/t".into()]);
    assert!(t.contains("ExecStart=/h/.local/share/hearth-node/hearth-relay --lan --host 0.0.0.0 --port 47601 --dir /h/.config/hearth-node --move-root /h/projects --seed-token-file /h/t --voice-port 47602 --voice-token-file /h/t\n"));
    assert!(t.contains("Environment=PATH=/h/.local/bin:/usr/local/bin:/usr/bin\nRestart=always\nRestartSec=3\n\n[Install]\nWantedBy=default.target\n"));
}
#[test]
fn voice_unit_and_exec_patch() {
    let u = voice_unit_text("/usr/bin/python3", "/h/.local/share/hearth-relay", "/h");
    assert!(u.contains("ExecStart=/usr/bin/python3 /h/.local/share/hearth-relay/claude_relay.py\n") && u.contains("Environment=CM_PORT=47602\nEnvironment=CM_SIDECAR=1\n"));
    let p = voice::patch_unit_exec(&u, "/v/bin/python", "/h/.local/share/hearth-relay");
    assert!(p.contains("ExecStart=/v/bin/python /h/.local/share/hearth-relay/claude_relay.py\n") && p.contains("Restart=always"));
    assert!(voice::CLAUDE_RELAY_PY.contains("CM_SIDECAR") && voice::model_downloads().len() == 6);
}

// ───────────── ui ─────────────
#[test]
fn wrap_and_questions() {
    assert_eq!(wrap("aaa bbb ccc", "  ", 7), "  aaa bbb\n  ccc");
    assert_eq!(wrap("a\nb", "-", 10), "-a\n-b");
    let mut p = Prompt::scripted(false, "9\nx\n2\n\nmaybe\ny\n");
    assert_eq!(p.choose("q", &[("a", ""), ("b", "")], 0), 1);   // 9 and x are refused, 2 taken
    assert!(p.confirm("sure", false) == false);                  // empty line: the default
    assert!(p.confirm("sure", false));                           // "maybe" is asked again, then y
    assert_eq!(Prompt::scripted(false, "").ask("name", "dflt"), "dflt");
    let mut yes = Prompt::scripted(true, "");
    assert_eq!(yes.choose("q", &[("a", ""), ("b", "")], 1), 1);
    assert_eq!(yes.ask("q", "d"), "d");
}

// ───────────── small helpers ─────────────
#[test]
fn versions_checksums_and_links() {
    assert!(update::newer("1.41.0", "1.40.9") && update::newer("v2.0.0", "1.99.99") && !update::newer("1.40.0", "1.40.0") && !update::newer("1.39.9", "1.40.0"));
    assert!(update::valid_tag("v1.2.3") && !update::valid_tag("1.2.3") && !update::valid_tag("v1.2") && !update::valid_tag("v1.2.3; rm"));
    let sums = "abc123  hearth-relay-linux-x64\nDEF456 *hearth-relay-linux-arm64\n";
    assert_eq!(update::checksum_for(sums, "hearth-relay-linux-x64").as_deref(), Some("abc123"));
    assert_eq!(update::checksum_for(sums, "hearth-relay-linux-arm64").as_deref(), Some("def456"));
    assert_eq!(update::checksum_for(sums, "relay-x64"), None);
    assert_eq!(update::sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    assert_eq!(manage::pairing_link("https://a.b", "tok en", "My server"), "hearth://pair?url=https%3A%2F%2Fa.b&token=tok+en&name=My+server");
    assert_eq!(manage::pretty_code("123456"), "123 456");
    assert!(valid_domain_ok());
    assert_eq!(install::instance_for("Bob_Smith-9"), "hearth-bobsmith9");
    assert_eq!(install::free_port(&[47601, 47602], false), 47603);
    assert_eq!(parse_os_release("NAME=x\nPRETTY_NAME=\"Debian GNU/Linux 12\"\nID=debian\n"), ("Debian GNU/Linux 12".into(), "debian".into()));
}
fn valid_domain_ok() -> bool { install::valid_domain("hearth.example.com") && install::valid_domain("1-2-3-4.sslip.io") && !install::valid_domain("nodots") && !install::valid_domain("a b.com") }

// ───────────── network helpers against a local server ─────────────
#[test]
fn public_ip_lookup_and_health_check() {
    let mut r = HashMap::new();
    r.insert("/bad".to_string(), (200, b"<html>".to_vec()));
    r.insert("/ip".to_string(), (200, b"198.51.100.4\n".to_vec()));
    r.insert("/health".to_string(), (200, br#"{"ok":true}"#.to_vec()));
    let port = serve(r);
    assert_eq!(public_ip_from(&[&format!("http://127.0.0.1:{port}/missing"), &format!("http://127.0.0.1:{port}/bad"), &format!("http://127.0.0.1:{port}/ip")]), "198.51.100.4");
    assert_eq!(public_ip_from(&[&format!("http://127.0.0.1:{port}/bad")]), "");
    assert!(tcp_up(port));
    let mut r2 = HashMap::new(); r2.insert("/health".to_string(), (500, b"x".to_vec()));
    assert!(!tcp_up(serve(r2)), "TCP is open but /health says 500");
    let free = { let l = TcpListener::bind("127.0.0.1:0").unwrap(); l.local_addr().unwrap().port() };
    assert!(!tcp_up(free), "nothing listens");
}

// ───────────── the installer on a fake root ─────────────
fn opts(relay: &Path) -> Opts { Opts { public_ip: Some("203.0.113.7".into()), relay_bin: Some(relay.to_path_buf()), ..Default::default() } }

#[test]
fn install_direct_with_own_domain_answers_typed_by_hand() {
    let t = tmp(); let root = t.path();
    let relay = script(root, "relay-src", hearth_server::VERSION);
    let env = fake_env(root, "alice");
    // Enter (intro), 1 = directly, 2 = own domain, the domain, the name
    let mut pr = Prompt::scripted(false, "\n1\n2\nhearth.example.com\nHome box\n");
    let meta = install::install(&env, &mut pr, &opts(&relay)).unwrap();
    let dir = root.join("opt/hearth");
    assert_eq!(meta["url"], "https://hearth.example.com");
    assert_eq!(meta["port"], 47601);
    let unit = std::fs::read_to_string(root.join("etc/systemd/system/hearth.service")).unwrap();
    let home = root.join("home/alice");
    assert!(unit.contains(&format!("ExecStart={}/opt/hearth/hearth-relay --host 127.0.0.1 --port 47601 --projects {}/projects --dir {}/.config/hearth --url https://hearth.example.com --name \"Home box\"\n", root.display(), home.display(), home.display())));
    assert!(unit.contains("User=alice\n"));
    assert!(dir.join("hearth-relay").is_file() && dir.join("hearth-server").is_file() && home.join("projects").is_dir() && home.join(".config/hearth").is_dir());
    let caddy = std::fs::read_to_string(root.join("etc/caddy/hearth.caddy")).unwrap();
    assert!(caddy.starts_with("# Written by the Hearth installer\nhearth.example.com {\n") && caddy.contains("reverse_proxy 127.0.0.1:47601 {"));
    assert!(caddy.contains("import /etc/caddy/hearth.d/*.caddy"));
    assert!(root.join("etc/caddy/hearth.d/00-hearth.caddy").is_file());
    let main = std::fs::read_to_string(root.join("etc/caddy/Caddyfile")).unwrap();
    assert_eq!(main, "import /etc/caddy/hearth.caddy\n");
    let wrapper = std::fs::read_to_string(root.join("usr/local/bin/hearth-server")).unwrap();
    assert_eq!(wrapper, format!("#!/bin/sh\nexec {}/opt/hearth/hearth-server \"$@\"\n", root.display()));
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("install.json")).unwrap()).unwrap();
    assert_eq!(saved["mode"], "direct"); assert_eq!(saved["name"], "Home box"); assert_eq!(saved["user"], "alice"); assert_eq!(saved["bin"], "hearth-server"); assert_eq!(saved["version"], hearth_server::VERSION);
    // running it twice is safe and keeps one import line
    let mut pr2 = Prompt::scripted(true, "");
    install::install(&env, &mut pr2, &Opts { address: Some("hearth.example.com".into()), name: Some("Home box".into()), ..opts(&relay) }).unwrap();
    assert_eq!(std::fs::read_to_string(root.join("etc/caddy/Caddyfile")).unwrap(), main);
}
#[test]
fn install_free_address_and_no_padlock() {
    let t = tmp(); let root = t.path();
    let relay = script(root, "relay-src", hearth_server::VERSION);
    let env = fake_env(root, "alice");
    let mut yes = Prompt::scripted(true, "");
    let m = install::install(&env, &mut yes, &opts(&relay)).unwrap();   // all easy choices: direct + free sslip.io address
    assert_eq!(m["domain"], "203-0-113-7.sslip.io"); assert_eq!(m["url"], "https://203-0-113-7.sslip.io"); assert_eq!(m["name"], "My server");
    let t2 = tmp(); let root2 = t2.path();
    let env2 = fake_env(root2, "alice");
    let m2 = install::install(&env2, &mut Prompt::scripted(true, ""), &Opts { address: Some("none".into()), ..opts(&relay) }).unwrap();
    assert_eq!(m2["url"], "http://203.0.113.7:47601");
    let unit = std::fs::read_to_string(root2.join("etc/systemd/system/hearth.service")).unwrap();
    assert!(unit.contains("--host 0.0.0.0 ") && !root2.join("etc/caddy").exists());
}
#[test]
fn install_refuses_a_bad_domain_and_someone_elses_instance() {
    let t = tmp(); let root = t.path();
    let relay = script(root, "relay-src", hearth_server::VERSION);
    let env = fake_env(root, "alice");
    let e = install::install(&env, &mut Prompt::scripted(true, ""), &Opts { address: Some("not a domain".into()), ..opts(&relay) }).unwrap_err();
    assert!(e.to_string().contains("does not look like a domain"));
    install::install(&env, &mut Prompt::scripted(true, ""), &opts(&relay)).unwrap();
    let bob = fake_env(root, "bob");
    let e = install::install(&bob, &mut Prompt::scripted(true, ""), &Opts { instance: Some("hearth".into()), ..opts(&relay) }).unwrap_err();
    assert!(e.to_string().contains("belongs to alice"));
}
#[test]
fn second_person_gets_their_own_instance_port_and_address() {
    let t = tmp(); let root = t.path();
    let relay = script(root, "relay-src", hearth_server::VERSION);
    install::install(&fake_env(root, "alice"), &mut Prompt::scripted(true, ""), &Opts { address: Some("a.example.com".into()), ..opts(&relay) }).unwrap();
    let m = install::install(&fake_env(root, "bob"), &mut Prompt::scripted(true, ""), &Opts { address: Some("b.example.com".into()), ..opts(&relay) }).unwrap();
    assert_eq!(m["instance"], "hearth-bob"); assert_eq!(m["port"], 47602); assert_eq!(m["bin"], "hearth-bob-server");
    assert!(root.join("etc/systemd/system/hearth-bob.service").is_file());
    assert!(std::fs::read_to_string(root.join("etc/caddy/Caddyfile")).unwrap().lines().count() >= 2);
    assert_eq!(std::fs::read_to_string(root.join("usr/local/bin/hearth-bob-server")).unwrap(), format!("#!/bin/sh\nexec {}/opt/hearth-bob/hearth-server --instance hearth-bob \"$@\"\n", root.display()));
    let e = install::install(&fake_env(root, "carol"), &mut Prompt::scripted(true, ""), &Opts { address: Some("b.example.com".into()), ..opts(&relay) }).unwrap_err();
    assert!(e.to_string().contains("already belongs to the Hearth of bob"));
    assert_eq!(install::find_instances(&root.join("opt")).len(), 2);
}
#[test]
fn install_docker_writes_compose_dockerfile_and_caddyfile() {
    let t = tmp(); let root = t.path();
    let relay = script(root, "relay-src", hearth_server::VERSION);
    let m = install::install(&fake_env(root, "alice"), &mut Prompt::scripted(true, ""), &Opts { mode: Some("docker".into()), address: Some("d.example.com".into()), ..opts(&relay) }).unwrap();
    let dir = root.join("opt/hearth");
    assert_eq!(m["mode"], "docker"); assert_eq!(m["cfgDir"], dir.join("data/config").to_string_lossy().to_string());
    assert!(dir.join("hearth-relay").is_file() && dir.join("data/claude.json").is_file() && dir.join("data/caddy").is_dir());
    assert_eq!(std::fs::read_to_string(dir.join("Dockerfile")).unwrap(), dockerfile_text(47601));
    assert_eq!(std::fs::read_to_string(dir.join("docker-compose.yml")).unwrap(), compose_text("https://d.example.com", "My server", true, 47601));
    assert_eq!(std::fs::read_to_string(dir.join("Caddyfile")).unwrap(), caddy_text("d.example.com", "hearth:47601", None));
}
#[test]
fn dry_run_writes_nothing() {
    let t = tmp(); let root = t.path();
    let relay = script(root, "relay-src", hearth_server::VERSION);
    let mut env = fake_env(root, "alice"); env.dry = true;
    install::install(&env, &mut Prompt::scripted(true, ""), &opts(&relay)).unwrap();
    assert!(!root.join("opt").exists() && !root.join("etc").exists());
}
#[test]
fn uninstall_removes_program_unit_and_caddy_but_keeps_projects() {
    let t = tmp(); let root = t.path();
    let relay = script(root, "relay-src", hearth_server::VERSION);
    let env = fake_env(root, "alice");
    let m = install::install(&env, &mut Prompt::scripted(true, ""), &Opts { address: Some("a.example.com".into()), ..opts(&relay) }).unwrap();
    std::fs::write(root.join("etc/caddy/Caddyfile"), "other.example.com {\n}\n\nimport /etc/caddy/hearth.caddy\n").unwrap();
    let (inst, meta) = manage::pick_instance(&env, None).unwrap();
    assert_eq!(meta["url"], m["url"]);
    let c = manage::Ctx { env: &env, instance: inst, meta, install_dir: root.join("opt/hearth"), port: 47601 };
    manage::uninstall(&c, &mut Prompt::scripted(false, "y\n")).unwrap();
    assert!(!root.join("etc/systemd/system/hearth.service").exists() && !root.join("etc/caddy/hearth.caddy").exists() && !root.join("opt/hearth/install.json").exists() && !root.join("opt/hearth/hearth-relay").exists() && !root.join("usr/local/bin/hearth-server").exists());
    assert_eq!(std::fs::read_to_string(root.join("etc/caddy/Caddyfile")).unwrap(), "other.example.com {\n}\n\n");
    assert!(root.join("home/alice/projects").is_dir());
    // "no" keeps everything
    install::install(&env, &mut Prompt::scripted(true, ""), &opts(&relay)).unwrap();
    manage::uninstall(&c, &mut Prompt::scripted(false, "n\n")).unwrap();
    assert!(root.join("opt/hearth/install.json").exists());
}

// ───────────── migration on a fake HOME ─────────────
#[test]
fn migration_from_claude_meter_and_old_node_files() {
    let t = tmp(); let h = t.path();
    let w = |p: &str, s: &str| { let f = h.join(p); std::fs::create_dir_all(f.parent().unwrap()).unwrap(); std::fs::write(f, s).unwrap(); };
    w(".config/claude-meter-node/relay.json", "{\"token\":\"OLD\"}");
    w(".config/claude-meter-node/sub/state.json", "{}");
    w(".config/systemd/user/claude-meter-node.service", "[Unit]\n");
    w(".local/share/claude-meter-node/standalone.js", "x");
    w(".local/share/claude-meter-relay/claude_relay.py", "py");
    w(".config/claude-meter-relay/token", "VOICE");
    w(".local/share/hearth-node/server.js", "old node"); w(".local/share/hearth-node/embedded.js", "old"); w(".local/share/hearth-node/keep.txt", "k");
    let m = local::migrate(h, false).unwrap();
    assert!(m.copied_config && m.retired_service);
    assert_eq!(std::fs::read_to_string(h.join(".config/hearth-node/relay.json")).unwrap(), "{\"token\":\"OLD\"}");
    assert!(h.join(".config/hearth-node/sub/state.json").is_file());
    assert!(!h.join(".config/systemd/user/claude-meter-node.service").exists() && !h.join(".local/share/claude-meter-node").exists());
    assert_eq!(std::fs::read_link(h.join(".local/share/hearth-relay")).unwrap(), h.join(".local/share/claude-meter-relay"));
    assert_eq!(std::fs::read_to_string(h.join(".config/hearth-relay/token")).unwrap(), "VOICE");   // through the link
    assert_eq!(m.removed_scripts, vec!["embedded.js".to_string(), "server.js".to_string()]);
    assert!(h.join(".local/share/hearth-node/keep.txt").exists() && !h.join(".local/share/hearth-node/server.js").exists());
    // pairing already there is never overwritten; second run changes nothing
    w(".config/hearth-node/relay.json", "{\"token\":\"NEW\"}");
    let again = local::migrate(h, false).unwrap();
    assert!(!again.copied_config && !again.retired_service && again.linked.is_empty() && again.removed_scripts.is_empty());
    assert_eq!(std::fs::read_to_string(h.join(".config/hearth-node/relay.json")).unwrap(), "{\"token\":\"NEW\"}");
}
#[test]
fn local_install_writes_unit_with_rust_binary_and_voice_args() {
    let t = tmp(); let h = t.path().join("home"); std::fs::create_dir_all(h.join(".config/hearth-relay")).unwrap();
    std::fs::write(h.join(".config/hearth-relay/token"), "VT").unwrap();
    std::fs::create_dir_all(h.join(".config/claude-meter-node")).unwrap(); std::fs::write(h.join(".config/claude-meter-node/relay.json"), "{}").unwrap();
    let relay = script(t.path(), "r", "1.0.0");
    let unit = local::install(&h, &local::LocalOpts { relay_bin: Some(relay), ..Default::default() }, false).unwrap();
    let p = local::paths(&h);
    assert_eq!(std::fs::read_to_string(&p.unit).unwrap(), unit);
    let tok = h.join(".config/hearth-relay/token").to_string_lossy().to_string();
    assert!(unit.contains(&format!("ExecStart={}/.local/share/hearth-node/hearth-relay --lan --host 0.0.0.0 --port 47601 --dir {}/.config/hearth-node --move-root {}/projects --seed-token-file {tok} --voice-port 47602 --voice-token-file {tok}\n", h.display(), h.display(), h.display())));
    assert!(p.dest.join("hearth-relay").is_file() && p.cfg.join("relay.json").is_file());
}
#[test]
fn update_local_refreshes_unit_and_voice_files() {
    let t = tmp(); let h = t.path().join("home");
    let relay = script(t.path(), "r", "1.0.0");
    let o = local::LocalOpts { relay_bin: Some(relay), ..Default::default() };
    assert!(local::update_local(&h, &o).unwrap().contains("nothing installed"));
    local::install(&h, &o, false).unwrap();
    let py = h.join(".local/share/hearth-relay"); std::fs::create_dir_all(&py).unwrap();
    std::fs::write(py.join("claude_relay.py"), "old").unwrap(); std::fs::write(py.join("nl_tts_server.py"), "old").unwrap();
    std::fs::remove_file(local::paths(&h).unit).unwrap(); std::fs::write(local::paths(&h).unit, "stale").unwrap();
    let r = local::update_local(&h, &o).unwrap();
    assert!(r.contains("relay") && r.contains("voice engine"));
    assert!(std::fs::read_to_string(local::paths(&h).unit).unwrap().starts_with("[Unit]"));
    assert_eq!(std::fs::read_to_string(py.join("claude_relay.py")).unwrap(), voice::CLAUDE_RELAY_PY);
    assert_eq!(std::fs::read_to_string(py.join("nl_tts_server.py")).unwrap(), voice::NL_TTS_PY);
}
#[test]
fn voice_service_unit_uses_venv_when_present() {
    let t = tmp(); let h = t.path();
    let u = voice::install_service(h, false).unwrap();
    assert!(u.contains("ExecStart=/usr/bin/python3 "));
    std::fs::create_dir_all(h.join(".local/share/hearth-relay/venv/bin")).unwrap(); std::fs::write(h.join(".local/share/hearth-relay/venv/bin/python"), "").unwrap();
    let u2 = voice::install_service(h, false).unwrap();
    assert!(u2.contains(&format!("ExecStart={}/.local/share/hearth-relay/venv/bin/python ", h.display())));
    assert!(h.join(".local/share/hearth-relay/claude_relay.py").is_file());
}

// ───────────── hooks, cm_hook, call_me ─────────────
#[test]
fn hooks_install_remove_keep_foreign_settings_and_order() {
    let t = tmp(); let h = t.path();
    let f = hooks::settings_path(h); std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    std::fs::write(&f, r#"{"zeta": 1, "alpha": {"b": 2, "a": 1}, "hooks": {"Stop": [{"matcher": "", "hooks": [{"type": "command", "command": "notify-send done"}]}], "SessionStart": [{"hooks": [{"type": "command", "command": "echo hi"}]}]}}"#).unwrap();
    assert!(!hooks::status(h));
    assert!(hooks::install(h, 47601, &hooks::HookCommand::Curl).unwrap());
    assert!(hooks::status(h));
    let text = std::fs::read_to_string(&f).unwrap();
    assert!(text.find("\"zeta\"").unwrap() < text.find("\"alpha\"").unwrap(), "key order kept");
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["hooks"]["Stop"].as_array().unwrap().len(), 2);
    assert_eq!(v["hooks"]["Stop"][1]["hooks"][0]["timeout"], 10);
    assert_eq!(v["hooks"]["PreToolUse"][0]["matcher"], "AskUserQuestion");
    assert_eq!(v["hooks"]["PermissionRequest"][0]["hooks"][0]["timeout"], 45);
    assert_eq!(v["hooks"]["PermissionRequest"][0]["hooks"][0]["command"], "curl -s -m 40 -X POST -H \"Content-Type: application/json\" --data-binary @- http://127.0.0.1:47601/hook");
    assert!(f.with_extension("json.bak-hearth").is_file());
    assert!(!hooks::install(h, 47601, &hooks::HookCommand::Curl).unwrap(), "second run adds nothing");
    assert_eq!(serde_json::from_str::<Value>(&std::fs::read_to_string(&f).unwrap()).unwrap()["hooks"]["Stop"].as_array().unwrap().len(), 2);
    assert!(hooks::remove(h).unwrap());
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
    assert_eq!(v["hooks"]["Stop"].as_array().unwrap().len(), 1);
    assert_eq!(v["hooks"]["Stop"][0]["hooks"][0]["command"], "notify-send done");
    assert!(v["hooks"].get("PreToolUse").is_none() && v["hooks"].get("SessionStart").is_some() && v["alpha"]["b"] == 2);
    assert!(!hooks::status(h));
}
#[test]
fn hooks_recognise_the_old_python_hook_and_the_program_form() {
    let t = tmp(); let h = t.path();
    let f = hooks::settings_path(h); std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    std::fs::write(&f, r#"{"hooks": {"Stop": [{"matcher": "", "hooks": [{"type": "command", "command": "python3 /home/x/.local/share/hearth-relay/cm_hook.py", "timeout": 10}]}]}}"#).unwrap();
    hooks::install(h, 47601, &hooks::HookCommand::Program(PathBuf::from("/usr/local/bin/hearth-server"))).unwrap();
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
    assert_eq!(v["hooks"]["Stop"].as_array().unwrap().len(), 1, "the old hook counts as ours");
    assert_eq!(v["hooks"]["PreToolUse"][0]["hooks"][0]["command"], "\"/usr/local/bin/hearth-server\" hook --port 47601");
    assert!(hooks::is_our_hook(&json!({"command": "\"/opt/hearth/hearth-server\" hook --port 1"})));
    assert!(!hooks::is_our_hook(&json!({"command": "echo hearth-server status"})));
    hooks::remove(h).unwrap();
    assert!(serde_json::from_str::<Value>(&std::fs::read_to_string(&f).unwrap()).unwrap().get("hooks").is_none());
}
#[test]
fn hooks_never_overwrite_a_settings_file_they_cannot_read() {
    let t = tmp(); let h = t.path();
    let f = hooks::settings_path(h); std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    std::fs::write(&f, "{ this is not json").unwrap();
    assert!(hooks::install(h, 47601, &hooks::HookCommand::Curl).unwrap_err().to_string().contains("not valid JSON"));
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "{ this is not json");
    let t2 = tmp();   // no file at all: created
    assert!(hooks::install(t2.path(), 47601, &hooks::HookCommand::Curl).unwrap() && hooks::status(t2.path()));
}
#[test]
fn cm_hook_output_rules() {
    let finished = json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": {"behavior": "allow"}}});
    assert_eq!(hooks::hook_output("{}", &finished), Some(finished.clone()));
    let perm = r#"{"hook_event_name":"PermissionRequest"}"#;
    assert_eq!(hooks::hook_output(perm, &json!({"decision": "allow"})).unwrap(), json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": {"behavior": "allow"}}}));
    assert_eq!(hooks::hook_output(perm, &json!({"decision": "deny"})).unwrap()["hookSpecificOutput"]["decision"], json!({"behavior": "deny", "message": "Denied from your phone"}));
    assert_eq!(hooks::hook_output(perm, &json!({"decision": "ask"})), None);
    assert_eq!(hooks::hook_output(r#"{"hook_event_name":"Stop"}"#, &json!({"decision": "allow"})), None);
    assert_eq!(hooks::hook_output("not json", &json!({"decision": "allow"})), None);
    assert_eq!(hooks::call_message(0).0, 2);
    assert_eq!(hooks::call_message(1), (0, "Calling (1 device listening).".into()));
    assert_eq!(hooks::call_message(3).1, "Calling (3 devices listening).");
}

// ───────────── update: download, sha256, atomic replace ─────────────
fn release_routes(version: &str, relay_body: &[u8], relay_sum: &str, server_body: Option<(&[u8], String)>) -> HashMap<String, (u16, Vec<u8>)> {
    let ra = update::relay_asset().unwrap();
    let mut r = HashMap::new();
    r.insert("/api".to_string(), (200, format!("{{\"tag_name\":\"v{version}\"}}").into_bytes()));
    r.insert(format!("/dl/v{version}/hearth-relay.sha256"), (200, format!("{relay_sum}  {ra}\n").into_bytes()));
    r.insert(format!("/dl/v{version}/{ra}"), (200, relay_body.to_vec()));
    if let Some((b, sum)) = server_body {
        let sa = update::server_asset().unwrap();
        r.insert(format!("/dl/v{version}/hearth-server.sha256"), (200, format!("{sum}  {sa}\n").into_bytes()));
        r.insert(format!("/dl/v{version}/{sa}"), (200, b.to_vec()));
    }
    r
}
fn src(port: u16) -> Source { Source { api: format!("http://127.0.0.1:{port}/api"), download: format!("http://127.0.0.1:{port}/dl") } }
fn sh_version(v: &str) -> Vec<u8> { format!("#!/bin/sh\n[ \"$1\" = --version ] && echo {v}\n").into_bytes() }

#[test]
fn update_downloads_verifies_and_replaces_both_programs() {
    if update::arch_suffix().is_none() { return; }
    let t = tmp(); let root = t.path();
    let env = fake_env(root, "alice");
    let dir = root.join("opt/hearth"); std::fs::create_dir_all(&dir).unwrap();
    script(&dir, "hearth-relay", "1.0.0"); script(&dir, "hearth-server", "1.0.0");
    std::fs::write(dir.join("install.json"), r#"{"version":"1.0.0","mode":"direct"}"#).unwrap();
    let (rb, sb) = (sh_version("2.0.0"), sh_version("2.0.0"));
    let port = serve(release_routes("2.0.0", &rb, &update::sha256_hex(&rb), Some((&sb, update::sha256_hex(&sb)))));
    let up = update::update_install(&env, &dir, &src(port), false).unwrap().unwrap();
    assert_eq!((up.from.as_str(), up.to.as_str(), up.relay, up.server), ("1.0.0", "2.0.0", true, true));
    assert_eq!(update::program_version(&dir.join("hearth-relay")).as_deref(), Some("2.0.0"));
    assert_eq!(update::program_version(&dir.join("hearth-server")).as_deref(), Some("2.0.0"));
    assert!(!dir.join("hearth-relay.prev").exists(), "no copy of the old version is kept");
    assert!(!dir.join("hearth-relay.new").exists());
    update::bump_meta(&env, &dir, "2.0.0").unwrap();
    assert_eq!(serde_json::from_str::<Value>(&std::fs::read_to_string(dir.join("install.json")).unwrap()).unwrap()["version"], "2.0.0");
    // nothing newer: nothing done
    assert!(update::update_install(&env, &dir, &src(port), false).unwrap().is_none());
}
#[test]
fn update_rejects_wrong_checksum_and_wrong_version_and_leaves_old_in_place() {
    if update::arch_suffix().is_none() { return; }
    let t = tmp(); let root = t.path();
    let env = fake_env(root, "alice");
    let dir = root.join("opt/hearth"); std::fs::create_dir_all(&dir).unwrap();
    script(&dir, "hearth-relay", "1.0.0");
    let rb = sh_version("2.0.0");
    let port = serve(release_routes("2.0.0", &rb, &"0".repeat(64), None));
    let e = update::update_install(&env, &dir, &src(port), false).unwrap_err();
    assert!(e.to_string().contains("did not match its checksum"), "{e}");
    assert_eq!(update::program_version(&dir.join("hearth-relay")).as_deref(), Some("1.0.0"));
    // checksum right but the program says another version
    let liar = sh_version("1.5.0");
    let port2 = serve(release_routes("2.0.0", &liar, &update::sha256_hex(&liar), None));
    let e = update::update_install(&env, &dir, &src(port2), false).unwrap_err();
    assert!(e.to_string().contains("another version"), "{e}");
    assert_eq!(update::program_version(&dir.join("hearth-relay")).as_deref(), Some("1.0.0"));
    // a release without the installer program still updates the relay
    let port3 = serve(release_routes("2.0.0", &rb, &update::sha256_hex(&rb), None));
    script(&dir, "hearth-server", "1.0.0");
    let up = update::update_install(&env, &dir, &src(port3), false).unwrap().unwrap();
    assert!(up.relay && !up.server);
    // a missing release asset is an error, a bad tag is refused
    let mut r = HashMap::new(); r.insert("/api".to_string(), (200, br#"{"tag_name":"v2.0.0; rm -rf"}"#.to_vec()));
    assert!(update::latest_tag(&src(serve(r))).unwrap_err().to_string().contains("strange name"));
}
#[test]
fn install_downloads_the_release_of_its_own_version_when_no_binary_is_given() {
    if update::arch_suffix().is_none() { return; }
    let t = tmp(); let root = t.path();
    let mut env = fake_env(root, "alice");
    env.prefix = None; env.is_root = true;   // a real folder, no sudo: the folder is made by acquire_relay
    let v = hearth_server::VERSION;
    let rb = sh_version(v);
    let port = serve(release_routes(v, &rb, &update::sha256_hex(&rb), None));
    let dest = root.join("out/hearth-relay");
    install::acquire_relay(&env, &Opts { source: Some(src(port)), ..Default::default() }, &dest).unwrap();
    assert_eq!(update::program_version(&dest).as_deref(), Some(v));
}

#[test]
fn cli_parses_every_command_word() {
    use clap::Parser;
    for args in [vec!["hearth-server"], vec!["hearth-server", "code"], vec!["hearth-server", "link"], vec!["hearth-server", "status"], vec!["hearth-server", "restart"], vec!["hearth-server", "logs"], vec!["hearth-server", "update", "--local"], vec!["hearth-server", "update", "--force"], vec!["hearth-server", "uninstall"],
        vec!["hearth-server", "hooks", "install", "--program"], vec!["hearth-server", "hooks", "remove"], vec!["hearth-server", "voice", "install", "--dutch"], vec!["hearth-server", "local", "install", "--no-systemctl"], vec!["hearth-server", "migrate"], vec!["hearth-server", "call", "hello", "there"],
        vec!["hearth-server", "install", "--yes", "--mode", "docker", "--address", "auto", "--name", "x", "--prefix", "/x", "--dry-run", "--instance", "hearth-bob"]] {
        hearth_server::cli::Cli::try_parse_from(&args).unwrap_or_else(|e| panic!("{args:?}: {e}"));
    }
    assert!(hearth_server::cli::Cli::try_parse_from(["hearth-server", "nonsense"]).is_err());
}
