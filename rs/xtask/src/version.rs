//! scripts/set-version.js and the release workflow's `check` job.
use anyhow::{anyhow, bail, Result};
use regex::{NoExpand, Regex};
use std::fs;
use std::path::{Path, PathBuf};

/// The project root: the nearest folder above (or at) the current one that has rs/Cargo.toml.
pub fn find_root() -> Result<PathBuf> {
    let mut cur = std::env::current_dir()?;
    loop {
        if cur.join("rs/Cargo.toml").exists() { return Ok(cur); }
        if !cur.pop() { bail!("Could not find the project root (looking for rs/Cargo.toml)"); }
    }
}

/// The version now: desktop/relay/version.js, or (when that file is gone) the workspace version in rs/Cargo.toml.
pub fn current_version(root: &Path) -> Result<String> {
    if let Ok(t) = fs::read_to_string(root.join("desktop/relay/version.js")) {
        if let Some(c) = Regex::new(r"'(\d+\.\d+\.\d+)'")?.captures(&t) { return Ok(c[1].to_string()); }
    }
    let t = fs::read_to_string(root.join("rs/Cargo.toml"))?;
    Regex::new(r#"(?m)^version = "(\d+\.\d+\.\d+)""#)?.captures(&t).map(|c| c[1].to_string()).ok_or_else(|| anyhow!("Could not find the current version"))
}

/// feature | fix | major | X.Y.Z  ->  the new version (None: not a valid spec).
pub fn next_version(current: &str, spec: &str) -> Option<String> {
    let p: Vec<u64> = current.split('.').map(|n| n.parse().unwrap_or(0)).collect();
    let (a, b, c) = (*p.first().unwrap_or(&0), *p.get(1).unwrap_or(&0), *p.get(2).unwrap_or(&0));
    let v = match spec { "feature" => format!("{a}.{}.0", b + 1), "fix" => format!("{a}.{b}.{}", c + 1), "major" => format!("{}.0.0", a + 1), s => s.to_string() };
    Regex::new(r"^\d+\.\d+\.\d+$").unwrap().is_match(&v).then_some(v)
}
pub fn version_code(v: &str) -> u64 { let p: Vec<u64> = v.split('.').map(|n| n.parse().unwrap_or(0)).collect(); p[0] * 10000 + p[1] * 100 + p[2] }

/// Replaces the first match only (like a JS replace without the g flag).
fn first(re: &str, s: &str, with: &str) -> String { Regex::new(re).unwrap().replace(s, NoExpand(with)).to_string() }
fn edit(root: &Path, rel: &str, f: impl Fn(&str) -> String) -> Result<()> {
    let p = root.join(rel);
    let t = fs::read_to_string(&p).map_err(|e| anyhow!("{}: {e}", p.display()))?;
    fs::write(&p, f(&t))?;
    Ok(())
}
fn gradle(s: &str, v: &str) -> String {
    let s = first(r"versionCode = \d+", s, &format!("versionCode = {}", version_code(v)));
    first(r#"versionName = "[^"]+""#, &s, &format!("versionName = \"{v}\""))
}

/// Sets the version in every file. The text written to the files is exactly what scripts/set-version.js writes
/// (including its comment in version.js); rs/Cargo.toml (the workspace version) is also kept in step.
pub fn set_version(root: &Path, spec: &str) -> Result<String> {
    let current = current_version(root)?;
    let Some(v) = next_version(&current, spec) else { bail!("Usage: cargo xtask set-version feature | fix | major | 1.2.3   (now {current})") };
    edit(root, "desktop/package.json", |s| first(r#""version": "[^"]+""#, s, &format!("\"version\": \"{v}\"")))?;
    let vjs = root.join("desktop/relay/version.js");
    if vjs.exists() || root.join("desktop/relay").is_dir() {
        fs::write(&vjs, format!("'use strict';\n/* The version of this Hearth. Set by scripts/set-version.js. */\nmodule.exports = '{v}';\n"))?;
    }
    edit(root, "desktop/relay-rs/Cargo.toml", |s| first(r#"(?m)^version = "[^"]+""#, s, &format!("version = \"{v}\"")))?;
    edit(root, "desktop/tauri/Cargo.toml", |s| first(r#"(?m)^version = "[^"]+""#, s, &format!("version = \"{v}\"")))?;
    edit(root, "app/build.gradle.kts", |s| gradle(s, &v))?;
    edit(root, "wear/build.gradle.kts", |s| gradle(s, &v))?;
    if root.join("rs/Cargo.toml").exists() { edit(root, "rs/Cargo.toml", |s| first(r#"(?m)^version = "[^"]+""#, s, &format!("version = \"{v}\"")))?; }
    let notes = root.join("release-notes").join(format!("{v}.md"));
    if !notes.exists() {
        fs::create_dir_all(notes.parent().unwrap())?;
        fs::write(&notes, format!("## What is new\n\n- TODO: write the notes for {v} (what is new or fixed, in plain words)\n"))?;
        println!("Made release-notes/{v}.md: fill it in before you tag.");
    }
    println!("Version is now {v}");
    Ok(v)
}

fn file(root: &Path, rel: &str) -> Option<String> { fs::read_to_string(root.join(rel)).ok() }
fn toml_has(s: &str, v: &str) -> bool { s.lines().any(|l| l == format!("version = \"{v}\"") || l.starts_with(&format!("version = \"{v}\""))) }

/// The release workflow's check, as a list of problems (empty = fine). `tag` like v1.41.0; None: the version now.
pub fn check_release(root: &Path, tag: Option<&str>) -> Result<Vec<String>> {
    let v = match tag { Some(t) => t.trim_start_matches('v').to_string(), None => current_version(root)? };
    if !Regex::new(r"^\d+\.\d+\.\d+$").unwrap().is_match(&v) { bail!("{v} is not a version like 1.2.3"); }
    let mut e: Vec<String> = vec![];
    let notes = format!("release-notes/{v}.md");
    match file(root, &notes) {
        Some(t) if !t.trim().is_empty() => { if t.contains("TODO") { e.push(format!("{notes} still says TODO")); } }
        _ => e.push(format!("{notes} is missing: every release needs notes")),
    }
    match file(root, "desktop/package.json").and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).and_then(|j| j["version"].as_str().map(String::from)) {
        Some(x) if x == v => {}
        _ => e.push("desktop/package.json is not ".to_string() + &v),
    }
    if root.join("desktop/relay/version.js").exists() {
        if file(root, "desktop/relay/version.js").and_then(|t| Regex::new(r"module\.exports = '([^']+)'").unwrap().captures(&t).map(|c| c[1].to_string())).as_deref() != Some(v.as_str()) { e.push(format!("desktop/relay/version.js is not {v}")); }
    }
    for f in ["desktop/relay-rs/Cargo.toml", "desktop/tauri/Cargo.toml", "rs/Cargo.toml"] {
        if !file(root, f).map(|t| toml_has(&t, &v)).unwrap_or(false) { e.push(format!("{f} is not {v}")); }
    }
    for f in ["app/build.gradle.kts", "wear/build.gradle.kts"] {
        let t = file(root, f).unwrap_or_default();
        if !t.contains(&format!("versionName = \"{v}\"")) { e.push(format!("{f} is not {v}")); }
        else if !t.contains(&format!("versionCode = {}", version_code(&v))) { e.push(format!("{f} has the right versionName but versionCode is not {}", version_code(&v))); }
    }
    Ok(e)
}
