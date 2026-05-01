#![warn(missing_docs)]
//! # mikrotik-tokio
//!
//! Tokio-based async client for `MikroTik` `RouterOS` API.
//!
//! This crate provides a high-level async interface built on top of the
//! sans-IO [`mikrotik_proto`] crate. It drives the protocol state machine
//! using Tokio's async runtime.

/// Async device client for connecting to MikroTik routers.
mod device;
/// Tokio-specific error types.
pub mod error;

pub use device::MikrotikDevice;
/// Re-export the protocol crate for convenience.
pub use mikrotik_proto as proto;
