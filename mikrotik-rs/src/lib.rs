#![warn(missing_docs)]
//! # `MikroTik`-rs
//!
//! `mikrotik-rs` is an asynchronous Rust library for interfacing with `MikroTik` routers.
//! It allows for sending commands and receiving responses in parallel through channels.
//!
//! This crate is a convenience facade that re-exports types from:
//! - [`mikrotik_proto`] — sans-IO protocol implementation (codec, types, state machine)
//! - [`mikrotik_tokio`] — Tokio-based async client
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

// Re-export the protocol crate
pub use mikrotik_proto as proto;

// Re-export the tokio adapter
pub use mikrotik_tokio as tokio_client;

// Re-export key types at crate root for convenience
pub use mikrotik_proto::command::{Command, CommandBuilder, QueryOperator};
pub use mikrotik_proto::connection::{Connection, Event, State, Transmit};
pub use mikrotik_proto::error::{ConnectionError, LoginError, ProtocolError};
pub use mikrotik_proto::handshake::{Authenticated, Handshaking, LoginProgress};
pub use mikrotik_proto::response::{
    CommandResponse, DoneResponse, EmptyResponse, FatalResponse, ReplyResponse, TrapCategory,
    TrapResponse,
};
pub use mikrotik_proto::tag::Tag;
pub use mikrotik_tokio::MikrotikDevice;
pub use mikrotik_tokio::error::{ActorError, DeviceError, DeviceResult};

/// Re-export the `command!` macro from `mikrotik_proto`.
pub use mikrotik_proto::command;
