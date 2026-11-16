//! Running things and writing files: honours --prefix (a folder instead of the real system), --dry-run and sudo.
use crate::ui::*;
use anyhow::{anyhow, bail, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone)]
pub struct Env {
    pub prefix: Option<PathBuf>,
    pub dry: bool,
    pub yes: bool,
    pub is_root: bool,
    /// The person this install is for (SUDO_USER, else the login name).
    pub user: String,
    /// This program's own file, copied next to the relay (tests point it at a fake).
    pub self_exe: PathBuf,
}

#[derive(Default, Clone)]
pub struct RunOpts { pub sudo: bool, pub cwd: Option<PathBuf>, pub env: Vec<(String, String)> }
pub struct Ran { pub ok: bool, pub out: String }

pub fn current_user() -> String {
    for k in ["SUDO_USER", "USER", "LOGNAME"] { if let Ok(v) = std::env::var(k) { if !v.is_empty() { return v; } } }
    Command::new("id").arg("-un").output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| "hearth".into())
}
pub fn is_root() -> bool {
    // SAFETY: geteuid has no arguments and cannot fail.
    unsafe { libc::geteuid() == 0 }
}
/// Is `bin` on PATH?
pub fn have(bin: &str) -> bool { which(bin).is_some() }
pub fn which(bin: &str) -> Option<PathBuf> {
    if bin.contains('/') { return Path::new(bin).is_file().then(|| PathBuf::from(bin)); }
    std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join(bin)).find(|f| f.is_file()))
}

impl Env {
    pub fn real() -> Env {
        let root = is_root();
        Env { prefix: None, dry: false, yes: false, is_root: root, user: current_user(), self_exe: std::env::current_exe().unwrap_or_else(|_| PathBuf::from("hearth-server")) }
    }
    /// A path on the real system, or inside the --prefix folder.
    pub fn sys(&self, p: &str) -> PathBuf { match &self.prefix { Some(x) => x.join(p.trim_start_matches('/')), None => PathBuf::from(p) } }
    pub fn opt(&self) -> PathBuf { self.sys("/opt") }
    pub fn sudo_prefix(&self) -> Vec<String> { if self.is_root || self.prefix.is_some() { vec![] } else { vec!["sudo".into(), "-n".into()] } }
    /// The program runs for real (not a test folder, not a dry run).
    pub fn live(&self) -> bool { self.prefix.is_none() && !self.dry }

    /// Runs a command with a little spinner. Never panics.
    pub fn run(&self, label: &str, command: &str, args: &[&str], o: &RunOpts) -> Ran {
        if self.dry { println!("  {}", dim(&format!("[dry run] {command} {}", args.join(" ")))); return Ran { ok: true, out: String::new() }; }
        let mut full: Vec<String> = if o.sudo { self.sudo_prefix() } else { vec![] };
        full.push(command.into()); full.extend(args.iter().map(|s| s.to_string()));
        let mut cmd = Command::new(&full[0]);
        cmd.args(&full[1..]).env("DEBIAN_FRONTEND", "noninteractive").stdin(Stdio::null());
        for (k, v) in &o.env { cmd.env(k, v); }
        if let Some(d) = &o.cwd { cmd.current_dir(d); }
        if !tty() { println!("  … {label}"); }
        match cmd.output() {
            Ok(r) => { let mut out = String::from_utf8_lossy(&r.stdout).to_string(); out.push_str(&String::from_utf8_lossy(&r.stderr)); Ran { ok: r.status.success(), out: out.trim().to_string() } }
            Err(e) => Ran { ok: false, out: e.to_string() },
        }
    }
    /// Like run, but a failure is an error (after showing the last lines of what the command said).
    pub fn must(&self, label: &str, command: &str, args: &[&str], o: &RunOpts) -> Result<Ran> {
        let r = self.run(label, command, args, o);
        if r.ok { ok(label); Ok(r) } else {
            bad(&format!("{label} did not work."));
            let tail: Vec<&str> = r.out.lines().rev().take(6).collect::<Vec<_>>().into_iter().rev().collect();
            println!("{}", dim(&wrap(&tail.join("\n"), "     ", width().saturating_sub(8))));
            Err(anyhow!("{label} failed"))
        }
    }
    /// Writes a file as root when needed (through `sudo install`), or into the prefix folder.
    pub fn write(&self, file: &Path, text: &str, mode: u32) -> Result<()> {
        if self.dry { println!("  {}", dim(&format!("[dry run] write {}", file.display()))); return Ok(()); }
        if self.prefix.is_some() || self.is_root {
            if let Some(d) = file.parent() { std::fs::create_dir_all(d)?; }
            std::fs::write(file, text)?; set_mode(file, mode)?;
            return Ok(());
        }
        let tmp = std::env::temp_dir().join(format!("hearth-{}-{}", std::process::id(), text.len()));
        std::fs::write(&tmp, text)?;
        let r = self.run(&format!("write {}", file.display()), "install", &["-D", "-m", &format!("{mode:o}"), &tmp.to_string_lossy(), &file.to_string_lossy()], &RunOpts { sudo: true, ..Default::default() });
        let _ = std::fs::remove_file(&tmp);
        if !r.ok { bail!("could not write {}: {}", file.display(), r.out); }
        Ok(())
    }
    /// Copies a file (a program) into place the same way.
    pub fn copy_file(&self, from: &Path, to: &Path, mode: u32) -> Result<()> {
        if self.dry { println!("  {}", dim(&format!("[dry run] copy {} to {}", from.display(), to.display()))); return Ok(()); }
        if self.prefix.is_some() || self.is_root {
            if let Some(d) = to.parent() { std::fs::create_dir_all(d)?; }
            let tmp = to.with_file_name(format!("{}.new", to.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
            std::fs::copy(from, &tmp)?; set_mode(&tmp, mode)?; std::fs::rename(&tmp, to)?;
            return Ok(());
        }
        let staged = to.with_file_name(format!("{}.new", to.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
        let o = RunOpts { sudo: true, ..Default::default() };
        let r = self.run(&format!("copy {}", to.display()), "install", &["-D", "-m", &format!("{mode:o}"), &from.to_string_lossy(), &staged.to_string_lossy()], &o);
        if !r.ok { bail!("could not copy to {}: {}", to.display(), r.out); }
        let r = self.run(&format!("put {} in place", to.display()), "mv", &["-f", &staged.to_string_lossy(), &to.to_string_lossy()], &o);
        if !r.ok { bail!("could not replace {}: {}", to.display(), r.out); }
        Ok(())
    }
    /// mkdir -p (with sudo when needed).
    pub fn mkdirs(&self, dirs: &[&Path]) -> Result<()> {
        if self.dry { for d in dirs { println!("  {}", dim(&format!("[dry run] mkdir -p {}", d.display()))); } return Ok(()); }
        if self.prefix.is_some() || self.is_root { for d in dirs { std::fs::create_dir_all(d)?; } return Ok(()); }
        let a: Vec<String> = dirs.iter().map(|d| d.to_string_lossy().to_string()).collect();
        let mut args = vec!["-p"]; args.extend(a.iter().map(|s| s.as_str()));
        let r = self.run("make folders", "mkdir", &args, &RunOpts { sudo: true, ..Default::default() });
        if !r.ok { bail!("could not make folders: {}", r.out); }
        Ok(())
    }
    /// chown, best effort, only where it can matter (real system as root or sudo).
    pub fn chown(&self, user: &str, paths: &[&Path]) {
        if self.prefix.is_some() || self.dry { return; }
        let a: Vec<String> = paths.iter().map(|d| d.to_string_lossy().to_string()).collect();
        let mut args = vec!["-R", user]; args.extend(a.iter().map(|s| s.as_str()));
        let _ = self.run("set owner", "chown", &args, &RunOpts { sudo: true, ..Default::default() });
    }
    /// rm -rf, natively in a test folder or as root, else through sudo.
    pub fn remove(&self, paths: &[&Path]) {
        if self.dry { for p in paths { println!("  {}", dim(&format!("[dry run] rm -rf {}", p.display()))); } return; }
        if self.prefix.is_some() || self.is_root {
            for p in paths { if p.is_dir() { let _ = std::fs::remove_dir_all(p); } else { let _ = std::fs::remove_file(p); } }
            return;
        }
        let a: Vec<String> = paths.iter().map(|d| d.to_string_lossy().to_string()).collect();
        let mut args = vec!["-rf"]; args.extend(a.iter().map(|s| s.as_str()));
        let _ = self.run("remove files", "rm", &args, &RunOpts { sudo: true, ..Default::default() });
    }
    pub fn sudo_cat(&self, file: &Path) -> String {
        let mut full = self.sudo_prefix(); full.push("cat".into()); full.push(file.to_string_lossy().to_string());
        Command::new(&full[0]).args(&full[1..]).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default()
    }
    pub fn sudo_test_file(&self, file: &Path) -> bool {
        if self.prefix.is_some() { return std::fs::metadata(file).map(|m| m.len() > 0).unwrap_or(false); }
        let mut full = self.sudo_prefix(); full.extend(["test".into(), "-s".into(), file.to_string_lossy().to_string()]);
        Command::new(&full[0]).args(&full[1..]).status().map(|s| s.success()).unwrap_or(false)
    }
}

pub fn set_mode(f: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(f, std::fs::Permissions::from_mode(mode))?;
    Ok(())
}

impl Env {
    /// For things inside the user's own home (no sudo, no prefix): files are written directly.
    pub fn user_level(dry: bool) -> Env {
        Env { prefix: None, dry, yes: true, is_root: true, user: current_user(), self_exe: std::env::current_exe().unwrap_or_else(|_| PathBuf::from("hearth-server")) }
    }
}

/// Recursive copy that keeps symlinks and permissions (cp -a).
pub fn copy_dir_all(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?; let src = e.path(); let dst = to.join(e.file_name());
        let ft = e.file_type()?;
        if ft.is_symlink() { let t = std::fs::read_link(&src)?; let _ = std::fs::remove_file(&dst); std::os::unix::fs::symlink(t, &dst)?; }
        else if ft.is_dir() { copy_dir_all(&src, &dst)?; }
        else { std::fs::copy(&src, &dst)?; }
    }
    Ok(())
}
