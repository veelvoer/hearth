//! Small helpers that behave like the JavaScript the Node relay uses, so both give the same answers.
use serde_json::Value;

pub fn is_js_ws(c: char) -> bool {
    matches!(c, '\u{9}' | '\u{a}' | '\u{b}' | '\u{c}' | '\u{d}' | '\u{20}' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
}

/// JavaScript `String.prototype.trim`.
pub fn trim(s: &str) -> &str {
    s.trim_matches(is_js_ws)
}

/// JavaScript `s.slice(from, to)` for non-negative positions, counted in UTF-16 units.
pub fn slice16(s: &str, from: usize, to: usize) -> String {
    if from == 0 && s.len() <= to {
        return s.to_string();
    }
    let u: Vec<u16> = s.encode_utf16().collect();
    let to = to.min(u.len());
    if from >= to {
        return String::new();
    }
    String::from_utf16_lossy(&u[from..to])
}

/// JavaScript `String(v)` for the values that show up in chat files.
pub fn js_string(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.fract() == 0.0 && f.abs() < 1e21 { format!("{}", f as i128) } else { n.to_string() }
            } else {
                n.to_string()
            }
        }
        Value::Array(a) => a.iter().map(js_string).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".into(),
    }
}

/// `clip(v, n)` of core.js: `String(v == null ? '' : v).slice(0, n)`.
pub fn clip(v: Option<&Value>, n: usize) -> String {
    match v {
        None => String::new(),
        Some(v) => slice16(&js_string(v), 0, n),
    }
}

/// `textOf` of core.js: a string, or the text blocks of a content array joined by newlines.
pub fn text_of(c: Option<&Value>) -> String {
    match c {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(a)) => a
            .iter()
            .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
            .map(|b| match b.get("text") { Some(Value::String(s)) => s.clone(), Some(v) if !v.is_null() && v != &Value::Bool(false) => js_string(v), _ => String::new() })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// `x || 0` for a token count.
pub fn num(v: Option<&Value>) -> f64 {
    v.and_then(|x| x.as_f64()).unwrap_or(0.0)
}

/// `Date.parse(o.timestamp) / 1000`, or 0.
pub fn iso_ts(v: Option<&Value>) -> f64 {
    match v.and_then(|x| x.as_str()) {
        Some(s) => chrono::DateTime::parse_from_rfc3339(s).map(|d| d.timestamp_millis() as f64 / 1000.0).unwrap_or(0.0),
        None => 0.0,
    }
}

/// JavaScript `n.toFixed(d)`: rounds half up on the exact value of the double.
pub fn to_fixed(x: f64, d: usize) -> String {
    let exact = format!("{:.60}", x.abs());
    let (int, frac) = exact.split_once('.').unwrap_or((&exact, ""));
    let mut digits: Vec<u8> = int.bytes().chain(frac.bytes().take(d)).map(|b| b - b'0').collect();
    if frac.as_bytes().get(d).map(|b| b - b'0' >= 5).unwrap_or(false) {
        let mut i = digits.len();
        loop {
            if i == 0 { digits.insert(0, 1); break; }
            i -= 1;
            if digits[i] == 9 { digits[i] = 0; } else { digits[i] += 1; break; }
        }
    }
    let il = digits.len() - d;
    let mut out: String = digits[..il].iter().map(|x| (x + b'0') as char).collect();
    if d > 0 { out.push('.'); out.extend(digits[il..].iter().map(|x| (x + b'0') as char)); }
    if x < 0.0 && out.bytes().any(|b| b != b'0' && b != b'.') { out.insert(0, '-'); }
    out
}
