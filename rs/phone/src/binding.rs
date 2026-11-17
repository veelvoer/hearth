//! Binds `hearth_ui::App` (rs/ui) to the [`Controller`]: state -> properties ([`apply`]) and callbacks -> actions
//! ([`wire`]). Runs on the UI thread; the controller may change state from any thread, so [`attach`] hands the
//! newest state to the event loop. Everything here is host-testable with a headless Slint platform.

use crate::controller::{Controller, Screen, State};
use hearth_client::markdown::{parse, Block};
use hearth_ui::{App, ChatRow, Device, Msg, Palette, SettingsState, Tile, Usage};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn screen_name(s: Screen) -> &'static str {
    match s {
        Screen::Chats => "chats",
        Screen::Chat => "chat",
        Screen::Home => "home",
        Screen::History => "history",
        Screen::Settings => "settings",
        Screen::Onboarding => "onboarding",
        Screen::Pairing => "pairing",
        Screen::Call => "call",
    }
}

fn screen_of(s: &str) -> Option<Screen> {
    Some(match s {
        "chats" => Screen::Chats,
        "chat" => Screen::Chat,
        "home" => Screen::Home,
        "history" => Screen::History,
        "settings" => Screen::Settings,
        "onboarding" => Screen::Onboarding,
        "pairing" => Screen::Pairing,
        "call" => Screen::Call,
        _ => return None,
    })
}

fn rows(list: &[crate::controller::ChatVm], current: &str) -> Vec<ChatRow> {
    let any_pinned = list.iter().any(|c| c.pinned);
    list.iter()
        .enumerate()
        .map(|(i, c)| ChatRow {
            header: if i == 0 && c.pinned {
                "Pinned".into()
            } else if any_pinned && !c.pinned && (i == 0 || list[i - 1].pinned) {
                "Chats".into()
            } else {
                SharedString::new()
            },
            title: c.title.clone().into(),
            sub: if c.working { "working now".into() } else { c.sub.clone().into() },
            working: c.working,
            pinned: c.pinned,
            selected: c.id == current && !current.is_empty(),
            swiped: false,
        })
        .collect()
}

fn m(kind: &str, lead: &str, text: &str) -> Msg {
    Msg { kind: kind.into(), lead: lead.into(), text: text.into(), time: SharedString::new(), status: SharedString::new() }
}

/// The messages as the UI draws them: markdown blocks, tool cards, the queue and the working dots at the end.
pub fn messages(st: &State) -> Vec<Msg> {
    let mut out = vec![];
    for x in &st.messages {
        match x.kind.as_str() {
            "user" => out.push(m("user", "", &x.text)),
            "tool" => {
                let mut t = m("tool", &x.name, &x.text);
                t.status = if x.failed { "failed" } else if x.detail.is_empty() && st.working { "running" } else { "done" }.into();
                out.push(t);
            }
            "working" => {}
            _ => {
                for b in parse(&x.text) {
                    match b {
                        Block::Heading { text, .. } => out.push(m("heading", "", &text)),
                        Block::Paragraph { text } => out.push(m("text", "", &text)),
                        Block::Code { lang, text } => out.push(m("code", &lang.to_uppercase(), &text)),
                        Block::List { items, .. } => items.iter().for_each(|i| out.push(m("bullet", "", i))),
                        Block::Quote { text } => out.push(m("quote", "", &text)),
                        Block::Table { rows, .. } => rows.iter().for_each(|r| out.push(m("text", "", &r.join(" · ")))),
                        Block::Rule => {}
                    }
                }
            }
        }
    }
    for q in &st.queued {
        out.push(m("queued", "", q));
    }
    if st.working {
        out.push(m("working", "", ""));
    }
    out
}

/// What the last `apply` pushed into the App, so user-owned properties (screen, draft) are only overwritten when the
/// controller really changed them.
#[derive(Default)]
pub struct Applied {
    screen: Option<Screen>,
    draft: String,
    prompt: bool,
}

pub fn apply(app: &App, st: &State, last: &mut Applied) {
    if last.screen != Some(st.screen) {
        app.set_screen(screen_name(st.screen).into());
        last.screen = Some(st.screen);
    }
    if last.draft != st.draft {
        app.set_draft(st.draft.clone().into());
        last.draft = st.draft.clone();
    }
    let status = if !st.error.is_empty() {
        st.error.clone()
    } else if st.busy && st.status.is_empty() {
        "Loading...".into()
    } else if st.paired {
        format!("{} online", st.server)
    } else {
        st.status.clone()
    };
    app.set_status(status.into());
    app.set_pair_error(st.error.clone().into());
    app.set_chats(ModelRc::new(VecModel::from(rows(&st.chats, &st.current))));
    app.set_history(ModelRc::new(VecModel::from(rows(&st.history, &st.current))));
    app.set_messages(ModelRc::new(VecModel::from(messages(st))));
    app.set_chat_title(st.current_title.clone().into());
    app.set_chat_working(st.working);
    app.set_chat_sub(
        if st.working { "working now".to_string() } else if st.uploading { "uploading...".into() } else { st.attachments.iter().map(|a| a.name.clone()).collect::<Vec<_>>().join(", ") }.into(),
    );
    app.set_tiles(ModelRc::new(VecModel::from(st.tiles.iter().map(|t| Tile { label: t.label.clone().into(), value: t.value.clone().into(), sub: t.sub.clone().into() }).collect::<Vec<_>>())));
    app.set_devices(ModelRc::new(VecModel::from(st.devices.iter().map(|(n, b)| Device { name: n.clone().into(), sub: b.clone().into(), removable: true }).collect::<Vec<_>>())));
    if let Some(u) = &st.usage {
        let mut v = app.get_usage();
        let pct = |l: &Option<hearth_client::model::Limit>| l.as_ref().map(|l| l.pct as f32).unwrap_or(0.0);
        v.session_pct = pct(&u.session);
        v.week_pct = pct(&u.week);
        v.opus_pct = pct(&u.opus);
        v.sonnet_pct = pct(&u.sonnet);
        let reset = |l: &Option<hearth_client::model::Limit>| l.as_ref().and_then(|l| l.resets_at).map(|r| format!("Resets in {}", hearth_client::fmt::dur((r - u.at).max(0)))).unwrap_or_default();
        v.session_reset = reset(&u.session).into();
        v.week_reset = reset(&u.week).into();
        v.updated = "Usage updated just now".into();
        let _: &Usage = &v;
        app.set_usage(v);
    }
    let get = |k: &str, d: bool| st.settings.get(k).and_then(|v| v.as_bool()).unwrap_or(d);
    let mut s: SettingsState = app.get_settings();
    s.theme = st.settings.get("theme").and_then(|v| v.as_str()).unwrap_or("system").into();
    s.motion = get("motion", true);
    s.notify_finished = get("notifications", true);
    app.set_settings(s);
    match &st.prompt {
        Some(p) => {
            app.set_prompt_tool(p.tool.clone().into());
            app.set_prompt_target(p.text.clone().into());
            app.set_prompt_detail(p.text.clone().into());
            app.set_prompt(true);
            last.prompt = true;
        }
        None => {
            if last.prompt {
                app.set_prompt(false);
                last.prompt = false;
            }
        }
    }
    app.set_call_state(if st.call.active { if st.call.muted { "Muted" } else { "Listening" } } else { "" }.into());
    app.global::<hearth_ui::Motion>().set_scale(if get("motion", true) { 1.0 } else { 0.0 });
}

/// Dark or light: `system` follows the phone.
pub fn set_theme(app: &App, theme: &str, system_dark: bool) {
    app.global::<Palette>().set_dark(match theme {
        "dark" => true,
        "light" => false,
        _ => system_dark,
    });
}

/// Connect every callback of the App to the controller.
pub fn wire(app: &App, ctl: &Controller, system_dark: bool) {
    let c = ctl.clone();
    let chat_id = move |i: i32| c.state().chats.get(i as usize).map(|x| x.id.clone());
    let c = ctl.clone();
    let hist_id = move |i: i32| c.state().history.get(i as usize).map(|x| x.id.clone());
    {
        let c = ctl.clone();
        app.on_new_chat(move || {
            c.open_chat("");
            c.load_projects();
        });
    }
    {
        let (c, f) = (ctl.clone(), chat_id.clone());
        app.on_open_chat(move |i| {
            if let Some(id) = f(i) {
                c.open_chat(&id);
            }
        });
    }
    {
        let (c, f) = (ctl.clone(), hist_id);
        app.on_open_history(move |i| {
            if let Some(id) = f(i) {
                c.open_chat(&id);
            }
        });
    }
    {
        let c = ctl.clone();
        app.on_send(move |t| c.send(&t));
    }
    {
        let c = ctl.clone();
        app.on_stop(move || c.stop());
    }
    // queued messages are drawn after the real ones: index -> queue index
    {
        let (c, a) = (ctl.clone(), app.as_weak());
        app.on_send_now(move |i| {
            if let Some(q) = queue_index(&c, &a, i) {
                c.send_now(q);
            }
        });
    }
    {
        let (c, a) = (ctl.clone(), app.as_weak());
        app.on_remove_queued(move |i| {
            if let Some(q) = queue_index(&c, &a, i) {
                c.remove_queued(q);
            }
        });
    }
    {
        let (c, f) = (ctl.clone(), chat_id);
        app.on_pin_chat(move |i| {
            if let Some(id) = f(i) {
                let pinned = c.state().chats.iter().find(|x| x.id == id).map(|x| x.pinned).unwrap_or(false);
                c.set_pinned(&id, !pinned);
            }
        });
    }
    {
        let c = ctl.clone();
        app.on_chat_menu(move || c.share_last_answer());
    }
    {
        let c = ctl.clone();
        app.on_list_menu(move || c.refresh());
    }
    {
        let c = ctl.clone();
        app.on_history_search_edited(move |q| c.search(&q));
    }
    {
        let c = ctl.clone();
        app.on_refresh_usage(move || c.refresh());
    }
    {
        let c = ctl.clone();
        app.on_tab_changed(move |i| {
            if let Some(s) = [Screen::Chats, Screen::Home, Screen::History, Screen::Settings].get(i as usize) {
                c.go(*s);
            }
            if i == 0 || i == 1 {
                c.refresh();
            }
        });
    }
    {
        let c = ctl.clone();
        app.on_mic(move || c.start_call());
    }
    {
        let c = ctl.clone();
        app.on_call_hangup(move || c.hang_up());
    }
    {
        let c = ctl.clone();
        app.on_call_mute(move |m| c.set_muted(m));
    }
    {
        let c = ctl.clone();
        app.on_setting_toggled(move |id, on| {
            let key = match id.as_str() {
                "notify-finished" => "notifications",
                other => other,
            };
            c.set_setting(key, serde_json::json!(on));
        });
    }
    {
        let (c, a) = (ctl.clone(), app.as_weak());
        app.on_theme_picked(move |t| {
            c.set_setting("theme", serde_json::json!(t.as_str()));
            if let Some(a) = a.upgrade() {
                set_theme(&a, &t, system_dark);
            }
        });
    }
    {
        let c = ctl.clone();
        app.on_composer_option(move |o| {
            if o == "edits" {
                let cur = c.state().settings.get("mode").and_then(|v| v.as_str().map(String::from)).unwrap_or_else(|| "auto".into());
                c.set_setting("mode", serde_json::json!(if cur == "auto" { "acceptEdits" } else { "auto" }));
            }
        });
    }
    {
        let c = ctl.clone();
        app.on_device_remove(move |i| c.remove_device(i as usize));
    }
    {
        let c = ctl.clone();
        app.on_device_add(move || c.go(Screen::Pairing));
    }
    {
        let c = ctl.clone();
        app.on_device_refresh(move || c.refresh());
    }
    {
        // a hearth:// link, or "address 123456"
        let c = ctl.clone();
        app.on_pair_connect(move |l| {
            let l = l.trim().to_string();
            match l.rsplit_once(char::is_whitespace) {
                Some((addr, code)) if !l.starts_with("hearth://") => c.pair(addr.trim(), code.trim()),
                _ => c.pair(&l, ""),
            }
        });
    }
    {
        let c = ctl.clone();
        app.on_pair_scan(move || {
            // no scanner yet: take a link from the clipboard
            let _ = &c;
        });
    }
    {
        let c = ctl.clone();
        app.on_onboarding_finish(move || c.go(Screen::Pairing));
    }
    {
        let c = ctl.clone();
        app.on_onboarding_skip(move || c.go(Screen::Pairing));
    }
    {
        let c = ctl.clone();
        app.on_prompt_answered(move |a| c.answer_prompt(&a));
    }
    {
        let c = ctl.clone();
        app.on_page_picked(move |_| c.go(Screen::Settings));
    }
    let _ = screen_of;
}

fn queue_index(c: &Controller, a: &slint::Weak<App>, i: i32) -> Option<usize> {
    let st = c.state();
    let app = a.upgrade()?;
    let total = messages(&st).len() as i32 - if st.working { 1 } else { 0 };
    let first_q = total - st.queued.len() as i32;
    let _ = app;
    let q = i - first_q;
    (q >= 0 && (q as usize) < st.queued.len()).then_some(q as usize)
}

/// Keep the App in step with the controller: every change is applied on the UI thread, coalesced.
pub fn attach(app: &App, ctl: &Controller) {
    let weak = app.as_weak();
    let pending = Arc::new(AtomicBool::new(false));
    let last = Rc::new(RefCell::new(Applied::default()));
    let _ = &last;
    let c = ctl.clone();
    ctl.subscribe(move |_| {
        if pending.swap(true, Ordering::AcqRel) {
            return;
        }
        let (c, pending) = (c.clone(), pending.clone());
        let _ = weak.upgrade_in_event_loop(move |app| {
            pending.store(false, Ordering::Release);
            thread_local! { static LAST: RefCell<Applied> = RefCell::new(Applied::default()); }
            LAST.with(|l| apply(&app, &c.state(), &mut l.borrow_mut()));
        });
    });
}
