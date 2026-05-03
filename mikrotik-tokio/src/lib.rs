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
//! | `tokio-tls` | Enable TLS support via `tokio-rustls` (bring your own crypto provider) |
//!
//! To enable TLS, activate the `tokio-tls` feature and add `rustls` with a
//! crypto backend to your dependencies:
//!
//! ```toml
//! mikrotik-tokio = { version = "0.1", features = ["tokio-tls"] }
//! rustls = { version = "0.23", features = ["ring"] }  # or "aws-lc-rs"
//! ```

extern crate alloc;

/// Typestate builder for constructing device connections.
pub mod builder;
/// Async device client for connecting to `MikroTik` routers.
mod device;
/// Tokio-specific error types.
pub mod error;

#[cfg(feature = "tokio-tls")]
mod tls;

pub use device::MikrotikDevice;
/// Re-export the protocol crate for convenience.
pub use mikrotik_proto as proto;
