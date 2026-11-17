//! Headless renderer: draws every screen with Slint's software renderer into a buffer and writes PNGs.
//! Usage: shots [outdir]   (default ~/hearth-shots/ui)
use hearth_ui::{App, Motion, Palette, WatchApp};
use slint::platform::software_renderer::{MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter};
use slint::{Model, ComponentHandle, PhysicalSize, Rgb8Pixel};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

thread_local! { static WIN: RefCell<Option<Rc<MinimalSoftwareWindow>>> = RefCell::new(None); }
struct P(Instant);
impl Platform for P {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let w = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        WIN.with(|c| *c.borrow_mut() = Some(w.clone()));
        Ok(w)
    }
    fn duration_since_start(&self) -> std::time::Duration { self.0.elapsed() }
}

fn render(w: u32, h: u32, path: &PathBuf) {
    let win = WIN.with(|c| c.borrow().clone()).expect("window");
    win.set_size(PhysicalSize::new(w, h));
    slint::platform::update_timers_and_animations();
    let mut buf = vec![Rgb8Pixel::default(); (w * h) as usize];
    win.request_redraw();
    win.draw_if_needed(|r| { r.render(&mut buf, w as usize); });
    let mut raw = Vec::with_capacity(buf.len() * 3);
    for p in &buf { raw.extend_from_slice(&[p.r, p.g, p.b]); }
    let f = std::fs::File::create(path).expect("create png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&raw).unwrap();
    println!("wrote {} ({}x{})", path.display(), w, h);
}

fn main() {
    let _ = PremultipliedRgbaColor { red: 0, green: 0, blue: 0, alpha: 0 };
    slint::platform::set_platform(Box::new(P(Instant::now()))).unwrap();
    let out = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap()).join("hearth-shots/ui"));
    std::fs::create_dir_all(&out).unwrap();
    let phone = [("onboarding", ""), ("pairing", ""), ("call", ""), ("prompt", ""), ("settings", "updates"), ("settings", "keys"), ("chats-swipe", ""), ("chat-top", ""), ("chats", ""), ("chat", ""), ("home", ""), ("history", ""), ("settings", ""), ("settings", "devices")];
    let wide = [("onboarding", ""), ("prompt", ""), ("settings", "updates"), ("chat-top", ""), ("chat", ""), ("home", ""), ("history", ""), ("settings", "devices"), ("settings", "appearance")];
    for dark in [true, false] {
        let tag = if dark { "dark" } else { "light" };
        for (size, name, list) in [((360, 800), "phone", &phone[..]), ((1180, 780), "wide", &wide[..])] {
            for (screen, page) in list {
                let app = App::new().unwrap();
                app.global::<Motion>().set_scale(0.0);
                app.global::<Palette>().set_dark(dark);
                app.set_chat_top(*screen == "chat-top");
                if *screen == "chats-swipe" {
                    let mut v: Vec<hearth_ui::ChatRow> = app.get_chats().iter().collect();
                    v[1].swiped = true;
                    app.set_chats(slint::ModelRc::new(slint::VecModel::from(v)));
                }
                if *screen == "prompt" { app.set_prompt(true); }
                app.set_screen(match *screen { "chat-top" | "prompt" => "chat", "chats-swipe" => "chats", s => s }.into());
                app.set_page((*page).into());
                app.show().unwrap();
                let n = if page.is_empty() { screen.to_string() } else { format!("{screen}-{page}") };
                render(size.0, size.1, &out.join(format!("{name}-{tag}-{n}.png")));
                app.hide().unwrap();
            }
        }
        for screen in ["home", "chats"] {
            let app = WatchApp::new().unwrap();
            app.global::<Motion>().set_scale(0.0);
            app.global::<Palette>().set_dark(dark);
            app.set_screen(screen.into());
            app.show().unwrap();
            render(400, 400, &out.join(format!("watch-{tag}-{screen}.png")));
            app.hide().unwrap();
        }
    }

    // motion check: spring on the dock marker, sampled while it runs, then idle detection
    let app = App::new().unwrap();
    app.global::<Palette>().set_dark(true);
    app.set_screen("chats".into());
    app.show().unwrap();
    render(360, 800, &out.join("motion-0-start.png"));
    app.set_screen("settings".into());
    render(360, 800, &out.join("motion-0b-first-frame.png"));
    println!("animations active right after change: {}", WIN.with(|c| c.borrow().as_ref().unwrap().has_active_animations()));
    std::thread::sleep(std::time::Duration::from_millis(110));
    render(360, 800, &out.join("motion-1-mid.png"));
    std::thread::sleep(std::time::Duration::from_millis(700));
    render(360, 800, &out.join("motion-2-end.png"));
    slint::platform::update_timers_and_animations();
    println!("animations active at idle: {}", WIN.with(|c| c.borrow().as_ref().unwrap().has_active_animations()));
}
