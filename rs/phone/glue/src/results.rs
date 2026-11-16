//! Results that come back from Android (activity results, permission answers). The generated Java
//! `HearthActivity` forwards them to the native methods exported by [`crate::export_natives!`], which call
//! [`deliver`] and [`deliver_permission`].

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

pub const REQ_PICK: i32 = 7101;
pub const REQ_CAMERA: i32 = 7102;
pub const REQ_SPEECH: i32 = 7103;

type Cb = Box<dyn FnOnce(Option<String>) + Send>;
type PermCb = Box<dyn Fn(&str, bool) + Send>;

static PENDING: LazyLock<Mutex<HashMap<i32, Cb>>> = LazyLock::new(Default::default);
static PERM: LazyLock<Mutex<Option<PermCb>>> = LazyLock::new(Default::default);

/// Register what to do when request `req` returns (`None` = cancelled or failed). Replaces an earlier one.
pub fn expect(req: i32, cb: impl FnOnce(Option<String>) + Send + 'static) {
    if let Ok(mut m) = PENDING.lock() {
        m.insert(req, Box::new(cb));
    }
}

/// Called by the native method; `data` is the picked URI, the recognised sentence, or empty for a photo.
pub fn deliver(req: i32, ok: bool, data: Option<String>) {
    let cb = PENDING.lock().ok().and_then(|mut m| m.remove(&req));
    if let Some(cb) = cb {
        cb(if ok { Some(data.unwrap_or_default()) } else { None });
    }
}

/// Be told when the user answers a permission prompt.
pub fn on_permission(f: impl Fn(&str, bool) + Send + 'static) {
    if let Ok(mut p) = PERM.lock() {
        *p = Some(Box::new(f));
    }
}

pub fn deliver_permission(perm: &str, granted: bool) {
    if let Ok(p) = PERM.lock() {
        if let Some(f) = p.as_ref() {
            f(perm, granted);
        }
    }
}
