//! Hearth for Windows and Linux. A Slint window (no webview), the relay inside the app, a tray icon, notifications, popups.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use hearth_desktop::app::{data_dir, App, Event};
use hearth_desktop::{single, sys, tray, ui};
use std::sync::atomic::Ordering;

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    // the small MCP server that gives Claude the ask_user tool: Claude starts this program with "mcp-ask"
    if argv.get(1).map(|a| a == "mcp-ask").unwrap_or(false) { hearth_relay::mcpask::run(); return; }
    if argv.iter().any(|a| a == "--version") { println!("{}", env!("CARGO_PKG_VERSION")); return; }
    let hidden = argv.iter().any(|a| a == "--hidden");
    let dir = data_dir();

    // one instance: a second start shows the first one's window and ends
    let mut guard = match single::acquire(&dir) { single::Outcome::First(g) => g, single::Outcome::Other => return };

    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).thread_name("hearth-rt").enable_all().build().expect("async runtime");
    if let Err(e) = slint::BackendSelector::new().backend_name("winit".into()).renderer_name("software".into()).select() { eprintln!("Hearth could not start its window: {e}"); std::process::exit(1); }

    let app = App::new(dir, rt.handle().clone(), ui::sink());
    // this computer's relay, inside the app (unless the always-on relay service already runs here)
    rt.block_on(app.start_relay());
    { let a = app.clone(); guard.on_wake(move || a.emit(Event::ShowMain)); }
    let ui = ui::start(app.clone(), hidden);
    // the tray: needs a tray host on the desktop, the app works without one
    let tray_handle = if std::env::var("CM_NO_TRAY").is_err() { match rt.block_on(tray::start(app.clone())) { Ok(t) => Some(std::sync::Arc::new(t)), Err(e) => { eprintln!("no tray icon: {e}"); None } } } else { None };
    if let Some(t) = &tray_handle { ui::attach_tray(t.clone()); }
    { let _enter = rt.enter(); app.start_timers(); }
    if let Err(e) = ui::run() { eprintln!("Hearth: {e}"); }

    app.shutdown();
    drop(ui); drop(guard);
    if ui::RELAUNCH.load(Ordering::SeqCst) {
        let mut c = sys::cmd(&sys::launcher());
        if hidden { c.arg("--hidden"); }
        let _ = c.spawn();
    }
    // end the process: the relay's tasks and the event feeds die with it
    rt.shutdown_background();
}
