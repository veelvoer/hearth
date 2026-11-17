//! Renders the real screens (rs/ui) and the popup with the Slint software renderer, no display needed, and writes PNGs to ~/hearth-tmp/shots.
use hearth_desktop::screens::{apply, Row, ScreenData};
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType, Rgb565Pixel};
use slint::platform::{Platform, WindowAdapter};
use slint::{ComponentHandle, PhysicalSize};
use std::rc::Rc;

struct P(Rc<MinimalSoftwareWindow>);
impl Platform for P { fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> { Ok(self.0.clone()) } }

fn shot(win: &Rc<MinimalSoftwareWindow>, w: u32, h: u32, name: &str) -> Vec<u8> {
    win.set_size(PhysicalSize::new(w, h));
    let mut buf = vec![slint::Rgb8Pixel::default(); (w * h) as usize];
    win.draw_if_needed(|r| { r.render(&mut buf, w as usize); });
    let bytes: Vec<u8> = buf.iter().flat_map(|p| [p.r, p.g, p.b, 255]).collect();
    let dir = std::path::PathBuf::from(std::env::var("HOME").unwrap()).join("hearth-tmp/shots"); std::fs::create_dir_all(&dir).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(std::fs::File::create(dir.join(name)).unwrap()), w, h); enc.set_color(png::ColorType::Rgba); enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&bytes).unwrap();
    bytes
}
fn distinct(b: &[u8]) -> usize { let mut s = std::collections::HashSet::new(); for p in b.chunks(4) { s.insert([p[0], p[1], p[2]]); } s.len() }
fn has(b: &[u8], c: (u8, u8, u8)) -> bool { b.chunks(4).any(|p| p[0] == c.0 && p[1] == c.1 && p[2] == c.2) }
fn _unused(_: Rgb565Pixel, _: RepaintBufferType) {}

#[test]
fn render_real_screens_and_popup() {
    let win = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(P(win.clone()))).unwrap();
    let app = hearth_ui::App::new().unwrap();
    let d = ScreenData {
        chats: vec![Row { header: "Working now".into(), title: "Fix the login bug".into(), sub: "webshop · working now".into(), working: true, ..Default::default() }, Row { header: "Today".into(), title: "Plan the trip".into(), sub: "Talk · 1h".into(), ..Default::default() }],
        history: vec![], tiles: vec![("Session (5h)".into(), "80%".into(), "plan usage".into())], devices: vec![("laptop (this computer)".into(), "This computer".into(), false)], device_ids: vec![], status: String::new(),
    };
    apply(&app, &d, "");
    app.show().unwrap();
    let dark = shot(&win, 1180, 760, "chats-dark.png");
    assert!(distinct(&dark) > 20, "the screen is not blank");
    // the desktop's accent flows into the window
    let p = app.global::<hearth_ui::Palette>(); p.set_dark(false); p.set_accent_override(slint::Color::from_rgb_u8(0x12, 0x34, 0x56));
    let light = shot(&win, 1180, 760, "chats-light-accent.png");
    assert_ne!(dark, light); assert!(has(&light, (0x12, 0x34, 0x56)), "custom accent is drawn");
    app.set_screen("home".into()); let home = shot(&win, 1180, 760, "home.png"); assert!(distinct(&home) > 20);
    app.set_screen("settings".into()); shot(&win, 1180, 760, "settings.png");
    app.hide().unwrap();
    // the popup
    let pop = hearth_desktop::ui::PopupWindow::new().unwrap();
    pop.set_heading("myproj: needs your OK".into()); pop.set_body("Bash: rm -rf build".into()); pop.set_kind("permission".into()); pop.set_permission(true);
    pop.show().unwrap(); let b = shot(&win, 380, 330, "popup.png"); assert!(distinct(&b) > 5);
    pop.hide().unwrap();
}
