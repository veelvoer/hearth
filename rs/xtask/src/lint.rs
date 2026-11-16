//! scripts/lint-renderer.js: finds names that are used but never defined (typos only show as a ReferenceError when somebody clicks).
//! eslint is third-party JavaScript and comes through `npx` (needs the network once). The old desktop screens are the only thing it checks.
use anyhow::Result;
use regex::Regex;
use serde_json::{json, Map, Value};
use std::path::Path;
use std::process::{Command, Stdio};

pub const RENDERER_GLOBALS: &str = "document window navigator localStorage console setTimeout clearTimeout setInterval clearInterval requestAnimationFrame cancelAnimationFrame matchMedia getComputedStyle innerWidth innerHeight devicePixelRatio performance Audio AudioContext AudioWorkletNode webkitAudioContext FileReader Blob URL fetch Promise HTMLElement Node Event KeyboardEvent MouseEvent Image ResizeObserver IntersectionObserver MutationObserver AbortController AbortSignal crypto prompt confirm alert location history Notification speechSynthesis SpeechSynthesisUtterance MediaRecorder getSelection DOMParser Intl TextDecoder TextEncoder btoa atob Uint8Array Float32Array Map Set WeakMap requestIdleCallback structuredClone queueMicrotask CSS DOMException Response Headers FormData screen scrollTo open close addEventListener removeEventListener dispatchEvent CustomEvent DocumentFragment SVGElement Element DataTransfer ClipboardEvent DragEvent File Path2D cm CMW md WeekPlan module";
pub const NODE_GLOBALS: &str = "require module exports process __dirname __filename Buffer console setTimeout clearTimeout setInterval clearInterval setImmediate clearImmediate fetch AbortSignal AbortController URL URLSearchParams TextDecoder TextEncoder structuredClone queueMicrotask Promise Intl WebAssembly performance crypto Blob atob btoa global globalThis";

/// The scripts of index.html, in order.
pub fn script_list(html: &str) -> Vec<String> { Regex::new(r#"<script src="([^"]+)""#).unwrap().captures_iter(html).map(|m| m[1].to_string()).collect() }
fn globals(names: &str) -> Value { Value::Object(names.split(' ').map(|n| (n.to_string(), json!("readonly"))).collect::<Map<_, _>>()) }
pub fn renderer_config() -> String { format!("module.exports = [{{ files: ['all.js'], languageOptions: {{ ecmaVersion: 2023, sourceType: 'script', globals: {} }}, rules: {{ 'no-undef': 'error' }} }}];", globals(RENDERER_GLOBALS)) }
pub fn node_config() -> String { format!("module.exports = [{{ languageOptions: {{ ecmaVersion: 2023, sourceType: 'commonjs', globals: {} }}, rules: {{ 'no-undef': 'error' }} }}];", globals(NODE_GLOBALS)) }

fn eslint(cwd: &Path, args: &[&str], cache: &Path) -> Option<i32> {
    let out = Command::new("npx").args(["--yes", "eslint@9"]).args(args).current_dir(cwd).env("npm_config_cache", cache).stdin(Stdio::null()).output().ok()?;
    print!("{}", String::from_utf8_lossy(&out.stdout));
    for l in String::from_utf8_lossy(&out.stderr).lines().filter(|l| !l.contains("npm notice")) { eprintln!("{l}"); }
    out.status.code()
}
pub fn run(root: &Path) -> Result<i32> {
    let dir = root.join("desktop/renderer");
    let html = std::fs::read_to_string(dir.join("index.html"))?;
    let work = std::env::temp_dir().join(format!("hearth-lint-{}", std::process::id()));
    std::fs::create_dir_all(&work)?;
    let mut all = String::new();
    for s in script_list(&html) { all += &format!("// ---- {s}\n{}\n", std::fs::read_to_string(dir.join(&s))?); }
    std::fs::write(work.join("all.js"), all.trim_end_matches('\n'))?;
    std::fs::write(work.join("eslint.config.js"), renderer_config())?;
    let cache = std::env::temp_dir().join("hearth-npm-cache");
    let r = eslint(&work, &["-c", "eslint.config.js", "all.js"], &cache);
    std::fs::write(work.join("eslint.node.js"), node_config())?;
    let mut files: Vec<String> = vec![];
    for d in ["desktop", "desktop/relay", "server", "support", "scripts"] {
        if let Ok(rd) = std::fs::read_dir(root.join(d)) { for f in rd.flatten() { let n = f.file_name().to_string_lossy().to_string(); if n.ends_with(".js") && !n.starts_with("sw") && !n.starts_with("preload-stub") && n != "mcpkit-files.js" && n != "weekplan.js" { files.push(f.path().to_string_lossy().to_string()); } } }
    }
    files.sort();
    let cfg = work.join("eslint.node.js").to_string_lossy().to_string();
    let mut args = vec!["-c", cfg.as_str()]; args.extend(files.iter().map(|s| s.as_str()));
    let r2 = eslint(root, &args, &cache);
    let _ = std::fs::remove_dir_all(&work);
    Ok(if r == Some(0) && r2 == Some(0) { 0 } else { 1 })
}
