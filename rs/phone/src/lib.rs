//! Hearth for Android (phone), Slint on android-activity. The crate is empty on a desktop host.

/// Answers watch RPCs (also used by the generated Java `WatchLink`).
pub use hearth_android::bridge::watch_handler;

#[cfg(target_os = "android")]
mod app;

#[cfg(target_os = "android")]
hearth_android::export_natives!(watch_handler);
