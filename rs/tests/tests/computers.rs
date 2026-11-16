// Two of your computers linked to one server: choose where a chat runs.
use hearth_tests::*;
use serde_json::{json, Value};

fn arr(v: &Value) -> Vec<Value> { v.as_array().cloned().unwrap_or_default() }

#[test]
fn computers() {
    let env = Env::new("computers");
    std::fs::create_dir_all(env.proj.join("app")).unwrap();
    let relay = Relay::start(&env, Opts::new());
    let a = relay.api(); let tok = relay.token.clone();
    let mut c = Checks::new();
    let cwd = env.proj.join("app").to_string_lossy().to_string();
    let _p1 = a.tstream("POST", "/agent/poll", &tok, Some(json!({"name": "Laptop", "epoch": 0})));
    let _p2 = a.tstream("POST", "/agent/poll", &tok, Some(json!({"name": "Desktop", "epoch": 0}))); sleep(700);
    let list = a.t("GET", "/computers", &tok, None).j;
    let l = arr(&list);
    c.ok_x("1 the server lists both computers and itself", !l.is_empty() && l[0]["kind"] == "server" && l.iter().any(|c| c["id"] == "pc:Laptop" && c["online"] == true) && l.iter().any(|c| c["id"] == "pc:Desktop" && c["online"] == true), json!(l.iter().map(|c| c["id"].clone()).collect::<Vec<_>>()).to_string());
    let out = a.t("POST", "/sessions/new/send", &tok, Some(json!({"text": "make a todo app", "cwd": cwd, "run": "pc:Desktop", "mode": "auto"}))).text;
    c.ok_x("2 a new chat can be started on one computer (queued for it)", out.contains(r#""t":"queued""#) && out.contains(r#""to":"Desktop""#), out.chars().take(120).collect::<String>());
    let aa = a.t("POST", "/agent/poll", &tok, Some(json!({"name": "Laptop", "epoch": 0}))).j;
    c.ok("3 the other computer gets nothing", arr(&aa["work"]).is_empty());
    let b = a.t("POST", "/agent/poll", &tok, Some(json!({"name": "Desktop", "epoch": 0}))).j;
    let w = arr(&b["work"]);
    c.ok_x("4 the chosen computer gets the work, as a new chat in the right folder", w.len() == 1 && w[0]["newChat"] == true && w[0]["rel"] == "app" && w[0]["target"] == "Desktop", json!(w.iter().map(|w| json!([w["rel"], w["target"]])).collect::<Vec<_>>()).to_string());
    let bad = a.t("POST", "/sessions/new/send", &tok, Some(json!({"text": "x", "cwd": "/etc", "run": "pc:Desktop"}))).text;
    c.ok("5 a folder outside the projects folder is refused", bad.contains("error"));
    c.finish();
}
