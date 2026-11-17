use crate::binding::{self, Dictate, Runner};
use crate::core::{Core, Transport};
use hearth_android as android;
use hearth_android::bridge;
use hearth_android::wear::{self, Reply, Request};
use hearth_ui::WatchApp;
use slint::ComponentHandle;
use std::path::PathBuf;
use std::sync::{mpsc, Arc};

/// Through the phone when one is in reach, else straight to the server.
struct Auto(String);
impl Transport for Auto {
    fn call(&self, q: Request) -> (Reply, bool) {
        match wear::ask(&q) {
            Ok(Some(r)) => (r, true),
            _ => (bridge::answer_blocking(&self.0, q), false),
        }
    }
}

#[no_mangle]
fn android_main(app: slint::android::AndroidApp) {
    let dir: PathBuf = app.internal_data_path().unwrap_or_else(|| PathBuf::from("/data/local/tmp"));
    slint::android::init(app).expect("slint android init");
    android::init();
    let dir_s = dir.display().to_string();
    let core = Arc::new(Core::new(&dir_s, Arc::new(Auto(dir_s.clone()))));
    let ui = WatchApp::new().expect("window");
    let weak = ui.as_weak();
    let c2 = core.clone();
    let done: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        let st = c2.state();
        let _ = weak.upgrade_in_event_loop(move |a| binding::apply(&a, &st));
    });
    let run: Runner = Arc::new(|f| {
        std::thread::spawn(f);
    });
    // voice reply: the system recogniser answers through the activity result; wait for it on this worker thread
    let dictate: Dictate = Arc::new(|| {
        let (tx, rx) = mpsc::channel();
        android::intents::recognize_speech("Reply to Claude", move |t| {
            let _ = tx.send(t);
        })
        .ok()?;
        rx.recv_timeout(std::time::Duration::from_secs(90)).ok().flatten()
    });
    binding::wire(&ui, core.clone(), run.clone(), dictate, done.clone());
    // servers: whatever the phone put in the data layer while the app was closed, then the first load
    {
        let (core, dir_s, done) = (core.clone(), dir_s.clone(), done.clone());
        std::thread::spawn(move || {
            if let Ok(Some(json)) = wear::fetch_machines() {
                bridge::save_machines(&dir_s, &json);
            }
            core.refresh();
            done();
        });
    }
    let _ = ui.run();
}
