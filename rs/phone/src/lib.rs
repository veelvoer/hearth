//! Hearth for Android (phone), Slint on android-activity.
//! `controller` is the view-model (plain Rust, tested on the host); the Slint app only exists on Android.

pub mod binding;
pub mod controller;

/// Answers watch RPCs (also used by the generated Java `WatchLink`).
pub use hearth_android::bridge::watch_handler;

#[cfg(target_os = "android")]
mod app;
#[cfg(target_os = "android")]
pub mod platform_android;

#[cfg(target_os = "android")]
hearth_android::export_natives!(watch_handler);
