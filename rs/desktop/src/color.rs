//! Colors: parsing, mixing in OKLab (the same as CSS `color-mix(in oklab, ...)` that the old renderer used) and the
//! token set the UI paints with, derived from what the desktop tells (Oxide's palette, an accent color, dark or light).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn parse(s: &str) -> Option<Rgb> {
        let s = s.trim().strip_prefix('#')?;
        if s.len() < 6 || !s.is_char_boundary(6) || !s[..6].chars().all(|c| c.is_ascii_hexdigit()) { return None; }
        let n = u32::from_str_radix(&s[..6], 16).ok()?;
        Some(Rgb((n >> 16) as u8, (n >> 8) as u8, n as u8))
    }
    pub fn hex(self) -> String { format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2) }
    /// Perceived brightness 0..1 (the same weights the old app used to pick text on the accent).
    pub fn luma(self) -> f64 { (0.299 * self.0 as f64 + 0.587 * self.1 as f64 + 0.114 * self.2 as f64) / 255.0 }
    pub fn slint(self) -> slint::Color { slint::Color::from_rgb_u8(self.0, self.1, self.2) }
}

fn to_lin(c: u8) -> f64 { let c = c as f64 / 255.0; if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) } }
fn from_lin(c: f64) -> u8 { let c = c.clamp(0.0, 1.0); let v = if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }; (v * 255.0).round().clamp(0.0, 255.0) as u8 }

fn to_oklab(c: Rgb) -> [f64; 3] {
    let (r, g, b) = (to_lin(c.0), to_lin(c.1), to_lin(c.2));
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    [0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s, 1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s, 0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s]
}
fn from_oklab(l_: [f64; 3]) -> Rgb {
    let [l, a, b] = l_;
    let l3 = (l + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let m3 = (l - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let s3 = (l - 0.0894841775 * a - 1.2914855480 * b).powi(3);
    Rgb(from_lin(4.0767416621 * l3 - 3.3077115913 * m3 + 0.2309699292 * s3), from_lin(-1.2684380046 * l3 + 2.6097574011 * m3 - 0.3413193965 * s3), from_lin(-0.0041960863 * l3 - 0.7034186147 * m3 + 1.7076147010 * s3))
}

/// `color-mix(in oklab, a pct%, b)`
pub fn mix(a: Rgb, b: Rgb, pct: f64) -> Rgb {
    let (x, y) = (to_oklab(a), to_oklab(b)); let t = (pct / 100.0).clamp(0.0, 1.0);
    from_oklab([x[0] * t + y[0] * (1.0 - t), x[1] * t + y[1] * (1.0 - t), x[2] * t + y[2] * (1.0 - t)])
}

/// What the window manager hands over (Oxide: `~/.config/oxide/theme.json`). Only bg, text and accent are required.
#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    pub bg: Rgb, pub text: Rgb, pub accent: Rgb,
    pub surface: Option<Rgb>, pub raised: Option<Rgb>, pub line: Option<Rgb>, pub muted: Option<Rgb>, pub ink: Option<Rgb>, pub error: Option<Rgb>,
    pub light: bool, pub name: String,
}

/// The tokens every screen paints with: background, card, text, muted text, accent, on-accent, error (plus line and the raised/active step).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tokens { pub background: Rgb, pub card: Rgb, pub line: Rgb, pub text: Rgb, pub muted: Rgb, pub raised: Rgb, pub accent: Rgb, pub on_accent: Rgb, pub error: Rgb }

impl Palette {
    pub fn from_json(o: &serde_json::Value) -> Option<Palette> {
        let c = |k: &str| o.get(k).and_then(|v| v.as_str()).and_then(Rgb::parse);
        Some(Palette { bg: c("bg")?, text: c("text")?, accent: c("accent")?, surface: c("surface"), raised: c("raised"), line: c("line"), muted: c("muted"), ink: c("ink"), error: c("bad"),
            light: o.get("light").and_then(|v| v.as_bool()).unwrap_or(false), name: o.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string() })
    }
    /// The whole palette as the UI tokens; missing steps are mixed from text and background like the old renderer did.
    pub fn tokens(&self) -> Tokens {
        let (bg, text) = (self.bg, self.text);
        let surface = self.surface.unwrap_or_else(|| mix(text, bg, 6.0));
        let raised = self.raised.unwrap_or_else(|| mix(text, bg, 11.0));
        let line = self.line.unwrap_or_else(|| mix(text, bg, 18.0));
        let muted = self.muted.unwrap_or_else(|| mix(text, bg, 62.0));
        Tokens { background: bg, card: surface, line, text, muted, raised, accent: self.accent, on_accent: on_accent(self.accent, self.ink), error: self.error.unwrap_or(Rgb(0xe5, 0x48, 0x4d)) }
    }
}

/// Dark text on a light accent, white on a dark one; the desktop's own ink color wins when it gives one.
pub fn on_accent(accent: Rgb, ink: Option<Rgb>) -> Rgb { ink.unwrap_or(if accent.luma() > 0.62 { Rgb(0x11, 0x11, 0x11) } else { Rgb(0xff, 0xff, 0xff) }) }

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn parse_hex() { assert_eq!(Rgb::parse("#1b1a19"), Some(Rgb(0x1b, 0x1a, 0x19))); assert_eq!(Rgb::parse("#1b1a19ff"), Some(Rgb(0x1b, 0x1a, 0x19))); assert_eq!(Rgb::parse("1b1a19"), None); assert_eq!(Rgb::parse("#12"), None); assert_eq!(Rgb::parse("#zzzzzz"), None); }
    #[test] fn mix_ends_and_middle() {
        let (w, b) = (Rgb(255, 255, 255), Rgb(0, 0, 0));
        assert_eq!(mix(w, b, 100.0), w); assert_eq!(mix(w, b, 0.0), b);
        let m = mix(w, b, 50.0); assert!(m.0 > 90 && m.0 < 130, "{m:?}");   // oklab midpoint of white and black is mid grey (~99)
        assert_eq!(mix(Rgb(10, 20, 30), Rgb(10, 20, 30), 37.0), Rgb(10, 20, 30));
    }
    #[test] fn tokens_fill_gaps() {
        let p = Palette::from_json(&serde_json::json!({"bg": "#101010", "text": "#eeeeee", "accent": "#ffcc00", "light": false})).unwrap();
        let t = p.tokens();
        assert_eq!(t.background, Rgb(16, 16, 16)); assert!(t.card.0 > 16 && t.card.0 < t.line.0 && t.line.0 < t.muted.0 && t.muted.0 < 0xee);
        assert_eq!(t.on_accent, Rgb(0x11, 0x11, 0x11));   // yellow is light enough for dark text
        let p2 = Palette { ink: Some(Rgb(1, 2, 3)), ..p }; assert_eq!(p2.tokens().on_accent, Rgb(1, 2, 3));
    }
    #[test] fn palette_needs_bg_text_accent() { assert!(Palette::from_json(&serde_json::json!({"bg": "#101010", "text": "#eeeeee"})).is_none()); }
}
