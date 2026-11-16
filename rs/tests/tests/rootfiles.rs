// A chat file that the server's user cannot read (made by a run with root rights) is noticed, so it can be given back.
// Runs the relay's own reader in this process with HOME set to a scratch folder (this file holds one test only).
use hearth_tests::*;
use serde_json::json;
use std::fs;
use std::os::unix::fs::PermissionsExt;

#[test]
fn rootfiles() {
    if unsafe { libc::getuid() } == 0 { println!("skipped: needs a normal Linux user"); return; }
    let env = Env::new("rootfiles");
    std::env::set_var("HOME", &env.t);
    let dir = env.t.join(".claude/projects/-proj-a"); fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("ok.jsonl"), json!({"type":"user","cwd":"/proj/a","message":{"content":"hi"}}).to_string() + "\n").unwrap();
    fs::write(dir.join("locked.jsonl"), "x\n").unwrap(); fs::set_permissions(dir.join("locked.jsonl"), fs::Permissions::from_mode(0)).unwrap();
    let locked_dir = env.t.join(".claude/projects/-proj-b"); fs::create_dir_all(&locked_dir).unwrap(); fs::write(locked_dir.join("in.jsonl"), "x\n").unwrap(); fs::set_permissions(&locked_dir, fs::Permissions::from_mode(0)).unwrap();
    let mut c = Checks::new();
    let files = hearth_relay::core::list_files();
    c.ok("1 a readable chat is listed, the locked ones are skipped", files.len() == 1 && files[0].id == "ok");
    c.ok_x("2 the locked file and the locked folder are both counted", hearth_relay::core::denied_count() == 2, hearth_relay::core::denied_count().to_string());
    fs::set_permissions(&locked_dir, fs::Permissions::from_mode(0o755)).unwrap(); fs::set_permissions(dir.join("locked.jsonl"), fs::Permissions::from_mode(0o644)).unwrap();
    c.ok("3 once readable again they are listed", hearth_relay::core::list_files().len() == 3);
    c.finish();
}
