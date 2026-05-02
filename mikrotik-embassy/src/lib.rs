#![no_std]
#![warn(missing_docs)]
//! # mikrotik-embassy
//!
//! Embassy async embedded client for the `MikroTik` `RouterOS` API.
//!
//! This crate provides an embedded-friendly async adapter built on top of the
//! sans-IO [`mikrotik_proto`] crate, using [Embassy](https://embassy.dev/) for
//! async networking via [`embassy_net::tcp::TcpSocket`].
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
//! │              TcpSocket (embassy-net)                    │
//! │                     │  ▲                               │
//! │              Connection (mikrotik-proto, sans-IO)       │
//! └────────────────────────────────────────────────────────┘
//! ```
//!
//! All events are delivered to a single output channel. The consumer filters
//! by [`Tag`](mikrotik_proto::tag::Tag) to correlate responses with commands.
//!
//! # Example
//!
//! ```rust,ignore
//! use embassy_sync::channel::Channel;
//! use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
//! use embassy_net::tcp::TcpSocket;
//! use embassy_net::IpEndpoint;
//! use mikrotik_proto::command::{Command, CommandBuilder};
//! use mikrotik_proto::connection::Event;
//! use mikrotik_embassy::run;
//!
//! static CMD: Channel<CriticalSectionRawMutex, Command, 4> = Channel::new();
//! static EVT: Channel<CriticalSectionRawMutex, Event, 8> = Channel::new();
//!
//! #[embassy_executor::task]
//! async fn mikrotik_task(stack: embassy_net::Stack<'static>) {
//!     let mut rx_buf = [0; 4096];
//!     let mut tx_buf = [0; 4096];
//!     let socket = TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);
//!
//!     let endpoint = IpEndpoint::new(
//!         embassy_net::IpAddress::v4(192, 168, 88, 1),
//!         8728,
//!     );
//!
//!     run(socket, endpoint, "admin", Some("password"),
//!         CMD.receiver(), EVT.sender()).await.unwrap();
//! }
//!
//! #[embassy_executor::task]
//! async fn user_task() {
//!     let cmd = CommandBuilder::new()
//!         .command("/system/identity/print")
//!         .build();
//!     CMD.send(cmd).await;
//!
//!     loop {
//!         let event = EVT.receive().await;
//!         match event {
//!             Event::Done { .. } => break,
//!             _ => {}
//!         }
//!     }
//! }
//! ```
//!
//! # Requirements
//!
//! This crate requires an allocator (`extern crate alloc`) because the
//! underlying [`mikrotik_proto`] crate uses `Vec`, `HashMap`, and `String`
//! for protocol processing.

extern crate alloc;

/// Embassy-specific error types.
pub mod error;

mod device;
pub use device::run;

/// Re-export the protocol crate for convenience.
pub use mikrotik_proto as proto;
