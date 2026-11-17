//! Hearth for Windows and Linux: the platform layer and the app logic. No webview: the window is Slint (see `ui.rs`).
pub mod app;
pub mod color;
pub mod ctl;
pub mod net;
pub mod screens;
pub mod single;
pub mod sys;
pub mod theme;
pub mod tray;
pub mod ui;
pub mod upd;

pub use app::{App, Event, PopupSpec};
