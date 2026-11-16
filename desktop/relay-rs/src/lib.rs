//! The Hearth relay as a library: the standalone binary (main.rs) and the desktop app both run it.
pub mod agent;
pub mod apikeys;
pub mod boot;
pub mod chathub;
pub mod core;
pub mod filehub;
pub mod hooks;
pub mod jobs;
pub mod jsu;
pub mod mcpkit;
pub mod perms;
pub mod post;
pub mod pricing;
pub mod relay;
pub mod routes;
pub mod safety;
pub mod selfupdate;
pub mod service;
pub mod shares;
pub mod tools;
pub mod util;
pub mod weekplan;
pub mod mcpask;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
