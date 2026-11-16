//! This computer: colors and dark mode of the desktop, starting with the computer, Claude Code's hooks, opening files and terminals.
use hearth_relay::util::{home, read_json, J};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

pub fn platform() -> &'static str { if cfg!(windows) { "win32" } else if cfg!(target_os = "macos") { "darwin" } else { "linux" } }

/// A command that never flashes a console window on Windows.
pub fn cmd(prog: &str) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(prog);
    #[cfg(windows)] { use std::os::windows::process::CommandExt; c.creation_flags(0x0800_0000); }
    c
}

/// Runs a small command and gives its output ("" when it fails or takes longer than 1.5 s).
pub async fn run(prog: &str, args: &[&str]) -> String {
    let mut c = tokio::process::Command::from(cmd(prog));
    c.args(args).stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null()).kill_on_drop(true);
    match tokio::time::timeout(std::time::Duration::from_millis(1500), c.output()).await {
        Ok(Ok(o)) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => String::new(),
    }
}

fn hex2(n: f64) -> String { format!("{:02x}", n.round().clamp(0.0, 255.0) as u8) }
static ACCENT: Mutex<(f64, Option<String>)> = Mutex::new((0.0, None));

/// The accent color of this desktop. Windows says it directly; on Linux every desktop does it differently, so the places are asked in turn.
pub async fn accent() -> Option<String> {
    { let c = ACCENT.lock().unwrap(); if hearth_relay::util::now_ms() - c.0 < 15000.0 { return c.1.clone(); } }
    let mut v: Option<String> = None;
    if cfg!(windows) {
        let o = run("reg", &["query", r"HKCU\Software\Microsoft\Windows\DWM", "/v", "AccentColor"]).await;
        if let Some(m) = regex::Regex::new(r"0x([0-9a-fA-F]{1,8})").unwrap().captures(&o) {
            if let Ok(n) = u32::from_str_radix(&m[1], 16) { v = Some(format!("#{:02x}{:02x}{:02x}", n & 0xff, (n >> 8) & 0xff, (n >> 16) & 0xff)); }
        }
    } else {
        // 1. the standard desktop portal (GNOME 47+, KDE 6, most others)
        let o = run("gdbus", &["call", "--session", "--dest", "org.freedesktop.portal.Desktop", "--object-path", "/org/freedesktop/portal/desktop", "--method", "org.freedesktop.portal.Settings.Read", "org.freedesktop.appearance", "accent-color"]).await;
        if let Some(m) = regex::Regex::new(r"\(\s*(-?[\d.e-]+),\s*(-?[\d.e-]+),\s*(-?[\d.e-]+)\s*\)").unwrap().captures(&o) {
            let f = |i: usize| m[i].parse::<f64>().unwrap_or(-1.0);
            if f(1) >= 0.0 { v = Some(format!("#{}{}{}", hex2(f(1).min(1.0) * 255.0), hex2(f(2).min(1.0) * 255.0), hex2(f(3).min(1.0) * 255.0))); }
        }
        // 2. KDE Plasma
        if v.is_none() {
            if let Ok(t) = std::fs::read_to_string(home().join(".config/kdeglobals")) {
                if let Some(m) = regex::Regex::new(r"(?m)^AccentColor=(\d+),(\d+),(\d+)").unwrap().captures(&t) { v = Some(format!("#{}{}{}", hex2(m[1].parse().unwrap_or(0.0)), hex2(m[2].parse().unwrap_or(0.0)), hex2(m[3].parse().unwrap_or(0.0)))); }
            }
        }
        // 3. Hyprland: the color of the focused window border is the closest thing it has
        if v.is_none() && std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
            let hj = run("hyprctl", &["getoption", "general:col.active_border", "-j"]).await;
            if let Some(m) = regex::Regex::new(r#"(?i)"gradient":\s*"([0-9a-f]{8})"#).unwrap().captures(&hj) { v = Some(format!("#{}", &m[1][2..])); }
        }
        // 4. GNOME's own setting (only on GNOME-like desktops: elsewhere it always says "blue")
        let gnome = regex::Regex::new(r"(?i)gnome|unity|budgie|cinnamon|pantheon|ubuntu").unwrap().is_match(&std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default());
        if v.is_none() && gnome {
            let g = run("gsettings", &["get", "org.gnome.desktop.interface", "accent-color"]).await.replace(['\'', ' ', '\n'], "");
            v = match g.as_str() { "blue" => Some("#3584e4"), "teal" => Some("#2190a4"), "green" => Some("#3a944a"), "yellow" => Some("#c88800"), "orange" => Some("#ed5b00"), "red" => Some("#e62d42"), "pink" => Some("#d56199"), "purple" => Some("#9141ac"), "slate" => Some("#6f8396"), _ => None }.map(String::from);
        }
        // 5. the GTK theme's own selection / accent color
        if v.is_none() {
            let re = regex::Regex::new(r"@define-color\s+(?:accent_bg_color|accent_color|theme_selected_bg_color)\s+(#[0-9a-fA-F]{6})").unwrap();
            for f in ["gtk-4.0/gtk.css", "gtk-3.0/colors.css", "gtk-3.0/gtk.css", "gtk-4.0/colors.css"] {
                if let Ok(t) = std::fs::read_to_string(home().join(".config").join(f)) { if let Some(m) = re.captures(&t) { v = Some(m[1].to_lowercase()); break; } }
            }
        }
    }
    *ACCENT.lock().unwrap() = (hearth_relay::util::now_ms(), v.clone());
    v
}

/// Dark or light, for desktops that do not tell the window. None = unknown.
pub async fn dark() -> Option<bool> {
    if cfg!(windows) {
        let o = run("reg", &["query", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "/v", "AppsUseLightTheme"]).await;
        if o.contains("0x0") { return Some(true); }
        if o.contains("0x1") { return Some(false); }
        return None;
    }
    if cfg!(target_os = "macos") { return None; }
    let o = run("gdbus", &["call", "--session", "--dest", "org.freedesktop.portal.Desktop", "--object-path", "/org/freedesktop/portal/desktop", "--method", "org.freedesktop.portal.Settings.Read", "org.freedesktop.appearance", "color-scheme"]).await;
    if let Some(m) = regex::Regex::new(r"uint32 (\d)").unwrap().captures(&o) { match &m[1] { "1" => return Some(true), "2" => return Some(false), _ => {} } }
    let cs = run("gsettings", &["get", "org.gnome.desktop.interface", "color-scheme"]).await.replace(['\'', ' ', '\n'], "");
    if cs == "prefer-dark" { return Some(true); }
    if cs == "prefer-light" { return Some(false); }
    let th = run("gsettings", &["get", "org.gnome.desktop.interface", "gtk-theme"]).await.to_lowercase();
    if !th.trim().is_empty() {
        if regex::Regex::new("dark|black|night").unwrap().is_match(&th) { return Some(true); }
        if regex::Regex::new("light|white").unwrap().is_match(&th) { return Some(false); }
    }
    if let Ok(t) = std::fs::read_to_string(home().join(".config/gtk-3.0/settings.ini")) {
        if let Some(m) = regex::Regex::new(r"(?i)gtk-application-prefer-dark-theme\s*=\s*(\w+)").unwrap().captures(&t) { return Some(regex::Regex::new("(?i)1|true").unwrap().is_match(&m[1])); }
        if let Some(m) = regex::Regex::new(r"(?i)gtk-theme-name\s*=\s*(.+)").unwrap().captures(&t) { return Some(regex::Regex::new("(?i)dark|black|night").unwrap().is_match(&m[1])); }
    }
    None
}

/// The program to start again (an AppImage is started through its file, not the unpacked copy that runs).
pub fn launcher() -> String {
    std::env::var("APPIMAGE").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| std::env::current_exe().map(|p| p.to_string_lossy().to_string()).unwrap_or_default())
}

pub fn set_autostart(on: bool, icon: &str) -> Result<(), String> {
    if cfg!(windows) {
        let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
        let st = if on { cmd("reg").args(["add", key, "/v", "Hearth", "/t", "REG_SZ", "/d", &format!("\"{}\" --hidden", launcher()), "/f"]).status() } else { cmd("reg").args(["delete", key, "/v", "Hearth", "/f"]).status() };
        return st.map(|_| ()).map_err(|e| e.to_string());
    }
    let f = home().join(".config/autostart/hearth.desktop");
    if !on { let _ = std::fs::remove_file(&f); return Ok(()); }
    let _ = std::fs::create_dir_all(f.parent().unwrap());
    std::fs::write(&f, format!("[Desktop Entry]\nType=Application\nName=Hearth\nExec=\"{}\" --hidden\nIcon={}\nTerminal=false\n", launcher(), icon)).map_err(|e| e.to_string())
}

// ── Claude Code's hooks: Claude tells this app when it is done, has a question or needs permission ──
const HOOK_DEFS: [(&str, &str, u32); 3] = [("Stop", "", 10), ("PreToolUse", "AskUserQuestion", 10), ("PermissionRequest", "", 45)];
fn claude_settings() -> PathBuf { home().join(".claude").join("settings.json") }
fn is_our_hook(h: &Value) -> bool { let c = h.s("command"); regex::Regex::new(r"127\.0\.0\.1:\d+/hook").unwrap().is_match(&c) || c.contains("cm_hook.py") }
fn read_settings() -> Value { read_json(&claude_settings()).filter(|v| v.is_object()).unwrap_or_else(|| json!({})) }
fn write_settings(v: &Value) -> Result<(), String> {
    let f = claude_settings();
    let _ = std::fs::create_dir_all(f.parent().unwrap());
    std::fs::write(&f, serde_json::to_string_pretty(v).unwrap_or_default() + "\n").map_err(|e| e.to_string())
}
fn has_ours(groups: &Value) -> bool { groups.as_array().map(|g| g.iter().any(|g| g.g("hooks").as_array().map(|h| h.iter().any(is_our_hook)).unwrap_or(false))).unwrap_or(false) }
pub fn hooks_status() -> bool { let s = read_settings(); HOOK_DEFS.iter().all(|(ev, _, _)| has_ours(s.g("hooks").g(ev))) }
pub fn hooks_install(port: u16) -> Result<bool, String> {
    let mut data = read_settings();
    let f = claude_settings();
    if f.exists() { let _ = std::fs::copy(&f, f.with_extension("json.bak-hearth")); }
    let cmdline = format!("curl -s -m 40 -X POST -H \"Content-Type: application/json\" --data-binary @- http://127.0.0.1:{port}/hook");
    if !data.g("hooks").is_object() { data["hooks"] = json!({}); }
    for (ev, matcher, timeout) in HOOK_DEFS {
        if !data["hooks"].g(ev).is_array() { data["hooks"][ev] = json!([]); }
        if !has_ours(&data["hooks"][ev]) { data["hooks"][ev].as_array_mut().unwrap().push(json!({"matcher": matcher, "hooks": [{"type": "command", "command": cmdline, "timeout": timeout}]})); }
    }
    write_settings(&data)?;
    Ok(true)
}
pub fn hooks_remove() -> Result<bool, String> {
    let mut data = read_settings();
    if let Some(hooks) = data.get_mut("hooks").and_then(|h| h.as_object_mut()) {
        let keys: Vec<String> = hooks.keys().cloned().collect();
        for ev in keys {
            let groups: Vec<Value> = hooks[&ev].as_array().cloned().unwrap_or_default().into_iter().map(|mut g| {
                let kept: Vec<Value> = g.g("hooks").as_array().cloned().unwrap_or_default().into_iter().filter(|x| !is_our_hook(x)).collect();
                g["hooks"] = Value::Array(kept); g
            }).filter(|g| g.g("hooks").as_array().map(|a| !a.is_empty()).unwrap_or(false)).collect();
            if groups.is_empty() { hooks.remove(&ev); } else { hooks.insert(ev, Value::Array(groups)); }
        }
        if hooks.is_empty() { data.as_object_mut().unwrap().remove("hooks"); }
    }
    write_settings(&data)?;
    Ok(true)
}

/// Opens a file, folder or web page with the program the desktop uses for it.
pub fn open(target: &str) {
    let _ = if cfg!(windows) { cmd("explorer").arg(target).spawn() }
        else if cfg!(target_os = "macos") { cmd("open").arg(target).spawn() }
        else { cmd("xdg-open").arg(target).spawn() };
}

/// A terminal in the chat's folder, running `claude --resume <id>`: the same Claude Code session, outside Hearth.
pub fn terminal(cwd: &str, sid: &str) -> Value {
    if !regex::Regex::new(r"^[A-Za-z0-9-]{6,80}$").unwrap().is_match(sid) { return json!({"error": "That is not a chat id."}); }
    if !std::path::Path::new(cwd).is_dir() { return json!({"error": "That project folder is not on this computer."}); }
    let q = |x: &str| format!("'{}'", x.replace('\'', "'\\''"));
    let r = if cfg!(windows) {
        #[cfg(windows)] { use std::os::windows::process::CommandExt; Command::new("cmd.exe").raw_arg(format!("/c start \"Claude Code\" cmd.exe /k \"cd /d \"{cwd}\" && claude --resume {sid}\"")).spawn() }
        #[cfg(not(windows))] { Command::new("true").spawn() }
    } else if cfg!(target_os = "macos") {
        Command::new("osascript").args(["-e", &format!("tell application \"Terminal\" to do script {}", serde_json::to_string(&format!("cd {} && claude --resume {sid}", q(cwd))).unwrap()), "-e", "tell application \"Terminal\" to activate"]).spawn()
    } else {
        let inner = format!("cd {} && claude --resume {sid}; exec \"${{SHELL:-bash}}\"", q(cwd));
        let pick = "for t in x-terminal-emulator kgx gnome-terminal konsole kitty alacritty foot wezterm ptyxis xfce4-terminal tilix xterm; do command -v \"$t\" >/dev/null 2>&1 || continue; case \"$t\" in kgx|gnome-terminal|ptyxis) exec \"$t\" -- bash -lc \"$0\";; kitty|foot) exec \"$t\" bash -lc \"$0\";; wezterm) exec wezterm start -- bash -lc \"$0\";; xfce4-terminal|tilix) exec \"$t\" -x bash -lc \"$0\";; *) exec \"$t\" -e bash -lc \"$0\";; esac; done; exit 9";
        if std::env::var("FLATPAK_ID").is_ok() { Command::new("flatpak-spawn").args(["--host", "sh", "-c", pick, &inner]).spawn() } else { Command::new("sh").args(["-c", pick, &inner]).spawn() }
    };
    match r { Ok(_) => json!({"ok": true}), Err(e) => json!({"error": format!("No terminal could be opened: {e}")}) }
}

/// "Linux 6.9.1", "Windows_NT 10.0.22631": the way Node's os.type() + os.release() said it.
pub fn os_text() -> String {
    if cfg!(windows) {
        let v = cmd("cmd").args(["/c", "ver"]).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
        let n = regex::Regex::new(r"(\d+\.\d+\.\d+)").unwrap().captures(&v).map(|m| m[1].to_string()).unwrap_or_default();
        format!("Windows_NT {n}")
    } else if cfg!(target_os = "macos") { "Darwin".into() } else { format!("Linux {}", std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default().trim()) }
}
