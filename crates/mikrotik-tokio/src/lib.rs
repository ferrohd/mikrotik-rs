//! # mikrotik-tokio
//!
//! Tokio adapter for the MikroTik RouterOS API, driving the sans-IO
//! [`mikrotik_proto`] state machine.
//!
//! # Features
//!
//! | Feature     | Description |
//! |-------------|-------------|
//! | `tokio-tls` | TLS via `tokio-rustls`; bring your own crypto provider |
//!
//! ```toml
//! mikrotik-tokio = { version = "0.10", features = ["tokio-tls"] }
//! rustls = { version = "0.23", features = ["ring"] }  # or "aws-lc-rs"
//! ```

extern crate alloc;

/// Typestate builder for constructing device connections.
pub mod builder;
/// Async device client for connecting to MikroTik routers.
mod device;
/// Tokio-specific error types.
pub mod error;

#[cfg(feature = "tokio-tls")]
mod tls;

pub use device::MikrotikDevice;
/// The sans-IO protocol crate.
pub use mikrotik_proto as proto;
