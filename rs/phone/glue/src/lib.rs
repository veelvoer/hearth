//! Android glue for Hearth, behind small modules with a plain Rust API (no JNI types leak out).
//!
//! Call [`init`] once from `android_main` (after the UI backend started: it fills `ndk_context`).
//! Every function that needs the activity returns `Err(Error::NoContext)` before that, so host builds
//! and unit tests never panic.
//!
//! Java that Android forces (see `java/` and the `hearth-pack` tool): `HearthActivity` (activity and
//! permission results), `CallService` (foreground service), `WatchLink` (Wear data layer listener),
//! `HearthNative` (the native method declarations). Everything else is done from Rust.

pub mod bridge;
pub mod clipboard;
pub mod haptics;
pub mod intents;
pub mod jvm;
pub mod natives;
pub mod notify;
pub mod results;
pub mod service;
pub mod wear;

pub use jvm::Error;
pub type Result<T> = std::result::Result<T, Error>;

/// Package that holds the generated Java classes.
pub const JAVA_PACKAGE: &str = "dev.voer.hearth.rust";

/// Marks the Android context as ready. Call from `android_main` after the UI backend is initialised.
pub fn init() {
    jvm::mark_ready();
}
