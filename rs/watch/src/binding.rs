//! Binds `hearth_ui::WatchApp` to the watch [`Core`] (properties and callbacks), host-testable.

use crate::core::{Core, WState};
use hearth_ui::{WatchApp, WatchChat};
use slint::{ModelRc, VecModel};
use std::sync::Arc;

/// Runs a closure off the UI thread (a thread on Android, inline in tests).
pub type Runner = Arc<dyn Fn(Box<dyn FnOnce() + Send>) + Send + Sync>;
/// Speech to text (the system recogniser), blocking.
pub type Dictate = Arc<dyn Fn() -> Option<String> + Send + Sync>;

pub fn apply(app: &WatchApp, st: &WState) {
    app.set_session_pct(st.session_pct);
    app.set_session_reset(if st.status.is_empty() { st.session_reset.clone() } else { st.status.clone() }.into());
    app.set_week_line(st.week_line.clone().into());
    app.set_chats(ModelRc::new(VecModel::from(
        st.chats.iter().map(|c| WatchChat { title: c.title.clone().into(), sub: c.sub.clone().into(), working: c.working }).collect::<Vec<_>>(),
    )));
}

/// `done` is called after every background step so the caller can apply the new state on the UI thread.
pub fn wire(app: &WatchApp, core: Arc<Core>, run: Runner, dictate: Dictate, done: Arc<dyn Fn() + Send + Sync>) {
    {
        let (core, run, done) = (core.clone(), run.clone(), done.clone());
        app.on_refresh(move || {
            let (core, done) = (core.clone(), done.clone());
            run(Box::new(move || {
                core.refresh();
                done();
            }));
        });
    }
    {
        let (core, run, done) = (core.clone(), run.clone(), done.clone());
        app.on_open_chats(move || {
            let (core, done) = (core.clone(), done.clone());
            run(Box::new(move || {
                core.refresh();
                done();
            }));
        });
    }
    {
        let (core, run, done) = (core, run, done);
        app.on_open_chat(move |i| {
            let (core, done, dictate) = (core.clone(), done.clone(), dictate.clone());
            run(Box::new(move || {
                core.open_chat(i as usize, &*dictate);
                core.refresh();
                done();
            }));
        });
    }
    app.on_back(|| {});
}
