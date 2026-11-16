//! scripts/lucide-gen.js: Lucide SVG files -> (a) inner SVG markup for the desktop, (b) one path string per icon for the phone.
use anyhow::{anyhow, Result};
use regex::Regex;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::Path;

pub struct El { pub tag: String, pub a: HashMap<String, String>, pub raw: String }

fn attrs(s: &str) -> HashMap<String, String> {
    Regex::new(r#"([a-z0-9-]+)="([^"]*)""#).unwrap().captures_iter(s).map(|m| (m[1].to_string(), m[2].to_string())).collect()
}
pub fn elements(svg: &str) -> Vec<El> {
    Regex::new(r"<(path|circle|rect|line|polyline|polygon|ellipse)\b([^>]*?)/>").unwrap().captures_iter(svg).map(|m| El { tag: m[1].to_string(), a: attrs(&m[2]), raw: m[0].to_string() }).collect()
}
/// JS Number(v || 0) printed like JS prints numbers.
fn n(a: &HashMap<String, String>, k: &str) -> f64 { a.get(k).filter(|v| !v.is_empty()).and_then(|v| v.trim().parse::<f64>().ok()).unwrap_or(0.0) }
fn num(x: f64) -> String { if x == x.trunc() && x.abs() < 1e15 { format!("{}", x as i64) } else { format!("{x}") } }
fn or(a: &HashMap<String, String>, k1: &str, k2: &str) -> f64 { let v = n(a, k1); if v != 0.0 { v } else { n(a, k2) } }

/// The path data of one element (what the phone draws).
pub fn to_d(e: &El) -> Result<String> {
    let a = &e.a;
    Ok(match e.tag.as_str() {
        "path" => a.get("d").cloned().unwrap_or_default(),
        "circle" | "ellipse" => { let (rx, ry, cx, cy) = (or(a, "r", "rx"), or(a, "r", "ry"), n(a, "cx"), n(a, "cy")); format!("M{} {}a{} {} 0 1 0 {} 0a{} {} 0 1 0 {} 0", num(cx - rx), num(cy), num(rx), num(ry), num(2.0 * rx), num(rx), num(ry), num(-2.0 * rx)) }
        "line" => format!("M{} {}L{} {}", a["x1"], a["y1"], a["x2"], a["y2"]),
        "polyline" | "polygon" => {
            let p: Vec<f64> = Regex::new(r"[\s,]+").unwrap().split(a["points"].trim()).map(|x| x.parse().unwrap_or(f64::NAN)).collect();
            let mut d = format!("M{} {}", num(p[0]), num(p[1]));
            let mut i = 2; while i < p.len() { d += &format!("L{} {}", num(p[i]), num(p[i + 1])); i += 2; }
            if e.tag == "polygon" { d.push('Z'); } d
        }
        "rect" => {
            let (x, y, w, h) = (n(a, "x"), n(a, "y"), n(a, "width"), n(a, "height"));
            let (mut rx, mut ry) = (or(a, "rx", "ry"), or(a, "ry", "rx"));
            rx = rx.min(w / 2.0); ry = ry.min(h / 2.0);
            if rx == 0.0 { format!("M{} {}h{}v{}h{}Z", num(x), num(y), num(w), num(h), num(-w)) }
            else { format!("M{} {}h{}a{} {} 0 0 1 {} {}v{}a{} {} 0 0 1 {} {}h{}a{} {} 0 0 1 {} {}v{}a{} {} 0 0 1 {} {}Z",
                num(x + rx), num(y), num(w - 2.0 * rx), num(rx), num(ry), num(rx), num(ry), num(h - 2.0 * ry), num(rx), num(ry), num(-rx), num(ry), num(-(w - 2.0 * rx)), num(rx), num(ry), num(-rx), num(-ry), num(-(h - 2.0 * ry)), num(rx), num(ry), num(rx), num(-ry)) }
        }
        t => return Err(anyhow!("{t}")),
    })
}
fn tidy(raw: &str) -> String {   // .replace(/\s+/g, ' ').replace(' />', '/>')
    Regex::new(r"\s+").unwrap().replace_all(raw, " ").replacen(" />", "/>", 1)
}
/// Reads the map: JSON text, or a JSON file (then `key` picks the part, like lucide-map.json's "phone").
pub fn load_map(arg: &str, key: &str) -> Result<Map<String, Value>> {
    let text = if Path::new(arg).is_file() { std::fs::read_to_string(arg)? } else { arg.to_string() };
    let v: Value = serde_json::from_str(&text)?;
    let m = if v.get(key).map(|x| x.is_object()).unwrap_or(false) { v[key].clone() } else { v };
    m.as_object().cloned().ok_or_else(|| anyhow!("the map must be a JSON object of name -> lucide icon name"))
}
/// {"desktop": {name: markup}, "phone": {name: "path data"}} as compact JSON.
pub fn run(dir: &Path, map: &Map<String, Value>) -> Result<String> {
    let (mut desktop, mut phone) = (Map::new(), Map::new());
    for (k, v) in map {
        let name = v.as_str().unwrap_or("");
        let svg = std::fs::read_to_string(dir.join(format!("{name}.svg"))).map_err(|e| anyhow!("{name}.svg: {e}"))?;
        let els = elements(&svg);
        desktop.insert(k.clone(), Value::String(els.iter().map(|e| tidy(&e.raw)).collect::<Vec<_>>().join("")));
        phone.insert(k.clone(), Value::String(els.iter().map(to_d).collect::<Result<Vec<_>>>()?.join(" ")));
    }
    let mut out = Map::new(); out.insert("desktop".into(), Value::Object(desktop)); out.insert("phone".into(), Value::Object(phone));
    Ok(serde_json::to_string(&Value::Object(out))?)
}
