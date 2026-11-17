//! The controller's `Platform` on Android, through `hearth-android`. Failures are ignored on purpose:
//! a missing notification permission must never crash the app.

use crate::controller::{Picked, Platform};
use hearth_android::{clipboard, intents, notify, service, wear};

pub struct AndroidPlatform;

fn read(uri: String, cb: Box<dyn FnOnce(Option<Picked>) + Send>) {
    std::thread::spawn(move || {
        let name = intents::display_name(&uri).ok().flatten().unwrap_or_else(|| "file".into());
        cb(intents::read_uri(&uri).ok().map(|bytes| Picked { name, bytes }));
    });
}

impl Platform for AndroidPlatform {
    fn notify(&self, tag: i32, title: &str, text: &str) {
        let _ = notify::ensure_channel("hearth", "Hearth", notify::Importance::Default);
        let _ = notify::post("hearth", tag, title, text);
    }
    fn start_call(&self, title: &str, text: &str) {
        let _ = service::start_call(title, text);
    }
    fn stop_call(&self) {
        let _ = service::stop_call();
    }
    fn share_text(&self, title: &str, text: &str) {
        let _ = intents::share_text(title, text);
    }
    fn open_url(&self, url: &str) {
        let _ = intents::open_url(url);
    }
    fn pick_file(&self, cb: Box<dyn FnOnce(Option<Picked>) + Send>) {
        let cb = std::sync::Arc::new(std::sync::Mutex::new(Some(cb)));
        let c2 = cb.clone();
        let started = intents::pick_file(move |u| {
            if let Some(cb) = c2.lock().ok().and_then(|mut c| c.take()) {
                match u {
                    Some(u) => read(u, cb),
                    None => cb(None),
                }
            }
        });
        if started.is_err() {
            if let Some(cb) = cb.lock().ok().and_then(|mut c| c.take()) {
                cb(None);
            }
        }
    }
    fn take_photo(&self, cb: Box<dyn FnOnce(Option<Picked>) + Send>) {
        let cb = std::sync::Arc::new(std::sync::Mutex::new(Some(cb)));
        let c2 = cb.clone();
        let name = format!("hearth-{}.jpg", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0));
        let started = intents::take_photo(&name, move |u| {
            if let Some(cb) = c2.lock().ok().and_then(|mut c| c.take()) {
                match u {
                    Some(u) => read(u, cb),
                    None => cb(None),
                }
            }
        });
        if started.is_err() {
            if let Some(cb) = cb.lock().ok().and_then(|mut c| c.take()) {
                cb(None);
            }
        }
    }
    fn dictate(&self, prompt: &str, cb: Box<dyn FnOnce(Option<String>) + Send>) {
        let cb = std::sync::Arc::new(std::sync::Mutex::new(Some(cb)));
        let c2 = cb.clone();
        let started = intents::recognize_speech(prompt, move |t| {
            if let Some(cb) = c2.lock().ok().and_then(|mut c| c.take()) {
                cb(t);
            }
        });
        if started.is_err() {
            if let Some(cb) = cb.lock().ok().and_then(|mut c| c.take()) {
                cb(None);
            }
        }
    }
    fn copy(&self, text: &str) {
        let _ = clipboard::set_text(text);
    }
    fn push_machines(&self, json: &str) {
        let _ = wear::push_machines(json);
    }
}
