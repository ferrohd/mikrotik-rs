#![warn(missing_docs)]
//! # mikrotik-tokio
//!
//! Tokio-based async client for `MikroTik` `RouterOS` API.
//!
//! This crate provides a high-level async interface built on top of the
//! sans-IO [`mikrotik_proto`] crate. It drives the protocol state machine
//! using Tokio's async runtime.
//!
//! # Features
//!
//! | Feature     | Description |
//! |-------------|-------------|
//! | `tls`       | Enable TLS support via `tokio-rustls` (requires a crypto backend) |
//! | `ring`      | Use the `ring` crypto backend for TLS |
//! | `aws-lc-rs` | Use the `aws-lc-rs` crypto backend for TLS |
//!
//! To enable TLS, activate the `tls` feature **and** a crypto backend:
//!
//! ```toml
//! mikrotik-tokio = { version = "0.1", features = ["tls", "ring"] }
//! ```

extern crate alloc;

/// Typestate builder for constructing device connections.
pub mod builder;
/// Async device client for connecting to `MikroTik` routers.
mod device;
/// Tokio-specific error types.
pub mod error;

#[cfg(feature = "tls")]
mod tls;

pub use device::MikrotikDevice;
/// Re-export the protocol crate for convenience.
pub use mikrotik_proto as proto;
