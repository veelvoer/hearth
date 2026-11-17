//! Hearth for Wear OS (watch), Slint on android-activity. It asks the phone over the Wear data layer
//! (`/hearth/rpc`), falls back to the server over HTTP, and takes its servers from the phone (`/hearth/machines`).

pub mod binding;
pub mod core;

/// The watch does not answer RPCs; the symbol only has to exist for `HearthNative`.
pub fn no_rpc(_files_dir: &str, _request: &[u8]) -> Vec<u8> {
    br#"{"error":"unknown"}"#.to_vec()
}

#[cfg(target_os = "android")]
mod app;

#[cfg(target_os = "android")]
hearth_android::export_natives!(no_rpc);
