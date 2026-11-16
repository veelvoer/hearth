//! What tokens would cost at the published API prices: the same table as relay/pricing.js.
use regex::Regex;
use std::sync::OnceLock;

struct Row { re: Regex, input: f64, out: f64, cr_share: f64 }
fn table() -> &'static Vec<Row> {
    static T: OnceLock<Vec<Row>> = OnceLock::new();
    T.get_or_init(|| {
        [
            (r"fable-5", 10.0, 50.0, 0.1),
            (r"opus-5-5", 4.0, 20.0, 0.05),
            (r"opus-(5|4-8|4-7|4-6|4-5)", 5.0, 25.0, 0.1),
            (r"opus", 15.0, 75.0, 0.1),
            (r"sonnet-5", 2.0, 10.0, 0.1),
            (r"sonnet", 3.0, 15.0, 0.1),
            (r"haiku-4", 1.0, 5.0, 0.1),
            (r"haiku", 0.8, 4.0, 0.1),
        ]
        .into_iter()
        .map(|(re, input, out, cr_share)| Row { re: Regex::new(re).unwrap(), input, out, cr_share })
        .collect()
    })
}
const CACHE_WRITE: f64 = 1.25;

pub struct Tokens { pub input: f64, pub out: f64, pub cr: f64, pub cw: f64 }

/// Dollars for one set of token counts.
pub fn cost_of(model: &str, t: &Tokens) -> f64 {
    let m = model.to_lowercase();
    let (i, o, cr, cw) = match table().iter().find(|r| r.re.is_match(&m)) {
        Some(r) => (r.input, r.out, r.cr_share * r.input, CACHE_WRITE * r.input),
        None => (3.0, 15.0, 0.3, 3.75),
    };
    (t.input * i + t.out * o + t.cr * cr + t.cw * cw) / 1e6
}

/// (input, output, cache read, cache write) dollars per million tokens.
pub fn price_of(model: &str) -> (f64, f64, f64, f64) {
    let m = model.to_lowercase();
    match table().iter().find(|r| r.re.is_match(&m)) { Some(r) => (r.input, r.out, r.cr_share * r.input, CACHE_WRITE * r.input), None => (3.0, 15.0, 0.3, 3.75) }
}
/// A readable name: claude-opus-4-8 -> "Opus 4.8".
pub fn label_of(model: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(?i)(opus|sonnet|haiku|fable|mythos)[- ](\d+)(?:[-.](\d+))?").unwrap());
    match re.captures(model) {
        Some(c) => {
            let n = c[1].to_lowercase();
            let mut s = format!("{}{} {}", n[..1].to_uppercase(), &n[1..], &c[2]);
            if let Some(x) = c.get(3) { if x.as_str().len() < 3 { s.push('.'); s.push_str(x.as_str()); } }
            s
        }
        None => model.to_string(),
    }
}
