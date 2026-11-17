//! Android entry: Slint on android-activity showing `hearth_ui::App`, driven by the controller.

use crate::binding;
use crate::controller::{Controller, State};
use crate::platform_android::AndroidPlatform;
use hearth_android as android;
use hearth_ui::App;
use slint::ComponentHandle;
use std::path::PathBuf;
use std::sync::Arc;

#[no_mangle]
fn android_main(app: slint::android::AndroidApp) {
    let dir: PathBuf = app.internal_data_path().unwrap_or_else(|| PathBuf::from("/data/local/tmp"));
    slint::android::init(app).expect("slint android init");
    android::init();
    let rt: &'static tokio::runtime::Runtime = Box::leak(Box::new(tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().expect("runtime")));

    if !android::intents::has_permission("android.permission.POST_NOTIFICATIONS") {
        let _ = android::notify::request_permission();
    }
    let ctl = match Controller::new(&dir, Arc::new(AndroidPlatform), rt.handle().clone()) {
        Ok(c) => c,
        Err(_) => {
            // a damaged store must not keep the app from opening: start from a new one
            let _ = std::fs::rename(dir.join("hearth.json"), dir.join("hearth.json.bad"));
            Controller::new(&dir, Arc::new(AndroidPlatform), rt.handle().clone()).expect("store")
        }
    };
    let ui = App::new().expect("window");
    let theme = ctl.state().settings.get("theme").and_then(|v| v.as_str().map(String::from)).unwrap_or_else(|| "system".into());
    let dark = android::jvm::dark_mode();
    binding::set_theme(&ui, &theme, dark);
    binding::wire(&ui, &ctl, dark);
    binding::attach(&ui, &ctl);
    let st: State = ctl.state();
    binding::apply(&ui, &st, &mut binding::Applied::default());
    if st.paired {
        ctl.refresh();
        ctl.start_feed();
        ctl.load_projects();
        ctl.sync_watch();
    }
    let _ = ui.run();
}
