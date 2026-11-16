// A server that hangs is stopped and started again by the program that watches it, without anyone doing anything.
use hearth_tests::*;
use serde_json::Value;

fn get(port: u16, p: &str, ms: u64) -> (u16, Value) {
    let c = reqwest::blocking::Client::builder().no_proxy().timeout(std::time::Duration::from_millis(ms)).build().unwrap();
    match c.get(format!("http://127.0.0.1:{port}{p}")).send() { Ok(r) => { let code = r.status().as_u16(); (code, serde_json::from_str(&r.text().unwrap_or_default()).unwrap_or(Value::Null)) } Err(_) => (0, Value::Null) }
}

#[test]
fn watchdog() {
    let env = Env::new("wd");
    let log = env.t.join("log.txt");
    let mut relay = Relay::start(&env, Opts::new().watchdog().log(&log).env("HEARTH_MEM_CHILD", "").env("HEARTH_WD_INTERVAL_MS", "400").env("HEARTH_WD_TIMEOUT_MS", "700").env("HEARTH_WD_FAILS", "2").env("HEARTH_WD_GRACE_MS", "1500"));
    let port = relay.port;
    let mut c = Checks::new();
    c.ok("1 the server answers /health, and /health shows nothing private", wait_for(10000, || get(port, "/health", 3000).0 == 200) && get(port, "/health", 3000).1.to_string().len() < 60);
    let boot1 = get(port, "/health", 3000).1["boot"].clone();
    let _hang = std::thread::spawn(move || get(port, "/__hang", 60000));   // the server is now stuck for a minute
    sleep(500);
    c.ok("2 while stuck it does not answer", get(port, "/health", 600).0 == 0);
    let back = wait_for(20000, || { let h = get(port, "/health", 800); h.0 == 200 && !h.1["boot"].is_null() && h.1["boot"] != boot1 });
    c.ok("3 the watchdog stops it and starts a new one, which answers again", back);
    let lg = std::fs::read_to_string(&log).unwrap_or_default();
    c.ok("4 it says so in the log", re("watchdog: no answer from the server", &lg) && re("starting it again", &lg));
    let boot2 = get(port, "/health", 3000).1["boot"].clone(); sleep(3000);
    c.ok("5 a healthy server is left alone", get(port, "/health", 3000).1["boot"] == boot2);
    let t0 = std::time::Instant::now(); let gone = relay.terminate();
    c.ok_x("6 stopping the program stops the server too", gone && get(port, "/health", 800).0 == 0, format!("{} ms", t0.elapsed().as_millis()));
    c.finish();
}
