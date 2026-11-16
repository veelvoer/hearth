// Pins and names of chats are kept on the server, so the phone and the computer show the same.
use hearth_tests::*;
use serde_json::{json, Value};
use std::fs;

fn arr(v: &Value) -> Vec<Value> { v.as_array().cloned().unwrap_or_default() }

#[test]
fn chattags() {
    let env = Env::new("tag");
    let proj = env.proj.join("app");
    let enc: String = proj.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let pdir = env.home.join(".claude/projects").join(enc);
    fs::create_dir_all(&proj).unwrap(); fs::create_dir_all(&pdir).unwrap();
    let ps = proj.to_string_lossy().to_string();
    let mk = |id: &str, user: &str, reply: &str| fs::write(pdir.join(format!("{id}.jsonl")), [
        json!({"type":"user","cwd":ps,"timestamp":"2026-10-20T10:00:00Z","message":{"content":user}}).to_string(),
        json!({"type":"assistant","cwd":ps,"timestamp":"2026-10-20T10:00:05Z","message":{"role":"assistant","content":[{"type":"text","text":reply}]}}).to_string()].join("\n") + "\n").unwrap();
    mk("one", "fix the login page", "I changed the Pineapple handler in auth.js"); mk("two", "write a poem", "Roses are red");
    let relay = Relay::start(&env, Opts::new().env("HEARTH_MEM_CHILD", "1"));
    let a = relay.api(); let tok = relay.token.clone();
    let mut c = Checks::new();
    let guest = "share.abcd.abcdefghijklmnop";
    let g = |p: &str, t: &str| a.t("GET", p, t, None);
    let p = |b: Value| a.t("POST", "/chat-tags", &tok, Some(b));
    c.ok("1 starts with no tags", g("/chat-tags", &tok).j["tags"].as_object().map(|o| o.is_empty()) == Some(true));
    c.ok("2 a pin is kept", p(json!({"id":"abc","pin":true})).j["tags"]["abc"]["pin"] == true);
    c.ok("3 a name is kept next to the pin", p(json!({"id":"abc","name":"  Groceries "})).j["tags"]["abc"]["name"] == "Groceries" && g("/chat-tags", &tok).j["tags"]["abc"]["pin"] == true);
    c.ok("4 unpinning keeps the name", p(json!({"id":"abc","pin":false})).j["tags"]["abc"]["name"] == "Groceries");
    c.ok("5 an empty name and no pin removes the entry", p(json!({"id":"abc","name":""})).j["tags"].get("abc").is_none());
    c.ok("6 a strange id is refused", p(json!({"id":"../x","pin":true})).code == 400);
    c.ok("7 a guest key cannot read or change tags", g("/chat-tags", guest).code == 401 && a.t("POST", "/chat-tags", guest, Some(json!({"id":"q","pin":true}))).code == 401);
    c.ok("8 no key is refused", g("/chat-tags", "nope").code == 401);
    let f1 = arr(&g("/search?q=pineapple", &tok).j["results"]);
    c.ok_x("9 search finds words inside a chat, in any case", f1.len() == 1 && f1[0]["id"] == "one" && re("Pineapple", &jstr(&f1[0]["snippet"])) && f1[0]["count"] == 1, json!(f1).to_string().chars().take(160).collect::<String>());
    c.ok("10 search says nothing for words that are not there, or too short", arr(&g("/search?q=zzzzqq", &tok).j["results"]).is_empty() && arr(&g("/search?q=p", &tok).j["results"]).is_empty());
    c.ok("11 search matches what you wrote too", arr(&g("/search?q=poem", &tok).j["results"]).iter().map(|x| jstr(&x["id"])).collect::<Vec<_>>().join(",") == "two");
    c.ok("12 a guest key cannot search", g("/search?q=poem", guest).code == 401);
    for i in 0..9 { g("/chat-tags", &format!("wrong{i}")); }
    c.ok("13 after many wrong tokens, a wrong one is locked out but the right one still gets in", g("/chat-tags", "wrong-again").code == 429 && g("/chat-tags", &tok).code == 200);
    c.finish();
}
