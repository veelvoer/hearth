//! Hearth client library: a pure Rust API client for the Hearth relay.
//!
//! This crate provides:
//! - **model**: Data structures for sessions, messages, usage, machines, etc.
//! - **api**: The main RelayClient for making HTTP requests to the relay
//! - **store**: Atomic JSON persistence for machines, settings, pins, names
//! - **stream**: Event streaming (SSE and ndjson) with resume support
//! - **markdown**: Markdown to blocks parser for UI rendering
//! - **fmt**: Formatting helpers (age, duration, pace)
//! - **pair**: Pairing link parsing
//! - **update**: GitHub release version checking
//!
//! # Example
//!
//! ```ignore
//! use hearth_client::model::Machine;
//! use hearth_client::api::RelayClient;
//!
//! let machine = Machine {
//!     name: "laptop".to_string(),
//!     host: "192.168.1.1".to_string(),
//!     port: 47601,
//!     token: "abc123".to_string(),
//!     secure: false,
//!     guest: false,
//!     label: String::new(),
//!     share: String::new(),
//! };
//!
//! let client = RelayClient::new(machine)?;
//! let sessions = client.sessions().await?;
//! ```

pub mod model;
pub mod api;
pub mod store;
pub mod stream;
pub mod markdown;
pub mod fmt;
pub mod pair;
pub mod update;

// Re-export commonly used types
pub use api::{ApiError, RelayClient};
pub use model::{
    Machine, Sess, Msg, Usage, Limit, WeekPlan, Profile, CallEvent, Dir, Listing,
    Skill, Computer, Project, pace, pace_word
};
pub use store::{Store, StoreError};
pub use markdown::Block;
pub use pair::parse_machine;
pub use fmt::{age, dur, clock};
pub use update::UpdateChecker;
pub use stream::{RunEvent, RunEvents, EventFeed};
