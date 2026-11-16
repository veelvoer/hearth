// Deleting a chat: it goes to a trash folder, disappears from the list, can be restored; nothing else is touched.
use hearth_tests::*;
use serde_json::{json, Value};
use std::fs;

fn arr(v: &Value) -> Vec<Value> { v.as_array().cloned().unwrap_or_default() }

#[test]
fn delete_chats() {
    let env = Env::new("del");
    let proj = env.proj.join("app");
    let enc: String = proj.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let pdir = env.home.join(".claude/projects").join(enc);
    fs::create_dir_all(&proj).unwrap(); fs::create_dir_all(&pdir).unwrap();
    let proj_s = proj.to_string_lossy().to_string();
    let mk = |id: &str, text: &str| fs::write(pdir.join(format!("{id}.jsonl")), json!({"type":"user","cwd":proj_s,"timestamp":"2026-10-20T10:00:00Z","message":{"content":text}}).to_string() + "\n").unwrap();
    mk("keep", "keep me"); mk("gone", "delete me"); mk("gone2", "delete me too");
    let relay = Relay::start(&env, Opts::new());
    let a = relay.api(); let tok = relay.token.clone();
    let mut c = Checks::new();
    let ids = || { let mut v: Vec<String> = arr(&a.t("GET", "/sessions", &tok, None).j).iter().map(|x| jstr(&x["id"])).collect(); v.sort(); v.join(",") };
    let guest = "share.abcd.abcdefghijklmnop";
    c.ok("1 three chats to begin with", ids() == "gone,gone2,keep");
    c.ok("2 a guest key cannot delete", a.t("POST", "/sessions/gone/delete", guest, Some(json!({}))).code == 401);
    c.ok("3 deleting works", a.t("POST", "/sessions/gone/delete", &tok, Some(json!({}))).code == 200 && ids() == "gone2,keep");
    let trash = env.home.join(".claude/projects/.hearth-trash");
    c.ok("4 the file is in the trash, not erased", fs::read_dir(&trash).map(|d| d.flatten().any(|e| e.file_name().to_string_lossy().ends_with("__gone.jsonl"))).unwrap_or(false) && !pdir.join("gone.jsonl").exists());
    c.ok("5 the trash folder is not shown as a project or chat", !arr(&a.t("GET", "/sessions", &tok, None).j).iter().any(|x| x["id"] == "gone"));
    c.ok("6 a second delete says it is gone", a.t("POST", "/sessions/gone/delete", &tok, Some(json!({}))).code == 404);
    c.ok("7 undo brings it back where it was", a.t("POST", "/sessions/gone/restore", &tok, Some(json!({}))).code == 200 && ids() == "gone,gone2,keep" && pdir.join("gone.jsonl").exists());
    c.ok("8 restoring something that is not in the trash says so", a.t("POST", "/sessions/nothing/restore", &tok, Some(json!({}))).code == 404);
    c.ok("9 a strange id cannot reach other files", a.t("POST", "/sessions/..%2F..%2Fx/delete", &tok, Some(json!({}))).code == 404);
    // whole projects
    fs::write(proj.join("main.js"), "console.log(1)").unwrap(); mk("keep", "keep me");
    c.ok("10 a guest key cannot delete a project", a.t("POST", "/projects/delete", guest, Some(json!({"cwd": proj_s}))).code == 401);
    c.ok("11 the projects folder itself and outside folders cannot be deleted", a.t("POST", "/projects/delete", &tok, Some(json!({"cwd": env.proj.to_string_lossy()}))).code == 403 && a.t("POST", "/projects/delete", &tok, Some(json!({"cwd": "/tmp"}))).code == 403);
    let dp = a.t("POST", "/projects/delete", &tok, Some(json!({"cwd": proj_s})));
    c.ok("12 deleting a project moves its folder and its chats to the trash", dp.code == 200 && dp.j["chats"] == 3 && !proj.exists() && !pdir.exists() && ids().is_empty());
    let tr = arr(&a.t("GET", "/projects/trash", &tok, None).j);
    c.ok("13 the trash lists it, and the trash is not a project", tr.len() == 1 && tr[0]["name"] == "app" && !arr(&a.t("GET", "/projects", &tok, None).j["projects"]).iter().any(|x| jstr(&x["name"]).starts_with('.')));
    c.ok("14 restoring brings back files and chats", a.t("POST", "/projects/restore", &tok, Some(json!({"stem": tr[0]["stem"]}))).code == 200 && proj.join("main.js").exists() && ids() == "gone,gone2,keep");
    c.ok("15 a restore of something unknown is refused", a.t("POST", "/projects/restore", &tok, Some(json!({"stem": "1__nothing"}))).code == 404 && a.t("POST", "/projects/restore", &tok, Some(json!({"stem": "../../etc"}))).code == 400);
    c.finish();
}
