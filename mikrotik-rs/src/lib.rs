#![warn(missing_docs)]
//! # MikroTik-rs
//!
//! `mikrotik-rs` is an asynchronous Rust library for interfacing with MikroTik routers.
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
//! use mikrotik_rs::MikrotikDevice;
//! use mikrotik_rs::proto::command::CommandBuilder;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let device = MikrotikDevice::connect("192.168.88.1:8728", "admin", Some("password")).await?;
//!
//! let cmd = CommandBuilder::new().command("/interface/print").build();
//! let mut rx = device.send_command(cmd).await?;
//!
//! while let Some(event) = rx.recv().await {
//!     println!("{:?}", event);
//! }
//! # Ok(())
//! # }
//! ```

#[cfg(target_pointer_width = "16")]
compiler_error!("This library supports 32-bit architectures or higher.");

// Re-export the protocol crate
pub use mikrotik_proto as proto;

// Re-export the tokio adapter
pub use mikrotik_tokio as tokio_client;

// Re-export key types at crate root for convenience
pub use mikrotik_proto::command::{Command, CommandBuilder};
pub use mikrotik_proto::connection::{Connection, Event, State, Transmit};
pub use mikrotik_proto::handshake::{Authenticated, Handshaking, LoginProgress};
pub use mikrotik_proto::response::CommandResponse;
pub use mikrotik_tokio::error::{DeviceError, DeviceResult};
pub use mikrotik_tokio::MikrotikDevice;

/// Compile-time command validation and the `command!` macro.
pub mod macros {
    pub use mikrotik_proto::macros::check_mikrotik_command;
}

/// Re-export the `command!` macro from `mikrotik_proto`.
pub use mikrotik_proto::command;
