#![no_std]
#![warn(missing_docs)]
//! # mikrotik-embassy
//!
//! Embassy async embedded client for the `MikroTik` `RouterOS` API.
//!
//! This crate provides an embedded-friendly async adapter built on top of the
//! sans-IO [`mikrotik_proto`] crate. It is **transport-agnostic** — it works
//! with any type implementing [`embedded_io_async::Read`] + [`embedded_io_async::Write`]:
//!
//! - **Plain TCP**: `embassy_net::tcp::TcpSocket`
//! - **TLS**: `embedded_tls::TlsConnection` (for `MikroTik` API-SSL on port 8729)
//! - **Any other**: UART, pipes, test mocks, etc.
//!
//! # Architecture
//!
//! Unlike the Tokio adapter which spawns a background actor task, the Embassy
//! adapter exposes a single [`run`] function that the user spawns as an
//! `#[embassy_executor::task]`. Communication happens through statically
//! allocated [`embassy_sync::channel::Channel`]s:
//!
//! ```text
//! ┌────────────────────────────────────────────────────────┐
//! │  User task                                             │
//! │                                                        │
//! │  CMD_CHANNEL ─────► run() ─────► EVT_CHANNEL           │
//! │  (Sender)           │  ▲         (Receiver)            │
//! │                     ▼  │                               │
//! │              &mut T: Read + Write                      │
//! │              (TcpSocket, TlsConnection, ...)           │
//! │                     │  ▲                               │
//! │              Connection (mikrotik-proto, sans-IO)       │
//! └────────────────────────────────────────────────────────┘
//! ```
//!
//! All events are delivered to a single output channel. The consumer filters
//! by [`Tag`](mikrotik_proto::tag::Tag) to correlate responses with commands.
//!
//! # Example — Plain TCP
//!
//! ```rust,ignore
//! use embassy_net::tcp::TcpSocket;
//! use embassy_sync::channel::Channel;
//! use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
//! use mikrotik_proto::command::{Command, CommandBuilder};
//! use mikrotik_proto::connection::Event;
//!
//! static CMD: Channel<CriticalSectionRawMutex, Command, 4> = Channel::new();
//! static EVT: Channel<CriticalSectionRawMutex, Event, 8> = Channel::new();
//!
//! #[embassy_executor::task]
//! async fn mikrotik_task(stack: embassy_net::Stack<'static>) {
//!     let mut rx_buf = [0; 4096];
//!     let mut tx_buf = [0; 4096];
//!     let mut socket = TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);
//!     socket.connect(endpoint).await.unwrap();
//!
//!     mikrotik_embassy::run(
//!         &mut socket, "admin", Some("password"),
//!         CMD.receiver(), EVT.sender(),
//!     ).await.unwrap();
//! }
//! ```
//!
//! # Example — TLS (with `embedded-tls`)
//!
//! ```rust,ignore
//! use embedded_tls::*;
//!
//! // Connect TCP, then wrap in TLS
//! let mut socket = TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);
//! socket.connect(endpoint).await.unwrap();
//!
//! let mut tls = TlsConnection::new(socket, &mut tls_read_buf, &mut tls_write_buf);
//! tls.open(TlsContext::new(
//!     &TlsConfig::new(),
//!     UnsecureProvider::new::<Aes128GcmSha256>(rng), // no cert verification
//! )).await.unwrap();
//!
//! // run() doesn't care what the transport is — just Read + Write
//! mikrotik_embassy::run(
//!     &mut tls, "admin", Some("password"),
//!     CMD.receiver(), EVT.sender(),
//! ).await.unwrap();
//! ```
//!
//! # Requirements
//!
//! This crate requires an allocator (`extern crate alloc`) because the
//! underlying [`mikrotik_proto`] crate uses `Vec`, `HashMap`, and `String`
//! for protocol processing.

extern crate alloc;

/// Error types for the Embassy adapter.
pub mod error;

mod device;
pub use device::run;

/// Re-export the protocol crate for convenience.
pub use mikrotik_proto as proto;
