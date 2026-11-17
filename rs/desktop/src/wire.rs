//! Wires rs/ui's root `App` window to the controller: every callback calls a function of [`App`], and what comes back
//! (events from the controller's tasks) is poured into the window's properties. UI thread only.
//! Async results always come back as [`Event`]s, so a test can fire callbacks and then feed the events itself.
use crate::app::{App, Event, PopupSpec};
use crate::chatview::{self, UiMsg};
use crate::color::Rgb;
use crate::screens::{self, Row, ScreenData};
use crate::theme::ResolvedTheme;
use crate::view;
use hearth_relay::util::{basename, rand_hex, J};
use hearth_ui::{Msg, Palette, SettingsState, Usage};
use serde_json::{json, Value};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::sync::Arc;

#[derive(Default)]
pub struct Chat { pub machine: String, pub id: String, pub title: String, pub cwd: String, pub base: Vec<UiMsg>, pub live: Vec<UiMsg>, pub working: bool, pub queued: Vec<String>, pub mode: String, pub model: String, pub effort: String, pub sid: Option<String>, pub tail: i32, pub last_text: String }

struct St { chat: Chat, data: ScreenData, vis_chats: Vec<Row>, vis_hist: Vec<Row>, project: String, chat_q: String, hist_q: String, update: Value, prompt: Option<String>, days: u32, defaults: Option<(PalSet, PalSet)> }

pub struct Wire { pub app: Arc<App>, pub w: hearth_ui::App, st: RefCell<St> }

#[derive(Clone, Copy)]
pub struct PalSet { pub bg: slint::Color, pub surface_low: slint::Color, pub card: slint::Color, pub card_high: slint::Color, pub card_highest: slint::Color, pub text: slint::Color, pub muted: slint::Color, pub faint: slint::Color, pub outline: slint::Color, pub outline_soft: slint::Color, pub accent: slint::Color, pub on_accent: slint::Color, pub accent_container: slint::Color, pub on_accent_container: slint::Color, pub selected: slint::Color, pub on_selected: slint::Color, pub error: slint::Color, pub ok: slint::Color, pub scrim: slint::Color, pub dock: slint::Color }
fn read(p: &Palette) -> PalSet { PalSet { bg: p.get_bg(), surface_low: p.get_surface_low(), card: p.get_card(), card_high: p.get_card_high(), card_highest: p.get_card_highest(), text: p.get_text(), muted: p.get_muted(), faint: p.get_faint(), outline: p.get_outline(), outline_soft: p.get_outline_soft(), accent: p.get_accent(), on_accent: p.get_on_accent(), accent_container: p.get_accent_container(), on_accent_container: p.get_on_accent_container(), selected: p.get_selected(), on_selected: p.get_on_selected(), error: p.get_error(), ok: p.get_ok(), scrim: p.get_scrim(), dock: p.get_dock() } }
fn write(p: &Palette, s: &PalSet) { p.set_bg(s.bg); p.set_surface_low(s.surface_low); p.set_card(s.card); p.set_card_high(s.card_high); p.set_card_highest(s.card_highest); p.set_text(s.text); p.set_muted(s.muted); p.set_faint(s.faint); p.set_outline(s.outline); p.set_outline_soft(s.outline_soft); p.set_accent(s.accent); p.set_on_accent(s.on_accent); p.set_accent_container(s.accent_container); p.set_on_accent_container(s.on_accent_container); p.set_selected(s.selected); p.set_on_selected(s.on_selected); p.set_error(s.error); p.set_ok(s.ok); p.set_scrim(s.scrim); p.set_dock(s.dock); }

fn rgb(c: slint::Color) -> Rgb { Rgb(c.red(), c.green(), c.blue()) }
/// The palette to paint: rs/ui's own for dark or light, with the desktop's surfaces (Oxide) and accent laid over it; steps the desktop does not give are mixed from the ones it does (like the old renderer).
pub fn palette_for(own: &PalSet, t: &ResolvedTheme) -> PalSet {
    use crate::color::mix;
    let mut s = *own;
    if let Some(k) = &t.surfaces {
        s.bg = k.background.slint(); s.card = k.card.slint(); s.card_high = k.raised.slint(); s.outline_soft = k.line.slint(); s.text = k.text.slint(); s.muted = k.muted.slint();
        s.surface_low = mix(k.background, k.card, 50.0).slint(); s.card_highest = mix(k.line, k.raised, 55.0).slint(); s.outline = mix(k.muted, k.line, 50.0).slint(); s.faint = mix(k.muted, k.background, 70.0).slint(); s.error = k.error.slint();
        s.dock = k.card.slint().with_alpha(0.95); s.accent = k.accent.slint(); s.on_accent = k.on_accent.slint();
    }
    let (bg, text, raised) = (rgb(s.bg), rgb(s.text), rgb(s.card_high));
    if let Some((a, on)) = t.accent {
        s.accent = a.slint(); s.on_accent = on.slint();
        s.accent_container = mix(a, bg, 34.0).slint(); s.on_accent_container = mix(a, text, 35.0).slint(); s.selected = mix(a, raised, 20.0).slint(); s.on_selected = mix(a, text, 25.0).slint();
    } else if t.surfaces.is_some() { let a = rgb(s.accent); s.accent_container = mix(a, bg, 34.0).slint(); s.on_accent_container = mix(a, text, 35.0).slint(); s.selected = mix(a, raised, 20.0).slint(); s.on_selected = mix(a, text, 25.0).slint(); }
    s
}

fn ui_msgs(v: &[UiMsg]) -> ModelRc<Msg> { ModelRc::new(VecModel::from(v.iter().map(|m| Msg { kind: m.kind.clone().into(), lead: m.lead.clone().into(), text: m.text.clone().into(), time: m.time.clone().into(), status: m.status.clone().into() }).collect::<Vec<_>>())) }

impl Wire {
    pub fn new(app: Arc<App>, w: hearth_ui::App) -> Rc<Wire> {
        let st = St { chat: Chat { tail: 80, mode: "acceptEdits".into(), ..Default::default() }, data: ScreenData::default(), vis_chats: vec![], vis_hist: vec![], project: String::new(), chat_q: String::new(), hist_q: String::new(), update: Value::Null, prompt: None, days: 30, defaults: None };
        let me = Rc::new(Wire { app, w, st: RefCell::new(st) });
        // rs/ui ships demo data: start empty
        me.w.set_chats(ModelRc::new(VecModel::from(Vec::<hearth_ui::ChatRow>::new()))); me.w.set_history(ModelRc::new(VecModel::from(Vec::<hearth_ui::ChatRow>::new()))); me.w.set_messages(ui_msgs(&[]));
        me.w.set_status("".into()); me.w.set_update(false); me.w.set_prompt(false); me.w.set_chat_title("".into()); me.w.set_chat_sub("".into()); me.w.set_chat_working(false);
        { let p = me.w.global::<Palette>(); let saved = p.get_dark(); p.set_dark(true); let d = read(&p); p.set_dark(false); let l = read(&p); p.set_dark(saved); me.st.borrow_mut().defaults = Some((d, l)); }
        me.bind();
        me.fill_settings(); me.fill_usage();
        if !me.app.setting("onboarded").as_bool().unwrap_or(false) { me.w.set_screen("onboarding".into()); }
        me
    }

    fn status(&self, t: impl Into<SharedString>) { self.w.set_status(t.into()); }
    fn spawn<F: std::future::Future<Output = ()> + Send + 'static>(&self, f: F) { self.app.rt.spawn(f); }

    // ───────────── properties from state ─────────────
    pub fn fill_settings(&self) {
        let s = view::settings_from(&self.app.setting_all());
        self.w.set_settings(SettingsState { theme: s.theme.into(), motion: s.motion, frosted: s.frosted, expressive: s.expressive, notify_finished: s.notify_finished, notify_usage: s.notify_usage, auto_update: s.auto_update, install_on_quit: s.install_on_quit, run_on_server: s.run_on_server, keep_copy: s.keep_copy, ask_edit: s.ask_edit, ask_cmd: s.ask_cmd });
        self.w.global::<hearth_ui::Motion>().set_scale(if s.motion { 1.0 } else { 0.0 });
    }
    pub fn fill_usage(&self) {
        let st = self.app.public_state(); let now = hearth_relay::util::now_ms();
        let v = view::usage_view(st.g("usage"), now);
        let hour: u32 = chrono::Local::now().format("%H").to_string().parse().unwrap_or(12);
        let at = |l: &Value| if l.n("resetsAt") > 0.0 { hearth_client::fmt::clock(l.n("resetsAt") as i64) } else { String::new() };
        let range = match self.st.borrow().days { 7 => "7d", 365 => "all", _ => "30d" };
        self.w.set_usage(Usage { greeting: (if hour < 12 { "Good morning" } else if hour < 18 { "Good afternoon" } else { "Good evening" }).into(), date_line: chrono::Local::now().format("%A, %B %-d").to_string().into(), updated: v.updated.into(),
            session_pct: v.session_pct, session_reset: v.session_reset.into(), session_at: at(st.g("usage").g("session")).into(), session_pace: v.session_pace.into(), session_note: "".into(),
            week_pct: v.week_pct, week_reset: v.week_reset.into(), week_at: at(st.g("usage").g("week")).into(), week_pace: v.week_pace.into(), week_note: "".into(),
            opus_pct: v.opus_pct, sonnet_pct: v.sonnet_pct, range: range.into(), scope: (if self.app.server().is_some() { "All computers" } else { "This computer only" }).into() });
        if !st.s("error").is_empty() { self.status(st.s("error")); }
    }
    pub fn apply_theme(&self, t: &ResolvedTheme) {
        let defs = self.st.borrow().defaults; let Some((d, l)) = defs else { return };
        let p = self.w.global::<Palette>(); p.set_dark(t.dark);
        write(&p, &palette_for(if t.dark { &d } else { &l }, t));
    }
    fn apply_lists(&self) {
        let (mut d, sel) = { let s = self.st.borrow(); (s.data.clone(), s.chat.id.clone()) };
        let (project, cq, hq) = { let s = self.st.borrow(); (s.project.clone(), s.chat_q.clone(), s.hist_q.clone()) };
        d.chats = view::filter_rows(&d.chats, &cq, &project); d.history = view::filter_rows(&d.history, &hq, "");
        { let mut s = self.st.borrow_mut(); s.vis_chats = d.chats.clone(); s.vis_hist = d.history.clone(); }
        screens::apply(&self.w, &d, &sel);
        self.w.set_filter(if project.is_empty() { "All chats".into() } else { project.into() });
    }
    /// Loads chats, history, tiles and computers in the background; they arrive as `Event::Screens`.
    pub fn refresh(&self) { let (a, days) = (self.app.clone(), self.st.borrow().days); self.spawn(async move { let d = screens::load(&a, days).await; a.emit(Event::Screens(Box::new(d))); }); }
    fn load_chat(&self) {
        let (m, id, tail) = { let s = self.st.borrow(); (s.chat.machine.clone(), s.chat.id.clone(), s.chat.tail) };
        if id.is_empty() { return; }
        let a = self.app.clone();
        self.spawn(async move { if let Ok(v) = a.relay(&m, "GET", &format!("/sessions/{}/messages?tail={tail}", hearth_relay::util::enc(&id)), None).await { let msgs = v.as_array().cloned().unwrap_or_default(); a.emit(Event::ChatLoaded { key: format!("{m}/{id}"), more: msgs.len() as i32 >= tail, msgs }); } });
    }
    fn push_chat(&self) {
        let s = self.st.borrow(); let c = &s.chat;
        self.w.set_messages(ui_msgs(&chatview::compose(&c.base, &c.live, c.working, &c.queued)));
        self.w.set_chat_working(c.working);
        let comp = self.app.machine(&c.machine).map(|m| m.s("name")).unwrap_or_default();
        self.w.set_chat_title(if c.title.is_empty() { "New chat".into() } else { c.title.clone().into() });
        self.w.set_chat_sub(format!("{}{}{}", basename(&c.cwd), if comp.is_empty() { String::new() } else { format!(" · {comp}") }, if c.working { " · working now" } else { "" }).into());
    }
    pub fn chat(&self) -> std::cell::Ref<'_, Chat> { std::cell::Ref::map(self.st.borrow(), |s| &s.chat) }
    pub fn rows(&self) -> (Vec<Row>, Vec<Row>) { let s = self.st.borrow(); (s.vis_chats.clone(), s.vis_hist.clone()) }

    fn open_row(&self, r: Row) {
        { let mut s = self.st.borrow_mut(); let c = &mut s.chat; if c.id != r.id || c.machine != r.machine { *c = Chat { machine: r.machine.clone(), id: r.id.clone(), title: r.title.clone(), cwd: r.cwd.clone(), tail: 80, mode: c.mode.clone(), model: c.model.clone(), effort: c.effort.clone(), ..Default::default() }; } }
        self.w.set_screen("chat".into()); self.push_chat(); self.load_chat(); self.apply_lists();
    }
    fn new_chat(&self, cwd: String) {
        { let mut s = self.st.borrow_mut(); let c = &s.chat; let (mode, model, effort) = (c.mode.clone(), c.model.clone(), c.effort.clone()); s.chat = Chat { machine: "local".into(), title: basename(&cwd), cwd, tail: 80, mode, model, effort, ..Default::default() }; }
        self.w.set_screen("chat".into()); self.push_chat();
    }

    // ───────────── sending ─────────────
    fn send(&self, text: String) {
        if text.trim().is_empty() { return; }
        if self.st.borrow().chat.working { self.st.borrow_mut().chat.queued.push(text); self.push_chat(); return; }
        if self.st.borrow().chat.machine.is_empty() { self.st.borrow_mut().chat.machine = "local".into(); }
        self.start_run(text);
    }
    fn start_run(&self, text: String) {
        let (m, id, cwd, mode, model, effort) = { let s = self.st.borrow(); let c = &s.chat; (c.machine.clone(), c.id.clone(), c.cwd.clone(), c.mode.clone(), c.model.clone(), c.effort.clone()) };
        let sid = rand_hex(4);
        let mut extra = json!({}); if id.is_empty() && !cwd.is_empty() { extra["cwd"] = json!(cwd); } if !model.is_empty() { extra["model"] = json!(model); } if !effort.is_empty() { extra["effort"] = json!(effort); }
        match self.app.send(&m, &id, json!(text), json!(mode), false, Some(sid.clone()), extra) {
            Ok(sid) => { { let mut s = self.st.borrow_mut(); let c = &mut s.chat; c.base.push(UiMsg::new("user", &text)); c.live.clear(); c.working = true; c.sid = Some(sid); c.last_text = text.clone(); if c.title.is_empty() || c.title == basename(&c.cwd) { c.title = text.chars().take(70).collect(); } } self.push_chat(); }
            Err(e) => { self.status(e); self.w.set_draft(text.into()); }
        }
    }
    fn finish_run(&self) {
        let next = { let mut s = self.st.borrow_mut(); let c = &mut s.chat; c.working = false; c.sid = None; let live = std::mem::take(&mut c.live); c.base.extend(live); if c.queued.is_empty() { None } else { Some(c.queued.remove(0)) } };
        self.push_chat(); self.load_chat(); self.refresh();
        if let Some(t) = next { self.start_run(t); }
    }
    fn stop(&self) { let (m, id) = { let c = &self.st.borrow().chat; (c.machine.clone(), c.id.clone()) }; if id.is_empty() { return; } let a = self.app.clone(); self.spawn(async move { let _ = a.stop(&m, &json!(id)).await; }); }
    /// Index of a queued follow-up in `messages` -> index in the queue.
    fn queue_index(&self, i: i32) -> Option<usize> { let s = self.st.borrow(); let c = &s.chat; let total = chatview::compose(&c.base, &c.live, c.working, &c.queued).len(); let first = total - c.queued.len(); (i as usize >= first && (i as usize) < total).then(|| i as usize - first) }

    // ───────────── controller events ─────────────
    /// Pours an event of the controller into the window. Returns true when it took a popup over (the in-app permission sheet).
    pub fn handle(&self, e: &Event) -> bool {
        match e {
            Event::Screens(d) => { self.st.borrow_mut().data = (**d).clone(); self.st.borrow_mut().data.status = String::new(); self.apply_lists(); self.fill_usage(); self.w.set_tiles(ModelRc::new(VecModel::from(d.tiles.iter().map(|(l, v, s)| hearth_ui::Tile { label: l.into(), value: v.into(), sub: s.into() }).collect::<Vec<_>>()))); self.st.borrow_mut().data.device_ids = d.device_ids.clone(); }
            Event::State => { self.fill_usage(); self.fill_settings(); self.refresh(); }
            Event::ChatLoaded { key, msgs, more } => {
                let mine = { let s = self.st.borrow(); format!("{}/{}", s.chat.machine, s.chat.id) == *key };
                if mine && !self.st.borrow().chat.working { let rows = chatview::from_relay(msgs, *more); self.st.borrow_mut().chat.base = rows; self.push_chat(); }
            }
            Event::NewChat { cwd } => self.new_chat(cwd.clone()),
            Event::Stream { sid, rec } => {
                if self.st.borrow().chat.sid.as_deref() != Some(sid.as_str()) { return false; }
                match rec.s("t").as_str() {
                    "session" => { let mut s = self.st.borrow_mut(); s.chat.id = rec.s("id"); }
                    "fail" => { let text = rec.s("text"); { let mut s = self.st.borrow_mut(); s.chat.live.push(UiMsg::new("text", &text)); } let last = self.st.borrow().chat.last_text.clone(); self.w.set_draft(last.into()); self.status(text); self.finish_run(); }
                    "end" => self.finish_run(),
                    _ => { { let mut s = self.st.borrow_mut(); let c = &mut s.chat; chatview::apply_record(&mut c.live, rec); } self.push_chat(); }
                }
            }
            Event::PairDone(err) => match err { None => { self.w.set_pair_error("".into()); self.w.set_pair_link("".into()); self.w.set_screen("settings".into()); self.refresh(); } Some(e) => self.w.set_pair_error(e.clone().into()) },
            Event::UpdateAvailable(n) => { self.st.borrow_mut().update = n.clone(); self.w.set_update_version(n.s("latest").into()); self.w.set_update_notes(n.s("notes").into()); self.w.set_update(true); }
            Event::UpdateProgress(p) => { if !p.s("text").is_empty() { self.status(p.s("text")); } if p.s("step") == "end" && p.s("state") == "ok" { self.w.set_update(false); } }
            Event::MoveProgress(t) => self.status(t.clone()),
            Event::Popup(s) => return self.take_popup(s),
            Event::ClosePopup(k) => { if self.st.borrow().prompt.as_deref() == Some(k.as_str()) { self.st.borrow_mut().prompt = None; self.w.set_prompt(false); } }
            Event::OpenSession { machine, session, title, cwd, .. } => { let r = Row { machine: machine.clone(), id: session.clone(), title: title.clone(), cwd: cwd.clone(), ..Default::default() }; self.open_row(r); }
            Event::Goto(t) => { if ["chats", "home", "history", "settings"].contains(&t.as_str()) { self.w.set_screen(t.as_str().into()); } else if t == "sessions" { self.w.set_screen("chats".into()); } }
            _ => {}
        }
        false
    }
    /// A permission question while the window is in front is shown as rs/ui's sheet instead of a separate window.
    fn take_popup(&self, s: &PopupSpec) -> bool {
        if !(s.permission && s.kind != "support" && s.kind != "request" && self.app.main_in_front()) { return false; }
        let (tool, target) = match s.text.split_once(": ") { Some((a, b)) => (a.to_string(), b.to_string()), None => (s.title.clone(), s.text.clone()) };
        self.w.set_prompt_tool(tool.into()); self.w.set_prompt_target(target.into()); self.w.set_prompt_detail(s.title.clone().into()); self.w.set_prompt(true);
        self.st.borrow_mut().prompt = Some(s.key.clone()); true
    }

    // ───────────── callbacks ─────────────
    fn bind(self: &Rc<Self>) {
        macro_rules! on { ($reg:ident, |$s:ident $(, $a:ident : $t:ty)*| $body:expr) => {{ let wk: Weak<Wire> = Rc::downgrade(self); self.w.$reg(move |$($a: $t),*| { if let Some($s) = wk.upgrade() { $body } }); }}; }
        let w = &self.w; let _ = w;
        on!(on_new_chat, |s| { let a = s.app.clone(); s.spawn(async move { if let Some(f) = a.pick_folder().await { a.emit(Event::NewChat { cwd: f }); } }); });
        on!(on_open_chat, |s, i: i32| { let r = s.st.borrow().vis_chats.get(i as usize).cloned(); if let Some(r) = r { s.open_row(r); } });
        on!(on_open_history, |s, i: i32| { let r = s.st.borrow().vis_hist.get(i as usize).cloned(); if let Some(r) = r { s.open_row(r); } });
        on!(on_send, |s, t: SharedString| s.send(t.to_string()));
        on!(on_stop, |s| s.stop());
        on!(on_composer_option, |s, o: SharedString| { let (cur, label) = { let mut st = s.st.borrow_mut(); let c = &mut st.chat; let f = match o.as_str() { "edits" => &mut c.mode, "model" => &mut c.model, _ => &mut c.effort }; *f = view::cycle(o.as_str(), f); (f.clone(), view::option_label(o.as_str(), f)) }; let _ = cur; s.status(format!("{}: {label}", match o.as_str() { "edits" => "Mode", "model" => "Model", _ => "Effort" })); });
        on!(on_send_now, |s, i: i32| { if let Some(q) = s.queue_index(i) { let t = s.st.borrow_mut().chat.queued.remove(q); s.st.borrow_mut().chat.queued.insert(0, t); if s.st.borrow().chat.working { s.stop(); } else { let t = s.st.borrow_mut().chat.queued.remove(0); s.start_run(t); } s.push_chat(); } });
        on!(on_remove_queued, |s, i: i32| { if let Some(q) = s.queue_index(i) { s.st.borrow_mut().chat.queued.remove(q); s.push_chat(); } });
        on!(on_show_earlier, |s| { s.st.borrow_mut().chat.tail *= 2; s.load_chat(); });
        on!(on_pin_chat, |s, i: i32| { if let Some(r) = s.st.borrow().vis_chats.get(i as usize).cloned() { s.tag(r.machine, r.id, json!({"pin": !r.pinned})); } });
        on!(on_delete_chat, |s, i: i32| { let r = s.st.borrow().vis_chats.get(i as usize).cloned(); if let Some(r) = r { let a = s.app.clone(); let gone = s.st.borrow().chat.id == r.id; if gone { s.st.borrow_mut().chat = Chat { tail: 80, ..Default::default() }; s.push_chat(); } s.spawn(async move { let _ = a.relay(&r.machine, "POST", &format!("/sessions/{}/delete", r.id), Some(json!({}))).await; a.emit(Event::State); a.emit(Event::MoveProgress("Chat deleted. It stays in the trash for 30 days.".into())); }); } });
        on!(on_rename_chat, |s, i: i32| { let name = s.w.get_chat_search().to_string(); let r = s.st.borrow().vis_chats.get(i as usize).cloned(); if let Some(r) = r { if name.trim().is_empty() { s.status("Type the new name in the search box, then double-click the chat."); } else { s.tag(r.machine, r.id, json!({"name": name.trim()})); s.w.set_chat_search("".into()); s.st.borrow_mut().chat_q.clear(); } } });
        on!(on_chat_search_edited, |s, q: SharedString| { s.st.borrow_mut().chat_q = q.to_string(); s.apply_lists(); });
        on!(on_history_search_edited, |s, q: SharedString| { s.st.borrow_mut().hist_q = q.to_string(); s.apply_lists(); });
        on!(on_filter_clicked, |s| { let next = { let st = s.st.borrow(); let mut ps: Vec<String> = vec![]; for r in &st.data.chats { if !ps.contains(&r.project) && !r.project.is_empty() { ps.push(r.project.clone()); } } if st.project.is_empty() { ps.first().cloned().unwrap_or_default() } else { ps.iter().position(|p| *p == st.project).and_then(|i| ps.get(i + 1)).cloned().unwrap_or_default() } }; s.st.borrow_mut().project = next; s.apply_lists(); });
        on!(on_list_menu, |s| s.refresh());
        on!(on_chat_menu, |s| { let (cwd, id) = { let c = &s.st.borrow().chat; (c.cwd.clone(), c.id.clone()) }; if let Err(e) = s.app.terminal_open(&cwd, &id) { s.status(e); } });
        on!(on_mic, |s| s.status("Voice calls need audio capture, which this build does not have yet."));
        on!(on_refresh_usage, |s| { let a = s.app.clone(); s.spawn(async move { a.refresh().await; }); });
        on!(on_usage_range_picked, |s, r: SharedString| { s.st.borrow_mut().days = match r.as_str() { "7d" => 7, "all" => 365, _ => 30 }; s.fill_usage(); s.refresh(); });
        on!(on_tab_changed, |s, _i: i32| s.refresh());
        on!(on_page_picked, |s, p: SharedString| { if p == "updates" { let a = s.app.clone(); s.spawn(async move { let d = a.update_check().await; let t = [d.g("desktop").s("text"), d.g("server").s("text"), d.s("unavailable")].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · "); a.emit(Event::MoveProgress(t)); }); } });
        on!(on_setting_toggled, |s, id: SharedString, on: bool| { if let Some(p) = view::setting_patch(id.as_str(), on) { s.app.settings_set(p); } });
        on!(on_theme_picked, |s, t: SharedString| { s.app.settings_set(json!({"theme": t.to_string()})); });
        on!(on_device_remove, |s, i: i32| { let id = s.st.borrow().data.device_ids.get(i as usize).cloned(); if let Some(id) = id { if let Some(share) = id.strip_prefix("g:") { let keep: Vec<Value> = s.app.s.lock().unwrap().g("guests").as_array().cloned().unwrap_or_default().into_iter().filter(|g| g.s("share") != share).collect(); s.app.guests_set(&json!(keep)); } else if id != "local" { s.app.remove_machine(&id); } s.refresh(); } });
        on!(on_device_add, |s| s.w.set_screen("pairing".into()));
        on!(on_device_refresh, |s| { let a = s.app.clone(); s.spawn(async move { let f = a.discover().await; a.emit(Event::MoveProgress(format!("Found {} new computer{} on this network.", f.len(), if f.len() == 1 { "" } else { "s" }))); a.emit(Event::State); }); });
        on!(on_routine_new, |s| s.status("Routines run on your server. Make them in the server's own screen once it is connected."));
        on!(on_update_install, |s| { let n = s.st.borrow().update.clone(); s.app.update_run(json!({"desktop": n.t("desktop"), "server": n.t("server")})); });
        on!(on_update_later, |s| { let a = s.app.clone(); s.w.set_update(false); s.spawn(async move { a.update_later().await; }); });
        on!(on_prompt_answered, |s, a: SharedString| { let key = s.st.borrow_mut().prompt.take(); if let Some(k) = key { let app = s.app.clone(); let act = if a == "deny" { "deny" } else { "allow" }; s.spawn(async move { app.call_action(&k, act).await; }); } });
        on!(on_pair_connect, |s, l: SharedString| { let l = l.trim().to_string(); let a = s.app.clone(); s.spawn(async move { let m = if l.starts_with("hearth://") || l.starts_with("http") { json!({"host": l, "token": ""}) } else { let mut p = l.split_whitespace(); json!({"host": p.next().unwrap_or(""), "token": p.next().unwrap_or("")}) }; let r = a.add_machine(m).await; a.emit(Event::PairDone(r.err())); }); });
        on!(on_pair_scan, |s| s.w.set_pair_error("Scanning a QR code is for phones. Paste the link or type the address and code.".into()));
        on!(on_onboarding_finish, |s| { s.app.settings_set(json!({"onboarded": true})); s.w.set_screen("chats".into()); });
        on!(on_onboarding_skip, |s| { s.app.settings_set(json!({"onboarded": true})); s.w.set_screen("chats".into()); });
        on!(on_call_hangup, |s| s.w.set_screen("chats".into()));
        on!(on_call_mute, |_s, _m: bool| {});
    }
    fn tag(&self, machine: String, id: String, patch: Value) {
        let a = self.app.clone(); let mut body = patch; body["id"] = json!(id);
        self.spawn(async move { let _ = a.relay(&machine, "POST", "/chat-tags", Some(body)).await; a.emit(Event::State); });
    }
}
