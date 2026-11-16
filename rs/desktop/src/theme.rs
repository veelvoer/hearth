//! What the UI should look like given the user's choice (Settings: theme, palette, custom accent) and what the desktop says.
use crate::color::{on_accent, Palette, Rgb, Tokens};
use crate::sys;
use hearth_relay::util::J;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedTheme {
    pub dark: bool,
    /// The desktop hands over its whole palette (Oxide): the UI paints with these surfaces and text instead of its own.
    pub surfaces: Option<Tokens>,
    /// Accent and the text color on it, when the user picked the system or a custom accent (None: Hearth's own).
    pub accent: Option<(Rgb, Rgb)>,
}

/// The pure part: no desktop is asked here.
pub fn resolve_with(settings: &Value, pal: Option<Palette>, sys_accent: Option<Rgb>, sys_dark: Option<bool>) -> ResolvedTheme {
    let palette = { let p = settings.s("palette"); if p.is_empty() { "claude".to_string() } else { p } };
    let mode = { let m = settings.s("theme"); if m.is_empty() { "system".to_string() } else { m } };
    let sp = if palette == "system" { pal } else { None };
    // the desktop's own surfaces only when they match the mode asked for ("dark" with a light desktop palette keeps Hearth's own dark)
    let surfaces = sp.as_ref().filter(|p| mode == "system" || (mode == "dark") == !p.light).map(|p| p.tokens());
    let dark = match mode.as_str() { "dark" => true, "light" => false, _ => sp.as_ref().map(|p| !p.light).or(sys_dark).unwrap_or(true) };
    let a = match palette.as_str() {
        "system" => sp.as_ref().map(|p| p.accent).or(sys_accent),
        "custom" => Some(Rgb::parse(&settings.s("customAccent")).unwrap_or(Rgb(0xF0, 0x64, 0x3C))),
        _ => None,
    };
    let accent = a.map(|a| (a, on_accent(a, sp.as_ref().filter(|p| p.accent == a).and_then(|p| p.ink))));
    ResolvedTheme { dark, surfaces, accent }
}

pub async fn resolve(settings: &Value) -> ResolvedTheme {
    let pal = sys::oxide_palette();
    let need_sys = settings.s("palette") == "system";
    let accent = if need_sys && pal.is_none() { sys::accent().await } else { None };
    let dark = if settings.s("theme").is_empty() || settings.s("theme") == "system" { sys::dark().await } else { None };
    resolve_with(settings, pal, accent, dark)
}

#[cfg(test)]
mod tests {
    use super::*; use serde_json::json;
    fn oxide(light: bool) -> Palette { Palette::from_json(&json!({"bg": "#101010", "text": "#eeeeee", "accent": "#336699", "ink": "#fafafa", "light": light})).unwrap() }
    #[test] fn default_is_hearths_own() { let t = resolve_with(&json!({}), Some(oxide(false)), Some(Rgb(1, 2, 3)), Some(false)); assert!(!t.dark && t.surfaces.is_none() && t.accent.is_none()); }
    #[test] fn system_palette_takes_over() {
        let t = resolve_with(&json!({"palette": "system"}), Some(oxide(false)), None, None);
        assert!(t.dark && t.surfaces.is_some()); let (a, on) = t.accent.unwrap(); assert_eq!(a, Rgb(0x33, 0x66, 0x99)); assert_eq!(on, Rgb(0xfa, 0xfa, 0xfa));
    }
    #[test] fn forced_mode_against_desktop_palette() {
        let t = resolve_with(&json!({"palette": "system", "theme": "light"}), Some(oxide(false)), None, None);
        assert!(!t.dark && t.surfaces.is_none(), "a dark Oxide palette is not used for a light window"); assert!(t.accent.is_some());
    }
    #[test] fn accent_without_palette_and_custom() {
        let t = resolve_with(&json!({"palette": "system"}), None, Some(Rgb(200, 30, 30)), Some(true)); assert!(t.dark && t.surfaces.is_none()); assert_eq!(t.accent.unwrap().1, Rgb(255, 255, 255));
        let c = resolve_with(&json!({"palette": "custom", "customAccent": "#ffee00"}), None, None, None); assert_eq!(c.accent.unwrap(), (Rgb(255, 238, 0), Rgb(0x11, 0x11, 0x11)));
        let d = resolve_with(&json!({"palette": "custom"}), None, None, None); assert_eq!(d.accent.unwrap().0, Rgb(0xF0, 0x64, 0x3C));
    }
}
