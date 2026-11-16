//! Hearth for Wear OS (watch), Slint on android-activity. The crate is empty on a desktop host.
//! It asks the phone over the Wear data layer (`/hearth/rpc`) and falls back to the server over HTTP.

/// The watch does not answer RPCs; the symbol only has to exist for `HearthNative`.
pub fn no_rpc(_files_dir: &str, _request: &[u8]) -> Vec<u8> {
    br#"{"error":"unknown"}"#.to_vec()
}

#[cfg(target_os = "android")]
mod app;

#[cfg(target_os = "android")]
hearth_android::export_natives!(no_rpc);
