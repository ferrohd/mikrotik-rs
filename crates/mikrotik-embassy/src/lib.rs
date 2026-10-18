#![no_std]
//! # mikrotik-embassy
//!
//! Embassy async embedded client for the MikroTik RouterOS API.
//!
//! This crate provides an embedded-friendly async adapter built on top of the
//! sans-IO [`mikrotik_proto`] crate. It is **transport-agnostic**: it works
//! with any type implementing [`embedded_io_async::Read`] + [`embedded_io_async::Write`]:
//!
//! - **Plain TCP**: `embassy_net::tcp::TcpSocket`
//! - **TLS**: `embedded_tls::TlsConnection` (for MikroTik API-SSL on port 8729)
//! - **Any other**: UART, pipes, test mocks, etc. if you feel fancy.
//!
//! # Architecture
//!
//! The Embassy adapter exposes a single [`run`] function that the user spawns as an
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
//! Because the command channel is one-way, a command must carry its tag before
//! it is sent — there is no reply path to learn one afterwards. [`Client`]
//! pairs the channel's [`Sender`](embassy_sync::channel::Sender) with the
//! counter that supplies those tags, so the producer side reads the same way
//! the tokio adapter's device handle does.
//!
//! # Example — Plain TCP
//!
//! ```rust,ignore
//! use embassy_net::tcp::TcpSocket;
//! use embassy_sync::channel::Channel;
//! use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
//! use mikrotik_proto::command::Command;
//! use mikrotik_proto::connection::Event;
//!
//! static CMD: Channel<CriticalSectionRawMutex, Command, 4> = Channel::new();
//! static EVT: Channel<CriticalSectionRawMutex, Event, 8> = Channel::new();
//!
//! // The connection task: owns the socket, drives the protocol.
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
//!
//! // Any other task talks to it through a `Client`.
//! #[embassy_executor::task]
//! async fn poller() {
//!     let client = mikrotik_embassy::Client::new(CMD.sender());
//!
//!     let cmd = client.build().command("/system/resource/print").build();
//!     let tag = cmd.tag;
//!     client.send(cmd).await;
//!
//!     loop {
//!         let event = EVT.receive().await;
//!         if event.tag() == Some(tag) {
//!             // ...
//!         }
//!     }
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

mod client;
mod device;
pub use client::Client;
pub use device::run;

/// Re-export the protocol crate for convenience.
pub use mikrotik_proto as proto;
