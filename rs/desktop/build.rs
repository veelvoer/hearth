//! The patch notes of every version go inside the program (the portable Windows file has no folder of its own); and the Slint files.
use std::{fs, path::Path};

fn main() {
    let mut notes = String::from("pub const NOTES: &[(&str, &str)] = &[\n");
    let ok = |n: &str| { let v: Vec<&str> = n.trim_end_matches(".md").split('.').collect(); n.ends_with(".md") && v.len() == 3 && v.iter().all(|x| !x.is_empty() && x.chars().all(|c| c.is_ascii_digit())) };
    let dir = fs::canonicalize("../../release-notes").unwrap_or_default();
    for e in fs::read_dir(&dir).into_iter().flatten().flatten() {
        let n = e.file_name().to_string_lossy().to_string();
        if ok(&n) { notes += &format!("    ({:?}, include_str!({:?})),\n", n.trim_end_matches(".md"), e.path().to_string_lossy()); }
    }
    notes += "];\n";
    let _ = fs::write(Path::new(&std::env::var("OUT_DIR").unwrap()).join("notes.rs"), notes);
    println!("cargo:rerun-if-changed=../../release-notes");
    slint_build::compile("ui/main.slint").expect("slint compile");
}
