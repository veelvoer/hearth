// One odd chat file must never keep the other chats from loading.
use hearth_tests::*;
use serde_json::{json, Value};
use std::fs;
use std::io::Write;

fn line(o: Value) -> String { o.to_string() + "\n" }
fn arr(v: &Value) -> Vec<Value> { v.as_array().cloned().unwrap_or_default() }

#[test]
fn odd_chats() {
    let env = Env::new("odd");
    let proj = env.proj.join("a"); let p = env.home.join(".claude/projects");
    fs::create_dir_all(&proj).unwrap(); for d in ["-a", "-b", "-c"] { fs::create_dir_all(p.join(d)).unwrap(); }
    let ps = proj.to_string_lossy().to_string();
    fs::write(p.join("-a/good.jsonl"), line(json!({"type":"user","cwd":ps,"timestamp":"2026-10-19T10:00:00Z","message":{"content":"a good chat"}}))).unwrap();
    fs::write(p.join("-a/empty.jsonl"), "").unwrap();
    let mut seed: u64 = 88172645463325252; let bin: Vec<u8> = (0..3000).map(|_| { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; (seed >> 24) as u8 }).collect();
    fs::write(p.join("-a/binary.jsonl"), bin).unwrap();
    fs::write(p.join("-b/numcwd.jsonl"), line(json!({"type":"user","cwd":5,"timestamp":"nope","message":{"content":null}})) + &line(json!({"type":"user","cwd":ps,"message":{"content":"chat with a number as folder"}})) + "not json\n[]\nnull\n").unwrap();
    fs::create_dir_all(p.join("-c/dir.jsonl")).unwrap(); std::os::unix::fs::symlink("/nonexistent", p.join("-c/dead.jsonl")).unwrap();
    // a huge chat (bigger than one look reads) must not stall the list
    {
        let row = |b: usize, i: usize| line(json!({"type":"assistant","cwd":ps,"timestamp":"2026-10-19T10:00:01Z","message":{"id":format!("m{b}_{i}"),"model":"claude-opus-4","role":"assistant","usage":{"input_tokens":10,"output_tokens":20},"content":[{"type":"text","text":"x".repeat(600)}]}}));
        let mut f = fs::File::create(p.join("-a/huge.jsonl")).unwrap();
        f.write_all(line(json!({"type":"user","cwd":ps,"timestamp":"2026-10-19T09:00:00Z","message":{"content":"a huge chat"}})).as_bytes()).unwrap();
        for b in 0..20 { let mut blk = String::new(); for i in 0..10000 { blk.push_str(&row(b, i)); } f.write_all(blk.as_bytes()).unwrap(); }   // about 130 MB
    }
    let relay = Relay::start(&env, Opts::new());
    let a = relay.api(); let tok = relay.token.clone();
    let mut c = Checks::new();
    let r = a.t("GET", "/sessions", &tok, None); let list = arr(&r.j);
    c.ok_x("1 the list loads", r.code == 200 && r.j.is_array(), format!("code {}", r.code));
    c.ok_x("2 the good chat and the one with a number as folder are both there", list.iter().any(|x| x["title"] == "a good chat") && list.iter().any(|x| x["title"] == "chat with a number as folder"), list.iter().map(|x| jstr(&x["title"])).collect::<Vec<_>>().join(" | "));
    let m = a.t("GET", "/sessions/good/messages", &tok, None);
    c.ok("3 messages of the good chat load", m.code == 200 && re("a good chat", &m.text));
    let t0 = std::time::Instant::now(); let r1 = a.t("GET", "/sessions", &tok, None); let ms = t0.elapsed().as_millis();
    let tokens = |l: &Value| arr(l).into_iter().find(|x| x["title"] == "a huge chat").map(|x| x["tokens"].as_f64().unwrap_or(0.0)).unwrap_or(0.0);
    let big1 = tokens(&r1.j);
    c.ok_x("4 a 130 MB chat does not stall the list", r1.code == 200 && ms < 3000 && big1 >= 0.0, format!("{ms} ms"));
    a.t("GET", "/sessions", &tok, None); let big2 = tokens(&a.t("GET", "/sessions", &tok, None).j);
    c.ok_x("5 its totals fill in over a few looks (at once with the Rust reads)", big2 > big1 || (big1 > 0.0 && big2 == big1), format!("{big1} {big2}"));
    c.finish();
}
