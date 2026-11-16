//! Puts the renderer (desktop/renderer) and the one relay script it loads into web/, the folder the app serves its pages from.
use std::{fs, path::Path};

fn copy_dir(from: &Path, to: &Path) {
    let _ = fs::create_dir_all(to);
    for e in fs::read_dir(from).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.contains(".conflict") || name.contains("sync-conflict") || name.ends_with('~') { continue; }
        let p = e.path();
        if p.is_dir() { copy_dir(&p, &to.join(&name)); } else { let _ = fs::copy(&p, to.join(&name)); }
    }
}

fn main() {
    let web = Path::new("web");
    let _ = fs::remove_dir_all(web);
    copy_dir(Path::new("../renderer"), web);
    let _ = fs::create_dir_all(web.join("relay"));
    let _ = fs::copy("../relay/weekplan.js", web.join("relay/weekplan.js"));
    // index.html loads ../relay/weekplan.js: from the root of the app that is relay/weekplan.js; the app's own calls must pass the page's CSP
    for f in ["index.html", "call.html"] {
        let p = web.join(f);
        if let Ok(t) = fs::read_to_string(&p) {
            let t = t.replace("../relay/weekplan.js", "relay/weekplan.js").replace("connect-src 'self'", "connect-src 'self' ipc: http://ipc.localhost");
            let _ = fs::write(&p, t);
        }
    }
    // the patch notes of every version go inside the program (the portable Windows file has no folder of its own)
    let mut notes = String::from("pub const NOTES: &[(&str, &str)] = &[\n");
    let re_ok = |n: &str| { let v: Vec<&str> = n.trim_end_matches(".md").split('.').collect(); n.ends_with(".md") && v.len() == 3 && v.iter().all(|x| !x.is_empty() && x.chars().all(|c| c.is_ascii_digit())) };
    let dir = fs::canonicalize("../../release-notes").unwrap_or_default();
    for e in fs::read_dir(&dir).into_iter().flatten().flatten() {
        let n = e.file_name().to_string_lossy().to_string();
        if re_ok(&n) { notes += &format!("    ({:?}, include_str!({:?})),\n", n.trim_end_matches(".md"), e.path().to_string_lossy()); }
    }
    notes += "];\n";
    let _ = fs::write(Path::new(&std::env::var("OUT_DIR").unwrap()).join("notes.rs"), notes);
    println!("cargo:rerun-if-changed=../../release-notes");
    println!("cargo:rerun-if-changed=../renderer");
    println!("cargo:rerun-if-changed=../relay/weekplan.js");
    println!("cargo:rerun-if-changed=src/shim.js");
    tauri_build::build()
}
