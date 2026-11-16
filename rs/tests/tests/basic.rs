// Framework smoke tests: the relay starts, has a token, refuses unknown callers.
use hearth_tests::*;

#[test]
fn relay_starts_and_guards() {
    let env = Env::new("basic");
    let r = Relay::start(&env, Opts::new());
    let mut c = Checks::new();
    c.ok("1 relay has a token", !r.token.is_empty());
    let own = r.own(); let own: Vec<(&str, &str)> = own.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    c.ok("2 sessions answers for the owner", r.api().get("/sessions", &own).code == 200);
    c.ok("3 no token is refused", r.api().get("/sessions", &[]).code == 401);
    c.ok("4 wrong token is refused", r.api().get("/sessions", &[("Authorization", "Bearer nope")]).code == 401);
    let r2 = Relay::start(&Env::new("basic2"), Opts::new());
    c.ok("5 two relays run side by side", r2.port != r.port && r2.token != r.token);
    c.finish();
}
