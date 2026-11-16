use hearth_android as android;
use hearth_client::api::RelayClient;
use hearth_client::model::Machine;
use hearth_client::pair::parse_machine;
use hearth_client::store::Store;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel, Weak};
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};

slint::include_modules!();

static RT: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().expect("runtime"));

#[derive(Clone)]
struct Shared {
    store: Arc<Mutex<Store>>,
}

fn show(ui: &Weak<PhoneWindow>, f: impl FnOnce(&PhoneWindow) + Send + 'static) {
    let _ = ui.upgrade_in_event_loop(move |u| f(&u));
}

fn load_chats(sh: &Shared, ui: &Weak<PhoneWindow>) {
    let Some(m) = sh.store.lock().ok().and_then(|s| s.machines().into_iter().next()) else { return };
    show(ui, |u| u.set_working(true));
    let ui = ui.clone();
    RT.spawn(async move {
        let res = match RelayClient::new(m.clone()) {
            Ok(c) => c.sessions().await,
            Err(e) => Err(e),
        };
        let name = m.name.clone();
        match res {
            Ok(mut s) => {
                s.sort_by_key(|x| std::cmp::Reverse(x.mtime));
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
                let rows: Vec<(String, String, bool)> = s
                    .into_iter()
                    .take(100)
                    .map(|x| (if x.title.trim().is_empty() { "Chat".to_string() } else { x.title.clone() }, format!("{} ago", hearth_client::fmt::age(now - x.mtime)), x.running || x.busy))
                    .collect();
                let n = rows.len();
                show(&ui, move |u| {
                    let items: Vec<ChatItem> = rows.into_iter().map(|(t, s, b)| ChatItem { title: t.into(), sub: s.into(), busy: b }).collect();
                    u.set_chats(ModelRc::new(VecModel::from(items)));
                    u.set_paired(true);
                    u.set_server(name.into());
                    u.set_status(format!("{n} chats").into());
                    u.set_working(false);
                });
            }
            Err(e) => show(&ui, move |u| {
                u.set_paired(true);
                u.set_server(name.into());
                u.set_status(SharedString::from(e.to_string()));
                u.set_working(false);
            }),
        }
    });
}

fn pair(sh: &Shared, ui: &Weak<PhoneWindow>, input: String, code: String) {
    show(ui, |u| {
        u.set_working(true);
        u.set_status("Pairing...".into());
    });
    let (sh, ui) = (sh.clone(), ui.clone());
    RT.spawn(async move {
        let result: Result<Machine, String> = async {
            let m = parse_machine(&input, "", "").ok_or("That is not an address or link")?;
            let m = if input.trim().starts_with("hearth://") {
                parse_machine(&input, "", "").ok_or("Bad link")?
            } else {
                m
            };
            if !m.token.is_empty() {
                return Ok(m);
            }
            if code.trim().len() != 6 {
                return Err("Enter the 6-digit code shown on the computer".to_string());
            }
            RelayClient::new(m).map_err(|e| e.to_string())?.pair(code.trim()).await.map_err(|e| e.to_string())
        }
        .await;
        match result {
            Ok(m) => {
                let saved = sh.store.lock().map_err(|e| e.to_string()).and_then(|mut s| {
                    let mut all: Vec<Machine> = s.machines().into_iter().filter(|x| !(x.host == m.host && x.port == m.port)).collect();
                    all.insert(0, m);
                    s.set_machines(all).map_err(|e| e.to_string())
                });
                match saved {
                    Ok(()) => {
                        let wsh = sh.clone();
                        load_chats(&wsh, &ui);
                    }
                    Err(e) => show(&ui, move |u| {
                        u.set_status(e.into());
                        u.set_working(false);
                    }),
                }
            }
            Err(e) => show(&ui, move |u| {
                u.set_status(e.into());
                u.set_working(false);
            }),
        }
    });
}

#[no_mangle]
fn android_main(app: slint::android::AndroidApp) {
    let dir: PathBuf = app.internal_data_path().unwrap_or_else(|| PathBuf::from("/data/local/tmp"));
    slint::android::init(app).expect("slint android init");
    android::init();
    let _ = android::notify::ensure_channel("hearth", "Hearth", android::notify::Importance::Default);

    let store = Store::open(dir.join("hearth.json")).or_else(|_| Store::open(dir.join("hearth-new.json"))).expect("store");
    let sh = Shared { store: Arc::new(Mutex::new(store)) };
    let ui = PhoneWindow::new().expect("window");
    let w = ui.as_weak();
    {
        let (sh, w) = (sh.clone(), w.clone());
        ui.on_pair(move |a, c| pair(&sh, &w, a.to_string(), c.to_string()));
    }
    {
        let (sh, w) = (sh.clone(), w.clone());
        ui.on_refresh(move || load_chats(&sh, &w));
    }
    {
        let (sh, w) = (sh.clone(), w.clone());
        ui.on_forget(move || {
            if let Ok(mut s) = sh.store.lock() {
                let _ = s.set_machines(vec![]);
            }
            show(&w, |u| {
                u.set_paired(false);
                u.set_chats(ModelRc::new(VecModel::<ChatItem>::default()));
                u.set_status("".into());
            });
        });
    }
    if sh.store.lock().map(|s| !s.machines().is_empty()).unwrap_or(false) {
        load_chats(&sh, &w);
    }
    let _ = ui.run();
}
