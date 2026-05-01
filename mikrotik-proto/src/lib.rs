#![no_std]
#![warn(missing_docs)]
//! # mikrotik-proto
//!
//! Sans-IO protocol implementation for the MikroTik RouterOS API.
//!
//! This crate provides a runtime-agnostic, `no_std`-compatible implementation
//! of the MikroTik RouterOS API wire protocol. It handles:
//!
//! - **Wire-format encoding/decoding** — variable-length prefix codec for words and sentences
//! - **Command building** — typestate builder pattern with compile-time validation
//! - **Response parsing** — zero-copy sentence parsing into typed responses
//! - **Connection state machine** — multiplexed command/response correlation
//! - **Login handshake** — typestate-enforced authentication flow
//!
//! This crate performs **no I/O**. It accepts byte slices as input and produces
//! byte buffers and events as output. A runtime adapter (e.g., `mikrotik-tokio`)
//! is responsible for actual network communication.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────┐
//! │  mikrotik-proto (this crate)                        │
//! │                                                     │
//! │  codec ──▶ sentence ──▶ response                    │
//! │                           ▲                         │
//! │  command ─────────────────┘                          │
//! │                                                     │
//! │  connection (state machine, multiplexing)            │
//! │  handshake  (typestate login flow)                   │
//! └─────────────────────────────────────────────────────┘
//!          ▲ receive(&[u8])    │ poll_transmit()
//!          │                  │ poll_event()
//!          │                  ▼
//! ┌─────────────────────────────────────────────────────┐
//! │  Runtime adapter (e.g., mikrotik-tokio)             │
//! │  Thin async glue: TcpStream ↔ Connection            │
//! └─────────────────────────────────────────────────────┘
//! ```

extern crate alloc;
