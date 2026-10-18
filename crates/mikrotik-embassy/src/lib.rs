#![no_std]
//! # mikrotik-embassy
//!
//! Embassy adapter for the MikroTik RouterOS API, driving the sans-IO
//! [`mikrotik_proto`] state machine.
//!
//! Transport-agnostic: anything implementing [`embedded_io_async::Read`] +
//! [`embedded_io_async::Write`] will do — `embassy_net::tcp::TcpSocket`,
//! `embedded_tls::TlsConnection`, a UART, a test pipe.
//!
//! [`run`] is spawned as a task and talks to the rest of the program over two
//! static channels:
//!
//! ```text
//! CMD_CHANNEL ─────► run() ─────► EVT_CHANNEL
//!                     │  ▲
//!                     ▼  │
//!              &mut T: Read + Write
//! ```
//!
//! Every event lands on the one channel, so the consumer filters by
//! [`Tag`](mikrotik_proto::tag::Tag). The command channel is one-way, so a
//! command must carry its tag before it is sent — [`Client`] pairs the sender
//! with the counter that supplies them.
//!
//! ```rust,ignore
//! static CMD: Channel<CriticalSectionRawMutex, Command, 4> = Channel::new();
//! static EVT: Channel<CriticalSectionRawMutex, Event, 8> = Channel::new();
//!
//! #[embassy_executor::task]
//! async fn mikrotik_task(stack: embassy_net::Stack<'static>) {
//!     let mut socket = TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);
//!     socket.connect(endpoint).await.unwrap();
//!
//!     mikrotik_embassy::run(
//!         &mut socket, "admin", Some("password"),
//!         CMD.receiver(), EVT.sender(),
//!     ).await.unwrap();
//! }
//!
//! #[embassy_executor::task]
//! async fn poller() {
//!     let client = mikrotik_embassy::Client::new(CMD.sender());
//!     let cmd = client.build().command("/system/resource/print").build();
//!     let tag = cmd.tag;
//!     client.send(cmd).await;
//!
//!     loop {
//!         let event = EVT.receive().await;
//!         if event.tag() == Some(tag) { /* ... */ }
//!     }
//! }
//! ```
//!
//! See the crate README for the TLS variant.
//!
//! Requires an allocator: [`mikrotik_proto`] uses `Vec`, `String` and
//! `HashMap`.

extern crate alloc;

/// Error types for the Embassy adapter.
pub mod error;

mod client;
mod device;
pub use client::Client;
pub use device::run;

/// The sans-IO protocol crate.
pub use mikrotik_proto as proto;
