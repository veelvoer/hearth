//! Hearth server: installer and manager for a server (or a laptop) that runs the Rust `hearth-relay`.
//! Replaces server/hearth-server.js and relay/*.sh. The only Python left is the optional voice engine (see voice.rs).
pub mod cli;
pub mod detect;
pub mod hooks;
pub mod install;
pub mod local;
pub mod manage;
pub mod sysx;
pub mod texts;
pub mod ui;
pub mod update;
pub mod voice;

/// The version of this program, the same as the relay it installs (cargo xtask set-version keeps them equal).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DEFAULT_PORT: u16 = 47601;
pub const REPO: &str = "veelvoer/hearth";
