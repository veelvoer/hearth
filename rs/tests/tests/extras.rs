// The recap, saved chat templates and the one-project summary.
use hearth_tests::*;
use serde_json::{json, Value};
use std::fs;

fn arr(v: &Value) -> Vec<Value> { v.as_array().cloned().unwrap_or_default() }
fn iso(ms: f64) -> String { // RFC3339 UTC from epoch ms
    let secs = (ms / 1000.0) as i64; let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    let z = days + 719468; let era = z.div_euclid(146097); let doe = z.rem_euclid(146097); let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400; let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); let mp = (5 * doy + 2) / 153; let d = doy - (153 * mp + 2) / 5 + 1; let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.000Z", if m <= 2 { y + 1 } else { y }, m, d, rem / 3600, rem % 3600 / 60, rem % 60)
}

#[test]
fn extras() {
    let env = Env::new("extras");
    let proj = env.proj.join("app"); fs::create_dir_all(&proj).unwrap();
    let _ = std::process::Command::new("git").args(["init", "-q"]).arg(&proj).status();
    fs::write(proj.join("a.txt"), "x").unwrap();
    let ps = proj.to_string_lossy().to_string();
    let enc: String = ps.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let home = env.home.join(".claude/projects").join(enc); fs::create_dir_all(&home).unwrap();
    let line = |id: &str, ts: &str| json!({"type":"assistant","cwd":ps,"sessionId":id,"timestamp":ts,"message":{"id":format!("m{id}"),"role":"assistant","model":"claude-sonnet-4","usage":{"input_tokens":100,"output_tokens":50000},"content":[{"type":"text","text":"hi"}]}}).to_string() + "\n";
    let u = |id: &str, ts: &str| json!({"type":"user","cwd":ps,"sessionId":id,"timestamp":ts,"message":{"role":"user","content":format!("do {id}")}}).to_string() + "\n";
    let (t1, t2) = (iso(now_ms()), iso(now_ms() - 864e5));
    fs::write(home.join("c1.jsonl"), u("c1", &t1) + &line("c1", &t1)).unwrap();
    fs::write(home.join("c2.jsonl"), u("c2", &t2) + &line("c2", &t2)).unwrap();
    let relay = Relay::start(&env, Opts::new().env("HEARTH_MEM_CHILD", "1"));
    let a = relay.api(); let tok = relay.token.clone();
    let mut c = Checks::new();
    let rc = a.t("GET", "/recap?days=7", &tok, None).j;
    c.ok_x("1 the recap counts the days, prompts and cost", rc["days"] == 7 && rc["activeDays"] == 2 && rc["prompts"] == 2 && rc["cost"].as_f64().unwrap_or(0.0) > 0.0 && !rc["busiest"].is_null() && rc["empty"] == false, rc.to_string().chars().take(160).collect::<String>());
    c.ok("2 a quiet period says it is empty", a.t("GET", "/recap?days=1", &tok, None).j["activeDays"].as_f64().unwrap_or(9.0) <= 1.0);
    let t = a.t("POST", "/templates", &tok, Some(json!({"add": {"name": "Review", "text": "Review my last change and tell me what is risky."}})));
    c.ok("3 a template is saved and listed", t.code == 200 && a.t("GET", "/templates", &tok, None).j["templates"][0]["name"] == "Review");
    c.ok("4 a template needs a name and a text", a.t("POST", "/templates", &tok, Some(json!({"add": {"name": "", "text": "x"}}))).code == 400);
    a.t("POST", "/templates", &tok, Some(json!({"add": {"name": "Review", "text": "Newer text"}})));
    let tl = a.t("GET", "/templates", &tok, None).j;
    c.ok("5 the same name replaces the old one", arr(&tl["templates"]).len() == 1 && tl["templates"][0]["text"] == "Newer text");
    let id = tl["templates"][0]["id"].clone(); a.t("POST", "/templates", &tok, Some(json!({"remove": id})));
    c.ok("6 and removed", arr(&a.t("GET", "/templates", &tok, None).j["templates"]).is_empty());
    let ps_ = a.t("GET", &format!("/projects/summary?cwd={ps}"), &tok, None).j;
    c.ok_x("7 a project summary: chats, cost, and the changes of its git repository", ps_["name"] == "app" && ps_["chats"] == 2 && ps_["cost"].as_f64().unwrap_or(0.0) > 0.0 && ps_["changed"] == 1, ps_.to_string().chars().take(160).collect::<String>());
    a.t("POST", "/admin/options", &tok, Some(json!({"outside": false})));
    c.ok("8 a folder outside the projects folder is refused", a.t("GET", "/projects/summary?cwd=/etc", &tok, None).code == 403);
    c.ok("9 guests cannot read any of it", a.t("GET", "/recap", "share.abcd.abcdefghijklmnop", None).code == 401 && a.t("GET", "/templates", "share.abcd.abcdefghijklmnop", None).code == 401);
    c.finish();
}
