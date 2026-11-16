// Hearth API keys: make a key, call /v1/messages with it, limits, models, auto picking, expiry, revoke. Fake "claude".
use hearth_tests::*;
use serde_json::{json, Value};

fn msg(text: &str, extra: Value) -> Value {
    let mut b = json!({ "messages": [{ "role": "user", "content": text }] });
    if let Some(o) = extra.as_object() { for (k, v) in o { b[k] = v.clone(); } }
    b
}
fn s(v: &Value) -> String { v.as_str().unwrap_or("").to_string() }

#[test]
fn apikeys() {
    let env = Env::new("keys");
    env.stub("claude", &py_stub("rec()\nout({'type': 'result', 'is_error': False, 'result': 'echo: ' + t[:60], 'usage': {'input_tokens': 10, 'output_tokens': 5}})"));
    let mut relay = Relay::start(&env, Opts::new());
    let port = relay.port;
    let a = relay.api();
    let tok = relay.token.clone();
    let own = [("Authorization", format!("Bearer {tok}"))];
    let own: Vec<(&str, &str)> = own.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let own = &own[..];
    let mut c = Checks::new();
    let calls = || env.calls();

    c.ok("1 only the owner can make keys", a.post("/admin/apikeys", &[("Authorization", "Bearer nope")], json!({"kind":"low"})).code == 401);
    let low = a.post("/admin/apikeys", own, json!({"name":"My app","kind":"low","perDay":3})).j;
    let lowkey = s(&low["key"]);
    c.ok("2 a key is made and shown once, only its last 4 characters are kept",
        lowkey.starts_with("hk_") && low["ends"] == json!(&lowkey[lowkey.len() - 4..]) && !a.get("/admin/apikeys", own).text.contains(&lowkey));
    c.ok("3 the key file holds only a hash", !env.read("d/apikeys.json").contains(&lowkey));
    c.ok("4 no key, no answer", a.post("/v1/messages", &[], msg("hi", json!({}))).code == 401
        && a.post("/v1/messages", &[("x-api-key", "hk_wrongwrongwrongwrongwrong")], msg("hi", json!({}))).code == 401);
    let h = [("x-api-key", lowkey.as_str())];
    let r = a.post("/v1/messages", &h, msg("Say hello", json!({})));
    c.ok_x("5 a call works the way the normal API answers", r.code == 200 && r.j["type"] == "message" && r.j["role"] == "assistant"
        && r.j["content"][0]["type"] == "text" && s(&r.j["content"][0]["text"]).contains("echo: Say hello") && r.j["usage"]["input_tokens"] == 10, &r.text[..r.text.len().min(140)]);
    let c1 = calls().get(0).cloned().unwrap_or(json!({}));
    let ca = &c1["args"];
    c.ok("6 Claude runs without tools, in its own empty folder, without a saved session",
        has_arg(ca, "--tools") && arg_after(ca, "--tools").as_deref() == Some("") && has_arg(ca, "--no-session-persistence") && s(&c1["cwd"]).ends_with("api-sandbox/_"));
    c.ok("7 a short question on a low key picks the lightest model and effort", r.j["model"] == "haiku" && r.j["hearth"]["effort"] == "low" && r.j["hearth"]["picked_automatically"] == true);
    let big = a.post("/v1/messages", &h, msg(&format!("Please analyze and refactor this architecture step by step. {} ```code```", "x".repeat(4000)), json!({})));
    c.ok_x("8 a big job on a low key picks the best it may use, never more", big.code == 200 && big.j["model"] == "sonnet" && big.j["hearth"]["effort"] == "medium", format!("{} {}", big.j["model"], big.j["hearth"]["effort"]));
    let no = a.post("/v1/messages", &h, msg("hi", json!({"model":"claude-opus-4"})));
    c.ok("9 asking for a model the key may not use is refused and says what is allowed", no.code == 403 && s(&no.j["error"]["message"]).contains("haiku, sonnet"));
    c.ok("10 same for the effort level", a.post("/v1/messages", &h, msg("hi", json!({"effort":"max"}))).code == 403);
    c.ok("11 the per-day limit stops the key", a.post("/v1/messages", &h, msg("third", json!({}))).code == 200 && a.post("/v1/messages", &h, msg("fourth", json!({}))).code == 429);
    let hk = s(&a.post("/admin/apikeys", own, json!({"kind":"high","name":"Heavy"})).j["key"]);
    let mod_ = a.get("/v1/models", &[("x-api-key", hk.as_str())]);
    let ids: Vec<String> = mod_.j["data"].as_array().map(|d| d.iter().map(|x| s(&x["id"])).collect()).unwrap_or_default();
    let efs: Vec<String> = mod_.j["efforts"].as_array().map(|d| d.iter().map(|x| s(x)).collect()).unwrap_or_default();
    c.ok("12 the models call lists what a high key may use", mod_.code == 200 && ids.join(",") == "sonnet,opus" && efs.join(",") == "high,xhigh,max");
    let gen = a.post("/admin/apikeys", own, json!({"kind":"general","name":"All"})).j;
    let genkey = s(&gen["key"]);
    let bh = format!("Bearer {genkey}");
    let g = a.post("/v1/messages", &[("Authorization", bh.as_str())], msg("hi", json!({"model":"opus","effort":"max"})));
    c.ok("13 a general key can use every model and effort, also with Bearer", g.code == 200 && g.j["model"] == "opus" && g.j["hearth"]["effort"] == "max" && has_arg(&calls().pop().unwrap()["args"], "opus"));
    c.ok("14 a bad request is explained", a.post("/v1/messages", &[("x-api-key", genkey.as_str())], json!({"messages":[]})).code == 400
        && a.post("/v1/messages", &[("x-api-key", genkey.as_str())], msg("hi", json!({"stream":true}))).code == 400);
    let exp = s(&a.post("/admin/apikeys", own, json!({"kind":"general","name":"Soon","expiresAt": now_ms() + 1500.0})).j["key"]);
    c.ok("15 before it expires it works", a.post("/v1/messages", &[("x-api-key", exp.as_str())], msg("hi", json!({}))).code == 200);
    sleep(1700);
    c.ok("16 after the expiry date it does not", a.post("/v1/messages", &[("x-api-key", exp.as_str())], msg("hi", json!({}))).code == 401);
    let lim = s(&a.post("/admin/apikeys", own, json!({"kind":"general","name":"Careful","weekStopPct":1})).j["key"]);
    env.write("d/usage.json", &json!({"week":{"pct":50,"resetsAt":now_ms()+86400000.0},"at":now_ms()}).to_string());
    relay.kill(); sleep(400);
    let mut relay2 = Relay::start(&env, Opts::new().port(port));
    c.ok("17 a key that stops at a point of the weekly usage stops there", a.post("/v1/messages", &[("x-api-key", lim.as_str())], msg("hi", json!({}))).code == 429);
    c.ok("18 a revoked key stops at once", a.post("/admin/apikeys/revoke", own, json!({"id": gen["id"]})).code == 200
        && a.post("/v1/messages", &[("x-api-key", genkey.as_str())], msg("hi", json!({}))).code == 401);
    c.ok("19 keys survive a restart", a.get("/admin/apikeys", own).j["keys"].as_array().map(|k| k.len()) == Some(5));
    // a key that remembers: the same Claude session is continued, with only the newest message
    let mem = s(&a.post("/admin/apikeys", own, json!({"kind":"general","name":"Remembers","session":"key"})).j["key"]);
    let mh = [("x-api-key", mem.as_str())];
    let before = calls().len();
    let m1 = a.post("/v1/messages", &mh, msg("My project is called Tulip.", json!({})));
    let m2 = a.post("/v1/messages", &mh, json!({"messages":[{"role":"user","content":"My project is called Tulip."},{"role":"assistant","content":"ok"},{"role":"user","content":"What is it called?"}]}));
    let mc: Vec<Value> = calls().into_iter().skip(before).collect();
    c.ok("20 the first call starts a session, the second continues it with the newest message only", m1.code == 200 && m2.code == 200 && mc.len() >= 2
        && has_arg(&mc[0]["args"], "--session-id") && has_arg(&mc[1]["args"], "--resume")
        && arg_after(&mc[0]["args"], "--session-id") == arg_after(&mc[1]["args"], "--resume")
        && s(&mc[1]["in"]).trim() == "What is it called?" && !has_arg(&mc[0]["args"], "--no-session-persistence"));
    c.ok("21 it says which session it used", m2.j["hearth"]["session"] == "main");
    let m3 = a.post("/v1/messages", &mh, msg("Start over.", json!({"reset_session":true})));
    let c3 = calls().pop().unwrap();
    c.ok("22 reset_session starts a new session", m3.code == 200 && has_arg(&c3["args"], "--session-id") && arg_after(&c3["args"], "--session-id") != arg_after(&mc[0]["args"], "--session-id"));
    let nm = s(&a.post("/admin/apikeys", own, json!({"kind":"general","name":"Named","session":"named"})).j["key"]);
    let nh = [("x-api-key", nm.as_str())];
    let b4 = calls().len();
    a.post("/v1/messages", &nh, msg("a", json!({"session":"alice"})));
    a.post("/v1/messages", &nh, msg("b", json!({"session":"bob"})));
    a.post("/v1/messages", &nh, msg("c", json!({"session":"alice"})));
    let nc: Vec<Value> = calls().into_iter().skip(b4).collect();
    c.ok("23 named sessions are kept apart: alice continues, bob starts his own", nc.len() >= 3 && has_arg(&nc[0]["args"], "--session-id") && has_arg(&nc[1]["args"], "--session-id")
        && has_arg(&nc[2]["args"], "--resume") && arg_after(&nc[2]["args"], "--resume") == arg_after(&nc[0]["args"], "--session-id") && nc[0]["cwd"] == nc[2]["cwd"]);
    let lk = s(&a.post("/admin/apikeys", own, json!({"kind":"general","name":"SessionStop","sessionStopPct":40})).j["key"]);
    env.write("d/usage.json", &json!({"week":{"pct":5,"resetsAt":now_ms()+86400000.0},"session":{"pct":70,"resetsAt":now_ms()+3600000.0},"at":now_ms()}).to_string());
    relay2.kill(); sleep(400);
    let _relay3 = Relay::start(&env, Opts::new().port(port));
    let sr = a.post("/v1/messages", &[("x-api-key", lk.as_str())], msg("hi", json!({})));
    c.ok("25 a key that stops at a point of the 5-hour session stops there", sr.code == 429 && s(&sr.j["error"]["message"]).contains("5-hour session"));
    c.ok("26 keys with memory survive a restart and still continue", a.post("/v1/messages", &mh, msg("still there?", json!({}))).code == 200 && has_arg(&calls().pop().unwrap()["args"], "--resume"));
    let rs = a.post("/admin/restart", own, json!({}));
    c.ok_x("27 restart: only the owner, and only where a service manager restarts it", a.post("/admin/restart", &[("Authorization", "Bearer x")], json!({})).code == 401 && [200, 400].contains(&rs.code), format!("{} {}", rs.code, rs.text));
    c.finish();
}
