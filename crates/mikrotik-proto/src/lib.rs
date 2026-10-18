#![cfg_attr(not(feature = "std"), no_std)]
//! # mikrotik-proto
//!
//! Sans-IO implementation of the MikroTik RouterOS API: wire codec, command
//! builder, response parsing, connection multiplexing and the login handshake.
//!
//! This crate performs no I/O. It takes byte slices in and produces byte
//! buffers and events out; a runtime adapter such as `mikrotik-tokio` or
//! `mikrotik-embassy` does the networking.
//!
//! # Feature flags
//!
//! | Feature | Default | Description |
//! |---------|---------|-------------|
//! | `std`   | no      | `std::collections::HashMap` in public types instead of `hashbrown`'s, and a process-wide tag counter |

extern crate alloc;

#[cfg(not(feature = "std"))]
pub use hashbrown::HashMap;
#[cfg(feature = "std")]
pub use std::collections::HashMap;

/// Wire-format codec for MikroTik API length-prefixed words and sentences.
pub mod codec;
/// Command builder with typestate pattern and compile-time validation.
pub mod command;
/// Sans-IO connection state machine with multiplexed command/response correlation.
pub mod connection;
/// Error types for the protocol implementation.
pub mod error;
/// Typestate-enforced login handshake.
pub mod handshake;
/// Compile-time command path validation and `command!` macro.
pub mod macros;
/// Response types for parsed command responses.
pub mod response;
// Private: keeping it so is what guarantees no word can be written with a
// length that disagrees with its content.
mod sentence;
/// Command tags, correlating commands with their responses.
pub mod tag;
/// Word types: the fundamental unit of the MikroTik API protocol.
pub mod word;

pub use command::{Command, CommandBuilder};
pub use connection::{Connection, Event, State, Transmit};
pub use handshake::{Authenticated, Handshaking, LoginProgress};
pub use response::CommandResponse;
pub use tag::Tag;
