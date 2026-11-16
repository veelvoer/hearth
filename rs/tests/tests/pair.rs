// "Ask to connect": a phone asks, the computer accepts or declines.
use hearth_tests::*;
use serde_json::json;

#[test]
fn pair() {
    let env = Env::new("pair");
    let r = Relay::start(&env, Opts::new().no_projects().args(&["--lan"]));
    let a = r.api(); let tok = r.token.clone();
    let mut c = Checks::new();
    let ra = a.t("POST", "/pair/request", "", Some(json!({"name":"Pixel 9 <script>"})));
    let id = jstr(&ra.j["id"]);
    c.ok("1 a phone can ask without a code", ra.code == 200 && id.len() == 32);
    c.ok("2 asking again right away is refused", a.t("POST", "/pair/request", "", Some(json!({"name":"x"}))).code == 429);
    c.ok("3 the list needs the key", a.t("GET", "/pair/requests", "", None).code == 401);
    let l = a.t("GET", "/pair/requests", &tok, None);
    c.ok_x("4 the computer sees the request, name cleaned", l.j.as_array().map(|x| x.len()) == Some(1) && l.j[0]["name"] == "Pixel 9 script", &l.text);
    c.ok("5 pending until answered", a.t("GET", &format!("/pair/request/{id}"), "", None).j["status"] == "pending");
    a.t("POST", "/pair/decision", &tok, Some(json!({"id": id, "accept": true})));
    let got = a.t("GET", &format!("/pair/request/{id}"), "", None);
    c.ok_x("6 accepted: the phone gets the key", got.j["status"] == "accepted" && got.j["token"] == json!(tok), jstr(&got.j["status"]));
    c.ok("7 the key is handed out only once", a.t("GET", &format!("/pair/request/{id}"), "", None).j["token"].is_null());
    sleep(3200);
    let b = a.t("POST", "/pair/request", "", Some(json!({"name":"Other"})));
    let bid = jstr(&b.j["id"]);
    a.t("POST", "/pair/decision", &tok, Some(json!({"id": bid, "accept": false})));
    c.ok("8 declined: no key", a.t("GET", &format!("/pair/request/{bid}"), "", None).j["status"] == "declined");
    c.ok("9 a declined phone must wait", a.t("POST", "/pair/request", "", Some(json!({"name":"Other"}))).code == 429);
    c.ok("10 unknown request id", a.t("GET", &format!("/pair/request/{}", "a".repeat(32)), "", None).j["status"] == "expired");
    c.finish();
}
