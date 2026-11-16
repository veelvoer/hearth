use hearth_android as android;
use hearth_android::bridge;
use hearth_android::wear::{self, Reply, Request};
use hearth_client::model::Machine;
use hearth_client::pair::parse_machine;
use hearth_client::store::Store;
use slint::{ComponentHandle, ModelRc, VecModel, Weak};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

slint::include_modules!();

#[derive(Clone)]
struct Shared {
    dir: Arc<String>,
    store: Arc<Mutex<Store>>,
}

fn show(ui: &Weak<WatchWindow>, f: impl FnOnce(&WatchWindow) + Send + 'static) {
    let _ = ui.upgrade_in_event_loop(move |u| f(&u));
}

/// Through the phone when one is in reach, else straight to the server (same answer code either way).
fn call(sh: &Shared, m: &Machine, q: Request) -> (Reply, bool) {
    match wear::ask(&q) {
        Ok(Some(r)) => (r, true),
        _ => (bridge::answer_blocking(&sh.dir, q), false),
    }
}

fn first_machine(sh: &Shared) -> Option<Machine> {
    sh.store.lock().ok().and_then(|s| s.machines().into_iter().find(|m| !m.guest))
}

fn refresh(sh: &Shared, ui: &Weak<WatchWindow>) {
    let Some(m) = first_machine(sh) else {
        show(ui, |u| {
            u.set_has_server(false);
            u.set_status("No server yet. Copy a hearth:// link on the phone, then paste it here.".into());
        });
        return;
    };
    show(ui, |u| {
        u.set_has_server(true);
        u.set_status("Loading...".into());
    });
    let (sh, ui) = (sh.clone(), ui.clone());
    std::thread::spawn(move || {
        let base = m.base();
        let (chats, via) = call(&sh, &m, Request { op: "chats".into(), base: base.clone(), ..Default::default() });
        let (usage, _) = call(&sh, &m, Request { op: "usage".into(), base, ..Default::default() });
        let via_text = if via { "via phone" } else { "direct" };
        let usage_text = usage
            .usage
            .as_ref()
            .map(|u| {
                let p = |k: &str| u.get(k).and_then(|l| l.get("pct")).and_then(|v| v.as_f64()).map(|v| format!("{v:.0}%")).unwrap_or("-".into());
                format!("{} / {}", p("session"), p("week"))
            })
            .unwrap_or_default();
        let err = chats.error_text(&m.name);
        let rows: Vec<(String, String, bool)> = chats.chats.unwrap_or_default().into_iter().map(|c| (c.id, if c.title.is_empty() { "Chat".into() } else { c.title }, c.busy)).collect();
        show(&ui, move |u| {
            u.set_via(via_text.into());
            u.set_usage(usage_text.into());
            u.set_status(err.unwrap_or_default().into());
            u.set_chats(ModelRc::new(VecModel::from(rows.into_iter().map(|(id, t, b)| ChatItem { id: id.into(), title: t.into(), busy: b }).collect::<Vec<_>>())));
        });
    });
}

#[no_mangle]
fn android_main(app: slint::android::AndroidApp) {
    let dir: PathBuf = app.internal_data_path().unwrap_or_else(|| PathBuf::from("/data/local/tmp"));
    slint::android::init(app).expect("slint android init");
    android::init();
    let store = Store::open(dir.join("hearth.json")).or_else(|_| Store::open(dir.join("hearth-new.json"))).expect("store");
    let sh = Shared { dir: Arc::new(dir.display().to_string()), store: Arc::new(Mutex::new(store)) };
    let ui = WatchWindow::new().expect("window");
    let w = ui.as_weak();
    {
        let (sh, w) = (sh.clone(), w.clone());
        ui.on_refresh(move || refresh(&sh, &w));
    }
    {
        let w = w.clone();
        ui.on_close_detail(move || show(&w, |u| u.set_detail("".into())));
    }
    {
        let (sh, w) = (sh.clone(), w.clone());
        ui.on_paste(move || {
            let text = android::clipboard::get_text().ok().flatten().unwrap_or_default();
            match parse_machine(&text, "", "").filter(|m| !m.token.is_empty()) {
                Some(m) => {
                    if let Ok(mut s) = sh.store.lock() {
                        let _ = s.set_machines(vec![m]);
                    }
                    let _ = android::haptics::effect(android::haptics::Effect::Click);
                    refresh(&sh, &w);
                }
                None => show(&w, |u| u.set_status("The clipboard has no hearth:// link".into())),
            }
        });
    }
    {
        let (sh, w) = (sh.clone(), w.clone());
        ui.on_open_chat(move |id| {
            let Some(m) = first_machine(&sh) else { return };
            let (sh, w, id) = (sh.clone(), w.clone(), id.to_string());
            std::thread::spawn(move || {
                let (r, _) = call(&sh, &m, Request { op: "turn".into(), base: m.base(), id, ..Default::default() });
                let text = r.error_text(&m.name).unwrap_or_else(|| format!("{}\n\n{}", r.asked.unwrap_or_default(), r.answer.unwrap_or_default()));
                show(&w, move |u| u.set_detail(text.into()));
            });
        });
    }
    refresh(&sh, &w);
    let _ = ui.run();
}
