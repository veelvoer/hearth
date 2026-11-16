// Big answers are compressed for clients that ask (phones on mobile data) and plain for the rest.
use hearth_tests::*;
use serde_json::json;
use std::io::Read;

#[test]
fn gzip() {
    let env = Env::new("gzip");
    let relay = Relay::start(&env, Opts::new());
    let a = relay.api(); let tok = relay.token.clone();
    let mut c = Checks::new();
    for i in 0..12 { a.t("POST", "/templates", &tok, Some(json!({"add": {"name": format!("Template {i}"), "text": format!("Write a friendly, short email to a customer about order number {i}. {}", "Keep it polite. ".repeat(20))}}))); }
    let auth = format!("Bearer {tok}");
    let (pc, ph, pb) = a.raw("GET", "/templates", &[("Authorization", auth.as_str())], None);
    let (gc, gh, gb) = a.raw("GET", "/templates", &[("Authorization", auth.as_str()), ("Accept-Encoding", "gzip")], None);
    let enc = |h: &Vec<(String, String)>| h.iter().find(|(k, _)| k == "content-encoding").map(|(_, v)| v.clone());
    c.ok_x("1 without a request for it the answer is plain", pc == 200 && enc(&ph).is_none() && pb.len() > 1024, pb.len().to_string());
    let mut un = Vec::new(); let _ = flate2::read::GzDecoder::new(&gb[..]).read_to_end(&mut un);
    c.ok_x("2 with Accept-Encoding gzip it is compressed, smaller, and the same", gc == 200 && enc(&gh).as_deref() == Some("gzip") && gb.len() < pb.len() && un == pb, format!("{} vs {}", gb.len(), pb.len()));
    c.finish();
}
