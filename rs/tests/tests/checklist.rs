// The owner's checklist of reports.
use hearth_tests::*;
use serde_json::{json, Value};
use std::fs;

fn arr(v: &Value) -> Vec<Value> { v.as_array().cloned().unwrap_or_default() }

#[test]
fn checklist() {
    let env = Env::new("checklist");
    let proj = env.proj.join("app");
    let enc: String = proj.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let pdir = env.home.join(".claude/projects").join(enc);
    fs::create_dir_all(&proj).unwrap(); fs::create_dir_all(&pdir).unwrap();
    let ps = proj.to_string_lossy().to_string();
    let mk = |id: &str, user: &str, reply: &str| fs::write(pdir.join(format!("{id}.jsonl")), [
        json!({"type":"user","cwd":ps,"timestamp":"2026-10-20T10:00:00Z","message":{"content":user}}).to_string(),
        json!({"type":"assistant","cwd":ps,"timestamp":"2026-10-20T10:00:05Z","message":{"role":"assistant","content":[{"type":"text","text":reply}]}}).to_string()].join("\n") + "\n").unwrap();
    mk("one", "fix the login page", "I changed the Pineapple handler in auth.js"); mk("two", "write a poem", "Roses are red");
    env.stub("claude", "#!/bin/sh\nexit 1\n");
    let relay = Relay::start(&env, Opts::new().env("HEARTH_MEM_CHILD", "1"));
    let a = relay.api(); let tok = relay.token.clone();
    let mut c = Checks::new();
    let ck = |b: Value| a.t("POST", "/inbox/checklist", &tok, Some(b));
    let ids = |r: &R| arr(&r.j["items"]).iter().map(|x| jstr(&x["id"])).collect::<Vec<_>>().join(",");
    let now = now_ms();
    let mut r = ck(json!({"sync": [
        {"id":"HRT-1","title":"Crash","email":"a@x.nl","last":"It crashes","updatedAt":now,"from":"user"},
        {"id":"HRT-2","title":"Idea","email":"b@x.nl","last":"More icons","updatedAt":now-5.0,"from":"user"},
        {"id":"HRT-3","title":"Old","email":"c@x.nl","last":"Good news","updatedAt":now-9.0,"from":"support","fixed":true}]}));
    c.ok_x("1 open reports become items, fixed ones are not added", ids(&r) == "HRT-1,HRT-2" && arr(&r.j["items"]).iter().all(|x| x["done"] != true), r.text.chars().take(120).collect::<String>());
    r = ck(json!({"id":"HRT-1","done":true}));
    c.ok("2 ticking moves an item to done, below the open ones", arr(&r.j["items"]).iter().map(|x| format!("{}{}", jstr(&x["id"]), if x["done"] == true { "!" } else { "" })).collect::<Vec<_>>().join(",") == "HRT-2,HRT-1!");
    r = ck(json!({"id":"HRT-1","done":false}));
    c.ok("3 unticking brings it back", arr(&r.j["items"]).iter().all(|x| x["done"] != true));
    let item = |r: &R, id: &str| arr(&r.j["items"]).into_iter().find(|x| x["id"] == id).unwrap_or(json!({}));
    r = ck(json!({"sync": [{"id":"HRT-2","title":"Idea","email":"b@x.nl","last":"Good news: done","updatedAt":now+10.0,"from":"support","fixed":true}]}));
    c.ok("4 a report the desk says is fixed gets ticked", item(&r, "HRT-2")["done"] == true);
    r = ck(json!({"sync": [{"id":"HRT-2","title":"Idea","email":"b@x.nl","last":"Still wrong","updatedAt":now+5000.0,"from":"user"}]}));
    c.ok("5 a new message from the person after it was ticked opens it again", item(&r, "HRT-2")["done"] == false);
    ck(json!({"id":"HRT-1","done":true}));
    r = ck(json!({"remove":"done"}));
    c.ok("6 \"remove done\" removes only the done ones", ids(&r) == "HRT-2");
    r = ck(json!({"remove":"HRT-2"}));
    c.ok("7 one item can be removed", arr(&r.j["items"]).is_empty());
    c.ok("8 a guest key cannot touch the checklist", a.t("POST", "/support/checklist", "share.abcd.abcdefghijklmnop", Some(json!({"remove":"done"}))).code == 401);
    let inc = a.t("POST", "/support/incoming", &tok, Some(json!({"kind":"new","id":"HRT-4242","title":"With a picture","category":"bug","from":"a@x.nl","text":"See the screenshot",
        "files":[{"name":"shot one.jpg","type":"image/jpeg","data":"SlBFR0RBVEE="},{"name":"../evil.sh","data":"eA=="}]})));
    let fd = env.dir.join("support-files/HRT-4242");
    let got: Vec<String> = fs::read_dir(&fd).map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect()).unwrap_or_default();
    c.ok_x("9 pictures that come with a support mail are kept for Claude", inc.code == 200 && got.len() == 2 && got.iter().any(|n| re(r"shot one\.jpg$", n)) && !got.iter().any(|n| n.contains("..")), got.join(","));
    fs::write(env.dir.join("pending-runs.json"), json!({"lost-run-1": {"cwd": ps, "text": "make me a license plate counter web app", "at": now_ms() - 60000.0}}).to_string()).unwrap();
    let lst = a.t("GET", "/sessions", &tok, None).j;
    let lost = arr(&lst).into_iter().find(|x| x["id"] == "lost-run-1");
    c.ok("10 a run that never reached the disk still shows in the list, marked as interrupted", lost.as_ref().map(|l| l["interrupted"] == true && re("license plate", &jstr(&l["title"]))).unwrap_or(false));
    let lm = arr(&a.t("GET", "/sessions/lost-run-1/messages", &tok, None).j);
    c.ok("11 opening it shows the prompt and says what happened", lm.len() >= 2 && lm[0]["role"] == "user" && re("license plate", &jstr(&lm[0]["text"])) && re("interrupted", &jstr(&lm[1]["text"])));
    c.ok("12 deleting it works and it does not come back", a.t("POST", "/sessions/lost-run-1/delete", &tok, Some(json!({}))).code == 200 && !arr(&a.t("GET", "/sessions", &tok, None).j).iter().any(|x| x["id"] == "lost-run-1"));
    // a Claude that quits before it reads the prompt must not take the server down
    a.t("POST", "/sessions/new/send", &tok, Some(json!({"text": "x".repeat(400000), "mode": "acceptEdits", "cwd": ps})));
    sleep(800);
    c.ok("13 the server is still up after a run that died at once", a.t("GET", "/health", &tok, None).code == 200 && a.t("GET", "/chat-tags", &tok, None).code != 0);
    let lg = a.t("GET", "/admin/logs?tail=5", &tok, None);
    c.ok("14 the server log can be read by its owner, with the version", lg.code == 200 && lg.j["lines"].is_array() && re(r"^\d+\.\d+\.\d+", &jstr(&lg.j["version"])));
    c.ok("15 a guest key cannot read the log", a.t("GET", "/admin/logs", "share.abcd.abcdefghijklmnop", None).code == 401);
    let dr = a.t("GET", "/admin/doctor", &tok, None);
    let checks = arr(&dr.j["checks"]);
    c.ok_x("16 the health check lists what works and what does not", dr.code == 200 && !checks.is_empty() && checks.iter().any(|x| x["name"] == "Projects folder" && x["ok"] == true) && checks.iter().any(|x| x["name"] == "Claude Code starts" && x["ok"] == false && !x["fix"].is_null()),
        json!(checks.iter().map(|x| format!("{}:{}", jstr(&x["name"]), x["ok"])).collect::<Vec<_>>()).to_string().chars().take(200).collect::<String>());
    c.finish();
}
