//! This computer: colors and dark mode of the desktop, autostart, Claude Code's hooks, opening files and terminals.
use crate::color::{Palette, Rgb};
use hearth_relay::util::{home, read_json, J};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{LazyLock, Mutex};

macro_rules! re { ($p:expr) => {{ static R: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new($p).unwrap()); &*R }}; }

pub fn platform() -> &'static str { if cfg!(windows) { "win32" } else if cfg!(target_os = "macos") { "darwin" } else { "linux" } }

/// `$XDG_CONFIG_HOME` or `~/.config`.
pub fn config_home() -> PathBuf { std::env::var("XDG_CONFIG_HOME").ok().filter(|s| !s.is_empty()).map(PathBuf::from).unwrap_or_else(|| home().join(".config")) }

/// A command that never flashes a console window on Windows.
pub fn cmd(prog: &str) -> Command {
    #[allow(unused_mut)]
    let mut c = hearth_relay::util::cmd(prog);
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

// ───────────────────────── colors ─────────────────────────
static ACCENT: Mutex<(f64, Option<Rgb>)> = Mutex::new((0.0, None));

/// The Oxide window manager keeps its whole theme in one file: background, surfaces, text, accent, light or dark.
pub fn oxide_palette() -> Option<Palette> {
    if !cfg!(target_os = "linux") { return None; }
    let f = config_home().join("oxide/theme.json");
    if !std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_lowercase().contains("oxide") && !f.exists() { return None; }
    Palette::from_json(&read_json(&f)?)
}

/// `(0.2, 0.4, 0.8)` from `gdbus call ... accent-color` (r, g, b in 0..1).
pub fn parse_portal_accent(o: &str) -> Option<Rgb> {
    let m = re!(r"\(\s*(-?[\d.e-]+),\s*(-?[\d.e-]+),\s*(-?[\d.e-]+)\s*\)").captures(o)?;
    let f = |i: usize| m[i].parse::<f64>().unwrap_or(-1.0);
    if f(1) < 0.0 || f(2) < 0.0 || f(3) < 0.0 { return None; }
    let c = |x: f64| (x.min(1.0) * 255.0).round() as u8;
    Some(Rgb(c(f(1)), c(f(2)), c(f(3))))
}
/// `AccentColor=61,174,233` in kdeglobals.
pub fn parse_kdeglobals_accent(t: &str) -> Option<Rgb> {
    let m = re!(r"(?m)^AccentColor=(\d+),(\d+),(\d+)").captures(t)?;
    let c = |i: usize| m[i].parse::<u16>().unwrap_or(0).min(255) as u8;
    Some(Rgb(c(1), c(2), c(3)))
}
/// Hyprland's `getoption general:col.active_border -j`.
pub fn parse_hypr_border(j: &str) -> Option<Rgb> { let m = re!(r#"(?i)"gradient":\s*"([0-9a-f]{8})"#).captures(j)?; Rgb::parse(&format!("#{}", &m[1][2..])) }
pub fn gnome_accent(name: &str) -> Option<Rgb> {
    Rgb::parse(match name { "blue" => "#3584e4", "teal" => "#2190a4", "green" => "#3a944a", "yellow" => "#c88800", "orange" => "#ed5b00", "red" => "#e62d42", "pink" => "#d56199", "purple" => "#9141ac", "slate" => "#6f8396", _ => return None })
}
/// `@define-color accent_bg_color #3584e4;` in a GTK css file.
pub fn parse_gtk_css_accent(t: &str) -> Option<Rgb> { Rgb::parse(&re!(r"@define-color\s+(?:accent_bg_color|accent_color|theme_selected_bg_color)\s+(#[0-9a-fA-F]{6})").captures(t)?[1]) }

/// The accent color of this desktop. Windows says it directly; on Linux every desktop does it differently, so the places are asked in turn.
pub async fn accent() -> Option<Rgb> {
    { let c = ACCENT.lock().unwrap(); if hearth_relay::util::now_ms() - c.0 < 15000.0 { return c.1; } }
    let v = accent_uncached().await;
    *ACCENT.lock().unwrap() = (hearth_relay::util::now_ms(), v);
    v
}
async fn accent_uncached() -> Option<Rgb> {
    if let Some(p) = oxide_palette() { return Some(p.accent); }
    if cfg!(windows) {
        let o = run("reg", &["query", r"HKCU\Software\Microsoft\Windows\DWM", "/v", "AccentColor"]).await;
        let m = re!(r"0x([0-9a-fA-F]{1,8})").captures(&o)?;
        let n = u32::from_str_radix(&m[1], 16).ok()?;   // ABGR
        return Some(Rgb(n as u8, (n >> 8) as u8, (n >> 16) as u8));
    }
    // 1. the standard desktop portal (GNOME 47+, KDE 6, most others)
    let o = run("gdbus", &["call", "--session", "--dest", "org.freedesktop.portal.Desktop", "--object-path", "/org/freedesktop/portal/desktop", "--method", "org.freedesktop.portal.Settings.Read", "org.freedesktop.appearance", "accent-color"]).await;
    if let Some(c) = parse_portal_accent(&o) { return Some(c); }
    // 2. KDE Plasma
    if let Ok(t) = std::fs::read_to_string(config_home().join("kdeglobals")) { if let Some(c) = parse_kdeglobals_accent(&t) { return Some(c); } }
    // 3. Hyprland: the color of the focused window border is the closest thing it has
    if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
        let hj = run("hyprctl", &["getoption", "general:col.active_border", "-j"]).await;
        if let Some(c) = parse_hypr_border(&hj) { return Some(c); }
    }
    // 4. GNOME's own setting (only on GNOME-like desktops: elsewhere it always says "blue")
    if re!(r"(?i)gnome|unity|budgie|cinnamon|pantheon|ubuntu").is_match(&std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default()) {
        let g = run("gsettings", &["get", "org.gnome.desktop.interface", "accent-color"]).await.replace(['\'', ' ', '\n'], "");
        if let Some(c) = gnome_accent(&g) { return Some(c); }
    }
    // 5. the GTK theme's own selection / accent color
    for f in ["gtk-4.0/gtk.css", "gtk-3.0/colors.css", "gtk-3.0/gtk.css", "gtk-4.0/colors.css"] {
        if let Ok(t) = std::fs::read_to_string(config_home().join(f)) { if let Some(c) = parse_gtk_css_accent(&t) { return Some(c); } }
    }
    None
}

/// `org.freedesktop.appearance color-scheme` answer: 1 dark, 2 light, 0 no preference.
pub fn parse_portal_scheme(o: &str) -> Option<bool> { match &re!(r"uint32 (\d)").captures(o)?[1] { "1" => Some(true), "2" => Some(false), _ => None } }
pub fn dark_from_theme_name(n: &str) -> Option<bool> {
    let n = n.to_lowercase();
    if re!("dark|black|night").is_match(&n) { Some(true) } else if re!("light|white").is_match(&n) { Some(false) } else { None }
}
/// gtk-3.0/settings.ini
pub fn parse_gtk_settings_dark(t: &str) -> Option<bool> {
    if let Some(m) = re!(r"(?i)gtk-application-prefer-dark-theme\s*=\s*(\w+)").captures(t) { return Some(re!("(?i)^(1|true)$").is_match(&m[1])); }
    if let Some(m) = re!(r"(?i)gtk-theme-name\s*=\s*(.+)").captures(t) { return Some(re!("(?i)dark|black|night").is_match(&m[1])); }
    None
}

/// Dark or light, for desktops that do not tell the window. None = unknown.
pub async fn dark() -> Option<bool> {
    if cfg!(windows) {
        let o = run("reg", &["query", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "/v", "AppsUseLightTheme"]).await;
        return if o.contains("0x0") { Some(true) } else if o.contains("0x1") { Some(false) } else { None };
    }
    if cfg!(target_os = "macos") { return None; }
    if let Some(p) = oxide_palette() { return Some(!p.light); }
    let o = run("gdbus", &["call", "--session", "--dest", "org.freedesktop.portal.Desktop", "--object-path", "/org/freedesktop/portal/desktop", "--method", "org.freedesktop.portal.Settings.Read", "org.freedesktop.appearance", "color-scheme"]).await;
    if let Some(d) = parse_portal_scheme(&o) { return Some(d); }
    match run("gsettings", &["get", "org.gnome.desktop.interface", "color-scheme"]).await.replace(['\'', ' ', '\n'], "").as_str() { "prefer-dark" => return Some(true), "prefer-light" => return Some(false), _ => {} }
    let th = run("gsettings", &["get", "org.gnome.desktop.interface", "gtk-theme"]).await;
    if let Some(d) = dark_from_theme_name(&th) { return Some(d); }
    if let Ok(t) = std::fs::read_to_string(config_home().join("gtk-3.0/settings.ini")) { if let Some(d) = parse_gtk_settings_dark(&t) { return Some(d); } }
    None
}

// ───────────────────────── autostart ─────────────────────────
/// The program to start again (an AppImage is started through its file, not the unpacked copy that runs).
pub fn launcher() -> String {
    std::env::var("APPIMAGE").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| std::env::current_exe().map(|p| p.to_string_lossy().to_string()).unwrap_or_default())
}
pub fn autostart_file() -> PathBuf { config_home().join("autostart/hearth.desktop") }
pub fn autostart_entry(exe: &str, icon: &str) -> String { format!("[Desktop Entry]\nType=Application\nName=Hearth\nExec=\"{exe}\" --hidden\nIcon={icon}\nTerminal=false\n") }
pub fn autostart_enabled() -> bool { if cfg!(windows) { false } else { autostart_file().exists() } }

/// Start with the computer, hidden: a .desktop file in the autostart folder (Linux), the Run key (Windows).
pub fn set_autostart(on: bool, icon: &str) -> Result<(), String> {
    if cfg!(windows) {
        let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
        let st = if on { cmd("reg").args(["add", key, "/v", "Hearth", "/t", "REG_SZ", "/d", &format!("\"{}\" --hidden", launcher()), "/f"]).status() } else { cmd("reg").args(["delete", key, "/v", "Hearth", "/f"]).status() };
        return st.map(|_| ()).map_err(|e| e.to_string());
    }
    let f = autostart_file();
    if !on { let _ = std::fs::remove_file(&f); return Ok(()); }
    let _ = std::fs::create_dir_all(f.parent().unwrap());
    std::fs::write(&f, autostart_entry(&launcher(), icon)).map_err(|e| e.to_string())
}

// ── Claude Code's hooks: Claude tells this app when it is done, has a question or needs permission ──
const HOOK_DEFS: [(&str, &str, u32); 3] = [("Stop", "", 10), ("PreToolUse", "AskUserQuestion", 10), ("PermissionRequest", "", 45)];
fn claude_settings() -> PathBuf { home().join(".claude").join("settings.json") }
fn is_our_hook(h: &Value) -> bool { let c = h.s("command"); re!(r"127\.0\.0\.1:\d+/hook").is_match(&c) || c.contains("cm_hook.py") }
fn has_ours(groups: &Value) -> bool { groups.as_array().map(|g| g.iter().any(|g| g.g("hooks").as_array().map(|h| h.iter().any(is_our_hook)).unwrap_or(false))).unwrap_or(false) }
fn read_settings(f: &Path) -> Value { read_json(f).filter(|v| v.is_object()).unwrap_or_else(|| json!({})) }
fn write_settings(f: &Path, v: &Value) -> Result<(), String> {
    let _ = std::fs::create_dir_all(f.parent().unwrap());
    std::fs::write(f, serde_json::to_string_pretty(v).unwrap_or_default() + "\n").map_err(|e| e.to_string())
}
pub fn hooks_status() -> bool { hooks_status_in(&claude_settings()) }
pub fn hooks_install(port: u16) -> Result<bool, String> { hooks_install_in(&claude_settings(), port) }
pub fn hooks_remove() -> Result<bool, String> { hooks_remove_in(&claude_settings()) }

pub fn hooks_status_in(f: &Path) -> bool { let s = read_settings(f); HOOK_DEFS.iter().all(|(ev, _, _)| has_ours(s.g("hooks").g(ev))) }
pub fn hooks_install_in(f: &Path, port: u16) -> Result<bool, String> {
    let mut data = read_settings(f);
    if f.exists() { let _ = std::fs::copy(f, f.with_extension("json.bak-hearth")); }
    let cmdline = format!("curl -s -m 40 -X POST -H \"Content-Type: application/json\" --data-binary @- http://127.0.0.1:{port}/hook");
    if !data.g("hooks").is_object() { data["hooks"] = json!({}); }
    for (ev, matcher, timeout) in HOOK_DEFS {
        if !data["hooks"].g(ev).is_array() { data["hooks"][ev] = json!([]); }
        if !has_ours(&data["hooks"][ev]) { data["hooks"][ev].as_array_mut().unwrap().push(json!({"matcher": matcher, "hooks": [{"type": "command", "command": cmdline, "timeout": timeout}]})); }
    }
    write_settings(f, &data)?;
    Ok(true)
}
pub fn hooks_remove_in(f: &Path) -> Result<bool, String> {
    let mut data = read_settings(f);
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
    write_settings(f, &data)?;
    Ok(true)
}

// ───────────────────────── open, terminal ─────────────────────────
/// Opens a file, folder or web page with the program the desktop uses for it.
pub fn open(target: &str) {
    let _ = if cfg!(windows) { cmd("explorer").arg(target).spawn() }
        else if cfg!(target_os = "macos") { cmd("open").arg(target).spawn() }
        else { cmd("xdg-open").arg(target).spawn() };
}

pub fn valid_session_id(sid: &str) -> bool { re!(r"^[A-Za-z0-9-]{6,80}$").is_match(sid) }
/// The shell line a terminal runs: go to the folder, resume the chat, keep the shell open.
pub fn terminal_line(cwd: &str, sid: &str) -> String {
    let q = |x: &str| format!("'{}'", x.replace('\'', "'\\''"));
    format!("cd {} && claude --resume {sid}; exec \"${{SHELL:-bash}}\"", q(cwd))
}

/// A terminal in the chat's folder, running `claude --resume <id>`: the same Claude Code session, outside Hearth.
pub fn terminal(cwd: &str, sid: &str) -> Result<(), String> {
    if !valid_session_id(sid) { return Err("That is not a chat id.".into()); }
    if !Path::new(cwd).is_dir() { return Err("That project folder is not on this computer.".into()); }
    let q = |x: &str| format!("'{}'", x.replace('\'', "'\\''"));
    let r = if cfg!(windows) {
        #[cfg(windows)] { use std::os::windows::process::CommandExt; cmd("cmd.exe").raw_arg(format!("/c start \"Claude Code\" cmd.exe /k \"cd /d \"{cwd}\" && claude --resume {sid}\"")).spawn() }
        #[cfg(not(windows))] { cmd("true").spawn() }
    } else if cfg!(target_os = "macos") {
        cmd("osascript").args(["-e", &format!("tell application \"Terminal\" to do script {}", serde_json::to_string(&format!("cd {} && claude --resume {sid}", q(cwd))).unwrap()), "-e", "tell application \"Terminal\" to activate"]).spawn()
    } else {
        let inner = terminal_line(cwd, sid);
        let pick = "for t in x-terminal-emulator kgx gnome-terminal konsole kitty alacritty foot wezterm ptyxis xfce4-terminal tilix xterm; do command -v \"$t\" >/dev/null 2>&1 || continue; case \"$t\" in kgx|gnome-terminal|ptyxis) exec \"$t\" -- bash -lc \"$0\";; kitty|foot) exec \"$t\" bash -lc \"$0\";; wezterm) exec wezterm start -- bash -lc \"$0\";; xfce4-terminal|tilix) exec \"$t\" -x bash -lc \"$0\";; *) exec \"$t\" -e bash -lc \"$0\";; esac; done; exit 9";
        if std::env::var("FLATPAK_ID").is_ok() { cmd("flatpak-spawn").args(["--host", "sh", "-c", pick, &inner]).spawn() } else { cmd("sh").args(["-c", pick, &inner]).spawn() }
    };
    r.map(|_| ()).map_err(|e| format!("No terminal could be opened: {e}"))
}

/// "Linux 6.9.1", "Windows_NT 10.0.22631": the way Node's os.type() + os.release() said it.
pub fn os_text() -> String {
    if cfg!(windows) {
        let v = cmd("cmd").args(["/c", "ver"]).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
        let n = re!(r"(\d+\.\d+\.\d+)").captures(&v).map(|m| m[1].to_string()).unwrap_or_default();
        format!("Windows_NT {n}")
    } else if cfg!(target_os = "macos") { "Darwin".into() } else { format!("Linux {}", std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default().trim()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn portal_accent() { assert_eq!(parse_portal_accent("(<(0.2078, 0.5176, 0.8941)>,)"), Some(Rgb(53, 132, 228))); assert_eq!(parse_portal_accent("(<(-1.0, -1.0, -1.0)>,)"), None); assert_eq!(parse_portal_accent(""), None); }
    #[test] fn kde_accent() { assert_eq!(parse_kdeglobals_accent("[General]\nAccentColor=61,174,233\n"), Some(Rgb(61, 174, 233))); assert_eq!(parse_kdeglobals_accent("[General]\n"), None); }
    #[test] fn hypr() { assert_eq!(parse_hypr_border(r#"{"option":"general:col.active_border","custom":"0","gradient": "ff33ccff"}"#), Some(Rgb(0x33, 0xcc, 0xff))); }
    #[test] fn gnome() { assert_eq!(gnome_accent("purple"), Some(Rgb(0x91, 0x41, 0xac))); assert_eq!(gnome_accent("nope"), None); }
    #[test] fn gtk_css() { assert_eq!(parse_gtk_css_accent("@define-color accent_bg_color #FF0000;\n"), Some(Rgb(255, 0, 0))); }
    #[test] fn schemes() {
        assert_eq!(parse_portal_scheme("(<<uint32 1>>,)"), Some(true)); assert_eq!(parse_portal_scheme("(<<uint32 2>>,)"), Some(false)); assert_eq!(parse_portal_scheme("(<<uint32 0>>,)"), None);
        assert_eq!(dark_from_theme_name("'Adwaita-dark'"), Some(true)); assert_eq!(dark_from_theme_name("'Breeze-Light'"), Some(false)); assert_eq!(dark_from_theme_name("''"), None);
        assert_eq!(parse_gtk_settings_dark("[Settings]\ngtk-application-prefer-dark-theme=1\n"), Some(true));
        assert_eq!(parse_gtk_settings_dark("[Settings]\ngtk-application-prefer-dark-theme=0\ngtk-theme-name=Foo-dark\n"), Some(false));
        assert_eq!(parse_gtk_settings_dark("[Settings]\ngtk-theme-name=Nord-Dark\n"), Some(true));
    }
    #[test] fn autostart_text() { let t = autostart_entry("/opt/Hearth.AppImage", "/x/icon.png"); assert!(t.contains("Exec=\"/opt/Hearth.AppImage\" --hidden") && t.contains("Icon=/x/icon.png")); }
    #[test] fn terminal_quoting() { let l = terminal_line("/home/a/it's", "abc-123456"); assert!(l.starts_with("cd '/home/a/it'\\''s' && claude --resume abc-123456;")); assert!(!valid_session_id("a;rm -rf")); assert!(valid_session_id("0123abcd-ef")); }
    #[test] fn hooks_roundtrip() {
        let d = tempfile::tempdir().unwrap(); let f = d.path().join(".claude/settings.json");
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, r#"{"model":"x","hooks":{"Stop":[{"matcher":"","hooks":[{"type":"command","command":"echo mine"}]}]}}"#).unwrap();
        assert!(!hooks_status_in(&f));
        hooks_install_in(&f, 47601).unwrap(); assert!(hooks_status_in(&f));
        hooks_install_in(&f, 47601).unwrap();   // twice: no duplicates
        assert_eq!(read_settings(&f).g("hooks").g("Stop").as_array().unwrap().len(), 2);
        assert!(f.with_extension("json.bak-hearth").exists());
        hooks_remove_in(&f).unwrap(); assert!(!hooks_status_in(&f));
        let s = read_settings(&f); assert_eq!(s.s("model"), "x"); assert_eq!(s.g("hooks").g("Stop").as_array().unwrap().len(), 1); assert!(s.g("hooks").get("PermissionRequest").is_none());
    }
    #[test] fn oxide_file() {
        let d = tempfile::tempdir().unwrap(); std::fs::create_dir_all(d.path().join("oxide")).unwrap();
        std::fs::write(d.path().join("oxide/theme.json"), r##"{"name":"Ember","bg":"#1b1a19","text":"#ece7e1","accent":"#ff7a45","surface":"#232120","light":false,"bad":"#ff5555"}"##).unwrap();
        let p = Palette::from_json(&read_json(&d.path().join("oxide/theme.json")).unwrap()).unwrap();
        assert_eq!(p.name, "Ember"); assert_eq!(p.tokens().card, Rgb(0x23, 0x21, 0x20)); assert_eq!(p.tokens().error, Rgb(255, 0x55, 0x55));
    }
}
