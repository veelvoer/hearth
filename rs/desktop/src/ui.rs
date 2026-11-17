//! The glue between the app logic and the Slint windows: the main window (a minimal wiring window until the screens of `rs/ui` replace it),
//! the small always-on-top popups, theme colors, drag-and-drop of files, close-to-tray.
//! Everything here runs on the UI thread; [`App`] sends it [`Event`]s through `invoke_from_event_loop`.
use crate::app::{App, Event, PopupSpec};
use crate::color::{Rgb, Tokens};
use crate::ctl::version;
use crate::theme::{self, ResolvedTheme};
use hearth_relay::util::{home, J};
use slint::winit_030::{winit, EventResult, WinitWindowAccessor};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

slint::include_modules!();

/// Hearth's own colors for when the desktop gives none (dark is the reference). The real design lives in rs/ui/theme.slint.
pub fn own_tokens(dark: bool) -> Tokens {
    if dark { Tokens { background: Rgb(0x1b, 0x1a, 0x19), card: Rgb(0x23, 0x21, 0x20), line: Rgb(0x3a, 0x37, 0x35), text: Rgb(0xec, 0xe7, 0xe1), muted: Rgb(0xa3, 0x9c, 0x94), raised: Rgb(0x2e, 0x2b, 0x29), accent: Rgb(0xf0, 0x64, 0x3c), on_accent: Rgb(0xff, 0xff, 0xff), error: Rgb(0xe5, 0x48, 0x4d) } }
    else { Tokens { background: Rgb(0xfa, 0xf8, 0xf6), card: Rgb(0xff, 0xff, 0xff), line: Rgb(0xe4, 0xde, 0xd8), text: Rgb(0x26, 0x22, 0x1f), muted: Rgb(0x6f, 0x68, 0x61), raised: Rgb(0xf0, 0xec, 0xe8), accent: Rgb(0xd9, 0x4f, 0x27), on_accent: Rgb(0xff, 0xff, 0xff), error: Rgb(0xc9, 0x2a, 0x30) } }
}
/// The tokens a theme result paints with.
pub fn tokens_for(t: &ResolvedTheme) -> Tokens {
    let mut k = t.surfaces.unwrap_or_else(|| own_tokens(t.dark));
    if let Some((a, on)) = t.accent { k.accent = a; k.on_accent = on; }
    k
}
fn set_pal<C: ComponentHandle>(c: &C, k: &Tokens) where for<'a> Pal<'a>: slint::Global<'a, C> {
    let p = c.global::<Pal>();
    p.set_background(k.background.slint()); p.set_card(k.card.slint()); p.set_line(k.line.slint()); p.set_text(k.text.slint()); p.set_muted(k.muted.slint());
    p.set_accent(k.accent.slint()); p.set_on_accent(k.on_accent.slint()); p.set_error(k.error.slint());
}

fn rows(v: Vec<(String, String)>) -> ModelRc<Row> { ModelRc::new(VecModel::from(v.into_iter().map(|(t, s)| Row { title: t.into(), sub: s.into() }).collect::<Vec<_>>())) }

/// What the main window shows, from the app's state.
pub fn fill_main(w: &MainWindow, a: &App, log: &VecDeque<(String, String)>) {
    let st = a.public_state();
    w.set_version(version().into());
    let line = |l: &serde_json::Value| -> (String, f32) {
        if l.is_null() { return ("No data yet".into(), 0.0) }
        let now = hearth_relay::util::now_ms();
        let pct = crate::app::now_pct(l, st.g("usage").n("at").max(now)).unwrap_or(0.0);
        (format!("{}%{}", pct.floor(), if l.n("resetsAt") > now && pct > 0.0 { format!("  ·  resets in {}", crate::app::dur(l.n("resetsAt") - now)) } else { String::new() }), pct as f32)
    };
    let (t, p) = line(st.g("usage").g("session")); w.set_session_text(t.into()); w.set_session_pct(p);
    let (t, p) = line(st.g("usage").g("week")); w.set_week_text(t.into()); w.set_week_pct(p);
    w.set_status(st.s("error").into());
    w.set_ring_me(a.call_me());
    w.set_autostart(crate::sys::autostart_enabled());
    w.set_computers(rows(a.machines_public().iter().map(|m| (m.s("name"), format!("{}:{}{}", m.s("host"), m.n("port"), if m.t("secure") { "  ·  server" } else { "" }))).collect()));
    w.set_events(rows(log.iter().cloned().collect()));
}

/// The UI thread's side: windows and what they show.
pub struct Ui {
    pub app: Arc<App>,
    main: RefCell<Option<MainWindow>>,
    real: RefCell<Option<Rc<crate::wire::Wire>>>,
    popups: RefCell<HashMap<String, PopupWindow>>,
    log: RefCell<VecDeque<(String, String)>>,
    theme: RefCell<Option<ResolvedTheme>>,
    update: RefCell<serde_json::Value>,
    hidden_start: bool,
}
thread_local! { static UI: RefCell<Option<Rc<Ui>>> = const { RefCell::new(None) }; }

/// The sink to give [`App::new`]: runs every event on the UI thread.
pub fn sink() -> crate::app::Sink {
    Arc::new(|e: Event| { let _ = slint::invoke_from_event_loop(move || { if let Some(u) = UI.with(|u| u.borrow().clone()) { u.handle(e); } }); })
}

/// Creates the UI side (call on the UI thread, once the event loop can run). Shows the main window unless `hidden`.
pub fn start(app: Arc<App>, hidden: bool) -> Rc<Ui> {
    let ui = Rc::new(Ui { app, main: RefCell::new(None), real: RefCell::new(None), popups: RefCell::new(HashMap::new()), log: RefCell::new(VecDeque::new()), theme: RefCell::new(None), update: RefCell::new(serde_json::Value::Null), hidden_start: hidden });
    UI.with(|u| *u.borrow_mut() = Some(ui.clone()));
    ui.refresh_theme();
    if !hidden { ui.show_main(); }
    ui
}

impl Ui {
    fn create_main(self: &Rc<Self>) -> MainWindow {
        let w = MainWindow::new().expect("main window");
        let a = self.app.clone();
        w.on_refresh({ let a = a.clone(); move || { let a = a.clone(); a.clone().rt.spawn(async move { a.refresh().await; }); } });
        w.on_test_notify({ let a = a.clone(); move || a.notify_test() });
        w.on_toggle_ring({ let a = a.clone(); move |on| a.set_call_me(on) });
        w.on_toggle_autostart({ let a = a.clone(); move |on| { let _ = a.set_autostart(on); a.emit(Event::State); } });
        w.on_run_update({ let a = a.clone(); let me = Rc::downgrade(self); move || { if let Some(u) = me.upgrade() { let n = u.update.borrow().clone(); a.update_run(serde_json::json!({"desktop": n.t("desktop"), "server": n.t("server")})); } } });
        w.on_pick_file({ let a = a.clone(); move || { let a = a.clone(); a.clone().rt.spawn(async move { for f in a.pick_files().await { a.emit(Event::MoveProgress(drop_upload(&a, f).await)); } }); } });
        w.on_quit({ let a = a.clone(); move || a.quit() });
        w.set_drop_hint("Drop a file here to upload it to the projects folder of this computer.".into());
        // close = hide while the app has work to do in the background (tray); drop files = upload; focus = look for news
        w.window().on_close_requested({ let a = a.clone(); move || { if !a.quitting.load(std::sync::atomic::Ordering::SeqCst) && a.keep_running() { a.main_visible.store(false, std::sync::atomic::Ordering::SeqCst); a.main_focused.store(false, std::sync::atomic::Ordering::SeqCst); slint::CloseRequestResponse::HideWindow } else { a.quit(); slint::CloseRequestResponse::HideWindow } } });
        w.window().on_winit_window_event({ let a = a.clone(); move |_, ev| {
            use winit::event::WindowEvent as E;
            match ev {
                E::Focused(f) => { a.main_focused.store(*f, std::sync::atomic::Ordering::SeqCst); if *f { a.on_focus(); } }
                E::DroppedFile(p) => { let (a, p) = (a.clone(), p.clone()); a.clone().rt.spawn(async move { let t = drop_upload(&a, p).await; a.emit(Event::MoveProgress(t)); }); }
                _ => {}
            }
            EventResult::Propagate
        } });
        if let Some(t) = self.theme.borrow().as_ref() { set_pal(&w, &tokens_for(t)); }
        fill_main(&w, &self.app, &self.log.borrow());
        w
    }

    fn wiring() -> bool { std::env::var("HEARTH_UI").map(|v| v == "wiring").unwrap_or(false) }
    /// The screens of rs/ui, wired to the controller (see wire.rs).
    fn create_real(self: &Rc<Self>) -> Rc<crate::wire::Wire> {
        let win = hearth_ui::App::new().expect("ui window");
        let a = self.app.clone();
        win.window().on_close_requested({ let a = a.clone(); move || { if !a.quitting.load(std::sync::atomic::Ordering::SeqCst) && a.keep_running() { a.main_visible.store(false, std::sync::atomic::Ordering::SeqCst); a.main_focused.store(false, std::sync::atomic::Ordering::SeqCst); } else { a.quit(); } slint::CloseRequestResponse::HideWindow } });
        win.window().on_winit_window_event({ let a = a.clone(); move |_, ev| {
            use winit::event::WindowEvent as E;
            match ev {
                E::Focused(f) => { a.main_focused.store(*f, std::sync::atomic::Ordering::SeqCst); if *f { a.on_focus(); } }
                E::DroppedFile(p) => { let (a, p) = (a.clone(), p.clone()); a.clone().rt.spawn(async move { let t = drop_upload(&a, p).await; a.emit(Event::MoveProgress(t)); }); }
                _ => {}
            }
            EventResult::Propagate
        } });
        let wire = crate::wire::Wire::new(self.app.clone(), win);
        if let Some(t) = self.theme.borrow().as_ref() { wire.apply_theme(t); }
        wire
    }

    pub fn show_main(self: &Rc<Self>) {
        if !Self::wiring() {
            if self.real.borrow().is_none() { let w = self.create_real(); *self.real.borrow_mut() = Some(w); }
            { let r = self.real.borrow(); let w = &r.as_ref().unwrap().w; let _ = w.show(); w.window().with_winit_window(|x| { x.set_minimized(false); x.focus_window(); }); }
            self.app.main_visible.store(true, std::sync::atomic::Ordering::SeqCst);
            self.real.borrow().as_ref().unwrap().refresh();
            return;
        }
        let mut m = self.main.borrow_mut();
        if m.is_none() { *m = Some(self.create_main()); }
        let w = m.as_ref().unwrap();
        fill_main(w, &self.app, &self.log.borrow());
        let _ = w.show();
        w.window().with_winit_window(|x| { x.set_minimized(false); x.focus_window(); });
        self.app.main_visible.store(true, std::sync::atomic::Ordering::SeqCst);
        let _ = self.hidden_start;
    }
    fn redraw(&self) { if let Some(w) = self.main.borrow().as_ref() { if self.app.main_visible.load(std::sync::atomic::Ordering::SeqCst) { fill_main(w, &self.app, &self.log.borrow()); } } }
    fn push_log(&self, title: String, sub: String) { let mut l = self.log.borrow_mut(); l.push_front((title, sub)); l.truncate(50); }

    /// Reads the desktop's colors off the UI thread and paints every window with them when they changed.
    pub fn refresh_theme(self: &Rc<Self>) {
        let a = self.app.clone(); let settings = a.setting_all();
        a.rt.spawn(async move { let t = theme::resolve(&settings).await; let _ = slint::invoke_from_event_loop(move || { if let Some(u) = UI.with(|u| u.borrow().clone()) { u.apply_theme(t); } }); });
    }
    pub fn apply_theme(&self, t: ResolvedTheme) {
        if self.theme.borrow().as_ref() == Some(&t) { return; }
        let k = tokens_for(&t);
        if let Some(w) = self.main.borrow().as_ref() { set_pal(w, &k); }
        if let Some(w) = self.real.borrow().as_ref() { w.apply_theme(&t); }
        for p in self.popups.borrow().values() { set_pal(p, &k); }
        *self.theme.borrow_mut() = Some(t);
    }

    fn open_popup(self: &Rc<Self>, s: PopupSpec) {
        if self.popups.borrow().contains_key(&s.key) { return; }
        let w = PopupWindow::new().expect("popup");
        w.set_heading(s.title.clone().into()); w.set_body(s.text.clone().into()); w.set_kind(s.kind.clone().into()); w.set_permission(s.permission); w.set_own(s.own);
        if let Some(t) = self.theme.borrow().as_ref() { set_pal(&w, &tokens_for(t)); }
        w.window().set_size(slint::LogicalSize::new(380.0, s.height));
        let n = self.popups.borrow().len() as f32;
        w.window().set_position(slint::LogicalPosition::new(900.0, 40.0 + n * (s.height + 8.0)));   // ignored where the desktop places windows itself (Wayland)
        let (a, key) = (self.app.clone(), s.key.clone());
        w.on_action({ let a = a.clone(); let key = key.clone(); move |act| { let (a, key, act) = (a.clone(), key.clone(), act.to_string()); a.clone().rt.spawn(async move { if !a.call_action(&key, &act).await { a.close_popup(&key); } }); } });
        w.window().on_close_requested({ let a = a.clone(); let key = key.clone(); move || { a.popup_gone(&key); slint::CloseRequestResponse::HideWindow } });
        let _ = w.show();
        w.window().with_winit_window(|x| { if let Some(m) = x.primary_monitor() { let sf = m.scale_factor(); let size = m.size(); x.set_outer_position(winit::dpi::PhysicalPosition::new(size.width as f64 - (400.0 * sf), 40.0 * sf + n as f64 * (s.height as f64 + 8.0) * sf)); } });
        self.popups.borrow_mut().insert(s.key, w);
    }

    pub fn handle(self: &Rc<Self>, e: Event) {
        let wire = self.real.borrow().clone();
        if let Some(w) = &wire { if w.handle(&e) { return; } }
        match e {
            Event::State => self.redraw(),
            Event::Relay { ev, .. } => { if ev.s("kind") != "resolved" { let cwd = ev.s("cwd"); let proj = cwd.trim_end_matches('/').rsplit('/').next().filter(|s| !s.is_empty()).unwrap_or("Claude").to_string(); self.push_log(format!("{proj}: {}", ev.s("kind")), if ev.s("text").is_empty() { ev.s("title") } else { ev.s("text") }); self.redraw(); } }
            Event::MoveProgress(t) => { if let Some(w) = self.main.borrow().as_ref() { w.set_status(t.into()); } }
            Event::UpdateAvailable(n) => { if let Some(w) = self.main.borrow().as_ref() { w.set_update_text(format!("Version {} is ready.", n.s("latest")).into()); } *self.update.borrow_mut() = n; }
            Event::UpdateProgress(p) => { if let Some(w) = self.main.borrow().as_ref() { w.set_status(p.s("text").into()); } if p.s("step") == "end" && p.s("state") == "ok" { if let Some(w) = self.main.borrow().as_ref() { w.set_update_text(SharedString::new()); } } }
            Event::ShowMain => self.show_main(),
            Event::Popup(s) => self.open_popup(s),
            Event::ClosePopup(k) => { if let Some(w) = self.popups.borrow_mut().remove(&k) { let _ = w.hide(); } }
            Event::ThemeChanged => self.refresh_theme(),
            Event::RingMe(on) => { TRAY.with(|t| if let Some(t) = t.borrow().as_ref() { t.set_ring(on) }); self.redraw() }
            Event::Icon(png) => { TRAY.with(|t| if let Some(t) = t.borrow().as_ref() { t.set_icon(&png) });
                if let Some(icon) = decode_icon(&png) { if let Some(w) = self.main.borrow().as_ref() { w.window().with_winit_window(|x| x.set_window_icon(Some(icon))); } } }
            Event::Quit => { let _ = slint::quit_event_loop(); }
            Event::Relaunch => { RELAUNCH.store(true, std::sync::atomic::Ordering::SeqCst); let _ = slint::quit_event_loop(); }
            _ => {}
        }
    }
}

/// Uploads a dropped or picked file to the projects folder of this computer (what the wiring window offers; the chat screens upload into their chat).
async fn drop_upload(a: &Arc<App>, f: PathBuf) -> String {
    let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    match a.upload_path("local", "", &home().join("projects").to_string_lossy(), &f).await { Ok(p) => format!("Uploaded {name} to {}", p.as_str().unwrap_or("the projects folder")), Err(e) => format!("Could not upload {name}: {e}") }
}

/// PNG bytes as a window icon.
pub fn decode_icon(png_bytes: &[u8]) -> Option<winit::window::Icon> {
    let (rgba, w, h) = decode_png_rgba(png_bytes)?;
    winit::window::Icon::from_rgba(rgba, w, h).ok()
}
/// PNG bytes as straight RGBA8 pixels.
pub fn decode_png_rgba(png_bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(png_bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf.chunks(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
        _ => return None,
    };
    Some((rgba, info.width, info.height))
}

/// Set when the app should start itself again after the loop ends.
pub static RELAUNCH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
thread_local! { static TRAY: RefCell<Option<Arc<crate::tray::TrayHandle>>> = const { RefCell::new(None) }; }
/// The UI thread keeps the tray handle to update its check mark and icon.
pub fn attach_tray(t: Arc<crate::tray::TrayHandle>) { TRAY.with(|x| *x.borrow_mut() = Some(t)); }

/// Runs the Slint event loop until the app quits.
pub fn run() -> Result<(), slint::PlatformError> { slint::run_event_loop_until_quit() }
