// Pins belong to one week: after the reset the days are shared evenly again, and the weekday of the reset is split in two.
use hearth_tests::*;
use serde_json::{json, Value};

fn arr(v: &Value) -> Vec<Value> { v.as_array().cloned().unwrap_or_default() }

#[test]
fn weekpins() {
    let env = Env::new("pins");
    std::fs::create_dir_all(&env.dir).unwrap();
    let day = 86400000.0; let now = now_ms();
    // a week that reset 2 hours ago (so the reset weekday has a small first piece and a long last piece)
    let end = now - 2.0 * 3600000.0 + 7.0 * day;
    env.write("d/usage.json", &json!({"week":{"pct":1,"resetsAt":end},"session":{"pct":0,"resetsAt":now+4.0*3600000.0},"at":now}).to_string());
    env.write("d/weekhist.json", &json!([{"t": now - 3600000.0, "w": 1, "r": end}]).to_string());
    env.write("d/weekplan.json", &json!({"5": 94, "6": 0, "1": 0}).to_string());   // pins in the old format (unknown week)
    let mut relay = Relay::start(&env, Opts::new());
    let port = relay.port; let a = relay.api(); let tok = relay.token.clone();
    let mut c = Checks::new();
    let r = a.t("GET", "/weekplan?tz=0", &tok, None);
    c.ok_x("1 pins of an unknown (old) week are dropped", r.code == 200 && r.j["targets"].as_object().map(|o| o.len()) == Some(0), r.j["targets"].to_string());
    let days = arr(&r.j["days"]);
    let sum: f64 = days.iter().map(|d| d["planned"].as_f64().unwrap_or(0.0)).sum();
    c.ok_x("2 the reset weekday is two days: a small one at the start and one at the end", days.len() == 8 && days[0]["start"] == true && days[0]["wd"] == days[7]["wd"] && days[7]["start"] != true,
        days.iter().map(|d| format!("{}{}", jstr(&d["name"]), if d["start"] == true { "(start)" } else { "" })).collect::<Vec<_>>().join(" "));
    c.ok_x("3 together the days add up to 100%", (sum - 100.0).abs() < 0.6, format!("sum {sum:.1}"));
    c.ok_x("4 the end of the week is not a huge block: every open day is about the same", days.iter().skip(2).all(|d| d["planned"].as_f64().unwrap_or(0.0) < 30.0), days.iter().map(|d| d["planned"].to_string()).collect::<Vec<_>>().join(" "));
    let last_wd = days[7]["wd"].clone();
    let p = a.t("POST", "/weekplan", &tok, Some(json!({"day": last_wd, "pct": 30, "tz": 0})));
    let pd = arr(&p.j["days"]);
    c.ok_x("5 pinning the last piece works and gets its full share", p.code == 200 && pd.len() == 8 && pd[7]["fixed"] == true && (pd[7]["planned"].as_f64().unwrap_or(0.0) - 30.0).abs() < 0.6 && pd[0]["fixed"] == false, format!("{} {}", p.code, pd.get(7).map(|d| d["planned"].to_string()).unwrap_or_default()));
    c.ok("6 the pin is kept for this week", a.t("GET", "/weekplan?tz=0", &tok, None).j["targets"][last_wd.to_string().trim_matches('"')] == 30);
    relay.kill(); sleep(400);
    // the week resets: a new end -> the pin is gone
    let mut u = env.read_json("d/usage.json"); u["week"]["resetsAt"] = json!(u["week"]["resetsAt"].as_f64().unwrap() + 7.0 * day); env.write("d/usage.json", &u.to_string());
    let _relay2 = Relay::start(&env, Opts::new().port(port)); sleep(2000);
    c.ok("7 next week starts without the pins", a.t("GET", "/weekplan?tz=0", &tok, None).j["targets"].as_object().map(|o| o.len()) == Some(0));
    c.finish();
}
