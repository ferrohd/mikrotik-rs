#![warn(missing_docs)]
//! # `MikroTik`-rs
//!
//! `mikrotik-rs` is a Rust library for interfacing with `MikroTik` routers via
//! the `RouterOS` API protocol. It allows sending commands and receiving responses
//! in parallel through channels.
//!
//! This crate re-exports types from:
//! - [`mikrotik_proto`](proto) — sans-IO protocol implementation (always available)
//! - [`mikrotik_tokio`](tokio_client) — Tokio-based async client (requires the `tokio` feature)
//!
//! ## Feature flags
//!
//! | Feature | Default | Description |
//! |---------|---------|-------------|
//! | `tokio` | **yes** | Enables the Tokio async adapter and [`MikrotikDevice`] client |
//!
//! To use only the protocol types without pulling in Tokio:
//!
//! ```toml
//! [dependencies]
//! mikrotik-rs = { version = "0.7", default-features = false }
//! ```
//!
//! ## Architecture
//!
//! The library is split into three crates:
//!
//! - **`mikrotik-proto`** — `#![no_std]`-compatible, runtime-agnostic protocol core.
//!   Handles wire-format encoding/decoding, command building, response parsing,
//!   and the connection state machine. Performs no I/O.
//!
//! - **`mikrotik-tokio`** — Thin async adapter that drives `mikrotik-proto` using
//!   Tokio's async runtime. Provides the high-level [`MikrotikDevice`] client.
//!
//! - **`mikrotik-rs`** (this crate) — Convenience re-exports from both crates.
//!
//! ## Examples
//!
//! ```rust,no_run
//! use mikrotik_rs::{MikrotikDevice, CommandBuilder};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let device = MikrotikDevice::connect("192.168.88.1:8728", "admin", Some("password")).await?;
//!
//! let cmd = CommandBuilder::new().command("/interface/print").build();
//! let mut rx = device.send_command(cmd).await?;
//!
//! while let Some(event) = rx.recv().await {
//!     println!("{event:?}");
//! }
//! # Ok(())
//! # }
//! ```

#[cfg(target_pointer_width = "16")]
compile_error!("This library supports 32-bit architectures or higher.");

// ── Protocol crate (always available) ──

/// Re-export of the sans-IO protocol crate.
pub use mikrotik_proto as proto;

// Re-export key protocol types at crate root
pub use mikrotik_proto::command::{Command, CommandBuilder, QueryOperator};
pub use mikrotik_proto::connection::{Connection, Event, State, Transmit};
pub use mikrotik_proto::error::{ConnectionError, LoginError, ProtocolError};
pub use mikrotik_proto::handshake::{Authenticated, Handshaking, LoginProgress};
pub use mikrotik_proto::response::{
    CommandResponse, DoneResponse, EmptyResponse, FatalResponse, ReplyResponse, TrapCategory,
    TrapResponse,
};
pub use mikrotik_proto::tag::Tag;

/// Re-export the `command!` macro from `mikrotik_proto`.
pub use mikrotik_proto::command;

// ── Tokio adapter (behind "tokio" feature) ──

/// Re-export of the Tokio async adapter crate.
///
/// Only available when the `tokio` feature is enabled (default).
#[cfg(feature = "tokio")]
pub use mikrotik_tokio as tokio_client;

#[cfg(feature = "tokio")]
pub use mikrotik_tokio::MikrotikDevice;
#[cfg(feature = "tokio")]
pub use mikrotik_tokio::error::{ActorError, DeviceError, DeviceResult};
